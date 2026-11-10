//! Who attacks whom: a new hero is a nobody whom villains treat like any
//! passer-by. A gang on its turf warns a hero off and fights one who stays;
//! a faction that has turned hostile attacks on sight; one that does not
//! mind the hero lets them pass. Beating a gang's people turns it against
//! the hero (and warms its rivals).

use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_net::{Faction, FxKind, InputState, PlayerId, Sheet};
use hw_sim::collision::{Body, Obstacles};
use hw_sim::factions::{self, Attitude, WARN_SECONDS};
use hw_sim::{progress, Sim, GROUND_Y, TICK_HZ};
use std::sync::Arc;

/// A turf corner of the Lab at the origin, facing +X, and a hero with
/// `regard` (the sheet's offsets) parked 40 m off along +X.
fn turf(regard: [i8; hw_net::FACTIONS]) -> (Sim, NetId) {
    let mut sim = Sim::with_solids(Seed(5), Arc::new(Obstacles::flat(GROUND_Y)));
    sim.set_spawn((DVec2::ZERO, DVec2::X));
    sim.add_villain_corner(DVec3::new(0.0, GROUND_Y, 0.0), DVec2::X, 0, 220.0);
    let sheet = Sheet {
        regard,
        ..progress::starter()
    };
    let hero = sim.spawn_hero_with(PlayerId(1), DVec3::new(40.0, GROUND_Y + 0.5, 0.0), sheet);
    for _ in 0..3 {
        sim.step();
    }
    assert!(!sim.enemies().is_empty(), "the corner's group stands there");
    (sim, hero)
}

fn put(sim: &mut Sim, hero: NetId, x: f64) {
    sim.set_body(
        hero,
        Body {
            pos: DVec3::new(x, GROUND_Y + 0.05, 0.0),
            vel: DVec3::ZERO,
            grounded: true,
        },
    );
}

/// Steps `seconds` with the hero standing still at `x`; returns whether a
/// warning was shouted and whether any villain went for the hero.
fn wait(sim: &mut Sim, hero: NetId, x: f64, seconds: f64) -> (bool, bool) {
    let (mut warned, mut attacked) = (false, false);
    for _ in 0..(seconds * TICK_HZ as f64) as u64 {
        put(sim, hero, x);
        sim.set_input(PlayerId(1), InputState::default());
        sim.step();
        warned |= sim
            .fx()
            .iter()
            .any(|e| e.kind == FxKind::Warn && e.to == hero);
        attacked |= sim.enemies().iter().any(|e| e.3.target == Some(hero));
    }
    (warned, attacked)
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn a_new_hero_is_warned_off_turf_before_anyone_attacks() {
    let fresh = [0; hw_net::FACTIONS];
    assert_eq!(
        Attitude::of(factions::toward_hero(Faction::Lab, &fresh)),
        Attitude::Unfriendly
    );
    let (mut sim, hero) = turf(fresh);
    // Well outside: they don't care.
    let (warned, attacked) = wait(&mut sim, hero, 40.0, 3.0);
    assert!(!warned && !attacked);
    // Within earshot: a warning, and a few seconds to leave.
    let (warned, attacked) = wait(&mut sim, hero, 14.0, WARN_SECONDS - 1.5);
    assert!(warned, "the group warns the hero off");
    assert!(!attacked, "nobody attacks during the warning");
    // Staying means a fight.
    let (_, attacked) = wait(&mut sim, hero, 14.0, 3.0);
    assert!(attacked, "a hero who stays is attacked");
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn a_trespasser_who_walks_right_up_is_attacked() {
    let (mut sim, hero) = turf([0; hw_net::FACTIONS]);
    let (_, attacked) = wait(&mut sim, hero, 6.0, 1.0);
    assert!(attacked);
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn a_hostile_faction_attacks_on_sight_and_a_neutral_one_lets_people_pass() {
    // Deeds have turned the Lab hostile: no warning.
    let mut hated = [0i8; hw_net::FACTIONS];
    hated[Faction::Lab.index()] = -30;
    let (mut sim, hero) = turf(hated);
    let (warned, attacked) = wait(&mut sim, hero, 14.0, 1.0);
    assert!(attacked && !warned);
    // Deeds have made the Lab neutral: they let the hero walk past.
    let mut liked = [0i8; hw_net::FACTIONS];
    liked[Faction::Lab.index()] = 40;
    let (mut sim, hero) = turf(liked);
    let (warned, attacked) = wait(&mut sim, hero, 6.0, 10.0);
    assert!(!warned && !attacked);
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn beating_villains_moves_the_heros_standing() {
    let mut sim = Sim::with_solids(Seed(2), Arc::new(Obstacles::flat(GROUND_Y)));
    sim.set_spawn((DVec2::ZERO, DVec2::X));
    let hero = sim.spawn_hero(PlayerId(1), DVec3::new(0.0, GROUND_Y + 0.5, 0.0));
    sim.add_fight_spot(DVec3::new(6.0, GROUND_Y, 0.0), DVec2::new(-1.0, 0.0));
    let before = sim.sheet(PlayerId(1)).unwrap().regard;
    // Punch and bolt the nearest until the group is down.
    for t in 0..TICK_HZ as u64 * 90 {
        let me = sim.position(hero).unwrap();
        let foes: Vec<_> = sim
            .enemies()
            .into_iter()
            .filter(|e| e.3.standing())
            .collect();
        let Some(f) = foes
            .iter()
            .min_by(|a, b| a.2.distance(me).total_cmp(&b.2.distance(me)))
        else {
            break;
        };
        let mut input = InputState {
            target: Some(f.0),
            ..Default::default()
        };
        let to = DVec2::new(f.2.x - me.x, f.2.z - me.z);
        if to.length() > 2.0 {
            let d = to.normalize();
            input.move_x = d.x as f32;
            input.move_z = d.y as f32;
        }
        if t.is_multiple_of(4) {
            input.smash = true;
        }
        sim.set_input(PlayerId(1), input);
        sim.step();
    }
    let after = sim.sheet(PlayerId(1)).unwrap().regard;
    let lab = Faction::Lab.index();
    assert!(after[lab] < before[lab], "the Lab likes the hero less");
    assert!(
        after[Faction::Townsfolk.index()] > before[Faction::Townsfolk.index()],
        "townsfolk like the hero more"
    );
    assert!(after[Faction::Syndicate.index()] > before[Faction::Syndicate.index()]);
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn a_warning_is_heard_in_the_street() {
    let (mut sim, hero) = turf([0; hw_net::FACTIONS]);
    let (warned, _) = wait(&mut sim, hero, 14.0, 1.0);
    assert!(warned);
    assert!(sim
        .alarms
        .active()
        .any(|a| a.kind == hw_sim::alarms::AlarmKind::Warning));
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn chrome_jackets_hang_out_round_the_quarter() {
    use hw_sim::combat::EnemyKind;
    use hw_sim::GeneratedTown;
    let town = GeneratedTown::generate(Seed(3), 20_000.0);
    let mut sim = Sim::for_town(Seed(3), &town);
    let hero = sim.spawn_hero(PlayerId(1), sim.spawn_point(1));
    let spot = sim
        .fights()
        .spots
        .iter()
        .position(|s| s.only == Some(EnemyKind::Greaser))
        .expect("a Jackets hangout");
    let s = sim.fights().spots[spot].clone();
    let face = DVec3::new(s.facing.x, 0.0, s.facing.y);
    sim.set_body(
        hero,
        Body {
            pos: s.center + face * 40.0 + DVec3::Y,
            vel: DVec3::ZERO,
            grounded: false,
        },
    );
    for _ in 0..3 {
        sim.step();
    }
    let jackets: Vec<_> = sim
        .enemies()
        .into_iter()
        .filter(|e| e.1 == EnemyKind::Greaser && e.2.distance(s.center) < 8.0)
        .collect();
    assert_eq!(jackets.len(), 3, "{:?} at {:?}", jackets, s.center);
}
