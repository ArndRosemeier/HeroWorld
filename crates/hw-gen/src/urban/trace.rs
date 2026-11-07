//! Streets traced along a direction field ([`super::tensor`]).
//!
//! Main streets follow the field's main direction, cross streets the one at
//! right angles. Each street starts from a seed (usually a point on a
//! street already traced), runs both ways and stops at the edge of its area,
//! when it comes too close to a street of its own family, or when it grows
//! too long. Spacing comes from a function, so streets are tight in an old
//! town and loose at the edge. Dangling ends then reach for a nearby street
//! to join, unless left as dead ends.

use super::tensor::Field;
use glam::DVec2;
use hw_core::Rng;
use std::collections::HashMap;

/// What to trace, and where.
pub struct TraceSpec<'a> {
    pub field: &'a Field,
    /// Where streets may run.
    pub inside: &'a dyn Fn(DVec2) -> bool,
    /// Spacing between parallel streets at a point (m).
    pub dsep: &'a dyn Fn(DVec2) -> f64,
    /// Step along a street (m).
    pub step: f64,
    pub max_len: f64,
    /// Streets shorter than this are dropped.
    pub min_len: f64,
    /// How far a dangling end reaches to join another street, in spacings.
    pub join: f64,
    /// Chance that a dangling end is left as a dead end.
    pub dead_ends: f64,
    /// Seeds tried at random once the queues run dry.
    pub random_seeds: usize,
}

/// A traced street.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub pts: Vec<DVec2>,
    /// Main direction (true) or cross direction.
    pub major: bool,
}

/// Points of streets, bucketed for neighbour queries.
struct Hash {
    cell: f64,
    map: HashMap<(i64, i64), Vec<(DVec2, u8)>>,
}

/// Family tags: main streets, cross streets, given roads.
const MAJOR: u8 = 0;
const MINOR: u8 = 1;
const GIVEN: u8 = 2;

impl Hash {
    fn key(&self, p: DVec2) -> (i64, i64) {
        (
            (p.x / self.cell).floor() as i64,
            (p.y / self.cell).floor() as i64,
        )
    }

    fn add(&mut self, p: DVec2, tag: u8) {
        let k = self.key(p);
        self.map.entry(k).or_default().push((p, tag));
    }

    /// Is a point of one of `tags` within `r` of `p`?
    fn near(&self, p: DVec2, r: f64, tags: &[u8]) -> bool {
        let n = (r / self.cell).ceil() as i64;
        let (ki, kj) = self.key(p);
        for j in kj - n..=kj + n {
            for i in ki - n..=ki + n {
                if let Some(v) = self.map.get(&(i, j)) {
                    if v.iter()
                        .any(|(q, t)| tags.contains(t) && q.distance_squared(p) < r * r)
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// The nearest point within `r` of `p` lying ahead along `dir` (within
    /// 60°), to join a dangling end to.
    fn ahead(&self, p: DVec2, dir: DVec2, r: f64) -> Option<DVec2> {
        let n = (r / self.cell).ceil() as i64;
        let (ki, kj) = self.key(p);
        let mut best: Option<(f64, DVec2)> = None;
        for j in kj - n..=kj + n {
            for i in ki - n..=ki + n {
                let Some(v) = self.map.get(&(i, j)) else {
                    continue;
                };
                for (q, _) in v {
                    let d = *q - p;
                    let l = d.length();
                    if l < 1e-6 || l > r || d.dot(dir) < l * 0.5 {
                        continue;
                    }
                    if best.is_none_or(|b| l < b.0) {
                        best = Some((l, *q));
                    }
                }
            }
        }
        best.map(|b| b.1)
    }
}

/// Traces streets over `given` roads (which they join but never duplicate),
/// starting from `seeds`.
pub fn trace(spec: &TraceSpec, given: &[Vec<DVec2>], seeds: &[DVec2], rng: &mut Rng) -> Vec<Line> {
    let probe = (spec.dsep)(seeds.first().copied().unwrap_or(DVec2::ZERO));
    let mut hash = Hash {
        cell: probe.max(spec.step * 2.0),
        map: HashMap::new(),
    };
    for g in given {
        for p in resample(g, spec.step) {
            hash.add(p, GIVEN);
        }
    }
    let mut lines: Vec<Line> = Vec::new();
    let mut queue: [Vec<DVec2>; 2] = [seeds.to_vec(), seeds.to_vec()];
    // Cross streets start off the given roads too.
    for g in given {
        let pts = resample(g, probe);
        queue[MAJOR as usize].extend(pts.iter().copied());
        queue[MINOR as usize].extend(pts);
    }
    let mut random_left = spec.random_seeds;
    let mut family = MAJOR;
    let mut idle = 0;
    while idle < 2 {
        let major = family == MAJOR;
        let seed = next_seed(spec, &hash, &mut queue[family as usize], family).or_else(|| {
            while random_left > 0 {
                random_left -= 1;
                let p = random_point(spec, rng);
                if let Some(p) = p {
                    if valid_seed(spec, &hash, p, family) {
                        return Some(p);
                    }
                }
            }
            None
        });
        let Some(seed) = seed else {
            idle += 1;
            family = 1 - family;
            continue;
        };
        idle = 0;
        if let Some(pts) = trace_one(spec, &hash, seed, major) {
            for p in &pts {
                hash.add(*p, family);
            }
            // Streets of the other family start along this one.
            let d = (spec.dsep)(seed);
            queue[1 - family as usize].extend(resample(&pts, d));
            lines.push(Line { pts, major });
        }
        family = 1 - family;
    }
    // Dangling ends reach for a street to join.
    let mut all = Hash {
        cell: hash.cell,
        map: HashMap::new(),
    };
    for g in given {
        for p in resample(g, spec.step) {
            all.add(p, GIVEN);
        }
    }
    for l in &lines {
        for p in &l.pts {
            all.add(*p, MAJOR);
        }
    }
    for l in &mut lines {
        for end in [false, true] {
            let n = l.pts.len();
            let (p, q) = if end {
                (l.pts[n - 1], l.pts[n - 2])
            } else {
                (l.pts[0], l.pts[1])
            };
            if rng.chance(spec.dead_ends) {
                continue;
            }
            let dir = (p - q).normalize_or_zero();
            let reach = (spec.dsep)(p) * spec.join;
            // Skip its own last stretch.
            let from = p + dir * spec.step * 1.5;
            if let Some(t) = all.ahead(from, dir, reach) {
                if (spec.inside)(p.lerp(t, 0.5)) {
                    if end {
                        l.pts.push(t);
                    } else {
                        l.pts.insert(0, t);
                    }
                }
            }
        }
    }
    lines
}

fn valid_seed(spec: &TraceSpec, hash: &Hash, p: DVec2, family: u8) -> bool {
    let d = (spec.dsep)(p);
    (spec.inside)(p) && !hash.near(p, d, &[family]) && !hash.near(p, d * 0.3, &[GIVEN])
}

fn next_seed(spec: &TraceSpec, hash: &Hash, q: &mut Vec<DVec2>, family: u8) -> Option<DVec2> {
    while let Some(p) = q.pop() {
        if valid_seed(spec, hash, p, family) {
            return Some(p);
        }
    }
    None
}

fn random_point(spec: &TraceSpec, rng: &mut Rng) -> Option<DVec2> {
    // Callers bound the area through `inside`; sample a wide square.
    let r = spec.max_len;
    let p = DVec2::new(rng.range(-r, r), rng.range(-r, r));
    (spec.inside)(p).then_some(p)
}

/// One street through `seed`, both ways.
fn trace_one(spec: &TraceSpec, hash: &Hash, seed: DVec2, major: bool) -> Option<Vec<DVec2>> {
    let fam = if major { MAJOR } else { MINOR };
    let t0 = spec.field.eval(seed);
    if t0.weight() < 1e-6 {
        return None;
    }
    let d0 = t0.dir(major);
    let fwd = run(spec, hash, seed, d0, major, fam);
    let back = run(spec, hash, seed, -d0, major, fam);
    let mut pts: Vec<DVec2> = back.into_iter().rev().collect();
    pts.push(seed);
    pts.extend(fwd);
    let len: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
    (pts.len() >= 2 && len >= spec.min_len).then_some(pts)
}

/// Follows the field from `start` along `dir` until a stop condition.
fn run(
    spec: &TraceSpec,
    hash: &Hash,
    start: DVec2,
    dir: DVec2,
    major: bool,
    fam: u8,
) -> Vec<DVec2> {
    let mut out = Vec::new();
    let mut p = start;
    let mut prev = dir;
    let mut len = 0.0;
    let h = spec.step;
    let aligned = |p: DVec2, prev: DVec2| -> Option<DVec2> {
        let t = spec.field.eval(p);
        if t.weight() < 1e-6 {
            return None;
        }
        let d = t.dir(major);
        Some(if d.dot(prev) < 0.0 { -d } else { d })
    };
    while len < spec.max_len * 0.5 {
        let Some(d1) = aligned(p, prev) else { break };
        let Some(d2) = aligned(p + d1 * h * 0.5, d1) else {
            break;
        };
        let next = p + d2 * h;
        if !(spec.inside)(next) {
            break;
        }
        let sep = (spec.dsep)(next) * 0.5;
        if hash.near(next, sep, &[fam]) {
            break;
        }
        // Meeting a given road: end on it.
        if hash.near(next, h * 0.75, &[GIVEN]) && len > h * 2.0 {
            out.push(next);
            break;
        }
        // Closing a loop on itself.
        if out.len() > 8
            && out[..out.len() - 6]
                .iter()
                .any(|q: &DVec2| q.distance(next) < h * 0.9)
        {
            break;
        }
        out.push(next);
        len += h;
        prev = d2;
        p = next;
    }
    out
}

/// Points along a polyline every `d` metres (ends included).
pub fn resample(pts: &[DVec2], d: f64) -> Vec<DVec2> {
    let mut out = Vec::new();
    if pts.is_empty() {
        return out;
    }
    out.push(pts[0]);
    let mut carry = 0.0;
    for w in pts.windows(2) {
        let l = w[0].distance(w[1]);
        let mut t = d - carry;
        while t <= l {
            out.push(w[0].lerp(w[1], t / l));
            t += d;
        }
        carry = l - (t - d);
    }
    if out.last() != pts.last() {
        out.push(*pts.last().expect("non-empty"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::tensor::{Basis, Field};
    use super::*;
    use hw_core::{Noise, Seed};

    #[test]
    fn a_grid_field_traces_a_grid() {
        let field = Field {
            basis: vec![Basis::Grid {
                center: DVec2::ZERO,
                angle: 0.0,
                reach: 1e6,
                weight: 1.0,
            }],
            noise: Noise::new(Seed(1)),
            wiggle_len: 100.0,
            wiggle: Box::new(|_| 0.0),
        };
        let inside = |p: DVec2| p.x.abs() < 200.0 && p.y.abs() < 200.0;
        let dsep = |_: DVec2| 50.0;
        let spec = TraceSpec {
            field: &field,
            inside: &inside,
            dsep: &dsep,
            step: 5.0,
            max_len: 1000.0,
            min_len: 20.0,
            join: 1.5,
            dead_ends: 0.0,
            random_seeds: 20,
        };
        let lines = trace(&spec, &[], &[DVec2::ZERO], &mut Seed(2).rng());
        let major = lines.iter().filter(|l| l.major).count();
        let minor = lines.len() - major;
        // About 400 / 50 = 8 streets each way.
        assert!(
            (5..=10).contains(&major) && (5..=10).contains(&minor),
            "{major} {minor}"
        );
        for l in &lines {
            let d = *l.pts.last().unwrap() - l.pts[0];
            if l.major {
                assert!(d.y.abs() < 1.0, "main streets run east-west");
            } else {
                assert!(d.x.abs() < 1.0, "cross streets run north-south");
            }
        }
    }
}
