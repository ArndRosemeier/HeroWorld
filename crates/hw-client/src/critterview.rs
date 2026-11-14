//! The town's animals on screen: wibbets on the pavements and up the lamp
//! posts, drakelets flying on their leads, driftbells floating over the
//! streets. Which animals there are and where is `hw_sim::citizens::critters`
//! (worked out from the town seed and the time, like the citizens); this
//! draws the ones near the camera.
//!
//! The models are AssetGenerator sculpts with their own skeletons and clips
//! (`assets/creatures/*.glb`, see `tools/creatures`), loaded on a worker
//! thread. Each colouring is drawn at three levels: the nearest few as full
//! characters, the rest as an Engine crowd of the middle model, and beyond
//! [`MID`] a crowd of the light one.
//!
//! Two things happen only here, to the wibbets the player meets: one that
//! sits on the pavement near a hero who stands about comes over to look at
//! them, and one a hero walks into hops out of the way. Both go straight
//! there and back over clear pavement, and their own round waits meanwhile.

use engine::prelude::*;
use glam::{DVec2, DVec3};
use hw_sim::alarms::Alarms;
use hw_sim::citizens::critters::{Pose, Species};
use hw_sim::citizens::Citizens;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

/// Wibbets and drakelets are drawn within this distance of the camera...
const REACH: f64 = 150.0;
/// ...and driftbells, big and bright in the sky, this far.
const SKY_REACH: f64 = 420.0;
/// At most this many animals at once (the nearest).
const MOST: usize = 90;
/// Full characters within this distance (and how many per colouring).
const NEAR_M: f32 = 26.0;
const NEAR_MAX: usize = 6;
/// Beyond this an animal is drawn with its lightest model...
const MID: f64 = 70.0;
/// ...and comes back to the middle one this much nearer.
const MID_BACK: f64 = 8.0;

/// A wibbet comes to look at a hero who stands about within this distance...
const CURIOUS: f64 = 7.5;
/// ...up to this near...
const VISIT_GAP: f64 = 1.25;
/// ...and stays this long at most (s), unless the hero walks off.
const VISIT_STAY: f64 = 9.0;
/// A hero slower than this is standing about (m/s).
const STROLL: f64 = 1.6;
/// A wibbet a hero comes this near to hops out of the way.
const DODGE: f64 = 0.55;
/// How fast a wibbet goes over and comes back (m/s).
const TROT: f64 = 2.5;

/// Where the lead is tied on a drakelet: its collar, from its feet (up,
/// forward), at size 1.
const COLLAR: (f32, f32) = (0.25, 0.16);
/// How thick a lead is (m), and how much it sags (of its length).
const LEAD_R: f64 = 0.007;
const LEAD_SAG: f64 = 0.1;

/// The model files of a species: `<name>_<coat>.glb` and its `_lod1`, `_lod2`.
fn files(s: Species) -> (&'static str, [&'static str; 3]) {
    match s {
        Species::Wibbet => ("wibbet", ["teal", "coral", "lilac"]),
        Species::Drakelet => ("drakelet", ["ember", "jade", "azure"]),
        Species::Driftbell => ("driftbell", ["rose", "aqua", "amber"]),
    }
}

/// A species' clips: standing, going, and the poses it can be held in.
fn clips(s: Species) -> (&'static str, &'static str, &'static [&'static str], f32) {
    match s {
        Species::Wibbet => ("Idle", "Walk", &["Sit", "Beg"], 1.8),
        Species::Drakelet => ("Hover", "Fly", &["Sit"], 1.4),
        Species::Driftbell => ("Drift", "Swim", &[], 0.55),
    }
}

/// Where the animals' models are: `assets/creatures` here, in the repo, or
/// next to the game.
pub fn creatures_dir() -> Option<PathBuf> {
    let has = |p: &Path| p.join("wibbet_teal.glb").is_file();
    let mut tries = vec![PathBuf::from("assets/creatures")];
    tries.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/creatures"));
    if let Ok(exe) = std::env::current_exe() {
        for up in exe.ancestors().skip(1).take(4) {
            tries.push(up.join("assets/creatures"));
        }
    }
    tries.into_iter().find(|p| has(p))
}

/// One colouring of one species: close, middle and far models.
pub type Loaded = (Species, u8, [Arc<AnimatedModel>; 3]);

/// Loads every model (on the worker thread that asks).
pub fn load_all(dir: &Path) -> EngineResult<Vec<Loaded>> {
    let dir = &dir.canonicalize()?;
    let limits = EngineLimits::default();
    let mut out = Vec::new();
    for s in Species::ALL {
        let (name, coats) = files(s);
        for (k, coat) in coats.iter().enumerate() {
            let load = |tail: &str| {
                let file = dir.join(format!("{name}_{coat}{tail}.glb"));
                AnimatedModel::load_with(file, dir, &limits).map(Arc::new)
            };
            out.push((s, k as u8, [load("")?, load("_lod1")?, load("_lod2")?]));
        }
    }
    Ok(out)
}

/// The crowds one colouring is drawn with.
#[derive(Clone, Copy)]
struct Crowds {
    /// Full characters near, the middle model further off.
    near: CrowdId,
    /// The lightest model, beyond [`MID`].
    far: CrowdId,
}

/// A wibbet off its round: over to a hero to look, or out of one's way.
struct Visit {
    /// Where it left its round, to come back to.
    home: DVec2,
    to: DVec2,
    /// Where it is now.
    at: DVec2,
    /// Seconds it has been where it was going.
    stayed: f64,
    /// How long it stays.
    stay: f64,
    back: bool,
    /// Come to look (it sits up and begs), or only out of the way.
    curious: bool,
}

/// A wibbet dropping from a post or a bench that has been knocked down.
#[derive(Clone, Copy)]
struct Fall {
    y: f64,
    v: f64,
    y0: f64,
    from: DVec2,
}

/// How hard gravity pulls a falling wibbet (m/s²).
const GRAVITY: f64 = 9.8;

/// An animal on screen.
struct Shown {
    kind: (Species, u8),
    crowd: CrowdId,
    agent: CrowdAgentId,
    far: bool,
    /// Its heading as drawn (degrees), turned smoothly.
    yaw: f32,
    pos: DVec3,
    /// Nose up or down a post (degrees).
    pitch: f64,
    /// Falling off a perch that broke: how high it is, how fast it falls
    /// (m/s), and where it came off (height, plan).
    fall: Option<Fall>,
    /// How high (absolute) it was drawn last frame.
    y: f64,
    visit: Option<Visit>,
    /// Seconds its round is behind the town's clock (spent on visits).
    delay: f64,
    /// No new visit before this much more time has passed (s).
    rest: f64,
}

struct Ask {
    citizens: Arc<Citizens>,
    at: DVec2,
    time: f64,
}

#[derive(Default)]
pub struct CritterView {
    loading: Option<Receiver<EngineResult<Vec<Loaded>>>>,
    failed: bool,
    crowds: BTreeMap<(Species, u8), Crowds>,
    ask: Option<Sender<Ask>>,
    answers: Option<Receiver<Vec<u64>>>,
    waiting: bool,
    refresh: f64,
    near: Vec<u64>,
    shown: BTreeMap<u64, Shown>,
    /// The leads, one stretched copy each.
    leads: Option<EntityId>,
    hero_was: Option<DVec3>,
}

/// A lead: a thin strap from (0, 0, 0) to (0, 0, 1), sagging a little,
/// stretched to its length where it is placed.
fn lead_mesh() -> Mesh {
    let mut b = crate::citymesh::MeshBuilder::new();
    let path: Vec<DVec3> = (0..=12)
        .map(|k| {
            let t = k as f64 / 12.0;
            DVec3::new(0.0, -LEAD_SAG * 4.0 * t * (1.0 - t), t)
        })
        .collect();
    let leather = Some((crate::citymesh::Surf::Plain, rgb(150, 40, 36)));
    crate::carmesh::tube(&mut b, &path, (LEAD_R, LEAD_R), 6, leather);
    b.finish()
}

fn yaw_of(facing: DVec2) -> f32 {
    facing.x.atan2(facing.y).to_degrees() as f32
}

/// `from` turned towards `to` (degrees) by at most `step`.
fn turned(from: f32, to: f32, step: f32) -> f32 {
    let d = (to - from + 540.0).rem_euclid(360.0) - 180.0;
    from + d.clamp(-step, step)
}

impl CritterView {
    pub fn new() -> Self {
        Self::default()
    }

    /// How many animals are on screen.
    pub fn shown(&self) -> usize {
        self.shown.len()
    }

    /// The animal of `species` shown nearest `eye`: where it is, the way it
    /// faces (degrees) and its size (for the `HW_VIEW=critter` close-up).
    /// With `up_a_post`, only those on a post count.
    pub fn nearest(
        &self,
        species: Species,
        eye: Vec3,
        up_a_post: bool,
    ) -> Option<(Vec3, f32, f32)> {
        self.shown
            .values()
            .filter(|s| s.kind.0 == species && (!up_a_post || s.pitch != 0.0))
            .map(|s| (s.pos.as_vec3(), s.yaw))
            .min_by(|a, b| a.0.distance(eye).total_cmp(&b.0.distance(eye)))
            .map(|(p, yaw)| (p, yaw, 1.0))
    }

    /// Takes every animal off the streets (another town comes).
    pub fn clear(&mut self, world: &mut World) {
        for (_, s) in std::mem::take(&mut self.shown) {
            world.remove_crowd_agent(s.crowd, s.agent);
        }
        if let Some(l) = self.leads {
            let _ = world.set_instances(l, &[]);
        }
        self.near.clear();
        self.refresh = 0.0;
        self.waiting = false;
    }

    /// The models, loaded on a worker thread the first time; then a crowd
    /// pair per colouring. False until they are there.
    fn ready(&mut self, world: &mut World) -> EngineResult<bool> {
        if !self.crowds.is_empty() {
            return Ok(true);
        }
        if self.failed {
            return Ok(false);
        }
        if self.loading.is_none() {
            let Some(dir) = creatures_dir() else {
                self.failed = true;
                eprintln!("no animals: assets/creatures not found");
                return Ok(false);
            };
            let (tx, rx) = channel();
            std::thread::Builder::new()
                .name("hw-critters".into())
                .spawn(move || {
                    engine::parallel::background_thread();
                    let _ = tx.send(load_all(&dir));
                })
                .map_err(|e| EngineError::InvalidValue(e.to_string()))?;
            self.loading = Some(rx);
        }
        let Some(Ok(loaded)) = self.loading.as_ref().map(|r| r.try_recv()) else {
            return Ok(false);
        };
        self.loading = None;
        let loaded = match loaded {
            Ok(l) => l,
            Err(e) => {
                self.failed = true;
                eprintln!("no animals: {e}");
                return Ok(false);
            }
        };
        for (species, coat, [close, mid, far]) in loaded {
            let (idle, walk, poses, mps) = clips(species);
            let reach = if species == Species::Driftbell {
                SKY_REACH
            } else {
                REACH
            } as f32;
            let near = CrowdProfile::new(idle, walk)
                .walk_speed(mps)
                .near(NEAR_M, NEAR_MAX)
                .far(reach + 20.0)
                .shadows_within(45.0)
                .far_model(mid)
                .poses(poses.iter().copied());
            let light = CrowdProfile::new(idle, walk)
                .walk_speed(mps)
                .near(0.0, 0)
                .far(reach + 20.0)
                .shadows_within(0.0)
                .far_model(far.clone())
                .poses(poses.iter().copied());
            let crowds = Crowds {
                near: world.add_crowd(close, near)?,
                far: world.add_crowd(far, light)?,
            };
            self.crowds.insert((species, coat), crowds);
        }
        self.leads = Some(world.spawn_instanced(lead_mesh()));
        Ok(true)
    }

    /// Asks the worker which animals are near, twice a second.
    fn ask_worker(&mut self, citizens: &Arc<Citizens>, camera: DVec2, time: f64, dt: f64) {
        if self.ask.is_none() {
            let (ask, asks) = channel::<Ask>();
            let (tell, answers) = channel();
            let spawned = std::thread::Builder::new()
                .name("hw-critters-near".into())
                .spawn(move || {
                    engine::parallel::background_thread();
                    while let Ok(mut a) = asks.recv() {
                        // Only the newest question matters.
                        while let Ok(newer) = asks.try_recv() {
                            a = newer;
                        }
                        let ids: Vec<u64> = a
                            .citizens
                            .critters_near(a.at, SKY_REACH, a.time, 4000)
                            .into_iter()
                            .filter(|c| {
                                c.species == Species::Driftbell
                                    || flat(c.pos).distance(a.at) <= REACH
                            })
                            .take(MOST)
                            .map(|c| c.id)
                            .collect();
                        if tell.send(ids).is_err() {
                            return;
                        }
                    }
                });
            if spawned.is_ok() {
                self.ask = Some(ask);
                self.answers = Some(answers);
            }
        }
        if let Some(r) = &self.answers {
            while let Ok(ids) = r.try_recv() {
                self.near = ids;
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
                });
                self.waiting = sent.is_ok();
            }
        }
    }

    /// Moves the animals near `camera` to `time`. `hero` is the player's
    /// hero (wibbets come to look at one who stands about); `owner_delay`
    /// tells how far behind the clock a citizen is drawn (one who stopped
    /// to stare at a fight, see `CrowdView`), `owner_hand` where their
    /// right hand is when they are drawn near enough to tell.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        world: &mut World,
        citizens: &Arc<Citizens>,
        alarms: &Alarms,
        owner_delay: &dyn Fn(u64) -> f64,
        owner_hand: &dyn Fn(&World, u64) -> Option<Vec3>,
        broken: &BTreeMap<u64, u8>,
        camera: DVec2,
        hero: Option<DVec3>,
        time: f64,
        dt: f64,
    ) -> EngineResult<()> {
        if !self.ready(world)? {
            return Ok(());
        }
        self.ask_worker(citizens, camera, time, dt);
        let hero_speed = match (hero, self.hero_was) {
            (Some(a), Some(b)) if dt > 0.0 => a.distance(b) / dt,
            _ => 0.0,
        };
        self.hero_was = hero;
        let trouble = |p: DVec2| {
            alarms
                .active()
                .any(|a| a.flat().distance(p) < a.radius.max(1.0) * 1.5)
        };
        let mut seen = BTreeSet::new();
        let mut leads = Vec::new();
        for id in self.near.clone() {
            let delay = match Species::of(id) {
                Some(Species::Drakelet) => owner_delay(id & ((1 << 56) - 1)),
                _ => self.shown.get(&id).map_or(0.0, |s| s.delay),
            };
            let Some(mut c) = citizens.critter_with(id, time - delay, &|p| broken.contains_key(&p))
            else {
                continue;
            };
            let Some(&crowds) = self.crowds.get(&(c.species, c.coat)) else {
                continue;
            };
            seen.insert(id);
            let far_off = flat(c.pos).distance(camera);
            let state = self.shown.get_mut(&id);
            // A wibbet's visit to the hero, or its hop out of the way.
            let mut pose: Option<&str> = match c.pose {
                Pose::Sit => Some("Sit"),
                _ => None,
            };
            if c.species == Species::Wibbet {
                if let Some(s) = state {
                    s.rest = (s.rest - dt).max(0.0);
                    let at = flat(c.pos);
                    let h = hero.map(flat);
                    let level = hero.is_some_and(|h| (h.y - c.pos.y).abs() < 1.5);
                    if s.visit.is_none() && c.on_ground && level && !trouble(at) {
                        if let Some(h) = h {
                            let d = h.distance(at);
                            let away = (at - h).normalize_or(c.facing);
                            let plan = if d < DODGE {
                                Some((at + away * 0.9, 0.8, false))
                            } else if d < CURIOUS
                                && d > VISIT_GAP + 0.5
                                && hero_speed < STROLL
                                && s.rest <= 0.0
                                && curious(id, time)
                            {
                                Some((h + away * VISIT_GAP, VISIT_STAY, true))
                            } else {
                                None
                            };
                            if let Some((to, stay, look)) = plan {
                                if citizens.critter_way_clear(at, to) {
                                    s.visit = Some(Visit {
                                        home: at,
                                        to,
                                        at,
                                        stayed: 0.0,
                                        stay,
                                        back: false,
                                        curious: look,
                                    });
                                } else {
                                    // Not this way: think again in a while.
                                    s.rest = 4.0;
                                }
                            }
                        }
                    }
                    let mut done = false;
                    if let Some(v) = s.visit.as_mut() {
                        s.delay += dt;
                        let goal = if v.back { v.home } else { v.to };
                        let left = goal.distance(v.at);
                        let step = TROT * dt;
                        if left > step {
                            let dir = (goal - v.at) / left;
                            v.at += dir * step;
                            c.facing = dir;
                            c.speed = TROT;
                            c.pose = Pose::Go;
                            pose = None;
                        } else {
                            v.at = goal;
                            c.speed = 0.0;
                            if v.back {
                                done = true;
                            } else {
                                v.stayed += dt;
                                if let Some(h) = h {
                                    c.facing = (h - v.at).normalize_or(c.facing);
                                }
                                pose = v.curious.then_some("Beg");
                                // Off again: time is up, the hero left or
                                // strode at it, or there is trouble.
                                let gone = h.is_none_or(|h| h.distance(v.at) > CURIOUS);
                                let rushed = hero_speed > 3.5
                                    && h.is_some_and(|h| h.distance(v.at) < VISIT_GAP);
                                if v.stayed > v.stay || gone || rushed || trouble(v.at) {
                                    v.back = true;
                                }
                            }
                        }
                        c.pos.x = v.at.x;
                        c.pos.z = v.at.y;
                        c.pitch = 0.0;
                    }
                    if done {
                        s.visit = None;
                        s.rest = 25.0;
                    }
                }
            }
            // A perch that broke under it: it falls to the pavement where the
            // town now puts it.
            if c.species == Species::Wibbet {
                if let Some(s) = self.shown.get_mut(&id) {
                    if s.fall.is_none() && s.y - c.pos.y > 1.0 + 8.0 * dt && s.visit.is_none() {
                        s.fall = Some(Fall {
                            y: s.y,
                            v: 0.0,
                            y0: s.y,
                            from: flat(s.pos),
                        });
                    }
                    if let Some(f) = s.fall.as_mut() {
                        f.v += GRAVITY * dt;
                        f.y -= f.v * dt;
                        if f.y <= c.pos.y {
                            s.fall = None;
                        } else {
                            let done = 1.0 - (f.y - c.pos.y) / (f.y0 - c.pos.y).max(0.01);
                            let xz = f.from.lerp(flat(c.pos), done.clamp(0.0, 1.0));
                            c.pos = DVec3::new(xz.x, f.y, xz.y);
                            c.pitch = 0.0;
                            c.speed = 0.0;
                            pose = None;
                        }
                    }
                }
            }
            // Its place: nose up or down a post, turned smoothly.
            let want = yaw_of(c.facing);
            let (yaw, was_far) = match self.shown.get(&id) {
                Some(s) if s.kind == (c.species, c.coat) => {
                    let quick = if c.pitch != 0.0 { 2000.0 } else { 540.0 };
                    (turned(s.yaw, want, quick * dt as f32), s.far)
                }
                _ => (want, far_off > MID),
            };
            let far = if was_far {
                far_off > MID - MID_BACK
            } else {
                far_off > MID
            };
            let place = Place::new(c.pos.x as f32, c.pos.y as f32, c.pos.z as f32)
                .with_yaw_deg(yaw)
                .with_pitch_deg(-c.pitch as f32)
                .with_scale(c.size as f32);
            let crowd = if far { crowds.far } else { crowds.near };
            // Into the crowd it belongs in (another colouring never
            // happens to an id; nearer or further does).
            let moved = self
                .shown
                .get(&id)
                .is_none_or(|s| s.crowd != crowd || s.kind != (c.species, c.coat));
            if moved {
                let old = self.shown.remove(&id);
                if let Some(o) = &old {
                    world.remove_crowd_agent(o.crowd, o.agent);
                }
                let agent = world.add_crowd_agent(crowd, place)?;
                self.shown.insert(
                    id,
                    Shown {
                        kind: (c.species, c.coat),
                        crowd,
                        agent,
                        far,
                        yaw,
                        pos: c.pos,
                        pitch: c.pitch,
                        fall: None,
                        y: c.pos.y,
                        delay: old.as_ref().map_or(0.0, |o| o.delay),
                        rest: old.as_ref().map_or(0.0, |o| o.rest),
                        visit: old.and_then(|o| o.visit),
                    },
                );
            }
            let s = self.shown.get_mut(&id).expect("just put");
            s.yaw = yaw;
            s.far = far;
            s.pos = c.pos;
            s.pitch = c.pitch;
            s.y = c.pos.y;
            world.set_crowd_agent(s.crowd, s.agent, place, c.speed as f32)?;
            world.set_crowd_agent_pose(s.crowd, s.agent, pose)?;
            // A drakelet's lead, from its owner's hand to its collar.
            if let (Some(owner), false) = (c.owner, far) {
                if let Some(o) = citizens.citizen(owner, time - delay) {
                    let right = DVec2::new(o.facing.y, -o.facing.x);
                    let rough = o.pos + DVec3::new(right.x * 0.3, 0.88, right.y * 0.3);
                    let hand = owner_hand(world, owner).unwrap_or(rough.as_vec3());
                    let f = Vec3::new(c.facing.x as f32, 0.0, c.facing.y as f32);
                    let size = c.size as f32;
                    let collar = c.pos.as_vec3() + (Vec3::Y * COLLAR.0 + f * COLLAR.1) * size;
                    let d = collar - hand;
                    let len = d.length();
                    if len > 0.05 {
                        let flat_len = glam::Vec2::new(d.x, d.z).length();
                        leads.push(
                            Place::new(hand.x, hand.y, hand.z)
                                .with_yaw_deg(d.x.atan2(d.z).to_degrees())
                                .with_pitch_deg(-d.y.atan2(flat_len).to_degrees())
                                .with_stretch(Vec3::new(1.0, len, len)),
                        );
                    }
                }
            }
        }
        let gone: Vec<u64> = (self.shown.keys())
            .filter(|id| !seen.contains(id))
            .copied()
            .collect();
        for id in gone {
            if let Some(s) = self.shown.remove(&id) {
                world.remove_crowd_agent(s.crowd, s.agent);
            }
        }
        if let Some(l) = self.leads {
            world.set_instances(l, &leads)?;
        }
        Ok(())
    }
}

fn flat(p: DVec3) -> DVec2 {
    DVec2::new(p.x, p.z)
}

/// Is wibbet `id` in the mood to come and look, this half minute?
fn curious(id: u64, time: f64) -> bool {
    let slot = (time / 30.0).floor() as u64;
    let h = (id ^ slot.wrapping_mul(0x9E37_79B9_7F4A_7C15)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    (h >> 40) % 100 < 60
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turning_takes_the_short_way_round() {
        assert_eq!(turned(170.0, -170.0, 30.0), 190.0);
        assert_eq!(turned(-170.0, 170.0, 5.0), -175.0);
        assert_eq!(turned(10.0, 20.0, 30.0), 20.0);
    }

    #[test]
    fn the_animals_models_are_complete() {
        // Every colouring at three levels, each with the clips it is played
        // with and one skeleton, so the levels swap in place.
        let Some(dir) = creatures_dir() else {
            panic!("assets/creatures is missing");
        };
        let all = load_all(&dir).expect("the models load");
        assert_eq!(all.len(), 9);
        for (species, coat, levels) in &all {
            let (idle, walk, poses, _) = clips(*species);
            let tris =
                |m: &AnimatedModel| -> usize { m.meshes.iter().map(|x| x.indices.len() / 3).sum() };
            for m in levels {
                for clip in [idle, walk].iter().chain(poses) {
                    assert!(
                        m.find_clip(clip).is_some(),
                        "{species:?} {coat} lacks {clip}"
                    );
                }
                assert_eq!(
                    m.skeleton.joint_names, levels[0].skeleton.joint_names,
                    "{species:?} {coat}: the levels' skeletons differ"
                );
            }
            // A close-up's worth near, a few hundred triangles far away.
            let (a, b, c) = (tris(&levels[0]), tris(&levels[1]), tris(&levels[2]));
            assert!(
                a > 9_000 && a > b * 3 && b > c * 3 && c < 1_000,
                "{a} {b} {c}"
            );
        }
    }
}
