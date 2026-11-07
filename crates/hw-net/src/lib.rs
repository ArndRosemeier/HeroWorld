//! The wire between server and clients: message types, UDP transport, state
//! deltas and host migration.

/// Protocol version. Server and client must match exactly to play together.
pub const PROTOCOL_VERSION: u32 = 1;

const _: () = assert!(PROTOCOL_VERSION >= 1);
