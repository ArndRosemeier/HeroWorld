//! Line helpers for town plans: contour lines of a raster and cutting
//! polylines where a condition fails.

use super::land::Grid;
use glam::DVec2;
use std::collections::BTreeMap;

/// Contour lines of `g` at `level` (marching squares over cell centres),
/// joined into polylines. Infinite values count as very high.
pub fn contours(g: &Grid<f32>, level: f32) -> Vec<Vec<DVec2>> {
    let v = |i: usize, j: usize| {
        let x = *g.at(i, j);
        if x.is_finite() {
            x
        } else {
            1e9
        }
    };
    // Edge keys: (i, j, 0) from (i, j) to (i+1, j); (i, j, 1) to (i, j+1).
    let point = |i: usize, j: usize, dir: u8| -> DVec2 {
        let (a, b) = (
            g.center(i, j),
            if dir == 0 {
                g.center(i + 1, j)
            } else {
                g.center(i, j + 1)
            },
        );
        let (va, vb) = (v(i, j), if dir == 0 { v(i + 1, j) } else { v(i, j + 1) });
        let t = ((level - va) / (vb - va)).clamp(0.0, 1.0) as f64;
        a.lerp(b, t)
    };
    let mut segs: Vec<[(usize, usize, u8); 2]> = Vec::new();
    for j in 0..g.h.saturating_sub(1) {
        for i in 0..g.w.saturating_sub(1) {
            let c = [
                v(i, j) > level,
                v(i + 1, j) > level,
                v(i + 1, j + 1) > level,
                v(i, j + 1) > level,
            ];
            // Edges of the square: bottom, right, top, left.
            let e = [(i, j, 0u8), (i + 1, j, 1u8), (i, j + 1, 0u8), (i, j, 1u8)];
            let crossed: Vec<usize> = (0..4).filter(|k| c[*k] != c[(*k + 1) % 4]).collect();
            match crossed.as_slice() {
                [a, b] => segs.push([e[*a], e[*b]]),
                [a, b, c2, d] => {
                    segs.push([e[*a], e[*b]]);
                    segs.push([e[*c2], e[*d]]);
                }
                _ => {}
            }
        }
    }
    // Join segments sharing edge points.
    let mut at: BTreeMap<(usize, usize, u8), Vec<usize>> = BTreeMap::new();
    for (k, s) in segs.iter().enumerate() {
        at.entry(s[0]).or_default().push(k);
        at.entry(s[1]).or_default().push(k);
    }
    let mut used = vec![false; segs.len()];
    let mut out = Vec::new();
    for start in 0..segs.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let mut keys = std::collections::VecDeque::from(vec![segs[start][0], segs[start][1]]);
        // Grow both ways.
        for front in [false, true] {
            loop {
                let end = if front { keys[0] } else { keys[keys.len() - 1] };
                let next = at[&end].iter().copied().find(|k| !used[*k]);
                let Some(k) = next else { break };
                used[k] = true;
                let other = if segs[k][0] == end {
                    segs[k][1]
                } else {
                    segs[k][0]
                };
                if front {
                    keys.push_front(other);
                } else {
                    keys.push_back(other);
                }
            }
        }
        out.push(keys.iter().map(|(i, j, d)| point(*i, *j, *d)).collect());
    }
    out
}

/// The pieces of a polyline where `ok` holds at its points.
pub fn split_where(pts: &[DVec2], ok: impl Fn(DVec2) -> bool) -> Vec<Vec<DVec2>> {
    let mut out = Vec::new();
    let mut cur: Vec<DVec2> = Vec::new();
    for p in pts {
        if ok(*p) {
            cur.push(*p);
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out.retain(|l| l.len() >= 2);
    out
}

/// Length of a polyline.
pub fn length(pts: &[DVec2]) -> f64 {
    pts.windows(2).map(|w| w[0].distance(w[1])).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contour_of_a_cone_is_one_closed_ring() {
        let mut g = Grid::new(100.0, 5.0, 0.0f32);
        for j in 0..g.h {
            for i in 0..g.w {
                let d = g.center(i, j).length() as f32;
                g.set(i, j, d);
            }
        }
        let c = contours(&g, 50.0);
        assert_eq!(c.len(), 1);
        let ring = &c[0];
        assert!(
            ring.first().unwrap().distance(*ring.last().unwrap()) < 1e-9,
            "closed"
        );
        for p in ring {
            assert!((p.length() - 50.0).abs() < 1.0);
        }
    }
}
