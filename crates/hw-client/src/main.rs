//! HeroWorld: the game client, with the server built in for single player.
//!
//! Milestone 0: fly over a generated city. Buildings around a focus point
//! can be cut open to show their rooms.
//!
//! Controls: you play your hero in third person. Hold the right mouse button
//! (or press P to lock the pointer) and move the mouse to look, WASD to run,
//! Space to jump, R/T to zoom in and out (all the way in is first person).
//! V to fly: steer with WASD and the camera, Space climbs, Ctrl sinks,
//! Shift goes faster; V again drops you.
//! Buildings are open: walk in through a door, and in the lift (the core in
//! the middle) hold Space to rise and Ctrl to sink; step out at any floor.
//! Tab switches to a free-flying camera and back. C cuts buildings open
//! around where you look (F moves the cut there), Z/X pick the floor.
//!
//! By default the server runs inside the game (single player). `HW_HOST=1`
//! also opens it to friends on UDP port 7777; `HW_JOIN=host:port` joins
//! someone else's world instead.
//!
//! Environment: `HW_SEED` (world seed, default 1); `HW_NAME`; `HW_VIEW` =
//! `hero` (default), `inside`, `sky`, `overview`, `street` or `cutaway` picks the starting
//! camera (handy with `ENGINE_SCREENSHOT`); `HW_CUT_LEVEL` the floor shown
//! cut open; `HW_WALK=1` makes the hero run forward on its own (for tests);
//! `HW_ZFIGHT=1` marks z-fighting with magenta posts (the `zfight` tool
//! checks a whole city without the game).

mod session;

use engine::prelude::*;
use glam::{DVec2, Vec3};
use hw_client::buildingmesh::Detail;
use hw_client::citymesh::{self, add_building, ground_hit, ground_mesh, Look, MeshBuilder};
use hw_client::zfight;
use hw_core::Seed;
use hw_gen::building::BuildingPlan;
use hw_gen::city::CityPlan;
use hw_sim::collision::{Obstacles, BODY_HEIGHT};
use hw_sim::GeneratedCity;
use session::Session;
use std::collections::{BTreeMap, BTreeSet};

/// Faces per city mesh, well inside a GPU buffer.
const MESH_FACES: usize = 250_000;

/// Buildings within about this distance of the camera get full detail.
const NEAR_DETAIL: f64 = 220.0;

/// Buildings within this distance of the focus are cut open.
const CUT_RADIUS: f64 = 110.0;

struct City {
    plan: CityPlan,
    /// One plan per building site, same order as `plan.buildings`.
    buildings: Vec<BuildingPlan>,
    /// Buildings per boulevard cell.
    by_cell: Vec<Vec<usize>>,
    /// Engine entity per cell.
    cell_entities: Vec<Vec<EntityId>>,
    cut_on: bool,
    cut_focus: DVec2,
    cut_level: u32,
    /// The building and floor our hero is in: drawn open at that floor.
    inside: Option<hw_sim::collision::Inside>,
    /// `HW_ZFIGHT=1`: mark z-fighting with magenta posts.
    mark_zfighting: bool,
    /// Centre and radius of each cell's buildings.
    cell_bounds: Vec<(DVec2, f64)>,
    /// Cells drawn in near detail.
    near: BTreeSet<usize>,
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
            cell_entities: vec![Vec::new(); cells],
            cut_on: false,
            cut_focus: DVec2::ZERO,
            cut_level: 1,
            inside: None,
            mark_zfighting: std::env::var_os("HW_ZFIGHT").is_some(),
            cell_bounds: Vec::new(),
            near: BTreeSet::new(),
        }
        .with_cell_bounds()
        .focused_on_centre()
    }

    fn with_cell_bounds(mut self) -> Self {
        self.cell_bounds = (0..self.by_cell.len())
            .map(|c| {
                let pts: Vec<DVec2> = self.by_cell[c].iter().map(|&i| self.centre_of(i)).collect();
                if pts.is_empty() {
                    return (DVec2::ZERO, 0.0);
                }
                let centre = pts.iter().sum::<DVec2>() / pts.len() as f64;
                let r = pts.iter().map(|p| p.distance(centre)).fold(0.0, f64::max) + 40.0;
                (centre, r)
            })
            .collect();
        self
    }

    /// Cells that should be drawn in near detail from `eye`.
    fn near_cells(&self, eye: DVec2) -> BTreeSet<usize> {
        (0..self.cell_bounds.len())
            .filter(|&c| {
                let (centre, r) = self.cell_bounds[c];
                !self.by_cell[c].is_empty() && centre.distance(eye) < NEAR_DETAIL + r
            })
            .collect()
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
        if let Some(i) = self.inside {
            if i.building == building {
                return Look::Cut { level: i.level };
            }
        }
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

    fn cell_of(&self, building: usize) -> usize {
        let lot = self.plan.buildings[building].0;
        self.plan.blocks[self.plan.lots[lot].block].cell
    }

    fn build_cell(&mut self, world: &mut World, cell: usize) {
        for e in self.cell_entities[cell].drain(..) {
            world.despawn(e);
        }
        let detail = if self.near.contains(&cell) {
            Detail::Near
        } else {
            Detail::Far
        };
        // A dense downtown cell can outgrow a GPU buffer (256 MB by default),
        // so a cell becomes several meshes of at most MESH_FACES faces each.
        let mut b = MeshBuilder::new();
        let buildings = self.by_cell[cell].clone();
        for (n, &i) in buildings.iter().enumerate() {
            let lot = self.plan.buildings[i].0;
            let kind =
                self.plan.districts[self.plan.blocks[self.plan.lots[lot].block].district].kind;
            add_building(&mut b, &self.buildings[i], kind, self.look_of(i), detail);
            if b.face_count() > MESH_FACES || n + 1 == buildings.len() {
                let full = std::mem::take(&mut b);
                self.spawn_part(world, cell, full);
            }
        }
    }

    fn spawn_part(&mut self, world: &mut World, cell: usize, b: MeshBuilder) {
        if b.is_empty() {
            return;
        }
        let mut mesh = b.finish();
        if self.mark_zfighting {
            let n = zfight::mark(&mut mesh);
            if n > 0 {
                println!("z-fighting: {n} spots in cell {cell}");
            }
        }
        self.cell_entities[cell].push(world.spawn(mesh));
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

    /// `open_above`: height above which nothing is drawn (the hero's floor
    /// ceiling when inside a building), so the camera may go there.
    fn update(
        &mut self,
        world: &mut World,
        frame: &Frame,
        target: Vec3,
        solids: &Obstacles,
        open_above: Option<f64>,
    ) {
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
            let q = glam::DVec3::new(p.x as f64, p.y as f64, p.z as f64);
            let hidden = open_above.is_some_and(|y| q.y > y);
            if !hidden && (solids.solid_at(q) || q.y < solids.ground + 0.2) {
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
        "HeroWorld: {} buildings. Right mouse to look, WASD to run, Space to jump, V to fly, Tab for the free camera.",
        city.buildings.len()
    );
    let mut hero_cam_yaw = 30.0f32;
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
    let mut flying = !matches!(view.as_str(), "hero" | "inside" | "sky");
    if view == "inside" {
        // Start in the lobby of the tallest tower near the centre.
        let tower = (0..city.buildings.len())
            .filter(|&i| city.centre_of(i).length() < 300.0 && city.buildings[i].core.is_some())
            .max_by_key(|&i| city.buildings[i].floors.len());
        if let Some(i) = tower {
            let plan = &city.buildings[i];
            let lobby = plan.floors[0]
                .rooms
                .iter()
                .find(|r| r.kind == hw_gen::building::RoomKind::Lobby)
                .map(|r| r.rect.center())
                .unwrap_or(plan.floors[0].outline.center());
            let p = plan
                .site
                .to_city(glam::DVec3::new(lobby.x, hw_sim::GROUND_Y + 0.1, lobby.y));
            session.teleport(p);
            let along = plan.site.inward;
            // Look into the building from behind the hero.
            hero_cam_yaw = along.x.atan2(along.y).to_degrees() as f32;
        }
    }
    if view == "sky" {
        // Hovering above the city centre (with HW_FLY or V held on).
        session.teleport(glam::DVec3::new(-40.0, 260.0, 40.0));
        hero_cam_yaw = 135.0;
    }
    let mut hero_cam = HeroCamera {
        yaw: hero_cam_yaw,
        pitch: match view.as_str() {
            "inside" => -35.0,
            "sky" => -22.0,
            _ => -12.0,
        },
        distance: 7.0,
    };
    let mut hero_entities: BTreeMap<hw_core::NetId, EntityId> = BTreeMap::new();
    let mut facings: BTreeMap<hw_core::NetId, f32> = BTreeMap::new();
    let mut tick_clock = 0.0f64;
    // The hero's flight toggle (V); sent with every input.
    let mut hero_flies = std::env::var_os("HW_FLY").is_some() || view == "sky";

    Engine::run("HeroWorld", move |world, frame| {
        if frame.first {
            world.set_view_distance(6000.0)?;
            world.set_physical_sky(Some(PhysicalSky::at_hour(10.5)));
            let mut ground = ground_mesh(&city.plan);
            if city.mark_zfighting {
                println!(
                    "z-fighting: {} spots on the ground",
                    zfight::mark(&mut ground)
                );
            }
            world.spawn(ground);
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
            if input.pressed(Key::V) {
                hero_flies = !hero_flies;
            }
            let mut dir = input.move_dir_xz(hero_cam.yaw);
            if auto_walk {
                dir = Camera::facing_xz(hero_cam.yaw);
            }
            if hero_flies {
                // Forward follows the camera up and down, so you fly where you look.
                let flat = Camera::facing_xz(hero_cam.yaw);
                let ahead = dir.dot(flat);
                dir = dir - flat * ahead
                    // The camera looks down at the hero a little; level that out.
                    + Camera::direction(hero_cam.yaw, hero_cam.pitch + 12.0) * ahead;
                dir = dir.clamp_length_max(1.0);
            }
            hero_input.move_x = dir.x;
            hero_input.move_y = if hero_flies { dir.y } else { 0.0 };
            hero_input.move_z = dir.z;
            hero_input.jump = input.down(Key::Space);
            hero_input.descend = input.down(Key::Ctrl);
            hero_input.fly = hero_flies;
            hero_input.boost = input.down(Key::Shift);
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
        let inside = me.and_then(|p| solids.inside(p));
        if inside != city.inside {
            let mut cells = BTreeSet::new();
            for i in [city.inside, inside].into_iter().flatten() {
                cells.insert(city.cell_of(i.building));
            }
            city.inside = inside;
            for c in cells {
                city.build_cell(world, c);
            }
        }
        let open_above = inside.map(|i| {
            let f = &city.buildings[i.building].floors[i.level as usize];
            hw_sim::BUILDING_BASE + f.elevation + 2.6
        });
        match (flying, me) {
            (false, Some(p)) => hero_cam.update(world, frame, p.as_vec3(), &solids, open_above),
            _ => {
                fly.update(world, frame);
                fly.position = fly.position.max(Vec3::new(f32::MIN, 1.8, f32::MIN));
            }
        }
        // Full detail around the camera.
        let eye = world.camera().eye();
        let near = city.near_cells(DVec2::new(eye.x as f64, eye.z as f64));
        if near != city.near {
            let changed: Vec<usize> = near.symmetric_difference(&city.near).copied().collect();
            city.near = near;
            for c in changed {
                city.build_cell(world, c);
            }
        }
        Ok(())
    })
}
