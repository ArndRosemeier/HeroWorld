//! The atomic plant: its model, and the plant alive.
//!
//! The model ([`plant`]) is a 1950s showpiece of the atomic age: a white
//! reactor drum under a ribbed dome with a ring of blue light round its
//! foot and a glass lantern on its crown, two concrete cooling towers on
//! slanting legs, a streamlined turbine hall with a rounded nose, ribbon
//! windows and fins, steam mains on trestles, a striped stack and, at the
//! gate, an atom on a pylon beside a saucer-roofed gatehouse.
//!
//! Alive ([`AtomView`]): the reactor's light breathes and now and then
//! surges, electrons race round the atom at the gate, steam rolls up out
//! of both towers, the stack's beacon blinks, and every half minute or so
//! a relief valve on the hall roof blows off with a roar. What the
//! soundscape plays comes from [`Heard`].
//!
//! The glowing parts are prebuilt instanced entities (see
//! [`crate::teslaview`]): brightness rides on the instance tint.

use crate::carmesh::{inside, lathe, tube};
use crate::citymesh::{MeshBuilder, Surf};
use crate::landmarkview::strut;
use crate::skins::Tex;
use engine::prelude::*;
use engine::texgen::{FacadeLayer, RoofLayer};
use glam::{DVec2, DVec3, Vec3};
use hw_gen::urban::atomic::*;
use hw_gen::urban::landmarks::{Landmark, LandmarkKind};
use hw_gen::urban::world::TownWorld;
use std::f64::consts::{PI, TAU};

/// The reactor's light: Cherenkov blue.
pub const GLOW: (u8, u8, u8) = (90, 175, 255);
/// The atom's nucleus: a hot rose.
const NUCLEUS: (u8, u8, u8) = (255, 120, 150);

fn col(t: (u8, u8, u8)) -> Color {
    rgb(t.0, t.1, t.2)
}
fn cream() -> Color {
    rgb(238, 233, 218)
}
fn aqua() -> Color {
    rgb(96, 178, 172)
}
fn red() -> Color {
    rgb(196, 52, 44)
}
fn chrome() -> Color {
    rgb(206, 212, 220)
}
fn concrete() -> Color {
    rgb(198, 194, 184)
}

fn v3(p: DVec2, y: f64) -> DVec3 {
    DVec3::new(p.x, y, p.y)
}

/// Draws `f` with surface `s` and texture `t`.
fn with(b: &mut MeshBuilder, s: Surf, t: Option<Tex>, f: impl FnOnce(&mut MeshBuilder)) {
    let (ss, st) = (b.surf, b.tex);
    b.surf = s;
    b.tex = t;
    f(b);
    b.surf = ss;
    b.tex = st;
}

/// A smooth ball.
fn ball(b: &mut MeshBuilder, c: DVec3, r: f64, seg: usize, surf: Surf, color: Color) {
    let rings = (seg / 2).max(4);
    let prof: Vec<(f64, f64)> = (0..=rings)
        .map(|i| {
            let a = PI * i as f64 / rings as f64;
            (r * a.sin(), r * (1.0 - a.cos()))
        })
        .collect();
    lathe(b, c - DVec3::Y * r, DVec3::Y, &prof, seg, |_| {
        Some((surf, color))
    });
}

/// A closed ring of tube: centre `c`, in the plane spanned by `e1`, `e2`
/// (unit, at right angles), radius `big`, tube radius `small`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn torus(
    b: &mut MeshBuilder,
    c: DVec3,
    e1: DVec3,
    e2: DVec3,
    big: f64,
    small: f64,
    seg: (usize, usize),
    paint: (Surf, Color),
) {
    let n = e1.cross(e2).normalize();
    let rows: Vec<Vec<DVec3>> = (0..=seg.0)
        .map(|i| {
            let a = TAU * i as f64 / seg.0 as f64;
            let out = e1 * a.cos() + e2 * a.sin();
            (0..seg.1)
                .map(|j| {
                    let t = TAU * j as f64 / seg.1 as f64;
                    c + out * (big + small * t.cos()) + n * (small * t.sin())
                })
                .collect()
        })
        .collect();
    // Normals: away from the tube's own core line, not from the ring's
    // centre (`surface` would point the inner half the wrong way).
    let mut acc = 0.0;
    for (i, row) in rows.iter().enumerate().take(seg.0) {
        let a = TAU * i as f64 / seg.0 as f64;
        let core = c + (e1 * a.cos() + e2 * a.sin()) * big;
        let du = rows[i + 1][0] - row[0];
        let dv = row[1] - row[0];
        acc += du.cross(dv).dot(row[0] - core);
    }
    b.grid(&rows, true, acc >= 0.0, |_, _| Some(paint));
}

/// The atom's three orbits: centre on the pylon, and each orbit's plane
/// (two unit vectors).
pub fn orbits() -> (DVec3, [(DVec3, DVec3); 3]) {
    let c = DVec3::new(PYLON_AT.0, PYLON_H + ATOM_R, PYLON_AT.1);
    let tilt = 62f64.to_radians();
    let planes = [0.0, 1.0, 2.0].map(|k| {
        let yaw = TAU * k / 3.0 + 0.3;
        let e1 = DVec3::new(yaw.cos(), 0.0, yaw.sin());
        let side = DVec3::new(-yaw.sin(), 0.0, yaw.cos());
        let e2 = (side * tilt.cos() + DVec3::Y * tilt.sin()).normalize();
        (e1, e2)
    });
    (c, planes)
}

/// The plant in its own frame (origin on the yard's top, +z the gate).
pub fn plant(b: &mut MeshBuilder, near: bool) {
    yard(b, near);
    reactor(b, near);
    for t in towers() {
        tower(b, t, near);
    }
    hall(b, near);
    mains(b, near);
    stack(b, near);
    pylon(b, near);
    gatehouse(b, near);
}

/// Painted lines on the yard: a walk from the gate to the reactor's door.
fn yard(b: &mut MeshBuilder, near: bool) {
    if !near {
        return;
    }
    let y = 0.012;
    let z0 = REACTOR_Z + REACTOR_R + 3.2;
    for x in [-2.2, 2.2] {
        b.quad(
            [
                DVec3::new(x - 0.08, y, z0),
                DVec3::new(x + 0.08, y, z0),
                DVec3::new(x + 0.08, y, PLANT_PLAZA - 1.0),
                DVec3::new(x - 0.08, y, PLANT_PLAZA - 1.0),
            ],
            DVec3::Y,
            rgb(232, 196, 64),
        );
    }
}

/// The reactor's glowing parts, drawn into the near model and again (a
/// hair larger) as the live entities: the ring round the dome's foot, the
/// band of glass round the drum, and the lantern on the crown.
pub fn reactor_glow(b: &mut MeshBuilder, part: usize, near: bool, live: bool) {
    let c = DVec3::new(0.0, 0.0, REACTOR_Z);
    let grow = if live { 0.03 } else { 0.0 };
    let seg = if near { 96 } else { 24 };
    let glow = (Surf::Glow, col(GLOW));
    match part {
        0 => torus(
            b,
            c + DVec3::Y * (REACTOR_WALL + 0.25),
            DVec3::X,
            DVec3::Z,
            REACTOR_R + 0.25,
            0.32 + grow,
            (seg, if near { 10 } else { 4 }),
            glow,
        ),
        1 => {
            let r = REACTOR_R + 0.02 + grow;
            let prof = [(r, 6.5), (r, 7.7)];
            lathe(b, c, DVec3::Y, &prof, seg, |_| Some(glow));
        }
        _ => {
            let y = REACTOR_WALL + REACTOR_R - 0.15;
            let r = CUPOLA_R - 0.25 + grow;
            lathe(
                b,
                c + DVec3::Y * y,
                DVec3::Y,
                &[
                    (0.0, 0.0),
                    (r, 0.0),
                    (r, CUPOLA_H * 0.7),
                    (0.0, CUPOLA_H * 0.7),
                ],
                if near { 32 } else { 10 },
                |_| Some(glow),
            );
        }
    }
}

/// The reactor: a drum with shallow pilasters, a ribbon of blue glass, a
/// ribbed white dome, a glass lantern and a streamlined porch.
fn reactor(b: &mut MeshBuilder, near: bool) {
    let c = DVec3::new(0.0, 0.0, REACTOR_Z);
    let seg = if near { 96 } else { 24 };
    let plaster = near.then_some(Tex::from(FacadeLayer::Plaster));
    // A dark plinth band, then the drum.
    with(
        b,
        Surf::Plain,
        near.then_some(Tex::from(FacadeLayer::Concrete)),
        |b| {
            let prof = [
                (0.0, 0.0),
                (REACTOR_R + 0.35, 0.0),
                (REACTOR_R + 0.35, 0.9),
                (REACTOR_R, 1.0),
            ];
            lathe(b, c, DVec3::Y, &prof, seg, |_| {
                Some((Surf::Plain, rgb(150, 146, 140)))
            });
        },
    );
    with(b, Surf::Plain, plaster, |b| {
        let r = REACTOR_R;
        let prof = [
            (r, 1.0),
            (r, 6.5),
            (r, 7.7),
            (r, REACTOR_WALL - 0.4),
            (r + 0.25, REACTOR_WALL - 0.3),
            (r + 0.25, REACTOR_WALL),
        ];
        lathe(b, c, DVec3::Y, &prof, seg, |k| match k {
            1 => Some((Surf::Glass, rgb(40, 70, 110))),
            3 | 4 => Some((Surf::Plain, aqua())),
            _ => Some((Surf::Plain, cream())),
        });
    });
    reactor_glow(b, 1, near, false);
    if near {
        // Pilasters between the glass, chrome-capped.
        for k in 0..16 {
            let a = TAU * (k as f64 + 0.5) / 16.0;
            let d = DVec3::new(a.sin(), 0.0, a.cos());
            let side = DVec3::new(d.z, 0.0, -d.x);
            // Not across the porch.
            if d.z > 0.93 {
                continue;
            }
            with(b, Surf::Plain, plaster, |b| {
                b.oriented_box(
                    c + d * (REACTOR_R + 0.18) + DVec3::Y * 4.9,
                    side,
                    DVec3::new(0.35, 3.9, 0.22),
                    cream(),
                )
            });
            with(b, Surf::Metal, None, |b| {
                b.oriented_box(
                    c + d * (REACTOR_R + 0.22) + DVec3::Y * 8.75,
                    side,
                    DVec3::new(0.4, 0.12, 0.26),
                    chrome(),
                )
            });
        }
    }
    // The dome: a half sphere, then the lantern's collar.
    let n = if near { 24 } else { 8 };
    let top = REACTOR_R;
    let neck = (top * top - CUPOLA_R * CUPOLA_R).sqrt();
    let prof: Vec<(f64, f64)> = (0..=n)
        .map(|i| {
            let y = neck * (1.0 - (1.0 - i as f64 / n as f64).powi(2));
            (dome_r(y), REACTOR_WALL + y)
        })
        .chain([
            (CUPOLA_R - 0.25, REACTOR_WALL + neck),
            (0.0, REACTOR_WALL + neck),
        ])
        .collect();
    with(b, Surf::Plain, plaster, |b| {
        lathe(b, c, DVec3::Y, &prof, seg, |_| {
            Some((Surf::Plain, rgb(244, 241, 232)))
        })
    });
    reactor_glow(b, 0, near, false);
    // Chrome ribs down the dome.
    let ribs = if near { 16 } else { 0 };
    for k in 0..ribs {
        let a = TAU * k as f64 / ribs as f64;
        let (s, cs) = a.sin_cos();
        let path: Vec<DVec3> = (0..=16)
            .map(|i| {
                let y = (neck - 0.2) * i as f64 / 16.0;
                let r = dome_r(y) + 0.08;
                c + DVec3::new(s * r, REACTOR_WALL + y, cs * r)
            })
            .collect();
        tube(b, &path, (0.11, 0.11), 6, Some((Surf::Metal, chrome())));
    }
    // The lantern: blue glow behind glass, a chrome cap and a needle with a
    // red beacon.
    let ly = REACTOR_WALL + top - 0.15;
    reactor_glow(b, 2, near, false);
    let lseg = if near { 32 } else { 10 };
    lathe(
        b,
        c + DVec3::Y * ly,
        DVec3::Y,
        &[(CUPOLA_R, 0.0), (CUPOLA_R, CUPOLA_H * 0.72)],
        lseg,
        |_| Some((Surf::Glass, rgb(70, 110, 150))),
    );
    let cap: Vec<(f64, f64)> = (0..=8)
        .map(|i| {
            let a = 0.5 * PI * i as f64 / 8.0;
            ((CUPOLA_R + 0.3) * a.cos(), CUPOLA_H * 0.72 + 0.8 * a.sin())
        })
        .collect();
    let mut cap = cap;
    cap.insert(0, (CUPOLA_R - 0.1, CUPOLA_H * 0.72));
    lathe(b, c + DVec3::Y * ly, DVec3::Y, &cap, lseg, |_| {
        Some((Surf::Metal, chrome()))
    });
    let tip = ly + CUPOLA_H * 0.72 + 0.8;
    with(b, Surf::Metal, None, |b| {
        b.cylinder(c + DVec3::Y * tip, (0.18, 0.03), 3.2, 8, false, chrome())
    });
    if near {
        ball(
            b,
            c + DVec3::Y * (tip + 3.3),
            0.16,
            10,
            Surf::Glow,
            rgb(255, 60, 50),
        );
        // A thin glowing collar ring a little up the dome.
        torus(
            b,
            c + DVec3::Y * (REACTOR_WALL + 4.0),
            DVec3::X,
            DVec3::Z,
            dome_r(4.0) + 0.05,
            0.07,
            (96, 6),
            (Surf::Metal, chrome()),
        );
    }
    porch(b, c, near);
}

/// The porch on the reactor's front: a streamlined block with rounded
/// corners, glass doors, a chrome-edged canopy and a fin above.
fn porch(b: &mut MeshBuilder, c: DVec3, near: bool) {
    let z = porch_z();
    let (hw, depth, h, reach) = PORCH;
    debug_assert!((c.z + REACTOR_R - 0.6 - z).abs() < 1e-9);
    let plaster = near.then_some(Tex::from(FacadeLayer::Plaster));
    with(b, Surf::Plain, plaster, |b| {
        b.oriented_box(
            DVec3::new(0.0, h * 0.5, z + depth * 0.5),
            DVec3::Z,
            DVec3::new(hw - 0.6, h * 0.5, depth * 0.5),
            cream(),
        );
        // Rounded front corners.
        for x in [-(hw - 0.6), hw - 0.6] {
            b.cylinder(
                DVec3::new(x, 0.0, z + depth - 0.6),
                (0.6, 0.6),
                h,
                16,
                true,
                cream(),
            );
        }
        b.oriented_box(
            DVec3::new(0.0, h * 0.5, z + (depth - 0.6) * 0.5),
            DVec3::Z,
            DVec3::new(hw, h * 0.5, (depth - 0.6) * 0.5),
            cream(),
        );
    });
    // The canopy: a thin slab with a rounded front and a chrome edge.
    let cz = z + depth + reach;
    with(b, Surf::Plain, None, |b| {
        b.oriented_box(
            DVec3::new(0.0, h - 0.2, z + (depth + reach) * 0.5),
            DVec3::Z,
            DVec3::new(hw + 0.4, 0.14, (depth + reach) * 0.5),
            aqua(),
        );
    });
    if near {
        let edge: Vec<DVec3> = (0..=16)
            .map(|i| {
                let t = i as f64 / 16.0;
                DVec3::new((t - 0.5) * 2.0 * (hw + 0.4), h - 0.2, cz)
            })
            .collect();
        tube(b, &edge, (0.16, 0.16), 8, Some((Surf::Metal, chrome())));
        // Doors: two leaves of glass in a chrome frame.
        with(b, Surf::Metal, None, |b| {
            b.oriented_box(
                DVec3::new(0.0, 1.35, z + depth + 0.01),
                DVec3::Z,
                DVec3::new(1.5, 1.4, 0.05),
                chrome(),
            )
        });
        with(b, Surf::Glass, None, |b| {
            for x in [-0.72, 0.72] {
                b.oriented_box(
                    DVec3::new(x, 1.3, z + depth + 0.06),
                    DVec3::Z,
                    DVec3::new(0.66, 1.25, 0.03),
                    rgb(50, 80, 100),
                );
            }
        });
        // A porthole either side.
        for x in [-2.3, 2.3] {
            torus(
                b,
                DVec3::new(x, 2.6, z + depth + 0.02),
                DVec3::X,
                DVec3::Y,
                0.45,
                0.06,
                (24, 6),
                (Surf::Metal, chrome()),
            );
            with(b, Surf::Glass, None, |b| {
                let pts: Vec<DVec3> = (0..20)
                    .map(|k| {
                        let a = TAU * k as f64 / 20.0;
                        DVec3::new(x + 0.42 * a.cos(), 2.6 + 0.42 * a.sin(), z + depth + 0.015)
                    })
                    .collect();
                b.polygon(&pts, DVec3::Z, rgb(50, 80, 100));
            });
        }
    }
    // A red fin standing up off the canopy against the drum.
    let fin = [
        DVec3::new(0.0, h, z + depth + 0.6),
        DVec3::new(0.0, h, z - 0.4),
        DVec3::new(0.0, h + 3.6, z - 0.4),
        DVec3::new(0.0, h + 0.5, z + depth + 0.6),
    ];
    let s = DVec3::X * 0.12;
    b.hexa(
        [fin[0] - s, fin[1] - s, fin[1] + s, fin[0] + s],
        [fin[3] - s, fin[2] - s, fin[2] + s, fin[3] + s],
        red(),
    );
}

/// A cooling tower: a hyperbolic concrete shell on slanting legs, painted
/// bands round its lip, dark and wet inside over a pool.
fn tower(b: &mut MeshBuilder, at: DVec2, near: bool) {
    let c = v3(at, 0.0);
    let seg = if near { 96 } else { 24 };
    let rows = if near { 40 } else { 10 };
    let ys: Vec<f64> = (0..=rows)
        .map(|i| TOWER_SHELL_Y + (TOWER_H - TOWER_SHELL_Y) * i as f64 / rows as f64)
        .collect();
    // Bands near the top: aqua, cream, red.
    let band = |y: f64| {
        let f = (TOWER_H - y) / TOWER_H;
        if f < 0.035 {
            red()
        } else if f < 0.05 {
            cream()
        } else if f < 0.11 {
            aqua()
        } else {
            concrete()
        }
    };
    let mut outer: Vec<(f64, f64)> = ys.iter().map(|&y| (tower_r(y), y)).collect();
    // Painted bands get their own rows, so their edges are crisp.
    for f in [0.035, 0.05, 0.11] {
        let y = TOWER_H * (1.0 - f);
        outer.push((tower_r(y), y));
    }
    outer.sort_by(|a, b| a.1.total_cmp(&b.1));
    let bands: Vec<Color> = outer
        .windows(2)
        .map(|w| band((w[0].1 + w[1].1) * 0.5))
        .collect();
    with(
        b,
        Surf::Plain,
        near.then_some(Tex::from(FacadeLayer::Concrete)),
        |b| {
            lathe(b, c, DVec3::Y, &outer, seg, |k| {
                Some((Surf::Plain, bands[k.min(bands.len() - 1)]))
            });
            // The lip: a flat ring on top.
            let lip = [(TOWER_LIP, TOWER_H), (TOWER_LIP - 0.35, TOWER_H)];
            lathe(b, c, DVec3::Y, &lip, seg, |_| Some((Surf::Plain, red())));
            // Inside, dark and stained.
            let inner: Vec<(f64, f64)> = outer.iter().map(|&(r, y)| (r - 0.35, y)).rev().collect();
            inside(|| {
                lathe(b, c, DVec3::Y, &inner, seg, |_| {
                    Some((Surf::Plain, rgb(92, 92, 90)))
                })
            });
            // The ring beam the shell sits on.
            let beam = [
                (TOWER_FOOT - 0.5, TOWER_SHELL_Y - 0.45),
                (TOWER_FOOT + 0.1, TOWER_SHELL_Y - 0.45),
                (TOWER_FOOT + 0.1, TOWER_SHELL_Y + 0.05),
            ];
            lathe(b, c, DVec3::Y, &beam, seg, |_| {
                Some((Surf::Plain, rgb(176, 172, 164)))
            });
            inside(|| {
                lathe(
                    b,
                    c,
                    DVec3::Y,
                    &[
                        (TOWER_FOOT - 0.5, TOWER_SHELL_Y + 0.05),
                        (TOWER_FOOT - 0.5, TOWER_SHELL_Y - 0.45),
                    ],
                    seg,
                    |_| Some((Surf::Plain, rgb(150, 146, 140))),
                )
            });
        },
    );
    // Legs: pairs leaning both ways, a diagonal lattice round the foot.
    let legs = if near { TOWER_LEGS } else { 12 };
    for k in 0..legs {
        let a = TAU * k as f64 / legs as f64;
        let half = TAU / legs as f64 * 0.5;
        let foot = c + DVec3::new(a.cos(), 0.0, a.sin()) * (TOWER_FOOT - 0.4);
        for s in [-1.0, 1.0] {
            let b2 = a + s * half;
            let head = c
                + DVec3::new(b2.cos(), 0.0, b2.sin()) * (TOWER_FOOT - 0.2)
                + DVec3::Y * (TOWER_SHELL_Y - 0.4);
            with(
                b,
                Surf::Plain,
                near.then_some(Tex::from(FacadeLayer::Concrete)),
                |b| strut(b, foot, head, 0.42, rgb(180, 176, 168)),
            );
        }
    }
    if near {
        // The cold-water pool and its kerb.
        let pool: Vec<DVec3> = (0..48)
            .map(|k| {
                let a = TAU * k as f64 / 48.0;
                c + DVec3::new(
                    (TOWER_FOOT - 1.2) * a.cos(),
                    0.14,
                    (TOWER_FOOT - 1.2) * a.sin(),
                )
            })
            .collect();
        with(b, Surf::Glass, None, |b| {
            b.polygon(&pool, DVec3::Y, rgb(60, 82, 88))
        });
        let kerb = [
            (TOWER_FOOT - 1.2, 0.0),
            (TOWER_FOOT - 1.2, 0.3),
            (TOWER_FOOT - 0.9, 0.3),
            (TOWER_FOOT - 0.9, 0.0),
        ];
        lathe(b, c, DVec3::Y, &kerb, 48, |_| {
            Some((Surf::Plain, rgb(160, 156, 148)))
        });
        // A maintenance ladder up the outside to the lip, with hoops.
        let a = PI * 0.25;
        let out = DVec3::new(a.cos(), 0.0, a.sin());
        let side = DVec3::new(-a.sin(), 0.0, a.cos());
        let n = 30;
        for s in [-0.25, 0.25] {
            let path: Vec<DVec3> = (0..=n)
                .map(|i| {
                    let y = TOWER_SHELL_Y + (TOWER_H - TOWER_SHELL_Y) * i as f64 / n as f64;
                    c + out * (tower_r(y) + 0.25) + side * s + DVec3::Y * y
                })
                .collect();
            tube(
                b,
                &path,
                (0.035, 0.035),
                5,
                Some((Surf::Metal, rgb(120, 124, 128))),
            );
        }
        // Rungs every 40 cm.
        let rungs = ((TOWER_H - TOWER_SHELL_Y) / 0.4) as usize;
        for i in 1..rungs {
            let y = TOWER_SHELL_Y + 0.4 * i as f64;
            let p = c + out * (tower_r(y) + 0.25) + DVec3::Y * y;
            strut(
                b,
                p - side * 0.25,
                p + side * 0.25,
                0.04,
                rgb(120, 124, 128),
            );
        }
    }
}

/// A smooth sheet through `rows`, outward from `centre` (the turbine
/// hall's body and nose).
fn sheet(
    b: &mut MeshBuilder,
    rows: &[Vec<DVec3>],
    centre: DVec3,
    paint: impl Fn(usize, usize) -> Option<(Surf, Color)>,
) {
    let (n, m) = (rows.len(), rows[0].len());
    let mut acc = 0.0;
    for i in 0..n - 1 {
        for j in 0..m - 1 {
            let du = rows[i + 1][j] - rows[i][j];
            let dv = rows[i][j + 1] - rows[i][j];
            acc += du.cross(dv).dot(rows[i][j] - centre);
        }
    }
    b.grid(rows, false, acc >= 0.0, paint);
}

/// The turbine hall: a long barrel-roofed hall with a rounded nose
/// towards the gate, a ribbon of windows (lit at night), a band of aqua
/// and three fins over the nose.
fn hall(b: &mut MeshBuilder, near: bool) {
    let (cx, cz) = HALL_AT;
    let (hw, hl) = HALL_HALF;
    let z0 = cz - hl;
    let z1 = cz + hl - hw;
    // The section's half from the crown down to the ground, as (x, y) and
    // what each stretch below the next point is.
    let arc = if near { 12 } else { 4 };
    let mut half: Vec<(f64, f64)> = (0..=arc)
        .map(|i| {
            let t = 0.5 * PI * i as f64 / arc as f64;
            (hw * t.sin(), HALL_EAVES + HALL_VAULT * t.cos())
        })
        .collect();
    let wall = [
        (hw, HALL_EAVES - 0.5),
        (hw, 7.6),
        (hw, 5.4),
        (hw, 1.0),
        (hw, 0.0),
    ];
    half.extend(wall);
    let roof_n = arc;
    let window =
        crate::citymesh::Surf::Lit(crate::daylight::channel(crate::daylight::Tone::Cool, 4));
    let paint_of = move |j: usize| -> (Surf, Color) {
        if j < roof_n {
            (Surf::Metal, rgb(150, 196, 190))
        } else if j == roof_n {
            (Surf::Plain, aqua())
        } else if j == roof_n + 2 {
            (window, rgb(46, 70, 92))
        } else if j == roof_n + 4 {
            (Surf::Plain, rgb(150, 146, 140))
        } else {
            (Surf::Plain, cream())
        }
    };
    let roof_tex = near.then_some(Tex::from(RoofLayer::Zinc));
    let wall_tex = near.then_some(Tex::from(FacadeLayer::Stucco));
    // Body: the full section (left half mirrored), swept along z.
    let section: Vec<(f64, f64, usize)> = half
        .iter()
        .enumerate()
        .rev()
        .map(|(j, &(x, y))| (-x, y, j))
        .chain(
            half.iter()
                .enumerate()
                .skip(1)
                .map(|(j, &(x, y))| (x, y, j)),
        )
        .collect();
    let n_z = if near { 8 } else { 2 };
    let rows: Vec<Vec<DVec3>> = (0..=n_z)
        .map(|i| {
            let z = z0 + (z1 - z0) * i as f64 / n_z as f64;
            section
                .iter()
                .map(|&(x, y, _)| DVec3::new(cx + x, y, z))
                .collect()
        })
        .collect();
    let centre = DVec3::new(cx, HALL_EAVES * 0.5, (z0 + z1) * 0.5);
    // Quad j runs between section points j and j + 1: its stretch is the
    // one of the point nearer the ground.
    let stretch = |j: usize| {
        let (a, bb) = (section[j], section[j + 1]);
        if a.1 < bb.1 {
            a.2.saturating_sub(1)
                .max(if a.2 == 0 { 0 } else { a.2 - 1 })
        } else {
            bb.2.saturating_sub(1)
        }
    };
    let stretch_of: Vec<usize> = (0..section.len() - 1).map(stretch).collect();
    for tex_roof in [true, false] {
        with(
            b,
            Surf::Plain,
            if tex_roof { roof_tex } else { wall_tex },
            |b| {
                sheet(b, &rows, centre, |_, j| {
                    let s = stretch_of[j];
                    let p = paint_of(s);
                    ((s < roof_n) == tex_roof).then_some(p)
                });
            },
        );
    }
    // The nose: the half section swung round the front.
    let n_a = if near { 24 } else { 6 };
    let nose: Vec<Vec<DVec3>> = (0..=n_a)
        .map(|i| {
            let a = PI * i as f64 / n_a as f64;
            half.iter()
                .map(|&(r, y)| DVec3::new(cx + r * a.cos(), y, z1 + r * a.sin()))
                .collect()
        })
        .collect();
    let nose_centre = DVec3::new(cx, HALL_EAVES * 0.5, z1);
    for tex_roof in [true, false] {
        with(
            b,
            Surf::Plain,
            if tex_roof { roof_tex } else { wall_tex },
            |b| {
                sheet(b, &nose, nose_centre, |_, j| {
                    let s = j;
                    let p = paint_of(s);
                    ((s < roof_n) == tex_roof).then_some(p)
                });
            },
        );
    }
    // The back wall, flat, with a big rolling door.
    let back: Vec<DVec3> = section
        .iter()
        .map(|&(x, y, _)| DVec3::new(cx + x, y, z0))
        .collect();
    with(b, Surf::Plain, wall_tex, |b| {
        b.polygon(&back, -DVec3::Z, cream())
    });
    with(b, Surf::Metal, None, |b| {
        b.oriented_box(
            DVec3::new(cx, 2.6, z0 - 0.04),
            DVec3::Z,
            DVec3::new(2.4, 2.6, 0.05),
            rgb(130, 136, 140),
        )
    });
    if near {
        // Mullions across the ribbon windows.
        let mull = rgb(214, 218, 224);
        let mut z = z0 + 1.2;
        while z < z1 {
            for s in [-1.0, 1.0] {
                with(b, Surf::Metal, None, |b| {
                    b.oriented_box(
                        DVec3::new(cx + s * (hw + 0.04), 6.5, z),
                        DVec3::Z,
                        DVec3::new(0.05, 1.1, 0.07),
                        mull,
                    )
                });
            }
            z += 1.6;
        }
        for i in 1..12 {
            let a = PI * i as f64 / 12.0;
            with(b, Surf::Metal, None, |b| {
                b.oriented_box(
                    DVec3::new(cx + (hw + 0.04) * a.cos(), 6.5, z1 + (hw + 0.04) * a.sin()),
                    DVec3::new(a.cos(), 0.0, a.sin()),
                    DVec3::new(0.07, 1.1, 0.05),
                    mull,
                )
            });
        }
        // A skylight strip along the crown.
        with(b, Surf::Glass, None, |b| {
            b.oriented_box(
                DVec3::new(cx, HALL_EAVES + HALL_VAULT + 0.05, (z0 + z1) * 0.5),
                DVec3::Z,
                DVec3::new(0.8, 0.12, (z1 - z0) * 0.5 - 1.0),
                rgb(80, 120, 140),
            )
        });
    }
    // Three red fins rising over the nose, the tallest in the middle.
    for (k, x) in [-1.6, 0.0, 1.6].iter().enumerate() {
        let tall = if k == 1 { 6.5 } else { 4.6 };
        let z = z1 + hw * 0.55;
        let s = DVec3::X * 0.13;
        let base = |dz: f64| DVec3::new(cx + x, HALL_EAVES + HALL_VAULT * 0.55, z + dz);
        let lo = [base(1.2) - s, base(-3.0) - s, base(-3.0) + s, base(1.2) + s];
        let top = |dz: f64, up: f64| DVec3::new(cx + x, HALL_EAVES + HALL_VAULT + up, z + dz);
        let hi = [
            top(0.4, tall * 0.35) - s,
            top(-3.4, tall) - s,
            top(-3.4, tall) + s,
            top(0.4, tall * 0.35) + s,
        ];
        b.hexa(lo, hi, red());
    }
    // The relief valve's pipe and silencer on the roof.
    let v = vent();
    with(b, Surf::Metal, None, |b| {
        b.cylinder(
            v - DVec3::Y * 3.2,
            (0.35, 0.35),
            2.2,
            16,
            false,
            rgb(140, 144, 148),
        );
        b.cylinder(
            v - DVec3::Y * 1.0,
            (0.6, 0.6),
            1.0,
            16,
            true,
            rgb(200, 70, 56),
        );
    });
}

/// Where the relief valve on the hall roof blows off (plant frame).
pub fn vent() -> DVec3 {
    DVec3::new(
        HALL_AT.0 + 1.6,
        HALL_EAVES + HALL_VAULT + 1.4,
        HALL_AT.1 - 5.0,
    )
}

/// The steam mains from the drum to the hall, on trestles, with flanges.
fn mains(b: &mut MeshBuilder, near: bool) {
    let x0 = REACTOR_R - 0.6;
    let x1 = HALL_AT.0 - HALL_HALF.0 + 0.2;
    let seg = if near { 20 } else { 8 };
    for (y, z) in MAINS {
        let path = [DVec3::new(x0, y, z), DVec3::new(x1, y, z)];
        tube(
            b,
            &path,
            (MAIN_R, MAIN_R),
            seg,
            Some((Surf::Metal, rgb(214, 214, 206))),
        );
        if near {
            for k in 0..4 {
                let x = x0 + 1.5 + (x1 - x0 - 3.0) * k as f64 / 3.0;
                torus(
                    b,
                    DVec3::new(x, y, z),
                    DVec3::Y,
                    DVec3::Z,
                    MAIN_R + 0.05,
                    0.07,
                    (seg, 6),
                    (Surf::Metal, chrome()),
                );
            }
        }
    }
    // One trestle under both, half way.
    let x = (x0 + x1) * 0.5;
    let (za, zb) = (MAINS[0].1, MAINS[1].1);
    let iron = rgb(110, 116, 120);
    for z in [za - 0.9, zb + 0.9] {
        strut(
            b,
            DVec3::new(x, 0.0, z),
            DVec3::new(x, MAINS[0].0 - MAIN_R, z),
            0.3,
            iron,
        );
    }
    b.oriented_box(
        DVec3::new(x, MAINS[0].0 - MAIN_R - 0.15, (za + zb) * 0.5),
        DVec3::Z,
        DVec3::new(0.25, 0.15, (zb - za) * 0.5 + 1.1),
        iron,
    );
}

/// The ventilation stack: red and white bands, a gallery near the top and
/// a beacon (the beacon itself is live).
fn stack(b: &mut MeshBuilder, near: bool) {
    let c = DVec3::new(STACK_AT.0, 0.0, STACK_AT.1);
    let seg = if near { 32 } else { 10 };
    with(
        b,
        Surf::Plain,
        near.then_some(Tex::from(FacadeLayer::Concrete)),
        |b| {
            b.oriented_box(
                c + DVec3::Y * 1.0,
                DVec3::Z,
                DVec3::new(2.3, 1.0, 2.3),
                concrete(),
            );
        },
    );
    let bands = 10;
    let r_at = |y: f64| STACK_R.0 + (STACK_R.1 - STACK_R.0) * (y - 2.0) / (STACK_H - 2.0);
    let prof: Vec<(f64, f64)> = (0..=bands)
        .map(|i| {
            let y = 2.0 + (STACK_H - 2.0) * i as f64 / bands as f64;
            (r_at(y), y)
        })
        .collect();
    lathe(b, c, DVec3::Y, &prof, seg, |k| {
        Some((
            Surf::Plain,
            if k % 2 == 1 {
                red()
            } else {
                rgb(240, 238, 232)
            },
        ))
    });
    lathe(
        b,
        c,
        DVec3::Y,
        &[
            (STACK_R.1, STACK_H),
            (STACK_R.1 - 0.2, STACK_H),
            (STACK_R.1 - 0.2, STACK_H - 1.0),
        ],
        seg,
        |_| Some((Surf::Plain, rgb(40, 38, 36))),
    );
    if near {
        let gy = STACK_H - 6.0;
        let gr = r_at(gy) + 1.0;
        lathe(
            b,
            c,
            DVec3::Y,
            &[
                (r_at(gy), gy),
                (gr, gy),
                (gr, gy + 0.15),
                (r_at(gy), gy + 0.15),
            ],
            seg,
            |_| Some((Surf::Metal, rgb(120, 124, 128))),
        );
        torus(
            b,
            c + DVec3::Y * (gy + 1.0),
            DVec3::X,
            DVec3::Z,
            gr - 0.05,
            0.04,
            (seg, 5),
            (Surf::Metal, rgb(120, 124, 128)),
        );
        for k in 0..12 {
            let a = TAU * k as f64 / 12.0;
            let p = c + DVec3::new(a.cos(), 0.0, a.sin()) * (gr - 0.05);
            strut(
                b,
                p + DVec3::Y * gy,
                p + DVec3::Y * (gy + 1.0),
                0.05,
                rgb(120, 124, 128),
            );
        }
    }
}

/// Where the stack's beacon is (plant frame).
pub fn beacon() -> DVec3 {
    DVec3::new(STACK_AT.0, STACK_H + 0.5, STACK_AT.1)
}

/// The atom at the gate: a slender pylon on three fins, and on top a
/// nucleus inside three chrome orbits (the electrons are live).
fn pylon(b: &mut MeshBuilder, near: bool) {
    let p = DVec3::new(PYLON_AT.0, 0.0, PYLON_AT.1);
    let seg = if near { 24 } else { 8 };
    with(
        b,
        Surf::Plain,
        near.then_some(Tex::from(FacadeLayer::Granite)),
        |b| {
            lathe(
                b,
                p,
                DVec3::Y,
                &[(0.0, 0.0), (1.4, 0.0), (1.4, 0.6), (1.1, 1.0), (0.0, 1.0)],
                seg,
                |_| Some((Surf::Plain, rgb(70, 74, 82))),
            )
        },
    );
    lathe(
        b,
        p,
        DVec3::Y,
        &[(0.45, 1.0), (0.32, PYLON_H * 0.6), (0.22, PYLON_H + 0.2)],
        seg,
        |_| Some((Surf::Metal, chrome())),
    );
    for k in 0..3 {
        let a = TAU * k as f64 / 3.0 + 0.3;
        let d = DVec3::new(a.cos(), 0.0, a.sin());
        let s = DVec3::new(-a.sin(), 0.0, a.cos()) * 0.08;
        let lo = [
            p + DVec3::Y + d * 0.3 - s,
            p + DVec3::Y + d * 1.3 - s,
            p + DVec3::Y + d * 1.3 + s,
            p + DVec3::Y + d * 0.3 + s,
        ];
        let hi = [
            p + DVec3::Y * 7.0 + d * 0.3 - s,
            p + DVec3::Y * 1.6 + d * 1.3 - s,
            p + DVec3::Y * 1.6 + d * 1.3 + s,
            p + DVec3::Y * 7.0 + d * 0.3 + s,
        ];
        b.hexa(lo, hi, if k == 0 { red() } else { aqua() });
    }
    let (c, planes) = orbits();
    ball(
        b,
        c,
        0.9,
        if near { 24 } else { 8 },
        Surf::Glow,
        col(NUCLEUS),
    );
    for (e1, e2) in planes {
        torus(
            b,
            c,
            e1,
            e2,
            ATOM_R,
            0.09,
            (if near { 64 } else { 16 }, if near { 8 } else { 4 }),
            (Surf::Metal, chrome()),
        );
    }
}

/// The gatehouse: a round booth, glass all round, under a saucer roof.
fn gatehouse(b: &mut MeshBuilder, near: bool) {
    let c = DVec3::new(GATEHOUSE_AT.0, 0.0, GATEHOUSE_AT.1);
    let seg = if near { 48 } else { 12 };
    let r = GATEHOUSE_R;
    lathe(
        b,
        c,
        DVec3::Y,
        &[(0.0, 0.0), (r, 0.0), (r, 1.0)],
        seg,
        |_| Some((Surf::Plain, cream())),
    );
    lathe(
        b,
        c,
        DVec3::Y,
        &[(r - 0.05, 1.0), (r - 0.05, GATEHOUSE_H)],
        seg,
        |_| {
            Some((
                Surf::Lit(crate::daylight::channel(crate::daylight::Tone::Lamp, 4)),
                rgb(50, 80, 100),
            ))
        },
    );
    // The saucer.
    let rr = r + 1.3;
    let roof: Vec<(f64, f64)> = vec![
        (0.0, GATEHOUSE_H),
        (r - 0.05, GATEHOUSE_H),
        (rr, GATEHOUSE_H + 0.25),
        (rr + 0.05, GATEHOUSE_H + 0.38),
        (rr - 0.2, GATEHOUSE_H + 0.5),
        (r * 0.6, GATEHOUSE_H + 0.85),
        (0.0, GATEHOUSE_H + 0.95),
    ];
    lathe(b, c, DVec3::Y, &roof, seg, |k| {
        Some(if k == 2 {
            (Surf::Metal, chrome())
        } else {
            (Surf::Plain, aqua())
        })
    });
    if near {
        // Mullions.
        for k in 0..8 {
            let a = TAU * k as f64 / 8.0;
            let d = DVec3::new(a.cos(), 0.0, a.sin());
            with(b, Surf::Metal, None, |b| {
                b.cylinder(
                    c + d * (r - 0.02) + DVec3::Y,
                    (0.05, 0.05),
                    GATEHOUSE_H - 1.0,
                    6,
                    false,
                    chrome(),
                )
            });
        }
        // A red ball on a mast.
        with(b, Surf::Metal, None, |b| {
            b.cylinder(
                c + DVec3::Y * (GATEHOUSE_H + 0.9),
                (0.05, 0.03),
                1.4,
                6,
                false,
                chrome(),
            )
        });
        ball(
            b,
            c + DVec3::Y * (GATEHOUSE_H + 2.4),
            0.2,
            12,
            Surf::Plain,
            red(),
        );
    }
}

/// The plant's live parts are made within this distance (m) ...
const LOAD: f32 = 2600.0;
/// ... and dropped past this.
const DROP: f32 = 2900.0;
/// Electrons and the beacon only play this near.
const ACTIVE: f32 = 1200.0;
/// Glowing reactor parts (see [`reactor_glow`]).
const GLOW_PARTS: usize = 3;

/// What the soundscape hears of the plant this frame.
#[derive(Clone, Debug, Default)]
pub struct Heard {
    /// The turbine hall's hum, and the nearest tower's rushing steam.
    pub hum: Option<Vec3>,
    pub steam: Option<Vec3>,
    /// Relief valves blown since the last frame.
    pub vents: Vec<Vec3>,
}

/// A tiny xorshift: the plant's moods need variety, not a seed.
struct Rng(u64);

impl Rng {
    fn f(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
}

struct Live {
    origin: Vec3,
    yaw: f32,
    glow: Vec<EntityId>,
    electron: EntityId,
    beacon: EntityId,
    steam: Vec<EmitterId>,
    vent: Option<(EmitterId, f32)>,
    lamp_core: LampId,
    lamp_beacon: LampId,
}

pub struct AtomView {
    plant: Option<Landmark>,
    live: Option<Live>,
    rng: Rng,
    clock: f32,
    /// A surge in the reactor's light (decays to 0).
    surge: f32,
    next_surge: f32,
    next_vent: f32,
    /// Seconds the valve keeps blowing (0: shut).
    venting: f32,
    heard: Heard,
}

impl AtomView {
    pub fn new(town: &TownWorld) -> Self {
        Self::with_plant(
            town.landmarks
                .iter()
                .find(|l| l.kind == LandmarkKind::AtomicPlant)
                .cloned(),
        )
    }

    fn with_plant(plant: Option<Landmark>) -> Self {
        Self {
            plant,
            live: None,
            rng: Rng(0xa70b_1c5e_ed00_0001),
            clock: 0.0,
            surge: 0.0,
            next_surge: 9.0,
            next_vent: 14.0,
            venting: 0.0,
            heard: Heard::default(),
        }
    }

    /// Is the plant's live part made (for the memory checks and tests)?
    pub fn is_live(&self) -> bool {
        self.live.is_some()
    }

    /// What the soundscape should hear this frame (vents handed over once).
    pub fn heard(&mut self) -> Heard {
        let vents = std::mem::take(&mut self.heard.vents);
        Heard {
            vents,
            ..self.heard.clone()
        }
    }

    /// The plant's world point at `local` (its own frame).
    fn world_at(origin: Vec3, yaw: f32, local: DVec3) -> Vec3 {
        let a = (yaw as f64).to_radians();
        let (s, c) = a.sin_cos();
        let x = local.x * c + local.z * s;
        let z = -local.x * s + local.z * c;
        origin + Vec3::new(x as f32, local.y as f32, z as f32)
    }

    fn spawn(world: &mut World, l: &Landmark) -> EngineResult<Live> {
        let origin = Vec3::new(l.pos.x as f32, l.base as f32, l.pos.y as f32);
        let yaw = l.dir.x.atan2(l.dir.y).to_degrees() as f32;
        let make = |world: &mut World, f: &dyn Fn(&mut MeshBuilder)| -> EngineResult<EntityId> {
            let mut b = MeshBuilder::new();
            b.surf = Surf::Glow;
            f(&mut b);
            let id = world.spawn_instanced(b.finish());
            world.set_casts_shadow(id, false)?;
            Ok(id)
        };
        let mut glow = Vec::new();
        for part in 0..GLOW_PARTS {
            glow.push(make(world, &|b| reactor_glow(b, part, true, true))?);
        }
        let electron = make(world, &|b| {
            ball(b, DVec3::ZERO, 0.32, 12, Surf::Glow, col(GLOW));
        })?;
        let lamp_ball = make(world, &|b| {
            ball(b, DVec3::ZERO, 0.45, 12, Surf::Glow, rgb(255, 50, 40));
        })?;
        // Steam rolling up out of each tower, drifting off downwind.
        let mut steam = Vec::new();
        for (k, t) in towers().iter().enumerate() {
            let at = Self::world_at(origin, yaw, v3(*t, TOWER_H - 2.0));
            let e = ParticleEmitter::new(at)
                .with_rate(5.0)
                .with_silhouette(ParticleSilhouette::Puff)
                .with_blend(ParticleBlend::Alpha)
                .with_shape(ParticleShape::Sphere)
                .with_spread(Vec3::new(4.2, 0.8, 4.2))
                .with_velocity(Vec3::new(0.0, 3.2, 0.0))
                .with_velocity_jitter(Vec3::new(0.5, 0.8, 0.5))
                .with_acceleration(Vec3::new(0.45, 0.25, 0.15))
                .with_lifetime(17.0)
                .with_size(5.5)
                .with_size_variation(0.4)
                .with_size_over_life(SizeOverLife::Expand)
                .with_color(rgba(244, 246, 248, 140))
                .with_secondary_color(rgba(222, 224, 230, 0))
                .with_turbulence(0.6)
                .with_drag(0.15)
                .with_spin(0.25)
                .with_lighting(0.1)
                .with_seed(91 + k as u32);
            steam.push(world.particles_mut().start(e)?);
        }
        let blue = col(GLOW);
        let core = Self::world_at(origin, yaw, DVec3::new(0.0, 3.0, porch_z() + PORCH.1 + 6.0));
        let lamp_core = world.add_lamp(Lamp::point(core, blue, 14.0, 35.0))?;
        let lamp_beacon = world.add_lamp(Lamp::point(
            Self::world_at(origin, yaw, beacon()),
            rgb(255, 50, 40),
            0.0,
            30.0,
        ))?;
        Ok(Live {
            origin,
            yaw,
            glow,
            electron,
            beacon: lamp_ball,
            steam,
            vent: None,
            lamp_core,
            lamp_beacon,
        })
    }

    pub fn clear(&mut self, world: &mut World) {
        if let Some(live) = self.live.take() {
            for id in live.glow.iter().chain([&live.electron, &live.beacon]) {
                world.despawn(*id);
            }
            let p = world.particles_mut();
            for e in live.steam.iter().chain(live.vent.as_ref().map(|v| &v.0)) {
                let _ = p.stop(*e);
            }
            world.remove_lamp(live.lamp_core);
            world.remove_lamp(live.lamp_beacon);
        }
        self.heard = Heard::default();
    }

    pub fn update(&mut self, world: &mut World, dt: f32) -> EngineResult<()> {
        let Some(l) = self.plant.clone() else {
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
        let dt = dt.clamp(0.0, 0.1);
        let active = far < ACTIVE;
        let blow = self.step(dt, active);
        let mut live = self.live.take().expect("live");
        let drawn = self.draw(world, &mut live, active, blow, eye);
        self.live = Some(live);
        drawn
    }

    /// Advances the plant's moods; true when the valve blows this frame.
    fn step(&mut self, dt: f32, active: bool) -> bool {
        self.clock += dt;
        self.surge = (self.surge - dt * 0.9).max(0.0);
        self.next_surge -= dt;
        if self.next_surge <= 0.0 {
            self.surge = 1.0;
            self.next_surge = self.rng.range(8.0, 22.0);
        }
        self.venting = (self.venting - dt).max(0.0);
        let mut blow = false;
        if active {
            self.next_vent -= dt;
            if self.next_vent <= 0.0 {
                self.venting = self.rng.range(2.5, 4.0);
                self.next_vent = self.rng.range(25.0, 60.0);
                blow = true;
            }
        }
        blow
    }

    /// How bright the reactor glows now (about 1).
    fn breath(&self) -> f32 {
        let slow = (self.clock * TAU as f32 / 3.4).sin();
        let flick = 0.04 * (self.clock * 41.0).sin() * (self.clock * 7.3).sin();
        0.85 + 0.3 * slow + 0.9 * self.surge * self.surge + flick
    }

    fn draw(
        &mut self,
        world: &mut World,
        live: &mut Live,
        active: bool,
        blow: bool,
        eye: Vec3,
    ) -> EngineResult<()> {
        let tint = |k: f32| Color::rgb01_unchecked(k, k, k);
        let at = Place::new(live.origin.x, live.origin.y, live.origin.z).with_yaw_deg(live.yaw);
        let k = self.breath();
        // The ring and the lantern breathe a beat apart from the band.
        let lag = 0.85 + 0.3 * (self.clock * TAU as f32 / 3.4 - 0.9).sin() + 0.9 * self.surge;
        let shades = [k * 1.1, lag, k * 1.25];
        for (id, s) in live.glow.iter().zip(shades) {
            world.set_instances(*id, &[at.with_tint(tint(s))])?;
        }
        // Electrons racing round their orbits, each with a fading tail.
        let mut dots = Vec::new();
        if active {
            let (c, planes) = orbits();
            for (i, (e1, e2)) in planes.iter().enumerate() {
                let speed = 2.4 + 0.5 * i as f64;
                let a0 = self.clock as f64 * speed + i as f64 * 2.1;
                for t in 0..5 {
                    let a = a0 - t as f64 * 0.11;
                    let p = c + (*e1 * a.cos() + *e2 * a.sin()) * ATOM_R;
                    let w = Self::world_at(live.origin, live.yaw, p);
                    let fade = 1.0 - t as f32 * 0.2;
                    dots.push(
                        Place::new(w.x, w.y, w.z)
                            .scale(1.0 - t as f32 * 0.16)?
                            .with_tint(tint(1.6 * fade)),
                    );
                }
            }
        }
        world.set_instances(live.electron, &dots)?;
        // The beacon: a slow blink.
        let on = active && (self.clock % 1.8) < 0.35;
        let b = Self::world_at(live.origin, live.yaw, beacon());
        let places = if on {
            vec![Place::new(b.x, b.y, b.z).with_tint(tint(2.0))]
        } else {
            Vec::new()
        };
        world.set_instances(live.beacon, &places)?;
        world.set_lamp(
            live.lamp_beacon,
            Lamp::point(b, rgb(255, 50, 40), if on { 60.0 } else { 0.0 }, 30.0),
        )?;
        let core = Self::world_at(
            live.origin,
            live.yaw,
            DVec3::new(0.0, 3.0, porch_z() + PORCH.1 + 6.0),
        );
        world.set_lamp(live.lamp_core, Lamp::point(core, col(GLOW), 14.0 * k, 35.0))?;
        // The relief valve.
        let v = Self::world_at(live.origin, live.yaw, vent());
        if blow {
            let e = ParticleEmitter::new(v)
                .with_rate(40.0)
                .with_silhouette(ParticleSilhouette::Puff)
                .with_blend(ParticleBlend::Alpha)
                .with_shape(ParticleShape::Sphere)
                .with_spread(Vec3::splat(0.3))
                .with_velocity(Vec3::new(0.0, 14.0, 0.0))
                .with_velocity_jitter(Vec3::new(1.2, 2.0, 1.2))
                .with_acceleration(Vec3::new(0.6, -1.5, 0.2))
                .with_lifetime(3.2)
                .with_size(1.4)
                .with_size_variation(0.4)
                .with_size_over_life(SizeOverLife::Expand)
                .with_color(rgba(248, 248, 250, 190))
                .with_secondary_color(rgba(232, 232, 236, 0))
                .with_turbulence(0.8)
                .with_drag(0.35)
                .with_lighting(0.1);
            if let Some((old, _)) = live.vent.take() {
                let _ = world.particles_mut().stop(old);
            }
            live.vent = Some((world.particles_mut().start(e)?, self.venting));
            self.heard.vents.push(v);
        }
        if self.venting <= 0.0 {
            if let Some((e, _)) = live.vent.take() {
                world.particles_mut().stop(e)?;
            }
        }
        // Sounds: the hall's hum, and the nearer tower's steam.
        self.heard.hum = Some(Self::world_at(
            live.origin,
            live.yaw,
            DVec3::new(HALL_AT.0, 4.0, HALL_AT.1),
        ));
        self.heard.steam = towers()
            .iter()
            .map(|t| Self::world_at(live.origin, live.yaw, v3(*t, TOWER_H * 0.5)))
            .min_by(|a, b| a.distance(eye).total_cmp(&b.distance(eye)));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plant_has_its_parts() {
        for near in [true, false] {
            let mut b = MeshBuilder::new();
            plant(&mut b, near);
            let n: usize = b.finish_parts().iter().map(|m| m.face_count()).sum();
            if near {
                // A close-up showpiece, not a stand-in.
                assert!(n > 30_000, "near {n}");
            } else {
                assert!((1_000..12_000).contains(&n), "far {n}");
            }
        }
        for part in 0..GLOW_PARTS {
            let mut b = MeshBuilder::new();
            reactor_glow(&mut b, part, true, true);
            assert!(b.finish().face_count() > 20);
        }
    }

    #[test]
    fn the_atom_spins_on_its_pylon() {
        let (c, planes) = orbits();
        assert!(c.y > PYLON_H);
        for (e1, e2) in planes {
            assert!((e1.length() - 1.0).abs() < 1e-9 && (e2.length() - 1.0).abs() < 1e-9);
            assert!(e1.dot(e2).abs() < 1e-9);
            // The lowest point of each orbit clears the pylon's top.
            let low = c.y - ATOM_R * e2.y.abs().max(e1.y.abs());
            assert!(low > PYLON_H - 1.0, "{low}");
        }
    }

    #[test]
    fn the_valve_blows_now_and_then_and_the_light_breathes() {
        let mut v = AtomView::with_plant(None);
        let (mut blows, mut lo, mut hi) = (0, f32::MAX, f32::MIN);
        for _ in 0..60 * 300 {
            if v.step(1.0 / 60.0, true) {
                blows += 1;
            }
            let k = v.breath();
            lo = lo.min(k);
            hi = hi.max(k);
        }
        assert!((4..=13).contains(&blows), "{blows}");
        assert!(lo > 0.4 && hi > 1.5, "{lo} {hi}");
        let mut v = AtomView::with_plant(None);
        for _ in 0..60 * 120 {
            assert!(!v.step(1.0 / 60.0, false));
        }
    }
}
