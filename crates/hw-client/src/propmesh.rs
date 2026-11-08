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
use hw_gen::urban::props::LampStyle;
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

/// Traffic lights facing +z: a turned pole, a rounded yellow housing, three
/// lenses under hooded visors.
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
    for (y, c) in [
        (3.62, rgb(255, 50, 40)),
        (3.35, rgb(255, 180, 40)),
        (3.08, rgb(60, 255, 120)),
    ] {
        let at = DVec3::new(0.0, y, 0.12);
        lathe(
            &mut b,
            at,
            DVec3::Z,
            &[
                (0.0, 0.0),
                (0.11, 0.0),
                (0.11, 0.015),
                (0.095, 0.02),
                (0.06, 0.035),
                (0.0, 0.04),
            ],
            sides(lod, 18),
            |k| {
                if k < 2 {
                    plain(rgb(24, 24, 26))
                } else {
                    glow(c)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
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
