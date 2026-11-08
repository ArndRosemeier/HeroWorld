//! A line-up of a hero in every clip and some citizens, for looking at
//! poses without the town.
//!
//! ```text
//! cargo run --release -p hw-client --example people -- [hero name] [town seed]
//! ```
//! Needs the packs (`scripts/fetch-packs.sh`). `ENGINE_SCREENSHOT` works.

use engine::prelude::*;
use hw_client::people::{self, Who};

fn main() -> EngineResult<()> {
    let mut args = std::env::args().skip(1);
    let name = args.next().unwrap_or_else(|| "Hero".into());
    let seed: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let packs = people::packs_dir().expect("no packs: run scripts/fetch-packs.sh");
    let mut builder = actors_engine::CharacterBuilder::load(&packs).expect("load packs");
    let hero = people::build(&mut builder, &Who::Hero(name), seed, None).expect("hero");
    let citizens: Vec<_> = (0..6)
        .map(|i| people::build(&mut builder, &Who::Citizen(i), seed, None).expect("citizen"))
        .collect();
    let clips = ["idle", "sprint", "jump", "hover", "fly"];
    let view = std::env::var("PEOPLE_VIEW").unwrap_or_default();
    Engine::run("people", move |world, frame| {
        if frame.first {
            world.set_physical_sky(Some(PhysicalSky::at_hour(10.5)));
            world.spawn(Shape::box_at(
                (0.0, -0.05, 0.0),
                (30.0, 0.1, 12.0),
                rgb(150, 145, 135),
            )?);
            for (i, clip) in clips.iter().enumerate() {
                let x = (i as f32 - 2.0) * 1.3;
                let mut place = Place::new(x, 0.0, 0.0);
                if *clip == "fly" && view == "flat" {
                    place = Place::new(x, 1.0, 0.0).with_pitch_deg(80.0);
                }
                let id = world.spawn_animated_shared(hero.model.clone(), place)?;
                world.play_animation(id, clip)?;
            }
            for (i, c) in citizens.iter().enumerate() {
                let x = (i as f32 - 2.5) * 1.1;
                let id = world.spawn_animated_shared(c.model.clone(), Place::new(x, 0.0, -3.0))?;
                world.play_animation(id, if i % 2 == 0 { "idle" } else { &c.walk })?;
            }
        }
        let yaw: f32 = std::env::var("PEOPLE_YAW")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(90.0);
        let dist: f32 = std::env::var("PEOPLE_DIST")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(8.5);
        let tx: f32 = std::env::var("PEOPLE_X")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);
        world.look_orbit(Vec3::new(tx, 1.0, -1.0), dist, yaw, 8.0);
        Ok(())
    })
}
