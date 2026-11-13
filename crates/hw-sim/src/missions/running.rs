//! A case being worked: the step it is at, and that step's gang, clues
//! and hostages, in a building's rooms or out at an open place.

use super::plan::{Plan, Step};
use super::sites::Site;
use super::*;
use crate::combat::{room_to_stand, Fighter, Spot};
use crate::{DT, TICK_HZ};
use glam::DVec2;
use hw_gen::layout::SpaceKind;
use hw_net::CaseStep;

/// A room (or a patch of open ground) the gang holds: its fight spot.
#[derive(Clone, Debug)]
struct Room {
    spot: usize,
    at: DVec3,
    /// When its crew first had a hero to fight (a tick).
    noticed: Option<u64>,
    /// It has raised the alarm (or tried to).
    called: bool,
}

/// A clue to find, or a hostage to free.
#[derive(Clone, Copy, Debug)]
struct Clue {
    at: DVec3,
    found: bool,
    /// A hostage: the room whose crew guards them (in `rooms`), and their
    /// bystander number.
    hostage: Option<(usize, u32)>,
    /// The room whose crew guards a thing to find (in `rooms`): it cannot
    /// be taken while they stand.
    guard: Option<usize>,
    /// The safe it is locked in.
    safe: Option<Safe>,
}

/// A gang's safe with the thing in it: a villain of its own kind
/// ([`EnemyKind::Safe`]) that stands there and is broken open.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Safe {
    pub id: NetId,
    spot: usize,
    /// Its middle on the floor, and the way its door faces.
    pub at: DVec3,
    pub facing: DVec2,
    /// Its door is broken open.
    open: bool,
}

impl Safe {
    pub(crate) fn solid(&self) -> crate::collision::Solid {
        safe_solid(self.at, self.facing)
    }

    /// Takes it out of the world.
    pub(crate) fn remove(&self, world: &mut World) {
        let found = world
            .query::<(Entity, &Net)>()
            .iter(world)
            .find(|(_, n)| n.0 == self.id)
            .map(|(e, _)| e);
        if let Some(e) = found {
            world.despawn(e);
        }
        if let Some(s) = world.resource_mut::<Fights>().spots.get_mut(self.spot) {
            s.members = 0;
            s.respawn_at = None;
        }
    }
}

/// The solid box of a safe whose middle stands at `at` on the floor, its
/// door facing `facing`: what [`crate::missions::SAFE_HALF`] and
/// [`crate::missions::SAFE_HEIGHT`] say, which is what is drawn.
pub fn safe_solid(at: DVec3, facing: DVec2) -> crate::collision::Solid {
    let f = facing.normalize_or(DVec2::Y);
    crate::collision::Solid {
        origin: DVec2::new(at.x, at.z),
        along: f.perp(),
        inward: f,
        min: -SAFE_HALF,
        max: SAFE_HALF,
        y0: at.y,
        y1: at.y + SAFE_HEIGHT,
    }
}

/// Where a safe's body (what heroes aim at) is: just in front of its
/// door, so a hero facing the door sees it past the safe's own box.
pub fn safe_front(at: DVec3, facing: DVec2) -> DVec3 {
    let f = facing.normalize_or(DVec2::Y) * (SAFE_HALF.y + SAFE_FRONT);
    at + DVec3::new(f.x, 0.0, f.y)
}

/// The middle of the safe whose body is at `front` (see [`safe_front`]).
pub fn safe_middle(front: DVec3, facing: DVec2) -> DVec3 {
    let f = facing.normalize_or(DVec2::Y) * (SAFE_HALF.y + SAFE_FRONT);
    front - DVec3::new(f.x, 0.0, f.y)
}

/// What a second's look at a case found.
pub(crate) enum Settled {
    Working,
    /// A lead step done at this place: on to the next.
    Lead(DVec3),
    /// The last step done: the case is closed.
    Done,
}

/// A case being worked.
#[derive(Clone, Debug)]
pub struct Running {
    /// Its number in the team's cases (1, 2, ...): its name everywhere.
    pub number: u16,
    pub gang: usize,
    plan: Plan,
    /// The step it is at.
    pub step: usize,
    /// [`GO`] or [`INSIDE`] (at it: the gang stands).
    pub stage: u8,
    floors: Arc<Floors>,
    rooms: Vec<Room>,
    boss: Option<Room>,
    /// The lookouts at the way in (a hideout).
    door: Option<Room>,
    /// The crew that cuts off the way out once the thing is taken.
    cutoff: Option<Room>,
    /// The way in, where the lookouts stand and the way out ends.
    way_in: Option<DVec3>,
    /// Who took the thing: they must get out with it.
    carrier: Option<NetId>,
    /// Alarms raised so far (a room called the next, the thing was taken,
    /// the boss called for help).
    alarms: u8,
    /// The boss broke off and ran until this tick (once).
    boss_ran: Option<u64>,
    /// The tick of the last look at it.
    now: u64,
    clues: Vec<Clue>,
    /// Seconds of holding the help key at a clue or hostage.
    held: f64,
    /// Since when no hero has been near the place (a tick).
    away_since: Option<u64>,
    /// A hero is down near an underground place (the radio stops leading
    /// to the way down).
    below: bool,
}

/// Is `a` near `b`: on the flat, and under the ground also in a straight
/// line (a hero on the street over a tunnel is not near it).
pub(crate) fn near(a: DVec3, b: DVec3, r: f64) -> bool {
    flat(a, b) < r && (a.y - b.y).abs() < r.min(25.0)
}

impl Running {
    pub(crate) fn new(number: u16, plan: Plan, step: usize, floors: Arc<Floors>) -> Self {
        Running {
            number,
            gang: plan.gang,
            plan,
            step,
            stage: GO,
            floors,
            rooms: Vec::new(),
            boss: None,
            door: None,
            cutoff: None,
            way_in: None,
            carrier: None,
            alarms: 0,
            boss_ran: None,
            now: 0,
            clues: Vec::new(),
            held: 0.0,
            away_since: None,
            below: false,
        }
    }

    pub fn steps(&self) -> &[Step] {
        &self.plan.steps
    }

    /// What closing it pays: a share of the next level's karma, by the
    /// hideout's size and its gang's level (other cases: the old share).
    pub fn share(&self) -> f64 {
        if self.is_hideout() {
            share_of(self.plan.size, self.plan.extra)
                * if self.plan.hot {
                    1.0 + board::HOT_BONUS
                } else {
                    1.0
                }
        } else {
            REWARD_SHARE
        }
    }

    pub fn variant(&self) -> u8 {
        self.plan.variant
    }

    /// The step it is at.
    pub fn current(&self) -> &Step {
        &self.plan.steps[self.step.min(self.plan.steps.len() - 1)]
    }

    pub fn site(&self) -> &Site {
        &self.current().site
    }

    pub fn job(&self) -> CaseKind {
        self.current().job
    }

    /// Where the clues (or hostages) are and whether each is found (freed).
    pub fn clues(&self) -> Vec<(DVec3, bool)> {
        self.clues.iter().map(|k| (k.at, k.found)).collect()
    }

    /// The fight spot of the crew guarding each thing to find (None: it
    /// lies unguarded).
    pub fn guards(&self) -> Vec<Option<usize>> {
        self.clues
            .iter()
            .map(|k| k.guard.and_then(|g| self.rooms.get(g)).map(|r| r.spot))
            .collect()
    }

    /// Alarms raised so far.
    pub fn alarms(&self) -> u8 {
        self.alarms
    }

    /// The fight spots of the lookouts at the way in, of the rooms, of the
    /// crew cutting off the way out and of the boss's guard.
    pub fn spots(&self) -> (Option<usize>, Vec<usize>, Option<usize>, Option<usize>) {
        (
            self.door.as_ref().map(|r| r.spot),
            self.rooms.iter().map(|r| r.spot).collect(),
            self.cutoff.as_ref().map(|r| r.spot),
            self.boss.as_ref().map(|r| r.spot),
        )
    }

    /// Has the thing been taken (it must be got out now)?
    pub fn carried(&self) -> bool {
        self.carrier.is_some()
    }

    /// The hostages' bystander numbers.
    pub fn hostages(&self) -> Vec<u32> {
        self.clues
            .iter()
            .filter_map(|k| k.hostage.map(|h| h.1))
            .collect()
    }

    /// Where to go: the place (under the ground: its way down until a hero
    /// is down there), or at it the next clue or hostage, the boss, a
    /// crew still standing.
    pub fn objective(&self) -> DVec3 {
        let site = self.site();
        let way_in = match site.way_down {
            Some(top) if !self.below => top,
            _ => site.at,
        };
        if self.stage != INSIDE {
            return way_in;
        }
        match self.job() {
            // Out with it: back to the way in.
            CaseKind::Evidence if self.carrier.is_some() => self.way_in.unwrap_or(way_in),
            CaseKind::Evidence | CaseKind::Hostages => self
                .clues
                .iter()
                .find(|k| !k.found)
                .map_or(way_in, |k| k.at),
            CaseKind::Takeover => self.boss.as_ref().map_or(way_in, |b| b.at),
            CaseKind::Crew => self.rooms.first().map_or(way_in, |r| r.at),
        }
    }

    /// Is a hero near enough for the gang to stand?
    fn woken(&self, heroes: &[Seen]) -> bool {
        let site = self.site();
        if site.place.underground() {
            heroes
                .iter()
                .any(|h| h.at.distance(site.at) < UNDER_WAKE && h.at.y < site.at.y + 6.0)
        } else {
            heroes.iter().any(|h| flat(h.at, site.at) < WAKE)
        }
    }

    /// Once a second: the gang stands when a hero comes near, leaves when
    /// they are long gone; a step done moves the case on.
    pub(crate) fn settle(&mut self, world: &mut World, tick: u64, heroes: &[Seen]) -> Settled {
        self.now = tick;
        let at = self.site().at;
        // Down there: below the street, not over it.
        self.below = self.site().place.underground()
            && heroes
                .iter()
                .any(|h| flat(h.at, at) < AWAY * 0.5 && h.at.y < at.y + 6.0);
        match self.stage {
            GO => {
                if self.woken(heroes) {
                    self.take(world, tick, heroes.len());
                    self.stage = INSIDE;
                    self.away_since = None;
                }
                Settled::Working
            }
            INSIDE => {
                if heroes.iter().all(|h| flat(h.at, at) > AWAY) {
                    let since = *self.away_since.get_or_insert(tick);
                    if tick >= since + (AWAY_SECONDS * TICK_HZ as f64) as u64 {
                        // Gone too long: the gang slips away, and stands
                        // there fresh when a hero comes back.
                        self.leave(world);
                        self.stage = GO;
                    }
                    return Settled::Working;
                }
                self.away_since = None;
                self.raise_alarms(world, tick, heroes);
                self.boss_breaks_off(world, tick, heroes);
                if !self.step_done(world, heroes) {
                    return Settled::Working;
                }
                if self.step + 1 >= self.plan.steps.len() {
                    return Settled::Done;
                }
                // A lead: what it turned up points to the next place.
                self.leave(world);
                self.step += 1;
                self.stage = GO;
                Settled::Lead(at)
            }
            _ => Settled::Working,
        }
    }

    fn step_done(&self, world: &mut World, heroes: &[Seen]) -> bool {
        let all_beaten = !self.rooms.is_empty() && self.rooms.iter().all(|r| beaten(world, r.spot));
        match self.job() {
            CaseKind::Takeover => match &self.boss {
                Some(b) => beaten(world, b.spot),
                None => all_beaten,
            },
            // Down and cuffed is enough: they talk while the drone comes.
            CaseKind::Crew => {
                !self.rooms.is_empty() && self.rooms.iter().all(|r| subdued(world, r.spot))
            }
            CaseKind::Evidence | CaseKind::Hostages => {
                if self.clues.is_empty() {
                    all_beaten
                } else {
                    self.clues.iter().all(|k| k.found) && self.got_out(world, heroes)
                }
            }
        }
    }

    /// What the clients are told of it.
    pub(crate) fn news(&self, world: &mut World) -> ToClient {
        let (cleared, groups) = self.cleared(world);
        let boss = match &self.boss {
            None => 0,
            Some(b) if beaten(world, b.spot) => 2,
            Some(_) if self.boss_ran.is_some_and(|t| self.now < t) => 3,
            Some(_) => 1,
        };
        let held = ((self.held / self.hold_seconds()).clamp(0.0, 1.0) * 255.0) as u8 & !15;
        ToClient::Case {
            number: self.number,
            stage: self.stage,
            gang: self.gang as u8,
            step: self.step.min(255) as u8,
            steps: self
                .plan
                .steps
                .iter()
                .map(|s| CaseStep {
                    place: s.site.place.code(),
                    job: s.job.code(),
                    at: s.site.at.as_vec3().to_array(),
                    name: s.site.name.clone(),
                })
                .collect(),
            at: self.objective().as_vec3().to_array(),
            cleared,
            groups,
            boss,
            clues: (0..self.clues.len())
                .map(|k| (self.clues[k].at.as_vec3().to_array(), self.state(world, k)))
                .collect(),
            held,
            size: self.plan.size,
            alarms: self.alarms,
            crime: self.plan.crime.code(),
            level: world
                .resource::<Fights>()
                .team_level
                .saturating_add(self.plan.extra),
        }
    }

    /// The gang stands at the step's place.
    fn take(&mut self, world: &mut World, tick: u64, heroes: usize) {
        match self.site().building {
            Some(k) => self.take_rooms(world, tick, heroes, k),
            None => self.take_open(world, tick, heroes),
        }
        self.held = 0.0;
    }

    /// Puts a group at `at` facing `facing`; its room.
    #[allow(clippy::too_many_arguments)]
    fn group(
        &self,
        world: &mut World,
        tick: u64,
        heroes: usize,
        at: DVec3,
        facing: DVec2,
        size: u32,
        lead: Option<EnemyKind>,
        boss: bool,
    ) -> Room {
        let gang = GANGS[self.gang];
        let spot = Spot {
            extra: self.plan.extra + if boss { BOSS_EXTRA } else { 0 },
            size: Some(size),
            crew: gang.crew,
            once: true,
            lead,
            ..Spot::fight(at, facing)
        };
        let index = {
            let mut f = world.resource_mut::<Fights>();
            f.spots.push(spot);
            f.spots.len() - 1
        };
        combat::spawn_group(world, tick, index, heroes);
        Room {
            spot: index,
            at,
            noticed: None,
            called: false,
        }
    }

    /// A safe stands at `at` in building `b`, square to its walls, its
    /// door towards `towards` if a hero can stand in front of it there.
    fn lock_up(
        &self,
        world: &mut World,
        tick: u64,
        heroes: usize,
        b: &crate::floornav::Building,
        at: DVec3,
        towards: DVec3,
    ) -> Safe {
        let solids = world.resource::<crate::Solids>().0.clone();
        let want = DVec2::new(towards.x - at.x, towards.z - at.z);
        let d = b.dir.normalize_or(DVec2::X);
        let mut ways = [d, -d, d.perp(), -d.perp()];
        ways.sort_by(|a, b| b.dot(want).total_cmp(&a.dot(want)));
        let c = DVec2::new(at.x, at.z);
        let facing = ways
            .into_iter()
            .find(|w| room_to_stand(&solids, c + *w * (SAFE_HALF.y + 0.9), at.y))
            .unwrap_or(ways[0]);
        let front = safe_front(at, facing);
        let level = world
            .resource::<Fights>()
            .team_level
            .saturating_add(self.plan.extra);
        let spot = {
            let mut f = world.resource_mut::<Fights>();
            f.spots.push(Spot {
                once: true,
                groups: 1,
                members: 1,
                ..Spot::fight(front, facing)
            });
            f.spots.len() - 1
        };
        let id = combat::spawn_enemy(world, tick, EnemyKind::Safe, level, spot, front, heroes);
        let found = world
            .query::<(Entity, &Net)>()
            .iter(world)
            .find(|(_, n)| n.0 == id)
            .map(|(e, _)| e);
        if let Some(e) = found {
            let yaw = facing.x.atan2(facing.y);
            world
                .entity_mut(e)
                .insert(crate::Orientation(crate::robots::yaw_rotation(yaw)));
        }
        Safe {
            id,
            spot,
            at,
            facing,
            open: false,
        }
    }

    /// The safes of this case (they stay solid, open or not).
    pub(crate) fn safes(&self) -> impl Iterator<Item = &Safe> {
        self.clues.iter().filter_map(|k| k.safe.as_ref())
    }

    /// A safe whose door was broken this tick stands open, and the noise
    /// brings the nearest crew that has heard nothing yet.
    fn open_safes(&mut self, world: &mut World, tick: u64, heroes: &[Seen]) {
        if self.clues.iter().all(|k| k.safe.is_none_or(|s| s.open)) {
            return;
        }
        let shut: Vec<NetId> = world
            .query::<(&Net, &combat::Enemy, &Fighter)>()
            .iter(world)
            .filter(|(_, en, f)| en.kind == EnemyKind::Safe && f.fighting())
            .map(|(n, _, _)| n.0)
            .collect();
        let mut broken = Vec::new();
        for s in self.clues.iter_mut().filter_map(|k| k.safe.as_mut()) {
            if !s.open && !shut.contains(&s.id) {
                s.open = true;
                broken.push(s.at);
            }
        }
        for at in broken {
            let nearest = heroes.iter().min_by(|a, b| {
                a.at.distance(at)
                    .total_cmp(&b.at.distance(at))
                    .then(a.id.cmp(&b.id))
            });
            if let Some(h) = nearest {
                self.call_nearest(world, tick, heroes, at, h.id);
            }
        }
    }

    /// A hostage held at `at`, facing their guards at `guard`.
    fn hostage(&self, world: &mut World, tick: u64, at: DVec3, guard: DVec3) -> u32 {
        let id = HOSTAGE_IDS | ((self.number as u32 & 0xffff) << 4) | self.clues.len() as u32;
        let face = DVec2::new(guard.x - at.x, guard.z - at.z).normalize_or(DVec2::X);
        crate::incidents::spawn_bystander(world, tick, id, at, face, None);
        id
    }

    /// Out in the open (a street, a plaza, a tunnel, a hall of the Deeps):
    /// the crew in the middle, side groups on open floor round it, clues
    /// and hostages further out.
    fn take_open(&mut self, world: &mut World, tick: u64, heroes: usize) {
        let site = self.site().clone();
        let job = self.job();
        let gang = GANGS[self.gang];
        let solids = world.resource::<crate::Solids>().0.clone();
        let extra = heroes.saturating_sub(1) as u32;
        let mut spots: Vec<DVec3> = std::iter::once(site.at)
            .chain(site.around.iter().copied())
            .filter_map(|p| open_near(&solids, p))
            .collect();
        spots.dedup_by(|a, b| a.distance(*b) < 3.0);
        let Some(&middle) = spots.first() else {
            return;
        };
        let rest: Vec<DVec3> = spots[1..].to_vec();
        let side_groups = match job {
            CaseKind::Takeover => 2,
            CaseKind::Crew | CaseKind::Hostages => 1,
            CaseKind::Evidence => 0,
        };
        let main = match job {
            CaseKind::Takeover => {
                self.boss = Some(self.group(
                    world,
                    tick,
                    heroes,
                    middle,
                    site.facing,
                    BOSS_SIZE + extra,
                    Some(gang.boss),
                    true,
                ));
                None
            }
            _ => Some(self.group(
                world,
                tick,
                heroes,
                middle,
                site.facing,
                OPEN_SIZE + extra,
                None,
                false,
            )),
        };
        self.rooms = main.into_iter().collect();
        for &p in rest.iter().take(side_groups) {
            let r = self.group(
                world,
                tick,
                heroes,
                p,
                site.facing,
                OPEN_SIZE + extra,
                None,
                false,
            );
            self.rooms.push(r);
        }
        self.clues.clear();
        let want = match job {
            CaseKind::Evidence => CLUES,
            CaseKind::Hostages => HOSTAGES,
            _ => 0,
        };
        // Clues and hostages on the spots left, the furthest out first.
        let mut left: Vec<DVec3> = rest.iter().skip(side_groups).copied().collect();
        left.sort_by(|a, b| flat(*b, middle).total_cmp(&flat(*a, middle)));
        for at in left.into_iter().take(want) {
            let hostage = (job == CaseKind::Hostages).then(|| {
                let room = (0..self.rooms.len())
                    .min_by(|&a, &b| {
                        flat(self.rooms[a].at, at).total_cmp(&flat(self.rooms[b].at, at))
                    })
                    .unwrap_or(0);
                let guard = self.rooms.get(room).map_or(middle, |r| r.at);
                (room, self.hostage(world, tick, at, guard))
            });
            self.clues.push(Clue {
                at,
                found: false,
                hostage,
                guard: None,
                safe: None,
            });
        }
    }

    /// In building `k`: a group in each of [`ROOMS`] rooms over the
    /// floors, the boss and its guard in the biggest room on top, and the
    /// clues or hostages.
    fn take_rooms(&mut self, world: &mut World, tick: u64, heroes: usize, k: usize) {
        let floors = self.floors.clone();
        let job = self.job();
        let b = &floors.buildings[k];
        let gang = GANGS[self.gang];
        let solids = world.resource::<crate::Solids>().0.clone();
        let mut rooms = gang_rooms(b);
        // Lowest first, then the biggest.
        rooms.sort_by(|a, b| {
            a.2.total_cmp(&b.2)
                .then(b.1.area().total_cmp(&a.1.area()))
                .then(a.1.min.x.total_cmp(&b.1.min.x))
                .then(a.1.min.y.total_cmp(&b.1.min.y))
        });
        let Some(top) = rooms
            .iter()
            .filter(|r| (r.2 - rooms.last().map_or(0.0, |t| t.2)).abs() < 1e-3)
            .max_by(|a, b| {
                let boardroom = |r: &(SpaceKind, _, _)| r.0 == SpaceKind::Boardroom;
                boardroom(a)
                    .cmp(&boardroom(b))
                    .then(a.1.area().total_cmp(&b.1.area()))
            })
            .copied()
        else {
            return;
        };
        let rest: Vec<_> = rooms.iter().filter(|r| **r != top).copied().collect();
        // Spread over the floors: one room per floor, round and round.
        let mut levels: Vec<f64> = rest.iter().map(|r| r.2).collect();
        levels.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
        let mut chosen = Vec::new();
        let mut round = 0;
        let want_rooms = rooms_of(self.plan.size);
        // Every room in that order: those with no floor to stand on are
        // passed over below.
        while chosen.len() < rest.len() {
            for &y in &levels {
                let on: Vec<_> = rest.iter().filter(|r| (r.2 - y).abs() < 1e-3).collect();
                if let Some(r) = on.get(round) {
                    chosen.push(**r);
                }
            }
            round += 1;
            if round > 32 {
                break;
            }
        }
        let extra = heroes.saturating_sub(1) as u32;
        // Facing the way in: heroes come from the street.
        let door = b.to_town(b.graph.front());
        // Lookouts at the way in.
        self.way_in = open_near(&solids, door).or(Some(door));
        self.door = open_near(&solids, door).map(|at| {
            let out = DVec2::new(at.x - b.pos.x, at.z - b.pos.y).normalize_or(DVec2::X);
            self.group(world, tick, heroes, at, out, DOOR_SIZE, None, false)
        });
        let mut placed: Vec<(Room, (SpaceKind, hw_gen::geom::Rect, f64))> = Vec::new();
        for r in &chosen {
            if placed.len() >= want_rooms {
                break;
            }
            if let Some(at) = open_middle(&solids, b, &r.1, r.2) {
                let facing = DVec2::new(door.x - at.x, door.z - at.z).normalize_or(DVec2::X);
                let room = self.group(
                    world,
                    tick,
                    heroes,
                    at,
                    facing,
                    ROOM_SIZE + extra,
                    None,
                    false,
                );
                placed.push((room, *r));
            }
        }
        let chosen: Vec<_> = placed.iter().map(|p| p.1).collect();
        self.rooms = placed.into_iter().map(|p| p.0).collect();
        self.boss = open_middle(&solids, b, &top.1, top.2).map(|at| {
            let facing = DVec2::new(door.x - at.x, door.z - at.z).normalize_or(DVec2::X);
            self.group(
                world,
                tick,
                heroes,
                at,
                facing,
                BOSS_SIZE + extra,
                Some(gang.boss),
                true,
            )
        });
        self.clues.clear();
        let want = match job {
            CaseKind::Hostages => HOSTAGES,
            CaseKind::Evidence => CLUES,
            _ => return,
        };
        // In rooms on different floors (high ones first), tucked away from
        // where each room's group stands; a second round fills up when
        // there are fewer floors than clues.
        let mut order: Vec<usize> = (0..self.rooms.len()).collect();
        order.sort_by(|&a, &b| chosen[b].2.total_cmp(&chosen[a].2).then(a.cmp(&b)));
        let mut used: Vec<usize> = Vec::new();
        for second in [false, true] {
            for &i in &order {
                if self.clues.len() >= want || used.contains(&i) {
                    continue;
                }
                let y = chosen[i].2;
                let floor_taken = used.iter().any(|&u| (chosen[u].2 - y).abs() < 1e-3);
                if floor_taken && !second {
                    continue;
                }
                if let Some(at) = far_corner(&solids, b, &chosen[i].1, y, self.rooms[i].at) {
                    let hostage = (job == CaseKind::Hostages)
                        .then(|| (i, self.hostage(world, tick, at, self.rooms[i].at)));
                    // The thing is locked in a safe, its door towards
                    // the room's crew.
                    let safe = (job == CaseKind::Evidence)
                        .then(|| self.lock_up(world, tick, heroes, b, at, self.rooms[i].at));
                    self.clues.push(Clue {
                        at,
                        found: false,
                        hostage,
                        // The room's own crew stands over the thing.
                        guard: (job == CaseKind::Evidence).then_some(i),
                        safe,
                    });
                    used.push(i);
                }
            }
        }
        // Whatever the rooms hold, a thing to find must be somewhere: the
        // boss's room, or any room with open floor.
        if self.clues.is_empty() && job == CaseKind::Evidence {
            let from = self.boss.as_ref().map_or(door, |b| b.at);
            let at = std::iter::once(&top)
                .chain(rooms.iter())
                .find_map(|r| far_corner(&solids, b, &r.1, r.2, from));
            if let Some(at) = at {
                let safe = Some(self.lock_up(world, tick, heroes, b, at, from));
                self.clues.push(Clue {
                    at,
                    found: false,
                    hostage: None,
                    guard: None,
                    safe,
                });
            }
        }
    }

    /// The gang leaves (its spots stay, empty, never refilled), and
    /// hostages not yet freed with it.
    pub(crate) fn leave(&mut self, world: &mut World) {
        let spots: Vec<usize> = self
            .rooms
            .iter()
            .chain(self.boss.iter())
            .chain(self.door.iter())
            .chain(self.cutoff.iter())
            .map(|r| r.spot)
            .collect();
        let mut gone: Vec<(NetId, Entity)> = world
            .query::<(Entity, &Net, &combat::Enemy)>()
            .iter(world)
            .filter(|(_, _, en)| spots.contains(&en.spot))
            .map(|(e, n, _)| (n.0, e))
            .collect();
        let held: Vec<u32> = self
            .clues
            .iter()
            .filter(|k| !k.found)
            .filter_map(|k| k.hostage.map(|h| h.1))
            .collect();
        gone.extend(
            world
                .query::<(Entity, &Net, &crate::incidents::Bystander)>()
                .iter(world)
                .filter(|(_, _, b)| held.contains(&b.incident))
                .map(|(e, n, _)| (n.0, e)),
        );
        gone.sort_by_key(|g| g.0);
        for (id, e) in gone {
            world.resource_mut::<Fights>().paths.remove(&id);
            world.despawn(e);
        }
        {
            let mut f = world.resource_mut::<Fights>();
            for s in spots {
                f.spots[s].members = 0;
                f.spots[s].respawn_at = None;
            }
        }
        for s in self.safes().copied().collect::<Vec<_>>() {
            s.remove(world);
        }
        self.rooms.clear();
        self.boss = None;
        self.door = None;
        self.cutoff = None;
        self.way_in = None;
        self.carrier = None;
        self.boss_ran = None;
        self.clues.clear();
        self.held = 0.0;
    }

    /// A hideout case closed: is this one (a crime from the board)?
    pub fn is_hideout(&self) -> bool {
        self.plan.variant == 0 && self.plan.steps.len() == 1 && self.site().building.is_some()
    }

    /// The ending of a hideout case: whoever of the gang still stands gives
    /// up (down and cuffed: the police drones take them in, with no karma
    /// for anyone), and someone waits outside to thank the heroes (the
    /// thing's owner, a neighbour). The case lets go of its groups.
    /// Its safes stay where they stand: they are handed back, to be taken
    /// away once nobody is near.
    /// Also where the police are called to: the door, and the way it
    /// looks out.
    pub(crate) fn wrap_up(
        &mut self,
        world: &mut World,
        tick: u64,
    ) -> (Vec<Safe>, Option<(DVec3, DVec2)>) {
        let mut police = None;
        let spots: Vec<usize> = self.groups_mut().map(|r| r.spot).collect();
        for (en, mut f) in world
            .query::<(&combat::Enemy, &mut Fighter)>()
            .iter_mut(world)
        {
            if spots.contains(&en.spot) && f.fighting() {
                f.health = 0.0;
                f.status = hw_net::Status::Down;
                f.until = tick;
                f.target = None;
            }
        }
        if let (Some(at), Some(k)) = (self.way_in, self.site().building) {
            let b = &self.floors.buildings[k];
            let out = DVec2::new(at.x - b.pos.x, at.z - b.pos.y).normalize_or(DVec2::X);
            // The front door looks square out of the house front.
            police = Some((at, if out.dot(b.dir) > 0.5 { b.dir } else { out }));
            let solids = world.resource::<crate::Solids>().0.clone();
            let stand = (2..7).find_map(|m| {
                let p = at + DVec3::new(out.x, 0.0, out.y) * (m as f64 * 1.5);
                (!b.holds(p, 0.5)).then(|| open_near(&solids, p)).flatten()
            });
            if let Some(p) = stand {
                let id = HOSTAGE_IDS | ((self.number as u32 & 0xffff) << 4) | OWNER;
                crate::incidents::spawn_bystander(world, tick, id, p, -out, None);
                crate::incidents::release_bystanders(world, tick, id);
            }
        }
        let safes = self.safes().copied().collect();
        self.rooms.clear();
        self.boss = None;
        self.door = None;
        self.cutoff = None;
        self.clues.clear();
        (safes, police)
    }

    /// Heroes holding the help key at a clue, or by a hostage whose
    /// guards are beaten.
    pub(crate) fn search(&mut self, world: &mut World, tick: u64, heroes: &[Seen]) {
        if self.stage != INSIDE {
            return;
        }
        self.open_safes(world, tick, heroes);
        let states: Vec<u8> = (0..self.clues.len())
            .map(|k| self.state(world, k))
            .collect();
        let by = |k: usize| {
            heroes
                .iter()
                .find(|h| h.help && h.at.distance(self.clues[k].at) < CLUE_REACH)
                .map(|h| h.id)
        };
        let Some((k, hero)) = (0..self.clues.len())
            .filter(|&k| states[k] == READY)
            .find_map(|k| by(k).map(|h| (k, h)))
        else {
            self.held = (self.held - DT).max(0.0);
            return;
        };
        self.held += DT;
        if self.held + 1e-9 >= self.hold_seconds() {
            self.clues[k].found = true;
            self.held = 0.0;
            if let Some((_, id)) = self.clues[k].hostage {
                self.set_free(world, tick, id);
            }
            if self.job() == CaseKind::Evidence && self.site().building.is_some() {
                self.thing_taken(world, tick, heroes, hero);
            }
        }
    }

    /// Seconds of holding E a clue or a hostage takes.
    fn hold_seconds(&self) -> f64 {
        match self.job() {
            CaseKind::Hostages => FREE_SECONDS,
            _ => CLUE_SECONDS,
        }
    }

    /// Clue or hostage `k`: locked in its safe, guarded, ready or done.
    fn state(&self, world: &mut World, k: usize) -> u8 {
        let c = &self.clues[k];
        if c.found {
            DONE
        } else if c.safe.is_some_and(|s| !s.open) {
            LOCKED
        } else if c.hostage.is_some_and(|(room, _)| {
            self.rooms
                .get(room)
                .is_some_and(|r| !subdued(world, r.spot))
        }) || c
            .guard
            .and_then(|room| self.rooms.get(room))
            .is_some_and(|r| !subdued(world, r.spot))
        {
            GUARDED
        } else {
            READY
        }
    }

    /// A hostage untied: they thank their hero, then walk away: out of a
    /// building by its route map and a few steps down the street, or away
    /// from their guards out in the open.
    fn set_free(&mut self, world: &mut World, tick: u64, id: u32) {
        crate::incidents::release_bystanders(world, tick, id);
        let floors = self.floors.clone();
        let site = self.site().clone();
        let solids = world.resource::<crate::Solids>().0.clone();
        let mut q = world.query::<(Entity, &Motion, &crate::incidents::Bystander)>();
        let Some((e, from)) = q
            .iter(world)
            .find(|(_, _, b)| b.incident == id)
            .map(|(e, m, _)| (e, m.0.pos))
        else {
            return;
        };
        let way = match site.building {
            Some(k) => {
                let b = &floors.buildings[k];
                let door = b.to_town(b.graph.front());
                let out =
                    DVec2::new(door.x - b.pos.x, door.z - b.pos.y).normalize_or(DVec2::X) * 8.0;
                let mut way = walk_out(&solids, &floors, from, door);
                way.push(door + DVec3::new(out.x, 0.0, out.y));
                way
            }
            None => {
                // Away from the middle, along open floor.
                let d =
                    DVec2::new(from.x - site.at.x, from.z - site.at.z).normalize_or(site.facing);
                let mut way = Vec::new();
                let mut at = from;
                for _ in 0..4 {
                    let p = DVec2::new(at.x, at.z) + d * 3.0;
                    let y = combat::stand_height(&solids, p, at.y);
                    if (y - at.y).abs() > 0.6 || !room_to_stand(&solids, p, y) {
                        break;
                    }
                    at = DVec3::new(p.x, y, p.y);
                    way.push(at);
                }
                if way.is_empty() {
                    way.push(from);
                }
                way
            }
        };
        world.entity_mut(e).insert(crate::incidents::WayOut(way));
    }

    /// Groups beaten, of the groups held (the boss's not counted).
    fn cleared(&self, world: &mut World) -> (u8, u8) {
        let all = || self.door.iter().chain(self.rooms.iter());
        let n = all().filter(|r| beaten(world, r.spot)).count();
        (n as u8, all().count() as u8)
    }

    /// Every group of the gang here, with what it is.
    fn groups_mut(&mut self) -> impl Iterator<Item = &mut Room> {
        self.door
            .iter_mut()
            .chain(self.rooms.iter_mut())
            .chain(self.cutoff.iter_mut())
            .chain(self.boss.iter_mut())
    }

    /// A room that fights for [`ALARM_SECONDS`] and still stands shouts
    /// for help: the nearest crew that has heard nothing yet comes along.
    /// One room, not the gang (the boss's guard never leaves the boss).
    fn raise_alarms(&mut self, world: &mut World, tick: u64, heroes: &[Seen]) {
        let after = (ALARM_SECONDS * TICK_HZ as f64) as u64;
        // Who each group fights, if it does.
        let fights: BTreeMap<usize, NetId> = fighting(world, heroes);
        let boss = self.boss.as_ref().map(|b| b.spot);
        let mut calls: Vec<(DVec3, NetId)> = Vec::new();
        for r in self.groups_mut() {
            if let Some(&foe) = fights.get(&r.spot) {
                let since = *r.noticed.get_or_insert(tick);
                if !r.called && tick >= since + after {
                    r.called = true;
                    if Some(r.spot) != boss {
                        calls.push((r.at, foe));
                    }
                }
            }
        }
        for (from, foe) in calls {
            self.call_nearest(world, tick, heroes, from, foe);
        }
    }

    /// The nearest crew to `from` that has nobody to fight takes on `foe`.
    /// Was there one?
    fn call_nearest(
        &mut self,
        world: &mut World,
        tick: u64,
        heroes: &[Seen],
        from: DVec3,
        foe: NetId,
    ) -> bool {
        let fights = fighting(world, heroes);
        let mut calm: Vec<(f64, usize)> = self
            .door
            .iter()
            .chain(self.rooms.iter())
            .filter(|r| r.noticed.is_none() && !fights.contains_key(&r.spot))
            .filter(|r| !beaten(world, r.spot))
            .map(|r| (r.at.distance(from), r.spot))
            .collect();
        calm.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let Some(&(_, spot)) = calm.first() else {
            return false;
        };
        set_on(world, &[spot], foe);
        // A crew that was called does not call on: one room, not a chain.
        for r in self.groups_mut().filter(|r| r.spot == spot) {
            r.noticed = Some(tick);
            r.called = true;
        }
        self.alarms = self.alarms.wrapping_add(1);
        true
    }

    /// The thing is taken: everyone still standing comes for `hero`, and a
    /// crew cuts off the way out.
    fn thing_taken(&mut self, world: &mut World, tick: u64, heroes: &[Seen], hero: NetId) {
        self.carrier = Some(hero);
        if let Some(at) = self.way_in {
            let facing = self
                .clues
                .first()
                .map(|c| DVec2::new(c.at.x - at.x, c.at.z - at.z))
                .unwrap_or(DVec2::X)
                .normalize_or(DVec2::X);
            let extra = heroes.len().saturating_sub(1) as u32;
            let r = self.group(
                world,
                tick,
                heroes.len(),
                at,
                facing,
                CUTOFF_SIZE + extra,
                None,
                false,
            );
            self.cutoff = Some(r);
        }
        let spots: Vec<usize> = self.groups_mut().map(|r| r.spot).collect();
        set_on(world, &spots, hero);
        for r in self.groups_mut() {
            r.noticed = Some(tick);
            r.called = true;
        }
        self.alarms = self.alarms.wrapping_add(1);
    }

    /// With the thing in hand: is its carrier (or, if they are gone, any
    /// hero) out of the building, or nobody left to stop them?
    fn got_out(&self, world: &mut World, heroes: &[Seen]) -> bool {
        let Some(k) = self.site().building else {
            return true;
        };
        if self.carrier.is_none() {
            return true;
        }
        let b = &self.floors.buildings[k];
        let carrier = heroes.iter().find(|h| Some(h.id) == self.carrier);
        let out = match carrier {
            Some(h) => !b.holds(h.at, OUT_MARGIN),
            None => heroes.iter().any(|h| !b.holds(h.at, OUT_MARGIN)),
        };
        let mut all = self
            .door
            .iter()
            .chain(self.rooms.iter())
            .chain(self.cutoff.iter())
            .chain(self.boss.iter());
        out || all.all(|r| subdued(world, r.spot))
    }

    /// The boss, hurt below [`BOSS_BREAKS`] of its health, breaks off once:
    /// it runs from the hero for [`BOSS_RUN_SECONDS`] and shouts for the
    /// nearest crew still standing.
    fn boss_breaks_off(&mut self, world: &mut World, tick: u64, heroes: &[Seen]) {
        if self.boss_ran.is_some() || self.job() != CaseKind::Takeover {
            return;
        }
        let Some(b) = self.boss.clone() else {
            return;
        };
        let kind = GANGS[self.gang].boss;
        let until = tick + (BOSS_RUN_SECONDS * TICK_HZ as f64) as u64;
        // Where to: the crew still standing farthest from here, else the
        // way out.
        let fights = fighting(world, heroes);
        let crews: Vec<DVec3> = self
            .rooms
            .iter()
            .chain(self.door.iter())
            .filter(|r| !fights.contains_key(&r.spot) && !subdued(world, r.spot))
            .map(|r| r.at)
            .collect();
        let way_in = self.way_in;
        let mut ran: Option<(DVec3, Option<NetId>)> = None;
        for (mut en, m, mut f) in world
            .query::<(&mut combat::Enemy, &Motion, &mut Fighter)>()
            .iter_mut(world)
        {
            if en.spot != b.spot || en.kind != kind || !f.standing() {
                continue;
            }
            if f.health > f.max_health * BOSS_BREAKS {
                continue;
            }
            let here = m.0.pos;
            let to = crews
                .iter()
                .copied()
                .max_by(|a, b| a.distance(here).total_cmp(&b.distance(here)))
                .or(way_in)
                .filter(|p| p.distance(here) > 4.0);
            match to {
                Some(to) => {
                    // A step short of the crew's middle: beside them.
                    en.home = to;
                    en.flee_until = until;
                    en.windup = None;
                }
                None => {
                    // Nowhere to run to: it backs off from the hero.
                    let from = heroes
                        .iter()
                        .min_by(|a, b| a.at.distance(here).total_cmp(&b.at.distance(here)))
                        .map_or(here, |h| h.at);
                    f.feared_until = f.feared_until.max(until);
                    f.fear_from = from;
                }
            }
            ran = Some((to.unwrap_or(here), f.target));
        }
        let Some((at, foe)) = ran else {
            return;
        };
        self.boss_ran = Some(until);
        self.alarms = self.alarms.wrapping_add(1);
        if let Some(foe) = foe.or(heroes.first().map(|h| h.id)) {
            // The crew it runs to takes the hero on (its own count of the
            // alarm was taken above).
            let before = self.alarms;
            self.call_nearest(world, tick, heroes, at, foe);
            self.alarms = before;
        }
    }
}

/// The groups that have someone to fight: spot, and whom.
/// Only a foe within [`FIGHT_REACH`] counts: one long gone is no fight.
fn fighting(world: &mut World, heroes: &[Seen]) -> BTreeMap<usize, NetId> {
    let mut out = BTreeMap::new();
    for (en, m, f) in world
        .query::<(&combat::Enemy, &Motion, &Fighter)>()
        .iter(world)
    {
        let near = |t: NetId| {
            heroes
                .iter()
                .any(|h| h.id == t && h.at.distance(m.0.pos) < FIGHT_REACH)
        };
        if let (true, Some(t)) = (f.standing(), f.target.filter(|t| near(*t))) {
            let e: &mut NetId = out.entry(en.spot).or_insert(t);
            *e = (*e).min(t);
        }
    }
    out
}

/// Everyone standing in `spots` with nobody to fight takes on `foe`.
fn set_on(world: &mut World, spots: &[usize], foe: NetId) {
    for (en, mut f) in world
        .query::<(&combat::Enemy, &mut Fighter)>()
        .iter_mut(world)
    {
        if spots.contains(&en.spot) && f.fighting() && f.target.is_none() {
            f.target = Some(foe);
        }
    }
}

/// Open floor near `p` where a group can stand: `p` itself, or the
/// nearest such place within a few metres, on the floor at its height.
fn open_near(solids: &crate::collision::Obstacles, p: DVec3) -> Option<DVec3> {
    let c = DVec2::new(p.x, p.z);
    for r in [0.0, 1.5, 3.0, 4.5, 6.0] {
        let n = if r == 0.0 { 1 } else { 8 };
        for k in 0..n {
            let q = c + hw_core::det::dir2(std::f64::consts::TAU * k as f64 / 8.0) * r;
            let y = combat::stand_height(solids, q, p.y);
            if (y - p.y).abs() < 1.5 && open_round(solids, q, y, 1.2) {
                return Some(DVec3::new(q.x, y, q.y));
            }
        }
    }
    None
}
/// The way from `from` in a building to its door, every flat stretch
/// round the furniture.
fn walk_out(
    solids: &crate::collision::Obstacles,
    floors: &Floors,
    from: DVec3,
    door: DVec3,
) -> Vec<DVec3> {
    let Some(legs) = floors.route(from, door, false) else {
        return vec![door];
    };
    let mut out = Vec::new();
    let mut at = from;
    for leg in legs {
        let flat = (leg.to.y - at.y).abs() < 0.3;
        if flat && !crate::nav::walkable_line(solids, at, leg.to) {
            if let Some(p) = crate::nav::find_path(solids, at, leg.to) {
                out.extend(p);
            }
        }
        out.push(leg.to);
        at = leg.to;
    }
    if out.last().is_none_or(|p| p.distance(door) > 0.1) {
        out.push(door);
    }
    out
}

/// Is a room's group all down (cuffed, or carried off by the drone) or gone?
fn subdued(world: &mut World, spot: usize) -> bool {
    beaten(world, spot)
        || (world
            .resource::<Fights>()
            .spots
            .get(spot)
            .is_some_and(|s| s.groups > 0)
            && world
                .query::<(&combat::Enemy, &Fighter)>()
                .iter(world)
                .filter(|(e, _)| e.spot == spot)
                .all(|(_, f)| {
                    !matches!(f.status, hw_net::Status::Standing | hw_net::Status::Knocked)
                }))
}

/// Is a room's group all beaten (or gone)?
fn beaten(world: &World, spot: usize) -> bool {
    let f = world.resource::<Fights>();
    f.spots
        .get(spot)
        .is_some_and(|s| s.groups > 0 && s.members == 0)
}

pub(crate) fn flat(a: DVec3, b: DVec3) -> f64 {
    DVec2::new(a.x - b.x, a.z - b.z).length()
}

/// Is there open floor round `p` (a group can stand and fight there)?
fn open_round(solids: &crate::collision::Obstacles, p: DVec2, y: f64, r: f64) -> bool {
    room_to_stand(solids, p, y)
        && (0..8).all(|k| {
            let d = hw_core::det::dir2(std::f64::consts::TAU * k as f64 / 8.0);
            room_to_stand(solids, p + d * r, y)
        })
}

/// The open floor nearest a room's middle, in the town's frame.
fn open_middle(
    solids: &crate::collision::Obstacles,
    b: &crate::floornav::Building,
    rect: &hw_gen::geom::Rect,
    y: f64,
) -> Option<DVec3> {
    let c = rect.center();
    // Room for a crew to stand in; a narrow back room has less.
    [1.8, 1.1].into_iter().find_map(|round| {
        candidates(rect)
            .into_iter()
            .filter_map(|p| {
                let t = b.to_town(DVec3::new(p.x, y, p.y));
                open_round(solids, DVec2::new(t.x, t.z), t.y, round).then_some((p.distance(c), t))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|x| x.1)
    })
}

/// Open floor in a room as far as can be from `from` (a clue tucked away
/// from where the group stands).
fn far_corner(
    solids: &crate::collision::Obstacles,
    b: &crate::floornav::Building,
    rect: &hw_gen::geom::Rect,
    y: f64,
    from: DVec3,
) -> Option<DVec3> {
    candidates(rect)
        .into_iter()
        .filter_map(|p| {
            let t = b.to_town(DVec3::new(p.x, y, p.y));
            open_round(solids, DVec2::new(t.x, t.z), t.y, 0.9).then_some((flat(t, from), t))
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|x| x.1)
}

/// Points on a metre grid in a room, two metres in from its walls.
fn candidates(rect: &hw_gen::geom::Rect) -> Vec<DVec2> {
    let r = rect.inset(2.0);
    let mut out = Vec::new();
    let mut x = r.min.x;
    while x <= r.max.x {
        let mut z = r.min.y;
        while z <= r.max.y {
            out.push(DVec2::new(x, z));
            z += 1.0;
        }
        x += 1.0;
    }
    out
}

/// The rooms of a building a gang can hold: halls, lobbies and shops at
/// least 8 m a side, a flat's rooms at least 4.5 m.
pub(crate) fn gang_rooms(
    b: &crate::floornav::Building,
) -> Vec<(SpaceKind, hw_gen::geom::Rect, f64)> {
    b.graph
        .rooms()
        .into_iter()
        .filter(|(k, r, _)| {
            let size = r.size().min_element();
            (matches!(
                k,
                SpaceKind::Hall
                    | SpaceKind::Boardroom
                    | SpaceKind::Lobby
                    | SpaceKind::Shop
                    | SpaceKind::Concourse
                    | SpaceKind::Vestibule
            ) && size >= 8.0)
                // A hideout's flats and back rooms are smaller.
                || (matches!(
                    k,
                    SpaceKind::Living | SpaceKind::Kitchen | SpaceKind::Bedroom | SpaceKind::Service
                ) && size >= 3.8)
        })
        .collect()
}
