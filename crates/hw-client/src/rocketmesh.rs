//! The rocket port's models, in the taste of a 1950s science-fiction
//! magazine cover. The pad and everything that stands still on it (the
//! launch stand and its flame duct, the orange lattice gantry with its
//! countdown board and atom, the domed blockhouse, the fuel spheres, the
//! floodlight masts) are the landmark's own LODs ([`gantry`]). The rocket,
//! its swing arms, the engine's flame and the board's digits move, and
//! [`crate::rocketview`] places them.
//!
//! Frames: the pad's (origin on the plinth's top at the rocket's axis, +z
//! towards the town, +x towards the gantry); the rocket's origin is its
//! fins' feet, on the stand's top.

use crate::carmesh::{disc, inside, lathe, surface, tube, Lod, Paint};
use crate::citymesh::{MeshBuilder, Surf};
use crate::landmarkview::strut;
use crate::skins::Tex;
use engine::prelude::*;
use engine::texgen::FacadeLayer;
use glam::{DVec2, DVec3};
use hw_gen::urban::gantry::{
    masts, ARM_HALF_W, ARM_HEIGHTS, ARM_LEN, BOARD_HALF, BOARD_Y, BUNKER_H, BUNKER_R, BUNKER_Z,
    DUCT_H, DUCT_HALF, DUCT_X, FIN_ANGLES, FIN_REACH, MAST_H, ROCKET_H, ROCKET_R, STAND_H, STAND_R,
    TANKS, TANK_R, TANK_Y, TOWER_H, TOWER_HALF, TOWER_X,
};
use hw_gen::urban::landmarks::Landmark;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// The gantry's paint: international orange, white near the top.
const ORANGE: (u8, u8, u8) = (212, 84, 42);
const WHITE: (u8, u8, u8) = (238, 236, 228);
/// The rocket's hull, its red trim and its chrome.
const HULL: (u8, u8, u8) = (240, 236, 222);
const RED: (u8, u8, u8) = (196, 34, 38);
const CHROME: (u8, u8, u8) = (224, 228, 234);
const CONCRETE: (u8, u8, u8) = (178, 176, 166);
const DARK: (u8, u8, u8) = (44, 46, 50);
const SOOT: (u8, u8, u8) = (26, 24, 22);
const YELLOW: (u8, u8, u8) = (236, 190, 40);
/// The flame: white-hot at its heart, orange outside.
pub const FLAME_CORE: (u8, u8, u8) = (255, 246, 214);
pub const FLAME: (u8, u8, u8) = (255, 140, 46);
/// The board's digits.
pub const DIGIT: (u8, u8, u8) = (255, 236, 220);

fn c3(t: (u8, u8, u8)) -> Color {
    rgb(t.0, t.1, t.2)
}
fn plain(t: (u8, u8, u8)) -> Paint {
    Some((Surf::Plain, c3(t)))
}
fn metal(t: (u8, u8, u8)) -> Paint {
    Some((Surf::Metal, c3(t)))
}
fn glow(t: (u8, u8, u8)) -> Paint {
    Some((Surf::Glow, c3(t)))
}

fn with(b: &mut MeshBuilder, s: Surf, f: impl FnOnce(&mut MeshBuilder)) {
    b.surf = s;
    f(b);
    b.surf = Surf::Plain;
}

/// A horizontal unit vector at `a` radians from +z (towards +x).
fn round(a: f64) -> DVec3 {
    DVec3::new(a.sin(), 0.0, a.cos())
}

// ------------------------------------------------------------ the pad

/// The rocket port's still parts, in the pad's frame.
pub fn gantry(b: &mut MeshBuilder, l: &Landmark, near: bool) {
    pad_marks(b, near);
    stand(b, near);
    duct(b, near);
    tower(b, near);
    blockhouse(b, near);
    for (x, z) in TANKS {
        tank(b, DVec3::new(x, 0.0, z), near);
    }
    for m in masts(l.plaza) {
        mast(b, m, near);
    }
}

/// Paint on the pad: a hazard ring round the stand, a yellow line round
/// the gantry's foot and grated trenches from the fuel spheres.
fn pad_marks(b: &mut MeshBuilder, near: bool) {
    let y = 0.012;
    let n = if near { 48 } else { 16 };
    let (r0, r1) = (STAND_R + 1.4, STAND_R + 2.2);
    for k in 0..n {
        let (a0, a1) = (TAU * k as f64 / n as f64, TAU * (k + 1) as f64 / n as f64);
        let col = if !near || k % 2 == 0 { YELLOW } else { SOOT };
        b.quad(
            [
                round(a0) * r0 + DVec3::Y * y,
                round(a1) * r0 + DVec3::Y * y,
                round(a1) * r1 + DVec3::Y * y,
                round(a0) * r1 + DVec3::Y * y,
            ],
            DVec3::Y,
            c3(col),
        );
    }
    if !near {
        return;
    }
    // Grated trenches from the spheres to the gantry's foot.
    let foot = DVec3::new(TOWER_X - TOWER_HALF - 0.4, 0.0, -TOWER_HALF + 0.6);
    for (x, z) in TANKS {
        let from = DVec3::new(x, 0.0, z);
        let d = (foot - from).normalize();
        let s = DVec3::new(d.z, 0.0, -d.x) * 0.35;
        let a = from + d * TANK_R;
        // They go under the pad's paint and the duct's hood: they stop
        // short of both (no two layers of paint on one spot).
        let clear = 7.6;
        let Some(c) = (0..=200)
            .map(|i| a + (foot - a) * (i as f64 / 200.0))
            .take_while(|p| p.length() > clear)
            .last()
        else {
            continue;
        };
        b.quad(
            [
                a - s + DVec3::Y * y,
                c - s + DVec3::Y * y,
                c + s + DVec3::Y * y,
                a + s + DVec3::Y * y,
            ],
            DVec3::Y,
            c3(DARK),
        );
    }
}

/// The launch stand: a concrete drum with the flame pit in its top, a
/// steel launch ring and four hold-down clamps where the fins stand.
fn stand(b: &mut MeshBuilder, near: bool) {
    let seg = if near { 48 } else { 14 };
    b.tex = near.then_some(Tex::from(FacadeLayer::Concrete));
    lathe(
        b,
        DVec3::ZERO,
        DVec3::Y,
        &[
            (0.0, -0.3),
            (STAND_R + 0.25, -0.3),
            (STAND_R + 0.25, 0.25),
            (STAND_R, 0.4),
            (STAND_R, STAND_H - 0.35),
            (STAND_R - 0.3, STAND_H),
            (1.6, STAND_H),
            (1.35, STAND_H - 0.2),
            (1.25, STAND_H - 1.4),
            (0.0, STAND_H - 1.4),
        ],
        seg,
        |k| match k {
            6..=8 => plain(SOOT),
            _ => plain(CONCRETE),
        },
    );
    b.tex = None;
    // The steel launch ring the fins stand on.
    lathe(
        b,
        DVec3::Y * STAND_H,
        DVec3::Y,
        &[
            (FIN_REACH - 0.9, -0.02),
            (FIN_REACH + 0.5, -0.02),
            (FIN_REACH + 0.5, 0.18),
            (FIN_REACH - 0.9, 0.18),
            (FIN_REACH - 0.9, -0.02),
        ],
        seg,
        |_| metal(DARK),
    );
    if !near {
        return;
    }
    for a in FIN_ANGLES {
        let d = round(a.to_radians());
        let side = DVec3::new(d.z, 0.0, -d.x);
        // Two jaws either side of the fin's foot.
        for s in [-1.0, 1.0] {
            b.oriented_box(
                d * FIN_REACH + side * (0.62 * s) + DVec3::Y * (STAND_H + 0.45),
                side,
                DVec3::new(0.14, 0.3, 0.5),
                c3(ORANGE),
            );
        }
        // A bolt head on each, and a hydraulic ram down the drum's side.
        let foot = d * (STAND_R + 0.08);
        tube(
            b,
            &[foot + DVec3::Y * (STAND_H - 0.4), foot + DVec3::Y * 0.6],
            (0.12, 0.12),
            10,
            metal(CHROME),
        );
    }
}

/// The flame duct's hood on the stand's -x side, black inside.
fn duct(b: &mut MeshBuilder, near: bool) {
    let n = if near { 14 } else { 5 };
    // The hood's section across z: an arch.
    let arch = |w: f64, h: f64, x: f64| -> Vec<DVec3> {
        let mut v = vec![DVec3::new(x, 0.0, -w)];
        for k in 0..=n {
            let t = PI * k as f64 / n as f64;
            v.push(DVec3::new(
                x,
                h - (w * 0.55) * (1.0 - t.sin()),
                -w * t.cos(),
            ));
        }
        v.push(DVec3::new(x, 0.0, w));
        v
    };
    let x_in = -(STAND_R - 0.6);
    let outer = [
        arch(DUCT_HALF, DUCT_H, x_in),
        arch(DUCT_HALF, DUCT_H, DUCT_X),
    ];
    b.tex = near.then_some(Tex::from(FacadeLayer::Concrete));
    surface(b, &outer, false, |_, _| plain(CONCRETE));
    b.tex = None;
    let (iw, ih) = (DUCT_HALF - 0.45, DUCT_H - 0.5);
    // The mouth's face: between the outer and the inner arch.
    let o = &outer[1];
    let i = arch(iw, ih, DUCT_X);
    for k in 0..o.len() - 1 {
        b.quad([o[k], o[k + 1], i[k + 1], i[k]], -DVec3::X, c3(CONCRETE));
    }
    // Inside: soot all the way back to the stand.
    let tunnel = [arch(iw, ih, DUCT_X), arch(iw, ih, x_in - 0.2)];
    inside(|| surface(b, &tunnel, false, |_, _| plain(SOOT)));
    let back = arch(iw, ih, x_in - 0.2);
    b.polygon(&back, -DVec3::X, c3(SOOT));
}

/// The service gantry: four orange legs cross-braced in bays, platforms at
/// the arms, a parked lift, a hammerhead crane on top with a red beacon
/// and a chrome atom, and the countdown board on its town face.
fn tower(b: &mut MeshBuilder, near: bool) {
    let h = TOWER_HALF;
    let corner = |sx: f64, sz: f64, y: f64| DVec3::new(TOWER_X + sx * h, y, sz * h);
    let paint = |y: f64| {
        // White bands near the top, for the aircraft.
        if y > TOWER_H - 8.0 && ((TOWER_H - y) / 2.0) as i64 % 2 == 0 {
            c3(WHITE)
        } else {
            c3(ORANGE)
        }
    };
    let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    let w = if near { 0.36 } else { 0.7 };
    let bay = 4.0;
    let bays = (TOWER_H / bay).round() as usize;
    for i in 0..bays {
        let (y0, y1) = (i as f64 * bay, (i + 1) as f64 * bay);
        let col = paint(y0 + 0.5);
        for &(sx, sz) in &corners {
            strut(b, corner(sx, sz, y0), corner(sx, sz, y1), w, col);
        }
        // A girt round each bay's top.
        for k in 0..4 {
            let (a, c) = (corners[k], corners[(k + 1) % 4]);
            strut(b, corner(a.0, a.1, y1), corner(c.0, c.1, y1), w * 0.7, col);
            if near {
                // X-bracing on every face.
                strut(b, corner(a.0, a.1, y0), corner(c.0, c.1, y1), 0.16, col);
                strut(b, corner(c.0, c.1, y0), corner(a.0, a.1, y1), 0.16, col);
            }
        }
    }
    // Platforms at the arms and on top: grating with a rail round them.
    let mut decks: Vec<f64> = ARM_HEIGHTS.to_vec();
    decks.push(TOWER_H);
    for y in decks {
        b.oriented_box(
            DVec3::new(TOWER_X, y - 0.12, 0.0),
            DVec3::X,
            DVec3::new(h + 0.6, 0.12, h + 0.6),
            c3(DARK),
        );
        if near {
            let r = h + 0.55;
            let pts = [(-r, -r), (r, -r), (r, r), (-r, r)];
            for k in 0..4 {
                let (a, c) = (pts[k], pts[(k + 1) % 4]);
                let p = |q: (f64, f64), up: f64| DVec3::new(TOWER_X + q.0, y + up, q.1);
                // The arms' side stays open.
                if k == 3 && y < TOWER_H {
                    continue;
                }
                strut(b, p(a, 1.05), p(c, 1.05), 0.07, c3(WHITE));
                strut(b, p(a, 0.55), p(c, 0.55), 0.05, c3(WHITE));
                for s in 0..=4 {
                    let t = s as f64 / 4.0;
                    let q = (a.0 + (c.0 - a.0) * t, a.1 + (c.1 - a.1) * t);
                    strut(b, p(q, 0.0), p(q, 1.05), 0.05, c3(WHITE));
                }
            }
        }
    }
    // The lift: its guide rails up the middle and its cab, parked at the
    // crew arm.
    if near {
        for z in [-0.9, 0.9] {
            strut(
                b,
                DVec3::new(TOWER_X + 0.9, 0.0, z),
                DVec3::new(TOWER_X + 0.9, TOWER_H, z),
                0.12,
                c3(DARK),
            );
        }
        let cab = ARM_HEIGHTS[1];
        b.oriented_box(
            DVec3::new(TOWER_X + 0.4, cab + 1.25, 0.0),
            DVec3::X,
            DVec3::new(0.8, 1.25, 0.95),
            c3(CHROME),
        );
        with(b, Surf::Glass, |b| {
            b.oriented_box(
                DVec3::new(TOWER_X - 0.42, cab + 1.5, 0.0),
                DVec3::X,
                DVec3::new(0.03, 0.7, 0.6),
                rgb(60, 90, 110),
            )
        });
    }
    // The hammerhead crane: a jib over the rocket's nose, a counterweight
    // behind, on a turntable.
    let top = TOWER_H;
    lathe(
        b,
        DVec3::new(TOWER_X, top, 0.0),
        DVec3::Y,
        &[(0.0, 0.0), (1.1, 0.0), (1.1, 0.5), (0.8, 0.8), (0.0, 0.8)],
        if near { 24 } else { 8 },
        |_| plain(ORANGE),
    );
    let (j0, j1) = (
        DVec3::new(TOWER_X - 7.0, top + 1.6, 0.0),
        DVec3::new(TOWER_X + 5.0, top + 1.6, 0.0),
    );
    for (dz, dy) in [(-0.5, 0.0), (0.5, 0.0), (0.0, 1.0)] {
        let o = DVec3::new(0.0, dy, dz);
        strut(b, j0 + o, j1 + o, if near { 0.2 } else { 0.4 }, c3(ORANGE));
    }
    if near {
        let n = 12;
        for k in 0..n {
            let t0 = k as f64 / n as f64;
            let t1 = (k + 1) as f64 / n as f64;
            let at = |t: f64, dz: f64, dy: f64| j0 + (j1 - j0) * t + DVec3::new(0.0, dy, dz);
            for dz in [-0.5, 0.5] {
                strut(b, at(t0, dz, 0.0), at(t1, 0.0, 1.0), 0.09, c3(ORANGE));
            }
        }
        // The hook block hanging over the nose.
        strut(
            b,
            j0 + DVec3::new(1.0, 0.0, 0.0),
            j0 + DVec3::new(1.0, -3.0, 0.0),
            0.05,
            c3(DARK),
        );
        b.oriented_box(
            j0 + DVec3::new(1.0, -3.3, 0.0),
            DVec3::X,
            DVec3::new(0.3, 0.3, 0.2),
            c3(YELLOW),
        );
    }
    b.oriented_box(
        j1 + DVec3::new(-0.6, -0.6, 0.0),
        DVec3::X,
        DVec3::new(0.9, 0.8, 0.9),
        c3(CONCRETE),
    );
    // The atom: a chrome nucleus in three orbits, on a mast over the
    // turntable. The beacon glows red on its tip.
    let mast_top = top + 6.0;
    strut(
        b,
        DVec3::new(TOWER_X, top + 0.8, 0.0),
        DVec3::new(TOWER_X, mast_top + 2.6, 0.0),
        0.16,
        c3(WHITE),
    );
    let nucleus = DVec3::new(TOWER_X, mast_top, 0.0);
    lathe(
        b,
        nucleus - DVec3::Y * 0.55,
        DVec3::Y,
        &(0..=8)
            .map(|i| {
                let t = PI * i as f64 / 8.0;
                (0.55 * t.sin(), 0.55 - 0.55 * t.cos())
            })
            .collect::<Vec<_>>(),
        if near { 20 } else { 8 },
        |_| metal(CHROME),
    );
    if near {
        for k in 0..3 {
            let tilt = PI * k as f64 / 3.0;
            let (u, v) = (
                DVec3::new(tilt.cos(), 0.0, tilt.sin()),
                DVec3::new(-tilt.sin() * 0.5, 0.87, tilt.cos() * 0.5).normalize(),
            );
            let ring: Vec<DVec3> = (0..=40)
                .map(|i| {
                    let a = TAU * i as f64 / 40.0;
                    nucleus + u * (2.0 * a.cos()) + v * (0.8 * a.sin())
                })
                .collect();
            tube(b, &ring, (0.07, 0.07), 8, metal(CHROME));
        }
    }
    with(b, Surf::Glow, |b| {
        b.cylinder(
            DVec3::new(TOWER_X, mast_top + 2.6, 0.0),
            (0.28, 0.2),
            0.45,
            if near { 12 } else { 6 },
            true,
            rgb(255, 40, 30),
        )
    });
    // The countdown board on the town face: a chrome frame round a black
    // face (the digits are the view's), with an amber "T-MINUS" lamp strip.
    let bz = TOWER_HALF + 0.3;
    let (bw, bh) = BOARD_HALF;
    b.oriented_box(
        DVec3::new(TOWER_X, BOARD_Y, bz),
        DVec3::X,
        DVec3::new(bw + 0.18, bh + 0.18, 0.12),
        c3(CHROME),
    );
    b.oriented_box(
        DVec3::new(TOWER_X, BOARD_Y, bz + 0.1),
        DVec3::X,
        DVec3::new(bw, bh, 0.04),
        rgb(14, 14, 16),
    );
    // Its brackets to the tower.
    for x in [-bw * 0.7, bw * 0.7] {
        b.oriented_box(
            DVec3::new(TOWER_X + x, BOARD_Y, TOWER_HALF + 0.1),
            DVec3::X,
            DVec3::new(0.1, bh * 0.8, 0.2),
            c3(DARK),
        );
    }
    // A finned chrome crest over the board.
    if near {
        let crest: Vec<DVec3> = (0..=16)
            .map(|i| {
                let t = i as f64 / 16.0;
                DVec3::new(
                    TOWER_X - bw - 0.6 + (2.0 * bw + 1.2) * t,
                    BOARD_Y + bh + 0.45 + 0.5 * (PI * t).sin(),
                    bz + 0.05,
                )
            })
            .collect();
        tube(b, &crest, (0.12, 0.12), 10, metal(CHROME));
    }
}

/// The blockhouse: a low concrete dome, slit windows looking at the
/// rocket, a steel door under a hood facing the town, periscopes, a radar
/// dish and a klaxon on its mast.
fn blockhouse(b: &mut MeshBuilder, near: bool) {
    let c = DVec3::new(0.0, 0.0, BUNKER_Z);
    let seg = if near { 48 } else { 12 };
    let (r, h) = (BUNKER_R, BUNKER_H);
    let profile: Vec<(f64, f64)> = vec![
        (0.0, -0.3),
        (r + 0.2, -0.3),
        (r + 0.2, 0.3),
        (r, 0.4),
        (r * 0.95, h * 0.3),
        (r * 0.85, h * 0.52),
        (r * 0.68, h * 0.72),
        (r * 0.45, h * 0.88),
        (r * 0.2, h * 0.97),
        (0.0, h),
    ];
    b.tex = near.then_some(Tex::from(FacadeLayer::Concrete));
    lathe(b, c, DVec3::Y, &profile, seg, |_| plain(CONCRETE));
    b.tex = None;
    let r_at = |y: f64| {
        let k = profile
            .windows(2)
            .find(|w| y >= w[0].1 && y <= w[1].1)
            .unwrap_or(&profile[3..5]);
        let t = (y - k[0].1) / (k[1].1 - k[0].1);
        k[0].0 + (k[1].0 - k[0].0) * t
    };
    // Slit windows towards the rocket (-z), under a concrete brow.
    let wy = h * 0.42;
    for a in [-0.45, 0.0, 0.45] {
        let d = round(PI + a);
        let side = DVec3::new(d.z, 0.0, -d.x);
        let at = c + d * (r_at(wy) - 0.12) + DVec3::Y * wy;
        with(b, Surf::Glass, |b| {
            b.oriented_box(at, side, DVec3::new(0.85, 0.16, 0.2), rgb(40, 60, 70))
        });
        b.oriented_box(
            at + d * 0.22 + DVec3::Y * 0.3,
            side,
            DVec3::new(1.0, 0.07, 0.3),
            c3(CONCRETE),
        );
    }
    // The door, deep in a hooded porch towards the town.
    let d = DVec3::Z;
    let porch = c + d * (r - 0.2);
    b.tex = near.then_some(Tex::from(FacadeLayer::Concrete));
    b.oriented_box(
        porch + DVec3::Y * 1.3,
        DVec3::X,
        DVec3::new(1.5, 1.3, 0.75),
        c3(CONCRETE),
    );
    b.tex = None;
    b.oriented_box(
        porch + d * 0.72 + DVec3::Y * 1.05,
        DVec3::X,
        DVec3::new(0.65, 1.05, 0.05),
        rgb(92, 96, 90),
    );
    if near {
        // A wheel on the door, and the red "launch on" lamp above it.
        let wc = porch + d * 0.8 + DVec3::Y * 1.15;
        let ring: Vec<DVec3> = (0..=24)
            .map(|i| {
                let a = TAU * i as f64 / 24.0;
                wc + DVec3::new(0.3 * a.cos(), 0.3 * a.sin(), 0.0)
            })
            .collect();
        tube(b, &ring, (0.035, 0.035), 6, metal(CHROME));
        with(b, Surf::Glow, |b| {
            b.oriented_box(
                porch + d * 0.78 + DVec3::Y * 2.35,
                DVec3::X,
                DVec3::new(0.18, 0.1, 0.06),
                rgb(255, 60, 40),
            )
        });
        // Periscopes, their heads turned to the pad.
        for x in [-1.1, 1.1] {
            let foot = c + DVec3::new(x, h * 0.9, -0.6);
            tube(
                b,
                &[
                    foot,
                    foot + DVec3::Y * 1.3,
                    foot + DVec3::new(0.0, 1.5, -0.25),
                ],
                (0.13, 0.13),
                12,
                metal(DARK),
            );
            b.oriented_box(
                foot + DVec3::new(0.0, 1.5, -0.45),
                DVec3::X,
                DVec3::new(0.18, 0.15, 0.15),
                c3(DARK),
            );
        }
    }
    // The radio mast with its radar dish and klaxon horns.
    let foot = c + DVec3::new(1.6, h * 0.82, 1.2);
    let tip = foot + DVec3::Y * 5.2;
    strut(b, foot, tip, 0.14, c3(WHITE));
    let dish = foot + DVec3::Y * 3.8;
    lathe(
        b,
        dish,
        DVec3::new(0.0, 0.5, -1.0),
        &[
            (0.0, 0.25),
            (0.6, 0.15),
            (1.0, 0.0),
            (1.02, 0.04),
            (0.0, 0.32),
        ],
        if near { 24 } else { 8 },
        |_| metal(CHROME),
    );
    if near {
        for s in [-1.0, 1.0] {
            lathe(
                b,
                foot + DVec3::new(0.12 * s, 2.4, 0.0),
                DVec3::new(s, 0.0, 0.25),
                &[
                    (0.06, 0.0),
                    (0.12, 0.35),
                    (0.3, 0.6),
                    (0.31, 0.62),
                    (0.0, 0.55),
                ],
                12,
                |_| plain(RED),
            );
        }
    }
}

/// A fuel sphere on six legs, a red band round it, a ladder up its side.
fn tank(b: &mut MeshBuilder, at: DVec3, near: bool) {
    let seg = if near { 40 } else { 12 };
    let rings = if near { 20 } else { 8 };
    let c = at + DVec3::Y * TANK_Y;
    let profile: Vec<(f64, f64)> = (0..=rings)
        .map(|i| {
            let t = PI * i as f64 / rings as f64;
            (TANK_R * t.sin(), -TANK_R * t.cos())
        })
        .collect();
    let band = |i: usize| {
        let y = -TANK_R * (PI * (i as f64 + 0.5) / rings as f64).cos();
        y.abs() < TANK_R * 0.16
    };
    lathe(b, c, DVec3::Y, &profile, seg, |i| {
        if band(i) {
            plain(RED)
        } else {
            plain(WHITE)
        }
    });
    // Legs from the equator down to footings, braced.
    let legs = 6;
    for k in 0..legs {
        let d = round(TAU * (k as f64 + 0.5) / legs as f64);
        let top = c + d * (TANK_R * 0.98);
        let foot = at + d * (TANK_R * 0.82);
        tube(
            b,
            &[top, foot],
            (0.13, 0.13),
            if near { 10 } else { 5 },
            plain(DARK),
        );
        if near {
            b.cylinder(foot, (0.32, 0.28), 0.25, 10, true, c3(CONCRETE));
            let n = round(TAU * (k as f64 + 1.5) / legs as f64);
            strut(
                b,
                foot + DVec3::Y * 0.6,
                at + n * (TANK_R * 0.86) + DVec3::Y * (TANK_Y - 0.6),
                0.06,
                c3(DARK),
            );
        }
    }
    if near {
        // A filler valve on top, and the drain pipe going down into the
        // trench.
        b.cylinder(
            c + DVec3::Y * (TANK_R - 0.05),
            (0.35, 0.3),
            0.35,
            12,
            true,
            c3(CHROME),
        );
        tube(
            b,
            &[
                c - DVec3::Y * (TANK_R - 0.05),
                at + DVec3::Y * 0.6,
                at + DVec3::Y * 0.02,
            ],
            (0.16, 0.16),
            10,
            metal(CHROME),
        );
        // A ladder up the side facing the rocket.
        let d = (-at).normalize_or(DVec3::X);
        let side = DVec3::new(d.z, 0.0, -d.x) * 0.25;
        let rail: Vec<DVec3> = (0..=10)
            .map(|i| {
                let t = -FRAC_PI_2 * 0.9 + PI * 0.75 * i as f64 / 10.0;
                c + d * ((TANK_R + 0.35) * t.cos()) + DVec3::Y * ((TANK_R + 0.35) * t.sin())
            })
            .collect();
        for s in [-1.0, 1.0] {
            let r: Vec<DVec3> = rail.iter().map(|p| *p + side * s).collect();
            tube(b, &r, (0.03, 0.03), 5, plain(DARK));
        }
        for p in rail.iter().step_by(1) {
            strut(b, *p - side, *p + side, 0.04, c3(DARK));
        }
    }
}

/// A floodlight mast: a tapering pole with a rack of lamps facing the
/// rocket.
fn mast(b: &mut MeshBuilder, at: DVec2, near: bool) {
    let foot = DVec3::new(at.x, 0.0, at.y);
    lathe(
        b,
        foot,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.55, 0.0),
            (0.55, 0.5),
            (0.3, 0.7),
            (0.18, MAST_H - 0.6),
            (0.0, MAST_H - 0.6),
        ],
        if near { 16 } else { 6 },
        |k| if k < 2 { plain(CONCRETE) } else { plain(WHITE) },
    );
    let to = (-foot).normalize_or(DVec3::Z);
    let side = DVec3::new(to.z, 0.0, -to.x);
    let head = foot + DVec3::Y * MAST_H;
    b.oriented_box(head, side, DVec3::new(1.0, 0.75, 0.15), c3(DARK));
    if near {
        // Six lamps in two rows, tipped down at the pad.
        let lens = rgb(232, 236, 240);
        for row in 0..2 {
            for k in 0..3 {
                let p = head
                    + side * (-0.62 + 0.62 * k as f64)
                    + DVec3::Y * (0.32 - 0.64 * row as f64)
                    + to * 0.2;
                let n = (to - DVec3::Y * 0.35).normalize();
                lathe(
                    b,
                    p,
                    n,
                    &[(0.0, -0.1), (0.24, -0.1), (0.27, 0.15), (0.0, 0.15)],
                    12,
                    |_| plain(DARK),
                );
                with(b, Surf::Glass, |b| {
                    let (u, v) = (side * 0.22, n.cross(side).normalize() * 0.22);
                    disc(b, p + n * 0.16, (u, v), 12, n, lens);
                });
            }
        }
    }
}

// ------------------------------------------------------------ the rocket

/// The hull's profile, (radius, height over the fins' feet), tail to tip.
fn hull_profile() -> Vec<(f64, f64)> {
    let r = ROCKET_R;
    let shoulder = 19.5;
    let tip = ROCKET_H - 1.0;
    let mut v = vec![
        (0.6, 2.05),
        (1.15, 2.2),
        (1.6, 2.6),
        (1.88, 3.3),
        (1.98, 4.2),
        (r, 5.0),
        (r, 8.0),
        (r, 11.0),
        (r, 14.0),
        (r, 15.6),
        (r, 16.6),
        (r, 17.4),
        (r, 19.0),
        (r, shoulder),
    ];
    // A tangent ogive up to the nose's tip.
    let n = 14;
    for i in 1..=n {
        let t = i as f64 / n as f64;
        let y = shoulder + (tip - shoulder) * t;
        let k = (1.0 - t * t).max(0.0).powf(0.62);
        v.push(((r * k).max(0.1), y));
    }
    v
}

/// The rocket, standing on its fins' feet at the origin, its hatch on +x.
pub fn rocket(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    rocket_into(&mut b, lod);
    b.finish()
}

fn rocket_into(b: &mut MeshBuilder, lod: Lod) {
    let near = lod == Lod::Near;
    let seg = lod.pick(72, 24, 10);
    let profile = hull_profile();
    let rows: Vec<Vec<DVec3>> = profile
        .iter()
        .map(|&(r, y)| {
            (0..seg)
                .map(|j| round(TAU * j as f64 / seg as f64) * r + DVec3::Y * y)
                .collect()
        })
        .collect();
    // The hull cream with a red nose; two red flashes down its sides
    // (towards and away from the town), the tail cone bare metal.
    let stripe = |j: usize| {
        let a = TAU * (j as f64 + 0.5) / seg as f64;
        let off = (a % PI).min(PI - a % PI);
        off < 0.16
    };
    surface(b, &rows, true, |i, j| {
        let y = profile[i].1;
        if y < 4.2 {
            metal(CHROME)
        } else if y >= 19.5 || (stripe(j) && (5.0..14.0).contains(&y)) {
            plain(RED)
        } else {
            plain(HULL)
        }
    });
    let segs = lod.pick(56, 24, 10);
    // Chrome bands: at the shoulder and the tail, and panel seams.
    let band = |b: &mut MeshBuilder, y: f64, w: f64, col: (u8, u8, u8)| {
        lathe(
            b,
            DVec3::Y * y,
            DVec3::Y,
            &[
                (ROCKET_R - 0.01, -w),
                (ROCKET_R + 0.05, -w * 0.6),
                (ROCKET_R + 0.05, w * 0.6),
                (ROCKET_R - 0.01, w),
            ],
            segs,
            |_| metal(col),
        );
    };
    band(b, 19.3, 0.2, CHROME);
    band(b, 5.0, 0.15, CHROME);
    if near {
        band(b, 10.0, 0.04, (200, 196, 186));
        band(b, 14.6, 0.04, (200, 196, 186));
    }
    // The chrome needle on the nose.
    let tip = ROCKET_H - 1.0;
    lathe(
        b,
        DVec3::Y * (tip - 0.15),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.16, 0.0),
            (0.09, 0.4),
            (0.03, 1.15),
            (0.0, 1.15),
        ],
        lod.pick(16, 8, 4),
        |_| metal(CHROME),
    );
    // The engine's bell under the tail, sooty inside.
    let bell = [
        (0.62, 2.1),
        (0.55, 1.75),
        (0.72, 1.3),
        (0.98, 0.95),
        (1.12, 0.8),
    ];
    lathe(
        b,
        DVec3::ZERO,
        DVec3::Y,
        &bell.map(|(r, y)| (r, y)),
        segs,
        |_| metal(DARK),
    );
    inside(|| {
        lathe(
            b,
            DVec3::ZERO,
            DVec3::Y,
            &bell.map(|(r, y)| (r - 0.05, y)),
            segs,
            |_| plain(SOOT),
        )
    });
    b.polygon(
        &(0..segs)
            .map(|k| round(TAU * k as f64 / segs as f64) * 0.55 + DVec3::Y * 1.9)
            .collect::<Vec<_>>(),
        -DVec3::Y,
        c3(SOOT),
    );
    for a in FIN_ANGLES {
        fin(b, a.to_radians(), lod);
    }
    if lod == Lod::Far {
        return;
    }
    // Portholes round the cabin, lit warm from inside.
    for a in [0.0f64, 150.0, 210.0] {
        porthole(b, a.to_radians(), 16.6, 0.42, lod);
    }
    // The hatch on +x, where the crew arm docks.
    hatch(b, lod);
    if near {
        // A row of small vents under the shoulder that steam before a
        // launch, and fuel-line couplings for the umbilical arm.
        for k in 0..8 {
            let a = TAU * (k as f64 + 0.25) / 8.0;
            let d = round(a);
            b.oriented_box(
                d * (ROCKET_R + 0.01) + DVec3::Y * 18.3,
                DVec3::new(d.z, 0.0, -d.x),
                DVec3::new(0.18, 0.08, 0.04),
                c3(DARK),
            );
        }
        let d = round(FRAC_PI_2);
        for y in [7.6, 8.4] {
            lathe(
                b,
                d * (ROCKET_R - 0.05) + DVec3::Y * y,
                d,
                &[
                    (0.0, 0.0),
                    (0.2, 0.0),
                    (0.2, 0.12),
                    (0.12, 0.16),
                    (0.0, 0.16),
                ],
                12,
                |_| metal(CHROME),
            );
        }
    }
}

/// One fin: a thick swept blade from the tail down to a chrome foot on
/// the launch ring, a lens-shaped section, red.
fn fin(b: &mut MeshBuilder, a: f64, lod: Lod) {
    let d = round(a);
    let t = DVec3::new(d.z, 0.0, -d.x);
    let stations = lod.pick(18, 6, 3);
    let around = lod.pick(24, 8, 4);
    let mut rows = Vec::new();
    for i in 0..=stations {
        let s = i as f64 / stations as f64;
        let r = 1.7 + (FIN_REACH - 0.06 - 1.7) * s;
        let yb = 2.4 + (0.55 - 2.4) * s;
        let yt = 11.0 + (3.2 - 11.0) * s.powf(0.6);
        let th = if i == stations {
            0.0
        } else {
            0.3 + (0.1 - 0.3) * s
        };
        let row: Vec<DVec3> = (0..around)
            .map(|k| {
                let q = TAU * k as f64 / around as f64;
                let u = (1.0 - q.cos()) * 0.5;
                d * r + DVec3::Y * (yb + (yt - yb) * u) + t * (th * q.sin())
            })
            .collect();
        rows.push(row);
    }
    surface(b, &rows, true, |_, _| plain(RED));
    // The foot: a chrome shoe on the launch ring, and a strut up into the
    // tail.
    let foot = d * FIN_REACH;
    lathe(
        b,
        foot,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.42, 0.0),
            (0.42, 0.12),
            (0.28, 0.4),
            (0.14, 0.75),
            (0.0, 0.8),
        ],
        lod.pick(16, 8, 4),
        |_| metal(CHROME),
    );
    if lod != Lod::Far {
        // A chrome cap along the leading edge's tip.
        tube(
            b,
            &[
                d * (FIN_REACH - 0.25) + DVec3::Y * 3.0,
                d * (FIN_REACH - 0.06) + DVec3::Y * 0.9,
            ],
            (0.07, 0.07),
            8,
            metal(CHROME),
        );
    }
}

/// A round window in the hull at `a` (radians from +z), `y` up.
fn porthole(b: &mut MeshBuilder, a: f64, y: f64, r: f64, lod: Lod) {
    let n = round(a);
    let c = n * ROCKET_R + DVec3::Y * y;
    let seg = lod.pick(20, 10, 6);
    lathe(
        b,
        c - n * 0.06,
        n,
        &[
            (r - 0.02, 0.0),
            (r + 0.1, 0.02),
            (r + 0.1, 0.1),
            (r - 0.04, 0.13),
            (r - 0.06, 0.06),
        ],
        seg,
        |_| metal(CHROME),
    );
    let (u, v) = (DVec3::new(n.z, 0.0, -n.x), DVec3::Y);
    with(b, Surf::Glow, |b| {
        disc(
            b,
            c + n * 0.07,
            (u * (r - 0.04), v * (r - 0.04)),
            seg,
            n,
            rgb(255, 214, 150),
        )
    });
}

/// The crew hatch on +x: a chrome rim, a cream door with a wheel.
fn hatch(b: &mut MeshBuilder, lod: Lod) {
    let n = round(FRAC_PI_2);
    let y = ARM_HEIGHTS[1] - STAND_H + 1.2;
    let c = n * ROCKET_R + DVec3::Y * y;
    let seg = lod.pick(24, 12, 6);
    lathe(
        b,
        c - n * 0.05,
        n,
        &[
            (0.0, 0.0),
            (0.82, 0.0),
            (0.82, 0.1),
            (0.7, 0.14),
            (0.0, 0.12),
        ],
        seg,
        |k| if k < 2 { metal(CHROME) } else { plain(HULL) },
    );
    if lod == Lod::Near {
        let w: Vec<DVec3> = (0..=20)
            .map(|i| {
                let a = TAU * i as f64 / 20.0;
                c + n * 0.18 + DVec3::Y * (0.3 * a.sin()) + DVec3::Z * (0.3 * a.cos())
            })
            .collect();
        tube(b, &w, (0.035, 0.035), 6, metal(CHROME));
    }
}

// ------------------------------------------------------------ moving parts

/// A swing arm, in its hinge's frame: the deck at y 0, reaching along -x
/// to the hull. `k` 0 is the lower arm (fuel lines), 1 the crew's.
pub fn arm(k: usize, lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let near = lod == Lod::Near;
    let len = ARM_LEN;
    let w = ARM_HALF_W;
    // The hinge post on the tower's face.
    b.cylinder(
        DVec3::new(0.0, -0.6, 0.0),
        (0.22, 0.22),
        if k == 1 { 3.4 } else { 2.0 },
        lod.pick(14, 8, 5),
        true,
        c3(CHROME),
    );
    // A truss either side under the deck.
    for s in [-1.0, 1.0] {
        let z = s * w;
        strut(
            &mut b,
            DVec3::new(0.0, -0.3, z),
            DVec3::new(-len, -0.3, z),
            0.16,
            c3(ORANGE),
        );
        strut(
            &mut b,
            DVec3::new(0.0, 0.85, z),
            DVec3::new(-len, 0.85, z),
            0.12,
            c3(ORANGE),
        );
        if near {
            for i in 0..4 {
                let (x0, x1) = (-len * i as f64 / 4.0, -len * (i + 1) as f64 / 4.0);
                strut(
                    &mut b,
                    DVec3::new(x0, -0.3, z),
                    DVec3::new(x1, 0.85, z),
                    0.08,
                    c3(ORANGE),
                );
            }
        }
    }
    b.oriented_box(
        DVec3::new(-len * 0.5, -0.12, 0.0),
        DVec3::X,
        DVec3::new(len * 0.5, 0.06, w),
        c3(DARK),
    );
    if k == 1 {
        // The crew's walkway: a rounded white tube with portholes, ending
        // in the "white room" at the hatch.
        let n = lod.pick(18, 10, 6);
        let arch = |x: f64| -> Vec<DVec3> {
            (0..=n)
                .map(|i| {
                    let t = PI * i as f64 / n as f64;
                    DVec3::new(x, 1.15 + 1.0 * t.sin(), -w * t.cos())
                })
                .collect()
        };
        let mut rows = Vec::new();
        for i in 0..=4 {
            rows.push(arch(-len * i as f64 / 4.0 * 0.98));
        }
        let shell: Vec<Vec<DVec3>> = rows
            .iter()
            .map(|r| {
                let mut v = vec![r[0] - DVec3::Y * 1.15];
                v.extend(r.iter().copied());
                v.push(r[n] - DVec3::Y * 1.15);
                v
            })
            .collect();
        // Rows run along the walkway; points across it.
        let shell_t: Vec<Vec<DVec3>> = (0..shell[0].len())
            .map(|j| shell.iter().map(|r| r[j]).collect())
            .collect();
        surface(&mut b, &shell_t, false, |_, _| plain(WHITE));
        if near {
            for i in 1..4 {
                for s in [-1.0, 1.0] {
                    let c = DVec3::new(-len * i as f64 / 4.0, 1.4, s * (w + 0.01));
                    with(&mut b, Surf::Glass, |b| {
                        disc(
                            b,
                            c,
                            (DVec3::X * 0.22, DVec3::Y * 0.22),
                            12,
                            DVec3::Z * s,
                            rgb(70, 100, 120),
                        )
                    });
                }
            }
        }
        // A red stripe along its roof.
        b.oriented_box(
            DVec3::new(-len * 0.5, 2.17, 0.0),
            DVec3::X,
            DVec3::new(len * 0.5, 0.03, 0.18),
            c3(RED),
        );
    } else {
        // Fuel lines sagging from the arm's end to the couplings.
        for (dz, col) in [(-0.3, CHROME), (0.3, WHITE)] {
            let path: Vec<DVec3> = (0..=10)
                .map(|i| {
                    let t = i as f64 / 10.0;
                    DVec3::new(
                        -len * t,
                        0.2 - 0.9 * (PI * t).sin() - (STAND_H - 1.6) * 0.0,
                        dz * (1.0 - t) + dz * 0.4 * t,
                    )
                })
                .collect();
            tube(&mut b, &path, (0.12, 0.12), lod.pick(10, 6, 4), metal(col));
        }
    }
    // A rubber-lipped plate where it meets the hull.
    b.oriented_box(
        DVec3::new(-len + 0.06, 0.6, 0.0),
        DVec3::X,
        DVec3::new(0.06, 0.95, w + 0.1),
        rgb(30, 30, 32),
    );
    b.finish()
}

/// The engine's flame, hanging down from the nozzle's exit at the origin:
/// a white-hot core in an orange plume with shock diamonds. Stretch it
/// along y with the thrust.
pub fn flame(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    b.surf = Surf::Glow;
    let seg = lod.pick(24, 12, 8);
    lathe(
        &mut b,
        DVec3::ZERO,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (1.05, 0.0),
            (1.35, -2.0),
            (1.45, -5.0),
            (1.1, -9.0),
            (0.55, -13.0),
            (0.0, -16.0),
        ],
        seg,
        |_| glow(FLAME),
    );
    lathe(
        &mut b,
        DVec3::ZERO,
        DVec3::Y,
        &[
            (0.0, 0.05),
            (0.9, 0.05),
            (0.95, -1.5),
            (0.6, -4.0),
            (0.0, -6.5),
        ],
        seg,
        |_| glow(FLAME_CORE),
    );
    // Shock diamonds poking out of the core down the plume.
    for (y, r) in [(-6.8, 0.5), (-9.0, 0.42), (-11.0, 0.32)] {
        lathe(
            &mut b,
            DVec3::Y * y,
            DVec3::Y,
            &[(0.0, 0.8), (r, 0.0), (0.0, -0.8)],
            seg / 2,
            |_| glow(FLAME_CORE),
        );
    }
    b.surf = Surf::Plain;
    b.finish()
}

/// The seven segments of a board digit (a..g), each as its bar's middle
/// and half size in the digit's frame (x right, y up, origin its middle).
pub const SEGMENTS: [(f64, f64, f64, f64); 7] = [
    (0.0, 1.05, 0.5, 0.13),     // a top
    (0.62, 0.53, 0.13, 0.45),   // b top right
    (0.62, -0.53, 0.13, 0.45),  // c bottom right
    (0.0, -1.05, 0.5, 0.13),    // d bottom
    (-0.62, -0.53, 0.13, 0.45), // e bottom left
    (-0.62, 0.53, 0.13, 0.45),  // f top left
    (0.0, 0.0, 0.5, 0.13),      // g middle
];

/// Which segments light for each digit.
pub fn lit(d: u32) -> [bool; 7] {
    let m: u8 = match d % 10 {
        0 => 0b0111111,
        1 => 0b0000110,
        2 => 0b1011011,
        3 => 0b1001111,
        4 => 0b1100110,
        5 => 0b1101101,
        6 => 0b1111101,
        7 => 0b0000111,
        8 => 0b1111111,
        _ => 0b1101111,
    };
    std::array::from_fn(|k| m & (1 << k) != 0)
}

/// One digit segment (glowing, facing +z, its middle at the origin): a
/// bar with pointed ends.
pub fn segment(k: usize) -> Mesh {
    let (_, _, hx, hy) = SEGMENTS[k];
    let mut b = MeshBuilder::new();
    b.surf = Surf::Glow;
    let along_x = hx > hy;
    let (l, t) = if along_x { (hx, hy) } else { (hy, hx) };
    let p = |a: f64, c: f64, z: f64| {
        if along_x {
            DVec3::new(a, c, z)
        } else {
            DVec3::new(c, a, z)
        }
    };
    let outline = [
        p(-l, 0.0, 0.0),
        p(-l + t, -t, 0.0),
        p(l - t, -t, 0.0),
        p(l, 0.0, 0.0),
        p(l - t, t, 0.0),
        p(-l + t, t, 0.0),
    ];
    b.polygon(&outline, DVec3::Z, c3(DIGIT));
    b.surf = Surf::Plain;
    b.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_models_have_faces_and_budgets() {
        let tris = |m: &Mesh| m.face_count();
        let near = tris(&rocket(Lod::Near));
        let mid = tris(&rocket(Lod::Mid));
        let far = tris(&rocket(Lod::Far));
        // A close-up model, a fifth or so in the middle, a few hundred far.
        assert!((5_000..20_000).contains(&near), "near {near}");
        assert!(mid < near / 2, "mid {mid}");
        assert!(far < 1_500, "far {far}");
        for k in 0..2 {
            assert!(tris(&arm(k, Lod::Near)) > 100);
        }
        assert!(tris(&flame(Lod::Near)) > 100);
        for k in 0..7 {
            assert_eq!(tris(&segment(k)), 4);
        }
    }

    #[test]
    fn digits_light_their_segments() {
        assert_eq!(lit(8), [true; 7]);
        assert_eq!(lit(1).iter().filter(|x| **x).count(), 2);
        assert!(!lit(0)[6], "no middle bar in a zero");
    }

    #[test]
    fn the_hull_is_smooth_and_round() {
        let p = hull_profile();
        for w in p.windows(2) {
            assert!(w[1].1 > w[0].1, "the profile climbs");
            assert!((w[1].0 - w[0].0).abs() < 0.7, "no step in the hull");
        }
        assert!((p.last().unwrap().1 - (ROCKET_H - 1.0)).abs() < 1e-9);
    }
}
