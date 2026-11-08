//! Incidents come to the heroes: one starts near a hero after a while, the
//! wrist radio hears of it, beating its villains resolves it (karma for the
//! heroes there), and one nobody answers times out and its villains leave.

use glam::DVec3;
use hw_core::Seed;
use hw_net::{PlayerId, Status, ToClient};
use hw_sim::collision::Body;
use hw_sim::incidents::{self, State, FARTHEST, NEAREST};
use hw_sim::{GeneratedTown, Sim, TICK_HZ};

fn news_of(sim: &mut Sim) -> Vec<(u32, u8, [f32; 3], u8)> {
    sim.take_news()
        .into_iter()
        .filter_map(|m| match m {
            ToClient::Incident {
                id,
                kind,
                at,
                state,
                ..
            } => Some((id, kind, at, state)),
            _ => None,
        })
        .collect()
}

fn run(sim: &mut Sim, seconds: f64, news: &mut Vec<(u32, u8, [f32; 3], u8)>) {
    for _ in 0..(seconds * TICK_HZ as f64) as u64 {
        sim.step();
        news.extend(news_of(sim));
    }
}

#[test]
fn an_incident_starts_near_a_hero_and_is_resolved_by_beating_its_villains() {
    let town = GeneratedTown::generate(Seed(3), 20_000.0);
    let mut sim = Sim::for_town(Seed(3), &town);
    let me = PlayerId(1);
    let hero = sim.spawn_hero(me, sim.spawn_point(1));
    let mut news = Vec::new();
    run(&mut sim, incidents::FIRST_SECONDS + 2.0, &mut news);
    assert_eq!(sim.incidents.open.len(), 1, "no incident yet");
    let inc = sim.incidents.open[0].clone();
    assert!(
        news.iter().any(|n| n.0 == inc.id && n.3 == 0),
        "not on the radio"
    );
    let d = sim.position(hero).unwrap().distance(inc.at);
    assert!(d > NEAREST - 1.0 && d < FARTHEST + 1.0, "{d:.0} m away");
    // Run there and beat them all at once (a cheat); the police take them.
    sim.set_body(
        hero,
        Body {
            pos: inc.at + DVec3::new(0.0, 1.0, 20.0),
            vel: DVec3::ZERO,
            grounded: false,
        },
    );
    sim.step();
    let karma = sim.sheet(me).unwrap().karma;
    for (id, _, at, mut f) in sim.enemies() {
        if at.distance(inc.at) < 30.0 {
            f.health = 0.0;
            f.status = Status::Down;
            f.until = sim.tick;
            sim.set_fighter(id, f);
        }
    }
    run(&mut sim, 15.0, &mut news);
    assert!(
        news.iter().any(|n| n.0 == inc.id && n.3 == 1),
        "not resolved: {news:?}"
    );
    assert!(sim.incidents.open.iter().all(|i| i.id != inc.id));
    assert!(sim.sheet(me).unwrap().karma > karma + 20, "no karma for it");
}

#[test]
fn an_unanswered_incident_times_out_and_more_follow() {
    let town = GeneratedTown::generate(Seed(3), 20_000.0);
    let mut sim = Sim::for_town(Seed(3), &town);
    sim.spawn_hero(PlayerId(1), sim.spawn_point(1));
    let mut news = Vec::new();
    run(
        &mut sim,
        incidents::FIRST_SECONDS + incidents::TIMEOUT_SECONDS + 5.0,
        &mut news,
    );
    let first = news[0].0;
    assert!(
        news.iter().any(|n| n.0 == first && n.3 == 2),
        "never timed out"
    );
    // Others started meanwhile, a minute or more apart; never more than
    // the radio wants at once.
    let started: Vec<u32> = news.iter().filter(|n| n.3 == 0).map(|n| n.0).collect();
    assert!(started.len() >= 3, "{started:?}");
    assert!(sim.incidents.open.len() <= incidents::wanted(1));
    assert!(sim.incidents.open.iter().all(|i| i.state == State::Open));
    // Its villains left.
    let at = DVec3::from_array(news[0].2.map(|v| v as f64));
    assert!(sim.enemies().iter().all(|e| e.2.distance(at) > 30.0));
}
