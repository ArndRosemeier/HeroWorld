//! Exploring: heroes find the world's sights (`hw_gen::urban::sights`).
//!
//! * A **wisp** is gathered by touching it: a little karma each, a rare
//!   augment for every tenth, a relic for the last.
//! * An **Aether spring** gives an augment the first time a hero stands in
//!   its pool (the further out the spring, the better), and fills their
//!   health and energy every time.
//! * **Waystones** give karma the first time a hero stands on their altar,
//!   and every time they sing: the client then shows where unfound sights
//!   lie near by for a while.
//! * A **course** starts when a hero comes through its first gate and ends
//!   at its last; the time earns a bronze, silver or gold medal, each paid
//!   once. On a rooftop course flying ends the run.
//!
//! What a hero found is kept in their file ([`hw_net::FindBook`]) by the
//! world's own numbers, so every sight pays once per hero. The server
//! decides everything here; clients only draw it.

use crate::augments::{self, Grade};
use crate::combat::{Drop, Fighter, Fights, Reward};
use crate::{progress, Control, Hero, Motion, Moves, Net, Progress, DT};
use bevy_ecs::prelude::*;
use glam::DVec3;
use hw_core::NetId;
use hw_gen::urban::sights::{CourseKind, Sights};
use hw_gen::urban::world::TownWorld;
use hw_net::protocol::{course, sight};
use hw_net::{FindBook, Fx, FxKind, PlayerId, Status};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Karma for a find, as a share of what the hero's level costs: so a wisp
/// is worth stopping for at any level.
pub const WISP_SHARE: f64 = 0.03;
pub const SPRING_SHARE: f64 = 0.25;
pub const STONES_SHARE: f64 = 0.3;
/// Karma for a medal earned for the first time: bronze, silver, gold (a
/// gold run that skips the others is paid all three).
pub const MEDAL_SHARE: [f64; 3] = [0.08, 0.1, 0.14];
/// Every so many wisps a hero gathers, a rare augment comes with it.
pub const WISPS_A_PRIZE: usize = 10;
/// A run is off once it has taken this many times the bronze time, or
/// the hero is this far from the next gate (m).
const RUN_PATIENCE: f64 = 3.0;
const RUN_LEASH: f64 = 700.0;
/// A spring fills a hero up again at most this often, and waystones sing
/// again this often while someone stands on the altar (s).
const SOAK_EVERY: f64 = 2.0;
const SING_EVERY: f64 = 6.0;
/// Edge of a cell of the wisps' grid (m).
const CELL: f64 = 64.0;

/// Karma for a share of the level a hero of `lifetime` karma is on.
pub fn share_karma(lifetime: u32, share: f64) -> u32 {
    let level = progress::level_of(lifetime);
    ((progress::karma_to_next(level) as f64 * share).round() as u32).max(1)
}

/// A hero's run on a course.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    pub course: u8,
    /// The next gate to come through.
    pub next: u8,
    /// The tick the first gate was passed.
    pub started: u64,
}

/// What each hero is doing among the sights.
#[derive(Clone, Copy, Debug, Default)]
struct Roamer {
    run: Option<Run>,
    /// The start gate the hero is in now (a run starts on the way in).
    in_start: Option<u8>,
    /// When a spring last filled them up and stones last sang (ticks).
    soaked: u64,
    sung: u64,
}

/// The exploring of one world.
#[derive(Default)]
pub struct Explore {
    town: Option<Arc<TownWorld>>,
    sights: Option<Arc<Sights>>,
    world_key: u64,
    wisps: BTreeMap<(i64, i64), Vec<u16>>,
    roamers: BTreeMap<PlayerId, Roamer>,
    /// Augments owed to heroes: one is handed out per hero and tick.
    owed: Vec<Drop>,
}

fn cell(p: DVec3) -> (i64, i64) {
    ((p.x / CELL).floor() as i64, (p.z / CELL).floor() as i64)
}

/// `sheet`'s book of finds in the world `key`: a book from another world
/// is put away for a new one.
fn book(finds: &mut FindBook, key: u64) -> &mut FindBook {
    if finds.world != key {
        *finds = FindBook {
            world: key,
            ..FindBook::default()
        };
    }
    finds
}

/// An augment of `grade` for a hero of `level` (which effect is rolled
/// when it is handed over).
fn prize(hero: NetId, at: DVec3, grade: Grade, level: u8) -> Drop {
    Drop {
        hero,
        from: hero,
        at,
        carried: Some(hw_net::Augment {
            effect: 0,
            percent: augments::strength(grade, level),
            grade: grade.code(),
        }),
        level,
    }
}

impl Explore {
    /// The exploring of `town`, whose world is `world_key` in heroes'
    /// files. The sights are planned on a thread of their own; until they
    /// are, there is nothing to find.
    pub fn new(town: Arc<TownWorld>, world_key: u64) -> Self {
        town.plan_sights_in_background();
        Self {
            town: Some(town),
            world_key,
            ..Self::default()
        }
    }

    /// The sights, once planned.
    pub fn sights(&self) -> Option<&Arc<Sights>> {
        self.sights.as_ref()
    }

    /// The run `owner`'s hero is on.
    pub fn run_of(&self, owner: PlayerId) -> Option<Run> {
        self.roamers.get(&owner).and_then(|r| r.run)
    }

    /// Waits for the sights (tests and tools; the game never waits).
    pub fn wait(&mut self) {
        if let Some(t) = &self.town {
            let _ = t.sights();
        }
        self.take_sights();
    }

    fn take_sights(&mut self) {
        if self.sights.is_some() {
            return;
        }
        let Some(s) = self.town.as_ref().and_then(|t| t.sights_ready()) else {
            return;
        };
        for (i, w) in s.wisps.iter().enumerate() {
            self.wisps.entry(cell(w.at)).or_default().push(i as u16);
        }
        self.sights = Some(s.clone());
    }

    /// One tick: who found what, and how the runs stand.
    pub fn step(&mut self, world: &mut World, tick: u64) {
        self.take_sights();
        let Some(sights) = self.sights.clone() else {
            return;
        };
        let key = self.world_key;
        let secs = |ticks: u64| ticks as f64 * DT;
        let mut events: Vec<Fx> = Vec::new();
        let mut rewards: Vec<Reward> = Vec::new();
        let mut seen: Vec<PlayerId> = Vec::new();
        let mut q = world.query::<(&Hero, &Net, &Motion, &Control, &mut Progress, &mut Fighter)>();
        for (hero, net, motion, control, mut progress, mut fighter) in q.iter_mut(world) {
            seen.push(hero.owner);
            let roamer = self.roamers.entry(hero.owner).or_default();
            if !matches!(fighter.status, Status::Standing | Status::Knocked) {
                // Down or carried off: a run is over.
                if let Some(run) = roamer.run.take() {
                    events.push(run_event(
                        tick,
                        net.0,
                        motion.0.pos,
                        run.course,
                        course::OFF,
                        0,
                    ));
                }
                continue;
            }
            let feet = motion.0.pos;
            let mid = feet + DVec3::Y;
            let id = net.0;
            let fx = |kind: FxKind, at: DVec3| Fx {
                tick,
                kind,
                from: id,
                to: id,
                at: at.as_vec3().to_array(),
                amount: 0,
                knock: false,
                defeat: false,
                theme: 0,
                element: 0,
            };
            // Is the hero's book this world's?
            let known = |p: &Progress| p.sheet.finds.world == key;
            let lifetime = progress.sheet.lifetime;
            let level = progress::level_of(lifetime);

            // Wisps.
            let c = cell(mid);
            for cx in c.0 - 1..=c.0 + 1 {
                for cz in c.1 - 1..=c.1 + 1 {
                    let Some(list) = self.wisps.get(&(cx, cz)) else {
                        continue;
                    };
                    for &i in list {
                        let w = sights.wisps[i as usize];
                        if w.at.distance_squared(mid)
                            > hw_gen::urban::sights::WISP_REACH * hw_gen::urban::sights::WISP_REACH
                        {
                            continue;
                        }
                        if known(&progress) && progress.sheet.finds.has_wisp(i as usize) {
                            continue;
                        }
                        let b = book(&mut progress.sheet.finds, key);
                        if !b.take_wisp(i as usize) {
                            continue;
                        }
                        let n = b.wisp_count();
                        progress.changed = true;
                        rewards.push(Reward {
                            hero: id,
                            karma: share_karma(lifetime, WISP_SHARE),
                        });
                        let mut e = fx(FxKind::Sight, w.at);
                        e.theme = sight::WISP;
                        e.amount = i;
                        e.knock = true;
                        events.push(e);
                        if n == sights.wisps.len() {
                            self.owed.push(prize(id, w.at, Grade::Relic, level));
                        } else if n.is_multiple_of(WISPS_A_PRIZE) {
                            self.owed.push(prize(id, w.at, Grade::Rare, level));
                        }
                    }
                }
            }

            // Springs.
            for (i, s) in sights.wild.springs.iter().enumerate() {
                if !s.holds(feet) {
                    continue;
                }
                let first = !(known(&progress) && progress.sheet.finds.has_spring(i));
                if first {
                    book(&mut progress.sheet.finds, key).springs |= 1 << i;
                    progress.changed = true;
                    rewards.push(Reward {
                        hero: id,
                        karma: share_karma(lifetime, SPRING_SHARE),
                    });
                    // The near springs give a rare augment, the far ones
                    // a relic, those between either.
                    self.owed.push(match s.tier {
                        0 => prize(id, s.pool(), Grade::Rare, level),
                        1 => Drop {
                            hero: id,
                            from: id,
                            at: s.pool(),
                            carried: None,
                            level,
                        },
                        _ => prize(id, s.pool(), Grade::Relic, level),
                    });
                }
                let thirsty =
                    fighter.health < fighter.max_health || fighter.energy < fighter.max_energy;
                if first || (thirsty && secs(tick.saturating_sub(roamer.soaked)) >= SOAK_EVERY) {
                    roamer.soaked = tick;
                    fighter.health = fighter.max_health;
                    fighter.energy = fighter.max_energy;
                    let mut e = fx(FxKind::Sight, s.pool());
                    e.theme = sight::SPRING;
                    e.amount = i as u16;
                    e.knock = first;
                    events.push(e);
                }
            }

            // Waystones.
            for (i, s) in sights.wild.stones.iter().enumerate() {
                if !s.holds(feet) {
                    continue;
                }
                let first = !(known(&progress) && progress.sheet.finds.has_stones(i));
                if first {
                    let b = book(&mut progress.sheet.finds, key);
                    b.stones |= 1 << i;
                    let all = b.stones.count_ones() as usize == sights.wild.stones.len();
                    progress.changed = true;
                    rewards.push(Reward {
                        hero: id,
                        karma: share_karma(lifetime, STONES_SHARE),
                    });
                    if all {
                        self.owed.push(prize(id, s.altar(), Grade::Relic, level));
                    }
                }
                if first || secs(tick.saturating_sub(roamer.sung)) >= SING_EVERY {
                    roamer.sung = tick;
                    let mut e = fx(FxKind::Sight, s.altar());
                    e.theme = sight::STONES;
                    e.amount = i as u16;
                    e.knock = first;
                    events.push(e);
                }
            }

            // Courses.
            let flying = control.0.fly && Moves::of(&fighter.kit, fighter.level).fly;
            let start = sights
                .courses
                .iter()
                .position(|c| c.through(0, mid) && !(c.kind == CourseKind::Rooftop && flying));
            let entered = start
                .map(|s| s as u8)
                .filter(|s| roamer.in_start != Some(*s));
            roamer.in_start = start.map(|s| s as u8);
            if let Some(c) = entered {
                roamer.run = Some(Run {
                    course: c,
                    next: 1,
                    started: tick,
                });
                events.push(run_event(
                    tick,
                    id,
                    sights.courses[c as usize].gates[0].at,
                    c,
                    course::START,
                    0,
                ));
            }
            if let Some(mut run) = roamer.run {
                let c = &sights.courses[run.course as usize];
                let gate = c.gates[run.next as usize];
                let took = secs(tick - run.started);
                let off = (c.kind == CourseKind::Rooftop && flying)
                    || took > c.pars[2] * RUN_PATIENCE
                    || gate.at.distance(mid) > RUN_LEASH;
                if off {
                    roamer.run = None;
                    events.push(run_event(
                        tick,
                        id,
                        mid,
                        run.course,
                        course::OFF,
                        run.next as u16,
                    ));
                } else if c.through(run.next as usize, mid) {
                    if run.next as usize + 1 < c.gates.len() {
                        events.push(run_event(
                            tick,
                            id,
                            gate.at,
                            run.course,
                            course::GATE,
                            run.next as u16,
                        ));
                        run.next += 1;
                        roamer.run = Some(run);
                    } else {
                        roamer.run = None;
                        let tenths = (took * 10.0).round().clamp(1.0, u16::MAX as f64) as u16;
                        let medal = c.medal(tenths as f64 / 10.0);
                        let b = book(&mut progress.sheet.finds, key);
                        let had = b.best(run.course).map_or(0, |t| c.medal(t as f64 / 10.0));
                        let best = b.note_time(run.course, tenths);
                        if best {
                            progress.changed = true;
                        }
                        if medal > had {
                            let share: f64 = MEDAL_SHARE[had as usize..medal as usize].iter().sum();
                            rewards.push(Reward {
                                hero: id,
                                karma: share_karma(lifetime, share),
                            });
                            if medal == 3 {
                                self.owed.push(prize(id, gate.at, Grade::Rare, level));
                            }
                        }
                        let mut e =
                            run_event(tick, id, gate.at, run.course, course::FINISH, tenths);
                        e.knock = best;
                        e.defeat = medal > had;
                        events.push(e);
                    }
                }
            }
        }
        self.roamers.retain(|p, _| seen.contains(p));
        // One owed augment per hero and tick (the hand-over tells two of
        // one tick apart by who they come from).
        let mut now: Vec<Drop> = Vec::new();
        self.owed.retain(|d| {
            if now.iter().any(|n| n.hero == d.hero) {
                return true;
            }
            now.push(*d);
            false
        });
        let mut fights = world.resource_mut::<Fights>();
        fights.events.extend(events);
        fights.rewards.extend(rewards);
        fights.drops.extend(now);
    }
}

fn run_event(tick: u64, hero: NetId, at: DVec3, c: u8, what: u8, amount: u16) -> Fx {
    Fx {
        tick,
        kind: FxKind::Course,
        from: hero,
        to: hero,
        at: at.as_vec3().to_array(),
        amount,
        knock: false,
        defeat: false,
        theme: c,
        element: what,
    }
}
