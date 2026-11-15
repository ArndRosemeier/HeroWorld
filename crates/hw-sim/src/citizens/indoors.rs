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
    /// Upstairs: the stair climbed on the way in, from its foot to the
    /// doorway's far side (the rest None), before [`Self::door`].
    pub climb: [Option<DVec3>; hw_gen::layout::people::CLIMB],
    /// Instead of a stair: in by a lift, its car at the first point of
    /// `climb` and its door at the second (see
    /// `hw_gen::layout::people::Ride`).
    pub ride: Option<hw_gen::layout::people::Ride>,
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
            climb: [None; hw_gen::layout::people::CLIMB],
            ride: None,
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
    let (pos, facing, speed, act) = match (place.ride, place.door) {
        (Some(r), Some(_)) => by_lift(place, &r, gv, v, time, lt)?,
        _ => visit_at(place, gv, v, lt, STAY),
    };
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

/// A visit that comes and goes by lift: the person is in the car as it
/// rides up to their floor, steps out when its doors open, stays, comes
/// back to the door when the car next arrives, steps in and is carried
/// off. The car's whole timetable is the game clock's (see
/// `hw_gen::layout::lifts`), so it is the very car anyone sees; None while
/// they are not there. `lt` is the time in the slot, which starts at
/// `time - lt`.
fn by_lift(
    place: &Place,
    r: &hw_gen::layout::people::Ride,
    gv: Seed,
    v: Seed,
    time: f64,
    lt: f64,
) -> Option<(DVec3, DVec2, f64, Option<Act>)> {
    use hw_gen::layout::lifts::{car_at, next_arrival, CAR_LIP, DOORS, WAIT};
    let stops = &r.stops[..r.n];
    let s0 = time - lt;
    let (arr, pre) = next_arrival(stops, r.phase, r.stop, s0 + 120.0)?;
    let (back, _) = next_arrival(stops, r.phase, r.stop, s0 + STAY - 420.0)?;
    let car = place.climb[0]?;
    let door_at = place.climb[1]?;
    let heading =
        |from: DVec3, to: DVec3| DVec2::new(to.x - from.x, to.z - from.z).normalize_or_zero();
    let face_door = heading(car, door_at);
    let in_car = |t: f64| {
        let y = car_at(stops, r.phase, t).y + CAR_LIP;
        DVec3::new(car.x, y, car.z)
    };
    // In the car on the way up, and while its doors open.
    let leaves_car = arr + DOORS;
    if time < arr - pre {
        return None;
    }
    if time < leaves_car {
        return Some((in_car(time), face_door, 0.0, Some(Act::Read)));
    }
    // From the door of the car on: the usual visit, which ends back at
    // the car as it comes again.
    let stay = back + DOORS * 0.5 - leaves_car;
    let x = time - leaves_car;
    if x < stay {
        return Some(visit_at(place, gv, v, x, stay));
    }
    // Stepping in, then off.
    if time < back + WAIT + 2.0 {
        return Some((in_car(time), face_door, 0.0, Some(Act::Read)));
    }
    None
}

/// The way in as a polyline: up the stair if there is one, through the
/// doorway, to the place beside the act.
struct Path {
    pts: [DVec3; hw_gen::layout::people::CLIMB + 2],
    n: usize,
    len: f64,
}

impl Path {
    /// The point `d` metres along, and the way it heads (in the ground
    /// plane, unit; zero if the leg is straight up).
    fn at(&self, d: f64) -> (DVec3, DVec2) {
        let mut left = d.clamp(0.0, self.len);
        for k in 0..self.n.saturating_sub(1) {
            let (a, b) = (self.pts[k], self.pts[k + 1]);
            let l = a.distance(b);
            if left <= l || k + 2 == self.n {
                let f = if l > 0.0 {
                    (left / l).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                let dir = DVec2::new(b.x - a.x, b.z - a.z).normalize_or_zero();
                return (a.lerp(b, f), dir);
            }
            left -= l;
        }
        (self.pts[0], DVec2::ZERO)
    }
}

fn entry_path(place: &Place, door: Option<DVec3>) -> Path {
    let mut pts = [DVec3::ZERO; hw_gen::layout::people::CLIMB + 2];
    let mut n = 0;
    if let Some(d) = door {
        for p in place.climb.iter().flatten() {
            pts[n] = *p;
            n += 1;
        }
        pts[n] = d;
        pts[n + 1] = place.via;
        n += 2;
    }
    let len = (0..n.saturating_sub(1))
        .map(|k| pts[k].distance(pts[k + 1]))
        .sum();
    Path { pts, n, len }
}

/// The way `dir` (a ground direction) heads, else `keep`.
fn heading_to(dir: DVec2, keep: DVec2) -> DVec2 {
    if dir.length_squared() > 1e-9 {
        dir
    } else {
        keep
    }
}

/// Where someone is `lt` seconds into a visit, facing which way, how fast
/// they walk, and what they do.
fn visit_at(
    place: &Place,
    gv: Seed,
    v: Seed,
    lt: f64,
    stay: f64,
) -> (DVec3, DVec2, f64, Option<Act>) {
    let (at, via) = (place.here(), place.via);
    let seat = place.act.seated();
    // In by the door (up a stair first, upstairs) and out again.
    let door = place.door.filter(|d| flat(*d, via) > 1.0);
    let path = entry_path(place, door);
    let w = path.len / WALK;
    if door.is_some() {
        if lt < w {
            let (p, dir) = path.at(lt * WALK);
            return (p, heading_to(dir, place.facing), WALK, None);
        }
        // Rising, then out.
        let rise = if seat { SETTLE } else { 0.0 };
        if lt >= stay - w - rise {
            if lt < stay - w {
                let f = (lt - (stay - w - rise)) / rise;
                return (at.lerp(via, f), place.facing, 0.0, None);
            }
            let (p, dir) = path.at(path.len - (lt - (stay - w)) * WALK);
            return (p, heading_to(-dir, place.facing), WALK, None);
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

fn forced_act() -> Option<Act> {
    static FORCE: std::sync::OnceLock<Option<Act>> = std::sync::OnceLock::new();
    *FORCE.get_or_init(|| {
        let want = std::env::var("HW_ACT_FORCE").ok()?;
        Act::ALL.into_iter().find(|a| format!("{a:?}") == want)
    })
}

/// The legs of one cycle at `place`, `len` seconds in all.
fn cycle(place: &Place, cv: Seed, len: f64) -> Vec<Leg> {
    let (at, via) = (place.here(), place.via);
    let seat = place.act.seated();
    let (mut a1, mut a2) = (
        place.act.variant(cv.child("first").0),
        place.act.variant(cv.child("second").0),
    );
    // (`HW_ACT_FORCE=<Act>`, for looking at one act's pose: everyone standing
    // does it, wherever they are.)
    if let Some(a) = forced_act() {
        if !place.act.seated() && !a.seated() {
            (a1, a2) = (a, a);
        }
    }
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
            climb: [None; hw_gen::layout::people::CLIMB],
            ride: None,
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
            climb: [None; hw_gen::layout::people::CLIMB],
            ride: None,
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

    /// Upstairs, the way in climbs: from the stair's foot, rising along it,
    /// to the doorway and the place; the way out retraces it.
    #[test]
    fn people_climb_in_and_down_out() {
        let mut p = walker();
        p.at = DVec3::new(0.0, 4.0, 3.0);
        p.via = p.at;
        p.door = Some(DVec3::new(0.0, 4.0, 8.0));
        p.climb = [None; hw_gen::layout::people::CLIMB];
        p.climb[0] = Some(DVec3::new(10.0, 0.0, 8.0));
        p.climb[1] = Some(DVec3::new(10.0, 4.0, 8.0));
        p.climb[2] = Some(DVec3::new(2.0, 4.0, 8.0));
        let c = citizens();
        c.set_hour(11.0);
        let mut found = 0;
        for n in 0..60 {
            let at = |t: f64| c.indoors(3, 1, n, &p, t).map(|i| i.citizen.pos);
            let t0 = 1000.0 + n as f64 * 977.0;
            // Find the start of a visit: first moment present.
            let Some(start) = (0..(STAY as i64 * 3))
                .map(|k| t0 + k as f64)
                .find(|t| at(*t).is_some() && at(*t - 1.0).is_none())
            else {
                continue;
            };
            found += 1;
            let a = at(start + 0.5).expect("present");
            assert!(a.y < 2.5, "starts at the foot: {a:?}");
            let b = at(start + 12.0).expect("present");
            assert!(b.y > a.y, "rises: {a:?} {b:?}");
            // Never jumps more than a walking pace in a second.
            let mut prev = at(start + 0.5).expect("present");
            for k in 1..40 {
                let q = at(start + 0.5 + k as f64 * 0.5).expect("present");
                assert!(q.distance(prev) < 1.5, "jump {prev:?} -> {q:?}");
                prev = q;
            }
        }
        assert!(found > 0);
    }

    /// By lift: nobody before the car rides up, in the car as it arrives
    /// (at the car's own height), out at its door, and back in the car
    /// when it comes again.
    #[test]
    fn people_come_and_go_by_lift() {
        use hw_gen::layout::lifts::{car_at, next_arrival, WAIT};
        let mut stops = [0.0; hw_gen::layout::people::RIDE_STOPS];
        stops[..4].copy_from_slice(&[0.0, 4.0, 8.0, 12.0]);
        let ride = hw_gen::layout::people::Ride {
            stops,
            n: 4,
            phase: 11.0,
            stop: 2,
        };
        let mut p = walker();
        p.at = DVec3::new(0.0, 8.0, 3.0);
        p.via = p.at;
        p.door = Some(DVec3::new(0.0, 8.0, 8.0));
        p.climb = [None; hw_gen::layout::people::CLIMB];
        p.climb[0] = Some(DVec3::new(10.0, 8.0, 8.0));
        p.climb[1] = Some(DVec3::new(8.0, 8.0, 8.0));
        p.ride = Some(ride);
        p.trips = [None, None];
        let c = citizens();
        c.set_hour(11.0);
        let mut seen = 0;
        for n in 0..40 {
            let t0 = 1000.0 + n as f64 * 977.0;
            let mut prev: Option<DVec3> = None;
            let mut appeared = false;
            let mut t = t0;
            while t < t0 + STAY * 1.2 {
                let here = c.indoors(3, 1, n, &p, t).map(|i| i.citizen.pos);
                if let (Some(a), Some(b)) = (prev, here) {
                    assert!(a.distance(b) < 3.0, "jump {a:?} -> {b:?} at {t}");
                }
                if let Some(h) = here {
                    // In the car: at the car's own height and place.
                    if h.x == 10.0 && h.z == 8.0 {
                        let want = car_at(&stops[..4], 11.0, t).y + 0.05;
                        assert!((h.y - want).abs() < 1e-6, "{h:?} at {t}");
                        appeared = true;
                    }
                }
                prev = here;
                t += 0.5;
            }
            if appeared {
                seen += 1;
            }
            // Whoever is in the car when it is at the stop with its doors open.
            let (arr, _) = next_arrival(&stops[..4], 11.0, 2, t0).expect("stops");
            let _ = (arr, WAIT);
        }
        assert!(seen > 0, "nobody rode");
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
