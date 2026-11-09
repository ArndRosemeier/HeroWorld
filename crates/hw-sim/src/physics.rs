//! Server physics on Engine's headless [`PhysicsWorld`]: no window, no GPU.
//!
//! Loose things (crates now; debris, cars and ragdolls later) are Engine
//! physics bodies. They land on the same ground the heroes walk on and hit
//! the same buildings: the colliders are the boxes of [`Obstacles`], made
//! from the building plans every machine generates, and only around bodies
//! that are awake, so a whole city costs nothing.
//!
//! Heroes stay on their own movement code ([`crate::step_hero`]), which the
//! client replays to predict its hero exactly. In the physics world each hero
//! is a kinematic capsule that follows the hero and shoves bodies aside.

use crate::collision::{Obstacles, Solid, SolidSet, BODY_HEIGHT, BODY_RADIUS};
use crate::TICK_HZ;
use engine::glam::{Mat3, Mat4, Quat, Vec3};
use engine::physics::{
    Body as PhysicsBody, BodyId, BodyShape, PhysicsGround, PhysicsSettings, PhysicsWorld,
};
use glam::{DVec2, DVec3};
use hw_core::NetId;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Half the edge of a crate (m).
pub const CRATE_HALF: f64 = 0.5;
/// Building colliders are present within this distance of an awake body.
pub const STATIC_RANGE: f64 = 40.0;

/// A loose body's state: centre, orientation (x, y, z, w), velocity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PropState {
    pub pos: DVec3,
    pub rot: [f32; 4],
    pub vel: [f32; 3],
}

pub struct Physics {
    world: PhysicsWorld,
    solids: Arc<Obstacles>,
    /// Building colliders present, with the version of their solids.
    statics: BTreeMap<u64, (u64, Vec<BodyId>)>,
    heroes: BTreeMap<NetId, BodyId>,
    props: BTreeMap<NetId, BodyId>,
}

impl std::fmt::Debug for Physics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Physics")
            .field("statics", &self.statics.len())
            .field("heroes", &self.heroes.len())
            .field("props", &self.props.len())
            .finish()
    }
}

fn v3(p: DVec3) -> Vec3 {
    Vec3::new(p.x as f32, p.y as f32, p.z as f32)
}

/// A box of the city as a fixed physics body: centre, rotation, half size.
fn solid_box(s: &Solid) -> (Vec3, Quat, Vec3) {
    let mid = s.to_world((s.min + s.max) * 0.5);
    let half = (s.max - s.min) * 0.5;
    let x = Vec3::new(s.along.x as f32, 0.0, s.along.y as f32);
    let mut z = Vec3::new(s.inward.x as f32, 0.0, s.inward.y as f32);
    // The plan's frame may be mirrored; a box does not care which way z points.
    if x.cross(Vec3::Y).dot(z) < 0.0 {
        z = -z;
    }
    let rot = Quat::from_mat3(&Mat3::from_cols(x, Vec3::Y, z)).normalize();
    let center = Vec3::new(mid.x as f32, ((s.y0 + s.y1) * 0.5) as f32, mid.y as f32);
    let half = Vec3::new(half.x as f32, ((s.y1 - s.y0) * 0.5) as f32, half.y as f32)
        .max(Vec3::splat(0.01));
    (center, rot, half)
}

impl Physics {
    /// Physics on the ground of `solids`, one step per simulation tick.
    pub fn new(solids: Arc<Obstacles>) -> Self {
        let mut settings = PhysicsSettings {
            steps_per_second: TICK_HZ as f32,
            max_steps_per_frame: 1,
            ..Default::default()
        };
        let mut world = PhysicsWorld::new();
        if solids.has_relief() {
            let ground = solids.clone();
            world
                .set_height_fn(move |x, z| ground.ground_at(DVec2::new(x as f64, z as f64)) as f32);
        } else {
            settings.ground = PhysicsGround::Plane(solids.ground as f32);
        }
        world
            .set_settings(settings)
            .expect("valid physics settings");
        Self {
            world,
            solids,
            statics: BTreeMap::new(),
            heroes: BTreeMap::new(),
            props: BTreeMap::new(),
        }
    }

    /// Adds (or wakes) a crate.
    pub fn add_crate(&mut self, id: NetId, state: PropState) {
        self.remove_prop(id);
        let h = CRATE_HALF as f32;
        let body = PhysicsBody::dynamic()
            .shape(BodyShape::Box(Vec3::splat(h)))
            .density(0.6)
            .friction(0.7)
            .velocity(Vec3::from_array(state.vel));
        let rot = Quat::from_array(state.rot).normalize();
        let at = Mat4::from_rotation_translation(rot, v3(state.pos));
        if let Ok(b) = self.world.add(&body, at) {
            self.props.insert(id, b);
        }
    }

    /// Takes a body out of the simulation (it was removed, or froze outside
    /// every bubble).
    pub fn remove_prop(&mut self, id: NetId) {
        if let Some(b) = self.props.remove(&id) {
            self.world.remove(b);
        }
    }

    pub fn is_awake(&self, id: NetId) -> bool {
        self.props.contains_key(&id)
    }

    /// Puts a hero's capsule where the hero stands (feet at `feet`).
    pub fn place_hero(&mut self, id: NetId, feet: DVec3) {
        let center = v3(feet) + Vec3::Y * (BODY_HEIGHT as f32 * 0.5);
        let at = Mat4::from_translation(center);
        match self.heroes.get(&id) {
            Some(&b) => {
                let _ = self.world.set_transform(b, at);
            }
            None => {
                let body = PhysicsBody::kinematic().shape(BodyShape::Capsule {
                    half_height: (BODY_HEIGHT * 0.5 - BODY_RADIUS) as f32,
                    radius: BODY_RADIUS as f32,
                });
                if let Ok(b) = self.world.add(&body, at) {
                    self.heroes.insert(id, b);
                }
            }
        }
    }

    pub fn remove_hero(&mut self, id: NetId) {
        if let Some(b) = self.heroes.remove(&id) {
            self.world.remove(b);
        }
    }

    /// Heroes not in `keep` lose their capsules.
    pub fn keep_heroes(&mut self, keep: &BTreeSet<NetId>) {
        let gone: Vec<NetId> = self
            .heroes
            .keys()
            .filter(|id| !keep.contains(id))
            .copied()
            .collect();
        for id in gone {
            self.remove_hero(id);
        }
    }

    /// One tick: building colliders around awake bodies, then one step.
    pub fn step(&mut self) {
        self.stream_statics();
        self.world.step_once();
    }

    fn stream_statics(&mut self) {
        let mut wanted: BTreeMap<u64, SolidSet<'_>> = BTreeMap::new();
        for b in self.props.values() {
            let Some(p) = self.world.pose(*b).map(|(p, _)| p) else {
                continue;
            };
            for (i, solids) in self
                .solids
                .solids_near(DVec2::new(p.x as f64, p.z as f64), STATIC_RANGE)
            {
                wanted.insert(i, solids);
            }
        }
        // Gone out of range, or changed (a building took damage or was repaired).
        let gone: Vec<u64> = self
            .statics
            .iter()
            .filter(|(i, (v, _))| !wanted.contains_key(i) || *v != self.solids.version(**i))
            .map(|(i, _)| *i)
            .collect();
        for i in gone {
            for b in self.statics.remove(&i).map(|s| s.1).unwrap_or_default() {
                self.world.remove(b);
            }
        }
        for (i, solids) in wanted {
            if self.statics.contains_key(&i) {
                continue;
            }
            let mut bodies = Vec::with_capacity(solids.len());
            for s in solids.iter() {
                let (c, r, h) = solid_box(s);
                let body = PhysicsBody::fixed().shape(BodyShape::Box(h));
                if let Ok(b) = self.world.add(&body, Mat4::from_rotation_translation(r, c)) {
                    bodies.push(b);
                }
            }
            self.statics.insert(i, (self.solids.version(i), bodies));
        }
    }

    /// An awake body's state after the last step.
    pub fn prop(&self, id: NetId) -> Option<PropState> {
        let b = *self.props.get(&id)?;
        let (p, r) = self.world.pose(b)?;
        let v = self.world.velocity(b).unwrap_or(Vec3::ZERO);
        Some(PropState {
            pos: DVec3::new(p.x as f64, p.y as f64, p.z as f64),
            rot: r.to_array(),
            vel: v.to_array(),
        })
    }

    /// Building colliders present now (how much of the city is solid to physics).
    pub fn static_buildings(&self) -> usize {
        self.statics.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GROUND_Y;

    fn still(pos: DVec3) -> PropState {
        PropState {
            pos,
            rot: [0.0, 0.0, 0.0, 1.0],
            vel: [0.0; 3],
        }
    }

    #[test]
    fn a_crate_lands_on_the_ground() {
        let mut p = Physics::new(Arc::new(Obstacles::flat(GROUND_Y)));
        let id = NetId(1);
        p.add_crate(id, still(DVec3::new(0.0, 4.0, 0.0)));
        for _ in 0..TICK_HZ * 3 {
            p.step();
        }
        let s = p.prop(id).unwrap();
        assert!(
            (s.pos.y - (GROUND_Y + CRATE_HALF)).abs() < 0.05,
            "rests at {}",
            s.pos.y
        );
    }

    #[test]
    fn a_crate_lands_on_a_box_of_the_city() {
        let mut solids = Obstacles::flat(GROUND_Y);
        // A 4 m high block, turned 30 degrees.
        let (s, c) = (0.5f64, 0.75f64.sqrt());
        solids.add(Solid {
            origin: DVec2::new(-3.0, -3.0),
            along: DVec2::new(c, s),
            inward: DVec2::new(-s, c),
            min: DVec2::ZERO,
            max: DVec2::new(6.0, 6.0),
            y0: GROUND_Y,
            y1: 4.0,
        });
        let solids = Arc::new(solids);
        let top = DVec2::new(-3.0, -3.0) + DVec2::new(c, s) * 3.0 + DVec2::new(-s, c) * 3.0;
        let mut p = Physics::new(solids);
        let id = NetId(1);
        p.add_crate(id, still(DVec3::new(top.x, 8.0, top.y)));
        for _ in 0..TICK_HZ * 3 {
            p.step();
        }
        assert_eq!(p.static_buildings(), 1);
        let st = p.prop(id).unwrap();
        assert!(
            (st.pos.y - (4.0 + CRATE_HALF)).abs() < 0.05,
            "rests on the block at {}",
            st.pos.y
        );
    }

    #[test]
    fn a_hero_shoves_a_crate() {
        let mut p = Physics::new(Arc::new(Obstacles::flat(GROUND_Y)));
        let id = NetId(1);
        let start = DVec3::new(2.0, GROUND_Y + CRATE_HALF, 0.0);
        p.add_crate(id, still(start));
        let hero = NetId(2);
        // The hero walks east through where the crate stands.
        for t in 0..TICK_HZ * 2 {
            let x = -1.0 + t as f64 * 5.0 / TICK_HZ as f64;
            p.place_hero(hero, DVec3::new(x, GROUND_Y, 0.0));
            p.step();
        }
        let moved = p.prop(id).unwrap().pos.x - start.x;
        assert!(moved > 2.0, "the crate moved only {moved} m");
    }
}
