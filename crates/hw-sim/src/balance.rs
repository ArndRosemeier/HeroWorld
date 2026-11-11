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
use crate::combat::{
    EnemyKind, BOMB_AREA, COMBAT_SECONDS, HEALTH_REGEN, HERO_HEALTH, SHIELD_REACH,
};
use crate::powers::{self, def_of, travel, Delivery, Tree};
use crate::progress;
use crate::{Sim, GROUND_Y, TICK_HZ};
use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_net::{InputState, OwnedPower, PowerId, Sheet, Status};
use std::sync::Arc;

/// Can `hero` use `p` now (owned, recharged, energy enough)?
fn ready(sim: &mut Sim, hero: NetId, p: PowerId) -> bool {
    let f = sim.fighter(hero).expect("hero");
    f.kit.has(p) && sim.tick >= f.ready[p.0 as usize] && f.energy >= powers::energy(def_of(p))
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
        // A hypnotised villain fights on our side for now.
        .filter(|(_, _, _, e)| {
            matches!(e.status, Status::Standing | Status::Knocked) && !e.charmed(tick)
        })
        .collect();
    // Whom to fight: an orderly first (it mends the rest), then the
    // nearest; a shield bearer only when nothing else is left or it is
    // right there (blows from afar glance off its shield).
    // The colossus's hull shrugs blows off until it kneels: its weak
    // points first.
    let rank = |k: EnemyKind, d: f64, e: &crate::combat::Fighter| match k {
        EnemyKind::Orderly if d < 16.0 => 0,
        EnemyKind::Bulwark if d > SHIELD_REACH => 2,
        EnemyKind::Colossus if e.marks & hw_net::EFFECT_KNEELING == 0 => 2,
        _ => 1,
    };
    let nearest = foes
        .iter()
        .min_by(|a, b| {
            let (da, db) = (a.2.distance(me), b.2.distance(me));
            rank(a.1, da, &a.3)
                .cmp(&rank(b.1, db, &b.3))
                .then(da.total_cmp(&db))
        })
        .copied();
    let Some((target, kind, at, foe)) = nearest else {
        if let Some(g) = goal {
            let d = DVec2::new(g.x - me.x, g.z - me.z);
            if d.length() > 2.0 {
                let d = d.normalize();
                input.move_x = d.x as f32;
                input.move_z = d.y as f32;
                input.jump = f.kit.travels(travel::JUMP) && tick.is_multiple_of(30);
            }
        }
        return input;
    };
    input.target = Some(target);
    let d = at.distance(me);
    let flat = DVec2::new(at.x - me.x, at.z - me.z).normalize_or_zero();
    let aim = (at - me).normalize_or_zero();
    input.aim = aim.as_vec3().to_array();
    let strike_range = def_of(powers::STRIKE).fields.range;
    // The hero's active powers, by mechanic.
    let owned: Vec<&powers::PowerDef> = f
        .kit
        .owned()
        .map(def_of)
        .filter(|d| !d.passive() && !powers::is_free(d.id))
        .collect();
    let of = |want: Delivery| owned.iter().filter(move |d| d.delivery() == want);
    // Up close only while a burst is ready to go off there.
    let ready_now =
        |d: &powers::PowerDef| tick >= f.ready[d.id.0 as usize] && f.energy >= powers::energy(d);
    let wants_close =
        of(Delivery::Burst).any(|d| d.fields.damage > 0.0 && ready_now(d)) && foes.len() >= 2;
    // Holds and shields do no damage of their own: without a ranged
    // attack the hero fights up close.
    let ranged = owned.iter().any(|d| {
        matches!(
            d.delivery(),
            Delivery::Ranged | Delivery::Grab | Delivery::Line
        ) && d.fields.damage >= 20.0
    });
    // Close in to fighting range; melee-only heroes go right up.
    // One out of sight (round a corner, behind a wall) cannot be shot:
    // walk up to it.
    let chest = DVec3::Y * (crate::collision::BODY_HEIGHT * 0.6);
    let seen = crate::combat::in_sight(&sim.solids(), me + chest, at + chest);
    let keep = if wants_close || !ranged || f.energy < 15.0 || !seen {
        strike_range - 0.5
    } else {
        14.0
    };
    if d > keep {
        input.move_x = flat.x as f32;
        input.move_z = flat.y as f32;
    }
    // A bomb coming down here: out of its ring first.
    let bombs = &sim.fights().bombs;
    // People notice a bomb late: the last moment before it bursts.
    if let Some(b) = bombs.iter().find(|b| {
        b.land <= tick + (TICK_HZ as f64 * 0.6) as u64
            && DVec2::new(me.x - b.at.x, me.z - b.at.z).length() < BOMB_AREA + 0.8
    }) {
        let away = DVec2::new(me.x - b.at.x, me.z - b.at.z).normalize_or(DVec2::X);
        input.move_x = away.x as f32;
        input.move_z = away.y as f32;
    }
    // Press a key every few ticks, as people do.
    if !tick.is_multiple_of(4) {
        return input;
    }
    let shielded = f.shielded(tick) || f.buffed(tick);
    let hurt = f.health < f.max_health * 0.6;
    let ids: Vec<(PowerId, Delivery, powers::Fields)> = owned
        .iter()
        .map(|d| (d.id, d.delivery(), d.fields))
        .collect();
    let mut pick = None;
    let mut bind = None;
    if f.carrying.is_some() {
        pick = Some(f.lifted_with);
    }
    // Toggles go on first, and stay on.
    if pick.is_none() {
        pick = owned
            .iter()
            .find(|d| d.toggle && !f.kit.is_on(d.id) && ready(sim, hero, d.id))
            .map(|d| d.id);
    }
    for &(id, how, fl) in &ids {
        if pick.is_some() {
            break;
        }
        if !ready(sim, hero, id) {
            continue;
        }
        let near = |r: f64| foes.iter().filter(|x| x.2.distance(me) < r).count();
        let near_foe = |r: f64| foes.iter().filter(|x| x.2.distance(at) < r).count();
        let good = match how {
            Delivery::Allies if fl.heal > 0.0 => hurt,
            Delivery::Burst if fl.radius >= 12.0 => near(fl.radius * 0.75) >= 3,
            Delivery::Burst => {
                near(fl.radius * 0.85) >= 2 || (fl.damage <= 0.0 && near(fl.radius) >= 1)
            }
            // A strike from the sky that only holds: on a group that can
            // be held, not on a machine.
            Delivery::Sky if fl.damage < 20.0 => {
                d <= fl.range && !kind.heavy() && near_foe(fl.radius) >= 2 && !foe.held(tick)
            }
            Delivery::Sky => d <= fl.range,
            Delivery::Line => d <= fl.range - 6.0,
            Delivery::Summon => d <= 40.0,
            Delivery::Ranged if fl.charm > 0.0 => {
                foes.len() >= 2 && d <= fl.range - 2.0 && crate::combat::charmable(kind)
            }
            Delivery::Cone => d <= fl.range - 2.0 && !foe.frozen(tick),
            Delivery::Allies => !shielded && d < 25.0,
            Delivery::Ranged if fl.hold > 0.0 && fl.damage < 20.0 => {
                // Bind the nearest one that hits hard, not a medic.
                bind = foes
                    .iter()
                    .filter(|x| !x.3.held(tick) && x.2.distance(me) <= fl.range - 5.0)
                    .filter(|x| !matches!(x.1, EnemyKind::Orderly | EnemyKind::Grenadier))
                    .min_by(|a, b| a.2.distance(me).total_cmp(&b.2.distance(me)))
                    .map(|x| x.0);
                bind.is_some()
            }
            Delivery::Grab => d <= 25.0,
            Delivery::Ranged => Some(id) != f.kit.blast && d <= fl.range - 2.0,
            Delivery::Melee => d <= fl.range - 0.3,
            Delivery::Blink if fl.damage > 0.0 => d > 5.0 && d <= fl.range,
            _ => false,
        };
        if good {
            pick = Some(id);
        }
    }
    if let Some(p) = pick {
        if let Some(b) = bind.filter(|_| def_of(p).fields.hold > 0.0) {
            input.target = Some(b);
        }
        input.power = p.0;
    } else if d <= strike_range - 0.3 && (f.energy < 40.0 || f.kit.blast.is_none()) {
        input.smash = true;
    } else if f.kit.blast.is_some_and(|b| ready(sim, hero, b)) {
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

/// A hero of `level` with Strike and `powers` (of one origin's tree and
/// the travel tree), the element `Element::None` so only the powers count.
pub fn hero_sheet(level: u8, powers: &[PowerId]) -> Sheet {
    let tree = powers
        .iter()
        .map(|&p| def_of(p).tree)
        .find(|t| Tree::ORIGIN_TREES.contains(t))
        .unwrap_or(Tree::Science);
    let origin = Tree::ORIGIN_TREES
        .iter()
        .position(|&t| t == tree)
        .unwrap_or(0) as u8;
    let mut s = progress::new_hero(origin, powers::Element::None);
    s.lifetime = progress::karma_for_level(level);
    s.karma = 0;
    s.powers = powers
        .iter()
        .map(|&power| OwnedPower { power, paid: 0 })
        .collect();
    s
}

/// One hero of `level` with `powers` against a standard group at their
/// level, on open ground. `seed` varies the group.
pub fn standard_fight(seed: u64, level: u8, powers: &[PowerId]) -> FightResult {
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
    pub power: PowerId,
    /// The budget it is held to (1 for the first powers, 7 for the last).
    pub tier: u8,
    /// Mean seconds to win and mean health lost, over the seeds.
    pub seconds: f64,
    pub hurt: f64,
    /// Fights lost (not won in three minutes, or the hero carried off).
    pub lost: usize,
    /// How much better than Strike alone (1 = no better): the product of
    /// how much faster and how much less hurt.
    pub worth: f64,
    /// For travel powers: metres per second across a block with a house
    /// in the way.
    pub travel: f64,
}

/// The lowest level the run fights at; a power further down its tree
/// fights at the level a hero usually has by then.
pub const BALANCE_LEVEL: u8 = 8;
/// About the level a hero buys each row's powers at.
pub const ROW_LEVELS: [u8; 6] = [1, 3, 5, 8, 12, 16];

/// The powers of before the trees, re-expressed as tree powers, with the
/// tier each had: the run holds these to their old budgets, so the
/// redesign keeps what they did. (Energy Bolt, Super Jump, Binding Hold,
/// Force Shield, Lift and Throw, Flight, Shockwave, Super Speed, Mend,
/// Beam, Vitality, Meteor, Invulnerable, Nova, Rally, Freeze Ray,
/// Teleport, Rescue Bubble, Robot Sidekick, Hypno Ray.)
pub const CLASSIC: [(PowerId, u8); 20] = [
    (PowerId(10), 1),
    (PowerId(2), 1),
    (PowerId(13), 2),
    (PowerId(15), 2),
    (PowerId(27), 2),
    (PowerId(5), 3),
    (PowerId(17), 3),
    (PowerId(6), 4),
    (PowerId(20), 4),
    (PowerId(21), 5),
    (PowerId(52), 5),
    (PowerId(26), 6),
    (PowerId(74), 6),
    (PowerId(30), 7),
    (PowerId(94), 7),
    (PowerId(16), 2),
    (PowerId(14), 3),
    (PowerId(19), 3),
    (PowerId(18), 4),
    (PowerId(23), 5),
];

/// The level `p` is measured at.
pub fn balance_level(p: PowerId) -> u8 {
    ROW_LEVELS[def_of(p).row.min(5) as usize].max(BALANCE_LEVEL)
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
pub fn travel_speed(powers: &[PowerId]) -> f64 {
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
    let kit = powers::Kit::with(powers);
    let flies = kit.travels(travel::FLY);
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
        } else if kit.travels(travel::JUMP) {
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

/// Is `p` a travel power (measured by how fast it crosses a block)?
pub fn is_movement(p: PowerId) -> bool {
    let f = def_of(p).fields;
    def_of(p).passive()
        && f.travel & (travel::JUMP | travel::FLY | travel::SPEED | travel::SPRINT) != 0
}

/// Powers that are no help in a standard fight (getting about, rescues,
/// helping powers): measured, but held to no fight budget.
pub fn is_utility(p: PowerId) -> bool {
    let d = def_of(p);
    d.helper
        || matches!(d.delivery(), Delivery::Rescue)
        || (d.delivery() == Delivery::Blink && d.fields.damage <= 0.0)
        || (d.passive() && d.fields.travel != 0 && !is_movement(p))
}

/// The run over `list`: (power, the tier whose budget it is held to).
pub fn run_over(list: &[(PowerId, u8)]) -> Vec<PowerResult> {
    let mut bases: std::collections::BTreeMap<u8, (f64, f64)> = Default::default();
    let walk = travel_speed(&[]);
    list.iter()
        .map(|&(p, tier)| {
            if is_movement(p) {
                let travel = travel_speed(&[p]);
                return PowerResult {
                    power: p,
                    tier,
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
                tier,
                seconds,
                hurt,
                lost,
                worth,
                travel: 0.0,
            }
        })
        .collect()
}

/// The run that must stay green: the old powers, re-expressed.
pub fn run() -> Vec<PowerResult> {
    run_over(&CLASSIC)
}

/// Every tree power, each held to the budget of its row (balancing the new
/// powers is the next step; this is its report).
pub fn run_all() -> Vec<PowerResult> {
    let list: Vec<(PowerId, u8)> = powers::POWERS
        .iter()
        .filter(|d| !powers::is_free(d.id))
        .map(|d| (d.id, d.row + 1 + (d.row >= 4) as u8))
        .collect();
    run_over(&list)
}

/// The powers that fall outside their tier's budget, with why.
pub fn outliers(results: &[PowerResult]) -> Vec<String> {
    let mut out = Vec::new();
    for r in results {
        let name = def_of(r.power).name;
        // Teleport and the rescue bubble do not fight: nothing to weigh.
        if is_utility(r.power) {
            continue;
        }
        if is_movement(r.power) {
            // Each movement power must beat running.
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
    let travel = |id: u8| {
        results
            .iter()
            .find(|r| r.power == PowerId(id))
            .map(|r| r.travel)
    };
    if let (Some(j), Some(f)) = (travel(2), travel(5)) {
        if f <= j {
            out.push("Flight is no faster than Super Jump".into());
        }
    }
    if let (Some(j), Some(s)) = (travel(2), travel(6)) {
        if s <= j {
            out.push("Super Speed is no faster than Super Jump".into());
        }
    }
    out
}
