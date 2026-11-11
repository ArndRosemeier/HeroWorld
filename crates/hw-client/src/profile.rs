//! What the game remembers on this PC between runs: your hero's name, looks
//! (the Actors character made in the creator) and hero file (origin, karma,
//! powers), the world you last played and your sound settings. A small JSON file in the
//! user's settings folder (`%APPDATA%\HeroWorld` on Windows,
//! `~/.config/heroworld` elsewhere). A missing or broken file is a fresh start.

use crate::missionview::{Filed, CABINET_KEEP};
use crate::tray::Tray;
use actors_generator::CharacterConfig;
use hw_net::{OwnedPower, PowerId, Sheet};
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub name: String,
    /// The world you last played (its seed), and about how many people
    /// live in its city (picked in the title's city window).
    pub seed: u64,
    pub people: f64,
    /// True once you have played a hero: the title then offers Continue.
    pub played: bool,
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
            played: false,
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
            people: v["people"]
                .as_f64()
                .filter(|p| p.is_finite())
                .map_or(d.people, |p| p.clamp(10.0, 1e8)),
            played: v["played"].as_bool().unwrap_or(d.played),
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
            hero: sheet_from_json(&v["hero"]),
            look: serde_json::from_value::<CharacterConfig>(v["look"].clone()).ok(),
            tray: Tray::from_json(&v["tray"]),
            cabinet: v["cabinet"].as_array().map_or_else(Vec::new, |a| {
                let keep = a.len().saturating_sub(CABINET_KEEP);
                a[keep..].iter().filter_map(Filed::from_json).collect()
            }),
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "seed": self.seed,
            "people": self.people,
            "played": self.played,
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
            "hero": self.hero.as_ref().map(sheet_to_json),
            "look": self.look.as_ref().and_then(|l| serde_json::to_value(l).ok()),
            "tray": self.tray.to_json(),
            "cabinet": self.cabinet.iter().map(Filed::to_json).collect::<Vec<_>>(),
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

/// Hero file format, so later fields don't break old heroes. Version 2
/// brought the power trees: older heroes get all their karma back once and
/// pick again, element too.
const HERO_VERSION: u64 = 2;

fn sheet_to_json(s: &Sheet) -> Value {
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

fn sheet_from_json(v: &Value) -> Option<Sheet> {
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
            played: true,
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
        assert_eq!(Profile::from_json(&p.to_json()), p);
        let junk = json!({"name": 7, "music": 9.0, "seed": "x", "people": "many", "quality": "max", "fov": 500, "render_scale": 0.1});
        let q = Profile::from_json(&junk);
        assert_eq!(q.name, "Hero");
        assert_eq!(q.music, 1.0);
        assert_eq!(q.seed, 1);
        assert_eq!(q.people, hw_sim::DEFAULT_POPULATION);
        assert_eq!(q.quality, "high");
        assert_eq!(q.fov, 90.0);
        assert_eq!(q.render_scale, 0.5);
        assert_eq!(q.hero, None);
        assert_eq!(q.look, None);
        assert!(q.cabinet.is_empty());
        // A hero file with a power from a later version keeps the rest.
        let later = json!({"hero": {"version": 2, "lifetime_karma": 99, "karma": 5,
            "origin": 2, "element": 3,
            "powers": [{"power": 2, "paid": 7}, {"power": 200}]}});
        let h = Profile::from_json(&later).hero.unwrap();
        assert_eq!((h.lifetime, h.karma, h.origin, h.element), (99, 5, 2, 3));
        assert_eq!(h.powers.len(), 1);
        assert!(h.has(PowerId(2)));
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
