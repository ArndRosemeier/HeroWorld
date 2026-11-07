//! Deterministic gradient noise (2D Perlin) and fractal sums of it.
//!
//! Only additions, multiplications and `floor`, so every platform computes
//! the same bits. Gradients come from a hash of the lattice point and the
//! seed, not from a permutation table, so the noise covers the whole plane.

use crate::seed::{combine, mix64, Seed};

/// Eight unit gradient directions.
const GRADS: [(f64, f64); 8] = [
    (1.0, 0.0),
    (-1.0, 0.0),
    (0.0, 1.0),
    (0.0, -1.0),
    (
        std::f64::consts::FRAC_1_SQRT_2,
        std::f64::consts::FRAC_1_SQRT_2,
    ),
    (
        -std::f64::consts::FRAC_1_SQRT_2,
        std::f64::consts::FRAC_1_SQRT_2,
    ),
    (
        std::f64::consts::FRAC_1_SQRT_2,
        -std::f64::consts::FRAC_1_SQRT_2,
    ),
    (
        -std::f64::consts::FRAC_1_SQRT_2,
        -std::f64::consts::FRAC_1_SQRT_2,
    ),
];

/// A noise field for one seed.
#[derive(Clone, Copy, Debug)]
pub struct Noise {
    seed: u64,
}

fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

impl Noise {
    pub fn new(seed: Seed) -> Self {
        Noise { seed: seed.0 }
    }

    fn grad(&self, ix: i64, iy: i64) -> (f64, f64) {
        let h = mix64(combine(combine(self.seed, ix as u64), iy as u64));
        GRADS[(h & 7) as usize]
    }

    /// Perlin noise at `(x, y)`, roughly in `[-1, 1]`, one feature per unit.
    pub fn get(&self, x: f64, y: f64) -> f64 {
        let (fx, fy) = (x.floor(), y.floor());
        let (ix, iy) = (fx as i64, fy as i64);
        let (tx, ty) = (x - fx, y - fy);
        let dot = |gx: i64, gy: i64, dx: f64, dy: f64| {
            let g = self.grad(gx, gy);
            g.0 * dx + g.1 * dy
        };
        let n00 = dot(ix, iy, tx, ty);
        let n10 = dot(ix + 1, iy, tx - 1.0, ty);
        let n01 = dot(ix, iy + 1, tx, ty - 1.0);
        let n11 = dot(ix + 1, iy + 1, tx - 1.0, ty - 1.0);
        let (u, v) = (fade(tx), fade(ty));
        let a = n00 + (n10 - n00) * u;
        let b = n01 + (n11 - n01) * u;
        ((a + (b - a) * v) * 1.4).clamp(-1.0, 1.0)
    }

    /// Fractal sum of `octaves` layers, each twice as fine and half as
    /// strong, normalised to roughly `[-1, 1]`.
    pub fn fbm(&self, x: f64, y: f64, octaves: u32) -> f64 {
        let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
        for o in 0..octaves {
            // Offset each octave so their lattices don't line up.
            let off = o as f64 * 17.31;
            sum += amp * self.get(x * freq + off, y * freq - off);
            norm += amp;
            amp *= 0.5;
            freq *= 2.0;
        }
        sum / norm
    }

    /// Ridged fractal noise in `[0, 1]`: sharp crests, broad valleys.
    pub fn ridged(&self, x: f64, y: f64, octaves: u32) -> f64 {
        let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
        for o in 0..octaves {
            let off = o as f64 * 31.7;
            let n = 1.0 - self.get(x * freq + off, y * freq + off).abs();
            sum += amp * n * n;
            norm += amp;
            amp *= 0.5;
            freq *= 2.0;
        }
        sum / norm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_smooth_bounded_and_seeded() {
        let a = Noise::new(Seed(1));
        let b = Noise::new(Seed(2));
        let mut differ = 0;
        let mut lo = f64::MAX;
        let mut hi = f64::MIN;
        for i in 0..2000 {
            let (x, y) = (i as f64 * 0.137, i as f64 * 0.071 - 40.0);
            let v = a.fbm(x, y, 4);
            lo = lo.min(v);
            hi = hi.max(v);
            assert!((a.get(x, y) - a.get(x + 1e-4, y)).abs() < 1e-3);
            differ += (a.get(x, y) != b.get(x, y)) as u32;
        }
        assert!(lo >= -1.0 && hi <= 1.0 && hi - lo > 0.8, "{lo}..{hi}");
        assert!(differ > 1900);
        assert_eq!(a.get(0.0, 0.0), 0.0, "zero at lattice points");
    }
}
