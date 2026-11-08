//! A client's connection to a server: says hello until welcomed, then sends
//! its inputs every tick, mirrors the world from the server's updates, and
//! keeps a reliable channel both ways (joins, chat, the end of a session).

use crate::protocol::{
    ClientMsg, EntityState, InputState, PlayerId, ServerMsg, ToClient, ToServer, INPUT_REDUNDANCY,
};
use crate::reliable::{ReliableReceiver, ReliableSender};
use crate::replication::{AckWindow, Mirror};
use crate::transport::{ClientSocket, Link, TIMEOUT};
use hw_core::NetId;
use std::collections::VecDeque;
use std::net::ToSocketAddrs;
use std::time::{Duration, Instant};

/// How often an unanswered hello is repeated.
const HELLO_EVERY: Duration = Duration::from_millis(250);
/// Ticks before an unconfirmed reliable message goes out again.
const RESEND_TICKS: u64 = 6;

#[derive(Clone, Debug, PartialEq)]
pub enum ConnState {
    Connecting,
    Connected {
        player: PlayerId,
        hero: NetId,
        world_seed: u64,
        town_population: f64,
        tick_hz: u16,
    },
    Rejected(String),
    /// The host closed the world, or stopped answering.
    Ended(String),
}

pub struct Connection {
    link: Box<dyn Link>,
    name: String,
    pub state: ConnState,
    last_hello: Option<Instant>,
    last_heard: Instant,
    seq: u32,
    recent: VecDeque<InputState>,
    acks: AckWindow,
    /// The world near our hero as the server described it.
    pub mirror: Mirror,
    /// Newest update: its tick and the last input the server had applied.
    latest: Option<(u64, u32)>,
    rel_in: ReliableReceiver<ToClient>,
    rel_out: ReliableSender<ToServer>,
    news: Vec<ToClient>,
    /// Bytes received from the server so far.
    pub bytes_in: u64,
    ticks: u64,
}

impl Connection {
    /// Connects over UDP.
    pub fn open(server: impl ToSocketAddrs, name: &str) -> std::io::Result<Self> {
        Ok(Self::over(Box::new(ClientSocket::connect(server)?), name))
    }

    /// Connects over any link (the GameHub relay).
    pub fn over(link: Box<dyn Link>, name: &str) -> Self {
        Self {
            link,
            name: name.to_string(),
            state: ConnState::Connecting,
            last_hello: None,
            last_heard: Instant::now(),
            seq: 0,
            recent: VecDeque::new(),
            acks: AckWindow::default(),
            mirror: Mirror::new(),
            latest: None,
            rel_in: ReliableReceiver::new(),
            rel_out: ReliableSender::new(),
            news: Vec::new(),
            bytes_in: 0,
            ticks: 0,
        }
    }

    /// Call once per client tick with the current input. Returns the
    /// sequence number the input went out with, once connected.
    pub fn update(&mut self, input: InputState) -> Option<u32> {
        self.ticks += 1;
        for bytes in self.link.recv() {
            let Ok(msg) = ServerMsg::decode(&bytes) else {
                continue;
            };
            self.bytes_in += bytes.len() as u64;
            self.last_heard = Instant::now();
            match msg {
                ServerMsg::Welcome {
                    player,
                    hero,
                    world_seed,
                    town_population,
                    tick_hz,
                } => {
                    if self.state == ConnState::Connecting {
                        self.state = ConnState::Connected {
                            player,
                            hero,
                            world_seed,
                            town_population,
                            tick_hz,
                        };
                    }
                }
                ServerMsg::Reject { reason } => {
                    if self.state == ConnState::Connecting {
                        self.state = ConnState::Rejected(reason)
                    }
                }
                ServerMsg::Update {
                    tick,
                    ack,
                    rel_ack,
                    reliable,
                    entities,
                    removed,
                } => {
                    if !matches!(self.state, ConnState::Connected { .. }) {
                        continue;
                    }
                    self.acks.record(tick);
                    self.mirror.apply(tick, &entities, &removed);
                    if self.latest.is_none_or(|(t, _)| tick > t) {
                        self.latest = Some((tick, ack));
                    }
                    self.rel_out.ack(rel_ack);
                    for (id, m) in reliable {
                        for m in self.rel_in.receive(id, m) {
                            if let ToClient::SessionEnd { reason } = &m {
                                self.state = ConnState::Ended(reason.clone());
                            }
                            self.news.push(m);
                        }
                    }
                }
            }
        }
        match self.state {
            ConnState::Connecting => {
                if self.last_hello.is_none_or(|t| t.elapsed() >= HELLO_EVERY) {
                    self.link.send(
                        &ClientMsg::Hello {
                            name: self.name.clone(),
                        }
                        .encode(),
                    );
                    self.last_hello = Some(Instant::now());
                    self.last_heard = Instant::now();
                }
            }
            ConnState::Connected { .. } => {
                if self.last_heard.elapsed() > TIMEOUT {
                    self.state = ConnState::Ended("the host stopped answering".into());
                    return None;
                }
                self.seq = self.seq.wrapping_add(1);
                self.recent.push_back(input);
                while self.recent.len() > INPUT_REDUNDANCY {
                    self.recent.pop_front();
                }
                let msg = ClientMsg::Input {
                    seq: self.seq,
                    inputs: self.recent.iter().copied().collect(),
                    ack_tick: self.acks.newest,
                    ack_bits: self.acks.bits,
                    rel_ack: self.rel_in.ack(),
                    reliable: self.rel_out.due(self.ticks, RESEND_TICKS),
                };
                self.link.send(&msg.encode());
                return Some(self.seq);
            }
            ConnState::Rejected(_) | ConnState::Ended(_) => {}
        }
        None
    }

    /// Says something to everyone.
    pub fn chat(&mut self, text: &str) {
        self.send(ToServer::Chat { text: text.into() });
    }

    /// Sends a reliable message (a hero file, a power bought).
    pub fn send(&mut self, msg: ToServer) {
        self.rel_out.push(msg);
    }

    /// Hails a taxi to stand `dest`, or (255) gets out of it.
    pub fn hail_taxi(&mut self, dest: u8) {
        self.rel_out.push(ToServer::Taxi { dest });
    }

    /// Reliable news that arrived since the last call, in order.
    pub fn take_news(&mut self) -> Vec<ToClient> {
        std::mem::take(&mut self.news)
    }

    /// The newest world: update tick, last input the server applied, and
    /// every entity near our hero.
    pub fn world(&self) -> Option<(u64, u32, Vec<EntityState>)> {
        self.latest
            .map(|(tick, ack)| (tick, ack, self.mirror.states()))
    }

    /// Our hero's ID once connected.
    pub fn hero(&self) -> Option<NetId> {
        match self.state {
            ConnState::Connected { hero, .. } => Some(hero),
            _ => None,
        }
    }

    /// Tell the server we are leaving.
    pub fn close(&mut self) {
        self.link.send(&ClientMsg::Bye.encode());
    }
}
