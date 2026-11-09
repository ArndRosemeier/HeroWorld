//! The deep underground, drawn tile by tile around the camera: the Aether
//! Tube's cream-and-brass tubes and its tiled, vaulted stations, the
//! buried city's columns and domed vault, the Doctor's lab with its coils,
//! vats, ray cannon and reactor, and the Deeps' grottos of crystals, giant
//! mushrooms, glow lakes and fossils (their rock is Engine's carved voxel
//! ground, [`density`]).
//!
//! Tiles are built on a worker thread from `hw_gen`'s plans and dropped
//! again as the camera leaves, so the deep layers cost the same under a
//! village as under a megacity. What stands on the street (station
//! pavilions, the kiosk) is spawned for every loaded tile; the rooms below
//! only while the camera is down there or at an opening.

use crate::carmesh::{lathe, slab, surface};
use crate::citymesh::{MeshBuilder, Surf};
use crate::skins::Tex;
use crate::undergroundview::{self as uv, v3};
use engine::prelude::*;
use engine::texgen::{FacadeLayer, GroundLayer, MedievalLayer, NaturalLayer};
use glam::{DVec2, DVec3, Vec3};
use hw_gen::urban::deep::{
    tile_of, DeepTile, EntranceKind, Fixture, FixtureKind, TileKey, TILE, TUBE_HALF,
};
use hw_gen::urban::underground::{Void, VoidKind, SHAFT_HALF};
use hw_gen::urban::world::TownWorld;
use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::{FRAC_PI_2, PI, TAU};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

/// Tiles are loaded within this distance of the camera (plan), dropped
/// beyond the second.
const LOAD: f64 = 380.0;
const DROP: f64 = 560.0;
/// Lamps lit at once.
const POOL: usize = 28;
/// Lining cells (m).
const STEP: f64 = 1.5;
/// Radius of the tube's shell, and its axis above the walkway.
const SHELL: f64 = 1.9;
const AXIS: f64 = 1.0;

/// Engine's density with the whole underground carved out of `ground`:
/// the shallow layer and the deep tiles near `p`.
pub fn density(town: &TownWorld, p: Vec3, ground: f32) -> f32 {
    let mut d = uv::density(town.core_underground(), p, ground);
    if p.y > ground + 0.5 {
        return d;
    }
    let q = DVec2::new(p.x as f64, p.z as f64);
    for t in town.deep_near(q, 3.0) {
        for i in t.near(q, 3.0) {
            d = d.min(uv::carve(&t.voids[i], p.as_dvec3()) as f32);
        }
    }
    d
}

#[derive(Clone, Copy)]
struct Glow {
    pos: DVec3,
    color: Color,
    strength: f32,
    range: f32,
}

/// A tile's meshes, made on the worker.
struct Built {
    key: TileKey,
    inner: Vec<Mesh>,
    outer: Vec<Mesh>,
    glows: Vec<Glow>,
    /// Openings to the street: where the camera counts as "at" the deep
    /// underground though above ground.
    openings: Vec<DVec2>,
}

struct Loaded {
    inner_meshes: Vec<Mesh>,
    inner: Vec<EntityId>,
    outer: Vec<EntityId>,
    glows: Vec<Glow>,
    openings: Vec<DVec2>,
}

pub struct DeepView {
    town: Arc<TownWorld>,
    tiles: BTreeMap<TileKey, Loaded>,
    asked: BTreeSet<TileKey>,
    jobs: Sender<TileKey>,
    done: Receiver<Built>,
    inside: bool,
    lit: Vec<LampId>,
    at: Option<(i32, i32, i32)>,
}

impl DeepView {
    pub fn new(town: Arc<TownWorld>) -> Self {
        let (jobs, rx) = channel::<TileKey>();
        let (tx, done) = channel::<Built>();
        let worker_town = town.clone();
        std::thread::Builder::new()
            .name("deep tiles".into())
            .spawn(move || {
                while let Ok(k) = rx.recv() {
                    let t = worker_town.deep_tile(k);
                    if tx.send(build(&worker_town, &t)).is_err() {
                        break;
                    }
                }
            })
            .expect("deep tile worker");
        Self {
            town,
            tiles: BTreeMap::new(),
            asked: BTreeSet::new(),
            jobs,
            done,
            inside: false,
            lit: Vec::new(),
            at: None,
        }
    }

    /// Builds the tiles round `eye` now (for the first frame and
    /// screenshots), then streams as usual.
    pub fn prime(&mut self, world: &mut World, eye: Vec3) -> EngineResult<()> {
        let q = DVec2::new(eye.x as f64, eye.z as f64);
        for k in self.wanted(q) {
            if !self.tiles.contains_key(&k) {
                let t = self.town.deep_tile(k);
                let b = build(&self.town, &t);
                self.install(world, b);
            }
        }
        self.update(world, eye)
    }

    fn wanted(&self, q: DVec2) -> Vec<TileKey> {
        let (a, b) = (
            tile_of(q - DVec2::splat(LOAD + TILE)),
            tile_of(q + DVec2::splat(LOAD + TILE)),
        );
        let mut out = Vec::new();
        for x in a.0..=b.0 {
            for y in a.1..=b.1 {
                let c = hw_gen::urban::deep::tile_centre((x, y));
                let near = (q - c).abs() - DVec2::splat(TILE * 0.5);
                if near.max(DVec2::ZERO).length() < LOAD && self.town.deep_in_land((x, y)) {
                    out.push((x, y));
                }
            }
        }
        out.sort_by_key(|k| {
            let c = hw_gen::urban::deep::tile_centre(*k);
            (c.distance(q) * 10.0) as i64
        });
        out
    }

    fn install(&mut self, world: &mut World, b: Built) {
        let mut outer = Vec::new();
        for mesh in b.outer {
            let id = world.spawn(mesh);
            world.set_name(id, format!("deep street {} {}", b.key.0, b.key.1));
            outer.push(id);
        }
        let mut l = Loaded {
            inner_meshes: b.inner,
            inner: Vec::new(),
            outer,
            glows: b.glows,
            openings: b.openings,
        };
        if self.inside {
            spawn_inner(world, b.key, &mut l);
        }
        self.asked.remove(&b.key);
        self.tiles.insert(b.key, l);
    }

    /// Streams tiles round the camera, shows the rooms while it is below,
    /// and lights the lamps near it.
    pub fn update(&mut self, world: &mut World, eye: Vec3) -> EngineResult<()> {
        let q = DVec2::new(eye.x as f64, eye.z as f64);
        while let Ok(b) = self.done.try_recv() {
            let c = hw_gen::urban::deep::tile_centre(b.key);
            if c.distance(q) < DROP + TILE {
                self.install(world, b);
            } else {
                self.asked.remove(&b.key);
            }
        }
        for k in self.wanted(q) {
            if !self.tiles.contains_key(&k) && self.asked.insert(k) {
                let _ = self.jobs.send(k);
            }
        }
        let far: Vec<TileKey> = self
            .tiles
            .keys()
            .copied()
            .filter(|k| hw_gen::urban::deep::tile_centre(*k).distance(q) > DROP + TILE * 0.5)
            .collect();
        for k in far {
            if let Some(mut l) = self.tiles.remove(&k) {
                despawn(world, &mut l);
            }
        }
        // Below ground, or at an opening to the street.
        let ground = self.town.ground(q);
        let at_opening = self
            .tiles
            .values()
            .any(|l| l.openings.iter().any(|o| o.distance(q) < 30.0))
            && (eye.y as f64) < ground + 40.0;
        let inside = (eye.y as f64) < ground - 0.5 || at_opening;
        if inside != self.inside {
            self.inside = inside;
            for (k, l) in self.tiles.iter_mut() {
                if inside {
                    spawn_inner(world, *k, l);
                } else {
                    for id in l.inner.drain(..) {
                        world.despawn(id);
                    }
                }
            }
            if !inside {
                self.clear_lamps(world);
            }
        }
        if inside {
            // Tiles that came in while inside got their rooms in install.
            for (k, l) in self.tiles.iter_mut() {
                if l.inner.is_empty() && !l.inner_meshes.is_empty() {
                    spawn_inner(world, *k, l);
                }
            }
            self.light(world, eye)?;
        }
        Ok(())
    }

    fn light(&mut self, world: &mut World, eye: Vec3) -> EngineResult<()> {
        let key = (
            (eye.x / 5.0).round() as i32,
            (eye.y / 5.0).round() as i32,
            (eye.z / 5.0).round() as i32,
        );
        if self.at == Some(key) {
            return Ok(());
        }
        self.at = Some(key);
        let e = eye.as_dvec3();
        let mut near: Vec<(f64, Glow)> = self
            .tiles
            .values()
            .flat_map(|l| l.glows.iter())
            .map(|g| (g.pos.distance(e), *g))
            .filter(|(d, _)| *d < 80.0)
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.truncate(POOL);
        for (k, (_, g)) in near.iter().enumerate() {
            let lamp = Lamp::point(g.pos.as_vec3(), g.color, g.strength, g.range);
            match self.lit.get(k) {
                Some(&id) => world.set_lamp(id, lamp)?,
                None => self.lit.push(world.add_lamp(lamp)?),
            }
        }
        for id in self.lit.drain(near.len()..) {
            world.remove_lamp(id);
        }
        Ok(())
    }

    fn clear_lamps(&mut self, world: &mut World) {
        for id in self.lit.drain(..) {
            world.remove_lamp(id);
        }
        self.at = None;
    }

    /// Takes everything out of the world.
    pub fn clear(&mut self, world: &mut World) {
        self.clear_lamps(world);
        for (_, mut l) in std::mem::take(&mut self.tiles) {
            despawn(world, &mut l);
        }
    }

    /// Tiles loaded now (bounded by the load distance).
    pub fn loaded(&self) -> usize {
        self.tiles.len()
    }
}

fn spawn_inner(world: &mut World, k: TileKey, l: &mut Loaded) {
    if !l.inner.is_empty() {
        return;
    }
    for mesh in &l.inner_meshes {
        let id = world.spawn(mesh.clone());
        world.set_name(id, format!("deep {} {}", k.0, k.1));
        l.inner.push(id);
    }
}

fn despawn(world: &mut World, l: &mut Loaded) {
    for id in l.inner.drain(..).chain(l.outer.drain(..)) {
        world.despawn(id);
    }
}

/// A lamp's light colour: linear values, as light maths wants them.
fn col(c: [f32; 3]) -> Color {
    Color {
        r: c[0],
        g: c[1],
        b: c[2],
        a: 1.0,
    }
}

/// Is `p` in the open space of a void other than tile `key`'s void `i`?
fn elsewhere(town: &TownWorld, key: TileKey, i: usize, p: DVec3) -> bool {
    let q = DVec2::new(p.x, p.z);
    if town.core_underground().open_at(p) {
        return true;
    }
    town.deep_near(q, 0.0).iter().any(|t| {
        t.at(q)
            .any(|(j, v)| !(t.key == key && j == i) && v.contains(p))
    })
}

/// Everything of one tile.
fn build(town: &TownWorld, t: &DeepTile) -> Built {
    let mut inner = MeshBuilder::default();
    let mut outer = MeshBuilder::default();
    let mut glows: Vec<Glow> = t
        .lamps
        .iter()
        .map(|l| Glow {
            pos: l.pos,
            color: col(l.color),
            strength: l.strength,
            range: l.range,
        })
        .collect();
    let net = town.tube();
    for (i, v) in t.voids.iter().enumerate() {
        let other = |p: DVec3| elsewhere(town, t.key, i, p);
        match v.kind {
            VoidKind::Tube => tube_shell(&mut inner, v, &other),
            VoidKind::Station => station_hall(&mut inner, v, &other),
            VoidKind::Ruin => ruin_room(&mut inner, v, &other),
            VoidKind::Lair => lab_room(&mut inner, v, &other),
            VoidKind::Shaft | VoidKind::Passage => {
                uv::lining_of(
                    &mut inner,
                    v,
                    town,
                    |mid, f| {
                        !t.at(mid).any(|(j, o)| {
                            j < i && (o.floor_at(mid) - f).abs() < 0.6 && o.kind != VoidKind::Shaft
                        })
                    },
                    other,
                );
                if v.kind == VoidKind::Shaft && !v.open {
                    // A closed lift: a roof over the top.
                    let c = v.world((v.min + v.max) * 0.5);
                    let top = v.floor[0] + v.height;
                    inner.tex = Some(FacadeLayer::Concrete.into());
                    let k = v.corners();
                    inner.quad(k.map(|p| v3(p, top)), -DVec3::Y, rgb(150, 148, 144));
                    inner.tex = None;
                    let _ = c;
                }
            }
            _ => {}
        }
    }
    // Lift rings where the inner lifts stop.
    for l in &t.lifts {
        for y in [l.bottom, l.top] {
            uv::lift_ring(&mut inner, l.pos, y, rgb(130, 230, 255));
        }
    }
    for f in &t.fixtures {
        fixture(&mut inner, f, &mut glows);
    }
    // The lab's high lamps hang from the ceiling in enamel shades.
    for l in &t.lamps {
        let q = DVec2::new(l.pos.x, l.pos.z);
        let lab = t.at(q).find(|(_, v)| {
            v.kind == VoidKind::Lair && l.pos.y > v.floor_at(q) + 6.0 && v.height >= 9.0
        });
        if let Some((_, v)) = lab {
            hanging_lamp(&mut inner, l.pos, v.floor_at(q) + v.height);
        }
    }
    // Buffer stops where a line ends.
    for (li, line) in net.lines.iter().enumerate() {
        for e in [0, line.pts.len() - 1] {
            if tile_of(line.pts[e]) == t.key && net.is_end(li, e) {
                let o = if e == 0 { 1 } else { e - 1 };
                buffer_stop(
                    &mut inner,
                    line.pts[e],
                    (line.pts[e] - line.pts[o]).normalize_or(DVec2::X),
                    line.floors[e],
                    &mut glows,
                );
            }
        }
    }
    let mut openings = Vec::new();
    for e in &t.entrances {
        openings.push(e.pos);
        let (glow_col, roof) = match e.kind {
            EntranceKind::Tube => (rgb(255, 214, 130), rgb(196, 52, 44)),
            EntranceKind::Kiosk => (rgb(255, 120, 90), rgb(60, 92, 70)),
        };
        match e.kind {
            EntranceKind::Tube => {
                uv::pavilion_in(&mut outer, e.pos, e.door, e.top, roof, glow_col);
                uv::collar(&mut outer, e.pos, e.door, town);
            }
            EntranceKind::Kiosk => kiosk(&mut outer, e.pos, e.door, e.top),
        }
        uv::lift_ring(&mut outer, e.pos, e.top, glow_col);
        uv::lift_ring(&mut inner, e.pos, e.bottom, glow_col);
        glows.push(Glow {
            pos: DVec3::new(e.pos.x, e.bottom + 2.6, e.pos.y),
            color: glow_col,
            strength: 6.0,
            range: 10.0,
        });
    }
    Built {
        key: t.key,
        inner: inner.finish_parts(),
        outer: outer.finish_parts(),
        glows,
        openings,
    }
}

// ------------------------------------------------------------ rooms

/// How a box room is finished.
struct Finish {
    floor: (Option<Tex>, Color),
    wall: (Option<Tex>, Color),
    /// Walls stop here above the floor (None: up to the ceiling).
    wall_h: Option<f64>,
    ceiling: Option<(Option<Tex>, Color)>,
    /// A band round the walls: bottom, top, colour, surface.
    band: Option<(f64, f64, Color, Surf)>,
    /// A skirting: height and colour.
    dado: Option<(f64, Color)>,
}

/// Floor, walls (where rock is behind them) and ceiling of a box room.
fn box_room(b: &mut MeshBuilder, v: &Void, fin: &Finish, other: &dyn Fn(DVec3) -> bool) {
    let size = v.max - v.min;
    let (nx, ny) = (
        (size.x / STEP).ceil().max(1.0) as usize,
        (size.y / STEP).ceil().max(1.0) as usize,
    );
    let at = |x: f64, y: f64| v.world(v.min + DVec2::new(size.x * x, size.y * y));
    for ix in 0..nx {
        for iy in 0..ny {
            let (x0, x1) = (ix as f64 / nx as f64, (ix + 1) as f64 / nx as f64);
            let (y0, y1) = (iy as f64 / ny as f64, (iy + 1) as f64 / ny as f64);
            let corners = [at(x0, y0), at(x1, y0), at(x1, y1), at(x0, y1)];
            b.tex = fin.floor.0;
            b.quad(corners.map(|c| v3(c, v.floor_at(c))), DVec3::Y, fin.floor.1);
            if let Some((tex, c)) = fin.ceiling {
                let mid = at((x0 + x1) * 0.5, (y0 + y1) * 0.5);
                let top = v.floor_at(mid) + v.height;
                if !other(v3(mid, top + 0.4)) {
                    b.tex = tex;
                    b.quad(
                        corners.map(|p| v3(p, v.floor_at(p) + v.height)),
                        -DVec3::Y,
                        c,
                    );
                }
            }
            b.tex = None;
        }
    }
    let sides: [(DVec2, DVec2, DVec2); 4] = [
        (v.min, DVec2::new(v.max.x, v.min.y), DVec2::new(0.0, -1.0)),
        (DVec2::new(v.min.x, v.max.y), v.max, DVec2::new(0.0, 1.0)),
        (v.min, DVec2::new(v.min.x, v.max.y), DVec2::new(-1.0, 0.0)),
        (DVec2::new(v.max.x, v.min.y), v.max, DVec2::new(1.0, 0.0)),
    ];
    let wall_h = fin.wall_h.unwrap_or(v.height);
    for (a, c, out_l) in sides {
        let len = a.distance(c);
        let n = (len / STEP).ceil().max(1.0) as usize;
        let out = (v.along * out_l.x + v.inward() * out_l.y).normalize();
        let inw = -v3(out, 0.0);
        for k in 0..n {
            let p0 = v.world(a.lerp(c, k as f64 / n as f64));
            let p1 = v.world(a.lerp(c, (k + 1) as f64 / n as f64));
            let m = (p0 + p1) * 0.5;
            let f = v.floor_at(m);
            if other(v3(m + out * 0.4, f + 1.2)) {
                continue;
            }
            let (f0, f1) = (v.floor_at(p0), v.floor_at(p1));
            b.tex = fin.wall.0;
            b.quad(
                [
                    v3(p0, f0),
                    v3(p1, f1),
                    v3(p1, f1 + wall_h),
                    v3(p0, f0 + wall_h),
                ],
                inw,
                fin.wall.1,
            );
            b.tex = None;
            let inset = -out * 0.012;
            if let Some((h, dc)) = fin.dado {
                b.quad(
                    [
                        v3(p0 + inset, f0),
                        v3(p1 + inset, f1),
                        v3(p1 + inset, f1 + h),
                        v3(p0 + inset, f0 + h),
                    ],
                    inw,
                    dc,
                );
            }
            if let Some((lo, hi, bc, surf)) = fin.band {
                if hi <= wall_h {
                    b.surf = surf;
                    b.quad(
                        [
                            v3(p0 + inset * 1.5, f0 + lo),
                            v3(p1 + inset * 1.5, f1 + lo),
                            v3(p1 + inset * 1.5, f1 + hi),
                            v3(p0 + inset * 1.5, f0 + hi),
                        ],
                        inw,
                        bc,
                    );
                    b.surf = Surf::Plain;
                }
            }
        }
    }
}

/// The walkway and shell of a tube stretch: a cream enamelled tube with
/// brass ribs and a lit crown, open where gates and junctions meet it.
fn tube_shell(b: &mut MeshBuilder, v: &Void, other: &dyn Fn(DVec3) -> bool) {
    let half = v.max.y;
    let len = v.max.x - half;
    if len < 0.5 {
        return;
    }
    let side = v.inward();
    let n = (len / 2.0).ceil().max(1.0) as usize;
    // The arc above the walkway's edges.
    let a0 = -(AXIS / SHELL).asin();
    let a1 = PI - a0;
    let segs = 22;
    let point = |x: f64, a: f64, r: f64| {
        let p = v.world(DVec2::new(x, 0.0));
        let f = v.floor_at(p);
        v3(p + side * (r * a.cos()), f + AXIS + r * a.sin())
    };
    let cream = rgb(232, 224, 200);
    let rim = rgb(168, 60, 48);
    for i in 0..n {
        let (x0, x1) = (len * i as f64 / n as f64, len * (i + 1) as f64 / n as f64);
        for j in 0..segs {
            let (b0, b1) = (
                a0 + (a1 - a0) * j as f64 / segs as f64,
                a0 + (a1 - a0) * (j + 1) as f64 / segs as f64,
            );
            let mid = point((x0 + x1) * 0.5, (b0 + b1) * 0.5, SHELL + 0.45);
            if other(mid) {
                continue;
            }
            let c = if j < 2 || j >= segs - 2 { rim } else { cream };
            let axis_mid = point((x0 + x1) * 0.5, FRAC_PI_2, 0.0);
            let pts = [
                point(x0, b0, SHELL),
                point(x1, b0, SHELL),
                point(x1, b1, SHELL),
                point(x0, b1, SHELL),
            ];
            let centre = (pts[0] + pts[2]) * 0.5;
            b.quad(pts, axis_mid - centre, c);
        }
        // The walkway: a ribbed metal plate.
        let w = (SHELL * SHELL - AXIS * AXIS).sqrt();
        b.surf = Surf::Metal;
        b.quad(
            [
                point(x0, PI, w),
                point(x1, PI, w),
                point(x1, 0.0, w),
                point(x0, 0.0, w),
            ]
            .map(|p| p - DVec3::Y * (AXIS - 0.02)),
            DVec3::Y,
            rgb(96, 98, 104),
        );
        b.surf = Surf::Plain;
    }
    // Brass ribs every four metres.
    let ribs = (len / 4.0).floor() as usize;
    for k in 0..=ribs {
        let x = (k as f64 * 4.0).min(len);
        let ring: Vec<DVec3> = (0..=24)
            .map(|s| point(x, a0 + (a1 - a0) * s as f64 / 24.0, SHELL - 0.04))
            .collect();
        if ring
            .iter()
            .any(|p| other(*p + (*p - point(x, FRAC_PI_2, 0.0)).normalize() * 0.5))
        {
            continue;
        }
        crate::carmesh::tube(
            b,
            &ring,
            (0.07, 0.07),
            8,
            Some((Surf::Metal, rgb(198, 160, 82))),
        );
    }
    // The Aether strip along the crown.
    b.surf = Surf::Glow;
    let w = 0.09;
    for i in 0..n {
        let (x0, x1) = (len * i as f64 / n as f64, len * (i + 1) as f64 / n as f64);
        let top = |x: f64, o: f64| point(x, FRAC_PI_2, SHELL - 0.06) + v3(side * o, 0.0);
        if other(point((x0 + x1) * 0.5, FRAC_PI_2, SHELL + 0.45)) {
            continue;
        }
        b.quad(
            [top(x0, -w), top(x1, -w), top(x1, w), top(x0, w)],
            -DVec3::Y,
            rgb(120, 236, 255),
        );
    }
    b.surf = Surf::Plain;
}

/// A tube station's hall: terrazzo floor, cream glazed tiles with a teal
/// band, and a barrel vault with brass ribs.
fn station_hall(b: &mut MeshBuilder, v: &Void, other: &dyn Fn(DVec3) -> bool) {
    let spring = 4.6;
    box_room(
        b,
        v,
        &Finish {
            floor: (Some(GroundLayer::PlazaStone.into()), rgb(214, 204, 186)),
            wall: (Some(FacadeLayer::BrickWhite.into()), rgb(236, 228, 206)),
            wall_h: Some(spring),
            ceiling: None,
            band: Some((1.3, 1.62, rgb(36, 132, 128), Surf::Plain)),
            dado: Some((1.0, rgb(150, 54, 44))),
        },
        other,
    );
    let (hx, hy) = (v.max.x, v.max.y);
    let rise = v.height - spring - 0.1;
    let f = v.floor[0];
    // The vault: rows along the hall, an elliptic arc across.
    let cols = 20;
    let arc = |x: f64, k: usize| {
        let a = PI * k as f64 / cols as f64;
        let y = -hy * a.cos();
        let p = v.world(DVec2::new(x, y));
        v3(p, f + spring + rise * a.sin())
    };
    let n = ((2.0 * hx) / 2.0).ceil() as usize;
    let rows: Vec<Vec<DVec3>> = (0..=n)
        .map(|i| {
            let x = -hx + 2.0 * hx * i as f64 / n as f64;
            (0..=cols).map(|k| arc(x, k)).collect()
        })
        .collect();
    // Faces point down into the hall.
    let mid = v3(v.world(DVec2::ZERO), f);
    for i in 0..n {
        for k in 0..cols {
            let q = [
                rows[i][k],
                rows[i + 1][k],
                rows[i + 1][k + 1],
                rows[i][k + 1],
            ];
            let c = (q[0] + q[2]) * 0.5;
            b.quad(q, mid - c, rgb(238, 232, 214));
        }
    }
    // End lunettes.
    for s in [-1.0, 1.0] {
        let x = s * hx;
        let pts: Vec<DVec3> = (0..=cols).map(|k| arc(x, k)).collect();
        let inward = v3(-v.along * s, 0.0);
        b.polygon(&pts, inward, rgb(236, 228, 206));
    }
    // Brass ribs across the vault.
    for i in 0..=((2.0 * hx) / 6.5).floor() as usize {
        let x = -hx + 1.0 + i as f64 * 6.5;
        if x > hx - 0.5 {
            break;
        }
        let ring: Vec<DVec3> = (0..=cols).map(|k| arc(x, k) - DVec3::Y * 0.05).collect();
        crate::carmesh::tube(
            b,
            &ring,
            (0.08, 0.08),
            8,
            Some((Surf::Metal, rgb(198, 160, 82))),
        );
    }
    // Sputnik chandeliers along the crown.
    for k in 0..4 {
        let x = -19.5 + k as f64 * 13.0;
        let top = v3(v.world(DVec2::new(x, 0.0)), f + v.height - 0.15);
        let ball = top - DVec3::Y * 1.3;
        uv::tube(b, top, ball, 0.025, rgb(190, 160, 90));
        b.surf = Surf::Metal;
        uv::sphere(b, ball, 0.32, rgb(214, 190, 120));
        b.surf = Surf::Plain;
        for s in 0..10 {
            let a = TAU * s as f64 / 10.0;
            let e: f64 = if s % 2 == 0 { 0.5 } else { -0.35 };
            let d = DVec3::new(
                a.cos() * (1.0 - e.abs() * 0.5),
                e,
                a.sin() * (1.0 - e.abs() * 0.5),
            )
            .normalize();
            let tip = ball + d * 1.0;
            uv::tube(b, ball + d * 0.3, tip, 0.018, rgb(214, 190, 120));
            b.surf = Surf::Glow;
            uv::sphere(b, tip, 0.07, rgb(255, 236, 190));
            b.surf = Surf::Plain;
        }
    }
    // Starburst mosaics on the back wall's tiles.
    for k in 0..3 {
        let x = -13.0 + k as f64 * 13.0;
        for s in [-1.0, 1.0] {
            let p = v.world(DVec2::new(x, s * (hy - 0.02)));
            let c = v3(p, f + 3.0);
            let n = v3(-v.inward() * s, 0.0);
            if other(c + n * -0.5) {
                continue;
            }
            starburst(b, c, n, 0.9, rgb(36, 132, 128), rgb(214, 156, 64));
        }
    }
}

/// A Googie starburst flat on a wall: eight long and eight short rays.
fn starburst(b: &mut MeshBuilder, c: DVec3, n: DVec3, r: f64, a: Color, bcol: Color) {
    let u = n.cross(DVec3::Y).normalize_or(DVec3::X);
    let up = u.cross(n).normalize();
    for k in 0..16 {
        let ang = TAU * k as f64 / 16.0;
        let len = if k % 2 == 0 { r } else { r * 0.55 };
        let w = 0.06;
        let d = u * ang.cos() + up * ang.sin();
        let s = u * (-ang.sin()) + up * ang.cos();
        let o = c + n * 0.015;
        b.tri(
            [o + s * w, o - s * w, o + d * len],
            n,
            if k % 2 == 0 { a } else { bcol },
        );
    }
    let ring: Vec<DVec3> = (0..16)
        .map(|k| {
            let ang = TAU * k as f64 / 16.0;
            c + n * 0.02 + (u * ang.cos() + up * ang.sin()) * 0.12
        })
        .collect();
    b.polygon(&ring, n, bcol);
}

/// A room of the buried city: flagstones, great sandstone blocks with
/// pilasters, a glowing glyph frieze and a cornice; rock above.
fn ruin_room(b: &mut MeshBuilder, v: &Void, other: &dyn Fn(DVec3) -> bool) {
    let tall = v.height >= 10.5;
    let dome = (v.max.x - v.max.y).abs() < 0.1 && v.max.x > 9.0;
    let wall_h = if tall {
        9.5
    } else if dome {
        7.5
    } else {
        (v.height - 0.6).min(7.4)
    };
    let stone = rgb(196, 168, 128);
    box_room(
        b,
        v,
        &Finish {
            floor: (Some(MedievalLayer::Flagstone.into()), rgb(150, 134, 112)),
            wall: (Some(FacadeLayer::Sandstone.into()), stone),
            wall_h: Some(wall_h),
            ceiling: None,
            band: (wall_h > 5.0).then_some((
                wall_h - 2.3,
                wall_h - 2.0,
                rgb(70, 230, 200),
                Surf::Glow,
            )),
            dado: Some((0.5, rgb(150, 128, 96))),
        },
        other,
    );
    let f = v.floor[0];
    // Pilasters and the cornice along the long walls.
    if wall_h > 5.0 {
        for s in [-1.0, 1.0] {
            let y = s * (v.max.y - 0.2);
            let n = v3(-v.inward() * s, 0.0);
            let mut x = v.min.x + 2.5;
            while x < v.max.x - 1.0 {
                let p = v.world(DVec2::new(x, y));
                if !other(v3(p + v.inward() * s * 0.6, f + 1.5)) {
                    slab(
                        b,
                        v3(p, f + wall_h * 0.5),
                        [v3(v.along, 0.0), DVec3::Y],
                        DVec3::new(0.35, wall_h * 0.5, 0.2),
                        rgb(206, 180, 140),
                    );
                }
                x += 5.0;
            }
            let a = v.world(DVec2::new(v.min.x, y));
            let c = v.world(DVec2::new(v.max.x, y));
            let m = (a + c) * 0.5;
            slab(
                b,
                v3(m, f + wall_h - 0.2),
                [v3(v.along, 0.0), DVec3::Y],
                DVec3::new((v.max.x - v.min.x) * 0.5, 0.2, 0.32),
                rgb(210, 186, 146),
            );
            let _ = n;
        }
    }
    if dome {
        // The dome: from the walls' top up to an Aether eye.
        let c = v.world(DVec2::ZERO);
        let r = v.max.x - 0.05;
        let rise = v.height - wall_h - 0.3;
        let rings = 10;
        let seg = 48;
        let rows: Vec<Vec<DVec3>> = (0..=rings)
            .map(|k| {
                let t = FRAC_PI_2 * k as f64 / rings as f64;
                let rr = (r * t.cos()).max(1.0);
                (0..seg)
                    .map(|s| {
                        let a = TAU * s as f64 / seg as f64;
                        DVec3::new(
                            c.x + rr * a.cos(),
                            f + wall_h + rise * t.sin(),
                            c.y + rr * a.sin(),
                        )
                    })
                    .collect()
            })
            .collect();
        let centre = v3(c, f);
        for k in 0..rings {
            for s in 0..seg {
                let s1 = (s + 1) % seg;
                let q = [rows[k][s], rows[k][s1], rows[k + 1][s1], rows[k + 1][s]];
                let mid = (q[0] + q[2]) * 0.5;
                let colr = if k % 3 == 2 {
                    rgb(182, 154, 116)
                } else {
                    rgb(204, 178, 138)
                };
                b.tex = Some(FacadeLayer::Sandstone.into());
                b.quad(q, centre - mid, colr);
                b.tex = None;
            }
        }
        // Flat pendentives in the corners between the round dome and square
        // walls.
        for s in 0..seg {
            let a0 = TAU * s as f64 / seg as f64;
            let a1 = TAU * (s + 1) as f64 / seg as f64;
            let sq = |a: f64| {
                let d = DVec2::new(a.cos(), a.sin());
                let l = DVec2::new(d.dot(v.along), d.dot(v.inward()));
                let k = (v.max.x / l.x.abs().max(1e-6)).min(v.max.y / l.y.abs().max(1e-6));
                c + d * k
            };
            let q = [
                v3(c + DVec2::new(a0.cos(), a0.sin()) * r, f + wall_h),
                v3(sq(a0), f + wall_h),
                v3(sq(a1), f + wall_h),
                v3(c + DVec2::new(a1.cos(), a1.sin()) * r, f + wall_h),
            ];
            b.quad(q, -DVec3::Y, rgb(190, 164, 124));
        }
        b.surf = Surf::Glow;
        let eye: Vec<DVec3> = (0..seg)
            .map(|s| {
                let a = TAU * s as f64 / seg as f64;
                DVec3::new(
                    c.x + 1.0 * a.cos(),
                    f + wall_h + rise - 0.02,
                    c.y + 1.0 * a.sin(),
                )
            })
            .collect();
        b.polygon(&eye, -DVec3::Y, rgb(140, 255, 230));
        b.surf = Surf::Plain;
    }
}

/// A room of the Doctor's lab: steel plate walls with a red band, dark
/// floor panels, a ceiling with ducts.
/// An industrial lamp: a rod from the ceiling at `ceiling`, a green enamel
/// shade and a glowing bulb at `at`.
fn hanging_lamp(b: &mut MeshBuilder, at: DVec3, ceiling: f64) {
    let top = at + DVec3::Y * 0.45;
    if ceiling > top.y {
        uv::tube(
            b,
            top,
            DVec3::new(at.x, ceiling, at.z),
            0.025,
            rgb(60, 62, 64),
        );
    }
    lathe(
        b,
        at - DVec3::Y * 0.05,
        DVec3::Y,
        &[
            (0.62, 0.0),
            (0.6, 0.04),
            (0.42, 0.22),
            (0.16, 0.42),
            (0.05, 0.5),
            (0.0, 0.5),
        ],
        24,
        |_| Some((Surf::Plain, rgb(52, 98, 78))),
    );
    b.surf = Surf::Glow;
    uv::sphere(b, at + DVec3::Y * 0.02, 0.16, rgb(240, 255, 236));
    b.surf = Surf::Plain;
}

fn lab_room(b: &mut MeshBuilder, v: &Void, other: &dyn Fn(DVec3) -> bool) {
    box_room(
        b,
        v,
        &Finish {
            floor: (Some(FacadeLayer::ConcretePanel.into()), rgb(122, 126, 128)),
            wall: (Some(FacadeLayer::MetalPanel.into()), rgb(150, 172, 160)),
            wall_h: None,
            ceiling: Some((Some(FacadeLayer::MetalPanel.into()), rgb(96, 102, 104))),
            band: Some((1.9, 2.1, rgb(196, 40, 36), Surf::Plain)),
            dado: Some((0.35, rgb(40, 42, 44))),
        },
        other,
    );
    // Ducts along the ceiling of the big rooms.
    if v.height >= 9.0 {
        let f = v.floor[0];
        for s in [-1.0, 1.0] {
            let y = s * (v.max.y - 1.2);
            let a = v3(v.world(DVec2::new(v.min.x + 0.5, y)), f + v.height - 0.7);
            let c = v3(v.world(DVec2::new(v.max.x - 0.5, y)), f + v.height - 0.7);
            uv::tube(b, a, c, 0.38, rgb(122, 128, 126));
            let a2 = a - DVec3::Y * 0.9 + v3(v.inward() * -s * 0.6, 0.0);
            let c2 = c - DVec3::Y * 0.9 + v3(v.inward() * -s * 0.6, 0.0);
            uv::tube(b, a2, c2, 0.16, rgb(170, 120, 60));
        }
        // Hazard stripes along the walls' foot.
        let mut x = v.min.x + 0.3;
        let mut k = 0;
        while x < v.max.x - 0.6 {
            for s in [-1.0, 1.0] {
                let y = s * (v.max.y - 0.5);
                let p0 = v.world(DVec2::new(x, y - 0.25 * s));
                let p1 = v.world(DVec2::new(x + 0.6, y - 0.25 * s));
                let p2 = v.world(DVec2::new(x + 0.6, y + 0.25 * s));
                let p3 = v.world(DVec2::new(x, y + 0.25 * s));
                let c = if k % 2 == 0 {
                    rgb(230, 190, 40)
                } else {
                    rgb(30, 30, 30)
                };
                b.quad([p0, p1, p2, p3].map(|p| v3(p, f + 0.01)), DVec3::Y, c);
            }
            x += 0.6;
            k += 1;
        }
    }
}

// ------------------------------------------------------------ fixtures

fn fixture(b: &mut MeshBuilder, f: &Fixture, glows: &mut Vec<Glow>) {
    use FixtureKind::*;
    let p = f.plan();
    let y = f.at.y;
    let s = f.size;
    let side = DVec2::new(f.dir.y, -f.dir.x);
    // Stone is stone and wood is wood: the plain faces of a fixture take
    // its material's texture.
    let tex: Option<Tex> = match f.kind {
        Column | BrokenColumn | FallenColumn | GiantHead | Brazier => {
            Some(FacadeLayer::Sandstone.into())
        }
        Rubble => Some(Tex::Layer(NaturalLayer::CliffStrata.into())),
        Stalagmite => Some(Tex::Layer(NaturalLayer::Limestone.into())),
        Crate | Bench => Some(Tex::Layer(MedievalLayer::Oak.into())),
        _ => None,
    };
    let before = b.tex;
    if tex.is_some() {
        b.tex = tex;
    }
    fixture_parts(b, f, p, y, s, side, glows);
    b.tex = before;
}

#[allow(clippy::too_many_arguments)]
fn fixture_parts(
    b: &mut MeshBuilder,
    f: &Fixture,
    p: DVec2,
    y: f64,
    s: DVec3,
    side: DVec2,
    glows: &mut Vec<Glow>,
) {
    use FixtureKind::*;
    match f.kind {
        Bench => bench(b, p, f.dir, y),
        Planter => planter(b, p, y, f.look),
        DestinationBoard => board(b, f),
        Column => column(b, p, y, s.x, s.y, f.look),
        BrokenColumn => broken_column(b, p, y, s.x, s.y, f.look),
        FallenColumn => fallen_column(b, p, f.dir, y, s),
        Rubble => rubble(b, p, y, s, f.look),
        GiantHead => giant_head(b, p, f.dir, y, s.x),
        VaultDoor => vault_door(b, p, f.dir, y, s.x),
        Brazier => brazier(b, p, y, s),
        TeslaCoil => tesla(b, p, y, s),
        Vat => vat(b, p, y, s, f.look),
        Console => console(b, p, f.dir, y, s, f.look),
        Reactor => reactor(b, p, y, s),
        CellBars => bars(b, p, side, y, s),
        RayCannon => ray_cannon(b, p, f.dir, y, s),
        ScreenWall => screens(b, p, f.dir, y, s),
        Crate => crate_box(b, p, f.dir, y, s, f.look),
        Crystal => {
            let c = [
                rgb(150, 120, 255),
                rgb(110, 240, 210),
                rgb(120, 200, 255),
                rgb(230, 130, 255),
            ][f.look as usize % 4];
            b.surf = Surf::Glow;
            uv::crystal(b, p, y, s.y, c);
            b.surf = Surf::Plain;
        }
        GiantMushroom => {
            let caps = [
                rgb(70, 170, 150),
                rgb(120, 90, 190),
                rgb(196, 80, 60),
                rgb(220, 160, 70),
            ];
            let glow = [rgb(140, 255, 220), rgb(200, 160, 255)][f.look as usize % 2];
            uv::mushroom(
                b,
                p,
                y,
                s.x * 0.22,
                s.y,
                s.x,
                caps[f.look as usize % 4],
                glow,
            );
        }
        Stalagmite => stalagmite(b, p, y, s, f.look),
        Skeleton => skeleton(b, p, f.dir, y, s),
        GlowPool => pool(b, p, f.dir, y, s),
        Tent => tent(b, p, f.dir, y, s, f.look),
        Campfire => campfire(b, p, y, glows),
        // Drawn with its entrance, on the street.
        Kiosk => {}
    }
}

fn bench(b: &mut MeshBuilder, p: DVec2, dir: DVec2, y: f64) {
    let side = DVec2::new(dir.y, -dir.x);
    let wood = rgb(150, 98, 58);
    let chrome = rgb(220, 224, 230);
    // Slatted seat and back.
    for k in 0..3 {
        let z = -0.12 + k as f64 * 0.12;
        slab(
            b,
            v3(p + dir * z, y + 0.46),
            [v3(side, 0.0), DVec3::Y],
            DVec3::new(1.35, 0.025, 0.05),
            wood,
        );
    }
    for k in 0..3 {
        let h = 0.62 + k as f64 * 0.13;
        slab(
            b,
            v3(p - dir * 0.27, y + h),
            [v3(side, 0.0), v3(dir * 0.2, 1.0).normalize()],
            DVec3::new(1.35, 0.045, 0.02),
            wood,
        );
    }
    // Swept chrome legs.
    for sx in [-1.1, 1.1] {
        let q = p + side * sx;
        let pts = [
            v3(q + dir * 0.25, y),
            v3(q + dir * 0.1, y + 0.44),
            v3(q - dir * 0.24, y + 0.46),
            v3(q - dir * 0.32, y + 1.0),
        ];
        let path = crate::carmesh::spline(&pts, 6, false);
        crate::carmesh::tube(b, &path, (0.025, 0.025), 10, Some((Surf::Metal, chrome)));
    }
}

fn planter(b: &mut MeshBuilder, p: DVec2, y: f64, look: u32) {
    // A flared terrazzo bowl on a foot, and a fan of spiky leaves.
    lathe(
        b,
        v3(p, y),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.32, 0.0),
            (0.26, 0.12),
            (0.2, 0.35),
            (0.42, 0.62),
            (0.55, 0.78),
            (0.56, 0.84),
            (0.48, 0.84),
            (0.0, 0.8),
        ],
        28,
        |k| {
            Some((
                Surf::Plain,
                if k >= 5 {
                    rgb(196, 64, 52)
                } else {
                    rgb(226, 216, 196)
                },
            ))
        },
    );
    let leaf = rgb(52, 120, 64);
    for k in 0..9 {
        let a = TAU * k as f64 / 9.0 + look as f64;
        let d = DVec3::new(a.cos(), 0.0, a.sin());
        let base = v3(p, y + 0.8);
        let tip = base + d * 0.7 + DVec3::Y * (0.9 + 0.2 * (k % 3) as f64);
        let s = d.cross(DVec3::Y) * 0.09;
        b.tri([base + s, base - s, tip], d + DVec3::Y, leaf);
        b.tri([base - s, base + s, tip], -d - DVec3::Y, leaf);
    }
}

fn board(b: &mut MeshBuilder, f: &Fixture) {
    let n = f.dir;
    let side = DVec2::new(n.y, -n.x);
    let c = f.at + v3(n, 0.0) * 0.06;
    slab(
        b,
        c,
        [v3(side, 0.0), DVec3::Y],
        DVec3::new(f.size.x + 0.08, f.size.y + 0.08, 0.05),
        rgb(190, 160, 90),
    );
    slab(
        b,
        c + v3(n, 0.0) * 0.03,
        [v3(side, 0.0), DVec3::Y],
        DVec3::new(f.size.x, f.size.y, 0.03),
        rgb(24, 26, 30),
    );
    // Rows of amber dots: the next capsules.
    b.surf = Surf::Glow;
    for row in 0..3 {
        let yy = f.size.y * 0.55 - row as f64 * f.size.y * 0.42;
        let len = f.size.x * (0.5 + 0.4 * (((f.look as usize + row * 3) % 5) as f64 / 4.0));
        let a = c + v3(n, 0.0) * 0.065 + v3(side, 0.0) * (-f.size.x * 0.85) + DVec3::Y * yy;
        let e = a + v3(side, 0.0) * len * 1.4;
        let h = DVec3::Y * 0.06;
        b.quad([a - h, e - h, e + h, a + h], v3(n, 0.0), rgb(255, 180, 60));
    }
    b.surf = Surf::Plain;
}

/// A column with a moulded base and capital, holding up the rock.
fn column(b: &mut MeshBuilder, p: DVec2, y: f64, r: f64, h: f64, look: u32) {
    let stone = if look.is_multiple_of(3) {
        rgb(200, 176, 136)
    } else {
        rgb(210, 188, 150)
    };
    let mut prof = vec![
        (0.0, 0.0),
        (r * 1.45, 0.0),
        (r * 1.45, 0.3),
        (r * 1.3, 0.42),
        (r * 1.18, 0.55),
        (r * 1.02, 0.7),
    ];
    // A shaft with a slight swell (entasis).
    for k in 1..8 {
        let t = k as f64 / 8.0;
        prof.push((
            r * (1.0 + 0.04 * (PI * t).sin() - 0.08 * t),
            0.7 + (h - 1.7) * t,
        ));
    }
    prof.extend([
        (r * 0.95, h - 1.0),
        (r * 1.08, h - 0.88),
        (r * 1.3, h - 0.65),
        (r * 1.5, h - 0.45),
        (r * 1.62, h - 0.4),
        (r * 1.62, h),
        (0.0, h),
    ]);
    lathe(b, v3(p, y), DVec3::Y, &prof, 24, |_| {
        Some((Surf::Plain, stone))
    });
    // Flutes: thin darker grooves down the shaft.
    let groove = rgb(160, 136, 104);
    for k in 0..12 {
        let a = TAU * k as f64 / 12.0;
        let d = DVec3::new(a.cos(), 0.0, a.sin());
        let w = d.cross(DVec3::Y) * 0.025;
        let q0 = v3(p, y + 0.8) + d * (r * 1.005);
        let q1 = v3(p, y + h - 1.05) + d * (r * 0.925);
        b.quad([q0 - w, q0 + w, q1 + w, q1 - w], d, groove);
    }
}

fn broken_column(b: &mut MeshBuilder, p: DVec2, y: f64, r: f64, h: f64, look: u32) {
    let stone = rgb(198, 174, 134);
    lathe(
        b,
        v3(p, y),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (r * 1.45, 0.0),
            (r * 1.45, 0.3),
            (r * 1.18, 0.55),
            (r * 1.02, 0.7),
            (r * 1.0, h - 0.2),
            (r * 0.8, h - 0.05),
            (0.0, h),
        ],
        24,
        |_| Some((Surf::Plain, stone)),
    );
    // A jagged break: a few chunks tilted on top.
    for k in 0..4 {
        let a = TAU * k as f64 / 4.0 + look as f64 * 0.7;
        let d = DVec2::new(a.cos(), a.sin());
        let up = v3(d * 0.3, 1.0).normalize();
        slab(
            b,
            v3(p + d * r * 0.4, y + h - 0.05 + 0.08 * (k % 2) as f64),
            [v3(d.perp(), 0.0), up],
            DVec3::new(r * 0.35, 0.12 + 0.06 * k as f64, r * 0.3),
            stone,
        );
    }
}

fn fallen_column(b: &mut MeshBuilder, p: DVec2, dir: DVec2, y: f64, s: DVec3) {
    // Drums lying end to end along `dir`'s side, slightly apart.
    let along = DVec2::new(dir.y, -dir.x);
    let r = 0.6;
    let n = 3;
    let len = s.x * 2.0;
    for k in 0..n {
        let t0 = -len * 0.5 + len * k as f64 / n as f64 + 0.04;
        let t1 = -len * 0.5 + len * (k + 1) as f64 / n as f64 - 0.04;
        let a = v3(p + along * t0 + dir * (0.05 * k as f64), y + r);
        lathe(
            b,
            a,
            v3(along, 0.0),
            &[
                (0.0, 0.0),
                (r * 0.97, 0.0),
                (r, 0.04),
                (r, t1 - t0 - 0.04),
                (r * 0.97, t1 - t0),
                (0.0, t1 - t0),
            ],
            24,
            |_| Some((Surf::Plain, rgb(196, 172, 132))),
        );
    }
}

fn rubble(b: &mut MeshBuilder, p: DVec2, y: f64, s: DVec3, look: u32) {
    let mut r = hw_core::Seed(look as u64 + 77).rng();
    for k in 0..7 {
        let d = DVec2::new(r.range(-1.0, 1.0) * s.x, r.range(-1.0, 1.0) * s.z);
        let h = r.range(0.2, 0.5) * (1.0 - 0.1 * k as f64).max(0.4);
        let a = r.range(0.0, TAU);
        let u = DVec2::new(a.cos(), a.sin());
        let tilt = v3(u * r.range(-0.3, 0.3), 1.0).normalize();
        let c = rgb(
            (176.0 + r.range(-20.0, 20.0)) as u8,
            (154.0 + r.range(-20.0, 20.0)) as u8,
            (120.0 + r.range(-20.0, 20.0)) as u8,
        );
        let yy = y + h + if k > 3 { s.y * 0.5 } else { 0.0 };
        slab(
            b,
            v3(p + d * 0.7, yy),
            [v3(u, 0.0), tilt],
            DVec3::new(r.range(0.3, 0.7), h, r.range(0.25, 0.5)),
            c,
        );
    }
}

/// A colossal stone face half sunk in the floor.
fn giant_head(b: &mut MeshBuilder, p: DVec2, dir: DVec2, y: f64, r: f64) {
    let stone = rgb(170, 160, 140);
    let side = DVec2::new(dir.y, -dir.x);
    let c = v3(p, y + r * 0.55);
    // The head: an egg, flattened at the back.
    let rings = 14;
    let prof: Vec<(f64, f64)> = (0..=rings)
        .map(|k| {
            let a = PI * k as f64 / rings as f64;
            (
                r * a.sin() * (1.0 - 0.08 * (a - 1.2).abs()),
                -r * 1.15 * a.cos(),
            )
        })
        .collect();
    lathe(b, c, DVec3::Y, &prof, 32, |_| Some((Surf::Plain, stone)));
    let face = |x: f64, up: f64, out: f64| c + v3(side * x + dir * out, 0.0) + DVec3::Y * up;
    // Brow, nose, lips; dark eye hollows; a headband with a glowing gem.
    slab(
        b,
        face(0.0, r * 0.35, r * 0.85),
        [v3(side, 0.0), DVec3::Y],
        DVec3::new(r * 0.62, r * 0.09, r * 0.18),
        stone,
    );
    slab(
        b,
        face(0.0, r * 0.0, r * 0.98),
        [v3(side, 0.0), v3(dir * -0.25, 1.0).normalize()],
        DVec3::new(r * 0.16, r * 0.3, r * 0.14),
        stone,
    );
    slab(
        b,
        face(0.0, -r * 0.42, r * 0.86),
        [v3(side, 0.0), DVec3::Y],
        DVec3::new(r * 0.34, r * 0.07, r * 0.12),
        rgb(150, 138, 118),
    );
    for sx in [-0.32, 0.32] {
        uv::disc(
            b,
            face(sx * r, r * 0.17, r * 0.93),
            v3(dir, 0.0),
            r * 0.15,
            rgb(40, 36, 32),
        );
    }
    let band: Vec<DVec3> = (0..=24)
        .map(|k| {
            let a = -FRAC_PI_2 * 1.1 + PI * 1.1 * k as f64 / 24.0;
            c + v3(
                side * (a.sin() * r * 0.98) + dir * (a.cos() * r * 0.98),
                0.0,
            ) + DVec3::Y * (r * 0.62)
        })
        .collect();
    crate::carmesh::tube(
        b,
        &band,
        (0.12, 0.12),
        8,
        Some((Surf::Metal, rgb(180, 150, 80))),
    );
    b.surf = Surf::Glow;
    uv::sphere(b, face(0.0, r * 0.62, r * 1.0), 0.2, rgb(90, 255, 220));
    b.surf = Surf::Plain;
}

/// A great round vault door in the wall: brass, with a spoked wheel and a
/// ring of bolts.
fn vault_door(b: &mut MeshBuilder, p: DVec2, dir: DVec2, y: f64, r: f64) {
    let c = v3(p, y + r + 0.3);
    let n = v3(dir, 0.0);
    let brass = rgb(196, 160, 84);
    lathe(
        b,
        c - n * 0.1,
        n,
        &[
            (0.0, 0.0),
            (r * 1.12, 0.0),
            (r * 1.12, 0.12),
            (r, 0.18),
            (r * 0.98, 0.42),
            (r * 0.9, 0.48),
            (0.0, 0.5),
        ],
        48,
        |k| Some((Surf::Metal, if k < 2 { rgb(120, 100, 64) } else { brass })),
    );
    let side = n.cross(DVec3::Y).normalize();
    for k in 0..16 {
        let a = TAU * k as f64 / 16.0;
        let q = c + n * 0.42 + (side * a.cos() + DVec3::Y * a.sin()) * (r * 0.82);
        b.surf = Surf::Metal;
        uv::sphere(b, q, 0.09, rgb(150, 120, 60));
        b.surf = Surf::Plain;
    }
    let hub = c + n * 0.75;
    uv::tube(b, c + n * 0.45, hub, 0.12, brass);
    let ring: Vec<DVec3> = (0..=32)
        .map(|k| {
            let a = TAU * k as f64 / 32.0;
            hub + (side * a.cos() + DVec3::Y * a.sin()) * (r * 0.5)
        })
        .collect();
    crate::carmesh::tube(b, &ring, (0.06, 0.06), 8, Some((Surf::Metal, brass)));
    for k in 0..6 {
        let a = TAU * k as f64 / 6.0;
        uv::tube(
            b,
            hub,
            hub + (side * a.cos() + DVec3::Y * a.sin()) * (r * 0.5),
            0.045,
            brass,
        );
    }
}

fn brazier(b: &mut MeshBuilder, p: DVec2, y: f64, s: DVec3) {
    let bronze = rgb(150, 116, 64);
    for k in 0..3 {
        let a = TAU * k as f64 / 3.0;
        let d = DVec3::new(a.cos(), 0.0, a.sin());
        uv::tube(
            b,
            v3(p, y) + d * s.x * 0.9,
            v3(p, y + s.y * 0.75) + d * s.x * 0.4,
            0.04,
            bronze,
        );
    }
    lathe(
        b,
        v3(p, y + s.y * 0.7),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (s.x * 0.3, 0.0),
            (s.x * 0.8, 0.18),
            (s.x, 0.34),
            (s.x * 0.92, 0.36),
            (0.0, 0.3),
        ],
        24,
        |_| Some((Surf::Metal, bronze)),
    );
    b.surf = Surf::Glow;
    lathe(
        b,
        v3(p, y + s.y * 0.7 + 0.28),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (s.x * 0.7, 0.0),
            (s.x * 0.45, 0.3),
            (s.x * 0.15, 0.6),
            (0.0, 0.75),
        ],
        16,
        |_| Some((Surf::Glow, rgb(90, 255, 210))),
    );
    b.surf = Surf::Plain;
}

/// A Tesla coil: a stepped plinth, a copper-wound column and a chrome
/// torus, with frozen sparks.
fn tesla(b: &mut MeshBuilder, p: DVec2, y: f64, s: DVec3) {
    let r = s.x;
    let h = s.y;
    lathe(
        b,
        v3(p, y),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (r * 1.3, 0.0),
            (r * 1.3, 0.4),
            (r * 1.0, 0.45),
            (r * 1.0, 0.9),
            (r * 0.7, 1.0),
            (0.0, 1.0),
        ],
        32,
        |_| Some((Surf::Metal, rgb(60, 62, 66))),
    );
    let coil_h = h - 2.2;
    let n = 24;
    let prof: Vec<(f64, f64)> = (0..=n * 2 + 1)
        .map(|k| {
            let t = (k / 2) as f64 / n as f64;
            (
                r * 0.42 * (1.0 - 0.25 * t) + if k % 2 == 1 { 0.03 } else { 0.0 },
                1.0 + coil_h * ((k + 1) / 2) as f64 / (n as f64 + 0.5),
            )
        })
        .collect();
    let mut prof = prof;
    prof.insert(0, (0.0, 1.0));
    prof.push((0.0, 1.0 + coil_h));
    lathe(b, v3(p, y), DVec3::Y, &prof, 28, |k| {
        Some((
            Surf::Metal,
            if k % 4 < 2 {
                rgb(196, 110, 60)
            } else {
                rgb(160, 84, 44)
            },
        ))
    });
    let top = v3(p, y + h - 0.7);
    uv::tube(
        b,
        v3(p, y + 1.0 + coil_h - 0.1),
        top,
        0.06,
        rgb(200, 204, 210),
    );
    let ring: Vec<DVec3> = (0..=40)
        .map(|k| {
            let a = TAU * k as f64 / 40.0;
            top + DVec3::new(a.cos() * r * 0.75, 0.0, a.sin() * r * 0.75)
        })
        .collect();
    crate::carmesh::tube(
        b,
        &ring,
        (0.28, 0.22),
        14,
        Some((Surf::Metal, rgb(214, 218, 224))),
    );
    b.surf = Surf::Glow;
    uv::sphere(b, top + DVec3::Y * 0.35, 0.22, rgb(200, 180, 255));
    // Sparks: zigzags out from the torus.
    let mut rr = hw_core::Seed((p.x * 7.0 + p.y * 13.0) as i64 as u64).rng();
    for _ in 0..4 {
        let a = rr.range(0.0, TAU);
        let mut q = top + DVec3::new(a.cos(), 0.0, a.sin()) * (r * 0.95);
        let mut pts = vec![q];
        for _ in 0..5 {
            q += DVec3::new(
                a.cos() * 0.45 + rr.range(-0.3, 0.3),
                rr.range(-0.5, 0.2),
                a.sin() * 0.45 + rr.range(-0.3, 0.3),
            );
            pts.push(q);
        }
        crate::carmesh::tube(
            b,
            &pts,
            (0.025, 0.025),
            6,
            Some((Surf::Glow, rgb(210, 190, 255))),
        );
    }
    b.surf = Surf::Plain;
}

/// A vat: a glass cylinder of glowing green brew between brass caps, with
/// pipes up into the dark.
fn vat(b: &mut MeshBuilder, p: DVec2, y: f64, s: DVec3, look: u32) {
    let r = s.x;
    let h = s.y;
    let brass = rgb(176, 140, 74);
    lathe(
        b,
        v3(p, y),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (r * 1.15, 0.0),
            (r * 1.15, 0.35),
            (r * 1.02, 0.42),
            (0.0, 0.42),
        ],
        32,
        |_| Some((Surf::Metal, brass)),
    );
    let liquid = if look.is_multiple_of(2) {
        rgb(80, 255, 110)
    } else {
        rgb(150, 255, 60)
    };
    lathe(
        b,
        v3(p, y + 0.42),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (r * 0.94, 0.0),
            (r * 0.94, (h - 0.9) * 0.82),
            (0.0, (h - 0.9) * 0.82),
        ],
        32,
        |_| Some((Surf::Glow, liquid)),
    );
    lathe(
        b,
        v3(p, y + 0.42),
        DVec3::Y,
        &[(r, 0.0), (r, h - 0.9)],
        40,
        |_| Some((Surf::Glass, rgb(200, 240, 220))),
    );
    lathe(
        b,
        v3(p, y + h - 0.48),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (r * 1.06, 0.0),
            (r * 1.1, 0.08),
            (r * 0.9, 0.3),
            (r * 0.4, 0.45),
            (0.0, 0.48),
        ],
        32,
        |_| Some((Surf::Metal, brass)),
    );
    // Bubbles.
    b.surf = Surf::Glow;
    for k in 0..5 {
        let a = TAU * k as f64 / 5.0 + look as f64;
        let q = v3(p, y + 0.8 + k as f64 * 0.42) + DVec3::new(a.cos(), 0.0, a.sin()) * r * 0.45;
        uv::sphere(b, q, 0.06 + 0.02 * (k % 2) as f64, rgb(220, 255, 220));
    }
    b.surf = Surf::Plain;
    let top = v3(p, y + h);
    uv::tube(b, top, top + DVec3::Y * 3.0, 0.1, rgb(110, 116, 118));
}

fn console(b: &mut MeshBuilder, p: DVec2, dir: DVec2, y: f64, s: DVec3, look: u32) {
    let side = DVec2::new(dir.y, -dir.x);
    slab(
        b,
        v3(p, y + s.y * 0.45),
        [v3(side, 0.0), DVec3::Y],
        DVec3::new(s.x, s.y * 0.45, s.z),
        rgb(70, 84, 80),
    );
    let up = v3(dir * -0.7, 1.0).normalize();
    let c = v3(p + dir * 0.05, y + s.y * 0.95);
    slab(
        b,
        c,
        [v3(side, 0.0), up],
        DVec3::new(s.x, 0.04, s.z * 0.9),
        rgb(50, 56, 54),
    );
    b.surf = Surf::Glow;
    let lights = [
        rgb(255, 80, 60),
        rgb(80, 255, 120),
        rgb(255, 210, 60),
        rgb(80, 180, 255),
    ];
    let n = up.cross(v3(side, 0.0)).normalize();
    let n = if n.dot(v3(dir, 0.0)) < 0.0 { -n } else { n };
    for i in 0..6 {
        for j in 0..2 {
            let q = c
                + v3(side, 0.0) * (-s.x * 0.8 + i as f64 * s.x * 0.32)
                + up.cross(v3(side, 0.0)).normalize() * (j as f64 * 0.18 - 0.09)
                + n * 0.05;
            uv::sphere(b, q, 0.035, lights[(i + j * 2 + look as usize) % 4]);
        }
    }
    b.surf = Surf::Plain;
}

fn reactor(b: &mut MeshBuilder, p: DVec2, y: f64, s: DVec3) {
    let r = s.x;
    lathe(
        b,
        v3(p, y),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (r * 1.1, 0.0),
            (r * 1.1, 0.5),
            (r * 0.8, 0.7),
            (r * 0.5, 1.4),
            (0.0, 1.4),
        ],
        40,
        |_| Some((Surf::Metal, rgb(70, 74, 78))),
    );
    let core = v3(p, y + s.y * 0.5);
    b.surf = Surf::Glow;
    uv::sphere(b, core, r * 0.45, rgb(255, 140, 50));
    b.surf = Surf::Plain;
    for (k, tilt) in [0.0f64, 1.0, -1.0].into_iter().enumerate() {
        let ring: Vec<DVec3> = (0..=48)
            .map(|i| {
                let a = TAU * i as f64 / 48.0;
                let (x, z) = (
                    a.cos() * r * (0.75 + 0.08 * k as f64),
                    a.sin() * r * (0.75 + 0.08 * k as f64),
                );
                core + DVec3::new(x, z * tilt.sin() * 0.8, z * (1.0 - tilt.abs() * 0.4))
            })
            .collect();
        crate::carmesh::tube(
            b,
            &ring,
            (0.09, 0.09),
            10,
            Some((Surf::Metal, rgb(214, 190, 120))),
        );
    }
    // A cage of struts up to a cap.
    for k in 0..8 {
        let a = TAU * k as f64 / 8.0;
        let d = DVec3::new(a.cos(), 0.0, a.sin());
        uv::tube(
            b,
            v3(p, y + 1.3) + d * r * 0.85,
            v3(p, y + s.y - 0.8) + d * r * 0.7,
            0.07,
            rgb(120, 124, 128),
        );
    }
    lathe(
        b,
        v3(p, y + s.y - 0.9),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (r * 0.8, 0.0),
            (r * 0.6, 0.6),
            (0.2, 0.9),
            (0.0, 0.9),
        ],
        32,
        |_| Some((Surf::Metal, rgb(90, 94, 98))),
    );
}

fn bars(b: &mut MeshBuilder, p: DVec2, side: DVec2, y: f64, s: DVec3) {
    let iron = rgb(60, 60, 64);
    let n = (s.x * 2.0 / 0.16).round() as usize;
    for k in 0..=n {
        let q = p + side * (-s.x + 2.0 * s.x * k as f64 / n as f64);
        uv::tube(b, v3(q, y), v3(q, y + s.y), 0.025, iron);
    }
    for h in [0.06, 1.1, s.y - 0.06] {
        slab(
            b,
            v3(p, y + h),
            [v3(side, 0.0), DVec3::Y],
            DVec3::new(s.x, 0.04, 0.04),
            iron,
        );
    }
}

/// The doomsday ray cannon: a turret, a yoke and a long barrel with
/// cooling fins, aimed at the roof.
fn ray_cannon(b: &mut MeshBuilder, p: DVec2, dir: DVec2, y: f64, s: DVec3) {
    let r = s.x;
    let steel = rgb(92, 100, 104);
    lathe(
        b,
        v3(p, y),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (r, 0.0),
            (r, 0.6),
            (r * 0.8, 0.9),
            (r * 0.7, 2.0),
            (r * 0.55, 2.4),
            (0.0, 2.4),
        ],
        40,
        |_| Some((Surf::Metal, steel)),
    );
    let side = DVec2::new(dir.y, -dir.x);
    let pivot = v3(p, y + 3.0);
    for sx in [-1.0, 1.0] {
        slab(
            b,
            pivot + v3(side * sx * r * 0.55, 0.0) - DVec3::Y * 0.4,
            [v3(side, 0.0), DVec3::Y],
            DVec3::new(0.12, 0.9, 0.5),
            steel,
        );
    }
    let aim = (v3(dir, 0.0) * 0.55 + DVec3::Y).normalize();
    let len = s.y - 3.2;
    let mut prof = vec![(0.0, -1.0), (r * 0.45, -1.0), (r * 0.5, -0.6)];
    for k in 0..8 {
        let t = k as f64 / 8.0 * (len * 0.6);
        prof.push((r * 0.36, t));
        prof.push((r * 0.5, t + 0.08));
        prof.push((r * 0.5, t + 0.2));
        prof.push((r * 0.36, t + 0.28));
    }
    prof.extend([
        (r * 0.24, len * 0.62),
        (r * 0.2, len - 0.5),
        (r * 0.42, len - 0.2),
        (r * 0.42, len),
        (0.0, len),
    ]);
    lathe(b, pivot, aim, &prof, 28, |_| {
        Some((Surf::Metal, rgb(150, 156, 162)))
    });
    b.surf = Surf::Glow;
    uv::sphere(b, pivot + aim * (len + 0.05), r * 0.22, rgb(255, 60, 80));
    b.surf = Surf::Plain;
}

fn screens(b: &mut MeshBuilder, p: DVec2, dir: DVec2, y: f64, s: DVec3) {
    let side = DVec2::new(dir.y, -dir.x);
    let base = v3(p + dir * 0.06, y);
    let tints = [rgb(80, 255, 140), rgb(90, 180, 255), rgb(255, 90, 80)];
    for i in 0..4 {
        for j in 0..2 {
            let c = base
                + v3(side, 0.0) * (-s.x * 0.75 + i as f64 * s.x * 0.5)
                + DVec3::Y * (1.4 + j as f64 * 1.3);
            slab(
                b,
                c,
                [v3(side, 0.0), DVec3::Y],
                DVec3::new(s.x * 0.23, 0.58, 0.05),
                rgb(40, 44, 46),
            );
            b.surf = Surf::Glow;
            slab(
                b,
                c + v3(dir, 0.0) * 0.04,
                [v3(side, 0.0), DVec3::Y],
                DVec3::new(s.x * 0.2, 0.5, 0.02),
                tints[(i + j) % 3],
            );
            b.surf = Surf::Plain;
        }
    }
}

fn crate_box(b: &mut MeshBuilder, p: DVec2, dir: DVec2, y: f64, s: DVec3, look: u32) {
    let side = DVec2::new(dir.y, -dir.x);
    let wood = if look.is_multiple_of(2) {
        rgb(150, 112, 70)
    } else {
        rgb(128, 96, 60)
    };
    slab(
        b,
        v3(p, y + s.y * 0.5),
        [v3(side, 0.0), DVec3::Y],
        DVec3::new(s.x, s.y * 0.5, s.z),
        wood,
    );
    for t in [-1.0, 1.0] {
        for (ax, half) in [(side, s.z), (dir, s.x)] {
            let n = DVec2::new(ax.y, -ax.x);
            let c = v3(p + ax * 0.0 + n * (half + 0.012) * t, y + s.y * 0.5);
            slab(
                b,
                c,
                [v3(ax, 0.0), DVec3::Y],
                DVec3::new(if ax == side { s.x } else { s.z }, 0.06, 0.012),
                rgb(90, 66, 40),
            );
        }
    }
}

fn stalagmite(b: &mut MeshBuilder, p: DVec2, y: f64, s: DVec3, look: u32) {
    let r = s.x;
    let h = s.y;
    let n = 8;
    let prof: Vec<(f64, f64)> = (0..=n)
        .map(|k| {
            let t = k as f64 / n as f64;
            let bulge = 1.0 + 0.12 * ((t * 9.0 + look as f64).sin());
            (
                r * (1.0 - t).powf(1.3) * bulge + if k == 0 { r * 0.3 } else { 0.0 },
                h * t,
            )
        })
        .chain(std::iter::once((0.0, h)))
        .collect();
    let mut prof = prof;
    prof.insert(0, (0.0, -0.1));
    lathe(b, v3(p, y - 0.1), DVec3::Y, &prof, 16, |k| {
        Some((
            Surf::Plain,
            if k % 3 == 0 {
                rgb(132, 116, 96)
            } else {
                rgb(150, 132, 108)
            },
        ))
    });
}

/// The bones of a giant beast lying on the cave floor: a curved spine,
/// ribs, a long skull with teeth, legs and a tail.
fn skeleton(b: &mut MeshBuilder, p: DVec2, dir: DVec2, y: f64, s: DVec3) {
    let bone = rgb(222, 210, 180);
    let along = DVec2::new(dir.y, -dir.x);
    let len = s.x * 2.0;
    let at = |t: f64, off: f64, h: f64| v3(p + along * (t * len * 0.5) + dir * off, y + h);
    // Spine, sagging to the tail.
    let spine: Vec<DVec3> = (0..=20)
        .map(|k| {
            let t = -1.0 + 2.0 * k as f64 / 20.0;
            at(t, 0.3 * (t * 2.0).sin(), 1.6 * (1.0 - t * t).max(0.1) + 0.2)
        })
        .collect();
    crate::carmesh::tube(b, &spine, (0.16, 0.16), 10, Some((Surf::Plain, bone)));
    // Vertebrae knobs.
    for q in spine.iter().step_by(2) {
        uv::sphere(b, *q + DVec3::Y * 0.12, 0.17, bone);
    }
    // Ribs.
    for k in 0..9 {
        let t = -0.45 + k as f64 * 0.11;
        let top = at(t, 0.3 * (t * 2.0).sin(), 1.6 * (1.0 - t * t) + 0.2);
        for sgn in [-1.0, 1.0] {
            let rib: Vec<DVec3> = (0..=8)
                .map(|i| {
                    let a = PI * 0.5 * i as f64 / 8.0;
                    let r = 1.3 * (1.0 - (t * 1.4).powi(2)).max(0.35);
                    top + v3(dir * sgn * a.sin() * r, 0.0)
                        - DVec3::Y * (1.0 - a.cos()) * (top.y - y - 0.1)
                })
                .collect();
            crate::carmesh::tube(b, &rib, (0.06, 0.06), 8, Some((Surf::Plain, bone)));
        }
    }
    // The skull at the front, jaw agape.
    let head = at(1.0, 0.3 * 2f64.sin(), 0.7);
    let fwd = v3(along, 0.0);
    lathe(
        b,
        head - fwd * 0.3,
        fwd,
        &[
            (0.0, 0.0),
            (0.45, 0.1),
            (0.55, 0.5),
            (0.4, 1.2),
            (0.25, 1.8),
            (0.0, 2.0),
        ],
        16,
        |_| Some((Surf::Plain, bone)),
    );
    lathe(
        b,
        head - fwd * 0.2 - DVec3::Y * 0.45,
        (fwd + DVec3::Y * -0.25).normalize(),
        &[(0.0, 0.0), (0.22, 0.1), (0.2, 1.4), (0.0, 1.7)],
        12,
        |_| Some((Surf::Plain, rgb(206, 194, 164))),
    );
    for k in 0..6 {
        let q = head + fwd * (0.5 + k as f64 * 0.22) - DVec3::Y * 0.42;
        uv::tube(b, q, q - DVec3::Y * 0.18, 0.03, rgb(240, 236, 220));
    }
    b.surf = Surf::Plain;
    uv::disc(
        b,
        head + fwd * 0.4 + v3(dir, 0.0) * 0.46 + DVec3::Y * 0.12,
        v3(dir, 0.0),
        0.14,
        rgb(40, 34, 30),
    );
    // Legs folded on the floor.
    for (t, sgn) in [(-0.45, 1.0), (-0.45, -1.0), (0.35, 1.0), (0.35, -1.0)] {
        let hip = at(t, 0.3 * (t * 2.0).sin(), 1.0);
        let knee = hip + v3(dir * sgn * 1.2 + along * 0.6, 0.0) - DVec3::Y * 0.7;
        let foot = knee + v3(along * -1.1 + dir * sgn * 0.3, 0.0) - DVec3::Y * (knee.y - y - 0.12);
        crate::carmesh::tube(
            b,
            &[hip, knee, foot],
            (0.09, 0.09),
            8,
            Some((Surf::Plain, bone)),
        );
    }
}

fn pool(b: &mut MeshBuilder, p: DVec2, dir: DVec2, y: f64, s: DVec3) {
    let side = DVec2::new(dir.y, -dir.x);
    let n = 48;
    let pt = |a: f64, k: f64| {
        let wob = 1.0 + 0.08 * (a * 3.0).sin() + 0.05 * (a * 5.0 + 1.0).cos();
        p + side * (a.cos() * s.x * k * wob) + dir * (a.sin() * s.z * k * wob)
    };
    let ring: Vec<DVec3> = (0..n)
        .map(|i| v3(pt(TAU * i as f64 / n as f64, 1.0), y + 0.06))
        .collect();
    b.surf = Surf::Glow;
    b.polygon(&ring, DVec3::Y, rgb(60, 200, 230));
    let inner: Vec<DVec3> = (0..n)
        .map(|i| v3(pt(TAU * i as f64 / n as f64, 0.55), y + 0.07))
        .collect();
    b.polygon(&inner, DVec3::Y, rgb(120, 240, 255));
    b.surf = Surf::Plain;
    for i in 0..(n / 2) {
        let a = TAU * i as f64 / (n / 2) as f64;
        uv::sphere(
            b,
            v3(pt(a, 1.03), y + 0.05),
            0.22 + 0.08 * (i % 3) as f64,
            rgb(110, 104, 96),
        );
    }
}

/// A mole folk's tent: a patched ridge tent with a round door flap.
fn tent(b: &mut MeshBuilder, p: DVec2, dir: DVec2, y: f64, s: DVec3, look: u32) {
    let side = DVec2::new(dir.y, -dir.x);
    let canvas = [rgb(150, 120, 80), rgb(120, 110, 90), rgb(160, 100, 70)][look as usize % 3];
    let (hw, hd, h) = (s.x, s.z, s.y);
    let q = |x: f64, z: f64, yy: f64| v3(p + side * x + dir * z, y + yy);
    // Two roof slopes.
    for sx in [-1.0, 1.0] {
        b.quad(
            [
                q(sx * hw, -hd, 0.0),
                q(sx * hw, hd, 0.0),
                q(0.0, hd, h),
                q(0.0, -hd, h),
            ],
            v3(side * sx, 0.6),
            canvas,
        );
        b.quad(
            [
                q(sx * hw, hd, 0.0),
                q(sx * hw, -hd, 0.0),
                q(0.0, -hd, h),
                q(0.0, hd, h),
            ],
            v3(side * -sx, -0.6),
            rgb(90, 76, 56),
        );
    }
    // Back and front gables, the front with a dark round door.
    b.tri(
        [q(-hw, -hd, 0.0), q(hw, -hd, 0.0), q(0.0, -hd, h)],
        v3(-dir, 0.0),
        canvas,
    );
    b.tri(
        [q(hw, hd, 0.0), q(-hw, hd, 0.0), q(0.0, hd, h)],
        v3(dir, 0.0),
        canvas,
    );
    uv::disc(
        b,
        q(0.0, hd + 0.02, 0.62),
        v3(dir, 0.0),
        0.5,
        rgb(40, 30, 22),
    );
    // A patch and the ridge pole.
    slab(
        b,
        q(hw * 0.5, 0.2, h * 0.5) + v3(side, 0.6).normalize() * 0.02,
        [v3(dir, 0.0), v3(-side * h, hw).normalize()],
        DVec3::new(0.35, 0.3, 0.01),
        rgb(200, 160, 90),
    );
    uv::tube(
        b,
        q(0.0, -hd - 0.15, h + 0.03),
        q(0.0, hd + 0.15, h + 0.03),
        0.04,
        rgb(110, 80, 50),
    );
}

fn campfire(b: &mut MeshBuilder, p: DVec2, y: f64, glows: &mut Vec<Glow>) {
    for k in 0..10 {
        let a = TAU * k as f64 / 10.0;
        uv::sphere(
            b,
            v3(p + DVec2::new(a.cos(), a.sin()) * 0.75, y + 0.1),
            0.17,
            rgb(100, 96, 90),
        );
    }
    for k in 0..4 {
        let a = TAU * k as f64 / 4.0 + 0.4;
        let d = DVec2::new(a.cos(), a.sin());
        uv::tube(
            b,
            v3(p + d * 0.55, y + 0.08),
            v3(p - d * 0.1, y + 0.45),
            0.07,
            rgb(90, 60, 36),
        );
    }
    b.surf = Surf::Glow;
    lathe(
        b,
        v3(p, y + 0.15),
        DVec3::Y,
        &[(0.0, 0.0), (0.4, 0.0), (0.3, 0.35), (0.12, 0.8), (0.0, 1.0)],
        12,
        |_| Some((Surf::Glow, rgb(255, 150, 40))),
    );
    lathe(
        b,
        v3(p, y + 0.15),
        DVec3::Y,
        &[(0.0, 0.0), (0.22, 0.0), (0.15, 0.3), (0.0, 0.6)],
        10,
        |_| Some((Surf::Glow, rgb(255, 230, 120))),
    );
    b.surf = Surf::Plain;
    glows.push(Glow {
        pos: v3(p, y + 1.0),
        color: rgb(255, 150, 60),
        strength: 4.0,
        range: 9.0,
    });
}

/// The end of a line: a riveted buffer with a red lamp.
fn buffer_stop(b: &mut MeshBuilder, p: DVec2, out: DVec2, f: f64, glows: &mut Vec<Glow>) {
    let c = v3(p + out * (TUBE_HALF - 0.2), f + AXIS);
    let n = v3(-out, 0.0);
    lathe(
        b,
        c,
        -n,
        &[
            (0.0, 0.0),
            (SHELL + 0.05, 0.0),
            (SHELL + 0.05, 0.3),
            (0.0, 0.3),
        ],
        32,
        |_| Some((Surf::Metal, rgb(120, 110, 96))),
    );
    for sx in [-0.6, 0.6] {
        let side = v3(out.perp(), 0.0);
        let q = c + side * sx - DVec3::Y * 0.3;
        lathe(
            b,
            q,
            n,
            &[
                (0.0, 0.0),
                (0.22, 0.0),
                (0.22, 0.5),
                (0.3, 0.55),
                (0.3, 0.65),
                (0.0, 0.65),
            ],
            20,
            |_| Some((Surf::Metal, rgb(200, 60, 50))),
        );
    }
    b.surf = Surf::Glow;
    uv::sphere(b, c + DVec3::Y * 0.8 + n * 0.1, 0.12, rgb(255, 50, 40));
    b.surf = Surf::Plain;
    glows.push(Glow {
        pos: c + DVec3::Y * 0.8 + n * 0.4,
        color: rgb(255, 60, 40),
        strength: 2.0,
        range: 6.0,
    });
}

/// The newspaper kiosk over the lab's lift: a green booth with a canted
/// roof, a striped awning and racks of papers, and a door at the back of
/// the shop (the lift inside).
fn kiosk(b: &mut MeshBuilder, p: DVec2, door: DVec2, top: f64) {
    // A 1950s newsstand: enamelled green panels between chrome corner
    // posts, a chrome belt and kick plate, lit side windows full of
    // magazines, a flying-saucer roof and a lit drum sign on a stalk. The
    // walls stand where the fixture's footprints are (back, sides and the
    // front either side of the door).
    let side = door.perp();
    let half = SHAFT_HALF + 0.3;
    let wall_h = 2.9;
    let green = rgb(46, 104, 84);
    let cream = rgb(236, 226, 198);
    let red = rgb(190, 46, 40);
    let chrome = rgb(214, 216, 222);
    let q = |x: f64, z: f64, y: f64| v3(p + side * x + door * z, top + y);
    let before = b.tex;
    b.tex = Some(FacadeLayer::MetalPanel.into());
    // One wall panel, `hx` either side of `c` along `ax`, facing `out`.
    let panel = |b: &mut MeshBuilder, c: DVec3, ax: DVec2, hx: f64, y0: f64, y1: f64, col| {
        slab(
            b,
            c + DVec3::Y * ((y0 + y1) * 0.5 - c.y + top),
            [v3(ax, 0.0), DVec3::Y],
            DVec3::new(hx, (y1 - y0) * 0.5, 0.06),
            col,
        );
    };
    // Back wall, whole.
    panel(b, q(0.0, -half, 0.0), side, half - 0.12, 0.0, wall_h, green);
    // Side walls: below and above a window.
    for sx in [-1.0, 1.0] {
        let c = q(sx * half, 0.0, 0.0);
        panel(b, c, door, half - 0.12, 0.0, 0.95, green);
        panel(b, c, door, half - 0.12, 1.85, wall_h, green);
        for z in [-1.0, 1.0] {
            panel(
                b,
                q(sx * half, z * (half - 0.3), 0.0),
                door,
                0.18,
                0.95,
                1.85,
                green,
            );
        }
    }
    // The front either side of the door, and a lintel over it.
    for sx in [-1.0, 1.0] {
        panel(
            b,
            q(sx * (half + 0.6) * 0.5, half, 0.0),
            side,
            (half - 0.6) * 0.5 - 0.06,
            0.0,
            wall_h,
            green,
        );
    }
    panel(b, q(0.0, half, 0.0), side, 0.62, 2.35, wall_h, cream);
    b.tex = before;
    // Chrome corner posts, and a chrome belt and kick plate all round.
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        lathe(
            b,
            q(sx * half, sz * half, 0.0),
            DVec3::Y,
            &[(0.0, 0.0), (0.13, 0.0), (0.13, wall_h), (0.0, wall_h)],
            24,
            |_| Some((Surf::Metal, chrome)),
        );
    }
    for (y, h) in [(0.12, 0.12), (1.9, 0.05)] {
        for (c, ax, hx) in [
            (q(0.0, -half, y), side, half),
            (q(-half, 0.0, y), door, half),
            (q(half, 0.0, y), door, half),
        ] {
            let out = (c - q(0.0, 0.0, y)).normalize_or(DVec3::X);
            b.surf = Surf::Metal;
            slab(
                b,
                c + out * 0.07,
                [v3(ax, 0.0), DVec3::Y],
                DVec3::new(hx, h, 0.015),
                chrome,
            );
            b.surf = Surf::Plain;
        }
    }
    // Lit side windows with magazines on wire racks behind the glass.
    for sx in [-1.0, 1.0] {
        let c = q(sx * half, 0.0, 1.4);
        b.surf = Surf::Glow;
        slab(
            b,
            c - v3(side * sx * 0.03, 0.0),
            [v3(door, 0.0), DVec3::Y],
            DVec3::new(half - 0.5, 0.44, 0.01),
            rgb(255, 238, 200),
        );
        b.surf = Surf::Plain;
        for row in 0..2 {
            for k in 0..6 {
                let colr = [
                    rgb(220, 50, 44),
                    rgb(240, 196, 50),
                    rgb(50, 110, 210),
                    rgb(244, 240, 228),
                    rgb(50, 170, 84),
                    rgb(230, 110, 40),
                ][(k + row * 3) % 6];
                slab(
                    b,
                    c + v3(side * sx * 0.03, 0.0)
                        + v3(door, 0.0) * (-(half - 0.75) + k as f64 * (half - 0.75) * 0.4)
                        + DVec3::Y * (row as f64 * 0.42 - 0.2),
                    [v3(door, 0.0), v3(side * sx * 0.25, 1.0).normalize()],
                    DVec3::new(0.13, 0.17, 0.01),
                    colr,
                );
            }
        }
        b.surf = Surf::Glass;
        slab(
            b,
            c + v3(side * sx * 0.06, 0.0),
            [v3(door, 0.0), DVec3::Y],
            DVec3::new(half - 0.5, 0.45, 0.01),
            rgb(170, 200, 210),
        );
        b.surf = Surf::Plain;
        // A cream counter ledge under the window.
        slab(
            b,
            q(sx * (half + 0.2), 0.0, 0.92),
            [v3(door, 0.0), DVec3::Y],
            DVec3::new(half - 0.3, 0.03, 0.18),
            cream,
        );
    }
    // The flying-saucer roof: a red lens with a cream underside and a
    // chrome rim, overhanging all round.
    let o = half * 1.45;
    let roof = |y: f64| q(0.0, 0.0, wall_h + y);
    lathe(
        b,
        roof(0.0),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (o * 0.7, 0.02),
            (o, 0.16),
            (o + 0.05, 0.22),
            (o, 0.3),
            (o * 0.75, 0.45),
            (o * 0.35, 0.58),
            (0.0, 0.62),
        ],
        40,
        |k| {
            Some(match k {
                0 | 1 => (Surf::Plain, cream),
                2 | 3 => (Surf::Metal, chrome),
                _ => (Surf::Plain, red),
            })
        },
    );
    // A ring of lights under the rim.
    b.surf = Surf::Glow;
    for k in 0..16 {
        let a = TAU * k as f64 / 16.0;
        let d = side * a.cos() + door * a.sin();
        uv::sphere(
            b,
            v3(p + d * (o * 0.86), top + wall_h + 0.08),
            0.06,
            rgb(255, 226, 160),
        );
    }
    b.surf = Surf::Plain;
    // The drum sign on a chrome stalk, lit, with red bands.
    lathe(
        b,
        roof(0.6),
        DVec3::Y,
        &[(0.0, 0.0), (0.06, 0.0), (0.06, 0.55), (0.0, 0.55)],
        12,
        |_| Some((Surf::Metal, chrome)),
    );
    lathe(
        b,
        roof(1.15),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.5, 0.0),
            (0.55, 0.06),
            (0.55, 0.14),
            (0.55, 0.56),
            (0.55, 0.64),
            (0.5, 0.7),
            (0.0, 0.7),
        ],
        32,
        |k| {
            Some(match k {
                3 => (Surf::Glow, rgb(255, 236, 196)),
                1 | 5 => (Surf::Metal, chrome),
                _ => (Surf::Plain, red),
            })
        },
    );
    // A starburst on top.
    b.surf = Surf::Glow;
    uv::sphere(b, roof(2.0), 0.1, rgb(255, 210, 90));
    for k in 0..6 {
        let a = PI * k as f64 / 6.0;
        let d = DVec3::new(a.cos(), a.sin() * 0.7, a.sin() * 0.7).normalize() * 0.28;
        let w = DVec3::Y.cross(d).normalize_or(DVec3::X) * 0.015;
        let c = roof(2.0);
        b.quad(
            [c - d - w, c + d - w, c + d + w, c - d + w],
            DVec3::Y.cross(w),
            rgb(255, 200, 80),
        );
    }
    b.surf = Surf::Plain;
    // A scalloped striped awning over the door.
    let rows: Vec<Vec<DVec3>> = (0..=16)
        .map(|i| {
            let x = -1.0 + 2.0 * i as f64 / 16.0;
            let drop = 0.1 * (i as f64 * PI / 2.0).sin().abs();
            vec![
                q(x, half + 0.05, 2.6),
                q(x, half + 0.75, 2.25),
                q(x, half + 0.8, 2.08 - drop),
            ]
        })
        .collect();
    surface(b, &rows, false, |i, _| {
        Some((Surf::Plain, if (i / 2) % 2 == 0 { red } else { cream }))
    });
    // Inside: a lit ceiling disc over the hole and a red bulb.
    b.surf = Surf::Glow;
    slab(
        b,
        q(0.0, 0.0, wall_h - 0.05),
        [v3(side, 0.0), v3(door, 0.0)],
        DVec3::new(half * 0.6, half * 0.6, 0.02),
        rgb(255, 228, 190),
    );
    uv::sphere(b, q(0.0, half - 0.4, wall_h - 0.4), 0.08, rgb(255, 80, 60));
    b.surf = Surf::Plain;
}

/// An Aether Tube capsule, its floor point at the origin and its nose to
/// +Z: a cream streamliner with a red belt and nose, a chrome tail cone and bumper, a
/// bubble canopy, a tail fin and side fins, and a headlamp. About 6.8 m
/// long and 2.5 m across, it fits the tube with room to spare.
pub fn capsule_mesh() -> Mesh {
    let mut b = MeshBuilder::new();
    let cream = rgb(238, 230, 206);
    let red = rgb(186, 44, 40);
    let chrome = rgb(214, 216, 222);
    let axis_y = 1.35;
    let len = 6.8;
    let tail = DVec3::new(0.0, axis_y, -len * 0.5);
    // The body, tail to nose: (radius, along).
    let body: Vec<(f64, f64)> = {
        let mut v = vec![(0.0, -0.05), (0.38, 0.0), (0.72, 0.25), (0.98, 0.65)];
        v.extend([
            (1.12, 1.1),
            (1.2, 1.7),
            (1.22, 2.6),
            (1.22, 3.6),
            (1.2, 4.4),
            (1.12, 5.0),
        ]);
        // A rounded nose.
        for k in 1..=8 {
            let a = k as f64 / 8.0 * FRAC_PI_2;
            v.push((1.12 * a.cos(), 5.0 + 1.75 * a.sin()));
        }
        v
    };
    let rings = body.len();
    lathe(&mut b, tail, DVec3::Z, &body, 40, |k| {
        Some(match k {
            // Chrome tail cone, then cream, with a red belt and a red nose.
            0..=2 => (Surf::Metal, chrome),
            5 | 6 => (Surf::Plain, red),
            _ if k + 6 >= rings => (Surf::Plain, red),
            _ => (Surf::Plain, cream),
        })
    });
    // Chrome bumper ring round the nose.
    lathe(
        &mut b,
        tail + DVec3::Z * 4.95,
        DVec3::Z,
        &[(1.13, 0.0), (1.2, 0.04), (1.2, 0.14), (1.13, 0.18)],
        40,
        |_| Some((Surf::Metal, chrome)),
    );
    // The bubble canopy, sunk into the body's top.
    let canopy: Vec<(f64, f64)> = (0..=10)
        .map(|k| {
            let a = k as f64 / 10.0 * PI;
            (0.86 * a.sin(), 1.8 - 1.55 * a.cos())
        })
        .collect();
    lathe(
        &mut b,
        tail + DVec3::new(0.0, 0.62, 1.9),
        DVec3::Z,
        &canopy,
        36,
        |_| Some((Surf::Glass, rgb(150, 196, 214))),
    );
    // Seats under the glass.
    for z in [2.7, 3.9] {
        slab(
            &mut b,
            tail + DVec3::new(0.0, 0.25, z),
            [DVec3::X, DVec3::Y],
            DVec3::new(0.55, 0.35, 0.3),
            red,
        );
    }
    // Tail fin on top and two side fins, swept back.
    let fin = |b: &mut MeshBuilder, root: DVec3, up: DVec3, reach: f64| {
        let back = -DVec3::Z;
        let pts = [
            root + back * 0.1,
            root + DVec3::Z * 1.3,
            root + up * reach + back * 0.5,
            root + up * reach * 0.8 + back * 0.95,
        ];
        let side = up.cross(DVec3::Z).normalize() * 0.04;
        b.hexa(pts.map(|p| p - side), pts.map(|p| p + side), red);
    };
    fin(&mut b, tail + DVec3::new(0.0, 0.85, 0.3), DVec3::Y, 0.95);
    for sx in [-1.0, 1.0] {
        let up = DVec3::new(sx * 0.94, -0.34, 0.0);
        fin(&mut b, tail + DVec3::new(sx * 0.82, -0.3, 0.4), up, 0.6);
    }
    // A headlamp in the nose, and tail lamps.
    b.surf = Surf::Glow;
    uv::sphere(
        &mut b,
        tail + DVec3::new(0.0, 0.0, len - 0.04),
        0.16,
        rgb(255, 240, 200),
    );
    for sx in [-0.45, 0.45] {
        uv::sphere(
            &mut b,
            tail + DVec3::new(sx, 0.0, 0.12),
            0.09,
            rgb(255, 70, 50),
        );
    }
    b.surf = Surf::Plain;
    // A starburst on each flank, in brass.
    for sx in [-1.0, 1.0] {
        let c = tail + DVec3::new(sx * 1.225, 0.0, 3.1);
        for r in 0..8 {
            let a = r as f64 * PI / 8.0;
            let d = DVec3::new(0.0, a.sin(), a.cos()) * if r % 2 == 0 { 0.32 } else { 0.2 };
            let n = DVec3::X * sx * 0.012;
            let w = DVec3::new(0.0, -a.cos(), a.sin()) * 0.025;
            b.quad(
                [c - d - w + n, c + d - w + n, c + d + w + n, c - d + w + n],
                DVec3::X * sx,
                rgb(206, 160, 70),
            );
        }
    }
    b.finish()
}

/// The platform of the tube station nearest `near`, and the line's way
/// there (where `HW_VIEW=capsule` stands a capsule).
pub fn capsule_spot(town: &TownWorld, near: DVec2) -> Option<(DVec3, DVec2)> {
    let net = town.tube();
    let s = (0..net.stations.len()).min_by(|&a, &b| {
        net.stations[a]
            .pos
            .distance(near)
            .total_cmp(&net.stations[b].pos.distance(near))
    })?;
    Some(net.platform(s))
}

/// A camera for the deep underground's `HW_VIEW`s (see main.rs), looking
/// at the nearest such place to `near`.
pub fn view_camera(town: &TownWorld, view: &str, near: DVec2) -> Option<FlyCamera> {
    let cam = |p: DVec3, at: DVec3| {
        let d = at - p;
        let yaw = d.x.atan2(d.z).to_degrees() as f32;
        let pitch = (d.y / DVec2::new(d.x, d.z).length().max(0.1))
            .atan()
            .to_degrees() as f32;
        FlyCamera::new((p.x as f32, p.y as f32, p.z as f32), yaw, pitch)
    };
    let mut tiles = town.deep_near(near, 1400.0);
    tiles.sort_by(|a, b| {
        let da = ((a.lo + a.hi) * 0.5).distance(near);
        let db = ((b.lo + b.hi) * 0.5).distance(near);
        da.total_cmp(&db)
    });
    if view == "capsule" {
        // From the walkway ahead of the capsule, a little to one side.
        let (p, d) = capsule_spot(town, near)?;
        let eye = p + DVec3::new(d.x, 0.0, d.y) * 6.5 + DVec3::new(-d.y, 0.0, d.x) * 0.6;
        return Some(cam(eye + DVec3::Y * 1.7, p + DVec3::Y * 1.2));
    }
    if view == "lab" || view == "kiosk" {
        tiles = vec![town.deep_tile(tile_of(town.lair_centre()?))];
    }
    for t in &tiles {
        match view {
            "station" if !t.stations.is_empty() => {
                let h = &t.voids[t.stations[0].hall];
                let p = h.world(DVec2::new(h.min.x + 3.0, h.max.y * 0.6));
                let at = h.world(DVec2::new(h.max.x * 0.4, -h.max.y * 0.3));
                return Some(cam(
                    v3d(p, h.floor_at(p) + 2.4),
                    v3d(at, h.floor_at(at) + 2.0),
                ));
            }
            "tubeline" => {
                if let Some(v) = t
                    .voids
                    .iter()
                    .find(|v| v.kind == VoidKind::Tube && v.max.x - v.min.x > 15.0)
                {
                    // Tube voids are short stretches: look on along the
                    // line past this one.
                    let p = v.world(DVec2::new(v.min.x + 2.0, 0.0));
                    let at = p + v.along * 40.0;
                    return Some(cam(
                        v3d(p, v.floor_at(p) + 1.7),
                        v3d(at, v.floor_at(at) + 1.4),
                    ));
                }
            }
            "tube" | "kiosk" => {
                let want = if view == "tube" {
                    EntranceKind::Tube
                } else {
                    EntranceKind::Kiosk
                };
                if let Some(e) = t.entrances.iter().find(|e| e.kind == want) {
                    let p = e.pos + e.door * 9.0 + e.door.perp() * 4.0;
                    let g = town.ground(p);
                    return Some(cam(v3d(p, g + 2.6), v3d(e.pos, e.top + 2.0)));
                }
            }
            "ruin" if !t.ruins.is_empty() => {
                let r = &t.ruins[0];
                let v = &t.voids[r.street];
                let p = v.world(DVec2::new(v.min.x + 1.5, 0.0));
                let at = v.world(DVec2::new(v.max.x * 0.6, 0.0));
                return Some(cam(v3d(p, r.floor + 3.0), v3d(at, r.floor + 3.0)));
            }
            "lab" if !t.lairs.is_empty() => {
                let l = &t.lairs[0];
                let v = &t.voids[l.hall];
                // From high in a corner, over the coils, across the hall.
                let p = v.world(DVec2::new(v.min.x + 1.5, v.max.y - 1.5));
                let at = v.world(DVec2::new(v.max.x * 0.3, -v.max.y * 0.3));
                return Some(cam(v3d(p, l.floor + 6.5), v3d(at, l.floor + 1.0)));
            }
            "grotto" if !t.grottos.is_empty() => {
                let g = &t.grottos[0];
                let v = &t.voids[g.void];
                let p = v.world(DVec2::new(v.min.x + 2.5, v.min.y + 2.5));
                return Some(cam(v3d(p, g.floor + 3.0), v3d(g.centre, g.floor + 1.5)));
            }
            _ => {}
        }
    }
    None
}

fn v3d(p: DVec2, y: f64) -> DVec3 {
    DVec3::new(p.x, y, p.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_deep_tile_builds() {
        let w = TownWorld::generate(
            hw_core::Seed(3),
            hw_gen::urban::TownParams::sample(hw_core::Seed(3), 40_000.0),
        );
        let (core, _) = w.spawn_line();
        let mut tris = 0;
        for dx in -1..=1 {
            for dy in -1..=1 {
                let k = tile_of(core);
                let t = w.deep_tile((k.0 + dx, k.1 + dy));
                let b = build(&w, &t);
                tris += b.inner.iter().map(|m| m.face_count()).sum::<usize>();
            }
        }
        assert!(tris > 10_000, "{tris} triangles");
    }
}
