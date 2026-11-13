//! The police, headless: a Sky Patrol cruiser called to a door flies in,
//! lands on open ground near it and is solid there, two officers get out
//! and walk to posts by the door, stand guard, walk back, and the cruiser
//! flies off. While it stands in a street the traffic stops for it.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_net::{EntityKind, EntityState, PlayerId};
use hw_sim::collision::{Obstacles, Solid};
use hw_sim::police::{self, cruiser_solid, CRUISER_HALF, CRUISER_HEIGHT};
use hw_sim::{GeneratedTown, Sim, GROUND_Y, TICK_HZ};
use std::sync::Arc;

/// A box from `min` to `max` on the ground, `height` high.
fn block(min: DVec2, max: DVec2, height: f64) -> Solid {
    Solid {
        origin: DVec2::ZERO,
        along: DVec2::X,
        inward: DVec2::Y,
        min,
        max,
        y0: GROUND_Y,
        y1: GROUND_Y + height,
    }
}

fn of(all: &[EntityState], kind: EntityKind) -> Vec<EntityState> {
    all.iter().filter(|e| e.kind == kind).cloned().collect()
}

#[test]
fn a_cruiser_lands_by_the_door_and_its_officers_stand_guard() {
    // A house front along x at z = 0 (the house behind it, +z), its door
    // at the origin looking out to -z; a wall right in front of the door
    // a little way off, so the cruiser has to stand beside it.
    let mut o = Obstacles::flat(GROUND_Y);
    o.add(block(DVec2::new(-30.0, 0.0), DVec2::new(30.0, 20.0), 12.0));
    o.add(block(DVec2::new(-3.0, -12.0), DVec2::new(3.0, -5.0), 3.0));
    let o = Arc::new(o);
    let mut sim = Sim::with_solids(Seed(4), o.clone());
    sim.spawn_hero(PlayerId(1), DVec3::new(0.0, GROUND_Y, -30.0));
    sim.step();
    let door = DVec3::new(0.0, GROUND_Y, -0.2);
    let out = DVec2::new(0.0, -1.0);
    sim.call_police(door, out);
    // A second call to the same door brings no second cruiser.
    sim.call_police(door + DVec3::X, out);

    let mut landed = None;
    let mut posts: Vec<DVec3> = Vec::new();
    let mut seen_open = false;
    let mut flew = false;
    let mut gone = false;
    for _ in 0..TICK_HZ * 150 {
        sim.step();
        let all = sim.snapshot();
        let cars = of(&all, EntityKind::PoliceCar);
        let officers = of(&all, EntityKind::Officer);
        assert!(cars.len() <= 1, "one cruiser for one place");
        assert!(officers.len() <= 2);
        let Some(car) = cars.first() else {
            if flew {
                gone = true;
                assert!(officers.is_empty(), "nobody is left behind");
                break;
            }
            continue;
        };
        flew = true;
        let pos = DVec3::from_array(car.pos);
        let still = glam::Vec3::from_array(car.vel).length() < police::STANDING as f32;
        if still && pos.y < GROUND_Y + 0.5 {
            landed.get_or_insert(pos);
            // Standing, it is solid: the box the model is built in.
            let yaw = hw_sim::robots::rotation_yaw(car.rot);
            let b = cruiser_solid(pos, yaw);
            let mid = b.to_world(DVec2::ZERO);
            assert!(
                sim.solids()
                    .solid_at(DVec3::new(mid.x, pos.y + CRUISER_HEIGHT * 0.5, mid.y)),
                "a landed cruiser is solid"
            );
        }
        // Officers are only ever out of a cruiser that stands, canopy up.
        if !officers.is_empty() {
            assert!(still && car.grounded, "out only while it stands open");
            seen_open = true;
            for e in &officers {
                let p = DVec3::from_array(e.pos);
                assert!((p.y - GROUND_Y).abs() < 0.05, "on the ground: {p}");
                for h in [0.4, 1.3] {
                    assert!(!o.solid_at(p + DVec3::Y * h), "never inside a wall: {p}");
                }
                if glam::Vec3::from_array(e.vel).length() < 0.01 && p.distance(door) < 6.0 {
                    if !posts.iter().any(|q| q.distance(p) < 0.1) {
                        posts.push(p);
                    }
                    // On guard they face the street.
                    let yaw = hw_sim::robots::rotation_yaw(e.rot);
                    assert!(DVec2::new(yaw.sin(), yaw.cos()).dot(out) > 0.99);
                }
            }
        }
    }
    let landed = landed.expect("it landed");
    assert!(seen_open, "the officers got out");
    assert!(gone, "and in the end it flew off");
    assert_eq!(posts.len(), 2, "two posts by the door: {posts:?}");
    assert!(posts[0].distance(posts[1]) > 1.5, "either side of the door");
    assert!(
        landed.distance(door) < 30.0 && landed.distance(door) > CRUISER_HALF.x + 2.0,
        "near the door, not on it: {landed}"
    );
    assert_eq!(sim.police.out(), 0);
}

#[test]
fn with_no_hero_near_the_police_are_gone() {
    let o = Arc::new(Obstacles::flat(GROUND_Y));
    let mut sim = Sim::with_solids(Seed(4), o);
    let hero = sim.spawn_hero(PlayerId(1), DVec3::new(0.0, GROUND_Y, -30.0));
    sim.step();
    sim.call_police(DVec3::new(0.0, GROUND_Y, 0.0), DVec2::new(0.0, -1.0));
    for _ in 0..TICK_HZ * 5 {
        sim.step();
    }
    assert_eq!(of(&sim.snapshot(), EntityKind::PoliceCar).len(), 1);
    sim.set_body(
        hero,
        hw_sim::collision::Body {
            pos: DVec3::new(900.0, GROUND_Y, 0.0),
            vel: DVec3::ZERO,
            grounded: true,
        },
    );
    sim.step();
    sim.step();
    let all = sim.snapshot();
    assert!(of(&all, EntityKind::PoliceCar).is_empty());
    assert!(of(&all, EntityKind::Officer).is_empty());
    assert_eq!(sim.police.out(), 0);
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn cruisers_find_ground_by_the_towns_hideouts() {
    let seed = Seed(3);
    let town = GeneratedTown::generate(seed, 20_000.0);
    let sim = Sim::for_town(seed, &town);
    let places = sim.missions.places().expect("cases in a town").clone();
    let solids = sim.solids();
    let (mut found, mut none) = (0, 0);
    let mut worst = 0.0f64;
    let mut why = std::collections::BTreeMap::new();
    for &k in &places.hideouts {
        let b = &places.floors.buildings[k];
        let door = b.to_town(b.graph.front());
        let started = std::time::Instant::now();
        let landing = police::find_landing(Some(&town.area), &solids, door, b.dir);
        worst = worst.max(started.elapsed().as_secs_f64());
        let Some(l) = landing else {
            none += 1;
            for (k, n) in police::why_not(Some(&town.area), &solids, door, b.dir) {
                *why.entry(k).or_insert(0usize) += n;
            }
            continue;
        };
        found += 1;
        assert_eq!(
            Some(&l),
            police::find_landing(Some(&town.area), &solids, door, b.dir).as_ref(),
            "the same place every time"
        );
        // The whole box stands in nothing solid.
        let s = cruiser_solid(l.at, l.yaw);
        for i in -2..=2 {
            for j in -4..=4 {
                let q = s.to_world(DVec2::new(
                    CRUISER_HALF.x * i as f64 / 2.0,
                    CRUISER_HALF.y * j as f64 / 4.0,
                ));
                assert!(!solids.solid_at(DVec3::new(q.x, l.at.y + 1.0, q.y)));
            }
        }
        assert_eq!(l.ways.len(), 2, "two officers");
        for w in &l.ways {
            assert!(w.last().unwrap().distance(door) < 6.0, "posts by the door");
        }
    }
    println!(
        "{found} of {} hideouts have a place to land, {none} only a hover; slowest whole search {:.1} ms",
        places.hideouts.len(),
        worst * 1e3
    );
    println!("why not, over every place tried: {why:?}");
    assert!(
        found * 2 >= places.hideouts.len(),
        "most hideouts have ground for a cruiser: {found} of {}",
        places.hideouts.len()
    );
}
