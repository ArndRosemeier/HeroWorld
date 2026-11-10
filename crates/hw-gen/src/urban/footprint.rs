//! The solid shape of things standing in the town, as plain data.
//!
//! What is drawn and what stops a body come from the same numbers: props
//! and landmarks describe their solid parts here, collision turns them into
//! boxes, people on foot step round them and cars keep their lanes clear of
//! them. A visible barrier that is not in here can be walked through.

use glam::DVec2;

/// One solid part, standing from `y0` to `y1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Footprint {
    /// A round post, drum or tower: centre and radius.
    Disc { at: DVec2, r: f64, y0: f64, y1: f64 },
    /// A box: centre, the way its +z faces (unit), half its width (x) and
    /// half its depth (z).
    Box {
        at: DVec2,
        dir: DVec2,
        half: DVec2,
        y0: f64,
        y1: f64,
    },
}

impl Footprint {
    /// Bottom and top.
    pub fn span(&self) -> (f64, f64) {
        match *self {
            Footprint::Disc { y0, y1, .. } | Footprint::Box { y0, y1, .. } => (y0, y1),
        }
    }

    /// Its centre in plan.
    pub fn at(&self) -> DVec2 {
        match *self {
            Footprint::Disc { at, .. } | Footprint::Box { at, .. } => at,
        }
    }

    /// The radius of a circle round it in plan.
    pub fn reach(&self) -> f64 {
        match *self {
            Footprint::Disc { r, .. } => r,
            Footprint::Box { half, .. } => half.length(),
        }
    }

    /// Its extent along the unit vector `u` from `o`: (min, max).
    pub fn extent(&self, o: DVec2, u: DVec2) -> (f64, f64) {
        match *self {
            Footprint::Disc { at, r, .. } => {
                let c = (at - o).dot(u);
                (c - r, c + r)
            }
            Footprint::Box { at, dir, half, .. } => {
                let c = (at - o).dot(u);
                let x = DVec2::new(dir.y, -dir.x);
                let e = half.x * x.dot(u).abs() + half.y * dir.dot(u).abs();
                (c - e, c + e)
            }
        }
    }

    /// Distance in plan from `p` to its outline (0 inside).
    pub fn distance(&self, p: DVec2) -> f64 {
        match *self {
            Footprint::Disc { at, r, .. } => (p.distance(at) - r).max(0.0),
            Footprint::Box { at, dir, half, .. } => {
                let d = p - at;
                let x = DVec2::new(dir.y, -dir.x);
                let l = DVec2::new(d.dot(x).abs(), d.dot(dir).abs());
                (l - half).max(DVec2::ZERO).length()
            }
        }
    }

    /// The same shape moved into the frame of something standing at `pos`
    /// facing `dir` (local +z), lifted to `y`: `self` is in local metres.
    pub fn placed(self, pos: DVec2, dir: DVec2, y: f64) -> Footprint {
        let x = DVec2::new(dir.y, -dir.x);
        let to = |l: DVec2| pos + x * l.x + dir * l.y;
        match self {
            Footprint::Disc { at, r, y0, y1 } => Footprint::Disc {
                at: to(at),
                r,
                y0: y + y0,
                y1: y + y1,
            },
            Footprint::Box {
                at,
                dir: d,
                half,
                y0,
                y1,
            } => Footprint::Box {
                at: to(at),
                dir: (x * d.x + dir * d.y).normalize_or(dir),
                half,
                y0: y + y0,
                y1: y + y1,
            },
        }
    }
}

/// A round wall (a fountain's rim): boxes round a circle of radius `r`,
/// `thick` deep, in local metres.
pub fn ring(r: f64, thick: f64, y1: f64, n: usize) -> Vec<Footprint> {
    let tau = std::f64::consts::TAU;
    (0..n)
        .map(|k| {
            let a = tau * k as f64 / n as f64;
            let d = hw_core::det::dir2(a);
            let side = r * (tau / n as f64 * 0.5).tan() + thick * 0.5;
            Footprint::Box {
                at: d * (r - thick * 0.5),
                dir: d,
                half: DVec2::new(side, thick * 0.5),
                y0: 0.0,
                y1,
            }
        })
        .collect()
}

/// A local disc at the origin.
pub fn disc(r: f64, y1: f64) -> Footprint {
    Footprint::Disc {
        at: DVec2::ZERO,
        r,
        y0: 0.0,
        y1,
    }
}

/// A local box at the origin facing +z.
pub fn block(hx: f64, hz: f64, y1: f64) -> Footprint {
    Footprint::Box {
        at: DVec2::ZERO,
        dir: DVec2::Y,
        half: DVec2::new(hx, hz),
        y0: 0.0,
        y1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn placed_boxes_turn_with_their_owner() {
        let b = Footprint::Box {
            at: DVec2::new(0.0, 2.0),
            dir: DVec2::Y,
            half: DVec2::new(1.0, 0.5),
            y0: 0.0,
            y1: 1.0,
        };
        // Facing +x: local +z is world +x, local +x is world -z.
        let p = b.placed(DVec2::new(10.0, 0.0), DVec2::X, 5.0);
        assert!(p.at().distance(DVec2::new(12.0, 0.0)) < 1e-9);
        assert_eq!(p.span(), (5.0, 6.0));
        let (lo, hi) = p.extent(DVec2::ZERO, DVec2::X);
        assert!((lo - 11.5).abs() < 1e-9 && (hi - 12.5).abs() < 1e-9);
        assert!(p.distance(DVec2::new(12.0, 0.9)) < 1e-9);
        assert!((p.distance(DVec2::new(12.0, 2.0)) - 1.0).abs() < 1e-9);
    }
}
