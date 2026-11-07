//! Settlements of every kind, from a hamlet at a ford to a megalopolis on a
//! bay, generated the way real towns grow.
//!
//! Layers, coarse to fine (see the plan "HeroWorld cities: back to the
//! planning board"):
//!
//! 1. [`params`]: the settlement's parameters, sampled with realistic odds.
//! 2. [`land`]: terrain, sea, lakes and rivers.
//! 3. [`growth`]: centres, old roads, and how far the town spread, when.
//!
//! The older [`crate::city`] generator stays in use until this one can
//! replace it.

pub mod growth;
pub mod land;
pub mod params;

pub use growth::{Centre, CentreKind, Era, Growth};
pub use land::{Cover, Land};
pub use params::{Founding, PlanStyle, Preset, Site, TownParams};
