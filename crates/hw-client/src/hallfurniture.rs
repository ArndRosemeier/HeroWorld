//! The furniture of the town's buildings of halls
//! (`BuildingPlan::furniture`), drawn round the eye.
//!
//! One LOD set per look (a kind of piece, its width, its colours), made
//! the first time a building near the eye has one; as the eye moves, a
//! worker lists where every piece near it stands and the sets take those
//! places. Nothing is kept per building: what is out of reach is simply
//! not in the next list.

use crate::cometmesh::{look, piece_mesh, Look, FURNITURE_REACH, FURNITURE_SEGS};
use engine::prelude::*;
use glam::{DVec2, DVec3};
use hw_gen::layout::furnish::ItemKind;
use hw_gen::layout::Scheme;
use hw_gen::urban::world::TownWorld;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

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
}

fn scheme_key(s: Scheme) -> u8 {
    match s {
        Scheme::GrandHall => 0,
        Scheme::Atrium => 1,
        Scheme::SplitLevel => 2,
        Scheme::ShopHouse => 3,
        Scheme::Flats => 4,
    }
}

/// Every piece of furniture within [`REACH`] of `c`, by look, with models
/// for the looks not in `known`.
fn list(town: &TownWorld, c: DVec2, known: &BTreeSet<Key>) -> Listed {
    let m = DVec2::splat(REACH);
    let mut places: BTreeMap<Key, (Scheme, Look, Vec<Place>)> = BTreeMap::new();
    for q in town.quarters_in(c - m, c + m) {
        for b in &q.buildings {
            let s = &b.site;
            let mid = s.origin + (s.along * s.size.x + s.inward * s.size.y) * 0.5;
            if s.halls.is_none() || mid.distance(c) > REACH {
                continue;
            }
            let Some(plan) = s.layout() else { continue };
            for it in hw_gen::layout::furnish::items(&plan) {
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
    ) -> EngineResult<()> {
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
        let c = DVec2::new(eye.x as f64, eye.z as f64);
        // Planning the quarters and the floors, and building new models,
        // takes its time: never on the frame's thread.
        std::thread::Builder::new()
            .name("hall furniture".into())
            .spawn(move || {
                let _ = tx.send(list(&town, c, &known));
            })
            .map_err(EngineError::Io)?;
        self.worker = Some(rx);
        Ok(())
    }

    /// Takes the furniture out of the world (before switching towns).
    pub fn clear(&mut self, world: &mut World) {
        for (_, id) in std::mem::take(&mut self.sets) {
            world.despawn_lod(id);
        }
        self.at = None;
        self.worker = None;
    }
}
