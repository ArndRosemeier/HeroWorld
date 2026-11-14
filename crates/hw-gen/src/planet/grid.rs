//! A global grid on a cube-sphere: six square faces, `n x n` cells each, with
//! the equiangular mapping so cells are nearly the same size everywhere and
//! nothing bunches up at the poles. Climate and rivers are computed on it.

use glam::{DVec3, Vec3};
use hw_core::det;

/// A cell of the grid, as an index `face * n * n + j * n + i`.
pub type Cell = u32;

/// The shape of the grid; holds no data itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grid {
    pub n: usize,
}

/// The tangent of `x` radians, from the portable sin and cos.
fn tan(x: f64) -> f64 {
    let (s, c) = det::sin_cos(x);
    s / c
}

const QUARTER_PI: f64 = std::f64::consts::FRAC_PI_4;

impl Grid {
    pub fn new(n: usize) -> Self {
        assert!(n >= 2);
        Grid { n }
    }

    pub fn cells(&self) -> usize {
        6 * self.n * self.n
    }

    /// Edge of a cell at the face centre (m) on a planet of `radius`.
    pub fn cell_edge(&self, radius: f64) -> f64 {
        std::f64::consts::FRAC_PI_2 * radius / self.n as f64
    }

    /// Area of a cell on average (m²).
    pub fn cell_area(&self, radius: f64) -> f64 {
        4.0 * std::f64::consts::PI * radius * radius / self.cells() as f64
    }

    pub fn split(&self, c: Cell) -> (usize, usize, usize) {
        let c = c as usize;
        let nn = self.n * self.n;
        (c / nn, c % nn % self.n, c % nn / self.n)
    }

    pub fn index(&self, face: usize, i: usize, j: usize) -> Cell {
        (face * self.n * self.n + j * self.n + i) as Cell
    }

    /// The point on the cube face for face coordinates `(a, b)` in tangent space.
    fn cube_point(face: usize, a: f64, b: f64) -> DVec3 {
        match face {
            0 => DVec3::new(1.0, b, -a),
            1 => DVec3::new(-1.0, b, a),
            2 => DVec3::new(a, 1.0, -b),
            3 => DVec3::new(a, -1.0, b),
            4 => DVec3::new(a, b, 1.0),
            _ => DVec3::new(-a, b, -1.0),
        }
    }

    /// The unit vector through the point `(i + fx, j + fy)` of a face; the
    /// fractions may step outside the face, which wraps over its edge.
    fn unit_at(&self, face: usize, i: f64, j: f64) -> DVec3 {
        let s = (i / self.n as f64) * 2.0 - 1.0;
        let t = (j / self.n as f64) * 2.0 - 1.0;
        Self::cube_point(face, tan(s * QUARTER_PI), tan(t * QUARTER_PI)).normalize()
    }

    /// The unit vector through the middle of a cell.
    pub fn center(&self, c: Cell) -> DVec3 {
        let (f, i, j) = self.split(c);
        self.unit_at(f, i as f64 + 0.5, j as f64 + 0.5)
    }

    /// The cell holding the direction `u`.
    pub fn cell_of(&self, u: DVec3) -> Cell {
        let (ax, ay, az) = (u.x.abs(), u.y.abs(), u.z.abs());
        let (face, a, b) = if ax >= ay && ax >= az {
            if u.x > 0.0 {
                (0, -u.z / ax, u.y / ax)
            } else {
                (1, u.z / ax, u.y / ax)
            }
        } else if ay >= az {
            if u.y > 0.0 {
                (2, u.x / ay, -u.z / ay)
            } else {
                (3, u.x / ay, u.z / ay)
            }
        } else if u.z > 0.0 {
            (4, u.x / az, u.y / az)
        } else {
            (5, -u.x / az, u.y / az)
        };
        let to_cell = |x: f64| {
            let s = det::atan2(x, 1.0) / QUARTER_PI;
            (((s + 1.0) * 0.5 * self.n as f64) as usize).min(self.n - 1)
        };
        self.index(face, to_cell(a), to_cell(b))
    }

    /// The eight neighbours of a cell, over face edges too.
    pub fn neighbours(&self, c: Cell) -> [Cell; 8] {
        let (f, i, j) = self.split(c);
        let n = self.n as i64;
        let mut out = [0; 8];
        let mut k = 0;
        for dj in -1i64..=1 {
            for di in -1i64..=1 {
                if di == 0 && dj == 0 {
                    continue;
                }
                let (ni, nj) = (i as i64 + di, j as i64 + dj);
                out[k] = if (0..n).contains(&ni) && (0..n).contains(&nj) {
                    self.index(f, ni as usize, nj as usize)
                } else {
                    self.cell_of(self.unit_at(f, ni as f64 + 0.5, nj as f64 + 0.5))
                };
                k += 1;
            }
        }
        out
    }

    /// Every cell's centre, in cell order.
    pub fn centers(&self) -> Vec<Vec3> {
        par_map(self.cells(), |c| self.center(c as Cell).as_vec3())
    }
}

/// `f(0), f(1), ... f(len - 1)`, computed on all cores, in order.
pub fn par_map<T: Send>(len: usize, f: impl Fn(usize) -> T + Sync) -> Vec<T> {
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(16);
    if threads <= 1 || len < 4096 {
        return (0..len).map(f).collect();
    }
    let chunk = len.div_ceil(threads);
    let f = &f;
    let parts: Vec<Vec<T>> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..threads)
            .map(|t| {
                s.spawn(move || {
                    let (lo, hi) = (t * chunk, ((t + 1) * chunk).min(len));
                    (lo..hi.max(lo)).map(f).collect::<Vec<T>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("worker"))
            .collect()
    });
    parts.into_iter().flatten().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_round_trip_and_neighbours_are_close_and_symmetric() {
        let g = Grid::new(32);
        for c in (0..g.cells() as Cell).step_by(7) {
            assert_eq!(g.cell_of(g.center(c)), c, "cell {c}");
            let ns = g.neighbours(c);
            for nb in ns {
                assert_ne!(nb, c);
                let d = g.center(c).distance(g.center(nb));
                assert!(d < 2.5 * g.cell_edge(1.0), "cell {c} to {nb}: {d}");
            }
        }
        // Away from the 8-way corners, being a neighbour is mutual.
        let (mut asym, mut total) = (0, 0);
        for c in (0..g.cells() as Cell).step_by(5) {
            for nb in g.neighbours(c) {
                total += 1;
                asym += (!g.neighbours(nb).contains(&c)) as u32;
            }
        }
        assert!((asym as f64) < total as f64 * 0.02, "{asym} of {total}");
    }

    #[test]
    fn par_map_keeps_order() {
        let v = par_map(100_000, |i| i * 2);
        assert!(v.iter().enumerate().all(|(i, x)| *x == i * 2));
    }
}
