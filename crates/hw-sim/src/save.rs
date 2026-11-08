//! The host's world save: what has changed in the world it generates from its
//! seed: the clock, where the loose props lie, and the broken elements of
//! every damaged building and broken street prop (incidents join it with the milestone that adds
//! them). Heroes are not in
//! it: each player keeps their own hero on their own PC.
//!
//! A plain text file, one fact per line, so it is easy to look at and stays
//! readable as fields are added (unknown lines are skipped).

use crate::physics::PropState;
use crate::{Orientation, Prop, Sim};
use bevy_ecs::prelude::*;
use std::path::Path;

/// Format version written in the first line.
pub const SAVE_VERSION: u32 = 3;

#[derive(Clone, Debug, PartialEq)]
pub struct WorldSave {
    pub seed: u64,
    pub town_population: f64,
    pub tick: u64,
    pub crates: Vec<PropState>,
    /// Damaged buildings: index and broken elements.
    pub damage: Vec<(usize, Vec<u32>)>,
    /// Broken street props: index and which way each fell (a yaw byte).
    pub props: Vec<(u32, u8)>,
}

impl WorldSave {
    pub fn to_text(&self) -> String {
        let mut out = format!("heroworld-save {SAVE_VERSION}\n");
        out += &format!("seed {}\n", self.seed);
        out += &format!("population {:?}\n", self.town_population);
        out += &format!("tick {}\n", self.tick);
        for c in &self.crates {
            out += &format!(
                "crate {:?} {:?} {:?} {:?} {:?} {:?} {:?}\n",
                c.pos.x, c.pos.y, c.pos.z, c.rot[0], c.rot[1], c.rot[2], c.rot[3]
            );
        }
        for (i, ids) in &self.damage {
            out += &format!("damage {i}");
            for id in ids {
                out += &format!(" {id}");
            }
            out += "\n";
        }
        for (i, fell) in &self.props {
            out += &format!("prop {i} {fell}\n");
        }
        out
    }

    pub fn from_text(text: &str) -> Result<Self, String> {
        let mut lines = text.lines();
        let head = lines.next().unwrap_or_default();
        let version: u32 = head
            .strip_prefix("heroworld-save ")
            .and_then(|v| v.trim().parse().ok())
            .ok_or("not a HeroWorld save")?;
        if version > SAVE_VERSION {
            return Err(format!(
                "save version {version} is newer than this game ({SAVE_VERSION})"
            ));
        }
        let (mut seed, mut population, mut tick) = (None, None, 0);
        let mut crates = Vec::new();
        let mut damage = Vec::new();
        let mut props = Vec::new();
        for (n, line) in lines.enumerate() {
            let bad = || format!("line {}: {line}", n + 2);
            let mut words = line.split_whitespace();
            let key = words.next().unwrap_or_default();
            let rest: Vec<&str> = words.collect();
            match key {
                "seed" => seed = Some(rest.first().and_then(|v| v.parse().ok()).ok_or_else(bad)?),
                "population" => {
                    population = Some(rest.first().and_then(|v| v.parse().ok()).ok_or_else(bad)?)
                }
                "tick" => tick = rest.first().and_then(|v| v.parse().ok()).ok_or_else(bad)?,
                "crate" => {
                    let v: Vec<f64> = rest.iter().filter_map(|w| w.parse().ok()).collect();
                    if v.len() != 7 || v.iter().any(|x| !x.is_finite()) {
                        return Err(bad());
                    }
                    crates.push(PropState {
                        pos: glam::DVec3::new(v[0], v[1], v[2]),
                        rot: [v[3] as f32, v[4] as f32, v[5] as f32, v[6] as f32],
                        vel: [0.0; 3],
                    });
                }
                "damage" => {
                    let v: Option<Vec<u32>> = rest.iter().map(|w| w.parse().ok()).collect();
                    let v = v.filter(|v| !v.is_empty()).ok_or_else(bad)?;
                    damage.push((v[0] as usize, v[1..].to_vec()));
                }
                "prop" => {
                    let i = rest.first().and_then(|v| v.parse().ok()).ok_or_else(bad)?;
                    let fell = rest.get(1).and_then(|v| v.parse().ok()).ok_or_else(bad)?;
                    props.push((i, fell));
                }
                _ => {}
            }
        }
        Ok(Self {
            seed: seed.ok_or("the save has no seed")?,
            town_population: population.ok_or("the save has no population")?,
            tick,
            crates,
            damage,
            props,
        })
    }

    /// Writes the save; a crash halfway leaves the old file intact.
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, self.to_text())?;
        std::fs::rename(tmp, path)
    }

    pub fn read(path: &Path) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Self::from_text(&text).map_err(std::io::Error::other)
    }
}

impl Sim {
    /// What a save of this world holds now.
    pub fn save(&mut self) -> WorldSave {
        let mut crates: Vec<(hw_core::NetId, PropState)> = self
            .world
            .query_filtered::<(&crate::Net, &crate::Motion, &Orientation), With<Prop>>()
            .iter(&self.world)
            .map(|(n, m, o)| {
                (
                    n.0,
                    PropState {
                        pos: m.0.pos,
                        rot: o.0,
                        vel: [0.0; 3],
                    },
                )
            })
            .collect();
        crates.sort_by_key(|(id, _)| *id);
        WorldSave {
            seed: self.seed.0,
            town_population: self.town_population,
            tick: self.tick,
            crates: crates.into_iter().map(|(_, c)| c).collect(),
            damage: self.damage.broken(),
            props: self
                .street
                .broken
                .iter()
                .map(|(i, b)| (*i, b.fell))
                .collect(),
        }
    }

    /// Puts a save's changes into this world (generated from the same seed).
    pub fn restore(&mut self, save: &WorldSave) -> Result<(), String> {
        if save.seed != self.seed.0 {
            return Err(format!(
                "the save is of world {}, not {}",
                save.seed, self.seed.0
            ));
        }
        self.clear_props();
        self.tick = self.tick.max(save.tick);
        for c in &save.crates {
            self.spawn_crate(c.pos, c.rot);
        }
        let solids = self.solids();
        self.damage.clear(&solids);
        for (i, ids) in &save.damage {
            self.damage.apply(&solids, *i, ids, self.tick);
        }
        self.street.clear();
        self.street.apply(&save.props, self.tick);
        self.wrecks.clear();
        self.alarms.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::Obstacles;
    use crate::GROUND_Y;
    use hw_core::Seed;
    use std::sync::Arc;

    #[test]
    fn a_save_round_trips_through_text_and_a_file() {
        let mut sim = Sim::with_solids(Seed(9), Arc::new(Obstacles::flat(GROUND_Y)));
        sim.spawn_crate(glam::DVec3::new(1.5, 3.0, -2.25), [0.0, 0.6, 0.0, 0.8]);
        sim.spawn_crate(glam::DVec3::new(0.1, 0.76, 0.2), [0.0, 0.0, 0.0, 1.0]);
        for _ in 0..40 {
            sim.step();
        }
        let save = sim.save();
        assert_eq!(save.crates.len(), 2);
        assert_eq!(WorldSave::from_text(&save.to_text()).unwrap(), save);

        let dir = std::env::temp_dir().join(format!("hw-save-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("world.hwsave");
        save.write(&path).unwrap();
        let back = WorldSave::read(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();

        let mut fresh = Sim::with_solids(Seed(9), Arc::new(Obstacles::flat(GROUND_Y)));
        fresh.restore(&back).unwrap();
        assert_eq!(fresh.tick, 40);
        assert_eq!(fresh.save(), save);

        let mut other = Sim::with_solids(Seed(10), Arc::new(Obstacles::flat(GROUND_Y)));
        assert!(other.restore(&back).is_err());
        assert!(WorldSave::from_text("something else").is_err());
    }
}
