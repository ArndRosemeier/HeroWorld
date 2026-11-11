//! The invaders from Zorrak: a saucer raid brings an overlord with grabbers
//! and zappers, two townsfolk held in the tractor beam, leaves an unknown
//! hero alone until they come for the captives, and its saucer is solid
//! where every machine draws it.

use glam::DVec3;
use hw_core::Seed;
use hw_net::{EntityKind, Faction, PlayerId, Status};
use hw_sim::collision::Body;
use hw_sim::combat::EnemyKind;
use hw_sim::incidents::{self, IncidentKind};
use hw_sim::{factions, roosts, GeneratedTown, Sim, TICK_HZ};

fn run(sim: &mut Sim, seconds: f64) {
    for _ in 0..(seconds * TICK_HZ as f64) as u64 {
        sim.step();
    }
}

fn stand(sim: &mut Sim, hero: hw_core::NetId, at: DVec3) {
    sim.set_body(
        hero,
        Body {
            pos: at + DVec3::Y * 0.1,
            vel: DVec3::ZERO,
            grounded: true,
        },
    );
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn a_saucer_raid_holds_townsfolk_in_its_beam_until_it_is_beaten() {
    let town = GeneratedTown::generate(Seed(3), 20_000.0);
    let mut sim = Sim::for_town(Seed(3), &town);
    let hero = sim.spawn_hero(PlayerId(1), sim.spawn_point(1));
    sim.incidents.first_after = 0.0;
    sim.incidents.next_kind = Some(IncidentKind::SaucerRaid);
    run(&mut sim, 1.5);
    let inc = sim
        .incidents
        .open
        .iter()
        .find(|i| i.kind == IncidentKind::SaucerRaid)
        .cloned()
        .unwrap_or_else(|| panic!("no saucer raid: {:?}", sim.incidents.open));
    let crew: Vec<(hw_core::NetId, EnemyKind)> = sim
        .enemies()
        .into_iter()
        .filter(|e| e.2.distance(inc.at) < 25.0)
        .map(|e| (e.0, e.1))
        .collect();
    let kinds: Vec<EnemyKind> = crew.iter().map(|c| c.1).collect();
    for k in [EnemyKind::Overlord, EnemyKind::Zapper, EnemyKind::Grabber] {
        assert!(kinds.contains(&k), "{k:?} in {kinds:?}");
    }
    assert!(kinds
        .iter()
        .all(|k| factions::of_enemy(*k) == Faction::Invaders));
    let captives = |sim: &mut Sim| {
        sim.snapshot()
            .into_iter()
            .filter(|e| e.kind == EntityKind::Bystander)
            .map(|e| DVec3::from_array(e.pos))
            .collect::<Vec<_>>()
    };
    let held = captives(&mut sim);
    assert_eq!(held.len(), 2, "two townsfolk in the beam");
    // The saucer hangs over them, high over the street.
    let over = held.iter().fold(DVec3::ZERO, |a, p| a + *p) / 2.0;
    let saucer = roosts::saucer_hover(&town.world, over);
    assert!(saucer.y - over.y > 13.0, "{saucer} over {over}");

    // Its top is solid: a hero dropped onto it stands on it.
    stand(&mut sim, hero, saucer + DVec3::Y * 5.0);
    run(&mut sim, 3.0);
    let me = sim
        .snapshot()
        .into_iter()
        .find(|e| e.id == hero)
        .map(|e| DVec3::from_array(e.pos))
        .expect("the hero");
    assert!(
        me.y > saucer.y + 0.5 && DVec3::new(me.x - saucer.x, 0.0, me.z - saucer.z).length() < 9.0,
        "the hero fell through the saucer: {me} vs {saucer}"
    );

    // An unknown hero is left alone...
    let face = DVec3::new(inc.facing.x, 0.0, inc.facing.y);
    for _ in 0..TICK_HZ * 2 {
        stand(&mut sim, hero, inc.at + face * 30.0);
        sim.step();
    }
    assert!(sim.enemies().iter().all(|e| e.3.target.is_none()));
    // ...until they come for the captives.
    for _ in 0..TICK_HZ {
        stand(&mut sim, hero, inc.at + face * 12.0);
        sim.step();
    }
    assert!(sim.enemies().iter().any(|e| e.3.target == Some(hero)));
    // Beaten: the captives are free and go home.
    for (id, _) in crew {
        if let Some(e) = sim.enemies().into_iter().find(|e| e.0 == id) {
            let mut f = e.3;
            f.health = 0.0;
            f.status = Status::Down;
            f.until = sim.tick;
            sim.set_fighter(id, f);
        }
    }
    for _ in 0..20 {
        if sim.incidents.open.iter().all(|i| i.id != inc.id) {
            break;
        }
        run(&mut sim, 1.0);
    }
    assert!(sim.incidents.open.iter().all(|i| i.id != inc.id));
    run(
        &mut sim,
        incidents::THANK_SECONDS + incidents::LEAVE_SECONDS + 1.0,
    );
    assert!(captives(&mut sim).is_empty(), "the captives went home");
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn saucers_clear_every_roof_under_them() {
    // Over every street of a big town, the saucer clears what stands
    // under it by its own height and its margin.
    let town = GeneratedTown::generate(Seed(5), 120_000.0);
    let w = &town.world;
    let mut n = 0;
    let core = w.core();
    let roads = w.roads_in(
        core - glam::DVec2::splat(350.0),
        core + glam::DVec2::splat(350.0),
    );
    for r in roads.iter().step_by(3) {
        for p in r.pts.iter().step_by(4) {
            let street = DVec3::new(p.x, w.road_height(*p), p.y);
            let at = roosts::saucer_hover(w, street);
            let c = glam::DVec2::new(at.x, at.z);
            let r = roosts::SAUCER_RADIUS + 1.5;
            let roofs = roosts::roofs_under(w, c, glam::DVec2::X, r, r);
            assert!(
                at.y - roosts::SAUCER_UNDER_DEPTH >= roofs + 3.9,
                "saucer at {at} over roofs at {roofs}"
            );
            n += 1;
        }
    }
    assert!(n > 50, "{n}");
}
