//! Milestone 7, headless: karma and levels, the slice powers, the balance
//! run, and friends of different levels fighting together.

use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_net::{FxKind, InputState, PlayerId, Power, Status, ToServer};
use hw_sim::balance::{self, bot_input, hero_sheet};
use hw_sim::collision::Obstacles;
use hw_sim::combat::{power_spec, Fighter};
use hw_sim::host::Host;
use hw_sim::progress::{self, level_of};
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

#[test]
fn the_balance_run_is_green() {
    let results = balance::run();
    for r in &results {
        println!(
            "{:<16} tier {}  {:>5.1} s  hurt {:>3.0}%  worth {:.2}",
            power_spec(r.power).name,
            r.tier,
            r.seconds,
            r.hurt * 100.0,
            r.worth
        );
    }
    let flags = balance::outliers(&results);
    assert!(flags.is_empty(), "outliers: {flags:#?}");
    // Every power was measured.
    assert_eq!(results.len(), Power::ALL.len() - 1);
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

#[test]
fn a_new_hero_reaches_level_five_in_about_an_hour() {
    let mut sim = arena(11);
    // A fresh hero: Strike, and Energy Bolt bought with the starting karma.
    let me = PlayerId(1);
    let hero = sim.spawn_hero_with(me, ground(75.0, 75.0), progress::starter());
    // Street corners a block apart, as the city will have them.
    for (x, z) in [(0.0, 0.0), (150.0, 0.0), (150.0, 150.0), (0.0, 150.0)] {
        let c = ground(x, z);
        let facing = (DVec2::new(75.0, 75.0) - DVec2::new(x, z)).normalize();
        sim.add_fight_spot(c, facing);
    }
    let mut reached: Vec<(u8, f64)> = Vec::new();
    let mut flight_at = None;
    let mut level_ups = 0;
    let limit = TICK_HZ as u64 * 60 * 120;
    for t in 0..limit {
        let at = sim.position(hero).unwrap();
        let goal = next_spot(&mut sim, at);
        let input = bot_input(&mut sim, hero, t, Some(goal));
        sim.set_input(me, input);
        sim.step();
        level_ups += sim
            .fx()
            .iter()
            .filter(|e| e.kind == FxKind::LevelUp && e.to == hero)
            .count();
        let sheet = sim.sheet(me).unwrap();
        let level = level_of(sheet.lifetime);
        let minutes = t as f64 / TICK_HZ as f64 / 60.0;
        if reached.last().map_or(1, |r| r.0) < level {
            reached.push((level, minutes));
        }
        // Spend like a player saving for flight: a hold at level 3, then
        // flight as soon as it opens and can be paid for.
        if t.is_multiple_of(TICK_HZ as u64 * 10) {
            if !sheet.has(Power::Hold) && sheet.has(Power::Bolt) {
                let _ = sim.buy(me, Power::Hold, 1);
            }
            if level >= 5 && sim.buy(me, Power::Flight, 1).is_ok() {
                flight_at = Some(minutes);
            }
        }
        if flight_at.is_some() {
            break;
        }
    }
    println!("levels reached (level, minute): {reached:?}; flight at {flight_at:?}");
    let when = |l: u8| reached.iter().find(|r| r.0 == l).map(|r| r.1);
    let two = when(2).expect("level 2");
    let three = when(3).expect("level 3");
    let five = when(5).expect("level 5 within two hours");
    assert!((6.0..16.0).contains(&two), "level 2 after {two:.0} min");
    assert!(
        (16.0..35.0).contains(&three),
        "level 3 after {three:.0} min"
    );
    assert!((45.0..75.0).contains(&five), "level 5 after {five:.0} min");
    let flight = flight_at.expect("flight bought");
    assert!(
        flight - five < 12.0,
        "flight {flight:.0} min, level 5 {five:.0}"
    );
    assert_eq!(level_ups, 4, "a level-up moment for each level");
    assert_eq!(sim.fighter(hero).unwrap().level, 5, "fights at its level");
}

#[test]
fn friends_of_different_levels_fight_together_at_one_level() {
    let mut sim = arena(12);
    let host = PlayerId(1);
    let friend = PlayerId(2);
    let a = sim.spawn_hero_with(host, ground(30.0, 0.0), hero_sheet(3, &[Power::Bolt]));
    let b = sim.spawn_hero_with(
        friend,
        ground(30.0, 2.0),
        hero_sheet(9, &[Power::Bolt, Power::Shockwave]),
    );
    sim.step();
    // Both fight at the host's level 3: the level-9 friend is brought down
    // to it, with its health; the friend keeps its own level and powers.
    assert_eq!(sim.team_level(), 3);
    let (fa, fb) = (sim.fighter(a).unwrap(), sim.fighter(b).unwrap());
    assert_eq!((fa.level, fb.level), (3, 3));
    assert_eq!(fa.max_health, fb.max_health);
    assert!(fb.kit.has(Power::Shockwave));
    // The group they meet is at that level too.
    sim.add_fight_spot(ground(60.0, 0.0), DVec2::new(-1.0, 0.0));
    let levels: Vec<u8> = sim.enemies().iter().map(|e| e.3.level).collect();
    assert!(levels.iter().all(|&l| l == 3 || l == 4), "{levels:?}");
    let before = (sim.sheet(host).unwrap(), sim.sheet(friend).unwrap());
    let mut won = false;
    for t in 0..TICK_HZ as u64 * 120 {
        for (p, h) in [(host, a), (friend, b)] {
            let input = bot_input(&mut sim, h, t + p.0 as u64, None);
            sim.set_input(p, input);
        }
        sim.step();
        if sim.enemies().is_empty() {
            won = true;
            break;
        }
    }
    assert!(won, "they beat the group together");
    // Both earned karma for it, on their own sheets.
    let after = (sim.sheet(host).unwrap(), sim.sheet(friend).unwrap());
    assert!(after.0.lifetime > before.0.lifetime);
    assert!(after.1.lifetime > before.1.lifetime);
    // When the friend hosts instead, the low hero is brought up to 9.
    let mut sim = arena(12);
    let b = sim.spawn_hero_with(host, ground(0.0, 0.0), hero_sheet(9, &[]));
    let a = sim.spawn_hero_with(friend, ground(0.0, 2.0), hero_sheet(3, &[]));
    sim.step();
    assert_eq!(sim.fighter(a).unwrap().level, 9);
    assert_eq!(sim.fighter(b).unwrap().level, 9);
}

/// A hero with `powers` at level 5 next to one brute (held still by
/// nothing) on open ground; returns the sim, the hero and the brute.
fn duel(powers: &[Power]) -> (Sim, NetId, NetId) {
    let mut sim = arena(3);
    let hero = sim.spawn_hero_with(PlayerId(1), ground(0.0, 0.0), hero_sheet(5, powers));
    sim.step();
    sim.add_fight_spot(ground(12.0, 0.0), DVec2::new(-1.0, 0.0));
    let brute = sim.enemies()[0].0;
    // Only one villain: the rest of the group goes home.
    for (id, _, _, mut f) in sim.enemies().into_iter().skip(1) {
        f.status = Status::Down;
        f.health = 0.0;
        f.until = u64::MAX;
        sim.set_fighter(id, f);
    }
    (sim, hero, brute)
}

fn press(sim: &mut Sim, input: InputState, ticks: u64) -> Vec<hw_net::Fx> {
    let mut seen = Vec::new();
    for k in 0..ticks {
        sim.set_input(
            PlayerId(1),
            if k == 0 {
                input
            } else {
                InputState {
                    target: input.target,
                    ..Default::default()
                }
            },
        );
        sim.step();
        seen.extend(sim.fx().iter().copied());
    }
    seen
}

#[test]
fn a_binding_hold_stops_a_villain() {
    let (mut sim, hero, brute) = duel(&[Power::Hold]);
    let fx = press(
        &mut sim,
        InputState {
            power: Power::Hold.code(),
            target: Some(brute),
            ..Default::default()
        },
        2,
    );
    assert!(fx.iter().any(|e| e.kind == FxKind::Hold && e.to == brute));
    let held = sim.fighter(brute).unwrap();
    assert!(held.held(sim.tick));
    let at = sim.position(brute).unwrap();
    // Held, it neither walks nor strikes for several seconds.
    let fx = press(&mut sim, InputState::default(), TICK_HZ as u64 * 4);
    assert!(sim.position(brute).unwrap().distance(at) < 0.5);
    assert!(!fx.iter().any(|e| e.kind == FxKind::PunchWindup));
    let _ = hero;
}

#[test]
fn a_force_shield_halves_blows_for_the_hero_and_friends() {
    let mut sim = arena(4);
    let a = sim.spawn_hero_with(
        PlayerId(1),
        ground(0.0, 0.0),
        hero_sheet(5, &[Power::Shield]),
    );
    let b = sim.spawn_hero_with(PlayerId(2), ground(4.0, 0.0), hero_sheet(5, &[]));
    let far = sim.spawn_hero_with(PlayerId(3), ground(60.0, 0.0), hero_sheet(5, &[]));
    sim.step();
    let fx = press(
        &mut sim,
        InputState {
            power: Power::Shield.code(),
            ..Default::default()
        },
        2,
    );
    let shielded: Vec<NetId> = fx
        .iter()
        .filter(|e| e.kind == FxKind::Shield)
        .map(|e| e.to)
        .collect();
    assert!(shielded.contains(&a) && shielded.contains(&b));
    assert!(!shielded.contains(&far), "too far away");
    let fb: Fighter = sim.fighter(b).unwrap();
    assert!(fb.shielded(sim.tick));
    assert!(sim.fighter(a).unwrap().energy < 100.0);
    let _ = far;
}

#[test]
fn lift_and_throw_hits_a_villain_with_a_crate() {
    let (mut sim, hero, brute) = duel(&[Power::Lift]);
    sim.spawn_crate(ground(1.5, 0.0) + DVec3::Y * 0.5, [0.0, 0.0, 0.0, 1.0]);
    let lift = InputState {
        power: Power::Lift.code(),
        target: Some(brute),
        ..Default::default()
    };
    let fx = press(&mut sim, lift, 20);
    assert!(fx.iter().any(|e| e.kind == FxKind::Lift && e.from == hero));
    assert!(sim.fighter(hero).unwrap().carrying.is_some());
    let health = sim.fighter(brute).unwrap().health;
    let fx = press(&mut sim, lift, TICK_HZ as u64 * 2);
    assert!(fx.iter().any(|e| e.kind == FxKind::Throw));
    assert!(
        fx.iter()
            .any(|e| e.kind == FxKind::ThrowHit && e.to == brute),
        "{:?}",
        fx.iter().map(|e| e.kind).collect::<Vec<_>>()
    );
    assert!(sim.fighter(brute).unwrap().health < health);
    assert!(sim.fighter(hero).unwrap().carrying.is_none());
}

#[test]
fn a_shockwave_knocks_down_everyone_close() {
    let mut sim = arena(5);
    let hero = sim.spawn_hero_with(
        PlayerId(1),
        ground(0.0, 0.0),
        hero_sheet(5, &[Power::Shockwave]),
    );
    sim.step();
    sim.add_fight_spot(ground(3.0, 0.0), DVec2::new(-1.0, 0.0));
    let fx = press(
        &mut sim,
        InputState {
            power: Power::Shockwave.code(),
            ..Default::default()
        },
        2,
    );
    let hit = fx
        .iter()
        .filter(|e| e.kind == FxKind::Shockwave && e.from == hero && e.to != hero)
        .count();
    assert!(hit >= 3, "caught {hit}");
    let down = sim
        .enemies()
        .iter()
        .filter(|e| e.3.status != Status::Standing)
        .count();
    assert!(down >= 3);
}

#[test]
fn flight_is_an_unlock_and_super_jump_jumps_higher() {
    let peak = |powers: &[Power], fly: bool| {
        let mut sim = arena(6);
        let h = sim.spawn_hero_with(PlayerId(1), ground(0.0, 0.0), hero_sheet(5, powers));
        for _ in 0..10 {
            sim.step();
        }
        let mut top: f64 = 0.0;
        for k in 0..TICK_HZ * 3 {
            sim.set_input(
                PlayerId(1),
                InputState {
                    jump: k < 3 || fly,
                    fly,
                    ..Default::default()
                },
            );
            sim.step();
            top = top.max(sim.position(h).unwrap().y);
        }
        top - GROUND_Y
    };
    let plain = peak(&[], false);
    let super_jump = peak(&[Power::SuperJump], false);
    assert!(plain < 2.0 && super_jump > 5.0, "{plain} {super_jump}");
    // Flying without Flight is just a jump.
    assert!(peak(&[], true) < 2.0);
    assert!(peak(&[Power::Flight], true) > 15.0);
}

#[test]
fn powers_are_bought_and_given_up_over_the_network() {
    let mut host = Host::with_sim(arena(7));
    let addr = host.listen("127.0.0.1:0").unwrap();
    let mut c = hw_net::Connection::open(addr, "Friend").unwrap();
    let run = |host: &mut Host, c: &mut hw_net::Connection, n: usize| {
        for _ in 0..n {
            c.update(InputState::default());
            std::thread::sleep(std::time::Duration::from_millis(2));
            host.tick();
        }
    };
    run(&mut host, &mut c, 20);
    let me = match c.state {
        hw_net::ConnState::Connected { player, .. } => player,
        ref s => panic!("{s:?}"),
    };
    // The friend's hero file arrives; the server plays by it and confirms.
    let mut file = progress::new_hero(1);
    file.lifetime = progress::karma_for_level(3);
    file.karma = 5000;
    c.send(ToServer::Sheet(file.clone()));
    run(&mut host, &mut c, 20);
    assert_eq!(host.sim.sheet(me).unwrap().karma, 5000);
    c.send(ToServer::Buy {
        power: Power::Hold,
        theme: 1,
    });
    run(&mut host, &mut c, 20);
    let s = host.sim.sheet(me).unwrap();
    assert!(s.has(Power::Hold));
    let paid = 5000 - s.karma;
    c.send(ToServer::GiveUp { power: Power::Hold });
    // Flight is not for sale at level 3.
    c.send(ToServer::Buy {
        power: Power::Flight,
        theme: 0,
    });
    run(&mut host, &mut c, 20);
    let s = host.sim.sheet(me).unwrap();
    assert!(!s.has(Power::Hold) && !s.has(Power::Flight));
    assert_eq!(s.karma, 5000 - paid + (paid as f64 * 0.9) as u32);
    // The client heard every change, the last one last.
    let sheets: Vec<_> = c
        .take_news()
        .into_iter()
        .filter_map(|m| match m {
            hw_net::ToClient::Sheet(s) => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(sheets.last(), Some(&s));
}

/// Things lifted and held anywhere in the world (by what holds them).
fn held(sim: &mut Sim) -> Vec<NetId> {
    sim.world
        .query::<&hw_sim::combat::Carried>()
        .iter(&sim.world)
        .map(|c| c.0)
        .collect()
}

#[test]
fn nothing_lifted_outlives_its_power() {
    let lift = InputState {
        power: Power::Lift.code(),
        ..Default::default()
    };
    let secs = |s: f64| (s * TICK_HZ as f64).ceil() as u64;
    let hold_for = secs(hw_sim::combat::CARRY_SECONDS) + 2;

    // Lifted with no villain anywhere: a chunk of street rides overhead,
    // then slips free on its own, lands and is cleared away.
    let mut sim = arena(4);
    let hero = sim.spawn_hero_with(PlayerId(1), ground(0.0, 0.0), hero_sheet(5, &[Power::Lift]));
    sim.step();
    let fx = press(&mut sim, lift, 2);
    assert!(fx.iter().any(|e| e.kind == FxKind::Lift && e.from == hero));
    let chunk = sim
        .fighter(hero)
        .unwrap()
        .carrying
        .expect("lifted something");
    assert_eq!(held(&mut sim), vec![hero]);
    let fx = press(&mut sim, InputState::default(), hold_for);
    assert!(fx.iter().any(|e| e.kind == FxKind::Throw && e.from == hero));
    assert!(sim.fighter(hero).unwrap().carrying.is_none());
    assert!(
        held(&mut sim).is_empty(),
        "still held after the grip ran out"
    );
    press(
        &mut sim,
        InputState::default(),
        secs(hw_sim::combat::RUBBLE_SECONDS) + 2,
    );
    let left: Vec<NetId> = sim
        .world
        .query::<&hw_sim::Net>()
        .iter(&sim.world)
        .map(|n| n.0)
        .collect();
    assert!(!left.contains(&chunk), "torn-up rubble is never cleared");

    // Lifted with no target, then thrown where aimed: let go at once.
    let lifted = |sim: &mut Sim| {
        let fx = press(sim, lift, secs(2.0));
        assert!(fx.iter().any(|e| e.kind == FxKind::Lift));
        assert_eq!(held(sim).len(), 1);
    };
    lifted(&mut sim);
    let throw = InputState {
        aim: [0.0, 0.0, 1.0],
        ..lift
    };
    let fx = press(&mut sim, throw, 2);
    assert!(fx.iter().any(|e| e.kind == FxKind::Throw && e.from == hero));
    assert!(held(&mut sim).is_empty());

    // Giving the power up lets go of what it held.
    let fx = press(&mut sim, InputState::default(), secs(5.0));
    let _ = fx;
    lifted(&mut sim);
    sim.give_up(PlayerId(1), Power::Lift);
    sim.step();
    assert!(sim.fighter(hero).unwrap().carrying.is_none());
    assert!(
        held(&mut sim).is_empty(),
        "held after the power was given up"
    );

    // Knocked down while holding it: it falls.
    let mut sim = arena(4);
    let hero = sim.spawn_hero_with(PlayerId(1), ground(0.0, 0.0), hero_sheet(5, &[Power::Lift]));
    sim.step();
    lifted(&mut sim);
    let mut f = sim.fighter(hero).unwrap();
    f.status = Status::Down;
    f.until = u64::MAX;
    sim.set_fighter(hero, f);
    sim.step();
    assert!(held(&mut sim).is_empty(), "held by a downed hero");
}
