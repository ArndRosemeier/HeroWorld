//! An evening of co-op play, headless: a full table of bots on one host over
//! UDP, in the town the game itself hosts (villain corners, the first
//! fight, traffic, taxis, incidents). Most bots answer the wrist radio:
//! they fly to incidents, fight their villains, get people out of fires and
//! crashes and carry the hurt to the ambulance. Some ride taxis across town,
//! one leaves and comes back, all of them chat.
//!
//! Every game second the world is checked: nothing at a NaN or flung off
//! the planet, no pile-up of entities. At the end: nobody dropped, every
//! copy of the world agrees with the server's, no client went over the
//! bandwidth budget, the host kept up with real time, incidents came and
//! were resolved, taxis carried fares.
//!
//! `HW_SOAK_MINUTES` sets the length (default 6) and `HW_SOAK_PEOPLE` the
//! town's size (default 20 000, half the game's, to keep the checks quick).
//! An evening in the game's own town:
//! `HW_SOAK_MINUTES=120 HW_SOAK_PEOPLE=40000 cargo test -p hw-sim --test soak -- --nocapture`.

use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_net::{ConnState, Connection, InputState, PlayerId, Power, Status, ToServer};
use hw_sim::host::{Host, INTEREST_RADIUS, MAX_PLAYERS};
use hw_sim::incidents::{Hurt, Incident};
use hw_sim::{GeneratedTown, Sim, DEFAULT_POPULATION, TICK_HZ};
use std::time::{Duration, Instant};

const SECOND: u64 = TICK_HZ as u64;
/// The bot that leaves at two minutes and is back at three.
const LEAVER: usize = MAX_PLAYERS - 1;

/// Input that takes the hero towards `goal`: flying high over the roofs
/// while far, down onto it when near. `None` once there.
fn steer(at: DVec3, goal: DVec3, ground: f64) -> Option<InputState> {
    let flat = DVec2::new(goal.x - at.x, goal.z - at.z);
    let d = flat.length();
    if d < 2.0 && (at.y - goal.y).abs() < 3.0 {
        return None;
    }
    let dir = flat.normalize_or_zero();
    // Cruise 45 m over the ground while far; come down over the goal.
    let want_y = if d > 40.0 {
        ground.max(goal.y) + 45.0
    } else {
        goal.y + 0.5
    };
    let climb = ((want_y - at.y) * 0.3).clamp(-0.8, 0.8);
    // Rise before going on when far below the cruise height.
    let speed = if d > 40.0 && at.y < want_y - 15.0 {
        0.2
    } else {
        (d / 10.0).clamp(0.25, 1.0)
    };
    Some(InputState {
        move_x: (dir.x * speed) as f32,
        move_z: (dir.y * speed) as f32,
        move_y: climb as f32,
        fly: true,
        boost: d > 120.0,
        descend: climb < -0.1,
        ..Default::default()
    })
}

/// A wandering bot: a few seconds of one manoeuvre, then the next, never
/// far from `home`.
fn wander(bot: u32, tick: u64, at: DVec3, home: DVec3, ground: f64) -> InputState {
    let away = DVec2::new(at.x - home.x, at.z - home.z);
    if away.length() > 120.0 {
        return steer(at, home, ground).unwrap_or_default();
    }
    let step = tick / (SECOND * 4);
    let angle = (bot as f64 * 2.4 + step as f64 * 1.7) % std::f64::consts::TAU;
    let mut input = InputState {
        move_x: angle.cos() as f32,
        move_z: angle.sin() as f32,
        ..Default::default()
    };
    match (step + bot as u64 * 7) % 8 {
        0 => input = InputState::default(),
        1..=3 => input.smash = tick.is_multiple_of(SECOND / 2),
        4 | 5 => input.jump = tick.is_multiple_of(20),
        _ => {
            input.fly = true;
            input.jump = at.y < ground + 25.0;
        }
    }
    input
}

/// What a bot that answers the radio does at `inc`: carry the hurt one to
/// the ambulance, get the next person out, or fight the villains there.
fn answer(sim: &mut Sim, me: NetId, at: DVec3, inc: &Incident, tick: u64) -> InputState {
    let ground = sim.solids().ground_at(DVec2::new(at.x, at.z));
    match (inc.hurt, inc.medic) {
        (Some(Hurt::Carried(h)), Some((medic, _))) if h == me => {
            return steer(at, medic, ground).unwrap_or_default();
        }
        (Some(Hurt::Lying(p)), _) => return steer(at, p, ground).unwrap_or_default(),
        _ => {}
    }
    if inc.spot.is_none() {
        // A rescue: hold the help key beside the next one in.
        if let Some(next) = inc.victims().last().copied() {
            let near = next + (inc.at - next) * 0.4;
            return steer(at, near, ground).unwrap_or(InputState {
                revive: true,
                ..Default::default()
            });
        }
        return InputState::default();
    }
    // A fight: hover 10 m up, 15-20 m off the nearest villain, bolts and
    // punches.
    let foe = sim
        .enemies()
        .into_iter()
        .filter(|(_, _, p, f)| {
            matches!(f.status, Status::Standing | Status::Knocked) && p.distance(inc.at) < 60.0
        })
        .min_by(|a, b| a.2.distance(at).total_cmp(&b.2.distance(at)));
    let Some((target, _, there, _)) = foe else {
        return steer(at, inc.at, ground).unwrap_or_default();
    };
    let flat = DVec2::new(there.x - at.x, there.z - at.z);
    let d = flat.length();
    if d > 60.0 {
        return steer(at, there, ground).unwrap_or_default();
    }
    let dir = flat.normalize_or_zero();
    let want = if d > 20.0 {
        dir
    } else if d < 12.0 {
        -dir
    } else {
        DVec2::ZERO
    };
    let aim = (there + DVec3::Y - at).normalize_or_zero();
    let energy = sim.fighter(me).map_or(0.0, |f| f.energy);
    InputState {
        move_x: (want.x * 0.7) as f32,
        move_z: (want.y * 0.7) as f32,
        move_y: ((there.y + 10.0 - at.y) * 0.3).clamp(-0.6, 0.6) as f32,
        fly: true,
        aim: aim.as_vec3().to_array(),
        target: Some(target),
        blast: tick.is_multiple_of(4) && energy >= 10.0,
        smash: d < 3.0,
        ..Default::default()
    }
}

/// A level 8 hero file with flight and the fighting powers.
fn sheet() -> hw_net::Sheet {
    let mut s = hw_sim::progress::new_hero(0);
    s.lifetime = hw_sim::progress::karma_for_level(8);
    s.karma = 100_000;
    for p in [Power::Bolt, Power::Flight, Power::SuperJump, Power::Shield] {
        hw_sim::progress::buy(&mut s, p, 0).unwrap_or_else(|e| panic!("{p:?}: {e:?}"));
    }
    s
}

struct Bot {
    conn: Option<Connection>,
    player: Option<PlayerId>,
    /// Seconds this bot's hero has been down.
    down: u64,
    rides: u32,
}

fn join(host: &mut Host, addr: std::net::SocketAddr, name: &str) -> Connection {
    let mut c = Connection::open(addr, name).expect("socket");
    for _ in 0..30 {
        c.update(InputState::default());
        std::thread::sleep(Duration::from_millis(2));
        host.tick();
        if matches!(c.state, ConnState::Connected { .. }) {
            break;
        }
    }
    c.send(ToServer::Sheet(sheet()));
    c
}

#[test]
#[ignore = "daily (slow blocker): scripts/check.sh --daily, and CI's first run of the day"]
fn an_evening_of_co_op_play() {
    let minutes: u64 = std::env::var("HW_SOAK_MINUTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(6);
    let ticks = minutes * 60 * SECOND;
    let seed = Seed(5);
    let people: f64 = std::env::var("HW_SOAK_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_POPULATION / 2.0);
    let town = GeneratedTown::generate(seed, people);
    let mut host = Host::with_sim(Sim::for_town(seed, &town));
    host.sim.place_starter_crates();
    host.sim.place_first_fight();
    let addr = host.listen("127.0.0.1:0").expect("bind");
    let home = host.sim.spawn_point(1);
    let stands = host.sim.taxis.stands.len();
    assert!(stands >= 2, "{stands} taxi stands");

    let mut bots: Vec<Bot> = (0..MAX_PLAYERS)
        .map(|k| {
            let c = join(&mut host, addr, &format!("bot{k}"));
            Bot {
                conn: Some(c),
                player: None,
                down: 0,
                rides: 0,
            }
        })
        .collect();
    assert_eq!(host.players().len(), MAX_PLAYERS);

    let mut started = std::collections::BTreeSet::new();
    let mut resolved = std::collections::BTreeSet::new();
    let mut timed_out = std::collections::BTreeSet::new();
    let mut most_entities = 0usize;
    let mut tick_time = Duration::ZERO;
    let mut block_start = Duration::ZERO;
    let mut slowest = Duration::ZERO;
    let mut bytes_start: Vec<u64> = Vec::new();
    let mut late_rejoin = false;
    let clock = Instant::now();

    for t in 0..ticks {
        let settle = t + 3 * SECOND >= ticks;
        // The leaver goes at two minutes and is back at three.
        if minutes >= 4 && t == 120 * SECOND {
            if let Some(mut c) = bots[LEAVER].conn.take() {
                c.close();
            }
        }
        if minutes >= 4 && t == 180 * SECOND {
            bots[LEAVER].conn = Some(join(&mut host, addr, "bot-again"));
            bots[LEAVER].player = None;
            late_rejoin = true;
        }
        let open = host.sim.incidents.open.clone();
        for (k, bot) in bots.iter_mut().enumerate() {
            let Some(conn) = bot.conn.as_mut() else {
                continue;
            };
            let ConnState::Connected { player, hero, .. } = conn.state else {
                conn.update(InputState::default());
                continue;
            };
            bot.player = Some(player);
            let at = host.sim.position(hero).unwrap_or(home);
            let ground = host.sim.solids().ground_at(DVec2::new(at.x, at.z));
            let status = host.sim.fighter(hero).map(|f| f.status);
            let mut input = InputState::default();
            if settle {
                // The last seconds: stand still.
            } else if status == Some(Status::Down) {
                // Wait a little for a teammate, then call the drone.
                if t.is_multiple_of(SECOND) {
                    bot.down += 1;
                }
                input.hospital = bot.down > 8;
            } else if k % 3 == 2 {
                bot.down = 0;
                // A rider: every two and a half minutes a taxi to another
                // stand; walks about meanwhile.
                let riding = host.sim.taxis.has_ride(player);
                let due = (t + k as u64 * 20 * SECOND).is_multiple_of(150 * SECOND);
                if due && !riding {
                    let dest = ((k as u64 + t / SECOND) % stands as u64) as u8;
                    conn.hail_taxi(dest);
                    bot.rides += 1;
                }
                if !riding {
                    input = wander(k as u32, t, at, home, ground);
                }
            } else {
                bot.down = 0;
                // Answers the radio: the nearest open incident.
                let near = open
                    .iter()
                    .min_by(|a, b| a.at.distance(at).total_cmp(&b.at.distance(at)));
                input = match near {
                    Some(inc) => answer(&mut host.sim, hero, at, inc, t),
                    None => wander(k as u32, t, at, home, ground),
                };
            }
            conn.update(input);
            if t % (SECOND * 50) == k as u64 * 40 {
                conn.chat(&format!("bot{k}: all quiet at {}s", t / SECOND));
            }
            for m in conn.take_news() {
                if let hw_net::ToClient::Incident { id, state, .. } = m {
                    match state {
                        0 => started.insert(id),
                        1 => resolved.insert(id),
                        _ => timed_out.insert(id),
                    };
                }
            }
        }
        let before = Instant::now();
        host.tick();
        let took = before.elapsed();
        tick_time += took;
        slowest = slowest.max(took);
        if t == 3 * SECOND {
            bytes_start = bots
                .iter()
                .map(|b| {
                    b.player
                        .and_then(|p| host.traffic(p))
                        .map_or(0, |t| t.bytes_out)
                })
                .collect();
        }
        if t > 0 && t.is_multiple_of(600 * SECOND) {
            // Every ten minutes: the host still keeps up (a slow leak of
            // work shows here long before the end).
            let block = (tick_time - block_start).as_secs_f64() / (600 * SECOND) as f64;
            println!(
                "{} min: {:.0} s so far, host tick {:.2} ms mean over the last ten minutes",
                t / SECOND / 60,
                clock.elapsed().as_secs_f64(),
                block * 1000.0
            );
            assert!(
                block < hw_sim::DT,
                "the host fell behind: {:.1} ms a tick",
                block * 1000.0
            );
            block_start = tick_time;
        }
        if t.is_multiple_of(SECOND) {
            // The world stays sane.
            let all = host.sim.snapshot();
            most_entities = most_entities.max(all.len());
            for e in &all {
                let p = DVec3::from_array(e.pos);
                assert!(
                    p.is_finite() && p.length() < 1.0e6,
                    "{:?} ({:?}) at {p} after {} s",
                    e.id,
                    e.kind,
                    t / SECOND
                );
            }
            assert!(
                all.len() < 3000,
                "{} entities after {} s",
                all.len(),
                t / SECOND
            );
            assert!(
                host.sim.incidents.open.len() <= hw_sim::incidents::wanted(MAX_PLAYERS),
                "{} incidents open",
                host.sim.incidents.open.len()
            );
        }
        if t % 1000 == 0 {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    // Deliver the last updates.
    for _ in 0..10 {
        for b in &mut bots {
            if let Some(c) = b.conn.as_mut() {
                c.update(InputState::default());
                c.take_news();
            }
        }
        std::thread::sleep(Duration::from_millis(2));
        host.tick();
    }
    std::thread::sleep(Duration::from_millis(5));
    for b in &mut bots {
        if let Some(c) = b.conn.as_mut() {
            c.update(InputState::default());
        }
    }

    let seconds = ticks as f64 / SECOND as f64;
    let mean = tick_time.as_secs_f64() / ticks as f64;
    println!(
        "{minutes} game minutes in {:.0} s; host tick {:.2} ms mean, {:.1} ms slowest; \
         at most {most_entities} entities",
        clock.elapsed().as_secs_f64(),
        mean * 1000.0,
        slowest.as_secs_f64() * 1000.0,
    );
    println!(
        "incidents: {} started, {} resolved, {} timed out; taxi rides hailed: {}",
        started.len(),
        resolved.len(),
        timed_out.len(),
        bots.iter().map(|b| b.rides).sum::<u32>()
    );
    assert_eq!(host.players().len(), MAX_PLAYERS, "somebody dropped");
    assert!(
        mean < hw_sim::DT,
        "the host cannot keep up: {:.1} ms a tick",
        mean * 1000.0
    );
    let truth = host.sim.snapshot();
    for (k, b) in bots.iter().enumerate() {
        let c = b.conn.as_ref().unwrap();
        let ConnState::Connected { player, hero, .. } = c.state else {
            panic!("bot{k}: {:?}", c.state);
        };
        if k != LEAVER || !late_rejoin {
            let first = bytes_start.get(k).copied().unwrap_or(0);
            let per_second =
                (host.traffic(player).unwrap().bytes_out - first) as f64 / (seconds - 3.0);
            println!("bot{k}: {per_second:.0} bytes/s");
            assert!(
                per_second < hw_net::BANDWIDTH_BUDGET as f64,
                "bot{k} cost {per_second:.0} bytes/s"
            );
        }
        // Everything near this bot is in its copy, where the server has it.
        let me = truth.iter().find(|e| e.id == hero).unwrap();
        let at = DVec3::from_array(me.pos);
        for e in &truth {
            if DVec3::from_array(e.pos).distance(at) > INTEREST_RADIUS {
                continue;
            }
            let mine = c
                .mirror
                .get(e.id)
                .unwrap_or_else(|| panic!("bot{k} misses {:?} ({:?})", e.id, e.kind));
            // At rest: exactly where the server has it; moving: no more
            // than a few ticks behind (the update budget sends nearer
            // things first).
            let speed = glam::Vec3::from_array(e.vel).length() as f64;
            let off = DVec3::from_array(mine.pos).distance(DVec3::from_array(e.pos));
            assert!(
                off <= speed * 0.25 + 1e-9,
                "bot{k} sees {:?} ({:?}, {speed:.1} m/s) {off:.3} m off",
                e.id,
                e.kind
            );
        }
    }
    if minutes >= 4 {
        assert!(started.len() >= 3, "only {} incidents", started.len());
        assert!(!resolved.is_empty(), "the bots resolved no incident");
        assert!(
            bots.iter().map(|b| b.rides).sum::<u32>() >= 2,
            "no taxi rides"
        );
    }
}
