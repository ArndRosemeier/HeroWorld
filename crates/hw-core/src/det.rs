//! Deterministic maths for world generation.
//!
//! `+ - * /` and `sqrt` are exact IEEE operations and give the same bits on
//! every machine, but `f64::sin`, `cos`, `atan2`, `exp` and friends call the
//! platform's maths library, which may round differently on Windows, macOS
//! and Linux. Co-op players each generate the world from the seed, so a single
//! flipped threshold would hand them different cities. Generation code uses
//! these pure-Rust versions instead (from `libm`), which are bit-identical
//! everywhere.

#[inline]
pub fn sin(x: f64) -> f64 {
    libm::sin(x)
}

#[inline]
pub fn cos(x: f64) -> f64 {
    libm::cos(x)
}

#[inline]
pub fn sin_cos(x: f64) -> (f64, f64) {
    libm::sincos(x)
}

#[inline]
pub fn atan2(y: f64, x: f64) -> f64 {
    libm::atan2(y, x)
}

#[inline]
pub fn asin(x: f64) -> f64 {
    libm::asin(x)
}

#[inline]
pub fn exp(x: f64) -> f64 {
    libm::exp(x)
}

#[inline]
pub fn powf(x: f64, y: f64) -> f64 {
    libm::pow(x, y)
}

#[inline]
pub fn log10(x: f64) -> f64 {
    libm::log10(x)
}

#[inline]
pub fn ln(x: f64) -> f64 {
    libm::log(x)
}

/// A unit vector at angle `a` (radians) in a 2D plane: `(cos a, sin a)`.
#[inline]
pub fn dir2(a: f64) -> glam::DVec2 {
    let (s, c) = sin_cos(a);
    glam::DVec2::new(c, s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agrees_with_std_closely() {
        for i in -50..50 {
            let x = i as f64 * 0.137;
            assert!((sin(x) - x.sin()).abs() < 1e-15);
            assert!((cos(x) - x.cos()).abs() < 1e-15);
            assert!((atan2(x, 1.3) - x.atan2(1.3)).abs() < 1e-15);
        }
    }
}
