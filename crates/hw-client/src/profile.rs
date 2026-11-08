//! What the game remembers on this PC between runs: your hero's name, the
//! world you last played and your sound settings. A small JSON file in the
//! user's settings folder (`%APPDATA%\HeroWorld` on Windows,
//! `~/.config/heroworld` elsewhere). A missing or broken file is a fresh start.

use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub name: String,
    /// The world you last played (its seed).
    pub seed: u64,
    /// True once you have played a hero: the title then offers Continue.
    pub played: bool,
    /// Music and effect volumes, 0..1.
    pub music: f32,
    pub sounds: f32,
    /// Keep the title still (no swooping letters, no drifting camera).
    pub reduced_motion: bool,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: "Hero".into(),
            seed: 1,
            played: false,
            music: 0.8,
            sounds: 0.9,
            reduced_motion: false,
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

    pub fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .map(|v| Self::from_json(&v))
            .unwrap_or_default()
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
            played: v["played"].as_bool().unwrap_or(d.played),
            music: f("music", d.music),
            sounds: f("sounds", d.sounds),
            reduced_motion: v["reduced_motion"].as_bool().unwrap_or(d.reduced_motion),
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "seed": self.seed,
            "played": self.played,
            "music": self.music,
            "sounds": self.sounds,
            "reduced_motion": self.reduced_motion,
        })
    }

    /// Writes the file; a failure only costs remembering.
    pub fn save(&self) {
        let Some(path) = Self::path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(&self.to_json()) {
            if let Err(e) = std::fs::write(&path, s) {
                eprintln!("Could not save {}: {e}", path.display());
            }
        }
    }
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
            played: true,
            music: 0.25,
            sounds: 0.5,
            reduced_motion: true,
        };
        assert_eq!(Profile::from_json(&p.to_json()), p);
        let junk = json!({"name": 7, "music": 9.0, "seed": "x"});
        let q = Profile::from_json(&junk);
        assert_eq!(q.name, "Hero");
        assert_eq!(q.music, 1.0);
        assert_eq!(q.seed, 1);
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  Zap\n "), "Zap");
        assert_eq!(clean_name(&"x".repeat(40)).len(), 24);
    }
}
