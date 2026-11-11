//! Street life on screen (the plan is `hw_sim::streetlife`): performers
//! walking in with their things, setting up, playing to whoever stops,
//! packing up and going; watchers who clap and drop a coin; the night
//! club's queue and its doorman under a neon atom. Each person is an Actors
//! character with their act's own clips (see `poses::add_street_clips`);
//! their things are drawn instanced from `streetgear`. They talk to a hero
//! who comes close, the living statue jumps at one who comes too close,
//! and trouble near breaks up the act: everyone runs, the things are left.
//!
//! Which acts play near the camera is worked out on a worker thread; the
//! people of those few acts (a few dozen at most) are placed here each
//! frame, which is cheap: their parts are planned already.

use crate::chat::{Said, Voice, SAY_RANGE, SHOUT_RANGE};
use crate::people::{People, Who, JOG_MPS, WALK_MPS};
use crate::soundscape::{keep_loop, Bank};
use crate::streetgear::Gear;
use crate::streetlines::{lines, Moment, WATCHER};
use engine::prelude::*;
use glam::{DVec2, DVec3, Vec3, Vec3Swizzles};
use hw_core::Seed;
use hw_gen::urban::area::Area;
use hw_sim::alarms::Alarms;
use hw_sim::citizens::{Citizens, Escape};
use hw_sim::streetlife::{Act, Doing, Role, Show, StreetLife, StreetPerson};
use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::TAU;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

/// Acts are looked for within this distance of the camera...
const REACH: f64 = 110.0;
/// ...and at most this many are shown, the nearest first.
const MOST: usize = 8;
/// People of an act are drawn within this distance of the camera.
const PEOPLE_REACH: f64 = 70.0;
/// A hero this close hears an act's patter...
const HEAR: f64 = 14.0;
/// ...this close gets spoken to...
const GREET: f64 = 6.0;
/// ...and this close makes the living statue jump.
const BOO: f64 = 2.6;
/// How long the statue's start lasts (s).
const STARTLE: f64 = 1.4;
/// How fast people run from trouble (m/s).
const FLEE: f64 = 2.6;
/// How far they run before they are out of the story (m).
const FLEE_MAX: f64 = 70.0;
/// Trouble this much beyond an alarm's own reach still breaks up an act.
const NERVES: f64 = 10.0;

macro_rules! clips {
    ($($name:literal),* $(,)?) => {
        &[$(($name, include_bytes!(concat!("../assets/sound/", $name, ".ogg")) as &[u8])),*]
    };
}

/// The street acts' music: a theremin, the dancers' radio, the club's
/// band through its door.
static MUSIC: &[(&str, &[u8])] = clips![
    "street_theremin",
    "street_radio_swing",
    "street_club_muffled",
];

/// Every thing's levels of detail, as the worker builds them.
type GearModels = Vec<(Gear, Vec<LodLevel>)>;

/// One person on screen.
struct Person {
    e: EntityId,
    who: Who,
    clip: &'static str,
    rate: f32,
}

/// An act broken up by trouble: when, from where, and the way each of
/// them runs (none: they stand back and stare).
struct Broken {
    at: f64,
    danger: DVec2,
    folk: Vec<(StreetPerson, Option<Escape>)>,
    /// Their things stay where they stood (set up at the moment it broke).
    gear: f64,
}

/// What an act has said, so it does not repeat itself too often.
#[derive(Default)]
struct Talk {
    next: f64,
    count: u64,
    greeted: bool,
    packed: bool,
    boo_until: f64,
    boo_next: f64,
    cheer_next: f64,
}

/// A request to the worker: the acts near `at` at `time`.
struct Ask {
    citizens: Arc<Citizens>,
    at: DVec2,
    time: f64,
    hour: f64,
}

pub struct StreetView {
    life: Arc<StreetLife>,
    ask: Option<Sender<Ask>>,
    answers: Option<Receiver<Vec<Arc<Show>>>>,
    waiting: bool,
    refresh: f64,
    shows: Vec<Arc<Show>>,
    shown: BTreeMap<u64, Person>,
    asked: BTreeSet<Who>,
    gear: BTreeMap<Gear, LodId>,
    gear_job: Option<Receiver<GearModels>>,
    broken: BTreeMap<(u64, i64), Broken>,
    talk: BTreeMap<(u64, i64), Talk>,
    bank: Bank,
    music: BTreeMap<(u64, i64), Option<SoundId>>,
    music_near: bool,
    /// What was said this frame: for the floating words and the chat.
    pub said: Vec<Said>,
}

impl StreetView {
    pub fn new(area: Arc<Area>) -> Self {
        Self {
            life: Arc::new(StreetLife::new(area)),
            ask: None,
            answers: None,
            waiting: false,
            refresh: 0.0,
            shows: Vec::new(),
            shown: BTreeMap::new(),
            asked: BTreeSet::new(),
            gear: BTreeMap::new(),
            gear_job: None,
            broken: BTreeMap::new(),
            talk: BTreeMap::new(),
            bank: Bank::new(MUSIC),
            music: BTreeMap::new(),
            music_near: false,
            said: Vec::new(),
        }
    }

    /// Whether a street act's music plays near the camera (the scenery's
    /// own music keeps quiet then).
    pub fn music_near(&self) -> bool {
        self.music_near
    }

    /// How many acts and people are on screen.
    pub fn counts(&self) -> (usize, usize) {
        (self.shows.len(), self.shown.len())
    }

    /// The acts near the camera now (for `HW_VIEW=street`).
    pub fn shows(&self) -> &[Arc<Show>] {
        &self.shows
    }

    /// Where an act of `act` near the camera is played, and the way its
    /// pitch faces (for views).
    pub fn pitch_of(&self, act: Act) -> Option<(Vec3, DVec2)> {
        let show = self.shows.iter().find(|s| s.act == act)?;
        let p = show.pitch(0);
        Some((
            Vec3::new(p.x as f32, show.site.y as f32, p.y as f32),
            show.site.face,
        ))
    }

    /// The nearest act of `act` within `radius` of `around` that is
    /// playing at `time` (`HW_VIEW=act`): its pitch and the way it faces.
    /// On the main thread: a one-off for a view, not for play.
    pub fn find(
        &self,
        citizens: &Citizens,
        act: Act,
        around: DVec2,
        radius: f64,
        time: f64,
    ) -> Option<(Vec3, DVec2)> {
        let shows = self
            .life
            .shows_near(citizens, around, radius, time, citizens.hour());
        let show = shows
            .iter()
            .filter(|s| s.act == act && (s.gear(time) > 0.95 || act == Act::Club))
            .min_by(|a, b| {
                a.site
                    .at
                    .distance(around)
                    .total_cmp(&b.site.at.distance(around))
            })?;
        let p = show.pitch(0);
        Some((
            Vec3::new(p.x as f32, show.site.y as f32, p.y as f32),
            show.site.face,
        ))
    }

    /// Moves the street life near `camera` to `time`.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        world: &mut World,
        citizens: &Arc<Citizens>,
        alarms: &Alarms,
        models: &BTreeMap<Who, Arc<AnimatedModel>>,
        people: Option<&People>,
        camera: DVec2,
        eye: Vec3,
        hero: Option<Vec3>,
        time: f64,
        dt: f64,
    ) -> EngineResult<()> {
        self.load_gear(world)?;
        self.ask_worker(citizens, camera, time, dt)?;
        self.break_up(citizens, alarms, time);

        let mut seen = BTreeSet::new();
        let mut places: BTreeMap<Gear, Vec<Place>> = BTreeMap::new();
        let mut music_near = false;
        let hero2 = hero.map(|h| DVec2::new(h.x as f64, h.z as f64));
        for show in self.shows.clone() {
            let key = show.key();
            let near = show.site.at.distance(camera) < PEOPLE_REACH;
            let folk: Vec<(StreetPerson, bool)> = match self.broken.get(&key) {
                Some(b) => fleeing(citizens, b, time),
                None => show
                    .people(&self.life, citizens, time)
                    .into_iter()
                    .map(|p| (p, false))
                    .collect(),
            };
            // Things set up at the pitch.
            let g = match self.broken.get(&key) {
                Some(b) => b.gear,
                None => show.gear(time),
            };
            set_gear(&show, g, time, &mut places);
            if !near {
                continue;
            }
            let talk = self.talk.entry(key).or_default();
            let startled = talk.boo_until > time;
            for (p, running) in folk {
                if p.pos.xz().distance(camera) > PEOPLE_REACH {
                    continue;
                }
                let who = who_of(p.role);
                let Some(model) = models.get(&who) else {
                    if self.asked.insert(who.clone()) {
                        if let Some(people) = people {
                            people.request(who, 0);
                        }
                    }
                    continue;
                };
                let clip = clip_of(show.act, &p, running, startled);
                let rate = rate_of(clip, p.speed, running);
                let yaw = p.facing.x.atan2(p.facing.y).to_degrees() as f32;
                let place =
                    Place::new(p.pos.x as f32, p.pos.y as f32, p.pos.z as f32).with_yaw_deg(yaw);
                let person = match self.shown.get_mut(&p.id) {
                    Some(s) if s.who == who => {
                        world.set_place(s.e, place)?;
                        s
                    }
                    _ => {
                        if let Some(old) = self.shown.remove(&p.id) {
                            world.despawn(old.e);
                        }
                        let e = world.spawn_animated_shared(model.clone(), place)?;
                        world.set_animation_blend(e, 0.3)?;
                        self.shown.entry(p.id).or_insert(Person {
                            e,
                            who,
                            clip: "",
                            rate: 0.0,
                        })
                    }
                };
                if person.clip != clip {
                    world.play_animation(person.e, clip)?;
                    person.clip = clip;
                }
                if (person.rate - rate).abs() > 0.02 {
                    world.set_animation_speed(person.e, rate)?;
                    person.rate = rate;
                }
                seen.insert(p.id);
                held_gear(world, person.e, show.act, &p, time, &mut places);
            }
            // Music, and words.
            music_near |= self.play_music(world, &show, eye, time);
            self.speak(&show, citizens, hero2, time);
        }
        // Those no longer about.
        let gone: Vec<u64> = self
            .shown
            .keys()
            .filter(|id| !seen.contains(id))
            .copied()
            .collect();
        for id in gone {
            if let Some(p) = self.shown.remove(&id) {
                world.despawn(p.e);
            }
        }
        for (g, lod) in &self.gear {
            world.set_lod_places(*lod, places.get(g).map_or(&[][..], |v| &v[..]))?;
        }
        // Music of acts that have gone.
        let keys: BTreeSet<(u64, i64)> = self.shows.iter().map(|s| s.key()).collect();
        let stale: Vec<(u64, i64)> = self
            .music
            .keys()
            .filter(|k| !keys.contains(k))
            .copied()
            .collect();
        for k in stale {
            if let Some(mut slot) = self.music.remove(&k) {
                keep_loop(
                    world,
                    &mut self.bank,
                    &mut slot,
                    "street_theremin",
                    None,
                    (1.0, 2.0),
                    time,
                );
            }
        }
        self.broken.retain(|k, _| keys.contains(k));
        self.talk.retain(|k, _| keys.contains(k));
        self.bank.sweep(time);
        self.music_near = music_near;
        Ok(())
    }

    /// Takes everything off screen (a new town, a new square).
    pub fn clear(&mut self, world: &mut World) {
        for (_, p) in std::mem::take(&mut self.shown) {
            world.despawn(p.e);
        }
        for lod in self.gear.values() {
            let _ = world.set_lod_places(*lod, &[]);
        }
        for (_, slot) in std::mem::take(&mut self.music) {
            if let Some(s) = slot {
                world.stop_sound(s, 0.5);
            }
        }
        self.shows.clear();
        self.broken.clear();
        self.talk.clear();
        self.music_near = false;
        self.refresh = 0.0;
    }

    /// The things' models, built on a worker thread the first time.
    fn load_gear(&mut self, world: &mut World) -> EngineResult<()> {
        if !self.gear.is_empty() {
            return Ok(());
        }
        if self.gear_job.is_none() {
            let (tx, rx) = channel();
            std::thread::Builder::new()
                .name("hw-street-gear".into())
                .spawn(move || {
                    let _ = tx.send(Gear::ALL.iter().map(|g| (*g, g.levels())).collect());
                })
                .map_err(|e| EngineError::InvalidValue(e.to_string()))?;
            self.gear_job = Some(rx);
        }
        let Some(Ok(levels)) = self.gear_job.as_ref().map(|r| r.try_recv()) else {
            return Ok(());
        };
        self.gear_job = None;
        for (g, l) in levels {
            self.gear.insert(g, world.spawn_lod_at(l, &[])?);
        }
        Ok(())
    }

    /// Asks the worker for the acts near the camera twice a second, and
    /// takes its answer in.
    fn ask_worker(
        &mut self,
        citizens: &Arc<Citizens>,
        camera: DVec2,
        time: f64,
        dt: f64,
    ) -> EngineResult<()> {
        if self.ask.is_none() {
            let (ask, asks) = channel::<Ask>();
            let (tell, answers) = channel();
            let life = self.life.clone();
            std::thread::Builder::new()
                .name("hw-street".into())
                .spawn(move || {
                    while let Ok(mut a) = asks.recv() {
                        // Only the newest question matters.
                        while let Ok(newer) = asks.try_recv() {
                            a = newer;
                        }
                        let mut shows: Vec<Arc<Show>> = life
                            .shows_near(&a.citizens, a.at, REACH, a.time, a.hour)
                            .into_iter()
                            .filter(|s| s.active(a.time))
                            .collect();
                        shows.sort_by(|x, y| {
                            x.site
                                .at
                                .distance(a.at)
                                .total_cmp(&y.site.at.distance(a.at))
                                .then(x.key().cmp(&y.key()))
                        });
                        shows.truncate(MOST);
                        if tell.send(shows).is_err() {
                            return;
                        }
                    }
                })
                .map_err(|e| EngineError::InvalidValue(e.to_string()))?;
            self.ask = Some(ask);
            self.answers = Some(answers);
        }
        if let Some(r) = &self.answers {
            while let Ok(shows) = r.try_recv() {
                self.shows = shows;
                self.waiting = false;
            }
        }
        self.refresh -= dt;
        if self.refresh <= 0.0 && !self.waiting {
            self.refresh = 0.5;
            if let Some(ask) = &self.ask {
                let sent = ask.send(Ask {
                    citizens: citizens.clone(),
                    at: camera,
                    time,
                    hour: citizens.hour(),
                });
                self.waiting = sent.is_ok();
            }
        }
        // Acts that have ended since the last answer.
        self.shows.retain(|s| s.active(time));
        Ok(())
    }

    /// Trouble near an act breaks it up: everyone runs.
    fn break_up(&mut self, citizens: &Citizens, alarms: &Alarms, time: f64) {
        for show in &self.shows {
            let key = show.key();
            if self.broken.contains_key(&key) {
                continue;
            }
            let at = show.site.at;
            let danger = alarms
                .active()
                .filter(|a| a.kind.frightens() && (a.at.y - show.site.y).abs() < 30.0)
                .find(|a| a.flat().distance(at) < a.radius + NERVES)
                .map(|a| a.flat());
            let Some(danger) = danger else {
                continue;
            };
            let folk = show
                .people(&self.life, citizens, time)
                .into_iter()
                .map(|p| {
                    let run = citizens.escape(p.pos.xz(), danger, FLEE_MAX);
                    (p, run)
                })
                .collect();
            self.broken.insert(
                key,
                Broken {
                    at: time,
                    danger,
                    folk,
                    gear: show.gear(time),
                },
            );
            let words = lines(show.act, Moment::Panic);
            if !words.is_empty() {
                let p = show.pitch(0);
                self.said.push(Said {
                    at: Vec3::new(p.x as f32, show.site.y as f32, p.y as f32),
                    range: SHOUT_RANGE,
                    who: show.act.title().to_string(),
                    text: words[(show.seed() % words.len() as u64) as usize].to_string(),
                    voice: Voice::Folk,
                });
            }
        }
    }

    /// The act's music, while it plays; true if it is close to `eye`.
    fn play_music(&mut self, world: &mut World, show: &Show, eye: Vec3, time: f64) -> bool {
        let (clip, at, vol, range) = match show.act {
            Act::Theremin => ("street_theremin", show.pitch(0), 0.8, (4.0, 45.0)),
            Act::Dancers => (
                "street_radio_swing",
                show.site.at - show.site.face * 0.5,
                0.7,
                (3.0, 40.0),
            ),
            Act::Club => {
                let door = show.site.door.unwrap_or(show.site.at);
                let open = show.door_open(time);
                (
                    "street_club_muffled",
                    door,
                    if open { 1.0 } else { 0.55 },
                    (4.0, 50.0),
                )
            }
            _ => return false,
        };
        let on = !self.broken.contains_key(&show.key())
            && match show.act {
                Act::Club => true,
                _ => show.gear(time) > 0.95,
            };
        let p = Vec3::new(at.x as f32, show.site.y as f32 + 1.0, at.y as f32);
        let want = (on && p.distance(eye) < range.1 + 10.0).then_some((p, vol, 1.0));
        let slot = self.music.entry(show.key()).or_default();
        keep_loop(world, &mut self.bank, slot, clip, want, range, time);
        on && p.distance(eye) < 25.0
    }

    /// Patter for a hero close by, a greeting, the statue's start, the
    /// prophet's goodbye, a watcher's cheer.
    fn speak(&mut self, show: &Show, citizens: &Citizens, hero: Option<DVec2>, time: f64) {
        let key = show.key();
        if self.broken.contains_key(&key) {
            return;
        }
        let Some(hero) = hero else {
            return;
        };
        let lead = show.pitch(0);
        let d = hero.distance(lead);
        let talk = self.talk.entry(key).or_default();
        let at = Vec3::new(
            lead.x as f32,
            (show.site.y + show.act.lift()) as f32,
            lead.y as f32,
        );
        let pick = |words: &[&'static str], n: u64| -> Option<&'static str> {
            (!words.is_empty())
                .then(|| words[(Seed(show.seed()).child(n).0 % words.len() as u64) as usize])
        };
        let say = |said: &mut Vec<Said>, text: &str, range: f32| {
            said.push(Said {
                at,
                range,
                who: show.act.title().to_string(),
                text: text.to_string(),
                voice: Voice::Folk,
            });
        };
        let on = show.gear(time) > 0.95;
        let packing = time >= show.off - hw_sim::streetlife::PACK && time < show.off;
        if packing && !talk.packed && d < HEAR * 2.0 {
            talk.packed = true;
            if let Some(t) = pick(lines(show.act, Moment::Leave), 7) {
                say(&mut self.said, t, SAY_RANGE);
            }
            return;
        }
        if !on {
            return;
        }
        // The living statue: come too close and it jumps.
        if show.act == Act::Statue {
            let near = d < BOO;
            if near && time >= talk.boo_next {
                talk.boo_until = time + STARTLE;
                talk.boo_next = time + 20.0;
                talk.count += 1;
                if let Some(t) = pick(lines(Act::Statue, Moment::Boo), talk.count) {
                    say(&mut self.said, t, SAY_RANGE);
                }
            }
            return;
        }
        if d < GREET && !talk.greeted {
            talk.greeted = true;
            talk.next = time + 8.0;
            if let Some(t) = pick(lines(show.act, Moment::Hero), 3) {
                say(&mut self.said, t, SAY_RANGE);
                return;
            }
        }
        if d < HEAR && time >= talk.next {
            talk.count += 1;
            let wait = 9.0 + (Seed(show.seed()).child(talk.count).0 % 6) as f64;
            talk.next = time + wait;
            let shout = matches!(show.act, Act::Newsboy | Act::Prophet);
            if let Some(t) = pick(lines(show.act, Moment::Own), talk.count) {
                say(
                    &mut self.said,
                    t,
                    if shout { SHOUT_RANGE } else { SAY_RANGE },
                );
            }
        }
        // Now and then someone watching cheers.
        if d < HEAR && time >= talk.cheer_next {
            talk.cheer_next = time + 18.0 + (show.seed() % 9) as f64;
            let clapper = show
                .people(&self.life, citizens, time)
                .into_iter()
                .find(|p| p.doing == Doing::Clapping);
            if let Some(p) = clapper {
                let t = WATCHER[(p.id.wrapping_add(talk.count) % WATCHER.len() as u64) as usize];
                self.said.push(Said {
                    at: p.pos.as_vec3(),
                    range: SAY_RANGE,
                    who: "Passer-by".to_string(),
                    text: t.to_string(),
                    voice: Voice::Folk,
                });
            }
        }
    }
}

/// Everyone of a broken-up act at `time`: running along their way out,
/// or (no way out) standing back to stare. Those who are gone are left out.
fn fleeing(citizens: &Citizens, b: &Broken, time: f64) -> Vec<(StreetPerson, bool)> {
    let x = (time - b.at) * FLEE;
    b.folk
        .iter()
        .filter_map(|(p, run)| match run {
            Some(e) if x >= e.len() => None,
            Some(e) => {
                let (pos, dir) = citizens.escape_at(e, x);
                Some((
                    StreetPerson {
                        pos,
                        facing: dir,
                        speed: FLEE,
                        doing: Doing::Walking,
                        carrying: false,
                        ..*p
                    },
                    true,
                ))
            }
            None => Some((
                StreetPerson {
                    facing: (b.danger - p.pos.xz()).normalize_or(p.facing),
                    speed: 0.0,
                    doing: Doing::Watching,
                    carrying: false,
                    pos: DVec3::new(p.pos.x, p.pos.y - lift_of(p), p.pos.z),
                    ..*p
                },
                false,
            )),
        })
        .collect()
}

/// How high someone stood on their soapbox or crate (they step down).
fn lift_of(p: &StreetPerson) -> f64 {
    match (p.role, p.doing) {
        (Role::Performer(act, _), Doing::Performing | Doing::Talking) => act.lift(),
        _ => 0.0,
    }
}

/// Which built character someone is.
fn who_of(role: Role) -> Who {
    match role {
        Role::Performer(act, k) => Who::Street(act, k),
        Role::Townsfolk(look) => Who::Citizen(look),
        Role::Guest(k) => Who::Guest(k),
    }
}

/// The clip someone plays.
fn clip_of(act: Act, p: &StreetPerson, running: bool, startled: bool) -> &'static str {
    let performer = matches!(p.role, Role::Performer(..));
    let townsfolk = matches!(p.role, Role::Townsfolk(_));
    if running {
        return if townsfolk { "walk" } else { "jog" };
    }
    match p.doing {
        Doing::Walking if p.carrying && performer && act != Act::Club => "carry case",
        Doing::Walking => "walk",
        Doing::Setting | Doing::Packing => "interact",
        Doing::Performing => match act {
            Act::Prophet => "preach",
            Act::Theremin => "theremin",
            Act::Statue if startled => "startle",
            Act::Statue => "statue",
            Act::Juggler => "juggle",
            Act::Spotter => "binoculars",
            Act::Newsboy => "holler",
            Act::Stargazer => "peer",
            Act::Dancers => "dance",
            Act::Club => "arms folded",
        },
        Doing::Talking => match act {
            Act::Stargazer => "point up",
            Act::Club => "nod",
            _ => "talk",
        },
        Doing::Watching => match act {
            Act::Spotter | Act::Stargazer => "look up",
            _ => "idle",
        },
        Doing::Clapping => "clap",
        Doing::Tipping | Doing::Buying => "interact",
        Doing::Queueing if !townsfolk && p.id.is_multiple_of(3) => "talk",
        Doing::Queueing => "idle",
    }
}

/// How fast a clip plays: walks match the walking speed.
fn rate_of(clip: &str, speed: f64, running: bool) -> f32 {
    let r = match clip {
        "walk" | "carry case" => speed as f32 / WALK_MPS,
        "jog" => speed as f32 / JOG_MPS,
        _ => 1.0,
    };
    if running {
        r.clamp(0.6, 2.0)
    } else {
        r.clamp(0.5, 1.6)
    }
}

fn place_at(p: DVec2, y: f64, face: DVec2) -> Place {
    Place::new(p.x as f32, y as f32, p.y as f32)
        .with_yaw_deg(face.x.atan2(face.y).to_degrees() as f32)
}

/// The things standing at an act's pitch, `g` of the way set up (they
/// appear once mostly unpacked, and go as packing starts).
fn set_gear(show: &Show, g: f64, time: f64, places: &mut BTreeMap<Gear, Vec<Place>>) {
    let s = &show.site;
    let face = s.face;
    let side = DVec2::new(face.y, -face.x);
    let p = show.pitch(0);
    let mut put = |gear: Gear, at: DVec2, y: f64, f: DVec2| {
        places.entry(gear).or_default().push(place_at(at, y, f));
    };
    if show.act == Act::Club {
        // The club's sign hangs over its door all night.
        if let Some(door) = s.door {
            let lit = (time * 0.7).sin() > -0.95;
            if lit {
                put(Gear::ClubSign, door + face * 0.05, s.y + 3.2, face);
            }
        }
        return;
    }
    if g < 0.5 {
        return;
    }
    let coins = |put: &mut dyn FnMut(Gear, DVec2, f64, DVec2)| {
        put(Gear::CaseOpen, p + face * 0.95 + side * 0.35, s.y, face);
    };
    match show.act {
        Act::Prophet => put(Gear::Soapbox, p, s.y, face),
        Act::Theremin => {
            put(Gear::Theremin, p + face * 0.42, s.y, face);
            coins(&mut put);
        }
        Act::Statue => {
            put(Gear::StatueCrate, p, s.y, face);
            coins(&mut put);
        }
        Act::Juggler | Act::Dancers => {
            coins(&mut put);
            if show.act == Act::Dancers {
                put(Gear::Radio, s.at - face * 0.6 + side * 0.9, s.y, face);
            }
        }
        Act::Newsboy => put(Gear::Bundle, p + side * 0.45 + face * 0.2, s.y, face),
        Act::Stargazer => put(Gear::Telescope, p + face * 0.7 + side * 0.5, s.y, face),
        Act::Spotter | Act::Club => {}
    }
}

/// Things someone holds or wears: the prophet's board, the spotter's foil
/// cap and binoculars, the newsboy's paper, the juggler's atoms, a
/// performer's case on the way in and out.
fn held_gear(
    world: &World,
    e: EntityId,
    act: Act,
    p: &StreetPerson,
    time: f64,
    places: &mut BTreeMap<Gear, Vec<Place>>,
) {
    let Role::Performer(_, k) = p.role else {
        return;
    };
    let feet = p.pos.as_vec3();
    let yaw = p.facing.x.atan2(p.facing.y).to_degrees() as f32;
    let fwd = Vec3::new(p.facing.x as f32, 0.0, p.facing.y as f32);
    // A bone where it is this frame (none until the model has posed).
    let bone = |name: &str, local: Vec3| -> Option<Vec3> {
        let m = world.bone_transform(e, name).ok()?;
        let at = m.transform_point3(local);
        (at.distance(feet) < 2.5).then_some(at)
    };
    let mut put = |g: Gear, at: Vec3, pitch: f32| {
        places.entry(g).or_default().push(
            Place::new(at.x, at.y, at.z)
                .with_yaw_deg(yaw)
                .with_pitch_deg(pitch),
        );
    };
    let doing = p.doing;
    if doing == Doing::Walking && p.carrying && act != Act::Club && act != Act::Prophet {
        if let Some(h) = bone("hand_r", Vec3::ZERO) {
            put(Gear::CaseShut, h, 0.0);
        }
    }
    match act {
        Act::Prophet => {
            if let Some(c) = bone("spine_03", Vec3::ZERO) {
                put(Gear::Board, c, 0.0);
            }
        }
        Act::Spotter => {
            if let Some(h) = bone("head", Vec3::new(0.0, 0.19, 0.0)) {
                put(Gear::FoilCap, h, 0.0);
            }
            if doing == Doing::Performing {
                if let Some(h) = bone("head", Vec3::new(0.0, 0.08, 0.0)) {
                    put(Gear::Binoculars, h + fwd * 0.13 + Vec3::Y * 0.03, -35.0);
                }
            }
        }
        Act::Newsboy if matches!(doing, Doing::Performing | Doing::Talking) => {
            if let Some(h) = bone("hand_r", Vec3::ZERO) {
                put(Gear::Paper, h - Vec3::Y * 0.04, 0.0);
            }
        }
        Act::Juggler if doing == Doing::Performing && k == 0 => {
            // A cascade: three atoms passing hand to hand over the head.
            let side = Vec3::new(fwd.z, 0.0, -fwd.x);
            let chest = feet + Vec3::Y * 1.12 + fwd * 0.38;
            for n in 0..3 {
                let a = (TAU * (time * 0.9 + n as f64 / 3.0)) as f32;
                let x = 0.24 * a.cos();
                let y = 0.1 * a.sin() + 0.42 * a.sin().max(0.0);
                put(Gear::Atom, chest + side * x + Vec3::Y * y, 0.0);
            }
        }
        _ => {}
    }
}
