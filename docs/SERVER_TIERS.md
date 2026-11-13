# Server tiers: who is simulated how often

Status: scheduler built (`crates/hw-sim/src/tiers.rs`, tested) and driving villains since 0.257
(see build order). Decided with Arnd on 2026-11-13.

## Goal

The world around the heroes stays responsive however much happens elsewhere, and this holds
**by construction**: the work of a tick is bounded by counts the code enforces, not by the
machine being fast enough on the cases we tried.

## What exists today (checked in the code, 0.254)

- One server world (`Sim`, one bevy `World`), one thread (single player: `hostthread.rs`;
  dedicated: `hw-headless`). One `Sim::step` per tick, 30 Hz.
- Already "near heroes only": people, traffic, taxis and incidents live in a 6 km `Area` that
  follows the heroes; villains go to sleep when no hero is within `combat::SLEEP` (420 m);
  loose physics bodies outside a bubble hold still; bubbles carry an entity budget.
- Bounded work, by wall clock or by ad-hoc caps: `THINK_CAP` (400 villains think per tick, the
  rest take turns), one path search per tick, `TICK_BUDGET_US` (20 ms: vandalism and housekeeping
  are put off).

What is missing is one place that decides, for every thing, *how often it steps*, with a hard
count behind it.

## The model

Every simulated thing (a villain, a car, a body, a robot) is in **exactly one tier** at any time:

| Tier | Steps | Who |
| --- | --- | --- |
| Full | every tick | within 1 km of a hero, or pinned (see below) |
| Reduced | every 2nd tick (stride is a constant) | 1 to 2 km from a hero |
| Dormant | never; holds still | farther than 2 km |

The tier is decided in one function (`Tiers::plan`), from three things:

1. **Distance** to the nearest hero. Sets the *order* in which things are let into a tier.
2. **Pins.** A thing doing something that cannot wait (fighting a hero, carrying someone, in a
   lift, a colossus walking) is pinned Full whatever its distance. This is the "flag" of the
   proposal: a thing asks to stay near, and releases itself by dropping the pin.
3. **Budgets.** Each tier has a hard count. When a tier is full, the things farthest from the
   heroes (pinned ones first in line) drop one tier. So a crowd can make things slower in the
   outer ring, but can never make a tick longer.

Around that:

- **Hysteresis:** enter at 1.0 km, stay until 1.1 km (same at 2 km), so nothing flickers.
- **Staggered turns:** a Reduced thing steps when `(id + tick)` is a multiple of the stride, so
  every tick runs the same share (no heavy tick, light tick, heavy tick).
- **Look-ahead:** a hero's velocity moves the "near" test ahead by 3 s, so a boosted hero
  (60 m/s) does not fly into a sleeping town.
- **Rationed waking:** waking a thing is work, so at most `max_wakes` per tick wake, nearest first.
- **Several heroes:** distance is to the nearest hero; budgets are shared, so ten heroes spread
  over the planet cost the same as ten together.
- **No hidden state:** the plan depends only on the inputs and the previous tiers, not on the
  order things are listed or on the wall clock, so tests can pin it down.

The numbers in `tiers::Config` (800 Full, 1600 Reduced, 40 wakes per tick, 3 s look-ahead) are
starting values, to be set from measurements.

### What is not in the tiers

- The 6 km `Area` stays: it is generated plan data (streets, building sites), not simulation,
  and costs nothing per tick. Only the *simulated* radii shrink.
- Engine's `PhysicsWorld` steps all its bodies together, so Reduced does not apply to it: a
  body is awake (Full) or frozen (Dormant), as with bubbles today.
- Dormant is not "catching up". Nothing is simulated beyond 2 km; things come back from the seed,
  the clock and saved changes (citizens already work like that).

## The real work: stepping by elapsed time

A Reduced thing steps every 2nd tick, and has to move at normal speed, so each step is handed
the time since its last one (`Plan::run` gives `(id, ticks)`). Systems that assume one tick per
call would run at half speed. Audit of what is time-based and what is tick-based:

| System | Today | Needed |
| --- | --- | --- |
| Traffic (`traffic.rs`, `advance_to(now)`) | by clock | nothing |
| Citizens (`citizens.rs`) | pure function of time | nothing |
| Taxis, tube (`step(.., dt)`), airships, rocket, lifts | by clock or `dt` | check dt limits for big steps |
| Combat (`combat.rs`, 5,400 lines, ~450 uses of `tick`) | per tick: cooldowns and deadlines are absolute ticks, movement uses `DT` | pass elapsed ticks, sub-step movement, keep deadlines absolute |
| Robots (`robots.rs`, ~110 uses of `tick`) | per tick | same |
| Incidents (~90), crimes (~25), alarms, NPC heroes, sidekicks | per tick, some every 6th or 30th tick | same, or leave them Full (few things) |

Two findings from the audit:

1. **`combat::enemies_act` already does a version of Reduced** (`THINK_CAP`: with more than 400
   villains each acts every `stride`th tick), but its comment says "a huge crowd moves slower
   rather than stalling": exactly the half-speed problem. It is the first thing to convert, and
   the first test to write (a villain on stride 2 covers the same distance as on stride 1).
2. Combat collects **every** fighter into one list per tick and runs its phases over the whole
   list. For tiers, it must gather only what the plan says runs, and take the heroes' view of the
   rest (who is near enough to be hit or seen) from the unscheduled list.

Absolute deadlines (`busy_until`, `ready_at`, status ends) are in ticks. A dormant thing's world
is frozen, so on waking its deadlines move on by the time it slept (`Plan::woke` reports it). The
rule: a thing is never changed by time it was not simulated.

## Build order

1. **Scheduler** (done): `tiers.rs`, 11 tests: rings, hysteresis, pins, budget guarantee with
   50,000 things and ten heroes, order independence, turns, look-ahead, waking, sleep time.
2. **Wire it in** (started 0.256: `enemies_act` walks the ticks a villain missed, `Enemy::last_think`, test `villain_speed.rs`; driven by the plan since 0.257: 400 Full, 200 Reduced, `combat::ACT_CAP` per tick; tier counts in the overrun log 0.258) for one system at a time, behind the plan: first `enemies_act` (replace
   `THINK_CAP`), then the rest of combat, robots, incidents. Each conversion comes with a test
   that the thing moves the same distance per second on every stride.
3. **Counts in the hitch log:** per tier counts and deferred wakes next to the part timings, so a
   slow stretch shows which ring is full.
4. **Worst-case tests:** a crowd of 3,000 villains downtown, a boosted hero crossing the town,
   ten heroes apart; assert the work per tick stays under the caps and nothing near a hero is
   put off.
5. **Retire the ad-hoc caps** (`THINK_CAP`, wall-clock deferral) once their work is covered by
   budgets.

## Open questions

- Which tier-1 radius for the heaviest work (combat AI)? 1 km may be more than it needs, while
  movement and physics want the full radius.
- Pins: what exactly pins a thing? Proposal: in combat with a hero within 150 m, carried, in a
  lift or vehicle, part of an open incident a hero has answered.
- Replication: the client interest radius (500 m) is inside Full; Reduced things reach clients at
  half the rate and are interpolated. To check against the bandwidth budget.

## Status 2026-11-13 (0.258)

Done: scheduler; villains (`enemies_act`) run by the plan with elapsed-tick walking and deadline
shift on wake; per-tier counts in the overrun log; tests `villain_speed.rs` and the
thousand-villain test in `many_villains.rs`.

Done 0.260: (a) colossus and weak points are pinned Full (`robots::act` steps them every tick).

Left, in order: (a) done, (b) incidents and crimes (bounded by the 6 km area and a few dozen things: keep
Full unless measured otherwise), (c) a boosted hero crossing town test for wake rationing,
(d) retire the wall-clock deferral (`TICK_BUDGET_US`) once (a) to (c) are in, (e) the open
questions above, with Arnd.

## Decisions (Arnd, 2026-11-13)

- Villain AI keeps the 1 km first ring, capped at 400 (distance only decides who is dropped first).
- Pins: a thing with a target or a windup, the colossus and its weak points, plus (when those
  systems join the plan) things carrying someone, in a lift or vehicle, and incidents a hero has
  answered.
- Replication: the 500 m interest radius stays inside the first ring; the 1 to 2 km ring gets no
  extra bandwidth.
