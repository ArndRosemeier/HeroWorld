//! The power effects' WGSL looks side by side, for close-ups without a
//! fight: the rescue bubble (blue and gold), the ice block and a shield's
//! force field; `POWERFX_TESLA=1` shows the Tesla tower's live coil and
//! arcs at dusk instead.
//!
//! ```text
//! cargo run --release -p hw-client --example powerfx
//! ```
//! `ENGINE_SCREENSHOT` works; `POWERFX_PLAIN=1` leaves the materials off.

use engine::prelude::*;
use glam::DVec2;
use hw_client::powerfx;
use hw_client::teslaview::TeslaView;
use hw_gen::urban::landmarks::{Landmark, LandmarkKind};

fn main() -> EngineResult<()> {
    let plain = std::env::var_os("POWERFX_PLAIN").is_some();
    let tesla = std::env::var_os("POWERFX_TESLA").is_some();
    let mut tower = tesla.then(|| {
        TeslaView::for_tower(Landmark {
            kind: LandmarkKind::TeslaTower,
            pos: DVec2::ZERO,
            plaza: 12.0,
            base: 0.0,
            low: 0.0,
            height: 60.0,
            dir: DVec2::Y,
            stairs: Vec::new(),
        })
    });
    let settings = Settings::default().window(1280, 720).vsync(false);
    Engine::run_with_settings("powerfx", settings, move |world, frame| {
        if frame.first {
            let hour = if tesla { 19.8 } else { 16.0 };
            world.set_physical_sky(Some(PhysicalSky::at_hour(hour)));
            if !plain {
                powerfx::install_materials(world)?;
            }
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
            let field = world.place(
                powerfx::shield_sphere(),
                Place::new(6.0, 1.0, 0.0).with_stretch(Vec3::new(1.05, 1.2, 1.05)),
            )?;
            powerfx::dress_field(world, field);
            world.spawn(Shape::box_at(
                (6.0, 0.9, 0.0),
                (0.5, 1.8, 0.3),
                rgb(70, 90, 160),
            )?);
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
        if let Some(t) = tower.as_mut() {
            let (y, d) = std::env::var("POWERFX_NEAR").map_or((30.0, 75.0), |_| (40.0, 16.0));
            world.look_orbit(Vec3::new(0.0, y, 0.0), d, 160.0, 4.0);
            t.update(world, frame.dt)?;
        } else {
            match std::env::var("POWERFX_NEAR") {
                Ok(_) => world.look_orbit(Vec3::new(6.0, 1.1, 0.0), 3.8, 200.0, 8.0),
                Err(_) => world.look_orbit(Vec3::new(1.5, 1.2, 0.0), 11.0, 160.0, 10.0),
            }
        }
        world.check("frame advances", frame.time >= 0.0);
        Ok(())
    })
}
