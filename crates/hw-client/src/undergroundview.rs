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
use hw_gen::urban::spiral::{Spiral, NEWEL};
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
                if e.molehaven {
                    crate::molehavenview::gate(&mut b, e, &mut out.glows);
                } else {
                    pavilion(&mut b, e.pos, e.door, e.top);
                }
                collar(&mut b, e.pos, e.door, town);
                out.openings.push(e.pos);
            }
            Layer::Below => {
                out.glows.push(Glow {
                    pos: DVec3::new(e.pos.x, e.bottom + 2.6, e.pos.y),
                    color: col,
                    strength: 6.0,
                    range: 10.0,
                });
                spiral_stair(&mut b, &e.spiral);
            }
        }
    }
    if layer == Layer::Street {
        for sg in u.signs.iter().filter(|sg| chunk_of(sg.pos) == k) {
            crate::molehavenview::signpost(&mut b, sg, &mut out.glows);
        }
    }
    if layer == Layer::Below {
        if let Some(rs) = u
            .cavern
            .as_ref()
            .and_then(|c| c.ramp_sign.as_ref())
            .filter(|rs| chunk_of(rs.at) == k)
        {
            crate::molehavenview::ramp_sign(&mut b, rs, &mut out.glows);
        }
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
            if let Some(l) = hw_gen::urban::underground::molehaven_layout(u) {
                crate::molehavenview::cavern(&mut b, &l, &mut out.glows);
            }
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
    lining_of(
        b,
        v,
        town,
        |mid, f| {
            !u.at(mid).any(|(j, o)| {
                j < i && (o.floor_at(mid) - f).abs() < 0.6 && o.kind != VoidKind::Shaft
            })
        },
        |p| open_elsewhere(u, i, p),
    );
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
    elsewhere: impl Fn(DVec3) -> bool,
) {
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
            // One floor where voids overlap: the first one's.
            if owns_floor(mid, f) {
                b.tex = Some(
                    if cavern && v.height < hw_gen::urban::molehaven::HALL_HEIGHT - 0.5 {
                        // The side chambers (lower than the great hall): beaten earth.
                        GroundLayer::Dirt.into()
                    } else if cavern {
                        GroundLayer::Cobble.into()
                    } else {
                        GroundLayer::Paving.into()
                    },
                );
                let pts = corners.map(|c| v3(c, v.floor_at(c)));
                b.quad(pts, DVec3::Y, floor_col);
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
            b.tex = Some(if v.open {
                FacadeLayer::Limestone.into()
            } else {
                FacadeLayer::Concrete.into()
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
    let outer = inner + 2.4;
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

/// A spiral stair down a shaft: the pillar in the middle and every step, a
/// slab of stone, from the numbers the collision uses.
pub(crate) fn spiral_stair(b: &mut MeshBuilder, sp: &Spiral) {
    let stone = rgb(172, 168, 160);
    let side = rgb(138, 134, 128);
    let under = rgb(104, 102, 98);
    let pillar = rgb(196, 190, 176);
    // The pillar.
    b.tex = Some(FacadeLayer::Concrete.into());
    let corners = [
        DVec2::new(-NEWEL, -NEWEL),
        DVec2::new(NEWEL, -NEWEL),
        DVec2::new(NEWEL, NEWEL),
        DVec2::new(-NEWEL, NEWEL),
    ];
    for k in 0..4 {
        let (a, c) = (corners[k], corners[(k + 1) % 4]);
        let out = sp.world(((a + c) * 0.5).normalize()) - sp.c;
        b.quad(
            [
                v3(sp.world(a), sp.bottom),
                v3(sp.world(c), sp.bottom),
                v3(sp.world(c), sp.top),
                v3(sp.world(a), sp.top),
            ],
            v3(out, 0.0),
            pillar,
        );
    }
    b.tex = None;
    b.quad(corners.map(|c| v3(sp.world(c), sp.top)), DVec3::Y, pillar);
    for st in sp.steps() {
        let (lo, hi) = (st.at - st.half, st.at + st.half);
        let c = [
            DVec2::new(lo.x, lo.y),
            DVec2::new(hi.x, lo.y),
            DVec2::new(hi.x, hi.y),
            DVec2::new(lo.x, hi.y),
        ];
        b.tex = Some(GroundLayer::Paving.into());
        b.quad(c.map(|p| v3(sp.world(p), st.top)), DVec3::Y, stone);
        b.tex = None;
        b.quad(
            c.map(|p| v3(sp.world(p), st.bottom))
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .try_into()
                .expect("four"),
            -DVec3::Y,
            under,
        );
        for k in 0..4 {
            let (a, d) = (c[k], c[(k + 1) % 4]);
            let out = sp.world((a + d) * 0.5 - st.at) - sp.c;
            let out = out - (sp.world(DVec2::ZERO) - sp.c);
            b.quad(
                [
                    v3(sp.world(a), st.bottom),
                    v3(sp.world(d), st.bottom),
                    v3(sp.world(d), st.top),
                    v3(sp.world(a), st.top),
                ],
                v3(out, 0.0),
                side,
            );
        }
    }
}

/// A Googie pavilion over a stair: four tapered chrome posts, a thin swept
/// canopy with a glowing front edge, and a glowing atomic sign on a mast,
/// in Aether blue (Molehaven's stair has its own gate, `molehavenview`).
fn pavilion(b: &mut MeshBuilder, c: DVec2, door: DVec2, top: f64) {
    pavilion_in(b, c, door, top, rgb(46, 150, 156), rgb(130, 230, 255));
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
