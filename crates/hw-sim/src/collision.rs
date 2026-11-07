//! What heroes bump into: the city's buildings as stacks of boxes (one per
//! run of floors with the same outline, so towers keep their setbacks).
//!
//! Server and client use the same code and the same city, so a predicted
//! hero lands exactly where the server's does.

use glam::{DVec2, DVec3};
use hw_gen::building::BuildingPlan;
use hw_gen::city::CityPlan;
use std::collections::BTreeMap;

/// Ground grid cell for lookups (m).
const CELL: f64 = 32.0;

/// One solid box: a rectangle in a building's frame, from `y0` to `y1`.
#[derive(Clone, Debug, PartialEq)]
pub struct Solid {
    pub origin: DVec2,
    pub along: DVec2,
    pub inward: DVec2,
    /// Rectangle in building coordinates (x along, z inward).
    pub min: DVec2,
    pub max: DVec2,
    pub y0: f64,
    pub y1: f64,
}

impl Solid {
    fn to_local(&self, p: DVec2) -> DVec2 {
        let d = p - self.origin;
        DVec2::new(d.dot(self.along), d.dot(self.inward))
    }

    fn to_world(&self, l: DVec2) -> DVec2 {
        self.origin + self.along * l.x + self.inward * l.y
    }

    fn corners(&self) -> [DVec2; 4] {
        [
            self.to_world(self.min),
            self.to_world(DVec2::new(self.max.x, self.min.y)),
            self.to_world(self.max),
            self.to_world(DVec2::new(self.min.x, self.max.y)),
        ]
    }
}

/// All solids of a city with a grid to find nearby ones fast.
#[derive(Clone, Debug, Default)]
pub struct Obstacles {
    pub solids: Vec<Solid>,
    grid: BTreeMap<(i64, i64), Vec<u32>>,
    /// Height of open ground.
    pub ground: f64,
}

/// A body that moves through the city: feet position, velocity.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Body {
    pub pos: DVec3,
    pub vel: DVec3,
    pub grounded: bool,
}

/// Radius and height of a person.
pub const BODY_RADIUS: f64 = 0.4;
pub const BODY_HEIGHT: f64 = 1.8;
/// Ledges lower than this are stepped onto.
pub const STEP: f64 = 0.45;

impl Obstacles {
    /// Open ground only.
    pub fn flat(ground: f64) -> Self {
        Self {
            ground,
            ..Default::default()
        }
    }

    /// Every building of a city, from its plans (one per `city.buildings`).
    pub fn from_city(city: &CityPlan, plans: &[BuildingPlan], ground: f64, base: f64) -> Self {
        let mut out = Self::flat(ground);
        for (_, p) in city.buildings.iter().zip(plans) {
            let s = &p.site;
            let mut i = 0;
            while i < p.floors.len() {
                let outline = p.floors[i].outline;
                let mut j = i;
                while j + 1 < p.floors.len() && p.floors[j + 1].outline == outline {
                    j += 1;
                }
                out.add(Solid {
                    origin: s.origin,
                    along: s.along,
                    inward: s.inward,
                    min: outline.min,
                    max: outline.max,
                    y0: base + p.floors[i].elevation,
                    // The roof slab on top.
                    y1: base + p.floors[j].elevation + p.floors[j].height + 0.3,
                });
                i = j + 1;
            }
        }
        out
    }

    pub fn add(&mut self, solid: Solid) {
        let idx = self.solids.len() as u32;
        let (mut lo, mut hi) = (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN));
        for c in solid.corners() {
            lo = lo.min(c);
            hi = hi.max(c);
        }
        let (x0, z0) = ((lo.x / CELL).floor() as i64, (lo.y / CELL).floor() as i64);
        let (x1, z1) = ((hi.x / CELL).floor() as i64, (hi.y / CELL).floor() as i64);
        for x in x0..=x1 {
            for z in z0..=z1 {
                self.grid.entry((x, z)).or_default().push(idx);
            }
        }
        self.solids.push(solid);
    }

    /// Solids whose grid cells touch a circle (may repeat none; sorted).
    fn near(&self, p: DVec2, r: f64) -> Vec<u32> {
        let mut out = Vec::new();
        let (x0, z0) = (
            ((p.x - r) / CELL).floor() as i64,
            ((p.y - r) / CELL).floor() as i64,
        );
        let (x1, z1) = (
            ((p.x + r) / CELL).floor() as i64,
            ((p.y + r) / CELL).floor() as i64,
        );
        for x in x0..=x1 {
            for z in z0..=z1 {
                if let Some(v) = self.grid.get(&(x, z)) {
                    out.extend_from_slice(v);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Height of whatever is under `p` (x, z) no higher than `max_y`.
    pub fn floor_under(&self, p: DVec2, max_y: f64) -> f64 {
        let mut best = self.ground;
        for i in self.near(p, 0.0) {
            let s = &self.solids[i as usize];
            let l = s.to_local(p);
            if l.x >= s.min.x && l.x <= s.max.x && l.y >= s.min.y && l.y <= s.max.y && s.y1 <= max_y
            {
                best = best.max(s.y1);
            }
        }
        best
    }

    /// Pushes a body standing at `pos` out of every wall it overlaps; returns
    /// the corrected position and the outward normals of walls it touched.
    fn push_out(&self, mut pos: DVec3) -> (DVec3, Vec<DVec2>) {
        let mut normals = Vec::new();
        // A few passes settle corners between neighbouring boxes.
        for _ in 0..3 {
            let mut moved = false;
            for i in self.near(DVec2::new(pos.x, pos.z), BODY_RADIUS) {
                let s = &self.solids[i as usize];
                // Only walls: the body overlaps the box vertically and is not
                // standing on top of it (or stepping up onto it).
                if pos.y + BODY_HEIGHT <= s.y0 || pos.y >= s.y1 - STEP {
                    continue;
                }
                let l = s.to_local(DVec2::new(pos.x, pos.z));
                let closest = l.clamp(s.min, s.max);
                let d = l - closest;
                let dist = d.length();
                let push_local = if dist > 1e-9 {
                    if dist >= BODY_RADIUS {
                        continue;
                    }
                    d / dist * (BODY_RADIUS - dist)
                } else {
                    // Centre inside the box: leave by the nearest side.
                    let exits = [
                        (l.x - s.min.x, DVec2::new(-1.0, 0.0)),
                        (s.max.x - l.x, DVec2::new(1.0, 0.0)),
                        (l.y - s.min.y, DVec2::new(0.0, -1.0)),
                        (s.max.y - l.y, DVec2::new(0.0, 1.0)),
                    ];
                    let (depth, dir) = exits
                        .iter()
                        .copied()
                        .min_by(|a, b| a.0.total_cmp(&b.0))
                        .expect("four sides");
                    dir * (depth + BODY_RADIUS)
                };
                let push = s.along * push_local.x + s.inward * push_local.y;
                pos.x += push.x;
                pos.z += push.y;
                normals.push(push.normalize_or_zero());
                moved = true;
            }
            if !moved {
                break;
            }
        }
        (pos, normals)
    }

    /// Moves a body by its velocity for `dt` seconds: walls stop it, roofs
    /// and ledges catch it, gravity is the caller's business.
    pub fn move_body(&self, body: &mut Body, dt: f64) {
        // Sub-steps keep fast bodies from passing through thin walls.
        let travel = body.vel.length() * dt;
        let steps = ((travel / (BODY_RADIUS * 0.8)).ceil() as usize).clamp(1, 32);
        let h = dt / steps as f64;
        body.grounded = false;
        for _ in 0..steps {
            let mut next = body.pos + body.vel * h;
            let (pushed, normals) = self.push_out(next);
            next = pushed;
            for n in normals {
                let n3 = DVec3::new(n.x, 0.0, n.y);
                let into = body.vel.dot(n3);
                if into < 0.0 {
                    body.vel -= n3 * into;
                }
            }
            // Land on whatever is below, stepping up small ledges.
            let floor = self.floor_under(DVec2::new(next.x, next.z), next.y.max(body.pos.y) + STEP);
            if next.y <= floor {
                next.y = floor;
                body.vel.y = body.vel.y.max(0.0);
                body.grounded = true;
            }
            body.pos = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_box() -> Obstacles {
        let mut o = Obstacles::flat(0.0);
        o.add(Solid {
            origin: DVec2::ZERO,
            along: DVec2::X,
            inward: DVec2::Y,
            min: DVec2::new(0.0, 0.0),
            max: DVec2::new(10.0, 10.0),
            y0: 0.0,
            y1: 12.0,
        });
        o
    }

    #[test]
    fn walls_stop_a_runner() {
        let o = one_box();
        let mut b = Body {
            pos: DVec3::new(-3.0, 0.0, 5.0),
            vel: DVec3::new(7.0, 0.0, 0.0),
            grounded: true,
        };
        for _ in 0..60 {
            o.move_body(&mut b, 1.0 / 30.0);
        }
        assert!(b.pos.x <= -BODY_RADIUS + 1e-6, "went through: {}", b.pos);
        assert!(b.vel.x.abs() < 1e-9);
    }

    #[test]
    fn sliding_along_a_wall_keeps_the_sideways_part() {
        let o = one_box();
        let mut b = Body {
            pos: DVec3::new(-1.0, 0.0, 2.0),
            vel: DVec3::new(5.0, 0.0, 3.0),
            grounded: true,
        };
        for _ in 0..10 {
            o.move_body(&mut b, 1.0 / 30.0);
        }
        assert!(b.pos.z > 2.5, "no slide: {}", b.pos);
    }

    #[test]
    fn falling_onto_a_roof_lands_on_it() {
        let o = one_box();
        let mut b = Body {
            pos: DVec3::new(5.0, 20.0, 5.0),
            vel: DVec3::new(0.0, -10.0, 0.0),
            grounded: false,
        };
        for _ in 0..60 {
            o.move_body(&mut b, 1.0 / 30.0);
        }
        assert_eq!(b.pos.y, 12.0);
        assert!(b.grounded);
    }

    #[test]
    fn a_fast_body_does_not_tunnel() {
        let mut o = Obstacles::flat(0.0);
        o.add(Solid {
            origin: DVec2::ZERO,
            along: DVec2::X,
            inward: DVec2::Y,
            min: DVec2::new(0.0, -50.0),
            max: DVec2::new(0.3, 50.0),
            y0: 0.0,
            y1: 30.0,
        });
        let mut b = Body {
            pos: DVec3::new(-2.0, 1.0, 0.0),
            vel: DVec3::new(300.0, 0.0, 0.0),
            grounded: false,
        };
        o.move_body(&mut b, 1.0 / 30.0);
        assert!(b.pos.x < 0.0, "tunnelled to {}", b.pos);
    }
}
