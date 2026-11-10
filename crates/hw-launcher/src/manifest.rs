//! What a release holds: `manifest.json`, written by
//! `scripts/release-bundles.py` next to the bundle zips.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

/// Where the nightly Windows build lives.
pub const RELEASE_URL: &str =
    "https://github.com/ArndRosemeier/HeroWorld/releases/download/windows-latest/";

/// The release folder to use: [`RELEASE_URL`], or `HW_LAUNCHER_RELEASE`
/// (a URL ending in `/`) to try a build served from somewhere else.
pub fn release_url() -> String {
    std::env::var("HW_LAUNCHER_RELEASE").unwrap_or_else(|_| RELEASE_URL.to_string())
}

/// One release: the game's version, what's new, the launcher, the bundles.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub game: String,
    pub version: String,
    #[serde(default)]
    pub commit: String,
    #[serde(default)]
    pub built: String,
    #[serde(default)]
    pub whats_new: String,
    pub launcher: LauncherFile,
    pub bundles: Vec<Bundle>,
}

/// The launcher's own exe in the release.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LauncherFile {
    pub version: String,
    pub file: String,
    pub size: u64,
    pub sha256: String,
}

/// A zip of some of the game's files (the exes, the people packs, ...).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Bundle {
    pub name: String,
    pub file: String,
    /// The zip's size and hash, checked after download.
    pub size: u64,
    pub sha256: String,
    /// The hash of the files inside (see [`content_hash`]): the same files
    /// give the same value however the zip was packed.
    pub content: String,
    pub files: Vec<FileEntry>,
}

/// One file of a bundle, relative to the install folder.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FileEntry {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, String> {
        let m: Manifest =
            serde_json::from_str(text).map_err(|e| format!("bad manifest.json: {e}"))?;
        for b in &m.bundles {
            if content_hash(&b.files) != b.content {
                return Err(format!(
                    "bad manifest.json: bundle {} does not add up",
                    b.name
                ));
            }
            for f in &b.files {
                safe_path(&f.path)?;
            }
        }
        Ok(m)
    }
}

/// The same rule as `content_hash` in `scripts/release-bundles.py`.
pub fn content_hash(files: &[FileEntry]) -> String {
    let mut sorted: Vec<&FileEntry> = files.iter().collect();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));
    let mut h = Sha256::new();
    for f in sorted {
        h.update(format!("{}\t{}\t{}\n", f.path, f.size, f.sha256).as_bytes());
    }
    hex(&h.finalize())
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-256 of a file on disk.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

/// A release path must stay inside the install folder: relative, no `..`.
pub fn safe_path(p: &str) -> Result<&str, String> {
    let bad = p.is_empty()
        || p.starts_with('/')
        || p.starts_with('\\')
        || p.contains(':')
        || p.split(['/', '\\'])
            .any(|part| part == ".." || part.is_empty());
    if bad {
        Err(format!("refusing the path {p:?} from the release"))
    } else {
        Ok(p)
    }
}

/// "0.147" vs "0.148" and "1.0.0" vs "1.2.0": compares number by number.
pub fn newer(theirs: &str, ours: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.split('.')
            .map(|p| p.trim().parse::<u64>().unwrap_or(0))
            .collect()
    };
    let (a, b) = (parse(theirs), parse(ours));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (
            a.get(i).copied().unwrap_or(0),
            b.get(i).copied().unwrap_or(0),
        );
        if x != y {
            return x > y;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str) -> FileEntry {
        FileEntry {
            path: path.into(),
            size: 3,
            sha256: "ab".into(),
        }
    }

    #[test]
    fn content_hash_ignores_order() {
        let a = [entry("x"), entry("y/z")];
        let b = [entry("y/z"), entry("x")];
        assert_eq!(content_hash(&a), content_hash(&b));
        assert_ne!(content_hash(&a), content_hash(&a[..1]));
    }

    #[test]
    fn content_hash_matches_the_python_rule() {
        // python: sha256(b"a.txt\t3\tab\n").hexdigest()
        let h = content_hash(&[FileEntry {
            path: "a.txt".into(),
            size: 3,
            sha256: "ab".into(),
        }]);
        let mut s = Sha256::new();
        s.update(b"a.txt\t3\tab\n");
        assert_eq!(h, hex(&s.finalize()));
    }

    #[test]
    fn paths_outside_the_folder_are_refused() {
        assert!(safe_path("assets/packs/a.bin").is_ok());
        for bad in ["../x", "/etc/x", "C:/x", "a//b", "a/../b", "", "\\x"] {
            assert!(safe_path(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn versions_compare_by_number() {
        assert!(newer("0.148", "0.147"));
        assert!(newer("0.1000", "0.999"));
        assert!(!newer("0.147", "0.147"));
        assert!(newer("1.0.1", "1.0"));
        assert!(!newer("1.0", "1.0.0"));
    }
}
