//! The mole folk, the sky pirates and the Iron-Masked Empire in the world:
//! frightened mole folk can be talked down instead of fought (and
//! Molehaven remembers), pirates raid a shop and leave passers-by alone,
//! the Empire's legionaries hold townsfolk until they are beaten, and the
//! pirates and the delvers have their own corners of the town.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_net::{EntityKind, Faction, InputState, PlayerId, Status};
use hw_sim::collision::Body;
use hw_sim::combat::EnemyKind;
use hw_sim::factions;
use hw_sim::incidents::{self, IncidentKind};
use hw_sim::{GeneratedTown, Sim, TICK_HZ};

fn run(sim: &mut Sim, seconds: f64) {
    for _ in 0..(seconds * TICK_HZ as f64) as u64 {
        sim.step();
    }
}

/// A town with one hero and the first incident of `kind` open at once.
fn with_incident(kind: IncidentKind) -> (Sim, hw_core::NetId, incidents::Incident) {
    let town = GeneratedTown::generate(Seed(3), 20_000.0);
    let mut sim = Sim::for_town(Seed(3), &town);
    let hero = sim.spawn_hero(PlayerId(1), sim.spawn_point(1));
    sim.incidents.first_after = 0.0;
    sim.incidents.next_kind = Some(kind);
    run(&mut sim, 1.5);
    let inc = sim
        .incidents
        .open
        .iter()
        .find(|i| i.kind == kind)
        .cloned()
        .unwrap_or_else(|| panic!("no {kind:?}: {:?}", sim.incidents.open));
    (sim, hero, inc)
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

/// The incident's own villains (not a corner group that happens near).
fn group(sim: &mut Sim, inc: &incidents::Incident) -> Vec<(hw_core::NetId, EnemyKind)> {
    sim.enemies()
        .into_iter()
        .filter(|e| e.2.distance(inc.at) < 25.0)
        .map(|e| (e.0, e.1))
        .collect()
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn mole_folk_talked_down_go_home_and_molehaven_remembers() {
    let (mut sim, hero, inc) = with_incident(IncidentKind::Surfacing);
    let me = PlayerId(1);
    let moles = group(&mut sim, &inc);
    assert!(moles.len() >= 3, "{moles:?}");
    assert!(moles.iter().all(|m| m.1 == EnemyKind::Delver));
    assert_eq!(inc.left, 1, "frightened");
    // A hero walking right up to them is no threat to them, nor they to
    // the hero.
    let face = DVec3::new(inc.facing.x, 0.0, inc.facing.y);
    let by = inc.at + face * 3.0;
    for _ in 0..TICK_HZ * 2 {
        stand(&mut sim, hero, by);
        sim.step();
    }
    assert!(
        sim.enemies().iter().all(|e| e.3.target.is_none()),
        "frightened mole folk do not attack a passer-by"
    );
    let karma = sim.sheet(me).unwrap().karma;
    let before = sim.sheet(me).unwrap().regard[Faction::Moles.index()];
    // Holding the help key by them talks them down.
    sim.set_input(
        me,
        InputState {
            revive: true,
            ..Default::default()
        },
    );
    for _ in 0..((incidents::CALM_SECONDS + 0.5) * TICK_HZ as f64) as u64 {
        stand(&mut sim, hero, by);
        sim.step();
    }
    sim.set_input(me, InputState::default());
    let open = sim.incidents.open.iter().find(|i| i.id == inc.id).cloned();
    assert!(
        open.as_ref()
            .is_some_and(|i| i.calmed.is_some() && i.left == 0),
        "not talked down: {open:?}"
    );
    let sheet = sim.sheet(me).unwrap();
    assert!(sheet.karma > karma, "no karma for talking them down");
    assert_eq!(
        sheet.regard[Faction::Moles.index()] as i16,
        before as i16 + factions::CALMED
    );
    // Home in a few seconds: resolved, and they are gone.
    run(&mut sim, incidents::HOME_SECONDS + 1.5);
    assert!(sim.incidents.open.iter().all(|i| i.id != inc.id));
    assert!(group(&mut sim, &inc).is_empty(), "they went home");
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn fighting_mole_folk_wins_no_friends_in_molehaven() {
    let (mut sim, hero, inc) = with_incident(IncidentKind::Surfacing);
    let me = PlayerId(1);
    stand(&mut sim, hero, inc.at + DVec3::new(0.0, 0.0, 12.0));
    sim.step();
    for (id, _) in group(&mut sim, &inc) {
        let mut f = sim
            .enemies()
            .into_iter()
            .find(|e| e.0 == id)
            .expect("a mole")
            .3;
        f.health = 0.0;
        f.status = Status::Down;
        f.until = sim.tick;
        sim.set_fighter(id, f);
    }
    run(&mut sim, 15.0);
    assert!(sim.incidents.open.iter().all(|i| i.id != inc.id));
    let r = sim.sheet(me).unwrap().regard;
    assert!(r[Faction::Moles.index()] <= 0, "{r:?}");
    // What a beaten mole costs and earns (blows land in the fight itself,
    // which this test skips): Molehaven minds, the Lab is glad.
    let mut regard = [0i8; hw_net::FACTIONS];
    factions::beat(&mut regard, Faction::Moles);
    assert!(regard[Faction::Moles.index()] < 0);
    assert!(regard[Faction::Lab.index()] > 0);
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn sky_pirates_raid_a_shop_and_leave_a_passer_by_alone() {
    let (mut sim, hero, inc) = with_incident(IncidentKind::PirateRaid);
    let crew = group(&mut sim, &inc);
    let kinds: Vec<EnemyKind> = crew.iter().map(|c| c.1).collect();
    assert!(kinds.contains(&EnemyKind::Captain), "{kinds:?}");
    assert!(kinds.contains(&EnemyKind::Buccaneer), "{kinds:?}");
    assert!(kinds.contains(&EnemyKind::Gunner), "{kinds:?}");
    assert!(kinds
        .iter()
        .all(|k| factions::of_enemy(*k) == Faction::SkyPirates));
    // A hero across the street is not their business...
    let face = DVec3::new(inc.facing.x, 0.0, inc.facing.y);
    for _ in 0..TICK_HZ * 3 {
        stand(&mut sim, hero, inc.at + face * 18.0);
        sim.step();
    }
    assert!(sim.enemies().iter().all(|e| e.3.target.is_none()));
    // ...stepping in is.
    // (In the open and in their sight: not behind a kiosk or a hedge.)
    let solids = sim.solids();
    let chest = DVec3::Y * 1.3;
    let near = [6.0, 4.5, 3.0, 1.5]
        .into_iter()
        .map(|d| inc.at + face * d)
        .find(|p| {
            solids.inside(*p + DVec3::Y * 0.3).is_none()
                && hw_sim::combat::in_sight(&solids, *p + chest, inc.at + chest)
        })
        .unwrap_or(inc.at);
    for _ in 0..TICK_HZ {
        stand(&mut sim, hero, near);
        sim.step();
    }
    assert!(sim.enemies().iter().any(|e| e.3.target == Some(hero)));
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn the_iron_legion_holds_townsfolk_until_it_is_beaten() {
    let (mut sim, hero, inc) = with_incident(IncidentKind::IronLanding);
    let legion = group(&mut sim, &inc);
    let kinds: Vec<EnemyKind> = legion.iter().map(|c| c.1).collect();
    assert!(kinds.contains(&EnemyKind::Praetorian), "{kinds:?}");
    assert!(kinds.contains(&EnemyKind::Legionary), "{kinds:?}");
    let captives = |sim: &mut Sim| {
        sim.snapshot()
            .into_iter()
            .filter(|e| e.kind == EntityKind::Bystander)
            .map(|e| DVec3::from_array(e.pos))
            .collect::<Vec<_>>()
    };
    let held = captives(&mut sim);
    assert_eq!(held.len(), 2, "two townsfolk rounded up");
    assert!(held.iter().all(|p| p.distance(inc.at) < 5.0));
    // Unknown heroes are not attacked on sight, even by the Empire...
    let face = DVec3::new(inc.facing.x, 0.0, inc.facing.y);
    for _ in 0..TICK_HZ * 2 {
        stand(&mut sim, hero, inc.at + face * 30.0);
        sim.step();
    }
    assert!(sim.enemies().iter().all(|e| e.3.target.is_none()));
    // ...but coming for the captives is a fight.
    for _ in 0..TICK_HZ {
        stand(&mut sim, hero, inc.at + face * 12.0);
        sim.step();
    }
    assert!(sim.enemies().iter().any(|e| e.3.target == Some(hero)));
    // Beaten: the captives are free and walk off.
    for (id, _) in legion {
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
    // They thank the hero first (standing a moment), then go.
    run(
        &mut sim,
        incidents::THANK_SECONDS + incidents::LEAVE_SECONDS + 1.0,
    );
    assert!(captives(&mut sim).is_empty(), "the captives went home");
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn the_empire_waits_until_the_team_can_take_it() {
    let town = GeneratedTown::generate(Seed(3), 20_000.0);
    let mut sim = Sim::for_town(Seed(3), &town);
    sim.spawn_hero(PlayerId(1), sim.spawn_point(1));
    sim.incidents.first_after = 0.0;
    // Many incidents at level 1: never the Iron Legion, never a saucer.
    for _ in 0..40 {
        run(&mut sim, incidents::GAP_SECONDS + 1.0);
        assert!(sim
            .incidents
            .open
            .iter()
            .all(|i| i.kind != IncidentKind::IronLanding && i.kind != IncidentKind::SaucerRaid));
        // Clear them so more start.
        sim.incidents.open.clear();
        if sim.incidents.count > 25 {
            break;
        }
    }
    assert!(sim.incidents.count > 10);
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn pirates_keep_watch_on_the_quay_and_delvers_guard_molehaven() {
    let town = GeneratedTown::generate(Seed(3), 20_000.0);
    let mut sim = Sim::for_town(Seed(3), &town);
    let hero = sim.spawn_hero(PlayerId(1), sim.spawn_point(1));
    if let Some(r) = hw_sim::roosts::pirate_roost(&town.world) {
        let (watch, facing) = r.watch();
        if DVec2::new(watch.x, watch.z)
            .distance(DVec2::new(sim.spawn_point(1).x, sim.spawn_point(1).z))
            > hw_sim::ROOST_CLEAR
        {
            // Walk up to them: they are there, and warn a stranger off.
            let f = DVec3::new(facing.x, 0.0, facing.y);
            for _ in 0..TICK_HZ {
                stand(&mut sim, hero, watch + f * 60.0);
                sim.step();
            }
            for _ in 0..TICK_HZ * 3 {
                stand(&mut sim, hero, watch + f * 16.0);
                sim.step();
            }
            let near: Vec<_> = sim
                .enemies()
                .into_iter()
                .filter(|e| e.2.distance(watch) < 12.0)
                .collect();
            assert!(!near.is_empty(), "nobody on the quay");
            assert!(near
                .iter()
                .all(|e| factions::of_enemy(e.1) == Faction::SkyPirates));
        }
    }
    if let Some((gate, _)) = hw_sim::roosts::mole_gate(town.world.underground()) {
        // Near enough to wake them, not so near they would not come.
        for _ in 0..TICK_HZ {
            stand(&mut sim, hero, gate + DVec3::new(0.0, 40.0, 0.0));
            sim.step();
        }
        for _ in 0..TICK_HZ * 2 {
            stand(&mut sim, hero, gate + DVec3::new(0.0, 0.0, 6.0));
            sim.step();
        }
        let near: Vec<_> = sim
            .enemies()
            .into_iter()
            .filter(|e| e.2.distance(gate) < 10.0)
            .collect();
        assert!(!near.is_empty(), "nobody at Molehaven's gate");
        assert!(near.iter().all(|e| e.1 == EnemyKind::Delver));
        // Standing on the cavern floor, not up in the street.
        assert!(
            near.iter().all(|e| (e.2.y - gate.y).abs() < 1.5),
            "{near:?}"
        );
        // Molehaven lets a stranger pass.
        assert!(near.iter().all(|e| e.3.target.is_none()));
    }
}
