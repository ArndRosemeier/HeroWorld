//! Hostages on a roof have no stairs to walk down: Sky Patrol fetches
//! them. Once the first of the roof's crews is beaten a cruiser flies in
//! and lands on the roof; everyone set free walks to it and climbs in;
//! with the last aboard it shuts its canopy and climbs away. A roof with
//! no room for a cruiser has no lift, and people leave it as from any
//! open place.

use super::running::Running;
use super::*;
use crate::collision::{Obstacles, Solid};
use crate::police::{self, Pickup};
use glam::DVec2;

/// Room kept free round the cruiser's box (m).
const ROOM: f64 = 0.7;
/// The pad keeps this far from the crews and the hostages (m).
const CLEAR: f64 = 5.0;
/// Seconds the cruiser waits on the roof at most once the case is over.
pub const WAIT: f64 = 45.0;
/// Seconds between the last aboard and the canopy shut.
const SHUT: f64 = 1.5;

/// The cruiser on (or on its way to) a roof, and who walks to it.
#[derive(Clone, Debug)]
pub struct Lift {
    car: Pickup,
    riders: Vec<Entity>,
    /// The case is over since this tick: it leaves when its riders are in.
    over: Option<u64>,
}

/// A place for a cruiser on the roof round `site.at`: level roof under
/// the whole box, nothing standing in it, open to the sky, clear of
/// `used`. Its skids' feet and its yaw.
pub(crate) fn pad(solids: &Obstacles, site: &sites::Site, used: &[DVec3]) -> Option<(DVec3, f64)> {
    let mid = DVec2::new(site.at.x, site.at.z);
    let f = site.facing.normalize_or(DVec2::X);
    let top = site.at.y;
    for ring in 0..=4 {
        let n = if ring == 0 { 1 } else { 8 };
        for k in 0..n {
            let c = mid
                + hw_core::det::dir2(std::f64::consts::TAU * k as f64 / 8.0) * (ring as f64 * 3.0);
            for fwd in [f, f.perp()] {
                let yaw = hw_core::det::atan2(fwd.x, fwd.y);
                let at = DVec3::new(c.x, top, c.y);
                if used.iter().any(|u| u.distance(at) < CLEAR) {
                    continue;
                }
                let b = police::cruiser_solid(at, yaw);
                let grow = police::CRUISER_HALF + DVec2::splat(ROOM);
                let fits = [-1.0, 0.0, 1.0].iter().all(|&i| {
                    [-1.0, -0.5, 0.0, 0.5, 1.0].iter().all(|&j| {
                        let q = b.to_world(DVec2::new(grow.x * i, grow.y * j));
                        (solids.floor_under(q, top + 1.0) - top).abs() < 0.2
                            && [0.3, 1.0, 1.7]
                                .iter()
                                .all(|h| !solids.solid_at(DVec3::new(q.x, top + h, q.y)))
                    })
                });
                let m = b.to_world(DVec2::ZERO);
                let m = DVec3::new(m.x, top, m.y);
                if fits && !solids.line_blocked(m + DVec3::Y * 70.0, m + DVec3::Y * 2.0) {
                    return Some((at, yaw));
                }
            }
        }
    }
    None
}

impl Lift {
    pub(crate) fn come(
        world: &mut World,
        town: Option<&hw_gen::urban::area::Area>,
        tick: u64,
        pad: (DVec3, f64),
    ) -> Self {
        Lift {
            car: Pickup::come(world, town, tick, pad.0, pad.1),
            riders: Vec::new(),
            over: None,
        }
    }

    /// Someone set free at `from` goes to the cruiser: their way, and the
    /// tick they may start walking from (it has landed).
    pub(crate) fn board(&mut self, e: Entity, from: DVec3) -> (Vec<DVec3>, u64) {
        self.riders.push(e);
        let door = self.car.door();
        // Round the nose or the tail rather than through the car.
        let mut way = Vec::new();
        let b = police::cruiser_solid(self.car.pad, self.car.yaw);
        let l = |p: DVec3| {
            let d = DVec2::new(p.x, p.z) - b.origin;
            DVec2::new(d.dot(b.along), d.dot(b.inward))
        };
        let (a, z) = (l(from), l(door));
        if a.x * z.x < 0.0 && a.y.abs() < b.max.y + 1.0 {
            let end = if a.y >= 0.0 { 1.0 } else { -1.0 } * (b.max.y + 1.2);
            for x in [a.x.signum() * (b.max.x + 1.0), z.x] {
                let q = b.to_world(DVec2::new(x, end));
                way.push(DVec3::new(q.x, door.y, q.y));
            }
        }
        way.push(door);
        (way, self.car.lands_at() + crate::TICK_HZ as u64)
    }

    /// The case is over (closed or left): it goes once its riders are in.
    pub(crate) fn release(&mut self, tick: u64) {
        self.over.get_or_insert(tick);
    }

    /// Every tick. True once it has flown off and is gone.
    pub(crate) fn step(
        &mut self,
        world: &mut World,
        town: Option<&hw_gen::urban::area::Area>,
        tick: u64,
    ) -> bool {
        self.riders.retain(|&e| world.get_entity(e).is_ok());
        if let Some(since) = self.over {
            let late = tick >= since + (WAIT * crate::TICK_HZ as f64) as u64;
            if !self.car.leaving() && ((self.riders.is_empty() && self.car.stands(tick)) || late) {
                for e in self.riders.drain(..) {
                    world.despawn(e);
                }
                self.car.leave(town, tick, SHUT);
            }
        }
        self.car.step(world, tick)
    }

    pub fn stands(&self, tick: u64) -> bool {
        self.car.stands(tick)
    }

    pub fn leaving(&self) -> bool {
        self.car.leaving()
    }

    pub fn riders(&self) -> usize {
        self.riders.len()
    }

    pub fn pad(&self) -> DVec3 {
        self.car.pad
    }

    pub(crate) fn solid(&self, tick: u64) -> Option<Solid> {
        self.car.solid(tick)
    }

    pub(crate) fn remove(self, world: &mut World) {
        self.car.remove(world);
    }
}

impl Running {
    /// The roof's cruiser, if one came.
    pub fn lift(&self) -> Option<&Lift> {
        self.lift.as_ref()
    }

    pub(crate) fn take_lift(&mut self, tick: u64) -> Option<Lift> {
        let mut l = self.lift.take()?;
        l.release(tick);
        Some(l)
    }
}
