//! Shared basics for every HeroWorld crate: planet coordinates, seed hashing
//! and IDs. Pure Rust, no Engine, so the server and tools can use it anywhere.

/// Version of the world-generation rules. Bump it when a change would make an
/// existing seed produce a different world, so saves can tell.
pub const WORLD_VERSION: u32 = 1;

const _: () = assert!(WORLD_VERSION >= 1);
