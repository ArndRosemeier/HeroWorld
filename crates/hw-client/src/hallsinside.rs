//! The insides of the town's buildings of halls (`cometmesh::halls`),
//! drawn round the eye only.
//!
//! The town's cells draw every building's outside. The inside of a
//! building of halls (floors, walls, stairs, rails, lamps) is thousands of
//! faces, and a town has such buildings by the hundred: drawn with the
//! cells, a small town took gigabytes for rooms nobody was near. So each
//! inside is its own set of meshes, made on a worker thread when the eye
//! comes within [`SHOW`] of the building and dropped beyond [`DROP`].
//! Windows show nothing of a room from further off.
//!
//! A damaged building draws its own inside, cut into the pieces it breaks
//! in (`damageview`): it is left out here while the town hides it.

use crate::citymesh::{halls_frame, MeshBuilder, BLOCK_TOP};
use engine::prelude::*;
use glam::{DVec2, DVec3};
use hw_gen::building::{BuildingPlan, BuildingSite};
use hw_gen::urban::world::TownWorld;
use std::collections::BTreeMap;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;

/// An inside is drawn while the eye is within this of the building's
/// walls (m), and dropped beyond [`DROP`].
pub const SHOW: f64 = 90.0;
pub const DROP: f64 = 120.0;
/// Insides being made at once: the nearest first.
const AT_ONCE: usize = 3;
/// The list of buildings near the eye is made anew when it has moved this
/// far (m).
const STEP: f64 = 6.0;

#[derive(Default)]
pub struct HallInsides {
    shown: BTreeMap<u64, Vec<EntityId>>,
    making: BTreeMap<u64, Receiver<Vec<PreparedMesh>>>,
    /// Buildings of halls near the eye where the list was made: id, site,
    /// ground height.
    near: Vec<(u64, BuildingSite, f64)>,
    at: Option<DVec2>,
    /// Wait for the workers (screenshots: the picture is taken at a set
    /// frame).
    pub wait: bool,
}

/// How far `p` is from the building's walls (0 inside it).
fn distance(site: &BuildingSite, p: DVec2) -> f64 {
    let d = p - site.origin;
    let l = DVec2::new(d.dot(site.along), d.dot(site.inward));
    (l - l.clamp(DVec2::ZERO, site.size)).length()
}

/// The meshes of one building's inside, in city space at its own ground.
fn inside(site: &BuildingSite, base: f64) -> Vec<PreparedMesh> {
    let plan = BuildingPlan::generate(site);
    let mut b = MeshBuilder::new();
    b.frame = Some(halls_frame(site));
    crate::cometmesh::halls(&mut b, &plan);
    b.frame = None;
    let mut out = MeshBuilder::new();
    out.append(b, Vec3::new(0.0, (base - BLOCK_TOP) as f32, 0.0));
    out.finish_parts().into_iter().map(Mesh::prepare).collect()
}

impl HallInsides {
    /// `wait`: wait for the workers (screenshots).
    pub fn new(wait: bool) -> Self {
        Self {
            wait,
            ..Default::default()
        }
    }

    /// Once a frame: puts finished insides into the world, drops the ones
    /// left behind and starts the nearest missing ones. `hidden` says
    /// which buildings the town leaves out (they are drawn broken).
    pub fn update(
        &mut self,
        world: &mut World,
        town: &Arc<TownWorld>,
        eye: Vec3,
        hidden: &dyn Fn(u64) -> bool,
    ) {
        let c = DVec2::new(eye.x as f64, eye.z as f64);
        if self.at.is_none_or(|a| a.distance(c) >= STEP) {
            self.at = Some(c);
            let m = DVec2::splat(DROP + STEP + 80.0);
            self.near = town
                .quarters_in(c - m, c + m)
                .iter()
                .flat_map(|q| {
                    q.buildings_with_ids()
                        .filter(|(_, b)| b.site.halls.is_some())
                        .filter(|(_, b)| distance(&b.site, c) < DROP + STEP)
                        .map(|(id, b)| (id.0, b.site.clone(), b.base))
                        .collect::<Vec<_>>()
                })
                .collect();
        }
        // Finished ones in, unless the eye has left or the building broke
        // meanwhile.
        let mut landed = Vec::new();
        let wait = self.wait;
        self.making.retain(|&i, rx| {
            let got = if wait {
                rx.recv().map_err(|_| TryRecvError::Disconnected)
            } else {
                rx.try_recv()
            };
            match got {
                Ok(meshes) => {
                    landed.push((i, meshes));
                    false
                }
                Err(TryRecvError::Empty) => true,
                Err(TryRecvError::Disconnected) => false,
            }
        });
        let dist = |near: &[(u64, BuildingSite, f64)], i: u64| {
            near.iter()
                .find(|n| n.0 == i)
                .map_or(f64::INFINITY, |n| distance(&n.1, c))
        };
        for (i, meshes) in landed {
            if hidden(i) || dist(&self.near, i) > DROP {
                continue;
            }
            let ids = meshes
                .into_iter()
                .map(|m| {
                    let id = world.spawn_prepared(m);
                    world.set_name(id, format!("inside of building {i}"));
                    id
                })
                .collect();
            self.shown.insert(i, ids);
        }
        let gone: Vec<u64> = self
            .shown
            .keys()
            .copied()
            .filter(|&i| hidden(i) || dist(&self.near, i) > DROP)
            .collect();
        for i in gone {
            for e in self.shown.remove(&i).into_iter().flatten() {
                world.despawn(e);
            }
        }
        // The nearest missing ones.
        let mut want: Vec<(f64, usize)> = self
            .near
            .iter()
            .enumerate()
            .filter(|(_, n)| {
                !self.shown.contains_key(&n.0) && !self.making.contains_key(&n.0) && !hidden(n.0)
            })
            .map(|(k, n)| (distance(&n.1, c), k))
            .filter(|(d, _)| *d < SHOW)
            .collect();
        want.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, k) in want {
            if self.making.len() >= AT_ONCE {
                break;
            }
            let (i, site, base) = self.near[k].clone();
            let (tx, rx) = channel();
            let made = std::thread::Builder::new()
                .name("halls inside".into())
                .spawn(move || {
                    engine::parallel::background_thread();
                    let _ = tx.send(inside(&site, base));
                });
            if made.is_ok() {
                self.making.insert(i, rx);
            }
        }
    }

    /// Is every inside within `r` of `p` drawn? (For screenshots: wait
    /// before the picture.)
    pub fn ready_near(&self, p: DVec3, r: f64) -> bool {
        let c = DVec2::new(p.x, p.z);
        self.making.is_empty()
            && self
                .near
                .iter()
                .filter(|n| distance(&n.1, c) < r.min(SHOW - STEP))
                .all(|n| self.shown.contains_key(&n.0))
    }

    /// Forgets everything (before switching towns).
    pub fn clear(&mut self, world: &mut World) {
        for (_, ids) in std::mem::take(&mut self.shown) {
            for e in ids {
                world.despawn(e);
            }
        }
        self.making.clear();
        self.near.clear();
        self.at = None;
    }
}
