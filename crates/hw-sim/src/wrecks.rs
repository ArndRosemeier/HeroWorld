//! Moving cars wrecked by heroes. A punch or blast that is strong enough
//! stops a car where it is: dented, blackened, knocked askew. Its lane
//! stands still behind it (a jam) until nobody has been near for a while
//! and it is towed away ([`crate::destruction::REPAIR_SECONDS`]).
//!
//! Only the vehicle's id and the tick go over the wire: every machine
//! works out where the car was from the traffic plan (see [`crate::traffic`]).

use crate::destruction::{REPAIR_DISTANCE, REPAIR_SECONDS};
use crate::props::yaw_to_byte;
use crate::traffic::{Traffic, Vehicle};
use crate::DT;
use glam::{DVec2, DVec3};
use hw_net::ToClient;
use std::collections::BTreeMap;

/// How hard a moving car is to wreck (as a parked one, see `props`).
pub const STRENGTH: f64 = 300.0;
/// A car as a round post: radius and height (m).
const ROUND: f64 = 1.6;
const TALL: f64 = 1.6;
/// Collateral cost of a wrecked car.
const COST: f64 = 12_000.0;

/// One wreck.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wreck {
    /// When it was hit.
    pub tick: u64,
    /// Which way the blow went (a yaw byte).
    pub fell: u8,
    /// Where along its lane it stands.
    pub along: f32,
    /// Where it stands.
    pub at: DVec3,
    pub last_seen: u64,
}

/// The town's moving traffic and the wrecks in it.
#[derive(Default)]
pub struct Wrecks {
    pub traffic: Option<Traffic>,
    pub cars: BTreeMap<u64, Wreck>,
    news: Vec<ToClient>,
}

/// Distance from `at` to a car's body (0 inside it).
fn reach(v: &Vehicle, at: DVec3) -> f64 {
    let y = at.y.clamp(v.pos.y, v.pos.y + TALL);
    let d = (DVec3::new(v.pos.x, y, v.pos.z) - at).length();
    (d - ROUND).max(0.0)
}

impl Wrecks {
    pub fn new(traffic: Traffic) -> Self {
        Self {
            traffic: Some(traffic),
            ..Default::default()
        }
    }

    /// Street cars (not fliers) near `at` at `tick`, not yet wrecked.
    fn cars_near(&self, at: DVec2, r: f64, tick: u64) -> Vec<Vehicle> {
        let Some(t) = &self.traffic else {
            return Vec::new();
        };
        t.near(at, r, tick as f64 * DT, 64)
            .into_iter()
            .filter(|v| !v.flies() && !self.cars.contains_key(&v.id))
            .collect()
    }

    /// A blow at `at` going along `dir`: cars within `radius` that are
    /// weaker than the power there are wrecked. Returns how many and their
    /// collateral cost.
    pub fn hit(
        &mut self,
        at: DVec3,
        dir: DVec3,
        radius: f32,
        power: f32,
        tick: u64,
    ) -> (usize, f64) {
        let r = radius as f64;
        let fell = yaw_to_byte(dir.x.atan2(dir.z));
        let mut fresh = Vec::new();
        for v in self.cars_near(DVec2::new(at.x, at.z), r + ROUND + 1.0, tick) {
            let d = reach(&v, at);
            if d >= r || power as f64 * (1.0 - d / r) < STRENGTH {
                continue;
            }
            let Some(along) = self.traffic.as_mut().and_then(|t| t.wreck(v.id)) else {
                continue;
            };
            self.cars.insert(
                v.id,
                Wreck {
                    tick,
                    fell,
                    along,
                    at: v.pos,
                    last_seen: tick,
                },
            );
            fresh.push((v.id, tick, fell, along));
        }
        for chunk in fresh.chunks(hw_net::protocol::MAX_DAMAGE_IDS) {
            self.news.push(ToClient::CarsWrecked {
                cars: chunk.to_vec(),
                live: true,
            });
        }
        (fresh.len(), COST * fresh.len() as f64)
    }

    /// The first moving car along a ray (from `from` along unit `dir`, at
    /// most `max` m): where the ray meets it.
    pub fn raycast(&self, from: DVec3, dir: DVec3, max: f64, tick: u64) -> Option<DVec3> {
        let mid = from + dir * (max * 0.5);
        let cars = self.cars_near(DVec2::new(mid.x, mid.z), max * 0.5 + ROUND + 1.0, tick);
        if cars.is_empty() {
            return None;
        }
        let mut t = 0.0;
        while t < max {
            let at = from + dir * t;
            if cars.iter().any(|v| reach(v, at) <= 0.0) {
                return Some(at);
            }
            t += 0.25;
        }
        None
    }

    /// Tows away wrecks nobody has been near for [`REPAIR_SECONDS`].
    pub fn repair_unseen(&mut self, watchers: &[DVec3], tick: u64, hz: u32) {
        let due = (REPAIR_SECONDS * hz as f64) as u64;
        let mut gone = Vec::new();
        for (id, w) in self.cars.iter_mut() {
            let p = DVec2::new(w.at.x, w.at.z);
            if watchers
                .iter()
                .any(|h| DVec2::new(h.x, h.z).distance(p) < REPAIR_DISTANCE)
            {
                w.last_seen = tick;
            } else if tick.saturating_sub(w.last_seen) >= due {
                gone.push(*id);
            }
        }
        if gone.is_empty() {
            return;
        }
        for id in &gone {
            self.cars.remove(id);
            if let Some(t) = self.traffic.as_mut() {
                t.clear_wreck(*id);
            }
        }
        for chunk in gone.chunks(hw_net::protocol::MAX_DAMAGE_IDS) {
            self.news.push(ToClient::CarsCleared {
                cars: chunk.to_vec(),
            });
        }
    }

    /// Every wreck as messages, for a player who joins.
    pub fn snapshot(&self) -> Vec<ToClient> {
        let all: Vec<(u64, u64, u8, f32)> = self
            .cars
            .iter()
            .map(|(id, w)| (*id, w.tick, w.fell, w.along))
            .collect();
        all.chunks(hw_net::protocol::MAX_DAMAGE_IDS)
            .map(|c| ToClient::CarsWrecked {
                cars: c.to_vec(),
                live: false,
            })
            .collect()
    }

    pub fn take_news(&mut self) -> Vec<ToClient> {
        std::mem::take(&mut self.news)
    }

    /// No wrecks (a save is loaded: they are not kept).
    pub fn clear(&mut self) {
        self.cars.clear();
        self.news.clear();
        if let Some(t) = self.traffic.as_mut() {
            t.clear_wrecks();
        }
    }

    /// The street cars as they are, for a player who joins (before the
    /// wrecks in [`Self::snapshot`]).
    pub fn traffic_sync(&self) -> Vec<ToClient> {
        self.traffic.as_ref().map_or(Vec::new(), |t| {
            t.sync_out()
                .into_iter()
                .map(|c| ToClient::Traffic {
                    time: c.time,
                    lane: c.lane,
                    first: c.first,
                    cars: c.cars,
                })
                .collect()
        })
    }
}
