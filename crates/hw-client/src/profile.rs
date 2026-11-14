//! What the game remembers on this PC between runs: the settings (world,
//! sound, picture, controls) in `profile.json`, and twenty hero slots (see
//! `heroslots`) of which one is played. The folder is `%APPDATA%\HeroWorld`
//! on Windows, `~/.config/heroworld` elsewhere. A missing or broken file is a
//! fresh start.
//!
//! A [`Profile`] is the settings plus the played hero's fields (name, looks,
//! hero file, tray, cabinet). The truth lives in one in-memory store behind
//! `Profile::load`/`update`/`save`; each change is queued to a saving thread
//! (`heroslots::Writer`) and never blocks the game.

use crate::heroslots::{self, HeroFile, Summary, Writer, SLOTS};
use crate::missionview::{Filed, CABINET_KEEP};
use crate::tray::Tray;
use actors_generator::CharacterConfig;
use hw_net::{OwnedPower, PowerId, Sheet};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub name: String,
    /// The world you last played (its seed), and about how many people
    /// live in its city (picked in the title's city window).
    pub seed: u64,
    pub people: f64,
    /// Multiplier on all karma earned (0.1 to 10), picked in the same
    /// window; the host's value counts for everyone in a shared world.
    pub karma_rate: f32,
    /// True when a hero is picked: the title then offers Continue.
    pub played: bool,
    /// The hero slot being played (0..20), None before one is picked. The
    /// fields below (`name` .. `cabinet`, `created`, `last_played`) are that
    /// hero's; the slot's file keeps them.
    pub slot: Option<usize>,
    /// Unix seconds the hero was made, and last taken into a game.
    pub created: u64,
    pub last_played: u64,
    /// Music and effect volumes, 0..1.
    pub music: f32,
    pub sounds: f32,
    /// Keep the title still (no swooping letters, no drifting camera).
    pub reduced_motion: bool,
    /// Picture: the quality preset's name (see `engine::quality::Quality`),
    /// holding 60 fps by trading far detail, vertical sync, field of view.
    pub quality: String,
    pub smooth: bool,
    pub vsync: bool,
    pub fov: f32,
    /// Share of the window's resolution the scene is drawn at (0.5..1);
    /// Engine's TAA upscales the rest. Below 1 helps slow graphics cards.
    pub render_scale: f32,
    /// Mouse: look speed (1 is the default) and inverted up and down.
    pub look_speed: f32,
    pub invert_y: bool,
    /// Always-on mouselook: the pointer is hidden and the mouse turns the
    /// camera without the right button (see `mouselook`).
    pub mouselook: bool,
    /// Team voice: on or off, the key, volumes and mutes.
    pub voice: crate::voice::VoiceProfile,
    /// The hero file: karma, affinities, origin and powers. None until an
    /// origin is chosen.
    pub hero: Option<Sheet>,
    /// The hero's looks, as made in the creator. None for a profile from
    /// before the creator (the game then dresses the hero itself, once).
    pub look: Option<CharacterConfig>,
    /// Which key fires which power (see `tray`).
    pub tray: Tray,
    /// Closed cases for the case file's cabinet, oldest first.
    pub cabinet: Vec<Filed>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: "Hero".into(),
            seed: 1,
            people: hw_sim::DEFAULT_POPULATION,
            karma_rate: hw_sim::karma_rate::DEFAULT,
            played: false,
            slot: None,
            created: 0,
            last_played: 0,
            music: 0.8,
            sounds: 0.9,
            reduced_motion: false,
            quality: "high".into(),
            smooth: true,
            vsync: true,
            fov: 55.0,
            render_scale: 1.0,
            look_speed: 1.0,
            invert_y: false,
            mouselook: true,
            voice: crate::voice::VoiceProfile::default(),
            hero: None,
            look: None,
            tray: Tray::default(),
            cabinet: Vec::new(),
        }
    }
}

impl Profile {
    fn path() -> Option<PathBuf> {
        if let Some(dir) = std::env::var_os("HW_PROFILE_DIR") {
            return Some(PathBuf::from(dir).join("profile.json"));
        }
        let base = if cfg!(windows) {
            PathBuf::from(std::env::var_os("APPDATA")?).join("HeroWorld")
        } else if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") {
            PathBuf::from(x).join("heroworld")
        } else {
            PathBuf::from(std::env::var_os("HOME")?)
                .join(".config")
                .join("heroworld")
        };
        Some(base.join("profile.json"))
    }

    /// The folder the profile lives in (crash reports go beside it).
    pub fn dir() -> Option<PathBuf> {
        Self::path().and_then(|p| p.parent().map(|d| d.to_path_buf()))
    }

    /// The profile as it is now (loaded from disk on the first call).
    pub fn load() -> Self {
        with_store(|st| st.profile.clone())
    }

    /// Changes the profile in place and queues it for saving.
    pub fn update(f: impl FnOnce(&mut Profile)) {
        with_store(|st| {
            f(&mut st.profile);
            st.persist();
        });
    }

    /// Makes this the profile and queues it for saving. Never waits for
    /// the disk.
    pub fn save(&self) {
        Self::update(|p| *p = self.clone());
    }

    /// Waits (a few seconds at most) until everything is on disk: before
    /// the game ends or restarts itself.
    pub fn flush() {
        if let Some(w) = WRITER.get() {
            if !w.flush(std::time::Duration::from_secs(5)) {
                eprintln!("Saving did not finish in time");
            }
        }
    }

    /// What the twenty slot cards show.
    pub fn slots() -> Vec<Option<Summary>> {
        with_store(|st| {
            st.files
                .iter()
                .map(|f| f.as_ref().map(HeroFile::summary))
                .collect()
        })
    }

    /// The first slot with no hero.
    pub fn free_slot() -> Option<usize> {
        with_store(|st| st.free_slot())
    }

    /// Picks `slot`'s hero as the one played (here; `save` keeps it).
    /// False when the slot is empty.
    pub fn select(&mut self, slot: usize) -> bool {
        let Some(f) = with_store(|st| st.files.get(slot).cloned().flatten()) else {
            return false;
        };
        self.take_hero(&f);
        self.slot = Some(slot);
        self.played = true;
        true
    }

    /// Makes a new hero in `slot` (an empty one: a hero already there is
    /// kept and this does nothing): no origin yet, nothing owned.
    pub fn start_new(&mut self, slot: usize, name: &str, look: Option<CharacterConfig>) -> bool {
        if slot >= SLOTS || with_store(|st| st.files[slot].is_some()) {
            return false;
        }
        self.clear_hero();
        self.name = clean_name(name);
        if self.name.is_empty() {
            self.name = "Hero".into();
        }
        self.look = look;
        self.created = heroslots::now_secs();
        self.slot = Some(slot);
        self.played = true;
        true
    }

    /// Makes sure a hero is picked before a game: the played one, else a new
    /// one in the first free slot (a runs straight into the game), else the
    /// most recently played.
    pub fn ensure_hero(&mut self) {
        if self.slot.is_some() {
            return;
        }
        if let Some(free) = Self::free_slot() {
            let name = self.name.clone();
            let look = self.look.take();
            self.start_new(free, &name, look);
        } else if let Some(last) = with_store(|st| {
            (0..SLOTS)
                .filter(|&s| st.files[s].is_some())
                .max_by_key(|&s| st.files[s].as_ref().map_or(0, |f| f.last_played))
        }) {
            self.select(last);
        }
    }

    /// Deletes the hero in `slot` for good. If it was the played one, nothing
    /// is picked afterwards. The caller takes the new state with `load`.
    pub fn delete_slot(slot: usize) {
        with_store(|st| {
            if slot >= SLOTS {
                return;
            }
            st.files[slot] = None;
            st.sent_files[slot] = None;
            writer().hero(slot, None);
            if st.profile.slot == Some(slot) {
                st.profile.clear_hero();
            }
            st.persist();
        });
    }

    fn from_json(v: &Value) -> Self {
        let d = Self::default();
        let f = |k: &str, d: f32| v[k].as_f64().map_or(d, |x| (x as f32).clamp(0.0, 1.0));
        Self {
            name: v["name"]
                .as_str()
                .map(clean_name)
                .filter(|s| !s.is_empty())
                .unwrap_or(d.name),
            seed: v["seed"].as_u64().unwrap_or(d.seed),
            people: v["people"]
                .as_f64()
                .filter(|p| p.is_finite())
                .map_or(d.people, |p| p.clamp(10.0, 1e8)),
            karma_rate: v["karma_rate"]
                .as_f64()
                .map_or(d.karma_rate, |x| hw_sim::karma_rate::clamp(x as f32)),
            played: v["played"].as_bool().unwrap_or(d.played),
            slot: v["slot"]
                .as_u64()
                .map(|s| s as usize)
                .filter(|&s| s < SLOTS),
            created: 0,
            last_played: 0,
            music: f("music", d.music),
            sounds: f("sounds", d.sounds),
            reduced_motion: v["reduced_motion"].as_bool().unwrap_or(d.reduced_motion),
            quality: v["quality"]
                .as_str()
                .filter(|q| engine::quality::Quality::from_name(q).is_some())
                .map_or(d.quality, |q| q.to_ascii_lowercase()),
            smooth: v["smooth"].as_bool().unwrap_or(d.smooth),
            vsync: v["vsync"].as_bool().unwrap_or(d.vsync),
            fov: v["fov"]
                .as_f64()
                .map_or(d.fov, |x| (x as f32).clamp(45.0, 90.0)),
            render_scale: v["render_scale"]
                .as_f64()
                .map_or(d.render_scale, |x| (x as f32).clamp(0.5, 1.0)),
            look_speed: v["look_speed"]
                .as_f64()
                .map_or(d.look_speed, |x| (x as f32).clamp(0.3, 3.0)),
            invert_y: v["invert_y"].as_bool().unwrap_or(d.invert_y),
            mouselook: v["mouselook"].as_bool().unwrap_or(d.mouselook),
            voice: crate::voice::VoiceProfile::from_json(&v["voice"]),
            hero: sheet_from_json(&v["hero"]),
            look: serde_json::from_value::<CharacterConfig>(v["look"].clone()).ok(),
            tray: Tray::from_json(&v["tray"]),
            cabinet: v["cabinet"].as_array().map_or_else(Vec::new, |a| {
                let keep = a.len().saturating_sub(CABINET_KEEP);
                a[keep..].iter().filter_map(Filed::from_json).collect()
            }),
        }
    }

    /// The settings file's content: everything but the hero, who lives in
    /// the slot's file.
    fn to_json(&self) -> Value {
        json!({
            "slot": self.slot,
            "seed": self.seed,
            "people": self.people,
            "karma_rate": self.karma_rate,
            "music": self.music,
            "sounds": self.sounds,
            "reduced_motion": self.reduced_motion,
            "quality": self.quality,
            "smooth": self.smooth,
            "vsync": self.vsync,
            "fov": self.fov,
            "render_scale": self.render_scale,
            "look_speed": self.look_speed,
            "invert_y": self.invert_y,
            "mouselook": self.mouselook,
            "voice": self.voice.to_json(),
        })
    }

    /// The played hero as the slot's file keeps it.
    fn hero_file(&self) -> HeroFile {
        HeroFile {
            name: self.name.clone(),
            look: self.look.clone(),
            hero: self.hero.clone(),
            tray: self.tray.clone(),
            cabinet: self.cabinet.clone(),
            created: self.created,
            last_played: self.last_played,
        }
    }

    /// Takes a hero's fields from their file.
    fn take_hero(&mut self, f: &HeroFile) {
        self.name = f.name.clone();
        self.look = f.look.clone();
        self.hero = f.hero.clone();
        self.tray = f.tray.clone();
        self.cabinet = f.cabinet.clone();
        self.created = f.created;
        self.last_played = f.last_played;
    }

    /// No hero picked: the fields of a fresh start.
    fn clear_hero(&mut self) {
        let d = Self::default();
        self.slot = None;
        self.played = false;
        self.name = d.name;
        self.look = None;
        self.hero = None;
        self.tray = d.tray;
        self.cabinet = Vec::new();
        self.created = 0;
        self.last_played = 0;
    }
}

/// The in-memory truth: the profile, every hero, and what was last handed to
/// the saving thread (so unchanged text is not written again).
struct Store {
    profile: Profile,
    files: Vec<Option<HeroFile>>,
    sent_settings: String,
    sent_files: Vec<Option<String>>,
}

static STORE: Mutex<Option<Store>> = Mutex::new(None);
static WRITER: OnceLock<Writer> = OnceLock::new();

fn writer() -> &'static Writer {
    WRITER.get_or_init(|| {
        Writer::new(Profile::dir().unwrap_or_else(|| std::env::temp_dir().join("heroworld")))
    })
}

fn with_store<R>(f: impl FnOnce(&mut Store) -> R) -> R {
    let mut g = STORE.lock().unwrap_or_else(|e| e.into_inner());
    let st = g.get_or_insert_with(Store::open);
    f(st)
}

impl Store {
    /// Reads the settings and all heroes from disk. A profile from before
    /// the slots (name, looks and hero in `profile.json`) becomes slot 1.
    fn open() -> Self {
        let base = Profile::dir();
        let legacy = base
            .as_ref()
            .and_then(|b| std::fs::read_to_string(b.join("profile.json")).ok())
            .and_then(|s| serde_json::from_str::<Value>(&s).ok());
        let mut profile = legacy.as_ref().map(Profile::from_json).unwrap_or_default();
        let mut files = base
            .as_ref()
            .map_or_else(|| vec![None; SLOTS], |b| heroslots::read_all(b));
        // What is on disk needs no writing again until it changes.
        let sent_files: Vec<Option<String>> = files
            .iter()
            .map(|f| f.as_ref().map(|f| to_text(&f.to_json())))
            .collect();
        let mut migrated = false;
        if files.iter().all(|f| f.is_none())
            && (profile.hero.is_some() || profile.look.is_some() || profile.played)
        {
            // The one hero of the old single profile moves into slot 1.
            profile.created = heroslots::now_secs();
            files[0] = Some(profile.hero_file());
            profile.slot = Some(0);
            migrated = true;
        }
        match profile.slot.and_then(|s| files[s].clone()) {
            Some(f) => {
                profile.take_hero(&f);
                profile.played = true;
            }
            None => profile.clear_hero(),
        }
        let mut st = Store {
            profile,
            files,
            sent_settings: String::new(),
            sent_files,
        };
        if migrated {
            st.persist();
        }
        st
    }

    fn free_slot(&self) -> Option<usize> {
        (0..SLOTS).find(|&s| self.files[s].is_none())
    }

    /// Queues what changed: the settings, and the played hero's file.
    fn persist(&mut self) {
        let p = &mut self.profile;
        // A hero with an origin always has a slot (a run that skipped the title).
        if p.slot.is_none() && p.hero.is_some() {
            p.slot = (0..SLOTS).find(|&s| self.files[s].is_none());
        }
        if let Some(slot) = p.slot {
            let mut f = p.hero_file();
            if f.created == 0 {
                f.created = heroslots::now_secs();
                p.created = f.created;
            }
            let text = to_text(&f.to_json());
            if self.sent_files[slot].as_deref() != Some(text.as_str()) {
                self.sent_files[slot] = Some(text.clone());
                writer().hero(slot, Some(text));
            }
            self.files[slot] = Some(f);
        }
        let text = to_text(&p.to_json());
        if text != self.sent_settings {
            self.sent_settings = text.clone();
            writer().settings(text);
        }
    }
}

fn to_text(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

/// Hero file format, so later fields don't break old heroes. Version 2
/// brought the power trees: older heroes get all their karma back once and
/// pick again, element too. Version 3 brought augments: older heroes have
/// none, with empty sockets.
const HERO_VERSION: u64 = 3;

pub(crate) fn sheet_to_json(s: &Sheet) -> Value {
    json!({
        "version": HERO_VERSION,
        "lifetime_karma": s.lifetime,
        "karma": s.karma,
        "element": s.element,
        "origin": s.origin,
        "regard": s.regard,
        "story": s.story,
        "briefed": s.briefed,
        "powers": s.powers.iter().map(|p| json!({
            "power": p.power.0,
            "paid": p.paid,
        })).collect::<Vec<_>>(),
        // Augments set into powers' sockets, and the bag of those found.
        "augments": s.augments.iter().map(|a| json!({
            "power": a.power.0,
            "slot": a.slot,
            "augment": augment_to_json(a.augment),
        })).collect::<Vec<_>>(),
        "bag": s.bag.iter().map(|a| augment_to_json(*a)).collect::<Vec<_>>(),
        // Where the hero's portals lead, in the world it was marked in
        // (a string: JSON numbers lose bits past 2^53).
        "portal_home": s.gate_home.map(|g| json!({
            "world": g.world.to_string(),
            "at": g.at,
            "yaw": g.yaw,
        })),
        // The team's cases (see `hw_sim::missions`), in the world they
        // were opened in.
        "cases": {
            "world": s.cases.world.to_string(),
            "opened": s.cases.opened,
            "closed": s.cases.closed,
            "open": s.cases.open.iter().map(|c| json!({
                "number": c.number,
                "gang": c.gang,
                "variant": c.variant,
                "step": c.step,
            })).collect::<Vec<_>>(),
        },
    })
}

fn augment_to_json(a: hw_net::Augment) -> Value {
    json!({ "effect": a.effect, "percent": a.percent, "grade": a.grade })
}

fn augment_from_json(v: &Value) -> Option<hw_net::Augment> {
    let byte = |k: &str| Some(v.get(k)?.as_u64()?.min(255) as u8);
    Some(hw_net::Augment {
        effect: byte("effect")?,
        percent: byte("percent")?,
        grade: byte("grade")?,
    })
}

fn cases_from_json(v: &Value) -> hw_net::CaseBook {
    let small = |x: &Value| x.as_u64().unwrap_or(0).min(u16::MAX as u64) as u16;
    let byte = |x: &Value| x.as_u64().unwrap_or(0).min(255) as u8;
    hw_net::CaseBook {
        world: v["world"]
            .as_str()
            .and_then(|w| w.parse().ok())
            .unwrap_or(0),
        opened: small(&v["opened"]),
        closed: small(&v["closed"]),
        open: v["open"]
            .as_array()
            .map(|list| {
                list.iter()
                    .take(hw_net::protocol::MAX_SAVED_CASES)
                    .map(|c| hw_net::SavedCase {
                        number: small(&c["number"]),
                        gang: byte(&c["gang"]),
                        // A case from before cases were planned (0.209)
                        // has none: never planned again.
                        variant: c["variant"]
                            .as_u64()
                            .map_or(hw_sim::missions::FORCED, |v| v.min(255) as u8),
                        step: byte(&c["step"]),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn gate_home_from_json(v: &Value) -> Option<hw_net::GateHome> {
    let at = v.get("at")?.as_array()?;
    let c = |i: usize| -> Option<i32> { i32::try_from(at.get(i)?.as_i64()?).ok() };
    Some(hw_net::GateHome {
        world: v.get("world")?.as_str()?.parse().ok()?,
        at: [c(0)?, c(1)?, c(2)?],
        yaw: u16::try_from(v.get("yaw")?.as_u64()?).ok()?,
    })
}

pub(crate) fn sheet_from_json(v: &Value) -> Option<Sheet> {
    let o = v.as_object()?;
    let num = |k: &str| {
        o.get(k)
            .and_then(|x| x.as_u64())
            .unwrap_or(0)
            .min(u32::MAX as u64) as u32
    };
    let mut regard = [0i8; hw_net::FACTIONS];
    if let Some(a) = o.get("regard").and_then(|a| a.as_array()) {
        for (slot, x) in regard.iter_mut().zip(a) {
            *slot = x.as_i64().unwrap_or(0).clamp(-128, 127) as i8;
        }
    }
    let powers = o
        .get("powers")
        .and_then(|p| p.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|p| {
                    Some(OwnedPower {
                        power: PowerId(p["power"].as_u64()?.min(255) as u8),
                        paid: p["paid"].as_u64().unwrap_or(0).min(u32::MAX as u64) as u32,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let mut sheet = Sheet {
        lifetime: num("lifetime_karma"),
        karma: num("karma"),
        element: num("element").min(255) as u8,
        origin: num("origin").min(255) as u8,
        powers,
        regard,
        story: num("story").min(255) as u8,
        briefed: o.get("briefed").and_then(|b| b.as_bool()).unwrap_or(false),
        gate_home: o.get("portal_home").and_then(gate_home_from_json),
        cases: cases_from_json(&v["cases"]),
        // Junk, and what no longer fits, is dropped by `sanitize`.
        augments: o
            .get("augments")
            .and_then(|a| a.as_array())
            .map(|list| {
                list.iter()
                    .filter_map(|a| {
                        Some(hw_net::SetAugment {
                            power: PowerId(a.get("power")?.as_u64()?.min(255) as u8),
                            slot: a.get("slot")?.as_u64()?.min(255) as u8,
                            augment: augment_from_json(a.get("augment")?)?,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        bag: o
            .get("bag")
            .and_then(|a| a.as_array())
            .map(|list| list.iter().filter_map(augment_from_json).collect())
            .unwrap_or_default(),
    };
    if num("version") < 2 {
        // Before the power trees: every power back, nothing picked.
        hw_sim::progress::refund_all(&mut sheet);
        sheet.element = 0;
    }
    Some(hw_sim::progress::sanitize(sheet))
}

/// A hero name as typed: trimmed, at most 24 characters, no control characters.
pub fn clean_name(s: &str) -> String {
    s.trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(24)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_survives_junk() {
        let p = Profile {
            name: "Captain Comet".into(),
            seed: 42,
            people: 1_000_000.0,
            karma_rate: 2.5,
            played: true,
            slot: Some(3),
            created: 1000,
            last_played: 2000,
            music: 0.25,
            sounds: 0.5,
            reduced_motion: true,
            quality: "low".into(),
            smooth: false,
            vsync: false,
            fov: 70.0,
            render_scale: 0.75,
            look_speed: 1.6,
            invert_y: true,
            mouselook: false,
            voice: crate::voice::VoiceProfile {
                enabled: false,
                key: "H".into(),
                volume: 0.5,
                muted: ["Mal".to_string()].into(),
                gains: [("Ada".to_string(), 1.5)].into(),
            },
            hero: Some(hw_sim::progress::starter()),
            look: Some(CharacterConfig {
                hair: Some("ponytail01".into()),
                outfit: vec!["future-suit".into()],
                ..CharacterConfig::default()
            }),
            tray: {
                let mut t = Tray::default();
                t.sync(&hw_sim::progress::starter());
                t.put(PowerId::STRIKE, 14);
                t
            },
            cabinet: vec![Filed {
                number: 3,
                kind: hw_sim::missions::CaseKind::Hostages,
                gang: 1,
                building: "Daily Comet building".into(),
            }],
        };
        // The settings file holds the settings; the hero lives in a slot file.
        let settings_only = Profile {
            name: "Hero".into(),
            played: false,
            hero: None,
            look: None,
            tray: Tray::default(),
            cabinet: Vec::new(),
            created: 0,
            last_played: 0,
            ..p.clone()
        };
        assert_eq!(Profile::from_json(&p.to_json()), settings_only);
        let mut slotted = Profile::default();
        slotted.take_hero(&p.hero_file());
        assert_eq!(slotted.hero_file(), p.hero_file());
        assert_eq!(
            HeroFile::from_json(&p.hero_file().to_json()),
            Some(p.hero_file())
        );
        let junk = json!({"name": 7, "music": 9.0, "seed": "x", "people": "many", "quality": "max", "fov": 500, "render_scale": 0.1});
        let q = Profile::from_json(&junk);
        assert_eq!(q.name, "Hero");
        assert_eq!(q.music, 1.0);
        assert_eq!(q.seed, 1);
        assert_eq!(q.people, hw_sim::DEFAULT_POPULATION);
        assert_eq!(q.karma_rate, 1.0, "no setting: the normal rate");
        assert_eq!(
            Profile::from_json(&json!({"karma_rate": 99.0})).karma_rate,
            10.0
        );
        assert_eq!(q.quality, "high");
        assert_eq!(q.fov, 90.0);
        assert_eq!(q.render_scale, 0.5);
        assert_eq!(q.hero, None);
        assert_eq!(q.look, None);
        assert!(q.cabinet.is_empty());
        // A hero file with a power from a later version keeps the rest.
        let later = json!({"hero": {"version": 2, "lifetime_karma": 99, "karma": 5,
            "origin": 2, "element": 3,
            "powers": [{"power": 5, "paid": 7}, {"power": 200}]}});
        let h = Profile::from_json(&later).hero.unwrap();
        assert_eq!((h.lifetime, h.karma, h.origin, h.element), (99, 5, 2, 3));
        assert_eq!(h.powers.len(), 1);
        assert!(h.has(PowerId(5)));
        // Super Jump bought before it became free: kept free, price back.
        let bought = json!({"hero": {"version": 2, "lifetime_karma": 99, "karma": 5,
            "origin": 2, "element": 3,
            "powers": [{"power": 2, "paid": 7}, {"power": 5, "paid": 9}]}});
        let h = Profile::from_json(&bought).hero.unwrap();
        assert_eq!((h.karma, h.powers.len()), (12, 1));
        assert!(h.has(PowerId::SUPER_JUMP) && h.has(PowerId::SPRINT));
        // A hero from before the power trees: all karma back, pick again.
        let old = json!({"hero": {"version": 1, "lifetime_karma": 9000, "karma": 50,
            "origin": 1, "affinity": [0, 1, 0, 0, 0],
            "powers": [{"power": 2, "theme": 1, "paid": 700}, {"power": 5, "paid": 2000}]}});
        let h = Profile::from_json(&old).hero.unwrap();
        assert_eq!((h.karma, h.element, h.powers.len()), (2750, 0, 0));
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  Zap\n "), "Zap");
        assert_eq!(clean_name(&"x".repeat(40)).len(), 24);
    }
}
