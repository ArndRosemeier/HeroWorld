//! Going below: down a spiral stair beside a main road, along the passage
//! into the utility tunnel, back up; and down Molehaven's stair into the
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

/// Walks the hero over `route` (feet points), steering at each in turn.
fn follow(sim: &mut Sim, me: PlayerId, hero: hw_core::NetId, route: &[DVec3]) {
    for (k, t) in route.iter().enumerate() {
        let mut tries = 0;
        loop {
            let p = sim.position(hero).unwrap();
            let d = DVec2::new(t.x - p.x, t.z - p.z);
            if d.length() < 0.35 {
                break;
            }
            tries += 1;
            assert!(
                tries < 3000,
                "stuck on the way to point {k} {t:?}: at {p:?}"
            );
            // Slowly: a bot that cuts corners at a run falls off the stair.
            run(sim, me, walk(d.normalize() * 0.4), 1.0 / TICK_HZ as f64);
        }
    }
}

/// The floor-level points of a stair's foot route, at `y`.
fn foot(
    sp: &hw_gen::urban::spiral::Spiral,
    passage: Option<&hw_gen::urban::underground::Void>,
    y: f64,
) -> Vec<DVec3> {
    sp.foot_route(passage)
        .expect("a way out of the foot")
        .into_iter()
        .map(|q| DVec3::new(q.x, y, q.y))
        .collect()
}

fn walk(dir: DVec2) -> InputState {
    InputState {
        move_x: dir.x as f32,
        move_z: dir.y as f32,
        ..Default::default()
    }
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn down_the_stairs_through_the_tunnels_and_up_again() {
    let town = GeneratedTown::generate(Seed(1), 40_000.0);
    let mut sim = Sim::for_town(Seed(1), &town);
    let me = PlayerId(1);
    let hero = sim.spawn_hero(me, sim.spawn_point(1));
    let u = town.world.underground().clone();
    let e = u.entrances.iter().find(|e| !e.molehaven).expect("a stair");
    let passage = u
        .voids
        .iter()
        .find(|v| v.kind == VoidKind::Passage && v.origin.distance(e.pos) < 0.1);
    // Step in from the pavement side, on to the top landing and down.
    let start = e.pos + e.door * (SHAFT_HALF + 1.5);
    put(
        &mut sim,
        hero,
        DVec3::new(start.x, town.world.ground(start) + 0.3, start.y),
    );
    run(&mut sim, me, InputState::default(), 0.5);
    let route = e.spiral.route();
    follow(&mut sim, me, hero, &route);
    let p = sim.position(hero).unwrap();
    assert!(
        DVec2::new(p.x, p.z).distance(e.pos) < SHAFT_HALF,
        "not in the shaft: {p:?}"
    );
    assert!(p.y < e.top - 5.0, "still near the top: {p:?}");
    // Out of the stair's foot, along the floor.
    let way = foot(&e.spiral, passage, e.bottom);
    follow(&mut sim, me, hero, &way);
    run(&mut sim, me, InputState::default(), 0.5);
    let p = sim.position(hero).unwrap();
    assert!(
        (p.y - e.bottom).abs() < 0.3,
        "at {:.1}, floor {:.1}",
        p.y,
        e.bottom
    );
    // Along the passage into the tunnel and on along it.
    let passage = passage.expect("passage");
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
    // Back to the foot of the stair and up.
    let mut up = way.clone();
    up.reverse();
    put(&mut sim, hero, DVec3::new(up[0].x, e.bottom + 0.1, up[0].z));
    follow(&mut sim, me, hero, &up);
    let mut climb = route.clone();
    climb.reverse();
    follow(&mut sim, me, hero, &climb);
    let p = sim.position(hero).unwrap();
    assert!(
        p.y > e.top - 0.5,
        "stair stopped at {:.1} (top {:.1})",
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
    let e = u.entrances.iter().find(|e| e.molehaven).expect("its stair");
    let r0 = e.spiral.route()[0];
    put(&mut sim, hero, DVec3::new(r0.x, r0.y + 0.2, r0.z));
    follow(&mut sim, me, hero, &e.spiral.route());
    // Off the last step: into the hall when the stair stands in it, else
    // out along the passage.
    let passage = u
        .voids
        .iter()
        .find(|v| v.kind == VoidKind::Passage && v.origin.distance(e.pos) < 0.1);
    follow(&mut sim, me, hero, &foot(&e.spiral, passage, c.floor));
    run(&mut sim, me, walk(-e.spiral.dir), 0.6);
    run(&mut sim, me, InputState::default(), 0.5);
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
