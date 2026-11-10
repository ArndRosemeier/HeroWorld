//! The power effects' see-through looks side by side, for close-ups
//! without a fight: the rescue bubble (blue and gold) and the ice block,
//! with their WGSL materials, beside the plain vertex-colour versions.
//!
//! ```text
//! cargo run --release -p hw-client --example powerfx
//! ```
//! `ENGINE_SCREENSHOT` works; `POWERFX_PLAIN=1` leaves the materials off.

use engine::prelude::*;
use hw_client::powerfx;

fn main() -> EngineResult<()> {
    let plain = std::env::var_os("POWERFX_PLAIN").is_some();
    let settings = Settings::default().window(1280, 720).vsync(false);
    Engine::run_with_settings("powerfx", settings, move |world, frame| {
        if frame.first {
            world.set_physical_sky(Some(PhysicalSky::at_hour(16.0)));
            world.spawn(Shape::box_at(
                (0.0, -0.05, 0.0),
                (30.0, 0.1, 20.0),
                rgb(120, 125, 118),
            )?);
            // Something behind to see through to.
            world.spawn(Shape::box_at(
                (0.0, 1.5, -3.0),
                (9.0, 3.0, 0.3),
                rgb(180, 70, 50),
            )?);
            for x in [-3.0f32, 0.0, 3.0] {
                world.spawn(Shape::box_at(
                    (x, 0.9, 0.0),
                    (0.5, 1.8, 0.3),
                    rgb(70, 90, 160),
                )?);
            }
            if !plain {
                powerfx::install_materials(world)?;
            }
            let blue = world.place(
                powerfx::bubble_sphere(1.25, false),
                Place::new(-3.0, 1.05, 0.0),
            )?;
            let gold = world.place(
                powerfx::bubble_sphere(1.25, true),
                Place::new(0.0, 1.05, 0.0),
            )?;
            let ice = world.place(powerfx::ice_block(), Place::new(3.0, 0.0, 0.0))?;
            powerfx::dress_bubble(world, blue);
            powerfx::dress_bubble(world, gold);
            powerfx::dress_ice(world, ice);
        }
        world.look_orbit(Vec3::new(0.0, 1.2, 0.0), 9.0, 160.0, 10.0);
        world.check("frame advances", frame.time >= 0.0);
        Ok(())
    })
}
