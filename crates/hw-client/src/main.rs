//! HeroWorld: the game client, with the server built in for single player.

use engine::prelude::*;

fn main() -> EngineResult<()> {
    let mut sim = hw_sim::Sim::new();
    Engine::run("HeroWorld", move |world, frame| {
        if frame.first {
            world.spawn(
                Shape::box_at((0.0, -0.05, 0.0), (40.0, 0.1, 40.0), rgb(120, 150, 110)).unwrap(),
            );
            world.spawn(
                Shape::box_at((0.0, 4.0, 0.0), (6.0, 8.0, 6.0), rgb(230, 214, 180)).unwrap(),
            );
            world.set_sun((0.4, 1.0, -0.55), 0.2);
        }
        sim.step();
        world.look_orbit((0.0, 3.0, 0.0), 30.0, -40.0, 25.0);
        Ok(())
    })
}
