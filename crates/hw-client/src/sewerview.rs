//! Sewer water and the rooms off the utility tunnels: a murky, dull stream
//! down each tunnel (never a polished sheet, which reads as glass), and
//! pump halls, workers' rooms and smugglers' dens with their furniture.
//! Every solid piece is a `hw_gen::urban::sewer::Prop`, the same numbers
//! collision uses.

use crate::citymesh::{MeshBuilder, Surf};
use crate::tilestream::Glow;
use crate::undergroundview::{open_elsewhere, tube, v3};
use engine::prelude::*;
use glam::{DVec2, DVec3};
use hw_gen::urban::sewer::{Prop, PropKind, Room, RoomKind};
use hw_gen::urban::underground::{Underground, Void, TUNNEL_HALF};
use std::f64::consts::TAU;

/// Half width of the stream, and how high it stands over the floor (m).
const STREAM_HALF: f64 = 0.5;
const STREAM_UP: f64 = 0.035;

fn hash(a: u32, b: u32) -> u32 {
    let mut h = a.wrapping_mul(0x9e37_79b1) ^ b.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^ (h >> 12)
}

fn murk(seed: u32) -> Color {
    // Dull olive-brown with some variation, never shiny.
    let k = (seed % 7) as u8;
    rgb(44 + k * 2, 50 + k * 2, 30 + k)
}

/// A murky stream along the middle of tunnel `i`, with dull kerbs, scum
/// and foam at the edges, dry where another void opens onto the tunnel.
pub fn tunnel_stream(b: &mut MeshBuilder, u: &Underground, i: usize) {
    let v = &u.voids[i];
    let len = v.max.x - v.max.y;
    if len < 0.5 {
        return;
    }
    let side = v.inward();
    let seed = hash(
        v.origin.x.round() as i32 as u32,
        v.origin.y.round() as i32 as u32,
    );
    let n = (len / 1.0).ceil().max(1.0) as usize;
    b.surf = Surf::Plain;
    for k in 0..n {
        let (x0, x1) = (len * k as f64 / n as f64, len * (k as f64 + 1.0) / n as f64);
        let at = |x: f64, off: f64, up: f64| {
            let p = v.world(DVec2::new(x, 0.0)) + side * off;
            v3(p, v.floor_at(p) + up)
        };
        let m = at((x0 + x1) * 0.5, 0.0, 0.3);
        if open_elsewhere(u, i, m) || open_elsewhere(u, i, at(x0, STREAM_HALF + 0.6, 0.3)) {
            continue;
        }
        let h = hash(seed, k as u32);
        b.quad(
            [
                at(x0, -STREAM_HALF, STREAM_UP),
                at(x1, -STREAM_HALF, STREAM_UP),
                at(x1, STREAM_HALF, STREAM_UP),
                at(x0, STREAM_HALF, STREAM_UP),
            ],
            DVec3::Y,
            murk(h),
        );
        // Dull kerbs at both edges.
        for s in [-1.0, 1.0] {
            let (a, c) = (s * STREAM_HALF, s * (STREAM_HALF + 0.12));
            b.quad(
                [
                    at(x0, a, 0.06),
                    at(x1, a, 0.06),
                    at(x1, c, 0.06),
                    at(x0, c, 0.06),
                ],
                DVec3::Y,
                rgb(104, 100, 92),
            );
        }
        // A scum patch or a rag of foam now and then.
        if h.is_multiple_of(5) {
            let o = ((h >> 8) % 7) as f64 / 7.0 - 0.5;
            let (c0, c1) = (x0 + 0.1, x0 + 0.7);
            b.quad(
                [
                    at(c0, o * 0.5 - 0.2, STREAM_UP + 0.004),
                    at(c1, o * 0.5 - 0.2, STREAM_UP + 0.004),
                    at(c1, o * 0.5 + 0.2, STREAM_UP + 0.004),
                    at(c0, o * 0.5 + 0.2, STREAM_UP + 0.004),
                ],
                DVec3::Y,
                if h.is_multiple_of(2) {
                    rgb(92, 98, 64)
                } else {
                    rgb(120, 118, 104)
                },
            );
        }
    }
    let _ = TUNNEL_HALF;
}

fn boxp(b: &mut MeshBuilder, p: &Prop, lift: f64, h: f64, scale: DVec2, col: Color) {
    let u = DVec3::new(p.dir.y, 0.0, -p.dir.x);
    let c = v3(p.at, p.floor + lift + h * 0.5);
    b.oriented_box(
        c,
        u,
        DVec3::new(p.half.x * scale.x, h * 0.5, p.half.y * scale.y),
        col,
    );
}

fn ring(b: &mut MeshBuilder, c: DVec3, axis_u: DVec3, axis_v: DVec3, r: f64, w: f64, col: Color) {
    let pts: Vec<DVec3> = (0..=16)
        .map(|k| {
            let a = TAU * k as f64 / 16.0;
            c + axis_u * (a.cos() * r) + axis_v * (a.sin() * r)
        })
        .collect();
    for s in pts.windows(2) {
        tube(b, s[0], s[1], w, col);
    }
}

fn furniture(b: &mut MeshBuilder, p: &Prop) {
    let f = p.floor;
    let top = f + p.height;
    let c3 = v3(p.at, f);
    b.surf = Surf::Plain;
    match p.kind {
        PropKind::Pump => {
            let r = p.half.x;
            b.cylinder(c3, (r, r), p.height * 0.8, 24, true, rgb(70, 92, 98));
            b.dome(c3 + DVec3::Y * p.height * 0.8, r, 24, 4, rgb(80, 104, 110));
            for y in [0.5, 1.7, 2.8] {
                b.cylinder(
                    c3 + DVec3::Y * y,
                    (r + 0.06, r + 0.06),
                    0.16,
                    24,
                    true,
                    rgb(176, 138, 62),
                );
            }
            // Pipes out to the sides.
            b.surf = Surf::Metal;
            for s in [-1.0, 1.0] {
                let q = p.at + DVec2::new(s * (r + 1.8), 0.0);
                tube(b, v3(p.at, f + 1.2), v3(q, f + 1.2), 0.22, rgb(150, 70, 56));
                tube(b, v3(q, f + 1.2), v3(q, f + 3.6), 0.22, rgb(150, 70, 56));
            }
            b.surf = Surf::Plain;
        }
        PropKind::Pillar => {
            b.cylinder(
                c3,
                (p.half.x, p.half.x),
                p.height,
                16,
                true,
                rgb(150, 146, 138),
            );
            for y in [0.0, 0.5] {
                b.cylinder(
                    c3 + DVec3::Y * (0.3 + y),
                    (p.half.x + 0.015, p.half.x + 0.015),
                    0.22,
                    16,
                    false,
                    if y == 0.0 {
                        rgb(226, 190, 40)
                    } else {
                        rgb(36, 36, 38)
                    },
                );
            }
        }
        PropKind::Desk => {
            boxp(
                b,
                p,
                0.0,
                p.height - 0.1,
                DVec2::new(1.0, 1.0),
                rgb(78, 96, 92),
            );
            boxp(
                b,
                p,
                p.height - 0.1,
                0.1,
                DVec2::new(1.05, 1.1),
                rgb(60, 62, 64),
            );
            b.surf = Surf::Glow;
            let u = DVec3::new(p.dir.y, 0.0, -p.dir.x);
            let c = v3(p.at + p.dir * (p.half.y * 0.6), top + 0.25);
            b.oriented_box(c, u, DVec3::new(0.45, 0.2, 0.03), rgb(110, 236, 255));
            b.surf = Surf::Plain;
        }
        PropKind::Valve => {
            b.cylinder(c3, (0.12, 0.12), p.height, 10, true, rgb(120, 70, 56));
            b.surf = Surf::Metal;
            ring(
                b,
                v3(p.at, top),
                DVec3::X,
                DVec3::Z,
                0.34,
                0.035,
                rgb(190, 40, 36),
            );
            b.surf = Surf::Plain;
        }
        PropKind::Locker => {
            boxp(b, p, 0.0, p.height, DVec2::new(1.0, 1.0), rgb(70, 112, 118));
            let front = p.at + p.dir * (p.half.y + 0.01);
            let u = DVec3::new(p.dir.y, 0.0, -p.dir.x);
            b.oriented_box(
                v3(front, f + p.height * 0.55),
                u,
                DVec3::new(0.02, 0.3, 0.01),
                rgb(40, 40, 44),
            );
        }
        PropKind::Table => {
            boxp(
                b,
                p,
                p.height - 0.08,
                0.08,
                DVec2::new(1.0, 1.0),
                rgb(140, 104, 68),
            );
            let u = DVec3::new(p.dir.y, 0.0, -p.dir.x);
            let v = DVec3::new(p.dir.x, 0.0, p.dir.y);
            for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let l = v3(p.at, f) + u * (sx * (p.half.x - 0.08)) + v * (sz * (p.half.y - 0.08));
                b.oriented_box(
                    l + DVec3::Y * (p.height * 0.5 - 0.04),
                    u,
                    DVec3::new(0.04, p.height * 0.5 - 0.04, 0.04),
                    rgb(100, 74, 50),
                );
            }
            // A mug and a lamp-ish lantern.
            b.cylinder(
                v3(p.at + DVec2::new(0.2, 0.1), top),
                (0.05, 0.05),
                0.1,
                8,
                true,
                rgb(200, 190, 170),
            );
        }
        PropKind::Stool => {
            b.cylinder(
                c3,
                (p.half.x, p.half.x),
                p.height,
                10,
                true,
                rgb(120, 84, 56),
            );
        }
        PropKind::Stove => {
            boxp(b, p, 0.0, p.height, DVec2::new(1.0, 1.0), rgb(40, 40, 42));
            uv_pipe(b, p.at, top, top + 1.8);
        }
        PropKind::Rack => {
            boxp(b, p, 0.0, p.height, DVec2::new(1.0, 1.0), rgb(84, 62, 44));
            // Tools hanging on its front.
            let u = DVec3::new(p.dir.y, 0.0, -p.dir.x);
            for k in 0..4 {
                let off = (k as f64 - 1.5) * 0.35;
                let at = v3(
                    p.at + p.dir * (p.half.y + 0.03),
                    f + 0.9 + 0.2 * (k % 2) as f64,
                );
                b.oriented_box(
                    at + DVec3::new(-p.dir.y, 0.0, p.dir.x) * 0.0
                        + DVec3::new(p.dir.x, 0.0, p.dir.y) * off * 0.0,
                    u,
                    DVec3::new(0.03, 0.3, 0.02),
                    rgb(170, 170, 176),
                );
            }
        }
        PropKind::Crate => {
            let col =
                [rgb(150, 112, 70), rgb(128, 96, 62), rgb(112, 108, 80)][(p.look % 3) as usize];
            boxp(b, p, 0.0, p.height, DVec2::new(1.0, 1.0), col);
            boxp(b, p, 0.0, p.height, DVec2::new(1.02, 0.2), rgb(86, 62, 40));
            boxp(b, p, 0.0, p.height, DVec2::new(0.2, 1.02), rgb(86, 62, 40));
        }
        PropKind::Barrel => {
            b.cylinder(
                c3,
                (p.half.x, p.half.x),
                p.height,
                16,
                true,
                rgb(90, 74, 56),
            );
            for y in [0.2, 0.8] {
                b.cylinder(
                    c3 + DVec3::Y * y,
                    (p.half.x + 0.012, p.half.x + 0.012),
                    0.05,
                    16,
                    false,
                    rgb(60, 60, 64),
                );
            }
        }
        PropKind::Bed => {
            boxp(b, p, 0.0, 0.3, DVec2::new(1.0, 1.0), rgb(78, 66, 54));
            boxp(b, p, 0.3, 0.2, DVec2::new(0.95, 0.95), rgb(98, 88, 60));
            boxp(b, p, 0.5, 0.0001, DVec2::new(0.0, 0.0), rgb(0, 0, 0));
        }
        PropKind::Strongbox => {
            boxp(b, p, 0.0, p.height, DVec2::new(1.0, 1.0), rgb(56, 62, 70));
            b.surf = Surf::Glow;
            uv_glint(b, p.at, top + 0.04);
            b.surf = Surf::Plain;
        }
    }
}

fn uv_pipe(b: &mut MeshBuilder, at: DVec2, y0: f64, y1: f64) {
    b.surf = Surf::Metal;
    tube(b, v3(at, y0), v3(at, y1), 0.06, rgb(60, 60, 64));
    b.surf = Surf::Plain;
}

fn uv_glint(b: &mut MeshBuilder, at: DVec2, y: f64) {
    crate::undergroundview::sphere(b, v3(at, y), 0.06, rgb(255, 224, 120));
}

/// A room's furniture, a murky run of water, its lamp and its glow.
pub fn room(b: &mut MeshBuilder, u: &Underground, r: &Room, glows: &mut Vec<Glow>) {
    let v: &Void = &u.voids[r.void];
    for p in &r.props {
        furniture(b, p);
    }
    // Pump halls have a stream running under the pump from the back wall.
    if r.kind == RoomKind::PumpHall {
        b.surf = Surf::Plain;
        let (y0, y1) = (v.min.y + 1.0, v.max.y - 0.1);
        let at = |x: f64, y: f64| v3(v.world(DVec2::new(x, y)), v.floor[0] + STREAM_UP);
        let _ = y0;
        b.quad(
            [
                at(-0.8, v.min.y + 3.0),
                at(0.8, v.min.y + 3.0),
                at(0.8, y1),
                at(-0.8, y1),
            ],
            DVec3::Y,
            rgb(44, 50, 30),
        );
    }
    let (col, strength, range) = match r.kind {
        RoomKind::PumpHall => (rgb(190, 225, 255), 9.0, 16.0),
        RoomKind::Workers => (rgb(255, 214, 150), 8.0, 11.0),
        RoomKind::Den => (rgb(255, 170, 90), 5.0, 9.0),
    };
    crate::deepview::hanging_lamp(b, r.lamp - DVec3::Y * 0.2, v.floor[0] + v.height);
    glows.push(Glow {
        pos: r.lamp,
        color: col,
        strength,
        range,
    });
}
