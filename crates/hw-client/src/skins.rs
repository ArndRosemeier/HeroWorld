//! Surface textures beyond Engine's stock layers: worn and dirty versions
//! of its walls, roofs and ground, and a few layers of our own (enamel
//! panels, cast stone, painted wood).
//!
//! A [`Tex`] names a texture: an Engine layer as it comes, or a [`Skin`]
//! (a base and how worn it is). [`texture`] makes each one once, at Engine's
//! layer size, and shares it; Engine shares the GPU copy by content, so a
//! texture costs the same however many meshes use it.
//!
//! Wear is baked into the tile: mottled discolouring, rain streaks, hairline
//! cracks, soot, flaking paint and plaster that has come away from the
//! brick, lichen on roofs. What depends on where a face is on a building
//! (dirt splashed up from the street, streaks under the sills) comes from
//! vertex colours instead (`citymesh::Weather`), so it does not repeat.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use engine::texgen::{
    self, FacadeLayer, GroundLayer, Layer, MedievalLayer, RoofLayer, TextureLayer, TEX_SIZE,
};

/// A texture: an Engine layer, or one of ours.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tex {
    Layer(Layer),
    Skin(Skin),
}

impl From<Layer> for Tex {
    fn from(l: Layer) -> Self {
        Tex::Layer(l)
    }
}

impl From<Skin> for Tex {
    fn from(s: Skin) -> Self {
        Tex::Skin(s)
    }
}

macro_rules! from_layer {
    ($($t:ty),*) => {$(
        impl From<$t> for Tex {
            fn from(l: $t) -> Self {
                Tex::Layer(l.into())
            }
        }
    )*};
}
from_layer!(FacadeLayer, GroundLayer, RoofLayer, MedievalLayer);

/// How worn a surface is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Wear {
    /// New or freshly kept.
    Fresh,
    /// A few decades: discoloured, a little streaked.
    Aged,
    /// Old and neglected: cracks, stains, flaking.
    Worn,
    /// Soot and filth: the works, the dystopia.
    Grimy,
}

impl Wear {
    pub const ALL: [Wear; 4] = [Wear::Fresh, Wear::Aged, Wear::Worn, Wear::Grimy];

    /// 0 (fresh) to 1 (grimy).
    pub fn amount(self) -> f32 {
        match self {
            Wear::Fresh => 0.0,
            Wear::Aged => 0.35,
            Wear::Worn => 0.7,
            Wear::Grimy => 1.0,
        }
    }

    /// One step more (`d` > 0) or less worn.
    pub fn step(self, d: i32) -> Wear {
        let i = (self as i32 + d).clamp(0, 3);
        Wear::ALL[i as usize]
    }
}

/// The surface under the wear.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Base {
    Facade(FacadeLayer),
    Roof(RoofLayer),
    Medieval(MedievalLayer),
    Ground(GroundLayer),
    /// Porcelain-enamel panels on a grid, glossy (near white: tint it).
    /// The cladding of Googie and streamline buildings.
    Enamel,
    /// Fine cast stone without joints, for sills, frames and cornices
    /// (near white: tint it).
    CastStone,
    /// Painted wood, for frames and trim on wooden and old houses (near
    /// white: tint it).
    PaintedWood,
    /// Woven upholstery (near white: tint it).
    Fabric,
    /// Wallpaper in a pattern (near white: tint it).
    Wallpaper(Paper),
    /// Polished wood veneer, grain along u (tint it with the wood).
    Veneer,
    /// Block parquet in a basket weave (tint it with the wood).
    Parquet,
    /// Linoleum in a chequerboard, glossy (black and near white: tint the
    /// light squares).
    Lino,
    /// Terrazzo: stone chips in polished cement (near white: tint it).
    Terrazzo,
}

/// Wallpaper patterns of the golden age. On walls that do not run along
/// the town's axes Engine's triplanar mapping blends two projections, so
/// every pattern is mostly vertical: it stays clean when blended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Paper {
    /// Broad stripes of two tones.
    Stripes,
    /// Fine pinstripes on a plain ground.
    Pinstripe,
    /// Columns of ogee damask flowers, a sheen against matt.
    Damask,
    /// Thin double lines with little atomic stars between them.
    Atomic,
}

impl Paper {
    pub const ALL: [Paper; 4] = [
        Paper::Stripes,
        Paper::Pinstripe,
        Paper::Damask,
        Paper::Atomic,
    ];
}

/// A base and its wear.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Skin {
    pub base: Base,
    pub wear: Wear,
}

impl Skin {
    pub fn new(base: Base, wear: Wear) -> Self {
        Skin { base, wear }
    }
}

/// The texture for `t`, made once per process.
pub fn texture(t: Tex) -> Arc<TextureLayer> {
    match t {
        Tex::Layer(l) => texgen::layer(l),
        Tex::Skin(s) => skin(s),
    }
}

fn skin(s: Skin) -> Arc<TextureLayer> {
    static CACHE: OnceLock<Mutex<HashMap<Skin, Arc<TextureLayer>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(t) = cache.lock().expect("skins").get(&s) {
        return t.clone();
    }
    // Made outside the lock: two threads may both make one, the same.
    let t = Arc::new(make(s, TEX_SIZE));
    cache
        .lock()
        .expect("skins")
        .entry(s)
        .or_insert_with(|| t.clone())
        .clone()
}

/// What kind of surface a base is, for the wear it takes.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// Plaster, stucco, render, limewash: cracks, and it comes away.
    Render,
    Concrete,
    /// Brick and stone: soot and salt bloom.
    Masonry,
    Wood,
    Metal,
    Roof,
    Ground,
}

fn kind(b: Base) -> Kind {
    use FacadeLayer as F;
    match b {
        Base::Facade(F::Plaster | F::Stucco) | Base::Medieval(MedievalLayer::Limewash) => {
            Kind::Render
        }
        Base::Facade(F::Concrete | F::ConcretePanel) => Kind::Concrete,
        Base::Facade(F::WoodSiding | F::Timber)
        | Base::PaintedWood
        | Base::Fabric
        | Base::Veneer
        | Base::Parquet
        | Base::Wallpaper(_) => Kind::Wood,
        Base::Facade(F::MetalPanel | F::GlassCurtain) | Base::Enamel => Kind::Metal,
        Base::Facade(_) | Base::Medieval(_) | Base::CastStone => Kind::Masonry,
        Base::Roof(_) => Kind::Roof,
        Base::Ground(_) | Base::Lino | Base::Terrazzo => Kind::Ground,
    }
}

/// Makes skin `s` at `size` pixels.
pub fn make(s: Skin, size: u32) -> TextureLayer {
    let mut t = match s.base {
        Base::Facade(l) => l.generate(size),
        Base::Roof(l) => l.generate(size),
        Base::Medieval(l) => l.generate(size),
        Base::Ground(l) => l.generate(size),
        Base::Enamel => enamel(size),
        Base::CastStone => cast_stone(size),
        Base::PaintedWood => painted_wood(size),
        Base::Fabric => fabric(size),
        Base::Wallpaper(p) => wallpaper(p, size),
        Base::Veneer => veneer(size),
        Base::Parquet => parquet(size),
        Base::Lino => lino(size),
        Base::Terrazzo => terrazzo(size),
    };
    weather(&mut t, s);
    t
}

// ---------------------------------------------------------------- noise

/// A hash of integers to [0, 1).
fn hash(seed: u64, x: i64, y: i64) -> f32 {
    let mut h = seed ^ 0x9E37_79B9_7F4A_7C15;
    for v in [x as u64, y as u64] {
        h ^= v.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h = h.rotate_left(31).wrapping_mul(0x94D0_49BB_1331_11EB);
    }
    h ^= h >> 29;
    (h >> 40) as f32 / (1u64 << 24) as f32
}

/// Value noise that tiles: `period` lattice cells across the tile, at
/// tile coordinates u, v in [0, 1).
fn value(seed: u64, period: i64, u: f32, v: f32) -> f32 {
    let (x, y) = (u * period as f32, v * period as f32);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (s(fx), s(fy));
    let w = |i: i64| i.rem_euclid(period);
    let (i0, j0) = (x0 as i64, y0 as i64);
    let h = |i: i64, j: i64| hash(seed, w(i), w(j));
    let a = h(i0, j0) + (h(i0 + 1, j0) - h(i0, j0)) * sx;
    let b = h(i0, j0 + 1) + (h(i0 + 1, j0 + 1) - h(i0, j0 + 1)) * sx;
    a + (b - a) * sy
}

/// Fractal noise in [0, 1): `octaves` layers from `period` up.
fn fbm(seed: u64, period: i64, octaves: u32, u: f32, v: f32) -> f32 {
    let (mut sum, mut amp, mut norm, mut p) = (0.0, 1.0, 0.0, period);
    for o in 0..octaves {
        sum += value(seed.wrapping_add(o as u64 * 7919), p, u, v) * amp;
        norm += amp;
        amp *= 0.5;
        p *= 2;
    }
    sum / norm
}

/// Stretched noise: `px` cells across, `py` down.
fn value2(seed: u64, px: i64, py: i64, u: f32, v: f32) -> f32 {
    let (x, y) = (u * px as f32, v * py as f32);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (s(fx), s(fy));
    let (i0, j0) = (x0 as i64, y0 as i64);
    let h = |i: i64, j: i64| hash(seed, i.rem_euclid(px), j.rem_euclid(py));
    let a = h(i0, j0) + (h(i0 + 1, j0) - h(i0, j0)) * sx;
    let b = h(i0, j0 + 1) + (h(i0 + 1, j0 + 1) - h(i0, j0 + 1)) * sx;
    a + (b - a) * sy
}

/// Blends `c` towards `to` by `t`.
fn mix(c: &mut [f32; 3], to: [f32; 3], t: f32) {
    for (ch, v) in c.iter_mut().zip(to) {
        *ch = *ch * (1.0 - t) + v * t;
    }
}

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// ---------------------------------------------------------------- maps

/// A layer's maps as floats to work on, then packed back.
struct Work {
    n: usize,
    /// Linear-ish 0..1 albedo (kept in sRGB, as stored).
    rgb: Vec<[f32; 3]>,
    /// Relief height 0..1.
    height: Vec<f32>,
    /// Height before our changes, to add only our relief to the normals.
    height0: Vec<f32>,
    occlusion: Vec<f32>,
    rough: Vec<f32>,
}

impl Work {
    fn of(t: &TextureLayer) -> Work {
        let n = t.size as usize;
        let px = n * n;
        let mut w = Work {
            n,
            rgb: Vec::with_capacity(px),
            height: Vec::with_capacity(px),
            height0: Vec::new(),
            occlusion: Vec::with_capacity(px),
            rough: Vec::with_capacity(px),
        };
        for i in 0..px {
            let a = &t.albedo[i * 4..i * 4 + 3];
            w.rgb.push([
                a[0] as f32 / 255.0,
                a[1] as f32 / 255.0,
                a[2] as f32 / 255.0,
            ]);
            w.height.push(t.normal[i * 4 + 3] as f32 / 255.0);
            w.occlusion.push(t.orm[i * 4] as f32 / 255.0);
            w.rough.push(t.orm[i * 4 + 1] as f32 / 255.0);
        }
        w.height0 = w.height.clone();
        w
    }

    fn idx(&self, x: i64, y: i64) -> usize {
        let n = self.n as i64;
        (y.rem_euclid(n) * n + x.rem_euclid(n)) as usize
    }

    /// Writes back, adding the slopes of our relief to the normal map.
    fn store(self, t: &mut TextureLayer, strength: f32) {
        let n = self.n as i64;
        for y in 0..n {
            for x in 0..n {
                let i = self.idx(x, y);
                let d = |xx: i64, yy: i64| {
                    let j = self.idx(xx, yy);
                    self.height[j] - self.height0[j]
                };
                // Slopes of our relief; +y is up the image (rows go down).
                let gx = (d(x + 1, y) - d(x - 1, y)) * 0.5 * strength;
                let gy = (d(x, y - 1) - d(x, y + 1)) * 0.5 * strength;
                let o = &t.normal[i * 4..i * 4 + 3];
                let mut nx = o[0] as f32 / 127.5 - 1.0 - gx;
                let mut ny = o[1] as f32 / 127.5 - 1.0 - gy;
                let mut nz = o[2] as f32 / 127.5 - 1.0;
                let l = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-6);
                nx /= l;
                ny /= l;
                nz /= l;
                let q = |v: f32| ((v * 0.5 + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8;
                t.normal[i * 4] = q(nx);
                t.normal[i * 4 + 1] = q(ny);
                t.normal[i * 4 + 2] = q(nz);
                t.normal[i * 4 + 3] = q(self.height[i] * 2.0 - 1.0);
                let c = self.rgb[i];
                for (k, ch) in c.iter().enumerate() {
                    t.albedo[i * 4 + k] = (ch * 255.0).round().clamp(0.0, 255.0) as u8;
                }
                t.orm[i * 4] = (self.occlusion[i] * 255.0).round().clamp(0.0, 255.0) as u8;
                t.orm[i * 4 + 1] = (self.rough[i] * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// A blank layer of `size`, flat and mid-height.
fn blank(size: u32, tile_metres: f32, normal_scale: f32) -> TextureLayer {
    let px = (size * size) as usize;
    TextureLayer {
        size,
        tile_metres,
        albedo: vec![255; px * 4],
        normal: [128u8, 128, 255, 128].repeat(px),
        orm: [255u8, 128, 0, 255].repeat(px),
        normal_scale,
    }
}

// ---------------------------------------------------------------- bases

/// Porcelain-enamel panels: 1.2 m wide, 0.6 m high, recessed joints,
/// a screw at each corner, a slight dish and orange-peel gloss.
fn enamel(size: u32) -> TextureLayer {
    let mut t = blank(size, 2.4, 1.0);
    let mut w = Work::of(&t);
    let n = w.n as f32;
    let (cols, rows) = (2.0, 4.0);
    for y in 0..w.n {
        for x in 0..w.n {
            let (u, v) = (x as f32 / n, y as f32 / n);
            let (pu, pv) = (u * cols, v * rows);
            let (cu, cv) = (pu.floor(), pv.floor());
            let (fu, fv) = (pu - cu, pv - cv);
            // Distance to the joint in metres (panel 1.2 x 0.6).
            let du = fu.min(1.0 - fu) * 1.2;
            let dv = fv.min(1.0 - fv) * 0.6;
            let edge = du.min(dv);
            let joint = 1.0 - smooth(0.004, 0.012, edge);
            let bevel = smooth(0.0, 0.04, edge);
            // Each panel a hair different in tone; orange peel all over.
            let tone = 0.95 + 0.04 * hash(11, cu as i64, cv as i64);
            let peel = (value(12, 96, u, v) - 0.5) * 0.012;
            // Screws 4 cm in from each corner.
            let sx = (du - 0.04).abs();
            let sy = (dv - 0.04).abs();
            let screw = 1.0 - smooth(0.004, 0.008, (sx * sx + sy * sy).sqrt());
            let i = w.idx(x as i64, y as i64);
            let g = (tone + peel) * (1.0 - 0.55 * joint) * (1.0 - 0.25 * screw);
            w.rgb[i] = [g, g, g];
            w.height[i] = 0.55 + 0.05 * bevel - 0.4 * joint + 0.1 * screw + peel;
            w.occlusion[i] = 1.0 - 0.5 * joint;
            w.rough[i] = 0.18 + 0.5 * joint + 0.1 * screw;
        }
    }
    w.store(&mut t, 60.0);
    t
}

/// Fine cast stone: no joints, a little grain and a few pores.
fn cast_stone(size: u32) -> TextureLayer {
    let mut t = blank(size, 2.0, 1.0);
    let mut w = Work::of(&t);
    let n = w.n as f32;
    for y in 0..w.n {
        for x in 0..w.n {
            let (u, v) = (x as f32 / n, y as f32 / n);
            let grain = fbm(21, 32, 4, u, v);
            let fine = value(22, 256, u, v);
            let pore = (hash(23, x as i64, y as i64) > 0.996) as u8 as f32;
            let i = w.idx(x as i64, y as i64);
            let g = 0.9 + (grain - 0.5) * 0.08 + (fine - 0.5) * 0.04 - pore * 0.25;
            w.rgb[i] = [g, g * 0.99, g * 0.96];
            w.height[i] = 0.5 + (fine - 0.5) * 0.1 + (grain - 0.5) * 0.1 - pore * 0.2;
            w.occlusion[i] = 1.0 - pore * 0.4;
            w.rough[i] = 0.72 + (fine - 0.5) * 0.1;
        }
    }
    w.store(&mut t, 30.0);
    t
}

/// Painted wood: grain along u under the paint, brush marks.
fn painted_wood(size: u32) -> TextureLayer {
    let mut t = blank(size, 1.6, 1.0);
    let mut w = Work::of(&t);
    let n = w.n as f32;
    for y in 0..w.n {
        for x in 0..w.n {
            let (u, v) = (x as f32 / n, y as f32 / n);
            let grain = value2(31, 4, 128, u, v) * 0.6 + value2(32, 8, 256, u, v) * 0.4;
            let brush = value2(33, 16, 512, u, v);
            let i = w.idx(x as i64, y as i64);
            let g = 0.93 + (grain - 0.5) * 0.05 + (brush - 0.5) * 0.02;
            w.rgb[i] = [g, g, g * 0.98];
            w.height[i] = 0.5 + (grain - 0.5) * 0.12 + (brush - 0.5) * 0.04;
            w.rough[i] = 0.5 + (brush - 0.5) * 0.1;
        }
    }
    w.store(&mut t, 40.0);
    t
}

/// Woven cloth: over-and-under threads, slubs, a soft sheen.
fn fabric(size: u32) -> TextureLayer {
    let mut t = blank(size, 0.4, 0.8);
    let mut w = Work::of(&t);
    let n = w.n as f32;
    let threads = (w.n / 4).max(4) as f32;
    for y in 0..w.n {
        for x in 0..w.n {
            let (u, v) = (x as f32 / n, y as f32 / n);
            let (tu, tv) = (u * threads, v * threads);
            let over = ((tu.floor() + tv.floor()) as i64).rem_euclid(2) == 0;
            let f = if over { tu.fract() } else { tv.fract() };
            let bump = (f * std::f32::consts::PI).sin();
            let slub = value2(41, 8, 64, u, v);
            let i = w.idx(x as i64, y as i64);
            let g = 0.86 + 0.08 * bump + (slub - 0.5) * 0.06;
            w.rgb[i] = [g, g, g];
            w.height[i] = 0.4 + 0.2 * bump;
            w.occlusion[i] = 0.85 + 0.15 * bump;
            w.rough[i] = 0.85;
        }
    }
    w.store(&mut t, 20.0);
    t
}

/// Wallpaper: a printed pattern on paper with a faint fibre, matt where
/// the ground is and with a little sheen on the print. A tile is 1.2 m
/// wide and high.
fn wallpaper(p: Paper, size: u32) -> TextureLayer {
    let mut t = blank(size, 1.2, 0.6);
    let mut w = Work::of(&t);
    let n = w.n as f32;
    for y in 0..w.n {
        for x in 0..w.n {
            let (u, v) = (x as f32 / n, y as f32 / n);
            let fibre = value(51, 128, u, v) * 0.6 + value(52, 256, u, v) * 0.4;
            // 0 ground, 1 print.
            let print = match p {
                Paper::Stripes => {
                    // Eight stripes across a tile, each a broad band and a
                    // thin edge line.
                    let f = (u * 8.0).fract();
                    let band = smooth(0.02, 0.04, f) * (1.0 - smooth(0.46, 0.48, f));
                    let line = smooth(0.56, 0.575, f) * (1.0 - smooth(0.6, 0.615, f));
                    band.max(line * 0.8)
                }
                Paper::Pinstripe => {
                    let f = (u * 24.0).fract();
                    smooth(0.0, 0.06, f) * (1.0 - smooth(0.1, 0.16, f))
                }
                Paper::Damask => {
                    // Columns of ogee flowers, every other column dropped
                    // half a repeat.
                    let (cols, rows) = (6.0, 4.0);
                    let cu = (u * cols).floor();
                    let fu = (u * cols).fract() - 0.5;
                    let drop = if (cu as i64) % 2 == 0 { 0.0 } else { 0.5 };
                    let fv = (v * rows + drop).fract() - 0.5;
                    // An ogee: a diamond with swelling sides, a stem through it.
                    let d = (fu.abs() * 2.2).powf(1.4) + (fv.abs() * 2.0).powf(0.9);
                    let flower = 1.0 - smooth(0.62, 0.7, d);
                    let hollow = 1.0 - smooth(0.25, 0.32, d);
                    let stem = (1.0 - smooth(0.012, 0.025, fu.abs())) * smooth(0.3, 0.32, fv.abs());
                    let rail = 1.0 - smooth(0.01, 0.02, (fu.abs() - 0.5).abs());
                    (flower - hollow * 0.7).max(stem * 0.6).max(rail * 0.5)
                }
                Paper::Atomic => {
                    // Pairs of fine lines, little four-point stars between.
                    let (cols, rows) = (5.0, 7.0);
                    let fu = (u * cols).fract();
                    let lines = [0.06, 0.1]
                        .iter()
                        .map(|c| 1.0 - smooth(0.006, 0.014, (fu - c).abs()))
                        .fold(0.0f32, f32::max);
                    let cu = (u * cols).floor();
                    let drop = if (cu as i64) % 2 == 0 { 0.0 } else { 0.5 };
                    let fv = (v * rows + drop).fract() - 0.5;
                    let su = (fu - 0.55) * cols / rows;
                    let (a, b) = (su.abs() * 9.0, fv.abs() * 9.0);
                    let star = 1.0
                        - smooth(
                            0.05,
                            0.12,
                            a.min(b) + (a * b).sqrt() * 0.6 + a.max(b) * 0.12,
                        );
                    let dot = 1.0 - smooth(0.02, 0.035, (su * su + fv * fv).sqrt());
                    lines.max(star).max(dot)
                }
            };
            let i = w.idx(x as i64, y as i64);
            let g = (0.95 + (fibre - 0.5) * 0.04) * (1.0 - 0.3 * print);
            w.rgb[i] = [g, g * 0.995, g * 0.985];
            w.height[i] = 0.5 + (fibre - 0.5) * 0.06 + print * 0.04;
            w.occlusion[i] = 1.0 - 0.05 * print;
            w.rough[i] = 0.85 - 0.3 * print;
        }
    }
    w.store(&mut t, 12.0);
    t
}

/// Wood grain for veneer and parquet: fine streaks along u, slow figure,
/// a pore now and then. 0..1, about 0.5 on average.
fn grain(seed: u64, u: f32, v: f32) -> f32 {
    let figure = value2(seed, 2, 12, u, v);
    let streak = value2(seed ^ 1, 6, 96, u + figure * 0.08, v);
    let fine = value2(seed ^ 2, 24, 384, u, v);
    let pore = (hash(seed ^ 3, (u * 512.0) as i64, (v * 1024.0) as i64) > 0.985) as u8 as f32;
    (streak * 0.55 + fine * 0.3 + figure * 0.15 - pore * 0.2).clamp(0.0, 1.0)
}

/// Polished veneer: long grain, light figure, satin lacquer.
fn veneer(size: u32) -> TextureLayer {
    let mut t = blank(size, 1.2, 0.5);
    let mut w = Work::of(&t);
    let n = w.n as f32;
    for y in 0..w.n {
        for x in 0..w.n {
            let (u, v) = (x as f32 / n, y as f32 / n);
            let g = grain(61, u, v);
            let i = w.idx(x as i64, y as i64);
            let c = 0.82 + (g - 0.5) * 0.3;
            // Darker grain lines lean towards red-brown.
            w.rgb[i] = [c, c * 0.97, c * 0.93];
            w.height[i] = 0.5 + (g - 0.5) * 0.04;
            w.rough[i] = 0.3 + (g - 0.5) * 0.08;
        }
    }
    w.store(&mut t, 10.0);
    t
}

/// Block parquet: squares of four slats, turned every other square (a
/// basket weave), each slat its own tone, with fine dark joints.
fn parquet(size: u32) -> TextureLayer {
    let mut t = blank(size, 1.2, 0.8);
    let mut w = Work::of(&t);
    let n = w.n as f32;
    let blocks = 4.0;
    for y in 0..w.n {
        for x in 0..w.n {
            let (u, v) = (x as f32 / n, y as f32 / n);
            let (bu, bv) = ((u * blocks).floor(), (v * blocks).floor());
            let (fu, fv) = ((u * blocks).fract(), (v * blocks).fract());
            let turned = ((bu + bv) as i64).rem_euclid(2) == 1;
            let (across, along) = if turned { (fv, fu) } else { (fu, fv) };
            let slat = (across * 4.0).floor();
            let fs = (across * 4.0).fract();
            let joint_d = fs.min(1.0 - fs) * 0.075;
            let edge_d = along.min(1.0 - along) * 0.3;
            let joint = 1.0 - smooth(0.0006, 0.0018, joint_d.min(edge_d));
            let tone = hash(71, (bu * 4.0 + slat) as i64, bv as i64);
            let (gu, gv) = if turned { (v, u) } else { (u, v) };
            let g = grain(72 + slat as u64, gv * 0.5, gu * 2.0);
            let i = w.idx(x as i64, y as i64);
            let c = (0.8 + (tone - 0.5) * 0.14 + (g - 0.5) * 0.22) * (1.0 - 0.6 * joint);
            w.rgb[i] = [c, c * 0.96, c * 0.9];
            w.height[i] = 0.55 - 0.3 * joint + (g - 0.5) * 0.03;
            w.occlusion[i] = 1.0 - 0.4 * joint;
            w.rough[i] = 0.38 + 0.4 * joint + (tone - 0.5) * 0.06;
        }
    }
    w.store(&mut t, 30.0);
    t
}

/// Linoleum squares, 30 cm, black and near white, glossy, faint marbling.
fn lino(size: u32) -> TextureLayer {
    let mut t = blank(size, 1.2, 0.5);
    let mut w = Work::of(&t);
    let n = w.n as f32;
    let squares = 4.0;
    for y in 0..w.n {
        for x in 0..w.n {
            let (u, v) = (x as f32 / n, y as f32 / n);
            let (su, sv) = ((u * squares).floor(), (v * squares).floor());
            let (fu, fv) = ((u * squares).fract(), (v * squares).fract());
            let dark = ((su + sv) as i64).rem_euclid(2) == 1;
            let edge = fu.min(1.0 - fu).min(fv.min(1.0 - fv)) * 0.3;
            let joint = 1.0 - smooth(0.0005, 0.0015, edge);
            let marble = fbm(81, 8, 4, u, v);
            let i = w.idx(x as i64, y as i64);
            let base = if dark { 0.11 } else { 0.93 };
            let c = (base + (marble - 0.5) * 0.06) * (1.0 - 0.3 * joint);
            w.rgb[i] = [c, c, c * 0.98];
            w.height[i] = 0.5 - 0.2 * joint;
            w.rough[i] = 0.2 + 0.4 * joint + (marble - 0.5) * 0.05;
        }
    }
    w.store(&mut t, 20.0);
    t
}

/// Terrazzo: white cement with chips of marble in three greys and a few
/// coloured ones, polished.
fn terrazzo(size: u32) -> TextureLayer {
    let mut t = blank(size, 1.6, 0.4);
    let mut w = Work::of(&t);
    let n = w.n as f32;
    let cells = 48.0;
    for y in 0..w.n {
        for x in 0..w.n {
            let (u, v) = (x as f32 / n, y as f32 / n);
            let (cu, cv) = ((u * cells).floor() as i64, (v * cells).floor() as i64);
            let (fu, fv) = ((u * cells).fract(), (v * cells).fract());
            // The nearest chip among the cell's neighbours.
            let mut best = (9.0f32, 0i64, 0i64);
            for dx in -1..=1 {
                for dy in -1..=1 {
                    let (i, j) = ((cu + dx).rem_euclid(48), (cv + dy).rem_euclid(48));
                    let px = dx as f32 + hash(91, i, j);
                    let py = dy as f32 + hash(92, i, j);
                    let d = ((fu - px).powi(2) + (fv - py).powi(2)).sqrt();
                    if d < best.0 {
                        best = (d, i, j);
                    }
                }
            }
            let r = 0.18 + 0.25 * hash(93, best.1, best.2);
            let chip = 1.0 - smooth(r - 0.04, r, best.0);
            let kind = hash(94, best.1, best.2);
            let tint = if kind < 0.4 {
                [0.62, 0.6, 0.58]
            } else if kind < 0.75 {
                [0.85, 0.83, 0.8]
            } else if kind < 0.93 {
                [0.4, 0.39, 0.38]
            } else {
                [0.75, 0.5, 0.42]
            };
            let ground = 0.95 + (value(95, 64, u, v) - 0.5) * 0.03;
            let mut c = [ground, ground * 0.99, ground * 0.97];
            mix(&mut c, tint, chip);
            let i = w.idx(x as i64, y as i64);
            w.rgb[i] = c;
            w.height[i] = 0.5;
            w.rough[i] = 0.22;
        }
    }
    w.store(&mut t, 10.0);
    t
}

// ---------------------------------------------------------------- wear

/// A seed per skin, so two worn layers do not stain alike.
fn skin_seed(s: Skin) -> u64 {
    use std::hash::Hash;
    // DefaultHasher is not stable across Rust versions, but the layers
    // only have to agree within one run; still, keep it simple and fixed.
    let mut h = FixedHasher(0xCBF2_9CE4_8422_2325);
    s.hash(&mut h);
    h.0
}

/// FNV-1a, stable everywhere (unlike `DefaultHasher`).
struct FixedHasher(u64);

impl std::hash::Hasher for FixedHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x0100_0000_01B3);
        }
    }
}

fn weather(t: &mut TextureLayer, s: Skin) {
    let k = kind(s.base);
    let a = s.wear.amount();
    let seed = skin_seed(s);
    let mut w = Work::of(t);
    let n = w.n as f32;

    // Plaster that has come away shows the brick under it.
    let brick =
        (k == Kind::Render && s.wear == Wear::Grimy).then(|| texgen::layer(MedievalLayer::Brick));

    // Every surface, even a fresh one, gets a faint large-scale
    // unevenness, so a wall is never one flat colour.
    let mottle_amt = match k {
        Kind::Render => 0.05 + 0.14 * a,
        Kind::Concrete => 0.06 + 0.2 * a,
        Kind::Masonry => 0.03 + 0.18 * a,
        Kind::Wood => 0.03 + 0.15 * a,
        Kind::Metal => 0.01 + 0.12 * a,
        Kind::Roof => 0.04 + 0.2 * a,
        Kind::Ground => 0.03 + 0.12 * a,
    };
    let streak_amt = match k {
        Kind::Render | Kind::Concrete => 0.03 + 0.3 * a,
        Kind::Masonry | Kind::Wood => 0.02 + 0.18 * a,
        Kind::Metal => 0.25 * a,
        Kind::Roof | Kind::Ground => 0.0,
    };
    // Dirt is a warm grey-brown; soot a cold near-black.
    let dirt = [0.62, 0.57, 0.5];
    let soot = [0.32, 0.31, 0.3];
    let soot_amt = match (k, s.wear) {
        (_, Wear::Grimy) => 0.35,
        (Kind::Masonry, Wear::Worn) => 0.15,
        _ => 0.0,
    };
    let bloom_amt = if k == Kind::Masonry && s.wear >= Wear::Aged {
        0.12 * a
    } else {
        0.0
    };
    let lichen_amt = if k == Kind::Roof { 0.5 * a } else { 0.0 };
    let rust_amt = if k == Kind::Metal && s.wear >= Wear::Worn {
        0.3 * a
    } else {
        0.0
    };
    let flake_amt = if k == Kind::Wood && s.wear >= Wear::Worn {
        a
    } else {
        0.0
    };

    for y in 0..w.n {
        for x in 0..w.n {
            let (u, v) = (x as f32 / n, y as f32 / n);
            let i = w.idx(x as i64, y as i64);
            let mut c = w.rgb[i];
            let mut r = w.rough[i];
            let mut h = w.height[i];
            let mut occ = w.occlusion[i];

            // Patches of discolouring.
            let m = fbm(seed, 3, 5, u, v);
            let patch = smooth(0.35, 0.8, m) * mottle_amt;
            for (ch, d) in c.iter_mut().zip(dirt) {
                *ch *= 1.0 - patch * (1.0 - d) * 1.6;
            }
            // Lighter, sun-bleached patches too.
            let bleach = smooth(0.6, 0.9, fbm(seed ^ 0x55, 2, 3, u, v)) * mottle_amt * 0.5;
            for ch in c.iter_mut() {
                *ch += (1.0 - *ch) * bleach;
            }

            // Rain streaks: thin vertical runs, stronger in places.
            if streak_amt > 0.0 {
                let col = value2(seed ^ 0x11, 48, 2, u, v);
                let fine = value2(seed ^ 0x12, 160, 3, u, v);
                let run = smooth(0.55, 0.95, col * 0.7 + fine * 0.3);
                let fade = smooth(0.2, 0.8, value2(seed ^ 0x13, 3, 2, u, v));
                let st = run * fade * streak_amt;
                for (ch, d) in c.iter_mut().zip(dirt) {
                    *ch *= 1.0 - st * (1.0 - d) * 1.8;
                }
                r += st * 0.1;
            }

            // Soot: dark, uneven, everywhere a little.
            if soot_amt > 0.0 {
                let sm = smooth(0.2, 0.9, fbm(seed ^ 0x21, 4, 4, u, v)) * soot_amt;
                for (ch, d) in c.iter_mut().zip(soot) {
                    *ch = *ch * (1.0 - sm) + *ch * d * sm;
                }
                r += sm * 0.1;
            }

            // Salt bloom on brick and stone: white haze in patches.
            if bloom_amt > 0.0 {
                let b = smooth(0.62, 0.9, fbm(seed ^ 0x31, 4, 4, u, v)) * bloom_amt;
                for ch in c.iter_mut() {
                    *ch += (0.92 - *ch) * b;
                }
            }

            // Lichen and moss on roofs: yellow-green spots, darker moss.
            if lichen_amt > 0.0 {
                let spot = smooth(0.7, 0.85, fbm(seed ^ 0x41, 16, 3, u, v)) * lichen_amt;
                let moss = smooth(0.6, 0.9, fbm(seed ^ 0x42, 4, 4, u, v)) * lichen_amt * 0.7;
                let lichen = [0.72, 0.7, 0.42];
                let green = [0.24, 0.3, 0.14];
                mix(&mut c, lichen, spot);
                mix(&mut c, green, moss);
                h += spot * 0.05 + moss * 0.08;
                r += moss * 0.2;
            }

            // Rust running from joints and screws on metal.
            if rust_amt > 0.0 {
                let col = value2(seed ^ 0x51, 64, 2, u, v);
                let st = smooth(0.7, 0.95, col)
                    * smooth(0.3, 0.9, value2(seed ^ 0x52, 4, 3, u, v))
                    * rust_amt;
                let rust = [0.5, 0.27, 0.13];
                for (ch, r) in c.iter_mut().zip(rust) {
                    *ch = *ch * (1.0 - st) + r * ch.max(0.4) * st;
                }
                r += st * 0.5;
            }

            // Flaking paint on wood: bare grey boards in patches.
            if flake_amt > 0.0 {
                let f = fbm(seed ^ 0x61, 8, 5, u, v);
                let thr = if s.wear == Wear::Grimy { 0.7 } else { 0.78 };
                let bare = smooth(thr, thr + 0.04, f) * flake_amt;
                let wood = [0.52, 0.47, 0.4];
                mix(&mut c, wood, bare);
                h -= bare * 0.05;
                r += bare * 0.3;
                // A dark rim where the paint edge lifts.
                let rim =
                    smooth(thr - 0.04, thr, f) * (1.0 - smooth(thr, thr + 0.04, f)) * flake_amt;
                for ch in c.iter_mut() {
                    *ch *= 1.0 - rim * 0.3;
                }
            }

            // Plaster fallen off: brick shows, with a broken rim.
            if let Some(brick) = &brick {
                let thr = 0.86;
                let f = fbm(seed ^ 0x71, 3, 5, u, v);
                let hole = smooth(thr, thr + 0.01, f);
                let rim = smooth(thr - 0.03, thr, f) * (1.0 - hole);
                if hole > 0.0 {
                    // The brick tile is 1.8 m; scale its pixels to ours.
                    let bs = brick.size as f32;
                    let scale = t.tile_metres / brick.tile_metres;
                    let bx = ((u * scale * bs) as i64).rem_euclid(brick.size as i64) as usize;
                    let by = ((v * scale * bs) as i64).rem_euclid(brick.size as i64) as usize;
                    let bi = (by * brick.size as usize + bx) * 4;
                    // Undo the tint a little: the brick is darker than paint.
                    let bc = [
                        brick.albedo[bi] as f32 / 255.0 * 0.85,
                        brick.albedo[bi + 1] as f32 / 255.0 * 0.85,
                        brick.albedo[bi + 2] as f32 / 255.0 * 0.85,
                    ];
                    mix(&mut c, bc, hole);
                    let bh = brick.normal[bi + 3] as f32 / 255.0;
                    h = h * (1.0 - hole) + (0.25 + bh * 0.2) * hole;
                    r = r * (1.0 - hole) + 0.85 * hole;
                }
                for ch in c.iter_mut() {
                    *ch *= 1.0 - rim * 0.35;
                }
                occ *= 1.0 - rim * 0.3;
            }

            w.rgb[i] = c;
            w.rough[i] = r.clamp(0.04, 1.0);
            w.height[i] = h.clamp(0.0, 1.0);
            w.occlusion[i] = occ;
        }
    }

    // Hairline cracks in render and concrete.
    if matches!(k, Kind::Render | Kind::Concrete) && s.wear >= Wear::Aged {
        let count = (3.0 + 12.0 * a) as usize;
        cracks(&mut w, seed ^ 0x81, count, 0.2 + 0.2 * a);
    }

    w.store(t, 40.0);
}

/// Random-walk cracks: thin dark grooves that wander and fork.
fn cracks(w: &mut Work, seed: u64, count: usize, depth: f32) {
    let n = w.n as f32;
    let mut k = 0i64;
    let mut next = || {
        k += 1;
        hash(seed, k, 7)
    };
    for _ in 0..count {
        let mut x = next() * n;
        let mut y = next() * n;
        let mut ang = next() * std::f32::consts::TAU;
        let len = (0.05 + next() * 0.2) * n;
        let mut steps = 0.0;
        while steps < len {
            let i = w.idx(x as i64, y as i64);
            let fade = 1.0 - steps / len;
            for ch in w.rgb[i].iter_mut() {
                *ch *= 1.0 - depth * fade;
            }
            w.height[i] -= 0.25 * fade;
            w.occlusion[i] *= 1.0 - 0.4 * fade;
            ang += (next() - 0.5) * 0.9;
            x += ang.cos();
            y += ang.sin();
            steps += 1.0;
            if next() > 0.985 {
                // A fork: turn sharply for a while.
                ang += if next() > 0.5 { 0.8 } else { -0.8 };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "on demand (looks): scripts/check.sh --on-demand"]
    fn worn_skins_are_darker_and_less_even_than_fresh() {
        let stats = |w: Wear| {
            let t = make(Skin::new(Base::Facade(FacadeLayer::Plaster), w), 64);
            let lum: Vec<f32> = t
                .albedo
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| (p[0] as f32 + p[1] as f32 + p[2] as f32) / 3.0)
                .collect();
            let mean = lum.iter().sum::<f32>() / lum.len() as f32;
            let var = lum.iter().map(|l| (l - mean).powi(2)).sum::<f32>() / lum.len() as f32;
            (mean, var.sqrt())
        };
        let (fresh_mean, fresh_sd) = stats(Wear::Fresh);
        let (grimy_mean, grimy_sd) = stats(Wear::Grimy);
        assert!(grimy_mean < fresh_mean, "{grimy_mean} vs {fresh_mean}");
        assert!(grimy_sd > fresh_sd, "{grimy_sd} vs {fresh_sd}");
    }

    #[test]
    fn skins_are_the_same_every_time() {
        let s = Skin::new(Base::Enamel, Wear::Worn);
        assert_eq!(make(s, 32).albedo, make(s, 32).albedo);
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn noise_tiles() {
        for p in [3, 7, 16] {
            let a = value(5, p, 0.0, 0.3);
            let b = value(5, p, 1.0, 0.3);
            assert!((a - b).abs() < 1e-5);
        }
    }
}
