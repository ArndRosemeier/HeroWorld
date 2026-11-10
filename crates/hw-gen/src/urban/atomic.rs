//! The atomic plant: a white reactor dome, two cooling towers breathing
//! steam, a streamlined turbine hall, a striped stack and an atom on a
//! pylon at the gate. It stands out in the country just past the town's
//! edge, beside a main road, by the water if there is any.
//!
//! The layout is fixed (the plinth is always [`PLANT_PLAZA`] across); the
//! client draws it from the same numbers ([`towers`], [`tower_r`], ...) the
//! solid parts here come from.

use super::footprint::{ring, Footprint};
use super::greenery::built;
use super::landmarks::{landmark_at, rim, Landmark, LandmarkKind};
use super::roadgrid::{keys_around, RoadGrid};
use super::world::{TownWorld, WATER_LEVEL};
use crate::water::segment_distance;
use glam::DVec2;

/// Radius of the plant's paved yard.
pub const PLANT_PLAZA: f64 = 38.0;
/// The reactor building: a drum of this radius and wall height under a
/// half-sphere dome, its centre this far forward of the yard's.
pub const REACTOR_R: f64 = 11.0;
pub const REACTOR_WALL: f64 = 9.0;
pub const REACTOR_Z: f64 = 4.0;
/// The glass lantern on the dome's crown: radius and height.
pub const CUPOLA_R: f64 = 2.0;
pub const CUPOLA_H: f64 = 2.2;
/// The porch on the drum's front: half width, depth out from the drum,
/// height, and how far its canopy reaches past it.
pub const PORCH: (f64, f64, f64, f64) = (3.2, 2.6, 4.6, 0.9);

/// Where the porch starts (local z): a little into the drum.
pub fn porch_z() -> f64 {
    REACTOR_Z + REACTOR_R - 0.6
}
/// The cooling towers: height, radius at the foot of the shell, at the
/// waist (where it is narrowest) and at the lip; the shell starts this
/// high, on a ring of slanting legs.
pub const TOWER_H: f64 = 34.0;
pub const TOWER_FOOT: f64 = 9.0;
pub const TOWER_WAIST: f64 = 5.6;
pub const TOWER_LIP: f64 = 6.3;
pub const TOWER_SHELL_Y: f64 = 3.0;
/// Where the waist is, as a share of the height.
pub const TOWER_WAIST_AT: f64 = 0.78;
/// Legs under each tower's shell.
pub const TOWER_LEGS: usize = 24;
/// The turbine hall: centre, half width (x) and half length (z), eaves
/// height and the rise of its barrel roof.
pub const HALL_AT: (f64, f64) = (23.5, 5.0);
pub const HALL_HALF: (f64, f64) = (4.5, 11.0);
pub const HALL_EAVES: f64 = 9.0;
pub const HALL_VAULT: f64 = 2.6;
/// The two steam mains from the reactor to the hall: height of each and
/// where they run (z), and their radius.
pub const MAINS: [(f64, f64); 2] = [(6.2, 7.0), (6.2, 10.0)];
pub const MAIN_R: f64 = 0.55;
/// The ventilation stack: where, radius at the foot and top, height.
pub const STACK_AT: (f64, f64) = (-24.0, 7.0);
pub const STACK_R: (f64, f64) = (1.7, 1.0);
pub const STACK_H: f64 = 50.0;
/// The atom on its pylon by the gate: where, the pylon's height, and the
/// radius of the electrons' orbits round the nucleus on top.
pub const PYLON_AT: (f64, f64) = (-11.0, 26.0);
pub const PYLON_H: f64 = 13.0;
pub const ATOM_R: f64 = 3.2;
/// The gatehouse: a round booth by the way in.
pub const GATEHOUSE_AT: (f64, f64) = (9.5, 27.0);
pub const GATEHOUSE_R: f64 = 2.3;
pub const GATEHOUSE_H: f64 = 3.0;

/// The cooling towers' centres (local frame, +z the front).
pub fn towers() -> [DVec2; 2] {
    [DVec2::new(-14.5, -19.5), DVec2::new(14.5, -20.0)]
}

/// The shell's radius `y` up from the yard (from [`TOWER_SHELL_Y`] to the
/// lip): one sheet of a hyperboloid through the foot and the waist, eased
/// out to the lip above the waist.
pub fn tower_r(y: f64) -> f64 {
    let yw = TOWER_H * TOWER_WAIST_AT;
    if y <= yw {
        // r = waist * sqrt(1 + ((y - yw) / c)^2), through the foot.
        let k = (TOWER_FOOT / TOWER_WAIST).powi(2) - 1.0;
        let c = (yw - TOWER_SHELL_Y) / k.sqrt();
        TOWER_WAIST * (1.0 + ((y - yw) / c).powi(2)).sqrt()
    } else {
        let t = ((y - yw) / (TOWER_H - yw)).min(1.0);
        TOWER_WAIST + (TOWER_LIP - TOWER_WAIST) * t * t
    }
}

/// The dome's radius `y` above the drum's top.
pub fn dome_r(y: f64) -> f64 {
    (REACTOR_R * REACTOR_R - y * y).max(0.0).sqrt()
}

/// The plant's solid parts in its own frame.
pub(super) fn footprints() -> Vec<Footprint> {
    let disc = |at: DVec2, r: f64, y0: f64, y1: f64| Footprint::Disc { at, r, y0, y1 };
    let mut v = Vec::new();
    // The reactor: its drum, then the dome in steps a little inside it.
    let c = DVec2::new(0.0, REACTOR_Z);
    v.push(disc(c, REACTOR_R, 0.0, REACTOR_WALL));
    let steps = 6;
    for k in 0..steps {
        let (a, b) = (
            REACTOR_R * k as f64 / steps as f64,
            REACTOR_R * (k + 1) as f64 / steps as f64,
        );
        v.push(disc(
            c,
            dome_r(b).max(CUPOLA_R),
            REACTOR_WALL + a,
            REACTOR_WALL + b,
        ));
    }
    v.push(disc(
        c,
        CUPOLA_R,
        REACTOR_WALL + REACTOR_R,
        REACTOR_WALL + REACTOR_R + CUPOLA_H,
    ));
    let (hw, depth, h, reach) = PORCH;
    let z = porch_z();
    v.push(Footprint::Box {
        at: DVec2::new(0.0, z + depth * 0.5),
        dir: DVec2::Y,
        half: DVec2::new(hw, depth * 0.5),
        y0: 0.0,
        y1: h + 4.0,
    });
    v.push(Footprint::Box {
        at: DVec2::new(0.0, z + (depth + reach) * 0.5),
        dir: DVec2::Y,
        half: DVec2::new(hw + 0.4, (depth + reach) * 0.5),
        y0: h - 0.35,
        y1: h,
    });
    // The towers: hollow shells (you can walk in between the legs and look
    // up through the steam), as rings of walls a band at a time.
    for t in towers() {
        for k in 0..TOWER_LEGS {
            let a = std::f64::consts::TAU * k as f64 / TOWER_LEGS as f64;
            let at = t + hw_core::det::dir2(a) * (TOWER_FOOT - 0.4);
            v.push(disc(at, 0.4, 0.0, TOWER_SHELL_Y + 0.2));
        }
        let bands = 8;
        for k in 0..bands {
            let y0 = TOWER_SHELL_Y + (TOWER_H - TOWER_SHELL_Y) * k as f64 / bands as f64;
            let y1 = TOWER_SHELL_Y + (TOWER_H - TOWER_SHELL_Y) * (k + 1) as f64 / bands as f64;
            let r = tower_r((y0 + y1) * 0.5) + 0.2;
            for f in ring(r, 0.9, 1.0, 32) {
                if let Footprint::Box { at, dir, half, .. } = f {
                    v.push(Footprint::Box {
                        at: at + t,
                        dir,
                        half,
                        y0,
                        y1,
                    });
                }
            }
        }
    }
    // The turbine hall, up to its roof's crown.
    v.push(Footprint::Box {
        at: DVec2::new(HALL_AT.0, HALL_AT.1),
        dir: DVec2::Y,
        half: DVec2::new(HALL_HALF.0, HALL_HALF.1),
        y0: 0.0,
        y1: HALL_EAVES + HALL_VAULT,
    });
    // The steam mains, across from the drum to the hall.
    let (x0, x1) = (REACTOR_R - 0.6, HALL_AT.0 - HALL_HALF.0);
    for (y, z) in MAINS {
        v.push(Footprint::Box {
            at: DVec2::new((x0 + x1) * 0.5, z),
            dir: DVec2::Y,
            half: DVec2::new((x1 - x0) * 0.5, MAIN_R + 0.1),
            y0: y - MAIN_R - 0.1,
            y1: y + MAIN_R + 0.1,
        });
    }
    // The trestle's two legs under the mains.
    for z in [MAINS[0].1 - 0.9, MAINS[1].1 + 0.9] {
        v.push(disc(DVec2::new((x0 + x1) * 0.5, z), 0.25, 0.0, MAINS[0].0));
    }
    let s = DVec2::new(STACK_AT.0, STACK_AT.1);
    v.push(Footprint::Box {
        at: s,
        dir: DVec2::Y,
        half: DVec2::splat(2.3),
        y0: 0.0,
        y1: 2.0,
    });
    v.push(disc(s, (STACK_R.0 + STACK_R.1) * 0.5, 2.0, STACK_H));
    let p = DVec2::new(PYLON_AT.0, PYLON_AT.1);
    v.push(disc(p, 1.4, 0.0, 1.0));
    v.push(disc(p, 1.0, 1.0, 5.0));
    v.push(disc(p, 0.45, 5.0, PYLON_H));
    v.push(disc(
        p,
        ATOM_R + 0.3,
        PYLON_H + ATOM_R * 0.3,
        PYLON_H + ATOM_R * 1.7,
    ));
    v.push(disc(
        DVec2::new(GATEHOUSE_AT.0, GATEHOUSE_AT.1),
        GATEHOUSE_R,
        0.0,
        GATEHOUSE_H,
    ));
    // Its saucer roof.
    v.push(disc(
        DVec2::new(GATEHOUSE_AT.0, GATEHOUSE_AT.1),
        GATEHOUSE_R + 1.35,
        GATEHOUSE_H,
        GATEHOUSE_H + 1.0,
    ));
    v
}

/// Where the plant stands, if the town has room for it: just past the
/// built-up edge, its yard clear of every road and close beside a main
/// road, on dry and fairly level ground, the nearer the water the better.
/// It faces the road.
pub(super) fn plan(w: &TownWorld, taken: &[Landmark]) -> Option<Landmark> {
    let plaza = PLANT_PLAZA;
    let g = &w.town.growth;
    let land = &w.town.land;
    let roads = RoadGrid::new(&w.main_roads, 0.0);
    // The nearest main road's edge from `p` (searching out to `reach`).
    let road_gap = |p: DVec2, reach: f64| -> Option<(f64, DVec2)> {
        let mut best: Option<(f64, DVec2)> = None;
        for k in keys_around(p, reach) {
            for &(ri, si) in roads.at(k) {
                let r = &roads.roads[ri];
                let (a, b) = (r.pts[si], r.pts[si + 1]);
                let d = segment_distance(p, a, b) - r.half - r.pavement;
                if best.is_none_or(|(e, _)| d < e) {
                    let ab = b - a;
                    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
                    best = Some((d, a + ab * t));
                }
            }
        }
        best
    };
    let time = |p: DVec2| {
        g.time
            .cell_of(p)
            .map_or(f64::INFINITY, |(i, j)| *g.time.at(i, j) as f64)
    };
    let limit = g.limit as f64;
    let half = w.half() - plaza - 40.0;
    let step = 12.0;
    let n = (half / step) as i64;
    let mut found: Vec<(f64, DVec2, DVec2)> = Vec::new();
    for j in -n..=n {
        for i in -n..=n {
            let p = DVec2::new(i as f64, j as f64) * step;
            // A short way out of town: past the edge, no further than a
            // brisk walk.
            let t = time(p);
            if !(t > limit + plaza + 30.0 && t < limit + 650.0) {
                continue;
            }
            let Some((gap, on_road)) = road_gap(p, plaza + 60.0) else {
                continue;
            };
            // Its yard's edge within a few metres of the road, never on it.
            if gap < plaza + 4.0 || gap > plaza + 40.0 {
                continue;
            }
            // Nothing built round it (villages included), not on water.
            let clear = (0..16).all(|k| {
                let a = std::f64::consts::TAU * k as f64 / 16.0;
                let q = p + hw_core::det::dir2(a) * (plaza + 25.0);
                !built(&w.town, q) && !built(&w.town, p + hw_core::det::dir2(a) * plaza * 0.5)
            });
            if !clear || built(&w.town, p) {
                continue;
            }
            // Clear of the other landmarks, and well away from the airfield,
            // where the liner glides in low past the mast.
            let apart = |l: &Landmark| match l.kind {
                LandmarkKind::MooringMast => 450.0,
                _ => l.plaza + plaza + 40.0,
            };
            if taken.iter().any(|l| l.pos.distance(p) < apart(l)) {
                continue;
            }
            // Water close by draws its cooling water.
            let wet = (0..24)
                .filter(|k| {
                    let a = std::f64::consts::TAU * *k as f64 / 24.0;
                    [plaza + 30.0, plaza + 90.0, plaza + 160.0]
                        .iter()
                        .any(|r| land.cover_at(p + hw_core::det::dir2(a) * *r).is_water())
                })
                .count() as f64
                / 24.0;
            let score =
                wet.min(0.5) * 2.0 - (gap - plaza - 10.0).abs() / 40.0 - (t - limit) / 900.0;
            found.push((score, p, on_road));
        }
    }
    // Best first; ties go to the earlier spot, so the same seed always
    // picks the same one.
    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (_, p, on_road) in found.into_iter().take(40) {
        let (lo, hi) = rim(w, p, plaza);
        if lo < WATER_LEVEL + 1.2 || hi - lo > 5.0 {
            continue;
        }
        // Side streets of a village or the town's edge, too.
        let m = DVec2::splat(plaza + 4.0);
        let near = w.roads_in(p - m, p + m);
        if RoadGrid::new(&near, 0.0).touches(p, plaza + 0.8, None) {
            continue;
        }
        return Some(Landmark {
            kind: LandmarkKind::AtomicPlant,
            height: STACK_H,
            dir: (on_road - p).normalize_or(DVec2::Y),
            ..landmark_at(w, p, plaza)
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::urban::params::TownParams;
    use hw_core::Seed;

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn the_layout_fits_its_yard() {
        let all = footprints();
        for f in &all {
            let (lo, hi) = f.extent(DVec2::ZERO, DVec2::X);
            let (zlo, zhi) = f.extent(DVec2::ZERO, DVec2::Y);
            let far = [lo, hi, zlo, zhi]
                .iter()
                .map(|v| v.abs())
                .fold(0.0, f64::max);
            assert!(far < PLANT_PLAZA - 1.5, "{f:?} reaches the edge");
        }
        // The way in from the front, down the middle, stays open.
        for z in [PLANT_PLAZA - 1.0, 30.0, 24.0, porch_z() + PORCH.1 + 2.0] {
            let p = DVec2::new(0.0, z);
            assert!(
                all.iter().all(|f| f.distance(p) > 1.5 || f.span().0 > 2.5),
                "blocked at z {z}"
            );
        }
        // The hyperboloid runs through the foot, waist and lip.
        assert!((tower_r(TOWER_SHELL_Y) - TOWER_FOOT).abs() < 1e-9);
        assert!((tower_r(TOWER_H * TOWER_WAIST_AT) - TOWER_WAIST).abs() < 1e-9);
        assert!((tower_r(TOWER_H) - TOWER_LIP).abs() < 1e-9);
        // Towers clear of the reactor and of each other.
        let [a, b] = towers();
        assert!(a.distance(b) > 2.0 * TOWER_FOOT + 4.0);
        for t in towers() {
            assert!(t.distance(DVec2::new(0.0, REACTOR_Z)) > TOWER_FOOT + REACTOR_R + 2.0);
        }
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn the_plant_stands_out_of_town_by_a_road() {
        let mut planted = 0;
        for s in [1u64, 3, 7, 11] {
            let w = TownWorld::generate(Seed(s), TownParams::sample(Seed(s), 40_000.0));
            let Some(l) = w
                .landmarks
                .iter()
                .find(|l| l.kind == LandmarkKind::AtomicPlant)
            else {
                continue;
            };
            planted += 1;
            assert!(!built(&w.town, l.pos), "seed {s}: in town");
            // Facing a main road close in front of its yard.
            let ahead = l.pos + l.dir * (l.plaza + 3.0);
            let to_road = w
                .main_roads
                .iter()
                .flat_map(|r| {
                    r.pts
                        .windows(2)
                        .map(move |s| segment_distance(ahead, s[0], s[1]) - r.half - r.pavement)
                })
                .fold(f64::INFINITY, f64::min);
            assert!(to_road < 45.0, "seed {s}: {to_road:.0} m from the road");
            assert!(l.footprints().len() > 100);
            for o in &w.landmarks {
                if o.kind == LandmarkKind::MooringMast {
                    assert!(o.pos.distance(l.pos) > 400.0, "seed {s}: by the airfield");
                } else if o.kind != LandmarkKind::AtomicPlant {
                    assert!(o.pos.distance(l.pos) > o.plaza + l.plaza);
                }
            }
        }
        assert!(planted >= 3, "only {planted} towns of 4 have a plant");
    }
}
