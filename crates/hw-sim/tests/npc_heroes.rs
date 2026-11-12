//! The town's own heroes at Hero Hall: they stand on its plaza, give a
//! newcomer tips one after another, fly out to a fight nobody answered,
//! beat it without paying anyone karma, and fly home again.

use glam::DVec3;
use hw_core::Seed;
use hw_net::{EntityKind, FxKind, PlayerId};
use hw_sim::collision::Body;
use hw_sim::incidents::{self, IncidentKind};
use hw_sim::npcheroes;
use hw_sim::{GeneratedTown, Sim, TICK_HZ};

fn town() -> (Sim, hw_core::NetId) {
    let town = GeneratedTown::generate(Seed(3), 20_000.0);
    let mut sim = Sim::for_town(Seed(3), &town);
    let hero = sim.spawn_hero(PlayerId(1), sim.spawn_point(1));
    (sim, hero)
}

fn npcs(sim: &mut Sim) -> Vec<hw_net::EntityState> {
    sim.snapshot()
        .into_iter()
        .filter(|e| e.kind == EntityKind::NpcHero)
        .collect()
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
fn the_town_heroes_greet_a_newcomer_with_tips() {
    let (mut sim, hero) = town();
    let all = npcs(&mut sim);
    assert_eq!(all.len(), npcheroes::NAMES.len());
    // Each knows which of them it is.
    let mut index: Vec<u8> = all.iter().filter_map(|e| e.mount.map(|m| m.1)).collect();
    index.sort();
    assert_eq!(index, vec![0, 1, 2, 3, 4]);
    let middle = DVec3::from_array(all[2].pos);
    let mut tips = Vec::new();
    for _ in 0..TICK_HZ * 14 {
        stand(&mut sim, hero, middle + DVec3::new(0.0, 0.0, 2.0));
        sim.step();
        tips.extend(
            sim.fx()
                .iter()
                .filter(|f| f.kind == FxKind::Tip && f.to == hero)
                .map(|f| (f.from, f.amount)),
        );
    }
    // One at a time, each a new tip, from different heroes.
    assert!(tips.len() >= 2, "{tips:?}");
    assert_eq!(tips[0].1, 0);
    assert_eq!(tips[1].1, 1);
    assert_ne!(tips[0].0, tips[1].0);
    assert!(tips.len() <= 3, "they talk over each other: {tips:?}");
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn a_town_hero_answers_a_fight_nobody_came_to() {
    let (mut sim, hero) = town();
    sim.incidents.first_after = 0.0;
    sim.incidents.next_kind = Some(IncidentKind::Mugging);
    for _ in 0..TICK_HZ * 2 {
        sim.step();
    }
    let inc = sim
        .incidents
        .open
        .iter()
        .find(|i| i.kind == IncidentKind::Mugging)
        .cloned()
        .expect("a mugging");
    // Our hero keeps far away (and is never paid for it).
    let start = sim.position(hero).unwrap();
    assert!(start.distance(inc.at) > incidents::REWARD);
    let karma = sim.sheet(PlayerId(1)).unwrap().karma;
    let mut answered = None;
    let mut resolved = false;
    for t in 0..TICK_HZ as u64 * 230 {
        stand(&mut sim, hero, start);
        sim.step();
        if answered.is_none() {
            answered = npcs(&mut sim)
                .iter()
                .find(|e| DVec3::from_array(e.pos).distance(inc.at) < 20.0)
                .map(|e| (e.id, t));
        }
        if sim.incidents.open.iter().all(|i| i.id != inc.id) {
            resolved = true;
            break;
        }
    }
    let (helper, when) = answered.expect("no town hero came");
    assert!(
        when as f64 / TICK_HZ as f64 >= npcheroes::ANSWER_AFTER - 2.0,
        "came too soon"
    );
    assert!(resolved, "the mugging was not beaten");
    assert!(
        sim.sheet(PlayerId(1)).unwrap().karma <= karma,
        "karma for nothing"
    );
    // Home again, on its feet. (New trouble keeps coming, a call a minute,
    // and a hero home and fit flies out again to the next one: being home
    // once is what counts, not still being there at the end.)
    let mut home = None;
    for _ in 0..TICK_HZ * 60 {
        stand(&mut sim, hero, start);
        sim.step();
        let h = npcs(&mut sim)
            .into_iter()
            .find(|e| e.id == helper)
            .expect("still here");
        assert!(h.combat.unwrap().health > 0);
        if h.grounded {
            home = Some(h.pos);
            break;
        }
    }
    assert!(home.is_some(), "not home within a minute");
}
