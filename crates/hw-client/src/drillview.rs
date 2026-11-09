//! The mole folk's drill rig ([`hw_gen::urban::drillrig`]): its model, and
//! the rig at work.
//!
//! The model is a riveted derrick in red oxide and brass over an iron bore
//! collar, a gearbox hung under the crown with its steam motor, a green
//! enamelled boiler on a brick furnace with a flywheel and a tall banded
//! smokestack, and the mole folk's glowing toadstool lamps round the
//! plinth.
//!
//! At work ([`DrillView`]): the fluted drill turns in the bore and the
//! ring gear and flywheel with it; steam wisps from the collar and smoke
//! from the stack. Every half minute or so (the cycle is
//! [`drillrig::phase`] of the game clock, so every player sees the same)
//! the bore rumbles: the drill races and shakes, the vents spit, the
//! beacon flashes and a whistle blows; then the drill is hauled up and the
//! geyser roars out of the bore, a column of steam thirty metres high.
//!
//! The moving and glowing parts are instanced entities made once when the
//! camera comes within [`LOAD`]; moving one is giving it a new place.

use crate::carmesh::{lathe, spline, tube, Paint};
use crate::citymesh::{MeshBuilder, Surf};
use crate::landmarkview::strut;
use engine::prelude::*;
use glam::{DVec2, DVec3, Vec3};
use hw_gen::urban::drillrig::{self as rig, Phase};
use hw_gen::urban::landmarks::{Landmark, LandmarkKind};
use hw_gen::urban::world::TownWorld;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// The live parts are made within this distance of the rig (m) and
/// dropped past [`DROP`]; they play only within [`ACTIVE`].
const LOAD: f32 = 1000.0;
const DROP: f32 = 1200.0;
const ACTIVE: f32 = 700.0;

// The rig's colours (sRGB, picked by eye).
fn oxide() -> Color {
    rgb(122, 46, 32)
}
fn oxide_dark() -> Color {
    rgb(84, 34, 26)
}
fn brass() -> Color {
    rgb(200, 152, 72)
}
fn iron() -> Color {
    rgb(46, 44, 42)
}
fn steel() -> Color {
    rgb(168, 172, 176)
}
fn enamel() -> Color {
    rgb(38, 84, 64)
}
/// The heat in the bore.
const BORE: (u8, u8, u8) = (255, 120, 40);
/// The beacon's amber.
const BEACON: (u8, u8, u8) = (255, 176, 60);

fn metal(c: Color) -> Paint {
    Some((Surf::Metal, c))
}
fn plain(c: Color) -> Paint {
    Some((Surf::Plain, c))
}
fn v3(p: DVec2, y: f64) -> DVec3 {
    DVec3::new(p.x, y, p.y)
}

/// A leg's point at height `y`, in the rig's frame.
fn leg(k: usize, y: f64) -> DVec3 {
    v3(rig::leg_at(k, y), y)
}

/// A round-headed rivet on a surface at `c` facing `n`.
fn rivet(b: &mut MeshBuilder, c: DVec3, n: DVec3, r: f64, col: Color) {
    lathe(
        b,
        c - n * 0.01,
        n,
        &[
            (r, 0.0),
            (r * 0.85, r * 0.45),
            (r * 0.45, r * 0.7),
            (0.0, r * 0.75),
        ],
        8,
        |_| metal(col),
    );
}

/// The static model, in the rig's frame (origin on the plinth's top, +z
/// its front). Near: every part and rivet; far: the silhouette. The parts
/// that move or glow with the cycle are [`DrillView`]'s, except far away,
/// where the far model carries the glow.
pub fn model(b: &mut MeshBuilder, l: &Landmark, near: bool) {
    deck(b, near);
    collar(b, near);
    derrick(b, near);
    crown(b, near);
    gearbox(b, near);
    boiler(b, near);
    stack(b, near);
    if near {
        steam_pipe(b);
        lamps(b, l.plaza);
    } else {
        // Far: the bore's glow and the beacon, which the live view lights
        // when near.
        bore_glow(b, false);
        beacon_lens(b, false);
    }
}

/// Iron deck plates round the bore, flush on the paving, and footings
/// under the legs.
fn deck(b: &mut MeshBuilder, near: bool) {
    let h = rig::LEG_FOOT + 1.3;
    if near {
        b.tex = Some(crate::skins::Tex::from(crate::skins::Skin::new(
            crate::skins::Base::Facade(engine::texgen::FacadeLayer::MetalPanel),
            crate::skins::Wear::Grimy,
        )));
    }
    // An octagon of plates, its corners cut back to the legs.
    let pts: Vec<DVec2> = (0..8)
        .map(|k| {
            let a = TAU * (k as f64 + 0.5) / 8.0;
            DVec2::new(a.sin(), a.cos()) * h / (PI / 8.0).cos()
        })
        .collect();
    b.prism(&pts, 0.0, 0.05, iron().lerp(oxide_dark(), 0.3));
    b.tex = None;
    for k in 0..4 {
        let f = leg(k, 0.0);
        b.oriented_box(
            f + DVec3::Y * 0.3,
            DVec3::X,
            DVec3::new(0.85, 0.3, 0.85),
            rgb(128, 120, 108),
        );
        if near {
            // Anchor bolts with brass nuts.
            for (dx, dz) in [(-0.6, -0.6), (0.6, -0.6), (0.6, 0.6), (-0.6, 0.6)] {
                b.cylinder(
                    f + DVec3::new(dx, 0.6, dz),
                    (0.08, 0.08),
                    0.12,
                    8,
                    true,
                    brass(),
                );
            }
            // Mole crystals growing round two of the feet.
            if k % 2 == 0 {
                b.surf = Surf::Glow;
                let col = if k == 0 {
                    rgb(120, 255, 210)
                } else {
                    rgb(170, 140, 255)
                };
                let out = DVec2::new(f.x, f.z).normalize() * 1.05;
                crate::undergroundview::crystal(
                    b,
                    DVec2::new(f.x, f.z) + out.perp() * 0.5,
                    0.05,
                    1.1,
                    col,
                );
                b.surf = Surf::Plain;
            }
        }
    }
}

/// The bore's collar: an iron drum with a rolled brass lip, riveted bands
/// and four vent pipes with valve wheels on its sides. Its inside goes
/// down to the glowing floor.
fn collar(b: &mut MeshBuilder, near: bool) {
    let seg = if near { 64 } else { 16 };
    let (r, h) = (rig::COLLAR_R, rig::COLLAR_H);
    let ri = rig::BIT_R + 0.35;
    let prof = [
        (r + 0.25, 0.0),
        (r + 0.25, 0.18),
        (r, 0.22),
        (r, h - 0.2),
        (r + 0.12, h - 0.12),
        (r + 0.12, h),
        (r - 0.05, h + 0.06),
        (ri + 0.05, h + 0.06),
        (ri, h - 0.05),
        (ri, 0.2),
        (0.0, 0.2),
    ];
    lathe(b, DVec3::ZERO, DVec3::Y, &prof, seg, |i| match i {
        3..=6 => metal(brass().lerp(iron(), 0.25)),
        8 | 9 => plain(rgb(24, 20, 18)),
        _ => metal(iron()),
    });
    if !near {
        return;
    }
    // Two riveted bands round the drum.
    for y in [0.55, 1.0] {
        lathe(
            b,
            DVec3::Y * (y - 0.07),
            DVec3::Y,
            &[(r, 0.0), (r + 0.05, 0.02), (r + 0.05, 0.12), (r, 0.14)],
            seg,
            |_| metal(iron().lerp(steel(), 0.2)),
        );
        for k in 0..32 {
            let a = TAU * k as f64 / 32.0;
            let n = DVec3::new(a.sin(), 0.0, a.cos());
            rivet(b, n * (r + 0.05) + DVec3::Y * y, n, 0.045, brass());
        }
    }
    // The vents: an elbow out of the drum, up, with a valve wheel.
    for (_, at, up) in vents() {
        let n = DVec3::new(at.x, 0.0, at.z).normalize();
        let base = n * (r - 0.05) + DVec3::Y * 0.7;
        let path = spline(&[base, base + n * 0.35, at - DVec3::Y * 0.35, at], 4, false);
        tube(b, &path, (0.14, 0.14), 14, metal(iron().lerp(steel(), 0.3)));
        lathe(
            b,
            at - up * 0.05,
            up,
            &[(0.21, 0.0), (0.21, 0.08), (0.17, 0.1), (0.0, 0.1)],
            14,
            |_| metal(brass()),
        );
        valve_wheel(b, base + n * 0.25 + DVec3::Y * 0.22, DVec3::Y, 0.22);
    }
}

/// The four vents on the collar: (index, mouth, the way it blows).
fn vents() -> [(usize, DVec3, DVec3); 4] {
    let mut out = [(0, DVec3::ZERO, DVec3::Y); 4];
    for (k, o) in out.iter_mut().enumerate() {
        let a = TAU * (k as f64 + 0.5) / 4.0 + 0.4;
        let n = DVec3::new(a.sin(), 0.0, a.cos());
        *o = (
            k,
            n * (rig::COLLAR_R + 0.42) + DVec3::Y * (rig::COLLAR_H + 0.3),
            (n * 0.35 + DVec3::Y).normalize(),
        );
    }
    out
}

/// A spoked valve wheel at `c` turning about `axis`.
fn valve_wheel(b: &mut MeshBuilder, c: DVec3, axis: DVec3, r: f64) {
    let (u, v) = basis(axis);
    let ring: Vec<DVec3> = (0..=24)
        .map(|k| {
            let a = TAU * k as f64 / 24.0;
            c + (u * a.cos() + v * a.sin()) * r
        })
        .collect();
    tube(b, &ring, (0.03, 0.03), 6, metal(oxide()));
    for k in 0..3 {
        let a = TAU * k as f64 / 3.0;
        strut(b, c, c + (u * a.cos() + v * a.sin()) * r, 0.03, oxide());
    }
    lathe(
        b,
        c - axis * 0.05,
        axis,
        &[(0.05, 0.0), (0.05, 0.1), (0.0, 0.1)],
        8,
        |_| metal(brass()),
    );
}

/// Two unit vectors at right angles to `axis`.
fn basis(axis: DVec3) -> (DVec3, DVec3) {
    let axis = axis.normalize();
    let helper = if axis.y.abs() < 0.9 {
        DVec3::Y
    } else {
        DVec3::X
    };
    let u = axis.cross(helper).normalize();
    (u, axis.cross(u).normalize())
}

/// The derrick: four box legs leaning in, girts and cross braces in bays,
/// riveted gusset plates at the joints, a ladder up the front right leg.
fn derrick(b: &mut MeshBuilder, near: bool) {
    let w = if near { 0.42 } else { 0.8 };
    let top = rig::CROWN_Y;
    for k in 0..4 {
        strut(b, leg(k, 0.6), leg(k, top), w, oxide());
        if near {
            // A flange stripe down the outside of each leg.
            let out = DVec3::new(leg(k, 0.0).x, 0.0, leg(k, 0.0).z).normalize() * (w * 0.5);
            strut(
                b,
                leg(k, 0.6) + out,
                leg(k, top) + out,
                w * 0.25,
                oxide_dark(),
            );
        }
    }
    let bays = if near { 6 } else { 3 };
    for i in 0..bays {
        let y0 = 0.6 + (top - 0.6) * i as f64 / bays as f64;
        let y1 = 0.6 + (top - 0.6) * (i + 1) as f64 / bays as f64;
        for k in 0..4 {
            let j = (k + 1) % 4;
            if i + 1 < bays {
                strut(b, leg(k, y1), leg(j, y1), w * 0.6, oxide());
            }
            if near {
                strut(b, leg(k, y0), leg(j, y1), w * 0.32, oxide_dark());
                strut(b, leg(j, y0), leg(k, y1), w * 0.32, oxide_dark());
                // A gusset plate with brass rivets where the braces meet
                // the leg.
                let c = leg(k, y1);
                let n = DVec3::new(c.x, 0.0, c.z).normalize();
                let side = DVec3::new(n.z, 0.0, -n.x);
                for s in [side, -side] {
                    let p = c + s * (w * 0.5 + 0.02);
                    b.oriented_box(p, n, DVec3::new(0.36, 0.36, 0.02), oxide());
                    for (dy, dn) in [(-0.2, -0.2), (0.2, -0.2), (-0.2, 0.2), (0.2, 0.2)] {
                        rivet(b, p + DVec3::Y * dy + n * dn + s * 0.03, s, 0.035, brass());
                    }
                }
            }
        }
    }
    if near {
        ladder(b);
    }
}

/// A ladder up the outside of the front right leg.
fn ladder(b: &mut MeshBuilder) {
    let k = 0;
    let top = rig::CROWN_Y + 0.2;
    let out = |y: f64| {
        let p = leg(k, y);
        DVec3::new(p.x, 0.0, p.z).normalize()
    };
    let at = |y: f64, s: f64| {
        let n = out(y);
        let side = DVec3::new(n.z, 0.0, -n.x);
        leg(k, y) + n * 0.55 + side * s
    };
    for s in [-0.24, 0.24] {
        tube(
            b,
            &[at(0.1, s), at(top, s)],
            (0.035, 0.035),
            6,
            metal(iron()),
        );
    }
    let mut y = 0.4;
    while y < top {
        strut(b, at(y, -0.24), at(y, 0.24), 0.04, iron());
        y += 0.32;
    }
    // Stand-offs to the leg every few metres.
    let mut y = 1.5;
    while y < top {
        for s in [-0.24, 0.24] {
            strut(
                b,
                at(y, s),
                leg(k, y) + (at(y, s) - leg(k, y)) * 0.3,
                0.05,
                iron(),
            );
        }
        y += 3.0;
    }
}

/// The crown: a deck on the legs with a railing, the sheave wheel on its
/// bearings, and the beacon's cage.
fn crown(b: &mut MeshBuilder, near: bool) {
    let y = rig::CROWN_Y;
    let half = rig::LEG_TOP_AT + 0.6;
    b.oriented_box(
        DVec3::Y * y,
        DVec3::X,
        DVec3::new(half, 0.25, half),
        oxide(),
    );
    // Hangers down to the gearbox.
    for (x, z) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let a = DVec3::new(x * 1.6, y - 0.25, z * 1.6);
        let c = DVec3::new(x * 1.2, rig::GEAR_Y1 + 0.1, z * 1.2);
        if near {
            tube(b, &[a, c], (0.05, 0.05), 6, metal(steel()));
        } else {
            strut(b, a, c, 0.15, iron());
        }
    }
    if near {
        // A railing round the deck.
        let r = half - 0.1;
        let corners = [
            DVec3::new(-r, 0.0, -r),
            DVec3::new(r, 0.0, -r),
            DVec3::new(r, 0.0, r),
            DVec3::new(-r, 0.0, r),
        ];
        let deck = DVec3::Y * (y + 0.25);
        for i in 0..4 {
            let (p, q) = (corners[i] + deck, corners[(i + 1) % 4] + deck);
            for t in [0.0, 0.5] {
                let m = p + (q - p) * t;
                tube(
                    b,
                    &[m, m + DVec3::Y * 1.05],
                    (0.04, 0.04),
                    6,
                    metal(oxide()),
                );
            }
            for h in [0.55, 1.05] {
                tube(
                    b,
                    &[p + DVec3::Y * h, q + DVec3::Y * h],
                    (0.035, 0.035),
                    6,
                    metal(oxide()),
                );
            }
        }
        // The sheave: a grooved wheel on two bearing blocks.
        let c = DVec3::new(0.0, y + 1.35, -0.4);
        let prof: Vec<(f64, f64)> = vec![
            (0.15, -0.22),
            (1.0, -0.22),
            (1.0, -0.12),
            (0.88, -0.04),
            (0.88, 0.04),
            (1.0, 0.12),
            (1.0, 0.22),
            (0.15, 0.22),
        ];
        lathe(b, c, DVec3::X, &prof, 40, |_| metal(oxide_dark()));
        for s in [-1.0, 1.0] {
            b.oriented_box(
                DVec3::new(s * 0.42, y + 0.8, -0.4),
                DVec3::X,
                DVec3::new(0.12, 0.55, 0.3),
                iron(),
            );
        }
        lathe(
            b,
            c - DVec3::X * 0.55,
            DVec3::X,
            &[(0.12, 0.0), (0.12, 1.1), (0.0, 1.1)],
            12,
            |_| metal(brass()),
        );
    }
    // The beacon's cage on a post at the deck's front corner.
    let at = DVec3::new(half - 0.4, y + 0.25, half - 0.4);
    b.cylinder(
        at,
        (0.12, 0.12),
        1.6,
        if near { 10 } else { 4 },
        false,
        oxide(),
    );
    if near {
        let c = at + DVec3::Y * 2.0;
        lathe(
            b,
            c - DVec3::Y * 0.45,
            DVec3::Y,
            &[
                (0.0, 0.0),
                (0.36, 0.0),
                (0.36, 0.08),
                (0.3, 0.12),
                (0.0, 0.12),
            ],
            20,
            |_| metal(brass()),
        );
        lathe(
            b,
            c + DVec3::Y * 0.42,
            DVec3::Y,
            &[
                (0.0, 0.0),
                (0.36, 0.0),
                (0.3, 0.1),
                (0.12, 0.2),
                (0.0, 0.24),
            ],
            20,
            |_| metal(brass()),
        );
        for k in 0..6 {
            let a = TAU * k as f64 / 6.0;
            let d = DVec3::new(a.sin(), 0.0, a.cos()) * 0.33;
            tube(
                b,
                &[
                    c + d - DVec3::Y * 0.37,
                    c + d * 1.15,
                    c + d + DVec3::Y * 0.42,
                ],
                (0.025, 0.025),
                5,
                metal(brass()),
            );
        }
    }
}

/// Where the beacon's lens is.
fn beacon_at() -> DVec3 {
    let half = rig::LEG_TOP_AT + 0.6;
    DVec3::new(half - 0.4, rig::CROWN_Y + 2.25, half - 0.4)
}

/// The beacon's glowing lens (a hair bigger in the live copy).
fn beacon_lens(b: &mut MeshBuilder, live: bool) {
    let c = beacon_at();
    let r = 0.27 + if live { 0.01 } else { 0.0 };
    let prof: Vec<(f64, f64)> = (0..=12)
        .map(|i| {
            let a = PI * i as f64 / 12.0;
            (r * a.sin(), -r * 1.3 * a.cos())
        })
        .collect();
    b.surf = Surf::Glow;
    lathe(b, c, DVec3::Y, &prof, 20, |_| {
        Some((Surf::Glow, c3(BEACON)))
    });
    b.surf = Surf::Plain;
}

fn c3(t: (u8, u8, u8)) -> Color {
    rgb(t.0, t.1, t.2)
}

/// The bore's glowing floor, down inside the collar.
fn bore_glow(b: &mut MeshBuilder, live: bool) {
    let r = rig::BIT_R + 0.35;
    let y = 0.21 + if live { 0.01 } else { 0.0 };
    lathe(
        b,
        DVec3::Y * y,
        DVec3::Y,
        &[(r, 0.0), (r * 0.6, 0.02), (0.0, 0.03)],
        32,
        |_| Some((Surf::Glow, c3(BORE))),
    );
}

/// The gearbox the drill hangs from: a riveted drum with flanges, a
/// bushing for the kelly bar underneath, and its steam motor.
fn gearbox(b: &mut MeshBuilder, near: bool) {
    let seg = if near { 40 } else { 10 };
    let (y0, y1) = (rig::GEAR_Y0, rig::GEAR_Y1);
    let r = 1.45;
    let prof = [
        (0.0, y0 - 0.4),
        (rig::KELLY_R + 0.25, y0 - 0.4),
        (rig::KELLY_R + 0.35, y0 - 0.1),
        (r + 0.15, y0),
        (r + 0.15, y0 + 0.22),
        (r, y0 + 0.28),
        (r, y1 - 0.28),
        (r + 0.15, y1 - 0.22),
        (r + 0.15, y1),
        (0.0, y1),
    ];
    lathe(b, DVec3::ZERO, DVec3::Y, &prof, seg, |i| match i {
        3 | 4 | 7 | 8 => metal(brass().lerp(iron(), 0.2)),
        _ => metal(oxide_dark()),
    });
    let (mx, my, mz) = rig::MOTOR;
    let m = DVec3::new(mx, my, mz);
    lathe(
        b,
        m - DVec3::X * rig::MOTOR_HALF,
        DVec3::X,
        &[
            (0.0, -0.08),
            (rig::MOTOR_R * 0.8, -0.08),
            (rig::MOTOR_R, 0.05),
            (rig::MOTOR_R, rig::MOTOR_HALF * 2.0 - 0.05),
            (rig::MOTOR_R * 0.7, rig::MOTOR_HALF * 2.0 + 0.1),
            (0.0, rig::MOTOR_HALF * 2.0 + 0.1),
        ],
        if near { 28 } else { 8 },
        |i| {
            if i == 2 {
                metal(enamel())
            } else {
                metal(brass())
            }
        },
    );
    if !near {
        return;
    }
    for k in 0..24 {
        let a = TAU * k as f64 / 24.0;
        let n = DVec3::new(a.sin(), 0.0, a.cos());
        for y in [y0 + 0.5, y1 - 0.5] {
            rivet(b, n * r + DVec3::Y * y, n, 0.05, brass());
        }
    }
    // A riveted name plate: the mole folk's mark in brass.
    let n = DVec3::Z;
    b.oriented_box(
        n * (r + 0.02) + DVec3::Y * ((y0 + y1) * 0.5),
        DVec3::X,
        DVec3::new(0.55, 0.32, 0.02),
        brass(),
    );
    for (dx, dy) in [(-0.45, -0.22), (0.45, -0.22), (-0.45, 0.22), (0.45, 0.22)] {
        rivet(
            b,
            DVec3::new(dx, (y0 + y1) * 0.5 + dy, r + 0.05),
            n,
            0.035,
            iron(),
        );
    }
}

/// The ring gear on top of the gearbox, which turns with the drill.
fn ring_gear(b: &mut MeshBuilder) {
    let teeth = 36;
    let (r0, r1, h) = (1.0, 1.32, 0.3);
    let n = teeth * 4;
    let rad = |i: usize| {
        // Square-ish teeth: up for a quarter, down for a quarter.
        match i % 4 {
            0 | 3 => r0 + (r1 - r0) * 0.55,
            _ => r1,
        }
    };
    let rows = |y: f64, k: f64| -> Vec<DVec3> {
        (0..n)
            .map(|i| {
                let a = TAU * i as f64 / n as f64;
                DVec3::new(a.sin(), 0.0, a.cos()) * rad(i) * k + DVec3::Y * y
            })
            .collect()
    };
    crate::carmesh::surface(
        b,
        &[rows(0.0, 0.92), rows(0.05, 1.0), rows(h, 1.0)],
        true,
        |_, _| metal(brass()),
    );
    lathe(
        b,
        DVec3::ZERO,
        DVec3::Y,
        &[
            (r0 * 0.92 + 0.02, h),
            (rig::KELLY_R + 0.3, h + 0.15),
            (0.0, h + 0.15),
        ],
        36,
        |_| metal(brass().lerp(iron(), 0.3)),
    );
}

/// The drill: a fluted cone of bright steel, three spiral cutting flutes
/// round it, a brass collar at its top and a hexagonal kelly bar up into
/// the gearbox. Origin at the bit's tip; it turns about +y.
fn drill(b: &mut MeshBuilder) {
    let len = rig::BIT_LEN;
    let br = rig::BIT_R;
    // Fine along the length: the flutes wind round 2.6 times.
    let along = 110;
    let around = 64;
    let flutes = 3.0;
    let twist = 2.6; // turns from tip to top
    let rows: Vec<Vec<DVec3>> = (0..=along)
        .map(|i| {
            let t = i as f64 / along as f64;
            // A rounded point, then a cone, then a short straight shank.
            let base = if t < 0.08 {
                br * 0.18 * (1.0 - (1.0 - t / 0.08).powi(2)).sqrt()
            } else if t < 0.88 {
                br * (0.18 + 0.82 * (t - 0.08) / 0.8)
            } else {
                br
            };
            (0..around)
                .map(|j| {
                    let a = TAU * j as f64 / around as f64;
                    let ph = a * flutes - TAU * twist * t * flutes;
                    // A sharp blade and a hollow behind it.
                    let s = (ph.sin() * 0.5 + 0.5).powf(1.6);
                    let r = base * (0.8 + 0.2 * s) * if t >= 0.88 { 0.88 } else { 1.0 };
                    DVec3::new(a.sin() * r, t * len, a.cos() * r)
                })
                .collect()
        })
        .collect();
    crate::carmesh::surface(b, &rows, true, |i, j| {
        let a = TAU * j as f64 / around as f64;
        let t = i as f64 / along as f64;
        let ph = a * flutes - TAU * twist * t * flutes;
        let s = (ph.sin() * 0.5 + 0.5).powf(1.6);
        // Bright worn edges on the blades, darker steel in the hollows.
        metal(steel().lerp(rgb(92, 96, 100), (1.0 - s) as f32 * 0.6))
    });
    // The brass collar and the cap the kelly bar comes out of.
    lathe(
        b,
        DVec3::Y * len,
        DVec3::Y,
        &[
            (br * 0.86, -0.05),
            (br * 0.95, 0.05),
            (br * 0.95, 0.35),
            (br * 0.7, 0.55),
            (rig::KELLY_R + 0.2, 0.8),
            (rig::KELLY_R + 0.2, 1.0),
            (0.0, 1.0),
        ],
        48,
        |i| {
            if i <= 1 {
                metal(brass())
            } else {
                metal(oxide_dark())
            }
        },
    );
    for k in 0..16 {
        let a = TAU * k as f64 / 16.0;
        let n = DVec3::new(a.sin(), 0.0, a.cos());
        rivet(b, n * br * 0.95 + DVec3::Y * (len + 0.2), n, 0.06, iron());
    }
    // The kelly bar: six flat faces (as real ones have), up past the
    // gearbox's top at full lift.
    let top = rig::GEAR_Y1 + 0.3 - rig::BIT_TIP_Y;
    let pts: Vec<DVec2> = (0..6)
        .map(|k| {
            let a = TAU * k as f64 / 6.0;
            DVec2::new(a.sin(), a.cos()) * rig::KELLY_R
        })
        .collect();
    b.surf = Surf::Metal;
    b.prism(&pts, len + 0.9, top, rgb(86, 84, 82));
    b.surf = Surf::Plain;
}

/// The boiler: a green enamelled drum with domed ends and brass bands,
/// lying on a brick furnace with two fire doors, a steam dome and safety
/// valve on top, a pressure gauge, and the flywheel's bearings at its end.
fn boiler(b: &mut MeshBuilder, near: bool) {
    let seg = if near { 40 } else { 10 };
    let (y, z, r, half) = (
        rig::BOILER_Y,
        rig::BOILER_Z,
        rig::BOILER_R,
        rig::BOILER_HALF,
    );
    let c = DVec3::new(0.0, y, z);
    let mut prof = Vec::new();
    let dome = 0.7;
    let ends = if near { 8 } else { 3 };
    for i in 0..=ends {
        let a = FRAC_PI_2 * i as f64 / ends as f64;
        prof.push((r * a.sin(), -half - dome * a.cos()));
    }
    for i in 0..=ends {
        let a = FRAC_PI_2 * i as f64 / ends as f64;
        prof.push((r * a.cos(), half + dome * a.sin()));
    }
    lathe(b, c, DVec3::X, &prof, seg, |i| {
        if i == ends {
            metal(enamel())
        } else {
            metal(enamel().lerp(iron(), 0.4))
        }
    });
    // The furnace under it: brick, with an iron top.
    b.tex = near.then(|| crate::skins::Tex::from(engine::texgen::MedievalLayer::Brick));
    b.oriented_box(
        DVec3::new(-0.8, (y - r * 0.55) * 0.5, z),
        DVec3::X,
        DVec3::new(2.4, (y - r * 0.55) * 0.5, r * 0.85),
        rgb(150, 70, 52),
    );
    b.tex = None;
    // An iron saddle under the far end.
    b.oriented_box(
        DVec3::new(2.7, (y - r * 0.6) * 0.5, z),
        DVec3::X,
        DVec3::new(0.25, (y - r * 0.6) * 0.5, r * 0.8),
        iron(),
    );
    // The steam dome and the safety valve.
    let top = c + DVec3::Y * (r - 0.1);
    lathe(
        b,
        top + DVec3::X * -1.6,
        DVec3::Y,
        &[
            (0.75, 0.0),
            (0.6, 0.1),
            (0.55, 0.25),
            (0.55, 0.55),
            (0.45, 0.72),
            (0.2, 0.8),
            (0.0, 0.82),
        ],
        if near { 32 } else { 8 },
        |_| metal(brass()),
    );
    if !near {
        return;
    }
    lathe(
        b,
        top + DVec3::X * 1.2,
        DVec3::Y,
        &[
            (0.16, 0.0),
            (0.16, 0.5),
            (0.24, 0.55),
            (0.24, 0.62),
            (0.0, 0.66),
        ],
        16,
        |_| metal(brass()),
    );
    strut(
        b,
        top + DVec3::new(1.2, 0.64, 0.0),
        top + DVec3::new(2.1, 0.5, 0.0),
        0.06,
        iron(),
    );
    b.cylinder(
        top + DVec3::new(2.1, 0.32, 0.0),
        (0.12, 0.12),
        0.18,
        12,
        true,
        iron(),
    );
    // Brass bands with rivets round the drum.
    for x in [-half + 0.3, -1.2, 1.2, half - 0.3] {
        lathe(
            b,
            c + DVec3::X * (x - 0.08),
            DVec3::X,
            &[(r, 0.0), (r + 0.04, 0.02), (r + 0.04, 0.14), (r, 0.16)],
            seg,
            |_| metal(brass()),
        );
        for k in 0..20 {
            let a = TAU * k as f64 / 20.0;
            let n = DVec3::new(0.0, a.cos(), a.sin());
            rivet(b, c + DVec3::X * x + n * (r + 0.04), n, 0.04, brass());
        }
    }
    // The pressure gauge on the front, facing the drill.
    let g = c + DVec3::new(0.6, 0.45, r * 0.9 + 0.15);
    lathe(
        b,
        g - DVec3::Z * 0.15,
        DVec3::Z,
        &[
            (0.0, 0.0),
            (0.32, 0.0),
            (0.34, 0.1),
            (0.34, 0.16),
            (0.3, 0.17),
        ],
        24,
        |_| metal(brass()),
    );
    let face: Vec<DVec3> = (0..24)
        .map(|k| {
            let a = TAU * k as f64 / 24.0;
            g + DVec3::new(a.cos() * 0.29, a.sin() * 0.29, 0.02)
        })
        .collect();
    b.polygon(&face, DVec3::Z, rgb(236, 228, 206));
    strut(
        b,
        g + DVec3::Z * 0.03,
        g + DVec3::new(0.16, 0.16, 0.03),
        0.025,
        rgb(150, 20, 16),
    );
    tube(
        b,
        &[g - DVec3::Z * 0.15, g - DVec3::new(0.0, 0.35, 0.3)],
        (0.04, 0.04),
        6,
        metal(brass()),
    );
    // The fire doors: iron frames round the glow (the live view's).
    for x in [-2.2, -0.2] {
        let d = DVec3::new(x, 0.55, z + r * 0.85 + 0.02);
        for (dx, dy, hx, hy) in [
            (0.0, 0.42, 0.55, 0.06),
            (0.0, -0.42, 0.55, 0.06),
            (-0.5, 0.0, 0.06, 0.42),
            (0.5, 0.0, 0.06, 0.42),
        ] {
            b.oriented_box(
                d + DVec3::new(dx, dy, 0.04),
                DVec3::X,
                DVec3::new(hx, hy, 0.05),
                iron(),
            );
        }
        // Bars across the grate.
        for k in 0..4 {
            let bx = -0.33 + 0.22 * k as f64;
            b.oriented_box(
                d + DVec3::new(bx, 0.0, 0.05),
                DVec3::X,
                DVec3::new(0.025, 0.38, 0.03),
                iron(),
            );
        }
    }
    // The flywheel's bearings.
    let (fx, fy, _) = rig::FLYWHEEL;
    for s in [-0.38, 0.38] {
        let p = DVec3::new(fx + s, 0.0, z);
        b.hexa(
            [
                p + DVec3::new(-0.08, 0.0, -0.6),
                p + DVec3::new(0.08, 0.0, -0.6),
                p + DVec3::new(0.08, 0.0, 0.6),
                p + DVec3::new(-0.08, 0.0, 0.6),
            ],
            [
                p + DVec3::new(-0.08, fy, -0.18),
                p + DVec3::new(0.08, fy, -0.18),
                p + DVec3::new(0.08, fy, 0.18),
                p + DVec3::new(-0.08, fy, 0.18),
            ],
            iron(),
        );
    }
    // The shaft from the boiler's engine to the flywheel.
    lathe(
        b,
        DVec3::new(-half - 0.4, fy, z),
        -DVec3::X,
        &[
            (0.0, 0.0),
            (0.11, 0.0),
            (0.11, fx.abs() - half + 0.1),
            (0.0, fx.abs() - half + 0.1),
        ],
        12,
        |_| metal(steel()),
    );
}

/// The fire doors' glow (a hair proud of the static frames).
fn fire_glow(b: &mut MeshBuilder) {
    let z = rig::BOILER_Z + rig::BOILER_R * 0.85 + 0.03;
    for x in [-2.2, -0.2] {
        b.surf = Surf::Glow;
        b.quad(
            [
                DVec3::new(x - 0.45, 0.55 - 0.37, z),
                DVec3::new(x + 0.45, 0.55 - 0.37, z),
                DVec3::new(x + 0.45, 0.55 + 0.37, z),
                DVec3::new(x - 0.45, 0.55 + 0.37, z),
            ],
            DVec3::Z,
            rgb(255, 140, 50),
        );
        b.surf = Surf::Plain;
    }
}

/// The flywheel: a heavy rim, six curved spokes and a hub, about +z.
fn flywheel(b: &mut MeshBuilder) {
    let r = rig::FLYWHEEL.2;
    let rim: Vec<(f64, f64)> = vec![
        (r - 0.22, -0.13),
        (r, -0.13),
        (r + 0.02, -0.1),
        (r + 0.02, 0.1),
        (r, 0.13),
        (r - 0.22, 0.13),
        (r - 0.22, -0.13),
    ];
    lathe(b, DVec3::ZERO, DVec3::Z, &rim, 64, |_| metal(oxide_dark()));
    lathe(
        b,
        DVec3::Z * -0.2,
        DVec3::Z,
        &[(0.0, 0.0), (0.24, 0.0), (0.24, 0.4), (0.0, 0.4)],
        20,
        |_| metal(brass()),
    );
    for k in 0..6 {
        let a = TAU * k as f64 / 6.0;
        let pts: Vec<DVec3> = (0..=6)
            .map(|i| {
                let t = i as f64 / 6.0;
                let rr = 0.2 + (r - 0.4) * t;
                let aa = a + 0.25 * (t * PI).sin();
                DVec3::new(aa.cos() * rr, aa.sin() * rr, 0.0)
            })
            .collect();
        tube(b, &pts, (0.07, 0.05), 8, metal(oxide_dark()));
    }
    // A brass balance weight, so the turning shows.
    lathe(
        b,
        DVec3::new(0.0, r - 0.35, -0.12),
        DVec3::Z,
        &[(0.0, 0.0), (0.16, 0.0), (0.16, 0.24), (0.0, 0.24)],
        12,
        |_| metal(brass()),
    );
}

/// The smokestack: a flanged foot, a banded tapering shaft and a flared
/// crown, with the flue from the boiler's end.
fn stack(b: &mut MeshBuilder, near: bool) {
    let (sx, sz) = rig::STACK_AT;
    let base = DVec3::new(sx, 0.0, sz);
    let r = rig::STACK_R;
    let h = rig::STACK_TOP;
    let prof = [
        (r + 0.35, 0.0),
        (r + 0.35, 0.45),
        (r + 0.1, 0.6),
        (r, 0.8),
        (r * 0.85, h - 1.6),
        (r * 1.0, h - 1.0),
        (r * 1.45, h - 0.25),
        (r * 1.5, h),
        (r * 1.3, h),
        (r * 0.8, h - 0.4),
        (0.0, h - 0.4),
    ];
    lathe(
        b,
        base,
        DVec3::Y,
        &prof,
        if near { 32 } else { 8 },
        |i| match i {
            0 | 1 => metal(iron()),
            5..=7 => metal(brass()),
            8 | 9 => plain(rgb(20, 18, 16)),
            _ => metal(iron().lerp(oxide_dark(), 0.4)),
        },
    );
    let (y, z, half) = (rig::BOILER_Y, rig::BOILER_Z, rig::BOILER_HALF);
    tube(
        b,
        &[
            DVec3::new(half + 0.4, y + 0.2, z),
            DVec3::new(sx - r, y + 0.2, z),
        ],
        (0.38, 0.38),
        if near { 20 } else { 6 },
        metal(iron()),
    );
    if !near {
        return;
    }
    let mut yy = 2.5;
    while yy < h - 2.0 {
        let rr = r + (r * 0.85 - r) * (yy - 0.8) / (h - 2.4);
        lathe(
            b,
            base + DVec3::Y * (yy - 0.08),
            DVec3::Y,
            &[(rr, 0.0), (rr + 0.04, 0.03), (rr + 0.04, 0.13), (rr, 0.16)],
            32,
            |_| metal(brass()),
        );
        yy += 2.4;
    }
}

/// The steam pipe from the boiler's dome up the back left leg to the
/// gearbox's motor, with flanged joints.
fn steam_pipe(b: &mut MeshBuilder) {
    let pts: Vec<DVec3> = rig::PIPE
        .iter()
        .map(|&(x, y, z)| DVec3::new(x, y, z))
        .collect();
    let path = spline(&pts, 6, false);
    tube(
        b,
        &path,
        (rig::PIPE_R, rig::PIPE_R),
        16,
        metal(brass().lerp(oxide_dark(), 0.35)),
    );
    // Flanges every few points along it.
    for w in path.windows(2).step_by(5) {
        let d = (w[1] - w[0]).normalize_or(DVec3::Y);
        lathe(
            b,
            w[0] - d * 0.05,
            d,
            &[
                (0.0, 0.0),
                (rig::PIPE_R + 0.09, 0.0),
                (rig::PIPE_R + 0.09, 0.1),
                (0.0, 0.1),
            ],
            16,
            |_| metal(brass()),
        );
    }
}

/// The mole folk's glowing toadstool lamps round the plinth.
fn lamps(b: &mut MeshBuilder, plaza: f64) {
    let (stem, h) = rig::LAMP_STEM;
    for (k, at) in rig::lamps(plaza).into_iter().enumerate() {
        let glow = if k % 2 == 0 {
            rgb(120, 255, 210)
        } else {
            rgb(190, 130, 255)
        };
        crate::undergroundview::mushroom(b, at, 0.0, stem, h, 0.75, rgb(150, 60, 50), glow);
    }
}

/// What the soundscape hears of the rig this frame.
#[derive(Clone, Debug, Default)]
pub struct Heard {
    /// Where the drill's grind comes from (the collar), if near.
    pub at: Option<Vec3>,
    /// How hard it works (0..1): the grind rises with it.
    pub effort: f32,
    /// How loud the geyser roars now (0..1).
    pub roar: f32,
    /// Since the last frame: the rumble began (the whistle blows), the
    /// geyser burst out.
    pub rumbled: bool,
    pub burst: bool,
}

struct Live {
    origin: Vec3,
    yaw: f32,
    drill: EntityId,
    gear: EntityId,
    wheel: EntityId,
    bore: EntityId,
    beacon: EntityId,
    fire: EntityId,
    lamp_bore: LampId,
    lamp_beacon: LampId,
    /// Steady emitters while active: the collar's wisp, the stack's smoke;
    /// and the geyser's column while it blows.
    wisp: Option<EmitterId>,
    smoke: Option<EmitterId>,
    column: Option<EmitterId>,
}

pub struct DrillView {
    rig: Option<Landmark>,
    live: Option<Live>,
    /// The game clock last frame (s).
    last: f64,
    phase: Phase,
    /// The drill's and flywheel's turn (degrees).
    spin: f64,
    wheel: f64,
    /// Seconds to the next vent spit and dust puff while it rumbles.
    spit: f32,
    rng: u64,
    heard: Heard,
    /// `HW_DRILL_AT=s`, for screenshots of a moment in the cycle: the rig
    /// starts at that second and runs at 30 frames a game second whatever
    /// the frame time (a software-rendered frame takes seconds), and so do
    /// the particles.
    pinned: Option<(f64, u32)>,
}

impl DrillView {
    pub fn new(town: &TownWorld) -> Self {
        Self::with_rig(
            town.landmarks
                .iter()
                .find(|l| l.kind == LandmarkKind::MoleDrill)
                .cloned(),
        )
    }

    fn with_rig(rig: Option<Landmark>) -> Self {
        Self {
            rig,
            live: None,
            last: -1.0,
            phase: Phase::Drill,
            spin: 0.0,
            wheel: 0.0,
            spit: 0.0,
            rng: 0x6d6f_6c65_6472_696c,
            heard: Heard::default(),
            pinned: std::env::var("HW_DRILL_AT")
                .ok()
                .and_then(|v| v.parse().ok())
                .map(|t| (t, 0)),
        }
    }

    /// Is the live part made (for the memory checks and tests)?
    pub fn is_live(&self) -> bool {
        self.live.is_some()
    }

    /// What the soundscape should hear this frame (events once).
    pub fn heard(&mut self) -> Heard {
        let h = self.heard.clone();
        self.heard.rumbled = false;
        self.heard.burst = false;
        h
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    fn spawn(world: &mut World, l: &Landmark) -> EngineResult<Live> {
        let origin = Vec3::new(l.pos.x as f32, l.base as f32, l.pos.y as f32);
        let yaw = l.dir.x.atan2(l.dir.y).to_degrees() as f32;
        let make = |world: &mut World, f: &dyn Fn(&mut MeshBuilder), shadow: bool| {
            let mut b = MeshBuilder::new();
            f(&mut b);
            let id = world.spawn_instanced(b.finish());
            world.set_casts_shadow(id, shadow)?;
            EngineResult::Ok(id)
        };
        let drill = make(world, &drill, true)?;
        let gear = make(world, &ring_gear, true)?;
        let wheel = make(world, &flywheel, true)?;
        let bore = make(world, &|b| bore_glow(b, true), false)?;
        let beacon = make(world, &|b| beacon_lens(b, true), false)?;
        let fire = make(world, &fire_glow, false)?;
        let at = |p: DVec3| world_at(origin, yaw, p);
        let lamp_bore = world.add_lamp(Lamp::point(at(DVec3::Y * 1.2), c3(BORE), 20.0, 25.0))?;
        let lamp_beacon = world.add_lamp(Lamp::point(at(beacon_at()), c3(BEACON), 0.0, 40.0))?;
        Ok(Live {
            origin,
            yaw,
            drill,
            gear,
            wheel,
            bore,
            beacon,
            fire,
            lamp_bore,
            lamp_beacon,
            wisp: None,
            smoke: None,
            column: None,
        })
    }

    pub fn clear(&mut self, world: &mut World) {
        if let Some(mut live) = self.live.take() {
            for id in [
                live.drill,
                live.gear,
                live.wheel,
                live.bore,
                live.beacon,
                live.fire,
            ] {
                world.despawn(id);
            }
            world.remove_lamp(live.lamp_bore);
            world.remove_lamp(live.lamp_beacon);
            stop_emitters(world, &mut live);
        }
        self.heard = Heard::default();
    }

    /// One frame at game clock `now` (s).
    pub fn update(&mut self, world: &mut World, now: f64) -> EngineResult<()> {
        let Some(l) = self.rig.clone() else {
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
        }
        let now = match &mut self.pinned {
            Some((t, frames)) => {
                *frames += 1;
                world.particles_mut().advance(1.0 / 30.0)?;
                *t + *frames as f64 / 30.0
            }
            None => now,
        };
        let dt = if self.last < 0.0 {
            0.0
        } else {
            (now - self.last).clamp(0.0, 0.2)
        };
        self.last = now;
        let active = far < ACTIVE;
        let was = self.phase;
        self.step(now, dt);
        let mut live = self.live.take().expect("live");
        let drawn = self.draw(world, &mut live, was, active, dt as f32);
        self.live = Some(live);
        drawn
    }

    /// Advances the cycle and the turning parts.
    fn step(&mut self, now: f64, dt: f64) {
        let was = self.phase;
        self.phase = rig::phase(now);
        // The drill's speed (turns a second): steady, racing as the
        // pressure builds, slow while it is hauled up.
        let rps = match self.phase {
            Phase::Drill => 0.45,
            Phase::Rumble(f) => 0.45 + 0.9 * f,
            Phase::Erupt(f) => 1.35 * (1.0 - f).max(0.15),
            Phase::Settle(f) => 0.2 + 0.25 * f,
        };
        self.spin = (self.spin + rps * 360.0 * dt) % 360.0;
        self.wheel = (self.wheel + rps * 1.6 * 360.0 * dt) % 360.0;
        let rumbling = matches!(self.phase, Phase::Rumble(_));
        if rumbling && !matches!(was, Phase::Rumble(_)) {
            self.heard.rumbled = true;
        }
        if matches!(self.phase, Phase::Erupt(_)) && !matches!(was, Phase::Erupt(_)) {
            self.heard.burst = true;
        }
        self.heard.effort = (rps / 1.35) as f32;
        self.heard.roar = match self.phase {
            Phase::Erupt(f) => (1.0 - ((f - 0.75) / 0.25).max(0.0)) as f32,
            Phase::Settle(f) => ((1.0 - f) * 0.3) as f32,
            _ => 0.0,
        };
    }

    fn draw(
        &mut self,
        world: &mut World,
        live: &mut Live,
        was: Phase,
        active: bool,
        dt: f32,
    ) -> EngineResult<()> {
        let (origin, yaw) = (live.origin, live.yaw);
        let at = |p: DVec3| world_at(origin, yaw, p);
        let place = |p: DVec3| {
            let w = at(p);
            Place::new(w.x, w.y, w.z).with_yaw_deg(yaw)
        };
        let tint = |k: f32| Color::rgb01_unchecked(k, k, k);
        let pressure = match self.phase {
            Phase::Drill => 0.0,
            Phase::Rumble(f) => f,
            Phase::Erupt(_) => 1.0,
            Phase::Settle(f) => 1.0 - f,
        } as f32;
        // The drill shakes as it races.
        let shake = if matches!(self.phase, Phase::Rumble(_)) && active {
            let k = 0.05 * pressure as f64;
            DVec3::new(
                (self.rand() as f64 - 0.5) * k,
                0.0,
                (self.rand() as f64 - 0.5) * k,
            )
        } else {
            DVec3::ZERO
        };
        let tip = rig::bit_tip(self.phase);
        let d = place(DVec3::Y * tip + shake).with_yaw_deg(yaw + self.spin as f32);
        world.set_instances(live.drill, &[d])?;
        let g = place(DVec3::Y * rig::GEAR_Y1).with_yaw_deg(yaw + self.spin as f32);
        world.set_instances(live.gear, &[g])?;
        let (fx, fy, _) = rig::FLYWHEEL;
        let w = place(DVec3::new(fx, fy, rig::BOILER_Z))
            .with_yaw_deg(yaw + 90.0)
            .with_roll_deg(self.wheel as f32);
        world.set_instances(live.wheel, &[w])?;
        // The glows: the bore brightens with the pressure; the beacon
        // blinks slowly, fast while it rumbles and blows; the fire flickers.
        let flick = 0.85 + 0.15 * self.rand();
        world.set_instances(
            live.bore,
            &[place(DVec3::ZERO).with_tint(tint((0.7 + 1.3 * pressure) * flick))],
        )?;
        let t = self.last as f32;
        let blink = if pressure > 0.0 {
            if (t * 4.0).fract() < 0.5 {
                1.6
            } else {
                0.15
            }
        } else if (t * 0.8).fract() < 0.25 {
            1.1
        } else {
            0.25
        };
        world.set_instances(live.beacon, &[place(DVec3::ZERO).with_tint(tint(blink))])?;
        world.set_instances(
            live.fire,
            &[place(DVec3::ZERO).with_tint(tint(0.8 + 0.4 * self.rand()))],
        )?;
        world.set_lamp(
            live.lamp_bore,
            Lamp::point(
                at(DVec3::Y * (1.2 + 3.0 * pressure as f64)),
                c3(BORE),
                (18.0 + 60.0 * pressure) * flick,
                25.0 + 15.0 * pressure,
            ),
        )?;
        world.set_lamp(
            live.lamp_beacon,
            Lamp::point(at(beacon_at()), c3(BEACON), 18.0 * blink, 40.0),
        )?;
        self.heard.at = Some(at(DVec3::Y * 2.0));
        if !active {
            stop_emitters(world, live);
            return Ok(());
        }
        // Steam and smoke.
        let collar = at(DVec3::Y * (rig::COLLAR_H + 0.2));
        if live.wisp.is_none() {
            live.wisp = world.particles_mut().start(wisp(collar)).ok();
        }
        if live.smoke.is_none() {
            let top = at(DVec3::new(
                rig::STACK_AT.0,
                rig::STACK_TOP + 0.2,
                rig::STACK_AT.1,
            ));
            live.smoke = world.particles_mut().start(smoke(top)).ok();
        }
        let blowing = matches!(self.phase, Phase::Erupt(f) if f < 0.85);
        match (blowing, live.column) {
            (true, None) => {
                live.column = world.particles_mut().start(column(collar)).ok();
                // The first blast: a burst of mist round the collar.
                let _ = world.particles_mut().burst(burst(collar));
            }
            (false, Some(id)) => {
                let _ = world.particles_mut().stop(id);
                live.column = None;
            }
            _ => {}
        }
        if matches!(self.phase, Phase::Rumble(_)) {
            self.spit -= dt;
            if self.spit <= 0.0 {
                self.spit = 0.25 + 0.5 * self.rand() * (1.2 - pressure);
                let (_, mouth, up) = vents()[(self.rand() * 4.0) as usize % 4];
                let dir = (at(mouth + up) - at(mouth)).normalize_or_zero();
                let _ = world.particles_mut().burst(spit(at(mouth), dir));
                if self.rand() < 0.4 {
                    // Grit shaken loose on the deck.
                    let a = self.rand() as f64 * TAU;
                    let p = at(DVec3::new(a.sin() * 5.0, 0.1, a.cos() * 5.0));
                    let _ = world.particles_mut().burst(ParticleEmitter::dust(
                        p,
                        0.8,
                        rgb(120, 104, 86),
                    ));
                }
            }
        } else if matches!(was, Phase::Rumble(_)) {
            self.spit = 0.0;
        }
        Ok(())
    }
}

fn stop_emitters(world: &mut World, live: &mut Live) {
    for id in [live.wisp.take(), live.smoke.take(), live.column.take()]
        .into_iter()
        .flatten()
    {
        let _ = world.particles_mut().stop(id);
    }
}

/// The rig's world point at `local` (its own frame).
fn world_at(origin: Vec3, yaw: f32, local: DVec3) -> Vec3 {
    let a = (yaw as f64).to_radians();
    let (s, c) = a.sin_cos();
    // Local +z is the way it faces, +x its right (see `Landmark::to_plan`).
    let x = local.x * c + local.z * s;
    let z = -local.x * s + local.z * c;
    origin + Vec3::new(x as f32, local.y as f32, z as f32)
}

/// A linear colour with alpha (particles take linear colours).
fn lin(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color { r, g, b, a }
}

/// A thin wisp of steam rising from the bore while the drill works.
fn wisp(at: Vec3) -> ParticleEmitter {
    ParticleEmitter::new(at)
        .with_rate(4.0)
        .with_silhouette(ParticleSilhouette::Puff)
        .with_blend(ParticleBlend::Alpha)
        .with_shape(ParticleShape::Sphere)
        .with_spread(Vec3::new(1.6, 0.1, 1.6))
        .with_velocity(Vec3::Y * 1.4)
        .with_velocity_jitter(Vec3::new(0.3, 0.4, 0.3))
        .with_acceleration(Vec3::new(0.15, 0.3, 0.05))
        .with_lifetime(4.0)
        .with_size(0.6)
        .with_size_variation(0.4)
        .with_size_over_life(SizeOverLife::Expand)
        .with_color(lin(0.92, 0.93, 0.95, 0.35))
        .with_secondary_color(lin(0.85, 0.87, 0.9, 0.12))
        .with_turbulence(0.6)
        .with_drag(0.3)
        .with_spin(0.4)
}

/// Coal smoke from the stack.
fn smoke(at: Vec3) -> ParticleEmitter {
    ParticleEmitter::smoke(at, 1.1)
        .with_rate(6.0)
        .with_velocity(Vec3::Y * 2.0)
        .with_color(lin(0.12, 0.11, 0.1, 0.55))
        .with_secondary_color(lin(0.38, 0.37, 0.36, 0.2))
}

/// The geyser: a roaring column of steam out of the bore.
fn column(at: Vec3) -> ParticleEmitter {
    ParticleEmitter::new(at)
        .with_rate(85.0)
        .with_silhouette(ParticleSilhouette::Puff)
        .with_blend(ParticleBlend::Alpha)
        .with_shape(ParticleShape::Sphere)
        .with_spread(Vec3::new(1.4, 0.3, 1.4))
        .with_velocity(Vec3::Y * 21.0)
        .with_velocity_jitter(Vec3::new(1.6, 4.0, 1.6))
        .with_acceleration(Vec3::new(0.4, -3.0, 0.15))
        .with_lifetime(3.6)
        .with_size(1.5)
        .with_size_variation(0.45)
        .with_size_over_life(SizeOverLife::Expand)
        .with_color(lin(0.97, 0.98, 0.99, 0.92))
        .with_secondary_color(lin(0.86, 0.88, 0.92, 0.25))
        .with_turbulence(0.9)
        .with_drag(0.4)
        .with_spin(0.6)
        // Steam glows white even against the sun.
        .with_lighting(0.55)
}

/// The first blast: a ring of steam bursting out round the collar.
fn burst(at: Vec3) -> ParticleEmitter {
    ParticleEmitter::new(at)
        .with_mode(ParticleMode::Burst)
        .with_burst_count(70)
        .with_silhouette(ParticleSilhouette::Puff)
        .with_blend(ParticleBlend::Alpha)
        .with_shape(ParticleShape::Ring)
        .with_spread(Vec3::new(2.5, 0.2, 2.5))
        .with_velocity(Vec3::Y * 6.0)
        .with_velocity_jitter(Vec3::new(5.0, 3.0, 5.0))
        .with_acceleration(Vec3::new(0.0, -1.0, 0.0))
        .with_lifetime(3.0)
        .with_size(1.0)
        .with_size_variation(0.5)
        .with_size_over_life(SizeOverLife::Expand)
        .with_color(lin(0.95, 0.96, 0.97, 0.7))
        .with_secondary_color(lin(0.85, 0.87, 0.9, 0.2))
        .with_turbulence(0.8)
        .with_drag(0.9)
        .with_spin(0.5)
}

/// A hiss of steam from a vent as the pressure builds.
fn spit(at: Vec3, dir: Vec3) -> ParticleEmitter {
    ParticleEmitter::new(at)
        .with_mode(ParticleMode::Burst)
        .with_burst_count(14)
        .with_silhouette(ParticleSilhouette::Puff)
        .with_blend(ParticleBlend::Alpha)
        .with_shape(ParticleShape::Point)
        .with_velocity(dir * 7.0)
        .with_velocity_jitter(Vec3::splat(1.2))
        .with_lifetime(1.4)
        .with_size(0.35)
        .with_size_variation(0.4)
        .with_size_over_life(SizeOverLife::Expand)
        .with_color(lin(0.95, 0.96, 0.97, 0.6))
        .with_secondary_color(lin(0.88, 0.9, 0.92, 0.15))
        .with_turbulence(0.7)
        .with_drag(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rig_at() -> Landmark {
        Landmark {
            kind: LandmarkKind::MoleDrill,
            pos: DVec2::ZERO,
            plaza: 14.0,
            base: 3.0,
            low: 1.0,
            height: rig::HEIGHT,
            dir: DVec2::Y,
            stairs: Vec::new(),
        }
    }

    fn faces(f: impl Fn(&mut MeshBuilder)) -> usize {
        let mut b = MeshBuilder::new();
        f(&mut b);
        b.face_count()
    }

    #[test]
    fn the_rig_is_built_in_detail_near_and_light_far() {
        let l = rig_at();
        let near = faces(|b| model(b, &l, true));
        let far = faces(|b| model(b, &l, false));
        // A close-up landmark: tens of thousands of faces near, a few
        // thousand far.
        assert!((20_000..120_000).contains(&near), "near {near}");
        assert!(far * 6 < near && far > 300, "far {far}");
        let parts = faces(drill) + faces(ring_gear) + faces(flywheel);
        assert!(parts > 5_000, "moving parts {parts}");
    }

    #[test]
    fn it_works_rumbles_and_blows() {
        let mut v = DrillView::with_rig(Some(rig_at()));
        let (mut rumbles, mut bursts, mut roar) = (0, 0, 0.0f32);
        let mut t = 0.0;
        while t < rig::CYCLE * 5.0 {
            v.step(t, 1.0 / 30.0);
            let h = v.heard();
            rumbles += h.rumbled as i32;
            bursts += h.burst as i32;
            roar = roar.max(h.roar);
            t += 1.0 / 30.0;
        }
        assert_eq!((rumbles, bursts), (5, 5));
        assert!(roar > 0.9);
    }

    #[test]
    fn every_steam_profile_is_valid() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        for e in [
            wisp(at),
            smoke(at),
            column(at),
            burst(at),
            spit(at, Vec3::Y),
        ] {
            assert!(e.validate().is_ok(), "{e:?}");
        }
    }
}
