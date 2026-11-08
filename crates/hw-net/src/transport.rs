//! Endpoints. Both sides are non-blocking and polled once per tick, so they
//! slot into a game loop without threads of their own.
//!
//! Packets travel over UDP when the players can reach each other directly
//! (one machine, a LAN, a forwarded port), or through the GameHub relay
//! ([`crate::gamehub`]) when they cannot. The game code sees no difference:
//! a [`Link`] moves packets, a [`PeerAddr`] says where they go.

use crate::protocol::{ClientMsg, PlayerId, ServerMsg, MAX_PACKET};
use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

/// A client that has said nothing for this long is dropped.
pub const TIMEOUT: Duration = Duration::from_secs(5);

/// Where a peer is: a UDP address, or a player in the host's GameHub room.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PeerAddr {
    Udp(SocketAddr),
    Hub(u32),
}

impl std::fmt::Display for PeerAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PeerAddr::Udp(a) => write!(f, "{a}"),
            PeerAddr::Hub(id) => write!(f, "relay player {id}"),
        }
    }
}

/// Something that moves packets for the server besides its UDP socket.
pub trait ServerLink: Send {
    fn send(&mut self, to: PeerAddr, bytes: &[u8]);
    /// Everything that arrived since the last call.
    fn recv(&mut self) -> Vec<(PeerAddr, Vec<u8>)>;
    /// Called once per tick after all sends (a relay batches them).
    fn flush(&mut self) {}
    /// The world's save, for a player to take over from if the host
    /// leaves (the relay keeps it in the room).
    fn publish(&mut self, _state: &str) {}
}

/// The host left and another took over (see [`Link::handover`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handover {
    /// We are the host now: start a server from [`Link::world_state`].
    ToUs,
    /// Another player is: say hello to it.
    ToOther,
}

/// Something that moves a client's packets to and from its server.
pub trait Link: Send {
    fn send(&mut self, bytes: &[u8]);
    fn recv(&mut self) -> Vec<Vec<u8>>;
    /// Whether another player can take over when the host leaves.
    fn can_hand_over(&self) -> bool {
        false
    }
    /// A change of host since the last call.
    fn handover(&mut self) -> Option<Handover> {
        None
    }
    /// The newest world save the host published.
    fn world_state(&self) -> Option<&str> {
        None
    }
    /// The GameHub connection, for a player taking over as host.
    fn take_hub(&mut self) -> Option<crate::gamehub::Hub> {
        None
    }
}

/// Something that happened on the server's socket.
#[derive(Clone, Debug, PartialEq)]
pub enum ServerEvent {
    /// A new address said hello. The game decides whether to accept it.
    Hello {
        addr: PeerAddr,
        name: String,
    },
    Message {
        player: PlayerId,
        msg: ClientMsg,
    },
    Left {
        player: PlayerId,
    },
}

struct Peer {
    addr: PeerAddr,
    last_heard: Instant,
    stats: Traffic,
}

/// Bytes and packets one player sent and received.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Traffic {
    pub bytes_out: u64,
    pub packets_out: u64,
    pub bytes_in: u64,
    pub packets_in: u64,
}

/// The server side: one socket (and maybe a relay), many players.
pub struct ServerSocket {
    udp: Option<UdpSocket>,
    links: Vec<Box<dyn ServerLink>>,
    peers: BTreeMap<PlayerId, Peer>,
}

impl ServerSocket {
    /// Binds to `addr` (use port 0 for any free port).
    pub fn bind(addr: impl ToSocketAddrs) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(addr)?;
        socket.set_nonblocking(true)?;
        Ok(Self {
            udp: Some(socket),
            links: Vec::new(),
            peers: BTreeMap::new(),
        })
    }

    /// A server reachable only through links (no UDP port).
    pub fn without_udp() -> Self {
        Self {
            udp: None,
            links: Vec::new(),
            peers: BTreeMap::new(),
        }
    }

    /// Opens (or replaces) the UDP socket on `addr`; returns its address.
    pub fn open_udp(&mut self, addr: impl ToSocketAddrs) -> std::io::Result<SocketAddr> {
        let socket = UdpSocket::bind(addr)?;
        socket.set_nonblocking(true)?;
        let local = socket.local_addr()?;
        self.udp = Some(socket);
        Ok(local)
    }

    /// Also take players through `link` (the GameHub relay).
    pub fn add_link(&mut self, link: Box<dyn ServerLink>) {
        self.links.push(link);
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        match &self.udp {
            Some(s) => s.local_addr(),
            None => Err(std::io::Error::new(ErrorKind::NotFound, "no UDP socket")),
        }
    }

    pub fn players(&self) -> impl Iterator<Item = PlayerId> + '_ {
        self.peers.keys().copied()
    }

    /// Accepts the address from a Hello as `player` (the game hands out IDs).
    pub fn accept(&mut self, addr: PeerAddr, player: PlayerId) {
        self.peers.insert(
            player,
            Peer {
                addr,
                last_heard: Instant::now(),
                stats: Traffic::default(),
            },
        );
    }

    /// The player at an address, if it was accepted.
    pub fn player_at(&self, addr: PeerAddr) -> Option<PlayerId> {
        self.peers
            .iter()
            .find(|(_, p)| p.addr == addr)
            .map(|(id, _)| *id)
    }

    fn send_bytes(&mut self, addr: PeerAddr, bytes: &[u8]) {
        match addr {
            PeerAddr::Udp(a) => {
                if let Some(s) = &self.udp {
                    let _ = s.send_to(bytes, a);
                }
            }
            PeerAddr::Hub(_) => {
                for l in &mut self.links {
                    l.send(addr, bytes);
                }
            }
        }
    }

    /// Sends to an address that is not (yet) a player, e.g. a Reject.
    pub fn send_to(&mut self, addr: PeerAddr, msg: &ServerMsg) {
        self.send_bytes(addr, &msg.encode());
    }

    pub fn send(&mut self, player: PlayerId, msg: &ServerMsg) {
        let bytes = msg.encode();
        if let Some(p) = self.peers.get_mut(&player) {
            p.stats.bytes_out += bytes.len() as u64;
            p.stats.packets_out += 1;
            let addr = p.addr;
            self.send_bytes(addr, &bytes);
        }
    }

    /// Gives the world's save to the links that keep it (the relay).
    pub fn publish(&mut self, state: &str) {
        for l in &mut self.links {
            l.publish(state);
        }
    }

    /// Whether any link keeps a published state (the relay).
    pub fn has_links(&self) -> bool {
        !self.links.is_empty()
    }

    /// Hands batched packets to the relay; call once per tick after sending.
    pub fn flush(&mut self) {
        for l in &mut self.links {
            l.flush();
        }
    }

    pub fn traffic(&self, player: PlayerId) -> Option<Traffic> {
        self.peers.get(&player).map(|p| p.stats)
    }

    pub fn kick(&mut self, player: PlayerId) {
        self.peers.remove(&player);
    }

    fn receive(&mut self) -> Vec<(PeerAddr, Vec<u8>)> {
        let mut out = Vec::new();
        if let Some(s) = &self.udp {
            let mut buf = [0u8; MAX_PACKET + 1];
            loop {
                match s.recv_from(&mut buf) {
                    Ok((n, addr)) => out.push((PeerAddr::Udp(addr), buf[..n].to_vec())),
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    // Windows reports a closed remote port as an error on the
                    // next receive; skip it and keep reading.
                    Err(e) if e.kind() == ErrorKind::ConnectionReset => continue,
                    Err(_) => break,
                }
            }
        }
        for l in &mut self.links {
            out.extend(l.recv());
        }
        out
    }

    /// Reads everything waiting and checks for timeouts.
    pub fn poll(&mut self) -> Vec<ServerEvent> {
        let mut events = Vec::new();
        for (addr, bytes) in self.receive() {
            let Ok(msg) = ClientMsg::decode(&bytes) else {
                continue;
            };
            let known = self
                .peers
                .iter_mut()
                .find(|(_, p)| p.addr == addr)
                .map(|(id, p)| (*id, p));
            match (known, msg) {
                (Some((id, _)), ClientMsg::Bye) => {
                    self.peers.remove(&id);
                    events.push(ServerEvent::Left { player: id });
                }
                // A repeated hello means the welcome got lost; the game resends it.
                (Some((_, peer)), ClientMsg::Hello { name }) => {
                    peer.last_heard = Instant::now();
                    events.push(ServerEvent::Hello { addr, name });
                }
                (Some((id, peer)), msg) => {
                    peer.last_heard = Instant::now();
                    peer.stats.bytes_in += bytes.len() as u64;
                    peer.stats.packets_in += 1;
                    events.push(ServerEvent::Message { player: id, msg });
                }
                (None, ClientMsg::Hello { name }) => events.push(ServerEvent::Hello { addr, name }),
                (None, _) => {}
            }
        }
        let now = Instant::now();
        let gone: Vec<PlayerId> = self
            .peers
            .iter()
            .filter(|(_, p)| now.duration_since(p.last_heard) > TIMEOUT)
            .map(|(id, _)| *id)
            .collect();
        for id in gone {
            self.peers.remove(&id);
            events.push(ServerEvent::Left { player: id });
        }
        events
    }
}

/// The client side over UDP: talks to one server.
pub struct ClientSocket {
    socket: UdpSocket,
    server: SocketAddr,
}

impl ClientSocket {
    pub fn connect(server: impl ToSocketAddrs) -> std::io::Result<Self> {
        let server = server
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| std::io::Error::new(ErrorKind::InvalidInput, "no server address"))?;
        let bind: SocketAddr = if server.is_ipv4() {
            "0.0.0.0:0".parse().expect("addr")
        } else {
            "[::]:0".parse().expect("addr")
        };
        let socket = UdpSocket::bind(bind)?;
        socket.set_nonblocking(true)?;
        Ok(Self { socket, server })
    }
}

impl Link for ClientSocket {
    fn send(&mut self, bytes: &[u8]) {
        let _ = self.socket.send_to(bytes, self.server);
    }

    fn recv(&mut self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let mut buf = [0u8; MAX_PACKET + 1];
        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((n, addr)) if addr == self.server => out.push(buf[..n].to_vec()),
                Ok(_) => {}
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::ConnectionReset => continue,
                Err(_) => break,
            }
        }
        out
    }
}
