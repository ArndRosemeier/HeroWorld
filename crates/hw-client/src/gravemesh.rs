//! Graveyards at full quality: eight headstones of the golden age (an
//! arched slab, a rocket on its fins, an atom, a Googie boomerang, a ringed
//! planet, a star-tipped obelisk, an old ringed cross and a stone with a
//! little sheet ghost peeking over it), each with its kerbed plot; three
//! mausoleums (a chapel under a swooping parabolic roof, a rocket monument
//! and a saucer tomb); the railing round the ground and its gate.
//!
//! Each fits the solid box the town gives it
//! (`hw_gen::urban::props::GraveStyle::block`, `CHAPEL`, `GATE`, ...).
//! Units are metres, +y up, standing on y = 0, the front to +z; three
//! levels each ([`Lod`]).

use crate::carmesh::{lathe, slab, spline, tube, Lod, Paint};
use crate::citymesh::{MeshBuilder, Surf};
use engine::prelude::*;
use glam::{DVec2, DVec3};
use hw_gen::urban::props::{
    GraveStyle, TombStyle, CHAPEL, GATE, GRAVE_LAWN, GRAVE_PLOT, RAIL_HEIGHT, RAIL_LEN,
    ROCKET_TOMB, SAUCER_TOMB,
};
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

fn sides(lod: Lod, near: usize) -> usize {
    lod.pick(near, (near / 2).max(8), 6)
}

/// Runs `f` with faces of `surf` (boxes and polygons take the builder's).
fn with(b: &mut MeshBuilder, surf: Surf, f: impl FnOnce(&mut MeshBuilder)) {
    let saved = b.surf;
    b.surf = surf;
    f(b);
    b.surf = saved;
}

/// An axis-aligned box from `lo` to `hi`.
fn bx(b: &mut MeshBuilder, lo: DVec3, hi: DVec3, c: Color) {
    let (lo, hi) = (lo.min(hi), lo.max(hi));
    b.oriented_box((lo + hi) * 0.5, DVec3::X, (hi - lo) * 0.5, c);
}

// Stone colours.
const GRANITE: Color = Color {
    r: 0.30,
    g: 0.30,
    b: 0.32,
    a: 1.0,
};
const DARK: Color = Color {
    r: 0.035,
    g: 0.038,
    b: 0.045,
    a: 1.0,
};
const MARBLE: Color = Color {
    r: 0.80,
    g: 0.78,
    b: 0.73,
    a: 1.0,
};
const SANDSTONE: Color = Color {
    r: 0.47,
    g: 0.40,
    b: 0.29,
    a: 1.0,
};

fn gold() -> Color {
    rgb(212, 168, 70)
}
fn chrome() -> Color {
    rgb(214, 218, 224)
}
/// The faint Aether glow of the graveyard's lights and ghost eyes.
fn ghostlight() -> Color {
    rgb(140, 255, 200)
}

/// A slab whose outline (`pts`, counter-clockwise seen from the front,
/// x right and y up) is pushed from z0 to z1. Sides bend smoothly round
/// curves and keep crisp corners where the outline turns sharply; the
/// faces front and back are fans, so the outline must be convex.
fn prism(b: &mut MeshBuilder, pts: &[DVec2], (z0, z1): (f64, f64), paint: Paint) {
    let Some((surf, col)) = paint else {
        return;
    };
    let n = pts.len();
    if n < 3 {
        return;
    }
    let at = |p: DVec2, z: f64| DVec3::new(p.x, p.y, z);
    with(b, surf, |b| {
        b.polygon(
            &pts.iter().map(|p| at(*p, z1)).collect::<Vec<_>>(),
            DVec3::Z,
            col,
        );
        b.polygon(
            &pts.iter().map(|p| at(*p, z0)).collect::<Vec<_>>(),
            -DVec3::Z,
            col,
        );
    });
    // Sharp corners: where the outline turns by more than ~35 degrees.
    let sharp: Vec<bool> = (0..n)
        .map(|k| {
            let a = (pts[k] - pts[(k + n - 1) % n]).normalize_or_zero();
            let c = (pts[(k + 1) % n] - pts[k]).normalize_or_zero();
            a.dot(c) < 0.82
        })
        .collect();
    let first = sharp.iter().position(|s| *s);
    let runs: Vec<Vec<usize>> = match first {
        // A smooth loop all round.
        None => vec![(0..=n).map(|k| k % n).collect()],
        Some(f) => {
            let mut runs = Vec::new();
            let mut cur = vec![f];
            for s in 1..=n {
                let k = (f + s) % n;
                cur.push(k);
                if sharp[k] {
                    runs.push(std::mem::replace(&mut cur, vec![k]));
                }
            }
            runs
        }
    };
    for run in runs {
        let rows = vec![
            run.iter().map(|&k| at(pts[k], z0)).collect::<Vec<_>>(),
            run.iter().map(|&k| at(pts[k], z1)).collect::<Vec<_>>(),
        ];
        // Counter-clockwise outline: (+z) x along is inwards, so `out` is false.
        b.grid(&rows, false, false, |_, _| Some((surf, col)));
    }
}

/// Points of an arc round `c`, from angle `a0` to `a1` (radians).
fn arc(c: DVec2, r: f64, a0: f64, a1: f64, steps: usize) -> Vec<DVec2> {
    (0..=steps)
        .map(|k| {
            let a = a0 + (a1 - a0) * k as f64 / steps as f64;
            c + DVec2::new(a.cos(), a.sin()) * r
        })
        .collect()
}

/// A ring (torus) round `c` in the plane at right angles to `axis`.
fn torus(b: &mut MeshBuilder, c: DVec3, axis: DVec3, r: f64, thick: f64, lod: Lod, paint: Paint) {
    let axis = axis.normalize();
    let helper = if axis.y.abs() < 0.9 {
        DVec3::Y
    } else {
        DVec3::X
    };
    let u = axis.cross(helper).normalize();
    let v = axis.cross(u);
    let seg = sides(lod, 40);
    let path: Vec<DVec3> = (0..=seg)
        .map(|k| {
            let a = TAU * k as f64 / seg as f64;
            c + (u * a.cos() + v * a.sin()) * r
        })
        .collect();
    tube(b, &path, (thick, thick), sides(lod, 10), paint);
}

/// A closed ellipsoid, smooth.
fn ellipsoid(b: &mut MeshBuilder, c: DVec3, r: DVec3, lod: Lod, paint: Paint) {
    let seg = sides(lod, 24);
    let rings = lod.pick(12, 8, 4);
    let prof: Vec<(f64, f64)> = (0..=rings)
        .map(|k| {
            let e = PI * k as f64 / rings as f64 - PI / 2.0;
            (e.cos(), e.sin())
        })
        .collect();
    // Lathe a unit sphere, then stretch: lathe has no stretch, so build
    // the rows here.
    let rows: Vec<Vec<DVec3>> = prof
        .iter()
        .map(|&(rc, y)| {
            (0..seg)
                .map(|i| {
                    let a = TAU * i as f64 / seg as f64;
                    c + DVec3::new(r.x * rc * a.cos(), r.y * y, r.z * rc * a.sin())
                })
                .collect()
        })
        .collect();
    crate::carmesh::surface(b, &rows, true, |_, _| paint);
}

/// A small rounded lump (a bloom, a clump of leaves): few faces.
fn blob(b: &mut MeshBuilder, c: DVec3, r: DVec3, paint: Paint) {
    let (seg, rings) = (8, 4);
    let rows: Vec<Vec<DVec3>> = (0..=rings)
        .map(|k| {
            let e = PI * k as f64 / rings as f64 - PI / 2.0;
            (0..seg)
                .map(|i| {
                    let a = TAU * i as f64 / seg as f64;
                    c + DVec3::new(
                        r.x * e.cos() * a.cos(),
                        r.y * e.sin(),
                        r.z * e.cos() * a.sin(),
                    )
                })
                .collect()
        })
        .collect();
    crate::carmesh::surface(b, &rows, true, |_, _| paint);
}

/// A flat star with `points` points in the plane facing `n` (`up` its
/// first point), `thick` deep.
#[allow(clippy::too_many_arguments)]
fn star(
    b: &mut MeshBuilder,
    c: DVec3,
    up: DVec3,
    n: DVec3,
    r: f64,
    points: usize,
    thick: f64,
    paint: Paint,
) {
    let Some((surf, col)) = paint else {
        return;
    };
    let side = n.cross(up).normalize();
    let pt = |k: usize, z: f64| {
        let a = PI * k as f64 / points as f64;
        let rr = if k.is_multiple_of(2) { r } else { r * 0.42 };
        c + (up * a.cos() + side * a.sin()) * rr + n * z
    };
    let m = points * 2;
    with(b, surf, |b| {
        for k in 0..m {
            let j = (k + 1) % m;
            // Each face rises to a ridge in the middle: a faceted star.
            for (z, nn) in [(thick * 0.5, n), (-thick * 0.5, -n)] {
                b.tri([c + n * (z * 1.8), pt(k, z), pt(j, z)], nn, col);
            }
            b.quad(
                [
                    pt(k, -thick * 0.5),
                    pt(j, -thick * 0.5),
                    pt(j, thick * 0.5),
                    pt(k, thick * 0.5),
                ],
                (pt(k, 0.0) + pt(j, 0.0)) * 0.5 - c,
                col,
            );
        }
    });
}

/// Lines of gilt lettering on a stone's face at depth `z`: a name and the
/// dates, centred on `x`, from `y` down.
fn lettering(b: &mut MeshBuilder, x: f64, y: f64, z: f64, width: f64, lines: usize) {
    with(b, Surf::Metal, |b| {
        for k in 0..lines {
            let w = width
                * if k == 0 {
                    1.0
                } else {
                    0.62 - 0.1 * (k % 2) as f64
                };
            let h = if k == 0 { 0.03 } else { 0.018 };
            let yy = y - k as f64 * 0.075;
            bx(
                b,
                DVec3::new(x - w * 0.5, yy - h * 0.5, z),
                DVec3::new(x + w * 0.5, yy + h * 0.5, z + 0.004),
                gold(),
            );
        }
    });
}

// ---------------------------------------------------------------- graves

/// A grave: the headstone of [`GraveStyle::of`]`(v)`, and its plot in front
/// unless [`GRAVE_LAWN`] is set.
pub fn grave(v: u8, lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let style = GraveStyle::of(v);
    let (hx, hz, _) = style.block();
    match style {
        GraveStyle::Arched => arched(&mut b, lod, false),
        GraveStyle::Rocket => rocket_stele(&mut b, lod),
        GraveStyle::Atom => atom_stele(&mut b, lod),
        GraveStyle::Boomerang => boomerang(&mut b, lod),
        GraveStyle::Planet => planet(&mut b, lod),
        GraveStyle::Star => obelisk(&mut b, lod),
        GraveStyle::Cross => cross(&mut b, lod),
        GraveStyle::Ghost => arched(&mut b, lod, true),
    }
    if v & GRAVE_LAWN == 0 {
        plot(&mut b, hz.min(0.12), lod, v);
    } else {
        // A marker on the lawn: a flat stone footing round it, flush.
        slab(
            &mut b,
            DVec3::new(0.0, -0.04, 0.0),
            [DVec3::X, DVec3::Y],
            DVec3::new(hx.min(0.42) + 0.06, 0.07, hz + 0.08),
            GRANITE,
        );
    }
    b.finish()
}

/// A low stepped plinth under a stone: half width, half depth, height.
fn plinth(b: &mut MeshBuilder, hx: f64, hz: f64, h: f64, col: Color) {
    // It reaches below the ground, for stones on a slope.
    slab(
        b,
        DVec3::new(0.0, (h - 0.15) * 0.5, 0.0),
        [DVec3::X, DVec3::Y],
        DVec3::new(hx, (h + 0.15) * 0.5, hz),
        col,
    );
}

/// The arched slab, or (with `ghost`) the round stone a ghost peeks over.
fn arched(b: &mut MeshBuilder, lod: Lod, ghost: bool) {
    let (hx, hz, h) = if ghost {
        GraveStyle::Ghost.block()
    } else {
        GraveStyle::Arched.block()
    };
    let steps = lod.pick(20, 10, 4);
    let base = 0.14;
    plinth(b, hx, hz, base, GRANITE);
    // The stone: straight sides up to the arch.
    let w = hx - 0.06;
    let (r, top) = if ghost { (w, 0.93) } else { (w, h) };
    let c = DVec2::new(0.0, top - r);
    let mut pts = vec![DVec2::new(-w, base), DVec2::new(w, base)];
    pts.extend(arc(c, r, 0.0, PI, steps));
    let d = hz - 0.03;
    let col = if ghost { MARBLE } else { GRANITE };
    prism(b, &pts, (-d, d), plain(col));
    if lod == Lod::Far {
        return;
    }
    // A polished panel inset on the front, and the lettering on it.
    let pw = w - 0.07;
    let mut panel = vec![DVec2::new(-pw, base + 0.1), DVec2::new(pw, base + 0.1)];
    panel.extend(arc(c, pw, 0.0, PI, steps));
    let polish = if ghost { rgb(200, 205, 214) } else { DARK };
    prism(b, &panel, (d, d + 0.008), metal(polish));
    if lod == Lod::Near {
        lettering(b, 0.0, top - r - 0.02, d + 0.008, pw * 1.1, 4);
        if !ghost {
            // A small chrome star in the arch.
            star(
                b,
                DVec3::new(0.0, c.y + r * 0.55, d + 0.012),
                DVec3::Y,
                DVec3::Z,
                0.06,
                5,
                0.01,
                metal(chrome()),
            );
        }
    }
    if ghost {
        // The little sheet ghost behind the stone, peeking over its top,
        // its hands over the edge and its eyes glowing.
        let gx = 0.08;
        let body = DVec3::new(gx, top + 0.02, -0.02);
        ellipsoid(
            b,
            body,
            DVec3::new(0.17, 0.2, 0.1),
            lod,
            plain(rgb(244, 244, 238)),
        );
        // The sheet's hem falls behind the stone.
        let seg = sides(lod, 16);
        let hem: Vec<(f64, f64)> = vec![(0.0, 0.0), (0.13, 0.02), (0.16, 0.2), (0.165, 0.32)];
        lathe(
            b,
            DVec3::new(gx, top - 0.3, -0.04),
            DVec3::Y,
            &hem,
            seg,
            |_| plain(rgb(240, 240, 234)),
        );
        for sx in [-1.0, 1.0] {
            ellipsoid(
                b,
                DVec3::new(gx + sx * 0.13, top - 0.005, d - 0.01),
                DVec3::new(0.045, 0.035, 0.05),
                lod,
                plain(rgb(244, 244, 238)),
            );
        }
        for sx in [-1.0, 1.0] {
            ellipsoid(
                b,
                DVec3::new(gx + sx * 0.055, top + 0.07, 0.075),
                DVec3::new(0.022, 0.032, 0.008),
                lod,
                glow(ghostlight()),
            );
        }
        // A round little "o" of a mouth.
        if lod == Lod::Near {
            ellipsoid(
                b,
                DVec3::new(gx, top - 0.0, 0.078),
                DVec3::new(0.016, 0.02, 0.006),
                lod,
                plain(rgb(40, 40, 46)),
            );
        }
    }
}

/// A rocket standing on two fins, a plaque on its belly.
fn rocket_stele(b: &mut MeshBuilder, lod: Lod) {
    let (hx, hz, h) = GraveStyle::Rocket.block();
    plinth(b, 0.3, hz - 0.02, 0.18, GRANITE);
    let cream = rgb(232, 224, 200);
    let teal = rgb(56, 140, 138);
    let r = 0.14;
    let y0 = 0.3;
    let mut prof = vec![(0.0, y0 - 0.06), (r * 0.7, y0 - 0.06), (r, y0 + 0.04)];
    prof.extend([(r * 1.04, 0.62), (r * 1.02, 0.95), (r * 0.98, 1.08)]);
    let tip = h - 0.03;
    let nose = 1.08;
    for k in 1..=8 {
        let t = k as f64 / 8.0;
        prof.push((r * 0.98 * (1.0 - t * t).sqrt(), nose + (tip - nose) * t));
    }
    lathe(b, DVec3::ZERO, DVec3::Y, &prof, sides(lod, 28), |k| {
        if k >= 5 {
            metal(chrome())
        } else if k == 4 || k == 0 {
            plain(teal)
        } else {
            plain(cream)
        }
    });
    // Two swept fins down to the plinth, out to the sides.
    for sx in [-1.0, 1.0] {
        let p = |x: f64, y: f64| DVec3::new(sx * x, y, 0.0);
        let edge = spline(
            &[p(r, 0.78), p(0.3, 0.5), p(hx - 0.02, 0.2)],
            lod.pick(6, 3, 1),
            false,
        );
        let t = 0.025;
        let inner: Vec<DVec3> = edge
            .iter()
            .map(|e| p(r * 0.9, (e.y - 0.05).max(0.2)))
            .collect();
        for k in 0..edge.len() - 1 {
            for sz in [-1.0, 1.0] {
                let o = DVec3::Z * t * sz;
                b.quad(
                    [inner[k] + o, edge[k] + o, edge[k + 1] + o, inner[k + 1] + o],
                    o,
                    teal,
                );
            }
        }
        tube(b, &edge, (t, t), sides(lod, 8), plain(teal));
    }
    if lod != Lod::Far {
        // The plaque: a dark polished oval with gilt letters, in a chrome bezel.
        let zc = r + 0.004;
        let oval = arc(DVec2::new(0.0, 0.66), 1.0, 0.0, TAU, sides(lod, 24));
        let scale = |v: &Vec<DVec2>, sx: f64, sy: f64| {
            v.iter()
                .map(|p| DVec2::new(p.x * sx, 0.66 + (p.y - 0.66) * sy))
                .collect::<Vec<_>>()
        };
        prism(
            b,
            &scale(&oval, 0.11, 0.17),
            (zc - 0.03, zc + 0.012),
            metal(chrome()),
        );
        prism(b, &scale(&oval, 0.09, 0.15), (zc, zc + 0.016), metal(DARK));
        if lod == Lod::Near {
            lettering(b, 0.0, 0.73, zc + 0.016, 0.1, 3);
            // A porthole above, glowing faintly.
            lathe(
                b,
                DVec3::new(0.0, 0.94, r * 0.98),
                DVec3::Z,
                &[
                    (0.0, 0.0),
                    (0.04, 0.0),
                    (0.04, 0.012),
                    (0.03, 0.016),
                    (0.0, 0.02),
                ],
                16,
                |k| {
                    if k < 2 {
                        metal(chrome())
                    } else {
                        glow(ghostlight())
                    }
                },
            );
        }
    }
}

/// A short tapered column under an atom: a nucleus and three orbits.
fn atom_stele(b: &mut MeshBuilder, lod: Lod) {
    let (hx, hz, h) = GraveStyle::Atom.block();
    plinth(b, hx - 0.02, hz - 0.02, 0.16, GRANITE);
    // The shaft: square, tapering, polished dark granite with chamfered
    // edges (an octagon of uneven sides).
    let section = |half: f64, y: f64| -> Vec<DVec3> {
        let c = half * 0.25;
        [
            (half - c, half),
            (-(half - c), half),
            (-half, half - c),
            (-half, -(half - c)),
            (-(half - c), -half),
            (half - c, -half),
            (half, -(half - c)),
            (half, half - c),
        ]
        .iter()
        .map(|&(x, z)| DVec3::new(x, y, z))
        .collect()
    };
    let rows = vec![section(0.17, 0.16), section(0.11, 1.18)];
    b.grid(&rows, true, false, |_, _| Some((Surf::Metal, DARK)));
    // Its top.
    let top = section(0.11, 1.18);
    with(b, Surf::Metal, |b| b.polygon(&top, DVec3::Y, DARK));
    if lod != Lod::Far {
        lettering(b, 0.0, 0.95, 0.141, 0.16, 4);
        // Gold lettering is on the slanted face: close enough at 0.14.
    }
    let c = DVec3::new(0.0, h - 0.33, 0.0);
    tube(
        b,
        &[DVec3::new(0.0, 1.17, 0.0), c],
        (0.02, 0.02),
        sides(lod, 10),
        metal(chrome()),
    );
    ellipsoid(b, c, DVec3::splat(0.075), lod, metal(gold()));
    let r = hx - 0.04;
    for k in 0..3 {
        let a = PI * k as f64 / 3.0;
        let axis = DVec3::new(a.cos() * 0.8, 0.6, a.sin() * 0.8);
        let axis = DVec3::new(axis.x, axis.y, axis.z).normalize();
        torus(b, c, axis, r * 0.95, 0.012, lod, metal(chrome()));
        if lod != Lod::Far {
            // An electron on each orbit.
            let helper = if axis.y.abs() < 0.9 {
                DVec3::Y
            } else {
                DVec3::X
            };
            let u = axis.cross(helper).normalize();
            let v = axis.cross(u);
            let t = 0.7 + k as f64 * 2.1;
            let e = c + (u * t.cos() + v * t.sin()) * r * 0.95;
            ellipsoid(b, e, DVec3::splat(0.03), lod, glow(ghostlight()));
        }
    }
}

/// A Googie boomerang: a cream fin sweeping up to one side, a teal band
/// along its foot and a chrome starburst.
fn boomerang(b: &mut MeshBuilder, lod: Lod) {
    let (hx, hz, h) = GraveStyle::Boomerang.block();
    plinth(b, hx, hz, 0.1, GRANITE);
    let cream = rgb(236, 228, 206);
    let teal = rgb(52, 150, 146);
    let per = lod.pick(10, 5, 2);
    // Up the right side, along a sweeping top to the high left tip.
    let mut pts = vec![
        DVec2::new(-hx + 0.08, 0.1),
        DVec2::new(hx - 0.02, 0.1),
        DVec2::new(hx, 0.2),
    ];
    let top = spline(
        &[
            DVec3::new(hx, 0.2, 0.0),
            DVec3::new(hx * 0.4, 0.42, 0.0),
            DVec3::new(-hx * 0.35, 0.68, 0.0),
            DVec3::new(-hx + 0.05, h - 0.01, 0.0),
        ],
        per,
        false,
    );
    pts.extend(top.iter().skip(1).map(|p| DVec2::new(p.x, p.y)));
    pts.push(DVec2::new(-hx + 0.02, h - 0.12));
    let pts = convex(&pts);
    let d = hz - 0.04;
    prism(b, &pts, (-d, d), plain(cream));
    // The band along the foot, proud of the face.
    let band = convex(&[
        DVec2::new(-hx + 0.1, 0.13),
        DVec2::new(hx - 0.06, 0.13),
        DVec2::new(hx - 0.04, 0.2),
        DVec2::new(-hx + 0.1, 0.3),
    ]);
    prism(b, &band, (-d - 0.012, d + 0.012), plain(teal));
    if lod != Lod::Far {
        star(
            b,
            DVec3::new(-hx * 0.45, 0.6, d + 0.02),
            DVec3::new(0.3, 1.0, 0.0).normalize(),
            DVec3::Z,
            0.09,
            4,
            0.016,
            metal(chrome()),
        );
        if lod == Lod::Near {
            lettering(b, 0.1, 0.4, d, 0.32, 3);
        }
    }
}

/// The convex hull of points (in order round, counter-clockwise).
fn convex(pts: &[DVec2]) -> Vec<DVec2> {
    let mut p: Vec<DVec2> = pts.to_vec();
    p.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    p.dedup_by(|a, b| a.distance(*b) < 1e-9);
    if p.len() < 3 {
        return p;
    }
    let cross = |o: DVec2, a: DVec2, b: DVec2| (a - o).perp_dot(b - o);
    let mut lower: Vec<DVec2> = Vec::new();
    for &q in &p {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], q) <= 0.0 {
            lower.pop();
        }
        lower.push(q);
    }
    let mut upper: Vec<DVec2> = Vec::new();
    for &q in p.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], q) <= 0.0 {
            upper.pop();
        }
        upper.push(q);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// A ringed planet on a fluted column.
fn planet(b: &mut MeshBuilder, lod: Lod) {
    let (hx, _, h) = GraveStyle::Planet.block();
    plinth(b, 0.27, 0.27, 0.14, GRANITE);
    let seg = sides(lod, 28);
    let prof = [
        (0.0, 0.13),
        (0.2, 0.13),
        (0.2, 0.19),
        (0.15, 0.25),
        (0.12, 0.32),
        (0.105, 0.72),
        (0.14, 0.78),
        (0.14, 0.82),
        (0.0, 0.83),
    ];
    lathe(b, DVec3::ZERO, DVec3::Y, &prof, seg, |_| plain(MARBLE));
    if lod == Lod::Near {
        // Flutes: thin dark grooves up the shaft.
        for k in 0..12 {
            let a = TAU * k as f64 / 12.0;
            let d = DVec3::new(a.cos(), 0.0, a.sin());
            tube(
                b,
                &[d * 0.112 + DVec3::Y * 0.36, d * 0.103 + DVec3::Y * 0.7],
                (0.008, 0.008),
                4,
                plain(rgb(150, 146, 136)),
            );
        }
        lettering(b, 0.0, 0.62, 0.11, 0.12, 2);
    }
    let r = 0.2;
    let c = DVec3::new(0.0, h - r - 0.02, 0.0);
    ellipsoid(b, c, DVec3::splat(r), lod, metal(rgb(36, 70, 120)));
    // The ring: a flat band round it, tilted.
    let axis = DVec3::new(0.0, 1.0, 0.0)
        .lerp(DVec3::new(0.35, 0.0, 0.25), 0.55)
        .normalize();
    let helper = DVec3::X;
    let u = axis.cross(helper).normalize();
    let v = axis.cross(u);
    let n = sides(lod, 40);
    let (r0, r1) = (r * 1.3, hx - 0.03);
    let ring = |rr: f64, lift: f64| -> Vec<DVec3> {
        (0..n)
            .map(|k| {
                let a = TAU * k as f64 / n as f64;
                c + (u * a.cos() + v * a.sin()) * rr + axis * lift
            })
            .collect()
    };
    let rows_top = vec![ring(r0, 0.006), ring(r1, 0.006)];
    let rows_bot = vec![ring(r0, -0.006), ring(r1, -0.006)];
    b.grid(&rows_top, true, true, |_, _| Some((Surf::Metal, gold())));
    b.grid(&rows_bot, true, false, |_, _| Some((Surf::Metal, gold())));
    b.grid(&[ring(r1, -0.006), ring(r1, 0.006)], true, true, |_, _| {
        Some((Surf::Metal, gold()))
    });
}

/// A tall obelisk on two steps, a chrome star on its tip.
fn obelisk(b: &mut MeshBuilder, lod: Lod) {
    let (hx, _, h) = GraveStyle::Star.block();
    plinth(b, hx, hx, 0.12, GRANITE);
    slab(
        b,
        DVec3::new(0.0, 0.18, 0.0),
        [DVec3::X, DVec3::Y],
        DVec3::new(hx - 0.05, 0.06, hx - 0.05),
        GRANITE,
    );
    // Shaft and pyramidion: hard edges, as an obelisk has.
    let (b0, b1, y0, y1, tip) = (0.15, 0.09, 0.24, 1.95, 2.06);
    let sq = |half: f64, y: f64| {
        [
            DVec3::new(-half, y, -half),
            DVec3::new(half, y, -half),
            DVec3::new(half, y, half),
            DVec3::new(-half, y, half),
        ]
    };
    with(b, Surf::Plain, |b| b.hexa(sq(b0, y0), sq(b1, y1), MARBLE));
    let top = sq(b1, y1);
    let apex = DVec3::new(0.0, tip, 0.0);
    for k in 0..4 {
        let (p, q) = (top[k], top[(k + 1) % 4]);
        b.tri([p, q, apex], (p + q) * 0.5 - DVec3::Y * y1, MARBLE);
    }
    if lod != Lod::Far {
        let face = |y: f64| b0 + (b1 - b0) * (y - y0) / (y1 - y0);
        lettering(b, 0.0, 1.0, face(1.0), 0.17, 4);
        // A bronze laurel wreath hung on its front.
        let wy = 0.62;
        torus(
            b,
            DVec3::new(0.0, wy, face(wy) + 0.03),
            DVec3::Z,
            0.09,
            0.018,
            lod,
            metal(rgb(150, 110, 52)),
        );
    }
    tube(
        b,
        &[
            DVec3::new(0.0, tip - 0.02, 0.0),
            DVec3::new(0.0, tip + 0.06, 0.0),
        ],
        (0.012, 0.012),
        8,
        metal(chrome()),
    );
    star(
        b,
        DVec3::new(0.0, h - 0.12, 0.0),
        DVec3::Y,
        DVec3::Z,
        0.11,
        lod.pick(5, 5, 4),
        0.03,
        metal(chrome()),
    );
}

/// An old ringed cross of weathered sandstone.
fn cross(b: &mut MeshBuilder, lod: Lod) {
    let (hx, hz, h) = GraveStyle::Cross.block();
    plinth(b, hx - 0.04, hz, 0.12, SANDSTONE);
    slab(
        b,
        DVec3::new(0.0, 0.18, 0.0),
        [DVec3::X, DVec3::Y],
        DVec3::new(0.16, 0.06, hz - 0.02),
        SANDSTONE,
    );
    let d = hz - 0.035;
    let shaft = [
        DVec2::new(-0.075, 0.24),
        DVec2::new(0.075, 0.24),
        DVec2::new(0.065, h - 0.02),
        DVec2::new(-0.065, h - 0.02),
    ];
    prism(b, &shaft, (-d, d), plain(SANDSTONE));
    let arm_y = h - 0.36;
    let arms = [
        DVec2::new(-hx + 0.04, arm_y - 0.065),
        DVec2::new(hx - 0.04, arm_y - 0.065),
        DVec2::new(hx - 0.04, arm_y + 0.065),
        DVec2::new(-hx + 0.04, arm_y + 0.065),
    ];
    prism(b, &arms, (-d, d), plain(SANDSTONE));
    // The ring round the crossing.
    let ring = |r: f64| arc(DVec2::new(0.0, arm_y), r, 0.0, TAU, sides(lod, 32));
    let (ro, ri) = (0.2, 0.15);
    let (outer, inner) = (ring(ro), ring(ri));
    let n = outer.len() - 1;
    let at = |p: DVec2, z: f64| DVec3::new(p.x, p.y, z);
    for k in 0..n {
        for (z, nz) in [(d * 0.7, DVec3::Z), (-d * 0.7, -DVec3::Z)] {
            b.quad(
                [
                    at(inner[k], z),
                    at(outer[k], z),
                    at(outer[k + 1], z),
                    at(inner[k + 1], z),
                ],
                nz,
                SANDSTONE,
            );
        }
    }
    b.grid(
        &[
            outer.iter().map(|p| at(*p, -d * 0.7)).collect(),
            outer.iter().map(|p| at(*p, d * 0.7)).collect(),
        ],
        false,
        false,
        |_, _| Some((Surf::Plain, SANDSTONE)),
    );
    b.grid(
        &[
            inner.iter().map(|p| at(*p, -d * 0.7)).collect(),
            inner.iter().map(|p| at(*p, d * 0.7)).collect(),
        ],
        false,
        true,
        |_, _| Some((Surf::Plain, SANDSTONE)),
    );
    if lod == Lod::Near {
        // Lichen on the arms and the plinth: pale green-grey patches.
        let lichen = rgb(124, 132, 96);
        for (x, y) in [
            (0.2, arm_y + 0.066),
            (-0.25, arm_y + 0.066),
            (0.0, h - 0.019),
        ] {
            ellipsoid(
                b,
                DVec3::new(x, y, 0.0),
                DVec3::new(0.05, 0.006, d * 0.8),
                lod,
                plain(lichen),
            );
        }
        lettering(b, 0.0, 0.62, d, 0.09, 2);
    }
}

/// The plot in front of a stone (towards +z): a granite kerb round a bed
/// of ivy, flowers in it and, on some, a little lantern glowing.
fn plot(b: &mut MeshBuilder, from: f64, lod: Lod, v: u8) {
    let (hw, len, h) = GRAVE_PLOT;
    let (z0, z1) = (from + 0.02, from + len);
    let t = 0.07;
    // Kerb: four stones, down into the ground for slopes.
    let kerb = |b: &mut MeshBuilder, lo: DVec3, hi: DVec3| bx(b, lo, hi, GRANITE);
    kerb(b, DVec3::new(-hw, -0.15, z0), DVec3::new(-hw + t, h, z1));
    kerb(b, DVec3::new(hw - t, -0.15, z0), DVec3::new(hw, h, z1));
    kerb(
        b,
        DVec3::new(-hw + t, -0.15, z1 - t),
        DVec3::new(hw - t, h, z1),
    );
    if lod == Lod::Far {
        bx(
            b,
            DVec3::new(-hw + t, -0.1, z0),
            DVec3::new(hw - t, h * 0.6, z1 - t),
            rgb(40, 62, 34),
        );
        return;
    }
    // The bed: dark earth under ivy, mounded a little.
    let ivy = rgb(34, 64, 30);
    let n = lod.pick(8, 3, 1);
    let rows: Vec<Vec<DVec3>> = (0..=n)
        .map(|i| {
            let z = z0 + 0.01 + (z1 - t - z0 - 0.01) * i as f64 / n as f64;
            (0..=4)
                .map(|j| {
                    let x = (-hw + t) + (2.0 * (hw - t)) * j as f64 / 4.0;
                    let s = (PI * i as f64 / n as f64).sin();
                    let e = 1.0 - (x / (hw - t)).powi(2);
                    DVec3::new(x, h * 0.55 + 0.06 * s * e.max(0.0), z)
                })
                .collect()
        })
        .collect();
    b.grid(&rows, false, true, |i, j| {
        let k = (i * 7 + j * 3 + v as usize) % 5;
        Some((Surf::Plain, if k == 0 { rgb(44, 76, 36) } else { ivy }))
    });
    if lod == Lod::Near {
        // Flowers: a few clusters of blooms, in the colours of this grave.
        let palettes = [
            [rgb(220, 40, 60), rgb(250, 240, 230)],
            [rgb(250, 200, 40), rgb(240, 120, 30)],
            [rgb(150, 90, 210), rgb(250, 250, 250)],
            [rgb(250, 130, 170), rgb(220, 40, 60)],
        ];
        let pal = palettes[v as usize % palettes.len()];
        let bed = h * 0.55 + 0.05;
        for k in 0..5 {
            let x = (-0.22 + 0.11 * k as f64) * if k % 2 == 0 { 1.0 } else { -0.8 };
            let z = z0 + 0.25 + 0.17 * ((k * 3) % 5) as f64;
            let c = DVec3::new(x, bed, z);
            // A low clump of leaves, small blooms on it.
            blob(b, c, DVec3::new(0.075, 0.035, 0.07), plain(rgb(48, 96, 40)));
            for j in 0..4 {
                let a = TAU * (j as f64 + 0.3 * k as f64) / 4.0;
                let o = DVec3::new(a.cos() * 0.04, 0.03 + 0.01 * (j % 2) as f64, a.sin() * 0.04);
                blob(
                    b,
                    c + o,
                    DVec3::new(0.022, 0.014, 0.022),
                    plain(pal[(j + k) % 2]),
                );
            }
        }
        // Every other grave keeps a lantern burning on its bed.
        if v % 2 == 1 {
            let c = DVec3::new(hw * 0.45, h * 0.55 + 0.04, z0 + 0.14);
            lathe(
                b,
                c,
                DVec3::Y,
                &[
                    (0.0, 0.0),
                    (0.05, 0.0),
                    (0.05, 0.02),
                    (0.04, 0.03),
                    (0.04, 0.13),
                    (0.055, 0.14),
                    (0.03, 0.19),
                    (0.0, 0.2),
                ],
                12,
                |k| match k {
                    3 => glow(rgb(255, 120, 60)),
                    _ => metal(rgb(40, 40, 44)),
                },
            );
        }
    }
}

// ---------------------------------------------------------------- tombs

/// A mausoleum or monument ([`TombStyle::of`]).
pub fn tomb(v: u8, lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    match TombStyle::of(v) {
        TombStyle::Chapel => chapel(&mut b, lod),
        TombStyle::Rocket => rocket_tomb(&mut b, lod),
        TombStyle::Saucer => saucer_tomb(&mut b, lod),
    }
    b.finish()
}

/// A family chapel: white marble walls, a bronze door under a glowing
/// porthole, glass slits down the sides, and a parabolic roof shell that
/// sweeps up to a peak over the door.
fn chapel(b: &mut MeshBuilder, lod: Lod) {
    let (hx, hz, wall, peak) = CHAPEL;
    let foot = 0.3;
    // The plinth and its steps up to the door.
    bx(
        b,
        DVec3::new(-hx - 0.1, -0.3, -hz - 0.1),
        DVec3::new(hx + 0.1, foot, hz + 0.0),
        GRANITE,
    );
    for k in 0..2 {
        let y = foot * (2 - k) as f64 / 3.0;
        bx(
            b,
            DVec3::new(-0.95, -0.3, hz - 0.05),
            DVec3::new(0.95, y, hz + 0.32 * (k + 1) as f64),
            GRANITE,
        );
    }
    // Walls.
    bx(
        b,
        DVec3::new(-hx, foot - 0.01, -hz),
        DVec3::new(hx, wall, hz),
        MARBLE,
    );
    // A dark band at the walls' top.
    bx(
        b,
        DVec3::new(-hx - 0.03, wall - 0.18, -hz - 0.03),
        DVec3::new(hx + 0.03, wall, hz + 0.03),
        rgb(52, 58, 62),
    );
    // The door, recessed, two bronze leaves with chrome bars.
    let bronze = rgb(120, 82, 46);
    with(b, Surf::Metal, |b| {
        bx(
            b,
            DVec3::new(-0.62, foot, hz - 0.02),
            DVec3::new(-0.01, foot + 1.95, hz + 0.02),
            bronze,
        );
        bx(
            b,
            DVec3::new(0.01, foot, hz - 0.02),
            DVec3::new(0.62, foot + 1.95, hz + 0.02),
            bronze,
        );
    });
    bx(
        b,
        DVec3::new(-0.78, foot, hz),
        DVec3::new(0.78, foot + 2.12, hz + 0.06),
        DARK,
    );
    if lod != Lod::Far {
        for x in [-0.32, 0.32] {
            tube(
                b,
                &[
                    DVec3::new(x - 0.2, foot + 1.0, hz + 0.05),
                    DVec3::new(x + 0.2, foot + 1.0, hz + 0.05),
                ],
                (0.015, 0.015),
                8,
                metal(chrome()),
            );
        }
    }
    // The porthole over the door, glowing.
    lathe(
        b,
        DVec3::new(0.0, wall - 0.05 + 0.0, hz + 0.0),
        DVec3::Z,
        &[
            (0.0, 0.0),
            (0.3, 0.0),
            (0.3, 0.06),
            (0.24, 0.08),
            (0.0, 0.07),
        ],
        sides(lod, 32),
        |k| {
            if k < 2 {
                metal(chrome())
            } else {
                glow(ghostlight())
            }
        },
    );
    // Slit windows down the sides.
    for sx in [-1.0, 1.0] {
        for z in [-1.0, 0.0, 1.0] {
            with(b, Surf::Glow, |b| {
                bx(
                    b,
                    DVec3::new(sx * hx - 0.01, foot + 0.6, z - 0.08),
                    DVec3::new(sx * (hx + 0.02), wall - 0.35, z + 0.08),
                    rgb(90, 200, 170),
                );
            });
        }
    }
    // The roof: a shell rising from the back to a high front, dipping in
    // the middle across.
    let (w, z0, z1) = (hx + 0.3, -hz - 0.25, hz + 0.7);
    let base = wall + 0.08;
    let rise = peak - base - 0.38;
    let y = |x: f64, z: f64| {
        let s = ((z - z0) / (z1 - z0)).clamp(0.0, 1.0);
        base + rise * s * s + 0.38 * (x / w).powi(2)
    };
    let (nx, nz) = (lod.pick(16, 8, 4), lod.pick(14, 7, 3));
    let thick = 0.12;
    let grid = |off: f64| -> Vec<Vec<DVec3>> {
        (0..=nz)
            .map(|i| {
                let z = z0 + (z1 - z0) * i as f64 / nz as f64;
                (0..=nx)
                    .map(|j| {
                        let x = -w + 2.0 * w * j as f64 / nx as f64;
                        DVec3::new(x, y(x, z) + off, z)
                    })
                    .collect()
            })
            .collect()
    };
    let roof = rgb(232, 230, 222);
    let under = rgb(66, 150, 146);
    let (top, bot) = (grid(0.0), grid(-thick));
    // Rows run along z, columns across x: (z) x (x) points down, so the
    // top wants `out` false to face up.
    b.grid(&top, false, false, |_, _| Some((Surf::Plain, roof)));
    b.grid(&bot, false, true, |_, _| Some((Surf::Plain, under)));
    // The shell's edges.
    let edge = |b: &mut MeshBuilder, pts: Vec<DVec3>, out: DVec3| {
        for k in 0..pts.len() - 1 {
            let (p, q) = (pts[k], pts[k + 1]);
            b.quad(
                [p, q, q - DVec3::Y * thick, p - DVec3::Y * thick],
                out,
                roof,
            );
        }
    };
    edge(b, top[0].clone(), -DVec3::Z);
    edge(b, top[nz].clone(), DVec3::Z);
    edge(b, top.iter().map(|r| r[0]).collect(), -DVec3::X);
    edge(b, top.iter().map(|r| r[nx]).collect(), DVec3::X);
    // Between the walls and the shell: glass at the front, marble at the
    // sides and back.
    let fill = |b: &mut MeshBuilder,
                pts: Vec<(f64, f64)>,
                plane: &dyn Fn(f64, f64) -> DVec3,
                out: DVec3,
                col: Color,
                surf: Surf| {
        with(b, surf, |b| {
            for k in 0..pts.len() - 1 {
                let (a, c) = (pts[k], pts[k + 1]);
                b.quad(
                    [
                        plane(a.0, wall),
                        plane(c.0, wall),
                        plane(c.0, c.1),
                        plane(a.0, a.1),
                    ],
                    out,
                    col,
                );
            }
        });
    };
    let along_x = |z: f64| -> Vec<(f64, f64)> {
        (0..=nx)
            .map(|j| {
                let x = -hx + 2.0 * hx * j as f64 / nx as f64;
                (x, y(x, z) - thick + 0.01)
            })
            .collect()
    };
    let along_z = |x: f64| -> Vec<(f64, f64)> {
        (0..=nz)
            .map(|i| {
                let z = -hz + 2.0 * hz * i as f64 / nz as f64;
                (z, y(x, z) - thick + 0.01)
            })
            .collect()
    };
    fill(
        b,
        along_x(hz),
        &|x, yy| DVec3::new(x, yy, hz),
        DVec3::Z,
        rgb(150, 200, 210).with_alpha(0.35),
        Surf::Glass,
    );
    fill(
        b,
        along_x(-hz),
        &|x, yy| DVec3::new(x, yy, -hz),
        -DVec3::Z,
        MARBLE,
        Surf::Plain,
    );
    for sx in [-1.0, 1.0] {
        fill(
            b,
            along_z(sx * hx),
            &|z, yy| DVec3::new(sx * hx, yy, z),
            DVec3::X * sx,
            MARBLE,
            Surf::Plain,
        );
    }
    if lod != Lod::Far {
        // Glazing bars in the front glass, and a chrome star on the peak.
        for k in 1..4 {
            let x = -hx + 2.0 * hx * k as f64 / 4.0;
            tube(
                b,
                &[
                    DVec3::new(x, wall, hz + 0.02),
                    DVec3::new(x, y(x, hz) - thick, hz + 0.02),
                ],
                (0.02, 0.02),
                6,
                metal(chrome()),
            );
        }
        // Urns on the plinth by the steps.
        for sx in [-1.0, 1.0] {
            lathe(
                b,
                DVec3::new(sx * 1.3, foot, hz - 0.3),
                DVec3::Y,
                &[
                    (0.0, 0.0),
                    (0.14, 0.0),
                    (0.1, 0.1),
                    (0.08, 0.3),
                    (0.2, 0.45),
                    (0.22, 0.6),
                    (0.16, 0.72),
                    (0.0, 0.73),
                ],
                sides(lod, 20),
                |_| metal(bronze),
            );
        }
    }
    let tip = DVec3::new(0.0, y(0.0, z1 - 0.15), z1 - 0.15);
    tube(
        b,
        &[tip - DVec3::Y * 0.1, tip + DVec3::Y * 0.12],
        (0.02, 0.02),
        8,
        metal(chrome()),
    );
    star(
        b,
        tip + DVec3::Y * 0.22,
        DVec3::Y,
        DVec3::Z,
        0.16,
        5,
        0.04,
        metal(chrome()),
    );
}

/// A rocket on three fins over a two-step granite plinth, a bronze plaque
/// on the plinth's front.
fn rocket_tomb(b: &mut MeshBuilder, lod: Lod) {
    let (half, top, tip) = ROCKET_TOMB;
    bx(
        b,
        DVec3::new(-half, -0.3, -half),
        DVec3::new(half, top * 0.5, half),
        GRANITE,
    );
    bx(
        b,
        DVec3::new(-half + 0.25, top * 0.5 - 0.01, -half + 0.25),
        DVec3::new(half - 0.25, top, half - 0.25),
        GRANITE,
    );
    let cream = rgb(236, 230, 210);
    let red = rgb(196, 50, 44);
    let r = 0.55;
    let y0 = top + 0.45;
    let mut prof = vec![
        (0.0, top),
        (0.28, top),
        (0.36, top + 0.2),
        (0.44, y0 - 0.1),
        (r * 0.95, y0),
        (r, y0 + 0.6),
        (r * 1.01, y0 + 1.8),
        (r * 0.98, y0 + 2.6),
    ];
    let nose = y0 + 2.6;
    let cone = tip - 0.4;
    for k in 1..=10 {
        let t = k as f64 / 10.0;
        prof.push((r * 0.98 * (1.0 - t * t).sqrt(), nose + (cone - nose) * t));
    }
    let n_nose = prof.len();
    lathe(b, DVec3::ZERO, DVec3::Y, &prof, sides(lod, 36), |k| {
        if k < 3 {
            metal(chrome())
        } else if k >= n_nose - 8 {
            plain(red)
        } else if k == 5 {
            metal(chrome())
        } else {
            plain(cream)
        }
    });
    // An aerial on the nose.
    tube(
        b,
        &[
            DVec3::new(0.0, cone - 0.02, 0.0),
            DVec3::new(0.0, tip - 0.05, 0.0),
        ],
        (0.02, 0.02),
        8,
        metal(chrome()),
    );
    ellipsoid(
        b,
        DVec3::new(0.0, tip - 0.05, 0.0),
        DVec3::splat(0.05),
        lod,
        glow(ghostlight()),
    );
    // Three fins down onto the plinth.
    for k in 0..3 {
        let a = TAU * (k as f64 / 3.0) + PI / 2.0;
        let u = DVec3::new(a.cos(), 0.0, a.sin());
        let p = |rr: f64, yy: f64| u * rr + DVec3::Y * yy;
        let edge = spline(
            &[p(r, y0 + 1.4), p(1.0, y0 + 0.3), p(half - 0.3, top + 0.02)],
            lod.pick(8, 4, 2),
            false,
        );
        let side = DVec3::Y.cross(u).normalize() * 0.04;
        let inner: Vec<DVec3> = edge
            .iter()
            .map(|e| p(r * 0.85, (e.y - 0.25).max(top + 0.02)))
            .collect();
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
        tube(b, &edge, (0.04, 0.04), sides(lod, 10), plain(red));
    }
    // Portholes up its side, glowing.
    if lod != Lod::Far {
        for (k, yy) in [y0 + 1.0, y0 + 1.7].iter().enumerate() {
            let a = PI / 2.0 + k as f64 * 0.0;
            let d = DVec3::new(a.cos(), 0.0, a.sin());
            lathe(
                b,
                d * (r * 0.99) + DVec3::Y * *yy,
                d,
                &[
                    (0.0, 0.0),
                    (0.13, 0.0),
                    (0.13, 0.03),
                    (0.1, 0.045),
                    (0.0, 0.05),
                ],
                sides(lod, 20),
                |k| {
                    if k < 2 {
                        metal(chrome())
                    } else {
                        glow(ghostlight())
                    }
                },
            );
        }
        // The plaque.
        with(b, Surf::Metal, |b| {
            bx(
                b,
                DVec3::new(-0.45, 0.12, half - 0.01),
                DVec3::new(0.45, 0.4, half + 0.02),
                rgb(120, 84, 46),
            );
        });
        if lod == Lod::Near {
            lettering(b, 0.0, 0.33, half + 0.02, 0.6, 3);
        }
    }
}

/// A round marble drum under a flying-saucer roof, its rim lit.
fn saucer_tomb(b: &mut MeshBuilder, lod: Lod) {
    let (r, wall, rim, top) = SAUCER_TOMB;
    let seg = sides(lod, 48);
    // Plinth and drum.
    lathe(
        b,
        DVec3::ZERO,
        DVec3::Y,
        &[
            (0.0, -0.3),
            (r + 0.1, -0.3),
            (r + 0.1, 0.2),
            (r, 0.24),
            (r, wall),
            (r * 0.7, wall + 0.02),
            (0.0, wall + 0.02),
        ],
        seg,
        |k| match k {
            0..=2 => plain(GRANITE),
            _ => plain(MARBLE),
        },
    );
    // Chrome ribs round the drum.
    if lod != Lod::Far {
        for k in 0..12 {
            let a = TAU * (k as f64 + 0.5) / 12.0;
            if (a - PI / 2.0).abs() < 0.4 {
                continue; // the door
            }
            let d = DVec3::new(a.cos(), 0.0, a.sin());
            tube(
                b,
                &[
                    d * (r + 0.02) + DVec3::Y * 0.3,
                    d * (r + 0.02) + DVec3::Y * (wall - 0.05),
                ],
                (0.035, 0.035),
                sides(lod, 8),
                metal(chrome()),
            );
        }
        // Portholes between the ribs.
        for k in 0..6 {
            let a = TAU * k as f64 / 6.0 + PI / 2.0 + PI / 6.0;
            let d = DVec3::new(a.cos(), 0.0, a.sin());
            lathe(
                b,
                d * (r - 0.01) + DVec3::Y * 1.7,
                d,
                &[
                    (0.0, 0.0),
                    (0.16, 0.0),
                    (0.16, 0.04),
                    (0.12, 0.05),
                    (0.0, 0.055),
                ],
                sides(lod, 20),
                |k| {
                    if k < 2 {
                        metal(chrome())
                    } else {
                        glow(ghostlight())
                    }
                },
            );
        }
    }
    // The door: a bronze panel on the front, in a dark frame, a step up.
    let bronze = rgb(120, 82, 46);
    bx(
        b,
        DVec3::new(-0.65, 0.2, r - 0.25),
        DVec3::new(0.65, 2.15, r + 0.03),
        DARK,
    );
    with(b, Surf::Metal, |b| {
        bx(
            b,
            DVec3::new(-0.55, 0.22, r - 0.2),
            DVec3::new(0.55, 2.05, r + 0.05),
            bronze,
        );
    });
    bx(
        b,
        DVec3::new(-0.8, -0.3, r),
        DVec3::new(0.8, 0.12, r + 0.12),
        GRANITE,
    );
    // The saucer.
    let silver = rgb(200, 206, 212);
    let prof = [
        (0.0, wall),
        (r * 0.9, wall),
        (rim, wall + 0.42),
        (rim + 0.02, wall + 0.5),
        (rim - 0.05, wall + 0.56),
        (rim * 0.55, top - 0.42),
        (rim * 0.38, top - 0.34),
        (rim * 0.33, top - 0.2),
        (rim * 0.24, top - 0.06),
        (0.0, top),
    ];
    lathe(b, DVec3::ZERO, DVec3::Y, &prof, seg, |k| match k {
        2 | 3 => glow(ghostlight()),
        5..=8 => Some((Surf::Glass, rgb(120, 200, 190).with_alpha(0.6))),
        _ => metal(silver),
    });
    if lod != Lod::Far {
        // A star on the dome's tip.
        star(
            b,
            DVec3::new(0.0, top + 0.12, 0.0),
            DVec3::Y,
            DVec3::Z,
            0.14,
            5,
            0.03,
            metal(gold()),
        );
    }
}

// ---------------------------------------------------------------- railing

/// A piece of the graveyard's railing along x: a stone base, a pier at
/// the left end with a ball on it, iron bars with gilt spear tips between
/// two rails, a row of little iron rings under the top rail.
pub fn rail(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let half = RAIL_LEN * 0.5;
    let iron = rgb(26, 28, 30);
    let stone = rgb(150, 144, 132);
    let base = 0.38;
    bx(
        &mut b,
        DVec3::new(-half, -0.25, -0.15),
        DVec3::new(half, base, 0.15),
        stone,
    );
    bx(
        &mut b,
        DVec3::new(-half - 0.02, base - 0.05, -0.17),
        DVec3::new(half + 0.02, base, 0.17),
        rgb(170, 164, 150),
    );
    // The pier.
    let px = -half + 0.17;
    bx(
        &mut b,
        DVec3::new(px - 0.17, -0.25, -0.17),
        DVec3::new(px + 0.17, RAIL_HEIGHT - 0.2, 0.17),
        stone,
    );
    bx(
        &mut b,
        DVec3::new(px - 0.2, RAIL_HEIGHT - 0.24, -0.2),
        DVec3::new(px + 0.2, RAIL_HEIGHT - 0.17, 0.2),
        rgb(170, 164, 150),
    );
    lathe(
        &mut b,
        DVec3::new(px, RAIL_HEIGHT - 0.17, 0.0),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.07, 0.0),
            (0.05, 0.03),
            (0.08, 0.06),
            (0.085, 0.09),
            (0.07, 0.13),
            (0.0, 0.16),
        ],
        sides(lod, 16),
        |_| plain(stone),
    );
    if lod == Lod::Far {
        bx(
            &mut b,
            DVec3::new(px + 0.17, base, -0.01),
            DVec3::new(half, 1.45, 0.01),
            iron,
        );
        return b.finish();
    }
    let x0 = px + 0.17;
    for yy in [base + 0.12, 1.32] {
        tube(
            &mut b,
            &[DVec3::new(x0, yy, 0.0), DVec3::new(half, yy, 0.0)],
            (0.016, 0.02),
            6,
            metal(iron),
        );
    }
    let n = lod.pick(18, 10, 0);
    for k in 0..n {
        let x = x0 + 0.06 + (half - x0 - 0.06) * k as f64 / n as f64;
        let tall = k % 2 == 0;
        let top = if tall { 1.48 } else { 1.4 };
        tube(
            &mut b,
            &[DVec3::new(x, base, 0.0), DVec3::new(x, top, 0.0)],
            (0.011, 0.011),
            lod.pick(6, 4, 4),
            metal(iron),
        );
        if lod == Lod::Near {
            // Spear tips, gilt.
            lathe(
                &mut b,
                DVec3::new(x, top, 0.0),
                DVec3::Y,
                &[(0.0, 0.0), (0.016, 0.005), (0.028, 0.03), (0.0, 0.09)],
                6,
                |_| metal(gold()),
            );
            if k % 4 == 1 {
                torus(
                    &mut b,
                    DVec3::new(x + 0.03, 1.2, 0.0),
                    DVec3::Z,
                    0.055,
                    0.008,
                    Lod::Far,
                    metal(iron),
                );
            }
        }
    }
    b.finish()
}

/// The gate: two tapered stone pylons with glowing globes, an iron arch
/// between them with a ring and a star at its crown, and two leaves of
/// iron bars swung open inwards (to -z).
pub fn gate(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let (x, half, h, under, _top, way) = GATE;
    let stone = rgb(160, 152, 138);
    let iron = rgb(26, 28, 30);
    for sx in [-1.0, 1.0] {
        let cx = sx * x;
        let sq = |hh: f64, y: f64| {
            [
                DVec3::new(cx - hh, y, -hh),
                DVec3::new(cx + hh, y, -hh),
                DVec3::new(cx + hh, y, hh),
                DVec3::new(cx - hh, y, hh),
            ]
        };
        b.hexa(sq(half, -0.25), sq(half, 0.5), stone);
        b.hexa(sq(half - 0.04, 0.5), sq(half - 0.1, h - 0.55), stone);
        // A chrome band and a cap.
        with(&mut b, Surf::Metal, |b| {
            b.hexa(sq(half - 0.07, 1.9), sq(half - 0.07, 2.0), chrome());
        });
        b.hexa(sq(half - 0.02, h - 0.55), sq(half - 0.02, h - 0.45), stone);
        // The globe on its neck.
        lathe(
            &mut b,
            DVec3::new(cx, h - 0.45, 0.0),
            DVec3::Y,
            &[
                (0.0, 0.0),
                (0.12, 0.0),
                (0.06, 0.06),
                (0.05, 0.1),
                (0.0, 0.1),
            ],
            sides(lod, 16),
            |_| metal(chrome()),
        );
        ellipsoid(
            &mut b,
            DVec3::new(cx, h - 0.17, 0.0),
            DVec3::splat(0.17),
            lod,
            glow(ghostlight()),
        );
        // The open leaf, hinged at the pylon's inner face, lying along -z.
        let hx = sx * (way - 0.02);
        let lz = way;
        let leaf_top = |z: f64| 1.9 + 0.3 * (1.0 - (2.0 * z / lz - 1.0).powi(2));
        tube(
            &mut b,
            &[DVec3::new(hx, 0.1, -0.05), DVec3::new(hx, 2.1, -0.05)],
            (0.025, 0.025),
            sides(lod, 8),
            metal(iron),
        );
        tube(
            &mut b,
            &[DVec3::new(hx, 0.1, -lz), DVec3::new(hx, 1.95, -lz)],
            (0.02, 0.02),
            sides(lod, 8),
            metal(iron),
        );
        tube(
            &mut b,
            &[DVec3::new(hx, 0.15, -0.05), DVec3::new(hx, 0.15, -lz)],
            (0.02, 0.02),
            6,
            metal(iron),
        );
        let top_rail: Vec<DVec3> = (0..=8)
            .map(|k| {
                let z = 0.05 + (lz - 0.05) * k as f64 / 8.0;
                DVec3::new(hx, leaf_top(z), -z)
            })
            .collect();
        tube(&mut b, &top_rail, (0.02, 0.02), 6, metal(iron));
        if lod != Lod::Far {
            let n = lod.pick(12, 6, 0);
            for k in 1..n {
                let z = lz * k as f64 / n as f64;
                let t = leaf_top(z);
                tube(
                    &mut b,
                    &[DVec3::new(hx, 0.15, -z), DVec3::new(hx, t + 0.1, -z)],
                    (0.011, 0.011),
                    4,
                    metal(iron),
                );
                if lod == Lod::Near {
                    lathe(
                        &mut b,
                        DVec3::new(hx, t + 0.1, -z),
                        DVec3::Y,
                        &[(0.0, 0.0), (0.016, 0.005), (0.026, 0.03), (0.0, 0.08)],
                        6,
                        |_| metal(gold()),
                    );
                }
            }
            // A ring in the middle of each leaf.
            torus(
                &mut b,
                DVec3::new(hx, 1.2, -lz * 0.5),
                DVec3::X,
                0.28,
                0.014,
                lod,
                metal(iron),
            );
        }
    }
    // The arch: from the pylons' inner faces up to its crown.
    let crown = 4.35;
    let arch = spline(
        &[
            DVec3::new(-x + half - 0.1, under, 0.0),
            DVec3::new(-1.5, under + 0.85, 0.0),
            DVec3::new(0.0, crown, 0.0),
            DVec3::new(1.5, under + 0.85, 0.0),
            DVec3::new(x - half + 0.1, under, 0.0),
        ],
        lod.pick(8, 4, 2),
        false,
    );
    tube(&mut b, &arch, (0.05, 0.05), sides(lod, 10), metal(iron));
    let low: Vec<DVec3> = arch.iter().map(|p| *p - DVec3::Y * 0.28).collect();
    tube(&mut b, &low, (0.03, 0.03), sides(lod, 8), metal(iron));
    if lod != Lod::Far {
        // Uprights between the two curves.
        for k in (1..low.len() - 1).step_by(2) {
            tube(&mut b, &[low[k], arch[k]], (0.012, 0.012), 4, metal(iron));
        }
    }
    // The crest: a ring with a gilt star in it, and a crescent moon.
    let c = DVec3::new(0.0, crown + 0.02, 0.0);
    torus(&mut b, c, DVec3::Z, 0.26, 0.025, lod, metal(gold()));
    star(&mut b, c, DVec3::Y, DVec3::Z, 0.17, 5, 0.04, metal(gold()));
    if lod != Lod::Far {
        // The moon, glowing faintly, to one side of the crown.
        let mc = DVec3::new(-0.75, under + 0.62, 0.0);
        let outer = arc(DVec2::ZERO, 0.16, 0.6, TAU - 0.6, 16);
        let inner = arc(DVec2::new(0.07, 0.0), 0.12, 0.9, TAU - 0.9, 16);
        for k in 0..16 {
            let p = |v: DVec2, z: f64| mc + DVec3::new(v.x, v.y, z);
            for (z, nz) in [(0.02, DVec3::Z), (-0.02, -DVec3::Z)] {
                with(&mut b, Surf::Glow, |b| {
                    b.quad(
                        [
                            p(outer[k], z),
                            p(outer[k + 1], z),
                            p(inner[k + 1], z),
                            p(inner[k], z),
                        ],
                        nz,
                        rgb(220, 255, 210),
                    );
                });
            }
        }
    }
    b.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(m: &Mesh) -> (Vec3, Vec3) {
        m.bounds().expect("faces")
    }

    #[test]
    fn graveyard_models_stay_inside_their_solids_and_have_light_far_levels() {
        let count = |m: &Mesh| m.clone().build().triangle_count();
        for v in 0..GraveStyle::ALL.len() as u8 {
            for lawn in [0, GRAVE_LAWN] {
                let style = GraveStyle::of(v);
                let (hx, hz, h) = style.block();
                let near = grave(v | lawn, Lod::Near);
                let far = grave(v | lawn, Lod::Far);
                let (lo, hi) = bounds(&near);
                // The stone (the plot lies in front, low).
                assert!(hi.y as f64 <= h + 0.02, "{style:?} {hi:?} over {h}");
                let wide = hx.max(GRAVE_PLOT.0) + 0.12;
                assert!(
                    lo.x as f64 >= -wide && hi.x as f64 <= wide,
                    "{style:?} {lo:?} {hi:?}"
                );
                assert!(lo.z as f64 >= -(hz + 0.12), "{style:?} {lo:?}");
                let (n, f) = (count(&near), count(&far));
                assert!(n >= 200, "{style:?}: only {n} triangles close up");
                assert!(f * 2 < n, "{style:?}: far {f} vs near {n}");
            }
        }
        for v in 0..3 {
            let (n, f) = (count(&tomb(v, Lod::Near)), count(&tomb(v, Lod::Far)));
            assert!(n >= 1500 && f * 2 < n, "tomb {v}: {n} / {f}");
        }
        for (name, m) in [("rail", rail as fn(Lod) -> Mesh), ("gate", gate)] {
            let (n, f) = (count(&m(Lod::Near)), count(&m(Lod::Far)));
            assert!(n >= 400 && f * 2 < n, "{name}: {n} / {f}");
        }
    }
}
