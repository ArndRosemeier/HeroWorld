//! The new factions' things: the sky pirates' airship, the Iron-Masked
//! Empire's drop capsule, the hole mole folk dig up through a street, and
//! the weapons their fighters carry (a delver's pick, a pirate's cutlass,
//! a praetorian's shock lance).
//!
//! Everything is built the way the cars are: lofted and lathed surfaces
//! with smooth normals ([`MeshBuilder::grid`] through `carmesh`'s helpers),
//! hard edges only where the real thing has them (broken asphalt, fins,
//! planks). Big things come in three levels.
//!
//! Units are metres, +y up; each model says where its origin is.

use crate::carmesh::{inside, lathe, spline, surface, tube, Lod, Paint};
use crate::citymesh::{MeshBuilder, Surf};
use engine::prelude::*;
use glam::{DVec2, DVec3};
use std::f64::consts::{PI, TAU};

fn plain(c: Color) -> Paint {
    Some((Surf::Plain, c))
}

fn metal(c: Color) -> Paint {
    Some((Surf::Metal, c))
}

fn glow(c: Color) -> Paint {
    Some((Surf::Glow, c))
}

/// `c` a little lighter (k > 1) or darker (k < 1).
fn shade(c: Color, k: f64) -> Color {
    let k = k as f32;
    Color {
        r: (c.r * k).clamp(0.0, 1.0),
        g: (c.g * k).clamp(0.0, 1.0),
        b: (c.b * k).clamp(0.0, 1.0),
        a: c.a,
    }
}

/// A small deterministic jitter in -1..1 from two numbers.
fn jitter(a: u32, b: u32) -> f64 {
    let mut h = a.wrapping_mul(0x9E37_79B9) ^ b.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h as f64 / u32::MAX as f64) * 2.0 - 1.0
}

// ---------------------------------------------------------------- airship

pub use hw_sim::roosts::{envelope_r, AIRSHIP_LENGTH, AIRSHIP_RADIUS, GONDOLA_DECK};

/// The sky pirates' airship *Cloudcutter*: a patched canvas envelope with
/// brass ribs and a red tail, a wooden galleon of a gondola slung beneath
/// on cables, two brass engines with propellers, a black flag at the
/// stern. Origin at the envelope's middle, nose to +z.
pub fn airship(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let seg = lod.pick(56, 24, 12);
    let rows = lod.pick(60, 24, 12);
    let canvas = rgb(232, 220, 190);
    let patch = rgb(206, 188, 150);
    let red = rgb(160, 30, 28);
    let brass = rgb(196, 160, 82);
    let dark = rgb(40, 30, 24);
    let half = AIRSHIP_LENGTH * 0.5;
    // The envelope, in gores: canvas with darker patches here and there,
    // a red band round the tail.
    let ring = |t: f64| -> Vec<DVec3> {
        let r = envelope_r(t);
        let z = half - t * AIRSHIP_LENGTH;
        (0..seg)
            .map(|j| {
                let a = TAU * j as f64 / seg as f64;
                DVec3::new(r * a.cos(), r * a.sin(), z)
            })
            .collect()
    };
    let env: Vec<Vec<DVec3>> = (0..=rows)
        .map(|i| {
            // Rows bunch at the nose and tail where the curve is tight.
            let u = i as f64 / rows as f64;
            ring(0.5 - 0.5 * (PI * u).cos())
        })
        .collect();
    surface(&mut b, &env, true, |i, j| {
        let t = i as f64 / rows as f64;
        if i == 0 {
            return metal(brass);
        }
        if (0.78..0.84).contains(&t) {
            return plain(red);
        }
        if lod != Lod::Far && jitter(i as u32 / 3, j as u32 / 2) > 0.62 {
            return plain(patch);
        }
        plain(canvas)
    });
    // Brass ribs round the envelope.
    if lod != Lod::Far {
        for k in 1..8 {
            let t = k as f64 / 8.0;
            let r = envelope_r(t) + 0.06;
            let z = half - t * AIRSHIP_LENGTH;
            let n = seg.max(12);
            let path: Vec<DVec3> = (0..=n)
                .map(|j| {
                    let a = TAU * j as f64 / n as f64;
                    DVec3::new(r * a.cos(), r * a.sin(), z)
                })
                .collect();
            tube(&mut b, &path, (0.09, 0.09), 6, metal(brass));
        }
    }
    // Four tail fins, red with a canvas stripe.
    for k in 0..4 {
        let a = TAU * k as f64 / 4.0 + PI * 0.25;
        let out = DVec3::new(a.cos(), a.sin(), 0.0);
        let root_t = 0.8;
        let root_z = half - root_t * AIRSHIP_LENGTH;
        let chord = 9.0;
        let span = 5.5;
        let section = |z0: f64, len: f64, r: f64, thick: f64| -> Vec<DVec3> {
            // A thin aerofoil: round leading edge, sharp trailing edge.
            let n = lod.pick(10, 6, 4);
            let side = DVec3::new(-a.sin(), a.cos(), 0.0);
            let mut pts = Vec::new();
            for s in 0..n {
                let u = s as f64 / (n - 1) as f64;
                let w = thick * (1.0 - u) * (u.sqrt() * 2.0).min(1.0);
                pts.push(out * r + side * w + DVec3::Z * (z0 - u * len));
            }
            for s in (0..n).rev() {
                let u = s as f64 / (n - 1) as f64;
                let w = thick * (1.0 - u) * (u.sqrt() * 2.0).min(1.0);
                pts.push(out * r - side * w + DVec3::Z * (z0 - u * len));
            }
            pts
        };
        let r0 = envelope_r(root_t) - 0.3;
        let root = section(root_z, chord, r0, 0.35);
        let tip = section(root_z - 2.8, chord - 3.4, r0 + span, 0.12);
        let stripe = lod.pick(10, 6, 4);
        surface(&mut b, &[root, tip], true, |_, j| {
            if lod != Lod::Far && (j + 2) % stripe < 2 {
                plain(canvas)
            } else {
                plain(red)
            }
        });
    }
    gondola(&mut b, lod, dark, brass);
    b.finish()
}

/// Length, half beam and depth under the deck of the gondola's hull (the
/// solids in `hw_sim::airships` are this size).
const HULL_LEN: f64 = 18.0;
const HULL_BEAM: f64 = 2.0;
const HULL_DEPTH: f64 = 2.4;

/// A point on the gondola's hull: `u` from the bow (0) to the transom (1),
/// `a` round the section from the port gunwale (0) down past the keel (pi/2)
/// to starboard (pi). A fine bow, full midships, a broad stern under a
/// raised quarterdeck, the sides tumbling home a little at the top and the
/// keel sweeping up into the stem.
fn hull_at(u: f64, a: f64) -> DVec3 {
    let deck_y = -GONDOLA_DECK;
    let z = HULL_LEN * 0.5 - u * HULL_LEN;
    let smooth = |e0: f64, e1: f64, x: f64| {
        let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    // Width: from the stem out to full beam, held, then in a little to
    // the transom.
    let bow = (u / 0.38).min(1.0);
    let w = HULL_BEAM
        * (1.0 - (1.0 - bow).powi(2)).max(0.0).sqrt().max(0.03)
        * (1.0 - 0.2 * smooth(0.7, 1.0, u));
    // Depth: the forefoot rises into the stem, the run aft lifts a little.
    let d = HULL_DEPTH
        * (1.0 - 0.6 * (1.0 - (u / 0.32).min(1.0)).powi(2))
        * (1.0 - 0.28 * smooth(0.8, 1.0, u));
    // The gunwale's sheer: up at the bow, up more at the quarterdeck.
    let top = deck_y + 0.4 + 0.45 * (1.0 - u / 0.4).max(0.0).powi(2) + 0.55 * smooth(0.72, 0.9, u);
    let s = a.sin();
    let tumble = 1.0 - 0.07 * (1.0 - s);
    let x = -w * a.cos() * tumble;
    let y = top - (top - (deck_y - d)) * s.powf(0.85);
    DVec3::new(x, y, z)
}

/// The outward normal of the hull at (`u`, `a`).
fn hull_normal(u: f64, a: f64) -> DVec3 {
    let (du, da) = (1e-3, 1e-3);
    let tu = hull_at(u + du, a) - hull_at(u - du, a);
    let ta = hull_at(u, a + da) - hull_at(u, a - da);
    let n = tu.cross(ta).normalize_or(DVec3::X);
    let p = hull_at(u, a);
    // Out is away from the hull's middle line, a little downwards.
    let away = DVec3::new(p.x, (p.y + GONDOLA_DECK + 1.0).min(0.0), 0.0);
    if n.dot(away) < 0.0 {
        -n
    } else {
        n
    }
}

/// The gondola: a planked galleon hull slung under the envelope, red
/// topsides over a dark wale, copper-sheathed below, brass portholes, a
/// keel and stem post, a windowed stern with lanterns; a planked deck
/// behind bulwarks and a brass rail, a glazed wheelhouse forward, engines
/// on outriggers, cables up to the envelope and a black flag astern.
fn gondola(b: &mut MeshBuilder, lod: Lod, dark: Color, brass: Color) {
    let wood = rgb(120, 74, 40);
    let wood2 = rgb(98, 58, 30);
    let deck_wood = rgb(158, 118, 76);
    let red = rgb(138, 32, 26);
    let copper = rgb(176, 104, 62);
    let deck_y = -GONDOLA_DECK;
    let len = HULL_LEN;
    let rows = lod.pick(56, 22, 10);
    let strakes = lod.pick(14, 8, 5);
    // Each strake is a narrow seam and its plank (near), or the plank alone.
    let seams = lod == Lod::Near;
    let mut cols: Vec<(f64, usize, bool)> = Vec::new();
    for k in 0..strakes {
        let a0 = PI * k as f64 / strakes as f64;
        let step = PI / strakes as f64;
        if seams {
            cols.push((a0, k, true));
            cols.push((a0 + step * 0.07, k, false));
        } else {
            cols.push((a0, k, false));
        }
    }
    cols.push((PI, strakes - 1, false));
    let us: Vec<f64> = (0..=rows).map(|i| i as f64 / rows as f64).collect();
    let hull: Vec<Vec<DVec3>> = us
        .iter()
        .map(|&u| cols.iter().map(|&(a, _, _)| hull_at(u, a)).collect())
        .collect();
    // Which strakes are copper (round the keel), the red topside and the
    // dark wale under it.
    let mid_of = |k: usize| PI * (k as f64 + 0.5) / strakes as f64;
    let copper_k = |k: usize| mid_of(k).sin() > 0.8;
    let top_k = |k: usize| k == 0 || k + 1 == strakes;
    let wale_k = |k: usize| k == 1 || k + 2 == strakes;
    surface(b, &hull, false, |i, j| {
        let (_, k, seam) = cols[j];
        if seam {
            return plain(if copper_k(k) {
                rgb(120, 70, 44)
            } else {
                rgb(52, 32, 18)
            });
        }
        if copper_k(k) {
            // Sheathing plates: a slightly different tone every two rows.
            let t = jitter(k as u32 * 7 + (i / 2) as u32, 3) * 0.06;
            return metal(shade(copper, 1.0 + t));
        }
        if top_k(k) {
            return plain(red);
        }
        if wale_k(k) {
            return plain(dark);
        }
        if lod == Lod::Far {
            return plain(wood);
        }
        // Planks butt-jointed every few rows, staggered strake to strake,
        // each its own shade of oak.
        let plank = (i + k * 3) / 5;
        let t = jitter(k as u32, plank as u32);
        plain(shade(if t > 0.3 { wood2 } else { wood }, 1.0 + t * 0.08))
    });
    // The transom: the flat stern closing the hull, both faces.
    let last = &hull[hull.len() - 1];
    let mid: Vec<DVec3> = last
        .iter()
        .map(|p| DVec3::new(0.0, (p.y + deck_y) * 0.5, p.z))
        .collect();
    let transom = [last.clone(), mid];
    b.grid(&transom, false, true, |_, _| plain(wood2));
    b.grid(&transom, false, false, |_, _| plain(wood2));
    // The deck at the bulwarks' foot, planked fore and aft; the bulwarks'
    // inner faces up to the gunwale.
    let planks = lod.pick(9, 5, 1);
    let deck: Vec<Vec<DVec3>> = us
        .iter()
        .map(|&u| {
            let p0 = hull_at(u, 0.0);
            let w = p0.x.abs() * 0.97;
            (0..=planks)
                .map(|q| DVec3::new(-w + 2.0 * w * q as f64 / planks as f64, deck_y, p0.z))
                .collect()
        })
        .collect();
    b.grid(&deck, false, true, |i, q| {
        let t = jitter(q as u32 + 40, ((i + q * 2) / 6) as u32);
        plain(shade(deck_wood, 1.0 + t * 0.07))
    });
    for a in [0.0, PI] {
        let wall: Vec<Vec<DVec3>> = us
            .iter()
            .map(|&u| {
                let g = hull_at(u, a);
                vec![g, DVec3::new(g.x * 0.97, deck_y, g.z)]
            })
            .collect();
        b.grid(&wall, false, true, |_, _| plain(wood2));
        b.grid(&wall, false, false, |_, _| plain(wood2));
    }
    // The keel along the bottom and the stem post sweeping up the bow.
    let keel: Vec<DVec3> = us
        .iter()
        .filter(|&&u| u >= 0.04)
        .map(|&u| hull_at(u, PI * 0.5) - DVec3::Y * 0.06)
        .collect();
    tube(b, &keel, (0.1, 0.12), lod.pick(8, 6, 4), plain(dark));
    if lod == Lod::Far {
        return;
    }
    let stem: Vec<DVec3> = (0..=10)
        .map(|s| {
            let a = PI * 0.5 * (1.0 - s as f64 / 10.0);
            let p = hull_at(0.0, a);
            DVec3::new(0.0, p.y, p.z + 0.08)
        })
        .chain([hull_at(0.0, 0.0) + DVec3::new(0.0, 0.55, 0.25)])
        .collect();
    tube(b, &spline(&stem, 2, false), (0.09, 0.07), 6, plain(dark));
    // A gilded line along the wale, and the wooden cap on the gunwale.
    let side_seg = lod.pick(6, 4, 4);
    for a in [0.0, PI] {
        let cap: Vec<DVec3> = us
            .iter()
            .map(|&u| hull_at(u, a) + DVec3::Y * 0.04)
            .collect();
        tube(b, &cap, (0.08, 0.05), side_seg, plain(wood2));
        // A gilded moulding just under the cap, along the red topside.
        let gilt: Vec<DVec3> = us
            .iter()
            .filter(|&&u| u > 0.02)
            .map(|&u| {
                let aa = if a == 0.0 { 0.06 } else { PI - 0.06 };
                hull_at(u, aa) + hull_normal(u, aa) * 0.015
            })
            .collect();
        tube(b, &gilt, (0.03, 0.03), 4, metal(brass));
        let k = PI * 1.04 / strakes as f64;
        let line: Vec<DVec3> = us
            .iter()
            .filter(|&&u| u > 0.03)
            .map(|&u| {
                let aa = if a == 0.0 { k } else { PI - k };
                hull_at(u, aa) + hull_normal(u, aa) * 0.015
            })
            .collect();
        tube(b, &line, (0.035, 0.035), 4, metal(brass));
    }
    // Brass portholes along the topsides.
    let port_n = lod.pick(7, 7, 0);
    let ring_seg = lod.pick(18, 8, 6);
    for k in 0..port_n {
        let u = 0.2 + 0.55 * k as f64 / (port_n - 1).max(1) as f64;
        for a in [PI * 1.55 / strakes as f64, PI - PI * 1.55 / strakes as f64] {
            let p = hull_at(u, a);
            let n = hull_normal(u, a);
            if lod == Lod::Near {
                lathe(
                    b,
                    p,
                    n,
                    &[
                        (0.13, -0.02),
                        (0.2, 0.0),
                        (0.2, 0.05),
                        (0.15, 0.07),
                        (0.13, 0.05),
                    ],
                    ring_seg,
                    |_| metal(brass),
                );
            }
            lathe(
                b,
                p + n * 0.04,
                n,
                &[(0.0, 0.02), (0.14, 0.0), (0.14, -0.01)],
                ring_seg,
                |_| metal(rgb(36, 52, 62)),
            );
        }
    }
    // The stern: three glazed windows in brass frames, and a lantern at
    // each quarter.
    let back = hull_at(1.0, 0.0);
    let tz = back.z - 0.02;
    let ty = back.y - 0.55;
    for x in [-0.95, 0.0, 0.95] {
        b.surf = Surf::Metal;
        b.oriented_box(
            DVec3::new(x, ty, tz),
            DVec3::X,
            DVec3::new(0.34, 0.3, 0.03),
            brass,
        );
        b.oriented_box(
            DVec3::new(x, ty, tz - 0.03),
            DVec3::X,
            DVec3::new(0.27, 0.23, 0.02),
            rgb(36, 52, 62),
        );
        if lod == Lod::Near {
            b.oriented_box(
                DVec3::new(x, ty, tz - 0.055),
                DVec3::X,
                DVec3::new(0.025, 0.23, 0.01),
                brass,
            );
        }
        b.surf = Surf::Plain;
    }
    for x in [-1.0, 1.0] {
        let at = DVec3::new(x * (back.x.abs() - 0.15), back.y + 0.1, back.z + 0.15);
        lathe(
            b,
            at,
            DVec3::Y,
            &[(0.0, 0.0), (0.12, 0.0), (0.14, 0.05), (0.14, 0.12)],
            ring_seg,
            |_| metal(brass),
        );
        lathe(
            b,
            at + DVec3::Y * 0.12,
            DVec3::Y,
            &[(0.11, 0.0), (0.11, 0.3)],
            ring_seg,
            |_| glow(rgb(255, 196, 120)),
        );
        lathe(
            b,
            at + DVec3::Y * 0.42,
            DVec3::Y,
            &[(0.15, 0.0), (0.06, 0.16), (0.0, 0.2)],
            ring_seg,
            |_| metal(brass),
        );
    }
    // A brass rail along both gunwales, on turned stanchions.
    for a in [0.0, PI] {
        let path: Vec<DVec3> = us.iter().map(|&u| hull_at(u, a) + DVec3::Y * 0.9).collect();
        tube(b, &spline(&path, 2, false), (0.045, 0.045), 6, metal(brass));
        for (k, &u) in us.iter().enumerate().step_by(lod.pick(4, 3, 3)) {
            if k == 0 {
                continue;
            }
            let p = hull_at(u, a) + DVec3::Y * 0.06;
            tube(b, &[p, p + DVec3::Y * 0.84], (0.03, 0.03), 5, metal(brass));
        }
    }
    // The wheelhouse forward: a rounded cabin with big windows.
    let cab_z = 3.5;
    let cab: Vec<Vec<DVec3>> = (0..=8)
        .map(|i| {
            let u = i as f64 / 8.0;
            let z = cab_z + 2.5 - u * 5.0;
            let w = 1.5 * (1.0 - (2.0 * u - 1.0).powi(4) * 0.3);
            (0..=10)
                .map(|j| {
                    let a = PI * j as f64 / 10.0;
                    DVec3::new(-w * a.cos(), deck_y + 2.1 * a.sin().powf(0.4), z)
                })
                .collect()
        })
        .collect();
    surface(b, &cab, false, |i, j| {
        if (3..7).contains(&j) || j == 0 || j == 9 {
            if (1..7).contains(&i) && (j == 1 || j == 8 || (4..6).contains(&j)) {
                return Some((Surf::Glass, rgb(70, 110, 130).with_alpha(0.35)));
            }
            plain(wood2)
        } else if (1..7).contains(&i) && (j == 1 || j == 2 || j == 7 || j == 8) {
            Some((Surf::Glass, rgb(70, 110, 130).with_alpha(0.35)))
        } else {
            plain(rgb(210, 196, 160))
        }
    });
    // A lamp in the wheelhouse.
    b.surf = Surf::Glow;
    b.oriented_box(
        DVec3::new(0.0, deck_y + 1.7, cab_z),
        DVec3::X,
        DVec3::new(0.12, 0.12, 0.12),
        rgb(255, 210, 140),
    );
    b.surf = Surf::Plain;
    // Cables from the gunwales up to the envelope.
    for z in [-6.0, -1.0, 4.0, 8.0] {
        for x in [-1.0, 1.0] {
            let top_t = (0.5 - z / AIRSHIP_LENGTH).clamp(0.0, 1.0);
            let r = envelope_r(top_t);
            let top = DVec3::new(x * r * 0.45, -r * 0.89, z);
            let foot = DVec3::new(x * 1.6, deck_y + 0.9, z);
            tube(b, &[foot, top], (0.03, 0.03), 4, plain(dark));
        }
    }
    // Engines on outriggers, brass with dark cowls, propellers astern.
    let prop_n = lod.pick(3, 3, 2);
    for x in [-1.0, 1.0] {
        let c = DVec3::new(x * 4.6, deck_y + 0.8, -2.0);
        tube(
            b,
            &[DVec3::new(x * 1.8, deck_y + 0.6, -2.0), c],
            (0.08, 0.08),
            6,
            metal(dark),
        );
        lathe(
            b,
            c + DVec3::Z * 1.4,
            -DVec3::Z,
            &[
                (0.0, 0.0),
                (0.35, 0.15),
                (0.6, 0.6),
                (0.65, 1.4),
                (0.55, 2.4),
                (0.25, 2.9),
                (0.0, 3.0),
            ],
            lod.pick(32, 14, 8),
            |k| if k == 2 { metal(dark) } else { metal(brass) },
        );
        for p in 0..prop_n {
            let a = TAU * p as f64 / prop_n as f64 + x;
            let d = DVec3::new(a.cos(), a.sin(), 0.0);
            let hub = c + DVec3::Z * -1.65;
            let blade: Vec<DVec3> = (0..5).map(|s| hub + d * (0.15 + 0.32 * s as f64)).collect();
            tube(b, &blade, (0.14, 0.025), 6, plain(rgb(110, 70, 36)));
        }
    }
    // The mast astern with the black flag: crossed bones and a skull.
    let mast = DVec3::new(0.0, deck_y, -len * 0.5 + 1.5);
    tube(
        b,
        &[mast, mast + DVec3::Y * 4.2],
        (0.07, 0.07),
        6,
        plain(wood2),
    );
    let fl = mast + DVec3::Y * 4.0;
    let flag: Vec<Vec<DVec3>> = (0..=6)
        .map(|i| {
            let u = i as f64 / 6.0;
            let wave = (u * 5.0).sin() * 0.18 * u;
            vec![
                fl + DVec3::new(wave, 0.0, -u * 2.6),
                fl + DVec3::new(wave, -1.6, -u * 2.6),
            ]
        })
        .collect();
    b.grid(&flag, false, true, |_, _| plain(rgb(20, 20, 22)));
    inside(|| b.grid(&flag, false, true, |_, _| plain(rgb(20, 20, 22))));
    let mid = fl + DVec3::new(0.0, -0.85, -1.3);
    for side in [0.06, -0.06] {
        let off = DVec3::X * side;
        b.surf = Surf::Plain;
        b.oriented_box(
            mid + off - DVec3::Y * 0.15,
            DVec3::new(0.0, 0.6, 0.8).normalize(),
            DVec3::new(0.02, 0.05, 0.45),
            rgb(236, 232, 220),
        );
        b.oriented_box(
            mid + off - DVec3::Y * 0.15,
            DVec3::new(0.0, -0.6, 0.8).normalize(),
            DVec3::new(0.02, 0.05, 0.45),
            rgb(236, 232, 220),
        );
        let skull = mid + off + DVec3::Y * 0.22;
        lathe(
            b,
            skull - DVec3::X * 0.02 * side.signum(),
            DVec3::X * side.signum(),
            &[(0.0, 0.0), (0.2, 0.0), (0.2, 0.01), (0.0, 0.02)],
            14,
            |_| plain(rgb(236, 232, 220)),
        );
    }
}

/// A rope ladder from `top` down to `foot`: two ropes and wooden rungs.
pub fn rope_ladder(top: DVec3, foot: DVec3) -> Mesh {
    let mut b = MeshBuilder::new();
    let rope = rgb(170, 140, 96);
    let rung = rgb(110, 72, 40);
    let down = foot - top;
    let side = DVec3::new(down.z, 0.0, -down.x).normalize_or(DVec3::X) * 0.25;
    let side = if side.length_squared() < 1e-6 {
        DVec3::X * 0.25
    } else {
        side
    };
    // Sagging a little, as a rope does.
    let sag = |s: f64| top + down * s + DVec3::new(0.3, 0.0, 0.2) * (PI * s).sin();
    for k in [-1.0, 1.0] {
        let path: Vec<DVec3> = (0..=10).map(|i| sag(i as f64 / 10.0) + side * k).collect();
        tube(&mut b, &path, (0.025, 0.025), 5, plain(rope));
    }
    let n = (down.length() / 0.4) as usize;
    for i in 1..n {
        let p = sag(i as f64 / n as f64);
        tube(
            &mut b,
            &[p - side, p + side],
            (0.022, 0.022),
            5,
            plain(rung),
        );
    }
    b.finish()
}

/// The airship in three levels: close, down the street, across the town.
pub fn airship_levels() -> Vec<LodLevel> {
    vec![
        LodLevel::new(airship(Lod::Near), 90.0),
        LodLevel::new(airship(Lod::Mid), 400.0),
        LodLevel::new(airship(Lod::Far), 4000.0),
    ]
}

/// The sky pirates' airship moored over the harbour, a rope ladder down to
/// their lookouts on the quay (see `hw_sim::roosts`). Returns its LOD set
/// and the ladder.
pub fn spawn_roost(
    world: &mut World,
    town: &hw_gen::urban::world::TownWorld,
) -> EngineResult<(Vec<LodId>, Vec<EntityId>)> {
    match roost_parts(town) {
        Some(r) => install_roost(world, r),
        None => Ok((Vec::new(), Vec::new())),
    }
}

/// The airship's levels and the ladder, where they go. Pure data, no
/// `World`: the start makes them on a worker thread.
pub struct RoostParts {
    levels: Vec<LodLevel>,
    place: Place,
    ladder: Mesh,
    ladder_place: Place,
}

/// The roost's parts, if the town has one.
pub fn roost_parts(town: &hw_gen::urban::world::TownWorld) -> Option<RoostParts> {
    let r = hw_sim::roosts::pirate_roost(town)?;
    let ship = r.ship();
    let yaw = r.ship_yaw();
    let place = Place::new(ship.x as f32, ship.y as f32, ship.z as f32)
        .with_yaw_deg(yaw.to_degrees() as f32);
    // From the gondola's rail on the quay side down to the quay's edge.
    let side = DVec3::new(-r.out.x, 0.0, -r.out.y);
    let top = ship - DVec3::Y * GONDOLA_DECK + side * 1.9;
    let foot = DVec3::new(r.quay.x, r.top + 0.05, r.quay.y) + side * 0.6;
    Some(RoostParts {
        levels: airship_levels(),
        place,
        ladder: rope_ladder(DVec3::ZERO, foot - top),
        ladder_place: Place::new(top.x as f32, top.y as f32, top.z as f32),
    })
}

/// Puts the roost into the world: its LOD set and the ladder.
pub fn install_roost(
    world: &mut World,
    r: RoostParts,
) -> EngineResult<(Vec<LodId>, Vec<EntityId>)> {
    let lod = world.spawn_lod_at(r.levels, &[r.place])?;
    let ladder = world.place(r.ladder, r.ladder_place)?;
    Ok((vec![lod], vec![ladder]))
}

// ----------------------------------------------------------- iron capsule

/// The Iron-Masked Empire's drop capsule, crashed nose-down and tilted: a
/// riveted iron shell with brass bands, stabiliser fins, the open hatch
/// glowing red inside, and the Empire's iron mask on its flank. Origin at
/// the ground where it struck, the hatch towards +z.
pub fn iron_capsule(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let seg = lod.pick(40, 20, 10);
    let iron = rgb(64, 66, 72);
    let band = rgb(168, 132, 70);
    let red = rgb(255, 50, 30);
    let tilt = DVec3::new(0.12, 1.0, -0.18).normalize();
    // Half a metre of it is in the ground.
    let base = -tilt * 0.6;
    let profile = [
        (0.0, 0.0),
        (0.7, 0.25),
        (1.25, 0.8),
        (1.5, 1.4),
        (1.52, 1.5),
        (1.6, 1.55),
        (1.6, 1.7),
        (1.52, 1.75),
        (1.5, 3.2),
        (1.58, 3.25),
        (1.58, 3.4),
        (1.48, 3.45),
        (1.25, 4.0),
        (0.75, 4.45),
        (0.3, 4.62),
        (0.0, 4.65),
    ];
    lathe(&mut b, base, tilt, &profile, seg, |k| match k {
        4..=6 | 8..=10 => metal(band),
        _ => metal(iron),
    });
    if lod != Lod::Far {
        // Rivets round the bands.
        let (u, v) = {
            let helper = DVec3::X;
            let u = tilt.cross(helper).normalize();
            (u, tilt.cross(u).normalize())
        };
        let n = lod.pick(28, 14, 0);
        for h in [1.62, 3.32] {
            for k in 0..n {
                let a = TAU * k as f64 / n as f64;
                let p = base + tilt * h + (u * a.cos() + v * a.sin()) * 1.6;
                b.surf = Surf::Metal;
                b.oriented_box(
                    p,
                    u * a.cos() + v * a.sin(),
                    DVec3::new(0.04, 0.04, 0.04),
                    rgb(120, 96, 52),
                );
            }
        }
        b.surf = Surf::Plain;
        // Four stabiliser fins near the top (it fell tail-up).
        for k in 0..4 {
            let a = TAU * k as f64 / 4.0 + PI * 0.25;
            let out = u * a.cos() + v * a.sin();
            let side = tilt.cross(out).normalize();
            let fin: Vec<Vec<DVec3>> = [(1.45, 3.0, 4.3, 0.1), (2.4, 3.6, 4.45, 0.04)]
                .iter()
                .map(|&(r, lo, hi, th)| {
                    vec![
                        base + tilt * lo + out * r + side * th,
                        base + tilt * hi + out * r + side * th,
                        base + tilt * hi + out * r - side * th,
                        base + tilt * lo + out * r - side * th,
                    ]
                })
                .collect();
            surface(&mut b, &fin, true, |_, _| metal(iron));
        }
        // The Empire's mask on its flank (+x side): an iron face with
        // glowing slits for eyes.
        let face_n = (u * 0.0 + DVec3::X - tilt * DVec3::X.dot(tilt)).normalize();
        let face_up = tilt;
        let face_side = face_up.cross(face_n).normalize();
        let centre = base + tilt * 2.45 + face_n * 1.5;
        let mask: Vec<Vec<DVec3>> = (0..=6)
            .map(|i| {
                let y = -0.6 + 1.2 * i as f64 / 6.0;
                let w = 0.5 * (1.0 - (y / 0.62).powi(2)).max(0.0).sqrt() + 0.02;
                (0..=6)
                    .map(|j| {
                        let x = -w + 2.0 * w * j as f64 / 6.0;
                        let bulge = 0.08 * (1.0 - (x / 0.55).powi(2)) * (1.0 - (y / 0.7).powi(2));
                        centre + face_side * x + face_up * y + face_n * (0.04 + bulge)
                    })
                    .collect()
            })
            .collect();
        b.grid(&mask, false, false, |_, _| metal(rgb(150, 152, 160)));
        b.surf = Surf::Glow;
        for k in [-1.0, 1.0] {
            b.oriented_box(
                centre + face_side * (0.2 * k) + face_up * 0.12 + face_n * 0.14,
                face_side,
                DVec3::new(0.13, 0.03, 0.03),
                red,
            );
        }
        b.surf = Surf::Plain;
    }
    // The hatch, blown open: a dark doorway glowing red inside, the door
    // hanging from its lower hinge.
    let (u, v) = {
        let u = tilt.cross(DVec3::X).normalize();
        (u, tilt.cross(u).normalize())
    };
    let front = -v;
    let _ = u;
    let door_c = base + tilt * 2.4 + front * 1.52;
    let side = tilt.cross(front).normalize();
    b.surf = Surf::Glow;
    b.oriented_box(
        door_c - front * 0.04,
        side,
        DVec3::new(0.55, 0.85, 0.02),
        rgb(150, 24, 16),
    );
    b.surf = Surf::Metal;
    let hinge = door_c - tilt * 0.85 + front * 0.05;
    let open = (front * 0.85 - tilt * 0.5).normalize();
    b.oriented_box(
        hinge + open * 0.85,
        side,
        DVec3::new(0.58, 0.04, 0.85),
        rgb(70, 72, 78),
    );
    b.surf = Surf::Plain;
    b.finish()
}

pub fn iron_capsule_levels() -> Vec<LodLevel> {
    vec![
        LodLevel::new(iron_capsule(Lod::Near), 60.0),
        LodLevel::new(iron_capsule(Lod::Mid), 220.0),
        LodLevel::new(iron_capsule(Lod::Far), 1200.0),
    ]
}

// --------------------------------------------------------------- mole hole

/// Broken street round a hole: slabs of asphalt heaved up and tilted
/// outward, a ring of dug earth, the dark shaft going down, timber props
/// and a ladder the mole folk climbed up by. Origin at the hole's middle
/// on the street; `scorch` blackens the slabs (a capsule's crater).
pub fn broken_ground(lod: Lod, radius: f64, scorch: bool, ladder: bool) -> Mesh {
    let mut b = MeshBuilder::new();
    let asphalt = if scorch {
        rgb(50, 47, 45)
    } else {
        rgb(58, 58, 60)
    };
    let under = rgb(110, 100, 86);
    let earth = rgb(92, 66, 42);
    let earth2 = rgb(74, 52, 34);
    let seg = lod.pick(72, 28, 12);
    // The heap of dug earth round the rim: low, so people stand on it, not
    // in it, and lumpy in long smooth waves rather than facets.
    let r = radius;
    let rim: Vec<(f64, f64)> = vec![
        (r * 1.8, -0.04),
        (r * 1.55, 0.1),
        (r * 1.3, 0.24),
        (r * 1.1, 0.28),
        (r * 0.99, 0.16),
        (r * 0.95, 0.0),
    ];
    let rows: Vec<Vec<DVec3>> = rim
        .iter()
        .map(|&(rad, y)| {
            (0..seg)
                .map(|j| {
                    let a = TAU * j as f64 / seg as f64;
                    let wob = 1.0 + 0.06 * (3.0 * a + 0.7).sin() + 0.035 * (7.0 * a + 2.1).sin();
                    DVec3::new(rad * wob * a.cos(), y * wob, rad * wob * a.sin())
                })
                .collect()
        })
        .collect();
    surface(&mut b, &rows, true, |i, j| {
        if lod != Lod::Far && i == 3 && j % 7 < 3 {
            plain(earth2)
        } else {
            plain(earth)
        }
    });
    // The shaft: its wall seen from inside, its floor lost in the dark.
    let shaft: Vec<(f64, f64)> = vec![(r * 0.95, 0.0), (r * 0.9, -1.2), (r * 0.85, -2.6)];
    let rows: Vec<Vec<DVec3>> = shaft
        .iter()
        .map(|&(rad, y)| {
            (0..seg)
                .map(|j| {
                    let a = TAU * j as f64 / seg as f64;
                    DVec3::new(rad * a.cos(), y, rad * a.sin())
                })
                .collect()
        })
        .collect();
    inside(|| {
        surface(&mut b, &rows, true, |i, _| {
            plain(if i == 0 { earth } else { earth2 })
        })
    });
    let floor: Vec<DVec3> = (0..seg)
        .map(|j| {
            let a = TAU * j as f64 / seg as f64;
            DVec3::new(r * 0.85 * a.cos(), -2.6, r * 0.85 * a.sin())
        })
        .collect();
    b.polygon(&floor, DVec3::Y, rgb(12, 10, 9));
    if lod == Lod::Far {
        return b.finish();
    }
    // Broken slabs of road heaved up round it, each tipped outward: jagged
    // plates a hand thick, grey on top, pale where they broke.
    let n = lod.pick(16, 9, 0);
    for k in 0..n {
        let a = TAU * (k as f64 + 0.4 * jitter(k, 1)) / n as f64;
        let out = DVec3::new(a.cos(), 0.0, a.sin());
        let along = DVec3::new(-a.sin(), 0.0, a.cos());
        let tip = (0.25 + 0.3 * jitter(k, 2).abs()) * if k % 4 == 0 { 1.6 } else { 1.0 };
        let up = (DVec3::Y * tip.cos() + out * tip.sin()).normalize();
        let face_out = up.cross(along).normalize();
        let size = 0.4 + 0.25 * jitter(k, 4).abs();
        let c = out * (r * 1.1 + size * 0.6) + DVec3::Y * (0.12 + 0.18 * jitter(k, 6).abs());
        let corners: Vec<DVec3> = (0..6)
            .map(|i| {
                let t = TAU * (i as f64 + 0.3 * jitter(k * 7 + i, 9)) / 6.0;
                let rr = size * (0.7 + 0.3 * jitter(k * 7 + i, 10).abs());
                c + along * (rr * 1.3 * t.cos()) + face_out * (rr * t.sin())
            })
            .collect();
        let th = up * 0.1;
        let top: Vec<DVec3> = corners.iter().map(|p| *p + th).collect();
        b.polygon(&top, up, asphalt);
        b.polygon(&corners, -up, under);
        for i in 0..6 {
            let j = (i + 1) % 6;
            let mid = (corners[i] + corners[j]) * 0.5 - c;
            b.quad(
                [corners[i], corners[j], top[j], top[i]],
                mid.normalize(),
                under,
            );
        }
    }
    // Timber props and a ladder for the mole folk.
    let timber = rgb(112, 80, 48);
    for k in 0..3 {
        let a = TAU * k as f64 / 3.0 + 0.4;
        let out = DVec3::new(a.cos(), 0.0, a.sin());
        let foot = out * r * 0.85 + DVec3::Y * -1.6;
        let top = out * r * 1.05 + DVec3::Y * 0.4;
        tube(&mut b, &[foot, top], (0.08, 0.08), 6, plain(timber));
    }
    if ladder {
        let out = DVec3::new(0.0, 0.0, -1.0);
        let side = DVec3::X * 0.22;
        let foot = out * r * 0.5 + DVec3::Y * -2.5;
        let top = out * r * 0.95 + DVec3::Y * 1.2;
        for k in [-1.0, 1.0] {
            tube(
                &mut b,
                &[foot + side * k, top + side * k],
                (0.035, 0.035),
                6,
                plain(timber),
            );
        }
        for i in 1..10 {
            let p = foot + (top - foot) * (i as f64 / 10.0);
            tube(
                &mut b,
                &[p - side, p + side],
                (0.025, 0.025),
                5,
                plain(timber),
            );
        }
        // A miner's lamp hung on the ladder.
        let lamp = top - (top - foot) * 0.12 + side * 1.4;
        lathe(
            &mut b,
            lamp,
            DVec3::Y,
            &[
                (0.0, -0.1),
                (0.07, -0.08),
                (0.08, 0.06),
                (0.03, 0.12),
                (0.0, 0.13),
            ],
            12,
            |k| {
                if k == 1 {
                    glow(rgb(255, 200, 120))
                } else {
                    metal(rgb(150, 120, 60))
                }
            },
        );
    }
    b.finish()
}

pub fn broken_ground_levels(radius: f64, scorch: bool, ladder: bool) -> Vec<LodLevel> {
    vec![
        LodLevel::new(broken_ground(Lod::Near, radius, scorch, ladder), 50.0),
        LodLevel::new(broken_ground(Lod::Mid, radius, scorch, ladder), 160.0),
        LodLevel::new(broken_ground(Lod::Far, radius, scorch, ladder), 500.0),
    ]
}

// ----------------------------------------------------------------- weapons

/// What a fighter carries in the right hand, the grip about the origin
/// and the weapon running up +y (the way a fist holds a stick), the edge
/// or point forward along +z.
pub fn weapon(kind: hw_net::EntityKind) -> Option<Mesh> {
    use hw_net::EntityKind as K;
    let mut b = MeshBuilder::new();
    match kind {
        K::Delver => pick(&mut b),
        K::Buccaneer => cutlass(&mut b, false),
        K::Captain => cutlass(&mut b, true),
        K::Praetorian => lance(&mut b),
        K::Doctor => tesla_staff(&mut b),
        K::Bulwark => shield(&mut b),
        K::Grenadier => bomb(&mut b, DVec3::Y * 0.1),
        K::Orderly => mender(&mut b),
        _ => return None,
    }
    Some(b.finish())
}

/// The bone a villain's weapon hangs from: the shield on the left arm,
/// everything else in the right hand.
pub fn weapon_bone(kind: hw_net::EntityKind) -> &'static str {
    if kind == hw_net::EntityKind::Bulwark {
        "hand_l"
    } else {
        "hand_r"
    }
}

/// A shield bearer's shield: a riveted steel lens 0.9 m across, domed to
/// the front, with a brass rim and the Doctor's glowing atom on its boss.
/// Its axis is the hand's x (out from the arm), the grip at the middle.
fn shield(b: &mut MeshBuilder) {
    let steel = rgb(92, 96, 104);
    let brass = rgb(176, 140, 70);
    let atom = rgb(120, 255, 140);
    // The left hand's -X points away from the body: the face looks out.
    let axis = -DVec3::X;
    let o = DVec3::new(-0.06, 0.1, 0.0);
    lathe(
        b,
        o,
        axis,
        &[
            (0.0, -0.02),
            (0.3, -0.03),
            (0.43, -0.012),
            (0.455, 0.008),
            (0.44, 0.03),
            (0.4, 0.045),
            (0.25, 0.08),
            (0.1, 0.095),
            (0.0, 0.1),
        ],
        32,
        |k| match k {
            2..=4 => metal(brass),
            _ => metal(steel),
        },
    );
    // The boss and the atom's ring on it.
    lathe(
        b,
        o + axis * 0.09,
        axis,
        &[
            (0.0, 0.0),
            (0.09, 0.0),
            (0.08, 0.03),
            (0.04, 0.05),
            (0.0, 0.055),
        ],
        24,
        |_| metal(brass),
    );
    for k in 0..3 {
        let a = PI * k as f64 / 3.0;
        let (u, v) = (
            DVec3::new(0.0, a.cos(), a.sin()),
            DVec3::new(0.0, -a.sin(), a.cos()),
        );
        let ring: Vec<DVec3> = (0..=24)
            .map(|i| {
                let t = TAU * i as f64 / 24.0;
                o + axis * 0.12 + u * (0.2 * t.cos()) + v * (0.07 * t.sin())
            })
            .collect();
        tube(b, &ring, (0.008, 0.008), 6, glow(atom));
    }
    // Rivets round the rim.
    for k in 0..16 {
        let a = TAU * k as f64 / 16.0;
        let p = o + axis * 0.035 + DVec3::new(0.0, a.cos(), a.sin()) * 0.38;
        lathe(
            b,
            p,
            axis,
            &[(0.0, 0.0), (0.014, 0.0), (0.01, 0.01), (0.0, 0.014)],
            8,
            |_| metal(brass),
        );
    }
}

/// A grenadier's bomb: a black iron ball with a brass cap and a lit fuse,
/// centred at `at`. The same ball flies when it is lobbed.
pub fn bomb(b: &mut MeshBuilder, at: DVec3) {
    let iron = rgb(30, 32, 36);
    let brass = rgb(190, 150, 70);
    let spark = rgb(255, 200, 90);
    let r = 0.11;
    let profile: Vec<(f64, f64)> = (0..=12)
        .map(|i| {
            let a = PI * i as f64 / 12.0;
            (r * a.sin(), -r * a.cos())
        })
        .collect();
    lathe(b, at, DVec3::Y, &profile, 24, |_| metal(iron));
    lathe(
        b,
        at + DVec3::Y * (r - 0.01),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.035, 0.0),
            (0.035, 0.03),
            (0.02, 0.04),
            (0.0, 0.04),
        ],
        16,
        |_| metal(brass),
    );
    let fuse = [
        at + DVec3::Y * (r + 0.03),
        at + DVec3::new(0.02, r + 0.07, 0.0),
        at + DVec3::new(0.05, r + 0.09, 0.01),
    ];
    tube(b, &fuse, (0.006, 0.006), 6, plain(rgb(120, 100, 70)));
    lathe(
        b,
        fuse[2],
        DVec3::Y,
        &[(0.0, -0.015), (0.015, 0.0), (0.0, 0.015)],
        8,
        |_| glow(spark),
    );
}

/// The bomb alone, for one in the air.
pub fn bomb_mesh() -> Mesh {
    let mut b = MeshBuilder::new();
    bomb(&mut b, DVec3::ZERO);
    b.finish()
}

/// A lab orderly's mending ray: a cream bakelite grip, a chrome barrel
/// and a green glass bulb that glows.
fn mender(b: &mut MeshBuilder) {
    let cream = rgb(232, 224, 200);
    let chrome = rgb(210, 214, 222);
    let green = rgb(110, 255, 150);
    lathe(
        b,
        DVec3::Y * -0.04,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.022, 0.0),
            (0.02, 0.1),
            (0.024, 0.12),
            (0.0, 0.13),
        ],
        12,
        |_| plain(cream),
    );
    lathe(
        b,
        DVec3::new(0.0, 0.1, 0.0),
        DVec3::Z,
        &[
            (0.0, -0.05),
            (0.03, -0.05),
            (0.026, 0.06),
            (0.04, 0.09),
            (0.03, 0.12),
            (0.0, 0.12),
        ],
        16,
        |k| {
            if k == 3 {
                metal(rgb(176, 140, 70))
            } else {
                metal(chrome)
            }
        },
    );
    let bulb: Vec<(f64, f64)> = (0..=8)
        .map(|i| {
            let a = PI * i as f64 / 8.0;
            (0.035 * a.sin(), -0.035 * a.cos())
        })
        .collect();
    lathe(b, DVec3::new(0.0, 0.1, 0.155), DVec3::Z, &bulb, 16, |_| {
        glow(green)
    });
}

/// A miner's pick: an ash handle and a curved iron head.
fn pick(b: &mut MeshBuilder) {
    let ash = rgb(150, 112, 70);
    let iron = rgb(80, 82, 88);
    lathe(
        b,
        DVec3::Y * -0.18,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.024, 0.0),
            (0.02, 0.4),
            (0.024, 0.8),
            (0.03, 0.86),
            (0.0, 0.87),
        ],
        12,
        |_| plain(ash),
    );
    let head: Vec<DVec3> = (0..=8)
        .map(|i| {
            let u = i as f64 / 8.0 * 2.0 - 1.0;
            DVec3::new(0.0, 0.66 - 0.07 * u * u, u * 0.3)
        })
        .collect();
    tube(b, &head, (0.03, 0.025), 8, metal(iron));
    // Points.
    for (z, dir) in [(0.3, 1.0), (-0.3, -1.0)] {
        lathe(
            b,
            DVec3::new(0.0, 0.59, z),
            DVec3::new(0.0, -0.3, dir).normalize(),
            &[(0.03, 0.0), (0.0, 0.1)],
            8,
            |_| metal(iron),
        );
    }
}

/// A pirate's cutlass: a leather grip, a brass basket guard, a broad
/// curved blade. The captain's is longer, with a gilded guard.
fn cutlass(b: &mut MeshBuilder, captain: bool) {
    let steel = rgb(206, 210, 218);
    let brass = if captain {
        rgb(222, 184, 80)
    } else {
        rgb(176, 140, 70)
    };
    lathe(
        b,
        DVec3::Y * -0.06,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.03, 0.0),
            (0.022, 0.03),
            (0.022, 0.13),
            (0.03, 0.15),
            (0.0, 0.16),
        ],
        10,
        |k| {
            if k == 0 || k == 4 {
                metal(brass)
            } else {
                plain(rgb(60, 36, 22))
            }
        },
    );
    // The basket guard: a shell round the knuckles.
    let guard: Vec<Vec<DVec3>> = (0..=6)
        .map(|i| {
            let a = PI * (0.1 + 0.8 * i as f64 / 6.0);
            (0..=6)
                .map(|j| {
                    let y = -0.06 + 0.2 * j as f64 / 6.0;
                    let r = 0.07 + 0.02 * (PI * j as f64 / 6.0).sin();
                    DVec3::new(-r * a.sin() * 0.6, y, r * a.cos() + 0.04)
                })
                .collect()
        })
        .collect();
    b.grid(&guard, false, true, |_, _| metal(brass));
    inside(|| b.grid(&guard, false, true, |_, _| metal(brass)));
    let len = if captain { 0.82 } else { 0.7 };
    let blade: Vec<DVec3> = (0..=10)
        .map(|i| {
            let u = i as f64 / 10.0;
            DVec3::new(0.0, 0.14 + u * len, 0.03 + 0.12 * u * u)
        })
        .collect();
    // Broad near the point, as a cutlass is: a flat ellipse, wide along z.
    let rows: Vec<Vec<DVec3>> = blade
        .iter()
        .enumerate()
        .map(|(i, &p)| {
            let u = i as f64 / 10.0;
            let w = if i == 10 { 0.002 } else { 0.018 + 0.01 * u };
            let t = 0.004;
            (0..8)
                .map(|j| {
                    let a = TAU * j as f64 / 8.0;
                    p + DVec3::new(t * a.sin(), 0.0, w * a.cos())
                })
                .collect()
        })
        .collect();
    surface(b, &rows, true, |_, _| metal(steel));
}

/// A praetorian's shock lance: an iron shaft, a brass collar, a forked
/// head crackling blue-white.
fn lance(b: &mut MeshBuilder) {
    let iron = rgb(70, 72, 80);
    let brass = rgb(176, 140, 70);
    let arc = rgb(150, 210, 255);
    lathe(
        b,
        DVec3::Y * -0.7,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.035, 0.0),
            (0.03, 0.05),
            (0.025, 1.9),
            (0.045, 1.95),
            (0.05, 2.1),
            (0.03, 2.15),
            (0.0, 2.16),
        ],
        12,
        |k| {
            if (4..6).contains(&k) {
                metal(brass)
            } else {
                metal(iron)
            }
        },
    );
    for k in [-1.0, 1.0] {
        let path = [
            DVec3::new(0.0, 1.42, 0.0),
            DVec3::new(0.0, 1.55, 0.06 * k),
            DVec3::new(0.0, 1.75, 0.07 * k),
            DVec3::new(0.0, 1.85, 0.03 * k),
        ];
        tube(b, &path, (0.016, 0.016), 6, metal(iron));
    }
    lathe(
        b,
        DVec3::Y * 1.5,
        DVec3::Y,
        &[(0.0, 0.0), (0.03, 0.05), (0.022, 0.3), (0.0, 0.42)],
        10,
        |_| glow(arc),
    );
}

/// The Doctor's Tesla staff: a black staff with brass rings, a coil wound
/// round its head, and a glowing Aether sphere caged at its top.
fn tesla_staff(b: &mut MeshBuilder) {
    let ebony = rgb(28, 26, 30);
    let brass = rgb(186, 148, 72);
    let copper = rgb(176, 98, 52);
    let aether = rgb(120, 255, 160);
    lathe(
        b,
        DVec3::Y * -0.75,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.03, 0.0),
            (0.028, 0.05),
            (0.024, 1.55),
            (0.04, 1.6),
            (0.045, 1.66),
            (0.03, 1.7),
            (0.0, 1.71),
        ],
        16,
        |k| {
            if (3..6).contains(&k) {
                metal(brass)
            } else {
                plain(ebony)
            }
        },
    );
    // Brass rings down the shaft.
    for y in [-0.6, 0.0, 0.5] {
        lathe(
            b,
            DVec3::Y * y,
            DVec3::Y,
            &[
                (0.024, -0.02),
                (0.034, -0.012),
                (0.034, 0.012),
                (0.024, 0.02),
            ],
            16,
            |_| metal(brass),
        );
    }
    // The coil: copper wound up the head.
    let coil: Vec<DVec3> = (0..=48)
        .map(|i| {
            let t = i as f64 / 48.0;
            let a = t * std::f64::consts::TAU * 6.0;
            DVec3::new(a.cos() * 0.036, 0.62 + t * 0.26, a.sin() * 0.036)
        })
        .collect();
    tube(b, &coil, (0.009, 0.009), 6, metal(copper));
    // The cage: four brass ribs round the sphere.
    for k in 0..4 {
        let a = k as f64 * std::f64::consts::FRAC_PI_2;
        let (x, z) = (a.cos(), a.sin());
        let rib: Vec<DVec3> = (0..=8)
            .map(|i| {
                let t = i as f64 / 8.0 * std::f64::consts::PI;
                let r = 0.02 + t.sin() * 0.07;
                DVec3::new(x * r, 0.96 + 0.08 - t.cos() * 0.08, z * r)
            })
            .collect();
        tube(b, &rib, (0.008, 0.008), 6, metal(brass));
    }
    // The Aether sphere.
    let sphere: Vec<(f64, f64)> = (0..=10)
        .map(|i| {
            let t = i as f64 / 10.0 * std::f64::consts::PI;
            (t.sin() * 0.055, -t.cos() * 0.055)
        })
        .collect();
    lathe(b, DVec3::Y * 1.04, DVec3::Y, &sphere, 16, |_| glow(aether));
}

/// Where the hand bone holds a weapon (see [`weapon`]).
pub fn weapon_grip() -> Place {
    Place::new(0.0, 0.08, 0.03)
}

/// Unit vector in the ground plane from a yaw (radians, 0 = +z).
pub fn flat_dir(yaw: f64) -> DVec2 {
    DVec2::new(yaw.sin(), yaw.cos())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_new_things_are_built_at_full_quality() {
        let tris = |m: Mesh| m.build().triangle_count();
        let near = tris(airship(Lod::Near));
        let mid = tris(airship(Lod::Mid));
        let far = tris(airship(Lod::Far));
        assert!((5_000..40_000).contains(&near), "airship near {near}");
        assert!(mid * 2 < near, "airship mid {mid} vs near {near}");
        assert!(far < 2_000, "airship far {far}");
        let capsule = tris(iron_capsule(Lod::Near));
        assert!((1_500..15_000).contains(&capsule), "capsule {capsule}");
        let hole = tris(broken_ground(Lod::Near, 1.6, false, true));
        assert!((600..10_000).contains(&hole), "hole {hole}");
        for k in [
            hw_net::EntityKind::Delver,
            hw_net::EntityKind::Buccaneer,
            hw_net::EntityKind::Captain,
            hw_net::EntityKind::Praetorian,
        ] {
            let w = weapon(k).expect("a weapon");
            assert!(tris(w) > 100, "{k:?}");
        }
        assert!(weapon(hw_net::EntityKind::Gunner).is_none());
    }
}
