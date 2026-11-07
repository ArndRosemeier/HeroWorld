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
//! Water comes first ([`crate::water`]): rivers and the coast are lines the
//! cells are split along before anything else, registered as quay and shore
//! "streets" so blocks keep clear of them and lots can front them.
//! Boulevards cross rivers on bridges and stop at the shore.
//!
//! Coordinates are the city's local frame on the ground (x east, z south),
//! in metres, centred on the civic centre.

use crate::building::{BuildingSite, BuildingUse};
use crate::geom::{EdgeTag, Poly};
use crate::water::{self, CityWater};
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
    /// A river reach: the "carriageway" is the water, the "sidewalks" the
    /// quay promenades.
    Quay,
    /// A reach of the coastline: beach and promenade on the land side.
    Shore,
}

impl StreetClass {
    /// Carriageway width and sidewalk width (each side), roomier than real life.
    pub fn widths(self) -> (f64, f64) {
        match self {
            Self::Boulevard => (22.0, 6.0),
            Self::Avenue => (16.0, 5.0),
            Self::Street => (12.0, 4.0),
            Self::Lane => (8.0, 2.5),
            Self::Quay => (0.0, water::QUAY_WIDTH),
            Self::Shore => (0.0, water::SHORE_WIDTH),
        }
    }

    /// Water, not a road.
    pub fn is_water(self) -> bool {
        matches!(self, Self::Quay | Self::Shore)
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
        match self.class {
            // The coast is one-sided: the line is the water's edge.
            StreetClass::Shore => self.sidewalk,
            _ => self.width * 0.5 + self.sidewalk,
        }
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
    pub water: CityWater,
    /// The ground down to the water's edge, out past the built-up area.
    /// Edges facing water are tagged with that quay or shore street.
    pub land: Vec<Poly>,
    /// Promenades and beaches: land within reach of a quay or shore street,
    /// with that street's index.
    pub waterfront: Vec<(Poly, u32)>,
    pub bridges: Vec<Bridge>,
}

/// Where a boulevard crosses a river.
#[derive(Clone, Debug, PartialEq)]
pub struct Bridge {
    /// The boulevard.
    pub street: u32,
    /// The quay street (river reach) it crosses.
    pub river: u32,
    /// Ends of the span, on the boulevard's centre line, just past the banks.
    pub a: DVec2,
    pub b: DVec2,
}

/// One straight piece of a river or the coast, registered as a street.
#[derive(Clone, Copy, Debug)]
struct Reach {
    street: u32,
    a: DVec2,
    b: DVec2,
    /// Half the water's width (0 for the coast).
    half: f64,
    /// From the line to the blocks.
    reach: f64,
    coast: bool,
}

/// How far the ground plan reaches past the built-up area (m).
const COUNTRY: f64 = 1200.0;

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
            water: CityWater::generate(seed, params.radius, params.radius + COUNTRY * 1.6),
            land: Vec::new(),
            waterfront: Vec::new(),
            bridges: Vec::new(),
        };
        let reaches = plan.add_water_streets();
        plan.place_districts();
        let outer = plan.lay_boulevards();
        for c in 0..plan.cells.len() {
            plan.cut_cell(c, &reaches);
        }
        for poly in outer {
            for piece in plan.split_by_water(poly, &reaches, None) {
                plan.add_land(&piece, &reaches, false);
            }
        }
        plan.trim_boulevards(&reaches);
        for b in 0..plan.blocks.len() {
            if plan.blocks[b].usage == BlockUse::Lots {
                plan.make_lots(b);
            }
        }
        for l in 0..plan.lots.len() {
            if let Some(site) = plan.site_for(l) {
                // Corner lots can bring buildings of two streets close; keep a
                // gap, so cornices and towers never reach into a neighbour.
                let clear = plan
                    .buildings
                    .iter()
                    .all(|(_, other)| rects_apart(&site, other, BUILDING_GAP));
                if clear {
                    plan.buildings.push((l, site));
                }
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
        let step = std::f64::consts::TAU / ring.len() as f64;
        let mut base = rng.range(0.0, std::f64::consts::TAU);
        if let Some(c) = &self.water.coast {
            // The Aether Works keep the harbour.
            let k = ring
                .iter()
                .position(|d| *d == DistrictKind::AetherWorks)
                .expect("works");
            base = det::atan2(c.seaward.y, c.seaward.x) - step * k as f64;
        }
        for (i, kind) in ring.into_iter().enumerate() {
            let a = base + step * i as f64 + rng.range(-0.2, 0.2);
            let dist = r * if kind == DistrictKind::Dystopia {
                rng.range(0.62, 0.72)
            } else {
                rng.range(0.5, 0.62)
            };
            let mut center = det::dir2(a) * dist;
            if let Some(c) = &self.water.coast {
                // On land, and the works right on the shore (the harbour).
                let shore = 250.0;
                for _ in 0..60 {
                    let d = c.land_distance(center);
                    if d < shore && center.length() > 150.0 {
                        center *= 0.96;
                    } else if kind == DistrictKind::AetherWorks && d > shore + 100.0 {
                        center *= 1.04;
                    } else {
                        break;
                    }
                }
            }
            self.districts.push(District {
                id: StableId::from_seed(seed.child(i as u32 + 1)),
                kind,
                center,
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

    /// The boulevard lattice and its cells. Returns the lattice quads out in
    /// the country past the city, which become plain land.
    fn lay_boulevards(&mut self) -> Vec<Poly> {
        let seed = self.seed.child("boulevards");
        let s = self.params.boulevard_spacing;
        let r = self.params.radius;
        // At about 45 degrees to the main river near the centre, so bridges
        // cross it squarely enough and blocks meet it at a slant.
        let main = &self.water.rivers[0].pts;
        let k = (0..main.len() - 1)
            .min_by(|&i, &j| {
                let d = |k: usize| water::segment_distance(DVec2::ZERO, main[k], main[k + 1]);
                d(i).total_cmp(&d(j))
            })
            .unwrap_or(0);
        let flow = main[k + 1] - main[k];
        let angle = det::atan2(flow.y, flow.x)
            + std::f64::consts::FRAC_PI_4
            + (seed.child("angle").unit() - 0.5) * 0.5;
        let mut outer = Vec::new();
        let (u, v) = (det::dir2(angle), det::dir2(angle).perp());
        let n = ((r + COUNTRY) / s).ceil() as i64 + 1;
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
                if poly.area() <= 0.0 {
                    continue;
                }
                let dist = poly.centroid().length();
                if dist > r {
                    if dist < r + COUNTRY {
                        outer.push(poly);
                    }
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
        outer
    }

    /// Registers every reach of the rivers and the coast as a street.
    fn add_water_streets(&mut self) -> Vec<Reach> {
        let mut out = Vec::new();
        let rivers: Vec<(Vec<DVec2>, f64)> = self
            .water
            .rivers
            .iter()
            .map(|r| (r.pts.clone(), r.width))
            .collect();
        for (pts, width) in rivers {
            for w in pts.windows(2) {
                let street = self.add_street(StreetClass::Quay, w[0], w[1]);
                self.streets[street as usize].width = width;
                out.push(Reach {
                    street,
                    a: w[0],
                    b: w[1],
                    half: width * 0.5,
                    reach: self.streets[street as usize].reach(),
                    coast: false,
                });
            }
        }
        if let Some(c) = self.water.coast.clone() {
            for w in c.pts.windows(2) {
                let street = self.add_street(StreetClass::Shore, w[0], w[1]);
                out.push(Reach {
                    street,
                    a: w[0],
                    b: w[1],
                    half: 0.0,
                    reach: water::SHORE_WIDTH,
                    coast: true,
                });
            }
        }
        out
    }

    /// Splits a piece of land along every water line crossing it, and drops
    /// what lies at sea. A cut carried on past the end of its reach (beyond
    /// a bend) runs between two pieces of land: with `class` it becomes a
    /// street of that class, else (country, no blocks) an inner edge.
    fn split_by_water(
        &mut self,
        poly: Poly,
        reaches: &[Reach],
        class: Option<StreetClass>,
    ) -> Vec<Poly> {
        let mut pieces = vec![poly];
        for r in reaches {
            let mut next = Vec::with_capacity(pieces.len() + 1);
            for p in pieces {
                if crosses(&p, r.a, r.b) {
                    if let Some((x, y)) = p.split(r.a, r.b - r.a, EdgeTag::Street(r.street)) {
                        next.push(x);
                        next.push(y);
                        continue;
                    }
                }
                next.push(p);
            }
            pieces = next;
        }
        let at_sea = |p: DVec2| {
            self.water
                .coast
                .as_ref()
                .is_some_and(|c| c.land_distance(p) < 0.0)
        };
        let mut pieces: Vec<Poly> = pieces
            .into_iter()
            .filter(|p| !at_sea(p.centroid()))
            .collect();
        // Edges on a reach's line but past its ends, by reach.
        let mut past: BTreeMap<u32, Vec<(usize, usize)>> = BTreeMap::new();
        for (k, p) in pieces.iter().enumerate() {
            for i in 0..p.len() {
                if let EdgeTag::Street(id) = p.tags[i] {
                    if let Some(r) = reaches.iter().find(|r| r.street == id) {
                        let (x, y) = p.edge(i);
                        if water::segment_distance((x + y) * 0.5, r.a, r.b) > 0.5 {
                            past.entry(id).or_default().push((k, i));
                        }
                    }
                }
            }
        }
        for (id, edges) in past {
            let tag = match class {
                Some(class) => {
                    // One street along the whole run, stopped at the water.
                    let r = reaches.iter().find(|r| r.street == id).expect("reach");
                    let dir = (r.b - r.a).normalize();
                    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
                    for &(k, i) in &edges {
                        let (x, y) = pieces[k].edge(i);
                        for t in [(x - r.a).dot(dir), (y - r.a).dot(dir)] {
                            lo = lo.min(t);
                            hi = hi.max(t);
                        }
                    }
                    let (p, q) = (r.a + dir * lo, r.a + dir * hi);
                    let (p, q) = trim_at_water(p, q, reaches, self.water.coast.as_ref());
                    EdgeTag::Street(self.add_street(class, p, q))
                }
                None => EdgeTag::Inner,
            };
            for (k, i) in edges {
                pieces[k].tags[i] = tag;
            }
        }
        pieces
    }

    /// Keeps `poly` at least `margin(reach)` from every water line, on its
    /// own side (the land side of the coast). None if nothing is left.
    fn clear_of_water(
        poly: Poly,
        reaches: &[Reach],
        margin: impl Fn(&Reach) -> f64,
    ) -> Option<Poly> {
        let mut out = poly;
        for r in reaches {
            let m = margin(r);
            if poly_segment_distance(&out, r.a, r.b) >= m - 1e-9 {
                continue;
            }
            let dir = (r.b - r.a).normalize_or_zero();
            let mut n = dir.perp();
            if !r.coast && n.dot(out.centroid() - r.a) < 0.0 {
                n = -n;
            }
            out = out.clip(r.a + n * m, n, EdgeTag::Street(r.street))?;
        }
        // A clip past the end of its reach (at a bend) fronts no water.
        for i in 0..out.len() {
            if let EdgeTag::Street(id) = out.tags[i] {
                if let Some(r) = reaches.iter().find(|r| r.street == id) {
                    let (x, y) = out.edge(i);
                    if water::segment_distance((x + y) * 0.5, r.a, r.b) > r.reach + 1.0 {
                        out.tags[i] = EdgeTag::Inner;
                    }
                }
            }
        }
        Some(out)
    }

    /// Records a piece of ground (and its promenades and beaches).
    /// `city`: inside the built-up area, where promenades and beaches go.
    fn add_land(&mut self, piece: &Poly, reaches: &[Reach], city: bool) {
        let Some(land) = Self::clear_of_water(piece.clone(), reaches, |r| r.half) else {
            return;
        };
        for r in reaches.iter().filter(|_| city) {
            if poly_segment_distance(&land, r.a, r.b) >= r.reach - 1e-9 {
                continue;
            }
            let dir = (r.b - r.a).normalize_or_zero();
            let mut n = dir.perp();
            if !r.coast && n.dot(land.centroid() - r.a) < 0.0 {
                n = -n;
            }
            // The strip between the water and the blocks, along this reach
            // only (not on past its ends).
            let strip = land
                .clip(r.a + n * r.reach, -n, EdgeTag::Inner)
                .and_then(|p| p.clip(r.a, dir, EdgeTag::Inner))
                .and_then(|p| p.clip(r.b, -dir, EdgeTag::Inner));
            if let Some(strip) = strip {
                if strip.area() > 1.0 {
                    self.waterfront.push((strip, r.street));
                }
            }
        }
        self.land.push(land);
    }

    /// Boulevards stop short of the sea, and bridge the rivers they cross.
    fn trim_boulevards(&mut self, reaches: &[Reach]) {
        for id in 0..self.streets.len() {
            let s = &self.streets[id];
            if s.class != StreetClass::Boulevard {
                continue;
            }
            let (a, b) = (s.a, s.b);
            let dir = (b - a).normalize_or_zero();
            // Crossings steep enough to bridge: (point, half span, reach).
            let spans: Vec<(DVec2, f64, &Reach)> = reaches
                .iter()
                .filter(|r| !r.coast)
                .filter_map(|r| {
                    let (x, sin) = intersect(a, b, r.a, r.b)?;
                    (sin > 0.45).then(|| (x, (r.half + 2.0) / sin, r))
                })
                .collect();
            let keep = water::SHORE_WIDTH - water::PROMENADE_WIDTH + s.reach();
            let coast = self.water.coast.as_ref();
            let rivers = &self.water.rivers;
            let ok = |x: DVec2| {
                let on_bridge = spans.iter().any(|(c, h, _)| c.distance(x) <= *h);
                coast.is_none_or(|c| c.land_distance(x) >= keep)
                    && (on_bridge || rivers.iter().all(|r| r.distance(x) >= r.width * 0.5 + 2.0))
            };
            let (t0, t1) = dry_span(a, b, ok).unwrap_or((0.0, 0.0));
            let (a2, b2) = (a.lerp(b, t0), a.lerp(b, t1));
            let mut bridges = Vec::new();
            for (x, half, r) in &spans {
                // Clamped to the street (two boulevards can meet on a bridge).
                let len = a.distance(b);
                let tx = (*x - a).dot(dir);
                if tx >= t0 * len && tx <= t1 * len {
                    let (ta, tb) = ((tx - half).max(t0 * len), (tx + half).min(t1 * len));
                    bridges.push(Bridge {
                        street: id as u32,
                        river: r.street,
                        a: a + dir * ta,
                        b: a + dir * tb,
                    });
                }
            }
            self.bridges.extend(bridges);
            let s = &mut self.streets[id];
            s.a = a2;
            s.b = b2;
        }
    }

    /// Cuts a cell into blocks with local streets.
    fn cut_cell(&mut self, cell: usize, reaches: &[Reach]) {
        let seed = Seed(self.cells[cell].id.0).child("cuts");
        let mut rng = seed.rng();
        // One street class per cell, so streets meeting at a T are equally wide
        // (a wider street ending on a narrower one would poke into the blocks).
        let class = self.districts[self.cells[cell].district]
            .kind
            .rules()
            .street;
        let mut queue = self.split_by_water(self.cells[cell].poly.clone(), reaches, Some(class));
        for piece in &queue {
            self.add_land(piece, reaches, true);
        }
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
            let (p, q) = trim_at_water(p, q, reaches, self.water.coast.as_ref());
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
            // Clear of every river and the coast, even past bends.
            let Some(poly) = Self::clear_of_water(poly, reaches, |r| r.reach) else {
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
            // Only edges really along their street: one cut short at the
            // water no longer reaches every block it once bounded.
            let along = |i: usize, s: u32| {
                let st = &self.streets[s as usize];
                let (x, y) = poly.edge(i);
                st.distance_to((x + y) * 0.5) <= st.reach() + 2.0
            };
            let front = (0..poly.len())
                .filter_map(|i| match poly.tags[i] {
                    EdgeTag::Street(s) if along(i, s) => Some((i, s)),
                    _ => None,
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
            district: district.kind,
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

/// Does segment `ab` pass through the inside of convex `poly`?
fn crosses(poly: &Poly, a: DVec2, b: DVec2) -> bool {
    // Clip the segment to the polygon (Cyrus-Beck).
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    let d = b - a;
    for i in 0..poly.len() {
        let (p, _) = poly.edge(i);
        let n = poly.inward(i);
        let num = n.dot(a - p);
        let den = n.dot(d);
        if den.abs() < 1e-12 {
            if num < 0.0 {
                return false;
            }
        } else {
            let t = -num / den;
            if den > 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
        if t0 >= t1 {
            return false;
        }
    }
    (t1 - t0) * d.length() > 1e-6
}

/// Least distance between convex `poly` and segment `ab` (0 if they meet).
fn poly_segment_distance(poly: &Poly, a: DVec2, b: DVec2) -> f64 {
    if crosses(poly, a, b) || poly.contains(a) {
        return 0.0;
    }
    let mut best = f64::MAX;
    for i in 0..poly.len() {
        let (p, q) = poly.edge(i);
        best = best
            .min(water::segment_distance(p, a, b))
            .min(water::segment_distance(a, p, q))
            .min(water::segment_distance(b, p, q));
    }
    best
}

/// Where segments `ab` and `cd` cross, with the sine of their angle.
fn intersect(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> Option<(DVec2, f64)> {
    let (r, s) = (b - a, d - c);
    let den = r.perp_dot(s);
    if den.abs() < 1e-9 {
        return None;
    }
    let t = (c - a).perp_dot(s) / den;
    let u = (c - a).perp_dot(r) / den;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u))
        .then(|| (a + r * t, (den / (r.length() * s.length())).abs()))
}

/// The longest part of segment `ab` where `ok` holds, as (t0, t1).
fn dry_span(a: DVec2, b: DVec2, ok: impl Fn(DVec2) -> bool) -> Option<(f64, f64)> {
    let n = 48;
    let at = |t: f64| ok(a.lerp(b, t));
    let mut best: Option<(f64, f64)> = None;
    let mut run: Option<f64> = None;
    for i in 0..=n {
        let t = i as f64 / n as f64;
        let good = at(t);
        match (good, run) {
            (true, None) => run = Some(t),
            (false, Some(t0)) => {
                let t1 = (i - 1) as f64 / n as f64;
                if best.is_none_or(|(b0, b1)| t1 - t0 > b1 - b0) {
                    best = Some((t0, t1));
                }
                run = None;
            }
            _ => {}
        }
    }
    if let Some(t0) = run {
        if best.is_none_or(|(b0, b1)| 1.0 - t0 > b1 - b0) {
            best = Some((t0, 1.0));
        }
    }
    let (t0, t1) = best?;
    // Refine each end between the last wet and the first dry sample.
    let refine = |mut wet: f64, mut dry: f64| {
        for _ in 0..30 {
            let m = (wet + dry) * 0.5;
            if at(m) {
                dry = m;
            } else {
                wet = m;
            }
        }
        dry
    };
    let step = 1.0 / n as f64;
    let t0 = if t0 > 0.0 { refine(t0 - step, t0) } else { t0 };
    let t1 = if t1 < 1.0 { refine(t1 + step, t1) } else { t1 };
    (t1 > t0).then_some((t0, t1))
}

/// The part of street `pq` clear of the water: it ends at the quays and
/// the beach. A street wholly in the water shrinks to a point.
fn trim_at_water(
    p: DVec2,
    q: DVec2,
    reaches: &[Reach],
    coast: Option<&water::Coast>,
) -> (DVec2, DVec2) {
    let ok = |x: DVec2| {
        reaches
            .iter()
            .filter(|r| !r.coast)
            .all(|r| water::segment_distance(x, r.a, r.b) >= r.reach - 1e-6)
            && coast.is_none_or(|c| c.land_distance(x) >= water::SHORE_WIDTH - 1e-6)
    };
    match dry_span(p, q, ok) {
        Some((t0, t1)) => (p.lerp(q, t0), p.lerp(q, t1)),
        None => (p, p),
    }
}

/// Least gap between two buildings (m).
pub const BUILDING_GAP: f64 = 1.0;

/// Are two building rectangles at least `gap` apart? (Separating axes on
/// the rectangles grown by half the gap.)
fn rects_apart(a: &BuildingSite, b: &BuildingSite, gap: f64) -> bool {
    let (ca, cb) = (a.corners(), b.corners());
    if ((ca[0] + ca[2]) * 0.5).distance((cb[0] + cb[2]) * 0.5)
        > (a.size.length() + b.size.length()) * 0.5 + gap
    {
        return true;
    }
    let axes = [a.along, a.inward, b.along, b.inward];
    axes.iter().any(|ax| {
        let span = |c: &[DVec2; 4]| {
            c.iter()
                .map(|p| p.dot(*ax))
                .fold((f64::MAX, f64::MIN), |(l, h), v| (l.min(v), h.max(v)))
        };
        let ((a0, a1), (b0, b1)) = (span(&ca), span(&cb));
        a1 + gap <= b0 + 1e-6 || b1 + gap <= a0 + 1e-6
    })
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
                assert!(
                    rects_apart(a, b, BUILDING_GAP - 1e-6),
                    "two buildings too close"
                );
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
    fn nothing_is_built_in_the_water() {
        for seed in 1..8 {
            let p = plan(seed);
            for (_, s) in &p.buildings {
                for c in s.corners() {
                    assert!(!p.water.is_water(c), "seed {seed}: building in the water");
                    for r in &p.water.rivers {
                        let d = r.distance(c);
                        assert!(
                            d >= r.width * 0.5 + water::QUAY_WIDTH - 1e-6,
                            "seed {seed}: {d} m from a river"
                        );
                    }
                }
            }
            for st in p
                .streets
                .iter()
                .filter(|s| !s.class.is_water() && s.a != s.b)
            {
                let mid = (st.a + st.b) * 0.5;
                let bridged = p
                    .bridges
                    .iter()
                    .any(|b| p.streets[b.street as usize] == *st);
                assert!(
                    bridged || !p.water.is_water(mid),
                    "seed {seed}: {:?} street in the water",
                    st.class
                );
            }
            if p.water
                .rivers
                .iter()
                .any(|r| r.distance(DVec2::ZERO) < p.params.radius * 0.5)
            {
                assert!(!p.bridges.is_empty(), "seed {seed}: no bridges");
            }
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
