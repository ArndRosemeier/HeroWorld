//! Street furniture at full quality: lamps, traffic lights, benches, bins,
//! hydrants, rocket mailboxes and bollards. Turned parts are lathed and
//! shade smooth, bent iron is swept tube, planks stay crisp boxes; every
//! model has three levels ([`Lod`]).
//!
//! Units are metres, +y up, standing on y = 0, facing +z (the road for
//! lamps and lights, the sitter's view for benches).

use crate::carmesh::{lathe, slab, spline, tube, Lod, Paint};
use crate::citymesh::{MeshBuilder, Surf};
use engine::prelude::*;
use glam::DVec3;
use hw_gen::urban::props::{BarricadePiece, LampStyle};
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

const WARM: Color = Color {
    r: 1.0,
    g: 0.70,
    b: 0.36,
    a: 1.0,
};

fn sides(lod: Lod, near: usize) -> usize {
    lod.pick(near, (near / 2).max(8), 6)
}

/// A cast-iron post: a stepped, moulded base, a fluted-looking shaft (rings
/// every so often) and a collar at `top`.
fn iron_post(b: &mut MeshBuilder, lod: Lod, r: f64, top: f64, col: Color) {
    let seg = sides(lod, 24);
    let mut prof = vec![
        (0.0, 0.0),
        (r * 2.6, 0.0),
        (r * 2.6, 0.06),
        (r * 2.2, 0.1),
        (r * 2.1, 0.3),
        (r * 1.6, 0.42),
        (r * 1.7, 0.5),
        (r * 1.2, 0.62),
        (r * 1.05, 0.9),
    ];
    if lod == Lod::Near {
        // Rings up the shaft.
        let mut y = 1.4;
        while y < top - 0.5 {
            prof.extend([(r, y - 0.03), (r * 1.25, y), (r, y + 0.03)]);
            y += 0.9;
        }
    }
    prof.extend([
        (r * 0.9, top - 0.12),
        (r * 1.5, top - 0.06),
        (r * 1.5, top),
        (0.0, top + 0.01),
    ]);
    lathe(b, DVec3::ZERO, DVec3::Y, &prof, seg, |_| metal(col));
}

/// A street lamp, its light towards +z.
pub fn lamp(style: LampStyle, lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    match style {
        LampStyle::Lantern => {
            let iron = rgb(36, 42, 38);
            iron_post(&mut b, lod, 0.065, 3.75, iron);
            // The lantern: a bowl, a glowing globe in four panes' frame, a
            // cap with a finial.
            let seg = sides(lod, 24);
            let h = 3.76;
            lathe(
                &mut b,
                DVec3::Y * h,
                DVec3::Y,
                &[
                    (0.0, 0.0),
                    (0.09, 0.0),
                    (0.15, 0.05),
                    (0.2, 0.1),
                    (0.0, 0.1),
                ],
                seg,
                |_| metal(iron),
            );
            lathe(
                &mut b,
                DVec3::Y * (h + 0.1),
                DVec3::Y,
                &[
                    (0.0, 0.0),
                    (0.17, 0.0),
                    (0.21, 0.15),
                    (0.24, 0.4),
                    (0.22, 0.56),
                    (0.0, 0.58),
                ],
                seg,
                |_| glow(WARM),
            );
            lathe(
                &mut b,
                DVec3::Y * (h + 0.64),
                DVec3::Y,
                &[
                    (0.0, -0.04),
                    (0.3, -0.04),
                    (0.31, 0.0),
                    (0.2, 0.12),
                    (0.08, 0.24),
                    (0.05, 0.3),
                    (0.08, 0.36),
                    (0.0, 0.45),
                ],
                seg,
                |_| metal(iron),
            );
            if lod != Lod::Far {
                // The frame's ribs, and the ladder bar under the lantern.
                for k in 0..4 {
                    let a = TAU * (k as f64 + 0.5) / 4.0;
                    let d = DVec3::new(a.cos(), 0.0, a.sin());
                    tube(
                        &mut b,
                        &[
                            DVec3::Y * (h + 0.1) + d * 0.18,
                            DVec3::Y * (h + 0.38) + d * 0.255,
                            DVec3::Y * (h + 0.62) + d * 0.29,
                        ],
                        (0.012, 0.012),
                        6,
                        metal(iron),
                    );
                }
                tube(
                    &mut b,
                    &[DVec3::new(-0.38, 3.45, 0.0), DVec3::new(0.38, 3.45, 0.0)],
                    (0.02, 0.02),
                    8,
                    metal(iron),
                );
            }
        }
        LampStyle::Deco => {
            let bronze = rgb(58, 76, 64);
            iron_post(&mut b, lod, 0.09, 5.7, bronze);
            let seg = sides(lod, 20);
            // Two arms curling out to the globes.
            for sx in [-1.0, 1.0] {
                let p = |x: f64, y: f64| DVec3::new(sx * x, y, 0.0);
                let arm = spline(
                    &[p(0.0, 5.2), p(0.3, 5.7), p(0.7, 5.95), p(0.95, 5.85)],
                    lod.pick(6, 3, 1),
                    false,
                );
                tube(&mut b, &arm, (0.04, 0.04), sides(lod, 10), metal(bronze));
                if lod == Lod::Near {
                    let scroll = spline(
                        &[p(0.08, 5.0), p(0.3, 5.35), p(0.5, 5.6), p(0.42, 5.42)],
                        5,
                        false,
                    );
                    tube(&mut b, &scroll, (0.018, 0.018), 6, metal(bronze));
                }
                let c = p(0.95, 5.88);
                lathe(
                    &mut b,
                    c,
                    -DVec3::Y,
                    &[
                        (0.0, -0.06),
                        (0.1, -0.06),
                        (0.13, 0.0),
                        (0.06, 0.06),
                        (0.0, 0.06),
                    ],
                    seg,
                    |_| metal(bronze),
                );
                let globe: Vec<(f64, f64)> = (0..=10)
                    .map(|k| {
                        let a = PI * k as f64 / 10.0;
                        (0.27 * a.sin(), 0.06 + 0.27 * (1.0 - a.cos()))
                    })
                    .collect();
                lathe(&mut b, c, -DVec3::Y, &globe, seg, |_| glow(WARM));
            }
            lathe(
                &mut b,
                DVec3::Y * 5.7,
                DVec3::Y,
                &[
                    (0.0, 0.0),
                    (0.07, 0.0),
                    (0.05, 0.4),
                    (0.02, 0.75),
                    (0.0, 0.8),
                ],
                sides(lod, 12),
                |_| metal(bronze),
            );
        }
        LampStyle::Atomic => {
            let chrome = rgb(214, 220, 224);
            let accent = rgb(64, 186, 180);
            let seg = sides(lod, 24);
            lathe(
                &mut b,
                DVec3::ZERO,
                DVec3::Y,
                &[
                    (0.0, 0.0),
                    (0.26, 0.0),
                    (0.26, 0.05),
                    (0.14, 0.2),
                    (0.11, 1.0),
                    (0.09, 2.5),
                    (0.16, 2.56),
                    (0.16, 2.66),
                    (0.09, 2.72),
                    (0.06, 6.8),
                    (0.0, 6.85),
                ],
                seg,
                |k| {
                    if matches!(k, 5..=7) {
                        plain(accent)
                    } else {
                        metal(chrome)
                    }
                },
            );
            // A swan-neck arm out over the road.
            let arm = spline(
                &[
                    DVec3::new(0.0, 6.4, 0.0),
                    DVec3::new(0.0, 6.95, 0.3),
                    DVec3::new(0.0, 7.0, 1.0),
                    DVec3::new(0.0, 6.92, 1.6),
                ],
                lod.pick(6, 3, 1),
                false,
            );
            tube(&mut b, &arm, (0.05, 0.05), sides(lod, 10), metal(chrome));
            // The saucer: teal above, glowing below, a chrome rim.
            let hub = DVec3::new(0.0, 6.86, 1.75);
            lathe(
                &mut b,
                hub,
                DVec3::Y,
                &[
                    (0.0, -0.06),
                    (0.62, -0.06),
                    (0.8, 0.0),
                    (0.82, 0.03),
                    (0.78, 0.06),
                    (0.5, 0.14),
                    (0.2, 0.2),
                    (0.0, 0.22),
                ],
                sides(lod, 32),
                |k| match k {
                    0 => glow(rgb(225, 245, 255)),
                    1..=3 => metal(chrome),
                    _ => plain(accent),
                },
            );
            // An atom on top: a glowing nucleus and two orbits.
            let atom = hub + DVec3::Y * 0.5;
            let ball: Vec<(f64, f64)> = (0..=8)
                .map(|k| {
                    let a = PI * k as f64 / 8.0;
                    (0.1 * a.sin(), -0.1 * a.cos())
                })
                .collect();
            lathe(&mut b, atom, DVec3::Y, &ball, sides(lod, 14), |_| {
                glow(rgb(255, 120, 90))
            });
            tube(
                &mut b,
                &[hub + DVec3::Y * 0.2, atom - DVec3::Y * 0.1],
                (0.025, 0.025),
                6,
                metal(chrome),
            );
            if lod != Lod::Far {
                for tilt in [0.6f64, -0.6] {
                    let ring: Vec<DVec3> = (0..=24)
                        .map(|k| {
                            let a = TAU * k as f64 / 24.0;
                            let p = DVec3::new(0.26 * a.cos(), 0.0, 0.12 * a.sin());
                            let (s, c) = tilt.sin_cos();
                            atom + DVec3::new(p.x * c, p.x * s, p.z)
                        })
                        .collect();
                    tube(&mut b, &ring, (0.012, 0.012), 6, metal(chrome));
                }
            }
        }
    }
    b.finish()
}

/// The traffic light's lamps (red, amber, green), in model space: height,
/// the lens's front (z) and its radius. The model's lenses are unlit dark
/// glass; whoever shows the signal draws a lit lens over them here.
pub const TRAFFIC_LAMPS: [(f64, f64, f64); 3] =
    [(3.62, 0.19, 0.1), (3.35, 0.19, 0.1), (3.08, 0.19, 0.1)];

/// Traffic lights facing +z: a turned pole, a rounded yellow housing, three
/// unlit lenses (see [`TRAFFIC_LAMPS`]) under hooded visors.
pub fn traffic_light(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let pole = rgb(60, 62, 66);
    let housing = rgb(214, 172, 44);
    let seg = sides(lod, 20);
    lathe(
        &mut b,
        DVec3::ZERO,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.17, 0.0),
            (0.17, 0.08),
            (0.11, 0.2),
            (0.08, 0.4),
            (0.07, 2.8),
            (0.0, 2.82),
        ],
        seg,
        |_| metal(pole),
    );
    // The housing: a rounded capsule standing up.
    tube(
        &mut b,
        &[DVec3::new(0.0, 2.88, 0.0), DVec3::new(0.0, 3.82, 0.0)],
        (0.2, 0.15),
        sides(lod, 20),
        plain(housing),
    );
    lathe(
        &mut b,
        DVec3::Y * 3.95,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.06, 0.0),
            (0.05, 0.1),
            (0.07, 0.14),
            (0.0, 0.2),
        ],
        sides(lod, 12),
        |_| metal(pole),
    );
    let tints = [rgb(90, 20, 18), rgb(96, 66, 18), rgb(18, 80, 40)];
    for (&(y, front, _), c) in TRAFFIC_LAMPS.iter().zip(tints) {
        let at = DVec3::new(0.0, y, front - 0.04);
        lathe(
            &mut b,
            at,
            DVec3::Z,
            &[
                (0.0, -0.05),
                (0.11, -0.05),
                (0.11, 0.0),
                (0.11, 0.015),
                (0.095, 0.02),
                (0.06, 0.035),
                (0.0, 0.04),
            ],
            sides(lod, 18),
            |k| {
                if k < 3 {
                    plain(rgb(24, 24, 26))
                } else {
                    Some((Surf::Glass, c))
                }
            },
        );
        if lod != Lod::Far {
            // A visor: the top half of a short tube.
            let half: Vec<DVec3> = (0..=8)
                .map(|k| {
                    let a = PI * k as f64 / 8.0;
                    at + DVec3::new(0.125 * a.cos(), 0.125 * a.sin(), 0.0)
                })
                .collect();
            let rows: Vec<Vec<DVec3>> = [0.0, 0.14]
                .iter()
                .map(|&dz| half.iter().map(|p| *p + DVec3::Z * dz).collect())
                .collect();
            let mut inner: Vec<Vec<DVec3>> = rows.clone();
            for r in &mut inner {
                for p in r.iter_mut() {
                    *p = at + (*p - at) * DVec3::new(0.9, 0.9, 1.0);
                }
            }
            b.grid(&rows, false, false, |_, _| plain(housing));
            b.grid(&inner, false, true, |_, _| plain(rgb(30, 30, 32)));
        }
    }
    b.finish()
}

/// A deco bench, its sitters looking along +z: cast-iron scrolled ends,
/// curved slatted seat and back, bolts.
pub fn bench(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let iron = rgb(40, 46, 42);
    let wood = rgb(150, 98, 56);
    let r = lod.pick(0.024, 0.024, 0.03);
    let per = lod.pick(5, 3, 1);
    for x in [-0.82, 0.82] {
        let p = |y: f64, z: f64| DVec3::new(x, y, z);
        // Front leg curling into the arm rest, back leg up into the back.
        let front = spline(
            &[
                p(0.0, 0.28),
                p(0.25, 0.24),
                p(0.44, 0.2),
                p(0.62, 0.22),
                p(0.66, 0.05),
                p(0.6, -0.05),
            ],
            per,
            false,
        );
        let back = spline(
            &[
                p(0.0, -0.3),
                p(0.3, -0.24),
                p(0.44, -0.22),
                p(0.7, -0.3),
                p(0.95, -0.36),
            ],
            per,
            false,
        );
        let seat = spline(&[p(0.43, 0.24), p(0.44, 0.0), p(0.42, -0.23)], per, false);
        for path in [&front, &back, &seat] {
            tube(&mut b, path, (r, r * 1.6), sides(lod, 10), metal(iron));
        }
        if lod == Lod::Near {
            let scroll = spline(
                &[p(0.6, -0.05), p(0.55, 0.04), p(0.52, -0.02), p(0.56, -0.03)],
                4,
                false,
            );
            tube(&mut b, &scroll, (0.012, 0.016), 6, metal(iron));
        }
        // Feet.
        for z in [0.28, -0.3] {
            lathe(
                &mut b,
                p(0.0, z),
                DVec3::Y,
                &[(0.0, 0.0), (0.05, 0.0), (0.04, 0.03), (0.0, 0.04)],
                sides(lod, 10),
                |_| metal(iron),
            );
        }
    }
    // Slats: four across the seat, three up the back, following the curve.
    let w = 0.97;
    let seat = [(0.46, 0.2), (0.47, 0.08), (0.465, -0.04), (0.455, -0.16)];
    let back = [(0.58, -0.25), (0.72, -0.3), (0.86, -0.34)];
    let slats: Vec<(f64, f64, f64)> = seat
        .iter()
        .map(|&(y, z)| (y, z, 0.0))
        .chain(back.iter().map(|&(y, z)| (y, z, 1.2)))
        .collect();
    for (y, z, tilt) in slats {
        let up = DVec3::new(0.0, tilt.cos(), -tilt.sin() * 0.5).normalize();
        let along = DVec3::X;
        slab(
            &mut b,
            DVec3::new(0.0, y, z),
            [along, up.cross(along).normalize()],
            DVec3::new(w, 0.05, 0.02),
            wood,
        );
    }
    b.finish()
}

/// A litter bin: a ribbed green barrel with a chrome rim and a domed lid.
pub fn bin(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let green = rgb(46, 94, 72);
    let chrome = rgb(200, 204, 206);
    let mut prof = vec![(0.0, 0.0), (0.24, 0.0), (0.26, 0.04)];
    if lod == Lod::Near {
        for k in 0..4 {
            let y = 0.18 + 0.18 * k as f64;
            prof.extend([(0.27, y - 0.02), (0.285, y), (0.275, y + 0.02)]);
        }
    }
    prof.extend([
        (0.29, 0.8),
        (0.31, 0.82),
        (0.31, 0.86),
        (0.28, 0.88),
        (0.2, 0.93),
        (0.0, 0.95),
    ]);
    let n = prof.len();
    lathe(&mut b, DVec3::ZERO, DVec3::Y, &prof, sides(lod, 24), |k| {
        if k >= n - 5 && k < n - 3 {
            metal(chrome)
        } else {
            plain(green)
        }
    });
    if lod != Lod::Far {
        // The flap.
        slab(
            &mut b,
            DVec3::new(0.0, 0.84, 0.3),
            [DVec3::X, DVec3::Y],
            DVec3::new(0.12, 0.05, 0.012),
            rgb(20, 22, 22),
        );
    }
    b.finish()
}

/// A red hydrant with a chrome bonnet and nozzle caps.
pub fn hydrant(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let red = rgb(198, 38, 34);
    let chrome = rgb(220, 224, 228);
    let seg = sides(lod, 20);
    lathe(
        &mut b,
        DVec3::ZERO,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.17, 0.0),
            (0.17, 0.05),
            (0.13, 0.08),
            (0.12, 0.12),
            (0.11, 0.45),
            (0.14, 0.47),
            (0.14, 0.51),
            (0.12, 0.53),
            (0.1, 0.6),
            (0.05, 0.65),
            (0.04, 0.7),
            (0.0, 0.71),
        ],
        seg,
        |k| if k >= 8 { metal(chrome) } else { plain(red) },
    );
    for dir in [DVec3::X, -DVec3::X, DVec3::Z] {
        let big = dir == DVec3::Z;
        let r = if big { 0.065 } else { 0.045 };
        lathe(
            &mut b,
            DVec3::new(0.0, 0.36, 0.0),
            dir,
            &[
                (0.0, 0.0),
                (r, 0.0),
                (r, 0.15),
                (r * 1.25, 0.16),
                (r * 1.25, 0.2),
                (r * 0.8, 0.21),
                (0.0, 0.21),
            ],
            sides(lod, 14),
            |k| if k >= 2 { metal(chrome) } else { plain(red) },
        );
    }
    b.finish()
}

/// A mailbox shaped like a rocket: three swept fins, a red nose cone, a
/// porthole and a chrome mouth.
pub fn mailbox(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let blue = rgb(40, 82, 168);
    let red = rgb(204, 46, 40);
    let chrome = rgb(214, 218, 222);
    let mut prof: Vec<(f64, f64)> = vec![(0.0, 0.18), (0.14, 0.18), (0.2, 0.25), (0.235, 0.4)];
    prof.extend([(0.245, 0.8), (0.24, 1.1), (0.23, 1.22)]);
    // The nose: an ogive.
    for k in 1..=8 {
        let t = k as f64 / 8.0;
        prof.push((0.23 * (1.0 - t * t).sqrt().max(0.0), 1.22 + 0.55 * t));
    }
    lathe(&mut b, DVec3::ZERO, DVec3::Y, &prof, sides(lod, 28), |k| {
        if k >= 6 {
            plain(red)
        } else if k == 5 {
            metal(chrome)
        } else {
            plain(blue)
        }
    });
    // Fins down to the ground.
    for k in 0..3 {
        let a = TAU * (k as f64 / 3.0 + 1.0 / 12.0);
        let u = DVec3::new(a.cos(), 0.0, a.sin());
        let p = |r: f64, y: f64| u * r + DVec3::Y * y;
        let edge = spline(
            &[p(0.2, 0.7), p(0.36, 0.35), p(0.42, 0.0)],
            lod.pick(5, 3, 1),
            false,
        );
        // A flat blade from the body out to the edge, the edge rounded
        // by a tube.
        let side = DVec3::Y.cross(u).normalize() * 0.016;
        let inner: Vec<DVec3> = edge.iter().map(|e| p(0.16, e.y.max(0.02) + 0.12)).collect();
        for k in 0..edge.len() - 1 {
            for sg in [-1.0, 1.0] {
                let o = side * sg;
                b.quad(
                    [inner[k] + o, edge[k] + o, edge[k + 1] + o, inner[k + 1] + o],
                    o,
                    red,
                );
            }
        }
        tube(&mut b, &edge, (0.02, 0.02), sides(lod, 8), plain(red));
    }
    // The porthole and the letter slot.
    lathe(
        &mut b,
        DVec3::new(0.0, 0.95, 0.235),
        DVec3::Z,
        &[
            (0.0, 0.0),
            (0.075, 0.0),
            (0.075, 0.02),
            (0.06, 0.025),
            (0.04, 0.035),
            (0.0, 0.04),
        ],
        sides(lod, 18),
        |k| {
            if k < 3 {
                metal(chrome)
            } else {
                glow(rgb(255, 210, 120))
            }
        },
    );
    tube(
        &mut b,
        &[DVec3::new(-0.1, 0.62, 0.24), DVec3::new(0.1, 0.62, 0.24)],
        (0.012, 0.02),
        8,
        metal(chrome),
    );
    b.finish()
}

/// A cast-iron bollard with a ball top.
pub fn bollard(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let iron = rgb(30, 34, 36);
    let mut prof = vec![
        (0.0, 0.0),
        (0.2, 0.0),
        (0.2, 0.05),
        (0.16, 0.09),
        (0.15, 0.3),
        (0.13, 0.48),
        (0.17, 0.52),
        (0.17, 0.56),
        (0.11, 0.6),
    ];
    for k in 0..=6 {
        let a = PI * k as f64 / 6.0 - PI / 2.0;
        prof.push((0.09 * a.cos().max(0.0), 0.66 + 0.09 * a.sin()));
    }
    lathe(&mut b, DVec3::ZERO, DVec3::Y, &prof, sides(lod, 20), |_| {
        metal(iron)
    });
    b.finish()
}

/// A round news kiosk: a red plinth, cream walls with a curved glass
/// counter window towards +z, a glowing sign band, a flared red saucer
/// roof and an atom on a mast.
pub fn kiosk(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let cream = rgb(236, 226, 200);
    let red = rgb(196, 54, 44);
    let chrome = rgb(210, 214, 220);
    let seg = sides(lod, 48);
    lathe(
        &mut b,
        DVec3::ZERO,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (1.28, 0.0),
            (1.28, 0.42),
            (1.24, 0.5),
            (1.2, 0.52),
            (1.2, 2.18),
            (1.25, 2.2),
            (1.25, 2.64),
            (1.22, 2.66),
            (1.95, 2.86),
            (1.98, 2.92),
            (1.9, 2.96),
            (1.2, 3.25),
            (0.5, 3.55),
            (0.1, 3.62),
            (0.0, 3.63),
        ],
        seg,
        |k| match k {
            0..=2 => plain(red),
            3 | 4 => metal(chrome),
            5 => plain(cream),
            6 | 7 => glow(rgb(255, 210, 120)),
            9 | 10 => metal(chrome),
            _ => plain(red),
        },
    );
    // The counter window: a curved pane set into the wall.
    let n = lod.pick(16, 8, 4);
    let rows: Vec<Vec<DVec3>> = [1.0, 1.95]
        .iter()
        .map(|&y| {
            (0..=n)
                .map(|k| {
                    let a = PI * 0.5 + (k as f64 / n as f64 - 0.5) * 1.3;
                    DVec3::new(1.212 * a.cos(), y, 1.212 * a.sin())
                })
                .collect()
        })
        .collect();
    crate::carmesh::surface(&mut b, &rows, false, |_, _| {
        Some((Surf::Glass, rgb(50, 70, 90)))
    });
    if lod != Lod::Far {
        // The counter's chrome sill.
        let sill: Vec<DVec3> = (0..=n)
            .map(|k| {
                let a = PI * 0.5 + (k as f64 / n as f64 - 0.5) * 1.36;
                DVec3::new(1.26 * a.cos(), 0.98, 1.26 * a.sin())
            })
            .collect();
        tube(&mut b, &sill, (0.06, 0.02), 8, metal(chrome));
    }
    tube(
        &mut b,
        &[DVec3::Y * 3.6, DVec3::Y * 4.3],
        (0.035, 0.035),
        sides(lod, 10),
        metal(chrome),
    );
    let ball: Vec<(f64, f64)> = (0..=8)
        .map(|k| {
            let a = PI * k as f64 / 8.0;
            (0.14 * a.sin(), -0.14 * a.cos())
        })
        .collect();
    lathe(
        &mut b,
        DVec3::Y * 4.42,
        DVec3::Y,
        &ball,
        sides(lod, 16),
        |_| glow(rgb(140, 230, 255)),
    );
    if lod != Lod::Far {
        for tilt in [0.0f64, 1.05, -1.05] {
            let ring: Vec<DVec3> = (0..=24)
                .map(|k| {
                    let a = TAU * k as f64 / 24.0;
                    let (s, c) = tilt.sin_cos();
                    let p = DVec3::new(0.3 * a.cos(), 0.0, 0.12 * a.sin());
                    DVec3::Y * 4.42 + DVec3::new(p.x * c - p.z * s * 0.0, p.x * s, p.z)
                })
                .collect();
            tube(&mut b, &ring, (0.012, 0.012), 6, metal(chrome));
        }
    }
    b.finish()
}

/// A round advertising pillar in green iron: a moulded foot, posters all
/// round the drum, a cornice and a domed cap with a gilt finial.
pub fn ad_pillar(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let green = rgb(40, 82, 60);
    let gold = rgb(214, 170, 72);
    let posters = [
        rgb(232, 196, 90),
        rgb(210, 70, 60),
        rgb(90, 150, 200),
        rgb(240, 236, 220),
        rgb(220, 120, 170),
        rgb(120, 190, 120),
    ];
    let seg = sides(lod, 36);
    lathe(
        &mut b,
        DVec3::ZERO,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.68, 0.0),
            (0.68, 0.12),
            (0.62, 0.18),
            (0.6, 0.3),
            (0.56, 0.32),
        ],
        seg,
        |_| plain(green),
    );
    // The posters: panels round the drum, each its own colour.
    let panels = 12;
    let per = (seg / panels).max(1);
    let n = panels * per;
    let rows: Vec<Vec<DVec3>> = [0.32, 2.48]
        .iter()
        .map(|&y| {
            (0..n)
                .map(|k| {
                    let a = TAU * k as f64 / n as f64;
                    DVec3::new(0.555 * a.cos(), y, 0.555 * a.sin())
                })
                .collect()
        })
        .collect();
    crate::carmesh::surface(&mut b, &rows, true, |_, j| {
        let p = j / per;
        plain(posters[(p * 5 + p / 3) % posters.len()])
    });
    let dome: Vec<(f64, f64)> = std::iter::once((0.56, 2.48))
        .chain([(0.64, 2.52), (0.7, 2.6), (0.7, 2.66), (0.64, 2.7)])
        .chain((1..=8).map(|k| {
            let a = PI * 0.5 * k as f64 / 8.0;
            (0.64 * a.cos(), 2.7 + 0.45 * a.sin())
        }))
        .collect();
    lathe(&mut b, DVec3::ZERO, DVec3::Y, &dome, seg, |_| plain(green));
    lathe(
        &mut b,
        DVec3::Y * 3.13,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.06, 0.0),
            (0.05, 0.12),
            (0.08, 0.2),
            (0.03, 0.3),
            (0.0, 0.42),
        ],
        sides(lod, 12),
        |_| metal(gold),
    );
    b.finish()
}

/// A round stone fountain: a moulded basin with water, a column, a bowl,
/// and a glowing atom with orbiting electrons on top.
pub fn fountain(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let stone = rgb(208, 200, 184);
    let seg = sides(lod, 64);
    lathe(
        &mut b,
        DVec3::ZERO,
        DVec3::Y,
        &[
            (2.95, 0.6),
            (2.95, 0.15),
            (3.2, 0.15),
            (3.25, 0.0),
            (3.3, 0.0),
            (3.3, 0.5),
            (3.36, 0.56),
            (3.3, 0.66),
            (3.0, 0.66),
            (2.95, 0.6),
        ],
        seg,
        |_| plain(stone),
    );
    lathe(
        &mut b,
        DVec3::Y * 0.55,
        DVec3::Y,
        &[(0.0, 0.0), (2.96, 0.0), (0.0, 0.0001)],
        seg,
        |_| Some((Surf::Glass, rgb(70, 130, 160))),
    );
    lathe(
        &mut b,
        DVec3::ZERO,
        DVec3::Y,
        &[
            (0.0, 0.5),
            (0.42, 0.5),
            (0.42, 0.7),
            (0.32, 0.78),
            (0.26, 1.2),
            (0.24, 1.6),
            (0.3, 1.72),
            (0.6, 1.8),
            (1.0, 1.95),
            (1.15, 2.1),
            (1.12, 2.18),
            (1.0, 2.14),
            (0.3, 2.05),
            (0.0, 2.05),
        ],
        sides(lod, 40),
        |_| plain(stone),
    );
    // Water in the bowl.
    lathe(
        &mut b,
        DVec3::Y * 2.1,
        DVec3::Y,
        &[(0.0, 0.0), (1.05, 0.0), (0.0, 0.0001)],
        sides(lod, 40),
        |_| Some((Surf::Glass, rgb(70, 130, 160))),
    );
    let ball = |r: f64| -> Vec<(f64, f64)> {
        (0..=10)
            .map(|k| {
                let a = PI * k as f64 / 10.0;
                (r * a.sin(), -r * a.cos())
            })
            .collect()
    };
    let atom = DVec3::Y * 2.55;
    // A fluted stem lifts the atom out of the bowl.
    lathe(
        &mut b,
        DVec3::Y * 2.0,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.12, 0.0),
            (0.07, 0.15),
            (0.06, 0.32),
            (0.0, 0.36),
        ],
        sides(lod, 12),
        |_| metal(rgb(214, 170, 72)),
    );
    lathe(&mut b, atom, DVec3::Y, &ball(0.25), sides(lod, 20), |_| {
        glow(rgb(140, 230, 255))
    });
    for k in 0..3 {
        let a = TAU * k as f64 / 3.0;
        let r = DVec3::new(0.55 * a.cos(), 0.12 * (a * 2.0).sin(), 0.55 * a.sin());
        lathe(
            &mut b,
            atom + r,
            DVec3::Y,
            &ball(0.07),
            sides(lod, 12),
            |_| glow(rgb(255, 160, 220)),
        );
    }
    if lod != Lod::Far {
        let ring: Vec<DVec3> = (0..=40)
            .map(|k| {
                let a = TAU * k as f64 / 40.0;
                atom + DVec3::new(0.55 * a.cos(), 0.12 * (a * 2.0).sin(), 0.55 * a.sin())
            })
            .collect();
        tube(&mut b, &ring, (0.012, 0.012), 6, metal(rgb(210, 214, 220)));
    }
    b.finish()
}

/// A barricade piece (see [`BarricadePiece::block`] for its size), long
/// along x: a concrete jersey barrier with a painted band, a striped
/// sawhorse, a wall of plump sandbags, or a ribbed oil drum with a fire.
pub fn barricade(piece: BarricadePiece, lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let (hx, hz, h) = piece.block();
    match piece {
        BarricadePiece::Jersey => {
            let concrete = rgb(158, 156, 148);
            // The section (z, y), with tight pairs at the edges so they
            // stay crisp.
            let e = 0.012;
            let half = [
                (hz, 0.0),
                (hz, 0.08 - e),
                (hz - e * 0.3, 0.08),
                (hz * 0.88, 0.28),
                (hz * 0.4, h - 0.06),
                (hz * 0.36, h - e),
                (hz * 0.32, h),
            ];
            let mut sec: Vec<(f64, f64)> = half.to_vec();
            sec.extend(half.iter().rev().map(|&(z, y)| (-z, y)));
            // Rows along x; the band's stripes need rows of their own.
            let stripes = 5;
            let mut xs: Vec<f64> = (0..=stripes)
                .map(|k| -hx + 0.03 + (2.0 * hx - 0.06) * k as f64 / stripes as f64)
                .collect();
            if lod != Lod::Far {
                let extra: Vec<f64> = (0..stripes).map(|k| xs[k] + 0.0005).collect();
                xs.extend(extra);
            }
            xs.sort_by(f64::total_cmp);
            let mut rows: Vec<Vec<DVec3>> = Vec::new();
            let ring = |x: f64, inset: f64| -> Vec<DVec3> {
                let cy = h * 0.4;
                sec.iter()
                    .map(|&(z, y)| {
                        let p = DVec3::new(x, y, z);
                        let c = DVec3::new(x, cy, 0.0);
                        let d = p - c;
                        c + DVec3::new(0.0, d.y, d.z) * (1.0 - inset)
                            + DVec3::Y * if y == 0.0 { -cy * 0.0 } else { 0.0 }
                    })
                    .collect()
            };
            rows.push(ring(-hx, 1.0));
            rows.push(ring(-hx + 0.005, 0.04));
            for &x in &xs {
                rows.push(ring(x, 0.0));
            }
            rows.push(ring(hx - 0.005, 0.04));
            rows.push(ring(hx, 1.0));
            let nx = rows.len();
            crate::carmesh::surface(&mut b, &rows, true, |i, j| {
                let y = (sec[j].1 + sec[(j + 1) % sec.len()].1) * 0.5;
                if lod != Lod::Far && i >= 2 && i + 2 < nx && y > h - 0.25 && y < h - 0.02 {
                    let x = (rows[i][0].x + rows[i + 1][0].x) * 0.5;
                    let k = ((x + hx) / (2.0 * hx) * stripes as f64).floor() as i64;
                    let c = if k % 2 == 0 {
                        rgb(200, 40, 40)
                    } else {
                        rgb(236, 234, 226)
                    };
                    return plain(c);
                }
                plain(concrete)
            });
        }
        BarricadePiece::Sawhorse => {
            let wood = rgb(120, 92, 60);
            for x in [-hx + 0.15, hx - 0.15] {
                for dz in [-1.0, 1.0] {
                    let foot = DVec3::new(x, 0.0, dz * hz);
                    let top = DVec3::new(x, h - 0.2, 0.0);
                    tube(
                        &mut b,
                        &[foot, top],
                        (0.035, 0.035),
                        sides(lod, 8),
                        plain(wood),
                    );
                }
            }
            // The plank, striped yellow and black, on a crossbar.
            tube(
                &mut b,
                &[
                    DVec3::new(-hx + 0.1, h - 0.22, 0.0),
                    DVec3::new(hx - 0.1, h - 0.22, 0.0),
                ],
                (0.04, 0.04),
                sides(lod, 8),
                plain(wood),
            );
            let n = 6;
            let w = hx * 2.0 / n as f64;
            for k in 0..n {
                let x = -hx + w * (k as f64 + 0.5);
                let c = if k % 2 == 0 {
                    rgb(235, 190, 40)
                } else {
                    rgb(30, 30, 30)
                };
                slab(
                    &mut b,
                    DVec3::new(x, h - 0.1, 0.05),
                    [DVec3::X, DVec3::Y],
                    DVec3::new(w * 0.5, 0.1, 0.02),
                    c,
                );
            }
            if lod != Lod::Far {
                // A warning lamp on top.
                lathe(
                    &mut b,
                    DVec3::new(-hx + 0.25, h, 0.05),
                    DVec3::Y,
                    &[
                        (0.0, 0.0),
                        (0.07, 0.0),
                        (0.07, 0.05),
                        (0.06, 0.12),
                        (0.0, 0.16),
                    ],
                    sides(lod, 12),
                    |k| {
                        if k < 2 {
                            plain(rgb(30, 30, 30))
                        } else {
                            glow(rgb(255, 160, 40))
                        }
                    },
                );
            }
        }
        BarricadePiece::Sandbags => {
            let sand = [rgb(170, 150, 110), rgb(156, 138, 100)];
            for layer in 0..3 {
                let shift = if layer % 2 == 0 { 0.0 } else { 0.2 };
                let mut x = -hx + 0.2 + shift;
                let mut k = 0;
                while x + 0.2 <= hx + 1e-6 {
                    let c = DVec3::new(x, 0.1 + layer as f64 * 0.17, 0.0);
                    pillow(
                        &mut b,
                        c,
                        DVec3::new(0.21, 0.1, hz * 0.92),
                        lod,
                        sand[(k + layer) % 2],
                    );
                    x += 0.4;
                    k += 1;
                }
            }
        }
        BarricadePiece::Drum => {
            let rust = rgb(110, 50, 30);
            let r = hx;
            let mut prof = vec![(0.0, 0.0), (r * 0.97, 0.0), (r, 0.03)];
            for y in [0.28, 0.58] {
                prof.extend([
                    (r, y - 0.03),
                    (r + 0.018, y - 0.01),
                    (r + 0.018, y + 0.01),
                    (r, y + 0.03),
                ]);
            }
            prof.extend([
                (r, h - 0.06),
                (r + 0.01, h - 0.04),
                (r * 0.94, h - 0.04),
                (r * 0.94, h - 0.1),
                (0.0, h - 0.1),
            ]);
            lathe(&mut b, DVec3::ZERO, DVec3::Y, &prof, sides(lod, 24), |_| {
                plain(rust)
            });
            // Flames licking out of the top.
            for (k, (dx, dz, fh)) in [
                (0.0, 0.0, 0.5),
                (0.1, 0.06, 0.32),
                (-0.09, -0.05, 0.36),
                (0.05, -0.1, 0.28),
            ]
            .into_iter()
            .enumerate()
            {
                if lod == Lod::Far && k > 0 {
                    break;
                }
                lathe(
                    &mut b,
                    DVec3::new(dx, h - 0.1, dz),
                    DVec3::Y,
                    &[
                        (0.0, 0.0),
                        (0.13, 0.0),
                        (0.12, fh * 0.3),
                        (0.06, fh * 0.7),
                        (0.0, fh),
                    ],
                    sides(lod, 10),
                    |_| glow(rgb(255, 140, 40)),
                );
            }
        }
    }
    b.finish()
}

/// A pillow: a rounded box (superellipsoid), for sandbags.
fn pillow(b: &mut MeshBuilder, c: DVec3, half: DVec3, lod: Lod, col: Color) {
    let (seg, rings) = (lod.pick(16, 8, 4), lod.pick(8, 4, 2));
    let f = |v: f64, e: f64| v.signum() * v.abs().powf(e);
    let rows: Vec<Vec<DVec3>> = (0..=rings)
        .map(|k| {
            let t = PI * (k as f64 / rings as f64 - 0.5);
            (0..seg)
                .map(|i| {
                    let a = TAU * i as f64 / seg as f64;
                    let (ct, st) = (t.cos(), t.sin());
                    c + DVec3::new(
                        half.x * f(ct, 0.4) * f(a.cos(), 0.4),
                        half.y * f(st, 0.7),
                        half.z * f(ct, 0.4) * f(a.sin(), 0.7),
                    )
                })
                .collect()
        })
        .collect();
    crate::carmesh::surface(b, &rows, true, |_, _| plain(col));
}

/// A round phone booth (door towards +z): a red plinth, cream panels to
/// waist height, glass all round between chrome ribs, a glowing band and
/// a flared red dome with an aerial. Inside, the telephone on its post.
pub fn phone_booth(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let red = rgb(198, 40, 36);
    let cream = rgb(240, 230, 206);
    let chrome = rgb(214, 218, 224);
    let seg = sides(lod, 40);
    let r = 0.54;
    lathe(
        &mut b,
        DVec3::ZERO,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.6, 0.0),
            (0.6, 0.12),
            (0.57, 0.15),
            (r, 0.17),
            (r, 0.9),
            (r + 0.02, 0.92),
            (r + 0.02, 0.97),
            (r, 0.99),
            (r, 2.12),
            (r + 0.02, 2.14),
            (r + 0.02, 2.2),
            (r + 0.03, 2.22),
            (r + 0.03, 2.4),
            (0.62, 2.44),
            (0.64, 2.48),
            (0.5, 2.6),
            (0.28, 2.7),
            (0.08, 2.74),
            (0.0, 2.745),
        ],
        seg,
        |k| match k {
            0..=3 => plain(red),
            4 => plain(cream),
            5..=7 => metal(chrome),
            8 => Some((Surf::Glass, rgb(70, 96, 112))),
            9..=11 => metal(chrome),
            12 => glow(rgb(255, 214, 120)),
            _ => plain(red),
        },
    );
    if lod != Lod::Far {
        // Ribs between the panes, and the door's edge and handle.
        let ribs = lod.pick(8, 6, 0);
        for k in 0..ribs {
            let a = TAU * k as f64 / ribs as f64 + PI / 2.0 + PI / ribs as f64;
            let u = DVec3::new(a.cos(), 0.0, a.sin());
            tube(
                &mut b,
                &[
                    u * (r + 0.01) + DVec3::Y * 0.99,
                    u * (r + 0.01) + DVec3::Y * 2.12,
                ],
                (0.022, 0.022),
                sides(lod, 8),
                metal(chrome),
            );
        }
        tube(
            &mut b,
            &[
                DVec3::new(0.2, 1.05, r + 0.06),
                DVec3::new(0.2, 1.45, r + 0.06),
            ],
            (0.016, 0.016),
            8,
            metal(chrome),
        );
        // The telephone on the back wall.
        slab(
            &mut b,
            DVec3::new(0.0, 1.35, -r + 0.12),
            [DVec3::X, DVec3::Y],
            DVec3::new(0.12, 0.17, 0.07),
            rgb(30, 30, 34),
        );
        tube(
            &mut b,
            &spline(
                &[
                    DVec3::new(-0.08, 1.46, -r + 0.2),
                    DVec3::new(0.0, 1.5, -r + 0.22),
                    DVec3::new(0.08, 1.46, -r + 0.2),
                ],
                3,
                false,
            ),
            (0.025, 0.025),
            8,
            metal(rgb(24, 24, 26)),
        );
    }
    // The aerial with a glowing ball.
    tube(
        &mut b,
        &[DVec3::Y * 2.72, DVec3::Y * 3.05],
        (0.015, 0.015),
        sides(lod, 8),
        metal(chrome),
    );
    lathe(
        &mut b,
        DVec3::Y * 3.05,
        DVec3::Y,
        &(0..=6)
            .map(|k| {
                let a = PI * k as f64 / 6.0;
                (0.06 * a.sin(), 0.06 - 0.06 * a.cos())
            })
            .collect::<Vec<_>>(),
        sides(lod, 12),
        |_| glow(rgb(255, 120, 90)),
    );
    b.finish()
}

/// A pneumatic post column: a brass drum with a glowing hatch to the
/// front (+z), a glass tube with a capsule in it rising between three
/// brass struts, and a domed head with a cyan ring and a gauge.
pub fn post_tube(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let brass = rgb(196, 152, 76);
    let green = rgb(38, 86, 78);
    let chrome = rgb(214, 218, 222);
    let cyan = rgb(110, 230, 255);
    let seg = sides(lod, 32);
    lathe(
        &mut b,
        DVec3::ZERO,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.34, 0.0),
            (0.34, 0.08),
            (0.3, 0.12),
            (0.3, 0.86),
            (0.33, 0.9),
            (0.33, 0.96),
            (0.2, 1.02),
            (0.14, 1.06),
            (0.0, 1.07),
        ],
        seg,
        |k| match k {
            0..=2 => metal(brass),
            3 => plain(green),
            _ => metal(brass),
        },
    );
    // The glass tube and the capsule riding in it.
    lathe(
        &mut b,
        DVec3::Y * 1.06,
        DVec3::Y,
        &[(0.0, 0.0), (0.11, 0.0), (0.11, 1.24), (0.0, 1.24)],
        sides(lod, 20),
        |_| Some((Surf::Glass, rgb(150, 190, 200))),
    );
    if lod != Lod::Far {
        lathe(
            &mut b,
            DVec3::Y * 1.5,
            DVec3::Y,
            &[
                (0.0, 0.0),
                (0.05, 0.01),
                (0.075, 0.05),
                (0.075, 0.33),
                (0.05, 0.37),
                (0.0, 0.38),
            ],
            sides(lod, 14),
            |k| if k == 2 { metal(chrome) } else { metal(brass) },
        );
        for k in 0..3 {
            let a = TAU * k as f64 / 3.0 + PI / 2.0;
            let u = DVec3::new(a.cos(), 0.0, a.sin());
            tube(
                &mut b,
                &[u * 0.16 + DVec3::Y * 1.02, u * 0.16 + DVec3::Y * 2.32],
                (0.02, 0.02),
                sides(lod, 8),
                metal(brass),
            );
        }
        // Brass collars round the glass.
        for y in [1.45, 1.9] {
            lathe(
                &mut b,
                DVec3::Y * y,
                DVec3::Y,
                &[(0.112, 0.0), (0.18, 0.0), (0.18, 0.05), (0.112, 0.05)],
                sides(lod, 20),
                |_| metal(brass),
            );
        }
        // The hatch: a glowing slot in a brass frame, a handle under it.
        let z = 0.3;
        slab(
            &mut b,
            DVec3::new(0.0, 0.62, z + 0.01),
            [DVec3::X, DVec3::Y],
            DVec3::new(0.15, 0.1, 0.02),
            brass,
        );
        b.surf = Surf::Glow;
        b.quad(
            [
                DVec3::new(-0.11, 0.57, z + 0.032),
                DVec3::new(0.11, 0.57, z + 0.032),
                DVec3::new(0.11, 0.67, z + 0.032),
                DVec3::new(-0.11, 0.67, z + 0.032),
            ],
            DVec3::Z,
            cyan,
        );
        b.surf = Surf::Plain;
        tube(
            &mut b,
            &[
                DVec3::new(-0.07, 0.45, z + 0.04),
                DVec3::new(0.07, 0.45, z + 0.04),
            ],
            (0.014, 0.014),
            8,
            metal(chrome),
        );
    }
    // The head: a brass dome over a glowing ring, the gauge to the front.
    lathe(
        &mut b,
        DVec3::Y * 2.28,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.22, 0.0),
            (0.24, 0.04),
            (0.24, 0.12),
            (0.24, 0.18),
            (0.22, 0.24),
            (0.16, 0.32),
            (0.06, 0.36),
            (0.0, 0.37),
        ],
        seg,
        |k| if k == 3 { glow(cyan) } else { metal(brass) },
    );
    tube(
        &mut b,
        &[DVec3::Y * 2.64, DVec3::Y * 2.7],
        (0.03, 0.03),
        8,
        metal(brass),
    );
    if lod != Lod::Far {
        lathe(
            &mut b,
            DVec3::new(0.0, 2.48, 0.235),
            DVec3::Z,
            &[(0.0, 0.0), (0.07, 0.0), (0.07, 0.03), (0.0, 0.032)],
            sides(lod, 16),
            |k| {
                if k == 2 {
                    plain(rgb(244, 238, 220))
                } else {
                    metal(brass)
                }
            },
        );
    }
    b.finish()
}

/// A newspaper stand: a green wooden counter with sloping racks of papers
/// to the front (+z), corner posts and a striped roof with a glowing sign
/// along its edge.
pub fn news_stand(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let green = rgb(44, 96, 70);
    let cream = rgb(240, 232, 210);
    let red = rgb(196, 54, 44);
    let chrome = rgb(210, 214, 220);
    let papers = [
        rgb(238, 234, 222),
        rgb(232, 210, 120),
        rgb(220, 120, 100),
        rgb(150, 190, 220),
        rgb(240, 236, 228),
        rgb(200, 220, 160),
    ];
    // The counter: a box with a chrome edge.
    slab(
        &mut b,
        DVec3::new(0.0, 0.5, -0.12),
        [DVec3::X, DVec3::Y],
        DVec3::new(0.8, 0.5, 0.3),
        green,
    );
    slab(
        &mut b,
        DVec3::new(0.0, 1.02, -0.12),
        [DVec3::X, DVec3::Y],
        DVec3::new(0.83, 0.02, 0.33),
        chrome,
    );
    // Two sloping racks of papers in front.
    let per = lod.pick(6, 4, 1);
    for (row, (y, z)) in [(0.32, 0.26), (0.72, 0.22)].into_iter().enumerate() {
        let tilt = DVec3::new(0.0, 0.85, -0.5).normalize();
        slab(
            &mut b,
            DVec3::new(0.0, y, z),
            [DVec3::X, tilt],
            DVec3::new(0.78, 0.2, 0.015),
            cream,
        );
        if lod == Lod::Far {
            continue;
        }
        let w = 1.5 / per as f64;
        for k in 0..per {
            let x = -0.75 + w * (k as f64 + 0.5);
            let c = papers[(k * 3 + row * 2) % papers.len()];
            slab(
                &mut b,
                DVec3::new(x, y + 0.03, z + 0.035),
                [DVec3::X, tilt],
                DVec3::new(w * 0.42, 0.16, 0.01),
                c,
            );
            // The headline and a picture.
            let up = tilt;
            let face = DVec3::X.cross(tilt).normalize();
            let at = DVec3::new(x, y + 0.03, z + 0.035) + face * 0.011;
            slab(
                &mut b,
                at + up * 0.11,
                [DVec3::X, tilt],
                DVec3::new(w * 0.36, 0.025, 0.002),
                rgb(40, 38, 40),
            );
            slab(
                &mut b,
                at - up * 0.02 - DVec3::X * (w * 0.12),
                [DVec3::X, tilt],
                DVec3::new(w * 0.2, 0.07, 0.002),
                rgb(120, 116, 110),
            );
        }
    }
    // Corner posts up to the roof.
    for x in [-0.8, 0.8] {
        tube(
            &mut b,
            &[DVec3::new(x, 1.04, -0.38), DVec3::new(x, 2.2, -0.38)],
            (0.025, 0.025),
            sides(lod, 8),
            metal(chrome),
        );
    }
    // The striped roof, sloping down to the front.
    let stripes = lod.pick(8, 6, 2);
    for k in 0..stripes {
        let x0 = -0.85 + 1.7 * k as f64 / stripes as f64;
        let x1 = -0.85 + 1.7 * (k + 1) as f64 / stripes as f64;
        let c = if k % 2 == 0 { red } else { cream };
        let bottom = [
            DVec3::new(x0, 2.2, -0.45),
            DVec3::new(x1, 2.2, -0.45),
            DVec3::new(x1, 2.0, 0.45),
            DVec3::new(x0, 2.0, 0.45),
        ];
        let top = bottom.map(|p| p + DVec3::Y * 0.05);
        b.hexa(bottom, top, c);
    }
    // The sign along the front edge: a red board with the paper's name in
    // cream bars, a neon tube under it.
    slab(
        &mut b,
        DVec3::new(0.0, 1.9, 0.44),
        [DVec3::X, DVec3::Y],
        DVec3::new(0.8, 0.09, 0.015),
        red,
    );
    if lod != Lod::Far {
        for (x, w) in [(-0.5, 0.18), (-0.18, 0.1), (0.08, 0.12), (0.4, 0.22)] {
            slab(
                &mut b,
                DVec3::new(x, 1.9, 0.458),
                [DVec3::X, DVec3::Y],
                DVec3::new(w, 0.035, 0.004),
                cream,
            );
        }
    }
    tube(
        &mut b,
        &[DVec3::new(-0.78, 1.79, 0.44), DVec3::new(0.78, 1.79, 0.44)],
        (0.012, 0.012),
        6,
        glow(rgb(255, 214, 120)),
    );
    b.finish()
}

/// A bus stop facing the road (+z): a glass back wall in a chrome frame,
/// a bench, a turquoise boomerang roof on two raked posts with a light
/// strip under its edge, and a pylon with the round bus sign.
pub fn bus_stop(lod: Lod) -> Mesh {
    use hw_gen::urban::props::BUS_STOP;
    let mut b = MeshBuilder::new();
    let (hx, hz, under, top) = BUS_STOP;
    let chrome = rgb(214, 218, 224);
    let teal = rgb(64, 168, 160);
    let white = rgb(244, 242, 234);
    let red = rgb(204, 50, 44);
    let zb = -hz + 0.08;
    // The back wall: glass in a frame.
    b.surf = Surf::Glass;
    b.hexa(
        [
            DVec3::new(-hx + 0.15, 0.2, zb - 0.01),
            DVec3::new(hx - 0.15, 0.2, zb - 0.01),
            DVec3::new(hx - 0.15, 0.2, zb + 0.01),
            DVec3::new(-hx + 0.15, 0.2, zb + 0.01),
        ],
        [
            DVec3::new(-hx + 0.15, 2.2, zb - 0.01),
            DVec3::new(hx - 0.15, 2.2, zb - 0.01),
            DVec3::new(hx - 0.15, 2.2, zb + 0.01),
            DVec3::new(-hx + 0.15, 2.2, zb + 0.01),
        ],
        rgb(120, 160, 176),
    );
    b.surf = Surf::Plain;
    let frame = [
        DVec3::new(-hx + 0.12, 0.17, zb),
        DVec3::new(hx - 0.12, 0.17, zb),
        DVec3::new(hx - 0.12, 2.23, zb),
        DVec3::new(-hx + 0.12, 2.23, zb),
        DVec3::new(-hx + 0.12, 0.17, zb),
    ];
    tube(&mut b, &frame, (0.03, 0.03), sides(lod, 8), metal(chrome));
    // Two raked posts carrying the roof.
    for x in [-hx + 0.12, hx - 0.12] {
        tube(
            &mut b,
            &[
                DVec3::new(x, 0.0, zb),
                DVec3::new(x, under + 0.02, zb + 0.1),
            ],
            (0.045, 0.045),
            sides(lod, 12),
            metal(chrome),
        );
    }
    // The roof: rising to the front, a rounded edge, a light strip under it.
    let rise = 0.16;
    let th = top - under - rise;
    let y_at = |z: f64| under + rise * (z + hz) / (2.0 * hz);
    let bottom = [
        DVec3::new(-hx, y_at(-hz), -hz),
        DVec3::new(hx, y_at(-hz), -hz),
        DVec3::new(hx, y_at(hz - 0.06), hz - 0.06),
        DVec3::new(-hx, y_at(hz - 0.06), hz - 0.06),
    ];
    let lid = bottom.map(|p| p + DVec3::Y * th);
    b.hexa(bottom, lid, teal);
    tube(
        &mut b,
        &[
            DVec3::new(-hx, y_at(hz - 0.06) + th * 0.5, hz - 0.06),
            DVec3::new(hx, y_at(hz - 0.06) + th * 0.5, hz - 0.06),
        ],
        (th * 0.5 + 0.005, th * 0.5 + 0.005),
        sides(lod, 12),
        plain(white),
    );
    b.surf = Surf::Glow;
    b.quad(
        [
            DVec3::new(-hx + 0.2, y_at(hz - 0.3) - 0.012, hz - 0.36),
            DVec3::new(hx - 0.2, y_at(hz - 0.3) - 0.012, hz - 0.36),
            DVec3::new(hx - 0.2, y_at(hz - 0.2) - 0.012, hz - 0.22),
            DVec3::new(-hx + 0.2, y_at(hz - 0.2) - 0.012, hz - 0.22),
        ],
        -DVec3::Y,
        rgb(255, 236, 190),
    );
    b.surf = Surf::Plain;
    // The bench: slats on two legs, in front of the glass.
    for x in [-hx + 0.5, hx - 0.5] {
        tube(
            &mut b,
            &[
                DVec3::new(x, 0.0, zb + 0.28),
                DVec3::new(x, 0.44, zb + 0.28),
            ],
            (0.035, 0.035),
            sides(lod, 8),
            metal(chrome),
        );
    }
    for k in 0..3 {
        slab(
            &mut b,
            DVec3::new(0.0, 0.47, zb + 0.14 + 0.13 * k as f64),
            [DVec3::X, DVec3::Y],
            DVec3::new(hx - 0.3, 0.025, 0.05),
            rgb(196, 128, 64),
        );
    }
    // The pylon and its round sign.
    let px = -hx - 0.35;
    let pz = hz - 0.2;
    lathe(
        &mut b,
        DVec3::new(px, 0.0, pz),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.14, 0.0),
            (0.14, 0.08),
            (0.06, 0.14),
            (0.05, 2.6),
            (0.0, 2.61),
        ],
        sides(lod, 14),
        |_| metal(chrome),
    );
    let sign = DVec3::new(px, 2.85, pz);
    lathe(
        &mut b,
        sign - DVec3::X * 0.04,
        DVec3::X,
        &[
            (0.0, 0.0),
            (0.32, 0.0),
            (0.34, 0.02),
            (0.34, 0.06),
            (0.32, 0.08),
            (0.0, 0.08),
        ],
        sides(lod, 28),
        |k| match k {
            1..=3 => glow(rgb(255, 200, 120)),
            _ => plain(teal),
        },
    );
    if lod != Lod::Far {
        // A cream band across the disc both sides, and a little rocket on top.
        for s in [-1.0, 1.0] {
            slab(
                &mut b,
                sign + DVec3::X * (0.045 * s),
                [DVec3::Z, DVec3::Y],
                DVec3::new(0.27, 0.06, 0.006),
                white,
            );
        }
        lathe(
            &mut b,
            sign + DVec3::Y * 0.34,
            DVec3::Y,
            &[
                (0.0, 0.0),
                (0.05, 0.03),
                (0.05, 0.14),
                (0.03, 0.22),
                (0.0, 0.26),
            ],
            sides(lod, 12),
            |k| if k == 3 { plain(red) } else { metal(chrome) },
        );
    }
    b.finish()
}

/// A cafe table: a marble top on a chrome pedestal, three wire chairs
/// round it and a striped parasol. `variant` picks the colours.
pub fn cafe_table(variant: u8, lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let chrome = rgb(214, 218, 224);
    let marble = rgb(240, 236, 228);
    let (stripe, seat) = match variant % 4 {
        0 => (rgb(204, 54, 48), rgb(204, 54, 48)),
        1 => (rgb(52, 140, 170), rgb(240, 196, 70)),
        2 => (rgb(236, 170, 40), rgb(56, 120, 90)),
        _ => (rgb(60, 150, 100), rgb(232, 120, 150)),
    };
    let white = rgb(246, 244, 236);
    let seg = sides(lod, 28);
    lathe(
        &mut b,
        DVec3::ZERO,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.24, 0.0),
            (0.24, 0.03),
            (0.05, 0.08),
            (0.035, 0.7),
            (0.06, 0.72),
            (0.4, 0.72),
            (0.4, 0.76),
            (0.0, 0.76),
        ],
        seg,
        |k| if k >= 5 { plain(marble) } else { metal(chrome) },
    );
    // Chairs: a round seat on four legs and a hoop back, facing the table.
    for k in 0..3 {
        let a = TAU * (k as f64 / 3.0) + PI / 6.0;
        let out = DVec3::new(a.cos(), 0.0, a.sin());
        let side = DVec3::Y.cross(out).normalize();
        let c = out * 0.66;
        lathe(
            &mut b,
            c + DVec3::Y * 0.42,
            DVec3::Y,
            &[
                (0.0, 0.0),
                (0.2, 0.0),
                (0.21, 0.02),
                (0.2, 0.05),
                (0.0, 0.05),
            ],
            sides(lod, 16),
            |_| plain(seat),
        );
        if lod == Lod::Far {
            continue;
        }
        for (dx, dz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let foot = c + side * (0.18 * dx) + out * (0.18 * dz);
            let top = c + side * (0.13 * dx) + out * (0.13 * dz) + DVec3::Y * 0.42;
            tube(&mut b, &[foot, top], (0.012, 0.012), 6, metal(chrome));
        }
        let hoop: Vec<DVec3> = (0..=8)
            .map(|i| {
                let t = PI * i as f64 / 8.0;
                c + out * (0.16 + 0.04 * t.sin())
                    + side * (0.17 * t.cos())
                    + DVec3::Y * (0.47 + 0.36 * t.sin())
            })
            .collect();
        tube(&mut b, &hoop, (0.014, 0.014), 6, metal(chrome));
        let bars: Vec<DVec3> = vec![
            c + out * 0.16 + side * 0.17 + DVec3::Y * 0.47,
            c + out * 0.16 - side * 0.17 + DVec3::Y * 0.47,
        ];
        tube(&mut b, &bars, (0.012, 0.012), 6, metal(chrome));
    }
    // The parasol: a pole up through the table, striped canopy, valance.
    tube(
        &mut b,
        &[DVec3::Y * 0.76, DVec3::Y * 2.5],
        (0.022, 0.022),
        sides(lod, 8),
        metal(chrome),
    );
    let ribs = lod.pick(16, 12, 8);
    let rings = [
        (1.2, 2.06),
        (0.9, 2.2),
        (0.55, 2.33),
        (0.2, 2.43),
        (0.0, 2.46),
    ];
    let rows: Vec<Vec<DVec3>> = rings
        .iter()
        .map(|&(r, y)| {
            (0..ribs)
                .map(|j| {
                    let a = TAU * j as f64 / ribs as f64;
                    DVec3::new(r * a.cos(), y, r * a.sin())
                })
                .collect()
        })
        .collect();
    let paint = |_: usize, j: usize| {
        if j.is_multiple_of(2) {
            plain(stripe)
        } else {
            plain(white)
        }
    };
    crate::carmesh::surface(&mut b, &rows, true, paint);
    crate::carmesh::inside(|| crate::carmesh::surface(&mut b, &rows, true, paint));
    if lod != Lod::Far {
        // The valance: a short skirt hanging from the rim.
        let rim: Vec<Vec<DVec3>> = [(1.2, 2.06), (1.2, 1.92)]
            .iter()
            .map(|&(r, y)| {
                (0..ribs)
                    .map(|j| {
                        let a = TAU * j as f64 / ribs as f64;
                        DVec3::new(r * a.cos(), y, r * a.sin())
                    })
                    .collect()
            })
            .collect();
        crate::carmesh::surface(&mut b, &rim, true, paint);
        crate::carmesh::inside(|| crate::carmesh::surface(&mut b, &rim, true, paint));
        lathe(
            &mut b,
            DVec3::Y * 2.46,
            DVec3::Y,
            &[(0.0, 0.0), (0.04, 0.0), (0.04, 0.06), (0.0, 0.1)],
            8,
            |_| metal(chrome),
        );
    }
    b.finish()
}

/// One piece of front-garden fence, [`FENCE_LEN`] along x, facing +z:
/// 0 a white picket fence, 1 a black iron railing with gilt spear heads
/// on a stone kerb, 2 a chrome rail with ball-topped posts. Each piece has
/// the post at its left end; the next piece's post closes the right.
pub fn fence(variant: u8, lod: Lod) -> Mesh {
    use hw_gen::urban::props::{FENCE_HEIGHT, FENCE_LEN};
    let mut b = MeshBuilder::new();
    let half = FENCE_LEN * 0.5;
    match variant % 3 {
        0 => {
            let white = rgb(244, 242, 234);
            // Post and two rails.
            slab(
                &mut b,
                DVec3::new(-half + 0.05, FENCE_HEIGHT * 0.5 + 0.03, 0.0),
                [DVec3::X, DVec3::Y],
                DVec3::new(0.05, FENCE_HEIGHT * 0.5 + 0.03, 0.05),
                white,
            );
            for y in [0.25, 0.72] {
                slab(
                    &mut b,
                    DVec3::new(0.05, y, -0.03),
                    [DVec3::X, DVec3::Y],
                    DVec3::new(half - 0.05, 0.035, 0.015),
                    white,
                );
            }
            // Pickets with pointed tops, in front of the rails.
            let n = lod.pick(11, 11, 0);
            for k in 0..n {
                let x = -half + 0.2 + (FENCE_LEN - 0.3) * k as f64 / n as f64;
                let (w, d) = (0.045, 0.012);
                let top = FENCE_HEIGHT - 0.12;
                let z0 = -0.012 + 0.0;
                let pts = |z: f64| {
                    vec![
                        DVec3::new(x - w, 0.05, z),
                        DVec3::new(x + w, 0.05, z),
                        DVec3::new(x + w, top, z),
                        DVec3::new(x, top + 0.1, z),
                        DVec3::new(x - w, top, z),
                    ]
                };
                let (f, bk) = (pts(z0 + d), pts(z0 - d));
                b.polygon(&f, DVec3::Z, white);
                b.polygon(&bk, -DVec3::Z, white);
                for i in 0..5 {
                    let j = (i + 1) % 5;
                    let mid = (f[i] + f[j]) * 0.5;
                    let out =
                        (mid - DVec3::new(x, (top + 0.05) * 0.5, z0)) * DVec3::new(1.0, 1.0, 0.0);
                    b.quad([f[i], f[j], bk[j], bk[i]], out, white);
                }
            }
            if lod == Lod::Far {
                slab(
                    &mut b,
                    DVec3::new(0.05, 0.5, 0.0),
                    [DVec3::X, DVec3::Y],
                    DVec3::new(half - 0.05, 0.42, 0.01),
                    white,
                );
            }
        }
        1 => {
            let iron = rgb(30, 32, 34);
            let stone = rgb(176, 168, 152);
            let gold = rgb(214, 170, 72);
            slab(
                &mut b,
                DVec3::new(0.0, 0.12, 0.0),
                [DVec3::X, DVec3::Y],
                DVec3::new(half, 0.12, 0.13),
                stone,
            );
            // A stone pier at the left end with a cap.
            slab(
                &mut b,
                DVec3::new(-half + 0.14, 0.24 + 0.4, 0.0),
                [DVec3::X, DVec3::Y],
                DVec3::new(0.14, 0.4, 0.15),
                stone,
            );
            slab(
                &mut b,
                DVec3::new(-half + 0.14, 1.07, 0.0),
                [DVec3::X, DVec3::Y],
                DVec3::new(0.17, 0.03, 0.18),
                stone,
            );
            for y in [0.36, 0.86] {
                tube(
                    &mut b,
                    &[DVec3::new(-half + 0.28, y, 0.0), DVec3::new(half, y, 0.0)],
                    (0.014, 0.014),
                    6,
                    metal(iron),
                );
            }
            let n = lod.pick(17, 9, 0);
            for k in 0..n {
                let x = -half + 0.36 + (FENCE_LEN - 0.4) * k as f64 / n as f64;
                tube(
                    &mut b,
                    &[DVec3::new(x, 0.24, 0.0), DVec3::new(x, 0.94, 0.0)],
                    (0.011, 0.011),
                    lod.pick(6, 4, 4),
                    metal(iron),
                );
                if lod == Lod::Near {
                    lathe(
                        &mut b,
                        DVec3::new(x, 0.94, 0.0),
                        DVec3::Y,
                        &[(0.0, 0.0), (0.022, 0.01), (0.0, 0.07)],
                        6,
                        |_| metal(gold),
                    );
                }
            }
            if lod == Lod::Far {
                slab(
                    &mut b,
                    DVec3::new(0.14, 0.6, 0.0),
                    [DVec3::X, DVec3::Y],
                    DVec3::new(half - 0.14, 0.36, 0.006),
                    iron,
                );
            }
        }
        _ => {
            let chrome = rgb(214, 218, 224);
            let teal = rgb(64, 168, 160);
            let post = |b: &mut MeshBuilder, x: f64| {
                tube(
                    b,
                    &[
                        DVec3::new(x, 0.0, 0.0),
                        DVec3::new(x, FENCE_HEIGHT - 0.1, 0.0),
                    ],
                    (0.03, 0.03),
                    sides(lod, 10),
                    metal(chrome),
                );
                lathe(
                    b,
                    DVec3::new(x, FENCE_HEIGHT - 0.12, 0.0),
                    DVec3::Y,
                    &(0..=6)
                        .map(|k| {
                            let a = PI * k as f64 / 6.0;
                            (0.06 * a.sin(), 0.06 - 0.06 * a.cos())
                        })
                        .collect::<Vec<_>>(),
                    sides(lod, 12),
                    |_| plain(teal),
                );
            };
            post(&mut b, -half + 0.05);
            post(&mut b, 0.05);
            for y in [0.3, 0.55, 0.8] {
                tube(
                    &mut b,
                    &[
                        DVec3::new(-half + 0.05, y, 0.0),
                        DVec3::new(half - 0.02, y, 0.0),
                    ],
                    (0.018, 0.018),
                    sides(lod, 8),
                    metal(chrome),
                );
            }
        }
    }
    b.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "on demand (looks): scripts/check.sh --on-demand"]
    fn props_have_round_parts_and_light_far_levels() {
        let count = |m: Mesh| m.build().triangle_count();
        for style in [LampStyle::Lantern, LampStyle::Deco, LampStyle::Atomic] {
            let (near, far) = (count(lamp(style, Lod::Near)), count(lamp(style, Lod::Far)));
            assert!(near > 800 && far * 3 < near, "{style:?}: {near} / {far}");
        }
        for (name, f) in [
            ("bench", bench as fn(Lod) -> Mesh),
            ("bin", bin),
            ("hydrant", hydrant),
            ("mailbox", mailbox),
            ("bollard", bollard),
            ("traffic light", traffic_light),
        ] {
            let (near, far) = (count(f(Lod::Near)), count(f(Lod::Far)));
            assert!(near >= 300, "{name}: {near}");
            assert!(far < near, "{name}: {far} vs {near}");
        }
    }
}
