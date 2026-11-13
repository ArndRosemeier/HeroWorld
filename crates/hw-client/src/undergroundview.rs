//! The town below the town, drawn: concrete-lined utility tunnels with
//! pipes, cable trays and an Aether conduit along the ceiling; tiled lift
//! shafts under Googie pavilions on the pavements; and Molehaven, the mole
//! people's cavern, with mushroom houses, glowing toadstools, crystals and
//! a market round a well.
//!
//! The ground itself is carved by Engine's voxel terrain from the same
//! voids ([`carve`], used by `TownGround::density`); the linings here sit
//! just inside the carved rock. Lamps come on near the camera while it is
//! down there.

use crate::carmesh::lathe;
use crate::citymesh::{MeshBuilder, Surf};
use crate::tilestream::{Built, Glow, Key, Layer, Reach, Settings, UnderTiles, UnderView};
use engine::prelude::*;
use engine::texgen::{FacadeLayer, GroundLayer};
use glam::{DVec2, DVec3, Vec3};
use hw_gen::urban::spiral::{Spiral, Step, RAIL_HEIGHT, RAIL_T, RING, SLAB};
use hw_gen::urban::underground::{Underground, Void, VoidKind};
use hw_gen::urban::world::TownWorld;
use std::collections::BTreeSet;
use std::f64::consts::{FRAC_PI_2, PI, TAU};
use std::sync::Arc;

/// Lamps lit at once underground.
const POOL: usize = 28;
/// Cells of floor, wall and ceiling linings (m).
const STEP: f64 = 1.5;
/// Group linings in squares this big, one mesh each.
const CHUNK: f64 = 192.0;

/// How the rock is carved round a void: signed distance, negative in the
/// open (Engine's density is solid where positive). Tunnels are carved a
/// little beyond their linings; the cavern rougher and wider.
pub fn carve(v: &Void, p: DVec3) -> f64 {
    let q = DVec2::new(p.x, p.z);
    let l = v.local(q);
    let (side, below, above) = match v.kind {
        VoidKind::Gallery | VoidKind::Grotto => {
            // Natural rock: lumpy walls and roof, the floor where it is.
            let n = (p.x * 0.23).sin() * (p.z * 0.19).cos() + (p.y * 0.31 + p.z * 0.09).sin() * 0.6;
            (1.0 + n * 0.8, 0.05, 1.2 + n * 0.7)
        }
        VoidKind::Cavern => {
            // Rough walls: a few metres of lumps.
            let n = (p.x * 0.21).sin() * (p.z * 0.17).cos() + (p.y * 0.33 + p.x * 0.07).sin() * 0.6;
            (1.2 + n, 0.3, 1.5 + n * 0.8)
        }
        _ => (0.6, 0.35, 0.6),
    };
    let floor = v.floor_at(q);
    let mut d = (v.min.x - side - l.x)
        .max(l.x - v.max.x - side)
        .max(v.min.y - side - l.y)
        .max(l.y - v.max.y - side)
        .max(floor - below - p.y);
    if let Some(c) = v.ceiling_at(q) {
        d = d.max(p.y - c - above);
    }
    d
}

/// Engine's density with the underground carved out of `ground`.
pub fn density(u: &Underground, p: Vec3, ground: f32) -> f32 {
    let mut d = ground - p.y;
    if p.y > ground + 0.5 {
        return d;
    }
    let q = DVec2::new(p.x as f64, p.z as f64);
    for i in u.near(q, 3.0) {
        let v = &u.voids[i];
        d = d.min(carve(v, p.as_dvec3()) as f32);
    }
    d
}

/// How the shallow underground streams: the pavilions on the street
/// widely, the tunnels and Molehaven while the camera is down there or by a
/// lift.
pub const SETTINGS: Settings = Settings {
    name: "underground",
    size: CHUNK,
    street: Reach {
        load: 700.0,
        drop: 900.0,
    },
    below: Reach {
        load: 320.0,
        drop: 480.0,
    },
    near: 80.0,
    under: 1.0,
    lamp_range: 90.0,
    pool: POOL,
};

/// The town below the town in the world, streamed by distance.
pub struct UndergroundView(pub UnderView);

impl UndergroundView {
    /// Starts the worker; tiles come in from the first update.
    pub fn new(town: Arc<TownWorld>) -> Self {
        Self(UnderView::new(Self::tiles(town)))
    }

    /// The bookkeeping alone (no `World`): tests walk it across a town.
    pub fn tiles(town: Arc<TownWorld>) -> UnderTiles {
        // The chunks that hold anything (the plan is small and bounded).
        let u = town.underground();
        let mut used: BTreeSet<(i64, i64)> = u
            .voids
            .iter()
            .map(|v| chunk_of(v.world((v.min + v.max) * 0.5)))
            .collect();
        used.extend(u.entrances.iter().map(|e| chunk_of(e.pos)));
        used.extend(u.lamps.iter().map(|l| chunk_of(DVec2::new(l.x, l.z))));
        used.extend(u.cavern.as_ref().map(|c| chunk_of(c.centre)));
        UnderTiles::new(
            SETTINGS,
            move |k| used.contains(&k),
            move |k| build(&town, k),
        )
    }

    /// Builds the tiles round `eye` now (first frame, screenshots).
    pub fn prime(&mut self, world: &mut World, town: &TownWorld, eye: Vec3) -> EngineResult<()> {
        let g = town.ground(DVec2::new(eye.x as f64, eye.z as f64));
        self.0.prime(world, eye, g, |k| build(town, k))
    }

    pub fn update(&mut self, world: &mut World, eye: Vec3, town: &TownWorld) -> EngineResult<()> {
        let g = town.ground(DVec2::new(eye.x as f64, eye.z as f64));
        self.0.update(world, eye, g)
    }

    pub fn clear(&mut self, world: &mut World) {
        self.0.clear(world);
    }
}

fn chunk_of(p: DVec2) -> (i64, i64) {
    ((p.x / CHUNK).floor() as i64, (p.y / CHUNK).floor() as i64)
}

fn chunk_centre(k: (i64, i64)) -> DVec2 {
    DVec2::new((k.0 as f64 + 0.5) * CHUNK, (k.1 as f64 + 0.5) * CHUNK)
}

/// One layer of one chunk: what belongs to the chunk holding its middle.
/// Pure data, no `World`: runs on the stream's worker.
pub fn build(town: &TownWorld, key: Key) -> Built {
    let (layer, k) = (key.0, (key.1, key.2));
    let u = town.underground();
    let mut b = MeshBuilder::default();
    let mut out = Built::default();
    for e in u.entrances.iter().filter(|e| chunk_of(e.pos) == k) {
        let col = if e.molehaven {
            rgb(255, 170, 90)
        } else {
            rgb(130, 230, 255)
        };
        match layer {
            Layer::Street => {
                pavilion(&mut b, e.pos, e.door, e.top, e.molehaven);
                collar(&mut b, e.pos, e.door, town);
                out.openings.push(e.pos);
            }
            Layer::Below => {
                out.glows.push(Glow {
                    pos: foot_lamp(&e.spiral),
                    color: col,
                    strength: 6.0,
                    range: 10.0,
                });
                spiral_stair(&mut b, &e.spiral);
                out.glows.extend(stair_lamps(&e.spiral));
            }
        }
    }
    if layer == Layer::Below {
        for i in u.near(chunk_centre(k), CHUNK) {
            let v = &u.voids[i];
            if chunk_of(v.world((v.min + v.max) * 0.5)) != k {
                continue;
            }
            lining(&mut b, u, i, town);
            if v.kind == VoidKind::Tunnel {
                tunnel_fittings(&mut b, u, i);
            }
        }
        for l in u
            .lamps
            .iter()
            .filter(|l| chunk_of(DVec2::new(l.x, l.z)) == k)
        {
            let in_cavern = u
                .cavern
                .as_ref()
                .is_some_and(|c| (l.y - c.floor).abs() < 4.0);
            out.glows.push(Glow {
                pos: *l,
                color: if in_cavern {
                    rgb(150, 120, 255)
                } else {
                    rgb(255, 226, 180)
                },
                strength: if in_cavern { 9.0 } else { 7.0 },
                range: if in_cavern { 16.0 } else { 13.0 },
            });
        }
        if u.cavern.as_ref().is_some_and(|c| chunk_of(c.centre) == k) {
            molehaven(&mut b, u, &mut out.glows);
        }
    }
    out.meshes = b.finish_parts().into_iter().map(|m| m.prepare()).collect();
    out
}

pub(crate) fn v3(p: DVec2, y: f64) -> DVec3 {
    DVec3::new(p.x, y, p.y)
}

/// Is `p` in the open space of a void other than `i`?
fn open_elsewhere(u: &Underground, i: usize, p: DVec3) -> bool {
    u.at(DVec2::new(p.x, p.z))
        .any(|(j, v)| j != i && v.contains(p))
}

/// Floor, walls and ceiling of void `i`, in cells, leaving out what another
/// void opens onto (junctions, side passages, the shafts' open tops).
fn lining(b: &mut MeshBuilder, u: &Underground, i: usize, town: &TownWorld) {
    let v = &u.voids[i];
    let mid = v.world((v.min + v.max) * 0.5);
    let near: Vec<&Void> = u
        .near(mid, (v.max - v.min).length() * 0.5 + 4.0)
        .into_iter()
        .filter(|&j| j != i)
        .map(|j| &u.voids[j])
        .collect();
    lining_of(
        b,
        v,
        town,
        |mid, f| {
            !u.at(mid).any(|(j, o)| {
                j < i && (o.floor_at(mid) - f).abs() < 0.6 && o.kind != VoidKind::Shaft
            })
        },
        &near,
        |p| open_elsewhere(u, i, p),
    );
}

/// The part of convex polygon `poly` (plan) where `n·p <= d`.
fn clip(poly: &[DVec2], n: DVec2, d: f64) -> Vec<DVec2> {
    let mut out = Vec::with_capacity(poly.len() + 1);
    for k in 0..poly.len() {
        let (a, c) = (poly[k], poly[(k + 1) % poly.len()]);
        let (da, dc) = (a.dot(n) - d, c.dot(n) - d);
        if da <= 0.0 {
            out.push(a);
        }
        if (da < 0.0) != (dc < 0.0) && (da - dc).abs() > 1e-12 {
            out.push(a + (c - a) * (da / (da - dc)));
        }
    }
    if out.len() < 3 {
        out.clear();
    }
    out
}

/// Convex `poly` less the rectangle of void `s`: up to four convex pieces.
fn minus_rect(poly: &[DVec2], s: &Void) -> Vec<Vec<DVec2>> {
    let (ax, ay) = (s.along, s.inward());
    let (x0, x1) = (s.origin.dot(ax) + s.min.x, s.origin.dot(ax) + s.max.x);
    let (y0, y1) = (s.origin.dot(ay) + s.min.y, s.origin.dot(ay) + s.max.y);
    let left = clip(poly, ax, x0);
    let right = clip(poly, -ax, -x1);
    let mid = clip(&clip(poly, -ax, -x0), ax, x1);
    let below = clip(&mid, ay, y0);
    let above = clip(&mid, -ay, -y1);
    [left, right, below, above]
        .into_iter()
        .filter(|p| p.len() >= 3)
        .collect()
}

/// Does the floor of shaft `s` lie wholly on another void's floor at the
/// same height (a room it stands in), so that one draws it?
fn floor_inside(s: &Void, others: &[&Void]) -> bool {
    let f = s.floor_at(s.world((s.min + s.max) * 0.5));
    let corners = [
        s.min,
        DVec2::new(s.max.x, s.min.y),
        s.max,
        DVec2::new(s.min.x, s.max.y),
    ];
    others.iter().any(|o| {
        o.kind != VoidKind::Shaft
            && corners.iter().all(|c| {
                let p = s.world(*c);
                o.covers(p, 0.01) && (o.floor_at(p) - f).abs() < 0.6
            })
    })
}

/// Floor, walls and ceiling of a concrete void (tunnel, passage, shaft):
/// `owns_floor(mid, floor)` says whether this void draws the floor cell at
/// `mid` (one floor where voids overlap), `elsewhere(p)` whether `p` is in
/// another void's open space (so a wall there would block an opening).
pub(crate) fn lining_of(
    b: &mut MeshBuilder,
    v: &Void,
    town: &TownWorld,
    owns_floor: impl Fn(DVec2, f64) -> bool,
    others: &[&Void],
    elsewhere: impl Fn(DVec3) -> bool,
) {
    // Where a shaft meets another void at its foot, the shaft draws the
    // floor of its own square and the other leaves that square out, so no
    // two floors lie on each other; a shaft standing in a room leaves the
    // floor to the room.
    let shafts: Vec<&Void> = if v.kind == VoidKind::Shaft {
        Vec::new()
    } else {
        others
            .iter()
            .copied()
            .filter(|o| o.kind == VoidKind::Shaft && !floor_inside(o, &[v]))
            .collect()
    };
    let shaft_floor = v.kind != VoidKind::Shaft || !floor_inside(v, others);
    let size = v.max - v.min;
    let (nx, ny) = (
        (size.x / STEP).ceil().max(1.0) as usize,
        (size.y / STEP).ceil().max(1.0) as usize,
    );
    let at = |x: f64, y: f64| v.world(v.min + DVec2::new(size.x * x, size.y * y));
    let cavern = v.kind == VoidKind::Cavern;
    let (floor_col, wall_col) = match v.kind {
        VoidKind::Cavern => (rgb(120, 98, 72), rgb(110, 96, 80)),
        VoidKind::Shaft => (rgb(70, 74, 80), rgb(226, 218, 196)),
        _ => (rgb(150, 148, 142), rgb(178, 174, 166)),
    };
    // Floor and ceiling cells.
    for ix in 0..nx {
        for iy in 0..ny {
            let (x0, x1) = (ix as f64 / nx as f64, (ix + 1) as f64 / nx as f64);
            let (y0, y1) = (iy as f64 / ny as f64, (iy + 1) as f64 / ny as f64);
            let corners = [at(x0, y0), at(x1, y0), at(x1, y1), at(x0, y1)];
            let mid = at((x0 + x1) * 0.5, (y0 + y1) * 0.5);
            let f = v.floor_at(mid);
            // One floor where voids overlap: the first one's, a shaft's
            // own square always its own.
            let owns = if v.kind == VoidKind::Shaft {
                shaft_floor
            } else {
                owns_floor(mid, f)
            };
            if owns {
                b.tex = Some(if cavern {
                    GroundLayer::Cobble.into()
                } else {
                    GroundLayer::Paving.into()
                });
                let mut pieces = vec![corners.to_vec()];
                for o in shafts.iter().filter(|o| (o.floor_at(mid) - f).abs() < 0.6) {
                    pieces = pieces.iter().flat_map(|p| minus_rect(p, o)).collect();
                }
                for p in pieces {
                    for k in 1..p.len() - 1 {
                        let t = [p[0], p[k], p[k + 1]].map(|c| v3(c, v.floor_at(c)));
                        b.tri(t, DVec3::Y, floor_col);
                    }
                }
                b.tex = None;
            }
            if cavern || v.open {
                continue;
            }
            if let Some(c) = v.ceiling_at(mid) {
                if !elsewhere(v3(mid, c + 0.4)) {
                    let pts = corners.map(|p| v3(p, v.ceiling_at(p).unwrap_or(c)));
                    b.quad(pts, -DVec3::Y, rgb(150, 148, 144));
                }
            }
        }
    }
    if cavern {
        return;
    }
    // Walls, where rock is behind them.
    let sides: [(DVec2, DVec2, DVec2); 4] = [
        (v.min, DVec2::new(v.max.x, v.min.y), DVec2::new(0.0, -1.0)),
        (DVec2::new(v.min.x, v.max.y), v.max, DVec2::new(0.0, 1.0)),
        (v.min, DVec2::new(v.min.x, v.max.y), DVec2::new(-1.0, 0.0)),
        (DVec2::new(v.max.x, v.min.y), v.max, DVec2::new(1.0, 0.0)),
    ];
    for (a, c, out_l) in sides {
        let len = a.distance(c);
        let n = (len / STEP).ceil().max(1.0) as usize;
        let out = (v.along * out_l.x + v.inward() * out_l.y).normalize();
        for k in 0..n {
            let p0 = v.world(a.lerp(c, k as f64 / n as f64));
            let p1 = v.world(a.lerp(c, (k + 1) as f64 / n as f64));
            let m = (p0 + p1) * 0.5;
            let f = v.floor_at(m);
            if elsewhere(v3(m + out * 0.4, f + 1.2)) {
                continue;
            }
            let top = |p: DVec2| match v.ceiling_at(p) {
                Some(t) => t,
                None => town.ground(p) + 0.05,
            };
            b.tex = Some(match v.kind {
                // Glazed cream tiles down a stairwell.
                VoidKind::Shaft => FacadeLayer::BrickWhite.into(),
                _ if v.open => FacadeLayer::Limestone.into(),
                _ => FacadeLayer::Concrete.into(),
            });
            b.quad(
                [
                    v3(p0, v.floor_at(p0)),
                    v3(p1, v.floor_at(p1)),
                    v3(p1, top(p1)),
                    v3(p0, top(p0)),
                ],
                -v3(out, 0.0),
                wall_col,
            );
            b.tex = None;
            // A painted band at waist height in the tunnels.
            if v.kind != VoidKind::Shaft {
                let inset = -out * 0.01;
                b.quad(
                    [
                        v3(p0 + inset, v.floor_at(p0) + 0.9),
                        v3(p1 + inset, v.floor_at(p1) + 0.9),
                        v3(p1 + inset, v.floor_at(p1) + 1.15),
                        v3(p0 + inset, v.floor_at(p0) + 1.15),
                    ],
                    -v3(out, 0.0),
                    rgb(40, 120, 128),
                );
            }
        }
    }
}

/// A round pipe from `a` to `c`, open at the ends, in the builder's
/// current surface.
pub(crate) fn tube(b: &mut MeshBuilder, a: DVec3, c: DVec3, r: f64, col: Color) {
    let len = (c - a).length();
    if len < 1e-6 {
        return;
    }
    let surf = b.surf;
    let seg = if r < 0.06 { 8 } else { 24 };
    // Closed ends, with a flange collar where a run starts and stops.
    let f = (r * 1.25).min(r + 0.05);
    let prof = if len > 0.6 && r >= 0.06 {
        // Tight pairs of points keep the flange's edges crisp.
        let e = 0.006;
        vec![
            (0.0, 0.0),
            (f - e, 0.0),
            (f, e),
            (f, 0.06 - e),
            (f - e, 0.06),
            (r + e, 0.06),
            (r, 0.06 + e),
            (r, len - 0.06 - e),
            (r + e, len - 0.06),
            (f - e, len - 0.06),
            (f, len - 0.06 + e),
            (f, len - e),
            (f - e, len),
            (0.0, len),
        ]
    } else {
        let e = (r * 0.1).min(0.006);
        vec![
            (0.0, 0.0),
            (r - e, 0.0),
            (r, e),
            (r, len - e),
            (r - e, len),
            (0.0, len),
        ]
    };
    lathe(b, a, c - a, &prof, seg, |_| Some((surf, col)));
}

/// Pipes and the Aether conduit along a tunnel piece, broken off where
/// another tunnel crosses so they never run through its open space.
fn tunnel_fittings(b: &mut MeshBuilder, u: &Underground, i: usize) {
    let v = &u.voids[i];
    let half = v.max.y;
    let len = v.max.x - half;
    if len < 0.5 {
        return;
    }
    let side = v.inward();
    let at = |x: f64, off: f64, y: f64| {
        let p = v.world(DVec2::new(x, 0.0)) + side * off;
        v3(p, v.floor_at(p) + y)
    };
    // Runs of [x0, x1] along the piece where `(off, y)` is clear of others.
    let runs = |off: f64, y: f64| {
        let n = (len / 1.0).ceil().max(1.0) as usize;
        let mut out: Vec<(f64, f64)> = Vec::new();
        let mut start: Option<f64> = None;
        for k in 0..=n {
            let x = len * k as f64 / n as f64;
            let clear = k < n && {
                let m = len * (k as f64 + 0.5) / n as f64;
                !open_elsewhere(u, i, at(m, off, y))
            };
            match (clear, start) {
                (true, None) => start = Some(x),
                (false, Some(s0)) => {
                    if x - s0 > 0.8 {
                        out.push((s0, x));
                    }
                    start = None;
                }
                _ => {}
            }
        }
        out
    };
    b.surf = Surf::Metal;
    for (off, y, r, col) in [
        (half - 0.35, 0.55, 0.22, rgb(150, 60, 50)),
        (half - 0.35, 1.05, 0.16, rgb(70, 110, 150)),
        (-(half - 0.3), 2.6, 0.12, rgb(120, 124, 128)),
    ] {
        for (x0, x1) in runs(off, y) {
            tube(b, at(x0, off, y), at(x1, off, y), r, col);
            // Brackets every few metres.
            let mut x = x0 + 1.0;
            while x < x1 - 0.5 {
                let p = at(x, off, y);
                let wall = p + v3(side * off.signum() * (r + 0.12), 0.0);
                tube(b, p, wall, 0.04, rgb(90, 92, 96));
                x += 3.0;
            }
        }
    }
    // The Aether conduit: a glowing strip along the ceiling's middle.
    b.surf = Surf::Glow;
    let w = 0.12;
    let top = v.height - 0.04;
    for (x0, x1) in runs(0.0, top - 0.3) {
        b.quad(
            [
                at(x0, -w, top),
                at(x1, -w, top),
                at(x1, w, top),
                at(x0, w, top),
            ],
            -DVec3::Y,
            rgb(110, 236, 255),
        );
    }
    b.surf = Surf::Plain;
}

/// Paving round a lift's opening in the street, with a chrome rail on the
/// three sides away from the door.
pub(crate) fn collar(b: &mut MeshBuilder, c: DVec2, door: DVec2, town: &TownWorld) {
    let inner = hw_gen::urban::underground::SHAFT_HALF;
    let outer = inner + hw_gen::urban::underground::APRON;
    let side = door.perp();
    let p = |x: f64, z: f64| {
        let q = c + side * x + door * z;
        v3(q, town.ground(q) + 0.07)
    };
    b.tex = Some(GroundLayer::Paving.into());
    let col = rgb(196, 192, 182);
    // Four strips round the hole.
    for (x0, z0, x1, z1) in [
        (-outer, -outer, outer, -inner),
        (-outer, inner, outer, outer),
        (-outer, -inner, -inner, inner),
        (inner, -inner, outer, inner),
    ] {
        b.quad([p(x0, z0), p(x1, z0), p(x1, z1), p(x0, z1)], DVec3::Y, col);
    }
    b.tex = None;
    // A kerb on the hole's edge, and rails on three sides.
    let k = inner + 0.1;
    b.surf = Surf::Metal;
    let rail = rgb(214, 218, 224);
    let corners = [(-k, -k), (k, -k), (k, k), (-k, k)];
    // The door is +z: rail along -z, -x and +x.
    for (a, e) in [(0usize, 1usize), (3, 0), (1, 2)] {
        let (pa, pe) = (p(corners[a].0, corners[a].1), p(corners[e].0, corners[e].1));
        tube(b, pa + DVec3::Y * 1.0, pe + DVec3::Y * 1.0, 0.04, rail);
        tube(b, pa + DVec3::Y * 0.5, pe + DVec3::Y * 0.5, 0.03, rail);
        for q in [pa, pe] {
            tube(b, q, q + DVec3::Y * 1.0, 0.04, rail);
        }
    }
    b.surf = Surf::Plain;
}

/// Granite for treads and landings, darker for the risers, cast stone
/// under them, cream plaster on the pillar.
fn c_tread() -> Color {
    rgb(232, 226, 212)
}
fn c_riser() -> Color {
    rgb(150, 144, 134)
}
fn c_soffit() -> Color {
    rgb(204, 198, 186)
}
fn c_pillar() -> Color {
    rgb(222, 212, 190)
}
fn c_brass() -> Color {
    rgb(206, 164, 92)
}
fn c_chrome() -> Color {
    rgb(218, 222, 230)
}
/// The pillar's lit strip: an Aether line following the stair round.
fn c_strip() -> Color {
    rgb(130, 230, 255)
}

/// A spiral stair down a shaft, from the numbers the collision uses: granite
/// treads with chrome nosing on a smooth cast underside, a chrome
/// balustrade on the open side, a brass handrail on the pillar and a lit
/// Aether strip winding round the pillar above it.
pub(crate) fn spiral_stair(b: &mut MeshBuilder, sp: &Spiral) {
    let steps = sp.steps();
    let r = sp.riser();
    let floor = sp.bottom;
    let w = |l: DVec2, y: f64| v3(sp.world(l), y);
    let wn = |n: DVec2| v3(sp.world(n) - sp.c, 0.0);
    let n4 = [DVec2::X, -DVec2::X, DVec2::Y, -DVec2::Y];
    // The face of `s` towards `o` (unit, local), if they touch.
    let towards = |s: &Step, o: &Step| {
        n4.into_iter().find(|n| {
            let fc = s.at + s.half * *n;
            let d = (fc - o.at).abs() - o.half;
            d.max_element() < 1e-6 && (fc - o.at).dot(*n) < 0.0
        })
    };
    let fronts: Vec<Option<DVec2>> = (0..steps.len())
        .map(|i| steps.get(i + 1).and_then(|o| towards(&steps[i], o)))
        .collect();
    let landing = |i: usize| i.is_multiple_of(4);
    // The underside's height at local `p` of step `i`: level under a
    // landing, sloping half a riser either way under a tread, so the
    // undersides of the treads meet in one smooth helix.
    let soffit = |i: usize, p: DVec2| -> f64 {
        let s = &steps[i];
        let flat = s.top - SLAB;
        let h = if landing(i) {
            flat
        } else {
            let nf = fronts[i].unwrap_or_else(|| {
                // The last tread: on, the way the one above it came.
                -towards(s, &steps[i - 1]).unwrap_or(DVec2::X)
            });
            let t = (p - s.at).dot(nf) / s.half.dot(nf.abs());
            flat - r * 0.5 * t
        };
        h.max(floor)
    };
    let face = |s: &Step, n: DVec2| {
        let t = DVec2::new(n.y.abs(), n.x.abs());
        let fc = s.at + s.half * n;
        (fc - t * s.half.dot(t), fc + t * s.half.dot(t))
    };
    for (i, s) in steps.iter().enumerate() {
        let (lo, hi) = (s.at - s.half, s.at + s.half);
        let c = [
            DVec2::new(lo.x, lo.y),
            DVec2::new(hi.x, lo.y),
            DVec2::new(hi.x, hi.y),
            DVec2::new(lo.x, hi.y),
        ];
        // The tread: pale terrazzo.
        b.tex = Some(GroundLayer::PlazaStone.into());
        b.quad(c.map(|p| w(p, s.top)), DVec3::Y, c_tread());
        // The underside, where it is off the floor.
        b.tex = None;
        if c.iter().any(|p| soffit(i, *p) > floor + 0.005) {
            b.quad(c.map(|p| w(p, soffit(i, p))), -DVec3::Y, c_soffit());
        }
        for n in n4 {
            let (e0, e1) = face(s, n);
            let fc = (e0 + e1) * 0.5;
            let t = DVec2::new(n.y.abs(), n.x.abs());
            if (fc.dot(n) - RING.0).abs() < 1e-6 && fc.dot(t).abs() < RING.0 {
                continue; // against the pillar
            }
            if (fc.dot(n) - RING.1).abs() < 1e-6 {
                // The outer edge: a stringer just inside the shaft's wall.
                let (a, d) = (e0 - n * 0.02, e1 - n * 0.02);
                b.quad(
                    [
                        w(a, soffit(i, e0)),
                        w(d, soffit(i, e1)),
                        w(d, s.top),
                        w(a, s.top),
                    ],
                    wn(n),
                    c_soffit(),
                );
                continue;
            }
            if fronts[i] == Some(n) {
                // The riser down to the next step, and the step in the
                // underside where a landing meets a tread.
                let o = &steps[i + 1];
                b.tex = Some(FacadeLayer::Granite.into());
                b.quad(
                    [w(e0, o.top), w(e1, o.top), w(e1, s.top), w(e0, s.top)],
                    wn(n),
                    c_riser(),
                );
                b.tex = None;
                let (h0, h1) = (soffit(i, fc), soffit(i + 1, fc));
                if (h0 - h1).abs() > 0.005 {
                    let out = if h0 > h1 { -n } else { n };
                    b.quad(
                        [
                            w(e0, h0.min(h1)),
                            w(e1, h0.min(h1)),
                            w(e1, h0.max(h1)),
                            w(e0, h0.max(h1)),
                        ],
                        wn(out),
                        c_soffit(),
                    );
                }
                // Chrome nosing on the edge.
                if i > 0 {
                    let y = s.top - 0.022;
                    let (a, d) = (e0 + t * 0.01 + n * 0.006, e1 - t * 0.01 + n * 0.006);
                    crate::carmesh::tube(
                        b,
                        &[w(a, y), w(d, y)],
                        (0.024, 0.024),
                        6,
                        Some((Surf::Metal, c_chrome())),
                    );
                }
                continue;
            }
            if i > 0 && towards(s, &steps[i - 1]) == Some(n) {
                continue; // drawn by the step above
            }
            // A face nothing meets: the top landing's back, the foot.
            b.tex = Some(FacadeLayer::Granite.into());
            b.quad(
                [
                    w(e0, soffit(i, e0)),
                    w(e1, soffit(i, e1)),
                    w(e1, s.top),
                    w(e0, s.top),
                ],
                wn(n),
                c_riser(),
            );
            b.tex = None;
        }
    }
    pillar(b, sp, &steps);
    balustrade(b, sp, &steps);
}

/// Where the light at a stair's foot hangs: over the lowest steps, off
/// the pillar (which would hide it).
pub(crate) fn foot_lamp(sp: &Spiral) -> DVec3 {
    let steps = sp.steps();
    let p = steps
        .last()
        .map_or(sp.c, |s| sp.world(ring_point(s, RING.0 + 0.5)));
    DVec3::new(p.x, sp.bottom + 2.6, p.y)
}

/// The light of the pillar's strip, one lamp a turn down the stair (the
/// strip itself glows but lights nothing).
pub(crate) fn stair_lamps(sp: &Spiral) -> Vec<Glow> {
    let steps = sp.steps();
    steps
        .iter()
        .enumerate()
        .filter(|(i, s)| i % 16 == 6 && s.top + 2.2 < sp.top - 0.2)
        .map(|(_, s)| {
            let p = sp.world(ring_point(s, RING.0 + 0.35));
            Glow {
                pos: DVec3::new(p.x, s.top + 2.1, p.y),
                color: c_strip(),
                strength: 3.5,
                range: 7.0,
            }
        })
        .chain(steps.last().filter(|_| steps.len() > 8).map(|s| {
            // At the foot, on the far side of the pillar from the lamp
            // over the last steps, so the pillar shades neither side.
            let p = sp.world(-ring_point(s, RING.0 + 0.5));
            Glow {
                pos: DVec3::new(p.x, sp.bottom + 2.4, p.y),
                color: c_strip(),
                strength: 3.5,
                range: 7.0,
            }
        }))
        .collect()
}

/// The plan point of step `s` on the square ring `k` from the centre: a
/// tread's middle pushed out (or in) to that ring, a landing's corner.
fn ring_point(s: &Step, k: f64) -> DVec2 {
    let sg = |v: f64| if v >= 0.0 { 1.0 } else { -1.0 };
    if (s.half.x - s.half.y).abs() < 1e-6 {
        return DVec2::new(sg(s.at.x) * k, sg(s.at.y) * k);
    }
    if s.half.x < s.half.y {
        // Narrow along x: it runs along x, on the z side.
        DVec2::new(s.at.x, sg(s.at.y) * k)
    } else {
        DVec2::new(sg(s.at.x) * k, s.at.y)
    }
}

/// Rounds the corners of a path in plan (radius `rad`), keeping heights
/// running smoothly through.
fn rounded(pts: &[DVec3], rad: f64) -> Vec<DVec3> {
    let mut pts = pts.to_vec();
    pts.dedup_by(|a, b| a.distance(*b) < 1e-3);
    let mut out: Vec<DVec3> = Vec::with_capacity(pts.len() * 2);
    for (k, &p) in pts.iter().enumerate() {
        if k == 0 || k + 1 == pts.len() {
            out.push(p);
            continue;
        }
        let (a, c) = (pts[k - 1], pts[k + 1]);
        let flat = |v: DVec3| DVec2::new(v.x, v.z);
        let (din, dout) = (flat(p - a), flat(c - p));
        if din.length() < 1e-6
            || dout.length() < 1e-6
            || din.normalize().dot(dout.normalize()) > 0.9
        {
            out.push(p);
            continue;
        }
        let ra = rad.min(din.length() * 0.45);
        let rc = rad.min(dout.length() * 0.45);
        let q0 = p + (a - p) * (ra / din.length());
        let q1 = p + (c - p) * (rc / dout.length());
        for j in 0..=5 {
            let t = j as f64 / 5.0;
            out.push(q0 * (1.0 - t) * (1.0 - t) + p * 2.0 * t * (1.0 - t) + q1 * t * t);
        }
    }
    out.dedup_by(|a, b| a.distance(*b) < 1e-3);
    out
}

/// The pillar: cream plaster on a dark granite plinth, a brass handrail on
/// brackets and a glowing strip winding down with the stair.
fn pillar(b: &mut MeshBuilder, sp: &Spiral, steps: &[Step]) {
    let w = |l: DVec2, y: f64| v3(sp.world(l), y);
    let wn = |n: DVec2| v3(sp.world(n) - sp.c, 0.0);
    let sides = [DVec2::X, DVec2::Y, -DVec2::X, -DVec2::Y];
    let k = RING.0;
    let plinth = 0.2;
    for n in sides {
        let t = DVec2::new(-n.y, n.x);
        let (a, d) = (n * k - t * k, n * k + t * k);
        b.tex = Some(FacadeLayer::Plaster.into());
        b.quad(
            [
                w(a, sp.bottom + plinth),
                w(d, sp.bottom + plinth),
                w(d, sp.top),
                w(a, sp.top),
            ],
            wn(n),
            c_pillar(),
        );
        // The plinth stands a little proud, with a bevelled top.
        let (pa, pd) = (a + n * 0.025 - t * 0.025, d + n * 0.025 + t * 0.025);
        b.tex = Some(FacadeLayer::Granite.into());
        b.quad(
            [
                w(pa, sp.bottom),
                w(pd, sp.bottom),
                w(pd, sp.bottom + plinth - 0.025),
                w(pa, sp.bottom + plinth - 0.025),
            ],
            wn(n),
            c_riser(),
        );
        b.quad(
            [
                w(pa, sp.bottom + plinth - 0.025),
                w(pd, sp.bottom + plinth - 0.025),
                w(d, sp.bottom + plinth),
                w(a, sp.bottom + plinth),
            ],
            wn(n) + DVec3::Y,
            c_riser(),
        );
    }
    b.tex = Some(GroundLayer::PlazaStone.into());
    let cap = [(-k, -k), (k, -k), (k, k), (-k, k)].map(|(x, z)| w(DVec2::new(x, z), sp.top));
    b.quad(cap, DVec3::Y, c_tread());
    b.tex = None;
    // On the cap, flush with the street: a chrome rim and an inlaid atom
    // (a lit ring round a chrome bead), stood a few millimetres proud.
    let rim: Vec<DVec3> = [(-k, -k), (k, -k), (k, k), (-k, k), (-k, -k)]
        .map(|(x, z)| w(DVec2::new(x, z) * 0.97, sp.top))
        .to_vec();
    crate::carmesh::tube(
        b,
        &rounded(&rim, 0.05),
        (0.02, 0.006),
        6,
        Some((Surf::Metal, c_chrome())),
    );
    for (sx, sz) in [(1.0, 0.45), (0.45, 1.0), (0.75, 0.75)] {
        let ring: Vec<DVec3> = (0..=40)
            .map(|j| {
                let a = TAU * j as f64 / 40.0;
                let l = DVec2::new(a.cos() * sx, a.sin() * sz) * (k * 0.6);
                w(l, sp.top + 0.004)
            })
            .collect();
        crate::carmesh::tube(b, &ring, (0.018, 0.004), 6, Some((Surf::Glow, c_strip())));
    }
    b.surf = Surf::Metal;
    sphere(b, w(DVec2::ZERO, sp.top - 0.03), 0.07, c_chrome());
    b.surf = Surf::Plain;
    // The handrail on the pillar, 0.9 m over the steps.
    let on: Vec<&Step> = steps
        .iter()
        .skip(1)
        .filter(|s| s.top + 0.9 < sp.top - 0.05)
        .collect();
    let mut rail: Vec<DVec3> = on
        .iter()
        .map(|s| w(ring_point(s, k + 0.09), s.top + 0.9))
        .collect();
    // Each end turns back into the pillar.
    if let (Some(a), Some(z)) = (on.first(), on.last()) {
        rail.insert(0, w(ring_point(a, k + 0.01), a.top + 0.9));
        rail.push(w(ring_point(z, k + 0.01), z.top + 0.9));
    }
    let rail = rounded(&rail, 0.06);
    crate::carmesh::tube(b, &rail, (0.026, 0.026), 10, Some((Surf::Metal, c_brass())));
    // Its brackets: one on the middle tread of every side.
    for (i, s) in steps.iter().enumerate().skip(1) {
        if i % 4 != 2 || s.top + 0.9 >= sp.top - 0.05 {
            continue;
        }
        let p = ring_point(s, k + 0.09);
        let q = ring_point(s, k);
        let y = s.top + 0.9;
        crate::carmesh::tube(
            b,
            &[w(p, y - 0.02), w(p, y - 0.09), w(q, y - 0.09)],
            (0.012, 0.012),
            6,
            Some((Surf::Metal, c_brass())),
        );
        // A round chrome rose where the bracket meets the pillar.
        let n = (q / k).round();
        lathe(
            b,
            w(q, y - 0.09),
            wn(n),
            &[(0.0, 0.0), (0.04, 0.0), (0.035, 0.012), (0.0, 0.016)],
            12,
            |_| Some((Surf::Metal, c_chrome())),
        );
    }
    // The lit strip, 2.2 m over the steps, while it is below the street.
    let strip: Vec<(DVec2, f64)> = steps
        .iter()
        .map(|s| (ring_point(s, k + 0.006), s.top + 2.2))
        .filter(|(_, y)| *y < sp.top - 0.2)
        .collect();
    b.surf = Surf::Glow;
    for pair in strip.windows(2) {
        let ((a, ya), (d, yd)) = (pair[0], pair[1]);
        // Along a face, or round a corner (through the corner point).
        let mid = (a + d) * 0.5;
        let n = if (a.x - d.x).abs() < 1e-6 {
            DVec2::new(a.x.signum(), 0.0)
        } else if (a.y - d.y).abs() < 1e-6 {
            DVec2::new(0.0, a.y.signum())
        } else {
            DVec2::ZERO
        };
        let segs: Vec<(DVec2, f64, DVec2, f64, DVec2)> = if n != DVec2::ZERO {
            vec![(a, ya, d, yd, n)]
        } else {
            // a and d are on two faces: meet at the corner.
            let corner = DVec2::new(
                if a.x.abs() > a.y.abs() { a.x } else { d.x },
                if a.x.abs() > a.y.abs() { d.y } else { a.y },
            );
            let ym = (ya + yd) * 0.5;
            let n1 = (corner - a).perp().normalize();
            let n1 = if n1.dot(mid) < 0.0 { -n1 } else { n1 };
            let n2 = (d - corner).perp().normalize();
            let n2 = if n2.dot(mid) < 0.0 { -n2 } else { n2 };
            vec![(a, ya, corner, ym, n1), (corner, ym, d, yd, n2)]
        };
        for (p0, y0, p1, y1, n) in segs {
            b.quad(
                [
                    w(p0, y0 - 0.025),
                    w(p1, y1 - 0.025),
                    w(p1, y1 + 0.025),
                    w(p0, y0 + 0.025),
                ],
                wn(n),
                c_strip(),
            );
        }
    }
    b.surf = Surf::Plain;
}

/// The chrome balustrade on the open side of the steps (the same line as
/// [`Spiral::rails`]): a top rail, a lower rail and posts.
fn balustrade(b: &mut MeshBuilder, sp: &Spiral, steps: &[Step]) {
    let w = |l: DVec2, y: f64| v3(sp.world(l), y);
    let k = RING.1 - RAIL_T * 0.5 - 0.04;
    let mut run: Vec<(usize, DVec2, f64)> = Vec::new();
    let mut runs = Vec::new();
    for (i, s) in steps.iter().enumerate() {
        if sp.railed(s) {
            run.push((i, ring_point(s, k), s.top));
        } else if !run.is_empty() {
            runs.push(std::mem::take(&mut run));
        }
    }
    if !run.is_empty() {
        runs.push(run);
    }
    b.surf = Surf::Metal;
    for run in runs {
        if run.len() < 2 {
            continue;
        }
        for (h, rad) in [(RAIL_HEIGHT, 0.032), (RAIL_HEIGHT - 0.5, 0.016)] {
            let pts: Vec<DVec3> = run.iter().map(|&(_, p, y)| w(p, y + h)).collect();
            crate::carmesh::tube(
                b,
                &rounded(&pts, 0.14),
                (rad, rad),
                10,
                Some((Surf::Metal, c_chrome())),
            );
        }
        for &(i, p, y) in &run {
            if i.is_multiple_of(2) {
                crate::carmesh::tube(
                    b,
                    &[w(p, y), w(p, y + RAIL_HEIGHT)],
                    (0.018, 0.018),
                    8,
                    Some((Surf::Metal, c_chrome())),
                );
            }
        }
    }
    b.surf = Surf::Plain;
}

/// A Googie pavilion over a stair: four tapered chrome posts, a thin swept
/// canopy with a glowing front edge, and a glowing atomic sign on a mast
/// (amber for Molehaven, Aether blue for the tunnels).
fn pavilion(b: &mut MeshBuilder, c: DVec2, door: DVec2, top: f64, molehaven: bool) {
    let (roof, glow) = if molehaven {
        (rgb(206, 92, 58), rgb(255, 170, 90))
    } else {
        (rgb(46, 150, 156), rgb(130, 230, 255))
    };
    pavilion_in(b, c, door, top, roof, glow);
}

/// [`pavilion`] in any colours: the canopy's paint and the lit edge and sign.
pub(crate) fn pavilion_in(
    b: &mut MeshBuilder,
    c: DVec2,
    door: DVec2,
    top: f64,
    roof: Color,
    glow: Color,
) {
    let h = hw_gen::urban::underground::SHAFT_HALF + 0.35;
    let side = door.perp();
    let chrome = rgb(220, 224, 230);
    let w = h + 0.9;
    let (front, back) = (top + 3.6, top + 3.0);
    // The canopy's underside height at `dz` along the door axis.
    let under = |dz: f64| back + (front - back) * (dz + w) / (2.0 * w + 0.6) - 0.1;
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let p = c + door * h * sz + side * h * sx;
        let len = under(h * sz) - (top - 0.2) + 0.05;
        lathe(
            b,
            v3(p, top - 0.2),
            DVec3::Y,
            &[
                (0.0, 0.0),
                (0.16, 0.0),
                (0.16, 0.05),
                (0.1, 0.16),
                (0.07, 0.3),
                (0.09, len - 0.1),
                (0.12, len),
                (0.0, len + 0.01),
            ],
            16,
            |_| Some((Surf::Metal, chrome)),
        );
    }
    // The canopy: a thin wing, higher at the door and swept back, its
    // front edge sharp and lit, its back and sides rounded.
    let q = |dz: f64, sx: f64, y: f64| v3(c + door * dz + side * sx, y);
    let (z0, z1) = (-w, w + 0.6);
    let n = 12;
    let mut sec: Vec<(f64, f64)> = Vec::new(); // (dz, dy) round the section
    for k in 0..=n {
        let t = k as f64 / n as f64;
        sec.push((z0 + (z1 - z0) * t, 0.18 - 0.12 * t + 0.04 * (PI * t).sin()));
    }
    for k in (0..=n).rev() {
        let t = k as f64 / n as f64;
        sec.push((z0 + (z1 - z0) * t, 0.0 + 0.03 * t - 0.02 * (PI * t).sin()));
    }
    let base = |dz: f64| back + (front - back) * (dz - z0) / (z1 - z0) - 0.18;
    let mut rows: Vec<Vec<DVec3>> = Vec::new();
    let ends = 4;
    let row = |sx: f64, k: f64| -> Vec<DVec3> {
        sec.iter()
            .map(|&(dz, dy)| {
                let mid = (z0 + z1) * 0.5;
                let dz = mid + (dz - mid) * (1.0 - (1.0 - k) * 0.04);
                let y0 = base(dz) + 0.09;
                q(dz, sx, y0 + (base(dz) + dy - y0) * k)
            })
            .collect()
    };
    for e in 0..ends {
        let th = FRAC_PI_2 * e as f64 / ends as f64;
        rows.push(row(-(w + 0.12 * th.cos()), th.sin().max(0.02)));
    }
    rows.push(row(-w, 1.0));
    rows.push(row(w, 1.0));
    for e in (0..ends).rev() {
        let th = FRAC_PI_2 * e as f64 / ends as f64;
        rows.push(row(w + 0.12 * th.cos(), th.sin().max(0.02)));
    }
    crate::carmesh::surface(b, &rows, true, |_, j| {
        if j + 1 == n || j == n || j == n + 1 {
            Some((Surf::Glow, glow))
        } else {
            Some((Surf::Plain, roof))
        }
    });
    // The sign: a mast with an atom (a glowing ball in two rings).
    let mpos = c - door * (w - 0.4) + side * (w - 0.4);
    let mast = 5.4;
    lathe(
        b,
        v3(mpos, top),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.14, 0.0),
            (0.1, 0.12),
            (0.07, 0.4),
            (0.05, mast),
            (0.0, mast + 0.01),
        ],
        12,
        |_| Some((Surf::Metal, chrome)),
    );
    let ball = v3(mpos, top + 5.6);
    b.surf = Surf::Glow;
    sphere(b, ball, 0.32, glow);
    b.surf = Surf::Plain;
    for tilt in [0.6f64, -0.6] {
        let ring: Vec<DVec3> = (0..=32)
            .map(|k| {
                let a = TAU * k as f64 / 32.0;
                let (x, y) = (a.cos() * 0.75, a.sin() * 0.75);
                ball + v3(side * x, 0.0)
                    + DVec3::Y * (y * tilt.cos())
                    + v3(door * y * tilt.sin(), 0.0)
            })
            .collect();
        crate::carmesh::tube(b, &ring, (0.03, 0.03), 8, Some((Surf::Glow, glow)));
    }
}

/// A smooth ball in the builder's current surface.
pub(crate) fn sphere(b: &mut MeshBuilder, c: DVec3, r: f64, col: Color) {
    let rings = 12;
    let prof: Vec<(f64, f64)> = (0..=rings)
        .map(|k| {
            let a = PI * k as f64 / rings as f64;
            (r * a.sin(), -r * a.cos())
        })
        .collect();
    let surf = b.surf;
    lathe(b, c, DVec3::Y, &prof, 24, |_| Some((surf, col)));
}

/// A mushroom: a swelling stem with a flared foot, a domed cap with a
/// curled lip and pale spots, and `glow` in the gills underneath.
#[allow(clippy::too_many_arguments)]
pub(crate) fn mushroom(
    b: &mut MeshBuilder,
    p: DVec2,
    y: f64,
    stem_r: f64,
    h: f64,
    cap_r: f64,
    cap: Color,
    glow: Color,
) {
    let big = cap_r > 2.0;
    let seg = if big { 40 } else { 20 };
    let base = v3(p, y);
    let r = stem_r;
    lathe(
        b,
        base,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (r * 1.35, 0.0),
            (r * 1.2, h * 0.06),
            (r * 1.02, h * 0.2),
            (r * 1.05, h * 0.5),
            (r * 0.92, h * 0.85),
            (r * 0.95, h),
            (0.0, h + 0.01),
        ],
        seg,
        |_| Some((Surf::Plain, rgb(226, 214, 186))),
    );
    // The cap: gills (lit) from the stem out to a curled lip, then the dome.
    let dome = cap_r * 0.55;
    let mut prof = vec![
        (r * 0.9, -0.02),
        (cap_r * 0.9, -0.12),
        (cap_r * 0.98, -0.1),
        (cap_r * 1.02, -0.02),
    ];
    let rings = if big { 10 } else { 6 };
    for k in 0..=rings {
        let a = FRAC_PI_2 * k as f64 / rings as f64;
        prof.push((cap_r * a.cos().max(0.0), dome * a.sin() + 0.04 * a.cos()));
    }
    if let Some(l) = prof.last_mut() {
        l.0 = 0.0;
    }
    let c = v3(p, y + h);
    lathe(b, c, DVec3::Y, &prof, seg, |k| {
        if k == 0 {
            Some((Surf::Glow, glow))
        } else {
            Some((Surf::Plain, cap))
        }
    });
    // A disc closes the gills round the stem.
    let ring: Vec<DVec3> = (0..seg)
        .rev()
        .map(|i| {
            let a = TAU * i as f64 / seg as f64;
            c + DVec3::new(r * 0.9 * a.cos(), -0.02, r * 0.9 * a.sin())
        })
        .collect();
    b.surf = Surf::Glow;
    b.polygon(&ring, -DVec3::Y, glow);
    b.surf = Surf::Plain;
    // Pale spots on the dome.
    if big {
        for k in 0..9 {
            let a = TAU * k as f64 / 9.0 + (k % 2) as f64 * 0.3;
            let e = 0.35 + 0.35 * ((k * 7) % 3) as f64 / 2.0;
            let n = DVec3::new(e.cos() * a.cos(), e.sin(), e.cos() * a.sin());
            let at = c + DVec3::new(cap_r * n.x, dome * n.y + 0.04 * e.cos(), cap_r * n.z);
            let up = DVec3::new(n.x / cap_r, n.y / dome, n.z / cap_r).normalize();
            let sr = cap_r * 0.12;
            lathe(
                b,
                at - up * 0.06,
                up,
                &[
                    (0.0, 0.0),
                    (sr, 0.0),
                    (sr * 0.9, 0.05),
                    (sr * 0.5, 0.09),
                    (0.0, 0.1),
                ],
                24,
                |_| Some((Surf::Plain, rgb(244, 236, 216))),
            );
        }
    }
}

/// Molehaven's dressing: mushroom houses round the great hall and in the
/// side chambers, toadstool lamps, crystals, a well and a market.
fn molehaven(b: &mut MeshBuilder, u: &Underground, glows: &mut Vec<Glow>) {
    let Some(c) = &u.cavern else { return };
    let hall = &u.voids[c.chambers[0]];
    let f = c.floor;
    let caps = [
        rgb(196, 64, 48),
        rgb(214, 132, 48),
        rgb(150, 70, 160),
        rgb(70, 130, 170),
    ];
    // Houses round the hall, doors to its middle.
    for k in 0..10 {
        let a = TAU * k as f64 / 10.0 + 0.3;
        let l = DVec2::new(a.cos() * (hall.max.x - 6.0), a.sin() * (hall.max.y - 6.0));
        let p = hall.world(l);
        let cap = caps[k % caps.len()];
        let r = 2.0 + (k % 3) as f64 * 0.4;
        mushroom(
            b,
            p,
            f,
            r,
            3.6 + (k % 2) as f64,
            r * 2.1,
            cap,
            rgb(255, 200, 130),
        );
        // A round door and a lit window, facing the middle.
        let to = (c.centre - p).normalize_or(DVec2::X);
        let door = p + to * (r + 0.02);
        disc(b, v3(door, f + 1.0), v3(to, 0.0), 0.75, rgb(70, 46, 30));
        b.surf = Surf::Glow;
        disc(
            b,
            v3(p + (to + to.perp() * 0.9).normalize() * (r + 0.02), f + 2.3),
            v3((to + to.perp() * 0.9).normalize(), 0.0),
            0.35,
            rgb(255, 210, 120),
        );
        b.surf = Surf::Plain;
        glows.push(Glow {
            pos: v3(door + to * 1.5, f + 2.2),
            color: rgb(255, 190, 120),
            strength: 3.0,
            range: 7.0,
        });
    }
    // One house in each side chamber.
    for &i in &c.chambers[1..] {
        let v = &u.voids[i];
        let p = v.world((v.min + v.max) * 0.5);
        mushroom(
            b,
            p,
            f,
            2.4,
            4.4,
            5.0,
            caps[i % caps.len()],
            rgb(255, 200, 130),
        );
    }
    // Toadstool lamps where the plan puts lamps.
    for (k, l) in u.lamps.iter().filter(|l| (l.y - f).abs() < 4.0).enumerate() {
        let p = DVec2::new(l.x, l.z);
        let glow = if k % 2 == 0 {
            rgb(120, 220, 255)
        } else {
            rgb(190, 130, 255)
        };
        mushroom(b, p, f, 0.15, 2.2, 0.8, glow, glow);
    }
    // Crystals near the hall's walls.
    b.surf = Surf::Glow;
    for k in 0..14 {
        let a = TAU * k as f64 / 14.0 + 0.11;
        let l = DVec2::new(a.cos() * (hall.max.x - 1.2), a.sin() * (hall.max.y - 1.2));
        let p = hall.world(l);
        let h = 1.4 + (k % 4) as f64 * 0.6;
        let col = if k % 3 == 0 {
            rgb(120, 255, 210)
        } else {
            rgb(170, 140, 255)
        };
        crystal(b, p, f, h, col);
    }
    b.surf = Surf::Plain;
    // The well in the middle: a stone ring with a moulded lip round
    // glowing water, an iron arch over it with a lantern.
    let stone = rgb(120, 110, 96);
    lathe(
        b,
        v3(c.centre, f),
        DVec3::Y,
        &[
            (1.75, 0.75),
            (1.8, 0.0),
            (2.3, 0.0),
            (2.25, 0.12),
            (2.2, 0.7),
            (2.32, 0.76),
            (2.34, 0.9),
            (2.2, 0.98),
            (1.85, 0.98),
            (1.75, 0.9),
        ],
        48,
        |_| Some((Surf::Plain, stone)),
    );
    lathe(
        b,
        v3(c.centre, f + 0.75),
        DVec3::Y,
        &[(0.0, 0.0), (1.8, 0.0), (0.0, 0.001)],
        48,
        |_| Some((Surf::Glow, rgb(90, 220, 255))),
    );
    let arch: Vec<DVec3> = (0..=16)
        .map(|k| {
            let a = PI * k as f64 / 16.0;
            v3(c.centre, f + 0.95) + DVec3::new(-2.05 * a.cos(), 2.4 * a.sin(), 0.0)
        })
        .collect();
    crate::carmesh::tube(
        b,
        &arch,
        (0.07, 0.07),
        10,
        Some((Surf::Metal, rgb(70, 66, 62))),
    );
    let lamp = v3(c.centre, f + 3.35);
    tube(
        b,
        lamp + DVec3::Y * 0.2,
        lamp - DVec3::Y * 0.4,
        0.015,
        rgb(70, 66, 62),
    );
    b.surf = Surf::Glow;
    sphere(b, lamp - DVec3::Y * 0.55, 0.18, rgb(120, 230, 255));
    b.surf = Surf::Plain;
    glows.push(Glow {
        pos: v3(c.centre, f + 2.0),
        color: rgb(90, 220, 255),
        strength: 8.0,
        range: 14.0,
    });
    // Market stalls with striped, scalloped awnings round the well.
    for k in 0..5 {
        let a = TAU * k as f64 / 5.0 + 0.6;
        let d = DVec2::new(a.cos(), a.sin());
        let p = c.centre + d * 9.0;
        stall(b, p, d, f, caps[k % caps.len()]);
    }
}

/// A market stall facing `d`: a wooden counter with a top board, turned
/// posts, and a striped awning that curves down to a scalloped edge.
pub(crate) fn stall(b: &mut MeshBuilder, p: DVec2, d: DVec2, f: f64, stripe: Color) {
    let side = d.perp();
    let wood = rgb(140, 100, 64);
    crate::carmesh::slab(
        b,
        v3(p, f + 0.5),
        [v3(side, 0.0), DVec3::Y],
        DVec3::new(1.4, 0.5, 0.6),
        wood,
    );
    crate::carmesh::slab(
        b,
        v3(p + d * 0.05, f + 1.03),
        [v3(side, 0.0), DVec3::Y],
        DVec3::new(1.5, 0.03, 0.72),
        rgb(170, 126, 84),
    );
    for sx in [-1.3, 1.3] {
        for dz in [-0.55, 0.85] {
            let q = v3(p + side * sx + d * dz, f);
            lathe(
                b,
                q,
                DVec3::Y,
                &[
                    (0.0, 0.0),
                    (0.07, 0.0),
                    (0.05, 0.1),
                    (0.045, 2.35),
                    (0.0, 2.4),
                ],
                10,
                |_| Some((Surf::Plain, wood)),
            );
        }
    }
    // The awning: rows across the stall, each a thin curved section from
    // the back (high) to the scalloped front; stripes every 0.4 m.
    let (w, stripes) = (1.6, 8);
    let n = 8;
    let per = 4;
    let cols = stripes * per;
    let mut rows: Vec<Vec<DVec3>> = Vec::new();
    for i in 0..=cols {
        let t = i as f64 / cols as f64;
        let sx = -w + 2.0 * w * t;
        // The scallops: the front edge dips between stripe centres.
        let ph = (t * stripes as f64).fract();
        let dip = 0.12 * (PI * ph).sin();
        let mut top = Vec::new();
        let mut bot = Vec::new();
        for k in 0..=n {
            let u = k as f64 / n as f64;
            let dz = -0.8 + 2.0 * u;
            let y = f + 2.6 - 0.35 * u - 0.25 * u * u - if k == n { dip } else { 0.0 };
            let at = v3(p + side * sx + d * dz, y);
            top.push(at);
            bot.push(at - DVec3::Y * 0.03);
        }
        bot.reverse();
        top.extend(bot);
        rows.push(top);
    }
    crate::carmesh::surface(b, &rows, true, |i, _| {
        let c = if (i / per) % 2 == 0 {
            stripe
        } else {
            rgb(240, 232, 212)
        };
        Some((Surf::Plain, c))
    });
}

pub(crate) fn disc(b: &mut MeshBuilder, c: DVec3, n: DVec3, r: f64, col: Color) {
    let s = n.cross(DVec3::Y).normalize_or(DVec3::X);
    let pts: Vec<DVec3> = (0..32)
        .map(|k| {
            let a = TAU * k as f64 / 32.0;
            c + s * a.cos() * r + DVec3::Y * a.sin() * r
        })
        .collect();
    b.polygon(&pts, n, col);
    let surf = b.surf;
    let mut ring = pts.clone();
    ring.push(pts[0]);
    crate::carmesh::tube(
        b,
        &ring,
        (r * 0.09, r * 0.09),
        8,
        Some((Surf::Plain, rgb(110, 74, 44))),
    );
    b.surf = surf;
}

/// A crystal: a leaning hexagonal prism with a pointed tip, and two
/// smaller ones at its foot.
pub(crate) fn crystal(b: &mut MeshBuilder, p: DVec2, y: f64, h: f64, col: Color) {
    for (k, (dx, dz, f, lean)) in [
        (0.0, 0.0, 1.0, 0.12),
        (0.35, 0.2, 0.5, 0.35),
        (-0.3, 0.25, 0.4, -0.4),
    ]
    .into_iter()
    .enumerate()
    {
        let r = h * f * 0.16;
        let foot = DVec3::new(p.x + dx * h * 0.4, y - 0.1, p.y + dz * h * 0.4);
        let axis = DVec3::new(lean, 1.0, lean * 0.5 * (k as f64 - 1.0)).normalize();
        let (u, v) = {
            let u = axis.cross(DVec3::X).normalize();
            (u, axis.cross(u))
        };
        let ring = |t: f64, s: f64| -> Vec<DVec3> {
            (0..6)
                .map(|i| {
                    let a = TAU * i as f64 / 6.0;
                    foot + axis * t + (u * a.cos() + v * a.sin()) * r * s
                })
                .collect()
        };
        let len = h * f;
        let (lo, hi) = (ring(0.0, 1.0), ring(len * 0.7, 1.0));
        let tip = foot + axis * len;
        for i in 0..6 {
            let j = (i + 1) % 6;
            let out = (lo[i] + lo[j]) * 0.5 - foot;
            b.quad([lo[i], lo[j], hi[j], hi[i]], out, col);
            b.tri([hi[i], hi[j], tip], out + axis * r, col);
        }
    }
}
