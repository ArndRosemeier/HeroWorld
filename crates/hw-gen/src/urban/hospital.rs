//! The Aether Mercy Hospital: where the medical drone flies a beaten hero.
//!
//! A long streamline hall with round ends (cream enamel, teal bands, ribbon
//! windows, a glass vault down the middle of its roof), a round ward tower
//! behind it crowned with the drone deck and a pylon carrying the glowing
//! ring-and-cross, and a boomerang canopy over the open entrance. In front
//! of the canopy a landing ring marks the drone bay.
//!
//! The ground floor is open to walk in: one Recovery Hall with the
//! reception desk in the middle, a row of upright Aether recovery pods
//! along the back wall (a hero is healed floating in one, then steps out),
//! a waiting corner at the left end and examination couches at the right.
//!
//! Every town has one, a few hundred metres from the core; a big city gets
//! another for each [`PEOPLE_PER_HOSPITAL`] people, out at its business
//! districts and swallowed villages. The drone takes a hero to the nearest.
//!
//! All numbers are in the hospital's own frame: origin on the plinth's top
//! in its middle, x to the right, z the way it faces (out of the entrance).

use super::footprint::Footprint;
use super::growth::CentreKind;
use super::landmarks::{best_spot, landmark_at, Landmark, LandmarkKind};
use super::roadgrid::RoadGrid;
use super::world::TownWorld;
use glam::{DVec2, DVec3};

/// The hall: half the length of its straight sides, the radius of its round
/// ends, the middle of its plan (local z), its walls' thickness, the top
/// of the walls (the parapet's top), and the roof slab's underside and top.
pub const HALL_A: f64 = 10.0;
pub const HALL_R: f64 = 7.0;
pub const HALL_Z: f64 = -6.0;
pub const WALL_T: f64 = 0.3;
pub const PARAPET: f64 = 7.0;
pub const CEILING: f64 = 5.9;
pub const ROOF: f64 = 6.5;
/// The entrance: half its width and its height.
pub const DOOR_HALF: f64 = 2.0;
pub const DOOR_H: f64 = 3.4;
/// The glass vault over the middle of the hall: half its length (x), its
/// span (local z from, to) and its crown above the roof.
pub const VAULT_A: f64 = 7.0;
pub const VAULT_Z: (f64, f64) = (-9.6, -2.4);
pub const VAULT_RISE: f64 = 1.2;
/// The ward tower behind the hall: its middle (local z), radius and top,
/// the drone deck's radius and thickness on top of it, and the pylon's
/// top.
pub const TOWER_Z: f64 = -17.9;
pub const TOWER_R: f64 = 5.0;
pub const TOWER_TOP: f64 = 22.0;
pub const DECK_R: f64 = 6.6;
pub const DECK_T: f64 = 1.2;
pub const PYLON_TOP: f64 = 35.0;
/// The entrance canopy: half its width, how far it reaches out from the
/// front wall, its underside, and where its two columns stand (local x
/// and z).
pub const CANOPY_HALF: f64 = 3.6;
pub const CANOPY_REACH: f64 = 6.5;
pub const CANOPY_Y: f64 = 3.9;
pub const CANOPY_COLS: [(f64, f64); 2] = [(-2.9, 6.6), (2.9, 6.6)];
/// The drone bay's landing ring on the plaza (local x, z) and radius.
pub const BAY: (f64, f64) = (-10.5, 10.5);
pub const BAY_R: f64 = 2.6;
/// The recovery pods along the back wall: their local x, the line of their
/// middles (local z), their glass's radius and height.
pub const PODS_X: [f64; 6] = [-8.6, -5.8, -3.0, 3.0, 5.8, 8.6];
pub const POD_Z: f64 = HALL_Z - HALL_R + WALL_T + 0.95;
pub const POD_R: f64 = 0.8;
pub const POD_H: f64 = 2.7;
/// Where a hero floats in a pod (feet above the floor).
pub const POD_LIFT: f64 = 0.35;
/// How far in front of its pod a healed hero steps out.
pub const POD_STEP: f64 = 1.7;
/// The healing core on the back wall between the pods: half its width,
/// how far it stands out from the wall, and its top.
pub const CORE_HALF: f64 = 1.5;
pub const CORE_DEPTH: f64 = 0.5;
pub const CORE_TOP: f64 = 4.3;
/// The tower's two fins: how far out from its middle (local x) their
/// outer edges reach, their thickness, and their foot and top.
pub const FIN_OUT: f64 = 6.8;
pub const FIN_T: f64 = 0.36;
pub const FIN_Y: (f64, f64) = (7.4, 28.0);
/// The reception desk: its middle (local z), half width and depth, height.
pub const DESK_Z: f64 = -3.6;
pub const DESK_HALF: (f64, f64) = (2.6, 0.45);
pub const DESK_H: f64 = 1.1;
/// The examination couches in the right end: their middles (local x, z)
/// and the way their heads point (radians from local -z towards +x).
pub const COUCHES: [(f64, f64, f64); 3] =
    [(13.0, -9.0, 0.6), (14.6, -6.0, 1.57), (13.0, -3.0, 2.5)];
pub const COUCH_HALF: (f64, f64) = (0.45, 1.05);
pub const COUCH_H: f64 = 0.8;
/// The plaza's radius, largest first.
pub const PLAZAS: [f64; 2] = [26.0, 24.5];
/// A city gets one more hospital for each this many people (up to
/// [`MAX_HOSPITALS`]).
pub const PEOPLE_PER_HOSPITAL: f64 = 300_000.0;
pub const MAX_HOSPITALS: usize = 4;
/// Hospitals stand at least this far apart (m).
pub const HOSPITALS_APART: f64 = 1200.0;

/// The hall's two round ends' middles (local).
fn ends() -> [DVec2; 2] {
    [DVec2::new(-HALL_A, HALL_Z), DVec2::new(HALL_A, HALL_Z)]
}

/// How far a local point is inside the hall's inner face (negative:
/// outside or in the wall).
pub fn inside_depth(p: DVec2) -> f64 {
    let x = p.x.clamp(-HALL_A, HALL_A);
    let d = p.distance(DVec2::new(x, HALL_Z));
    HALL_R - WALL_T - d
}

/// The hall's solid parts, the tower, the canopy and the furniture that
/// blocks the way, in the hospital's own frame.
pub fn footprints() -> Vec<Footprint> {
    let mut v = Vec::new();
    let wall = |a: DVec2, b: DVec2, out: DVec2, y1: f64| Footprint::Box {
        at: (a + b) * 0.5 - out * (WALL_T * 0.5),
        dir: out,
        half: DVec2::new(a.distance(b) * 0.5 + 0.06, WALL_T * 0.5),
        y0: 0.0,
        y1,
    };
    let front = HALL_Z + HALL_R;
    let back = HALL_Z - HALL_R;
    // The front wall either side of the entrance, the lintel over it, and
    // the back wall.
    for s in [-1.0, 1.0] {
        v.push(wall(
            DVec2::new(s * HALL_A, front),
            DVec2::new(s * DOOR_HALF, front),
            DVec2::Y,
            PARAPET,
        ));
    }
    v.push(Footprint::Box {
        at: DVec2::new(0.0, front - WALL_T * 0.5),
        dir: DVec2::Y,
        half: DVec2::new(DOOR_HALF + 0.06, WALL_T * 0.5),
        y0: DOOR_H,
        y1: PARAPET,
    });
    v.push(wall(
        DVec2::new(-HALL_A, back),
        DVec2::new(HALL_A, back),
        -DVec2::Y,
        PARAPET,
    ));
    // The round ends, a short straight piece of wall at a time.
    let n = 12;
    for (k, c) in ends().into_iter().enumerate() {
        let s = if k == 0 { -1.0 } else { 1.0 };
        for i in 0..n {
            let t0 = -std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * i as f64 / n as f64;
            let t1 = t0 + std::f64::consts::PI / n as f64;
            let at = |t: f64| c + DVec2::new(s * t.cos(), t.sin()) * HALL_R;
            let mid = (t0 + t1) * 0.5;
            let out = DVec2::new(s * mid.cos(), mid.sin());
            v.push(wall(at(t0), at(t1), out, PARAPET));
        }
    }
    // The roof slab: the straight part and the round ends.
    v.push(Footprint::Box {
        at: DVec2::new(0.0, HALL_Z),
        dir: DVec2::Y,
        half: DVec2::new(HALL_A, HALL_R - 0.05),
        y0: CEILING,
        y1: ROOF,
    });
    for c in ends() {
        v.push(Footprint::Disc {
            at: c,
            r: HALL_R - 0.05,
            y0: CEILING,
            y1: ROOF,
        });
    }
    // The glass vault: its low sides, then its crown.
    let (z0, z1) = VAULT_Z;
    for (shrink, rise) in [(0.0, VAULT_RISE * 0.55), (0.28, VAULT_RISE)] {
        let span = (z1 - z0) * (1.0 - shrink * 2.0);
        v.push(Footprint::Box {
            at: DVec2::new(0.0, (z0 + z1) * 0.5),
            dir: DVec2::Y,
            half: DVec2::new(VAULT_A, span * 0.5),
            y0: ROOF,
            y1: ROOF + rise,
        });
    }
    // The tower, its deck and the pylon.
    let tower = DVec2::new(0.0, TOWER_Z);
    v.push(Footprint::Disc {
        at: tower,
        r: TOWER_R,
        y0: 0.0,
        y1: TOWER_TOP,
    });
    v.push(Footprint::Disc {
        at: tower,
        r: DECK_R,
        y0: TOWER_TOP,
        y1: TOWER_TOP + DECK_T,
    });
    v.push(Footprint::Disc {
        at: tower,
        r: 0.7,
        y0: TOWER_TOP + DECK_T,
        y1: PYLON_TOP,
    });
    // The canopy and its columns.
    for (x, z) in CANOPY_COLS {
        v.push(Footprint::Disc {
            at: DVec2::new(x, z),
            r: 0.2,
            y0: 0.0,
            y1: CANOPY_Y,
        });
    }
    v.push(Footprint::Box {
        at: DVec2::new(0.0, front + CANOPY_REACH * 0.5),
        dir: DVec2::Y,
        half: DVec2::new(CANOPY_HALF, CANOPY_REACH * 0.5 + 0.2),
        y0: CANOPY_Y,
        y1: CANOPY_Y + 0.3,
    });
    // The fins either side of the tower, from its wall out.
    for sx in [-1.0, 1.0] {
        v.push(Footprint::Box {
            at: DVec2::new(sx * (TOWER_R + FIN_OUT) * 0.5, TOWER_Z),
            dir: DVec2::Y,
            half: DVec2::new((FIN_OUT - TOWER_R) * 0.5 + 0.1, FIN_T * 0.5),
            y0: FIN_Y.0,
            y1: FIN_Y.1,
        });
    }
    // Inside: the pods, the healing core, the desk and the couches.
    let back = HALL_Z - HALL_R + WALL_T;
    v.push(Footprint::Box {
        at: DVec2::new(0.0, back + CORE_DEPTH * 0.5),
        dir: DVec2::Y,
        half: DVec2::new(CORE_HALF, CORE_DEPTH * 0.5),
        y0: 0.0,
        y1: CORE_TOP,
    });
    for x in PODS_X {
        v.push(Footprint::Disc {
            at: DVec2::new(x, POD_Z),
            r: POD_R,
            y0: 0.0,
            y1: POD_H,
        });
    }
    v.push(Footprint::Box {
        at: DVec2::new(0.0, DESK_Z),
        dir: DVec2::Y,
        half: DVec2::new(DESK_HALF.0, DESK_HALF.1),
        y0: 0.0,
        y1: DESK_H,
    });
    for (x, z, a) in COUCHES {
        v.push(Footprint::Box {
            at: DVec2::new(x, z),
            dir: couch_dir(a),
            half: DVec2::new(COUCH_HALF.0, COUCH_HALF.1),
            y0: 0.0,
            y1: COUCH_H,
        });
    }
    v
}

/// The way a couch's head points (local, unit) from its angle.
pub fn couch_dir(a: f64) -> DVec2 {
    DVec2::new(hw_core::det::sin(a), -hw_core::det::cos(a))
}

/// Where the drone sets a hero down and where they are healed, in the
/// town's frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Ward {
    /// The middle of the drone bay's ring, on the plaza.
    pub bay: DVec3,
    /// Each pod: where a hero floats in it (feet), where they step out
    /// to, and the way they face (out of the pod).
    pub pods: Vec<(DVec3, DVec3, DVec2)>,
    /// The entrance, on the plaza in front of the doorway.
    pub door: DVec3,
}

/// The hospital's ward, if `l` is a hospital.
pub fn ward(l: &Landmark) -> Option<Ward> {
    if l.kind != LandmarkKind::Hospital {
        return None;
    }
    let at = |x: f64, z: f64, y: f64| {
        let p = l.to_plan(DVec2::new(x, z));
        DVec3::new(p.x, l.base + y, p.y)
    };
    Some(Ward {
        bay: at(BAY.0, BAY.1, 0.0),
        pods: PODS_X
            .iter()
            .map(|&x| {
                (
                    at(x, POD_Z, POD_LIFT),
                    at(x, POD_Z + POD_STEP + POD_R * 0.5, 0.0),
                    l.dir,
                )
            })
            .collect(),
        door: at(0.0, HALL_Z + HALL_R + 1.0, 0.0),
    })
}

/// The hall's ceiling where `p` (town frame) is inside a hospital's hall,
/// below its roof; None elsewhere.
pub fn ceiling_over(l: &Landmark, p: DVec3) -> Option<f64> {
    if l.kind != LandmarkKind::Hospital || p.y > l.base + CEILING || p.y < l.base - 1.0 {
        return None;
    }
    let d = DVec2::new(p.x, p.z) - l.pos;
    let x = DVec2::new(l.dir.y, -l.dir.x);
    let local = DVec2::new(d.dot(x), d.dot(l.dir));
    (inside_depth(local) > 0.0).then_some(l.base + CEILING)
}

/// How many hospitals a town of `pop` people has.
pub fn count(pop: f64) -> usize {
    (1 + (pop / PEOPLE_PER_HOSPITAL) as usize).min(MAX_HOSPITALS)
}

/// Picks the hospitals' places: the first a few hundred metres from the
/// core, the others at the business districts and villages farthest from
/// it, each clear of the other landmarks and well apart.
pub(super) fn plan(w: &TownWorld, core: DVec2, others: &[Landmark]) -> Vec<Landmark> {
    let mut out: Vec<Landmark> = Vec::new();
    let want = count(w.town.params.population);
    let mut centres: Vec<DVec2> = vec![core];
    let mut far: Vec<(f64, DVec2)> = w
        .town
        .growth
        .centres
        .iter()
        .filter(|c| matches!(c.kind, CentreKind::Business | CentreKind::Village))
        .map(|c| (c.pos.distance(core), c.pos))
        .collect();
    far.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.x.total_cmp(&b.1.x)));
    centres.extend(far.into_iter().map(|(_, p)| p));
    for (k, centre) in centres.into_iter().enumerate() {
        if out.len() >= want {
            break;
        }
        if out.iter().any(|h| h.pos.distance(centre) < HOSPITALS_APART) {
            continue;
        }
        // The first one, off the core's busiest blocks; the others near
        // their district's heart.
        let (reach, ideal) = if k == 0 { (750.0, 320.0) } else { (450.0, 0.0) };
        let near = w.roads_in(
            centre - DVec2::splat(reach + 60.0),
            centre + DVec2::splat(reach + 60.0),
        );
        let roads = RoadGrid::new(&near, 0.0);
        for plaza in PLAZAS {
            let placed: Vec<&Landmark> = others.iter().chain(out.iter()).collect();
            let spot = best_spot(w, &roads, centre, reach, 8.0, plaza, 3.0, |p| {
                if w.town
                    .dystopia
                    .as_ref()
                    .is_some_and(|d| d.distance(p) < 60.0)
                    || placed
                        .iter()
                        .any(|o| o.pos.distance(p) < o.plaza + plaza + 30.0)
                {
                    return f64::NEG_INFINITY;
                }
                -(p.distance(centre) - ideal).abs()
            });
            if let Some((p, _)) = spot {
                // The entrance faces the nearest street.
                let dir = (0..32)
                    .filter_map(|i| {
                        let d = hw_core::det::dir2(std::f64::consts::TAU * i as f64 / 32.0);
                        (0..30)
                            .map(|j| plaza + 1.0 + j as f64 * 3.0)
                            .find(|t| roads.touches(p + d * *t, 1.0, None))
                            .map(|t| (d, t))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map_or((core - p).normalize_or(DVec2::Y), |(d, _)| d);
                out.push(Landmark {
                    kind: LandmarkKind::Hospital,
                    height: PYLON_TOP,
                    dir,
                    ..landmark_at(w, p, plaza)
                });
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::urban::params::TownParams;
    use hw_core::Seed;

    fn local(l: &Landmark, x: f64, z: f64) -> DVec2 {
        l.to_plan(DVec2::new(x, z))
    }

    #[test]
    fn the_hall_is_open_inside_and_its_pods_are_reachable() {
        let l = Landmark {
            kind: LandmarkKind::Hospital,
            pos: DVec2::new(100.0, -40.0),
            plaza: PLAZAS[0],
            base: 3.0,
            low: 2.0,
            height: PYLON_TOP,
            dir: DVec2::new(0.6, 0.8),
            stairs: Vec::new(),
            variant: 0,
        };
        let fps = l.footprints();
        let blocked = |p: DVec2, y: f64| {
            fps.iter().any(|f| {
                let (y0, y1) = f.span();
                y >= y0 && y <= y1 && f.distance(p) < 0.35
            })
        };
        let y = l.base + 1.0;
        // A walk in through the doorway, along the hall to every pod's
        // step-out spot, and to the desk's front: nothing in the way.
        let w = ward(&l).expect("a ward");
        let door = DVec2::new(w.door.x, w.door.z);
        let inside = local(&l, 0.0, -0.8);
        for k in 0..=20 {
            let p = door.lerp(inside, k as f64 / 20.0);
            assert!(!blocked(p, y), "the doorway is shut at {k}");
        }
        let aisle_z = POD_Z + POD_STEP + POD_R * 0.5;
        for (pod, out, _) in &w.pods {
            let o = DVec2::new(out.x, out.z);
            assert!(!blocked(o, y), "a pod's step-out spot is blocked");
            assert!(
                blocked(DVec2::new(pod.x, pod.z), y),
                "a pod's glass is not solid"
            );
            assert!((out.y - l.base).abs() < 1e-9);
            for k in 0..=20 {
                let p = inside.lerp(o, k as f64 / 20.0);
                // Round the desk on the aisle in front of the pods.
                let q = local(&l, 0.0, aisle_z).lerp(o, k as f64 / 20.0);
                assert!(!blocked(q, y) || !blocked(p, y), "no way to a pod");
            }
        }
        // Walls and roof are solid, the hall below the ceiling is not.
        assert!(blocked(
            local(&l, 0.0, HALL_Z + HALL_R - 0.15),
            l.base + DOOR_H + 0.5
        ));
        assert!(blocked(local(&l, -HALL_A - HALL_R + 0.15, HALL_Z), y));
        assert!(blocked(local(&l, 5.0, HALL_Z), l.base + ROOF - 0.1));
        assert!(!blocked(local(&l, -14.0, HALL_Z), y), "the waiting corner");
        assert!(ceiling_over(&l, DVec3::new(inside.x, y, inside.y)).is_some());
        assert!(ceiling_over(&l, w.door).is_none());
        // The bay is open plaza, under the sky.
        assert!(!blocked(DVec2::new(w.bay.x, w.bay.z), l.base + 0.5));
        assert!(!blocked(DVec2::new(w.bay.x, w.bay.z), l.base + 20.0));
        // Everything stands on the plaza.
        for f in &fps {
            assert!(f.at().distance(l.pos) + f.reach() < l.plaza + 0.5);
        }
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn every_town_has_a_hospital_and_big_cities_more() {
        for (s, pop) in [
            (1u64, 40_000.0),
            (3, 8_000.0),
            (7, 40_000.0),
            (11, 700_000.0),
        ] {
            let w = TownWorld::generate(Seed(s), TownParams::sample(Seed(s), pop));
            let hs: Vec<&Landmark> = w
                .landmarks
                .iter()
                .filter(|l| l.kind == LandmarkKind::Hospital)
                .collect();
            assert!(!hs.is_empty(), "seed {s}: no hospital");
            assert!(hs.len() <= count(pop), "seed {s}: too many");
            if pop >= PEOPLE_PER_HOSPITAL * 2.0 {
                assert!(hs.len() >= 2, "seed {s}: one hospital for {pop} people");
            }
            for h in &hs {
                for o in &w.landmarks {
                    if std::ptr::eq(*h, o) {
                        continue;
                    }
                    assert!(
                        o.pos.distance(h.pos) > o.plaza + h.plaza + 10.0,
                        "seed {s}: the hospital crowds the {:?}",
                        o.kind
                    );
                }
                if let Some(d) = &w.town.dystopia {
                    assert!(
                        d.distance(h.pos) >= 60.0,
                        "seed {s}: by the villains' quarter"
                    );
                }
            }
        }
    }
}
