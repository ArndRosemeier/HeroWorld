//! Gang hideouts on the police computer: the board, taking a crime, the
//! gang standing in the building's rooms, and the one real thing to find.

use glam::DVec3;
use hw_core::Seed;
use hw_net::{PlayerId, ToClient};
use hw_sim::collision::Body;
use hw_sim::missions::{board, CaseKind};
use hw_sim::{GeneratedTown, Sim, TICK_HZ};

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

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn the_board_lists_hideouts_and_a_taken_crime_fills_the_building() {
    let seed = Seed(3);
    let town = GeneratedTown::generate(seed, 20_000.0);
    let mut sim = Sim::for_town(seed, &town);
    let places = sim.missions.places().expect("cases in a town").clone();
    println!(
        "{} hideouts of {} buildings",
        places.hideouts.len(),
        places.floors.buildings.len()
    );
    assert!(places.hideouts.len() >= 5, "the town has hideouts");
    let mut sizes = [0usize; 3];
    for c in 1..=places.hideouts.len() as u16 {
        sizes[board::crime(&places, seed.0, c).unwrap().size as usize] += 1;
    }
    let mut spare: Vec<u8> = places.hideouts.iter().map(|&k| places.rooms[k]).collect();
    spare.sort();
    println!("sizes small/middling/big {sizes:?}, rooms per hideout {spare:?}");
    assert!(sizes.iter().all(|&n| n > 0), "all three sizes turn up");
    let hero = sim.spawn_hero(PlayerId(1), DVec3::new(0.0, 1.0, 0.0));
    sim.step();
    let board_msg = sim
        .missions
        .snapshot()
        .into_iter()
        .find_map(|m| match m {
            ToClient::CaseBoard { crimes, .. } => Some(crimes),
            _ => None,
        })
        .expect("a board");
    assert_eq!(board_msg.len(), board::BOARD);
    let mut jobs = std::collections::BTreeSet::new();
    for c in 1..=30u16 {
        let crime = board::crime(&places, seed.0, c).unwrap();
        jobs.insert(crime.job.code());
        sim.missions.take_case(c);
        sim.step();
        let Some(r) = sim.missions.case(c) else {
            // The limit: close what is open and go on.
            let open: Vec<u16> = sim.missions.cases.iter().map(|r| r.number).collect();
            println!("crime {c}: not opened (open {open:?})");
            sim.missions.cases.clear();
            continue;
        };
        let at = r.site().at;
        let before = sim.enemies().len();
        println!(
            "crime {c} {:?} {:?} size {} at {:.0?} in {}",
            crime.kind, crime.job, crime.size, at, places.names[crime.building]
        );
        // The hero at the way in: the gang stands.
        stand(&mut sim, hero, at + DVec3::new(0.0, 0.5, 0.0), 3.0);
        let enemies = sim.enemies().len() - before;
        let r = sim.missions.case(c).unwrap();
        println!(
            "  stage {} enemies {enemies} clues {:?}",
            r.stage,
            r.clues()
        );
        assert!(enemies >= 3, "the gang stands in crime {c}");
        match crime.job {
            CaseKind::Evidence => assert_eq!(r.clues().len(), 1, "one thing to find"),
            _ => assert!(r.clues().is_empty()),
        }
        sim.missions.cases.clear();
    }
    assert_eq!(jobs.len(), 2, "both jobs turn up");
}
