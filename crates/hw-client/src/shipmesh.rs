//! The harbour's ships, from the same numbers they collide by
//! ([`hw_gen::urban::harbour::ShipKind`]): the streamlined freighter, the
//! ocean liner, the tug and the motor launch, each in three levels.
//!
//! Golden-age streamline: hulls are smooth lofted shells (a fine raked
//! bow, a rounded cruiser stern, the bilge narrowing to the keel), the
//! upper works are decks with rounded fronts, overhanging white edges
//! and bands of windows, the funnels are raked teardrops with fins and
//! chrome rings, and a chrome speed spear sweeps up the hull to the bow.
//!
//! Frame: origin on the waterline in the middle of the ship's length, +z
//! to the bow, +x to starboard, y up (as [`ShipKind::blocks`]).

use crate::carmesh::{lathe, tube, Lod};
use crate::citymesh::{MeshBuilder, Surf};
use engine::prelude::*;
use glam::{DVec2, DVec3};
use hw_gen::urban::harbour::{deck_y, hull_half, stem_lean, Block, ShipKind};
use std::f64::consts::{PI, TAU};

/// One ship's colours.
#[derive(Clone, Copy)]
struct Livery {
    /// Below the waterline, the boot-topping band, the topsides.
    bottom: Color,
    boot: Color,
    hull: Color,
    /// The deck and the upper works.
    deck: Color,
    works: Color,
    /// The funnel, its band, its top.
    funnel: Color,
    band: Color,
    top: Color,
    /// The speed spear and trim.
    trim: Color,
}

fn livery(kind: ShipKind) -> Livery {
    match kind {
        ShipKind::Freighter => Livery {
            bottom: rgb(150, 44, 38),
            boot: rgb(236, 232, 222),
            hull: rgb(34, 58, 84),
            deck: rgb(132, 74, 58),
            works: rgb(244, 238, 226),
            funnel: rgb(236, 176, 52),
            band: rgb(30, 120, 140),
            top: rgb(32, 32, 36),
            trim: rgb(214, 216, 222),
        },
        ShipKind::Liner => Livery {
            bottom: rgb(156, 40, 40),
            boot: rgb(40, 40, 46),
            hull: rgb(246, 244, 238),
            deck: rgb(176, 140, 98),
            works: rgb(250, 248, 242),
            funnel: rgb(214, 46, 52),
            band: rgb(250, 246, 236),
            top: rgb(36, 36, 40),
            trim: rgb(56, 156, 196),
        },
        ShipKind::Tug => Livery {
            bottom: rgb(150, 44, 38),
            boot: rgb(30, 30, 34),
            hull: rgb(222, 72, 48),
            deck: rgb(120, 112, 104),
            works: rgb(246, 236, 210),
            funnel: rgb(246, 236, 210),
            band: rgb(222, 72, 48),
            top: rgb(32, 32, 36),
            trim: rgb(214, 216, 222),
        },
        ShipKind::Launch => Livery {
            bottom: rgb(60, 110, 150),
            boot: rgb(240, 238, 230),
            hull: rgb(80, 186, 196),
            deck: rgb(170, 120, 78),
            works: rgb(246, 242, 232),
            funnel: rgb(246, 242, 232),
            band: rgb(80, 186, 196),
            top: rgb(32, 32, 36),
            trim: rgb(214, 216, 222),
        },
    }
}

fn v(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z)
}

fn glass() -> Color {
    rgb(40, 58, 76)
}

/// Windows that light up at night (warm cabin lights).
fn pane() -> Surf {
    Surf::Lit(crate::daylight::channel(crate::daylight::Tone::Warm, 4))
}

/// Lengthwise stations from stern to bow, closer together at the ends.
fn stations(n: usize) -> Vec<f64> {
    (0..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            -0.5 * (PI * t).cos()
        })
        .collect()
}

/// The hull's shell, its deck and (near) its bulwarks.
fn hull(b: &mut MeshBuilder, kind: ShipKind, lv: &Livery, lod: Lod) {
    let d = kind.dims();
    let n_len = lod.pick(56, 24, 12);
    let us = stations(n_len);
    // Heights of the section's rows: the bottom, the boot-topping band,
    // the topsides, the deck edge.
    let levels = |u: f64| -> Vec<f64> {
        let top = deck_y(kind, u);
        let mut ys = vec![-d.draft];
        let below = lod.pick(4, 2, 1);
        for k in 1..=below {
            ys.push(-d.draft + (d.draft - 0.4) * k as f64 / (below + 1) as f64);
        }
        ys.push(-0.4);
        ys.push(0.3);
        let above = lod.pick(4, 2, 1);
        for k in 1..=above {
            ys.push(0.3 + (top - 0.3) * k as f64 / (above + 1) as f64);
        }
        ys.push(top);
        ys
    };
    let h_of = |u: f64, y: f64| (y + d.draft) / (deck_y(kind, u) + d.draft);
    let point = |u: f64, y: f64, side: f64| {
        let h = h_of(u, y);
        v(
            side * hull_half(kind, u, h),
            y,
            u * d.len + stem_lean(kind, u, h),
        )
    };
    let rows: Vec<Vec<DVec3>> = us
        .iter()
        .map(|&u| {
            let ys = levels(u);
            let mut row: Vec<DVec3> = ys.iter().rev().map(|&y| point(u, y, -1.0)).collect();
            row.push(v(0.0, -d.draft, u * d.len));
            row.extend(ys.iter().map(|&y| point(u, y, 1.0)));
            row
        })
        .collect();
    let paint = |i: usize, j: usize| {
        let y = (rows[i][j].y + rows[i][j + 1].y + rows[i + 1][j].y + rows[i + 1][j + 1].y) * 0.25;
        let c = if y < -0.4 + 1e-6 {
            lv.bottom
        } else if y < 0.3 {
            lv.boot
        } else {
            lv.hull
        };
        Some((Surf::Plain, c))
    };
    b.grid(&rows, false, false, paint);
    // The deck, with a little camber.
    let deck_rows: Vec<Vec<DVec3>> = us
        .iter()
        .map(|&u| {
            let y = deck_y(kind, u);
            let p = point(u, y, 1.0);
            vec![v(-p.x, y, p.z), v(0.0, y + p.x * 0.02, p.z), v(p.x, y, p.z)]
        })
        .collect();
    let deck = lv.deck;
    b.grid(&deck_rows, false, true, |_, _| Some((Surf::Plain, deck)));
    // Bulwarks: a white rail round the deck edge, both faces.
    if lod != Lod::Far {
        let rail = if kind == ShipKind::Liner {
            lv.hull
        } else {
            lv.works
        };
        let up = if kind == ShipKind::Launch { 0.45 } else { 1.0 };
        for side in [-1.0, 1.0] {
            let rows: Vec<Vec<DVec3>> = us
                .iter()
                .map(|&u| {
                    let p = point(u, deck_y(kind, u), side);
                    vec![p, p + DVec3::Y * up]
                })
                .collect();
            // Outer face: rows along +z, columns up: z × y = -x, so `out`
            // is true on the port side.
            b.grid(&rows, false, side < 0.0, |_, _| Some((Surf::Plain, rail)));
            b.grid(&rows, false, side > 0.0, |_, _| Some((Surf::Plain, rail)));
        }
    }
    // The chrome speed spear: a strip along the hull that sweeps up to
    // the bow, and a row of portholes under the deck.
    if lod != Lod::Far && kind != ShipKind::Launch {
        let trim = lv.trim;
        for side in [-1.0, 1.0] {
            let n = lod.pick(40, 16, 8);
            let strip: Vec<Vec<DVec3>> = (0..=n)
                .map(|k| {
                    let u = -0.38 + 0.84 * k as f64 / n as f64;
                    let rise = ((u + 0.1) / 0.56).clamp(0.0, 1.0);
                    let top = deck_y(kind, u);
                    let y = top * (0.55 + 0.33 * rise * rise);
                    let w = (0.06 + 0.035 * (top / 3.0)).min(0.4) * (1.0 - rise * 0.5);
                    let a = point(u, y - w, side) + DVec3::X * side * 0.03;
                    let c = point(u, y + w, side) + DVec3::X * side * 0.03;
                    vec![a, c]
                })
                .collect();
            b.grid(&strip, false, side < 0.0, |_, _| Some((Surf::Metal, trim)));
        }
        if lod == Lod::Near {
            let y = deck_y(kind, 0.0) - (d.free * 0.22).clamp(0.5, 1.4);
            let r = (d.free * 0.045).clamp(0.12, 0.3);
            let step = (r * 9.0).max(1.6);
            let n = (d.len * 0.5 / step) as i64;
            for k in -n..=n {
                let z = k as f64 * step;
                let u = z / d.len;
                if u.abs() > 0.28 {
                    continue;
                }
                for side in [-1.0, 1.0] {
                    let x = side * (hull_half(kind, u, h_of(u, y)) + 0.025);
                    crate::carmesh::disc(
                        b,
                        v(x, y, z),
                        (DVec3::Z * r, DVec3::Y * r),
                        8,
                        DVec3::X * side,
                        rgb(30, 36, 44),
                    );
                }
            }
        }
    }
}

/// A rounded rectangle in the ship's plan: x from -half to half, z from
/// z0 to z1, the front corners rounded by `rf`, the back ones by `rb`.
fn rounded(half: f64, z0: f64, z1: f64, rf: f64, rb: f64, seg: usize) -> Vec<DVec2> {
    let rf = rf.min(half).min((z1 - z0) * 0.5);
    let rb = rb.min(half).min((z1 - z0) * 0.5);
    let mut pts = Vec::new();
    let mut arc = |c: DVec2, r: f64, a0: f64| {
        if r < 1e-3 {
            pts.push(c);
            return;
        }
        for k in 0..=seg {
            let a = a0 + PI * 0.5 * k as f64 / seg as f64;
            pts.push(c + DVec2::new(a.cos(), a.sin()) * r);
        }
    };
    // Counter-clockwise seen from above with x right and z up the page:
    // front right, front left, back left, back right.
    arc(DVec2::new(half - rf, z1 - rf), rf, 0.0);
    arc(DVec2::new(-half + rf, z1 - rf), rf, PI * 0.5);
    arc(DVec2::new(-half + rb, z0 + rb), rb, PI);
    arc(DVec2::new(half - rb, z0 + rb), rb, PI * 1.5);
    pts
}

/// Walls round a plan outline from `y0` to `y1`, a flat roof on top.
fn prism(b: &mut MeshBuilder, pts: &[DVec2], y0: f64, y1: f64, wall: Color, roof: Color) {
    let n = pts.len();
    let c = pts.iter().sum::<DVec2>() / n as f64;
    for i in 0..n {
        let (a, e) = (pts[i], pts[(i + 1) % n]);
        if a.distance(e) < 1e-6 {
            continue;
        }
        let m = (a + e) * 0.5 - c;
        b.quad(
            [
                v(a.x, y0, a.y),
                v(e.x, y0, e.y),
                v(e.x, y1, e.y),
                v(a.x, y1, a.y),
            ],
            v(m.x, 0.0, m.y),
            wall,
        );
    }
    let top: Vec<DVec3> = pts.iter().map(|p| v(p.x, y1, p.y)).collect();
    b.polygon(&top, DVec3::Y, roof);
}

/// A thin slab round a plan outline (an overhanging deck edge).
fn slab(b: &mut MeshBuilder, pts: &[DVec2], y0: f64, y1: f64, col: Color) {
    prism(b, pts, y0, y1, col, col);
    let bottom: Vec<DVec3> = pts.iter().map(|p| v(p.x, y0, p.y)).collect();
    b.polygon(&bottom, -DVec3::Y, col);
}

/// A band of windows round an outline, a hair proud of its walls: panes
/// with frames between them near, one band further off.
fn windows(b: &mut MeshBuilder, pts: &[DVec2], y0: f64, y1: f64, lod: Lod, skip_back: bool) {
    let n = pts.len();
    let c = pts.iter().sum::<DVec2>() / n as f64;
    let saved = b.surf;
    b.surf = pane();
    for i in 0..n {
        let (a, e) = (pts[i], pts[(i + 1) % n]);
        let len = a.distance(e);
        if len < 1e-6 {
            continue;
        }
        let out = ((a + e) * 0.5 - c).normalize_or(DVec2::X);
        let mut normal = (e - a).perp().normalize();
        if normal.dot(out) < 0.0 {
            normal = -normal;
        }
        // The back wall (facing aft) has doors, not windows.
        if skip_back && normal.y < -0.9 {
            continue;
        }
        let off = normal * 0.035;
        let panes: Vec<(f64, f64)> = if lod == Lod::Near && len > 1.6 {
            let k = (len / 1.5).round().max(1.0) as usize;
            (0..k)
                .map(|j| {
                    let s0 = j as f64 / k as f64;
                    let s1 = (j + 1) as f64 / k as f64;
                    let gap = 0.09 / len;
                    (s0 + gap, s1 - gap)
                })
                .collect()
        } else {
            vec![(0.0, 1.0)]
        };
        for (s0, s1) in panes {
            let p0 = a.lerp(e, s0) + off;
            let p1 = a.lerp(e, s1) + off;
            b.quad(
                [
                    v(p0.x, y0, p0.y),
                    v(p1.x, y0, p1.y),
                    v(p1.x, y1, p1.y),
                    v(p0.x, y1, p0.y),
                ],
                v(normal.x, 0.0, normal.y),
                glass(),
            );
        }
    }
    b.surf = saved;
}

/// One deck of upper works: walls with a band of windows, a rounded
/// front, and an overhanging white edge on top.
#[allow(clippy::too_many_arguments)]
fn works_block(
    b: &mut MeshBuilder,
    blk: &Block,
    lv: &Livery,
    lod: Lod,
    front: f64,
    back: f64,
    overhang: f64,
    glazed: bool,
) {
    let seg = lod.pick(6, 3, 1);
    let pts = rounded(blk.half, blk.z0, blk.z1, front, back, seg);
    prism(b, &pts, blk.y0, blk.y1, lv.works, lv.works);
    let h = blk.y1 - blk.y0;
    if glazed && lod != Lod::Far && h > 1.6 {
        windows(b, &pts, blk.y0 + h * 0.38, blk.y0 + h * 0.78, lod, true);
    }
    if overhang > 0.0 && lod != Lod::Far {
        let o = overhang;
        let rim = rounded(
            blk.half + o,
            blk.z0 - o * 0.4,
            blk.z1 + o,
            front + o,
            back + o * 0.4,
            seg,
        );
        slab(b, &rim, blk.y1 - 0.12, blk.y1 + 0.16, lv.works);
    }
}

/// A raked teardrop funnel: elliptic rings leaning aft as it rises, a
/// coloured band and a dark top, chrome rings, and swept fins at its foot.
#[allow(clippy::too_many_arguments)]
fn funnel(b: &mut MeshBuilder, x: f64, z: f64, r: f64, y0: f64, y1: f64, lv: &Livery, lod: Lod) {
    let seg = lod.pick(28, 14, 8);
    let rows_n = lod.pick(10, 5, 3);
    let rake = (y1 - y0) * 0.32;
    let ring = |t: f64| -> Vec<DVec3> {
        let y = y0 + (y1 - y0) * t;
        // A little fuller in the middle, slimmer at the top.
        let k = 1.0 + 0.08 * (PI * t).sin() - 0.1 * t;
        let cz = z - rake * t;
        (0..seg)
            .map(|j| {
                let a = TAU * j as f64 / seg as f64;
                // Teardrop: round at the front, drawn out aft.
                let (s, c) = a.sin_cos();
                let rz = if c > 0.0 { r * 1.0 } else { r * 1.45 };
                v(x + s * r * 0.78 * k, y, cz + c * rz * k)
            })
            .collect()
    };
    let ts: Vec<f64> = (0..=rows_n).map(|i| i as f64 / rows_n as f64).collect();
    let rows: Vec<Vec<DVec3>> = ts.iter().map(|&t| ring(t)).collect();
    let paint = |i: usize, _j: usize| {
        let t = (ts[i] + ts[i + 1]) * 0.5;
        let c = if t > 0.86 {
            lv.top
        } else if (0.55..0.72).contains(&t) {
            lv.band
        } else {
            lv.funnel
        };
        Some((Surf::Plain, c))
    };
    b.grid(&rows, true, true, paint);
    // The cap.
    let top = ring(1.0);
    b.polygon(&top, DVec3::Y, rgb(20, 20, 22));
    if lod != Lod::Far {
        // Chrome rings either side of the band.
        for t in [0.53, 0.74] {
            let rg = ring(t);
            let mut path: Vec<DVec3> = rg
                .iter()
                .map(|p| {
                    let c = v(x, p.y, z - rake * t);
                    c + (*p - c) * 1.03
                })
                .collect();
            path.push(path[0]);
            tube(b, &path, (0.09, 0.09), 6, Some((Surf::Metal, lv.trim)));
        }
        // Swept fins either side of its foot.
        for side in [-1.0, 1.0] {
            let root0 = v(x + side * r * 0.7, y0, z + r * 0.4);
            let root1 = v(x + side * r * 0.7, y0, z - r * 2.2);
            let tip = v(
                x + side * r * 1.5,
                y0 + (y1 - y0) * 0.55,
                z - r * 1.9 - rake * 0.55,
            );
            let n = v(side, 0.0, 0.0);
            let th = v(side * 0.12, 0.0, 0.0);
            b.tri([root0, root1, tip], n, lv.funnel);
            b.tri([root0 - th, root1 - th, tip - th], -n, lv.funnel);
        }
    }
}

/// A mast: a slim pole, a yard across it, and (on the liner) an atom of
/// three rings round a ball, the radar of tomorrow.
#[allow(clippy::too_many_arguments)]
fn mast(
    b: &mut MeshBuilder,
    x: f64,
    z: f64,
    r: f64,
    y0: f64,
    y1: f64,
    lv: &Livery,
    lod: Lod,
    atom: bool,
) {
    let seg = lod.pick(10, 6, 4);
    b.cylinder(v(x, y0, z), (r, r * 0.55), y1 - y0, seg, false, lv.works);
    if lod == Lod::Far {
        return;
    }
    let yard = y0 + (y1 - y0) * 0.72;
    tube(
        b,
        &[v(x - 3.2, yard, z), v(x + 3.2, yard, z)],
        (0.12, 0.12),
        6,
        Some((Surf::Plain, lv.works)),
    );
    // Masthead light.
    b.surf = Surf::Glow;
    b.dome(v(x, y1, z), 0.22, 8, 3, rgb(255, 236, 190));
    b.surf = Surf::Plain;
    if atom {
        let c = v(x, yard + 2.2, z);
        b.surf = Surf::Metal;
        b.dome(c, 0.45, 10, 4, lv.trim);
        b.dome(c, -0.45, 10, 4, lv.trim);
        b.surf = Surf::Plain;
        for k in 0..3 {
            let tilt = PI * k as f64 / 3.0;
            let path: Vec<DVec3> = (0..=24)
                .map(|j| {
                    let a = TAU * j as f64 / 24.0;
                    let (s, co) = a.sin_cos();
                    let p = v(co * 1.6, s * 0.55, 0.0);
                    let (ts, tc) = tilt.sin_cos();
                    c + v(p.x * tc - p.z * ts, p.y, p.x * ts + p.z * tc)
                })
                .collect();
            tube(b, &path, (0.05, 0.05), 5, Some((Surf::Metal, lv.trim)));
        }
    }
}

/// A lifeboat in its davits: a little white hull with an orange cover.
fn lifeboat(b: &mut MeshBuilder, c: DVec3, side: f64, lod: Lod) {
    let len = 7.0;
    let seg = lod.pick(10, 6, 4);
    let profile: Vec<(f64, f64)> = (0..=8)
        .map(|k| {
            let t = k as f64 / 8.0;
            ((PI * t).sin().max(0.0).powf(0.7) * 1.1, t * len)
        })
        .collect();
    lathe(
        b,
        c - v(0.0, 0.0, len * 0.5),
        DVec3::Z,
        &profile,
        seg,
        |k| {
            Some((
                Surf::Plain,
                if !(1..=6).contains(&k) {
                    rgb(244, 240, 232)
                } else {
                    rgb(232, 110, 40)
                },
            ))
        },
    );
    if lod == Lod::Near {
        for dz in [-2.6, 2.6] {
            let foot = c + v(-side * 1.4, -1.6, dz);
            tube(
                b,
                &[foot, foot + v(0.0, 2.6, 0.0), c + v(side * 0.2, 1.4, dz)],
                (0.08, 0.08),
                5,
                Some((Surf::Plain, rgb(236, 234, 228))),
            );
        }
    }
}

/// A cargo hatch: a low coaming with a rounded tarpaulin top.
fn hatch(b: &mut MeshBuilder, z0: f64, z1: f64, half: f64, y: f64, lod: Lod) {
    let coaming = rgb(120, 70, 56);
    let tarp = rgb(86, 110, 92);
    let pts = rounded(half, z0, z1, 0.6, 0.6, lod.pick(3, 2, 1));
    prism(b, &pts, y, y + 1.1, coaming, coaming);
    let n = lod.pick(8, 4, 2);
    let rows: Vec<Vec<DVec3>> = [z0 + 0.2, z1 - 0.2]
        .iter()
        .map(|&z| {
            (0..=n)
                .map(|k| {
                    let a = PI * k as f64 / n as f64;
                    v(-(half - 0.2) * a.cos(), y + 1.1 + 0.45 * a.sin(), z)
                })
                .collect()
        })
        .collect();
    b.grid(&rows, false, false, |_, _| Some((Surf::Plain, tarp)));
}

/// A derrick boom from a post, cocked over a hatch.
fn derrick(b: &mut MeshBuilder, post: DVec3, to: DVec3, lv: &Livery) {
    tube(
        b,
        &[post, to],
        (0.16, 0.16),
        6,
        Some((Surf::Plain, lv.works)),
    );
    tube(
        b,
        &[post + v(0.0, 6.0, 0.0), to],
        (0.03, 0.03),
        4,
        Some((Surf::Plain, rgb(40, 40, 40))),
    );
}

/// The ship, built in its own frame.
pub fn ship(kind: ShipKind, lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let lv = livery(kind);
    let d = kind.dims();
    hull(&mut b, kind, &lv, lod);
    let blocks = kind.blocks();
    match kind {
        ShipKind::Freighter => {
            // Forecastle, bridge house (rounded front, overhanging edges),
            // poop.
            works_block(
                &mut b,
                &blocks[0],
                &Livery {
                    works: lv.hull,
                    ..lv
                },
                lod,
                4.0,
                0.3,
                0.0,
                false,
            );
            works_block(&mut b, &blocks[1], &lv, lod, 3.2, 0.4, 0.5, true);
            works_block(&mut b, &blocks[2], &lv, lod, 3.0, 0.4, 0.5, true);
            works_block(&mut b, &blocks[3], &lv, lod, 2.0, 0.6, 0.35, true);
            works_block(
                &mut b,
                &blocks[4],
                &Livery {
                    works: lv.hull,
                    ..lv
                },
                lod,
                0.3,
                4.0,
                0.0,
                false,
            );
            for s in kind.stacks().iter().skip(1) {
                mast(&mut b, s.x, s.z, s.r, s.y0, s.y1, &lv, lod, false);
            }
            let f = kind.stacks()[0];
            funnel(&mut b, f.x, f.z, f.r, f.y0, f.y1, &lv, lod);
            let y = d.free;
            if lod != Lod::Far {
                for (z0, z1) in [(33.5, 38.5), (8.0, 26.0), (-14.0, 4.0)] {
                    hatch(&mut b, z0, z1, 5.0, y, lod);
                }
                if lod == Lod::Near {
                    derrick(&mut b, v(0.0, y + 2.0, 30.0), v(3.0, y + 9.0, 20.0), &lv);
                    derrick(&mut b, v(0.0, y + 2.0, 30.0), v(-3.0, y + 9.0, 36.0), &lv);
                    derrick(&mut b, v(0.0, y + 2.0, -2.0), v(-3.0, y + 9.0, 12.0), &lv);
                    derrick(&mut b, v(0.0, y + 2.0, -2.0), v(3.0, y + 9.0, -9.0), &lv);
                }
            }
            // Radar atom on the bridge roof.
            mast(
                &mut b,
                0.0,
                -22.0,
                0.22,
                14.6,
                18.0,
                &lv,
                lod,
                lod == Lod::Near,
            );
        }
        ShipKind::Liner => {
            works_block(&mut b, &blocks[0], &lv, lod, 9.0, 3.0, 0.7, true);
            works_block(&mut b, &blocks[1], &lv, lod, 8.0, 3.0, 0.7, true);
            works_block(&mut b, &blocks[2], &lv, lod, 7.0, 3.0, 0.6, true);
            works_block(&mut b, &blocks[3], &lv, lod, 5.0, 0.6, 0.5, true);
            let st = kind.stacks();
            for f in &st[..2] {
                funnel(&mut b, f.x, f.z, f.r, f.y0, f.y1, &lv, lod);
            }
            mast(
                &mut b,
                st[2].x,
                st[2].z,
                st[2].r,
                st[2].y0,
                st[2].y1,
                &lv,
                lod,
                lod == Lod::Near,
            );
            // The cheatline: a blue band along the hull under the deck.
            if lod != Lod::Far {
                for (k, blk) in blocks.iter().take(3).enumerate() {
                    // Lifeboats along the second deck's edges.
                    if k == 1 {
                        let n = lod.pick(7, 7, 0);
                        for i in 0..n {
                            let z = blk.z0
                                + 6.0
                                + (blk.z1 - blk.z0 - 12.0) * i as f64 / (n - 1).max(1) as f64;
                            if (z - 10.0).abs() < 5.0 || (z + 16.0).abs() < 5.0 {
                                continue;
                            }
                            for side in [-1.0, 1.0] {
                                lifeboat(
                                    &mut b,
                                    v(side * (blk.half + 0.3), blk.y1 + 1.2, z),
                                    side,
                                    lod,
                                );
                            }
                        }
                    }
                }
            }
        }
        ShipKind::Tug => {
            works_block(
                &mut b,
                &blocks[1],
                &Livery {
                    works: lv.hull,
                    ..lv
                },
                lod,
                0.3,
                1.6,
                0.0,
                false,
            );
            works_block(&mut b, &blocks[0], &lv, lod, 2.2, 0.4, 0.35, true);
            let f = kind.stacks()[0];
            funnel(&mut b, f.x, f.z, f.r, f.y0, f.y1, &lv, lod);
            mast(&mut b, 0.0, 2.0, 0.12, 5.4, 9.5, &lv, lod, false);
            if lod != Lod::Far {
                // The rubber fender round its gunwale and at its bow.
                let us = stations(lod.pick(28, 12, 6));
                for side in [-1.0, 1.0] {
                    let path: Vec<DVec3> = us
                        .iter()
                        .filter(|u| u.abs() < 0.46)
                        .map(|&u| {
                            let y = deck_y(kind, u) - 0.25;
                            let h = (y + d.draft) / (deck_y(kind, u) + d.draft);
                            v(
                                side * (hull_half(kind, u, h) + 0.15),
                                y,
                                u * d.len + stem_lean(kind, u, h),
                            )
                        })
                        .collect();
                    tube(
                        &mut b,
                        &path,
                        (0.3, 0.3),
                        lod.pick(8, 5, 4),
                        Some((Surf::Plain, rgb(28, 28, 30))),
                    );
                }
                // The towing bitt aft.
                b.cylinder(
                    v(0.0, d.free, -10.0),
                    (0.35, 0.35),
                    1.0,
                    10,
                    true,
                    rgb(40, 40, 44),
                );
            }
        }
        ShipKind::Launch => {
            // A streamlined cabin: a loaf with a raked, curved windscreen,
            // a band of windows round it, a chrome spear and a little mast.
            let blk = blocks[0];
            let n = lod.pick(10, 5, 2);
            let zs: Vec<f64> = (0..=lod.pick(10, 5, 3))
                .map(|i| blk.z0 + (blk.z1 - blk.z0) * i as f64 / lod.pick(10, 5, 3) as f64)
                .collect();
            let h = blk.y1 - blk.y0;
            let section = |z: f64| -> Vec<DVec3> {
                let u = (z - blk.z0) / (blk.z1 - blk.z0);
                // Narrower and lower towards the front, the roof raking down.
                let w = blk.half * (1.0 - 0.35 * u.powi(3)) * (0.92 + 0.08 * (PI * u).sin());
                let top = h * (1.0 - 0.45 * u.powi(2)) + 0.25 * (1.0 - u) * u;
                (0..=n * 2)
                    .map(|j| {
                        let a = PI * j as f64 / (n * 2) as f64;
                        let x = -w * a.cos();
                        let y = blk.y0 + top * a.sin().powf(0.35);
                        v(x, y, z)
                    })
                    .collect()
            };
            let rows: Vec<Vec<DVec3>> = zs.iter().map(|&z| section(z)).collect();
            let cols = n * 2;
            let glass = rgb(40, 60, 76);
            b.grid(&rows, false, true, |i, j| {
                // The window band: the sides' upper half, and the front.
                let t = (j as f64 + 0.5) / cols as f64;
                let side = !(0.3..=0.7).contains(&t);
                let front = i + 3 >= zs.len() - 1;
                let band = lod != Lod::Far
                    && ((side
                        && i >= 1
                        && i + 1 < zs.len() - 1
                        && (0.12..0.3).contains(&t.min(1.0 - t)))
                        || (front && (0.22..=0.78).contains(&t)));
                Some(if band {
                    (Surf::Glass, glass)
                } else {
                    (Surf::Plain, lv.works)
                })
            });
            // Close its back.
            let back = &rows[0];
            for j in 0..back.len() - 1 {
                b.tri(
                    [v(0.0, blk.y0, blk.z0), back[j], back[j + 1]],
                    v(0.0, 0.0, -1.0),
                    lv.works,
                );
            }
            let front = rows.last().expect("rows");
            for j in 0..front.len() - 1 {
                b.tri(
                    [v(0.0, blk.y0, blk.z1), front[j], front[j + 1]],
                    v(0.0, 0.0, 1.0),
                    lv.works,
                );
            }
            if lod != Lod::Far {
                // A chrome spear along each flank and a mast with a pennant.
                for sd in [-1.0, 1.0] {
                    let path: Vec<DVec3> = zs
                        .iter()
                        .map(|&z| {
                            let r = section(z);
                            let p = r[if sd < 0.0 { 1 } else { r.len() - 2 }];
                            v(p.x + sd * 0.03, blk.y0 + 0.18, z)
                        })
                        .collect();
                    tube(
                        &mut b,
                        &path,
                        (0.03, 0.03),
                        5,
                        Some((Surf::Metal, rgb(214, 218, 224))),
                    );
                }
                let mast = v(0.0, blk.y0 + h, blk.z0 + 0.6);
                b.cylinder(mast, (0.04, 0.02), 1.4, 6, false, rgb(214, 218, 224));
                b.quad(
                    [
                        mast + v(0.0, 1.0, 0.0),
                        mast + v(0.0, 1.35, 0.0),
                        mast + v(0.0, 1.2, -0.6),
                        mast + v(0.0, 1.15, -0.6),
                    ],
                    v(1.0, 0.0, 0.0),
                    rgb(220, 60, 50),
                );
                b.quad(
                    [
                        mast + v(0.0, 1.0, 0.0),
                        mast + v(0.0, 1.35, 0.0),
                        mast + v(0.0, 1.2, -0.6),
                        mast + v(0.0, 1.15, -0.6),
                    ],
                    v(-1.0, 0.0, 0.0),
                    rgb(220, 60, 50),
                );
            }
        }
    }
    b.finish()
}

/// The ship's levels.
pub fn ship_levels(kind: ShipKind) -> Vec<LodLevel> {
    let (near, mid, far) = match kind {
        ShipKind::Liner | ShipKind::Freighter => (260.0, 1100.0, 9000.0),
        ShipKind::Tug => (120.0, 500.0, 3000.0),
        ShipKind::Launch => (70.0, 260.0, 1500.0),
    };
    vec![
        LodLevel::new(ship(kind, Lod::Near), near),
        LodLevel::new(ship(kind, Lod::Mid), mid),
        LodLevel::new(ship(kind, Lod::Far), far),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "on demand (looks): scripts/check.sh --on-demand"]
    fn ships_come_in_three_levels() {
        for kind in [
            ShipKind::Freighter,
            ShipKind::Liner,
            ShipKind::Tug,
            ShipKind::Launch,
        ] {
            let near = ship(kind, Lod::Near).face_count();
            let mid = ship(kind, Lod::Mid).face_count();
            let far = ship(kind, Lod::Far).face_count();
            assert!(mid < near && far < mid, "{kind:?}: {near} {mid} {far}");
            if matches!(kind, ShipKind::Freighter | ShipKind::Liner) {
                assert!((5_000..80_000).contains(&near), "{kind:?} near {near}");
                assert!(far < 3_000, "{kind:?} far {far}");
            }
        }
    }

    #[test]
    fn every_ship_builds() {
        for kind in [
            ShipKind::Freighter,
            ShipKind::Liner,
            ShipKind::Tug,
            ShipKind::Launch,
        ] {
            assert!(ship(kind, Lod::Far).face_count() > 50);
        }
    }
}
