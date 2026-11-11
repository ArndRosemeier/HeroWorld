//! Street crime: a hero who walks up to a crime going on steps in. Its
//! crooks become real villains where they were drawn, wearing the same
//! looks, and turn on the hero; the victim waits it out and thanks them;
//! beating the crooks stops it (karma), and every client is told.

use glam::DVec3;
use hw_core::Seed;
use hw_net::{EntityKind, PlayerId, Status, ToClient};
use hw_sim::collision::Body;
use hw_sim::streetlife::crime::CrimeRole;
use hw_sim::{GeneratedTown, Sim, DT, TICK_HZ};

fn crime_news(sim: &mut Sim, out: &mut Vec<(u64, i64, u8)>) {
    for m in sim.take_news() {
        if let ToClient::Crime { place, slot, state } = m {
            out.push((place, slot, state));
        }
    }
}

#[test]
fn a_hero_steps_into_a_bag_snatch_and_stops_it() {
    // Every pavement place has a snatch every hour (read once, before the
    // first crime is planned).
    std::env::set_var("HW_CRIME", "snatch");
    let town = GeneratedTown::generate(Seed(3), 20_000.0);
    let mut sim = Sim::for_town(Seed(3), &town);
    // No incidents in the way.
    sim.incidents.first_after = 1e9;
    let me = PlayerId(1);
    let hero = sim.spawn_hero(me, sim.spawn_point(1));
    let mut news = Vec::new();
    for _ in 0..3 * TICK_HZ {
        sim.step();
        crime_news(&mut sim, &mut news);
    }
    let life = sim.crimes.life().expect("street life").clone();
    let now = sim.tick as f64 * DT;
    // A snatch near the hero that starts a little later.
    let at = sim.position(hero).unwrap();
    let crime = life
        .crimes_near(glam::DVec2::new(at.x, at.z), 100.0, now, 200.0)
        .into_iter()
        .filter(|c| c.on > now + 1.0 && c.on < now + 200.0)
        .min_by(|a, b| a.on.total_cmp(&b.on))
        .expect("a snatch coming up near the hero");
    // Nobody steps in from afar: wait (out of the way) until it is on.
    let up = sim.position(hero).unwrap() + DVec3::new(0.0, 60.0, 0.0);
    sim.set_body(
        hero,
        Body {
            pos: up,
            vel: DVec3::ZERO,
            grounded: false,
        },
    );
    while (sim.tick as f64 * DT) < crime.on + 2.0 {
        sim.step();
        crime_news(&mut sim, &mut news);
    }
    assert_eq!(sim.crimes.stepped_in(), 0, "stepped in from afar");
    // Walk up to the thief.
    let t = sim.tick as f64 * DT;
    let thief = crime.crooks(&life, t)[0];
    let CrimeRole::Crook(_, look, _) = thief.role else {
        unreachable!()
    };
    sim.set_body(
        hero,
        Body {
            pos: thief.pos + DVec3::new(2.0, 0.3, 0.0),
            vel: DVec3::ZERO,
            grounded: false,
        },
    );
    for _ in 0..TICK_HZ / 2 {
        sim.step();
        crime_news(&mut sim, &mut news);
    }
    assert_eq!(sim.crimes.stepped_in(), 1, "did not step in");
    assert!(
        news.iter().any(|n| (n.0, n.1) == crime.key() && n.2 == 1),
        "clients not told: {news:?}"
    );
    // The thief is a real greaser now, where it was drawn, in its looks,
    // going for the hero.
    let state = sim.snapshot();
    let greaser = state
        .iter()
        .find(|e| {
            e.kind == EntityKind::Greaser && DVec3::from_array(e.pos).distance(thief.pos) < 3.0
        })
        .expect("a greaser where the thief was");
    assert_eq!(greaser.mount.map(|m| m.1), Some(look), "not in its looks");
    let target = sim
        .enemies()
        .into_iter()
        .find(|e| e.0 == greaser.id)
        .and_then(|e| e.3.target);
    assert_eq!(target, Some(hero), "not going for the hero");
    assert!(
        state.iter().any(|e| e.kind == EntityKind::Bystander),
        "no victim"
    );
    // Beat it (a cheat): stopped, karma.
    let karma = sim.sheet(me).unwrap().karma;
    for (id, _, at, mut f) in sim.enemies() {
        if at.distance(thief.pos) < 30.0 {
            f.health = 0.0;
            f.status = Status::Down;
            f.until = sim.tick;
            sim.set_fighter(id, f);
        }
    }
    for _ in 0..15 * TICK_HZ {
        sim.step();
        crime_news(&mut sim, &mut news);
    }
    assert!(
        news.iter().any(|n| (n.0, n.1) == crime.key() && n.2 == 2),
        "not stopped: {news:?}"
    );
    assert_eq!(sim.crimes.stepped_in(), 0);
    assert!(sim.sheet(me).unwrap().karma > karma, "no karma");
    // A joiner hears of it too, and nobody draws it again.
    assert!(sim.crimes.snapshot().iter().any(
        |m| matches!(m, ToClient::Crime { place, state: 2, .. } if *place == crime.place.key)
    ));
}
