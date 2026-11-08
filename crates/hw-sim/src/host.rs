//! The host: the simulation plus players, local and remote.
//!
//! In single player the host runs inside the game with one local player and
//! no socket. Opening it to friends adds a UDP socket (and the GameHub relay
//! for friends who cannot reach it directly); remote players join with a
//! hello and play by sending input every tick.
//!
//! Each tick every remote player gets one update: the entities near their
//! hero (within [`INTEREST_RADIUS`] or inside their bubble) that changed
//! since they last confirmed them, nearest first, within a byte budget that
//! keeps a client under [`hw_net::BANDWIDTH_BUDGET`]. Joins, leaves, chat and
//! the end of the session go out reliably in the same packets.

use crate::save::WorldSave;
use crate::{Sim, TICK_HZ};
use glam::DVec3;
use hw_core::Seed;
use hw_net::protocol::UPDATE_HEADER_BYTES;
use hw_net::reliable::{ReliableReceiver, ReliableSender};
use hw_net::{
    Candidate, ClientMsg, ClientView, InputState, PeerAddr, PlayerId, ServerEvent, ServerLink,
    ServerMsg, ServerSocket, ToClient, ToServer, Traffic,
};
use std::collections::{BTreeMap, VecDeque};
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::Path;
use std::time::{Duration, Instant};

/// Most players in one world.
pub const MAX_PLAYERS: usize = hw_net::MAX_PLAYERS;
/// A client hears about everything within this distance of its hero.
pub const INTEREST_RADIUS: f64 = 500.0;
/// Bytes of entities and removals per update; the rest of the per-tick
/// budget is left for reliable messages and the header.
pub const UPDATE_BUDGET: usize =
    hw_net::BANDWIDTH_BUDGET / TICK_HZ as usize - UPDATE_HEADER_BYTES - 120;
/// Inputs a remote player may have waiting; more means they run ahead, and
/// the oldest are dropped.
const INPUT_QUEUE: usize = 6;
/// Most bytes of reliable news in one update (damage after a big hit, or
/// the whole city's damage for a joining player, spreads over a few ticks).
const RELIABLE_BYTES: usize = 360;
/// What [`UPDATE_BUDGET`] leaves for reliable news.
const RELIABLE_RESERVE: usize = 120;
/// Ticks before an unconfirmed reliable message goes out again.
const RESEND_TICKS: u64 = 6;

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
    Joined {
        player: PlayerId,
        name: String,
    },
    Left {
        player: PlayerId,
        name: String,
    },
    Chat {
        player: PlayerId,
        name: String,
        text: String,
    },
}

/// What the host keeps per remote player.
#[derive(Default)]
struct Remote {
    view: ClientView,
    inputs: VecDeque<(u32, InputState)>,
    /// Newest input sequence queued.
    newest: u32,
    rel_out: ReliableSender<ToClient>,
    rel_in: ReliableReceiver<ToServer>,
}

pub struct Host {
    pub sim: Sim,
    socket: Option<ServerSocket>,
    players: BTreeMap<PlayerId, PlayerInfo>,
    remotes: BTreeMap<PlayerId, Remote>,
    next_player: u16,
    /// World news (damage, impacts) for the player at this machine.
    local_news: Vec<ToClient>,
    /// Fight events since the local player last took them.
    fx: Vec<hw_net::Fx>,
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
            remotes: BTreeMap::new(),
            next_player: 1,
            local_news: Vec::new(),
            fx: Vec::new(),
        }
    }

    /// Opens the world to remote players on `addr` (port 0 picks a free one).
    pub fn listen(&mut self, addr: impl ToSocketAddrs) -> std::io::Result<SocketAddr> {
        self.socket
            .get_or_insert_with(ServerSocket::without_udp)
            .open_udp(addr)
    }

    /// Also takes players through `link` (the GameHub relay).
    pub fn add_link(&mut self, link: Box<dyn ServerLink>) {
        self.socket
            .get_or_insert_with(ServerSocket::without_udp)
            .add_link(link);
    }

    pub fn players(&self) -> &BTreeMap<PlayerId, PlayerInfo> {
        &self.players
    }

    /// Adds the player sitting at this machine.
    pub fn join_local(&mut self, name: &str) -> PlayerId {
        self.add_player(name, false)
            .expect("room for the local player")
    }

    /// Bytes and packets exchanged with a remote player so far.
    pub fn traffic(&self, player: PlayerId) -> Option<Traffic> {
        self.socket.as_ref()?.traffic(player)
    }

    fn tell_all(&mut self, msg: ToClient) {
        for r in self.remotes.values_mut() {
            r.rel_out.push(msg.clone());
        }
    }

    fn add_player(&mut self, name: &str, remote: bool) -> Option<PlayerId> {
        if self.players.len() >= MAX_PLAYERS {
            return None;
        }
        let id = PlayerId(self.next_player);
        self.next_player = self.next_player.checked_add(1)?;
        self.tell_all(ToClient::Joined {
            player: id,
            name: name.to_string(),
        });
        if remote {
            // The newcomer hears who is here already, then about itself.
            let mut r = Remote::default();
            for (p, info) in &self.players {
                r.rel_out.push(ToClient::Joined {
                    player: *p,
                    name: info.name.clone(),
                });
            }
            r.rel_out.push(ToClient::Joined {
                player: id,
                name: name.to_string(),
            });
            // The damage done so far, so a late joiner sees the same city.
            for m in self.sim.damage.snapshot() {
                r.rel_out.push(m);
            }
            for m in self.sim.street.snapshot() {
                r.rel_out.push(m);
            }
            self.remotes.insert(id, r);
        }
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
        self.remotes.remove(&id);
        self.sim.remove_hero(id);
        if let Some(s) = self.socket.as_mut() {
            s.kick(id);
        }
        self.tell_all(ToClient::Left {
            player: id,
            name: info.name.clone(),
        });
        Some(HostEvent::Left {
            player: id,
            name: info.name,
        })
    }

    /// The local player says something to everyone.
    pub fn chat(&mut self, from: PlayerId, text: &str) {
        self.tell_all(ToClient::Chat {
            from,
            text: text.to_string(),
        });
    }

    fn welcome(&mut self, addr: PeerAddr, player: Option<PlayerId>) {
        let msg = match player.and_then(|p| self.sim.hero_of(p).map(|h| (p, h))) {
            Some((player, hero)) => ServerMsg::Welcome {
                player,
                hero,
                world_seed: self.sim.seed.0,
                town_population: self.sim.town_population,
                tick_hz: TICK_HZ as u16,
            },
            None => ServerMsg::Reject {
                reason: "this world is full".into(),
            },
        };
        if let Some(s) = self.socket.as_mut() {
            s.send_to(addr, &msg);
        }
    }

    fn on_input(&mut self, player: PlayerId, msg: ClientMsg, out: &mut Vec<HostEvent>) {
        let ClientMsg::Input {
            seq,
            inputs,
            ack_tick,
            ack_bits,
            rel_ack,
            reliable,
        } = msg
        else {
            return;
        };
        let Some(r) = self.remotes.get_mut(&player) else {
            return;
        };
        r.view.on_ack(ack_tick, ack_bits);
        r.rel_out.ack(rel_ack);
        let n = inputs.len() as u32;
        for (k, input) in inputs.into_iter().enumerate() {
            let s = seq.wrapping_sub(n - 1 - k as u32);
            let newer = s.wrapping_sub(r.newest) as i32 > 0;
            if newer {
                r.inputs.push_back((s, input));
                r.newest = s;
            }
        }
        while r.inputs.len() > INPUT_QUEUE {
            r.inputs.pop_front();
        }
        let mut said = Vec::new();
        for (id, m) in reliable {
            said.extend(r.rel_in.receive(id, m));
        }
        for m in said {
            match m {
                ToServer::Chat { text } => {
                    let name = self
                        .players
                        .get(&player)
                        .map(|p| p.name.clone())
                        .unwrap_or_default();
                    self.chat(player, &text);
                    out.push(HostEvent::Chat { player, name, text });
                }
            }
        }
    }

    /// One tick: read the network, advance the simulation, send updates.
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
                    self.welcome(addr, player);
                }
                ServerEvent::Message { player, msg } => self.on_input(player, msg, &mut out),
                ServerEvent::Left { player } => out.extend(self.remove_player(player)),
            }
        }
        // Each remote hero takes its next input; with none waiting it keeps
        // doing what it did.
        let mut take = Vec::new();
        for (p, r) in self.remotes.iter_mut() {
            if let Some((seq, input)) = r.inputs.pop_front() {
                take.push((*p, seq, input));
            }
        }
        for (p, seq, input) in take {
            self.sim.set_input(p, input);
            if let Some(info) = self.players.get_mut(&p) {
                info.ack = seq;
            }
        }
        self.sim.step();
        let news = self.sim.take_news();
        if self.players.values().any(|p| !p.remote) {
            self.local_news.extend(news.iter().cloned());
        }
        for m in news {
            self.tell_all(m);
        }
        self.share_fx();
        self.send_updates();
        out
    }

    /// World news (damage, repairs, impacts) for the player at this machine
    /// since the last call.
    pub fn take_local_news(&mut self) -> Vec<ToClient> {
        std::mem::take(&mut self.local_news)
    }

    /// Fight events since the last call (for the player at this machine).
    pub fn take_fx(&mut self) -> Vec<hw_net::Fx> {
        std::mem::take(&mut self.fx)
    }

    /// This tick's fight events: kept for the local player, and sent
    /// (reliably) to each remote player near enough to see them.
    fn share_fx(&mut self) {
        let events = self.sim.fx().to_vec();
        if events.is_empty() {
            return;
        }
        // Nobody reads them in a dedicated server; keep only the newest.
        if self.fx.len() > 256 {
            self.fx.clear();
        }
        self.fx.extend(events.iter().copied());
        let players: Vec<PlayerId> = self.remotes.keys().copied().collect();
        for p in players {
            let Some(at) = self.sim.hero_of(p).and_then(|h| self.sim.position(h)) else {
                continue;
            };
            let r = self.remotes.get_mut(&p).expect("remote");
            for e in &events {
                let d = DVec3::new(e.at[0] as f64, e.at[1] as f64, e.at[2] as f64).distance(at);
                if d <= INTEREST_RADIUS {
                    r.rel_out.push(ToClient::Fx(*e));
                }
            }
        }
    }

    fn send_updates(&mut self) {
        let Some(socket) = self.socket.as_mut() else {
            return;
        };
        if self.remotes.is_empty() {
            socket.flush();
            return;
        }
        let all = self.sim.replicated();
        let tick = self.sim.tick;
        for (p, r) in self.remotes.iter_mut() {
            let me = all.iter().find(|(e, _)| e.owner == Some(*p));
            let Some(&(me, _)) = me else { continue };
            let at = DVec3::from_array(me.pos);
            let bubble = self.sim.bubbles.of(*p);
            let candidates: Vec<Candidate> = all
                .iter()
                .filter_map(|(e, changed)| {
                    let pos = DVec3::from_array(e.pos);
                    let d = pos.distance(at);
                    let near = d <= INTEREST_RADIUS || bubble.is_some_and(|b| b.contains(pos));
                    near.then_some(Candidate {
                        state: *e,
                        changed: *changed,
                        priority: if e.id == me.id { 0.0 } else { d.max(1.0) },
                    })
                })
                .collect();
            // Reliable news first, at most RELIABLE_BYTES a packet (the rest
            // goes out again soon); entities fill what is left.
            let mut reliable = r.rel_out.due(tick, RESEND_TICKS);
            let mut used = 0;
            let fit = reliable
                .iter()
                .take_while(|(_, m)| {
                    used += 4 + m.wire_size();
                    used <= RELIABLE_BYTES
                })
                .count();
            reliable.truncate(fit.max(1).min(reliable.len()));
            let rel_bytes: usize = reliable.iter().map(|(_, m)| 4 + m.wire_size()).sum();
            let budget = (UPDATE_BUDGET + RELIABLE_RESERVE)
                .saturating_sub(rel_bytes)
                .max(200);
            let (entities, removed) = r.view.build(tick, &candidates, budget);
            let ack = self.players.get(p).map_or(0, |i| i.ack);
            socket.send(
                *p,
                &ServerMsg::Update {
                    tick,
                    ack,
                    rel_ack: r.rel_in.ack(),
                    reliable,
                    entities,
                    removed,
                },
            );
        }
        socket.flush();
    }

    /// Ends the session cleanly: every remote player is told (reliably) and
    /// leaves; waits up to `wait` for them to confirm.
    pub fn end_session(&mut self, reason: &str, wait: Duration) {
        self.tell_all(ToClient::SessionEnd {
            reason: reason.to_string(),
        });
        let start = Instant::now();
        while start.elapsed() < wait && self.remotes.values().any(|r| r.rel_out.unconfirmed() > 0) {
            self.tick();
            std::thread::sleep(Duration::from_millis(5));
        }
        self.socket = None;
        self.remotes.clear();
        let remote: Vec<PlayerId> = self
            .players
            .iter()
            .filter(|(_, i)| i.remote)
            .map(|(p, _)| *p)
            .collect();
        for p in remote {
            self.players.remove(&p);
            self.sim.remove_hero(p);
        }
    }

    /// Saves the world to `path`.
    pub fn save_world(&mut self, path: &Path) -> std::io::Result<()> {
        self.sim.save().write(path)
    }

    /// Loads a save into this host's world (same seed).
    pub fn load_world(&mut self, path: &Path) -> std::io::Result<()> {
        let save = WorldSave::read(path)?;
        self.sim.restore(&save).map_err(std::io::Error::other)
    }
}
