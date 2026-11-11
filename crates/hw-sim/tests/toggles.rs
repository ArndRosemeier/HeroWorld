//! Toggles (defensive powers switched on and off, paid for with energy
//! recovery), flight that brings no energy back, and the powers that help
//! a hero alone in a hard fight (a lifeline, a second wind).

use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_net::{InputState, PlayerId, PowerId, Status};
use hw_sim::balance::hero_sheet;
use hw_sim::collision::Obstacles;
use hw_sim::combat::{ENERGY_REGEN, SECOND_WIND_BELOW};
use hw_sim::powers::def_of;
use hw_sim::{Sim, GROUND_Y, TICK_HZ};
use std::sync::Arc;

const TOUGH: PowerId = PowerId(60);
const STEEL_HIDE: PowerId = PowerId(74);
const FLIGHT: PowerId = PowerId(5);
const LIFELINE: PowerId = PowerId(81);
const SECOND_WIND: PowerId = PowerId(61);
const REFLECTIVE_AEGIS: PowerId = PowerId(92);

fn arena() -> Sim {
    let mut sim = Sim::with_solids(Seed(4), Arc::new(Obstacles::flat(GROUND_Y)));
    sim.set_spawn((DVec2::new(35.0, 35.0), DVec2::X));
    sim
}

fn ground(x: f64, z: f64) -> DVec3 {
    DVec3::new(x, GROUND_Y, z)
}

fn run(sim: &mut Sim, input: InputState, secs: f64) {
    for _ in 0..(secs * TICK_HZ as f64) as u64 {
        sim.set_input(PlayerId(1), input);
        sim.step();
    }
}

fn press(sim: &mut Sim, p: PowerId) {
    run(
        sim,
        InputState {
            power: p.0,
            ..Default::default()
        },
        0.1,
    );
    run(sim, InputState::default(), 0.1);
}

fn hero(sim: &mut Sim, powers: &[PowerId]) -> NetId {
    let h = sim.spawn_hero_with(PlayerId(1), ground(0.0, 0.0), hero_sheet(5, powers));
    // Long enough out of a fight to get energy back at the calm pace.
    run(sim, InputState::default(), 7.0);
    h
}

/// Energy back over `secs` with `input`, from a quarter full.
fn energy_back(sim: &mut Sim, h: NetId, input: InputState, secs: f64) -> f32 {
    let mut f = sim.fighter(h).unwrap();
    f.energy = f.max_energy * 0.25;
    sim.set_fighter(h, f);
    let before = sim.fighter(h).unwrap().energy;
    run(sim, input, secs);
    sim.fighter(h).unwrap().energy - before
}

#[test]
fn a_toggle_switches_on_and_off_and_takes_its_share_of_recovery() {
    let mut sim = arena();
    let h = hero(&mut sim, &[TOUGH, STEEL_HIDE]);
    let f = sim.fighter(h).unwrap();
    assert!(!f.kit.is_on(TOUGH) && f.kit.passive.resist == 0.0);
    let free = energy_back(&mut sim, h, InputState::default(), 1.0);
    assert!((free - ENERGY_REGEN.1).abs() < 1.0, "{free}");

    press(&mut sim, TOUGH);
    let f = sim.fighter(h).unwrap();
    assert!(f.kit.is_on(TOUGH), "pressing a toggle switches it on");
    assert!((f.kit.passive.resist - 0.15).abs() < 1e-5);
    assert_eq!(f.view(sim.tick).toggles, 1 << TOUGH.0);
    let taxed = energy_back(&mut sim, h, InputState::default(), 1.0);
    let upkeep = def_of(TOUGH).fields.upkeep;
    assert!(
        (taxed - free * (1.0 - upkeep)).abs() < 1.0,
        "{taxed} of {free}"
    );

    // Two on take both shares.
    run(&mut sim, InputState::default(), 1.0);
    press(&mut sim, STEEL_HIDE);
    let both = energy_back(&mut sim, h, InputState::default(), 1.0);
    let upkeeps = upkeep + def_of(STEEL_HIDE).fields.upkeep;
    assert!((both - free * (1.0 - upkeeps)).abs() < 1.0, "{both}");

    // And pressing again switches it off.
    run(&mut sim, InputState::default(), 1.0);
    press(&mut sim, TOUGH);
    let f = sim.fighter(h).unwrap();
    assert!(!f.kit.is_on(TOUGH) && f.kit.is_on(STEEL_HIDE));
    assert!((f.kit.passive.resist - 0.5).abs() < 1e-5);
}

#[test]
fn no_energy_comes_back_in_the_air() {
    let mut sim = arena();
    let h = hero(&mut sim, &[FLIGHT]);
    let fly = InputState {
        fly: true,
        jump: true,
        ..Default::default()
    };
    run(&mut sim, fly, 1.0);
    assert!(sim.position(h).unwrap().y > GROUND_Y + 3.0, "flying");
    let up = energy_back(&mut sim, h, fly, 2.0);
    assert_eq!(up, 0.0, "flying brings no energy back");
    // Landed, it comes back again.
    run(&mut sim, InputState::default(), 6.0);
    assert_eq!(
        sim.position(h).unwrap().y.round(),
        GROUND_Y.round(),
        "landed"
    );
    let down = energy_back(&mut sim, h, InputState::default(), 1.0);
    assert!(down > 10.0, "{down}");
}

/// A hero with `powers` next to one brute, at `health` of their own.
fn cornered(powers: &[PowerId], health: f32) -> (Sim, NetId) {
    let mut sim = arena();
    let h = hero(&mut sim, powers);
    sim.add_fight_spot(ground(3.0, 0.0), DVec2::new(-1.0, 0.0));
    for (id, _, _, mut f) in sim.enemies().into_iter().skip(1) {
        f.status = Status::Down;
        f.health = 0.0;
        f.until = u64::MAX;
        sim.set_fighter(id, f);
    }
    let mut f = sim.fighter(h).unwrap();
    f.health = f.max_health * health;
    sim.set_fighter(h, f);
    (sim, h)
}

#[test]
fn a_downed_hero_drops_their_toggles_and_a_lifeline_gets_them_up() {
    // (Both Guardian powers: a hero has one origin's tree.)
    let (mut sim, h) = cornered(&[REFLECTIVE_AEGIS, LIFELINE], 0.05);
    press(&mut sim, REFLECTIVE_AEGIS);
    assert!(sim.fighter(h).unwrap().kit.is_on(REFLECTIVE_AEGIS));
    let mut down_at = None;
    for t in 0..TICK_HZ as u64 * 30 {
        run(&mut sim, InputState::default(), 1.0 / TICK_HZ as f64);
        let f = sim.fighter(h).unwrap();
        if f.status == Status::Down && down_at.is_none() {
            down_at = Some(t);
            assert!(
                !f.kit.is_on(REFLECTIVE_AEGIS),
                "a downed hero's toggles drop"
            );
        }
        if let Some(at) = down_at.filter(|_| f.status == Status::Standing) {
            let secs = (t - at) as f64 / TICK_HZ as f64;
            assert!((9.0..11.0).contains(&secs), "up after {secs} s");
            return;
        }
    }
    panic!("never went down and got up again ({down_at:?})");
}

#[test]
fn a_second_wind_comes_near_defeat() {
    let (mut sim, h) = cornered(&[SECOND_WIND], SECOND_WIND_BELOW + 0.2);
    let mut last = sim.fighter(h).unwrap().health;
    for _ in 0..TICK_HZ as u64 * 30 {
        run(&mut sim, InputState::default(), 1.0 / TICK_HZ as f64);
        let f = sim.fighter(h).unwrap();
        if f.health > last + f.max_health * 0.3 {
            assert_eq!(f.status, Status::Standing);
            return;
        }
        assert_ne!(f.status, Status::Down, "went down without a second wind");
        last = f.health;
    }
    panic!("no second wind");
}
