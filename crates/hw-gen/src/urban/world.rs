//! A whole town made ready for the game: ground heights (river beds and the
//! sea floor below the water), every road with its width, every building
//! with the height it stands at.
//!
//! The town lies in its own local frame: plan (x, y) is ground (x, z), and
//! heights are metres above the water. All water stands at
//! [`WATER_LEVEL`]; the land rises from it.

use super::graph::signed_area;
use super::growth::CentreKind;
use super::land::{distance_to, Grid};
use super::landmarks::{plan_landmarks, Landmark};
use super::plots::{plan_buildings, road_reach_at, side_reach};
use super::props::{plan_props, Prop};
use super::roadgrid::RoadGrid;
use super::roads::RoadClass;
use super::streets::{plan_piece, PieceStreets, StreetStyle};
use super::underground::{plan_underground, Underground};
use super::waterfront::{plan_waterfront, QuayWall, WaterSteps};
use super::{Town, TownParams};
use crate::building::BuildingSite;
use glam::DVec2;
use hw_core::Seed;

/// Height of every water surface (sea, lakes, rivers).
pub const WATER_LEVEL: f64 = 0.0;
/// How far a road surface stands above the ground under it.
pub const ROAD_LIFT: f64 = 0.06;
/// Bridge decks and roads by the water never dip below this.
pub const DECK: f64 = 1.0;

/// A road ready to draw: centre line, half the carriageway, and the
/// pavement on each side.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldRoad {
    pub pts: Vec<DVec2>,
    pub half: f64,
    pub pavement: f64,
    /// Main road (true) or side street.
    pub main: bool,
    /// A main road's class.
    pub class: Option<RoadClass>,
    /// A side street's style.
    pub style: Option<StreetStyle>,
}

/// Half the width of a moving car, with its mirrors (m).
pub const CAR_HALF: f64 = 1.1;
/// How far in from the kerb parked cars reach (m): the widest parked
/// car, from its place 1.15 m in, with a hand's breadth to spare.
pub const PARKED_ROOM: f64 = 2.4;
/// Between a moving car and anything beside it (m).
const CAR_SPARE: f64 = 0.15;
/// Half the room a person walking takes (m).
pub const WALKER_HALF: f64 = 0.3;

impl WorldRoad {
    /// Whether cars drive here: a lane each way, each wide enough for a car.
    pub fn has_traffic(&self) -> bool {
        self.pts.len() >= 2 && self.half >= 2.0 * (CAR_HALF + CAR_SPARE) + 0.2
    }

    /// Whether cars may park along both kerbs and still leave a lane each
    /// way beside them.
    pub fn parking_fits(&self) -> bool {
        self.half - PARKED_ROOM - CAR_SPARE >= 2.0 * CAR_HALF + 2.0 * CAR_SPARE
    }

    /// The middle of the lane each way, right of the centre line (m): half
    /// way to the kerb, or clear of the parked cars where there are any.
    pub fn lane_offset(&self) -> f64 {
        let free = if self.parking_fits() {
            self.half - PARKED_ROOM - CAR_SPARE
        } else {
            self.half
        };
        (self.half * 0.5)
            .min(free - CAR_HALF - CAR_SPARE)
            .max(CAR_HALF + CAR_SPARE)
    }

    /// The line people walk along the pavement, from the centre line (m):
    /// a little nearer the houses than the kerb, where the street lamps,
    /// trees and hydrants stand.
    pub fn walk_offset(&self) -> f64 {
        self.half + self.pavement * 0.55
    }

    /// Where along the kerb lamps, hydrants and tree pits stand, from the
    /// kerb (m).
    pub fn kerb_spot(&self) -> f64 {
        (self.pavement * 0.2).min(0.45)
    }
}

/// A building on the ground.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldBuilding {
    pub site: BuildingSite,
    /// Height of its ground floor.
    pub base: f64,
    /// Lowest ground under it: a plinth fills the gap on a slope.
    pub low: f64,
}

/// A bridge deck: centre line ends, half width, deck height.
#[derive(Clone, Debug, PartialEq)]
pub struct Bridge {
    pub a: DVec2,
    pub b: DVec2,
    pub half: f64,
    pub deck: f64,
}

/// The town, ready for the game.
pub struct TownWorld {
    pub seed: Seed,
    pub town: Town,
    pub pieces: Vec<PieceStreets>,
    pub roads: Vec<WorldRoad>,
    pub buildings: Vec<WorldBuilding>,
    pub bridges: Vec<Bridge>,
    /// Street furniture, trees and parked cars.
    pub props: Vec<Prop>,
    /// The Aether Spire and, by the sea, a lighthouse.
    pub landmarks: Vec<Landmark>,
    /// Stone walls where streets meet the water.
    pub quays: Vec<QuayWall>,
    /// Steps from the quays down to the water.
    pub water_steps: Vec<WaterSteps>,
    /// Utility tunnels, lifts and Molehaven, below the town.
    pub underground: Underground,
    /// Ground heights, river beds and sea floor included.
    terrain: Grid<f32>,
}

impl TownWorld {
    pub fn generate(seed: Seed, params: TownParams) -> TownWorld {
        let town = Town::generate(seed, params);
        let land = &town.land;
        // Water beds: deeper away from the shore.
        let to_land = distance_to(&land.cover, |c| !c.is_water());
        let mut terrain = land.height.clone();
        for j in 0..terrain.h {
            for i in 0..terrain.w {
                if land.cover.at(i, j).is_water() {
                    let d = *to_land.at(i, j) as f64;
                    let depth = 1.5 + (d * 0.12).min(12.0);
                    terrain.set(i, j, (WATER_LEVEL - depth) as f32);
                }
            }
        }
        let pieces: Vec<PieceStreets> = town
            .pieces
            .iter()
            .map(|pc| plan_piece(seed, &town, pc))
            .collect();
        let mut out = TownWorld {
            seed,
            town,
            pieces,
            roads: Vec::new(),
            buildings: Vec::new(),
            bridges: Vec::new(),
            props: Vec::new(),
            landmarks: Vec::new(),
            quays: Vec::new(),
            water_steps: Vec::new(),
            underground: Underground::default(),
            terrain,
        };
        out.roads = out.make_roads();
        out.bridges = out.make_bridges();
        out.landmarks = plan_landmarks(&out);
        (out.quays, out.water_steps) = plan_waterfront(&out);
        let mut buildings = Vec::new();
        let roads = RoadGrid::new(&out, 0.0);
        for (i, (pc, streets)) in out.town.pieces.iter().zip(&out.pieces).enumerate() {
            for site in plan_buildings(seed, &out.town, i, pc, streets) {
                // Never on a street, not even a neighbouring piece's (a
                // metre's grace on its pavement, as round a bend).
                if on_a_street(&roads, &site) {
                    continue;
                }
                let corners = site.corners();
                let hs: Vec<f64> = corners.iter().map(|c| out.ground(*c)).collect();
                let hi = hs.iter().copied().fold(f64::MIN, f64::max);
                let lo = hs.iter().copied().fold(f64::MAX, f64::min);
                // A building never stands in the water.
                if lo < WATER_LEVEL + 0.3 {
                    continue;
                }
                // Nor on a landmark's plinth.
                if out.landmarks.iter().any(|l| l.overlaps(&site, 1.5)) {
                    continue;
                }
                buildings.push(WorldBuilding {
                    site,
                    base: hi + 0.1,
                    low: lo,
                });
            }
        }
        out.buildings = buildings;
        out.props = plan_props(&out);
        out.underground = plan_underground(&out);
        out
    }

    /// Ground height at a point (below [`WATER_LEVEL`] under water).
    pub fn ground(&self, p: DVec2) -> f64 {
        self.terrain.sample(p)
    }

    /// Height of a road surface at a point: the ground, lifted a little,
    /// and never below a deck over the water.
    pub fn road_height(&self, p: DVec2) -> f64 {
        (self.ground(p) + ROAD_LIFT).max(DECK + ROAD_LIFT)
    }

    /// Does the town meet the sea?
    pub fn coastal(&self) -> bool {
        !self.town.land.sea.is_empty()
    }

    /// Half the side of the planned land around the town.
    pub fn half(&self) -> f64 {
        self.terrain.half()
    }

    /// Where heroes start: on a street near the town's core, and the way
    /// along it.
    pub fn spawn_line(&self) -> (DVec2, DVec2) {
        let core = self
            .town
            .growth
            .centres
            .iter()
            .find(|c| c.kind == CentreKind::Core)
            .map_or(DVec2::ZERO, |c| c.pos);
        let mut best: Option<(f64, DVec2, DVec2)> = None;
        for r in self.roads.iter().filter(|r| !r.main) {
            for w in r.pts.windows(2) {
                let m = (w[0] + w[1]) * 0.5;
                if w[0].distance(w[1]) < 4.0 || self.ground(m) < DECK {
                    continue;
                }
                let d = m.distance(core);
                if best.is_none_or(|b| d < b.0) {
                    best = Some((d, m, (w[1] - w[0]).normalize()));
                }
            }
        }
        best.map_or((core, DVec2::X), |b| (b.1, b.2))
    }

    fn make_roads(&self) -> Vec<WorldRoad> {
        let mut out = Vec::new();
        for r in &self.town.network.roads {
            // In an old town even a main road is narrow: split the road into
            // runs of one width.
            let reach_at = |p: DVec2| road_reach_at(&self.town, r.class, p);
            let mut run: Vec<DVec2> = vec![r.pts[0]];
            let mut cur = reach_at(r.pts[0]);
            for w in r.pts.windows(2) {
                let here = reach_at((w[0] + w[1]) * 0.5);
                if (here - cur).abs() > 1e-9 && run.len() >= 2 {
                    out.push(road(std::mem::take(&mut run), cur, Some(r.class), None));
                    run.push(w[0]);
                }
                cur = here;
                run.push(w[1]);
            }
            if run.len() >= 2 {
                out.push(road(run, cur, Some(r.class), None));
            }
        }
        for p in &self.pieces {
            let reach = side_reach(p.style);
            for s in &p.streets {
                if s.len() >= 2 {
                    out.push(road(s.clone(), reach, None, Some(p.style)));
                }
            }
        }
        out
    }

    fn make_bridges(&self) -> Vec<Bridge> {
        // Every stretch of road over water is a bridge.
        let mut out = Vec::new();
        for r in &self.roads {
            let wet = |p: DVec2| self.ground(p) < DECK;
            let mut start: Option<DVec2> = None;
            let step = 2.0;
            let mut last = r.pts[0];
            for w in r.pts.windows(2) {
                let l = w[0].distance(w[1]);
                let n = (l / step).ceil().max(1.0) as usize;
                for k in 0..=n {
                    let p = w[0].lerp(w[1], k as f64 / n as f64);
                    match (start, wet(p)) {
                        (None, true) => start = Some(last),
                        (Some(a), false) => {
                            if a.distance(p) > 1.0 {
                                out.push(Bridge {
                                    a,
                                    b: p,
                                    half: r.half + r.pavement,
                                    deck: DECK + ROAD_LIFT,
                                });
                            }
                            start = None;
                        }
                        _ => {}
                    }
                    last = p;
                }
            }
        }
        out
    }

    /// Total footprint of the town's blocks, for statistics.
    pub fn block_area(&self) -> f64 {
        self.pieces
            .iter()
            .flat_map(|p| p.blocks.iter())
            .map(|b| signed_area(b))
            .sum()
    }
}

/// Does a building's outline come onto any street's carriageway or more
/// than a metre into its pavement?
fn on_a_street(roads: &RoadGrid, site: &BuildingSite) -> bool {
    let c = site.corners();
    (0..4).any(|k| {
        let (a, b) = (c[k], c[(k + 1) % 4]);
        let n = (a.distance(b) / 0.8).ceil().max(1.0) as usize;
        (0..n).any(|j| roads.touches(a.lerp(b, j as f64 / n as f64), -1.0, None))
    })
}

fn road(
    pts: Vec<DVec2>,
    reach: f64,
    class: Option<RoadClass>,
    style: Option<StreetStyle>,
) -> WorldRoad {
    // Pavements: wide on grand roads, a strip in old lanes.
    let pavement = match class {
        Some(RoadClass::Boulevard) => (reach * 0.3).min(5.0),
        Some(RoadClass::Country) => (reach * 0.2).min(1.5),
        _ => (reach * 0.3).clamp(1.2, 4.0),
    };
    WorldRoad {
        pts,
        half: (reach - pavement).max(2.0),
        pavement,
        main: class.is_some(),
        class,
        style,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buildings_stand_on_dry_ground_and_roads_above_water() {
        let w = TownWorld::generate(Seed(3), TownParams::sample(Seed(3), 20_000.0));
        assert!(w.buildings.len() > 500, "{}", w.buildings.len());
        for b in &w.buildings {
            assert!(b.base >= b.low && b.low > WATER_LEVEL);
            assert!(b.base - b.low < 25.0, "plinth {}", b.base - b.low);
        }
        for r in &w.roads {
            for p in &r.pts {
                assert!(w.road_height(*p) > WATER_LEVEL);
            }
        }
        let (p, _) = w.spawn_line();
        assert!(w.ground(p) > WATER_LEVEL);
    }

    /// No building stands on a carriageway, dead ends and other pieces'
    /// streets included (a fifth of them did before 0.065, and cars drove
    /// through them), checked against the drawn width of every road.
    #[test]
    fn no_building_stands_on_a_street() {
        let w = TownWorld::generate(Seed(1), TownParams::sample(Seed(1), 40_000.0));
        let boxes: Vec<(DVec2, DVec2)> = w
            .roads
            .iter()
            .map(|r| {
                let lo = r.pts.iter().fold(DVec2::MAX, |m, p| m.min(*p));
                let hi = r.pts.iter().fold(DVec2::MIN, |m, p| m.max(*p));
                (lo - DVec2::splat(r.half), hi + DVec2::splat(r.half))
            })
            .collect();
        for b in &w.buildings {
            let c = b.site.corners();
            for k in 0..4 {
                for j in 0..=10 {
                    let q = c[k].lerp(c[(k + 1) % 4], j as f64 / 10.0);
                    let near = |i: &usize| {
                        let (lo, hi) = boxes[*i];
                        q.x >= lo.x && q.y >= lo.y && q.x <= hi.x && q.y <= hi.y
                    };
                    for r in (0..w.roads.len()).filter(near).map(|i| &w.roads[i]) {
                        let d = r
                            .pts
                            .windows(2)
                            .map(|s| crate::water::segment_distance(q, s[0], s[1]))
                            .fold(f64::MAX, f64::min);
                        assert!(d >= r.half, "a building on a street at {q:?}");
                    }
                }
            }
        }
    }
}
