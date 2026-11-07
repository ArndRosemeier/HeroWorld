//! What heroes bump into: the city's buildings as stacks of boxes (one per
//! run of floors with the same outline, so towers keep their setbacks).
//!
//! Server and client use the same code and the same city, so a predicted
//! hero lands exactly where the server's does.

use glam::{DVec2, DVec3};
use hw_gen::building::BuildingPlan;
use hw_gen::building::BuildingSite;
use hw_gen::city::CityPlan;
use hw_gen::geom::Rect;
use hw_gen::urban::world::{TownWorld, WATER_LEVEL};
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

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

    pub fn to_world(&self, l: DVec2) -> DVec2 {
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

/// One building for collision: its plan, and its solids made on first use
/// (walls with door gaps, floor slabs, terraces, the roof). Only buildings
/// something comes near ever get built, on the server and every client alike.
#[derive(Debug)]
struct Collider {
    /// The building's plan, made from its site on first use.
    plan: OnceLock<Option<Arc<BuildingPlan>>>,
    site: Option<BuildingSite>,
    base: f64,
    /// Bottom of the plinth under a building on a slope.
    plinth: Option<f64>,
    lo: DVec2,
    hi: DVec2,
    solids: OnceLock<Vec<Solid>>,
}

impl Collider {
    fn plan(&self) -> Option<&Arc<BuildingPlan>> {
        self.plan
            .get_or_init(|| {
                self.site
                    .as_ref()
                    .map(|s| Arc::new(BuildingPlan::generate(s)))
            })
            .as_ref()
    }

    fn solids(&self) -> &[Solid] {
        self.solids.get_or_init(|| match self.plan() {
            Some(p) => {
                let mut out = building_solids(p, self.base);
                if let Some(y0) = self.plinth {
                    let s = &p.site;
                    out.push(Solid {
                        origin: s.origin,
                        along: s.along,
                        inward: s.inward,
                        min: DVec2::ZERO,
                        max: s.size,
                        y0,
                        y1: self.base,
                    });
                }
                out
            }
            None => Vec::new(),
        })
    }
}

/// Ground height by position, for worlds with terrain.
#[derive(Clone)]
pub struct Terrain(pub Arc<dyn Fn(DVec2) -> f64 + Send + Sync>);

impl std::fmt::Debug for Terrain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Terrain")
    }
}

/// All solids of a city with a grid to find nearby ones fast.
#[derive(Debug, Default)]
pub struct Obstacles {
    colliders: Vec<Collider>,
    grid: BTreeMap<(i64, i64), Vec<u32>>,
    /// Height of open ground.
    pub ground: f64,
    /// Rivers and sea: open ground there is the water surface.
    water: Option<Arc<WaterMap>>,
    /// Hills and valleys (open ground is then the terrain, or the water
    /// surface at `ground` where the terrain dips below it).
    terrain: Option<Terrain>,
}

/// Where the open ground is water, and the bridges over it.
#[derive(Debug)]
struct WaterMap {
    water: hw_gen::water::CityWater,
    /// Deck centre line ends and half width.
    bridges: Vec<(DVec2, DVec2, f64)>,
}

impl WaterMap {
    fn is_water(&self, p: DVec2) -> bool {
        self.water.is_water(p)
            && !self
                .bridges
                .iter()
                .any(|(a, b, w)| hw_gen::water::segment_distance(p, *a, *b) <= *w)
    }
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
/// Thickness of floor slabs, outside walls and inside walls.
pub const SLAB: f64 = 0.3;
pub const OUTER_WALL: f64 = 0.3;
pub const INNER_WALL: f64 = 0.2;

/// Where a point is inside a building.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Inside {
    /// Index into the city's buildings.
    pub building: usize,
    pub level: u32,
}

impl Obstacles {
    /// Open ground only.
    pub fn flat(ground: f64) -> Self {
        Self {
            ground,
            ..Default::default()
        }
    }

    /// Every building of a city (one plan per `city.buildings`, same order),
    /// standing on `base`.
    pub fn from_city(city: &CityPlan, plans: &[BuildingPlan], ground: f64, base: f64) -> Self {
        let mut out = Self::flat(ground);
        out.water = Some(Arc::new(WaterMap {
            water: city.water.clone(),
            bridges: city
                .bridges
                .iter()
                .map(|b| (b.a, b.b, city.streets[b.street as usize].reach()))
                .collect(),
        }));
        for (_, p) in city.buildings.iter().zip(plans) {
            let corners = p.site.corners();
            out.insert(Collider {
                plan: OnceLock::from(Some(Arc::new(p.clone()))),
                site: None,
                base,
                plinth: None,
                lo: corners
                    .iter()
                    .fold(DVec2::splat(f64::MAX), |a, c| a.min(*c)),
                hi: corners
                    .iter()
                    .fold(DVec2::splat(f64::MIN), |a, c| a.max(*c)),
                solids: OnceLock::new(),
            });
        }
        out
    }

    /// A town on its terrain: open ground follows `terrain` (water stands at
    /// `water`), each building stands at its own base on a plinth, bridge
    /// decks span the water. Building plans are made when first needed.
    pub fn from_town(world: &TownWorld, terrain: Terrain) -> Self {
        let mut out = Self::flat(WATER_LEVEL);
        out.terrain = Some(terrain);
        for b in &world.buildings {
            let corners = b.site.corners();
            out.insert(Collider {
                plan: OnceLock::new(),
                site: Some(b.site.clone()),
                base: b.base,
                plinth: (b.base - b.low > 0.05).then_some(b.low - 1.0),
                lo: corners
                    .iter()
                    .fold(DVec2::splat(f64::MAX), |a, c| a.min(*c)),
                hi: corners
                    .iter()
                    .fold(DVec2::splat(f64::MIN), |a, c| a.max(*c)),
                solids: OnceLock::new(),
            });
        }
        for br in &world.bridges {
            let d = br.b - br.a;
            let len = d.length();
            if len < 1e-6 {
                continue;
            }
            let along = d / len;
            let inward = along.perp();
            out.add(Solid {
                origin: br.a - inward * br.half,
                along,
                inward,
                min: DVec2::ZERO,
                max: DVec2::new(len, br.half * 2.0),
                y0: br.deck - 0.6,
                y1: br.deck,
            });
        }
        out
    }

    /// Adds a free-standing box (tests, props).
    pub fn add(&mut self, solid: Solid) {
        let (mut lo, mut hi) = (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN));
        for c in solid.corners() {
            lo = lo.min(c);
            hi = hi.max(c);
        }
        let cell = OnceLock::new();
        let _ = cell.set(vec![solid]);
        self.insert(Collider {
            plan: OnceLock::from(None),
            site: None,
            base: 0.0,
            plinth: None,
            lo,
            hi,
            solids: cell,
        });
    }

    fn insert(&mut self, c: Collider) {
        let idx = self.colliders.len() as u32;
        let (x0, z0) = (
            (c.lo.x / CELL).floor() as i64,
            (c.lo.y / CELL).floor() as i64,
        );
        let (x1, z1) = (
            (c.hi.x / CELL).floor() as i64,
            (c.hi.y / CELL).floor() as i64,
        );
        for x in x0..=x1 {
            for z in z0..=z1 {
                self.grid.entry((x, z)).or_default().push(idx);
            }
        }
        self.colliders.push(c);
    }

    /// Colliders whose boxes come within `r` of `p`, sorted.
    fn near_colliders(&self, p: DVec2, r: f64) -> Vec<u32> {
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
                    out.extend(v.iter().copied().filter(|&i| {
                        let c = &self.colliders[i as usize];
                        p.x + r >= c.lo.x
                            && p.x - r <= c.hi.x
                            && p.y + r >= c.lo.y
                            && p.y - r <= c.hi.y
                    }));
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Every solid near a point (builds colliders on first touch).
    fn near(&self, p: DVec2, r: f64) -> impl Iterator<Item = &Solid> {
        self.near_colliders(p, r)
            .into_iter()
            .flat_map(move |i| self.colliders[i as usize].solids().iter())
    }

    /// The solids of every collider within `r` of `p`, by collider index
    /// (building colliders are made on first touch). For the physics world.
    pub fn solids_near(&self, p: DVec2, r: f64) -> Vec<(u32, &[Solid])> {
        self.near_colliders(p, r)
            .into_iter()
            .map(|i| (i, self.colliders[i as usize].solids()))
            .collect()
    }

    /// Does open ground follow terrain or water (not one flat height)?
    pub fn has_relief(&self) -> bool {
        self.terrain.is_some() || self.water.is_some()
    }

    /// Is `p` inside any solid?
    pub fn solid_at(&self, p: DVec3) -> bool {
        self.near(DVec2::new(p.x, p.z), 0.0).any(|s| {
            let l = s.to_local(DVec2::new(p.x, p.z));
            p.y > s.y0
                && p.y < s.y1
                && l.x > s.min.x
                && l.x < s.max.x
                && l.y > s.min.y
                && l.y < s.max.y
        })
    }

    /// The building and floor a point is in, if any.
    pub fn inside(&self, p: DVec3) -> Option<Inside> {
        let q = DVec2::new(p.x, p.z);
        for i in self.near_colliders(q, 0.0) {
            let c = &self.colliders[i as usize];
            let Some(plan) = c.plan() else { continue };
            let s = &plan.site;
            let d = q - s.origin;
            let l = DVec2::new(d.dot(s.along), d.dot(s.inward));
            if l.x < 0.0 || l.y < 0.0 || l.x > s.size.x || l.y > s.size.y {
                continue;
            }
            let y = p.y - c.base;
            if let Some(f) = plan
                .floors
                .iter()
                .find(|f| y >= f.elevation - 0.05 && y < f.elevation + f.height)
            {
                if f.outline.min.x <= l.x
                    && l.x <= f.outline.max.x
                    && f.outline.min.y <= l.y
                    && l.y <= f.outline.max.y
                {
                    return Some(Inside {
                        building: i as usize,
                        level: f.level,
                    });
                }
            }
        }
        None
    }

    /// The plan of building `i` (as [`Inside::building`] counts), made on
    /// first use, and the height it stands at.
    pub fn building(&self, i: usize) -> Option<(Arc<BuildingPlan>, f64)> {
        let c = self.colliders.get(i)?;
        c.plan().map(|p| (p.clone(), c.base))
    }

    /// Is `p` (feet) inside a lift shaft?
    pub fn in_lift(&self, p: DVec3) -> bool {
        let q = DVec2::new(p.x, p.z);
        self.near_colliders(q, 0.0).into_iter().any(|i| {
            let c = &self.colliders[i as usize];
            let Some(plan) = c.plan() else { return false };
            let Some(core) = plan.core else { return false };
            let s = &plan.site;
            let d = q - s.origin;
            let l = DVec2::new(d.dot(s.along), d.dot(s.inward));
            let top = c.base + plan.height();
            l.x > core.min.x
                && l.x < core.max.x
                && l.y > core.min.y
                && l.y < core.max.y
                && p.y >= c.base - 0.1
                && p.y < top
        })
    }

    /// Height of the open ground at `p`: the water surface over rivers and
    /// the sea (bridges excepted).
    pub fn ground_at(&self, p: DVec2) -> f64 {
        if let Some(t) = &self.terrain {
            return (t.0)(p).max(self.ground);
        }
        match &self.water {
            Some(w) if w.is_water(p) => hw_gen::water::WATER_LEVEL,
            _ => self.ground,
        }
    }

    /// Height of whatever is under `p` (x, z) no higher than `max_y`.
    pub fn floor_under(&self, p: DVec2, max_y: f64) -> f64 {
        let mut best = self.ground_at(p);
        for s in self.near(p, 0.0) {
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
            for s in self.near(DVec2::new(pos.x, pos.z), BODY_RADIUS) {
                // Only walls: the body overlaps the box vertically, is not
                // standing on top of it (or stepping up onto it), and it is not a
                // ceiling just above the head (handled in move_body).
                if pos.y + BODY_HEIGHT <= s.y0
                    || pos.y >= s.y1 - STEP
                    || s.y0 > pos.y + BODY_HEIGHT - 0.6
                {
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
            // Quay walls: the bank stands too high to step onto from the water.
            let q = DVec2::new(next.x, next.z);
            if self.ground_at(q) > body.pos.y.max(next.y) + STEP {
                next.x = body.pos.x;
                next.z = body.pos.z;
                body.vel.x = 0.0;
                body.vel.z = 0.0;
            }
            // Ceilings stop a rising body.
            if body.vel.y > 0.0 {
                let head = next.y + BODY_HEIGHT;
                let q = DVec2::new(next.x, next.z);
                for s in self.near(q, 0.0) {
                    let l = s.to_local(q);
                    let over = l.x >= s.min.x && l.x <= s.max.x && l.y >= s.min.y && l.y <= s.max.y;
                    if over && s.y0 >= body.pos.y + BODY_HEIGHT - 0.6 && head > s.y0 {
                        next.y = s.y0 - BODY_HEIGHT;
                        body.vel.y = 0.0;
                    }
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

/// The solids of one building, from its plan.
fn building_solids(plan: &BuildingPlan, base: f64) -> Vec<Solid> {
    let site = &plan.site;
    let mut out = Vec::new();
    let mut push = |r: Rect, y0: f64, y1: f64| {
        if r.size().x > 1e-6 && r.size().y > 1e-6 && y1 > y0 {
            out.push(Solid {
                origin: site.origin,
                along: site.along,
                inward: site.inward,
                min: r.min,
                max: r.max,
                y0,
                y1,
            });
        }
    };
    let n = plan.floors.len();
    for (k, f) in plan.floors.iter().enumerate() {
        let y = base + f.elevation;
        let top = if k + 1 < n {
            y + f.height - SLAB
        } else {
            y + f.height
        };
        // The slab under this floor covers the floor below (its terrace too),
        // with a hole for the lift shaft.
        if k > 0 {
            let below = plan.floors[k - 1].outline;
            for r in minus(below, plan.core) {
                push(r, y - SLAB, y);
            }
        }
        // Walls: every room edge, minus the doors on it. Shared edges come
        // twice; that is harmless for collision.
        for (ri, room) in f.rooms.iter().enumerate() {
            let r = room.rect;
            let edges = [
                (true, r.min.y, r.min.x, r.max.x),
                (true, r.max.y, r.min.x, r.max.x),
                (false, r.min.x, r.min.y, r.max.y),
                (false, r.max.x, r.min.y, r.max.y),
            ];
            for (along_x, fixed, t0, t1) in edges {
                let outer = if along_x {
                    (fixed - f.outline.min.y).abs() < 1e-6 || (fixed - f.outline.max.y).abs() < 1e-6
                } else {
                    (fixed - f.outline.min.x).abs() < 1e-6 || (fixed - f.outline.max.x).abs() < 1e-6
                };
                let mut gaps: Vec<(f64, f64)> = f
                    .doors
                    .iter()
                    .filter(|d| (d.a == ri || d.b == Some(ri)) && d.along_x == along_x)
                    .filter(|d| (if along_x { d.at.y } else { d.at.x } - fixed).abs() < 1e-6)
                    .map(|d| {
                        let c = if along_x { d.at.x } else { d.at.y };
                        (c - d.size.x * 0.5, c + d.size.x * 0.5)
                    })
                    .collect();
                gaps.sort_by(|a, b| a.0.total_cmp(&b.0));
                let half = if outer { OUTER_WALL } else { INNER_WALL } * 0.5;
                let mut t = t0;
                let mut pieces = Vec::new();
                for (g0, g1) in gaps {
                    if g0 > t {
                        pieces.push((t, g0.min(t1)));
                    }
                    t = t.max(g1);
                }
                if t < t1 {
                    pieces.push((t, t1));
                }
                for (a, b) in pieces {
                    if b - a < 0.05 {
                        continue;
                    }
                    let rect = if along_x {
                        Rect::from_xz(a, fixed - half, b, fixed + half)
                    } else {
                        Rect::from_xz(fixed - half, a, fixed + half, b)
                    };
                    push(rect, y, top);
                }
            }
        }
        if k + 1 == n {
            push(f.outline, y + f.height - SLAB, y + f.height);
        }
    }
    out
}

/// A rectangle minus an optional hole, as up to four rectangles.
fn minus(r: Rect, hole: Option<Rect>) -> Vec<Rect> {
    let Some(h) = hole else { return vec![r] };
    if h.min.x >= r.max.x || h.max.x <= r.min.x || h.min.y >= r.max.y || h.max.y <= r.min.y {
        return vec![r];
    }
    let (hx0, hx1) = (h.min.x.max(r.min.x), h.max.x.min(r.max.x));
    let (hz0, hz1) = (h.min.y.max(r.min.y), h.max.y.min(r.max.y));
    vec![
        Rect::from_xz(r.min.x, r.min.y, r.max.x, hz0),
        Rect::from_xz(r.min.x, hz1, r.max.x, r.max.y),
        Rect::from_xz(r.min.x, hz0, hx0, hz1),
        Rect::from_xz(hx1, hz0, r.max.x, hz1),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rivers_are_water_and_their_banks_too_high_to_step_onto() {
        let city = CityPlan::generate(hw_core::Seed(1), &hw_gen::city::CityParams::default());
        let o = Obstacles::from_city(&city, &[], 0.0, 0.0);
        let river = &city.water.rivers[0];
        let (a, b) = (
            river.pts[river.pts.len() / 2],
            river.pts[river.pts.len() / 2 + 1],
        );
        let dir = (b - a).normalize();
        let side = dir.perp();
        // Mid-river, away from any bridge.
        let mut mid = a.lerp(b, 0.5);
        for k in 0..40 {
            let p = a.lerp(b, k as f64 / 40.0);
            if o.ground_at(p) < 0.0 {
                mid = p;
                break;
            }
        }
        assert_eq!(o.ground_at(mid), hw_gen::water::WATER_LEVEL);
        let bank = mid + side * (river.width * 0.5 + 5.0);
        assert_eq!(o.ground_at(bank), 0.0);
        // Swim towards the bank: the quay wall stops the body.
        let mut body = Body {
            pos: DVec3::new(mid.x, hw_gen::water::WATER_LEVEL, mid.y),
            vel: DVec3::new(side.x, 0.0, side.y) * 5.0,
            grounded: true,
        };
        for _ in 0..200 {
            body.vel = DVec3::new(side.x, body.vel.y - 9.8 / 30.0, side.y) * 1.0;
            body.vel.x *= 5.0;
            body.vel.z *= 5.0;
            o.move_body(&mut body, 1.0 / 30.0);
        }
        assert!(body.pos.y < -3.0, "climbed the quay: {:?}", body.pos);
        let reached = DVec2::new(body.pos.x, body.pos.z).distance(mid);
        assert!(reached < river.width * 0.5 + 1.0, "went {reached} m");
    }

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
