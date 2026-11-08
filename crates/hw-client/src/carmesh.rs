//! Golden-age cars at full quality: the finned cruiser, the bubble car,
//! the hover car and the flying taxi.
//!
//! Bodies are lofted: a cross-section (a rounded superellipse with
//! tumblehome) swept along the car, with wheel arches, skirts and rounded
//! ends, shaded smooth through [`MeshBuilder::grid`]. Paint follows the
//! grid lines, so two-tone coves, chrome spears and door seams stay crisp.
//! Chrome bumpers and trim are swept tubes; lamps, bullets, nacelles and
//! wheels are lathed. Every model comes in three levels ([`Lod`]), so a
//! street full of them costs little beyond the few near the camera.
//!
//! Units are metres, nose to +z, +y up, wheels standing on y = 0.

use crate::citymesh::{MeshBuilder, Surf};
use engine::prelude::*;
use glam::{DVec2, DVec3};
use hw_gen::urban::props::CarModel;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// How much detail a level carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lod {
    /// The camera within a few dozen metres: every seam, handle and bar.
    Near,
    /// Down the street: the shapes and chrome, fewer sides.
    Mid,
    /// Far away: the silhouette and its colours.
    Far,
}

impl Lod {
    pub(crate) fn pick<T>(self, near: T, mid: T, far: T) -> T {
        match self {
            Lod::Near => near,
            Lod::Mid => mid,
            Lod::Far => far,
        }
    }
}

/// Two-tone paints: body, then roof and trim.
pub const PAINTS: [[(u8, u8, u8); 2]; 8] = [
    [(196, 36, 48), (244, 238, 228)],
    [(56, 186, 186), (244, 238, 228)],
    [(240, 150, 172), (250, 244, 238)],
    [(150, 218, 178), (240, 232, 212)],
    [(108, 166, 228), (32, 52, 102)],
    [(244, 206, 58), (40, 40, 44)],
    [(238, 226, 198), (120, 30, 42)],
    [(176, 148, 218), (204, 208, 218)],
];

pub(crate) type Paint = Option<(Surf, Color)>;

/// The colours of one car.
#[derive(Clone, Copy)]
struct Kit {
    body: Color,
    top: Color,
    chrome: Color,
    glass: Color,
    tyre: Color,
    white: Color,
    under: Color,
    seam: Color,
    warm: Color,
    red: Color,
}

impl Kit {
    fn new(paint: u8) -> Self {
        let [b, t] = PAINTS[paint as usize % PAINTS.len()];
        Self {
            body: rgb(b.0, b.1, b.2),
            top: rgb(t.0, t.1, t.2),
            chrome: rgb(226, 230, 236),
            glass: rgb(52, 78, 96),
            tyre: rgb(22, 22, 24),
            white: rgb(240, 238, 230),
            under: rgb(30, 30, 33),
            seam: rgb(
                (b.0 as f32 * 0.45) as u8,
                (b.1 as f32 * 0.45) as u8,
                (b.2 as f32 * 0.45) as u8,
            ),
            warm: rgb(255, 226, 170),
            red: rgb(255, 46, 36),
        }
    }
    fn plain(c: Color) -> Paint {
        Some((Surf::Plain, c))
    }
    fn metal(&self) -> Paint {
        Some((Surf::Metal, self.chrome))
    }
    fn glazed(&self) -> Paint {
        Some((Surf::Glass, self.glass))
    }
    fn glow(c: Color) -> Paint {
        Some((Surf::Glow, c))
    }
}

// ------------------------------------------------------------ surfaces

/// Lays a grid with its normals pointing away from the grid's centre
/// (right for closed, roughly convex solids, and for open shells such as
/// a canopy sitting on a body).
pub(crate) fn surface(
    b: &mut MeshBuilder,
    rows: &[Vec<DVec3>],
    wrap: bool,
    paint: impl Fn(usize, usize) -> Paint,
) {
    let n = rows.len();
    let Some(m) = rows.first().map(Vec::len) else {
        return;
    };
    if n < 2 || m < 2 {
        return;
    }
    let count = (n * m) as f64;
    let c: DVec3 = rows.iter().flatten().sum::<DVec3>() / count;
    let mut acc = 0.0;
    for i in 0..n {
        let (i0, i1) = (i.saturating_sub(1), (i + 1).min(n - 1));
        for j in 0..m {
            let (j0, j1) = if wrap {
                ((j + m - 1) % m, (j + 1) % m)
            } else {
                (j.saturating_sub(1), (j + 1).min(m - 1))
            };
            let du = rows[i1][j] - rows[i0][j];
            let dv = rows[i][j1] - rows[i][j0];
            acc += du.cross(dv).dot(rows[i][j] - c);
        }
    }
    b.grid(rows, wrap, (acc >= 0.0) != INSIDE.get(), paint);
}

thread_local! {
    /// Set while laying the inner face of a shell (see [`inside`]).
    static INSIDE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Runs `f` with every surface it lays facing inwards: the inner face of
/// a thin shell, such as a canopy seen from under it when it is open.
fn inside(f: impl FnOnce()) {
    INSIDE.set(true);
    f();
    INSIDE.set(false);
}

/// Two unit vectors at right angles to `axis` (and to each other).
fn basis(axis: DVec3) -> (DVec3, DVec3) {
    let helper = if axis.y.abs() < 0.9 {
        DVec3::Y
    } else {
        DVec3::X
    };
    let u = axis.cross(helper).normalize();
    (u, axis.cross(u).normalize())
}

/// A solid of revolution: `profile` is (radius, distance along `axis`)
/// from one end to the other; start and end at radius 0 to close it.
/// `paint(k)` colours the band between profile points k and k + 1.
pub(crate) fn lathe(
    b: &mut MeshBuilder,
    origin: DVec3,
    axis: DVec3,
    profile: &[(f64, f64)],
    seg: usize,
    paint: impl Fn(usize) -> Paint,
) {
    let axis = axis.normalize();
    let (u, v) = basis(axis);
    let rows: Vec<Vec<DVec3>> = profile
        .iter()
        .map(|&(r, t)| {
            (0..seg)
                .map(|k| {
                    let a = TAU * k as f64 / seg as f64;
                    origin + axis * t + (u * a.cos() + v * a.sin()) * r
                })
                .collect()
        })
        .collect();
    surface(b, &rows, true, |i, _| paint(i));
}

/// A tube along `path` with an elliptic section (`rx` across, `ry` up),
/// its ends rounded off.
pub(crate) fn tube(
    b: &mut MeshBuilder,
    path: &[DVec3],
    (rx, ry): (f64, f64),
    seg: usize,
    paint: Paint,
) {
    if path.len() < 2 {
        return;
    }
    let n = path.len();
    let frame = |i: usize| {
        let t = (path[(i + 1).min(n - 1)] - path[i.saturating_sub(1)]).normalize();
        let mut side = DVec3::Y.cross(t);
        if side.length_squared() < 1e-6 {
            side = DVec3::X;
        }
        let side = side.normalize();
        (t, side, t.cross(side).normalize())
    };
    let ring = |c: DVec3, side: DVec3, up: DVec3, k: f64| -> Vec<DVec3> {
        (0..seg)
            .map(|j| {
                let a = TAU * j as f64 / seg as f64;
                c + side * (rx * k * a.cos()) + up * (ry * k * a.sin())
            })
            .collect()
    };
    let r = rx.min(ry);
    let caps = 3;
    let mut rows = Vec::new();
    let (t0, s0, u0) = frame(0);
    for k in (1..=caps).rev() {
        let th = FRAC_PI_2 * k as f64 / caps as f64;
        rows.push(ring(path[0] - t0 * (r * th.sin()), s0, u0, th.cos()));
    }
    for (i, &p) in path.iter().enumerate() {
        let (_, s, u) = frame(i);
        rows.push(ring(p, s, u, 1.0));
    }
    let (t1, s1, u1) = frame(n - 1);
    for k in 1..=caps {
        let th = FRAC_PI_2 * k as f64 / caps as f64;
        rows.push(ring(path[n - 1] + t1 * (r * th.sin()), s1, u1, th.cos()));
    }
    surface(b, &rows, true, |_, _| paint);
}

/// A closed loop of points through `pts` (Catmull-Rom), `per` points per
/// span; `closed` joins the last point back to the first.
pub(crate) fn spline(pts: &[DVec3], per: usize, closed: bool) -> Vec<DVec3> {
    let n = pts.len();
    if n < 2 {
        return pts.to_vec();
    }
    let get = |i: isize| -> DVec3 {
        if closed {
            pts[i.rem_euclid(n as isize) as usize]
        } else {
            pts[i.clamp(0, n as isize - 1) as usize]
        }
    };
    let spans = if closed { n } else { n - 1 };
    let mut out = Vec::new();
    for s in 0..spans as isize {
        let (p0, p1, p2, p3) = (get(s - 1), get(s), get(s + 1), get(s + 2));
        for k in 0..per {
            let t = k as f64 / per as f64;
            let t2 = t * t;
            let t3 = t2 * t;
            out.push(
                0.5 * ((2.0 * p1)
                    + (-p0 + p2) * t
                    + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
                    + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3),
            );
        }
    }
    if closed {
        out.push(out[0]);
    } else {
        out.push(pts[n - 1]);
    }
    out
}

/// A flat elliptic disc facing `n` (for grille backs and glow plates).
pub(crate) fn disc(
    b: &mut MeshBuilder,
    c: DVec3,
    (u, v): (DVec3, DVec3),
    seg: usize,
    n: DVec3,
    col: Color,
) {
    let pts: Vec<DVec3> = (0..seg)
        .map(|k| {
            let a = TAU * k as f64 / seg as f64;
            c + u * a.cos() + v * a.sin()
        })
        .collect();
    b.polygon(&pts, n, col);
}

/// A box with its own axes: `axes[0]` and `axes[1]` (unit, at right
/// angles), the third their cross product; `half` sizes along the three.
pub(crate) fn slab(b: &mut MeshBuilder, c: DVec3, axes: [DVec3; 2], half: DVec3, col: Color) {
    let (e1, e2) = (axes[0] * half.x, axes[1] * half.y);
    let e3 = axes[0].cross(axes[1]).normalize() * half.z;
    let q = |z: f64| {
        [
            c - e1 - e2 + e3 * z,
            c + e1 - e2 + e3 * z,
            c + e1 + e2 + e3 * z,
            c - e1 + e2 + e3 * z,
        ]
    };
    b.hexa(q(-1.0), q(1.0), col);
}

fn smooth(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// ------------------------------------------------------------ bodies

/// One cross-section of a body: half width, the bottom, the line where it
/// is widest and the top; roundness above and below (2 = an ellipse,
/// higher = boxier); tumblehome (the top drawn in) and tuck (the bottom).
#[derive(Clone, Copy, Debug)]
struct Section {
    w: f64,
    bot: f64,
    mid: f64,
    top: f64,
    n_up: f64,
    n_dn: f64,
    tumble: f64,
    tuck: f64,
}

impl Section {
    fn at(&self, a: f64) -> DVec2 {
        let (s, c) = a.sin_cos();
        let up = s >= 0.0;
        let e = 2.0 / if up { self.n_up } else { self.n_dn };
        let xf = c.signum() * c.abs().powf(e);
        let yf = s.signum() * s.abs().powf(e);
        let draw = if up { self.tumble } else { self.tuck };
        let x = self.w * xf * (1.0 - draw * yf * yf);
        let y = if up {
            self.mid + (self.top - self.mid) * yf
        } else {
            self.mid + (self.mid - self.bot) * yf
        };
        DVec2::new(x, y)
    }

    /// Drawn in by `d` all round (for rounding the ends).
    fn inset(&self, d: f64) -> Section {
        let top = (self.top - d).max(self.mid + 0.01);
        let bot = (self.bot + d).min(self.mid - 0.01);
        Section {
            w: (self.w - d).max(0.01),
            top,
            bot,
            ..*self
        }
    }

    /// Height of the upper surface at `x` from the centre line.
    fn y_at(&self, x: f64) -> f64 {
        let x = x.abs();
        let mut prev = self.at(0.0);
        for k in 1..=96 {
            let p = self.at(FRAC_PI_2 * k as f64 / 96.0);
            if p.x <= x {
                let t = if (prev.x - p.x).abs() < 1e-9 {
                    0.0
                } else {
                    (prev.x - x) / (prev.x - p.x)
                };
                return prev.y + (p.y - prev.y) * t;
            }
            prev = p;
        }
        self.top
    }

    /// The side's distance from the centre line at height `y`.
    fn x_at(&self, y: f64) -> f64 {
        let mut best = (f64::MAX, self.w);
        for k in 0..=192 {
            let a = -FRAC_PI_2 + PI * k as f64 / 192.0;
            let p = self.at(a);
            let d = (p.y - y).abs();
            if d < best.0 {
                best = (d, p.x);
            }
        }
        best.1
    }
}

/// A wheel opening: its axle along z, its height and radius. A skirt
/// closes it down to `skirt` (the rear wheels of a fifties cruiser).
#[derive(Clone, Copy)]
struct Arch {
    z: f64,
    cy: f64,
    r: f64,
    skirt: Option<f64>,
    /// The well's inner wall, from the centre line.
    x_in: f64,
}

impl Arch {
    fn bottom(&self, z: f64) -> Option<f64> {
        let dz = z - self.z;
        if dz.abs() >= self.r {
            return None;
        }
        let y = self.cy + (self.r * self.r - dz * dz).sqrt();
        Some(match self.skirt {
            Some(s) => y.min(s),
            None => y,
        })
    }
}

/// Where a quad of a body sits: along the car, how high on the section
/// (the sine of its angle: -1 underneath, 0 the widest line, 1 on top),
/// and the cosine (the side it faces, + for the car's +x).
#[derive(Clone, Copy, Debug)]
struct At {
    z: f64,
    s: f64,
    c: f64,
    /// Inside a wheel well.
    well: bool,
}

impl At {
    fn side(&self) -> bool {
        self.c.abs() > 0.2
    }
}

struct Body<'a> {
    /// Tail and nose.
    z: [f64; 2],
    /// Rounding of the tail and the nose.
    end_r: [f64; 2],
    section: &'a dyn Fn(f64) -> Section,
    arches: Vec<Arch>,
    /// Places along the car that need a row of their own (seams, the
    /// ends of a paint panel).
    stations: Vec<f64>,
    /// Heights (sines) that need a column of their own: paint borders.
    bands: Vec<f64>,
}

impl Body<'_> {
    /// The section at `z` with the wheel arches cut in.
    fn sec(&self, z: f64) -> Section {
        let mut s = (self.section)(z.clamp(self.z[0], self.z[1]));
        if let Some((y, _)) = self.well(z) {
            s.bot = s.bot.max(y);
        }
        s.mid = s.mid.max(s.bot + 0.05);
        s.top = s.top.max(s.mid + 0.05);
        s
    }

    /// The body's own shape at `z`, before the arches are cut in.
    fn shape(&self, z: f64) -> Section {
        (self.section)(z.clamp(self.z[0], self.z[1]))
    }

    /// The wheel well at `z`: its roof, and how far in from the centre
    /// line it reaches.
    fn well(&self, z: f64) -> Option<(f64, f64)> {
        self.arches
            .iter()
            .filter_map(|a| a.bottom(z).map(|y| (y, a.x_in)))
            .reduce(|p, q| if p.0 > q.0 { p } else { q })
    }

    /// Lays the body; `paint` colours every quad from where it sits.
    fn build(&self, b: &mut MeshBuilder, lod: Lod, paint: impl Fn(At) -> Paint) {
        let around = lod.pick(30, 16, 10);
        let mut angles: Vec<f64> = (0..around)
            .map(|k| TAU * k as f64 / around as f64)
            .collect();
        if lod != Lod::Far {
            for &s in &self.bands {
                let a = s.clamp(-1.0, 1.0).asin();
                angles.push(a.rem_euclid(TAU));
                angles.push((PI - a).rem_euclid(TAU));
            }
        }
        angles.sort_by(f64::total_cmp);
        angles.dedup_by(|a, b| (*a - *b).abs() < 0.004);

        let [z0, z1] = self.z;
        let [r0, r1] = self.end_r;
        let (a, bz) = (z0 + r0, z1 - r1);
        let step = lod.pick(0.11, 0.24, 0.6);
        let n = ((bz - a) / step).ceil().max(1.0) as usize;
        let mut zs: Vec<f64> = (0..=n)
            .map(|k| a + (bz - a) * k as f64 / n as f64)
            .collect();
        if lod != Lod::Far {
            let k = lod.pick(12, 6, 0);
            for arch in &self.arches {
                for i in 0..=k {
                    let z = arch.z + arch.r * (PI * i as f64 / k as f64).cos();
                    zs.push(z);
                }
                // The skirt's corners.
                if let Some(s) = arch.skirt {
                    let dy = s - arch.cy;
                    if dy < arch.r {
                        let dz = (arch.r * arch.r - dy * dy).sqrt();
                        zs.push(arch.z - dz);
                        zs.push(arch.z + dz);
                    }
                }
            }
            zs.extend(self.stations.iter().copied());
        }
        zs.retain(|z| *z >= a - 1e-9 && *z <= bz + 1e-9);
        zs.sort_by(f64::total_cmp);
        zs.dedup_by(|p, q| (*p - *q).abs() < 0.003);

        // A wheel well lifts the lower side of the section, outboard of
        // the well's inner wall, up to the arch.
        let ring = |sec: &Section, z: f64| -> Vec<DVec3> {
            angles
                .iter()
                .map(|&t| {
                    let p = sec.at(t);
                    DVec3::new(p.x, p.y, z)
                })
                .collect()
        };
        let caps = lod.pick(6, 3, 1);
        let mut rows: Vec<Vec<DVec3>> = Vec::new();
        let mut row_z: Vec<f64> = Vec::new();
        // The tail: from the tip in to the first full section.
        let tail = self.sec(a);
        rows.push(vec![
            DVec3::new(0.0, (tail.top + tail.bot) * 0.5, z0);
            angles.len()
        ]);
        row_z.push(z0);
        for k in (1..caps).rev() {
            let th = FRAC_PI_2 * k as f64 / caps as f64;
            let z = a - r0 * th.sin();
            rows.push(ring(&tail.inset(r0 * (1.0 - th.cos())), z));
            row_z.push(z);
        }
        for &z in &zs {
            rows.push(ring(&self.sec(z), z));
            row_z.push(z);
        }
        let nose = self.sec(bz);
        for k in 1..caps {
            let th = FRAC_PI_2 * k as f64 / caps as f64;
            let z = bz + r1 * th.sin();
            rows.push(ring(&nose.inset(r1 * (1.0 - th.cos())), z));
            row_z.push(z);
        }
        rows.push(vec![
            DVec3::new(0.0, (nose.top + nose.bot) * 0.5, z1);
            angles.len()
        ]);
        row_z.push(z1);

        let m = angles.len();
        let moved = |i: usize, j: usize| {
            let p = rows[i][j];
            self.well(p.z).is_some_and(|(wy, _)| p.y < wy + 0.02)
        };
        surface(b, &rows, true, |i, j| {
            let a0 = angles[j];
            let a1 = if j + 1 < m {
                angles[j + 1]
            } else {
                angles[0] + TAU
            };
            let t = (a0 + a1) * 0.5;
            let jn = (j + 1) % m;
            paint(At {
                z: (row_z[i] + row_z[i + 1]) * 0.5,
                s: t.sin(),
                c: t.cos(),
                well: t.sin() < 0.0
                    && moved(i, j)
                    && moved(i + 1, j)
                    && moved(i, jn)
                    && moved(i + 1, jn),
            })
        });
    }
}

/// A glasshouse (or a canopy) on a body: lofted between a base line on
/// the body and a roof, its height rising from nothing at the ends.
struct Cabin<'a> {
    /// Where its base starts and ends along the car.
    z: [f64; 2],
    /// Where the full-height roof starts and ends.
    roof: [f64; 2],
    height: f64,
    /// Half widths at the base and at the roof.
    w_base: f64,
    w_roof: f64,
    /// Roundness of the roof edge (2 = a dome).
    n: f64,
    /// The base height along the car.
    base: &'a dyn Fn(f64) -> f64,
    /// How the height rises at the ends (1 = straight, below 1 = fuller).
    ease: f64,
    stations: Vec<f64>,
    bands: Vec<f64>,
}

impl Cabin<'_> {
    fn height_at(&self, z: f64) -> f64 {
        let [z0, z1] = self.z;
        let [r0, r1] = self.roof;
        let t = if z < r0 {
            ((z - z0) / (r0 - z0)).clamp(0.0, 1.0)
        } else if z > r1 {
            ((z1 - z) / (z1 - r1)).clamp(0.0, 1.0)
        } else {
            1.0
        };
        // A quarter ellipse eased: a windscreen that curves into the roof.
        let full = (1.0 - (1.0 - t).powi(2)).max(0.0).sqrt();
        self.height * full.powf(self.ease)
    }

    /// How far out the side is at height `y` at `z`.
    fn side_x(&self, z: f64, y: f64) -> f64 {
        let h = self.height_at(z).max(1e-3);
        let yf = ((y - (self.base)(z)) / h).clamp(0.0, 1.0);
        let w = self.w_base + (self.w_roof - self.w_base) * yf;
        let sn = yf.powf(self.n / 2.0);
        let c = (1.0 - sn * sn).max(0.0).sqrt();
        w * c.powf(2.0 / self.n)
    }

    fn build(&self, b: &mut MeshBuilder, lod: Lod, paint: impl Fn(At) -> Paint) {
        let around = lod.pick(22, 10, 6);
        let mut angles: Vec<f64> = (0..=around)
            .map(|k| PI * k as f64 / around as f64)
            .collect();
        if lod != Lod::Far {
            for &f in &self.bands {
                let a = f.clamp(0.0, 1.0).powf(self.n / 2.0).asin();
                angles.push(a);
                angles.push(PI - a);
            }
        }
        angles.sort_by(f64::total_cmp);
        angles.dedup_by(|a, b| (*a - *b).abs() < 0.004);
        let [z0, z1] = self.z;
        let step = lod.pick(0.08, 0.18, 0.4);
        let n = ((z1 - z0) / step).ceil().max(2.0) as usize;
        let mut zs: Vec<f64> = (0..=n)
            .map(|k| z0 + (z1 - z0) * k as f64 / n as f64)
            .collect();
        zs.extend(self.roof);
        if lod != Lod::Far {
            zs.extend(self.stations.iter().copied());
        }
        zs.retain(|z| *z >= z0 && *z <= z1);
        zs.sort_by(f64::total_cmp);
        zs.dedup_by(|p, q| (*p - *q).abs() < 0.003);
        let e = 2.0 / self.n;
        let rows: Vec<Vec<DVec3>> = zs
            .iter()
            .map(|&z| {
                let h = self.height_at(z).max(0.004);
                let y0 = (self.base)(z);
                angles
                    .iter()
                    .map(|&a| {
                        let (s, c) = a.sin_cos();
                        let yf = s.abs().powf(e);
                        let xf = c.signum() * c.abs().powf(e);
                        let w = self.w_base + (self.w_roof - self.w_base) * yf;
                        DVec3::new(w * xf, y0 + h * yf, z)
                    })
                    .collect()
            })
            .collect();
        surface(b, &rows, false, |i, j| {
            let t = (angles[j] + angles[j + 1]) * 0.5;
            paint(At {
                z: (zs[i] + zs[i + 1]) * 0.5,
                // As a fraction of the height, for a cabin.
                s: t.sin().abs().powf(e),
                c: t.cos(),
                well: false,
            })
        });
    }
}

/// A fin: a thin blade standing on the body at `x`, rising from nothing at
/// `z[0]` to `height` at `z[1]` and leaning out by `lean` per metre up.
#[allow(clippy::too_many_arguments)]
fn fin(
    b: &mut MeshBuilder,
    lod: Lod,
    x: f64,
    z: [f64; 2],
    height: f64,
    thick: f64,
    lean: f64,
    base: &dyn Fn(f64) -> f64,
    paint: Paint,
) {
    let n = lod.pick(26, 10, 4);
    let around = lod.pick(14, 8, 4);
    let mut rows = Vec::new();
    for k in 0..=n {
        let t = k as f64 / n as f64;
        let zz = z[0] + (z[1] - z[0]) * t;
        let h = height * t.powf(1.4);
        let y0 = base(zz) - 0.05;
        let th = thick * (0.35 + 0.65 * (1.0 - t * 0.5));
        let row: Vec<DVec3> = (0..around)
            .map(|j| {
                let a = TAU * j as f64 / around as f64;
                let (s, c) = a.sin_cos();
                let y = if s >= 0.0 {
                    y0 + (h + 0.05) * s.powf(0.6)
                } else {
                    y0 + 0.03 * s
                };
                DVec3::new(x + th * c + lean * (y - y0) * x.signum(), y, zz)
            })
            .collect();
        rows.push(row);
    }
    // Close the tail end.
    let last = rows.last().expect("rows");
    let c = last.iter().sum::<DVec3>() / last.len() as f64;
    rows.push(vec![c; around]);
    surface(b, &rows, true, |_, _| paint);
}

/// A wheel centred on `c`, its axle along x: a tyre with tread grooves,
/// whitewalls, a chrome rim and a domed hubcap on both faces, and (close
/// up) a spinner so it can be seen to turn.
pub fn wheel(b: &mut MeshBuilder, c: DVec3, r: f64, width: f64, lod: Lod) {
    let kit = Kit::new(0);
    let seg = lod.pick(24, 12, 8);
    let hw = width * 0.5;
    // One face from the hub outwards (radius, offset from the middle), and
    // what each band between its points is: 2 chrome, 1 whitewall, 0 tyre.
    let (face, bands): (Vec<(f64, f64)>, Vec<u8>) = if lod == Lod::Far {
        (
            vec![
                (0.0, hw + 0.03),
                (0.45 * r, hw + 0.01),
                (0.6 * r, hw),
                (0.85 * r, hw),
            ],
            vec![2, 1, 0],
        )
    } else {
        (
            vec![
                (0.0, hw + 0.06),
                (0.18 * r, hw + 0.055),
                (0.34 * r, hw + 0.035),
                (0.44 * r, hw + 0.012),
                (0.5 * r, hw + 0.004),
                (0.6 * r, hw - 0.004),
                (0.82 * r, hw),
                (0.93 * r, hw - 0.01),
            ],
            vec![2, 2, 2, 2, 2, 1, 0],
        )
    };
    // The tread, shoulder to shoulder, with grooves.
    let tread: Vec<(f64, f64)> = if lod == Lod::Near {
        [
            -0.85, -0.6, -0.5, -0.4, -0.15, -0.05, 0.05, 0.15, 0.4, 0.5, 0.6, 0.85,
        ]
        .iter()
        .enumerate()
        .map(|(k, t)| {
            let groove = matches!(k, 2 | 5 | 6 | 9);
            (if groove { r * 0.975 } else { r }, t * hw)
        })
        .collect()
    } else {
        vec![(r, -0.85 * hw), (r, 0.85 * hw)]
    };
    let mut profile: Vec<(f64, f64)> = face.iter().map(|&(rr, x)| (rr, -x)).collect();
    let mut kinds: Vec<u8> = bands.clone();
    kinds.push(0);
    profile.extend(tread.iter().copied());
    kinds.extend(std::iter::repeat_n(0, tread.len() - 1));
    kinds.push(0);
    profile.extend(face.iter().rev().copied());
    kinds.extend(bands.iter().rev().copied());
    lathe(b, c, DVec3::X, &profile, seg, |k| match kinds[k] {
        2 => kit.metal(),
        1 => Kit::plain(kit.white),
        _ => Kit::plain(kit.tyre),
    });
    if lod == Lod::Near {
        // Three-eared spinners on both hubcaps.
        let prev = b.surf;
        b.surf = Surf::Metal;
        for sgn in [-1.0, 1.0] {
            for k in 0..2 {
                let a = PI * k as f64 / 2.0 + 0.4;
                let u = DVec3::new(0.0, a.cos(), a.sin());
                slab(
                    b,
                    c + DVec3::X * (sgn * (hw + 0.065)),
                    [DVec3::X, u],
                    DVec3::new(0.012, 0.3 * r, 0.018),
                    kit.chrome,
                );
            }
        }
        b.surf = prev;
    }
}

// ------------------------------------------------------------ models

/// Where a model's wheels sit: half the track, the axles along z, the
/// radius. Hover cars have none.
pub fn wheel_layout(model: CarModel) -> Option<(f64, [f64; 2], f64)> {
    match model {
        CarModel::Cruiser => Some((0.84, [-1.6, 1.65], 0.36)),
        CarModel::Bubble => Some((0.66, [-1.05, 1.1], 0.3)),
        CarModel::Hover => None,
    }
}

/// Builds `model` in `paint` into `b`; `wheels` adds the wheels (parked
/// cars and far levels: moving cars turn theirs on their own).
pub fn car_into(b: &mut MeshBuilder, model: CarModel, paint: u8, lod: Lod, wheels: bool) {
    let kit = Kit::new(paint);
    match model {
        CarModel::Cruiser => cruiser(b, &kit, lod),
        CarModel::Bubble => bubble(b, &kit, lod),
        CarModel::Hover => hover(b, &kit, lod, false, true),
    }
    if wheels {
        if let Some((x, zs, r)) = wheel_layout(model) {
            for z in zs {
                for sx in [-x, x] {
                    wheel(b, DVec3::new(sx, r, z), r, 0.22, lod);
                }
            }
        }
    }
}

/// The flying taxi: the hover car in yellow with a black-and-white check
/// along its flanks and a glowing TAXI lamp on the roof.
///
/// `canopy` false leaves its bubble canopy (with the lamp) off, for a taxi
/// whose canopy is drawn on its own by [`taxi_door`] so it can open.
pub fn taxi_into(b: &mut MeshBuilder, lod: Lod, canopy: bool) {
    hover(b, &Kit::new(5), lod, true, canopy);
}

/// The taxi's door: its bubble canopy with the TAXI lamp, which tilts up
/// and forward like a jet's to let the fare in. Returns the mesh in the
/// hinge's frame (the hinge at the origin, the canopy closed), the hinge's
/// place on the taxi and its axis: turning the canopy by a positive angle
/// about the axis lifts its back (about 0.9 rad is fully open).
pub fn taxi_door(lod: Lod) -> (Mesh, DVec3, DVec3) {
    let mut part = MeshBuilder::new();
    let hinge = hover_canopy(&mut part, &Kit::new(5), lod, true);
    let mut b = MeshBuilder::new();
    b.append(part, -hinge.as_vec3());
    (b.finish(), hinge, DVec3::X)
}

/// The long, low two-tone sedan: tail fins, a wraparound windscreen, a
/// chrome spear and a contrasting cove, dual headlamps, bumper bullets,
/// rear wheel skirts.
fn cruiser(b: &mut MeshBuilder, kit: &Kit, lod: Lod) {
    let section = |z: f64| {
        let front = smooth(1.9, 2.65, z);
        let rear = smooth(-2.0, -2.65, z);
        let hood = smooth(0.9, 2.65, z);
        Section {
            w: 1.0 - 0.03 * front - 0.02 * rear,
            bot: 0.3 + 0.1 * front + 0.1 * rear,
            mid: 0.64,
            top: 0.97 - 0.07 * hood - 0.03 * rear,
            n_up: 5.0,
            n_dn: 3.2,
            tumble: 0.1,
            tuck: 0.16,
        }
    };
    let door = [-0.38, 0.92];
    let seam_w = 0.007;
    let body = Body {
        z: [-2.65, 2.65],
        end_r: [0.12, 0.14],
        section: &section,
        arches: vec![
            Arch {
                z: 1.65,
                cy: 0.36,
                r: 0.47,
                skirt: None,
                x_in: 0.66,
            },
            Arch {
                z: -1.6,
                cy: 0.36,
                r: 0.47,
                skirt: Some(0.45),
                x_in: 0.66,
            },
        ],
        stations: vec![
            door[0] - seam_w,
            door[0],
            door[1],
            door[1] + seam_w,
            2.42,
            2.42 + seam_w,
            -2.4,
            -2.4 - seam_w,
            -2.45,
            0.2,
        ],
        bands: vec![-0.9, -0.43, -0.36, 0.3, 0.37, 0.8, 0.84],
    };
    let k = *kit;
    body.build(b, lod, |at| {
        if at.s < -0.9 || at.well {
            return Kit::plain(k.under);
        }
        let seam = |z0: f64, z1: f64| at.z > z0 && at.z < z1;
        if lod == Lod::Near
            && at.side()
            && at.s > -0.85
            && at.s < 0.8
            && (seam(door[0] - seam_w, door[0]) || seam(door[1], door[1] + seam_w))
        {
            return Kit::plain(k.seam);
        }
        if lod == Lod::Near
            && at.s > 0.84
            && (seam(2.42, 2.42 + seam_w) || seam(-2.4 - seam_w, -2.4))
        {
            return Kit::plain(k.seam);
        }
        if at.side() && at.z > -2.45 && at.z < 2.45 {
            // The spear along the whole flank, a second one under the cove.
            if at.s > 0.3 && at.s < 0.37 {
                return k.metal();
            }
            if at.z < door[1] && at.s > -0.43 && at.s < -0.36 {
                return k.metal();
            }
            if at.z < door[1] && at.z > -2.4 && at.s >= -0.36 && at.s <= 0.3 {
                return Kit::plain(k.top);
            }
        }
        Kit::plain(k.body)
    });
    let deck = |z: f64, x: f64| body.shape(z).y_at(x);

    // The hardtop: wraparound glass front and back, a thick C-pillar.
    let base = |z: f64| deck(z, 0.86) - 0.025;
    let cabin = Cabin {
        z: [-1.35, 1.0],
        roof: [-0.78, 0.38],
        height: 0.47,
        w_base: 0.88,
        w_roof: 0.76,
        n: 4.5,
        base: &base,
        ease: 0.9,
        stations: vec![-0.5, 0.36, 0.44],
        bands: vec![0.05, 0.955, 0.975],
    };
    cabin.build(b, lod, |at| {
        if at.s < 0.05 {
            return k.metal();
        }
        let roof = at.z > -0.78 && at.z < 0.38;
        if roof && at.s > 0.975 {
            return Kit::plain(k.top);
        }
        if roof && at.s > 0.955 {
            return k.metal();
        }
        // The C-pillar, swept back.
        if at.z > -0.78 && at.z < -0.5 && at.c.abs() > 0.2 {
            return Kit::plain(k.top);
        }
        // The A-pillar: a thin chrome post where the screen wraps round.
        if at.z > 0.36 && at.z < 0.44 && at.c.abs() > 0.45 {
            return k.metal();
        }
        k.glazed()
    });

    // Fins rising to the tail, a rocket tail-light in each.
    let fin_base = |z: f64| deck(z.max(-2.6), 0.84);
    for sx in [-1.0, 1.0] {
        fin(
            b,
            lod,
            sx * 0.84,
            [-0.5, -2.78],
            0.33,
            0.055,
            0.25,
            &fin_base,
            Kit::plain(k.body),
        );
        let ty = fin_base(-2.6) + 0.17;
        lathe(
            b,
            DVec3::new(sx * 0.89, ty, -2.66),
            -DVec3::Z,
            &[
                (0.0, -0.02),
                (0.06, -0.02),
                (0.068, 0.03),
                (0.06, 0.05),
                (0.05, 0.1),
                (0.03, 0.14),
                (0.0, 0.16),
            ],
            lod.pick(16, 8, 5),
            |i| if i < 2 { k.metal() } else { Kit::glow(k.red) },
        );
    }

    // Dual headlamps under chrome hoods.
    for sx in [-1.0, 1.0] {
        for x in [0.6, 0.82] {
            lamp_pod(b, lod, DVec3::new(sx * x, 0.76, 2.56), 0.1, &k, k.body);
        }
    }
    if lod == Lod::Far {
        return;
    }

    chrome_ends(b, lod, &k, 2.66, -2.66);
    if lod == Lod::Near {
        // A chrome jet on the hood.
        lathe(
            b,
            DVec3::new(0.0, deck(2.3, 0.0) - 0.01, 2.2),
            DVec3::Z,
            &[
                (0.0, 0.0),
                (0.03, 0.03),
                (0.035, 0.12),
                (0.022, 0.24),
                (0.0, 0.3),
            ],
            10,
            |_| k.metal(),
        );
        let hy = deck(2.3, 0.0) + 0.02;
        tube(
            b,
            &[
                DVec3::new(0.0, hy - 0.02, 2.25),
                DVec3::new(0.0, hy + 0.06, 2.22),
            ],
            (0.004, 0.03),
            6,
            k.metal(),
        );
        // Door handles, mirrors, an aerial, the rear plate.
        for sx in [-1.0, 1.0] {
            let x = body.shape(-0.25).x_at(0.84) + 0.005;
            tube(
                b,
                &[
                    DVec3::new(sx * x, 0.84, -0.28),
                    DVec3::new(sx * x, 0.84, -0.14),
                ],
                (0.016, 0.014),
                8,
                k.metal(),
            );
            let mx = body.shape(0.75).x_at(0.95);
            tube(
                b,
                &[
                    DVec3::new(sx * (mx - 0.02), 0.95, 0.75),
                    DVec3::new(sx * (mx + 0.06), 1.06, 0.72),
                ],
                (0.012, 0.012),
                6,
                k.metal(),
            );
            lathe(
                b,
                DVec3::new(sx * (mx + 0.07), 1.08, 0.72),
                -DVec3::Z,
                &[
                    (0.0, -0.03),
                    (0.06, -0.025),
                    (0.065, 0.0),
                    (0.06, 0.02),
                    (0.0, 0.025),
                ],
                12,
                |_| k.metal(),
            );
        }
        tube(
            b,
            &[DVec3::new(-0.86, 0.94, -2.05), DVec3::new(-0.9, 1.6, -2.3)],
            (0.004, 0.004),
            5,
            k.metal(),
        );
        let plate_y = 0.66;
        b.oriented_box(
            DVec3::new(0.0, plate_y, -2.655),
            DVec3::X,
            DVec3::new(0.26, 0.075, 0.006),
            rgb(236, 226, 160),
        );
    }
}

/// Chrome bumpers round the nose (at `nose`) and the tail, bullets on the
/// front one, and an oval grille with bars and teeth.
fn chrome_ends(b: &mut MeshBuilder, lod: Lod, k: &Kit, nose: f64, tail: f64) {
    let seg = lod.pick(12, 8, 4);
    // Bumpers: a heavy chrome bar round each end; bullets in front.
    let bumper = |zf: f64, sg: f64| {
        let p = |x: f64, z: f64| DVec3::new(x, 0.44, zf + sg * z);
        spline(
            &[
                p(-1.0, -0.42),
                p(-0.98, -0.12),
                p(-0.82, 0.04),
                p(0.0, 0.1),
                p(0.82, 0.04),
                p(0.98, -0.12),
                p(1.0, -0.42),
            ],
            lod.pick(6, 3, 1),
            false,
        )
    };
    tube(b, &bumper(nose, 1.0), (0.07, 0.1), seg, k.metal());
    tube(b, &bumper(tail, -1.0), (0.07, 0.09), seg, k.metal());
    for sx in [-0.42, 0.42] {
        lathe(
            b,
            DVec3::new(sx, 0.5, nose + 0.02),
            DVec3::Z,
            &[
                (0.0, -0.05),
                (0.085, -0.03),
                (0.08, 0.07),
                (0.055, 0.15),
                (0.025, 0.2),
                (0.0, 0.215),
            ],
            seg,
            |_| k.metal(),
        );
    }
    // The grille: a chrome oval, a dark mouth and bars across.
    let gc = DVec3::new(0.0, 0.6, nose - 0.008);
    let oval: Vec<DVec3> = (0..24)
        .map(|i| {
            let a = TAU * i as f64 / 24.0;
            gc + DVec3::new(0.56 * a.cos(), 0.12 * a.sin(), 0.0)
        })
        .collect();
    disc(
        b,
        gc - DVec3::Z * 0.004,
        (DVec3::X * 0.56, DVec3::Y * 0.12),
        24,
        DVec3::Z,
        k.under,
    );
    tube(
        b,
        &spline(&oval, lod.pick(2, 1, 1), true),
        (0.022, 0.022),
        lod.pick(10, 6, 4),
        k.metal(),
    );
    if lod == Lod::Near {
        for dy in [-0.06, 0.0, 0.06] {
            let half = 0.55 * (1.0 - (dy / 0.12f64).powi(2)).sqrt();
            tube(
                b,
                &[
                    gc + DVec3::new(-half, dy, 0.0),
                    gc + DVec3::new(half, dy, 0.0),
                ],
                (0.01, 0.012),
                8,
                k.metal(),
            );
        }
        // Vertical teeth.
        for i in -3..=3 {
            let x = i as f64 * 0.14;
            let hh = 0.11 * (1.0 - (x / 0.56f64).powi(2)).max(0.0).sqrt();
            tube(
                b,
                &[
                    gc + DVec3::new(x, -hh, 0.004),
                    gc + DVec3::new(x, hh, 0.004),
                ],
                (0.008, 0.008),
                6,
                k.metal(),
            );
        }
    }
}

/// The 1950s coach-built ambulance: a long white limousine body with a
/// tall rear compartment, frosted windows, a red stripe and crosses, small
/// fins, and twin red bubble lights with a chrome siren on the roof.
pub fn ambulance_into(b: &mut MeshBuilder, lod: Lod) {
    let white = rgb(242, 240, 232);
    let red = rgb(196, 30, 40);
    let k = Kit {
        body: white,
        top: white,
        seam: rgb(150, 150, 146),
        ..Kit::new(0)
    };
    let frosted = rgb(200, 214, 218);
    let section = |z: f64| {
        let front = smooth(2.3, 3.0, z);
        let rear = smooth(-2.5, -3.1, z);
        let hood = smooth(1.3, 3.0, z);
        Section {
            w: 1.02 - 0.03 * front - 0.02 * rear,
            bot: 0.32 + 0.1 * front + 0.08 * rear,
            mid: 0.68,
            top: 1.02 - 0.08 * hood - 0.02 * rear,
            n_up: 5.0,
            n_dn: 3.2,
            tumble: 0.08,
            tuck: 0.14,
        }
    };
    let doors = [-2.95, 0.0, 1.32];
    let seam_w = 0.007;
    let mut stations = vec![-2.98, 2.82, 2.82 + seam_w];
    for d in doors {
        stations.extend([d, d + seam_w]);
    }
    let body = Body {
        z: [-3.1, 3.0],
        end_r: [0.1, 0.14],
        section: &section,
        arches: vec![
            Arch {
                z: 1.95,
                cy: 0.38,
                r: 0.48,
                skirt: None,
                x_in: 0.68,
            },
            Arch {
                z: -1.85,
                cy: 0.38,
                r: 0.48,
                skirt: Some(0.5),
                x_in: 0.68,
            },
        ],
        stations,
        bands: vec![-0.9, 0.02, 0.24, 0.3, 0.8],
    };
    body.build(b, lod, |at| {
        if at.s < -0.9 || at.well {
            return Kit::plain(k.under);
        }
        let seam = doors.iter().any(|&d| at.z > d && at.z < d + seam_w);
        if lod == Lod::Near && seam && at.side() && at.s > -0.85 && at.s < 0.8 {
            return Kit::plain(k.seam);
        }
        if at.side() && at.z > -2.98 && at.z < 2.82 {
            if at.s > 0.02 && at.s < 0.24 {
                return Kit::plain(red);
            }
            if at.s > 0.24 && at.s < 0.3 {
                return k.metal();
            }
        }
        Kit::plain(white)
    });
    let deck = |z: f64, x: f64| body.shape(z).y_at(x);

    // The tall compartment, running from the windscreen to the tail.
    let base = |z: f64| deck(z, 0.9) - 0.025;
    let cabin = Cabin {
        z: [-3.04, 1.42],
        roof: [-2.96, 0.78],
        height: 0.86,
        w_base: 0.94,
        w_roof: 0.88,
        n: 7.0,
        base: &base,
        ease: 0.55,
        stations: vec![-2.75, -2.05, -1.95, -0.45, -0.25, -0.1, 0.62, 0.7],
        bands: vec![0.04, 0.36, 0.84, 0.97],
    };
    let win = |z0: f64, z1: f64, at: &At| at.z > z0 && at.z < z1 && at.s > 0.36 && at.s < 0.84;
    cabin.build(b, lod, |at| {
        if at.s < 0.04 {
            return k.metal();
        }
        if at.s > 0.97 || at.z < -2.96 || !at.side() && at.z < 0.78 {
            // The roof, and the rear doors (with windows of their own).
            if at.z < -2.96 && at.s > 0.36 && at.s < 0.84 && at.c.abs() < 0.6 && lod != Lod::Far {
                return Some((Surf::Glass, frosted));
            }
            return Kit::plain(white);
        }
        if at.z > 0.78 {
            return k.glazed();
        }
        if win(-0.1, 0.62, &at) {
            return k.glazed();
        }
        if win(-1.95, -0.45, &at) {
            return Some((Surf::Glass, frosted));
        }
        Kit::plain(white)
    });
    // Red crosses on the panels behind the frosted windows.
    if lod != Lod::Far {
        let (zc, yc) = (-2.4, base(-2.4) + 0.5);
        for sx in [-1.0f64, 1.0] {
            let x = cabin.side_x(zc, yc) + 0.006;
            let nrm = DVec3::X * sx;
            for (u, half) in [
                (DVec3::Y, DVec3::new(0.004, 0.2, 0.07)),
                (DVec3::Z, DVec3::new(0.004, 0.2, 0.07)),
            ] {
                slab(b, DVec3::new(sx * x, yc, zc), [nrm, u], half, red);
            }
        }
    }
    // Small fins with round tail lamps.
    let fin_base = |z: f64| deck(z.max(-3.0), 0.86);
    for sx in [-1.0, 1.0] {
        fin(
            b,
            lod,
            sx * 0.86,
            [-1.6, -3.16],
            0.18,
            0.05,
            0.2,
            &fin_base,
            Kit::plain(white),
        );
        lathe(
            b,
            DVec3::new(sx * 0.9, fin_base(-3.0) + 0.1, -3.08),
            -DVec3::Z,
            &[
                (0.0, -0.02),
                (0.06, -0.02),
                (0.068, 0.03),
                (0.05, 0.08),
                (0.0, 0.11),
            ],
            lod.pick(14, 8, 5),
            |i| if i < 1 { k.metal() } else { Kit::glow(k.red) },
        );
    }
    // Twin red bubble lights and a chrome siren on the roof's front.
    let roof_y = base(0.6) + cabin.height_at(0.6);
    for sx in [-0.42, 0.42] {
        lathe(
            b,
            DVec3::new(sx, roof_y - 0.02, 0.5),
            DVec3::Y,
            &[
                (0.0, 0.0),
                (0.1, 0.0),
                (0.1, 0.04),
                (0.095, 0.06),
                (0.08, 0.14),
                (0.05, 0.19),
                (0.0, 0.21),
            ],
            lod.pick(18, 10, 6),
            |i| {
                if i < 2 {
                    k.metal()
                } else {
                    Kit::glow(rgb(255, 40, 30))
                }
            },
        );
    }
    lathe(
        b,
        DVec3::new(0.0, roof_y + 0.08, 0.35),
        DVec3::Z,
        &[
            (0.0, -0.12),
            (0.07, -0.1),
            (0.075, 0.08),
            (0.06, 0.14),
            (0.0, 0.16),
        ],
        lod.pick(16, 8, 5),
        |_| k.metal(),
    );
    for z in [-1.85, 1.95] {
        for sx in [-0.86, 0.86] {
            wheel(b, DVec3::new(sx, 0.38, z), 0.38, 0.22, lod);
        }
    }
    for sx in [-1.0, 1.0] {
        for x in [0.6, 0.83] {
            lamp_pod(b, lod, DVec3::new(sx * x, 0.8, 2.9), 0.1, &k, white);
        }
    }
    if lod == Lod::Far {
        return;
    }
    chrome_ends(b, lod, &k, 3.01, -3.11);
    if lod == Lod::Near {
        for sx in [-1.0, 1.0] {
            let x = body.shape(0.3).x_at(0.86) + 0.005;
            tube(
                b,
                &[
                    DVec3::new(sx * x, 0.86, 0.1),
                    DVec3::new(sx * x, 0.86, 0.24),
                ],
                (0.016, 0.014),
                8,
                k.metal(),
            );
        }
        b.oriented_box(
            DVec3::new(0.0, 0.7, -3.105),
            DVec3::X,
            DVec3::new(0.26, 0.075, 0.006),
            rgb(236, 226, 160),
        );
    }
}

/// A headlamp in a body-coloured pod with a chrome bezel, its lens to +z.
fn lamp_pod(b: &mut MeshBuilder, lod: Lod, at: DVec3, r: f64, k: &Kit, pod: Color) {
    let seg = lod.pick(20, 10, 6);
    let prof = [
        (0.0, -0.22),
        (r * 1.25, -0.22),
        (r * 1.25, -0.02),
        (r * 1.18, 0.02),
        (r * 1.12, 0.035),
        (r * 1.02, 0.04),
        (r * 0.95, 0.035),
        (r * 0.8, 0.055),
        (r * 0.45, 0.07),
        (0.0, 0.075),
    ];
    lathe(b, at, DVec3::Z, &prof, seg, |i| match i {
        0..=2 => Kit::plain(pod),
        3..=5 => k.metal(),
        _ => Kit::glow(k.warm),
    });
}

/// The little car under a glass bubble: an egg of a body, a continental
/// spare wheel on its tail, round lamps on its wings.
fn bubble(b: &mut MeshBuilder, kit: &Kit, lod: Lod) {
    let section = |z: f64| {
        let u = (z / 1.7).clamp(-1.0, 1.0);
        let front = smooth(0.6, 1.7, z);
        Section {
            w: 0.84 * (1.0 - u.powi(6)).max(0.05).powf(0.25),
            bot: 0.24 + 0.06 * front,
            mid: 0.62,
            top: 0.92 - 0.12 * front,
            n_up: 3.0,
            n_dn: 2.6,
            tumble: 0.12,
            tuck: 0.2,
        }
    };
    let body = Body {
        z: [-1.62, 1.7],
        end_r: [0.3, 0.38],
        section: &section,
        arches: vec![
            Arch {
                z: 1.1,
                cy: 0.3,
                r: 0.38,
                skirt: Some(0.47),
                x_in: 0.48,
            },
            Arch {
                z: -1.05,
                cy: 0.3,
                r: 0.38,
                skirt: Some(0.47),
                x_in: 0.48,
            },
        ],
        stations: vec![-1.4, 1.45],
        bands: vec![-0.9, -0.2, -0.14, 0.16, 0.22],
    };
    let k = *kit;
    body.build(b, lod, |at| {
        if at.s < -0.9 || at.well {
            return Kit::plain(k.under);
        }
        if at.side() && at.z > -1.4 && at.z < 1.45 {
            if (at.s > -0.2 && at.s < -0.14) || (at.s > 0.16 && at.s < 0.22) {
                return k.metal();
            }
            if at.s >= -0.14 && at.s <= 0.16 {
                return Kit::plain(k.top);
            }
        }
        Kit::plain(k.body)
    });
    let deck = |z: f64, x: f64| body.shape(z).y_at(x);
    let base = |z: f64| deck(z, 0.66) - 0.03;
    let canopy = Cabin {
        z: [-1.0, 1.02],
        roof: [-0.15, 0.2],
        height: 0.56,
        w_base: 0.7,
        w_roof: 0.5,
        n: 2.4,
        base: &base,
        ease: 0.75,
        stations: vec![],
        bands: vec![0.07, 0.99],
    };
    canopy.build(b, lod, |at| {
        if at.s < 0.07 || (lod == Lod::Near && at.s > 0.99) {
            return k.metal();
        }
        k.glazed()
    });
    // Little fins on the hips.
    let fin_base = |z: f64| deck(z.max(-1.5), 0.62);
    for sx in [-1.0, 1.0] {
        fin(
            b,
            lod,
            sx * 0.62,
            [-0.55, -1.6],
            0.16,
            0.04,
            0.4,
            &fin_base,
            Kit::plain(k.body),
        );
        lathe(
            b,
            DVec3::new(sx * 0.66, fin_base(-1.5) + 0.07, -1.56),
            -DVec3::Z,
            &[
                (0.0, -0.02),
                (0.045, -0.02),
                (0.05, 0.02),
                (0.035, 0.07),
                (0.0, 0.09),
            ],
            lod.pick(12, 6, 4),
            |i| if i < 1 { k.metal() } else { Kit::glow(k.red) },
        );
        lamp_pod(
            b,
            lod,
            DVec3::new(sx * 0.5, deck(1.42, 0.5) - 0.05, 1.42),
            0.1,
            &k,
            k.body,
        );
    }
    if lod == Lod::Far {
        return;
    }
    let seg = lod.pick(12, 8, 4);
    // Slim chrome bumpers.
    let bar = |zf: f64, sg: f64| {
        let p = |x: f64, z: f64| DVec3::new(x, 0.36, zf + sg * z);
        spline(
            &[
                p(-0.72, -0.25),
                p(-0.62, 0.0),
                p(0.0, 0.08),
                p(0.62, 0.0),
                p(0.72, -0.25),
            ],
            lod.pick(6, 3, 1),
            false,
        )
    };
    tube(b, &bar(1.66, 1.0), (0.04, 0.05), seg, k.metal());
    tube(b, &bar(-1.6, -1.0), (0.04, 0.05), seg, k.metal());
    // The continental kit: the spare wheel standing on the tail.
    let r = 0.27;
    lathe(
        b,
        DVec3::new(0.0, 0.6, -1.62),
        -DVec3::Z,
        &[
            (0.0, -0.05),
            (r, -0.05),
            (r + 0.02, 0.0),
            (r + 0.015, 0.06),
            (r - 0.01, 0.1),
            (r * 0.55, 0.12),
            (r * 0.25, 0.14),
            (0.0, 0.145),
        ],
        lod.pick(28, 12, 6),
        |i| match i {
            2 | 3 => k.metal(),
            5 | 6 => k.metal(),
            _ => Kit::plain(k.top),
        },
    );
}

/// The hover car: a jet-age teardrop under a bubble canopy, chrome
/// nacelles glowing at the tail, a swept fin and a glow plate underneath.
/// The hover car's hull, as a function of z, for the hull and the parts
/// that sit on it.
fn hover_section(z: f64) -> Section {
    let nose = smooth(1.2, 2.35, z);
    let tail = smooth(-0.4, -2.3, z);
    Section {
        w: 0.95 * (1.0 - 0.45 * tail) * (1.0 - 0.15 * nose),
        bot: 0.7 + 0.1 * nose + 0.12 * tail,
        mid: 0.98 + 0.02 * tail,
        top: 1.28 - 0.14 * nose - 0.12 * tail,
        n_up: 2.8,
        n_dn: 2.4,
        tumble: 0.08,
        tuck: 0.3,
    }
}

/// The hover car's bubble canopy (and the taxi's lamp on it), hinged at
/// its front foot; returns that hinge point.
fn hover_canopy(b: &mut MeshBuilder, k: &Kit, lod: Lod, taxi: bool) -> DVec3 {
    let base = |z: f64| hover_section(z).y_at(0.6) - 0.03;
    let canopy = Cabin {
        z: [-0.75, 1.35],
        roof: [-0.05, 0.4],
        height: 0.46,
        w_base: 0.64,
        w_roof: 0.46,
        n: 2.3,
        base: &base,
        ease: 0.7,
        stations: vec![],
        bands: vec![0.07, 0.993],
    };
    let paint = |at: At| {
        if at.s < 0.07 || (lod == Lod::Near && at.s > 0.993) {
            return k.metal();
        }
        k.glazed()
    };
    canopy.build(b, lod, paint);
    // Its inner face, seen from below when it is open.
    if lod != Lod::Far {
        inside(|| canopy.build(b, lod, paint));
    }
    if taxi {
        // The TAXI lamp on the canopy, on a chrome saddle.
        let top = base(0.15) + 0.46;
        tube(
            b,
            &[
                DVec3::new(-0.26, top + 0.1, 0.15),
                DVec3::new(0.26, top + 0.1, 0.15),
            ],
            (0.13, 0.09),
            lod.pick(16, 8, 6),
            Kit::glow(rgb(255, 214, 70)),
        );
        tube(
            b,
            &[
                DVec3::new(-0.3, top + 0.01, 0.15),
                DVec3::new(0.3, top + 0.01, 0.15),
            ],
            (0.08, 0.025),
            lod.pick(12, 6, 4),
            k.metal(),
        );
    }
    DVec3::new(0.0, base(1.35), 1.35)
}

/// The hover car's cabin seen through the canopy: leather floor, a rear
/// bench, a front seat, a chrome-rimmed dash and a steering wheel.
fn hover_interior(b: &mut MeshBuilder, k: &Kit, lod: Lod) {
    if lod == Lod::Far {
        return;
    }
    let leather = rgb(150, 36, 40);
    let deck = |z: f64, x: f64| hover_section(z).y_at(x);
    // The floor: a dark skin just over the deck, inside the canopy's foot.
    let (nx, nz) = (lod.pick(8, 4, 2), lod.pick(14, 6, 2));
    let rows: Vec<Vec<DVec3>> = (0..=nz)
        .map(|i| {
            let z = -0.68 + 1.95 * i as f64 / nz as f64;
            (0..=nx)
                .map(|j| {
                    let x = -0.56 + 1.12 * j as f64 / nx as f64;
                    DVec3::new(x, deck(z, x) + 0.012, z)
                })
                .collect()
        })
        .collect();
    b.grid(&rows, false, true, |_, _| Kit::plain(rgb(40, 30, 32)));
    // (z, half width, backrest height): the rear bench sits under the
    // canopy's falling tail, so its back is lower.
    let seats = [(-0.28, 0.36, 0.09), (0.3, 0.4, 0.12)];
    for (z, w, back) in seats {
        let y = deck(z, 0.0);
        cushion(
            b,
            lod,
            DVec3::new(0.0, y + 0.07, z),
            DVec3::new(w, 0.07, 0.17),
            0.0,
            Kit::plain(leather),
        );
        cushion(
            b,
            lod,
            DVec3::new(0.0, y + 0.06 + back, z - 0.17),
            DVec3::new(w, back, 0.05),
            -0.25,
            Kit::plain(leather),
        );
    }
    // The dash: a padded cowl with a chrome rim and glowing dials.
    let z = 0.95;
    let y = deck(z, 0.0);
    cushion(
        b,
        lod,
        DVec3::new(0.0, y + 0.1, z),
        DVec3::new(0.45, 0.08, 0.1),
        0.3,
        Kit::plain(k.body),
    );
    if lod == Lod::Near {
        tube(
            b,
            &[
                DVec3::new(-0.45, y + 0.18, z - 0.06),
                DVec3::new(0.45, y + 0.18, z - 0.06),
            ],
            (0.012, 0.012),
            6,
            k.metal(),
        );
        for dx in [-0.34, -0.16] {
            b.surf = Surf::Glow;
            disc(
                b,
                DVec3::new(dx, y + 0.11, z - 0.105),
                (DVec3::X * 0.045, DVec3::Y * 0.045),
                12,
                -DVec3::Z,
                rgb(160, 240, 255),
            );
            b.surf = Surf::Plain;
        }
        // The steering wheel, before the left seat.
        let c = DVec3::new(-0.25, y + 0.2, z - 0.2);
        let tilt = 0.5f64;
        let (u, v) = (DVec3::X, DVec3::new(0.0, tilt.cos(), tilt.sin()));
        let ring: Vec<DVec3> = (0..=24)
            .map(|i| {
                let a = TAU * i as f64 / 24.0;
                c + (u * a.cos() + v * a.sin()) * 0.11
            })
            .collect();
        tube(b, &ring, (0.012, 0.012), 6, Kit::plain(rgb(240, 236, 224)));
        tube(
            b,
            &[c, c + DVec3::new(0.0, -0.03, 0.15)],
            (0.012, 0.012),
            6,
            k.metal(),
        );
    }
}

/// A padded cushion: a rounded box with half sizes `half`, tipped back by
/// `tilt` (radians about x).
fn cushion(b: &mut MeshBuilder, lod: Lod, c: DVec3, half: DVec3, tilt: f64, paint: Paint) {
    let (seg, rings) = (lod.pick(16, 8, 4), lod.pick(8, 4, 2));
    let f = |v: f64, e: f64| v.signum() * v.abs().powf(e);
    let (st, ct) = tilt.sin_cos();
    let rows: Vec<Vec<DVec3>> = (0..=rings)
        .map(|k| {
            let t = PI * (k as f64 / rings as f64 - 0.5);
            (0..seg)
                .map(|i| {
                    let a = TAU * i as f64 / seg as f64;
                    let p = DVec3::new(
                        half.x * f(t.cos(), 0.3) * f(a.cos(), 0.3),
                        half.y * f(t.sin(), 0.5),
                        half.z * f(t.cos(), 0.3) * f(a.sin(), 0.5),
                    );
                    c + DVec3::new(p.x, p.y * ct - p.z * st, p.y * st + p.z * ct)
                })
                .collect()
        })
        .collect();
    surface(b, &rows, true, |_, _| paint);
}

fn hover(b: &mut MeshBuilder, kit: &Kit, lod: Lod, taxi: bool, canopy: bool) {
    // The checker: cells of a fifth of a metre along the flanks.
    let cells: Vec<f64> = (0..=18).map(|i| -1.9 + 0.2 * i as f64).collect();
    let mut stations = vec![-2.15, 1.95];
    if taxi {
        stations.extend(cells.iter().copied());
    }
    let body = Body {
        z: [-2.35, 2.35],
        end_r: [0.18, 0.5],
        section: &hover_section,
        arches: vec![],
        stations,
        bands: if taxi {
            vec![-0.9, -0.06, 0.0, 0.1, 0.2, 0.26, 0.6, 0.65]
        } else {
            vec![-0.9, -0.06, 0.0, 0.26, 0.32, 0.6, 0.65]
        },
    };
    let k = *kit;
    let glow_blue = rgb(120, 232, 255);
    body.build(b, lod, |at| {
        if at.s < -0.9 || at.well {
            return Kit::plain(k.under);
        }
        if at.side() && at.z > -2.15 && at.z < 1.95 {
            if at.s > -0.06 && at.s < 0.0 {
                return k.metal();
            }
            if taxi {
                if at.s >= 0.0 && at.s < 0.2 && at.z > -1.9 && at.z < 1.7 && lod != Lod::Far {
                    let cell = ((at.z + 1.9) / 0.2).floor() as i64;
                    let row = i64::from(at.s >= 0.1);
                    let dark = (cell + row) % 2 == 0;
                    return Kit::plain(if dark {
                        rgb(22, 22, 24)
                    } else {
                        rgb(245, 245, 240)
                    });
                }
                if at.s > 0.2 && at.s < 0.26 {
                    return k.metal();
                }
            } else {
                if at.s >= 0.0 && at.s < 0.26 {
                    return Kit::plain(k.top);
                }
                if at.s > 0.26 && at.s < 0.32 {
                    return k.metal();
                }
            }
        }
        if at.s > 0.6 && at.s < 0.65 && at.z > -2.15 && at.z < 1.95 {
            return k.metal();
        }
        Kit::plain(k.body)
    });
    let deck = |z: f64, x: f64| body.shape(z).y_at(x);
    hover_interior(b, &k, lod);
    if canopy {
        hover_canopy(b, &k, lod, taxi);
    }

    // Nacelles on short pylons, glowing at the tail.
    let seg = lod.pick(24, 12, 6);
    for sx in [-1.0, 1.0] {
        let c = DVec3::new(sx * 1.12, 0.88, -0.35);
        lathe(
            b,
            c,
            DVec3::Z,
            &[
                (0.0, -1.34),
                (0.13, -1.34),
                (0.16, -1.32),
                (0.2, -1.2),
                (0.23, -0.8),
                (0.235, 0.0),
                (0.22, 0.6),
                (0.17, 0.92),
                (0.1, 1.08),
                (0.0, 1.14),
            ],
            seg,
            |i| match i {
                0 => Kit::glow(rgb(255, 150, 70)),
                1 => k.metal(),
                3 | 4 if !taxi => Kit::plain(k.top),
                _ => k.metal(),
            },
        );
        if lod != Lod::Far {
            tube(
                b,
                &[
                    DVec3::new(sx * 0.7, 0.95, -0.4),
                    DVec3::new(sx * 0.95, 0.9, -0.42),
                ],
                (0.05, 0.11),
                lod.pick(12, 8, 4),
                Kit::plain(k.body),
            );
        }
        // Small canted fins on the nacelle tails.
        let nb = |_z: f64| 1.06;
        fin(
            b,
            lod,
            sx * 1.12,
            [-0.95, -1.62],
            0.28,
            0.03,
            0.7,
            &nb,
            Kit::plain(if taxi { rgb(22, 22, 24) } else { k.top }),
        );
    }
    // The swept tail fin on the centre line.
    let fin_base = |z: f64| deck(z.max(-2.25), 0.0);
    fin(
        b,
        lod,
        0.0,
        [-1.2, -2.45],
        0.42,
        0.045,
        0.0,
        &fin_base,
        Kit::plain(if taxi { rgb(22, 22, 24) } else { k.top }),
    );
    // Glow plate underneath.
    b.surf = Surf::Glow;
    disc(
        b,
        DVec3::new(0.0, body.sec(0.0).bot - 0.012, 0.0),
        (DVec3::X * 0.72, DVec3::Z * 1.7),
        lod.pick(32, 16, 8),
        -DVec3::Y,
        glow_blue,
    );
    b.surf = Surf::Plain;
    // Headlamps set into the nose.
    for sx in [-1.0, 1.0] {
        let z = 2.08;
        let x = 0.42;
        let y = body.sec(z).mid + 0.02;
        let zs = z + (body.sec(z).x_at(y) - x).max(0.0) * 0.4;
        lamp_pod(b, lod, DVec3::new(sx * x, y, zs), 0.09, &k, k.body);
    }
    if lod == Lod::Near {
        // An aerial.
        tube(
            b,
            &[
                DVec3::new(0.0, deck(-1.3, 0.0), -1.3),
                DVec3::new(0.0, 1.85, -1.75),
            ],
            (0.004, 0.004),
            5,
            k.metal(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tris(model: CarModel, lod: Lod) -> usize {
        let mut b = MeshBuilder::new();
        car_into(&mut b, model, 1, lod, true);
        b.finish().build().triangle_count()
    }

    #[test]
    fn cars_are_detailed_close_and_light_far() {
        for m in [CarModel::Cruiser, CarModel::Bubble, CarModel::Hover] {
            let (near, mid, far) = (tris(m, Lod::Near), tris(m, Lod::Mid), tris(m, Lod::Far));
            assert!(near >= 5_000, "{m:?} close up has only {near} triangles");
            assert!(near <= 30_000, "{m:?} close up is too heavy: {near}");
            assert!(mid * 3 <= near, "{m:?} mid level {mid} vs {near}");
            assert!(far <= 2_000, "{m:?} far level has {far} triangles");
        }
    }

    #[test]
    fn the_taxi_canopy_is_the_closed_taxis_canopy() {
        for lod in [Lod::Near, Lod::Mid, Lod::Far] {
            let count = |canopy: bool| {
                let mut b = MeshBuilder::new();
                taxi_into(&mut b, lod, canopy);
                b.finish().build().triangle_count()
            };
            let (door, hinge, axis) = taxi_door(lod);
            let door = door.build();
            assert_eq!(count(false) + door.triangle_count(), count(true));
            assert_eq!(axis, DVec3::X);
            // The hinge sits on the deck, at the canopy's front foot.
            assert!(hinge.y > 1.0 && hinge.y < 1.4 && hinge.z > 1.0, "{hinge}");
        }
    }

    #[test]
    fn bodies_shade_smooth_and_face_out() {
        let mut b = MeshBuilder::new();
        cruiser(&mut b, &Kit::new(0), Lod::Near);
        let m = b.finish();
        assert!(m.has_normals());
        let built = m.build();
        // Every triangle's winding agrees with its vertices' normals.
        let (pos, nrm, idx) = (&built.positions, &built.normals, &built.indices);
        let mut wrong = 0;
        for t in idx.chunks_exact(3) {
            let [a, b2, c] = [t[0], t[1], t[2]].map(|i| i as usize);
            let geo = (pos[b2] - pos[a]).cross(pos[c] - pos[a]);
            if geo.length() < 1e-9 {
                continue;
            }
            if geo.dot(nrm[a] + nrm[b2] + nrm[c]) < 0.0 {
                wrong += 1;
            }
        }
        // A few slivers at collapsed tips may disagree; nothing more.
        assert!(wrong * 1000 < idx.len() / 3, "{wrong} triangles face in");
        // The roof's normals point up.
        let (_, hi) = m.bounds().expect("bounds");
        let top = pos
            .iter()
            .zip(nrm)
            .filter(|(p, _)| p.y > hi.y - 0.02 && p.x.abs() < 0.2)
            .map(|(_, n)| n.y)
            .fold(f32::MAX, f32::min);
        assert!(top > 0.5, "roof normal y {top}");
    }

    #[test]
    fn wheels_touch_the_ground() {
        let mut b = MeshBuilder::new();
        car_into(&mut b, CarModel::Cruiser, 0, Lod::Near, true);
        let (lo, _) = b.finish().bounds().expect("bounds");
        assert!(lo.y.abs() < 0.01, "lowest point {}", lo.y);
    }
}
