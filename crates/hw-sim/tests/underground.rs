//! Going below: down an Aether lift beside a main road, along the passage
//! into the utility tunnel, back up; and down Molehaven's lift into the
//! cavern. The hero must stay on the floors down there, be kept in by the
//! rock, and come back up to the street.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_gen::urban::underground::{VoidKind, SHAFT_HALF};
use hw_net::{InputState, PlayerId};
use hw_sim::collision::Body;
use hw_sim::{GeneratedTown, Sim, TICK_HZ};

fn put(sim: &mut Sim, hero: hw_core::NetId, at: DVec3) {
    sim.set_body(
        hero,
        Body {
            pos: at,
            vel: DVec3::ZERO,
            grounded: false,
        },
    );
}

fn run(sim: &mut Sim, me: PlayerId, input: InputState, secs: f64) {
    sim.set_input(me, input);
    for _ in 0..(secs * TICK_HZ as f64) as u32 {
        sim.step();
    }
}

fn walk(dir: DVec2) -> InputState {
    InputState {
        move_x: dir.x as f32,
        move_z: dir.y as f32,
        ..Default::default()
    }
}

#[test]
fn down_the_lift_through_the_tunnels_and_up_again() {
    let town = GeneratedTown::generate(Seed(1), 40_000.0);
    let mut sim = Sim::for_town(Seed(1), &town);
    let me = PlayerId(1);
    let hero = sim.spawn_hero(me, sim.spawn_point(1));
    let u = town.world.underground().clone();
    let e = u.entrances.iter().find(|e| !e.molehaven).expect("a lift");
    // Step in from the pavement side.
    let start = e.pos + e.door * (SHAFT_HALF + 1.5);
    put(
        &mut sim,
        hero,
        DVec3::new(start.x, town.world.ground(start) + 0.3, start.y),
    );
    run(&mut sim, me, InputState::default(), 0.5);
    run(&mut sim, me, walk(-e.door), 0.45);
    run(&mut sim, me, InputState::default(), 0.5);
    let p = sim.position(hero).unwrap();
    assert!(
        DVec2::new(p.x, p.z).distance(e.pos) < SHAFT_HALF,
        "not in the shaft: {p:?}"
    );
    // Sink to the bottom.
    run(
        &mut sim,
        me,
        InputState {
            descend: true,
            ..Default::default()
        },
        6.0,
    );
    let p = sim.position(hero).unwrap();
    assert!(
        (p.y - e.bottom).abs() < 0.3,
        "at {:.1}, floor {:.1}",
        p.y,
        e.bottom
    );
    // Along the passage into the tunnel and on along it.
    let passage = u
        .voids
        .iter()
        .find(|v| v.kind == VoidKind::Passage && v.origin.distance(e.pos) < 0.1)
        .expect("passage");
    run(&mut sim, me, walk(passage.along), 3.0);
    let p = sim.position(hero).unwrap();
    let q = DVec2::new(p.x, p.z);
    let tunnel = u
        .voids
        .iter()
        .find(|v| v.kind == VoidKind::Tunnel && v.covers(q, 0.2))
        .expect("in a tunnel");
    assert!(p.y < town.world.ground(q) - 5.0, "not under ground");
    run(&mut sim, me, walk(tunnel.along), 4.0);
    let p2 = sim.position(hero).unwrap();
    let q2 = DVec2::new(p2.x, p2.z);
    assert!(q2.distance(q) > 8.0, "stuck in the tunnel");
    assert!(
        u.voids.iter().any(|v| v.covers(q2, 0.5)),
        "walked into the rock"
    );
    assert!(p2.y < town.world.ground(q2) - 5.0);
    // Walking into the wall gets nowhere.
    run(&mut sim, me, walk(tunnel.along.perp()), 3.0);
    let p3 = sim.position(hero).unwrap();
    assert!(u
        .voids
        .iter()
        .any(|v| v.covers(DVec2::new(p3.x, p3.z), 0.5)));
    // Back to the lift and up.
    put(&mut sim, hero, DVec3::new(e.pos.x, e.bottom + 0.1, e.pos.y));
    run(
        &mut sim,
        me,
        InputState {
            jump: true,
            ..Default::default()
        },
        6.0,
    );
    let p = sim.position(hero).unwrap();
    assert!(
        p.y > e.top - 0.5,
        "lift stopped at {:.1} (top {:.1})",
        p.y,
        e.top
    );
    run(&mut sim, me, walk(e.door), 1.0);
    run(&mut sim, me, InputState::default(), 1.0);
    let p = sim.position(hero).unwrap();
    let q = DVec2::new(p.x, p.z);
    assert!(
        (p.y - town.world.ground(q)).abs() < 0.6,
        "not back on the street"
    );
}

#[test]
fn molehaven_has_a_floor_and_walls() {
    let town = GeneratedTown::generate(Seed(1), 40_000.0);
    let mut sim = Sim::for_town(Seed(1), &town);
    let me = PlayerId(1);
    let hero = sim.spawn_hero(me, sim.spawn_point(1));
    let u = town.world.underground().clone();
    let c = u.cavern.clone().expect("Molehaven");
    let e = u.entrances.iter().find(|e| e.molehaven).expect("its lift");
    put(&mut sim, hero, DVec3::new(e.pos.x, e.top + 0.2, e.pos.y));
    run(
        &mut sim,
        me,
        InputState {
            descend: true,
            ..Default::default()
        },
        8.0,
    );
    let p = sim.position(hero).unwrap();
    assert!(
        (p.y - c.floor).abs() < 0.3,
        "at {:.1}, floor {:.1}",
        p.y,
        c.floor
    );
    // Run about the hall: always on its floor, never out through the rock.
    let hall = &u.voids[c.chambers[0]];
    for dir in [
        hall.along,
        hall.along.perp(),
        -hall.along,
        -hall.along.perp(),
    ] {
        run(&mut sim, me, walk(dir), 6.0);
        let p = sim.position(hero).unwrap();
        let q = DVec2::new(p.x, p.z);
        assert!(
            u.voids.iter().any(|v| v.covers(q, 0.5)),
            "out in the rock at {q:?}"
        );
        assert!(p.y < c.floor + 0.6, "climbed to {:.1}", p.y);
    }
}
