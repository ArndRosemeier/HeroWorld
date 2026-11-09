//! The town's sound: a bed of looping ambiences that crossfade with where
//! you are (busy streets by day or night, a park, a crowd, indoors, the
//! wind when you fly, the tunnels, Molehaven, the villains' quarter), the
//! sounds of real things nearby (car engines, horns, tyres, hover cars
//! overhead, sirens for real trouble, your own footsteps), small life
//! (pigeons, a dog at night, a café's clatter, the town clock), and music
//! that fits the scenery (see [`Music`]).
//!
//! Clips are decoded when first wanted and forgotten when not heard for a
//! while, so only what plays is in memory. Where each clip came from:
//! `assets/sound/README.md` and `assets/music/README.md`.

use crate::mapraster::Surroundings;
use engine::prelude::*;
use glam::{DVec2, DVec3};
use hw_gen::urban::growth::{CentreKind, Era};
use hw_gen::urban::props::CarModel;
use hw_gen::urban::world::TownWorld;
use hw_sim::alarms::AlarmKind;
use hw_sim::traffic::{Vehicle, VehicleKind};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};

macro_rules! clips {
    ($($name:literal),* $(,)?) => {
        &[$(($name, include_bytes!(concat!("../assets/sound/", $name, ".ogg")))),*]
    };
}

/// Every clip, built into the game (3.5 MB of Ogg Vorbis).
static CLIPS: &[(&str, &[u8])] = clips![
    "atom_hum",
    "atom_steam",
    "atom_vent_1",
    "atom_vent_2",
    "amb_city_day",
    "amb_city_night",
    "amb_park",
    "amb_crowd",
    "amb_interior",
    "amb_wind_flight",
    "amb_tunnel",
    "amb_molehaven",
    "amb_villains",
    "v8_idle",
    "v8_cruise",
    "hover_hum",
    "siren",
    "horn_1",
    "horn_2",
    "horn_long",
    "skid",
    "flyby",
    "clock_bell",
    "pigeons",
    "dog_far",
    "bird_flutter",
    "terrace_clatter",
    "alarm_bell",
    "scream_crowd_1",
    "crowd_gasp",
    "step_concrete_1",
    "step_concrete_2",
    "step_concrete_3",
    "step_concrete_4",
    "step_grass_1",
    "step_grass_2",
    "drill_grind",
    "geyser_blast",
    "geyser_rumble",
    "steam_whistle",
    "tesla_buzz",
    "tesla_crack_1",
    "tesla_crack_2",
    "tesla_zap",
    "airship_drone",
    "airship_horn",
    "mooring_clank",
    "winch",
    "rocket_roar",
    "rocket_ignite",
    "rocket_klaxon",
    "rocket_beep",
    "rocket_vent",
    "rocket_touchdown",
    "wx_puff",
    "wx_rain",
    "wx_rainbow",
    "wx_thunder_1",
    "wx_thunder_2",
    "wx_whirr",
];

/// The fight's clips (`fightview.rs` plays them).
pub static FIGHT_CLIPS: &[(&str, &[u8])] = clips![
    "punch",
    "bolt",
    "ray",
    "charge",
    "whoosh",
    "chime",
    "power_hum",
    "boom",
    "bind",
    "shield",
    "fanfare",
];

/// Seconds a decoded clip is kept after it was last wanted.
const KEEP_S: f64 = 60.0;

enum Slot {
    Loading,
    Ready(Sound, f64),
    Failed,
}

/// Clips decoded on demand by a worker thread, dropped when idle.
pub struct Bank {
    slots: BTreeMap<&'static str, Slot>,
    ask: Sender<&'static str>,
    done: Receiver<(&'static str, EngineResult<Sound>)>,
}

impl Bank {
    pub fn new(clips: &'static [(&'static str, &'static [u8])]) -> Self {
        let (ask, asks) = channel::<&'static str>();
        let (tx, done) = channel();
        std::thread::Builder::new()
            .name("sound-decode".into())
            .spawn(move || {
                while let Ok(name) = asks.recv() {
                    let Some((_, bytes)) = clips.iter().find(|(n, _)| *n == name) else {
                        continue;
                    };
                    if tx.send((name, Sound::from_bytes(bytes.to_vec()))).is_err() {
                        break;
                    }
                }
            })
            .expect("sound thread");
        Self {
            slots: BTreeMap::new(),
            ask,
            done,
        }
    }

    /// The clip if decoded (asks for it if not), marked as wanted `now`.
    pub fn get(&mut self, name: &'static str, now: f64) -> Option<Sound> {
        while let Ok((n, s)) = self.done.try_recv() {
            let slot = match s {
                Ok(s) => Slot::Ready(s, now),
                Err(e) => {
                    eprintln!("sound {n}: {e}");
                    Slot::Failed
                }
            };
            self.slots.insert(n, slot);
        }
        match self.slots.get_mut(name) {
            Some(Slot::Ready(s, at)) => {
                *at = now;
                Some(s.clone())
            }
            Some(_) => None,
            None => {
                self.slots.insert(name, Slot::Loading);
                let _ = self.ask.send(name);
                None
            }
        }
    }

    /// Forgets clips nobody wanted for [`KEEP_S`] (a playing voice holds
    /// its own copy of the samples until it ends).
    pub fn sweep(&mut self, now: f64) {
        self.slots
            .retain(|_, s| !matches!(s, Slot::Ready(_, at) if now - *at > KEEP_S));
    }

    /// Clips decoded now (for the memory checks).
    pub fn held(&self) -> usize {
        self.slots
            .values()
            .filter(|s| matches!(s, Slot::Ready(..)))
            .count()
    }
}

/// Where below the ground the listener is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Under {
    #[default]
    No,
    Tunnel,
    Molehaven,
}

/// The kind of town round a spot, for the music.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum District {
    Downtown,
    OldTown,
    #[default]
    Town,
    Country,
}

/// Is `eye` in the tunnels or Molehaven?
pub fn under(w: &TownWorld, eye: Vec3) -> Under {
    let p = DVec2::new(eye.x as f64, eye.z as f64);
    if (eye.y as f64) > w.ground(p) - 1.5 {
        return Under::No;
    }
    let chambers = w
        .underground()
        .cavern
        .as_ref()
        .map_or(&[][..], |c| c.chambers.as_slice());
    let mut found = Under::No;
    for (i, _) in w.underground().at(p) {
        if chambers.contains(&i) {
            return Under::Molehaven;
        }
        found = Under::Tunnel;
    }
    found
}

/// What kind of town `p` stands in: downtown near a business centre (or
/// the newer core), the old town, the rest of the town, or the country.
pub fn district(w: &TownWorld, p: DVec2) -> District {
    let t = &w.town;
    let Some(piece) = t
        .pieces
        .iter()
        .find(|pc| hw_gen::urban::contains(&pc.face.pts, p))
    else {
        return District::Country;
    };
    if piece.era == Era::Old {
        return District::OldTown;
    }
    let near = t.growth.centres.iter().any(|c| match c.kind {
        CentreKind::Business => c.pos.distance(p) < 600.0,
        CentreKind::Core => c.pos.distance(p) < 380.0,
        CentreKind::Village => false,
    });
    if near {
        District::Downtown
    } else {
        District::Town
    }
}

/// What the soundscape asks of the town round the camera: looked up on the
/// traffic worker each frame (see the game's frame loop), as it only reads.
#[derive(Default)]
pub struct Survey {
    /// Vehicles within 90 m, nearest first.
    pub cars: Vec<Vehicle>,
    pub under: Under,
    pub district: District,
}

/// What the soundscape hears each frame.
pub struct Scene<'a> {
    pub now: f64,
    pub dt: f32,
    pub eye: Vec3,
    /// The local hero's feet, if one is played.
    pub hero: Option<Vec3>,
    pub flying: bool,
    /// The hero's speed (m/s).
    pub speed: f32,
    /// Metres from the camera down to the ground.
    pub height: f32,
    pub indoors: bool,
    pub under: Under,
    pub hour: f32,
    pub district: District,
    pub around: Option<Surroundings>,
    /// People within about 35 m.
    pub people: usize,
    /// Vehicles near the camera.
    pub cars: &'a [Vehicle],
    /// Alarms now: id, kind, where.
    pub alarms: Vec<(u32, AlarmKind, Vec3)>,
    /// Ambulances waiting at rescues.
    pub medics: Vec<Vec3>,
    /// The town's old core, where the clock tower stands.
    pub clock: Option<Vec3>,
    /// A boss's music is on: the scenery's music keeps quiet.
    pub boss_music: bool,
    /// The Tesla tower's hum and bolts.
    pub tesla: crate::teslaview::Heard,
    /// The airship mooring mast: the liner's engines, its horn, the winch.
    pub mast: crate::mastview::Heard,
    /// The rocket port's roar, hiss and moments.
    pub rocket: crate::rocketview::Heard,
    /// The atomic plant's hum, steam and relief valve.
    pub atom: crate::atomview::Heard,
    /// The mole folk's drill rig at work and its geyser.
    pub drill: crate::drillview::Heard,
    /// The weather tower's machinery, cannons, rain and thunder.
    pub weather: crate::weatherview::Heard,
}

/// The ambience layers and how loud each one plays at full share.
const LAYERS: [(&str, f32); 9] = [
    ("amb_city_day", 0.6),
    ("amb_city_night", 0.55),
    ("amb_park", 0.55),
    ("amb_crowd", 0.5),
    ("amb_interior", 0.5),
    ("amb_wind_flight", 0.6),
    ("amb_tunnel", 0.55),
    ("amb_molehaven", 0.6),
    ("amb_villains", 0.55),
];

/// Engines heard at once (the nearest), and how far.
const CAR_VOICES: usize = 4;
const CAR_RANGE: f64 = 40.0;
const FLIER_VOICES: usize = 2;
const FLIER_RANGE: f64 = 70.0;

struct Layer {
    level: f32,
    voice: Option<SoundId>,
}

struct Engine {
    voice: SoundId,
    clip: &'static str,
}

/// What we know of a car from frame to frame.
#[derive(Default)]
struct CarMemory {
    speed: f64,
    stopped: f64,
    honked_at: f64,
    skid_at: f64,
    flyby_at: f64,
    seen: f64,
}

/// A sound to play later (a siren arriving after the alarm).
struct Later {
    at: f64,
    clip: &'static str,
    pos: Vec3,
    seconds: f64,
}

/// The town's sounds and music.
pub struct Soundscape {
    bank: Bank,
    layers: Vec<Layer>,
    engines: BTreeMap<u64, Engine>,
    cars: BTreeMap<u64, CarMemory>,
    alarms_heard: BTreeSet<u32>,
    medics_heard: Vec<Vec3>,
    later: Vec<Later>,
    /// Sirens and such playing for a while: voice, stop time.
    timed: Vec<(SoundId, f64)>,
    stride: f32,
    step_n: u32,
    /// Next time for each small life sound.
    next: BTreeMap<&'static str, f64>,
    rng: u64,
    /// The Tesla tower's hum.
    tesla_hum: Option<SoundId>,
    cracks: u32,
    /// The liner's engines and the mast's winch.
    mast_loops: [Option<SoundId>; 2],
    /// The rocket's engine and its venting.
    rocket_roar: Option<SoundId>,
    rocket_vent: Option<SoundId>,
    /// The atomic plant's hum and its towers' steam.
    atom_loops: [Option<SoundId>; 2],
    /// The drill rig's grind.
    drill_grind: Option<SoundId>,
    /// The weather tower's whirr and rain, and thunder still on its way
    /// (when it is heard, where the flash was).
    wx_whirr: Option<SoundId>,
    wx_rain: Option<SoundId>,
    wx_thunder: Vec<(f64, Vec3)>,
    pub music: Music,
}

impl Default for Soundscape {
    fn default() -> Self {
        Self::new()
    }
}

impl Soundscape {
    pub fn new() -> Self {
        Self {
            bank: Bank::new(CLIPS),
            layers: LAYERS
                .iter()
                .map(|_| Layer {
                    level: 0.0,
                    voice: None,
                })
                .collect(),
            engines: BTreeMap::new(),
            cars: BTreeMap::new(),
            alarms_heard: BTreeSet::new(),
            medics_heard: Vec::new(),
            later: Vec::new(),
            timed: Vec::new(),
            stride: 0.0,
            step_n: 0,
            next: BTreeMap::new(),
            rng: 0x9e37_79b9_7f4a_7c15,
            tesla_hum: None,
            cracks: 0,
            mast_loops: [None, None],
            rocket_roar: None,
            rocket_vent: None,
            atom_loops: [None, None],
            drill_grind: None,
            wx_whirr: None,
            wx_rain: None,
            wx_thunder: Vec::new(),
            music: Music::new(),
        }
    }

    /// A random number in 0..1 (sounds need no seed: nothing in the
    /// world depends on them).
    fn rand(&mut self) -> f64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Decoded clips held now (for the memory checks).
    pub fn clips_held(&self) -> usize {
        self.bank.held()
    }

    pub fn update(&mut self, world: &mut World, s: &Scene) {
        self.ambience(world, s);
        self.traffic(world, s);
        self.trouble(world, s);
        self.footsteps(world, s);
        self.life(world, s);
        self.tesla(world, s);
        self.mast(world, s);
        self.rocket(world, s);
        self.atom(world, s);
        self.drill(world, s);
        self.weather(world, s);
        self.timed.retain(|(id, until)| {
            if s.now >= *until {
                world.stop_sound(*id, 2.5);
                false
            } else {
                true
            }
        });
        self.bank.sweep(s.now);
        self.music.update(world, s);
    }

    /// Stops every sound this made (leaving the game for the title).
    pub fn stop(&mut self, world: &mut World) {
        for l in &mut self.layers {
            if let Some(v) = l.voice.take() {
                world.stop_sound(v, 1.0);
            }
            l.level = 0.0;
        }
        for (_, e) in std::mem::take(&mut self.engines) {
            world.stop_sound(e.voice, 0.5);
        }
        for (id, _) in std::mem::take(&mut self.timed) {
            world.stop_sound(id, 0.5);
        }
        for v in [
            self.tesla_hum.take(),
            self.wx_whirr.take(),
            self.wx_rain.take(),
        ]
        .into_iter()
        .flatten()
        {
            world.stop_sound(v, 0.5);
        }
        for v in [self.rocket_roar.take(), self.rocket_vent.take()]
            .into_iter()
            .flatten()
        {
            world.stop_sound(v, 0.5);
        }
        for v in self.mast_loops.iter_mut().filter_map(Option::take) {
            world.stop_sound(v, 0.5);
        }
        for v in self.atom_loops.iter_mut().filter_map(Option::take) {
            world.stop_sound(v, 0.5);
        }
        if let Some(v) = self.drill_grind.take() {
            world.stop_sound(v, 0.5);
        }
        self.wx_thunder.clear();
        self.music.stop(world);
    }

    fn ambience(&mut self, world: &mut World, s: &Scene) {
        let day = daylight(s.hour);
        let night = 1.0 - day;
        let high = 1.0 - smoothstep(30.0, 400.0, s.height);
        let outside = s.under == Under::No && !s.indoors;
        let open = if s.under != Under::No {
            0.0
        } else if s.indoors {
            0.25
        } else {
            1.0
        };
        let a = s.around.unwrap_or_default();
        let built = 0.35 + 0.65 * ((a.built + a.roads) * 1.4).min(1.0);
        let park = (a.park * 2.2).min(1.0);
        let crowd = (s.people as f32 / 22.0).min(1.0);
        let wind = if s.flying {
            (s.speed / 40.0).clamp(0.12, 1.0)
        } else {
            (s.height / 300.0).clamp(0.0, 0.4)
        };
        let targets = [
            day * built * high * open * (1.0 - 0.5 * park),
            night * built * high * open * (1.0 - 0.5 * park),
            park * high * open * (0.3 + 0.7 * day),
            crowd * high * if outside { 1.0 } else { 0.2 },
            if s.indoors { 1.0 } else { 0.0 },
            if s.under == Under::No { wind } else { 0.0 },
            if s.under == Under::Tunnel { 1.0 } else { 0.0 },
            if s.under == Under::Molehaven {
                1.0
            } else {
                0.0
            },
            (a.villains * 1.5).min(1.0) * high * open,
        ];
        let ease = 1.0 - (-s.dt / 1.2).exp();
        for (k, target) in targets.into_iter().enumerate() {
            let (name, gain) = LAYERS[k];
            let l = &mut self.layers[k];
            l.level += (target - l.level) * ease;
            if l.level > 0.003 {
                if l.voice.is_none() {
                    if let Some(clip) = self.bank.get(name, s.now) {
                        let start = PlaySound::ambient()
                            .looped()
                            .volume(0.0)
                            .bus(Bus::Ambience)
                            .fade_in(1.5);
                        l.voice = Some(world.play_sound(&clip, start));
                    }
                } else {
                    let _ = self.bank.get(name, s.now);
                }
                if let Some(v) = l.voice {
                    world.audio_mut().set_volume(v, l.level * gain);
                }
            } else if let Some(v) = l.voice.take() {
                world.stop_sound(v, 0.5);
            }
        }
    }

    fn traffic(&mut self, world: &mut World, s: &Scene) {
        let eye = s.eye.as_dvec3();
        let muffle = if s.indoors {
            0.3
        } else if s.under != Under::No {
            0.0
        } else {
            1.0
        };
        // The nearest street cars and fliers get an engine.
        let mut street: Vec<&Vehicle> = s
            .cars
            .iter()
            .filter(|v| !flies(v) && v.pos.distance(eye) < CAR_RANGE)
            .collect();
        street.sort_by(|a, b| a.pos.distance(eye).total_cmp(&b.pos.distance(eye)));
        let mut fliers: Vec<&Vehicle> = s
            .cars
            .iter()
            .filter(|v| flies(v) && v.pos.distance(eye) < FLIER_RANGE)
            .collect();
        fliers.sort_by(|a, b| a.pos.distance(eye).total_cmp(&b.pos.distance(eye)));
        let heard: Vec<&Vehicle> = street
            .into_iter()
            .take(CAR_VOICES)
            .chain(fliers.into_iter().take(FLIER_VOICES))
            .collect();
        let keep: BTreeSet<u64> = heard.iter().map(|v| v.id).collect();
        let gone: Vec<u64> = self
            .engines
            .keys()
            .filter(|id| !keep.contains(id))
            .copied()
            .collect();
        for id in gone {
            if let Some(e) = self.engines.remove(&id) {
                world.stop_sound(e.voice, 0.6);
            }
        }
        for v in &heard {
            let hover = flies(v) || matches!(v.kind, VehicleKind::Car(CarModel::Hover, _));
            let clip = if hover {
                "hover_hum"
            } else if v.speed < 1.0 {
                "v8_idle"
            } else {
                "v8_cruise"
            };
            let at = (v.pos + DVec3::Y * 0.8).as_vec3();
            let (vol, pitch) = if hover {
                (0.5, 0.85 + (v.speed / 40.0).min(0.5) as f32)
            } else if v.speed < 1.0 {
                (0.3, 1.0)
            } else {
                (
                    0.32 + (v.speed / 14.0).min(1.0) as f32 * 0.3,
                    0.8 + (v.speed / 18.0).min(0.6) as f32,
                )
            };
            if self.engines.get(&v.id).is_some_and(|e| e.clip != clip) {
                if let Some(e) = self.engines.remove(&v.id) {
                    world.stop_sound(e.voice, 0.4);
                }
            }
            match self.engines.get(&v.id) {
                Some(e) => {
                    let a = world.audio_mut();
                    a.set_position(e.voice, at);
                    a.set_volume(e.voice, vol * muffle);
                    a.set_pitch(e.voice, pitch);
                }
                None => {
                    if let Some(c) = self.bank.get(clip, s.now) {
                        let range = if hover { FLIER_RANGE } else { CAR_RANGE } as f32;
                        let p = PlaySound::at(at)
                            .looped()
                            .volume(vol * muffle)
                            .pitch(pitch)
                            .range(3.0, range)
                            .fade_in(0.6);
                        let voice = world.play_sound(&c, p);
                        self.engines.insert(v.id, Engine { voice, clip });
                    }
                }
            }
        }
        // Horns from cars held up, tyres from hard stops, hover cars
        // whooshing past.
        let dt = s.dt as f64;
        for v in s.cars {
            let d = v.pos.distance(eye);
            let m = self.cars.entry(v.id).or_default();
            let decel = (m.speed - v.speed) / dt.max(1e-3);
            let first = m.seen == 0.0;
            m.seen = s.now;
            m.stopped = if v.speed < 0.5 { m.stopped + dt } else { 0.0 };
            let mut play: Option<(&'static str, f32, f32)> = None;
            if !flies(v) {
                if m.stopped > 6.0 && d < 60.0 && s.now - m.honked_at > 25.0 {
                    m.honked_at = s.now;
                    m.stopped = 0.0;
                    play = Some(("horn", 0.8, 60.0));
                } else if !first
                    && decel > 7.0
                    && m.speed > 6.0
                    && d < 50.0
                    && s.now - m.skid_at > 10.0
                {
                    m.skid_at = s.now;
                    play = Some(("skid", 0.9, 60.0));
                }
            } else if d < 18.0 && v.speed > 8.0 && s.now - m.flyby_at > 15.0 {
                m.flyby_at = s.now;
                play = Some(("flyby", 0.9, 80.0));
            }
            m.speed = v.speed;
            if let Some((clip, vol, range)) = play {
                let clip = if clip == "horn" {
                    // Which horn: by the car, so one car always sounds the same.
                    ["horn_1", "horn_2", "horn_long"][(v.id % 3) as usize]
                } else {
                    clip
                };
                if let Some(c) = self.bank.get(clip, s.now) {
                    let p = PlaySound::at(v.pos.as_vec3() + Vec3::Y)
                        .volume(vol * muffle.max(0.2))
                        .range(4.0, range);
                    world.play_sound(&c, p);
                }
            }
        }
        let now = s.now;
        self.cars.retain(|_, m| now - m.seen < 5.0);
    }

    fn trouble(&mut self, world: &mut World, s: &Scene) {
        let eye = s.eye;
        let known: BTreeSet<u32> = s.alarms.iter().map(|a| a.0).collect();
        for &(id, kind, at) in &s.alarms {
            if !self.alarms_heard.insert(id) || at.distance(eye) > 350.0 {
                continue;
            }
            let (clip, vol, range) = match kind {
                AlarmKind::Robbery => ("alarm_bell", 0.8, 120.0),
                AlarmKind::Mugging | AlarmKind::Fight => ("crowd_gasp", 0.8, 40.0),
                AlarmKind::Collapse | AlarmKind::Explosion => ("scream_crowd_1", 0.9, 90.0),
                AlarmKind::Warning => continue,
            };
            if let Some(c) = self.bank.get(clip, s.now) {
                world.play_sound(&c, PlaySound::at(at).volume(vol).range(6.0, range));
            }
            if !matches!(kind, AlarmKind::Warning | AlarmKind::Fight) {
                // Help is on its way: a siren a street or two off, a little later.
                let off =
                    Vec3::new(self.rand() as f32 - 0.5, 0.0, self.rand() as f32 - 0.5) * 120.0;
                let wait = 6.0 + self.rand() * 6.0;
                self.later.push(Later {
                    at: s.now + wait,
                    clip: "siren",
                    pos: at + off,
                    seconds: 14.0,
                });
            }
        }
        self.alarms_heard.retain(|id| known.contains(id));
        for &m in &s.medics {
            if self.medics_heard.iter().any(|h| h.distance(m) < 5.0) {
                continue;
            }
            self.medics_heard.push(m);
            if m.distance(eye) < 400.0 {
                self.later.push(Later {
                    at: s.now,
                    clip: "siren",
                    pos: m,
                    seconds: 9.0,
                });
            }
        }
        self.medics_heard
            .retain(|h| s.medics.iter().any(|m| m.distance(*h) < 5.0));
        let mut i = 0;
        while i < self.later.len() {
            if self.later[i].at <= s.now {
                let l = self.later.swap_remove(i);
                if let Some(c) = self.bank.get(l.clip, s.now) {
                    let p = PlaySound::at(l.pos)
                        .looped()
                        .volume(0.9)
                        .range(10.0, 400.0)
                        .fade_in(2.0);
                    let id = world.play_sound(&c, p);
                    self.timed.push((id, s.now + l.seconds));
                } else if s.now - l.at < 3.0 {
                    // Not decoded yet: try again shortly.
                    self.later.push(Later {
                        at: s.now + 0.3,
                        ..l
                    });
                }
            } else {
                i += 1;
            }
        }
    }

    /// The weather tower: its machinery whirring at the crown (faster in
    /// the storm), a whoomp for each cannon shot, the rain's rush (all
    /// round the listener under the shower), thunder rolling in at the
    /// speed of sound after each flash, and a sparkle as the rainbow comes
    /// out.
    fn weather(&mut self, world: &mut World, s: &Scene) {
        let w = &s.weather;
        let muffle = if s.under != Under::No {
            0.0
        } else if s.indoors {
            0.4
        } else {
            1.0
        };
        let whirr = w
            .whirr
            .filter(|at| muffle > 0.0 && at.distance(s.eye) < 160.0)
            .map(|at| (at, (0.35 + 0.35 * w.spin) * muffle, 0.85 + 0.35 * w.spin));
        keep_loop(
            world,
            &mut self.bank,
            &mut self.wx_whirr,
            "wx_whirr",
            whirr,
            (8.0, 150.0),
            s.now,
        );
        let rain = w
            .rain
            .filter(|(at, k)| muffle > 0.0 && *k > 0.02 && at.distance(s.eye) < 260.0)
            .map(|(at, k)| (at, (0.25 + 0.75 * k) * muffle, 1.0));
        keep_loop(
            world,
            &mut self.bank,
            &mut self.wx_rain,
            "wx_rain",
            rain,
            (25.0, 250.0),
            s.now,
        );
        if muffle > 0.0 {
            for &at in &w.puffs {
                if at.distance(s.eye) > 300.0 {
                    continue;
                }
                if let Some(c) = self.bank.get("wx_puff", s.now) {
                    let pitch = 0.85 + 0.3 * self.rand() as f32;
                    world.play_sound(
                        &c,
                        PlaySound::at(at)
                            .volume(0.8 * muffle)
                            .pitch(pitch)
                            .range(12.0, 300.0),
                    );
                }
            }
            if let Some(at) = w.bow {
                if at.distance(s.eye) < 400.0 {
                    if let Some(c) = self.bank.get("wx_rainbow", s.now) {
                        world.play_sound(
                            &c,
                            PlaySound::at(at).volume(0.7 * muffle).range(60.0, 400.0),
                        );
                    }
                }
            }
        }
        for &at in &w.thunder {
            // Sound takes its time: a far flash rolls in late.
            self.wx_thunder
                .push((s.now + at.distance(s.eye) as f64 / 340.0, at));
        }
        let mut i = 0;
        while i < self.wx_thunder.len() {
            let (when, at) = self.wx_thunder[i];
            if when > s.now {
                i += 1;
                continue;
            }
            self.wx_thunder.swap_remove(i);
            if muffle <= 0.0 || at.distance(s.eye) > 1500.0 {
                continue;
            }
            self.cracks = self.cracks.wrapping_add(1);
            let clip = ["wx_thunder_1", "wx_thunder_2"][(self.cracks % 2) as usize];
            if let Some(c) = self.bank.get(clip, s.now) {
                let pitch = 0.85 + 0.25 * self.rand() as f32;
                world.play_sound(
                    &c,
                    PlaySound::at(at)
                        .volume(muffle)
                        .pitch(pitch)
                        .range(120.0, 1500.0),
                );
            }
        }
    }

    /// The Tesla tower: a crackling hum from the dome that rises with its
    /// charge, a thunderclap for each bolt onto a rod, a zap for each arc
    /// into the yard.
    fn tesla(&mut self, world: &mut World, s: &Scene) {
        let muffle = if s.under != Under::No {
            0.0
        } else if s.indoors {
            0.35
        } else {
            1.0
        };
        match s.tesla.hum {
            Some(at) if muffle > 0.0 && at.distance(s.eye) < 320.0 => {
                let vol = (0.45 + 0.4 * s.tesla.charge) * muffle;
                let pitch = 0.9 + 0.25 * s.tesla.charge;
                match self.tesla_hum {
                    Some(v) => {
                        let _ = self.bank.get("tesla_buzz", s.now);
                        let a = world.audio_mut();
                        a.set_position(v, at);
                        a.set_volume(v, vol);
                        a.set_pitch(v, pitch);
                    }
                    None => {
                        if let Some(c) = self.bank.get("tesla_buzz", s.now) {
                            let p = PlaySound::at(at)
                                .looped()
                                .volume(vol)
                                .pitch(pitch)
                                .range(12.0, 300.0)
                                .fade_in(1.0);
                            self.tesla_hum = Some(world.play_sound(&c, p));
                        }
                    }
                }
            }
            _ => {
                if let Some(v) = self.tesla_hum.take() {
                    world.stop_sound(v, 1.0);
                }
            }
        }
        for &(at, power) in &s.tesla.strikes {
            if muffle <= 0.0 {
                continue;
            }
            let (clip, range) = if power >= 0.9 {
                self.cracks = self.cracks.wrapping_add(1);
                (
                    ["tesla_crack_1", "tesla_crack_2"][(self.cracks % 2) as usize],
                    700.0,
                )
            } else {
                ("tesla_zap", 120.0)
            };
            if at.distance(s.eye) > range {
                continue;
            }
            if let Some(c) = self.bank.get(clip, s.now) {
                let pitch = 0.85 + 0.3 * self.rand() as f32;
                let p = PlaySound::at(at)
                    .volume(power.max(0.5) * muffle)
                    .pitch(pitch)
                    .range(15.0, range);
                world.play_sound(&c, p);
            }
        }
    }

    /// The atomic plant: the turbine hall's deep hum, the rush of steam
    /// round the cooling towers, and the roar of the relief valve.
    fn atom(&mut self, world: &mut World, s: &Scene) {
        let muffle = if s.under != Under::No {
            0.0
        } else if s.indoors {
            0.35
        } else {
            1.0
        };
        let loops = [
            ("atom_hum", s.atom.hum, 0.7, 200.0),
            ("atom_steam", s.atom.steam, 0.8, 260.0),
        ];
        for (k, (clip, at, loud, range)) in loops.into_iter().enumerate() {
            match at {
                Some(at) if muffle > 0.0 && at.distance(s.eye) < range => {
                    let vol = loud * muffle;
                    match self.atom_loops[k] {
                        Some(v) => {
                            let _ = self.bank.get(clip, s.now);
                            let a = world.audio_mut();
                            a.set_position(v, at);
                            a.set_volume(v, vol);
                        }
                        None => {
                            if let Some(c) = self.bank.get(clip, s.now) {
                                let p = PlaySound::at(at)
                                    .looped()
                                    .volume(vol)
                                    .range(15.0, range)
                                    .fade_in(1.5);
                                self.atom_loops[k] = Some(world.play_sound(&c, p));
                            }
                        }
                    }
                }
                _ => {
                    if let Some(v) = self.atom_loops[k].take() {
                        world.stop_sound(v, 1.5);
                    }
                }
            }
        }
        for &at in &s.atom.vents {
            if muffle <= 0.0 || at.distance(s.eye) > 500.0 {
                continue;
            }
            self.cracks = self.cracks.wrapping_add(1);
            let clip = ["atom_vent_1", "atom_vent_2"][(self.cracks % 2) as usize];
            if let Some(c) = self.bank.get(clip, s.now) {
                let pitch = 0.9 + 0.2 * self.rand() as f32;
                let p = PlaySound::at(at)
                    .volume(0.9 * muffle)
                    .pitch(pitch)
                    .range(20.0, 500.0);
                world.play_sound(&c, p);
            }
        }
    }

    /// The airship: the deep drone of its engines (louder as they work),
    /// a horn on the glide in and as it casts off, the latch clanking,
    /// the winch whirring while crates come down.
    fn mast(&mut self, world: &mut World, s: &Scene) {
        let muffle = if s.under != Under::No {
            0.0
        } else if s.indoors {
            0.35
        } else {
            1.0
        };
        let m = &s.mast;
        let loops = [
            (
                "airship_drone",
                m.engines
                    .map(|(p, k)| (p, (0.35 + 0.65 * k) * muffle, 0.85 + 0.25 * k)),
                (25.0, 900.0),
            ),
            (
                "winch",
                m.winch.map(|p| (p, 0.7 * muffle, 1.0)),
                (6.0, 160.0),
            ),
        ];
        for (k, (clip, want, (near, far))) in loops.into_iter().enumerate() {
            match want {
                Some((at, vol, pitch)) if vol > 0.0 && at.distance(s.eye) < far => {
                    match self.mast_loops[k] {
                        Some(v) => {
                            let _ = self.bank.get(clip, s.now);
                            let a = world.audio_mut();
                            a.set_position(v, at);
                            a.set_volume(v, vol);
                            a.set_pitch(v, pitch);
                        }
                        None => {
                            if let Some(c) = self.bank.get(clip, s.now) {
                                let p = PlaySound::at(at)
                                    .looped()
                                    .volume(vol)
                                    .pitch(pitch)
                                    .range(near, far)
                                    .fade_in(1.5);
                                self.mast_loops[k] = Some(world.play_sound(&c, p));
                            }
                        }
                    }
                }
                _ => {
                    if let Some(v) = self.mast_loops[k].take() {
                        world.stop_sound(v, 1.5);
                    }
                }
            }
        }
        if muffle <= 0.0 {
            return;
        }
        let shots = m
            .horns
            .iter()
            .map(|p| ("airship_horn", *p, 1.0, 1500.0))
            .chain(m.clanks.iter().map(|p| ("mooring_clank", *p, 0.8, 220.0)));
        for (clip, at, vol, range) in shots.collect::<Vec<_>>() {
            if at.distance(s.eye) > range {
                continue;
            }
            if let Some(c) = self.bank.get(clip, s.now) {
                let p = PlaySound::at(at).volume(vol * muffle).range(30.0, range);
                world.play_sound(&c, p);
            }
        }
    }

    /// A looping voice that follows `at` while it is wanted: started,
    /// moved, its volume and pitch set, and faded out when not.
    #[allow(clippy::too_many_arguments)]
    fn follow(
        &mut self,
        world: &mut World,
        voice: Option<SoundId>,
        clip: &'static str,
        at: Option<Vec3>,
        vol: f32,
        pitch: f32,
        range: (f32, f32),
        now: f64,
    ) -> Option<SoundId> {
        match (voice, at) {
            (Some(v), Some(p)) => {
                let _ = self.bank.get(clip, now);
                let a = world.audio_mut();
                a.set_position(v, p);
                a.set_volume(v, vol);
                a.set_pitch(v, pitch);
                Some(v)
            }
            (None, Some(p)) => self.bank.get(clip, now).map(|c| {
                let play = PlaySound::at(p)
                    .looped()
                    .volume(vol)
                    .pitch(pitch)
                    .range(range.0, range.1)
                    .fade_in(0.6);
                world.play_sound(&c, play)
            }),
            (Some(v), None) => {
                world.stop_sound(v, 1.2);
                None
            }
            (None, None) => None,
        }
    }

    /// The rocket port: the engine's roar (heard across the town, rising
    /// and falling with the thrust), the hiss of venting, the klaxon, the
    /// count's beeps, the ignition and the touchdown.
    fn rocket(&mut self, world: &mut World, s: &Scene) {
        let muffle = if s.under != Under::No {
            0.15
        } else if s.indoors {
            0.4
        } else {
            1.0
        };
        let r = &s.rocket;
        let roar = r
            .roar
            .filter(|(at, _)| at.distance(s.eye) < 3000.0)
            .map(|(at, _)| at);
        let thrust = r.roar.map_or(0.0, |(_, k)| k);
        let v = self.rocket_roar.take();
        self.rocket_roar = self.follow(
            world,
            v,
            "rocket_roar",
            roar,
            (0.4 + 0.6 * thrust) * muffle,
            0.85 + 0.2 * thrust,
            (40.0, 3000.0),
            s.now,
        );
        let vent = r.vent.filter(|at| at.distance(s.eye) < 220.0);
        let v = self.rocket_vent.take();
        self.rocket_vent = self.follow(
            world,
            v,
            "rocket_vent",
            vent,
            // The clip is mastered quiet (its pops limit it).
            0.9 * muffle,
            1.0,
            (10.0, 220.0),
            s.now,
        );
        for &(shot, at) in &r.shots {
            let (clip, vol, range, pitch) = match shot {
                crate::rocketview::Shot::Klaxon => ("rocket_klaxon", 0.9, 900.0, 1.0),
                // The last three beeps a little higher.
                crate::rocketview::Shot::Beep(n) => {
                    ("rocket_beep", 0.7, 400.0, if n <= 3 { 1.25 } else { 1.0 })
                }
                crate::rocketview::Shot::Ignition => ("rocket_ignite", 1.0, 3000.0, 1.0),
                crate::rocketview::Shot::Touchdown => ("rocket_touchdown", 1.0, 1200.0, 1.0),
            };
            if at.distance(s.eye) > range {
                continue;
            }
            if let Some(c) = self.bank.get(clip, s.now) {
                let p = PlaySound::at(at)
                    .volume(vol * muffle)
                    .pitch(pitch)
                    .range(20.0, range);
                world.play_sound(&c, p);
            }
        }
    }

    /// The drill rig: a grinding, chuffing loop that works harder as the
    /// pressure builds; a whistle and a rumble when it does; the geyser's
    /// roar when it blows. Heard faintly down in Molehaven too.
    fn drill(&mut self, world: &mut World, s: &Scene) {
        let muffle = if s.under != Under::No {
            0.3
        } else if s.indoors {
            0.35
        } else {
            1.0
        };
        let Some(at) = s.drill.at else {
            if let Some(v) = self.drill_grind.take() {
                world.stop_sound(v, 1.0);
            }
            return;
        };
        if at.distance(s.eye) < 260.0 {
            let vol = (0.4 + 0.45 * s.drill.effort) * muffle;
            let pitch = 0.85 + 0.3 * s.drill.effort;
            match self.drill_grind {
                Some(v) => {
                    let _ = self.bank.get("drill_grind", s.now);
                    let a = world.audio_mut();
                    a.set_volume(v, vol);
                    a.set_pitch(v, pitch);
                }
                None => {
                    if let Some(c) = self.bank.get("drill_grind", s.now) {
                        let p = PlaySound::at(at)
                            .looped()
                            .volume(vol)
                            .pitch(pitch)
                            .range(10.0, 240.0)
                            .fade_in(1.0);
                        self.drill_grind = Some(world.play_sound(&c, p));
                    }
                }
            }
        } else if let Some(v) = self.drill_grind.take() {
            world.stop_sound(v, 1.0);
        }
        let mut shot = |clip: &'static str, vol: f32, range: f32| {
            if at.distance(s.eye) > range {
                return;
            }
            if let Some(c) = self.bank.get(clip, s.now) {
                world.play_sound(
                    &c,
                    PlaySound::at(at).volume(vol * muffle).range(15.0, range),
                );
            }
        };
        if s.drill.rumbled {
            shot("steam_whistle", 0.9, 800.0);
            shot("geyser_rumble", 0.9, 450.0);
        }
        if s.drill.burst {
            shot("geyser_blast", 1.0, 700.0);
        }
    }

    fn footsteps(&mut self, world: &mut World, s: &Scene) {
        let Some(feet) = s.hero else {
            return;
        };
        if s.flying || s.speed < 0.6 || feet.distance(s.eye) > 25.0 {
            self.stride = 0.0;
            return;
        }
        self.stride += s.speed * s.dt;
        let stride = 0.7 + s.speed * 0.07;
        if self.stride < stride {
            return;
        }
        self.stride -= stride;
        self.step_n = self.step_n.wrapping_add(1);
        let green = s.around.is_some_and(|a| a.park > 0.5) && !s.indoors;
        let clip = if green {
            ["step_grass_1", "step_grass_2"][(self.step_n % 2) as usize]
        } else {
            [
                "step_concrete_1",
                "step_concrete_2",
                "step_concrete_3",
                "step_concrete_4",
            ][(self.step_n % 4) as usize]
        };
        let pitch = 0.92 + self.rand() as f32 * 0.16;
        if let Some(c) = self.bank.get(clip, s.now) {
            let vol = 0.35 + (s.speed / 12.0).min(0.4);
            world.play_sound(
                &c,
                PlaySound::at(feet)
                    .volume(vol)
                    .pitch(pitch)
                    .range(2.0, 25.0),
            );
        }
    }

    /// Is it time for `what` (first call schedules it `a..b` s ahead)?
    fn due(&mut self, what: &'static str, now: f64, a: f64, b: f64) -> bool {
        let r = self.rand();
        let next = *self.next.entry(what).or_insert(now + a + (b - a) * r);
        if now < next {
            return false;
        }
        let r = self.rand();
        self.next.insert(what, now + a + (b - a) * r);
        true
    }

    /// A spot `a..b` metres from the camera, on the ground.
    fn spot(&mut self, s: &Scene, a: f32, b: f32) -> Vec3 {
        let ang = self.rand() as f32 * std::f32::consts::TAU;
        let r = a + (b - a) * self.rand() as f32;
        Vec3::new(
            s.eye.x + ang.sin() * r,
            s.eye.y - s.height + 1.0,
            s.eye.z + ang.cos() * r,
        )
    }

    fn play_at(
        &mut self,
        world: &mut World,
        s: &Scene,
        clip: &'static str,
        at: Vec3,
        vol: f32,
        range: f32,
    ) {
        if let Some(c) = self.bank.get(clip, s.now) {
            world.play_sound(&c, PlaySound::at(at).volume(vol).range(5.0, range));
        }
    }

    fn life(&mut self, world: &mut World, s: &Scene) {
        if s.under != Under::No {
            return;
        }
        let a = s.around.unwrap_or_default();
        let day = daylight(s.hour) > 0.5;
        let low = s.height < 25.0 && !s.indoors;
        if day && low && a.roads + a.park > 0.25 && self.due("pigeons", s.now, 25.0, 60.0) {
            let at = self.spot(s, 8.0, 25.0);
            self.play_at(world, s, "pigeons", at, 0.6, 45.0);
        }
        if day && low && a.park > 0.3 && self.due("bird_flutter", s.now, 15.0, 40.0) {
            let at = self.spot(s, 6.0, 20.0);
            self.play_at(world, s, "bird_flutter", at, 0.5, 35.0);
        }
        if !day && low && s.district != District::Downtown && self.due("dog_far", s.now, 40.0, 90.0)
        {
            let at = self.spot(s, 50.0, 110.0);
            self.play_at(world, s, "dog_far", at, 0.7, 160.0);
        }
        let busy = matches!(s.district, District::Downtown | District::OldTown) && s.people > 8;
        if day && low && busy && self.due("terrace_clatter", s.now, 30.0, 70.0) {
            let at = self.spot(s, 10.0, 25.0);
            self.play_at(world, s, "terrace_clatter", at, 0.5, 40.0);
        }
        if let Some(clock) = s.clock {
            if clock.distance(s.eye) < 1500.0 && self.due("clock_bell", s.now, 420.0, 720.0) {
                self.play_at(world, s, "clock_bell", clock + Vec3::Y * 30.0, 1.0, 1500.0);
            }
        }
    }
}

fn flies(v: &Vehicle) -> bool {
    v.flies()
}

/// 1 by day, 0 by night, easing through dawn (5 to 7) and dusk (19 to 21).
pub fn daylight(hour: f32) -> f32 {
    smoothstep(5.0, 7.0, hour) * (1.0 - smoothstep(19.0, 21.0, hour))
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// What the music should fit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    DowntownDay,
    DowntownNight,
    TownDay,
    TownEvening,
    NightQuiet,
    Morning,
    Park,
    Waterfront,
    Villains,
    Molehaven,
    Flight,
}

impl Mood {
    /// The pieces written for it (files in `assets/music`).
    pub fn tracks(self) -> &'static [&'static str] {
        match self {
            Mood::DowntownDay => &["downtown_day_1", "downtown_day_2"],
            Mood::DowntownNight => &["downtown_night_1", "downtown_night_2"],
            Mood::TownDay => &["town_day_1", "town_day_2"],
            Mood::TownEvening => &["town_evening"],
            Mood::NightQuiet => &["night_quiet"],
            Mood::Morning => &["morning"],
            Mood::Park => &["park"],
            Mood::Waterfront => &["waterfront"],
            Mood::Villains => &["villains_1", "villains_2"],
            Mood::Molehaven => &["molehaven"],
            Mood::Flight => &["flight_1", "flight_2"],
        }
    }

    /// The scenery's mood: the strongest place first, then the hour.
    pub fn of(s: &Scene) -> Mood {
        let a = s.around.unwrap_or_default();
        let h = s.hour;
        if s.under == Under::Molehaven {
            return Mood::Molehaven;
        }
        if a.villains > 0.3 {
            return Mood::Villains;
        }
        if s.flying && s.height > 40.0 {
            return Mood::Flight;
        }
        let night = !(5.0..20.5).contains(&h);
        let evening = (18.0..20.5).contains(&h);
        let morning = (5.0..8.5).contains(&h);
        if !night && a.water > 0.25 {
            return Mood::Waterfront;
        }
        if !night && a.park > 0.4 {
            return Mood::Park;
        }
        let downtown = s.district == District::Downtown;
        if night {
            return if downtown {
                Mood::DowntownNight
            } else {
                Mood::NightQuiet
            };
        }
        if morning {
            return Mood::Morning;
        }
        if evening {
            return if downtown {
                Mood::DowntownNight
            } else {
                Mood::TownEvening
            };
        }
        if downtown {
            Mood::DowntownDay
        } else {
            Mood::TownDay
        }
    }
}

/// Seconds of quiet before the first piece, and between pieces.
const FIRST_GAP: f64 = 10.0;
const GAP: (f64, f64) = (35.0, 90.0);
/// A new mood must hold this long before the music changes to it.
const MOOD_DWELL: f64 = 12.0;
/// How loud the scenery's music plays (it sits under the world).
const MUSIC_VOLUME: f32 = 0.55;

enum Playing {
    /// Quiet until this time.
    Quiet(f64),
    Loading(Mood, Receiver<EngineResult<Sound>>),
    /// Mood, when it started, when it ends.
    On(Mood, f64, f64),
}

/// Background music that fits the scenery: a piece for where you are,
/// quiet for a while, then another; a strong change of scene (into the
/// villains' quarter, up into the sky) crossfades to a fitting piece.
/// Pieces are read from disk and decoded when they start, one at a time.
pub struct Music {
    state: Playing,
    dir: Option<PathBuf>,
    last: Option<&'static str>,
    /// The mood the scenery has now, and since when.
    mood: Option<(Mood, f64)>,
    on: bool,
    rng: u64,
}

impl Music {
    fn new() -> Self {
        Self {
            state: Playing::Quiet(f64::MIN),
            dir: music_dir(),
            last: None,
            mood: None,
            on: std::env::var("HW_MUSIC").map_or(true, |v| v != "0"),
            rng: 0x2545_f491_4f6c_dd1d,
        }
    }

    /// The piece playing now, if any (for tests and debugging).
    pub fn playing(&self) -> Option<Mood> {
        match self.state {
            Playing::On(m, ..) => Some(m),
            _ => None,
        }
    }

    fn rand(&mut self) -> f64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64 / (1u64 << 53) as f64
    }

    fn stop(&mut self, world: &mut World) {
        if matches!(self.state, Playing::On(..)) {
            world.stop_music(2.0);
        }
        self.state = Playing::Quiet(f64::MIN);
    }

    fn update(&mut self, world: &mut World, s: &Scene) {
        let Some(dir) = self.dir.clone().filter(|_| self.on) else {
            return;
        };
        if s.boss_music {
            // The boss's music owns the track; start again after it.
            self.state = Playing::Quiet(s.now + 20.0);
            return;
        }
        let now = s.now;
        if matches!(self.state, Playing::Quiet(t) if t == f64::MIN) {
            self.state = Playing::Quiet(now + FIRST_GAP);
        }
        let mood = Mood::of(s);
        match self.mood {
            Some((m, _)) if m == mood => {}
            _ => self.mood = Some((mood, now)),
        }
        let settled = self
            .mood
            .is_some_and(|(_, since)| now - since >= MOOD_DWELL);
        match &self.state {
            Playing::Quiet(until) => {
                if now >= *until && settled {
                    self.load(&dir, mood);
                }
            }
            Playing::Loading(m, rx) => {
                let m = *m;
                if let Ok(r) = rx.try_recv() {
                    match r {
                        Ok(sound) => {
                            let len = sound.duration() as f64;
                            let mut track = MusicTrack::new().layer(sound);
                            track.layers[0].volume = MUSIC_VOLUME;
                            world.play_music(track, 5.0);
                            self.state = Playing::On(m, now, now + len);
                        }
                        Err(e) => {
                            eprintln!("music: {e}");
                            self.state = Playing::Quiet(now + 60.0);
                        }
                    }
                }
            }
            Playing::On(m, started, ends) => {
                let (m, started, ends) = (*m, *started, *ends);
                if now >= ends - 6.0 {
                    world.stop_music(6.0);
                    let gap = GAP.0 + (GAP.1 - GAP.0) * self.rand();
                    self.state = Playing::Quiet(now + gap);
                } else if mood != m && settled && now - started > 20.0 && strong(m, mood) {
                    // The old piece plays on while the new one decodes; it
                    // crossfades when it starts.
                    self.load(&dir, mood);
                }
            }
        }
    }

    /// Starts decoding a piece for `mood` (not the one just heard).
    fn load(&mut self, dir: &Path, mood: Mood) {
        let tracks = mood.tracks();
        let mut pick = tracks[(self.rand() * tracks.len() as f64) as usize % tracks.len()];
        if Some(pick) == self.last && tracks.len() > 1 {
            pick = tracks
                .iter()
                .copied()
                .find(|t| Some(*t) != self.last)
                .unwrap_or(pick);
        }
        self.last = Some(pick);
        let path = dir.join(format!("{pick}.ogg"));
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(Sound::load(path));
        });
        self.state = Playing::Loading(mood, rx);
    }
}

/// Is going from `a` to `b` a change of scene worth changing the music
/// for (rather than waiting for the piece to end)?
fn strong(a: Mood, b: Mood) -> bool {
    let special = |m: Mood| matches!(m, Mood::Villains | Mood::Molehaven | Mood::Flight);
    special(a) || special(b)
}

/// Where the music is: `HW_MUSIC_DIR`, else `assets/music` here, in the
/// repository, or next to the game.
pub fn music_dir() -> Option<PathBuf> {
    let has = |p: &Path| p.join("town_day_1.ogg").is_file();
    if let Ok(p) = std::env::var("HW_MUSIC_DIR") {
        let p = PathBuf::from(p);
        return has(&p).then_some(p);
    }
    let mut tries = vec![PathBuf::from("assets/music")];
    tries.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/music"));
    if let Ok(exe) = std::env::current_exe() {
        for up in exe.ancestors().skip(1).take(4) {
            tries.push(up.join("assets/music"));
        }
    }
    tries.into_iter().find(|p| has(p))
}

/// Keeps a looped sound playing at `want` (where, volume, pitch), or
/// fades it out when there is none.
fn keep_loop(
    world: &mut World,
    bank: &mut Bank,
    slot: &mut Option<SoundId>,
    clip: &'static str,
    want: Option<(Vec3, f32, f32)>,
    (near, far): (f32, f32),
    now: f64,
) {
    match (want, *slot) {
        (Some((at, vol, pitch)), Some(v)) => {
            // Keep the clip in the bank while it plays.
            let _ = bank.get(clip, now);
            let a = world.audio_mut();
            a.set_position(v, at);
            a.set_volume(v, vol);
            a.set_pitch(v, pitch);
        }
        (Some((at, vol, pitch)), None) => {
            if let Some(c) = bank.get(clip, now) {
                let p = PlaySound::at(at)
                    .looped()
                    .volume(vol)
                    .pitch(pitch)
                    .range(near, far)
                    .fade_in(1.5);
                *slot = Some(world.play_sound(&c, p));
            }
        }
        (None, Some(v)) => {
            world.stop_sound(v, 1.5);
            *slot = None;
        }
        (None, None) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(cars: &[Vehicle]) -> Scene<'_> {
        Scene {
            now: 0.0,
            dt: 1.0 / 60.0,
            eye: Vec3::ZERO,
            hero: None,
            flying: false,
            speed: 0.0,
            height: 2.0,
            indoors: false,
            under: Under::No,
            hour: 15.5,
            district: District::Town,
            around: None,
            people: 0,
            cars,
            alarms: Vec::new(),
            medics: Vec::new(),
            clock: None,
            boss_music: false,
            tesla: Default::default(),
            mast: Default::default(),
            rocket: Default::default(),
            atom: Default::default(),
            drill: Default::default(),
            weather: Default::default(),
        }
    }

    #[test]
    fn day_and_night() {
        assert_eq!(daylight(12.0), 1.0);
        assert_eq!(daylight(2.0), 0.0);
        assert!(daylight(6.0) > 0.0 && daylight(6.0) < 1.0);
    }

    #[test]
    fn the_music_follows_the_scenery() {
        let mut s = scene(&[]);
        assert_eq!(Mood::of(&s), Mood::TownDay);
        s.district = District::Downtown;
        assert_eq!(Mood::of(&s), Mood::DowntownDay);
        s.hour = 23.0;
        assert_eq!(Mood::of(&s), Mood::DowntownNight);
        s.hour = 15.0;
        s.around = Some(Surroundings {
            villains: 0.8,
            ..Default::default()
        });
        assert_eq!(Mood::of(&s), Mood::Villains);
        s.around = None;
        s.flying = true;
        s.height = 120.0;
        assert_eq!(Mood::of(&s), Mood::Flight);
        s.under = Under::Molehaven;
        assert_eq!(Mood::of(&s), Mood::Molehaven);
    }

    #[test]
    fn every_mood_has_its_pieces_on_disk() {
        let dir = music_dir().expect("assets/music");
        for m in [
            Mood::DowntownDay,
            Mood::DowntownNight,
            Mood::TownDay,
            Mood::TownEvening,
            Mood::NightQuiet,
            Mood::Morning,
            Mood::Park,
            Mood::Waterfront,
            Mood::Villains,
            Mood::Molehaven,
            Mood::Flight,
        ] {
            for t in m.tracks() {
                assert!(dir.join(format!("{t}.ogg")).is_file(), "{t}");
            }
        }
    }

    #[test]
    fn every_clip_decodes() {
        for (name, bytes) in CLIPS.iter().chain(FIGHT_CLIPS) {
            let s = Sound::from_bytes(bytes.to_vec()).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(s.duration() > 0.2, "{name}");
        }
    }

    #[test]
    fn idle_clips_are_forgotten() {
        let mut b = Bank::new(CLIPS);
        let mut t = 0.0;
        while b.get("horn_1", t).is_none() {
            std::thread::sleep(std::time::Duration::from_millis(5));
            t += 0.005;
            assert!(t < 10.0, "decoded in time");
        }
        assert_eq!(b.held(), 1);
        b.sweep(t + KEEP_S + 1.0);
        assert_eq!(b.held(), 0);
    }
}
