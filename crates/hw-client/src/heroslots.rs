//! Hero slots, MMO style: twenty heroes live on this PC, each in its own
//! file (`heroes/hero-01.json` ... `hero-20.json` in the settings folder),
//! and the game keeps the one you play saved on its own, all the time. There
//! is no save button and no save game: the world is not kept, the hero is.
//!
//! How it is built:
//! - One place holds the truth in memory (`profile::Profile`'s store): the
//!   settings, all twenty heroes and which one is played. Reads never touch
//!   the disk after start, so a hero just changed can be picked again at
//!   once.
//! - Writing is one worker thread fed by a latest-wins queue ([`Writer`]):
//!   the game thread only hands over text (a few KB) and goes on, so saving
//!   never hitches. One queue means writes and deletes of a slot happen in
//!   the order they were asked, by construction.
//! - Every write is atomic: the text goes to `<file>.tmp`, is synced, the
//!   old file is kept as `<file>.bak`, and the new one is renamed over it. A
//!   crash or a power cut leaves the old or the new file, never half of one.
//!   A file that does not parse falls back to its `.bak`, and one that is
//!   broken either way is set aside as `.corrupt`, never overwritten.

use crate::missionview::{Filed, CABINET_KEEP};
use crate::profile::{sheet_from_json, sheet_to_json};
use crate::tray::Tray;
use actors_generator::CharacterConfig;
use hw_net::Sheet;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// How many heroes this PC keeps.
pub const SLOTS: usize = 20;

/// Hero file format.
const FILE_VERSION: u64 = 1;

/// Everything one hero owns: what makes them theirs on any world.
#[derive(Clone, Debug, PartialEq)]
pub struct HeroFile {
    pub name: String,
    /// Looks as made in the creator (None before the creator existed).
    pub look: Option<CharacterConfig>,
    /// Karma, powers, augments, standings, cases... None until the hero
    /// has picked an origin.
    pub hero: Option<Sheet>,
    /// Which key fires which power.
    pub tray: Tray,
    /// Closed cases, oldest first.
    pub cabinet: Vec<Filed>,
    /// Unix seconds: made, and last taken into a game (0: never).
    pub created: u64,
    pub last_played: u64,
}

impl HeroFile {
    pub fn to_json(&self) -> Value {
        json!({
            "version": FILE_VERSION,
            "name": self.name,
            "created": self.created,
            "last_played": self.last_played,
            "look": self.look.as_ref().and_then(|l| serde_json::to_value(l).ok()),
            "hero": self.hero.as_ref().map(sheet_to_json),
            "tray": self.tray.to_json(),
            "cabinet": self.cabinet.iter().map(Filed::to_json).collect::<Vec<_>>(),
        })
    }

    pub fn from_json(v: &Value) -> Option<Self> {
        v.as_object()?;
        let name = crate::profile::clean_name(v["name"].as_str()?);
        if name.is_empty() {
            return None;
        }
        Some(Self {
            name,
            look: serde_json::from_value::<CharacterConfig>(v["look"].clone()).ok(),
            hero: sheet_from_json(&v["hero"]),
            tray: Tray::from_json(&v["tray"]),
            cabinet: v["cabinet"].as_array().map_or_else(Vec::new, |a| {
                let keep = a.len().saturating_sub(CABINET_KEEP);
                a[keep..].iter().filter_map(Filed::from_json).collect()
            }),
            created: v["created"].as_u64().unwrap_or(0),
            last_played: v["last_played"].as_u64().unwrap_or(0),
        })
    }

    pub fn summary(&self) -> Summary {
        Summary {
            name: self.name.clone(),
            level: self
                .hero
                .as_ref()
                .map(|h| hw_sim::progress::level_of(h.lifetime)),
            origin: self.hero.as_ref().map(|h| h.origin),
            last_played: self.last_played,
        }
    }
}

/// What a slot card shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Summary {
    pub name: String,
    /// None while the hero has not yet picked an origin.
    pub level: Option<u8>,
    pub origin: Option<u8>,
    pub last_played: u64,
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// "never", "today", "yesterday", "5 days ago": when a hero was last played.
pub fn ago(now: u64, then: u64) -> String {
    if then == 0 {
        return "not played yet".into();
    }
    let days = now.saturating_sub(then) / 86_400;
    match days {
        0 => "played today".into(),
        1 => "played yesterday".into(),
        d if d < 60 => format!("played {d} days ago"),
        d => format!("played {} months ago", d / 30),
    }
}

/// `hero-07.json` for slot 6.
fn file_name(slot: usize) -> String {
    format!("hero-{:02}.json", slot + 1)
}

pub fn heroes_dir(base: &Path) -> PathBuf {
    base.join("heroes")
}

/// Writes `text` to `path` so that a crash leaves the old file or the new
/// one, whole. The old file stays beside it as `.bak`.
pub fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
    }
    if path.exists() {
        // A failing backup must not stop the save.
        let _ = std::fs::copy(path, path.with_extension("json.bak"));
    }
    std::fs::rename(&tmp, path)
}

/// Reads and parses a JSON file; a broken one gives None.
fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// A slot's hero from disk: the file, else its backup. A file broken both
/// ways is moved aside (`.corrupt`) so nothing overwrites it.
pub fn read_slot(base: &Path, slot: usize) -> Option<HeroFile> {
    let path = heroes_dir(base).join(file_name(slot));
    if !path.exists() {
        return None;
    }
    if let Some(f) = read_json(&path).and_then(|v| HeroFile::from_json(&v)) {
        return Some(f);
    }
    let bak = path.with_extension("json.bak");
    if let Some(f) = read_json(&bak).and_then(|v| HeroFile::from_json(&v)) {
        eprintln!("{} is broken: using its backup", path.display());
        return Some(f);
    }
    eprintln!("{} is broken: set aside as .corrupt", path.display());
    let _ = std::fs::rename(&path, path.with_extension("json.corrupt"));
    None
}

pub fn read_all(base: &Path) -> Vec<Option<HeroFile>> {
    (0..SLOTS).map(|s| read_slot(base, s)).collect()
}

/// What waits to be written. A later request for the same thing replaces an
/// earlier one, which is also what keeps a slot's write and delete in order.
#[derive(Default)]
struct Pending {
    settings: Option<String>,
    /// Some(text): write the slot; None: delete it.
    heroes: BTreeMap<usize, Option<String>>,
    busy: bool,
    closed: bool,
}

impl Pending {
    fn is_empty(&self) -> bool {
        self.settings.is_none() && self.heroes.is_empty()
    }
}

/// The saving thread and its queue (see the module docs).
pub struct Writer {
    shared: Arc<(Mutex<Pending>, Condvar)>,
}

impl Writer {
    /// A writer for the settings folder `base`: `profile.json` in it and the
    /// hero files under `heroes/`.
    pub fn new(base: PathBuf) -> Self {
        let shared = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let theirs = Arc::clone(&shared);
        let spawned = std::thread::Builder::new()
            .name("hero-saver".into())
            .spawn(move || {
                engine::parallel::background_thread();
                work(&theirs, &base);
            });
        if let Err(e) = spawned {
            eprintln!("Could not start the saving thread: {e}");
        }
        Self { shared }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Pending> {
        self.shared.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Queue the settings file's new text.
    pub fn settings(&self, text: String) {
        self.lock().settings = Some(text);
        self.shared.1.notify_all();
    }

    /// Queue a slot's new text, or None to delete the slot.
    pub fn hero(&self, slot: usize, text: Option<String>) {
        self.lock().heroes.insert(slot, text);
        self.shared.1.notify_all();
    }

    /// Waits until everything asked so far is on disk (or `limit` passes).
    /// Returns true when it is.
    pub fn flush(&self, limit: Duration) -> bool {
        let end = Instant::now() + limit;
        let mut p = self.lock();
        while !(p.is_empty() && !p.busy) {
            let Some(left) = end.checked_duration_since(Instant::now()) else {
                return false;
            };
            p = self
                .shared
                .1
                .wait_timeout(p, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        true
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.lock().closed = true;
        self.shared.1.notify_all();
    }
}

fn work(shared: &(Mutex<Pending>, Condvar), base: &Path) {
    let (lock, signal) = shared;
    loop {
        let job = {
            let mut p = lock.lock().unwrap_or_else(|e| e.into_inner());
            while p.is_empty() {
                if p.closed {
                    return;
                }
                p = signal.wait(p).unwrap_or_else(|e| e.into_inner());
            }
            p.busy = true;
            (p.settings.take(), std::mem::take(&mut p.heroes))
        };
        let (settings, heroes) = job;
        let mut failed = Pending::default();
        if let Some(text) = settings {
            if let Err(e) = write_atomic(&base.join("profile.json"), &text) {
                eprintln!("Could not save the settings: {e}");
                failed.settings = Some(text);
            }
        }
        for (slot, text) in heroes {
            let path = heroes_dir(base).join(file_name(slot));
            let done = match &text {
                Some(t) => write_atomic(&path, t),
                None => remove_slot_files(&path),
            };
            if let Err(e) = done {
                eprintln!("Could not save {}: {e}", path.display());
                failed.heroes.insert(slot, text);
            }
        }
        let retry = !failed.is_empty();
        {
            let mut p = lock.lock().unwrap_or_else(|e| e.into_inner());
            // What failed is asked again, unless something newer came.
            if p.settings.is_none() {
                p.settings = failed.settings;
            }
            for (slot, text) in failed.heroes {
                p.heroes.entry(slot).or_insert(text);
            }
            p.busy = false;
        }
        signal.notify_all();
        if retry {
            std::thread::sleep(Duration::from_secs(2));
        }
    }
}

fn remove_slot_files(path: &Path) -> std::io::Result<()> {
    for p in [
        path.to_path_buf(),
        path.with_extension("json.bak"),
        path.with_extension("json.tmp"),
    ] {
        match std::fs::remove_file(&p) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// The window is closing: if shutting down hangs (a thread that will not
/// stop), the hero is flushed to disk and the process ends after a few
/// seconds anyway, so it never lingers in the terminal.
pub fn exit_watchdog() {
    let _ = std::thread::Builder::new()
        .name("exit watchdog".into())
        .spawn(|| {
            std::thread::sleep(std::time::Duration::from_secs(5));
            crate::profile::Profile::flush();
            eprintln!("Shutting down took too long: ending the process");
            hard_exit(0);
        });
}

/// Ends the process now, for good, without running destructors, atexit
/// handlers or DLL detach code: whatever the sound, graphics or network
/// libraries would still wait for cannot keep the process (and the
/// terminal waiting for it) alive. Call it only once the hero is flushed.
pub fn hard_exit(code: i32) -> ! {
    #[cfg(windows)]
    {
        // ExitProcess (what `std::process::exit` calls) runs DLL detach code
        // under the loader lock, where a thread stuck in a driver can hold
        // it forever; TerminateProcess does not wait for anyone.
        #[link(name = "kernel32")]
        extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn TerminateProcess(process: *mut std::ffi::c_void, code: u32) -> i32;
        }
        // SAFETY: both are plain kernel32 calls on our own process handle.
        unsafe {
            TerminateProcess(GetCurrentProcess(), code as u32);
        }
    }
    std::process::exit(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("hw-heroslots-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn hero(name: &str, lifetime: u32) -> HeroFile {
        let mut sheet = hw_sim::progress::starter();
        sheet.lifetime = lifetime;
        HeroFile {
            name: name.into(),
            look: Some(CharacterConfig {
                hair: Some("ponytail01".into()),
                ..CharacterConfig::default()
            }),
            hero: Some(sheet),
            tray: Tray::default(),
            cabinet: Vec::new(),
            created: 1000,
            last_played: 2000,
        }
    }

    #[test]
    fn a_hero_file_round_trips() {
        let h = hero("Captain Comet", hw_sim::progress::karma_for_level(5));
        let back = HeroFile::from_json(&h.to_json()).unwrap();
        assert_eq!(back, h);
        assert_eq!(back.summary().level, Some(5));
        // A file without a name is not a hero.
        assert!(HeroFile::from_json(&json!({"name": "  "})).is_none());
        assert!(HeroFile::from_json(&json!(7)).is_none());
    }

    #[test]
    fn writes_are_atomic_and_keep_a_backup() {
        let d = temp("atomic");
        let path = heroes_dir(&d).join(file_name(0));
        write_atomic(&path, &hero("One", 0).to_json().to_string()).unwrap();
        write_atomic(&path, &hero("Two", 0).to_json().to_string()).unwrap();
        assert_eq!(read_slot(&d, 0).unwrap().name, "Two");
        // No half-written file is left behind.
        assert!(!path.with_extension("json.tmp").exists());
        // A torn main file (a crash while writing it) falls back to the backup.
        std::fs::write(&path, "{\"name\": \"Tw").unwrap();
        assert_eq!(read_slot(&d, 0).unwrap().name, "One");
        // Broken both ways: set aside, never overwritten.
        std::fs::write(path.with_extension("json.bak"), "nope").unwrap();
        assert!(read_slot(&d, 0).is_none());
        assert!(path.with_extension("json.corrupt").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_writer_keeps_order_and_the_latest() {
        let d = temp("writer");
        let w = Writer::new(d.clone());
        for lifetime in [10, 20, 30] {
            w.hero(3, Some(hero("Nova", lifetime).to_json().to_string()));
        }
        w.hero(5, Some(hero("Gone", 0).to_json().to_string()));
        w.hero(5, None);
        w.settings("{}".into());
        assert!(w.flush(Duration::from_secs(10)));
        assert_eq!(read_slot(&d, 3).unwrap().hero.unwrap().lifetime, 30);
        assert!(read_slot(&d, 5).is_none(), "delete after write wins");
        // A write after a delete brings the slot back.
        w.hero(5, None);
        w.hero(5, Some(hero("Back", 0).to_json().to_string()));
        assert!(w.flush(Duration::from_secs(10)));
        assert_eq!(read_slot(&d, 5).unwrap().name, "Back");
        assert_eq!(read_all(&d).iter().flatten().count(), 2);
        drop(w);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The whole life of heroes through the profile's store: the only test
    /// that touches it (it points the store at a folder of its own).
    #[test]
    fn heroes_live_in_slots_and_survive_a_restart() {
        use crate::profile::Profile;
        let d = temp("store");
        // SAFETY: no other test reads this variable or the store.
        unsafe { std::env::set_var("HW_PROFILE_DIR", &d) };
        let mut p = Profile::load();
        assert_eq!(p.slot, None);
        assert!(p.start_new(0, "Nova", None));
        p.save();
        assert!(!p.start_new(0, "Taken", None), "an occupied slot is kept");
        Profile::update(|p| p.hero = Some(hw_sim::progress::starter()));
        // A second hero in slot 5, then back to the first.
        let mut q = Profile::load();
        assert!(q.start_new(5, "Blaze", None));
        q.save();
        Profile::update(|p| {
            let mut h = hw_sim::progress::starter();
            h.lifetime = 5000;
            p.hero = Some(h);
        });
        let mut r = Profile::load();
        assert!(r.select(0));
        assert_eq!(
            (r.name.as_str(), r.hero.as_ref().map(|h| h.lifetime)),
            ("Nova", Some(0))
        );
        r.save();
        Profile::flush();
        // What is on disk is what a restart would read.
        let files = read_all(&d);
        assert_eq!(files[0].as_ref().unwrap().name, "Nova");
        assert_eq!(
            files[5].as_ref().unwrap().hero.as_ref().unwrap().lifetime,
            5000
        );
        assert_eq!(Profile::slots().iter().flatten().count(), 2);
        // Deleting the played hero leaves nobody picked.
        Profile::delete_slot(0);
        assert_eq!(Profile::load().slot, None);
        Profile::flush();
        assert!(read_slot(&d, 0).is_none());
        assert!(read_slot(&d, 5).is_some());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn last_played_reads_in_plain_words() {
        let day = 86_400;
        assert_eq!(ago(10 * day, 0), "not played yet");
        assert_eq!(ago(10 * day + 5, 10 * day), "played today");
        assert_eq!(ago(11 * day, 10 * day), "played yesterday");
        assert_eq!(ago(15 * day, 10 * day), "played 5 days ago");
    }
}
