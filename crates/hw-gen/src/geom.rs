//! 2D geometry on the ground plane of a city's local frame.
//!
//! A [`DVec2`] here is `(x, z)` of the local frame: x east, y-of-the-vector =
//! local z (south). Polygons are convex, with positive signed area, and carry
//! one tag per edge (edge `i` runs from `pts[i]` to `pts[i + 1]`), so a block
//! knows which street each of its sides faces.

use glam::DVec2;

const EPS: f64 = 1e-9;

/// What lies beyond an edge of a polygon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeTag {
    /// A street, by index into the city's street list.
    Street(u32),
    /// Another piece of the same parent (a neighbouring lot, a courtyard).
    Inner,
}

/// A convex polygon with positive area and a tag per edge.
#[derive(Clone, Debug, PartialEq)]
pub struct Poly {
    pub pts: Vec<DVec2>,
    pub tags: Vec<EdgeTag>,
}

/// Shoelace area of a point loop (positive counter-clockwise in x/z).
pub fn signed_area(pts: &[DVec2]) -> f64 {
    let n = pts.len();
    (0..n)
        .map(|k| pts[k].perp_dot(pts[(k + 1) % n]))
        .sum::<f64>()
        * 0.5
}

impl Poly {
    /// A polygon from points in either winding; all edges get `tag`.
    pub fn new(mut pts: Vec<DVec2>, tag: EdgeTag) -> Self {
        if signed_area(&pts) < 0.0 {
            pts.reverse();
        }
        let tags = vec![tag; pts.len()];
        Self { pts, tags }
    }

    pub fn len(&self) -> usize {
        self.pts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pts.len() < 3
    }

    pub fn area(&self) -> f64 {
        signed_area(&self.pts)
    }

    pub fn centroid(&self) -> DVec2 {
        let n = self.pts.len();
        let (mut c, mut a) = (DVec2::ZERO, 0.0);
        for i in 0..n {
            let (p, q) = (self.pts[i], self.pts[(i + 1) % n]);
            let w = p.perp_dot(q);
            c += (p + q) * w;
            a += w;
        }
        if a.abs() < EPS {
            return self.pts.iter().copied().sum::<DVec2>() / n.max(1) as f64;
        }
        c / (3.0 * a)
    }

    /// Edge `i` as (start, end).
    pub fn edge(&self, i: usize) -> (DVec2, DVec2) {
        (self.pts[i], self.pts[(i + 1) % self.pts.len()])
    }

    pub fn edge_len(&self, i: usize) -> f64 {
        let (a, b) = self.edge(i);
        a.distance(b)
    }

    /// Unit normal of edge `i` pointing into the polygon.
    pub fn inward(&self, i: usize) -> DVec2 {
        let (a, b) = self.edge(i);
        (b - a).perp().normalize_or_zero()
    }

    pub fn contains(&self, p: DVec2) -> bool {
        (0..self.len()).all(|i| {
            let (a, b) = self.edge(i);
            (b - a).perp_dot(p - a) >= -1e-7
        })
    }

    /// Keeps the part where `(v - point) · normal >= 0`. The new edge along
    /// the cut gets `tag`. Returns None when nothing (or a sliver) is left.
    pub fn clip(&self, point: DVec2, normal: DVec2, tag: EdgeTag) -> Option<Poly> {
        let n = self.len();
        let side: Vec<f64> = self.pts.iter().map(|p| (*p - point).dot(normal)).collect();
        let mut pts = Vec::with_capacity(n + 1);
        let mut tags = Vec::with_capacity(n + 1);
        for i in 0..n {
            let j = (i + 1) % n;
            let (si, sj) = (side[i], side[j]);
            if si >= 0.0 {
                pts.push(self.pts[i]);
                // The edge leaving a kept point keeps its tag, unless it leaves the
                // kept side, in which case the edge after the crossing is the cut.
                tags.push(self.tags[i]);
            }
            if (si >= 0.0) != (sj >= 0.0) {
                let t = si / (si - sj);
                pts.push(self.pts[i].lerp(self.pts[j], t));
                // Entering the kept side: the next edge is the remainder of edge i.
                // Leaving it: the next edge runs along the cut.
                tags.push(if si >= 0.0 { tag } else { self.tags[i] });
            }
        }
        let poly = Poly { pts, tags }.cleaned();
        (poly.len() >= 3 && poly.area() > 1e-6).then_some(poly)
    }

    /// Cuts along the line through `point` with direction `dir`. Both new
    /// edges get `tag`. Returns the two sides, left of the line first.
    pub fn split(&self, point: DVec2, dir: DVec2, tag: EdgeTag) -> Option<(Poly, Poly)> {
        let n = dir.perp().normalize_or_zero();
        let left = self.clip(point, n, tag)?;
        let right = self.clip(point, -n, tag)?;
        Some((left, right))
    }

    /// Moves every edge inwards by its own distance (`dist(tag)`), as for
    /// streets of different widths. Edges keep their tags.
    pub fn inset_by(&self, dist: impl Fn(EdgeTag) -> f64) -> Option<Poly> {
        let mut out = self.clone();
        for i in 0..self.len() {
            let d = dist(self.tags[i]);
            if d <= 0.0 {
                continue;
            }
            let (a, _) = self.edge(i);
            let n = self.inward(i);
            out = out.clip(a + n * d, n, self.tags[i])?;
        }
        Some(out)
    }

    /// Removes duplicate points, and collinear vertices between edges with the
    /// same tag (a vertex between differently tagged edges stays, so tags are
    /// never lost).
    fn cleaned(mut self) -> Poly {
        let mut i = 0;
        while self.pts.len() >= 3 && i < self.pts.len() {
            let n = self.pts.len();
            let prev = self.pts[(i + n - 1) % n];
            let (p, next) = (self.pts[i], self.pts[(i + 1) % n]);
            let dup = p.distance_squared(next) < 1e-12;
            let collinear = (p - prev).perp_dot(next - p).abs()
                < 1e-9 * (1.0 + prev.distance(next))
                && self.tags[(i + n - 1) % n] == self.tags[i];
            if dup || collinear {
                // Edge i-1 now runs to the next point and keeps its tag.
                self.pts.remove(i);
                self.tags.remove(i);
                i = 0;
            } else {
                i += 1;
            }
        }
        self
    }

    /// The minimum-area rectangle around the polygon, aligned with one of its
    /// edges (exact for convex polygons).
    pub fn obb(&self) -> Obb {
        let mut best: Option<Obb> = None;
        for i in 0..self.len() {
            let (a, b) = self.edge(i);
            let u = (b - a).normalize_or_zero();
            if u == DVec2::ZERO {
                continue;
            }
            let v = u.perp();
            let (mut lo, mut hi) = (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN));
            for p in &self.pts {
                let q = DVec2::new(p.dot(u), p.dot(v));
                lo = lo.min(q);
                hi = hi.max(q);
            }
            let ext = hi - lo;
            let c = (lo + hi) * 0.5;
            let obb = Obb {
                center: u * c.x + v * c.y,
                u,
                half: ext * 0.5,
            };
            if best.as_ref().is_none_or(|o| obb.area() < o.area()) {
                best = Some(obb);
            }
        }
        best.unwrap_or(Obb {
            center: DVec2::ZERO,
            u: DVec2::X,
            half: DVec2::ZERO,
        })
    }

    /// Where the line `p · v = t` crosses the polygon, as the interval of
    /// `p · u` (u ⟂ v). None if the line misses it.
    pub fn span_at(&self, u: DVec2, v: DVec2, t: f64) -> Option<(f64, f64)> {
        let n = self.len();
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for i in 0..n {
            let (a, b) = self.edge(i);
            let (ta, tb) = (a.dot(v) - t, b.dot(v) - t);
            if ta.abs() < EPS {
                lo = lo.min(a.dot(u));
                hi = hi.max(a.dot(u));
            }
            if (ta < 0.0) != (tb < 0.0) && (ta - tb).abs() > EPS {
                let p = a.lerp(b, ta / (ta - tb));
                lo = lo.min(p.dot(u));
                hi = hi.max(p.dot(u));
            }
        }
        (lo <= hi).then_some((lo, hi))
    }
}

/// An oriented rectangle: centre, unit axis `u` (v = u.perp()) and half extents.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Obb {
    pub center: DVec2,
    pub u: DVec2,
    pub half: DVec2,
}

impl Obb {
    pub fn area(&self) -> f64 {
        4.0 * self.half.x * self.half.y
    }

    /// Unit vector of the long axis.
    pub fn long_axis(&self) -> DVec2 {
        if self.half.x >= self.half.y {
            self.u
        } else {
            self.u.perp()
        }
    }

    pub fn long(&self) -> f64 {
        2.0 * self.half.x.max(self.half.y)
    }

    pub fn short(&self) -> f64 {
        2.0 * self.half.x.min(self.half.y)
    }
}

/// An axis-aligned rectangle in some local 2D frame (a building's floor).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub min: DVec2,
    pub max: DVec2,
}

impl Rect {
    pub fn new(min: DVec2, max: DVec2) -> Self {
        Self { min, max }
    }

    pub fn from_xz(x0: f64, z0: f64, x1: f64, z1: f64) -> Self {
        Self::new(DVec2::new(x0, z0), DVec2::new(x1, z1))
    }

    pub fn size(&self) -> DVec2 {
        self.max - self.min
    }

    pub fn area(&self) -> f64 {
        let s = self.size();
        s.x * s.y
    }

    pub fn center(&self) -> DVec2 {
        (self.min + self.max) * 0.5
    }

    pub fn contains_rect(&self, o: &Rect, eps: f64) -> bool {
        o.min.x >= self.min.x - eps
            && o.min.y >= self.min.y - eps
            && o.max.x <= self.max.x + eps
            && o.max.y <= self.max.y + eps
    }

    /// Area shared with another rectangle.
    pub fn overlap(&self, o: &Rect) -> f64 {
        let lo = self.min.max(o.min);
        let hi = self.max.min(o.max);
        let d = (hi - lo).max(DVec2::ZERO);
        d.x * d.y
    }

    /// Shrinks by `d` on every side.
    pub fn inset(&self, d: f64) -> Rect {
        Rect::new(self.min + d, self.max - d)
    }

    /// True if `p` lies on the rectangle's outline.
    pub fn on_boundary(&self, p: DVec2, eps: f64) -> bool {
        let inside = p.x >= self.min.x - eps
            && p.x <= self.max.x + eps
            && p.y >= self.min.y - eps
            && p.y <= self.max.y + eps;
        inside
            && ((p.x - self.min.x).abs() < eps
                || (p.x - self.max.x).abs() < eps
                || (p.y - self.min.y).abs() < eps
                || (p.y - self.max.y).abs() < eps)
    }
}

/// Segment `k` and fraction `t` at distance `s` along a polyline whose
/// points lie at distances `at` (two or more, increasing). `s` is not clamped
/// (`t` is).
pub fn locate(at: &[f64], s: f64) -> (usize, f64) {
    let last = at.len() - 2;
    let k = match at.binary_search_by(|v| v.total_cmp(&s)) {
        Ok(i) => i.min(last),
        Err(i) => i.saturating_sub(1).min(last),
    };
    let seg = (at[k + 1] - at[k]).max(1e-9);
    (k, ((s - at[k]) / seg).clamp(0.0, 1.0))
}

/// A line through points, measured: a road's centre line, a pavement.
#[derive(Clone, Debug, PartialEq)]
pub struct Polyline {
    pub pts: Vec<DVec2>,
    /// Distance along the line at each point.
    pub at: Vec<f64>,
}

impl Polyline {
    /// Repeated points are dropped; a line of fewer than two points becomes
    /// a 1 cm stub, so every query has a segment.
    pub fn new(pts: &[DVec2]) -> Self {
        let mut pts = pts.to_vec();
        pts.dedup_by(|a, b| a.distance(*b) < 1e-6);
        if pts.len() < 2 {
            let p = pts.first().copied().unwrap_or_default();
            pts = vec![p, p + DVec2::X * 0.01];
        }
        let mut at = vec![0.0];
        for w in pts.windows(2) {
            at.push(at[at.len() - 1] + w[0].distance(w[1]));
        }
        Self { pts, at }
    }

    pub fn len(&self) -> f64 {
        *self.at.last().unwrap_or(&0.0)
    }

    pub fn is_empty(&self) -> bool {
        self.pts.len() < 2
    }

    /// Segment and fraction at distance `s` (clamped to the line).
    pub fn locate(&self, s: f64) -> (usize, f64) {
        locate(&self.at, s.clamp(0.0, self.len()))
    }

    /// The point at distance `s` (clamped to the line).
    pub fn point(&self, s: f64) -> DVec2 {
        let (k, t) = self.locate(s);
        self.pts[k].lerp(self.pts[k + 1], t)
    }

    /// Distance along the line of the point nearest `p`, and how far off.
    pub fn nearest(&self, p: DVec2) -> (f64, f64) {
        let mut best = (0.0, f64::MAX);
        for k in 0..self.pts.len() - 1 {
            let (a, b) = (self.pts[k], self.pts[k + 1]);
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
            let d = p.distance(a + ab * t);
            if d < best.1 {
                best = (self.at[k] + t * (self.at[k + 1] - self.at[k]), d);
            }
        }
        best
    }

    /// The line from `s0` to `s1` (either way round).
    pub fn slice(&self, s0: f64, s1: f64) -> Vec<DVec2> {
        let (lo, hi) = (s0.min(s1), s0.max(s1));
        let mut out = vec![self.point(lo)];
        for (i, &a) in self.at.iter().enumerate() {
            if a > lo + 1e-6 && a < hi - 1e-6 {
                out.push(self.pts[i]);
            }
        }
        out.push(self.point(hi));
        if s0 > s1 {
            out.reverse();
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(s: f64) -> Poly {
        Poly::new(
            vec![
                DVec2::new(0.0, 0.0),
                DVec2::new(s, 0.0),
                DVec2::new(s, s),
                DVec2::new(0.0, s),
            ],
            EdgeTag::Street(0),
        )
    }

    #[test]
    fn area_and_centroid() {
        let p = square(10.0);
        assert!((p.area() - 100.0).abs() < 1e-9);
        assert!(p.centroid().distance(DVec2::new(5.0, 5.0)) < 1e-9);
        // Clockwise input is flipped.
        let mut pts = p.pts.clone();
        pts.reverse();
        assert!(Poly::new(pts, EdgeTag::Inner).area() > 0.0);
    }

    #[test]
    fn split_preserves_area_and_tags() {
        let p = square(10.0);
        let (a, b) = p
            .split(DVec2::new(4.0, 0.0), DVec2::Y, EdgeTag::Street(7))
            .unwrap();
        assert!((a.area() + b.area() - 100.0).abs() < 1e-9);
        assert!((a.area().min(b.area()) - 40.0).abs() < 1e-9);
        for q in [&a, &b] {
            assert_eq!(q.len(), 4);
            assert_eq!(
                q.tags.iter().filter(|t| **t == EdgeTag::Street(7)).count(),
                1
            );
            assert_eq!(
                q.tags.iter().filter(|t| **t == EdgeTag::Street(0)).count(),
                3
            );
        }
    }

    #[test]
    fn cut_edge_is_tagged_on_the_cut() {
        let p = square(10.0);
        let (a, _) = p
            .split(DVec2::new(4.0, 0.0), DVec2::Y, EdgeTag::Street(7))
            .unwrap();
        for i in 0..a.len() {
            let (s, e) = a.edge(i);
            let on_cut = (s.x - 4.0).abs() < 1e-9 && (e.x - 4.0).abs() < 1e-9;
            assert_eq!(on_cut, a.tags[i] == EdgeTag::Street(7), "edge {i}");
        }
    }

    #[test]
    fn inset_by_tag() {
        let mut p = square(20.0);
        p.tags[0] = EdgeTag::Street(1);
        let q = p
            .inset_by(|t| if t == EdgeTag::Street(1) { 5.0 } else { 1.0 })
            .unwrap();
        // 20 - 5 - 1 by 20 - 1 - 1
        assert!((q.area() - 14.0 * 18.0).abs() < 1e-9);
        assert!(q.tags.contains(&EdgeTag::Street(1)));
        assert!(p.inset_by(|_| 11.0).is_none());
    }

    #[test]
    fn obb_of_rotated_rectangle() {
        let (c, s) = (0.6, 0.8);
        let u = DVec2::new(c, s);
        let v = u.perp();
        let pts = vec![DVec2::ZERO, u * 30.0, u * 30.0 + v * 10.0, v * 10.0];
        let obb = Poly::new(pts, EdgeTag::Inner).obb();
        assert!((obb.long() - 30.0).abs() < 1e-9);
        assert!((obb.short() - 10.0).abs() < 1e-9);
        assert!(obb.long_axis().dot(u).abs() > 0.999_999);
    }

    #[test]
    fn span_of_square() {
        let p = square(10.0);
        let (lo, hi) = p.span_at(DVec2::X, DVec2::Y, 3.0).unwrap();
        assert!((lo - 0.0).abs() < 1e-9 && (hi - 10.0).abs() < 1e-9);
        assert!(p.span_at(DVec2::X, DVec2::Y, 11.0).is_none());
    }

    #[test]
    fn rect_overlap() {
        let a = Rect::from_xz(0.0, 0.0, 4.0, 4.0);
        let b = Rect::from_xz(2.0, 2.0, 6.0, 6.0);
        assert_eq!(a.overlap(&b), 4.0);
        assert_eq!(a.overlap(&Rect::from_xz(4.0, 0.0, 5.0, 4.0)), 0.0);
        assert!(a.on_boundary(DVec2::new(4.0, 1.0), 1e-9));
        assert!(!a.on_boundary(DVec2::new(2.0, 1.0), 1e-9));
    }
}
