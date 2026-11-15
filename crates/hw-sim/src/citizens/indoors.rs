//! People indoors: who is at a place in a building, and what they are doing
//! there, as a pure function of the place and the game time (nothing is
//! kept, so only the places near the camera ever cost anything).
//!
//! A place is taken for a visit of about a quarter of an hour, by a
//! stranger (at home, the same household). They come in by a doorway,
//! walk to the place, and then live in cycles of a minute or so: a few
//! tens of seconds of one act, perhaps a short walk to something else in
//! the room (the water cooler, a shelf, the stove) and an act there, the
//! walk back, and another act at the place. At the end of the visit they
//! walk out again. Nobody stands about: whenever they are not walking
//! they are doing something that fits the place (see [`hw_core::Act`]),
//! and the stillest they ever get is the second it takes to sit down or
//! rise.

use super::{indoor_share, Citizen, Citizens, INDOOR, LOOKS, STAY};
use glam::{DVec2, DVec3};
use hw_core::{Act, Seed};
use hw_gen::building::RoomKind;

/// How fast people walk inside (m/s).
pub const WALK: f64 = 1.2;
/// Seconds it takes to sit down or to rise.
pub const SETTLE: f64 = 0.8;
/// How long a cycle lasts (s): the shortest and the longest.
const CYCLE: (f64, f64) = (50.0, 110.0);
/// A walk to something else is taken in this share of the cycles, by
/// someone on their feet and by someone sitting.
const GO_STANDING: f64 = 0.7;
const GO_SITTING: f64 = 0.4;
/// The time spent at the other thing (s): the shortest and the longest.
const AT_THING: (f64, f64) = (10.0, 30.0);
/// A cycle with a walk keeps at least this long (s) at the place.
const AT_LEAST: f64 = 12.0;
/// Turning to face the next thing takes this long (s).
const TURN: f64 = 0.4;

/// A short walk from a place to something else in the room.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaceTrip {
    pub at: DVec3,
    pub facing: DVec2,
    pub act: Act,
}

/// A place to be indoors, in the town's frame (see
/// `hw_gen::layout::people::Stand`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    /// The room's use, which says when it is busy.
    pub room: RoomKind,
    /// Where the act is done (the seat, for someone sitting).
    pub at: DVec3,
    pub facing: DVec2,
    pub act: Act,
    /// Where one steps to and from.
    pub via: DVec3,
    /// Where one comes in and leaves, if a clear walk leads from there.
    pub door: Option<DVec3>,
    pub trips: [Option<PlaceTrip>; 2],
    /// Talking neighbours share a key and come and go together.
    pub pair: Option<u32>,
    /// For a sitter: how high their hips are over the floor (m).
    pub hip: f64,
}

impl Place {
    /// Where the act is done, at the height the body is held (the hips'
    /// height for a sitter).
    fn here(&self) -> DVec3 {
        self.at + DVec3::new(0.0, self.hip, 0.0)
    }
}

impl Place {
    /// A place with just an act (no walks, no doorway): someone who
    /// appears there and goes on with what they do.
    pub fn simple(room: RoomKind, at: DVec3, facing: DVec2, act: Act) -> Self {
        Self {
            room,
            at,
            facing,
            act,
            via: at,
            door: None,
            trips: [None, None],
            pair: None,
            hip: 0.0,
        }
    }
}

/// Someone indoors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Indoor {
    pub citizen: Citizen,
    /// What they are doing where they are; `None` while they walk (and
    /// for the moment of rising from a seat).
    pub act: Option<Act>,
}

/// One stretch of a visit: from where to where (the same, for an act),
/// doing what.
struct Leg {
    dur: f64,
    from: DVec3,
    to: DVec3,
    /// The way they face during an act (walking: the way they go).
    face: Option<DVec2>,
    act: Option<Act>,
    /// Walking speed shown (0 for an act).
    speed: f64,
}

fn flat(a: DVec3, b: DVec3) -> f64 {
    DVec2::new(a.x - b.x, a.z - b.z).length()
}

fn heading(from: DVec3, to: DVec3, or: DVec2) -> DVec2 {
    DVec2::new(to.x - from.x, to.z - from.z).normalize_or(or)
}

/// `a` turned towards `b` by the share `t` (0 to 1) of the way round.
fn turned(a: DVec2, b: DVec2, t: f64) -> DVec2 {
    let ang = a.angle_to(b);
    DVec2::from_angle(ang * t.clamp(0.0, 1.0))
        .rotate(a)
        .normalize_or(b)
}

impl Citizens {
    /// Who is at `place` (number `n` of floor `level` of `building`) at
    /// `time`, and what they are doing: someone at home, at work or
    /// shopping, by the hour ([`Self::set_hour`]); `None` while it is
    /// empty.
    pub fn indoors(
        &self,
        building: u64,
        level: u32,
        n: u32,
        place: &Place,
        time: f64,
    ) -> Option<Indoor> {
        indoors_at(self.seed, self.hour(), building, level, n, place, time)
    }
}

/// [`Citizens::indoors`] for a town's `seed` at `hour`.
pub fn indoors_at(
    seed: Seed,
    hour: f64,
    building: u64,
    level: u32,
    n: u32,
    place: &Place,
    time: f64,
) -> Option<Indoor> {
    let id = INDOOR
        | ((building & ((1 << 48) - 1)) << 14)
        | ((level as u64 & 0x3F) << 8)
        | (n as u64 & 0xFF);
    let own = seed.child("indoors").child(id);
    // Neighbours who talk come and go, and change what they do, together.
    let group = match place.pair {
        Some(k) => seed
            .child("indoors pair")
            .child(building)
            .child(level as u64)
            .child(k as u64),
        None => own,
    };
    let off = group.child("off").unit() * STAY;
    let visit = ((time + off) / STAY).floor();
    let lt = time + off - visit * STAY;
    let gv = group.child(visit as i64 as u64);
    if gv.child("here").unit() >= indoor_share(place.room, hour) {
        return None;
    }
    let v = own.child(visit as i64 as u64);
    let home = matches!(
        place.room,
        RoomKind::Apartment | RoomKind::Living | RoomKind::Kitchen | RoomKind::Bedroom
    );
    let who = if home { own } else { v };
    let look = (who.child("look").0 % LOOKS as u64) as u32;
    let (pos, facing, speed, act) = visit_at(place, gv, v, lt);
    Some(Indoor {
        citizen: Citizen {
            id,
            pos,
            facing,
            speed,
            look,
        },
        act,
    })
}

/// Where someone is `lt` seconds into a visit, facing which way, how fast
/// they walk, and what they do.
fn visit_at(place: &Place, gv: Seed, v: Seed, lt: f64) -> (DVec3, DVec2, f64, Option<Act>) {
    let (at, via) = (place.here(), place.via);
    let seat = place.act.seated();
    // In by the door and out again.
    let door = place.door.filter(|d| flat(*d, via) > 1.0);
    let w = door.map_or(0.0, |d| flat(d, via) / WALK);
    if let Some(d) = door {
        if lt < w {
            let f = lt / w;
            return (d.lerp(via, f), heading(d, via, place.facing), WALK, None);
        }
        // Rising, then out.
        let rise = if seat { SETTLE } else { 0.0 };
        if lt >= STAY - w - rise {
            if lt < STAY - w {
                let f = (lt - (STAY - w - rise)) / rise;
                return (at.lerp(via, f), place.facing, 0.0, None);
            }
            let f = (lt - (STAY - w)) / w;
            return (via.lerp(d, f), heading(via, d, place.facing), WALK, None);
        }
    }
    // Sitting down on arrival.
    let x = lt - w;
    if seat && door.is_some() && x < SETTLE {
        return (via.lerp(at, x / SETTLE), place.facing, 0.0, Some(place.act));
    }
    let x = if seat && door.is_some() {
        x - SETTLE
    } else {
        x
    };
    let len = CYCLE.0 + (CYCLE.1 - CYCLE.0) * v.child("len").unit();
    let k = (x / len).floor();
    let u = x - k * len;
    let cv = gv.child("cycle").child(k as i64 as u64);
    let legs = cycle(place, cv, len);
    let mut t = u;
    let mut prev = place.facing;
    for leg in &legs {
        if t <= leg.dur || std::ptr::eq(leg, legs.last().expect("a cycle has legs")) {
            let f = if leg.dur > 0.0 {
                (t / leg.dur).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let pos = leg.from.lerp(leg.to, f);
            let face = match leg.face {
                // Turning to what they do, over the first moments.
                Some(want) => turned(prev, want, t / TURN),
                None => heading(leg.from, leg.to, prev),
            };
            return (pos, face, leg.speed, leg.act);
        }
        t -= leg.dur;
        prev = match leg.face {
            Some(f) => f,
            None => heading(leg.from, leg.to, prev),
        };
    }
    (at, place.facing, 0.0, Some(place.act))
}

/// The legs of one cycle at `place`, `len` seconds in all.
fn cycle(place: &Place, cv: Seed, len: f64) -> Vec<Leg> {
    let (at, via) = (place.here(), place.via);
    let seat = place.act.seated();
    let a1 = place.act.variant(cv.child("first").0);
    let a2 = place.act.variant(cv.child("second").0);
    let hold = |dur: f64, act: Act| Leg {
        dur,
        from: at,
        to: at,
        face: Some(place.facing),
        act: Some(act),
        speed: 0.0,
    };
    let trips: Vec<PlaceTrip> = place.trips.iter().flatten().copied().collect();
    let chance = if seat { GO_SITTING } else { GO_STANDING };
    if !trips.is_empty() && cv.child("go").unit() < chance {
        let t = trips[(cv.child("which").0 % trips.len() as u64) as usize];
        let walk = flat(via, t.at) / WALK;
        let there = AT_THING.0 + (AT_THING.1 - AT_THING.0) * cv.child("there").unit();
        let settle = if seat { SETTLE } else { 0.0 };
        let rest = len - 2.0 * walk - there - 2.0 * settle;
        if rest >= AT_LEAST {
            let first = rest * (0.35 + 0.3 * cv.child("split").unit());
            let mut legs = vec![hold(first, a1)];
            if seat {
                legs.push(Leg {
                    dur: SETTLE,
                    from: at,
                    to: via,
                    face: Some(place.facing),
                    act: None,
                    speed: 0.0,
                });
            }
            legs.push(Leg {
                dur: walk,
                from: via,
                to: t.at,
                face: None,
                act: None,
                speed: WALK,
            });
            legs.push(Leg {
                dur: there,
                from: t.at,
                to: t.at,
                face: Some(t.facing),
                act: Some(t.act),
                speed: 0.0,
            });
            legs.push(Leg {
                dur: walk,
                from: t.at,
                to: via,
                face: None,
                act: None,
                speed: WALK,
            });
            if seat {
                legs.push(Leg {
                    dur: SETTLE,
                    from: via,
                    to: at,
                    face: Some(place.facing),
                    act: Some(a2),
                    speed: 0.0,
                });
            }
            legs.push(hold(rest - first, a2));
            return legs;
        }
    }
    let first = len * (0.35 + 0.3 * cv.child("split").unit());
    vec![hold(first, a1), hold(len - first, a2)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    /// A town's people for these tests: just its seed and the hour.
    struct Folk(Cell<f64>);
    impl Folk {
        fn set_hour(&self, h: f64) {
            self.0.set(h);
        }
        fn indoors(&self, b: u64, l: u32, n: u32, p: &Place, t: f64) -> Option<Indoor> {
            indoors_at(Seed(5), self.0.get(), b, l, n, p, t)
        }
    }
    fn citizens() -> Folk {
        Folk(Cell::new(11.0))
    }

    fn sitter() -> Place {
        Place {
            room: RoomKind::Office,
            at: DVec3::new(10.0, 0.0, 10.0),
            facing: DVec2::Y,
            act: Act::Type,
            via: DVec3::new(10.0, 0.0, 9.2),
            door: Some(DVec3::new(10.0, 0.0, 0.0)),
            trips: [
                Some(PlaceTrip {
                    at: DVec3::new(14.0, 0.0, 9.2),
                    facing: DVec2::X,
                    act: Act::Sip,
                }),
                None,
            ],
            pair: None,
            hip: 0.6,
        }
    }

    fn walker() -> Place {
        Place {
            room: RoomKind::Shop,
            at: DVec3::new(3.0, 0.0, 3.0),
            facing: DVec2::X,
            act: Act::Browse,
            via: DVec3::new(3.0, 0.0, 3.0),
            door: Some(DVec3::new(0.0, 0.0, 3.0)),
            trips: [
                Some(PlaceTrip {
                    at: DVec3::new(3.0, 0.0, 8.0),
                    facing: DVec2::Y,
                    act: Act::Browse,
                }),
                Some(PlaceTrip {
                    at: DVec3::new(8.0, 0.0, 3.0),
                    facing: DVec2::X,
                    act: Act::Read,
                }),
            ],
            pair: None,
            hip: 0.0,
        }
    }

    /// Someone indoors always does something: across a whole workday and
    /// on every kind of place, they are never still without an act for
    /// more than the moment of sitting down or rising.
    #[test]
    fn nobody_just_stands() {
        let c = citizens();
        c.set_hour(11.0);
        for (name, place) in [("sitter", sitter()), ("walker", walker())] {
            let mut idle = 0.0;
            let mut longest: f64 = 0.0;
            let mut run = 0.0;
            let mut seen = 0.0;
            let dt = 0.25;
            let mut acts = std::collections::BTreeSet::new();
            for n in 0..40 {
                let mut t = 1000.0 + n as f64 * 977.0;
                let end = t + STAY * 3.0;
                while t < end {
                    if let Some(i) = c.indoors(3, 1, n, &place, t) {
                        seen += dt;
                        match i.act {
                            Some(a) => {
                                acts.insert(a);
                                run = 0.0;
                            }
                            None if i.citizen.speed > 0.0 => run = 0.0,
                            None => {
                                idle += dt;
                                run += dt;
                                longest = longest.max(run);
                            }
                        }
                    } else {
                        run = 0.0;
                    }
                    t += dt;
                }
            }
            assert!(seen > 1000.0, "{name}: somebody is there");
            assert!(longest <= 1.0, "{name}: {longest} s still without an act");
            assert!(
                idle / seen < 0.03,
                "{name}: still {:.1}% of the time",
                100.0 * idle / seen
            );
            assert!(acts.len() >= 2, "{name}: only {acts:?}");
        }
    }

    /// They only walk the ways the planner found clear: along the line
    /// from the doorway to the place, and between the place and its trips.
    #[test]
    fn walks_keep_to_the_clear_lines() {
        let c = citizens();
        c.set_hour(11.0);
        let place = walker();
        let lines = [
            (place.door.unwrap(), place.via),
            (place.via, place.trips[0].unwrap().at),
            (place.via, place.trips[1].unwrap().at),
        ];
        let mut walked = 0;
        for n in 0..30 {
            let mut t = 500.0 + n as f64 * 311.0;
            for _ in 0..4000 {
                if let Some(i) = c.indoors(9, 0, n, &place, t) {
                    let p = i.citizen.pos;
                    assert!(i.citizen.speed <= WALK + 1e-9);
                    let near = lines.iter().any(|(a, b)| {
                        let (a, b) = (DVec2::new(a.x, a.z), DVec2::new(b.x, b.z));
                        let q = DVec2::new(p.x, p.z);
                        let ab = b - a;
                        let f = ((q - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
                        q.distance(a + ab * f) < 1e-6
                    });
                    assert!(near, "off the lines at {p:?}");
                    if i.citizen.speed > 0.0 {
                        walked += 1;
                    }
                }
                t += 0.5;
            }
        }
        assert!(walked > 500, "only {walked} steps walked");
    }

    /// The same place at the same time gives the same person doing the
    /// same thing, and a pair of talkers are there together.
    #[test]
    fn indoors_is_a_function_of_place_and_time() {
        let c = citizens();
        c.set_hour(11.0);
        let p = sitter();
        for n in 0..20 {
            let t = 321.0 * n as f64;
            assert_eq!(c.indoors(1, 0, n, &p, t), c.indoors(1, 0, n, &p, t));
        }
        let mut a = walker();
        a.act = Act::Talk;
        a.pair = Some(7);
        a.trips = [None, None];
        let mut b = a;
        b.at = DVec3::new(4.0, 0.0, 3.0);
        b.via = b.at;
        b.door = None;
        let mut both = 0;
        for k in 0..2000 {
            let t = 17.0 * k as f64;
            let (x, y) = (c.indoors(2, 0, 1, &a, t), c.indoors(2, 0, 2, &b, t));
            assert_eq!(x.is_some(), y.is_some(), "a pair is there together");
            if let (Some(x), Some(y)) = (x, y) {
                both += 1;
                assert_ne!(x.citizen.id, y.citizen.id);
            }
        }
        assert!(both > 100);
    }

    /// Hours: offices empty at night, busy by day.
    #[test]
    fn people_indoors_keep_hours() {
        let c = citizens();
        let p = |n: u32, room| {
            let mut pl = sitter();
            pl.room = room;
            c.indoors(7, 1, n, &pl, 1000.0)
        };
        let count = |room| (0..200).filter(|&n| p(n, room).is_some()).count();
        c.set_hour(3.0);
        assert_eq!(count(RoomKind::Office), 0);
        assert_eq!(count(RoomKind::Apartment), 0);
        c.set_hour(11.0);
        assert!(count(RoomKind::Office) > 80);
        assert!(count(RoomKind::Shop) > 50);
        c.set_hour(20.0);
        assert!(count(RoomKind::Apartment) > 70);
        assert!(count(RoomKind::Club) > 70);
    }
}
