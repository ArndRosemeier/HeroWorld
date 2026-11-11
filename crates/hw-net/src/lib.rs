//! The wire between server and clients: message types, transports (UDP and
//! the GameHub relay), reliable messages and replication.
//!
//! Hand-rolled on `std::net::UdpSocket`: the server sends each client, every
//! tick, only what changed near its player ([`replication`]); joins, chat and
//! the end of a session ride a reliable channel ([`reliable`]) in the same
//! packets.

pub mod connection;
pub mod gamehub;
pub mod look;
pub mod protocol;
pub mod reliable;
pub mod replication;
pub mod transport;
pub mod upnp;
pub mod wire;

pub use connection::{ConnState, Connection};
pub use protocol::{
    fizzle, ClientMsg, CombatState, EntityKind, EntityState, Faction, Fx, FxKind, ImpactKind,
    InputState, OwnedPower, PlayerId, Power, PowerId, ServerMsg, Sheet, Status, ToClient, ToServer,
    EFFECT_ARMOURED, EFFECT_BURNING, EFFECT_CAN_FLY, EFFECT_CARRYING, EFFECT_CHARMED,
    EFFECT_CHILLED, EFFECT_CONFUSED, EFFECT_DISARMED, EFFECT_EMPOWERED, EFFECT_ENRAGED,
    EFFECT_FEARED, EFFECT_FROZEN, EFFECT_GLIDE, EFFECT_HELD, EFFECT_HIDDEN, EFFECT_KNEELING,
    EFFECT_RALLIED, EFFECT_SHIELD, EFFECT_SPRINT, EFFECT_SUPER_JUMP, EFFECT_SUPER_SPEED,
    EFFECT_TOUGH, EFFECT_WALL_RUN, EFFECT_WEAKENED, FACTIONS, POWER_IDS,
};
pub use replication::{Candidate, ClientView};
pub use transport::{
    ClientSocket, Handover, Link, PeerAddr, ServerEvent, ServerLink, ServerSocket, Traffic,
};

/// Protocol version. Server and client must match exactly to play together.
pub const PROTOCOL_VERSION: u32 = 40;

/// Most players in one world.
pub const MAX_PLAYERS: usize = 10;

/// Port a host listens on unless told otherwise.
pub const DEFAULT_PORT: u16 = 7777;

/// What one client may cost the host per second, in bytes (about 200 kbit/s:
/// fine for any home connection, ten players included).
pub const BANDWIDTH_BUDGET: usize = 25_000;
