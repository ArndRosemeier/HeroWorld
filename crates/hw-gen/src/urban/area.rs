//! A square of the town with all its detail at hand, for the parts of the
//! game that still need a whole network at once: the people walking, the
//! traffic, the incidents and the taxis.
//!
//! An [`Area`] reads like the town it comes from (it derefs to
//! [`TownWorld`]), with the roads, buildings and props inside its square
//! as plain lists. Roads are cut off at its edge.

use super::props::Prop;
use super::quarter::{DetailId, Quarter, Streets, Unit};
use super::waterfront::{QuayWall, WaterSteps};
use super::world::{TownWorld, WorldBuilding, WorldRoad};
use glam::DVec2;
use hw_core::{Cancel, Cancelled};
use std::collections::BTreeMap;
use std::sync::Arc;

/// The square of the town around the heroes' start that has people and
/// traffic: half its side (m).
pub const AREA_HALF: f64 = 3000.0;

pub struct Area {
    pub world: Arc<TownWorld>,
    pub lo: DVec2,
    pub hi: DVec2,
    /// Every road in the square, cut off at its edge.
    pub roads: Vec<WorldRoad>,
    pub buildings: Vec<WorldBuilding>,
    /// The id of each building (same order).
    pub building_ids: Vec<DetailId>,
    pub props: Vec<Prop>,
    /// The id of each prop (same order).
    pub prop_ids: Vec<DetailId>,
    /// The districts' streets and blocks.
    pub pieces: Vec<Arc<Streets>>,
    pub quays: Vec<QuayWall>,
    pub water_steps: Vec<WaterSteps>,
    prop_at: BTreeMap<DetailId, usize>,
    building_at: BTreeMap<DetailId, usize>,
}

impl std::ops::Deref for Area {
    type Target = TownWorld;
    fn deref(&self) -> &TownWorld {
        &self.world
    }
}

impl Area {
    /// The square of half side `half` round `centre`. Plans its units on
    /// all cores at once.
    pub fn around(world: Arc<TownWorld>, centre: DVec2, half: f64) -> Area {
        Self::around_or_stop(world, centre, half, &Cancel::new()).expect("nobody can cancel this")
    }

    /// [`Area::around`] that gives up between units once `cancel` is set.
    pub fn around_or_stop(
        world: Arc<TownWorld>,
        centre: DVec2,
        half: f64,
        cancel: &Cancel,
    ) -> Result<Area, Cancelled> {
        let lo = centre - DVec2::splat(half);
        let hi = centre + DVec2::splat(half);
        let units = world.units_in(lo, hi);
        cancel.stage("Planning the streets", 0.4, 0.97);
        let quarters = plan_all_or_stop(&world, &units, cancel)?;
        let mut out = Area {
            roads: Vec::new(),
            buildings: Vec::new(),
            building_ids: Vec::new(),
            props: Vec::new(),
            prop_ids: Vec::new(),
            pieces: Vec::new(),
            quays: Vec::new(),
            water_steps: Vec::new(),
            prop_at: BTreeMap::new(),
            building_at: BTreeMap::new(),
            lo,
            hi,
            world: world.clone(),
        };
        let inside = |p: DVec2| p.x >= lo.x && p.y >= lo.y && p.x <= hi.x && p.y <= hi.y;
        for q in &quarters {
            for (id, b) in q.buildings_with_ids() {
                if inside(b.site.origin) {
                    out.buildings.push(b.clone());
                    out.building_ids.push(id);
                }
            }
            for (id, p) in q.props_with_ids() {
                if inside(p.pos) {
                    out.props.push(*p);
                    out.prop_ids.push(id);
                }
            }
            if let Some(s) = &q.streets {
                out.pieces.push(s.clone());
            }
            out.quays.extend(q.quays.iter().cloned());
            out.water_steps.extend(q.water_steps.iter().copied());
        }
        for r in world.roads_in(lo, hi) {
            out.roads.extend(clip(&r, lo, hi));
        }
        out.prop_at = out
            .prop_ids
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect();
        out.building_at = out
            .building_ids
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect();
        Ok(out)
    }

    /// Does the square hold `p`?
    pub fn contains(&self, p: DVec2) -> bool {
        p.x >= self.lo.x && p.y >= self.lo.y && p.x <= self.hi.x && p.y <= self.hi.y
    }

    /// The prop `id`'s place in [`Self::props`].
    pub fn prop_index(&self, id: DetailId) -> Option<usize> {
        self.prop_at.get(&id).copied()
    }

    /// The building `id`'s place in [`Self::buildings`].
    pub fn building_index(&self, id: DetailId) -> Option<usize> {
        self.building_at.get(&id).copied()
    }
}

/// Plans units on every core.
pub fn plan_all(world: &Arc<TownWorld>, units: &[Unit]) -> Vec<Arc<Quarter>> {
    plan_all_or_stop(world, units, &Cancel::new()).expect("nobody can cancel this")
}

/// [`plan_all`] that gives up between units once `cancel` is set.
pub fn plan_all_or_stop(
    world: &Arc<TownWorld>,
    units: &[Unit],
    cancel: &Cancel,
) -> Result<Vec<Arc<Quarter>>, Cancelled> {
    let threads = std::thread::available_parallelism()
        .map_or(2, |n| n.get())
        .clamp(1, 8);
    let chunk = units.len().div_ceil(threads).max(1);
    let done = std::sync::atomic::AtomicUsize::new(0);
    let done = &done;
    std::thread::scope(|s| {
        let jobs: Vec<_> = units
            .chunks(chunk)
            .map(|part| {
                s.spawn(move || {
                    part.iter()
                        .map(|u| {
                            cancel.check()?;
                            let q = world.quarter(*u);
                            // The stage's bar: units finished of all.
                            let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                            cancel.set_fraction(n as f32 / units.len() as f32);
                            Ok(q)
                        })
                        .collect::<Result<Vec<_>, Cancelled>>()
                })
            })
            .collect();
        let mut out = Vec::with_capacity(units.len());
        for j in jobs {
            out.extend(j.join().expect("planning a unit")?);
        }
        Ok(out)
    })
}

/// The runs of a road inside a box (with its width, so a road along the
/// edge stays whole), cut where it leaves.
fn clip(r: &WorldRoad, lo: DVec2, hi: DVec2) -> Vec<WorldRoad> {
    let m = r.half + r.pavement;
    let (lo, hi) = (lo - DVec2::splat(m), hi + DVec2::splat(m));
    let inside = |p: DVec2| p.x >= lo.x && p.y >= lo.y && p.x <= hi.x && p.y <= hi.y;
    if r.pts.iter().all(|p| inside(*p)) {
        return vec![r.clone()];
    }
    // Where segment a-b crosses the box's edge (a inside, b not, or back).
    let cross = |a: DVec2, b: DVec2| {
        let (mut x, mut y) = (a, b);
        let ins = inside(a);
        for _ in 0..30 {
            let mid = (x + y) * 0.5;
            if inside(mid) == ins {
                x = mid;
            } else {
                y = mid;
            }
        }
        if ins {
            x
        } else {
            y
        }
    };
    let mut out = Vec::new();
    let mut run: Vec<DVec2> = Vec::new();
    for (k, &p) in r.pts.iter().enumerate() {
        let here = inside(p);
        if k > 0 {
            let prev = r.pts[k - 1];
            if inside(prev) != here {
                run.push(cross(prev, p));
                if !here && run.len() >= 2 {
                    out.push(WorldRoad {
                        pts: std::mem::take(&mut run),
                        ..r.clone()
                    });
                }
                run.clear();
                if here {
                    run.push(cross(prev, p));
                }
            }
        }
        if here {
            run.push(p);
        }
    }
    if run.len() >= 2 {
        out.push(WorldRoad {
            pts: run,
            ..r.clone()
        });
    }
    out
}
