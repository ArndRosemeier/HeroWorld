//! The netcode over real UDP sockets on this machine: what a client is sent
//! (only changes, only what is near), reliable news, and a clean end.

use glam::DVec3;
use hw_core::Seed;
use hw_net::{ConnState, Connection, InputState, ToClient};
use hw_sim::collision::Body;
use hw_sim::host::{Host, HostEvent, INTEREST_RADIUS};
use std::time::Duration;

fn run(
    host: &mut Host,
    clients: &mut [&mut Connection],
    input: InputState,
    ticks: u32,
) -> Vec<HostEvent> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        for c in clients.iter_mut() {
            c.update(input);
        }
        std::thread::sleep(Duration::from_millis(1));
        events.extend(host.tick());
    }
    events
}

fn joined_host() -> (Host, Connection, hw_net::PlayerId) {
    let mut host = Host::new(Seed(5));
    let addr = host.listen("127.0.0.1:0").expect("bind");
    let me = host.join_local("Host");
    let mut c = Connection::open(addr, "Alice").expect("socket");
    run(&mut host, &mut [&mut c], InputState::default(), 20);
    assert!(
        matches!(c.state, ConnState::Connected { .. }),
        "{:?}",
        c.state
    );
    (host, c, me)
}

#[test]
fn heroes_standing_still_cost_almost_nothing() {
    let (mut host, mut c, _) = joined_host();
    // Let everyone land and the client confirm what it has.
    run(&mut host, &mut [&mut c], InputState::default(), 60);
    let player = match c.state {
        ConnState::Connected { player, .. } => player,
        _ => unreachable!(),
    };
    let before = host.traffic(player).unwrap();
    run(&mut host, &mut [&mut c], InputState::default(), 90);
    let after = host.traffic(player).unwrap();
    let per_packet = (after.bytes_out - before.bytes_out) as f64
        / (after.packets_out - before.packets_out) as f64;
    // Only the header: nothing moved.
    assert!(
        per_packet <= 40.0,
        "{per_packet} bytes per update while idle"
    );
    assert_eq!(c.mirror.len(), 2);

    // Running costs a hero's state per tick, still far under the budget.
    let east = InputState {
        move_x: 1.0,
        ..Default::default()
    };
    let before = host.traffic(player).unwrap();
    run(&mut host, &mut [&mut c], east, 90);
    let after = host.traffic(player).unwrap();
    let per_second = (after.bytes_out - before.bytes_out) as f64 / 3.0;
    assert!(per_second > 500.0 && per_second < hw_net::BANDWIDTH_BUDGET as f64);
}

#[test]
fn far_away_heroes_leave_the_client_and_come_back() {
    let (mut host, mut c, me) = joined_host();
    let my_hero = host.sim.hero_of(me).unwrap();
    run(&mut host, &mut [&mut c], InputState::default(), 10);
    assert!(c.mirror.get(my_hero).is_some());
    let home = host.sim.body(my_hero).unwrap();

    // The host's hero flies far away: Alice's client forgets it...
    let far = Body {
        pos: home.pos + DVec3::new(INTEREST_RADIUS * 4.0, 300.0, 0.0),
        ..home
    };
    host.sim.set_body(my_hero, far);
    host.sim.set_input(
        me,
        InputState {
            fly: true,
            ..Default::default()
        },
    );
    run(&mut host, &mut [&mut c], InputState::default(), 15);
    assert!(c.mirror.get(my_hero).is_none());
    assert_eq!(host.sim.bubbles.len(), 2, "two bubbles while apart");

    // ...and hears of it again when it returns.
    host.sim.set_body(my_hero, home);
    host.sim.set_input(me, InputState::default());
    run(&mut host, &mut [&mut c], InputState::default(), 15);
    let back = c.mirror.get(my_hero).expect("back in range");
    assert!(DVec3::from_array(back.pos).distance(home.pos) < 5.0);
    assert_eq!(host.sim.bubbles.len(), 1, "one bubble together");
}

#[test]
fn news_arrives_reliably_and_the_host_ends_the_session_cleanly() {
    let (mut host, mut c, me) = joined_host();
    let mut news = c.take_news();
    // Alice hears who was there and that she joined.
    assert!(news
        .iter()
        .any(|n| matches!(n, ToClient::Joined { name, .. } if name == "Host")));
    assert!(news
        .iter()
        .any(|n| matches!(n, ToClient::Joined { name, .. } if name == "Alice")));

    c.chat("hello, world");
    host.chat(me, "welcome!");
    let events = run(&mut host, &mut [&mut c], InputState::default(), 20);
    assert!(events
        .iter()
        .any(|e| matches!(e, HostEvent::Chat { text, name, .. } if text == "hello, world" && name == "Alice")));
    news = c.take_news();
    let chats: Vec<&str> = news
        .iter()
        .filter_map(|n| match n {
            ToClient::Chat { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    // The host's line went out at once, Alice's after it came round.
    assert_eq!(chats, vec!["welcome!", "hello, world"]);

    // The Team channel: a line from Alice reaches the host as an event and
    // a line from the host reaches Alice as team news, apart from Local.
    c.team_chat("meet at the tower");
    host.team_chat(me, "coming");
    let events = run(&mut host, &mut [&mut c], InputState::default(), 20);
    assert!(events.iter().any(|e| matches!(e, HostEvent::TeamChat { text, name, .. } if text == "meet at the tower" && name == "Alice")));
    let team: Vec<String> = c
        .take_news()
        .iter()
        .filter_map(|n| match n {
            ToClient::TeamChat { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(team, vec!["coming", "meet at the tower"]);

    // The host closes: Alice is told why, in time.
    let addr_client = std::thread::scope(|s| {
        let h = s.spawn(|| {
            for _ in 0..200 {
                c.update(InputState::default());
                if matches!(c.state, ConnState::Ended(_)) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            c
        });
        host.end_session("the host went to bed", Duration::from_secs(1));
        h.join().unwrap()
    });
    assert_eq!(
        addr_client.state,
        ConnState::Ended("the host went to bed".into())
    );
    assert_eq!(host.players().len(), 1);
}
