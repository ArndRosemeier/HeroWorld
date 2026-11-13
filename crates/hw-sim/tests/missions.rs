//! An indoor case from start to end, headless: a hero asks the contact
//! for a case, goes to the building, the gang takes its rooms over the
//! floors (each group on its own floor's floor, the boss on top), and the
//! hero finds the clues and closes the case for karma.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_gen::layout::furnish::items;
use hw_gen::layout::office::plan_scheme;
use hw_gen::layout::solid::{pieces, Part};
use hw_gen::layout::Scheme;
use hw_net::{InputState, PlayerId};
use hw_sim::collision::{solids_of, Body, Obstacles};
use hw_sim::combat::{EnemyKind, Fights};
use hw_sim::floornav::{Building, FloorGraph, Floors};
use hw_sim::missions::board;
use hw_sim::missions::{self, CaseKind, Missions, GANGS};
use hw_sim::{Sim, GROUND_Y, TICK_HZ};
use std::sync::Arc;

/// An atrium building on flat ground, its layout's frame the world's.
fn town() -> (Sim, Arc<Floors>) {
    town_of(1)
}

/// `n` such buildings in a row, 120 m apart along +X.
fn town_of(n: usize) -> (Sim, Arc<Floors>) {
    let size = DVec2::new(56.0, 52.0);
    let l = (0..16u64)
        .find_map(|k| plan_scheme(Seed(60 + k), size, 5, Scheme::Atrium))
        .expect("it fits");
    let mut o = Obstacles::flat(GROUND_Y);
    let at = |k: usize| DVec2::new(k as f64 * 120.0, 0.0);
    for k in 0..n {
        let lift = |f: hw_gen::urban::footprint::Footprint| f.placed(at(k), DVec2::Y, GROUND_Y);
        o.add_all(
            pieces(&l)
                .iter()
                .filter(|p| p.part != Part::LiftDoor)
                .flat_map(|p| solids_of(&lift(p.shape)))
                .chain(
                    items(&l)
                        .iter()
                        .filter_map(|i| i.footprint())
                        .flat_map(|f| solids_of(&lift(f))),
                )
                .collect(),
        );
    }
    let mut sim = Sim::with_solids(Seed(9), Arc::new(o));
    sim.set_spawn((DVec2::new(-150.0, -150.0), DVec2::X));
    let floors = Arc::new(Floors::of(
        (0..n)
            .map(|k| {
                Building::at(
                    size * 0.5 + at(k),
                    DVec2::new(0.0, -1.0),
                    GROUND_Y,
                    FloorGraph::new(&l),
                )
            })
            .collect(),
    ));
    sim.world.resource_mut::<Fights>().floors = Some(floors.clone());
    sim.missions = Missions::new(floors.clone(), 9);
    (sim, floors)
}

/// The crew of fight spot `spot` goes down (cuffed).
fn put_down(sim: &mut Sim, spot: usize) {
    for (en, mut f) in sim
        .world
        .query::<(&hw_sim::combat::Enemy, &mut hw_sim::combat::Fighter)>()
        .iter_mut(&mut sim.world)
    {
        if en.spot == spot {
            f.health = 0.0;
            f.status = hw_net::Status::Down;
            f.until = 0;
        }
    }
}

fn stage(sim: &Sim, number: u16) -> u8 {
    sim.missions
        .case(number)
        .map_or(missions::IDLE, |r| r.stage)
}

fn put(sim: &mut Sim, hero: hw_core::NetId, at: DVec3) {
    sim.set_body(
        hero,
        Body {
            pos: at,
            grounded: true,
            ..Body::default()
        },
    );
    // The hero holds out: the test is about the case, not the fight.
    if let Some(mut f) = sim.fighter(hero) {
        f.health = f.max_health;
        sim.set_fighter(hero, f);
    }
}

#[test]
fn a_hero_takes_a_case_and_closes_it() {
    let (mut sim, floors) = town();
    let contact = DVec3::new(-150.0, GROUND_Y, -150.0);
    let hero = sim.spawn_hero(PlayerId(1), contact + DVec3::X);
    // An evidence case in the building (the test's own pick).
    let number = sim.missions.force(&mut sim.world, CaseKind::Evidence, 0, 0);
    assert_eq!(sim.missions.cases.len(), 1);
    let b = &floors.buildings[0];
    let front = b.to_town(b.graph.front());
    put(&mut sim, hero, front);
    for _ in 0..TICK_HZ * 2 {
        put(&mut sim, hero, front);
        sim.step();
    }
    assert_eq!(stage(&sim, number), missions::INSIDE);
    // The gang on the floors: every villain stands on a floor of the plan,
    // on three floors or more, with the boss on the top floor.
    for _ in 0..TICK_HZ {
        put(&mut sim, hero, front);
        sim.step();
    }
    let levels: Vec<f64> = b.graph.rooms().iter().map(|r| r.2 + GROUND_Y).collect();
    let foes = sim.enemies();
    assert!(foes.len() >= 15, "{} villains", foes.len());
    let mut floors_held: Vec<i64> = Vec::new();
    for (_, kind, p, _) in &foes {
        // A robot's parts sit up on it.
        if *kind == EnemyKind::WeakPoint {
            continue;
        }
        assert!(
            levels.iter().any(|y| (p.y - y).abs() < 0.4),
            "{kind:?} at {p:.2?} stands on no floor"
        );
        floors_held.push((p.y * 10.0).round() as i64);
    }
    floors_held.sort();
    floors_held.dedup();
    assert!(floors_held.len() >= 3, "on {floors_held:?}");
    let top = levels.iter().copied().fold(0.0, f64::max);
    let boss = foes
        .iter()
        .find(|f| f.1 == GANGS[0].boss)
        .expect("the boss");
    assert!(
        (boss.2.y - top).abs() < 0.4,
        "the boss at {:.1?}, top {top}",
        boss.2
    );
    assert!(foes.iter().all(|f| f.1 != EnemyKind::Colossus));
    // The clues, each held for its seconds.
    let clues = sim.missions.case(number).expect("open").clues();
    assert_eq!(clues.len(), missions::CLUES);
    // One thing to find, up on a floor above the street.
    assert_eq!(clues.len(), 1, "one thing to find: {clues:?}");
    assert!(
        clues[0].0.y > GROUND_Y + 1.0,
        "on the ground floor: {clues:?}"
    );
    let karma = sim.sheet(PlayerId(1)).expect("a sheet").karma;
    let at = clues[0].0;
    // Its room's crew stands over it: holding E does nothing yet.
    let guard = sim.missions.case(number).unwrap().guards()[0].expect("a guard");
    hold(&mut sim, hero, at, missions::CLUE_SECONDS * 1.3);
    assert!(
        !sim.missions.case(number).unwrap().clues()[0].1,
        "taken past its guards"
    );
    put_down(&mut sim, guard);
    let before = sim.enemies().len();
    hold(&mut sim, hero, at, missions::CLUE_SECONDS * 1.3);
    let r = sim.missions.case(number).expect("still open: not out yet");
    assert!(r.clues()[0].1 && r.carried(), "the thing is taken");
    // The gang knows: a crew cuts off the way out, and everyone comes.
    assert!(r.alarms() >= 1);
    let (_, _, cutoff, _) = r.spots();
    assert!(cutoff.is_some(), "a crew at the way out");
    assert!(sim.enemies().len() >= before + missions::CUTOFF_SIZE as usize);
    let after_me = sim
        .world
        .query::<(&hw_sim::combat::Enemy, &hw_sim::combat::Fighter)>()
        .iter(&sim.world)
        .filter(|(_, f)| f.target == Some(hero))
        .count();
    assert!(after_me >= 6, "{after_me} villains after the hero");
    // Still inside: the case stays open.
    for _ in 0..TICK_HZ * 2 {
        put(&mut sim, hero, at);
        sim.step();
    }
    assert_eq!(sim.missions.closed, 0, "closed before the hero got out");
    // Out on the street with it.
    let outside = front + DVec3::new(0.0, 0.0, -12.0);
    assert!(!b.holds(outside, missions::OUT_MARGIN));
    for _ in 0..TICK_HZ * 2 {
        put(&mut sim, hero, outside);
        sim.step();
    }
    assert_eq!(sim.missions.closed, 1);
    assert!(sim.missions.cases.is_empty());
    let after = sim.sheet(PlayerId(1)).expect("a sheet").karma;
    assert!(after > karma, "karma {karma} -> {after}");
    // The gang that was left has gone.
    assert!(sim.enemies().is_empty());
}

fn hold(sim: &mut Sim, hero: hw_core::NetId, at: DVec3, seconds: f64) {
    sim.set_input(
        PlayerId(1),
        InputState {
            revive: true,
            ..InputState::default()
        },
    );
    for _ in 0..(seconds * TICK_HZ as f64) as usize {
        put(sim, hero, at);
        sim.step();
    }
    sim.set_input(PlayerId(1), InputState::default());
    sim.step();
}

/// Where the hostages are now (bystanders of the case).
fn hostages(sim: &mut Sim, ids: &[u32]) -> Vec<DVec3> {
    sim.world
        .query::<(&hw_sim::Motion, &hw_sim::incidents::Bystander)>()
        .iter(&sim.world)
        .filter(|(_, b)| ids.contains(&b.incident))
        .map(|(m, _)| m.0.pos)
        .collect()
}

#[test]
fn hostages_are_freed_room_by_room_and_walk_out() {
    let (mut sim, floors) = town();
    let b = &floors.buildings[0];
    let front = b.to_town(b.graph.front());
    let hero = sim.spawn_hero(PlayerId(1), front);
    let number = sim.missions.force(&mut sim.world, CaseKind::Hostages, 0, 1);
    for _ in 0..TICK_HZ * 3 {
        put(&mut sim, hero, front);
        sim.step();
    }
    assert_eq!(stage(&sim, number), missions::INSIDE);
    let ids = sim.missions.case(number).expect("open").hostages();
    assert_eq!(ids.len(), missions::HOSTAGES);
    let clues = |sim: &Sim| sim.missions.case(number).expect("open").clues();
    let held = clues(&sim);
    assert_eq!(hostages(&mut sim, &ids).len(), missions::HOSTAGES);
    let mut ys: Vec<i64> = held.iter().map(|c| (c.0.y * 10.0).round() as i64).collect();
    ys.sort();
    ys.dedup();
    assert!(ys.len() >= 2, "hostages on one floor: {held:?}");
    // Guarded: holding E by them does nothing while their crew stands.
    hold(&mut sim, hero, held[0].0, missions::FREE_SECONDS * 1.5);
    assert!(!clues(&sim)[0].1, "untied under their guards' noses");
    // Every crew down (cuffed, waiting for the drone).
    for (id, _, _, mut f) in sim.enemies() {
        f.health = 0.0;
        f.status = hw_net::Status::Down;
        sim.set_fighter(id, f);
    }
    sim.step();
    let karma = sim.sheet(PlayerId(1)).expect("a sheet").karma;
    for (at, _) in &held {
        hold(&mut sim, hero, *at, missions::FREE_SECONDS * 1.3);
    }
    assert!(
        sim.missions
            .case(number)
            .is_none_or(|r| r.clues().iter().all(|c| c.1)),
        "not all freed"
    );
    // Out of the hero's way, the freed walk out: down the stairs, out of
    // the door and gone.
    let mut lowest = f64::MAX;
    let mut last = Vec::new();
    for _ in 0..TICK_HZ * 150 {
        put(&mut sim, hero, front);
        sim.step();
        let now = hostages(&mut sim, &ids);
        if now.is_empty() {
            break;
        }
        lowest = now.iter().map(|p| p.y).fold(lowest, f64::min);
        last = now;
    }
    assert!(
        hostages(&mut sim, &ids).is_empty(),
        "still inside after 150 s at {last:.1?}"
    );
    assert!((lowest - GROUND_Y).abs() < 0.3, "never came down: {lowest}");
    for p in &last {
        assert!(
            p.distance(front) < 12.0,
            "gone at {p:.1?}, not by the door {front:.1?}"
        );
    }
    assert_eq!(sim.missions.closed, 1);
    let after = sim.sheet(PlayerId(1)).expect("a sheet").karma;
    assert!(after > karma, "karma {karma} -> {after}");
}

#[test]
fn crimes_are_taken_from_the_board_one_to_a_building() {
    let (mut sim, _) = town_of(2);
    let contact = DVec3::new(-150.0, GROUND_Y, -150.0);
    sim.spawn_hero(PlayerId(1), contact + DVec3::X);
    let take = |sim: &mut Sim, number: u16| {
        sim.missions.take_case(number);
        for _ in 0..TICK_HZ / 2 {
            sim.step();
        }
    };
    // Crime 1, then crime 2 (the other building).
    take(&mut sim, 1);
    assert_eq!(sim.missions.cases.len(), 1);
    take(&mut sim, 2);
    assert_eq!(sim.missions.cases.len(), 2);
    let numbers: Vec<u16> = sim.missions.cases.iter().map(|r| r.number).collect();
    assert_eq!(numbers, vec![1, 2]);
    // Each case is one step in a hideout.
    for r in &sim.missions.cases {
        assert_eq!(r.steps().len(), 1);
    }
    assert_ne!(
        sim.missions.cases[0].site().building,
        sim.missions.cases[1].site().building
    );
    // Crime 3 is in building 0 again: not while case 1 is open there. One
    // past the board's window is no crime yet.
    take(&mut sim, 3);
    take(&mut sim, board::BOARD as u16 + 1);
    assert_eq!(sim.missions.cases.len(), 2);
    // Both are on the air, each by its number, and the board says which
    // crimes are taken.
    let news = sim.missions.snapshot();
    let told: Vec<u16> = news
        .iter()
        .filter_map(|m| match m {
            hw_net::ToClient::Case { number, .. } => Some(*number),
            _ => None,
        })
        .collect();
    assert_eq!(told, vec![1, 2]);
    let crimes = news
        .iter()
        .find_map(|m| match m {
            hw_net::ToClient::CaseBoard { crimes, open, .. } => {
                assert_eq!(*open, 2);
                Some(crimes)
            }
            _ => None,
        })
        .expect("the board");
    assert_eq!(crimes.len(), board::BOARD);
    let taken: Vec<u16> = crimes
        .iter()
        .filter(|c| c.taken)
        .map(|c| c.number)
        .collect();
    assert!(
        taken.starts_with(&[1, 2]) && taken.contains(&3),
        "{taken:?}"
    );
}

#[test]
fn open_cases_come_back_from_the_hosts_file() {
    let (mut sim, _) = town_of(2);
    let contact = DVec3::new(-150.0, GROUND_Y, -150.0);
    // Case 7 as it was planned: planned again from the same numbers.
    let places = sim.missions.places().expect("places").clone();
    let (variant, was) = (
        0,
        missions::plan::crime_plan(&places, 9, 7, 2).expect("a plan"),
    );
    let mut sheet = hw_sim::progress::starter();
    sheet.cases = hw_net::CaseBook {
        world: 9,
        opened: 7,
        closed: 4,
        open: vec![hw_net::SavedCase {
            number: 7,
            gang: 2,
            variant,
            step: 0,
        }],
    };
    sim.spawn_hero_with(PlayerId(1), contact + DVec3::X, sheet);
    for _ in 0..TICK_HZ * 3 {
        sim.step();
    }
    assert_eq!(sim.missions.closed, 4);
    let r = sim.missions.case(7).expect("case 7 is open again");
    assert_eq!(r.gang, 2);
    assert_eq!(r.steps(), &was.steps[..]);
    let first = r.site().building.expect("in a building");
    // A taken crime gets its own number and the other building, and both
    // are written back into the file.
    sim.missions.take_case(8);
    for _ in 0..TICK_HZ * 3 {
        sim.step();
    }
    let numbers: Vec<u16> = sim.missions.cases.iter().map(|r| r.number).collect();
    assert_eq!(numbers, vec![7, 8]);
    let book = sim.sheet(PlayerId(1)).expect("a sheet").cases;
    assert_eq!((book.opened, book.closed, book.world), (8, 4, 9));
    let saved: Vec<(u16, u8)> = book.open.iter().map(|c| (c.number, c.variant)).collect();
    assert_eq!(saved[0], (7, variant));
    assert_eq!(saved[1].0, 8);
    let second = sim.missions.case(8).expect("case 8").site().building;
    assert_eq!(second, Some(1 - first));
}

#[test]
fn a_book_from_another_world_keeps_its_count_but_not_its_cases() {
    let (mut sim, _) = town();
    let mut sheet = hw_sim::progress::starter();
    sheet.cases = hw_net::CaseBook {
        world: 12345,
        opened: 3,
        closed: 2,
        open: vec![hw_net::SavedCase {
            number: 3,
            gang: 1,
            variant: 0,
            step: 0,
        }],
    };
    sim.spawn_hero_with(PlayerId(1), DVec3::new(-150.0, GROUND_Y, -148.0), sheet);
    for _ in 0..TICK_HZ * 2 {
        sim.step();
    }
    assert!(sim.missions.cases.is_empty());
    assert_eq!((sim.missions.opened, sim.missions.closed), (3, 2));
}

#[test]
fn a_room_that_fights_on_calls_the_next_room_only() {
    let (mut sim, floors) = town();
    let hero = sim.spawn_hero(PlayerId(1), DVec3::new(-150.0, GROUND_Y, -150.0));
    let number = sim.missions.force(&mut sim.world, CaseKind::Takeover, 0, 0);
    let b = &floors.buildings[0];
    let front = b.to_town(b.graph.front());
    for _ in 0..TICK_HZ * 2 {
        put(&mut sim, hero, front);
        sim.step();
    }
    let (door, rooms, _, boss) = sim.missions.case(number).unwrap().spots();
    assert!(door.is_some(), "lookouts at the way in");
    assert!(rooms.len() >= 4 && boss.is_some());
    // One room's crew gets the hero to fight; the hero holds out.
    let first = rooms[0];
    let on = |sim: &mut Sim| -> Vec<usize> {
        let mut v: Vec<usize> = sim
            .world
            .query::<(&hw_sim::combat::Enemy, &hw_sim::combat::Fighter)>()
            .iter(&sim.world)
            .filter(|(_, f)| f.target == Some(hero) && f.standing())
            .map(|(e, _)| e.spot)
            .collect();
        v.sort();
        v.dedup();
        v
    };
    let home = sim
        .world
        .query::<&hw_sim::combat::Enemy>()
        .iter(&sim.world)
        .find(|e| e.spot == first)
        .map(|e| e.home)
        .unwrap();
    for (en, mut f) in sim
        .world
        .query::<(&hw_sim::combat::Enemy, &mut hw_sim::combat::Fighter)>()
        .iter_mut(&mut sim.world)
    {
        if en.spot == first {
            f.target = Some(hero);
        }
    }
    let stand = home + DVec3::new(2.0, 0.0, 0.0);
    for _ in 0..(TICK_HZ as f64 * (missions::ALARM_SECONDS + 3.0)) as usize {
        put(&mut sim, hero, stand);
        sim.step();
    }
    let r = sim.missions.case(number).unwrap();
    // The room called; so may a crew that saw the hero come in. Nobody
    // who was called calls on.
    assert!((1..=3).contains(&r.alarms()), "{} alarms", r.alarms());
    let fighting = on(&mut sim);
    assert!(fighting.contains(&first));
    let called: Vec<usize> = fighting
        .iter()
        .copied()
        .filter(|s| *s != first && Some(*s) != boss)
        .collect();
    // The room itself may have drawn neighbours in sight; the alarm adds
    // one, and never the whole gang.
    assert!(!called.is_empty(), "nobody came: {fighting:?}");
    assert!(
        fighting.len() < rooms.len() + 2,
        "the whole gang came: {fighting:?}"
    );
}

#[test]
fn a_hurt_boss_breaks_off_and_calls_for_help() {
    let (mut sim, floors) = town();
    let hero = sim.spawn_hero(PlayerId(1), DVec3::new(-150.0, GROUND_Y, -150.0));
    let number = sim.missions.force(&mut sim.world, CaseKind::Takeover, 0, 0);
    let b = &floors.buildings[0];
    let front = b.to_town(b.graph.front());
    for _ in 0..TICK_HZ * 2 {
        put(&mut sim, hero, front);
        sim.step();
    }
    let boss_spot = sim
        .missions
        .case(number)
        .unwrap()
        .spots()
        .3
        .expect("a boss");
    let boss_kind = GANGS[0].boss;
    let mut at = DVec3::ZERO;
    for (en, m, mut f) in sim
        .world
        .query::<(
            &hw_sim::combat::Enemy,
            &hw_sim::Motion,
            &mut hw_sim::combat::Fighter,
        )>()
        .iter_mut(&mut sim.world)
    {
        if en.spot == boss_spot && en.kind == boss_kind {
            f.health = f.max_health * 0.3;
            f.target = Some(hero);
            at = m.0.pos;
        }
    }
    let stand = at + DVec3::new(3.0, 0.0, 0.0);
    for _ in 0..TICK_HZ + 2 {
        put(&mut sim, hero, stand);
        sim.step();
    }
    let news = sim.missions.snapshot();
    let boss = news
        .iter()
        .find_map(|m| match m {
            hw_net::ToClient::Case { boss, .. } => Some(*boss),
            _ => None,
        })
        .unwrap();
    assert_eq!(boss, 3, "the boss is on the run");
    // It runs through the building to a crew that still stands.
    let run_from = at;
    assert_eq!(sim.missions.case(number).unwrap().alarms(), 1);
    let where_is = |sim: &mut Sim| {
        sim.world
            .query::<(&hw_sim::combat::Enemy, &hw_sim::Motion)>()
            .iter(&sim.world)
            .find(|(e, _)| e.spot == boss_spot && e.kind == boss_kind)
            .map(|(e, m)| (m.0.pos, e.home))
            .unwrap()
    };
    let (_, home) = where_is(&mut sim);
    let far = run_from.distance(home);
    assert!(far > 6.0, "somewhere to run to: {far:.1} m");
    for _ in 0..TICK_HZ * 4 {
        put(&mut sim, hero, stand);
        sim.step();
    }
    // On its way by the stairs (the straight line to the crew may grow
    // on the way round), fighting nobody.
    let (now_at, _) = where_is(&mut sim);
    assert!(
        now_at.distance(run_from) > 2.5,
        "the boss ran {:.1} m in 4 s",
        now_at.distance(run_from)
    );
    let fights = sim
        .world
        .query::<(&hw_sim::combat::Enemy, &hw_sim::combat::Fighter)>()
        .iter(&sim.world)
        .find(|(e, _)| e.spot == boss_spot && e.kind == boss_kind)
        .map(|(_, f)| f.target)
        .unwrap();
    assert_eq!(fights, None, "a boss on the run fights nobody");
    // It runs once only, and stands to fight again after.
    for _ in 0..(TICK_HZ as f64 * (missions::BOSS_RUN_SECONDS + 2.0)) as usize {
        put(&mut sim, hero, stand);
        sim.step();
    }
    let boss = sim
        .missions
        .snapshot()
        .iter()
        .find_map(|m| match m {
            hw_net::ToClient::Case { boss, .. } => Some(*boss),
            _ => None,
        })
        .unwrap();
    assert_eq!(boss, 1);
}

#[test]
fn crimes_nobody_takes_go_off_the_board_and_new_ones_come() {
    let (mut sim, _) = town_of(2);
    sim.spawn_hero(PlayerId(1), DVec3::new(-150.0, GROUND_Y, -150.0));
    let board_now = |sim: &Sim| {
        sim.missions
            .snapshot()
            .into_iter()
            .find_map(|m| match m {
                hw_net::ToClient::CaseBoard { crimes, missed, .. } => Some((crimes, missed)),
                _ => None,
            })
            .expect("a board")
    };
    sim.step();
    // Crime 1 is taken: it stays however long it takes.
    sim.missions.take_case(1);
    for _ in 0..TICK_HZ {
        sim.step();
    }
    let (first, missed) = board_now(&sim);
    assert_eq!(missed, 0);
    assert_eq!(first.len(), board::BOARD);
    assert!(first.iter().all(|c| c.minutes >= 5), "{first:?}");
    assert!(first.iter().any(|c| c.minutes == 255 && c.taken));
    // The longest any crime stays is 30 minutes.
    let mut left = 31 * 60 * TICK_HZ;
    while board_now(&sim).1 < 9 && left > 0 {
        sim.step();
        left -= 1;
    }
    let (crimes, missed) = board_now(&sim);
    assert_eq!(missed, 9, "every crime but the taken one went");
    assert_eq!(crimes.len(), board::BOARD, "new ones came");
    assert!(crimes.iter().any(|c| c.number == 1 && c.taken));
    assert!(crimes.iter().filter(|c| c.number > 10).count() >= 9);
    // The streets round a hideout nobody went to are rough for a while.
    assert!(!sim.missions.emboldened.is_empty());
    assert_eq!(sim.missions.cases.len(), 1);
}

#[test]
fn a_closed_hideout_gives_up_and_someone_says_thanks() {
    let (mut sim, floors) = town();
    let hero = sim.spawn_hero(PlayerId(1), DVec3::new(-150.0, GROUND_Y, -150.0));
    sim.step();
    sim.missions.take_case(1);
    sim.step();
    let b = &floors.buildings[0];
    let front = b.to_town(b.graph.front());
    let outside = front + DVec3::new(0.0, 0.0, -12.0);
    for _ in 0..TICK_HZ * 2 {
        put(&mut sim, hero, front);
        sim.step();
    }
    let r = sim.missions.case(1).expect("crime 1 is open");
    assert!(r.is_hideout());
    let (_, _, _, boss) = r.spots();
    let job = r.job();
    let clue = r.clues().first().map(|c| c.0);
    let guard = r.guards().first().copied().flatten();
    let standing = |sim: &mut Sim| {
        sim.enemies()
            .iter()
            .filter(|e| e.3.fighting() && e.1 != EnemyKind::WeakPoint)
            .count()
    };
    assert!(standing(&mut sim) >= 8);
    match job {
        CaseKind::Evidence => {
            if let Some(g) = guard {
                put_down(&mut sim, g);
            }
            hold(
                &mut sim,
                hero,
                clue.expect("a thing"),
                missions::CLUE_SECONDS * 1.3,
            );
        }
        _ => put_down(&mut sim, boss.expect("a boss")),
    }
    // Out on the street; the drones take the boss's crew (a takeover is
    // done when they are gone).
    let mut left = TICK_HZ * 90;
    while sim.missions.closed == 0 && left > 0 {
        put(&mut sim, hero, outside);
        sim.step();
        left -= 1;
    }
    assert_eq!(sim.missions.closed, 1, "the case closed");
    // Nobody fights on, and someone stands outside to say thanks.
    assert_eq!(standing(&mut sim), 0, "the rest gave up");
    let thanks: Vec<DVec3> = sim
        .world
        .query::<(&hw_sim::Motion, &hw_sim::incidents::Bystander)>()
        .iter(&sim.world)
        .map(|(m, _)| m.0.pos)
        .collect();
    assert_eq!(thanks.len(), 1, "one to say thanks");
    assert!(!b.holds(thanks[0], 0.0), "outside the building");
    // The police drones take them all in.
    let mut left = TICK_HZ * 240;
    while !sim.enemies().is_empty() && left > 0 {
        put(&mut sim, hero, outside);
        sim.step();
        left -= 1;
    }
    let rest: Vec<_> = sim.enemies().iter().map(|e| (e.1, e.3.status)).collect();
    assert!(rest.is_empty(), "still there: {rest:?}");
}
