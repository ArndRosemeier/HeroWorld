//! The factions' people in the deep underground: mole folk round their
//! camps in the Deeps, the Doctor's guards in his lab, and Syndicate relic
//! hunters prowling the buried ruins. The places come from the town plan
//! ([`hw_gen::urban::deep`]); a place gets its group only once a hero is
//! down there near it, and places far from every hero are forgotten again,
//! so exploring the whole underground keeps no more in memory than one
//! corner of it.

use crate::combat::{EnemyKind, Fights, Spot};
use glam::{DVec2, DVec3};
use hw_gen::urban::deep::{DeepSpot, SpotKind};
use hw_gen::urban::world::TownWorld;
use std::collections::BTreeMap;
use std::sync::Arc;

/// A hero this far below the street is underground (m).
pub const BELOW: f64 = 8.0;
/// How far round an underground hero the places are looked up (m).
pub const REACH: f64 = 160.0;
/// A place's group stands there while a hero is this near (m).
pub const WAKE: f64 = 45.0;
/// Places no hero is within this of are forgotten (m).
pub const FORGET: f64 = 320.0;

/// Group sizes.
const CAMP: u32 = 4;
const GUARDS: u32 = 3;
const PROWLERS: u32 = 2;
/// The lab's guards stand above the team, like the rest of the Doctor's.
const LAB_EXTRA: u8 = 2;

/// Places taken in, by where they are (to the decimetre), and spot slots
/// free for reuse.
#[derive(Default)]
pub struct DeepSpots {
    town: Option<Arc<TownWorld>>,
    known: BTreeMap<(i64, i64, i64), usize>,
    free: Vec<usize>,
}

fn key(p: DVec3) -> (i64, i64, i64) {
    (
        (p.x * 10.0).round() as i64,
        (p.y * 10.0).round() as i64,
        (p.z * 10.0).round() as i64,
    )
}

/// The fight spot for a deep place.
pub fn spot_for(d: &DeepSpot) -> Spot {
    let mut s = Spot {
        respawn_at: Some(0),
        wake: Some(WAKE),
        ..Spot::corner(d.at, d.facing)
    };
    match d.kind {
        SpotKind::MoleCamp => {
            s.only = Some(EnemyKind::Delver);
            s.size = Some(CAMP);
        }
        SpotKind::LairGuard => {
            // Brutes and technicians, as in the Doctor's quarter.
            s.extra = LAB_EXTRA;
            s.size = Some(GUARDS);
        }
        SpotKind::RuinProwler => {
            s.only = Some(EnemyKind::Gunsel);
            s.size = Some(PROWLERS);
        }
    }
    s
}

impl DeepSpots {
    pub fn new(town: Arc<TownWorld>) -> Self {
        Self {
            town: Some(town),
            ..Self::default()
        }
    }

    /// How many places are taken in now.
    pub fn len(&self) -> usize {
        self.known.len()
    }

    pub fn is_empty(&self) -> bool {
        self.known.is_empty()
    }

    /// Takes in the places round heroes underground and forgets those far
    /// from everyone (once their group has gone).
    pub fn update(&mut self, fights: &mut Fights, heroes: &[DVec3]) {
        let Some(town) = self.town.clone() else {
            return;
        };
        for &h in heroes {
            let p = DVec2::new(h.x, h.z);
            if h.y > town.ground(p) - BELOW {
                continue;
            }
            let r = DVec2::splat(REACH);
            let mut found = town.deep_spots_in(p - r, p + r);
            found.sort_by_key(|d| key(d.at));
            for d in found {
                let k = key(d.at);
                if self.known.contains_key(&k) {
                    continue;
                }
                let spot = spot_for(&d);
                let i = match self.free.pop() {
                    Some(i) => {
                        // Keep the slot's count of groups: it varies the roster.
                        let groups = fights.spots[i].groups;
                        fights.spots[i] = Spot { groups, ..spot };
                        i
                    }
                    None => {
                        fights.spots.push(spot);
                        fights.spots.len() - 1
                    }
                };
                self.known.insert(k, i);
            }
        }
        let far: Vec<(i64, i64, i64)> = self
            .known
            .iter()
            .filter(|(_, &i)| {
                let s = &fights.spots[i];
                s.members == 0 && heroes.iter().all(|h| h.distance(s.center) > FORGET)
            })
            .map(|(k, _)| *k)
            .collect();
        for k in far {
            if let Some(i) = self.known.remove(&k) {
                // Asleep for good until reused: no hero wakes it.
                let s = &mut fights.spots[i];
                s.wake = Some(0.0);
                s.respawn_at = None;
                self.free.push(i);
            }
        }
    }
}
