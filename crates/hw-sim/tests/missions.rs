//! An indoor case from start to end, headless: a hero asks the contact
//! for a case, goes to the building, the gang takes its rooms over the
//! floors (each group on its own floor's floor, the boss on top), and the
//! hero finds the clues and closes the case for karma.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_gen::layout::furnish::items;
use hw_gen::layout::office::plan_scheme;
use hw_gen::layout::solid::{pieces, Part};
use hw_gen::layout::Scheme;
use hw_net::{InputState, PlayerId};
use hw_sim::collision::{solids_of, Body, Obstacles};
use hw_sim::combat::{EnemyKind, Fights};
use hw_sim::floornav::{Building, FloorGraph, Floors};
use hw_sim::missions::{self, Case, CaseKind, Missions, GANGS};
use hw_sim::{Sim, GROUND_Y, TICK_HZ};
use std::sync::Arc;

/// An atrium building on flat ground, its layout's frame the world's.
fn town() -> (Sim, Arc<Floors>) {
    let size = DVec2::new(56.0, 52.0);
    let l = (0..16u64)
        .find_map(|k| plan_scheme(Seed(60 + k), size, 5, Scheme::Atrium))
        .expect("it fits");
    let mut o = Obstacles::flat(GROUND_Y);
    let lift = |f: hw_gen::urban::footprint::Footprint| f.placed(DVec2::ZERO, DVec2::Y, GROUND_Y);
    o.add_all(
        pieces(&l)
            .iter()
            .filter(|p| p.part != Part::LiftDoor)
            .flat_map(|p| solids_of(&lift(p.shape)))
            .chain(
                items(&l)
                    .iter()
                    .filter_map(|i| i.footprint())
                    .flat_map(|f| solids_of(&lift(f))),
            )
            .collect(),
    );
    let mut sim = Sim::with_solids(Seed(9), Arc::new(o));
    sim.set_spawn((DVec2::new(-150.0, -150.0), DVec2::X));
    let floors = Arc::new(Floors {
        buildings: vec![Building {
            pos: size * 0.5,
            dir: DVec2::new(0.0, -1.0),
            base: GROUND_Y,
            graph: FloorGraph::new(&l),
        }],
    });
    sim.world.resource_mut::<Fights>().floors = Some(floors.clone());
    let contact = DVec3::new(-150.0, GROUND_Y, -150.0);
    sim.missions = Missions::new(floors.clone(), Some(contact), 9);
    (sim, floors)
}

fn put(sim: &mut Sim, hero: hw_core::NetId, at: DVec3) {
    sim.set_body(
        hero,
        Body {
            pos: at,
            grounded: true,
            ..Body::default()
        },
    );
    // The hero holds out: the test is about the case, not the fight.
    if let Some(mut f) = sim.fighter(hero) {
        f.health = f.max_health;
        sim.set_fighter(hero, f);
    }
}

#[test]
fn a_hero_takes_a_case_and_closes_it() {
    let (mut sim, floors) = town();
    let contact = DVec3::new(-150.0, GROUND_Y, -150.0);
    let hero = sim.spawn_hero(PlayerId(1), contact + DVec3::X);
    // Asking: hold E beside the contact.
    sim.set_input(
        PlayerId(1),
        InputState {
            revive: true,
            ..InputState::default()
        },
    );
    for _ in 0..TICK_HZ * 2 {
        sim.step();
    }
    sim.set_input(PlayerId(1), InputState::default());
    assert_eq!(sim.missions.stage, missions::GO);
    assert!(sim.missions.case.is_some());
    // An evidence case in the building (the test's own pick).
    let case = Case {
        kind: CaseKind::Evidence,
        building: 0,
        gang: 0,
    };
    sim.missions.force(&mut sim.world, case);
    let b = &floors.buildings[0];
    let front = b.to_town(b.graph.front());
    put(&mut sim, hero, front);
    for _ in 0..TICK_HZ * 2 {
        put(&mut sim, hero, front);
        sim.step();
    }
    assert_eq!(sim.missions.stage, missions::INSIDE);
    // The gang on the floors: every villain stands on a floor of the plan,
    // on three floors or more, with the boss on the top floor.
    for _ in 0..TICK_HZ {
        put(&mut sim, hero, front);
        sim.step();
    }
    let levels: Vec<f64> = b.graph.rooms().iter().map(|r| r.2 + GROUND_Y).collect();
    let foes = sim.enemies();
    assert!(foes.len() >= 15, "{} villains", foes.len());
    let mut floors_held: Vec<i64> = Vec::new();
    for (_, kind, p, _) in &foes {
        // A robot's parts sit up on it.
        if *kind == EnemyKind::WeakPoint {
            continue;
        }
        assert!(
            levels.iter().any(|y| (p.y - y).abs() < 0.4),
            "{kind:?} at {p:.2?} stands on no floor"
        );
        floors_held.push((p.y * 10.0).round() as i64);
    }
    floors_held.sort();
    floors_held.dedup();
    assert!(floors_held.len() >= 3, "on {floors_held:?}");
    let top = levels.iter().copied().fold(0.0, f64::max);
    let boss = foes
        .iter()
        .find(|f| f.1 == GANGS[0].boss)
        .expect("the boss");
    assert!(
        (boss.2.y - top).abs() < 0.4,
        "the boss at {:.1?}, top {top}",
        boss.2
    );
    assert!(foes.iter().all(|f| f.1 != EnemyKind::Colossus));
    // The clues, each held for its seconds.
    let clues = sim.missions.clues();
    assert_eq!(clues.len(), missions::CLUES);
    let mut ys: Vec<i64> = clues
        .iter()
        .map(|c| (c.0.y * 10.0).round() as i64)
        .collect();
    ys.dedup();
    assert!(ys.len() >= 2, "clues on one floor: {clues:?}");
    let karma = sim.sheet(PlayerId(1)).expect("a sheet").karma;
    for (at, _) in clues {
        sim.set_input(
            PlayerId(1),
            InputState {
                revive: true,
                ..InputState::default()
            },
        );
        for _ in 0..(missions::CLUE_SECONDS * 1.3 * TICK_HZ as f64) as usize {
            put(&mut sim, hero, at);
            sim.step();
        }
        sim.set_input(PlayerId(1), InputState::default());
        sim.step();
    }
    for _ in 0..TICK_HZ * 2 {
        sim.step();
    }
    assert_eq!(sim.missions.closed, 1);
    assert_eq!(sim.missions.stage, missions::IDLE);
    let after = sim.sheet(PlayerId(1)).expect("a sheet").karma;
    assert!(after > karma, "karma {karma} -> {after}");
    // The gang that was left has gone.
    assert!(sim.enemies().is_empty());
}
