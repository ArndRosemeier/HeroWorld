//! Who is simulated how often: the tier scheduler.
//!
//! Every simulated thing (a villain, a car, a body) is in exactly one tier at
//! any time, decided here and nowhere else (see `docs/SERVER_TIERS.md`):
//!
//! - [`Tier::Full`]: steps every tick. Close to a hero (or pinned there by
//!   what it is doing: fighting a hero, carrying someone).
//! - [`Tier::Reduced`]: steps every `stride`th tick, handed the time that
//!   passed, so it moves at normal speed in bigger steps.
//! - [`Tier::Dormant`]: does not step. It holds still where it is and wakes
//!   when a hero comes near.
//!
//! Distance only sets the order in which things are let in. What makes the
//! cost bounded is the count budget of each tier: when a tier is full, the
//! things farthest from the heroes drop one tier, so a crowd can slow things
//! down but never make a tick longer. Waking things is work too, so only
//! `max_wakes` things wake per tick (the nearest first).
//!
//! The scheduler is pure data in, plan out: no ECS, no clock, no threads.

use glam::DVec3;
use hw_core::NetId;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Tier {
    Full,
    Reduced,
    Dormant,
}

impl Tier {
    fn index(self) -> usize {
        self as usize
    }
}

/// A hero the tiers are measured from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    pub pos: DVec3,
    /// Velocity (m/s): things ahead of a fast hero wake before it arrives.
    pub vel: DVec3,
}

/// A thing to schedule.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Subject {
    pub id: NetId,
    pub pos: DVec3,
    /// Must step every tick whatever its distance (it is fighting a hero,
    /// carrying someone, in a lift...). Pinned things are let in first, but
    /// the budget still holds for them.
    pub pinned: bool,
}

/// The rings and budgets. The numbers are starting values to tune with
/// measurements, not findings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    /// Full inside this distance...
    pub full_enter: f64,
    /// ...and stays Full until this far (hysteresis, so nothing flickers).
    pub full_exit: f64,
    pub reduced_enter: f64,
    pub reduced_exit: f64,
    /// A Reduced thing steps every this many ticks.
    pub reduced_stride: u64,
    /// Most things per tier (the guarantee).
    pub full_budget: usize,
    pub reduced_budget: usize,
    /// Most things woken from Dormant in one tick.
    pub max_wakes: usize,
    /// Seconds ahead of a moving hero in which things already count as near.
    pub look_ahead: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            full_enter: 1000.0,
            full_exit: 1100.0,
            reduced_enter: 2000.0,
            reduced_exit: 2100.0,
            reduced_stride: 2,
            full_budget: 800,
            reduced_budget: 1600,
            max_wakes: 40,
            look_ahead: 3.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Slot {
    tier: Tier,
    /// The last tick it stepped (or woke).
    last_run: u64,
}

/// What to run this tick.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    /// Things to step this tick with the ticks of game time to give each
    /// (1 for Full; the stride for Reduced), in ID order.
    pub run: Vec<(NetId, u32)>,
    /// Things that woke this tick and how many ticks they were dormant: a
    /// system keeping absolute deadlines moves them on by this much, so what
    /// was frozen stays frozen.
    pub woke: Vec<(NetId, u64)>,
    /// How many things are in each tier, `[Full, Reduced, Dormant]`.
    pub counts: [usize; 3],
    /// Things that wanted to wake but were held back by `max_wakes`.
    pub wakes_deferred: usize,
}

/// The scheduler: remembers each thing's tier between ticks.
#[derive(Clone, Debug, Default)]
pub struct Tiers {
    pub cfg: Config,
    slots: BTreeMap<NetId, Slot>,
}

fn nearest(anchors: &[Anchor], p: DVec3) -> f64 {
    anchors
        .iter()
        .map(|a| a.pos.distance(p))
        .fold(f64::INFINITY, f64::min)
}

impl Tiers {
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg,
            slots: BTreeMap::new(),
        }
    }

    /// The tier of a thing as of the last plan (Dormant if unknown).
    pub fn tier(&self, id: NetId) -> Tier {
        self.slots.get(&id).map_or(Tier::Dormant, |s| s.tier)
    }

    /// Decides the tiers for `tick` and what runs.
    pub fn plan(&mut self, tick: u64, anchors: &[Anchor], subjects: &[Subject]) -> Plan {
        let cfg = self.cfg;
        // Things that vanished leave the books.
        let live: std::collections::BTreeSet<NetId> = subjects.iter().map(|s| s.id).collect();
        self.slots.retain(|id, _| live.contains(id));

        struct Cand {
            id: NetId,
            /// Distance now, and the nearest distance along the heroes' way.
            now: f64,
            ahead: f64,
            prev: Option<Tier>,
            pinned: bool,
        }
        let mut cands: Vec<Cand> = subjects
            .iter()
            .map(|s| {
                let now = nearest(anchors, s.pos);
                let ahead = anchors
                    .iter()
                    .flat_map(|a| [0.5, 1.0].map(|k| a.pos + a.vel * (cfg.look_ahead * k)))
                    .map(|p| p.distance(s.pos))
                    .fold(now, f64::min);
                Cand {
                    id: s.id,
                    now,
                    ahead,
                    prev: self.slots.get(&s.id).map(|s| s.tier),
                    pinned: s.pinned,
                }
            })
            .collect();
        // Pinned first, then nearest, then by ID: the order is total, so the
        // result never depends on how the caller listed things.
        cands.sort_by(|a, b| {
            b.pinned
                .cmp(&a.pinned)
                .then(a.now.total_cmp(&b.now))
                .then(a.id.cmp(&b.id))
        });

        let mut counts = [0usize; 3];
        let mut wakes = 0usize;
        let mut wakes_deferred = 0usize;
        let mut woke = Vec::new();
        let mut assigned: Vec<(NetId, Tier)> = Vec::with_capacity(cands.len());
        for c in &cands {
            let prev = c.prev.unwrap_or(Tier::Dormant);
            // The tier this thing would have with room to spare.
            let wanted = if c.pinned
                || c.ahead < cfg.full_enter
                || (prev == Tier::Full && c.now < cfg.full_exit)
            {
                Tier::Full
            } else if c.ahead < cfg.reduced_enter
                || (prev != Tier::Dormant && c.now < cfg.reduced_exit)
            {
                Tier::Reduced
            } else {
                Tier::Dormant
            };
            // Budgets: what does not fit drops a tier.
            let mut tier = wanted;
            if tier == Tier::Full && counts[0] >= cfg.full_budget {
                tier = Tier::Reduced;
            }
            if tier == Tier::Reduced && counts[1] >= cfg.reduced_budget {
                tier = Tier::Dormant;
            }
            // Waking is work: only so many per tick.
            if prev == Tier::Dormant && tier != Tier::Dormant {
                if wakes >= cfg.max_wakes {
                    tier = Tier::Dormant;
                    wakes_deferred += 1;
                } else {
                    wakes += 1;
                }
            }
            counts[tier.index()] += 1;
            assigned.push((c.id, tier));
        }

        let mut run = Vec::new();
        for (id, tier) in assigned {
            let slot = self.slots.get(&id).copied();
            let prev = slot.map_or(Tier::Dormant, |s| s.tier);
            if prev == Tier::Dormant && tier != Tier::Dormant {
                let asleep = slot.map_or(0, |s| tick.saturating_sub(s.last_run));
                woke.push((id, asleep));
                // The first step after waking is one ordinary tick.
                self.slots.insert(
                    id,
                    Slot {
                        tier,
                        last_run: tick,
                    },
                );
                run.push((id, 1));
                continue;
            }
            let due = match tier {
                Tier::Full => true,
                // Turns go round by ID, so each tick runs the same share.
                Tier::Reduced => (id.0 as u64 + tick).is_multiple_of(cfg.reduced_stride),
                Tier::Dormant => false,
            };
            let last_run = slot.map_or(tick, |s| s.last_run);
            if due {
                let elapsed = tick.saturating_sub(last_run).max(1);
                run.push((id, elapsed.min(u32::MAX as u64) as u32));
                self.slots.insert(
                    id,
                    Slot {
                        tier,
                        last_run: tick,
                    },
                );
            } else {
                self.slots.insert(id, Slot { tier, last_run });
            }
        }
        run.sort_by_key(|(id, _)| *id);
        woke.sort_by_key(|(id, _)| *id);
        Plan {
            run,
            woke,
            counts,
            wakes_deferred,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hero(x: f64) -> Anchor {
        Anchor {
            pos: DVec3::new(x, 0.0, 0.0),
            vel: DVec3::ZERO,
        }
    }

    fn at(id: u32, x: f64) -> Subject {
        Subject {
            id: NetId(id),
            pos: DVec3::new(x, 0.0, 0.0),
            pinned: false,
        }
    }

    #[test]
    fn rings_set_the_tier() {
        let mut t = Tiers::default();
        let p = t.plan(
            10,
            &[hero(0.0)],
            &[at(1, 500.0), at(2, 1500.0), at(3, 2500.0)],
        );
        assert_eq!(t.tier(NetId(1)), Tier::Full);
        assert_eq!(t.tier(NetId(2)), Tier::Reduced);
        assert_eq!(t.tier(NetId(3)), Tier::Dormant);
        assert_eq!(p.counts, [1, 1, 1]);
        assert!(p.run.iter().any(|(id, e)| *id == NetId(1) && *e == 1));
    }

    #[test]
    fn a_thing_on_a_ring_edge_does_not_flicker() {
        let mut t = Tiers::default();
        t.plan(0, &[hero(0.0)], &[at(1, 990.0)]);
        assert_eq!(t.tier(NetId(1)), Tier::Full);
        // Wobbling around the 1 km line changes nothing until 1.1 km.
        for (k, x) in [1010.0, 990.0, 1050.0, 1090.0, 1005.0]
            .into_iter()
            .enumerate()
        {
            t.plan(1 + k as u64, &[hero(0.0)], &[at(1, x)]);
            assert_eq!(t.tier(NetId(1)), Tier::Full, "at {x}");
        }
        t.plan(10, &[hero(0.0)], &[at(1, 1110.0)]);
        assert_eq!(t.tier(NetId(1)), Tier::Reduced);
        // And it needs to come back under 1 km to be Full again.
        t.plan(11, &[hero(0.0)], &[at(1, 1050.0)]);
        assert_eq!(t.tier(NetId(1)), Tier::Reduced);
        t.plan(12, &[hero(0.0)], &[at(1, 990.0)]);
        assert_eq!(t.tier(NetId(1)), Tier::Full);
    }

    #[test]
    fn a_pinned_thing_stays_full_far_away() {
        let mut t = Tiers::default();
        let mut s = at(1, 5000.0);
        s.pinned = true;
        t.plan(0, &[hero(0.0)], &[s]);
        assert_eq!(t.tier(NetId(1)), Tier::Full);
        s.pinned = false;
        t.plan(1, &[hero(0.0)], &[s]);
        assert_eq!(t.tier(NetId(1)), Tier::Dormant);
    }

    #[test]
    fn reduced_things_take_turns_and_get_the_elapsed_time() {
        let mut t = Tiers::new(Config {
            max_wakes: 1000,
            ..Config::default()
        });
        let things: Vec<Subject> = (0..100).map(|i| at(i, 1500.0)).collect();
        // The first plan wakes them all (ordinary tick), then they take turns.
        t.plan(0, &[hero(0.0)], &things);
        let mut per_tick = Vec::new();
        let mut last = vec![0u64; 100];
        for tick in 1..21u64 {
            let p = t.plan(tick, &[hero(0.0)], &things);
            per_tick.push(p.run.len());
            for (id, elapsed) in &p.run {
                // Whatever the turn, the thing is handed all the time since its last one.
                assert_eq!((tick - last[id.0 as usize]) as u32, *elapsed);
                last[id.0 as usize] = tick;
            }
        }
        // Half of them each tick: an even load, not a spike every other tick.
        assert!(per_tick.iter().all(|n| *n == 50), "{per_tick:?}");
    }

    #[test]
    fn budgets_hold_however_many_things_there_are() {
        let cfg = Config {
            full_budget: 100,
            reduced_budget: 200,
            max_wakes: 1_000_000,
            ..Config::default()
        };
        let mut t = Tiers::new(cfg);
        // Ten heroes spread over 20 km, 50,000 things everywhere.
        let anchors: Vec<Anchor> = (0..10).map(|k| hero(k as f64 * 2000.0)).collect();
        let things: Vec<Subject> = (0..50_000u32)
            .map(|i| at(i, (i % 2500) as f64 * 8.0 - 500.0))
            .collect();
        for tick in 0..4 {
            let p = t.plan(tick, &anchors, &things);
            assert!(p.counts[0] <= 100 && p.counts[1] <= 200, "{:?}", p.counts);
            assert_eq!(p.counts.iter().sum::<usize>(), 50_000);
            // What runs is bounded: Full plus the Reduced share.
            assert!(p.run.len() <= 100 + 200);
        }
    }

    #[test]
    fn the_nearest_win_the_budget_and_pinned_come_first() {
        let cfg = Config {
            full_budget: 2,
            ..Config::default()
        };
        let mut t = Tiers::new(cfg);
        let mut far = at(9, 900.0);
        far.pinned = true;
        t.plan(
            0,
            &[hero(0.0)],
            &[at(1, 100.0), at(2, 200.0), at(3, 300.0), far],
        );
        assert_eq!(t.tier(NetId(9)), Tier::Full);
        assert_eq!(t.tier(NetId(1)), Tier::Full);
        // The two others fall one tier, the farthest of the near ones first.
        assert_eq!(t.tier(NetId(2)), Tier::Reduced);
        assert_eq!(t.tier(NetId(3)), Tier::Reduced);
    }

    #[test]
    fn the_result_does_not_depend_on_the_order_things_are_listed() {
        let things: Vec<Subject> = (0..300u32).map(|i| at(i, i as f64 * 11.0)).collect();
        let mut reversed = things.clone();
        reversed.reverse();
        let cfg = Config {
            full_budget: 50,
            reduced_budget: 60,
            ..Config::default()
        };
        let (mut a, mut b) = (Tiers::new(cfg), Tiers::new(cfg));
        for tick in 0..5 {
            assert_eq!(
                a.plan(tick, &[hero(0.0)], &things),
                b.plan(tick, &[hero(0.0)], &reversed)
            );
        }
    }

    #[test]
    fn a_fast_hero_wakes_what_is_ahead_in_time() {
        let mut t = Tiers::default();
        let boosting = Anchor {
            pos: DVec3::ZERO,
            vel: DVec3::new(60.0, 0.0, 0.0),
        };
        // 3 s at 60 m/s is 180 m: a thing 1150 m ahead is within 1 km of
        // where the hero will be, so it is awake before the hero arrives.
        t.plan(0, &[boosting], &[at(1, 1150.0), at(2, -1150.0)]);
        assert_eq!(t.tier(NetId(1)), Tier::Full);
        assert_eq!(t.tier(NetId(2)), Tier::Reduced);
    }

    #[test]
    fn waking_is_rationed_nearest_first() {
        let cfg = Config {
            max_wakes: 3,
            ..Config::default()
        };
        let mut t = Tiers::new(cfg);
        let things: Vec<Subject> = (0..10).map(|i| at(i, 100.0 + i as f64)).collect();
        let p = t.plan(0, &[hero(0.0)], &things);
        assert_eq!(p.woke.len(), 3);
        assert_eq!(p.wakes_deferred, 7);
        assert_eq!(t.tier(NetId(0)), Tier::Full);
        assert_eq!(t.tier(NetId(9)), Tier::Dormant);
        // The rest follow on the next ticks.
        let p = t.plan(1, &[hero(0.0)], &things);
        assert_eq!(p.woke.len(), 3);
        for tick in 2..6 {
            t.plan(tick, &[hero(0.0)], &things);
        }
        assert_eq!(t.tier(NetId(9)), Tier::Full);
    }

    #[test]
    fn a_woken_thing_reports_how_long_it_slept() {
        let mut t = Tiers::default();
        t.plan(100, &[hero(0.0)], &[at(1, 500.0)]);
        // The hero leaves: the thing falls dormant, last run at tick 100.
        t.plan(101, &[hero(5000.0)], &[at(1, 500.0)]);
        assert_eq!(t.tier(NetId(1)), Tier::Dormant);
        let p = t.plan(400, &[hero(0.0)], &[at(1, 500.0)]);
        assert_eq!(p.woke, vec![(NetId(1), 300)]);
        // The first step after waking is one ordinary tick.
        assert_eq!(p.run, vec![(NetId(1), 1)]);
    }

    #[test]
    fn things_that_vanish_leave_the_books() {
        let mut t = Tiers::default();
        t.plan(0, &[hero(0.0)], &[at(1, 10.0), at(2, 10.0)]);
        t.plan(1, &[hero(0.0)], &[at(2, 10.0)]);
        assert_eq!(t.tier(NetId(1)), Tier::Dormant);
        assert_eq!(t.slots.len(), 1);
    }
}
