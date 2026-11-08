//! GameHub (games.futuremagic.de): the lobby where hosts list their worlds,
//! and a relay for friends who cannot reach the host directly.
//!
//! A host opens a public room; its 6-letter code is what friends type to
//! join. GameHub forwards binary frames between the players of a room, so
//! the game's own packets travel through it unchanged: no port forwarding,
//! no NAT trouble. GameHub allows 60 messages a second per player, so the
//! host bundles all its packets of one tick into one frame
//! ([`HubServerLink`]); clients send one frame per tick ([`HubClientLink`]).
//!
//! The protocol is GameHub's (`docs/PROTOCOL.md` in ArndRosemeier/gameserver):
//! JSON text frames for the lobby, binary frames prefixed with the sender's
//! player ID for everything else. The WebSocket runs on a thread of its own;
//! the game polls it once per tick like the UDP socket.

use crate::transport::{Link, PeerAddr, ServerLink};
use serde_json::{json, Value};
use std::io::ErrorKind;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

/// GameHub's address.
pub const GAMEHUB_URL: &str = "wss://games.futuremagic.de";
/// HeroWorld's app on GameHub.
pub const APP: &str = "heroworld";
/// The app key. Not a secret: it only says which game is connecting
/// (Arnd, 2026-11-07); GameHub's limits protect the server.
pub const APP_KEY: &str = "ghk_heroworld_b7LnXkaCjSeenAQHb_jMwWwHeQVCm7vo";

/// The GameHub URL to use: `HW_GAMEHUB_URL` if set (tests, a local hub).
pub fn hub_url() -> String {
    std::env::var("HW_GAMEHUB_URL").unwrap_or_else(|_| GAMEHUB_URL.to_string())
}

/// A room as the lobby lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct RoomInfo {
    /// The 6-character code.
    pub id: String,
    pub name: String,
    pub players: u32,
    pub max_players: u32,
    pub host: u32,
    pub meta: Value,
}

impl RoomInfo {
    fn from_json(v: &Value) -> Option<Self> {
        Some(Self {
            id: v.get("id")?.as_str()?.to_string(),
            name: v
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            players: v.get("players").and_then(Value::as_u64).unwrap_or(0) as u32,
            max_players: v.get("maxPlayers").and_then(Value::as_u64).unwrap_or(0) as u32,
            host: v.get("host").and_then(Value::as_u64).unwrap_or(0) as u32,
            meta: v.get("meta").cloned().unwrap_or(Value::Null),
        })
    }
}

/// Something GameHub told us.
#[derive(Clone, Debug, PartialEq)]
pub enum HubEvent {
    Welcome { you: u32 },
    Joined { room: RoomInfo, you: u32, host: u32 },
    PlayerJoined { id: u32, name: String },
    PlayerLeft { id: u32 },
    Host { id: u32 },
    Rooms(Vec<RoomInfo>),
    RoomAdded(RoomInfo),
    RoomRemoved(String),
    Binary { from: u32, bytes: Vec<u8> },
    Error(String),
    Closed(String),
}

fn parse(text: &str) -> Option<HubEvent> {
    let m: Value = serde_json::from_str(text).ok()?;
    let id = |k: &str| m.get(k).and_then(Value::as_u64).map(|v| v as u32);
    Some(match m.get("t")?.as_str()? {
        "welcome" => HubEvent::Welcome { you: id("you")? },
        "joined" => HubEvent::Joined {
            room: RoomInfo::from_json(m.get("room")?)?,
            you: id("you")?,
            host: id("host")?,
        },
        "player+" => {
            let p = m.get("player")?;
            HubEvent::PlayerJoined {
                id: p.get("id")?.as_u64()? as u32,
                name: p.get("name").and_then(Value::as_str).unwrap_or("").into(),
            }
        }
        "player-" => HubEvent::PlayerLeft { id: id("id")? },
        "host" => HubEvent::Host { id: id("id")? },
        "rooms" => HubEvent::Rooms(
            m.get("rooms")?
                .as_array()?
                .iter()
                .filter_map(RoomInfo::from_json)
                .collect(),
        ),
        "room+" => HubEvent::RoomAdded(RoomInfo::from_json(m.get("room")?)?),
        "room-" => HubEvent::RoomRemoved(m.get("id")?.as_str()?.to_string()),
        "error" => HubEvent::Error(m.get("code")?.as_str()?.to_string()),
        _ => return None,
    })
}

enum Out {
    Text(String),
    Binary(Vec<u8>),
    Close,
}

/// A connection to GameHub.
pub struct Hub {
    tx: Sender<Out>,
    rx: Receiver<HubEvent>,
    pub you: Option<u32>,
    pub host: Option<u32>,
    pub room: Option<RoomInfo>,
    pub closed: Option<String>,
    /// Events read by [`Self::wait_for`] that the caller has not taken yet.
    held: Vec<HubEvent>,
}

fn set_timeout(ws: &mut WebSocket<MaybeTlsStream<TcpStream>>, t: Duration) {
    let tcp = match ws.get_mut() {
        MaybeTlsStream::Plain(s) => s,
        MaybeTlsStream::Rustls(s) => s.get_mut(),
        _ => return,
    };
    let _ = tcp.set_read_timeout(Some(t));
}

impl Hub {
    /// Connects to the hub at `url` (`wss://…` or `ws://…`) as `name`.
    pub fn connect(url: &str, name: &str) -> std::io::Result<Self> {
        let url = format!(
            "{}/v1/apps/{APP}/ws?key={APP_KEY}",
            url.trim_end_matches('/')
        );
        // Connect with a time limit, so a blocked network cannot hang the game.
        let rest = url
            .strip_prefix("wss://")
            .map(|r| (r, 443))
            .or_else(|| url.strip_prefix("ws://").map(|r| (r, 80)));
        let Some((rest, port)) = rest else {
            return Err(std::io::Error::other(format!("not a WebSocket URL: {url}")));
        };
        let authority = rest.split('/').next().unwrap_or(rest);
        let target = if authority.contains(':') {
            authority.to_string()
        } else {
            format!("{authority}:{port}")
        };
        let addr = target
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| std::io::Error::other(format!("cannot find {target}")))?;
        let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(5))?;
        tcp.set_read_timeout(Some(Duration::from_secs(10)))?;
        let (mut ws, _) = tungstenite::client_tls(url.as_str(), tcp)
            .map_err(|e| std::io::Error::other(format!("cannot reach GameHub: {e}")))?;
        set_timeout(&mut ws, Duration::from_millis(5));
        let (tx, commands) = channel::<Out>();
        let (events, rx) = channel::<HubEvent>();
        std::thread::Builder::new()
            .name("gamehub".into())
            .spawn(move || run(ws, commands, events))?;
        let hub = Self {
            tx,
            rx,
            you: None,
            host: None,
            room: None,
            closed: None,
            held: Vec::new(),
        };
        hub.text(json!({ "t": "hello", "name": name }));
        Ok(hub)
    }

    fn text(&self, v: Value) {
        let _ = self.tx.send(Out::Text(v.to_string()));
    }

    /// Opens a public room and becomes its host.
    pub fn create(&self, name: &str, max_players: u32, meta: Value) {
        self.text(json!({
            "t": "create",
            "room": { "name": name, "public": true, "maxPlayers": max_players, "meta": meta },
        }));
    }

    /// Joins the room with this code.
    pub fn join(&self, code: &str) {
        self.text(json!({ "t": "join", "id": code.trim().to_uppercase() }));
    }

    /// Starts (or stops) the live list of public rooms.
    pub fn watch_lobby(&self, on: bool) {
        self.text(json!({ "t": "lobby", "on": on }));
    }

    /// Sends bytes to everyone else in the room.
    pub fn send_binary(&self, bytes: Vec<u8>) {
        let _ = self.tx.send(Out::Binary(bytes));
    }

    /// Everything that arrived since the last call.
    pub fn poll(&mut self) -> Vec<HubEvent> {
        let mut out = std::mem::take(&mut self.held);
        loop {
            match self.rx.try_recv() {
                Ok(e) => out.push(e),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if self.closed.is_none() {
                        out.push(HubEvent::Closed("the GameHub connection ended".into()));
                    }
                    break;
                }
            }
        }
        for e in &out {
            match e {
                HubEvent::Welcome { you } => self.you = Some(*you),
                HubEvent::Joined { room, you, host } => {
                    self.you = Some(*you);
                    self.host = Some(*host);
                    self.room = Some(room.clone());
                }
                HubEvent::Host { id } => self.host = Some(*id),
                HubEvent::Closed(why) => self.closed = Some(why.clone()),
                _ => {}
            }
        }
        out
    }

    /// Polls until `pick` finds what it wants (or `timeout`); the other
    /// events stay for the next [`Self::poll`].
    pub fn wait_for<T>(
        &mut self,
        timeout: Duration,
        mut pick: impl FnMut(&HubEvent) -> Option<T>,
    ) -> Option<T> {
        let start = Instant::now();
        let mut keep = Vec::new();
        let mut found = None;
        while found.is_none() && start.elapsed() < timeout {
            for e in self.poll() {
                if found.is_none() {
                    if let Some(t) = pick(&e) {
                        found = Some(t);
                        continue;
                    }
                }
                keep.push(e);
            }
            if self.closed.is_some() {
                break;
            }
            if found.is_none() {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        self.held = keep;
        found
    }

    /// Hosts a world: opens a room and waits for GameHub to confirm it.
    /// Returns the room code.
    pub fn host_room(&mut self, name: &str, meta: Value) -> std::io::Result<String> {
        self.create(name, crate::MAX_PLAYERS as u32, meta);
        let got = self.wait_for(Duration::from_secs(10), |e| match e {
            HubEvent::Joined { room, .. } => Some(Ok(room.id.clone())),
            HubEvent::Error(code) => Some(Err(code.clone())),
            _ => None,
        });
        match got {
            Some(Ok(code)) => Ok(code),
            Some(Err(code)) => Err(std::io::Error::other(format!("GameHub said {code}"))),
            None => Err(std::io::Error::other("GameHub did not answer")),
        }
    }

    /// Joins a friend's room and waits until we are in. Returns the room.
    pub fn join_room(&mut self, code: &str) -> std::io::Result<RoomInfo> {
        self.join(code);
        let got = self.wait_for(Duration::from_secs(10), |e| match e {
            HubEvent::Joined { room, .. } => Some(Ok(room.clone())),
            HubEvent::Error(code) => Some(Err(code.clone())),
            _ => None,
        });
        match got {
            Some(Ok(room)) => Ok(room),
            Some(Err(c)) if c == "no_such_room" => Err(std::io::Error::other(format!(
                "there is no world with the code {code}"
            ))),
            Some(Err(c)) => Err(std::io::Error::other(format!("GameHub said {c}"))),
            None => Err(std::io::Error::other("GameHub did not answer")),
        }
    }

    /// The public rooms right now.
    pub fn rooms(&mut self) -> std::io::Result<Vec<RoomInfo>> {
        self.watch_lobby(true);
        let got = self.wait_for(Duration::from_secs(10), |e| match e {
            HubEvent::Rooms(r) => Some(r.clone()),
            _ => None,
        });
        self.watch_lobby(false);
        got.ok_or_else(|| std::io::Error::other("GameHub did not answer"))
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        let _ = self.tx.send(Out::Close);
    }
}

fn run(
    mut ws: WebSocket<MaybeTlsStream<TcpStream>>,
    commands: Receiver<Out>,
    events: Sender<HubEvent>,
) {
    loop {
        match ws.read() {
            Ok(Message::Text(t)) => {
                if let Some(e) = parse(t.as_str()) {
                    if events.send(e).is_err() {
                        return;
                    }
                }
            }
            Ok(Message::Binary(b)) if b.len() >= 4 => {
                let from = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
                let e = HubEvent::Binary {
                    from,
                    bytes: b[4..].to_vec(),
                };
                if events.send(e).is_err() {
                    return;
                }
            }
            Ok(Message::Close(why)) => {
                let reason = why.map(|f| f.reason.to_string()).unwrap_or_default();
                let _ = events.send(HubEvent::Closed(format!("GameHub closed: {reason}")));
                return;
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(e) => {
                let _ = events.send(HubEvent::Closed(format!("GameHub connection lost: {e}")));
                return;
            }
        }
        loop {
            let msg = match commands.try_recv() {
                Ok(Out::Text(t)) => Message::text(t),
                Ok(Out::Binary(b)) => Message::binary(b),
                Ok(Out::Close) | Err(TryRecvError::Disconnected) => {
                    let _ = ws.close(None);
                    let _ = ws.flush();
                    return;
                }
                Err(TryRecvError::Empty) => break,
            };
            if ws.write(msg).is_err() {
                let _ = events.send(HubEvent::Closed("GameHub connection lost".into()));
                return;
            }
        }
        let _ = ws.flush();
    }
}

/// Frame kinds inside GameHub binary frames.
const TO_CLIENTS: u8 = 1;
const TO_HOST: u8 = 2;

/// The host's side of the relay: players in its room become peers
/// ([`PeerAddr::Hub`]) of its [`crate::ServerSocket`].
pub struct HubServerLink {
    hub: Hub,
    batch: Vec<(u32, Vec<u8>)>,
}

impl HubServerLink {
    pub fn new(hub: Hub) -> Self {
        Self {
            hub,
            batch: Vec::new(),
        }
    }

    /// The room code friends type to join.
    pub fn code(&self) -> Option<&str> {
        self.hub.room.as_ref().map(|r| r.id.as_str())
    }
}

impl ServerLink for HubServerLink {
    fn send(&mut self, to: PeerAddr, bytes: &[u8]) {
        if let PeerAddr::Hub(id) = to {
            self.batch.push((id, bytes.to_vec()));
        }
    }

    fn flush(&mut self) {
        if self.batch.is_empty() {
            return;
        }
        let mut frame = vec![TO_CLIENTS];
        frame.extend_from_slice(&(self.batch.len() as u16).to_le_bytes());
        for (id, bytes) in self.batch.drain(..) {
            frame.extend_from_slice(&id.to_le_bytes());
            frame.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
            frame.extend_from_slice(&bytes);
        }
        self.hub.send_binary(frame);
    }

    fn recv(&mut self) -> Vec<(PeerAddr, Vec<u8>)> {
        let mut out = Vec::new();
        for e in self.hub.poll() {
            match e {
                HubEvent::Binary { from, bytes } if bytes.first() == Some(&TO_HOST) => {
                    out.push((PeerAddr::Hub(from), bytes[1..].to_vec()));
                }
                // Someone left the room: as good as a goodbye.
                HubEvent::PlayerLeft { id } => {
                    out.push((PeerAddr::Hub(id), crate::ClientMsg::Bye.encode()))
                }
                _ => {}
            }
        }
        out
    }
}

/// A player's side of the relay: packets to and from the room's host.
pub struct HubClientLink {
    hub: Hub,
}

impl HubClientLink {
    /// Over a hub that has joined a room.
    pub fn new(hub: Hub) -> Self {
        Self { hub }
    }
}

impl Link for HubClientLink {
    fn send(&mut self, bytes: &[u8]) {
        let mut frame = Vec::with_capacity(bytes.len() + 1);
        frame.push(TO_HOST);
        frame.extend_from_slice(bytes);
        self.hub.send_binary(frame);
    }

    fn recv(&mut self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let events = self.hub.poll();
        let (Some(me), host) = (self.hub.you, self.hub.host) else {
            return out;
        };
        for e in events {
            let HubEvent::Binary { from, bytes } = e else {
                continue;
            };
            if Some(from) != host || bytes.first() != Some(&TO_CLIENTS) {
                continue;
            }
            out.extend(unbundle(&bytes[1..], me));
        }
        out
    }
}

/// The packets for player `me` in a host's bundle.
fn unbundle(mut b: &[u8], me: u32) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let Some(n) = b.get(..2).map(|x| u16::from_le_bytes([x[0], x[1]])) else {
        return out;
    };
    b = &b[2..];
    for _ in 0..n {
        if b.len() < 6 {
            break;
        }
        let to = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        let len = u16::from_le_bytes([b[4], b[5]]) as usize;
        let Some(bytes) = b.get(6..6 + len) else {
            break;
        };
        if to == me {
            out.push(bytes.to_vec());
        }
        b = &b[6 + len..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_gamehub_messages() {
        let joined = r#"{"t":"joined","room":{"id":"K7Q2ZP","name":"Arnd's world","public":true,
            "players":1,"maxPlayers":10,"meta":{"seed":1},"host":4,"createdAt":0},
            "you":4,"host":4,"players":[{"id":4,"name":"Arnd"}],
            "state":{"seq":0,"snapshotSeq":0,"snapshot":null,"deltas":[],"complete":true}}"#;
        let Some(HubEvent::Joined { room, you, host }) = parse(joined) else {
            panic!()
        };
        assert_eq!((room.id.as_str(), you, host), ("K7Q2ZP", 4, 4));
        assert_eq!(room.max_players, 10);
        assert_eq!(room.meta["seed"], 1);
        assert_eq!(
            parse(r#"{"t":"player+","player":{"id":7,"name":"Bob"}}"#),
            Some(HubEvent::PlayerJoined {
                id: 7,
                name: "Bob".into()
            })
        );
        assert_eq!(
            parse(r#"{"t":"error","code":"room_full","req":3}"#),
            Some(HubEvent::Error("room_full".into()))
        );
        assert_eq!(parse("not json"), None);
    }

    #[test]
    fn bundles_carry_each_player_their_own_packets() {
        let mut frame = vec![2u8, 0];
        for (to, bytes) in [(5u32, &b"abc"[..]), (6, b"de"), (5, b"f")] {
            frame.extend_from_slice(&to.to_le_bytes());
            frame.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
            frame.extend_from_slice(bytes);
        }
        frame[0] = 3;
        assert_eq!(unbundle(&frame, 5), vec![b"abc".to_vec(), b"f".to_vec()]);
        assert_eq!(unbundle(&frame, 6), vec![b"de".to_vec()]);
        assert!(unbundle(&frame[..5], 5).is_empty());
    }
}
