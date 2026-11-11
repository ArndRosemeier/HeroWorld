//! The portal power (after MasterWorld's portal stone): every hero's, free,
//! with two buttons. The first marks home: the floor a step ahead of the
//! hero, its way through the way the hero faces. The second tears a
//! portal open a little further ahead, joined to a twin standing at home.
//! Only one pair per hero is open at a time: a new portal elsewhere closes
//! the old far one (home now leads to the new place); marking a new home
//! closes both.
//!
//! A pair is two upright ellipses (rifts) a head taller than a hero. Each
//! is seen through and walked through from its front only: the far one
//! faces the hero who opened it, home faces the way the hero faced when
//! marking it. Stepping into one's front brings you out of the other's
//! front, turned as the portals are, at the same height over its floor
//! and moving the same way: anyone may use them, villains excepted.
//!
//! The rules here are pure (no `World`): combat asks for a spot with room,
//! the simulation keeps the pairs, and the client's prediction runs the
//! same [`pass`] as the server so stepping through never snaps.

use crate::collision::{Body, Obstacles};
use glam::{DVec2, DVec3};
use hw_core::det;
use hw_core::NetId;
use hw_net::protocol::MAX_GATES;
use hw_net::{GateView, PlayerId, ToClient};
use std::collections::BTreeMap;

/// A whole portal's half width and height over its floor (m): a head and
/// more over a hero.
pub const HALF_WIDTH: f64 = 1.3;
pub const HEIGHT: f64 = 3.5;
/// The ellipse's foot stands this far over the floor (m).
pub const LIFT: f64 = 0.12;
/// Where there is less room a portal shrinks to fit, down to the least of
/// these (of a whole one): still a head over a hero.
pub const FITS: [f64; 4] = [1.0, 0.86, 0.74, 0.64];
/// Home is marked on the floor this far ahead of the hero's feet (m).
pub const MARK_AHEAD: f64 = 1.3;
/// A portal opens this far ahead of the hero's feet (m): one step into it.
pub const OPEN_AHEAD: f64 = 2.7;
/// A far portal nearer home than this would stand in its twin (m).
pub const NEAR_HOME: f64 = 6.0;
/// The floor must be no further below the hero's feet than this (m): a
/// hero flying high cannot mark or open on the street far below.
pub const REACH_DOWN: f64 = 4.0;
/// How far the floor may rise or fall under the arch (m).
const LEVEL: f64 = 0.55;
/// A body crossing a portal is looked at this high over its feet (m).
const CROSS_AT: f64 = 1.0;
/// Inside this share of the ellipse a body goes through; nearer the rim it
/// brushes past.
const THROUGH: f64 = 0.92;
/// A body coming out stands this far in front of the other side (m).
const OUT: f64 = 0.03;

/// One side of a pair: the floor spot under its middle, and the way its
/// front faces (radians: 0 faces +Z, a quarter turn +X, as Engine's yaw).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Side {
    pub at: DVec3,
    pub yaw: f64,
}

impl Side {
    /// Out of its front.
    pub fn normal(&self) -> DVec3 {
        let (s, c) = det::sin_cos(self.yaw);
        DVec3::new(s, 0.0, c)
    }

    /// The same opening seen from behind: its back as a front of its own.
    pub fn back(&self) -> Side {
        Side {
            at: self.at,
            yaw: self.yaw + std::f64::consts::PI,
        }
    }

    /// Across it, to its right seen from its front's back (Engine's local +X).
    pub fn right(&self) -> DVec3 {
        let (s, c) = det::sin_cos(self.yaw);
        DVec3::new(c, 0.0, -s)
    }

    /// `p` in its own frame: across, up from the floor, out of the front.
    pub fn local(&self, p: DVec3) -> DVec3 {
        let d = p - self.at;
        DVec3::new(d.dot(self.right()), d.y, d.dot(self.normal()))
    }

    /// A point of its own frame in the world.
    pub fn world(&self, l: DVec3) -> DVec3 {
        self.at + self.right() * l.x + DVec3::Y * l.y + self.normal() * l.z
    }

    /// A direction of its own frame in the world.
    fn world_dir(&self, l: DVec3) -> DVec3 {
        self.right() * l.x + DVec3::Y * l.y + self.normal() * l.z
    }

    fn local_dir(&self, d: DVec3) -> DVec3 {
        DVec3::new(d.dot(self.right()), d.y, d.dot(self.normal()))
    }
}

/// The ellipse of a portal `size` of a whole one: its middle's height over
/// the floor and its half sizes across and up (m).
pub fn ellipse(size: f64) -> (f64, f64, f64) {
    let ry = (HEIGHT * size - LIFT) * 0.5;
    (LIFT + ry, HALF_WIDTH * size, ry)
}

/// Is `(u, v)` (across, up from the floor) inside `share` of the ellipse?
pub fn inside(size: f64, u: f64, v: f64, share: f64) -> bool {
    let (cy, rx, ry) = ellipse(size);
    let (a, b) = (u / (rx * share), (v - cy) / (ry * share));
    a * a + b * b <= 1.0
}

/// An open pair: home, the far side, their size, the tick it opened.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pair {
    pub owner: PlayerId,
    pub home: Side,
    pub far: Side,
    pub size: f64,
    pub opened: u64,
}

impl Pair {
    pub fn view(&self) -> GateView {
        GateView {
            owner: self.owner,
            home: self.home.at.to_array(),
            home_yaw: self.home.yaw as f32,
            far: self.far.at.to_array(),
            far_yaw: self.far.yaw as f32,
            size: self.size as f32,
            opened: self.opened,
        }
    }

    pub fn from_view(v: &GateView) -> Self {
        Self {
            owner: v.owner,
            home: Side {
                at: DVec3::from_array(v.home),
                yaw: v.home_yaw as f64,
            },
            far: Side {
                at: DVec3::from_array(v.far),
                yaw: v.far_yaw as f64,
            },
            size: v.size as f64,
            opened: v.opened,
        }
    }

    /// Every way through (into this side, out of that one): a portal has
    /// two faces, as MasterWorld's had. Into home's front, out of the far
    /// side's front; into home's back, out of the far side's back; and the
    /// same the other way.
    pub fn ways(&self) -> [(Side, Side); 4] {
        let (h, f) = (self.home, self.far);
        [(h, f), (f, h), (h.back(), f.back()), (f.back(), h.back())]
    }
}

/// The way a hero faces, from where it aims (radians, as [`Side::yaw`]);
/// None when it aims straight up or down.
pub fn yaw_of(aim: DVec3) -> Option<f64> {
    let flat = DVec2::new(aim.x, aim.z);
    (flat.length_squared() > 1e-6).then(|| det::atan2(flat.x, flat.y))
}

fn ahead(yaw: f64) -> DVec3 {
    let (s, c) = det::sin_cos(yaw);
    DVec3::new(s, 0.0, c)
}

/// The floor under `p` (x, z) for a hero whose feet are at `feet_y`, if it
/// is near enough below.
fn floor_near(solids: &Obstacles, p: DVec3, feet_y: f64) -> Option<f64> {
    let y = solids.floor_under(DVec2::new(p.x, p.z), feet_y + 0.6);
    (y.is_finite() && feet_y - y <= REACH_DOWN).then_some(y)
}

/// Home for a hero at `feet` facing `yaw`: the floor a step ahead, its
/// front facing the same way (you come out walking on). None when the
/// floor there is far below (flying).
pub fn mark_spot(solids: &Obstacles, feet: DVec3, yaw: f64) -> Option<Side> {
    let p = feet + ahead(yaw) * MARK_AHEAD;
    let y = floor_near(solids, p, feet.y)?;
    Some(Side {
        at: DVec3::new(p.x, y, p.z),
        yaw,
    })
}

/// Where a portal opens for a hero at `feet` facing `yaw`: on the floor a
/// stride ahead, its front facing the hero.
pub fn open_spot(solids: &Obstacles, feet: DVec3, yaw: f64) -> Option<Side> {
    let p = feet + ahead(yaw) * OPEN_AHEAD;
    let y = floor_near(solids, p, feet.y)?;
    Some(Side {
        at: DVec3::new(p.x, y, p.z),
        yaw: yaw + std::f64::consts::PI,
    })
}

/// Is there room for a portal `size` of a whole one at `s`: the floor
/// under the arch about level, nothing solid in the arch or just before
/// and behind it?
pub fn has_room(solids: &Obstacles, s: &Side, size: f64) -> bool {
    let (cy, rx, ry) = ellipse(size);
    for along in [-0.6, 0.0, 0.6] {
        for k in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            let foot = s.world(DVec3::new(rx * k, 0.0, along));
            let y = solids.floor_under(DVec2::new(foot.x, foot.z), s.at.y + LEVEL + 0.05);
            if !y.is_finite() || (y - s.at.y).abs() > LEVEL {
                return false;
            }
            for up in [0.4, 1.2, 2.0, cy + ry * 0.6, cy + ry - 0.15] {
                // The rounded top is narrower: its outer points are not
                // asked high up.
                if !inside(size, rx * k * 0.95, up, 1.0) {
                    continue;
                }
                if solids.solid_at(DVec3::new(foot.x, s.at.y + up, foot.z)) {
                    return false;
                }
            }
        }
    }
    true
}

/// If a body moving from `before` to `after` stepped into the front of a
/// portal in `pairs`, brings it out of the other side: its new body, and
/// how far it turned (radians, to add to the way it faces).
pub fn pass<'a>(
    pairs: impl IntoIterator<Item = &'a Pair>,
    before: &Body,
    after: &Body,
) -> Option<(Body, f64)> {
    through(pairs, before, after).map(|(b, t, _)| (b, t))
}

/// [`pass`], also saying whose portal it was.
fn through<'a>(
    pairs: impl IntoIterator<Item = &'a Pair>,
    before: &Body,
    after: &Body,
) -> Option<(Body, f64, PlayerId)> {
    let q0 = before.pos + DVec3::Y * CROSS_AT;
    let q1 = after.pos + DVec3::Y * CROSS_AT;
    let mut best: Option<(f64, Side, Side, PlayerId)> = None;
    for pair in pairs {
        for (into, out) in pair.ways() {
            let (a, b) = (into.local(q0), into.local(q1));
            if !(a.z > 0.0 && b.z <= 0.0) {
                continue;
            }
            let t = a.z / (a.z - b.z);
            let hit = a + (b - a) * t;
            if !inside(pair.size, hit.x, hit.y, THROUGH) {
                continue;
            }
            if best.is_none_or(|(bt, ..)| t < bt) {
                best = Some((t, into, out, pair.owner));
            }
        }
    }
    let (_, into, out, owner) = best?;
    // Through: across and out of the front turn half round, up stays.
    let flip = |l: DVec3| DVec3::new(-l.x, l.y, -l.z);
    let mut l = flip(into.local(after.pos));
    l.z = l.z.max(OUT);
    let body = Body {
        pos: out.world(l),
        vel: out.world_dir(flip(into.local_dir(after.vel))),
        grounded: after.grounded,
    };
    let turn = out.yaw - into.yaw + std::f64::consts::PI;
    Some((body, turn, owner))
}

/// Why a portal did not open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// No home marked yet.
    NoHome,
    /// No room (a wall, a slope, too high up, too near home).
    NoRoom,
}

/// Someone who stepped through a portal: who, whose portal, where they
/// came out (feet).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crossing {
    pub hero: NetId,
    pub owner: PlayerId,
    pub at: DVec3,
    /// How far they turned on the way through (radians).
    pub turn: f64,
}

/// A yaw (radians) in 1/65536 turns, as fight events carry it.
pub fn yaw_turns(yaw: f64) -> u16 {
    let t = yaw / std::f64::consts::TAU;
    ((t - t.floor()) * 65536.0) as u32 as u16
}

/// A yaw in 1/65536 turns back in radians.
pub fn turns_yaw(turns: u16) -> f64 {
    turns as f64 / 65536.0 * std::f64::consts::TAU
}

/// Every hero's home and open pair.
#[derive(Clone, Debug, Default)]
pub struct Gates {
    homes: BTreeMap<PlayerId, Side>,
    open: BTreeMap<PlayerId, Pair>,
    /// The open pairs changed since the news was last taken.
    changed: bool,
    /// Homes marked since last taken: the simulation writes them into the
    /// hero files.
    marks: Vec<(PlayerId, Side)>,
    /// Who stepped through since last taken (combat shows it).
    crossed: Vec<Crossing>,
}

impl Gates {
    /// Moves `hero` through a portal it stepped into between `before` and
    /// `body`: its body comes out of the other side (noted for the fight's
    /// light). Returns how far it turned.
    pub fn carry(&mut self, hero: NetId, before: &Body, body: &mut Body) -> Option<f64> {
        let (out, turn, owner) = through(self.open.values(), before, body)?;
        *body = out;
        self.crossed.push(Crossing {
            hero,
            owner,
            at: out.pos,
            turn,
        });
        Some(turn)
    }

    /// Who stepped through since the last call.
    pub fn take_crossed(&mut self) -> Vec<Crossing> {
        std::mem::take(&mut self.crossed)
    }

    /// The hero's home, if marked.
    pub fn home(&self, owner: PlayerId) -> Option<Side> {
        self.homes.get(&owner).copied()
    }

    /// The hero's open pair, if any.
    pub fn pair(&self, owner: PlayerId) -> Option<&Pair> {
        self.open.get(&owner)
    }

    /// Every open pair.
    pub fn pairs(&self) -> impl Iterator<Item = &Pair> {
        self.open.values()
    }

    /// A home from a hero file (a joining hero), used only if the hero has
    /// none here yet.
    pub fn remember_home(&mut self, owner: PlayerId, home: Side) {
        self.homes.entry(owner).or_insert(home);
    }

    /// Marks `owner`'s home: an open pair closes (returned, for its
    /// closing light).
    pub fn mark(&mut self, owner: PlayerId, home: Side) -> Option<Pair> {
        self.homes.insert(owner, home);
        self.marks.push((owner, home));
        let closed = self.open.remove(&owner);
        self.changed |= closed.is_some();
        closed
    }

    /// How big a portal of `owner`'s can open at `far`.
    pub fn can_open(
        &self,
        owner: PlayerId,
        solids: &Obstacles,
        far: &Side,
    ) -> Result<f64, Refused> {
        let home = self.home(owner).ok_or(Refused::NoHome)?;
        let flat = DVec2::new(far.at.x - home.at.x, far.at.z - home.at.z);
        if flat.length() < NEAR_HOME && (far.at.y - home.at.y).abs() < HEIGHT {
            return Err(Refused::NoRoom);
        }
        FITS.into_iter()
            .find(|&k| has_room(solids, far, k))
            .ok_or(Refused::NoRoom)
    }

    /// Opens `owner`'s far portal at `far` (`size` from [`Gates::can_open`]):
    /// the pair now open, and the old far side if one closed. None without
    /// a home.
    pub fn open(
        &mut self,
        owner: PlayerId,
        far: Side,
        size: f64,
        tick: u64,
    ) -> Option<(Pair, Option<Side>)> {
        let home = self.home(owner)?;
        let closed = self.open.get(&owner).map(|p| p.far);
        let keep = self.open.get(&owner).map_or(tick, |p| p.opened);
        let pair = Pair {
            owner,
            home,
            far,
            size,
            // Home stays open when only the far side moves.
            opened: if closed.is_some() { keep } else { tick },
        };
        if self.open.len() >= MAX_GATES && !self.open.contains_key(&owner) {
            return None;
        }
        self.open.insert(owner, pair);
        self.changed = true;
        Some((pair, closed))
    }

    /// Players who left: their pairs close (returned, for their light),
    /// their homes are forgotten here (their files keep them).
    pub fn keep_only(&mut self, here: &[PlayerId]) -> Vec<Pair> {
        let gone: Vec<PlayerId> = self
            .open
            .keys()
            .filter(|p| !here.contains(p))
            .copied()
            .collect();
        self.homes.retain(|p, _| here.contains(p));
        let closed: Vec<Pair> = gone.iter().filter_map(|p| self.open.remove(p)).collect();
        self.changed |= !closed.is_empty();
        closed
    }

    /// Homes marked since the last call.
    pub fn take_marks(&mut self) -> Vec<(PlayerId, Side)> {
        std::mem::take(&mut self.marks)
    }

    /// Every open pair, for clients.
    pub fn snapshot(&self) -> ToClient {
        ToClient::Gates {
            gates: self.open.values().map(Pair::view).collect(),
        }
    }

    /// The open pairs, if they changed since the last call.
    pub fn take_news(&mut self) -> Option<ToClient> {
        std::mem::take(&mut self.changed).then(|| self.snapshot())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GROUND_Y;

    fn flat() -> Obstacles {
        Obstacles::flat(GROUND_Y)
    }

    fn pair(home: Side, far: Side) -> Pair {
        Pair {
            owner: PlayerId(1),
            home,
            far,
            size: 1.0,
            opened: 0,
        }
    }

    #[test]
    fn stepping_into_the_far_portal_comes_out_at_home_walking_on() {
        let solids = flat();
        // Home marked facing +X at the origin; a portal opened 50 m away
        // by a hero facing +Z.
        let home = mark_spot(&solids, DVec3::ZERO, std::f64::consts::FRAC_PI_2).unwrap();
        let far = open_spot(&solids, DVec3::new(0.0, 0.0, 50.0), 0.0).unwrap();
        assert!(
            (home.at - DVec3::new(MARK_AHEAD, GROUND_Y, 0.0)).length() < 1e-9,
            "{}",
            home.at
        );
        let p = pair(home, far);
        // Walking +Z into the far portal's front.
        let before = Body {
            pos: far.at - DVec3::Z * 0.1,
            vel: DVec3::Z * 5.0,
            grounded: true,
        };
        let after = Body {
            pos: far.at + DVec3::Z * 0.1,
            ..before
        };
        let (out, turn) = pass([&p], &before, &after).expect("through");
        // Out of home's front, walking +X, turned a quarter.
        assert!(
            (out.pos - (home.at + DVec3::X * 0.1)).length() < 1e-6,
            "{}",
            out.pos
        );
        assert!((out.vel - DVec3::X * 5.0).length() < 1e-6, "{}", out.vel);
        let turned = DVec2::new(det::sin(turn), det::cos(turn));
        assert!((turned - DVec2::new(1.0, 0.0)).length() < 1e-6);
        // And back: walking -X into home's front comes out of the far one
        // walking -Z.
        let back = Body {
            pos: home.at + DVec3::X * 0.1,
            vel: -DVec3::X * 5.0,
            grounded: true,
        };
        let (out, _) = pass(
            [&p],
            &back,
            &Body {
                pos: home.at - DVec3::X * 0.1,
                ..back
            },
        )
        .unwrap();
        assert!((out.vel + DVec3::Z * 5.0).length() < 1e-6, "{}", out.vel);
        assert!(out.pos.z < far.at.z);
    }

    #[test]
    fn both_faces_carry_but_the_rim_does_not() {
        let solids = flat();
        let home = mark_spot(&solids, DVec3::ZERO, 0.0).unwrap();
        let far = open_spot(&solids, DVec3::new(30.0, 0.0, 0.0), 0.0).unwrap();
        let p = pair(home, far);
        // Into the far portal from behind (its front faces -Z): out of
        // home's back (home's front faces +Z), still going the way home's
        // back faces.
        let a = Body {
            pos: far.at + DVec3::Z * 0.1,
            vel: -DVec3::Z,
            grounded: true,
        };
        let (out, _) = pass(
            [&p],
            &a,
            &Body {
                pos: far.at - DVec3::Z * 0.1,
                ..a
            },
        )
        .expect("the back carries too");
        assert!(out.pos.z < home.at.z, "out behind home: {}", out.pos);
        assert!(out.vel.z < 0.0, "going on out of home's back: {}", out.vel);
        // Past its edge.
        let side = far.at + DVec3::X * (HALF_WIDTH + 0.2);
        let b = Body {
            pos: side - DVec3::Z * 0.1,
            vel: DVec3::Z,
            grounded: true,
        };
        assert!(pass(
            [&p],
            &b,
            &Body {
                pos: side + DVec3::Z * 0.1,
                ..b
            }
        )
        .is_none());
    }

    #[test]
    fn a_portal_needs_a_home_room_and_distance() {
        let solids = flat();
        let mut g = Gates::default();
        let me = PlayerId(1);
        let far = open_spot(&solids, DVec3::new(0.0, 0.0, 40.0), 0.0).unwrap();
        assert_eq!(g.can_open(me, &solids, &far), Err(Refused::NoHome));
        g.mark(me, mark_spot(&solids, DVec3::ZERO, 0.0).unwrap());
        assert_eq!(g.can_open(me, &solids, &far), Ok(1.0));
        let near = open_spot(&solids, DVec3::new(0.0, 0.0, 1.0), 0.0).unwrap();
        assert_eq!(g.can_open(me, &solids, &near), Err(Refused::NoRoom));
        // Flying high over the street: no floor within reach.
        assert!(open_spot(&solids, DVec3::new(0.0, 30.0, 40.0), 0.0).is_none());
        let (pair, closed) = g.open(me, far, 1.0, 10).unwrap();
        assert!(closed.is_none());
        assert!(g.take_news().is_some());
        // A second far portal moves the far side; home stays as it opened.
        let far2 = open_spot(&solids, DVec3::new(0.0, 0.0, 80.0), 0.0).unwrap();
        let (pair2, closed) = g.open(me, far2, 1.0, 50).unwrap();
        assert_eq!(closed, Some(pair.far));
        assert_eq!(pair2.opened, 10);
        // Marking a new home closes the pair.
        assert!(g
            .mark(me, mark_spot(&solids, DVec3::X * 9.0, 0.0).unwrap())
            .is_some());
        assert!(g.pair(me).is_none());
        assert_eq!(g.take_marks().len(), 2);
    }

    #[test]
    fn views_survive_the_wire_shape() {
        let p = pair(
            Side {
                at: DVec3::new(1.5, 2.0, -3.0),
                yaw: 0.5,
            },
            Side {
                at: DVec3::new(100.0, 0.0, 7.0),
                yaw: -2.0,
            },
        );
        let back = Pair::from_view(&p.view());
        assert_eq!(back.home.at, p.home.at);
        assert!((back.far.yaw - p.far.yaw).abs() < 1e-6);
    }
}
