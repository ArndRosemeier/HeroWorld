//! The Doctor's story, end to end in a real town: a bot hero reports to
//! Hero Hall for each briefing, goes where the chapter plays, beats the
//! group there (the Doctor himself at the end, in his lab), searches the
//! find, and the hero file carries the story on. In co-op the team follows
//! the host's story.

use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_net::{InputState, PlayerId, PowerId, ToClient};
use hw_sim::balance::{bot_input, hero_sheet};
use hw_sim::collision::Body;
use hw_sim::combat::EnemyKind;
use hw_sim::story::{self, CHAPTERS, FIGHT, FINISHED, GO, LOCKED, REPORT, SEARCH};
use hw_sim::{GeneratedTown, Sim, TICK_HZ};

fn town() -> Sim {
    let town = GeneratedTown::generate(Seed(3), 20_000.0);
    Sim::for_town(Seed(3), &town)
}

/// Every power of the Lab Accident and travel trees: the bot is here to
/// play the chain, not to test the balance (`tests/powers.rs` does that).
fn all_powers() -> Vec<PowerId> {
    use hw_sim::powers::{tree, Tree};
    tree(Tree::Science)
        .chain(tree(Tree::Common))
        .map(|d| d.id)
        .collect()
}

fn put(sim: &mut Sim, id: NetId, at: DVec3) {
    sim.set_body(
        id,
        Body {
            pos: at + DVec3::Y * 0.3,
            ..Default::default()
        },
    );
}

fn steps(sim: &mut Sim, seconds: f64) {
    for _ in 0..(seconds * TICK_HZ as f64) as usize {
        sim.step();
    }
}

/// Fights whatever stands at the chapter's place until the stage moves
/// on. Kept on its feet (healed every second): the chain is the test.
fn fight(sim: &mut Sim, hero: NetId, k: usize, goal: DVec3) {
    let mut seconds = 0.0;
    let mut last_at = DVec3::ZERO;
    let mut still = 0;
    while sim.story.stage == FIGHT {
        for _ in 0..TICK_HZ {
            let tick = sim.tick;
            let mut input = bot_input(sim, hero, tick, Some(goal));
            input.fly = sim.enemies().iter().any(|e| e.1 == EnemyKind::Colossus);
            sim.set_input(PlayerId(1), input);
            sim.step();
        }
        // The quarter's own corners are not the chapter's: after the
        // chapter's own group, wherever it went.
        let me = sim.position(hero).unwrap();
        let next = sim.story.spot().and_then(|i| {
            sim.world
                .query::<(
                    &hw_sim::combat::Enemy,
                    &hw_sim::Motion,
                    &hw_sim::combat::Fighter,
                )>()
                .iter(&sim.world)
                .filter(|(e, _, f)| e.spot == i && f.standing())
                .map(|(_, m, _)| m.0.pos)
                .min_by(|a, b| a.distance(me).total_cmp(&b.distance(me)))
        });
        if let Some(p) = next.filter(|p| p.distance(me) > 25.0) {
            put(sim, hero, p + DVec3::new(6.0, 1.0, 0.0));
            last_at = p;
        }
        // A bot that has not moved for 5 s is stuck (on furniture, in a
        // corner: it cannot find a way round): put it where a villain
        // stands, which is clear floor. Not against the colossus, where it
        // hovers on purpose.
        if me.distance(last_at) > 0.5 {
            last_at = me;
            still = 0;
        } else {
            still += 1;
        }
        let colossus = sim.enemies().iter().any(|e| e.1 == EnemyKind::Colossus);
        if let Some(p) = next.filter(|_| still >= 5 && !colossus) {
            put(sim, hero, p);
            still = 0;
        }
        let mut f = sim.fighter(hero).unwrap();
        f.health = f.max_health;
        f.energy = f.max_energy;
        f.status = hw_net::Status::Standing;
        sim.set_fighter(hero, f);
        seconds += 1.0;
        assert!(
            seconds < 900.0,
            "chapter {} ({}): the fight never ended",
            k + 1,
            CHAPTERS[k].title
        );
    }
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn a_bot_plays_the_doctors_story_end_to_end() {
    let mut sim = town();
    assert!(sim.story.available(), "seed 3's town has the story");
    let hero = sim.spawn_hero(PlayerId(1), DVec3::new(0.0, 5.0, 0.0));
    sim.set_sheet(PlayerId(1), hero_sheet(1, &all_powers()));
    let hall = sim.story.hall().unwrap();
    steps(&mut sim, 1.5);
    assert_eq!(sim.story.stage, REPORT, "a new hero is sent to Hero Hall");
    let mut heard = Vec::new();
    for (k, c) in CHAPTERS.iter().enumerate() {
        // Locked until the team reaches the chapter's level.
        if c.level > 1 {
            assert_eq!(sim.story.stage, LOCKED, "chapter {} opened early", k + 1);
            let mut s = sim.sheet(PlayerId(1)).unwrap();
            s.lifetime = hw_sim::progress::karma_for_level(c.level);
            sim.set_sheet(PlayerId(1), s);
            steps(&mut sim, 1.5);
        }
        assert_eq!(
            sim.story.stage,
            REPORT,
            "chapter {}: not sent to the hall",
            k + 1
        );
        // The briefing at the hall.
        put(&mut sim, hero, hall);
        steps(&mut sim, 1.5);
        assert_eq!(sim.story.stage, GO, "chapter {}: no briefing", k + 1);
        assert!(sim.sheet(PlayerId(1)).unwrap().briefed);
        // Off to where it plays: the group takes its place.
        let (at, facing) = sim.story.place(k).unwrap();
        let start = at + DVec3::new(facing.x, 0.0, facing.y) * 12.0;
        put(&mut sim, hero, DVec3::new(start.x, at.y, start.z));
        steps(&mut sim, 1.5);
        assert_eq!(sim.story.stage, FIGHT, "chapter {}: nobody there", k + 1);
        let lead = c.group.lead.or(c.group.only);
        if let Some(lead) = lead {
            assert!(
                sim.enemies().iter().any(|e| e.1 == lead),
                "chapter {}: no {lead:?}",
                k + 1
            );
        }
        fight(&mut sim, hero, k, at);
        if c.search > 0.0 {
            assert_eq!(
                sim.story.stage,
                SEARCH,
                "chapter {}: nothing to search",
                k + 1
            );
            put(
                &mut sim,
                hero,
                at + DVec3::new(facing.x, 0.0, facing.y) * 1.5,
            );
            sim.set_input(
                PlayerId(1),
                InputState {
                    revive: true,
                    ..Default::default()
                },
            );
            steps(&mut sim, c.search + 1.5);
            sim.set_input(PlayerId(1), InputState::default());
        }
        steps(&mut sim, 1.5);
        let s = sim.sheet(PlayerId(1)).unwrap();
        assert_eq!(s.story as usize, k + 1, "chapter {} not done", k + 1);
        assert!(!s.briefed, "briefed on a chapter not yet given");
        for m in sim.take_news() {
            if let ToClient::Story { done, stage, .. } = m {
                heard.push((done, stage));
            }
        }
    }
    assert_eq!(sim.story.stage, FINISHED);
    // The radio heard every step of every chapter, in order.
    for k in 0..CHAPTERS.len() as u8 {
        for stage in [REPORT, GO, FIGHT] {
            assert!(
                heard.contains(&(k, stage)),
                "never told chapter {k} stage {stage}"
            );
        }
    }
    assert!(heard.contains(&(CHAPTERS.len() as u8, FINISHED)));
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn the_team_follows_the_hosts_story() {
    let mut sim = town();
    let host = sim.spawn_hero(PlayerId(1), DVec3::new(0.0, 5.0, 0.0));
    let guest = sim.spawn_hero(PlayerId(2), DVec3::new(3.0, 5.0, 0.0));
    let mut a = hero_sheet(4, &[]);
    a.story = 1;
    sim.set_sheet(PlayerId(1), a);
    let mut b = hero_sheet(9, &[]);
    b.story = 5;
    b.briefed = true;
    sim.set_sheet(PlayerId(2), b);
    steps(&mut sim, 1.5);
    assert_eq!(sim.story.done, 1, "the team is on the host's chapter");
    assert_eq!(sim.story.stage, REPORT);
    // The guest is briefed for the team.
    let hall = sim.story.hall().unwrap();
    put(&mut sim, guest, hall);
    steps(&mut sim, 1.5);
    assert_eq!(sim.story.stage, GO);
    assert!(sim.sheet(PlayerId(1)).unwrap().briefed);
    // The guest's own file keeps its own place in the story.
    let b = sim.sheet(PlayerId(2)).unwrap();
    assert_eq!((b.story, b.briefed), (5, true));
    let _ = host;
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn chapters_are_placed_apart_and_near_the_quarter() {
    let mut sim = town();
    let _ = &mut sim;
    let hall = sim.story.hall().unwrap();
    let flat = |p: DVec3| DVec2::new(p.x, p.z);
    for (k, _) in CHAPTERS.iter().enumerate() {
        let (at, _) = sim.story.place(k).unwrap();
        assert!(
            flat(at).distance(flat(hall)) > 100.0,
            "chapter {} plays on the hall's doorstep",
            k + 1
        );
    }
    // The last chapter is below ground, in the Doctor's lab.
    let (lair, _) = sim.story.place(CHAPTERS.len() - 1).unwrap();
    let ground = sim.solids().ground_at(flat(lair));
    assert!(
        lair.y < ground - 10.0,
        "the Doctor is not in his lab: {lair}"
    );
    let _ = story::WAKE;
}
