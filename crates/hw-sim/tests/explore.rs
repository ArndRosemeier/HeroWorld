//! Exploring, headless: heroes gather wisps, stand in springs and on
//! waystones' altars and run the courses; every sight pays once, and what
//! was found is in the hero's file.

use glam::DVec3;
use hw_core::{NetId, Seed};
use hw_gen::urban::sights::CourseKind;
use hw_net::protocol::{course, sight};
use hw_net::{FindBook, FxKind, InputState, PlayerId};
use hw_sim::augments::Grade;
use hw_sim::balance::hero_sheet;
use hw_sim::collision::Body;
use hw_sim::{explore, progress, GeneratedTown, Sim, TICK_HZ};

const ME: PlayerId = PlayerId(1);

fn world() -> (Sim, NetId) {
    let town = GeneratedTown::generate(Seed(1), 6000.0);
    let mut sim = Sim::for_town(Seed(1), &town);
    sim.explore.wait();
    let at = sim.spawn_point(0);
    let hero = sim.spawn_hero_with(ME, at, hero_sheet(5, &[]));
    (sim, hero)
}

/// Puts the hero's feet at `at`, standing still.
fn put(sim: &mut Sim, hero: NetId, at: DVec3) {
    sim.set_body(
        hero,
        Body {
            pos: at,
            vel: DVec3::ZERO,
            grounded: false,
        },
    );
}

fn finds(sim: &mut Sim) -> FindBook {
    sim.sheet(ME).expect("a sheet").finds
}

#[test]
fn a_wisp_is_gathered_once_and_every_tenth_brings_an_augment() {
    let (mut sim, hero) = world();
    let sights = sim.explore.sights().expect("planned").clone();
    assert!(sights.wisps.len() >= 20);
    let before = sim.sheet(ME).expect("a sheet");
    let w = sights.wisps[3];
    put(&mut sim, hero, w.at - DVec3::Y);
    sim.step();
    let e = sim
        .fx()
        .iter()
        .find(|e| e.kind == FxKind::Sight)
        .copied()
        .expect("the find is told");
    assert_eq!((e.theme, e.amount, e.knock), (sight::WISP, 3, true));
    assert!(sim.fx().iter().any(|e| e.kind == FxKind::Karma));
    let after = sim.sheet(ME).expect("a sheet");
    assert!(after.finds.has_wisp(3) && after.finds.wisp_count() == 1);
    assert_eq!(after.finds.world, sim.world_key());
    let paid = explore::share_karma(before.lifetime, explore::WISP_SHARE);
    assert_eq!(after.lifetime, before.lifetime + paid);
    // Staying there pays nothing more.
    for _ in 0..30 {
        put(&mut sim, hero, w.at - DVec3::Y);
        sim.step();
        assert!(!sim.fx().iter().any(|e| e.kind == FxKind::Sight));
    }
    assert_eq!(sim.sheet(ME).expect("a sheet").lifetime, after.lifetime);
    // Nine more: the tenth comes with a rare augment.
    assert!(after.bag.is_empty());
    for i in 10..19 {
        put(&mut sim, hero, sights.wisps[i].at - DVec3::Y);
        sim.step();
    }
    sim.step();
    let s = sim.sheet(ME).expect("a sheet");
    assert_eq!(s.finds.wisp_count(), 10);
    assert_eq!(s.bag.len(), 1, "the tenth wisp's augment");
    assert_eq!(Grade::of(s.bag[0]), Grade::Rare);
}

#[test]
fn a_spring_gives_its_augment_once_and_fills_a_hero_up() {
    let (mut sim, hero) = world();
    let sights = sim.explore.sights().expect("planned").clone();
    let spring = sights.wild.springs.first().expect("a spring").clone();
    // Hurt and tired, set down over the pool: the terraces hold them up.
    let mut f = sim.fighter(hero).expect("a fighter");
    f.health = f.max_health * 0.3;
    f.energy = 0.0;
    sim.set_fighter(hero, f);
    put(&mut sim, hero, spring.pool() + DVec3::Y * 0.5);
    let mut told = Vec::new();
    for _ in 0..TICK_HZ {
        sim.step();
        told.extend(sim.fx().iter().filter(|e| e.kind == FxKind::Sight).copied());
    }
    let at = sim.position(hero).expect("there");
    assert!(
        (at.y - spring.pool().y).abs() < 0.2,
        "the hero stands on the pool: {:.2} against {:.2}",
        at.y,
        spring.pool().y
    );
    assert_eq!(told.len(), 1, "told once: {told:?}");
    assert_eq!((told[0].theme, told[0].knock), (sight::SPRING, true));
    let f = sim.fighter(hero).expect("a fighter");
    assert_eq!((f.health, f.energy), (f.max_health, f.max_energy));
    let s = sim.sheet(ME).expect("a sheet");
    assert!(s.finds.has_spring(0));
    assert_eq!(s.bag.len(), 1, "the spring's augment");
    assert_ne!(Grade::of(s.bag[0]), Grade::Common);
    // Back later, hurt again: filled up, nothing more to find.
    let lifetime = s.lifetime;
    let mut f = sim.fighter(hero).expect("a fighter");
    f.health = 1.0;
    sim.set_fighter(hero, f);
    for _ in 0..TICK_HZ * 3 {
        sim.step();
    }
    let f = sim.fighter(hero).expect("a fighter");
    assert_eq!(f.health, f.max_health);
    let s = sim.sheet(ME).expect("a sheet");
    assert_eq!((s.bag.len(), s.lifetime), (1, lifetime));
}

#[test]
fn waystones_pay_once_and_keep_singing() {
    let (mut sim, hero) = world();
    let sights = sim.explore.sights().expect("planned").clone();
    let ring = sights.wild.stones.first().expect("a ring").clone();
    let before = sim.sheet(ME).expect("a sheet").lifetime;
    put(&mut sim, hero, ring.altar() + DVec3::Y * 0.3);
    let mut told = Vec::new();
    for _ in 0..TICK_HZ * 14 {
        sim.step();
        told.extend(sim.fx().iter().filter(|e| e.kind == FxKind::Sight).copied());
    }
    assert!(told.len() >= 3, "they sing again and again: {}", told.len());
    assert!(told[0].knock && told[1..].iter().all(|e| !e.knock));
    assert!(told.iter().all(|e| e.theme == sight::STONES));
    let s = sim.sheet(ME).expect("a sheet");
    assert!(s.finds.has_stones(0));
    assert_eq!(
        s.lifetime,
        before + explore::share_karma(before, explore::STONES_SHARE)
    );
    // The stones stand in the hero's way: walking out of the ring through
    // one does not work.
    let stone = ring.footprints(|p| sim.solids().ground_at(p))[1];
    let to = stone.at() - ring.pos;
    let input = InputState {
        move_x: to.x as f32,
        move_z: to.y as f32,
        ..Default::default()
    };
    sim.set_input(ME, input);
    for _ in 0..TICK_HZ * 4 {
        sim.step();
    }
    let at = sim.position(hero).expect("there");
    let out = glam::DVec2::new(at.x, at.z).distance(ring.pos);
    assert!(
        out < to.length() + 0.5,
        "walked through a standing stone: {out:.1} m out, the stone at {:.1}",
        to.length()
    );
}

/// Carries the hero through the gates of course `c` in order, a few ticks
/// at each; the run's events.
fn run(sim: &mut Sim, hero: NetId, c: usize, fly: bool) -> Vec<hw_net::Fx> {
    let sights = sim.explore.sights().expect("planned").clone();
    let mut told = Vec::new();
    // From outside the first gate.
    put(sim, hero, sights.courses[c].gates[0].at + DVec3::Y * 80.0);
    sim.step();
    sim.set_input(
        ME,
        InputState {
            fly,
            ..Default::default()
        },
    );
    for g in &sights.courses[c].gates {
        for _ in 0..3 {
            put(sim, hero, g.at - DVec3::Y);
            sim.step();
            told.extend(
                sim.fx()
                    .iter()
                    .filter(|e| e.kind == FxKind::Course)
                    .copied(),
            );
        }
    }
    told
}

#[test]
fn a_course_is_timed_gate_by_gate_and_a_medal_pays_once() {
    let (mut sim, hero) = world();
    let sights = sim.explore.sights().expect("planned").clone();
    let c = sights
        .courses
        .iter()
        .position(|c| c.kind == CourseKind::Flight)
        .expect("a flight course");
    let gates = sights.courses[c].gates.len();
    let before = sim.sheet(ME).expect("a sheet");
    let told = run(&mut sim, hero, c, false);
    assert_eq!(told.len(), gates, "an event per gate: {told:?}");
    assert_eq!(told[0].element, course::START);
    for (k, e) in told.iter().enumerate().take(gates - 1).skip(1) {
        assert_eq!(
            (e.element, e.amount as usize, e.theme as usize),
            (course::GATE, k, c)
        );
    }
    let end = told[gates - 1];
    assert_eq!(end.element, course::FINISH);
    assert!(end.knock && end.defeat, "a best time and a new medal");
    assert!(sim.explore.run_of(ME).is_none());
    sim.step();
    let s = sim.sheet(ME).expect("a sheet");
    assert_eq!(s.finds.best(c as u8), Some(end.amount));
    // Carried through in a blink: gold, all three medals paid, and the
    // rare augment for gold.
    assert_eq!(sights.courses[c].medal(end.amount as f64 / 10.0), 3);
    let all: f64 = explore::MEDAL_SHARE.iter().sum();
    assert_eq!(
        s.lifetime,
        before.lifetime + explore::share_karma(before.lifetime, all)
    );
    assert_eq!(s.bag.len(), 1);
    // Again: timed, but nothing more to earn.
    let told = run(&mut sim, hero, c, false);
    let end = told.last().expect("a finish");
    assert_eq!(end.element, course::FINISH);
    assert!(!end.defeat);
    sim.step();
    let again = sim.sheet(ME).expect("a sheet");
    assert_eq!((again.lifetime, again.bag.len()), (s.lifetime, 1));
}

#[test]
fn flying_ends_a_rooftop_run_and_leaving_ends_any() {
    let (mut sim, hero) = world();
    let sights = sim.explore.sights().expect("planned").clone();
    // A hero who can fly.
    let fly = hw_sim::powers::POWERS
        .iter()
        .find(|d| d.name == "Flight")
        .expect("flight")
        .id;
    sim.set_sheet(ME, hero_sheet(5, &[fly]));
    if let Some(c) = sights
        .courses
        .iter()
        .position(|c| c.kind == CourseKind::Rooftop)
    {
        let told = run(&mut sim, hero, c, true);
        assert!(
            told.iter().all(|e| e.element != course::START),
            "a rooftop run started on the wing: {told:?}"
        );
        // Started on foot, then taking off: off.
        let g = &sights.courses[c].gates;
        sim.set_input(ME, InputState::default());
        put(&mut sim, hero, g[0].at + DVec3::Y * 80.0);
        sim.step();
        put(&mut sim, hero, g[0].at - DVec3::Y);
        sim.step();
        assert!(sim.explore.run_of(ME).is_some());
        sim.set_input(
            ME,
            InputState {
                fly: true,
                ..Default::default()
            },
        );
        sim.step();
        assert!(sim.explore.run_of(ME).is_none());
        assert!(sim
            .fx()
            .iter()
            .any(|e| e.kind == FxKind::Course && e.element == course::OFF));
        sim.set_input(ME, InputState::default());
    }
    // A flight run, then away over the hills: off.
    let c = sights
        .courses
        .iter()
        .position(|c| c.kind == CourseKind::Flight)
        .expect("a flight course");
    let g = &sights.courses[c].gates;
    put(&mut sim, hero, g[0].at + DVec3::Y * 80.0);
    sim.step();
    put(&mut sim, hero, g[0].at - DVec3::Y);
    sim.step();
    assert_eq!(sim.explore.run_of(ME).map(|r| r.next), Some(1));
    put(&mut sim, hero, g[1].at + DVec3::new(0.0, 900.0, 0.0));
    sim.step();
    assert!(sim.explore.run_of(ME).is_none());
}

#[test]
fn finds_from_another_world_make_way_for_this_one() {
    let (mut sim, hero) = world();
    let sights = sim.explore.sights().expect("planned").clone();
    let mut sheet = hero_sheet(5, &[]);
    sheet.finds = FindBook {
        world: 99,
        wisps: vec![0xff, 0xff],
        springs: 3,
        stones: 1,
        bests: vec![(0, 100)],
    };
    sim.set_sheet(ME, sheet.clone());
    // Kept as long as nothing is found here.
    sim.step();
    assert_eq!(finds(&mut sim), sheet.finds);
    put(&mut sim, hero, sights.wisps[2].at - DVec3::Y);
    sim.step();
    let b = finds(&mut sim);
    assert_eq!(b.world, sim.world_key());
    assert_eq!((b.wisp_count(), b.springs, b.stones), (1, 0, 0));
    assert!(b.bests.is_empty());
    // A wisp is worth the same share of a level at every level.
    let (low, high) = (
        explore::share_karma(0, explore::WISP_SHARE),
        explore::share_karma(progress::karma_for_level(15), explore::WISP_SHARE),
    );
    assert!(low >= 20 && high > low * 3, "{low} and {high}");
}
