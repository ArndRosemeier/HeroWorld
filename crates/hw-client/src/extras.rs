//! What stands in the town besides its roads and buildings: props,
//! landmarks, grass, the pirates' roost and the streamed ground and woods;
//! and the views that stream on their own (the underground's layers).
//!
//! Behind the title these load without stopping the window: a worker thread
//! makes every mesh ([`ExtrasParts::build`], pure data, no `World`), and the
//! main thread puts them into the world a few per frame ([`ExtrasLoad::step`]).
//! The order is fixed and checked, not left to timing:
//!
//! 1. the worker's parts arrive (the only hand-over between the threads: one
//!    message on a channel; the worker never touches the `World`);
//! 2. the steps run strictly in the order queued: props, landmarks,
//!    grass, roost, then the ground stream, which draws the woods
//!    with the props' tree sets (asserted to be in by then);
//! 3. [`TownExtras`] is handed out only once every step is done and the
//!    ground around the camera is loaded.

use crate::factionview::{self, RoostParts};
use crate::grassview::GrassView;
use crate::landmarkview::LandmarkView;
use crate::propview::{self, PropView};
use crate::townmesh::TownGround;
use crate::undergroundview::UndergroundView;
use engine::prelude::*;
use engine::stream::{StreamSettings, WorldStream};
use hw_gen::urban::greenery::Greenery;
use hw_gen::urban::props::PropKind;
use hw_gen::urban::world::TownWorld;
use std::collections::{BTreeMap, VecDeque};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;
use std::time::Instant;

/// Longest the title waits for the ground around the camera once the
/// stream is set. Only how it looks hangs on it (the ground keeps
/// streaming in either way): around a moving camera the stream may not
/// report ready for a while.
const GROUND_WAIT_S: f32 = 10.0;
/// What the first frame waits for the ground when everything is made at once.
const PREPARE_S: f32 = 20.0;

/// The town's extras in the world, kept so they can be taken out again
/// when the title switches worlds.
#[derive(Default)]
pub struct TownExtras {
    pub lods: Vec<LodId>,
    pub entities: Vec<EntityId>,
    pub lights: Option<crate::nightlights::NightLights>,
    /// The Comet buildings' ceiling lamps (on all day: no sun in there).
    pub comet_lights: Option<crate::cometmesh::CometLights>,
    /// Their lifts: cars and sliding doors.
    pub lifts: Option<crate::cometmesh::LiftView>,
    pub signals: Option<crate::signalview::SignalView>,
    pub props: Option<PropView>,
    pub under: Option<UndergroundView>,
    pub deep: Option<crate::deepview::DeepView>,
    pub dystopia: Option<crate::dystopiaview::DystopiaView>,
    /// The airship mooring mast and its liner.
    pub mast: Option<crate::mastview::MastView>,
    /// The rocket port's launches.
    pub rocket: Option<crate::rocketview::RocketView>,
    pub atom: Option<crate::atomview::AtomView>,
    /// The mole folk's drill rig at work.
    pub drill: Option<crate::drillview::DrillView>,
    /// The weather tower's cloud, rain and rainbow.
    pub weather: Option<crate::weatherview::WeatherView>,
    pub grass: Option<GrassView>,
    pub waterfront: Option<crate::waterfrontview::WaterfrontView>,
}

impl TownExtras {
    pub fn despawn(&mut self, world: &mut World) {
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
        if let Some(mut r) = self.rocket.take() {
            r.clear(world);
        }
        if let Some(mut d) = self.dystopia.take() {
            d.clear(world);
        }
        if let Some(mut m) = self.mast.take() {
            m.clear(world);
        }
        if let Some(mut a) = self.atom.take() {
            a.clear(world);
        }
        if let Some(mut d) = self.drill.take() {
            d.clear(world);
        }
        if let Some(mut w) = self.weather.take() {
            w.clear(world);
        }
        if let Some(g) = self.grass.take() {
            g.despawn(world);
        }
    }
}

/// Every mesh of the extras, made without a `World` (on a worker thread).
pub struct ExtrasParts {
    /// One per kind of [`PropKind::all`], in that order.
    props: Vec<Vec<LodLevel>>,
    landmarks: Vec<(Vec<LodLevel>, Place)>,
    grass: Vec<Vec<LodLevel>>,
    /// The furniture of the Comet buildings: a model per look and where
    /// each stands.
    furniture: Vec<(Vec<LodLevel>, Vec<Place>)>,
    roost: Option<RoostParts>,
}

impl ExtrasParts {
    /// Makes the meshes on `threads` threads (the main thread keeps a core
    /// of its own while the title shows).
    pub fn build(town: &TownWorld, green: &Greenery, threads: usize) -> EngineResult<Self> {
        Ok(Self {
            props: PropView::models(threads)?,
            landmarks: LandmarkView::models(town),
            grass: GrassView::models(green)?,
            furniture: crate::cometmesh::furniture_models(town),
            roost: factionview::roost_parts(town),
        })
    }
}

/// One thing to put into the world, in the order queued.
enum Step {
    Prop(Vec<LodLevel>),
    Landmark(Vec<LodLevel>, Place),
    Grass(Vec<LodLevel>),
    Furniture(Vec<LodLevel>, Vec<Place>),
    Roost(Box<RoostParts>),
    /// The ground stream (needs every prop set: the woods use the trees').
    Stream,
    /// Waits until the stream has the ground around the camera.
    Ground(Option<Instant>),
}

impl Step {
    fn name(&self) -> &'static str {
        match self {
            Step::Prop(_) => "a prop model",
            Step::Landmark(..) => "a landmark",
            Step::Grass(_) => "a grass clump",
            Step::Furniture(..) => "a piece of furniture",
            Step::Roost(_) => "the roost",
            Step::Stream => "the ground stream",
            Step::Ground(_) => "the ground",
        }
    }
}

/// Steps put into the world per frame at most (each spawns a few meshes).
const STEPS_PER_FRAME: usize = 2;

/// A step this slow is noted in the hitch log.
const SLOW_STEP_MS: f64 = 12.0;

/// The extras coming in behind the title.
pub struct ExtrasLoad {
    town: Arc<TownWorld>,
    green: Arc<Greenery>,
    /// The worker's parts, until they arrive.
    worker: Option<Receiver<EngineResult<ExtrasParts>>>,
    steps: VecDeque<Step>,
    total: usize,
    // What is in the world so far (taken out again by `abandon`).
    prop_sets: Vec<LodId>,
    prop_bounds: Vec<(Vec3, Vec3)>,
    landmark_sets: Vec<LodId>,
    grass_sets: Vec<LodId>,
    roost_lods: Vec<LodId>,
    roost_entities: Vec<EntityId>,
    streamed: bool,
    started: Instant,
}

impl ExtrasLoad {
    /// Starts making the meshes on a worker thread.
    pub fn start(town: Arc<TownWorld>, green: Arc<Greenery>) -> Self {
        let (tx, rx) = channel();
        let (t, g) = (town.clone(), green.clone());
        std::thread::Builder::new()
            .name("town extras".into())
            .spawn(move || {
                // Nobody waiting any more (the title switched worlds): fine.
                let _ = tx.send(ExtrasParts::build(&t, &g, background_threads()));
            })
            .expect("town extras worker");
        Self {
            worker: Some(rx),
            ..Self::empty(town, green)
        }
    }

    /// How far it is (0..1): the worker's half, then the steps.
    pub fn progress(&self) -> f32 {
        if self.worker.is_some() {
            return 0.0;
        }
        let left = self.steps.len() as f32;
        (1.0 - left / self.total.max(1) as f32).clamp(0.0, 1.0)
    }

    /// What it is doing, for the hitch log.
    pub fn state(&self) -> String {
        match self.steps.front() {
            _ if self.worker.is_some() => format!(
                "meshes on the worker for {:.1} s",
                self.started.elapsed().as_secs_f32()
            ),
            Some(Step::Ground(_)) => "waiting for the ground around the camera".into(),
            _ => format!("{} of {} put in", self.total - self.steps.len(), self.total),
        }
    }

    /// Puts parts into the world for about `budget_ms` (at least one step),
    /// and hands out the extras once all are in.
    pub fn step(&mut self, world: &mut World, budget_ms: f64) -> EngineResult<Option<TownExtras>> {
        if let Some(rx) = &self.worker {
            match rx.try_recv() {
                Ok(parts) => {
                    self.worker = None;
                    self.queue(parts?);
                }
                Err(TryRecvError::Empty) => return Ok(None),
                Err(TryRecvError::Disconnected) => {
                    return Err(EngineError::InvalidValue(
                        "the town extras worker stopped without its meshes".into(),
                    ))
                }
            }
        }
        let t = Instant::now();
        let mut done = 0;
        loop {
            // Engine uploads what a step spawned in the frame after (a few
            // meshes each): a handful of steps at once stopped frames for
            // 40-180 ms on the title.
            if done == STEPS_PER_FRAME {
                return Ok(None);
            }
            done += 1;
            let Some(step) = self.steps.pop_front() else {
                return Ok(Some(self.finish()));
            };
            let (what, s) = (step.name(), Instant::now());
            if !self.run(world, step)? {
                // Still waiting (the ground): no more this frame.
                return Ok(None);
            }
            let ms = s.elapsed().as_secs_f64() * 1000.0;
            if ms > SLOW_STEP_MS {
                crate::hitch::event(format!("town extras: {what} took {ms:.1} ms"));
            }
            if t.elapsed().as_secs_f64() * 1000.0 >= budget_ms {
                return Ok(None);
            }
        }
    }

    fn queue(&mut self, p: ExtrasParts) {
        assert!(self.steps.is_empty(), "the parts arrive once");
        assert_eq!(
            p.props.len(),
            PropKind::all().len(),
            "a model per prop kind"
        );
        self.steps.extend(p.props.into_iter().map(Step::Prop));
        self.steps
            .extend(p.landmarks.into_iter().map(|(l, at)| Step::Landmark(l, at)));
        self.steps.extend(p.grass.into_iter().map(Step::Grass));
        self.steps.extend(
            p.furniture
                .into_iter()
                .map(|(l, at)| Step::Furniture(l, at)),
        );
        self.steps.extend(p.roost.map(|r| Step::Roost(Box::new(r))));
        self.steps.push_back(Step::Stream);
        self.steps.push_back(Step::Ground(None));
        self.total = self.steps.len();
    }

    /// Runs one step; false when it has to wait (it stays first in line).
    fn run(&mut self, world: &mut World, step: Step) -> EngineResult<bool> {
        match step {
            Step::Prop(levels) => {
                self.prop_bounds.push(propview::model_bounds(&levels));
                self.prop_sets.push(world.spawn_lod_at(levels, &[])?);
            }
            Step::Landmark(levels, at) => {
                self.landmark_sets.push(world.spawn_lod_at(levels, &[at])?)
            }
            Step::Grass(levels) => self.grass_sets.push(GrassView::install(world, levels)?),
            Step::Furniture(levels, at) => {
                self.landmark_sets.push(world.spawn_lod_at(levels, &at)?)
            }
            Step::Roost(r) => {
                let (lods, entities) = factionview::install_roost(world, *r)?;
                self.roost_lods = lods;
                self.roost_entities = entities;
            }
            Step::Stream => {
                let kinds = PropKind::all();
                assert_eq!(
                    self.prop_sets.len(),
                    kinds.len(),
                    "every prop set is in before the woods stream with them"
                );
                set_town_stream(
                    world,
                    &self.town,
                    self.green.clone(),
                    &kinds,
                    &self.prop_sets,
                )?;
                self.streamed = true;
            }
            Step::Ground(since) => {
                assert!(self.streamed, "the ground waits on its stream");
                let since = since.unwrap_or_else(Instant::now);
                let ready = world.stream().is_none_or(|s| s.is_ready());
                if !ready && since.elapsed().as_secs_f32() < GROUND_WAIT_S {
                    self.steps.push_front(Step::Ground(Some(since)));
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    fn finish(&mut self) -> TownExtras {
        assert!(self.worker.is_none() && self.steps.is_empty() && self.streamed);
        let props = PropView::with_sets(
            self.town.clone(),
            std::mem::take(&mut self.prop_sets),
            std::mem::take(&mut self.prop_bounds),
        );
        let landmark_sets = std::mem::take(&mut self.landmark_sets);
        let grass = GrassView::with_sets(
            self.town.clone(),
            self.green.clone(),
            std::mem::take(&mut self.grass_sets),
        );
        let mut lods: Vec<LodId> = props.sets.iter().copied().chain(landmark_sets).collect();
        lods.append(&mut self.roost_lods);
        let entities = std::mem::take(&mut self.roost_entities);
        self.streamed = false;
        assemble(&self.town, props, grass, lods, entities)
    }

    /// Takes out what is already in (the title switched worlds).
    pub fn abandon(mut self, world: &mut World) {
        if self.streamed {
            let _ = world.set_stream(None);
        }
        let lods = self
            .prop_sets
            .drain(..)
            .chain(self.landmark_sets.drain(..))
            .chain(self.grass_sets.drain(..))
            .chain(self.roost_lods.drain(..));
        for id in lods.collect::<Vec<_>>() {
            world.despawn_lod(id);
        }
        for e in self.roost_entities.drain(..) {
            world.despawn(e);
        }
    }
}

/// The views that need no meshes of their own up front.
fn assemble(
    town: &Arc<TownWorld>,
    props: PropView,
    grass: GrassView,
    lods: Vec<LodId>,
    entities: Vec<EntityId>,
) -> TownExtras {
    TownExtras {
        grass: Some(grass),
        lods,
        entities,
        waterfront: Some(crate::waterfrontview::WaterfrontView::new(town.clone())),
        lights: Some(crate::nightlights::NightLights::new(town.clone())),
        comet_lights: Some(crate::cometmesh::CometLights::new(town)),
        lifts: Some(crate::cometmesh::LiftView::new(town)),
        signals: Some(crate::signalview::SignalView::new()),
        props: Some(props),
        // The underground's tiles stream on their own workers from the
        // first update.
        under: Some(UndergroundView::new(town.clone())),
        deep: Some(crate::deepview::DeepView::new(town.clone())),
        drill: Some(crate::drillview::DrillView::new(town)),
        dystopia: Some(crate::dystopiaview::DystopiaView::new(town)),
        mast: Some(crate::mastview::MastView::new(town)),
        rocket: Some(crate::rocketview::RocketView::new(town)),
        atom: Some(crate::atomview::AtomView::new(town)),
        weather: Some(crate::weatherview::WeatherView::new(town)),
    }
}

/// Everything at once, on this thread: for a start straight into the game
/// (and screenshots), where the first frame has to show it all.
pub fn spawn_now(
    world: &mut World,
    town: Arc<TownWorld>,
    green: Arc<Greenery>,
) -> EngineResult<TownExtras> {
    let t = Instant::now();
    let all = std::thread::available_parallelism().map_or(4, |n| n.get());
    let parts = ExtrasParts::build(&town, &green, all)?;
    let mut load = ExtrasLoad::empty(town.clone(), green);
    load.queue(parts);
    let mut extras = loop {
        if let Some(e) = load.step(world, f64::INFINITY)? {
            break e;
        }
    };
    world.prepare_stream(PREPARE_S);
    let eye = world.camera().eye();
    if let Some(u) = extras.under.as_mut() {
        u.prime(world, &town, eye)?;
    }
    if let Some(d) = extras.deep.as_mut() {
        d.prime(world, &town, eye)?;
    }
    if let Some(d) = extras.dystopia.as_mut() {
        d.wait();
    }
    eprintln!("town extras in {:.2} s", t.elapsed().as_secs_f64());
    Ok(extras)
}

impl ExtrasLoad {
    fn empty(town: Arc<TownWorld>, green: Arc<Greenery>) -> Self {
        Self {
            town,
            green,
            worker: None,
            steps: VecDeque::new(),
            total: 0,
            prop_sets: Vec::new(),
            prop_bounds: Vec::new(),
            landmark_sets: Vec::new(),
            grass_sets: Vec::new(),
            roost_lods: Vec::new(),
            roost_entities: Vec::new(),
            streamed: false,
            started: Instant::now(),
        }
    }
}

/// Threads for work behind the title: all cores but one, which the main
/// thread keeps for drawing.
fn background_threads() -> usize {
    std::thread::available_parallelism().map_or(2, |n| n.get().saturating_sub(1).max(1))
}

/// Engine streams the town's ground and water, and the country's woods
/// drawn with the props' tree sets.
pub fn set_town_stream(
    world: &mut World,
    town: &Arc<TownWorld>,
    green: Arc<Greenery>,
    kinds: &[PropKind],
    prop_sets: &[LodId],
) -> EngineResult<()> {
    let trees: BTreeMap<PropKind, LodId> = kinds
        .iter()
        .zip(prop_sets)
        .filter(|(k, _)| matches!(k, PropKind::Tree(..)))
        .map(|(k, id)| (*k, *id))
        .collect();
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
        town: town.clone(),
        green,
        trees,
    };
    let stream = WorldStream::new(ground, settings).with_terrain(|t| t.with_beach(1.8));
    world.set_stream(Some(stream))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The parts cross from the worker to the main thread.
    fn sendable<T: Send + 'static>() {}

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn parts_are_made_without_a_world_and_cross_threads() {
        sendable::<EngineResult<ExtrasParts>>();
        let g = hw_sim::GeneratedTown::generate(hw_core::Seed(1), 20_000.0);
        let green = Arc::new(Greenery::new(&g.world));
        let t = Instant::now();
        let p = ExtrasParts::build(&g.world, &green, background_threads()).expect("parts");
        eprintln!(
            "extras parts in {:.2} s: {} prop models, {} landmark sets, {} grass clumps",
            t.elapsed().as_secs_f64(),
            p.props.len(),
            p.landmarks.len(),
            p.grass.len()
        );
        assert_eq!(p.props.len(), PropKind::all().len());
        // The queue puts the stream after every prop set, and the ground last.
        let mut load = ExtrasLoad::empty(g.world.clone(), green);
        load.queue(p);
        let n = load.steps.len();
        let stream = load.steps.iter().position(|s| matches!(s, Step::Stream));
        let last_prop = load.steps.iter().rposition(|s| matches!(s, Step::Prop(_)));
        assert!(last_prop < stream);
        assert_eq!(stream, Some(n - 2));
        assert!(matches!(load.steps.back(), Some(Step::Ground(None))));
    }
}
