//! The host: the simulation plus players, local and remote.
//!
//! In single player the host runs inside the game with one local player and
//! no socket. Opening it to friends adds a UDP socket; remote players then
//! join with a hello and play by sending input every tick.

use crate::{Sim, TICK_HZ};
use hw_core::Seed;
use hw_net::{ClientMsg, PlayerId, ServerEvent, ServerMsg, ServerSocket};
use std::collections::BTreeMap;
use std::net::{SocketAddr, ToSocketAddrs};

/// Most players in one world.
pub const MAX_PLAYERS: usize = 10;

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerInfo {
    pub name: String,
    /// Last input sequence applied (remote players).
    pub ack: u32,
    pub remote: bool,
}

/// Something worth telling the person at the host.
#[derive(Clone, Debug, PartialEq)]
pub enum HostEvent {
    Joined { player: PlayerId, name: String },
    Left { player: PlayerId, name: String },
}

pub struct Host {
    pub sim: Sim,
    socket: Option<ServerSocket>,
    players: BTreeMap<PlayerId, PlayerInfo>,
    next_player: u16,
}

impl Host {
    /// A host for single player: no socket.
    pub fn new(seed: Seed) -> Self {
        Self::with_sim(Sim::new(seed))
    }

    /// A host around an existing simulation.
    pub fn with_sim(sim: Sim) -> Self {
        Self {
            sim,
            socket: None,
            players: BTreeMap::new(),
            next_player: 1,
        }
    }

    /// Opens the world to remote players on `addr` (port 0 picks a free one).
    pub fn listen(&mut self, addr: impl ToSocketAddrs) -> std::io::Result<SocketAddr> {
        let socket = ServerSocket::bind(addr)?;
        let local = socket.local_addr()?;
        self.socket = Some(socket);
        Ok(local)
    }

    pub fn players(&self) -> &BTreeMap<PlayerId, PlayerInfo> {
        &self.players
    }

    /// Adds the player sitting at this machine.
    pub fn join_local(&mut self, name: &str) -> PlayerId {
        self.add_player(name, false)
            .expect("room for the local player")
    }

    fn add_player(&mut self, name: &str, remote: bool) -> Option<PlayerId> {
        if self.players.len() >= MAX_PLAYERS {
            return None;
        }
        let id = PlayerId(self.next_player);
        self.next_player = self.next_player.checked_add(1)?;
        self.players.insert(
            id,
            PlayerInfo {
                name: name.to_string(),
                ack: 0,
                remote,
            },
        );
        // Simple spawn rule for now: a row along the central street.
        let at = self.sim.spawn_point(id.0 as u32);
        self.sim.spawn_hero(id, at);
        Some(id)
    }

    fn remove_player(&mut self, id: PlayerId) -> Option<HostEvent> {
        let info = self.players.remove(&id)?;
        self.sim.remove_hero(id);
        if let Some(s) = self.socket.as_mut() {
            s.kick(id);
        }
        Some(HostEvent::Left {
            player: id,
            name: info.name,
        })
    }

    /// One tick: read the network, advance the simulation, send snapshots.
    pub fn tick(&mut self) -> Vec<HostEvent> {
        let mut out = Vec::new();
        let events = self.socket.as_mut().map(|s| s.poll()).unwrap_or_default();
        for e in events {
            match e {
                ServerEvent::Hello { addr, name } => {
                    let known = self.socket.as_ref().and_then(|s| s.player_at(addr));
                    let player = match known {
                        Some(p) => Some(p),
                        None => {
                            let p = self.add_player(&name, true);
                            if let (Some(p), Some(s)) = (p, self.socket.as_mut()) {
                                s.accept(addr, p);
                                out.push(HostEvent::Joined { player: p, name });
                            }
                            p
                        }
                    };
                    let socket = self.socket.as_ref().expect("events come from the socket");
                    match player.and_then(|p| self.sim.hero_of(p).map(|h| (p, h))) {
                        Some((player, hero)) => socket.send(
                            player,
                            &ServerMsg::Welcome {
                                player,
                                hero,
                                world_seed: self.sim.seed.0,
                                town_population: self.sim.town_population,
                                tick_hz: TICK_HZ as u16,
                            },
                        ),
                        None => socket.send_to(
                            addr,
                            &ServerMsg::Reject {
                                reason: "this world is full".into(),
                            },
                        ),
                    }
                }
                ServerEvent::Message { player, msg } => match msg {
                    ClientMsg::Input { seq, input } => {
                        if let Some(info) = self.players.get_mut(&player) {
                            // Ignore stale packets that arrive late.
                            if seq.wrapping_sub(info.ack) < u32::MAX / 2 {
                                info.ack = seq;
                                self.sim.set_input(player, input);
                            }
                        }
                    }
                    ClientMsg::Hello { .. } | ClientMsg::Bye => {}
                },
                ServerEvent::Left { player } => out.extend(self.remove_player(player)),
            }
        }
        self.sim.step();
        if let Some(socket) = &self.socket {
            let entities = self.sim.snapshot();
            for (id, info) in self.players.iter().filter(|(_, i)| i.remote) {
                socket.send(
                    *id,
                    &ServerMsg::Snapshot {
                        tick: self.sim.tick,
                        ack: info.ack,
                        entities: entities.clone(),
                    },
                );
            }
        }
        out
    }
}
