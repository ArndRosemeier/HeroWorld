//! The deep underground: down a station's street stair into the tube hall,
//! about the hall without leaving it, and down into the Deeps. The hero
//! must land on the floors down there and be kept in by the rock.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_gen::urban::deep::{tile_of, EntranceKind};
use hw_gen::urban::underground::SHAFT_HALF;
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

/// The passage void that joins entrance `e`'s shaft to the rest, if any.
fn tile_passage(
    w: &hw_gen::urban::world::TownWorld,
    e: &hw_gen::urban::deep::DeepEntrance,
) -> Option<hw_gen::urban::underground::Void> {
    w.deep_tile(tile_of(e.pos))
        .voids
        .iter()
        .find(|v| v.kind == hw_gen::urban::underground::VoidKind::Passage && v.covers(e.pos, 0.0))
        .cloned()
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn down_into_a_tube_station_and_about_its_hall() {
    let town = GeneratedTown::generate(Seed(1), 40_000.0);
    let w = town.world.clone();
    let mut sim = Sim::for_town(Seed(1), &town);
    let me = PlayerId(1);
    let hero = sim.spawn_hero(me, sim.spawn_point(1));
    // The first station with a street lift.
    let (tile, e) = w
        .tube()
        .stations
        .iter()
        .find_map(|s| {
            let t = w.deep_tile(tile_of(s.pos));
            let e = t
                .entrances
                .iter()
                .find(|e| e.kind == EntranceKind::Tube)
                .cloned()?;
            Some((t, e))
        })
        .expect("a station with a street lift");
    let r0 = e.spiral.route()[0];
    put(&mut sim, hero, DVec3::new(r0.x, r0.y + 0.2, r0.z));
    follow(&mut sim, me, hero, &e.spiral.route());
    run(&mut sim, me, walk(-e.door), 0.6);
    run(&mut sim, me, InputState::default(), 0.5);
    let p = sim.position(hero).unwrap();
    assert!(
        (p.y - e.bottom).abs() < 0.4,
        "at {:.1}, stair foot {:.1}",
        p.y,
        e.bottom
    );
    // Out of the stair's foot and about the hall.
    let hall = &tile.voids[tile.stations[0].hall];
    for dir in [
        e.door,
        hall.along,
        hall.along.perp(),
        -hall.along,
        -hall.along.perp(),
    ] {
        run(&mut sim, me, walk(dir), 4.0);
        let p = sim.position(hero).unwrap();
        let q = DVec2::new(p.x, p.z);
        assert!(
            w.deep_open_at(DVec3::new(p.x, p.y + 1.0, p.z)),
            "out in the rock at {p:?}"
        );
        let f = w.deep_floor_for(q, p.y + 0.5).expect("on a deep floor");
        assert!((p.y - f).abs() < 0.6, "at {:.1}, floor {f:.1}", p.y);
        assert!(p.y < w.ground(q) - 8.0, "came up out of the ground");
    }
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn down_a_stair_to_the_deeps() {
    let town = GeneratedTown::generate(Seed(1), 40_000.0);
    let w = town.world.clone();
    let mut sim = Sim::for_town(Seed(1), &town);
    let me = PlayerId(1);
    let hero = sim.spawn_hero(me, sim.spawn_point(1));
    let (core, _) = w.spawn_line();
    let l = w
        .deep_near(core, 900.0)
        .iter()
        .flat_map(|t| t.lifts.clone())
        .next()
        .expect("a stair to the Deeps");
    eprintln!("lift top {} bottom {} pos {}", l.top, l.bottom, l.pos);
    let r0 = l.spiral.route()[0];
    put(&mut sim, hero, DVec3::new(r0.x, r0.y + 0.2, r0.z));
    follow(&mut sim, me, hero, &l.spiral.route());
    eprintln!("after route {:?}", sim.position(hero));
    run(&mut sim, me, walk(DVec2::X), 0.6);
    run(&mut sim, me, InputState::default(), 0.5);
    let p = sim.position(hero).unwrap();
    assert!(
        (p.y - l.bottom).abs() < 0.4,
        "at {:.1}, foot {:.1}",
        p.y,
        l.bottom
    );
    // Wander: never into the rock, never through the floor.
    for a in 0..6 {
        run(&mut sim, me, walk(hw_core::det::dir2(a as f64 * 1.05)), 3.0);
        let p = sim.position(hero).unwrap();
        assert!(
            w.deep_open_at(DVec3::new(p.x, p.y + 1.0, p.z)),
            "out in the rock at {p:?}"
        );
        assert!(
            w.deep_floor_for(DVec2::new(p.x, p.z), p.y + 0.5)
                .is_some_and(|f| (p.y - f).abs() < 0.6),
            "off the floor at {p:?}"
        );
    }
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn a_capsule_takes_the_hero_to_another_station() {
    let town = GeneratedTown::generate(Seed(1), 40_000.0);
    let w = town.world.clone();
    let net = w.tube();
    let mut sim = Sim::for_town(Seed(1), &town);
    let me = PlayerId(1);
    let hero = sim.spawn_hero(me, sim.spawn_point(1));
    let (from, to) = (0..net.stations.len())
        .find_map(|a| {
            hw_sim::tube::reachable(net, a)
                .into_iter()
                .find(|&b| net.stations[b].pos.distance(net.stations[a].pos) > 500.0)
                .map(|b| (a, b))
        })
        .expect("two joined stations");
    let start = net.stations[from].hall_centre();
    put(
        &mut sim,
        hero,
        DVec3::new(start.x, net.stations[from].floor + 0.2, start.y),
    );
    run(&mut sim, me, InputState::default(), 1.0);
    sim.ride_tube(me, to as u16);
    assert!(sim.tubes.has_ride(me));
    let mut rode = false;
    for _ in 0..240 {
        run(&mut sim, me, InputState::default(), 0.5);
        if let Some(p) = sim.position(hero) {
            // Under the ground the whole way.
            let q = DVec2::new(p.x, p.z);
            assert!(p.y < w.ground(q) - 6.0, "came up at {p:?}");
        }
        if sim.tubes.destination(me).is_some() {
            rode = true;
        } else if rode {
            break;
        }
    }
    let p = sim.position(hero).unwrap();
    let end = net.stations[to].hall_centre();
    assert!(
        DVec2::new(p.x, p.z).distance(end) < 2.0,
        "ended at {p:?}, the hall is at {end}"
    );
    assert!((p.y - net.stations[to].floor).abs() < 0.6);
    // And walks on from there.
    run(&mut sim, me, walk(net.stations[to].dir), 1.0);
    let p2 = sim.position(hero).unwrap();
    assert!(p2.distance(p) > 2.0, "stuck after the ride");
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn out_of_the_tube_by_an_exit() {
    let town = GeneratedTown::generate(Seed(1), 40_000.0);
    let w = town.world.clone();
    let mut sim = Sim::for_town(Seed(1), &town);
    let me = PlayerId(1);
    let hero = sim.spawn_hero(me, sim.spawn_point(1));
    let (core, _) = w.spawn_line();
    // The nearest way up from the tube between stations.
    let (n, e) = w
        .deep_near(core, 900.0)
        .iter()
        .flat_map(|t| {
            t.nooks
                .iter()
                .filter(|n| n.kind == hw_gen::urban::deep::NookKind::Exit)
                .filter_map(|n| {
                    let e = t
                        .entrances
                        .iter()
                        .find(|e| e.kind == EntranceKind::Exit && e.pos == n.centre)?;
                    Some((n.clone(), e.clone()))
                })
                .collect::<Vec<_>>()
        })
        .min_by(|a, b| {
            a.0.mouth
                .distance(core)
                .total_cmp(&b.0.mouth.distance(core))
        })
        .expect("a tube exit");
    // On the walkway at its mouth, then along the passage into the stair.
    put(
        &mut sim,
        hero,
        DVec3::new(n.mouth.x, n.floor + 0.3, n.mouth.y),
    );
    run(&mut sim, me, InputState::default(), 1.0);
    let way = (e.pos - n.mouth).normalize();
    let mut steps = 0;
    while steps < 40 {
        let p = sim.position(hero).unwrap();
        let q = DVec2::new(p.x, p.z);
        if q.distance(e.pos) < SHAFT_HALF + 1.0 {
            break;
        }
        run(&mut sim, me, walk((e.pos - q).normalize_or(way)), 0.1);
        steps += 1;
    }
    let p = sim.position(hero).unwrap();
    assert!(
        DVec2::new(p.x, p.z).distance(e.pos) < SHAFT_HALF + 1.4,
        "could not walk to the stair: at {p:?}, stair at {}",
        e.pos
    );
    assert!((p.y - e.bottom).abs() < 0.6, "not on the stair's floor");
    // Up it: the foot's way, then the steps from the lowest.
    let passage = tile_passage(&w, &e);
    let mut way = e
        .spiral
        .foot_route(passage.as_ref())
        .expect("a way into the foot");
    way.reverse();
    let mut climb: Vec<DVec3> = way.iter().map(|q| DVec3::new(q.x, e.bottom, q.y)).collect();
    let mut steps_up = e.spiral.route();
    steps_up.reverse();
    climb.extend(steps_up);
    follow(&mut sim, me, hero, &climb);
    let p = sim.position(hero).unwrap();
    assert!(
        p.y > e.top - 0.5,
        "stair stopped at {:.1} (street {:.1})",
        p.y,
        e.top
    );
    run(&mut sim, me, walk(e.door), 1.0);
    run(&mut sim, me, InputState::default(), 1.0);
    let p = sim.position(hero).unwrap();
    let q = DVec2::new(p.x, p.z);
    assert!(
        (p.y - w.ground(q)).abs() < 0.6,
        "not out on the street: {:.1} over ground {:.1}",
        p.y,
        w.ground(q)
    );
}
