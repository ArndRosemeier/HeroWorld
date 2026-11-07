//! A host with a local player, and two more clients joining over UDP on
//! this machine. Everyone sees everyone; each hero moves by its own input.

use hw_core::Seed;
use hw_net::{ConnState, Connection, InputState};
use hw_sim::host::{Host, HostEvent};
use std::time::Duration;

fn run_until(
    host: &mut Host,
    clients: &mut [(&mut Connection, InputState)],
    ticks: u32,
) -> Vec<HostEvent> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        for (c, input) in clients.iter_mut() {
            c.update(*input);
        }
        // Give the loopback a moment to deliver.
        std::thread::sleep(Duration::from_millis(2));
        events.extend(host.tick());
    }
    events
}

#[test]
fn two_clients_join_and_play_together() {
    let mut host = Host::new(Seed(5));
    let addr = host.listen("127.0.0.1:0").expect("bind");
    let me = host.join_local("Host");

    let mut a = Connection::open(addr, "Alice").expect("socket");
    let mut b = Connection::open(addr, "Bob").expect("socket");
    let east = InputState {
        move_x: 1.0,
        ..Default::default()
    };
    let south = InputState {
        move_z: 1.0,
        ..Default::default()
    };

    // Join: hellos until welcomed.
    let events = run_until(&mut host, &mut [(&mut a, east), (&mut b, south)], 30);
    let joined = events
        .iter()
        .filter(|e| matches!(e, HostEvent::Joined { .. }))
        .count();
    assert_eq!(joined, 2, "events: {events:?}");
    let (
        ConnState::Connected {
            player: pa,
            world_seed,
            ..
        },
        ConnState::Connected { player: pb, .. },
    ) = (a.state.clone(), b.state.clone())
    else {
        panic!("not connected: {:?} {:?}", a.state, b.state);
    };
    assert_eq!(world_seed, 5);
    assert!(pa != pb && pa != me && pb != me);

    let start_a = host.sim.position(a.hero().unwrap()).unwrap();
    let start_b = host.sim.position(b.hero().unwrap()).unwrap();
    run_until(&mut host, &mut [(&mut a, east), (&mut b, south)], 60);
    let end_a = host.sim.position(a.hero().unwrap()).unwrap();
    let end_b = host.sim.position(b.hero().unwrap()).unwrap();
    assert!(
        end_a.x - start_a.x > 5.0,
        "Alice ran east: {start_a} -> {end_a}"
    );
    assert!((end_a.z - start_a.z).abs() < 0.5);
    assert!(
        end_b.z - start_b.z > 5.0,
        "Bob ran south: {start_b} -> {end_b}"
    );

    // Both clients see all three heroes, with owners.
    for c in [&a, &b] {
        let (_, ack, entities) = c.latest.as_ref().expect("snapshots arrive");
        assert!(*ack > 0);
        assert_eq!(entities.len(), 3);
        let mut owners: Vec<_> = entities.iter().filter_map(|e| e.owner).collect();
        owners.sort();
        let mut expected = vec![me, pa, pb];
        expected.sort();
        assert_eq!(owners, expected);
    }

    // Bob leaves; his hero goes.
    b.close();
    let events = run_until(&mut host, &mut [(&mut a, east)], 10);
    assert!(events
        .iter()
        .any(|e| matches!(e, HostEvent::Left { player, .. } if *player == pb)));
    assert_eq!(host.sim.snapshot().len(), 2);
}

#[test]
fn an_eleventh_player_is_turned_away() {
    let mut host = Host::new(Seed(1));
    let addr = host.listen("127.0.0.1:0").expect("bind");
    for k in 0..hw_sim::host::MAX_PLAYERS {
        host.join_local(&format!("p{k}"));
    }
    let mut c = Connection::open(addr, "late").expect("socket");
    run_until(&mut host, &mut [(&mut c, InputState::default())], 20);
    assert!(matches!(c.state, ConnState::Rejected(_)), "{:?}", c.state);
}
