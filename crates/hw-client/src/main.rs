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
//! Left mouse (or 1) punches, Q (or 2) fires an energy bolt: at the villain
//! you have picked with Tab, else where you look, breaking glass, plaster
//! and brick at level 1 (`HW_LEVEL` sets the hero level for now).
//! G hails a flying taxi: pick a stand under "Where to?", walk up to the
//! taxi when it lands beside you, and it flies you there above the roofs
//! (G again hops out). Cars drive the streets; hover cars and taxis fly
//! lanes over the main roads.
//! O switches to a free-flying camera and back. C cuts buildings open
//! around where you look (F moves the cut there), Z/X pick the floor.
//!
//! Fighting (see `fightview.rs`): Tab picks the villain nearest the
//! crosshair (again for the next one), 1 strikes, 2 fires an energy bolt.
//! Hold E next to a downed teammate to revive them; H calls the medical
//! drone when you are down. The mad scientist's minions wait a short run
//! down the street from where you start.
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
//! `hero` (default), `inside`, `sky`, `overview`, `street`, `river`, `cutaway`, `park`, `pad`, `spire`, `lighthouse`, `quay`, `wrecked`, `traffic`, `skylanes` or
//! `props` picks the starting camera (handy with `ENGINE_SCREENSHOT`; `props` looks at the
//! nearest prop whose kind contains `HW_PROP`, a car by default); `HW_CUT_LEVEL` the floor shown
//! cut open; `HW_WALK=1` makes the hero run forward on its own (for tests);
//! `HW_VIEW=fight` starts in front of the minions, `HW_AUTOFIGHT=1`
//! fights them on its own (both for screenshots);
//! `HW_TAXI=<stand>` hails a taxi at the start and `HW_TAXI_SECS=<s>` runs
//! the world on that far (screenshots of a ride);
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
use hw_client::damageview::DamageView;
use hw_client::fightview::{self, FightView};
use hw_client::heroview::{tipped_origin, HeroAnim};
use hw_client::people::{self, People, VillainKind, Who};
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
    // Lit windows are baked into the buildings: decide before any is built.
    hw_client::buildingmesh::NIGHT.store(
        hw_client::nightlights::dark_at(game_hour()),
        std::sync::atomic::Ordering::Relaxed,
    );
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
    let auto_fight = std::env::var_os("HW_AUTOFIGHT").is_some();
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
    let mut traffic = hw_sim::traffic::Traffic::new(generated.world.clone());
    // Taxi stands of the town, the "Where to?" list (G), and where we asked to go.
    let mut stands = hw_sim::taxi::stands(&generated.world);
    let mut taxi_menu = false;
    let mut taxi_dest: Option<usize> = None;
    let mut taxi_seen = false;
    // HW_TAXI=<stand> hails a taxi at the start and HW_TAXI_SECS runs the
    // world on that far (for screenshots of a ride).
    let mut taxi_demo =
        env_num("HW_TAXI").map(|d| (d as usize, env_num("HW_TAXI_SECS").unwrap_or(0)));
    let mut traffic_view = hw_client::trafficview::TrafficView::new();
    // People from Actors, built on a worker thread from the packs.
    let actors = match people::packs_dir() {
        Some(dir) => {
            let p = People::start(dir);
            if std::env::var_os("HW_JOIN").is_none() {
                // Our own hero first (the local player is player 1).
                p.request(Who::Hero("player 1".into()), 0);
            }
            for kind in [VillainKind::Brute, VillainKind::Technician] {
                for look in 0..people::VILLAIN_LOOKS {
                    p.request(Who::Villain(kind, look), 0);
                }
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
    for kind in [VillainKind::Brute, VillainKind::Technician] {
        for look in 0..people::VILLAIN_LOOKS {
            asked.insert(Who::Villain(kind, look));
        }
    }
    // The fight: targets, effects, the screen.
    let mut fight = FightView::new();
    // What was drawn last frame (for targeting), and a power key pressed
    // but not yet sent with a tick.
    let mut drawn: Vec<hw_net::EntityState> = Vec::new();
    let mut pending_power = 0u8;
    // Characters a ragdoll moved, and how far from where the server has
    // them when they got up (faded out).
    let mut settle: BTreeMap<hw_core::NetId, (Vec3, bool)> = BTreeMap::new();
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
        "wrecked" => {
            // Two buildings near the start broken for the camera.
            let (eye, at) = session
                .as_mut()
                .and_then(|s| s.wreck_demo(&generated))
                .unwrap_or((
                    glam::DVec3::new(spawn.x, ground(spawn) + 10.0, spawn.y),
                    glam::DVec3::new(spawn.x + 20.0, ground(spawn), spawn.y),
                ));
            let d = at - eye;
            let yaw = d.x.atan2(d.z).to_degrees() as f32;
            let pitch = (d.y / DVec2::new(d.x, d.z).length()).atan().to_degrees() as f32;
            FlyCamera::new((eye.x as f32, eye.y as f32, eye.z as f32), yaw, pitch)
        }
        "traffic" | "skylanes" => {
            // Down the main road nearest the start: cars on it, or (from
            // higher up) the sky lanes over it.
            let mut best: Option<(f64, DVec2, DVec2)> = None;
            let sky = view == "skylanes";
            let long = if sky { 60.0 } else { 45.0 };
            for r in generated.world.roads.iter().filter(|r| r.main) {
                for w in r.pts.windows(2) {
                    let d = w[0].distance(spawn);
                    if w[0].distance(w[1]) > long && best.is_none_or(|b| d < b.0) {
                        best = Some((d, w[0], (w[1] - w[0]).normalize()));
                    }
                }
            }
            let (_, c, dir) = best.unwrap_or((0.0, spawn, spawn_dir));
            // From the start of a straight stretch, along it.
            let p = c - dir * if sky { 30.0 } else { 2.0 };
            let yaw = dir.x.atan2(dir.y).to_degrees() as f32;
            let (up, pitch) = if sky { (30.0, 4.0) } else { (6.0, -10.0) };
            FlyCamera::new((p.x as f32, ground(p) as f32 + up, p.y as f32), yaw, pitch)
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
    let mut flying = !matches!(view.as_str(), "hero" | "inside" | "sky" | "title" | "fight");
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
    if view == "fight" {
        // Down the street, facing the mad scientist's minions.
        let p = spawn - spawn_dir * 19.0;
        if let Some(s) = session.as_mut() {
            s.teleport(glam::DVec3::new(p.x, ground(p) + 0.3, p.y));
        }
        let ahead = -spawn_dir;
        hero_cam_yaw = ahead.x.atan2(ahead.y).to_degrees() as f32 + 18.0;
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
    // Broken buildings, once a game is on.
    let mut damage: Option<DamageView> = None;
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
                    traffic = hw_sim::traffic::Traffic::new(generated.world.clone());
                    stands = hw_sim::taxi::stands(&generated.world);
                    traffic_view.clear(world);
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
            if input.pressed(Key::O) {
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
            if let Some((dest, secs)) = taxi_demo.take() {
                session.hail_taxi(dest as u8);
                taxi_dest = Some(dest);
                session.fast_forward(secs as u32 * hw_sim::TICK_HZ);
            }
            let ride_yaw: Option<f32>;
            // G: a taxi. "Where to?", or out of the one you are in (sending
            // a coming one away).
            {
                let my_id = session.my_hero();
                let me = drawn.iter().find(|e| Some(e.id) == my_id);
                let cab = me.and_then(|m| {
                    drawn
                        .iter()
                        .find(|t| t.kind == hw_net::EntityKind::Taxi && t.owner == m.owner)
                });
                let riding = me.is_some_and(|m| hw_sim::taxi::riding(m, &drawn));
                // Riding, the camera swings round behind the taxi.
                ride_yaw = cab.filter(|_| riding).map(|t| {
                    let (y, _, _) = glam::Quat::from_array(t.rot).to_euler(glam::EulerRot::YXZ);
                    y.to_degrees()
                });
                if !flying && input.pressed(Key::G) {
                    if cab.is_some() {
                        session.hail_taxi(hw_sim::taxi::HOP_OUT);
                        taxi_dest = None;
                    } else {
                        taxi_menu = !taxi_menu;
                        if taxi_menu {
                            world.set_pointer_lock(false);
                        }
                    }
                }
                if let Some(i) = taxi_panel(&frame.ui, &mut taxi_menu, &stands) {
                    session.hail_taxi(i as u8);
                    taxi_dest = Some(i);
                }
                // The ride is over once our taxi has come and gone.
                if cab.is_some() {
                    taxi_seen = true;
                } else if taxi_seen {
                    taxi_seen = false;
                    taxi_dest = None;
                }
                let to = taxi_dest
                    .and_then(|i| stands.get(i))
                    .map(|s| s.name.as_str());
                taxi_status(&frame.ui, cab.is_some(), riding, to);
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
                // Left mouse punches, Q fires an energy blast, both where the
                // camera looks (levelled a little, as it looks down at the hero).
                hero_input.smash = input.mouse_down(MouseButton::Left);
                hero_input.blast = input.down(Key::Q);
                hero_input.aim = Camera::direction(hero_cam.yaw, hero_cam.pitch + 10.0).to_array();
                // Fighting: keys, target, revive.
                let me_state = session
                    .my_hero()
                    .and_then(|id| drawn.iter().find(|e| e.id == id));
                let cam = world.camera();
                let look = (cam.target() - cam.eye()).normalize_or_zero();
                let f = fight.input(input, me_state, &drawn, cam.eye(), look);
                if f.power != 0 {
                    pending_power = f.power;
                }
                // HW_AUTOFIGHT=1 fights on its own (for screenshots): bolts
                // while there is energy, punches up close.
                if auto_fight
                    && (frame.time / 0.75).floor() != ((frame.time - frame.dt) / 0.75).floor()
                {
                    if fight.target.is_none() {
                        fight.target = fightview::pick_target(&drawn, cam.eye(), look, None);
                    }
                    let close = me_state
                        .zip(fight.target.and_then(|t| drawn.iter().find(|e| e.id == t)))
                        .is_some_and(|(m, t)| {
                            glam::DVec3::from_array(m.pos).distance(glam::DVec3::from_array(t.pos))
                                < 2.6
                        });
                    pending_power = if close { 1 } else { 2 };
                }
                let f = fightview::FightInput {
                    target: fight.target,
                    ..f
                };
                hero_input.target = f.target;
                hero_input.revive = f.revive;
                hero_input.hospital = f.hospital;
                let down = me_state
                    .and_then(|m| m.combat)
                    .is_some_and(|c| c.status != hw_net::Status::Standing);
                if down {
                    // Down or carried: no running about (the server agrees).
                    hero_input.move_x = 0.0;
                    hero_input.move_y = 0.0;
                    hero_input.move_z = 0.0;
                    hero_input.jump = false;
                }
                if down {
                    hero_input.smash = false;
                    hero_input.blast = false;
                }
            }

            // Fixed ticks for the game, however fast frames come.
            tick_clock += frame.dt as f64;
            let mut ticks = 0;
            while tick_clock >= hw_sim::DT && ticks < 5 {
                tick_clock -= hw_sim::DT;
                ticks += 1;
                // A power key goes out once, with the next tick.
                // (a click shorter than a tick still punches).
                let mut i = hero_input;
                match std::mem::take(&mut pending_power) {
                    1 => i.smash = true,
                    2 => i.blast = true,
                    _ => {}
                }
                session.tick(i);
            }
            if ticks == 5 {
                tick_clock = 0.0;
            }
            // Damage: broken buildings, repairs, impacts.
            let joined = session.joined();
            let mut news = session.take_news();
            // Broken and mended street props: their places, and lamps on them.
            news.retain(|m| match m {
                hw_net::ToClient::PropsBroken { props, .. } => {
                    for &(i, fell) in props {
                        if let Some(v) = extras.props.as_mut() {
                            v.set_broken(&generated.world, i as usize, Some(fell));
                        }
                        if let Some(l) = extras.lights.as_mut() {
                            l.set_broken(i, true);
                        }
                    }
                    false
                }
                hw_net::ToClient::PropsRepaired { props } => {
                    for &i in props {
                        if let Some(v) = extras.props.as_mut() {
                            v.set_broken(&generated.world, i as usize, None);
                        }
                        if let Some(l) = extras.lights.as_mut() {
                            l.set_broken(i, false);
                        }
                    }
                    false
                }
                _ => true,
            });
            if let Some(v) = extras.props.as_mut() {
                v.flush(world)?;
            }
            damage
                .get_or_insert_with(|| DamageView::new(joined))
                .news(world, &mut town, &solids, news);

            // People finished on the worker thread (HW_WAIT_PEOPLE=1 waits
            // for all of them at the start, for screenshots).
            if let Some(p) = &actors {
                let mut done = p.finished();
                if frame.first && std::env::var_os("HW_WAIT_PEOPLE").is_some() {
                    let all =
                        hw_sim::citizens::LOOKS as usize + 1 + 2 * people::VILLAIN_LOOKS as usize;
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
            // Where everyone the server sends is, for the fight's lookups.
            let at_of = |id: hw_core::NetId| {
                heroes
                    .iter()
                    .find(|e| e.id == id)
                    .map(|e| Vec3::new(e.pos[0] as f32, e.pos[1] as f32, e.pos[2] as f32))
            };
            for h in &heroes {
                let vel = Vec3::from_array(h.vel);
                if h.kind == hw_net::EntityKind::Taxi
                    || (h.kind == hw_net::EntityKind::Hero && hw_sim::taxi::riding(h, &heroes))
                {
                    // A taxi, and its fare out of sight inside.
                    let (y, _, _) = glam::Quat::from_array(h.rot).to_euler(glam::EulerRot::YXZ);
                    let hidden = h.kind == hw_net::EntityKind::Hero;
                    let place = Place::new(
                        h.pos[0] as f32,
                        h.pos[1] as f32 - if hidden { 2000.0 } else { 0.0 },
                        h.pos[2] as f32,
                    )
                    .with_yaw_deg(y.to_degrees());
                    match hero_entities.get(&h.id) {
                        Some(&(e, _)) => world.set_place(e, place)?,
                        None if !hidden => {
                            let mesh = hw_client::propview::taxi_levels()
                                .into_iter()
                                .next()
                                .map(|l| l.mesh)
                                .expect("taxi model");
                            let e = world.place(mesh, place)?;
                            hero_entities.insert(h.id, (e, false));
                        }
                        None => {}
                    }
                    continue;
                }
                if matches!(
                    h.kind,
                    hw_net::EntityKind::Crate | hw_net::EntityKind::Drone
                ) {
                    // Physics bodies turn every way; Place turns yaw, pitch, roll in that order.
                    let (y, x, z) = glam::Quat::from_array(h.rot).to_euler(glam::EulerRot::YXZ);
                    let mut place = Place::new(h.pos[0] as f32, h.pos[1] as f32, h.pos[2] as f32)
                        .with_yaw_deg(y.to_degrees())
                        .with_pitch_deg(x.to_degrees())
                        .with_roll_deg(z.to_degrees());
                    if h.kind == hw_net::EntityKind::Drone {
                        // A drone turns slowly as it flies, bobbing a little.
                        let t = frame.time + h.id.0 as f32;
                        place = Place::new(
                            h.pos[0] as f32,
                            h.pos[1] as f32 + (t * 2.3).sin() * 0.06,
                            h.pos[2] as f32,
                        )
                        .with_yaw_deg(t * 40.0);
                    }
                    match hero_entities.get(&h.id) {
                        Some(&(e, _)) => world.set_place(e, place)?,
                        None => {
                            let mesh = if h.kind == hw_net::EntityKind::Drone {
                                // A medical drone fetches heroes, police ones villains.
                                let medical = heroes.iter().any(|o| {
                                    o.kind == hw_net::EntityKind::Hero
                                        && at_of(o.id).unwrap_or(Vec3::ZERO).distance(Vec3::new(
                                            h.pos[0] as f32,
                                            h.pos[1] as f32,
                                            h.pos[2] as f32,
                                        )) < 3.0
                                });
                                fightview::drone_mesh(medical)
                            } else {
                                citymesh::crate_mesh()
                            };
                            let e = world.place(mesh, place)?;
                            hero_entities.insert(h.id, (e, false));
                        }
                    }
                    continue;
                }
                let feet = Vec3::new(h.pos[0] as f32, h.pos[1] as f32, h.pos[2] as f32);
                let villain = h.kind.is_enemy();
                // Our own flight we know; others' is told from their motion.
                let flying = (mine == Some(h.id))
                    .then(|| hero_flies || solids.in_lift(glam::DVec3::from_array(h.pos)));
                let anim = anims.entry(h.id).or_default();
                let mut d = anim.step(
                    vel,
                    h.grounded,
                    if villain { Some(false) } else { flying },
                    frame.dt,
                );
                // Fighters face their foe while they stand and fight.
                let action = fight.action(h.id);
                let foe = h.combat.and_then(|c| c.target).and_then(at_of);
                let slow = Vec3::new(vel.x, 0.0, vel.z).length() < 0.6;
                if let (Some(f), true) = (foe, action.is_some() || (villain && slow)) {
                    let to = f - feet;
                    if to.length() > 0.2 {
                        anim.face(to.x.atan2(to.z).to_degrees(), frame.dt);
                        d.yaw = anim.yaw();
                    }
                }
                // A look per player until the hero creator, the same on
                // every machine; villains by kind, a few looks each.
                let who = match h.kind {
                    hw_net::EntityKind::Brute => {
                        Who::Villain(VillainKind::Brute, h.id.0 % people::VILLAIN_LOOKS)
                    }
                    hw_net::EntityKind::Technician => {
                        Who::Villain(VillainKind::Technician, h.id.0 % people::VILLAIN_LOOKS)
                    }
                    _ => Who::Hero(format!("player {}", h.owner.map_or(0, |o| o.0))),
                };
                let model = models.get(&who).cloned();
                if model.is_none() && !asked.contains(&who) {
                    if let Some(p) = &actors {
                        p.request(who.clone(), 0);
                        asked.insert(who.clone());
                    }
                }
                let animated = model.is_some();
                let place = Place::new(feet.x, feet.y, feet.z).with_yaw_deg(d.yaw);
                let (e, _) = match hero_entities.get(&h.id) {
                    Some(&(e, a)) if a == animated => (e, a),
                    old => {
                        if let Some(&(e, _)) = old {
                            fight.drop_gun(world, e);
                            world.despawn(e);
                        }
                        let e = match &model {
                            Some(m) => {
                                let e = world.spawn_animated_shared(m.clone(), place)?;
                                world.set_animation_blend(e, 0.2)?;
                                anim.shown = "";
                                if h.kind == hw_net::EntityKind::Technician {
                                    fight.arm(world, e);
                                }
                                e
                            }
                            None if villain => {
                                world.place(fightview::villain_placeholder(h.kind), place)?
                            }
                            None => world.place(citymesh::hero_mesh(mine == Some(h.id)), place)?,
                        };
                        hero_entities.insert(h.id, (e, animated));
                        (e, false)
                    }
                };
                if !animated {
                    world.set_place(e, place)?;
                    continue;
                }
                // A ragdoll moves the body itself; afterwards the drawn body
                // eases back to where the server has it.
                if world.ragdoll_state(e).is_some() {
                    let at = world
                        .world_transform(e)
                        .map(|m| m.w_axis.truncate())
                        .unwrap_or(feet);
                    settle.insert(h.id, (at - feet, true));
                    continue;
                }
                let mut offset = Vec3::ZERO;
                if let Some((o, _)) = settle.get_mut(&h.id) {
                    *o *= (-frame.dt * 4.0).exp();
                    offset = *o;
                    if o.length() < 0.02 {
                        settle.remove(&h.id);
                    }
                }
                let o = tipped_origin(feet + offset, &d);
                world.set_place(
                    e,
                    Place::new(o.x, o.y, o.z)
                        .with_yaw_deg(d.yaw)
                        .with_pitch_deg(d.pitch)
                        .with_roll_deg(d.roll),
                )?;
                // Fighting clips over walking; a stance while squaring up.
                let in_fight = h.combat.is_some_and(|c| c.target.is_some()) && slow && villain;
                let (clip, once) = match action {
                    Some(a) => (a, a != "cuffed" && a != "hover"),
                    None if in_fight
                        && h.kind == hw_net::EntityKind::Technician
                        && d.clip == "idle" =>
                    {
                        ("pistol idle", false)
                    }
                    None if in_fight && d.clip == "idle" => ("fight stance", false),
                    None => (d.clip, d.once),
                };
                if anim.shown != clip {
                    // A clip a character lacks is skipped, not fatal.
                    let played = if once {
                        world.play_animation_once(e, clip)
                    } else {
                        world.play_animation(e, clip)
                    };
                    if let Err(err) = played {
                        eprintln!("{who:?}: {err}");
                    }
                    // Feet find the ground when standing on it.
                    let on_ground = action.is_none()
                        && matches!(
                            clip,
                            "idle"
                                | "walk"
                                | "jog"
                                | "sprint"
                                | "jump land"
                                | "fight stance"
                                | "pistol idle"
                        );
                    let _ = world.set_foot_ik(e, on_ground.then(FootIk::default));
                    anim.shown = clip;
                }
                let rate = if fight.frozen(h.id) {
                    0.0
                } else if action.is_some() {
                    1.0
                } else {
                    d.rate
                };
                world.set_animation_speed(e, rate)?;
            }
            settle.retain(|id, _| heroes.iter().any(|h| h.id == *id));
            // Knocks, defeats, cuffs and drones follow the fighters' status;
            // then what happened in fights this frame.
            fight.statuses(world, &heroes, &mut hero_entities);
            let (events, now) = session.take_fx();
            fight.on_fx(world, &events, now, mine, &hero_entities, &heroes);
            let me_state = mine.and_then(|id| heroes.iter().find(|e| e.id == id));
            fight.update(world, frame.dt, &heroes, me_state);

            // Citizens around the camera, at the game's time.
            let eye = world.camera().eye();
            crowd.update(
                world,
                &citizens,
                DVec2::new(eye.x as f64, eye.z as f64),
                session.time(tick_clock / hw_sim::DT),
                frame.dt as f64,
            )?;
            // Cars on the streets, hover cars and taxis in the sky lanes.
            traffic_view.update(
                world,
                &traffic,
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
                (false, Some(p)) => {
                    if let Some(y) = ride_yaw {
                        if !frame.input.mouse_down(MouseButton::Right) {
                            let turn = (y - hero_cam.yaw + 540.0).rem_euclid(360.0) - 180.0;
                            hero_cam.yaw += turn * (1.0 - 0.3f32.powf(frame.dt));
                        }
                        hero_cam.distance = hero_cam.distance.max(10.0);
                    }
                    hero_cam.update(world, frame, p.as_vec3(), &solids, open_above);
                    let shake = fight.shake();
                    if shake != Vec3::ZERO {
                        let cam = world.camera();
                        world.look_first_person(cam.eye() + shake, hero_cam.yaw, hero_cam.pitch);
                    }
                }
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
            // The fight's screen over everything.
            {
                let view_proj = world.camera().view_projection(frame.aspect);
                let me_state = mine.and_then(|id| heroes.iter().find(|e| e.id == id));
                let names = |e: &hw_net::EntityState| {
                    e.owner
                        .and_then(|o| session.player_name(o))
                        .unwrap_or_else(|| "Your teammate".into())
                };
                fight.hud(frame.ui.ctx(), view_proj, me_state, &heroes, &names);
            }
            drawn = heroes;

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
    props: Option<hw_client::propview::PropView>,
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
        self.props = None;
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
        lods: props.sets.iter().copied().chain(landmarks.sets).collect(),
        entities: waterfront.entities,
        lights: Some(hw_client::nightlights::NightLights::new(&generated.world)),
        props: Some(props),
    })
}

/// "Where to?": the town's taxi stands as buttons. Returns the one picked.
fn taxi_panel(
    ui: &engine::ui::UiFrame,
    open: &mut bool,
    stands: &[hw_sim::taxi::Stand],
) -> Option<usize> {
    use engine::ui::egui;
    if !*open {
        return None;
    }
    let mut picked = None;
    egui::Area::new(egui::Id::new("taxi"))
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_unmultiplied(30, 26, 12, 230))
                .stroke(egui::Stroke::new(
                    2.0_f32,
                    egui::Color32::from_rgb(255, 210, 60),
                ))
                .corner_radius(8.0)
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.set_min_width(280.0);
                    ui.label(
                        egui::RichText::new("TAXI! Where to?")
                            .color(egui::Color32::from_rgb(255, 214, 70))
                            .strong()
                            .size(20.0),
                    );
                    ui.add_space(6.0);
                    for (i, s) in stands.iter().enumerate() {
                        let b = egui::Button::new(egui::RichText::new(&s.name).size(15.0))
                            .min_size(egui::vec2(260.0, 26.0));
                        if ui.add(b).clicked() {
                            picked = Some(i);
                        }
                    }
                    ui.add_space(4.0);
                    if ui.button("Never mind (G)").clicked() {
                        *open = false;
                    }
                });
        });
    if picked.is_some() {
        *open = false;
    }
    picked
}

/// A line at the top while a taxi comes for you or takes you somewhere.
fn taxi_status(ui: &engine::ui::UiFrame, cab: bool, riding: bool, to: Option<&str>) {
    use engine::ui::egui;
    if !cab {
        return;
    }
    let text = match (riding, to) {
        (true, Some(to)) => format!("Taxi to {to}  ·  G to hop out"),
        (true, None) => "In a taxi  ·  G to hop out".into(),
        (false, _) => "Your taxi is coming: walk up to it to get in  ·  G sends it away".into(),
    };
    egui::Area::new(egui::Id::new("taxi status"))
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 14.0))
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_black_alpha(160))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(10, 5))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(text)
                            .color(egui::Color32::from_rgb(255, 214, 70))
                            .size(15.0),
                    );
                });
        });
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
