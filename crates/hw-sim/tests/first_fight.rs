//! The first fight, headless: heroes against a group of the mad
//! scientist's minions on flat ground. A simple bot plays each hero the way
//! a player would (Tab to the nearest villain, bolt from range, punch up
//! close); the fight must be won, must take a while, and must end the same
//! way every time.

use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_net::{FxKind, InputState, PlayerId, Power, Status};
use hw_sim::collision::Obstacles;
use hw_sim::combat::{power_spec, MEDIC_DELAY, REVIVE_SECONDS};
use hw_sim::{Sim, GROUND_Y, TICK_HZ};
use std::sync::Arc;

fn arena(seed: u64) -> Sim {
    let mut sim = Sim::with_solids(Seed(seed), Arc::new(Obstacles::flat(GROUND_Y)));
    sim.set_spawn((DVec2::ZERO, DVec2::X));
    sim
}

/// What a sensible player presses: keep the nearest standing villain
/// targeted, stay at bolt range from the brutes, bolt when there is energy
/// and punch whatever comes close.
fn bot(sim: &mut Sim, hero: NetId, tick: u64) -> InputState {
    let me = sim.position(hero).unwrap();
    let f = sim.fighter(hero).unwrap();
    let foes: Vec<_> = sim
        .enemies()
        .into_iter()
        .filter(|(_, _, _, e)| matches!(e.status, Status::Standing | Status::Knocked))
        .collect();
    let mut input = InputState::default();
    let Some((target, _, at, _)) = foes
        .iter()
        .min_by(|a, b| a.2.distance(me).total_cmp(&b.2.distance(me)))
        .copied()
    else {
        return input;
    };
    input.target = Some(target);
    let d = at.distance(me);
    let flat = DVec2::new(at.x - me.x, at.z - me.z).normalize_or_zero();
    // Close in to bolt range; punch when it is right there.
    if d > 14.0 || (d > power_spec(Power::Strike).range && f.energy < 15.0) {
        input.move_x = flat.x as f32;
        input.move_z = flat.y as f32;
    }
    // Press a key every few ticks, as people do.
    if tick.is_multiple_of(4) {
        if d <= power_spec(Power::Strike).range - 0.3 && f.energy < 40.0 {
            input.smash = true;
        } else if f.energy >= power_spec(Power::Bolt).cost {
            input.blast = true;
        } else {
            input.smash = true;
        }
    }
    input
}

/// The end of a fight: who won, when, how hurt everyone was.
#[derive(Debug, PartialEq)]
struct Outcome {
    ticks: u64,
    villains_left: usize,
    hero_health: Vec<f32>,
    /// Lowest health any hero fell to.
    lowest: f32,
    /// Blows the villains landed.
    taken: usize,
    downs: usize,
    hits: usize,
    knocks: usize,
    lifted: usize,
}

fn fight(seed: u64, players: u16) -> Outcome {
    let mut sim = arena(seed);
    let heroes: Vec<NetId> = (1..=players)
        .map(|k| sim.spawn_hero(PlayerId(k), DVec3::new(0.0, GROUND_Y, k as f64 * 2.0)))
        .collect();
    sim.add_fight_spot(DVec3::new(30.0, GROUND_Y, 0.0), DVec2::new(-1.0, 0.0));
    let size = sim.enemies().len();
    assert_eq!(size, 4 + players as usize, "group grows with the team");
    let (mut hits, mut knocks, mut lifted, mut downs, mut taken) = (0, 0, 0, 0, 0);
    let mut lowest = f32::MAX;
    let limit = TICK_HZ as u64 * 120;
    let mut t = 0;
    while t < limit {
        for (k, h) in heroes.iter().enumerate() {
            let input = bot(&mut sim, *h, t + k as u64);
            sim.set_input(PlayerId(k as u16 + 1), input);
        }
        sim.step();
        for e in sim.fx() {
            match e.kind {
                FxKind::Strike | FxKind::Bolt => {
                    hits += 1;
                    knocks += e.knock as usize;
                }
                FxKind::Lifted => lifted += 1,
                FxKind::Punch | FxKind::Ray => {
                    downs += e.defeat as usize;
                    taken += (e.amount > 0) as usize;
                }
                _ => {}
            }
        }
        for h in &heroes {
            lowest = lowest.min(sim.fighter(*h).unwrap().health);
        }
        t += 1;
        // Done once the whole group has been carried off.
        if sim.enemies().is_empty() {
            break;
        }
    }
    Outcome {
        ticks: t,
        villains_left: sim.enemies().len(),
        hero_health: heroes
            .iter()
            .map(|h| sim.fighter(*h).unwrap().health)
            .collect(),
        lowest,
        taken,
        downs,
        hits,
        knocks,
        lifted,
    }
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn one_hero_beats_five_minions_and_it_is_a_fight() {
    let o = fight(1, 1);
    println!("{o:?}");
    assert_eq!(o.villains_left, 0, "the group is beaten and carried off");
    assert_eq!(o.lifted, 5);
    assert_eq!(o.downs, 0, "a careful hero is not beaten by minions");
    // Not a walkover: it takes a while and costs health.
    let secs = o.ticks as f64 / TICK_HZ as f64;
    assert!(secs > 8.0 && secs < 60.0, "took {secs:.1} s");
    assert!(o.lowest < 85.0, "hero only fell to {}", o.lowest);
    assert!(o.taken >= 3, "villains landed {} blows", o.taken);
    assert!(o.hits >= 7, "{} hits", o.hits);
    assert!(o.knocks >= 3, "bolts knock villains around");
}

#[test]
fn a_test_fight_ends_the_same_every_time() {
    assert_eq!(fight(7, 1), fight(7, 1));
    assert_eq!(fight(7, 2), fight(7, 2));
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn two_heroes_win_faster_against_a_bigger_group() {
    let solo = fight(3, 1);
    let duo = fight(3, 2);
    println!("solo {solo:?}\nduo {duo:?}");
    assert_eq!(duo.villains_left, 0);
    assert_eq!(duo.lifted, 6);
    assert!(duo.ticks < solo.ticks);
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn a_downed_hero_is_revived_by_a_teammate() {
    let mut sim = arena(2);
    let a = sim.spawn_hero(PlayerId(1), DVec3::new(0.0, GROUND_Y, 0.0));
    let b = sim.spawn_hero(PlayerId(2), DVec3::new(1.5, GROUND_Y, 0.0));
    sim.step();
    // Hero A goes down (as if beaten).
    let mut f = sim.fighter(a).unwrap();
    f.health = 0.0;
    f.status = Status::Down;
    f.until = sim.tick + (MEDIC_DELAY * TICK_HZ as f64) as u64;
    sim.set_fighter(a, f);
    // A downed hero cannot move.
    sim.set_input(
        PlayerId(1),
        InputState {
            move_x: 1.0,
            ..Default::default()
        },
    );
    // B holds revive next to them.
    sim.set_input(
        PlayerId(2),
        InputState {
            revive: true,
            ..Default::default()
        },
    );
    let start = sim.position(a).unwrap();
    let mut revived_at = None;
    for t in 0..TICK_HZ as u64 * 5 {
        sim.step();
        if sim
            .fx()
            .iter()
            .any(|e| e.kind == FxKind::Revived && e.to == a)
        {
            revived_at = Some(t);
            break;
        }
    }
    let t = revived_at.expect("revived");
    let secs = t as f64 / TICK_HZ as f64;
    assert!((secs - REVIVE_SECONDS).abs() < 0.2, "took {secs}");
    let f = sim.fighter(a).unwrap();
    assert_eq!(f.status, Status::Standing);
    assert!(f.health > 40.0);
    assert!(sim.position(a).unwrap().distance(start) < 0.1, "lay still");
    let _ = b;
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn alone_a_downed_hero_is_flown_to_hospital() {
    let mut sim = arena(5);
    let a = sim.spawn_hero(PlayerId(1), DVec3::new(20.0, GROUND_Y, 0.0));
    sim.step();
    let mut f = sim.fighter(a).unwrap();
    f.health = 0.0;
    f.status = Status::Down;
    f.until = sim.tick + 10_000;
    sim.set_fighter(a, f);
    // Calling the medical drone at once.
    sim.set_input(
        PlayerId(1),
        InputState {
            hospital: true,
            ..Default::default()
        },
    );
    let mut seen = Vec::new();
    for _ in 0..TICK_HZ as u64 * 30 {
        sim.step();
        seen.extend(sim.fx().iter().map(|e| e.kind));
        if seen.contains(&FxKind::Healed) {
            break;
        }
    }
    assert!(seen.contains(&FxKind::Lifted));
    assert!(seen.contains(&FxKind::Healed), "{seen:?}");
    let f = sim.fighter(a).unwrap();
    assert_eq!(f.status, Status::Standing);
    assert_eq!(f.health, f.max_health);
    // Back at the hospital (the spawn street for now), the drone gone.
    let p = sim.position(a).unwrap();
    assert!(DVec2::new(p.x, p.z).length() < 6.0, "at {p}");
    assert!(sim
        .snapshot()
        .iter()
        .all(|e| e.kind != hw_net::EntityKind::Drone));
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn a_beaten_group_comes_back_once_the_heroes_have_gone() {
    let o = fight(4, 1);
    assert_eq!(o.villains_left, 0);
    let mut sim = arena(4);
    let h = sim.spawn_hero(PlayerId(1), DVec3::new(0.0, GROUND_Y, 0.0));
    let spot = sim.add_fight_spot(DVec3::new(30.0, GROUND_Y, 0.0), DVec2::new(-1.0, 0.0));
    // Beat them all at once (a cheat) and let the police come.
    for (id, _, _, mut f) in sim.enemies() {
        f.health = 0.0;
        f.status = Status::Down;
        f.until = sim.tick;
        sim.set_fighter(id, f);
    }
    for _ in 0..TICK_HZ * 12 {
        sim.step();
    }
    assert!(sim.enemies().is_empty());
    assert!(sim.fights().spots[spot].respawn_at.is_some());
    for _ in 0..TICK_HZ * 20 {
        sim.step();
    }
    assert_eq!(sim.enemies().len(), 5, "a new group");
    let _ = h;
}
