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
//! Shift goes faster; V again drops you. K knocks the people around you
//! over (a test of ragdolls; they get up again).
//! Buildings are open: walk in through a door, and in the lift (the core in
//! the middle) hold Space to rise and Ctrl to sink; step out at any floor.
//! Tab switches to a free-flying camera and back. C cuts buildings open
//! around where you look (F moves the cut there), Z/X pick the floor.
//!
//! By default the server runs inside the game (single player). `HW_HOST=1`
//! also opens it to friends: on UDP port 7777, and listed on GameHub with a
//! six-letter room code (shown top right) that works from anywhere through
//! GameHub's relay. `HW_JOIN=<code>` or `HW_JOIN=host:port` joins someone
//! else's world instead.
//!
//! The game opens on the title screen (the town at golden hour behind it,
//! built while you look); `HW_VIEW`, `HW_HOST` or `HW_JOIN` skip it,
//! `HW_TITLE=0` too, and `HW_VIEW=title` forces it (`HW_TITLE_TIME=<s>` sets
//! its clock, for screenshots). Your name, world and sound levels are
//! remembered between runs (see `profile.rs`).
//!
//! Environment: `HW_SEED` (world seed, default 1, or the one you last played); `HW_POP` (people in the
//! town: 300 a hamlet, 40000 the default, millions a metropolis; joining
//! players get the host's); `HW_NAME`; `HW_VIEW` =
//! `hero` (default), `inside`, `sky`, `overview`, `street`, `river`, `cutaway`, `park`, `pad`, `spire`, `lighthouse`, `quay` or
//! `props` picks the starting camera (handy with `ENGINE_SCREENSHOT`; `props` looks at the
//! nearest prop whose kind contains `HW_PROP`, a car by default); `HW_CUT_LEVEL` the floor shown
//! cut open; `HW_WALK=1` makes the hero run forward on its own (for tests);
//! `HW_WAIT_PEOPLE=1` waits at the start until every character is built
//! (for screenshots); `HW_PACKS` points at the Actors packs (default
//! `assets/packs`, fetched by `scripts/fetch-packs`);
//! `ENGINE_ZFIGHT=1` marks z-fighting with magenta posts (the `zfight` tool
//! checks a whole city without the game).

mod session;

use engine::prelude::*;
use engine::stream::{StreamSettings, WorldStream};
use glam::{DVec2, Vec3};
use hw_client::citymesh;
use hw_client::crowdview::CrowdView;
use hw_client::heroview::{tipped_origin, HeroAnim};
use hw_client::people::{self, People, Who};
use hw_client::profile::Profile;
use hw_client::title::{self, Choice, Title};
use hw_client::townmesh::{TownGround, TownView};
use hw_core::Seed;
use hw_sim::collision::{Obstacles, BODY_HEIGHT};
use hw_sim::GeneratedTown;
use session::Session;
use std::collections::{BTreeMap, BTreeSet};

/// Milliseconds per frame for building town cells as you move.
const BUILD_BUDGET_MS: f64 = 6.0;
/// Milliseconds per frame for building the town behind the title.
const TITLE_BUILD_MS: f64 = 20.0;
/// The hour the game is played at (the title is at golden hour).
const GAME_HOUR: f32 = 10.5;

/// The hour of day: `HW_HOUR` (0 to 24, e.g. 22 for night) or [`GAME_HOUR`].
fn game_hour() -> f32 {
    static HOUR: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *HOUR.get_or_init(|| {
        std::env::var("HW_HOUR")
            .ok()
            .and_then(|h| h.parse::<f32>().ok())
            .filter(|h| (0.0..=24.0).contains(h))
            .unwrap_or(GAME_HOUR)
    })
}
/// Seconds the camera takes to swoop from the title down to the hero.
const SWOOP_SECONDS: f32 = 2.4;
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
    let profile = Profile::load();
    let title_mode = std::env::var("HW_TITLE").map_or(true, |v| v != "0")
        && (view == "title"
            || (std::env::var_os("HW_VIEW").is_none()
                && std::env::var_os("HW_JOIN").is_none()
                && std::env::var_os("HW_HOST").is_none()));
    let mut name = std::env::var("HW_NAME").unwrap_or_else(|_| profile.name.clone());
    let auto_walk = std::env::var_os("HW_WALK").is_some();
    let io = |e: std::io::Error| EngineError::InvalidValue(e.to_string());

    // A joining player learns the world seed from the host first.
    let t0 = std::time::Instant::now();
    // On the title there is no session yet: the player starts one there.
    let people = env_num("HW_POP")
        .map(|p| p as f64)
        .unwrap_or(hw_sim::DEFAULT_POPULATION);
    let (mut session, mut generated, mut world_seed, mut world_people) =
        match std::env::var("HW_JOIN") {
            Ok(addr) if !title_mode => {
                let (mut session, seed, people) = Session::join(&addr, &name).map_err(io)?;
                let generated = GeneratedTown::generate(Seed(seed), people);
                session.attach_town(&generated);
                (Some(session), generated, seed, people)
            }
            _ => {
                let seed = Seed(env_num("HW_SEED").unwrap_or(profile.seed));
                let generated = GeneratedTown::generate(seed, people);
                let open = std::env::var_os("HW_HOST").is_some();
                let session = if title_mode {
                    None
                } else {
                    Some(Session::single(seed, &generated, &name, open).map_err(io)?)
                };
                (session, generated, seed.0, people)
            }
        };
    let mut solids = generated.obstacles.clone();
    let mut citizens = hw_sim::citizens::Citizens::new(generated.world.clone());
    // People from Actors, built on a worker thread from the packs.
    let actors = match people::packs_dir() {
        Some(dir) => {
            let p = People::start(dir);
            if std::env::var_os("HW_JOIN").is_none() {
                // Our own hero first (the local player is player 1).
                p.request(Who::Hero("player 1".into()), 0);
            }
            for look in 0..hw_sim::citizens::LOOKS {
                p.request(Who::Citizen(look), generated.world.seed.0);
            }
            Some(p)
        }
        None => {
            eprintln!(
                "No Actors packs found: heroes are drawn as placeholders and the streets stay empty. Run scripts/fetch-packs (start.bat does it)."
            );
            None
        }
    };
    let mut models: BTreeMap<Who, std::sync::Arc<AnimatedModel>> = BTreeMap::new();
    let mut asked: BTreeSet<Who> = [Who::Hero("player 1".into())].into();
    let mut crowd = CrowdView::new();
    let mut anims: BTreeMap<hw_core::NetId, HeroAnim> = BTreeMap::new();
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
        "pad" => {
            // Above the nearest rooftop taxi pad.
            let pad = generated
                .world
                .buildings
                .iter()
                .filter(|b| b.site.look().taxi_pad)
                .min_by(|a, b| {
                    let d = |x: &hw_gen::urban::world::WorldBuilding| x.site.origin.distance(spawn);
                    d(a).total_cmp(&d(b))
                });
            let (c, h) = pad.map_or((spawn, 20.0), |b| {
                let s = &b.site;
                let c = s.origin + s.along * s.size.x * 0.5 + s.inward * s.size.y * 0.5;
                let plan = hw_gen::building::BuildingPlan::generate(s);
                (c, b.base + plan.height())
            });
            let p = c + DVec2::new(-30.0, 30.0);
            FlyCamera::new((p.x as f32, h as f32 + 32.0, p.y as f32), 135.0, -42.0)
        }
        "spire" | "lighthouse" => {
            // Looking up at a landmark from across its plaza.
            use hw_gen::urban::landmarks::LandmarkKind;
            let kind = if view == "spire" {
                LandmarkKind::AetherSpire
            } else {
                LandmarkKind::Lighthouse
            };
            let l = hw_client::landmarkview::nearest(&generated.world, kind, spawn);
            let (c, base, h, face) = l.map_or((spawn, ground(spawn), 30.0, spawn_dir), |l| {
                // The lighthouse faces the land: look at it from the sea.
                let face = if view == "lighthouse" { -l.dir } else { l.dir };
                (l.pos, l.base, l.height, face)
            });
            // From above the rooftops, the landmark's whole height in view.
            let p = c + face * (h * 0.9 + 12.0) + face.perp() * h * 0.35;
            let d = c - p;
            let yaw = d.x.atan2(d.y).to_degrees() as f32;
            let eye = ground(p).max(base) + h * 0.5 + 6.0;
            let pitch = ((h * 0.4 - (eye - base)) / d.length()).atan().to_degrees() as f32;
            FlyCamera::new((p.x as f32, eye as f32, p.y as f32), yaw, pitch)
        }
        "quay" => {
            // Along a quay wall at the nearest steps down to the water.
            let st = generated
                .world
                .water_steps
                .iter()
                .min_by(|a, b| a.pos.distance(spawn).total_cmp(&b.pos.distance(spawn)));
            let (c, out, top) =
                st.map_or((spawn, spawn_dir, ground(spawn)), |s| (s.pos, s.out, s.top));
            let p = c + out * 9.0 + out.perp() * 14.0;
            let d = c - p;
            let yaw = d.x.atan2(d.y).to_degrees() as f32;
            FlyCamera::new((p.x as f32, top as f32 + 4.0, p.y as f32), yaw, -14.0)
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
    let mut flying = !matches!(view.as_str(), "hero" | "inside" | "sky" | "title");
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
                if let Some(s) = session.as_mut() {
                    s.teleport(p);
                }
                let along = plan.site.inward;
                // Look into the building from behind the hero.
                hero_cam_yaw = along.x.atan2(along.y).to_degrees() as f32;
            }
        }
    }
    if view == "sky" {
        // Hovering above the town centre (with HW_FLY or V held on).
        let p = spawn + DVec2::new(-40.0, 40.0);
        if let Some(s) = session.as_mut() {
            s.teleport(glam::DVec3::new(p.x, ground(p) + 260.0, p.y));
        }
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
    // Each hero's entity, and whether it is the Actors character yet.
    let mut hero_entities: BTreeMap<hw_core::NetId, (EntityId, bool)> = BTreeMap::new();
    let mut tick_clock = 0.0f64;
    // The hero's flight toggle (V); sent with every input.
    let mut hero_flies = std::env::var_os("HW_FLY").is_some() || view == "sky";

    // HW_WHATS_NEW=1 opens the list at start (for screenshots).
    let mut whats_new = std::env::var_os("HW_WHATS_NEW").is_some();
    // A reflection probe over the street around the camera, moved along in
    // steps, so chrome and glass reflect the buildings around them.
    let mut street_probe: Option<(engine::reflections::ReflectionProbeId, (i32, i32))> = None;
    // The title screen, while it shows; a town being generated for another
    // world (a new hero's, or a friend's); a joined session waiting for it.
    let mut title_screen = title_mode.then(|| Title::new(profile.clone()));
    let mut switching: Option<std::sync::mpsc::Receiver<(u64, f64, GeneratedTown)>> = None;
    let mut waiting: Option<Waiting> = None;
    // Props and landmarks of the current town.
    let mut extras = TownExtras::default();
    // From the title down to the hero: (start, eye, yaw, pitch).
    let mut swoop: Option<(f32, Vec3, f32, f32)> = None;
    let mut music_fade_at: Option<f32> = None;
    let shooting = std::env::var_os("ENGINE_SCREENSHOT").is_some();
    Engine::run(
        format!("HeroWorld {}", hw_core::version()),
        move |world, frame| {
            if frame.first {
                world.set_view_distance(6000.0)?;
                world.set_physical_sky(Some(PhysicalSky::at_hour(game_hour())));
                world.set_moons(moons())?;
                if let Some(t) = title_screen.as_mut() {
                    t.enter(world);
                    let (eye, yaw, pitch) = t.camera(&generated.world, &solids);
                    world.look_first_person(eye, yaw, pitch);
                }
                set_town_stream(world, &generated)?;
                // Behind the title the town builds bit by bit while you look.
                if title_screen.is_none() || shooting {
                    let eye = world.camera().eye();
                    town.build_all(world, DVec2::new(eye.x as f64, eye.z as f64));
                }
                extras = spawn_extras(world, &generated)?;
            }

            if let Some(t) = title_screen.as_mut() {
                // Another world finished generating: swap it in.
                if let Some((seed, people, g)) =
                    switching.as_ref().and_then(|rx| rx.try_recv().ok())
                {
                    switching = None;
                    town.clear(world);
                    extras.despawn(world);
                    world_seed = seed;
                    world_people = people;
                    generated = g;
                    solids = generated.obstacles.clone();
                    town = TownView::new(generated.world.clone());
                    citizens = hw_sim::citizens::Citizens::new(generated.world.clone());
                    crowd.clear(world);
                    set_town_stream(world, &generated)?;
                    extras = spawn_extras(world, &generated)?;
                }
                let eye = world.camera().eye();
                let eye = DVec2::new(eye.x as f64, eye.z as f64);
                town.update(world, eye, TITLE_BUILD_MS);
                let (built, wanted) = town.progress(eye);
                let (done, what) = if switching.is_some() {
                    (0.0, "Building your world...")
                } else if wanted == 0 {
                    (1.0, "")
                } else {
                    (built as f32 / wanted as f32, "Building the city...")
                };
                let mut choice = t.frame(world, frame, &generated.world, &solids, (done, what))?;
                // A joined world (or a new hero's) waits for its town.
                if choice.is_none() && switching.is_none() && done >= 1.0 {
                    match waiting.take() {
                        Some(Waiting::Joined(mut s)) => {
                            s.attach_town(&generated);
                            session = Some(*s);
                        }
                        Some(Waiting::Single) => choice = Some(Choice::Continue),
                        None => {}
                    }
                }
                let mut want_world = |seed: u64, people: f64, then: Waiting| {
                    if seed == world_seed && people == world_people {
                        return Some(then);
                    }
                    let (tx, rx) = std::sync::mpsc::channel();
                    std::thread::spawn(move || {
                        let _ =
                            tx.send((seed, people, GeneratedTown::generate(Seed(seed), people)));
                    });
                    switching = Some(rx);
                    waiting = Some(then);
                    None
                };
                match choice {
                    Some(Choice::Continue) => {
                        session = Some(
                            Session::single(Seed(world_seed), &generated, &name, false)
                                .map_err(io)?,
                        );
                    }
                    Some(Choice::NewHero { name: n, seed }) => {
                        name = n;
                        t.profile.name = name.clone();
                        t.profile.seed = seed;
                        if want_world(seed, people, Waiting::Single).is_some() {
                            session = Some(
                                Session::single(Seed(world_seed), &generated, &name, false)
                                    .map_err(io)?,
                            );
                        }
                    }
                    Some(Choice::Host) => {
                        match Session::single(Seed(world_seed), &generated, &name, true) {
                            Ok(s) => session = Some(s),
                            Err(e) => {
                                t.status = Some((format!("Could not open your world: {e}"), true))
                            }
                        }
                    }
                    Some(Choice::Join(code)) => match Session::join(&code, &name) {
                        Ok((mut s, seed, people)) => {
                            if seed == world_seed && people == world_people {
                                s.attach_town(&generated);
                                session = Some(s);
                            } else {
                                want_world(seed, people, Waiting::Joined(Box::new(s)));
                            }
                        }
                        Err(e) => t.status = Some((format!("Could not join: {e}"), true)),
                    },
                    Some(Choice::Quit) | None => {}
                }
                if session.is_some() {
                    // Off we go: the camera swoops down to the hero.
                    let (eye, yaw, pitch) = t.camera(&generated.world, &solids);
                    swoop = Some((frame.time, eye, yaw, pitch));
                    hero_cam.yaw = yaw;
                    music_fade_at = Some(frame.time + 20.0);
                    t.profile.played = true;
                    t.profile.seed = world_seed;
                    t.profile.save();
                    t.leave(world);
                    title_screen = None;
                }
                return Ok(());
            }
            if music_fade_at.is_some_and(|at| frame.time > at) {
                world.stop_music(8.0);
                music_fade_at = None;
            }
            let Some(session) = session.as_mut() else {
                return Ok(());
            };
            version_corner(&frame.ui, &mut whats_new, session.room_code());
            if let Some(lights) = extras.lights.as_mut() {
                let dark = swoop.is_none() && hw_client::nightlights::dark_at(game_hour());
                lights.update(world, world.camera().eye(), dark)?;
            }
            {
                let eye = world.camera().eye();
                let step = 48.0;
                let key = ((eye.x / step).round() as i32, (eye.z / step).round() as i32);
                if street_probe.map(|(_, k)| k) != Some(key) {
                    let (x, z) = (key.0 as f32 * step, key.1 as f32 * step);
                    let g = generated.world.ground(DVec2::new(x as f64, z as f64)) as f32;
                    let probe = engine::reflections::ReflectionProbe::new(
                        (x, g.max(0.0) + 6.0, z),
                        (70.0, 24.0, 70.0),
                    );
                    match street_probe {
                        Some((id, _)) => world.set_reflection_probe(id, probe)?,
                        None => {
                            let id = world.add_reflection_probe(probe)?;
                            street_probe = Some((id, key));
                        }
                    }
                    if let Some(p) = street_probe.as_mut() {
                        p.1 = key;
                    }
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

            // People finished on the worker thread (HW_WAIT_PEOPLE=1 waits
            // for all of them at the start, for screenshots).
            if let Some(p) = &actors {
                let mut done = p.finished();
                if frame.first && std::env::var_os("HW_WAIT_PEOPLE").is_some() {
                    let all = hw_sim::citizens::LOOKS as usize + 1;
                    let more = p.wait(all.saturating_sub(done.len()));
                    done.extend(more);
                }
                for b in done {
                    if let Who::Citizen(look) = b.who {
                        crowd.add_look(world, look, &b)?;
                    }
                    models.insert(b.who.clone(), b.model.clone());
                }
            }

            // Draw heroes.
            let heroes = session.entities(frame.dt as f64);
            anims.retain(|id, _| heroes.iter().any(|h| h.id == *id));
            hero_entities.retain(|id, (e, _)| {
                let keep = heroes.iter().any(|h| h.id == *id);
                if !keep {
                    world.despawn(*e);
                }
                keep
            });
            let mine = session.my_hero();
            for h in &heroes {
                let vel = Vec3::from_array(h.vel);
                if h.kind == hw_net::EntityKind::Crate {
                    // Physics bodies turn every way; Place turns yaw, pitch, roll in that order.
                    let (y, x, z) = glam::Quat::from_array(h.rot).to_euler(glam::EulerRot::YXZ);
                    let place = Place::new(h.pos[0] as f32, h.pos[1] as f32, h.pos[2] as f32)
                        .with_yaw_deg(y.to_degrees())
                        .with_pitch_deg(x.to_degrees())
                        .with_roll_deg(z.to_degrees());
                    match hero_entities.get(&h.id) {
                        Some(&(e, _)) => world.set_place(e, place)?,
                        None => {
                            let e = world.place(citymesh::crate_mesh(), place)?;
                            hero_entities.insert(h.id, (e, false));
                        }
                    }
                    continue;
                }
                let feet = Vec3::new(h.pos[0] as f32, h.pos[1] as f32, h.pos[2] as f32);
                // Our own flight we know; others' is told from their motion.
                let flying = (mine == Some(h.id))
                    .then(|| hero_flies || solids.in_lift(glam::DVec3::from_array(h.pos)));
                let anim = anims.entry(h.id).or_default();
                let d = anim.step(vel, h.grounded, flying, frame.dt);
                // Until the hero creator: a look per player, the same on
                // every machine.
                let who = Who::Hero(format!("player {}", h.owner.map_or(0, |o| o.0)));
                let model = models.get(&who).cloned();
                if model.is_none() && !asked.contains(&who) {
                    if let Some(p) = &actors {
                        p.request(who.clone(), 0);
                        asked.insert(who.clone());
                    }
                }
                let animated = model.is_some();
                let place = Place::new(feet.x, feet.y, feet.z).with_yaw_deg(d.yaw);
                let (e, was_animated) = match hero_entities.get(&h.id) {
                    Some(&(e, a)) if a == animated => (e, a),
                    old => {
                        if let Some(&(e, _)) = old {
                            world.despawn(e);
                        }
                        let e = match &model {
                            Some(m) => {
                                let e = world.spawn_animated_shared(m.clone(), place)?;
                                world.set_animation_blend(e, 0.25)?;
                                anim.shown = "";
                                e
                            }
                            None => world.place(citymesh::hero_mesh(mine == Some(h.id)), place)?,
                        };
                        hero_entities.insert(h.id, (e, animated));
                        (e, false)
                    }
                };
                let _ = was_animated;
                if animated {
                    let o = tipped_origin(feet, &d);
                    world.set_place(
                        e,
                        Place::new(o.x, o.y, o.z)
                            .with_yaw_deg(d.yaw)
                            .with_pitch_deg(d.pitch)
                            .with_roll_deg(d.roll),
                    )?;
                    if anim.shown != d.clip {
                        if d.once {
                            world.play_animation_once(e, d.clip)?;
                        } else {
                            world.play_animation(e, d.clip)?;
                        }
                        // Feet find the ground when standing on it.
                        let on_ground =
                            matches!(d.clip, "idle" | "walk" | "jog" | "sprint" | "jump land");
                        let _ = world.set_foot_ik(e, on_ground.then(FootIk::default));
                        anim.shown = d.clip;
                    }
                    world.set_animation_speed(e, d.rate)?;
                } else {
                    world.set_place(e, place)?;
                }
            }

            // Citizens around the camera, at the game's time.
            let eye = world.camera().eye();
            crowd.update(
                world,
                &citizens,
                DVec2::new(eye.x as f64, eye.z as f64),
                session.time(tick_clock / hw_sim::DT),
                frame.dt as f64,
            )?;
            // K knocks the people around the hero over (a test of ragdolls).
            if !flying && input.pressed(Key::K) {
                if let Some(p) = session.my_position(&heroes) {
                    let n = crowd.knock_down(world, p.as_vec3() + Vec3::Y, 6.0);
                    eprintln!("knocked over {n}");
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
                    let floor = generated
                        .world
                        .ground(q)
                        .max(hw_gen::urban::world::WATER_LEVEL);
                    fly.position.y = fly.position.y.max(floor as f32 + 1.8);
                }
            }
            // Just off the title: swoop from its camera down to the hero,
            // the sun climbing from golden hour to the game's morning.
            if let Some((t0, from, from_yaw, from_pitch)) = swoop {
                let k = ((frame.time - t0) / SWOOP_SECONDS).clamp(0.0, 1.0);
                let k = k * k * (3.0 - 2.0 * k);
                let cam = world.camera();
                let to = cam.eye();
                let dir = (cam.target() - to).normalize_or_zero();
                let to_yaw = dir.x.atan2(dir.z).to_degrees();
                let to_pitch = dir.y.clamp(-1.0, 1.0).asin().to_degrees();
                let turn = (to_yaw - from_yaw + 540.0).rem_euclid(360.0) - 180.0;
                world.look_first_person(
                    from.lerp(to, k),
                    from_yaw + turn * k,
                    from_pitch + (to_pitch - from_pitch) * k,
                );
                let hour = title::TITLE_HOUR + (game_hour() - title::TITLE_HOUR) * k;
                world.set_physical_sky(Some(PhysicalSky::at_hour(hour)));
                if k >= 1.0 {
                    swoop = None;
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

/// Someone waiting on the title for another world's town to be generated.
enum Waiting {
    /// A new hero: plays alone once the town is there.
    Single,
    /// Joined a friend whose world is not ours yet.
    Joined(Box<Session>),
}

/// The planet's two moons: a big pale one and a small quick one.
fn moons() -> Vec<engine::sky::MoonSpec> {
    use engine::sky::MoonSpec;
    vec![
        MoonSpec::new(1.7, 29.5, 0.5).tilt(14.0),
        MoonSpec::new(0.7, 9.0, 0.44).tilt(-26.0).brightness(0.7),
    ]
}

/// Engine streams the town's ground and water.
fn set_town_stream(world: &mut World, generated: &GeneratedTown) -> EngineResult<()> {
    // A wide, coarse water window, so rivers and lakes show across the town
    // and not only around the camera.
    let settings = StreamSettings {
        water_window: 1536.0,
        water_cell: 4.0,
        ..StreamSettings::default()
    };
    let stream = WorldStream::new(TownGround(generated.world.clone()), settings)
        .with_terrain(|t| t.with_beach(1.8));
    world.set_stream(Some(stream))?;
    world.prepare_stream(20.0);
    Ok(())
}

/// What stands in the town besides its roads and buildings, kept so it can
/// be taken out again when the title switches worlds.
#[derive(Default)]
struct TownExtras {
    lods: Vec<LodId>,
    entities: Vec<EntityId>,
    lights: Option<hw_client::nightlights::NightLights>,
}

impl TownExtras {
    fn despawn(&mut self, world: &mut World) {
        for id in self.lods.drain(..) {
            world.despawn_lod(id);
        }
        for e in self.entities.drain(..) {
            world.despawn(e);
        }
        if let Some(mut l) = self.lights.take() {
            l.clear(world);
        }
    }
}

/// The town's props, landmarks and waterfronts.
fn spawn_extras(world: &mut World, generated: &GeneratedTown) -> EngineResult<TownExtras> {
    let t = std::time::Instant::now();
    let props = hw_client::propview::PropView::spawn(world, &generated.world)?;
    eprintln!(
        "{} props in {:.2} s",
        props.count,
        t.elapsed().as_secs_f64()
    );
    let landmarks = hw_client::landmarkview::LandmarkView::spawn(world, &generated.world)?;
    let waterfront = hw_client::waterfrontview::WaterfrontView::spawn(world, &generated.world)?;
    Ok(TownExtras {
        lods: props.sets.into_iter().chain(landmarks.sets).collect(),
        entities: waterfront.entities,
        lights: Some(hw_client::nightlights::NightLights::new(&generated.world)),
    })
}

/// The version at the top right, with a "What's new" button and its list.
fn version_corner(ui: &engine::ui::UiFrame, open: &mut bool, room: Option<&str>) {
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
                        if let Some(code) = room {
                            // What friends type to join this world.
                            ui.label(
                                egui::RichText::new(format!("Room {code}"))
                                    .color(egui::Color32::from_rgb(255, 214, 90))
                                    .strong(),
                            );
                        }
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
