//! The shape of the planet: continents, ocean floors and mountain ranges as
//! one analytic function of a point on the unit sphere.
//!
//! No grid and no seams: the height anywhere is computed from seamless 3D
//! noise and a handful of tectonic plates, so a hero standing anywhere can ask
//! for a patch of ground at the detail they need, and neighbours agree. The
//! coarse shape is the same at every detail; finer detail only adds octaves.
//!
//! Plates give the layout: continental plates carry land, oceanic ones the sea
//! floor. Where two plates push together there are mountain belts (or island
//! arcs and trenches at sea); where they pull apart, rifts. Sea level is
//! calibrated per seed so that about [`LAND_SHARE`] of the surface is land.

use crate::urban::land::smooth;
use glam::DVec3;
use hw_core::{det, Noise3, Rng, Seed};

/// Fraction of the surface that is land.
pub const LAND_SHARE: f64 = 0.30;

/// How many tectonic plates the planet has.
const PLATES: usize = 16;

/// Highest point the shape makes (m); a few tall peaks reach it.
pub const MAX_HEIGHT: f64 = 9000.0;

#[derive(Clone, Debug)]
struct Plate {
    site: DVec3,
    continental: bool,
    /// How big it is: distances to its site are divided by this.
    size: f64,
    /// Where it is moving, a vector of length 0.4 to 1.
    drift: DVec3,
}

/// What the plates say about one point.
#[derive(Clone, Copy, Debug)]
struct Tectonics {
    /// How continental the ground is (blended over a few hundred km), about [0, 1].
    cont: f64,
    /// Distance to the nearest plate boundary (m).
    edge: f64,
    /// Plates closing (1) or pulling apart (-1) at that boundary.
    conv: f64,
    /// Whether the two plates at the boundary are continental.
    both_land: bool,
    both_sea: bool,
}

/// The planet's shape for one seed.
#[derive(Clone, Debug)]
pub struct Globe {
    radius: f64,
    plates: Vec<Plate>,
    n_cont: Noise3,
    n_warp: Noise3,
    n_hill: Noise3,
    n_ridge: Noise3,
    n_old: Noise3,
    /// Where sea level lies in the land value (calibrated).
    sea: f64,
}

impl Globe {
    /// The planet of `seed`, Earth-sized, with about [`LAND_SHARE`] of land.
    pub fn new(seed: Seed) -> Self {
        Self::with_radius(seed, hw_core::coords::HOME_RADIUS_M)
    }

    pub fn with_radius(seed: Seed, radius: f64) -> Self {
        let mut rng = seed.child("plates").rng();
        let mut plates = Vec::with_capacity(PLATES);
        let golden = 2.399_963_229_728_653;
        for k in 0..PLATES {
            let y = 1.0 - 2.0 * (k as f64 + 0.5) / PLATES as f64;
            let r = (1.0 - y * y).sqrt();
            let (s, c) = det::sin_cos(k as f64 * golden + rng.range(-0.5, 0.5));
            let jitter = DVec3::new(
                rng.range(-0.3, 0.3),
                rng.range(-0.3, 0.3),
                rng.range(-0.3, 0.3),
            );
            let site = (DVec3::new(r * c, y, r * s) + jitter).normalize();
            let mut v = DVec3::new(
                rng.range(-1.0, 1.0),
                rng.range(-1.0, 1.0),
                rng.range(-1.0, 1.0),
            );
            v -= site * v.dot(site);
            let drift = v.normalize_or(DVec3::X) * rng.range(0.4, 1.0);
            plates.push(Plate {
                site,
                continental: false,
                size: rng.range(0.6, 1.5),
                drift,
            });
        }
        // About four in ten plates carry land.
        let mut order: Vec<usize> = (0..PLATES).collect();
        rng.shuffle(&mut order);
        for &k in order.iter().take(PLATES * 4 / 10) {
            plates[k].continental = true;
        }
        let mut g = Globe {
            radius,
            plates,
            n_cont: Noise3::new(seed.child("continents")),
            n_warp: Noise3::new(seed.child("warp")),
            n_hill: Noise3::new(seed.child("hills")),
            n_ridge: Noise3::new(seed.child("ridges")),
            n_old: Noise3::new(seed.child("old")),
            sea: 0.0,
        };
        g.calibrate(&mut seed.child("calibrate").rng());
        g
    }

    pub fn radius(&self) -> f64 {
        self.radius
    }

    /// Set sea level so that [`LAND_SHARE`] of random points are above water.
    fn calibrate(&mut self, rng: &mut Rng) {
        let pts: Vec<DVec3> = (0..6000).map(|_| random_unit(rng)).collect();
        let (mut lo, mut hi) = (-1.5, 1.5);
        for _ in 0..14 {
            self.sea = 0.5 * (lo + hi);
            let land = pts
                .iter()
                .filter(|p| self.elevation(**p, 20_000.0) > 0.0)
                .count();
            if (land as f64 / pts.len() as f64) > LAND_SHARE {
                lo = self.sea; // too much land: raise the sea
            } else {
                hi = self.sea;
            }
        }
        self.sea = 0.5 * (lo + hi);
    }

    fn tectonics(&self, u: DVec3) -> Tectonics {
        // Ragged plate boundaries: look the plates up at a warped point.
        let w = DVec3::new(
            self.n_warp.fbm(u.x * 2.3, u.y * 2.3, u.z * 2.3, 4),
            self.n_warp
                .fbm(u.x * 2.3 + 9.1, u.y * 2.3 - 4.7, u.z * 2.3 + 2.2, 4),
            self.n_warp
                .fbm(u.x * 2.3 - 5.5, u.y * 2.3 + 8.3, u.z * 2.3 - 7.9, 4),
        );
        let q = (u + w * 0.22).normalize();
        let mut best = (f64::MAX, 0usize);
        let mut second = (f64::MAX, 0usize);
        let mut dist = [0.0f64; PLATES];
        for (k, p) in self.plates.iter().enumerate() {
            let d = q.distance(p.site) / p.size;
            dist[k] = d;
            if d < best.0 {
                second = best;
                best = (d, k);
            } else if d < second.0 {
                second = (d, k);
            }
        }
        let (mut sum, mut cont) = (0.0, 0.0);
        for (k, p) in self.plates.iter().enumerate() {
            let wgt = det::exp(-(dist[k] - best.0) / 0.12);
            sum += wgt;
            cont += wgt * if p.continental { 1.0 } else { 0.0 };
        }
        let (a, b) = (&self.plates[best.1], &self.plates[second.1]);
        let dir = (b.site - a.site).normalize_or(DVec3::X);
        Tectonics {
            cont: cont / sum,
            edge: (second.0 - best.0) * 0.5 * self.radius,
            conv: (a.drift - b.drift).dot(dir).clamp(-1.0, 1.0),
            both_land: a.continental && b.continental,
            both_sea: !a.continental && !b.continental,
        }
    }

    /// The land value: above [`Self::sea`] is land. Continents come from the
    /// plates, split and ragged by low-frequency noise.
    fn land_value(&self, u: DVec3, t: &Tectonics) -> f64 {
        let n = self.n_cont.fbm(u.x * 3.4, u.y * 3.4, u.z * 3.4, 4);
        let mid = self
            .n_cont
            .fbm(u.x * 8.0 + 3.0, u.y * 8.0, u.z * 8.0 - 3.0, 4);
        let coast = self
            .n_cont
            .fbm(u.x * 36.0 - 7.0, u.y * 36.0 + 1.0, u.z * 36.0, 5);
        0.45 * (t.cont - 0.5) + 1.1 * n + 0.28 * mid + 0.12 * coast
    }

    /// Height above sea level (m) at the unit vector `u`, with detail down to
    /// about `finest_m` metres. Below zero is the sea floor.
    pub fn elevation(&self, u: DVec3, finest_m: f64) -> f64 {
        let t = self.tectonics(u);
        let s = self.land_value(u, &t) - self.sea;
        let octaves = |base_m: f64| -> u32 {
            ((det::ln(base_m / finest_m.max(1.0)) / std::f64::consts::LN_2).ceil() as i64 + 1)
                .clamp(3, 24) as u32
        };

        let mut h = if s >= 0.0 {
            650.0 * smooth(0.0, 0.7, s)
        } else {
            -(250.0 * smooth(0.0, 0.1, -s) + 3900.0 * smooth(0.05, 0.5, -s))
        };
        let land_gate = smooth(-0.02, 0.2, s);
        let shore_gate = smooth(-0.03, 0.12, s);

        // Rolling hills, bigger inland.
        let hills = self
            .n_hill
            .fbm(u.x * 38.0, u.y * 38.0, u.z * 38.0, octaves(168_000.0));
        h += (150.0 + 650.0 * smooth(0.1, 0.8, s)) * hills * shore_gate;

        // Mountain belts where plates close.
        let ridge = self
            .n_ridge
            .ridged(u.x * 12.0, u.y * 12.0, u.z * 12.0, octaves(530_000.0));
        let conv_pos = t.conv.max(0.0);
        let belt = det::exp(-(t.edge / 350_000.0).powi(2));
        h += conv_pos * belt * (0.15 + 0.85 * ridge) * 6800.0 * land_gate;
        // Island arcs where two sea plates close; a trench beside a coast range.
        if t.both_sea && s < 0.0 {
            let arc = det::exp(-(t.edge / 180_000.0).powi(2));
            h += conv_pos * arc * smooth(0.5, 0.85, ridge) * 5200.0;
        }
        if !t.both_land && !t.both_sea && s < 0.0 {
            h -= 2500.0 * conv_pos * det::exp(-((t.edge - 60_000.0) / 150_000.0).powi(2));
        }
        // Rifts where plates part.
        if t.conv < 0.0 {
            h -= 350.0 * (-t.conv) * det::exp(-(t.edge / 250_000.0).powi(2)) * land_gate;
        }
        // Mountain chains inland: long ridges that wander.
        let wx = self.n_old.fbm(u.x * 6.0 + 1.0, u.y * 6.0, u.z * 6.0, 3);
        let wy = self.n_old.fbm(u.x * 6.0, u.y * 6.0 + 2.0, u.z * 6.0, 3);
        let wz = self.n_old.fbm(u.x * 6.0, u.y * 6.0, u.z * 6.0 + 3.0, 3);
        let wv = u + DVec3::new(wx, wy, wz) * 0.12;
        let chain =
            self.n_ridge
                .ridged(wv.x * 9.0 + 5.0, wv.y * 9.0, wv.z * 9.0, octaves(710_000.0));
        let chain = smooth(0.45, 0.95, chain);
        let massif = 0.5 + 0.5 * self.n_old.fbm(u.x * 2.5, u.y * 2.5, u.z * 2.5, 2);
        h += 4200.0 * chain * chain * massif * smooth(0.12, 0.5, s);
        // Old worn ranges inland.
        let old = self
            .n_old
            .ridged(u.x * 5.0, u.y * 5.0, u.z * 5.0, octaves(1_270_000.0));
        h += 900.0 * smooth(0.1, 0.5, s) * old * old;
        h.clamp(-11_000.0, MAX_HEIGHT)
    }
}

fn random_unit(rng: &mut Rng) -> DVec3 {
    loop {
        let v = DVec3::new(
            rng.range(-1.0, 1.0),
            rng.range(-1.0, 1.0),
            rng.range(-1.0, 1.0),
        );
        let l = v.length_squared();
        if l > 1e-4 && l <= 1.0 {
            return v / l.sqrt();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(g: &Globe, n: usize, finest: f64) -> Vec<f64> {
        let mut rng = Seed(5).rng();
        (0..n)
            .map(|_| g.elevation(random_unit(&mut rng), finest))
            .collect()
    }

    #[test]
    fn same_seed_same_planet_and_land_share_is_calibrated() {
        let a = Globe::new(Seed(7));
        let b = Globe::new(Seed(7));
        let ha = sample(&a, 2000, 20_000.0);
        assert_eq!(ha, sample(&b, 2000, 20_000.0));
        assert_ne!(ha, sample(&Globe::new(Seed(8)), 2000, 20_000.0));
        let share = ha.iter().filter(|h| **h > 0.0).count() as f64 / ha.len() as f64;
        assert!((share - LAND_SHARE).abs() < 0.05, "land share {share}");
        assert!(ha.iter().all(|h| (-11_000.0..=MAX_HEIGHT).contains(h)));
    }

    #[test]
    fn finer_detail_keeps_the_coarse_shape_and_has_no_seam_at_the_poles() {
        let g = Globe::new(Seed(3));
        let mut rng = Seed(9).rng();
        for _ in 0..200 {
            let u = random_unit(&mut rng);
            let coarse = g.elevation(u, 50_000.0);
            let fine = g.elevation(u, 50.0);
            assert!((coarse - fine).abs() < 250.0, "{coarse} vs {fine}");
        }
        // Across the date line and over the pole the height is continuous.
        for u in [DVec3::Y, DVec3::NEG_Y, DVec3::Z, DVec3::NEG_Z] {
            let a = g.elevation(u, 5000.0);
            let b = g.elevation((u + DVec3::new(1e-6, 1e-6, 1e-6)).normalize(), 5000.0);
            assert!((a - b).abs() < 5.0, "{a} vs {b}");
        }
    }
}
