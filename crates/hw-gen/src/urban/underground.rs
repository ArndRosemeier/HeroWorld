//! The town below the town: utility tunnels under the main roads of the
//! centre, Aether lifts down to them from the pavements, and Molehaven, the
//! mole people's cavern, deeper under the old quarters, reached by a long
//! ramp from the tunnels and by its own lift.
//!
//! Everything below ground is a set of [`Void`]s: boxes in plan (a
//! rectangle turned any way) with a floor that may slope along their
//! length and a ceiling some way above it, or open to the sky (a lift
//! shaft). Collision, the carved ground and the drawn linings all come from
//! the same voids.

use super::world::{TownWorld, DECK, WATER_LEVEL};
use glam::{DVec2, DVec3};
use std::collections::BTreeMap;

/// Tunnel floor below the road, and the tunnel's height and half width.
pub const TUNNEL_DEPTH: f64 = 7.5;
pub const TUNNEL_HEIGHT: f64 = 4.2;
pub const TUNNEL_HALF: f64 = 2.4;
/// Side passages from a lift to a tunnel.
const PASSAGE_HALF: f64 = 1.5;
const PASSAGE_HEIGHT: f64 = 3.2;
/// Half the side of a lift shaft.
pub const SHAFT_HALF: f64 = 1.7;
/// Molehaven's floor below the lowest ground over it.
pub const CAVERN_DEPTH: f64 = 27.0;
/// Grid cell for finding voids by place.
const CELL: f64 = 32.0;

/// What a void is (for drawing it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum VoidKind {
    /// A utility tunnel under a main road.
    Tunnel,
    /// A side passage (lift to tunnel, tunnel to cavern).
    Passage,
    /// An Aether lift shaft, open to the sky.
    Shaft,
    /// A chamber of the mole people's cavern.
    Cavern,
}

/// A box of open space under the ground.
#[derive(Clone, Debug, PartialEq)]
pub struct Void {
    pub kind: VoidKind,
    pub origin: DVec2,
    /// Unit vector of its local x; local y is `along.perp()`.
    pub along: DVec2,
    /// The rectangle in local coordinates.
    pub min: DVec2,
    pub max: DVec2,
    /// Floor at local x = `min.x` and at `max.x` (straight between).
    pub floor: [f64; 2],
    /// Ceiling above the floor (ignored when `open`).
    pub height: f64,
    /// Open to the sky: a shaft from its floor up through the ground.
    pub open: bool,
}

impl Void {
    pub fn inward(&self) -> DVec2 {
        self.along.perp()
    }

    pub fn local(&self, p: DVec2) -> DVec2 {
        let d = p - self.origin;
        DVec2::new(d.dot(self.along), d.dot(self.inward()))
    }

    pub fn world(&self, l: DVec2) -> DVec2 {
        self.origin + self.along * l.x + self.inward() * l.y
    }

    /// Is `p` (plan) within the void's rectangle, `margin` outside it too?
    pub fn covers(&self, p: DVec2, margin: f64) -> bool {
        let l = self.local(p);
        l.x >= self.min.x - margin
            && l.x <= self.max.x + margin
            && l.y >= self.min.y - margin
            && l.y <= self.max.y + margin
    }

    /// Floor height at `p` (plan).
    pub fn floor_at(&self, p: DVec2) -> f64 {
        let l = self.local(p);
        let t = ((l.x - self.min.x) / (self.max.x - self.min.x).max(1e-9)).clamp(0.0, 1.0);
        self.floor[0] + (self.floor[1] - self.floor[0]) * t
    }

    /// Ceiling height at `p`, `None` when open to the sky.
    pub fn ceiling_at(&self, p: DVec2) -> Option<f64> {
        (!self.open).then(|| self.floor_at(p) + self.height)
    }

    /// The four corners in plan.
    pub fn corners(&self) -> [DVec2; 4] {
        [
            self.world(self.min),
            self.world(DVec2::new(self.max.x, self.min.y)),
            self.world(self.max),
            self.world(DVec2::new(self.min.x, self.max.y)),
        ]
    }

    /// Is the point inside the open space (`p` in 3D, town frame)?
    pub fn contains(&self, p: DVec3) -> bool {
        let q = DVec2::new(p.x, p.z);
        self.covers(q, 0.0)
            && p.y >= self.floor_at(q)
            && self.ceiling_at(q).is_none_or(|c| p.y <= c)
    }
}

/// A way down from the street.
#[derive(Clone, Debug, PartialEq)]
pub struct Entrance {
    pub name: String,
    /// Centre of the lift shaft.
    pub pos: DVec2,
    /// The way the pavilion's door faces (towards the road).
    pub door: DVec2,
    /// Ground at the top, floor at the bottom.
    pub top: f64,
    pub bottom: f64,
    /// Down to Molehaven (true) or to the utility tunnels.
    pub molehaven: bool,
}

/// The mole people's cavern.
#[derive(Clone, Debug, PartialEq)]
pub struct Cavern {
    pub centre: DVec2,
    pub floor: f64,
    /// Indices of its chambers in [`Underground::voids`].
    pub chambers: Vec<usize>,
}

/// Everything below the town.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Underground {
    pub voids: Vec<Void>,
    pub entrances: Vec<Entrance>,
    pub cavern: Option<Cavern>,
    /// Lamps: on tunnel ceilings, and the cavern's glowing mushrooms.
    pub lamps: Vec<DVec3>,
    grid: BTreeMap<(i64, i64), Vec<u32>>,
}

impl Underground {
    /// Voids whose rectangle reaches `p` (plan).
    pub fn at(&self, p: DVec2) -> impl Iterator<Item = (usize, &Void)> + '_ {
        let k = cell_of(p);
        self.grid
            .get(&k)
            .into_iter()
            .flatten()
            .map(|&i| (i as usize, &self.voids[i as usize]))
            .filter(move |(_, v)| v.covers(p, 0.0))
    }

    /// Voids near `p` (within `r`, roughly).
    pub fn near(&self, p: DVec2, r: f64) -> Vec<usize> {
        let (a, b) = (cell_of(p - DVec2::splat(r)), cell_of(p + DVec2::splat(r)));
        let mut out = Vec::new();
        for x in a.0..=b.0 {
            for (_, list) in self.grid.range((x, a.1)..=(x, b.1)) {
                out.extend(list.iter().map(|&i| i as usize));
            }
        }
        out.sort();
        out.dedup();
        out
    }

    /// The floor under `p` for something at height `y` below the ground:
    /// the highest void floor at most a step above `y` whose space reaches
    /// up to `y`. `None` when `p` is in no void at that height.
    pub fn floor_for(&self, p: DVec2, y: f64) -> Option<f64> {
        let mut best: Option<f64> = None;
        for (_, v) in self.at(p) {
            let f = v.floor_at(p);
            let reach = v.ceiling_at(p).unwrap_or(f64::MAX);
            if y >= f - 1.0 && y <= reach + 0.45 && best.is_none_or(|b| f > b) {
                best = Some(f);
            }
        }
        best
    }

    /// The lowest ceiling above `y` at `p`, among voids `y` is in (none in
    /// a lift shaft: it is open to the sky).
    pub fn ceiling_for(&self, p: DVec2, y: f64) -> Option<f64> {
        if self.at(p).any(|(_, v)| v.open && y >= v.floor_at(p) - 1.0) {
            return None;
        }
        self.at(p)
            .filter(|(_, v)| y >= v.floor_at(p) - 1.0)
            .filter_map(|(_, v)| v.ceiling_at(p))
            .filter(|c| *c >= y)
            .reduce(f64::min)
    }

    /// Is `p` (3D) inside an Aether lift shaft?
    pub fn in_shaft(&self, p: DVec3, ground: f64) -> bool {
        let q = DVec2::new(p.x, p.z);
        self.at(q).any(|(_, v)| {
            v.kind == VoidKind::Shaft && p.y >= v.floor_at(q) - 0.5 && p.y <= ground + 2.5
        })
    }

    /// Is `p` (3D) in open space under the ground?
    pub fn open_at(&self, p: DVec3) -> bool {
        let q = DVec2::new(p.x, p.z);
        self.at(q).any(|(_, v)| v.contains(p))
    }

    fn index(&mut self) {
        self.grid.clear();
        for (i, v) in self.voids.iter().enumerate() {
            let c = v.corners();
            let lo = c.iter().fold(DVec2::splat(f64::MAX), |a, p| a.min(*p));
            let hi = c.iter().fold(DVec2::splat(f64::MIN), |a, p| a.max(*p));
            let (a, b) = (cell_of(lo), cell_of(hi));
            for x in a.0..=b.0 {
                for y in a.1..=b.1 {
                    self.grid.entry((x, y)).or_default().push(i as u32);
                }
            }
        }
    }
}

fn cell_of(p: DVec2) -> (i64, i64) {
    ((p.x / CELL).floor() as i64, (p.y / CELL).floor() as i64)
}

fn seg_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// What stands on the ground, to find free spots for lift pavilions.
struct Ground<'a> {
    w: &'a TownWorld,
    roads: BTreeMap<(i64, i64), Vec<(usize, usize)>>,
    buildings: BTreeMap<(i64, i64), Vec<usize>>,
    props: BTreeMap<(i64, i64), Vec<usize>>,
}

impl<'a> Ground<'a> {
    fn new(w: &'a TownWorld) -> Self {
        let mut roads: BTreeMap<(i64, i64), Vec<(usize, usize)>> = BTreeMap::new();
        for (ri, r) in w.roads.iter().enumerate() {
            for si in 0..r.pts.len().saturating_sub(1) {
                let (a, b) = (r.pts[si], r.pts[si + 1]);
                let reach = r.half + r.pavement + 2.0;
                let lo = a.min(b) - DVec2::splat(reach);
                let hi = a.max(b) + DVec2::splat(reach);
                let (c0, c1) = (cell_of(lo), cell_of(hi));
                for x in c0.0..=c1.0 {
                    for y in c0.1..=c1.1 {
                        roads.entry((x, y)).or_default().push((ri, si));
                    }
                }
            }
        }
        let mut buildings: BTreeMap<(i64, i64), Vec<usize>> = BTreeMap::new();
        for (i, b) in w.buildings.iter().enumerate() {
            let c = b.site.corners();
            let lo = c.iter().fold(DVec2::splat(f64::MAX), |a, p| a.min(*p));
            let hi = c.iter().fold(DVec2::splat(f64::MIN), |a, p| a.max(*p));
            let (c0, c1) = (cell_of(lo), cell_of(hi));
            for x in c0.0..=c1.0 {
                for y in c0.1..=c1.1 {
                    buildings.entry((x, y)).or_default().push(i);
                }
            }
        }
        let mut props: BTreeMap<(i64, i64), Vec<usize>> = BTreeMap::new();
        for (i, p) in w.props.iter().enumerate() {
            props.entry(cell_of(p.pos)).or_default().push(i);
        }
        Self {
            w,
            roads,
            buildings,
            props,
        }
    }

    fn cells(p: DVec2, r: f64) -> impl Iterator<Item = (i64, i64)> {
        let (a, b) = (cell_of(p - DVec2::splat(r)), cell_of(p + DVec2::splat(r)));
        (a.0..=b.0).flat_map(move |x| (a.1..=b.1).map(move |y| (x, y)))
    }

    /// Is a circle of radius `r` at `p` free of roads (and pavements),
    /// buildings, props, landmarks and water?
    fn free(&self, p: DVec2, r: f64) -> bool {
        if self.w.ground(p) < DECK + 1.0 {
            return false;
        }
        for k in Self::cells(p, r) {
            if let Some(list) = self.roads.get(&k) {
                for &(ri, si) in list {
                    let road = &self.w.roads[ri];
                    let d = seg_distance(p, road.pts[si], road.pts[si + 1]);
                    if d < road.half + road.pavement + r + 0.3 {
                        return false;
                    }
                }
            }
            if let Some(list) = self.buildings.get(&k) {
                for &i in list {
                    let s = &self.w.buildings[i].site;
                    let d = p - s.origin;
                    let (x, z) = (d.dot(s.along), d.dot(s.inward));
                    let q = DVec2::new(x.clamp(0.0, s.size.x), z.clamp(0.0, s.size.y));
                    if DVec2::new(x, z).distance(q) < r + 0.8 {
                        return false;
                    }
                }
            }
            if let Some(list) = self.props.get(&k) {
                if list
                    .iter()
                    .any(|&i| self.w.props[i].pos.distance(p) < r + 1.2)
                {
                    return false;
                }
            }
        }
        !self
            .w
            .landmarks
            .iter()
            .any(|l| l.pos.distance(p) < l.plaza + r + 2.0)
    }
}

/// Plans the town's underground (deterministic: from the town alone).
pub fn plan_underground(w: &TownWorld) -> Underground {
    let mut u = Underground::default();
    let (core, _) = w.spawn_line();
    let reach = (w.half() * 0.35).clamp(250.0, 700.0);
    let wet = |p: DVec2| w.ground(p) < DECK + 1.5;
    // Tunnels under the main roads of the centre.
    let mut tunnel_roads = Vec::new();
    for (ri, r) in w.roads.iter().enumerate().filter(|(_, r)| r.main) {
        let mut any = false;
        for s in r.pts.windows(2) {
            let (a, b) = (s[0], s[1]);
            let len = a.distance(b);
            if len < 0.5
                || a.distance(core) > reach
                || b.distance(core) > reach
                || wet(a)
                || wet(b)
                || wet((a + b) * 0.5)
            {
                continue;
            }
            let along = (b - a) / len;
            let v = Void {
                kind: VoidKind::Tunnel,
                origin: a,
                along,
                min: DVec2::new(-TUNNEL_HALF, -TUNNEL_HALF),
                max: DVec2::new(len + TUNNEL_HALF, TUNNEL_HALF),
                floor: [
                    w.road_height(a) - TUNNEL_DEPTH,
                    w.road_height(b) - TUNNEL_DEPTH,
                ],
                height: TUNNEL_HEIGHT,
                open: false,
            };
            if !roofed(w, &v) {
                continue;
            }
            u.voids.push(v);
            // A lamp on the ceiling every dozen metres.
            let n = (len / 12.0).floor() as usize;
            for k in 0..n {
                let p = a + along * (k as f64 * 12.0 + 6.0);
                u.lamps.push(DVec3::new(
                    p.x,
                    w.road_height(p) - TUNNEL_DEPTH + TUNNEL_HEIGHT - 0.4,
                    p.y,
                ));
            }
            any = true;
        }
        if any {
            tunnel_roads.push(ri);
        }
    }
    let tunnels = u.voids.len();
    u.index();
    let ground = Ground::new(w);
    // The tunnel point nearest `p`: (point, tunnel floor there).
    let nearest_tunnel = |u: &Underground, p: DVec2| -> Option<(DVec2, f64)> {
        let mut best: Option<(f64, DVec2, f64)> = None;
        for v in &u.voids[..tunnels] {
            let a = v.origin;
            let b = v.world(DVec2::new(v.max.x - TUNNEL_HALF, 0.0));
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
            let q = a + ab * t;
            let d = q.distance(p);
            if best.is_none_or(|x| d < x.0) {
                best = Some((d, q, v.floor_at(q)));
            }
        }
        best.map(|b| (b.1, b.2))
    };
    // Aether lifts down to the tunnels, beside the main roads, a few
    // hundred metres apart.
    let mut lifts: Vec<DVec2> = Vec::new();
    for &ri in &tunnel_roads {
        let r = &w.roads[ri];
        let mut since = 1e9;
        for s in r.pts.windows(2) {
            let (a, b) = (s[0], s[1]);
            let len = a.distance(b);
            let dir = (b - a).normalize_or_zero();
            let mut t = 0.0;
            while t < len {
                let p = a + dir * t;
                t += 4.0;
                since += 4.0;
                if since < 260.0
                    || p.distance(core) > reach - 10.0
                    || lifts.iter().any(|l| l.distance(p) < 320.0)
                {
                    continue;
                }
                for side in [1.0, -1.0] {
                    let off = r.half + r.pavement + SHAFT_HALF + 1.6;
                    let c = p + dir.perp() * side * off;
                    if !ground.free(c, SHAFT_HALF + 1.2) {
                        continue;
                    }
                    let Some((q, floor)) = nearest_tunnel(&u, c) else {
                        continue;
                    };
                    let top = w.ground(c);
                    if !add_lift(w, &mut u, c, q, floor) {
                        continue;
                    }
                    u.entrances.push(Entrance {
                        name: "Utility tunnels".into(),
                        pos: c,
                        door: -dir.perp() * side,
                        top,
                        bottom: floor,
                        molehaven: false,
                    });
                    lifts.push(c);
                    since = 0.0;
                    break;
                }
            }
        }
    }
    u.index();
    // Molehaven: under dry, open-ish ground not far from the tunnels.
    if let Some((site, hall_dir)) = cavern_site(w, core, reach, &ground) {
        let lowest = (0..16)
            .map(|k| {
                let a = std::f64::consts::TAU * k as f64 / 16.0;
                w.ground(site + DVec2::new(hw_core::det::cos(a), hw_core::det::sin(a)) * 45.0)
            })
            .fold(w.ground(site), f64::min);
        let floor = lowest - CAVERN_DEPTH;
        let mut chambers = Vec::new();
        // The great hall, and side chambers off it.
        let hall = DVec2::new(28.0, 20.0);
        let hall_void = Void {
            kind: VoidKind::Cavern,
            origin: site,
            along: hall_dir,
            min: -hall,
            max: hall,
            floor: [floor, floor],
            height: 16.0,
            open: false,
        };
        chambers.push(u.voids.len());
        u.voids.push(hall_void.clone());
        let sides = [
            (DVec2::new(hall.x + 9.0, 4.0), DVec2::new(11.0, 9.0), 10.0),
            (
                DVec2::new(-hall.x - 8.0, -5.0),
                DVec2::new(10.0, 10.0),
                11.0,
            ),
            (DVec2::new(6.0, hall.y + 8.0), DVec2::new(12.0, 10.0), 9.0),
            (DVec2::new(-10.0, -hall.y - 7.0), DVec2::new(9.0, 9.0), 9.0),
        ];
        for (at, half, h) in sides {
            let v = Void {
                kind: VoidKind::Cavern,
                origin: hall_void.world(at),
                along: hall_dir,
                min: -half - DVec2::splat(3.0),
                max: half,
                floor: [floor, floor],
                height: h,
                open: false,
            };
            if roofed(w, &v) {
                chambers.push(u.voids.len());
                u.voids.push(v);
            }
        }
        // Glowing mushrooms round the hall.
        for k in 0..18 {
            let a = std::f64::consts::TAU * k as f64 / 18.0;
            let l = DVec2::new(
                hw_core::det::cos(a) * (hall.x - 3.0),
                hw_core::det::sin(a) * (hall.y - 3.0),
            );
            let p = hall_void.world(l);
            u.lamps.push(DVec3::new(p.x, floor + 2.6, p.y));
        }
        // A ramp up to the nearest tunnel.
        if let Some((q, tf)) = nearest_tunnel(&u, site) {
            let edge = {
                // Leave the hall on the side facing the tunnel.
                let d = (q - site).normalize_or(hall_dir);
                let l = DVec2::new(d.dot(hall_dir), d.dot(hall_dir.perp()));
                let k = (hall.x / l.x.abs().max(1e-6)).min(hall.y / l.y.abs().max(1e-6)) - 2.0;
                site + d * k.max(1.0)
            };
            let len = edge.distance(q);
            if len > 5.0 {
                let along = (q - edge) / len;
                let ramp = Void {
                    kind: VoidKind::Passage,
                    origin: edge,
                    along,
                    min: DVec2::new(-2.0, -PASSAGE_HALF - 0.5),
                    max: DVec2::new(len + 1.0, PASSAGE_HALF + 0.5),
                    floor: [floor, tf],
                    height: 3.6,
                    open: false,
                };
                if roofed(w, &ramp) {
                    u.voids.push(ramp);
                }
            }
        }
        // Its own lift from the street, over a corner of the hall.
        u.index();
        let mut lift = None;
        'find: for ring in [0.0, 8.0, 14.0, 20.0, 26.0] {
            for k in 0..16 {
                let a = std::f64::consts::TAU * k as f64 / 16.0;
                let l = DVec2::new(
                    hw_core::det::cos(a) * ring,
                    hw_core::det::sin(a) * ring * 0.7,
                );
                let c = hall_void.world(l);
                if hall_void.covers(c, -SHAFT_HALF - 1.0) && ground.free(c, SHAFT_HALF + 1.2) {
                    lift = Some(c);
                    break 'find;
                }
            }
        }
        if let Some(c) = lift {
            let top = w.ground(c);
            let door = nearest_road_dir(w, c);
            u.voids.push(Void {
                kind: VoidKind::Shaft,
                origin: c - DVec2::splat(SHAFT_HALF),
                along: DVec2::X,
                min: DVec2::ZERO,
                max: DVec2::splat(SHAFT_HALF * 2.0),
                floor: [floor, floor],
                height: 0.0,
                open: true,
            });
            u.entrances.push(Entrance {
                name: "Molehaven".into(),
                pos: c,
                door,
                top,
                bottom: floor,
                molehaven: true,
            });
        }
        u.cavern = Some(Cavern {
            centre: site,
            floor,
            chambers,
        });
    }
    u.index();
    u
}

/// Does the ground cover the void with a roof at least 1.5 m thick?
fn roofed(w: &TownWorld, v: &Void) -> bool {
    if v.open {
        return true;
    }
    let size = v.max - v.min;
    let (nx, ny) = (
        (size.x / 4.0).ceil() as usize,
        (size.y / 4.0).ceil() as usize,
    );
    (0..=nx).all(|i| {
        (0..=ny).all(|j| {
            let l =
                v.min + DVec2::new(size.x * i as f64 / nx as f64, size.y * j as f64 / ny as f64);
            let p = v.world(l);
            v.ceiling_at(p).is_none_or(|c| c < w.ground(p) - 1.5)
        })
    })
}

/// A lift at `c` (ground `top`) down to the tunnel at `q` (floor `floor`),
/// with a passage between.
fn add_lift(w: &TownWorld, u: &mut Underground, c: DVec2, q: DVec2, floor: f64) -> bool {
    let len = c.distance(q);
    let passage = (len > 0.5).then(|| Void {
        kind: VoidKind::Passage,
        origin: c,
        along: (q - c) / len,
        min: DVec2::new(0.0, -PASSAGE_HALF),
        max: DVec2::new(len, PASSAGE_HALF),
        floor: [floor, floor],
        height: PASSAGE_HEIGHT,
        open: false,
    });
    if passage.as_ref().is_some_and(|p| !roofed(w, p)) {
        return false;
    }
    u.voids.push(Void {
        kind: VoidKind::Shaft,
        origin: c - DVec2::splat(SHAFT_HALF),
        along: DVec2::X,
        min: DVec2::ZERO,
        max: DVec2::splat(SHAFT_HALF * 2.0),
        floor: [floor, floor],
        height: 0.0,
        open: true,
    });
    u.voids.extend(passage);
    true
}

fn nearest_road_dir(w: &TownWorld, p: DVec2) -> DVec2 {
    let mut best: Option<(f64, DVec2)> = None;
    for r in &w.roads {
        for s in r.pts.windows(2) {
            let ab = s[1] - s[0];
            let t = ((p - s[0]).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
            let q = s[0] + ab * t;
            let d = q.distance(p);
            if best.is_none_or(|b| d < b.0) {
                best = Some((d, (q - p).normalize_or(DVec2::Y)));
            }
        }
    }
    best.map_or(DVec2::Y, |b| b.1)
}

/// A dry place for the cavern: high above the water all round, a few
/// hundred metres from the centre, with room for a lift on top.
fn cavern_site(w: &TownWorld, core: DVec2, reach: f64, ground: &Ground) -> Option<(DVec2, DVec2)> {
    let mut best: Option<(f64, DVec2)> = None;
    for ring in 0..6 {
        let r = 140.0 + ring as f64 * 70.0;
        if r > reach + 150.0 {
            break;
        }
        for k in 0..24 {
            let a = std::f64::consts::TAU * k as f64 / 24.0 + ring as f64 * 0.37;
            let p = core + DVec2::new(hw_core::det::cos(a), hw_core::det::sin(a)) * r;
            let mut low = f64::MAX;
            let mut high = f64::MIN;
            for j in 0..12 {
                let b = std::f64::consts::TAU * j as f64 / 12.0;
                for d in [20.0, 45.0, 60.0] {
                    let g =
                        w.ground(p + DVec2::new(hw_core::det::cos(b), hw_core::det::sin(b)) * d);
                    low = low.min(g);
                    high = high.max(g);
                }
            }
            if low < WATER_LEVEL + 4.0 {
                continue;
            }
            // Prefer flat ground with a free spot for the lift.
            let room = (0..8).any(|j| {
                let b = std::f64::consts::TAU * j as f64 / 8.0;
                ground.free(
                    p + DVec2::new(hw_core::det::cos(b), hw_core::det::sin(b)) * 12.0,
                    SHAFT_HALF + 1.2,
                )
            });
            let score = (high - low) + r * 0.02 + if room { 0.0 } else { 50.0 };
            if best.is_none_or(|b| score < b.0) {
                best = Some((score, p));
            }
        }
    }
    let (_, p) = best?;
    let d = (p - core).normalize_or(DVec2::X);
    Some((p, d.perp()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::urban::TownParams;
    use hw_core::Seed;

    #[test]
    fn a_town_has_tunnels_lifts_and_molehaven() {
        for seed in [1u64, 3, 11] {
            let w = TownWorld::generate(Seed(seed), TownParams::sample(Seed(seed), 40_000.0));
            let u = &w.underground;
            let tunnels = u
                .voids
                .iter()
                .filter(|v| v.kind == VoidKind::Tunnel)
                .count();
            assert!(tunnels > 20, "seed {seed}: {tunnels} tunnel pieces");
            assert!(
                u.entrances.iter().filter(|e| !e.molehaven).count() >= 2,
                "seed {seed}: {} lifts",
                u.entrances.len()
            );
            let c = u.cavern.as_ref().expect("Molehaven");
            eprintln!(
                "seed {seed}: {tunnels} tunnel pieces, {} lifts, Molehaven {} chambers, ramp {}",
                u.entrances.len(),
                c.chambers.len(),
                u.voids
                    .iter()
                    .any(|v| v.kind == VoidKind::Passage && v.floor[0] != v.floor[1])
            );
            assert!(
                u.entrances.iter().any(|e| e.molehaven),
                "seed {seed}: no Molehaven lift"
            );
            // Everything is below the ground over it, under a good roof.
            for v in &u.voids {
                for p in v.corners() {
                    let g = w.ground(p);
                    if let Some(top) = v.ceiling_at(p) {
                        assert!(
                            top < g - 1.5,
                            "seed {seed}: {:?} roof {top:.1} under ground {g:.1}",
                            v.kind
                        );
                    }
                }
            }
            assert!(c.floor < w.ground(c.centre) - 20.0);
            // A lift reaches the floor of what it leads to.
            for e in &u.entrances {
                let below = DVec3::new(e.pos.x, e.bottom + 1.0, e.pos.y);
                assert!(
                    u.open_at(below),
                    "seed {seed}: lift {} goes nowhere",
                    e.name
                );
                assert!(u.floor_for(e.pos, e.bottom + 1.0).is_some());
            }
        }
    }
}
