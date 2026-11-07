//! The game server. It owns the truth: every entity, every hit point, every
//! broken wall. Clients (including the local one in single player) only show
//! and predict what this crate decides.

use bevy_ecs::world::World;

/// Simulation ticks per second.
pub const TICK_HZ: u32 = 30;

/// The server's simulation state.
pub struct Sim {
    pub world: World,
    pub tick: u64,
}

impl Sim {
    pub fn new() -> Self {
        Self {
            world: World::new(),
            tick: 0,
        }
    }

    /// Advance the simulation by one fixed tick.
    pub fn step(&mut self) {
        self.tick += 1;
    }
}

impl Default for Sim {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_count_ticks() {
        let mut sim = Sim::new();
        for _ in 0..3 {
            sim.step();
        }
        assert_eq!(sim.tick, 3);
    }
}
