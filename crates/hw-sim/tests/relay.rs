//! Playing through the GameHub relay: a host opens a room, two friends join
//! it by its code and play, their packets travelling as WebSocket frames.
//! A small stand-in for GameHub runs on this machine, speaking the same
//! protocol (the real one is not reachable from test machines).
//! When the host leaves, another player takes over the world in the same
//! room.

use hw_core::Seed;
use hw_net::gamehub::{Hub, HubClientLink, HubServerLink};
use hw_net::{ConnState, Connection, InputState};
use hw_sim::host::{Host, HostEvent};
use serde_json::{json, Value};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tungstenite::{Message, WebSocket};

#[derive(Default)]
struct Room {
    code: String,
    host: u32,
    members: Vec<u32>,
    /// The host's newest state.
    state: Option<Value>,
}

#[derive(Default)]
struct FakeHub {
    next: u32,
    players: Vec<(u32, String, Sender<Message>)>,
    room: Option<Room>,
}

impl FakeHub {
    fn send(&self, to: u32, m: Message) {
        if let Some((_, _, tx)) = self.players.iter().find(|(id, _, _)| *id == to) {
            let _ = tx.send(m);
        }
    }

    fn room_json(&self) -> Value {
        let r = self.room.as_ref().unwrap();
        json!({"id": r.code, "name": "test world", "public": true, "players": r.members.len(),
               "maxPlayers": 10, "meta": {}, "host": r.host, "createdAt": 0})
    }

    fn joined(&self, you: u32) -> Message {
        let r = self.room.as_ref().unwrap();
        let players: Vec<Value> = r
            .members
            .iter()
            .map(|id| {
                let name = &self.players.iter().find(|p| p.0 == *id).unwrap().1;
                json!({"id": id, "name": name})
            })
            .collect();
        Message::text(
            json!({"t": "joined", "room": self.room_json(), "you": you, "host": r.host,
                   "players": players,
                   "state": {"seq": 0, "snapshotSeq": 0, "snapshot": r.state, "deltas": [], "complete": true}})
            .to_string(),
        )
    }
}

fn serve(stream: TcpStream, hub: Arc<Mutex<FakeHub>>) {
    let mut ws: WebSocket<TcpStream> = tungstenite::accept(stream).expect("handshake");
    ws.get_mut()
        .set_read_timeout(Some(Duration::from_millis(2)))
        .unwrap();
    let (tx, rx) = channel::<Message>();
    let me = {
        let mut h = hub.lock().unwrap();
        h.next += 1;
        let id = h.next;
        h.players.push((id, format!("p{id}"), tx.clone()));
        id
    };
    let _ = tx.send(Message::text(
        json!({"t": "welcome", "you": me, "name": "", "app": "heroworld"}).to_string(),
    ));
    loop {
        match ws.read() {
            Ok(Message::Text(t)) => {
                let m: Value = serde_json::from_str(t.as_str()).unwrap();
                let mut h = hub.lock().unwrap();
                match m["t"].as_str().unwrap() {
                    "hello" => {
                        let name = m["name"].as_str().unwrap_or("").to_string();
                        if let Some(p) = h.players.iter_mut().find(|p| p.0 == me) {
                            p.1 = name;
                        }
                    }
                    "create" => {
                        h.room = Some(Room {
                            code: "K7Q2ZP".into(),
                            host: me,
                            members: vec![me],
                            state: None,
                        });
                        let j = h.joined(me);
                        h.send(me, j);
                    }
                    "join" => {
                        let ok = h.room.as_ref().is_some_and(|r| r.code == m["id"]);
                        if !ok {
                            h.send(
                                me,
                                Message::text(
                                    json!({"t":"error","code":"no_such_room"}).to_string(),
                                ),
                            );
                            continue;
                        }
                        h.room.as_mut().unwrap().members.push(me);
                        let j = h.joined(me);
                        h.send(me, j);
                        let name = h.players.iter().find(|p| p.0 == me).unwrap().1.clone();
                        let others: Vec<u32> = h.room.as_ref().unwrap().members.clone();
                        for o in others.into_iter().filter(|o| *o != me) {
                            h.send(
                                o,
                                Message::text(
                                    json!({"t":"player+","player":{"id":me,"name":name}})
                                        .to_string(),
                                ),
                            );
                        }
                    }
                    "state" => {
                        let Some(r) = h.room.as_mut().filter(|r| r.host == me) else {
                            continue;
                        };
                        r.state = Some(m["data"].clone());
                        let others: Vec<u32> =
                            r.members.iter().copied().filter(|o| *o != me).collect();
                        let msg = json!({"t":"state","seq":1,"data":m["data"]}).to_string();
                        for o in others {
                            h.send(o, Message::text(msg.clone()));
                        }
                    }
                    "lobby" => {
                        let rooms = if h.room.is_some() {
                            vec![h.room_json()]
                        } else {
                            vec![]
                        };
                        h.send(
                            me,
                            Message::text(json!({"t":"rooms","rooms":rooms}).to_string()),
                        );
                    }
                    _ => {}
                }
            }
            Ok(Message::Binary(b)) => {
                let h = hub.lock().unwrap();
                let mut framed = me.to_be_bytes().to_vec();
                framed.extend_from_slice(&b);
                if let Some(r) = &h.room {
                    for o in r.members.iter().filter(|o| **o != me) {
                        h.send(*o, Message::binary(framed.clone()));
                    }
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => break,
        }
        while let Ok(m) = rx.try_recv() {
            if ws.write(m).is_err() {
                break;
            }
        }
        let _ = ws.flush();
    }
    let mut h = hub.lock().unwrap();
    h.players.retain(|p| p.0 != me);
    if let Some(r) = h.room.as_mut() {
        r.members.retain(|m| *m != me);
        // The host left: the player with the lowest id takes over.
        let new_host = (r.host == me)
            .then(|| r.members.iter().min().copied())
            .flatten();
        if let Some(n) = new_host {
            r.host = n;
        }
        let others = r.members.clone();
        for o in others {
            h.send(o, Message::text(json!({"t":"player-","id":me}).to_string()));
            if let Some(n) = new_host {
                h.send(o, Message::text(json!({"t":"host","id":n}).to_string()));
            }
        }
    }
}

fn fake_hub() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let hub = Arc::new(Mutex::new(FakeHub::default()));
    std::thread::spawn(move || {
        for s in listener.incoming().map_while(Result::ok) {
            let hub = hub.clone();
            std::thread::spawn(move || serve(s, hub));
        }
    });
    url
}

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
        std::thread::sleep(Duration::from_millis(8));
        events.extend(host.tick());
    }
    events
}

/// Ticks like `run` until `done` holds for the events so far and the
/// clients, so a slow machine waits longer instead of failing; gives up
/// after `max` ticks (several seconds).
fn run_until(
    host: &mut Host,
    clients: &mut [&mut Connection],
    input: InputState,
    max: u32,
    done: impl Fn(&[HostEvent], &[&mut Connection]) -> bool,
) -> Vec<HostEvent> {
    let mut events = Vec::new();
    for _ in 0..max {
        events.extend(run(host, clients, input, 1));
        if done(&events, clients) {
            break;
        }
    }
    events
}

#[test]
fn friends_join_by_room_code_and_play_through_the_relay() {
    let url = fake_hub();
    let mut host = Host::new(Seed(5));
    let me = host.join_local("Host");
    let mut hub = Hub::connect(&url, "Host").expect("hub");
    let code = hub
        .host_room("Host's world", json!({ "seed": 5 }))
        .expect("room");
    assert_eq!(code, "K7Q2ZP");
    host.add_link(Box::new(HubServerLink::new(hub)));

    let join = |name: &str| {
        let mut hub = Hub::connect(&url, name).expect("hub");
        let room = hub.join_room(&code).expect("join");
        assert_eq!(room.id, code);
        Connection::over(Box::new(HubClientLink::new(hub)), name)
    };
    let mut a = join("Alice");
    let mut b = join("Bob");
    let east = InputState {
        move_x: 1.0,
        ..Default::default()
    };
    // Over WebSockets the joins take a varying number of ticks (a busy CI
    // machine is slow): wait for both, not for a fixed count.
    let events = run_until(&mut host, &mut [&mut a, &mut b], east, 1000, |ev, cs| {
        let joined = ev
            .iter()
            .filter(|e| matches!(e, HostEvent::Joined { .. }))
            .count();
        joined == 2
            && cs
                .iter()
                .all(|c| matches!(c.state, ConnState::Connected { .. }))
    });
    let joined: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            HostEvent::Joined { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(joined.len(), 2, "{events:?}");
    assert!(matches!(
        a.state,
        ConnState::Connected { world_seed: 5, .. }
    ));
    let start = host.sim.position(a.hero().unwrap()).unwrap();
    run(&mut host, &mut [&mut a, &mut b], east, 60);
    let end = host.sim.position(a.hero().unwrap()).unwrap();
    assert!(end.x - start.x > 4.0, "Alice ran east: {start} -> {end}");
    for c in [&a, &b] {
        let (_, _, entities) = c.world().expect("updates arrive");
        assert_eq!(entities.len(), 3);
    }
    let mine = host.sim.hero_of(me).unwrap();
    assert!(b.mirror.get(mine).is_some());

    // Bob closes the game: the relay says he left, his hero goes.
    drop(b);
    let events = run_until(
        &mut host,
        &mut [&mut a],
        InputState::default(),
        1000,
        |ev, _| {
            ev.iter()
                .any(|e| matches!(e, HostEvent::Left { name, .. } if name == "Bob"))
        },
    );
    assert!(events
        .iter()
        .any(|e| matches!(e, HostEvent::Left { name, .. } if name == "Bob")));
    assert_eq!(host.players().len(), 2);
}

#[test]
fn a_wrong_code_says_so() {
    let url = fake_hub();
    let mut hub = Hub::connect(&url, "Lost").expect("hub");
    let err = hub.join_room("NOPE00").unwrap_err();
    assert!(err.to_string().contains("NOPE00"), "{err}");
    assert!(hub.rooms().unwrap().is_empty());
}

#[test]
fn when_the_host_leaves_a_friend_takes_the_world_over() {
    use hw_sim::host::PUBLISH_TICKS;
    use hw_sim::Sim;
    let url = fake_hub();
    let mut host = Host::new(Seed(5));
    host.join_local("Host");
    let mut hub = Hub::connect(&url, "Host").expect("hub");
    let code = hub
        .host_room("Host's world", json!({ "seed": 5 }))
        .expect("room");
    host.add_link(Box::new(HubServerLink::new(hub)));
    let join = |name: &str| {
        let mut hub = Hub::connect(&url, name).expect("hub");
        hub.join_room(&code).expect("join");
        Connection::over(Box::new(HubClientLink::new(hub)), name)
    };
    let mut a = join("Alice");
    let mut b = join("Bob");
    // Bob runs east a while, then stands; the host publishes the world.
    let east = InputState {
        move_x: 1.0,
        ..Default::default()
    };
    run(&mut host, &mut [&mut a, &mut b], InputState::default(), 30);
    for _ in 0..40 {
        a.update(InputState::default());
        b.update(east);
        std::thread::sleep(Duration::from_millis(8));
        host.tick();
    }
    let ticks = PUBLISH_TICKS - host.sim.tick % PUBLISH_TICKS + 20;
    run(
        &mut host,
        &mut [&mut a, &mut b],
        InputState::default(),
        ticks as u32,
    );
    let bob_was = host.sim.position(b.hero().unwrap()).unwrap();
    let tick_was = host.sim.tick;
    assert!(b.world_state().is_some_and(|s| s.contains("hero")));

    // The host closes the game (as the game does on quitting).
    host.end_session("the host closed the game", Duration::from_millis(300));
    drop(host);

    // Alice joined first: GameHub makes her the host.
    let mut new_host = None;
    for _ in 0..300 {
        a.update(InputState::default());
        b.update(InputState::default());
        if new_host.is_none() && a.state == ConnState::TakingOver {
            let state = a.world_state().map(str::to_string);
            let hub = a.take_hub().expect("the hub");
            let mut h = Host::taking_over(Sim::new(Seed(5)), state.as_deref());
            h.add_link(Box::new(HubServerLink::new(hub)));
            h.join_local("Alice");
            new_host = Some(h);
        }
        if let Some(h) = new_host.as_mut() {
            h.tick();
        }
        std::thread::sleep(Duration::from_millis(8));
        if b.handovers == 1 && matches!(b.state, ConnState::Connected { .. }) && b.world().is_some()
        {
            break;
        }
    }
    let mut h =
        new_host.unwrap_or_else(|| panic!("nobody took over: {:?} / {:?}", a.state, b.state));
    assert!(
        matches!(b.state, ConnState::Connected { world_seed: 5, .. }),
        "{:?}",
        b.state
    );
    assert_eq!(b.handovers, 1);
    // Bob is back where he was, in the world as it was.
    let bob = h.sim.position(b.hero().unwrap()).unwrap();
    assert!(bob.distance(bob_was) < 1.0, "{bob_was} -> {bob}");
    assert!(h.sim.tick >= tick_was - PUBLISH_TICKS);
    assert_eq!(h.players().len(), 2);
}
