//! HeroWorld: the game client, with the server built in for single player.
//!
//! Milestone 0: fly over a generated city. Buildings around a focus point
//! can be cut open to show their rooms.
//!
//! Controls: you play your hero in third person. Hold the right mouse button
//! (or press P to lock the pointer) and move the mouse to look, WASD to run,
//! Space to jump, R/T to zoom in and out (all the way in is first person).
//! Tab switches to a free-flying camera and back. C cuts buildings open
//! around where you look (F moves the cut there), Z/X pick the floor.
//!
//! By default the server runs inside the game (single player). `HW_HOST=1`
//! also opens it to friends on UDP port 7777; `HW_JOIN=host:port` joins
//! someone else's world instead.
//!
//! Environment: `HW_SEED` (world seed, default 1); `HW_NAME`; `HW_VIEW` =
//! `hero` (default), `overview`, `street` or `cutaway` picks the starting
//! camera (handy with `ENGINE_SCREENSHOT`); `HW_CUT_LEVEL` the floor shown
//! cut open; `HW_WALK=1` makes the hero run forward on its own (for tests).

mod citymesh;
mod session;

use citymesh::{add_building, ground_hit, ground_mesh, Look, MeshBuilder};
use engine::prelude::*;
use glam::{DVec2, Vec3};
use hw_core::Seed;
use hw_gen::building::BuildingPlan;
use hw_gen::city::CityPlan;
use hw_sim::collision::{Obstacles, BODY_HEIGHT};
use hw_sim::GeneratedCity;
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
    fn new(generated: &GeneratedCity) -> Self {
        let plan = generated.plan.clone();
        let buildings = generated.buildings.clone();
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
            cut_on: false,
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

/// The camera that follows the hero: orbit angles and distance.
struct HeroCamera {
    yaw: f32,
    pitch: f32,
    distance: f32,
}

impl HeroCamera {
    const MAX_DISTANCE: f32 = 14.0;

    fn update(&mut self, world: &mut World, frame: &Frame, target: Vec3, solids: &Obstacles) {
        let input = &frame.input;
        if input.mouse_down(MouseButton::Right) || world.pointer_lock() {
            let (yaw, pitch) = input.look_delta(0.15);
            self.yaw = (self.yaw + yaw).rem_euclid(360.0);
            self.pitch = (self.pitch + pitch).clamp(-80.0, 80.0);
        }
        let zoom = input.axis(Key::R, Key::T) * frame.dt.min(0.1) * 12.0;
        self.distance = (self.distance + zoom).clamp(0.0, Self::MAX_DISTANCE);
        let head = target + Vec3::Y * (BODY_HEIGHT as f32 - 0.15);
        let dir = Camera::direction(self.yaw, self.pitch);
        // Pull the camera in front of walls between it and the hero.
        let wanted = self.distance;
        let mut d = wanted;
        let step = 0.25;
        let mut t = 0.3;
        while t < wanted {
            let p = head - dir * t;
            let floor = solids.floor_under(DVec2::new(p.x as f64, p.z as f64), f64::MAX);
            if (p.y as f64) < floor + 0.2 {
                d = (t - step).max(0.0);
                break;
            }
            t += step;
        }
        let eye = if d < 0.4 { head } else { head - dir * d };
        world.look_first_person(eye, self.yaw, self.pitch);
    }
}

fn main() -> EngineResult<()> {
    let env_num = |name: &str| std::env::var(name).ok().and_then(|s| s.parse::<u64>().ok());
    let view = std::env::var("HW_VIEW").unwrap_or_else(|_| "hero".into());
    let name = std::env::var("HW_NAME").unwrap_or_else(|_| "Hero".into());
    let auto_walk = std::env::var_os("HW_WALK").is_some();
    let io = |e: std::io::Error| EngineError::InvalidValue(e.to_string());

    // A joining player learns the world seed from the host first.
    let (mut session, generated) = match std::env::var("HW_JOIN") {
        Ok(addr) => {
            let (mut session, seed) = Session::join(&addr, &name).map_err(io)?;
            let generated = GeneratedCity::generate(Seed(seed));
            session.attach_city(&generated);
            (session, generated)
        }
        Err(_) => {
            let seed = Seed(env_num("HW_SEED").unwrap_or(1));
            let generated = GeneratedCity::generate(seed);
            let open = std::env::var_os("HW_HOST").is_some();
            (
                Session::single(seed, &generated, &name, open).map_err(io)?,
                generated,
            )
        }
    };
    let solids = generated.obstacles.clone();
    let mut city = City::new(&generated);
    if view == "cutaway" {
        city.cut_on = true;
    }
    if let Some(level) = env_num("HW_CUT_LEVEL") {
        city.cut_level = level as u32;
    }
    println!(
        "HeroWorld: {} buildings. Right mouse to look, WASD to run, Space to jump, Tab for the free camera.",
        city.buildings.len()
    );
    let mut fly = match view.as_str() {
        "street" => FlyCamera::new((-60.0, 6.0, 140.0), 160.0, 8.0),
        "cutaway" => {
            let f = city.cut_focus.as_vec2();
            FlyCamera::new((f.x - 55.0, 70.0, f.y + 55.0), 135.0, -45.0)
        }
        _ => FlyCamera::new((-900.0, 700.0, 900.0), 135.0, -30.0),
    }
    .with_speed(40.0);
    fly.fast = 8.0;
    let mut flying = view != "hero";
    let mut hero_cam = HeroCamera {
        yaw: 30.0,
        pitch: -12.0,
        distance: 7.0,
    };
    let mut hero_entities: BTreeMap<hw_core::NetId, EntityId> = BTreeMap::new();
    let mut facings: BTreeMap<hw_core::NetId, f32> = BTreeMap::new();
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
        let input = &frame.input;
        if input.pressed(Key::Tab) {
            flying = !flying;
            if flying {
                // Start the free camera where the hero camera was.
                fly.yaw_deg = hero_cam.yaw;
                fly.pitch_deg = hero_cam.pitch;
            }
        }
        if input.pressed(Key::P) {
            let locked = world.pointer_lock();
            world.set_pointer_lock(!locked);
        }

        // Cut-open view.
        let mut changed: BTreeSet<usize> = BTreeSet::new();
        if input.pressed(Key::C) {
            city.cut_on = !city.cut_on;
            changed.extend(city.cells_near(city.cut_focus));
        }
        let look_from = world.camera().eye();
        let look_dir = (world.camera().target() - look_from).normalize_or_zero();
        if input.pressed(Key::F) {
            if let Some(hit) = ground_hit(look_from, look_dir) {
                changed.extend(city.cells_near(city.cut_focus));
                city.cut_focus = DVec2::new(hit.x as f64, hit.z as f64);
                city.cut_on = true;
                changed.extend(city.cells_near(city.cut_focus));
            }
        }
        if input.pressed(Key::Z) && city.cut_level > 0 {
            city.cut_level -= 1;
            changed.extend(city.cells_near(city.cut_focus));
        }
        if input.pressed(Key::X) && city.cut_level < 40 {
            city.cut_level += 1;
            changed.extend(city.cells_near(city.cut_focus));
        }
        for c in changed {
            city.build_cell(world, c);
        }

        // Hero input, relative to where the camera faces.
        let mut hero_input = hw_net::InputState::default();
        if !flying {
            let mut dir = input.move_dir_xz(hero_cam.yaw);
            if auto_walk {
                dir = Camera::facing_xz(hero_cam.yaw);
            }
            hero_input.move_x = dir.x;
            hero_input.move_z = dir.z;
            hero_input.jump = input.down(Key::Space);
        }

        // Fixed ticks for the game, however fast frames come.
        tick_clock += frame.dt as f64;
        let mut ticks = 0;
        while tick_clock >= hw_sim::DT && ticks < 5 {
            tick_clock -= hw_sim::DT;
            ticks += 1;
            session.tick(hero_input);
        }
        if ticks == 5 {
            tick_clock = 0.0;
        }

        // Draw heroes.
        let heroes = session.entities(frame.dt as f64);
        facings.retain(|id, _| heroes.iter().any(|h| h.id == *id));
        hero_entities.retain(|id, e| {
            let keep = heroes.iter().any(|h| h.id == *id);
            if !keep {
                world.despawn(*e);
            }
            keep
        });
        let mine = session.my_hero();
        for h in &heroes {
            let vel = Vec3::from_array(h.vel);
            // Face where you run; keep the last heading when standing.
            let facing = facings.entry(h.id).or_insert(0.0);
            if vel.x.abs() + vel.z.abs() > 0.3 {
                *facing = vel.x.atan2(vel.z).to_degrees();
            }
            let facing = *facing;
            let place =
                Place::new(h.pos[0] as f32, h.pos[1] as f32, h.pos[2] as f32).with_yaw_deg(facing);
            match hero_entities.get(&h.id) {
                Some(&e) => world.set_place(e, place)?,
                None => {
                    let e = world.place(citymesh::hero_mesh(mine == Some(h.id)), place)?;
                    hero_entities.insert(h.id, e);
                }
            }
        }

        // Camera.
        let me = session.my_position(&heroes);
        match (flying, me) {
            (false, Some(p)) => hero_cam.update(world, frame, p.as_vec3(), &solids),
            _ => {
                fly.update(world, frame);
                fly.position = fly.position.max(Vec3::new(f32::MIN, 1.8, f32::MIN));
            }
        }
        Ok(())
    })
}
