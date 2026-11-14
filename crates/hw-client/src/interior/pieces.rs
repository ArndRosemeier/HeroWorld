//! Furniture, one piece at a time, in the golden age's taste: streamlined
//! sofas on tapered legs, kidney tables, radio consoles, rounded
//! refrigerators, steel desks with typewriters.
//!
//! Every piece is built in its own frame ([`Maker`]): x to its right, y up,
//! z out of its front, the origin on the floor under its middle. Round
//! parts are lathed or lofted with shared normals, so they shade smooth;
//! upholstery is a rounded block (a superellipsoid), never a plain box.
//!
//! Two rules keep the z-fight check clean: nothing has a face where it
//! stands (a lathe starts open, a block standing on something has no
//! bottom), and parts laid on other parts stand at least 12 mm proud of
//! them or sink 2 cm into them, never a hair apart.

use crate::carmesh::{lathe, surface, tube, Paint};
use crate::citymesh::{MeshBuilder, Surf};
use crate::skins::{Base, Skin, Tex, Wear};
use engine::prelude::*;
use glam::{DVec2, DVec3};
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// What a part is made of.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mat {
    /// Polished veneer in this wood's colour.
    Wood(Color),
    /// Upholstery.
    Cloth(Color),
    /// Painted wood or enamelled steel.
    Paint(Color),
    Chrome,
    Brass,
    /// Bakelite, rubber, leather: smooth, no texture.
    Plain(Color),
    /// See-through glass with a tint.
    Glass(Color),
    /// Always glowing: a radio dial, a sign.
    Glow(Color),
    /// A lamp's bulb or globe: glowing when its household's lights are on
    /// (a glow channel, see `daylight`), frosted glass by day.
    Lamp(u8),
}

pub fn chrome() -> Color {
    rgb(216, 220, 228)
}

pub fn brass() -> Color {
    rgb(214, 170, 92)
}

/// Builds pieces in a frame.
pub struct Maker<'a> {
    pub b: &'a mut MeshBuilder,
    o: DVec3,
    x: DVec3,
    z: DVec3,
    /// Sides of round parts (24 near the camera).
    pub seg: usize,
    pub wear: Wear,
}

impl<'a> Maker<'a> {
    /// A frame at `o` (a city point on the floor) whose front faces `front`
    /// (horizontal, need not be unit).
    pub fn new(b: &'a mut MeshBuilder, o: DVec3, front: DVec3, seg: usize, wear: Wear) -> Self {
        let z = DVec3::new(front.x, 0.0, front.z).normalize_or(DVec3::Z);
        Self {
            b,
            o,
            x: DVec3::Y.cross(z),
            z,
            seg: seg.max(6),
            wear,
        }
    }

    /// The same builder in a frame moved by `(x, y, z)` (local) and turned
    /// by `yaw` (radians, from z towards x).
    pub fn sub(&mut self, at: DVec3, yaw: f64) -> Maker<'_> {
        let (s, c) = yaw.sin_cos();
        let z = self.z * c + self.x * s;
        Maker {
            o: self.p(at),
            x: DVec3::Y.cross(z),
            z,
            seg: self.seg,
            wear: self.wear,
            b: &mut *self.b,
        }
    }

    /// A local point in the city.
    pub fn p(&self, v: DVec3) -> DVec3 {
        self.o + self.x * v.x + DVec3::Y * v.y + self.z * v.z
    }

    /// A local direction in the city.
    pub fn d(&self, v: DVec3) -> DVec3 {
        self.x * v.x + DVec3::Y * v.y + self.z * v.z
    }

    /// Sets the builder up for `m`; the colour to draw it in.
    fn mat(&mut self, m: Mat) -> Color {
        self.b.surf = Surf::Plain;
        self.b.tex = None;
        let skin = |base| -> Option<Tex> { Some(Skin::new(base, self.wear).into()) };
        match m {
            Mat::Wood(c) => {
                self.b.tex = skin(Base::Veneer);
                c
            }
            Mat::Cloth(c) => {
                self.b.tex = skin(Base::Fabric);
                c
            }
            Mat::Paint(c) => {
                self.b.tex = skin(Base::PaintedWood);
                c
            }
            Mat::Plain(c) => c,
            Mat::Chrome => {
                self.b.surf = Surf::Metal;
                chrome()
            }
            Mat::Brass => {
                self.b.surf = Surf::Metal;
                brass()
            }
            Mat::Glass(c) => {
                self.b.surf = Surf::Glass;
                c.with_alpha(0.35)
            }
            Mat::Glow(c) => {
                self.b.surf = Surf::Glow;
                c
            }
            // Channel 0: a room whose lights stay off, a frosted shade.
            Mat::Lamp(0) => {
                self.b.surf = Surf::Plain;
                rgb(236, 226, 206)
            }
            Mat::Lamp(ch) => {
                self.b.surf = Surf::Lit(ch);
                rgb(255, 228, 178)
            }
        }
    }

    fn paint(&mut self, m: Mat) -> Paint {
        let c = self.mat(m);
        Some((self.b.surf, c))
    }

    fn reset(&mut self) {
        self.b.surf = Surf::Plain;
        self.b.tex = None;
    }

    /// A box from `lo` to `hi` (local, axis-aligned). With its bottom at 0
    /// it stands on something and has no bottom face.
    pub fn block(&mut self, m: Mat, lo: DVec3, hi: DVec3) {
        let (lo, hi) = (lo.min(hi), lo.max(hi));
        let c = self.mat(m);
        let p = |x: f64, y: f64, z: f64| self.p(DVec3::new(x, y, z));
        let (x0, x1, y0, y1, z0, z1) = (lo.x, hi.x, lo.y, hi.y, lo.z, hi.z);
        let mut faces = vec![
            (
                [p(x0, y1, z0), p(x1, y1, z0), p(x1, y1, z1), p(x0, y1, z1)],
                DVec3::Y,
            ),
            (
                [p(x0, y0, z0), p(x1, y0, z0), p(x1, y1, z0), p(x0, y1, z0)],
                -DVec3::Z,
            ),
            (
                [p(x0, y0, z1), p(x1, y0, z1), p(x1, y1, z1), p(x0, y1, z1)],
                DVec3::Z,
            ),
            (
                [p(x0, y0, z0), p(x0, y0, z1), p(x0, y1, z1), p(x0, y1, z0)],
                -DVec3::X,
            ),
            (
                [p(x1, y0, z0), p(x1, y0, z1), p(x1, y1, z1), p(x1, y1, z0)],
                DVec3::X,
            ),
        ];
        if lo.y.abs() > 1e-9 {
            faces.push((
                [p(x0, y0, z0), p(x1, y0, z0), p(x1, y0, z1), p(x0, y0, z1)],
                -DVec3::Y,
            ));
        }
        for (q, n) in faces {
            let n = self.d(n);
            self.b.quad(q, n, c);
        }
        self.reset();
    }

    /// A rounded block (superellipsoid) centred on `c` with half sizes
    /// `half` along the local axes turned by `tilt` (radians about x, top
    /// leaning back). `e` is how square it is around (horizontal) and up
    /// and down: 1 an ellipsoid, 0.2 a box with soft edges.
    pub fn blob(&mut self, m: Mat, c: DVec3, half: DVec3, tilt: f64, e: (f64, f64)) {
        let (st, ct) = tilt.sin_cos();
        let axes = [DVec3::X, DVec3::new(0.0, ct, -st), DVec3::new(0.0, st, ct)];
        self.blob_axes(m, c, axes, half, e);
    }

    /// A rounded block along any three local axes (unit, at right angles).
    pub fn blob_axes(&mut self, m: Mat, c: DVec3, axes: [DVec3; 3], half: DVec3, e: (f64, f64)) {
        let paint = self.paint(m);
        let seg = (self.seg / 4).max(2) * 4;
        let rings = (self.seg / 3).max(4);
        let f = |v: f64, e: f64| v.signum() * v.abs().powf(e);
        let rows: Vec<Vec<DVec3>> = (0..=rings)
            .map(|k| {
                let t = PI * (k as f64 / rings as f64 - 0.5);
                (0..seg)
                    .map(|i| {
                        // Start between the corners so each corner gets
                        // its own column.
                        let a = TAU * (i as f64 + 0.5) / seg as f64;
                        let l = DVec3::new(
                            half.x * f(t.cos(), e.1) * f(a.cos(), e.0),
                            half.y * f(t.sin(), e.1),
                            half.z * f(t.cos(), e.1) * f(a.sin(), e.0),
                        );
                        self.p(c + axes[0] * l.x + axes[1] * l.y + axes[2] * l.z)
                    })
                    .collect()
            })
            .collect();
        surface(self.b, &rows, true, |_, _| paint);
        self.reset();
    }

    /// A solid of revolution about the local y axis standing on `at`:
    /// `profile` is (radius, height); start at a radius above 0 to leave
    /// the bottom open.
    pub fn lathe(&mut self, m: Mat, at: DVec3, profile: &[(f64, f64)]) {
        self.lathe_along(m, at, DVec3::Y, profile);
    }

    /// The same about any local axis.
    pub fn lathe_along(&mut self, m: Mat, at: DVec3, axis: DVec3, profile: &[(f64, f64)]) {
        let paint = self.paint(m);
        let (o, a) = (self.p(at), self.d(axis));
        lathe(self.b, o, a, profile, self.seg, |_| paint);
        self.reset();
    }

    /// An open shell of revolution seen from both sides (a lampshade).
    pub fn shell(&mut self, m: Mat, at: DVec3, profile: &[(f64, f64)]) {
        let paint = self.paint(m);
        let o = self.p(at);
        lathe(self.b, o, DVec3::Y, profile, self.seg, |_| paint);
        crate::carmesh::inside(|| lathe(self.b, o, DVec3::Y, profile, self.seg, |_| paint));
        self.reset();
    }

    /// A round tube through local points.
    pub fn tube(&mut self, m: Mat, pts: &[DVec3], r: f64) {
        let paint = self.paint(m);
        let pts: Vec<DVec3> = pts.iter().map(|&q| self.p(q)).collect();
        tube(self.b, &pts, (r, r), (self.seg / 2).max(6), paint);
        self.reset();
    }

    /// A leg from the floor at (x, z) up `h`, tapering from `r0` at the
    /// foot to `r1`, leaning so its top is at (x + lean.x, h, z + lean.y).
    pub fn leg(&mut self, m: Mat, x: f64, z: f64, h: f64, (r0, r1): (f64, f64), lean: DVec2) {
        let top = DVec3::new(x + lean.x, h, z + lean.y);
        let foot = DVec3::new(x, 0.0, z);
        let axis = top - foot;
        let len = axis.length();
        self.lathe_along(m, foot, axis, &[(r0, 0.0), (r1, len), (0.0, len)]);
    }

    /// A flat slab from `y0` to `y1` with outline `pts` (local x, z; star
    /// shaped around their mean), its sides shaded round.
    pub fn slab(&mut self, m: Mat, pts: &[DVec2], y0: f64, y1: f64) {
        let c = self.mat(m);
        let n = pts.len();
        if n < 3 {
            return;
        }
        let mid = pts.iter().copied().sum::<DVec2>() / n as f64;
        let at = |q: DVec2, y: f64| DVec3::new(q.x, y, q.y);
        for i in 0..n {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            let t = [self.p(at(mid, y1)), self.p(at(a, y1)), self.p(at(b, y1))];
            self.b.tri(t, DVec3::Y, c);
            if y0.abs() > 1e-9 {
                let t = [self.p(at(mid, y0)), self.p(at(b, y0)), self.p(at(a, y0))];
                self.b.tri(t, -DVec3::Y, c);
            }
        }
        // The rim: one smooth band.
        let ring = |y: f64| -> Vec<DVec3> {
            let mut r: Vec<DVec3> = pts.iter().map(|&q| self.p(at(q, y))).collect();
            r.push(r[0]);
            r
        };
        let rows = [ring(y0), ring(y1)];
        let surf = self.b.surf;
        let rows_t: Vec<Vec<DVec3>> = (0..=n).map(|j| vec![rows[0][j], rows[1][j]]).collect();
        surface(self.b, &rows_t, false, |_, _| Some((surf, c)));
        self.reset();
    }

    /// A flat quad facing `n` (local), corners `q` (local).
    pub fn face(&mut self, m: Mat, q: [DVec3; 4], n: DVec3) {
        let c = self.mat(m);
        let q = q.map(|v| self.p(v));
        let n = self.d(n);
        self.b.quad(q, n, c);
        self.reset();
    }
}

/// A rounded rectangle, `w` by `d`, corner radius `r`, centred on 0.
pub fn rrect(w: f64, d: f64, r: f64, per: usize) -> Vec<DVec2> {
    let r = r.min(w * 0.5).min(d * 0.5);
    let (hx, hz) = (w * 0.5 - r, d * 0.5 - r);
    let mut out = Vec::new();
    for (k, (cx, cz)) in [(hx, hz), (-hx, hz), (-hx, -hz), (hx, -hz)]
        .iter()
        .enumerate()
    {
        for i in 0..=per {
            let a = FRAC_PI_2 * (k as f64 + i as f64 / per as f64);
            out.push(DVec2::new(cx + r * a.cos(), cz + r * a.sin()));
        }
    }
    out
}

/// An ellipse, `w` by `d`.
pub fn oval(w: f64, d: f64, n: usize) -> Vec<DVec2> {
    (0..n)
        .map(|i| {
            let a = TAU * i as f64 / n as f64;
            DVec2::new(w * 0.5 * a.cos(), d * 0.5 * a.sin())
        })
        .collect()
}

/// A kidney (boomerang) table top, `w` by `d`, its dent at the back.
pub fn kidney(w: f64, d: f64, n: usize) -> Vec<DVec2> {
    (0..n)
        .map(|i| {
            let a = TAU * i as f64 / n as f64;
            // Dent where the angle points back (-z).
            let da = (a - 1.5 * PI + PI).rem_euclid(TAU) - PI;
            let dent = 1.0 - 0.38 * (-(da * da) / 0.35).exp();
            let r = DVec2::new(w * 0.5 * a.cos(), d * 0.5 * a.sin() * dent);
            // A little asymmetry: one lobe fuller than the other.
            DVec2::new(r.x, r.y * (1.0 + 0.12 * a.cos()))
        })
        .collect()
}

// ------------------------------------------------------------------ seats

/// Upholstery colours.
pub struct Cover {
    pub main: Color,
    /// Piping, pillows.
    pub second: Color,
    pub wood: Color,
}

/// A streamlined sofa, `w` wide (about 2 m), 0.9 m deep.
pub fn sofa(m: &mut Maker, w: f64, cov: &Cover) {
    let d = 0.9;
    let cloth = Mat::Cloth(cov.main);
    let legs = 0.13;
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let (x, z) = (sx * (w * 0.5 - 0.14), sz * (d * 0.5 - 0.14));
        m.leg(
            Mat::Wood(cov.wood),
            x + sx * 0.03,
            z + sz * 0.03,
            legs + 0.04,
            (0.014, 0.024),
            DVec2::new(-sx * 0.03, -sz * 0.03),
        );
    }
    // The frame, the seat cushions on it, the back, the arms.
    m.blob(
        cloth,
        DVec3::new(0.0, legs + 0.11, 0.0),
        DVec3::new(w * 0.5 - 0.02, 0.11, d * 0.5 - 0.02),
        0.0,
        (0.18, 0.3),
    );
    let inner = w - 0.4;
    let n = if inner > 1.5 { 3 } else { 2 };
    let cw = inner / n as f64;
    for k in 0..n {
        let x = -inner * 0.5 + cw * (k as f64 + 0.5);
        m.blob(
            cloth,
            DVec3::new(x, legs + 0.27, 0.06),
            DVec3::new(cw * 0.5 - 0.008, 0.075, 0.33),
            0.0,
            (0.25, 0.35),
        );
    }
    m.blob(
        cloth,
        DVec3::new(0.0, legs + 0.47, -0.33),
        DVec3::new(w * 0.5 - 0.05, 0.27, 0.11),
        0.2,
        (0.22, 0.35),
    );
    for s in [-1.0, 1.0] {
        m.blob(
            cloth,
            DVec3::new(s * (w * 0.5 - 0.1), legs + 0.33, 0.0),
            DVec3::new(0.1, 0.2, d * 0.5 - 0.01),
            0.0,
            (0.45, 0.45),
        );
        // A pillow leaning into each corner.
        m.blob(
            Mat::Cloth(cov.second),
            DVec3::new(s * (w * 0.5 - 0.38), legs + 0.53, -0.16),
            DVec3::new(0.19, 0.17, 0.06),
            0.35,
            (0.5, 0.5),
        );
    }
}

/// An armchair facing +z, about 0.85 m square.
pub fn armchair(m: &mut Maker, cov: &Cover) {
    let w = 0.82;
    let d = 0.84;
    let cloth = Mat::Cloth(cov.main);
    let legs = 0.15;
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        m.leg(
            Mat::Wood(cov.wood),
            sx * (w * 0.5 - 0.09),
            sz * (d * 0.5 - 0.09),
            legs + 0.04,
            (0.013, 0.022),
            DVec2::new(-sx * 0.035, -sz * 0.035),
        );
    }
    m.blob(
        cloth,
        DVec3::new(0.0, legs + 0.1, 0.0),
        DVec3::new(w * 0.5 - 0.02, 0.1, d * 0.5 - 0.02),
        0.0,
        (0.25, 0.3),
    );
    m.blob(
        cloth,
        DVec3::new(0.0, legs + 0.25, 0.06),
        DVec3::new(w * 0.5 - 0.15, 0.07, 0.31),
        0.0,
        (0.3, 0.35),
    );
    // A tall rounded back, a wing each side.
    m.blob(
        cloth,
        DVec3::new(0.0, legs + 0.5, -0.31),
        DVec3::new(w * 0.5 - 0.04, 0.33, 0.1),
        0.22,
        (0.35, 0.4),
    );
    for s in [-1.0, 1.0] {
        m.blob(
            cloth,
            DVec3::new(s * (w * 0.5 - 0.08), legs + 0.3, 0.02),
            DVec3::new(0.08, 0.17, d * 0.5 - 0.03),
            0.0,
            (0.5, 0.5),
        );
    }
    m.blob(
        Mat::Cloth(cov.second),
        DVec3::new(0.0, legs + 0.45, -0.17),
        DVec3::new(0.18, 0.15, 0.06),
        0.3,
        (0.5, 0.5),
    );
}

/// A dining chair facing +z: an upholstered seat, a bent wood back.
pub fn chair(m: &mut Maker, cov: &Cover) {
    let wood = Mat::Wood(cov.wood);
    let h = 0.45;
    for (sx, sz) in [(-1.0, 1.0), (1.0, 1.0)] {
        m.leg(
            wood,
            sx * 0.19,
            sz * 0.18,
            h - 0.03,
            (0.012, 0.018),
            DVec2::new(-sx * 0.02, -sz * 0.02),
        );
    }
    // Back legs run on up into the back's posts.
    for sx in [-1.0, 1.0] {
        m.tube(
            wood,
            &[
                DVec3::new(sx * 0.19, 0.0, -0.22),
                DVec3::new(sx * 0.18, h - 0.02, -0.19),
                DVec3::new(sx * 0.18, 0.88, -0.25),
            ],
            0.016,
        );
    }
    m.blob(
        Mat::Cloth(cov.main),
        DVec3::new(0.0, h, 0.0),
        DVec3::new(0.22, 0.04, 0.22),
        0.0,
        (0.3, 0.4),
    );
    m.blob(
        wood,
        DVec3::new(0.0, 0.76, -0.235),
        DVec3::new(0.21, 0.1, 0.022),
        0.12,
        (0.4, 0.4),
    );
}

/// An office swivel chair facing +z: five feet on casters, a chrome
/// column, a padded seat and back.
pub fn swivel_chair(m: &mut Maker, cov: &Cover) {
    for k in 0..5 {
        let a = TAU * k as f64 / 5.0;
        let out = DVec3::new(a.sin(), 0.0, a.cos());
        m.tube(
            Mat::Chrome,
            &[DVec3::new(0.0, 0.11, 0.0), out * 0.3 + DVec3::Y * 0.07],
            0.018,
        );
        m.lathe(
            Mat::Plain(rgb(30, 30, 32)),
            out * 0.3 + DVec3::Y * 0.004,
            &[(0.025, 0.0), (0.03, 0.02), (0.025, 0.05), (0.0, 0.055)],
        );
    }
    m.lathe(
        Mat::Chrome,
        DVec3::new(0.0, 0.09, 0.0),
        &[(0.05, 0.0), (0.03, 0.04), (0.025, 0.36), (0.0, 0.36)],
    );
    m.blob(
        Mat::Cloth(cov.main),
        DVec3::new(0.0, 0.48, 0.02),
        DVec3::new(0.24, 0.05, 0.23),
        0.0,
        (0.35, 0.4),
    );
    m.tube(
        Mat::Chrome,
        &[
            DVec3::new(0.0, 0.44, -0.12),
            DVec3::new(0.0, 0.46, -0.26),
            DVec3::new(0.0, 0.7, -0.27),
        ],
        0.016,
    );
    m.blob(
        Mat::Cloth(cov.main),
        DVec3::new(0.0, 0.82, -0.25),
        DVec3::new(0.21, 0.16, 0.045),
        0.1,
        (0.35, 0.4),
    );
}

/// A bar stool: a chrome column, a red padded top.
pub fn stool(m: &mut Maker, top: Color) {
    m.lathe(
        Mat::Chrome,
        DVec3::ZERO,
        &[
            (0.2, 0.0),
            (0.19, 0.03),
            (0.05, 0.05),
            (0.035, 0.1),
            (0.035, 0.66),
            (0.0, 0.66),
        ],
    );
    m.tube(
        Mat::Chrome,
        &oval(0.36, 0.36, 16)
            .iter()
            .map(|q| DVec3::new(q.x, 0.3, q.y))
            .chain(std::iter::once(DVec3::new(0.18, 0.3, 0.0)))
            .collect::<Vec<_>>(),
        0.01,
    );
    m.lathe(
        Mat::Plain(top),
        DVec3::new(0.0, 0.64, 0.0),
        &[
            (0.17, 0.0),
            (0.2, 0.03),
            (0.2, 0.07),
            (0.17, 0.1),
            (0.0, 0.11),
        ],
    );
}

// ----------------------------------------------------------------- tables

/// A low table with a top of outline `top` at 0.4 m, on three or four
/// tapered legs.
pub fn coffee_table(m: &mut Maker, top: &[DVec2], wood: Color, legs: &[DVec2]) {
    m.slab(Mat::Wood(wood), top, 0.37, 0.405);
    for &l in legs {
        m.leg(
            Mat::Wood(wood),
            l.x * 1.15,
            l.y * 1.15,
            0.38,
            (0.012, 0.022),
            -l * 0.15,
        );
    }
}

/// A dining table, `w` by `d` (round when they match), at 0.75 m.
pub fn dining_table(m: &mut Maker, w: f64, d: f64, wood: Color) {
    let top = if (w - d).abs() < 1e-6 {
        oval(w, d, m.seg.max(16) * 2)
    } else {
        rrect(w, d, 0.12, (m.seg / 4).max(2))
    };
    m.slab(Mat::Wood(wood), &top, 0.71, 0.75);
    // An apron under the top.
    let apron = rrect(w - 0.16, d - 0.16, 0.06, 2);
    m.slab(Mat::Wood(wood), &apron, 0.63, 0.715);
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let (x, z) = if (w - d).abs() < 1e-6 {
            (sx * w * 0.24, sz * d * 0.24)
        } else {
            (sx * (w * 0.5 - 0.12), sz * (d * 0.5 - 0.12))
        };
        m.leg(
            Mat::Wood(wood),
            x + sx * 0.04,
            z + sz * 0.04,
            0.64,
            (0.016, 0.03),
            DVec2::new(-sx * 0.04, -sz * 0.04),
        );
    }
}

// ---------------------------------------------------------------- storage

/// Door or drawer fronts on a cabinet's front face (at local z `front`):
/// each a panel 15 mm proud with a pull.
fn fronts(
    m: &mut Maker,
    front: f64,
    cells: &[(DVec2, DVec2)],
    panel: Mat,
    pull: Mat,
    vertical: bool,
) {
    for &(lo, hi) in cells {
        m.block(
            panel,
            DVec3::new(lo.x + 0.006, lo.y + 0.006, front - 0.01),
            DVec3::new(hi.x - 0.006, hi.y - 0.006, front + 0.015),
        );
        let c = (lo + hi) * 0.5;
        if vertical {
            let x = if c.x < 0.0 { hi.x - 0.06 } else { lo.x + 0.06 };
            let (y0, y1) = (c.y - 0.08, c.y + 0.08);
            m.tube(
                pull,
                &[
                    DVec3::new(x, y0, front + 0.016),
                    DVec3::new(x, y0, front + 0.04),
                    DVec3::new(x, y1, front + 0.04),
                    DVec3::new(x, y1, front + 0.016),
                ],
                0.007,
            );
        } else {
            m.tube(
                pull,
                &[
                    DVec3::new(c.x - 0.07, c.y, front + 0.016),
                    DVec3::new(c.x - 0.06, c.y, front + 0.035),
                    DVec3::new(c.x + 0.06, c.y, front + 0.035),
                    DVec3::new(c.x + 0.07, c.y, front + 0.016),
                ],
                0.007,
            );
        }
    }
}

/// A low sideboard (credenza) on tapered legs, `w` wide, with sliding
/// doors and a vase or two on top.
pub fn sideboard(m: &mut Maker, w: f64, wood: Color, vase: Color) {
    let (d, y0, y1) = (0.45, 0.18, 0.78);
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        m.leg(
            Mat::Wood(wood),
            sx * (w * 0.5 - 0.06),
            sz * (d * 0.5 - 0.06),
            y0 + 0.03,
            (0.012, 0.022),
            DVec2::new(-sx * 0.04, -sz * 0.03),
        );
    }
    m.block(
        Mat::Wood(wood),
        DVec3::new(-w * 0.5, y0, -d * 0.5),
        DVec3::new(w * 0.5, y1, d * 0.5),
    );
    let n = ((w / 0.55).round() as usize).max(2);
    let cw = (w - 0.06) / n as f64;
    let cells: Vec<(DVec2, DVec2)> = (0..n)
        .map(|k| {
            let x = -w * 0.5 + 0.03 + cw * k as f64;
            (DVec2::new(x, y0 + 0.04), DVec2::new(x + cw, y1 - 0.04))
        })
        .collect();
    fronts(
        m,
        d * 0.5,
        &cells,
        Mat::Wood(scale(wood, 1.12)),
        Mat::Brass,
        true,
    );
    // On top: a tall vase and a squat bowl.
    m.lathe(
        Mat::Paint(vase),
        DVec3::new(-w * 0.3, y1, 0.0),
        &[
            (0.06, 0.0),
            (0.1, 0.08),
            (0.07, 0.22),
            (0.04, 0.32),
            (0.055, 0.38),
            (0.0, 0.37),
        ],
    );
    m.lathe(
        Mat::Paint(rgb(236, 230, 214)),
        DVec3::new(w * 0.25, y1, 0.02),
        &[(0.07, 0.0), (0.13, 0.05), (0.15, 0.09), (0.0, 0.08)],
    );
}

/// A wardrobe, `w` wide and 2 m tall, two doors and a cornice.
pub fn wardrobe(m: &mut Maker, w: f64, wood: Color) {
    let d = 0.6;
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        m.leg(
            Mat::Wood(wood),
            sx * (w * 0.5 - 0.05),
            sz * (d * 0.5 - 0.05),
            0.1,
            (0.02, 0.03),
            DVec2::ZERO,
        );
    }
    m.block(
        Mat::Wood(wood),
        DVec3::new(-w * 0.5, 0.09, -d * 0.5),
        DVec3::new(w * 0.5, 1.98, d * 0.5),
    );
    m.blob(
        Mat::Wood(wood),
        DVec3::new(0.0, 2.01, 0.0),
        DVec3::new(w * 0.5 + 0.03, 0.05, d * 0.5 + 0.03),
        0.0,
        (0.15, 0.4),
    );
    let half = (w - 0.06) * 0.5;
    let cells = [
        (DVec2::new(-half, 0.14), DVec2::new(0.0, 1.93)),
        (DVec2::new(0.0, 0.14), DVec2::new(half, 1.93)),
    ];
    fronts(
        m,
        d * 0.5,
        &cells,
        Mat::Wood(scale(wood, 1.1)),
        Mat::Brass,
        true,
    );
}

/// A bedside table with a drawer, and a lamp on it.
pub fn nightstand(m: &mut Maker, wood: Color, lamp: u8, shade: Color) {
    let (w, d) = (0.46, 0.38);
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        m.leg(
            Mat::Wood(wood),
            sx * (w * 0.5 - 0.05),
            sz * (d * 0.5 - 0.05),
            0.22,
            (0.011, 0.018),
            DVec2::new(-sx * 0.025, -sz * 0.025),
        );
    }
    m.block(
        Mat::Wood(wood),
        DVec3::new(-w * 0.5, 0.2, -d * 0.5),
        DVec3::new(w * 0.5, 0.56, d * 0.5),
    );
    let cells = [(
        DVec2::new(-w * 0.5 + 0.03, 0.4),
        DVec2::new(w * 0.5 - 0.03, 0.53),
    )];
    fronts(
        m,
        d * 0.5,
        &cells,
        Mat::Wood(scale(wood, 1.12)),
        Mat::Brass,
        false,
    );
    let mut l = m.sub(DVec3::new(0.0, 0.56, -0.02), 0.0);
    table_lamp(&mut l, lamp, shade, rgb(110, 170, 160));
}

/// A bookcase, `w` wide and `h` tall, full of books.
pub fn bookcase(m: &mut Maker, w: f64, h: f64, wood: Color, seed: u64) {
    let d = 0.34;
    let t = 0.025;
    let wm = Mat::Wood(wood);
    // Sides, top, back, a plinth.
    for s in [-1.0, 1.0] {
        m.block(
            wm,
            DVec3::new(s * w * 0.5 - t * 0.5, 0.0, -d * 0.5),
            DVec3::new(s * w * 0.5 + t * 0.5, h, d * 0.5),
        );
    }
    m.block(
        wm,
        DVec3::new(-w * 0.5 + t * 0.5, h - t, -d * 0.5),
        DVec3::new(w * 0.5 - t * 0.5, h, d * 0.5),
    );
    m.block(
        Mat::Wood(scale(wood, 0.8)),
        DVec3::new(-w * 0.5 + t * 0.5, 0.08, -d * 0.5),
        DVec3::new(w * 0.5 - t * 0.5, h - t, -d * 0.5 + 0.012),
    );
    m.block(
        wm,
        DVec3::new(-w * 0.5 + t * 0.5, 0.0, -d * 0.5 + 0.012),
        DVec3::new(w * 0.5 - t * 0.5, 0.08, d * 0.5 - 0.02),
    );
    let shelves = ((h - 0.1) / 0.36).floor().max(1.0) as usize;
    let gap = (h - 0.08 - t) / shelves as f64;
    let mut r = Rng(seed);
    for k in 0..shelves {
        let y = 0.08 + gap * k as f64;
        if k > 0 {
            m.block(
                wm,
                DVec3::new(-w * 0.5 + t * 0.5, y - t, -d * 0.5 + 0.012),
                DVec3::new(w * 0.5 - t * 0.5, y, d * 0.5),
            );
        }
        // Books along the shelf, a gap now and then, an ornament.
        let mut x = -w * 0.5 + t + 0.02;
        while x < w * 0.5 - t - 0.06 {
            let roll = r.unit();
            if roll < 0.07 {
                x += 0.1 + r.unit() * 0.12;
                continue;
            }
            let bw = 0.025 + r.unit() * 0.035;
            if x + bw > w * 0.5 - t - 0.01 {
                break;
            }
            let bh = (0.19 + r.unit() * 0.11).min(gap - t - 0.03);
            let bd = 0.15 + r.unit() * 0.08;
            let col = BOOKS[(r.next_u64() % BOOKS.len() as u64) as usize];
            let z0 = -d * 0.5 + 0.035;
            m.block(
                Mat::Plain(rgb(col.0, col.1, col.2)),
                DVec3::new(x, y, z0),
                DVec3::new(x + bw, y + bh, z0 + bd),
            );
            x += bw + 0.002;
        }
    }
}

const BOOKS: [(u8, u8, u8); 10] = [
    (140, 40, 36),
    (40, 70, 110),
    (60, 100, 60),
    (180, 150, 80),
    (90, 50, 80),
    (30, 30, 34),
    (200, 190, 170),
    (170, 90, 40),
    (70, 110, 120),
    (120, 30, 40),
];

/// A filing cabinet: four drawers with brass pulls and label holders.
pub fn filing_cabinet(m: &mut Maker, steel: Color) {
    let (w, d, h) = (0.46, 0.62, 1.32);
    m.blob(
        Mat::Plain(steel),
        DVec3::new(0.0, h * 0.5 + 0.02, 0.0),
        DVec3::new(w * 0.5, h * 0.5, d * 0.5),
        0.0,
        (0.08, 0.1),
    );
    let cells: Vec<(DVec2, DVec2)> = (0..4)
        .map(|k| {
            let y = 0.08 + k as f64 * 0.31;
            (
                DVec2::new(-w * 0.5 + 0.03, y),
                DVec2::new(w * 0.5 - 0.03, y + 0.3),
            )
        })
        .collect();
    fronts(
        m,
        d * 0.5 - 0.008,
        &cells,
        Mat::Plain(scale(steel, 1.08)),
        Mat::Brass,
        false,
    );
    for (_, hi) in &cells {
        m.block(
            Mat::Plain(rgb(240, 236, 220)),
            DVec3::new(-0.05, hi.y - 0.07, d * 0.5 + 0.007),
            DVec3::new(0.05, hi.y - 0.04, d * 0.5 + 0.022),
        );
    }
}

// ------------------------------------------------------------------- beds

/// A bed with its head at the back (-z): `w` wide (1.6 double, 1.0
/// single), 2.1 m long, an upholstered headboard, a bedspread over the
/// lower part.
pub fn bed(m: &mut Maker, w: f64, cov: &Cover) {
    let l = 2.1;
    let wood = Mat::Wood(cov.wood);
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        m.leg(
            wood,
            sx * (w * 0.5 - 0.08),
            sz * (l * 0.5 - 0.1),
            0.16,
            (0.016, 0.026),
            DVec2::new(-sx * 0.02, -sz * 0.02),
        );
    }
    m.block(
        wood,
        DVec3::new(-w * 0.5, 0.14, -l * 0.5 + 0.08),
        DVec3::new(w * 0.5, 0.34, l * 0.5),
    );
    // Mattress, sheet white.
    m.blob(
        Mat::Cloth(rgb(242, 238, 228)),
        DVec3::new(0.0, 0.42, 0.04),
        DVec3::new(w * 0.5 - 0.03, 0.11, l * 0.5 - 0.06),
        0.0,
        (0.2, 0.3),
    );
    // Bedspread: over the lower two thirds, hanging over the sides.
    m.blob(
        Mat::Cloth(cov.main),
        DVec3::new(0.0, 0.45, 0.32),
        DVec3::new(w * 0.5 + 0.015, 0.105, l * 0.5 - 0.26),
        0.0,
        (0.15, 0.25),
    );
    // A folded band across the top of the spread.
    m.blob(
        Mat::Cloth(cov.second),
        DVec3::new(0.0, 0.55, -0.24),
        DVec3::new(w * 0.5 + 0.005, 0.03, 0.09),
        0.0,
        (0.15, 0.6),
    );
    // Pillows.
    let n = if w > 1.3 { 2 } else { 1 };
    for k in 0..n {
        let x = if n == 1 {
            0.0
        } else {
            (k as f64 - 0.5) * (w * 0.5)
        };
        m.blob(
            Mat::Cloth(rgb(250, 248, 242)),
            DVec3::new(x, 0.58, -l * 0.5 + 0.3),
            DVec3::new((w / n as f64) * 0.5 - 0.08, 0.07, 0.2),
            0.25,
            (0.45, 0.5),
        );
    }
    // The headboard: a padded arch on a wooden base.
    m.block(
        wood,
        DVec3::new(-w * 0.5 - 0.04, 0.0, -l * 0.5 - 0.02),
        DVec3::new(w * 0.5 + 0.04, 0.5, -l * 0.5 + 0.08),
    );
    m.blob(
        Mat::Cloth(cov.second),
        DVec3::new(0.0, 0.82, -l * 0.5 + 0.03),
        DVec3::new(w * 0.5 + 0.02, 0.36, 0.06),
        0.0,
        (0.55, 0.3),
    );
}

// ---------------------------------------------------------------- kitchen

/// A run of kitchen base units `w` wide, 0.6 deep: pastel doors with
/// chrome pulls, a white top with a chrome edge, a sink at `sink` (local
/// x) if any.
pub fn counter(m: &mut Maker, w: f64, front: Color, sink: Option<f64>) {
    let d = 0.6;
    // A toe kick, then the cabinet body.
    m.block(
        Mat::Plain(rgb(40, 40, 44)),
        DVec3::new(-w * 0.5 + 0.01, 0.0, -d * 0.5),
        DVec3::new(w * 0.5 - 0.01, 0.1, d * 0.5 - 0.06),
    );
    m.block(
        Mat::Paint(front),
        DVec3::new(-w * 0.5, 0.1, -d * 0.5),
        DVec3::new(w * 0.5, 0.86, d * 0.5 - 0.02),
    );
    // The doors stop short of the cabinet's ends: flush with them they
    // would fight its sides.
    let n = ((w / 0.6).round() as usize).max(1);
    let inner = w - 0.03;
    let cw = inner / n as f64;
    let cells: Vec<(DVec2, DVec2)> = (0..n)
        .flat_map(|k| {
            let x = -inner * 0.5 + cw * k as f64;
            [
                (DVec2::new(x, 0.66), DVec2::new(x + cw, 0.84)),
                (DVec2::new(x, 0.12), DVec2::new(x + cw, 0.64)),
            ]
        })
        .collect();
    fronts(
        m,
        d * 0.5 - 0.02,
        &cells,
        Mat::Paint(scale(front, 1.06)),
        Mat::Chrome,
        false,
    );
    // The worktop and its chrome edge band.
    m.block(
        Mat::Plain(rgb(238, 234, 226)),
        DVec3::new(-w * 0.5, 0.86, -d * 0.5),
        DVec3::new(w * 0.5, 0.9, d * 0.5 + 0.02),
    );
    m.block(
        Mat::Chrome,
        DVec3::new(-w * 0.5 - 0.004, 0.858, d * 0.5 + 0.02),
        DVec3::new(w * 0.5 + 0.004, 0.902, d * 0.5 + 0.034),
    );
    if let Some(x) = sink {
        // A steel basin standing proud of the top (one piece: a rim and a
        // bowl of their own fought), and a swan-neck tap.
        m.blob(
            Mat::Plain(rgb(176, 182, 188)),
            DVec3::new(x, 0.9, 0.0),
            DVec3::new(0.3, 0.025, 0.22),
            0.0,
            (0.25, 0.4),
        );
        m.tube(
            Mat::Chrome,
            &[
                DVec3::new(x, 0.9, -0.24),
                DVec3::new(x, 1.15, -0.22),
                DVec3::new(x, 1.2, -0.12),
                DVec3::new(x, 1.08, -0.05),
            ],
            0.014,
        );
    }
}

/// Wall cupboards over a counter, `w` wide, from 1.45 to 2.15 m.
pub fn wall_cupboards(m: &mut Maker, w: f64, front: Color) {
    let d = 0.34;
    m.block(
        Mat::Paint(front),
        DVec3::new(-w * 0.5, 1.45, -d * 0.5),
        DVec3::new(w * 0.5, 2.15, d * 0.5 - 0.02),
    );
    let n = ((w / 0.6).round() as usize).max(1);
    let inner = w - 0.03;
    let cw = inner / n as f64;
    let cells: Vec<(DVec2, DVec2)> = (0..n)
        .map(|k| {
            let x = -inner * 0.5 + cw * k as f64;
            (DVec2::new(x, 1.47), DVec2::new(x + cw, 2.13))
        })
        .collect();
    fronts(
        m,
        d * 0.5 - 0.02,
        &cells,
        Mat::Paint(scale(front, 1.06)),
        Mat::Chrome,
        true,
    );
}

/// A rounded refrigerator with a chrome handle and badge.
pub fn fridge(m: &mut Maker, body: Color) {
    let (w, d, h) = (0.76, 0.7, 1.62);
    m.blob(
        Mat::Paint(body),
        DVec3::new(0.0, 0.06 + h * 0.5, 0.0),
        DVec3::new(w * 0.5, h * 0.5, d * 0.5),
        0.0,
        (0.2, 0.18),
    );
    for x in [-0.28, 0.28] {
        for z in [-0.25, 0.25] {
            m.leg(
                Mat::Plain(rgb(40, 40, 42)),
                x,
                z,
                0.08,
                (0.03, 0.03),
                DVec2::ZERO,
            );
        }
    }
    // The door's seam: a dark line 15 mm proud would read as a rail, so
    // a slim chrome strip marks the freezer door instead.
    m.block(
        Mat::Chrome,
        DVec3::new(-w * 0.5 + 0.05, 1.22, d * 0.5 - 0.01),
        DVec3::new(w * 0.5 - 0.05, 1.235, d * 0.5 + 0.014),
    );
    // A long lever handle and a badge.
    m.tube(
        Mat::Chrome,
        &[
            DVec3::new(w * 0.5 - 0.1, 0.95, d * 0.5 - 0.01),
            DVec3::new(w * 0.5 - 0.09, 0.98, d * 0.5 + 0.05),
            DVec3::new(w * 0.5 - 0.09, 1.16, d * 0.5 + 0.05),
            DVec3::new(w * 0.5 - 0.1, 1.19, d * 0.5 - 0.01),
        ],
        0.014,
    );
    m.blob(
        Mat::Chrome,
        DVec3::new(0.0, 1.5, d * 0.5),
        DVec3::new(0.13, 0.03, 0.02),
        0.0,
        (0.6, 0.6),
    );
}

/// An enamel range: four burners, an oven door with a window, chrome knobs
/// on a back panel with a clock.
pub fn stove(m: &mut Maker, body: Color) {
    let (w, d) = (0.76, 0.62);
    m.block(
        Mat::Plain(rgb(40, 40, 44)),
        DVec3::new(-w * 0.5 + 0.02, 0.0, -d * 0.5),
        DVec3::new(w * 0.5 - 0.02, 0.1, d * 0.5 - 0.06),
    );
    m.blob(
        Mat::Paint(body),
        DVec3::new(0.0, 0.5, 0.0),
        DVec3::new(w * 0.5, 0.4, d * 0.5),
        0.0,
        (0.15, 0.12),
    );
    for (x, z) in [(-0.18, -0.12), (0.18, -0.12), (-0.18, 0.13), (0.18, 0.13)] {
        m.lathe(
            Mat::Plain(rgb(36, 36, 38)),
            DVec3::new(x, 0.88, z),
            &[(0.1, 0.0), (0.1, 0.02), (0.07, 0.025), (0.0, 0.02)],
        );
    }
    // Oven door: a panel with a dark window.
    m.blob(
        Mat::Paint(scale(body, 1.04)),
        DVec3::new(0.0, 0.42, d * 0.5),
        DVec3::new(w * 0.5 - 0.06, 0.24, 0.025),
        0.0,
        (0.15, 0.15),
    );
    m.blob(
        Mat::Plain(rgb(24, 24, 28)),
        DVec3::new(0.0, 0.46, d * 0.5 + 0.03),
        DVec3::new(0.2, 0.1, 0.012),
        0.0,
        (0.3, 0.3),
    );
    m.tube(
        Mat::Chrome,
        &[
            DVec3::new(-0.25, 0.63, d * 0.5 + 0.06),
            DVec3::new(0.25, 0.63, d * 0.5 + 0.06),
        ],
        0.012,
    );
    // The back panel.
    m.blob(
        Mat::Paint(body),
        DVec3::new(0.0, 1.0, -d * 0.5 + 0.06),
        DVec3::new(w * 0.5, 0.13, 0.05),
        0.0,
        (0.2, 0.3),
    );
    for k in 0..4 {
        let x = -0.27 + k as f64 * 0.18;
        m.lathe_along(
            Mat::Chrome,
            DVec3::new(x, 0.98, -d * 0.5 + 0.1),
            DVec3::Z,
            &[(0.025, 0.0), (0.025, 0.025), (0.0, 0.03)],
        );
    }
    m.lathe_along(
        Mat::Plain(rgb(246, 240, 220)),
        DVec3::new(0.0, 1.05, -d * 0.5 + 0.1),
        DVec3::Z,
        &[(0.06, 0.0), (0.06, 0.012), (0.0, 0.013)],
    );
}

// ----------------------------------------------------------------- lights

/// A table lamp: a ceramic base, a brass neck, a pleated drum shade, the
/// bulb lit with its household (`lamp`, a glow channel).
pub fn table_lamp(m: &mut Maker, lamp: u8, shade: Color, base: Color) {
    m.lathe(
        Mat::Paint(base),
        DVec3::ZERO,
        &[
            (0.06, 0.0),
            (0.09, 0.05),
            (0.1, 0.12),
            (0.07, 0.2),
            (0.03, 0.25),
            (0.0, 0.25),
        ],
    );
    m.lathe(
        Mat::Brass,
        DVec3::new(0.0, 0.24, 0.0),
        &[(0.012, 0.0), (0.012, 0.14), (0.0, 0.14)],
    );
    m.lathe(
        Mat::Lamp(lamp),
        DVec3::new(0.0, 0.33, 0.0),
        &[(0.02, 0.0), (0.04, 0.03), (0.035, 0.07), (0.0, 0.08)],
    );
    m.shell(
        Mat::Cloth(shade),
        DVec3::new(0.0, 0.3, 0.0),
        &[(0.17, 0.0), (0.11, 0.2)],
    );
}

/// A floor lamp: a weighted chrome base, a slim pole and a cone shade.
pub fn floor_lamp(m: &mut Maker, lamp: u8, shade: Color) {
    m.lathe(
        Mat::Chrome,
        DVec3::ZERO,
        &[
            (0.17, 0.0),
            (0.17, 0.015),
            (0.12, 0.04),
            (0.03, 0.05),
            (0.0, 0.05),
        ],
    );
    m.lathe(
        Mat::Brass,
        DVec3::new(0.0, 0.045, 0.0),
        &[(0.014, 0.0), (0.012, 1.32), (0.0, 1.32)],
    );
    m.lathe(
        Mat::Lamp(lamp),
        DVec3::new(0.0, 1.36, 0.0),
        &[(0.025, 0.0), (0.045, 0.04), (0.04, 0.09), (0.0, 0.1)],
    );
    m.shell(
        Mat::Cloth(shade),
        DVec3::new(0.0, 1.3, 0.0),
        &[(0.26, 0.0), (0.12, 0.28)],
    );
}

/// A banker's lamp for a desk: brass, a green glass shade, lit.
pub fn desk_lamp(m: &mut Maker, lamp: u8) {
    m.lathe(
        Mat::Brass,
        DVec3::ZERO,
        &[
            (0.08, 0.0),
            (0.08, 0.015),
            (0.05, 0.03),
            (0.012, 0.04),
            (0.012, 0.3),
            (0.0, 0.3),
        ],
    );
    m.tube(
        Mat::Brass,
        &[DVec3::new(0.0, 0.3, 0.0), DVec3::new(0.0, 0.33, 0.06)],
        0.008,
    );
    m.blob(
        Mat::Lamp(lamp),
        DVec3::new(0.0, 0.31, 0.08),
        DVec3::new(0.11, 0.03, 0.05),
        0.0,
        (1.0, 1.0),
    );
    m.blob(
        Mat::Glass(rgb(40, 140, 80)),
        DVec3::new(0.0, 0.345, 0.08),
        DVec3::new(0.15, 0.05, 0.09),
        0.0,
        (0.6, 0.8),
    );
}

// ------------------------------------------------------------- appliances

/// A radio console on legs: veneer cabinet, cloth grille, a glowing dial
/// and knobs.
pub fn radio(m: &mut Maker, wood: Color) {
    let (w, d) = (0.95, 0.42);
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        m.leg(
            Mat::Wood(wood),
            sx * (w * 0.5 - 0.07),
            sz * (d * 0.5 - 0.06),
            0.2,
            (0.013, 0.022),
            DVec2::new(-sx * 0.04, -sz * 0.03),
        );
    }
    m.blob(
        Mat::Wood(wood),
        DVec3::new(0.0, 0.54, 0.0),
        DVec3::new(w * 0.5, 0.37, d * 0.5),
        0.0,
        (0.12, 0.25),
    );
    // A grille of gold cloth behind slim wooden bars.
    m.blob(
        Mat::Cloth(rgb(196, 168, 110)),
        DVec3::new(-0.16, 0.5, d * 0.5 - 0.01),
        DVec3::new(0.24, 0.24, 0.025),
        0.0,
        (0.3, 0.3),
    );
    for k in 0..5 {
        let x = -0.32 + k as f64 * 0.08;
        m.block(
            Mat::Wood(scale(wood, 0.8)),
            DVec3::new(x - 0.01, 0.28, d * 0.5 + 0.014),
            DVec3::new(x + 0.01, 0.72, d * 0.5 + 0.03),
        );
    }
    // The dial, an arch glowing amber, and two knobs.
    m.blob(
        Mat::Glow(rgb(255, 196, 110)),
        DVec3::new(0.26, 0.62, d * 0.5),
        DVec3::new(0.14, 0.08, 0.022),
        0.0,
        (0.6, 0.6),
    );
    for x in [0.18, 0.34] {
        m.lathe_along(
            Mat::Plain(rgb(60, 40, 30)),
            DVec3::new(x, 0.42, d * 0.5 - 0.01),
            DVec3::Z,
            &[(0.035, 0.0), (0.035, 0.04), (0.025, 0.05), (0.0, 0.05)],
        );
    }
}

/// A television cabinet on splayed legs: a rounded screen, knobs and
/// rabbit-ear antennae. The screen glows blue with the household.
pub fn television(m: &mut Maker, wood: Color, lamp: u8) {
    let (w, d) = (0.72, 0.5);
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        m.leg(
            Mat::Brass,
            sx * (w * 0.5 - 0.05),
            sz * (d * 0.5 - 0.05),
            0.32,
            (0.008, 0.012),
            DVec2::new(-sx * 0.05, -sz * 0.04),
        );
    }
    m.blob(
        Mat::Wood(wood),
        DVec3::new(0.0, 0.66, 0.0),
        DVec3::new(w * 0.5, 0.36, d * 0.5),
        0.0,
        (0.2, 0.2),
    );
    m.blob(
        Mat::Plain(rgb(214, 206, 186)),
        DVec3::new(-0.06, 0.68, d * 0.5),
        DVec3::new(0.25, 0.22, 0.03),
        0.0,
        (0.3, 0.3),
    );
    m.blob(
        Mat::Lamp(lamp),
        DVec3::new(-0.06, 0.68, d * 0.5 + 0.026),
        DVec3::new(0.21, 0.17, 0.02),
        0.0,
        (0.35, 0.35),
    );
    for y in [0.78, 0.6] {
        m.lathe_along(
            Mat::Plain(rgb(60, 50, 40)),
            DVec3::new(0.27, y, d * 0.5 - 0.01),
            DVec3::Z,
            &[(0.03, 0.0), (0.03, 0.04), (0.0, 0.045)],
        );
    }
    m.lathe(
        Mat::Chrome,
        DVec3::new(0.0, 1.01, -0.05),
        &[(0.05, 0.0), (0.05, 0.03), (0.0, 0.04)],
    );
    for s in [-1.0, 1.0] {
        m.tube(
            Mat::Chrome,
            &[
                DVec3::new(0.0, 1.04, -0.05),
                DVec3::new(s * 0.32, 1.52, -0.1),
            ],
            0.005,
        );
    }
}

// ------------------------------------------------------------------ office

/// A steel tanker desk, 1.5 m wide: two drawer pedestals, a lino top with
/// a chrome edge, a typewriter or papers, a telephone and a lamp.
pub fn desk(m: &mut Maker, steel: Color, top: Color, lamp: u8, seed: u64) {
    let (w, d, h) = (1.5, 0.78, 0.74);
    for s in [-1.0, 1.0] {
        m.blob(
            Mat::Plain(steel),
            DVec3::new(s * (w * 0.5 - 0.22), (h - 0.03) * 0.5 + 0.015, 0.0),
            DVec3::new(0.22, (h - 0.03) * 0.5, d * 0.5 - 0.02),
            0.0,
            (0.08, 0.08),
        );
        let x0 = s * (w * 0.5 - 0.22) - 0.19;
        let cells: Vec<(DVec2, DVec2)> = (0..3)
            .map(|k| {
                let y = 0.07 + k as f64 * 0.21;
                (DVec2::new(x0, y), DVec2::new(x0 + 0.38, y + 0.2))
            })
            .collect();
        fronts(
            m,
            d * 0.5 - 0.022,
            &cells,
            Mat::Plain(scale(steel, 1.07)),
            Mat::Chrome,
            false,
        );
    }
    // A modesty panel at the back.
    m.block(
        Mat::Plain(steel),
        DVec3::new(-w * 0.5 + 0.44, 0.2, -d * 0.5 + 0.04),
        DVec3::new(w * 0.5 - 0.44, h - 0.03, -d * 0.5 + 0.06),
    );
    m.block(
        Mat::Plain(top),
        DVec3::new(-w * 0.5, h - 0.03, -d * 0.5),
        DVec3::new(w * 0.5, h, d * 0.5),
    );
    m.block(
        Mat::Chrome,
        DVec3::new(-w * 0.5 - 0.004, h - 0.035, -d * 0.5 - 0.012),
        DVec3::new(w * 0.5 + 0.004, h - 0.005, -d * 0.5),
    );
    m.block(
        Mat::Chrome,
        DVec3::new(-w * 0.5 - 0.004, h - 0.035, d * 0.5),
        DVec3::new(w * 0.5 + 0.004, h - 0.005, d * 0.5 + 0.012),
    );
    let mut on = m.sub(DVec3::new(0.0, h, 0.0), 0.0);
    if !seed.is_multiple_of(3) {
        typewriter(&mut on, DVec3::new(-0.05, 0.0, 0.08));
    } else {
        // A blotter and papers, each at least 1 cm proud of what it lies
        // on (thinner, their tops would fight).
        on.block(
            Mat::Plain(rgb(60, 90, 70)),
            DVec3::new(-0.3, 0.0, -0.05),
            DVec3::new(0.25, 0.012, 0.3),
        );
        on.block(
            Mat::Plain(rgb(246, 244, 236)),
            DVec3::new(-0.2, 0.012, 0.0),
            DVec3::new(0.02, 0.026, 0.28),
        );
    }
    telephone(&mut on, DVec3::new(0.5, 0.0, 0.05));
    let mut l = on.sub(DVec3::new(-0.55, 0.0, -0.18), 0.0);
    desk_lamp(&mut l, lamp);
    // An in-tray with papers.
    on.block(
        Mat::Wood(rgb(150, 100, 60)),
        DVec3::new(0.38, 0.0, -0.33),
        DVec3::new(0.68, 0.05, -0.1),
    );
    on.block(
        Mat::Plain(rgb(246, 244, 236)),
        DVec3::new(0.4, 0.05, -0.31),
        DVec3::new(0.66, 0.065, -0.12),
    );
}

/// A typewriter: a rounded body, rows of keys, the carriage and platen.
pub fn typewriter(m: &mut Maker, at: DVec3) {
    let body = Mat::Plain(rgb(48, 62, 58));
    m.blob(
        body,
        at + DVec3::new(0.0, 0.06, 0.0),
        DVec3::new(0.2, 0.06, 0.17),
        0.0,
        (0.3, 0.3),
    );
    for r in 0..3 {
        let z = at.z + 0.1 - r as f64 * 0.045;
        let y = 0.11 + r as f64 * 0.012;
        m.block(
            Mat::Plain(rgb(230, 226, 214)),
            DVec3::new(at.x - 0.15, y, z - 0.015),
            DVec3::new(at.x + 0.15, y + 0.02, z + 0.015),
        );
    }
    m.lathe_along(
        Mat::Plain(rgb(26, 26, 28)),
        at + DVec3::new(-0.25, 0.15, -0.1),
        DVec3::X,
        &[(0.0, 0.0), (0.035, 0.005), (0.035, 0.495), (0.0, 0.5)],
    );
    m.block(
        Mat::Plain(rgb(250, 250, 244)),
        at + DVec3::new(-0.11, 0.18, -0.12),
        at + DVec3::new(0.11, 0.31, -0.115),
    );
}

/// A rotary telephone in black bakelite.
pub fn telephone(m: &mut Maker, at: DVec3) {
    let black = Mat::Plain(rgb(22, 22, 24));
    m.blob(
        black,
        at + DVec3::new(0.0, 0.05, 0.0),
        DVec3::new(0.1, 0.05, 0.12),
        0.25,
        (0.5, 0.4),
    );
    m.lathe_along(
        Mat::Plain(rgb(236, 232, 220)),
        at + DVec3::new(0.0, 0.07, 0.06),
        DVec3::new(0.0, 0.6, 0.8),
        &[(0.045, 0.0), (0.045, 0.012), (0.0, 0.014)],
    );
    m.tube(
        black,
        &[
            at + DVec3::new(-0.11, 0.11, -0.04),
            at + DVec3::new(-0.08, 0.13, -0.04),
            at + DVec3::new(0.08, 0.13, -0.04),
            at + DVec3::new(0.11, 0.11, -0.04),
        ],
        0.022,
    );
}

/// A water cooler: a white cabinet and a blue glass bottle upside down.
pub fn water_cooler(m: &mut Maker) {
    m.blob(
        Mat::Paint(rgb(236, 234, 228)),
        DVec3::new(0.0, 0.52, 0.0),
        DVec3::new(0.18, 0.5, 0.18),
        0.0,
        (0.3, 0.12),
    );
    m.blob(
        Mat::Chrome,
        DVec3::new(0.0, 0.8, 0.19),
        DVec3::new(0.05, 0.03, 0.03),
        0.0,
        (0.6, 0.6),
    );
    m.lathe(
        Mat::Glass(rgb(120, 180, 230)),
        DVec3::new(0.0, 1.0, 0.0),
        &[
            (0.03, 0.0),
            (0.04, 0.06),
            (0.15, 0.12),
            (0.15, 0.42),
            (0.1, 0.48),
            (0.0, 0.5),
        ],
    );
}

/// A coat stand with a hat and a coat on it.
pub fn coat_stand(m: &mut Maker, coat: Color) {
    let wood = Mat::Wood(rgb(110, 70, 40));
    for k in 0..3 {
        let a = TAU * k as f64 / 3.0;
        let out = DVec3::new(a.sin(), 0.0, a.cos());
        m.tube(
            wood,
            &[DVec3::new(0.0, 0.2, 0.0), out * 0.26 + DVec3::Y * 0.015],
            0.015,
        );
    }
    m.lathe(
        wood,
        DVec3::new(0.0, 0.15, 0.0),
        &[(0.03, 0.0), (0.022, 1.6), (0.035, 1.66), (0.0, 1.7)],
    );
    for k in 0..4 {
        let a = TAU * k as f64 / 4.0 + 0.4;
        let out = DVec3::new(a.sin(), 0.0, a.cos());
        m.tube(
            Mat::Brass,
            &[DVec3::new(0.0, 1.6, 0.0), out * 0.12 + DVec3::Y * 1.68],
            0.008,
        );
    }
    // A trench coat hanging from one hook, a fedora on another.
    m.blob(
        Mat::Cloth(coat),
        DVec3::new(0.0, 1.22, 0.12),
        DVec3::new(0.2, 0.42, 0.07),
        -0.12,
        (0.6, 0.5),
    );
    m.lathe(
        Mat::Cloth(rgb(70, 60, 50)),
        DVec3::new(-0.11, 1.68, -0.06),
        &[
            (0.0, 0.0),
            (0.16, 0.0),
            (0.16, 0.01),
            (0.09, 0.02),
            (0.085, 0.1),
            (0.0, 0.12),
        ],
    );
}

// ------------------------------------------------------------------ plants

/// A plant in a pot: big leaves on curved stems (a rubber plant or a
/// philodendron), `h` tall.
pub fn plant(m: &mut Maker, h: f64, pot: Color, seed: u64) {
    let r = 0.17 + h * 0.05;
    m.lathe(
        Mat::Paint(pot),
        DVec3::ZERO,
        &[
            (r * 0.75, 0.0),
            (r, r * 1.6),
            (r * 1.06, r * 1.7),
            (r * 0.98, r * 1.75),
            (0.0, r * 1.7),
        ],
    );
    let top = r * 1.7;
    let leaf = rgb(52, 112, 58);
    let mut g = Rng(seed);
    let n = 7 + (seed % 4) as usize;
    for k in 0..n {
        let a = TAU * (k as f64 + g.unit() * 0.6) / n as f64;
        let out = DVec3::new(a.sin(), 0.0, a.cos());
        let rise = top + h * (0.35 + 0.6 * (k as f64 / n as f64));
        let reach = 0.18 + g.unit() * 0.22;
        let tip = out * reach + DVec3::Y * rise;
        m.tube(
            Mat::Plain(rgb(70, 100, 50)),
            &[
                DVec3::new(0.0, top - 0.02, 0.0),
                out * reach * 0.3 + DVec3::Y * (top + (rise - top) * 0.7),
                tip,
            ],
            0.008,
        );
        // The leaf: a long flat oval, drooping outwards.
        let along = (out + DVec3::Y * (0.2 - g.unit() * 0.5)).normalize();
        let side = DVec3::Y.cross(along).normalize_or(DVec3::X);
        let normal = along.cross(side).normalize();
        let size = 0.11 + g.unit() * 0.06;
        m.blob_axes(
            Mat::Plain(scale(leaf, 0.85 + g.unit() as f32 * 0.3)),
            tip + along * size,
            [side, normal, along],
            DVec3::new(size * 0.55, 0.008, size),
            (1.0, 1.0),
        );
    }
}

// -------------------------------------------------------------------- shop

/// A shop counter, `w` wide: a curved streamline front in two colours with
/// chrome bands, and a cash register on it.
pub fn shop_counter(m: &mut Maker, w: f64, body: Color, band: Color) {
    let d = 0.65;
    let shape = rrect(w, d, 0.3, (m.seg / 4).max(3));
    m.slab(
        Mat::Plain(rgb(40, 40, 44)),
        &rrect(w - 0.1, d - 0.1, 0.25, 4),
        0.0,
        0.1,
    );
    m.slab(Mat::Paint(body), &shape, 0.1, 0.92);
    for y in [0.32, 0.6] {
        let grown: Vec<DVec2> = rrect(w + 0.03, d + 0.03, 0.315, (m.seg / 4).max(3));
        m.slab(Mat::Paint(band), &grown, y, y + 0.08);
    }
    let lip: Vec<DVec2> = rrect(w + 0.06, d + 0.06, 0.33, (m.seg / 4).max(3));
    m.slab(Mat::Chrome, &lip, 0.92, 0.96);
    let mut on = m.sub(DVec3::new(w * 0.25, 0.96, -0.05), 0.0);
    cash_register(&mut on);
}

/// A chrome cash register with its keys and the sale sign up top.
pub fn cash_register(m: &mut Maker) {
    m.blob(
        Mat::Chrome,
        DVec3::new(0.0, 0.12, 0.0),
        DVec3::new(0.2, 0.12, 0.2),
        0.0,
        (0.25, 0.25),
    );
    m.blob(
        Mat::Chrome,
        DVec3::new(0.0, 0.25, -0.08),
        DVec3::new(0.18, 0.12, 0.12),
        0.35,
        (0.25, 0.25),
    );
    for r in 0..3 {
        for c in 0..4 {
            m.lathe(
                Mat::Plain(rgb(240, 236, 224)),
                DVec3::new(
                    -0.12 + c as f64 * 0.08,
                    0.24 + r as f64 * 0.045,
                    0.08 - r as f64 * 0.05,
                ),
                &[(0.016, 0.0), (0.016, 0.02), (0.0, 0.022)],
            );
        }
    }
    m.blob(
        Mat::Glow(rgb(255, 240, 200)),
        DVec3::new(0.0, 0.42, -0.1),
        DVec3::new(0.13, 0.05, 0.022),
        0.0,
        (0.3, 0.3),
    );
}

/// A gondola of open shelves, `w` wide, stocked from both sides with tins,
/// boxes and bottles in bright labels.
pub fn gondola(m: &mut Maker, w: f64, frame: Color, seed: u64, two_sided: bool) {
    let d = if two_sided { 0.9 } else { 0.5 };
    let h = 1.55;
    m.block(
        Mat::Paint(frame),
        DVec3::new(-w * 0.5, 0.0, -0.03),
        DVec3::new(w * 0.5, h, 0.03),
    );
    let faces: &[f64] = if two_sided { &[-1.0, 1.0] } else { &[1.0] };
    let mut g = Rng(seed);
    for &f in faces {
        m.block(
            Mat::Paint(scale(frame, 0.8)),
            DVec3::new(-w * 0.5, 0.0, if f > 0.0 { 0.03 } else { -d * 0.5 }),
            DVec3::new(w * 0.5, 0.12, if f > 0.0 { d * 0.5 } else { -0.03 }),
        );
        for k in 0..4 {
            let y = 0.12 + k as f64 * 0.36;
            if k > 0 {
                let (z0, z1) = if f > 0.0 {
                    (0.03, d * 0.5)
                } else {
                    (-d * 0.5, -0.03)
                };
                m.block(
                    Mat::Chrome,
                    DVec3::new(-w * 0.5 + 0.01, y - 0.02, z0),
                    DVec3::new(w * 0.5 - 0.01, y, z1),
                );
            }
            // Goods along the shelf: rows of the same thing.
            let mut x = -w * 0.5 + 0.05;
            while x < w * 0.5 - 0.15 {
                let kind = g.next_u64() % 3;
                let n = 2 + (g.next_u64() % 4) as usize;
                let col = LABELS[(g.next_u64() % LABELS.len() as u64) as usize];
                let col = rgb(col.0, col.1, col.2);
                let z = f * (0.08 + d * 0.15);
                for _ in 0..n {
                    let step = match kind {
                        0 => {
                            // A tin.
                            m.lathe(
                                Mat::Plain(col),
                                DVec3::new(x + 0.04, y, z),
                                &[(0.04, 0.0), (0.04, 0.11), (0.035, 0.115), (0.0, 0.115)],
                            );
                            0.09
                        }
                        1 => {
                            // A carton.
                            m.block(
                                Mat::Plain(col),
                                DVec3::new(x, y, z - 0.06),
                                DVec3::new(x + 0.07, y + 0.22, z + 0.06),
                            );
                            0.08
                        }
                        _ => {
                            // A bottle.
                            m.lathe(
                                Mat::Glass(col),
                                DVec3::new(x + 0.035, y, z),
                                &[
                                    (0.035, 0.0),
                                    (0.035, 0.16),
                                    (0.012, 0.22),
                                    (0.012, 0.27),
                                    (0.0, 0.27),
                                ],
                            );
                            0.08
                        }
                    };
                    x += step;
                    if x > w * 0.5 - 0.12 {
                        break;
                    }
                }
                x += 0.03;
            }
        }
    }
}

const LABELS: [(u8, u8, u8); 9] = [
    (214, 60, 50),
    (240, 190, 60),
    (60, 120, 190),
    (240, 236, 220),
    (80, 160, 90),
    (230, 120, 50),
    (150, 70, 140),
    (40, 40, 44),
    (110, 200, 210),
];

/// A clothes rail: chrome stand, garments on hangers.
pub fn clothes_rail(m: &mut Maker, w: f64, seed: u64) {
    for s in [-1.0, 1.0] {
        m.tube(
            Mat::Chrome,
            &[
                DVec3::new(s * w * 0.5, 0.0, -0.25),
                DVec3::new(s * w * 0.5, 0.02, 0.0),
                DVec3::new(s * w * 0.5, 1.5, 0.0),
            ],
            0.016,
        );
        m.tube(
            Mat::Chrome,
            &[
                DVec3::new(s * w * 0.5, 0.02, 0.25),
                DVec3::new(s * w * 0.5, 0.02, 0.0),
            ],
            0.016,
        );
    }
    m.tube(
        Mat::Chrome,
        &[
            DVec3::new(-w * 0.5, 1.5, 0.0),
            DVec3::new(w * 0.5, 1.5, 0.0),
        ],
        0.012,
    );
    let mut g = Rng(seed);
    let n = ((w - 0.2) / 0.11) as usize;
    let col = CLOTHES[(g.next_u64() % CLOTHES.len() as u64) as usize];
    for k in 0..n {
        let x = -w * 0.5 + 0.15 + k as f64 * 0.11;
        let c = if g.unit() < 0.35 {
            CLOTHES[(g.next_u64() % CLOTHES.len() as u64) as usize]
        } else {
            col
        };
        let long = g.unit() < 0.4;
        let len = if long { 0.5 } else { 0.32 };
        m.blob(
            Mat::Cloth(rgb(c.0, c.1, c.2)),
            DVec3::new(x, 1.43 - len, 0.0),
            DVec3::new(0.035, len, 0.24),
            0.0,
            (0.5, 0.35),
        );
        m.tube(
            Mat::Chrome,
            &[
                DVec3::new(x, 1.44, -0.2),
                DVec3::new(x, 1.5, 0.0),
                DVec3::new(x, 1.44, 0.2),
            ],
            0.004,
        );
    }
}

const CLOTHES: [(u8, u8, u8); 8] = [
    (200, 60, 60),
    (60, 90, 150),
    (230, 210, 170),
    (90, 140, 120),
    (220, 150, 170),
    (60, 60, 64),
    (240, 236, 228),
    (190, 140, 70),
];

/// A dress form (mannequin) on a chrome stand, wearing a dress.
pub fn dress_form(m: &mut Maker, dress: Color) {
    m.lathe(
        Mat::Chrome,
        DVec3::ZERO,
        &[
            (0.2, 0.0),
            (0.2, 0.02),
            (0.03, 0.04),
            (0.018, 0.06),
            (0.018, 0.75),
            (0.0, 0.75),
        ],
    );
    m.lathe(
        Mat::Cloth(dress),
        DVec3::new(0.0, 0.62, 0.0),
        &[
            (0.0, 0.0),
            (0.3, 0.02),
            (0.24, 0.3),
            (0.15, 0.55),
            (0.13, 0.65),
            (0.17, 0.82),
            (0.16, 0.92),
            (0.08, 0.98),
            (0.05, 1.0),
            (0.0, 1.0),
        ],
    );
    m.lathe(
        Mat::Plain(rgb(230, 214, 190)),
        DVec3::new(0.0, 1.6, 0.0),
        &[(0.045, 0.0), (0.04, 0.08), (0.0, 0.12)],
    );
}

/// A glass display case on a wooden base, things to look at inside.
pub fn display_case(m: &mut Maker, w: f64, wood: Color, seed: u64) {
    let d = 0.55;
    m.block(
        Mat::Wood(wood),
        DVec3::new(-w * 0.5, 0.0, -d * 0.5),
        DVec3::new(w * 0.5, 0.75, d * 0.5),
    );
    // Inside: a few things on a velvet floor.
    m.block(
        Mat::Cloth(rgb(110, 30, 40)),
        DVec3::new(-w * 0.5 + 0.03, 0.75, -d * 0.5 + 0.03),
        DVec3::new(w * 0.5 - 0.03, 0.765, d * 0.5 - 0.03),
    );
    let mut g = Rng(seed);
    let n = ((w - 0.2) / 0.3) as usize;
    for k in 0..n {
        let x = -w * 0.5 + 0.2 + k as f64 * 0.3;
        let c = LABELS[(g.next_u64() % LABELS.len() as u64) as usize];
        if g.unit() < 0.5 {
            m.lathe(
                Mat::Brass,
                DVec3::new(x, 0.765, 0.0),
                &[(0.04, 0.0), (0.03, 0.04), (0.05, 0.1), (0.0, 0.12)],
            );
        } else {
            m.blob(
                Mat::Plain(rgb(c.0, c.1, c.2)),
                DVec3::new(x, 0.81, 0.0),
                DVec3::new(0.08, 0.045, 0.06),
                0.0,
                (0.4, 0.4),
            );
        }
    }
    // The glass box and its chrome edges.
    let (y0, y1) = (0.766, 1.08);
    let (x0, x1, z0, z1) = (
        -w * 0.5 + 0.02,
        w * 0.5 - 0.02,
        -d * 0.5 + 0.02,
        d * 0.5 - 0.02,
    );
    let g = Mat::Glass(rgb(220, 236, 240));
    m.face(
        g,
        [
            DVec3::new(x0, y1, z0),
            DVec3::new(x1, y1, z0),
            DVec3::new(x1, y1, z1),
            DVec3::new(x0, y1, z1),
        ],
        DVec3::Y,
    );
    m.face(
        g,
        [
            DVec3::new(x0, y0, z1),
            DVec3::new(x1, y0, z1),
            DVec3::new(x1, y1, z1),
            DVec3::new(x0, y1, z1),
        ],
        DVec3::Z,
    );
    m.face(
        g,
        [
            DVec3::new(x0, y0, z0),
            DVec3::new(x1, y0, z0),
            DVec3::new(x1, y1, z0),
            DVec3::new(x0, y1, z0),
        ],
        -DVec3::Z,
    );
    m.face(
        g,
        [
            DVec3::new(x0, y0, z0),
            DVec3::new(x0, y0, z1),
            DVec3::new(x0, y1, z1),
            DVec3::new(x0, y1, z0),
        ],
        -DVec3::X,
    );
    m.face(
        g,
        [
            DVec3::new(x1, y0, z0),
            DVec3::new(x1, y0, z1),
            DVec3::new(x1, y1, z1),
            DVec3::new(x1, y1, z0),
        ],
        DVec3::X,
    );
    for (a, b) in [
        (DVec3::new(x0, y1, z1), DVec3::new(x1, y1, z1)),
        (DVec3::new(x0, y1, z0), DVec3::new(x1, y1, z0)),
        (DVec3::new(x0, y1, z0), DVec3::new(x0, y1, z1)),
        (DVec3::new(x1, y1, z0), DVec3::new(x1, y1, z1)),
        (DVec3::new(x0, y0 + 0.01, z1), DVec3::new(x0, y1, z1)),
        (DVec3::new(x1, y0 + 0.01, z1), DVec3::new(x1, y1, z1)),
    ] {
        m.tube(Mat::Chrome, &[a, b], 0.008);
    }
}

/// A jukebox: an arched cabinet with glowing tubes and a chrome grille.
pub fn jukebox(m: &mut Maker) {
    let wood = Mat::Wood(rgb(130, 70, 40));
    m.block(
        wood,
        DVec3::new(-0.42, 0.0, -0.3),
        DVec3::new(0.42, 1.0, 0.3),
    );
    m.lathe_along(
        wood,
        DVec3::new(0.0, 1.0, -0.3),
        DVec3::Z,
        &[(0.0, 0.0), (0.42, 0.001), (0.42, 0.6), (0.0, 0.601)],
    );
    // Glowing tubes up the sides and over the arch.
    let arch: Vec<DVec3> = (0..=16)
        .map(|k| {
            let a = PI * k as f64 / 16.0;
            DVec3::new(0.38 * a.cos(), 1.0 + 0.38 * a.sin(), 0.31)
        })
        .collect();
    let mut path = vec![DVec3::new(0.38, 0.1, 0.31)];
    path.extend(arch);
    path.push(DVec3::new(-0.38, 0.1, 0.31));
    m.tube(Mat::Glow(rgb(255, 120, 60)), &path, 0.03);
    m.blob(
        Mat::Glass(rgb(255, 230, 180)),
        DVec3::new(0.0, 1.08, 0.3),
        DVec3::new(0.26, 0.2, 0.03),
        0.0,
        (0.5, 0.5),
    );
    m.blob(
        Mat::Chrome,
        DVec3::new(0.0, 0.45, 0.3),
        DVec3::new(0.28, 0.22, 0.03),
        0.0,
        (0.25, 0.25),
    );
    // The speaker grille, proud of the chrome.
    m.blob(
        Mat::Plain(rgb(40, 30, 30)),
        DVec3::new(0.0, 0.45, 0.335),
        DVec3::new(0.2, 0.15, 0.013),
        0.0,
        (0.3, 0.3),
    );
}

/// A night club's back bar, `w` long, against the wall: a walnut
/// cabinet, a mirror above with two glass shelves of bottles in front.
pub fn back_bar(m: &mut Maker, w: f64, wood: Color) {
    let hw = w * 0.5;
    m.block(
        Mat::Wood(wood),
        DVec3::new(-hw, 0.0, -0.2),
        DVec3::new(hw, 0.9, 0.2),
    );
    m.block(
        Mat::Chrome,
        DVec3::new(-hw, 0.9, -0.2),
        DVec3::new(hw, 0.92, 0.21),
    );
    m.block(
        Mat::Glass(rgb(200, 210, 220)),
        DVec3::new(-hw + 0.05, 1.0, -0.2),
        DVec3::new(hw - 0.05, 1.9, -0.18),
    );
    m.block(
        Mat::Wood(wood),
        DVec3::new(-hw, 1.9, -0.2),
        DVec3::new(hw, 2.0, 0.0),
    );
    let tints = [
        rgb(120, 70, 20),
        rgb(60, 120, 60),
        rgb(200, 190, 150),
        rgb(150, 40, 40),
        rgb(90, 60, 30),
    ];
    for (row, y) in [1.2, 1.55].into_iter().enumerate() {
        m.block(
            Mat::Glass(rgb(220, 230, 235)),
            DVec3::new(-hw + 0.05, y - 0.02, -0.18),
            DVec3::new(hw - 0.05, y, -0.04),
        );
        let n = ((w - 0.2) / 0.12) as usize;
        for i in 0..n {
            let x = -hw + 0.16 + i as f64 * 0.12;
            let k = (i * 7 + row * 3) % tints.len();
            let tall = 0.22 + 0.06 * ((i + row) % 3) as f64;
            m.lathe(
                Mat::Glass(tints[k]),
                DVec3::new(x, y, -0.11),
                &[
                    (0.03, 0.0),
                    (0.035, 0.01),
                    (0.035, tall * 0.65),
                    (0.012, tall * 0.8),
                    (0.012, tall),
                    (0.0, tall),
                ],
            );
        }
    }
}

/// A small bandstand for a corner: a round red stage a step high with a
/// chrome rail behind, a microphone on its stand, a drum and a double
/// bass leaning on the rail.
pub fn bandstand(m: &mut Maker) {
    m.slab(Mat::Paint(rgb(150, 30, 40)), &oval(1.9, 1.5, 24), 0.0, 0.22);
    // The chrome band stops short of the stage top, so the two tops never
    // lie in one plane.
    m.slab(Mat::Chrome, &oval(1.94, 1.54, 24), 0.15, 0.2);
    let rail: Vec<DVec3> = (0..=12)
        .map(|k| {
            let a = PI * k as f64 / 12.0;
            DVec3::new(0.9 * a.cos(), 1.0, -0.7 * a.sin())
        })
        .collect();
    m.tube(Mat::Chrome, &rail, 0.02);
    for p in [rail[0], rail[6], rail[12]] {
        m.tube(Mat::Chrome, &[DVec3::new(p.x, 0.22, p.z), p], 0.015);
    }
    // The microphone at the front.
    m.lathe(
        Mat::Chrome,
        DVec3::new(0.0, 0.22, 0.4),
        &[
            (0.12, 0.0),
            (0.1, 0.02),
            (0.012, 0.03),
            (0.012, 1.45),
            (0.0, 1.45),
        ],
    );
    m.blob(
        Mat::Chrome,
        DVec3::new(0.0, 1.72, 0.4),
        DVec3::new(0.05, 0.07, 0.04),
        0.0,
        (0.6, 0.6),
    );
    // A drum, its skin up.
    m.lathe(
        Mat::Paint(rgb(230, 230, 236)),
        DVec3::new(-0.45, 0.22, -0.2),
        &[(0.0, 0.0), (0.25, 0.0), (0.25, 0.35), (0.0, 0.36)],
    );
    m.lathe(
        Mat::Chrome,
        DVec3::new(-0.45, 0.4, -0.2),
        &[(0.255, 0.0), (0.255, 0.03), (0.0, 0.031)],
    );
    // A double bass leaning on the rail.
    m.blob_axes(
        Mat::Wood(rgb(140, 70, 30)),
        DVec3::new(0.5, 0.75, -0.35),
        [
            DVec3::X,
            DVec3::new(0.0, 0.97, -0.24).normalize(),
            DVec3::new(0.0, 0.24, 0.97).normalize(),
        ],
        DVec3::new(0.3, 0.5, 0.12),
        (0.7, 0.9),
    );
    m.tube(
        Mat::Plain(rgb(30, 24, 20)),
        &[DVec3::new(0.5, 1.2, -0.46), DVec3::new(0.5, 1.75, -0.6)],
        0.03,
    );
}

/// A laboratory bench, `w` long: a white enamelled cabinet with a black
/// slate top, a chrome rail of taps along the back, and flasks glowing
/// with Aether in green and blue.
pub fn lab_bench(m: &mut Maker, w: f64) {
    let hw = w * 0.5;
    m.block(
        Mat::Paint(rgb(236, 236, 236)),
        DVec3::new(-hw, 0.1, -0.33),
        DVec3::new(hw, 0.88, 0.33),
    );
    m.block(
        Mat::Plain(rgb(40, 40, 44)),
        DVec3::new(-hw + 0.04, 0.0, -0.29),
        DVec3::new(hw - 0.04, 0.1, 0.29),
    );
    m.block(
        Mat::Plain(rgb(30, 30, 34)),
        DVec3::new(-hw - 0.01, 0.88, -0.35),
        DVec3::new(hw + 0.01, 0.92, 0.35),
    );
    // Door seams and chrome pulls.
    let doors = (w / 0.6).round().max(1.0) as usize;
    for k in 0..doors {
        let x = -hw + (k as f64 + 0.5) * w / doors as f64;
        m.tube(
            Mat::Chrome,
            &[
                DVec3::new(x - 0.08, 0.7, 0.345),
                DVec3::new(x + 0.08, 0.7, 0.345),
            ],
            0.012,
        );
        if k > 0 {
            let xs = -hw + k as f64 * w / doors as f64;
            m.block(
                Mat::Plain(rgb(150, 150, 150)),
                DVec3::new(xs - 0.004, 0.14, 0.33),
                DVec3::new(xs + 0.004, 0.84, 0.342),
            );
        }
    }
    // The rail of taps at the back.
    m.tube(
        Mat::Chrome,
        &[
            DVec3::new(-hw + 0.1, 1.22, -0.27),
            DVec3::new(hw - 0.1, 1.22, -0.27),
        ],
        0.016,
    );
    for x in [-hw + 0.1, hw - 0.1] {
        m.tube(
            Mat::Chrome,
            &[DVec3::new(x, 0.92, -0.27), DVec3::new(x, 1.22, -0.27)],
            0.016,
        );
    }
    // Flasks: round-bottomed, a neck, the Aether glowing inside.
    let n = ((w * 2.0) as usize).clamp(2, 10);
    for k in 0..n {
        let x = -hw + 0.3 + k as f64 * (w - 0.6) / (n - 1) as f64;
        let z = if k % 3 == 1 { 0.12 } else { -0.05 };
        let col = if k % 2 == 0 {
            rgb(120, 255, 140)
        } else {
            rgb(120, 230, 255)
        };
        let big = 1.0 + (k % 3) as f64 * 0.25;
        m.lathe(
            Mat::Glow(col),
            DVec3::new(x, 0.925, z),
            &[
                (0.02 * big, 0.0),
                (0.07 * big, 0.04 * big),
                (0.075 * big, 0.08 * big),
                (0.05 * big, 0.13 * big),
                (0.0, 0.14 * big),
            ],
        );
        m.lathe(
            Mat::Glass(rgb(210, 230, 230)),
            DVec3::new(x, 0.92, z),
            &[
                (0.03 * big, 0.0),
                (0.085 * big, 0.05 * big),
                (0.085 * big, 0.1 * big),
                (0.028 * big, 0.2 * big),
                (0.028 * big, 0.3 * big),
                (0.034 * big, 0.31 * big),
            ],
        );
    }
}

/// An Aether coil: a chrome foot, a tall glass bell with the coil's
/// glowing rings round a brass core inside, a chrome cap with a ball on
/// top, and a dial on the foot.
pub fn aether_coil(m: &mut Maker) {
    m.lathe(
        Mat::Chrome,
        DVec3::ZERO,
        &[
            (0.56, 0.0),
            (0.56, 0.1),
            (0.5, 0.16),
            (0.46, 0.34),
            (0.4, 0.38),
            (0.0, 0.38),
        ],
    );
    m.lathe(
        Mat::Brass,
        DVec3::new(0.0, 0.38, 0.0),
        &[(0.09, 0.0), (0.09, 1.3), (0.0, 1.3)],
    );
    for k in 0..7 {
        let y = 0.5 + k as f64 * 0.16;
        m.lathe(
            Mat::Glow(rgb(120, 230, 255)),
            DVec3::new(0.0, y, 0.0),
            &[
                (0.1, 0.0),
                (0.26, 0.0),
                (0.29, 0.035),
                (0.26, 0.07),
                (0.1, 0.07),
            ],
        );
    }
    m.lathe(
        Mat::Glass(rgb(200, 235, 240)),
        DVec3::new(0.0, 0.38, 0.0),
        &[
            (0.4, 0.0),
            (0.4, 1.15),
            (0.36, 1.32),
            (0.24, 1.44),
            (0.12, 1.48),
        ],
    );
    m.lathe(
        Mat::Chrome,
        DVec3::new(0.0, 1.84, 0.0),
        &[
            (0.14, 0.0),
            (0.14, 0.06),
            (0.04, 0.1),
            (0.04, 0.2),
            (0.0, 0.2),
        ],
    );
    m.blob(
        Mat::Chrome,
        DVec3::new(0.0, 2.1, 0.0),
        DVec3::splat(0.1),
        0.0,
        (1.0, 1.0),
    );
    // The dial on the foot, towards the front.
    m.lathe_along(
        Mat::Glow(rgb(255, 214, 150)),
        DVec3::new(0.0, 0.24, 0.47),
        DVec3::Z,
        &[(0.0, 0.0), (0.07, 0.0), (0.07, 0.02), (0.0, 0.025)],
    );
}

/// A box with soft edges from `lo` to `hi`: a machine's cast housing.
fn casing(m: &mut Maker, mat: Mat, lo: DVec3, hi: DVec3) {
    m.blob(mat, (lo + hi) * 0.5, (hi - lo) * 0.5, 0.0, (0.16, 0.16));
}

/// A round dial facing the front at `at`: a brass rim, a lit face.
fn dial(m: &mut Maker, at: DVec3, r: f64, lit: Color) {
    m.lathe_along(
        Mat::Brass,
        at,
        DVec3::Z,
        &[(r * 1.25, 0.0), (r * 1.25, 0.02), (r, 0.03), (r, 0.0)],
    );
    m.lathe_along(
        Mat::Glow(lit),
        at,
        DVec3::Z,
        &[(0.0, 0.0), (r, 0.0), (r, 0.016), (0.0, 0.02)],
    );
}

/// The enamel of the works' machines.
fn works_green() -> Mat {
    Mat::Paint(rgb(86, 128, 112))
}

fn cast_iron() -> Mat {
    Mat::Plain(rgb(52, 54, 58))
}

/// A stamping press (2.2 wide, 1.7 deep, 2.9 high): a bed between four
/// chrome columns, the ram under a heavy crown, a flywheel at its side
/// and the control box at the front.
pub fn press(m: &mut Maker) {
    let green = works_green();
    m.slab(cast_iron(), &rrect(1.9, 1.6, 0.12, 4), 0.0, 0.14);
    casing(
        m,
        green,
        DVec3::new(-0.8, 0.1, -0.62),
        DVec3::new(0.8, 0.98, 0.62),
    );
    // The bolster: a steel plate with a slot, the die on it.
    m.slab(
        Mat::Plain(rgb(120, 124, 130)),
        &rrect(1.36, 1.0, 0.04, 3),
        0.97,
        1.05,
    );
    m.slab(Mat::Chrome, &rrect(0.7, 0.5, 0.03, 3), 1.05, 1.13);
    for (x, z) in [(-0.68, -0.46), (0.68, -0.46), (-0.68, 0.46), (0.68, 0.46)] {
        m.lathe(
            Mat::Chrome,
            DVec3::new(x, 0.97, z),
            &[(0.1, 0.0), (0.1, 0.06), (0.07, 0.08), (0.07, 1.2)],
        );
    }
    // The ram and its punch, part way down.
    casing(
        m,
        cast_iron(),
        DVec3::new(-0.52, 1.5, -0.36),
        DVec3::new(0.52, 2.2, 0.36),
    );
    m.slab(Mat::Chrome, &rrect(0.6, 0.42, 0.03, 3), 1.4, 1.52);
    // The crown.
    casing(
        m,
        green,
        DVec3::new(-0.85, 2.12, -0.64),
        DVec3::new(0.85, 2.82, 0.64),
    );
    m.slab(cast_iron(), &rrect(1.3, 0.9, 0.1, 4), 2.8, 2.88);
    // The flywheel on its shaft, with a brass hub.
    m.lathe_along(
        cast_iron(),
        DVec3::new(0.86, 2.42, 0.0),
        DVec3::X,
        &[
            (0.1, 0.0),
            (0.1, 0.05),
            (0.4, 0.05),
            (0.44, 0.08),
            (0.44, 0.17),
            (0.4, 0.2),
            (0.0, 0.2),
        ],
    );
    m.lathe_along(
        Mat::Brass,
        DVec3::new(1.06, 2.42, 0.0),
        DVec3::X,
        &[(0.12, 0.0), (0.1, 0.035), (0.0, 0.04)],
    );
    // The maker's plate and a pressure dial on the crown.
    m.blob(
        Mat::Brass,
        DVec3::new(-0.3, 2.47, 0.64),
        DVec3::new(0.3, 0.1, 0.012),
        0.0,
        (0.3, 0.3),
    );
    dial(m, DVec3::new(0.42, 2.47, 0.642), 0.11, rgb(255, 226, 170));
    // The control box on a stalk at the front left: two buttons and a lamp.
    m.tube(
        Mat::Chrome,
        &[DVec3::new(-0.98, 0.14, 0.7), DVec3::new(-0.98, 1.2, 0.7)],
        0.03,
    );
    casing(
        m,
        cast_iron(),
        DVec3::new(-1.1, 1.15, 0.62),
        DVec3::new(-0.86, 1.5, 0.78),
    );
    for (y, col) in [
        (1.42, rgb(255, 70, 50)),
        (1.32, rgb(90, 255, 120)),
        (1.22, rgb(255, 214, 150)),
    ] {
        m.lathe_along(
            Mat::Glow(col),
            DVec3::new(-0.98, y, 0.78),
            DVec3::Z,
            &[(0.0, 0.0), (0.032, 0.0), (0.032, 0.012), (0.0, 0.02)],
        );
    }
}

/// A lathe (3 m long, 1.1 deep, 1.5 high): a bed on two pedestals, the
/// headstock with its chuck and a brass bar in it, the carriage with its
/// handwheel, the tailstock.
pub fn lathe_machine(m: &mut Maker) {
    let green = works_green();
    for x in [-1.08, 1.0] {
        let mut f = m.sub(DVec3::new(x, 0.0, 0.0), 0.0);
        f.slab(cast_iron(), &rrect(0.72, 0.62, 0.06, 3), 0.0, 0.08);
        casing(
            &mut f,
            green,
            DVec3::new(-0.32, 0.06, -0.27),
            DVec3::new(0.32, 0.78, 0.27),
        );
    }
    // The chip tray and the bed with its two ways.
    m.slab(
        Mat::Plain(rgb(96, 100, 104)),
        &rrect(2.5, 0.72, 0.05, 3),
        0.56,
        0.6,
    );
    casing(
        m,
        cast_iron(),
        DVec3::new(-1.44, 0.76, -0.2),
        DVec3::new(1.44, 0.93, 0.2),
    );
    for z in [-0.13, 0.13] {
        m.tube(
            Mat::Chrome,
            &[DVec3::new(-0.7, 0.935, z), DVec3::new(1.42, 0.935, z)],
            0.022,
        );
    }
    // The headstock, its chuck and the work.
    casing(
        m,
        green,
        DVec3::new(-1.46, 0.9, -0.3),
        DVec3::new(-0.72, 1.46, 0.3),
    );
    m.lathe_along(
        Mat::Chrome,
        DVec3::new(-0.72, 1.2, 0.0),
        DVec3::X,
        &[
            (0.09, 0.0),
            (0.09, 0.04),
            (0.2, 0.04),
            (0.2, 0.17),
            (0.06, 0.19),
            (0.0, 0.19),
        ],
    );
    m.lathe_along(
        Mat::Brass,
        DVec3::new(-0.54, 1.2, 0.0),
        DVec3::X,
        &[(0.05, 0.0), (0.05, 0.9), (0.035, 0.92), (0.035, 1.3)],
    );
    dial(m, DVec3::new(-1.26, 1.3, 0.302), 0.07, rgb(255, 226, 170));
    for (x, lean) in [(-1.02, 0.07), (-0.88, -0.05)] {
        m.tube(
            Mat::Chrome,
            &[DVec3::new(x, 1.12, 0.3), DVec3::new(x + lean, 1.2, 0.42)],
            0.012,
        );
        m.blob(
            Mat::Plain(rgb(24, 24, 26)),
            DVec3::new(x + lean, 1.2, 0.42),
            DVec3::splat(0.03),
            0.0,
            (1.0, 1.0),
        );
    }
    // The carriage: a saddle over the bed, the tool post, an apron down
    // the front with the handwheel.
    casing(
        m,
        Mat::Plain(rgb(96, 100, 104)),
        DVec3::new(0.0, 0.93, -0.26),
        DVec3::new(0.4, 1.03, 0.3),
    );
    casing(
        m,
        cast_iron(),
        DVec3::new(0.12, 1.02, -0.08),
        DVec3::new(0.28, 1.16, 0.08),
    );
    casing(
        m,
        green,
        DVec3::new(0.02, 0.62, 0.2),
        DVec3::new(0.38, 0.95, 0.3),
    );
    m.lathe_along(
        Mat::Chrome,
        DVec3::new(0.2, 0.78, 0.3),
        DVec3::Z,
        &[
            (0.02, 0.0),
            (0.02, 0.07),
            (0.11, 0.07),
            (0.125, 0.085),
            (0.11, 0.1),
            (0.0, 0.1),
        ],
    );
    // The tailstock and its centre.
    casing(
        m,
        green,
        DVec3::new(1.0, 0.92, -0.16),
        DVec3::new(1.36, 1.32, 0.16),
    );
    m.lathe_along(
        Mat::Chrome,
        DVec3::new(1.0, 1.2, 0.0),
        -DVec3::X,
        &[(0.045, 0.0), (0.045, 0.14), (0.0, 0.24)],
    );
    m.lathe_along(
        Mat::Chrome,
        DVec3::new(1.36, 1.2, 0.0),
        DVec3::X,
        &[
            (0.02, 0.0),
            (0.02, 0.04),
            (0.09, 0.04),
            (0.09, 0.06),
            (0.0, 0.06),
        ],
    );
}

/// An Aether dynamo (2 m square, 2.3 high): a finned drum in its cradle,
/// a band of light round it, a terminal on top, dials at the front.
pub fn dynamo(m: &mut Maker) {
    let teal = Mat::Paint(rgb(60, 120, 130));
    m.slab(cast_iron(), &rrect(1.9, 1.5, 0.1, 4), 0.0, 0.16);
    for x in [-0.5, 0.5] {
        casing(
            m,
            cast_iron(),
            DVec3::new(x - 0.12, 0.12, -0.6),
            DVec3::new(x + 0.12, 0.62, 0.6),
        );
    }
    m.lathe_along(
        teal,
        DVec3::new(-0.8, 1.1, 0.0),
        DVec3::X,
        &[
            (0.0, 0.0),
            (0.42, 0.0),
            (0.62, 0.08),
            (0.72, 0.2),
            (0.72, 1.4),
            (0.62, 1.52),
            (0.42, 1.6),
            (0.0, 1.6),
        ],
    );
    // Cooling fins either side of the lit band.
    for k in 0..8 {
        let t = if k < 4 {
            0.26 + k as f64 * 0.1
        } else {
            1.0 + (k - 4) as f64 * 0.1
        };
        m.lathe_along(
            Mat::Chrome,
            DVec3::new(-0.8 + t, 1.1, 0.0),
            DVec3::X,
            &[(0.7, 0.0), (0.78, 0.008), (0.78, 0.032), (0.7, 0.04)],
        );
    }
    m.lathe_along(
        Mat::Glow(rgb(120, 230, 255)),
        DVec3::new(-0.11, 1.1, 0.0),
        DVec3::X,
        &[(0.7, 0.0), (0.735, 0.01), (0.735, 0.21), (0.7, 0.22)],
    );
    // The shaft's brass caps.
    for (x, d) in [(-0.8, -1.0), (0.8, 1.0)] {
        m.lathe_along(
            Mat::Brass,
            DVec3::new(x, 1.1, 0.0),
            DVec3::X * d,
            &[(0.16, 0.0), (0.16, 0.06), (0.08, 0.1), (0.0, 0.1)],
        );
    }
    // The terminal: ribbed insulators, a glowing ball.
    m.lathe(
        Mat::Paint(rgb(236, 232, 220)),
        DVec3::new(0.0, 1.78, 0.0),
        &[
            (0.16, 0.0),
            (0.16, 0.06),
            (0.09, 0.09),
            (0.14, 0.14),
            (0.09, 0.19),
            (0.13, 0.24),
            (0.08, 0.29),
            (0.05, 0.34),
            (0.0, 0.34),
        ],
    );
    m.blob(
        Mat::Glow(rgb(150, 240, 255)),
        DVec3::new(0.0, 2.2, 0.0),
        DVec3::splat(0.1),
        0.0,
        (1.0, 1.0),
    );
    // The switchboard at the front.
    casing(
        m,
        cast_iron(),
        DVec3::new(-0.45, 0.14, 0.76),
        DVec3::new(0.45, 1.0, 0.96),
    );
    for (x, col) in [
        (-0.26, rgb(255, 226, 170)),
        (0.0, rgb(120, 230, 255)),
        (0.26, rgb(255, 226, 170)),
    ] {
        dial(m, DVec3::new(x, 0.78, 0.962), 0.085, col);
    }
    m.tube(
        Mat::Chrome,
        &[
            DVec3::new(-0.3, 0.44, 0.97),
            DVec3::new(-0.3, 0.5, 1.0),
            DVec3::new(0.3, 0.5, 1.0),
            DVec3::new(0.3, 0.44, 0.97),
        ],
        0.014,
    );
}

/// A workbench, `w` long: a thick beech top on a steel frame, drawers at
/// one end, a vice at the other, a tool box and an oil can on it.
pub fn workbench(m: &mut Maker, w: f64) {
    let hw = w * 0.5;
    let steel = Mat::Paint(rgb(92, 104, 110));
    m.block(
        Mat::Wood(rgb(196, 150, 100)),
        DVec3::new(-hw, 0.82, -0.4),
        DVec3::new(hw, 0.9, 0.4),
    );
    for x in [-hw + 0.06, hw - 0.06] {
        for z in [-0.33, 0.33] {
            m.block(
                steel,
                DVec3::new(x - 0.03, 0.0, z - 0.03),
                DVec3::new(x + 0.03, 0.82, z + 0.03),
            );
        }
    }
    m.block(
        steel,
        DVec3::new(-hw + 0.04, 0.16, -0.34),
        DVec3::new(hw - 0.04, 0.19, 0.34),
    );
    // Drawers at the left, with chrome pulls.
    m.block(
        steel,
        DVec3::new(-hw + 0.1, 0.3, -0.34),
        DVec3::new(-hw + 0.7, 0.81, 0.36),
    );
    for k in 0..3 {
        let y = 0.4 + k as f64 * 0.16;
        m.tube(
            Mat::Chrome,
            &[
                DVec3::new(-hw + 0.3, y, 0.375),
                DVec3::new(-hw + 0.5, y, 0.375),
            ],
            0.012,
        );
    }
    // The vice at the right front corner.
    let mut v = m.sub(DVec3::new(hw - 0.4, 0.9, 0.24), 0.0);
    casing(
        &mut v,
        cast_iron(),
        DVec3::new(-0.09, 0.0, -0.16),
        DVec3::new(0.09, 0.1, 0.1),
    );
    for z in [-0.02, 0.12] {
        casing(
            &mut v,
            cast_iron(),
            DVec3::new(-0.08, 0.08, z - 0.03),
            DVec3::new(0.08, 0.22, z + 0.03),
        );
    }
    v.tube(
        Mat::Chrome,
        &[DVec3::new(0.0, 0.12, 0.12), DVec3::new(0.0, 0.12, 0.26)],
        0.016,
    );
    v.tube(
        Mat::Chrome,
        &[DVec3::new(-0.11, 0.12, 0.25), DVec3::new(0.11, 0.12, 0.25)],
        0.009,
    );
    // A red tool box with a chrome handle, and an oil can.
    let mut t = m.sub(DVec3::new(-0.1, 0.9, -0.12), 0.12);
    casing(
        &mut t,
        Mat::Paint(rgb(190, 50, 40)),
        DVec3::new(-0.26, 0.0, -0.12),
        DVec3::new(0.26, 0.2, 0.12),
    );
    t.tube(
        Mat::Chrome,
        &[
            DVec3::new(-0.12, 0.2, 0.0),
            DVec3::new(-0.1, 0.25, 0.0),
            DVec3::new(0.1, 0.25, 0.0),
            DVec3::new(0.12, 0.2, 0.0),
        ],
        0.01,
    );
    m.lathe(
        Mat::Brass,
        DVec3::new(0.42, 0.9, -0.18),
        &[
            (0.055, 0.0),
            (0.055, 0.07),
            (0.02, 0.11),
            (0.008, 0.2),
            (0.0, 0.2),
        ],
    );
}

/// One crate from `lo` to `hi`: boards, a batten round each end, steel
/// corners and a stencilled mark on the front.
fn crate_box(m: &mut Maker, lo: DVec3, hi: DVec3, wood: Color) {
    m.block(Mat::Paint(wood), lo, hi);
    let dark = Mat::Paint(scale(wood, 0.78));
    let t = 0.018;
    for y in [lo.y + 0.03, hi.y - 0.11] {
        m.block(
            dark,
            DVec3::new(lo.x - t, y, lo.z - t),
            DVec3::new(hi.x + t, y + 0.08, hi.z + t),
        );
    }
    for x in [lo.x - t, hi.x - 0.07 + t] {
        m.block(
            dark,
            DVec3::new(x, lo.y + 0.11, lo.z - t),
            DVec3::new(x + 0.07, hi.y - 0.11, hi.z + t),
        );
    }
    let c = (lo + hi) * 0.5;
    m.lathe_along(
        Mat::Plain(rgb(150, 40, 34)),
        DVec3::new(c.x, c.y, hi.z),
        DVec3::Z,
        &[(0.0, 0.0), (0.11, 0.0), (0.11, 0.006), (0.0, 0.008)],
    );
}

/// A stack of crates (1.6 wide, 1.2 deep, 1.5 high): two below, one
/// askew on top.
pub fn crates(m: &mut Maker) {
    let pine = rgb(200, 164, 112);
    crate_box(
        m,
        DVec3::new(-0.76, 0.0, -0.52),
        DVec3::new(-0.04, 0.72, 0.5),
        pine,
    );
    crate_box(
        m,
        DVec3::new(0.04, 0.0, -0.5),
        DVec3::new(0.76, 0.8, 0.46),
        scale(pine, 0.92),
    );
    let mut top = m.sub(DVec3::new(-0.18, 0.72, -0.02), 0.2);
    crate_box(
        &mut top,
        DVec3::new(-0.3, 0.0, -0.36),
        DVec3::new(0.3, 0.62, 0.36),
        scale(pine, 1.06),
    );
}

/// A gang's cot (0.9 wide, 2.0 long, 0.5 high): a canvas bed on a steel
/// frame, a rumpled grey blanket and a flat pillow at the head (at -z).
pub fn cot(m: &mut Maker) {
    let steel = Mat::Paint(rgb(88, 96, 90));
    for x in [-0.4, 0.4] {
        m.block(
            steel,
            DVec3::new(x - 0.025, 0.24, -0.95),
            DVec3::new(x + 0.025, 0.3, 0.95),
        );
        for z in [-0.9, 0.9] {
            m.block(
                steel,
                DVec3::new(x - 0.03, 0.0, z - 0.03),
                DVec3::new(x + 0.03, 0.26, z + 0.03),
            );
        }
    }
    for z in [-0.9, 0.0, 0.9] {
        m.block(
            steel,
            DVec3::new(-0.42, 0.26, z - 0.02),
            DVec3::new(0.42, 0.29, z + 0.02),
        );
    }
    m.block(
        Mat::Cloth(rgb(112, 118, 86)),
        DVec3::new(-0.39, 0.29, -0.93),
        DVec3::new(0.39, 0.33, 0.93),
    );
    m.blob(
        Mat::Cloth(rgb(98, 104, 112)),
        DVec3::new(0.0, 0.39, 0.22),
        DVec3::new(0.36, 0.075, 0.6),
        0.0,
        (0.5, 0.5),
    );
    m.blob(
        Mat::Cloth(rgb(226, 222, 208)),
        DVec3::new(0.02, 0.37, -0.72),
        DVec3::new(0.26, 0.05, 0.16),
        0.0,
        (0.5, 0.5),
    );
}

/// A round card table (1.95 across, 0.78 high): a walnut rail round green
/// felt, stacks of chips, a fan of cards, an ashtray and a glass of
/// something amber.
pub fn poker_table(m: &mut Maker) {
    let walnut = Mat::Wood(rgb(96, 58, 36));
    m.lathe(
        walnut,
        DVec3::ZERO,
        &[
            (0.0, 0.0),
            (0.36, 0.0),
            (0.3, 0.07),
            (0.11, 0.13),
            (0.1, 0.66),
            (0.3, 0.7),
            (0.0, 0.7),
        ],
    );
    m.lathe(
        walnut,
        DVec3::new(0.0, 0.7, 0.0),
        &[
            (0.84, 0.0),
            (0.97, 0.0),
            (0.98, 0.03),
            (0.98, 0.07),
            (0.9, 0.09),
            (0.84, 0.08),
        ],
    );
    m.lathe(
        Mat::Cloth(rgb(30, 104, 66)),
        DVec3::new(0.0, 0.7, 0.0),
        &[(0.0, 0.0), (0.85, 0.0), (0.85, 0.07), (0.0, 0.07)],
    );
    let top = 0.77;
    let chips = [
        rgb(200, 40, 40),
        rgb(40, 80, 190),
        rgb(236, 232, 220),
        rgb(30, 30, 34),
    ];
    for (k, (x, z)) in [
        (-0.45, 0.2),
        (-0.25, 0.5),
        (0.3, 0.45),
        (0.5, 0.1),
        (0.1, -0.5),
        (-0.4, -0.35),
    ]
    .into_iter()
    .enumerate()
    {
        let n = 3 + (k * 2) % 5;
        m.lathe(
            Mat::Plain(chips[k % 4]),
            DVec3::new(x, top, z),
            &[
                (0.0, 0.0),
                (0.032, 0.0),
                (0.032, 0.012 * n as f64),
                (0.0, 0.012 * n as f64),
            ],
        );
        m.lathe(
            Mat::Plain(scale(chips[(k + 1) % 4], 1.1)),
            DVec3::new(x, top + 0.012 * n as f64 * 0.5, z),
            &[(0.0, 0.0), (0.034, 0.0), (0.034, 0.004), (0.0, 0.004)],
        );
    }
    // A fan of cards.
    for k in 0..4 {
        let a = -0.5 + k as f64 * 0.33;
        let mut c = m.sub(DVec3::new(0.05, top, 0.05), a);
        c.block(
            Mat::Paint(rgb(240, 236, 224)),
            DVec3::new(-0.03, 0.0, -0.05 + k as f64 * 0.003),
            DVec3::new(0.03, 0.004, 0.05 + k as f64 * 0.003),
        );
    }
    m.lathe(
        Mat::Chrome,
        DVec3::new(0.62, top, -0.3),
        &[
            (0.0, 0.0),
            (0.07, 0.0),
            (0.075, 0.02),
            (0.05, 0.025),
            (0.0, 0.02),
        ],
    );
    m.lathe(
        Mat::Glass(rgb(224, 160, 70)),
        DVec3::new(-0.7, top, -0.1),
        &[(0.0, 0.0), (0.03, 0.0), (0.04, 0.09), (0.0, 0.09)],
    );
}

/// A safe (0.8 deep, 1.5 high) in dark green enamel: a chrome-rimmed
/// door with a brass dial and a spoked wheel handle, on little feet.
pub fn safe(m: &mut Maker, w: f64) {
    let hw = w * 0.5;
    let enamel = Mat::Paint(rgb(40, 72, 60));
    casing(
        m,
        enamel,
        DVec3::new(-hw, 0.07, -0.4),
        DVec3::new(hw, 1.5, 0.4),
    );
    for x in [-hw + 0.1, hw - 0.1] {
        m.block(
            cast_iron(),
            DVec3::new(x - 0.07, 0.0, -0.32),
            DVec3::new(x + 0.07, 0.08, 0.32),
        );
    }
    // The door, a step proud of the front, with a chrome rim.
    m.block(
        Mat::Chrome,
        DVec3::new(-hw + 0.07, 0.2, 0.37),
        DVec3::new(hw - 0.07, 1.4, 0.4),
    );
    m.block(
        Mat::Paint(rgb(52, 90, 76)),
        DVec3::new(-hw + 0.11, 0.24, 0.395),
        DVec3::new(hw - 0.11, 1.36, 0.42),
    );
    dial(m, DVec3::new(0.0, 1.0, 0.42), 0.11, rgb(255, 226, 170));
    // The wheel handle: a hub, three spokes.
    m.lathe_along(
        Mat::Chrome,
        DVec3::new(0.0, 0.58, 0.42),
        DVec3::Z,
        &[(0.0, 0.0), (0.05, 0.0), (0.05, 0.05), (0.0, 0.05)],
    );
    for k in 0..3 {
        let a = k as f64 * TAU / 3.0 + 0.4;
        m.tube(
            Mat::Chrome,
            &[
                DVec3::new(0.0, 0.58, 0.46),
                DVec3::new(a.cos() * 0.15, 0.58 + a.sin() * 0.15, 0.46),
            ],
            0.014,
        );
    }
    // Hinges at the left edge.
    for y in [0.4, 1.2] {
        m.block(
            Mat::Chrome,
            DVec3::new(-hw + 0.02, y, 0.34),
            DVec3::new(-hw + 0.09, y + 0.12, 0.43),
        );
    }
}

/// A gang's strongroom safe (1.1 wide, 0.84 deep, 1.5 high, its middle at
/// the origin, the door to the front): what a hideout case's thing is
/// locked in. Hollow, with a shelf, so the same body stands there shut
/// (`open` 0) and broken open (`open` 1: the door hangs wide on its
/// hinges, its bolts drawn, and what is inside shows). Its box is
/// `hw_sim::missions::SAFE_HALF` by `SAFE_HEIGHT`: what heroes bump into.
pub fn vault(m: &mut Maker, open: f64) {
    let (hw, back, front) = (0.55, -0.42, 0.33);
    let enamel = Mat::Paint(rgb(38, 66, 58));
    let lining = Mat::Paint(rgb(70, 78, 80));
    // A cast-iron plinth on four turned feet, and a stepped cap.
    for (x, z) in [(-0.43, -0.3), (0.43, -0.3), (-0.43, 0.24), (0.43, 0.24)] {
        m.lathe(
            cast_iron(),
            DVec3::new(x, 0.0, z),
            &[
                (0.0, 0.0),
                (0.07, 0.0),
                (0.08, 0.03),
                (0.055, 0.07),
                (0.0, 0.07),
            ],
        );
    }
    let mut body = m.sub(DVec3::new(0.0, 0.0, (back + front) * 0.5), 0.0);
    let depth = front - back;
    body.slab(cast_iron(), &rrect(hw * 2.0, depth, 0.05, 5), 0.06, 0.12);
    body.slab(enamel, &rrect(hw * 2.0, depth, 0.05, 5), 1.4, 1.46);
    body.slab(
        cast_iron(),
        &rrect(hw * 2.0 - 0.08, depth - 0.08, 0.04, 5),
        1.46,
        1.5,
    );
    // The walls: 8 cm of steel all round, open to the front.
    let (lo, hi) = (0.12, 1.4);
    m.block(
        enamel,
        DVec3::new(-hw + 0.01, lo, back + 0.01),
        DVec3::new(hw - 0.01, hi, back + 0.09),
    );
    for x in [-hw + 0.01, hw - 0.09] {
        m.block(
            enamel,
            DVec3::new(x, lo, back + 0.09),
            DVec3::new(x + 0.08, hi, front),
        );
    }
    m.block(
        enamel,
        DVec3::new(-hw + 0.09, lo, back + 0.09),
        DVec3::new(hw - 0.09, lo + 0.08, front),
    );
    m.block(
        enamel,
        DVec3::new(-hw + 0.09, hi - 0.08, back + 0.09),
        DVec3::new(hw - 0.09, hi, front),
    );
    // Chrome bands round the body, riveted at the corners.
    for y in [0.3, 1.22] {
        for x in [-hw, hw - 0.012] {
            m.block(
                Mat::Chrome,
                DVec3::new(x, y, back + 0.02),
                DVec3::new(x + 0.012, y + 0.05, front),
            );
        }
        for x in [-hw + 0.02, hw - 0.02] {
            m.lathe_along(
                Mat::Brass,
                DVec3::new(x, y + 0.025, front),
                DVec3::Z,
                &[(0.0, 0.0), (0.016, 0.0), (0.01, 0.012), (0.0, 0.014)],
            );
        }
    }
    // Inside: a grey lining, a steel shelf with a few bundles and a ledger.
    m.block(
        lining,
        DVec3::new(-hw + 0.09, lo + 0.08, back + 0.09),
        DVec3::new(hw - 0.09, hi - 0.08, back + 0.1),
    );
    m.block(
        lining,
        DVec3::new(-hw + 0.09, lo + 0.08, back + 0.1),
        DVec3::new(hw - 0.09, lo + 0.085, front - 0.02),
    );
    m.block(
        Mat::Chrome,
        DVec3::new(-hw + 0.09, 0.98, back + 0.1),
        DVec3::new(hw - 0.09, 1.0, front - 0.06),
    );
    for (k, x) in [-0.34, -0.1, 0.16].into_iter().enumerate() {
        m.block(
            Mat::Paint(rgb(138, 178, 112)),
            DVec3::new(x, 1.0, -0.2),
            DVec3::new(x + 0.2, 1.05 + 0.02 * k as f64, -0.08),
        );
        m.block(
            Mat::Paint(rgb(236, 226, 190)),
            DVec3::new(x + 0.08, 1.0, -0.202),
            DVec3::new(x + 0.12, 1.052 + 0.02 * k as f64, -0.078),
        );
    }
    m.block(
        Mat::Plain(rgb(96, 40, 36)),
        DVec3::new(-0.3, 1.0, 0.0),
        DVec3::new(-0.04, 1.045, 0.18),
    );
    m.block(
        Mat::Paint(rgb(236, 226, 190)),
        DVec3::new(-0.29, 1.005, 0.004),
        DVec3::new(-0.045, 1.04, 0.184),
    );
    // The maker's plate over the door.
    m.block(
        Mat::Brass,
        DVec3::new(-0.16, hi - 0.062, front),
        DVec3::new(0.16, hi - 0.018, front + 0.006),
    );
    // The door on its hinge at the left: wide open, it swings out to the
    // front and round to the side.
    let (dw, dy0, dy1) = (hw * 2.0 - 0.18 - 0.02, lo + 0.09, hi - 0.09);
    let hinge = DVec3::new(-hw + 0.1, 0.0, front);
    for y in [dy0 + 0.14, (dy0 + dy1) * 0.5 - 0.06, dy1 - 0.26] {
        m.lathe(
            Mat::Chrome,
            DVec3::new(hinge.x - 0.035, y, front + 0.03),
            &[(0.0, 0.0), (0.028, 0.0), (0.028, 0.12), (0.0, 0.12)],
        );
    }
    let mut d = m.sub(hinge, -open * 1.95);
    // A chrome rim, the enamelled face a step proud, a steel plug behind.
    d.block(
        Mat::Chrome,
        DVec3::new(0.0, dy0, 0.0),
        DVec3::new(dw, dy1, 0.05),
    );
    d.block(
        Mat::Paint(rgb(50, 88, 76)),
        DVec3::new(0.045, dy0 + 0.045, 0.05),
        DVec3::new(dw - 0.045, dy1 - 0.045, 0.085),
    );
    d.block(
        lining,
        DVec3::new(0.05, dy0 + 0.05, -0.05),
        DVec3::new(dw - 0.05, dy1 - 0.05, 0.0),
    );
    // Behind the plug, the bolt work: a brass case over the lock, and
    // the bars that throw the bolts.
    d.block(
        Mat::Brass,
        DVec3::new(dw * 0.5 - 0.16, dy0 + 0.66, -0.065),
        DVec3::new(dw * 0.5 + 0.16, dy0 + 0.98, -0.05),
    );
    for y in [dy0 + 0.2, (dy0 + dy1) * 0.5, dy1 - 0.2] {
        d.block(
            Mat::Chrome,
            DVec3::new(0.12, y - 0.018, -0.062),
            DVec3::new(dw - 0.06, y + 0.018, -0.05),
        );
    }
    d.block(
        Mat::Chrome,
        DVec3::new(dw * 0.5 - 0.02, dy0 + 0.2, -0.068),
        DVec3::new(dw * 0.5 + 0.02, dy1 - 0.2, -0.062),
    );
    // The bolts in the free edge: drawn back unless it is shut.
    for y in [dy0 + 0.2, (dy0 + dy1) * 0.5, dy1 - 0.2] {
        d.lathe_along(
            Mat::Chrome,
            DVec3::new(dw - 0.06, y, -0.025),
            DVec3::X,
            &[(0.0, 0.0), (0.02, 0.0), (0.02, 0.085), (0.0, 0.09)],
        );
    }
    let mid = dw * 0.5;
    dial(
        &mut d,
        DVec3::new(mid, dy0 + 0.82, 0.085),
        0.1,
        rgb(255, 226, 170),
    );
    // The wheel handle: a hub, five spokes and a rim.
    let hub = DVec3::new(mid, dy0 + 0.44, 0.085);
    d.lathe_along(
        Mat::Chrome,
        hub,
        DVec3::Z,
        &[
            (0.0, 0.0),
            (0.05, 0.0),
            (0.045, 0.06),
            (0.02, 0.075),
            (0.0, 0.075),
        ],
    );
    let rim: Vec<DVec3> = (0..=20)
        .map(|k| {
            let a = k as f64 * TAU / 20.0;
            hub + DVec3::new(a.cos() * 0.16, a.sin() * 0.16, 0.055)
        })
        .collect();
    d.tube(Mat::Chrome, &rim, 0.014);
    for k in 0..5 {
        let a = k as f64 * TAU / 5.0 + 0.3;
        d.tube(
            Mat::Chrome,
            &[
                hub + DVec3::new(0.0, 0.0, 0.055),
                hub + DVec3::new(a.cos() * 0.16, a.sin() * 0.16, 0.055),
            ],
            0.011,
        );
    }
}

/// A planning board on legs (0.5 deep, 1.9 high): a map of the city with
/// photographs, coloured pins and red string between them.
pub fn pin_board(m: &mut Maker, w: f64) {
    let hw = w * 0.5;
    let steel = Mat::Paint(rgb(74, 80, 86));
    for x in [-hw + 0.1, hw - 0.1] {
        m.block(
            steel,
            DVec3::new(x - 0.03, 0.0, -0.22),
            DVec3::new(x + 0.03, 1.4, -0.16),
        );
        m.block(
            steel,
            DVec3::new(x - 0.05, 0.0, -0.25),
            DVec3::new(x + 0.05, 0.04, 0.2),
        );
    }
    m.block(
        Mat::Wood(rgb(120, 82, 52)),
        DVec3::new(-hw, 0.8, -0.21),
        DVec3::new(hw, 1.9, -0.15),
    );
    m.block(
        Mat::Paint(rgb(196, 160, 112)),
        DVec3::new(-hw + 0.05, 0.85, -0.15),
        DVec3::new(hw - 0.05, 1.85, -0.135),
    );
    // The map: a pale sheet with street lines and a blue river.
    m.block(
        Mat::Paint(rgb(236, 226, 190)),
        DVec3::new(-hw + 0.25, 0.95, -0.135),
        DVec3::new(hw - 0.4, 1.75, -0.125),
    );
    for k in 0..5 {
        let y = 1.0 + k as f64 * 0.16;
        m.block(
            Mat::Paint(rgb(150, 142, 124)),
            DVec3::new(-hw + 0.28, y, -0.125),
            DVec3::new(hw - 0.43, y + 0.012, -0.12),
        );
    }
    m.block(
        Mat::Paint(rgb(110, 160, 200)),
        DVec3::new(-hw + 0.5, 0.97, -0.125),
        DVec3::new(-hw + 0.6, 1.73, -0.118),
    );
    // Photographs and pins, joined by string.
    let spots = [
        (hw - 0.3, 1.62),
        (hw - 0.34, 1.2),
        (-hw + 0.45, 1.5),
        (0.1, 1.25),
        (-0.15, 1.65),
    ];
    for (k, &(x, y)) in spots.iter().enumerate() {
        let x = x.clamp(-hw + 0.15, hw - 0.15);
        m.block(
            Mat::Paint(rgb(230, 230, 222)),
            DVec3::new(x - 0.08, y - 0.1, -0.12),
            DVec3::new(x + 0.08, y + 0.1, -0.105),
        );
        m.block(
            Mat::Paint(rgb(70, 70, 76)),
            DVec3::new(x - 0.065, y - 0.04, -0.105),
            DVec3::new(x + 0.065, y + 0.085, -0.1),
        );
        m.blob(
            Mat::Plain(if k % 2 == 0 {
                rgb(210, 40, 40)
            } else {
                rgb(230, 190, 40)
            }),
            DVec3::new(x, y + 0.12, -0.09),
            DVec3::splat(0.018),
            0.0,
            (1.0, 1.0),
        );
        if let Some(&(nx, ny)) = spots.get(k + 1) {
            let nx = nx.clamp(-hw + 0.15, hw - 0.15);
            m.tube(
                Mat::Plain(rgb(190, 30, 36)),
                &[
                    DVec3::new(x, y + 0.12, -0.085),
                    DVec3::new(nx, ny + 0.12, -0.085),
                ],
                0.004,
            );
        }
    }
}

/// Three oil drums (1.1 across, 1.0 high): rust red, olive and yellow
/// with a hazard band, ribbed, with bungs in their lids.
pub fn drums(m: &mut Maker) {
    for (k, (x, z, col)) in [
        (-0.28, -0.2, rgb(150, 62, 40)),
        (0.3, -0.15, rgb(92, 104, 70)),
        (0.0, 0.3, rgb(214, 170, 40)),
    ]
    .into_iter()
    .enumerate()
    {
        let h = 0.9 + 0.04 * k as f64;
        m.lathe(
            Mat::Paint(col),
            DVec3::new(x, 0.0, z),
            &[
                (0.0, 0.0),
                (0.26, 0.0),
                (0.28, 0.03),
                (0.28, h - 0.03),
                (0.26, h),
                (0.0, h),
            ],
        );
        for y in [h * 0.33, h * 0.66] {
            m.lathe(
                Mat::Paint(scale(col, 0.8)),
                DVec3::new(x, y, z),
                &[(0.278, 0.0), (0.292, 0.012), (0.292, 0.028), (0.278, 0.04)],
            );
        }
        m.lathe(
            Mat::Chrome,
            DVec3::new(x + 0.1, h, z + 0.05),
            &[(0.0, 0.0), (0.035, 0.0), (0.035, 0.012), (0.0, 0.012)],
        );
        if k == 2 {
            m.lathe(
                Mat::Plain(rgb(28, 28, 30)),
                DVec3::new(x, h * 0.45, z),
                &[(0.279, 0.0), (0.285, 0.0), (0.285, 0.16), (0.279, 0.16)],
            );
        }
    }
}

/// A low wall of sandbags `w` wide (0.6 deep, 0.85 high), its front
/// to +z: two courses two bags deep, two more one bag deep on top, each
/// course laid half a bag on from the one below, every bag a little
/// different in size, lie and the colour of its burlap.
pub fn sandbags(m: &mut Maker, w: f64) {
    use hw_gen::layout::furnish::SANDBAGS_DEPTH;
    const LONG: f64 = 0.56;
    const PITCH: f64 = 0.2;
    let n = ((w / LONG).round() as usize).max(2);
    let long = w / n as f64;
    let mut k: u64 = 0;
    // A number from 0 to 1, the next for every bag (same every time).
    let mut next = || {
        k += 1;
        let h = k.wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(29) ^ k;
        (h.wrapping_mul(0xBF58_476D_1CE4_E5B9) >> 40) as f64 / (1u64 << 24) as f64
    };
    for course in 0..4 {
        let y = 0.115 + course as f64 * PITCH;
        let rows: &[f64] = if course < 2 {
            &[-SANDBAGS_DEPTH * 0.24, SANDBAGS_DEPTH * 0.24]
        } else {
            &[0.0]
        };
        let half_z = if course < 2 { 0.15 } else { 0.17 };
        // Every other course half a bag on, one bag fewer.
        let (count, start) = if course % 2 == 0 {
            (n, -w * 0.5 + long * 0.5)
        } else {
            (n - 1, -w * 0.5 + long)
        };
        for &z in rows {
            for i in 0..count {
                let (a, b, c) = (next(), next(), next());
                let x = start + i as f64 * long + (a - 0.5) * 0.04;
                let burlap = rgb(
                    (142.0 + 22.0 * b) as u8,
                    (122.0 + 18.0 * b) as u8,
                    (84.0 + 14.0 * c) as u8,
                );
                let mut bag = m.sub(DVec3::new(x, y, z), (c - 0.5) * 0.12);
                bag.blob(
                    Mat::Cloth(burlap),
                    DVec3::ZERO,
                    DVec3::new(long * (0.52 + 0.04 * a), 0.112, half_z * (0.96 + 0.08 * b)),
                    0.0,
                    (0.3, 0.55),
                );
            }
        }
    }
}

/// The thing a gang is hiding, lying on the floor for a hero to find
/// (about 0.7 m across, 0.4 high): a real object with a small glint
/// of its own (a lit seal, a glowing coil, a gilt rim) so it shows from
/// across the room.
pub fn evidence(m: &mut Maker, kind: hw_sim::missions::board::CrimeKind) {
    use hw_sim::missions::board::CrimeKind as K;
    let glint = Mat::Glow(rgb(255, 214, 120));
    match kind {
        K::Payroll => {
            // A steel strongbox, lid thrown back on bundles of banknotes.
            let steel = Mat::Paint(rgb(78, 92, 100));
            casing(
                m,
                steel,
                DVec3::new(-0.34, 0.0, -0.22),
                DVec3::new(0.34, 0.24, 0.22),
            );
            m.block(
                Mat::Chrome,
                DVec3::new(-0.35, 0.22, -0.23),
                DVec3::new(0.35, 0.25, 0.23),
            );
            for (k, (x, z)) in [(-0.17, -0.08), (0.05, -0.08), (-0.17, 0.1), (0.05, 0.1)]
                .into_iter()
                .enumerate()
            {
                m.block(
                    Mat::Paint(rgb(138, 178, 112)),
                    DVec3::new(x, 0.25, z - 0.05),
                    DVec3::new(x + 0.2, 0.25 + 0.06 + 0.01 * k as f64, z + 0.05),
                );
                m.block(
                    Mat::Paint(rgb(236, 226, 190)),
                    DVec3::new(x + 0.08, 0.25, z - 0.052),
                    DVec3::new(x + 0.12, 0.26 + 0.01 * k as f64 + 0.06, z + 0.052),
                );
            }
            // The lid, hinged back.
            let mut lid = m.sub(DVec3::new(0.0, 0.25, -0.22), 0.0);
            lid.block(
                steel,
                DVec3::new(-0.34, 0.0, -0.03),
                DVec3::new(0.34, 0.2, 0.0),
            );
            // A lit seal on the front.
            m.block(
                glint,
                DVec3::new(-0.06, 0.1, 0.22),
                DVec3::new(0.06, 0.16, 0.23),
            );
        }
        K::Counterfeit => {
            // A stack of engraved printing plates in a tray, an ink roller.
            m.block(
                Mat::Paint(rgb(60, 64, 70)),
                DVec3::new(-0.3, 0.0, -0.22),
                DVec3::new(0.3, 0.06, 0.22),
            );
            for k in 0..5 {
                let y = 0.06 + 0.025 * k as f64;
                m.block(
                    Mat::Chrome,
                    DVec3::new(-0.26 + 0.01 * k as f64, y, -0.18),
                    DVec3::new(0.24 + 0.01 * k as f64, y + 0.02, 0.18),
                );
            }
            m.lathe_along(
                Mat::Plain(rgb(40, 90, 56)),
                DVec3::new(-0.32, 0.12, 0.0),
                DVec3::X,
                &[(0.0, 0.0), (0.05, 0.0), (0.05, 0.64), (0.0, 0.64)],
            );
            m.block(
                glint,
                DVec3::new(-0.2, 0.185, -0.1),
                DVec3::new(0.2, 0.19, -0.06),
            );
        }
        K::Smuggling => {
            // A crate, its lid pried off, packed with straw and a glowing
            // canister of something that should not be there.
            crate_box(
                m,
                DVec3::new(-0.34, 0.0, -0.26),
                DVec3::new(0.34, 0.34, 0.26),
                rgb(190, 148, 96),
            );
            m.block(
                Mat::Paint(rgb(214, 190, 120)),
                DVec3::new(-0.3, 0.33, -0.22),
                DVec3::new(0.3, 0.36, 0.22),
            );
            m.lathe(
                Mat::Glass(rgb(120, 255, 130)),
                DVec3::new(0.0, 0.36, 0.0),
                &[
                    (0.0, 0.0),
                    (0.09, 0.0),
                    (0.09, 0.26),
                    (0.06, 0.3),
                    (0.0, 0.3),
                ],
            );
            m.lathe(
                Mat::Glow(rgb(130, 255, 120)),
                DVec3::new(0.0, 0.4, 0.0),
                &[(0.0, 0.0), (0.075, 0.0), (0.075, 0.17), (0.0, 0.17)],
            );
            let mut lid = m.sub(DVec3::new(0.46, 0.0, 0.14), 0.5);
            lid.block(
                Mat::Paint(rgb(180, 138, 88)),
                DVec3::new(-0.3, 0.0, -0.22),
                DVec3::new(0.3, 0.035, 0.22),
            );
        }
        K::Prototype => {
            // A prototype ray gun in its case, on blue velvet, its coil lit.
            m.block(
                Mat::Paint(rgb(44, 48, 60)),
                DVec3::new(-0.36, 0.0, -0.22),
                DVec3::new(0.36, 0.12, 0.22),
            );
            m.block(
                Mat::Cloth(rgb(30, 50, 120)),
                DVec3::new(-0.32, 0.1, -0.18),
                DVec3::new(0.32, 0.125, 0.18),
            );
            casing(
                m,
                Mat::Chrome,
                DVec3::new(-0.26, 0.125, -0.05),
                DVec3::new(0.1, 0.2, 0.07),
            );
            m.blob(
                Mat::Paint(rgb(200, 50, 40)),
                DVec3::new(-0.18, 0.2, 0.1),
                DVec3::new(0.05, 0.08, 0.04),
                -0.3,
                (0.5, 0.5),
            );
            m.lathe_along(
                Mat::Glow(rgb(120, 220, 255)),
                DVec3::new(0.1, 0.165, 0.0),
                DVec3::X,
                &[(0.0, 0.0), (0.035, 0.0), (0.035, 0.18), (0.0, 0.2)],
            );
        }
        K::Art => {
            // A gilt-framed landscape propped on two blocks, a lit plaque.
            for x in [-0.22, 0.22] {
                m.block(
                    Mat::Wood(rgb(120, 84, 52)),
                    DVec3::new(x - 0.04, 0.0, -0.06),
                    DVec3::new(x + 0.04, 0.1, 0.06),
                );
            }
            let mut f = m.sub(DVec3::new(0.0, 0.1, 0.0), 0.0);
            f.blob(
                Mat::Brass,
                DVec3::new(0.0, 0.3, 0.0),
                DVec3::new(0.36, 0.3, 0.025),
                0.0,
                (0.2, 0.2),
            );
            f.block(
                Mat::Paint(rgb(118, 176, 220)),
                DVec3::new(-0.3, 0.06, 0.02),
                DVec3::new(0.3, 0.54, 0.03),
            );
            f.block(
                Mat::Paint(rgb(86, 140, 84)),
                DVec3::new(-0.3, 0.06, 0.03),
                DVec3::new(0.3, 0.22, 0.035),
            );
            f.blob(
                Mat::Plain(rgb(255, 230, 120)),
                DVec3::new(0.18, 0.43, 0.04),
                DVec3::new(0.05, 0.05, 0.01),
                0.0,
                (1.0, 1.0),
            );
            f.block(
                glint,
                DVec3::new(-0.08, 0.02, 0.025),
                DVec3::new(0.08, 0.05, 0.03),
            );
        }
        K::Racket => {
            // A fat black ledger with a brass clasp, a pencil, bills tucked in.
            m.block(
                Mat::Paint(rgb(40, 36, 40)),
                DVec3::new(-0.24, 0.0, -0.17),
                DVec3::new(0.24, 0.1, 0.17),
            );
            m.block(
                Mat::Paint(rgb(236, 228, 204)),
                DVec3::new(-0.22, 0.02, -0.15),
                DVec3::new(0.23, 0.08, 0.16),
            );
            m.block(
                Mat::Paint(rgb(138, 178, 112)),
                DVec3::new(0.1, 0.095, -0.1),
                DVec3::new(0.26, 0.11, 0.0),
            );
            m.tube(
                Mat::Plain(rgb(220, 180, 40)),
                &[DVec3::new(-0.2, 0.11, 0.22), DVec3::new(0.2, 0.11, 0.2)],
                0.012,
            );
            m.block(
                glint,
                DVec3::new(-0.04, 0.05, 0.17),
                DVec3::new(0.04, 0.09, 0.18),
            );
        }
    }
}

/// An exhibit's round plinth (0.56 high) with a brass band and a lit
/// label at the front.
fn plinth(m: &mut Maker) {
    m.lathe(
        Mat::Paint(rgb(238, 230, 212)),
        DVec3::ZERO,
        &[
            (0.78, 0.0),
            (0.78, 0.07),
            (0.7, 0.11),
            (0.7, 0.48),
            (0.74, 0.52),
            (0.74, 0.56),
            (0.0, 0.56),
        ],
    );
    m.lathe(
        Mat::Brass,
        DVec3::new(0.0, 0.38, 0.0),
        &[(0.7, 0.0), (0.712, 0.005), (0.712, 0.045), (0.7, 0.05)],
    );
    m.blob(
        Mat::Glow(rgb(255, 226, 170)),
        DVec3::new(0.0, 0.24, 0.7),
        DVec3::new(0.2, 0.07, 0.012),
        0.0,
        (0.3, 0.3),
    );
}

/// A model rocket on a plinth (3 m high): a chrome hull on three red
/// fins, a red nose, a lit porthole, a glow in the nozzle.
pub fn rocket_model(m: &mut Maker) {
    plinth(m);
    let red = Mat::Paint(rgb(206, 52, 40));
    m.lathe(
        Mat::Chrome,
        DVec3::new(0.0, 0.92, 0.0),
        &[
            (0.1, 0.0),
            (0.17, 0.12),
            (0.23, 0.4),
            (0.25, 0.75),
            (0.23, 1.15),
            (0.17, 1.5),
        ],
    );
    m.lathe(
        red,
        DVec3::new(0.0, 2.42, 0.0),
        &[
            (0.17, 0.0),
            (0.12, 0.2),
            (0.06, 0.4),
            (0.015, 0.54),
            (0.0, 0.56),
        ],
    );
    m.lathe(
        Mat::Brass,
        DVec3::new(0.0, 2.4, 0.0),
        &[(0.172, 0.0), (0.178, 0.01), (0.178, 0.03), (0.17, 0.04)],
    );
    // The nozzle and its glow.
    m.lathe(
        cast_iron(),
        DVec3::new(0.0, 0.78, 0.0),
        &[(0.13, 0.0), (0.09, 0.08), (0.1, 0.16)],
    );
    m.lathe(
        Mat::Glow(rgb(255, 170, 70)),
        DVec3::new(0.0, 0.8, 0.0),
        &[(0.0, 0.0), (0.11, 0.0), (0.07, 0.07), (0.0, 0.1)],
    );
    // Three swept fins it stands on.
    for k in 0..3 {
        let mut f = m.sub(DVec3::ZERO, TAU * k as f64 / 3.0);
        let lean = 0.32f64;
        let (sl, cl) = lean.sin_cos();
        f.blob_axes(
            red,
            DVec3::new(0.0, 1.0, 0.36),
            [DVec3::X, DVec3::new(0.0, cl, -sl), DVec3::new(0.0, sl, cl)],
            DVec3::new(0.022, 0.46, 0.17),
            (1.0, 0.7),
        );
        f.blob(
            Mat::Chrome,
            DVec3::new(0.0, 0.6, 0.5),
            DVec3::new(0.035, 0.05, 0.035),
            0.0,
            (1.0, 1.0),
        );
    }
    // Portholes up the front.
    for (y, z) in [(1.66, 0.246), (2.02, 0.232)] {
        m.lathe_along(
            Mat::Brass,
            DVec3::new(0.0, y, z - 0.012),
            DVec3::Z,
            &[(0.062, 0.0), (0.062, 0.018), (0.045, 0.024), (0.045, 0.0)],
        );
        m.lathe_along(
            Mat::Glow(rgb(150, 230, 255)),
            DVec3::new(0.0, y, z - 0.012),
            DVec3::Z,
            &[(0.0, 0.0), (0.045, 0.0), (0.045, 0.014), (0.0, 0.02)],
        );
    }
}

/// A ring of radius `r` about `c` in the plane of the unit axes `u`, `v`.
fn ring(c: DVec3, u: DVec3, v: DVec3, r: f64, n: usize) -> Vec<DVec3> {
    (0..=n)
        .map(|k| {
            let (s, co) = (TAU * k as f64 / n as f64).sin_cos();
            c + (u * co + v * s) * r
        })
        .collect()
}

/// A great model of the atom on a plinth (2.4 m high): a nucleus of red
/// and cream balls, three chrome orbits with glowing electrons.
pub fn atom_model(m: &mut Maker) {
    plinth(m);
    let c = DVec3::new(0.0, 1.68, 0.0);
    m.lathe(
        Mat::Chrome,
        DVec3::new(0.0, 0.56, 0.0),
        &[(0.12, 0.0), (0.1, 0.04), (0.03, 0.1), (0.03, 0.96)],
    );
    let balls = [
        (0.0, 0.0, 0.0),
        (0.1, 0.05, 0.03),
        (-0.09, 0.06, -0.04),
        (0.02, -0.09, 0.08),
        (-0.03, 0.1, 0.09),
        (0.04, -0.04, -0.11),
        (-0.08, -0.07, 0.02),
    ];
    for (k, (x, y, z)) in balls.into_iter().enumerate() {
        let mat = if k % 2 == 0 {
            Mat::Paint(rgb(206, 52, 40))
        } else {
            Mat::Paint(rgb(238, 230, 212))
        };
        m.blob(
            mat,
            c + DVec3::new(x, y, z),
            DVec3::splat(0.09),
            0.0,
            (1.0, 1.0),
        );
    }
    for k in 0..3 {
        let a = TAU * k as f64 / 3.0;
        let (s, co) = a.sin_cos();
        // Each orbit leans 60 degrees, turned a third round from the last.
        let u = DVec3::new(co, 0.0, s);
        let v = DVec3::new(-s * 0.5, 0.866, co * 0.5);
        m.tube(Mat::Chrome, &ring(c, u, v, 0.62, 40), 0.012);
        let (es, ec) = (1.1 + k as f64 * 1.9).sin_cos();
        m.blob(
            Mat::Glow(rgb(120, 230, 255)),
            c + (u * ec + v * es) * 0.62,
            DVec3::splat(0.055),
            0.0,
            (1.0, 1.0),
        );
    }
}

/// The world as a globe on a plinth (2.2 m high): blue seas and green
/// lands on a leaning brass axis in a meridian ring, two moons on arms.
pub fn orrery(m: &mut Maker) {
    plinth(m);
    let c = DVec3::new(0.0, 1.5, 0.0);
    let tilt = 0.41f64;
    let (st, ct) = tilt.sin_cos();
    let axis = DVec3::new(st, ct, 0.0);
    m.lathe(
        Mat::Brass,
        DVec3::new(0.0, 0.56, 0.0),
        &[(0.16, 0.0), (0.12, 0.05), (0.035, 0.12), (0.035, 0.42)],
    );
    m.blob(
        Mat::Paint(rgb(52, 110, 170)),
        c,
        DVec3::splat(0.42),
        0.0,
        (1.0, 1.0),
    );
    // Lands: low caps that stand a little proud of the sea.
    let lands = [
        (0.5, 0.6, 0.2, 0.2),
        (2.1, 0.2, 0.17, 0.24),
        (3.6, -0.3, 0.22, 0.16),
        (4.9, 0.5, 0.14, 0.14),
        (1.2, -0.6, 0.16, 0.12),
        (5.6, -0.5, 0.12, 0.18),
    ];
    for (lon, lat, a, b) in lands {
        let (sl, cl): (f64, f64) = f64::sin_cos(lon);
        let (sp, cp): (f64, f64) = f64::sin_cos(lat);
        let n = DVec3::new(cp * cl, sp, cp * sl);
        let e = DVec3::new(-sl, 0.0, cl);
        let up = n.cross(e);
        m.blob_axes(
            Mat::Paint(rgb(96, 150, 84)),
            c + n * 0.385,
            [e, n, up],
            DVec3::new(a, 0.045, b),
            (1.0, 1.0),
        );
    }
    // The axis with its finials, and the meridian ring it turns in.
    m.tube(Mat::Brass, &[c - axis * 0.56, c + axis * 0.56], 0.014);
    for d in [-1.0, 1.0] {
        m.blob(
            Mat::Brass,
            c + axis * 0.56 * d,
            DVec3::splat(0.035),
            0.0,
            (1.0, 1.0),
        );
    }
    m.tube(
        Mat::Brass,
        &ring(c, axis, DVec3::new(-ct, st, 0.0), 0.52, 40),
        0.016,
    );
    // Two moons on chrome arms from the foot of the axis.
    for (a, r, y, size, col) in [
        (0.9f64, 0.64, 0.28, 0.07, rgb(236, 230, 214)),
        (3.7f64, 0.7, -0.1, 0.05, rgb(214, 120, 90)),
    ] {
        let (s, co) = a.sin_cos();
        let at = c + DVec3::new(co * r, y, s * r);
        let foot = DVec3::new(0.0, 0.94, 0.0);
        m.tube(
            Mat::Chrome,
            &[foot, DVec3::new(co * r, 0.94, s * r), at],
            0.008,
        );
        m.blob(Mat::Paint(col), at, DVec3::splat(size), 0.0, (1.0, 1.0));
    }
}

/// A night club's stage, `w` wide, its back to the wall: a platform with a
/// bowed front, a chrome band and footlights, a pleated curtain behind,
/// and the band's stand on it (`hw_gen::layout::furnish::STAGE_DEPTH`
/// deep and `STAGE_HEIGHT` high).
pub fn stage(m: &mut Maker, w: f64) {
    use hw_gen::layout::furnish::{STAGE_DEPTH, STAGE_HEIGHT};
    let (hw, hd, h) = (w * 0.5, STAGE_DEPTH * 0.5, STAGE_HEIGHT);
    // The platform: straight at the back, bowed out at the front.
    let n = 24;
    let bow = 0.5;
    let mut pts = vec![DVec2::new(hw, -hd), DVec2::new(-hw, -hd)];
    pts.extend((0..=n).map(|k| {
        let t = k as f64 / n as f64;
        let x = -hw + w * t;
        DVec2::new(x, hd - bow + bow * (PI * t).sin())
    }));
    m.slab(Mat::Wood(rgb(70, 40, 26)), &pts, 0.0, h - 0.04);
    let top: Vec<DVec2> = pts.iter().map(|p| *p * 0.985).collect();
    m.slab(Mat::Wood(rgb(150, 100, 60)), &top, h - 0.04, h);
    // Footlights along the bow.
    let lights = (w / 0.9).floor().max(2.0) as usize;
    for k in 0..lights {
        let t = (k as f64 + 0.5) / lights as f64;
        let x = -hw + w * t;
        let z = hd - bow + bow * (PI * t).sin() - 0.18;
        m.lathe(
            Mat::Chrome,
            DVec3::new(x, h, z),
            &[(0.09, 0.0), (0.1, 0.05), (0.07, 0.11), (0.0, 0.12)],
        );
        m.lathe(
            Mat::Glow(rgb(255, 214, 150)),
            DVec3::new(x, h + 0.1, z),
            &[(0.05, 0.0), (0.04, 0.03), (0.0, 0.04)],
        );
    }
    // The curtain: pleats of deep red velvet along the back, a gold pelmet.
    let pleats = (w / 0.32).round().max(4.0) as usize;
    let top_y = 3.3;
    for k in 0..pleats {
        let x = -hw + (k as f64 + 0.5) * w / pleats as f64;
        let r = w / pleats as f64 * 0.52;
        m.lathe(
            Mat::Cloth(rgb(120, 22, 40)),
            DVec3::new(x, h, -hd + 0.16),
            &[(r, 0.0), (r * 0.9, top_y - h), (0.0, top_y - h)],
        );
    }
    m.block(
        Mat::Paint(rgb(196, 160, 70)),
        DVec3::new(-hw, top_y - 0.1, -hd + 0.02),
        DVec3::new(hw, top_y + 0.25, -hd + 0.36),
    );
    // The band's stand in the middle, a second microphone to one side.
    let mut band = m.sub(DVec3::new(0.0, h, -0.3), 0.0);
    bandstand(&mut band);
    if w >= 6.0 {
        for s in [-1.0, 1.0] {
            m.lathe(
                Mat::Chrome,
                DVec3::new(s * (hw - 1.2), h, 0.3),
                &[
                    (0.12, 0.0),
                    (0.1, 0.02),
                    (0.012, 0.03),
                    (0.012, 1.45),
                    (0.0, 1.45),
                ],
            );
            m.blob(
                Mat::Chrome,
                DVec3::new(s * (hw - 1.2), 1.52 + h, 0.3),
                DVec3::new(0.05, 0.07, 0.04),
                0.0,
                (0.6, 0.6),
            );
        }
    }
}

/// A diner booth: two padded benches facing each other across a table,
/// `w` long (across the booth).
pub fn booth(m: &mut Maker, cover: Color, top: Color) {
    for s in [-1.0, 1.0] {
        let z = s * 0.62;
        m.block(
            Mat::Plain(rgb(200, 200, 204)),
            DVec3::new(-0.6, 0.0, z - 0.25),
            DVec3::new(0.6, 0.38, z + 0.25),
        );
        m.blob(
            Mat::Plain(cover),
            DVec3::new(0.0, 0.43, z - s * 0.02),
            DVec3::new(0.6, 0.07, 0.24),
            0.0,
            (0.2, 0.35),
        );
        m.blob(
            Mat::Plain(cover),
            DVec3::new(0.0, 0.85, z + s * 0.2),
            DVec3::new(0.6, 0.38, 0.08),
            -s * 0.12,
            (0.2, 0.35),
        );
    }
    m.slab(Mat::Plain(top), &rrect(1.1, 0.7, 0.15, 3), 0.72, 0.75);
    m.slab(Mat::Chrome, &rrect(1.12, 0.72, 0.16, 3), 0.69, 0.72);
    m.lathe(
        Mat::Chrome,
        DVec3::ZERO,
        &[
            (0.2, 0.0),
            (0.2, 0.02),
            (0.04, 0.04),
            (0.04, 0.69),
            (0.0, 0.69),
        ],
    );
}

// ------------------------------------------------------------------ rugs

/// A rug of outline `pts` (local, around 0) with a border band `bw` wide
/// in `border`; lies 12 mm high on the floor.
pub fn rug(m: &mut Maker, pts: &[DVec2], field: Color, border: Color, bw: f64) {
    let h = 0.012;
    let n = pts.len();
    if n < 3 {
        return;
    }
    let mid = pts.iter().copied().sum::<DVec2>() / n as f64;
    let inner: Vec<DVec2> = pts
        .iter()
        .map(|&q| {
            let d = q - mid;
            let l = d.length();
            if l < 1e-9 {
                q
            } else {
                mid + d * ((l - bw).max(0.0) / l)
            }
        })
        .collect();
    let at = |q: DVec2| DVec3::new(q.x, h, q.y);
    let cf = m.mat(Mat::Cloth(field));
    for i in 0..n {
        let j = (i + 1) % n;
        let t = [m.p(at(mid)), m.p(at(inner[i])), m.p(at(inner[j]))];
        m.b.tri(t, DVec3::Y, cf);
    }
    let cb = m.mat(Mat::Cloth(border));
    for i in 0..n {
        let j = (i + 1) % n;
        let q = [
            m.p(at(inner[i])),
            m.p(at(pts[i])),
            m.p(at(pts[j])),
            m.p(at(inner[j])),
        ];
        m.b.quad(q, DVec3::Y, cb);
        // The rug's edge.
        let e = [
            m.p(DVec3::new(pts[i].x, 0.0, pts[i].y)),
            m.p(DVec3::new(pts[j].x, 0.0, pts[j].y)),
            m.p(at(pts[j])),
            m.p(at(pts[i])),
        ];
        let out = (pts[i] + pts[j]) * 0.5 - mid;
        let out = m.d(DVec3::new(out.x, 0.0, out.y));
        m.b.quad(e, out, cb);
    }
    m.reset();
}

// ------------------------------------------------------------------ helpers

/// `c` lighter (k > 1) or darker.
pub fn scale(c: Color, k: f32) -> Color {
    Color {
        r: (c.r * k).min(1.0),
        g: (c.g * k).min(1.0),
        b: (c.b * k).min(1.0),
        a: c.a,
    }
}

/// A small seeded random sequence (SplitMix64).
pub struct Rng(pub u64);

impl Rng {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// In [0, 1).
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zfight::{city_options, ZFightCheck};

    /// Each piece on its own: no two differently coloured faces of it
    /// fight (the city check finds them too, but cannot say which piece).
    #[test]
    #[ignore = "on demand (looks): scripts/check.sh --on-demand"]
    fn pieces_have_no_z_fighting() {
        let cov = || Cover {
            main: rgb(180, 60, 50),
            second: rgb(230, 200, 120),
            wood: rgb(120, 80, 50),
        };
        let wood = rgb(140, 96, 60);
        let steel = rgb(140, 150, 160);
        type Build = Box<dyn Fn(&mut Maker)>;
        let pieces: Vec<(&str, Build)> = vec![
            ("sofa", Box::new(move |m| sofa(m, 2.0, &cov()))),
            ("armchair", Box::new(move |m| armchair(m, &cov()))),
            ("chair", Box::new(move |m| chair(m, &cov()))),
            ("swivel_chair", Box::new(move |m| swivel_chair(m, &cov()))),
            ("stool", Box::new(|m| stool(m, rgb(200, 50, 50)))),
            (
                "coffee_table",
                Box::new(move |m| {
                    let legs = [DVec2::new(-0.4, -0.2), DVec2::new(0.4, 0.2)];
                    coffee_table(m, &kidney(1.1, 0.6, 24), wood, &legs)
                }),
            ),
            (
                "dining_table",
                Box::new(move |m| dining_table(m, 1.4, 0.8, wood)),
            ),
            (
                "sideboard",
                Box::new(move |m| sideboard(m, 1.6, wood, rgb(60, 140, 160))),
            ),
            ("wardrobe", Box::new(move |m| wardrobe(m, 1.2, wood))),
            (
                "nightstand",
                Box::new(move |m| nightstand(m, wood, 3, rgb(240, 220, 180))),
            ),
            (
                "bookcase",
                Box::new(move |m| bookcase(m, 1.0, 1.9, wood, 7)),
            ),
            (
                "filing_cabinet",
                Box::new(move |m| filing_cabinet(m, steel)),
            ),
            ("bed", Box::new(move |m| bed(m, 1.6, &cov()))),
            (
                "counter",
                Box::new(|m| counter(m, 2.4, rgb(220, 230, 210), Some(0.3))),
            ),
            (
                "wall_cupboards",
                Box::new(|m| wall_cupboards(m, 2.0, rgb(220, 230, 210))),
            ),
            ("fridge", Box::new(|m| fridge(m, rgb(190, 226, 214)))),
            ("stove", Box::new(|m| stove(m, rgb(240, 200, 190)))),
            (
                "table_lamp",
                Box::new(|m| table_lamp(m, 3, rgb(240, 220, 180), rgb(60, 60, 60))),
            ),
            (
                "floor_lamp",
                Box::new(|m| floor_lamp(m, 3, rgb(240, 220, 180))),
            ),
            ("desk_lamp", Box::new(|m| desk_lamp(m, 3))),
            ("radio", Box::new(move |m| radio(m, wood))),
            ("television", Box::new(move |m| television(m, wood, 3))),
            (
                "desk",
                Box::new(move |m| desk(m, steel, rgb(60, 120, 100), 3, 5)),
            ),
            ("water_cooler", Box::new(water_cooler)),
            ("coat_stand", Box::new(|m| coat_stand(m, rgb(90, 60, 50)))),
            ("plant", Box::new(|m| plant(m, 1.2, rgb(180, 90, 60), 3))),
            (
                "shop_counter",
                Box::new(|m| shop_counter(m, 2.2, rgb(236, 232, 220), rgb(200, 50, 50))),
            ),
            (
                "gondola",
                Box::new(|m| gondola(m, 1.8, rgb(220, 214, 200), 4, true)),
            ),
            ("clothes_rail", Box::new(|m| clothes_rail(m, 1.8, 4))),
            ("dress_form", Box::new(|m| dress_form(m, rgb(60, 90, 160)))),
            (
                "display_case",
                Box::new(move |m| display_case(m, 1.6, wood, 4)),
            ),
            ("jukebox", Box::new(jukebox)),
            (
                "booth",
                Box::new(|m| booth(m, rgb(196, 50, 56), rgb(240, 236, 226))),
            ),
        ];
        let mut pieces = pieces;
        for k in 0..6u64 {
            pieces.push((
                "desk seeded",
                Box::new(move |m| desk(m, steel, rgb(60, 120, 100), 3, k)),
            ));
            pieces.push((
                "bookcase seeded",
                Box::new(move |m| bookcase(m, 1.0, 1.9, wood, k)),
            ));
            pieces.push((
                "gondola seeded",
                Box::new(move |m| gondola(m, 1.6, rgb(220, 214, 200), k, false)),
            ));
            pieces.push((
                "clothes_rail seeded",
                Box::new(move |m| clothes_rail(m, 1.8, k)),
            ));
            pieces.push((
                "display_case seeded",
                Box::new(move |m| display_case(m, 1.6, wood, k)),
            ));
            pieces.push((
                "plant seeded",
                Box::new(move |m| plant(m, 1.0, rgb(180, 90, 60), k)),
            ));
        }
        let mut bad = Vec::new();
        for (name, build) in &pieces {
            let mut b = MeshBuilder::new();
            {
                let mut m = Maker::new(&mut b, DVec3::ZERO, DVec3::Z, 24, Wear::Fresh);
                build(&mut m);
            }
            let report = ZFightCheck::new()
                .options(city_options())
                .with(name.to_string(), &b.finish())
                .run();
            if !report.is_clean() {
                bad.push(format!("{name}: {}", report.summary(4)));
            }
        }
        assert!(bad.is_empty(), "{}", bad.join("\n"));
    }
}
