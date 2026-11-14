//! Augments through the simulation: set, replaced, traded in, lost with a
//! power given up, dropped by villains, and how many an hour of play finds.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_net::{FxKind, PlayerId, PowerId, MAX_BAG};
use hw_sim::augments::{self, Effect, Grade};
use hw_sim::balance::{bot_input, hero_sheet};
use hw_sim::collision::Obstacles;
use hw_sim::progress;
use hw_sim::{Sim, GROUND_Y, TICK_HZ};
use std::sync::Arc;

const ATOMIC_RAY: PowerId = PowerId(10);

fn arena(seed: u64) -> Sim {
    let mut sim = Sim::with_solids(Seed(seed), Arc::new(Obstacles::flat(GROUND_Y)));
    sim.set_spawn((DVec2::new(35.0, 35.0), DVec2::X));
    sim
}

fn ground(x: f64, z: f64) -> DVec3 {
    DVec3::new(x, GROUND_Y, z)
}

#[test]
fn augments_are_set_replaced_traded_and_lost_with_the_power() {
    let mut sim = arena(3);
    let me = PlayerId(1);
    let hero = sim.spawn_hero_with(
        me,
        ground(0.0, 0.0),
        hero_sheet(10, &[ATOMIC_RAY, PowerId(12)]),
    );
    let plain = sim.fighter(hero).unwrap().kit.fields(ATOMIC_RAY);
    let damage = augments::make(Effect::Damage, Grade::Common, 10);
    let more = augments::make(Effect::Damage, Grade::Relic, 20);
    let hold = augments::make(Effect::Hold, Grade::Rare, 10);
    for a in [damage, more, hold] {
        sim.give_augment(me, a);
    }
    // Into a socket: the power hits harder at once, the bag is one shorter.
    assert_eq!(sim.set_augment(me, 0, ATOMIC_RAY, 0), Ok(None));
    let with = sim.fighter(hero).unwrap().kit.fields(ATOMIC_RAY);
    assert!(
        with.damage > plain.damage,
        "{} > {}",
        with.damage,
        plain.damage
    );
    assert_eq!(sim.sheet(me).unwrap().bag, vec![more, hold]);
    // The same effect cannot sit in two sockets of one power...
    assert_eq!(
        sim.set_augment(me, 0, ATOMIC_RAY, 1),
        Err(augments::SetError::SameEffect)
    );
    // ...but replaces itself, and the old one is gone for good.
    assert_eq!(sim.set_augment(me, 0, ATOMIC_RAY, 0), Ok(Some(damage)));
    let best = sim.fighter(hero).unwrap().kit.fields(ATOMIC_RAY);
    assert!(best.damage > with.damage);
    // A new behaviour: the ray holds now.
    assert_eq!(plain.hold, 0.0);
    assert_eq!(sim.set_augment(me, 0, ATOMIC_RAY, 1), Ok(None));
    assert!(sim.fighter(hero).unwrap().kit.fields(ATOMIC_RAY).hold > 0.0);
    assert!(sim.sheet(me).unwrap().bag.is_empty());
    // Nothing there, a power not owned, a socket there is not.
    assert_eq!(
        sim.set_augment(me, 0, ATOMIC_RAY, 2),
        Err(augments::SetError::NoSuchAugment)
    );
    sim.give_augment(me, damage);
    assert_eq!(
        sim.set_augment(me, 0, PowerId(30), 0),
        Err(augments::SetError::NotOwned)
    );
    assert_eq!(
        sim.set_augment(me, 0, ATOMIC_RAY, 3),
        Err(augments::SetError::NoSuchSocket)
    );
    // Traded in: karma to spend, the level stays.
    let before = sim.sheet(me).unwrap();
    let karma = sim.trade_augment(me, 0).unwrap();
    let after = sim.sheet(me).unwrap();
    assert_eq!(karma, augments::trade_value(damage));
    assert!(karma > 0);
    assert_eq!(after.karma, before.karma + karma);
    assert_eq!(after.lifetime, before.lifetime);
    assert!(after.bag.is_empty());
    // Given up: the power's augments go with it (Ray Burst needs the ray,
    // so that goes first).
    sim.give_up(me, PowerId(12)).unwrap();
    assert_eq!(after.augments.len(), 2);
    sim.give_up(me, ATOMIC_RAY).unwrap();
    assert!(sim.sheet(me).unwrap().augments.is_empty());
}

#[test]
fn a_full_bag_trades_the_next_one_in() {
    let mut sim = arena(4);
    let me = PlayerId(1);
    sim.spawn_hero_with(me, ground(0.0, 0.0), hero_sheet(5, &[ATOMIC_RAY]));
    let a = augments::make(Effect::Damage, Grade::Common, 5);
    for _ in 0..MAX_BAG {
        sim.give_augment(me, a);
    }
    let before = sim.sheet(me).unwrap();
    assert_eq!(before.bag.len(), MAX_BAG);
    sim.give_augment(me, a);
    let after = sim.sheet(me).unwrap();
    assert_eq!(after.bag.len(), MAX_BAG);
    assert_eq!(after.karma, before.karma + augments::trade_value(a));
}

/// Where a lone hero goes next: the nearest corner with villains, or, with
/// all beaten, the farthest one (so the others fill again behind them).
fn next_spot(sim: &mut Sim, me: DVec3) -> DVec3 {
    let spots = sim.fights().spots.clone();
    let near = spots
        .iter()
        .filter(|s| s.members > 0)
        .min_by(|a, b| a.center.distance(me).total_cmp(&b.center.distance(me)));
    match near {
        Some(s) => s.center,
        None => {
            spots
                .iter()
                .max_by(|a, b| a.center.distance(me).total_cmp(&b.center.distance(me)))
                .expect("spots")
                .center
        }
    }
}

/// An hour of a hero of `level` fighting from corner to corner: the drops
/// found, by grade, and the villains beaten.
fn an_hour(level: u8, seed: u64) -> ([u32; 3], u32) {
    let mut sim = arena(seed);
    let me = PlayerId(1);
    let powers = [ATOMIC_RAY, PowerId(12), PowerId(13)];
    let hero = sim.spawn_hero_with(me, ground(75.0, 75.0), hero_sheet(level, &powers));
    for (x, z) in [(0.0, 0.0), (150.0, 0.0), (150.0, 150.0), (0.0, 150.0)] {
        let c = ground(x, z);
        let facing = (DVec2::new(75.0, 75.0) - DVec2::new(x, z)).normalize();
        sim.add_fight_spot(c, facing);
    }
    let mut drops = [0u32; 3];
    let mut beaten = 0;
    for t in 0..TICK_HZ as u64 * 3600 {
        let at = sim.position(hero).unwrap();
        let goal = next_spot(&mut sim, at);
        let input = bot_input(&mut sim, hero, t, Some(goal));
        sim.set_input(me, input);
        sim.step();
        for e in sim.fx() {
            if e.kind == FxKind::Drop && e.to == hero {
                drops[((e.amount >> 8) as usize).min(2)] += 1;
            }
            if e.defeat && e.from == hero && e.to != hero && e.kind != FxKind::Drop {
                beaten += 1;
            }
        }
        // The bag never overflows a test: what is found is traded in.
        if t.is_multiple_of(TICK_HZ as u64 * 60) {
            while sim.trade_augment(me, 0).is_ok() {}
        }
    }
    (drops, beaten)
}

#[test]
#[ignore = "on demand (balance): scripts/check.sh --on-demand"]
fn an_hour_of_fighting_finds_about_twenty_augments() {
    for (level, seed) in [(3, 21), (10, 22)] {
        let (drops, beaten) = an_hour(level, seed);
        let all: u32 = drops.iter().sum();
        println!("level {level}: {beaten} beaten, drops {drops:?} = {all}");
        let want = augments::DROPS_AN_HOUR;
        assert!(
            (want * 0.6..want * 1.5).contains(&(all as f64)),
            "level {level}: {all} drops in an hour, want about {want}"
        );
        // Mostly common ones, and a better one now and then.
        assert!(drops[0] > drops[1] + drops[2], "{drops:?}");
    }
    let _ = progress::MAX_LEVEL;
}

/// Two heroes swap augments: nothing moves until both have said yes to
/// the same table, and then both bags change in one step.
#[test]
fn two_heroes_swap_augments_only_when_both_say_yes_to_the_same_table() {
    use hw_net::SwapState;
    let mut sim = arena(5);
    let (ann, bob) = (PlayerId(1), PlayerId(2));
    sim.spawn_hero_with(ann, ground(0.0, 0.0), hero_sheet(10, &[ATOMIC_RAY]));
    sim.spawn_hero_with(bob, ground(3.0, 0.0), hero_sheet(10, &[ATOMIC_RAY]));
    let damage = augments::make(Effect::Damage, Grade::Common, 10);
    let hold = augments::make(Effect::Hold, Grade::Rare, 10);
    let burn = augments::make(Effect::Burn, Grade::Relic, 10);
    sim.give_augment(ann, damage);
    sim.give_augment(ann, hold);
    sim.give_augment(bob, burn);
    let bag = |sim: &mut Sim, p| sim.sheet(p).unwrap().bag;
    let last = |sim: &mut Sim, p: PlayerId| {
        sim.take_swap_news()
            .into_iter()
            .rev()
            .find(|(q, _)| *q == p)
            .map(|(_, s)| s)
    };

    // Ann lays one on the table; Bob sees it.
    sim.swap_offer(ann, bob, vec![damage]);
    let seen = last(&mut sim, bob).expect("Bob is told");
    assert_eq!((seen.with, seen.state), (ann, SwapState::Open));
    assert_eq!(
        (seen.theirs.clone(), seen.mine.clone()),
        (vec![damage], vec![])
    );
    // What is not in the bag cannot be laid on it.
    sim.swap_offer(ann, bob, vec![burn]);
    assert_eq!(last(&mut sim, bob).unwrap().theirs, vec![damage]);

    // Bob says yes; Ann then changes the table: his yes is gone, and a
    // yes to the old table does not count.
    let rev = seen.rev;
    sim.swap_yes(bob, rev);
    assert!(last(&mut sim, ann).unwrap().their_yes);
    sim.swap_offer(ann, bob, vec![hold]);
    let now = last(&mut sim, ann).unwrap();
    assert!(!now.their_yes && now.rev != rev);
    sim.swap_yes(ann, rev);
    sim.swap_yes(bob, rev);
    assert_eq!(bag(&mut sim, ann), vec![damage, hold], "nothing has moved");

    // Bob lays his own, both say yes to what they see: the bags change.
    sim.swap_offer(bob, ann, vec![burn]);
    let rev = last(&mut sim, ann).unwrap().rev;
    sim.swap_yes(ann, rev);
    assert_eq!(bag(&mut sim, bob), vec![burn], "one yes moves nothing");
    sim.swap_yes(bob, rev);
    assert_eq!(last(&mut sim, ann).unwrap().state, SwapState::Done);
    assert_eq!(bag(&mut sim, ann), vec![damage, burn]);
    assert_eq!(bag(&mut sim, bob), vec![hold]);
    assert_eq!(sim.swaps.open(), 0);
}

/// A swap needs the two heroes together, one table per hero, and room in
/// the bags; an augment traded in meanwhile leaves the table.
#[test]
fn a_swap_needs_nearness_room_and_what_was_laid_down() {
    use hw_net::SwapState;
    let mut sim = arena(6);
    let (ann, bob, cy) = (PlayerId(1), PlayerId(2), PlayerId(3));
    sim.spawn_hero_with(ann, ground(0.0, 0.0), hero_sheet(10, &[ATOMIC_RAY]));
    sim.spawn_hero_with(bob, ground(3.0, 0.0), hero_sheet(10, &[ATOMIC_RAY]));
    sim.spawn_hero_with(cy, ground(60.0, 0.0), hero_sheet(10, &[ATOMIC_RAY]));
    let damage = augments::make(Effect::Damage, Grade::Common, 10);
    let hold = augments::make(Effect::Hold, Grade::Rare, 10);
    sim.give_augment(ann, damage);
    sim.give_augment(ann, hold);
    let state = |sim: &mut Sim, p: PlayerId| {
        sim.take_swap_news()
            .into_iter()
            .rev()
            .find(|(q, _)| *q == p)
            .map(|(_, s)| s.state)
    };
    // Too far away.
    sim.swap_offer(ann, cy, vec![damage]);
    assert_eq!(state(&mut sim, ann), Some(SwapState::Far));
    assert_eq!(sim.swaps.open(), 0);
    // Bob is at a table with Ann: Cy cannot have him.
    sim.swap_offer(ann, bob, vec![damage, hold]);
    sim.swap_offer(cy, bob, vec![]);
    assert_eq!(state(&mut sim, cy), Some(SwapState::Busy));
    // Ann trades one in: it leaves the table, and yeses start over.
    let rev = {
        sim.swap_offer(ann, bob, vec![damage, hold]);
        sim.take_swap_news().last().unwrap().1.rev
    };
    sim.swap_yes(bob, rev);
    sim.trade_augment(ann, 0).unwrap();
    sim.step();
    let news = sim.take_swap_news();
    let told = &news.iter().rev().find(|(q, _)| *q == bob).unwrap().1;
    assert_eq!(told.theirs, vec![hold]);
    assert!(!told.my_yes && told.rev != rev);
    // Bob's bag is full: nothing moves, and both are told why.
    while sim.sheet(bob).unwrap().bag.len() < MAX_BAG {
        sim.give_augment(bob, damage);
    }
    let rev = told.rev;
    sim.swap_yes(ann, rev);
    sim.swap_yes(bob, rev);
    assert_eq!(state(&mut sim, ann), Some(SwapState::Full));
    assert_eq!(sim.sheet(ann).unwrap().bag, vec![hold]);
    // Called off: the table is gone.
    sim.swap_off(bob);
    assert_eq!(state(&mut sim, ann), Some(SwapState::Off));
    assert_eq!(sim.swaps.open(), 0);
    // A hero who leaves ends their swap.
    sim.swap_offer(ann, bob, vec![hold]);
    sim.remove_hero(bob);
    assert_eq!(state(&mut sim, ann), Some(SwapState::Off));
}

/// Either hero puts back what they do not want of the other's side; a
/// Herobot lays out its whole bag when asked and agrees to what is left.
#[test]
fn a_swap_can_be_for_a_selection_and_a_herobot_lays_out_its_bag() {
    use hw_net::SwapState;
    let mut sim = arena(8);
    let (ann, bot) = (PlayerId(1), PlayerId(2));
    sim.spawn_hero_with(ann, ground(0.0, 0.0), hero_sheet(10, &[ATOMIC_RAY]));
    sim.spawn_hero_with(bot, ground(3.0, 0.0), hero_sheet(10, &[ATOMIC_RAY]));
    sim.tag_herobot(bot, 0);
    let damage = augments::make(Effect::Damage, Grade::Common, 10);
    let hold = augments::make(Effect::Hold, Grade::Rare, 10);
    let burn = augments::make(Effect::Burn, Grade::Relic, 10);
    for a in [damage, hold, burn] {
        sim.give_augment(bot, a);
    }
    let last = |sim: &mut Sim, p: PlayerId| {
        sim.take_swap_news()
            .into_iter()
            .rev()
            .find(|(q, _)| *q == p)
            .map(|(_, s)| s)
    };
    // Ann asks: the robot's whole bag is on the table, and its yes said.
    sim.swap_offer(ann, bot, vec![]);
    let seen = last(&mut sim, ann).unwrap();
    assert_eq!(seen.theirs, vec![damage, hold, burn]);
    assert!(seen.their_yes && !seen.my_yes);
    // She wants only the relic: the other two go back.
    sim.swap_put_back(ann, damage);
    sim.swap_put_back(ann, hold);
    let seen = last(&mut sim, ann).unwrap();
    assert_eq!(seen.theirs, vec![burn]);
    assert!(seen.their_yes, "the robot agrees to what is left");
    sim.swap_yes(ann, seen.rev);
    assert_eq!(last(&mut sim, ann).unwrap().state, SwapState::Done);
    assert_eq!(sim.sheet(ann).unwrap().bag, vec![burn]);
    assert_eq!(sim.sheet(bot).unwrap().bag, vec![damage, hold]);
}
