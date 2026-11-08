//! Heroes' looks travel with them: a joining player sends theirs in
//! pieces, the host passes it on, and a late joiner hears everyone's.

use hw_core::Seed;
use hw_net::look::{pieces, LookParts};
use hw_net::{Connection, InputState, PlayerId, ToClient, ToServer};
use hw_sim::host::Host;
use std::collections::BTreeMap;
use std::time::Duration;

/// A look as big as a hero with many sliders set.
fn look(tag: &str) -> Vec<u8> {
    let mut s = format!("{{\"name\":\"{tag}\",\"details\":{{");
    for i in 0..120 {
        s += &format!("\"head/slider-{i}\":0.{i:03},");
    }
    s += "\"end\":1}}";
    s.into_bytes()
}

/// Looks each connection has put together so far.
fn heard(
    c: &mut Connection,
    parts: &mut BTreeMap<PlayerId, LookParts>,
    got: &mut BTreeMap<PlayerId, Vec<u8>>,
) {
    for m in c.take_news() {
        if let ToClient::Look {
            player,
            part,
            parts: n,
            bytes,
        } = m
        {
            if let Some(l) = parts.entry(player).or_default().add(part, n, bytes) {
                got.insert(player, l);
            }
        }
    }
}

#[test]
fn looks_reach_everyone_and_late_joiners() {
    let mut host = Host::new(Seed(3));
    let addr = host.listen("127.0.0.1:0").expect("bind");
    let me = host.join_local("Host");
    host.set_look(me, look("host"));

    let mut a = Connection::open(addr, "Alice").expect("socket");
    let (mut a_parts, mut a_got) = (BTreeMap::new(), BTreeMap::new());
    let mut sent = false;
    for _ in 0..120 {
        a.update(InputState::default());
        if !sent && a.hero().is_some() {
            sent = true;
            for (part, parts, bytes) in pieces(&look("alice")) {
                a.send(ToServer::Look { part, parts, bytes });
            }
        }
        std::thread::sleep(Duration::from_millis(2));
        host.tick();
        heard(&mut a, &mut a_parts, &mut a_got);
    }
    assert!(look("alice").len() > 3 * hw_net::look::LOOK_PIECE);
    assert_eq!(
        a_got.get(&me),
        Some(&look("host")),
        "Alice sees the host's looks"
    );
    let pa = *host
        .players()
        .iter()
        .find(|(_, i)| i.name == "Alice")
        .unwrap()
        .0;
    assert_eq!(
        host.players()[&pa].look,
        look("alice"),
        "the host has Alice's"
    );

    // Bob comes later and hears both.
    let mut b = Connection::open(addr, "Bob").expect("socket");
    let (mut b_parts, mut b_got) = (BTreeMap::new(), BTreeMap::new());
    for _ in 0..120 {
        a.update(InputState::default());
        b.update(InputState::default());
        std::thread::sleep(Duration::from_millis(2));
        host.tick();
        heard(&mut b, &mut b_parts, &mut b_got);
    }
    assert_eq!(b_got.get(&me), Some(&look("host")));
    assert_eq!(b_got.get(&pa), Some(&look("alice")));
}
