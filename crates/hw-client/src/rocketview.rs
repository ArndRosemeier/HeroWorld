//! The rocket port, alive. Its launches are the server's schedule
//! ([`hw_sim::rocket`], a pure function of the game clock), so every player
//! sees the same one: white vapour streams off the hull and the klaxon
//! sounds; the arms swing away; the board on the gantry counts the last
//! ten seconds; the engine lights, fire and smoke roll out of the flame
//! duct; the rocket climbs out of sight on a column of fire and smoke.
//! Minutes later it comes back down on its tail, braking on its flame,
//! settles onto the stand in a ring of dust, steams as it cools, and the
//! arms swing back in. Between launches the board counts the minutes.
//!
//! The rocket and its arms are LOD sets moved each frame; the flame and the
//! board's segments are instanced entities shown by giving them places
//! (tint for brightness), so nothing is built while it flies. The meshes
//! are made on a worker thread when the town starts. The sound is the
//! soundscape's ([`Heard`]).

use crate::carmesh::Lod;
use crate::rocketmesh::{arm, flame, lit, rocket, segment, FLAME, SEGMENTS};
use engine::prelude::*;
use glam::{DVec2, Vec3};
use hw_gen::urban::gantry::{
    ARM_HEIGHTS, ARM_HINGE, BOARD_Y, DUCT_X, STAND_H, TOWER_H, TOWER_HALF, TOWER_X,
};
use hw_gen::urban::world::TownWorld;
use hw_sim::rocket::{arm_angle, pads, Now, Pad, Phase};
use std::sync::mpsc::{channel, Receiver, TryRecvError};

/// Smoke, fire and vapour play only this near (m).
const ACTIVE: f32 = 3000.0;
/// The board's two digits: their middles across the board (m).
const DIGIT_X: [f64; 2] = [-0.95, 0.95];

/// A sound the rocket port makes once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shot {
    /// The klaxon on the blockhouse: venting begins.
    Klaxon,
    /// A tick of the count (seconds left).
    Beep(u32),
    /// The engine lights.
    Ignition,
    /// The rocket settles onto the stand.
    Touchdown,
}

/// What the soundscape hears of the rocket port this frame.
#[derive(Clone, Debug, Default)]
pub struct Heard {
    /// The engine: where, and how hard it burns (0..1).
    pub roar: Option<(Vec3, f32)>,
    /// Vapour hissing off the hull.
    pub vent: Option<Vec3>,
    /// One-shots since the last frame.
    pub shots: Vec<(Shot, Vec3)>,
}

/// The meshes, made off the main thread.
struct Built {
    rocket: Vec<LodLevel>,
    arms: [Vec<LodLevel>; 2],
    flame: Mesh,
    segments: Vec<Mesh>,
}

fn build() -> Built {
    let rocket = vec![
        LodLevel::new(rocket(Lod::Near), 160.0),
        LodLevel::new(rocket(Lod::Mid), 900.0),
        LodLevel::new(rocket(Lod::Far), 6000.0),
    ];
    let arms = [0, 1].map(|k| {
        vec![
            LodLevel::new(arm(k, Lod::Near), 140.0),
            LodLevel::new(arm(k, Lod::Mid), 700.0),
            LodLevel::new(arm(k, Lod::Far), 2500.0),
        ]
    });
    Built {
        rocket,
        arms,
        flame: flame(Lod::Near),
        segments: (0..7).map(segment).collect(),
    }
}

struct Emitters {
    /// White vapour off the hull's vents.
    vapour: Option<EmitterId>,
    /// Fire and smoke rolling out of the flame duct.
    duct_fire: Option<EmitterId>,
    duct_smoke: Option<EmitterId>,
    /// Fire off the nozzle, and the smoke trail behind the rocket.
    plume: Option<EmitterId>,
    trail: Option<EmitterId>,
}

struct Live {
    rocket: LodId,
    arms: [LodId; 2],
    flame: EntityId,
    segments: Vec<EntityId>,
    engine: LampId,
    beacon: LampId,
    fx: Emitters,
}

pub struct RocketView {
    pad: Option<Pad>,
    job: Option<Receiver<Built>>,
    live: Option<Live>,
    last: Option<Now>,
    heard: Heard,
    clock: f32,
    rng: u64,
    /// `HW_ROCKET_T`: the first launch frozen this many seconds into its
    /// cycle (for screenshots).
    fixed: Option<f64>,
}

fn emitter_on(
    world: &mut World,
    slot: &mut Option<EmitterId>,
    want: bool,
    at: Vec3,
    make: impl FnOnce() -> ParticleEmitter,
) -> EngineResult<()> {
    match (*slot, want) {
        (Some(id), true) => world.particles_mut().set_emitter_position(id, at)?,
        (None, true) => *slot = Some(world.particles_mut().start(make())?),
        (Some(id), false) => {
            world.particles_mut().stop(id)?;
            *slot = None;
        }
        (None, false) => {}
    }
    Ok(())
}

/// A linear colour (values 0..1, alpha as given).
fn lin(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color { r, g, b, a }
}

impl RocketView {
    pub fn new(town: &TownWorld) -> Self {
        let pad = pads(town).into_iter().next();
        let job = pad.as_ref().map(|_| {
            let (tx, rx) = channel();
            std::thread::Builder::new()
                .name("rocket meshes".into())
                .spawn(move || {
                    let _ = tx.send(build());
                })
                .expect("a thread for the rocket's meshes");
            rx
        });
        Self {
            pad,
            job,
            live: None,
            last: None,
            heard: Heard::default(),
            clock: 0.0,
            rng: 0x2b99_2ddf_a232_49d6,
            fixed: std::env::var("HW_ROCKET_T")
                .ok()
                .and_then(|v| v.parse().ok()),
        }
    }

    /// The pad, if the town has one (for views and the debug menu).
    pub fn pad(&self) -> Option<&Pad> {
        self.pad.as_ref()
    }

    /// What the soundscape should hear this frame (one-shots handed over
    /// once).
    pub fn heard(&mut self) -> Heard {
        let shots = std::mem::take(&mut self.heard.shots);
        Heard {
            shots,
            ..self.heard.clone()
        }
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    fn spawn(world: &mut World, b: Built) -> EngineResult<Live> {
        let rocket = world.spawn_lod_at(b.rocket, &[])?;
        let [a0, a1] = b.arms;
        let arms = [world.spawn_lod_at(a0, &[])?, world.spawn_lod_at(a1, &[])?];
        let make = |world: &mut World, mesh: Mesh| -> EngineResult<EntityId> {
            let id = world.spawn_instanced(mesh);
            world.set_casts_shadow(id, false)?;
            Ok(id)
        };
        let flame = make(world, b.flame)?;
        let segments = b
            .segments
            .into_iter()
            .map(|m| make(world, m))
            .collect::<EngineResult<Vec<_>>>()?;
        let fire = rgb(FLAME.0, FLAME.1, FLAME.2);
        let engine = world.add_lamp(Lamp::point(Vec3::ZERO, fire, 0.0, 150.0))?;
        let beacon = world.add_lamp(Lamp::point(Vec3::ZERO, rgb(255, 40, 30), 0.0, 30.0))?;
        Ok(Live {
            rocket,
            arms,
            flame,
            segments,
            engine,
            beacon,
            fx: Emitters {
                vapour: None,
                duct_fire: None,
                duct_smoke: None,
                plume: None,
                trail: None,
            },
        })
    }

    pub fn clear(&mut self, world: &mut World) {
        if let Some(live) = self.live.take() {
            world.despawn_lod(live.rocket);
            for a in live.arms {
                world.despawn_lod(a);
            }
            world.despawn(live.flame);
            for s in live.segments {
                world.despawn(s);
            }
            world.remove_lamp(live.engine);
            world.remove_lamp(live.beacon);
            for id in [
                live.fx.vapour,
                live.fx.duct_fire,
                live.fx.duct_smoke,
                live.fx.plume,
                live.fx.trail,
            ]
            .into_iter()
            .flatten()
            {
                let _ = world.particles_mut().stop(id);
            }
        }
        self.heard = Heard::default();
        self.last = None;
    }

    /// Is the rocket in the world (for the memory checks and tests)?
    pub fn is_live(&self) -> bool {
        self.live.is_some()
    }

    /// Moves everything to game second `t`.
    pub fn update(&mut self, world: &mut World, t: f64, dt: f32) -> EngineResult<()> {
        let Some(pad) = self.pad.clone() else {
            return Ok(());
        };
        if let Some(rx) = &self.job {
            match rx.try_recv() {
                Ok(b) => {
                    self.job = None;
                    self.live = Some(Self::spawn(world, b)?);
                }
                Err(TryRecvError::Empty) => return Ok(()),
                Err(TryRecvError::Disconnected) => {
                    self.job = None;
                    return Err(EngineError::InvalidValue(
                        "the rocket's mesh worker stopped without its meshes".into(),
                    ));
                }
            }
        }
        let Some(mut live) = self.live.take() else {
            return Ok(());
        };
        self.clock += dt.clamp(0.0, 0.1);
        let t = match self.fixed {
            Some(s) => pad.schedule.first + s,
            None => t,
        };
        let now = pad.schedule.at(t);
        let done = self.draw(world, &mut live, &pad, now);
        self.live = Some(live);
        self.last = Some(now);
        done
    }

    fn draw(
        &mut self,
        world: &mut World,
        live: &mut Live,
        pad: &Pad,
        now: Now,
    ) -> EngineResult<()> {
        let l = &pad.landmark;
        let yaw = l.dir.x.atan2(l.dir.y).to_degrees() as f32;
        let at = |local: DVec2, y: f64| pad.world_at(local, y).as_vec3();
        let foot = at(DVec2::ZERO, STAND_H + now.height);
        let eye = world.camera().eye();
        let near = at(DVec2::ZERO, 0.0).distance(eye) < ACTIVE
            || (now.shown && foot.distance(eye) < ACTIVE);
        // The rocket.
        let places: Vec<Place> = if now.shown {
            vec![Place::new(foot.x, foot.y, foot.z).with_yaw_deg(yaw)]
        } else {
            Vec::new()
        };
        world.set_lod_places(live.rocket, &places)?;
        // The arms, swung about their hinges.
        let swing = arm_angle(now.arms).to_degrees() as f32;
        for (k, id) in live.arms.iter().enumerate() {
            let h = at(ARM_HINGE, ARM_HEIGHTS[k]);
            world.set_lod_places(*id, &[Place::new(h.x, h.y, h.z).with_yaw_deg(yaw + swing)])?;
        }
        // The flame, flickering and as long as the thrust.
        let flicker = 0.9 + 0.2 * self.rand();
        let burning = now.shown && now.thrust > 0.01;
        let nozzle = at(DVec2::ZERO, STAND_H + now.height + 0.8);
        let flame: Vec<Place> = if burning {
            let k = (0.3 + 0.7 * now.thrust as f32) * flicker;
            let g = 1.1 + 0.5 * self.rand();
            vec![Place::new(nozzle.x, nozzle.y, nozzle.z)
                .with_yaw_deg(yaw + 360.0 * self.rand())
                .with_stretch(Vec3::new(0.9 + 0.2 * k, k, 0.9 + 0.2 * k))
                .with_tint(Color::rgb01_unchecked(g, g, g))]
        } else {
            Vec::new()
        };
        world.set_instances(live.flame, &flame)?;
        self.board(world, live, pad, now, yaw)?;
        // Lights: the engine's glare, the beacon blinking on the crane.
        let fire = rgb(FLAME.0, FLAME.1, FLAME.2);
        let glare = if burning {
            (900.0 + 500.0 * self.rand()) * now.thrust as f32
        } else {
            0.0
        };
        let below = nozzle - Vec3::Y * 4.0;
        world.set_lamp(live.engine, Lamp::point(below, fire, glare, 160.0))?;
        let blink = if (self.clock * 1.1).fract() < 0.18 {
            60.0
        } else {
            0.0
        };
        let tip = at(DVec2::new(TOWER_X, 0.0), TOWER_H + 8.8);
        world.set_lamp(live.beacon, Lamp::point(tip, rgb(255, 40, 30), blink, 30.0))?;
        self.effects(world, &mut live.fx, pad, now, near, nozzle)?;
        self.sounds(pad, now, nozzle, burning);
        Ok(())
    }

    /// The board: minutes to the launch while there are many (amber), the
    /// last 99 seconds in red, 00 flashing while it burns, -- after a
    /// scrub or while it is away.
    fn board(
        &mut self,
        world: &mut World,
        live: &Live,
        pad: &Pad,
        now: Now,
        yaw: f32,
    ) -> EngineResult<()> {
        let flash = (self.clock * 2.0).fract() < 0.6;
        let amber = Color::rgb01_unchecked(1.6, 0.85, 0.2);
        let red = Color::rgb01_unchecked(2.0, 0.25, 0.15);
        let dash = [false, false, false, false, false, false, true];
        let (digits, tint): (Option<[[bool; 7]; 2]>, Color) = match now.phase {
            Phase::Ignition | Phase::Ascent => (flash.then(|| [lit(0), lit(0)]), red),
            Phase::Away | Phase::Descent => (Some([dash, dash]), amber),
            Phase::Scrubbed => (flash.then_some([dash, dash]), red),
            _ => {
                let secs = now.to_launch.ceil() as u32;
                if secs >= 100 {
                    let m = secs.div_ceil(60).min(99);
                    (Some([lit(m / 10), lit(m % 10)]), amber)
                } else {
                    (Some([lit(secs / 10), lit(secs % 10)]), red)
                }
            }
        };
        let face = TOWER_HALF + 0.3 + 0.17;
        for (k, id) in live.segments.iter().enumerate() {
            let mut places = Vec::new();
            if let Some(d) = digits {
                for (i, x) in DIGIT_X.iter().enumerate() {
                    if d[i][k] {
                        let (sx, sy, _, _) = SEGMENTS[k];
                        let p = pad.world_at(DVec2::new(TOWER_X + x + sx, face), BOARD_Y + sy);
                        let p = p.as_vec3();
                        places.push(Place::new(p.x, p.y, p.z).with_yaw_deg(yaw).with_tint(tint));
                    }
                }
            }
            world.set_instances(*id, &places)?;
        }
        Ok(())
    }

    fn effects(
        &mut self,
        world: &mut World,
        fx: &mut Emitters,
        pad: &Pad,
        now: Now,
        near: bool,
        nozzle: Vec3,
    ) -> EngineResult<()> {
        let at = |local: DVec2, y: f64| pad.world_at(local, y).as_vec3();
        // Vapour off the hull before the launch and while it cools.
        let venting = matches!(now.phase, Phase::Venting | Phase::Countdown(_))
            || (now.phase == Phase::Landed && now.local < hw_sim::rocket::ARMS_IN.1);
        let vent_at = at(DVec2::ZERO, STAND_H + now.height + 17.6);
        emitter_on(world, &mut fx.vapour, near && venting, vent_at, || {
            ParticleEmitter::smoke(vent_at, 2.2)
                .with_rate(40.0)
                .with_shape(ParticleShape::Ring)
                .with_spread(Vec3::new(2.3, 0.4, 2.3))
                .with_velocity(Vec3::new(0.0, -1.2, 0.0))
                .with_velocity_jitter(Vec3::new(1.4, 0.5, 1.4))
                .with_acceleration(Vec3::new(0.3, -0.4, 0.0))
                .with_lifetime(4.0)
                .with_color(lin(0.92, 0.94, 0.96, 0.55))
                .with_secondary_color(lin(0.85, 0.87, 0.9, 0.0))
        })?;
        // Fire and smoke out of the duct while the engine burns low over
        // the stand.
        let low = now.shown && now.thrust > 0.05 && now.height < 120.0;
        let k = (1.0 - now.height / 120.0).clamp(0.0, 1.0) as f32;
        let dir = {
            let p = pad.world_at(DVec2::new(-1.0, 0.0), 0.0) - pad.world_at(DVec2::ZERO, 0.0);
            p.as_vec3().normalize_or(Vec3::X)
        };
        let mouth = at(DVec2::new(DUCT_X - 0.5, 0.0), 1.2);
        emitter_on(world, &mut fx.duct_fire, near && low, mouth, || {
            ParticleEmitter::flames(mouth, 4.0)
                .with_rate(220.0)
                .with_velocity(dir * 24.0 + Vec3::Y * 2.0)
                .with_velocity_jitter(Vec3::new(4.0, 3.0, 4.0))
                .with_lifetime(0.9)
                .with_size(2.4)
        })?;
        emitter_on(world, &mut fx.duct_smoke, near && low, mouth, || {
            ParticleEmitter::smoke(mouth, 10.0)
                .with_rate(110.0)
                .with_velocity(dir * 18.0 + Vec3::Y * 3.0)
                .with_velocity_jitter(Vec3::new(5.0, 3.0, 5.0))
                .with_acceleration(Vec3::new(0.0, 1.2, 0.0))
                .with_drag(0.5)
                .with_lifetime(9.0)
                .with_size(4.5)
                .with_color(lin(0.78, 0.76, 0.72, 0.75))
                .with_secondary_color(lin(0.9, 0.9, 0.9, 0.0))
        })?;
        if let (Some(id), true) = (fx.duct_smoke, low) {
            // Fading as it climbs away.
            world
                .particles_mut()
                .set_emitter_position(id, mouth + Vec3::Y * (1.0 - k))?;
        }
        // Fire off the nozzle and a smoke trail behind the rocket.
        let burning = now.shown && now.thrust > 0.05 && now.height > 2.0;
        emitter_on(world, &mut fx.plume, near && burning, nozzle, || {
            ParticleEmitter::flames(nozzle, 3.0)
                .with_rate(160.0)
                .with_velocity(Vec3::new(0.0, -30.0, 0.0))
                .with_velocity_jitter(Vec3::new(2.5, 6.0, 2.5))
                .with_lifetime(0.5)
                .with_size(1.8)
        })?;
        emitter_on(world, &mut fx.trail, near && burning, nozzle, || {
            ParticleEmitter::smoke(nozzle, 6.0)
                .with_rate(90.0)
                .with_velocity(Vec3::new(0.0, -6.0, 0.0))
                .with_velocity_jitter(Vec3::new(1.5, 2.0, 1.5))
                .with_acceleration(Vec3::new(0.4, 0.2, 0.1))
                .with_lifetime(16.0)
                .with_size(3.0)
                .with_color(lin(0.86, 0.85, 0.83, 0.7))
                .with_secondary_color(lin(0.92, 0.92, 0.93, 0.0))
        })?;
        // Dust thrown off the pad at lift-off and touchdown.
        if let Some(last) = self.last {
            let lifted = last.phase == Phase::Ignition && now.phase == Phase::Ascent;
            let landed = last.phase == Phase::Descent && now.phase == Phase::Landed;
            if near && (lifted || landed) {
                let ground = at(DVec2::ZERO, 0.5);
                world.particles_mut().burst(
                    ParticleEmitter::dust(ground, 14.0, rgb(196, 188, 170))
                        .with_shape(ParticleShape::Ring)
                        .with_velocity_jitter(Vec3::new(9.0, 2.0, 9.0)),
                )?;
            }
        }
        Ok(())
    }

    /// What the soundscape hears: the roar while the engine burns, the
    /// hiss while it vents, and the moments.
    fn sounds(&mut self, pad: &Pad, now: Now, nozzle: Vec3, burning: bool) {
        let at = |local: DVec2, y: f64| pad.world_at(local, y).as_vec3();
        self.heard.roar = burning.then_some((nozzle, now.thrust as f32));
        let venting = matches!(now.phase, Phase::Venting | Phase::Countdown(_))
            || (now.phase == Phase::Landed && now.local < hw_sim::rocket::ARMS_IN.1);
        self.heard.vent = venting.then(|| at(DVec2::ZERO, STAND_H + 17.6));
        let Some(last) = self.last else {
            return;
        };
        let board = at(DVec2::new(TOWER_X, TOWER_HALF + 1.0), BOARD_Y);
        let bunker = at(
            DVec2::new(0.0, hw_gen::urban::gantry::BUNKER_Z),
            hw_gen::urban::gantry::BUNKER_H + 2.0,
        );
        let shots = &mut self.heard.shots;
        match (last.phase, now.phase) {
            (Phase::Rest, Phase::Venting) => shots.push((Shot::Klaxon, bunker)),
            (a, Phase::Countdown(n)) if a != Phase::Countdown(n) => {
                shots.push((Shot::Beep(n), board));
                if n == 10 {
                    shots.push((Shot::Klaxon, bunker));
                }
            }
            (Phase::Countdown(_), Phase::Ignition) => {
                shots.push((Shot::Ignition, at(DVec2::new(DUCT_X, 0.0), 1.5)))
            }
            (Phase::Descent, Phase::Landed) => {
                shots.push((Shot::Touchdown, at(DVec2::ZERO, STAND_H)))
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hw_sim::rocket::{Schedule, IGNITION, LANDED, LIFTOFF};

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn a_launch_is_heard() {
        let pad = Pad {
            landmark: hw_gen::urban::landmarks::Landmark {
                kind: hw_gen::urban::landmarks::LandmarkKind::RocketGantry,
                pos: DVec2::ZERO,
                plaza: 26.0,
                base: 2.0,
                low: 1.0,
                height: TOWER_H,
                dir: DVec2::Y,
                stairs: Vec::new(),
                variant: 0,
            },
            schedule: Schedule::new(hw_core::Seed(4)),
        };
        let mut v = RocketView {
            pad: Some(pad.clone()),
            job: None,
            live: None,
            last: None,
            heard: Heard::default(),
            clock: 0.0,
            rng: 1,
            fixed: None,
        };
        let c = pad.schedule.first;
        let (mut beeps, mut klaxons, mut ignitions, mut touchdowns, mut roared) = (0, 0, 0, 0, 0);
        let mut t = c - 2.0;
        while t < c + LANDED + 5.0 {
            let now = pad.schedule.at(t);
            let burning = now.shown && now.thrust > 0.01;
            v.sounds(&pad, now, Vec3::ZERO, burning);
            v.last = Some(now);
            for (s, _) in v.heard().shots {
                match s {
                    Shot::Beep(_) => beeps += 1,
                    Shot::Klaxon => klaxons += 1,
                    Shot::Ignition => ignitions += 1,
                    Shot::Touchdown => touchdowns += 1,
                }
            }
            if v.heard.roar.is_some() {
                roared += 1;
            }
            t += 1.0 / 30.0;
        }
        assert_eq!(beeps, 10, "ten seconds counted");
        assert_eq!(klaxons, 2);
        assert_eq!(ignitions, 1);
        assert_eq!(touchdowns, 1);
        assert!(roared as f64 > (LIFTOFF - IGNITION + 20.0) * 30.0);
    }
}
