//! Wonders: the rare, one-of-a-kind places of the planet.
//!
//! The Aether is everywhere in small amounts; a wonder is where it gathers
//! into something nobody mistakes. Each is unique, named, and kept far from
//! the others, so finding one is an event. They are placed from the seed on
//! the planet map, by kind, wherever the land suits them best.

use super::biome::Biome;
use super::map::PlanetMap;
use glam::DVec3;
use hw_core::{Rng, Seed};

/// Minimum distance between two wonders (m).
pub const SPACING_M: f64 = 1_700_000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WonderKind {
    /// A mountain adrift above its own shadow, rock islands around it.
    FloatingMountain,
    /// A canyon of living crystal.
    CrystalCanyon,
    /// A river that falls from the clouds.
    SkyFall,
    /// A whirlpool wide enough to see from the sky.
    Maelstrom,
    /// A peak crowned with an aurora that never ends.
    AuroraPeak,
    /// A desert of mirror-glass dunes.
    GlassDesert,
    /// Jungle that grows in hanging gardens up the cliffs.
    HangingGardens,
    /// A chasm that goes down further than it should.
    TheWound,
}

impl WonderKind {
    pub const ALL: [WonderKind; 8] = [
        WonderKind::FloatingMountain,
        WonderKind::CrystalCanyon,
        WonderKind::SkyFall,
        WonderKind::Maelstrom,
        WonderKind::AuroraPeak,
        WonderKind::GlassDesert,
        WonderKind::HangingGardens,
        WonderKind::TheWound,
    ];

    pub fn label(self) -> &'static str {
        match self {
            WonderKind::FloatingMountain => "floating mountain",
            WonderKind::CrystalCanyon => "crystal canyon",
            WonderKind::SkyFall => "sky fall",
            WonderKind::Maelstrom => "maelstrom",
            WonderKind::AuroraPeak => "aurora peak",
            WonderKind::GlassDesert => "glass desert",
            WonderKind::HangingGardens => "hanging gardens",
            WonderKind::TheWound => "the wound",
        }
    }

    /// How well a cell suits this kind, 0 if it cannot be here.
    fn suits(self, map: &PlanetMap, c: usize, aether: f64) -> f64 {
        let (h, t) = (map.elev[c] as f64, map.temp[c] as f64);
        let sea = h <= 0.0;
        let biome = map.biome[c];
        let ok = match self {
            WonderKind::FloatingMountain => !sea && h > 500.0,
            WonderKind::CrystalCanyon => !sea && map.rain[c] < 700.0 && h > 100.0,
            WonderKind::SkyFall => map.is_river(c as u32) && h > 800.0,
            WonderKind::Maelstrom => h < -2500.0 && t > -2.0,
            WonderKind::AuroraPeak => !sea && h > 2000.0 && t < 0.0,
            WonderKind::GlassDesert => biome == Biome::Desert,
            WonderKind::HangingGardens => {
                matches!(biome, Biome::Rainforest | Biome::SeasonalForest)
            }
            WonderKind::TheWound => !sea && h > 100.0 && h < 1500.0,
        };
        if ok {
            0.2 + aether
        } else {
            0.0
        }
    }
}

/// A wonder of the planet.
#[derive(Clone, Debug)]
pub struct Wonder {
    pub kind: WonderKind,
    pub name: String,
    /// Where it lies (unit vector).
    pub at: DVec3,
    /// How far its influence reaches (m).
    pub radius_m: f64,
    /// Seed for building it, unique to this wonder.
    pub seed: Seed,
}

/// Place the planet's wonders: two of each kind where there is room.
pub fn place(map: &PlanetMap, seed: Seed) -> Vec<Wonder> {
    let mut rng = seed.child("wonders").rng();
    let cells = map.grid.cells();
    let aether: Vec<f64> = (0..cells)
        .map(|c| map.globe.aether(map.centers[c].as_dvec3()))
        .collect();
    let mut out: Vec<Wonder> = Vec::new();
    for round in 0..2 {
        for kind in WonderKind::ALL {
            // The best few hundred places for it, then a random one of those.
            let mut cands: Vec<(f64, usize)> = (0..cells)
                .filter_map(|c| {
                    let s = kind.suits(map, c, aether[c]);
                    (s > 0.0).then_some((s, c))
                })
                .collect();
            cands.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
            cands.truncate(2000);
            rng.shuffle(&mut cands);
            let radius = map.globe.radius();
            for (_, c) in cands {
                let at = map.centers[c].as_dvec3();
                let far = out
                    .iter()
                    .all(|w| w.at.dot(at).clamp(-1.0, 1.0).acos() * radius > SPACING_M);
                if far {
                    out.push(Wonder {
                        kind,
                        name: name(kind, &mut rng),
                        at,
                        radius_m: 60_000.0 + rng.range(0.0, 60_000.0),
                        seed: seed.child("wonder").child(out.len() as u64 + 100 * round),
                    });
                    break;
                }
            }
        }
    }
    out
}

fn name(kind: WonderKind, rng: &mut Rng) -> String {
    let adj = [
        "Weeping",
        "Singing",
        "Hollow",
        "Gilded",
        "Drifting",
        "Sleepless",
        "Mirrored",
        "Veiled",
        "Burning",
        "Silent",
        "Last",
        "First",
        "Thousand-Voiced",
        "Starlit",
    ];
    let place = match kind {
        WonderKind::FloatingMountain => ["Mount", "Crown", "Spire"],
        WonderKind::CrystalCanyon => ["Canyon", "Gorge", "Cut"],
        WonderKind::SkyFall => ["Falls", "Cascade", "Stair"],
        WonderKind::Maelstrom => ["Maw", "Eye", "Drain"],
        WonderKind::AuroraPeak => ["Peak", "Throne", "Beacon"],
        WonderKind::GlassDesert => ["Sands", "Mirror", "Waste"],
        WonderKind::HangingGardens => ["Gardens", "Terraces", "Canopy"],
        WonderKind::TheWound => ["Wound", "Chasm", "Rift"],
    };
    format!("The {} {}", rng.pick(&adj), rng.pick(&place))
}
