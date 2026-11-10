//! The rocket port at the edge of town: a finned rocket standing on its
//! launch stand beside a red lattice gantry, a domed blockhouse facing the
//! pad, two fuel spheres on legs and floodlight masts round a paved round
//! pad. Now and then the rocket counts down and goes up in fire and smoke,
//! and later comes back down on its tail (the schedule is the server's,
//! `hw_sim::rocket`).
//!
//! Everything here is in the pad's own frame: origin on the plinth's top at
//! the rocket's axis, +z the way it faces (towards the town's core), +x its
//! right. [`super::landmarks::Landmark::to_plan`] takes it to the town.

use super::footprint::{disc, Footprint};
use super::landmarks::{Landmark, LandmarkKind};
use super::roadgrid::RoadGrid;
use super::world::{TownWorld, WATER_LEVEL};
use glam::DVec2;

/// The pad's plinth radius, biggest first: the first that fits is taken.
pub const GANTRY_PLAZAS: [f64; 3] = [26.0, 23.0, 21.0];

/// The launch stand: a round concrete pedestal under the rocket, with the
/// flame duct's mouth opening on its -x side.
pub const STAND_R: f64 = 4.4;
pub const STAND_H: f64 = 3.0;
/// The rocket: from its nozzle's lip on the stand to the tip of its nose,
/// its body's radius, and how far out its four fins reach (they are its
/// legs: their feet stand on the stand at this radius).
pub const ROCKET_H: f64 = 30.0;
pub const ROCKET_R: f64 = 2.0;
pub const FIN_REACH: f64 = 4.1;
/// Where the fins stand round the body (degrees from +z): between them, on
/// +x, the gantry's arms reach the hull.
pub const FIN_ANGLES: [f64; 4] = [45.0, 135.0, 225.0, 315.0];
/// The service gantry: a square lattice tower beside the rocket, its
/// middle this far out on +x, this wide (half), this tall.
pub const TOWER_X: f64 = 8.2;
pub const TOWER_HALF: f64 = 2.2;
pub const TOWER_H: f64 = 40.0;
/// The swing arms: where each turns on the tower's face (local x, z), at
/// which height, and how long it is (reaching the hull when swung in).
/// The upper one carries the crew's covered walkway to the hatch, the
/// lower one the fuel lines.
pub const ARM_HINGE: DVec2 = DVec2::new(TOWER_X - TOWER_HALF, -1.6);
pub const ARM_HEIGHTS: [f64; 2] = [11.0, 20.0];
pub const ARM_LEN: f64 = TOWER_X - TOWER_HALF - ROCKET_R + 0.15;
/// The arms' deck: half its width and its depth.
pub const ARM_HALF_W: f64 = 0.8;
pub const ARM_DEPTH: f64 = 2.2;
/// The flame duct's concrete hood on the stand's -x side: how far out it
/// reaches, half its width and its height. Fire and smoke pour out of its
/// mouth while the engine burns on the stand.
pub const DUCT_X: f64 = -7.0;
pub const DUCT_HALF: f64 = 2.1;
pub const DUCT_H: f64 = 2.6;
/// The blockhouse: a low concrete dome on the pad's town side, slit
/// windows towards the rocket, its door towards the town.
pub const BUNKER_Z: f64 = 14.0;
pub const BUNKER_R: f64 = 4.6;
pub const BUNKER_H: f64 = 3.9;
/// Fuel spheres on legs, behind the rocket: where (local x, z), radius, and
/// the height of their middles.
pub const TANKS: [(f64, f64); 2] = [(-12.5, -8.5), (-6.0, -14.5)];
pub const TANK_R: f64 = 2.7;
pub const TANK_Y: f64 = 4.2;
/// Floodlight masts round the pad (degrees from +z), their height, and how
/// far in from the plinth's edge they stand.
pub const MAST_ANGLES: [f64; 4] = [60.0, 150.0, 210.0, 300.0];
pub const MAST_H: f64 = 18.0;
pub const MAST_IN: f64 = 1.6;
/// The countdown board on the tower's town face: its middle's height and
/// its half width and half height.
pub const BOARD_Y: f64 = TOWER_H - 6.5;
pub const BOARD_HALF: (f64, f64) = (2.6, 1.5);

// The fins stand on the stand; the rocket is shorter than its gantry,
// the board on the tower.
const _: () = assert!(FIN_REACH < STAND_R);
const _: () = assert!(ROCKET_H + STAND_H < TOWER_H);
const _: () = assert!(BOARD_Y + BOARD_HALF.1 < TOWER_H);

/// The pad's middle (local): where the rocket stands.
pub fn rocket_foot() -> f64 {
    STAND_H
}

/// Where the floodlight masts stand (local), on a plinth of radius `plaza`.
pub fn masts(plaza: f64) -> Vec<DVec2> {
    MAST_ANGLES
        .iter()
        .map(|a| hw_core::det::dir2(a.to_radians()) * (plaza - MAST_IN))
        .collect()
}

/// A spot a saboteur would go for (a hook for a mission: plant a charge,
/// cut a line, hold the blockhouse), in the pad's frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weak {
    /// The fuel lines' valves at the gantry's foot.
    Umbilical,
    /// A fuel sphere's filler valve (one per sphere).
    Tank(usize),
    /// The blockhouse door, where the launch is run from.
    Blockhouse,
}

/// The saboteur's spots: what, where (local), at what height over the pad.
pub fn weak_spots() -> Vec<(Weak, DVec2, f64)> {
    let mut v = vec![(
        Weak::Umbilical,
        DVec2::new(TOWER_X - TOWER_HALF - 0.6, 1.0),
        0.9,
    )];
    for (k, (x, z)) in TANKS.iter().enumerate() {
        // Under the sphere, on the side towards the rocket.
        let to = DVec2::new(-x, -z).normalize();
        v.push((Weak::Tank(k), DVec2::new(*x, *z) + to * (TANK_R + 0.7), 1.0));
    }
    v.push((
        Weak::Blockhouse,
        DVec2::new(0.0, BUNKER_Z + BUNKER_R + 1.0),
        1.0,
    ));
    v
}

/// The pad's solid parts (local), the rocket and its arms aside: they move,
/// and are the server's ([`hw_sim::rocket`]).
pub fn footprints(plaza: f64) -> Vec<Footprint> {
    let mut v = vec![
        disc(STAND_R, STAND_H),
        Footprint::Box {
            at: DVec2::new((DUCT_X - STAND_R + 0.6) * 0.5, 0.0),
            dir: DVec2::Y,
            half: DVec2::new((STAND_R - 0.6 + DUCT_X).abs() * 0.5, DUCT_HALF),
            y0: 0.0,
            y1: DUCT_H,
        },
        // The tower, a lattice too fine to slip through.
        Footprint::Box {
            at: DVec2::new(TOWER_X, 0.0),
            dir: DVec2::Y,
            half: DVec2::splat(TOWER_HALF),
            y0: 0.0,
            y1: TOWER_H,
        },
        // The blockhouse's dome: two drums, the upper one narrower.
        Footprint::Disc {
            at: DVec2::new(0.0, BUNKER_Z),
            r: BUNKER_R,
            y0: 0.0,
            y1: BUNKER_H * 0.55,
        },
        // Its door's hooded porch.
        Footprint::Box {
            at: DVec2::new(0.0, BUNKER_Z + BUNKER_R - 0.2),
            dir: DVec2::Y,
            half: DVec2::new(1.5, 0.75),
            y0: 0.0,
            y1: 2.6,
        },
        Footprint::Disc {
            at: DVec2::new(0.0, BUNKER_Z),
            r: BUNKER_R * 0.72,
            y0: 0.0,
            y1: BUNKER_H,
        },
    ];
    for (x, z) in TANKS {
        v.push(Footprint::Disc {
            at: DVec2::new(x, z),
            r: TANK_R,
            y0: 0.0,
            y1: TANK_Y + TANK_R,
        });
    }
    for at in masts(plaza) {
        v.push(Footprint::Disc {
            at,
            r: 0.35,
            y0: 0.0,
            y1: MAST_H,
        });
    }
    v
}

/// Picks the rocket port's place: just outside the town's built edge, on
/// dry, fairly level ground clear of every road and building, away from
/// the villain quarter and the other landmarks. `None` when no spot fits
/// (a town hemmed in by water and hills).
pub fn plan_gantry(w: &TownWorld, others: &[Landmark]) -> Option<Landmark> {
    let core = w.core();
    let half = w.half();
    let dirs = 48;
    let mut candidates: Vec<(f64, DVec2)> = Vec::new();
    for k in 0..dirs {
        let d = hw_core::det::dir2(std::f64::consts::TAU * k as f64 / dirs as f64);
        // Out from the core to where the town's pieces end (a gap of 90 m
        // with nothing built is the edge).
        let (mut s, mut edge) = (0.0, 0.0);
        while s < half {
            if w.in_a_piece(core + d * s) {
                edge = s;
            } else if s - edge > 90.0 {
                break;
            }
            s += 15.0;
        }
        let out = edge + GANTRY_PLAZAS[0] + 45.0;
        let c = core + d * out;
        if c.x.abs() > half * 0.9 || c.y.abs() > half * 0.9 {
            continue;
        }
        let (lo, hi) = super::landmarks::rim(w, c, GANTRY_PLAZAS[0] + 10.0);
        if lo < WATER_LEVEL + 1.5 {
            continue;
        }
        // Level ground first; nearer the town a little better.
        candidates.push((-(hi - lo) * 3.0 - out * 0.01, c));
    }
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (_, c) in candidates.into_iter().take(8) {
        let reach = 80.0;
        let box_r = DVec2::splat(reach + GANTRY_PLAZAS[0] + 40.0);
        let near = w.roads_in(c - box_r, c + box_r);
        let roads = RoadGrid::new(&near, 0.0);
        for plaza in GANTRY_PLAZAS {
            let clear = |p: DVec2| {
                let in_quarter = w.town.dystopia.as_ref().is_some_and(|d| {
                    d.contains(p) || p.distance(d.centre) < d.radius + plaza + 60.0
                });
                // Well clear of the others; of the airfield's mast by more,
                // for its moored liner lies out over the fields.
                let crowded = others.iter().any(|l| {
                    let room = if l.kind == LandmarkKind::MooringMast {
                        200.0
                    } else {
                        60.0
                    };
                    p.distance(l.pos) < l.plaza + plaza + room
                });
                // No building may stand on it: keep off the town's pieces.
                let built = w.in_a_piece(p)
                    || (0..12).any(|k| {
                        let a = std::f64::consts::TAU * k as f64 / 12.0;
                        w.in_a_piece(p + hw_core::det::dir2(a) * (plaza + 4.0))
                    });
                !(in_quarter || crowded || built)
            };
            let spot = super::landmarks::best_spot(w, &roads, c, reach, 5.0, plaza, 3.0, |p| {
                if clear(p) {
                    -p.distance(core) * 0.01
                } else {
                    f64::NEG_INFINITY
                }
            });
            if let Some((p, _)) = spot {
                let mut l = Landmark {
                    kind: LandmarkKind::RocketGantry,
                    height: TOWER_H,
                    dir: (core - p).normalize_or(DVec2::Y),
                    ..super::landmarks::landmark_at(w, p, plaza)
                };
                l.stairs = super::landmarks::plan_stairs(w, &roads, &l);
                return Some(l);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn the_pad_fits_on_its_plinth() {
        for plaza in GANTRY_PLAZAS {
            for f in footprints(plaza) {
                assert!(
                    f.at().length() + f.reach() < plaza - 0.3,
                    "{f:?} off a {plaza} m pad"
                );
            }
            for (_, at, _) in weak_spots() {
                assert!(at.length() < plaza - 0.5);
                // A saboteur can stand there: outside every solid part.
                for f in footprints(plaza) {
                    assert!(f.distance(at) > 0.2, "{at} inside {f:?}");
                }
            }
        }
        // The arms reach from the tower's face to the hull.
        assert!((ARM_HINGE.x - ARM_LEN - ROCKET_R).abs() < 0.2);
    }
}

#[cfg(test)]
mod sizes {
    use crate::urban::landmarks::LandmarkKind;
    use crate::urban::params::TownParams;
    use crate::urban::world::TownWorld;
    use hw_core::Seed;

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn a_rocket_port_at_any_size() {
        // A hamlet, a market town and a big city: each gets its pad out
        // past its edge.
        for (s, pop) in [(2u64, 1_500.0), (5, 12_000.0), (9, 400_000.0)] {
            let w = TownWorld::generate(Seed(s), TownParams::sample(Seed(s), pop));
            let pad = w
                .landmarks
                .iter()
                .find(|l| l.kind == LandmarkKind::RocketGantry)
                .unwrap_or_else(|| panic!("no rocket port at {pop}"));
            assert!(!w.in_a_piece(pad.pos), "{pop}: the pad in town");
            for l in &w.landmarks {
                if l.kind != LandmarkKind::RocketGantry {
                    assert!(l.pos.distance(pad.pos) > l.plaza + pad.plaza + 50.0);
                }
            }
        }
    }
}
