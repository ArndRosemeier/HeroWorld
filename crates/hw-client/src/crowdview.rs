//! The citizens on screen: the ones near the camera (worked out from the
//! town seed and the game time, see `hw_sim::citizens`) shown with Engine
//! crowds, one crowd per look. The nearest become full characters, which
//! can be knocked over.

use crate::people::Built;
use engine::prelude::*;
use glam::DVec2;
use hw_sim::alarms::Alarms;
use hw_sim::citizens::{Citizen, Citizens};
use std::collections::{BTreeMap, BTreeSet};

/// Citizens are shown within this distance of the camera...
pub const SHOW_RADIUS: f64 = 260.0;
/// ...and at most this many at once (the nearest).
pub const MAX_SHOWN: usize = 600;
/// Running from trouble: how fast a citizen's clock goes (forwards, or
/// backwards to run back the way it came).
const RUN: f64 = 2.6;
/// Full characters (the rest are drawn instanced).
const NEAR_M: f32 = 22.0;
const NEAR_MAX: usize = 20;

/// A citizen on screen: the agent it is, in the crowd of the look it had
/// when it was added. The crowd is kept with the agent, so the agent is
/// always used with the crowd that holds it.
struct Shown {
    look: u32,
    crowd: CrowdId,
    agent: CrowdAgentId,
}

pub struct CrowdView {
    crowds: BTreeMap<u32, CrowdId>,
    /// Looks that fit in a car seat (`Built::sits`).
    sits: BTreeMap<u32, bool>,
    shown: BTreeMap<u64, Shown>,
    /// Seconds a citizen is behind the town's clock (it spent them lying
    /// on the ground after a knock).
    delay: BTreeMap<u64, f64>,
    /// Citizens knocked over and not up yet, with their character.
    down: BTreeMap<u64, EntityId>,
    /// Running from trouble: how fast each one's clock goes (see [`RUN`]).
    running: BTreeMap<u64, f64>,
    /// Who is near enough to show (looked up twice a second; their places
    /// are worked out every frame).
    near: Vec<u64>,
    refresh: f64,
    /// Where the citizens shown stood this frame.
    placed: Vec<glam::DVec3>,
    /// People indoors to show next update (see [`Self::set_indoor`]).
    indoor: Vec<Citizen>,
}

impl Default for CrowdView {
    fn default() -> Self {
        Self::new()
    }
}

impl CrowdView {
    pub fn new() -> Self {
        Self {
            crowds: BTreeMap::new(),
            sits: BTreeMap::new(),
            shown: BTreeMap::new(),
            delay: BTreeMap::new(),
            down: BTreeMap::new(),
            running: BTreeMap::new(),
            near: Vec::new(),
            refresh: 0.0,
            placed: Vec::new(),
            indoor: Vec::new(),
        }
    }

    /// The citizen shown nearest `eye`, where they stand (for the
    /// `HW_VIEW=citizen` close-up).
    pub fn nearest(&self, world: &World, eye: Vec3) -> Option<Place> {
        self.shown
            .values()
            .filter_map(|s| world.crowd_agent_place(s.crowd, s.agent).ok())
            .min_by(|a, b| {
                a.position
                    .distance(eye)
                    .total_cmp(&b.position.distance(eye))
            })
    }

    /// The crowd of `look`, once it is ready (people in cars are agents
    /// of it too, see `riderview`).
    pub fn crowd_of(&self, look: u32) -> Option<CrowdId> {
        self.crowds.get(&look).copied()
    }

    /// Whether look `look` fits in a car seat; `None` until it is ready.
    pub fn sits(&self, look: u32) -> Option<bool> {
        self.sits.get(&look).copied()
    }

    /// How many looks are ready.
    pub fn looks(&self) -> usize {
        self.crowds.len()
    }

    pub fn shown(&self) -> usize {
        self.shown.len()
    }

    /// How many citizens shown this frame stand within `r` of `p` (the
    /// soundscape's crowd).
    pub fn within(&self, p: Vec3, r: f64) -> usize {
        let p = p.as_dvec3();
        self.placed
            .iter()
            .filter(|c| (DVec2::new(c.x - p.x, c.z - p.z)).length() <= r)
            .count()
    }

    /// Takes everyone off the streets (another town comes).
    pub fn clear(&mut self, world: &mut World) {
        self.placed.clear();
        for (_, s) in std::mem::take(&mut self.shown) {
            world.remove_crowd_agent(s.crowd, s.agent);
        }
        self.near.clear();
        self.delay.clear();
        self.running.clear();
        self.down.clear();
        self.refresh = 0.0;
    }

    /// A finished citizen look becomes a crowd. A look has one crowd for
    /// good: the people at bus stops, in cars and on the streets hold agents
    /// of it by its id, so a look built a second time (asked for again)
    /// keeps the crowd it has. Returns whether a crowd was made.
    pub fn add_look(&mut self, world: &mut World, look: u32, built: &Built) -> EngineResult<bool> {
        if self.crowds.contains_key(&look) {
            return Ok(false);
        }
        let profile = CrowdProfile::new("idle", built.walk.clone())
            .walk_speed(crate::people::WALK_MPS)
            .near(NEAR_M, NEAR_MAX / 4)
            .far(SHOW_RADIUS as f32)
            .shadows_within(70.0)
            .far_detail(0.06)
            // Drivers and passengers (see `riderview`).
            .poses([crate::riderview::DRIVE, crate::riderview::RIDE]);
        let id = world.add_crowd(built.model.clone(), profile)?;
        self.crowds.insert(look, id);
        self.sits.insert(look, built.sits);
        Ok(true)
    }

    /// Moves everyone to `time`; `camera` is where they are seen from.
    /// Near trouble (`alarms`) people run off along their way, or back the
    /// way they came, whichever is away from it; a little further off they
    /// stop and stare.
    /// The people indoors near the camera, for the next [`Self::update`]
    /// (from `Citizens::indoors`).
    pub fn set_indoor(&mut self, people: Vec<Citizen>) {
        self.indoor = people;
    }

    pub fn update(
        &mut self,
        world: &mut World,
        citizens: &Citizens,
        alarms: &Alarms,
        camera: DVec2,
        time: f64,
        dt: f64,
    ) -> EngineResult<()> {
        if self.crowds.is_empty() {
            return Ok(());
        }
        // The knocked down lie still; their clock waits until they're up.
        let mut up = Vec::new();
        for (id, e) in &self.down {
            if world.ragdoll_state(*e).is_none() {
                up.push(*id);
            }
        }
        for id in up {
            self.down.remove(&id);
        }
        self.refresh -= dt;
        if self.refresh <= 0.0 {
            self.refresh = 0.5;
            // Robot sweepers are drawn by `sweeperview`.
            self.near = citizens
                .near(camera, SHOW_RADIUS, time, MAX_SHOWN)
                .iter()
                .map(|c| c.id)
                .filter(|id| !citizens.is_sweeper(*id))
                .collect();
        }
        let mut seen = BTreeSet::new();
        self.placed.clear();
        // People indoors stand at their places (see `Citizens::indoors`).
        let inside: BTreeMap<u64, Citizen> = std::mem::take(&mut self.indoor)
            .into_iter()
            .map(|c| (c.id, c))
            .collect();
        for id in self.near.clone().into_iter().chain(inside.keys().copied()) {
            let delay = self.delay.get(&id).copied().unwrap_or(0.0);
            let found = match inside.get(&id) {
                Some(c) => Some(*c),
                None => citizens.citizen(id, time - delay),
            };
            let Some(mut c): Option<Citizen> = found else {
                continue;
            };
            if !self.down.contains_key(&id) {
                if let Some(r) = self.react(citizens, alarms, &c, time - delay, dt) {
                    c = r;
                }
            }
            if !self.crowds.contains_key(&c.look) {
                continue;
            }
            seen.insert(c.id);
            self.placed.push(c.pos);
            if self.down.contains_key(&c.id) {
                *self.delay.entry(c.id).or_insert(0.0) += dt;
                if let Some(s) = self.shown.get(&c.id) {
                    let p = world.crowd_agent_place(s.crowd, s.agent)?;
                    world.set_crowd_agent(s.crowd, s.agent, p, 0.0)?;
                }
                continue;
            }
            self.put(world, &c)?;
        }
        let gone: Vec<u64> = self
            .shown
            .keys()
            .filter(|id| !seen.contains(id))
            .copied()
            .collect();
        for id in gone {
            if let Some(s) = self.shown.remove(&id) {
                world.remove_crowd_agent(s.crowd, s.agent);
            }
            self.down.remove(&id);
        }
        Ok(())
    }

    /// Shows citizen `c` where it is now. Someone can come back under the
    /// same id with another look (people indoors at a shop or office are a
    /// new visitor every visit, see `Citizens::indoors`): they leave the
    /// old look's crowd and join the new one's, so an agent is never used
    /// with a crowd that does not hold it.
    fn put(&mut self, world: &mut World, c: &Citizen) -> EngineResult<()> {
        let Some(&crowd) = self.crowds.get(&c.look) else {
            return Ok(());
        };
        let yaw = c.facing.x.atan2(c.facing.y).to_degrees() as f32;
        let place = Place::new(c.pos.x as f32, c.pos.y as f32, c.pos.z as f32).with_yaw_deg(yaw);
        if let Some(s) = self.shown.get(&c.id) {
            if s.look == c.look {
                return world.set_crowd_agent(s.crowd, s.agent, place, c.speed as f32);
            }
        }
        if let Some(old) = self.shown.remove(&c.id) {
            world.remove_crowd_agent(old.crowd, old.agent);
        }
        let agent = world.add_crowd_agent(crowd, place)?;
        world.set_crowd_agent(crowd, agent, place, c.speed as f32)?;
        self.shown.insert(
            c.id,
            Shown {
                look: c.look,
                crowd,
                agent,
            },
        );
        Ok(())
    }

    /// How citizen `c` (at its own clock `t`) reacts to the nearest
    /// trouble, if any: running (its clock sped up, or run backwards) or
    /// standing to stare. Moves its clock on.
    fn react(
        &mut self,
        citizens: &Citizens,
        alarms: &Alarms,
        c: &Citizen,
        t: f64,
        dt: f64,
    ) -> Option<Citizen> {
        let p = DVec2::new(c.pos.x, c.pos.z);
        let near = alarms
            .active()
            .filter(|a| (c.pos.y - a.at.y).abs() < 30.0)
            .map(|a| (a.flat().distance(p) / a.radius.max(1.0), a))
            .min_by(|x, y| x.0.total_cmp(&y.0));
        let Some((ratio, a)) = near.filter(|n| n.0 < 2.0) else {
            self.running.remove(&c.id);
            return None;
        };
        let away = (p - a.flat()).normalize_or(DVec2::Y);
        let delay = self.delay.entry(c.id).or_insert(0.0);
        if ratio < 1.0 && a.kind.frightens() {
            // Run: along its way if that leads away, else back.
            let rate = *self.running.entry(c.id).or_insert_with(|| {
                let next = citizens
                    .citizen(c.id, t + 1.0)
                    .map_or(p, |n| DVec2::new(n.pos.x, n.pos.z));
                if (next - a.flat()).length() >= (p - a.flat()).length() {
                    RUN
                } else {
                    -RUN
                }
            });
            *delay += dt * (1.0 - rate);
            let mut r = citizens.citizen(c.id, t + dt * rate)?;
            if r.speed > 0.0 {
                r.speed *= rate.abs();
                if rate < 0.0 {
                    r.facing = -r.facing;
                }
            } else {
                // Held up (at a kerb, or one who stands about): turned away.
                r.facing = away;
            }
            return Some(r);
        }
        // Stop and stare.
        *delay += dt;
        Some(Citizen {
            facing: -away,
            speed: 0.0,
            ..*c
        })
    }

    /// Knocks over every full-character citizen within `radius` of
    /// `center`, thrown away from it. Returns how many fell.
    pub fn knock_down(&mut self, world: &mut World, center: Vec3, radius: f32) -> usize {
        let mut n = 0;
        for (id, s) in &self.shown {
            let crowd = s.crowd;
            let Some(e) = world.crowd_agent_character(crowd, s.agent) else {
                continue;
            };
            let Ok(p) = world.crowd_agent_place(crowd, s.agent) else {
                continue;
            };
            let to = p.position + Vec3::Y * 0.9 - center;
            let d = to.length();
            if d > radius {
                continue;
            }
            let fall = 1.0 - d / radius;
            let dir = (to.normalize_or_zero() + Vec3::Y * 0.6).normalize_or(Vec3::Y);
            let knock = engine::ragdoll::Knock::new(Vec3::ZERO).velocity(dir * (4.0 + 6.0 * fall));
            if world.knock_down(e, knock).is_ok() {
                self.down.insert(*id, e);
                n += 1;
            }
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::people::Who;
    use engine::anim::{AnimatedModel, AnimationClip, JointChannels, Skeleton, SkinMesh};
    use glam::{DVec3, Mat4, Quat};
    use std::sync::Arc;

    /// A one-bone person with the clips a citizen's crowd asks for.
    fn built(look: u32) -> Built {
        let clip = |name: &str| AnimationClip {
            name: name.into(),
            duration: 1.0,
            channels: vec![JointChannels::default()],
        };
        let model = AnimatedModel {
            skeleton: Skeleton {
                joint_names: vec!["root".into()],
                parents: vec![None],
                inverse_bind: vec![Mat4::IDENTITY],
                joint_node_indices: vec![0],
                root_transforms: vec![Mat4::IDENTITY],
            },
            meshes: vec![SkinMesh {
                positions: vec![Vec3::ZERO, Vec3::X, Vec3::Y],
                normals: vec![Vec3::Z; 3],
                colors: vec![glam::Vec4::ONE; 3],
                uvs: vec![[0.0; 2]; 3],
                joints: vec![[0; 4]; 3],
                weights: vec![[1.0, 0.0, 0.0, 0.0]; 3],
                indices: vec![0, 1, 2],
                double_sided: false,
                albedo: None,
                maps: Default::default(),
                morphs: vec![],
            }],
            clips: [
                "idle",
                "walk",
                crate::riderview::DRIVE,
                crate::riderview::RIDE,
            ]
            .map(clip)
            .into(),
            bind_local: vec![(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE)],
            morph_targets: vec![],
        };
        Built {
            who: Who::Citizen(look),
            model: Arc::new(model),
            walk: "walk".into(),
            sits: true,
        }
    }

    fn citizen(id: u64, look: u32) -> Citizen {
        Citizen {
            id,
            pos: DVec3::new(id as f64, 0.0, 0.0),
            facing: DVec2::Y,
            speed: 1.0,
            look,
        }
    }

    /// "no crowd agent 0" (0.171): the street acts asked for a citizen look
    /// again, and the second build replaced the look's crowd with an empty
    /// one while the people at bus stops and in cars still held agents of
    /// the first.
    #[test]
    fn a_look_built_twice_keeps_its_crowd_and_its_agents() {
        let mut world = World::new();
        let mut crowd = CrowdView::new();
        assert!(crowd.add_look(&mut world, 3, &built(3)).unwrap());
        let id = crowd.crowd_of(3).unwrap();
        // Someone waiting at a bus stop (busview holds the agent).
        let waiting = world
            .add_crowd_agent(id, Place::new(1.0, 0.0, 0.0))
            .unwrap();
        // The look comes again.
        assert!(!crowd.add_look(&mut world, 3, &built(3)).unwrap());
        assert_eq!(crowd.crowd_of(3), Some(id));
        let again = crowd.crowd_of(3).unwrap();
        world
            .set_crowd_agent(again, waiting, Place::new(2.0, 0.0, 0.0), 1.0)
            .expect("the waiting one is still in its look's crowd");
    }

    /// A shop's visitor comes back under the same id with another look
    /// (`Citizens::indoors`): it moves to the new look's crowd.
    #[test]
    fn a_citizen_whose_look_changes_moves_to_that_looks_crowd() {
        let mut world = World::new();
        let mut crowd = CrowdView::new();
        for look in [1, 2] {
            crowd.add_look(&mut world, look, &built(look)).unwrap();
        }
        let (one, two) = (crowd.crowd_of(1).unwrap(), crowd.crowd_of(2).unwrap());
        // Look 1's crowd has more people than look 2's, so the visitor's
        // agent number does not exist in look 2's crowd.
        crowd.put(&mut world, &citizen(10, 1)).unwrap();
        crowd.put(&mut world, &citizen(11, 1)).unwrap();
        crowd.put(&mut world, &citizen(11, 2)).expect("a new look");
        crowd
            .put(&mut world, &citizen(11, 2))
            .expect("and on with it");
        assert_eq!(world.crowd_agent_count(one), 1);
        assert_eq!(world.crowd_agent_count(two), 1);
        crowd.clear(&mut world);
        assert_eq!(world.crowd_agent_count(one), 0);
        assert_eq!(world.crowd_agent_count(two), 0);
    }
}
