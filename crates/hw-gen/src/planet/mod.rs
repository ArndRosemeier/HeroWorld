//! The planet as pure data: its shape, climate, biomes, lakes and rivers.
//!
//! [`Globe`] is the analytic shape, evaluated anywhere at any detail.
//! [`PlanetMap`] solves what needs the whole planet (rain, drainage) on a
//! global grid. Everything derives from the world seed. See
//! `docs/LANDSCAPE.md`.

pub mod biome;
pub mod globe;
pub mod grid;
pub mod map;

pub use biome::Biome;
pub use globe::Globe;
pub use grid::{Cell, Grid};
pub use map::{Landmass, PlanetMap, Sample};
