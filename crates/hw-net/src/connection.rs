//! A client's connection to a server: says hello until welcomed, then sends
//! its inputs every tick, mirrors the world from the server's updates, and
//! keeps a reliable channel both ways (joins, chat, the end of a session).
//!
//! Over the GameHub relay the session outlives its host: when the host
//! leaves, the connection says hello to the player who took over (and
//! sends our hero file and looks again), or, if that is us, stops in
//! [`ConnState::TakingOver`] for the game to start a server from the
//! room's world save.

use crate::protocol::{
    ClientMsg, EntityState, InputState, PlayerId, ServerMsg, ToClient, ToServer, INPUT_REDUNDANCY,
};
use crate::reliable::{ReliableReceiver, ReliableSender};
use crate::replication::{AckWindow, Mirror};
use crate::transport::{ClientSocket, Handover, Link, TIMEOUT};
use hw_core::NetId;
use std::collections::VecDeque;
use std::net::ToSocketAddrs;
use std::time::{Duration, Instant};

/// How long to wait for another player to take over after the host left.
const HANDOVER_WAIT: Duration = Duration::from_secs(12);
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
    /// The host left and we are the new one: the game starts a server from
    /// [`Connection::world_state`] over [`Connection::take_hub`].
    TakingOver,
}

pub struct Connection {
    link: Box<dyn Link>,
    /// Team voice to send and what was heard (see [`crate::voice`]).
    pub voice: crate::VoiceBox,
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
    /// The host said goodbye; waiting (since then) for another to take over.
    host_gone: Option<Instant>,
    /// How many times the host changed (a new host means new IDs).
    pub handovers: u32,
    /// Our hero file and looks, to send again to a new host.
    sheet: Option<ToServer>,
    looks: Vec<ToServer>,
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
            voice: crate::VoiceBox::new(),
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
            host_gone: None,
            handovers: 0,
            sheet: None,
            looks: Vec::new(),
        }
    }

    /// Starts over with a new host: hello, then our hero file and looks.
    fn rejoin(&mut self) {
        self.state = ConnState::Connecting;
        self.last_hello = None;
        self.last_heard = Instant::now();
        self.recent.clear();
        self.acks = AckWindow::default();
        self.mirror = Mirror::new();
        self.latest = None;
        self.rel_in = ReliableReceiver::new();
        self.rel_out = ReliableSender::new();
        self.host_gone = None;
        self.handovers += 1;
        let again: Vec<ToServer> = self.sheet.iter().chain(&self.looks).cloned().collect();
        for m in again {
            self.rel_out.push(m);
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
                ServerMsg::Voice { from, seq, frames } => {
                    if matches!(self.state, ConnState::Connected { .. }) {
                        self.voice.hear(crate::VoicePacket { from, seq, frames });
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
                                if self.link.can_hand_over() {
                                    // Someone else takes over (or nobody
                                    // does, and it ends after all).
                                    self.host_gone.get_or_insert_with(Instant::now);
                                } else {
                                    self.state = ConnState::Ended(reason.clone());
                                }
                            }
                            if let ToClient::Sheet(s) = &m {
                                self.sheet = Some(ToServer::Sheet(s.clone()));
                            }
                            self.news.push(m);
                        }
                    }
                }
            }
        }
        match self.link.handover() {
            Some(Handover::ToOther) if self.state != ConnState::TakingOver => self.rejoin(),
            Some(Handover::ToUs) => {
                self.state = ConnState::TakingOver;
                self.handovers += 1;
                return None;
            }
            _ => {}
        }
        if self.host_gone.is_some_and(|t| t.elapsed() > HANDOVER_WAIT) {
            self.state = ConnState::Ended("the host left and nobody took over".into());
            self.host_gone = None;
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
                if self.host_gone.is_none() && self.last_heard.elapsed() > TIMEOUT {
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
                for (seq, frames) in self.voice.take_outgoing() {
                    self.link.send(&ClientMsg::Voice { seq, frames }.encode());
                }
                return Some(self.seq);
            }
            ConnState::Rejected(_) | ConnState::Ended(_) | ConnState::TakingOver => {}
        }
        None
    }

    /// Says something to everyone.
    pub fn chat(&mut self, text: &str) {
        self.send(ToServer::Chat { text: text.into() });
    }

    /// Says something on the Team channel: every player of the session
    /// hears it, wherever they are. (A bot calls this the same way.)
    pub fn team_chat(&mut self, text: &str) {
        self.send(ToServer::TeamChat { text: text.into() });
    }

    /// Sends a reliable message (a hero file, a power bought).
    pub fn send(&mut self, msg: ToServer) {
        match &msg {
            ToServer::Sheet(_) => self.sheet = Some(msg.clone()),
            ToServer::Look { part: 0, .. } => self.looks = vec![msg.clone()],
            ToServer::Look { .. } => self.looks.push(msg.clone()),
            _ => {}
        }
        self.rel_out.push(msg);
    }

    /// The newest world save the host published (to take over from).
    pub fn world_state(&self) -> Option<&str> {
        self.link.world_state()
    }

    /// The GameHub connection, once we are [`ConnState::TakingOver`].
    pub fn take_hub(&mut self) -> Option<crate::gamehub::Hub> {
        self.link.take_hub()
    }

    /// Hails a taxi to stand `dest`, or (255) gets out of it.
    pub fn hail_taxi(&mut self, dest: u8) {
        self.rel_out.push(ToServer::Taxi { dest });
    }

    /// Rides the Aether Tube to station `dest`, or (65535) gets out.
    pub fn ride_tube(&mut self, dest: u16) {
        self.rel_out.push(ToServer::Tube { dest });
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
