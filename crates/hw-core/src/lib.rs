//! Shared basics for every HeroWorld crate: planet coordinates, seed hashing
//! and IDs. Pure Rust, no Engine, so the server and tools can use it anywhere.

pub mod coords;
pub mod det;
pub mod ids;
pub mod seed;

pub use coords::{LatLon, LocalFrame, Planet, PlanetPos};
pub use ids::{NetId, NetIdAllocator, StableId};
pub use seed::{Rng, Seed};

/// Version of the world-generation rules. Bump it when a change would make an
/// existing seed produce a different world, so saves can tell.
pub const WORLD_VERSION: u32 = 5;
