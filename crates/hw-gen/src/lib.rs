//! World generation as pure data. Everything here is a plan (planet, region,
//! city, building, interior) derived from the world seed; nothing renders and
//! nothing simulates. Same seed and inputs, same plan, on every machine.

pub mod building;
pub mod city;
pub mod doors;
pub mod geom;
pub mod layout;
pub mod planet;
pub mod roofsolid;
pub mod style;
pub mod urban;
pub mod water;

pub use hw_core::WORLD_VERSION;
