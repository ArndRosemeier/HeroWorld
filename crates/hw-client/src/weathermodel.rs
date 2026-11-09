//! The weather tower's model: a round glass pavilion under an overhanging
//! roof, a tapering white mast with three swept coral fins, an observation
//! saucer, and a crown of six trumpet-mouthed cloud cannons round a
//! weather globe. Its moving and glowing parts (the globe, the cannons'
//! mouths, the anemometer, the barometer's needle) are
//! [`crate::weatherview`]'s; the far model carries still copies.
//!
//! Every curved part is lathed or lofted smooth (see [`crate::carmesh`]).
//! Origin on the plinth's top, the doors towards +z.

use crate::carmesh::{lathe, tube, Paint};
use crate::citymesh::{MeshBuilder, Surf};
use crate::daylight::{channel, Tone};
use crate::landmarkview::strut;
use engine::prelude::*;
use glam::DVec3;
use hw_gen::urban::landmarks::Landmark;
use hw_gen::urban::weather_tower::{
    fin_dir, WxDims, WX_CANNONS, WX_FIN_AT, WX_FIN_T, WX_HALL_R, WX_HALL_WALL, WX_ROOF_R,
    WX_ROOF_TOP,
};
use std::f64::consts::{PI, TAU};

pub fn white() -> Color {
    rgb(242, 239, 230)
}
pub fn sky_blue() -> Color {
    rgb(104, 170, 222)
}
pub fn coral() -> Color {
    rgb(228, 94, 76)
}
pub fn chrome() -> Color {
    rgb(206, 210, 216)
}
pub fn brass() -> Color {
    rgb(206, 164, 84)
}
/// The globe's and the cannons' light, before the view tints it.
pub fn glow_white() -> Color {
    rgb(250, 248, 236)
}

fn plain(c: Color) -> Paint {
    Some((Surf::Plain, c))
}
fn metal(c: Color) -> Paint {
    Some((Surf::Metal, c))
}

/// The barometer's dial on the mast, facing +z: its centre and radius.
pub fn dial(d: &WxDims) -> (DVec3, f64) {
    let y = WX_ROOF_TOP + 4.2;
    (DVec3::new(0.0, y, d.shaft_at(y) + 0.55), 1.7)
}

/// Where cannon `k` sits on the crown, and the way its mouth points.
pub fn cannon(d: &WxDims, k: usize) -> (DVec3, DVec3) {
    let a = TAU * (k as f64 + 0.5) / WX_CANNONS as f64;
    let out = DVec3::new(a.sin(), 0.0, a.cos());
    let tilt = 32f64.to_radians();
    let axis = (out * tilt.sin() + DVec3::Y * tilt.cos()).normalize();
    (out * d.crown_r + DVec3::Y * (d.crown_y + 0.3), axis)
}

/// A cannon's length, from its throat to its mouth.
pub const CANNON_LEN: f64 = 2.8;
pub const MOUTH_R: f64 = 0.95;

/// The whole tower. `near`: every part, smooth; far: the silhouette, with
/// the glowing parts still on it.
pub fn tower(b: &mut MeshBuilder, l: &Landmark, near: bool) {
    let d = WxDims::of(l.height);
    let seg = if near { 64 } else { 16 };
    pavilion(b, &d, near, seg);
    mast(b, &d, near, seg);
    for k in 0..WX_FIN_AT.len() {
        fin(b, &d, k, near);
    }
    saucer(b, &d, near, seg);
    crown(b, &d, near);
    if near {
        barometer(b, &d);
    } else {
        globe(b, &d, false);
        mouths(b, &d, false);
    }
}

/// The pavilion: a stone footing, a glass drum between chrome mullions,
/// a white fascia, and a thin roof overhanging all round, sky blue
/// underneath; a coral canopy over the doors.
fn pavilion(b: &mut MeshBuilder, d: &WxDims, near: bool, seg: usize) {
    let r = WX_HALL_R;
    // Footing, glass, fascia, and the roof's edge rolled round.
    let glass_top = WX_HALL_WALL - 0.55;
    let mut prof = vec![
        (0.0, 0.0),
        (r + 0.25, 0.0),
        (r + 0.25, 0.45),
        (r - 0.15, 0.5),
        (r - 0.15, glass_top),
        (r + 0.1, glass_top + 0.02),
        (r + 0.1, WX_HALL_WALL),
        (WX_ROOF_R - 0.25, WX_ROOF_TOP - 0.42),
    ];
    // The roof's rounded edge.
    for i in 0..=6 {
        let a = -PI * 0.5 + PI * i as f64 / 6.0;
        prof.push((
            WX_ROOF_R - 0.25 + 0.25 * a.cos(),
            WX_ROOF_TOP - 0.17 + 0.25 * a.sin(),
        ));
    }
    prof.push((d.shaft_r.0, WX_ROOF_TOP + 0.08));
    prof.push((0.0, WX_ROOF_TOP + 0.08));
    let pane = Surf::Lit(channel(Tone::Cool, 0));
    lathe(b, DVec3::ZERO, DVec3::Y, &prof, seg, |i| match i {
        0..=2 => plain(rgb(198, 192, 180)),
        3 => Some((pane, rgb(120, 168, 196))),
        4..=5 => plain(white()),
        // Under the overhang, sky blue.
        6 => plain(sky_blue()),
        7..=12 => metal(chrome()),
        _ => plain(white()),
    });
    if !near {
        return;
    }
    // Chrome mullions round the glass, a gap at the doors.
    let n = 36;
    for k in 0..n {
        let a = TAU * (k as f64 + 0.5) / n as f64;
        let out = DVec3::new(a.sin(), 0.0, a.cos());
        if out.z > 0.985 {
            continue;
        }
        b.surf = Surf::Metal;
        b.oriented_box(
            out * (r - 0.05) + DVec3::Y * (0.5 + glass_top) * 0.5,
            DVec3::new(out.z, 0.0, -out.x),
            DVec3::new(0.07, (glass_top - 0.5) * 0.5, 0.14),
            chrome(),
        );
        b.surf = Surf::Plain;
    }
    // A ring of sky-blue tiles round the fascia.
    lathe(
        b,
        DVec3::Y * (glass_top + 0.18),
        DVec3::Y,
        &[
            (r + 0.1, 0.0),
            (r + 0.14, 0.0),
            (r + 0.14, 0.12),
            (r + 0.1, 0.12),
        ],
        seg,
        |_| plain(sky_blue()),
    );
    // The doors: a chrome frame, two glass leaves, and over them a coral
    // canopy on thin chrome posts.
    let z = r - 0.1;
    b.surf = Surf::Metal;
    for x in [-1.45, 0.0, 1.45] {
        b.oriented_box(
            DVec3::new(x, 1.35, z + 0.05),
            DVec3::X,
            DVec3::new(0.08, 1.35, 0.1),
            chrome(),
        );
    }
    b.oriented_box(
        DVec3::new(0.0, 2.75, z + 0.05),
        DVec3::X,
        DVec3::new(1.55, 0.08, 0.1),
        chrome(),
    );
    b.surf = Surf::Glass;
    for x in [-0.72, 0.72] {
        b.oriented_box(
            DVec3::new(x, 1.35, z),
            DVec3::X,
            DVec3::new(0.64, 1.3, 0.03),
            rgb(110, 150, 170),
        );
    }
    b.surf = Surf::Plain;
    // The canopy: a thin wing, wider at its front, rounded.
    let pts: Vec<DVec3> = (0..=16)
        .map(|i| {
            let t = i as f64 / 16.0;
            let a = PI * (t - 0.5);
            DVec3::new(2.6 * a.sin(), 0.0, r + 0.2 + 2.4 * a.cos())
        })
        .collect();
    let y = 3.25;
    let mut rows = Vec::new();
    for (dy, k) in [(0.0, 1.0), (0.16, 1.0), (0.2, 0.96)] {
        rows.push(
            pts.iter()
                .map(|p| {
                    let c = DVec3::new(0.0, 0.0, r + 0.2);
                    c + (*p - c) * k + DVec3::Y * (y + dy)
                })
                .collect::<Vec<_>>(),
        );
    }
    b.grid(&rows, false, true, |_, _| plain(coral()));
    let base_row: Vec<DVec3> = pts.iter().map(|p| *p + DVec3::Y * y).collect();
    b.polygon(&base_row, -DVec3::Y, sky_blue());
    let top: Vec<DVec3> = rows[2].clone();
    b.polygon(&top, DVec3::Y, coral());
    for x in [-1.9, 1.9] {
        lathe(
            b,
            DVec3::new(x, 0.0, r + 2.2),
            DVec3::Y,
            &[(0.0, 0.0), (0.07, 0.0), (0.07, y), (0.0, y)],
            12,
            |_| metal(chrome()),
        );
    }
}

/// The mast from the roof to the globe: white, tapering, ringed with sky
/// blue bands and chrome collars; and the mast's needle up through the
/// crown.
fn mast(b: &mut MeshBuilder, d: &WxDims, near: bool, seg: usize) {
    let steps = if near { 24 } else { 6 };
    let mut prof = vec![(0.0, WX_ROOF_TOP)];
    let mut ys = Vec::new();
    for i in 0..=steps {
        let y = WX_ROOF_TOP + (d.crown_y - WX_ROOF_TOP) * i as f64 / steps as f64;
        ys.push(y);
        prof.push((d.shaft_at(y), y));
    }
    prof.push((0.0, d.crown_y));
    let band = |y: f64| {
        let t = (y - WX_ROOF_TOP) / (d.crown_y - WX_ROOF_TOP);
        (t * 9.0).fract() < 0.18 && t > 0.12
    };
    lathe(b, DVec3::ZERO, DVec3::Y, &prof, seg, |i| {
        if i == 0 || i > steps {
            plain(white())
        } else if near && band(ys[i - 1]) {
            plain(sky_blue())
        } else {
            plain(white())
        }
    });
    // The needle: from the crown up through the globe to the anemometer.
    lathe(
        b,
        DVec3::Y * d.crown_y,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.42, 0.0),
            (0.3, 0.8),
            (0.18, 1.4),
            (0.16, d.vane_y - d.crown_y),
            (0.0, d.vane_y - d.crown_y + 0.2),
        ],
        if near { 24 } else { 8 },
        |_| metal(chrome()),
    );
    if !near {
        return;
    }
    // Chrome collars where the mast steps.
    for y in [
        WX_ROOF_TOP + 0.4,
        d.deck_y + d.deck_span.1 + 0.4,
        d.crown_y - 1.2,
    ] {
        let r = d.shaft_at(y) + 0.06;
        lathe(
            b,
            DVec3::Y * y,
            DVec3::Y,
            &round_band(r, 0.12, 0.3),
            seg,
            |_| metal(chrome()),
        );
    }
    // Portholes up the mast's front and back, lit like the bureau's rooms.
    let pane = Surf::Lit(channel(Tone::Cool, 1));
    for k in 0..7 {
        let y = WX_ROOF_TOP + 7.0 + k as f64 * (d.deck_y - WX_ROOF_TOP - 9.5) / 6.0;
        for side in [1.0, -1.0] {
            let r = d.shaft_at(y);
            let c = DVec3::new(0.0, y, side * (r - 0.02));
            porthole(b, c, DVec3::Z * side, 0.32, pane);
        }
    }
}

/// A round window: a chrome ring and a pane, facing `n` at `c`.
fn porthole(b: &mut MeshBuilder, c: DVec3, n: DVec3, r: f64, pane: Surf) {
    lathe(
        b,
        c,
        n,
        &[
            (0.0, 0.04),
            (r, 0.04),
            (r, 0.0),
            (r + 0.1, 0.0),
            (r + 0.1, 0.1),
            (r - 0.02, 0.1),
            (r - 0.02, 0.05),
            (0.0, 0.05),
        ],
        20,
        |i| match i {
            0 | 6 => Some((pane, rgb(110, 160, 190))),
            _ => metal(chrome()),
        },
    );
}

/// A rounded ring's profile (for `lathe`): radius `r`, standing out
/// `out`, `h` tall.
fn round_band(r: f64, out: f64, h: f64) -> Vec<(f64, f64)> {
    (0..=8)
        .map(|i| {
            let t = PI * i as f64 / 8.0;
            (r + out * t.sin(), h * i as f64 / 8.0)
        })
        .collect()
}

/// A fin: a thin coral blade from the plaza sweeping up into the mast,
/// lens-shaped in section, its outer edge trimmed in chrome.
fn fin(b: &mut MeshBuilder, d: &WxDims, k: usize, near: bool) {
    let dir = fin_dir(k);
    let out = DVec3::new(dir.x, 0.0, dir.y);
    let side = DVec3::new(dir.y, 0.0, -dir.x);
    let rows_n = if near { 28 } else { 7 };
    let across = if near { 6 } else { 2 };
    let mut rows = Vec::new();
    for i in 0..=rows_n {
        let y = d.fin_top * i as f64 / rows_n as f64;
        let outer = d.fin_edge(y);
        let inner = if y < WX_HALL_WALL {
            WX_HALL_R - 0.3
        } else {
            d.shaft_at(y) - 0.15
        };
        let outer = outer.max(inner + 0.05);
        // Thickest at the root, thin at the edge: a lens from inner to
        // outer and back on the other side.
        let mut ring = Vec::new();
        for s in [1.0, -1.0] {
            for j in 0..=across {
                let t = if s > 0.0 {
                    j as f64 / across as f64
                } else {
                    1.0 - j as f64 / across as f64
                };
                let r = inner + (outer - inner) * t;
                let w = WX_FIN_T * 0.5 * (1.0 - t * t).sqrt().max(0.12);
                ring.push(out * r + side * (w * s) + DVec3::Y * y);
            }
        }
        rows.push(ring);
    }
    let cols = 2 * (across + 1);
    b.grid(&rows, true, true, |_, j| {
        // The faces round the outer edge, chrome; the sides coral.
        if j == across || j + 1 == cols {
            metal(chrome())
        } else {
            plain(coral())
        }
    });
    if !near {
        return;
    }
    // A chrome foot where the fin meets the plaza.
    let foot = out * (d.fin_edge(0.0) + WX_HALL_R) * 0.5;
    b.surf = Surf::Metal;
    b.oriented_box(
        foot + DVec3::Y * 0.12,
        side,
        DVec3::new(
            WX_FIN_T * 0.9,
            0.12,
            (d.fin_edge(0.0) - WX_HALL_R) * 0.5 + 0.3,
        ),
        chrome(),
    );
    b.surf = Surf::Plain;
}

/// The observation saucer: a white underside sweeping out to a chrome rim,
/// a band of lit windows leaning in, and a sky-blue cap.
fn saucer(b: &mut MeshBuilder, d: &WxDims, near: bool, seg: usize) {
    let (lo, hi) = d.deck_span;
    let y = d.deck_y;
    let r = d.deck_r;
    let mut prof = vec![(0.0, y - lo), (d.shaft_r.1 + 0.1, y - lo)];
    // The underside: a shallow curve out to the rim.
    let under = if near { 10 } else { 3 };
    for i in 1..=under {
        let t = i as f64 / under as f64;
        prof.push((
            d.shaft_r.1 + 0.1 + (r - 0.3 - d.shaft_r.1 - 0.1) * t,
            y - lo + (lo - 0.15) * t.powf(2.2),
        ));
    }
    let a = prof.len() - 1;
    // The rim, rolled.
    for i in 1..=6 {
        let t = -PI * 0.5 + PI * i as f64 / 6.0;
        prof.push((r - 0.3 + 0.3 * t.cos(), y + 0.15 * t.sin()));
    }
    let rim = prof.len() - 1;
    // Windows leaning in.
    prof.push((r - 0.45, y + 0.25));
    prof.push((r - 0.85, y + 1.05));
    let win = prof.len() - 1;
    // The cap.
    let capn = if near { 10 } else { 3 };
    for i in 1..=capn {
        let t = i as f64 / capn as f64;
        let ang = t * PI * 0.5;
        prof.push((
            (r - 0.85) * ang.cos() + d.shaft_r.1 * 0.6 * (1.0 - ang.cos()),
            y + 1.05 + (hi - 1.05) * ang.sin(),
        ));
    }
    let pane = Surf::Lit(channel(Tone::Warm, 1));
    lathe(b, DVec3::ZERO, DVec3::Y, &prof, seg * 2, |i| {
        if i < a {
            plain(white())
        } else if i <= rim {
            metal(chrome())
        } else if i < win {
            Some((pane, rgb(120, 170, 200)))
        } else {
            plain(sky_blue())
        }
    });
    if !near {
        return;
    }
    // Window mullions, and a rail round the cap.
    let n = 28;
    for k in 0..n {
        let t = TAU * k as f64 / n as f64;
        let o = DVec3::new(t.sin(), 0.0, t.cos());
        tube(
            b,
            &[
                o * (r - 0.43) + DVec3::Y * (y + 0.22),
                o * (r - 0.87) + DVec3::Y * (y + 1.08),
            ],
            (0.05, 0.05),
            6,
            metal(chrome()),
        );
    }
    let rail: Vec<DVec3> = (0..=64)
        .map(|i| {
            let t = TAU * i as f64 / 64.0;
            DVec3::new(t.sin(), 0.0, t.cos()) * (r - 1.2) + DVec3::Y * (y + 1.75)
        })
        .collect();
    tube(b, &rail, (0.045, 0.045), 6, metal(chrome()));
    for k in 0..16 {
        let t = TAU * k as f64 / 16.0;
        let o = DVec3::new(t.sin(), 0.0, t.cos()) * (r - 1.2);
        strut(
            b,
            o + DVec3::Y * (y + 1.2),
            o + DVec3::Y * (y + 1.75),
            0.06,
            chrome(),
        );
    }
}

/// The crown: a chrome ring on the mast, six coral cannons on curved arms
/// with brass throats, each pointing up and out.
fn crown(b: &mut MeshBuilder, d: &WxDims, near: bool) {
    let seg = if near { 48 } else { 12 };
    let y = d.crown_y;
    lathe(
        b,
        DVec3::ZERO,
        DVec3::Y,
        &[
            (0.0, y - 0.5),
            (d.shaft_r.2 + 0.2, y - 0.5),
            (d.crown_r + 0.6, y - 0.15),
            (d.crown_r + 0.7, y),
            (d.crown_r + 0.6, y + 0.15),
            (d.shaft_r.2, y + 0.3),
            (0.0, y + 0.3),
        ],
        seg,
        |i| {
            if i == 2 || i == 3 {
                metal(chrome())
            } else {
                plain(white())
            }
        },
    );
    for k in 0..WX_CANNONS {
        let (at, axis) = cannon(d, k);
        let pts = if near { 20 } else { 6 };
        // The bell: a throat widening into a flared mouth, with a rolled
        // lip; open inside (the mouth's glow is the view's).
        let mut prof = vec![(0.0, -0.3), (0.36, -0.3), (0.36, 0.0)];
        for i in 1..=pts {
            let t = i as f64 / pts as f64;
            prof.push((0.36 + (MOUTH_R - 0.36) * t.powf(2.6), CANNON_LEN * t));
        }
        prof.push((MOUTH_R + 0.08, CANNON_LEN + 0.04));
        prof.push((MOUTH_R + 0.05, CANNON_LEN + 0.12));
        prof.push((MOUTH_R - 0.06, CANNON_LEN + 0.08));
        prof.push((MOUTH_R * 0.7, CANNON_LEN - 0.3));
        prof.push((0.0, CANNON_LEN - 0.8));
        let n = prof.len();
        lathe(b, at, axis, &prof, if near { 32 } else { 10 }, |i| {
            if i < 3 {
                metal(brass())
            } else if i >= n - 5 && i < n - 2 {
                metal(chrome())
            } else if i >= n - 2 {
                plain(rgb(60, 66, 74))
            } else {
                plain(coral())
            }
        });
        if near {
            // A brass collar at the throat, and a pipe in from the mast.
            lathe(b, at, axis, &round_band(0.38, 0.08, 0.35), 20, |_| {
                metal(brass())
            });
            let o = DVec3::new(at.x, 0.0, at.z).normalize();
            let path: Vec<DVec3> = (0..=8)
                .map(|i| {
                    let t = i as f64 / 8.0;
                    let r = d.shaft_r.2 + (d.crown_r - d.shaft_r.2) * t;
                    o * r + DVec3::Y * (y - 1.4 + 1.5 * (t * PI * 0.5).sin())
                })
                .collect();
            tube(b, &path, (0.14, 0.14), 10, metal(brass()));
        }
    }
    // The globe's cradle: two chrome hoops round it.
    if near {
        for k in 0..2 {
            let a = PI * 0.5 * k as f64 + PI * 0.25;
            let (s, c) = a.sin_cos();
            let r = d.globe_r + 0.22;
            let hoop: Vec<DVec3> = (0..=40)
                .map(|i| {
                    let t = PI * (0.12 + 0.76 * i as f64 / 40.0);
                    DVec3::new(s * r * t.sin(), d.globe_y - r * t.cos(), c * r * t.sin())
                })
                .collect();
            tube(b, &hoop, (0.06, 0.06), 8, metal(chrome()));
            let hoop2: Vec<DVec3> = hoop.iter().map(|p| DVec3::new(-p.x, p.y, -p.z)).collect();
            tube(b, &hoop2, (0.06, 0.06), 8, metal(chrome()));
        }
        lathe(
            b,
            DVec3::Y * (d.globe_y - d.globe_r - 0.35),
            DVec3::Y,
            &round_band(0.5, 0.25, 0.35),
            24,
            |_| metal(chrome()),
        );
    }
}

/// The barometer on the mast's front: a chrome-rimmed dial, gold for fair,
/// grey for change, blue for rain (its needle is the view's).
fn barometer(b: &mut MeshBuilder, d: &WxDims) {
    let (c, r) = dial(d);
    // A boss on the mast it stands on, the face, and a chrome rim round it.
    let back = d.shaft_at(c.y) - 0.3;
    lathe(
        b,
        DVec3::new(0.0, c.y, back),
        DVec3::Z,
        &[
            (0.0, 0.0),
            (r * 0.7, 0.0),
            (r + 0.02, c.z - back - 0.08),
            (r + 0.02, c.z - back),
            (0.0, c.z - back),
        ],
        48,
        |i| {
            if i < 2 {
                plain(white())
            } else {
                plain(rgb(244, 236, 214))
            }
        },
    );
    let ring: Vec<DVec3> = (0..=64)
        .map(|i| {
            let t = TAU * i as f64 / 64.0;
            DVec3::new(t.cos() * (r + 0.04), c.y + t.sin() * (r + 0.04), c.z + 0.02)
        })
        .collect();
    tube(b, &ring, (0.11, 0.11), 10, metal(chrome()));
    // The face's sectors: a fan from the top left round to the top right.
    let sectors = [
        (150.0, 100.0, rgb(240, 196, 70)),
        (100.0, 80.0, rgb(196, 198, 204)),
        (80.0, 30.0, rgb(84, 140, 214)),
    ];
    let z = c.z + 0.012;
    for (a0, a1, col) in sectors {
        let n = 8;
        for i in 0..n {
            let t0 = (a0 + (a1 - a0) * i as f64 / n as f64).to_radians();
            let t1 = (a0 + (a1 - a0) * (i + 1) as f64 / n as f64).to_radians();
            let p = |t: f64, k: f64| DVec3::new(t.cos() * r * k, c.y + t.sin() * r * k, z);
            b.quad(
                [p(t0, 0.55), p(t1, 0.55), p(t1, 0.9), p(t0, 0.9)],
                DVec3::Z,
                col,
            );
        }
    }
    // Ticks round it.
    for k in 0..=12 {
        let t = (150.0 - 120.0 * k as f64 / 12.0f64).to_radians();
        let (s, co) = t.sin_cos();
        let p = |k: f64| DVec3::new(co * r * k, c.y + s * r * k, z + 0.01);
        strut(b, p(0.9), p(0.98), 0.04, rgb(40, 44, 52));
    }
}

/// The weather globe at the top: a sphere of light (the view tints it).
pub fn globe(b: &mut MeshBuilder, d: &WxDims, live: bool) {
    let r = d.globe_r + if live { 0.02 } else { 0.0 };
    let prof: Vec<(f64, f64)> = (0..=16)
        .map(|i| {
            let a = PI * i as f64 / 16.0;
            (r * a.sin(), r * (1.0 - a.cos()))
        })
        .collect();
    lathe(b, DVec3::Y * (d.globe_y - r), DVec3::Y, &prof, 40, |_| {
        Some((Surf::Glow, glow_white()))
    });
}

/// The lit insides of the six cannons' mouths.
pub fn mouths(b: &mut MeshBuilder, d: &WxDims, live: bool) {
    let k_r = if live { 1.0 } else { 0.98 };
    for k in 0..WX_CANNONS {
        let (at, axis) = cannon(d, k);
        lathe(
            b,
            at,
            axis,
            &[
                (0.0, CANNON_LEN - 0.85),
                (MOUTH_R * 0.72 * k_r, CANNON_LEN - 0.3),
                (MOUTH_R * 0.92 * k_r, CANNON_LEN),
            ],
            24,
            |_| Some((Surf::Glow, glow_white())),
        );
    }
}

/// The anemometer over the globe: a hub, four arms and their cups, about
/// its own axis (the view spins it).
pub fn anemometer(b: &mut MeshBuilder, d: &WxDims) {
    lathe(
        b,
        DVec3::ZERO,
        DVec3::Y,
        &[
            (0.0, -0.25),
            (0.22, -0.25),
            (0.26, 0.0),
            (0.18, 0.25),
            (0.0, 0.32),
        ],
        20,
        |_| metal(chrome()),
    );
    for k in 0..4 {
        let a = TAU * k as f64 / 4.0;
        let o = DVec3::new(a.sin(), 0.0, a.cos());
        let side = DVec3::new(o.z, 0.0, -o.x);
        tube(
            b,
            &[o * 0.2, o * d.vane_arm],
            (0.045, 0.045),
            8,
            metal(chrome()),
        );
        // A hemispherical cup facing along the spin.
        let prof: Vec<(f64, f64)> = (0..=8)
            .map(|i| {
                let t = PI * 0.5 * i as f64 / 8.0;
                (0.34 * t.sin(), 0.34 * (1.0 - t.cos()))
            })
            .collect();
        lathe(b, o * d.vane_arm - side * 0.1, side, &prof, 20, |_| {
            Some((Surf::Plain, coral()))
        });
    }
}

/// The barometer's needle about the dial's centre, pointing up (+y), in
/// the dial's plane.
pub fn needle(b: &mut MeshBuilder, r: f64) {
    let tip = DVec3::Y * (r * 0.86);
    let tail = DVec3::Y * (-r * 0.22);
    let w = DVec3::X * 0.07;
    let z = DVec3::Z * 0.06;
    b.quad(
        [tail - w + z, tail + w + z, tip + z, tip + z],
        DVec3::Z,
        rgb(30, 32, 38),
    );
    b.tri(
        [tail - w + z, tail + w + z, tip + z],
        DVec3::Z,
        rgb(30, 32, 38),
    );
    b.surf = Surf::Metal;
    lathe(
        b,
        DVec3::ZERO,
        DVec3::Z,
        &[(0.0, 0.0), (0.14, 0.0), (0.14, 0.1), (0.0, 0.14)],
        16,
        |_| metal(brass()),
    );
    b.surf = Surf::Plain;
}
