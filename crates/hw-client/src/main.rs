//! HeroWorld: the game client, with the server built in for single player.
//!
//! Milestone 0: fly over a generated city. Buildings around a focus point
//! can be cut open to show their rooms.
//!
//! Controls: right mouse + move to look, WASD to fly, Space/E up, Ctrl/Q
//! down, Shift fast. C toggles the cut-open view, F moves the cut-open area
//! to where you look, Z/X lower and raise the cut floor. Your hero (the red
//! figure) runs with I/J/K/L and jumps with U.
//!
//! By default the server runs inside the game (single player). `HW_HOST=1`
//! also opens it to friends on UDP port 7777; `HW_JOIN=host:port` joins
//! someone else's world instead.
//!
//! Environment: `HW_SEED` (world seed, default 1); `HW_NAME`; `HW_VIEW` =
//! `overview`, `street`, `cutaway` or `heroes` picks the starting camera
//! (handy with `ENGINE_SCREENSHOT`); `HW_CUT_LEVEL` the floor shown cut open.

mod citymesh;
mod session;

use citymesh::{add_building, ground_hit, ground_mesh, Look, MeshBuilder};
use engine::prelude::*;
use glam::{DVec2, Vec3};
use hw_core::Seed;
use hw_gen::building::BuildingPlan;
use hw_gen::city::{CityParams, CityPlan};
use session::Session;
use std::collections::{BTreeMap, BTreeSet};

/// Buildings within this distance of the focus are cut open.
const CUT_RADIUS: f64 = 110.0;

struct City {
    plan: CityPlan,
    /// One plan per building site, same order as `plan.buildings`.
    buildings: Vec<BuildingPlan>,
    /// Buildings per boulevard cell.
    by_cell: Vec<Vec<usize>>,
    /// Engine entity per cell.
    cell_entities: Vec<Option<EntityId>>,
    cut_on: bool,
    cut_focus: DVec2,
    cut_level: u32,
}

impl City {
    fn new(seed: Seed) -> Self {
        let plan = CityPlan::generate(seed, &CityParams::default());
        let buildings: Vec<BuildingPlan> = plan
            .buildings
            .iter()
            .map(|(_, site)| BuildingPlan::generate(site))
            .collect();
        let mut by_cell = vec![Vec::new(); plan.cells.len()];
        for (i, (lot, _)) in plan.buildings.iter().enumerate() {
            by_cell[plan.blocks[plan.lots[*lot].block].cell].push(i);
        }
        let cells = plan.cells.len();
        Self {
            plan,
            buildings,
            by_cell,
            cell_entities: vec![None; cells],
            cut_on: true,
            cut_focus: DVec2::ZERO,
            cut_level: 1,
        }
        .focused_on_centre()
    }

    /// Starts with the cut-open area on the tallest building near the centre.
    fn focused_on_centre(mut self) -> Self {
        let best = (0..self.buildings.len())
            .filter(|&i| self.centre_of(i).length() < 250.0)
            .max_by(|&a, &b| {
                self.buildings[a]
                    .floors
                    .len()
                    .cmp(&self.buildings[b].floors.len())
            })
            .or_else(|| {
                (0..self.buildings.len()).min_by(|&a, &b| {
                    self.centre_of(a)
                        .length()
                        .total_cmp(&self.centre_of(b).length())
                })
            });
        if let Some(i) = best {
            self.cut_focus = self.centre_of(i);
        }
        self
    }

    fn centre_of(&self, building: usize) -> DVec2 {
        let site = &self.buildings[building].site;
        site.origin + site.along * site.size.x * 0.5 + site.inward * site.size.y * 0.5
    }

    fn look_of(&self, building: usize) -> Look {
        if self.cut_on && self.centre_of(building).distance(self.cut_focus) < CUT_RADIUS {
            Look::Cut {
                level: self.cut_level,
            }
        } else {
            Look::Shell
        }
    }

    fn cells_near(&self, p: DVec2) -> BTreeSet<usize> {
        (0..self.plan.cells.len())
            .filter(|&c| {
                self.by_cell[c].iter().any(|&b| {
                    let s = &self.buildings[b].site;
                    s.origin.distance(p) < CUT_RADIUS + 120.0
                })
            })
            .collect()
    }

    fn build_cell(&mut self, world: &mut World, cell: usize) {
        if let Some(e) = self.cell_entities[cell].take() {
            world.despawn(e);
        }
        let mut b = MeshBuilder::new();
        for &i in &self.by_cell[cell] {
            let lot = self.plan.buildings[i].0;
            let kind =
                self.plan.districts[self.plan.blocks[self.plan.lots[lot].block].district].kind;
            add_building(&mut b, &self.buildings[i], kind, self.look_of(i));
        }
        if !b.is_empty() {
            self.cell_entities[cell] = Some(world.spawn(b.mesh));
        }
    }
}

fn main() -> EngineResult<()> {
    let seed = std::env::var("HW_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1u64);
    let view = std::env::var("HW_VIEW").unwrap_or_else(|_| "overview".into());
    let name = std::env::var("HW_NAME").unwrap_or_else(|_| "Hero".into());
    let mut session = match std::env::var("HW_JOIN") {
        Ok(addr) => Session::join(&addr, &name)
            .map_err(|e| EngineError::InvalidValue(format!("cannot join {addr}: {e}")))?,
        Err(_) => {
            let host = std::env::var_os("HW_HOST").is_some();
            Session::single(Seed(seed), &name, host)
                .map_err(|e| EngineError::InvalidValue(format!("cannot open port: {e}")))?
        }
    };
    // A joining player builds the city once the host has said which world it is.
    let seed = session.wait_for_seed().unwrap_or(seed);
    let mut city = City::new(Seed(seed));
    if let Some(level) = std::env::var("HW_CUT_LEVEL")
        .ok()
        .and_then(|s| s.parse().ok())
    {
        city.cut_level = level;
    }
    println!(
        "HeroWorld seed {seed}: {} buildings. Right mouse to look, WASD to fly, C cut-open, F focus, Z/X floor.",
        city.buildings.len()
    );
    let mut camera = match view.as_str() {
        "street" => FlyCamera::new((-60.0, 6.0, 140.0), 160.0, 8.0),
        "heroes" => FlyCamera::new((-4.0, 6.0, 48.0), 160.0, -14.0),
        "cutaway" => {
            let f = city.cut_focus.as_vec2();
            FlyCamera::new((f.x - 55.0, 70.0, f.y + 55.0), 135.0, -45.0)
        }
        _ => FlyCamera::new((-900.0, 700.0, 900.0), 135.0, -30.0),
    }
    .with_speed(40.0);
    camera.fast = 8.0;
    let mut hero_entities: BTreeMap<hw_core::NetId, EntityId> = BTreeMap::new();
    let mut tick_clock = 0.0f64;

    Engine::run("HeroWorld", move |world, frame| {
        if frame.first {
            world.set_view_distance(6000.0)?;
            world.set_physical_sky(Some(PhysicalSky::at_hour(10.5)));
            world.spawn(ground_mesh(&city.plan));
            for c in 0..city.plan.cells.len() {
                city.build_cell(world, c);
            }
        }
        let mut changed: BTreeSet<usize> = BTreeSet::new();
        if frame.input.pressed(Key::C) {
            city.cut_on = !city.cut_on;
            changed.extend(city.cells_near(city.cut_focus));
        }
        if frame.input.pressed(Key::F) {
            if let Some(hit) = ground_hit(camera.position, camera.forward()) {
                changed.extend(city.cells_near(city.cut_focus));
                city.cut_focus = DVec2::new(hit.x as f64, hit.z as f64);
                city.cut_on = true;
                changed.extend(city.cells_near(city.cut_focus));
            }
        }
        if frame.input.pressed(Key::Z) && city.cut_level > 0 {
            city.cut_level -= 1;
            changed.extend(city.cells_near(city.cut_focus));
        }
        if frame.input.pressed(Key::X) && city.cut_level < 40 {
            city.cut_level += 1;
            changed.extend(city.cells_near(city.cut_focus));
        }
        for c in changed {
            city.build_cell(world, c);
        }
        camera.update(world, frame);
        camera.position = camera.position.max(Vec3::new(f32::MIN, 1.8, f32::MIN));

        // Fixed ticks for the game, however fast frames come.
        let key = |k: Key| if frame.input.down(k) { 1.0f32 } else { 0.0 };
        let input = hw_net::InputState {
            move_x: key(Key::L) - key(Key::J),
            move_z: key(Key::K) - key(Key::I),
            jump: frame.input.down(Key::U),
        };
        tick_clock += frame.dt as f64;
        let mut ticks = 0;
        while tick_clock >= hw_sim::DT && ticks < 5 {
            tick_clock -= hw_sim::DT;
            ticks += 1;
            session.tick(input);
        }
        if ticks == 5 {
            tick_clock = 0.0;
        }
        // Show heroes where the server says they are (smoothing comes later).
        let heroes = session.entities();
        hero_entities.retain(|id, e| {
            let keep = heroes.iter().any(|h| h.id == *id);
            if !keep {
                world.despawn(*e);
            }
            keep
        });
        for h in &heroes {
            let place = Place::new(h.pos[0] as f32, h.pos[1] as f32, h.pos[2] as f32);
            match hero_entities.get(&h.id) {
                Some(&e) => world.set_place(e, place)?,
                None => {
                    let mine = session.my_hero() == Some(h.id);
                    let e = world.place(citymesh::hero_mesh(mine), place)?;
                    hero_entities.insert(h.id, e);
                }
            }
        }
        Ok(())
    })
}
