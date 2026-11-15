//! Where people are in a building at game scale, and what they do there.
//! Every place has an act that fits what is at hand: typing on a chair at
//! a desk, standing at a shelf, cooking at a stove, serving behind a
//! counter, talking with a neighbour in a lobby. Places are on open floor,
//! clear of walls, furniture, stairs and doorways, and they come with the
//! short walks the one there takes now and then (to the water cooler, a
//! bookcase, the stove) and the doorway they come in by and leave through.
//! Who is there, and when, is the game's business (`hw_sim::citizens`).

use super::furnish::{Item, ItemKind};
use super::solid::{pieces, Part};
use super::{Flight, Layout, LinkKind, SpaceKind};
use crate::urban::footprint::Footprint;
use glam::{DVec2, DVec3};
use hw_core::{Act, Seed};

/// A short walk from a place to a thing it does something at, and back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trip {
    /// Where they stand there (the layout's frame: x, floor height, z).
    pub at: DVec3,
    /// The way they face there.
    pub face: DVec2,
    pub act: Act,
}

/// A place to be.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stand {
    /// In the layout's frame: (x, floor height, z). For someone sitting,
    /// the seat.
    pub at: DVec3,
    /// The way the one there looks.
    pub face: DVec2,
    /// The room it is in (an index into the layout's spaces).
    pub space: usize,
    /// What is done here (see [`Act::variant`] for what else).
    pub act: Act,
    /// Where one steps to and from (beside the seat of a sitter, else
    /// the place itself): the walks start and end here, in clear floor.
    pub via: DVec3,
    /// Where one comes into the room and leaves it, if a straight clear
    /// walk leads from there.
    pub door: Option<DVec3>,
    /// On an upper floor, where the way in climbs a stair first: the path
    /// from the stair's foot to the doorway's far side (in order; the rest
    /// None), each leg straight and inside one room or on a flight.
    pub climb: [Option<DVec3>; CLIMB],
    /// Up to two walks to something else in the room, each straight and
    /// clear from [`Self::via`].
    pub trips: [Option<Trip>; 2],
    /// Neighbours who talk to one another share this key, and come and go
    /// together.
    pub pair: Option<u32>,
    /// For a sitter: how high their hips are over the floor (m).
    pub hip: f64,
}

impl Stand {
    /// Is it a seat?
    pub fn seat(&self) -> bool {
        self.act.seated()
    }
}

/// Candidate places lie on a grid this wide (m); a body needs this much
/// room to the next thing, and this much to a wall.
const GRID: f64 = 1.6;
const ROOM: f64 = 0.55;
const OFF_WALL: f64 = 0.9;
/// Nobody stands within this of the middle of a doorway.
const DOORWAY: f64 = 2.6;
/// Walks keep this far from things (m).
const CLEAR: f64 = 0.4;
/// The longest short walk (m), and the longest from a doorway.
pub const TRIP_MAX: f64 = 9.0;
pub const DOOR_MAX: f64 = 16.0;
/// The most points of a climb (three flights and their landings).
pub const CLIMB: usize = 8;
/// The sitter's hips stand this far back of the middle of a seat.
const SEAT_BACK: f64 = 0.04;
/// The hips' joint is this far above the cushion (m).
const HIP: f64 = 0.1;

/// Floor area to one free place, by the room's use; None where nobody
/// stands about (people stand at what is in the room, see [`anchors`]).
fn area_each(kind: SpaceKind) -> Option<f64> {
    match kind {
        SpaceKind::Hall | SpaceKind::Boardroom => Some(40.0),
        SpaceKind::Shop => Some(30.0),
        SpaceKind::Living | SpaceKind::Kitchen => Some(45.0),
        SpaceKind::Lobby | SpaceKind::Atrium | SpaceKind::Vestibule => Some(34.0),
        SpaceKind::Gallery | SpaceKind::Arcade | SpaceKind::Concourse => Some(48.0),
        SpaceKind::Bridge
        | SpaceKind::Landing
        | SpaceKind::StairHall
        | SpaceKind::Shaft
        | SpaceKind::Canyon
        | SpaceKind::Bedroom
        | SpaceKind::Service => None,
    }
}

/// What a body does standing at `kind`, if anything (the piece's own
/// pieces for sitting are [`seats`]).
fn at_piece(kind: ItemKind) -> Option<Act> {
    use ItemKind::*;
    Some(match kind {
        Cabinet | Bookcase | Shelves | Sideboard | Wardrobe | PinBoard | Jukebox | Fridge
        | Crates => Act::Browse,
        Case | RocketModel | AtomModel | Orrery => Act::Admire,
        WaterCooler => Act::Sip,
        KitchenRun | Stove => Act::Cook,
        LabBench | AetherCoil | Workbench | Press | Lathe | Dynamo => Act::Tinker,
        _ => return None,
    })
}

/// A seat: where the hips are, the way the sitter faces, and what they do.
struct Seat {
    at: DVec2,
    face: DVec2,
    act: Act,
    /// The hips' height over the floor.
    hip: f64,
}

/// The seats a piece has.
fn seats(it: &Item, desk_ahead: bool, h: u64) -> Vec<Seat> {
    let back = -it.face * SEAT_BACK;
    let pick = |opts: &[Act]| opts[(h % opts.len() as u64) as usize];
    match it.kind {
        ItemKind::SwivelChair => vec![Seat {
            at: it.at + back,
            face: it.face,
            act: if desk_ahead { Act::Type } else { Act::SitRead },
            hip: 0.5 + HIP,
        }],
        ItemKind::Chair => vec![Seat {
            at: it.at + back,
            face: it.face,
            act: pick(&[Act::SitSip, Act::SitTalk, Act::SitTalk, Act::Write]),
            hip: 0.46 + HIP,
        }],
        ItemKind::Armchair => vec![Seat {
            at: it.at + back,
            face: it.face,
            act: pick(&[Act::SitRelax, Act::SitRead]),
            hip: 0.42 + HIP,
        }],
        ItemKind::Sofa => {
            let right = it.face.perp();
            let n = ((it.w / 0.9).floor() as usize).clamp(1, 3);
            (0..n)
                .map(|k| {
                    let x = (k as f64 - (n - 1) as f64 * 0.5) * 0.85;
                    Seat {
                        at: it.at + right * x + back,
                        face: it.face,
                        act: pick(&[Act::SitRelax, Act::SitRelax, Act::SitTalk]),
                        hip: 0.42 + HIP,
                    }
                })
                .collect()
        }
        ItemKind::BarStool => vec![Seat {
            at: it.at + back,
            face: it.face,
            act: pick(&[Act::SitSip, Act::SitTalk]),
            hip: 0.72 + HIP,
        }],
        ItemKind::Booth => {
            // Two benches either side of the table, along the booth.
            let right = it.face.perp();
            [-1.0, 1.0]
                .into_iter()
                .map(|s| Seat {
                    at: it.at + right * (s * 0.5),
                    face: -right * s,
                    act: pick(&[Act::SitTalk, Act::SitSip, Act::SitTalk]),
                    hip: 0.45 + HIP,
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

/// What stands in one room, for the checks places have to pass.
struct Floor {
    rect: crate::geom::Rect,
    /// What stands in the room up to head height, and the furniture it
    /// is (its index in `items`).
    things: Vec<(Footprint, Option<usize>)>,
    doors: Vec<DVec2>,
    /// Which doorways one can come in by.
    entries: Vec<DVec2>,
    /// For each entry: the opening on the wall line, and the space beyond.
    beyond: Vec<(DVec2, Option<usize>)>,
}

impl Floor {
    /// Is `p` at least `margin` clear of everything but `ignore`?
    fn free(&self, p: DVec2, margin: f64, ignore: &[usize]) -> bool {
        self.things
            .iter()
            .all(|(f, o)| o.is_some_and(|o| ignore.contains(&o)) || f.distance(p) >= margin)
    }

    /// Is the straight walk from `a` to `b` clear of everything but `ignore`?
    fn walk_clear(&self, a: DVec2, b: DVec2, ignore: &[usize]) -> bool {
        let n = (a.distance(b) / 0.2).ceil().max(1.0) as usize;
        (0..=n).all(|k| {
            let p = a.lerp(b, k as f64 / n as f64);
            self.rect.contains_point(p) && self.free(p, CLEAR, ignore)
        })
    }

    fn doorway(&self, p: DVec2) -> bool {
        self.doors.iter().any(|d| d.distance(p) < DOORWAY)
    }
}

/// Every place to be in the building, room by room, with what is at hand:
/// `items` is the furniture of the building ([`super::furnish::items_for`]),
/// `club` the space (if any) furnished as a night club.
pub fn standing(l: &Layout, items: &[Item], club: Option<usize>) -> Vec<Stand> {
    let solid = pieces(l);
    let feet: Vec<Option<Footprint>> = items.iter().map(|i| i.footprint()).collect();
    let seed = Seed(l.size.x.to_bits())
        .child(l.size.y.to_bits())
        .child(l.spaces.len() as u64)
        .child("standing");
    let mut out = Vec::new();
    for (i, s) in l.spaces.iter().enumerate() {
        if s.within.is_some() {
            continue;
        }
        if matches!(
            s.kind,
            SpaceKind::Bridge
                | SpaceKind::Landing
                | SpaceKind::StairHall
                | SpaceKind::Shaft
                | SpaceKind::Canyon
                | SpaceKind::Bedroom
                | SpaceKind::Service
        ) {
            continue;
        }
        // What stands in the room up to head height.
        let near = |f: &Footprint| {
            let (y0, y1) = f.span();
            let (c, r) = (f.at(), f.reach() + ROOM);
            y1 > s.y + 0.15
                && y0 < s.y + 1.9
                && c.x > s.rect.min.x - r
                && c.x < s.rect.max.x + r
                && c.y > s.rect.min.y - r
                && c.y < s.rect.max.y + r
        };
        let mut things: Vec<(Footprint, Option<usize>)> = solid
            .iter()
            .filter(|p| !matches!(p.part, Part::Floor(_) | Part::Roof))
            .map(|p| p.shape)
            .filter(near)
            .map(|f| (f, None))
            .collect();
        for (k, f) in feet.iter().enumerate() {
            if let Some(f) = f.filter(near) {
                things.push((f, Some(k)));
            }
        }
        let links = || {
            l.links.iter().filter(|k| {
                (k.a == i || k.b == Some(i))
                    && (k.y - s.y).abs() < 0.5
                    && !matches!(k.kind, LinkKind::Rail | LinkKind::Glass)
            })
        };
        let doors: Vec<DVec2> = links().map(|k| k.at).collect();
        let c = s.rect.center();
        // Where a doorway lets in: just inside, on the room's side.
        let (entries, beyond): (Vec<DVec2>, Vec<(DVec2, Option<usize>)>) = links()
            .filter(|k| {
                matches!(
                    k.kind,
                    LinkKind::Door | LinkKind::Double | LinkKind::Arch | LinkKind::Entrance
                )
            })
            .map(|k| {
                let inward = if k.along_x {
                    DVec2::new(0.0, (c.y - k.at.y).signum())
                } else {
                    DVec2::new((c.x - k.at.x).signum(), 0.0)
                };
                let other = if k.a == i { k.b } else { Some(k.a) };
                (k.at + inward * 0.9, (k.at, other))
            })
            .filter(|(p, _)| s.rect.contains_point(*p))
            .unzip();
        let inner = s.rect.inset(OFF_WALL);
        let floor = Floor {
            rect: s.rect.inset(0.3),
            things,
            doors,
            entries,
            beyond,
        };
        let room = seed.child(i as u64);
        let mine: Vec<usize> = (0..items.len()).filter(|&k| items[k].space == i).collect();
        let y = s.y;
        let mut places: Vec<Stand> = Vec::new();
        // --- At what is in the room ---
        for &k in &mine {
            let it = &items[k];
            let h = room.child("item").child(k as u64).0;
            let right = it.face.perp();
            let (depth, _) = it.kind.depth_height();
            // Sitting: the seat, and where one steps from it.
            let desk_ahead = items.iter().any(|d| {
                d.kind == ItemKind::Desk
                    && d.space == i
                    && (d.at - it.at).dot(it.face) > 0.2
                    && d.at.distance(it.at) < 1.4
            });
            for seat in seats(it, desk_ahead, h) {
                let Some(via) = [
                    -it.face * 0.8,
                    right * 0.85,
                    -right * 0.85,
                    it.face * 1.0,
                    -it.face * 1.3,
                ]
                .into_iter()
                .map(|o| it.at + o)
                .find(|p| {
                    floor.rect.contains_point(*p)
                        && floor.free(*p, 0.45, &[k])
                        && floor.walk_clear(seat.at, *p, &[k])
                }) else {
                    // Boxed in: it stays a seat that is just taken or not.
                    places.push(Stand {
                        at: DVec3::new(seat.at.x, y, seat.at.y),
                        face: seat.face,
                        space: i,
                        act: seat.act,
                        via: DVec3::new(seat.at.x, y, seat.at.y),
                        door: None,
                        climb: [None; CLIMB],
                        trips: [None, None],
                        pair: None,
                        hip: seat.hip,
                    });
                    continue;
                };
                places.push(Stand {
                    at: DVec3::new(seat.at.x, y, seat.at.y),
                    face: seat.face,
                    space: i,
                    act: seat.act,
                    via: DVec3::new(via.x, y, via.y),
                    door: None,
                    climb: [None; CLIMB],
                    trips: [None, None],
                    pair: None,
                    hip: seat.hip,
                });
            }
            // Standing at it.
            let front = it.at + it.face * (depth * 0.5 + ROOM);
            let mut stand = |p: DVec2, face: DVec2, act: Act, ignore: &[usize]| {
                if floor.rect.contains_point(p) && floor.free(p, 0.45, ignore) && !floor.doorway(p)
                {
                    places.push(Stand {
                        at: DVec3::new(p.x, y, p.y),
                        face,
                        space: i,
                        act,
                        via: DVec3::new(p.x, y, p.y),
                        door: None,
                        climb: [None; CLIMB],
                        trips: [None, None],
                        pair: None,
                        hip: 0.0,
                    });
                }
            };
            match it.kind {
                ItemKind::Counter => {
                    // The clerk behind it, a customer in front.
                    let behind = it.at - it.face * (depth * 0.5 + ROOM + 0.1);
                    stand(behind, it.face, Act::Serve, &[]);
                    let act = [Act::Lean, Act::Phone, Act::Browse][(h % 3) as usize];
                    stand(front, -it.face, act, &[]);
                }
                ItemKind::Bar => {
                    // The barman in his walk between the back bar and
                    // the counter, one to every four metres of bar.
                    let n = ((it.w / 4.0).floor() as usize).clamp(1, 3);
                    for j in 0..n {
                        let x = (j as f64 - (n - 1) as f64 * 0.5) * 3.2;
                        let p = it.at + right * x - it.face * 0.1;
                        stand(p, it.face, Act::Serve, &[k]);
                    }
                    // Customers leaning on it where there are no stools.
                    for x in [-0.3, 0.3] {
                        let p = it.at + right * (it.w * x) + it.face * (depth * 0.5 + ROOM);
                        stand(p, -it.face, Act::Lean, &[]);
                    }
                }
                kind => {
                    if let Some(act) = at_piece(kind) {
                        // Shelves and cases are browsed from the front,
                        // a stove or a bench worked at.
                        let wide = it.w > 2.4;
                        let xs: &[f64] = if wide { &[-0.28, 0.28] } else { &[0.0] };
                        for x in xs {
                            stand(front + right * (it.w * x), -it.face, act, &[]);
                        }
                    }
                }
            }
        }
        // --- A dance floor ---
        if club == Some(i) {
            for &k in &mine {
                if items[k].kind != ItemKind::Rug {
                    continue;
                }
                let (c, r) = (items[k].at, items[k].w * 0.5 - 0.5);
                let n = (r / 0.55).floor() as i32;
                for ix in -n..=n {
                    for iz in -n..=n {
                        let hh = room
                            .child("dance")
                            .child(k as u64)
                            .child(((ix + 20) * 64 + iz + 20) as u64);
                        if hh.child("here").unit() > 0.28 {
                            continue;
                        }
                        let p = c
                            + DVec2::new(ix as f64, iz as f64) * 1.05
                            + (DVec2::new(hh.child("x").unit(), hh.child("z").unit()) - 0.5) * 0.4;
                        if p.distance(c) > r || !floor.free(p, ROOM, &[]) {
                            continue;
                        }
                        places.push(Stand {
                            at: DVec3::new(p.x, y, p.y),
                            face: hw_core::det::dir2(
                                hh.child("turn").unit() * std::f64::consts::TAU,
                            ),
                            space: i,
                            act: Act::Dance,
                            via: DVec3::new(p.x, y, p.y),
                            door: None,
                            climb: [None; CLIMB],
                            trips: [None, None],
                            pair: None,
                            hip: 0.0,
                        });
                    }
                }
            }
        }
        // --- On open floor ---
        if let Some(each) = area_each(s.kind) {
            let size = inner.size();
            if size.min_element() > 0.0 {
                let keep = (GRID * GRID / each).min(1.0);
                let (nx, nz) = ((size.x / GRID) as u32, (size.y / GRID) as u32);
                let mut taken: Vec<DVec2> = Vec::new();
                for ix in 0..=nx {
                    for iz in 0..=nz {
                        let hh = room.child(((ix as u64) << 32) | iz as u64);
                        if hh.child("here").unit() >= keep {
                            continue;
                        }
                        let jitter = DVec2::new(hh.child("x").unit(), hh.child("z").unit()) - 0.5;
                        let p =
                            (inner.min + DVec2::new(ix as f64, iz as f64) * GRID + jitter * 0.5)
                                .clamp(inner.min, inner.max);
                        if !floor.free(p, ROOM, &[]) || floor.doorway(p) {
                            continue;
                        }
                        if taken.iter().any(|q| q.distance(p) < 1.0) {
                            continue;
                        }
                        // Turned to the nearest piece (a shelf, a desk, a
                        // counter), or any way at all.
                        let by = floor
                            .things
                            .iter()
                            .filter_map(|(f, o)| o.map(|o| (f.distance(p), f.at(), o)))
                            .filter(|(d, _, _)| *d < 1.6)
                            .min_by(|a, b| a.0.total_cmp(&b.0));
                        let any =
                            hw_core::det::dir2(hh.child("turn").unit() * std::f64::consts::TAU);
                        let face = by.map_or(any, |(_, at, _)| (at - p).normalize_or(any));
                        let act = match by.and_then(|(_, _, o)| at_piece(items[o].kind)) {
                            Some(a) => a,
                            None => free_act(s.kind, hh.child("act").0),
                        };
                        // Two neighbours talking, where the floor is open.
                        let free_floor = by.is_none();
                        if free_floor && hh.child("pair").unit() < 0.5 {
                            let dir = hw_core::det::dir2(
                                hh.child("pair dir").unit() * std::f64::consts::TAU,
                            );
                            let q = p + dir * 1.15;
                            if inner.contains_point(q)
                                && floor.free(q, ROOM, &[])
                                && !floor.doorway(q)
                            {
                                let key = ix * 1024 + iz;
                                for (a, f) in [(p, dir), (q, -dir)] {
                                    places.push(Stand {
                                        at: DVec3::new(a.x, y, a.y),
                                        face: f,
                                        space: i,
                                        act: Act::Talk,
                                        via: DVec3::new(a.x, y, a.y),
                                        door: None,
                                        climb: [None; CLIMB],
                                        trips: [None, None],
                                        pair: Some(key),
                                        hip: 0.0,
                                    });
                                }
                                taken.push(p);
                                taken.push(q);
                                continue;
                            }
                        }
                        taken.push(p);
                        places.push(Stand {
                            at: DVec3::new(p.x, y, p.y),
                            face,
                            space: i,
                            act,
                            via: DVec3::new(p.x, y, p.y),
                            door: None,
                            climb: [None; CLIMB],
                            trips: [None, None],
                            pair: None,
                            hip: 0.0,
                        });
                    }
                }
            }
        }
        // --- The walks: to what else is in the room, and the way in ---
        let targets: Vec<Trip> = places
            .iter()
            .filter(|p| !p.seat() && p.pair.is_none() && p.act != Act::Dance)
            .filter(|p| p.act != Act::Serve)
            .map(|p| Trip {
                at: p.at,
                face: p.face,
                act: p.act,
            })
            .collect();
        for (n, st) in places.iter_mut().enumerate() {
            if st.pair.is_some() || st.act == Act::Dance {
                continue;
            }
            let via = DVec2::new(st.via.x, st.via.z);
            let h = room.child("walks").child(n as u64);
            let mut ok: Vec<&Trip> = targets
                .iter()
                .filter(|t| {
                    let to = DVec2::new(t.at.x, t.at.z);
                    to.distance(via) > 1.5
                        && to.distance(via) <= TRIP_MAX
                        && t.act != st.act
                        && floor.walk_clear(via, to, &[])
                })
                .collect();
            // Which two: spread by a hash, not the nearest.
            let mut chosen = [None, None];
            for slot in chosen.iter_mut() {
                if ok.is_empty() {
                    break;
                }
                let j = (h.child(slot_key(ok.len())).0 % ok.len() as u64) as usize;
                *slot = Some(*ok.remove(j));
            }
            // Where nothing in the room is in reach, a stroll across the
            // open floor, to look about or take a call.
            if !st.seat() {
                for (j, slot) in chosen.iter_mut().enumerate() {
                    if slot.is_some() {
                        continue;
                    }
                    let first = (h.child("stroll").child(j as u64).0 % 8) as usize;
                    for d in 0..8 {
                        let a = (first + d) as f64 * std::f64::consts::FRAC_PI_4;
                        let len = 2.5 + 2.0 * h.child("len").child(j as u64).unit();
                        let to = via + DVec2::new(a.cos(), a.sin()) * len;
                        if floor.rect.contains_point(to)
                            && !floor.doorway(to)
                            && floor.free(to, ROOM, &[])
                            && floor.walk_clear(via, to, &[])
                        {
                            let mut act = free_act(s.kind, h.child("walk act").child(j as u64).0);
                            if act == st.act {
                                act = act.variant(h.child("other").0 | 1);
                            }
                            *slot = Some(Trip {
                                at: DVec3::new(to.x, y, to.y),
                                face: DVec2::new(a.cos(), a.sin()),
                                act,
                            });
                            break;
                        }
                    }
                }
            }
            st.trips = chosen;
            // The way in: the nearest doorway with a clear straight walk.
            let mut ways: Vec<(f64, usize)> = floor
                .entries
                .iter()
                .enumerate()
                .map(|(k, d)| (d.distance(via), k))
                .filter(|(d, _)| *d <= DOOR_MAX && *d > 1.0)
                .filter(|(_, k)| floor.walk_clear(floor.entries[*k], via, &[]))
                .collect();
            ways.sort_by(|a, b| a.0.total_cmp(&b.0));
            // Upstairs, in from a stair if any doorway leads to the step-off
            // of one (the nearest such), else by the nearest doorway.
            let climbing = ways.iter().find_map(|(_, k)| {
                let (wall, other) = floor.beyond[*k];
                climb_to(l, other?, y, wall).map(|c| (*k, c))
            });
            if let Some((k, c)) = climbing {
                st.climb = c;
                st.door = Some(DVec3::new(floor.entries[k].x, y, floor.entries[k].y));
            } else {
                st.door = ways
                    .first()
                    .map(|(_, k)| DVec3::new(floor.entries[*k].x, y, floor.entries[*k].y));
            }
        }
        out.extend(places);
    }
    out
}

/// The path up a stair to its step-off at height `y`, in the space `host`
/// (the room's neighbour beyond `wall`, the opening): from the foot of
/// the stair, flight after flight over their landings, to the opening.
/// Straight flights only; None unless every leg lies in one room or on a
/// flight, so nobody climbs through a wall or a step.
fn climb_to(l: &Layout, host: usize, y: f64, wall: DVec2) -> Option<[Option<DVec3>; CLIMB]> {
    let sp = l.spaces.get(host)?;
    let room = sp.rect.inset(-0.05);
    if !room.contains_point(wall) {
        return None;
    }
    let plan = |f: &Flight| -> Option<(DVec3, DVec3)> {
        match *f {
            Flight::Straight {
                from, dir, risers, ..
            } => {
                let foot = DVec3::new(from.x, from.y, from.z) - DVec3::new(dir.x, 0.0, dir.y) * 0.3;
                let run = Flight::run(risers) + 0.5;
                let top = DVec3::new(from.x + dir.x * run, f.y1(), from.z + dir.y * run);
                Some((foot, top))
            }
            _ => None,
        }
    };
    for st in l.stairs.iter().filter(|st| st.host == host) {
        let low = st
            .flights
            .iter()
            .map(|f| f.y0())
            .fold(f64::INFINITY, f64::min);
        for last in st.flights.iter().filter(|f| (f.y1() - y).abs() < 0.2) {
            // Back from the top flight to the bottom.
            let mut chain = vec![last];
            while chain.last().expect("a flight").y0() > low + 0.2 && chain.len() < 4 {
                let cur = chain.last().expect("a flight");
                let Some((foot, _)) = plan(cur) else { break };
                let prev = st.flights.iter().find(|g| {
                    (g.y1() - cur.y0()).abs() < 0.2
                        && plan(g).is_some_and(|(_, t)| {
                            st.landings.iter().any(|ld| {
                                (ld.y - cur.y0()).abs() < 0.2 && {
                                    let r = ld.rect.inset(-0.3);
                                    r.contains_point(DVec2::new(t.x, t.z))
                                        && r.contains_point(DVec2::new(foot.x, foot.z))
                                }
                            })
                        })
                });
                match prev {
                    Some(g) => chain.push(g),
                    None => break,
                }
            }
            if chain.last().expect("a flight").y0() > low + 0.2 {
                continue;
            }
            chain.reverse();
            let mut path: Vec<DVec3> = Vec::new();
            let mut ok = true;
            for f in &chain {
                match plan(f) {
                    Some((foot, top)) => {
                        path.push(foot);
                        path.push(top);
                    }
                    None => ok = false,
                }
            }
            let Some((_, top)) = plan(last) else { continue };
            if !ok || path.len() + 1 > CLIMB {
                continue;
            }
            // From the step-off to the opening: inside the host, clear of
            // every flight at another height.
            let (a, b) = (DVec2::new(top.x, top.z), wall);
            if !room.contains_point(a) {
                continue;
            }
            let n = ((a.distance(b) / 0.25).ceil() as usize).max(1);
            let blocked = (0..=n).any(|k| {
                let p = a.lerp(b, k as f64 / n as f64);
                st.flights
                    .iter()
                    .any(|g| g.height_at(p).is_some_and(|h| (h - y).abs() > 0.35))
            });
            if blocked {
                continue;
            }
            path.push(DVec3::new(wall.x, y, wall.y));
            let mut out = [None; CLIMB];
            for (k, p) in path.into_iter().enumerate() {
                out[k] = Some(p);
            }
            return Some(out);
        }
    }
    None
}

fn slot_key(n: usize) -> u64 {
    n as u64 + 0x51
}

/// What someone standing on open floor does, in a room of `kind`.
fn free_act(kind: SpaceKind, h: u64) -> Act {
    let pick = |opts: &[Act]| opts[(h % opts.len() as u64) as usize];
    match kind {
        SpaceKind::Shop => pick(&[Act::Browse, Act::Read, Act::Phone, Act::Browse]),
        SpaceKind::Kitchen => pick(&[Act::Cook, Act::Wipe, Act::Sip]),
        SpaceKind::Living => pick(&[Act::Sip, Act::Read, Act::Phone]),
        SpaceKind::Hall | SpaceKind::Boardroom => {
            pick(&[Act::Read, Act::Phone, Act::Sip, Act::Read])
        }
        SpaceKind::Gallery | SpaceKind::Arcade | SpaceKind::Concourse => {
            pick(&[Act::Admire, Act::Phone, Act::Read, Act::Admire])
        }
        _ => pick(&[Act::Phone, Act::Read, Act::Admire, Act::Sip, Act::Read]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::office::plan_town;

    fn all(k: u64) -> (Layout, Vec<Item>, Vec<Stand>) {
        let l = plan_town(Seed(40 + k), DVec2::new(40.0 + 3.0 * k as f64, 46.0), 4);
        let items = crate::layout::furnish::items(&l);
        let st = standing(&l, &items, None);
        (l, items, st)
    }

    /// Halls, shops and lobbies all have places, on their floors, in
    /// their rooms; those on their feet clear of everything solid.
    #[test]
    fn people_stand_on_open_floor() {
        for k in 0..6u64 {
            let (l, items, stands) = all(k);
            assert!(stands.len() > 60, "seed {k}: {} places", stands.len());
            let solid = pieces(&l);
            for st in &stands {
                let s = &l.spaces[st.space];
                let p = DVec2::new(st.at.x, st.at.z);
                assert_eq!(st.at.y, s.y);
                assert_eq!(p.clamp(s.rect.min, s.rect.max), p);
                assert!((st.face.length() - 1.0).abs() < 1e-9);
                for part in &solid {
                    if matches!(part.part, Part::Floor(_) | Part::Roof) {
                        continue;
                    }
                    let (y0, y1) = part.shape.span();
                    assert!(
                        y1 <= s.y + 0.15
                            || y0 >= s.y + 1.9
                            || part.shape.distance(p) >= 0.45 - 1e-9,
                        "seed {k}: a place at {p:?} in {:?} is in a {:?}",
                        s.kind,
                        part.part
                    );
                }
                if !st.seat() {
                    for it in &items {
                        if let Some(f) = it.footprint() {
                            let (y0, y1) = f.span();
                            assert!(
                                y1 <= s.y + 0.15 || y0 >= s.y + 1.9 || f.distance(p) >= 0.44,
                                "seed {k}: a {:?} at {p:?} stands in a {:?}",
                                st.act,
                                it.kind
                            );
                        }
                    }
                }
            }
            assert_eq!(stands, standing(&l, &items, None));
        }
    }

    /// People sit on chairs at desks and stand at shelves: the acts of the
    /// places fit what is there, and every kind of work shows up.
    #[test]
    fn acts_fit_what_is_at_hand() {
        let mut seen = std::collections::BTreeSet::new();
        for k in 0..6u64 {
            let (_, items, stands) = all(k);
            for st in &stands {
                seen.insert(st.act);
                let p = DVec2::new(st.at.x, st.at.z);
                let near = |kinds: &[ItemKind], r: f64| {
                    items
                        .iter()
                        .any(|i| kinds.contains(&i.kind) && i.at.distance(p) < r)
                };
                match st.act {
                    Act::Type => assert!(
                        near(&[ItemKind::Desk], 1.6),
                        "seed {k}: typing away from a desk"
                    ),
                    Act::SitTalk | Act::SitSip | Act::SitRelax | Act::SitRead | Act::Write => {
                        assert!(
                            near(
                                &[
                                    ItemKind::Chair,
                                    ItemKind::Sofa,
                                    ItemKind::Armchair,
                                    ItemKind::SwivelChair,
                                    ItemKind::BarStool,
                                    ItemKind::Booth
                                ],
                                1.2
                            ),
                            "seed {k}: sitting on nothing at {p:?}"
                        );
                    }
                    _ => {}
                }
            }
        }
        for a in [Act::Type, Act::Browse, Act::Talk] {
            assert!(seen.contains(&a), "no {a:?} in any building: {seen:?}");
        }
    }

    /// Upstairs, people climb a real stair to their floor: the path starts
    /// at a stair's foot below, rises with its flights and steps off to the
    /// doorway, and the last stretch crosses no wall, column or rail.
    #[test]
    fn climbs_follow_stairs() {
        let mut climbs = 0;
        for k in 0..12u64 {
            let (l, _, stands) = all(k);
            let solid = pieces(&l);
            for st in &stands {
                let pts: Vec<DVec3> = st.climb.iter().flatten().copied().collect();
                if pts.is_empty() {
                    continue;
                }
                climbs += 1;
                assert!(st.door.is_some(), "a climb leads to a doorway");
                assert!(pts.len() >= 3, "{pts:?}");
                assert!(pts[0].y < st.at.y - 0.4, "starts below: {pts:?}");
                let (a, b) = (pts[pts.len() - 2], pts[pts.len() - 1]);
                assert!((a.y - st.at.y).abs() < 0.3 && (b.y - st.at.y).abs() < 0.3);
                let (a2, b2) = (DVec2::new(a.x, a.z), DVec2::new(b.x, b.z));
                let n = (a2.distance(b2) / 0.2).ceil().max(1.0) as usize;
                for j in 0..=n {
                    let q = a2.lerp(b2, j as f64 / n as f64);
                    if q.distance(b2) < 0.7 {
                        continue;
                    }
                    for part in &solid {
                        if !matches!(
                            part.part,
                            Part::Wall
                                | Part::Outer
                                | Part::Glass
                                | Part::Column
                                | Part::Shaft
                                | Part::Rail
                        ) {
                            continue;
                        }
                        let (y0, y1) = part.shape.span();
                        let tall = y1 > st.at.y + 0.15 && y0 < st.at.y + 1.9;
                        assert!(
                            !tall || part.shape.distance(q) >= 0.2,
                            "seed {k}: the step-off crosses a {:?} at {q:?}",
                            part.part
                        );
                    }
                }
            }
        }
        assert!(climbs > 0, "nobody climbs a stair in any building");
        eprintln!("climbs: {climbs}");
    }

    /// The walks are straight and clear: a trip's ends are open floor and
    /// the line between them touches nothing, so people never walk through
    /// furniture.
    #[test]
    fn walks_cross_open_floor() {
        let mut trips = 0;
        let mut doors = 0;
        for k in 0..6u64 {
            let (l, items, stands) = all(k);
            let feet: Vec<_> = items.iter().filter_map(|i| i.footprint()).collect();
            let solid = pieces(&l);
            for st in &stands {
                let s = &l.spaces[st.space];
                let via = DVec2::new(st.via.x, st.via.z);
                let check = |a: DVec2, b: DVec2, what: &str| {
                    let n = (a.distance(b) / 0.2).ceil().max(1.0) as usize;
                    for j in 0..=n {
                        let q = a.lerp(b, j as f64 / n as f64);
                        for f in &feet {
                            let (y0, y1) = f.span();
                            let tall = y1 > s.y + 0.15 && y0 < s.y + 1.9;
                            assert!(
                                !tall || f.distance(q) >= 0.3,
                                "seed {k}: a {what} walk crosses furniture at {q:?}"
                            );
                        }
                        for part in &solid {
                            if matches!(part.part, Part::Floor(_) | Part::Roof) {
                                continue;
                            }
                            let (y0, y1) = part.shape.span();
                            let tall = y1 > s.y + 0.15 && y0 < s.y + 1.9;
                            assert!(
                                !tall || part.shape.distance(q) >= 0.3,
                                "seed {k}: a {what} walk crosses a {:?} at {q:?}",
                                part.part
                            );
                        }
                    }
                };
                for t in st.trips.iter().flatten() {
                    trips += 1;
                    check(via, DVec2::new(t.at.x, t.at.z), "trip");
                    assert!(DVec2::new(t.at.x, t.at.z).distance(via) <= TRIP_MAX + 1e-9);
                }
                if let Some(d) = st.door {
                    doors += 1;
                    check(DVec2::new(d.x, d.z), via, "door");
                    assert!(DVec2::new(d.x, d.z).distance(via) <= DOOR_MAX + 1e-9);
                }
            }
        }
        assert!(trips > 100, "only {trips} trips");
        assert!(doors > 100, "only {doors} doorway walks");
    }
}
