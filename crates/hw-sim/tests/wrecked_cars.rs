//! A hero's blast wrecks a car driving by: it stops where it was hit, the
//! cars behind it on its lane stand still, the news goes out, and it is
//! towed away once nobody has been near for a while.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_net::{InputState, PlayerId, ToClient};
use hw_sim::collision::Body;
use hw_sim::destruction::REPAIR_SECONDS;
use hw_sim::{GeneratedTown, Sim, DT, TICK_HZ};

#[test]
fn a_blast_wrecks_a_passing_car_and_holds_up_its_lane() {
    let town = GeneratedTown::generate(Seed(3), 20_000.0);
    let mut sim = Sim::for_town(Seed(3), &town);
    let me = PlayerId(1);
    let hero = sim.spawn_hero(me, sim.spawn_point(1));
    for _ in 0..TICK_HZ {
        sim.step();
    }
    let spawn = sim.position(hero).unwrap();
    // The nearest street car to the hero.
    let time = sim.tick as f64 * DT;
    let traffic = sim.wrecks.traffic.as_ref().unwrap();
    let car = traffic
        .near(DVec2::new(spawn.x, spawn.z), 400.0, time, 200)
        .into_iter()
        .find(|v| !v.flies())
        .expect("a car near the start");
    // Stand 8 m beside where it will be next tick, and blast it.
    let next = traffic.vehicle(car.id, (sim.tick + 1) as f64 * DT).unwrap();
    let side = DVec3::new(next.dir.z, 0.0, -next.dir.x).normalize();
    let stand = next.pos + side * 8.0;
    let ground = town.world.ground(DVec2::new(stand.x, stand.z));
    sim.set_body(
        hero,
        Body {
            pos: DVec3::new(stand.x, ground.max(next.pos.y), stand.z),
            vel: DVec3::ZERO,
            grounded: true,
        },
    );
    let chest = DVec3::new(stand.x, ground.max(next.pos.y) + 1.2, stand.z);
    let aim = (next.pos + DVec3::Y * 0.8 - chest).normalize();
    sim.set_input(
        me,
        InputState {
            blast: true,
            aim: aim.as_vec3().to_array(),
            ..Default::default()
        },
    );
    sim.step();
    sim.set_input(me, InputState::default());
    assert!(sim.wrecks.cars.contains_key(&car.id), "the car drove on");
    let news = sim.take_news();
    assert!(news.iter().any(|m| matches!(m,
        ToClient::CarsWrecked { cars, live: true } if cars.iter().any(|c| c.0 == car.id))));
    // It stands still, and the car behind it queues.
    let traffic = sim.wrecks.traffic.as_ref().unwrap();
    let t0 = sim.tick as f64 * DT;
    let wreck_at = traffic.vehicle(car.id, t0).unwrap().pos;
    let behind = traffic.behind(car.id);
    for _ in 0..(TICK_HZ * 40) {
        sim.step();
    }
    let traffic = sim.wrecks.traffic.as_ref().unwrap();
    let t1 = sim.tick as f64 * DT;
    let still = traffic.vehicle(car.id, t1).unwrap().pos;
    assert!(still.distance(wreck_at) < 1e-6, "the wreck moved");
    if let Some(v) = behind.and_then(|b| traffic.vehicle(b, t1)) {
        // Queued behind it (or turned off the road and gone, `None`).
        assert!(
            v.speed < 0.5,
            "drove through the wreck at {:.1} m/s",
            v.speed
        );
        assert!(v.pos.distance(wreck_at) > 4.0, "ran into the wreck");
    }
    // Leave town: it is towed away after a while.
    let far = DVec3::new(1e6, 0.0, 1e6);
    let later = sim.tick + (REPAIR_SECONDS * TICK_HZ as f64) as u64 + 1;
    sim.wrecks.repair_unseen(&[far], sim.tick, TICK_HZ);
    sim.wrecks.repair_unseen(&[far], later, TICK_HZ);
    assert!(sim.wrecks.cars.is_empty());
    let news = sim.wrecks.take_news();
    assert!(news
        .iter()
        .any(|m| matches!(m, ToClient::CarsCleared { cars } if cars.contains(&car.id))));
    for _ in 0..(TICK_HZ * 10) {
        sim.step();
    }
    let traffic = sim.wrecks.traffic.as_ref().unwrap();
    let t2 = sim.tick as f64 * DT;
    if let Some(v) = traffic.vehicle(car.id, t2) {
        assert!(
            v.pos.distance(wreck_at) > 5.0,
            "the towed car never drove on"
        );
    }
}
