//! Rooms off the utility tunnels: pump halls, workers' rooms and smugglers'
//! dens. Each is a [`Void`] behind a short passage in a tunnel's wall, with
//! furniture whose solid shapes come from the same numbers it is drawn from.
//!
//! Planned from the tunnels alone (and the seed), never from each other's
//! drawing, so the same seed digs the same rooms.

use super::deep::Folk;
use super::footprint::Footprint;
use super::underground::{Underground, Void, VoidKind, TUNNEL_HALF};
use super::world::TownWorld;
use glam::{DVec2, DVec3};

/// What a room is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RoomKind {
    /// A big round hall: a pump drum, pillars, a control desk, valve stands.
    PumpHall,
    /// A workers' room: lockers, a table, a stove, tool racks.
    Workers,
    /// A smugglers' den: crates, barrels, a camp bed, a strongbox.
    Den,
}

/// What a piece of furniture is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PropKind {
    Pump,
    Pillar,
    Desk,
    Valve,
    Locker,
    Table,
    Stool,
    Stove,
    Rack,
    Crate,
    Barrel,
    Bed,
    Strongbox,
}

/// One piece of furniture, standing on `floor`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prop {
    pub kind: PropKind,
    pub at: DVec2,
    /// The way its front faces (unit).
    pub dir: DVec2,
    /// Half width (x) and depth (z); for round things `x` is the radius.
    pub half: DVec2,
    pub floor: f64,
    pub height: f64,
    /// Which of a few looks.
    pub look: u32,
}

impl Prop {
    pub fn round(&self) -> bool {
        matches!(
            self.kind,
            PropKind::Pump | PropKind::Pillar | PropKind::Barrel | PropKind::Stool
        )
    }

    pub fn footprint(&self) -> Footprint {
        if self.round() {
            Footprint::Disc {
                at: self.at,
                r: self.half.x,
                y0: self.floor,
                y1: self.floor + self.height,
            }
        } else {
            Footprint::Box {
                at: self.at,
                dir: self.dir,
                half: self.half,
                y0: self.floor,
                y1: self.floor + self.height,
            }
        }
    }
}

/// A room, its way in, and what stands in it.
#[derive(Clone, Debug, PartialEq)]
pub struct Room {
    pub kind: RoomKind,
    /// The room's void and the passage into it (indices in the voids).
    pub void: usize,
    pub passage: usize,
    /// The tunnel it opens off.
    pub tunnel: usize,
    pub props: Vec<Prop>,
    /// Who is in it (workers, smugglers), shown while the camera is near.
    pub folk: Vec<Folk>,
    /// Where its lamp hangs.
    pub lamp: DVec3,
    /// Hung with grease and dust: 0..3.
    pub look: u32,
}

impl Room {
    pub fn footprints(&self) -> Vec<Footprint> {
        self.props.iter().map(Prop::footprint).collect()
    }
}

/// How far into the passage the room starts, and the passage's half width.
const PASS_LEN: f64 = 2.6;
const PASS_HALF: f64 = 1.2;
const PASS_HEIGHT: f64 = 3.0;

fn spec(kind: RoomKind) -> (f64, f64, f64) {
    // Half width along the wall, depth, height.
    match kind {
        RoomKind::PumpHall => (5.0, 10.0, 5.4),
        RoomKind::Workers => (3.6, 6.4, 3.4),
        RoomKind::Den => (4.2, 7.4, 3.2),
    }
}

/// The furniture of a room, in the room's own frame (`x` along the wall,
/// `y` away from the tunnel, the door at `x = 0`).
fn layout(kind: RoomKind, rng: &mut hw_core::Rng) -> Vec<(PropKind, DVec2, DVec2, DVec2, f64)> {
    // (kind, local position, local direction, half size, height)
    let mut out: Vec<(PropKind, DVec2, DVec2, DVec2, f64)> = Vec::new();
    let z = DVec2::new(0.0, 1.0);
    let y0 = PASS_LEN;
    let mut add = |k: PropKind, x: f64, y: f64, d: DVec2, hx: f64, hz: f64, h: f64| {
        out.push((k, DVec2::new(x, y0 + y), d, DVec2::new(hx, hz), h));
    };
    match kind {
        RoomKind::PumpHall => {
            add(PropKind::Pump, 0.0, 6.6, z, 1.4, 1.4, 3.8);
            for (x, y) in [(-3.8, 3.2), (3.8, 3.2), (-3.8, 8.0), (3.8, 8.0)] {
                add(PropKind::Pillar, x, y, z, 0.4, 0.4, 5.4);
            }
            add(
                PropKind::Desk,
                -3.9,
                9.4,
                DVec2::new(0.0, -1.0),
                1.0,
                0.45,
                1.05,
            );
            add(PropKind::Valve, 3.7, 9.4, z, 0.35, 0.35, 1.5);
            add(PropKind::Valve, 3.7, 5.6, z, 0.35, 0.35, 1.5);
        }
        RoomKind::Workers => {
            for k in 0..3 {
                add(
                    PropKind::Locker,
                    -3.1,
                    1.2 + 0.85 * k as f64,
                    DVec2::new(1.0, 0.0),
                    0.3,
                    0.4,
                    1.9,
                );
            }
            add(PropKind::Table, 1.4, 3.4, z, 1.1, 0.6, 0.8);
            add(PropKind::Stool, 0.1, 3.4, z, 0.25, 0.25, 0.45);
            add(PropKind::Stool, 2.7, 3.4, z, 0.25, 0.25, 0.45);
            add(PropKind::Stove, 0.0, 5.7, z, 0.5, 0.4, 1.1);
            add(
                PropKind::Rack,
                3.1,
                5.7,
                DVec2::new(-1.0, 0.0),
                0.35,
                0.9,
                1.6,
            );
        }
        RoomKind::Den => {
            let flip = if rng.chance(0.5) { 1.0 } else { -1.0 };
            add(PropKind::Bed, flip * 3.1, 5.8, z, 0.5, 1.0, 0.5);
            add(PropKind::Table, -flip * 2.6, 5.4, z, 0.9, 0.5, 0.8);
            add(PropKind::Strongbox, -flip * 2.6, 6.5, z, 0.4, 0.3, 0.6);
            let mut x = -flip * 3.4;
            for k in 0..3 {
                let h = 0.8 + 0.8 * rng.unit().round();
                add(PropKind::Crate, x, 1.1 + 0.9 * k as f64, z, 0.4, 0.4, h);
            }
            x = flip * 3.4;
            for k in 0..2 {
                add(PropKind::Barrel, x, 1.2 + 0.9 * k as f64, z, 0.4, 0.4, 1.0);
            }
            add(PropKind::Crate, flip * 1.2, 7.0, z, 0.5, 0.4, 1.3);
            add(PropKind::Crate, flip * 0.3, 7.0, z, 0.4, 0.4, 0.8);
        }
    }
    out
}

/// Plans the rooms off the tunnels `0..tunnels` of `u`, and adds their
/// voids. The voids must already be indexed.
pub(crate) fn plan_rooms(
    w: &TownWorld,
    u: &mut Underground,
    tunnels: usize,
    roofed: &dyn Fn(&Void) -> bool,
) -> Vec<Room> {
    let seed = w.seed.child("sewer rooms");
    let mut rooms: Vec<Room> = Vec::new();
    let mut taken: Vec<(Void, f64)> = Vec::new();
    for t in 0..tunnels {
        let tv = u.voids[t].clone();
        let len = tv.max.x - TUNNEL_HALF;
        if len < 26.0 {
            continue;
        }
        let mut rng = seed
            .cell(tv.origin.x.round() as i64, tv.origin.y.round() as i64)
            .rng();
        let mut x = 11.0 + rng.range(0.0, 10.0);
        while x < len - 11.0 {
            let step = rng.range(34.0, 70.0);
            let kind = match rng.weighted(&[1.0, 1.4, 0.8]) {
                0 => RoomKind::PumpHall,
                1 => RoomKind::Workers,
                _ => RoomKind::Den,
            };
            let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
            let look = rng.range_int(0, 3) as u32;
            let skip = rng.chance(0.25);
            let here = x;
            x += step;
            if skip {
                continue;
            }
            let (hx, depth, height) = spec(kind);
            let wall = tv.world(DVec2::new(here, side * TUNNEL_HALF));
            let along = tv.along * side;
            let floor = tv.floor_at(wall);
            let passage = Void {
                kind: VoidKind::Passage,
                origin: wall,
                along,
                min: DVec2::new(-PASS_HALF, -0.5),
                max: DVec2::new(PASS_HALF, PASS_LEN + 0.3),
                floor: [floor, floor],
                height: PASS_HEIGHT,
                open: false,
            };
            let room = Void {
                kind: VoidKind::Service,
                origin: wall,
                along,
                min: DVec2::new(-hx, PASS_LEN),
                max: DVec2::new(hx, PASS_LEN + depth),
                floor: [floor, floor],
                height,
                open: false,
            };
            if !roofed(&passage) || !roofed(&room) {
                continue;
            }
            // Clear of every other void and every other room.
            let clear = |v: &Void, own: usize| {
                let size = v.max - v.min;
                let (nx, ny) = (
                    (size.x / 2.0).ceil() as usize,
                    (size.y / 2.0).ceil() as usize,
                );
                let mid = v.world((v.min + v.max) * 0.5);
                let reach = size.length() * 0.5 + 4.0;
                let near = u.near(mid, reach);
                (0..=nx).all(|i| {
                    (0..=ny).all(|j| {
                        let p = v.world(
                            v.min
                                + DVec2::new(
                                    size.x * i as f64 / nx as f64,
                                    size.y * j as f64 / ny as f64,
                                ),
                        );
                        near.iter()
                            .filter(|&&k| k != own)
                            .all(|&k| !u.voids[k].covers(p, 1.2))
                            && taken.iter().all(|(o, _)| !o.covers(p, 1.2))
                    })
                })
            };
            // The passage touches its own tunnel (that is its point): test
            // only the room, and the passage against the others.
            if !clear(&room, t) {
                continue;
            }
            let pass_ok = {
                let mid = passage.world((passage.min + passage.max) * 0.5);
                u.near(mid, 6.0).into_iter().filter(|&k| k != t).all(|k| {
                    !u.voids[k].covers(mid, 2.0)
                        && !u.voids[k].covers(passage.world(DVec2::new(0.0, PASS_LEN)), 1.2)
                })
            };
            if !pass_ok {
                continue;
            }
            let mut props = Vec::new();
            for (pk, lp, ld, half, h) in layout(kind, &mut rng) {
                let at = passage.world(lp);
                let dir = (passage.along * ld.x + passage.inward() * ld.y).normalize_or_zero();
                props.push(Prop {
                    kind: pk,
                    at,
                    dir,
                    half,
                    floor,
                    height: h,
                    look: rng.range_int(0, 3) as u32,
                });
            }
            // People: standing at their work, one of them walking a round.
            let at = |x: f64, y: f64| {
                let q = passage.world(DVec2::new(x, PASS_LEN + y));
                DVec3::new(q.x, floor, q.y)
            };
            let toward = passage.along.perp();
            let face = |a: f64| DVec2::from_angle(a).rotate(toward);
            let folk = match kind {
                RoomKind::PumpHall => vec![
                    Folk {
                        at: at(-3.0, 8.4),
                        facing: -toward,
                        to: None,
                        mole: false,
                    },
                    Folk {
                        at: at(2.6, 2.0),
                        facing: toward,
                        to: Some(passage.world(DVec2::new(-2.4, PASS_LEN + 4.0))),
                        mole: false,
                    },
                ],
                RoomKind::Workers => vec![
                    Folk {
                        at: at(1.4, 2.5),
                        facing: face(0.4),
                        to: None,
                        mole: false,
                    },
                    Folk {
                        at: at(-1.0, 4.6),
                        facing: face(-0.6),
                        to: None,
                        mole: false,
                    },
                ],
                RoomKind::Den => vec![
                    Folk {
                        at: at(1.0, 3.4),
                        facing: face(2.8),
                        to: None,
                        mole: rng.chance(0.3),
                    },
                    Folk {
                        at: at(0.0, 1.6),
                        facing: -toward,
                        to: Some(passage.world(DVec2::new(1.5, PASS_LEN + 5.5))),
                        mole: false,
                    },
                ],
            };
            let lamp = room.world(DVec2::new(0.0, PASS_LEN + depth * 0.5));
            let lamp = DVec3::new(lamp.x, floor + height - 0.35, lamp.y);
            let vi = u.voids.len();
            u.voids.push(passage.clone());
            u.voids.push(room.clone());
            taken.push((room, floor));
            taken.push((passage, floor));
            rooms.push(Room {
                kind,
                void: vi + 1,
                passage: vi,
                tunnel: t,
                props,
                folk,
                lamp,
                look,
            });
        }
    }
    rooms
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::urban::TownParams;
    use hw_core::Seed;

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn rooms_off_the_tunnels_are_reachable_and_clear_of_their_doors() {
        for seed in [1u64, 3, 11] {
            let w = TownWorld::generate(Seed(seed), TownParams::sample(Seed(seed), 40_000.0));
            let u = w.underground();
            assert!(u.rooms.len() >= 6, "seed {seed}: {} rooms", u.rooms.len());
            let kinds: std::collections::BTreeSet<_> = u.rooms.iter().map(|r| r.kind).collect();
            assert!(kinds.len() >= 2, "seed {seed}: kinds: {kinds:?}");
            println!("seed {seed}: {} rooms, {kinds:?}", u.rooms.len());
            for r in &u.rooms {
                let room = &u.voids[r.void];
                let pass = &u.voids[r.passage];
                // The tunnel, the passage and the room are one walk.
                let tunnel = &u.voids[r.tunnel];
                let door = pass.world(DVec2::new(0.0, 0.5));
                assert!(
                    (pass.floor_at(door) - tunnel.floor_at(door)).abs() < 0.2,
                    "room floor level with its tunnel"
                );
                // Nothing stands in the way from the door to the room's middle.
                for f in r.footprints() {
                    let c = f.at();
                    let l = pass.local(c);
                    assert!(
                        l.x.abs() - f.reach() > 1.4 || l.y - f.reach() > PASS_LEN + 1.5,
                        "{:?} blocks the door at {c:?}",
                        r.kind
                    );
                    assert!(room.contains(DVec3::new(c.x, room.floor[0] + 0.5, c.y)));
                }
                assert!(u.floor_for(door, room.floor[0] + 0.5).is_some());
            }
        }
    }
}
