//! The balance run: every power fights the standard targets, headless, and
//! the results are compared against what its tier should be worth.
//!
//! A bot plays a hero the way a careful player would (target the nearest
//! villain, keep at range, use every power it has when it helps). For each
//! power it fights the same standard groups with Strike plus that one
//! power, and the run measures how much faster the fight is won and how
//! much less it hurts than with Strike alone. A power worth far less or far
//! more than its tier's budget is an outlier. Movement powers are measured
//! by how fast they cross a block instead.
//!
//! Run it with `cargo run --release -p hw-sim --example balance`; the test
//! `tests/powers.rs` keeps it green.

use crate::collision::Obstacles;
use crate::combat::{power_spec, COMBAT_SECONDS, HEALTH_REGEN, HERO_HEALTH};
use crate::progress::{self, is_movement, tier};
use crate::{Sim, GROUND_Y, TICK_HZ};
use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_net::{InputState, OwnedPower, Power, Sheet, Status};
use std::sync::Arc;

/// Can `hero` use `p` now (owned, recharged, energy enough)?
fn ready(sim: &mut Sim, hero: NetId, p: Power) -> bool {
    let f = sim.fighter(hero).expect("hero");
    sim.tick >= f.ready[(p.code() - 1) as usize] && f.energy >= power_spec(p).cost && f.kit.has(p)
}

/// A careful player's input for `hero` at `tick`: fight the nearest
/// villain with every power it has; with none about, walk to `goal`.
pub fn bot_input(sim: &mut Sim, hero: NetId, tick: u64, goal: Option<DVec3>) -> InputState {
    let me = sim.position(hero).expect("hero");
    let f = sim.fighter(hero).expect("hero");
    let mut input = InputState::default();
    if f.status == Status::Down {
        // Nobody to revive us: the medical drone.
        input.hospital = true;
        return input;
    }
    let foes: Vec<_> = sim
        .enemies()
        .into_iter()
        .filter(|(_, _, _, e)| matches!(e.status, Status::Standing | Status::Knocked))
        .collect();
    let nearest = foes
        .iter()
        .min_by(|a, b| a.2.distance(me).total_cmp(&b.2.distance(me)))
        .copied();
    let Some((target, _, at, foe)) = nearest else {
        if let Some(g) = goal {
            let d = DVec2::new(g.x - me.x, g.z - me.z);
            if d.length() > 2.0 {
                let d = d.normalize();
                input.move_x = d.x as f32;
                input.move_z = d.y as f32;
                input.jump = f.kit.has(Power::SuperJump) && tick.is_multiple_of(30);
            }
        }
        return input;
    };
    input.target = Some(target);
    let d = at.distance(me);
    let flat = DVec2::new(at.x - me.x, at.z - me.z).normalize_or_zero();
    let aim = (at - me).normalize_or_zero();
    input.aim = aim.as_vec3().to_array();
    let close = foes.iter().filter(|x| x.2.distance(me) < 7.0).count();
    let strike_range = power_spec(Power::Strike).range;
    let wants_close =
        (ready(sim, hero, Power::Shockwave) || ready(sim, hero, Power::Nova)) && foes.len() >= 2;
    // Holds and shields do no damage of their own: without a ranged
    // attack the hero fights up close.
    // (A meteor calls down a blast but is no way to keep fighting from afar.)
    let ranged = [Power::Bolt, Power::Lift, Power::Beam]
        .into_iter()
        .any(|p| f.kit.has(p));
    // Close in to fighting range; melee-only heroes go right up.
    let keep = if wants_close || !ranged || f.energy < 15.0 {
        strike_range - 0.5
    } else {
        14.0
    };
    if d > keep {
        input.move_x = flat.x as f32;
        input.move_z = flat.y as f32;
    }
    // Press a key every few ticks, as people do.
    if !tick.is_multiple_of(4) {
        return input;
    }
    let shielded = f.shielded(tick);
    let hurt = f.health < f.max_health * 0.6;
    let nova_close = foes.iter().filter(|x| x.2.distance(me) < 12.0).count();
    if f.carrying.is_some() {
        input.power = Power::Lift.code();
    } else if ready(sim, hero, Power::Mend) && hurt {
        input.power = Power::Mend.code();
    } else if ready(sim, hero, Power::Nova) && nova_close >= 3 {
        input.power = Power::Nova.code();
    } else if ready(sim, hero, Power::Meteor) && d <= 40.0 {
        input.power = Power::Meteor.code();
    } else if ready(sim, hero, Power::Beam) && d <= 30.0 {
        input.power = Power::Beam.code();
    } else if ready(sim, hero, Power::Shield) && !shielded && d < 25.0 {
        input.power = Power::Shield.code();
    } else if ready(sim, hero, Power::Shockwave) && close >= 2 {
        input.power = Power::Shockwave.code();
    } else if ready(sim, hero, Power::Hold) && !foe.held(tick) && d <= 25.0 {
        input.power = Power::Hold.code();
    } else if ready(sim, hero, Power::Lift) && d <= 25.0 {
        input.power = Power::Lift.code();
    } else if d <= strike_range - 0.3 && (f.energy < 40.0 || !f.kit.has(Power::Bolt)) {
        input.smash = true;
    } else if ready(sim, hero, Power::Bolt) {
        input.blast = true;
    } else {
        input.smash = true;
    }
    input
}

/// How one fight went.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FightResult {
    pub won: bool,
    pub seconds: f64,
    /// Health lost, as a share of the hero's health (several heroes'
    /// worth if they went down).
    pub hurt: f64,
}

/// A hero of `level` with Strike and `powers`.
pub fn hero_sheet(level: u8, powers: &[Power]) -> Sheet {
    let mut s = progress::new_hero(0);
    s.lifetime = progress::karma_for_level(level);
    s.karma = 0;
    s.powers = powers
        .iter()
        .map(|&power| OwnedPower {
            power,
            theme: 0,
            paid: 0,
        })
        .collect();
    s
}

/// One hero of `level` with `powers` against a standard group at their
/// level, on open ground. `seed` varies the group.
pub fn standard_fight(seed: u64, level: u8, powers: &[Power]) -> FightResult {
    let mut sim = Sim::with_solids(Seed(seed), Arc::new(Obstacles::flat(GROUND_Y)));
    sim.set_spawn((DVec2::ZERO, DVec2::X));
    let hero = sim.spawn_hero_with(
        hw_net::PlayerId(1),
        DVec3::new(0.0, GROUND_Y, 0.0),
        hero_sheet(level, powers),
    );
    sim.step();
    sim.add_fight_spot(DVec3::new(30.0, GROUND_Y, 0.0), DVec2::new(-1.0, 0.0));
    let limit = TICK_HZ as u64 * 180;
    let mut hurt = 0.0;
    let mut last = sim.fighter(hero).expect("hero").health;
    let max = sim.fighter(hero).expect("hero").max_health;
    for t in 0..limit {
        let input = bot_input(&mut sim, hero, t, None);
        sim.set_input(hw_net::PlayerId(1), input);
        sim.step();
        let f = sim.fighter(hero).expect("hero");
        if f.health < last {
            hurt += (last - f.health) as f64 / max as f64;
        } else {
            // Health a power gave back (Mend, Vitality) makes up for blows
            // taken; ordinary regeneration does not count.
            let calm =
                sim.tick.saturating_sub(f.last_combat) > (COMBAT_SECONDS * TICK_HZ as f64) as u64;
            let regen = if calm { HEALTH_REGEN.1 } else { HEALTH_REGEN.0 };
            let natural = regen * f.max_health / HERO_HEALTH * crate::DT as f32;
            let given = (f.health - last - natural).max(0.0);
            hurt -= given as f64 / max as f64;
        }
        last = f.health;
        if sim.enemies().is_empty() {
            return FightResult {
                won: true,
                seconds: t as f64 / TICK_HZ as f64,
                hurt: hurt.max(0.0),
            };
        }
    }
    FightResult {
        won: false,
        seconds: limit as f64 / TICK_HZ as f64,
        hurt: hurt.max(0.0),
    }
}

/// What a power is worth against the standard targets, next to Strike alone.
#[derive(Clone, Debug, PartialEq)]
pub struct PowerResult {
    pub power: Power,
    pub tier: u8,
    /// Mean seconds to win and mean health lost, over the seeds.
    pub seconds: f64,
    pub hurt: f64,
    /// Fights lost (not won in three minutes, or the hero carried off).
    pub lost: usize,
    /// How much better than Strike alone (1 = no better): the product of
    /// how much faster and how much less hurt.
    pub worth: f64,
    /// For movement powers: metres per second across a block with a house
    /// in the way.
    pub travel: f64,
}

/// The lowest level the run fights at (tiers 1 to 4 open); a power of a
/// higher tier fights at the level its tier opens.
pub const BALANCE_LEVEL: u8 = 8;

/// The level `p` is measured at.
pub fn balance_level(p: Power) -> u8 {
    progress::tier_level(tier(p)).max(BALANCE_LEVEL)
}
/// Seeds per power (each a different group).
pub const BALANCE_SEEDS: u64 = 6;

/// What a power of each tier should be worth: (least, most).
pub fn budget(tier: u8) -> (f64, f64) {
    match tier {
        1 => (1.15, 2.6),
        2 => (1.25, 3.2),
        3 => (1.35, 3.8),
        4 => (1.45, 4.4),
        5 => (1.55, 5.0),
        6 => (1.65, 5.6),
        _ => (1.75, 6.5),
    }
}

fn average(results: &[FightResult]) -> (f64, f64, usize) {
    let n = results.len() as f64;
    (
        results.iter().map(|r| r.seconds).sum::<f64>() / n,
        results.iter().map(|r| r.hurt).sum::<f64>() / n,
        results.iter().filter(|r| !r.won).count(),
    )
}

/// Seconds Strike alone takes at `level` and the health it costs, as the
/// yardstick.
pub fn baseline(level: u8) -> (f64, f64) {
    let r: Vec<_> = (0..BALANCE_SEEDS)
        .map(|s| standard_fight(100 + s, level, &[]))
        .collect();
    let (s, h, _) = average(&r);
    (s, h)
}

/// How fast a hero with `powers` crosses 120 m with a two-storey house in
/// the way (m/s).
pub fn travel_speed(powers: &[Power]) -> f64 {
    use crate::collision::Solid;
    let mut solids = Obstacles::flat(GROUND_Y);
    solids.add(Solid {
        origin: DVec2::ZERO,
        along: DVec2::X,
        inward: DVec2::Y,
        min: DVec2::new(50.0, -40.0),
        max: DVec2::new(62.0, 40.0),
        y0: 0.0,
        y1: 6.0,
    });
    let mut sim = Sim::with_solids(Seed(1), Arc::new(solids));
    let hero = sim.spawn_hero_with(
        hw_net::PlayerId(1),
        DVec3::new(0.0, GROUND_Y, 0.0),
        hero_sheet(BALANCE_LEVEL, powers),
    );
    let flies = powers.contains(&Power::Flight);
    let limit = TICK_HZ as u64 * 90;
    for t in 0..limit {
        let at = sim.position(hero).expect("hero");
        if at.x >= 120.0 {
            return 120.0 / (t as f64 / TICK_HZ as f64);
        }
        let blocked = at.x > 40.0 && at.x < 62.0;
        let mut input = InputState {
            move_x: 1.0,
            ..Default::default()
        };
        if flies {
            input.fly = true;
            input.jump = at.y < 9.0;
        } else if powers.contains(&Power::SuperJump) {
            // Bound along, over the house.
            input.jump = true;
        } else if blocked {
            // Walk round the house.
            {
                input.move_x = if at.z > -42.0 { 0.0 } else { 1.0 };
                input.move_z = -1.0;
            }
        }
        if !flies && !blocked && at.z < -0.5 && at.x > 62.0 {
            input.move_z = 0.0;
        }
        sim.set_input(hw_net::PlayerId(1), input);
        sim.step();
    }
    0.0
}

/// The whole run: every power.
pub fn run() -> Vec<PowerResult> {
    let mut bases: std::collections::BTreeMap<u8, (f64, f64)> = Default::default();
    let walk = travel_speed(&[]);
    Power::ALL
        .into_iter()
        .filter(|&p| p != Power::Strike)
        .map(|p| {
            if is_movement(p) {
                let travel = travel_speed(&[p]);
                return PowerResult {
                    power: p,
                    tier: tier(p),
                    seconds: 0.0,
                    hurt: 0.0,
                    lost: 0,
                    worth: travel / walk,
                    travel,
                };
            }
            let level = balance_level(p);
            let (base_s, base_h) = *bases.entry(level).or_insert_with(|| baseline(level));
            let r: Vec<_> = (0..BALANCE_SEEDS)
                .map(|s| standard_fight(100 + s, level, &[p]))
                .collect();
            let (seconds, hurt, lost) = average(&r);
            let worth = (base_s / seconds) * ((base_h + 0.1) / (hurt + 0.1));
            PowerResult {
                power: p,
                tier: tier(p),
                seconds,
                hurt,
                lost,
                worth,
                travel: 0.0,
            }
        })
        .collect()
}

/// The powers that fall outside their tier's budget, with why.
pub fn outliers(results: &[PowerResult]) -> Vec<String> {
    let mut out = Vec::new();
    for r in results {
        let name = power_spec(r.power).name;
        if is_movement(r.power) {
            // Each movement power must beat the one below it.
            if r.worth < 1.3 {
                out.push(format!(
                    "{name} is barely faster than running ({:.2}x)",
                    r.worth
                ));
            }
            continue;
        }
        let (lo, hi) = budget(r.tier);
        if r.lost > 0 {
            out.push(format!("{name} lost {} standard fights", r.lost));
        }
        if r.worth < lo {
            out.push(format!(
                "{name} (tier {}) is worth {:.2}, under {lo}",
                r.tier, r.worth
            ));
        }
        if r.worth > hi {
            out.push(format!(
                "{name} (tier {}) is worth {:.2}, over {hi}",
                r.tier, r.worth
            ));
        }
    }
    if let (Some(j), Some(f)) = (
        results.iter().find(|r| r.power == Power::SuperJump),
        results.iter().find(|r| r.power == Power::Flight),
    ) {
        if f.travel <= j.travel {
            out.push("Flight is no faster than Super Jump".into());
        }
    }
    if let (Some(j), Some(s)) = (
        results.iter().find(|r| r.power == Power::SuperJump),
        results.iter().find(|r| r.power == Power::SuperSpeed),
    ) {
        if s.travel <= j.travel {
            out.push("Super Speed is no faster than Super Jump".into());
        }
    }
    out
}
