//! The cases that are not hideouts, in a whole generated town: the board
//! deals them, and each plays where the town really is: the getaway car
//! finds room at the kerb and flies the streets clear of the houses, the
//! roof's gang stands on the roof and Sky Patrol finds room to land on
//! it, the courier walks the pavements to the hideout's door.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_net::PlayerId;
use hw_sim::collision::Body;
use hw_sim::missions::board::{self, Shape};
use hw_sim::missions::getaway::{CarState, Phase, FLY_HEIGHT};
use hw_sim::missions::tail::Walk;
use hw_sim::missions::{self, plan, CaseKind, Place};
use hw_sim::{GeneratedTown, Sim, TICK_HZ};
use std::collections::BTreeMap;

fn put(sim: &mut Sim, hero: hw_core::NetId, at: DVec3) {
    sim.set_body(
        hero,
        Body {
            pos: at,
            vel: DVec3::ZERO,
            grounded: true,
        },
    );
    if let Some(mut f) = sim.fighter(hero) {
        f.health = f.max_health;
        sim.set_fighter(hero, f);
    }
}

fn stand(sim: &mut Sim, hero: hw_core::NetId, at: DVec3, seconds: f64) {
    for _ in 0..(seconds * TICK_HZ as f64) as u64 {
        put(sim, hero, at);
        sim.step();
    }
}

fn town() -> (Sim, GeneratedTown, hw_core::NetId) {
    let seed = Seed(3);
    let town = GeneratedTown::generate(seed, 20_000.0);
    let mut sim = Sim::for_town(seed, &town);
    let hero = sim.spawn_hero(PlayerId(1), DVec3::new(0.0, 60.0, 0.0));
    (sim, town, hero)
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn the_board_deals_every_shape_the_same_every_time() {
    let (sim, town, _) = town();
    let places = sim.missions.places().expect("cases in a town").clone();
    let solids = town.obstacles.clone();
    let mut shapes: BTreeMap<String, usize> = BTreeMap::new();
    let mut made: BTreeMap<String, usize> = BTreeMap::new();
    for n in 1..=200u16 {
        let c = board::crime(&places, 3, n).expect("a crime");
        *shapes.entry(format!("{:?}", c.shape)).or_default() += 1;
        let Some(p) = plan::crime_plan(&places, 3, n, 1) else {
            continue;
        };
        assert_eq!(plan::crime_plan(&places, 3, n, 1), Some(p.clone()));
        let first = &p.steps[0];
        assert!(first.job.fits(first.site.place), "{:?}", first);
        *made
            .entry(format!("{:?}@{:?}", first.job, first.site.place))
            .or_default() += 1;
        let at = DVec2::new(first.site.at.x, first.site.at.z);
        match (c.shape, first.site.place) {
            (Shape::Rooftop, Place::Rooftop) => {
                // A roof, well over the street, as high as the plan says.
                let top = solids.floor_under(at, first.site.at.y + 3.0);
                assert!(
                    (top - first.site.at.y).abs() < 2.0 && top > solids.ground_at(at) + 5.0,
                    "{}: the roof is at {top:.1}, the plan says {:.1}",
                    first.site.name,
                    first.site.at.y
                );
            }
            (Shape::Tail, Place::Street) => {
                assert_eq!(p.steps.len(), 2, "a tail leads somewhere");
                assert!(p.steps[1].site.building.is_some());
            }
            _ => {}
        }
    }
    // Every shape turns up, and most of what is rolled finds its place.
    for s in ["Bust", "Heist", "Getaway", "Rooftop", "Tail"] {
        assert!(shapes.get(s).copied().unwrap_or(0) >= 10, "{s}: {shapes:?}");
    }
    for (k, least) in [
        ("Heist@Shopfront", 20),
        ("Chase@Road", 12),
        ("Hostages@Rooftop", 12),
        ("Tail@Street", 15),
    ] {
        assert!(
            made.get(k).copied().unwrap_or(0) >= least,
            "{k}: {made:?} of {shapes:?}"
        );
    }
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn a_heist_in_town_gets_its_car_and_the_car_flies_the_streets() {
    let (mut sim, town, hero) = town();
    let solids = town.obstacles.clone();
    let mut flown = 0;
    for k in 0..3 {
        let Some(n) = sim.missions.force_shape(&mut sim.world, CaseKind::Heist) else {
            panic!("no heist on the board");
        };
        let site = sim.missions.case(n).unwrap().site().clone();
        // Watching from across the street.
        let f = site.facing.normalize_or(DVec2::X);
        let side = f.perp() * (40.0 + 10.0 * k as f64);
        let q = DVec2::new(site.at.x, site.at.z) + f * 8.0 + side;
        let watch = DVec3::new(q.x, solids.ground_at(q) + 0.2, q.y);
        stand(&mut sim, hero, watch, 14.0);
        let r = sim.missions.case(n).expect("open");
        assert_eq!(
            r.getaway_phase(),
            Some(Phase::Loading),
            "no room for a car by {}",
            site.name
        );
        let (_, at, state) = r.getaway_car().expect("a car");
        assert_eq!(state, CarState::Parked);
        assert!(
            at.distance(site.at) < 45.0,
            "the car is {:.0} m from the door",
            at.distance(site.at)
        );
        let ground = solids.ground_at(DVec2::new(at.x, at.z));
        assert!(
            (at.y - ground).abs() < 0.6,
            "it stands at {at:.1?}, ground {ground:.1}"
        );
        // Loaded and off: it stays over the streets, clear of everything.
        let mut t = 0.0;
        while sim.missions.case(n).and_then(|r| r.getaway_phase()) != Some(Phase::Flying) {
            stand(&mut sim, hero, watch, 1.0);
            t += 1.0;
            assert!(t < 200.0, "never took off from {}", site.name);
        }
        let mut gone = 0.0;
        let (mut samples, mut low) = (0usize, 0usize);
        let mut last = sim.missions.case(n).unwrap().getaway_car().unwrap().1;
        for _ in 0..TICK_HZ * 40 {
            let Some((_, at, state)) = sim.missions.case(n).and_then(|r| r.getaway_car()) else {
                break;
            };
            assert_eq!(state, CarState::Flying);
            gone += at.distance(last);
            last = at;
            if std::env::var("HW_TEST_TRACE").is_ok() && sim.tick % 30 == 0 {
                println!(
                    "car {at:.1?} over ground {:.1} gone {gone:.0}",
                    at.y - solids.ground_at(DVec2::new(at.x, at.z))
                );
            }
            let under = solids.ground_at(DVec2::new(at.x, at.z));
            if gone > 30.0 {
                // Low over the street, but for a hop over what stands in
                // its way.
                assert!(
                    at.y - under > FLY_HEIGHT - 3.0 && at.y - under < FLY_HEIGHT + 40.0,
                    "it flies {:.1} m over the ground at {at:.0?}",
                    at.y - under
                );
                samples += 1;
                low += usize::from(at.y - under < FLY_HEIGHT + 4.0);
            }
            for h in [0.2, 0.9, 1.6] {
                assert!(
                    !solids.solid_at(at + DVec3::Y * h),
                    "it flies through something at {at:.1?}"
                );
            }
            // The hero keeps up on the ground.
            let b = DVec3::new(at.x - 15.0, under + 0.2, at.z);
            put(&mut sim, hero, b);
            sim.step();
        }
        assert!(gone > 250.0, "it went {gone:.0} m in 40 s");
        assert!(
            low * 10 >= samples * 7,
            "low over the street only {low} of {samples} ticks"
        );
        flown += 1;
        // Brought down, it lies in the street and the case can be closed.
        let (id, _, _) = sim.missions.case(n).unwrap().getaway_car().unwrap();
        let mut f = sim.fighter(id).unwrap();
        f.health = 0.0;
        f.status = hw_net::Status::Down;
        sim.set_fighter(id, f);
        let at = last;
        let near = DVec3::new(
            at.x - 15.0,
            solids.ground_at(DVec2::new(at.x - 15.0, at.z)) + 0.2,
            at.z,
        );
        stand(&mut sim, hero, near, 8.0);
        let (_, down, state) = sim.missions.case(n).unwrap().getaway_car().unwrap();
        assert_eq!(state, CarState::Wrecked);
        let under = solids.ground_at(DVec2::new(down.x, down.z));
        assert!(
            (down.y - under).abs() < 1.0,
            "the wreck lies at {down:.1?}, ground {under:.1}"
        );
        for (id, _, _, mut f) in sim.enemies() {
            f.health = 0.0;
            f.status = hw_net::Status::Down;
            sim.set_fighter(id, f);
        }
        stand(&mut sim, hero, near, 3.0);
        assert!(sim.missions.case(n).is_none(), "the case is closed");
        // Away, so the wreck is cleared before the next.
        stand(&mut sim, hero, DVec3::new(0.0, 300.0, 0.0), 3.0);
    }
    assert_eq!(flown, 3);
    assert_eq!(sim.missions.closed, 3);
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn a_roof_in_town_holds_its_gang_and_a_cruiser() {
    let (mut sim, town, hero) = town();
    let solids = town.obstacles.clone();
    let n = sim
        .missions
        .force_shape(&mut sim.world, CaseKind::Hostages)
        .expect("a roof on the board");
    let site = sim.missions.case(n).unwrap().site().clone();
    assert_eq!(site.place, Place::Rooftop);
    let q = DVec2::new(site.at.x, site.at.z);
    let top = solids.floor_under(q, site.at.y + 3.0);
    let edge = site.around.last().copied().unwrap();
    let edge = DVec3::new(
        edge.x,
        solids.floor_under(DVec2::new(edge.x, edge.z), top + 3.0),
        edge.z,
    );
    stand(&mut sim, hero, edge, 3.0);
    let r = sim.missions.case(n).expect("open");
    assert_eq!(r.stage, missions::INSIDE);
    let ids = r.hostages();
    assert!(!ids.is_empty(), "nobody held on {}", site.name);
    let foes = sim.enemies();
    assert!(foes.len() >= 4, "{} villains", foes.len());
    for (_, _, at, _) in &foes {
        assert!(
            (at.y - top).abs() < 1.5,
            "a villain at {at:.1?}, the roof at {top:.1}"
        );
    }
    for (id, _, _, mut f) in foes {
        f.health = 0.0;
        f.status = hw_net::Status::Down;
        sim.set_fighter(id, f);
    }
    stand(&mut sim, hero, edge, 2.0);
    let lift = sim.missions.case(n).unwrap().lift();
    let pad = lift.expect("room for a cruiser on the roof").pad();
    assert!((pad.y - top).abs() < 0.3, "the pad at {pad:.1?}");
    // It comes down onto the roof and stands there, solid.
    let mut landed = false;
    for _ in 0..TICK_HZ * 40 {
        put(&mut sim, hero, edge);
        sim.step();
        let tick = sim.tick;
        if sim
            .missions
            .case(n)
            .and_then(|r| r.lift())
            .is_some_and(|l| l.stands(tick))
        {
            landed = true;
            break;
        }
    }
    assert!(landed, "the cruiser never landed");
    assert!(!sim.missions.safe_solids().is_empty());
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn a_courier_in_town_walks_the_streets_to_the_hideout() {
    let (mut sim, town, hero) = town();
    let solids = town.obstacles.clone();
    let n = sim
        .missions
        .force_shape(&mut sim.world, CaseKind::Tail)
        .expect("a tail on the board");
    let r = sim.missions.case(n).unwrap();
    let start = r.site().at;
    let hideout = r.steps()[1].site.clone();
    // Watching from the roofs: over him, where he does not look.
    let over = |at: DVec3| at + DVec3::Y * 30.0;
    stand(&mut sim, hero, over(start), 3.0);
    if std::env::var("HW_TEST_TRACE").is_ok() {
        let way = sim.missions.case(n).unwrap().tail_way();
        println!("start {start:.0?} hideout {:.0?}", hideout.at);
        for p in &way {
            println!(
                "  way {:.1?} ground {:.1}",
                p,
                solids.ground_at(DVec2::new(p.x, p.z))
            );
        }
    }
    let mut seconds = 0.0;
    let mut walked = 0.0;
    let mut last = start;
    let mut stuck = 0.0;
    loop {
        let Some(r) = sim.missions.case(n) else {
            panic!(
                "the case is gone after {seconds} s (lost: {})",
                sim.missions.lost_cases
            );
        };
        if r.step_index() > 0 {
            break;
        }
        let j = r.tail().expect("a tail").clone();
        assert!(
            matches!(j.walk, Walk::Walking | Walk::Looking(_)),
            "{:?} after {seconds} s at {:.0?}",
            j.walk,
            j.at
        );
        if std::env::var("HW_TEST_TRACE").is_ok() {
            println!("{seconds:.0} s: {:?} at {:.1?}", j.walk, j.at);
        }
        walked += j.at.distance(last);
        stuck = if j.at.distance(last) < 0.2 && j.walk == Walk::Walking {
            stuck + 1.0
        } else {
            0.0
        };
        assert!(
            stuck < 15.0,
            "he is stuck at {:.1?} after {seconds} s",
            j.at
        );
        last = j.at;
        // On his feet, on the ground, in nothing.
        let ground = solids.ground_at(DVec2::new(j.at.x, j.at.z));
        assert!(
            (j.at.y - ground).abs() < 2.5,
            "he is at {:.1?}, ground {ground:.1}",
            j.at
        );
        stand(&mut sim, hero, over(j.at), 1.0);
        seconds += 1.0;
        assert!(
            seconds < 420.0,
            "still walking after {seconds} s: {:.0} m from the door",
            j.at.distance(hideout.at)
        );
    }
    assert!(walked > 100.0, "he walked {walked:.0} m");
    // Unseen: the hideout is the next step, and nobody watches its door.
    let r = sim.missions.case(n).unwrap();
    assert!(r.site().building.is_some());
    assert!(
        last.distance(hideout.at) < 80.0,
        "he vanished {:.0} m from the hideout",
        last.distance(hideout.at)
    );
    let b = r.site().at;
    stand(&mut sim, hero, b + DVec3::Y * 0.5, 3.0);
    let r = sim.missions.case(n).expect("the hideout");
    assert_eq!(r.stage, missions::INSIDE);
    assert!(r.spots().0.is_none(), "lookouts at the door after all");
}
