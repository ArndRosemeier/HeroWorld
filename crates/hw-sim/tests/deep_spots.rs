//! The factions' people in the deep underground: a mole camp gets its
//! delvers once a hero is down there near it, and is forgotten again when
//! every hero has gone far away (nothing kept for places nobody is near).

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_gen::urban::deep::SpotKind;
use hw_net::{EntityKind, PlayerId};
use hw_sim::collision::Body;
use hw_sim::{GeneratedTown, Sim, TICK_HZ};

fn stand(sim: &mut Sim, hero: hw_core::NetId, at: DVec3) {
    sim.set_body(
        hero,
        Body {
            pos: at,
            vel: DVec3::ZERO,
            grounded: true,
        },
    );
}

fn run_at(sim: &mut Sim, hero: hw_core::NetId, at: DVec3, seconds: f64) {
    for _ in 0..(seconds * TICK_HZ as f64) as u64 {
        stand(sim, hero, at);
        sim.step();
    }
}

#[test]
fn a_mole_camp_wakes_for_a_hero_below_and_is_forgotten_after() {
    let town = GeneratedTown::generate(Seed(3), 20_000.0);
    let core = town.world.core();
    let r = DVec2::splat(900.0);
    let camp = town
        .world
        .deep_spots_in(core - r, core + r)
        .into_iter()
        .find(|d| d.kind == SpotKind::MoleCamp)
        .expect("a mole camp in the Deeps");
    let mut sim = Sim::for_town(Seed(3), &town);
    let hero = sim.spawn_hero(PlayerId(1), sim.spawn_point(1));
    // Open cave floor 28-42 m from the camp: near enough to wake it, not
    // so near that its people would appear in front of the hero.
    let solids = town.obstacles.clone();
    let c = DVec2::new(camp.at.x, camp.at.z);
    let near = (0..32)
        .flat_map(|k| {
            let a = std::f64::consts::TAU * k as f64 / 32.0;
            [28.0, 32.0, 36.0, 42.0].map(|d| c + DVec2::new(a.sin(), a.cos()) * d)
        })
        .find(|&p| (solids.ground_for(p, camp.at.y + 1.5) - camp.at.y).abs() < 1.0)
        .expect("open floor near the camp");
    let near = DVec3::new(near.x, solids.ground_for(near, camp.at.y + 1.5), near.y);
    let delvers = |sim: &mut Sim| {
        sim.snapshot()
            .into_iter()
            .filter(|e| e.kind == EntityKind::Delver)
            .filter(|e| DVec3::from_array(e.pos).distance(camp.at) < 12.0)
            .count()
    };
    assert_eq!(delvers(&mut sim), 0);
    // Nearer: the mole folk are round their camp.
    run_at(&mut sim, hero, near, 3.0);
    let at = sim.position(hero).unwrap();
    assert!(at.distance(near) < 1.0, "pushed off the floor: {at:?}");
    assert!(!sim.deep.is_empty(), "no place taken in");
    assert!(delvers(&mut sim) >= 3, "no mole folk at the camp");
    // Far away on the street: they go home and the place is forgotten.
    let (street, _) = town.world.spawn_line();
    let up = DVec3::new(street.x, town.world.ground(street), street.y);
    assert!(up.distance(camp.at) > hw_sim::deepspots::FORGET);
    run_at(&mut sim, hero, up, 4.0);
    assert_eq!(delvers(&mut sim), 0);
    assert_eq!(sim.deep.len(), 0, "places kept for nobody");
}
