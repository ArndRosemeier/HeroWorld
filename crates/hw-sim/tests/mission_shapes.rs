//! The cases that are not hideouts, headless on flat ground: a heist
//! whose bagmen load a flying car that takes off, the chase after it, the
//! car brought down and its crew beaten; a heist stopped in time; a
//! getaway nobody follows.

use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_net::PlayerId;
use hw_sim::collision::{Body, Obstacles};
use hw_sim::missions::getaway::{CarState, Phase, ESCAPE_SECONDS, FLY_HEIGHT, TRIPS};
use hw_sim::missions::{self, CaseKind, Missions, Place, Site};
use hw_sim::{Sim, GROUND_Y, TICK_HZ};
use std::sync::Arc;

fn world() -> Sim {
    world_with(Vec::new())
}

/// Flat ground with `blocks` standing on it.
fn world_with(blocks: Vec<hw_sim::collision::Solid>) -> Sim {
    let mut o = Obstacles::flat(GROUND_Y);
    if !blocks.is_empty() {
        o.add_all(blocks);
    }
    let mut sim = Sim::with_solids(Seed(11), Arc::new(o));
    sim.set_spawn((DVec2::new(-400.0, -400.0), DVec2::X));
    // Cases need a town with a building: one stands far off, on paper only.
    let l = (0..16u64)
        .find_map(|k| {
            hw_gen::layout::office::plan_scheme(
                Seed(60 + k),
                DVec2::new(56.0, 52.0),
                5,
                hw_gen::layout::Scheme::Atrium,
            )
        })
        .expect("it fits");
    let floors = Arc::new(hw_sim::floornav::Floors::of(vec![
        hw_sim::floornav::Building::at(
            DVec2::new(3000.0, 3000.0),
            DVec2::new(0.0, -1.0),
            GROUND_Y,
            hw_sim::floornav::FloorGraph::new(&l),
        ),
    ]));
    sim.missions = Missions::new(floors, 11);
    sim
}

fn site(place: Place, at: DVec3, facing: DVec2) -> Site {
    Site {
        place,
        name: "the test place".into(),
        at,
        facing,
        way_down: None,
        around: Vec::new(),
        building: None,
    }
}

fn put(sim: &mut Sim, hero: NetId, at: DVec3) {
    sim.set_body(
        hero,
        Body {
            pos: at,
            grounded: true,
            ..Body::default()
        },
    );
    if let Some(mut f) = sim.fighter(hero) {
        f.health = f.max_health;
        sim.set_fighter(hero, f);
    }
}

/// The crew of fight spot `spot` goes down (cuffed).
fn put_down(sim: &mut Sim, spot: usize) {
    for (en, mut f) in sim
        .world
        .query::<(&hw_sim::combat::Enemy, &mut hw_sim::combat::Fighter)>()
        .iter_mut(&mut sim.world)
    {
        if en.spot == spot {
            f.health = 0.0;
            f.status = hw_net::Status::Down;
            f.until = 0;
        }
    }
}

fn phase(sim: &Sim, number: u16) -> Option<Phase> {
    sim.missions.case(number).and_then(|r| r.getaway_phase())
}

fn car(sim: &Sim, number: u16) -> (NetId, DVec3, CarState) {
    sim.missions
        .case(number)
        .and_then(|r| r.getaway_car())
        .expect("a car")
}

/// Steps `seconds` with the hero held at `at` (or, with `follow`, that far
/// behind the car).
fn wait(sim: &mut Sim, hero: NetId, number: u16, at: DVec3, follow: bool, seconds: f64) {
    for _ in 0..(seconds * TICK_HZ as f64) as u64 {
        let p = match sim.missions.case(number).and_then(|r| r.getaway_car()) {
            Some((_, c, _)) if follow => DVec3::new(c.x + at.x, GROUND_Y, c.z + at.z),
            _ => at,
        };
        put(sim, hero, p);
        sim.step();
    }
}

#[test]
fn a_heist_loads_its_car_takes_off_and_is_brought_down() {
    let mut sim = world();
    let hero = sim.spawn_hero(PlayerId(1), DVec3::new(-400.0, GROUND_Y, -400.0));
    let door = DVec3::new(0.0, GROUND_Y, 0.0);
    let number = sim.missions.force_site(
        &mut sim.world,
        CaseKind::Heist,
        site(Place::Shopfront, door, DVec2::Y),
        1,
    );
    // Watching from down the street: near enough for the gang to be
    // there, too far to be noticed.
    let watch = DVec3::new(70.0, GROUND_Y, 20.0);
    wait(&mut sim, hero, number, watch, false, 4.0);
    assert_eq!(phase(&sim, number), Some(Phase::Loading));
    let (id, at, state) = car(&sim, number);
    assert_eq!(state, CarState::Parked);
    assert!(
        at.distance(door) < 30.0 && (at.y - GROUND_Y).abs() < 0.2,
        "the car stands by the door: {at:.1?}"
    );
    // It is a target, and it stands in the way.
    assert!(sim
        .enemies()
        .iter()
        .any(|e| e.0 == id && e.1 == hw_sim::combat::EnemyKind::Getaway));
    assert_eq!(sim.missions.safe_solids().len(), 1);
    // Guards, lookouts and two bagmen.
    let r = sim.missions.case(number).unwrap();
    let (lookouts, guards, _, _) = r.spots();
    assert!(lookouts.is_some() && guards.len() == 1 && r.crew_spot().is_some());
    // Left alone, the bagmen load it sack by sack ...
    let mut seen = 0;
    let mut seconds = 0.0;
    while phase(&sim, number) == Some(Phase::Loading) {
        wait(&mut sim, hero, number, watch, false, 1.0);
        seconds += 1.0;
        let now = sim.missions.case(number).unwrap().loot_loaded();
        assert!(now >= seen, "sacks do not come out again");
        seen = now;
        assert!(seconds < 240.0, "{seen} of {TRIPS} sacks after {seconds} s");
    }
    assert_eq!(seen, TRIPS);
    assert!(seconds > 25.0, "loaded in {seconds} s: no time to stop it");
    // ... then everyone climbs in and it takes off.
    assert!(matches!(phase(&sim, number), Some(Phase::Boarding(_))));
    let before = sim.enemies().len();
    wait(&mut sim, hero, number, watch, false, 12.0);
    assert_eq!(phase(&sim, number), Some(Phase::Flying));
    assert!(
        sim.enemies().len() < before,
        "nobody climbed in: {} of {before} still there",
        sim.enemies().len()
    );
    // The chase: it flies low over the street and keeps going.
    let from = car(&sim, number).1;
    wait(
        &mut sim,
        hero,
        number,
        DVec3::new(-20.0, 0.0, 0.0),
        true,
        10.0,
    );
    let (_, at, state) = car(&sim, number);
    assert_eq!(state, CarState::Flying);
    assert!(
        (at.y - GROUND_Y - FLY_HEIGHT).abs() < 0.5,
        "it flies at {:.1} m",
        at.y - GROUND_Y
    );
    assert!(
        at.distance(from) > 40.0,
        "it went {:.0} m in 10 s",
        at.distance(from)
    );
    // Hurt, it sputters: slower and lower for a moment.
    let mut f = sim.fighter(id).unwrap();
    f.health = f.max_health * 0.7;
    sim.set_fighter(id, f);
    wait(
        &mut sim,
        hero,
        number,
        DVec3::new(-20.0, 0.0, 0.0),
        true,
        2.0,
    );
    let low = car(&sim, number).1;
    assert!(
        low.y - GROUND_Y < FLY_HEIGHT - 1.5,
        "it sinks: {:.1}",
        low.y
    );
    // Out of health it comes down and its crew gets out.
    let mut f = sim.fighter(id).unwrap();
    f.health = 0.0;
    f.status = hw_net::Status::Down;
    sim.set_fighter(id, f);
    wait(
        &mut sim,
        hero,
        number,
        DVec3::new(-25.0, 0.0, 0.0),
        true,
        6.0,
    );
    let (_, at, state) = car(&sim, number);
    assert_eq!(state, CarState::Wrecked);
    assert!((at.y - GROUND_Y).abs() < 0.2, "it lies on the street");
    assert_eq!(phase(&sim, number), Some(Phase::Down));
    let crew = sim.missions.case(number).unwrap().crew_spot().unwrap();
    let out = sim
        .world
        .query::<&hw_sim::combat::Enemy>()
        .iter(&sim.world)
        .filter(|e| e.spot == crew)
        .count();
    assert!(out >= 3, "{out} got out");
    // Beat them: case closed, the wreck stays for now.
    let karma = sim.sheet(PlayerId(1)).expect("a sheet").karma;
    put_down(&mut sim, crew);
    wait(
        &mut sim,
        hero,
        number,
        DVec3::new(-25.0, 0.0, 0.0),
        true,
        2.0,
    );
    assert!(sim.missions.case(number).is_none(), "the case is closed");
    assert_eq!(sim.missions.closed, 1);
    assert!(
        sim.sheet(PlayerId(1)).expect("a sheet").karma > karma,
        "it pays"
    );
    assert_eq!(sim.missions.safe_solids().len(), 1, "the wreck is solid");
}

#[test]
fn a_heist_whose_bagmen_are_beaten_never_takes_off() {
    let mut sim = world();
    let hero = sim.spawn_hero(PlayerId(1), DVec3::new(-400.0, GROUND_Y, -400.0));
    let number = sim.missions.force_site(
        &mut sim.world,
        CaseKind::Heist,
        site(Place::Shopfront, DVec3::new(0.0, GROUND_Y, 0.0), DVec2::Y),
        1,
    );
    let watch = DVec3::new(70.0, GROUND_Y, 20.0);
    wait(&mut sim, hero, number, watch, false, 6.0);
    assert_eq!(phase(&sim, number), Some(Phase::Loading));
    let r = sim.missions.case(number).unwrap();
    let bagmen = r.crew_spot().unwrap();
    let (lookouts, guards, _, _) = r.spots();
    put_down(&mut sim, bagmen);
    wait(&mut sim, hero, number, watch, false, 40.0);
    let loaded = sim.missions.case(number).unwrap().loot_loaded();
    assert!(loaded < TRIPS, "nobody carries: {loaded} sacks");
    assert_eq!(phase(&sim, number), Some(Phase::Loading));
    // The rest beaten too: caught in the act, and the car never flew.
    put_down(&mut sim, lookouts.unwrap());
    put_down(&mut sim, guards[0]);
    wait(&mut sim, hero, number, watch, false, 2.0);
    assert!(sim.missions.case(number).is_none());
    assert_eq!((sim.missions.closed, sim.missions.lost_cases), (1, 0));
}

#[test]
fn a_car_broken_at_the_kerb_takes_nobody_away() {
    let mut sim = world();
    let hero = sim.spawn_hero(PlayerId(1), DVec3::new(-400.0, GROUND_Y, -400.0));
    let number = sim.missions.force_site(
        &mut sim.world,
        CaseKind::Heist,
        site(Place::Shopfront, DVec3::new(0.0, GROUND_Y, 0.0), DVec2::Y),
        1,
    );
    let watch = DVec3::new(70.0, GROUND_Y, 20.0);
    wait(&mut sim, hero, number, watch, false, 6.0);
    let (id, _, _) = car(&sim, number);
    let mut f = sim.fighter(id).unwrap();
    f.health = 0.0;
    f.status = hw_net::Status::Down;
    sim.set_fighter(id, f);
    wait(&mut sim, hero, number, watch, false, 3.0);
    assert_eq!(car(&sim, number).2, CarState::Wrecked);
    assert_eq!(phase(&sim, number), Some(Phase::Down));
    wait(&mut sim, hero, number, watch, false, 60.0);
    assert_eq!(phase(&sim, number), Some(Phase::Down), "it stays down");
    assert!(sim.missions.case(number).is_some(), "the gang still stands");
}

#[test]
fn a_getaway_nobody_catches_is_lost() {
    let mut sim = world();
    let hero = sim.spawn_hero(PlayerId(1), DVec3::new(-400.0, GROUND_Y, -400.0));
    let at = DVec3::new(0.0, GROUND_Y, 0.0);
    let number = sim.missions.force_site(
        &mut sim.world,
        CaseKind::Chase,
        site(Place::Road, at, DVec2::X),
        1,
    );
    wait(
        &mut sim,
        hero,
        number,
        DVec3::new(30.0, GROUND_Y, 0.0),
        false,
        3.0,
    );
    assert_eq!(phase(&sim, number), Some(Phase::Flying));
    assert_eq!(car(&sim, number).2, CarState::Flying);
    // Followed but never hit, it is away after its time.
    wait(
        &mut sim,
        hero,
        number,
        DVec3::new(-30.0, 0.0, 0.0),
        true,
        ESCAPE_SECONDS + 3.0,
    );
    assert!(sim.missions.case(number).is_none());
    assert_eq!((sim.missions.closed, sim.missions.lost_cases), (0, 1));
    // The car is gone with it.
    assert!(sim
        .enemies()
        .iter()
        .all(|e| e.1 != hw_sim::combat::EnemyKind::Getaway));
    // And one let out of sight is lost sooner.
    let number = sim.missions.force_site(
        &mut sim.world,
        CaseKind::Chase,
        site(Place::Road, at, DVec2::X),
        1,
    );
    wait(
        &mut sim,
        hero,
        number,
        DVec3::new(30.0, GROUND_Y, 0.0),
        false,
        3.0,
    );
    wait(
        &mut sim,
        hero,
        number,
        DVec3::new(-900.0, GROUND_Y, -900.0),
        false,
        missions::AWAY_SECONDS + 3.0,
    );
    assert_eq!(sim.missions.lost_cases, 2);
}

#[test]
fn nobody_comes_to_a_heist_in_time_and_it_is_lost() {
    let mut sim = world();
    let hero = sim.spawn_hero(PlayerId(1), DVec3::new(-400.0, GROUND_Y, -400.0));
    let number = sim.missions.force_site(
        &mut sim.world,
        CaseKind::Heist,
        site(Place::Shopfront, DVec3::new(0.0, GROUND_Y, 0.0), DVec2::Y),
        1,
    );
    let far = DVec3::new(-400.0, GROUND_Y, -400.0);
    wait(&mut sim, hero, number, far, false, 2.0);
    // The clock is on the air, counting down.
    let left = |sim: &Sim| {
        sim.missions.snapshot().iter().find_map(|m| match m {
            hw_net::ToClient::Case {
                number: n, left, ..
            } if *n == number => Some(*left),
            _ => None,
        })
    };
    let first = left(&sim).expect("the case is on the air");
    assert!(first > 120, "{first} s to get there");
    wait(&mut sim, hero, number, far, false, 60.0);
    assert!(left(&sim).unwrap() < first - 50);
    wait(&mut sim, hero, number, far, false, first as f64);
    assert!(sim.missions.case(number).is_none());
    assert_eq!((sim.missions.closed, sim.missions.lost_cases), (0, 1));
}

/// A block `size` across and `height` high, its middle at `mid`.
fn block(mid: DVec2, size: DVec2, height: f64) -> hw_sim::collision::Solid {
    hw_sim::collision::Solid {
        origin: mid,
        along: DVec2::X,
        inward: DVec2::Y,
        min: -size * 0.5,
        max: size * 0.5,
        y0: GROUND_Y,
        y1: GROUND_Y + height,
    }
}

fn hold(sim: &mut Sim, hero: NetId, at: DVec3, seconds: f64) {
    sim.set_input(
        PlayerId(1),
        hw_net::InputState {
            revive: true,
            ..hw_net::InputState::default()
        },
    );
    for _ in 0..(seconds * TICK_HZ as f64) as usize {
        put(sim, hero, at);
        sim.step();
    }
    sim.set_input(PlayerId(1), hw_net::InputState::default());
    sim.step();
}

fn bystanders(sim: &mut Sim, ids: &[u32]) -> Vec<DVec3> {
    sim.world
        .query::<(&hw_sim::Motion, &hw_sim::incidents::Bystander)>()
        .iter(&sim.world)
        .filter(|(_, b)| ids.contains(&b.incident))
        .map(|(m, _)| m.0.pos)
        .collect()
}

fn cruisers(sim: &mut Sim) -> Vec<(DVec3, bool)> {
    sim.world
        .query::<(&hw_sim::Motion, &hw_sim::police::Cruiser)>()
        .iter(&sim.world)
        .map(|(m, _)| (m.0.pos, m.0.grounded))
        .collect()
}

#[test]
fn hostages_on_a_roof_are_flown_off_by_sky_patrol() {
    let height = 14.0;
    let top = GROUND_Y + height;
    let mut sim = world_with(vec![block(DVec2::ZERO, DVec2::new(34.0, 30.0), height)]);
    let hero = sim.spawn_hero(PlayerId(1), DVec3::new(-400.0, GROUND_Y, -400.0));
    // The plan says the roof is a little lower than it was built.
    let on = |x: f64, z: f64| DVec3::new(x, top - 0.4, z);
    let mut roof = site(Place::Rooftop, on(0.0, 0.0), DVec2::Y);
    roof.around = vec![
        on(0.0, -10.0),
        on(-11.0, 8.0),
        on(11.0, 8.0),
        on(11.0, -8.0),
        on(-11.0, -8.0),
        on(0.0, 10.0),
    ];
    let number = sim
        .missions
        .force_site(&mut sim.world, CaseKind::Hostages, roof, 1);
    let corner = DVec3::new(-15.0, top, -13.0);
    wait(&mut sim, hero, number, corner, false, 3.0);
    let r = sim.missions.case(number).expect("open");
    assert_eq!(r.stage, missions::INSIDE);
    let ids = r.hostages();
    assert!(!ids.is_empty(), "nobody held");
    let (_, rooms, _, _) = r.spots();
    // Everyone stands on the roof as built.
    for (_, _, at, _) in sim.enemies() {
        assert!((at.y - top).abs() < 0.3, "a villain at {at:.1?}");
    }
    for p in bystanders(&mut sim, &ids) {
        assert!((p.y - top).abs() < 0.3, "a hostage at {p:.1?}");
    }
    assert!(sim.missions.case(number).unwrap().lift().is_none());
    assert!(cruisers(&mut sim).is_empty());
    // The crews beaten: a cruiser sets out and lands on the roof.
    for &s in &rooms {
        put_down(&mut sim, s);
    }
    wait(&mut sim, hero, number, corner, false, 2.0);
    assert!(sim.missions.case(number).unwrap().lift().is_some());
    let held = sim.missions.case(number).unwrap().clues();
    // Freed at once, they wait for it rather than walk off the edge.
    for (at, _) in &held {
        hold(&mut sim, hero, *at, missions::FREE_SECONDS * 1.3);
    }
    assert!(
        sim.missions
            .case(number)
            .is_none_or(|r| r.clues().iter().all(|c| c.1)),
        "not all freed"
    );
    wait(&mut sim, hero, number, corner, false, 1.5);
    assert!(sim.missions.case(number).is_none(), "the case is closed");
    assert_eq!(sim.missions.closed, 1);
    let mut landed = false;
    let mut aboard_at = Vec::new();
    for _ in 0..TICK_HZ * 80 {
        put(&mut sim, hero, corner);
        sim.step();
        let people = bystanders(&mut sim, &ids);
        for p in &people {
            assert!(
                (p.y - top).abs() < 0.4 && p.x.abs() < 17.0 && p.z.abs() < 15.0,
                "a hostage left the roof at {p:.1?}"
            );
        }
        let cars = cruisers(&mut sim);
        if let Some(&(at, open)) = cars.first() {
            if open {
                landed = true;
                assert!((at.y - top).abs() < 0.05, "it stands at {at:.1?}");
                // Standing, it is solid.
                assert!(sim.missions.safe_solids().len() == 1);
            }
            if !people.is_empty() {
                aboard_at = people;
            }
        } else if landed {
            break;
        }
    }
    assert!(landed, "no cruiser landed");
    assert!(bystanders(&mut sim, &ids).is_empty(), "left behind");
    assert!(cruisers(&mut sim).is_empty(), "it never left");
    let pad = aboard_at.first().copied().expect("someone walked");
    assert!(pad.y > top - 0.5);
}

/// The courier of case `number`.
fn courier(sim: &mut Sim, number: u16) -> Option<(NetId, DVec3)> {
    let r = sim.missions.case(number)?.clone();
    r.courier(&mut sim.world)
}

fn tail_site(to: DVec3) -> Site {
    let mut s = site(Place::Street, DVec3::new(0.0, GROUND_Y, 0.0), DVec2::X);
    s.around = vec![to];
    s
}

#[test]
fn a_courier_followed_unseen_leads_to_the_door() {
    use hw_sim::missions::tail::{Walk, SEE, TRAIL};
    let mut sim = world();
    let hero = sim.spawn_hero(PlayerId(1), DVec3::new(-400.0, GROUND_Y, -400.0));
    let goal = DVec3::new(120.0, GROUND_Y, 0.0);
    let number = sim
        .missions
        .force_site(&mut sim.world, CaseKind::Tail, tail_site(goal), 1);
    let behind = |sim: &mut Sim, by: f64| {
        let at = courier(sim, number).map_or(DVec3::ZERO, |c| c.1);
        DVec3::new(at.x - by, GROUND_Y, at.z)
    };
    put(&mut sim, hero, DVec3::new(-50.0, GROUND_Y, 0.0));
    for _ in 0..TICK_HZ * 3 {
        let p = DVec3::new(-50.0, GROUND_Y, 0.0);
        put(&mut sim, hero, p);
        sim.step();
    }
    let (id, from) = courier(&mut sim, number).expect("a courier");
    // Followed at a distance: he walks all the way and goes in.
    let mut stopped = 0;
    let mut seconds = 0.0;
    while sim.missions.case(number).is_some() {
        let p = behind(&mut sim, (SEE * 1.6 + TRAIL) * 0.5);
        put(&mut sim, hero, p);
        sim.step();
        seconds += 1.0 / TICK_HZ as f64;
        let Some(r) = sim.missions.case(number) else {
            break;
        };
        let j = r.tail().expect("a tail");
        assert!(j.suspicion < 0.01, "seen from afar: {}", j.suspicion);
        if matches!(j.walk, Walk::Looking(_)) {
            stopped += 1;
        }
        assert!(seconds < 200.0, "still walking at {:.0?}", j.at);
    }
    assert!(stopped > TICK_HZ as usize * 4, "he never looked about");
    assert!(
        seconds > 30.0,
        "{seconds} s for {:.0} m",
        from.distance(goal)
    );
    // The lone step was the whole case: closed, and he is gone.
    assert_eq!((sim.missions.closed, sim.missions.lost_cases), (1, 0));
    assert!(sim.enemies().iter().all(|e| e.0 != id), "he went in");
}

#[test]
fn a_courier_who_sees_his_tail_runs_and_can_be_caught_or_lost() {
    use hw_sim::missions::tail::{Walk, AWAY_RUNNING};
    for catch in [true, false] {
        let mut sim = world();
        let hero = sim.spawn_hero(PlayerId(1), DVec3::new(-400.0, GROUND_Y, -400.0));
        let goal = DVec3::new(200.0, GROUND_Y, 0.0);
        let number = sim
            .missions
            .force_site(&mut sim.world, CaseKind::Tail, tail_site(goal), 1);
        for _ in 0..TICK_HZ * 3 {
            put(&mut sim, hero, DVec3::new(-50.0, GROUND_Y, 0.0));
            sim.step();
        }
        // Right on his heels: he notices within a few seconds.
        let mut seconds = 0.0;
        loop {
            let at = courier(&mut sim, number).expect("a courier").1;
            put(&mut sim, hero, DVec3::new(at.x - 12.0, GROUND_Y, at.z));
            sim.step();
            seconds += 1.0 / TICK_HZ as f64;
            let j = sim.missions.case(number).unwrap().tail().unwrap().clone();
            if matches!(j.walk, Walk::Running(_)) {
                break;
            }
            assert!(
                seconds < 12.0,
                "12 m behind him for {seconds} s: {}",
                j.suspicion
            );
        }
        assert!(seconds > 2.0, "seen in {seconds} s");
        // On the air: the gauge is full and he is the one to follow.
        let news = sim.missions.snapshot();
        let (mark, gauge) = news
            .iter()
            .find_map(|m| match m {
                hw_net::ToClient::Case {
                    number: n,
                    mark,
                    gauge,
                    ..
                } if *n == number => Some((*mark, *gauge)),
                _ => None,
            })
            .expect("news");
        assert_eq!(mark, courier(&mut sim, number).unwrap().0 .0);
        assert_eq!(gauge, 255);
        let (_, from) = courier(&mut sim, number).unwrap();
        if catch {
            // He runs, away from the hero; beaten, he talks.
            let stay = DVec3::new(from.x - 12.0, GROUND_Y, from.z);
            for _ in 0..TICK_HZ * 4 {
                put(&mut sim, hero, stay);
                sim.step();
            }
            let now = courier(&mut sim, number).unwrap().1;
            assert!(now.x > from.x + 8.0, "he ran from {from:.0?} to {now:.0?}");
            let spot = sim.missions.case(number).unwrap().spots().1[0];
            put_down(&mut sim, spot);
            for _ in 0..TICK_HZ * 3 {
                put(&mut sim, hero, now);
                sim.step();
            }
            assert_eq!((sim.missions.closed, sim.missions.lost_cases), (1, 0));
        } else {
            // Let go, he is away and the case with him.
            let stay = DVec3::new(from.x - 12.0, GROUND_Y, from.z);
            let mut t = 0.0;
            while sim.missions.case(number).is_some() {
                put(&mut sim, hero, stay);
                sim.step();
                t += 1.0 / TICK_HZ as f64;
                assert!(t < 60.0, "never away");
            }
            assert_eq!((sim.missions.closed, sim.missions.lost_cases), (0, 1));
            let _ = AWAY_RUNNING;
        }
    }
}

#[test]
fn a_trail_let_go_cold_is_lost_and_a_roof_hides_a_hero() {
    use hw_sim::missions::tail::{suspicion_rate, ABOVE, CLOSE, SEE, TRAIL_SECONDS};
    assert!(suspicion_rate(CLOSE - 1.0, false) > suspicion_rate(SEE - 1.0, false));
    assert_eq!(suspicion_rate(SEE + 1.0, false), 0.0);
    assert!(suspicion_rate(SEE + 1.0, true) > 0.0);
    let mut sim = world();
    let hero = sim.spawn_hero(PlayerId(1), DVec3::new(-400.0, GROUND_Y, -400.0));
    let goal = DVec3::new(600.0, GROUND_Y, 0.0);
    let number = sim
        .missions
        .force_site(&mut sim.world, CaseKind::Tail, tail_site(goal), 1);
    for _ in 0..TICK_HZ * 3 {
        put(&mut sim, hero, DVec3::new(-50.0, GROUND_Y, 0.0));
        sim.step();
    }
    // Right over his head he is not seen.
    for _ in 0..TICK_HZ * 10 {
        let at = courier(&mut sim, number).expect("a courier").1;
        sim.set_body(
            hero,
            Body {
                pos: at + DVec3::Y * (ABOVE + 3.0),
                grounded: false,
                ..Body::default()
            },
        );
        sim.step();
    }
    let j = sim.missions.case(number).unwrap().tail().unwrap().clone();
    assert!(j.suspicion < 0.01, "seen from above: {}", j.suspicion);
    // Standing still while he walks on: the trail goes cold and is lost.
    let stay = DVec3::new(j.at.x - 60.0, GROUND_Y, j.at.z);
    let mut t = 0.0;
    let mut counted = false;
    while sim.missions.case(number).is_some() {
        put(&mut sim, hero, stay);
        sim.step();
        t += 1.0 / TICK_HZ as f64;
        counted |= sim.missions.snapshot().iter().any(|m| {
            matches!(m, hw_net::ToClient::Case { number: n, left, .. } if *n == number && *left > 0)
        });
        assert!(t < 60.0 + TRAIL_SECONDS, "never lost");
    }
    assert!(counted, "no countdown on the air");
    assert!(t > TRAIL_SECONDS, "lost in {t} s");
    assert_eq!((sim.missions.closed, sim.missions.lost_cases), (0, 1));
}
