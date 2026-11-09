//! HeroWorld: the game client, with the server built in for single player.
//!
//! The world is one generated town on its hills, rivers and coast: Engine
//! streams the ground and water, the town's roads and buildings are built
//! in cells around the camera. Buildings around a focus point can be cut
//! open to show their rooms.
//!
//! Controls: you play your hero in third person. Hold the right mouse button
//! (or press P to lock the pointer) and move the mouse to look, WASD to run,
//! Space to jump, R to run on by itself (autorun: the mouse steers, A/D
//! step aside, S or R stops), the mouse wheel zooms in and out (all the way
//! in is first person).
//! V to fly once you have Flight: steer with WASD and the camera, Space
//! climbs, Ctrl sinks, Shift goes faster; V again drops you. K knocks the people around you
//! over (a test of ragdolls; they get up again).
//! Buildings are open: walk in through a door, and in the lift (the core in
//! the middle) hold Space to rise and Ctrl to sink; step out at any floor.
//! Left mouse (or 1) punches, Q (or 2) fires an energy bolt: at the villain
//! you have picked with Tab, else where you look, breaking glass, plaster
//! and brick at level 1 (higher levels break more).
//! G hails a flying taxi: pick a stand under "Where to?", walk up to the
//! taxi when it lands beside you, and it flies you there above the roofs
//! (G again hops out). Cars drive the streets; hover cars and taxis fly
//! lanes over the main roads.
//! N hides or shows the minimap (bottom right; it turns with the camera),
//! M opens the whole town map.
//! Escape opens the menu (settings, controls, quit; see `settings.rs`).
//! O switches to a free-flying camera and back. C cuts buildings open
//! around where you look (F moves the cut there), Z/X pick the floor.
//!
//! Fighting (see `fightview.rs`): Tab picks the villain nearest the
//! crosshair (again for the next one), 1 strikes, 2 fires an energy bolt,
//! 3 to 6 binding hold, force shield, lift and throw, and shockwave once
//! bought. B opens the powers window (`powersview.rs`): karma from beaten
//! villains raises your level and buys powers; your hero file is kept in
//! the profile. Hold E next to a downed teammate to revive them; H calls the medical
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
//! its clock, for screenshots; `HW_TITLE_PANEL=creator` opens the hero
//! creator, `HW_CREATOR_SECTION=face|costume|colours` and
//! `HW_CREATOR_DETAILED=1` pick its tab). Your name, looks, world and sound
//! levels are remembered between runs (see `profile.rs`).
//!
//! Environment: `HW_SEED` (world seed, default 1, or the one you last played); `HW_POP` (people in the
//! town: 300 a hamlet, 40000 the default, millions a metropolis; joining
//! players get the host's); `HW_NAME`; `HW_VIEW` =
//! `hero` (default), `inside`, `sky`, `overview`, `street`, `river`, `cutaway`, `park`, `pad`, `spire`, `plinth` (the spire plaza's edge and steps close up), `lighthouse`, `quay`, `wrecked`, `traffic`, `signals`, `market`, `skylanes`, `lift`, `tunnel`, `molehaven`, `tube` (a tube station's street pavilion), `station`, `tubeline`, `capsule`, `ruin`, `lab`, `kiosk`, `grotto` or
//! `props` picks the starting camera (handy with `ENGINE_SCREENSHOT`; `props` looks at the
//! nearest prop whose kind contains `HW_PROP`, a car by default); `HW_CUT_LEVEL` the floor shown
//! cut open; `HW_WALK=1` makes the hero run forward on its own (for tests);
//! `HW_LEVEL=n` plays a level-n hero with karma to spend, `HW_POWERS=1`
//! with every power open bought (`HW_THEME=1` the second themes);
//! `HW_VIEW=powers` opens the powers window, `HW_VIEW=origin` shows a new
//! hero's origin card.
//! `HW_VIEW=fight` starts in front of the minions, `HW_AUTOFIGHT=1`
//! fights them on its own (both for screenshots);
//! `HW_WRECK=<frames>` wrecks the cars near the camera that many frames in;
//! `HW_TAXI=<stand>` hails a taxi at the start and `HW_TAXI_SECS=<s>` runs
//! the world on that far (screenshots of a ride);
//! `HW_WAIT_PEOPLE=1` waits at the start until every character is built
//! (for screenshots); `HW_PACKS` points at the Actors packs (default
//! `assets/packs`, fetched by `scripts/fetch-packs`);
//! Ctrl+Shift+F12 opens the debug menu (`debugmenu.rs`; `HW_DEBUG=<tab>`
//! opens it at start);
//! `ENGINE_ZFIGHT=1` marks z-fighting with magenta posts (the `zfight` tool
//! checks a whole city without the game).

mod session;

use engine::prelude::*;
use engine::stream::{StreamSettings, WorldStream};
use glam::{DVec2, DVec3, Vec3};
use hw_client::citymesh;
use hw_client::crowdview::CrowdView;
use hw_client::damageview::DamageView;
use hw_client::fightview::{self, FightView};
use hw_client::heroview::{tipped_origin, HeroAnim};
use hw_client::looks;
use hw_client::people::{self, People, VillainKind, Who};
use hw_client::powersview::{PowerAction, PowersView};
use hw_client::profile::Profile;
use hw_client::robotview::RobotView;
use hw_client::title::{self, Choice, Title};
use hw_client::townmesh::{TownGround, TownView};
use hw_core::Seed;
use hw_sim::collision::{Obstacles, BODY_HEIGHT};
use hw_sim::GeneratedTown;
use session::Session;
use std::collections::{BTreeMap, BTreeSet};

/// Milliseconds per frame for building town cells as you move.
const BUILD_BUDGET_MS: f64 = 6.0;
/// Milliseconds per frame for putting cells drawn on worker threads into
/// the world behind the title (and for its props).
const TITLE_BUILD_MS: f64 = 5.0;
/// The hour the game is played at: mid-afternoon, a sun low enough to
/// model the facades and throw long shadows (the title is at golden hour).
const GAME_HOUR: f32 = 15.5;

/// The hour of day: `HW_HOUR` (0 to 24, e.g. 22 for night) or [`GAME_HOUR`],
/// until the debug menu sets another ([`set_game_hour`]).
fn game_hour() -> f32 {
    use std::sync::atomic::Ordering;
    let bits = HOUR.load(Ordering::Relaxed);
    if bits != u32::MAX {
        return f32::from_bits(bits);
    }
    let h = std::env::var("HW_HOUR")
        .ok()
        .and_then(|h| h.parse::<f32>().ok())
        .filter(|h| (0.0..=24.0).contains(h))
        .unwrap_or(GAME_HOUR);
    set_game_hour(h);
    h
}

/// The hour of day as bits (`u32::MAX`: not read yet).
static HOUR: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(u32::MAX);

fn set_game_hour(h: f32) {
    HOUR.store(h.to_bits(), std::sync::atomic::Ordering::Relaxed);
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
    /// Degrees per pixel of mouse, and -1 for inverted up and down.
    look: (f32, f32),
}

impl HeroCamera {
    const MAX_DISTANCE: f32 = 14.0;
    /// Metres of zoom per point the wheel scrolls (a notch is about 50).
    const ZOOM_PER_POINT: f32 = 0.02;

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
            let (yaw, pitch) = input.look_delta(self.look.0);
            self.yaw = (self.yaw + yaw).rem_euclid(360.0);
            self.pitch = (self.pitch + pitch * self.look.1).clamp(-80.0, 80.0);
        }
        // The mouse wheel zooms (not while it scrolls a window).
        let ctx = frame.ui.ctx();
        if !ctx.is_pointer_over_area() {
            let wheel = ctx.input(|i| i.smooth_scroll_delta.y);
            self.distance =
                (self.distance - wheel * Self::ZOOM_PER_POINT).clamp(0.0, Self::MAX_DISTANCE);
        }
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
                && (solids.solid_at(q)
                    || q.y < solids.ground_for(glam::DVec2::new(q.x, q.z), q.y) + 0.2)
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

/// Is a text field taking the keys (so letters are not game keys)?
fn on_ui_keys(frame: &Frame) -> bool {
    frame.ui.ctx().wants_keyboard_input()
}

/// Where an autorunning hero goes: ahead, plus any step aside (A/D) from
/// the keys, `keys` being the keys' direction on the ground.
fn autorun_dir(keys: Vec3, ahead: Vec3) -> Vec3 {
    let aside = keys - ahead * keys.dot(ahead);
    (ahead + aside).normalize_or_zero()
}

/// Keeps the hero's looks in the profile on this PC.
fn save_look(look: &actors_generator::CharacterConfig) {
    let mut p = Profile::load();
    p.look = Some(look.clone());
    p.save();
}

/// Keeps the hero file in the profile on this PC.
fn save_hero(hero: Option<hw_net::Sheet>) {
    let mut p = Profile::load();
    p.hero = hero;
    p.save();
}

fn main() -> EngineResult<()> {
    // A crash leaves a report beside the profile (see `crash.rs`).
    hw_client::crash::install();
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
    // R: the hero runs on by itself until S or R.
    let mut autorun = false;
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
    let mut traffic = hw_sim::traffic::Traffic::new(generated.area.clone());
    let mut citizens = hw_sim::citizens::Citizens::new(generated.area.clone(), traffic.signals());
    citizens.set_hour(game_hour() as f64);
    // Taxi stands of the town, the "Where to?" list (G), and where we asked to go.
    let mut stands = hw_sim::taxi::stands(&generated.area);
    let mut taxi_menu = false;
    let mut taxi_dest: Option<usize> = None;
    let mut taxi_seen = false;
    // The Aether Tube's "Where to?" list (T, in a station), and where we
    // asked to go.
    let mut tube_menu: Option<(usize, Vec<usize>)> = None;
    let mut tube_dest: Option<usize> = None;
    let mut tube_seen = false;
    // HW_TAXI=<stand> hails a taxi at the start and HW_TAXI_SECS runs the
    // world on that far (for screenshots of a ride).
    let mut taxi_demo =
        env_num("HW_TAXI").map(|d| (d as usize, env_num("HW_TAXI_SECS").unwrap_or(0)));
    let mut traffic_view = hw_client::trafficview::TrafficView::new();
    // Trouble bystanders notice, as the server tells it.
    let mut alarms = hw_sim::alarms::Alarms::default();
    // HW_WRECK=<frames>: that many frames in, the cars near the camera are
    // wrecked (for screenshots).
    let mut wreck_demo = env_num("HW_WRECK");
    // People from Actors, built on a worker thread from the packs.
    let actors = match people::packs_dir() {
        Some(dir) => {
            let p = People::start(dir);
            // Our own hero first: in the looks from the creator, or (a
            // hero from before it) dressed by name until the catalog is in.
            match &profile.look {
                Some(l) => p.request_look(looks::key(l), l.clone(), false),
                None => p.request(Who::Hero(name.clone()), 0),
            }
            for kind in VillainKind::ALL {
                for look in 0..people::VILLAIN_LOOKS {
                    p.request(Who::Villain(kind, look), 0);
                }
            }
            for look in 0..hw_sim::citizens::LOOKS + hw_sim::citizens::MOLE_LOOKS {
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
    let mut asked: BTreeSet<Who> = [match &profile.look {
        Some(l) => Who::Look(looks::key(l)),
        None => Who::Hero(name.clone()),
    }]
    .into();
    // Our hero's looks (None until made, or dressed by name once the hero
    // catalog is in), the catalog, and whether the others have them yet.
    let mut my_look: Option<actors_generator::CharacterConfig> = profile.look.clone();
    let mut hero_catalog: Option<actors_generator::CharacterCatalog> = None;
    let mut look_sent = false;
    // Which model each hero was drawn with (a look can arrive later).
    let mut hero_who: BTreeMap<hw_core::NetId, Who> = BTreeMap::new();
    // Characters built while the title showed, for when the game starts.
    let mut held: Vec<people::Built> = Vec::new();
    for kind in VillainKind::ALL {
        for look in 0..people::VILLAIN_LOOKS {
            asked.insert(Who::Villain(kind, look));
        }
    }
    // The fight: targets, effects, the screen.
    let mut fight = FightView::new();
    let mut robots = RobotView::new();
    let mut radio = hw_client::radioview::Radio::default();
    let mut signal = hw_client::dispatchview::Signal::default();
    let mut town_map = view == "map";
    // What was drawn last frame (for targeting), and a power key pressed
    // but not yet sent with a tick.
    let mut drawn: Vec<hw_net::EntityState> = Vec::new();
    let mut pending_power = 0u8;
    // Karma, levels and powers: the hero file from this PC (None until a new
    // hero picks an origin), the sheet as the host last confirmed it, and
    // the powers window.
    let mut powers = PowersView::new();
    let mut hero_file: Option<hw_net::Sheet> = profile.hero.clone();
    // HW_LEVEL=n starts a hero at level n with karma to spend (for trying
    // powers out); it is not saved.
    let cheat_level = std::env::var("HW_LEVEL")
        .ok()
        .and_then(|v| v.parse::<u8>().ok());
    if let Some(level) = cheat_level {
        let mut s = hero_file.clone().unwrap_or_else(hw_sim::progress::starter);
        s.lifetime = s
            .lifetime
            .max(hw_sim::progress::karma_for_level(level.clamp(1, 20)));
        s.karma = s.karma.max(20_000);
        // HW_POWERS=1 buys every power that is open (HW_THEME=1: the
        // second themes).
        if std::env::var_os("HW_POWERS").is_some() {
            let theme = env_num("HW_THEME").unwrap_or(0).min(1) as u8;
            s.karma = s.karma.max(1_000_000);
            for p in hw_client::powersview::SHOP {
                let _ = hw_sim::progress::buy(&mut s, p, theme);
            }
        }
        hero_file = Some(s);
    }
    if hero_file.is_none() && !title_mode && view != "origin" {
        // Runs straight into the game (tools, screenshots): the starter hero.
        hero_file = Some(hw_sim::progress::starter());
    }
    let mut my_sheet = hero_file.clone().unwrap_or_else(hw_sim::progress::starter);
    // Our file went to the host; the host confirmed it (from then on its
    // sheets are saved).
    let mut sheet_sent = false;
    let mut sheet_synced = false;
    let mut sheet_saved_at = 0.0f32;
    let mut sheet_dirty = false;
    if view == "powers" {
        powers.open = true;
    }
    // Characters a ragdoll moved, and how far from where the server has
    // them when they got up (faded out).
    let mut settle: BTreeMap<hw_core::NetId, (Vec3, bool)> = BTreeMap::new();
    let mut crowd = CrowdView::new();
    let mut anims: BTreeMap<hw_core::NetId, HeroAnim> = BTreeMap::new();
    let mut town = TownView::new(generated.world.clone());
    // The minimap (N) and the town's sounds and music.
    let mut minimap = hw_client::minimap::Minimap::new(generated.world.clone());
    let mut sounds = hw_client::soundscape::Soundscape::new();
    let mut hero_was: Option<(Vec3, f32)> = None;
    let mut hero_speed = 0.0f32;
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
        "HeroWorld {}: a town of {:.0} people, {} buildings around the start, planned in {:.1} s. Right mouse to look, WASD to run, Space to jump, V to fly, Tab for the free camera.",
        hw_core::version(),
        generated.population(),
        generated.area.buildings.len(),
        t0.elapsed().as_secs_f64()
    );
    let (spawn, spawn_dir) = generated.world.spawn_line();
    let mut hero_cam_yaw = 30.0f32;
    let mut fly = match view.as_str() {
        "street" | "citizen" | "alpha" => {
            let p = spawn - spawn_dir * 8.0;
            let yaw = spawn_dir.x.atan2(spawn_dir.y).to_degrees() as f32;
            FlyCamera::new((p.x as f32, ground(p) as f32 + 6.0, p.y as f32), yaw, 8.0)
        }
        "props" | "park" | "lawn" => {
            // Close to parked cars (or a park fountain; `lawn`: low over the
            // park's grass), for screenshots.
            use hw_gen::urban::props::PropKind;
            let want = |k: &PropKind| {
                if view == "park" || view == "lawn" {
                    matches!(k, PropKind::Fountain)
                } else {
                    // HW_PROP picks a kind by name ("Hover", "Atomic", ...).
                    let name = std::env::var("HW_PROP").unwrap_or_else(|_| "Car".into());
                    format!("{k:?}").contains(&name)
                }
            };
            let target = generated
                .area
                .props
                .iter()
                .filter(|p| want(&p.kind))
                .min_by(|a, b| a.pos.distance(spawn).total_cmp(&b.pos.distance(spawn)));
            let (c, side) = target.map_or((spawn, spawn_dir.perp()), |t| (t.pos, t.dir.perp()));
            let back = match view.as_str() {
                "park" => 22.0,
                "lawn" => 10.0,
                _ => 9.0,
            };
            let p = c + side * back + side.perp() * back * 0.5;
            let d = c - p;
            let yaw = d.x.atan2(d.y).to_degrees() as f32;
            let (up, pitch) = match view.as_str() {
                "park" => (9.0, -14.0),
                "lawn" => (1.5, -8.0),
                _ => (3.0, -14.0),
            };
            FlyCamera::new((p.x as f32, ground(p) as f32 + up, p.y as f32), yaw, pitch)
        }
        "pad" => {
            // Above the nearest rooftop taxi pad.
            let pad = generated
                .area
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
        "timber" => {
            // Across the street from the nearest half-timbered house.
            let house = generated
                .area
                .building_ids
                .iter()
                .filter_map(|id| generated.obstacles.building(id.0))
                .filter(|(p, _)| hw_client::facades::facade(p).timber.is_some())
                .min_by(|a, b| {
                    a.0.site
                        .origin
                        .distance(spawn)
                        .total_cmp(&b.0.site.origin.distance(spawn))
                });
            match house {
                Some((plan, base)) => {
                    let s = &plan.site;
                    let front = s.origin + s.along * (s.size.x * 0.5);
                    let p = front - s.inward * 13.0 + s.along * 4.0;
                    let d = front - p;
                    let yaw = d.x.atan2(d.y).to_degrees() as f32;
                    let eye = base + 2.0;
                    let pitch = (4.0 / d.length()).atan().to_degrees() as f32;
                    FlyCamera::new((p.x as f32, eye as f32, p.y as f32), yaw, pitch)
                }
                None => FlyCamera::new((spawn.x as f32, 40.0, spawn.y as f32), 0.0, -20.0),
            }
        }
        "plinth" => {
            // Close up at the spire's plaza: its edge, steps and base, from
            // just outside, at eye height.
            use hw_gen::urban::landmarks::LandmarkKind;
            let l = hw_client::landmarkview::nearest(
                &generated.world,
                LandmarkKind::AetherSpire,
                spawn,
            );
            let (target, p) = l.map_or((spawn, spawn + spawn_dir * 10.0), |l| {
                let side = l.stairs.first().map_or(l.dir, |s| s.down);
                (
                    l.pos + side * (l.plaza * 0.5),
                    l.pos + side * (l.plaza + 9.0) + side.perp() * 7.0,
                )
            });
            let d = target - p;
            let yaw = d.x.atan2(d.y).to_degrees() as f32;
            let eye = ground(p) + 2.6;
            let pitch = ((ground(target).max(l.map_or(0.0, |l| l.base)) + 1.5 - eye) / d.length())
                .atan()
                .to_degrees() as f32;
            FlyCamera::new((p.x as f32, eye as f32, p.y as f32), yaw, pitch)
        }
        "spire" | "lighthouse" | "tesla" => {
            // Looking up at a landmark from across its plaza.
            use hw_gen::urban::landmarks::LandmarkKind;
            let kind = match view.as_str() {
                "spire" => LandmarkKind::AetherSpire,
                "tesla" => LandmarkKind::TeslaTower,
                _ => LandmarkKind::Lighthouse,
            };
            let l = hw_client::landmarkview::nearest(&generated.world, kind, spawn);
            let (c, base, h, face) = l.map_or((spawn, ground(spawn), 30.0, spawn_dir), |l| {
                // The lighthouse faces the land: look at it from the sea.
                let face = if view == "lighthouse" { -l.dir } else { l.dir };
                (l.pos, l.base, l.height, face)
            });
            // From above the rooftops, the landmark's whole height in view.
            let back = if view == "tesla" { 1.5 } else { 0.9 };
            let p = c + face * (h * back + 12.0) + face.perp() * h * 0.35;
            let d = c - p;
            let yaw = d.x.atan2(d.y).to_degrees() as f32;
            let eye = ground(p).max(base) + h * 0.5 + 6.0;
            let pitch = ((h * 0.4 - (eye - base)) / d.length()).atan().to_degrees() as f32;
            FlyCamera::new((p.x as f32, eye as f32, p.y as f32), yaw, pitch)
        }
        "quay" => {
            // Along a quay wall at the nearest steps down to the water.
            let st = generated
                .area
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
            for r in &generated.world.main_roads {
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
        "signals" => {
            // Up the road at the traffic light nearest the start, its lamps
            // and the junction behind it.
            let lights = generated
                .area
                .props
                .iter()
                .filter(|p| p.kind == hw_gen::urban::props::PropKind::TrafficLight);
            match lights.min_by(|a, b| a.pos.distance(spawn).total_cmp(&b.pos.distance(spawn))) {
                Some(l) => {
                    let p = l.pos + l.dir * 5.0 - l.dir.perp() * 1.5;
                    let d = l.pos - p;
                    let yaw = d.x.atan2(d.y).to_degrees() as f32;
                    FlyCamera::new((p.x as f32, l.y as f32 + 3.0, p.y as f32), yaw, 0.0)
                }
                None => FlyCamera::new((spawn.x as f32, 10.0, spawn.y as f32), 0.0, -10.0),
            }
        }
        "graffiti" => {
            // In front of a tagged wall at the quarter's edge.
            let w = &generated.world;
            let a = &generated.area;
            let tags =
                hw_gen::urban::edge::tags(w, a.building_ids.iter().copied().zip(&a.buildings));
            match tags.iter().find(|t| t.image == 0).or(tags.first()) {
                Some(t) => {
                    let at = DVec2::new(t.at.x, t.at.z);
                    let p = at + t.normal * 7.0 + t.normal.perp() * 2.0;
                    let d = at - p;
                    let yaw = d.x.atan2(d.y).to_degrees() as f32;
                    FlyCamera::new((p.x as f32, t.at.y as f32 + 0.3, p.y as f32), yaw, -3.0)
                }
                None => FlyCamera::new((spawn.x as f32, 40.0, spawn.y as f32), 0.0, -20.0),
            }
        }
        "edge" => {
            // Outside the quarter, looking in past its first barricade.
            let w = &generated.world;
            match hw_gen::urban::edge::barricades(w, &generated.area.roads).first() {
                Some(b) => {
                    let p = b.at - b.along * 14.0 + b.along.perp() * (b.half + 1.0);
                    let d = b.at + b.along * 10.0 - p;
                    let yaw = d.x.atan2(d.y).to_degrees() as f32;
                    FlyCamera::new((p.x as f32, ground(p) as f32 + 2.2, p.y as f32), yaw, -6.0)
                }
                None => FlyCamera::new((spawn.x as f32, 40.0, spawn.y as f32), 0.0, -20.0),
            }
        }
        "rooftop" => {
            // Across the street from the quarter's first rooftop lookout, a
            // little above its roof.
            let w = &generated.area;
            let corners = hw_sim::quarter::corners(w);
            match hw_sim::quarter::lookouts(w, &corners).first() {
                Some(l) => {
                    let top = hw_sim::quarter::perch(&solids, l)
                        .unwrap_or(glam::DVec3::new(l.at.x, 20.0, l.at.y));
                    let (at, roof) = (glam::DVec2::new(top.x, top.z), top.y);
                    let p = at + l.facing * 13.0 + l.facing.perp() * 5.0;
                    let d = at - p;
                    let yaw = d.x.atan2(d.y).to_degrees() as f32;
                    let pitch = (-2.0 / d.length()).atan().to_degrees() as f32;
                    FlyCamera::new((p.x as f32, roof as f32 + 3.0, p.y as f32), yaw, pitch)
                }
                None => FlyCamera::new((spawn.x as f32, 40.0, spawn.y as f32), 0.0, -20.0),
            }
        }
        "quarter" => {
            // On a street in the mad scientist's quarter, looking at his tower.
            use hw_gen::urban::landmarks::LandmarkKind;
            let w = &generated.area;
            let tower = hw_client::landmarkview::nearest(w, LandmarkKind::TeslaTower, spawn);
            let (c, h) = tower.map_or((spawn, 40.0), |l| (l.pos, l.height));
            let deep = |p: DVec2| w.town.dystopia.as_ref().is_some_and(|d| d.depth(p) > 0.9);
            let p = w
                .roads
                .iter()
                .flat_map(|r| r.pts.iter().copied())
                .filter(|p| deep(*p))
                .min_by(|a, b| {
                    (a.distance(c) - 110.0)
                        .abs()
                        .total_cmp(&(b.distance(c) - 110.0).abs())
                })
                .unwrap_or(spawn);
            let d = c - p;
            let yaw = d.x.atan2(d.y).to_degrees() as f32;
            let pitch = ((h * 0.5 - 3.0) / d.length()).atan().to_degrees() as f32;
            FlyCamera::new((p.x as f32, ground(p) as f32 + 3.0, p.y as f32), yaw, pitch)
        }
        "market" if generated.world.underground().cavern.is_some() => {
            // Molehaven's market from above: the well, the stalls and the
            // mole people about them.
            let c = generated
                .world
                .underground()
                .cavern
                .as_ref()
                .expect("cavern");
            let a = 0.6 + std::f64::consts::TAU / 10.0;
            let p = c.centre + DVec2::new(a.cos(), a.sin()) * 14.0;
            let d = c.centre - p;
            let yaw = d.x.atan2(d.y).to_degrees() as f32;
            FlyCamera::new((p.x as f32, c.floor as f32 + 8.0, p.y as f32), yaw, -26.0)
        }
        "lift" | "tunnel" | "molehaven" => {
            // The underground: a lift pavilion on the street, a utility
            // tunnel from inside, or Molehaven's great hall.
            let u = generated.world.underground();
            let lift = u
                .entrances
                .iter()
                .filter(|e| !e.molehaven)
                .min_by(|a, b| a.pos.distance(spawn).total_cmp(&b.pos.distance(spawn)));
            let tunnel = u
                .voids
                .iter()
                .filter(|v| v.kind == hw_gen::urban::underground::VoidKind::Tunnel)
                .filter(|v| v.max.x - v.min.x > 30.0)
                .min_by(|a, b| {
                    a.origin
                        .distance(spawn)
                        .total_cmp(&b.origin.distance(spawn))
                });
            match (view.as_str(), lift, tunnel, &u.cavern) {
                ("lift", Some(e), _, _) => {
                    let p = e.pos + e.door * 9.0 + e.door.perp() * 4.0;
                    let d = e.pos - p;
                    let yaw = d.x.atan2(d.y).to_degrees() as f32;
                    FlyCamera::new((p.x as f32, e.top as f32 + 3.0, p.y as f32), yaw, -8.0)
                }
                ("tunnel", _, Some(v), _) => {
                    let p = v.world(DVec2::new(v.min.x + 2.0, 0.0)) - v.inward() * 0.8;
                    let yaw = v.along.x.atan2(v.along.y).to_degrees() as f32;
                    let y = v.floor_at(p) + 1.8;
                    FlyCamera::new((p.x as f32, y as f32, p.y as f32), yaw, -4.0)
                }
                ("molehaven", _, _, Some(c)) => {
                    let hall = &u.voids[c.chambers[0]];
                    let p = hall.world(DVec2::new(hall.max.x - 4.0, hall.max.y * 0.5));
                    let d = c.centre - p;
                    let yaw = d.x.atan2(d.y).to_degrees() as f32;
                    FlyCamera::new((p.x as f32, c.floor as f32 + 5.0, p.y as f32), yaw, -10.0)
                }
                _ => FlyCamera::new(
                    (spawn.x as f32, ground(spawn) as f32 + 6.0, spawn.y as f32),
                    0.0,
                    -10.0,
                ),
            }
        }
        "station" | "tubeline" | "ruin" | "lab" | "grotto" | "kiosk" | "tube" | "capsule" => {
            // The deep underground: a tube station's hall, a tube stretch from
            // inside, a buried ruin, the villain's lab, a grotto in the Deeps,
            // the lab's street kiosk, or a station's street pavilion.
            hw_client::deepview::view_camera(&generated.world, view.as_str(), spawn).unwrap_or_else(
                || {
                    FlyCamera::new(
                        (spawn.x as f32, ground(spawn) as f32 + 6.0, spawn.y as f32),
                        0.0,
                        -10.0,
                    )
                },
            )
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
    let mut flying = !matches!(
        view.as_str(),
        "hero" | "inside" | "sky" | "title" | "fight" | "hangout"
    );
    if view == "inside" {
        // Start in the lobby of the tallest building near the start.
        let tower = town.tallest_near(spawn, 400.0);
        if let Some((i, plan)) = tower.and_then(|i| Some((i, town.plan(i)?))) {
            if plan.core.is_some() {
                let base = generated
                    .world
                    .building(hw_gen::urban::quarter::DetailId(i))
                    .map_or(0.0, |b| b.base);
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
    if view == "hangout" {
        // Down the street from a Chrome Jackets corner.
        let w = &generated.area;
        if let Some(c) = hw_sim::quarter::hangouts(w, spawn, hw_sim::HANGOUT_CLEAR).first() {
            // Down their pavement, from the kerb, looking at them.
            let p = c.at + c.facing.perp() * 26.0 + c.facing * 6.0;
            if let Some(s) = session.as_mut() {
                s.teleport(glam::DVec3::new(p.x, ground(p) + 0.3, p.y));
            }
            let ahead = (c.at - p).normalize_or(-c.facing);
            hero_cam_yaw = ahead.x.atan2(ahead.y).to_degrees() as f32;
        }
    }
    if view == "rooftop" {
        // In the street below the quarter's first rooftop lookout.
        let w = &generated.area;
        let corners = hw_sim::quarter::corners(w);
        if let Some(l) = hw_sim::quarter::lookouts(w, &corners).first() {
            let p = l.at + l.facing * 30.0;
            if let Some(s) = session.as_mut() {
                s.teleport(glam::DVec3::new(p.x, ground(p) + 0.3, p.y));
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
    let mut menu = hw_client::settings::GameMenu::new(&profile);
    // The debug menu (Ctrl+Shift+F12). A hero it changed (or one from
    // HW_LEVEL) is not saved; weak points of the colossus it broke.
    let mut debug = hw_client::debugmenu::DebugMenu::new();
    // `HW_DEBUG=<tab>` opens it at start (screenshots).
    if let Ok(tab) = std::env::var("HW_DEBUG") {
        debug.open_on(&tab);
    }
    let mut cheated = cheat_level.is_some();
    let mut colossus_breaks = 0u8;
    // The world the crash reporter last noted.
    let mut crash_noted: Option<u64> = None;
    let mut hero_cam = HeroCamera {
        look: menu.look,
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
    // Taxi canopies: the hinged part, and how far open (0 shut, 1 up).
    let mut canopies: BTreeMap<hw_core::NetId, (EntityId, f32)> = BTreeMap::new();
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
    // Closed unexpectedly last time: say where the report is.
    if let Some(report) = hw_client::crash::take_unseen() {
        let line = format!(
            "HeroWorld closed unexpectedly last time. A report is in {}: please send it to Arnd.",
            report.display()
        );
        eprintln!("{line}");
        if let Some(t) = title_screen.as_mut() {
            t.status = Some((line, true));
        }
    }
    hw_client::crash::note("view", &view);
    let mut switching: Option<std::sync::mpsc::Receiver<(u64, f64, GeneratedTown)>> = None;
    let mut waiting: Option<Waiting> = None;
    // Props and landmarks of the current town.
    let mut extras = TownExtras::default();
    // Behind the title the props, trees and ground come in once the logo
    // has landed (they take a moment), not on the first frame.
    let mut extras_loaded = false;
    // From the title down to the hero: (start, eye, yaw, pitch).
    let mut swoop: Option<(f32, Vec3, f32, f32)> = None;
    let mut music_fade_at: Option<f32> = None;
    // Broken buildings, once a game is on.
    let mut damage: Option<DamageView> = None;
    let shooting = std::env::var_os("ENGINE_SCREENSHOT").is_some();
    let run = Engine::run(
        format!("HeroWorld {}", hw_core::version()),
        move |world, frame| {
            if frame.first {
                world.set_view_distance(6000.0)?;
                // The player's settings (screenshots keep full detail).
                let mut p = profile.clone();
                p.smooth &= !shooting;
                hw_client::settings::apply(world, &p);
                hw_client::settings::apply_volumes(world, &p);
                // `HW_CRASH_TEST=1`: crash at once (tries the crash reporter).
                if std::env::var_os("HW_CRASH_TEST").is_some() {
                    panic!("HW_CRASH_TEST asked for a crash");
                }
                hw_client::lighting::set_sky(world, game_hour());
                world.set_moons(moons())?;
                if let Some(t) = title_screen.as_mut() {
                    t.enter(world);
                    let (eye, yaw, pitch) = t.camera(&generated.world, &solids);
                    world.look_first_person(eye, yaw, pitch);
                }
                // Behind the title the town builds bit by bit while you look.
                let eye = world.camera().eye();
                let eye = DVec2::new(eye.x as f64, eye.z as f64);
                let all_now = title_screen.is_none() || shooting;
                if all_now {
                    town.build_all(world, eye);
                    extras = spawn_extras(world, &generated, town.green.clone())?;
                    extras_loaded = true;
                    // `HW_VIEW=capsule`: a tube capsule standing at the
                    // nearest platform, for a close-up.
                    if view == "capsule" {
                        if let Some((p, d)) = hw_client::deepview::capsule_spot(
                            &generated.world,
                            generated.world.spawn_line().0,
                        ) {
                            let place = Place::new(p.x as f32, p.y as f32, p.z as f32)
                                .with_yaw_deg(d.x.atan2(d.y).to_degrees() as f32);
                            let e = world.place(hw_client::deepview::capsule_mesh(), place)?;
                            extras.entities.push(e);
                        }
                    }
                    if let Some(v) = extras.props.as_mut() {
                        v.update(eye, f64::INFINITY);
                        v.flush(world)?;
                    }
                    if let Some(w) = extras.waterfront.as_mut() {
                        w.update(world, eye, false);
                    }
                }
            }

            // What heroes can be made of, once the packs are loaded.
            if hero_catalog.is_none() {
                if let Some(c) = actors.as_ref().and_then(|p| p.take_catalog()) {
                    if let Some(t) = title_screen.as_mut() {
                        t.set_catalog(c.clone());
                    }
                    hero_catalog = Some(c);
                }
            }
            if let Some(t) = title_screen.as_mut() {
                t.no_people = actors.is_none();
                // The hero in the foreground: built on the worker, the
                // creator's newest looks first.
                if let Some(p) = &actors {
                    if let Some((key, look)) = t.wanted_look() {
                        p.request_look(key, look, true);
                    }
                    let want = t.wanted_key();
                    for b in p.finished() {
                        match b.who {
                            Who::Look(k) => {
                                t.hero_built(world, k, b.model.clone())?;
                                if Some(k) == want {
                                    asked.insert(b.who.clone());
                                    models.insert(b.who.clone(), b.model);
                                }
                            }
                            _ => held.push(b),
                        }
                    }
                    // Looks left behind in the creator are not kept.
                    let mine = my_look.as_ref().map(looks::key);
                    models.retain(|w, _| match w {
                        Who::Look(k) => Some(*k) == want || Some(*k) == mine,
                        _ => true,
                    });
                    asked.retain(|w| match w {
                        Who::Look(k) => models.contains_key(w) || Some(*k) == mine,
                        _ => true,
                    });
                }
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
                    minimap = hw_client::minimap::Minimap::new(generated.world.clone());
                    sounds.stop(world);
                    traffic = hw_sim::traffic::Traffic::new(generated.area.clone());
                    citizens =
                        hw_sim::citizens::Citizens::new(generated.area.clone(), traffic.signals());
                    citizens.set_hour(game_hour() as f64);
                    stands = hw_sim::taxi::stands(&generated.area);
                    traffic_view.clear(world);
                    radio.clear(world);
                    signal.clear(world);
                    crowd.clear(world);
                    extras = spawn_extras(world, &generated, town.green.clone())?;
                }
                if !extras_loaded && t.wants_heavy() {
                    extras = spawn_extras(world, &generated, town.green.clone())?;
                    extras_loaded = true;
                }
                let eye = world.camera().eye();
                let eye = DVec2::new(eye.x as f64, eye.z as f64);
                town.update_background(world, eye, TITLE_BUILD_MS);
                if let Some(v) = extras.props.as_mut() {
                    v.update(eye, TITLE_BUILD_MS * 0.5);
                    v.flush(world)?;
                }
                if let Some(w) = extras.waterfront.as_mut() {
                    w.update(world, eye, true);
                }
                let (built, wanted) = town.progress(eye);
                let (done, what) = if switching.is_some() {
                    (0.0, "Building your world...")
                } else if wanted == 0 {
                    (1.0, "")
                } else {
                    (built as f32 / wanted as f32, "Building the city...")
                };
                t.set_world_ready(extras_loaded && switching.is_none(), done);
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
                    Some(Choice::NewHero {
                        name: n,
                        seed,
                        look,
                    }) => {
                        name = n;
                        t.profile.name = name.clone();
                        t.profile.seed = seed;
                        t.profile.look = look.map(|l| *l);
                        // A new hero picks an origin first.
                        t.profile.hero = None;
                        hero_file = None;
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
                    // A hero from before the creator keeps the clothes
                    // the game dressed them in.
                    if t.profile.look.is_none() {
                        if let Some(cat) = &hero_catalog {
                            t.profile.look = Some(hw_client::creator::Creator::random_look(
                                cat,
                                people::name_seed(&name),
                            ));
                        }
                    }
                    my_look = t.profile.look.clone();
                    t.profile.save();
                    menu = hw_client::settings::GameMenu::new(&t.profile);
                    t.leave(world);
                    // The game builds its cells itself, as before.
                    town.stop_background();
                    title_screen = None;
                }
                return Ok(());
            }
            if music_fade_at.is_some_and(|at| frame.time > at) {
                // The title theme fades, unless a colossus fight took over.
                if !robots.music_on() {
                    world.stop_music(8.0);
                }
                music_fade_at = None;
            }
            let Some(session) = session.as_mut() else {
                return Ok(());
            };
            let open_at = session.open_address();
            version_corner(
                &frame.ui,
                &mut whats_new,
                session.room_code(),
                open_at.as_deref(),
            );
            if crash_noted != Some(world_seed) {
                crash_noted = Some(world_seed);
                hw_client::crash::note("world seed", world_seed);
                hw_client::crash::note("people", world_people);
                hw_client::crash::note("room", session.room_code().unwrap_or("none"));
            }
            // Escape: the menu (settings, controls, quit).
            if menu.show(&frame.ui, world) {
                world.request_exit();
            }
            hero_cam.look = menu.look;
            // Ctrl+Shift+F12: the debug menu (see `debugmenu.rs`).
            if hw_client::debugmenu::hotkey(frame.ui.ctx()) {
                debug.open = !debug.open;
                if debug.open {
                    world.set_pointer_lock(false);
                }
            }
            if debug.open {
                use hw_client::debugmenu::{self as dm, DebugAction as A};
                debug.find_places(
                    world_seed ^ world_people.to_bits(),
                    &generated.area,
                    &solids,
                );
                let me = session.my_position(&drawn);
                let mut calls: Vec<(u32, &'static str, f32)> = radio
                    .calls
                    .iter()
                    .map(|(id, c)| {
                        let d = me.map_or(0.0, |m| m.as_vec3().distance(c.at));
                        (*id, c.kind.name(), d)
                    })
                    .collect();
                calls.sort_by(|a, b| a.2.total_cmp(&b.2));
                let info = dm::DebugInfo {
                    sheet: &my_sheet,
                    calls,
                    hour: game_hour(),
                    local: !session.joined(),
                    unsaved: cheated,
                };
                let at = me.unwrap_or_else(|| world.camera().eye().as_dvec3());
                for a in debug.ui(frame.ui.ctx(), &info) {
                    // Puts the hero at `p`, looking along `facing`.
                    let mut go = |session: &mut Session, p: glam::DVec3, facing: DVec2| {
                        session.teleport(p);
                        flying = false;
                        hero_cam.yaw = facing.x.atan2(facing.y).to_degrees() as f32;
                        hero_cam.pitch = -12.0;
                    };
                    match a {
                        A::Go(i) => {
                            let Some(p) = debug.places.get(i).cloned() else {
                                continue;
                            };
                            if p.fly && !my_sheet.has(hw_net::Power::Flight) {
                                fight.say("Flight first: Hero tab, Level 20, Buy all open");
                                continue;
                            }
                            go(session, p.at, p.facing);
                            if p.fly {
                                hero_flies = true;
                            }
                            fight.say(&p.name);
                        }
                        A::GoCall(id) => {
                            let Some(c) = radio.calls.get(&id) else {
                                continue;
                            };
                            let c2 = DVec2::new(c.at.x as f64, c.at.z as f64);
                            let p = c2 + c.facing * 14.0;
                            let to = dm::clear_near(&solids, p, None).unwrap_or(glam::DVec3::new(
                                p.x,
                                ground(p) + 0.3,
                                p.y,
                            ));
                            go(
                                session,
                                to,
                                (c2 - DVec2::new(to.x, to.z)).normalize_or(DVec2::X),
                            );
                        }
                        A::Level(l) => {
                            cheated = true;
                            session.debug_sheet(|s| dm::at_level(s, l));
                        }
                        A::Karma(k) => {
                            cheated = true;
                            session.debug_sheet(|mut s| {
                                s.karma = s.karma.saturating_add(k);
                                s
                            });
                        }
                        A::BuyAll(theme) => {
                            cheated = true;
                            session.debug_sheet(|s| dm::buy_all(s, theme));
                        }
                        A::ClearPowers => {
                            cheated = true;
                            session.debug_sheet(dm::clear_powers);
                        }
                        A::Regard(f, by) => {
                            cheated = true;
                            session.debug_sheet(|s| dm::regard(s, f, by));
                        }
                        A::ResetRegard => {
                            cheated = true;
                            session.debug_sheet(dm::reset_regard);
                        }
                        A::Heal => session.debug_heal(),
                        A::RestoreHero => {
                            if let Some(s) = hero_file.clone() {
                                session.send_sheet(s);
                            }
                            cheated = cheat_level.is_some();
                        }
                        A::Incident(hw_sim::incidents::IncidentKind::Collapse) => {
                            if session.debug_collapse(at).is_none() {
                                fight.say("No small building to bring down near here");
                            }
                        }
                        A::Incident(k) => {
                            session.debug_incident(k);
                            fight.say(&format!("Starting: {} (Go to lists it)", k.name()));
                        }
                        A::BlastBuilding => {
                            if session.debug_blast(at).is_none() {
                                fight.say("No building near here");
                            }
                        }
                        A::WreckCars => {
                            let n = session.wreck_cars(at, 45.0);
                            fight.say(&format!("Wrecked {n} cars"));
                        }
                        A::SmashProps => {
                            let n = session.debug_smash_props(&generated, at, 25.0);
                            fight.say(&format!("Knocked over {n} things"));
                        }
                        A::KnockPeople => {
                            let n = crowd.knock_down(world, at.as_vec3() + Vec3::Y, 10.0);
                            fight.say(&format!("Knocked over {n} people"));
                        }
                        A::BreakColossus => {
                            colossus_breaks = (colossus_breaks + 1).min(8);
                            session.debug_break_colossus(colossus_breaks);
                        }
                        A::Hour(h) => {
                            let was = hw_client::nightlights::dark_at(game_hour());
                            set_game_hour(h);
                            hw_client::lighting::set_sky(world, h);
                            citizens.set_hour(h as f64);
                            let dark = hw_client::nightlights::dark_at(h);
                            if dark != was {
                                // Lit windows are baked into the buildings.
                                hw_client::buildingmesh::NIGHT
                                    .store(dark, std::sync::atomic::Ordering::Relaxed);
                                let cells = town.built_cells();
                                town.rebuild(world, cells);
                            }
                        }
                        A::FastForward(s) => session.fast_forward(s * hw_sim::TICK_HZ),
                        A::FreeCamera => {
                            flying = !flying;
                            if flying {
                                fly.yaw_deg = hero_cam.yaw;
                                fly.pitch_deg = hero_cam.pitch;
                            }
                        }
                        A::Cutaway => {
                            let mut changed = town.cells_near(town.cut_focus, CUT_REACH);
                            town.cut_focus = DVec2::new(at.x, at.z);
                            town.cut_on = !town.cut_on;
                            changed.extend(town.cells_near(town.cut_focus, CUT_REACH));
                            town.rebuild(world, changed);
                        }
                        A::Map => town_map = !town_map,
                    }
                }
            }
            // Our looks go to the others once (a hero from before the
            // creator is dressed by name as soon as the catalog is in).
            if my_look.is_none() && title_screen.is_none() {
                if let Some(cat) = &hero_catalog {
                    let l = hw_client::creator::Creator::random_look(cat, people::name_seed(&name));
                    if std::env::var_os("HW_NAME").is_none() {
                        save_look(&l);
                    }
                    my_look = Some(l);
                }
            }
            if !look_sent {
                if let Some(l) = &my_look {
                    look_sent = true;
                    session.send_look(looks::encode(l));
                }
            }
            // Our hero file goes to the host once.
            if !sheet_sent {
                sheet_sent = true;
                if let Some(s) = hero_file.clone() {
                    session.send_sheet(s);
                }
            }
            let creating = hero_file.is_none();
            if let (Some(lights), Some(props)) = (extras.lights.as_mut(), extras.props.as_ref()) {
                let dark = swoop.is_none() && hw_client::nightlights::dark_at(game_hour());
                lights.update(world, world.camera().eye(), dark, props)?;
            }
            if let Some(under) = extras.under.as_mut() {
                under.update(world, world.camera().eye(), &generated.world)?;
            }
            if let Some(deep) = extras.deep.as_mut() {
                deep.update(world, world.camera().eye())?;
            }
            if let Some(g) = extras.grass.as_mut() {
                g.update(world, world.camera().eye())?;
            }
            if let Some(d) = extras.dystopia.as_mut() {
                d.update(world, &generated.world, frame.dt)?;
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
            if let Some(n) = wreck_demo.as_mut() {
                if *n == 0 {
                    let eye = world.camera().eye().as_dvec3();
                    eprintln!("wrecked {} cars", session.wreck_cars(eye, 45.0));
                    wreck_demo = None;
                } else {
                    *n -= 1;
                }
            }
            if let Some((dest, secs)) = taxi_demo.take() {
                session.hail_taxi(dest as u8);
                taxi_dest = Some(dest);
                session.fast_forward(secs as u32 * hw_sim::TICK_HZ);
            }
            let mut ride_yaw: Option<f32>;
            let mut ride_close = false;
            // In a tube station or a capsule, G is for the tube, not a taxi.
            let tube_here = {
                let my_id = session.my_hero();
                drawn.iter().find(|e| Some(e.id) == my_id).is_some_and(|m| {
                    hw_sim::tube::station_at(generated.world.tube(), DVec3::from_array(m.pos))
                        .is_some()
                        || drawn
                            .iter()
                            .any(|t| t.kind == hw_net::EntityKind::Capsule && t.owner == m.owner)
                })
            };
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
                if !flying && !tube_here && input.pressed(Key::G) {
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
            // G in a tube station: the Aether Tube's "Where to?"; in a capsule
            // (or waiting for one), out.
            {
                let net = generated.world.tube();
                let my_id = session.my_hero();
                let me = drawn.iter().find(|e| Some(e.id) == my_id);
                let capsule = me.and_then(|m| {
                    drawn
                        .iter()
                        .find(|t| t.kind == hw_net::EntityKind::Capsule && t.owner == m.owner)
                });
                let riding = me.is_some_and(|m| hw_sim::tube::riding(m, &drawn));
                if let Some(t) = capsule.filter(|_| riding) {
                    let (y, _, _) = glam::Quat::from_array(t.rot).to_euler(glam::EulerRot::YXZ);
                    ride_yaw = Some(y.to_degrees());
                    ride_close = true;
                }
                let here = me.and_then(|m| hw_sim::tube::station_at(net, DVec3::from_array(m.pos)));
                if !flying && tube_here && input.pressed(Key::G) {
                    if capsule.is_some() {
                        session.ride_tube(hw_sim::tube::HOP_OUT);
                        tube_dest = None;
                    } else if tube_menu.is_some() {
                        tube_menu = None;
                    } else if let Some(s) = here {
                        tube_menu = Some((s, hw_sim::tube::reachable(net, s)));
                        world.set_pointer_lock(false);
                    }
                }
                // Walked out of the station: the list goes.
                if tube_menu.as_ref().is_some_and(|(s, _)| here != Some(*s)) {
                    tube_menu = None;
                }
                if let Some(i) = tube_panel(&frame.ui, &mut tube_menu, net) {
                    session.ride_tube(i as u16);
                    tube_dest = Some(i);
                }
                if capsule.is_some() {
                    tube_seen = true;
                } else if tube_seen {
                    tube_seen = false;
                    tube_dest = None;
                }
                let to = tube_dest
                    .and_then(|i| net.stations.get(i))
                    .map(|s| s.name.as_str());
                tube_status(&frame.ui, capsule.is_some(), riding, to, here.is_some());
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
            let can_fly = my_sheet.has(hw_net::Power::Flight);
            if !can_fly {
                hero_flies = false;
            }
            if input.pressed(Key::B) && !creating {
                powers.open = !powers.open;
            }
            // Clicks on a window are not punches.
            let on_ui = frame.ui.ctx().is_pointer_over_area();
            if !flying {
                if input.pressed(Key::V) {
                    if can_fly {
                        hero_flies = !hero_flies;
                    } else {
                        fight.say(&format!(
                            "Flight opens at level {} (B: powers)",
                            hw_sim::progress::tier_level(hw_sim::progress::tier(
                                hw_net::Power::Flight
                            ))
                        ));
                    }
                }
                if input.pressed(Key::R) && !creating && !on_ui_keys(frame) {
                    autorun = !autorun;
                } else if input.pressed(Key::S) {
                    autorun = false;
                }
                let mut dir = input.move_dir_xz(hero_cam.yaw);
                if auto_walk || autorun {
                    dir = autorun_dir(dir, Camera::facing_xz(hero_cam.yaw));
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
                hero_input.smash = input.mouse_down(MouseButton::Left) && !on_ui;
                hero_input.blast = input.down(Key::Q);
                hero_input.aim = Camera::direction(hero_cam.yaw, hero_cam.pitch + 10.0).to_array();
                // Fighting: keys, target, revive.
                let me_state = session
                    .my_hero()
                    .and_then(|id| drawn.iter().find(|e| e.id == id));
                let cam = world.camera();
                let look = (cam.target() - cam.eye()).normalize_or_zero();
                let f = fight.input(input, me_state, &drawn, cam.eye(), look, Some(&my_sheet));
                if f.power != 0 && !(on_ui && f.power == hw_net::Power::Strike.code()) {
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
                    // Every power the hero has, in turn; punches up close.
                    let turn = [3u8, 2, 6, 11, 2, 5, 13, 2, 4, 10, 2, 15, 5];
                    let k = (frame.time / 0.75) as usize % turn.len();
                    let want = hw_net::Power::from_code(turn[k]).filter(|p| my_sheet.has(*p));
                    pending_power = match want {
                        Some(p) if !(close && p == hw_net::Power::Bolt) => p.code(),
                        _ if close => 1,
                        _ => 2,
                    };
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
                    0 => {}
                    1 => i.smash = true,
                    2 => i.blast = true,
                    other => i.power = other,
                }
                session.tick(i);
                // The host left and GameHub made us the new one.
                if session.wants_take_over()
                    && session.take_over(Seed(world_seed), &generated, &name)
                {
                    sheet_sent = false;
                    look_sent = false;
                }
            }
            if ticks == 5 {
                tick_clock = 0.0;
            }
            // Damage: broken buildings, repairs, impacts.
            let joined = session.joined();
            let mut news = session.take_news();
            // Broken and mended street props: their places, and lamps on them.
            radio.solids = Some(solids.clone());
            radio.town = Some(generated.world.clone());
            news.retain(|m| match m {
                hw_net::ToClient::Incident { id, at, yaw, .. } => {
                    radio.hear(world, m);
                    // `HW_VIEW=incident`: the camera goes to the first call.
                    if view == "incident" && radio.calls.len() == 1 && radio.calls.contains_key(id)
                    {
                        let c = Vec3::from_array(*at);
                        // From its front, close by, or the nearest way round
                        // that no wall blocks.
                        let a0 = hw_sim::props::byte_to_yaw(*yaw) as f32;
                        let clear = |a: f32| {
                            (1..=20).all(|k| {
                                let q = c
                                    + Vec3::new(a.sin(), 0.0, a.cos()) * (k as f32 * 0.5)
                                    + Vec3::Y * 1.6;
                                !solids.solid_at(q.as_dvec3())
                            })
                        };
                        let a = (0..16)
                            .map(|k| {
                                let s = if k % 2 == 0 { 1.0 } else { -1.0 };
                                a0 + s * ((k + 1) / 2) as f32 * std::f32::consts::TAU / 16.0
                            })
                            .find(|&a| clear(a))
                            .unwrap_or(a0);
                        let p = c + Vec3::new(a.sin(), 0.0, a.cos()) * 9.0 + Vec3::Y * 3.5;
                        let d = c - p;
                        let yaw = d.x.atan2(d.z).to_degrees();
                        let pitch = (d.y / glam::Vec2::new(d.x, d.z).length())
                            .atan()
                            .to_degrees();
                        fly = FlyCamera::new((p.x, p.y, p.z), yaw, pitch + 22.0);
                    }
                    // `HW_VIEW=colossus`: the walking tank from in front, a
                    // little above, as a hero would meet it.
                    // (`HW_VIEW=colossus_close`: lower, to see the sentries it drops.)
                    let close = view == "colossus_close";
                    if (view == "colossus" || close)
                        && radio.calls.len() == 1
                        && radio.calls.contains_key(id)
                    {
                        let c = Vec3::from_array(*at);
                        let a = hw_sim::props::byte_to_yaw(*yaw) as f32 + 0.5;
                        let (out, up) = if close { (18.0, 9.0) } else { (16.0, 52.0) };
                        let p = c + Vec3::new(a.sin(), 0.0, a.cos()) * out + Vec3::Y * up;
                        let d = c + Vec3::Y * 4.0 - p;
                        let yaw = d.x.atan2(d.z).to_degrees();
                        let pitch = (d.y / glam::Vec2::new(d.x, d.z).length())
                            .atan()
                            .to_degrees();
                        fly = FlyCamera::new((p.x, p.y, p.z), yaw, pitch);
                    }
                    // `HW_VIEW=signal`: from a rooftop, up at the emblem.
                    if view == "signal" && radio.calls.len() == 1 && radio.calls.contains_key(id) {
                        let c = Vec3::from_array(*at)
                            + Vec3::Y * hw_client::dispatchview::SIGNAL_HEIGHT as f32;
                        let a = hw_sim::props::byte_to_yaw(*yaw) as f32;
                        let p = Vec3::from_array(*at)
                            + Vec3::new(a.sin(), 0.0, a.cos()) * 260.0
                            + Vec3::Y * 40.0;
                        let d = c - p;
                        let yaw = d.x.atan2(d.z).to_degrees();
                        let pitch = (d.y / glam::Vec2::new(d.x, d.z).length())
                            .atan()
                            .to_degrees();
                        fly = FlyCamera::new((p.x, p.y, p.z), yaw, pitch);
                    }
                    false
                }
                hw_net::ToClient::Casualty { medic, yaw, .. } => {
                    radio.hear(world, m);
                    // `HW_VIEW=medic`: the camera by the ambulance.
                    if view == "medic" {
                        let c = Vec3::from_array(*medic);
                        let a = hw_sim::props::byte_to_yaw(*yaw) as f32 + 0.7;
                        let p = c + Vec3::new(a.sin(), 0.0, a.cos()) * 12.0 + Vec3::Y * 5.0;
                        let d = c - p;
                        fly = FlyCamera::new((p.x, p.y, p.z), d.x.atan2(d.z).to_degrees(), -20.0);
                    }
                    false
                }
                hw_net::ToClient::PropsBroken { props, .. } => {
                    for &(i, fell) in props {
                        if let Some(v) = extras.props.as_mut() {
                            v.set_broken(i, Some(fell));
                        }
                        // A barricade knocked down no longer blocks the way.
                        generated
                            .obstacles
                            .set_prop_broken(hw_gen::urban::quarter::DetailId(i), true);
                        if let Some(l) = extras.lights.as_mut() {
                            l.refresh();
                        }
                        if let Some(s) = extras.signals.as_mut() {
                            s.refresh();
                        }
                    }
                    false
                }
                hw_net::ToClient::CarsWrecked { cars, live } => {
                    for &(id, _, fell, along) in cars {
                        let _ = traffic_view.wreck(world, &mut traffic, id, fell, along, *live);
                    }
                    false
                }
                hw_net::ToClient::Traffic {
                    time,
                    lane,
                    first,
                    cars,
                } => {
                    traffic.sync_in(&hw_sim::traffic::TrafficChunk {
                        time: *time,
                        lane: *lane,
                        first: *first,
                        cars: cars.clone(),
                    });
                    false
                }
                hw_net::ToClient::Alarm { .. } => {
                    alarms.apply(m);
                    false
                }
                hw_net::ToClient::CarsCleared { cars } => {
                    for &id in cars {
                        traffic_view.clear_wreck(world, &mut traffic, id);
                    }
                    false
                }
                hw_net::ToClient::PropsRepaired { props } => {
                    for &i in props {
                        if let Some(v) = extras.props.as_mut() {
                            v.set_broken(i, None);
                        }
                        generated
                            .obstacles
                            .set_prop_broken(hw_gen::urban::quarter::DetailId(i), false);
                        if let Some(l) = extras.lights.as_mut() {
                            l.refresh();
                        }
                        if let Some(s) = extras.signals.as_mut() {
                            s.refresh();
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
                done.append(&mut held);
                if frame.first && std::env::var_os("HW_WAIT_PEOPLE").is_some() {
                    let all = (hw_sim::citizens::LOOKS + hw_sim::citizens::MOLE_LOOKS) as usize
                        + 1
                        + 2 * people::VILLAIN_LOOKS as usize;
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
            hero_who.retain(|id, _| hero_entities.contains_key(id));
            canopies.retain(|id, (e, _)| {
                let keep = hero_entities.contains_key(id);
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
                    || h.kind == hw_net::EntityKind::Capsule
                    || (h.kind == hw_net::EntityKind::Hero
                        && (hw_sim::taxi::riding(h, &heroes) || hw_sim::tube::riding(h, &heroes)))
                {
                    // A taxi or a tube capsule, and its passenger out of
                    // sight inside.
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
                            let mesh = if h.kind == hw_net::EntityKind::Capsule {
                                hw_client::deepview::capsule_mesh()
                            } else {
                                hw_client::propview::taxi_open_levels()
                                    .into_iter()
                                    .next()
                                    .map(|l| l.mesh)
                                    .expect("taxi model")
                            };
                            let e = world.place(mesh, place)?;
                            hero_entities.insert(h.id, (e, false));
                        }
                        None => {}
                    }
                    if !hidden && h.kind == hw_net::EntityKind::Taxi {
                        // The canopy swings up on its front hinge while the
                        // taxi is parked for a fare (the server's
                        // `grounded`), and shuts before it flies.
                        let (levels, hinge, _) = hw_client::propview::taxi_door_levels();
                        let (e, open) = match canopies.get(&h.id) {
                            Some(&c) => c,
                            None => {
                                let mesh =
                                    levels.into_iter().next().map(|l| l.mesh).expect("canopy");
                                (world.place(mesh, place)?, 0.0)
                            }
                        };
                        let step = frame.dt / hw_sim::taxi::CANOPY as f32;
                        let open = if h.grounded {
                            (open + step).min(1.0)
                        } else {
                            (open - step).max(0.0)
                        };
                        canopies.insert(h.id, (e, open));
                        let turn = glam::Quat::from_rotation_y(y);
                        let at =
                            Vec3::from_array([h.pos[0] as f32, h.pos[1] as f32, h.pos[2] as f32])
                                + turn * hinge.as_vec3();
                        let eased = open * open * (3.0 - 2.0 * open);
                        world.set_place(
                            e,
                            Place::new(at.x, at.y, at.z)
                                .with_yaw_deg(y.to_degrees())
                                .with_pitch_deg(eased * 52.0),
                        )?;
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
                // Machines are drawn from their parts (`robotview`).
                if h.kind.is_robot() {
                    continue;
                }
                let feet = Vec3::new(h.pos[0] as f32, h.pos[1] as f32, h.pos[2] as f32);
                let villain = h.kind.is_enemy();
                let bystander = h.kind == hw_net::EntityKind::Bystander;
                // The mutant alpha is a brute a head and a half taller.
                let size = if h.kind == hw_net::EntityKind::Alpha {
                    1.35
                } else {
                    1.0
                };
                // Our own flight we know; others' is told from their motion.
                let flying = (mine == Some(h.id))
                    .then(|| hero_flies || solids.in_lift(glam::DVec3::from_array(h.pos)));
                let anim = anims.entry(h.id).or_default();
                let mut d = anim.step(
                    vel,
                    h.grounded,
                    if villain || bystander {
                        Some(false)
                    } else {
                        flying
                    },
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
                // A bystander standing still faces the way the server turned
                // them (towards the muggers).
                if bystander && slow {
                    let (y, _, _) = glam::Quat::from_array(h.rot).to_euler(glam::EulerRot::YXZ);
                    anim.face(y.to_degrees(), frame.dt);
                    d.yaw = anim.yaw();
                }
                // A look per player until the hero creator, the same on
                // every machine; villains by kind, a few looks each.
                let who = match h.kind {
                    k if VillainKind::of(k).is_some() => {
                        let v = VillainKind::of(k).expect("a villain");
                        Who::Villain(v, h.id.0 % v.looks())
                    }
                    // A townsperson in a crime: one of the citizens' looks.
                    hw_net::EntityKind::Bystander => Who::Citizen(h.id.0 % hw_sim::citizens::LOOKS),
                    // A hero in their looks, once they arrived (ours we
                    // know); until then a stand-in, the same on every machine.
                    _ => {
                        let bytes = if mine == Some(h.id) {
                            my_look.as_ref().map(looks::encode)
                        } else {
                            h.owner
                                .and_then(|o| session.player_look(o))
                                .map(|b| b.to_vec())
                        };
                        match bytes.and_then(|b| Some((looks::key_of(&b), looks::decode(&b)?))) {
                            Some((key, look)) => {
                                let who = Who::Look(key);
                                if !asked.contains(&who) {
                                    if let Some(p) = &actors {
                                        p.request_look(key, look, false);
                                        asked.insert(who.clone());
                                    }
                                }
                                who
                            }
                            None if mine == Some(h.id) && my_look.is_none() => {
                                Who::Hero(name.clone())
                            }
                            None => Who::Hero(format!("player {}", h.owner.map_or(0, |o| o.0))),
                        }
                    }
                };
                let model = models.get(&who).cloned();
                if model.is_none() && !asked.contains(&who) {
                    if let Some(p) = &actors {
                        p.request(who.clone(), 0);
                        asked.insert(who.clone());
                    }
                }
                let animated = model.is_some();
                let place = Place::new(feet.x, feet.y, feet.z)
                    .with_yaw_deg(d.yaw)
                    .with_scale(size);
                // A new model (their looks arrived) means a new body.
                let same = !animated || hero_who.get(&h.id) == Some(&who);
                let (e, _) = match hero_entities.get(&h.id) {
                    Some(&(e, a)) if a == animated && same => (e, a),
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
                                if h.kind.is_ranged() {
                                    fight.arm(world, e);
                                } else if h.kind.wields() {
                                    fight.wield(world, e, h.kind);
                                }
                                e
                            }
                            None if villain => {
                                world.place(fightview::villain_placeholder(h.kind), place)?
                            }
                            None => world.place(citymesh::hero_mesh(mine == Some(h.id)), place)?,
                        };
                        hero_entities.insert(h.id, (e, animated));
                        hero_who.insert(h.id, who.clone());
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
                        .with_roll_deg(d.roll)
                        .with_scale(size),
                )?;
                // Fighting clips over walking; a stance while squaring up.
                let in_fight = h.combat.is_some_and(|c| c.target.is_some()) && slow && villain;
                let (clip, once) = match action {
                    Some(a) => (a, a != "cuffed" && a != "hover"),
                    None if in_fight && h.kind.is_ranged() && d.clip == "idle" => {
                        ("pistol idle", false)
                    }
                    None if in_fight && d.clip == "idle" => ("fight stance", false),
                    // The one being mugged cowers until it is over.
                    None if bystander && d.clip == "idle" => ("hands up", false),
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
            fight.muzzles = robots.muzzles(world);
            fight.on_fx(world, &events, now, mine, &hero_entities, &heroes);
            let muzzles = fight.muzzles.clone();
            robots.on_fx(world, &mut fight, &events, &heroes, &muzzles, mine);
            robots.update(world, &heroes, frame.dt)?;
            robots.music(world, &heroes);
            // `HW_VIEW=sentry`: a close-up of the first robo-sentry (with
            // HW_INCIDENT=colossus HW_COLOSSUS_BREAK=2, the ones it drops).
            if view == "sentry" {
                if let Some((at, yaw)) = robots.sentry_at() {
                    let ahead = Vec3::new(yaw.sin(), 0.0, yaw.cos());
                    let side = Vec3::new(ahead.z, 0.0, -ahead.x);
                    let p = at + ahead * 4.2 + side * 1.8 + Vec3::Y * 2.0;
                    let d = at + Vec3::Y * 1.2 - p;
                    let pitch = (d.y / glam::Vec2::new(d.x, d.z).length())
                        .atan()
                        .to_degrees();
                    fly = FlyCamera::new((p.x, p.y, p.z), d.x.atan2(d.z).to_degrees(), pitch);
                }
            }
            // `HW_VIEW=alpha` (with HW_INCIDENT=rampage): a close-up of the
            // mutant alpha, from the side its fight faces.
            if view == "alpha" {
                if let Some(a) = heroes.iter().find(|e| e.kind == hw_net::EntityKind::Alpha) {
                    let at = Vec3::new(a.pos[0] as f32, a.pos[1] as f32, a.pos[2] as f32);
                    let p = at + Vec3::new(3.6, 2.4, 3.6);
                    let d = at + Vec3::Y * 1.4 - p;
                    let pitch = (d.y / glam::Vec2::new(d.x, d.z).length())
                        .atan()
                        .to_degrees();
                    fly = FlyCamera::new((p.x, p.y, p.z), d.x.atan2(d.z).to_degrees(), pitch);
                }
            }
            // `HW_VIEW=villain HW_VILLAIN=<kind>` (with an incident of theirs):
            // a close-up of the first villain of that kind ("legionary",
            // "captain", "delver"...).
            if view == "villain" {
                let want = std::env::var("HW_VILLAIN").unwrap_or_default();
                if let Some(a) = heroes
                    .iter()
                    .find(|e| format!("{:?}", e.kind).eq_ignore_ascii_case(&want))
                {
                    let at = Vec3::new(a.pos[0] as f32, a.pos[1] as f32, a.pos[2] as f32);
                    let (y, _, _) = glam::Quat::from_array(a.rot).to_euler(glam::EulerRot::YXZ);
                    let ahead = Vec3::new(y.sin(), 0.0, y.cos());
                    let side = Vec3::new(ahead.z, 0.0, -ahead.x);
                    let p = at + ahead * 2.4 + side * 1.0 + Vec3::Y * 1.7;
                    let d = at + Vec3::Y * 1.15 - p;
                    let pitch = (d.y / glam::Vec2::new(d.x, d.z).length())
                        .atan()
                        .to_degrees();
                    fly = FlyCamera::new((p.x, p.y, p.z), d.x.atan2(d.z).to_degrees(), pitch);
                }
            }
            // `HW_VIEW=airship` (with HW_INCIDENT=pirates): the raiders'
            // airship from a rooftop's height down the street.
            if view == "airship" {
                if let Some((at, yaw)) = radio.ship_at() {
                    let a = yaw.to_radians() + 1.1;
                    let p = at + Vec3::new(a.sin(), 0.0, a.cos()) * 38.0 - Vec3::Y * 9.0;
                    let d = at - Vec3::Y * 3.0 - p;
                    let pitch = (d.y / glam::Vec2::new(d.x, d.z).length())
                        .atan()
                        .to_degrees();
                    fly = FlyCamera::new((p.x, p.y, p.z), d.x.atan2(d.z).to_degrees(), pitch);
                }
            }
            let me_state = mine.and_then(|id| heroes.iter().find(|e| e.id == id));
            fight.update(world, frame.dt, &heroes, me_state);
            // Our sheet as the host has it: a level up shows, the file is kept.
            if let Some(s) = session.take_sheet() {
                let before = hw_sim::progress::level_of(my_sheet.lifetime);
                let after = hw_sim::progress::level_of(s.lifetime);
                if sheet_synced && after > before {
                    powers.level_up(after);
                }
                if hero_file.as_ref() == Some(&s) {
                    sheet_synced = true;
                }
                if sheet_synced && !cheated {
                    hero_file = Some(s.clone());
                    sheet_dirty = true;
                }
                my_sheet = s;
            }
            if sheet_dirty && frame.time - sheet_saved_at > 3.0 {
                save_hero(hero_file.clone());
                sheet_dirty = false;
                sheet_saved_at = frame.time;
            }
            powers.update(frame.dt);

            // Citizens around the camera, at the game's time.
            let eye = world.camera().eye();
            crowd.update(
                world,
                &citizens,
                &alarms,
                DVec2::new(eye.x as f64, eye.z as f64),
                session.time(tick_clock / hw_sim::DT),
                frame.dt as f64,
            )?;
            // `HW_VIEW=citizen`: a close-up of the nearest passer-by.
            if view == "citizen" {
                if let Some(at) = crowd.nearest(world, eye) {
                    let yaw = at.yaw_degrees.to_radians();
                    let ahead = Vec3::new(yaw.sin(), 0.0, yaw.cos());
                    let side = Vec3::new(ahead.z, 0.0, -ahead.x);
                    let p = at.position + ahead * 2.2 + side * 0.6 + Vec3::Y * 1.6;
                    let d = at.position + Vec3::Y * 1.3 - p;
                    let pitch = (d.y / glam::Vec2::new(d.x, d.z).length())
                        .atan()
                        .to_degrees();
                    fly = FlyCamera::new((p.x, p.y, p.z), d.x.atan2(d.z).to_degrees(), pitch);
                }
            }
            // Cars on the streets, hover cars and taxis in the sky lanes.
            // Cars stop for anyone in the road (on their feet, knocked down
            // or lying hurt) and short of trouble, as the server's do.
            let mut standing: Vec<DVec3> = heroes
                .iter()
                .filter(|e| match e.kind {
                    hw_net::EntityKind::Hero | hw_net::EntityKind::Bystander => true,
                    _ => e
                        .combat
                        .is_some_and(|c| c.status != hw_net::Status::Carried),
                })
                .map(|e| DVec3::from_array(e.pos))
                .collect();
            standing.extend(radio.lying().map(|p| p.as_dvec3()));
            traffic.set_blockers(&standing, &alarms.car_stops());
            traffic.advance_to(session.time(tick_clock / hw_sim::DT));
            // Joined a friend: our own hero bumps into the cars near it as
            // the host's does (the host sets them itself when we host).
            if matches!(session, Session::Remote { .. }) {
                if let Some(me) = session.my_position(&heroes) {
                    let t = session.time(tick_clock / hw_sim::DT);
                    solids.set_movers(traffic.car_solids(&[me], 25.0, t));
                }
            }
            if let (Some(s), Some(props)) = (extras.signals.as_mut(), extras.props.as_ref()) {
                s.update(
                    world,
                    &traffic,
                    props,
                    DVec2::new(eye.x as f64, eye.z as f64),
                    session.time(tick_clock / hw_sim::DT),
                )?;
            }
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
                    .filter_map(|i| town.cell_of_building(i.building))
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
                        // Behind the taxi, well back; close behind a
                        // capsule, in the tube.
                        hero_cam.distance = if ride_close {
                            5.5
                        } else {
                            hero_cam.distance.max(10.0)
                        };
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
                    let under = solids.ground_for(q, fly.position.y as f64);
                    // Below the streets the water level does not count.
                    let floor = if under < solids.ground_at(q) - 1.0 {
                        under
                    } else {
                        under.max(hw_gen::urban::world::WATER_LEVEL)
                    };
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
                hw_client::lighting::set_sky(world, hour);
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
                fight.hud(
                    frame.ui.ctx(),
                    view_proj,
                    me_state,
                    &heroes,
                    &names,
                    Some(&my_sheet),
                );
                let fighting = me_state.and_then(|m| m.combat).map_or(1, |c| c.level);
                radio.me = mine.map(|m| m.0);
                radio.update(frame.dt);
                radio.animate(world, frame.dt);
                radio.carry(world, |id| {
                    let h = heroes.iter().find(|h| h.id.0 == id)?;
                    let yaw = anims.get(&h.id).map_or(0.0, |a| a.yaw());
                    Some((
                        Vec3::new(h.pos[0] as f32, h.pos[1] as f32, h.pos[2] as f32),
                        yaw,
                    ))
                });
                signal.update(world, &generated.world, &radio);
                let cam = world.camera();
                let hero_at = session.my_position(&heroes).map(|p| p.as_vec3());
                radio.panel(
                    frame.ui.ctx(),
                    cam.eye(),
                    cam.target() - cam.eye(),
                    fighting,
                    hero_at,
                );
                let cam = world.camera();
                let look = cam.target() - cam.eye();
                hw_client::dispatchview::compass(frame.ui.ctx(), cam.eye(), look, &radio, fighting);
                if frame.input.pressed(Key::M) {
                    town_map = !town_map;
                }
                hw_client::dispatchview::map(
                    frame.ui.ctx(),
                    &mut town_map,
                    &generated.world,
                    hero_at,
                    look,
                    &radio,
                    fighting,
                );
                // The minimap (N hides it) and the town's sounds.
                let eye = cam.eye();
                let centre = hero_at.unwrap_or(eye);
                let at2 = DVec2::new(centre.x as f64, centre.z as f64);
                let eye2 = DVec2::new(eye.x as f64, eye.z as f64);
                let height = (eye.y as f64 - generated.world.ground(eye2)).max(0.0) as f32;
                let mates: Vec<DVec2> = heroes
                    .iter()
                    .filter(|e| e.kind == hw_net::EntityKind::Hero && Some(e.id) != mine)
                    .map(|e| DVec2::new(e.pos[0], e.pos[2]))
                    .collect();
                minimap.update(
                    frame.ui.ctx(),
                    &generated.world,
                    at2,
                    look,
                    height as f64,
                    &mates,
                    &radio,
                    fighting,
                    frame.input.pressed(Key::N) && !on_ui_keys(frame),
                );
                if let Some(h) = hero_at {
                    if let Some((was, _)) = hero_was {
                        let moved = glam::Vec2::new(h.x - was.x, h.z - was.z).length();
                        let now = moved / frame.dt.max(1e-3);
                        hero_speed += (now.min(80.0) - hero_speed) * (frame.dt * 8.0).min(1.0);
                    }
                    hero_was = Some((h, frame.time));
                }
                let game_time = session.time(tick_clock / hw_sim::DT);
                let cars = traffic.near(eye2, 90.0, game_time, 40);
                let scene = hw_client::soundscape::Scene {
                    now: frame.time as f64,
                    dt: frame.dt,
                    eye,
                    hero: hero_at,
                    flying: hero_flies,
                    speed: hero_speed,
                    height,
                    indoors: town.inside.is_some(),
                    under: hw_client::soundscape::under(&generated.world, eye),
                    hour: game_hour(),
                    district: hw_client::soundscape::district(&generated.world, eye2),
                    around: minimap.surroundings(eye2, 45.0),
                    people: citizens.near(eye2, 35.0, game_time, 64).len(),
                    cars: &cars,
                    alarms: alarms
                        .active()
                        .map(|a| (a.id, a.kind, a.at.as_vec3()))
                        .collect(),
                    medics: radio
                        .calls
                        .values()
                        .filter_map(|c| c.casualty.map(|k| k.medic))
                        .collect(),
                    clock: generated
                        .world
                        .town
                        .growth
                        .centres
                        .iter()
                        .find(|c| c.kind == hw_gen::urban::growth::CentreKind::Core)
                        .map(|c| {
                            Vec3::new(
                                c.pos.x as f32,
                                generated.world.ground(c.pos) as f32,
                                c.pos.y as f32,
                            )
                        }),
                    boss_music: robots.music_on() || music_fade_at.is_some(),
                };
                sounds.update(world, &scene);
                for a in powers.ui(frame.ui.ctx(), &my_sheet, fighting, creating) {
                    match a {
                        PowerAction::Buy(p, theme) => session.buy(p, theme),
                        PowerAction::GiveUp(p) => session.give_up(p),
                        PowerAction::Create(s) => {
                            hero_file = Some(s.clone());
                            my_sheet = s.clone();
                            session.send_sheet(s);
                            save_hero(hero_file.clone());
                        }
                    }
                }
            }
            drawn = heroes;

            // Roads and buildings around the camera, full detail close by.
            let eye = world.camera().eye();
            town.update(
                world,
                DVec2::new(eye.x as f64, eye.z as f64),
                BUILD_BUDGET_MS,
            );
            // Street props, trees and hedges around the camera.
            if let Some(v) = extras.props.as_mut() {
                v.update(
                    DVec2::new(eye.x as f64, eye.z as f64),
                    BUILD_BUDGET_MS * 0.5,
                );
                v.flush(world)?;
            }
            if let Some(w) = extras.waterfront.as_mut() {
                w.update(world, DVec2::new(eye.x as f64, eye.z as f64), true);
            }
            Ok(())
        },
    );
    if let Err(e) = &run {
        hw_client::crash::engine_error(e);
    }
    run
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
        MoonSpec::new(1.7, 29.5, 0.5).tilt(14.0).brightness(0.65),
        MoonSpec::new(0.7, 9.0, 0.44).tilt(-26.0).brightness(0.45),
    ]
}

/// Engine streams the town's ground and water.
fn set_town_stream(
    world: &mut World,
    generated: &GeneratedTown,
    green: std::sync::Arc<hw_gen::urban::greenery::Greenery>,
    trees: std::collections::BTreeMap<hw_gen::urban::props::PropKind, LodId>,
) -> EngineResult<()> {
    // A wide, coarse water window, so rivers and lakes show across the town
    // and not only around the camera.
    let settings = StreamSettings {
        water_window: 1536.0,
        water_cell: 4.0,
        // Deep enough for the Deeps, the lowest caves under the town.
        voxel_below: hw_gen::urban::deep::VOXEL_DEPTH as f32,
        ..StreamSettings::default()
    };
    let ground = TownGround {
        town: generated.world.clone(),
        green,
        trees,
    };
    let stream = WorldStream::new(ground, settings).with_terrain(|t| t.with_beach(1.8));
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
    signals: Option<hw_client::signalview::SignalView>,
    props: Option<hw_client::propview::PropView>,
    under: Option<hw_client::undergroundview::UndergroundView>,
    deep: Option<hw_client::deepview::DeepView>,
    dystopia: Option<hw_client::dystopiaview::DystopiaView>,
    grass: Option<hw_client::grassview::GrassView>,
    waterfront: Option<hw_client::waterfrontview::WaterfrontView>,
}

impl TownExtras {
    fn despawn(&mut self, world: &mut World) {
        // The streamed woods copy tree sets below: out with them first.
        let _ = world.set_stream(None);
        for id in self.lods.drain(..) {
            world.despawn_lod(id);
        }
        for e in self.entities.drain(..) {
            world.despawn(e);
        }
        if let Some(mut l) = self.lights.take() {
            l.clear(world);
        }
        if let Some(mut w) = self.waterfront.take() {
            w.clear(world);
        }
        self.props = None;
        if let Some(mut s) = self.signals.take() {
            s.clear(world);
        }
        if let Some(mut u) = self.under.take() {
            u.clear(world);
        }
        if let Some(mut d) = self.deep.take() {
            d.clear(world);
        }
        if let Some(mut d) = self.dystopia.take() {
            d.clear(world);
        }
        if let Some(g) = self.grass.take() {
            g.despawn(world);
        }
    }
}

/// The town's props, landmarks and waterfronts.
fn spawn_extras(
    world: &mut World,
    generated: &GeneratedTown,
    green: std::sync::Arc<hw_gen::urban::greenery::Greenery>,
) -> EngineResult<TownExtras> {
    let t = std::time::Instant::now();
    let props = hw_client::propview::PropView::spawn(world, generated.world.clone())?;
    eprintln!(
        "{} prop models in {:.2} s (the props stream in around the camera)",
        props.sets.len(),
        t.elapsed().as_secs_f64()
    );
    let landmarks = hw_client::landmarkview::LandmarkView::spawn(world, &generated.world)?;
    let under = hw_client::undergroundview::UndergroundView::spawn(world, &generated.world)?;
    let grass =
        hw_client::grassview::GrassView::spawn(world, generated.world.clone(), green.clone())?;
    // The country's woods stream in around the camera, drawn with the
    // props' tree sets.
    let mut lods: Vec<LodId> = props.sets.iter().copied().chain(landmarks.sets).collect();
    let trees: std::collections::BTreeMap<_, _> = hw_gen::urban::props::PropKind::all()
        .into_iter()
        .filter(|k| matches!(k, hw_gen::urban::props::PropKind::Tree(..)))
        .filter_map(|k| Some((k, props.set_for(k)?)))
        .collect();
    set_town_stream(world, generated, green, trees)?;
    let (roost_lods, roost_entities) =
        hw_client::factionview::spawn_roost(world, &generated.world)?;
    lods.extend(roost_lods);
    let mut deep = hw_client::deepview::DeepView::new(generated.world.clone());
    deep.prime(world, world.camera().eye())?;
    Ok(TownExtras {
        grass: Some(grass),
        lods,
        entities: roost_entities,
        waterfront: Some(hw_client::waterfrontview::WaterfrontView::new(
            generated.world.clone(),
        )),
        lights: Some(hw_client::nightlights::NightLights::new(
            generated.world.clone(),
        )),
        signals: Some(hw_client::signalview::SignalView::new()),
        props: Some(props),
        under: Some(under),
        deep: Some(deep),
        dystopia: Some(hw_client::dystopiaview::DystopiaView::new(&generated.world)),
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

/// "Aether Tube: where to?": the stations the lines reach from this one.
/// Returns the one picked.
fn tube_panel(
    ui: &engine::ui::UiFrame,
    menu: &mut Option<(usize, Vec<usize>)>,
    net: &hw_gen::urban::deep::TubeNet,
) -> Option<usize> {
    use engine::ui::egui;
    let (from, list) = menu.as_ref()?;
    let mut picked = None;
    let mut close = false;
    egui::Area::new(egui::Id::new("tube"))
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_unmultiplied(24, 32, 30, 235))
                .stroke(egui::Stroke::new(
                    2.0_f32,
                    egui::Color32::from_rgb(200, 60, 50),
                ))
                .corner_radius(8.0)
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.set_min_width(300.0);
                    ui.label(
                        egui::RichText::new("AETHER TUBE")
                            .color(egui::Color32::from_rgb(232, 196, 96))
                            .strong()
                            .size(20.0),
                    );
                    ui.label(
                        egui::RichText::new(format!("{}: where to?", net.stations[*from].name))
                            .color(egui::Color32::from_rgb(236, 228, 206))
                            .size(14.0),
                    );
                    ui.add_space(6.0);
                    if list.is_empty() {
                        ui.label("No line runs on from here yet.");
                    }
                    egui::ScrollArea::vertical()
                        .max_height(360.0)
                        .show(ui, |ui| {
                            for &i in list {
                                let s = &net.stations[i];
                                let d = s.pos.distance(net.stations[*from].pos);
                                let text = format!("{}  ·  {:.1} km", s.name, d / 1000.0);
                                let b = egui::Button::new(egui::RichText::new(text).size(15.0))
                                    .min_size(egui::vec2(280.0, 26.0));
                                if ui.add(b).clicked() {
                                    picked = Some(i);
                                }
                            }
                        });
                    ui.add_space(4.0);
                    if ui.button("Never mind (G)").clicked() {
                        close = true;
                    }
                });
        });
    if picked.is_some() || close {
        *menu = None;
    }
    picked
}

/// A line at the top in a tube station, and while a capsule comes for you
/// or takes you somewhere.
fn tube_status(
    ui: &engine::ui::UiFrame,
    capsule: bool,
    riding: bool,
    to: Option<&str>,
    in_station: bool,
) {
    use engine::ui::egui;
    let text = match (capsule, riding, to) {
        (true, true, Some(to)) => format!("Aether Tube to {to}  ·  G to get out"),
        (true, true, None) => "In a capsule  ·  G to get out".into(),
        (true, false, _) => "Your capsule is coming: wait here  ·  G sends it away".into(),
        (false, _, _) if in_station => "Aether Tube station  ·  G: where to?".into(),
        _ => return,
    };
    egui::Area::new(egui::Id::new("tube status"))
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 44.0))
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_black_alpha(160))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(10, 5))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(text)
                            .color(egui::Color32::from_rgb(232, 196, 96))
                            .size(15.0),
                    );
                });
        });
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
fn version_corner(
    ui: &engine::ui::UiFrame,
    open: &mut bool,
    room: Option<&str>,
    address: Option<&str>,
) {
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
                            // What friends type to join this world (or its
                            // address, when the router forwards our port).
                            let text = match address {
                                Some(a) => format!("Room {code} · {a}"),
                                None => format!("Room {code}"),
                            };
                            ui.label(
                                egui::RichText::new(text)
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
