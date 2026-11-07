//! Direction fields for laying out streets.
//!
//! A street pattern is a field of directions: at every point, which way the
//! streets run (and, at right angles, which way the cross streets run). The
//! field is a sum of simple pieces: a grid at some angle, rings and radials
//! around a square, the run of a nearby road or shore. Streets traced along
//! it ([`super::trace`]) curve smoothly where the pieces blend, follow
//! rivers, line up with the roads they meet and stay straight where a plan
//! rules. This is the method of Chen et al., "Interactive Procedural Street
//! Modeling" (SIGGRAPH 2008).
//!
//! Directions here are unsigned (a street runs both ways), so they are kept
//! as tensors: a direction at angle `t` is the vector (cos 2t, sin 2t),
//! which is the same for `t` and `t + 180°`, and tensors add up.

use glam::DVec2;
use hw_core::{det, Noise};

/// A weighted, unsigned direction.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Tensor {
    pub a: f64,
    pub b: f64,
}

impl Tensor {
    /// Streets along angle `t` (radians), with weight `w`.
    pub fn along(t: f64, w: f64) -> Self {
        let (s, c) = det::sin_cos(2.0 * t);
        Tensor { a: c * w, b: s * w }
    }

    /// Streets along direction `d` (need not be unit length), weight `w`.
    pub fn along_dir(d: DVec2, w: f64) -> Self {
        // (cos 2t, sin 2t) from (cos t, sin t) without trigonometry.
        let l2 = d.length_squared();
        if l2 < 1e-18 {
            return Tensor::default();
        }
        Tensor {
            a: (d.x * d.x - d.y * d.y) / l2 * w,
            b: 2.0 * d.x * d.y / l2 * w,
        }
    }

    pub fn plus(self, o: Tensor) -> Tensor {
        Tensor {
            a: self.a + o.a,
            b: self.b + o.b,
        }
    }

    pub fn scale(self, k: f64) -> Tensor {
        Tensor {
            a: self.a * k,
            b: self.b * k,
        }
    }

    pub fn weight(self) -> f64 {
        (self.a * self.a + self.b * self.b).sqrt()
    }

    /// Angle of the main streets (radians).
    pub fn angle(self) -> f64 {
        det::atan2(self.b, self.a) * 0.5
    }

    /// Unit direction of the main streets (`major`) or the cross streets.
    pub fn dir(self, major: bool) -> DVec2 {
        let d = det::dir2(self.angle());
        if major {
            d
        } else {
            d.perp()
        }
    }

    /// The same field turned by `t` radians.
    pub fn rotated(self, t: f64) -> Tensor {
        let (s, c) = det::sin_cos(2.0 * t);
        Tensor {
            a: self.a * c - self.b * s,
            b: self.a * s + self.b * c,
        }
    }
}

/// One ingredient of a street pattern.
#[derive(Clone, Debug, PartialEq)]
pub enum Basis {
    /// Straight streets at `angle` around `center`.
    Grid {
        center: DVec2,
        angle: f64,
        /// Distance over which its influence fades (m).
        reach: f64,
        weight: f64,
    },
    /// Rings and radials around `center` (a square, a monument, a core).
    Radial {
        center: DVec2,
        reach: f64,
        weight: f64,
    },
    /// Streets following a line (a road, a quay, a shore), and at right
    /// angles to it.
    Line {
        pts: Vec<DVec2>,
        reach: f64,
        weight: f64,
    },
}

/// Gaussian fall-off.
fn fade(d: f64, reach: f64) -> f64 {
    let x = d / reach;
    det::exp(-x * x)
}

impl Basis {
    pub fn eval(&self, p: DVec2) -> Tensor {
        match self {
            Basis::Grid {
                center,
                angle,
                reach,
                weight,
            } => Tensor::along(*angle, weight * fade(p.distance(*center), *reach)),
            Basis::Radial {
                center,
                reach,
                weight,
            } => {
                let d = p - *center;
                // Main streets run round the centre; cross streets radiate.
                Tensor::along_dir(d.perp(), weight * fade(d.length(), *reach))
            }
            Basis::Line { pts, reach, weight } => {
                let mut best = (f64::MAX, DVec2::X);
                for w in pts.windows(2) {
                    let d = crate::water::segment_distance(p, w[0], w[1]);
                    if d < best.0 {
                        best = (d, w[1] - w[0]);
                    }
                }
                if best.0 > reach * 3.0 {
                    return Tensor::default();
                }
                Tensor::along_dir(best.1, weight * fade(best.0, *reach))
            }
        }
    }
}

/// A street pattern: basis pieces, plus a meander that bends streets the
/// more, the less planned a place is.
pub struct Field {
    pub basis: Vec<Basis>,
    pub noise: Noise,
    /// Wavelength of the meander (m).
    pub wiggle_len: f64,
    /// Largest turn of the meander (radians), per point.
    pub wiggle: Box<WiggleFn>,
}

/// How much the meander turns streets at a point (radians).
pub type WiggleFn = dyn Fn(DVec2) -> f64 + Send + Sync;

impl Field {
    pub fn eval(&self, p: DVec2) -> Tensor {
        let mut t = Tensor::default();
        for b in &self.basis {
            t = t.plus(b.eval(p));
        }
        let amount = (self.wiggle)(p);
        if amount > 0.0 {
            let n = self
                .noise
                .fbm(p.x / self.wiggle_len, p.y / self.wiggle_len, 3);
            t = t.rotated(n * amount);
        }
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directions_are_unsigned_and_add_up() {
        let a = Tensor::along(0.3, 1.0);
        let b = Tensor::along(0.3 + std::f64::consts::PI, 1.0);
        assert!((a.a - b.a).abs() < 1e-12 && (a.b - b.b).abs() < 1e-12);
        assert!(
            (Tensor::along_dir(DVec2::new(1.0, 1.0), 1.0).angle() - std::f64::consts::FRAC_PI_4)
                .abs()
                < 1e-12
        );
        // Two crossing pulls of equal weight meet in the middle.
        let m = Tensor::along(0.0, 1.0).plus(Tensor::along(0.6, 1.0));
        assert!((m.angle() - 0.3).abs() < 1e-9);
        assert!((Tensor::along(0.2, 2.0).rotated(0.1).angle() - 0.3).abs() < 1e-9);
    }

    #[test]
    fn radial_fields_ring_the_centre() {
        let r = Basis::Radial {
            center: DVec2::ZERO,
            reach: 100.0,
            weight: 1.0,
        };
        let d = r.eval(DVec2::new(50.0, 0.0)).dir(true);
        assert!(d.x.abs() < 1e-9, "main streets run round: {d}");
    }
}
