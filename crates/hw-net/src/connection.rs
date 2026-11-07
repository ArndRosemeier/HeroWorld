//! A client's connection to a server: says hello until welcomed, then sends
//! input every tick and keeps the newest snapshot.

use crate::protocol::{ClientMsg, EntityState, InputState, PlayerId, ServerMsg};
use crate::transport::ClientSocket;
use hw_core::NetId;
use std::net::ToSocketAddrs;
use std::time::{Duration, Instant};

/// How often an unanswered hello is repeated.
const HELLO_EVERY: Duration = Duration::from_millis(250);

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
}

pub struct Connection {
    socket: ClientSocket,
    name: String,
    pub state: ConnState,
    last_hello: Option<Instant>,
    seq: u32,
    /// The newest snapshot: server tick, last applied input, entities.
    pub latest: Option<(u64, u32, Vec<EntityState>)>,
}

impl Connection {
    pub fn open(server: impl ToSocketAddrs, name: &str) -> std::io::Result<Self> {
        Ok(Self {
            socket: ClientSocket::connect(server)?,
            name: name.to_string(),
            state: ConnState::Connecting,
            last_hello: None,
            seq: 0,
            latest: None,
        })
    }

    /// Call once per client tick with the current input. Returns the
    /// sequence number the input went out with, once connected.
    pub fn update(&mut self, input: InputState) -> Option<u32> {
        for msg in self.socket.poll() {
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
                ServerMsg::Reject { reason } => self.state = ConnState::Rejected(reason),
                ServerMsg::Snapshot {
                    tick,
                    ack,
                    entities,
                } => {
                    // Packets can arrive out of order; keep only the newest.
                    if self.latest.as_ref().is_none_or(|(t, _, _)| tick > *t) {
                        self.latest = Some((tick, ack, entities));
                    }
                }
            }
        }
        match self.state {
            ConnState::Connecting => {
                if self.last_hello.is_none_or(|t| t.elapsed() >= HELLO_EVERY) {
                    self.socket.send(&ClientMsg::Hello {
                        name: self.name.clone(),
                    });
                    self.last_hello = Some(Instant::now());
                }
            }
            ConnState::Connected { .. } => {
                self.seq = self.seq.wrapping_add(1);
                self.socket.send(&ClientMsg::Input {
                    seq: self.seq,
                    input,
                });
                return Some(self.seq);
            }
            ConnState::Rejected(_) => {}
        }
        None
    }

    /// Our hero's ID once connected.
    pub fn hero(&self) -> Option<NetId> {
        match self.state {
            ConnState::Connected { hero, .. } => Some(hero),
            _ => None,
        }
    }

    /// Tell the server we are leaving.
    pub fn close(&self) {
        self.socket.send(&ClientMsg::Bye);
    }
}
