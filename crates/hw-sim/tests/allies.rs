//! Ally targeting and augment trading: a picked teammate gets the heal
//! from further away than the power's radius, and an augment offered to
//! another player moves between bags only when they take it.

use glam::DVec3;
use hw_core::Seed;
use hw_net::{Augment, Connection, InputState, PlayerId, PowerId, ToClient, ToServer};
use hw_sim::collision::{Body, Obstacles};
use hw_sim::host::Host;
use hw_sim::powers::{self, Element};
use hw_sim::{progress, GeneratedTown, Sim, GROUND_Y, TICK_HZ};
use std::sync::Arc;
use std::time::Duration;

const MEND: PowerId = PowerId(48);

/// A hero file with Aether Mend (and what leads to it).
fn healer() -> hw_net::Sheet {
    for origin in 0..5u8 {
        let mut s = progress::new_hero(origin, Element::Fire);
        s.lifetime = progress::karma_for_level(20);
        s.karma = 1_000_000;
        for _ in 0..40 {
            for d in powers::POWERS.iter() {
                if progress::can_buy(&s, d.id).is_ok() {
                    let _ = progress::buy(&mut s, d.id);
                }
            }
        }
        if s.powers.iter().any(|o| o.power == MEND) {
            return s;
        }
    }
    panic!("no origin reaches Aether Mend");
}

/// Healer at the origin, a wounded friend `apart` metres off; the healer
/// casts Mend (aimed at `ally`). Returns the friend's health afterwards.
fn mended(apart: f64, pick: bool) -> f32 {
    let mut sim = Sim::with_solids(Seed(5), Arc::new(Obstacles::flat(GROUND_Y)));
    sim.set_spawn((glam::DVec2::ZERO, glam::DVec2::X));
    let a = sim.spawn_hero_with(PlayerId(1), DVec3::new(0.0, GROUND_Y, 0.0), healer());
    let b = sim.spawn_hero(PlayerId(2), DVec3::new(apart, GROUND_Y, 0.0));
    sim.step();
    assert!(
        sim.fighter(a).unwrap().kit.has(MEND),
        "the healer owns Mend"
    );
    let mut f = sim.fighter(b).unwrap();
    f.health = f.max_health * 0.3;
    let hurt = f.health;
    sim.set_fighter(b, f);
    for t in 0..TICK_HZ as u64 * 3 {
        sim.set_input(
            PlayerId(1),
            InputState {
                power: if t == 2 { MEND.0 } else { 0 },
                ally: pick.then_some(b),
                ..Default::default()
            },
        );
        sim.step();
    }
    let after = sim.fighter(b).unwrap().health;
    assert!(after >= hurt - 0.5, "nobody hurt the friend");
    after - hurt
}

#[test]
fn a_picked_teammate_is_healed_from_beyond_the_radius() {
    // Mend reaches 14 m. At 30 m only a picked friend gets it.
    // (A hurt hero also mends a little by itself: compare with a control.)
    let rest = mended(30.0, false);
    let near = mended(5.0, false);
    assert!(
        near > rest + 5.0,
        "a friend in reach is healed ({near} v {rest})"
    );
    let far_picked = mended(30.0, true);
    assert!(
        far_picked > rest + 5.0,
        "out of reach, picked: {far_picked} v {rest}"
    );
    // Past the ally reach even a pick does not help.
    let too_far = mended(hw_net::ALLY_REACH + 10.0, true);
    assert!(
        too_far < rest + 1.0,
        "beyond ALLY_REACH: {too_far} v {rest}"
    );
}

fn gift() -> Augment {
    Augment {
        effect: 2,
        percent: 14,
        grade: 1,
    }
}

/// One local hero and one player over UDP, standing together.
fn pair() -> (Host, PlayerId, PlayerId, Connection) {
    let seed = Seed(3);
    let town = GeneratedTown::generate(seed, 20_000.0);
    let mut host = Host::with_sim(Sim::for_town(seed, &town));
    let me = host.join_local("Giver");
    let addr = host.listen("127.0.0.1:0").unwrap();
    let mut friend = Connection::open(addr, "Taker").unwrap();
    for _ in 0..90 {
        friend.update(InputState::default());
        std::thread::sleep(Duration::from_millis(3));
        host.tick();
    }
    let id = *host
        .players()
        .iter()
        .find(|(_, p)| p.name == "Taker")
        .map(|(id, _)| id)
        .expect("joined");
    // Side by side.
    let mine = host.sim.hero_of(me).unwrap();
    let at = host.sim.position(mine).unwrap();
    let hero = host.sim.hero_of(id).unwrap();
    host.sim.set_body(
        hero,
        Body {
            pos: at + DVec3::new(2.0, 0.0, 0.0),
            vel: DVec3::ZERO,
            grounded: true,
        },
    );
    (host, me, id, friend)
}

/// Runs until the friend hears the offer (a join's backlog of news comes
/// first, about 35 s). Returns whether it came.
fn hear_offer(host: &mut Host, friend: &mut Connection, from: PlayerId) -> bool {
    for _ in 0..TICK_HZ * 80 {
        friend.update(InputState::default());
        std::thread::sleep(Duration::from_millis(2));
        host.tick();
        for m in friend.take_news() {
            if matches!(m, ToClient::AugmentOffer { from: f, open: true, .. } if f == from) {
                return true;
            }
        }
    }
    false
}

fn bag_has(host: &mut Host, p: PlayerId) -> bool {
    host.sim.sheet(p).unwrap().bag.contains(&gift())
}

#[test]
fn an_offered_augment_moves_bags_when_it_is_taken() {
    let (mut host, me, id, mut friend) = pair();
    host.sim.give_augment(me, gift());
    host.offer_augment(me, 0, id);
    assert!(hear_offer(&mut host, &mut friend, me), "the offer arrives");
    // Offered is not given: it is still in the giver's bag.
    assert!(bag_has(&mut host, me) && !bag_has(&mut host, id));
    friend.send(ToServer::AnswerOffer {
        from: me,
        accept: true,
    });
    for _ in 0..TICK_HZ * 3 {
        friend.update(InputState::default());
        std::thread::sleep(Duration::from_millis(2));
        host.tick();
    }
    assert!(bag_has(&mut host, id), "the taker has it");
    assert!(!bag_has(&mut host, me), "the giver does not");
}

#[test]
fn a_refused_or_far_offer_changes_nothing() {
    let (mut host, me, id, mut friend) = pair();
    host.sim.give_augment(me, gift());
    host.offer_augment(me, 0, id);
    assert!(hear_offer(&mut host, &mut friend, me));
    friend.send(ToServer::AnswerOffer {
        from: me,
        accept: false,
    });
    for _ in 0..TICK_HZ * 2 {
        friend.update(InputState::default());
        std::thread::sleep(Duration::from_millis(2));
        host.tick();
    }
    assert!(bag_has(&mut host, me) && !bag_has(&mut host, id));
    // Too far to trade: no offer is made, so nothing can be taken.
    let hero = host.sim.hero_of(id).unwrap();
    let at = host.sim.position(hero).unwrap();
    host.sim.set_body(
        hero,
        Body {
            pos: at + DVec3::new(100.0, 0.0, 0.0),
            vel: DVec3::ZERO,
            grounded: true,
        },
    );
    host.offer_augment(me, 0, id);
    friend.send(ToServer::AnswerOffer {
        from: me,
        accept: true,
    });
    for _ in 0..TICK_HZ * 2 {
        friend.update(InputState::default());
        std::thread::sleep(Duration::from_millis(2));
        host.tick();
    }
    assert!(bag_has(&mut host, me) && !bag_has(&mut host, id));
}
