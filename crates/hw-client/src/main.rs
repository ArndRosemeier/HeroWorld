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
//! `hero` (default), `inside`, `sky`, `overview`, `street`, `river`, `cutaway`, `park`, `pad`, `spire`, `plinth` (the spire plaza's edge and steps close up), `lighthouse`, `quay`, `wrecked`, `traffic`, `signals`, `market`, `skylanes`, `lift` (a stair pavilion), `stairs` (part way down its spiral), `tunnel`, `molehaven`, `molegate` (its gate on the street), `molesign` (a signpost to it), `moleramp` (the sign over the ramp down from the tunnels), `moleroom` (a side chamber), `tube` (a tube station's street pavilion), `station`, `tubeline`, `capsule`, `ruin`, `lab`, `kiosk`, `grotto`, `comet`, `cometlobby`, `cometgallery`, `cometstairs`, `comethall`, `cometlift` (a Comet building with its game-scale interior; `HW_COMET=0|1|2` picks the atrium, grand hall or split-level one), `hospital`, `ward`, `pods`, `hospitaldoor` (the nearest hospital: across its plaza, in its Recovery Hall, at its pods, at its door), `rocket`, `rocketpad`, `rocketarm`, `rocketboard` (the rocket port; `HW_ROCKET_T=<s>` freezes its first launch that many seconds in: 5 venting, 25 counting, 31 lit, 40 climbing, 290 landing), `call` (the first call on the police visor, with `HW_INCIDENT`; `HW_VISOR=1` keeps the visor down, `HW_VISOR=0.4` freezes it part way in), `far` (a main road across town, where people and traffic must follow) or
//! town: 300 a hamlet, 40000 the default, millions a metropolis; joining
//! players get the host's); `HW_NAME`; `HW_VIEW` =
//! `hero` (default), `inside`, `sky`, `overview`, `street`, `river`, `cutaway`, `park`, `pad`, `spire`, `plinth` (the spire plaza's edge and steps close up), `lighthouse`, `weather` (the weather tower and its shower; `weathertop`, `weatherfoot`, `weatherstreet` close up; `HW_WEATHER=brew|rain|clear|bow|fair` picks the moment), `quay`, `wrecked`, `traffic`, `signals`, `market`, `skylanes`, `lift` (a stair pavilion), `stairs` (part way down its spiral), `tunnel`, `molehaven`, `molegate` (its gate on the street), `molesign` (a signpost to it), `moleramp` (the sign over the ramp down from the tunnels), `moleroom` (a side chamber), `tube` (a tube station's street pavilion), `station`, `tubeline`, `capsule`, `ruin`, `lab`, `kiosk`, `grotto`, `far` (a main road across town, where people and traffic must follow) or
//! `props` picks the starting camera (handy with `ENGINE_SCREENSHOT`; `props` looks at the
//! nearest prop whose kind contains `HW_PROP`, a car by default); `HW_CUT_LEVEL` the floor shown
//! cut open; `HW_WALK=1` makes the hero run forward on its own (for tests);
//! `HW_LEVEL=n` plays a level-n hero with karma to spend, `HW_POWERS=1`
//! with the whole tree bought (the first of each fork; `HW_ORIGIN=n` picks
//! the origin, `HW_ELEMENT=n` the element, 1-6);
//! `HW_VIEW=powers` opens the powers window, `HW_VIEW=origin` shows a new
//! hero's origin card.
//! `HW_VIEW=fight` starts in front of the minions, `HW_AUTOFIGHT=1` (or a list of power codes)
//! fights them on its own (both for screenshots);
//! `HW_SMASH=<frames>` blasts the building `HW_SMASH_AT` metres (4) in front
//! of the camera that many frames in;
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
//! report names; `HW_CAM_DIST`, `HW_CAM_PITCH` and `HW_CAM_YAW` set the hero camera.
//! `HW_TRICK=wave` (or another of `tricks::TRICKS`) has a standing sidekick do that trick.

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
/// People indoors are shown within this distance of the camera...
const INDOOR_SHOWN_M: f64 = 70.0;
/// ...and at most this many (the nearest).
const INDOOR_SHOWN_MAX: usize = 120;
/// The tube's folk are shown this far from the camera, this many at most.
const TUBE_FOLK_SHOWN_M: f64 = 90.0;
const TUBE_FOLK_SHOWN_MAX: usize = 48;
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
    /// The yaw the camera looked with last frame (degrees): `yaw`, or
    /// turned when looking from beyond a portal.
    view_yaw: f32,
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
        gates: &[hw_sim::gates::Pair],
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
        // A boom reaching back through a portal looks from beyond its twin,
        // so the hero just through never vanishes behind the rift.
        if let Some((eye, turn)) = hw_client::gateview::eye_through(gates, head, eye) {
            self.view_yaw = self.yaw + turn;
            world.look_first_person(eye, self.view_yaw, self.pitch);
            return;
        }
        self.view_yaw = self.yaw;
        world.look_first_person(eye, self.yaw, self.pitch);
    }
}

/// `HW_PORTAL=1`: the portal power on its own, for screenshots. Half a
/// second in the hero marks home behind it (facing back the way the
/// camera looks from); walks on `HW_PORTAL_WALK` seconds (1.2); opens a
/// portal ahead (seen through: the street behind, out of home); and walks
/// through it. With `HW_PORTAL_SHOT=<dir>`
/// it saves `open.png` (the portal open, seen through) and `through.png`
/// (just out at home), then quits.
#[derive(Default)]
struct PortalScript {
    /// Frame times (s, count) standing before the portal opens and
    /// standing looking at it open: printed at the end.
    before: (f32, u32),
    open: (f32, u32),
    told: bool,
    last: Option<std::time::Instant>,
    /// Aim behind the hero this frame (marking home).
    back: bool,
    clock: f32,
    stage: u8,
    shots: u8,
}

impl PortalScript {
    fn walk_secs() -> f32 {
        std::env::var("HW_PORTAL_WALK")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1.2)
    }

    fn walking(&self) -> bool {
        let w = Self::walk_secs();
        (1.0..1.0 + w).contains(&self.clock) || (w + 6.0..w + 7.6).contains(&self.clock)
    }

    /// The power to press now.
    fn press(&mut self) -> Option<hw_net::PowerId> {
        let w = Self::walk_secs();
        match self.stage {
            0 if self.clock >= 0.5 => {
                self.stage = 1;
                self.back = true;
                Some(hw_net::PowerId::GATE_MARK)
            }
            1 if self.clock >= w + 1.8 => {
                self.stage = 2;
                Some(hw_net::PowerId::GATE_OPEN)
            }
            _ => None,
        }
    }

    /// `HW_PORTAL_ORBIT=<degrees>`: while the portal stands open, the
    /// camera swings that far round the hero and pulls back (to see it
    /// from behind).
    fn orbit(&self) -> Option<f32> {
        let w = Self::walk_secs();
        let deg = std::env::var("HW_PORTAL_ORBIT").ok()?.parse().ok()?;
        (w + 4.0..w + 5.8).contains(&self.clock).then_some(deg)
    }

    /// A screenshot to take now: `open.png` (open, seen through),
    /// `orbit.png` (with `HW_PORTAL_ORBIT`), `walking.png` (walking up to
    /// it) and `through.png` (just out at home).
    fn shot(&mut self) -> Option<std::path::PathBuf> {
        let dir = std::path::PathBuf::from(std::env::var("HW_PORTAL_SHOT").ok()?);
        let w = Self::walk_secs();
        const SHOTS: [(f32, &str); 4] = [
            (3.8, "open.png"),
            (5.6, "orbit.png"),
            (6.35, "walking.png"),
            (9.5, "through.png"),
        ];
        let &(at, name) = SHOTS.get(self.shots as usize)?;
        if self.clock < w + at {
            return None;
        }
        self.shots += 1;
        if name == "orbit.png" && std::env::var_os("HW_PORTAL_ORBIT").is_none() {
            return None;
        }
        Some(dir.join(name))
    }

    /// Counts a frame's time in the window it falls in (printed once both
    /// are over).
    fn time_frame(&mut self, _dt: f32) {
        // Wall-clock time: the frame's dt is capped (slow software GPUs).
        let now = std::time::Instant::now();
        let dt = self.last.map_or(0.0, |l| (now - l).as_secs_f32());
        self.last = Some(now);
        let w = Self::walk_secs();
        let c = self.clock;
        if !self.told && c >= w + 5.9 {
            self.told = true;
            let ms = |s: (f32, u32)| s.0 * 1000.0 / s.1.max(1) as f32;
            println!(
                "portal frame time: {:.1} ms before it opens, {:.1} ms with it open",
                ms(self.before),
                ms(self.open)
            );
        }
        let slot = if (w + 1.0..w + 1.8).contains(&c) {
            &mut self.before
        } else if (w + 3.7..w + 5.9).contains(&c) {
            &mut self.open
        } else {
            return;
        };
        slot.0 += dt;
        slot.1 += 1;
    }

    fn done(&self) -> bool {
        self.clock > Self::walk_secs() + 10.5
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

/// Keeps the cabinet of closed cases in the profile on this PC.
fn save_cabinet(cabinet: &[hw_client::missionview::Filed]) {
    let mut p = Profile::load();
    p.cabinet = cabinet.to_vec();
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
    // `HW_PORTAL=1` (screenshots): the hero marks a portal home, walks on,
    // opens a portal and walks through it (see `PortalScript`).
    let mut portal_script = std::env::var_os("HW_PORTAL").map(|_| PortalScript::default());
    // R: the hero runs on by itself until S or R.
    let mut autorun = false;
    let auto_fight = std::env::var_os("HW_AUTOFIGHT").is_some();
    let carry_all = std::env::var_os("HW_CARRY").is_some();
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
    let mut citizens = std::sync::Arc::new(hw_sim::citizens::Citizens::new(
        generated.area.clone(),
        traffic.signals(),
    ));
    // Street performers, odd folk and night life round the camera.
    let mut street = hw_client::streetview::StreetView::new(generated.area.clone());
    // `HW_VIEW=act`: where the act it looks at plays.
    let mut act_spot: Option<(Vec3, DVec2)> = None;
    let mut act_try = 0u64;
    // `HW_SPEECH=1`: speech bubbles round the hero to look at.
    let speech_demo = std::env::var("HW_SPEECH").is_ok_and(|v| v == "1");
    let mut speech_demo_at: Option<Vec3> = None;
    // `HW_VIEW=act` jumps the street acts' clock on (`HW_ACT_SKIP` seconds,
    // 150 by default: the middle of a two-hour slot) so acts are set up
    // in the first frames; play leaves it at 0.
    // `HW_VIEW=crime`: the crime it looks at (where, the way to look from)
    // and tries; `HW_VIEW=getaway`: where crooks are making off.
    let mut crime_spot: Option<(Vec3, Vec3, f64)> = None;
    let mut crime_try = 0u64;
    let mut getaway_at: Option<Vec3> = None;
    let mut act_skip = if view == "act" {
        env_num("HW_ACT_SKIP").unwrap_or(150) as f64
    } else {
        0.0
    };
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
    let mut bus_folk = hw_client::busview::BusView::new();
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
    // HW_SMASH=<frames>: that many frames in, a blast `HW_SMASH_AT` metres
    // (4) in front of the camera breaks what stands there (for screenshots).
    let mut smash_demo = env_num("HW_SMASH");
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
            // The ambulance medic at rescues.
            p.request(Who::Medic, 0);
            // The police's officers.
            for k in 0..hw_sim::police::LOOKS {
                p.request(Who::Officer(k), 0);
            }
            // `HW_VIEW=act`: the street acts' people at once (in play they
            // are asked for when an act first comes near).
            if view == "act" {
                for act in hw_sim::streetlife::Act::ALL {
                    for k in 0..act.performers() {
                        p.request(Who::Street(act, k), 0);
                    }
                }
                for k in 0..hw_sim::streetlife::GUEST_LOOKS {
                    p.request(Who::Guest(k), 0);
                }
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
    // The citizens' looks were asked for above, with the world's seed:
    // never again (a bystander in a crime waits for its look like the rest).
    for look in 0..hw_sim::citizens::LOOKS + hw_sim::citizens::MOLE_LOOKS {
        asked.insert(Who::Citizen(look));
    }
    // The fight: targets, effects, the screen.
    let mut fight = FightView::new();
    let mut robots = RobotView::new();
    let mut safes = hw_client::safeview::SafeView::default();
    let mut police_view = hw_client::policeview::PoliceView::default();
    let mut safe_shown = false;
    let mut radio = hw_client::radioview::Radio::default();
    // The police visor: marks the radio's calls in front of the hero's eyes.
    let mut visor = hw_client::visor::Visor::from_env();
    // `HW_HOLD=1` (screenshots): the help key is held down all along.
    let hold_help = std::env::var_os("HW_HOLD").is_some();
    let mut signal = hw_client::dispatchview::Signal::default();
    let mut story = hw_client::storyview::StoryView::default();
    let mut cases = hw_client::missionview::MissionView::default();
    // The cabinet of closed cases lives in the profile on this PC.
    cases.filed = profile.cabinet.clone();
    let mut cabinet_saved = cases.filed.clone();
    let mut case_file = hw_client::casefile::CaseFile::default();
    // `HW_CASEFILE=case|doctor|closed`: the case file open on that tab
    // (screenshots).
    if let Ok(tab) = std::env::var("HW_CASEFILE") {
        case_file.toggle();
        case_file.tab = match tab.as_str() {
            "doctor" => hw_client::casefile::Tab::Doctor,
            "closed" => hw_client::casefile::Tab::Closed,
            _ => hw_client::casefile::Tab::Case,
        };
    }
    // The chat window (bottom left): what is said near the hero.
    let mut chat = hw_client::chat::ChatLog::new();
    // The pointer was locked when the chat line opened: lock it again after.
    let mut chat_relock = false;
    // Always-on mouselook: when the pointer is pinned and when it is freed.
    let mut mouselook = hw_client::mouselook::Mouselook::default();
    // What was drawn last frame (for targeting), and a power key pressed
    // but not yet sent with a tick.
    let mut drawn: Vec<hw_net::EntityState> = Vec::new();
    let mut pending_power = 0u8;
    // The left button went down on a villain out of punching reach (it
    // picked it): no punching until it is let go.
    let mut click_picked = false;
    // Karma, levels and powers: the hero file from this PC (None until a new
    // hero picks an origin), the sheet as the host last confirmed it, and
    // the powers window.
    let mut powers = PowersView::new();
    // `HW_POWERS_WINDOW=1` opens the powers window at start (screenshots).
    powers.open = std::env::var_os("HW_POWERS_WINDOW").is_some();
    // Until when our hero strikes the level-up power pose.
    let mut power_pose_until = f32::NEG_INFINITY;
    // The hand goes to the temple to switch the police visor until then.
    let mut visor_tap_until = f32::NEG_INFINITY;
    // The police computer (Y, or the monitor in the HUD), and a visor
    // switch asked of it.
    let mut police = hw_client::police::Police::default();
    let mut visor_asked = false;
    // `HW_POLICE=desktop|crimes`: the police computer open (screenshots).
    match std::env::var("HW_POLICE").as_deref() {
        Ok("desktop") => police.desktop = true,
        Ok("crimes") => police.crimes = true,
        _ => {}
    }
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
        // HW_ORIGIN and HW_ELEMENT pick those; HW_POWERS=1 buys the whole
        // tree (the first of each fork).
        if let Some(o) = env_num("HW_ORIGIN") {
            let element = hw_sim::powers::Element::from_code(s.element);
            let karma = s.karma;
            s = hw_sim::progress::sanitize(hw_net::Sheet {
                origin: o.min(4) as u8,
                ..s
            });
            hw_sim::progress::refund_all(&mut s);
            s.element = element.code();
            s.karma = s.karma.max(karma);
        }
        if let Some(e) = env_num("HW_ELEMENT") {
            s.element = hw_sim::powers::Element::from_code(e.min(6) as u8).code();
        }
        if std::env::var_os("HW_POWERS").is_some() {
            s = hw_client::debugmenu::buy_all(s);
        }
        // HW_AUGMENTS=1: a bag of augments of every grade, and a few set
        // into the hero's powers (screenshots, trying them out).
        if std::env::var_os("HW_AUGMENTS").is_some() {
            hw_client::debugmenu::give_augments(&mut s, level.clamp(1, 20));
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
    // Sidekicks' little tricks while their heroes stand about.
    let mut tricks: BTreeMap<hw_core::NetId, hw_client::tricks::Tricks> = BTreeMap::new();
    let force_trick: Option<&'static str> = std::env::var("HW_TRICK").ok().and_then(|t| {
        hw_client::tricks::TRICKS
            .iter()
            .find(|k| k.0 == t)
            .map(|k| k.0)
    });
    let mut town = TownView::new(generated.world.clone());
    // Doors near the camera, swinging as the server opens and shuts them.
    let mut door_view = hw_client::doorview::DoorView::new();
    // The minimap (N) and the town's sounds and music.
    let mut minimap = hw_client::minimap::Minimap::new(generated.world.clone());
    // The town map (M).
    let mut town_map = hw_client::townmap::TownMap::new(generated.world.clone());
    town_map.open = view == "map";
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
    if portal_script.is_some() {
        // Down the street, where there is room for a portal.
        hero_cam_yaw = spawn_dir.x.atan2(spawn_dir.y).to_degrees() as f32;
    }
    let mut fly = match view.as_str() {
        "street" | "citizen" | "alpha" | "riders" | "act" | "safe" => {
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
        "graveyard" => {
            // `HW_VIEW=graveyard`: outside the nearest graveyard's gate,
            // looking in down its avenue (`HW_GRAVE_BACK` metres back,
            // `HW_GRAVE_UP` up; `HW_GRAVE_N` picks the n-th nearest).
            use hw_gen::urban::props::PropKind;
            let n = env_num("HW_GRAVE_N").unwrap_or(0) as usize;
            let back = env_num("HW_GRAVE_BACK").unwrap_or(14) as f64;
            let up = env_num("HW_GRAVE_UP").unwrap_or(7) as f64;
            let mut gates: Vec<_> = generated
                .area
                .props
                .iter()
                .filter(|p| p.kind == PropKind::GraveGate)
                .collect();
            gates.sort_by(|a, b| a.pos.distance(spawn).total_cmp(&b.pos.distance(spawn)));
            let (c, front) = gates
                .get(n.min(gates.len().saturating_sub(1)))
                .map_or((spawn, spawn_dir), |g| (g.pos, g.dir));
            println!(
                "graveyard view: gate at {c:?} ({} gates in the area)",
                gates.len()
            );
            let p = c + front * back;
            let d = c - front * 25.0 - p;
            let yaw = d.x.atan2(d.y).to_degrees() as f32;
            let pitch = -(up / back.max(1.0)).atan().to_degrees() as f32 * 0.6;
            FlyCamera::new(
                (p.x as f32, ground(p) as f32 + up as f32, p.y as f32),
                yaw,
                pitch,
            )
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
        "comet" | "cometlobby" | "cometgallery" | "cometstairs" | "comethall" | "cometlift" => {
            // A Comet building (`HW_COMET=0|1|2`: the atrium, the grand
            // hall, the split-level one): from across its plaza, in at
            // its entrance, on its highest gallery, at its grand stair, in
            // its biggest office hall.
            use hw_gen::urban::landmarks::LandmarkKind;
            let want: usize = std::env::var("HW_COMET")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let all: Vec<_> = generated
                .world
                .landmarks
                .iter()
                .filter(|l| l.kind == LandmarkKind::CometBuilding)
                .collect();
            let l = all
                .iter()
                .find(|l| l.variant as usize % 3 == want)
                .or(all.first());
            let which = view.trim_start_matches("comet");
            match l.and_then(|l| hw_client::cometmesh::view(l, which)) {
                Some((eye, at)) => {
                    let d = at - eye;
                    let yaw = d.x.atan2(d.z).to_degrees() as f32;
                    let pitch = (d.y / d.x.hypot(d.z)).atan().to_degrees() as f32;
                    FlyCamera::new((eye.x as f32, eye.y as f32, eye.z as f32), yaw, pitch)
                }
                None => FlyCamera::new((spawn.x as f32, 40.0, spawn.y as f32), 0.0, -20.0),
            }
        }
        "halls" | "hallslobby" | "hallsgallery" | "hallsstairs" | "hallshall" | "hallslift" => {
            // A town office building of halls, the `HW_HALLS`-th nearest
            // the start (0 by default): the same views as a Comet building.
            let nth: usize = std::env::var("HW_HALLS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let m = DVec2::splat(4000.0);
            let mut near: Vec<(f64, hw_gen::urban::world::WorldBuilding)> = generated
                .world
                .quarters_in(spawn - m, spawn + m)
                .iter()
                .flat_map(|q| q.buildings.clone())
                .filter(|b| b.site.halls.is_some())
                // (`HW_HALLS_USE=Lab`: only buildings of that use.)
                .filter(|b| {
                    std::env::var("HW_HALLS_USE")
                        .map_or(true, |want| format!("{:?}", b.site.usage) == want)
                })
                // (`HW_HALLS_HIDEOUT=1`: only gang hideouts.)
                .filter(|b| std::env::var("HW_HALLS_HIDEOUT").is_err() || b.site.is_hideout())
                // (`HW_HALLS_CLUB=1`: only night clubs.)
                .filter(|b| std::env::var("HW_HALLS_CLUB").is_err() || b.site.is_club())
                // (`HW_HALLS_SCHEME=Block`: only buildings of that scheme.)
                .filter(|b| {
                    std::env::var("HW_HALLS_SCHEME").map_or(true, |want| {
                        b.site
                            .layout()
                            .is_some_and(|l| format!("{:?}", l.scheme) == want)
                    })
                })
                .map(|b| (b.site.origin.distance(spawn), b))
                .collect();
            near.sort_by(|a, b| a.0.total_cmp(&b.0));
            let which = view.trim_start_matches("halls");
            if let Some((_, b)) = near.get(nth.min(near.len().saturating_sub(1))) {
                let l = b.site.layout();
                eprintln!(
                    "halls view: {:?} {} x {} storeys, origin {:.1} along {:.2} base {:.1}, {:?}",
                    b.site.usage,
                    b.site.size,
                    b.site.floors,
                    b.site.origin,
                    b.site.along,
                    b.base,
                    l.as_ref().map(|l| l.scheme)
                );
                // (A den's hall and its levels, to place `HW_HALLS_EYE`.)
                if let Some(l) = l
                    .as_ref()
                    .filter(|l| l.scheme == hw_gen::layout::Scheme::Den)
                {
                    let r = l.spaces[0].rect;
                    eprintln!(
                        "  den hall z {:.1}..{:.1}, levels {:?}",
                        r.min.y,
                        r.max.y,
                        l.levels()
                    );
                }
            }
            // `HW_HALLS_EYE=x,y,z,x,y,z`: the eye and what it looks at, in
            // the building's own (x, z) and height over its ground.
            let own: Option<Vec<f64>> = std::env::var("HW_HALLS_EYE")
                .ok()
                .map(|v| v.split(',').filter_map(|t| t.trim().parse().ok()).collect());
            let chosen = near.get(nth.min(near.len().saturating_sub(1)));
            let pair = match (chosen, own.as_deref()) {
                (Some((_, b)), Some(&[x, y, z, tx, ty, tz])) => {
                    let p = |x, y, z| b.site.to_city(DVec3::new(x, b.base + y, z));
                    Some((p(x, y, z), p(tx, ty, tz)))
                }
                (Some((_, b)), _) => hw_client::cometmesh::halls_view(b, which),
                _ => None,
            };
            match pair {
                Some((eye, at)) => {
                    let d = at - eye;
                    let yaw = d.x.atan2(d.z).to_degrees() as f32;
                    let pitch = (d.y / d.x.hypot(d.z)).atan().to_degrees() as f32;
                    FlyCamera::new((eye.x as f32, eye.y as f32, eye.z as f32), yaw, pitch)
                }
                None => FlyCamera::new((spawn.x as f32, 40.0, spawn.y as f32), 0.0, -20.0),
            }
        }
        "hospital" | "ward" | "pods" | "hospitaldoor" => {
            // The hospital nearest the start: from across its plaza, inside
            // its Recovery Hall (`ward` from the door end, `pods` close at
            // the recovery pods), or at its entrance.
            use hw_gen::urban::landmarks::LandmarkKind;
            let l =
                hw_client::landmarkview::nearest(&generated.world, LandmarkKind::Hospital, spawn);
            match l {
                Some(l) => {
                    let (from, up, at, at_y) = match view.as_str() {
                        "ward" => (DVec2::new(-9.0, -1.0), 2.4, DVec2::new(4.0, -10.5), 1.4),
                        "pods" => (DVec2::new(-1.5, -7.2), 1.7, DVec2::new(-6.0, -11.8), 1.3),
                        "hospitaldoor" => (DVec2::new(5.0, 14.0), 2.0, DVec2::new(0.0, 1.0), 3.0),
                        _ => (DVec2::new(26.0, 44.0), 14.0, DVec2::new(0.0, -8.0), 10.0),
                    };
                    let p = l.to_plan(from);
                    let t = l.to_plan(at);
                    let d = t - p;
                    let yaw = d.x.atan2(d.y).to_degrees() as f32;
                    let eye = l.base + up;
                    let pitch = ((l.base + at_y - eye) / d.length()).atan().to_degrees() as f32;
                    FlyCamera::new((p.x as f32, eye as f32, p.y as f32), yaw, pitch)
                }
                None => FlyCamera::new((spawn.x as f32, 40.0, spawn.y as f32), 0.0, -20.0),
            }
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
        "lift" | "stairs" | "stairtop" | "stairfoot" | "tunnel" | "molehaven" | "molegate"
        | "moleramp" | "moleroom" | "molesign" => {
            // The underground: a stair pavilion on the street (`stairs`:
            // part way down the spiral), a utility tunnel from inside, or
            // Molehaven's great hall.
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
                ("stairs", Some(e), _, _) => {
                    // A few steps down, looking along the stair.
                    let route = e.spiral.route();
                    let k = 9.min(route.len() - 1);
                    let (a, b) = (route[k], route[(k + 3).min(route.len() - 1)]);
                    let d = b - a;
                    let yaw = d.x.atan2(d.z).to_degrees() as f32;
                    FlyCamera::new((a.x as f32, a.y as f32 + 1.6, a.z as f32), yaw, -30.0)
                }
                ("stairtop", Some(e), _, _) => {
                    // At the parapet, looking down the stairwell.
                    let p = e.pos + e.door * 2.6;
                    let d = e.pos - p;
                    let yaw = d.x.atan2(d.y).to_degrees() as f32;
                    FlyCamera::new((p.x as f32, e.top as f32 + 1.7, p.y as f32), yaw, -55.0)
                }
                ("stairfoot", Some(e), _, _) => {
                    // Where the stair comes down, looking at its foot.
                    // From the passage the shaft opens into, a few metres out.
                    let side = e.door.perp();
                    let a = [e.door, -e.door, side, -side]
                        .into_iter()
                        .flat_map(|d| [4.5, 3.5].map(|k| e.pos + d * k))
                        .find(|p| {
                            u.at(*p).any(|(_, v)| {
                                v.kind != hw_gen::urban::underground::VoidKind::Shaft
                                    && (v.floor_at(*p) - e.bottom).abs() < 0.6
                            })
                        })
                        .unwrap_or(e.pos + e.door * 3.5);
                    let d = e.pos - a;
                    let yaw = d.x.atan2(d.y).to_degrees() as f32;
                    FlyCamera::new((a.x as f32, e.bottom as f32 + 1.7, a.y as f32), yaw, -18.0)
                }
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
                ("molegate", _, _, _) if u.entrances.iter().any(|e| e.molehaven) => {
                    // Molehaven's gate from the street, a few steps off.
                    let e = u.entrances.iter().find(|e| e.molehaven).expect("gate");
                    let p = e.pos + e.door * 10.0 + e.door.perp() * 3.5;
                    let d = e.pos - p;
                    let yaw = d.x.atan2(d.y).to_degrees() as f32;
                    FlyCamera::new((p.x as f32, e.top as f32 + 2.2, p.y as f32), yaw, 8.0)
                }
                ("molesign", _, _, _) if !u.signs.is_empty() => {
                    // The signpost nearest the gate, from beside it.
                    let sg = &u.signs[0];
                    let p = sg.pos - sg.point * 3.0 + sg.point.perp() * 4.0;
                    let d = sg.pos - p;
                    let yaw = d.x.atan2(d.y).to_degrees() as f32;
                    let g = generated.world.ground(p);
                    FlyCamera::new((p.x as f32, g as f32 + 2.0, p.y as f32), yaw, 8.0)
                }
                ("moleramp", _, _, Some(c)) if c.ramp_sign.is_some() => {
                    // In the tunnel, looking at the sign over the ramp down.
                    let rs = c.ramp_sign.expect("sign");
                    let p = rs.at - rs.face * 7.0;
                    let d = rs.at - p;
                    let yaw = d.x.atan2(d.y).to_degrees() as f32;
                    FlyCamera::new((p.x as f32, rs.bottom as f32 - 0.9, p.y as f32), yaw, 4.0)
                }
                ("moleroom", _, _, Some(c)) if c.chambers.len() > 1 => {
                    // Into a side chamber from the hall (`HW_ROOM=n`: the
                    // n-th, 1 by default; `HW_ROOM_HIGH=1` looks down on it).
                    let n = std::env::var("HW_ROOM")
                        .ok()
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(1)
                        .clamp(1, c.chambers.len() - 1);
                    let high = std::env::var_os("HW_ROOM_HIGH").is_some();
                    let hall = &u.voids[c.chambers[0]];
                    let ch = &u.voids[c.chambers[n]];
                    let mid = ch.world((ch.min + ch.max) * 0.5);
                    let back = if high { 4.0 } else { 11.0 };
                    let p = mid + (hall.origin - mid).normalize_or(DVec2::X) * back;
                    let d = mid - p;
                    let yaw = d.x.atan2(d.y).to_degrees() as f32;
                    let (h, pitch) = if high { (7.5, -62.0) } else { (4.6, -18.0) };
                    FlyCamera::new((p.x as f32, c.floor as f32 + h, p.y as f32), yaw, pitch)
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
        "station" | "tubeline" | "ruin" | "lab" | "grotto" | "kiosk" | "tube" | "capsule"
        | "homes" | "tubemarket" | "workshop" | "shrine" | "burrow" | "exit" | "exitstreet" => {
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
    // `HW_VIEW=room HW_ROOM=flat|shop|office|lobby|kitchen|bedroom|living`:
    // the free camera looking into the nearest room of that kind, its
    // floor cut open (furniture close-ups).
    let room_view = (view == "room")
        .then(|| std::env::var("HW_ROOM").ok())
        .flatten()
        .and_then(|r| {
            use hw_gen::building::RoomKind as K;
            let kind = match r.as_str() {
                "flat" => K::Apartment,
                "shop" => K::Shop,
                "office" => K::Office,
                "lobby" => K::Lobby,
                "kitchen" => K::Kitchen,
                "bedroom" => K::Bedroom,
                "living" => K::Living,
                "corridor" => K::Corridor,
                _ => return None,
            };
            // `HW_ROOM_SKIP=n` skips the n nearest such rooms' buildings.
            let skip = env_num("HW_ROOM_SKIP").unwrap_or(0) as usize;
            let mut from = spawn;
            let mut found = None;
            for _ in 0..=skip {
                found = town.find_room(from, 2500.0, kind);
                let (i, ..) = found?;
                // The next search starts just past this building.
                from = town.centre_of(i)? + DVec2::new(37.0, 23.0);
            }
            found
        });
    if let Some((i, level, k)) = room_view {
        if let Some(plan) = town.plan(i) {
            let base = generated
                .world
                .building(hw_gen::urban::quarter::DetailId(i))
                .map_or(0.0, |b| b.base);
            let f = &plan.floors[level];
            let room = &f.rooms[k];
            let c = room.rect.center();
            let y = base + f.elevation;
            // The free camera over the front wall, looking down into the
            // room; the floor is cut open there (`HW_ROOM_YAW` turns it,
            // `HW_ROOM_UP` raises it).
            let up = std::env::var("HW_ROOM_UP")
                .ok()
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(5.5);
            let turn = std::env::var("HW_ROOM_YAW")
                .ok()
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0)
                .to_radians();
            let back = DVec2::new(-turn.sin(), -turn.cos()) * (room.rect.size().y * 0.5 + 1.5);
            let eye = plan
                .site
                .to_city(glam::DVec3::new(c.x + back.x, y + up, c.y + back.y));
            let look = plan.site.to_city(glam::DVec3::new(c.x, y, c.y));
            let d = look - eye;
            let yaw = d.x.atan2(d.z).to_degrees() as f32;
            let pitch = (d.y.atan2(DVec2::new(d.x, d.z).length())).to_degrees() as f32;
            fly = FlyCamera::new((eye.x as f32, eye.y as f32, eye.z as f32), yaw, pitch);
            flying = true;
            town.cut_on = true;
            let fc = plan.site.to_city(glam::DVec3::new(c.x, 0.0, c.y));
            town.cut_focus = DVec2::new(fc.x, fc.z);
            town.cut_level = level as u32;
            println!(
                "room view: building {i}, floor {level}, room {k} ({:?})",
                room.kind
            );
        }
    } else if view == "door" {
        // `HW_VIEW=door HW_DOOR=front|glass|inner [HW_DOOR_OPEN=1]
        // [HW_DOOR_SKIP=n]`: the free camera in front of the nearest door
        // of that kind (inner ones with their floor cut open).
        use hw_gen::building::RoomKind as K;
        use hw_gen::doors::DoorKind;
        let want = std::env::var("HW_DOOR").unwrap_or_else(|_| "front".into());
        let (kind, rooms): (DoorKind, &[K]) = match want.as_str() {
            "glass" => (DoorKind::Glass, &[K::Shop, K::Lobby]),
            "inner" => (DoorKind::Inner, &[K::Office, K::Apartment, K::Living]),
            _ => (
                DoorKind::Front,
                &[K::Living, K::Apartment, K::Shop, K::Kitchen],
            ),
        };
        let skip = env_num("HW_DOOR_SKIP").unwrap_or(0) as usize;
        let mut seen = 0;
        let mut found = None;
        'look: for &rk in rooms {
            let mut from = spawn;
            for _ in 0..12 {
                let Some((i, ..)) = town.find_room(from, 2500.0, rk) else {
                    break;
                };
                from = town.centre_of(i).unwrap_or(from) + DVec2::new(37.0, 23.0);
                let Some(plan) = town.plan(i) else { continue };
                if let Some(d) = plan.door_leaves().into_iter().find(|d| d.kind == kind) {
                    if seen == skip {
                        found = Some((i, plan, d));
                        break 'look;
                    }
                    seen += 1;
                }
            }
        }
        if let Some((i, plan, d)) = found {
            let base = generated
                .world
                .building(hw_gen::urban::quarter::DetailId(i))
                .map_or(0.0, |b| b.base);
            let k = plan
                .floors
                .iter()
                .position(|f| f.level == d.level)
                .unwrap_or(0);
            let y = base + plan.floors[k].elevation;
            // From the side the door opens away from (the street, or the
            // room it does not swing into), a little off centre.
            let (back, up) = if d.kind == DoorKind::Inner {
                (3.2, 3.2)
            } else {
                (4.2, 1.7)
            };
            let eye_b = d.point(0.9, -back);
            let eye = plan
                .site
                .to_city(glam::DVec3::new(eye_b.x, y + up, eye_b.y));
            let look = plan.site.to_city(glam::DVec3::new(d.at.x, y + 1.3, d.at.y));
            let dir = look - eye;
            let yaw = dir.x.atan2(dir.z).to_degrees() as f32;
            let pitch = (dir.y.atan2(DVec2::new(dir.x, dir.z).length())).to_degrees() as f32;
            fly = FlyCamera::new((eye.x as f32, eye.y as f32, eye.z as f32), yaw, pitch);
            flying = true;
            if d.kind == DoorKind::Inner {
                town.cut_on = true;
                let fc = plan.site.to_city(glam::DVec3::new(d.at.x, 0.0, d.at.y));
                town.cut_focus = DVec2::new(fc.x, fc.z);
                town.cut_level = d.level;
            }
            if std::env::var_os("HW_DOOR_OPEN").is_some() {
                generated.obstacles.set_door(i, d.key, true);
            }
            println!("door view: building {i}, door {} ({:?})", d.key, d.kind);
        } else {
            println!("door view: no {want} door near the start");
        }
    } else if view == "inside" {
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
    // The comic look for the game's windows and HUD, set on the first
    // frame of play (the title keeps its own).
    let mut ui_styled = false;
    let mut still_ui = profile.reduced_motion;
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
        view_yaw: hero_cam_yaw,
    };
    // `HW_CAM_DIST` and `HW_CAM_PITCH` set the hero camera (close-up screenshots),
    // `HW_CAM_TURN` turns it (degrees, to the left).
    let cam_env = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<f32>().ok());
    if let Some(t) = cam_env("HW_CAM_TURN") {
        hero_cam.yaw += t;
    }
    if let Some(d) = cam_env("HW_CAM_DIST") {
        hero_cam.distance = d.clamp(0.0, HeroCamera::MAX_DISTANCE);
    }
    if let Some(p) = cam_env("HW_CAM_PITCH") {
        hero_cam.pitch = p.clamp(-80.0, 80.0);
    }
    // `HW_CAM_YAW`: turned round the hero by that many degrees (its face).
    if let Some(y) = cam_env("HW_CAM_YAW") {
        hero_cam.yaw += y;
    }
    // Each hero's entity, and whether it is the Actors character yet.
    let mut hero_entities: BTreeMap<hw_core::NetId, (EntityId, bool)> = BTreeMap::new();
    // Taxi canopies: the hinged part, and how far open (0 shut, 1 up).
    let mut canopies: BTreeMap<hw_core::NetId, (EntityId, f32)> = BTreeMap::new();
    let mut tick_clock = hw_sim::hostthread::TickClock::default();
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
    // From the title down to the hero: waits for the hero's surroundings,
    // then flies an arc (see `startflight.rs`).
    let mut swoop: Option<hw_client::startflight::StartFlight> = None;
    let mut flight_frames = 0u32;
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
    // Screenshots keep full detail (no frame-rate tuner).
    let mut startup = profile.clone();
    startup.smooth &= !shooting;
    let startup = hw_client::settings::startup(&startup);
    // `HW_HUD=0`: no HUD, menus or labels on screen (clean shots for the
    // website and the press; the game runs as usual underneath).
    let hide_hud = std::env::var("HW_HUD").is_ok_and(|v| v == "0");
    let run = Engine::run_with_settings(
        format!("HeroWorld {}", hw_core::version()),
        startup,
        move |world, frame| {
            // Notices late frames and writes what went on (see `hitch.rs`).
            let _hitch = hw_client::hitch::begin(world, frame.time);
            if hide_hud {
                // Every layer egui knows of is moved far off screen.
                use engine::ui::egui::{emath::TSTransform, vec2};
                let ctx = frame.ui.ctx();
                let layers: Vec<_> = ctx.memory(|m| m.layer_ids().collect());
                for l in layers {
                    ctx.set_transform_layer(l, TSTransform::from_translation(vec2(1.0e6, 1.0e6)));
                }
            }
            quality_log.note(world, t0.elapsed().as_secs_f32());
            if frame.first {
                hw_client::hitch::mark("first frame");
                hw_client::hitch::line(format!(
                    "first frame, {:.1} s after the start (the town was planned before the window opened)",
                    t0.elapsed().as_secs_f64()
                ));
                // The power effects' WGSL materials (bubble film, ice):
                // added now so Engine builds their pipelines at the start.
                hw_client::powerfx::install_materials(world)?;
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
                    town_map = hw_client::townmap::TownMap::new(generated.world.clone());
                    sounds.stop(world);
                    traffic = hw_sim::traffic::Traffic::new(generated.area.clone());
                    citizens = std::sync::Arc::new(hw_sim::citizens::Citizens::new(
                        generated.area.clone(),
                        traffic.signals(),
                    ));
                    street.clear(world);
                    street = hw_client::streetview::StreetView::new(generated.area.clone());
                    citizens.set_hour(game_hour() as f64);
                    stands = hw_sim::taxi::stands(&generated.area);
                    area_job = None;
                    area_held.clear();
                    traffic_view.clear(world);
                    radio.clear(world);
                    signal.clear(world);
                    story.clear(world);
                    cases.clear(world);
                    riders.clear(world, &crowd);
                    bus_folk.clear(world, &crowd);
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
                // While a plan runs, the bar is the plan's own progress and
                // its stage; the town's cells count once it is there.
                let planning = builds.progress();
                let what = match (building, planning) {
                    (Some(c), Some((_, stage))) if !stage.is_empty() => format!(
                        "Building the {}: {}...",
                        hw_client::citybuild::size_of(c.people).name.to_lowercase(),
                        stage.to_lowercase()
                    ),
                    (Some(c), _) => format!(
                        "Building the {}...",
                        hw_client::citybuild::size_of(c.people).name.to_lowercase()
                    ),
                    (None, _) if wanted == 0 => String::new(),
                    (None, _) => "Building the city...".into(),
                };
                // Planning the city is the first PLAN_SHARE of the bar and
                // drawing its cells the rest, so it never jumps back.
                const PLAN_SHARE: f32 = 0.8;
                let cells = if wanted == 0 {
                    1.0
                } else {
                    built as f32 / wanted as f32
                };
                let done = if builds.busy() {
                    // Waiting to start (an older plan still stopping) shows
                    // nothing yet.
                    planning.map_or(0.0, |(f, _)| f * PLAN_SHARE)
                } else if wanted == 0 {
                    1.0
                } else {
                    PLAN_SHARE + (1.0 - PLAN_SHARE) * cells
                };
                t.set_world_ready(extras_loaded && !builds.busy(), cells);
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
                    swoop = Some(hw_client::startflight::StartFlight::new(eye, yaw, pitch));
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
                    still_ui = t.profile.reduced_motion;
                    t.leave(world);
                    // The game goes on drawing cells on the workers.
                    hw_client::hitch::event("left the title");
                    title_screen = None;
                }
                return Ok(());
            }
            hw_client::hitch::mark("menus");
            if !ui_styled {
                let ctx = frame.ui.ctx();
                if !ctx.fonts(|f| {
                    f.families()
                        .contains(&engine::ui::egui::FontFamily::Name("hw-ui".into()))
                }) {
                    // Started straight into the street: the fonts arrive
                    // with the next frame, the style after them.
                    hw_client::title::install_fonts(ctx);
                } else {
                    hw_client::uikit::install(ctx);
                    hw_client::uikit::set_reduced_motion(ctx, still_ui);
                    ui_styled = true;
                }
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
            // Enter opens the chat line, Escape closes it. Before the menu,
            // so the Escape that closes the line opens no menu; and the
            // Engine is told it was handled, so it does not quit either.
            let menus_open = menu.open || debug.open || powers.open || hero_file.is_none();
            if chat.keys(frame.ui.ctx(), menus_open) {
                world.set_bind_listen(true);
            }
            // While typing the keys are the chat's: a locked pointer would let
            // them through to the hero (Engine only holds them back from a
            // free pointer), so it is freed until the line closes.
            if chat.typing && world.pointer_lock() {
                world.set_pointer_lock(false);
                chat_relock = true;
            } else if !chat.typing && chat_relock {
                world.set_pointer_lock(true);
                chat_relock = false;
            }
            // Escape: the menu (settings, controls, quit).
            menu.hosting = open_at.is_some() || session.room_code().is_some();
            match menu.show(&frame.ui, world) {
                hw_client::settings::MenuExit::Stay => {}
                hw_client::settings::MenuExit::Quit => world.request_exit(),
                hw_client::settings::MenuExit::Title => {
                    // Everything the hero owns goes to disk first. The
                    // window then closes, which stops every worker and the
                    // server (friends are told), and `main` starts the game
                    // again from its title screen: a fresh process is the
                    // one teardown that cannot leak or race.
                    if sheet_dirty && !cheated {
                        save_hero(hero_file.clone());
                    }
                    save_tray(&powers.tray);
                    save_cabinet(&cases.filed);
                    hw_client::hitch::event("back to the title screen");
                    RESTART_AT_TITLE.store(true, std::sync::atomic::Ordering::SeqCst);
                    world.request_exit();
                }
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
                            if p.fly && !my_sheet.has(hw_net::PowerId(5)) {
                                fight.say("Flight first: Hero tab, Buy the whole tree");
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
                        A::BuyAll => {
                            cheated = true;
                            session.debug_sheet(dm::buy_all);
                        }
                        A::Element(e) => {
                            cheated = true;
                            session.debug_sheet(move |mut s| {
                                s.element = e.code();
                                s
                            });
                        }
                        A::ClearPowers => {
                            cheated = true;
                            session.debug_sheet(dm::clear_powers);
                        }
                        A::Augments => {
                            cheated = true;
                            session.debug_sheet(|mut s| {
                                let level = hw_sim::progress::level_of(s.lifetime);
                                dm::give_augments(&mut s, level);
                                s
                            });
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
                        A::Map => town_map.toggle(radio.hero_at),
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
            let hour = hour_now(session.time(tick_clock.frac()));
            if swoop.is_none() {
                hw_client::lighting::set_sky(world, hour);
            }
            citizens.set_hour(hour as f64);
            town.set_hour(world, hour);
            if let (Some(lights), Some(props)) = (extras.lights.as_mut(), extras.props.as_ref()) {
                let dark = swoop.is_none() && hw_client::nightlights::dark_at(hour);
                let eye = world.camera().eye();
                let rooms = if dark {
                    town.room_lamps(glam::DVec2::new(eye.x as f64, eye.z as f64), hour)
                } else {
                    (0, &[][..])
                };
                lights.update(world, eye, dark, props, rooms)?;
            }
            if let Some(lights) = extras.comet_lights.as_mut() {
                let eye = world.camera().eye();
                lights.update(world, &town.world, eye)?;
            }
            {
                let eye = world.camera().eye();
                extras.hall_furniture.update(world, &town.world, eye)?;
                extras
                    .hall_insides
                    .update(world, &town.world, eye, &|i| town.is_hidden(i));
            }
            if let Some(lifts) = extras.lifts.as_mut() {
                let now = session.time(tick_clock.frac());
                lifts.update(world, world.camera().eye(), now)?;
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
                m.update(world, session.time(tick_clock.frac()), frame.dt)?;
            }
            hw_client::hitch::mark("rocket port");
            if let Some(r) = extras.rocket.as_mut() {
                r.update(world, session.time(tick_clock.frac()), frame.dt)?;
            }
            hw_client::hitch::mark("atom");
            if let Some(a) = extras.atom.as_mut() {
                a.update(world, frame.dt)?;
            }
            hw_client::hitch::mark("drill rig");
            if let Some(d) = extras.drill.as_mut() {
                d.update(world, session.time(tick_clock.frac()))?;
            }
            hw_client::hitch::mark("weather tower");
            if let Some(w) = extras.weather.as_mut() {
                let eye = world.camera().eye();
                let sheltered = town.inside.is_some()
                    || hw_client::soundscape::under(&generated.world, eye)
                        != hw_client::soundscape::Under::No;
                w.update(
                    world,
                    session.time(tick_clock.frac()),
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
            if let Some(n) = smash_demo.as_mut() {
                if *n == 0 {
                    let cam = world.camera();
                    let reach = env_num("HW_SMASH_AT").unwrap_or(4) as f32;
                    let dir = (cam.target() - cam.eye()).normalize_or_zero();
                    let at = (cam.eye() + dir * reach).as_dvec3();
                    eprintln!("smashed {} elements", session.smash(at, 3.0, 4000.0));
                    smash_demo = None;
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
            // The debug log: every key the game got, and any the window got
            // that the game did not (the UI kept them).
            if let Some(k) = input.last_key_down() {
                hw_client::devlog::add(format!("key {k:?}"));
            } else {
                let ctx = frame.ui.ctx();
                let kept: Vec<String> = ctx.input(|i| {
                    i.events
                        .iter()
                        .filter_map(|e| match e {
                            engine::ui::egui::Event::Key {
                                key,
                                pressed: true,
                                repeat: false,
                                ..
                            } => Some(format!("{key:?}")),
                            _ => None,
                        })
                        .collect()
                });
                if !kept.is_empty() {
                    hw_client::devlog::add(format!(
                        "keys {kept:?} reached the window but not the game (UI wants keyboard {}, pointer {})",
                        ctx.wants_keyboard_input(),
                        ctx.wants_pointer_input()
                    ));
                }
            }
            // Hero input, relative to where the camera faces.
            let mut hero_input = hw_net::InputState::default();
            let my_kit = hw_sim::combat::Kit::of(&my_sheet);
            let can_fly = my_kit.travels(hw_sim::powers::travel::FLY);
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
                        fight.say("Flight is in the travel tree (B: powers)");
                    }
                }
                if input.pressed(Key::R) && !creating && !on_ui_keys(frame) {
                    autorun = !autorun;
                } else if input.pressed(Key::S) {
                    autorun = false;
                }
                let mut dir = input.move_dir_xz(hero_cam.yaw);
                if let Some(s) = portal_script.as_mut() {
                    if session.my_hero().is_some() {
                        s.clock += frame.dt;
                        s.time_frame(frame.dt);
                    }
                    if s.walking() {
                        dir = Camera::facing_xz(hero_cam.yaw);
                    }
                }
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
                let click_ray = world.cursor_ray(frame);
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
                        click_ray: (!on_ui).then_some(click_ray),
                    },
                );
                click_picked = (click_picked || f.picked) && input.mouse_down(MouseButton::Left);
                if click_picked {
                    hero_input.smash = false;
                }
                if f.power != 0 && !(on_ui && f.power == hw_net::PowerId::STRIKE.0) {
                    pending_power = f.power;
                }
                // HW_CARRY=1: every villain carries an augment (screenshots
                // of the gems and the drops).
                if carry_all {
                    session.debug_carry();
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
                    // Every keyed power the hero has, in turn (the blast
                    // between them); punches up close.
                    let blast = my_kit.blast.map_or(1, |b| b.0);
                    let every: Vec<u8> = my_kit
                        .owned()
                        .filter(|&p| !hw_sim::powers::is_free(p) && hw_client::tray::keyed(p))
                        // Toggles would only flip on and off.
                        .filter(|&p| !hw_sim::powers::def_of(p).toggle)
                        .flat_map(|p| [blast, p.0])
                        .collect();
                    let turn = if !auto_turn.is_empty() {
                        &auto_turn[..]
                    } else if every.is_empty() {
                        &[1u8][..]
                    } else {
                        &every[..]
                    };
                    let k = (frame.time / 0.75) as usize % turn.len();
                    let want = Some(hw_net::PowerId(turn[k])).filter(|p| my_sheet.has(*p));
                    pending_power = match want {
                        Some(p) if !(close && p.0 == blast) => p.0,
                        _ if close => 1,
                        _ => blast,
                    };
                    // With `HW_TRICK`, the sidekick is called once and left
                    // to stand about.
                    let have_pal = drawn.iter().any(|e| {
                        e.kind == hw_net::EntityKind::Sidekick
                            && e.mount.map(|m| m.0) == me_state.map(|m| m.id)
                    });
                    if force_trick.is_some() && (have_pal || fight.target.is_some()) {
                        pending_power = 0;
                    }
                }
                if let Some(s) = portal_script.as_mut() {
                    if let Some(p) = s.press() {
                        pending_power = p.0;
                    }
                    if std::mem::take(&mut s.back) {
                        let a = hero_input.aim;
                        hero_input.aim = [-a[0], a[1], -a[2]];
                    }
                    if let Some(shot) = s.shot() {
                        world.queue_screenshot(shot);
                    }
                    if s.done() {
                        world.request_exit();
                    }
                }
                let f = fightview::FightInput {
                    target: fight.target,
                    ..f
                };
                hero_input.target = f.target;
                // `HW_HOLD=1` (screenshots): the help key held down.
                hero_input.revive = f.revive || hold_help;
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
                match b.step(frame.time, world) {
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
            let ticks = tick_clock.advance(frame.dt as f64);
            for _ in 0..ticks {
                // A power key goes out once, with the next tick.
                // (a click shorter than a tick still punches).
                let mut i = hero_input;
                if pending_power != 0 {
                    hw_client::devlog::add(format!(
                        "client: power {pending_power} goes to the server with this tick"
                    ));
                }
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
            // Single player: the server runs these ticks on its own thread
            // while this frame goes on; we draw the world as it stood after
            // the frame before (waiting here only if it is not there yet).
            hw_client::hitch::mark("server wait");
            session.sync();
            if let Some((busy, _)) = session.server_load() {
                // The server thread's own time on the frame before's ticks.
                hw_client::hitch::count("server busy us", (busy * 1e6) as u64);
                // Which parts of it took long (1 ms and more).
                for &(name, us) in session.server_parts() {
                    if us >= 1000 {
                        hw_client::hitch::count(name, us);
                    }
                }
                for line in session.server_overruns() {
                    hw_client::hitch::event(line.clone());
                }
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
                        citizens = std::sync::Arc::new(people);
                        citizens.set_hour(game_hour() as f64);
                        street.clear(world);
                        street = hw_client::streetview::StreetView::new(generated.area.clone());
                        stands = next_stands;
                        traffic_view.clear(world);
                        riders.clear(world, &crowd);
                        bus_folk.clear(world, &crowd);
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
                        engine::parallel::background_thread();
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
            // What players say: the chat window's Local channel.
            for m in &news {
                if let hw_net::ToClient::Chat { from, text } = m {
                    let who = session
                        .player_name(*from)
                        .unwrap_or_else(|| "A hero".into());
                    let voice = if who == name {
                        hw_client::chat::Voice::Me
                    } else {
                        hw_client::chat::Voice::Player
                    };
                    chat.add(hw_client::chat::Channel::Local, &who, text, voice);
                }
                if let hw_net::ToClient::TeamChat { from, text } = m {
                    let who = session
                        .player_name(*from)
                        .unwrap_or_else(|| "A hero".into());
                    let voice = if who == name {
                        hw_client::chat::Voice::Me
                    } else {
                        hw_client::chat::Voice::Player
                    };
                    chat.add(hw_client::chat::Channel::Team, &who, text, voice);
                }
            }
            news.retain(|m| match m {
                hw_net::ToClient::Chat { .. } | hw_net::ToClient::TeamChat { .. } => false,
                hw_net::ToClient::Story { .. } => {
                    story.hear(m);
                    false
                }
                hw_net::ToClient::Case { .. } | hw_net::ToClient::CaseBoard { .. } => {
                    cases.hear(m);
                    false
                }
                hw_net::ToClient::Incident {
                    id, at, yaw, state, ..
                } => {
                    // New, or over (not each step of a help bar).
                    if *state != 0 || !radio.calls.contains_key(id) {
                        hw_client::hitch::event(format!(
                            "incident {id} at {:.0}, {:.0}",
                            at[0], at[2]
                        ));
                    }
                    radio.hear(world, m);
                    // `HW_VIEW=getaway`: the camera follows the crooks
                    // making off.
                    if view == "getaway" && *state == hw_sim::incidents::GETTING_AWAY {
                        getaway_at = Some(Vec3::from_array(*at));
                    }
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
                hw_net::ToClient::Crime { .. } => {
                    street.crime.hear(m, session.time(tick_clock.frac()));
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
                hw_net::ToClient::Gates { gates } => {
                    let _ = fight.gates.apply(world, gates);
                    false
                }
                hw_net::ToClient::Doors { doors, live } => {
                    // A joined client keeps its own copy for collision (in
                    // single player the server's is shared: no change then).
                    for &(b, k, open) in doors {
                        generated.obstacles.set_door(b, k, open);
                        if *live {
                            door_view.heard(world, &town, b, k, open, frame.time as f64);
                        }
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
                    // Rescues' people (trapped, hurt, the medic).
                    radio.cast.add(&b.who, &b.model);
                    models.insert(b.who.clone(), b.model.clone());
                }
            }

            hw_client::hitch::mark("heroes");
            // Draw heroes.
            let heroes = session.entities(frame.dt as f64, tick_clock.frac());
            anims.retain(|id, _| heroes.iter().any(|h| h.id == *id));
            tricks.retain(|id, _| heroes.iter().any(|h| h.id == *id));
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
                        // A drone turns slowly while it hovers, bobbing a
                        // little; crossing town it faces the way it flies,
                        // leaning into it.
                        let t = frame.time + h.id.0 as f32;
                        let v = Vec3::from(h.vel);
                        let flat = glam::Vec2::new(v.x, v.z);
                        place = Place::new(
                            h.pos[0] as f32,
                            h.pos[1] as f32 + (t * 2.3).sin() * 0.06,
                            h.pos[2] as f32,
                        );
                        place = if flat.length() > 4.0 {
                            place
                                .with_yaw_deg(flat.x.atan2(flat.y).to_degrees())
                                .with_pitch_deg((flat.length() * 0.4).min(14.0))
                        } else {
                            place.with_yaw_deg(t * 40.0)
                        };
                    }
                    match hero_entities.get(&h.id) {
                        Some(&(e, _)) => world.set_place(e, place)?,
                        None => {
                            let mesh = if h.kind == hw_net::EntityKind::Drone {
                                // A medical drone fetches heroes, police ones
                                // villains: the server says which.
                                let medical = h.mount.is_some_and(|m| m.1 == 1);
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
                // Machines are drawn from their parts (`robotview`), a
                // police cruiser by `policeview`.
                if h.kind.is_robot() || h.kind == hw_net::EntityKind::PoliceCar {
                    continue;
                }
                let feet = Vec3::new(h.pos[0] as f32, h.pos[1] as f32, h.pos[2] as f32);
                let villain = h.kind.is_enemy();
                // An officer walks and stands as the townsfolk do.
                let bystander = matches!(
                    h.kind,
                    hw_net::EntityKind::Bystander | hw_net::EntityKind::Officer
                );
                // The mutant alpha is a brute a head and a half taller.
                let size = if h.kind == hw_net::EntityKind::Alpha {
                    1.35
                } else if h.kind == hw_net::EntityKind::Sidekick {
                    hw_sim::sidekick::SIZE
                } else {
                    1.0
                };
                // Our own flight we know; others' is told from their motion.
                let flying = (mine == Some(h.id)).then_some(hero_flies);
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
                // A sidekick standing by turns, unhurried, to its hero, and
                // now and then does a little trick.
                let mut trick = None;
                if h.kind == hw_net::EntityKind::Sidekick {
                    let idle = d.clip == "idle"
                        && h.grounded
                        && action.is_none()
                        && h.combat.is_none_or(|c| c.target.is_none());
                    let owner = h.mount.and_then(|m| at_of(m.0));
                    if let (true, Some(o)) = (idle, owner) {
                        let to = o - feet;
                        if to.length() > 0.3 {
                            anim.turn_to(to.x.atan2(to.z).to_degrees(), 2.5, frame.dt);
                            d.yaw = anim.yaw();
                        }
                    }
                    trick = tricks
                        .entry(h.id)
                        .or_default()
                        .step(h.id.0 as u64, idle, frame.dt);
                    // `HW_TRICK=wave` (screenshots): that trick, over and over.
                    if let Some(t) = force_trick.filter(|_| idle) {
                        trick = Some((t, false));
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
                // A crook or victim of a street crime a hero stepped into
                // keeps the looks it was drawn in (the server sends them).
                let own_look = h.mount.filter(|m| m.0 == h.id).map(|m| m.1 as u32);
                let who = match h.kind {
                    k if VillainKind::of(k).is_some() => {
                        let v = VillainKind::of(k).expect("a villain");
                        Who::Villain(v, own_look.unwrap_or(h.id.0) % v.looks())
                    }
                    // A townsperson in a crime: one of the citizens' looks.
                    hw_net::EntityKind::Bystander => {
                        Who::Citizen(own_look.unwrap_or(h.id.0) % hw_sim::citizens::LOOKS)
                    }
                    // A police officer.
                    hw_net::EntityKind::Officer => {
                        Who::Officer(h.mount.map_or(0, |m| m.1) % hw_sim::police::LOOKS)
                    }
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
                // The server is the truth: a mob that can act again stands
                // where the server has it. A ragdoll thrown elsewhere (the
                // client's physics knows no walls) is replaced by a fresh
                // body there, not left drawing a body nothing is attacking
                // from.
                if let Some(&(old, true)) = hero_entities.get(&h.id) {
                    let drawn = world
                        .ragdoll_state(old)
                        .and_then(|_| world.world_transform(old).ok().map(|m| m.w_axis.truncate()));
                    let up = h
                        .combat
                        .is_some_and(|c| c.status == hw_net::Status::Standing);
                    if drawn.is_some_and(|at| fightview::ragdoll_strayed(up, at, feet)) {
                        fight.drop_gun(world, old);
                        world.despawn(old);
                        hero_entities.remove(&h.id);
                        settle.remove(&h.id);
                    }
                }
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
                                fight.crown(world, e, h.kind);
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
                // Someone hurt in the arms: carried in a cradle of both.
                let carry = radio
                    .is_carrying(h.id.0)
                    .then(|| hw_client::poses::carry_clip(d.clip))
                    .flatten();
                let (clip, once) = match action {
                    Some(a) => (a, a != "cuffed" && a != "hover"),
                    None if in_fight && h.kind.is_ranged() && d.clip == "idle" => {
                        ("pistol idle", false)
                    }
                    None if in_fight && d.clip == "idle" => ("fight stance", false),
                    // Villains at a call nobody has stopped yet go about
                    // their crime: menacing their victims, waving guns.
                    None if villain && d.clip == "idle" && radio.menacing(feet) => {
                        if h.kind.is_ranged() {
                            ("pistol aim", false)
                        } else {
                            ("threaten", false)
                        }
                    }
                    // Villains standing about are never statues: each has its
                    // own way of killing time, changing now and then.
                    None if villain
                        && d.clip == "idle"
                        && carry.is_none()
                        && h.combat.is_none_or(|c| c.target.is_none()) =>
                    {
                        (hw_client::crimeview::idle_act(h.id.0, frame.time), false)
                    }
                    None if trick.is_some() => trick.expect("a trick"),
                    // The one being mugged cowers until it is over.
                    None if bystander && d.clip == "idle" => ("hands up", false),
                    // A level up: a fist to the sky, standing or hovering.
                    None if mine == Some(h.id)
                        && frame.time < power_pose_until
                        && matches!(d.clip, "idle" | "hover") =>
                    {
                        ("power pose", true)
                    }
                    // Switching the police visor: a hand to the temple.
                    None if mine == Some(h.id)
                        && frame.time < visor_tap_until
                        && carry.is_none()
                        && matches!(d.clip, "idle" | "walk") =>
                    {
                        if d.clip == "walk" {
                            ("visor tap walk", true)
                        } else {
                            ("visor tap", true)
                        }
                    }
                    None => match carry {
                        Some(c) => (c, false),
                        None => (d.clip, d.once),
                    },
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
                                | "lounge"
                                | "lookout"
                                | "count"
                                | "laugh"
                                | "carry idle"
                                | "carry walk"
                                | "carry jog"
                                | "carry sprint"
                        )
                        || (matches!(
                            clip,
                            "power pose"
                                | "visor tap"
                                | "visor tap walk"
                                | "wave"
                                | "look around"
                                | "polish"
                                | "head tilt"
                        ) && h.grounded);
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
            for line in session.take_server_log() {
                hw_client::devlog::add(line);
            }
            fight.muzzles = robots.muzzles(world);
            fight.on_fx(world, &events, now, mine, &hero_entities, &heroes);
            // Rescue bubbles take the rescue's own people; those set down
            // in the street walk off.
            for (key, at, hurt) in std::mem::take(&mut fight.bubbled) {
                if let Some((who, lift)) = radio.into_bubble(world, at, hurt) {
                    fight.bubbles.hold(world, key, who, lift, !hurt);
                }
            }
            for (who, feet, dir) in std::mem::take(&mut fight.bubbles.landed) {
                radio.walk_off(world, who, feet, dir);
            }
            let muzzles = fight.muzzles.clone();
            robots.on_fx(world, &mut fight, &events, &heroes, &muzzles, mine);
            hw_client::hitch::mark("robots");
            robots.update(world, &heroes, frame.dt)?;
            // The hideouts' safes, and where they stand for the things in them.
            safes.update(world, &heroes)?;
            police_view.update(world, &heroes, frame.dt)?;
            cases.safes = heroes
                .iter()
                .filter(|e| e.kind == hw_net::EntityKind::Safe)
                .map(|e| {
                    let (at, facing) = hw_client::safeview::stands(e);
                    (at.as_vec3(), facing.as_vec2())
                })
                .collect();
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
            // `HW_VIEW=police` (with `HW_POLICE=<s>`): the cruiser called to
            // the hero, from in front of its nose (`HW_POLICE_CAM="side,up,
            // ahead"` in metres moves the camera; `HW_POLICE_AT=officer`
            // looks at the first officer instead, from 2.5 m).
            if view == "police" {
                let car = heroes
                    .iter()
                    .find(|e| e.kind == hw_net::EntityKind::PoliceCar);
                let officer = heroes
                    .iter()
                    .find(|e| e.kind == hw_net::EntityKind::Officer)
                    .filter(|_| std::env::var("HW_POLICE_AT").as_deref() == Ok("officer"));
                if let Some(c) = car {
                    let cam: Vec<f32> = std::env::var("HW_POLICE_CAM")
                        .ok()
                        .map(|v| v.split(',').filter_map(|n| n.parse().ok()).collect())
                        .filter(|v: &Vec<f32>| v.len() == 3)
                        .unwrap_or(vec![4.6, 1.9, 6.4]);
                    let turn = glam::Quat::from_array(c.rot);
                    let at = Vec3::from_array([c.pos[0] as f32, c.pos[1] as f32, c.pos[2] as f32]);
                    let (eye, look) = match officer {
                        Some(o) => {
                            let p = Vec3::from_array([
                                o.pos[0] as f32,
                                o.pos[1] as f32,
                                o.pos[2] as f32,
                            ]);
                            let face = glam::Quat::from_array(o.rot) * Vec3::Z;
                            (
                                p + face * 2.3 + Vec3::Y * 1.55 + face.cross(Vec3::Y) * 0.8,
                                p + Vec3::Y * 1.35,
                            )
                        }
                        None => (
                            at + turn * Vec3::new(cam[0], cam[1], cam[2]),
                            at + turn * Vec3::new(0.0, 0.75, 0.5),
                        ),
                    };
                    let d = look - eye;
                    let pitch = (d.y / glam::Vec2::new(d.x, d.z).length())
                        .atan()
                        .to_degrees();
                    fly = FlyCamera::new((eye.x, eye.y, eye.z), d.x.atan2(d.z).to_degrees(), pitch);
                }
            }
            // `HW_VIEW=safe`: a hideout's safe shut and broken open (the
            // painting in it), close up, set down by the hero.
            if view == "safe" && !safe_shown {
                if let Some(me) = session.my_position(&heroes) {
                    safe_shown = true;
                    let at = me.as_vec3() + Vec3::new(3.0, 0.0, 0.0);
                    for (k, open) in [false, true].into_iter().enumerate() {
                        let p = at + Vec3::Z * (k as f32 * 1.7 - 0.85);
                        world.place(
                            hw_client::safeview::safe_mesh(open),
                            Place::new(p.x, p.y, p.z).with_yaw_deg(-90.0),
                        )?;
                        if open {
                            world.place(
                                hw_client::missionview::thing_mesh(
                                    hw_sim::missions::board::CrimeKind::Art,
                                ),
                                Place::new(p.x, p.y + 0.21, p.z)
                                    .with_yaw_deg(-90.0)
                                    .with_scale(0.95),
                            )?;
                        }
                    }
                    let eye = at + Vec3::new(-2.6, 1.5, 0.9);
                    let d = at + Vec3::Y * 0.8 - eye;
                    let pitch = (d.y / glam::Vec2::new(d.x, d.z).length())
                        .atan()
                        .to_degrees();
                    fly = FlyCamera::new((eye.x, eye.y, eye.z), d.x.atan2(d.z).to_degrees(), pitch);
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
            // `HW_VIEW=saucer` (with HW_INCIDENT=saucer): the Zorrak saucer and
            // its beam from above the roofs, looking down at the street.
            if view == "saucer" {
                if let Some(at) = radio.saucer_at() {
                    let c = radio.calls.values().next();
                    let f = c.map_or(Vec3::Z, |c| {
                        Vec3::new(c.facing.x as f32, 0.0, c.facing.y as f32)
                    });
                    let side = Vec3::new(f.z, 0.0, -f.x);
                    let foot = c.map_or(at - Vec3::Y * 14.0, |c| c.at);
                    // Above the roofs (the saucer hovers over them), so no wall
                    // stands between the camera and the beam.
                    let p = at + f * 42.0 + side * 16.0 + Vec3::Y * 2.0;
                    let d = Vec3::new(at.x, at.y * 0.6 + foot.y * 0.4, at.z) - p;
                    let pitch = (d.y / glam::Vec2::new(d.x, d.z).length())
                        .atan()
                        .to_degrees();
                    fly = FlyCamera::new((p.x, p.y, p.z), d.x.atan2(d.z).to_degrees(), pitch);
                }
            }
            // `HW_VIEW=call` (with HW_INCIDENT=<kind>): the first call on the
            // police visor, from over the rooftops 50 m off.
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
            traffic.set_blocked(&alarms.road_blocks());
            let game_time = session.time(tick_clock.frac());
            let eye = world.camera().eye();
            let eye2 = DVec2::new(eye.x as f64, eye.z as f64);
            let dt = frame.dt as f64;
            // People at home, at work and shopping on the floors drawn
            // open near the camera: a hash per place, the nearest shown.
            let mut indoor: Vec<hw_sim::citizens::Citizen> = town
                .indoor_spots()
                .flat_map(|f| {
                    f.spots
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| DVec2::new(s.at.x, s.at.z).distance(eye2) < INDOOR_SHOWN_M)
                        .filter_map(|(n, s)| {
                            citizens.indoors(
                                f.building, f.level, n as u32, s.room, s.at, s.facing, game_time,
                            )
                        })
                })
                .collect();
            // (Nearest in space: a building of halls has people on every
            // floor, and those on the camera's floor come first.)
            let eye3 = eye.as_dvec3();
            indoor.sort_by(|a, b| {
                let d = |c: &hw_sim::citizens::Citizen| c.pos.distance_squared(eye3);
                d(a).total_cmp(&d(b)).then(a.id.cmp(&b.id))
            });
            indoor.truncate(INDOOR_SHOWN_MAX);
            // The people who live and work along the Aether Tube, while the
            // camera is down there (tiles the deep view has planned).
            let world_plan = &generated.world;
            if (eye.y as f64) < world_plan.ground(eye2) - 3.0 {
                let mut folk: Vec<hw_sim::citizens::Citizen> = world_plan
                    .deep_planned_near(eye2, TUBE_FOLK_SHOWN_M)
                    .iter()
                    .flat_map(|t| {
                        t.folk
                            .iter()
                            .enumerate()
                            .filter(|(_, f)| {
                                DVec2::new(f.at.x, f.at.z).distance(eye2) < TUBE_FOLK_SHOWN_M
                            })
                            .map(|(n, f)| citizens.tube_folk(t.key, n as u32, f, game_time))
                            .collect::<Vec<_>>()
                    })
                    .collect();
                folk.sort_by(|a, b| {
                    let d = |c: &hw_sim::citizens::Citizen| {
                        DVec2::new(c.pos.x, c.pos.z).distance_squared(eye2)
                    };
                    d(a).total_cmp(&d(b)).then(a.id.cmp(&b.id))
                });
                folk.truncate(TUBE_FOLK_SHOWN_MAX);
                indoor.extend(folk);
            }
            crowd.set_indoor(indoor);
            // The traffic drives on and its cars are placed on a worker,
            // while the citizens go to Engine here; the join below is the
            // only meeting point (the borrows keep the two apart).
            // It runs on one of Engine's frame threads, which are idle
            // while the game's own update runs and so start at once: a
            // thread started here each frame waited up to 31 ms for a core
            // while the town was being built on all of them.
            let (traffic_plan, survey) = {
                let (traffic, traffic_view) = (&mut traffic, &mut traffic_view);
                let town_plan = &generated.world;
                let (plan, crowd_done) = engine::parallel::beside_frame(
                    move || {
                        traffic.advance_to(game_time);
                        let plan = traffic_view.plan(traffic, eye2, game_time, dt);
                        // What the soundscape asks of the town round the camera.
                        let survey = hw_client::soundscape::Survey {
                            cars: traffic.near(eye2, 90.0, game_time, 40),
                            under: hw_client::soundscape::under(town_plan, eye),
                            way_up: hw_client::soundscape::way_up(town_plan, eye),
                            district: hw_client::soundscape::district(town_plan, eye2),
                        };
                        (plan, survey)
                    },
                    // Citizens around the camera, at the game's time.
                    || {
                        crowd
                            .update(world, &citizens, &alarms, eye2, game_time, dt)
                            .and_then(|()| sweepers.update(world, &citizens, eye2, game_time, dt))
                    },
                );
                crowd_done?;
                plan
            };
            // Street performers, odd folk and the night club round the camera.
            // (`HW_STREETLIFE=0` turns it off, to measure without it.)
            if std::env::var("HW_STREETLIFE").map_or(true, |v| v != "0") {
                let hero_at = session.my_position(&heroes).map(|p| p.as_vec3());
                // Villains carrying something off: their bodies.
                fight.loot.retain(|id, _| {
                    heroes.iter().any(|h| {
                        h.id == *id
                            && h.combat
                                .is_none_or(|c| c.status == hw_net::Status::Standing)
                    })
                });
                let carried: Vec<(EntityId, u16)> = fight
                    .loot
                    .iter()
                    .filter_map(|(id, l)| hero_entities.get(id).map(|e| (e.0, *l)))
                    .collect();
                street.update(
                    world,
                    &citizens,
                    &alarms,
                    &models,
                    actors.as_ref(),
                    eye2,
                    eye,
                    hero_at,
                    &carried,
                    game_time + act_skip,
                    dt,
                )?;
            }
            // `HW_VIEW=act HW_ACT=<juggler|theremin|statue|prophet|spotter|
            // newsboy|stargazer|dancers|club|trader>`: the nearest such street act,
            // from in front and a little to the side (`HW_ACT_FAR=1`: wider).
            if view == "act" {
                act_try += 1;
                if act_spot.is_none() && act_try % 60 == 1 {
                    let want = std::env::var("HW_ACT").unwrap_or_else(|_| "juggler".into());
                    let act = hw_sim::streetlife::Act::ALL
                        .into_iter()
                        .find(|a| format!("{a:?}").eq_ignore_ascii_case(&want));
                    act_spot = act.and_then(|a| {
                        street.find(&citizens, a, spawn, 600.0, game_time + act_skip)
                    });
                    println!("act view: {want} at {act_spot:?} (game time {game_time:.0} s)");
                }
                if let Some((at, face)) = act_spot {
                    let far = if std::env::var_os("HW_ACT_FAR").is_some() {
                        2.0
                    } else {
                        1.0
                    };
                    let f = Vec3::new(face.x as f32, 0.0, face.y as f32);
                    let side = Vec3::new(f.z, 0.0, -f.x);
                    let p = at + f * 4.2 * far + side * 1.6 * far + Vec3::Y * (1.7 + 0.6 * far);
                    let d = at + Vec3::Y * 1.2 - p;
                    let pitch = (d.y / glam::Vec2::new(d.x, d.z).length())
                        .atan()
                        .to_degrees();
                    fly = FlyCamera::new((p.x, p.y, p.z), d.x.atan2(d.z).to_degrees(), pitch);
                }
            }
            // `HW_VIEW=crime HW_CRIME=<snatch|hassle|shakedown|protection|
            // tagging|vandals>`: the nearest such crime, the street's clock
            // jumped to the middle of it (`HW_CRIME_AT`: 0 as the crooks
            // arrive, 1 as the deed is done, 2 as they leave), from a few
            // metres off.
            if view == "crime" {
                crime_try += 1;
                if crime_spot.is_none() && crime_try % 30 == 1 {
                    let life = street.crime.life().clone();
                    let near = life.crimes_near(spawn, 400.0, game_time, 2.0 * 150.0);
                    if let Some(c) = near.first() {
                        let at: f64 = std::env::var("HW_CRIME_AT")
                            .ok()
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0.6);
                        let when = if at <= 1.0 {
                            c.on + (c.done - c.on) * at
                        } else {
                            c.done + (c.gone - c.done) * (at - 1.0) * 0.5
                        };
                        act_skip = when - game_time;
                        let p = &c.place;
                        // Where its people are then (a thief runs on).
                        let folk = c.people(&life, when);
                        let mid = if folk.is_empty() {
                            Vec3::new(p.at.x as f32, p.y as f32, p.at.y as f32)
                        } else {
                            folk.iter().map(|f| f.pos.as_vec3()).sum::<Vec3>() / folk.len() as f32
                        };
                        let out = Vec3::new(p.inner.x as f32, 0.0, p.inner.y as f32);
                        let side = Vec3::new(p.along.x as f32, 0.0, p.along.y as f32);
                        let eye = mid - out * 5.5 + side * 2.5 + Vec3::Y * 2.2;
                        crime_spot = Some((mid, eye, when));
                        println!(
                            "crime view: {:?} at {:.0}, {:.0}, game time {:.0} s (skip {:.0} s)",
                            c.kind, p.at.x, p.at.y, game_time, act_skip
                        );
                    }
                }
                if let Some((at, p, when)) = crime_spot {
                    // The street's clock held at that moment (a slow
                    // render takes minutes of real time).
                    act_skip = when - game_time;
                    let d = at + Vec3::Y * 1.1 - p;
                    let pitch = (d.y / glam::Vec2::new(d.x, d.z).length())
                        .atan()
                        .to_degrees();
                    fly = FlyCamera::new((p.x, p.y, p.z), d.x.atan2(d.z).to_degrees(), pitch);
                }
            }
            // `HW_VIEW=getaway HW_INCIDENT=robbery HW_TIMEOUT=<s>`: the crooks
            // of an unanswered call making off, from above and behind.
            if let (true, Some(at)) = (view == "getaway", getaway_at) {
                let p = at + Vec3::new(9.0, 7.0, 9.0);
                let d = at + Vec3::Y * 1.0 - p;
                let pitch = (d.y / glam::Vec2::new(d.x, d.z).length())
                    .atan()
                    .to_degrees();
                fly = FlyCamera::new((p.x, p.y, p.z), d.x.atan2(d.z).to_degrees(), pitch);
            }
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
                    movers.extend(hw_client::safeview::solids(&heroes));
                    movers.extend(hw_sim::police::solids_of(&heroes));
                    if let Some(l) = extras.lifts.as_ref() {
                        movers.extend(l.lifts().solids_near(&[me], 25.0, game_time));
                    }
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
            // People waiting at the bus stops, getting on and off.
            bus_folk.update(
                world,
                &crowd,
                &traffic,
                &citizens,
                traffic_view.riding(),
                eye2,
                game_time,
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
                        hw_sim::traffic::VehicleKind::Bus(_) => "Bus".into(),
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
                // Buildings of halls are never drawn cut open: their
                // walls and ceilings are all there, so the camera must
                // stay clear of every one of them.
                if plan.layout.is_some() {
                    return None;
                }
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
                    // Stepped through a portal: the camera turns as the
                    // hero did, so the walking keys still go the same way.
                    if let Some(turn) = fight.gate_turn.take() {
                        hero_cam.yaw = (hero_cam.yaw + turn).rem_euclid(360.0);
                    }
                    let orbit = portal_script.as_ref().and_then(|s| s.orbit());
                    let kept = (hero_cam.yaw, hero_cam.distance);
                    if let Some(deg) = orbit {
                        hero_cam.yaw += deg;
                        hero_cam.distance = HeroCamera::MAX_DISTANCE;
                    }
                    hero_cam.update(
                        world,
                        frame,
                        p.as_vec3(),
                        &solids,
                        open_above,
                        fight.gates.pairs(),
                    );
                    if orbit.is_some() {
                        (hero_cam.yaw, hero_cam.distance) = kept;
                    }
                    let shake = fight.shake();
                    if shake != Vec3::ZERO {
                        let cam = world.camera();
                        world.look_first_person(
                            cam.eye() + shake,
                            hero_cam.view_yaw,
                            hero_cam.pitch,
                        );
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
            // Just off the title: the camera holds the title's view until the
            // hero's surroundings are in, then flies an arc down to the hero,
            // the sun climbing from golden hour to the game's hour.
            if let Some(f) = swoop.as_mut() {
                use hw_client::startflight::{Pose, WARM_RADIUS_M};
                let cam = world.camera();
                let (to_eye, look_at) = (cam.eye(), cam.target());
                let d = (look_at - to_eye).normalize_or_zero();
                let to = Pose {
                    eye: to_eye,
                    yaw: d.x.atan2(d.z).to_degrees(),
                    pitch: d.y.clamp(-1.0, 1.0).asin().to_degrees(),
                };
                let hero = me.map(|p| p.as_vec3());
                let ready = hero.map(|h| {
                    let at = DVec2::new(h.x as f64, h.z as f64);
                    let focus = f.focus(world.camera().eye(), hero);
                    let mine = session.my_hero();
                    town.ready_near(&focus, at, WARM_RADIUS_M)
                        && extras
                            .props
                            .as_ref()
                            .is_none_or(|v| v.ready_near(at, WARM_RADIUS_M))
                        && mine.is_some_and(|m| hero_entities.contains_key(&m))
                });
                match f.step(frame.dt, ready, to, look_at) {
                    Some((p, k)) => {
                        world.look_first_person(p.eye, p.yaw, p.pitch);
                        // `HW_FLIGHT_SHOTS=<dir>`: a picture every 8 frames of the
                        // flight (to look at the path).
                        if k > 0.0 {
                            if let Ok(dir) = std::env::var("HW_FLIGHT_SHOTS") {
                                flight_frames += 1;
                                if flight_frames % 8 == 1 {
                                    world.queue_screenshot(
                                        std::path::Path::new(&dir)
                                            .join(format!("flight-{flight_frames:04}.png")),
                                    );
                                }
                            }
                        }
                        let hour = title::TITLE_HOUR + (hour - title::TITLE_HOUR) * k;
                        hw_client::lighting::set_sky(world, hour);
                    }
                    None => {
                        hw_client::hitch::event("the camera reached the hero");
                        swoop = None;
                    }
                }
            }
            hw_client::hitch::mark("fight screen");
            // The fight's screen over everything.
            {
                let view_proj = world.camera().view_projection(frame.aspect);
                let me_state = mine.and_then(|id| heroes.iter().find(|e| e.id == id));
                // The police visor first, so the HUD's panels sit over it.
                {
                    let toggle =
                        (frame.input.pressed(Key::T) && !chat.typing && !on_ui_keys(frame))
                            || std::mem::take(&mut visor_asked);
                    let why = visor.update(frame.dt, toggle, &radio);
                    if visor.take_tap() {
                        visor_tap_until = frame.time + hw_client::visor::TAP_SECONDS;
                    }
                    // Over the hero's eyes, seen from behind.
                    let body = mine.and_then(|m| hero_entities.get(&m)).map(|&(e, _)| e);
                    visor.holo(world, body, world.camera().eye());
                    let level = me_state.and_then(|m| m.combat).map_or(1, |c| c.level);
                    let eye = world.camera().eye();
                    visor.draw(frame.ui.ctx(), view_proj, eye, &radio, level, why);
                }
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
                        click_ray: None,
                    },
                );
                if let Some(c) = moved {
                    powers.tray.apply(c);
                    save_tray(&powers.tray);
                }
                let fighting = me_state.and_then(|m| m.combat).map_or(1, |c| c.level);
                hw_client::hitch::mark("radio");
                radio.me = mine.map(|m| m.0);
                // Holding the help key: the rescue bar fills.
                let holding = (frame.input.down(Key::E) || hold_help)
                    .then(|| session.my_position(&heroes).map(|p| p.as_vec3()))
                    .flatten();
                radio.hero_at = session.my_position(&heroes).map(|p| p.as_vec3());
                radio.update(frame.dt, holding);
                // E at a door opens or shuts it, unless E has a rescue or
                // a downed teammate to help here (holding E is theirs).
                if let Some(hero) = session.my_position(&heroes) {
                    let h = hero.as_vec3();
                    let rescue = radio
                        .help(h)
                        .is_some_and(|x| x != hw_client::radioview::Help::CarryThem);
                    let me = mine.and_then(|id| heroes.iter().find(|e| e.id == id));
                    let standing = me
                        .and_then(|m| m.combat)
                        .is_none_or(|c| c.status == hw_net::Status::Standing);
                    let teammate_down = heroes.iter().any(|e| {
                        Some(e.id) != mine
                            && e.kind == hw_net::EntityKind::Hero
                            && e.combat.is_some_and(|c| c.status == hw_net::Status::Down)
                            && Vec3::from_array(e.pos.map(|v| v as f32)).distance(h)
                                <= hw_sim::combat::REVIVE_REACH as f32
                    });
                    let door = (!rescue && !teammate_down && standing)
                        .then(|| hw_client::doorview::door_at_hand(&generated.obstacles, hero))
                        .flatten();
                    if let Some(d) = door {
                        if frame.input.pressed(Key::E) && !chat.typing && !on_ui_keys(frame) {
                            session.use_door(d.building, d.key, !d.open);
                        }
                        engine::ui::egui::Area::new(engine::ui::egui::Id::new("door prompt"))
                            .interactable(false)
                            .anchor(
                                engine::ui::egui::Align2::CENTER_BOTTOM,
                                engine::ui::egui::vec2(0.0, -170.0),
                            )
                            .show(frame.ui.ctx(), |ui| {
                                hw_client::googie::plate(ui, false, |ui| {
                                    ui.label(
                                        engine::ui::egui::RichText::new(
                                            hw_client::doorview::prompt(d.open, d.kind),
                                        )
                                        .color(engine::ui::egui::Color32::from_rgb(255, 230, 140))
                                        .size(17.0),
                                    );
                                });
                            });
                    }
                }
                radio.animate(world, frame.dt);
                radio.rescues(world, frame.dt, &|at, heading| {
                    traffic.street_stretch(at, heading, 9.0, 70.0, 400.0)
                });
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
                cases.update(world, &generated.world, &mut radio, frame.dt);
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
                cases.panel(frame.ui.ctx(), hero_at);
                // The chat window: what was said near the hero this frame,
                // Starling on the wrist radio, and what the player types.
                {
                    use hw_client::chat::{Channel, Voice};
                    for s in fight.said.drain(..) {
                        chat.hear(&s, hero_at);
                    }
                    // `HW_SPEECH=1`: a few bubbles round the hero, to look at
                    // them (a long line, a shout, one behind the camera).
                    let demo_here = hero_at.unwrap_or(Vec3::ZERO);
                    if speech_demo && speech_demo_at.is_none_or(|a| a.distance(demo_here) > 3.0) {
                        // Round what the camera looks at (again when the
                        // hero is placed somewhere else).
                        if speech_demo_at.is_none() {
                            hw_client::title::install_fonts(frame.ui.ctx());
                        }
                        speech_demo_at = Some(demo_here);
                        fight.speech.clear();
                        if let Some(h) = hero_at.map(|_| demo_here) {
                            use hw_client::speech::Speech;
                            let side = (cam.target() - cam.eye())
                                .cross(Vec3::Y)
                                .normalize_or_zero();
                            let ahead = (cam.target() - cam.eye()).with_y(0.0).normalize_or_zero();
                            fight.speech.add(Speech::say(
                                h + side * 2.5 + ahead * 3.0,
                                "Gee whiz, a real hero! Wait till I tell the folks down at the diner, they'll never believe it!",
                                Voice::Folk,
                            ));
                            fight.speech.add(Speech::shout(
                                h - side * 2.0 + ahead * 5.0,
                                "Stop, thief!",
                                Voice::Folk,
                            ));
                            fight.speech.add(Speech::say(
                                h + side * 0.6 + ahead * 9.0,
                                "Swell day for it.",
                                Voice::Ally,
                            ));
                            fight.speech.add(Speech::say(
                                h - ahead * 12.0 + side * 3.0,
                                "Hey, over here!",
                                Voice::Folk,
                            ));
                            fight
                                .speech
                                .add(Speech::say(h, "Hello, citizens!", Voice::Me));
                        }
                    }
                    // What the street acts and their watchers say.
                    for s in street.said.drain(..) {
                        fight.bubble(&s);
                        chat.hear(&s, hero_at);
                    }
                    // Thanks from the people at rescues.
                    for s in radio.said.drain(..) {
                        fight.bubble(&s);
                        chat.hear(&s, hero_at);
                    }
                    for l in story.heard.drain(..) {
                        chat.add(Channel::Local, l.who, &l.text, Voice::Ally);
                    }
                    for l in cases.heard.drain(..) {
                        chat.add(Channel::Local, l.who, &l.text, Voice::Ally);
                    }
                    chat.update(frame.dt);
                    let (beside, above) = fightview::beside_tray(
                        frame.ui.ctx().screen_rect().width(),
                        powers.tray.second_row(),
                    );
                    if let Some((channel, text)) = chat.ui(frame.ui.ctx(), beside, above) {
                        // A friend's world sends our words back to us; our
                        // own world does not.
                        match channel {
                            Channel::Local => {
                                session.say(&text);
                                if !session.joined() {
                                    chat.add(Channel::Local, &name, &text, Voice::Me);
                                }
                                if let Some(h) = hero_at {
                                    fight.speech(h, &text);
                                }
                            }
                            Channel::Team => {
                                session.say_team(&text);
                                if !session.joined() {
                                    chat.add(Channel::Team, &name, &text, Voice::Me);
                                }
                            }
                        }
                    }
                }
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
                // The town map (M): the minimap's tiles, the whole screen;
                // the pointer is let go while it is open.
                if frame.input.pressed(Key::M) {
                    town_map.toggle(hero_at);
                }
                if town_map.open && world.pointer_lock() {
                    world.set_pointer_lock(false);
                    town_map.relock = true;
                }
                town_map.show(
                    frame.ui.ctx(),
                    &generated.world,
                    hero_at,
                    look,
                    &mates,
                    &radio,
                    fighting,
                );
                if !town_map.open && std::mem::take(&mut town_map.relock) {
                    world.set_pointer_lock(true);
                }
                // The case file (J): it comes up on its own when a case
                // opens or closes; opened with J it takes the pointer.
                if frame.input.pressed(Key::J) && !on_ui_keys(frame) {
                    case_file.toggle();
                }
                case_file.update(&mut cases, frame.dt);
                // A case closed: keep the cabinet. On this thread like the
                // hero file's saves (`save_hero`), so the two never race over
                // the profile; it happens once a case, a small file.
                if cases.filed != cabinet_saved {
                    cabinet_saved = cases.filed.clone();
                    save_cabinet(&cabinet_saved);
                }
                case_file.show(frame.ui.ctx(), &mut cases, &story);
                if case_file.taking_pointer() && world.pointer_lock() {
                    world.set_pointer_lock(false);
                    case_file.relock = true;
                }
                if !case_file.taking_pointer() && std::mem::take(&mut case_file.relock) {
                    world.set_pointer_lock(true);
                }
                // The police computer (Y, or its monitor): Crime-o-Vision
                // and the desktop of tools; it takes the pointer.
                if frame.input.pressed(Key::Y) && !chat.typing && !on_ui_keys(frame) {
                    police.toggle();
                }
                if police.icon(frame.ui.ctx(), &cases.board) && !police.crimes {
                    police.desktop = !police.desktop;
                }
                let my_level = me_state.and_then(|m| m.combat).map_or(1, |c| c.level);
                for act in police.show(frame.ui.ctx(), &cases.board, hero_at, my_level) {
                    use hw_client::police::{Action, App};
                    match act {
                        Action::Take(n) => session.take_case(n),
                        Action::Open(app) => {
                            if app != App::CrimeOVision {
                                police.desktop = false;
                            }
                            match app {
                                App::CrimeOVision => {}
                                App::CaseFile => case_file.toggle(),
                                App::Visor => visor_asked = true,
                                App::Map => town_map.toggle(hero_at),
                                App::Powers => powers.open = true,
                            }
                        }
                    }
                }
                if police.taking_pointer() && world.pointer_lock() {
                    world.set_pointer_lock(false);
                    police.relock = true;
                }
                if !police.taking_pointer() && std::mem::take(&mut police.relock) {
                    world.set_pointer_lock(true);
                }
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
                way_up_hint(&frame.ui, eye, world.camera().target() - eye, survey.way_up);
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
                    boss_music: robots.music_on() || music_fade_at.is_some() || street.music_near(),
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
                        PowerAction::Buy(p) => session.buy(p),
                        PowerAction::GiveUp(p) => session.give_up(p),
                        PowerAction::Augment(a) => session.augment(a),
                        PowerAction::Create(s) => {
                            hero_file = Some(s.clone());
                            my_sheet = s.clone();
                            session.send_sheet(s);
                            save_hero(hero_file.clone());
                        }
                        PowerAction::Element(e) => {
                            // Picked once by a hero from before the power
                            // trees: the hero file as the server has it,
                            // with the element.
                            let mut s = my_sheet.clone();
                            s.element = e.code();
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
                // Always-on mouselook: decided last, once every window of
                // this frame is known, so it wins over the pointer frees
                // and re-pins above. Alt held shows the pointer.
                let cursor_needed = menu.open
                    || debug.open
                    || powers.open
                    || creating
                    || hw_sim::powers::Element::from_code(my_sheet.element)
                        == hw_sim::powers::Element::None
                    || chat.typing
                    || taxi_menu
                    || tube_menu.is_some()
                    || town_map.open
                    || case_file.taking_pointer()
                    || police.taking_pointer()
                    || frame.ui.ctx().input(|i| i.modifiers.alt);
                let clicked_in_world = frame.input.mouse_clicked(MouseButton::Left)
                    && !frame.ui.ctx().is_pointer_over_area();
                if let Some(lock) = mouselook.update(
                    menu.mouselook,
                    cursor_needed,
                    world.pointer_lock(),
                    clicked_in_world,
                ) {
                    world.set_pointer_lock(lock);
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
            // Roads and buildings around the camera, full detail close by;
            // on the way in from the title also around the hero, so the
            // camera arrives where everything is in.
            let eye = world.camera().eye();
            let focus = match &swoop {
                Some(f) => f.focus(eye, me.map(|p| p.as_vec3())),
                None => vec![DVec2::new(eye.x as f64, eye.z as f64)],
            };
            town.update_background_around(world, &focus, BUILD_BUDGET_MS);
            hw_client::hitch::mark("doors");
            door_view.update(
                world,
                &town,
                &generated.obstacles,
                frame.dt,
                frame.time as f64,
            )?;
            hw_client::hitch::mark("props");
            // Street props, trees and hedges around the camera.
            if let Some(v) = extras.props.as_mut() {
                v.update_around(&focus, BUILD_BUDGET_MS * 0.5);
                v.flush(world)?;
            }
            hw_client::hitch::mark("waterfront");
            // Engine's water: a coast's open sea, or the clear green-blue of the
            // town's rivers and lakes (the silty river look reads as black from above).
            if let Some(w) = world.water_mut() {
                let look = if town.world.coastal() {
                    engine::water::WaterLook::ocean()
                } else {
                    engine::water::WaterLook::lake()
                };
                if *w.look() != look {
                    w.set_look(look);
                }
            }
            if let Some(w) = extras.waterfront.as_mut() {
                w.update(world, DVec2::new(eye.x as f64, eye.z as f64), true);
            }
            if hide_hud {
                // Painters on their own layers never show up in the layer list
                // above, so everything painted this frame is dropped as well.
                frame.ui.ctx().graphics_mut(|g| *g = Default::default());
            }
            Ok(())
        },
    );
    if let Err(e) = &run {
        hw_client::crash::engine_error(e);
    }
    if run.is_ok() && RESTART_AT_TITLE.load(std::sync::atomic::Ordering::SeqCst) {
        restart_at_title();
    }
    run
}

/// Set by the Escape menu's "Back to the title screen".
static RESTART_AT_TITLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Starts the game again, as if from the launcher, once this one has
/// released its window, sound, server port and threads (`main` calls it after
/// the engine loop returned). Debug switches that skip the title are dropped.
fn restart_at_title() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut cmd = std::process::Command::new(exe);
    cmd.args(std::env::args_os().skip(1));
    for var in [
        "HW_VIEW", "HW_JOIN", "HW_HOST", "HW_AT", "HW_BENCH", "HW_TITLE", "HW_STORY",
    ] {
        cmd.env_remove(var);
    }
    if let Err(e) = cmd.spawn() {
        eprintln!("could not start the title screen again: {e}");
    }
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
            hw_client::googie::plate(ui, false, |ui| {
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
            hw_client::googie::plate(ui, true, |ui| {
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
        // Words only: the pointer over it must not take the keys.
        .interactable(false)
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 44.0))
        .show(ui.ctx(), |ui| {
            hw_client::googie::plate(ui, false, |ui| {
                ui.label(
                    egui::RichText::new(text)
                        .color(egui::Color32::from_rgb(232, 196, 96))
                        .size(15.0),
                );
            });
        });
}

/// Down in the tube: which way the nearest way up to the street is, from
/// where the camera looks, and how far.
fn way_up_hint(ui: &engine::ui::UiFrame, eye: Vec3, look: Vec3, to: Option<DVec2>) {
    use engine::ui::egui;
    let Some(to) = to else {
        return;
    };
    let here = DVec2::new(eye.x as f64, eye.z as f64);
    let d = to - here;
    let dist = d.length();
    let fwd = DVec2::new(look.x as f64, look.z as f64).normalize_or(DVec2::Y);
    // Angle from straight ahead, positive to the right: the screen's right
    // is `forward x up` (right-handed, Y up).
    let right = DVec2::new(-fwd.y, fwd.x);
    let (a, r) = (d.dot(fwd), d.dot(right));
    let ang = r.atan2(a).to_degrees();
    let way = if dist < 6.0 {
        "here".to_string()
    } else {
        let w = match ang {
            x if x.abs() <= 30.0 => "ahead",
            x if x.abs() >= 150.0 => "behind",
            x if x > 0.0 => "to the right",
            _ => "to the left",
        };
        format!("{:.0} m {w}", (dist / 5.0).round() * 5.0)
    };
    egui::Area::new(egui::Id::new("way up"))
        .interactable(false)
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -150.0))
        .show(ui.ctx(), |ui| {
            hw_client::googie::plate(ui, false, |ui| {
                ui.label(
                    egui::RichText::new(format!("Way up to the street: {way}"))
                        .color(egui::Color32::from_rgb(130, 255, 160))
                        .size(14.0),
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
            hw_client::googie::plate(ui, false, |ui| {
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
            hw_client::googie::plate(ui, true, |ui| {
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
            hw_client::googie::plate(ui, false, |ui| {
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
    hw_client::googie::modal(ui, "What's new", open, |ui, open| {
        egui::ScrollArea::vertical()
            .max_height(420.0)
            .show(ui, |ui| {
                ui.set_max_width(520.0);
                for line in hw_core::WHATS_NEW
                    .lines()
                    .skip_while(|l| !l.starts_with("## "))
                {
                    if let Some(h) = line.strip_prefix("## ") {
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(h)
                                .font(hw_client::uikit::ui_font(ui.ctx(), 16.0))
                                .color(hw_client::uikit::GOLD),
                        );
                    } else if let Some(item) = line.strip_prefix("- ") {
                        ui.label(format!("•  {item}"));
                    } else if !line.trim().is_empty() {
                        ui.label(line);
                    }
                }
            });
        ui.add_space(8.0);
        if ui.button("Close").clicked() {
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
