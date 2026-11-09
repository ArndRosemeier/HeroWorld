//! The mad scientist's robot works, alive.
//!
//! Its buildings are drawn with the other landmarks ([`model_into`], in
//! the landmark LOD sets): a streamlined hall under a ribbed barrel roof,
//! the assembly line and the crane runway over it, the dock with its
//! cradle and work decks, a glass foreman's booth, the dispatch shed and
//! the gate. What moves is made when the camera comes near ([`WorksView`]):
//!
//! - Robo-sentries ride pallets out of the hall, one stop every
//!   [`CYCLE`] seconds. A crane takes a dish off its rack and lowers it on,
//!   two welding arms seam the body in a shower of sparks, a second crane
//!   hangs the gun, the test arch wakes the robot (dish turning, gun
//!   coming up, lamp amber then green), and at the end it steps off down a
//!   ramp and marches into the dispatch shed. The empty pallet tips over
//!   the end drum.
//! - In the dock a walking tank sits half built on its cradle, thighs
//!   chained to the frame, running its turret and cannon through their
//!   tests while welders spark at its hips.
//! - The sign on the hall's roof, a robot's head ringed with atom orbits,
//!   turns; smoke rises from the stacks.
//!
//! The robots are the AssetGenerator parts the fighting machines use
//! (`assets/robots`), loaded on a worker thread when the camera comes
//! within [`LOAD`], and dropped past [`DROP`]. The other moving pieces are
//! built once as instanced entities: moving one is giving it a place.
//! The line runs on a clock, so every piece's pose is a pure function of
//! the time ([`Line`]), and the soundscape hears it through [`Heard`].

use crate::carmesh::{lathe, tube};
use crate::citymesh::{MeshBuilder, Surf};
use crate::landmarkview::strut;
use engine::prelude::*;
use glam::{DVec2, DVec3, EulerRot, Mat3, Mat4, Quat, Vec3};
use hw_gen::urban::landmarks::{Landmark, LandmarkKind};
use hw_gen::urban::robotworks::*;
use hw_gen::urban::world::TownWorld;
use hw_sim::robots::{hip, leg_dir, THIGH, TRUNNION, TURRET_UP};
use std::collections::HashMap;
use std::f64::consts::{PI, TAU};
use std::sync::mpsc::{channel, Receiver};

/// The robot parts are loaded within this distance of the works (m)...
const LOAD: f32 = 650.0;
/// ... and dropped past this.
const DROP: f32 = 900.0;
/// Where the landmark's near model ends (landmarkview's near level): the
/// live pieces show only nearer, as the near model has none of them.
const NEAR_END: f32 = 700.0;
/// One stop of the line: moving, then standing while the stations work.
pub const MOVE: f64 = 2.5;
pub const DWELL: f64 = 9.5;
pub const CYCLE: f64 = MOVE + DWELL;
/// The sentry's hips over its feet (robotview's).
const HIP: f32 = 1.2;
/// Where a robot's feet stand on a pallet.
const PALLET_TOP: f64 = BELT_TOP + 0.12;
/// The crane's magnet: its lowest when it carries nothing to or from a
/// part, and the trolley's height (cables hang from it).
const HOOK_UP: f64 = 5.4;
const TROLLEY_Y: f64 = RUNWAY_Y + 0.95;
/// The welding arms: shoulder over the pedestal's foot, and their bones.
const SHOULDER: f64 = 1.9;
const UPPER: f64 = 1.3;
const FORE: f64 = 1.15;
const TORCH: f64 = 0.42;
/// How fast a finished robot marches (m/s).
const MARCH: f64 = 1.15;

// ------------------------------------------------------------ colours

fn cream() -> Color {
    rgb(222, 212, 186)
}
fn bottle() -> Color {
    rgb(46, 76, 64)
}
fn signal_red() -> Color {
    rgb(176, 38, 32)
}
fn hazard() -> Color {
    rgb(238, 166, 14)
}
fn soot() -> Color {
    rgb(30, 30, 32)
}
fn patina() -> Color {
    rgb(84, 122, 104)
}
fn iron() -> Color {
    rgb(64, 68, 66)
}
fn oxide() -> Color {
    rgb(146, 60, 42)
}
fn chrome() -> Color {
    rgb(200, 204, 208)
}
fn concrete() -> Color {
    rgb(170, 166, 156)
}
/// The windows' warm light from inside the hall.
fn lamplight() -> Color {
    rgb(255, 200, 130)
}
/// The mad scientist's green.
fn arc_green() -> Color {
    rgb(150, 255, 110)
}

fn with(b: &mut MeshBuilder, s: Surf, f: impl FnOnce(&mut MeshBuilder)) {
    let was = b.surf;
    b.surf = s;
    f(b);
    b.surf = was;
}

fn v(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z)
}

/// An axis-aligned box from its two corners.
fn cuboid(b: &mut MeshBuilder, lo: DVec3, hi: DVec3, c: Color) {
    b.oriented_box((lo + hi) * 0.5, DVec3::X, (hi - lo) * 0.5, c);
}

/// A run of alternating hazard blocks from `a` to `b` (a beam painted in
/// stripes), `half` its half size across.
fn striped(b: &mut MeshBuilder, a: DVec3, c: DVec3, w: f64, n: usize) {
    for k in 0..n {
        let (t0, t1) = (k as f64 / n as f64, (k + 1) as f64 / n as f64);
        let col = if k % 2 == 0 { hazard() } else { soot() };
        strut(b, a.lerp(c, t0), a.lerp(c, t1), w, col);
    }
}

/// A round bar with rounded ends from `a` to `c`.
fn bar(b: &mut MeshBuilder, a: DVec3, c: DVec3, r: f64, seg: usize, s: Surf, col: Color) {
    tube(b, &[a, c], (r, r), seg, Some((s, col)));
}

/// A ball.
fn ball(b: &mut MeshBuilder, c: DVec3, r: f64, seg: usize, s: Surf, col: Color) {
    let prof: Vec<(f64, f64)> = (0..=seg / 2)
        .map(|i| {
            let a = PI * i as f64 / (seg / 2) as f64;
            (r * a.sin(), r * (1.0 - a.cos()))
        })
        .collect();
    lathe(b, c - DVec3::Y * r, DVec3::Y, &prof, seg, |_| {
        Some((s, col))
    });
}

// ------------------------------------------------------------ the buildings

/// The works' buildings and fixed machinery, in its own frame (origin on
/// the plinth's top, +z the way it faces).
pub fn model_into(b: &mut MeshBuilder, near: bool) {
    hall(b, near);
    line(b, near);
    runway(b, near);
    dock(b, near);
    booth(b, near);
    shed(b, near);
    gate(b, near);
    if !near {
        // Far, the sign does not turn: draw it with the rest.
        let mut s = MeshBuilder::new();
        sign_head(&mut s, false);
        b.append(s, glam::Vec3::new(HALL_AT.x as f32, SIGN_UP as f32, 0.0));
    }
}

/// The hall's plan: a stadium round a spine along z, sampled round its
/// edge (spine point, outward unit), with columns at the door's jambs.
fn hall_ring(near: bool) -> Vec<(DVec3, DVec3)> {
    let (cx, hl) = (HALL_AT.x, HALL_HL);
    let ends = if near { 18 } else { 6 };
    let mut zs: Vec<f64> = if near {
        (0..=13).map(|i| -hl + 2.0 * hl * i as f64 / 13.0).collect()
    } else {
        vec![-hl, hl]
    };
    zs.extend([-DOOR_HW, DOOR_HW]);
    zs.sort_by(f64::total_cmp);
    zs.dedup_by(|a, b| (*a - *b).abs() < 0.2);
    let mut out = Vec::new();
    // The right side (towards the line), back to front.
    for &z in &zs {
        out.push((v(cx, 0.0, z), DVec3::X));
    }
    // The front end, round.
    for k in 1..ends {
        let a = PI * k as f64 / ends as f64;
        out.push((v(cx, 0.0, hl), v(a.cos(), 0.0, a.sin())));
    }
    // The left side, front to back.
    for &z in zs.iter().rev() {
        out.push((v(cx, 0.0, z), -DVec3::X));
    }
    for k in 1..ends {
        let a = PI + PI * k as f64 / ends as f64;
        out.push((v(cx, 0.0, -hl), v(a.cos(), 0.0, a.sin())));
    }
    out
}

/// The hall's skin across its height: (out from the spine, up), and what
/// each band between two rows is.
#[derive(Clone, Copy, PartialEq)]
enum Band {
    Plinth,
    Wall,
    Stripe,
    Windows,
    Frieze,
    Lip,
    Roof,
}

fn hall_profile(near: bool) -> Vec<((f64, f64), Band)> {
    let hw = HALL_HW;
    let mut p = vec![
        ((hw + 0.14, 0.0), Band::Plinth),
        ((hw + 0.14, 0.55), Band::Plinth),
        ((hw, 0.62), Band::Wall),
        ((hw, DOOR_H), Band::Stripe),
        ((hw, DOOR_H + 0.4), Band::Windows),
        ((hw, HALL_WALL - 0.8), Band::Frieze),
        ((hw, HALL_WALL), Band::Lip),
        ((hw + 0.3, HALL_WALL + 0.12), Band::Lip),
        ((hw + 0.12, HALL_WALL + 0.3), Band::Roof),
    ];
    let n = if near { 10 } else { 3 };
    for i in 1..=n {
        let a = 0.5 * PI * i as f64 / n as f64;
        let r = (hw + 0.12) * a.cos();
        let y = HALL_WALL + 0.3 + (HALL_TOP - HALL_WALL - 0.3) * a.sin();
        p.push(((r.max(0.0), y), Band::Roof));
    }
    p
}

fn hall(b: &mut MeshBuilder, near: bool) {
    let ring = hall_ring(near);
    let prof = hall_profile(near);
    let rows: Vec<Vec<DVec3>> = prof
        .iter()
        .map(|&((r, y), _)| {
            ring.iter()
                .map(|&(s, n)| s + n * r + DVec3::Y * y)
                .collect()
        })
        .collect();
    let door = |j: usize| {
        let (s, n) = ring[j];
        n.x > 0.99 && s.z.abs() <= DOOR_HW + 1e-6
    };
    let m = ring.len();
    crate::carmesh::surface(b, &rows, true, |i, j| {
        let band = prof[i].1;
        let in_door = door(j) && door((j + 1) % m);
        match band {
            Band::Plinth => Some((Surf::Plain, bottle())),
            Band::Wall if in_door => None,
            Band::Wall => Some((Surf::Plain, cream())),
            Band::Stripe => Some((Surf::Plain, signal_red())),
            Band::Windows => Some(if near {
                (Surf::Glow, lamplight())
            } else {
                (Surf::Plain, rgb(150, 130, 100))
            }),
            Band::Frieze => Some((Surf::Plain, cream())),
            Band::Lip => Some((Surf::Metal, chrome())),
            Band::Roof => Some((Surf::Plain, patina())),
        }
    });
    let (cx, hl, hw) = (HALL_AT.x, HALL_HL, HALL_HW);
    // The door: a dark way in, framed in hazard stripes, a shutter's drum
    // over it and a lamp.
    let wall_x = cx + hw;
    let inside = |b: &mut MeshBuilder| {
        let (x0, x1) = (wall_x - 4.0, wall_x + 0.02);
        let (z0, z1) = (-DOOR_HW, DOOR_HW);
        let h = DOOR_H;
        let dark = rgb(22, 22, 20);
        // Faces looking into the tunnel.
        b.quad(
            [
                v(x0, 0.01, z0),
                v(x1, 0.01, z0),
                v(x1, 0.01, z1),
                v(x0, 0.01, z1),
            ],
            DVec3::Y,
            rgb(40, 40, 38),
        );
        b.quad(
            [v(x0, h, z0), v(x0, h, z1), v(x1, h, z1), v(x1, h, z0)],
            -DVec3::Y,
            dark,
        );
        b.quad(
            [v(x0, 0.0, z0), v(x0, h, z0), v(x1, h, z0), v(x1, 0.0, z0)],
            DVec3::Z,
            dark,
        );
        b.quad(
            [v(x0, 0.0, z1), v(x1, 0.0, z1), v(x1, h, z1), v(x0, h, z1)],
            -DVec3::Z,
            dark,
        );
        b.quad(
            [v(x0, 0.0, z0), v(x0, 0.0, z1), v(x0, h, z1), v(x0, h, z0)],
            DVec3::X,
            rgb(12, 12, 12),
        );
    };
    inside(b);
    let jamb = |b: &mut MeshBuilder, z: f64| {
        striped(
            b,
            v(wall_x + 0.08, 0.0, z),
            v(wall_x + 0.08, DOOR_H + 0.15, z),
            0.24,
            if near { 7 } else { 1 },
        );
    };
    jamb(b, -DOOR_HW - 0.12);
    jamb(b, DOOR_HW + 0.12);
    striped(
        b,
        v(wall_x + 0.08, DOOR_H + 0.1, -DOOR_HW - 0.24),
        v(wall_x + 0.08, DOOR_H + 0.1, DOOR_HW + 0.24),
        0.24,
        if near { 8 } else { 1 },
    );
    if near {
        lathe(
            b,
            v(wall_x + 0.3, DOOR_H + 0.55, -DOOR_HW - 0.2),
            DVec3::Z,
            &[
                (0.0, 0.0),
                (0.32, 0.0),
                (0.32, 2.0 * DOOR_HW + 0.4),
                (0.0, 2.0 * DOOR_HW + 0.4),
            ],
            16,
            |_| Some((Surf::Metal, iron())),
        );
        with(b, Surf::Glow, |b| {
            ball(
                b,
                v(wall_x + 0.35, DOOR_H + 1.15, 0.0),
                0.16,
                12,
                Surf::Glow,
                lamplight(),
            );
        });
        bar(
            b,
            v(wall_x, DOOR_H + 1.15, 0.0),
            v(wall_x + 0.3, DOOR_H + 1.15, 0.0),
            0.04,
            6,
            Surf::Metal,
            iron(),
        );
    }
    // Mullions across the windows.
    if near {
        for &(s, n) in ring.iter().step_by(2) {
            let p = s + n * (hw + 0.03);
            strut(
                b,
                p + DVec3::Y * (DOOR_H + 0.4),
                p + DVec3::Y * (HALL_WALL - 0.8),
                0.1,
                bottle(),
            );
        }
    }
    // Ribs over the roof, eave to eave, and a ridge.
    let rib = |z: f64, end: Option<f64>| -> Vec<DVec3> {
        let mut pts = Vec::new();
        let roof: Vec<(f64, f64)> = prof
            .iter()
            .filter(|p| p.1 == Band::Roof)
            .map(|p| p.0)
            .collect();
        for &(r, y) in &roof {
            let n = match end {
                Some(a) => v(a.cos(), 0.0, a.sin()),
                None => DVec3::NEG_X,
            };
            pts.push(v(cx, 0.0, z) + n * (r + 0.07) + DVec3::Y * (y + 0.05));
        }
        if end.is_none() {
            for &(r, y) in roof.iter().rev().skip(1) {
                pts.push(v(cx, 0.0, z) + DVec3::X * (r + 0.07) + DVec3::Y * (y + 0.05));
            }
        }
        pts
    };
    let ribs = if near { 7 } else { 2 };
    for k in 0..=ribs {
        let z = -hl + 2.0 * hl * k as f64 / ribs as f64;
        tube(
            b,
            &rib(z, None),
            (0.08, 0.08),
            if near { 8 } else { 4 },
            Some((Surf::Metal, iron())),
        );
    }
    if near {
        for k in 1..6 {
            let a = PI * k as f64 / 6.0;
            tube(
                b,
                &rib(hl, Some(a)),
                (0.08, 0.08),
                8,
                Some((Surf::Metal, iron())),
            );
            tube(
                b,
                &rib(-hl, Some(PI + a)),
                (0.08, 0.08),
                8,
                Some((Surf::Metal, iron())),
            );
        }
    }
    bar(
        b,
        v(cx, HALL_TOP + 0.05, -hl),
        v(cx, HALL_TOP + 0.05, hl),
        0.2,
        if near { 12 } else { 4 },
        Surf::Metal,
        chrome(),
    );
    // Two smokestacks on the back slope, banded red.
    for z in [-3.2, 2.4] {
        let foot = v(cx - 2.7, HALL_TOP - 1.4, z);
        let h = 7.2;
        lathe(
            b,
            foot,
            DVec3::Y,
            &[
                (0.0, 0.0),
                (0.75, 0.0),
                (0.62, 0.6),
                (0.55, h - 0.8),
                (0.72, h - 0.5),
                (0.72, h - 0.3),
                (0.5, h),
                (0.0, h),
            ],
            if near { 24 } else { 8 },
            |k| Some((Surf::Plain, if k == 4 { signal_red() } else { iron() })),
        );
        if near {
            for y in [2.2, 4.4] {
                lathe(
                    b,
                    foot + DVec3::Y * y,
                    DVec3::Y,
                    &[(0.0, 0.0), (0.66, 0.0), (0.66, 0.35), (0.0, 0.35)],
                    24,
                    |_| Some((Surf::Plain, signal_red())),
                );
            }
        }
    }
    // The sign's mast: a tapering fin up from the ridge, with stays.
    let mast_top = SIGN_UP - 1.4;
    lathe(
        b,
        v(cx, HALL_TOP - 0.2, 0.0),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.45, 0.0),
            (0.3, 1.0),
            (0.18, mast_top - HALL_TOP),
            (0.0, mast_top - HALL_TOP + 0.2),
        ],
        if near { 16 } else { 6 },
        |_| Some((Surf::Metal, chrome())),
    );
    if near {
        for z in [-1.0, 1.0] {
            strut(
                b,
                v(cx, mast_top - 2.0, 0.0),
                v(cx + 0.6, HALL_TOP - 0.3, z * 4.0),
                0.06,
                iron(),
            );
            strut(
                b,
                v(cx, mast_top - 2.0, 0.0),
                v(cx - 0.6, HALL_TOP - 0.3, z * 4.0),
                0.06,
                iron(),
            );
        }
        // A starburst on the mast, Googie fashion.
        with(b, Surf::Metal, |b| {
            for k in 0..8 {
                let a = TAU * k as f64 / 8.0;
                let d = v(a.cos() * 0.7, a.sin(), 0.0);
                bar(
                    b,
                    v(cx, HALL_TOP + 2.2, 0.0),
                    v(cx, HALL_TOP + 2.2, 0.0) + d * 1.1,
                    0.05,
                    6,
                    Surf::Metal,
                    hazard(),
                );
            }
        });
    }
}

/// The robot's head on the sign, in its own frame (its neck at the
/// origin, facing +z): a domed drum with glowing eyes, a grille mouth,
/// bolt ears and an antenna, ringed by three atom orbits.
pub fn sign_head(b: &mut MeshBuilder, near: bool) {
    let seg = if near { 32 } else { 10 };
    let (r, h) = (1.45, 2.1);
    lathe(
        b,
        DVec3::ZERO,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.55, 0.0),
            (0.55, 0.3),
            (r, 0.36),
            (r, 0.5),
            (r - 0.05, h - 0.2),
            (r * 0.85, h + 0.25),
            (r * 0.55, h + 0.55),
            (0.0, h + 0.68),
        ],
        seg,
        |k| {
            Some(match k {
                0..=2 => (Surf::Metal, chrome()),
                3 => (Surf::Plain, cream()),
                _ => (Surf::Plain, cream()),
            })
        },
    );
    // A red band round the brow.
    lathe(
        b,
        DVec3::Y * (h - 0.62),
        DVec3::Y,
        &[(0.0, 0.0), (r + 0.06, 0.0), (r + 0.06, 0.26), (0.0, 0.26)],
        seg,
        |_| Some((Surf::Plain, signal_red())),
    );
    if near {
        // Eyes: chrome bezels round glowing lenses.
        for x in [-0.55, 0.55] {
            let c = v(x, h - 1.0, 0.0);
            lathe(
                b,
                c + DVec3::Z * (r - 0.2),
                DVec3::Z,
                &[
                    (0.0, 0.0),
                    (0.42, 0.0),
                    (0.42, 0.28),
                    (0.34, 0.32),
                    (0.0, 0.32),
                ],
                24,
                |k| {
                    Some(if k == 3 {
                        (Surf::Glow, arc_green())
                    } else {
                        (Surf::Metal, chrome())
                    })
                },
            );
        }
        // The grille mouth.
        for k in 0..4 {
            let y = 0.75 + k as f64 * 0.13;
            bar(
                b,
                v(-0.6, y, r - 0.02),
                v(0.6, y, r - 0.02),
                0.04,
                8,
                Surf::Metal,
                chrome(),
            );
        }
        // Bolt ears.
        for s in [-1.0, 1.0] {
            lathe(
                b,
                v(s * (r - 0.1), h - 1.0, 0.0),
                DVec3::X * s,
                &[
                    (0.0, 0.0),
                    (0.38, 0.0),
                    (0.38, 0.22),
                    (0.2, 0.3),
                    (0.2, 0.55),
                    (0.0, 0.6),
                ],
                20,
                |_| Some((Surf::Plain, signal_red())),
            );
        }
    }
    // The antenna and its glowing ball.
    bar(
        b,
        v(0.0, h + 0.6, 0.0),
        v(0.0, h + 1.6, 0.0),
        0.05,
        8,
        Surf::Metal,
        chrome(),
    );
    ball(
        b,
        v(0.0, h + 1.75, 0.0),
        0.22,
        if near { 16 } else { 6 },
        Surf::Glow,
        arc_green(),
    );
    // Three atom orbits round it, each with its electron.
    let rings = if near { 64 } else { 16 };
    for k in 0..3 {
        let tilt = (k as f64 - 1.0) * 60f64.to_radians();
        let (st, ct) = tilt.sin_cos();
        let at = |a: f64| {
            let (x, y) = (a.cos() * 2.6, a.sin() * 0.9);
            v(x * ct, h * 0.55 + y, x * st)
        };
        let pts: Vec<DVec3> = (0..=rings)
            .map(|i| at(TAU * i as f64 / rings as f64))
            .collect();
        tube(
            b,
            &pts,
            (0.06, 0.06),
            if near { 8 } else { 4 },
            Some((Surf::Plain, signal_red())),
        );
        ball(
            b,
            at(1.2 + k as f64 * 2.1),
            0.18,
            if near { 12 } else { 6 },
            Surf::Glow,
            arc_green(),
        );
    }
}

/// The assembly line: the belt between its green side frames on legs, end
/// drums, stop markings on the floor, the ramp off its end, the racks, the
/// welding arms' pedestals and the test arch.
fn line(b: &mut MeshBuilder, near: bool) {
    let (x0, x1, z) = (LINE_FROM, LINE_TO, LINE_Z);
    let drum = 0.42;
    // The belt.
    cuboid(
        b,
        v(x0, BELT_TOP - 0.1, z - BELT_HW + 0.05),
        v(x1 - drum, BELT_TOP, z + BELT_HW - 0.05),
        rgb(34, 34, 36),
    );
    // Side frames, a chrome edge along their tops, skirts down to the
    // floor (the return run is out of sight behind them).
    for s in [-1.0, 1.0] {
        let zz = z + s * (BELT_HW + 0.06);
        b.oriented_box(
            v((x0 + x1) * 0.5, (BELT_TOP + 0.05 + 0.28) * 0.5, zz),
            DVec3::X,
            v((x1 - x0) * 0.5, (BELT_TOP + 0.05 - 0.28) * 0.5, 0.06),
            bottle(),
        );
        b.oriented_box(
            v((x0 + x1) * 0.5, 0.15, zz - s * 0.05),
            DVec3::X,
            v((x1 - x0) * 0.5, 0.15, 0.05),
            soot(),
        );
        if near {
            bar(
                b,
                v(x0, BELT_TOP + 0.06, zz),
                v(x1 - 0.1, BELT_TOP + 0.06, zz),
                0.06,
                8,
                Surf::Metal,
                chrome(),
            );
            // Roller ends along the frames.
            let n = ((x1 - x0) / 0.6) as usize;
            for k in 0..n {
                let x = x0 + 0.3 + k as f64 * 0.6;
                lathe(
                    b,
                    v(x, BELT_TOP - 0.18, zz),
                    DVec3::Z * s,
                    &[(0.0, 0.0), (0.07, 0.0), (0.07, 0.07), (0.0, 0.09)],
                    8,
                    |_| Some((Surf::Metal, chrome())),
                );
            }
        }
    }
    // Legs with foot plates.
    let legs = ((x1 - x0) / 2.2) as usize;
    for k in 0..=legs {
        let x = x0 + 0.4 + (x1 - x0 - 0.8) * k as f64 / legs as f64;
        for s in [-1.0, 1.0] {
            let zz = z + s * (BELT_HW - 0.1);
            cuboid(
                b,
                v(x - 0.07, 0.0, zz - 0.07),
                v(x + 0.07, BELT_TOP - 0.1, zz + 0.07),
                iron(),
            );
            if near {
                cuboid(
                    b,
                    v(x - 0.18, 0.0, zz - 0.18),
                    v(x + 0.18, 0.04, zz + 0.18),
                    iron(),
                );
            }
        }
    }
    // The end drum's round cover.
    lathe(
        b,
        v(x1 - drum, BELT_TOP - drum, z - BELT_HW - 0.12),
        DVec3::Z,
        &[
            (0.0, 0.0),
            (drum + 0.04, 0.0),
            (drum + 0.04, 2.0 * BELT_HW + 0.24),
            (0.0, 2.0 * BELT_HW + 0.24),
        ],
        if near { 24 } else { 8 },
        |_| Some((Surf::Plain, bottle())),
    );
    // The stops: hazard chevrons on the floor each side, and a post with a
    // signal lamp at each station.
    if near {
        for &x in &STOPS[1..] {
            for s in [-1.0, 1.0] {
                let zz = z + s * (BELT_HW + 0.45);
                for k in 0..4 {
                    let c = if k % 2 == 0 { hazard() } else { soot() };
                    let xa = x - 0.6 + k as f64 * 0.3;
                    b.quad(
                        [
                            v(xa, 0.012, zz - 0.25),
                            v(xa + 0.3, 0.012, zz - 0.25),
                            v(xa + 0.3, 0.012, zz + 0.25),
                            v(xa, 0.012, zz + 0.25),
                        ],
                        DVec3::Y,
                        c,
                    );
                }
            }
        }
    }
    // The ramp the finished robot walks down, with its side rails.
    let xr = STOPS[STOP_END];
    let (za, zb) = RAMP_Z;
    let top = PALLET_TOP - 0.05;
    let hw = RAMP_HW;
    b.hexa(
        [
            v(xr - hw, 0.0, zb),
            v(xr + hw, 0.0, zb),
            v(xr + hw, 0.0, za),
            v(xr - hw, 0.0, za),
        ],
        [
            v(xr - hw, 0.02, zb),
            v(xr + hw, 0.02, zb),
            v(xr + hw, top, za),
            v(xr - hw, top, za),
        ],
        iron(),
    );
    if near {
        for s in [-1.0, 1.0] {
            let x = xr + s * (hw + 0.05);
            bar(
                b,
                v(x, top + 0.9, za),
                v(x, 0.9, zb),
                0.04,
                8,
                Surf::Plain,
                hazard(),
            );
            for t in [0.0, 1.0] {
                let p = v(x, 0.0, za + (zb - za) * t);
                let y = top + (0.0 - top) * t;
                bar(
                    b,
                    p + DVec3::Y * y,
                    p + DVec3::Y * (y + 0.9),
                    0.04,
                    8,
                    Surf::Plain,
                    hazard(),
                );
            }
        }
    }
    // The parts racks: the dish rack a shelf, the gun rack a hanging bar.
    for (stop, hanging) in [(STOP_DISH, false), (STOP_GUN, true)] {
        let c = v(STOPS[stop] + RACK_STEP, 0.0, RACK_Z);
        for sx in [-1.0, 1.0] {
            for sz in [-1.0, 1.0] {
                let p = c + v(sx * 0.85, 0.0, sz * 0.5);
                cuboid(b, p - v(0.04, 0.0, 0.04), p + v(0.04, 1.65, 0.04), hazard());
            }
        }
        cuboid(b, c + v(-0.9, 1.6, -0.55), c + v(0.9, 1.68, 0.55), hazard());
        if hanging {
            bar(
                b,
                c + v(-0.88, 1.55, 0.0),
                c + v(0.88, 1.55, 0.0),
                0.05,
                8,
                Surf::Metal,
                chrome(),
            );
        } else {
            cuboid(b, c + v(-0.9, 0.42, -0.55), c + v(0.9, 0.5, 0.55), iron());
        }
        cuboid(b, c + v(-0.9, 0.0, -0.58), c + v(0.9, 1.6, -0.53), bottle());
    }
    // The welding arms' pedestals, with their cable runs to the line.
    for s in [-1.0, 1.0] {
        let p = v(STOPS[STOP_WELD], 0.0, s * WELDER_Z);
        lathe(
            b,
            p,
            DVec3::Y,
            &[
                (0.0, 0.0),
                (0.55, 0.0),
                (0.55, 0.12),
                (0.42, 0.2),
                (0.34, 1.2),
                (0.4, 1.3),
                (0.4, 1.4),
                (0.0, 1.4),
            ],
            if near { 24 } else { 8 },
            |k| Some((Surf::Plain, if k == 5 { soot() } else { hazard() })),
        );
        if near {
            tube(
                b,
                &[
                    p + v(0.3, 0.1, 0.0),
                    p + v(0.8, 0.06, -s * 0.5),
                    v(STOPS[STOP_WELD] + 1.0, 0.06, s * (BELT_HW + 0.2)),
                ],
                (0.07, 0.07),
                8,
                Some((Surf::Plain, soot())),
            );
        }
    }
    // The test arch: two legs and a sweep over the line, a fin on top, and
    // its control cabinet.
    let xt = STOPS[STOP_TEST];
    let arch: Vec<DVec3> = (0..=16)
        .map(|i| {
            let a = PI * i as f64 / 16.0;
            v(xt, ARCH_H - 0.6 + 0.6 * a.sin(), -ARCH_HW * a.cos())
        })
        .collect();
    let mut path = vec![v(xt, 0.0, -ARCH_HW)];
    path.extend(arch);
    path.push(v(xt, 0.0, ARCH_HW));
    tube(
        b,
        &path,
        (0.17, 0.17),
        if near { 12 } else { 6 },
        Some((Surf::Plain, cream())),
    );
    if near {
        b.hexa(
            [
                v(xt - 0.03, ARCH_H, -0.6),
                v(xt + 0.03, ARCH_H, -0.6),
                v(xt + 0.03, ARCH_H, 0.6),
                v(xt - 0.03, ARCH_H, 0.6),
            ],
            [
                v(xt - 0.03, ARCH_H + 1.1, 0.4),
                v(xt + 0.03, ARCH_H + 1.1, 0.4),
                v(xt + 0.03, ARCH_H + 1.1, 0.9),
                v(xt - 0.03, ARCH_H + 1.1, 0.9),
            ],
            signal_red(),
        );
        // Lamp housings (the lenses are live).
        for z in [-0.9, 0.0, 0.9] {
            lathe(
                b,
                v(xt, ARCH_H - 0.1, z),
                DVec3::Y,
                &[(0.0, 0.0), (0.22, 0.0), (0.24, 0.12), (0.0, 0.14)],
                16,
                |_| Some((Surf::Metal, chrome())),
            );
        }
        let cab = v(xt + 1.2, 0.0, -ARCH_HW - 0.9);
        cuboid(
            b,
            cab + v(-0.45, 0.0, -0.3),
            cab + v(0.45, 1.5, 0.3),
            bottle(),
        );
        with(b, Surf::Glow, |b| {
            for (k, x) in [-0.25, 0.0, 0.25].into_iter().enumerate() {
                let col = [arc_green(), hazard(), signal_red()][k];
                ball(b, cab + v(x, 1.25, 0.31), 0.06, 8, Surf::Glow, col);
            }
        });
        for x in [-0.2, 0.2] {
            lathe(
                b,
                cab + v(x, 0.8, 0.3),
                DVec3::Z,
                &[(0.0, 0.0), (0.14, 0.0), (0.14, 0.04), (0.0, 0.05)],
                16,
                |_| Some((Surf::Plain, cream())),
            );
        }
    }
}

/// The crane runway: I-beam columns, knee braces and the two rail girders.
fn runway(b: &mut MeshBuilder, near: bool) {
    let top = RUNWAY_Y - 0.7;
    for &x in &RUNWAY_COLUMNS {
        for s in [-1.0, 1.0] {
            let z = s * RUNWAY_Z;
            // Web and flanges.
            cuboid(
                b,
                v(x - 0.04, 0.0, z - 0.16),
                v(x + 0.04, top, z + 0.16),
                hazard(),
            );
            if near {
                for f in [-1.0, 1.0] {
                    cuboid(
                        b,
                        v(x + f * 0.16 - 0.03, 0.0, z - 0.16),
                        v(x + f * 0.16 + 0.03, top, z + 0.16),
                        hazard(),
                    );
                }
                cuboid(
                    b,
                    v(x - 0.32, 0.0, z - 0.32),
                    v(x + 0.32, 0.05, z + 0.32),
                    iron(),
                );
                striped(b, v(x, 0.05, z), v(x, 1.8, z), 0.36, 6);
                for d in [-1.0, 1.0] {
                    if (x + d * 1.4) < RUNWAY_COLUMNS[0] - 0.1
                        || (x + d * 1.4) > RUNWAY_COLUMNS[3] + 0.1
                    {
                        continue;
                    }
                    strut(
                        b,
                        v(x, top - 1.6, z),
                        v(x + d * 1.4, top - 0.02, z),
                        0.12,
                        hazard(),
                    );
                }
            }
        }
    }
    let (x0, x1) = (RUNWAY_COLUMNS[0] - 0.3, RUNWAY_COLUMNS[3] + 0.3);
    for s in [-1.0, 1.0] {
        let z = s * RUNWAY_Z;
        cuboid(
            b,
            v(x0, top, z - 0.3),
            v(x1, RUNWAY_Y - 0.08, z + 0.3),
            hazard(),
        );
        cuboid(
            b,
            v(x0, RUNWAY_Y - 0.08, z - 0.06),
            v(x1, RUNWAY_Y, z + 0.06),
            iron(),
        );
        if near {
            for (a, c) in [(x0, x0 + 1.6), (x1 - 1.6, x1)] {
                striped(
                    b,
                    v(a, top + 0.31, z + s * 0.31),
                    v(c, top + 0.31, z + s * 0.31),
                    0.6,
                    6,
                );
            }
        }
    }
}

/// The dock: four braced towers, girders round their tops, a bridge crane
/// over the tank, a cradle under it and a work deck along each side, with
/// floodlights on the corners.
fn dock(b: &mut MeshBuilder, near: bool) {
    let d = DOCK_AT;
    let c = v(d.x, 0.0, d.y);
    let leg = |sx: f64, sz: f64| c + v(sx * DOCK_HX, 0.0, sz * DOCK_HZ);
    let top = DOCK_TOP;
    for sx in [-1.0, 1.0] {
        for sz in [-1.0, 1.0] {
            let p = leg(sx, sz);
            if near {
                // A lattice tower: four posts braced in bays.
                let corner = |k: usize, y: f64| {
                    let a = TAU * (k as f64 + 0.5) / 4.0;
                    p + v(a.sin() * 0.42, y, a.cos() * 0.42)
                };
                for k in 0..4 {
                    strut(b, corner(k, 0.0), corner(k, top), 0.12, oxide());
                }
                let bays = 7;
                for i in 0..bays {
                    let (y0, y1) = (
                        top * i as f64 / bays as f64,
                        top * (i + 1) as f64 / bays as f64,
                    );
                    for k in 0..4 {
                        let j = (k + 1) % 4;
                        strut(b, corner(k, y1), corner(j, y1), 0.08, oxide());
                        strut(b, corner(k, y0), corner(j, y1), 0.05, oxide());
                    }
                }
                cuboid(b, p - v(0.6, 0.0, 0.6), p + v(0.6, 0.3, 0.6), concrete());
            } else {
                cuboid(b, p - v(0.4, 0.0, 0.4), p + v(0.4, top, 0.4), oxide());
            }
        }
    }
    // Girders round the top.
    for sz in [-1.0, 1.0] {
        let z = d.y + sz * DOCK_HZ;
        cuboid(
            b,
            v(d.x - DOCK_HX - 0.5, top, z - 0.35),
            v(d.x + DOCK_HX + 0.5, top + 0.9, z + 0.35),
            oxide(),
        );
    }
    for sx in [-1.0, 1.0] {
        let x = d.x + sx * DOCK_HX;
        cuboid(
            b,
            v(x - 0.35, top, d.y - DOCK_HZ - 0.35),
            v(x + 0.35, top + 0.9, d.y + DOCK_HZ + 0.35),
            oxide(),
        );
    }
    // The bridge crane across the dock, its hook block hanging over the
    // turret on four cables.
    cuboid(
        b,
        v(d.x - 0.5, top + 0.9, d.y - DOCK_HZ - 0.4),
        v(d.x + 0.5, top + 1.8, d.y + DOCK_HZ + 0.4),
        hazard(),
    );
    if near {
        striped(
            b,
            v(d.x, top + 1.35, d.y - DOCK_HZ - 0.45),
            v(d.x, top + 1.35, d.y - DOCK_HZ + 1.2),
            0.92,
            4,
        );
        striped(
            b,
            v(d.x, top + 1.35, d.y + DOCK_HZ - 1.2),
            v(d.x, top + 1.35, d.y + DOCK_HZ + 0.45),
            0.92,
            4,
        );
        cuboid(
            b,
            v(d.x - 1.0, top + 1.8, d.y - 0.9),
            v(d.x + 1.0, top + 2.5, d.y + 0.9),
            iron(),
        );
        lathe(
            b,
            v(d.x - 0.9, top + 2.15, d.y),
            DVec3::X,
            &[(0.0, 0.0), (0.4, 0.0), (0.4, 1.8), (0.0, 1.8)],
            16,
            |_| Some((Surf::Metal, chrome())),
        );
        let hook_y = DOCK_CRADLE + TURRET_TOP + 1.6;
        for (x, z) in [(-0.5, -0.4), (0.5, -0.4), (-0.5, 0.4), (0.5, 0.4)] {
            bar(
                b,
                v(d.x + x, top + 0.9, d.y + z),
                v(d.x + x * 0.6, hook_y + 0.8, d.y + z * 0.6),
                0.03,
                6,
                Surf::Metal,
                iron(),
            );
        }
        cuboid(
            b,
            v(d.x - 0.5, hook_y, d.y - 0.4),
            v(d.x + 0.5, hook_y + 0.9, d.y + 0.4),
            hazard(),
        );
        lathe(
            b,
            v(d.x, hook_y - 0.9, d.y),
            DVec3::Y,
            &[(0.0, 0.0), (0.12, 0.0), (0.3, 0.4), (0.18, 0.9), (0.0, 0.9)],
            12,
            |_| Some((Surf::Metal, iron())),
        );
    }
    // Chains from the frame's top down to each raised knee.
    if near {
        for k in 0..4 {
            let kn = dock_knee(k);
            let z = d.y + DOCK_HZ * (kn.z - d.y).signum();
            for dx in [-0.35, 0.35] {
                bar(
                    b,
                    kn + v(dx, 0.7, 0.0),
                    v(kn.x + dx * 1.6, top, z),
                    0.04,
                    6,
                    Surf::Metal,
                    iron(),
                );
            }
        }
    }
    // The cradle: a concrete block and three steel saddles.
    cuboid(
        b,
        c + v(-3.2, 0.0, -3.2),
        c + v(3.2, DOCK_CRADLE - 1.0, 3.2),
        concrete(),
    );
    for z in [-2.4, 0.0, 2.4] {
        cuboid(
            b,
            c + v(-2.8, DOCK_CRADLE - 1.0, z - 0.3),
            c + v(2.8, DOCK_CRADLE - 0.1, z + 0.3),
            oxide(),
        );
    }
    if near {
        striped(
            b,
            c + v(-3.25, 0.25, 3.25),
            c + v(3.25, 0.25, 3.25),
            0.5,
            10,
        );
    }
    // The work decks, on posts, with railings and a ladder each.
    for s in [-1.0, 1.0] {
        let x = d.x + s * DOCK_DECK_X;
        cuboid(
            b,
            v(x - 0.6, DOCK_DECK_Y - 0.25, d.y - DOCK_DECK_HL),
            v(x + 0.6, DOCK_DECK_Y, d.y + DOCK_DECK_HL),
            hazard(),
        );
        for z in [-1.0, 0.0, 1.0] {
            let p = v(x, 0.0, d.y + z * (DOCK_DECK_HL - 0.3));
            bar(
                b,
                p,
                p + DVec3::Y * (DOCK_DECK_Y - 0.25),
                0.11,
                8,
                Surf::Plain,
                oxide(),
            );
        }
        if near {
            let xo = x + s * 0.55;
            for y in [0.55, 1.05] {
                bar(
                    b,
                    v(xo, DOCK_DECK_Y + y, d.y - DOCK_DECK_HL),
                    v(xo, DOCK_DECK_Y + y, d.y + DOCK_DECK_HL),
                    0.035,
                    6,
                    Surf::Plain,
                    hazard(),
                );
            }
            for k in 0..=6 {
                let z = d.y - DOCK_DECK_HL + 2.0 * DOCK_DECK_HL * k as f64 / 6.0;
                bar(
                    b,
                    v(xo, DOCK_DECK_Y, z),
                    v(xo, DOCK_DECK_Y + 1.05, z),
                    0.035,
                    6,
                    Surf::Plain,
                    hazard(),
                );
            }
            // A ladder up the back end.
            let z = d.y - DOCK_DECK_HL - 0.2;
            for dx in [-0.25, 0.25] {
                bar(
                    b,
                    v(x + dx, 0.0, z),
                    v(x + dx, DOCK_DECK_Y + 0.9, z),
                    0.03,
                    6,
                    Surf::Metal,
                    chrome(),
                );
            }
            let rungs = (DOCK_DECK_Y / 0.3) as usize;
            for k in 1..rungs {
                let y = k as f64 * 0.3;
                bar(
                    b,
                    v(x - 0.25, y, z),
                    v(x + 0.25, y, z),
                    0.02,
                    6,
                    Surf::Metal,
                    chrome(),
                );
            }
            // A welding set on the deck.
            let w = v(x, DOCK_DECK_Y, d.y + 1.2);
            cuboid(b, w + v(-0.3, 0.0, -0.25), w + v(0.3, 0.7, 0.25), bottle());
            lathe(
                b,
                w + v(0.0, 0.0, -0.6),
                DVec3::Y,
                &[
                    (0.0, 0.0),
                    (0.16, 0.0),
                    (0.16, 1.1),
                    (0.08, 1.25),
                    (0.0, 1.25),
                ],
                12,
                |_| Some((Surf::Plain, signal_red())),
            );
        }
    }
    // Floodlights on the towers, aimed in.
    if near {
        for sx in [-1.0, 1.0] {
            for sz in [-1.0, 1.0] {
                let p = leg(sx, sz) + DVec3::Y * (top + 1.3);
                let into = (c + DVec3::Y * 6.0 - p).normalize();
                lathe(
                    b,
                    p,
                    into,
                    &[
                        (0.0, -0.3),
                        (0.2, -0.3),
                        (0.35, 0.05),
                        (0.35, 0.12),
                        (0.0, 0.12),
                    ],
                    16,
                    |k| {
                        Some(if k == 3 {
                            (Surf::Glow, rgb(255, 245, 225))
                        } else {
                            (Surf::Metal, iron())
                        })
                    },
                );
                bar(
                    b,
                    p - DVec3::Y * 0.3,
                    p - DVec3::Y * 1.2,
                    0.05,
                    6,
                    Surf::Metal,
                    iron(),
                );
            }
        }
    }
}

/// The foreman's booth: a flared glass cab on three legs, a saucer roof
/// with an aerial, and a ladder up.
fn booth(b: &mut MeshBuilder, near: bool) {
    let c = v(BOOTH_AT.x, 0.0, BOOTH_AT.y);
    let (r, f, top) = (BOOTH_R, BOOTH_FLOOR, BOOTH_ROOF);
    for k in 0..3 {
        let a = TAU * (k as f64 + 0.5) / 3.0;
        let p = c + v(a.cos(), 0.0, a.sin()) * (r * 0.7);
        bar(
            b,
            p,
            p + DVec3::Y * f,
            0.18,
            if near { 12 } else { 6 },
            Surf::Plain,
            cream(),
        );
    }
    let seg = if near { 32 } else { 10 };
    // The floor pan, the glass, the roof.
    lathe(
        b,
        c + DVec3::Y * (f - 0.6),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.6, 0.0),
            (r * 0.9, 0.45),
            (r, 0.6),
            (r, 0.75),
            (0.0, 0.75),
        ],
        seg,
        |_| Some((Surf::Plain, bottle())),
    );
    lathe(
        b,
        c + DVec3::Y * (f + 0.15),
        DVec3::Y,
        &[(r - 0.05, 0.0), (r + 0.35, top - f - 0.3)],
        seg,
        |_| Some((Surf::Glass, rgb(150, 190, 180))),
    );
    lathe(
        b,
        c + DVec3::Y * (top - 0.15),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (r + 0.8, 0.0),
            (r + 0.85, 0.12),
            (r * 0.6, 0.55),
            (0.0, 0.7),
        ],
        seg,
        |k| Some((Surf::Plain, if k == 1 { signal_red() } else { cream() })),
    );
    if near {
        // Mullions, a console inside, an aerial with a ball.
        for k in 0..8 {
            let a = TAU * k as f64 / 8.0;
            let d = v(a.cos(), 0.0, a.sin());
            strut(
                b,
                c + d * (r + 0.02) + DVec3::Y * (f + 0.15),
                c + d * (r + 0.37) + DVec3::Y * (top - 0.15),
                0.07,
                chrome(),
            );
        }
        cuboid(
            b,
            c + v(-1.0, f + 0.15, -0.9),
            c + v(1.0, f + 1.0, -0.4),
            bottle(),
        );
        with(b, Surf::Glow, |b| {
            ball(
                b,
                c + v(-0.4, f + 1.05, -0.65),
                0.07,
                8,
                Surf::Glow,
                arc_green(),
            );
            ball(
                b,
                c + v(0.3, f + 1.05, -0.65),
                0.07,
                8,
                Surf::Glow,
                signal_red(),
            );
        });
        bar(
            b,
            c + DVec3::Y * (top + 0.5),
            c + DVec3::Y * (top + 2.4),
            0.04,
            6,
            Surf::Metal,
            chrome(),
        );
        ball(
            b,
            c + DVec3::Y * (top + 2.5),
            0.15,
            10,
            Surf::Metal,
            chrome(),
        );
        // The ladder, up the side away from the dock.
        let lx = c.x + r * 0.95;
        for dz in [-0.25, 0.25] {
            bar(
                b,
                v(lx, 0.0, c.z + dz),
                v(lx, f, c.z + dz),
                0.03,
                6,
                Surf::Metal,
                chrome(),
            );
        }
        for k in 1..(f / 0.3) as usize {
            let y = k as f64 * 0.3;
            bar(
                b,
                v(lx, y, c.z - 0.25),
                v(lx, y, c.z + 0.25),
                0.02,
                6,
                Surf::Metal,
                chrome(),
            );
        }
    }
}

/// The dispatch shed: a corrugated barrel-roofed hut, its dark doorway
/// towards the line, a beacon on its roof (live) and a sign arrow.
fn shed(b: &mut MeshBuilder, near: bool) {
    let c = v(SHED_AT.x, 0.0, SHED_AT.y);
    let (hx, hz, h) = (SHED_HALF.0, SHED_HALF.1, SHED_H);
    let wall = h - hx * 0.55;
    // Side walls and a barrel roof as one smooth shell, front to back.
    let n = if near { 14 } else { 5 };
    let section: Vec<(f64, f64)> = {
        let mut s = vec![(-hx, 0.0), (-hx, wall)];
        for i in 1..n {
            let a = PI * i as f64 / n as f64;
            s.push((-hx * a.cos(), wall + (h - wall) * a.sin()));
        }
        s.push((hx, wall));
        s.push((hx, 0.0));
        s
    };
    let rows: Vec<Vec<DVec3>> = [-hz, hz]
        .iter()
        .map(|&z| section.iter().map(|&(x, y)| c + v(x, y, z)).collect())
        .collect();
    crate::carmesh::surface(b, &rows, false, |_, j| {
        let ribbed = near && j % 2 == 0;
        Some((
            Surf::Plain,
            if ribbed {
                rgb(120, 126, 120)
            } else {
                rgb(132, 138, 130)
            },
        ))
    });
    // The ends: the back closed, the front a frame round a dark doorway.
    let end = |z: f64| -> Vec<DVec3> { section.iter().map(|&(x, y)| c + v(x, y, z)).collect() };
    b.polygon(&end(-hz), -DVec3::Z, rgb(120, 126, 120));
    let door_hw = 1.4;
    let door_h = 2.9;
    let front: Vec<DVec3> = end(hz);
    // Front wall round the door: two side panels and a gable above.
    b.quad(
        [
            c + v(-hx, 0.0, hz),
            c + v(-door_hw, 0.0, hz),
            c + v(-door_hw, wall, hz),
            c + v(-hx, wall, hz),
        ],
        DVec3::Z,
        cream(),
    );
    b.quad(
        [
            c + v(door_hw, 0.0, hz),
            c + v(hx, 0.0, hz),
            c + v(hx, wall, hz),
            c + v(door_hw, wall, hz),
        ],
        DVec3::Z,
        cream(),
    );
    b.quad(
        [
            c + v(-door_hw, door_h, hz),
            c + v(door_hw, door_h, hz),
            c + v(door_hw, wall, hz),
            c + v(-door_hw, wall, hz),
        ],
        DVec3::Z,
        cream(),
    );
    let gable: Vec<DVec3> = front
        .iter()
        .skip(1)
        .take(front.len() - 2)
        .copied()
        .collect();
    b.polygon(&gable, DVec3::Z, cream());
    // The dark way in.
    let dark = rgb(14, 14, 14);
    let (z0, z1) = (hz - 3.5, hz + 0.01);
    b.quad(
        [
            c + v(-door_hw, 0.0, z0),
            c + v(-door_hw, door_h, z0),
            c + v(door_hw, door_h, z0),
            c + v(door_hw, 0.0, z0),
        ],
        DVec3::Z,
        dark,
    );
    b.quad(
        [
            c + v(-door_hw, 0.0, z0),
            c + v(-door_hw, 0.0, z1),
            c + v(-door_hw, door_h, z1),
            c + v(-door_hw, door_h, z0),
        ],
        DVec3::X,
        dark,
    );
    b.quad(
        [
            c + v(door_hw, 0.0, z0),
            c + v(door_hw, door_h, z0),
            c + v(door_hw, door_h, z1),
            c + v(door_hw, 0.0, z1),
        ],
        -DVec3::X,
        dark,
    );
    b.quad(
        [
            c + v(-door_hw, door_h, z0),
            c + v(-door_hw, door_h, z1),
            c + v(door_hw, door_h, z1),
            c + v(door_hw, door_h, z0),
        ],
        -DVec3::Y,
        dark,
    );
    b.quad(
        [
            c + v(-door_hw, 0.01, z0),
            c + v(door_hw, 0.01, z0),
            c + v(door_hw, 0.01, z1),
            c + v(-door_hw, 0.01, z1),
        ],
        DVec3::Y,
        rgb(36, 36, 34),
    );
    let fz = hz + 0.08;
    striped(
        b,
        c + v(-door_hw - 0.12, 0.0, fz),
        c + v(-door_hw - 0.12, door_h + 0.12, fz),
        0.24,
        if near { 6 } else { 1 },
    );
    striped(
        b,
        c + v(door_hw + 0.12, 0.0, fz),
        c + v(door_hw + 0.12, door_h + 0.12, fz),
        0.24,
        if near { 6 } else { 1 },
    );
    striped(
        b,
        c + v(-door_hw - 0.24, door_h + 0.12, fz),
        c + v(door_hw + 0.24, door_h + 0.12, fz),
        0.24,
        if near { 7 } else { 1 },
    );
    if near {
        // Ribs over the roof.
        for k in 0..=4 {
            let z = -hz + 2.0 * hz * k as f64 / 4.0;
            let pts: Vec<DVec3> = section
                .iter()
                .map(|&(x, y)| c + v(x * 1.02, y + 0.03, z))
                .collect();
            tube(b, &pts, (0.05, 0.05), 6, Some((Surf::Metal, iron())));
        }
        // A red arrow over the door, pointing in, with green bulbs.
        let a = c + v(0.0, door_h + 0.75, fz + 0.02);
        b.polygon(
            &[
                a + v(-1.1, 0.18, 0.0),
                a + v(0.6, 0.18, 0.0),
                a + v(0.6, 0.38, 0.0),
                a + v(1.1, 0.0, 0.0),
                a + v(0.6, -0.38, 0.0),
                a + v(0.6, -0.18, 0.0),
                a + v(-1.1, -0.18, 0.0),
            ],
            DVec3::Z,
            signal_red(),
        );
        with(b, Surf::Glow, |b| {
            for k in 0..5 {
                ball(
                    b,
                    a + v(-0.9 + k as f64 * 0.38, 0.0, 0.04),
                    0.05,
                    6,
                    Surf::Glow,
                    arc_green(),
                );
            }
        });
        // The beacon's base on the ridge (its lens is live).
        lathe(
            b,
            c + v(0.0, h, hz - 1.0),
            DVec3::Y,
            &[(0.0, 0.0), (0.25, 0.0), (0.25, 0.12), (0.0, 0.14)],
            12,
            |_| Some((Surf::Metal, iron())),
        );
    }
}

/// The gate: two tapering fin pylons and a boomerang arch between them,
/// the robot's head emblem at its middle.
fn gate(b: &mut MeshBuilder, near: bool) {
    for s in [-1.0, 1.0] {
        let x = s * GATE_X;
        let foot = v(x, 0.0, GATE_Z);
        // A fin: thick at the foot, swept back as it rises.
        let n = if near { 10 } else { 3 };
        let rows: Vec<Vec<DVec3>> = (0..=n)
            .map(|i| {
                let t = i as f64 / n as f64;
                let y = GATE_H * t;
                let half = 0.7 * (1.0 - 0.6 * t);
                let back = -1.2 * t * t;
                let seg = if near { 16 } else { 6 };
                (0..seg)
                    .map(|k| {
                        let a = TAU * k as f64 / seg as f64;
                        foot + v(a.cos() * half * 0.55, y, back + a.sin() * half)
                    })
                    .collect()
            })
            .collect();
        crate::carmesh::surface(b, &rows, true, |i, _| {
            Some((Surf::Plain, if i == n - 2 { signal_red() } else { cream() }))
        });
        ball(
            b,
            foot + v(0.0, GATE_H + 0.1, -1.2),
            0.32,
            if near { 16 } else { 6 },
            Surf::Glow,
            arc_green(),
        );
        cuboid(
            b,
            foot + v(-0.6, 0.0, -0.9),
            foot + v(0.6, 0.35, 0.9),
            concrete(),
        );
    }
    // The arch, sagging a little in the middle like a boomerang.
    let pts: Vec<DVec3> = (0..=12)
        .map(|i| {
            let t = i as f64 / 12.0;
            let x = -GATE_X + 2.0 * GATE_X * t;
            v(
                x,
                GATE_H - 1.6 - 0.5 * (PI * t).sin(),
                GATE_Z - 0.6 - 0.3 * (PI * t).sin(),
            )
        })
        .collect();
    tube(
        b,
        &pts,
        (0.18, 0.3),
        if near { 12 } else { 6 },
        Some((Surf::Plain, signal_red())),
    );
    // The emblem: a round plate with the robot's face.
    let e = v(0.0, GATE_H - 2.2, GATE_Z - 0.85);
    lathe(
        b,
        e - DVec3::Z * 0.15,
        DVec3::Z,
        &[
            (0.0, 0.0),
            (1.05, 0.0),
            (1.1, 0.1),
            (1.05, 0.3),
            (0.0, 0.32),
        ],
        if near { 32 } else { 8 },
        |k| Some((Surf::Plain, if k == 2 { chrome() } else { cream() })),
    );
    if near {
        for x in [-0.38, 0.38] {
            lathe(
                b,
                e + v(x, 0.18, 0.15),
                DVec3::Z,
                &[(0.0, 0.0), (0.2, 0.0), (0.2, 0.08), (0.0, 0.1)],
                16,
                |_| Some((Surf::Glow, arc_green())),
            );
        }
        for k in 0..3 {
            let y = -0.25 - k as f64 * 0.12;
            bar(
                b,
                e + v(-0.4, y, 0.2),
                e + v(0.4, y, 0.2),
                0.035,
                6,
                Surf::Metal,
                chrome(),
            );
        }
        bar(
            b,
            e + v(0.0, 1.05, 0.0),
            e + v(0.0, 1.5, 0.0),
            0.04,
            6,
            Surf::Metal,
            chrome(),
        );
        ball(b, e + v(0.0, 1.6, 0.0), 0.1, 8, Surf::Glow, arc_green());
    }
}

// ------------------------------------------------------------ the clock

/// Smooth start and stop for a move from 0 to 1.
fn ease(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// What a pallet carries and where it is at a moment, by its serial.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pallet {
    pub serial: i64,
    /// Along the line (x), and its tip over the end drum (0..1).
    pub x: f64,
    pub tip: f64,
    /// The stop it is at or moving to.
    pub stop: usize,
}

/// A robot's state at a moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Robot {
    pub serial: i64,
    /// Its feet, in the works' frame, and the way it faces (radians from
    /// +z towards +x).
    pub at: DVec3,
    pub yaw: f64,
    pub dish: bool,
    pub gun: bool,
    /// Test: the dish's turn (radians) and the gun's raise (0..1).
    pub dish_turn: f64,
    pub raise: f64,
    /// Walking: the gait's phase (radians) and how much it strides (0..1).
    pub phase: f64,
    pub stride: f64,
}

/// A crane's magnet, and what hangs on it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crane {
    /// The bridge's x, the trolley's z and the magnet's foot.
    pub bridge: f64,
    pub hook: DVec3,
    pub carrying: bool,
    /// Is the front part on its rack (not yet taken this cycle)?
    pub on_rack: bool,
}

/// A welding arm's torch tip and whether it burns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Torch {
    pub tip: DVec3,
    pub burning: bool,
}

/// The line's state at time `t`, all of it: a pure function of the clock.
pub struct Line {
    pub cycle: i64,
    /// Seconds into the cycle (moving first, then standing).
    pub tau: f64,
    pub pallets: Vec<Pallet>,
    pub robots: Vec<Robot>,
    pub cranes: [Crane; 2],
    pub torches: [Torch; 2],
    /// The test lamp: 0 off, 1 amber, 2 green.
    pub lamp: u8,
}

/// When in the standing time a crane lets go of its part.
const RELEASE: f64 = 7.4;
/// The test: when the lamp goes amber and green.
const TEST_AMBER: f64 = 0.5;
const TEST_GREEN: f64 = 6.5;
/// When the finished robot starts off the end of the line.
const STEP_OFF: f64 = 0.4;

/// The march from the end of the line into the shed, as (point, its
/// height over the plinth) knots in the works' frame.
fn march_path() -> Vec<DVec3> {
    let x = STOPS[STOP_END];
    let door = SHED_AT.y + SHED_HALF.1;
    vec![
        v(x, PALLET_TOP, LINE_Z),
        v(x, PALLET_TOP, RAMP_Z.0 + 0.1),
        v(x, 0.0, RAMP_Z.1 - 0.2),
        v(SHED_AT.x + 0.6, 0.0, door + 2.5),
        v(SHED_AT.x, 0.0, door + 0.4),
        v(SHED_AT.x, 0.0, SHED_AT.y + 0.5),
    ]
}

fn path_length(p: &[DVec3]) -> f64 {
    p.windows(2).map(|w| flat(w[1] - w[0]).length()).sum()
}

fn flat(d: DVec3) -> DVec2 {
    DVec2::new(d.x, d.z)
}

/// Where along `path` (point, heading) after `s` metres on the flat.
fn along(path: &[DVec3], s: f64) -> (DVec3, f64) {
    let mut left = s.max(0.0);
    for w in path.windows(2) {
        let d = flat(w[1] - w[0]);
        let l = d.length();
        if left <= l || std::ptr::eq(w, path.windows(2).last().unwrap_or(w)) {
            let t = if l > 0.0 { (left / l).min(1.0) } else { 1.0 };
            return (w[0].lerp(w[1], t), d.x.atan2(d.y));
        }
        left -= l;
    }
    let n = path.len();
    let d = flat(path[n - 1] - path[n - 2]);
    (path[n - 1], d.x.atan2(d.y))
}

/// How long a robot marches before it is through the shed's door.
fn march_time() -> f64 {
    path_length(&march_path()) / MARCH
}

impl Line {
    /// The line at `t` seconds of its clock.
    pub fn at(t: f64) -> Line {
        let cycle = (t / CYCLE).floor() as i64;
        let tau = t - cycle as f64 * CYCLE;
        let moving = tau < MOVE;
        let u = ease(tau / MOVE);
        let dwell = tau - MOVE;
        let mut pallets = Vec::new();
        let mut robots = Vec::new();
        // A pallet with serial n is at stop k - n in cycle k.
        for s in 0..=STOPS.len() {
            let serial = cycle - s as i64;
            let (x, tip) = if s == STOPS.len() {
                // Off the end: over the drum, then gone.
                if !moving {
                    continue;
                }
                (
                    STOPS[s - 1] + (LINE_TO - 0.3 - STOPS[s - 1]) * u.min(0.6) / 0.6,
                    ((u - 0.5) / 0.5).clamp(0.0, 1.0),
                )
            } else if moving {
                let from = if s == 0 { STOPS[0] - 4.0 } else { STOPS[s - 1] };
                (from + (STOPS[s] - from) * u, 0.0)
            } else {
                (STOPS[s], 0.0)
            };
            pallets.push(Pallet {
                serial,
                x,
                tip,
                stop: s,
            });
            if s == STOPS.len() {
                continue;
            }
            // Its robot, while it rides.
            let on_end = s == STOP_END && !moving && dwell >= STEP_OFF;
            if on_end {
                continue;
            }
            let fitted = |stop: usize| s > stop || (s == stop && !moving && dwell >= RELEASE);
            let testing = s == STOP_TEST && !moving;
            let (dish_turn, raise) = if testing {
                let turn = ease((dwell - 1.0) / 3.0) * TAU;
                let raise = ((dwell - 3.0) / 0.8).clamp(0.0, 1.0)
                    * (1.0 - ((dwell - 5.0) / 0.8).clamp(0.0, 1.0));
                (turn, raise)
            } else {
                (0.0, 0.0)
            };
            robots.push(Robot {
                serial,
                at: v(x, PALLET_TOP, LINE_Z),
                yaw: 0.0,
                dish: fitted(STOP_DISH),
                gun: fitted(STOP_GUN),
                dish_turn,
                raise,
                phase: 0.0,
                stride: 0.0,
            });
        }
        // Robots marching off: one leaves the end each cycle.
        let walk = march_time();
        let back = ((walk + STEP_OFF + 1.5) / CYCLE).ceil() as i64 + 1;
        let path = march_path();
        for k in 0..=back {
            let c = cycle - k;
            let left_at = c as f64 * CYCLE + MOVE + STEP_OFF;
            let w = t - left_at;
            if w < 0.0 {
                continue;
            }
            // First a turn on the spot towards the ramp (0.8 s), then the
            // march.
            let turn = (w / 0.8).min(1.0);
            let s = ((w - 0.8) * MARCH).max(0.0);
            if s >= path_length(&path) {
                continue;
            }
            let (at, heading) = along(&path, s);
            let yaw = if w < 0.8 {
                heading * ease(turn)
            } else {
                heading
            };
            robots.push(Robot {
                serial: c - STOP_END as i64,
                at,
                yaw,
                dish: true,
                gun: true,
                dish_turn: 0.0,
                raise: 0.0,
                phase: (w * MARCH / 1.5) * TAU,
                stride: if w < 0.8 { 0.4 } else { (s / 0.6).min(1.0) },
            });
        }
        let crane = |stop: usize, part_top: f64, rack_top: f64| -> Crane {
            let sx = STOPS[stop];
            let rx = sx + RACK_STEP + 0.45;
            let up = HOOK_UP;
            let line_up = v(sx, up, LINE_Z);
            let line_down = v(sx, part_top, LINE_Z);
            let rack_up = v(rx, up, RACK_Z);
            let rack_down = v(rx, rack_top, RACK_Z);
            let mv = |a: DVec3, b: DVec3, t0: f64, t1: f64, t: f64| {
                a.lerp(b, ease((t - t0) / (t1 - t0)))
            };
            let (hook, carrying, on_rack) = if moving {
                (mv(line_up, rack_up, 0.0, MOVE, tau), false, true)
            } else if dwell < 1.6 {
                (mv(rack_up, rack_down, 0.0, 1.6, dwell), false, true)
            } else if dwell < 2.0 {
                (rack_down, true, false)
            } else if dwell < 3.6 {
                (mv(rack_down, rack_up, 2.0, 3.6, dwell), true, false)
            } else if dwell < 5.6 {
                (mv(rack_up, line_up, 3.6, 5.6, dwell), true, false)
            } else if dwell < RELEASE {
                (mv(line_up, line_down, 5.6, RELEASE, dwell), true, false)
            } else if dwell < 7.9 {
                (line_down, false, false)
            } else {
                (mv(line_down, line_up, 7.9, DWELL, dwell), false, false)
            };
            Crane {
                bridge: hook.x,
                hook,
                carrying,
                on_rack,
            }
        };
        let body = PALLET_TOP + HIP as f64;
        let cranes = [
            crane(
                STOP_DISH,
                body + DISH_AT.1 + DISH_CARRY.1,
                DISH_SHELF + DISH_CARRY.1,
            ),
            crane(
                STOP_GUN,
                body + GUN_AT.1 + GUN_CARRY.1,
                GUN_BAR + GUN_CARRY.1,
            ),
        ];
        // Welding: the torches follow a seam up and down the body's side,
        // facing their arm.
        let torch = |side: f64| -> Torch {
            let rest = v(STOPS[STOP_WELD], 2.4, side * (WELDER_Z - 0.6));
            if moving || !(1.0..8.6).contains(&dwell) {
                let k = if moving {
                    0.0
                } else if dwell < 1.0 {
                    dwell
                } else {
                    1.0 - (dwell - 8.6) / (DWELL - 8.6)
                };
                let seam = v(STOPS[STOP_WELD] - 0.3, body + 0.9, side * 0.62);
                return Torch {
                    tip: rest.lerp(seam, ease(k)),
                    burning: false,
                };
            }
            let w = dwell - 1.0;
            let up = 0.5 + 0.5 * (w * 0.9 + if side > 0.0 { 0.0 } else { 1.7 }).sin();
            let across = 0.3 * (w * 0.45 + side).sin();
            Torch {
                tip: v(
                    STOPS[STOP_WELD] + across,
                    body + 0.5 + up * 1.1,
                    side * 0.62,
                ),
                // It burns in bursts, a breath between seams.
                burning: (w * 1.3 + side).sin() > -0.55,
            }
        };
        let torches = [torch(1.0), torch(-1.0)];
        let lamp = if moving {
            0
        } else if dwell >= TEST_GREEN {
            2
        } else if dwell >= TEST_AMBER {
            1
        } else {
            0
        };
        Line {
            cycle,
            tau,
            pallets,
            robots,
            cranes,
            torches,
            lamp,
        }
    }
}

// ------------------------------------------------------------ the sentry

/// Where the parts hang on a sentry's body (robotview's numbers), and
/// where a crane's magnet holds each (from the part's own origin).
const DISH_AT: (f64, f64, f64) = (0.0, 0.8, -0.5);
const GUN_AT: (f64, f64, f64) = (-0.5, 0.9, 0.0);
const ARM_AT: (f64, f64, f64) = (0.5, 0.9, 0.0);
const DISH_CARRY: (f64, f64, f64) = (0.0, 0.95, -0.2);
const GUN_CARRY: (f64, f64, f64) = (0.0, 0.06, 0.0);
/// Where parts sit on their racks: the dish shelf's top, the gun bar.
const DISH_SHELF: f64 = 0.5;
const GUN_BAR: f64 = 1.5;

fn tr(p: DVec3) -> Mat4 {
    Mat4::from_translation(p.as_vec3())
}

fn tr3(p: (f64, f64, f64)) -> Mat4 {
    Mat4::from_translation(Vec3::new(p.0 as f32, p.1 as f32, p.2 as f32))
}

/// The world place of a transform (rotation from yaw, pitch and roll, as
/// Engine's `Place` builds it; any stretch kept).
fn place_of(m: Mat4) -> Place {
    let (s, r, t) = m.to_scale_rotation_translation();
    let (yaw, pitch, roll) = r.to_euler(EulerRot::YXZ);
    let mut p = Place::new(t.x, t.y, t.z)
        .with_yaw_deg(yaw.to_degrees())
        .with_pitch_deg(pitch.to_degrees())
        .with_roll_deg(roll.to_degrees());
    if (s - Vec3::ONE).abs().max_element() > 1e-4 {
        p = p.with_stretch(s);
    }
    p
}

/// A sentry's parts at `r`, by part name (in the works' frame).
fn sentry(r: &Robot, out: &mut Vec<(&'static str, Mat4)>) {
    let root = tr(r.at) * Mat4::from_rotation_y(r.yaw as f32);
    let m = r.stride as f32;
    let ph = r.phase as f32;
    let swing = 26.0f32.to_radians() * m;
    let bob = 0.05 * m * ph.sin().abs();
    let body = root
        * Mat4::from_translation(Vec3::new(0.0, HIP + bob, 0.0))
        * Mat4::from_rotation_z((3.0 * m * ph.sin()).to_radians());
    out.push(("sentry_body", body));
    out.push((
        "sentry_arm",
        body * tr3(ARM_AT) * Mat4::from_rotation_x((12.0 * m * ph.sin()).to_radians()),
    ));
    if r.dish {
        out.push((
            "sentry_dish",
            body * tr3(DISH_AT) * Mat4::from_rotation_y(r.dish_turn as f32),
        ));
    }
    if r.gun {
        let pitch = -12.0 * m * ph.sin() - 90.0 * r.raise as f32;
        out.push((
            "sentry_gun",
            body * tr3(GUN_AT) * Mat4::from_rotation_x(pitch.to_radians()),
        ));
    }
    for (x, off) in [(0.25f32, 0.0f32), (-0.25, std::f32::consts::PI)] {
        let p = ph + off;
        let thigh = root
            * Mat4::from_translation(Vec3::new(x, HIP + bob, 0.0))
            * Mat4::from_rotation_x(-swing * p.sin());
        out.push(("sentry_thigh", thigh));
        let knee = (38.0 * m * p.cos().max(0.0)).to_radians();
        out.push((
            "sentry_shin",
            thigh * Mat4::from_translation(Vec3::new(0.0, -0.5, 0.0)) * Mat4::from_rotation_x(knee),
        ));
    }
}

/// The half-built tank in the dock at time `t`: its parts by name.
fn tank(t: f64, out: &mut Vec<(&'static str, Mat4)>) {
    let root = tr(v(DOCK_AT.x, 0.0, DOCK_AT.y));
    let hull = root * tr(v(0.0, DOCK_CRADLE, 0.0));
    out.push(("colossus_hull", hull));
    // Its tests: the turret sweeps, the cannon nods.
    let sweep = (35.0f64.to_radians() * (t * TAU / 41.0).sin()) as f32;
    let elev = (-14.0 - 12.0 * (t * TAU / 17.0).sin()) as f32;
    let turret = hull * tr(v(0.0, TURRET_UP, 0.0)) * Mat4::from_rotation_y(sweep);
    out.push(("colossus_turret", turret));
    out.push((
        "colossus_cannon",
        turret * tr(v(0.0, TRUNNION.0, TRUNNION.1)) * Mat4::from_rotation_x(elev.to_radians()),
    ));
    out.push(("colossus_shell", hull * tr(v(0.0, 0.0, 2.0))));
    let glow = 1.0 + 0.05 * (t * 3.0).sin() as f32;
    out.push((
        "colossus_core",
        hull * tr(v(0.0, -0.4, 0.0)) * Mat4::from_scale(Vec3::splat(glow)),
    ));
    // Thighs fitted, raised and chained to the frame; no shins yet.
    for k in 0..4 {
        let hp = hip(k, 0.0) - DVec3::Y * (hw_sim::robots::HULL_STAND - DOCK_CRADLE);
        let d = leg_dir(k);
        let yaw = (-d.y).atan2(d.x) as f32;
        let at = root * tr(hp) * Mat4::from_rotation_y(yaw);
        out.push(("colossus_thigh", at));
        out.push(("colossus_knee", at * tr(v(THIGH.0, THIGH.1, 0.0))));
    }
}

/// Where knee `k` of the docked tank is, in the works' frame.
fn dock_knee(k: usize) -> DVec3 {
    let hp = hip(k, 0.0) - DVec3::Y * (hw_sim::robots::HULL_STAND - DOCK_CRADLE);
    let d = leg_dir(k);
    v(DOCK_AT.x, 0.0, DOCK_AT.y) + hp + v(d.x * THIGH.0, THIGH.1, d.y * THIGH.0)
}

/// Parts the works shows, and their LOD ends (m).
const PARTS: [&str; 13] = [
    "sentry_body",
    "sentry_dish",
    "sentry_arm",
    "sentry_gun",
    "sentry_thigh",
    "sentry_shin",
    "colossus_hull",
    "colossus_turret",
    "colossus_cannon",
    "colossus_shell",
    "colossus_core",
    "colossus_thigh",
    "colossus_knee",
];

fn lod_ends(name: &str) -> [f32; 3] {
    if name.starts_with("colossus") {
        [70.0, 260.0, NEAR_END]
    } else {
        [28.0, 100.0, NEAR_END]
    }
}

/// Loaded parts: per part, one LOD level list per mesh (built on the
/// worker: loading and simplifying take a while).
type Loaded = Vec<(&'static str, Vec<Vec<LodLevel>>)>;

fn load_parts() -> Option<Loaded> {
    let dir = crate::robotview::robots_dir()?;
    let base = dir.canonicalize().ok()?;
    let mut out = Vec::new();
    for name in PARTS {
        let path = base.join(format!("{name}.glb"));
        let scene = match Model::load_scene_with(&path, &base, &EngineLimits::default()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("robot works part {name}: {e}");
                return None;
            }
        };
        let ends = lod_ends(name);
        let levels = scene
            .parts
            .into_iter()
            .map(|m| {
                let mid = LodLevel::new(m.simplified(0.22), ends[1]);
                let far = LodLevel::new(m.simplified(0.05), ends[2]);
                vec![LodLevel::new(m, ends[0]), mid, far]
            })
            .collect();
        out.push((name, levels));
    }
    Some(out)
}

// ------------------------------------------------------------ live pieces

/// The moving pieces built here (each one or more instanced entities).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Piece {
    Pallet,
    Bridge,
    Trolley,
    Magnet,
    Cable,
    Turntable,
    Upper,
    Fore,
    Torch,
    Bulb,
    Sign,
}

const PIECES: [Piece; 11] = [
    Piece::Pallet,
    Piece::Bridge,
    Piece::Trolley,
    Piece::Magnet,
    Piece::Cable,
    Piece::Turntable,
    Piece::Upper,
    Piece::Fore,
    Piece::Torch,
    Piece::Bulb,
    Piece::Sign,
];

fn piece_meshes(p: Piece) -> Vec<Mesh> {
    let mut b = MeshBuilder::new();
    match p {
        Piece::Pallet => {
            cuboid(&mut b, v(-0.85, 0.0, -0.85), v(0.85, 0.1, 0.85), iron());
            for (a, c) in [
                (v(-0.85, 0.1, -0.85), v(0.85, 0.12, -0.75)),
                (v(-0.85, 0.1, 0.75), v(0.85, 0.12, 0.85)),
                (v(-0.85, 0.1, -0.75), v(-0.75, 0.12, 0.75)),
                (v(0.75, 0.1, -0.75), v(0.85, 0.12, 0.75)),
            ] {
                cuboid(&mut b, a, c, hazard());
            }
            for (x, z) in [(-0.6, -0.6), (0.6, -0.6), (-0.6, 0.6), (0.6, 0.6)] {
                lathe(
                    &mut b,
                    v(x, 0.1, z),
                    DVec3::Y,
                    &[(0.0, 0.0), (0.06, 0.0), (0.05, 0.1), (0.0, 0.12)],
                    10,
                    |_| Some((Surf::Metal, chrome())),
                );
            }
        }
        Piece::Bridge => {
            // Spans the runway; its origin under its middle, on the floor.
            let y = RUNWAY_Y;
            cuboid(
                &mut b,
                v(-0.32, y + 0.15, -RUNWAY_Z),
                v(0.32, y + 0.85, RUNWAY_Z),
                hazard(),
            );
            for s in [-1.0, 1.0] {
                let z = s * RUNWAY_Z;
                cuboid(
                    &mut b,
                    v(-0.95, y, z - 0.28),
                    v(0.95, y + 0.5, z + 0.28),
                    hazard(),
                );
                striped(
                    &mut b,
                    v(-0.95, y + 0.25, z + s * 0.3),
                    v(0.95, y + 0.25, z + s * 0.3),
                    0.5,
                    6,
                );
                for x in [-0.6, 0.6] {
                    lathe(
                        &mut b,
                        v(x, y + 0.12, z - 0.2),
                        DVec3::Z,
                        &[(0.0, 0.0), (0.16, 0.0), (0.16, 0.4), (0.0, 0.4)],
                        12,
                        |_| Some((Surf::Metal, iron())),
                    );
                }
                striped(
                    &mut b,
                    v(0.0, y + 0.5, z - s * 0.3),
                    v(0.0, y + 0.5, z - s * 1.4),
                    0.7,
                    4,
                );
            }
            // A bubble cab for the operator, under one end.
            let c = v(0.0, y - 0.9, -RUNWAY_Z + 1.0);
            lathe(
                &mut b,
                c,
                DVec3::Y,
                &[
                    (0.0, 0.0),
                    (0.35, 0.0),
                    (0.6, 0.25),
                    (0.62, 0.45),
                    (0.0, 0.45),
                ],
                20,
                |_| Some((Surf::Plain, cream())),
            );
            let dome: Vec<(f64, f64)> = (0..=8)
                .map(|i| {
                    let a = 0.5 * PI * i as f64 / 8.0;
                    (0.58 * a.cos(), 0.45 + 0.6 * a.sin())
                })
                .collect();
            lathe(&mut b, c, DVec3::Y, &dome, 20, |_| {
                Some((Surf::Glass, rgb(140, 180, 170)))
            });
            bar(
                &mut b,
                c + DVec3::Y * 1.0,
                v(0.0, y + 0.15, c.z),
                0.08,
                8,
                Surf::Metal,
                iron(),
            );
        }
        Piece::Trolley => {
            let y = RUNWAY_Y + 0.85;
            cuboid(&mut b, v(-0.6, y, -0.5), v(0.6, y + 0.18, 0.5), iron());
            lathe(
                &mut b,
                v(-0.55, y + 0.45, 0.0),
                DVec3::X,
                &[(0.0, 0.0), (0.27, 0.0), (0.27, 1.1), (0.0, 1.1)],
                16,
                |_| Some((Surf::Metal, chrome())),
            );
            cuboid(
                &mut b,
                v(-0.6, y + 0.18, 0.2),
                v(-0.2, y + 0.6, 0.48),
                signal_red(),
            );
            lathe(
                &mut b,
                v(0.32, y + 0.18, -0.3),
                DVec3::Y,
                &[
                    (0.0, 0.0),
                    (0.18, 0.0),
                    (0.18, 0.35),
                    (0.1, 0.45),
                    (0.0, 0.45),
                ],
                12,
                |_| Some((Surf::Plain, hazard())),
            );
        }
        Piece::Magnet => {
            // Origin at the magnet's face.
            lathe(
                &mut b,
                DVec3::ZERO,
                DVec3::Y,
                &[
                    (0.0, 0.0),
                    (0.44, 0.0),
                    (0.46, 0.05),
                    (0.46, 0.22),
                    (0.3, 0.32),
                    (0.0, 0.34),
                ],
                24,
                |k| {
                    Some(if k == 0 {
                        (Surf::Metal, iron())
                    } else {
                        (Surf::Plain, signal_red())
                    })
                },
            );
            cuboid(&mut b, v(-0.5, 0.34, -0.16), v(0.5, 0.78, 0.16), hazard());
            for x in [-0.42, 0.42] {
                lathe(
                    &mut b,
                    v(x, 0.8, -0.12),
                    DVec3::Z,
                    &[(0.0, 0.0), (0.1, 0.0), (0.1, 0.24), (0.0, 0.24)],
                    12,
                    |_| Some((Surf::Metal, chrome())),
                );
            }
        }
        Piece::Cable => {
            lathe(
                &mut b,
                DVec3::ZERO,
                DVec3::Y,
                &[(0.0, 0.0), (0.025, 0.0), (0.025, 1.0), (0.0, 1.0)],
                6,
                |_| Some((Surf::Metal, iron())),
            );
        }
        Piece::Turntable => {
            lathe(
                &mut b,
                DVec3::ZERO,
                DVec3::Y,
                &[
                    (0.0, 0.0),
                    (0.42, 0.0),
                    (0.42, 0.18),
                    (0.32, 0.3),
                    (0.0, 0.32),
                ],
                20,
                |_| Some((Surf::Plain, hazard())),
            );
            for x in [-0.2, 0.2] {
                cuboid(
                    &mut b,
                    v(x - 0.05, 0.25, -0.15),
                    v(x + 0.05, SHOULDER - 1.4 + 0.15, 0.15),
                    hazard(),
                );
            }
        }
        Piece::Upper | Piece::Fore => {
            // Along +y from its joint; the hinge across x.
            let (len, r) = if p == Piece::Upper {
                (UPPER, 0.13)
            } else {
                (FORE, 0.1)
            };
            lathe(
                &mut b,
                v(-0.2, 0.0, 0.0),
                DVec3::X,
                &[(0.0, 0.0), (r + 0.06, 0.0), (r + 0.06, 0.4), (0.0, 0.4)],
                16,
                |_| Some((Surf::Plain, soot())),
            );
            lathe(
                &mut b,
                DVec3::ZERO,
                DVec3::Y,
                &[
                    (0.0, 0.0),
                    (r, 0.05),
                    (r * 0.85, len * 0.5),
                    (r * 0.75, len - 0.05),
                    (0.0, len),
                ],
                16,
                |_| Some((Surf::Plain, hazard())),
            );
            tube(
                &mut b,
                &[
                    v(0.0, 0.1, -r - 0.04),
                    v(0.0, len * 0.5, -r - 0.07),
                    v(0.0, len - 0.1, -r - 0.04),
                ],
                (0.03, 0.03),
                6,
                Some((Surf::Plain, soot())),
            );
        }
        Piece::Torch => {
            lathe(
                &mut b,
                DVec3::ZERO,
                DVec3::Y,
                &[
                    (0.0, 0.0),
                    (0.09, 0.0),
                    (0.09, 0.12),
                    (0.06, 0.18),
                    (0.035, TORCH - 0.05),
                    (0.02, TORCH),
                    (0.0, TORCH),
                ],
                12,
                |k| {
                    Some(if k >= 3 {
                        (Surf::Metal, rgb(184, 112, 60))
                    } else {
                        (Surf::Plain, soot())
                    })
                },
            );
        }
        Piece::Bulb => {
            ball(
                &mut b,
                DVec3::ZERO,
                0.17,
                14,
                Surf::Glow,
                rgb(255, 255, 255),
            );
        }
        Piece::Sign => sign_head(&mut b, true),
    }
    b.finish_parts()
}

/// What the soundscape hears of the works this frame.
#[derive(Clone, Debug, Default)]
pub struct Heard {
    /// The hall's machinery, if the works are near.
    pub hum: Option<Vec3>,
    /// Torches burning now.
    pub welding: Vec<Vec3>,
    /// Things that happened since the last frame.
    pub events: Vec<(Vec3, Clank)>,
}

/// One-off sounds of the works.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clank {
    /// The line stops: pallets thump into their stops.
    Stop,
    /// Clamps let go with a hiss as the line starts.
    Hiss,
    /// A crane's motor starts a travel.
    Crane,
    /// A finished robot wakes on the test stand.
    Boot,
    /// A marching robot's foot.
    Step,
}

struct Live {
    origin: Vec3,
    yaw: f32,
    /// Instanced entities per piece.
    pieces: HashMap<Piece, Vec<EntityId>>,
    /// The robot parts' LOD sets per part, once loaded.
    parts: HashMap<&'static str, Vec<LodId>>,
    lamp_weld: [LampId; 2],
    lamp_dock: LampId,
    lamp_test: LampId,
    lamp_beacon: LampId,
}

pub struct WorksView {
    works: Option<Landmark>,
    live: Option<Live>,
    loading: Option<Receiver<Option<Loaded>>>,
    /// Loading failed (no parts on disk): do not try again.
    no_parts: bool,
    clock: f64,
    last: Option<Line>,
    smoke: f32,
    sparks: f32,
    flicker: u64,
    heard: Heard,
}

impl WorksView {
    pub fn new(town: &TownWorld) -> Self {
        Self::with_works(
            town.landmarks
                .iter()
                .find(|l| l.kind == LandmarkKind::RobotWorks)
                .cloned(),
        )
    }

    fn with_works(works: Option<Landmark>) -> Self {
        Self {
            works,
            live: None,
            loading: None,
            no_parts: false,
            clock: 3.0,
            last: None,
            smoke: 0.0,
            sparks: 0.0,
            flicker: 0x9e37_79b9_7f4a_7c15,
            heard: Heard::default(),
        }
    }

    /// What the soundscape should hear this frame (events once).
    pub fn heard(&mut self) -> Heard {
        let events = std::mem::take(&mut self.heard.events);
        Heard {
            events,
            ..self.heard.clone()
        }
    }

    /// Are the live parts made (for the memory checks and tests)?
    pub fn is_live(&self) -> bool {
        self.live.is_some()
    }

    fn rand(&mut self) -> f32 {
        let x = &mut self.flicker;
        *x ^= *x << 13;
        *x ^= *x >> 7;
        *x ^= *x << 17;
        (*x >> 40) as f32 / (1u64 << 24) as f32
    }

    fn spawn(world: &mut World, l: &Landmark) -> EngineResult<Live> {
        let origin = Vec3::new(l.pos.x as f32, l.base as f32, l.pos.y as f32);
        let yaw = l.dir.x.atan2(l.dir.y).to_degrees() as f32;
        let mut pieces = HashMap::new();
        for p in PIECES {
            let ids = piece_meshes(p)
                .into_iter()
                .map(|m| {
                    let id = world.spawn_instanced(m);
                    if matches!(p, Piece::Bulb | Piece::Cable) {
                        world.set_casts_shadow(id, false)?;
                    }
                    Ok(id)
                })
                .collect::<EngineResult<Vec<_>>>()?;
            pieces.insert(p, ids);
        }
        let at = |x: f64, y: f64, z: f64| -> Vec3 {
            let a = (yaw as f64).to_radians();
            let (s, c) = a.sin_cos();
            origin + Vec3::new((x * c + z * s) as f32, y as f32, (-x * s + z * c) as f32)
        };
        let white = rgb(200, 220, 255);
        Ok(Live {
            origin,
            yaw,
            pieces,
            parts: HashMap::new(),
            lamp_weld: [
                world.add_lamp(Lamp::point(
                    at(STOPS[STOP_WELD], 3.0, 0.8),
                    white,
                    0.0,
                    14.0,
                ))?,
                world.add_lamp(Lamp::point(
                    at(STOPS[STOP_WELD], 3.0, -0.8),
                    white,
                    0.0,
                    14.0,
                ))?,
            ],
            lamp_dock: world.add_lamp(Lamp::point(
                at(DOCK_AT.x, DOCK_CRADLE + 1.5, DOCK_AT.y),
                white,
                0.0,
                18.0,
            ))?,
            lamp_test: world.add_lamp(Lamp::point(
                at(STOPS[STOP_TEST], ARCH_H - 0.3, 0.0),
                hazard(),
                0.0,
                10.0,
            ))?,
            lamp_beacon: world.add_lamp(Lamp::point(
                at(SHED_AT.x, SHED_H + 0.4, SHED_AT.y + SHED_HALF.1 - 1.0),
                hazard(),
                0.0,
                16.0,
            ))?,
        })
    }

    pub fn clear(&mut self, world: &mut World) {
        if let Some(live) = self.live.take() {
            for id in live.pieces.values().flatten() {
                world.despawn(*id);
            }
            for id in live.parts.values().flatten() {
                world.despawn_lod(*id);
            }
            for l in live.lamp_weld {
                world.remove_lamp(l);
            }
            for l in [live.lamp_dock, live.lamp_test, live.lamp_beacon] {
                world.remove_lamp(l);
            }
        }
        // A load under way is let go: its thread finishes and nobody
        // takes what it made.
        self.loading = None;
        self.last = None;
        self.heard = Heard::default();
    }

    pub fn update(&mut self, world: &mut World, dt: f32) -> EngineResult<()> {
        let Some(l) = self.works.clone() else {
            return Ok(());
        };
        let eye = world.camera().eye();
        let foot = Vec3::new(l.pos.x as f32, l.base as f32, l.pos.y as f32);
        let far = foot.distance(eye);
        if far > DROP {
            self.clear(world);
            return Ok(());
        }
        if self.live.is_none() {
            if far > LOAD {
                return Ok(());
            }
            self.live = Some(Self::spawn(world, &l)?);
            if !self.no_parts {
                let (tx, rx) = channel();
                std::thread::Builder::new()
                    .name("works-parts".into())
                    .spawn(move || {
                        let _ = tx.send(load_parts());
                    })
                    .map_err(|e| EngineError::InvalidValue(format!("works thread: {e}")))?;
                self.loading = Some(rx);
            }
        }
        // The parts, once the worker has them.
        if let Some(rx) = &self.loading {
            if let Ok(done) = rx.try_recv() {
                self.loading = None;
                match done {
                    Some(parts) => {
                        if let Some(live) = self.live.as_mut() {
                            for (name, sets) in parts {
                                let mut ids = Vec::new();
                                for levels in sets {
                                    ids.push(world.spawn_lod(levels)?);
                                }
                                live.parts.insert(name, ids);
                            }
                        }
                    }
                    None => self.no_parts = true,
                }
            }
        }
        self.clock += dt.clamp(0.0, 0.1) as f64;
        let shown = far < NEAR_END;
        let line = Line::at(self.clock);
        let live = self.live.take().expect("live");
        let drawn = self.draw(world, &live, &line, shown);
        self.live = Some(live);
        self.last = Some(line);
        drawn
    }

    /// The works' frame to the world.
    fn frame(live: &Live) -> Mat4 {
        Mat4::from_translation(live.origin) * Mat4::from_rotation_y(live.yaw.to_radians())
    }

    fn draw(
        &mut self,
        world: &mut World,
        live: &Live,
        line: &Line,
        shown: bool,
    ) -> EngineResult<()> {
        let f = Self::frame(live);
        let to_world = |p: DVec3| f.transform_point3(p.as_vec3());
        let mut places: HashMap<Piece, Vec<Place>> = HashMap::new();
        let mut parts: Vec<(&'static str, Mat4)> = Vec::new();
        let put = |places: &mut HashMap<Piece, Vec<Place>>, p: Piece, m: Mat4| {
            places.entry(p).or_default().push(place_of(f * m));
        };
        if shown {
            // Pallets and what they carry.
            for p in &line.pallets {
                let m = tr(v(p.x, BELT_TOP, LINE_Z));
                let m = if p.tip > 0.0 {
                    // Over the drum: tip about the end's axis, sinking out
                    // of sight behind the frame.
                    let edge = v(LINE_TO - 0.3, BELT_TOP - 0.42, LINE_Z);
                    tr(edge)
                        * Mat4::from_rotation_z((-p.tip * 100.0).to_radians() as f32)
                        * tr(v(p.x - edge.x, BELT_TOP - edge.y, 0.0))
                } else {
                    m
                };
                if p.tip < 0.95 {
                    put(&mut places, Piece::Pallet, m);
                }
            }
            for r in &line.robots {
                sentry(r, &mut parts);
            }
            // The racks: the front part gone while a crane has it.
            for (k, stop) in [STOP_DISH, STOP_GUN].into_iter().enumerate() {
                let c = &line.cranes[k];
                let rack = STOPS[stop] + RACK_STEP;
                let (name, y) = if k == 0 {
                    ("sentry_dish", DISH_SHELF)
                } else {
                    ("sentry_gun", GUN_BAR)
                };
                for dx in [-0.45, 0.45] {
                    if dx > 0.0 && !c.on_rack {
                        continue;
                    }
                    parts.push((name, tr(v(rack + dx, y, RACK_Z))));
                }
                // The crane: bridge, trolley, magnet, cables, and its load.
                put(&mut places, Piece::Bridge, tr(v(c.bridge, 0.0, 0.0)));
                put(&mut places, Piece::Trolley, tr(v(c.bridge, 0.0, c.hook.z)));
                put(&mut places, Piece::Magnet, tr(c.hook));
                let top = c.hook.y + 0.8;
                for x in [-0.42, 0.42] {
                    let len = (TROLLEY_Y + 0.4 - top).max(0.05);
                    put(
                        &mut places,
                        Piece::Cable,
                        tr(v(c.hook.x + x, top, c.hook.z))
                            * Mat4::from_scale(Vec3::new(1.0, len as f32, 1.0)),
                    );
                }
                if c.carrying {
                    let carry = if k == 0 { DISH_CARRY } else { GUN_CARRY };
                    parts.push((name, tr(c.hook) * tr3((-carry.0, -carry.1, -carry.2))));
                }
            }
            // The welding arms.
            for (k, side) in [1.0f64, -1.0].into_iter().enumerate() {
                let t = line.torches[k];
                let foot = v(STOPS[STOP_WELD], 1.4, side * WELDER_Z);
                let shoulder = foot + DVec3::Y * (SHOULDER - 1.4);
                // The torch points at the seam from the arm's side, a little
                // down.
                let dir = v(0.0, -0.35, -side).normalize();
                let wrist = t.tip - dir * TORCH;
                let reach = wrist - shoulder;
                let yaw = reach.x.atan2(reach.z);
                put(
                    &mut places,
                    Piece::Turntable,
                    tr(foot) * Mat4::from_rotation_y(yaw as f32),
                );
                let (elbow, _) = two_bone(shoulder, wrist, UPPER, FORE);
                put(&mut places, Piece::Upper, bone(shoulder, elbow, yaw));
                put(&mut places, Piece::Fore, bone(elbow, wrist, yaw));
                put(&mut places, Piece::Torch, bone(wrist, t.tip, yaw));
            }
            // The test arch's lamps.
            let (col, k) = match line.lamp {
                1 => (hazard(), 1.6 + 0.4 * (self.clock as f32 * 9.0).sin().abs()),
                2 => (arc_green(), 1.8),
                _ => (rgb(60, 50, 40), 0.4),
            };
            for z in [-0.9, 0.0, 0.9] {
                let p = place_of(f * tr(v(STOPS[STOP_TEST], ARCH_H - 0.1, z)));
                places
                    .entry(Piece::Bulb)
                    .or_default()
                    .push(p.with_tint(scale(col, k)));
            }
            // The shed's beacon: amber, flashing while a robot comes in.
            let coming = line.robots.iter().any(|r| r.stride > 0.0);
            let beat = (self.clock * 2.4).fract() < 0.5;
            let lit = coming && beat;
            let p = place_of(f * tr(v(SHED_AT.x, SHED_H + 0.3, SHED_AT.y + SHED_HALF.1 - 1.0)));
            places
                .entry(Piece::Bulb)
                .or_default()
                .push(p.with_tint(scale(hazard(), if lit { 2.0 } else { 0.35 })));
            // The sign turns.
            let sign =
                tr(v(HALL_AT.x, SIGN_UP, 0.0)) * Mat4::from_rotation_y((self.clock * 0.35) as f32);
            places
                .entry(Piece::Sign)
                .or_default()
                .push(place_of(f * sign));
            // The tank in the dock.
            tank(self.clock, &mut parts);
            // Lamps.
            let white = rgb(205, 222, 255);
            for (k, t) in line.torches.iter().enumerate() {
                let power = if t.burning {
                    25.0 + 45.0 * self.rand()
                } else {
                    0.0
                };
                world.set_lamp(
                    live.lamp_weld[k],
                    Lamp::point(to_world(t.tip), white, power, 14.0),
                )?;
            }
            world.set_lamp(
                live.lamp_test,
                Lamp::point(
                    to_world(v(STOPS[STOP_TEST], ARCH_H - 0.4, 0.0)),
                    col,
                    8.0 * k,
                    10.0,
                ),
            )?;
            let beacon = to_world(v(SHED_AT.x, SHED_H + 0.4, SHED_AT.y + SHED_HALF.1 - 1.0));
            world.set_lamp(
                live.lamp_beacon,
                Lamp::point(beacon, hazard(), if lit { 60.0 } else { 0.0 }, 16.0),
            )?;
        } else {
            for l in
                live.lamp_weld
                    .iter()
                    .chain([&live.lamp_dock, &live.lamp_test, &live.lamp_beacon])
            {
                world.set_lamp(*l, Lamp::point(live.origin, rgb(0, 0, 0), 0.0, 1.0))?;
            }
        }
        // Hand the places over.
        for (p, ids) in &live.pieces {
            let list = places.get(p).map_or(&[][..], Vec::as_slice);
            for id in ids {
                world.set_instances(*id, list)?;
            }
        }
        let mut by_part: HashMap<&str, Vec<Place>> = HashMap::new();
        for (name, m) in parts {
            by_part.entry(name).or_default().push(place_of(f * m));
        }
        for (name, ids) in &live.parts {
            let list = by_part.get(name).map_or(&[][..], Vec::as_slice);
            for id in ids {
                world.set_lod_places(*id, list)?;
            }
        }
        if shown {
            self.effects(world, live, line)?;
        }
        Ok(())
    }

    /// Sparks, smoke, the dock's welders, and what the soundscape hears.
    fn effects(&mut self, world: &mut World, live: &Live, line: &Line) -> EngineResult<()> {
        let f = Self::frame(live);
        let to_world = |p: DVec3| f.transform_point3(p.as_vec3());
        let dt = self.last.as_ref().map_or(0.0, |l| {
            let was = l.cycle as f64 * CYCLE + l.tau;
            (line.cycle as f64 * CYCLE + line.tau - was) as f32
        });
        self.sparks -= dt;
        let spark_now = self.sparks <= 0.0;
        if spark_now {
            self.sparks = 0.07;
        }
        let white = rgb(255, 236, 190);
        let mut welding = Vec::new();
        for (k, t) in line.torches.iter().enumerate() {
            if !t.burning {
                continue;
            }
            let p = to_world(t.tip);
            welding.push(p);
            if spark_now {
                let side = if k == 0 { 1.0 } else { -1.0 };
                let out =
                    (f.transform_vector3(Vec3::new(0.0, 0.4, side)) * 3.0).normalize_or_zero();
                world.particles_mut().burst(
                    ParticleEmitter::sparks(p, out * 3.0, 14)
                        .with_color(white)
                        .with_secondary_color(rgb(255, 150, 60)),
                )?;
            }
        }
        // The dock's welders: one hip at a time, in bursts.
        let hipk = ((self.clock / 6.0) as usize) % 4;
        let burst = (self.clock * 1.7).sin() > -0.3;
        let socket = {
            let hp = hip(hipk, 0.0) - DVec3::Y * (hw_sim::robots::HULL_STAND - DOCK_CRADLE);
            v(DOCK_AT.x, 0.0, DOCK_AT.y) + hp
        };
        let sp = to_world(socket);
        if burst {
            welding.push(sp);
            if spark_now {
                world.particles_mut().burst(
                    ParticleEmitter::sparks(sp, Vec3::new(0.0, -1.0, 0.0), 18)
                        .with_color(white)
                        .with_secondary_color(rgb(255, 150, 60)),
                )?;
            }
        }
        let flash = if burst {
            30.0 + 50.0 * self.rand()
        } else {
            0.0
        };
        world.set_lamp(
            live.lamp_dock,
            Lamp::point(sp, rgb(205, 222, 255), flash, 18.0),
        )?;
        // Smoke from the stacks.
        self.smoke -= dt;
        if self.smoke <= 0.0 {
            self.smoke = 0.6;
            for z in [-3.2, 2.4] {
                let top = to_world(v(HALL_AT.x - 2.7, HALL_TOP - 1.4 + 7.4, z));
                world.particles_mut().burst(
                    ParticleEmitter::smoke(top, 1.3)
                        .with_mode(ParticleMode::Burst)
                        .with_burst_count(2),
                )?;
            }
        }
        // Events since the last frame.
        if let Some(was) = &self.last {
            let started = was.tau >= MOVE && line.tau < MOVE || was.cycle != line.cycle;
            let stopped = was.tau < MOVE && line.tau >= MOVE;
            let mid = to_world(v((LINE_FROM + LINE_TO) * 0.5, 1.0, LINE_Z));
            if started {
                self.heard.events.push((mid, Clank::Hiss));
                for k in 0..2 {
                    let x = STOPS[[STOP_DISH, STOP_GUN][k]];
                    self.heard
                        .events
                        .push((to_world(v(x, RUNWAY_Y, 0.0)), Clank::Crane));
                }
            }
            if stopped {
                self.heard.events.push((mid, Clank::Stop));
                // A puff of steam from each clamp.
                for &x in &STOPS[1..] {
                    let p = to_world(v(x, BELT_TOP - 0.2, LINE_Z + BELT_HW + 0.2));
                    world.particles_mut().burst(
                        ParticleEmitter::smoke(p, 0.35)
                            .with_mode(ParticleMode::Burst)
                            .with_burst_count(1)
                            .with_color(rgb(235, 235, 230)),
                    )?;
                }
            }
            let dw = |l: &Line| l.tau - MOVE;
            for c in &line.cranes {
                for at in [2.0, 3.6, 5.6, RELEASE + 0.5] {
                    if dw(was) < at && dw(line) >= at {
                        self.heard
                            .events
                            .push((to_world(v(c.bridge, RUNWAY_Y, c.hook.z)), Clank::Crane));
                    }
                }
            }
            if dw(was) < TEST_AMBER && dw(line) >= TEST_AMBER {
                self.heard
                    .events
                    .push((to_world(v(STOPS[STOP_TEST], 3.0, 0.0)), Clank::Boot));
            }
            // Marching feet.
            for r in &line.robots {
                if r.stride <= 0.0 {
                    continue;
                }
                if let Some(o) = was.robots.iter().find(|o| o.serial == r.serial) {
                    let before = (o.phase / PI).floor();
                    if (r.phase / PI).floor() > before {
                        self.heard.events.push((to_world(r.at), Clank::Step));
                    }
                }
            }
        }
        self.heard.hum = Some(to_world(v(HALL_AT.x + HALL_HW, 2.5, 0.0)));
        self.heard.welding = welding;
        Ok(())
    }
}

/// A colour brightened by `k` (for an instance tint).
fn scale(c: Color, k: f32) -> Color {
    Color::rgb01_unchecked(c.r * k, c.g * k, c.b * k)
}

/// The elbow between `a` and `c` for bones `l1` and `l2`, bending up, and
/// whether the reach was short enough.
fn two_bone(a: DVec3, c: DVec3, l1: f64, l2: f64) -> (DVec3, bool) {
    let d = c - a;
    let len = d.length();
    let ok = len <= l1 + l2 - 0.01;
    let l = len.clamp((l1 - l2).abs() + 0.01, l1 + l2 - 0.01);
    let dir = d.normalize_or(DVec3::Y);
    let x = (l1 * l1 - l2 * l2 + l * l) / (2.0 * l);
    let h = (l1 * l1 - x * x).max(0.0).sqrt();
    let mut up = DVec3::Y - dir * dir.dot(DVec3::Y);
    if up.length_squared() < 1e-8 {
        up = DVec3::X;
    }
    (a + dir * x + up.normalize() * h, ok)
}

/// A bone piece from `a` to `c` (built along +y from its joint, its hinge
/// across x), turned so its hinge lies across the arm's plane at `yaw`.
fn bone(a: DVec3, c: DVec3, yaw: f64) -> Mat4 {
    let y = (c - a).normalize_or(DVec3::Y);
    let hinge = DVec3::new(yaw.cos(), 0.0, -yaw.sin());
    let x = (hinge - y * hinge.dot(y)).normalize_or(DVec3::X);
    let z = x.cross(y);
    let r = Mat3::from_cols(x.as_vec3(), y.as_vec3(), z.as_vec3());
    Mat4::from_rotation_translation(Quat::from_mat3(&r), a.as_vec3())
}

/// A camera for a close look at the works: `"works"` over the whole yard,
/// `"worksline"` down the line at the welders, `"worksdock"` up at the
/// tank. Eye, yaw and pitch (degrees), or none if the town has no works.
pub fn view(town: &TownWorld, view: &str, near: DVec2) -> Option<(Vec3, f32, f32)> {
    let l = crate::landmarkview::nearest(town, LandmarkKind::RobotWorks, near)?;
    let local = |p: DVec3| {
        let q = l.to_plan(DVec2::new(p.x, p.z));
        DVec3::new(q.x, l.base + p.y, q.y)
    };
    let (eye, look) = match view {
        "worksline" => (
            v(STOPS[STOP_WELD] + 4.5, 2.6, 5.2),
            v(STOPS[STOP_WELD] - 1.0, 2.2, 0.0),
        ),
        "worksdock" => (
            v(DOCK_AT.x + 13.0, 4.0, DOCK_AT.y + 9.0),
            v(DOCK_AT.x, 7.0, DOCK_AT.y),
        ),
        "workshall" => (
            v(STOPS[STOP_DISH] + 3.0, 3.2, -6.5),
            v(HALL_AT.x + HALL_HW, 2.8, 0.0),
        ),
        _ => (v(16.0, 16.0, l.plaza + 16.0), v(-2.0, 3.0, 0.0)),
    };
    let (e, t) = (local(eye), local(look));
    let d = t - e;
    let yaw = d.x.atan2(d.z).to_degrees() as f32;
    let pitch = (d.y / DVec2::new(d.x, d.z).length()).atan().to_degrees() as f32;
    Some((e.as_vec3(), yaw, pitch))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn works() -> Landmark {
        Landmark {
            kind: LandmarkKind::RobotWorks,
            pos: DVec2::ZERO,
            plaza: 23.5,
            base: 2.0,
            low: 1.0,
            height: 20.0,
            dir: DVec2::Y,
            stairs: Vec::new(),
        }
    }

    #[test]
    fn a_robot_is_built_station_by_station() {
        // Follow one serial down the line.
        let n = 10i64;
        let mut saw = (false, false, false);
        let mut t = n as f64 * CYCLE;
        while t < (n + STOP_END as i64 + 1) as f64 * CYCLE {
            let line = Line::at(t);
            if let Some(r) = line
                .robots
                .iter()
                .find(|r| r.serial == n && r.stride == 0.0)
            {
                let stop = (line.cycle - n) as usize;
                // Never a gun before the dish, never fitted before its stop.
                assert!(!r.gun || r.dish);
                if stop < STOP_DISH {
                    assert!(!r.dish);
                }
                if stop < STOP_GUN {
                    assert!(!r.gun);
                }
                saw.0 |= r.dish;
                saw.1 |= r.gun;
                saw.2 |= r.dish_turn > 0.1;
            }
            t += 0.1;
        }
        assert_eq!(saw, (true, true, true));
    }

    #[test]
    fn a_crane_carries_its_part_from_rack_to_robot() {
        let mut held = 0;
        let mut t = 100.0 * CYCLE;
        let end = t + CYCLE;
        let mut released_on = None;
        while t < end {
            let line = Line::at(t);
            let c = line.cranes[0];
            if c.carrying {
                held += 1;
            }
            // When it lets go, the robot at the dish stop has its dish.
            if !c.carrying && line.tau - MOVE >= RELEASE && released_on.is_none() {
                let r = line
                    .robots
                    .iter()
                    .find(|r| (r.at.x - STOPS[STOP_DISH]).abs() < 1e-6)
                    .expect("a robot at the dish stop");
                assert!(r.dish);
                // The magnet is down on the dish's carry point.
                let want = r.at.y + HIP as f64 + DISH_AT.1 + DISH_CARRY.1;
                assert!((c.hook.y - want).abs() < 1e-6, "{} {}", c.hook.y, want);
                released_on = Some(r.serial);
            }
            // The hook never sweeps through a robot's head while it travels.
            if (c.hook.z - LINE_Z).abs() > 0.4 && (c.hook.z - RACK_Z).abs() > 0.4 {
                assert!(c.hook.y >= HOOK_UP - 1e-6);
            }
            t += 0.05;
        }
        assert!(held > 50 && released_on.is_some());
    }

    #[test]
    fn robots_march_into_the_shed_and_go() {
        let walk = march_time();
        assert!((6.0..20.0).contains(&walk), "{walk}");
        let mut t = 50.0 * CYCLE;
        let mut most = 0;
        while t < 60.0 * CYCLE {
            let line = Line::at(t);
            let marching: Vec<&Robot> = line.robots.iter().filter(|r| r.stride > 0.0).collect();
            most = most.max(marching.len());
            for r in &marching {
                // Off the belt, never inside the runway's columns or the shed's walls.
                assert!(r.at.z <= LINE_Z + 1e-6);
                for &x in &RUNWAY_COLUMNS {
                    let d = DVec2::new(r.at.x - x, r.at.z + RUNWAY_Z).length();
                    assert!(d > 0.6, "a robot walks into a column");
                }
            }
            t += 0.25;
        }
        // One leaves each cycle; never a crowd.
        assert!((1..=3).contains(&most), "{most}");
    }

    #[test]
    fn welding_arms_reach_their_seams() {
        let mut t = 7.0 * CYCLE;
        while t < 8.0 * CYCLE {
            let line = Line::at(t);
            for (k, side) in [1.0f64, -1.0].into_iter().enumerate() {
                let tip = line.torches[k].tip;
                let dir = v(0.0, -0.35, -side).normalize();
                let wrist = tip - dir * TORCH;
                let shoulder = v(STOPS[STOP_WELD], SHOULDER, side * WELDER_Z);
                let (_, ok) = two_bone(shoulder, wrist, UPPER, FORE);
                assert!(ok, "an arm too short at {t}");
            }
            t += 0.1;
        }
    }

    #[test]
    fn places_round_trip() {
        let m = tr(v(1.0, 2.0, 3.0))
            * Mat4::from_rotation_y(0.7)
            * Mat4::from_rotation_x(-0.4)
            * Mat4::from_rotation_z(0.2);
        let back = place_of(m).to_matrix();
        assert!(m.abs_diff_eq(back, 1e-4), "{m} {back}");
        let s = tr(v(0.0, 1.0, 0.0)) * Mat4::from_scale(Vec3::new(1.0, 3.0, 1.0));
        assert!(s.abs_diff_eq(place_of(s).to_matrix(), 1e-4));
    }

    #[test]
    fn every_piece_has_faces() {
        for p in PIECES {
            let n: usize = piece_meshes(p).iter().map(Mesh::face_count).sum();
            assert!(n > 10, "{p:?}");
        }
        let near: usize = crate::landmarkview::model(&works(), true)
            .iter()
            .map(Mesh::face_count)
            .sum();
        let far: usize = crate::landmarkview::model(&works(), false)
            .iter()
            .map(Mesh::face_count)
            .sum();
        assert!(far * 4 < near, "far {far} near {near}");
    }
}
