//! What lives along the Aether Tube, drawn: the mole folk's round doors,
//! lanterns, barrels and washing, their market stalls and strings of
//! lamps, their idol, glyphs and burrows, the tube workers' bench, pump
//! and cart, and the green signs over the ways up. Also the tube's
//! sections: each has its own paint ([`Paint`]), and the mole folk's carry
//! their glyphs and patches.
//!
//! Pure mesh building from `hw_gen`'s plans: runs on the deep view's worker.

use crate::carmesh::{lathe, slab, spline};
use crate::citymesh::{MeshBuilder, Surf};
use crate::tilestream::Glow;
use crate::undergroundview::{self as uv, v3};
use engine::prelude::*;
use engine::texgen::{FacadeLayer, MedievalLayer, NaturalLayer};
use glam::{DVec2, DVec3};
use hw_gen::urban::deep::{Fixture, FixtureKind, Livery};
use std::f64::consts::{PI, TAU};

/// A small hash for looks.
fn h(x: u32, k: u32) -> u32 {
    let mut v = x.wrapping_mul(0x9E37_79B1) ^ k.wrapping_mul(0x85EB_CA77);
    v ^= v >> 15;
    v = v.wrapping_mul(0x2C1B_3C6D);
    v ^= v >> 12;
    v
}

fn pick(x: u32, k: u32, n: usize) -> usize {
    (h(x, k) % n as u32) as usize
}

/// A section's paint: shell, rim (the lower edges), ribs, the lit strip,
/// and an optional band of tiles low on the shell (two colours).
pub struct Paint {
    pub shell: Color,
    pub rim: Color,
    pub rib: Color,
    pub strip: Color,
    pub band: Option<(Color, Color)>,
}

/// The paint of a style (see `hw_gen::urban::deep::Livery`).
pub fn paint(l: &Livery) -> Paint {
    let brass = rgb(198, 160, 82);
    let chrome = rgb(210, 214, 222);
    match l.style % hw_gen::urban::deep::STYLES {
        0 => Paint {
            shell: rgb(232, 224, 200),
            rim: rgb(168, 60, 48),
            rib: brass,
            strip: rgb(120, 236, 255),
            band: None,
        },
        1 => Paint {
            shell: rgb(176, 222, 196),
            rim: rgb(36, 120, 120),
            rib: chrome,
            strip: rgb(170, 255, 220),
            band: None,
        },
        2 => Paint {
            shell: rgb(170, 204, 232),
            rim: rgb(36, 50, 96),
            rib: chrome,
            strip: rgb(200, 230, 255),
            band: Some((rgb(236, 230, 214), rgb(36, 50, 96))),
        },
        3 => Paint {
            shell: rgb(236, 176, 150),
            rim: rgb(28, 128, 132),
            rib: brass,
            strip: rgb(255, 214, 160),
            band: None,
        },
        4 => Paint {
            shell: rgb(238, 234, 222),
            rim: rgb(30, 30, 34),
            rib: chrome,
            strip: rgb(140, 240, 255),
            band: Some((rgb(240, 238, 230), rgb(28, 28, 32))),
        },
        _ => Paint {
            shell: rgb(240, 214, 130),
            rim: rgb(128, 36, 44),
            rib: brass,
            strip: rgb(255, 230, 150),
            band: Some((rgb(128, 36, 44), rgb(240, 214, 130))),
        },
    }
}

/// Does a mole section's shell carry a rusty patch here (cell `k` of
/// stretch `seed`)?
pub fn patched(seed: u32, k: usize, j: usize, mole: f64) -> bool {
    mole > 0.55 && h(seed, (k * 31 + j / 3) as u32).is_multiple_of(23)
}

/// Mole glyphs painted on the tube's curved shell: strokes round the
/// shell at `x` metres along the line through `axis_at` (the line's point
/// at the walkway, `along` its way, `out` the wall's side), from 0.9 to
/// 2.1 m up.
pub fn glyphs_on_shell(
    b: &mut MeshBuilder,
    axis_at: DVec2,
    floor: f64,
    along: DVec2,
    out: DVec2,
    look: u32,
) {
    let (shell, axis) = (crate::deepview::SHELL, crate::deepview::AXIS);
    let colours = [
        rgb(240, 120, 40),
        rgb(250, 220, 80),
        rgb(90, 220, 200),
        rgb(240, 90, 150),
        rgb(250, 250, 240),
    ];
    let col = colours[pick(look, 1, colours.len())];
    // A point on the shell, `u` along and `v` metres above the floor.
    let on = |u: f64, v: f64| {
        let dy = (v - axis).clamp(-shell * 0.95, shell * 0.95);
        let r = (shell * shell - dy * dy).sqrt() - 0.03;
        v3(axis_at + along * u + out * r, floor + v)
    };
    let n = 3 + pick(look, 2, 3);
    let width = 0.16;
    for g in 0..n {
        let u0 = -0.9 + 1.8 * g as f64 / n.max(1) as f64;
        let shape = pick(look, 10 + g as u32, 5);
        // Each glyph: a few strokes in a 0.5 by 0.9 box.
        let strokes: Vec<Vec<(f64, f64)>> = match shape {
            // A spiral eye.
            0 => vec![(0..14)
                .map(|k| {
                    let a = k as f64 * 0.7;
                    let r = 0.05 + k as f64 * 0.018;
                    (0.25 + a.cos() * r, 0.5 + a.sin() * r)
                })
                .collect()],
            // A digging claw: three curved prongs.
            1 => (0..3)
                .map(|k| {
                    let x = 0.08 + k as f64 * 0.17;
                    vec![(x, 0.9), (x + 0.04, 0.55), (x + 0.12, 0.25)]
                })
                .collect(),
            // A mushroom.
            2 => vec![
                vec![(0.25, 0.15), (0.25, 0.55)],
                vec![
                    (0.0, 0.55),
                    (0.12, 0.8),
                    (0.38, 0.8),
                    (0.5, 0.55),
                    (0.0, 0.55),
                ],
            ],
            // A zigzag tunnel with a dot.
            3 => vec![
                vec![
                    (0.0, 0.85),
                    (0.45, 0.7),
                    (0.05, 0.5),
                    (0.45, 0.3),
                    (0.1, 0.1),
                ],
                vec![(0.42, 0.9), (0.46, 0.94)],
            ],
            // A sun with rays: the surface.
            _ => {
                let mut v: Vec<Vec<(f64, f64)>> = vec![(0..=10)
                    .map(|k| {
                        let a = TAU * k as f64 / 10.0;
                        (0.25 + a.cos() * 0.13, 0.55 + a.sin() * 0.13)
                    })
                    .collect()];
                for k in 0..6 {
                    let a = TAU * k as f64 / 6.0;
                    v.push(vec![
                        (0.25 + a.cos() * 0.2, 0.55 + a.sin() * 0.2),
                        (0.25 + a.cos() * 0.33, 0.55 + a.sin() * 0.33),
                    ]);
                }
                v
            }
        };
        for s in strokes {
            for w in s.windows(2) {
                let (a, c) = (w[0], w[1]);
                let pa = (u0 + a.0, 0.95 + a.1 * 1.1);
                let pc = (u0 + c.0, 0.95 + c.1 * 1.1);
                let d = DVec2::new(pc.0 - pa.0, pc.1 - pa.1);
                let len = d.length();
                if len < 1e-4 {
                    continue;
                }
                let nrm = DVec2::new(-d.y, d.x) / len * (width * 0.25);
                let q = [
                    on(pa.0 - nrm.x, pa.1 - nrm.y),
                    on(pc.0 - nrm.x, pc.1 - nrm.y),
                    on(pc.0 + nrm.x, pc.1 + nrm.y),
                    on(pa.0 + nrm.x, pa.1 + nrm.y),
                ];
                b.quad(q, v3(-out, 0.0), col);
            }
        }
    }
}

/// A new fixture kind's parts (the old kinds are drawn in `deepview`).
pub fn fixture(b: &mut MeshBuilder, f: &Fixture, glows: &mut Vec<Glow>) -> bool {
    use FixtureKind::*;
    let p = f.plan();
    let y = f.at.y;
    let s = f.size;
    match f.kind {
        RoundDoor => round_door(b, p, f.dir, y, s, f.look),
        Barrel => barrel(b, p, y, s, f.look),
        MushroomPot => mushroom_pot(b, p, y, s, f.look),
        Lantern => lantern(b, p, f.dir, y, s, f.look),
        Washing => washing(b, f.at, f.dir, s, f.look),
        Rug => rug(b, p, f.dir, y, s, f.look),
        Stall => stall(b, p, f.dir, y, f.look),
        LampString => lamp_string(b, f.at, f.dir, s, f.look),
        Workbench => workbench(b, p, f.dir, y, s, f.look),
        Pump => pump(b, p, f.dir, y, s),
        Cart => cart(b, p, f.dir, y, s, f.look),
        Idol => idol(b, p, f.dir, y, s),
        Candles => candles(b, p, f.dir, y),
        Glyphs => {
            // On the tube's shell: the fixture stands at the shell, `dir`
            // facing into the tube.
            let out = -f.dir;
            let centre = p - out * (crate::deepview::SHELL - 0.05);
            glyphs_on_shell(b, centre, y - 1.0, f.dir.perp(), out, f.look);
        }
        ExitSign => exit_sign(b, f.at, f.dir, s, glows),
        _ => return false,
    }
    true
}

/// A round wooden door with iron straps and a brass knob, in a dressed
/// stone front set into the rock, a round lit window beside it.
fn round_door(b: &mut MeshBuilder, p: DVec2, into: DVec2, y: f64, s: DVec3, look: u32) {
    let side = into.perp();
    let r = s.x;
    let c = y + r * 0.98;
    let at = |x: f64, z: f64, yy: f64| v3(p + side * x + into * z, yy);
    // The stone front: a wall built into the rock, wide and tall enough to
    // hide the rough cave wall behind.
    let before = b.tex;
    b.tex = Some(FacadeLayer::Sandstone.into());
    slab(
        b,
        at(0.0, -0.55, y + 1.45),
        [v3(side, 0.0), DVec3::Y],
        DVec3::new(2.1, 1.5, 0.5),
        rgb(176, 156, 126),
    );
    b.tex = before;
    // The door: planks in a circle, recessed a little.
    let paints = [
        rgb(46, 110, 64),
        rgb(160, 52, 40),
        rgb(40, 84, 140),
        rgb(214, 170, 52),
        rgb(110, 70, 140),
    ];
    let door = paints[pick(look, 3, paints.len())];
    let ring = |rad: f64, z: f64, n: usize| -> Vec<DVec3> {
        (0..n)
            .map(|k| {
                let a = TAU * k as f64 / n as f64;
                at(a.cos() * rad, z, c + a.sin() * rad)
            })
            .collect()
    };
    let n = 40;
    let face = ring(r, -0.02, n);
    b.polygon(&face, v3(into, 0.0), door);
    // Plank seams.
    for k in -2..=2 {
        let x = k as f64 * r * 0.36;
        let half = (r * r - x * x).max(0.0).sqrt() * 0.97;
        slab(
            b,
            at(x, -0.01, c),
            [v3(side, 0.0), DVec3::Y],
            DVec3::new(0.012, half, 0.006),
            rgb(30, 24, 18),
        );
    }
    // Iron straps and hinges.
    for yy in [0.45, -0.45] {
        let w = (r * r - (r * yy) * (r * yy)).sqrt() * 0.92;
        slab(
            b,
            at(-0.15, 0.0, c + r * yy),
            [v3(side, 0.0), DVec3::Y],
            DVec3::new(w - 0.1, 0.05, 0.015),
            rgb(50, 48, 46),
        );
    }
    b.surf = Surf::Metal;
    uv::sphere(b, at(r * 0.62, 0.06, c), 0.07, rgb(214, 170, 80));
    b.surf = Surf::Plain;
    // The frame: a thick stone ring round it.
    let frame: Vec<DVec3> = ring(r + 0.09, 0.02, n);
    let mut closed = frame.clone();
    closed.push(frame[0]);
    crate::carmesh::tube(
        b,
        &closed,
        (0.12, 0.12),
        10,
        Some((Surf::Plain, rgb(150, 132, 110))),
    );
    // A step before it.
    slab(
        b,
        at(0.0, 0.25, y + 0.06),
        [v3(side, 0.0), DVec3::Y],
        DVec3::new(r * 0.8, 0.06, 0.25),
        rgb(140, 126, 106),
    );
    // A round window beside, lit warm from inside.
    let wx = if pick(look, 4, 2) == 0 { 1.45 } else { -1.45 };
    let wc = at(wx, 0.0, y + 1.55);
    b.surf = Surf::Glow;
    let pane: Vec<DVec3> = (0..24)
        .map(|k| {
            let a = TAU * k as f64 / 24.0;
            wc + v3(side * a.cos() * 0.32, 0.0) + DVec3::Y * a.sin() * 0.32
        })
        .collect();
    b.polygon(&pane, v3(into, 0.0), rgb(255, 196, 110));
    b.surf = Surf::Plain;
    // Its cross bars and frame.
    slab(
        b,
        wc + v3(into, 0.0) * 0.01,
        [v3(side, 0.0), DVec3::Y],
        DVec3::new(0.32, 0.02, 0.01),
        rgb(60, 40, 26),
    );
    slab(
        b,
        wc + v3(into, 0.0) * 0.01,
        [v3(side, 0.0), DVec3::Y],
        DVec3::new(0.02, 0.32, 0.01),
        rgb(60, 40, 26),
    );
    let mut wring: Vec<DVec3> = pane.clone();
    wring.push(pane[0]);
    crate::carmesh::tube(
        b,
        &wring,
        (0.05, 0.05),
        8,
        Some((Surf::Plain, rgb(110, 74, 44))),
    );
    // A little round plaque over the door.
    b.surf = Surf::Metal;
    lathe(
        b,
        at(0.0, -0.01, c + r + 0.32),
        v3(into, 0.0),
        &[(0.0, 0.0), (0.14, 0.0), (0.14, 0.03), (0.0, 0.035)],
        20,
        |_| Some((Surf::Metal, rgb(200, 160, 80))),
    );
    b.surf = Surf::Plain;
}

/// A barrel: bulging staves and iron hoops.
fn barrel(b: &mut MeshBuilder, p: DVec2, y: f64, s: DVec3, look: u32) {
    let (r, hgt) = (s.x, s.y);
    let woods = [rgb(140, 96, 58), rgb(120, 82, 50), rgb(156, 110, 66)];
    let wood = woods[pick(look, 1, woods.len())];
    let before = b.tex;
    b.tex = Some(crate::skins::Tex::Layer(MedievalLayer::Oak.into()));
    lathe(
        b,
        v3(p, y),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (r * 0.84, 0.0),
            (r * 0.86, 0.02),
            (r * 0.97, hgt * 0.3),
            (r, hgt * 0.5),
            (r * 0.97, hgt * 0.7),
            (r * 0.86, hgt - 0.02),
            (r * 0.8, hgt),
            (0.0, hgt - 0.02),
        ],
        24,
        |_| Some((Surf::Plain, wood)),
    );
    b.tex = before;
    for t in [0.1, 0.33, 0.67, 0.9] {
        let rr = r * (0.86 + 0.14 * (PI * t).sin()) + 0.012;
        lathe(
            b,
            v3(p, y + hgt * t - 0.03),
            DVec3::Y,
            &[(rr, 0.0), (rr, 0.06)],
            24,
            |_| Some((Surf::Metal, rgb(64, 60, 56))),
        );
    }
}

/// A clay pot of small glowing mushrooms.
fn mushroom_pot(b: &mut MeshBuilder, p: DVec2, y: f64, s: DVec3, look: u32) {
    let r = s.x;
    lathe(
        b,
        v3(p, y),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (r * 0.7, 0.0),
            (r, r * 0.6),
            (r * 0.95, r * 0.95),
            (r * 1.05, r * 1.0),
            (r * 0.9, r * 1.0),
            (0.0, r * 0.92),
        ],
        20,
        |_| Some((Surf::Plain, rgb(176, 96, 62))),
    );
    let glows = [rgb(140, 255, 220), rgb(200, 160, 255), rgb(255, 200, 120)];
    let caps = [rgb(70, 170, 150), rgb(120, 90, 190), rgb(196, 80, 60)];
    let n = 3 + pick(look, 2, 3);
    for k in 0..n {
        let a = TAU * k as f64 / n as f64 + look as f64 * 0.1;
        let off = if k == 0 { 0.0 } else { r * 0.5 };
        let q = p + DVec2::new(a.cos(), a.sin()) * off;
        let hh = s.y * (0.4 + 0.25 * pick(look, 5 + k as u32, 3) as f64);
        uv::mushroom(
            b,
            q,
            y + r * 0.95,
            0.03,
            hh,
            0.09 + 0.03 * pick(look, 9 + k as u32, 3) as f64,
            caps[pick(look, 7, 3)],
            glows[pick(look, 8, 3)],
        );
    }
}

/// A crooked iron lamp post with a hooked arm and a caged lamp.
fn lantern(b: &mut MeshBuilder, p: DVec2, dir: DVec2, y: f64, s: DVec3, look: u32) {
    let hgt = s.y;
    let iron = rgb(46, 44, 42);
    let lean = DVec2::new((look as f64 * 1.3).sin(), (look as f64 * 0.7).cos()) * 0.06;
    let path = spline(
        &[
            v3(p, y),
            v3(p + lean, y + hgt * 0.5),
            v3(p + lean * 1.5, y + hgt * 0.92),
            v3(p + lean * 1.5 + dir * 0.25, y + hgt + 0.05),
            v3(p + lean * 1.5 + dir * 0.45, y + hgt - 0.05),
        ],
        5,
        false,
    );
    crate::carmesh::tube(b, &path, (0.045, 0.045), 8, Some((Surf::Metal, iron)));
    lathe(
        b,
        v3(p, y),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.16, 0.0),
            (0.12, 0.08),
            (0.06, 0.2),
            (0.0, 0.22),
        ],
        12,
        |_| Some((Surf::Metal, iron)),
    );
    // The lamp: a glowing globe in a cage, a cap over it.
    let lamp = v3(p + lean * 1.5 + dir * 0.45, y + hgt - 0.32);
    b.surf = Surf::Glow;
    uv::sphere(b, lamp, 0.13, rgb(255, 190, 100));
    b.surf = Surf::Plain;
    for k in 0..4 {
        let a = TAU * k as f64 / 4.0 + 0.4;
        let d = DVec3::new(a.cos(), 0.0, a.sin()) * 0.16;
        crate::carmesh::tube(
            b,
            &[
                lamp + d - DVec3::Y * 0.16,
                lamp + d * 1.1,
                lamp + d + DVec3::Y * 0.16,
            ],
            (0.012, 0.012),
            6,
            Some((Surf::Metal, iron)),
        );
    }
    lathe(
        b,
        lamp + DVec3::Y * 0.15,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.22, 0.0),
            (0.1, 0.1),
            (0.03, 0.2),
            (0.0, 0.22),
        ],
        16,
        |_| Some((Surf::Metal, rgb(70, 96, 70))),
    );
}

/// Washing on a sagging line: shirts, trousers, a sock.
fn washing(b: &mut MeshBuilder, mid: DVec3, along: DVec2, s: DVec3, look: u32) {
    let half = s.x;
    let a = mid - v3(along, 0.0) * half;
    let c = mid + v3(along, 0.0) * half;
    let sag = |t: f64| a.lerp(c, t) - DVec3::Y * (0.35 * (PI * t).sin());
    let line: Vec<DVec3> = (0..=24).map(|k| sag(k as f64 / 24.0)).collect();
    crate::carmesh::tube(
        b,
        &line,
        (0.012, 0.012),
        6,
        Some((Surf::Plain, rgb(200, 190, 170))),
    );
    // Iron pegs in the rock at both ends.
    for e in [a, c] {
        uv::sphere(b, e, 0.05, rgb(60, 56, 52));
    }
    let cloths = [
        rgb(220, 70, 60),
        rgb(240, 236, 220),
        rgb(70, 120, 190),
        rgb(240, 200, 70),
        rgb(110, 170, 90),
        rgb(200, 120, 170),
    ];
    let n = ((half * 2.0) / 0.9).floor().max(2.0) as usize;
    let side = v3(along.perp(), 0.0);
    for k in 0..n {
        let t = (k as f64 + 0.5) / n as f64;
        let top = sag(t);
        let col = cloths[pick(look, k as u32, cloths.len())];
        let (w, hh) = match pick(look, 40 + k as u32, 3) {
            0 => (0.32, 0.5),  // a shirt
            1 => (0.22, 0.75), // trousers
            _ => (0.08, 0.22), // a sock
        };
        let u = v3(along, 0.0);
        let sway = side * 0.04 * ((k as f64) * 1.7).sin();
        let q = [
            top - u * w,
            top + u * w,
            top + u * w * 0.9 - DVec3::Y * hh + sway,
            top - u * w * 0.9 - DVec3::Y * hh + sway,
        ];
        b.quad(q, side, col);
        b.quad([q[1], q[0], q[3], q[2]], -side, col);
    }
}

/// A woven oval rug with a border and stripes.
fn rug(b: &mut MeshBuilder, p: DVec2, dir: DVec2, y: f64, s: DVec3, look: u32) {
    let side = dir.perp();
    let cols = [
        (rgb(150, 50, 44), rgb(230, 190, 90)),
        (rgb(50, 90, 130), rgb(220, 210, 180)),
        (rgb(90, 120, 60), rgb(200, 150, 80)),
    ];
    let (base, trim) = cols[pick(look, 1, cols.len())];
    let n = 40;
    let ring = |k: f64, yy: f64| -> Vec<DVec3> {
        (0..n)
            .map(|i| {
                let a = TAU * i as f64 / n as f64;
                v3(
                    p + dir * (a.cos() * s.x * k) + side * (a.sin() * s.z * k),
                    yy,
                )
            })
            .collect()
    };
    b.polygon(&ring(1.0, y + 0.012), DVec3::Y, trim);
    b.polygon(&ring(0.86, y + 0.016), DVec3::Y, base);
    for k in [-0.5, 0.0, 0.5] {
        let c = v3(p + dir * (s.x * k), y + 0.02);
        let w = s.z * 0.75 * (1.0 - k * k).sqrt();
        slab(
            b,
            c,
            [v3(side, 0.0), v3(dir, 0.0)],
            DVec3::new(w, 0.06, 0.002),
            trim,
        );
    }
}

/// A market stall (Molehaven's), with goods on its counter.
fn stall(b: &mut MeshBuilder, p: DVec2, d: DVec2, y: f64, look: u32) {
    let stripes = [
        rgb(196, 64, 52),
        rgb(52, 120, 176),
        rgb(220, 170, 50),
        rgb(70, 150, 90),
        rgb(150, 80, 160),
    ];
    // Molehaven's stall stands with its back at `p - d * 0.8`: shift so
    // the counter sits on the footprint.
    uv::stall(b, p - d * 0.05, d, y, stripes[pick(look, 1, stripes.len())]);
    let side = d.perp();
    let top = y + 1.06;
    match pick(look, 2, 4) {
        0 => {
            // Glowing mushrooms in baskets.
            for k in 0..3 {
                let q = p + side * (-0.9 + 0.9 * k as f64) + d * 0.2;
                lathe(
                    b,
                    v3(q, top),
                    DVec3::Y,
                    &[
                        (0.0, 0.0),
                        (0.2, 0.0),
                        (0.26, 0.16),
                        (0.24, 0.17),
                        (0.0, 0.12),
                    ],
                    16,
                    |_| Some((Surf::Plain, rgb(170, 130, 80))),
                );
                for j in 0..3 {
                    let a = TAU * j as f64 / 3.0;
                    uv::mushroom(
                        b,
                        q + DVec2::new(a.cos(), a.sin()) * 0.09,
                        top + 0.1,
                        0.025,
                        0.12,
                        0.07,
                        rgb(120, 90, 190),
                        rgb(200, 160, 255),
                    );
                }
            }
        }
        1 => {
            // Jars of preserves and glow juice.
            for k in 0..7 {
                let q = p + side * (-1.1 + 0.36 * k as f64) + d * (0.1 + 0.15 * (k % 2) as f64);
                let col = [rgb(200, 80, 60), rgb(240, 180, 60), rgb(90, 200, 160)][k % 3];
                b.surf = if k % 3 == 2 { Surf::Glow } else { Surf::Glass };
                lathe(
                    b,
                    v3(q, top),
                    DVec3::Y,
                    &[
                        (0.0, 0.0),
                        (0.08, 0.0),
                        (0.09, 0.14),
                        (0.06, 0.2),
                        (0.0, 0.2),
                    ],
                    14,
                    |_| Some((b_surf(k), col)),
                );
                b.surf = Surf::Plain;
            }
        }
        2 => {
            // Gadgets and gears: a tinker's stall.
            for k in 0..4 {
                let q = p + side * (-1.0 + 0.66 * k as f64) + d * 0.15;
                lathe(
                    b,
                    v3(q, top),
                    DVec3::Y,
                    &[
                        (0.0, 0.0),
                        (0.16, 0.0),
                        (0.16, 0.04),
                        (0.06, 0.05),
                        (0.06, 0.12),
                        (0.0, 0.12),
                    ],
                    18,
                    |_| Some((Surf::Metal, rgb(200, 160, 80))),
                );
            }
            slab(
                b,
                v3(p + side * 0.6 + d * 0.3, top + 0.12),
                [v3(side, 0.0), DVec3::Y],
                DVec3::new(0.2, 0.12, 0.12),
                rgb(60, 110, 140),
            );
        }
        _ => {
            // Rolls of cloth and a pile of roots.
            for k in 0..4 {
                let q = p + side * (-0.9 + 0.5 * k as f64) + d * 0.2;
                let col = [
                    rgb(200, 70, 60),
                    rgb(70, 110, 170),
                    rgb(230, 200, 120),
                    rgb(90, 150, 90),
                ][k];
                crate::carmesh::tube(
                    b,
                    &[v3(q - d * 0.3, top + 0.1), v3(q + d * 0.3, top + 0.1)],
                    (0.1, 0.1),
                    14,
                    Some((Surf::Plain, col)),
                );
            }
        }
    }
}

fn b_surf(k: usize) -> Surf {
    if k % 3 == 2 {
        Surf::Glow
    } else {
        Surf::Glass
    }
}

/// Strings of lit bulbs across a cave: from end to end along `along`,
/// sagging, at a few offsets across.
fn lamp_string(b: &mut MeshBuilder, mid: DVec3, along: DVec2, s: DVec3, look: u32) {
    let side = along.perp();
    let bulbs = [
        rgb(255, 200, 110),
        rgb(255, 140, 90),
        rgb(160, 255, 200),
        rgb(200, 170, 255),
    ];
    for (k, off) in [-0.6, 0.0, 0.6].into_iter().enumerate() {
        let a = mid + v3(along * -s.x + side * (s.z * off), 0.0);
        let c = mid + v3(along * s.x + side * (s.z * -off * 0.6), 0.0);
        let pt = |t: f64| a.lerp(c, t) - DVec3::Y * (0.8 * (PI * t).sin());
        let line: Vec<DVec3> = (0..=30).map(|i| pt(i as f64 / 30.0)).collect();
        crate::carmesh::tube(
            b,
            &line,
            (0.01, 0.01),
            5,
            Some((Surf::Plain, rgb(40, 36, 32))),
        );
        let n = (a.distance(c) / 1.3) as usize;
        b.surf = Surf::Glow;
        for i in 1..n {
            let q = pt(i as f64 / n as f64) - DVec3::Y * 0.08;
            uv::sphere(
                b,
                q,
                0.07,
                bulbs[pick(look, (k * 50 + i) as u32, bulbs.len())],
            );
        }
        b.surf = Surf::Plain;
    }
}

/// A heavy bench with a vice, tools on it and a pegboard behind.
fn workbench(b: &mut MeshBuilder, p: DVec2, front: DVec2, y: f64, s: DVec3, look: u32) {
    let side = front.perp();
    let at = |x: f64, z: f64, yy: f64| v3(p + side * x + front * z, yy);
    let before = b.tex;
    b.tex = Some(crate::skins::Tex::Layer(MedievalLayer::Oak.into()));
    slab(
        b,
        at(0.0, 0.0, y + s.y - 0.04),
        [v3(side, 0.0), DVec3::Y],
        DVec3::new(s.x, 0.04, s.z),
        rgb(150, 110, 70),
    );
    slab(
        b,
        at(0.0, -0.05, y + 0.25),
        [v3(side, 0.0), DVec3::Y],
        DVec3::new(s.x - 0.08, 0.02, s.z - 0.08),
        rgb(130, 96, 60),
    );
    b.tex = before;
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        slab(
            b,
            at(sx * (s.x - 0.07), sz * (s.z - 0.07), y + (s.y - 0.08) * 0.5),
            [v3(side, 0.0), DVec3::Y],
            DVec3::new(0.05, (s.y - 0.08) * 0.5, 0.05),
            rgb(70, 74, 80),
        );
    }
    // The pegboard with tool shapes.
    slab(
        b,
        at(0.0, -s.z - 0.03, y + 1.6),
        [v3(side, 0.0), DVec3::Y],
        DVec3::new(s.x, 0.55, 0.02),
        rgb(170, 150, 110),
    );
    for k in 0..6 {
        let x = -s.x + 0.25 + k as f64 * (2.0 * s.x - 0.5) / 5.0;
        let hh = 0.18 + 0.1 * pick(look, k, 3) as f64;
        slab(
            b,
            at(x, -s.z, y + 1.6),
            [v3(side, 0.0), DVec3::Y],
            DVec3::new(0.025, hh, 0.012),
            rgb(60, 60, 64),
        );
        slab(
            b,
            at(x, -s.z, y + 1.6 + hh),
            [v3(side, 0.0), DVec3::Y],
            DVec3::new(0.07, 0.03, 0.015),
            rgb(150, 40, 36),
        );
    }
    // A vice at one end, a hammer and a toolbox on top.
    b.surf = Surf::Metal;
    slab(
        b,
        at(s.x - 0.2, s.z - 0.05, y + s.y + 0.08),
        [v3(side, 0.0), DVec3::Y],
        DVec3::new(0.1, 0.08, 0.12),
        rgb(60, 90, 120),
    );
    b.surf = Surf::Plain;
    uv::tube(
        b,
        at(-0.2, 0.1, y + s.y + 0.03),
        at(0.2, 0.25, y + s.y + 0.03),
        0.02,
        rgb(120, 80, 50),
    );
    slab(
        b,
        at(-s.x + 0.35, -0.05, y + s.y + 0.12),
        [v3(side, 0.0), DVec3::Y],
        DVec3::new(0.25, 0.12, 0.14),
        rgb(180, 40, 36),
    );
    // A hanging shade lamp over it.
    let lamp = at(0.0, 0.0, y + 2.6);
    uv::tube(b, lamp, lamp + DVec3::Y * 0.9, 0.012, rgb(40, 40, 40));
    lathe(
        b,
        lamp,
        DVec3::Y,
        &[
            (0.0, -0.02),
            (0.3, -0.02),
            (0.22, 0.08),
            (0.06, 0.2),
            (0.0, 0.22),
        ],
        20,
        |_| Some((Surf::Plain, rgb(60, 120, 90))),
    );
    b.surf = Surf::Glow;
    uv::sphere(b, lamp + DVec3::Y * 0.02, 0.07, rgb(255, 236, 190));
    b.surf = Surf::Plain;
}

/// A riveted pump: a green drum with a domed head, a flywheel, a red valve
/// wheel, two gauges and pipes up into the rock.
fn pump(b: &mut MeshBuilder, p: DVec2, front: DVec2, y: f64, s: DVec3) {
    let side = front.perp();
    let (r, hgt) = (s.x, s.y);
    let green = rgb(58, 104, 76);
    lathe(
        b,
        v3(p, y),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (r, 0.0),
            (r, 0.08),
            (r * 0.9, 0.12),
            (r * 0.9, hgt * 0.62),
            (r * 0.95, hgt * 0.64),
            (r * 0.95, hgt * 0.68),
            (r * 0.85, hgt * 0.7),
            (r * 0.6, hgt * 0.84),
            (r * 0.2, hgt * 0.9),
            (0.0, hgt * 0.9),
        ],
        28,
        |k| {
            Some((
                if k == 0 || k == 5 {
                    Surf::Metal
                } else {
                    Surf::Plain
                },
                if k == 0 || k == 5 {
                    rgb(80, 80, 84)
                } else {
                    green
                },
            ))
        },
    );
    // Rivets round the seams.
    b.surf = Surf::Metal;
    for t in [0.15, 0.6] {
        for k in 0..14 {
            let a = TAU * k as f64 / 14.0;
            let d = DVec3::new(a.cos(), 0.0, a.sin());
            uv::sphere(
                b,
                v3(p, y + hgt * t) + d * (r * 0.9 + 0.01),
                0.025,
                rgb(110, 110, 104),
            );
        }
    }
    b.surf = Surf::Plain;
    // Pipes up to the ceiling, and one along to the side.
    for (dx, dz) in [(0.15, 0.0), (-0.2, -0.25)] {
        let q = v3(p + side * dx + front * dz, y + hgt * 0.85);
        uv::tube(b, q, q + DVec3::Y * 1.6, 0.08, rgb(120, 104, 80));
    }
    // The flywheel on the side.
    let hub = v3(p + side * (r + 0.12), y + hgt * 0.4);
    let ring: Vec<DVec3> = (0..=32)
        .map(|k| {
            let a = TAU * k as f64 / 32.0;
            hub + v3(front * a.cos() * 0.45, 0.0) + DVec3::Y * a.sin() * 0.45
        })
        .collect();
    crate::carmesh::tube(
        b,
        &ring,
        (0.04, 0.04),
        8,
        Some((Surf::Metal, rgb(50, 50, 54))),
    );
    for k in 0..4 {
        let a = TAU * k as f64 / 4.0;
        let e = hub + v3(front * a.cos() * 0.44, 0.0) + DVec3::Y * a.sin() * 0.44;
        uv::tube(b, hub, e, 0.025, rgb(50, 50, 54));
    }
    // A red valve wheel in front, and two gauges.
    let valve = v3(p + front * (r * 0.9 + 0.12), y + hgt * 0.5);
    let ring: Vec<DVec3> = (0..=24)
        .map(|k| {
            let a = TAU * k as f64 / 24.0;
            valve + v3(side * a.cos() * 0.18, 0.0) + DVec3::Y * a.sin() * 0.18
        })
        .collect();
    crate::carmesh::tube(
        b,
        &ring,
        (0.025, 0.025),
        8,
        Some((Surf::Plain, rgb(200, 40, 36))),
    );
    uv::tube(
        b,
        valve - v3(front, 0.0) * 0.12,
        valve,
        0.03,
        rgb(80, 80, 84),
    );
    for (k, dx) in [-0.22f64, 0.22].into_iter().enumerate() {
        let g = v3(
            p + side * dx + front * (r * 0.9 + 0.02),
            y + hgt * 0.72 + 0.04 * k as f64,
        );
        lathe(
            b,
            g,
            v3(front, 0.0),
            &[(0.0, 0.0), (0.11, 0.0), (0.11, 0.05), (0.0, 0.06)],
            20,
            |_| Some((Surf::Metal, rgb(200, 160, 80))),
        );
        let faceup: Vec<DVec3> = (0..20)
            .map(|i| {
                let a = TAU * i as f64 / 20.0;
                g + v3(front, 0.0) * 0.061
                    + v3(side * a.cos() * 0.09, 0.0)
                    + DVec3::Y * a.sin() * 0.09
            })
            .collect();
        b.polygon(&faceup, v3(front, 0.0), rgb(240, 236, 220));
        let n = g + v3(front, 0.0) * 0.065;
        uv::tube(
            b,
            n,
            n + v3(side * 0.05, 0.0) + DVec3::Y * 0.06,
            0.006,
            rgb(180, 30, 30),
        );
    }
}

/// A mine cart on a short length of track.
fn cart(b: &mut MeshBuilder, p: DVec2, along: DVec2, y: f64, s: DVec3, look: u32) {
    let side = along.perp();
    let (hw, hl) = (s.x, s.z);
    let at = |x: f64, z: f64, yy: f64| v3(p + side * x + along * z, yy);
    // Rails and sleepers.
    for x in [-0.32, 0.32] {
        slab(
            b,
            at(x, 0.0, y + 0.06),
            [v3(side, 0.0), DVec3::Y],
            DVec3::new(0.025, 0.04, hl + 0.6),
            rgb(90, 84, 80),
        );
    }
    for k in 0..4 {
        let z = -hl - 0.4 + k as f64 * (2.0 * hl + 0.8) / 3.0;
        slab(
            b,
            at(0.0, z, y + 0.02),
            [v3(side, 0.0), DVec3::Y],
            DVec3::new(0.5, 0.02, 0.08),
            rgb(100, 70, 44),
        );
    }
    // Wheels.
    for x in [-0.32, 0.32] {
        for z in [-hl * 0.6, hl * 0.6] {
            lathe(
                b,
                at(x - 0.04f64.copysign(x), z, y + 0.2),
                v3(side * x.signum(), 0.0),
                &[(0.0, 0.0), (0.17, 0.0), (0.17, 0.06), (0.0, 0.07)],
                18,
                |_| Some((Surf::Metal, rgb(60, 58, 56))),
            );
        }
    }
    // The bucket: wider at the top.
    let paint = [rgb(150, 70, 44), rgb(90, 96, 104), rgb(70, 100, 70)][pick(look, 1, 3)];
    let (y0, y1) = (y + 0.32, y + s.y);
    let corner = |sx: f64, sz: f64, top: bool| {
        let k = if top { 1.0 } else { 0.8 };
        at(sx * hw * k, sz * hl * k, if top { y1 } else { y0 })
    };
    let quads = [
        [(-1.0, -1.0), (1.0, -1.0)],
        [(1.0, -1.0), (1.0, 1.0)],
        [(1.0, 1.0), (-1.0, 1.0)],
        [(-1.0, 1.0), (-1.0, -1.0)],
    ];
    for q in quads {
        let (a, c) = (q[0], q[1]);
        let pts = [
            corner(a.0, a.1, false),
            corner(c.0, c.1, false),
            corner(c.0, c.1, true),
            corner(a.0, a.1, true),
        ];
        let mid = (pts[0] + pts[2]) * 0.5 - v3(p, (y0 + y1) * 0.5);
        b.quad(pts, mid, paint);
        b.quad([pts[1], pts[0], pts[3], pts[2]], -mid, rgb(60, 50, 40));
    }
    b.quad(
        [
            corner(-1.0, -1.0, false),
            corner(-1.0, 1.0, false),
            corner(1.0, 1.0, false),
            corner(1.0, -1.0, false),
        ],
        DVec3::Y,
        rgb(60, 50, 40),
    );
    // Rim, and a load of ore or rubble.
    let rim: Vec<DVec3> = [
        (-1.0, -1.0),
        (1.0, -1.0),
        (1.0, 1.0),
        (-1.0, 1.0),
        (-1.0, -1.0),
    ]
    .iter()
    .map(|&(sx, sz)| corner(sx, sz, true))
    .collect();
    crate::carmesh::tube(
        b,
        &rim,
        (0.03, 0.03),
        6,
        Some((Surf::Metal, rgb(70, 66, 62))),
    );
    let before = b.tex;
    b.tex = Some(crate::skins::Tex::Layer(NaturalLayer::CliffStrata.into()));
    for k in 0..6 {
        let q = at(
            ((k % 3) as f64 - 1.0) * hw * 0.5,
            ((k / 3) as f64 - 0.5) * hl * 0.8,
            y1 - 0.05,
        );
        uv::sphere(b, q, 0.2 + 0.04 * (k % 2) as f64, rgb(110, 100, 92));
    }
    b.tex = before;
}

/// The mole folk's idol: a stout stone mole on a plinth, its goggles lit,
/// its great claws held up.
fn idol(b: &mut MeshBuilder, p: DVec2, front: DVec2, y: f64, s: DVec3) {
    let side = front.perp();
    let stone = rgb(150, 140, 126);
    let before = b.tex;
    b.tex = Some(crate::skins::Tex::Layer(NaturalLayer::Limestone.into()));
    // The plinth.
    lathe(
        b,
        v3(p, y),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (s.x, 0.0),
            (s.x, 0.12),
            (s.x * 0.85, 0.2),
            (s.x * 0.85, 0.5),
            (s.x * 0.95, 0.56),
            (0.0, 0.58),
        ],
        8,
        |_| Some((Surf::Plain, rgb(130, 120, 108))),
    );
    let base = y + 0.58;
    let hh = s.y - 0.58;
    // The body: a pear.
    lathe(
        b,
        v3(p, base),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (s.x * 0.7, 0.0),
            (s.x * 0.82, hh * 0.25),
            (s.x * 0.72, hh * 0.5),
            (s.x * 0.48, hh * 0.68),
            (0.0, hh * 0.72),
        ],
        28,
        |_| Some((Surf::Plain, stone)),
    );
    // The head, and a long snout out in front.
    let head = v3(p, base + hh * 0.78) + v3(front, 0.0) * 0.08;
    uv::sphere(b, head, s.x * 0.38, stone);
    lathe(
        b,
        head + v3(front, 0.0) * (s.x * 0.2),
        v3(front, 0.0) + DVec3::Y * -0.25,
        &[
            (s.x * 0.2, 0.0),
            (s.x * 0.16, s.x * 0.25),
            (s.x * 0.1, s.x * 0.45),
            (0.0, s.x * 0.5),
        ],
        16,
        |_| Some((Surf::Plain, stone)),
    );
    b.tex = before;
    // Goggles, lit amber.
    for sx in [-1.0, 1.0] {
        let e = head + v3(front * (s.x * 0.3) + side * (sx * s.x * 0.15), 0.06);
        lathe(
            b,
            e,
            v3(front, 0.0),
            &[(0.0, -0.02), (0.1, -0.02), (0.1, 0.04), (0.0, 0.05)],
            18,
            |_| Some((Surf::Metal, rgb(180, 140, 70))),
        );
        b.surf = Surf::Glow;
        uv::sphere(b, e + v3(front, 0.0) * 0.04, 0.075, rgb(255, 170, 70));
        b.surf = Surf::Plain;
    }
    // Arms up, each with three great claws.
    for sx in [-1.0, 1.0] {
        let shoulder = v3(p + side * (sx * s.x * 0.6), base + hh * 0.55);
        let hand = shoulder + v3(side * (sx * 0.3) + front * 0.25, hh * 0.22);
        crate::carmesh::tube(
            b,
            &[
                shoulder,
                (shoulder + hand) * 0.5 + v3(side * sx * 0.08, 0.0),
                hand,
            ],
            (0.13, 0.13),
            12,
            Some((Surf::Plain, stone)),
        );
        for k in 0..3 {
            let off = side * (sx * (k as f64 - 1.0) * 0.07);
            let tip = hand + v3(off + front * 0.18, 0.22);
            lathe(
                b,
                hand + v3(off, 0.0),
                tip - (hand + v3(off, 0.0)),
                &[(0.05, 0.0), (0.03, 0.15), (0.0, 0.27)],
                8,
                |_| Some((Surf::Plain, rgb(220, 210, 190))),
            );
        }
    }
    // Offerings at its feet.
    for k in 0..3 {
        let q = p + front * (s.x + 0.2) + side * ((k as f64 - 1.0) * 0.35);
        uv::mushroom(
            b,
            q,
            y,
            0.03,
            0.2,
            0.1,
            rgb(196, 80, 60),
            rgb(255, 200, 120),
        );
    }
}

/// A cluster of candles on a flat stone.
fn candles(b: &mut MeshBuilder, p: DVec2, front: DVec2, y: f64) {
    let side = front.perp();
    slab(
        b,
        v3(p, y + 0.06),
        [v3(side, 0.0), DVec3::Y],
        DVec3::new(0.5, 0.06, 0.3),
        rgb(120, 110, 100),
    );
    for k in 0..7 {
        let q = p + side * ((k as f64 - 3.0) * 0.13) + front * (0.08 * ((k * 3) % 4) as f64 - 0.12);
        let hh = 0.12 + 0.08 * ((k * 5) % 3) as f64;
        lathe(
            b,
            v3(q, y + 0.12),
            DVec3::Y,
            &[(0.0, 0.0), (0.035, 0.0), (0.035, hh), (0.0, hh + 0.005)],
            10,
            |_| Some((Surf::Plain, rgb(236, 226, 200))),
        );
        b.surf = Surf::Glow;
        lathe(
            b,
            v3(q, y + 0.12 + hh + 0.01),
            DVec3::Y,
            &[(0.0, 0.0), (0.018, 0.015), (0.0, 0.06)],
            8,
            |_| Some((Surf::Glow, rgb(255, 200, 90))),
        );
        b.surf = Surf::Plain;
    }
}

/// A lit green sign over a way up: an arrow up and a lift, on both faces
/// (read from both ways along the tube), hung from two rods.
fn exit_sign(b: &mut MeshBuilder, c: DVec3, along: DVec2, s: DVec3, glows: &mut Vec<Glow>) {
    let side = v3(along.perp(), 0.0);
    let n = v3(along, 0.0);
    // The case.
    slab(
        b,
        c,
        [side, DVec3::Y],
        DVec3::new(s.x + 0.04, s.y + 0.04, s.z),
        rgb(200, 204, 210),
    );
    for sx in [-1.0, 1.0] {
        let top = c + side * (sx * s.x * 0.7) + DVec3::Y * s.y;
        uv::tube(b, top, top + DVec3::Y * 0.45, 0.015, rgb(80, 80, 84));
    }
    for face in [1.0, -1.0] {
        let f = n * face;
        let o = c + f * (s.z + 0.006);
        b.surf = Surf::Glow;
        b.quad(
            [
                o - side * s.x - DVec3::Y * s.y,
                o + side * s.x - DVec3::Y * s.y,
                o + side * s.x + DVec3::Y * s.y,
                o - side * s.x + DVec3::Y * s.y,
            ],
            f,
            rgb(40, 200, 100),
        );
        // Facing this way, its own right is `side * -face`.
        let right = side * -face;
        let w = o + f * 0.004;
        let white = rgb(250, 255, 250);
        // An arrow up on the left half.
        let ax = w - right * (s.x * 0.5);
        let sh = 0.05;
        b.quad(
            [
                ax - right * sh - DVec3::Y * 0.22,
                ax + right * sh - DVec3::Y * 0.22,
                ax + right * sh + DVec3::Y * 0.06,
                ax - right * sh + DVec3::Y * 0.06,
            ],
            f,
            white,
        );
        b.tri(
            [
                ax - right * 0.15 + DVec3::Y * 0.04,
                ax + right * 0.15 + DVec3::Y * 0.04,
                ax + DVec3::Y * 0.24,
            ],
            f,
            white,
        );
        // A lift car with a figure on the right half: a box outline and a
        // round head.
        let lx = w + right * (s.x * 0.45);
        for (dx, dy, hx, hy) in [
            (0.0, 0.22, 0.17, 0.025),
            (0.0, -0.22, 0.17, 0.025),
            (-0.17, 0.0, 0.025, 0.22),
            (0.17, 0.0, 0.025, 0.22),
        ] {
            let q = lx + right * dx + DVec3::Y * dy;
            b.quad(
                [
                    q - right * hx - DVec3::Y * hy,
                    q + right * hx - DVec3::Y * hy,
                    q + right * hx + DVec3::Y * hy,
                    q - right * hx + DVec3::Y * hy,
                ],
                f,
                white,
            );
        }
        let head: Vec<DVec3> = (0..12)
            .map(|k| {
                let a = TAU * k as f64 / 12.0 * -face;
                lx + right * (a.cos() * 0.045) + DVec3::Y * (0.09 + a.sin() * 0.045)
            })
            .collect();
        b.polygon(&head, f, white);
        let body = lx - DVec3::Y * 0.05;
        b.quad(
            [
                body - right * 0.05 - DVec3::Y * 0.1,
                body + right * 0.05 - DVec3::Y * 0.1,
                body + right * 0.05 + DVec3::Y * 0.08,
                body - right * 0.05 + DVec3::Y * 0.08,
            ],
            f,
            white,
        );
        b.surf = Surf::Plain;
    }
    glows.push(Glow {
        pos: c - DVec3::Y * 0.5,
        color: rgb(90, 255, 140),
        strength: 2.5,
        range: 6.0,
    });
}
