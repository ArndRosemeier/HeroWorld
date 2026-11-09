//! The square with people, traffic, taxis and incidents follows the heroes
//! across a big city: far from where they started they still have cars and
//! people round them, clients hear where the square went, and the old one
//! is let go.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_net::ToClient;
use hw_sim::{GeneratedTown, Sim, AREA_SHIFT};
use std::sync::Arc;

#[test]
fn people_and_traffic_follow_a_hero_across_a_big_city() {
    let t = GeneratedTown::generate(Seed(2), 400_000.0);
    let mut sim = Sim::for_town(Seed(2), &t);
    let first = sim.area.clone().expect("a square");
    let start = (first.lo + first.hi) * 0.5;
    let _ = sim.take_news();

    // Not far enough yet: nothing moves.
    let near = start + DVec2::new(AREA_SHIFT * 0.5, 0.0);
    sim.finish_area_move(&[DVec3::new(near.x, 0.0, near.y)]);
    assert!(Arc::ptr_eq(sim.area.as_ref().unwrap(), &first));
    assert!(sim.area_news().is_none());

    // Off across the city, along a main road so there is town there.
    let far = t
        .world
        .main_roads
        .iter()
        .flat_map(|r| r.pts.iter().copied())
        .filter(|p| (*p - start).abs().max_element() > AREA_SHIFT + 500.0)
        .min_by(|a, b| a.distance(start).total_cmp(&b.distance(start)))
        .expect("town far from the start");
    let hero = [DVec3::new(far.x, t.world.ground(far), far.y)];
    sim.finish_area_move(&hero);
    let area = sim.area.clone().unwrap();
    assert!(!Arc::ptr_eq(&area, &first), "the square did not move");
    assert!(area.contains(far));
    assert!(((area.lo + area.hi) * 0.5 - far).abs().max_element() < 300.0);
    assert!(Arc::strong_count(&first) <= 2, "the old square is kept");

    // Clients hear where it went, then where every car of it is.
    let news = sim.take_news();
    let moved = news
        .iter()
        .position(|m| matches!(m, ToClient::AreaMoved { .. }))
        .expect("no AreaMoved");
    assert!(news[moved + 1..]
        .iter()
        .any(|m| matches!(m, ToClient::Traffic { .. })));
    assert!(matches!(sim.area_news(), Some(ToClient::AreaMoved { .. })));

    // Cars drive round the hero, people walk there, taxis wait there.
    let traffic = sim.wrecks.traffic.as_ref().unwrap();
    let cars = traffic.near(far, 400.0, 0.0, 50);
    assert!(!cars.is_empty(), "no traffic near the hero");
    let people = hw_sim::citizens::Citizens::new(area.clone(), traffic.signals());
    assert!(people.count() > 0, "no people in the new square");
    // (The town centre stand stays on the list wherever the square is.)
    assert!(sim.taxis.stands.iter().any(|s| s.name == "Town centre"));
    assert!(sim
        .taxis
        .stands
        .iter()
        .any(|s| s.name != "Town centre" && area.contains(DVec2::new(s.pos.x, s.pos.z))));
}
