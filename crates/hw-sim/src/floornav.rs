//! Villains find their way through the game-scale interiors: a route map
//! made from each building's floor plan ([`hw_gen::layout`]), not from a
//! grid over its solids.
//!
//! The map's points are where one goes from one floor area to another:
//! both sides of every doorway, the foot and head of every flight of
//! stairs, where landings meet, the street in front of each entrance and
//! the lift doors at every stop. Every point stands on one or more *pads*,
//! the flat areas one walks on (a room's floor, a landing, a bridge), and
//! any two points on one pad are joined: a pad is a rectangle, so the
//! straight walk between them stays on it. A search over a few hundred
//! points gives the doorways and stairs to head for; inside a room the
//! walker goes round desks and columns with the street search of
//! [`crate::nav`], which only ever looks as far as the next point.
//!
//! The map is made once per building, in the layout's own frame, and turns
//! points to and from the town's frame as it answers.

use crate::nav;
use glam::{DVec2, DVec3};
use hw_gen::geom::Rect;
use hw_gen::layout::lifts::{SPEED as LIFT_SPEED, WAIT as LIFT_WAIT};
use hw_gen::layout::{Flight, Layout, LinkKind, SpaceKind};
use hw_gen::urban::landmarks::Landmark;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Points this far either side of a doorway (m): one walks through it
/// square, not along the wall.
const DOOR_SIDE: f64 = 0.9;
/// The street point this far out of an entrance (m).
const OUT_FRONT: f64 = 2.0;
/// A flight's foot and head points this far off its first and last step
/// (m), on the floor it starts from and the one it reaches.
const OFF_STEPS: f64 = 0.7;
/// Wide openings (a gallery open to the atrium) get a point every this
/// many metres along them.
const WIDE: f64 = 7.0;
/// How near (m) a point counts as on a pad's outline.
const ON: f64 = 0.05;
/// How far over a pad (m) a body's feet may be and still stand on it
/// (floor finishes, the half step of a landing's nosing).
const FEET_ABOVE: f64 = 0.45;
/// A stair's steps or landing this low over a pad (m) are in the way of
/// walking across it.
const LOW: f64 = 2.2;
/// Walks keep this far off a stair in the way (m), and go round it by
/// points this far off its corners.
const BLOCK_MARGIN: f64 = 0.4;
const ROUND: f64 = 0.9;
/// Arcs are walked as points this many radians apart.
const ARC_STEP: f64 = 0.2;
/// A lift ride costs its travel plus this share of a full wait, on top
/// of walking (it is seldom quicker than the stairs over one floor).
const LIFT_WAIT_SHARE: f64 = 1.5;

/// How an edge is walked.
#[derive(Clone, Debug, PartialEq)]
pub enum Via {
    /// Across one pad.
    Walk,
    /// Up or down a flight: the points between its ends.
    Steps(Vec<DVec3>),
    /// In a lift car: which lift (in the layout), from stop and to stop.
    Lift { lift: usize, from: usize, to: usize },
}

#[derive(Clone, Debug)]
struct Edge {
    to: usize,
    cost: f64,
    via: Via,
}

/// A flat area one walks on.
#[derive(Clone, Debug)]
struct Pad {
    rect: Rect,
    y: f64,
    /// The space it belongs to, and the tall space that holds it (landings
    /// and bridges share their host's).
    host: usize,
    nodes: Vec<usize>,
    /// Stairs in the way of walking across it.
    blocks: Vec<Rect>,
    /// What the space is (None for a stair's landing).
    kind: Option<SpaceKind>,
}

/// A flight as a walk: its points from foot to head.
#[derive(Clone, Debug)]
struct Steps {
    foot: usize,
    head: usize,
    points: Vec<DVec3>,
    footprint: Rect,
    y0: f64,
    y1: f64,
}

/// One building's route map, in its layout's frame.
#[derive(Clone, Debug)]
pub struct FloorGraph {
    pub size: DVec2,
    nodes: Vec<DVec3>,
    /// Points on the street in front of an entrance.
    outside: Vec<usize>,
    /// The widest entrance's street point (and its width).
    main: Option<(f64, usize)>,
    edges: Vec<Vec<Edge>>,
    pads: Vec<Pad>,
    steps: Vec<Steps>,
}

/// One leg of a route: the point to walk to, and how.
#[derive(Clone, Debug, PartialEq)]
pub struct Leg {
    pub to: DVec3,
    pub via: Via,
}

/// Where a point is in a building.
#[derive(Clone, Debug)]
enum Where {
    /// Standing on these pads.
    On(Vec<usize>),
    /// On a flight (its index), this far along its points.
    Stair(usize, usize),
    /// Outside the building.
    Out,
}

impl FloorGraph {
    /// The route map of a layout.
    pub fn new(l: &Layout) -> Self {
        let mut g = FloorGraph {
            size: l.size,
            nodes: Vec::new(),
            outside: Vec::new(),
            main: None,
            edges: Vec::new(),
            pads: Vec::new(),
            steps: Vec::new(),
        };
        // Pads: every floor (lift shafts are not walked), and every
        // landing of a stair.
        for (i, s) in l.spaces.iter().enumerate() {
            if s.kind == SpaceKind::Shaft {
                continue;
            }
            g.pads.push(Pad {
                rect: s.rect,
                y: s.y,
                host: s.within.unwrap_or(i),
                nodes: Vec::new(),
                blocks: Vec::new(),
                kind: Some(s.kind),
            });
        }
        for st in &l.stairs {
            for la in &st.landings {
                g.pads.push(Pad {
                    rect: la.rect,
                    y: la.y,
                    host: st.host,
                    nodes: Vec::new(),
                    blocks: Vec::new(),
                    kind: None,
                });
            }
        }
        // Stairs in the way on each pad (see `blocks`).
        let blocks: Vec<Vec<Rect>> = (0..g.pads.len()).map(|p| g.blocks(l, p)).collect();
        for (p, bl) in blocks.iter().enumerate() {
            g.pads[p].blocks = bl.clone();
        }
        // Doorways: a point on each side, joined; wide openings get one
        // pair every few metres (but none right beside a stair).
        let mut lift_doors: Vec<(usize, f64, usize)> = Vec::new();
        for link in &l.links {
            if !link.kind.passable() {
                continue;
            }
            let (a, b) = link.ends();
            let along = (b - a).normalize_or(DVec2::X);
            let n = along.perp();
            let ca = l.spaces[link.a].rect.center();
            // `n` points out of space `a`.
            let n = if (ca - link.at).dot(n) > 0.0 { -n } else { n };
            let count = ((link.width / WIDE).floor() as usize).max(1);
            for k in 0..count {
                let at = a + along * (link.width * (k as f64 + 0.5) / count as f64);
                let y = link.y;
                let shaft_a = l.spaces[link.a].kind == SpaceKind::Shaft;
                let shaft_b = link.b.is_some_and(|b| l.spaces[b].kind == SpaceKind::Shaft);
                if link.kind == LinkKind::Entrance {
                    let inside = g.add_on_pads(DVec3::new(
                        at.x - n.x * DOOR_SIDE,
                        y,
                        at.y - n.y * DOOR_SIDE,
                    ));
                    let out = g.add(DVec3::new(
                        at.x + n.x * OUT_FRONT,
                        y,
                        at.y + n.y * OUT_FRONT,
                    ));
                    g.outside.push(out);
                    if g.main.is_none_or(|(w, _)| link.width > w + 1e-6) {
                        g.main = Some((link.width, out));
                    }
                    if let Some(i) = inside {
                        g.join(i, out, Via::Walk);
                    }
                    continue;
                }
                let pa = DVec3::new(at.x - n.x * DOOR_SIDE, y, at.y - n.y * DOOR_SIDE);
                let pb = DVec3::new(at.x + n.x * DOOR_SIDE, y, at.y + n.y * DOOR_SIDE);
                if count > 1 && (g.in_block(pa) || g.in_block(pb)) {
                    continue;
                }
                if shaft_a || shaft_b {
                    // A lift door: the point on the landing side only.
                    let (shaft, landing) = if shaft_a {
                        (link.a, pb)
                    } else {
                        (link.b.unwrap_or(0), pa)
                    };
                    if let Some(i) = g.add_on_pads(landing) {
                        lift_doors.push((shaft, y, i));
                    }
                    continue;
                }
                // One side over a stair (an opening onto the head of a
                // flight) has no point: the flight's head point joins it.
                if let (Some(i), Some(j)) = (g.add_on_pads(pa), g.add_on_pads(pb)) {
                    g.join(i, j, Via::Walk);
                }
            }
        }
        // Flights: foot and head points, joined by the walk up the steps.
        for st in &l.stairs {
            for f in &st.flights {
                let points = walk_points(f);
                let (first, last) = (points[0], *points.last().expect("points"));
                let foot = g.add_on_pads(first).unwrap_or_else(|| g.add(first));
                let head = g.add_on_pads(last).unwrap_or_else(|| g.add(last));
                let inner = points[1..points.len() - 1].to_vec();
                let len: f64 = points.windows(2).map(|w| w[0].distance(w[1])).sum();
                g.link(foot, head, len, Via::Steps(inner.clone()));
                g.link(
                    head,
                    foot,
                    len,
                    Via::Steps(inner.into_iter().rev().collect()),
                );
                g.steps.push(Steps {
                    foot,
                    head,
                    points,
                    footprint: f.footprint(),
                    y0: f.y0(),
                    y1: f.y1(),
                });
            }
        }
        // A flight's end off every pad (one flight running straight into
        // the next) meets the other flights' ends near it.
        let ends: Vec<usize> = g.steps.iter().flat_map(|s| [s.foot, s.head]).collect();
        for &i in &ends {
            if g.pads.iter().any(|p| p.nodes.contains(&i)) {
                continue;
            }
            for &j in &ends {
                let (p, q) = (g.nodes[i], g.nodes[j]);
                if i != j && (p.y - q.y).abs() < 0.05 && p.distance(q) < 2.0 * OFF_STEPS + 0.2 {
                    g.join(i, j, Via::Walk);
                }
            }
        }
        // Pads of one tall space at one height that touch (the two parts
        // of a spiral's landing, a bridge and a landing): a point where
        // they meet.
        for i in 0..g.pads.len() {
            for j in i + 1..g.pads.len() {
                let (a, b) = (&g.pads[i], &g.pads[j]);
                if a.host != b.host || (a.y - b.y).abs() > 1e-3 {
                    continue;
                }
                let lo = a.rect.min.max(b.rect.min);
                let hi = a.rect.max.min(b.rect.max);
                if hi.x < lo.x - ON || hi.y < lo.y - ON {
                    continue;
                }
                let c = (lo + hi) * 0.5;
                let y = a.y;
                // Where they meet along an edge, a point either side of
                // it (as at a doorway), so one crosses it square and
                // not round the end of a rail.
                let across = if hi.x - lo.x < 2.0 * ON {
                    Some(DVec2::X)
                } else if hi.y - lo.y < 2.0 * ON {
                    Some(DVec2::Y)
                } else {
                    None
                };
                let sides = across.and_then(|n| {
                    let (p, q) = (c - n * DOOR_SIDE, c + n * DOOR_SIDE);
                    let on = |r: &Rect, v: DVec2| r.inset(-ON).contains_point(v);
                    let (ra, rb) = (g.pads[i].rect, g.pads[j].rect);
                    ((on(&ra, p) && on(&rb, q)) || (on(&ra, q) && on(&rb, p))).then_some((p, q))
                });
                match sides {
                    Some((p, q)) => {
                        let (p, q) = (DVec3::new(p.x, y, p.y), DVec3::new(q.x, y, q.y));
                        if let (Some(u), Some(v)) = (g.add_on_pads(p), g.add_on_pads(q)) {
                            g.join(u, v, Via::Walk);
                        }
                    }
                    None => {
                        g.add_on_pads(DVec3::new(c.x, y, c.y));
                    }
                }
            }
        }
        // Lifts: a ride between any two stops.
        for (k, lift) in l.lifts.iter().enumerate() {
            let doors: Vec<(usize, usize)> = lift
                .stops
                .iter()
                .enumerate()
                .filter_map(|(s, y)| {
                    lift_doors
                        .iter()
                        .find(|d| d.0 == lift.shaft && (d.1 - y).abs() < 1e-3)
                        .map(|d| (s, d.2))
                })
                .collect();
            for &(s, i) in &doors {
                for &(t, j) in &doors {
                    if s != t {
                        let ride = (lift.stops[s] - lift.stops[t]).abs() / LIFT_SPEED
                            + LIFT_WAIT * LIFT_WAIT_SHARE;
                        // Seconds into metres at a villain's walk.
                        g.link(
                            i,
                            j,
                            ride * 3.0,
                            Via::Lift {
                                lift: k,
                                from: s,
                                to: t,
                            },
                        );
                    }
                }
            }
        }
        // Stairs standing on a pad are in the way of a walk across it (the
        // low steps trip one up, a low landing is a wall): such walks go
        // round them, by points off their corners.
        for (p, bl) in blocks.iter().enumerate() {
            let y = g.pads[p].y;
            let room = g.pads[p].rect.inset(0.6);
            for b in bl {
                let r = b.inset(-(ROUND - BLOCK_MARGIN));
                for c in [
                    r.min,
                    DVec2::new(r.max.x, r.min.y),
                    r.max,
                    DVec2::new(r.min.x, r.max.y),
                ] {
                    if room.contains_point(c) && !bl.iter().any(|o| o.contains_point(c)) {
                        g.add_on_pads(DVec3::new(c.x, y, c.y));
                    }
                }
            }
        }
        // Everything on one pad is joined, unless a stair is in between.
        for (p, bl) in blocks.iter().enumerate() {
            let nodes = g.pads[p].nodes.clone();
            for (x, &i) in nodes.iter().enumerate() {
                for &j in &nodes[x + 1..] {
                    let (a, b) = (g.nodes[i], g.nodes[j]);
                    let (a, b) = (DVec2::new(a.x, a.z), DVec2::new(b.x, b.z));
                    if !bl.iter().any(|r| crosses(r, a, b)) {
                        g.join(i, j, Via::Walk);
                    }
                }
            }
        }
        g
    }

    /// The stairs in the way on pad `p`: flights rising from it (or
    /// starting low over it), flights coming up through it, and landings
    /// too low to walk under; each with a margin.
    fn blocks(&self, l: &Layout, p: usize) -> Vec<Rect> {
        let pad = &self.pads[p];
        let over = |r: &Rect| r.overlap(&pad.rect) > 1e-3;
        let mut out = Vec::new();
        for st in &l.stairs {
            for f in &st.flights {
                // Round stairs are walked round by their own points.
                if matches!(f, Flight::Arc { .. }) {
                    continue;
                }
                let fp = f.footprint();
                let rises = f.y0() > pad.y - 0.05 && f.y0() < pad.y + LOW;
                let arrives = (f.y1() - pad.y).abs() < 0.05 && f.y0() < pad.y - 0.05;
                if (rises || arrives) && over(&fp) {
                    out.push(fp.inset(-BLOCK_MARGIN));
                }
            }
            if st.host == pad.host {
                for la in &st.landings {
                    if la.y > pad.y + 0.05 && la.y < pad.y + LOW && over(&la.rect) {
                        out.push(la.rect.inset(-BLOCK_MARGIN));
                    }
                }
            }
        }
        out
    }

    fn add(&mut self, p: DVec3) -> usize {
        self.nodes.push(p);
        self.edges.push(Vec::new());
        self.nodes.len() - 1
    }

    /// A point on the pads under it; None if it stands on none.
    fn add_on_pads(&mut self, p: DVec3) -> Option<usize> {
        let pads = self.pads_at(p, 1e-3);
        if pads.is_empty() {
            return None;
        }
        let i = self.add(p);
        for k in pads {
            self.pads[k].nodes.push(i);
        }
        Some(i)
    }

    /// The pads a point at feet height `p.y` stands on.
    fn pads_at(&self, p: DVec3, above: f64) -> Vec<usize> {
        let q = DVec2::new(p.x, p.z);
        (0..self.pads.len())
            .filter(|&k| {
                let pad = &self.pads[k];
                p.y >= pad.y - 0.05
                    && p.y <= pad.y + above.max(0.05)
                    && pad.rect.inset(-ON).contains_point(q)
            })
            .collect()
    }

    fn link(&mut self, i: usize, j: usize, cost: f64, via: Via) {
        if !self.edges[i].iter().any(|e| e.to == j && e.via == via) {
            self.edges[i].push(Edge { to: j, cost, via });
        }
    }

    fn join(&mut self, i: usize, j: usize, via: Via) {
        let d = self.nodes[i].distance(self.nodes[j]);
        self.link(i, j, d, via.clone());
        self.link(j, i, d, via);
    }

    /// Does `p` stand in a stair's way on a pad under it?
    fn in_block(&self, p: DVec3) -> bool {
        let q = DVec2::new(p.x, p.z);
        self.pads_at(p, 1e-3)
            .iter()
            .any(|&k| self.pads[k].blocks.iter().any(|r| r.contains_point(q)))
    }

    /// Is the walk from `a` to `b` across `pad` clear of its stairs?
    fn clear(&self, pad: &Pad, a: DVec3, b: DVec3) -> bool {
        let (a, b) = (DVec2::new(a.x, a.z), DVec2::new(b.x, b.z));
        !pad.blocks.iter().any(|r| crosses(r, a, b))
    }

    /// Is `p` (layout frame) inside the building's outline?
    pub fn inside(&self, p: DVec3) -> bool {
        Rect::new(DVec2::ZERO, self.size).contains_point(DVec2::new(p.x, p.z))
    }

    fn locate(&self, p: DVec3) -> Option<Where> {
        if !self.inside(p) {
            return Some(Where::Out);
        }
        let q = DVec2::new(p.x, p.z);
        // On a flight's steps (not at its very foot or head).
        for (k, s) in self.steps.iter().enumerate() {
            if s.footprint.contains_point(q) && p.y > s.y0 + 0.1 && p.y < s.y1 - 0.1 {
                // The stretch of the walk it is on: from point `near` to
                // the next.
                let near = (0..s.points.len() - 1)
                    .min_by(|&a, &b| {
                        seg_dist(p, s.points[a], s.points[a + 1]).total_cmp(&seg_dist(
                            p,
                            s.points[b],
                            s.points[b + 1],
                        ))
                    })
                    .expect("points");
                return Some(Where::Stair(k, near));
            }
        }
        let on = self.pads_at(p, FEET_ABOVE);
        if !on.is_empty() {
            return Some(Where::On(on));
        }
        // In the air (jumping, flying, falling): the highest pad under it.
        let under = (0..self.pads.len())
            .filter(|&k| {
                let pad = &self.pads[k];
                pad.y <= p.y + 0.05 && pad.rect.contains_point(q)
            })
            .max_by(|&a, &b| self.pads[a].y.total_cmp(&self.pads[b].y))?;
        let y = self.pads[under].y;
        Some(Where::On(self.pads_at(DVec3::new(p.x, y, p.z), 0.05)))
    }

    /// Ways into the graph from `p`: (point, cost to get there, the legs
    /// walked before it).
    fn entries(&self, p: DVec3, w: &Where) -> Vec<(usize, f64, Vec<Leg>)> {
        let flat = |a: DVec3, b: DVec3| DVec2::new(a.x - b.x, a.z - b.z).length();
        match w {
            Where::Out => self
                .outside
                .iter()
                .map(|&i| (i, flat(p, self.nodes[i]), Vec::new()))
                .collect(),
            Where::On(pads) => {
                let mut v: Vec<usize> = pads
                    .iter()
                    .flat_map(|&k| {
                        let pad = &self.pads[k];
                        pad.nodes
                            .iter()
                            .copied()
                            .filter(|&i| self.clear(pad, p, self.nodes[i]))
                            .collect::<Vec<_>>()
                    })
                    .collect();
                v.sort_unstable();
                v.dedup();
                v.into_iter()
                    .map(|i| (i, flat(p, self.nodes[i]), Vec::new()))
                    .collect()
            }
            Where::Stair(k, at) => {
                let s = &self.steps[*k];
                let up: Vec<DVec3> = std::iter::once(p)
                    .chain(s.points[*at + 1..].iter().copied())
                    .collect();
                let down: Vec<DVec3> = std::iter::once(p)
                    .chain(s.points[..=*at].iter().rev().copied())
                    .collect();
                let len = |v: &[DVec3]| v.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>();
                let legs = |v: &[DVec3]| {
                    v[1..v.len() - 1]
                        .iter()
                        .map(|q| Leg {
                            to: *q,
                            via: Via::Walk,
                        })
                        .collect::<Vec<_>>()
                };
                vec![
                    (s.head, len(&up), legs(&up)),
                    (s.foot, len(&down), legs(&down)),
                ]
            }
        }
    }

    /// A route from `from` to `to` (both in the layout's frame): the legs
    /// to walk, ending at `to`. None when there is no need for one (both
    /// stand on one pad, or both are outside) or no way at all.
    pub fn route(&self, from: DVec3, to: DVec3, lifts: bool) -> Option<Vec<Leg>> {
        let wf = self.locate(from)?;
        let wt = self.locate(to)?;
        match (&wf, &wt) {
            (Where::Out, Where::Out) => return None,
            (Where::On(a), Where::On(b))
                if a.iter()
                    .any(|&k| b.contains(&k) && self.clear(&self.pads[k], from, to)) =>
            {
                return None
            }
            _ => {}
        }
        let starts = self.entries(from, &wf);
        // Ends, the other way round: the legs from the graph to `to`.
        let ends: Vec<(usize, f64, Vec<Leg>)> = self
            .entries(to, &wt)
            .into_iter()
            .map(|(i, c, legs)| {
                // Legs out of the graph are the same points reversed.
                let mut pts: Vec<DVec3> = legs.iter().map(|l| l.to).collect();
                pts.reverse();
                let legs = pts
                    .into_iter()
                    .map(|q| Leg {
                        to: q,
                        via: Via::Walk,
                    })
                    .collect();
                (i, c, legs)
            })
            .collect();
        let n = self.nodes.len();
        let mut cost = vec![f64::INFINITY; n];
        let mut came: Vec<Option<(usize, usize)>> = vec![None; n];
        let mut start_of = vec![usize::MAX; n];
        let mut open = BinaryHeap::new();
        let key = |c: f64| (c * 1000.0) as u64;
        for (k, (i, c, _)) in starts.iter().enumerate() {
            if *c < cost[*i] {
                cost[*i] = *c;
                start_of[*i] = k;
                open.push(Reverse((key(*c), *i)));
            }
        }
        let mut end_cost = vec![f64::INFINITY; n];
        let mut end_of = vec![usize::MAX; n];
        for (k, (i, c, _)) in ends.iter().enumerate() {
            if *c < end_cost[*i] {
                end_cost[*i] = *c;
                end_of[*i] = k;
            }
        }
        let mut best: Option<(f64, usize)> = None;
        while let Some(Reverse((c, i))) = open.pop() {
            if c > key(cost[i]) {
                continue;
            }
            if best.is_some_and(|(b, _)| cost[i] >= b) {
                break;
            }
            if end_of[i] != usize::MAX {
                let total = cost[i] + end_cost[i];
                if best.is_none_or(|(b, _)| total < b) {
                    best = Some((total, i));
                }
            }
            for (k, e) in self.edges[i].iter().enumerate() {
                if !lifts && matches!(e.via, Via::Lift { .. }) {
                    continue;
                }
                let nc = cost[i] + e.cost;
                if nc + 1e-9 < cost[e.to] {
                    cost[e.to] = nc;
                    came[e.to] = Some((i, k));
                    start_of[e.to] = start_of[i];
                    open.push(Reverse((key(nc), e.to)));
                }
            }
        }
        let (_, last) = best?;
        let mut chain: Vec<Leg> = Vec::new();
        let mut at = last;
        while let Some((prev, k)) = came[at] {
            let e = &self.edges[prev][k];
            let mut legs = Vec::new();
            if let Via::Steps(pts) = &e.via {
                legs.extend(pts.iter().map(|q| Leg {
                    to: *q,
                    via: Via::Walk,
                }));
            }
            let via = match &e.via {
                Via::Steps(_) => Via::Walk,
                v => v.clone(),
            };
            legs.push(Leg {
                to: self.nodes[at],
                via,
            });
            legs.extend(chain);
            chain = legs;
            at = prev;
        }
        let mut out = starts[start_of[last]].2.clone();
        out.push(Leg {
            to: self.nodes[at],
            via: Via::Walk,
        });
        out.extend(chain);
        out.extend(ends[end_of[last]].2.iter().cloned());
        out.push(Leg { to, via: Via::Walk });
        Some(out)
    }

    /// Every (point, pad) one can stand on, for the tests: the middle of
    /// each pad.
    pub fn pad_centres(&self) -> Vec<DVec3> {
        self.pads
            .iter()
            .map(|p| {
                let c = p.rect.center();
                DVec3::new(c.x, p.y, c.y)
            })
            .collect()
    }

    /// The points one can walk to (not the street ones), for the tests.
    pub fn points(&self) -> Vec<DVec3> {
        (0..self.nodes.len())
            .filter(|i| !self.outside.contains(i))
            .map(|i| self.nodes[i])
            .collect()
    }

    /// The rooms: each space's kind, floor plan and floor height.
    pub fn rooms(&self) -> Vec<(SpaceKind, Rect, f64)> {
        self.pads
            .iter()
            .filter_map(|p| p.kind.map(|k| (k, p.rect, p.y)))
            .collect()
    }

    /// The street in front of the main way in (the widest; the first of
    /// equals).
    pub fn front(&self) -> DVec3 {
        self.main
            .map_or(DVec3::new(self.size.x * 0.5, 0.0, -OUT_FRONT), |(_, i)| {
                self.nodes[i]
            })
    }

    /// How many points the map has.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// How far `p` is from the segment a-b.
fn seg_dist(p: DVec3, a: DVec3, b: DVec3) -> f64 {
    let d = b - a;
    let t = ((p - a).dot(d) / d.length_squared().max(1e-9)).clamp(0.0, 1.0);
    p.distance(a + d * t)
}

/// Does the segment a-b pass through the inside of `r`?
fn crosses(r: &Rect, a: DVec2, b: DVec2) -> bool {
    let d = b - a;
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [
        (-d.x, a.x - r.min.x),
        (d.x, r.max.x - a.x),
        (-d.y, a.y - r.min.y),
        (d.y, r.max.y - a.y),
    ] {
        if p.abs() < 1e-12 {
            if q <= 0.0 {
                return false;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    t1 - t0 > 1e-6
}

/// A flight's walk: a point off its foot, points up the middle, a point
/// off its head.
fn walk_points(f: &Flight) -> Vec<DVec3> {
    match *f {
        Flight::Straight {
            from, dir, risers, ..
        } => {
            let a = DVec2::new(from.x, from.z);
            let p = a - dir * OFF_STEPS;
            let q = a + dir * (Flight::run(risers) + OFF_STEPS);
            vec![DVec3::new(p.x, f.y0(), p.y), DVec3::new(q.x, f.y1(), q.y)]
        }
        Flight::Arc {
            centre,
            r_in,
            r_out,
            a0,
            sweep,
            ..
        } => {
            let r = (r_in + r_out) * 0.5;
            let over = OFF_STEPS / r * sweep.signum();
            let n = ((sweep.abs() / ARC_STEP).ceil() as usize).max(2);
            let mut v = vec![{
                let p = centre + hw_core::det::dir2(a0 - over) * r;
                DVec3::new(p.x, f.y0(), p.y)
            }];
            for i in 0..=n {
                let t = i as f64 / n as f64;
                let p = centre + hw_core::det::dir2(a0 + sweep * t) * r;
                v.push(DVec3::new(p.x, f.y0() + (f.y1() - f.y0()) * t, p.y));
            }
            let p = centre + hw_core::det::dir2(a0 + sweep + over) * r;
            v.push(DVec3::new(p.x, f.y1(), p.y));
            v
        }
    }
}

/// How a building's layout sits in the town.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Frame {
    /// A landmark: its place and the way it faces.
    Landmark { pos: DVec2, dir: DVec2 },
    /// A building site: the layout's x runs `along` from `origin`, its z
    /// `inward` (see `BuildingSite::to_city`).
    Site {
        origin: DVec2,
        along: DVec2,
        inward: DVec2,
    },
}

/// A building in town with its route map.
#[derive(Clone, Debug)]
pub struct Building {
    pub frame: Frame,
    /// Floor height of the layout's ground.
    pub base: f64,
    pub graph: FloorGraph,
    /// Where the building's front door looks, and the middle of its
    /// outline (town frame, flat).
    pub dir: DVec2,
    pub pos: DVec2,
}

impl Building {
    /// A Comet building's map, if `l` is one.
    pub fn of(l: &Landmark) -> Option<Self> {
        let plan = hw_gen::urban::comet::layout(l)?;
        Some(Building {
            frame: Frame::Landmark {
                pos: l.pos,
                dir: l.dir,
            },
            base: l.base,
            graph: FloorGraph::new(&plan),
            dir: l.dir,
            pos: l.pos,
        })
    }

    /// A landmark-style building at `pos` facing `dir` (the layout's frame
    /// as the landmark's).
    pub fn at(pos: DVec2, dir: DVec2, base: f64, graph: FloorGraph) -> Self {
        Building {
            frame: Frame::Landmark { pos, dir },
            base,
            graph,
            dir,
            pos,
        }
    }

    /// A town building of halls' map.
    pub fn of_site(b: &hw_gen::urban::world::WorldBuilding) -> Option<Self> {
        let plan = b.site.layout()?;
        let s = &b.site;
        let graph = FloorGraph::new(&plan);
        let mut me = Building {
            frame: Frame::Site {
                origin: s.origin,
                along: s.along,
                inward: s.inward,
            },
            base: b.base,
            graph,
            dir: DVec2::ZERO,
            pos: DVec2::ZERO,
        };
        let mid = me.to_town(DVec3::new(
            me.graph.size.x * 0.5,
            0.0,
            me.graph.size.y * 0.5,
        ));
        let front = me.to_town(me.graph.front());
        me.pos = DVec2::new(mid.x, mid.z);
        me.dir = DVec2::new(front.x - mid.x, front.z - mid.z).normalize_or(DVec2::X);
        Some(me)
    }

    /// A town point in the layout's frame.
    pub fn to_layout(&self, p: DVec3) -> DVec3 {
        match self.frame {
            Frame::Landmark { pos, dir } => {
                // The landmark's frame: x to the right, z the way it faces.
                let x = DVec2::new(dir.y, -dir.x);
                let d = DVec2::new(p.x, p.z) - pos;
                let local = DVec2::new(d.dot(x), d.dot(dir));
                // The layout's frame from the landmark's: see `comet::point_to_local`.
                let q = self.graph.size * 0.5 - local;
                DVec3::new(q.x, p.y - self.base, q.y)
            }
            Frame::Site {
                origin,
                along,
                inward,
            } => {
                let d = DVec2::new(p.x, p.z) - origin;
                DVec3::new(d.dot(along), p.y - self.base, d.dot(inward))
            }
        }
    }

    /// A layout point in the town's frame.
    pub fn to_town(&self, p: DVec3) -> DVec3 {
        match self.frame {
            Frame::Landmark { pos, dir } => {
                let local = self.graph.size * 0.5 - DVec2::new(p.x, p.z);
                let x = DVec2::new(dir.y, -dir.x);
                let q = pos + x * local.x + dir * local.y;
                DVec3::new(q.x, p.y + self.base, q.y)
            }
            Frame::Site {
                origin,
                along,
                inward,
            } => {
                let q = origin + along * p.x + inward * p.z;
                DVec3::new(q.x, p.y + self.base, q.y)
            }
        }
    }

    /// Is a town point inside (or within `margin` m of) the outline?
    pub fn holds(&self, p: DVec3, margin: f64) -> bool {
        let q = self.to_layout(p);
        Rect::new(DVec2::ZERO, self.graph.size)
            .inset(-margin)
            .contains_point(DVec2::new(q.x, q.z))
            && q.y > -2.0
            && q.y < self.graph_height() + 2.0
    }

    fn graph_height(&self) -> f64 {
        self.graph.pads.iter().map(|p| p.y).fold(0.0, f64::max) + 10.0
    }
}

/// Every building with a route map in town.
#[derive(Clone, Debug, Default)]
pub struct Floors {
    pub buildings: Vec<Building>,
    /// Gang hideouts among the buildings: the index in `area.buildings` and the
    /// index in `buildings`.
    pub hideouts: Vec<(usize, usize)>,
}

impl Floors {
    /// Just these buildings, none of them a hideout.
    pub fn of(buildings: Vec<Building>) -> Self {
        Self {
            buildings,
            hideouts: Vec::new(),
        }
    }

    pub fn new(town: &hw_gen::urban::world::TownWorld) -> Self {
        Self {
            buildings: town.landmarks.iter().filter_map(Building::of).collect(),
            ..Self::default()
        }
    }

    /// The Comet buildings and the town buildings of halls in `area` that
    /// are gang hideouts (see `BuildingSite::is_hideout`), those last in
    /// the order of [`Self::hideouts`].
    pub fn with_hideouts(
        town: &hw_gen::urban::world::TownWorld,
        area: &hw_gen::urban::area::Area,
    ) -> Self {
        let mut me = Self::new(town);
        for (n, wb) in area.buildings.iter().enumerate() {
            if wb.site.is_hideout() {
                if let Some(b) = Building::of_site(wb) {
                    me.hideouts.push((n, me.buildings.len()));
                    me.buildings.push(b);
                }
            }
        }
        me
    }

    /// The building a town point is in (or right at the door of).
    pub fn building_at(&self, p: DVec3) -> Option<usize> {
        self.buildings.iter().position(|b| b.holds(p, 0.0))
    }

    /// A route between two town points of which at least one is in a
    /// building: the points to walk to, in the town's frame. None when the
    /// way needs no map (same room, or neither inside) or there is none.
    pub fn route(&self, from: DVec3, to: DVec3, lifts: bool) -> Option<Vec<Leg>> {
        let k = self.building_at(from).or_else(|| self.building_at(to))?;
        let b = &self.buildings[k];
        let legs = b.graph.route(b.to_layout(from), b.to_layout(to), lifts)?;
        Some(
            legs.into_iter()
                .map(|l| Leg {
                    to: b.to_town(l.to),
                    via: l.via,
                })
                .collect(),
        )
    }

    /// Which way to walk from `here` to `goal` when either is in a
    /// building: the points to head for. Straight on when the way is
    /// free; round furniture by the street search on one floor; by the
    /// route map between rooms and floors. None when neither is inside
    /// (or there is no way).
    pub fn way(
        &self,
        solids: &crate::collision::Obstacles,
        here: DVec3,
        goal: DVec3,
    ) -> Option<Vec<DVec3>> {
        let k = self.building_at(here).or_else(|| self.building_at(goal))?;
        let b = &self.buildings[k];
        if b.graph
            .route(b.to_layout(here), b.to_layout(goal), false)
            .is_some()
        {
            return self.path(solids, here, goal);
        }
        if (goal.y - here.y).abs() < 1.0 && nav::walkable_line(solids, here, goal) {
            return Some(vec![goal]);
        }
        nav::find_path(solids, here, goal)
    }

    /// The points to head for from `here` to `goal`, round furniture too:
    /// the route's points, with the first stretches that are not a
    /// straight walk filled in by the street search.
    pub fn path(
        &self,
        solids: &crate::collision::Obstacles,
        here: DVec3,
        goal: DVec3,
    ) -> Option<Vec<DVec3>> {
        let legs = self.route(here, goal, false)?;
        let mut out = Vec::new();
        let mut at = here;
        // Only the first two stretches: the path is found again every
        // second or two as the walker moves on.
        for (k, leg) in legs.iter().enumerate() {
            let flat = (leg.to.y - at.y).abs() < 0.3;
            if k < 2 && flat && !nav::walkable_line(solids, at, leg.to) {
                if let Some(p) = nav::find_path(solids, at, leg.to) {
                    out.extend(p);
                    at = leg.to;
                    continue;
                }
            }
            out.push(leg.to);
            at = leg.to;
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hw_core::Seed;
    use hw_gen::layout::office::plan_scheme;
    use hw_gen::layout::Scheme;

    fn layouts() -> Vec<Layout> {
        let mut out = Vec::new();
        for (k, scheme) in [Scheme::Atrium, Scheme::GrandHall, Scheme::SplitLevel]
            .into_iter()
            .enumerate()
        {
            for (w, d, n) in hw_gen::urban::comet::PLOTS {
                for seed in 0..3u64 {
                    if let Some(l) =
                        plan_scheme(Seed(seed * 11 + k as u64), DVec2::new(w, d), n, scheme)
                    {
                        out.push(l);
                    }
                }
            }
        }
        // Gangs' dens: narrow plots, a hall of balconies and bridges.
        for k in 0..12u64 {
            let size = DVec2::new(12.0 + (k % 6) as f64 * 0.5, 22.0 + (k % 5) as f64 * 2.5);
            out.extend(plan_scheme(
                Seed(40 + k),
                size,
                3 + (k % 2) as u32,
                Scheme::Den,
            ));
        }
        out
    }

    /// From the street to every floor area of every building, and from
    /// the top back out, without lifts.
    #[test]
    fn every_floor_can_be_reached_by_the_stairs() {
        let mut routes = 0;
        for l in layouts() {
            let g = FloorGraph::new(&l);
            let street = DVec3::new(l.size.x * 0.5, 0.0, -6.0);
            for c in g.pad_centres() {
                let r = g.route(street, c, false);
                assert!(r.is_some(), "{:?}: no way to {c:?}", l.scheme);
                let back = g.route(c, street, false);
                assert!(back.is_some(), "{:?}: no way out of {c:?}", l.scheme);
                routes += 1;
            }
        }
        assert!(routes > 300, "{routes}");
    }

    /// A body walks from the street to points on every floor and back
    /// out, steering at the route's points the way villains do (found
    /// again every second and a half), through the real solids: walls,
    /// rails, steps, columns and furniture.
    #[test]
    fn a_body_walks_every_route_through_the_solids() {
        use crate::collision::{solids_of, Body, Obstacles};
        use hw_gen::layout::furnish::{items, items_for, Fit};
        use hw_gen::layout::solid::{pieces, Part};
        let mut walked = 0;
        let mut failed = Vec::new();
        for (n, l) in layouts().into_iter().enumerate() {
            // (Every den: their balconies are the most crowded.)
            if n % 3 != 0 && l.scheme != Scheme::Den {
                continue;
            }
            let mut o = Obstacles::flat(0.0);
            o.add_all(
                pieces(&l)
                    .iter()
                    .filter(|p| p.part != Part::LiftDoor)
                    .flat_map(|p| solids_of(&p.shape))
                    .chain(
                        // (A den is always a gang's: furnished as one,
                        // balconies and bridges too.)
                        if l.scheme == Scheme::Den {
                            items_for(&l, None, Fit::Hideout)
                        } else {
                            items(&l)
                        }
                        .iter()
                        .filter_map(|i| i.footprint())
                        .flat_map(|f| solids_of(&f)),
                    )
                    .collect(),
            );
            // The layout's own frame as the town's.
            let floors = Floors::of(vec![Building::at(
                l.size * 0.5,
                DVec2::new(0.0, -1.0),
                0.0,
                FloorGraph::new(&l),
            )]);
            let g = &floors.buildings[0].graph;
            let street = DVec3::new(l.size.x * 0.5, 0.0, -6.0);
            // Points on each floor, a few per height.
            let mut goals = g.points();
            goals.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
            let goals: Vec<DVec3> = goals.into_iter().step_by(6).collect();
            for goal in goals {
                for (from, to) in [(street, goal), (goal, street)] {
                    let mut b = Body {
                        pos: from,
                        grounded: true,
                        ..Body::default()
                    };
                    let mut path: Vec<DVec3> = Vec::new();
                    let mut ok = false;
                    for t in 0..(150.0 * 30.0) as usize {
                        let here = b.pos;
                        let flat = DVec2::new(to.x - here.x, to.z - here.z).length();
                        if flat < 0.8 && (to.y - here.y).abs() < 0.4 {
                            ok = true;
                            break;
                        }
                        if t % 45 == 0 {
                            path = floors.way(&o, here, to).unwrap_or_else(|| vec![to]);
                        }
                        while path.len() > 1
                            && DVec2::new(path[0].x - here.x, path[0].z - here.z).length() < 0.8
                        {
                            path.remove(0);
                        }
                        let d = DVec2::new(path[0].x - here.x, path[0].z - here.z);
                        let v = d.normalize_or_zero() * 4.0;
                        let fall = if b.grounded {
                            0.0
                        } else {
                            b.vel.y - 9.8 / 30.0
                        };
                        b.vel = DVec3::new(v.x, fall, v.y);
                        o.move_body(&mut b, 1.0 / 30.0);
                    }
                    walked += 1;
                    if !ok {
                        failed.push(format!(
                            "{:?} {:?}: from {from:.1?} to {to:.1?} stuck at {:.1?}",
                            l.scheme, l.size, b.pos
                        ));
                    }
                }
            }
        }
        assert!(
            failed.is_empty(),
            "{} of {walked} failed:\n{}",
            failed.len(),
            failed.join("\n")
        );
        assert!(walked > 60, "{walked}");
    }
}
