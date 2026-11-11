//! New heroes start where small crime is: a short walk from the Chrome
//! Jackets' corners, with muggings and hold-ups coming often and close. An
//! incident a hero is running to waits for them, and the town's own heroes
//! leave it to them.

use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_net::{PlayerId, Status};
use hw_sim::balance::{bot_input, hero_sheet};
use hw_sim::collision::Body;
use hw_sim::incidents::{self, State, FARTHEST_AFOOT, HOT_GAP, NEAREST_AFOOT};
use hw_sim::progress::{self, level_of};
use hw_sim::quarter::ROOKIE_LEVEL;
use hw_sim::{GeneratedTown, Sim, DT, RUN_SPEED, TICK_HZ};

fn town() -> (GeneratedTown, Sim) {
    let town = GeneratedTown::generate(Seed(3), 20_000.0);
    let sim = Sim::for_town(Seed(3), &town);
    (town, sim)
}

fn flat(p: DVec3) -> DVec2 {
    DVec2::new(p.x, p.z)
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn a_new_hero_starts_near_the_jackets_and_a_seasoned_one_in_the_centre() {
    let (_, mut sim) = town();
    let line = sim
        .rookie_line()
        .expect("a town with a gang has a rookie street");
    let rookie = sim.spawn_at_start(PlayerId(1));
    sim.set_sheet(PlayerId(1), progress::starter());
    let at = sim.position(rookie).unwrap();
    assert!(
        flat(at).distance(line.0) < 10.0,
        "not moved to the rookies' street"
    );
    assert!(
        sim.incidents.is_hot(at),
        "the start is not in rough streets"
    );
    // Knocked out, the medical drone brings them back to that street.
    let mut f = sim.fighter(rookie).unwrap();
    f.health = 0.0;
    f.status = Status::Down;
    f.until = sim.tick;
    sim.set_fighter(rookie, f);
    sim.set_input(
        PlayerId(1),
        hw_net::InputState {
            hospital: true,
            ..Default::default()
        },
    );
    let away = DVec3::new(300.0, 0.0, 0.0);
    sim.set_body(
        rookie,
        Body {
            pos: at + away,
            ..Default::default()
        },
    );
    for _ in 0..TICK_HZ * 60 {
        sim.step();
        if sim.fighter(rookie).unwrap().status == Status::Standing {
            break;
        }
    }
    let back = flat(sim.position(rookie).unwrap());
    assert!(
        back.distance(line.0) < 10.0,
        "back {:.0} m from the rookies' street",
        back.distance(line.0)
    );
    // A hero who can fly starts in the centre as before.
    let vet = sim.spawn_at_start(PlayerId(2));
    sim.remove_hero(PlayerId(1));
    sim.set_sheet(PlayerId(2), hero_sheet(ROOKIE_LEVEL, &[]));
    let at = sim.position(vet).unwrap();
    assert!(
        at.distance(sim.spawn_point(2)) < 1.0,
        "a level-{ROOKIE_LEVEL} hero moved"
    );
    // Nobody moves a hero already somewhere else (a view, the debug menu).
    let moved = sim.spawn_at_start(PlayerId(3));
    sim.remove_hero(PlayerId(2));
    let there = sim.spawn_point(3) + DVec3::new(40.0, 0.0, 0.0);
    sim.set_body(
        moved,
        Body {
            pos: there,
            ..Default::default()
        },
    );
    sim.set_sheet(PlayerId(3), progress::starter());
    assert!(sim.position(moved).unwrap().distance(there) < 1.0);
}

#[test]
#[ignore = "on demand (balance): scripts/check.sh --on-demand"]
fn crime_comes_often_and_close_to_a_new_hero() {
    let (_, mut sim) = town();
    let hero = sim.spawn_at_start(PlayerId(1));
    sim.set_sheet(PlayerId(1), progress::starter());
    let me = sim.position(hero).unwrap();
    let mut seen: Vec<(u32, f64)> = Vec::new();
    // Six minutes, standing still: every incident starts a short run away.
    let minutes = 6.0;
    for _ in 0..(minutes * 60.0 * TICK_HZ as f64) as u64 {
        sim.step();
        for i in &sim.incidents.open {
            if seen.iter().all(|s| s.0 != i.id) {
                seen.push((i.id, i.at.distance(me)));
            }
        }
    }
    println!("incidents (id, m away): {seen:.0?}");
    for (_, d) in &seen {
        assert!(
            (NEAREST_AFOOT - 1.0..FARTHEST_AFOOT + 1.0).contains(d),
            "{d:.0} m away"
        );
    }
    // The first after 20 s, then one each HOT_GAP while fewer than the
    // most are open (a few minutes each, so not every gap brings one).
    assert!(seen.len() >= 4, "only {} in {minutes} min", seen.len());
    const { assert!(HOT_GAP < incidents::GAP_SECONDS) };
}

/// Moves `hero` one tick from `from` towards `goal` at `speed` (m/s), on
/// the ground, through whatever is in the way (a test's stand-in for
/// running the streets). Returns where it put them.
fn run_towards(sim: &mut Sim, hero: NetId, from: DVec2, goal: DVec3, speed: f64) -> DVec2 {
    let d = flat(goal) - from;
    let p = from + d.normalize_or_zero() * (speed * DT).min(d.length());
    let y = sim.solids().ground_at(p).max(hw_sim::GROUND_Y);
    sim.set_body(
        hero,
        Body {
            pos: DVec3::new(p.x, y + 0.1, p.y),
            ..Default::default()
        },
    );
    p
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn an_incident_waits_for_a_hero_on_the_way() {
    let (_, mut sim) = town();
    let me = PlayerId(1);
    let hero = sim.spawn_at_start(me);
    sim.set_sheet(me, progress::starter());
    sim.incidents.forced = Some(incidents::IncidentKind::Robbery);
    for _ in 0..TICK_HZ * 2 {
        sim.step();
    }
    let inc = sim.incidents.open.last().expect("a robbery").clone();
    // The hero is a long way off, and the robbery about to end: they run.
    let start = inc.at + DVec3::new(700.0, 0.0, 0.0);
    sim.set_body(
        hero,
        Body {
            pos: start,
            ..Default::default()
        },
    );
    if let Some(i) = sim.incidents.open.iter_mut().find(|i| i.id == inc.id) {
        i.expires = sim.tick + TICK_HZ as u64 * 5;
    }
    let mut steps = 0;
    let mut p = flat(start);
    while p.distance(flat(inc.at)) > incidents::ANSWER - 20.0 {
        p = run_towards(&mut sim, hero, p, inc.at, RUN_SPEED * 0.75);
        sim.step();
        steps += 1;
        let open = sim.incidents.open.iter().find(|i| i.id == inc.id);
        let open = open.unwrap_or_else(|| {
            panic!(
                "gone after {:.0} s of running",
                steps as f64 / TICK_HZ as f64
            )
        });
        assert_eq!(open.state, State::Open);
        assert!(
            sim.fights().spots[open.spot.unwrap()].members > 0,
            "its villains were taken by someone else"
        );
    }
    // One nobody comes for still ends in time.
    sim.incidents.forced = Some(incidents::IncidentKind::Mugging);
    for _ in 0..TICK_HZ * 2 {
        sim.step();
    }
    let lone = sim.incidents.open.last().unwrap().id;
    // The hero walks away from it.
    sim.set_body(
        hero,
        Body {
            pos: start + DVec3::new(1500.0, 0.0, 0.0),
            ..Default::default()
        },
    );
    for _ in 0..(incidents::TIMEOUT_SECONDS as u64 + 5) * TICK_HZ as u64 {
        sim.step();
    }
    assert!(sim.incidents.open.iter().all(|i| i.id != lone));
}

/// A new hero answering the crime near the rookies' street climbs at about
/// the pace of the climb tests' careful player (tests/powers.rs: level 2
/// in 6-16 min, level 3 in 16-35), so the ten hours to the cap still hold.
/// Running is modelled (straight at three quarters of running speed);
/// fighting is the balance bot's. The bot leaves fires and crashes to
/// others, which a player would not, so level 3 may come a little later.
/// Forty minutes of a town: like the ten-hour climb it stays out of
/// `scripts/check.sh --daily`; run it with `--ignored` (release: ~1 min).
#[test]
#[ignore = "forty minutes of play: run it with --ignored in release"]
fn a_new_hero_near_the_jackets_climbs_at_the_climb_tests_pace() {
    let (_, mut sim) = town();
    let me = PlayerId(1);
    let hero = sim.spawn_at_start(me);
    sim.set_sheet(me, progress::starter());
    let mut reached: Vec<(u8, f64)> = Vec::new();
    let mut answered = 0;
    let mut last_goal = None;
    // Where a run through the streets has got to.
    let mut travel: Option<DVec2> = None;
    let mut last_blow = 0;
    let limit = TICK_HZ as u64 * 60 * 45;
    for t in 0..limit {
        let at = sim.position(hero).unwrap();
        // The nearest open fight.
        let goal = sim
            .incidents
            .open
            .iter()
            .filter(|i| i.spot.is_some())
            .min_by(|a, b| a.at.distance(at).total_cmp(&b.at.distance(at)))
            .map(|i| (i.id, i.at));
        // Fighting at the incident (a careful player passes the gangs'
        // corners by).
        let busy = goal.is_some_and(|g| g.1.distance(at) < 25.0)
            && sim.enemies().iter().any(|e| {
                e.2.distance(at) < 25.0 && matches!(e.3.status, Status::Standing | Status::Knocked)
            });
        if goal.map(|g| g.0) != last_goal {
            last_goal = goal.map(|g| g.0);
            answered += goal.is_some() as u32;
        }
        let down = sim.fighter(hero).unwrap().status == Status::Down;
        match goal {
            Some((_, g)) if !busy && !down && g.distance(at) > 15.0 => {
                // A run through the streets: the straight line at three
                // quarters of running speed (streets wind). Arrived, they
                // step in where it happens.
                let from = travel.unwrap_or(flat(at));
                let p = run_towards(&mut sim, hero, from, g, RUN_SPEED * 0.75);
                travel = Some(p);
                if p.distance(flat(g)) <= 15.0 {
                    travel = None;
                    sim.set_body(
                        hero,
                        Body {
                            pos: g + DVec3::Y * 0.3,
                            ..Default::default()
                        },
                    );
                }
                sim.set_input(me, Default::default());
            }
            _ => {
                travel = None;
                // The bot keeps its distance and shoots; with a wall in the
                // way it would wait for ever, where a player steps round.
                if let Some((_, g)) = goal.filter(|_| busy && t > last_blow + TICK_HZ as u64 * 10) {
                    sim.set_body(
                        hero,
                        Body {
                            pos: g + DVec3::Y * 0.3,
                            ..Default::default()
                        },
                    );
                    last_blow = t;
                }
                let input = bot_input(&mut sim, hero, t, goal.map(|g| g.1));
                sim.set_input(me, input);
            }
        }
        sim.step();
        // A blow landed (a power that fizzled is no blow: out of sight,
        // the bot steps round as a player would).
        if sim
            .fx()
            .iter()
            .any(|e| e.from == hero && e.kind != hw_net::FxKind::Fizzle)
        {
            last_blow = t;
        }
        if t.is_multiple_of(TICK_HZ as u64 * 10) {
            let level = level_of(sim.sheet(me).unwrap().lifetime);
            let minutes = t as f64 / TICK_HZ as f64 / 60.0;
            if reached.last().map_or(1, |r| r.0) < level {
                reached.push((level, minutes));
            }
            if level >= 3 {
                break;
            }
        }
    }
    println!("levels (level, minute): {reached:.1?}; {answered} incidents gone to");
    let when = |l: u8| reached.iter().find(|r| r.0 == l).map(|r| r.1);
    let two = when(2).expect("level 2 within 45 min");
    assert!((6.0..16.0).contains(&two), "level 2 after {two:.0} min");
    let three = when(3).expect("level 3 within 45 min");
    assert!(
        (16.0..45.0).contains(&three),
        "level 3 after {three:.0} min"
    );
}
