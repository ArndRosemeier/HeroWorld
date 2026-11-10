//! Doors: shut ones stop people, heroes open them only within reach and
//! never shut one on someone, people walking up to a door open it and it
//! shuts behind them, and open doors survive a save.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_gen::building::BuildingUse;
use hw_gen::doors::DoorKind;
use hw_net::InputState;
use hw_sim::collision::{Body, FIXED};
use hw_sim::doors::{Doors, Refused, REACH};
use hw_sim::{step_hero, GeneratedCity, BUILDING_BASE, TICK_HZ};

/// A house's street door: (building id, door key, doorway centre, the way
/// into the house).
fn front_door(city: &GeneratedCity) -> (u64, u32, DVec3, DVec2) {
    for (i, p) in city.buildings.iter().enumerate() {
        if p.site.usage == BuildingUse::Offices {
            continue;
        }
        if let Some(d) = p
            .door_leaves()
            .into_iter()
            .find(|d| d.kind == DoorKind::Front && d.level == 0)
        {
            let at = p.site.to_city(DVec3::new(d.at.x, BUILDING_BASE, d.at.y));
            let into = p.site.along * d.into.x + p.site.inward * d.into.y;
            return (FIXED | i as u64, d.key, at, into);
        }
    }
    panic!("no house with a street door");
}

fn run(city: &GeneratedCity, body: &mut Body, dir: DVec2, ticks: u32) {
    for _ in 0..ticks {
        step_hero(
            body,
            InputState {
                move_x: dir.x as f32,
                move_z: dir.y as f32,
                ..Default::default()
            },
            &city.obstacles,
        );
    }
}

#[test]
fn a_shut_door_stops_you_and_an_open_one_lets_you_in() {
    let city = GeneratedCity::generate(Seed(1));
    let (b, key, at, into) = front_door(&city);
    let solids = &city.obstacles;
    let start = at - DVec3::new(into.x, 0.0, into.y) * 2.0 + DVec3::Y * 0.3;
    let mut body = Body {
        pos: start,
        ..Default::default()
    };
    run(&city, &mut body, into, TICK_HZ * 2);
    assert!(
        solids.inside(body.pos).is_none(),
        "through the shut door to {}",
        body.pos
    );
    assert!(solids.line_blocked(
        at - DVec3::new(into.x, -1.0, into.y),
        at + DVec3::new(into.x, 1.0, into.y)
    ));

    let mut doors = Doors::default();
    // From across the street: too far.
    let far = at - DVec3::new(into.x, 0.0, into.y) * (REACH + 3.0);
    assert_eq!(
        doors.hero_uses(solids, far, b, key, true, &[far]),
        Err(Refused::TooFar)
    );
    let v0 = solids.version(b);
    assert_eq!(
        doors.hero_uses(solids, body.pos, b, key, true, &[body.pos]),
        Ok(())
    );
    assert!(solids.door_open(b, key));
    assert_ne!(solids.version(b), v0, "physics sees the change");
    assert_eq!(doors.take_news().len(), 1);
    run(&city, &mut body, into, TICK_HZ * 2);
    assert!(
        solids.inside(body.pos).is_some(),
        "not in through the open door: {}",
        body.pos
    );

    // Standing in the doorway, the door does not shut on you.
    let mut body = Body {
        pos: at + DVec3::Y * 0.05,
        ..Default::default()
    };
    run(&city, &mut body, DVec2::ZERO, 5);
    assert_eq!(
        doors.hero_uses(solids, body.pos, b, key, false, &[body.pos]),
        Err(Refused::InTheWay)
    );
    // Stepped back out, it shuts; all shut, nothing is kept.
    let out = at - DVec3::new(into.x, 0.0, into.y) * 1.5;
    assert_eq!(doors.hero_uses(solids, out, b, key, false, &[out]), Ok(()));
    assert!(solids.open_doors().is_empty());
}

#[test]
fn people_walking_up_open_a_door_and_it_shuts_behind_them() {
    let city = GeneratedCity::generate(Seed(1));
    let (b, key, at, into) = front_door(&city);
    let solids = &city.obstacles;
    let mut doors = Doors::default();
    let walker = at - DVec3::new(into.x, 0.0, into.y) * 1.0;
    let vel = DVec3::new(into.x, 0.0, into.y) * 1.4;
    let hero = at + DVec3::X * 30.0;
    // Walking along the wall past it: stays shut.
    let along = DVec3::new(-into.y, 0.0, into.x) * 1.4;
    doors.step(
        solids,
        &[(walker, along)],
        &[walker, hero],
        &[hero],
        0,
        TICK_HZ as f64,
    );
    assert!(!solids.door_open(b, key));
    // Walking at it: opens.
    doors.step(
        solids,
        &[(walker, vel)],
        &[walker, hero],
        &[hero],
        3,
        TICK_HZ as f64,
    );
    assert!(solids.door_open(b, key));
    // While they stand by it, it stays open; once gone, it shuts.
    let t = 3 + 3 * TICK_HZ as u64;
    doors.step(solids, &[], &[walker, hero], &[hero], t, TICK_HZ as f64);
    assert!(solids.door_open(b, key));
    let later = t + 3 * TICK_HZ as u64;
    doors.step(solids, &[], &[hero], &[hero], later, TICK_HZ as f64);
    assert!(!solids.door_open(b, key), "left open behind them");
    let news = doors.take_news();
    assert_eq!(news.len(), 1, "one message with both changes");
}

#[test]
fn open_doors_are_saved() {
    let mut save =
        hw_sim::save::WorldSave::from_text("heroworld-save 3\nseed 4\npopulation 1.0\n").unwrap();
    save.doors = vec![(FIXED | 7, 4097), (12, 3)];
    let back = hw_sim::save::WorldSave::from_text(&save.to_text()).unwrap();
    assert_eq!(back.doors, save.doors);
}
