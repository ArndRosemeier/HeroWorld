//! Diagnosis helpers for test builds on players' machines.
//!
//! `env.txt` beside the profile sets environment switches (`KEY=VALUE` per
//! line, `#` comments) for players who start the game from `start.bat`.
//! `logs/quality.log` records every change the frame-rate tuner makes.

use engine::World;
use std::io::Write;

/// Reads `env.txt` beside the profile. Variables already set win.
/// Call before anything reads the environment (before `Engine::run`).
pub fn load_env_file() {
    let Some(path) = crate::profile::Profile::dir().map(|d| d.join("env.txt")) else {
        return;
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let (k, v) = (k.trim(), v.trim());
            if !k.is_empty() && std::env::var_os(k).is_none() {
                // Still single-threaded: nothing else reads the environment yet.
                std::env::set_var(k, v);
            }
        }
    }
}

/// Logs the picture detail (LOD bias, AO, GI) whenever it changes.
pub struct QualityLog {
    file: Option<std::fs::File>,
    last: Option<(f32, bool, bool)>,
    lens: Option<(f32, f32, f32)>,
    frame: u64,
}

impl QualityLog {
    pub fn open() -> Self {
        let file = std::fs::create_dir_all("logs").ok().and_then(|_| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("logs/quality.log")
                .ok()
        });
        let mut log = Self {
            file,
            last: None,
            lens: None,
            frame: 0,
        };
        log.line(&format!("--- HeroWorld {} start", hw_core::version()));
        log
    }

    /// Frames seen so far.
    pub fn frame(&self) -> u64 {
        self.frame
    }

    pub fn mark(&mut self, what: &str) {
        let line = format!("frame {:6}: {what}", self.frame);
        self.line(&line);
    }

    fn line(&mut self, s: &str) {
        if let Some(f) = &mut self.file {
            let _ = writeln!(f, "{s}");
        }
    }

    /// Call once per frame.
    pub fn note(&mut self, world: &World, seconds: f32) {
        self.frame += 1;
        let cam = world.camera();
        let lens = (cam.near(), cam.far(), cam.fov_y_degrees());
        if self.lens != Some(lens) {
            self.lens = Some(lens);
            let line = format!(
                "{seconds:7.2} s frame {:6}: camera near {} far {} fov {}",
                self.frame, lens.0, lens.1, lens.2
            );
            self.line(&line);
        }
        let now = (
            world.lod_bias(),
            world.ambient_occlusion().is_some(),
            world.global_illumination().is_some(),
        );
        if self.last != Some(now) {
            self.last = Some(now);
            let fps = world
                .target_fps()
                .map_or("off".to_string(), |f| format!("{f}"));
            let line = format!(
                "{seconds:7.2} s frame {:6}: lod bias {:.3}, ao {}, gi {}, quality {}, smooth target {fps}",
                self.frame,
                now.0,
                now.1,
                now.2,
                world.quality().name()
            );
            self.line(&line);
        }
    }
}
