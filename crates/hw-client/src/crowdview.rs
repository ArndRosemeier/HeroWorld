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

struct Shown {
    look: u32,
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
        }
    }

    /// The citizen shown nearest `eye`, where they stand (for the
    /// `HW_VIEW=citizen` close-up).
    pub fn nearest(&self, world: &World, eye: Vec3) -> Option<Place> {
        self.shown
            .values()
            .filter_map(|s| {
                let crowd = *self.crowds.get(&s.look)?;
                world.crowd_agent_place(crowd, s.agent).ok()
            })
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
            if let Some(&crowd) = self.crowds.get(&s.look) {
                world.remove_crowd_agent(crowd, s.agent);
            }
        }
        self.near.clear();
        self.delay.clear();
        self.running.clear();
        self.down.clear();
        self.refresh = 0.0;
    }

    /// A finished citizen look becomes a crowd.
    pub fn add_look(&mut self, world: &mut World, look: u32, built: &Built) -> EngineResult<()> {
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
        Ok(())
    }

    /// Moves everyone to `time`; `camera` is where they are seen from.
    /// Near trouble (`alarms`) people run off along their way, or back the
    /// way they came, whichever is away from it; a little further off they
    /// stop and stare.
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
        for id in self.near.clone() {
            let delay = self.delay.get(&id).copied().unwrap_or(0.0);
            let Some(mut c): Option<Citizen> = citizens.citizen(id, time - delay) else {
                continue;
            };
            if !self.down.contains_key(&id) {
                if let Some(r) = self.react(citizens, alarms, &c, time - delay, dt) {
                    c = r;
                }
            }
            let Some(&crowd) = self.crowds.get(&c.look) else {
                continue;
            };
            seen.insert(c.id);
            self.placed.push(c.pos);
            if self.down.contains_key(&c.id) {
                *self.delay.entry(c.id).or_insert(0.0) += dt;
                if let Some(s) = self.shown.get(&c.id) {
                    let p = world.crowd_agent_place(crowd, s.agent)?;
                    world.set_crowd_agent(crowd, s.agent, p, 0.0)?;
                }
                continue;
            }
            let yaw = c.facing.x.atan2(c.facing.y).to_degrees() as f32;
            let place =
                Place::new(c.pos.x as f32, c.pos.y as f32, c.pos.z as f32).with_yaw_deg(yaw);
            match self.shown.get(&c.id) {
                Some(s) => world.set_crowd_agent(crowd, s.agent, place, c.speed as f32)?,
                None => {
                    let agent = world.add_crowd_agent(crowd, place)?;
                    world.set_crowd_agent(crowd, agent, place, c.speed as f32)?;
                    self.shown.insert(
                        c.id,
                        Shown {
                            look: c.look,
                            agent,
                        },
                    );
                }
            }
        }
        let gone: Vec<u64> = self
            .shown
            .keys()
            .filter(|id| !seen.contains(id))
            .copied()
            .collect();
        for id in gone {
            if let Some(s) = self.shown.remove(&id) {
                if let Some(&crowd) = self.crowds.get(&s.look) {
                    world.remove_crowd_agent(crowd, s.agent);
                }
            }
            self.down.remove(&id);
        }
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
            let Some(&crowd) = self.crowds.get(&s.look) else {
                continue;
            };
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
