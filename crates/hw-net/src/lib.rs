//! The wire between server and clients: message types, transports (UDP and
//! the GameHub relay), reliable messages and replication.
//!
//! Hand-rolled on `std::net::UdpSocket`: the server sends each client, every
//! tick, only what changed near its player ([`replication`]); joins, chat and
//! the end of a session ride a reliable channel ([`reliable`]) in the same
//! packets.

pub mod connection;
pub mod protocol;
pub mod reliable;
pub mod replication;
pub mod transport;
pub mod wire;

pub use connection::{ConnState, Connection};
pub use protocol::{
    ClientMsg, EntityKind, EntityState, InputState, PlayerId, ServerMsg, ToClient, ToServer,
};
pub use replication::{Candidate, ClientView};
pub use transport::{ClientSocket, Link, PeerAddr, ServerEvent, ServerLink, ServerSocket, Traffic};

/// Protocol version. Server and client must match exactly to play together.
pub const PROTOCOL_VERSION: u32 = 7;

/// Port a host listens on unless told otherwise.
pub const DEFAULT_PORT: u16 = 7777;

/// What one client may cost the host per second, in bytes (about 200 kbit/s:
/// fine for any home connection, ten players included).
pub const BANDWIDTH_BUDGET: usize = 25_000;
