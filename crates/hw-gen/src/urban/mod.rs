//! Settlements of every kind, from a hamlet at a ford to a megalopolis on a
//! bay, generated the way real towns grow.
//!
//! Layers, coarse to fine (see the plan "HeroWorld cities: back to the
//! planning board"):
//!
//! 1. [`params`]: the settlement's parameters, sampled with realistic odds.
//! 2. [`land`]: terrain, sea, lakes and rivers.
//! 3. [`growth`]: centres, old roads, and how far the town spread, when.
//! 4. [`roads`]: main roads (old roads, rings, quays, avenues) and bridges,
//!    the avenues traced along a direction field ([`tensor`], [`trace`]).
//!
//! The older [`crate::city`] generator stays in use until this one can
//! replace it.

pub mod growth;
pub mod land;
pub mod lines;
pub mod params;
pub mod roads;
pub mod tensor;
pub mod trace;

pub use growth::{Centre, CentreKind, Era, Growth};
pub use land::{Cover, Land};
pub use params::{Founding, PlanStyle, Preset, Site, TownParams};
pub use roads::{Network, Road, RoadClass};
