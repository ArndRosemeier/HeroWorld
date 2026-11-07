//! A planar graph from crossing polylines, and the faces it encloses.
//!
//! Roads are traced one by one and cross each other anywhere. This splits
//! them where they cross or end on one another, merges points closer than a
//! tolerance into one node, and walks the faces: the closed pieces of land
//! between the lines (the town's quarters, later its blocks).

use crate::water::segment_distance;
use glam::DVec2;
use std::collections::HashMap;

/// An input line with a caller's tag (its road class, or "edge of town").
#[derive(Clone, Debug, PartialEq)]
pub struct TaggedLine {
    pub pts: Vec<DVec2>,
    pub tag: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    pub a: usize,
    pub b: usize,
    /// Geometry from node `a` to node `b`, ends included.
    pub pts: Vec<DVec2>,
    pub tag: u32,
}

/// A face: a closed loop of edges, walked with the face on the left.
#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    /// (edge, forward) in walking order.
    pub edges: Vec<(usize, bool)>,
    /// The outline, counter-clockwise (positive area), spikes removed.
    pub pts: Vec<DVec2>,
    pub area: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Graph {
    pub nodes: Vec<DVec2>,
    pub edges: Vec<Edge>,
}

/// Bucketed segments for crossing tests.
struct SegHash {
    cell: f64,
    map: HashMap<(i64, i64), Vec<(usize, usize)>>,
}

impl SegHash {
    fn cells(&self, a: DVec2, b: DVec2, pad: f64) -> impl Iterator<Item = (i64, i64)> {
        let lo = a.min(b) - DVec2::splat(pad);
        let hi = a.max(b) + DVec2::splat(pad);
        let (i0, j0) = (
            (lo.x / self.cell).floor() as i64,
            (lo.y / self.cell).floor() as i64,
        );
        let (i1, j1) = (
            (hi.x / self.cell).floor() as i64,
            (hi.y / self.cell).floor() as i64,
        );
        (j0..=j1).flat_map(move |j| (i0..=i1).map(move |i| (i, j)))
    }
}

impl Graph {
    /// Builds the graph. Points closer than `tol` merge; a line ending
    /// within `tol` of another line is joined onto it.
    pub fn build(lines: &[TaggedLine], tol: f64) -> Graph {
        let lines: Vec<&TaggedLine> = lines.iter().filter(|l| l.pts.len() >= 2).collect();
        let longest = lines
            .iter()
            .flat_map(|l| l.pts.windows(2).map(|w| w[0].distance(w[1])))
            .fold(10.0f64, f64::max);
        let mut hash = SegHash {
            cell: longest.clamp(10.0, 200.0),
            map: HashMap::new(),
        };
        for (li, l) in lines.iter().enumerate() {
            for (si, w) in l.pts.windows(2).enumerate() {
                for c in hash.cells(w[0], w[1], tol).collect::<Vec<_>>() {
                    hash.map.entry(c).or_default().push((li, si));
                }
            }
        }
        // Cut points per line: (segment, t, point).
        let mut cuts: Vec<Vec<(usize, f64, DVec2)>> = lines
            .iter()
            .map(|l| {
                vec![
                    (0, 0.0, l.pts[0]),
                    (l.pts.len() - 2, 1.0, *l.pts.last().expect("2+")),
                ]
            })
            .collect();
        let seg = |li: usize, si: usize| (lines[li].pts[si], lines[li].pts[si + 1]);
        let mut seen = std::collections::HashSet::new();
        for (li, l) in lines.iter().enumerate() {
            for si in 0..l.pts.len() - 1 {
                let (a, b) = seg(li, si);
                for c in hash.cells(a, b, tol).collect::<Vec<_>>() {
                    let Some(others) = hash.map.get(&c) else {
                        continue;
                    };
                    for &(lj, sj) in others {
                        if (lj, sj) <= (li, si) {
                            continue;
                        }
                        // Neighbouring segments of one line meet at a vertex anyway.
                        if lj == li && sj <= si + 1 {
                            continue;
                        }
                        if !seen.insert((li, si, lj, sj)) {
                            continue;
                        }
                        let (c0, d0) = seg(lj, sj);
                        if let Some((t, u, p)) = cross(a, b, c0, d0) {
                            cuts[li].push((si, t, p));
                            cuts[lj].push((sj, u, p));
                        }
                    }
                }
            }
        }
        // Ends that stop on (or just short of) another line join it.
        for li in 0..lines.len() {
            let n = lines[li].pts.len();
            for (end, p) in [(0usize, lines[li].pts[0]), (n - 1, lines[li].pts[n - 1])] {
                let mut best: Option<(f64, usize, usize, f64, DVec2)> = None;
                for c in hash.cells(p, p, tol).collect::<Vec<_>>() {
                    let Some(others) = hash.map.get(&c) else {
                        continue;
                    };
                    for &(lj, sj) in others {
                        if lj == li && (sj + 1 >= end && sj <= end) {
                            continue;
                        }
                        let (c0, d0) = seg(lj, sj);
                        let d = segment_distance(p, c0, d0);
                        if d <= tol && best.is_none_or(|b| d < b.0) {
                            let ab = d0 - c0;
                            let t =
                                ((p - c0).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
                            best = Some((d, lj, sj, t, c0 + ab * t));
                        }
                    }
                }
                if let Some((_, lj, sj, t, q)) = best {
                    cuts[lj].push((sj, t, q));
                    let k = if end == 0 { 0 } else { 1 };
                    cuts[li][k].2 = q;
                }
            }
        }
        // Nodes: cut points merged within the tolerance.
        let mut nodes: Vec<DVec2> = Vec::new();
        let mut node_hash: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        let mut node_of = |p: DVec2, nodes: &mut Vec<DVec2>| -> usize {
            let key = |p: DVec2| ((p.x / tol).floor() as i64, (p.y / tol).floor() as i64);
            let (ki, kj) = key(p);
            for j in kj - 1..=kj + 1 {
                for i in ki - 1..=ki + 1 {
                    if let Some(v) = node_hash.get(&(i, j)) {
                        for &n in v {
                            if nodes[n].distance(p) <= tol {
                                return n;
                            }
                        }
                    }
                }
            }
            nodes.push(p);
            node_hash.entry((ki, kj)).or_default().push(nodes.len() - 1);
            nodes.len() - 1
        };
        let mut edges: Vec<Edge> = Vec::new();
        let mut have: std::collections::HashSet<(usize, usize, i64)> =
            std::collections::HashSet::new();
        for (li, l) in lines.iter().enumerate() {
            let c = &mut cuts[li];
            c.sort_by(|x, y| (x.0, x.1).partial_cmp(&(y.0, y.1)).expect("finite"));
            let mut prev: Option<(usize, usize, f64)> = None; // node, seg, t
            for &(si, t, p) in c.iter() {
                let n = node_of(p, &mut nodes);
                if let Some((pn, ps, pt)) = prev {
                    if pn != n {
                        let mut pts = vec![nodes[pn]];
                        // Vertices strictly between the two cuts (vertex v
                        // sits at the start of segment v).
                        for v in ps + 1..=si {
                            if (v == ps + 1 && pt >= 1.0) || (v == si && t <= 0.0) {
                                continue;
                            }
                            pts.push(l.pts[v]);
                        }
                        pts.push(nodes[n]);
                        pts.dedup_by(|x, y| x.distance(*y) < 1e-6);
                        let len: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
                        let key = (pn.min(n), pn.max(n), (len * 0.5).round() as i64);
                        if pts.len() >= 2 && have.insert(key) {
                            edges.push(Edge {
                                a: pn,
                                b: n,
                                pts,
                                tag: l.tag,
                            });
                        }
                    }
                }
                prev = Some((n, si, t));
            }
        }
        Graph { nodes, edges }
    }

    /// The bounded faces (positive area, larger than `min_area`).
    pub fn faces(&self, min_area: f64) -> Vec<Face> {
        let n = self.nodes.len();
        // Outgoing half-edges per node, sorted by angle: (angle, edge, forward).
        let mut out: Vec<Vec<(f64, usize, bool)>> = vec![Vec::new(); n];
        for (k, e) in self.edges.iter().enumerate() {
            let d0 = e.pts[1] - e.pts[0];
            let d1 = e.pts[e.pts.len() - 2] - e.pts[e.pts.len() - 1];
            out[e.a].push((hw_core::det::atan2(d0.y, d0.x), k, true));
            out[e.b].push((hw_core::det::atan2(d1.y, d1.x), k, false));
        }
        for o in &mut out {
            o.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
        }
        let mut used = vec![[false; 2]; self.edges.len()];
        let mut faces = Vec::new();
        for start in 0..self.edges.len() {
            for fwd in [true, false] {
                if used[start][fwd as usize] {
                    continue;
                }
                let mut loop_edges = Vec::new();
                let (mut e, mut f) = (start, fwd);
                let ok;
                loop {
                    if used[e][f as usize] {
                        ok = e == start && f == fwd;
                        break;
                    }
                    used[e][f as usize] = true;
                    loop_edges.push((e, f));
                    let edge = &self.edges[e];
                    let v = if f { edge.b } else { edge.a };
                    // Leave v by the outgoing edge just clockwise of the way back.
                    let list = &out[v];
                    let back = list
                        .iter()
                        .position(|(_, k, kf)| *k == e && *kf != f)
                        .expect("twin is listed");
                    let (_, ne, nf) = list[(back + list.len() - 1) % list.len()];
                    e = ne;
                    f = nf;
                    if loop_edges.len() > self.edges.len() * 2 {
                        ok = false;
                        break;
                    }
                }
                if !ok {
                    continue;
                }
                let mut pts: Vec<DVec2> = Vec::new();
                for &(e, f) in &loop_edges {
                    let p = &self.edges[e].pts;
                    let it: Box<dyn Iterator<Item = &DVec2>> = if f {
                        Box::new(p.iter())
                    } else {
                        Box::new(p.iter().rev())
                    };
                    for q in it {
                        if pts.last() != Some(q) {
                            pts.push(*q);
                        }
                    }
                }
                if pts.len() > 1 && pts.first() == pts.last() {
                    pts.pop();
                }
                let pts = despike(pts);
                let area = signed_area(&pts);
                if area > min_area {
                    faces.push(Face {
                        edges: loop_edges,
                        pts,
                        area,
                    });
                }
            }
        }
        faces
    }
}

/// Where segments `ab` and `cd` cross: (t on ab, u on cd, point).
fn cross(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> Option<(f64, f64, DVec2)> {
    let r = b - a;
    let s = d - c;
    let den = r.perp_dot(s);
    if den.abs() < 1e-12 {
        return None;
    }
    let t = (c - a).perp_dot(s) / den;
    let u = (c - a).perp_dot(r) / den;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then(|| (t, u, a + r * t))
}

/// Shoelace area (positive counter-clockwise in x/y).
pub fn signed_area(pts: &[DVec2]) -> f64 {
    let n = pts.len();
    (0..n)
        .map(|k| pts[k].perp_dot(pts[(k + 1) % n]))
        .sum::<f64>()
        * 0.5
}

/// Removes there-and-back spikes (dead ends walked both ways).
fn despike(mut pts: Vec<DVec2>) -> Vec<DVec2> {
    let mut changed = true;
    while changed && pts.len() >= 3 {
        changed = false;
        let n = pts.len();
        for k in 0..n {
            let (p, q) = (pts[(k + n - 1) % n], pts[(k + 1) % n]);
            if p.distance(q) < 1e-6 {
                // Drop the tip and one of the doubled points.
                let hi = k.max((k + 1) % n);
                let lo = k.min((k + 1) % n);
                pts.remove(hi);
                pts.remove(lo);
                changed = true;
                break;
            }
        }
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(pts: &[(f64, f64)]) -> TaggedLine {
        TaggedLine {
            pts: pts.iter().map(|(x, y)| DVec2::new(*x, *y)).collect(),
            tag: 0,
        }
    }

    #[test]
    fn a_hash_sign_makes_one_closed_face() {
        // # : two lines each way, crossing in four points; plus a dead end.
        let lines = [
            line(&[(0.0, 10.0), (30.0, 10.0)]),
            line(&[(0.0, 20.0), (30.0, 20.0)]),
            line(&[(10.0, 0.0), (10.0, 30.0)]),
            line(&[(20.0, 0.0), (20.0, 30.0)]),
            line(&[(15.0, 10.0), (15.0, 15.0)]),
        ];
        let g = Graph::build(&lines, 0.5);
        let faces = g.faces(1.0);
        assert_eq!(faces.len(), 1, "{faces:?}");
        assert!((faces[0].area - 100.0).abs() < 1e-6);
        assert!(
            faces[0].pts.iter().all(|p| p.y < 14.0 || p.y > 16.0),
            "dead end removed: {:?}",
            faces[0].pts
        );
    }

    #[test]
    fn ends_short_of_a_line_join_it() {
        let lines = [
            line(&[
                (0.0, 0.0),
                (10.0, 0.0),
                (10.0, 10.0),
                (0.0, 10.0),
                (0.0, 0.0),
            ]),
            line(&[(5.0, 0.3), (5.0, 9.8)]),
        ];
        let g = Graph::build(&lines, 0.5);
        let mut areas: Vec<f64> = g.faces(1.0).iter().map(|f| f.area).collect();
        areas.sort_by(f64::total_cmp);
        assert_eq!(areas.len(), 2, "{areas:?}");
        assert!((areas[0] - 50.0).abs() < 1e-6 && (areas[1] - 50.0).abs() < 1e-6);
    }
}
