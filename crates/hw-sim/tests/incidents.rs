//! Incidents come to the heroes: one starts near a hero after a while, the
//! wrist radio hears of it, beating its villains resolves it (karma for the
//! heroes there), and one nobody answers times out and its villains leave.

use glam::DVec3;
use hw_core::Seed;
use hw_net::{PlayerId, Status, ToClient};
use hw_sim::collision::Body;
use hw_sim::incidents::{self, Hurt, State, FARTHEST, NEAREST};
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

#[test]
fn heroes_get_people_out_of_a_fire_one_by_one() {
    use hw_net::InputState;
    use hw_sim::incidents::IncidentKind;
    let town = GeneratedTown::generate(Seed(3), 20_000.0);
    let mut sim = Sim::for_town(Seed(3), &town);
    let me = PlayerId(1);
    let hero = sim.spawn_hero(me, sim.spawn_point(1));
    sim.incidents.next_kind = Some(IncidentKind::Fire);
    let mut news = Vec::new();
    run(&mut sim, incidents::FIRST_SECONDS + 2.0, &mut news);
    let inc = sim.incidents.open[0].clone();
    assert_eq!(inc.kind, IncidentKind::Fire);
    assert_eq!(inc.left, 3);
    let karma = sim.sheet(me).unwrap().karma;
    // Standing by does nothing; holding the help key next to each gets
    // them out in turn.
    for k in (0..3u8).rev() {
        let next = *sim.incidents.open[0].victims().last().unwrap();
        sim.set_body(
            hero,
            Body {
                pos: next + DVec3::new(0.0, 0.2, 0.0) + (inc.at - next) * 0.5,
                vel: DVec3::ZERO,
                grounded: true,
            },
        );
        sim.set_input(me, InputState::default());
        run(&mut sim, 1.0, &mut news);
        assert_eq!(sim.incidents.open[0].left, k + 1, "freed without help");
        sim.set_input(
            me,
            InputState {
                revive: true,
                ..Default::default()
            },
        );
        run(
            &mut sim,
            IncidentKind::Fire.rescue_seconds() + 0.5,
            &mut news,
        );
        if k > 0 {
            assert_eq!(sim.incidents.open[0].left, k, "not got out");
        }
    }
    // The last one out is hurt, in the hero's arms; the rescue is not done
    // until they reach the ambulance.
    run(&mut sim, 1.5, &mut news);
    let open = sim.incidents.open[0].clone();
    assert_eq!(open.left, 0);
    assert!(
        matches!(open.hurt, Some(Hurt::Carried(_))),
        "nobody carries the hurt one: {:?}",
        open.hurt
    );
    assert!(
        !news.iter().any(|n| n.0 == inc.id && n.3 == 1),
        "done too soon"
    );
    let (medic, _) = open.medic.expect("an ambulance");
    let away = glam::DVec2::new(medic.x - inc.at.x, medic.z - inc.at.z).length();
    assert!(
        (20.0..120.0).contains(&away),
        "the ambulance is {away:.0} m off"
    );
    // Dropped (say the carrier was knocked down): lying there until a hero
    // lifts them again.
    let lying = inc.at + DVec3::new(2.0, 0.0, 0.0);
    sim.incidents.open[0].hurt = Some(Hurt::Lying(lying));
    sim.set_body(
        hero,
        Body {
            pos: lying + DVec3::new(1.0, 0.2, 0.0),
            vel: DVec3::ZERO,
            grounded: true,
        },
    );
    run(&mut sim, incidents::LIFT_SECONDS + 0.5, &mut news);
    assert!(matches!(sim.incidents.open[0].hurt, Some(Hurt::Carried(_))));
    // Handed to the medic: done.
    sim.set_input(me, InputState::default());
    sim.set_body(
        hero,
        Body {
            pos: medic + DVec3::new(1.5, 0.2, 0.0),
            vel: DVec3::ZERO,
            grounded: true,
        },
    );
    run(&mut sim, 1.5, &mut news);
    assert!(
        news.iter().any(|n| n.0 == inc.id && n.3 == 1),
        "the fire incident was not resolved"
    );
    assert!(sim.sheet(me).unwrap().karma > karma + 30, "no karma for it");
}

#[test]
fn a_mugging_has_a_victim_and_the_crooks_turn_on_a_hero_who_steps_in() {
    use hw_net::EntityKind;
    let town = GeneratedTown::generate(Seed(3), 20_000.0);
    let mut sim = Sim::for_town(Seed(3), &town);
    let me = PlayerId(1);
    let hero = sim.spawn_hero(me, sim.spawn_point(1));
    sim.incidents.first_after = 0.0;
    sim.incidents.next_kind = Some(incidents::IncidentKind::Mugging);
    let mut news = Vec::new();
    run(&mut sim, 1.5, &mut news);
    let inc = sim.incidents.open[0].clone();
    assert_eq!(inc.kind, incidents::IncidentKind::Mugging);
    let victim = |sim: &mut Sim| {
        sim.snapshot()
            .into_iter()
            .find(|e| e.kind == EntityKind::Bystander)
    };
    let v = victim(&mut sim).expect("someone is being mugged");
    let at = DVec3::from_array(v.pos);
    assert!(at.distance(inc.at) < 5.0);
    // A hero watching from across the street is not their business.
    let face = DVec3::new(inc.facing.x, 0.0, inc.facing.y);
    let stand = |sim: &mut Sim, d: f64| {
        sim.set_body(
            hero,
            Body {
                pos: inc.at + face * d + DVec3::Y * 0.1,
                vel: DVec3::ZERO,
                grounded: true,
            },
        );
    };
    for _ in 0..TICK_HZ * 3 {
        stand(&mut sim, 14.0);
        sim.step();
    }
    assert!(
        sim.enemies().iter().all(|e| e.3.target.is_none()),
        "the muggers leave a passer-by alone"
    );
    // Stepping in starts the fight.
    for _ in 0..TICK_HZ {
        stand(&mut sim, 5.5);
        sim.step();
    }
    assert!(sim.enemies().iter().any(|e| e.3.target == Some(hero)));
    // Beaten (a cheat): the victim walks off and is gone a while later.
    for (id, _, _, mut f) in sim.enemies() {
        f.health = 0.0;
        f.status = Status::Down;
        sim.set_fighter(id, f);
    }
    // The police drones take them away; then it is over.
    for _ in 0..20 {
        if sim.incidents.open.iter().all(|i| i.id != inc.id) {
            break;
        }
        run(&mut sim, 1.0, &mut news);
    }
    assert!(sim.incidents.open.iter().all(|i| i.id != inc.id));
    run(&mut sim, 4.0, &mut news);
    let leaving = victim(&mut sim).expect("still walking off");
    assert!(
        DVec3::from_array(leaving.pos).distance(at) > 3.0,
        "walking off"
    );
    run(&mut sim, incidents::LEAVE_SECONDS + 1.0, &mut news);
    assert!(victim(&mut sim).is_none());
}
