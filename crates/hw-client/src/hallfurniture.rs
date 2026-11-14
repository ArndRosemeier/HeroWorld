//! The furniture of the town's buildings of halls
//! (`BuildingPlan::furniture`), drawn round the eye.
//!
//! One LOD set per look (a kind of piece, its width, its colours), made
//! the first time a building near the eye has one; as the eye moves, a
//! worker lists where every piece near it stands and the sets take those
//! places. Nothing is kept per building: what is out of reach is simply
//! not in the next list.
//!
//! When a building breaks, the furniture that lost its floor
//! ([`hw_sim::destruction::furniture_lost`]) leaves the lists and, if the
//! blow was seen live, falls: the nearest few dozen pieces become physics
//! bodies for a while, the rest are buried in the dust. All of it is
//! bounded ([`MAX_FALLING`], [`PER_BLOW`], [`PER_FRAME`], [`LIVE_SECONDS`])
//! and planned on worker threads. A repaired building takes its furniture
//! back: nothing is remembered but the lost flags.

use crate::cometmesh::{look, piece_mesh, Look, FURNITURE_REACH, FURNITURE_SEGS};
use engine::building::{BuildingPlan as Breakable, Collapse, ElementBits};
use engine::prelude::*;
use glam::{DVec2, DVec3};
use hw_gen::layout::furnish::ItemKind;
use hw_gen::layout::Scheme;
use hw_gen::urban::quarter::DetailId;
use hw_gen::urban::world::TownWorld;
use hw_sim::destruction::{furniture_lost, Frame};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

/// Most pieces of furniture falling or lying in the rubble at once.
const MAX_FALLING: usize = 120;
/// Most pieces one blow turns into bodies (the nearest ones).
const PER_BLOW: usize = 48;
/// Bodies started per frame.
const PER_FRAME: usize = 8;
/// Pieces further than this from the eye are only taken out of the lists (m).
const FALL_REACH: f64 = 120.0;
/// Seconds a fallen piece lies about before it goes.
const LIVE_SECONDS: f32 = 45.0;

/// Furniture is listed within this of the eye (m): past the far level's
/// reach by more than the eye moves between lists.
const REACH: f64 = 130.0;
/// A new list is made when the eye has moved this far (m).
const STEP: f32 = 25.0;

/// A look in a house livery: (scheme, kind, width in cm, colour set).
type Key = (u8, u8, u32, u8);

/// What the worker sends back: per look where its pieces stand, and its
/// model if the view has none yet.
type Listed = Vec<(Key, Vec<Place>, Option<Vec<LodLevel>>)>;

#[derive(Default)]
pub struct HallFurniture {
    sets: BTreeMap<Key, LodId>,
    /// Where the eye was for the list shown or being made.
    at: Option<Vec3>,
    worker: Option<Receiver<Listed>>,
    /// Per damaged building: which of its furniture lost its floor.
    lost: BTreeMap<u64, Arc<Vec<bool>>>,
    /// Per building: the job working out what fell, and what came in since.
    jobs: BTreeMap<u64, Job>,
    /// Pieces made ready to fall, waiting for their turn.
    waiting: VecDeque<Fall>,
    /// Falling bodies and their age (s), oldest first.
    falling: VecDeque<(EntityId, f32, u64)>,
    /// Buildings repaired since the last frame.
    repaired: Vec<u64>,
    /// The town and the eye as of the last frame (jobs queued behind a job).
    town: Option<Arc<TownWorld>>,
    eye: Vec3,
}

/// A building's state after a change, as far as its furniture goes.
pub struct Change {
    pub building: u64,
    pub plan: Arc<Breakable>,
    pub gone: ElementBits,
    pub collapse: Option<Collapse>,
    pub frame: Frame,
    /// The blow was seen live: the furniture falls rather than just goes.
    pub animate: bool,
}

/// What the damage view tells the furniture.
pub enum News {
    Changed(Change),
    Repaired(u64),
}

struct Job {
    done: Receiver<Done>,
    /// A change that came while the job ran; made when it ends.
    next: Option<Change>,
}

/// What a job found: the lost flags and the pieces that fall.
struct Done {
    lost: Vec<bool>,
    falls: Vec<Fall>,
}

/// A piece about to fall: where it stood, and its look.
struct Fall {
    building: u64,
    at: Place,
    mesh: Mesh,
    push: Vec3,
}

/// Works out what a change loses, and the falls for what is new: the
/// town's plan for the building and the models take their time.
fn work(town: &TownWorld, ch: &Change, before: Option<&[bool]>, eye: DVec2) -> Option<Done> {
    let wb = town.building(DetailId(ch.building))?;
    let s = &wb.site;
    let plan = s.layout()?;
    let items = s.furniture_of(&plan);
    let lost = furniture_lost(&items, ch.plan.elements(), &ch.gone, &ch.frame, ch.collapse);
    let mut news: Vec<(f64, usize)> = Vec::new();
    if ch.animate {
        for (k, it) in items.iter().enumerate() {
            let was = before.and_then(|b| b.get(k)).copied().unwrap_or(false);
            if lost[k] && !was && it.kind != ItemKind::Rug {
                let p = s.to_city(DVec3::new(it.at.x, wb.base + it.y, it.at.y));
                let d = DVec2::new(p.x, p.z).distance(eye);
                if d < FALL_REACH {
                    news.push((d, k));
                }
            }
        }
        news.sort_by(|a, b| a.0.total_cmp(&b.0));
        news.truncate(PER_BLOW);
    }
    let mut models: BTreeMap<(u8, u32, u8), Mesh> = BTreeMap::new();
    let mut falls = Vec::new();
    for (_, k) in news {
        let it = &items[k];
        let lk = look(it);
        let mesh = models
            .entry((lk.0 as u8, lk.1, lk.2))
            .or_insert_with(|| piece_mesh(plan.scheme, lk, FURNITURE_SEGS[1]))
            .clone();
        let p = s.to_city(DVec3::new(it.at.x, wb.base + it.y, it.at.y));
        let f = s.along * it.face.x + s.inward * it.face.y;
        let yaw = f.x.atan2(f.y).to_degrees();
        // A little shove outwards from the middle, so pieces do not stack.
        let h = (it.seed % 360) as f32 * std::f32::consts::PI / 180.0;
        falls.push(Fall {
            building: ch.building,
            at: Place::new(p.x as f32, p.y as f32, p.z as f32).with_yaw_deg(yaw as f32),
            mesh,
            push: Vec3::new(h.cos(), 0.3, h.sin()) * 0.8,
        });
    }
    Some(Done { lost, falls })
}

fn scheme_key(s: Scheme) -> u8 {
    match s {
        Scheme::GrandHall => 0,
        Scheme::Atrium => 1,
        Scheme::SplitLevel => 2,
        Scheme::ShopHouse => 3,
        Scheme::Flats => 4,
        Scheme::Block => 5,
        Scheme::Terrace => 6,
        Scheme::Villa => 7,
        Scheme::Loft => 8,
        Scheme::Den => 9,
    }
}

/// Every piece of furniture within [`REACH`] of `c`, by look, with models
/// for the looks not in `known`.
fn list(
    town: &TownWorld,
    c: DVec2,
    known: &BTreeSet<Key>,
    lost: &BTreeMap<u64, Arc<Vec<bool>>>,
) -> Listed {
    let m = DVec2::splat(REACH);
    let mut places: BTreeMap<Key, (Scheme, Look, Vec<Place>)> = BTreeMap::new();
    for q in town.quarters_in(c - m, c + m) {
        for (n, b) in q.buildings.iter().enumerate() {
            let s = &b.site;
            let mid = s.origin + (s.along * s.size.x + s.inward * s.size.y) * 0.5;
            if s.halls.is_none() || mid.distance(c) > REACH {
                continue;
            }
            let Some(plan) = s.layout() else { continue };
            let gone = lost.get(&DetailId::new(q.unit, n).0);
            for (k, it) in s.furniture_of(&plan).into_iter().enumerate() {
                if gone.is_some_and(|g| g.get(k).copied().unwrap_or(false)) {
                    continue;
                }
                let lk = look(&it);
                let p = s.to_city(DVec3::new(it.at.x, b.base + it.y, it.at.y));
                let f = s.along * it.face.x + s.inward * it.face.y;
                // Chairs stand a little askew, as people leave them.
                let askew = match it.kind {
                    ItemKind::SwivelChair => (it.seed % 7) as f64 * 5.0 - 15.0,
                    ItemKind::Chair => (it.seed % 5) as f64 * 3.0 - 6.0,
                    _ => 0.0,
                };
                let yaw = f.x.atan2(f.y).to_degrees() + askew;
                places
                    .entry((scheme_key(plan.scheme), lk.0 as u8, lk.1, lk.2))
                    .or_insert_with(|| (plan.scheme, lk, Vec::new()))
                    .2
                    .push(Place::new(p.x as f32, p.y as f32, p.z as f32).with_yaw_deg(yaw as f32));
            }
        }
    }
    places
        .into_iter()
        .map(|(key, (scheme, lk, at))| {
            let model = (!known.contains(&key)).then(|| {
                FURNITURE_SEGS
                    .into_iter()
                    .zip(FURNITURE_REACH)
                    .map(|(seg, reach)| LodLevel::new(piece_mesh(scheme, lk, seg), reach))
                    .collect()
            });
            (key, at, model)
        })
        .collect()
}

impl HallFurniture {
    /// Once a frame: asks for a new list when the eye has moved, and puts
    /// a finished one into the world.
    pub fn update(
        &mut self,
        world: &mut World,
        town: &Arc<TownWorld>,
        eye: Vec3,
        dt: f32,
    ) -> EngineResult<()> {
        self.town = Some(town.clone());
        self.eye = eye;
        self.fall(world, dt)?;
        if let Some(rx) = &self.worker {
            match rx.try_recv() {
                Ok(listed) => {
                    self.worker = None;
                    let mut shown = BTreeSet::new();
                    for (key, at, model) in listed {
                        shown.insert(key);
                        match (self.sets.get(&key), model) {
                            (Some(&id), _) => world.set_lod_places(id, &at)?,
                            (None, Some(levels)) => {
                                self.sets.insert(key, world.spawn_lod_at(levels, &at)?);
                            }
                            // Asked for with the look known, and the town
                            // was cleared meanwhile: the next list brings it.
                            (None, None) => self.at = None,
                        }
                    }
                    for (key, &id) in &self.sets {
                        if !shown.contains(key) {
                            world.set_lod_places(id, &[])?;
                        }
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return Ok(()),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.worker = None;
                    self.at = None;
                }
            }
        }
        if self.at.is_some_and(|a| a.distance(eye) < STEP) {
            return Ok(());
        }
        self.at = Some(eye);
        let (tx, rx) = channel();
        let town = town.clone();
        let known: BTreeSet<Key> = self.sets.keys().copied().collect();
        let lost = self.lost.clone();
        let c = DVec2::new(eye.x as f64, eye.z as f64);
        // Planning the quarters and the floors, and building new models,
        // takes its time: never on the frame's thread.
        std::thread::Builder::new()
            .name("hall furniture".into())
            .spawn(move || {
                engine::parallel::background_thread();
                let _ = tx.send(list(&town, c, &known, &lost));
            })
            .map_err(EngineError::Io)?;
        self.worker = Some(rx);
        Ok(())
    }

    /// The damage view's news: a building changed or was repaired.
    pub fn news(&mut self, town: &Arc<TownWorld>, eye: Vec3, news: Vec<News>) {
        for n in news {
            match n {
                News::Repaired(i) => {
                    self.lost.remove(&i);
                    self.jobs.remove(&i);
                    self.waiting.retain(|f| f.building != i);
                    // Fallen pieces of it are cleared by the next `fall`.
                    self.repaired.push(i);
                    self.at = None;
                }
                News::Changed(ch) => match self.jobs.get_mut(&ch.building) {
                    Some(job) => match job.next.as_mut() {
                        Some(next) => {
                            let animate = next.animate | ch.animate;
                            *next = ch;
                            next.animate = animate;
                        }
                        None => job.next = Some(ch),
                    },
                    None => self.start(town, eye, ch),
                },
            }
        }
    }

    /// Starts the job for a change on a worker thread.
    fn start(&mut self, town: &Arc<TownWorld>, eye: Vec3, ch: Change) {
        let (tx, rx) = channel();
        let town = town.clone();
        let before = self.lost.get(&ch.building).cloned();
        let at = DVec2::new(eye.x as f64, eye.z as f64);
        let i = ch.building;
        let started = std::thread::Builder::new()
            .name("furniture falls".into())
            .spawn(move || {
                engine::parallel::background_thread();
                if let Some(done) = work(&town, &ch, before.as_deref().map(|v| &v[..]), at) {
                    let _ = tx.send(done);
                }
            });
        match started {
            Ok(_) => {
                self.jobs.insert(
                    i,
                    Job {
                        done: rx,
                        next: None,
                    },
                );
            }
            Err(e) => println!("Could not start the furniture of building {i}: {e}"),
        }
    }

    /// Takes finished jobs, starts the waiting falls, ages the fallen.
    fn fall(&mut self, world: &mut World, dt: f32) -> EngineResult<()> {
        let repaired = std::mem::take(&mut self.repaired);
        if !repaired.is_empty() {
            let (gone, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut self.falling)
                .into_iter()
                .partition(|f| repaired.contains(&f.2));
            self.falling = keep.into();
            for (id, _, _) in gone {
                world.despawn(id);
            }
        }
        let mut finished = Vec::new();
        for (&i, job) in &self.jobs {
            match job.done.try_recv() {
                Ok(done) => finished.push((i, Some(done))),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => finished.push((i, None)),
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        for (i, done) in finished {
            let job = self.jobs.remove(&i);
            if let Some(d) = done {
                self.lost.insert(i, Arc::new(d.lost));
                self.waiting.extend(d.falls);
                self.at = None;
            }
            // A change that came meanwhile starts from this result.
            if let (Some(Job { next: Some(ch), .. }), Some(town)) = (job, self.town.clone()) {
                self.start(&town, self.eye, ch);
            }
        }
        for _ in 0..PER_FRAME {
            let Some(f) = self.waiting.pop_front() else {
                break;
            };
            let id = world.spawn(f.mesh);
            world.set_place(id, f.at)?;
            let body = Body::dynamic()
                .shape(BodyShape::BoundingBox)
                .density(0.6)
                .friction(0.8)
                .bounce(0.15)
                .velocity(f.push);
            if world.add_body(id, body).is_err() {
                world.despawn(id);
                continue;
            }
            self.falling.push_back((id, 0.0, f.building));
        }
        for f in self.falling.iter_mut() {
            f.1 += dt;
        }
        while self.falling.len() > MAX_FALLING
            || self.falling.front().is_some_and(|f| f.1 > LIVE_SECONDS)
        {
            let Some((id, _, _)) = self.falling.pop_front() else {
                break;
            };
            world.despawn(id);
        }
        Ok(())
    }

    /// Takes the furniture out of the world (before switching towns).
    pub fn clear(&mut self, world: &mut World) {
        for (id, _, _) in std::mem::take(&mut self.falling) {
            world.despawn(id);
        }
        self.lost.clear();
        self.jobs.clear();
        self.waiting.clear();
        self.repaired.clear();
        for (_, id) in std::mem::take(&mut self.sets) {
            world.despawn_lod(id);
        }
        self.at = None;
        self.worker = None;
    }
}
