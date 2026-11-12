//! The portal power, headless: every hero has it, marking home writes the
//! hero file, a portal opens a stride ahead and walking into it brings the
//! hero out at home (and back), a wall in the way fizzles it, and a new
//! home closes the pair.

use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_net::{fizzle, FxKind, InputState, PlayerId, PowerId, ToClient};
use hw_sim::collision::{Obstacles, Solid};
use hw_sim::progress::starter;
use hw_sim::{Sim, GROUND_Y, TICK_HZ};
use std::sync::Arc;

fn ground(x: f64, z: f64) -> DVec3 {
    DVec3::new(x, GROUND_Y, z)
}

fn sim_with(solids: Obstacles) -> (Sim, NetId) {
    let mut sim = Sim::with_solids(Seed(9), Arc::new(solids));
    sim.set_spawn((DVec2::new(35.0, 35.0), DVec2::X));
    let hero = sim.spawn_hero_with(PlayerId(1), ground(0.0, 0.0), starter());
    sim.step();
    (sim, hero)
}

/// Holds `input` for `ticks` ticks (a power key only the first); returns
/// every fight event and every piece of news seen.
fn hold(sim: &mut Sim, input: InputState, ticks: u64) -> (Vec<hw_net::Fx>, Vec<ToClient>) {
    let (mut fx, mut news) = (Vec::new(), Vec::new());
    for k in 0..ticks {
        let mut i = input;
        if k > 0 {
            i.power = 0;
        }
        sim.set_input(PlayerId(1), i);
        sim.step();
        fx.extend(sim.fx().iter().copied());
        news.extend(sim.take_news());
    }
    (fx, news)
}

fn press(p: PowerId, aim: DVec3) -> InputState {
    InputState {
        power: p.0,
        aim: aim.as_vec3().to_array(),
        ..Default::default()
    }
}

fn walk(dir: DVec3) -> InputState {
    InputState {
        move_x: dir.x as f32,
        move_z: dir.z as f32,
        aim: dir.as_vec3().to_array(),
        ..Default::default()
    }
}

#[test]
fn a_new_hero_walks_through_its_portal_to_home_and_back() {
    let (mut sim, hero) = sim_with(Obstacles::flat(GROUND_Y));
    // A new hero owns both buttons, and has no home yet.
    let sheet = sim.sheet(PlayerId(1)).expect("hero file");
    assert!(sheet.has(PowerId::GATE_MARK) && sheet.has(PowerId::GATE_OPEN));
    assert!(sheet.gate_home.is_none());
    let (fx, _) = hold(&mut sim, press(PowerId::GATE_OPEN, DVec3::Z), 3);
    assert!(
        fx.iter()
            .any(|e| e.kind == FxKind::Fizzle && e.amount == fizzle::NO_HOME),
        "no home: it fizzles"
    );
    // Home: marked facing +X, a step ahead.
    let (fx, _) = hold(&mut sim, press(PowerId::GATE_MARK, DVec3::X), 3);
    assert!(fx.iter().any(|e| e.kind == FxKind::GateMark));
    let home = sim
        .sheet(PlayerId(1))
        .unwrap()
        .gate_home
        .expect("kept in the hero file");
    assert!((home.pos()[0] - hw_sim::gates::MARK_AHEAD).abs() < 0.02);
    // Walk 40 m down +Z and open a portal ahead.
    hold(&mut sim, walk(DVec3::Z), TICK_HZ as u64 * 6);
    hold(&mut sim, InputState::default(), TICK_HZ as u64);
    let here = sim.position(hero).unwrap();
    assert!(here.z > 30.0, "walked to {here}");
    let (fx, news) = hold(&mut sim, press(PowerId::GATE_OPEN, DVec3::Z), 3);
    assert!(fx.iter().any(|e| e.kind == FxKind::GateOpen));
    assert!(news
        .iter()
        .any(|m| matches!(m, ToClient::Gates { gates } if gates.len() == 1)));
    // Into it: out at home, walking on +X (the player's camera turns
    // with the hero, so its walking keys do too).
    let mut crossed = false;
    for _ in 0..TICK_HZ {
        let (fx, _) = hold(&mut sim, walk(DVec3::Z), 1);
        if fx
            .iter()
            .any(|e| e.kind == FxKind::GateCross && e.to == hero)
        {
            crossed = true;
            break;
        }
    }
    assert!(crossed, "stepped through");
    hold(&mut sim, walk(DVec3::X), TICK_HZ as u64 / 2);
    let at = sim.position(hero).unwrap();
    assert!(
        DVec2::new(at.x, at.z).distance(DVec2::new(home.pos()[0], 0.0)) < 8.0,
        "came out at {at}"
    );
    assert!(at.x > home.pos()[0], "in front of home");
    // Turn round and walk back in (-X): out at the far portal, going -Z.
    let (fx, _) = hold(&mut sim, walk(-DVec3::X), TICK_HZ as u64 * 2);
    assert!(fx.iter().any(|e| e.kind == FxKind::GateCross));
    let back = sim.position(hero).unwrap();
    let far = here.z + hw_sim::gates::OPEN_AHEAD;
    assert!(back.z > far - 10.0 && back.z < far, "back at {back}");
    // A new home closes the pair.
    let (fx, news) = hold(&mut sim, press(PowerId::GATE_MARK, DVec3::X), 3);
    assert_eq!(fx.iter().filter(|e| e.kind == FxKind::GateShut).count(), 2);
    assert!(news
        .iter()
        .any(|m| matches!(m, ToClient::Gates { gates } if gates.is_empty())));
}

#[test]
fn a_wall_ahead_fizzles_the_portal_and_costs_nothing() {
    // A wall across the street 2 m ahead.
    let mut solids = Obstacles::flat(GROUND_Y);
    solids.add(Solid {
        origin: DVec2::new(-20.0, 2.0),
        along: DVec2::new(1.0, 0.0),
        inward: DVec2::new(0.0, 1.0),
        min: DVec2::new(0.0, 0.0),
        max: DVec2::new(40.0, 1.0),
        y0: GROUND_Y,
        y1: GROUND_Y + 8.0,
    });
    let (mut sim, _) = sim_with(solids);
    hold(&mut sim, press(PowerId::GATE_MARK, -DVec3::Z), 3);
    hold(&mut sim, InputState::default(), TICK_HZ as u64 * 3);
    let (fx, news) = hold(&mut sim, press(PowerId::GATE_OPEN, DVec3::Z), 3);
    assert!(fx
        .iter()
        .any(|e| e.kind == FxKind::Fizzle && e.amount == fizzle::NO_ROOM));
    assert!(!fx.iter().any(|e| e.kind == FxKind::GateOpen));
    assert!(!news.iter().any(|m| matches!(m, ToClient::Gates { .. })));
}

#[test]
fn a_new_hero_has_its_portal_home_on_hero_halls_plaza() {
    let town = hw_sim::GeneratedTown::generate(Seed(2), 3000.0);
    let hall = town
        .world
        .landmarks
        .iter()
        .find(|l| l.kind == hw_gen::urban::landmarks::LandmarkKind::HeroHall)
        .expect("Hero Hall");
    let mut sim = Sim::for_town(Seed(2), &town);
    sim.spawn_at_start(PlayerId(1));
    sim.set_sheet(PlayerId(1), starter());
    sim.step();
    let home = sim
        .sheet(PlayerId(1))
        .unwrap()
        .gate_home
        .expect("a new hero's home is set");
    let p = home.pos();
    let d = DVec2::new(p[0], p[2]).distance(hall.pos);
    assert!(d < hall.plaza, "{d} m from the hall, plaza {}", hall.plaza);
    assert!((p[1] - hall.base).abs() < 0.1);
    // The server knows it as well, so Open Portal works at once.
    assert!(sim.gates_home_known(PlayerId(1)));
    // A home the hero marked itself is kept.
    let mut mine = starter();
    mine.gate_home = Some(hw_net::GateHome::new(sim.world_key(), [1.0, 2.0, 3.0], 0.0));
    sim.set_sheet(PlayerId(1), mine.clone());
    assert_eq!(sim.sheet(PlayerId(1)).unwrap().gate_home, mine.gate_home);
}
