//! The colossus, headless: flying heroes take on the walking tank on flat
//! ground. A bot plays each hero the way the fight is meant to be played:
//! shoot the knees while it walks, stay in the air over its stomps and
//! discharges, go for the core when it is down, deal with the robo-sentries
//! that drop. The tank must go down, get up again, go down for good, and the
//! whole fight must end the same way every time.

use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_net::{FxKind, InputState, PlayerId, Status};
use hw_sim::collision::Obstacles;
use hw_sim::combat::{EnemyKind, Kit};
use hw_sim::{Sim, GROUND_Y, TICK_HZ};
use std::sync::Arc;

fn arena(seed: u64) -> Sim {
    let mut sim = Sim::with_solids(Seed(seed), Arc::new(Obstacles::flat(GROUND_Y)));
    sim.set_spawn((DVec2::ZERO, DVec2::X));
    sim
}

/// A level 8 hero who can fly and has every power.
fn hero(sim: &mut Sim, player: u16, at: DVec3) -> NetId {
    let id = sim.spawn_hero(PlayerId(player), at);
    sim.set_fighter(id, hw_sim::combat::Fighter::hero_at(8, Kit::everything()));
    id
}

/// Hover 9 m up, 18 m from what we shoot; pick sentries first, then the
/// core when it is open, else the knees.
fn bot(sim: &mut Sim, me: NetId, tank: NetId, tick: u64) -> InputState {
    let at = sim.position(me).unwrap();
    let f = sim.fighter(me).unwrap();
    let foes = sim.enemies();
    let up = |s: Status| matches!(s, Status::Standing | Status::Knocked);
    let pick = |want: &dyn Fn(EnemyKind, NetId) -> bool| {
        foes.iter()
            .filter(|(id, k, _, f)| up(f.status) && want(*k, *id))
            .min_by(|a, b| a.2.distance(at).total_cmp(&b.2.distance(at)))
            .map(|(id, _, p, _)| (*id, *p))
    };
    let kneeling = sim
        .fighter(tank)
        .is_some_and(|f| f.marks & hw_net::EFFECT_KNEELING != 0);
    let target = pick(&|k, _| k == EnemyKind::Sentry)
        .or_else(|| {
            if kneeling {
                pick(&|_, id| id == tank)
            } else {
                None
            }
        })
        .or_else(|| pick(&|k, _| k == EnemyKind::WeakPoint))
        .or_else(|| pick(&|_, id| id == tank));
    let mut input = InputState {
        fly: true,
        ..Default::default()
    };
    let Some((target, there)) = target else {
        return input;
    };
    input.target = Some(target);
    // Keep 9 m over the ground and 18 m from the target.
    let flat = DVec2::new(there.x - at.x, there.z - at.z);
    let d = flat.length();
    let dir = flat.normalize_or_zero();
    let want = if d > 20.0 {
        dir
    } else if d < 14.0 {
        -dir
    } else {
        DVec2::ZERO
    };
    input.move_x = want.x as f32 * 0.7;
    input.move_z = want.y as f32 * 0.7;
    input.move_y = ((GROUND_Y + 9.0 - at.y) * 0.3).clamp(-0.6, 0.6) as f32;
    let aim = (there + DVec3::Y - at).normalize_or_zero();
    input.aim = aim.as_vec3().to_array();
    if tick.is_multiple_of(4) && f.energy >= 10.0 {
        input.blast = true;
    }
    input
}

#[derive(Debug, PartialEq)]
struct Outcome {
    ticks: u64,
    knelt: usize,
    rose: usize,
    parts: usize,
    shells: usize,
    beaten: bool,
    left: usize,
}

fn fight(seed: u64, players: u16) -> Outcome {
    let mut sim = arena(seed);
    let heroes: Vec<NetId> = (1..=players)
        .map(|k| hero(&mut sim, k, DVec3::new(0.0, GROUND_Y, k as f64 * 3.0)))
        .collect();
    let tank = sim.add_colossus(DVec3::new(80.0, GROUND_Y, 0.0), DVec2::new(-1.0, 0.0));
    let parts = sim
        .enemies()
        .iter()
        .filter(|e| e.1 == EnemyKind::WeakPoint)
        .count();
    assert_eq!(parts, 5, "four knees and the cannon");
    let (mut knelt, mut rose, mut broken, mut shells) = (0, 0, 0, 0);
    let mut beaten = false;
    let limit = TICK_HZ as u64 * 60 * 12;
    let mut t = 0;
    while t < limit {
        for (k, h) in heroes.iter().enumerate() {
            let input = bot(&mut sim, *h, tank, t + k as u64);
            sim.set_input(PlayerId(k as u16 + 1), input);
        }
        sim.step();
        for e in sim.fx() {
            match e.kind {
                FxKind::Kneel => knelt += 1,
                FxKind::Rise => rose += 1,
                FxKind::PartBroken => broken += 1,
                FxKind::Shell if e.from == e.to => shells += 1,
                _ => {}
            }
        }
        if sim.fighter(tank).is_some_and(|f| f.status == Status::Down) {
            beaten = true;
        }
        t += 1;
        if beaten && sim.enemies().is_empty() {
            break;
        }
    }
    Outcome {
        ticks: t,
        knelt,
        rose,
        parts: broken,
        shells,
        beaten,
        left: sim.enemies().len(),
    }
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn the_colossus_goes_down_gets_up_and_goes_down_for_good() {
    let o = fight(7, 2);
    println!("{o:?}");
    assert!(o.beaten, "{o:?}");
    assert_eq!(o.knelt, 2, "down twice: {o:?}");
    assert_eq!(o.rose, 1, "up once in between: {o:?}");
    assert!(o.parts >= 4, "four knees at least: {o:?}");
    assert!(o.shells > 0, "it shelled the heroes: {o:?}");
    assert_eq!(o.left, 0, "the wreck is hauled off: {o:?}");
    // A set piece, not a pushover: minutes, not seconds.
    let minutes = o.ticks as f64 / TICK_HZ as f64 / 60.0;
    assert!((2.0..11.0).contains(&minutes), "{minutes:.1} min: {o:?}");
}

#[test]
fn the_colossus_fight_is_repeatable() {
    assert_eq!(fight(3, 1), fight(3, 1));
}

/// In a real town it keeps to the main roads: wider than it is, so its feet
/// and hull stay out of the buildings.
#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn the_colossus_walks_down_main_roads() {
    use hw_sim::streets::{Streets, TANK_STREET};
    let town = hw_sim::GeneratedTown::generate(Seed(1), hw_sim::DEFAULT_POPULATION);
    let streets = Streets::new(&town.world, TANK_STREET);
    let mut sim = Sim::for_town(Seed(1), &town);
    // Start it on a main road a little way off its middle.
    let start = streets.node(streets.len() as u32 / 2) + DVec2::new(1.5, 0.0);
    let tank = sim.add_colossus(DVec3::new(start.x, GROUND_Y, start.y), DVec2::X);
    let first = sim.position(tank).unwrap();
    let mut off_street: f64 = 0.0;
    for t in 0..TICK_HZ as u64 * 120 {
        sim.step();
        if t % 15 == 0 {
            let p = sim.position(tank).unwrap();
            let q = DVec2::new(p.x, p.z);
            let n = streets.nearest(q).unwrap();
            off_street = off_street.max(streets.node(n).distance(q));
        }
    }
    let walked = sim.position(tank).unwrap().distance(first);
    println!("walked {walked:.0} m, at most {off_street:.1} m off a street's middle");
    assert!(walked > 60.0, "it walks: {walked:.0} m");
    assert!(
        off_street < 5.0,
        "it keeps to the street: {off_street:.1} m"
    );
}
