//! Doors in the simulation. Every door is shut until someone opens it:
//! heroes open and shut them with E (the client asks, the server checks
//! the hero stands at that door), and people on foot (villains, sidekicks,
//! the town's heroes) open a shut door they walk up to and shut it behind
//! them once nobody is near it any more.
//!
//! Whether a door stands open lives in [`Obstacles`], so it is what
//! collides; only open doors are kept (the rest are shut by default), so a
//! town of any size costs nothing for doors nobody touched. Every change
//! goes to the clients as news, and a joining player gets the open ones.

use crate::collision::{ColliderId, Obstacles};
use glam::{DVec2, DVec3};
use hw_net::ToClient;
use std::collections::BTreeMap;

/// A hero opens or shuts a door from this far from its doorway's centre
/// (m, across the ground).
pub const REACH: f64 = 2.6;
/// People on foot open a shut door when they come this close to it (m).
const WALK_UP: f64 = 1.4;
/// ...and walk at least this fast (m/s).
const WALKING: f64 = 0.3;
/// A door opened by someone walking through shuts once nobody has been
/// within [`CLEAR`] of it for this long (s).
const SHUT_AFTER: f64 = 2.5;
const CLEAR: f64 = 2.2;
/// People this far from every hero are not looked at (nobody sees them).
const SEEN: f64 = 160.0;
/// Walkers are looked at every this many ticks.
const EVERY: u64 = 3;

/// Doors opened by people walking through, and the news of every change.
#[derive(Debug, Default)]
pub struct Doors {
    /// (building, door) → where its doorway is and the tick someone was
    /// last near it.
    walked: BTreeMap<(ColliderId, u32), (DVec3, u64)>,
    news: Vec<(u64, u32, bool)>,
}

/// Why a hero's door did not move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// No such door within reach.
    TooFar,
    /// Shutting it would shut it on someone.
    InTheWay,
    /// It stands so already.
    Already,
}

impl Doors {
    /// A hero at `hero` opens (`open`) or shuts door `key` of `building`.
    /// `people` are everyone's feet (the hero's too): a door does not shut
    /// on anyone.
    pub fn hero_uses(
        &mut self,
        solids: &Obstacles,
        hero: DVec3,
        building: ColliderId,
        key: u32,
        open: bool,
        people: &[DVec3],
    ) -> Result<(), Refused> {
        let near = solids
            .doors_near(hero, REACH)
            .into_iter()
            .any(|d| d.building == building && d.key == key);
        if !near {
            return Err(Refused::TooFar);
        }
        if !open && solids.door_in_the_way(building, key, people) {
            return Err(Refused::InTheWay);
        }
        if !solids.set_door(building, key, open) {
            return Err(Refused::Already);
        }
        // A hero's door stays as the hero left it.
        self.walked.remove(&(building, key));
        self.news.push((building, key, open));
        Ok(())
    }

    /// One tick for people on foot: `walkers` (feet, velocity) open shut
    /// doors in front of them; doors they opened shut again once nobody
    /// (`everyone`, heroes too) is near. `heroes`: only people near one are
    /// looked at.
    pub fn step(
        &mut self,
        solids: &Obstacles,
        walkers: &[(DVec3, DVec3)],
        everyone: &[DVec3],
        heroes: &[DVec3],
        tick: u64,
        tick_hz: f64,
    ) {
        if !tick.is_multiple_of(EVERY) {
            return;
        }
        let seen = |p: DVec3| heroes.iter().any(|h| h.distance(p) < SEEN);
        for &(at, vel) in walkers {
            let flat = DVec2::new(vel.x, vel.z);
            if flat.length() < WALKING || !seen(at) {
                continue;
            }
            let dir = flat.normalize();
            for d in solids.doors_near(at, WALK_UP + 1.0) {
                if d.open {
                    continue;
                }
                // Walking at the doorway, not past it along the wall.
                let to = DVec2::new(d.at.x - at.x, d.at.z - at.z);
                let across = to.dot(d.into).abs();
                let along = (to - d.into * to.dot(d.into)).length();
                if to.dot(dir) <= 0.0 || across > WALK_UP || along > d.half + 0.3 {
                    continue;
                }
                if solids.set_door(d.building, d.key, true) {
                    self.walked.insert((d.building, d.key), (d.at, tick));
                    self.news.push((d.building, d.key, true));
                }
            }
        }
        // Shut what people left open behind them.
        let after = (SHUT_AFTER * tick_hz) as u64;
        let mut shut = Vec::new();
        for (k, (at, last)) in self.walked.iter_mut() {
            let flat = |p: &DVec3| DVec2::new(p.x - at.x, p.z - at.z).length();
            if everyone
                .iter()
                .any(|p| flat(p) < CLEAR && (p.y - at.y).abs() < 2.5)
            {
                *last = tick;
            } else if tick >= *last + after {
                shut.push(*k);
            }
        }
        for (b, key) in shut {
            if solids.door_in_the_way(b, key, everyone) {
                continue;
            }
            self.walked.remove(&(b, key));
            if solids.set_door(b, key, false) {
                self.news.push((b, key, false));
            }
        }
    }

    /// Forgets doors people opened (the world was reset or loaded); the
    /// doors themselves are set in [`Obstacles`].
    pub fn clear(&mut self) {
        self.walked.clear();
        self.news.clear();
    }

    /// What changed since the last call, for every client.
    pub fn take_news(&mut self) -> Vec<ToClient> {
        if self.news.is_empty() {
            return Vec::new();
        }
        std::mem::take(&mut self.news)
            .chunks(200)
            .map(|c| ToClient::Doors {
                doors: c.to_vec(),
                live: true,
            })
            .collect()
    }

    /// Every open door, for a player who joins.
    pub fn snapshot(solids: &Obstacles) -> Vec<ToClient> {
        let open: Vec<(u64, u32, bool)> = solids
            .open_doors()
            .into_iter()
            .map(|(b, k)| (b, k, true))
            .collect();
        open.chunks(200)
            .map(|c| ToClient::Doors {
                doors: c.to_vec(),
                live: false,
            })
            .collect()
    }
}
