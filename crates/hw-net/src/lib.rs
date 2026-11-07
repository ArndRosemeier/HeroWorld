//! The wire between server and clients: message types, UDP transport, and
//! (later) state deltas and host migration.
//!
//! Hand-rolled on `std::net::UdpSocket` for now: snapshots are full state, so
//! nothing needs to be reliable yet. Reliable channels (for chat, events and
//! host migration) come when the game needs them.

pub mod connection;
pub mod protocol;
pub mod transport;
pub mod wire;

pub use connection::{ConnState, Connection};
pub use protocol::{ClientMsg, EntityState, InputState, PlayerId, ServerMsg};
pub use transport::{ClientSocket, ServerEvent, ServerSocket};

/// Protocol version. Server and client must match exactly to play together.
pub const PROTOCOL_VERSION: u32 = 3;

/// Port a host listens on unless told otherwise.
pub const DEFAULT_PORT: u16 = 7777;
