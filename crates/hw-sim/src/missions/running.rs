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
}

/// A clue to find, or a hostage to free.
#[derive(Clone, Copy, Debug)]
struct Clue {
    at: DVec3,
    found: bool,
    /// A hostage: the room whose crew guards them (in `rooms`), and their
    /// bystander number.
    hostage: Option<(usize, u32)>,
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
            clues: Vec::new(),
            held: 0.0,
            away_since: None,
            below: false,
        }
    }

    pub fn steps(&self) -> &[Step] {
        &self.plan.steps
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
                if !self.step_done(world) {
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

    fn step_done(&self, world: &mut World) -> bool {
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
                    self.clues.iter().all(|k| k.found)
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
            extra: if boss { BOSS_EXTRA } else { 0 },
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
        Room { spot: index, at }
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
        let mut rooms: Vec<(SpaceKind, hw_gen::geom::Rect, f64)> = b
            .graph
            .rooms()
            .into_iter()
            .filter(|(k, r, _)| {
                matches!(
                    k,
                    SpaceKind::Hall
                        | SpaceKind::Boardroom
                        | SpaceKind::Lobby
                        | SpaceKind::Shop
                        | SpaceKind::Concourse
                        | SpaceKind::Vestibule
                ) && r.size().min_element() >= 8.0
            })
            .collect();
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
        while chosen.len() < ROOMS && chosen.len() < rest.len() {
            for &y in &levels {
                let on: Vec<_> = rest.iter().filter(|r| (r.2 - y).abs() < 1e-3).collect();
                if let Some(r) = on.get(round) {
                    if chosen.len() < ROOMS {
                        chosen.push(**r);
                    }
                }
            }
            round += 1;
            if round > 8 {
                break;
            }
        }
        let extra = heroes.saturating_sub(1) as u32;
        // Facing the way in: heroes come from the street.
        let door = b.to_town(b.graph.front());
        let mut placed: Vec<(Room, (SpaceKind, hw_gen::geom::Rect, f64))> = Vec::new();
        for r in &chosen {
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
                    self.clues.push(Clue {
                        at,
                        found: false,
                        hostage,
                    });
                    used.push(i);
                }
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
        self.rooms.clear();
        self.boss = None;
        self.clues.clear();
        self.held = 0.0;
    }

    /// Heroes holding the help key at a clue, or by a hostage whose
    /// guards are beaten.
    pub(crate) fn search(&mut self, world: &mut World, tick: u64, heroes: &[Seen]) {
        if self.stage != INSIDE {
            return;
        }
        let states: Vec<u8> = (0..self.clues.len())
            .map(|k| self.state(world, k))
            .collect();
        let Some(k) = (0..self.clues.len()).find(|&k| {
            states[k] == READY
                && heroes
                    .iter()
                    .any(|h| h.help && h.at.distance(self.clues[k].at) < CLUE_REACH)
        }) else {
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
        }
    }

    /// Seconds of holding E a clue or a hostage takes.
    fn hold_seconds(&self) -> f64 {
        match self.job() {
            CaseKind::Hostages => FREE_SECONDS,
            _ => CLUE_SECONDS,
        }
    }

    /// Clue or hostage `k`: guarded, ready or done.
    fn state(&self, world: &mut World, k: usize) -> u8 {
        let c = &self.clues[k];
        if c.found {
            DONE
        } else if c.hostage.is_some_and(|(room, _)| {
            self.rooms
                .get(room)
                .is_some_and(|r| !subdued(world, r.spot))
        }) {
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
        let n = self.rooms.iter().filter(|r| beaten(world, r.spot)).count();
        (n as u8, self.rooms.len() as u8)
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

fn flat(a: DVec3, b: DVec3) -> f64 {
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
    candidates(rect)
        .into_iter()
        .filter_map(|p| {
            let t = b.to_town(DVec3::new(p.x, y, p.y));
            open_round(solids, DVec2::new(t.x, t.z), t.y, 1.8).then_some((p.distance(c), t))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|x| x.1)
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
