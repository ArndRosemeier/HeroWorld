//! Ten bots play on one host over UDP for ten minutes of game time (run as
//! fast as the machine allows): running, jumping, flying, chatting. Nobody
//! drops, everybody's copy of the world agrees with the server's, and no
//! client costs more than the bandwidth budget.
//!
//! `HW_BOT_MINUTES` changes the length (default 10).

use glam::DVec3;
use hw_core::{NetId, Seed};
use hw_net::{ConnState, Connection, InputState, PlayerId, Power, ToServer};
use hw_sim::host::{Host, INTEREST_RADIUS, MAX_PLAYERS};
use hw_sim::{GeneratedTown, Sim, TICK_HZ};
use std::time::{Duration, Instant};

/// Where a bot hangs around: half of them move to the far side of town
/// every other half minute, so bubbles split and merge again.
fn bot_home(bot: u32, tick: u64, home: DVec3) -> DVec3 {
    if bot >= 5 && (tick / (TICK_HZ as u64 * 30)) % 2 == 1 {
        home + DVec3::new(800.0, 0.0, 300.0)
    } else {
        home
    }
}

/// A bot's input at `tick`: a few seconds of one manoeuvre, then the next;
/// it flies home when it strays too far.
fn bot_input(bot: u32, tick: u64, at: DVec3, home: DVec3) -> InputState {
    let home = bot_home(bot, tick, home);
    let phase = (tick / (TICK_HZ as u64 * 4) + bot as u64 * 7) % 10;
    let angle =
        (bot as f64 * 2.4 + (tick / (TICK_HZ as u64 * 4)) as f64 * 1.7) % std::f64::consts::TAU;
    let mut dir = DVec3::new(angle.cos(), 0.0, angle.sin());
    let away = at - home;
    let far = DVec3::new(away.x, 0.0, away.z).length();
    if far > 150.0 {
        dir = -DVec3::new(away.x, 0.0, away.z).normalize();
    }
    if far > 250.0 {
        return InputState {
            move_x: dir.x as f32,
            move_z: dir.z as f32,
            fly: true,
            boost: true,
            jump: at.y < home.y + 60.0,
            ..Default::default()
        };
    }
    let mut input = InputState {
        move_x: dir.x as f32,
        move_z: dir.z as f32,
        ..Default::default()
    };
    match phase {
        0 => input = InputState::default(),
        1..=4 => {}
        5 | 6 => input.jump = tick.is_multiple_of(20),
        _ => {
            input.fly = true;
            input.boost = phase == 9;
            input.jump = at.y < home.y + 40.0;
        }
    }
    input
}

#[test]
fn ten_bots_play_for_ten_minutes() {
    let minutes: u64 = std::env::var("HW_BOT_MINUTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let ticks = minutes * 60 * TICK_HZ as u64;
    let seed = Seed(21);
    let town = GeneratedTown::generate(seed, 20_000.0);
    let mut host = Host::with_sim(Sim::for_town(seed, &town));
    host.sim.place_starter_crates();
    let addr = host.listen("127.0.0.1:0").expect("bind");
    let mut bots: Vec<Connection> = (0..MAX_PLAYERS)
        .map(|k| Connection::open(addr, &format!("bot{k}")).expect("socket"))
        .collect();
    let home = host.sim.spawn_point(1);

    let started = Instant::now();
    for _ in 0..30 {
        for b in &mut bots {
            b.update(InputState::default());
        }
        std::thread::sleep(Duration::from_millis(2));
        host.tick();
    }
    assert_eq!(host.players().len(), MAX_PLAYERS);
    // Every bot plays a level-5 hero who can fly and jump.
    let mut flier = hw_sim::progress::new_hero(0);
    flier.lifetime = hw_sim::progress::karma_for_level(5);
    flier.karma = 5_000;
    hw_sim::progress::buy(&mut flier, Power::SuperJump, 1).unwrap();
    hw_sim::progress::buy(&mut flier, Power::Flight, 0).unwrap();
    for b in &mut bots {
        b.send(ToServer::Sheet(flier.clone()));
    }
    let ids: Vec<(PlayerId, NetId)> = bots
        .iter()
        .map(|b| match b.state {
            ConnState::Connected { player, hero, .. } => (player, hero),
            ref s => panic!("bot not connected: {s:?}"),
        })
        .collect();
    let first_bytes: Vec<u64> = ids
        .iter()
        .map(|(p, _)| host.traffic(*p).unwrap().bytes_out)
        .collect();

    let mut max_bubbles = 0;
    let mut min_bubbles = usize::MAX;
    for t in 0..ticks {
        let settle = t + 3 * TICK_HZ as u64 >= ticks;
        for (k, b) in bots.iter_mut().enumerate() {
            let me = b.hero().and_then(|h| b.mirror.get(h));
            let at = me.map_or(home, |e| DVec3::from_array(e.pos));
            let input = if settle {
                // The last seconds: land and stand still.
                InputState::default()
            } else {
                bot_input(k as u32, t, at, home)
            };
            b.update(input);
            if t % (TICK_HZ as u64 * 45) == k as u64 * 30 {
                b.chat(&format!("bot{k} at tick {t}"));
            }
            b.take_news();
        }
        host.tick();
        max_bubbles = max_bubbles.max(host.sim.bubbles.len());
        min_bubbles = min_bubbles.min(host.sim.bubbles.len());
        if t % 1000 == 0 {
            // Give the loopback a breather now and then.
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    // Deliver the last updates: the bots read last, after the host's
    // final tick, so what still moves (a villain falling) is up to date.
    for _ in 0..10 {
        for b in &mut bots {
            b.update(InputState::default());
        }
        std::thread::sleep(Duration::from_millis(2));
        host.tick();
    }
    std::thread::sleep(Duration::from_millis(5));
    for b in &mut bots {
        b.update(InputState::default());
    }
    let elapsed = started.elapsed();

    assert_eq!(host.players().len(), MAX_PLAYERS, "somebody dropped");
    let seconds = ticks as f64 / TICK_HZ as f64;
    let truth = host.sim.snapshot();
    for ((b, (player, hero)), first) in bots.iter().zip(&ids).zip(&first_bytes) {
        assert!(
            matches!(b.state, ConnState::Connected { .. }),
            "{player:?}: {:?}",
            b.state
        );
        let per_second = (host.traffic(*player).unwrap().bytes_out - first) as f64 / seconds;
        println!("{player:?}: {per_second:.0} bytes/s");
        assert!(
            per_second < hw_net::BANDWIDTH_BUDGET as f64,
            "{player:?} cost {per_second:.0} bytes/s"
        );
        // Everything near this bot is in its copy, exactly where the server has it.
        let me = truth.iter().find(|e| e.id == *hero).unwrap();
        let at = DVec3::from_array(me.pos);
        for e in &truth {
            let d = DVec3::from_array(e.pos).distance(at);
            if d > INTEREST_RADIUS {
                continue;
            }
            let mine = b
                .mirror
                .get(e.id)
                .unwrap_or_else(|| panic!("{player:?} misses {:?} at {d:.0} m", e.id));
            assert_eq!(
                mine.pos, e.pos,
                "{player:?} sees {:?} ({:?}) elsewhere",
                e.id, e.kind
            );
        }
    }
    assert!(max_bubbles >= 2, "the bots never split up");
    assert!(min_bubbles <= 1 + MAX_PLAYERS / 2);
    println!(
        "{minutes} game minutes in {:.1} s; bubbles {min_bubbles}..{max_bubbles}",
        elapsed.as_secs_f64()
    );
}
