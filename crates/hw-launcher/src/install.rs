//! Installing and updating: which bundles to fetch, fetching them, unpacking
//! them into the install folder. No window and no network here (the network
//! comes in as a [`Source`]), so the tests run it on a temp folder.

use crate::manifest::{safe_path, sha256_file, Bundle, Manifest};
use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// The launcher's own folder inside the install folder.
const META: &str = ".launcher";

/// Where release files come from: GitHub, or a folder in the tests.
pub trait Source {
    /// Fetch `file` into `to`. A partial `to` may be continued. `progress`
    /// gets the bytes written so far, including any already there.
    fn fetch(&self, file: &str, to: &Path, progress: &mut dyn FnMut(u64)) -> Result<(), String>;
}

/// What the work is doing now, for the progress bar.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// Checking the files on disk: `done` of `total` bytes read.
    Checking { done: u64, total: u64 },
    /// Downloading: `done` of `total` bytes over all bundles.
    Downloading {
        bundle: String,
        done: u64,
        total: u64,
    },
    /// Unpacking a bundle.
    Unpacking { bundle: String },
}

/// What an update needs: the bundles to download and the files to delete.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub fetch: Vec<Bundle>,
    pub remove: Vec<String>,
}

impl Plan {
    pub fn bytes(&self) -> u64 {
        self.fetch.iter().map(|b| b.size).sum()
    }
    pub fn is_empty(&self) -> bool {
        self.fetch.is_empty() && self.remove.is_empty()
    }
}

/// The game in an install folder.
pub struct Install {
    pub dir: PathBuf,
}

impl Install {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Install { dir: dir.into() }
    }

    fn meta(&self) -> PathBuf {
        self.dir.join(META)
    }

    /// The release installed last, if any.
    pub fn installed(&self) -> Option<Manifest> {
        let text = fs::read_to_string(self.meta().join("installed.json")).ok()?;
        Manifest::parse(&text).ok()
    }

    /// Is the game there to start?
    pub fn exe(&self) -> PathBuf {
        self.dir.join(if cfg!(windows) {
            "heroworld.exe"
        } else {
            "heroworld"
        })
    }

    /// What to do to get from what is on disk to `latest`. With `check`
    /// every file is read and hashed (repair, or a folder from the old zip);
    /// otherwise a bundle that was installed whole is trusted when its
    /// files are all there at the right size.
    pub fn plan(
        &self,
        latest: &Manifest,
        check: bool,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(Step),
    ) -> Result<Plan, String> {
        let installed = self.installed();
        let mut plan = Plan::default();
        let total: u64 = latest
            .bundles
            .iter()
            .flat_map(|b| &b.files)
            .map(|f| f.size)
            .sum();
        let mut done = 0;
        for b in &latest.bundles {
            let had = installed
                .as_ref()
                .and_then(|m| m.bundles.iter().find(|o| o.name == b.name))
                .is_some_and(|o| o.content == b.content);
            let ok = if had && !check {
                done += b.files.iter().map(|f| f.size).sum::<u64>();
                b.files
                    .iter()
                    .all(|f| fs::metadata(self.dir.join(&f.path)).is_ok_and(|m| m.len() == f.size))
            } else {
                let mut all = true;
                for f in &b.files {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("cancelled".into());
                    }
                    let path = self.dir.join(safe_path(&f.path)?);
                    let same = fs::metadata(&path).is_ok_and(|m| m.len() == f.size)
                        && sha256_file(&path).is_ok_and(|h| h == f.sha256);
                    done += f.size;
                    progress(Step::Checking { done, total });
                    if !same {
                        all = false;
                        break;
                    }
                }
                all
            };
            if !ok {
                plan.fetch.push(b.clone());
            }
        }
        // Files the old release had and the new one has not.
        if let Some(old) = installed {
            let keep: BTreeSet<&str> = latest
                .bundles
                .iter()
                .flat_map(|b| &b.files)
                .map(|f| f.path.as_str())
                .collect();
            for f in old.bundles.iter().flat_map(|b| &b.files) {
                if !keep.contains(f.path.as_str()) && safe_path(&f.path).is_ok() {
                    plan.remove.push(f.path.clone());
                }
            }
        }
        Ok(plan)
    }

    /// Download and unpack what `plan` says, then remember `latest` as
    /// installed. A failure part way leaves every file whole (each is
    /// written beside its place and renamed over it) and the next run
    /// picks up where this one stopped.
    pub fn apply(
        &self,
        latest: &Manifest,
        plan: &Plan,
        source: &dyn Source,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(Step),
    ) -> Result<(), String> {
        let downloads = self.meta().join("download");
        fs::create_dir_all(&downloads).map_err(|e| io_err("make", &downloads, e))?;
        let total = plan.bytes();
        let mut before = 0;
        for b in &plan.fetch {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            let zip = downloads.join(&b.file);
            let mut tries = 0;
            loop {
                tries += 1;
                // Left whole by a run that stopped before unpacking?
                let size = fs::metadata(&zip).map(|m| m.len()).unwrap_or(0);
                if size >= b.size {
                    if sha256_file(&zip).is_ok_and(|h| h == b.sha256) {
                        break;
                    }
                    let _ = fs::remove_file(&zip);
                }
                let mut on_bytes = |n: u64| {
                    progress(Step::Downloading {
                        bundle: b.name.clone(),
                        done: before + n.min(b.size),
                        total,
                    })
                };
                source.fetch(&b.file, &zip, &mut on_bytes)?;
                let size = fs::metadata(&zip).map(|m| m.len()).unwrap_or(0);
                let good = size == b.size && sha256_file(&zip).is_ok_and(|h| h == b.sha256);
                if good {
                    break;
                }
                // A broken or stale part (the release was rebuilt meanwhile):
                // start this file again once, then give up.
                let _ = fs::remove_file(&zip);
                if tries >= 2 {
                    return Err(format!(
                        "{} did not download right (the build may be being replaced; try again in a few minutes)",
                        b.file
                    ));
                }
            }
            before += b.size;
            progress(Step::Unpacking {
                bundle: b.name.clone(),
            });
            self.unpack(b, &zip, cancel)?;
            let _ = fs::remove_file(&zip);
        }
        for p in &plan.remove {
            let _ = fs::remove_file(self.dir.join(p));
        }
        let text = serde_json::to_string_pretty(latest).map_err(|e| e.to_string())?;
        write_atomic(&self.meta().join("installed.json"), text.as_bytes())?;
        Ok(())
    }

    /// Unpack the files of `bundle` from its zip, checking each one.
    fn unpack(&self, bundle: &Bundle, zip: &Path, cancel: &AtomicBool) -> Result<(), String> {
        let file = fs::File::open(zip).map_err(|e| io_err("open", zip, e))?;
        let mut archive =
            zip::ZipArchive::new(file).map_err(|e| format!("{}: not a zip ({e})", bundle.file))?;
        for f in &bundle.files {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            let rel = safe_path(&f.path)?;
            let mut entry = archive
                .by_name(rel)
                .map_err(|_| format!("{} is missing {rel}", bundle.file))?;
            let target = self.dir.join(rel);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| io_err("make", parent, e))?;
            }
            let tmp = target.with_extension("hwpart");
            {
                let mut out = fs::File::create(&tmp).map_err(|e| io_err("write", &tmp, e))?;
                let mut buf = vec![0u8; 1 << 20];
                loop {
                    let n = entry
                        .read(&mut buf)
                        .map_err(|e| format!("{}: {rel}: {e}", bundle.file))?;
                    if n == 0 {
                        break;
                    }
                    out.write_all(&buf[..n])
                        .map_err(|e| io_err("write", &tmp, e))?;
                }
            }
            if sha256_file(&tmp).ok().as_deref() != Some(f.sha256.as_str()) {
                let _ = fs::remove_file(&tmp);
                return Err(format!("{rel} came out wrong from {}", bundle.file));
            }
            fs::rename(&tmp, &target).map_err(|e| {
                let _ = fs::remove_file(&tmp);
                if e.kind() == io::ErrorKind::PermissionDenied {
                    format!("{rel} is in use: close HeroWorld and try again")
                } else {
                    io_err("replace", &target, e)
                }
            })?;
        }
        Ok(())
    }
}

/// Write a file whole or not at all.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| io_err("make", parent, e))?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(|e| io_err("write", &tmp, e))?;
    fs::rename(&tmp, path).map_err(|e| io_err("replace", path, e))
}

fn io_err(what: &str, path: &Path, e: io::Error) -> String {
    format!("could not {what} {}: {e}", path.display())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{content_hash, hex, FileEntry, LauncherFile};
    use sha2::{Digest, Sha256};
    use std::cell::RefCell;

    /// A release in a folder, counting what was fetched.
    struct Folder {
        dir: PathBuf,
        fetched: RefCell<Vec<String>>,
    }

    impl Source for Folder {
        fn fetch(
            &self,
            file: &str,
            to: &Path,
            progress: &mut dyn FnMut(u64),
        ) -> Result<(), String> {
            self.fetched.borrow_mut().push(file.to_string());
            fs::copy(self.dir.join(file), to).map_err(|e| e.to_string())?;
            progress(fs::metadata(to).unwrap().len());
            Ok(())
        }
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("hw-launcher-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn sha(bytes: &[u8]) -> String {
        hex(&Sha256::digest(bytes))
    }

    /// A release of `bundles` (name, [(path, contents)]) written to `dir`.
    fn release(dir: &Path, version: &str, bundles: &[(&str, &[(&str, &str)])]) -> Manifest {
        let mut out = Vec::new();
        for (name, files) in bundles {
            let zpath = dir.join(format!("{name}.zip"));
            let mut z = zip::ZipWriter::new(fs::File::create(&zpath).unwrap());
            let mut entries = Vec::new();
            for (path, body) in *files {
                z.start_file(*path, zip::write::SimpleFileOptions::default())
                    .unwrap();
                z.write_all(body.as_bytes()).unwrap();
                entries.push(FileEntry {
                    path: path.to_string(),
                    size: body.len() as u64,
                    sha256: sha(body.as_bytes()),
                });
            }
            z.finish().unwrap();
            let bytes = fs::read(&zpath).unwrap();
            out.push(Bundle {
                name: name.to_string(),
                file: format!("{name}.zip"),
                size: bytes.len() as u64,
                sha256: sha(&bytes),
                content: content_hash(&entries),
                files: entries,
            });
        }
        Manifest {
            game: "HeroWorld".into(),
            version: version.into(),
            commit: String::new(),
            built: String::new(),
            whats_new: String::new(),
            launcher: LauncherFile {
                version: "1.0.0".into(),
                file: "HeroWorldLauncher.exe".into(),
                size: 0,
                sha256: String::new(),
            },
            bundles: out,
        }
    }

    fn update(install: &Install, m: &Manifest, src: &Folder, check: bool) -> Plan {
        let no = AtomicBool::new(false);
        let plan = install.plan(m, check, &no, &mut |_| {}).unwrap();
        install.apply(m, &plan, src, &no, &mut |_| {}).unwrap();
        plan
    }

    #[test]
    fn installs_then_fetches_only_what_changed() {
        let rel = temp("rel");
        let dir = temp("dir");
        let src = Folder {
            dir: rel.clone(),
            fetched: RefCell::new(vec![]),
        };
        let install = Install::new(&dir);
        let v1 = release(
            &rel,
            "0.147",
            &[
                (
                    "game",
                    &[("heroworld.exe", "exe one"), ("old.txt", "gone soon")],
                ),
                ("music", &[("assets/music/a.mp3", "la la")]),
            ],
        );
        let plan = update(&install, &v1, &src, false);
        assert_eq!(plan.fetch.len(), 2);
        assert_eq!(
            fs::read_to_string(dir.join("heroworld.exe")).unwrap(),
            "exe one"
        );
        assert_eq!(
            fs::read_to_string(dir.join("assets/music/a.mp3")).unwrap(),
            "la la"
        );
        assert_eq!(install.installed().unwrap(), v1);

        // Next night: a new exe, the same music; old.txt dropped.
        let v2 = release(
            &rel,
            "0.148",
            &[
                ("game", &[("heroworld.exe", "exe two")]),
                ("music", &[("assets/music/a.mp3", "la la")]),
            ],
        );
        src.fetched.borrow_mut().clear();
        let plan = update(&install, &v2, &src, false);
        assert_eq!(*src.fetched.borrow(), vec!["game.zip".to_string()]);
        assert_eq!(plan.remove, vec!["old.txt".to_string()]);
        assert!(!dir.join("old.txt").exists());
        assert_eq!(
            fs::read_to_string(dir.join("heroworld.exe")).unwrap(),
            "exe two"
        );

        // Up to date: nothing to do.
        let no = AtomicBool::new(false);
        assert!(install
            .plan(&v2, false, &no, &mut |_| {})
            .unwrap()
            .is_empty());

        // A damaged file: the quick check trusts sizes, the full check finds it.
        fs::write(dir.join("assets/music/a.mp3"), "la lo").unwrap();
        assert!(install
            .plan(&v2, false, &no, &mut |_| {})
            .unwrap()
            .is_empty());
        let fix = install.plan(&v2, true, &no, &mut |_| {}).unwrap();
        assert_eq!(fix.fetch.len(), 1);
        assert_eq!(fix.fetch[0].name, "music");
        install.apply(&v2, &fix, &src, &no, &mut |_| {}).unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("assets/music/a.mp3")).unwrap(),
            "la la"
        );
        let _ = fs::remove_dir_all(&rel);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_from_the_old_zip_keeps_what_matches() {
        let rel = temp("rel2");
        let dir = temp("dir2");
        let src = Folder {
            dir: rel.clone(),
            fetched: RefCell::new(vec![]),
        };
        let m = release(
            &rel,
            "0.148",
            &[
                ("game", &[("heroworld.exe", "exe two")]),
                ("packs", &[("assets/packs/p.bin", "people")]),
            ],
        );
        // Unzipped by hand earlier: packs the same, exe older.
        fs::create_dir_all(dir.join("assets/packs")).unwrap();
        fs::write(dir.join("assets/packs/p.bin"), "people").unwrap();
        fs::write(dir.join("heroworld.exe"), "exe one").unwrap();
        update(&Install::new(&dir), &m, &src, false);
        assert_eq!(*src.fetched.borrow(), vec!["game.zip".to_string()]);
        let _ = fs::remove_dir_all(&rel);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_bad_download_is_refused() {
        let rel = temp("rel3");
        let dir = temp("dir3");
        let src = Folder {
            dir: rel.clone(),
            fetched: RefCell::new(vec![]),
        };
        let mut m = release(&rel, "0.148", &[("game", &[("heroworld.exe", "exe")])]);
        m.bundles[0].sha256 = "0".repeat(64);
        let install = Install::new(&dir);
        let no = AtomicBool::new(false);
        let plan = install.plan(&m, false, &no, &mut |_| {}).unwrap();
        let err = install
            .apply(&m, &plan, &src, &no, &mut |_| {})
            .unwrap_err();
        assert!(err.contains("did not download right"), "{err}");
        assert!(!dir.join("heroworld.exe").exists());
        assert!(install.installed().is_none());
        let _ = fs::remove_dir_all(&rel);
        let _ = fs::remove_dir_all(&dir);
    }
}
