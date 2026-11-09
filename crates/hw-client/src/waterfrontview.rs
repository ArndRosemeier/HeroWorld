//! The waterfront in the game: granite quay walls with a coping stone, and
//! steps down to a landing at the water.

use crate::citymesh::MeshBuilder;
use engine::prelude::*;
use engine::texgen::FacadeLayer;
use glam::{DVec2, DVec3};
use hw_gen::urban::waterfront::{QuayWall, WaterSteps, STEPS_WIDTH};
use hw_gen::urban::world::{TownWorld, WATER_LEVEL};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Side of the squares the walls are grouped in, one mesh each.
const CHUNK: f64 = 256.0;
/// Walls are built out to this distance from the camera, and dropped a bit
/// beyond it.
const LOAD: f64 = 1400.0;
const UNLOAD: f64 = LOAD + 300.0;

type Key = (i64, i64);

fn chunk(p: DVec2) -> Key {
    ((p.x / CHUNK).floor() as i64, (p.y / CHUNK).floor() as i64)
}

fn chunk_box(k: Key) -> (DVec2, DVec2) {
    (
        DVec2::new(k.0 as f64, k.1 as f64) * CHUNK,
        DVec2::new(k.0 as f64 + 1.0, k.1 as f64 + 1.0) * CHUNK,
    )
}

/// The quay walls and steps around the camera, in square chunks.
pub struct WaterfrontView {
    world: Arc<TownWorld>,
    chunks: BTreeMap<Key, Vec<EntityId>>,
}

impl WaterfrontView {
    pub fn new(town: Arc<TownWorld>) -> Self {
        Self {
            world: town,
            chunks: BTreeMap::new(),
        }
    }

    /// Builds chunks coming into range (once the town has planned their
    /// ground; all at once if `wait` is false) and drops those out of it.
    pub fn update(&mut self, world: &mut World, eye: DVec2, wait: bool) {
        let gone: Vec<Key> = self
            .chunks
            .keys()
            .copied()
            .filter(|k| {
                let (lo, hi) = chunk_box(*k);
                ((lo + hi) * 0.5).distance(eye) > UNLOAD
            })
            .collect();
        for k in gone {
            for e in self.chunks.remove(&k).unwrap_or_default() {
                world.despawn(e);
            }
        }
        let (a, b) = (
            chunk(eye - DVec2::splat(LOAD)),
            chunk(eye + DVec2::splat(LOAD)),
        );
        for x in a.0..=b.0 {
            for z in a.1..=b.1 {
                let k = (x, z);
                let (lo, hi) = chunk_box(k);
                if self.chunks.contains_key(&k) || ((lo + hi) * 0.5).distance(eye) > LOAD {
                    continue;
                }
                let m = DVec2::splat(8.0);
                if wait && !self.world.quarters_ready(lo - m, hi + m) {
                    continue;
                }
                let ids = self.build(world, k);
                self.chunks.insert(k, ids);
            }
        }
    }

    fn build(&self, world: &mut World, k: Key) -> Vec<EntityId> {
        let (lo, hi) = chunk_box(k);
        let m = DVec2::splat(8.0);
        let quarters = self.world.quarters_in(lo - m, hi + m);
        let all_steps: Vec<WaterSteps> = quarters
            .iter()
            .flat_map(|q| q.water_steps.iter().copied())
            .collect();
        let mut b = MeshBuilder::new();
        for q in quarters.iter().flat_map(|q| q.quays.iter()) {
            if chunk(q.pts[q.pts.len() / 2]) == k {
                wall(&mut b, q, &all_steps);
            }
        }
        for s in all_steps.iter().filter(|s| chunk(s.pos) == k) {
            steps(&mut b, s);
        }
        let mut out = Vec::new();
        for mesh in b.finish_parts() {
            let id = world.spawn(mesh);
            world.set_name(id, format!("quay {} {}", k.0, k.1));
            out.push(id);
        }
        out
    }

    /// Takes every chunk out of the world (before switching towns).
    pub fn clear(&mut self, world: &mut World) {
        for (_, ids) in std::mem::take(&mut self.chunks) {
            for e in ids {
                world.despawn(e);
            }
        }
    }
}

fn v(p: DVec2, y: f64) -> DVec3 {
    DVec3::new(p.x, y, p.y)
}

/// The wall's face and its coping, with a gap where steps go down.
pub fn wall(b: &mut MeshBuilder, q: &QuayWall, steps: &[WaterSteps]) {
    let granite = rgb(150, 146, 140);
    let coping = rgb(196, 190, 178);
    let gap = |p: DVec2| {
        steps.iter().any(|s| {
            (p - s.pos).dot(s.out.perp()).abs() < STEPS_WIDTH * 0.5 && s.pos.distance(p) < 3.0
        })
    };
    for k in 0..q.pts.len() - 1 {
        let (o0, o1) = (q.out[k], q.out[k + 1]);
        // The face stands a hand's breadth out from the paving's edge.
        let (a, c) = (q.pts[k] + o0 * 0.08, q.pts[k + 1] + o1 * 0.08);
        let out = v((o0 + o1).normalize_or_zero(), 0.0);
        b.tex = Some(FacadeLayer::Granite.into());
        b.quad(
            [
                v(a, q.bottom[k]),
                v(c, q.bottom[k + 1]),
                v(c, q.top[k + 1] - 0.3),
                v(a, q.top[k] - 0.3),
            ],
            out,
            granite,
        );
        b.tex = None;
        if gap((q.pts[k] + q.pts[k + 1]) * 0.5) {
            continue;
        }
        // Coping: a stone kerb along the edge, a little proud of the paving.
        let (ia, ic) = (q.pts[k] - o0 * 0.3, q.pts[k + 1] - o1 * 0.3);
        let (oa, oc) = (q.pts[k] + o0 * 0.14, q.pts[k + 1] + o1 * 0.14);
        let (ta, tc) = (q.top[k] + 0.12, q.top[k + 1] + 0.12);
        let (ba, bc) = (q.top[k] - 0.3, q.top[k + 1] - 0.3);
        b.quad(
            [v(ia, ta), v(ic, tc), v(oc, tc), v(oa, ta)],
            DVec3::Y,
            coping,
        );
        b.quad([v(oa, ba), v(oc, bc), v(oc, tc), v(oa, ta)], out, coping);
        b.quad([v(ia, ba), v(ic, bc), v(ic, tc), v(ia, ta)], -out, coping);
        b.quad(
            [v(ia, ba), v(ic, bc), v(oc, bc), v(oa, ba)],
            -DVec3::Y,
            coping,
        );
        if k == 0 || k + 2 == q.pts.len() {
            let (i, o, t, bt) = if k == 0 {
                (ia, oa, ta, ba)
            } else {
                (ic, oc, tc, bc)
            };
            let along = v(q.pts[k + 1] - q.pts[k], 0.0) * if k == 0 { -1.0 } else { 1.0 };
            b.quad([v(i, bt), v(o, bt), v(o, t), v(i, t)], along, coping);
        }
    }
}

/// A flight of steps out from the wall to a landing just above the water.
pub fn steps(b: &mut MeshBuilder, s: &WaterSteps) {
    let stone = rgb(184, 180, 172);
    let landing = s.landing();
    let rise = 0.17;
    let n = ((s.top - landing) / rise).ceil().max(1.0) as usize;
    let rise = (s.top - landing) / n as f64;
    let tread = 0.32;
    let side = s.out.perp() * STEPS_WIDTH * 0.5;
    let foot = WATER_LEVEL - 1.2;
    let base = s.pos + s.out * 0.08;
    b.tex = Some(FacadeLayer::Limestone.into());
    for k in 0..n {
        // Step k's tread, top down.
        let y = s.top - rise * (k + 1) as f64;
        let (z0, z1) = (tread * k as f64, tread * (k + 1) as f64);
        let (p0, p1) = (base + s.out * z0, base + s.out * z1);
        let last = k + 1 == n;
        let p1 = if last { p1 + s.out * 1.4 } else { p1 };
        b.quad(
            [
                v(p0 - side, y),
                v(p1 - side, y),
                v(p1 + side, y),
                v(p0 + side, y),
            ],
            DVec3::Y,
            stone,
        );
        // Riser up to the step above.
        let up = y + rise;
        b.quad(
            [
                v(p0 - side, y),
                v(p0 + side, y),
                v(p0 + side, up),
                v(p0 - side, up),
            ],
            v(s.out, 0.0),
            stone,
        );
        // Sides of this column, down below the water.
        for sg in [-1.0, 1.0] {
            let sd = side * sg;
            b.quad(
                [
                    v(p0 + sd, foot),
                    v(p1 + sd, foot),
                    v(p1 + sd, y),
                    v(p0 + sd, y),
                ],
                v(sd, 0.0),
                stone,
            );
        }
        if last {
            b.quad(
                [
                    v(p1 - side, foot),
                    v(p1 + side, foot),
                    v(p1 + side, y),
                    v(p1 - side, y),
                ],
                v(s.out, 0.0),
                stone,
            );
        }
    }
    b.tex = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walls_and_steps_have_faces() {
        let q = QuayWall {
            pts: vec![DVec2::ZERO, DVec2::new(2.0, 0.0), DVec2::new(4.0, 0.0)],
            out: vec![DVec2::Y; 3],
            top: vec![1.06; 3],
            bottom: vec![-1.5; 3],
        };
        let s = WaterSteps {
            pos: DVec2::new(2.0, 0.0),
            out: DVec2::Y,
            top: 1.06,
        };
        let mut b = MeshBuilder::new();
        wall(&mut b, &q, &[]);
        steps(&mut b, &s);
        assert!(b.face_count() > 20);
    }
}
