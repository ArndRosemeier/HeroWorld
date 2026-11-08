//! Shared basics for every HeroWorld crate: planet coordinates, seed hashing
//! and IDs. Pure Rust, no Engine, so the server and tools can use it anywhere.

pub mod coords;
pub mod det;
pub mod ids;
pub mod noise;
pub mod seed;

pub use coords::{LatLon, LocalFrame, Planet, PlanetPos};
pub use ids::{NetId, NetIdAllocator, StableId};
pub use noise::Noise;
pub use seed::{Rng, Seed};

/// Version of the world-generation rules. Bump it when a change would make an
/// existing seed produce a different world, so saves can tell.
pub const WORLD_VERSION: u32 = 19;

/// The game's version, from `VERSION` at the repository root: it starts at
/// 0.001 and grows by 0.001 with every build pushed to `main`.
pub fn version() -> &'static str {
    include_str!("../../../VERSION").trim()
}

/// What changed in each version, newest first (`WHATS_NEW.md`).
pub const WHATS_NEW: &str = include_str!("../../../WHATS_NEW.md");

#[cfg(test)]
mod version_tests {
    #[test]
    fn version_has_three_decimals_and_whats_new_lists_it() {
        let v = super::version();
        let (major, minor) = v.split_once('.').expect("x.yyy");
        assert!(major.parse::<u32>().is_ok() && minor.len() == 3, "{v}");
        assert!(minor.parse::<u32>().is_ok(), "{v}");
        let newest = super::WHATS_NEW
            .lines()
            .find_map(|l| l.strip_prefix("## "))
            .expect("a version heading");
        assert!(
            newest.starts_with(&format!("{v} ")) || newest == v,
            "WHATS_NEW.md must start with version {v}, found {newest}"
        );
    }
}
