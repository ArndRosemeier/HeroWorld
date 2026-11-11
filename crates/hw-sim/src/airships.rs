//! The airships are solid: the liner *Skylark* on its round from the
//! mooring mast ([`hw_gen::urban::mast::MastRoute`]), where it is at this
//! very tick of the game clock, the sky pirates' *Cloudcutter* moored
//! over its quay ([`crate::roosts`]) and their raiders moored over the
//! streets they raid ([`crate::roosts::raid_mooring`]), and the Zorrak
//! saucers hanging over theirs ([`crate::roosts::saucer_hover`]). Heroes
//! bump into them, stand on them and cannot fly through them.
//!
//! Each ship becomes a set of upright boxes from the same numbers it is
//! drawn by: its envelope sliced along its length into short drums, each
//! drum cut across into tiles whose height is the hull's at the tile's
//! inner edge; the passenger deck or gondola, fins and engine pods added.
//! Every box is small (at most [`TILE`] on a side), so it counts as one of
//! the moving solids ([`crate::collision::Obstacles::set_movers`]), which
//! the host sets for its heroes and a joined player for its own, both from
//! the game clock.

use crate::collision::Solid;
use crate::roosts::{
    envelope_r, pirate_roost, raid_mooring, saucer_hover, saucer_span, AIRSHIP_LENGTH,
    GONDOLA_DECK, SAUCER_RADIUS,
};
use glam::{DVec2, DVec3};
use hw_core::det;
use hw_gen::urban::landmarks::LandmarkKind;
use hw_gen::urban::mast::{
    hull_r, MastRoute, ShipPose, DECK_DROP, DECK_FROM, DECK_HALF, DECK_TO, FIN_FROM, FIN_SPAN,
    FIN_SPAN_LOW, NACELLES, PROP_BACK, SHIP_LEN,
};
use hw_gen::urban::world::TownWorld;
use std::collections::BTreeMap;
use std::sync::Arc;

/// The largest side of a box (m): small enough to count as a mover.
const TILE: f64 = 4.6;
/// Ships are solid only for heroes within this of them (m, past the hull).
const NEAR: f64 = 30.0;

/// A ship's own frame placed in the world: origin, then its right, up and
/// forward axes (heading `yaw` from +z towards +x, nose up by `pitch`).
#[derive(Clone, Copy, Debug)]
struct Frame {
    at: DVec3,
    x: DVec3,
    y: DVec3,
    z: DVec3,
}

impl Frame {
    fn new(at: DVec3, yaw: f64, pitch: f64) -> Self {
        let (sy, cy) = det::sin_cos(yaw);
        let (sp, cp) = det::sin_cos(pitch);
        Frame {
            at,
            x: DVec3::new(cy, 0.0, -sy),
            y: DVec3::new(-sp * sy, cp, -sp * cy),
            z: DVec3::new(sy * cp, sp, cy * cp),
        }
    }

    fn world(&self, l: DVec3) -> DVec3 {
        self.at + self.x * l.x + self.y * l.y + self.z * l.z
    }

    fn heading(&self) -> DVec2 {
        DVec2::new(self.z.x, self.z.z).normalize_or(DVec2::Y)
    }

    /// An upright box round the frame's local point `c`, `across` wide
    /// (along the frame's x) and `long` along its axis, from `y0` to `y1`
    /// (world heights).
    fn tile(&self, c: DVec3, across: f64, long: f64, y0: f64, y1: f64) -> Solid {
        let w = self.world(c);
        let fwd = self.heading();
        Solid {
            origin: DVec2::new(w.x, w.z),
            along: -fwd.perp(),
            inward: fwd,
            min: DVec2::new(-across * 0.5, -long * 0.5),
            max: DVec2::new(across * 0.5, long * 0.5),
            y0,
            y1,
        }
    }
}

/// What a hull adds below or beside its envelope at a slice.
struct Hull<'a> {
    /// Back from the frame's origin to the nose (the pirate's frame is at
    /// its middle), and the envelope's length.
    nose: f64,
    len: f64,
    /// The envelope's radius `s` back from the nose.
    r: &'a dyn Fn(f64) -> f64,
    /// How far below the axis something hangs under the tile from x0 to x1
    /// at `s` (a gondola, a passenger deck), if anything.
    keel: &'a dyn Fn(f64, f64, f64) -> Option<f64>,
}

/// The envelope as boxes, with what hangs under it.
fn envelope(f: &Frame, h: &Hull, out: &mut Vec<Solid>) {
    let slices = (h.len / TILE).ceil() as usize;
    let long = h.len / slices as f64;
    for i in 0..slices {
        let (s0, s1) = (long * i as f64, long * (i + 1) as f64);
        let s = (s0 + s1) * 0.5;
        let r = [s0, s, s1]
            .into_iter()
            .map(|q| (h.r)(q))
            .fold(0.0, f64::max);
        if r < 0.05 {
            continue;
        }
        let n = ((2.0 * r) / TILE).ceil().max(1.0) as usize;
        let across = 2.0 * r / n as f64;
        let axis = f.world(DVec3::new(0.0, 0.0, h.nose - s)).y;
        for k in 0..n {
            let (x0, x1) = (-r + across * k as f64, -r + across * (k + 1) as f64);
            let inner = if x0 < 0.0 && x1 > 0.0 {
                0.0
            } else {
                x0.abs().min(x1.abs())
            };
            let half = (r * r - inner * inner).max(0.0).sqrt();
            let mut y0 = axis - half;
            if let Some(d) = (h.keel)(s, x0, x1) {
                y0 = y0.min(axis - d);
            }
            out.push(f.tile(
                DVec3::new((x0 + x1) * 0.5, 0.0, h.nose - s),
                across,
                long,
                y0,
                axis + half,
            ));
        }
    }
}

/// The liner's solids at pose `p`.
pub fn liner_solids(p: &ShipPose) -> Vec<Solid> {
    let f = Frame::new(p.nose, p.yaw, p.pitch);
    let mut out = Vec::new();
    let keel = |s: f64, x0: f64, x1: f64| {
        let on = (DECK_FROM..DECK_TO).contains(&s) && x1 > -DECK_HALF && x0 < DECK_HALF;
        on.then_some(DECK_DROP)
    };
    envelope(
        &f,
        &Hull {
            nose: 0.0,
            len: SHIP_LEN,
            r: &hull_r,
            keel: &keel,
        },
        &mut out,
    );
    // Fins: up, down and to both sides, growing out over their roots.
    let tip_root = hull_r(88.0);
    let mut s = FIN_FROM;
    while s < SHIP_LEN - 1.0 {
        let grow = ((s + 2.0 - FIN_FROM) / 11.0).clamp(0.0, 1.0);
        let axis = f.world(DVec3::new(0.0, 0.0, -s)).y;
        let hull = hull_r(s);
        for (span, up) in [(FIN_SPAN, 1.0), (FIN_SPAN_LOW, -1.0)] {
            let reach = (tip_root + span * grow).max(hull);
            out.push(f.tile(
                DVec3::new(0.0, 0.0, -s - 2.0),
                0.9,
                TILE,
                if up > 0.0 { axis } else { axis - reach },
                if up > 0.0 { axis + reach } else { axis },
            ));
        }
        let reach = (tip_root + FIN_SPAN * grow).max(hull);
        // Side fins, a tile at a time out from the hull.
        let mut x = hull * 0.6;
        while x < reach {
            let w = (reach - x).min(TILE);
            for side in [1.0, -1.0] {
                out.push(f.tile(
                    DVec3::new(side * (x + w * 0.5), 0.0, -s - 2.0),
                    w,
                    TILE,
                    axis - 0.45,
                    axis + 0.45,
                ));
            }
            x += w;
        }
        s += TILE;
    }
    // Engine pods with their propellers.
    for n in NACELLES {
        for side in [1.0, -1.0] {
            for dz in [0.6, -PROP_BACK + 0.6] {
                let c = DVec3::new(n.x * side, n.y, n.z + dz);
                let y = f.world(c).y;
                out.push(f.tile(c, 3.2, 3.2, y - 1.6, y + 1.6));
            }
        }
    }
    out
}

/// The pirates' airship's solids: centred on `ship`, heading `yaw`.
pub fn pirate_solids(ship: DVec3, yaw: f64) -> Vec<Solid> {
    let f = Frame::new(ship, yaw, 0.0);
    let half = AIRSHIP_LENGTH * 0.5;
    let keel = |s: f64, x0: f64, x1: f64| {
        // The gondola: 18 m of hull under the middle, 2 m to each side.
        let on = (half - 9.0..half + 9.0).contains(&s) && x1 > -2.0 && x0 < 2.0;
        on.then_some(GONDOLA_DECK + 2.4)
    };
    let mut out = Vec::new();
    envelope(
        &f,
        &Hull {
            nose: half,
            len: AIRSHIP_LENGTH,
            r: &|s: f64| envelope_r(s / AIRSHIP_LENGTH),
            keel: &keel,
        },
        &mut out,
    );
    // Engines on outriggers beside the gondola.
    for side in [1.0, -1.0] {
        let c = DVec3::new(side * 4.6, -GONDOLA_DECK + 0.8, -2.0);
        out.push(f.tile(c, 1.6, 3.6, ship.y + c.y - 0.8, ship.y + c.y + 0.8));
    }
    // Four fins on the diagonals at the tail.
    let root = 0.8 * AIRSHIP_LENGTH - half;
    for (sx, sy) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
        let r = envelope_r(0.85) + 2.7;
        let c = DVec3::new(sx * r * 0.707, 0.0, -root - 4.0);
        let y = ship.y + sy * r * 0.707;
        out.push(f.tile(c, 3.9, TILE, y - 2.0, y + 2.0));
    }
    out
}

/// A Zorrak saucer whose rim's middle is at `at`: a grid of upright
/// tiles over its disc, each as deep and as tall as the saucer is at the
/// tile's edge nearest its middle.
pub fn saucer_solids(at: DVec3) -> Vec<Solid> {
    let f = Frame::new(at, 0.0, 0.0);
    let n = ((2.0 * SAUCER_RADIUS) / (TILE - 0.6)).ceil() as usize;
    let side = 2.0 * SAUCER_RADIUS / n as f64;
    let mut out = Vec::new();
    for i in 0..n {
        for k in 0..n {
            let x0 = -SAUCER_RADIUS + side * i as f64;
            let z0 = -SAUCER_RADIUS + side * k as f64;
            let near = |a: f64, b: f64| {
                if a < 0.0 && b > 0.0 {
                    0.0
                } else {
                    a.abs().min(b.abs())
                }
            };
            let r = near(x0, x0 + side).hypot(near(z0, z0 + side));
            let Some((under, over)) = saucer_span(r) else {
                continue;
            };
            let c = DVec3::new(x0 + side * 0.5, 0.0, z0 + side * 0.5);
            out.push(f.tile(c, side, side, at.y + under, at.y + over));
        }
    }
    out
}

/// The town plan, kept for working out where raiders hang.
#[derive(Clone)]
struct Town(Arc<TownWorld>);

impl std::fmt::Debug for Town {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Town")
    }
}

/// The town's airships, for collision.
#[derive(Clone, Debug, Default)]
pub struct Airships {
    liner: Option<MastRoute>,
    /// The moored pirate ship's solids (it does not move), and its middle.
    pirate: Vec<Solid>,
    pirate_at: Option<DVec3>,
    /// The pirates' raiders moored over the streets they raid, by
    /// incident: where each hangs and its solids.
    raids: BTreeMap<u32, (DVec3, Vec<Solid>)>,
    /// The Zorrak saucers over the streets they raid, by incident: where
    /// each hangs and its solids.
    saucers: BTreeMap<u32, (DVec3, Vec<Solid>)>,
    town: Option<Town>,
}

impl Airships {
    /// The town's airships, raiders too ([`Self::set_raids`]).
    pub fn with_raids(w: &Arc<TownWorld>) -> Self {
        Airships {
            town: Some(Town(w.clone())),
            ..Self::new(w)
        }
    }

    /// Keeps the raiders in step with the raids open now: `(incident,
    /// street)` each. A raider hangs where every machine draws it
    /// ([`raid_mooring`]), worked out once per raid.
    pub fn set_raids(&mut self, raids: &[(u32, DVec3)]) {
        let Some(Town(w)) = &self.town else {
            return;
        };
        self.raids.retain(|id, _| raids.iter().any(|r| r.0 == *id));
        for &(id, street) in raids {
            self.raids.entry(id).or_insert_with(|| {
                let (at, yaw) = raid_mooring(w, street);
                (at, pirate_solids(at, yaw))
            });
        }
    }

    /// Keeps the saucers in step with the saucer raids open now:
    /// `(incident, street)` each, hanging where every machine draws them
    /// ([`saucer_hover`]).
    pub fn set_saucers(&mut self, raids: &[(u32, DVec3)]) {
        let Some(Town(w)) = &self.town else {
            return;
        };
        self.saucers
            .retain(|id, _| raids.iter().any(|r| r.0 == *id));
        for &(id, street) in raids {
            self.saucers.entry(id).or_insert_with(|| {
                let at = saucer_hover(w, street);
                (at, saucer_solids(at))
            });
        }
    }

    pub fn new(w: &TownWorld) -> Self {
        let liner = w
            .landmarks
            .iter()
            .find(|l| l.kind == LandmarkKind::MooringMast)
            .map(|m| MastRoute::new(w, m));
        let roost = pirate_roost(w);
        Airships {
            liner,
            pirate: roost
                .map(|r| pirate_solids(r.ship(), r.ship_yaw()))
                .unwrap_or_default(),
            pirate_at: roost.map(|r| r.ship()),
            raids: BTreeMap::new(),
            saucers: BTreeMap::new(),
            town: None,
        }
    }

    /// The liner's route, if the town has a mast.
    pub fn liner(&self) -> Option<&MastRoute> {
        self.liner.as_ref()
    }

    /// The airships' solids at game second `time`, those near any of
    /// `heroes` only (none when nobody is near).
    pub fn solids_near(&self, heroes: &[DVec3], time: f64) -> Vec<Solid> {
        let mut out = Vec::new();
        let near = |c: DVec3, reach: f64| heroes.iter().any(|h| h.distance(c) < reach + NEAR);
        if let Some(r) = &self.liner {
            let pose = r.pose(time);
            let mid = pose.nose - pose.forward() * (SHIP_LEN * 0.5);
            if near(mid, SHIP_LEN * 0.5 + 12.0) {
                out.extend(liner_solids(&pose));
            }
        }
        if let Some(c) = self.pirate_at {
            if near(c, AIRSHIP_LENGTH * 0.5 + 12.0) {
                out.extend(self.pirate.iter().cloned());
            }
        }
        for (c, solids) in self.raids.values() {
            if near(*c, AIRSHIP_LENGTH * 0.5 + 12.0) {
                out.extend(solids.iter().cloned());
            }
        }
        for (c, solids) in self.saucers.values() {
            if near(*c, SAUCER_RADIUS + 4.0) {
                out.extend(solids.iter().cloned());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hw_gen::urban::mast::{Phase, SHIP_R};

    fn inside(solids: &[Solid], p: DVec3, slack: f64) -> bool {
        solids.iter().any(|s| {
            let d = DVec2::new(p.x, p.z) - s.origin;
            let l = DVec2::new(d.dot(s.along), d.dot(s.inward));
            p.y >= s.y0 - slack
                && p.y <= s.y1 + slack
                && l.x >= s.min.x - slack
                && l.x <= s.max.x + slack
                && l.y >= s.min.y - slack
                && l.y <= s.max.y + slack
        })
    }

    fn pose(yaw: f64, pitch: f64) -> ShipPose {
        ShipPose {
            nose: DVec3::new(100.0, 80.0, -40.0),
            yaw,
            pitch,
            roll: 0.0,
            thrust: 1.0,
            phase: Phase::Flight,
            t: 0.0,
        }
    }

    #[test]
    fn a_saucer_is_solid_where_it_is_drawn() {
        use crate::roosts::{SAUCER_TOP, SAUCER_UNDER};
        let at = DVec3::new(30.0, 25.0, -12.0);
        let solids = saucer_solids(at);
        for s in &solids {
            assert!((s.max - s.min).length() * 0.5 < 3.5, "a mover: {s:?}");
        }
        // Its skin, a hand inside, all round; under side and top alike.
        for &(r, y) in SAUCER_UNDER.iter().chain(SAUCER_TOP.iter()) {
            let r = (r - 0.3).max(0.0);
            for k in 0..24 {
                let a = std::f64::consts::TAU * k as f64 / 24.0;
                let inward = if y < 0.0 { 0.2 } else { -0.2 };
                let q = at + DVec3::new(r * a.cos(), y + inward, r * a.sin());
                assert!(inside(&solids, q, 1e-6), "open at r {r} y {y} angle {a}");
            }
        }
        // And open air well clear of it.
        let clear = [
            at + DVec3::Y * 4.0,
            at - DVec3::Y * 2.5,
            at + DVec3::X * (SAUCER_RADIUS + 1.0),
        ];
        for q in clear {
            assert!(!inside(&solids, q, 0.0), "{q}");
        }
    }

    #[test]
    fn the_liner_is_solid_where_it_is_drawn() {
        for (yaw, pitch) in [(0.0, 0.0), (2.3, 0.06), (-1.1, -0.05)] {
            let p = pose(yaw, pitch);
            let solids = liner_solids(&p);
            let f = Frame::new(p.nose, p.yaw, p.pitch);
            // Every box counts as a mover.
            for s in &solids {
                assert!((s.max - s.min).length() * 0.5 < 3.5, "{s:?}");
            }
            // The hull's skin, a hand inside it, is solid all round; the
            // middle of the deck and the pods too.
            for i in 1..48 {
                let s = SHIP_LEN * i as f64 / 48.0;
                for k in 0..16 {
                    let a = std::f64::consts::TAU * k as f64 / 16.0;
                    let r = hull_r(s) - 0.3;
                    if r < 0.3 {
                        continue;
                    }
                    let q = f.world(DVec3::new(r * a.cos(), r * a.sin(), -s));
                    assert!(inside(&solids, q, 1e-6), "skin open at s {s} angle {a}");
                }
            }
            let deck = f.world(DVec3::new(0.0, -DECK_DROP + 0.4, -25.0));
            assert!(inside(&solids, deck, 0.0), "the passenger deck");
            let pod = f.world(NACELLES[0]);
            assert!(inside(&solids, pod, 0.0), "an engine pod");
            // And not much more than the ship: well clear of it is open air.
            let above = f.world(DVec3::new(0.0, SHIP_R + 2.0, -30.0));
            let beside = f.world(DVec3::new(SHIP_R + 3.0, 0.0, -30.0));
            assert!(!inside(&solids, above, 0.0) && !inside(&solids, beside, 0.0));
        }
    }

    #[test]
    fn the_pirate_ship_is_solid_where_it_is_drawn() {
        let c = DVec3::new(10.0, 40.0, 5.0);
        let solids = pirate_solids(c, 0.7);
        let f = Frame::new(c, 0.7, 0.0);
        for i in 1..24 {
            let t = i as f64 / 24.0;
            let r = envelope_r(t) - 0.3;
            for k in 0..12 {
                let a = std::f64::consts::TAU * k as f64 / 12.0;
                let q = f.world(DVec3::new(
                    r * a.cos(),
                    r * a.sin(),
                    AIRSHIP_LENGTH * (0.5 - t),
                ));
                assert!(inside(&solids, q, 1e-6), "envelope open at t {t}");
            }
        }
        let gondola = f.world(DVec3::new(0.0, -GONDOLA_DECK - 1.0, 0.0));
        assert!(inside(&solids, gondola, 0.0), "the gondola");
    }

    #[test]
    fn the_liner_is_solid_where_the_timetable_has_it() {
        use hw_core::Seed;
        use hw_gen::urban::TownParams;
        let w = TownWorld::generate(Seed(2), TownParams::sample(Seed(2), 20_000.0));
        let a = Airships::new(&w);
        let r = a.liner().expect("a liner").clone();
        for t in [5.0, 200.0, 400.0, 700.0] {
            let g = r.when(0.0, t);
            let p = r.pose(g);
            let mid = p.nose - p.forward() * 30.0;
            // A hero beside it meets it; one far off does not.
            let solids = a.solids_near(&[mid + DVec3::Y * 15.0], g);
            assert!(inside(&solids, mid, 0.0), "cycle second {t}");
            let later = a.solids_near(&[mid], g + 30.0);
            if p.phase != Phase::Docked {
                assert!(!inside(&later, mid, 0.0), "it moved on by {t} + 30 s");
            }
            assert!(a.solids_near(&[mid + DVec3::Y * 500.0], g).is_empty());
        }
    }

    #[test]
    fn a_hero_bumps_into_the_liner_and_can_land_on_it() {
        use crate::collision::{Body, Obstacles};
        let p = pose(0.4, 0.0);
        let f = Frame::new(p.nose, p.yaw, p.pitch);
        let o = Obstacles::flat(0.0);
        o.set_movers(liner_solids(&p));
        // Flying at it from the side, level with its axis: stopped at the skin.
        let mid = f.world(DVec3::new(0.0, 0.0, -40.0));
        let mut b = Body {
            pos: mid + f.x * (SHIP_R + 8.0),
            vel: DVec3::ZERO,
            grounded: false,
        };
        for _ in 0..90 {
            b.vel = -f.x * 25.0;
            o.move_body(&mut b, 1.0 / 30.0);
        }
        let off = (b.pos - mid).dot(f.x);
        assert!(off > hull_r(40.0) - 0.5, "went into the hull: {off}");
        // Dropping onto its back: comes to rest on top.
        let mut b = Body {
            pos: mid + DVec3::Y * (SHIP_R + 4.0),
            vel: DVec3::ZERO,
            grounded: false,
        };
        for _ in 0..90 {
            b.vel.y -= 9.8 / 30.0;
            o.move_body(&mut b, 1.0 / 30.0);
        }
        assert!(b.grounded, "{:?}", b.pos);
        assert!(b.pos.y > mid.y + hull_r(40.0) - 0.5, "{:?}", b.pos);
    }
}
