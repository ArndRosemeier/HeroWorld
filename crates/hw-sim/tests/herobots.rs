//! The Herobots: two robots on Hero Hall's plaza that a player wakes and
//! sends home again. A woken one is a real client over UDP: it takes one of
//! the ten places, plays a hero file made from the waking hero's karma and
//! a random origin, fights what the wrist radio calls out, earns karma like
//! anyone, and when it is beaten the medical drone flies it to the hospital.

use glam::DVec3;
use hw_core::Seed;
use hw_net::{ConnState, Connection, EntityKind, FxKind, InputState, PlayerId, Status};
use hw_sim::collision::Body;
use hw_sim::herobot::{self, Brain, Ctl, Drive, Params};
use hw_sim::herobots;
use hw_sim::host::{Host, MAX_PLAYERS};
use hw_sim::incidents::IncidentKind;
use hw_sim::{progress, GeneratedTown, Sim, TICK_HZ};
use std::sync::Arc;
use std::time::Duration;

fn world() -> (Host, PlayerId) {
    let seed = Seed(3);
    let town = GeneratedTown::generate(seed, 20_000.0);
    let mut host = Host::with_sim(Sim::for_town(seed, &town));
    let me = host.join_local("Arnd");
    let mut sheet = progress::new_hero(2, hw_sim::powers::Element::Fire);
    sheet.lifetime = progress::karma_for_level(8);
    sheet.karma = 1_000;
    host.sim.set_sheet(me, sheet);
    (host, me)
}

fn idle_bots(host: &mut Host) -> Vec<u8> {
    let mut slots: Vec<u8> = host
        .sim
        .snapshot()
        .iter()
        .filter(|e| e.kind == EntityKind::Herobot)
        .filter_map(|e| e.mount.map(|m| m.1))
        .collect();
    slots.sort();
    slots
}

/// Stands our hero beside the idle Herobot of `slot`.
fn go_to_the_bot(host: &mut Host, me: PlayerId, slot: u8) {
    let at = host.sim.herobot_idle_at(slot).expect("it stands there");
    let hero = host.sim.hero_of(me).unwrap();
    host.sim.set_body(
        hero,
        Body {
            pos: at + DVec3::new(0.0, 0.1, 2.0),
            vel: DVec3::ZERO,
            grounded: true,
        },
    );
}

fn run_for(host: &mut Host, seconds: f64, until: impl Fn(&mut Host) -> bool) -> bool {
    for _ in 0..(seconds * TICK_HZ as f64) as u64 {
        host.tick();
        if until(host) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(1000 / TICK_HZ as u64));
    }
    false
}

#[test]
fn a_herobot_is_woken_plays_as_a_player_and_goes_home() {
    let (mut host, me) = world();
    assert_eq!(idle_bots(&mut host), vec![0, 1], "two stand on the plaza");
    // Too far away: nothing happens.
    host.herobot(me, 0);
    assert_eq!(host.players().len(), 1);
    assert_eq!(idle_bots(&mut host), vec![0, 1]);

    go_to_the_bot(&mut host, me, 0);
    let mine = host.sim.sheet(me).unwrap();
    host.herobot(me, 0);
    assert_eq!(
        idle_bots(&mut host),
        vec![1],
        "the woken one left the plaza"
    );
    // It joins over UDP, even though this host had no socket.
    assert!(
        run_for(&mut host, 15.0, |h| h.players().len() == 2),
        "the Herobot never joined"
    );
    let bot = *host
        .players()
        .iter()
        .find(|(_, p)| p.name == herobots::name(0))
        .map(|(id, _)| id)
        .expect("Herobot 1 is a player");
    assert!(
        host.players()[&bot].remote,
        "a remote player, not a local one"
    );
    // Its hero file shows once the first Sheet is in.
    assert!(run_for(&mut host, 5.0, |h| {
        h.sim
            .sheet(bot)
            .is_some_and(|s| s.lifetime == mine.lifetime)
    }));
    let sheet = host.sim.sheet(bot).unwrap();
    // All the waking hero's karma, spent on powers it can unlock.
    assert_eq!(sheet.lifetime, mine.lifetime);
    assert!(sheet.element != 0 && (sheet.origin as usize) < progress::ORIGINS.len());
    assert!(!sheet.powers.is_empty());
    let spent: u32 = sheet.powers.iter().map(|p| p.paid).sum();
    assert_eq!(spent + sheet.karma, mine.lifetime, "karma went missing");
    assert!(
        sheet.karma < 960 * 4,
        "it left karma unspent: {}",
        sheet.karma
    );
    // Its hero is marked, so every client draws it as a robot.
    let robots: Vec<_> = host
        .sim
        .snapshot()
        .into_iter()
        .filter(|e| e.kind == EntityKind::Hero && e.owner == Some(bot))
        .collect();
    assert_eq!(robots.len(), 1);
    assert_eq!(robots[0].mount.map(|m| m.1), Some(hw_net::HEROBOT_MOUNT));
    // The team's level is the first player's, as for any friend.
    assert_eq!(host.sim.team_level(), progress::level_of(mine.lifetime));

    // Sent home: it walks back, leaves, and the robot stands on the plaza.
    let bot_hero = host.sim.hero_of(bot).unwrap();
    let at = host.sim.position(bot_hero).unwrap();
    let mine_id = host.sim.hero_of(me).unwrap();
    host.sim.set_body(
        mine_id,
        Body {
            pos: at + DVec3::new(0.0, 0.1, 2.0),
            vel: DVec3::ZERO,
            grounded: true,
        },
    );
    host.herobot(me, 0);
    assert!(
        run_for(&mut host, 60.0, |h| h.players().len() == 1),
        "it never went home"
    );
    assert_eq!(idle_bots(&mut host), vec![0, 1], "back on the plaza");
}

#[test]
fn a_herobot_takes_one_of_the_ten_places() {
    let (mut host, me) = world();
    let addr = host.listen("127.0.0.1:0").unwrap();
    let mut fillers: Vec<Connection> = (0..MAX_PLAYERS - 2)
        .map(|k| Connection::open(addr, &format!("friend{k}")).unwrap())
        .collect();
    for _ in 0..40 {
        for f in &mut fillers {
            f.update(InputState::default());
        }
        std::thread::sleep(Duration::from_millis(3));
        host.tick();
    }
    assert_eq!(host.players().len(), MAX_PLAYERS - 1);
    go_to_the_bot(&mut host, me, 0);
    host.herobot(me, 0);
    assert!(run_for(&mut host, 15.0, |h| h.players().len() == MAX_PLAYERS));
    // The tenth place is taken: the second Herobot is refused.
    go_to_the_bot(&mut host, me, 1);
    host.herobot(me, 1);
    run_for(&mut host, 3.0, |_| false);
    assert_eq!(host.players().len(), MAX_PLAYERS);
    assert!(idle_bots(&mut host).contains(&1), "it stays on the plaza");
}

/// Plays a Herobot tick for tick with the host (no thread).
fn lockstep(host: &mut Host, lifetime: u32) -> (Connection, Brain) {
    let addr = host.listen("127.0.0.1:0").unwrap();
    let (home, _) = host.sim.herobot_home(0).unwrap();
    let ctl = Arc::new(Ctl::default());
    let brain = Brain::new(&Params {
        addr,
        name: herobots::name(0),
        slot: 0,
        lifetime,
        seed: 77,
        home,
        solids: host.sim.solids(),
        ctl,
    });
    let conn = Connection::open(addr, &herobots::name(0)).unwrap();
    (conn, brain)
}

fn step(host: &mut Host, conn: &mut Connection, brain: &mut Brain) -> Drive {
    let d = brain.drive(conn);
    std::thread::sleep(Duration::from_micros(300));
    host.tick();
    d
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn a_herobot_fights_earns_karma_and_the_drone_flies_it_to_the_hospital() {
    let (mut host, me) = world();
    host.sim.incidents.first_after = 0.0;
    host.sim.incidents.next_kind = Some(IncidentKind::Mugging);
    let level = progress::level_of(host.sim.sheet(me).unwrap().lifetime);
    let (mut conn, mut brain) = lockstep(&mut host, progress::karma_for_level(level));
    for _ in 0..90 {
        step(&mut host, &mut conn, &mut brain);
    }
    let ConnState::Connected {
        player: bot, hero, ..
    } = conn.state
    else {
        panic!("not connected: {:?}", conn.state);
    };
    let before = host.sim.sheet(bot).expect("a hero file").lifetime;
    // It follows and fights: within ten minutes it has earned karma.
    let mut earned = false;
    for _ in 0..TICK_HZ as u64 * 600 {
        step(&mut host, &mut conn, &mut brain);
        if std::env::var_os("HB_DEBUG").is_some() && host.sim.tick % 150 == 0 {
            let at = host.sim.position(hero).unwrap();
            let f = host.sim.fighter(hero).unwrap();
            let foes: Vec<_> = host
                .sim
                .snapshot()
                .into_iter()
                .filter(|e| e.kind.is_enemy())
                .map(|e| {
                    (
                        e.kind,
                        DVec3::from_array(e.pos).distance(at) as i32,
                        e.combat.map(|c| (c.health, c.status)),
                    )
                })
                .take(4)
                .collect();
            let names: Vec<_> = host
                .sim
                .sheet(bot)
                .unwrap()
                .powers
                .iter()
                .map(|p| hw_sim::powers::def_of(p.power).name)
                .collect();
            eprintln!(
                "t{} at {:.0},{:.0},{:.0} hp {:.0} en {:.0} {:?} foes {:?} inc {} powers {:?}",
                host.sim.tick,
                at.x,
                at.y,
                at.z,
                f.health,
                f.energy,
                f.status,
                foes,
                host.sim.incidents.open.len(),
                names
            );
            for l in host.take_power_log().iter().rev().take(3) {
                eprintln!("   {l}");
            }
        }
        // The waking hero does nothing; the Herobot plays alone.
        if host.sim.sheet(bot).is_some_and(|s| s.lifetime > before) {
            earned = true;
            break;
        }
    }
    assert!(earned, "no karma in ten minutes ({} fights)", brain.fights);
    assert!(brain.fights > 0);

    // Beaten: it waits, calls the drone, and the drone flies it to the
    // hospital, where a pod heals it.
    let mut f = host.sim.fighter(hero).unwrap();
    f.health = 0.0;
    f.status = Status::Down;
    f.until = host.sim.tick;
    host.sim.set_fighter(hero, f);
    let mut seen = Vec::new();
    for _ in 0..TICK_HZ as u64 * 150 {
        step(&mut host, &mut conn, &mut brain);
        seen.extend(host.sim.fx().iter().map(|e| e.kind));
        if seen.contains(&FxKind::Healed) {
            break;
        }
    }
    for k in [FxKind::Lifted, FxKind::Admitted, FxKind::Healed] {
        assert!(seen.contains(&k), "never {k:?}");
    }
    // And it is on its feet again.
    for _ in 0..TICK_HZ * 10 {
        step(&mut host, &mut conn, &mut brain);
    }
    assert_eq!(host.sim.fighter(hero).unwrap().status, Status::Standing);
    let _ = herobot::CALL_DRONE_AFTER;
}
