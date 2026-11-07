//! UDP endpoints. Both sides are non-blocking and polled once per tick, so
//! they slot into a game loop without threads.

use crate::protocol::{ClientMsg, PlayerId, ServerMsg, MAX_PACKET};
use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

/// A client that has said nothing for this long is dropped.
pub const TIMEOUT: Duration = Duration::from_secs(5);

/// Something that happened on the server's socket.
#[derive(Clone, Debug, PartialEq)]
pub enum ServerEvent {
    /// A new address said hello. The game decides whether to accept it.
    Hello {
        addr: SocketAddr,
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
    addr: SocketAddr,
    last_heard: Instant,
}

/// The server side: one socket, many players.
pub struct ServerSocket {
    socket: UdpSocket,
    peers: BTreeMap<PlayerId, Peer>,
}

impl ServerSocket {
    /// Binds to `addr` (use port 0 for any free port).
    pub fn bind(addr: impl ToSocketAddrs) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(addr)?;
        socket.set_nonblocking(true)?;
        Ok(Self {
            socket,
            peers: BTreeMap::new(),
        })
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    pub fn players(&self) -> impl Iterator<Item = PlayerId> + '_ {
        self.peers.keys().copied()
    }

    /// Accepts the address from a Hello as `player` (the game hands out IDs).
    pub fn accept(&mut self, addr: SocketAddr, player: PlayerId) {
        self.peers.insert(
            player,
            Peer {
                addr,
                last_heard: Instant::now(),
            },
        );
    }

    /// The player at an address, if it was accepted.
    pub fn player_at(&self, addr: SocketAddr) -> Option<PlayerId> {
        self.peers
            .iter()
            .find(|(_, p)| p.addr == addr)
            .map(|(id, _)| *id)
    }

    /// Sends to an address that is not (yet) a player, e.g. a Reject.
    pub fn send_to(&self, addr: SocketAddr, msg: &ServerMsg) {
        let _ = self.socket.send_to(&msg.encode(), addr);
    }

    pub fn send(&self, player: PlayerId, msg: &ServerMsg) {
        if let Some(p) = self.peers.get(&player) {
            let _ = self.socket.send_to(&msg.encode(), p.addr);
        }
    }

    pub fn kick(&mut self, player: PlayerId) {
        self.peers.remove(&player);
    }

    /// Reads everything waiting on the socket and checks for timeouts.
    pub fn poll(&mut self) -> Vec<ServerEvent> {
        let mut events = Vec::new();
        let mut buf = [0u8; MAX_PACKET + 1];
        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((n, addr)) => {
                    let Ok(msg) = ClientMsg::decode(&buf[..n]) else {
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
                            events.push(ServerEvent::Message { player: id, msg });
                        }
                        (None, ClientMsg::Hello { name }) => {
                            events.push(ServerEvent::Hello { addr, name })
                        }
                        (None, _) => {}
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                // Windows reports a closed remote port as an error on the next
                // receive; skip it and keep reading.
                Err(e) if e.kind() == ErrorKind::ConnectionReset => continue,
                Err(_) => break,
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

/// The client side: talks to one server.
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

    pub fn send(&self, msg: &ClientMsg) {
        let _ = self.socket.send_to(&msg.encode(), self.server);
    }

    /// Everything the server sent since the last poll.
    pub fn poll(&mut self) -> Vec<ServerMsg> {
        let mut out = Vec::new();
        let mut buf = [0u8; MAX_PACKET + 1];
        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((n, addr)) if addr == self.server => {
                    if let Ok(msg) = ServerMsg::decode(&buf[..n]) {
                        out.push(msg);
                    }
                }
                Ok(_) => {}
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::ConnectionReset => continue,
                Err(_) => break,
            }
        }
        out
    }
}
