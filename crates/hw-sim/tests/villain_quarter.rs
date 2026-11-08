//! The mad scientist's quarter: groups of his minions stand on its corners,
//! tougher than the team, while a hero is near, and go home when nobody is.

use glam::DVec3;
use hw_core::Seed;
use hw_net::PlayerId;
use hw_sim::collision::Body;
use hw_sim::quarter::{CORNER_EXTRA, WAKE};
use hw_sim::{GeneratedTown, Sim, TICK_HZ};

#[test]
fn villains_wait_on_the_quarters_corners_for_heroes() {
    let town = GeneratedTown::generate(Seed(3), 20_000.0);
    let mut sim = Sim::for_town(Seed(3), &town);
    let hero = sim.spawn_hero(PlayerId(1), sim.spawn_point(1));
    let corners: Vec<usize> = sim
        .fights()
        .spots
        .iter()
        .enumerate()
        .filter(|(_, s)| s.wake.is_some())
        .map(|(i, _)| i)
        .collect();
    assert!(corners.len() >= 3, "{} corners", corners.len());
    // Nobody near: nobody stands there.
    for _ in 0..TICK_HZ {
        sim.step();
    }
    let far: Vec<usize> = corners
        .iter()
        .copied()
        .filter(|&i| {
            let c = sim.fights().spots[i].center;
            sim.position(hero).unwrap().distance(c) > WAKE
        })
        .collect();
    assert!(!far.is_empty());
    for &i in &far {
        assert_eq!(sim.fights().spots[i].members, 0, "a group at a far corner");
    }
    // Walk up to a corner (a street away from it).
    let spot = sim.fights().spots[corners[1]].clone();
    let side = DVec3::new(spot.facing.x, 0.0, spot.facing.y);
    let stand = spot.center + side * 120.0 + DVec3::Y * 1.0;
    sim.set_body(
        hero,
        Body {
            pos: stand,
            vel: DVec3::ZERO,
            grounded: false,
        },
    );
    sim.step();
    sim.step();
    let s = &sim.fights().spots[corners[1]];
    assert!(s.members >= 5, "{} villains", s.members);
    let team = sim.team_level();
    let near: Vec<_> = sim
        .enemies()
        .into_iter()
        .filter(|e| e.2.distance(spot.center) < 20.0)
        .collect();
    assert!(!near.is_empty());
    for (_, _, _, f) in &near {
        assert!(f.level >= team + CORNER_EXTRA, "level {}", f.level);
    }
    // Leave town: they go home.
    sim.set_body(
        hero,
        Body {
            pos: DVec3::new(1e5, 50.0, 1e5),
            vel: DVec3::ZERO,
            grounded: false,
        },
    );
    sim.step();
    sim.step();
    assert_eq!(sim.fights().spots[corners[1]].members, 0);
    assert!(sim
        .enemies()
        .iter()
        .all(|e| e.2.distance(spot.center) > 30.0));
}
