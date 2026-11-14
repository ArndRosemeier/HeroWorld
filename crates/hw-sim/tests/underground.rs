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
    // Onto the tunnel's centre line first (the passage may come out
    // against its wall), then along it.
    let lx = (q - tunnel.origin)
        .dot(tunnel.along)
        .clamp(1.0, tunnel.max.x - 1.0);
    let mid = tunnel.origin + tunnel.along * lx;
    for _ in 0..40 {
        let p = sim.position(hero).unwrap();
        let d = mid - DVec2::new(p.x, p.z);
        if d.length() < 0.3 {
            break;
        }
        run(
            &mut sim,
            me,
            walk(d.normalize() * 0.4),
            1.0 / TICK_HZ as f64,
        );
    }
    let q = {
        let p = sim.position(hero).unwrap();
        DVec2::new(p.x, p.z)
    };
    // Along the tunnel, whichever way is open from where the passage
    // came out (it may come out against one end).
    run(&mut sim, me, walk(tunnel.along), 4.0);
    let mut p2 = sim.position(hero).unwrap();
    if DVec2::new(p2.x, p2.z).distance(q) <= 8.0 {
        run(&mut sim, me, walk(-tunnel.along), 6.0);
        p2 = sim.position(hero).unwrap();
    }
    let q2 = DVec2::new(p2.x, p2.z);
    assert!(
        q2.distance(q) > 8.0 || tunnel.max.x - tunnel.min.x < 12.0,
        "stuck in the tunnel"
    );
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
    // From the pavement in front of the gate, in under its arch, on to the
    // top landing and down.
    let start = e.pos + e.door * (SHAFT_HALF + 5.0);
    put(
        &mut sim,
        hero,
        DVec3::new(start.x, town.world.ground(start) + 0.3, start.y),
    );
    run(&mut sim, me, InputState::default(), 0.5);
    let r0 = e.spiral.route()[0];
    let lead_in = r0 + DVec3::new(e.door.x, 0.0, e.door.y) * 2.5;
    follow(&mut sim, me, hero, &[lead_in, r0]);
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

#[test]
fn down_the_ramp_from_the_tunnels_into_molehaven() {
    let town = GeneratedTown::generate(Seed(1), 40_000.0);
    let mut sim = Sim::for_town(Seed(1), &town);
    let me = PlayerId(1);
    let hero = sim.spawn_hero(me, sim.spawn_point(1));
    let u = town.world.underground().clone();
    let c = u.cavern.clone().expect("Molehaven");
    let ramp = &u.voids[c.ramp.expect("a ramp from the tunnels")];
    let sign = c.ramp_sign.expect("a sign over the ramp");
    // Under the sign in the tunnel, then down the ramp the way it points.
    let top = ramp.world(DVec2::new(ramp.max.x - 1.5, 0.0));
    put(
        &mut sim,
        hero,
        DVec3::new(top.x, ramp.floor_at(top) + 0.3, top.y),
    );
    run(&mut sim, me, InputState::default(), 0.5);
    let len = ramp.max.x - ramp.min.x;
    let foot = ramp.world(DVec2::new(ramp.min.x + 0.5, 0.0));
    let mut route = Vec::new();
    let n = (len / 2.0).ceil() as usize;
    for k in 1..=n {
        let q = top.lerp(foot, k as f64 / n as f64);
        route.push(DVec3::new(q.x, 0.0, q.y));
    }
    follow(&mut sim, me, hero, &route);
    assert!(
        sign.point.dot(foot - top) > 0.0,
        "the sign points up the ramp"
    );
    // On into the hall towards the well, over open floor.
    let well = c.centre;
    run(&mut sim, me, walk((well - foot).normalize()), 1.5);
    run(&mut sim, me, InputState::default(), 0.5);
    let p = sim.position(hero).unwrap();
    let q = DVec2::new(p.x, p.z);
    assert!(
        (p.y - c.floor).abs() < 0.3,
        "at {:.1}, cavern floor {:.1}",
        p.y,
        c.floor
    );
    assert!(
        c.chambers.iter().any(|&i| u.voids[i].covers(q, 0.0)),
        "not in Molehaven: {q:?}"
    );
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn from_molehaven_down_its_stair_to_the_deeps() {
    let town = GeneratedTown::generate(Seed(1), 40_000.0);
    let mut sim = Sim::for_town(Seed(1), &town);
    let me = PlayerId(1);
    let hero = sim.spawn_hero(me, sim.spawn_point(1));
    let w = town.world.clone();
    let u = w.underground().clone();
    let c = u.cavern.clone().expect("Molehaven");
    let spot = hw_gen::urban::molehaven::deep_lift_spot(&u).expect("a side chamber");
    let l = w
        .deep_near(spot, 10.0)
        .iter()
        .flat_map(|t| t.lifts.clone())
        .find(|l| l.pos.distance(spot) < 0.5)
        .expect("Molehaven's stair down");
    assert!(
        (l.top - c.floor).abs() < 0.1,
        "top {} floor {}",
        l.top,
        c.floor
    );
    // From the chamber's floor a few steps away, into the stair and down.
    let route = l.spiral.route();
    let r0 = route[0];
    let start = DVec2::new(r0.x, r0.z) + l.spiral.dir * 4.0;
    put(&mut sim, hero, DVec3::new(start.x, c.floor + 0.3, start.y));
    run(&mut sim, me, InputState::default(), 0.5);
    follow(&mut sim, me, hero, &route);
    run(&mut sim, me, InputState::default(), 0.5);
    let p = sim.position(hero).unwrap();
    assert!(
        (p.y - l.bottom).abs() < 0.6,
        "at {:.1}, foot {:.1} (top {:.1})",
        p.y,
        l.bottom,
        l.top
    );
}

/// Walks the hero up `route` (top first; climbed from the last step) at a
/// real walking pace, aiming at the step after the next so corners are
/// cut the way a player cuts them. `Err` says where it stuck.
fn climb(
    sim: &mut Sim,
    me: PlayerId,
    hero: hw_core::NetId,
    route: &[DVec3],
    pace: f32,
) -> Result<(), String> {
    for k in (0..route.len()).rev() {
        let t = route[k];
        let mut tries = 0;
        loop {
            let p = sim.position(hero).ok_or("no hero")?;
            let d = DVec2::new(t.x - p.x, t.z - p.z);
            if d.length() < 0.3 {
                break;
            }
            tries += 1;
            if tries > 400 {
                return Err(format!("stuck going up to step {k} {t:?}: at {p:?}"));
            }
            run(
                sim,
                me,
                walk(d.normalize() * pace as f64),
                1.0 / TICK_HZ as f64,
            );
        }
    }
    Ok(())
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn up_and_down_molehavens_stair_at_a_real_pace() {
    let town = GeneratedTown::generate(Seed(1), 40_000.0);
    let mut sim = Sim::for_town(Seed(1), &town);
    let me = PlayerId(1);
    let hero = sim.spawn_hero(me, sim.spawn_point(1));
    let w = town.world.clone();
    let u = w.underground().clone();
    let spot = hw_gen::urban::molehaven::deep_lift_spot(&u).expect("a side chamber");
    let l = w
        .deep_near(spot, 10.0)
        .iter()
        .flat_map(|t| t.lifts.clone())
        .find(|l| l.pos.distance(spot) < 0.5)
        .expect("Molehaven's stair down");
    let route = l.spiral.route();
    for pace in [0.4f32, 1.0] {
        // Down from the top, at the pace.
        let r0 = route[0];
        put(&mut sim, hero, DVec3::new(r0.x, r0.y + 0.2, r0.z));
        run(&mut sim, me, InputState::default(), 0.5);
        let down: Vec<DVec3> = route.clone();
        for (k, t) in down.iter().enumerate() {
            let mut tries = 0;
            loop {
                let p = sim.position(hero).unwrap();
                let d = DVec2::new(t.x - p.x, t.z - p.z);
                if d.length() < 0.3 {
                    break;
                }
                tries += 1;
                assert!(
                    tries < 400,
                    "pace {pace}: stuck down to step {k} {t:?}: at {p:?}"
                );
                run(
                    &mut sim,
                    me,
                    walk(d.normalize() * pace as f64),
                    1.0 / TICK_HZ as f64,
                );
            }
        }
        // Up from the foot, back to the top (a bot aiming straight at each
        // step cannot go round the pillar at a run: a person steers).
        if pace > 0.5 {
            continue;
        }
        let last = *route.last().unwrap();
        put(&mut sim, hero, DVec3::new(last.x, last.y, last.z));
        run(&mut sim, me, InputState::default(), 0.5);
        if let Err(e) = climb(&mut sim, me, hero, &route, pace) {
            panic!("pace {pace}: {e}");
        }
        let p = sim.position(hero).unwrap();
        assert!(
            (p.y - l.top).abs() < 0.6,
            "pace {pace}: top at {:.1}, hero {:.1}",
            l.top,
            p.y
        );
    }
}
