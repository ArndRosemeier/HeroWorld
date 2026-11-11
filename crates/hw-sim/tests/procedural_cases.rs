//! Cases made up from a whole town: chains of steps over its buildings,
//! streets, plazas and underground places, the same every time, each
//! place on floor a hero can reach; a crew on the street stands when a
//! hero comes and, beaten, moves the case on.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_net::PlayerId;
use hw_sim::collision::Body;
use hw_sim::missions::{self, plan, CaseKind, Place, GANGS};
use hw_sim::{GeneratedTown, Sim, TICK_HZ};
use std::collections::BTreeSet;

fn stand(sim: &mut Sim, hero: hw_core::NetId, at: DVec3, seconds: f64) {
    for _ in 0..(seconds * TICK_HZ as f64) as u64 {
        sim.set_body(
            hero,
            Body {
                pos: at,
                vel: DVec3::ZERO,
                grounded: true,
            },
        );
        sim.step();
    }
}

fn beat_everyone(sim: &mut Sim) {
    for (id, _, _, mut f) in sim.enemies() {
        f.health = 0.0;
        f.status = hw_net::Status::Down;
        sim.set_fighter(id, f);
    }
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn a_town_gives_cases_all_over_it_the_same_every_time() {
    let seed = Seed(3);
    let town = GeneratedTown::generate(seed, 20_000.0);
    let sim = Sim::for_town(seed, &town);
    let places = sim.missions.places().expect("cases in a town").clone();
    let solids = town.obstacles.clone();
    let mut kinds = BTreeSet::new();
    let mut chains = 0;
    for number in 1..=40u16 {
        let gang = number as usize % GANGS.len();
        let Some(p) = plan::plan(&places, seed.0, number, gang, 0) else {
            continue;
        };
        // Same numbers, same case.
        assert_eq!(
            plan::plan(&places, seed.0, number, gang, 0),
            Some(p.clone())
        );
        chains += usize::from(p.steps.len() > 1);
        for (k, s) in p.steps.iter().enumerate() {
            let site = &s.site;
            kinds.insert(site.place);
            assert!(s.job.fits(site.place), "{:?} at {:?}", s.job, site.place);
            if k + 1 < p.steps.len() {
                assert_ne!(site.place, Place::Building, "a lead in a whole building");
            }
            assert!(!site.name.is_empty());
            let at = DVec2::new(site.at.x, site.at.z);
            if site.place.underground() {
                assert!(site.way_down.is_some(), "{} has no way down", site.name);
                let floor = solids.ground_for(at, site.at.y + 1.5);
                assert!(
                    (floor - site.at.y).abs() < 1.5,
                    "{} ({:?}) at {:.1?}: the floor is at {floor:.1}",
                    site.name,
                    site.place,
                    site.at
                );
            } else if site.place != Place::Building {
                let ground = solids.ground_at(at);
                assert!(
                    (ground - site.at.y).abs() < 1.5,
                    "{} at {:.1?}: the ground is at {ground:.1}",
                    site.name,
                    site.at
                );
            }
        }
    }
    assert!(chains >= 10, "only {chains} cases of more than one step");
    for want in [Place::Building, Place::Street, Place::Plaza] {
        assert!(kinds.contains(&want), "no case at a {want:?}: {kinds:?}");
    }
    assert!(
        kinds.iter().any(|k| k.underground()),
        "no case under the ground: {kinds:?}"
    );
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn a_street_crew_stands_for_a_hero_and_beaten_moves_the_case_on() {
    let seed = Seed(3);
    let town = GeneratedTown::generate(seed, 20_000.0);
    let mut sim = Sim::for_town(seed, &town);
    let hero = sim.spawn_hero(PlayerId(1), sim.spawn_point(1));
    // A chain: a crew on a street, then a job somewhere else.
    let places = sim.missions.places().expect("places").clone();
    let (number, p) = (1..200u16)
        .find_map(|n| {
            plan::plan(&places, seed.0, n, 1, 0)
                .filter(|p| p.steps.len() >= 2 && p.steps[0].job == CaseKind::Crew)
                .filter(|p| !p.steps[0].site.place.underground())
                .map(|p| (n, p))
        })
        .expect("a case that starts with a crew in the open");
    sim.missions.opened = number - 1;
    let n = sim.missions.force_plan(&mut sim.world, 1).expect("planned");
    assert_eq!(n, number);
    assert_eq!(sim.missions.case(n).expect("open").steps(), &p.steps[..]);
    let first = p.steps[0].site.at;
    // The radio leads there; the crew stands once the hero comes.
    stand(&mut sim, hero, first + DVec3::new(0.0, 0.0, 30.0), 2.0);
    let r = sim.missions.case(n).expect("open");
    assert_eq!(r.stage, missions::INSIDE);
    let near: Vec<_> = sim
        .enemies()
        .into_iter()
        .filter(|e| e.2.distance(first) < 40.0)
        .collect();
    assert!(near.len() >= 3, "{} villains at the corner", near.len());
    beat_everyone(&mut sim);
    stand(&mut sim, hero, first + DVec3::new(0.0, 0.0, 30.0), 2.0);
    let r = sim.missions.case(n).expect("still open");
    assert_eq!(r.step, 1, "the crew talked: on to the next place");
    assert_eq!(r.stage, missions::GO);
    assert_eq!(r.site(), &p.steps[1].site);
    // The book keeps the step.
    let book = sim.sheet(PlayerId(1)).expect("a sheet").cases;
    let saved = book.open.iter().find(|c| c.number == n).expect("saved");
    assert_eq!(saved.step, 1);
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn hostages_in_a_tube_station_are_freed() {
    let seed = Seed(3);
    let town = GeneratedTown::generate(seed, 20_000.0);
    let mut sim = Sim::for_town(seed, &town);
    let hero = sim.spawn_hero(PlayerId(1), sim.spawn_point(1));
    let n = sim
        .missions
        .force_at(&mut sim.world, CaseKind::Hostages, Place::Station, 1)
        .expect("a station");
    let site = sim.missions.case(n).expect("open").site().clone();
    // From the street the radio points at the way down.
    stand(&mut sim, hero, site.way_down.expect("a way down"), 1.5);
    let goal = sim.missions.case(n).expect("open").objective();
    let top = site.way_down.unwrap();
    assert!(
        goal.distance(top) < 0.1,
        "{goal:.1?} not {top:.1?}, station {:.1?}",
        site.at
    );
    // Down in the hall the gang stands, with its hostages.
    stand(&mut sim, hero, site.at, 2.0);
    let r = sim.missions.case(n).expect("open");
    assert_eq!(r.stage, missions::INSIDE);
    let held = r.clues();
    assert!(!held.is_empty(), "no hostages");
    beat_everyone(&mut sim);
    sim.step();
    sim.set_input(
        PlayerId(1),
        hw_net::InputState {
            revive: true,
            ..Default::default()
        },
    );
    for (at, _) in &held {
        stand(&mut sim, hero, *at, missions::FREE_SECONDS * 1.4);
    }
    sim.set_input(PlayerId(1), hw_net::InputState::default());
    stand(&mut sim, hero, site.at, 2.0);
    assert!(sim.missions.case(n).is_none(), "the case is closed");
    assert_eq!(sim.missions.closed, 1);
}
