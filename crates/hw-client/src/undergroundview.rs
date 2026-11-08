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

use crate::citymesh::{MeshBuilder, Surf};
use engine::prelude::*;
use engine::texgen::{FacadeLayer, GroundLayer};
use glam::{DVec2, DVec3, Vec3};
use hw_gen::urban::underground::{Underground, Void, VoidKind};
use hw_gen::urban::world::TownWorld;
use std::collections::BTreeMap;
use std::f64::consts::TAU;

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

struct Glow {
    pos: DVec3,
    color: Color,
    strength: f32,
    range: f32,
}

pub struct UndergroundView {
    entities: Vec<EntityId>,
    glows: Vec<Glow>,
    lit: Vec<LampId>,
    at: Option<(i32, i32, i32)>,
}

impl UndergroundView {
    pub fn spawn(world: &mut World, town: &TownWorld) -> EngineResult<Self> {
        let u = &town.underground;
        let chunk = |p: DVec2| ((p.x / CHUNK).floor() as i64, (p.y / CHUNK).floor() as i64);
        let mut builders: BTreeMap<(i64, i64), MeshBuilder> = BTreeMap::new();
        let mut glows = Vec::new();
        for (i, v) in u.voids.iter().enumerate() {
            let c = v.world((v.min + v.max) * 0.5);
            let b = builders.entry(chunk(c)).or_default();
            lining(b, u, i, town);
            if v.kind == VoidKind::Tunnel {
                tunnel_fittings(b, u, i);
            }
        }
        for e in &u.entrances {
            let b = builders.entry(chunk(e.pos)).or_default();
            pavilion(b, e.pos, e.door, e.top, e.molehaven);
            collar(b, e.pos, e.door, town);
            let col = if e.molehaven {
                rgb(255, 170, 90)
            } else {
                rgb(130, 230, 255)
            };
            glows.push(Glow {
                pos: DVec3::new(e.pos.x, e.bottom + 2.6, e.pos.y),
                color: col,
                strength: 6.0,
                range: 10.0,
            });
            lift_ring(b, e.pos, e.bottom, col);
            lift_ring(b, e.pos, e.top, col);
        }
        for l in &u.lamps {
            let in_cavern = u
                .cavern
                .as_ref()
                .is_some_and(|c| (l.y - c.floor).abs() < 4.0);
            glows.push(Glow {
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
        if let Some(c) = &u.cavern {
            let b = builders.entry(chunk(c.centre)).or_default();
            molehaven(b, u, &mut glows);
        }
        let mut entities = Vec::new();
        for (k, b) in builders {
            for mesh in b.finish_parts() {
                let id = world.spawn(mesh);
                world.set_name(id, format!("underground {} {}", k.0, k.1));
                entities.push(id);
            }
        }
        Ok(Self {
            entities,
            glows,
            lit: Vec::new(),
            at: None,
        })
    }

    /// Lamps near the camera on while it is below ground or by a lift.
    pub fn update(&mut self, world: &mut World, eye: Vec3, town: &TownWorld) -> EngineResult<()> {
        let q = DVec2::new(eye.x as f64, eye.z as f64);
        let below = (eye.y as f64) < town.ground(q) - 1.0;
        let by_lift = town
            .underground
            .entrances
            .iter()
            .any(|e| e.pos.distance(q) < 30.0);
        if !below && !by_lift {
            self.clear_lamps(world);
            return Ok(());
        }
        let key = (
            (eye.x / 6.0).round() as i32,
            (eye.y / 6.0).round() as i32,
            (eye.z / 6.0).round() as i32,
        );
        if self.at == Some(key) {
            return Ok(());
        }
        self.at = Some(key);
        let e = eye.as_dvec3();
        let mut near: Vec<(f64, &Glow)> = self
            .glows
            .iter()
            .map(|g| (g.pos.distance(e), g))
            .filter(|(d, _)| *d < 90.0)
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

    pub fn clear(&mut self, world: &mut World) {
        self.clear_lamps(world);
        for e in self.entities.drain(..) {
            world.despawn(e);
        }
    }
}

fn v3(p: DVec2, y: f64) -> DVec3 {
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
            let owned = !u.at(mid).any(|(j, o)| {
                j < i && (o.floor_at(mid) - f).abs() < 0.6 && o.kind != VoidKind::Shaft
            });
            if owned {
                b.tex = Some(if cavern {
                    GroundLayer::Cobble.into()
                } else {
                    GroundLayer::Paving.into()
                });
                let pts = corners.map(|c| v3(c, v.floor_at(c)));
                b.quad(pts, DVec3::Y, floor_col);
                b.tex = None;
            }
            if cavern || v.open {
                continue;
            }
            if let Some(c) = v.ceiling_at(mid) {
                if !open_elsewhere(u, i, v3(mid, c + 0.4)) {
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
            if open_elsewhere(u, i, v3(m + out * 0.4, f + 1.2)) {
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

/// A round pipe from `a` to `c`.
fn tube(b: &mut MeshBuilder, a: DVec3, c: DVec3, r: f64, col: Color) {
    let d = (c - a).normalize_or(DVec3::X);
    let s = d.cross(DVec3::Y).normalize_or(DVec3::Z);
    let t = s.cross(d);
    let seg = 8;
    for k in 0..seg {
        let (a0, a1) = (
            TAU * k as f64 / seg as f64,
            TAU * (k + 1) as f64 / seg as f64,
        );
        let r0 = s * a0.cos() * r + t * a0.sin() * r;
        let r1 = s * a1.cos() * r + t * a1.sin() * r;
        b.quad([a + r0, c + r0, c + r1, a + r1], (r0 + r1) * 0.5, col);
    }
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
fn collar(b: &mut MeshBuilder, c: DVec2, door: DVec2, town: &TownWorld) {
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

/// A glowing ring on a lift's floor (and at the top of the shaft).
fn lift_ring(b: &mut MeshBuilder, c: DVec2, y: f64, col: Color) {
    b.surf = Surf::Glow;
    let seg = 24;
    let (r0, r1) = (1.0, 1.25);
    for k in 0..seg {
        let (a0, a1) = (
            TAU * k as f64 / seg as f64,
            TAU * (k + 1) as f64 / seg as f64,
        );
        let p = |r: f64, a: f64| DVec3::new(c.x + r * a.cos(), y + 0.03, c.y + r * a.sin());
        b.quad([p(r0, a0), p(r1, a0), p(r1, a1), p(r0, a1)], DVec3::Y, col);
    }
    b.surf = Surf::Plain;
}

/// A Googie pavilion over a lift: four chrome posts, a swept canopy and a
/// glowing atomic sign on a mast (amber for Molehaven, Aether blue for the
/// tunnels).
fn pavilion(b: &mut MeshBuilder, c: DVec2, door: DVec2, top: f64, molehaven: bool) {
    let h = hw_gen::urban::underground::SHAFT_HALF + 0.35;
    let side = door.perp();
    let roof = if molehaven {
        rgb(206, 92, 58)
    } else {
        rgb(46, 150, 156)
    };
    let glow = if molehaven {
        rgb(255, 170, 90)
    } else {
        rgb(130, 230, 255)
    };
    b.surf = Surf::Metal;
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let p = c + door * h * sz + side * h * sx;
        b.cylinder(
            v3(p, top - 0.2),
            (0.09, 0.09),
            3.4,
            8,
            false,
            rgb(220, 224, 230),
        );
    }
    b.surf = Surf::Plain;
    // The canopy: higher at the door, swept back.
    let w = h + 0.9;
    let (front, back) = (top + 3.6, top + 3.0);
    let q = |dz: f64, sx: f64, y: f64| v3(c + door * dz + side * sx * w, y);
    let bottom = [
        q(w + 0.6, -1.0, front - 0.18),
        q(w + 0.6, 1.0, front - 0.18),
        q(-w, 1.0, back - 0.18),
        q(-w, -1.0, back - 0.18),
    ];
    let topq = [
        q(w + 0.6, -1.0, front),
        q(w + 0.6, 1.0, front),
        q(-w, 1.0, back),
        q(-w, -1.0, back),
    ];
    b.hexa(bottom, topq, roof);
    // A glowing edge along the canopy's front.
    b.surf = Surf::Glow;
    b.quad(
        [
            q(w + 0.61, -1.0, front - 0.17),
            q(w + 0.61, 1.0, front - 0.17),
            q(w + 0.61, 1.0, front - 0.03),
            q(w + 0.61, -1.0, front - 0.03),
        ],
        v3(door, 0.0),
        glow,
    );
    b.surf = Surf::Plain;
    // The sign: a mast with an atom (a glowing ball in two rings).
    let m = c - door * (w - 0.4) + side * (w - 0.4);
    b.surf = Surf::Metal;
    b.cylinder(v3(m, top), (0.07, 0.07), 5.4, 8, false, rgb(220, 224, 230));
    b.surf = Surf::Glow;
    let ball = v3(m, top + 5.6);
    sphere(b, ball, 0.32, glow);
    for tilt in [0.6f64, -0.6] {
        let seg = 20;
        for k in 0..seg {
            let a0 = TAU * k as f64 / seg as f64;
            let a1 = TAU * (k + 1) as f64 / seg as f64;
            let ring = |a: f64| {
                let x = a.cos() * 0.75;
                let y = a.sin() * 0.75;
                ball + v3(side * x, 0.0)
                    + DVec3::new(0.0, y * tilt.cos(), 0.0)
                    + v3(door * y * tilt.sin(), 0.0)
            };
            tube(b, ring(a0), ring(a1), 0.03, glow);
        }
    }
    b.surf = Surf::Plain;
}

fn sphere(b: &mut MeshBuilder, c: DVec3, r: f64, col: Color) {
    let (seg, rings) = (12, 6);
    let at = |i: usize, k: usize| {
        let a = TAU * (i % seg) as f64 / seg as f64;
        let e = std::f64::consts::PI * (k as f64 / rings as f64 - 0.5);
        c + DVec3::new(r * e.cos() * a.cos(), r * e.sin(), r * e.cos() * a.sin())
    };
    for k in 0..rings {
        for i in 0..seg {
            let p = [at(i, k), at(i + 1, k), at(i + 1, k + 1), at(i, k + 1)];
            let out = (p[0] + p[1] + p[2] + p[3]) * 0.25 - c;
            b.quad(p, out, col);
        }
    }
}

/// A mushroom: stem and a dome cap, `glow` under the cap.
#[allow(clippy::too_many_arguments)]
fn mushroom(
    b: &mut MeshBuilder,
    p: DVec2,
    y: f64,
    stem_r: f64,
    h: f64,
    cap_r: f64,
    cap: Color,
    glow: Color,
) {
    b.cylinder(
        v3(p, y),
        (stem_r, stem_r * 0.85),
        h,
        12,
        false,
        rgb(226, 214, 186),
    );
    // The cap: a squashed dome.
    let (seg, rings) = (16, 5);
    let c = v3(p, y + h);
    let at = |i: usize, k: usize| {
        let a = TAU * (i % seg) as f64 / seg as f64;
        let e = std::f64::consts::FRAC_PI_2 * k as f64 / rings as f64;
        c + DVec3::new(
            cap_r * e.cos() * a.cos(),
            cap_r * 0.55 * e.sin(),
            cap_r * e.cos() * a.sin(),
        )
    };
    for k in 0..rings {
        for i in 0..seg {
            let q = [at(i, k), at(i + 1, k), at(i + 1, k + 1), at(i, k + 1)];
            let out = (q[0] + q[1] + q[2] + q[3]) * 0.25 - c + DVec3::Y * 0.2;
            b.quad(q, out, cap);
        }
    }
    // Glowing gills underneath.
    b.surf = Surf::Glow;
    let rim: Vec<DVec3> = (0..seg).rev().map(|i| at(i, 0) - DVec3::Y * 0.02).collect();
    b.polygon(&rim, -DVec3::Y, glow);
    b.surf = Surf::Plain;
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
    // The well in the middle, glowing water.
    b.cylinder(
        v3(c.centre, f),
        (2.2, 2.2),
        0.9,
        20,
        true,
        rgb(120, 110, 96),
    );
    b.surf = Surf::Glow;
    b.cylinder(
        v3(c.centre, f + 0.9),
        (1.8, 1.8),
        0.02,
        20,
        true,
        rgb(90, 220, 255),
    );
    b.surf = Surf::Plain;
    glows.push(Glow {
        pos: v3(c.centre, f + 2.0),
        color: rgb(90, 220, 255),
        strength: 8.0,
        range: 14.0,
    });
    // Market stalls with striped awnings round the well.
    for k in 0..5 {
        let a = TAU * k as f64 / 5.0 + 0.6;
        let d = DVec2::new(a.cos(), a.sin());
        let p = c.centre + d * 9.0;
        b.oriented_box(
            v3(p, f + 0.5),
            v3(d.perp(), 0.0),
            DVec3::new(1.4, 0.5, 0.7),
            rgb(140, 100, 64),
        );
        for s in 0..4 {
            let col = if s % 2 == 0 {
                caps[k % caps.len()]
            } else {
                rgb(240, 232, 212)
            };
            let o = d.perp() * (-1.2 + s as f64 * 0.8);
            b.oriented_box(
                v3(p + o, f + 2.3),
                v3(d.perp(), 0.0),
                DVec3::new(0.4, 0.05, 1.0),
                col,
            );
        }
        b.surf = Surf::Metal;
        for sx in [-1.3, 1.3] {
            b.cylinder(
                v3(p + d.perp() * sx, f),
                (0.05, 0.05),
                2.3,
                6,
                false,
                rgb(160, 150, 130),
            );
        }
        b.surf = Surf::Plain;
    }
}

fn disc(b: &mut MeshBuilder, c: DVec3, n: DVec3, r: f64, col: Color) {
    let s = n.cross(DVec3::Y).normalize_or(DVec3::X);
    let pts: Vec<DVec3> = (0..16)
        .map(|k| {
            let a = TAU * k as f64 / 16.0;
            c + s * a.cos() * r + DVec3::Y * a.sin() * r
        })
        .collect();
    b.polygon(&pts, n, col);
}

fn crystal(b: &mut MeshBuilder, p: DVec2, y: f64, h: f64, col: Color) {
    let r = h * 0.18;
    let base: Vec<DVec3> = (0..5)
        .map(|k| {
            let a = TAU * k as f64 / 5.0;
            DVec3::new(p.x + r * a.cos(), y, p.y + r * a.sin())
        })
        .collect();
    let tip = DVec3::new(p.x + r * 0.3, y + h, p.y);
    for k in 0..5 {
        let (a, c) = (base[k], base[(k + 1) % 5]);
        let out = (a + c) * 0.5 - DVec3::new(p.x, y, p.y);
        b.tri([a, c, tip], out, col);
    }
}
