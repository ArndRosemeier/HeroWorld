//! Streaming tiles by distance: a worker thread builds a tile's meshes when
//! the camera comes near, and the view takes them out again when it moves
//! away. This is the bookkeeping only (what to ask for, what arrived, what to
//! drop), without a `World`, so tests can walk a camera across a whole town
//! and check that what is held stays bounded.
//!
//! Ordering is explicit, not left to timing: the main thread owns every
//! decision; the worker only builds the keys it is sent, and skips a key the
//! main thread has stopped wanting since (it reads the wanted set under a
//! lock). A result for a key that is no longer wanted when it arrives is
//! dropped, so a tile is resident only while it is wanted or kept.

use engine::prelude::*;
use glam::{DVec2, DVec3, Vec3};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// What one [`TileStream::step`] changed.
pub struct Changes<K, B> {
    /// Built tiles to put in (each key at most once while resident).
    pub arrived: Vec<(K, B)>,
    /// Resident tiles to take out.
    pub gone: Vec<K>,
}

/// Which tiles are asked for, which are in, and the worker building them.
pub struct TileStream<K: Ord + Copy + Send + 'static, B: Send + 'static> {
    jobs: Sender<K>,
    done: Receiver<(K, Option<B>)>,
    /// Keys the worker should still build (shared with it).
    wanted: Arc<Mutex<BTreeSet<K>>>,
    /// Sent to the worker, no answer yet.
    asked: BTreeSet<K>,
    /// Put in and not taken out yet.
    resident: BTreeSet<K>,
}

impl<K: Ord + Copy + Send + 'static, B: Send + 'static> TileStream<K, B> {
    /// Starts the worker thread `name`, which builds a key with `build`.
    pub fn new(name: &str, build: impl Fn(K) -> B + Send + 'static) -> Self {
        let (jobs, rx) = channel::<K>();
        let (tx, done) = channel::<(K, Option<B>)>();
        let wanted = Arc::new(Mutex::new(BTreeSet::new()));
        let still = wanted.clone();
        std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                engine::parallel::background_thread();
                while let Ok(k) = rx.recv() {
                    let want = still.lock().map(|w| w.contains(&k)).unwrap_or(false);
                    let b = want.then(|| build(k));
                    if tx.send((k, b)).is_err() {
                        break;
                    }
                }
            })
            .expect("tile stream worker");
        Self {
            jobs,
            done,
            wanted,
            asked: BTreeSet::new(),
            resident: BTreeSet::new(),
        }
    }

    /// One step: `want` is every key wanted now, nearest first; `keep`
    /// says whether a resident (or arriving) key may stay. Wanted keys are
    /// always kept. Sends new keys to the worker and returns what to put
    /// in and take out.
    pub fn step(&mut self, want: &[K], keep: impl Fn(K) -> bool) -> Changes<K, B> {
        let wanted: BTreeSet<K> = want.iter().copied().collect();
        let stays = |k: K| wanted.contains(&k) || keep(k);
        let mut arrived = Vec::new();
        while let Ok((k, b)) = self.done.try_recv() {
            self.asked.remove(&k);
            if let Some(b) = b {
                if stays(k) && !self.resident.contains(&k) {
                    self.resident.insert(k);
                    arrived.push((k, b));
                }
            }
        }
        let gone: Vec<K> = self
            .resident
            .iter()
            .copied()
            .filter(|k| !stays(*k))
            .collect();
        for k in &gone {
            self.resident.remove(k);
        }
        // The worker builds only what is wanted now; keys still in its queue
        // that are not are skipped there.
        let mut shared = self.wanted.lock().expect("tile stream lock");
        *shared = wanted;
        drop(shared);
        for &k in want {
            if !self.resident.contains(&k) && self.asked.insert(k) {
                let _ = self.jobs.send(k);
            }
        }
        Changes { arrived, gone }
    }

    /// Marks a tile built on this thread (to prime the first frame) as in.
    /// False when it already was.
    pub fn insert(&mut self, k: K) -> bool {
        self.resident.insert(k)
    }

    pub fn is_resident(&self, k: K) -> bool {
        self.resident.contains(&k)
    }

    /// Tiles in now.
    pub fn resident(&self) -> usize {
        self.resident.len()
    }

    /// Forgets everything (the view took all its tiles out).
    pub fn clear(&mut self) {
        self.resident.clear();
        if let Ok(mut w) = self.wanted.lock() {
            w.clear();
        }
    }

    /// Waits for the worker to answer every key asked so far (tests).
    #[cfg(test)]
    pub fn settle(&mut self, want: &[K], keep: impl Fn(K) -> bool + Copy) -> Changes<K, B> {
        let mut all = self.step(want, keep);
        while !self.asked.is_empty() {
            if let Ok((k, b)) = self.done.recv() {
                self.asked.remove(&k);
                if let Some(b) = b {
                    let w = want.contains(&k) || keep(k);
                    if w && !self.resident.contains(&k) {
                        self.resident.insert(k);
                        all.arrived.push((k, b));
                    }
                }
            }
        }
        all
    }
}

// ------------------------------------------------------------ underground

/// The two layers of an underground tile: what stands on the street (lift
/// pavilions, kiosks), streamed wide; and the rooms below, streamed only
/// while the camera is below ground or near a way down.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Layer {
    Street,
    Below,
}

/// A tile of one layer: a square of the view's grid.
pub type Key = (Layer, i64, i64);

/// A lamp of the rooms below, lit when the camera comes near.
#[derive(Clone, Copy, Debug)]
pub struct Glow {
    pub pos: DVec3,
    pub color: Color,
    pub strength: f32,
    pub range: f32,
}

/// One layer of one tile, made on the worker.
#[derive(Default)]
pub struct Built {
    /// Prepared on the worker: putting one in is only bookkeeping.
    pub meshes: Vec<PreparedMesh>,
    pub glows: Vec<Glow>,
    /// Ways down from the street (street layer): near one, the rooms come.
    pub openings: Vec<DVec2>,
}

/// Load within `load`, drop beyond `drop` (plan distance to the tile).
#[derive(Clone, Copy, Debug)]
pub struct Reach {
    pub load: f64,
    pub drop: f64,
}

/// How an underground view streams.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// Worker thread and mesh names.
    pub name: &'static str,
    /// Side of a tile (m); tile (i, j) covers `[i, i+1) * size`.
    pub size: f64,
    pub street: Reach,
    pub below: Reach,
    /// The rooms are wanted within this of an opening (kept within 1.5x).
    pub near: f64,
    /// Below the ground by this much counts as being down there.
    pub under: f64,
    /// Lamps within this of the camera light up, at most `pool`.
    pub lamp_range: f64,
    pub pool: usize,
}

/// Plan distance from `q` to tile `(i, j)` of side `s`.
fn tile_dist(s: f64, q: DVec2, i: i64, j: i64) -> f64 {
    let c = DVec2::new((i as f64 + 0.5) * s, (j as f64 + 0.5) * s);
    ((q - c).abs() - DVec2::splat(s * 0.5))
        .max(DVec2::ZERO)
        .length()
}

struct Held {
    glows: Vec<Glow>,
    openings: Vec<DVec2>,
    triangles: usize,
}

/// The bookkeeping of an underground view, without a `World`: which tiles
/// are wanted round the camera, what they hold, what has to go.
pub struct UnderTiles {
    set: Settings,
    land: Box<dyn Fn((i64, i64)) -> bool>,
    stream: TileStream<Key, Built>,
    held: BTreeMap<Key, Held>,
    /// The rooms below are streamed (the camera is down there or near).
    pub below: bool,
}

impl UnderTiles {
    /// `land` says which tiles can hold anything; `build` makes a tile's
    /// layer on the worker.
    pub fn new(
        set: Settings,
        land: impl Fn((i64, i64)) -> bool + 'static,
        build: impl Fn(Key) -> Built + Send + 'static,
    ) -> Self {
        Self {
            set,
            land: Box::new(land),
            stream: TileStream::new(set.name, build),
            held: BTreeMap::new(),
            below: false,
        }
    }

    pub fn settings(&self) -> &Settings {
        &self.set
    }

    fn dist(&self, q: DVec2, i: i64, j: i64) -> f64 {
        tile_dist(self.set.size, q, i, j)
    }

    /// Tiles of `layer` within `r` of `q`, nearest first.
    fn around(&self, layer: Layer, q: DVec2, r: f64) -> Vec<Key> {
        let s = self.set.size;
        let lo = ((q - DVec2::splat(r)) / s).floor();
        let hi = ((q + DVec2::splat(r)) / s).floor();
        let mut out: Vec<(f64, Key)> = Vec::new();
        for i in lo.x as i64..=hi.x as i64 {
            for j in lo.y as i64..=hi.y as i64 {
                let d = self.dist(q, i, j);
                if d < r && (self.land)((i, j)) {
                    out.push((d, (layer, i, j)));
                }
            }
        }
        out.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        out.into_iter().map(|x| x.1).collect()
    }

    /// The nearest opening of a held street tile.
    pub fn nearest_opening(&self, q: DVec2) -> f64 {
        self.held
            .values()
            .flat_map(|h| h.openings.iter())
            .map(|o| o.distance(q))
            .fold(f64::MAX, f64::min)
    }

    /// One step for a camera at `eye` over ground `ground`: asks the worker
    /// for what is wanted, and returns the tiles to put in and take out.
    pub fn step(&mut self, eye: DVec3, ground: f64) -> (Vec<(Key, Vec<PreparedMesh>)>, Vec<Key>) {
        let q = DVec2::new(eye.x, eye.z);
        let set = self.set;
        let down = eye.y < ground - set.under;
        let opening = self.nearest_opening(q);
        let lifted = eye.y < ground + 40.0;
        let want_below = down || (opening < set.near && lifted);
        let keep_below = down || (opening < set.near * 1.5 && lifted);
        self.below = want_below || (self.below && keep_below);
        let mut want = self.around(Layer::Street, q, set.street.load);
        if self.below {
            want.extend(self.around(Layer::Below, q, set.below.load));
        }
        let below = self.below;
        let changes = self.stream.step(&want, |k| {
            let d = tile_dist(set.size, q, k.1, k.2);
            match k.0 {
                Layer::Street => d < set.street.drop,
                Layer::Below => below && d < set.below.drop,
            }
        });
        self.take(changes)
    }

    fn take(&mut self, c: Changes<Key, Built>) -> (Vec<(Key, Vec<PreparedMesh>)>, Vec<Key>) {
        for k in &c.gone {
            self.held.remove(k);
        }
        let mut put = Vec::new();
        for (k, b) in c.arrived {
            let triangles = b.meshes.iter().map(|m| m.triangle_count()).sum();
            self.held.insert(
                k,
                Held {
                    glows: b.glows,
                    openings: b.openings,
                    triangles,
                },
            );
            put.push((k, b.meshes));
        }
        (put, c.gone)
    }

    /// Builds the wanted tiles round `eye` on this thread (the first frame
    /// and screenshots); returns what to put in.
    pub fn prime(
        &mut self,
        eye: DVec3,
        ground: f64,
        build: impl Fn(Key) -> Built,
    ) -> (Vec<(Key, Vec<PreparedMesh>)>, Vec<Key>) {
        // Twice: the street tiles bring the openings that may call the rooms.
        let mut put = Vec::new();
        let mut gone = Vec::new();
        for _ in 0..2 {
            let (p, g) = self.step(eye, ground);
            put.extend(p);
            gone.extend(g);
            let q = DVec2::new(eye.x, eye.z);
            let mut want = self.around(Layer::Street, q, self.set.street.load);
            if self.below {
                want.extend(self.around(Layer::Below, q, self.set.below.load));
            }
            for k in want {
                if self.stream.insert(k) {
                    let b = build(k);
                    let (p, _) = self.take(Changes {
                        arrived: vec![(k, b)],
                        gone: Vec::new(),
                    });
                    put.extend(p);
                }
            }
        }
        (put, gone)
    }

    /// Lamps to light round `eye`, nearest first (none unless below).
    pub fn lamps(&self, eye: DVec3) -> Vec<Glow> {
        if !self.below {
            return Vec::new();
        }
        let mut near: Vec<(f64, Glow)> = self
            .held
            .values()
            .flat_map(|h| h.glows.iter())
            .map(|g| (g.pos.distance(eye), *g))
            .filter(|(d, _)| *d < self.set.lamp_range)
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.truncate(self.set.pool);
        near.into_iter().map(|x| x.1).collect()
    }

    /// Tiles held now, and their triangles.
    pub fn held(&self) -> (usize, usize) {
        (
            self.held.len(),
            self.held.values().map(|h| h.triangles).sum(),
        )
    }

    pub fn clear(&mut self) {
        self.held.clear();
        self.stream.clear();
        self.below = false;
    }

    /// Waits for the worker, for tests: steps until nothing is on its way.
    #[cfg(test)]
    pub fn settle(&mut self, eye: DVec3, ground: f64) -> (usize, usize) {
        let mut ins = 0;
        let mut outs = 0;
        for _ in 0..4 {
            let (p, g) = self.step(eye, ground);
            ins += p.len();
            outs += g.len();
            while !self.stream.asked.is_empty() {
                std::thread::sleep(std::time::Duration::from_millis(2));
                let (p, g) = self.step(eye, ground);
                ins += p.len();
                outs += g.len();
            }
        }
        (ins, outs)
    }
}

/// An underground view in the world: the tiles' meshes and the lamps.
pub struct UnderView {
    pub tiles: UnderTiles,
    entities: BTreeMap<Key, Vec<EntityId>>,
    /// Arrived, not put in yet (a few per frame).
    waiting: VecDeque<(Key, Vec<PreparedMesh>)>,
    lit: Vec<LampId>,
    at: Option<(i32, i32, i32)>,
}

/// Tiles put into the world per frame at most.
const PUT_PER_FRAME: usize = 6;

impl UnderView {
    pub fn new(tiles: UnderTiles) -> Self {
        Self {
            tiles,
            entities: BTreeMap::new(),
            waiting: VecDeque::new(),
            lit: Vec::new(),
            at: None,
        }
    }

    /// Streams round `eye` (ground under it `ground`) and lights the lamps.
    pub fn update(&mut self, world: &mut World, eye: Vec3, ground: f64) -> EngineResult<()> {
        let (put, gone) = self.tiles.step(eye.as_dvec3(), ground);
        self.apply(world, put, gone, PUT_PER_FRAME);
        self.light(world, eye)
    }

    /// Builds what is wanted round `eye` now and puts it all in.
    pub fn prime(
        &mut self,
        world: &mut World,
        eye: Vec3,
        ground: f64,
        build: impl Fn(Key) -> Built,
    ) -> EngineResult<()> {
        let (put, gone) = self.tiles.prime(eye.as_dvec3(), ground, build);
        self.apply(world, put, gone, usize::MAX);
        self.light(world, eye)
    }

    fn apply(
        &mut self,
        world: &mut World,
        put: Vec<(Key, Vec<PreparedMesh>)>,
        gone: Vec<Key>,
        budget: usize,
    ) {
        self.waiting.extend(put);
        for k in gone {
            self.waiting.retain(|w| w.0 != k);
            for id in self.entities.remove(&k).into_iter().flatten() {
                world.despawn(id);
            }
        }
        let name = self.tiles.settings().name;
        for _ in 0..budget {
            let Some((k, meshes)) = self.waiting.pop_front() else {
                break;
            };
            let ids = meshes
                .into_iter()
                .map(|m| {
                    let id = world.spawn_prepared(m);
                    world.set_name(id, format!("{name} {:?} {} {}", k.0, k.1, k.2));
                    id
                })
                .collect();
            self.entities.insert(k, ids);
        }
    }

    fn light(&mut self, world: &mut World, eye: Vec3) -> EngineResult<()> {
        if !self.tiles.below {
            self.clear_lamps(world);
            return Ok(());
        }
        let key = (
            (eye.x / 5.0).round() as i32,
            (eye.y / 5.0).round() as i32,
            (eye.z / 5.0).round() as i32,
        );
        if self.at == Some(key) {
            return Ok(());
        }
        self.at = Some(key);
        let near = self.tiles.lamps(eye.as_dvec3());
        for (k, g) in near.iter().enumerate() {
            let lamp = Lamp::point(g.pos.as_vec3(), g.color, g.strength, g.range);
            match self.lit.get(k) {
                Some(&id) => world.set_lamp(id, lamp)?,
                None => self.lit.push(world.add_lamp(lamp)?),
            }
        }
        for id in self.lit.drain(near.len()..) {
            world.remove_lamp(id);
        }
        Ok(())
    }

    fn clear_lamps(&mut self, world: &mut World) {
        for id in self.lit.drain(..) {
            world.remove_lamp(id);
        }
        self.at = None;
    }

    /// Takes everything out of the world.
    pub fn clear(&mut self, world: &mut World) {
        self.clear_lamps(world);
        self.waiting.clear();
        for (_, ids) in std::mem::take(&mut self.entities) {
            for id in ids {
                world.despawn(id);
            }
        }
        self.tiles.clear();
    }

    /// Tiles in the world now.
    pub fn loaded(&self) -> usize {
        self.entities.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_wanted_tiles_stay_and_stale_ones_are_skipped() {
        let mut s = TileStream::<i32, i32>::new("test tiles", |k| k * 10);
        let c = s.settle(&[1, 2, 3], |_| false);
        let mut got: Vec<i32> = c.arrived.iter().map(|a| a.1).collect();
        got.sort();
        assert_eq!(got, vec![10, 20, 30]);
        // Moving on: 1 is dropped, 2 kept by `keep`, 4 asked.
        let c = s.settle(&[3, 4], |k| k == 2);
        assert_eq!(c.gone, vec![1]);
        assert_eq!(c.arrived.iter().map(|a| a.0).collect::<Vec<_>>(), vec![4]);
        assert_eq!(s.resident(), 3);
        // Asked, then no longer wanted before it arrives: never resident.
        s.step(&[9], |_| false);
        let c = s.settle(&[], |_| false);
        assert!(c.arrived.is_empty());
        assert_eq!(s.resident(), 0);
    }
}

#[cfg(test)]
mod walk {
    use super::*;
    use crate::deepview::DeepView;
    use crate::undergroundview::UndergroundView;

    /// Walks a camera through a town: down a lift, along the tunnels, back
    /// up and far across town. What the underground views hold stays
    /// bounded, the rooms come only below or near a way down, and leaving
    /// lets everything go.
    #[test]
    fn the_underground_streams_and_memory_stays_bounded() {
        let g = hw_sim::GeneratedTown::generate(hw_core::Seed(1), 40_000.0);
        let w = g.world.clone();
        let u = w.underground();
        let lift = u
            .entrances
            .iter()
            .find(|e| !e.molehaven)
            .expect("a lift down to the tunnels");
        let tunnel = u
            .voids
            .iter()
            .find(|v| v.kind == hw_gen::urban::underground::VoidKind::Tunnel)
            .expect("a tunnel");
        // The old view held all of this at once, from the start.
        let all: usize = {
            let mut t = UndergroundView::tiles(w.clone());
            let lo = u
                .voids
                .iter()
                .flat_map(|v| v.corners())
                .fold(DVec2::MAX, DVec2::min);
            let hi = u
                .voids
                .iter()
                .flat_map(|v| v.corners())
                .fold(DVec2::MIN, DVec2::max);
            let mut sum = 0;
            let s = t.settings().size;
            for i in (lo.x / s).floor() as i64..=(hi.x / s).floor() as i64 {
                for j in (lo.y / s).floor() as i64..=(hi.y / s).floor() as i64 {
                    let b = crate::undergroundview::build(&w, (Layer::Below, i, j));
                    sum += b.meshes.iter().map(|m| m.triangle_count()).sum::<usize>();
                }
            }
            t.clear();
            sum
        };
        let ground = |p: DVec2| w.ground(p);
        let at = |p: DVec2, y: f64| DVec3::new(p.x, y, p.y);
        let (core, _) = w.spawn_line();
        let far = core + DVec2::new(w.half() * 0.8, 0.0);
        let mid = tunnel.world((tunnel.min + tunnel.max) * 0.5);
        // (place, height above ground)
        let mut path: Vec<(DVec2, f64)> = Vec::new();
        let walk = |path: &mut Vec<(DVec2, f64)>, a: DVec2, b: DVec2, dy: f64| {
            let n = (a.distance(b) / 60.0).ceil().max(1.0) as usize;
            for k in 0..=n {
                path.push((a.lerp(b, k as f64 / n as f64), dy));
            }
        };
        walk(&mut path, core, lift.pos + DVec2::new(150.0, 0.0), 1.7);
        walk(&mut path, lift.pos + DVec2::new(150.0, 0.0), lift.pos, 1.7);
        path.push((lift.pos, lift.bottom + 1.7 - ground(lift.pos)));
        walk(&mut path, lift.pos, mid, -6.0);
        walk(&mut path, mid, far, 1.7);
        for (name, mut t) in [
            ("underground", UndergroundView::tiles(w.clone())),
            ("deep", DeepView::tiles(w.clone())),
        ] {
            let mut peak = (0, 0);
            let mut went_below = false;
            for &(p, dy) in &path {
                let g = ground(p);
                t.settle(at(p, g + dy), g);
                let h = t.held();
                peak = (peak.0.max(h.0), peak.1.max(h.1));
                went_below |= t.below;
            }
            let end = t.held();
            eprintln!(
                "{name}: peak {} tiles, {} triangles; at the far end {} tiles, {} triangles; all of the shallow layer {all}",
                peak.0, peak.1, end.0, end.1
            );
            assert!(went_below, "{name}: the rooms came while below");
            assert!(!t.below, "{name}: back up, far from a way down");
            // Street and rooms within reach only: a bounded number of tiles.
            let s = *t.settings();
            let side = |r: f64| ((2.0 * r / s.size).ceil() + 2.0).powi(2) as usize;
            assert!(
                peak.0 <= side(s.street.drop) + side(s.below.drop),
                "{name}: {} tiles held",
                peak.0
            );
            // Far out of town only street tiles (pavilions) can be near.
            assert!(
                t.held.keys().all(|k| k.0 == Layer::Street),
                "{name}: no rooms held far up on the street"
            );
            if name == "underground" {
                assert!(peak.1 < all, "never all of it at once: {} of {all}", peak.1);
                assert!(
                    end.1 * 20 < all.max(1),
                    "almost nothing far away: {}",
                    end.1
                );
            }
        }
    }
}
