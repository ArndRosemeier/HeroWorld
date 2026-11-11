//! Levels 6 to 20, headless: the later powers do what they say,
//! and a new hero playing as a careful player would reaches the level cap
//! in about ten hours.

use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_net::{FxKind, InputState, PlayerId, PowerId, Status};
use hw_sim::balance::{bot_input, hero_sheet};
use hw_sim::collision::{Body, Obstacles};
use hw_sim::combat::{Fighter, METEOR_DELAY, RALLY_RADIUS};
use hw_sim::powers::{def_of, Tree};
use hw_sim::progress::{self, level_of, MAX_LEVEL};
use hw_sim::{Sim, GROUND_Y, TICK_HZ};
use std::sync::Arc;

fn arena(seed: u64) -> Sim {
    let mut sim = Sim::with_solids(Seed(seed), Arc::new(Obstacles::flat(GROUND_Y)));
    sim.set_spawn((DVec2::new(35.0, 35.0), DVec2::X));
    sim
}

fn ground(x: f64, z: f64) -> DVec3 {
    DVec3::new(x, GROUND_Y, z)
}

/// Presses `input` once (then only keeps the target) for `ticks` ticks;
/// returns every fight event seen.
fn press(sim: &mut Sim, input: InputState, ticks: u64) -> Vec<hw_net::Fx> {
    let mut seen = Vec::new();
    for k in 0..ticks {
        let i = if k == 0 {
            input
        } else {
            InputState {
                target: input.target,
                ..Default::default()
            }
        };
        sim.set_input(PlayerId(1), i);
        sim.step();
        seen.extend(sim.fx().iter().copied());
    }
    seen
}

fn use_power(p: PowerId, target: Option<NetId>) -> InputState {
    InputState {
        power: p.0,
        target,
        ..Default::default()
    }
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn passives_have_no_key() {
    // Pressing a passive power's code does nothing.
    let mut sim = arena(1);
    let h = sim.spawn_hero_with(
        PlayerId(1),
        ground(0.0, 0.0),
        hero_sheet(20, &[PowerId(113), PowerId(114)]),
    );
    sim.step();
    for p in [PowerId(113), PowerId(114)] {
        assert!(def_of(p).passive());
        press(&mut sim, use_power(p, None), 3);
        assert_eq!(sim.fighter(h).unwrap().energy, 100.0);
    }
}

/// A hero of `level` with `powers` and a group of villains `at` metres
/// down the street.
fn street(level: u8, powers: &[PowerId], at: f64) -> (Sim, NetId) {
    let mut sim = arena(9);
    let hero = sim.spawn_hero_with(PlayerId(1), ground(0.0, 0.0), hero_sheet(level, powers));
    sim.step();
    sim.add_fight_spot(ground(at, 0.0), DVec2::new(-1.0, 0.0));
    (sim, hero)
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn mend_heals_the_hero_and_friends_near() {
    let mut sim = arena(2);
    let a = sim.spawn_hero_with(PlayerId(1), ground(0.0, 0.0), hero_sheet(8, &[PowerId(20)]));
    let b = sim.spawn_hero_with(PlayerId(2), ground(5.0, 0.0), hero_sheet(8, &[]));
    let far = sim.spawn_hero_with(PlayerId(3), ground(60.0, 0.0), hero_sheet(8, &[]));
    sim.step();
    for id in [a, b, far] {
        let mut f: Fighter = sim.fighter(id).unwrap();
        f.health = f.max_health * 0.3;
        f.last_combat = sim.tick;
        sim.set_fighter(id, f);
    }
    let fx = press(&mut sim, use_power(PowerId(20), None), 2);
    let healed: Vec<NetId> = fx
        .iter()
        .filter(|e| e.kind == FxKind::Mend)
        .map(|e| e.to)
        .collect();
    assert!(healed.contains(&a) && healed.contains(&b));
    assert!(!healed.contains(&far));
    let mut share = |id| {
        let f = sim.fighter(id).unwrap();
        f.health / f.max_health
    };
    let (sa, sb, sf) = (share(a), share(b), share(far));
    assert!(sa > 0.55 && sb > 0.55, "{sa} {sb}");
    assert!(sf < 0.35);
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn a_beam_goes_through_every_villain_in_its_line() {
    // Three of the group stand one behind the other down the street: a beam
    // at the nearest goes through the two behind it, and misses the rest.
    let (mut sim, hero) = street(12, &[PowerId(21)], 20.0);
    let foes = sim.enemies();
    for (k, f) in foes.iter().enumerate() {
        let at = if k < 3 {
            ground(12.0 + 4.0 * k as f64, 0.0)
        } else {
            ground(14.0, 10.0 + 3.0 * k as f64)
        };
        sim.set_body(
            f.0,
            Body {
                pos: at,
                vel: DVec3::ZERO,
                grounded: true,
            },
        );
    }
    let first = foes[0].0;
    let fx = press(&mut sim, use_power(PowerId(21), Some(first)), 2);
    let hit: Vec<NetId> = fx
        .iter()
        .filter(|e| e.kind == FxKind::Beam && e.to != hero)
        .map(|e| e.to)
        .collect();
    assert_eq!(hit, foes[..3].iter().map(|f| f.0).collect::<Vec<_>>());
    assert!(fx
        .iter()
        .any(|e| e.kind == FxKind::Beam && e.to == hero && e.from == hero));
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn a_meteor_lands_a_moment_later_on_the_whole_group() {
    let (mut sim, hero) = street(16, &[PowerId(26)], 30.0);
    let target = sim.enemies()[0].0;
    let fx = press(&mut sim, use_power(PowerId(26), Some(target)), 2);
    assert!(fx.iter().any(|e| e.kind == FxKind::MeteorCall));
    let spent = 100.0 - sim.fighter(hero).unwrap().energy;
    assert!(
        spent >= hw_sim::powers::energy(hw_sim::powers::def_of(PowerId(26))) - 1.0,
        "{spent}"
    );
    assert!(!fx.iter().any(|e| e.kind == FxKind::Meteor), "not yet");
    let fx = press(
        &mut sim,
        InputState::default(),
        (METEOR_DELAY * TICK_HZ as f64) as u64 + 2,
    );
    let hit = fx
        .iter()
        .filter(|e| e.kind == FxKind::Meteor && e.to != hero)
        .count();
    assert!(hit >= 3, "caught {hit}");
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn a_nova_flattens_everyone_round_the_hero() {
    let (mut sim, hero) = street(20, &[PowerId(30)], 6.0);
    let fx = press(&mut sim, use_power(PowerId(30), None), 2);
    let hit = fx
        .iter()
        .filter(|e| e.kind == FxKind::Nova && e.to != hero)
        .count();
    assert!(hit >= 4, "caught {hit}");
    let standing = sim
        .enemies()
        .iter()
        .filter(|e| e.3.status == Status::Standing)
        .count();
    assert_eq!(standing, 0);
    assert!(sim.fighter(hero).unwrap().energy <= 41.0);
}

/// Health a level-16 hero loses to one brute's punches in `seconds`.
fn punished(powers: &[PowerId], seconds: f64) -> f32 {
    let (mut sim, hero) = street(16, powers, 3.0);
    // Toggles on first (set straight on: a key press would draw the
    // villains' attention, and the plain hero presses nothing).
    let mut f = sim.fighter(hero).unwrap();
    f.kit.set_toggles(u128::MAX);
    sim.set_fighter(hero, f);
    let before = sim.fighter(hero).unwrap();
    press(
        &mut sim,
        InputState::default(),
        (seconds * TICK_HZ as f64) as u64,
    );
    let after = sim.fighter(hero).unwrap();
    (before.health - after.health) / before.max_health
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn invulnerable_and_vitality_keep_a_hero_on_their_feet() {
    let plain = punished(&[], 4.0);
    let tough = punished(&[PowerId(74)], 4.0);
    let vital = punished(&[PowerId(52)], 4.0);
    eprintln!("VALS {plain} {tough} {vital}");
    assert!((0.2..0.9).contains(&plain), "the group hurts: {plain}");
    assert!(tough < plain * 0.6, "{tough} vs {plain}");
    // Two percent a second, four seconds.
    assert!(vital < plain - 0.06, "{vital} vs {plain}");
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn rally_makes_friends_near_hit_harder() {
    let mut sim = arena(4);
    let a = sim.spawn_hero_with(
        PlayerId(1),
        ground(0.0, 0.0),
        hero_sheet(20, &[PowerId(94)]),
    );
    let b = sim.spawn_hero_with(PlayerId(2), ground(8.0, 0.0), hero_sheet(20, &[]));
    let far = sim.spawn_hero_with(
        PlayerId(3),
        ground(RALLY_RADIUS + 30.0, 0.0),
        hero_sheet(20, &[]),
    );
    sim.step();
    // The aura is a toggle: on first.
    press(&mut sim, use_power(PowerId(94), None), 1);
    sim.step();
    let rallied = |sim: &mut Sim, id| sim.fighter(id).unwrap().rallied;
    assert!(rallied(&mut sim, a) && rallied(&mut sim, b));
    assert!(!rallied(&mut sim, far));
    let view = sim.fighter(b).unwrap().view(sim.tick);
    assert!(view.effects & hw_net::EFFECT_RALLIED != 0);
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn super_speed_outruns_everyone_on_foot() {
    let run = |powers: &[PowerId], boost: bool| {
        let mut sim = arena(5);
        let h = sim.spawn_hero_with(PlayerId(1), ground(0.0, 0.0), hero_sheet(8, powers));
        for _ in 0..TICK_HZ * 3 {
            sim.set_input(
                PlayerId(1),
                InputState {
                    move_x: 1.0,
                    boost,
                    ..Default::default()
                },
            );
            sim.step();
        }
        sim.position(h).unwrap().x / 3.0
    };
    let walk = run(&[], false);
    let fast = run(&[PowerId(6)], false);
    let faster = run(&[PowerId(6)], true);
    assert!(fast > walk * 2.0, "{fast} vs {walk}");
    assert!(faster > fast * 1.4, "{faster} vs {fast}");
}

/// Where a lone hero goes next: the nearest corner with villains, or, with
/// all beaten, the farthest one (so the others fill again behind them).
fn next_spot(sim: &mut Sim, me: DVec3) -> DVec3 {
    let spots = sim.fights().spots.clone();
    let near = spots
        .iter()
        .filter(|s| s.members > 0)
        .min_by(|a, b| a.center.distance(me).total_cmp(&b.center.distance(me)));
    match near {
        Some(s) => s.center,
        None => {
            spots
                .iter()
                .max_by(|a, b| a.center.distance(me).total_cmp(&b.center.distance(me)))
                .expect("spots")
                .center
        }
    }
}

/// Ten hours of play in one go: levels 1 to 20 (about a minute in release,
/// several in a debug build, so it runs with `--ignored` and in the balance
/// example; `scripts/check.sh --daily` skips it by name).
#[test]
#[ignore = "ten hours of play: run it with --ignored or the balance example"]
fn a_new_hero_reaches_level_twenty_in_about_ten_hours() {
    let mut sim = arena(11);
    let me = PlayerId(1);
    let hero = sim.spawn_hero_with(me, ground(75.0, 75.0), progress::starter());
    for (x, z) in [(0.0, 0.0), (150.0, 0.0), (150.0, 150.0), (0.0, 150.0)] {
        let c = ground(x, z);
        let facing = (DVec2::new(75.0, 75.0) - DVec2::new(x, z)).normalize();
        sim.add_fight_spot(c, facing);
    }
    let mut reached: Vec<(u8, f64)> = Vec::new();
    let mut bought: Vec<(PowerId, f64)> = Vec::new();
    let limit = TICK_HZ as u64 * 3600 * 16;
    for t in 0..limit {
        let at = sim.position(hero).unwrap();
        let goal = next_spot(&mut sim, at);
        let input = bot_input(&mut sim, hero, t, Some(goal));
        sim.set_input(me, input);
        sim.step();
        if !t.is_multiple_of(TICK_HZ as u64 * 10) {
            continue;
        }
        let sheet = sim.sheet(me).unwrap();
        let level = level_of(sheet.lifetime);
        let hours = t as f64 / TICK_HZ as f64 / 3600.0;
        if reached.last().map_or(1, |r| r.0) < level {
            reached.push((level, hours));
        }
        // A careful player: the cheapest power the tree has open, saving
        // up for it rather than buying something else.
        let next = hw_sim::powers::tree(Tree::Science)
            .chain(hw_sim::powers::tree(Tree::Common))
            .filter(|d| !sheet.has(d.id) && progress::open(&sheet, d.id).is_ok())
            .min_by_key(|d| (progress::price(&sheet, d.id), d.id.0))
            .map(|d| d.id);
        if let Some(next) = next {
            if sim.buy(me, next).is_ok() {
                bought.push((next, hours));
            }
        }
        // Past the cap karma still comes: play on two more hours.
        if reached
            .last()
            .is_some_and(|r| r.0 == MAX_LEVEL && hours > r.1 + 2.0)
        {
            break;
        }
    }
    println!("levels (level, hour): {reached:.2?}");
    println!("powers (power, hour): {bought:.2?}");
    let when = |l: u8| reached.iter().find(|r| r.0 == l).map(|r| r.1);
    let cap = when(MAX_LEVEL).expect("the cap within sixteen hours");
    assert!((8.0..13.0).contains(&cap), "level 20 after {cap:.1} h");
    // About thirteen powers by the cap, and karma keeps buying more.
    let by_cap = bought.iter().filter(|b| b.1 <= cap).count();
    assert!((10..=17).contains(&by_cap), "{by_cap} powers by the cap");
    assert!(bought.len() > by_cap, "nothing bought after the cap");
}
