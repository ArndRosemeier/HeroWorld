//! The launcher's own settings, kept beside the game's saves.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    /// Where the game is installed.
    pub install_dir: PathBuf,
    /// Close the launcher when the game starts.
    pub close_on_play: bool,
    /// The last room code typed into Join.
    pub last_room: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            install_dir: default_install_dir(),
            close_on_play: true,
            last_room: String::new(),
        }
    }
}

/// `%LOCALAPPDATA%\Programs\HeroWorld` on Windows: per user, no admin rights.
pub fn default_install_dir() -> PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    if cfg!(windows) {
        base.join("Programs").join("HeroWorld")
    } else {
        base.join("HeroWorld").join("game")
    }
}

/// Where the game keeps heroes, settings and crash reports
/// (`%APPDATA%\HeroWorld`); the launcher's settings go there too.
pub fn saves_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("HeroWorld")
}

fn path() -> PathBuf {
    saves_dir().join("launcher.json")
}

impl Config {
    pub fn load() -> Self {
        std::fs::read_to_string(path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = crate::install::write_atomic(&path(), text.as_bytes());
        }
    }
}
