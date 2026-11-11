//! The Daily Comet's offices: the first building with a game-scale
//! interior from [`crate::layout`].
//!
//! Halls ten metres and more across under ceilings six to eight metres up,
//! a lobby or an atrium several storeys high, galleries, bridges, grand
//! stairs one can fight on, and lifts in real shafts. Every wall, floor,
//! step and rail is a solid part ([`crate::layout::solid`]): the game
//! draws those very boxes, and bodies stand on and stop at them.
//!
//! A town has up to three, one in each scheme (grand hall, atrium, split
//! level), near its core. The landmark's `variant` picks the scheme and
//! the plot's size; the plan itself comes from the place it stands on, so
//! the server and every client make the same one.
//!
//! Local frame: origin on the plinth's top in the middle of the building,
//! x to the right, z the way it faces (out of the entrance). The layout's
//! own frame has its origin at the front left corner as seen from inside
//! looking out, z running inwards: [`to_local`] turns one into the other.

use super::footprint::Footprint;
use super::landmarks::{best_spot, landmark_at, Landmark, LandmarkKind};
use super::roadgrid::RoadGrid;
use super::world::TownWorld;
use crate::layout::furnish::{items, Item};
use crate::layout::office::plan_scheme;
use crate::layout::solid::{pieces, Piece};
use crate::layout::{Layout, Scheme};
use glam::DVec2;
use hw_core::Seed;

/// The plots it is built on, biggest first: width along the front, depth,
/// and storeys.
pub const PLOTS: [(f64, f64, u32); 3] = [(56.0, 52.0, 5), (48.0, 46.0, 5), (40.0, 42.0, 4)];
/// The schemes, in the order a town builds them.
pub const SCHEMES: [Scheme; 3] = [Scheme::Atrium, Scheme::GrandHall, Scheme::SplitLevel];
/// Paving kept clear round the building's corners (m).
const MARGIN: f64 = 4.0;
/// How far apart two of them stand at least (m).
const APART: f64 = 260.0;

/// The plinth a plot needs.
pub fn plaza(plot: usize) -> f64 {
    let (w, d, _) = PLOTS[plot];
    DVec2::new(w, d).length() * 0.5 + MARGIN
}

/// The landmark's variant for a scheme on a plot.
pub fn variant(scheme: usize, plot: usize) -> u8 {
    (scheme + SCHEMES.len() * plot) as u8
}

/// What the town calls it (taxi stands, the map).
pub fn name(l: &Landmark) -> &'static str {
    match SCHEMES[l.variant as usize % SCHEMES.len()] {
        Scheme::Atrium => "Daily Comet building",
        Scheme::GrandHall => "Atlas Assurance hall",
        Scheme::SplitLevel => "Zenith Radio house",
    }
}

/// The building's plan (in the layout's own frame).
pub fn layout(l: &Landmark) -> Option<Layout> {
    if l.kind != LandmarkKind::CometBuilding {
        return None;
    }
    let scheme = SCHEMES[l.variant as usize % SCHEMES.len()];
    let (w, d, storeys) = PLOTS[(l.variant as usize / SCHEMES.len()).min(PLOTS.len() - 1)];
    let size = DVec2::new(w, d);
    // The seed is the place it stands on: the same on every machine.
    let seed = Seed(l.pos.x.to_bits())
        .child(l.pos.y.to_bits())
        .child("comet");
    (0..16u64).find_map(|k| plan_scheme(seed.child(k), size, storeys, scheme))
}

/// A layout's part in the landmark's local frame.
pub fn to_local(size: DVec2, f: Footprint) -> Footprint {
    f.placed(size * 0.5, DVec2::new(0.0, -1.0), 0.0)
}

/// A point of the layout (x, z) in the landmark's local frame.
pub fn point_to_local(size: DVec2, p: DVec2) -> DVec2 {
    size * 0.5 - p
}

/// The building's solid parts in the landmark's local frame.
pub fn parts(l: &Landmark) -> Vec<Piece> {
    let Some(plan) = layout(l) else {
        return Vec::new();
    };
    pieces(&plan)
        .into_iter()
        .map(|p| Piece {
            part: p.part,
            shape: to_local(plan.size, p.shape),
        })
        .collect()
}

/// The building's furniture in the landmark's local frame.
pub fn furniture(l: &Landmark) -> Vec<Item> {
    let Some(plan) = layout(l) else {
        return Vec::new();
    };
    items(&plan)
        .into_iter()
        .map(|i| Item {
            at: point_to_local(plan.size, i.at),
            face: -i.face,
            ..i
        })
        .collect()
}

/// What stands still: everything but the lifts' landing doors, which
/// open and shut with their cars ([`lifts`]); and the furniture.
pub(super) fn footprints(l: &Landmark) -> Vec<Footprint> {
    parts(l)
        .into_iter()
        .filter(|p| p.part != crate::layout::solid::Part::LiftDoor)
        .map(|p| p.shape)
        .chain(furniture(l).iter().filter_map(Item::footprint))
        .collect()
}

/// The building's lifts in the town's frame.
pub fn lifts(l: &Landmark) -> Vec<crate::layout::lifts::LiftRun> {
    let Some(plan) = layout(l) else {
        return Vec::new();
    };
    // The same two steps as a part: the layout's frame to the landmark's,
    // the landmark's to the town's.
    crate::layout::lifts::runs(&plan)
        .iter()
        .map(|r| {
            r.placed(plan.size * 0.5, DVec2::new(0.0, -1.0), 0.0)
                .placed(l.pos, l.dir, l.base)
        })
        .collect()
}

/// Picks their places: near the core, clear of the other landmarks, each
/// facing its nearest street. A town too tight for the biggest plot gets
/// a smaller one; one with no room at all gets none.
pub(super) fn plan(w: &TownWorld, core: DVec2, others: &[Landmark]) -> Vec<Landmark> {
    let mut out: Vec<Landmark> = Vec::new();
    let reach = (w.half() * 0.8).min(900.0);
    let near = w.roads_in(
        core - DVec2::splat(reach + 120.0),
        core + DVec2::splat(reach + 120.0),
    );
    let roads = RoadGrid::new(&near, 0.0);
    for scheme in 0..SCHEMES.len() {
        for plot in 0..PLOTS.len() {
            let plaza = plaza(plot);
            let placed: Vec<&Landmark> = others.iter().chain(out.iter()).collect();
            let spot = best_spot(w, &roads, core, reach, 8.0, plaza, 3.0, |p| {
                if w.town
                    .dystopia
                    .as_ref()
                    .is_some_and(|d| d.distance(p) < 60.0)
                    || placed
                        .iter()
                        // The weather tower keeps its sky to itself.
                        .any(|o| {
                            let clear = if o.kind == LandmarkKind::WeatherTower {
                                170.0
                            } else {
                                30.0
                            };
                            o.pos.distance(p) < o.plaza + plaza + clear
                        })
                    || out.iter().any(|o| o.pos.distance(p) < APART)
                {
                    return f64::NEG_INFINITY;
                }
                -(p.distance(core) - 180.0).abs()
            });
            let Some((p, _)) = spot else { continue };
            // The entrance faces the nearest street, square to the town's
            // axes or its diagonals so the block reads as built, not dropped.
            let dir = (0..8)
                .filter_map(|i| {
                    let d = hw_core::det::dir2(std::f64::consts::TAU * i as f64 / 8.0);
                    (0..30)
                        .map(|j| plaza + 1.0 + j as f64 * 3.0)
                        .find(|t| roads.touches(p + d * *t, 1.0, None))
                        .map(|t| (d, t))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map_or((core - p).normalize_or(DVec2::Y), |(d, _)| d);
            let mut l = Landmark {
                kind: LandmarkKind::CometBuilding,
                dir,
                variant: variant(scheme, plot),
                ..landmark_at(w, p, plaza)
            };
            if let Some(plan) = layout(&l) {
                l.height = plan.height();
                out.push(l);
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::LinkKind;

    fn at(variant: u8, pos: DVec2) -> Landmark {
        Landmark {
            kind: LandmarkKind::CometBuilding,
            pos,
            plaza: 40.0,
            base: 3.0,
            low: 2.0,
            height: 30.0,
            dir: DVec2::new(0.6, 0.8),
            stairs: Vec::new(),
            variant,
        }
    }

    #[test]
    fn every_scheme_fits_the_big_plot_and_is_sound() {
        let mut plans = 0;
        for v in 0..9u8 {
            for k in 0..6 {
                let l = at(
                    v,
                    DVec2::new(100.0 + 37.0 * k as f64, -40.0 + 11.0 * k as f64),
                );
                // The atrium and the split levels need the big plot; the
                // grand hall fits them all.
                let plan = layout(&l);
                let (scheme, plot) = (v as usize % 3, v as usize / 3);
                assert!(
                    plan.is_some() || (plot > 0 && SCHEMES[scheme] != Scheme::GrandHall),
                    "no plan for variant {v}"
                );
                let Some(plan) = plan else { continue };
                assert_eq!(plan.scheme, SCHEMES[scheme]);
                assert_eq!(plan.problems(), Vec::<String>::new());
                assert_eq!(Some(plan), layout(&l), "same place, same plan");
                plans += 1;
            }
        }
        assert!(plans >= 30, "{plans} plans");
    }

    #[test]
    fn the_building_stands_on_its_plinth_facing_out() {
        for plot in 0..PLOTS.len() {
            let mut l = at(variant(1, plot), DVec2::new(10.0, 20.0));
            l.plaza = plaza(plot);
            let plan = layout(&l).expect("a plan");
            for f in l.footprints() {
                assert!(
                    f.at().distance(l.pos) + f.reach().min(3.0) < l.plaza + 3.0,
                    "a part off the plinth"
                );
            }
            // The entrance is on the side it faces.
            let door = plan
                .links
                .iter()
                .find(|k| k.kind == LinkKind::Entrance)
                .expect("an entrance");
            let p = l.to_plan(point_to_local(plan.size, door.at));
            assert!((p - l.pos).dot(l.dir) > plan.size.y * 0.5 - 0.01);
        }
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn a_town_has_its_comet_buildings() {
        use crate::urban::params::TownParams;
        for seed in [1u64, 2, 3] {
            let w = TownWorld::generate(Seed(seed), TownParams::sample(Seed(seed), 20_000.0));
            let n = w
                .landmarks
                .iter()
                .filter(|l| l.kind == LandmarkKind::CometBuilding)
                .count();
            assert!(n >= 1, "seed {seed}: no Comet building");
        }
    }
}
