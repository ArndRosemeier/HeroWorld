//! The city plan: districts, streets, blocks, lots and building sites.
//!
//! Layers, from coarse to fine, each seeded from its own path:
//!
//! 1. **Districts:** the civic centre in the middle and a ring of districts
//!    around it (commercial, residential, the Aether Works, the expo grounds
//!    and one dystopian quarter run by the mad scientist).
//! 2. **Boulevards:** a jittered, rotated lattice. Its quads are the city's
//!    *cells* (convex, about 330 m across).
//! 3. **Streets and blocks:** each cell is cut recursively across its long
//!    axis, in the rhythm of its district (tight grid downtown, big blocks at
//!    the works). Cuts become streets; what is left, minus streets and
//!    sidewalks, becomes blocks.
//! 4. **Lots:** blocks are split into lots that all front a street.
//! 5. **Building sites:** a rectangle per lot, set back from the street by
//!    district rules, with a use and a height. The building itself is planned
//!    later, on demand, by [`crate::building`].
//!
//! Coordinates are the city's local frame on the ground (x east, z south),
//! in metres, centred on the civic centre.

use crate::building::{BuildingSite, BuildingUse};
use crate::geom::{EdgeTag, Poly};
use glam::DVec2;
use hw_core::{det, Rng, Seed, StableId};
use std::collections::BTreeMap;

/// A boulevard lattice node (i, j).
type Node = (i64, i64);

/// Settings chosen when a world starts.
#[derive(Clone, Debug, PartialEq)]
pub struct CityParams {
    /// Radius of the built-up area (m).
    pub radius: f64,
    /// Distance between boulevards (m).
    pub boulevard_spacing: f64,
}

impl Default for CityParams {
    fn default() -> Self {
        Self {
            radius: 1300.0,
            boulevard_spacing: 330.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DistrictKind {
    Civic,
    Commercial,
    Residential,
    AetherWorks,
    Expo,
    /// The mad scientist's quarter: the dystopia.
    Dystopia,
}

impl DistrictKind {
    pub const ALL: [DistrictKind; 6] = [
        Self::Civic,
        Self::Commercial,
        Self::Residential,
        Self::AetherWorks,
        Self::Expo,
        Self::Dystopia,
    ];

    fn rules(self) -> &'static Rules {
        match self {
            Self::Civic => &CIVIC,
            Self::Commercial => &COMMERCIAL,
            Self::Residential => &RESIDENTIAL,
            Self::AetherWorks => &WORKS,
            Self::Expo => &EXPO,
            Self::Dystopia => &DYSTOPIA,
        }
    }
}

/// How a district builds.
struct Rules {
    /// Target block size (short, long) in m.
    block: (f64, f64),
    /// How much cuts wander off the straight grid (0 = none).
    wander: f64,
    /// Local street class (per cell, from the district at its centre).
    street: StreetClass,
    /// Lot frontage range.
    lot_width: (f64, f64),
    /// Building depth range.
    depth: (f64, f64),
    /// Setback from the sidewalk and from the neighbours.
    setback_front: f64,
    setback_side: f64,
    floors: (u32, u32),
    /// Chance a block is a plaza or a park instead of lots.
    plaza: f64,
    park: f64,
}

const CIVIC: Rules = Rules {
    block: (85.0, 130.0),
    wander: 0.0,
    street: StreetClass::Avenue,
    lot_width: (34.0, 56.0),
    depth: (28.0, 44.0),
    setback_front: 3.0,
    setback_side: 3.0,
    floors: (8, 32),
    plaza: 0.12,
    park: 0.0,
};
const COMMERCIAL: Rules = Rules {
    block: (70.0, 120.0),
    wander: 0.05,
    street: StreetClass::Street,
    lot_width: (16.0, 28.0),
    depth: (18.0, 28.0),
    setback_front: 0.5,
    setback_side: 0.6,
    floors: (3, 8),
    plaza: 0.04,
    park: 0.0,
};
const RESIDENTIAL: Rules = Rules {
    block: (75.0, 150.0),
    wander: 0.12,
    street: StreetClass::Street,
    lot_width: (18.0, 30.0),
    depth: (16.0, 24.0),
    setback_front: 4.0,
    setback_side: 2.0,
    floors: (4, 12),
    plaza: 0.0,
    park: 0.1,
};
const WORKS: Rules = Rules {
    block: (120.0, 200.0),
    wander: 0.03,
    street: StreetClass::Street,
    lot_width: (40.0, 70.0),
    depth: (30.0, 55.0),
    setback_front: 5.0,
    setback_side: 4.0,
    floors: (1, 3),
    plaza: 0.0,
    park: 0.0,
};
const EXPO: Rules = Rules {
    block: (110.0, 170.0),
    wander: 0.08,
    street: StreetClass::Avenue,
    lot_width: (40.0, 64.0),
    depth: (34.0, 56.0),
    setback_front: 8.0,
    setback_side: 6.0,
    floors: (1, 2),
    plaza: 0.25,
    park: 0.1,
};
const DYSTOPIA: Rules = Rules {
    block: (60.0, 110.0),
    wander: 0.2,
    street: StreetClass::Lane,
    lot_width: (13.0, 20.0),
    depth: (14.0, 22.0),
    setback_front: 0.5,
    setback_side: 0.5,
    floors: (3, 7),
    plaza: 0.0,
    park: 0.0,
};

#[derive(Clone, Debug, PartialEq)]
pub struct District {
    pub id: StableId,
    pub kind: DistrictKind,
    pub center: DVec2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StreetClass {
    /// Lattice boulevards with a planted median.
    Boulevard,
    Avenue,
    Street,
    Lane,
}

impl StreetClass {
    /// Carriageway width and sidewalk width (each side), roomier than real life.
    pub fn widths(self) -> (f64, f64) {
        match self {
            Self::Boulevard => (22.0, 6.0),
            Self::Avenue => (16.0, 5.0),
            Self::Street => (12.0, 4.0),
            Self::Lane => (8.0, 2.5),
        }
    }
}

/// A straight street segment between two points (x, z).
#[derive(Clone, Debug, PartialEq)]
pub struct Street {
    pub class: StreetClass,
    pub a: DVec2,
    pub b: DVec2,
    /// Carriageway width.
    pub width: f64,
    /// Sidewalk width on each side.
    pub sidewalk: f64,
}

impl Street {
    /// Distance from the centre line to the edge of the sidewalk.
    pub fn reach(&self) -> f64 {
        self.width * 0.5 + self.sidewalk
    }

    pub fn distance_to(&self, p: DVec2) -> f64 {
        let ab = self.b - self.a;
        let t = ((p - self.a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
        p.distance(self.a + ab * t)
    }
}

/// One face of the boulevard lattice.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub id: StableId,
    pub district: usize,
    pub poly: Poly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlockUse {
    Lots,
    Plaza,
    Park,
}

/// The land between streets (behind the sidewalks).
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub id: StableId,
    pub cell: usize,
    pub district: usize,
    pub usage: BlockUse,
    pub poly: Poly,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Lot {
    pub id: StableId,
    pub block: usize,
    pub poly: Poly,
    /// The edge of `poly` on the street the lot fronts.
    pub front: usize,
    /// That street.
    pub street: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CityPlan {
    pub seed: Seed,
    pub params: CityParams,
    pub districts: Vec<District>,
    pub streets: Vec<Street>,
    pub cells: Vec<Cell>,
    pub blocks: Vec<Block>,
    pub lots: Vec<Lot>,
    /// One per lot that holds a building; `lot` index in `lots`.
    pub buildings: Vec<(usize, BuildingSite)>,
}

impl CityPlan {
    pub fn generate(seed: Seed, params: &CityParams) -> CityPlan {
        let mut plan = CityPlan {
            seed,
            params: params.clone(),
            districts: Vec::new(),
            streets: Vec::new(),
            cells: Vec::new(),
            blocks: Vec::new(),
            lots: Vec::new(),
            buildings: Vec::new(),
        };
        plan.place_districts();
        plan.lay_boulevards();
        for c in 0..plan.cells.len() {
            plan.cut_cell(c);
        }
        for b in 0..plan.blocks.len() {
            if plan.blocks[b].usage == BlockUse::Lots {
                plan.make_lots(b);
            }
        }
        for l in 0..plan.lots.len() {
            if let Some(site) = plan.site_for(l) {
                plan.buildings.push((l, site));
            }
        }
        plan
    }

    /// The district a ground point belongs to (nearest centre, the civic
    /// centre counting a little closer).
    pub fn district_at(&self, p: DVec2) -> usize {
        let mut best = (f64::MAX, 0);
        for (i, d) in self.districts.iter().enumerate() {
            let w = if d.kind == DistrictKind::Civic {
                0.8
            } else {
                1.0
            };
            let dist = p.distance(d.center) * w;
            if dist < best.0 {
                best = (dist, i);
            }
        }
        best.1
    }

    fn place_districts(&mut self) {
        let r = self.params.radius;
        let seed = self.seed.child("districts");
        let mut rng = seed.rng();
        self.districts.push(District {
            id: StableId::from_seed(seed.child(0u32)),
            kind: DistrictKind::Civic,
            center: DVec2::ZERO,
        });
        let mut ring = [
            DistrictKind::Commercial,
            DistrictKind::Residential,
            DistrictKind::Residential,
            DistrictKind::AetherWorks,
            DistrictKind::Expo,
            DistrictKind::Dystopia,
        ];
        rng.shuffle(&mut ring);
        let base = rng.range(0.0, std::f64::consts::TAU);
        let step = std::f64::consts::TAU / ring.len() as f64;
        for (i, kind) in ring.into_iter().enumerate() {
            let a = base + step * i as f64 + rng.range(-0.2, 0.2);
            let dist = r * if kind == DistrictKind::Dystopia {
                rng.range(0.62, 0.72)
            } else {
                rng.range(0.5, 0.62)
            };
            self.districts.push(District {
                id: StableId::from_seed(seed.child(i as u32 + 1)),
                kind,
                center: det::dir2(a) * dist,
            });
        }
    }

    fn add_street(&mut self, class: StreetClass, a: DVec2, b: DVec2) -> u32 {
        let (width, sidewalk) = class.widths();
        self.streets.push(Street {
            class,
            a,
            b,
            width,
            sidewalk,
        });
        (self.streets.len() - 1) as u32
    }

    /// The boulevard lattice and its cells.
    fn lay_boulevards(&mut self) {
        let seed = self.seed.child("boulevards");
        let s = self.params.boulevard_spacing;
        let r = self.params.radius;
        let angle = seed.child("angle").unit() * std::f64::consts::FRAC_PI_2;
        let (u, v) = (det::dir2(angle), det::dir2(angle).perp());
        let n = (r / s).ceil() as i64 + 1;
        let node = |i: i64, j: i64| -> DVec2 {
            let mut g = seed.cell(i, j).rng();
            let jitter = DVec2::new(g.range(-0.15, 0.15), g.range(-0.15, 0.15)) * s;
            u * (i as f64 * s + jitter.x) + v * (j as f64 * s + jitter.y)
        };
        let mut edges: BTreeMap<(Node, Node), u32> = BTreeMap::new();
        for j in -n..n {
            for i in -n..n {
                let ids = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)];
                let pts: Vec<DVec2> = ids.iter().map(|&(a, b)| node(a, b)).collect();
                let poly = Poly {
                    pts: pts.clone(),
                    tags: vec![EdgeTag::Inner; 4],
                };
                if poly.centroid().length() > r || poly.area() <= 0.0 {
                    continue;
                }
                let mut tags = Vec::with_capacity(4);
                for k in 0..4 {
                    let (a, b) = (ids[k], ids[(k + 1) % 4]);
                    let key = if a < b { (a, b) } else { (b, a) };
                    let id = match edges.get(&key) {
                        Some(id) => *id,
                        None => {
                            let id =
                                self.add_street(StreetClass::Boulevard, pts[k], pts[(k + 1) % 4]);
                            edges.insert(key, id);
                            id
                        }
                    };
                    tags.push(EdgeTag::Street(id));
                }
                let poly = Poly { pts, tags };
                let district = self.district_at(poly.centroid());
                self.cells.push(Cell {
                    id: StableId::from_seed(seed.cell(i, j).child("cell")),
                    district,
                    poly,
                });
            }
        }
    }

    /// Cuts a cell into blocks with local streets.
    fn cut_cell(&mut self, cell: usize) {
        let seed = Seed(self.cells[cell].id.0).child("cuts");
        let mut rng = seed.rng();
        // One street class per cell, so streets meeting at a T are equally wide
        // (a wider street ending on a narrower one would poke into the blocks).
        let class = self.districts[self.cells[cell].district]
            .kind
            .rules()
            .street;
        let mut queue = vec![self.cells[cell].poly.clone()];
        let mut done: Vec<Poly> = Vec::new();
        // Districts can change inside a cell; each piece cuts by the rules of
        // the district at its own centre, so borders run along streets.
        while let Some(piece) = queue.pop() {
            let district = self.district_at(piece.centroid());
            let rules = self.districts[district].kind.rules();
            let obb = piece.obb();
            let (short_t, long_t) = rules.block;
            let small = obb.long() < long_t * 1.35 && obb.short() < short_t * 1.6;
            if small || done.len() + queue.len() > 400 {
                done.push(piece);
                continue;
            }
            let axis = obb.long_axis();
            let wander = rng.range(-rules.wander, rules.wander);
            let normal = (axis + axis.perp() * wander).normalize();
            let shift = rng.range(-0.12, 0.12) * obb.long();
            let point = piece.centroid() + axis * shift;
            let id = self.streets.len() as u32;
            let cut = piece.split(point, normal.perp(), EdgeTag::Street(id));
            let Some((a, b)) = cut else {
                done.push(piece);
                continue;
            };
            let min_area = short_t * short_t * 0.45;
            if a.area() < min_area || b.area() < min_area {
                done.push(piece);
                continue;
            }
            // The cut's chord, from the edge tagged with the new street.
            let k = a
                .tags
                .iter()
                .position(|t| *t == EdgeTag::Street(id))
                .expect("cut edge");
            let (p, q) = a.edge(k);
            self.add_street(class, p, q);
            queue.push(a);
            queue.push(b);
        }
        // Process in a stable order (the queue order is already deterministic, but
        // sorting makes block order independent of how cuts were scheduled).
        done.sort_by(|a, b| {
            let (ca, cb) = (a.centroid(), b.centroid());
            ca.x.total_cmp(&cb.x).then(ca.y.total_cmp(&cb.y))
        });
        for (k, piece) in done.into_iter().enumerate() {
            let streets = &self.streets;
            let Some(poly) = piece.inset_by(|t| match t {
                EdgeTag::Street(s) => streets[s as usize].reach(),
                EdgeTag::Inner => 0.0,
            }) else {
                continue;
            };
            if poly.area() < 200.0 {
                continue;
            }
            let bseed = seed.child("block").child(k as u32);
            let district = self.district_at(poly.centroid());
            let rules = self.districts[district].kind.rules();
            let mut brng = bseed.rng();
            let roll = brng.unit();
            let usage = if roll < rules.plaza {
                BlockUse::Plaza
            } else if roll < rules.plaza + rules.park {
                BlockUse::Park
            } else {
                BlockUse::Lots
            };
            self.blocks.push(Block {
                id: StableId::from_seed(bseed),
                cell,
                district,
                usage,
                poly,
            });
        }
    }

    /// Splits a block into lots that each front a street.
    fn make_lots(&mut self, block: usize) {
        let b = &self.blocks[block];
        let rules = self.districts[b.district].kind.rules();
        let seed = Seed(b.id.0).child("lots");
        let mut rng = seed.rng();
        let obb = b.poly.obb();
        // Deep blocks are halved along their length first, so both halves front
        // a long street and the middle becomes back yards.
        let mut strips = vec![b.poly.clone()];
        if obb.short() > 2.0 * (rules.depth.1 + rules.setback_front) + 8.0 {
            let axis = obb.long_axis();
            if let Some((l, r)) = b.poly.split(obb.center, axis, EdgeTag::Inner) {
                strips = vec![l, r];
            }
        }
        let mut lots = Vec::new();
        for strip in strips {
            let sobb = strip.obb();
            let axis = sobb.long_axis();
            let (lo, hi) = strip.pts.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
                (lo.min(p.dot(axis)), hi.max(p.dot(axis)))
            });
            let mut rest = strip;
            let mut t = lo;
            loop {
                let w = rng.range(rules.lot_width.0, rules.lot_width.1);
                if t + w > hi - rules.lot_width.0 * 0.7 {
                    break;
                }
                t += w;
                let point = axis * t;
                match rest.split(point, axis.perp(), EdgeTag::Inner) {
                    Some((a, b)) => {
                        // The piece on the low side is finished.
                        let (done, keep) = if a.centroid().dot(axis) < b.centroid().dot(axis) {
                            (a, b)
                        } else {
                            (b, a)
                        };
                        lots.push(done);
                        rest = keep;
                    }
                    None => break,
                }
            }
            lots.push(rest);
        }
        for (k, poly) in lots.into_iter().enumerate() {
            // Front: the longest edge on a street, preferring the busier street.
            let front = (0..poly.len())
                .filter_map(|i| match poly.tags[i] {
                    EdgeTag::Street(s) => Some((i, s)),
                    EdgeTag::Inner => None,
                })
                .max_by(|(i, s), (j, t)| {
                    let rank = |s: u32| match self.streets[s as usize].class {
                        StreetClass::Boulevard => 0.8,
                        _ => 1.0,
                    };
                    (poly.edge_len(*i) * rank(*s)).total_cmp(&(poly.edge_len(*j) * rank(*t)))
                });
            let Some((front, street)) = front else {
                continue;
            };
            if poly.area() < 60.0 {
                continue;
            }
            self.lots.push(Lot {
                id: StableId::from_seed(seed.child(k as u32)),
                block,
                poly,
                front,
                street,
            });
        }
    }

    /// The building rectangle on a lot, or None if it is too cramped.
    fn site_for(&self, lot_index: usize) -> Option<BuildingSite> {
        let lot = &self.lots[lot_index];
        let block = &self.blocks[lot.block];
        let district = &self.districts[block.district];
        let rules = district.kind.rules();
        let seed = Seed(lot.id.0).child("building");
        let mut rng = seed.rng();
        let (a, b) = lot.poly.edge(lot.front);
        let along = (b - a).normalize();
        let inward = lot.poly.inward(lot.front);
        let center_dist = lot.poly.centroid().length() / self.params.radius;

        let (usage, floors, front_sb, side_sb, depth) =
            self.programme(district.kind, center_dist, rules, &mut rng);

        let v0 = a.dot(inward) + front_sb;
        let max_v = lot
            .poly
            .pts
            .iter()
            .map(|p| p.dot(inward))
            .fold(f64::MIN, f64::max);
        let depth = depth.min(max_v - v0 - 2.0);
        if depth < crate::building::CORE_SIZE.y + crate::building::CORRIDOR_WIDTH {
            return None;
        }
        let s0 = lot.poly.span_at(along, inward, v0)?;
        let s1 = lot.poly.span_at(along, inward, v0 + depth)?;
        let (lo, hi) = (s0.0.max(s1.0) + side_sb, s0.1.min(s1.1) - side_sb);
        let width = hi - lo;
        if width < 10.0 {
            return None;
        }
        // Narrow buildings stay low.
        let floors = if width < 16.0 || depth < 16.0 {
            floors.min(6)
        } else {
            floors
        };
        Some(BuildingSite {
            id: StableId::from_seed(seed),
            seed,
            usage,
            origin: along * lo + inward * v0,
            along,
            inward,
            size: DVec2::new(width, depth),
            floors,
            street: lot.street,
        })
    }

    /// Use, floors, setbacks and depth for a building in a district.
    fn programme(
        &self,
        kind: DistrictKind,
        center_dist: f64,
        rules: &Rules,
        rng: &mut Rng,
    ) -> (BuildingUse, u32, f64, f64, f64) {
        let (f0, f1) = rules.floors;
        let mut floors = rng.range_int(f0 as i64, f1 as i64 + 1) as u32;
        let mut depth = rng.range(rules.depth.0, rules.depth.1);
        let (mut front, mut side) = (rules.setback_front, rules.setback_side);
        let usage = match kind {
            DistrictKind::Civic => {
                // Tallest at the very centre, so the skyline peaks there.
                let peak = (1.0 - center_dist / 0.35).clamp(0.0, 1.0);
                floors = (f0 as f64 + (f1 - f0) as f64 * (0.3 + 0.7 * peak) * rng.range(0.7, 1.0))
                    as u32;
                BuildingUse::Offices
            }
            DistrictKind::Commercial => BuildingUse::ShopsAndOffices,
            DistrictKind::Residential => {
                if center_dist > 0.72 && rng.chance(0.7) {
                    floors = rng.range_int(1, 3) as u32;
                    depth = rng.range(10.0, 14.0);
                    front = 6.0;
                    side = 3.0;
                    BuildingUse::House
                } else {
                    BuildingUse::Apartments
                }
            }
            DistrictKind::AetherWorks => BuildingUse::Works,
            DistrictKind::Expo => BuildingUse::Pavilion,
            DistrictKind::Dystopia => {
                if rng.chance(0.12) {
                    floors = rng.range_int(2, 5) as u32;
                    BuildingUse::Lab
                } else {
                    BuildingUse::Tenement
                }
            }
        };
        (usage, floors.max(1), front, side, depth)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::BuildingPlan;

    fn plan(seed: u64) -> CityPlan {
        CityPlan::generate(Seed(seed), &CityParams::default())
    }

    #[test]
    fn same_seed_same_city() {
        assert_eq!(plan(1), plan(1));
    }

    #[test]
    fn different_seeds_differ() {
        let (a, b) = (plan(1), plan(2));
        assert_ne!(a.streets, b.streets);
    }

    #[test]
    fn every_district_is_there_and_built() {
        for seed in 1..6 {
            let p = plan(seed);
            for kind in DistrictKind::ALL {
                let built = p
                    .buildings
                    .iter()
                    .filter(|(l, _)| p.districts[p.blocks[p.lots[*l].block].district].kind == kind)
                    .count();
                assert!(built > 3, "seed {seed}: {kind:?} has {built} buildings");
            }
        }
    }

    #[test]
    fn city_has_a_sensible_size() {
        let p = plan(7);
        let n = p.buildings.len();
        assert!((400..6000).contains(&n), "{n} buildings");
        assert!(p.blocks.len() > 100, "{} blocks", p.blocks.len());
    }

    #[test]
    fn blocks_keep_clear_of_streets() {
        let p = plan(3);
        for b in &p.blocks {
            let c = b.poly.centroid();
            for s in &p.streets {
                if s.distance_to(c) > 400.0 {
                    continue;
                }
                for &pt in &b.poly.pts {
                    let d = s.distance_to(pt);
                    assert!(
                        d >= s.reach() - 1e-6,
                        "block corner {pt} is {d} m from a street that reaches {}",
                        s.reach()
                    );
                }
                // Also the edge midpoints, for long edges.
                for i in 0..b.poly.len() {
                    let (x, y) = b.poly.edge(i);
                    let d = s.distance_to((x + y) * 0.5);
                    assert!(
                        d >= s.reach() - 1e-6,
                        "edge mid {d} m from {:?} reaching {}",
                        s.class,
                        s.reach()
                    );
                }
            }
        }
    }

    #[test]
    fn lots_sit_in_blocks_and_buildings_in_lots() {
        let p = plan(4);
        for l in &p.lots {
            let block = &p.blocks[l.block].poly;
            for &pt in &l.poly.pts {
                assert!(block.contains(pt), "lot corner outside its block");
            }
        }
        for (l, s) in &p.buildings {
            let lot = &p.lots[*l].poly;
            for c in s.corners() {
                assert!(lot.contains(c), "building corner outside its lot");
            }
            // The front faces its street, across the sidewalk and setback.
            let street = &p.streets[s.street as usize];
            let mid = s.origin + s.along * s.size.x * 0.5;
            let d = street.distance_to(mid);
            assert!(d < street.reach() + 12.0, "front {d} m from its street");
        }
    }

    #[test]
    fn buildings_do_not_overlap() {
        let p = plan(5);
        // Separating-axis test on the oriented rectangles of nearby buildings.
        let sites: Vec<&BuildingSite> = p.buildings.iter().map(|(_, s)| s).collect();
        for (i, a) in sites.iter().enumerate() {
            let ca = a.corners();
            let center_a = (ca[0] + ca[2]) * 0.5;
            for b in &sites[i + 1..] {
                let cb = b.corners();
                if center_a.distance((cb[0] + cb[2]) * 0.5) > 200.0 {
                    continue;
                }
                let axes = [a.along, a.inward, b.along, b.inward];
                let separated = axes.iter().any(|ax| {
                    let pa = ca.iter().map(|c| c.dot(*ax));
                    let pb = cb.iter().map(|c| c.dot(*ax));
                    let (a0, a1) = pa.fold((f64::MAX, f64::MIN), |(l, h), v| (l.min(v), h.max(v)));
                    let (b0, b1) = pb.fold((f64::MAX, f64::MIN), |(l, h), v| (l.min(v), h.max(v)));
                    a1 <= b0 + 1e-6 || b1 <= a0 + 1e-6
                });
                assert!(separated, "two buildings overlap");
            }
        }
    }

    #[test]
    fn every_building_plan_is_sound() {
        let p = plan(6);
        for (_, s) in &p.buildings {
            crate::building::tests::check_plan(&BuildingPlan::generate(s));
        }
    }

    #[test]
    fn civic_centre_is_tallest() {
        let p = plan(8);
        let tallest = p
            .buildings
            .iter()
            .max_by(|a, b| a.1.height().total_cmp(&b.1.height()))
            .unwrap();
        let kind = p.districts[p.blocks[p.lots[tallest.0].block].district].kind;
        assert_eq!(kind, DistrictKind::Civic);
    }
}
