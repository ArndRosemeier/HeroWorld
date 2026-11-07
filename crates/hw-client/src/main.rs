//! HeroWorld: the game client, with the server built in for single player.
//!
//! The world is one generated town on its hills, rivers and coast: Engine
//! streams the ground and water, the town's roads and buildings are built
//! in cells around the camera. Buildings around a focus point can be cut
//! open to show their rooms.
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
//! Environment: `HW_SEED` (world seed, default 1); `HW_POP` (people in the
//! town: 300 a hamlet, 40000 the default, millions a metropolis; joining
//! players get the host's); `HW_NAME`; `HW_VIEW` =
//! `hero` (default), `inside`, `sky`, `overview`, `street`, `river`, `cutaway`, `park` or
//! `props` picks the starting camera (handy with `ENGINE_SCREENSHOT`; `props` looks at the
//! nearest prop whose kind contains `HW_PROP`, a car by default); `HW_CUT_LEVEL` the floor shown
//! cut open; `HW_WALK=1` makes the hero run forward on its own (for tests);
//! `ENGINE_ZFIGHT=1` marks z-fighting with magenta posts (the `zfight` tool
//! checks a whole city without the game).

mod session;

use engine::prelude::*;
use engine::stream::{StreamSettings, WorldStream};
use glam::{DVec2, Vec3};
use hw_client::citymesh;
use hw_client::townmesh::{TownGround, TownView};
use hw_core::Seed;
use hw_sim::collision::{Obstacles, BODY_HEIGHT};
use hw_sim::GeneratedTown;
use session::Session;
use std::collections::{BTreeMap, BTreeSet};

/// Milliseconds per frame for building town cells as you move.
const BUILD_BUDGET_MS: f64 = 6.0;
/// Cells within this distance of the cut focus are redrawn when it changes.
const CUT_REACH: f64 = hw_client::townmesh::CUT_RADIUS + 120.0;

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
            if !hidden
                && (solids.solid_at(q) || q.y < solids.ground_at(glam::DVec2::new(q.x, q.z)) + 0.2)
            {
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
    let t0 = std::time::Instant::now();
    let (mut session, generated) = match std::env::var("HW_JOIN") {
        Ok(addr) => {
            let (mut session, seed, people) = Session::join(&addr, &name).map_err(io)?;
            let generated = GeneratedTown::generate(Seed(seed), people);
            session.attach_town(&generated);
            (session, generated)
        }
        Err(_) => {
            let seed = Seed(env_num("HW_SEED").unwrap_or(1));
            let people = env_num("HW_POP")
                .map(|p| p as f64)
                .unwrap_or(hw_sim::DEFAULT_POPULATION);
            let generated = GeneratedTown::generate(seed, people);
            let open = std::env::var_os("HW_HOST").is_some();
            (
                Session::single(seed, &generated, &name, open).map_err(io)?,
                generated,
            )
        }
    };
    let solids = generated.obstacles.clone();
    let mut town = TownView::new(generated.world.clone());
    let ground = {
        let w = generated.world.clone();
        move |p: DVec2| w.ground(p).max(hw_gen::urban::world::WATER_LEVEL)
    };
    if view == "cutaway" {
        town.cut_on = true;
    }
    if let Some(level) = env_num("HW_CUT_LEVEL") {
        town.cut_level = level as u32;
    }
    println!(
        "HeroWorld {}: a town of {:.0} people, {} buildings, planned in {:.1} s. Right mouse to look, WASD to run, Space to jump, V to fly, Tab for the free camera.",
        hw_core::version(),
        generated.population(),
        town.building_count(),
        t0.elapsed().as_secs_f64()
    );
    let (spawn, spawn_dir) = generated.world.spawn_line();
    let mut hero_cam_yaw = 30.0f32;
    let mut fly = match view.as_str() {
        "street" => {
            let p = spawn - spawn_dir * 8.0;
            let yaw = spawn_dir.x.atan2(spawn_dir.y).to_degrees() as f32;
            FlyCamera::new((p.x as f32, ground(p) as f32 + 6.0, p.y as f32), yaw, 8.0)
        }
        "props" | "park" => {
            // Close to parked cars (or a park fountain), for screenshots.
            use hw_gen::urban::props::PropKind;
            let want = |k: &PropKind| {
                if view == "park" {
                    matches!(k, PropKind::Fountain)
                } else {
                    // HW_PROP picks a kind by name ("Hover", "Atomic", ...).
                    let name = std::env::var("HW_PROP").unwrap_or_else(|_| "Car".into());
                    format!("{k:?}").contains(&name)
                }
            };
            let target = generated
                .world
                .props
                .iter()
                .filter(|p| want(&p.kind))
                .min_by(|a, b| a.pos.distance(spawn).total_cmp(&b.pos.distance(spawn)));
            let (c, side) = target.map_or((spawn, spawn_dir.perp()), |t| (t.pos, t.dir.perp()));
            let back = if view == "park" { 22.0 } else { 9.0 };
            let p = c + side * back + side.perp() * back * 0.5;
            let d = c - p;
            let yaw = d.x.atan2(d.y).to_degrees() as f32;
            let up = if view == "park" { 9.0 } else { 3.0 };
            FlyCamera::new((p.x as f32, ground(p) as f32 + up, p.y as f32), yaw, -14.0)
        }
        "cutaway" => {
            let f = town.cut_focus;
            let h = ground(f) as f32;
            FlyCamera::new(
                (f.x as f32 - 55.0, h + 70.0, f.y as f32 + 55.0),
                135.0,
                -45.0,
            )
        }
        "river" => {
            // Looking at the first river near the town (for screenshots).
            let r = generated.world.town.land.rivers.first().map_or(spawn, |r| {
                *r.pts
                    .iter()
                    .min_by(|a, b| a.distance(spawn).total_cmp(&b.distance(spawn)))
                    .expect("river has points")
            });
            let p = r + DVec2::new(-120.0, 120.0);
            FlyCamera::new(
                (p.x as f32, ground(r) as f32 + 90.0, p.y as f32),
                135.0,
                -30.0,
            )
        }
        _ => {
            let p = spawn + DVec2::new(-900.0, 900.0);
            FlyCamera::new(
                (p.x as f32, ground(spawn) as f32 + 700.0, p.y as f32),
                135.0,
                -30.0,
            )
        }
    }
    .with_speed(40.0);
    fly.fast = 8.0;
    let mut flying = !matches!(view.as_str(), "hero" | "inside" | "sky");
    if view == "inside" {
        // Start in the lobby of the tallest building near the start.
        let tower = town.tallest_near(spawn, 400.0);
        if let Some(i) = tower {
            let plan = town.plan(i);
            if plan.core.is_some() {
                let base = generated.world.buildings[i].base;
                let lobby = plan.floors[0]
                    .rooms
                    .iter()
                    .find(|r| r.kind == hw_gen::building::RoomKind::Lobby)
                    .map(|r| r.rect.center())
                    .unwrap_or(plan.floors[0].outline.center());
                let p = plan
                    .site
                    .to_city(glam::DVec3::new(lobby.x, base + 0.1, lobby.y));
                session.teleport(p);
                let along = plan.site.inward;
                // Look into the building from behind the hero.
                hero_cam_yaw = along.x.atan2(along.y).to_degrees() as f32;
            }
        }
    }
    if view == "sky" {
        // Hovering above the town centre (with HW_FLY or V held on).
        let p = spawn + DVec2::new(-40.0, 40.0);
        session.teleport(glam::DVec3::new(p.x, ground(p) + 260.0, p.y));
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

    // HW_WHATS_NEW=1 opens the list at start (for screenshots).
    let mut whats_new = std::env::var_os("HW_WHATS_NEW").is_some();
    Engine::run(
        format!("HeroWorld {}", hw_core::version()),
        move |world, frame| {
            if frame.first {
                world.set_view_distance(6000.0)?;
                world.set_physical_sky(Some(PhysicalSky::at_hour(10.5)));
                // A wide, coarse water window, so rivers and lakes show
                // across the town and not only around the camera.
                let settings = StreamSettings {
                    water_window: 1536.0,
                    water_cell: 4.0,
                    ..StreamSettings::default()
                };
                let stream = WorldStream::new(TownGround(generated.world.clone()), settings)
                    .with_terrain(|t| t.with_beach(1.8));
                world.set_stream(Some(stream))?;
                world.prepare_stream(20.0);
                let eye = world.camera().eye();
                town.build_all(world, DVec2::new(eye.x as f64, eye.z as f64));
                let t = std::time::Instant::now();
                let props = hw_client::propview::PropView::spawn(world, &generated.world)?;
                eprintln!(
                    "{} props in {:.2} s",
                    props.count,
                    t.elapsed().as_secs_f64()
                );
            }
            version_corner(&frame.ui, &mut whats_new);
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
            let mut changed = BTreeSet::new();
            if input.pressed(Key::C) {
                town.cut_on = !town.cut_on;
                changed.extend(town.cells_near(town.cut_focus, CUT_REACH));
            }
            let look_from = world.camera().eye();
            let look_dir = (world.camera().target() - look_from).normalize_or_zero();
            if input.pressed(Key::F) {
                if let Some(hit) = town.ground_hit(look_from, look_dir) {
                    changed.extend(town.cells_near(town.cut_focus, CUT_REACH));
                    town.cut_focus = hit;
                    town.cut_on = true;
                    changed.extend(town.cells_near(town.cut_focus, CUT_REACH));
                }
            }
            if input.pressed(Key::Z) && town.cut_level > 0 {
                town.cut_level -= 1;
                changed.extend(town.cells_near(town.cut_focus, CUT_REACH));
            }
            if input.pressed(Key::X) && town.cut_level < 40 {
                town.cut_level += 1;
                changed.extend(town.cells_near(town.cut_focus, CUT_REACH));
            }
            town.rebuild(world, changed);

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
                let place = Place::new(h.pos[0] as f32, h.pos[1] as f32, h.pos[2] as f32)
                    .with_yaw_deg(facing);
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
            if inside != town.inside {
                let cells: BTreeSet<_> = [town.inside, inside]
                    .into_iter()
                    .flatten()
                    .map(|i| town.cell_of_building(i.building))
                    .collect();
                town.inside = inside;
                town.rebuild(world, cells);
            }
            let open_above = inside.and_then(|i| {
                let (plan, base) = solids.building(i.building)?;
                let f = &plan.floors[i.level as usize];
                Some(base + f.elevation + 2.6)
            });
            match (flying, me) {
                (false, Some(p)) => hero_cam.update(world, frame, p.as_vec3(), &solids, open_above),
                _ => {
                    fly.update(world, frame);
                    let q = DVec2::new(fly.position.x as f64, fly.position.z as f64);
                    fly.position.y = fly.position.y.max(ground(q) as f32 + 1.8);
                }
            }
            // Roads and buildings around the camera, full detail close by.
            let eye = world.camera().eye();
            town.update(
                world,
                DVec2::new(eye.x as f64, eye.z as f64),
                BUILD_BUDGET_MS,
            );
            Ok(())
        },
    )
}

/// The version at the top right, with a "What's new" button and its list.
fn version_corner(ui: &engine::ui::UiFrame, open: &mut bool) {
    use engine::ui::egui;
    egui::Area::new(egui::Id::new("version"))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-10.0, 10.0))
        .show(ui.ctx(), |ui| {
            // A dark backing keeps it readable against a bright sky.
            egui::Frame::new()
                .fill(egui::Color32::from_black_alpha(150))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("HeroWorld {}", hw_core::version()))
                                .color(egui::Color32::WHITE),
                        );
                        if ui.button("What's new").clicked() {
                            *open = true;
                        }
                    });
                });
        });
    ui.modal("What's new", open, |panel, open| {
        egui::ScrollArea::vertical()
            .max_height(420.0)
            .show(panel.ui(), |ui| {
                ui.set_max_width(520.0);
                for line in hw_core::WHATS_NEW
                    .lines()
                    .skip_while(|l| !l.starts_with("## "))
                {
                    if let Some(h) = line.strip_prefix("## ") {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new(h).strong().size(16.0));
                    } else if let Some(item) = line.strip_prefix("- ") {
                        ui.label(format!("•  {item}"));
                    } else if !line.trim().is_empty() {
                        ui.label(line);
                    }
                }
            });
        if panel.button("Close") {
            *open = false;
        }
    });
}
