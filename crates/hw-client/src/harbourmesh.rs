//! The harbour's fixed parts, from the same plan they collide by
//! ([`hw_gen::urban::harbour::Harbour`]): the mole's paved deck and granite
//! quay walls with their fenders, crane rails and edge lines, the pontoons
//! and their gangways, the breakwater (a rubble mound under a concrete
//! crown and parapet) with its light, the harbour master's tower and the
//! liner's gangway towers; and, each in three levels to be placed many
//! times, the portal cranes, the mooring bollards and the crate stacks.
//!
//! The fixed parts are built in the town's frame, moved so that `origin`
//! (a point in the harbour) is at zero: the entity stands there.

use crate::carmesh::{lathe, tube, Lod};
use crate::citymesh::{MeshBuilder, Surf};
use engine::prelude::*;
use engine::texgen::FacadeLayer;
use glam::{DVec2, DVec3};
use hw_gen::urban::harbour::*;
use hw_gen::urban::world::{TownWorld, WATER_LEVEL};
use std::f64::consts::{PI, TAU};

fn concrete() -> Color {
    rgb(176, 172, 162)
}
fn granite() -> Color {
    rgb(150, 146, 140)
}
fn coping() -> Color {
    rgb(200, 194, 182)
}
fn cream() -> Color {
    rgb(244, 236, 214)
}
fn orange() -> Color {
    rgb(222, 104, 46)
}
fn teal() -> Color {
    rgb(46, 150, 156)
}
fn chrome() -> Color {
    rgb(212, 216, 222)
}
fn glass() -> Color {
    rgb(44, 64, 82)
}

/// A small, steady hash of a position (0..1): rubble that looks rough,
/// the same on every machine.
fn hash(p: DVec2, k: u32) -> f64 {
    let x = (p.x * 7.13).floor() as i64;
    let z = (p.y * 7.13).floor() as i64;
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (z as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ (k as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 29;
    (h % 10_000) as f64 / 10_000.0
}

/// Builds parts in the town's frame, moved by `origin`.
struct Here<'a> {
    b: &'a mut MeshBuilder,
    o: DVec3,
}

impl Here<'_> {
    fn p(&self, q: DVec2, y: f64) -> DVec3 {
        DVec3::new(q.x, y, q.y) - self.o
    }

    /// A grid laid with its normals pointing up.
    fn up_grid(&mut self, rows: &[Vec<DVec3>], col: Color) {
        if rows.len() < 2 || rows[0].len() < 2 {
            return;
        }
        let n = (rows[1][0] - rows[0][0]).cross(rows[0][1] - rows[0][0]);
        self.b
            .grid(rows, false, n.y >= 0.0, |_, _| Some((Surf::Plain, col)));
    }

    /// A wall from `a` to `b` (plan), from `y0` to `y1` at each end,
    /// facing `out`.
    #[allow(clippy::too_many_arguments)]
    fn wall(
        &mut self,
        a: DVec2,
        e: DVec2,
        (ya0, ya1): (f64, f64),
        (yb0, yb1): (f64, f64),
        out: DVec2,
        col: Color,
    ) {
        let pts = [
            self.p(a, ya0),
            self.p(e, yb0),
            self.p(e, yb1),
            self.p(a, ya1),
        ];
        self.b.quad(pts, DVec3::new(out.x, 0.0, out.y), col);
    }
}

/// The sea bed under `p`, never above the water.
fn bed(w: &TownWorld, p: DVec2) -> f64 {
    w.ground(p).min(WATER_LEVEL - 0.6) - 0.4
}

/// The mole: its deck, its quay walls down to the sea bed, the coping,
/// fenders, edge lines and crane rails, the wall at its land end where the
/// shore lies lower, and the pontoons.
fn mole(here: &mut Here, h: &Harbour, w: &TownWorld, lod: Lod) {
    let deck = h.deck;
    let l2p = |x: f64, z: f64| h.to_plan(DVec2::new(x, z));
    // The deck: rows from the root line out to the head.
    let xs: Vec<f64> = (0..ROOT_SAMPLES)
        .map(|i| -MOLE_HALF + 2.0 * MOLE_HALF * i as f64 / (ROOT_SAMPLES - 1) as f64)
        .collect();
    let mut zs = vec![f64::NAN];
    let mut z = 14.0;
    while z < MOLE_LEN - CHAMFER {
        zs.push(z);
        z += lod.pick(12.0, 24.0, 60.0);
    }
    zs.push(MOLE_LEN - CHAMFER);
    let rows: Vec<Vec<DVec3>> = zs
        .iter()
        .map(|&z| {
            xs.iter()
                .enumerate()
                .map(|(i, &x)| {
                    let zz = if z.is_nan() { h.root_line[i] } else { z };
                    here.p(l2p(x, zz), deck)
                })
                .collect()
        })
        .collect();
    here.b.tex = Some(FacadeLayer::Concrete.into());
    here.up_grid(&rows, concrete());
    // The head, its corners cut.
    let head: Vec<DVec3> = [
        (-MOLE_HALF, MOLE_LEN - CHAMFER),
        (MOLE_HALF, MOLE_LEN - CHAMFER),
        (MOLE_HALF - CHAMFER, MOLE_LEN),
        (-MOLE_HALF + CHAMFER, MOLE_LEN),
    ]
    .iter()
    .map(|&(x, z)| here.p(l2p(x, z), deck))
    .collect();
    here.b.polygon(&head, DVec3::Y, concrete());
    // The quay walls: down both sides and round the head.
    here.b.tex = Some(FacadeLayer::Granite.into());
    let outline = [
        (MOLE_HALF, h.root_line[ROOT_SAMPLES - 1]),
        (MOLE_HALF, MOLE_LEN - CHAMFER),
        (MOLE_HALF - CHAMFER, MOLE_LEN),
        (-MOLE_HALF + CHAMFER, MOLE_LEN),
        (-MOLE_HALF, MOLE_LEN - CHAMFER),
        (-MOLE_HALF, h.root_line[0]),
    ];
    let centre = l2p(0.0, MOLE_LEN * 0.5);
    let step = lod.pick(6.0, 12.0, 40.0);
    for wnd in outline.windows(2) {
        let (a, e) = (l2p(wnd[0].0, wnd[0].1), l2p(wnd[1].0, wnd[1].1));
        let len = a.distance(e);
        let n = (len / step).ceil().max(1.0) as usize;
        let mid = (a + e) * 0.5;
        let mut out = (e - a).perp().normalize();
        if out.dot(mid - centre) < 0.0 {
            out = -out;
        }
        for k in 0..n {
            let (p0, p1) = (
                a.lerp(e, k as f64 / n as f64),
                a.lerp(e, (k + 1) as f64 / n as f64),
            );
            here.wall(
                p0,
                p1,
                (bed(w, p0 + out * 1.5), deck - 0.25),
                (bed(w, p1 + out * 1.5), deck - 0.25),
                out,
                granite(),
            );
            if lod != Lod::Far {
                // The coping: a pale kerb along the edge.
                here.b.tex = None;
                let (i0, i1) = (p0 - out * 0.5, p1 - out * 0.5);
                let (o0, o1) = (p0 + out * 0.08, p1 + out * 0.08);
                let t = deck + 0.1;
                here.b.quad(
                    [here.p(i0, t), here.p(i1, t), here.p(o1, t), here.p(o0, t)],
                    DVec3::Y,
                    coping(),
                );
                here.wall(o0, o1, (deck - 0.25, t), (deck - 0.25, t), out, coping());
                here.wall(i0, i1, (deck, t), (deck, t), -out, coping());
                here.b.tex = Some(FacadeLayer::Granite.into());
            }
        }
    }
    // The land end, where the shore lies lower than the deck.
    for i in 0..ROOT_SAMPLES - 1 {
        let (a, e) = (
            l2p(xs[i], h.root_line[i]),
            l2p(xs[i + 1], h.root_line[i + 1]),
        );
        let (ga, ge) = (w.ground(a), w.ground(e));
        if ga < deck - 0.05 || ge < deck - 0.05 {
            let mut out = (e - a).perp().normalize();
            if out.dot(-h.dir) < 0.0 {
                out = -out;
            }
            here.wall(
                a,
                e,
                (ga.min(deck) - 0.5, deck),
                (ge.min(deck) - 0.5, deck),
                out,
                granite(),
            );
        }
    }
    here.b.tex = None;
    if lod == Lod::Far {
        return;
    }
    // Fenders: black timbers down the long quays.
    for side in [-1.0, 1.0] {
        let mut z = 10.0;
        while z < MOLE_LEN - CHAMFER - 4.0 {
            let c = l2p(side * (MOLE_HALF + 0.25), z);
            let out = h.right * side;
            let y1 = deck - 0.3;
            let y0 = WATER_LEVEL - 0.8;
            crate::carmesh::slab(
                here.b,
                here.p(c, (y0 + y1) * 0.5),
                [DVec3::new(out.x, 0.0, out.y), DVec3::Y],
                DVec3::new(0.22, (y1 - y0) * 0.5, 0.3),
                rgb(36, 32, 30),
            );
            z += 12.0;
        }
    }
    // Yellow edge lines and the crane rails on the cargo side.
    let line =
        |here: &mut Here, x: f64, z0: f64, z1: f64, half: f64, up: f64, col: Color, surf: Surf| {
            let (a0, a1) = (l2p(x - half, z0), l2p(x + half, z0));
            let (b0, b1) = (l2p(x - half, z1), l2p(x + half, z1));
            let y = deck + up;
            here.b.surf = surf;
            here.b.quad(
                [here.p(a0, y), here.p(a1, y), here.p(b1, y), here.p(b0, y)],
                DVec3::Y,
                col,
            );
            here.b.surf = Surf::Plain;
        };
    let yellow = rgb(230, 190, 40);
    for side in [-1.0, 1.0] {
        line(
            here,
            side * (MOLE_HALF - 2.0),
            6.0,
            MOLE_LEN - CHAMFER - 2.0,
            0.08,
            0.012,
            yellow,
            Surf::Plain,
        );
    }
    let steel = rgb(120, 118, 114);
    for x in [CRANE_LEGS.0, CRANE_LEGS.1] {
        for dx in [-0.75, 0.75] {
            line(here, x + dx, 18.0, 178.0, 0.05, 0.03, steel, Surf::Metal);
        }
    }
    // A railway line down the passenger side, to the gangways.
    for dx in [-0.72, 0.72] {
        line(here, -22.0 + dx, 2.0, 170.0, 0.04, 0.03, steel, Surf::Metal);
    }
    // The pontoons, floating, and their gangways down from the quay.
    for (k, (x0, z0, z1)) in PONTOONS.iter().copied().enumerate() {
        let y = WATER_LEVEL + PONTOON_UP;
        let pts: Vec<DVec2> = [
            (x0, z0),
            (x0 + PONTOON_W, z0),
            (x0 + PONTOON_W, z1),
            (x0, z1),
        ]
        .iter()
        .map(|&(x, z)| l2p(x, z))
        .collect();
        let wood = rgb(140, 104, 70);
        let top: Vec<DVec3> = pts.iter().map(|p| here.p(*p, y)).collect();
        here.b.polygon(&top, DVec3::Y, wood);
        let c = pts.iter().sum::<DVec2>() * 0.25;
        for i in 0..4 {
            let (a, e) = (pts[i], pts[(i + 1) % 4]);
            let m = (a + e) * 0.5 - c;
            here.wall(
                a,
                e,
                (WATER_LEVEL - 0.5, y),
                (WATER_LEVEL - 0.5, y),
                m.normalize(),
                rgb(60, 60, 64),
            );
        }
        // The gangway: a sloping deck with rails from the quay's edge.
        let gx = if k == 0 { MOLE_HALF } else { -MOLE_HALF };
        let gz = z1 - 3.0;
        let (q0, q1) = (l2p(gx, gz - 0.7), l2p(gx, gz + 0.7));
        let inward = if k == 0 { 1.0 } else { -1.0 };
        let (p0, p1) = (
            l2p(gx + inward * (PONTOON_W - 0.2), gz - 0.7),
            l2p(gx + inward * (PONTOON_W - 0.2), gz + 0.7),
        );
        let alu = rgb(190, 192, 196);
        here.b.surf = Surf::Metal;
        here.b.quad(
            [
                here.p(q0, deck),
                here.p(q1, deck),
                here.p(p1, y + 0.05),
                here.p(p0, y + 0.05),
            ],
            DVec3::Y,
            alu,
        );
        here.b.surf = Surf::Plain;
        for (qa, pa) in [(q0, p0), (q1, p1)] {
            tube(
                here.b,
                &[here.p(qa, deck + 1.0), here.p(pa, y + 1.0)],
                (0.03, 0.03),
                5,
                Some((Surf::Metal, alu)),
            );
        }
    }
}

/// The breakwater: a rubble mound sloping from its concrete crown down to
/// the sea bed, a parapet wall on its sea side, a rounded head.
fn breakwater(here: &mut Here, h: &Harbour, w: &TownWorld, lod: Lod) {
    let pts = h.breakwater();
    let crown = h.crown();
    // Points every few metres along the crown line, with the way across.
    let step = lod.pick(3.0, 6.0, 18.0);
    let mut line: Vec<(DVec2, DVec2)> = Vec::new();
    for wnd in pts.windows(2) {
        let (a, e) = (wnd[0], wnd[1]);
        let d = (e - a).normalize();
        let n = (a.distance(e) / step).ceil() as usize;
        for k in 0..n {
            line.push((a.lerp(e, k as f64 / n as f64), d));
        }
    }
    let last = *pts.last().expect("breakwater");
    let d_end = (last - pts[pts.len() - 2]).normalize();
    line.push((last, d_end));
    // The head: half a circle round its end.
    let segs = lod.pick(10, 6, 3);
    let mut head_dirs = Vec::new();
    for k in 1..=segs {
        let a = PI * k as f64 / segs as f64;
        let (s, c) = a.sin_cos();
        // Turning from across (one side) round the front to the other.
        let across = d_end.perp();
        head_dirs.push(across * c + d_end * s);
    }
    // Sea side: away from the mole.
    let sea = |n: DVec2| n.dot(h.right) > 0.0;
    let rubble = |q: DVec2, k: u32, y: f64| {
        let g = 126.0 + 30.0 * hash(q, k);
        let wet = if y < WATER_LEVEL + 0.4 { 0.62 } else { 1.0 };
        rgb(
            (g * wet) as u8,
            ((g - 4.0) * wet) as u8,
            ((g - 10.0) * wet) as u8,
        )
    };
    // Both flanks as grids: rows along the line, columns down the slope.
    for side in [-1.0, 1.0] {
        let down = lod.pick(5, 3, 2);
        let mut rows: Vec<Vec<DVec3>> = Vec::new();
        let mut cols_c: Vec<Vec<Color>> = Vec::new();
        let mut put = |q: DVec2, n: DVec2| {
            let floor = bed(w, q + n * h.toe(WATER_LEVEL - 6.0));
            let mut row = Vec::new();
            let mut cs = Vec::new();
            for j in 0..=down {
                let t = j as f64 / down as f64;
                let y = crown - 0.1 + (floor - crown + 0.1) * t;
                let reach = CROWN_HALF + (crown - y) * RUBBLE_SLOPE;
                let jit = if j > 0 && j < down {
                    (hash(q, j as u32) - 0.5) * 0.9
                } else {
                    0.0
                };
                let p = q + n * (reach + jit);
                row.push(here.p(p, y + jit * 0.3));
                cs.push(rubble(p, j as u32 + 7, y));
            }
            rows.push(row);
            cols_c.push(cs);
        };
        for &(q, d) in &line {
            put(q, d.perp() * side);
        }
        if side > 0.0 {
            for n in &head_dirs {
                put(last, *n);
            }
        }
        // Normals out of the mound: away from the crown line, and up.
        let along = rows[1][0] - rows[0][0];
        let down_slope = rows[0][1] - rows[0][0];
        let away = DVec3::new(down_slope.x, 0.0, down_slope.z).normalize_or_zero() + DVec3::Y;
        let out = along.cross(down_slope).dot(away) >= 0.0;
        here.b
            .grid(&rows, false, out, |i, j| Some((Surf::Plain, cols_c[i][j])));
    }
    // The crown: a concrete strip, and its rounded head.
    here.b.tex = Some(FacadeLayer::Concrete.into());
    let crown_rows: Vec<Vec<DVec3>> = line
        .iter()
        .map(|&(q, d)| {
            let n = d.perp();
            vec![
                here.p(q - n * CROWN_HALF, crown),
                here.p(q + n * CROWN_HALF, crown),
            ]
        })
        .collect();
    here.up_grid(&crown_rows, concrete());
    let mut fan = vec![here.p(last, crown)];
    fan.push(here.p(last + d_end.perp() * CROWN_HALF, crown));
    for n in &head_dirs {
        fan.push(here.p(last + *n * CROWN_HALF, crown));
    }
    for k in 1..fan.len() - 1 {
        here.b
            .tri([fan[0], fan[k], fan[k + 1]], DVec3::Y, concrete());
    }
    // The parapet on the sea side.
    for wnd in line.windows(2) {
        let ((a, d), (e, _)) = (wnd[0], wnd[1]);
        let mut n = d.perp();
        if !sea(n) {
            n = -n;
        }
        let (ao, eo) = (a + n * CROWN_HALF, e + n * CROWN_HALF);
        let (ai, ei) = (a + n * (CROWN_HALF - 1.0), e + n * (CROWN_HALF - 1.0));
        let top = crown + PARAPET;
        here.wall(ai, ei, (crown, top), (crown, top), -n, concrete());
        here.wall(
            ao,
            eo,
            (crown - 0.2, top),
            (crown - 0.2, top),
            n,
            concrete(),
        );
        here.b.quad(
            [
                here.p(ai, top),
                here.p(ei, top),
                here.p(eo, top),
                here.p(ao, top),
            ],
            DVec3::Y,
            coping(),
        );
    }
    here.b.tex = None;
    // Boulders armouring both flanks, from under the crown to the water.
    if lod != Lod::Far {
        for (k, &(q, d)) in line.iter().enumerate().step_by(lod.pick(1, 3, 9)) {
            for side in [-1.0, 1.0] {
                let n = d.perp() * side;
                let big = if sea(n) { 1.0 } else { 0.7 };
                for (j, drop) in [0.5f64, 1.3, 2.1].into_iter().enumerate() {
                    let y = crown - drop.min(crown - WATER_LEVEL + 0.2);
                    let reach = CROWN_HALF + (crown - y) * RUBBLE_SLOPE;
                    let key = k as u32 * 7 + j as u32 * 2 + (side > 0.0) as u32;
                    let c = q + n * (reach + 0.2) + d * (hash(q, 40 + key) - 0.5) * 2.4;
                    let r = (0.55 + 0.6 * hash(c, 3)) * big;
                    rock(here, c, y - r * 0.25, r, key);
                }
            }
        }
    }
}

/// A rough boulder: an octahedron pushed about.
fn rock(here: &mut Here, c: DVec2, y: f64, r: f64, k: u32) {
    let ctr = here.p(c, y);
    let mut v: Vec<DVec3> = [
        DVec3::X,
        -DVec3::X,
        DVec3::Y * 0.7,
        -DVec3::Y * 0.6,
        DVec3::Z,
        -DVec3::Z,
    ]
    .iter()
    .enumerate()
    .map(|(i, d)| *d * r * (0.75 + 0.5 * hash(c, k * 7 + i as u32)))
    .collect();
    for p in v.iter_mut() {
        *p += ctr;
    }
    let g = 112.0 + 40.0 * hash(c, k + 99);
    let col = rgb(g as u8, (g - 4.0) as u8, (g - 10.0) as u8);
    let faces = [
        (0, 2, 4),
        (4, 2, 1),
        (1, 2, 5),
        (5, 2, 0),
        (0, 4, 3),
        (4, 1, 3),
        (1, 5, 3),
        (5, 0, 3),
    ];
    for (a, b, e) in faces {
        let m = (v[a] + v[b] + v[e]) / 3.0;
        here.b.tri([v[a], v[b], v[e]], m - ctr, col);
    }
}

/// The light on the breakwater's head: a slim red and white tower with a
/// gallery and a glowing lantern under a dome.
fn harbour_light(here: &mut Here, h: &Harbour, lod: Lod) {
    let head = *h.breakwater().last().expect("breakwater");
    let base = here.p(head, h.crown());
    let seg = lod.pick(24, 12, 8);
    let r = LIGHT_R;
    let prof = [
        (0.0, 0.0),
        (r * 1.15, 0.0),
        (r * 1.1, 0.6),
        (r * 0.85, LIGHT_H * 0.35),
        (r * 0.75, LIGHT_H * 0.7),
        (r * 1.25, LIGHT_H * 0.72),
        (r * 1.25, LIGHT_H * 0.76),
        (r * 0.6, LIGHT_H * 0.77),
        (r * 0.6, LIGHT_H * 0.9),
        (r * 0.8, LIGHT_H * 0.92),
        (0.0, LIGHT_H * 1.02),
    ];
    let red = rgb(196, 40, 40);
    let white = rgb(244, 242, 236);
    lathe(here.b, base, DVec3::Y, &prof, seg, |k| {
        Some(match k {
            0 | 1 => (Surf::Plain, white),
            2 => (Surf::Plain, red),
            3 => (Surf::Plain, white),
            4..=6 => (Surf::Metal, chrome()),
            7 => (Surf::Glow, rgb(255, 80, 60)),
            _ => (Surf::Plain, red),
        })
    });
}

/// The harbour master's tower: a fluted shaft on a glazed podium, three
/// Googie fins, a flying-saucer cab with a band of windows, and a mast
/// with an atom and a beacon.
fn tower(here: &mut Here, h: &Harbour, lod: Lod) {
    let c = here.p(h.to_plan(DVec2::new(0.0, TOWER_Z)), h.deck);
    let seg = lod.pick(32, 16, 8);
    let (cr, c0, c1) = TOWER_CAB;
    // Podium with a glass band.
    let podium = [
        (0.0, 0.0),
        (TOWER_SHAFT + 1.2, 0.0),
        (TOWER_SHAFT + 1.2, 0.5),
        (TOWER_SHAFT + 1.1, 0.6),
        (TOWER_SHAFT + 1.1, 2.9),
        (TOWER_SHAFT + 1.3, 3.0),
        (TOWER_SHAFT + 1.3, 3.4),
        (TOWER_SHAFT, 3.4),
    ];
    lathe(here.b, c, DVec3::Y, &podium, seg, |k| {
        Some(match k {
            3 => (pane_surf(), glass()),
            _ => (Surf::Plain, cream()),
        })
    });
    // The shaft, narrowing a little, with a teal band of windows up it.
    let shaft = [
        (TOWER_SHAFT, 3.4),
        (TOWER_SHAFT * 0.92, c0 * 0.6),
        (TOWER_SHAFT * 0.86, c0),
    ];
    lathe(here.b, c, DVec3::Y, &shaft, seg, |_| {
        Some((Surf::Plain, cream()))
    });
    if lod != Lod::Far {
        // A slot of windows up the shaft facing the sea.
        let out = DVec3::new(h.dir.x, 0.0, h.dir.y);
        let side = DVec3::new(h.right.x, 0.0, h.right.y);
        let r0 = TOWER_SHAFT * 0.88 + 0.03;
        here.b.surf = pane_surf();
        here.b.quad(
            [
                c + out * r0 - side * 0.5 + DVec3::Y * 4.0,
                c + out * r0 + side * 0.5 + DVec3::Y * 4.0,
                c + out * r0 * 0.98 + side * 0.5 + DVec3::Y * (c0 - 1.0),
                c + out * r0 * 0.98 - side * 0.5 + DVec3::Y * (c0 - 1.0),
            ],
            out,
            glass(),
        );
        here.b.surf = Surf::Plain;
        // Three fins.
        for k in 0..3 {
            let a = TAU * k as f64 / 3.0 + PI / 3.0;
            let d = out * a.cos() + side * a.sin();
            let foot0 = c + d * (TOWER_SHAFT + 1.2) + DVec3::Y * 3.4;
            let foot1 = c + d * TOWER_SHAFT * 0.9;
            let tip = c + d * (TOWER_SHAFT * 0.86) + DVec3::Y * (c0 + 0.5);
            let out_far = c + d * (TOWER_SHAFT + 2.6) + DVec3::Y * 1.0;
            let n = d.cross(DVec3::Y);
            let th = n * 0.12;
            for (s, nn) in [(1.0, n), (-1.0, -n)] {
                let o = th * s;
                here.b.tri([foot1 + o, out_far + o, tip + o], nn, teal());
                here.b.tri([foot0 + o, out_far + o, foot1 + o], nn, teal());
            }
        }
    }
    // The saucer cab.
    let cab = [
        (TOWER_SHAFT * 0.86, c0 - 0.6),
        (cr * 0.7, c0 - 0.2),
        (cr, c0 + 0.6),
        (cr * 0.98, c0 + 0.9),
        (cr * 0.92, c0 + 3.0),
        (cr * 1.02, c0 + 3.2),
        (cr * 0.9, c1 - 0.4),
        (cr * 0.45, c1),
        (0.4, c1 + 0.2),
    ];
    lathe(here.b, c, DVec3::Y, &cab, seg, |k| {
        Some(match k {
            2 => (Surf::Metal, chrome()),
            3 => (pane_surf(), glass()),
            4 => (Surf::Metal, chrome()),
            _ => (Surf::Plain, if k < 2 { teal() } else { cream() }),
        })
    });
    // The mast, an atom and a beacon.
    let top = TOWER_TOP;
    here.b.cylinder(
        c + DVec3::Y * c1,
        (0.3, 0.12),
        top - c1,
        lod.pick(10, 6, 4),
        false,
        chrome(),
    );
    if lod != Lod::Far {
        let a = c + DVec3::Y * (c1 + 4.0);
        for k in 0..3 {
            let tilt = PI * k as f64 / 3.0;
            let path: Vec<DVec3> = (0..=24)
                .map(|j| {
                    let t = TAU * j as f64 / 24.0;
                    let (s, co) = t.sin_cos();
                    let p = DVec3::new(co * 1.4, s * 0.5, 0.0);
                    let (ts, tc) = tilt.sin_cos();
                    a + DVec3::new(p.x * tc - p.z * ts, p.y, p.x * ts + p.z * tc)
                })
                .collect();
            tube(
                here.b,
                &path,
                (0.06, 0.06),
                5,
                Some((Surf::Metal, chrome())),
            );
        }
        here.b.surf = Surf::Glow;
        here.b.dome(a, 0.35, 10, 4, rgb(120, 230, 255));
        here.b.dome(a, -0.35, 10, 4, rgb(120, 230, 255));
        here.b.surf = Surf::Plain;
    }
}

fn pane_surf() -> Surf {
    Surf::Lit(crate::daylight::channel(crate::daylight::Tone::Cool, 4))
}

/// The gangway towers on the passenger quay, with their covered bridges
/// out to the liner's side.
fn gangways(here: &mut Here, h: &Harbour, lod: Lod) {
    let (hx, hz, ht) = GANGWAY_TOWER;
    for z in GANGWAYS {
        let c = h.to_plan(DVec2::new(-MOLE_HALF + hx + 0.6, z));
        let out = -h.right;
        let along = h.dir;
        let base = here.p(c, h.deck);
        let o3 = DVec3::new(out.x, 0.0, out.y);
        let a3 = DVec3::new(along.x, 0.0, along.y);
        crate::carmesh::slab(
            here.b,
            base + DVec3::Y * (ht * 0.5),
            [o3, DVec3::Y],
            DVec3::new(hx, ht * 0.5, hz),
            cream(),
        );
        if lod != Lod::Far {
            // A tall stair window on its land face, a blue stripe, a cap.
            here.b.surf = pane_surf();
            here.b.quad(
                [
                    base - o3 * (hx + 0.03) - a3 * 0.8 + DVec3::Y * 1.0,
                    base - o3 * (hx + 0.03) + a3 * 0.8 + DVec3::Y * 1.0,
                    base - o3 * (hx + 0.03) + a3 * 0.8 + DVec3::Y * (ht - 1.5),
                    base - o3 * (hx + 0.03) - a3 * 0.8 + DVec3::Y * (ht - 1.5),
                ],
                -o3,
                glass(),
            );
            here.b.surf = Surf::Plain;
            crate::carmesh::slab(
                here.b,
                base + DVec3::Y * (ht + 0.15),
                [o3, DVec3::Y],
                DVec3::new(hx + 0.35, 0.15, hz + 0.35),
                teal(),
            );
        }
        // The covered bridge: a capsule tube out over the fenders.
        let reach = FENDER + 1.0;
        let y = h.deck + GANGWAY_Y + 1.0;
        let from = here.p(c, y);
        let to = here.p(c + out * (hx + reach + 0.6), y);
        tube(
            here.b,
            &[from, to],
            (1.0, 1.15),
            lod.pick(14, 8, 6),
            Some((Surf::Plain, cream())),
        );
        if lod == Lod::Near {
            for k in 0..3 {
                let t = 0.45 + 0.18 * k as f64;
                let p = from.lerp(to, t);
                for s in [-1.0, 1.0] {
                    crate::carmesh::disc(
                        here.b,
                        p + a3 * (1.03 * s),
                        (o3 * 0.28, DVec3::Y * 0.28),
                        10,
                        a3 * s,
                        glass(),
                    );
                }
            }
        }
    }
}

/// The harbour's fixed parts for `origin`: near, one mesh per texture;
/// farther, one mesh with the textures dropped.
pub fn fixed(h: &Harbour, w: &TownWorld, origin: DVec3, lod: Lod) -> Vec<Mesh> {
    let mut b = MeshBuilder::new();
    {
        let mut here = Here {
            b: &mut b,
            o: origin,
        };
        mole(&mut here, h, w, lod);
        breakwater(&mut here, h, w, lod);
        harbour_light(&mut here, h, lod);
        tower(&mut here, h, lod);
        gangways(&mut here, h, lod);
    }
    if lod == Lod::Near {
        b.finish_parts()
    } else {
        vec![b.finish()]
    }
}

// ----------------------------------------------------------- crane parts

/// The crane's portal in its own frame: origin on the deck in the middle
/// of the four legs, +z out over the water, +x along the quay. Four
/// tapered legs on bogies, bracing, girders and the slewing ring.
pub fn crane_portal(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let gauge = (CRANE_LEGS.1 - CRANE_LEGS.0) * 0.5;
    let h = PORTAL_H;
    for sx in [-4.0, 4.0] {
        for sz in [-gauge, gauge] {
            // A leg: wide at the foot, slim at the top, leaning in.
            let foot = DVec3::new(sx, 0.9, sz);
            let top = DVec3::new(sx * 0.8, h - 1.0, sz * 0.86);
            let rows: Vec<Vec<DVec3>> = [(foot, 0.45), (top, 0.32)]
                .iter()
                .map(|&(c, r)| {
                    [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                        .iter()
                        .map(|&(x, z)| c + DVec3::new(x * r, 0.0, z * r))
                        .collect()
                })
                .collect();
            b.grid(&rows, true, true, |_, _| Some((Surf::Plain, orange())));
            // The bogie on its rail.
            crate::carmesh::slab(
                &mut b,
                DVec3::new(sx, 0.45, sz),
                [DVec3::X, DVec3::Y],
                DVec3::new(1.4, 0.45, 0.5),
                rgb(52, 52, 56),
            );
            if lod == Lod::Near {
                for wx in [-0.9, 0.9] {
                    b.cylinder(
                        DVec3::new(sx + wx, 0.0, sz - 0.55),
                        (0.32, 0.32),
                        0.0001,
                        10,
                        false,
                        rgb(30, 30, 32),
                    );
                    let c = DVec3::new(sx + wx, 0.32, sz);
                    let saved = b.surf;
                    b.surf = Surf::Metal;
                    crate::carmesh::disc(
                        &mut b,
                        c + DVec3::Z * 0.52,
                        (DVec3::X * 0.3, DVec3::Y * 0.3),
                        10,
                        DVec3::Z,
                        rgb(80, 80, 84),
                    );
                    crate::carmesh::disc(
                        &mut b,
                        c - DVec3::Z * 0.52,
                        (DVec3::X * 0.3, DVec3::Y * 0.3),
                        10,
                        -DVec3::Z,
                        rgb(80, 80, 84),
                    );
                    b.surf = saved;
                }
            }
        }
    }
    // Girders round the top, and cross bracing on the long sides.
    for sz in [-gauge * 0.86, gauge * 0.86] {
        crate::carmesh::slab(
            &mut b,
            DVec3::new(0.0, h - 0.6, sz),
            [DVec3::X, DVec3::Y],
            DVec3::new(4.6, 0.6, 0.35),
            cream(),
        );
    }
    for sx in [-3.2, 3.2] {
        crate::carmesh::slab(
            &mut b,
            DVec3::new(sx, h - 0.6, 0.0),
            [DVec3::Z, DVec3::Y],
            DVec3::new(gauge * 0.9, 0.6, 0.35),
            cream(),
        );
    }
    if lod != Lod::Far {
        for sz in [-gauge, gauge] {
            for s in [-1.0, 1.0] {
                let a = DVec3::new(-4.0 * s * 0.95, 1.6, sz * 0.97);
                let e = DVec3::new(4.0 * s * 0.8, h - 1.4, sz * 0.88);
                tube(
                    &mut b,
                    &[a, e],
                    (0.12, 0.12),
                    6,
                    Some((Surf::Plain, orange())),
                );
            }
        }
    }
    // The slewing ring.
    b.surf = Surf::Metal;
    b.cylinder(
        DVec3::new(0.0, h, 0.0),
        (2.6, 2.6),
        0.5,
        lod.pick(24, 12, 8),
        true,
        rgb(70, 70, 74),
    );
    b.surf = Surf::Plain;
    b.finish()
}

/// The crane's turning part in its own frame: origin on the deck under the
/// slewing ring, +z the way the boom points. The streamlined machine house
/// with its glazed cab, the A-frame, the lattice boom and its counterweight,
/// and the hook hanging from the boom's tip.
pub fn crane_top(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let (hx, hz, hh) = CRANE_HOUSE;
    let y0 = PORTAL_H + 0.5;
    // The house: a rounded loaf, the cab's end glazed.
    let n = lod.pick(10, 5, 2);
    let section = |z: f64, k: f64| -> Vec<DVec3> {
        (0..=n * 2)
            .map(|j| {
                let a = PI * j as f64 / (n * 2) as f64;
                let x = -hx * k * a.cos();
                let y = y0 + (hh - 0.6) * k.sqrt() * a.sin().powf(0.4);
                DVec3::new(x, y, z)
            })
            .collect()
    };
    let zc = -0.6;
    let zs = [
        zc - hz,
        zc - hz + 0.5,
        zc + hz - 1.2,
        zc + hz - 0.3,
        zc + hz,
    ];
    let ks = [0.8, 1.0, 1.0, 0.92, 0.7];
    let rows: Vec<Vec<DVec3>> = zs.iter().zip(ks).map(|(&z, k)| section(z, k)).collect();
    b.grid(&rows, false, true, |i, _| {
        Some(if i == 3 {
            (pane_surf(), glass())
        } else {
            (Surf::Plain, cream())
        })
    });
    // Its floor, and the two ends closed.
    for (z, k) in [(zs[0], ks[0]), (zs[4], ks[4])] {
        let ring = section(z, k);
        let c = DVec3::new(0.0, y0, z);
        for j in 0..ring.len() - 1 {
            b.tri([c, ring[j], ring[j + 1]], DVec3::Z * (z - zc), cream());
        }
    }
    b.quad(
        [
            DVec3::new(-hx, y0, zs[0]),
            DVec3::new(hx, y0, zs[0]),
            DVec3::new(hx, y0, zs[4]),
            DVec3::new(-hx, y0, zs[4]),
        ],
        -DVec3::Y,
        rgb(60, 60, 64),
    );
    // A teal stripe along its flank and the maker's chrome spear.
    if lod != Lod::Far {
        for s in [-1.0, 1.0] {
            let y = y0 + 0.9;
            b.surf = Surf::Metal;
            b.quad(
                [
                    DVec3::new(s * (hx + 0.02), y, zs[0] + 0.4),
                    DVec3::new(s * (hx + 0.02), y, zs[3]),
                    DVec3::new(s * (hx + 0.02), y + 0.12, zs[3]),
                    DVec3::new(s * (hx + 0.02), y + 0.12, zs[0] + 0.4),
                ],
                DVec3::X * s,
                chrome(),
            );
            b.surf = Surf::Plain;
            b.quad(
                [
                    DVec3::new(s * (hx + 0.015), y0 + 0.05, zs[0] + 0.2),
                    DVec3::new(s * (hx + 0.015), y0 + 0.05, zs[3]),
                    DVec3::new(s * (hx + 0.015), y0 + 0.75, zs[3]),
                    DVec3::new(s * (hx + 0.015), y0 + 0.75, zs[0] + 0.2),
                ],
                DVec3::X * s,
                teal(),
            );
        }
    }
    // The A-frame behind the cab, and the counterweight jib.
    let apex = DVec3::new(0.0, y0 + hh + 5.5, -1.2);
    for s in [-1.0, 1.0] {
        tube(
            &mut b,
            &[DVec3::new(s * 1.6, y0 + hh - 0.6, 0.8), apex],
            (0.2, 0.2),
            lod.pick(8, 5, 4),
            Some((Surf::Plain, orange())),
        );
        tube(
            &mut b,
            &[DVec3::new(s * 1.6, y0 + hh - 0.6, -3.4), apex],
            (0.2, 0.2),
            lod.pick(8, 5, 4),
            Some((Surf::Plain, orange())),
        );
    }
    crate::carmesh::slab(
        &mut b,
        DVec3::new(0.0, y0 + 1.8, zc - hz - 1.0),
        [DVec3::X, DVec3::Y],
        DVec3::new(2.2, 1.6, 1.0),
        rgb(70, 70, 74),
    );
    // The boom.
    let (sl, cl) = BOOM_LIFT.sin_cos();
    let hinge = DVec3::new(0.0, BOOM_HINGE.0, BOOM_HINGE.1);
    let axis = DVec3::new(0.0, sl, cl);
    let tip = hinge + axis * BOOM_LEN;
    let up = DVec3::new(0.0, cl, -sl);
    let w0 = 0.8;
    let lower =
        |s: f64, t: f64| hinge + axis * t + DVec3::X * (s * w0 * (1.0 - 0.45 * t / BOOM_LEN));
    let upper = |t: f64| hinge + axis * t + up * (1.4 * (1.0 - 0.5 * t / BOOM_LEN));
    match lod {
        Lod::Far => {
            tube(
                &mut b,
                &[hinge, tip],
                (0.7, 0.7),
                4,
                Some((Surf::Plain, orange())),
            );
        }
        _ => {
            let chord = lod.pick(0.12, 0.2, 0.2);
            let seg = lod.pick(6, 4, 4);
            let c = Some((Surf::Plain, orange()));
            for s in [-1.0, 1.0] {
                tube(
                    &mut b,
                    &[lower(s, 0.0), lower(s, BOOM_LEN)],
                    (chord, chord),
                    seg,
                    c,
                );
            }
            tube(
                &mut b,
                &[upper(0.0), upper(BOOM_LEN)],
                (chord, chord),
                seg,
                c,
            );
            if lod == Lod::Near {
                // Lacing: zigzags on each face.
                let k = 12;
                for i in 0..k {
                    let (t0, t1) = (
                        BOOM_LEN * i as f64 / k as f64,
                        BOOM_LEN * (i + 1) as f64 / k as f64,
                    );
                    let lace = Some((Surf::Plain, cream()));
                    tube(&mut b, &[lower(-1.0, t0), upper(t1)], (0.05, 0.05), 4, lace);
                    tube(&mut b, &[lower(1.0, t0), upper(t1)], (0.05, 0.05), 4, lace);
                    tube(
                        &mut b,
                        &[lower(-1.0, t0), lower(1.0, t1)],
                        (0.05, 0.05),
                        4,
                        lace,
                    );
                }
            } else {
                // Solid webs seen from afar.
                for s in [-1.0, 1.0] {
                    b.quad(
                        [
                            lower(s, 0.0),
                            lower(s, BOOM_LEN),
                            upper(BOOM_LEN),
                            upper(0.0),
                        ],
                        DVec3::X * s,
                        orange(),
                    );
                }
            }
        }
    }
    // The pulley head, the luffing cables from the A-frame, the hook.
    b.surf = Surf::Metal;
    b.cylinder(
        tip - DVec3::Y * 0.4,
        (0.5, 0.5),
        0.8,
        lod.pick(12, 8, 4),
        true,
        rgb(60, 60, 64),
    );
    b.surf = Surf::Plain;
    if lod != Lod::Far {
        let rope = Some((Surf::Plain, rgb(34, 34, 36)));
        tube(
            &mut b,
            &[apex, upper(BOOM_LEN * 0.7)],
            (0.04, 0.04),
            4,
            rope,
        );
        let hook = tip - DVec3::Y * 12.0;
        tube(&mut b, &[tip, hook], (0.035, 0.035), 4, rope);
        crate::carmesh::slab(
            &mut b,
            hook - DVec3::Y * 0.4,
            [DVec3::X, DVec3::Y],
            DVec3::new(0.35, 0.4, 0.25),
            rgb(232, 190, 40),
        );
        tube(
            &mut b,
            &[
                hook - DVec3::Y * 0.8,
                hook - DVec3::Y * 1.3,
                hook - DVec3::Y * 1.3 + DVec3::Z * 0.3,
            ],
            (0.06, 0.06),
            5,
            Some((Surf::Metal, rgb(90, 90, 94))),
        );
    }
    b.finish()
}

/// A mooring bollard: a squat cast-iron post with a mushroom head.
pub fn mooring_bollard(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let iron = rgb(34, 36, 40);
    let prof = [
        (0.0, 0.0),
        (0.34, 0.0),
        (0.32, 0.08),
        (0.24, 0.14),
        (0.2, 0.48),
        (0.26, 0.56),
        (0.36, 0.6),
        (0.36, 0.68),
        (0.22, 0.74),
        (0.0, 0.76),
    ];
    lathe(
        &mut b,
        DVec3::ZERO,
        DVec3::Y,
        &prof,
        lod.pick(18, 10, 6),
        |_| Some((Surf::Metal, iron)),
    );
    b.finish()
}

/// A stack of cargo crates and drums, `hx` by `hz` and `h` high (its own
/// frame, standing on y = 0).
pub fn crates(hx: f64, hz: f64, h: f64, lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let woods = [rgb(168, 124, 76), rgb(150, 108, 64), rgb(186, 146, 96)];
    let layers = (h / 1.3).round().max(1.0) as usize;
    let lh = h / layers as f64;
    let nx = ((2.0 * hx) / 1.3).round().max(1.0) as usize;
    let nz = ((2.0 * hz) / 1.3).round().max(1.0) as usize;
    let (cx, cz) = (2.0 * hx / nx as f64, 2.0 * hz / nz as f64);
    for l in 0..layers {
        for i in 0..nx {
            for k in 0..nz {
                let col = woods[(i + k * 2 + l) % 3];
                let c = DVec3::new(
                    -hx + cx * (i as f64 + 0.5),
                    lh * (l as f64 + 0.5),
                    -hz + cz * (k as f64 + 0.5),
                );
                crate::carmesh::slab(
                    &mut b,
                    c,
                    [DVec3::X, DVec3::Y],
                    DVec3::new(cx * 0.47, lh * 0.48, cz * 0.47),
                    col,
                );
                if lod == Lod::Near {
                    // Dark bands round each crate.
                    for dy in [-0.3, 0.3] {
                        crate::carmesh::slab(
                            &mut b,
                            c + DVec3::Y * (lh * dy),
                            [DVec3::X, DVec3::Y],
                            DVec3::new(cx * 0.48, 0.05, cz * 0.48),
                            rgb(70, 52, 34),
                        );
                    }
                }
            }
        }
    }
    b.finish()
}

/// Levels of an instanced part.
pub fn levels(make: impl Fn(Lod) -> Mesh, near: f32, mid: f32, far: f32) -> Vec<LodLevel> {
    vec![
        LodLevel::new(make(Lod::Near), near),
        LodLevel::new(make(Lod::Mid), mid),
        LodLevel::new(make(Lod::Far), far),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crane_parts_build() {
        for lod in [Lod::Near, Lod::Mid, Lod::Far] {
            assert!(crane_portal(lod).face_count() > 20);
            assert!(crane_top(lod).face_count() > 20);
            assert!(mooring_bollard(lod).face_count() > 10);
            assert!(crates(2.0, 3.0, 2.6, lod).face_count() > 10);
        }
        assert!(crane_top(Lod::Near).face_count() > crane_top(Lod::Far).face_count());
    }
}
