//! The models of the sights (`hw_gen::urban::sights`): an Aether spring's
//! terraces, pool and crystals, a waystone ring's stones, glyphs and
//! altar, a wisp, and the gates of the courses.
//!
//! The solid shapes come from the same numbers the server's collision
//! uses (`TERRACES`, `Crystal`, `Monolith`, `ALTAR`): what is drawn is what
//! a hero bumps into. Everything here is plain data made without a
//! `World`, so the town's worker thread builds it.
//!
//! The parts that glow carry Engine custom materials ([`Materials`]): the
//! pool's drifting light, the crystals' inner fire, the glyphs' pulse, a
//! wisp's soft halo, the chevrons racing round a gate.

use crate::citymesh::{MeshBuilder, Surf};
use engine::prelude::*;
use glam::{DVec2, DVec3};
use hw_gen::urban::sights::{ALTAR, HOOP_RADIUS, POOL_R, RING_RADIUS, ROOT, TERRACES};
use std::f64::consts::TAU;
use std::sync::OnceLock;

/// Aether light (the Spire's cyan).
pub const AETHER: (u8, u8, u8) = (120, 230, 255);

fn c3(c: (u8, u8, u8)) -> Color {
    Color::rgb(c.0, c.1, c.2)
}

fn mix(a: (u8, u8, u8), b: (u8, u8, u8), t: f64) -> Color {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t).round() as u8;
    Color::rgb(l(a.0, b.0), l(a.1, b.1), l(a.2, b.2))
}

/// How fine a model is: close up, from across a field, from far away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detail {
    Near,
    Mid,
    Far,
}

impl Detail {
    pub const ALL: [Detail; 3] = [Detail::Near, Detail::Mid, Detail::Far];

    fn around(self, near: usize) -> usize {
        match self {
            Detail::Near => near,
            Detail::Mid => (near / 2).max(8),
            Detail::Far => (near / 6).max(6),
        }
    }
}

/// A smooth wobble in -1..1 round a circle, the same every time.
fn wobble(a: f64, k: f64) -> f64 {
    0.5 * (a * 3.0 + k * 1.7).sin() + 0.3 * (a * 7.0 - k * 2.3).sin() + 0.2 * (a * 13.0 + k).sin()
}

// ---- The spring ----

/// How far a terrace's rim stands out at angle `a`: scalloped like the
/// rimstone of a hot spring, never more than a hand past its solid disc.
fn rim(i: usize, a: f64) -> f64 {
    let (r, _) = TERRACES[i];
    let lobes = 5.0 + i as f64 * 2.0;
    r + 0.07 * (a * lobes + i as f64 * 1.3).sin().abs() - 0.05 + 0.03 * wobble(a, i as f64)
}

/// The spring's stone: four terraces of pale travertine, each a shallow
/// basin behind a lip, hanging over the one below in folds, and an apron
/// running out into the ground. Its origin is the spring's middle on the
/// ground; +z is the spring's `dir`.
pub fn spring_rock(detail: Detail) -> Mesh {
    let seg = detail.around(120);
    let mut b = MeshBuilder::new();
    b.flat = true;
    b.mottle = 0.1;
    let cream = (236, 229, 210);
    let ochre = (206, 160, 102);
    let pale_blue = (188, 226, 226);
    let angle = |j: usize| TAU * j as f64 / seg as f64;
    for i in 0..TERRACES.len() {
        let y = TERRACES[i].1;
        let below = if i == 0 { 0.0 } else { TERRACES[i - 1].1 };
        let drop = y - below;
        let inner = if i + 1 < TERRACES.len() {
            TERRACES[i + 1].0 - 0.12
        } else {
            0.02
        };
        // (radius by angle, height): from the basin out over the lip and
        // down the hanging folds.
        let mut rows: Vec<Vec<DVec3>> = Vec::new();
        let mut kinds: Vec<u8> = Vec::new();
        let mut row = |f: &dyn Fn(f64) -> (f64, f64), kind: u8| {
            rows.push(
                (0..seg)
                    .map(|j| {
                        let a = angle(j);
                        let (r, h) = f(a);
                        DVec3::new(a.sin() * r, h, a.cos() * r)
                    })
                    .collect(),
            );
            kinds.push(kind);
        };
        row(&|_| (inner, y - 0.04), 0);
        if i + 1 == TERRACES.len() {
            row(&|_| (POOL_R * 0.5, y - 0.045), 0);
        }
        row(&|a| (rim(i, a) - 0.34, y - 0.04), 0);
        row(&|a| (rim(i, a) - 0.17, y + 0.03), 1);
        row(&|a| (rim(i, a) - 0.03, y - 0.02), 1);
        // Folds: the stone hangs in drapes, thicker and thinner round the rim.
        let fold = move |a: f64| 0.05 * (a * (11.0 + i as f64 * 3.0)).sin();
        row(&|a| (rim(i, a) + 0.08 + fold(a), y - 0.38 * drop), 2);
        row(&|a| (rim(i, a) + 0.03 + fold(a) * 1.4, y - 0.78 * drop), 2);
        if i == 0 {
            row(&|a| (rim(i, a) + 0.16 + fold(a), below - 0.3), 3);
            row(&|a| (rim(i, a) + 0.4, below - ROOT), 3);
        } else {
            row(&|a| (rim(i, a) - 0.08, below - 0.06), 2);
        }
        let kinds = kinds;
        b.grid(&rows, true, true, |r, j| {
            let a = angle(j);
            // Streaks of mineral run down the folds; the basins are
            // tinged by the water.
            let streak = (0.5 + 0.5 * wobble(a * 2.0, 3.0 + i as f64)).powi(2);
            let color = match kinds[r] {
                0 => mix(cream, pale_blue, 0.55),
                1 => mix(cream, (250, 247, 236), 0.6),
                2 => mix(cream, ochre, streak * 0.75),
                _ => mix(ochre, (150, 132, 104), 0.5 + 0.4 * streak),
            };
            Some((Surf::Plain, color))
        });
    }
    b.finish()
}

/// The light in the spring: the pool on the top terrace and the thin
/// films in the basins below, as one mesh for [`Materials::pool`]. Vertex
/// colour: how bright (white the pool, grey the films).
pub fn spring_pool() -> Mesh {
    let seg = 64;
    let mut b = MeshBuilder::new();
    b.flat = true;
    b.surf = Surf::Glow;
    let ring = |r: f64, y: f64| -> Vec<DVec3> {
        (0..seg)
            .map(|j| {
                let a = TAU * j as f64 / seg as f64;
                DVec3::new(a.sin() * r, y, a.cos() * r)
            })
            .collect()
    };
    let top = TERRACES[3].1;
    let rows = vec![
        ring(0.01, top - 0.004),
        ring(POOL_R * 0.5, top - 0.004),
        ring(TERRACES[3].0 - 0.3, top - 0.004),
    ];
    b.grid(&rows, true, true, |_, _| {
        Some((Surf::Glow, Color::rgb(255, 255, 255)))
    });
    for i in 0..3 {
        let y = TERRACES[i].1 - 0.012;
        let rows = vec![
            ring(TERRACES[i + 1].0 + 0.02, y),
            ring(TERRACES[i].0 - 0.36, y),
        ];
        let v = 70 + 35 * i as u8;
        b.grid(&rows, true, true, |_, _| {
            Some((Surf::Glow, Color::rgb(v, v, v)))
        });
    }
    b.finish()
}

/// One crystal, a unit tall and a unit in radius at its foot, standing on
/// the origin: a six-sided shaft that narrows a little and ends in a
/// point, with hard edges. Placed with a stretch for its size and a pitch
/// for its lean ([`crystal_place`]), for [`Materials::crystal`].
pub fn crystal() -> Mesh {
    let mut b = MeshBuilder::new();
    b.flat = true;
    let col = Color::rgb(214, 246, 255);
    let ring = |r: f64, y: f64, turn: f64| -> [DVec3; 6] {
        std::array::from_fn(|k| {
            let a = TAU * (k as f64 + turn) / 6.0;
            DVec3::new(a.sin() * r, y, a.cos() * r)
        })
    };
    // Sunk a little into what it stands on; a shoulder, then the point,
    // off-centre as grown crystals are.
    let (foot, waist, shoulder) = (
        ring(1.0, -0.12, 0.0),
        ring(0.92, 0.45, 0.0),
        ring(0.8, 0.76, 0.0),
    );
    let tip = DVec3::new(0.12, 1.0, -0.08);
    for k in 0..6 {
        let n = (k + 1) % 6;
        let out = (foot[k] + foot[n]) * DVec3::new(1.0, 0.0, 1.0);
        b.quad([foot[k], foot[n], waist[n], waist[k]], out, col);
        b.quad([waist[k], waist[n], shoulder[n], shoulder[k]], out, col);
        b.tri([shoulder[k], shoulder[n], tip], out + DVec3::Y, col);
    }
    b.finish()
}

/// Where and how a crystal of `c` stands, for [`crystal`]: `origin` is the
/// spring's middle on the ground and `yaw` its `dir` (degrees).
pub fn crystal_place(
    s: &hw_gen::urban::sights::AetherSpring,
    c: &hw_gen::urban::sights::Crystal,
) -> Place {
    let at = s.to_plan(c.at);
    let lean = s.to_plan(c.lean) - s.pos;
    let tilt = (lean.length() / c.height).atan().to_degrees() as f32;
    let long = (c.height * c.height + lean.length_squared()).sqrt() as f32;
    Place::new(at.x as f32, (s.base + c.foot) as f32, at.y as f32)
        .with_yaw_deg(lean.x.atan2(lean.y).to_degrees() as f32)
        .with_pitch_deg(tilt)
        .with_stretch(Vec3::new(c.r as f32, long, c.r as f32))
}

// ---- The waystones ----

/// How many shapes of standing stone there are.
pub const STONE_SHAPES: usize = 3;

/// The cross-section of a standing stone at height `t` (0 foot, 1 top) of
/// shape `shape`: half its width and thickness as shares of the foot's,
/// and how far its middle has wandered.
fn stone_section(shape: usize, t: f64) -> (f64, f64, DVec2) {
    let k = shape as f64;
    // Narrower upwards, with a shoulder, and a top that is not level.
    let taper = 1.0 - 0.16 * t - 0.2 * ((t - 0.82).max(0.0) / 0.18).powi(2);
    let waist = 1.0 + 0.05 * (t * 5.0 + k * 2.0).sin();
    let drift = DVec2::new(0.06 * (t * 2.6 + k).sin(), 0.04 * (t * 3.1 + k * 1.7).cos());
    (
        taper * waist,
        taper * (1.0 + 0.06 * (t * 4.0 + k).cos()),
        drift,
    )
}

/// A standing stone of `shape`, a unit tall and with a foot of half
/// width and half thickness one: a weathered slab, broader than deep,
/// with a rough head. It stands on the origin (and a third of its height
/// into the ground); its broad face looks along +z.
pub fn monolith(shape: usize, detail: Detail) -> Mesh {
    let around = detail.around(28).max(8);
    let rows_n = match detail {
        Detail::Near => 14,
        Detail::Mid => 8,
        Detail::Far => 4,
    };
    let mut b = MeshBuilder::new();
    b.flat = true;
    b.mottle = 0.16;
    let stone = (124, 118, 108);
    let lichen = (150, 148, 84);
    let dark = (78, 76, 72);
    let point = |t: f64, j: usize| -> DVec3 {
        let (w, d, drift) = stone_section(shape, t.max(0.0));
        let a = TAU * j as f64 / around as f64;
        // A slab with rounded corners (a superellipse), chipped a little.
        let (s, c) = a.sin_cos();
        let e = 0.42;
        let chip = 1.0 + 0.035 * wobble(a + t * 4.0, shape as f64 * 3.0 + (t * 6.0).floor());
        let x = s.signum() * s.abs().powf(e) * w * chip + drift.x;
        let z = c.signum() * c.abs().powf(e) * d * chip + drift.y;
        // The head is higher at one end.
        let y = if t >= 1.0 {
            1.0 + 0.05 * (a * 2.0 + shape as f64).sin() * (1.0 - s.abs() * 0.4)
        } else {
            t
        };
        DVec3::new(x, y, z)
    };
    let mut rows: Vec<Vec<DVec3>> = Vec::new();
    for r in 0..=rows_n {
        let t = -0.3 + 1.3 * r as f64 / rows_n as f64;
        rows.push((0..around).map(|j| point(t, j)).collect());
    }
    // The head: rounded in to a point's worth of stone.
    rows.push(
        (0..around)
            .map(|j| {
                let p = point(1.0, j);
                DVec3::new(p.x * 0.55, p.y + 0.035, p.z * 0.55)
            })
            .collect(),
    );
    rows.push((0..around).map(|_| DVec3::new(0.02, 1.07, 0.0)).collect());
    let n = rows.len();
    b.grid(&rows, true, false, |r, j| {
        let t = r as f64 / n as f64;
        let a = TAU * j as f64 / around as f64;
        // Lichen on the weather side and the head, damp dark at the foot.
        let patch = 0.5 + 0.5 * wobble(a * 1.3 + t * 5.0, 7.0 + shape as f64);
        let color = if t < 0.3 {
            mix(dark, stone, (t / 0.3).powi(2))
        } else if patch > 0.72 && (a.sin() > 0.1 || t > 0.8) {
            mix(stone, lichen, (patch - 0.72) * 3.0)
        } else {
            mix(stone, (150, 143, 130), patch * 0.5)
        };
        Some((Surf::Plain, color))
    });
    b.finish()
}

/// A stroke of a glyph: a ribbon along `pts` (x across the stone's face,
/// y up it, both in the stone's own unit measure), `w` wide, lying on
/// the face of the stone of `shape`.
fn stroke(b: &mut MeshBuilder, shape: usize, pts: &[DVec2], w: f64) {
    let on_face = |p: DVec2| -> DVec3 {
        let (_, d, drift) = stone_section(shape, p.y);
        // The face is flat across its middle; the glyph stands a little proud.
        let x = p.x.clamp(-0.6, 0.6);
        let flat = (1.0 - (x.abs() / 1.0).powf(1.0 / 0.42)).max(0.0).powf(0.42);
        DVec3::new(x + drift.x, p.y, d * flat + drift.y + 0.07)
    };
    for s in pts.windows(2) {
        let dir = (s[1] - s[0]).normalize_or(DVec2::Y);
        let side = dir.perp() * (w * 0.5);
        b.quad(
            [
                on_face(s[0] - side),
                on_face(s[0] + side),
                on_face(s[1] + side),
                on_face(s[1] - side),
            ],
            DVec3::Z,
            c3(AETHER),
        );
    }
}

/// An arc of a circle as points.
fn arc(c: DVec2, r: DVec2, from: f64, to: f64, n: usize) -> Vec<DVec2> {
    (0..=n)
        .map(|k| {
            let a = from + (to - from) * k as f64 / n as f64;
            c + DVec2::new(a.sin() * r.x, a.cos() * r.y)
        })
        .collect()
}

/// The glyphs cut into the face of a standing stone of `shape`, glowing:
/// four signs one above the other (a ring, a wave, a chevron with a dot,
/// a spiral: which and in what order goes by the shape), for
/// [`Materials::glyph`]. In the stone's own unit measure, like [`monolith`].
pub fn glyphs(shape: usize) -> Mesh {
    let mut b = MeshBuilder::new();
    b.flat = true;
    b.surf = Surf::Glow;
    // A stone is some four metres tall and one and a half wide: a unit
    // up is four metres, a unit across three quarters of one. Signs are
    // drawn round, so their height is squeezed.
    let (rx, ry, w) = (0.34, 0.066, 0.075);
    for k in 0..4 {
        let c = DVec2::new(0.0, 0.3 + 0.16 * k as f64);
        match (k + shape) % 4 {
            0 => {
                // A ring with a bar through it.
                stroke(
                    &mut b,
                    shape,
                    &arc(c, DVec2::new(rx, ry), 0.0, TAU, 20),
                    w * 0.3,
                );
                stroke(
                    &mut b,
                    shape,
                    &[
                        c - DVec2::new(rx * 1.45, 0.0),
                        c + DVec2::new(rx * 1.45, 0.0),
                    ],
                    w * 0.22,
                );
            }
            1 => {
                // Two waves.
                for dy in [-0.4, 0.4] {
                    let pts: Vec<DVec2> = (0..=16)
                        .map(|i| {
                            let t = i as f64 / 16.0;
                            c + DVec2::new(
                                (t - 0.5) * rx * 2.6,
                                ry * (dy + 0.38 * (t * TAU * 1.5).sin()),
                            )
                        })
                        .collect();
                    stroke(&mut b, shape, &pts, w * 0.26);
                }
            }
            2 => {
                // A chevron over a dot.
                stroke(
                    &mut b,
                    shape,
                    &[
                        c + DVec2::new(-rx * 1.2, -ry * 0.7),
                        c + DVec2::new(0.0, ry * 0.9),
                        c + DVec2::new(rx * 1.2, -ry * 0.7),
                    ],
                    w * 0.28,
                );
                stroke(
                    &mut b,
                    shape,
                    &arc(
                        c - DVec2::new(0.0, ry * 0.55),
                        DVec2::new(rx, ry) * 0.2,
                        0.0,
                        TAU,
                        8,
                    ),
                    w * 0.3,
                );
            }
            _ => {
                // A spiral.
                let pts: Vec<DVec2> = (0..=30)
                    .map(|i| {
                        let t = i as f64 / 30.0;
                        let a = t * TAU * 2.2;
                        c + DVec2::new(a.sin() * rx, a.cos() * ry) * (0.15 + 0.95 * t)
                    })
                    .collect();
                stroke(&mut b, shape, &pts, w * 0.26);
            }
        }
    }
    b.finish()
}

/// The altar stone in the ring's middle: a low round table, worn at the
/// edge, on a root that runs into the ground.
pub fn altar(detail: Detail) -> Mesh {
    let seg = detail.around(48);
    let mut b = MeshBuilder::new();
    b.flat = true;
    b.mottle = 0.14;
    let (r, h) = ALTAR;
    let profile = [
        (0.01, h),
        (r * 0.62, h),
        (r * 0.9, h - 0.012),
        (r * 0.985, h - 0.05),
        (r, h - 0.11),
        (r * 0.97, h * 0.3),
        (r * 1.02, 0.0),
        (r * 1.1, -ROOT),
    ];
    let rows: Vec<Vec<DVec3>> = profile
        .iter()
        .map(|&(pr, y)| {
            (0..seg)
                .map(|j| {
                    let a = TAU * j as f64 / seg as f64;
                    let pr = pr * (1.0 + 0.012 * wobble(a, y * 9.0));
                    DVec3::new(a.sin() * pr, y, a.cos() * pr)
                })
                .collect()
        })
        .collect();
    b.grid(&rows, true, true, |r, j| {
        let a = TAU * j as f64 / seg as f64;
        let patch = 0.5 + 0.5 * wobble(a * 2.0, r as f64);
        Some((
            Surf::Plain,
            if r >= 4 {
                mix((92, 88, 82), (124, 118, 108), patch)
            } else {
                mix((136, 130, 118), (158, 152, 138), patch)
            },
        ))
    });
    b.finish()
}

/// The sign on the altar's top, glowing: a spiral of three arms round a
/// ring, for [`Materials::glyph`].
pub fn altar_sign() -> Mesh {
    let mut b = MeshBuilder::new();
    b.flat = true;
    b.surf = Surf::Glow;
    let y = ALTAR.1 + 0.006;
    let mut ribbon = |pts: &[DVec2], w: f64| {
        for s in pts.windows(2) {
            let side = (s[1] - s[0]).normalize_or(DVec2::Y).perp() * (w * 0.5);
            let p = |q: DVec2| DVec3::new(q.x, y, q.y);
            b.quad(
                [
                    p(s[0] - side),
                    p(s[0] + side),
                    p(s[1] + side),
                    p(s[1] - side),
                ],
                DVec3::Y,
                c3(AETHER),
            );
        }
    };
    ribbon(&arc(DVec2::ZERO, DVec2::splat(0.2), 0.0, TAU, 24), 0.035);
    for arm in 0..3 {
        let pts: Vec<DVec2> = (0..=28)
            .map(|i| {
                let t = i as f64 / 28.0;
                let a = TAU * arm as f64 / 3.0 + t * TAU * 0.8;
                DVec2::new(a.sin(), a.cos()) * (0.24 + 0.52 * t)
            })
            .collect();
        ribbon(&pts, 0.04);
    }
    ribbon(&arc(DVec2::ZERO, DVec2::splat(0.86), 0.0, TAU, 40), 0.03);
    b.finish()
}

// ---- Wisps ----

/// A unit ball, smooth (a wisp's heart and its halo: placed with a scale).
pub fn ball(around: usize, down: usize) -> Mesh {
    let mut b = MeshBuilder::new();
    b.flat = true;
    b.surf = Surf::Glow;
    let rows: Vec<Vec<DVec3>> = (0..=down)
        .map(|i| {
            let v = std::f64::consts::PI * i as f64 / down as f64;
            (0..around)
                .map(|j| {
                    let u = TAU * j as f64 / around as f64;
                    DVec3::new(v.sin() * u.sin(), v.cos(), v.sin() * u.cos())
                })
                .collect()
        })
        .collect();
    b.grid(&rows, true, true, |_, _| {
        Some((Surf::Glow, Color::rgb(235, 252, 255)))
    });
    b.finish()
}

// ---- Gates ----

/// A ring in the plane across +z, round the origin: `radius` to the
/// middle of its tube, `thick` the tube's radius.
fn torus(
    b: &mut MeshBuilder,
    radius: f64,
    thick: f64,
    around: usize,
    tube: usize,
    level: bool,
    color: Color,
) {
    let rows: Vec<Vec<DVec3>> = (0..around)
        .map(|i| {
            let a = TAU * i as f64 / around as f64;
            (0..tube)
                .map(|j| {
                    let t = TAU * j as f64 / tube as f64;
                    let r = radius + thick * t.cos();
                    let (x, y, z) = (a.sin() * r, a.cos() * r, thick * t.sin());
                    if level {
                        DVec3::new(x, z, y)
                    } else {
                        DVec3::new(x, y, z)
                    }
                })
                .collect()
        })
        .collect();
    let mut rows = rows;
    rows.push(rows[0].clone());
    let surf = b.surf;
    // A level ring is the standing one with two axes swapped: mirrored.
    b.grid(&rows, true, level, |_, _| Some((surf, color)));
}

/// A flight ring: a slim hoop of light [`RING_RADIUS`] in radius, standing
/// across +z (the way through), with eight small fins like a compass
/// rose. For [`Materials::gate`]; its instance tint is its colour.
pub fn flight_ring() -> Mesh {
    let mut b = MeshBuilder::new();
    b.flat = true;
    b.surf = Surf::Glow;
    let white = Color::rgb(255, 255, 255);
    torus(&mut b, RING_RADIUS, 0.22, 96, 10, false, white);
    for k in 0..8 {
        let a = TAU * k as f64 / 8.0;
        let d = DVec3::new(a.sin(), a.cos(), 0.0);
        let side = DVec3::new(a.cos(), -a.sin(), 0.0);
        let long = if k % 2 == 0 { 1.5 } else { 0.8 };
        let (base, tip) = (d * (RING_RADIUS + 0.15), d * (RING_RADIUS + 0.15 + long));
        for z in [-0.06, 0.06] {
            b.tri(
                [
                    base - side * 0.32 + DVec3::Z * z,
                    base + side * 0.32 + DVec3::Z * z,
                    tip + DVec3::Z * z,
                ],
                DVec3::Z * z,
                white,
            );
        }
    }
    b.finish()
}

/// What marks a course's first gate: a starburst of long and short rays
/// round the ring, golden-age style, in gold. Across +z like the ring.
pub fn start_burst(radius: f64) -> Mesh {
    let mut b = MeshBuilder::new();
    b.flat = true;
    b.surf = Surf::Glow;
    let gold = Color::rgb(255, 214, 110);
    let n = 24;
    for k in 0..n {
        let a = TAU * (k as f64 + 0.5) / n as f64;
        let d = DVec3::new(a.sin(), a.cos(), 0.0);
        let side = DVec3::new(a.cos(), -a.sin(), 0.0);
        let long = radius * if k % 3 == 0 { 0.34 } else { 0.16 };
        let (from, to) = (d * (radius * 1.09), d * (radius * 1.09 + long));
        let w = radius * 0.022;
        for z in [-0.04, 0.04] {
            b.quad(
                [
                    from - side * w + DVec3::Z * z,
                    from + side * w + DVec3::Z * z,
                    to + side * (w * 0.2) + DVec3::Z * z,
                    to - side * (w * 0.2) + DVec3::Z * z,
                ],
                DVec3::Z * z,
                gold,
            );
        }
        // A bead on every long ray's tip.
        if k % 3 == 0 {
            let c = to + d * (radius * 0.03);
            let r = radius * 0.028;
            for z in [-0.04, 0.04] {
                b.quad(
                    [
                        c - side * r + DVec3::Z * z,
                        c + d * r + DVec3::Z * z,
                        c + side * r + DVec3::Z * z,
                        c - d * r + DVec3::Z * z,
                    ],
                    DVec3::Z * z,
                    gold,
                );
            }
        }
    }
    b.finish()
}

/// What marks a course's last gate: a band of black and white squares
/// round the ring (a finish flag bent into a hoop). Not glowing. Across
/// +z, or lying level for a rooftop hoop.
pub fn finish_band(radius: f64, level: bool) -> Mesh {
    let mut b = MeshBuilder::new();
    b.flat = true;
    let n = 40;
    let (r0, r1) = (radius * 1.05, radius * 1.13);
    let mid = (r0 + r1) * 0.5;
    let p = |a: f64, r: f64, z: f64| {
        let v = DVec3::new(a.sin() * r, a.cos() * r, z);
        if level {
            DVec3::new(v.x, v.z, v.y)
        } else {
            v
        }
    };
    let face = if level { DVec3::Y } else { DVec3::Z };
    for k in 0..n {
        let (a0, a1) = (TAU * k as f64 / n as f64, TAU * (k + 1) as f64 / n as f64);
        for (band, (ra, rb)) in [(r0, mid), (mid, r1)].into_iter().enumerate() {
            let white = (k + band) % 2 == 0;
            let c = if white {
                Color::rgb(240, 240, 236)
            } else {
                Color::rgb(24, 24, 28)
            };
            for z in [-0.05, 0.05] {
                b.quad(
                    [p(a0, ra, z), p(a1, ra, z), p(a1, rb, z), p(a0, rb, z)],
                    face * z,
                    c,
                );
            }
        }
    }
    b.finish()
}

/// A rooftop hoop: a ring of light [`HOOP_RADIUS`] in radius lying level
/// round the origin. For [`Materials::gate`].
pub fn roof_hoop() -> Mesh {
    let mut b = MeshBuilder::new();
    b.flat = true;
    b.surf = Surf::Glow;
    torus(
        &mut b,
        HOOP_RADIUS,
        0.13,
        64,
        8,
        true,
        Color::rgb(255, 255, 255),
    );
    b.finish()
}

/// The shaft of light under a rooftop hoop: a tube a unit tall hanging
/// from the origin (stretched to reach the roof), narrower than the hoop.
/// For [`Materials::shaft`].
pub fn shaft() -> Mesh {
    let mut b = MeshBuilder::new();
    b.flat = true;
    b.surf = Surf::Glow;
    let seg = 24;
    let rows: Vec<Vec<DVec3>> = [0.0, -0.5, -1.0]
        .iter()
        .map(|&y| {
            (0..seg)
                .map(|j| {
                    let a = TAU * j as f64 / seg as f64;
                    DVec3::new(a.sin(), y, a.cos())
                })
                .collect()
        })
        .collect();
    b.grid(&rows, true, true, |_, _| {
        Some((Surf::Glow, Color::rgb(255, 255, 255)))
    });
    b.finish()
}

// ---- Materials ----

/// The pool: slow caustics of light drifting over a bright bed, brighter
/// in the middle, breathing. The vertex colour scales it (the films in
/// the lower basins are fainter). `params[0].x`: strength.
pub const POOL: &str = r#"
fn material(m: MaterialIn, base: MaterialOut) -> MaterialOut {
    var o = base;
    let p = m.object_pos.xz;
    let t = m.time;
    let r = length(p);
    let a = sin(p.x * 2.6 + sin(p.y * 1.9 + t * 0.5) * 1.6 + t * 0.7);
    let b = sin(p.y * 2.9 - t * 0.6 + sin(p.x * 2.1 - t * 0.4) * 1.5);
    let caustic = pow(clamp(0.5 + 0.5 * a * b, 0.0, 1.0), 2.5);
    let rings = pow(0.5 + 0.5 * sin(r * 5.0 - t * 1.3), 8.0) * 0.5;
    let breath = 0.85 + 0.15 * sin(t * 0.9);
    let core = 1.0 - smoothstep(0.0, 2.6, r) * 0.55;
    let k = base.color.r;
    let deep = vec3<f32>(0.10, 0.62, 0.86);
    let bright = vec3<f32>(0.62, 0.98, 1.0);
    let light = mix(deep, bright, clamp(caustic + rings, 0.0, 1.0)) * core * breath;
    o.color = vec4<f32>(light * 0.6 * k, 1.0);
    o.roughness = 0.08;
    o.metallic = 0.0;
    o.emission = light * (0.35 + 1.3 * caustic + 0.6 * rings) * k * m.params[0].x;
    return o;
}
"#;

/// A crystal: clear at the foot and alight towards the point, with veins
/// of light that creep up it and a bright edge where you look along a
/// face. `params[0].x`: strength.
pub const CRYSTAL: &str = r#"
fn material(m: MaterialIn, base: MaterialOut) -> MaterialOut {
    var o = base;
    let p = m.object_pos;
    let t = m.time;
    let n = normalize(m.normal);
    let rim = pow(1.0 - clamp(abs(dot(n, m.view_dir)), 0.0, 1.0), 2.5);
    let up = clamp(p.y, 0.0, 1.0);
    let vein = pow(0.5 + 0.5 * sin(p.y * 9.0 - t * 1.1 + sin(p.x * 5.0 + p.z * 4.0) * 2.0), 6.0);
    let breath = 0.8 + 0.2 * sin(t * 0.9 + m.world_pos.x * 0.3 + m.world_pos.z * 0.2);
    let body = mix(vec3<f32>(0.03, 0.14, 0.22), vec3<f32>(0.16, 0.46, 0.60), up);
    o.color = vec4<f32>(body, 1.0);
    o.roughness = 0.12;
    o.metallic = 0.0;
    let glow = (0.08 + 0.5 * up * up + 0.7 * vein * up + 0.5 * rim) * breath;
    o.emission = vec3<f32>(0.30, 0.86, 1.0) * glow * m.params[0].x;
    return o;
}
"#;

/// A glyph cut into stone: a steady low light, with a brighter pulse that
/// climbs the stone now and then. The instance tint scales it (bright
/// while the stones sing). `params[0].x`: strength.
pub const GLYPH: &str = r#"
fn material(m: MaterialIn, base: MaterialOut) -> MaterialOut {
    var o = base;
    let t = m.time;
    let h = m.object_pos.y + length(m.object_pos.xz) * 0.4;
    let pulse = pow(0.5 + 0.5 * sin(h * 6.0 - t * 1.6), 10.0);
    let flick = 0.9 + 0.1 * sin(t * 7.0 + h * 11.0);
    let lit = (0.55 + 2.2 * pulse) * flick;
    o.color = vec4<f32>(base.color.rgb * 0.5, 1.0);
    o.roughness = 0.5;
    o.emission = base.color.rgb * lit * m.params[0].x;
    return o;
}
"#;

/// A wisp's halo: a ball of light that is bright in the middle and fades
/// to nothing at its edge, so it has no outline. The instance tint is its
/// colour and strength. `params[0].x`: strength.
pub const HALO: &str = r#"
fn material(m: MaterialIn, base: MaterialOut) -> MaterialOut {
    var o = base;
    let n = normalize(m.normal);
    let facing = clamp(abs(dot(n, m.view_dir)), 0.0, 1.0);
    let soft = pow(facing, 3.2);
    let shimmer = 0.85 + 0.15 * sin(m.time * 5.0 + m.world_pos.y * 3.0 + m.world_pos.x);
    o.color = vec4<f32>(base.color.rgb, clamp(soft * 0.4, 0.0, 0.4));
    o.roughness = 1.0;
    o.emission = base.color.rgb * soft * 1.5 * shimmer * m.params[0].x;
    return o;
}
"#;

/// A gate's ring: chevrons of light that race round it. The instance tint
/// is its colour and strength (gold for a start, bright for the next
/// gate, dim for those after). `params[0].x`: strength.
pub const GATE: &str = r#"
fn material(m: MaterialIn, base: MaterialOut) -> MaterialOut {
    var o = base;
    let p = m.object_pos;
    // A standing ring lies in x and y, a level hoop in x and z.
    let flat = step(abs(p.y), 0.5) * step(1.0, abs(p.z));
    let a = mix(atan2(p.x, p.y), atan2(p.x, p.z), flat);
    let run = pow(0.5 + 0.5 * sin(a * 12.0 - m.time * 5.0), 3.0);
    let lit = 0.7 + 1.6 * run;
    o.color = vec4<f32>(base.color.rgb * 0.6, 1.0);
    o.roughness = 0.3;
    o.emission = base.color.rgb * lit * m.params[0].x;
    return o;
}
"#;

/// The shaft under a rooftop hoop: bands of light sinking to the roof,
/// clear in the middle and bright at the edge, fading towards the roof.
/// The instance tint is its colour and strength. `params[0].x`: strength.
pub const SHAFT: &str = r#"
fn material(m: MaterialIn, base: MaterialOut) -> MaterialOut {
    var o = base;
    let n = normalize(m.normal);
    let rim = pow(1.0 - clamp(abs(dot(n, m.view_dir)), 0.0, 1.0), 1.6);
    let down = clamp(-m.object_pos.y, 0.0, 1.0);
    let band = 0.6 + 0.4 * sin(m.world_pos.y * 3.0 + m.time * 2.5);
    let k = (0.12 + 0.5 * rim) * (1.0 - down * 0.8) * band;
    o.color = vec4<f32>(base.color.rgb, clamp(k, 0.0, 0.6));
    o.roughness = 1.0;
    o.emission = base.color.rgb * k * 2.0 * m.params[0].x;
    return o;
}
"#;

/// A wisp's heart: a small ball that is all light, flickering a little.
/// The instance tint is its colour. `params[0].x`: strength.
pub const HEART: &str = r#"
fn material(m: MaterialIn, base: MaterialOut) -> MaterialOut {
    var o = base;
    let flick = 0.8 + 0.2 * sin(m.time * 9.0 + m.world_pos.x * 2.0 + m.world_pos.z);
    o.color = vec4<f32>(base.color.rgb, 1.0);
    o.roughness = 1.0;
    o.emission = (base.color.rgb * 4.0 + vec3<f32>(1.2, 1.2, 1.2)) * flick * m.params[0].x;
    return o;
}
"#;

/// The sights' custom materials, once added to the world.
#[derive(Clone, Copy, Debug)]
pub struct Materials {
    pub pool: CustomMaterialId,
    pub crystal: CustomMaterialId,
    pub glyph: CustomMaterialId,
    pub halo: CustomMaterialId,
    pub heart: CustomMaterialId,
    pub gate: CustomMaterialId,
    pub shaft: CustomMaterialId,
}

/// Set once by [`install_materials`] on the main thread, read there after.
static MATERIALS: OnceLock<Materials> = OnceLock::new();

/// Each material's name, code and whether it is see-through (for the
/// check that every one compiles, and for [`install_materials`]).
pub fn material_sources() -> [(&'static str, &'static str, bool); 7] {
    [
        ("aether pool", POOL, false),
        ("aether crystal", CRYSTAL, false),
        ("aether glyph", GLYPH, false),
        ("wisp halo", HALO, true),
        ("course gate", GATE, false),
        ("hoop shaft", SHAFT, true),
        ("wisp heart", HEART, false),
    ]
}

/// Adds the sights' materials to the world (once; Engine builds a
/// material's pipelines when it first draws it).
pub fn install_materials(world: &mut World) -> EngineResult<Materials> {
    if let Some(m) = MATERIALS.get() {
        return Ok(*m);
    }
    let mut ids = Vec::new();
    for (name, code, clear) in material_sources() {
        let mut m = CustomMaterial::new(name, code).param(0, [1.0, 0.0, 0.0, 0.0]);
        if clear {
            m = m.see_through();
        }
        ids.push(world.add_material(m)?);
    }
    let m = Materials {
        pool: ids[0],
        crystal: ids[1],
        glyph: ids[2],
        halo: ids[3],
        gate: ids[4],
        shaft: ids[5],
        heart: ids[6],
    };
    let _ = MATERIALS.set(m);
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_models_are_fine_close_up_and_light_far_off() {
        let near = spring_rock(Detail::Near).face_count();
        let far = spring_rock(Detail::Far).face_count();
        assert!(near > 2500, "the spring close up: {near} faces");
        assert!(far < 700 && far * 5 < near, "the spring far off: {far}");
        for shape in 0..STONE_SHAPES {
            let (near, far) = (
                monolith(shape, Detail::Near).face_count(),
                monolith(shape, Detail::Far).face_count(),
            );
            assert!(near > 350 && far < 80, "a stone: {near} and {far}");
            assert!(glyphs(shape).face_count() > 40);
        }
        assert!(crystal().face_count() >= 18);
        assert!(flight_ring().face_count() > 900, "a round ring");
        assert!(roof_hoop().face_count() > 400);
        assert!(spring_pool().face_count() > 100 && altar_sign().face_count() > 60);
    }

    /// The WGSL of every material compiles inside Engine's lit shader.
    #[test]
    fn the_materials_compile() {
        for (name, code, _) in material_sources() {
            CustomMaterial::new(name, code).check().expect(name);
        }
    }

    #[test]
    fn a_crystal_leans_the_way_its_spring_says() {
        use hw_gen::urban::sights::AetherSpring;
        let s = AetherSpring {
            pos: DVec2::new(100.0, -40.0),
            base: 12.0,
            dir: DVec2::new(0.6, 0.8),
            tier: 1,
            seed: hw_core::Seed(9),
        };
        for c in s.crystals() {
            let place = crystal_place(&s, &c);
            let tip = place.to_matrix().transform_point3(Vec3::Y);
            let want = s.to_plan(c.at + c.lean);
            let got = DVec2::new(tip.x as f64, tip.z as f64);
            assert!(got.distance(want) < 0.02, "tip at {got:?}, wanted {want:?}");
            assert!((tip.y as f64 - (s.base + c.foot + c.height)).abs() < 0.02);
        }
    }
}
