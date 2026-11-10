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
//! creator, `=city` the city window, `HW_CREATOR_SECTION=face|costume|colours` and
//! `HW_CREATOR_DETAILED=1` pick its tab). Your name, looks, world and sound
//! levels are remembered between runs (see `profile.rs`).
//!
//! Environment: `HW_SEED` (world seed, default 1, or the one you last played); `HW_POP` (people in the
//! town: 300 a hamlet, 40000 the default, millions a metropolis; by default
//! the city picked in the title's city window; joining players get the host's); `HW_NAME`; `HW_VIEW` =
//! `hero` (default), `inside`, `sky`, `overview`, `street`, `river`, `cutaway`, `park`, `pad`, `spire`, `plinth` (the spire plaza's edge and steps close up), `lighthouse`, `quay`, `wrecked`, `traffic`, `signals`, `market`, `skylanes`, `lift`, `tunnel`, `molehaven`, `tube` (a tube station's street pavilion), `station`, `tubeline`, `capsule`, `ruin`, `lab`, `kiosk`, `grotto`, `rocket`, `rocketpad`, `rocketarm`, `rocketboard` (the rocket port; `HW_ROCKET_T=<s>` freezes its first launch that many seconds in: 5 venting, 25 counting, 31 lit, 40 climbing, 290 landing), `call` (the light over the first call, with `HW_INCIDENT`), `far` (a main road across town, where people and traffic must follow) or
//! town: 300 a hamlet, 40000 the default, millions a metropolis; joining
//! players get the host's); `HW_NAME`; `HW_VIEW` =
//! `hero` (default), `inside`, `sky`, `overview`, `street`, `river`, `cutaway`, `park`, `pad`, `spire`, `plinth` (the spire plaza's edge and steps close up), `lighthouse`, `weather` (the weather tower and its shower; `weathertop`, `weatherfoot`, `weatherstreet` close up; `HW_WEATHER=brew|rain|clear|bow|fair` picks the moment), `quay`, `wrecked`, `traffic`, `signals`, `market`, `skylanes`, `lift`, `tunnel`, `molehaven`, `tube` (a tube station's street pavilion), `station`, `tubeline`, `capsule`, `ruin`, `lab`, `kiosk`, `grotto`, `far` (a main road across town, where people and traffic must follow) or
//! `props` picks the starting camera (handy with `ENGINE_SCREENSHOT`; `props` looks at the
//! nearest prop whose kind contains `HW_PROP`, a car by default); `HW_CUT_LEVEL` the floor shown
//! cut open; `HW_WALK=1` makes the hero run forward on its own (for tests);
//! `HW_LEVEL=n` plays a level-n hero with karma to spend, `HW_POWERS=1`
//! with every power open bought (`HW_THEME=1` the second themes);
//! `HW_VIEW=powers` opens the powers window, `HW_VIEW=origin` shows a new
//! hero's origin card.
//! `HW_VIEW=fight` starts in front of the minions, `HW_AUTOFIGHT=1` (or a list of power codes)
//! fights them on its own (both for screenshots);
//! `HW_WRECK=<frames>` wrecks the cars near the camera that many frames in;
//! `HW_VIEW=riders` looks in at the driver of the nearest car
//! (`HW_RIDER=cruiser|bubble|hover|taxi` picks the kind, `HW_RIDER_CAM=front`
//! looks in through the windscreen);
//! `HW_TAXI=<stand>` hails a taxi at the start and `HW_TAXI_SECS=<s>` runs
//! the world on that far (screenshots of a ride);
//! `HW_WAIT_PEOPLE=1` waits at the start until every character is built
//! (for screenshots); `HW_PACKS` points at the Actors packs (default
//! `assets/packs`, fetched by `scripts/fetch-packs`);
//! Ctrl+Shift+F12 opens the debug menu (`debugmenu.rs`; `HW_DEBUG=<tab>`
//! opens it at start);
//! `ENGINE_ZFIGHT=1` marks z-fighting with magenta posts (the `zfight` tool
//! checks a whole city without the game).
//! Stutters are written to `logs/hitches.log` (see `hitch.rs`; `HW_HITCH_LOG`,
//! `HW_HITCH_MS`); `HW_AT=x,y,z` with `HW_YAW` starts the hero at a spot a
//! report names; `HW_CAM_DIST` and `HW_CAM_PITCH` set the hero camera.

mod session;

use engine::prelude::*;
use glam::{DVec2, DVec3, Vec3};
use hw_client::citybuild::{City, CityBuilds};
use hw_client::citymesh;
use hw_client::crowdview::CrowdView;
use hw_client::damageview::DamageView;
use hw_client::extras::{ExtrasLoad, TownExtras};
use hw_client::fightview::{self, FightView};
use hw_client::heroview::{tipped_origin, HeroAnim};
use hw_client::looks;
use hw_client::people::{self, People, VillainKind, Who};
use hw_client::powersview::{PowerAction, PowersView};
use hw_client::profile::Profile;
use hw_client::robotview::RobotView;
use hw_client::title::{self, Choice, Title};
use hw_client::townmesh::TownView;
use hw_core::Seed;
use hw_sim::collision::{Obstacles, BODY_HEIGHT};
use hw_sim::GeneratedTown;
use session::Session;
use std::collections::{BTreeMap, BTreeSet};

/// Milliseconds per frame for putting town cells, drawn on worker threads,
/// into the world as you move.
const BUILD_BUDGET_MS: f64 = 6.0;
/// Milliseconds per frame for putting cells drawn on worker threads into
/// the world behind the title (and for its props).
const TITLE_BUILD_MS: f64 = 5.0;
/// Milliseconds per frame for putting the town's extras into the world.
const EXTRAS_MS: f64 = 4.0;
/// The hour the game is played at: mid-afternoon, a sun low enough to
/// model the facades and throw long shadows (the title is at golden hour).
const GAME_HOUR: f32 = hw_sim::clock::START_HOUR as f32;

/// `HW_HOUR` (0 to 24, e.g. 22 for night): the hour stands still there
/// (for screenshots).
fn fixed_hour() -> Option<f32> {
    static HOUR: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
    *HOUR.get_or_init(|| {
        std::env::var("HW_HOUR")
            .ok()
            .and_then(|h| h.parse::<f32>().ok())
            .filter(|h| (0.0..=24.0).contains(h))
    })
}

/// Hours the debug menu moved the clock by (f32 bits).
static HOUR_SHIFT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// The hour last shown (f32 bits; `u32::MAX`: none yet).
static HOUR_NOW: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(u32::MAX);

fn hour_shift() -> f32 {
    f32::from_bits(HOUR_SHIFT.load(std::sync::atomic::Ordering::Relaxed))
}

/// The hour of day now: the last one [`hour_now`] worked out, or the
/// start (`HW_HOUR` or [`GAME_HOUR`]) before the first frame.
fn game_hour() -> f32 {
    match HOUR_NOW.load(std::sync::atomic::Ordering::Relaxed) {
        u32::MAX => (fixed_hour().unwrap_or(GAME_HOUR) + hour_shift()).rem_euclid(24.0),
        bits => f32::from_bits(bits),
    }
}

/// The hour after `seconds` of game clock: the day runs (see
/// `hw_sim::clock`) unless `HW_HOUR` holds it; the debug menu shifts it.
fn hour_now(seconds: f64) -> f32 {
    let base = fixed_hour().unwrap_or_else(|| hw_sim::clock::hour_at(seconds) as f32);
    let h = (base + hour_shift()).rem_euclid(24.0);
    HOUR_NOW.store(h.to_bits(), std::sync::atomic::Ordering::Relaxed);
    h
}

/// Moves the clock so that it reads `h` now, and runs on from there.
fn set_game_hour(h: f32) {
    use std::sync::atomic::Ordering;
    let shift = hour_shift() + h - game_hour();
    HOUR_SHIFT.store(shift.rem_euclid(24.0).to_bits(), Ordering::Relaxed);
    HOUR_NOW.store(h.to_bits(), Ordering::Relaxed);
}
/// Seconds the camera takes to swoop from the title down to the hero.
const SWOOP_SECONDS: f32 = 2.4;
/// Seconds of the power pose our hero strikes on a level up.
const POWER_POSE_SECONDS: f32 = 2.6;
/// Cells within this distance of the cut focus are redrawn when it changes.
const CUT_REACH: f64 = hw_client::townmesh::CUT_RADIUS + 120.0;

/// The camera that follows the hero: orbit angles and distance.
struct HeroCamera {
    yaw: f32,
    pitch: f32,
    distance: f32,
    /// Degrees per pixel of mouse, and -1 for inverted up and down.
    look: (f32, f32),
    /// The boom as drawn, kept out of walls (see `camclip`).
    boom: hw_client::camclip::Boom,
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
        use hw_client::camclip;
        let head = target + Vec3::Y * (BODY_HEIGHT as f32 - 0.15);
        let dir = Camera::direction(self.yaw, self.pitch);
        // Short of walls between the camera and the hero, near plane included.
        let clear = camclip::clear_length(solids, head, dir, self.distance, open_above);
        let length = self.boom.follow(clear, frame.dt);
        let eye = camclip::eye(head, dir, length);
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

/// Keeps the power tray in the profile on this PC.
fn save_tray(tray: &hw_client::tray::Tray) {
    let mut p = Profile::load();
    p.tray = tray.clone();
    p.save();
}

fn main() -> EngineResult<()> {
    // A crash leaves a report beside the profile (see `crash.rs`).
    hw_client::crash::install();
    let env_num = |name: &str| std::env::var(name).ok().and_then(|s| s.parse::<u64>().ok());
    let view = std::env::var("HW_VIEW").unwrap_or_else(|_| "hero".into());
    let profile = Profile::load();
    hw_client::diag::load_env_file();
    let mut quality_log = hw_client::diag::QualityLog::open();
    let title_mode = std::env::var("HW_TITLE").map_or(true, |v| v != "0")
        && (view == "title"
            || (std::env::var_os("HW_VIEW").is_none()
                && std::env::var_os("HW_JOIN").is_none()
                && std::env::var_os("HW_AT").is_none()
                && std::env::var_os("HW_BENCH").is_none()
                && std::env::var_os("HW_HOST").is_none()));
    let mut name = std::env::var("HW_NAME").unwrap_or_else(|_| profile.name.clone());
    let auto_walk = std::env::var_os("HW_WALK").is_some();
    // R: the hero runs on by itself until S or R.
    let mut autorun = false;
    let auto_fight = std::env::var_os("HW_AUTOFIGHT").is_some();
    // `HW_AUTOFIGHT=17,20` uses those powers in turn (codes, see `Power`;
    // `1` alone keeps the usual round of every power).
    let auto_turn: Vec<u8> = std::env::var("HW_AUTOFIGHT")
        .unwrap_or_default()
        .split(',')
        .filter_map(|c| c.trim().parse().ok())
        .filter(|&c| c > 1)
        .collect();
    let io = |e: std::io::Error| EngineError::InvalidValue(e.to_string());

    // A joining player learns the world seed from the host first.
    let t0 = std::time::Instant::now();
    // On the title there is no session yet: the player starts one there.
    let people = env_num("HW_POP")
        .map(|p| p as f64)
        .unwrap_or(profile.people);
    // Cities planned behind the title (see `citybuild.rs`).
    let mut builds = CityBuilds::default();
    // A big city takes a while: the title opens at once on a hamlet behind
    // its cover while the real one is planned on a worker.
    let mut stand_in = false;
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
                if title_mode && people > hw_sim::DEFAULT_POPULATION {
                    let small = hw_client::citybuild::SIZES[0].people;
                    let generated = GeneratedTown::generate(seed, small);
                    let live = City {
                        seed: seed.0,
                        people: small,
                    };
                    builds.want(
                        City {
                            seed: seed.0,
                            people,
                        },
                        live,
                    );
                    stand_in = true;
                    (None, generated, seed.0, small)
                } else {
                    let generated = GeneratedTown::generate(seed, people);
                    let open = std::env::var_os("HW_HOST").is_some();
                    let session = if title_mode {
                        None
                    } else {
                        Some(Session::single(seed, &generated, &name, open).map_err(io)?)
                    };
                    (session, generated, seed.0, people)
                }
            }
        };
    let mut solids = generated.obstacles.clone();
    let mut traffic = hw_sim::traffic::Traffic::new(generated.area.clone());
    // The airships a joined player's hero bumps into (the host sets its own).
    let airships = hw_sim::airships::Airships::new(&generated.world);
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
    let mut riders = hw_client::riderview::RiderView::new();
    // Hero taxis this frame, for their cabbies.
    let mut cabs: Vec<hw_sim::traffic::Vehicle> = Vec::new();
    // The square with people and traffic follows the heroes: when the
    // server moves it, the next one is made on a worker thread, and the
    // traffic news for it waits until it is in.
    let mut area_job: Option<std::sync::mpsc::Receiver<NextArea>> = None;
    let mut area_held: Vec<hw_net::ToClient> = Vec::new();
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
    let mut story = hw_client::storyview::StoryView::default();
    let mut town_map = view == "map";
    // What was drawn last frame (for targeting), and a power key pressed
    // but not yet sent with a tick.
    let mut drawn: Vec<hw_net::EntityState> = Vec::new();
    let mut pending_power = 0u8;
    // Karma, levels and powers: the hero file from this PC (None until a new
    // hero picks an origin), the sheet as the host last confirmed it, and
    // the powers window.
    let mut powers = PowersView::new();
    // Until when our hero strikes the level-up power pose.
    let mut power_pose_until = f32::NEG_INFINITY;
    powers.tray = profile.tray.clone();
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
    // HW_STORY=n starts with n chapters of the Doctor's story done
    // (HW_BRIEFED=1: briefed on the next); not saved either.
    let cheat_story =
        env_num("HW_STORY").map(|n| n.min(hw_sim::story::CHAPTERS.len() as u64) as u8);
    if let Some(n) = cheat_story {
        let mut s = hero_file.clone().unwrap_or_else(hw_sim::progress::starter);
        s.story = n;
        s.briefed = std::env::var_os("HW_BRIEFED").is_some();
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
    let mut sweepers = hw_client::sweeperview::SweeperView::new();
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
    // `HW_BENCH=street|flight`: a measured run along the main roads (see `bench.rs`).
    let mut bench = hw_client::bench::Bench::from_env(&generated.world, spawn);
    // Occlusion culling over the measured part: frames, meshes tested, hidden.
    let mut bench_occlusion = (0u64, 0u64, 0u64);
    let mut hero_cam_yaw = 30.0f32;
    let mut fly = match view.as_str() {
        "street" | "citizen" | "alpha" | "riders" => {
            let p = spawn - spawn_dir * 8.0;
            let yaw = spawn_dir.x.atan2(spawn_dir.y).to_degrees() as f32;
            FlyCamera::new((p.x as f32, ground(p) as f32 + 6.0, p.y as f32), yaw, 8.0)
        }
        "prop" => {
            // `HW_VIEW=prop HW_PROP=<kind>`: a close-up from in front of the
            // nearest prop of that kind (`HW_PROP_DIST` metres off).
            let name = std::env::var("HW_PROP").unwrap_or_else(|_| "Car".into());
            let dist = env_num("HW_PROP_DIST").unwrap_or(5) as f64;
            let target = generated
                .area
                .props
                .iter()
                .filter(|p| format!("{:?}", p.kind).contains(&name))
                .min_by(|a, b| a.pos.distance(spawn).total_cmp(&b.pos.distance(spawn)));
            let (c, front) = target.map_or((spawn, spawn_dir), |t| (t.pos, t.dir));
            let p = c + (front + front.perp() * 0.45).normalize() * dist;
            let d = c - p;
            let yaw = d.x.atan2(d.y).to_degrees() as f32;
            FlyCamera::new((p.x as f32, ground(p) as f32 + 1.9, p.y as f32), yaw, -12.0)
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
        "statue" => {
            // The hero's statue in front of Hero Hall (`HW_STORY=7`), from
            // a few metres off, a little to one side, looking up at it.
            use hw_gen::urban::landmarks::LandmarkKind;
            let l =
                hw_client::landmarkview::nearest(&generated.world, LandmarkKind::HeroHall, spawn);
            match l.and_then(|l| l.statue_top()) {
                Some((top, face)) => {
                    let c = DVec2::new(top.x, top.z);
                    let p = c + face * 5.5 + face.perp() * 2.0;
                    let d = c - p;
                    let yaw = d.x.atan2(d.y).to_degrees() as f32;
                    let eye = top.y - 0.2;
                    let pitch = ((top.y + 1.1 - eye) / d.length()).atan().to_degrees() as f32;
                    FlyCamera::new((p.x as f32, eye as f32, p.y as f32), yaw, pitch)
                }
                None => FlyCamera::new((spawn.x as f32, 40.0, spawn.y as f32), 0.0, -20.0),
            }
        }
        "townheroes" => {
            // Hero Hall's own heroes on its plaza, the hall behind them.
            use hw_gen::urban::landmarks::LandmarkKind;
            let l =
                hw_client::landmarkview::nearest(&generated.world, LandmarkKind::HeroHall, spawn);
            let (c, base, face) = l.map_or((spawn, ground(spawn), spawn_dir), |l| {
                let spots = l.hero_spots();
                (spots.get(2).map_or(l.pos, |s| s.0), l.base, l.dir)
            });
            let p = c + face * 7.5 + face.perp() * 1.5;
            let d = c - p;
            let yaw = d.x.atan2(d.y).to_degrees() as f32;
            let eye = base + 1.9;
            FlyCamera::new((p.x as f32, eye as f32, p.y as f32), yaw, 4.0)
        }
        "spire" | "lighthouse" | "tesla" | "hall" | "rocket" => {
            // Looking up at a landmark from across its plaza.
            use hw_gen::urban::landmarks::LandmarkKind;
            let kind = match view.as_str() {
                "spire" => LandmarkKind::AetherSpire,
                "tesla" => LandmarkKind::TeslaTower,
                "hall" => LandmarkKind::HeroHall,
                "rocket" => LandmarkKind::RocketGantry,
                _ => LandmarkKind::Lighthouse,
            };
            let l = hw_client::landmarkview::nearest(&generated.world, kind, spawn);
            let (c, base, h, face) = l.map_or((spawn, ground(spawn), 30.0, spawn_dir), |l| {
                // The lighthouse faces the land: look at it from the sea.
                let face = if view == "lighthouse" { -l.dir } else { l.dir };
                // Hero Hall's star stands 8 m over its roof.
                let h = if view == "hall" {
                    l.height + 9.0
                } else {
                    l.height
                };
                (l.pos, l.base, h, face)
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
        "rocketpad" | "rocketarm" | "rocketboard" => {
            // Close up at the rocket port: from the pad at eye height, level
            // with the crew arm, or at the countdown board.
            use hw_gen::urban::gantry::{ARM_HEIGHTS, BOARD_Y, TOWER_X};
            use hw_gen::urban::landmarks::LandmarkKind;
            let l = hw_client::landmarkview::nearest(
                &generated.world,
                LandmarkKind::RocketGantry,
                spawn,
            );
            let (p, eye, look_at, look_y) = match l {
                Some(l) => {
                    let (from, up, at, at_y) = match view.as_str() {
                        "rocketpad" => (DVec2::new(-9.0, 16.0), 1.8, DVec2::ZERO, 9.0),
                        "rocketarm" => (
                            DVec2::new(12.0, 13.0),
                            ARM_HEIGHTS[1] + 2.0,
                            DVec2::new(3.0, 0.0),
                            ARM_HEIGHTS[1],
                        ),
                        _ => (
                            DVec2::new(TOWER_X - 6.0, 18.0),
                            BOARD_Y - 4.0,
                            DVec2::new(TOWER_X, 0.0),
                            BOARD_Y,
                        ),
                    };
                    (l.to_plan(from), l.base + up, l.to_plan(at), l.base + at_y)
                }
                None => (spawn, ground(spawn) + 2.0, spawn + spawn_dir, ground(spawn)),
            };
            let d = look_at - p;
            let yaw = d.x.atan2(d.y).to_degrees() as f32;
            let pitch = ((look_y - eye) / d.length()).atan().to_degrees() as f32;
            FlyCamera::new((p.x as f32, eye as f32, p.y as f32), yaw, pitch)
        }
        "works" | "worksline" | "worksdock" | "workshall" => {
            // The robot works: the yard, the welders, the tank, the hall.
            match hw_client::worksview::view(&generated.world, &view, spawn) {
                Some((e, yaw, pitch)) => FlyCamera::new((e.x, e.y, e.z), yaw, pitch),
                None => FlyCamera::new((spawn.x as f32, 40.0, spawn.y as f32), 0.0, -20.0),
            }
        }
        "tesladome" | "teslayard" => {
            // Close up at the Tesla tower: level with its dome, or up at it
            // from the yard by the lab.
            use hw_gen::urban::landmarks::LandmarkKind;
            let l =
                hw_client::landmarkview::nearest(&generated.world, LandmarkKind::TeslaTower, spawn);
            let (c, base, h, face) = l.map_or((spawn, ground(spawn), 60.0, spawn_dir), |l| {
                (l.pos, l.base, l.height, l.dir)
            });
            let d = hw_client::landmarkview::tesla_dims(h);
            let (p, eye, look) = if view == "tesladome" {
                let p = c + (face + face.perp() * 0.4).normalize() * (d.dr * 2.2 + 6.0);
                (p, base + d.rim_y + 1.5, base + d.rim_y + d.cap_h * 0.3)
            } else {
                let p = c + (face + face.perp() * 0.6).normalize() * 22.0;
                (p, ground(p).max(base) + 1.8, base + d.rim_y * 0.6)
            };
            let dd = c - p;
            let yaw = dd.x.atan2(dd.y).to_degrees() as f32;
            let pitch = ((look - eye) / dd.length()).atan().to_degrees() as f32;
            FlyCamera::new((p.x as f32, eye as f32, p.y as f32), yaw, pitch)
        }
        "mast" | "mastapron" | "terminal" | "liner" => {
            // The airship mooring mast: across the airfield with the liner
            // moored (`mast`), up from the apron at the gangway and the
            // cargo hatch (`mastapron`), or alongside the liner on its round
            // (`liner`). HW_MAST_T sets how far into its cycle it is.
            use hw_gen::urban::landmarks::LandmarkKind;
            use hw_gen::urban::mast::{MastRoute, MOOR_Y, SHIP_LEN};
            let l = hw_client::landmarkview::nearest(
                &generated.world,
                LandmarkKind::MooringMast,
                spawn,
            );
            match l {
                Some(l) => {
                    let (eye, look) = match view.as_str() {
                        "mast" => {
                            let side = l.dir.perp();
                            let p = l.pos + side * 120.0 - l.dir * 35.0;
                            let c = l.pos + l.dir * 35.0;
                            (
                                DVec3::new(p.x, ground(p).max(l.base) + 14.0, p.y),
                                DVec3::new(c.x, l.base + MOOR_Y * 0.55, c.y),
                            )
                        }
                        "mastapron" => {
                            let p = l.to_plan(DVec2::new(14.0, 4.0));
                            let c = l.to_plan(DVec2::new(0.0, 9.0));
                            (
                                DVec3::new(p.x, l.base + 1.7, p.y),
                                DVec3::new(c.x, l.base + MOOR_Y - 6.0, c.y),
                            )
                        }
                        "terminal" => {
                            let p = l.to_plan(DVec2::new(13.0, 3.0));
                            let c = l.to_plan(DVec2::new(-2.0, -12.0));
                            (
                                DVec3::new(p.x, l.base + 1.7, p.y),
                                DVec3::new(c.x, l.base + 4.0, c.y),
                            )
                        }
                        _ => {
                            let r = MastRoute::new(&generated.world, l);
                            let pose = r.pose(r.when(0.0, hw_client::mastview::view_t()));
                            let f = pose.forward();
                            let mid = pose.nose - f * (SHIP_LEN * 0.45);
                            let side = DVec3::new(f.z, 0.0, -f.x).normalize();
                            (mid + side * 75.0 + f * 40.0 + DVec3::Y * 8.0, mid)
                        }
                    };
                    let d = look - eye;
                    let yaw = d.x.atan2(d.z).to_degrees() as f32;
                    let pitch = (d.y / DVec2::new(d.x, d.z).length()).atan().to_degrees() as f32;
                    FlyCamera::new((eye.x as f32, eye.y as f32, eye.z as f32), yaw, pitch)
                }
                None => FlyCamera::new((spawn.x as f32, 40.0, spawn.y as f32), 0.0, -20.0),
            }
        }
        "atomplant" | "atomgate" | "atomtower" => {
            // The atomic plant: the whole of it from the road, the gate and
            // its atom close up, or up at a cooling tower from its foot.
            use hw_gen::urban::atomic::{towers, PYLON_AT, PYLON_H, TOWER_FOOT, TOWER_H};
            use hw_gen::urban::landmarks::LandmarkKind;
            let l = hw_client::landmarkview::nearest(
                &generated.world,
                LandmarkKind::AtomicPlant,
                spawn,
            );
            let (p, eye, look, at) = match l {
                Some(l) => {
                    let base = l.base;
                    match view.as_str() {
                        "atomplant" => {
                            let p = l.to_plan(DVec2::new(-30.0, 105.0));
                            (p, ground(p).max(base) + 14.0, base + 14.0, l.pos)
                        }
                        "atomgate" => {
                            let a = DVec2::new(PYLON_AT.0, PYLON_AT.1);
                            let p = l.to_plan(a + DVec2::new(4.5, 6.5));
                            (p, base + 4.0, base + PYLON_H + 1.5, l.to_plan(a))
                        }
                        _ => {
                            let t = towers()[0];
                            let p = l.to_plan(t + DVec2::new(-(TOWER_FOOT + 4.0), 5.0));
                            (p, base + 1.8, base + TOWER_H * 0.6, l.to_plan(t))
                        }
                    }
                }
                None => (
                    spawn,
                    ground(spawn) + 2.0,
                    ground(spawn) + 2.0,
                    spawn + spawn_dir,
                ),
            };
            let dd = at - p;
            let yaw = dd.x.atan2(dd.y).to_degrees() as f32;
            let pitch = ((look - eye) / dd.length()).atan().to_degrees() as f32;
            FlyCamera::new((p.x as f32, eye as f32, p.y as f32), yaw, pitch)
        }
        "drill" | "drillbore" => {
            // The mole folk's drill rig: from the plinth's front edge up at
            // the derrick, or close at the bore and the drill.
            use hw_gen::urban::landmarks::LandmarkKind;
            let l =
                hw_client::landmarkview::nearest(&generated.world, LandmarkKind::MoleDrill, spawn);
            let (c, base, face, plaza) = l.map_or((spawn, ground(spawn), spawn_dir, 14.0), |l| {
                (l.pos, l.base, l.dir, l.plaza)
            });
            let (p, eye, look) = if view == "drill" {
                let p = c + (face + face.perp() * 0.35).normalize() * (plaza + 16.0);
                (p, ground(p).max(base) + 3.0, base + 11.0)
            } else {
                let p = c + (face - face.perp() * 0.5).normalize() * 12.0;
                (p, base + 4.5, base + 3.5)
            };
            let dd = c - p;
            let yaw = dd.x.atan2(dd.y).to_degrees() as f32;
            let pitch = ((look - eye) / dd.length()).atan().to_degrees() as f32;
            FlyCamera::new((p.x as f32, eye as f32, p.y as f32), yaw, pitch)
        }
        "weather" | "weathertop" | "weatherfoot" | "weatherstreet" => {
            // The weather tower: from afar with its shower and the blocks
            // round it, close up at its crown, up at it from the doors, and
            // out in its rain on the street. HW_WEATHER picks the moment.
            use hw_gen::urban::landmarks::LandmarkKind;
            use hw_gen::urban::weather_tower::{rain_centre, WxDims};
            let l = hw_client::landmarkview::nearest(
                &generated.world,
                LandmarkKind::WeatherTower,
                spawn,
            );
            let (c, base, h, face) = l.map_or((spawn, ground(spawn), 50.0, spawn_dir), |l| {
                (l.pos, l.base, l.height, l.dir)
            });
            let d = WxDims::of(h);
            let rain = l.map_or(c + face * 40.0, rain_centre);
            let side = face.perp();
            let (p, eye, target) = match view.as_str() {
                "weather" => {
                    let mid = (c + rain) * 0.5;
                    let p = mid + (side * 0.9 + face * 0.35).normalize() * 190.0;
                    (p, base + 28.0, (mid, base + h * 0.7))
                }
                "weathertop" => {
                    let p = c + (face + side * 0.5).normalize() * 15.0;
                    (p, base + d.crown_y + 2.0, (c, base + d.crown_y + 1.5))
                }
                "weatherfoot" => {
                    let p = c + (face * 1.0 + side * 0.25).normalize() * 24.0;
                    (p, ground(p).max(base) + 1.7, (c, base + h * 0.3))
                }
                _ => {
                    let p = rain + side * 18.0;
                    (p, ground(p) + 1.7, (c, base + h * 0.55))
                }
            };
            let dd = target.0 - p;
            let yaw = dd.x.atan2(dd.y).to_degrees() as f32;
            let pitch = ((target.1 - eye) / dd.length()).atan().to_degrees() as f32;
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
        "menace" => {
            // The mad scientist's quarter from 500 m out (HW_MENACE_D), over
            // the rooftops on the town's side, looking in.
            let w = &generated.area;
            match &w.town.dystopia {
                Some(d) => {
                    let out = std::env::var("HW_MENACE_D")
                        .ok()
                        .and_then(|v| v.parse::<f64>().ok())
                        .unwrap_or(500.0);
                    let dir = (spawn - d.centre).normalize_or(DVec2::X);
                    let edge = (0..400)
                        .map(|k| d.centre + dir * (k as f64 * 5.0))
                        .find(|p| !d.contains(*p))
                        .unwrap_or(d.centre + dir * d.radius);
                    let p = edge + dir * out;
                    let c = d.centre;
                    let dd = c - p;
                    let yaw = dd.x.atan2(dd.y).to_degrees() as f32;
                    let eye = ground(p) + 38.0;
                    let pitch = ((ground(c) + 25.0 - eye) / dd.length()).atan().to_degrees() as f32;
                    FlyCamera::new((p.x as f32, eye as f32, p.y as f32), yaw, pitch)
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
        "hero" | "inside" | "sky" | "title" | "fight" | "hangout" | "far"
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
        // Down the street, facing the mad scientist's minions, in the open.
        let (mid, way) = hw_sim::first_fight_way(&solids, (spawn, spawn_dir));
        let p = mid + way * hw_sim::FIRST_FIGHT_HERO;
        let p = hw_client::camclip::open_spot(&solids, p, 2.0, ground(p));
        if let Some(s) = session.as_mut() {
            s.teleport(glam::DVec3::new(p.x, ground(p) + 0.3, p.y));
        }
        hero_cam_yaw = way.x.atan2(way.y).to_degrees() as f32 + 18.0;
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
    if view == "far" {
        // On a main road across town, past where the people and traffic of
        // the start reach: the square with them must follow.
        let shift = hw_sim::AREA_SHIFT + 600.0;
        let far = generated
            .world
            .main_roads
            .iter()
            .flat_map(|r| r.pts.windows(2).map(|w| (w[0], w[1])))
            .filter(|(a, _)| (*a - spawn).abs().max_element() > shift)
            .min_by(|(a, _), (b, _)| a.distance(spawn).total_cmp(&b.distance(spawn)));
        if let Some((a, b)) = far {
            let along = (b - a).normalize_or(DVec2::X);
            let p = a + along * 30.0;
            if let Some(s) = session.as_mut() {
                s.teleport(glam::DVec3::new(p.x, ground(p) + 30.0, p.y));
            }
            hero_cam_yaw = along.x.atan2(along.y).to_degrees() as f32;
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
    let mut cheated = cheat_level.is_some() || cheat_story.is_some();
    let mut colossus_breaks = 0u8;
    // The world the crash reporter last noted.
    let mut crash_noted: Option<u64> = None;
    let mut hero_cam = HeroCamera {
        look: menu.look,
        yaw: hero_cam_yaw,
        pitch: match view.as_str() {
            "inside" => -35.0,
            "sky" | "far" => -22.0,
            _ => -12.0,
        },
        distance: 7.0,
        boom: Default::default(),
    };
    // `HW_CAM_DIST` and `HW_CAM_PITCH` set the hero camera (close-up screenshots).
    let cam_env = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<f32>().ok());
    if let Some(d) = cam_env("HW_CAM_DIST") {
        hero_cam.distance = d.clamp(0.0, HeroCamera::MAX_DISTANCE);
    }
    if let Some(p) = cam_env("HW_CAM_PITCH") {
        hero_cam.pitch = p.clamp(-80.0, 80.0);
    }
    // Each hero's entity, and whether it is the Actors character yet.
    let mut hero_entities: BTreeMap<hw_core::NetId, (EntityId, bool)> = BTreeMap::new();
    // Taxi canopies: the hinged part, and how far open (0 shut, 1 up).
    let mut canopies: BTreeMap<hw_core::NetId, (EntityId, f32)> = BTreeMap::new();
    let mut tick_clock = 0.0f64;
    // The hero's flight toggle (V); sent with every input.
    let mut hero_flies = std::env::var_os("HW_FLY").is_some() || view == "sky" || view == "far";

    // HW_WHATS_NEW=1 opens the list at start (for screenshots).
    let mut whats_new = std::env::var_os("HW_WHATS_NEW").is_some();
    // A warning about a graphics driver known to draw wrongly, until closed.
    let mut driver_note = true;
    // A reflection probe over the street around the camera, moved along in
    // steps, so chrome and glass reflect the buildings around them.
    let mut street_probe: Option<(engine::reflections::ReflectionProbeId, (i32, i32))> = None;
    // The title screen, while it shows; a town being generated for another
    // world (a new hero's, or a friend's); a joined session waiting for it.
    // The title shows the city we start in (`HW_SEED`, `HW_POP` too).
    let mut title_screen = title_mode.then(|| {
        Title::new(Profile {
            seed: world_seed,
            people: if stand_in { people } else { world_people },
            ..profile.clone()
        })
    });
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
    let mut waiting: Option<Waiting> = None;
    // Props and landmarks of the current town.
    let mut extras = TownExtras::default();
    // Behind the title the props, trees and ground come in once the logo
    // has landed (they take a moment), not on the first frame.
    let mut extras_loaded = false;
    let mut extras_load: Option<ExtrasLoad> = None;
    // From the title down to the hero: (start, eye, yaw, pitch).
    let mut swoop: Option<(f32, Vec3, f32, f32)> = None;
    let mut music_fade_at: Option<f32> = None;
    // Broken buildings, once a game is on.
    let mut damage: Option<DamageView> = None;
    let shooting = std::env::var_os("ENGINE_SCREENSHOT").is_some();
    let mut hitch_noted = f32::NEG_INFINITY;
    // `HW_AT=x,y,z` (and `HW_YAW`): the hero starts there, as a hitch
    // report's `repro` line gives it.
    let mut start_at = std::env::var("HW_AT").ok().and_then(|s| {
        let v: Vec<f64> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
        (v.len() == 3).then(|| DVec3::new(v[0], v[1], v[2]))
    });
    hw_client::hitch::install(&[
        ("world", format!("seed {world_seed}, {world_people} people")),
        (
            "settings",
            format!(
                "quality {}, smooth {}, vsync {}, fov {}",
                profile.quality, profile.smooth, profile.vsync, profile.fov
            ),
        ),
        ("start", format!("view {view}, title {title_mode}")),
    ]);
    let run = Engine::run(
        format!("HeroWorld {}", hw_core::version()),
        move |world, frame| {
            // Notices late frames and writes what went on (see `hitch.rs`).
            let _hitch = hw_client::hitch::begin(world, frame.time);
            quality_log.note(world, t0.elapsed().as_secs_f32());
            if frame.first {
                hw_client::hitch::mark("first frame");
                hw_client::hitch::line(format!(
                    "first frame, {:.1} s after the start (the town was planned before the window opened)",
                    t0.elapsed().as_secs_f64()
                ));
                world.set_view_distance(6000.0)?;
                // Meshes live on the graphics card only: Engine lets go of
                // its RAM copy once uploaded (feet still stand on them).
                // Robot parts are shattered the frame they are made, before
                // that. `HW_KEEP_MESHES=1` keeps the copies (to compare).
                if std::env::var_os("HW_KEEP_MESHES").is_none() {
                    world.set_cpu_meshes(engine::CpuMeshes::Release);
                }
                // The player's settings (screenshots keep full detail).
                let mut p = profile.clone();
                p.smooth &= !shooting;
                hw_client::settings::apply(world, &p);
                hw_client::settings::apply_volumes(world, &p);
                // `HW_SHADING=<flags>`: Engine's drawing checks from the start.
                if let Some(f) = std::env::var("HW_SHADING")
                    .ok()
                    .and_then(|v| v.parse().ok())
                {
                    world.set_debug_shading(f);
                }
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
                    extras = hw_client::extras::spawn_now(
                        world,
                        generated.world.clone(),
                        town.green.clone(),
                    )?;
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

            hw_client::hitch::mark("extras");
            // The town's extras, made on a worker and put in a few per frame
            // (see `extras.rs`); on the title and, if it was left early, in
            // the game.
            if let Some(l) = extras_load.as_mut() {
                match l.step(world, EXTRAS_MS)? {
                    Some(e) => {
                        hw_client::hitch::event("props, trees and ground are in");
                        hw_client::hitch::line(format!(
                            "props, trees and ground are in, {:.1} s after the start",
                            t0.elapsed().as_secs_f64()
                        ));
                        hw_client::hitch::job("town extras", None);
                        extras = e;
                        extras_loaded = true;
                        extras_load = None;
                    }
                    None => hw_client::hitch::job("town extras", Some(l.state())),
                }
            }
            hw_client::hitch::mark("packs");
            // What heroes can be made of, once the packs are loaded.
            if hero_catalog.is_none() {
                if let Some(c) = actors.as_ref().and_then(|p| p.take_catalog()) {
                    if let Some(t) = title_screen.as_mut() {
                        t.set_catalog(c.clone());
                    }
                    hero_catalog = Some(c);
                }
            }
            hw_client::hitch::mark("title");
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
                // Another city finished planning: swap it in.
                if let Some((city, g)) = builds.poll() {
                    hw_client::hitch::event(format!(
                        "world {} ({} people) swapped in behind the title",
                        city.seed, city.people
                    ));
                    hw_client::hitch::job("new world", None);
                    stand_in = false;
                    town.clear(world);
                    extras.despawn(world);
                    world_seed = city.seed;
                    world_people = city.people;
                    // A big city takes a while to free: not on this thread.
                    hw_client::citybuild::dispose(std::mem::replace(&mut generated, g));
                    solids = generated.obstacles.clone();
                    town = TownView::new(generated.world.clone());
                    minimap = hw_client::minimap::Minimap::new(generated.world.clone());
                    sounds.stop(world);
                    traffic = hw_sim::traffic::Traffic::new(generated.area.clone());
                    citizens =
                        hw_sim::citizens::Citizens::new(generated.area.clone(), traffic.signals());
                    citizens.set_hour(game_hour() as f64);
                    stands = hw_sim::taxi::stands(&generated.area);
                    area_job = None;
                    area_held.clear();
                    traffic_view.clear(world);
                    radio.clear(world);
                    signal.clear(world);
                    story.clear(world);
                    riders.clear(world, &crowd);
                    crowd.clear(world);
                    sweepers.clear(world);
                    // The new town's extras come in like the first ones.
                    if let Some(l) = extras_load.take() {
                        l.abandon(world);
                    }
                    extras_loaded = false;
                }
                if !extras_loaded && extras_load.is_none() && t.wants_heavy() && !builds.busy() {
                    hw_client::hitch::event(
                        "props, trees and ground start loading behind the title",
                    );
                    hw_client::hitch::line(
                        "props, trees and ground start loading behind the title",
                    );
                    extras_load = Some(ExtrasLoad::start(
                        generated.world.clone(),
                        town.green.clone(),
                    ));
                }
                let eye = world.camera().eye();
                let eye = DVec2::new(eye.x as f64, eye.z as f64);
                hw_client::hitch::mark("title town");
                // A hamlet standing in behind the cover is never seen.
                if !(stand_in && builds.busy()) {
                    town.update_background(world, eye, TITLE_BUILD_MS);
                }
                if let Some(v) = extras.props.as_mut() {
                    v.update(eye, TITLE_BUILD_MS * 0.5);
                    v.flush(world)?;
                }
                if let Some(w) = extras.waterfront.as_mut() {
                    w.update(world, eye, true);
                }
                let (built, wanted) = town.progress(eye);
                let building = builds.coming();
                let what = match building {
                    Some(c) => format!(
                        "Building the {}...",
                        hw_client::citybuild::size_of(c.people).name.to_lowercase()
                    ),
                    None if wanted == 0 => String::new(),
                    None => "Building the city...".into(),
                };
                let done = if builds.busy() {
                    0.0
                } else if wanted == 0 {
                    1.0
                } else {
                    built as f32 / wanted as f32
                };
                t.set_world_ready(extras_loaded && !builds.busy(), done);
                hw_client::hitch::mark("title screen");
                let mut choice = t.frame(world, frame, &generated.world, &solids, (done, &what))?;
                // A joined world (or a new hero's) waits for its town.
                if choice.is_none() && !builds.busy() && done >= 1.0 {
                    match waiting.take() {
                        Some(Waiting::Joined(mut s)) => {
                            s.attach_town(&generated);
                            session = Some(*s);
                        }
                        Some(Waiting::Single) => choice = Some(Choice::Continue),
                        Some(Waiting::Host) => choice = Some(Choice::Host),
                        None => {}
                    }
                }
                let live = City {
                    seed: world_seed,
                    people: world_people,
                };
                let mine = City {
                    seed: t.profile.seed,
                    people: t.profile.people,
                };
                // Play in `city` once it is there: Some(then) when it is
                // the one we have, else it is planned and `then` waits.
                let mut want_world = |city: City, then: Waiting| {
                    builds.want(city, live);
                    if city == live {
                        return Some(then);
                    }
                    hw_client::hitch::job(
                        "new world",
                        Some(format!("seed {}, {} people", city.seed, city.people)),
                    );
                    waiting = Some(then);
                    None
                };
                hw_client::hitch::mark("title choice");
                match choice {
                    Some(Choice::Continue) => {
                        if want_world(mine, Waiting::Single).is_some() {
                            session = Some(
                                Session::single(Seed(world_seed), &generated, &name, false)
                                    .map_err(io)?,
                            );
                        }
                    }
                    Some(Choice::City(city)) => {
                        if matches!(waiting, Some(Waiting::Joined(_))) {
                            // A friend's world is on its way: it comes first.
                            t.status = Some((
                                "You are joining a friend: your own city can wait.".into(),
                                true,
                            ));
                        } else {
                            builds.want(city, live);
                        }
                    }
                    Some(Choice::NewHero { name: n, look }) => {
                        name = n;
                        t.profile.name = name.clone();
                        t.profile.look = look.map(|l| *l);
                        // A new hero picks an origin first.
                        t.profile.hero = None;
                        hero_file = None;
                        if want_world(mine, Waiting::Single).is_some() {
                            session = Some(
                                Session::single(Seed(world_seed), &generated, &name, false)
                                    .map_err(io)?,
                            );
                        }
                    }
                    Some(Choice::Host) => {
                        if want_world(mine, Waiting::Host).is_some() {
                            match Session::single(Seed(world_seed), &generated, &name, true) {
                                Ok(s) => session = Some(s),
                                Err(e) => {
                                    t.status =
                                        Some((format!("Could not open your world: {e}"), true))
                                }
                            }
                        }
                    }
                    Some(Choice::Join(code)) => match Session::join(&code, &name) {
                        Ok((mut s, seed, people)) => {
                            let theirs = City { seed, people };
                            if want_world(theirs, Waiting::Single).is_some() {
                                s.attach_town(&generated);
                                session = Some(s);
                            } else {
                                waiting = Some(Waiting::Joined(Box::new(s)));
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
                    quality_log.mark("the game starts (title left)");
                    hero_cam.yaw = yaw;
                    music_fade_at = Some(frame.time + 20.0);
                    t.profile.played = true;
                    t.profile.seed = world_seed;
                    t.profile.people = world_people;
                    // Nothing else is wanted now.
                    builds.stop();
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
                    // The game goes on drawing cells on the workers.
                    hw_client::hitch::event("left the title");
                    title_screen = None;
                }
                return Ok(());
            }
            hw_client::hitch::mark("menus");
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
            if let Some(p) = start_at.take() {
                session.teleport(p);
                if let Some(yaw) = std::env::var("HW_YAW").ok().and_then(|y| y.parse().ok()) {
                    hero_cam.yaw = yaw;
                }
                hw_client::hitch::event(format!("started at {:.0}, {:.0}, {:.0}", p.x, p.y, p.z));
            }
            driver_notice(&frame.ui, world, &mut driver_note);
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
                    shading: world.debug_shading(),
                };
                let at = me.unwrap_or_else(|| world.camera().eye().as_dvec3());
                for a in debug.ui(frame.ui.ctx(), &info) {
                    // Puts the hero at `p`, looking along `facing`.
                    let mut go = |session: &mut Session, p: glam::DVec3, facing: DVec2| {
                        hw_client::hitch::event(format!(
                            "debug menu: went to {:.0}, {:.0}, {:.0}",
                            p.x, p.y, p.z
                        ));
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
                            session.debug_sheet(move |s| dm::at_level(s, l));
                        }
                        A::Karma(k) => {
                            cheated = true;
                            session.debug_sheet(move |mut s| {
                                s.karma = s.karma.saturating_add(k);
                                s
                            });
                        }
                        A::BuyAll(theme) => {
                            cheated = true;
                            session.debug_sheet(move |s| dm::buy_all(s, theme));
                        }
                        A::ClearPowers => {
                            cheated = true;
                            session.debug_sheet(dm::clear_powers);
                        }
                        A::Regard(f, by) => {
                            cheated = true;
                            session.debug_sheet(move |s| dm::regard(s, f, by));
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
                            // Windows follow by glow channel: no rebuild.
                            set_game_hour(h);
                            hw_client::lighting::set_sky(world, h);
                            citizens.set_hour(h as f64);
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
                        A::CutLevel(step) => {
                            town.cut_level = (town.cut_level as i32 + step).clamp(0, 40) as u32;
                            let changed = town.cells_near(town.cut_focus, CUT_REACH);
                            town.rebuild(world, changed);
                        }
                        A::Map => town_map = !town_map,
                        A::Rebuild(surface) => {
                            quality_log.mark(if surface {
                                "redo window surface"
                            } else {
                                "rebuild screen buffers"
                            });
                            world.request_renderer_rebuild(surface, !surface);
                        }
                        A::GpuCheck => {
                            quality_log.mark("GPU mesh check");
                            world.request_gpu_check();
                        }
                        A::Reupload(meshes) => {
                            quality_log.mark(if meshes {
                                "re-upload meshes"
                            } else {
                                "re-upload textures"
                            });
                            world.request_gpu_reupload(meshes, !meshes);
                        }
                        A::Shading(flags) => {
                            quality_log.mark(&format!("drawing checks {flags}"));
                            world.set_debug_shading(flags);
                        }
                    }
                }
            }
            hw_client::hitch::mark("looks");
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
            hw_client::hitch::mark("sky and lights");
            // The day runs: sky, grade, lit windows and the people out.
            let hour = hour_now(session.time(tick_clock / hw_sim::DT));
            if swoop.is_none() {
                hw_client::lighting::set_sky(world, hour);
            }
            citizens.set_hour(hour as f64);
            if let (Some(lights), Some(props)) = (extras.lights.as_mut(), extras.props.as_ref()) {
                let dark = swoop.is_none() && hw_client::nightlights::dark_at(hour);
                lights.update(world, world.camera().eye(), dark, props)?;
            }
            hw_client::hitch::mark("underground");
            if let Some(under) = extras.under.as_mut() {
                under.update(world, world.camera().eye(), &generated.world)?;
            }
            hw_client::hitch::mark("deep");
            if let Some(deep) = extras.deep.as_mut() {
                deep.update(world, world.camera().eye(), &generated.world)?;
            }
            hw_client::hitch::mark("grass");
            if let Some(g) = extras.grass.as_mut() {
                g.update(world, world.camera().eye())?;
            }
            hw_client::hitch::mark("dystopia");
            if let Some(d) = extras.dystopia.as_mut() {
                d.update(world, &generated.world, frame.dt, hour)?;
            }
            hw_client::hitch::mark("mast");
            if let Some(m) = extras.mast.as_mut() {
                m.update(world, session.time(tick_clock / hw_sim::DT), frame.dt)?;
            }
            hw_client::hitch::mark("rocket port");
            if let Some(r) = extras.rocket.as_mut() {
                r.update(world, session.time(tick_clock / hw_sim::DT), frame.dt)?;
            }
            hw_client::hitch::mark("atom");
            if let Some(a) = extras.atom.as_mut() {
                a.update(world, frame.dt)?;
            }
            hw_client::hitch::mark("drill rig");
            if let Some(d) = extras.drill.as_mut() {
                d.update(world, session.time(tick_clock / hw_sim::DT))?;
            }
            hw_client::hitch::mark("weather tower");
            if let Some(w) = extras.weather.as_mut() {
                let eye = world.camera().eye();
                let sheltered = town.inside.is_some()
                    || hw_client::soundscape::under(&generated.world, eye)
                        != hw_client::soundscape::Under::No;
                w.update(
                    world,
                    session.time(tick_clock / hw_sim::DT),
                    hour,
                    frame.dt,
                    sheltered,
                )?;
                // Under its cloud the sky greys over and the sun goes in.
                if swoop.is_none() && w.wet() > 0.0 {
                    hw_client::lighting::set_sky_wet(world, hour, w.wet());
                }
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
            hw_client::hitch::mark("input and menus");
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

            hw_client::hitch::mark("hero input");
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
                let alt = frame.ui.ctx().input(|i| i.modifiers.alt);
                let f = fight.input(
                    input,
                    me_state,
                    &drawn,
                    cam.eye(),
                    look,
                    fightview::Bar {
                        sheet: Some(&my_sheet),
                        tray: &powers.tray,
                        alt,
                    },
                );
                if f.power != 0 && !(on_ui && f.power == hw_net::Power::Strike.code()) {
                    pending_power = f.power;
                }
                // HW_AUTOFIGHT=1 fights on its own (for screenshots): bolts
                // while there is energy, punches up close.
                if auto_fight
                    && (frame.time / 0.75).floor() != ((frame.time - frame.dt) / 0.75).floor()
                {
                    if fight.target.is_none() {
                        let eye = cam.eye();
                        let t = fightview::pick_target(&drawn, eye, look, |e| {
                            fight.sees(me_state, eye, e)
                        });
                        fight.target = t;
                    }
                    let close = me_state
                        .zip(fight.target.and_then(|t| drawn.iter().find(|e| e.id == t)))
                        .is_some_and(|(m, t)| {
                            glam::DVec3::from_array(m.pos).distance(glam::DVec3::from_array(t.pos))
                                < 2.6
                        });
                    // Every power the hero has, in turn; punches up close.
                    let every = [3u8, 2, 6, 11, 2, 5, 13, 2, 4, 10, 2, 15, 5];
                    let turn = if auto_turn.is_empty() {
                        &every[..]
                    } else {
                        &auto_turn[..]
                    };
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

            // `HW_BENCH`: the hero goes the benchmark's way, then the report.
            if let Some(b) = bench.as_mut() {
                use hw_client::bench::{Route, Step};
                match b.step(frame.time) {
                    Step::Wait => {}
                    Step::Go { at, dir, height } => {
                        let o = world.occlusion_stats();
                        bench_occlusion.0 += 1;
                        bench_occlusion.1 += u64::from(o.tested);
                        bench_occlusion.2 += u64::from(o.hidden);
                        let floor = generated
                            .world
                            .ground(at)
                            .max(hw_gen::urban::world::WATER_LEVEL);
                        session.teleport(DVec3::new(at.x, floor + height, at.y));
                        hero_cam.yaw = dir.x.atan2(dir.y).to_degrees() as f32;
                        hero_cam.pitch = if b.route == Route::Flight {
                            -22.0
                        } else {
                            -12.0
                        };
                        hero_flies = b.route == Route::Flight;
                        hero_input.fly = hero_flies;
                    }
                    Step::Done => {
                        let gpu = world.gpu_info().map_or("unknown".into(), |g| {
                            format!("{} ({}, driver {})", g.name, g.backend, g.driver_info)
                        });
                        let facts = [
                            ("GPU", gpu),
                            (
                                "picture",
                                format!(
                                    "{}x{}, quality {} at the end (setting {}), smooth {}, vsync {}",
                                    frame.width,
                                    frame.height,
                                    world.quality().name(),
                                    profile.quality,
                                    profile.smooth,
                                    profile.vsync
                                ),
                            ),
                            ("world", format!("seed {world_seed}, {world_people} people")),
                            (
                                "meshes in RAM",
                                format!(
                                    "{:.0} MB, ground {:.0} MB ({} released after upload, {:?})",
                                    world.cpu_mesh_bytes() as f64 / 1048576.0,
                                    world.ground_mesh_bytes() as f64 / 1048576.0,
                                    world.cpu_mesh_counts().0,
                                    world.cpu_meshes()
                                ),
                            ),
                            (
                                "render scale",
                                format!("{:.0}%", world.render_scale() * 100.0),
                            ),
                            (
                                "occlusion",
                                if world.occlusion_culling() {
                                    let (n, tested, hidden) = bench_occlusion;
                                    let n = n.max(1);
                                    format!(
                                        "{:?}: {} meshes tested a frame, {} hidden ({:.0}%)",
                                        world.occlusion_mode(),
                                        tested / n,
                                        hidden / n,
                                        100.0 * hidden as f64 / tested.max(1) as f64
                                    )
                                } else {
                                    "off".into()
                                },
                            ),
                            (
                                "timings",
                                if std::env::var_os("ENGINE_GPU_PROFILE_EVERY").is_some() {
                                    "Engine's GPU timings on: frames are slower than in play".into()
                                } else {
                                    "plain".into()
                                },
                            ),
                        ];
                        let stats = hw_client::hitch::take_stats().unwrap_or_default();
                        let report = b.report(&stats, &facts);
                        print!("{report}");
                        if let Some(path) = b.write(&report) {
                            println!("benchmark report: {}", path.display());
                        }
                        bench = None;
                        world.request_exit();
                    }
                }
            }

            hw_client::hitch::mark("server ticks");
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
            hw_client::hitch::count("server ticks", ticks as u64);
            if ticks == 5 {
                tick_clock = 0.0;
            }
            // Single player: the server runs these ticks on its own thread
            // while this frame goes on; we draw the world as it stood after
            // the frame before (waiting here only if it is not there yet).
            hw_client::hitch::mark("server wait");
            session.sync();
            if let Some((busy, _)) = session.server_load() {
                // The server thread's own time on the frame before's ticks.
                hw_client::hitch::count("server busy us", (busy * 1e6) as u64);
            }
            hw_client::hitch::mark("news");
            // Damage: broken buildings, repairs, impacts.
            let joined = session.joined();
            let mut news = session.take_news();
            if let Some(rx) = &area_job {
                match rx.try_recv() {
                    Ok((area, next, people, next_stands)) => {
                        hw_client::hitch::event("new people and traffic square swapped in");
                        area_job = None;
                        generated.area = area;
                        traffic = next;
                        citizens = people;
                        citizens.set_hour(game_hour() as f64);
                        stands = next_stands;
                        traffic_view.clear(world);
                        riders.clear(world, &crowd);
                        crowd.clear(world);
                        sweepers.clear(world);
                        if let Some(s) = extras.signals.as_mut() {
                            s.clear(world);
                        }
                        news.splice(0..0, std::mem::take(&mut area_held));
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => area_job = None,
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                }
            }
            if area_job.is_some() {
                // Traffic news belongs to the square still being made.
                news.retain(|m| {
                    let traffic_news = matches!(
                        m,
                        hw_net::ToClient::Traffic { .. }
                            | hw_net::ToClient::CarsWrecked { .. }
                            | hw_net::ToClient::CarsCleared { .. }
                            | hw_net::ToClient::AreaMoved { .. }
                    );
                    if traffic_news {
                        area_held.push(m.clone());
                    }
                    !traffic_news
                });
            }
            if let Some(i) = news
                .iter()
                .position(|m| matches!(m, hw_net::ToClient::AreaMoved { .. }))
            {
                if let hw_net::ToClient::AreaMoved { x, z } = news[i] {
                    hw_client::hitch::event(format!(
                        "people and traffic square moves to {x:.0}, {z:.0} (built on a thread)"
                    ));
                    // What came before it is for the old square; what comes
                    // after waits for the new one.
                    area_held.extend(news.drain(i + 1..));
                    news.truncate(i);
                    let world_town = generated.world.clone();
                    let half = (generated.area.hi.x - generated.area.lo.x) * 0.5;
                    let (tx, rx) = std::sync::mpsc::channel();
                    std::thread::spawn(move || {
                        let area = std::sync::Arc::new(hw_gen::urban::area::Area::around(
                            world_town,
                            DVec2::new(x, z),
                            half,
                        ));
                        let next = hw_sim::traffic::Traffic::new(area.clone());
                        let people = hw_sim::citizens::Citizens::new(area.clone(), next.signals());
                        let next_stands = hw_sim::taxi::stands(&area);
                        let _ = tx.send((area, next, people, next_stands));
                    });
                    area_job = Some(rx);
                }
            }
            // Broken and mended street props: their places, and lamps on them.
            radio.solids = Some(solids.clone());
            fight.solids = Some(solids.clone());
            radio.town = Some(generated.world.clone());
            news.retain(|m| match m {
                hw_net::ToClient::Story { .. } => {
                    story.hear(m);
                    false
                }
                hw_net::ToClient::Incident { id, at, yaw, .. } => {
                    hw_client::hitch::event(format!("incident {id} at {:.0}, {:.0}", at[0], at[2]));
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
                        // The people in it get out and run (or, wrecked
                        // before we came, it is empty).
                        if let Some(v) = traffic_view.wreck_of(id) {
                            let blow = hw_sim::props::byte_to_yaw(fell);
                            let eye = world.camera().eye().as_dvec3();
                            let blow = live.then_some(blow);
                            let _ = riders.bail_out(world, &crowd, &citizens, &v, blow, eye);
                        }
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
                        riders.towed(id);
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
            hw_client::hitch::mark("props and damage");
            if let Some(v) = extras.props.as_mut() {
                v.flush(world)?;
            }
            damage
                .get_or_insert_with(|| DamageView::new(joined))
                .news(world, &mut town, &solids, news);

            hw_client::hitch::mark("people built");
            // People finished on the worker thread (HW_WAIT_PEOPLE=1 waits
            // for all of them at the start, for screenshots).
            if let Some(p) = &actors {
                let mut done = std::mem::take(&mut held);
                done.extend(p.finished());
                if frame.first && std::env::var_os("HW_WAIT_PEOPLE").is_some() {
                    let all = (hw_sim::citizens::LOOKS + hw_sim::citizens::MOLE_LOOKS) as usize
                        + 1
                        + 2 * people::VILLAIN_LOOKS as usize;
                    let more = p.wait(all.saturating_sub(done.len()));
                    done.extend(more);
                }
                // A new crowd uploads its look's model and maps (tens of
                // MB): one a frame, so they do not pile into one long frame.
                let all_now = frame.first || shooting;
                let mut crowds = 0;
                for b in done {
                    if let Who::Citizen(look) = b.who {
                        if crowds > 0 && !all_now {
                            held.push(b);
                            continue;
                        }
                        crowds += 1;
                        crowd.add_look(world, look, &b)?;
                    }
                    hw_client::hitch::count("characters taken in", 1);
                    models.insert(b.who.clone(), b.model.clone());
                }
            }

            hw_client::hitch::mark("heroes");
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
                        cabs.push(hw_sim::traffic::Vehicle {
                            id: h.id.0 as u64 | hw_client::riderview::CAB_BIT,
                            kind: hw_sim::traffic::VehicleKind::Taxi,
                            pos: DVec3::from_array(h.pos),
                            dir: DVec3::new(y.sin() as f64, 0.0, y.cos() as f64),
                            roll: 0.0,
                            speed: 0.0,
                            odometer: 0.0,
                        });
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
                } else if h.kind == hw_net::EntityKind::Sidekick {
                    hw_sim::sidekick::SIZE
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
                    if villain || bystander || h.kind == hw_net::EntityKind::Sidekick {
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
                if (bystander || h.kind == hw_net::EntityKind::NpcHero) && slow {
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
                    // One of Hero Hall's own heroes.
                    hw_net::EntityKind::NpcHero => Who::NpcHero(h.mount.map_or(0, |m| m.1)),
                    // A hero's robot sidekick, by the power's theme.
                    hw_net::EntityKind::Sidekick => {
                        Who::Sidekick(h.mount.map_or(0, |m| m.1).min(1))
                    }
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
                // A blow rocks the struck one back for a moment.
                if let Some((push, lean)) = fight.recoil(h.id) {
                    offset += push;
                    d.pitch += lean;
                }
                let o = tipped_origin(feet + offset, &d);
                let (yaw, pitch, roll) = d.place_angles();
                world.set_place(
                    e,
                    Place::new(o.x, o.y, o.z)
                        .with_yaw_deg(yaw)
                        .with_pitch_deg(pitch)
                        .with_roll_deg(roll)
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
                    // A level up: a fist to the sky, standing or hovering.
                    None if mine == Some(h.id)
                        && frame.time < power_pose_until
                        && matches!(d.clip, "idle" | "hover") =>
                    {
                        ("power pose", true)
                    }
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
                        )
                        || (clip == "power pose" && h.grounded);
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
            hw_client::hitch::mark("robots");
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
            // `HW_VIEW=call` (with HW_INCIDENT=<kind>): the light over the
            // first call, from over the rooftops 50 m off.
            if view == "call" {
                if let Some(c) = radio.calls.values().next() {
                    let p = c.at + Vec3::new(35.0, 28.0, 35.0);
                    let d = c.at + Vec3::Y * 12.0 - p;
                    let pitch = (d.y / glam::Vec2::new(d.x, d.z).length())
                        .atan()
                        .to_degrees();
                    fly = FlyCamera::new((p.x, p.y, p.z), d.x.atan2(d.z).to_degrees(), pitch);
                }
            }
            let me_state = mine.and_then(|id| heroes.iter().find(|e| e.id == id));
            hw_client::hitch::mark("fight");
            fight.update(world, frame.dt, &heroes, me_state);
            // Our sheet as the host has it: a level up shows, the file is kept.
            if let Some(s) = session.take_sheet() {
                let before = hw_sim::progress::level_of(my_sheet.lifetime);
                let after = hw_sim::progress::level_of(s.lifetime);
                if sheet_synced && after > before {
                    powers.level_up(after);
                    power_pose_until = frame.time + POWER_POSE_SECONDS;
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

            hw_client::hitch::mark("crowd and traffic");
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
            let game_time = session.time(tick_clock / hw_sim::DT);
            let eye = world.camera().eye();
            let eye2 = DVec2::new(eye.x as f64, eye.z as f64);
            let dt = frame.dt as f64;
            // The traffic drives on and its cars are placed on a worker,
            // while the citizens go to Engine here; the join below is the
            // only meeting point (the borrows keep the two apart).
            let (traffic_plan, survey) = std::thread::scope(|scope| {
                let (traffic, traffic_view) = (&mut traffic, &mut traffic_view);
                let town_plan = &generated.world;
                let job = std::thread::Builder::new()
                    .name("hw-traffic".into())
                    .spawn_scoped(scope, move || {
                        traffic.advance_to(game_time);
                        let plan = traffic_view.plan(traffic, eye2, game_time, dt);
                        // What the soundscape asks of the town round the camera.
                        let survey = hw_client::soundscape::Survey {
                            cars: traffic.near(eye2, 90.0, game_time, 40),
                            under: hw_client::soundscape::under(town_plan, eye),
                            district: hw_client::soundscape::district(town_plan, eye2),
                        };
                        (plan, survey)
                    })
                    .expect("start the traffic worker");
                // Citizens around the camera, at the game's time.
                let crowd_done = crowd
                    .update(world, &citizens, &alarms, eye2, game_time, dt)
                    .and_then(|()| sweepers.update(world, &citizens, eye2, game_time, dt));
                let plan = match job.join() {
                    Ok(p) => p,
                    Err(panic) => std::panic::resume_unwind(panic),
                };
                crowd_done.map(|()| plan)
            })?;
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
            hw_client::hitch::mark("traffic view");
            // Joined a friend: our own hero bumps into the cars and airships
            // near it as the host's does, from the same game clock (the host
            // sets them itself when we host).
            if matches!(session, Session::Remote { .. }) {
                if let Some(me) = session.my_position(&heroes) {
                    let mut movers = traffic.car_solids(&[me], 25.0, game_time);
                    if let Some(pad) = extras.rocket.as_ref().and_then(|r| r.pad()) {
                        movers.extend(pad.solids(game_time, &[me], 25.0));
                    }
                    movers.extend(airships.solids_near(&[me], game_time));
                    movers.extend(story.statue_solids(&generated.world));
                    solids.set_movers(movers);
                }
            }
            if let (Some(s), Some(props)) = (extras.signals.as_mut(), extras.props.as_ref()) {
                s.update(world, &traffic, props, eye2, game_time)?;
            }
            traffic_view.apply(world, traffic_plan)?;
            // Drivers and passengers in the cars near the camera, and the
            // cabbies of hero taxis.
            hw_client::hitch::mark("riders");
            let mut cars = traffic_view.riding().to_vec();
            cars.append(&mut cabs);
            riders.update(
                world,
                &crowd,
                &cars,
                &citizens,
                eye.as_dvec3(),
                frame.dt as f64,
            )?;
            // `HW_VIEW=riders`: beside the driver of the nearest car (of
            // `HW_RIDER=cruiser|bubble|hover|taxi`, if given), looking in.
            if view == "riders" {
                let want = std::env::var("HW_RIDER").unwrap_or_default();
                let near = cars.iter().find(|v| {
                    let name = match v.kind {
                        hw_sim::traffic::VehicleKind::Car(m, _) => format!("{m:?}"),
                        hw_sim::traffic::VehicleKind::SkyCar(_) => "Hover".into(),
                        hw_sim::traffic::VehicleKind::Taxi => "Taxi".into(),
                    };
                    want.is_empty() || name.eq_ignore_ascii_case(&want)
                });
                if let Some(at) = near.and_then(|v| hw_client::riderview::seat_place(v, 0)) {
                    let yaw = at.yaw_degrees.to_radians();
                    let ahead = Vec3::new(yaw.sin(), 0.0, yaw.cos());
                    let left = Vec3::new(ahead.z, 0.0, -ahead.x);
                    // `HW_RIDER_CAM=front` looks in through the windscreen.
                    let p = if std::env::var("HW_RIDER_CAM").is_ok_and(|c| c == "front") {
                        at.position + ahead * 3.2 + left * 0.4 + Vec3::Y * 0.7
                    } else {
                        at.position + left * 1.9 + ahead * 0.6 + Vec3::Y * 0.35
                    };
                    let d = at.position + Vec3::Y * 0.35 - p;
                    let pitch = (d.y / glam::Vec2::new(d.x, d.z).length())
                        .atan()
                        .to_degrees();
                    fly = FlyCamera::new((p.x, p.y, p.z), d.x.atan2(d.z).to_degrees(), pitch);
                }
            }
            // K knocks the people around the hero over (a test of ragdolls).
            if !flying && input.pressed(Key::K) {
                if let Some(p) = session.my_position(&heroes) {
                    let n = crowd.knock_down(world, p.as_vec3() + Vec3::Y, 6.0);
                    eprintln!("knocked over {n}");
                }
            }

            hw_client::hitch::mark("camera");
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
                let hour = title::TITLE_HOUR + (hour - title::TITLE_HOUR) * k;
                hw_client::lighting::set_sky(world, hour);
                if k >= 1.0 {
                    swoop = None;
                }
            }
            hw_client::hitch::mark("fight screen");
            // The fight's screen over everything.
            {
                let view_proj = world.camera().view_projection(frame.aspect);
                let me_state = mine.and_then(|id| heroes.iter().find(|e| e.id == id));
                let names = |e: &hw_net::EntityState| {
                    e.owner
                        .and_then(|o| session.player_name(o))
                        .unwrap_or_else(|| "Your teammate".into())
                };
                let moved = fight.hud(
                    frame.ui.ctx(),
                    view_proj,
                    me_state,
                    &heroes,
                    &names,
                    fightview::Bar {
                        sheet: Some(&my_sheet),
                        tray: &powers.tray,
                        alt: false,
                    },
                );
                if let Some(c) = moved {
                    powers.tray.apply(c);
                    save_tray(&powers.tray);
                }
                let fighting = me_state.and_then(|m| m.combat).map_or(1, |c| c.level);
                hw_client::hitch::mark("radio");
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
                story.update(world, &mut radio, frame.dt);
                story.statue(
                    world,
                    actors.as_ref(),
                    &models,
                    my_look.as_ref(),
                    &generated.world,
                )?;
                let cam = world.camera();
                let hero_at = session.my_position(&heroes).map(|p| p.as_vec3());
                story.panel(frame.ui.ctx(), hero_at);
                radio.panel(
                    frame.ui.ctx(),
                    cam.eye(),
                    cam.target() - cam.eye(),
                    fighting,
                    hero_at,
                );
                let cam = world.camera();
                let look = cam.target() - cam.eye();
                hw_client::hitch::mark("compass and map");
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
                hw_client::hitch::mark("minimap");
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
                hw_client::hitch::mark("soundscape");
                // `survey`: asked of the town on the traffic worker above;
                // people from the crowd.
                let scene = hw_client::soundscape::Scene {
                    now: frame.time as f64,
                    dt: frame.dt,
                    eye,
                    hero: hero_at,
                    flying: hero_flies,
                    speed: hero_speed,
                    height,
                    indoors: town.inside.is_some(),
                    under: survey.under,
                    hour: game_hour(),
                    district: survey.district,
                    around: minimap.surroundings(eye2, 45.0),
                    people: crowd.within(eye, 35.0),
                    cars: &survey.cars,
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
                    tesla: extras
                        .dystopia
                        .as_mut()
                        .map(|d| d.heard())
                        .unwrap_or_default(),
                    mast: extras.mast.as_mut().map(|m| m.heard()).unwrap_or_default(),
                    rocket: extras
                        .rocket
                        .as_mut()
                        .map(|r| r.heard())
                        .unwrap_or_default(),
                    atom: extras.atom.as_mut().map(|a| a.heard()).unwrap_or_default(),
                    drill: extras.drill.as_mut().map(|d| d.heard()).unwrap_or_default(),
                    weather: extras
                        .weather
                        .as_mut()
                        .map(|w| w.heard())
                        .unwrap_or_default(),
                    works: extras
                        .dystopia
                        .as_mut()
                        .map(|d| d.works_heard())
                        .unwrap_or_default(),
                };
                hw_client::hitch::mark("sounds");
                sounds.update(world, &scene);
                hw_client::hitch::mark("powers window");
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
                // Powers bought or given up take or free their keys.
                if powers.tray.sync(&my_sheet) {
                    save_tray(&powers.tray);
                }
            }
            drawn = heroes;

            // Where we are, for the hitch log (four times a second).
            if frame.time - hitch_noted >= 0.25 {
                hitch_noted = frame.time;
                note_for_hitches(
                    session.my_position(&drawn),
                    world.camera().eye(),
                    if flying {
                        (fly.yaw_deg, fly.pitch_deg)
                    } else {
                        (hero_cam.yaw, hero_cam.pitch)
                    },
                    (world_seed, world_people),
                    (hour, hero_flies, flying),
                );
                let c = (generated.area.lo + generated.area.hi) * 0.5;
                hw_client::hitch::context(
                    "streamed",
                    format!(
                        "people and traffic square round {:.0}, {:.0}; {} citizens, {} cars drawn",
                        c.x,
                        c.y,
                        crowd.shown(),
                        traffic_view.shown()
                    ),
                );
                let planning = generated.world.planning();
                hw_client::hitch::job(
                    "town planning",
                    (planning > 0).then(|| format!("{planning} districts or verge tiles")),
                );
                let drawing = town.drawing();
                hw_client::hitch::job(
                    "town cells",
                    (drawing > 0).then(|| format!("{drawing} being drawn")),
                );
                hw_client::hitch::job(
                    "next area",
                    area_job
                        .is_some()
                        .then(|| "people and traffic for the new square".into()),
                );
            }
            hw_client::hitch::mark("town");
            // Roads and buildings around the camera, full detail close by.
            let eye = world.camera().eye();
            town.update_background(
                world,
                DVec2::new(eye.x as f64, eye.z as f64),
                BUILD_BUDGET_MS,
            );
            hw_client::hitch::mark("props");
            // Street props, trees and hedges around the camera.
            if let Some(v) = extras.props.as_mut() {
                v.update(
                    DVec2::new(eye.x as f64, eye.z as f64),
                    BUILD_BUDGET_MS * 0.5,
                );
                v.flush(world)?;
            }
            hw_client::hitch::mark("waterfront");
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

/// Tells the hitch log where the hero and the camera are, and how to
/// start a game right there.
fn note_for_hitches(
    hero: Option<DVec3>,
    eye: Vec3,
    (yaw, pitch): (f32, f32),
    (seed, people): (u64, f64),
    (hour, hero_flies, free_cam): (f32, bool, bool),
) {
    use hw_client::hitch::context;
    let at = hero.unwrap_or(eye.as_dvec3());
    context(
        "where",
        format!(
            "hero {}, camera {:.0}, {:.0}, {:.0} looking {yaw:.0} deg, pitch {pitch:.0}{}{}",
            hero.map_or("-".into(), |p| format!(
                "{:.1}, {:.1}, {:.1}",
                p.x, p.y, p.z
            )),
            eye.x,
            eye.y,
            eye.z,
            if hero_flies { ", flying" } else { "" },
            if free_cam { ", free camera" } else { "" },
        ),
    );
    context("hour", format!("{hour:.2}"));
    context(
        "repro",
        format!(
            "HW_SEED={seed} HW_POP={people} HW_HOUR={hour:.1} HW_AT={:.1},{:.1},{:.1} HW_YAW={yaw:.0}{}",
            at.x,
            at.y,
            at.z,
            if hero_flies { " HW_FLY=1" } else { "" }
        ),
    );
}

/// Someone waiting on the title for another world's town to be generated.
enum Waiting {
    /// A new hero: plays alone once the town is there.
    Single,
    /// Opens the world to friends once the town is there.
    Host,
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

/// Below the version: Engine's warning when the graphics driver is known to
/// draw wrongly (an old NVIDIA driver drew far houses over near walls).
fn driver_notice(ui: &engine::ui::UiFrame, world: &World, open: &mut bool) {
    use engine::ui::egui;
    if !*open {
        return;
    }
    let Some(warning) = world.gpu_info().and_then(|g| g.driver_warning()) else {
        return;
    };
    egui::Area::new(egui::Id::new("driver notice"))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-10.0, 44.0))
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_unmultiplied(120, 20, 20, 220))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(8, 6))
                .show(ui, |ui| {
                    ui.set_max_width(360.0);
                    ui.label(egui::RichText::new(warning).color(egui::Color32::WHITE));
                    if ui.button("OK").clicked() {
                        *open = false;
                    }
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

/// The next square with people and traffic, made on a worker thread: the
/// square, its traffic, its people and its taxi stands.
type NextArea = (
    std::sync::Arc<hw_gen::urban::area::Area>,
    hw_sim::traffic::Traffic,
    hw_sim::citizens::Citizens,
    Vec<hw_sim::taxi::Stand>,
);
