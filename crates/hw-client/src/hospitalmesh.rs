//! The Aether Mercy Hospital (`hw_gen::urban::hospital`): a streamline
//! hall in cream porcelain enamel with teal bands, ribbon windows and
//! eyebrow ledges, a glass vault down its roof, a round ward tower framed
//! by two fins and crowned with the drone deck and a pylon carrying the
//! glowing ring-and-cross, and a rounded canopy over the open entrance.
//!
//! Inside, the Recovery Hall: terrazzo floor, mint wainscot, round ceiling
//! lights, the kidney-shaped reception desk, six upright glass recovery
//! pods along the back wall either side of the humming healing core, a
//! waiting corner with chairs and plants, and examination couches under
//! scanner arms.
//!
//! Everything is built in the hospital's own frame (origin on the plinth's
//! top, +z out of the entrance) from the same numbers its collision uses.

use crate::carmesh::{lathe, tube};
use crate::citymesh::{MeshBuilder, Surf};
use crate::interior::pieces::{self, Cover, Maker, Mat};
use crate::skins::{Base, Skin, Tex, Wear};
use engine::prelude::*;
use glam::{DVec2, DVec3};
use hw_gen::urban::hospital::*;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// The hospital's colours (sRGB, picked by eye).
fn cream() -> Color {
    rgb(240, 232, 212)
}
fn teal() -> Color {
    rgb(46, 150, 150)
}
fn mint() -> Color {
    rgb(178, 226, 206)
}
fn stone() -> Color {
    rgb(150, 160, 158)
}
fn chrome() -> Color {
    pieces::chrome()
}
fn glass() -> Color {
    rgb(70, 110, 120)
}
/// The healing light: a mint green, brighter than the town's Aether cyan.
pub const HEAL: (u8, u8, u8) = (120, 255, 200);
fn heal() -> Color {
    rgb(HEAL.0, HEAL.1, HEAL.2)
}
fn aether() -> Color {
    rgb(120, 230, 255)
}

fn enamel(b: &mut MeshBuilder, near: bool) {
    b.surf = Surf::Plain;
    b.tex = near.then(|| Tex::from(Skin::new(Base::Enamel, Wear::Fresh)));
}

fn cast(b: &mut MeshBuilder, near: bool) {
    b.surf = Surf::Plain;
    b.tex = near.then(|| Tex::from(Skin::new(Base::CastStone, Wear::Aged)));
}

fn plain(b: &mut MeshBuilder) {
    b.surf = Surf::Plain;
    b.tex = None;
}

/// A point of the hall's outline, `off` metres outside its outer face
/// (negative: in from it), with the way out there and where along the
/// outline it is: (point, outward, which side: 0 front, 1 left end,
/// 2 back, 3 right end, local x).
#[derive(Clone, Copy)]
struct Rim {
    p: DVec2,
    out: DVec2,
    side: u8,
    x: f64,
}

/// Points round the hall's outline, `off` outside its outer face: the
/// front from right to left (one a metre), the left end, the back from
/// left to right, the right end. Seen from above that runs so that
/// (up × along) points out.
fn outline(off: f64, arc: usize) -> Vec<Rim> {
    let r = HALL_R + off;
    let mut v = Vec::new();
    let n = (HALL_A * 2.0).round() as usize;
    for j in 0..n {
        let x = HALL_A - j as f64 * 2.0 * HALL_A / n as f64;
        v.push(Rim {
            p: DVec2::new(x, HALL_Z + r),
            out: DVec2::Y,
            side: 0,
            x,
        });
    }
    for j in 0..arc {
        let t = FRAC_PI_2 + PI * j as f64 / arc as f64;
        let out = DVec2::new(t.cos(), t.sin());
        v.push(Rim {
            p: DVec2::new(-HALL_A, HALL_Z) + out * r,
            out,
            side: 1,
            x: -HALL_A + out.x * r,
        });
    }
    for j in 0..n {
        let x = -HALL_A + j as f64 * 2.0 * HALL_A / n as f64;
        v.push(Rim {
            p: DVec2::new(x, HALL_Z - r),
            out: -DVec2::Y,
            side: 2,
            x,
        });
    }
    for j in 0..arc {
        let t = -FRAC_PI_2 + PI * j as f64 / arc as f64;
        let out = DVec2::new(t.cos(), t.sin());
        v.push(Rim {
            p: DVec2::new(HALL_A, HALL_Z) + out * r,
            out,
            side: 3,
            x: HALL_A + out.x * r,
        });
    }
    v
}

/// Is the stretch of outline from rim `a` to rim `b` the entrance?
fn is_door(a: &Rim, b: &Rim) -> bool {
    a.side == 0
        && b.side == 0
        && a.x.min(b.x) >= -DOOR_HALF - 1e-6
        && a.x.max(b.x) <= DOOR_HALF + 1e-6
}

/// Is it the back wall where the tower stands behind it (no windows)?
fn is_blind(a: &Rim, b: &Rim) -> bool {
    a.side == 2 && b.side == 2 && a.x.min(b.x) >= -TOWER_R - 0.5 && a.x.max(b.x) <= TOWER_R + 0.5
}

/// The window bands on the hall's walls: (bottom, top).
const WINDOWS: [(f64, f64); 2] = [(0.9, 2.9), (4.2, 5.4)];

/// A ring of the outline at height `y`, `off` outside the outer face.
fn ring(rims: &[Rim], off: f64, y: f64) -> Vec<DVec3> {
    rims.iter()
        .map(|r| {
            let p = r.p + r.out * off;
            DVec3::new(p.x, y, p.y)
        })
        .collect()
}

/// The whole hospital, in its own frame, onto `b` (one mesh per texture
/// once finished). `near`: the close-up with every part and the
/// furniture; otherwise the shape seen from across town.
pub fn hospital(b: &mut MeshBuilder, near: bool) {
    hall(b, near);
    roof(b, near);
    tower(b, near);
    canopy(b, near);
    bay(b, near);
    if near {
        inside(b);
        planters(b);
    }
    plain(b);
}

fn hall(b: &mut MeshBuilder, near: bool) {
    let arc = if near { 18 } else { 8 };
    let rims = outline(0.0, arc);
    let n = rims.len();
    let next = |j: usize| (j + 1) % n;
    // Outer face, band by band.
    let ys = [0.0, 0.45, 0.9, 2.9, 3.4, 4.2, 5.4, 6.2, 6.45, PARAPET];
    let rows: Vec<Vec<DVec3>> = ys.iter().map(|&y| ring(&rims, 0.0, y)).collect();
    let paint_out = |i: usize, j: usize| -> Option<(Surf, Color)> {
        let (a, c) = (&rims[j], &rims[next(j)]);
        let door = is_door(a, c);
        let blind = is_blind(a, c);
        match i {
            0 | 1 if door => None,
            3 if door => None,
            0 => Some((Surf::Plain, stone())),
            2 | 5 if !blind => None,
            3 | 7 => Some((Surf::Plain, teal())),
            _ => Some((Surf::Plain, cream())),
        }
    };
    // Stone, enamel and the rest are drawn in passes so each keeps its
    // texture.
    for pass in 0..2 {
        if pass == 0 {
            cast(b, near);
        } else {
            enamel(b, near);
        }
        b.grid(&rows, true, true, |i, j| {
            let p = paint_out(i, j)?;
            ((i == 0) == (pass == 0)).then_some(p)
        });
    }
    // Inner face, up to the ceiling.
    let yi = [0.0, 0.12, 0.9, 2.9, 3.4, 4.2, 5.4, CEILING];
    let rows_in: Vec<Vec<DVec3>> = yi.iter().map(|&y| ring(&rims, -WALL_T, y)).collect();
    enamel(b, near);
    b.grid(&rows_in, true, false, |i, j| {
        let (a, c) = (&rims[j], &rims[next(j)]);
        let door = is_door(a, c);
        let blind = is_blind(a, c);
        match i {
            0..=2 if door => None,
            0 => Some((Surf::Plain, rgb(30, 96, 100))),
            1 => Some((Surf::Plain, mint())),
            2 | 5 if !blind => None,
            2 => Some((Surf::Plain, mint())),
            3 => Some((Surf::Plain, mint())),
            _ => Some((Surf::Plain, cream())),
        }
    });
    // The wall's top between the faces, under a chrome cap.
    b.surf = Surf::Metal;
    b.tex = None;
    let cap = [
        ring(&rims, 0.04, PARAPET),
        ring(&rims, 0.04, PARAPET + 0.06),
        ring(&rims, -WALL_T - 0.04, PARAPET + 0.06),
        ring(&rims, -WALL_T - 0.04, PARAPET),
    ];
    b.grid(&cap, true, true, |_, _| Some((Surf::Metal, chrome())));
    // Sills, heads and jambs round the windows, glass in the middle of the
    // wall, chrome mullions.
    enamel(b, near);
    for (y0, y1) in WINDOWS {
        for j in 0..n {
            let (a, c) = (&rims[j], &rims[next(j)]);
            if is_blind(a, c) || (y0 < DOOR_H && is_door(a, c)) {
                continue;
            }
            let at = |r: &Rim, off: f64, y: f64| {
                let p = r.p + r.out * off;
                DVec3::new(p.x, y, p.y)
            };
            // Sill (up), head (down).
            b.quad(
                [
                    at(a, 0.0, y0),
                    at(c, 0.0, y0),
                    at(c, -WALL_T, y0),
                    at(a, -WALL_T, y0),
                ],
                DVec3::Y,
                cream(),
            );
            b.quad(
                [
                    at(a, 0.0, y1),
                    at(c, 0.0, y1),
                    at(c, -WALL_T, y1),
                    at(a, -WALL_T, y1),
                ],
                -DVec3::Y,
                cream(),
            );
        }
    }
    // The panes, facing out and in (two sheets a hair apart).
    for (y0, y1) in WINDOWS {
        for (off, out) in [
            (-WALL_T * 0.5 + 0.007, true),
            (-WALL_T * 0.5 - 0.007, false),
        ] {
            let rows = [ring(&rims, off, y0), ring(&rims, off, y1)];
            b.grid(&rows, true, out, |_, j| {
                let (a, c) = (&rims[j], &rims[next(j)]);
                if is_blind(a, c) || (y0 < DOOR_H && is_door(a, c)) {
                    None
                } else {
                    let lit = Surf::Lit(crate::daylight::channel(crate::daylight::Tone::Cool, 4));
                    Some((if out { lit } else { Surf::Glass }, glass().with_alpha(0.4)))
                }
            });
        }
    }
    if near {
        b.surf = Surf::Metal;
        b.tex = None;
        for (y0, y1) in WINDOWS {
            for j in 0..n {
                let (p, a, c) = (&rims[(j + n - 1) % n], &rims[j], &rims[next(j)]);
                let open = |x: &Rim, y: &Rim| !is_blind(x, y) && !(y0 < DOOR_H && is_door(x, y));
                // Every other point on the straights, every point round the
                // ends: a mullion where windows meet.
                if !(open(p, a) && open(a, c)) || (a.side % 2 == 0 && j % 2 == 1) {
                    continue;
                }
                let m = a.p - a.out * (WALL_T * 0.5);
                let along = DVec3::new(-a.out.y, 0.0, a.out.x);
                b.oriented_box(
                    DVec3::new(m.x, (y0 + y1) * 0.5, m.y),
                    along,
                    DVec3::new(0.04, (y1 - y0) * 0.5, 0.05),
                    chrome(),
                );
            }
        }
    }
    // Eyebrow ledges over both window bands: thin teal shelves round the
    // whole hall (but not across the door's opening below the canopy).
    enamel(b, near);
    for y in [3.0, 5.5] {
        let reach = if y < 4.0 { 0.42 } else { 0.3 };
        let prof = [
            (0.0, y - 0.06),
            (reach, y - 0.06),
            (reach, y + 0.06),
            (0.0, y + 0.06),
        ];
        let rows: Vec<Vec<DVec3>> = prof.iter().map(|&(o, yy)| ring(&rims, o, yy)).collect();
        b.grid(&rows, true, true, |_, j| {
            let (a, c) = (&rims[j], &rims[next(j)]);
            (!(y < 4.0 && is_door(a, c))).then_some((Surf::Plain, teal()))
        });
    }
    // The doorway's jambs and lintel's underside.
    let front = HALL_Z + HALL_R;
    for s in [-1.0, 1.0] {
        let x = s * DOOR_HALF;
        b.quad(
            [
                DVec3::new(x, 0.0, front),
                DVec3::new(x, 0.0, front - WALL_T),
                DVec3::new(x, DOOR_H, front - WALL_T),
                DVec3::new(x, DOOR_H, front),
            ],
            DVec3::new(-s, 0.0, 0.0),
            cream(),
        );
    }
    b.quad(
        [
            DVec3::new(-DOOR_HALF, DOOR_H, front),
            DVec3::new(DOOR_HALF, DOOR_H, front),
            DVec3::new(DOOR_HALF, DOOR_H, front - WALL_T),
            DVec3::new(-DOOR_HALF, DOOR_H, front - WALL_T),
        ],
        -DVec3::Y,
        cream(),
    );
    if near {
        // A chrome frame round the doorway.
        b.surf = Surf::Metal;
        b.tex = None;
        let z = front + 0.05;
        tube(
            b,
            &[
                DVec3::new(-DOOR_HALF - 0.08, 0.0, z),
                DVec3::new(-DOOR_HALF - 0.08, DOOR_H + 0.08, z),
                DVec3::new(DOOR_HALF + 0.08, DOOR_H + 0.08, z),
                DVec3::new(DOOR_HALF + 0.08, 0.0, z),
            ],
            (0.07, 0.07),
            10,
            Some((Surf::Metal, chrome())),
        );
    }
    plain(b);
}

/// The ceiling, the roof's top, the vault's well and the glass vault.
fn roof(b: &mut MeshBuilder, near: bool) {
    let inner = HALL_R - WALL_T;
    let (vz0, vz1) = VAULT_Z;
    let back = HALL_Z - inner;
    let front = HALL_Z + inner;
    let rects = [
        (-HALL_A, HALL_A, vz1, front),
        (-HALL_A, HALL_A, back, vz0),
        (-HALL_A, -VAULT_A, vz0, vz1),
        (VAULT_A, HALL_A, vz0, vz1),
    ];
    let arc = if near { 18 } else { 8 };
    for (y, up) in [(CEILING, false), (ROOF, true)] {
        if up {
            cast(b, near);
        } else {
            plain(b);
        }
        let color = if up {
            rgb(176, 178, 172)
        } else {
            rgb(246, 242, 232)
        };
        let n = if up { DVec3::Y } else { -DVec3::Y };
        for (x0, x1, z0, z1) in rects {
            b.quad(
                [
                    DVec3::new(x0, y, z0),
                    DVec3::new(x1, y, z0),
                    DVec3::new(x1, y, z1),
                    DVec3::new(x0, y, z1),
                ],
                n,
                color,
            );
        }
        for s in [-1.0, 1.0] {
            let c = DVec2::new(s * HALL_A, HALL_Z);
            let mut pts = vec![DVec3::new(c.x, y, c.y)];
            for j in 0..=arc {
                let t = -FRAC_PI_2 + PI * j as f64 / arc as f64;
                let p = c + DVec2::new(s * t.cos(), t.sin()) * inner;
                pts.push(DVec3::new(p.x, y, p.y));
            }
            b.polygon(&pts, n, color);
        }
    }
    // The vault's well, from the ceiling up to the roof.
    plain(b);
    let well = |b: &mut MeshBuilder, a: DVec2, c: DVec2, n: DVec3| {
        b.quad(
            [
                DVec3::new(a.x, CEILING, a.y),
                DVec3::new(c.x, CEILING, c.y),
                DVec3::new(c.x, ROOF, c.y),
                DVec3::new(a.x, ROOF, a.y),
            ],
            n,
            rgb(240, 236, 224),
        );
    };
    well(
        b,
        DVec2::new(-VAULT_A, vz0),
        DVec2::new(VAULT_A, vz0),
        DVec3::Z,
    );
    well(
        b,
        DVec2::new(-VAULT_A, vz1),
        DVec2::new(VAULT_A, vz1),
        -DVec3::Z,
    );
    well(
        b,
        DVec2::new(-VAULT_A, vz0),
        DVec2::new(-VAULT_A, vz1),
        DVec3::X,
    );
    well(
        b,
        DVec2::new(VAULT_A, vz0),
        DVec2::new(VAULT_A, vz1),
        -DVec3::X,
    );
    // The glass vault: a half ellipse across the well, ribbed in chrome.
    let cols = if near { 16 } else { 6 };
    let (zc, hz) = ((vz0 + vz1) * 0.5, (vz1 - vz0) * 0.5);
    let arc_at = |x: f64, k: usize, sink: f64| {
        let t = PI * k as f64 / cols as f64;
        DVec3::new(
            x,
            ROOF + (VAULT_RISE - sink) * t.sin(),
            zc - (hz - sink) * t.cos(),
        )
    };
    for (sink, out) in [(0.0, true), (0.012, false)] {
        let rows: Vec<Vec<DVec3>> = [-VAULT_A, VAULT_A]
            .iter()
            .map(|&x| (0..=cols).map(|k| arc_at(x, k, sink)).collect())
            .collect();
        // Rows run along +x, columns over the arch from front to back:
        // (+x) × (over) points down at the crown, so flip for out.
        b.grid(&rows, false, !out, |_, _| {
            Some((Surf::Glass, rgb(150, 200, 210).with_alpha(0.3)))
        });
        for s in [-1.0, 1.0] {
            let pts: Vec<DVec3> = (0..=cols).map(|k| arc_at(s * VAULT_A, k, sink)).collect();
            let n = if out { s } else { -s };
            b.surf = Surf::Glass;
            b.polygon(
                &pts,
                DVec3::new(n, 0.0, 0.0),
                rgb(150, 200, 210).with_alpha(0.3),
            );
        }
    }
    if near {
        let ribs = 9;
        for i in 0..ribs {
            let x = -VAULT_A + 2.0 * VAULT_A * i as f64 / (ribs - 1) as f64;
            let pts: Vec<DVec3> = (0..=cols).map(|k| arc_at(x, k, -0.03)).collect();
            tube(b, &pts, (0.045, 0.045), 8, Some((Surf::Metal, chrome())));
        }
        // Round ceiling lights, clear of the vault.
        b.surf = Surf::Glow;
        b.tex = None;
        for x in [-14.0, -10.0, -6.0, -2.0, 2.0, 6.0, 10.0, 14.0] {
            for z in [-0.6, -11.2] {
                if inside_depth(DVec2::new(x, z)) < 1.0 {
                    continue;
                }
                let c = DVec3::new(x, CEILING - 0.012, z);
                let pts: Vec<DVec3> = (0..20)
                    .map(|k| {
                        let a = TAU * k as f64 / 20.0;
                        c + DVec3::new(a.cos() * 0.55, 0.0, a.sin() * 0.55)
                    })
                    .collect();
                b.polygon(&pts, -DVec3::Y, rgb(255, 246, 226));
            }
        }
        // The roof's edge round the vault: a low chrome kerb.
        lathe_box(
            b,
            DVec3::new(0.0, ROOF, zc),
            DVec3::new(VAULT_A + 0.08, 0.1, hz + 0.08),
            chrome(),
        );
    }
    plain(b);
}

/// A thin chrome frame round a rectangle (a kerb), as four boxes.
fn lathe_box(b: &mut MeshBuilder, c: DVec3, half: DVec3, color: Color) {
    b.surf = Surf::Metal;
    b.tex = None;
    let t = 0.06;
    for s in [-1.0, 1.0] {
        b.oriented_box(
            c + DVec3::new(0.0, half.y, s * half.z),
            DVec3::X,
            DVec3::new(half.x, half.y, t),
            color,
        );
        b.oriented_box(
            c + DVec3::new(s * half.x, half.y, 0.0),
            DVec3::Z,
            DVec3::new(half.z, half.y, t),
            color,
        );
    }
    plain(b);
}

fn tower(b: &mut MeshBuilder, near: bool) {
    let seg = if near { 56 } else { 18 };
    let at = DVec3::new(0.0, 0.0, TOWER_Z);
    // Storeys of ribbon windows over a solid foot (behind the hall).
    let r = TOWER_R;
    let mut prof: Vec<(f64, f64)> = vec![(r + 0.12, 0.0), (r + 0.12, 0.5), (r, 0.5), (r, 7.6)];
    let mut kinds = vec![0u8, 0, 1];
    let mut y = 7.6;
    while y + 3.4 <= TOWER_TOP - 0.6 {
        // A teal band, the windows set in, then cream.
        prof.extend([
            (r, y + 0.25),
            (r - 0.08, y + 0.25),
            (r - 0.08, y + 1.85),
            (r, y + 1.85),
            (r, y + 3.4),
        ]);
        kinds.extend([2, 1, 3, 1, 1]);
        y += 3.4;
    }
    prof.extend([(r, TOWER_TOP), (0.0, TOWER_TOP)]);
    kinds.extend([1, 1]);
    // Two passes, so the stone and the enamel keep their own textures.
    for pass in 0..2 {
        if pass == 0 {
            cast(b, near);
        } else {
            enamel(b, near);
        }
        let k2 = kinds.clone();
        lathe(b, at, DVec3::Y, &prof, seg, |k| {
            let kind = k2.get(k).copied().unwrap_or(1);
            match (pass, kind) {
                (0, 0) => Some((Surf::Plain, stone())),
                (1, 1) => Some((Surf::Plain, cream())),
                (1, 2) => Some((Surf::Plain, teal())),
                (1, 3) => Some((
                    Surf::Lit(crate::daylight::channel(crate::daylight::Tone::Cool, 4)),
                    glass(),
                )),
                _ => None,
            }
        });
    }
    // Chrome mullions on the window bands.
    if near {
        b.surf = Surf::Metal;
        b.tex = None;
        let mut y = 7.6;
        while y + 3.4 <= TOWER_TOP - 0.6 {
            for k in 0..24 {
                let a = TAU * k as f64 / 24.0;
                let out = DVec3::new(a.sin(), 0.0, a.cos());
                b.oriented_box(
                    at + out * (r - 0.04) + DVec3::Y * (y + 1.05),
                    DVec3::new(out.z, 0.0, -out.x),
                    DVec3::new(0.035, 0.8, 0.06),
                    chrome(),
                );
            }
            y += 3.4;
        }
    }
    // The fins either side, rising past the deck, with chrome caps.
    enamel(b, near);
    for sx in [-1.0, 1.0] {
        let (y0, y1) = FIN_Y;
        let rows: Vec<Vec<DVec3>> = (0..=8)
            .map(|i| {
                let u = i as f64 / 8.0;
                let y = y0 + (y1 - y0) * u;
                // Swept: widest low down, narrowing to a point at the top.
                let out = TOWER_R + (FIN_OUT - TOWER_R) * (1.0 - u.powf(2.2)).max(0.05);
                let z = TOWER_Z;
                let t = FIN_T * 0.5;
                vec![
                    DVec3::new(sx * (TOWER_R - 0.1), y, z - t),
                    DVec3::new(sx * out, y, z - t),
                    DVec3::new(sx * out, y, z + t),
                    DVec3::new(sx * (TOWER_R - 0.1), y, z + t),
                ]
            })
            .collect();
        b.grid(&rows, false, sx < 0.0, |_, j| {
            (j < 3).then_some((Surf::Plain, teal()))
        });
        // The fin's foot (its bottom edge).
        let r0 = &rows[0];
        b.quad([r0[0], r0[1], r0[2], r0[3]], -DVec3::Y, teal());
    }
    // The drone deck: a saucer on the tower.
    let deck = at + DVec3::Y * TOWER_TOP;
    enamel(b, near);
    lathe(
        b,
        deck,
        DVec3::Y,
        &[
            (TOWER_R - 0.2, -0.01),
            (DECK_R - 0.6, 0.3),
            (DECK_R, 0.55),
            (DECK_R + 0.08, 0.75),
            (DECK_R - 0.05, 0.95),
            (DECK_R - 0.35, DECK_T),
            (0.0, DECK_T),
        ],
        seg,
        |k| match k {
            2 | 3 => Some((Surf::Plain, teal())),
            5 => Some((Surf::Plain, rgb(120, 126, 130))),
            _ => Some((Surf::Plain, cream())),
        },
    );
    // Its landing ring and the lights round its rim.
    b.surf = Surf::Glow;
    b.tex = None;
    let rows = [
        disc_ring(deck + DVec3::Y * (DECK_T + 0.012), 3.4, seg),
        disc_ring(deck + DVec3::Y * (DECK_T + 0.012), 3.7, seg),
    ];
    b.grid(&rows, true, false, |_, _| Some((Surf::Glow, heal())));
    if near {
        for k in 0..16 {
            let a = TAU * k as f64 / 16.0;
            let p = deck + DVec3::new(a.sin(), 0.0, a.cos()) * (DECK_R - 0.5) + DVec3::Y * DECK_T;
            b.oriented_box(
                p + DVec3::Y * 0.05,
                DVec3::X,
                DVec3::new(0.1, 0.05, 0.1),
                rgb(255, 236, 190),
            );
        }
    }
    // The pylon, chrome rings, and the emblem: a gyroscope of two chrome
    // rings round a glowing three-way cross, a globe on top.
    let foot = deck + DVec3::Y * DECK_T;
    let h = PYLON_TOP - TOWER_TOP - DECK_T;
    enamel(b, near);
    lathe(
        b,
        foot,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.75, 0.0),
            (0.5, 1.6),
            (0.22, h - 3.0),
            (0.14, h - 0.4),
            (0.0, h - 0.3),
        ],
        if near { 24 } else { 8 },
        |_| Some((Surf::Plain, cream())),
    );
    for (r, y) in [(0.66, 0.8), (0.42, 3.6), (0.33, 6.0)] {
        lathe(
            b,
            foot + DVec3::Y * y,
            DVec3::Y,
            &[
                (0.0, 0.0),
                (r, 0.0),
                (r + 0.05, 0.06),
                (r + 0.05, 0.16),
                (r, 0.22),
                (0.0, 0.22),
            ],
            if near { 24 } else { 8 },
            |_| Some((Surf::Metal, chrome())),
        );
    }
    emblem(b, foot + DVec3::Y * (h - 3.4), 1.7, near);
    plain(b);
}

/// A ring of points round `c` (horizontal), radius `r`.
fn disc_ring(c: DVec3, r: f64, seg: usize) -> Vec<DVec3> {
    (0..seg)
        .map(|k| {
            let a = TAU * k as f64 / seg as f64;
            c + DVec3::new(a.sin() * r, 0.0, a.cos() * r)
        })
        .collect()
}

/// The hospital's sign: two chrome rings crossed like a gyroscope round a
/// glowing cross with arms every way, a globe on top.
pub fn emblem(b: &mut MeshBuilder, c: DVec3, r: f64, near: bool) {
    let seg = if near { 40 } else { 12 };
    for axis in [DVec3::Z, DVec3::X] {
        let side = DVec3::Y.cross(axis);
        let pts: Vec<DVec3> = (0..=seg)
            .map(|k| {
                let a = TAU * k as f64 / seg as f64;
                c + side * (a.cos() * r) + DVec3::Y * (a.sin() * r)
            })
            .collect();
        tube(
            b,
            &pts,
            (r * 0.08, r * 0.08),
            if near { 10 } else { 5 },
            Some((Surf::Metal, chrome())),
        );
    }
    b.surf = Surf::Glow;
    b.tex = None;
    let arm = r * 0.62;
    let w = r * 0.2;
    b.oriented_box(c, DVec3::X, DVec3::new(arm, w, w), heal());
    b.oriented_box(c, DVec3::X, DVec3::new(w, arm, w), heal());
    b.oriented_box(c, DVec3::X, DVec3::new(w, w, arm), heal());
    // The globe.
    b.dome(
        c + DVec3::Y * (r + 0.05),
        r * 0.22,
        if near { 16 } else { 6 },
        4,
        aether(),
    );
    b.dome(
        c + DVec3::Y * (r + 0.05),
        -r * 0.22,
        if near { 16 } else { 6 },
        4,
        aether(),
    );
    plain(b);
}

fn canopy(b: &mut MeshBuilder, near: bool) {
    let front = HALL_Z + HALL_R;
    let (z0, z1) = (front - 0.05, front + CANOPY_REACH);
    let rr = pieces::rrect(CANOPY_HALF * 2.0, z1 - z0, 1.4, if near { 6 } else { 2 });
    let mid = (z0 + z1) * 0.5;
    let pts: Vec<DVec2> = rr.iter().map(|p| DVec2::new(p.x, p.y + mid)).collect();
    enamel(b, near);
    b.prism(&pts, CANOPY_Y, CANOPY_Y + 0.3, cream());
    // A teal fascia band round its edge, standing a little proud.
    let band: Vec<DVec2> = pieces::rrect(
        CANOPY_HALF * 2.0 + 0.06,
        z1 - z0 + 0.06,
        1.43,
        if near { 6 } else { 2 },
    )
    .iter()
    .map(|p| DVec2::new(p.x, p.y + mid))
    .collect();
    b.prism(&band, CANOPY_Y + 0.08, CANOPY_Y + 0.22, teal());
    // Its columns: tapering chrome with a teal sleeve.
    for (x, z) in CANOPY_COLS {
        lathe(
            b,
            DVec3::new(x, 0.0, z),
            DVec3::Y,
            &[
                (0.26, 0.0),
                (0.26, 0.12),
                (0.2, 0.2),
                (0.13, CANOPY_Y),
                (0.0, CANOPY_Y),
            ],
            if near { 20 } else { 8 },
            |k| {
                Some(if k == 0 {
                    (Surf::Plain, teal())
                } else {
                    (Surf::Metal, chrome())
                })
            },
        );
    }
    if near {
        // Downlights under it.
        b.surf = Surf::Glow;
        b.tex = None;
        for (x, z) in [(-1.8, 2.6), (1.8, 2.6), (-1.8, 5.0), (1.8, 5.0), (0.0, 3.8)] {
            let c = DVec3::new(x, CANOPY_Y - 0.012, front + z - 1.0);
            let pts: Vec<DVec3> = (0..14)
                .map(|k| {
                    let a = TAU * k as f64 / 14.0;
                    c + DVec3::new(a.cos() * 0.22, 0.0, a.sin() * 0.22)
                })
                .collect();
            b.polygon(&pts, -DVec3::Y, rgb(255, 240, 210));
        }
    }
    // The sign on its front edge.
    emblem(b, DVec3::new(0.0, CANOPY_Y + 1.25, z1 - 0.9), 0.85, near);
    plain(b);
}

/// The drone bay: a landing ring on the plaza, lights round it.
fn bay(b: &mut MeshBuilder, near: bool) {
    let c = DVec3::new(BAY.0, 0.012, BAY.1);
    let seg = if near { 40 } else { 12 };
    plain(b);
    let disc: Vec<DVec3> = disc_ring(c, BAY_R - 0.3, seg);
    b.polygon(&disc, DVec3::Y, rgb(232, 230, 220));
    let rows = [disc_ring(c, BAY_R - 0.3, seg), disc_ring(c, BAY_R, seg)];
    b.grid(&rows, true, false, |_, _| Some((Surf::Plain, teal())));
    // The cross painted in the middle, a hair above the disc.
    b.oriented_box(
        c + DVec3::Y * 0.01,
        DVec3::X,
        DVec3::new(1.0, 0.012, 0.26),
        teal(),
    );
    b.oriented_box(
        c + DVec3::Y * 0.01,
        DVec3::X,
        DVec3::new(0.26, 0.012, 1.0),
        teal(),
    );
    if near {
        b.surf = Surf::Glow;
        for k in 0..8 {
            let a = TAU * k as f64 / 8.0;
            let p = c + DVec3::new(a.sin(), 0.0, a.cos()) * (BAY_R + 0.25);
            b.dome(p, 0.1, 10, 3, heal());
        }
    }
    plain(b);
}

/// Planters either side of the entrance, with shrubs.
fn planters(b: &mut MeshBuilder) {
    let front = HALL_Z + HALL_R;
    for (k, x) in [-6.2f64, 6.2].into_iter().enumerate() {
        let mut m = Maker::new(
            b,
            DVec3::new(x, 0.0, front + 2.2),
            DVec3::Z,
            24,
            Wear::Fresh,
        );
        m.lathe(
            Mat::Paint(rgb(230, 226, 214)),
            DVec3::ZERO,
            &[(0.6, 0.0), (1.05, 0.55), (1.1, 0.62), (0.0, 0.62)],
        );
        pieces::plant(
            &mut m.sub(DVec3::new(0.0, 0.62, 0.0), 0.0),
            1.4,
            teal(),
            11 + k as u64,
        );
    }
}

/// The Recovery Hall's floor and furniture.
fn inside(b: &mut MeshBuilder) {
    let inner = HALL_R - WALL_T;
    // Terrazzo floor a centimetre over the plinth.
    b.surf = Surf::Plain;
    b.tex = Some(Tex::from(Skin::new(Base::Terrazzo, Wear::Fresh)));
    let y = 0.012;
    let floor = rgb(232, 228, 216);
    b.quad(
        [
            DVec3::new(-HALL_A, y, HALL_Z - inner),
            DVec3::new(HALL_A, y, HALL_Z - inner),
            DVec3::new(HALL_A, y, HALL_Z + inner),
            DVec3::new(-HALL_A, y, HALL_Z + inner),
        ],
        DVec3::Y,
        floor,
    );
    for s in [-1.0, 1.0] {
        let c = DVec2::new(s * HALL_A, HALL_Z);
        let mut pts = vec![DVec3::new(c.x, y, c.y)];
        for j in 0..=18 {
            let t = -FRAC_PI_2 + PI * j as f64 / 18.0;
            let p = c + DVec2::new(s * t.cos(), t.sin()) * inner;
            pts.push(DVec3::new(p.x, y, p.y));
        }
        b.polygon(&pts, DVec3::Y, floor);
    }
    // The doorway's threshold, over the plinth.
    b.quad(
        [
            DVec3::new(-DOOR_HALF, y, HALL_Z + inner),
            DVec3::new(DOOR_HALF, y, HALL_Z + inner),
            DVec3::new(DOOR_HALF, y, HALL_Z + HALL_R + 0.02),
            DVec3::new(-DOOR_HALF, y, HALL_Z + HALL_R + 0.02),
        ],
        DVec3::Y,
        floor,
    );
    // A teal ring inlaid round the desk.
    plain(b);
    let c = DVec3::new(0.0, y + 0.012, DESK_Z);
    let rows = [disc_ring(c, 3.6, 48), disc_ring(c, 3.85, 48)];
    b.grid(&rows, true, false, |_, _| Some((Surf::Plain, teal())));

    let back = HALL_Z - HALL_R + WALL_T;
    // The pods.
    for x in PODS_X {
        pod(b, DVec3::new(x, 0.0, POD_Z));
    }
    // The healing core on the back wall.
    core(b, DVec3::new(0.0, 0.0, back));
    // The reception desk, facing the door.
    {
        let mut m = Maker::new(b, DVec3::new(0.0, 0.0, DESK_Z), DVec3::Z, 24, Wear::Fresh);
        let outline = pieces::kidney(DESK_HALF.0 * 2.0, DESK_HALF.1 * 2.0, 40);
        m.slab(Mat::Paint(mint()), &outline, 0.0, DESK_H - 0.06);
        let top: Vec<DVec2> = pieces::kidney(DESK_HALF.0 * 2.0 + 0.1, DESK_HALF.1 * 2.0 + 0.1, 40);
        m.slab(Mat::Plain(rgb(250, 248, 240)), &top, DESK_H - 0.06, DESK_H);
        // A chrome band round its front.
        let band: Vec<DVec2> =
            pieces::kidney(DESK_HALF.0 * 2.0 + 0.03, DESK_HALF.1 * 2.0 + 0.03, 40);
        m.slab(Mat::Chrome, &band, 0.62, 0.7);
        pieces::telephone(&mut m, DVec3::new(-1.3, DESK_H, -0.05));
        pieces::typewriter(&mut m, DVec3::new(0.9, DESK_H, -0.08));
        let mut chair = m.sub(DVec3::new(0.4, 0.0, -1.1), 0.0);
        pieces::swivel_chair(
            &mut chair,
            &Cover {
                main: teal(),
                second: mint(),
                wood: rgb(120, 80, 50),
            },
        );
    }
    // The glowing cross on its front.
    b.surf = Surf::Glow;
    b.tex = None;
    let fz = DESK_Z + DESK_HALF.1 + 0.07;
    b.oriented_box(
        DVec3::new(0.0, 0.62, fz),
        DVec3::X,
        DVec3::new(0.22, 0.06, 0.012),
        heal(),
    );
    b.oriented_box(
        DVec3::new(0.0, 0.62, fz),
        DVec3::X,
        DVec3::new(0.06, 0.22, 0.012),
        heal(),
    );
    plain(b);
    // The waiting corner: two rows of chairs facing each other over a
    // kidney table, a water cooler, plants.
    let cov = Cover {
        main: rgb(214, 96, 70),
        second: rgb(240, 224, 196),
        wood: rgb(150, 104, 62),
    };
    for (x, face) in [(-15.2, 1.0), (-12.0, -1.0)] {
        for k in 0..4 {
            let z = -8.1 + k as f64 * 1.2;
            let mut m = Maker::new(
                b,
                DVec3::new(x, 0.0, z),
                DVec3::new(face, 0.0, 0.0),
                16,
                Wear::Fresh,
            );
            pieces::chair(&mut m, &cov);
        }
    }
    {
        let mut m = Maker::new(b, DVec3::new(-13.6, 0.0, -6.3), DVec3::X, 24, Wear::Fresh);
        let top = pieces::kidney(0.7, 1.6, 28);
        let legs = [
            DVec2::new(-0.2, -0.6),
            DVec2::new(0.2, 0.0),
            DVec2::new(-0.2, 0.6),
        ];
        pieces::coffee_table(&mut m, &top, rgb(150, 104, 62), &legs);
    }
    {
        let mut m = Maker::new(b, DVec3::new(-13.0, 0.0, -11.0), DVec3::Z, 16, Wear::Fresh);
        pieces::water_cooler(&mut m);
    }
    for (k, (x, z)) in [(-14.6, -2.3), (-10.4, -11.6), (10.4, -11.6), (14.2, -10.2)]
        .into_iter()
        .enumerate()
    {
        let mut m = Maker::new(b, DVec3::new(x, 0.0, z), DVec3::Z, 16, Wear::Fresh);
        pieces::plant(&mut m, 1.3, teal(), 40 + k as u64);
    }
    // Examination couches under their scanner arms.
    for (x, z, a) in COUCHES {
        let d = couch_dir(a);
        couch(b, DVec3::new(x, 0.0, z), DVec3::new(d.x, 0.0, d.y));
    }
    plain(b);
}

/// An upright recovery pod: chrome base and cap, a glass tube, a healing
/// glow in its floor and round its cap, three chrome bars at the back.
fn pod(b: &mut MeshBuilder, at: DVec3) {
    let seg = 32;
    let top = POD_H;
    lathe(
        b,
        at,
        DVec3::Y,
        &[
            (POD_R, 0.0),
            (POD_R, 0.16),
            (POD_R - 0.06, 0.24),
            (POD_R - 0.1, 0.3),
            (0.0, 0.3),
        ],
        seg,
        |k| {
            Some(if k == 0 {
                (Surf::Plain, teal())
            } else {
                (Surf::Metal, chrome())
            })
        },
    );
    lathe(
        b,
        at + DVec3::Y * (top - 0.3),
        DVec3::Y,
        &[
            (POD_R - 0.1, -0.012),
            (POD_R, 0.06),
            (POD_R, 0.2),
            (POD_R - 0.2, 0.3),
            (0.0, 0.32),
        ],
        seg,
        |k| {
            Some(if k == 1 {
                (Surf::Plain, teal())
            } else {
                (Surf::Metal, chrome())
            })
        },
    );
    // Glass, seen from out and in.
    for (r, out) in [(POD_R - 0.08, true), (POD_R - 0.092, false)] {
        let rows = [
            disc_ring(at + DVec3::Y * 0.3, r, seg),
            disc_ring(at + DVec3::Y * (top - 0.3), r, seg),
        ];
        b.grid(&rows, true, !out, |_, _| {
            Some((Surf::Glass, rgb(170, 230, 220).with_alpha(0.22)))
        });
    }
    // The light: a disc in the base, a band under the cap.
    b.surf = Surf::Glow;
    b.tex = None;
    let floor = disc_ring(at + DVec3::Y * 0.312, POD_R - 0.18, seg);
    b.polygon(&floor, DVec3::Y, heal());
    let rows = [
        disc_ring(at + DVec3::Y * (top - 0.36), POD_R - 0.1, seg),
        disc_ring(at + DVec3::Y * (top - 0.312), POD_R - 0.1, seg),
    ];
    b.grid(&rows, true, false, |_, _| Some((Surf::Glow, heal())));
    // The bars, at the back.
    for a in [-0.9f64, 0.0, 0.9] {
        let out = DVec3::new(a.sin(), 0.0, -a.cos());
        let p = at + out * (POD_R - 0.03);
        tube(
            b,
            &[p + DVec3::Y * 0.28, p + DVec3::Y * (top - 0.28)],
            (0.035, 0.035),
            8,
            Some((Surf::Metal, chrome())),
        );
    }
    plain(b);
}

/// The healing core: a console on the floor against the back wall, a
/// chrome ring over it round a glowing disc, two glass tubes of light.
fn core(b: &mut MeshBuilder, wall: DVec3) {
    let mut m = Maker::new(b, wall, DVec3::Z, 32, Wear::Fresh);
    let d = CORE_DEPTH;
    m.block(
        Mat::Paint(mint()),
        DVec3::new(-CORE_HALF, 0.0, 0.0),
        DVec3::new(CORE_HALF, 1.0, d),
    );
    m.block(
        Mat::Chrome,
        DVec3::new(-CORE_HALF - 0.02, 1.0, -0.0),
        DVec3::new(CORE_HALF + 0.02, 1.06, d + 0.02),
    );
    // Dials on the console's front.
    for k in 0..5 {
        let x = -1.0 + k as f64 * 0.5;
        m.lathe_along(
            Mat::Chrome,
            DVec3::new(x, 0.6, d),
            DVec3::Z,
            &[(0.13, 0.0), (0.13, 0.03), (0.0, 0.03)],
        );
        m.lathe_along(
            Mat::Glow(rgb(255, 214, 120)),
            DVec3::new(x, 0.6, d + 0.03),
            DVec3::Z,
            &[(0.09, 0.0), (0.0, 0.012)],
        );
    }
    // The ring and its light.
    let c = DVec3::new(0.0, 2.75, 0.18);
    m.lathe_along(
        Mat::Chrome,
        c,
        DVec3::Z,
        &[
            (1.0, 0.0),
            (1.38, 0.0),
            (1.42, 0.12),
            (1.38, 0.26),
            (1.1, 0.3),
            (1.0, 0.3),
        ],
    );
    m.lathe_along(Mat::Glow(heal()), c, DVec3::Z, &[(1.0, 0.1), (0.0, 0.18)]);
    m.lathe_along(
        Mat::Glow(aether()),
        c + DVec3::Z * 0.18,
        DVec3::Z,
        &[(0.35, 0.0), (0.2, 0.14), (0.0, 0.16)],
    );
    // Tubes of light up either side.
    for sx in [-1.0, 1.0] {
        let x = sx * (CORE_HALF - 0.2);
        m.lathe(
            Mat::Chrome,
            DVec3::new(x, 1.06, 0.25),
            &[(0.12, 0.0), (0.12, 0.08), (0.0, 0.08)],
        );
        m.lathe(
            Mat::Glow(heal()),
            DVec3::new(x, 1.14, 0.25),
            &[(0.07, 0.0), (0.07, CORE_TOP - 1.3), (0.0, CORE_TOP - 1.3)],
        );
        m.lathe(
            Mat::Chrome,
            DVec3::new(x, CORE_TOP - 0.16, 0.25),
            &[(0.12, 0.0), (0.12, 0.12), (0.0, 0.16)],
        );
    }
}

/// An examination couch (its head towards `head`), with a scanner arm
/// arching over it.
fn couch(b: &mut MeshBuilder, at: DVec3, head: DVec3) {
    // Maker's front is its +z: the couch's foot.
    let mut m = Maker::new(b, at, -head, 24, Wear::Fresh);
    let (hw, hl) = COUCH_HALF;
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        m.leg(
            Mat::Chrome,
            sx * (hw - 0.08),
            sz * (hl - 0.12),
            0.56,
            (0.02, 0.025),
            DVec2::ZERO,
        );
    }
    m.block(
        Mat::Chrome,
        DVec3::new(-hw + 0.02, 0.52, -hl + 0.04),
        DVec3::new(hw - 0.02, 0.58, hl - 0.04),
    );
    m.blob(
        Mat::Plain(mint()),
        DVec3::new(0.0, 0.66, 0.0),
        DVec3::new(hw - 0.02, 0.09, hl - 0.02),
        0.0,
        (0.2, 0.3),
    );
    m.blob(
        Mat::Cloth(rgb(250, 248, 242)),
        DVec3::new(0.0, 0.78, -hl + 0.3),
        DVec3::new(hw - 0.1, 0.06, 0.18),
        0.2,
        (0.45, 0.5),
    );
    // The scanner arm: from a foot beside the couch, up and over its
    // middle, ending in a glowing ring.
    let foot = DVec3::new(hw + 0.4, 0.0, -0.2);
    m.lathe(
        Mat::Paint(teal()),
        foot,
        &[(0.22, 0.0), (0.2, 0.1), (0.07, 0.2), (0.0, 0.2)],
    );
    let pts = [
        foot + DVec3::Y * 0.15,
        foot + DVec3::Y * 1.6,
        DVec3::new(hw * 0.4, 2.05, -0.2),
        DVec3::new(0.0, 1.9, -0.2),
    ];
    m.tube(Mat::Chrome, &pts, 0.05);
    let head_at = DVec3::new(0.0, 1.78, -0.2);
    m.lathe(
        Mat::Chrome,
        head_at,
        &[(0.28, 0.0), (0.32, 0.06), (0.3, 0.14), (0.0, 0.16)],
    );
    m.lathe(Mat::Glow(heal()), head_at, &[(0.0, -0.012), (0.24, -0.012)]);
}
