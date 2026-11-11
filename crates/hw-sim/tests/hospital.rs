//! A beaten hero is flown by the medical drone to the nearest hospital:
//! up over the roofs, across town, down onto the hospital's bay, into a
//! recovery pod, and out of it healed, inside the hall. A teammate can
//! still get them up first.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_net::{EntityKind, FxKind, InputState, PlayerId, Status};
use hw_sim::collision::Body;
use hw_sim::{GeneratedTown, Sim, TICK_HZ};

fn flat(p: DVec3) -> DVec2 {
    DVec2::new(p.x, p.z)
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn the_drone_flies_a_beaten_hero_to_the_hospital_and_a_pod_heals_them() {
    let town = GeneratedTown::generate(Seed(3), 20_000.0);
    let mut sim = Sim::for_town(Seed(3), &town);
    let hospital = town
        .world
        .landmarks
        .iter()
        .find(|l| l.ward().is_some())
        .expect("a hospital")
        .clone();
    let ward = hospital.ward().unwrap();
    let hero = sim.spawn_at_start(PlayerId(1));
    // Knocked out 250 m from the hospital, on the street.
    let solids = sim.solids();
    let away = flat(ward.bay) + DVec2::new(250.0, 0.0);
    let start = DVec3::new(away.x, solids.ground_at(away) + 0.5, away.y);
    sim.set_body(
        hero,
        Body {
            pos: start,
            ..Default::default()
        },
    );
    sim.step();
    let mut f = sim.fighter(hero).unwrap();
    f.health = 0.0;
    f.status = Status::Down;
    f.until = sim.tick;
    sim.set_fighter(hero, f);
    sim.set_input(
        PlayerId(1),
        InputState {
            hospital: true,
            ..Default::default()
        },
    );
    let mut seen = Vec::new();
    let mut highest: f64 = 0.0;
    let mut in_pod = 0u64;
    let mut medical_drone = false;
    for _ in 0..TICK_HZ as u64 * 120 {
        sim.step();
        seen.extend(sim.fx().iter().map(|e| e.kind));
        let p = sim.position(hero).unwrap();
        let f = sim.fighter(hero).unwrap();
        if f.status == Status::Carried && seen.contains(&FxKind::Admitted) {
            in_pod += 1;
            assert!(
                ward.pods.iter().any(|q| q.0.distance(p) < 0.01),
                "floats off the pod at {p}"
            );
        } else {
            highest = highest.max(p.y);
        }
        medical_drone |= sim
            .snapshot()
            .iter()
            .any(|e| e.kind == EntityKind::Drone && e.mount.is_some_and(|m| m.1 == 1));
        if seen.contains(&FxKind::Healed) {
            break;
        }
    }
    for k in [FxKind::Lifted, FxKind::Admitted, FxKind::Healed] {
        assert!(seen.contains(&k), "no {k:?} in {seen:?}");
    }
    assert!(medical_drone, "the drone is not told apart as medical");
    // It crossed above the hospital's pylon, then spent its time in a pod.
    assert!(
        highest > hospital.base + hw_gen::urban::hospital::PYLON_TOP,
        "flew at {highest:.0}"
    );
    let pod_secs = in_pod as f64 / TICK_HZ as f64;
    assert!(pod_secs > 3.0, "{pod_secs:.1} s in the pod");
    // Out of the pod, healed, inside the hall, on its floor.
    let f = sim.fighter(hero).unwrap();
    assert_eq!(f.status, Status::Standing);
    assert_eq!(f.health, f.max_health);
    let p = sim.position(hero).unwrap();
    assert!(
        ward.pods.iter().any(|q| q.1.distance(p) < 0.01),
        "not stepped out at a pod: {p}"
    );
    assert!(hw_gen::urban::hospital::ceiling_over(&hospital, p + DVec3::Y).is_some());
    assert!(!solids.solid_at(p + DVec3::Y));
    // The drone leaves.
    for _ in 0..TICK_HZ as u64 * 10 {
        sim.step();
    }
    assert!(sim.snapshot().iter().all(|e| e.kind != EntityKind::Drone));
    // And the hero can walk out through the door, round the desk.
    let local = |q: DVec3| {
        let d = flat(q) - hospital.pos;
        let x = DVec2::new(hospital.dir.y, -hospital.dir.x);
        DVec2::new(d.dot(x), d.dot(hospital.dir))
    };
    let side = local(p).x.signum();
    let beside = hospital.to_plan(DVec2::new(side * 4.3, hw_gen::urban::hospital::DESK_Z));
    let door = flat(ward.door);
    let mut goals = vec![beside, door];
    for _ in 0..TICK_HZ as u64 * 20 {
        let me = flat(sim.position(hero).unwrap());
        if me.distance(goals[0]) < 0.8 {
            if goals.len() == 1 {
                break;
            }
            goals.remove(0);
        }
        let d = (goals[0] - me).normalize_or_zero();
        sim.set_input(
            PlayerId(1),
            InputState {
                move_x: d.x as f32,
                move_z: d.y as f32,
                ..Default::default()
            },
        );
        sim.step();
    }
    let me = flat(sim.position(hero).unwrap());
    assert!(
        me.distance(door) < 1.0,
        "stuck at {me}, {:.1} m from the door",
        me.distance(door)
    );
}
