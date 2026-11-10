//! The mad scientist's Tesla tower, alive: green arcs that crackle from the
//! dome's lip down into the yard, flicker, jump and find a new way every
//! few frames; tendrils licking off the ball on the needle; a pulse that
//! climbs the coil; and every few seconds, as the charge builds, a great
//! bolt that cracks down onto one of the lab's lightning rods, lighting up
//! the street. The sound of it is the soundscape's ([`Heard`]).
//!
//! Every arc shape is built once, as an instanced Engine entity with no
//! instances: showing one is giving it a place, hiding it is taking the
//! place away, so nothing is built or uploaded while the tower crackles.
//! Brightness rides on the instance tint, which multiplies the glow too.
//! Everything is made when the camera comes within [`LOAD`] of the tower
//! and dropped past [`DROP`].

use crate::citymesh::{MeshBuilder, Surf};
use crate::landmarkview::{strut, tesla_ball, tesla_core, tesla_dims, tesla_rim, TeslaDims, ARC};
use engine::prelude::*;
use glam::{DVec3, Vec3};
use hw_gen::urban::landmarks::{tesla_rods, Landmark, LandmarkKind, TESLA_ROD_TIP};
use hw_gen::urban::world::TownWorld;

/// The live parts are made within this distance of the tower (m), past the
/// near model's reach so they are there whenever it is.
const LOAD: f32 = 1200.0;
/// ... and dropped past this.
const DROP: f32 = 1400.0;
/// Arcs play only this near (further, nobody sees them flicker).
const ACTIVE: f32 = 900.0;
/// The coil's core, in this many pieces, for the pulse to climb.
const CORE_PIECES: usize = 10;
/// Shapes made for each kind of arc.
const GROUND_SHAPES: usize = 10;
const ROD_SHAPES: usize = 5;
const TENDRIL_SHAPES: usize = 8;
/// Arcs that can be live at once.
const GROUND_SLOTS: usize = 3;
const TENDRIL_SLOTS: usize = 3;

/// What the soundscape hears of the tower this frame.
#[derive(Clone, Debug, Default)]
pub struct Heard {
    /// Where its hum comes from (the dome), if the tower is near.
    pub hum: Option<Vec3>,
    /// How charged it is (0..1): the hum rises with it.
    pub charge: f32,
    /// Discharges since the last frame: where, and how big (1 a bolt onto
    /// a rod, less a small arc).
    pub strikes: Vec<(Vec3, f32)>,
}

/// A tiny xorshift: the sparks need no seed (nothing in the world depends
/// on them), only variety.
struct Rng(u64);

impl Rng {
    fn f(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
    fn pick(&mut self, n: usize) -> usize {
        ((self.f() * n as f32) as usize).min(n - 1)
    }
}

/// A deterministic random stream for building shapes.
fn shape_rng(k: u64) -> Rng {
    Rng(0x2545_f491_4f6c_dd1d ^ (k.wrapping_add(1)).wrapping_mul(0x9e37_79b9_7f4a_7c15))
}

/// A jagged bolt from `a` to `b` (midpoint displacement), as points.
fn bolt_path(a: DVec3, b: DVec3, rough: f64, depth: u32, rng: &mut Rng) -> Vec<DVec3> {
    let mut pts = vec![a, b];
    let mut amp = a.distance(b) * rough;
    for _ in 0..depth {
        let mut next = Vec::with_capacity(pts.len() * 2);
        for w in pts.windows(2) {
            let (p, q) = (w[0], w[1]);
            let d = (q - p).normalize_or(DVec3::Y);
            let u = if d.y.abs() > 0.9 { DVec3::X } else { DVec3::Y };
            let s = d.cross(u).normalize();
            let t = d.cross(s).normalize();
            let off =
                s * (rng.f() as f64 - 0.5) * 2.0 * amp + t * (rng.f() as f64 - 0.5) * 2.0 * amp;
            next.push(p);
            next.push((p + q) * 0.5 + off);
        }
        next.push(*pts.last().expect("a point"));
        pts = next;
        amp *= 0.55;
    }
    pts
}

/// A bolt with forks: the main path `w` thick, and `forks` thinner side
/// branches that die out in the air.
fn bolt(b: &mut MeshBuilder, a: DVec3, c: DVec3, w: f64, forks: usize, rng: &mut Rng) {
    let core = rgb(215, 255, 190);
    let pts = bolt_path(a, c, 0.16, 5, rng);
    let n = pts.len();
    for (i, s) in pts.windows(2).enumerate() {
        // Thinning towards the far end a little.
        let k = 1.0 - 0.35 * i as f64 / n as f64;
        strut(b, s[0], s[1], w * k, core);
    }
    for _ in 0..forks {
        let i = 2 + rng.pick(n.saturating_sub(6).max(1));
        let from = pts[i.min(n - 1)];
        let along = (c - a).normalize_or(DVec3::NEG_Y);
        let len = a.distance(c) * rng.range(0.12, 0.3) as f64;
        let side = DVec3::new(
            rng.f() as f64 - 0.5,
            rng.f() as f64 * 0.4 - 0.3,
            rng.f() as f64 - 0.5,
        )
        .normalize_or(DVec3::X);
        let to = from + (along + side * 1.2).normalize() * len;
        let twig = bolt_path(from, to, 0.2, 3, rng);
        for s in twig.windows(2) {
            strut(b, s[0], s[1], w * 0.45, core);
        }
    }
}

fn glow_mesh(f: impl FnOnce(&mut MeshBuilder)) -> Mesh {
    let mut b = MeshBuilder::new();
    b.surf = Surf::Glow;
    f(&mut b);
    b.surf = Surf::Plain;
    b.finish()
}

/// The shapes, all in the tower's frame turned so the arc runs along +z
/// (a tendril: from the ball's centre).
struct Shapes {
    ground: Vec<Mesh>,
    rod: Vec<Mesh>,
    tendril: Vec<Mesh>,
}

/// How far out ground arcs land: inside the fence, outside the lab.
fn ground_reach(l: &Landmark) -> f64 {
    (l.plaza - 1.2).max(8.0)
}

fn shapes(l: &Landmark, d: &TeslaDims) -> Shapes {
    let rod_r = tesla_rods()[0].length();
    let ground = (0..GROUND_SHAPES)
        .map(|k| {
            let mut r = shape_rng(k as u64);
            glow_mesh(|b| {
                let from = DVec3::new(0.0, d.rim_y - 0.15, d.dr + 0.2);
                let to = DVec3::new(
                    r.range(-1.5, 1.5) as f64,
                    0.15,
                    ground_reach(l) - r.range(0.0, 1.5) as f64,
                );
                bolt(b, from, to, 0.13, 2 + k % 3, &mut r);
            })
        })
        .collect();
    let rod = (0..ROD_SHAPES)
        .map(|k| {
            let mut r = shape_rng(100 + k as u64);
            glow_mesh(|b| {
                let from = DVec3::new(0.0, d.rim_y - 0.2, d.dr + 0.1);
                let to = DVec3::new(0.0, TESLA_ROD_TIP + 0.2, rod_r);
                bolt(b, from, to, 0.3, 3 + k % 2, &mut r);
                // A second, thinner stroke beside the first.
                bolt(b, from, to, 0.12, 1, &mut r);
            })
        })
        .collect();
    let tendril = (0..TENDRIL_SHAPES)
        .map(|k| {
            let mut r = shape_rng(200 + k as u64);
            glow_mesh(|b| {
                let len = r.range(3.0, 6.5) as f64;
                let from = DVec3::new(0.0, 0.0, 0.9);
                let to = DVec3::new(r.range(-1.0, 1.0) as f64, r.range(-0.5, 1.5) as f64, len);
                bolt(b, from, to, 0.11, 1 + k % 2, &mut r);
            })
        })
        .collect();
    Shapes {
        ground,
        rod,
        tendril,
    }
}

/// An arc that comes and goes: which shape, turned how far, how bright.
#[derive(Clone, Copy, Default)]
struct Slot {
    /// Seconds left showing (0: off).
    life: f32,
    /// Seconds until it lights again, while off.
    wait: f32,
    /// Seconds until it jumps to another shape.
    hop: f32,
    shape: usize,
    yaw: f32,
    pitch: f32,
    bright: f32,
}

/// The bolt onto a rod: its strokes, one after another.
struct Strike {
    rod: usize,
    t: f32,
    /// (start, end, shape) of each stroke, in seconds from the start.
    strokes: [(f32, f32, usize); 3],
    /// Not drawn yet (its sparks fly on the first frame).
    fresh: bool,
}

struct Live {
    /// The tower's origin (plinth top), and its yaw.
    origin: Vec3,
    yaw: f32,
    dims: TeslaDims,
    /// The glowing parts: core pieces, then rim, then ball.
    glow: Vec<EntityId>,
    ground: Vec<EntityId>,
    rod: Vec<EntityId>,
    tendril: Vec<EntityId>,
    lamp_dome: LampId,
    lamp_flash: LampId,
}

pub struct TeslaView {
    tower: Option<Landmark>,
    live: Option<Live>,
    rng: Rng,
    clock: f32,
    charge: f32,
    /// Seconds until the next bolt onto a rod.
    next_strike: f32,
    strike: Option<Strike>,
    ground: [Slot; GROUND_SLOTS],
    tendrils: [Slot; TENDRIL_SLOTS],
    /// Where the pulse is up the coil (0..1, and on past 1 while waiting).
    pulse: f32,
    heard: Heard,
}

impl TeslaView {
    pub fn new(town: &TownWorld) -> Self {
        Self::with_tower(
            town.landmarks
                .iter()
                .find(|l| l.kind == LandmarkKind::TeslaTower)
                .cloned(),
        )
    }

    /// For one given tower (the `powerfx` example's close-ups).
    pub fn for_tower(tower: Landmark) -> Self {
        Self::with_tower(Some(tower))
    }

    fn with_tower(tower: Option<Landmark>) -> Self {
        Self {
            tower,
            live: None,
            rng: Rng(0x5eed_7e51_a000_0001),
            clock: 0.0,
            charge: 0.3,
            next_strike: 4.0,
            strike: None,
            ground: [Slot::default(); GROUND_SLOTS],
            tendrils: [Slot::default(); TENDRIL_SLOTS],
            pulse: 0.0,
            heard: Heard::default(),
        }
    }

    /// What the soundscape should hear this frame (strikes are handed over
    /// once).
    pub fn heard(&mut self) -> Heard {
        let strikes = std::mem::take(&mut self.heard.strikes);
        Heard {
            strikes,
            ..self.heard.clone()
        }
    }

    /// Is the tower's live part made (for the memory checks and tests)?
    pub fn is_live(&self) -> bool {
        self.live.is_some()
    }

    fn spawn(world: &mut World, l: &Landmark) -> EngineResult<Live> {
        let d = tesla_dims(l.height);
        let origin = Vec3::new(l.pos.x as f32, l.base as f32, l.pos.y as f32);
        let yaw = l.dir.x.atan2(l.dir.y).to_degrees() as f32;
        let make = |world: &mut World, mesh: Mesh| -> EngineResult<EntityId> {
            let id = world.spawn_instanced(mesh);
            world.set_casts_shadow(id, false)?;
            Ok(id)
        };
        let mut glow = Vec::new();
        for i in 0..CORE_PIECES {
            let (t0, t1) = (
                i as f64 / CORE_PIECES as f64,
                (i + 1) as f64 / CORE_PIECES as f64,
            );
            glow.push(make(world, glow_mesh(|b| tesla_core(b, &d, t0, t1, true)))?);
        }
        glow.push(make(world, glow_mesh(|b| tesla_rim(b, &d, true)))?);
        glow.push(make(world, glow_mesh(|b| tesla_ball(b, &d, true)))?);
        // Plasma that climbs and writhes over the glowing parts.
        for &id in &glow {
            crate::powerfx::dress_plasma(world, id);
        }
        let s = shapes(l, &d);
        let ids = |world: &mut World, v: Vec<Mesh>| -> EngineResult<Vec<EntityId>> {
            v.into_iter().map(|m| make(world, m)).collect()
        };
        let ground = ids(world, s.ground)?;
        let rod = ids(world, s.rod)?;
        let tendril = ids(world, s.tendril)?;
        // Pulses racing down every arc.
        for &id in ground.iter().chain(&rod).chain(&tendril) {
            crate::powerfx::dress_arc(world, id);
        }
        let arc = rgb(ARC.0, ARC.1, ARC.2);
        let at = |up: f64| origin + Vec3::Y * up as f32;
        let lamp_dome = world.add_lamp(Lamp::point(at(d.rim_y - 1.0), arc, 40.0, 60.0))?;
        let lamp_flash = world.add_lamp(Lamp::point(at(TESLA_ROD_TIP), arc, 0.0, 70.0))?;
        Ok(Live {
            origin,
            yaw,
            dims: d,
            glow,
            ground,
            rod,
            tendril,
            lamp_dome,
            lamp_flash,
        })
    }

    pub fn clear(&mut self, world: &mut World) {
        if let Some(live) = self.live.take() {
            for id in live
                .glow
                .iter()
                .chain(&live.ground)
                .chain(&live.rod)
                .chain(&live.tendril)
            {
                world.despawn(*id);
            }
            world.remove_lamp(live.lamp_dome);
            world.remove_lamp(live.lamp_flash);
        }
        self.heard = Heard::default();
    }

    pub fn update(&mut self, world: &mut World, dt: f32) -> EngineResult<()> {
        let Some(l) = self.tower.clone() else {
            return Ok(());
        };
        let eye = world.camera().eye();
        let foot = Vec3::new(l.pos.x as f32, l.base as f32, l.pos.y as f32);
        let far = foot.distance(eye);
        if far > DROP {
            self.clear(world);
            return Ok(());
        }
        if self.live.is_none() {
            if far > LOAD {
                return Ok(());
            }
            self.live = Some(Self::spawn(world, &l)?);
        }
        let dt = dt.clamp(0.0, 0.1);
        let active = far < ACTIVE;
        self.step(dt, active);
        let live = self.live.take().expect("live");
        let drawn = self.draw(world, &live, &l, active);
        self.live = Some(live);
        drawn
    }

    /// The tower's world point at `local` (its own frame).
    fn world_at(live: &Live, local: DVec3) -> Vec3 {
        let a = (live.yaw as f64).to_radians();
        let (s, c) = a.sin_cos();
        // Local +z is the way it faces, +x its right (see `Landmark::to_plan`).
        let x = local.x * c + local.z * s;
        let z = -local.x * s + local.z * c;
        live.origin + Vec3::new(x as f32, local.y as f32, z as f32)
    }

    /// Advances the charge, the arcs and the bolt.
    fn step(&mut self, dt: f32, active: bool) {
        self.clock += dt;
        let r = &mut self.rng;
        // The charge builds towards the next bolt.
        self.charge = (self.charge + dt * 0.12).min(1.0);
        self.pulse += dt * (0.6 + 1.2 * self.charge);
        if self.pulse > 1.6 {
            self.pulse = 0.0;
        }
        if let Some(s) = &mut self.strike {
            s.t += dt;
            if s.t > s.strokes[2].1 + 0.1 {
                self.strike = None;
            }
        } else if active {
            self.next_strike -= dt;
            if self.next_strike <= 0.0 {
                let rod = r.pick(4);
                let a = r.pick(ROD_SHAPES);
                let b = (a + 1 + r.pick(ROD_SHAPES - 1)) % ROD_SHAPES;
                let c = r.pick(ROD_SHAPES);
                let t1 = r.range(0.05, 0.09);
                let t2 = t1 + r.range(0.04, 0.08);
                let t3 = t2 + r.range(0.06, 0.1);
                let t4 = t3 + r.range(0.03, 0.06);
                self.strike = Some(Strike {
                    rod,
                    t: 0.0,
                    strokes: [(0.0, t1, a), (t2, t3, b), (t4, t4 + r.range(0.12, 0.25), c)],
                    fresh: true,
                });
                self.next_strike = r.range(3.5, 9.0);
                self.charge = 0.1;
                if let Some(live) = &self.live {
                    let tip = tesla_rods()[rod];
                    let p = Self::world_at(live, DVec3::new(tip.x, TESLA_ROD_TIP, tip.y));
                    self.heard.strikes.push((p, 1.0));
                }
            }
        }
        // Small arcs into the yard: more often and longer as the charge rises.
        let reach = self.tower.as_ref().map_or(8.0, ground_reach);
        for k in 0..GROUND_SLOTS {
            let s = &mut self.ground[k];
            if s.life > 0.0 {
                s.life -= dt;
                s.hop -= dt;
                if s.hop <= 0.0 {
                    // Jump: another shape, a little further round.
                    s.shape = (s.shape + 1 + r.pick(GROUND_SHAPES - 1)) % GROUND_SHAPES;
                    s.yaw += r.range(-9.0, 9.0);
                    s.hop = r.range(0.04, 0.12);
                }
                s.bright = r.range(0.55, 1.5);
                if s.life <= 0.0 {
                    s.wait = r.range(0.3, 2.5) * (1.4 - self.charge);
                }
            } else if active {
                s.wait -= dt;
                if s.wait <= 0.0 {
                    s.life = r.range(0.15, 0.5 + 0.8 * self.charge);
                    s.hop = r.range(0.04, 0.12);
                    s.shape = r.pick(GROUND_SHAPES);
                    // Down near an axis of the lab, where it is narrowest.
                    s.yaw = 90.0 * r.pick(4) as f32 + r.range(-28.0, 28.0);
                    s.bright = 1.0;
                    if r.f() < 0.6 {
                        if let Some(live) = &self.live {
                            let a = (s.yaw as f64).to_radians();
                            let p = Self::world_at(
                                live,
                                DVec3::new(a.sin() * reach, 0.5, a.cos() * reach),
                            );
                            self.heard.strikes.push((p, 0.35));
                        }
                    }
                }
            }
        }
        // Tendrils off the ball, licking about all the time.
        for s in &mut self.tendrils {
            s.hop -= dt;
            if s.hop <= 0.0 {
                s.shape = r.pick(TENDRIL_SHAPES);
                s.yaw = r.range(0.0, 360.0);
                s.pitch = r.range(-35.0, 50.0);
                s.hop = r.range(0.05, 0.14);
                // Now and then a tendril goes dark for a moment.
                s.life = if r.f() < 0.25 + 0.6 * self.charge {
                    1.0
                } else {
                    0.0
                };
            }
            s.bright = r.range(0.6, 1.3);
        }
    }

    fn draw(
        &mut self,
        world: &mut World,
        live: &Live,
        l: &Landmark,
        active: bool,
    ) -> EngineResult<()> {
        let d = live.dims;
        let tint = |k: f32| Color::rgb01_unchecked(k, k, k);
        let at = |yaw: f32| {
            Place::new(live.origin.x, live.origin.y, live.origin.z).with_yaw_deg(live.yaw + yaw)
        };
        // The bolt now, if a stroke is lit.
        let stroke = self.strike.as_ref().and_then(|s| {
            s.strokes
                .iter()
                .find(|(a, b, _)| s.t >= *a && s.t < *b)
                .map(|(a, b, shape)| (s.rod, *shape, 1.0 - (s.t - a) / (b - a)))
        });
        let flash = stroke.map_or(0.0, |(_, _, f)| 0.6 + 0.4 * f);
        // The glow: a pulse climbing the core; the rim and ball humming with
        // the charge, flaring with a bolt.
        let hum = 0.55 + 0.45 * self.charge + 0.08 * (self.clock * 37.0).sin();
        for i in 0..CORE_PIECES {
            let y = (i as f32 + 0.5) / CORE_PIECES as f32;
            let bump = (-((y - self.pulse) * 6.0).powi(2)).exp();
            let k = hum * 0.8 + 0.9 * bump + flash;
            world.set_instances(live.glow[i], &[at(0.0).with_tint(tint(k))])?;
        }
        let edge = hum + 0.15 * (self.clock * 53.0).sin() + 1.2 * flash;
        world.set_instances(live.glow[CORE_PIECES], &[at(0.0).with_tint(tint(edge))])?;
        world.set_instances(
            live.glow[CORE_PIECES + 1],
            &[at(0.0).with_tint(tint(edge * 1.1))],
        )?;
        // Ground arcs.
        let mut places: Vec<Vec<Place>> = vec![Vec::new(); GROUND_SHAPES];
        if active {
            for s in &self.ground {
                if s.life > 0.0 {
                    places[s.shape].push(at(s.yaw).with_tint(tint(s.bright * 1.2)));
                }
            }
        }
        for (id, p) in live.ground.iter().zip(&places) {
            world.set_instances(*id, p)?;
        }
        // The bolt.
        for (k, id) in live.rod.iter().enumerate() {
            let p: Vec<Place> = match stroke {
                Some((rod, shape, f)) if shape == k && active => {
                    let tip = tesla_rods()[rod];
                    let yaw = tip.x.atan2(tip.y).to_degrees() as f32;
                    vec![at(yaw).with_tint(tint(1.4 + f))]
                }
                _ => Vec::new(),
            };
            world.set_instances(*id, &p)?;
        }
        // Tendrils.
        let ball = Self::world_at(live, DVec3::new(0.0, d.ball_y - 1.0, 0.0));
        let mut places: Vec<Vec<Place>> = vec![Vec::new(); TENDRIL_SHAPES];
        if active {
            for s in &self.tendrils {
                if s.life > 0.0 {
                    places[s.shape].push(
                        Place::new(ball.x, ball.y, ball.z)
                            .with_yaw_deg(s.yaw)
                            .with_pitch_deg(-s.pitch)
                            .with_tint(tint(s.bright)),
                    );
                }
            }
        }
        for (id, p) in live.tendril.iter().zip(&places) {
            world.set_instances(*id, p)?;
        }
        // Lights: the dome's glow over the lattice and the yard, and the
        // bolt's flash at its rod.
        let arc = rgb(ARC.0, ARC.1, ARC.2);
        let dome = Self::world_at(live, DVec3::new(0.0, d.rim_y - 1.0, 0.0));
        world.set_lamp(
            live.lamp_dome,
            Lamp::point(dome, arc, 30.0 + 40.0 * edge, 60.0),
        )?;
        let (pos, power) = match stroke {
            Some((rod, _, f)) => {
                let tip = tesla_rods()[rod];
                (
                    Self::world_at(live, DVec3::new(tip.x, TESLA_ROD_TIP, tip.y)),
                    300.0 + 300.0 * f,
                )
            }
            None => {
                // The brightest yard arc lights where it lands.
                let reach = ground_reach(l);
                match self
                    .ground
                    .iter()
                    .filter(|s| s.life > 0.0 && active)
                    .max_by(|a, b| a.bright.total_cmp(&b.bright))
                {
                    Some(s) => {
                        let a = (s.yaw as f64).to_radians();
                        (
                            Self::world_at(live, DVec3::new(a.sin() * reach, 1.5, a.cos() * reach)),
                            40.0 * s.bright,
                        )
                    }
                    None => (dome, 0.0),
                }
            }
        };
        world.set_lamp(live.lamp_flash, Lamp::point(pos, arc, power, 70.0))?;
        // Sparks showering off the rod as a bolt lands.
        if let Some(s) = &mut self.strike {
            if std::mem::take(&mut s.fresh) && active {
                let tip = tesla_rods()[s.rod];
                let p = Self::world_at(live, DVec3::new(tip.x, TESLA_ROD_TIP, tip.y));
                world.particles_mut().burst(
                    ParticleEmitter::sparks(p, Vec3::Y * 6.0, 90)
                        .with_color(arc)
                        .with_secondary_color(rgb(240, 255, 225)),
                )?;
            }
        }
        self.heard.hum = Some(dome);
        self.heard.charge = self.charge;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec2;

    fn tower() -> Landmark {
        Landmark {
            kind: LandmarkKind::TeslaTower,
            pos: DVec2::ZERO,
            plaza: 12.0,
            base: 3.0,
            low: 1.0,
            height: 60.0,
            dir: DVec2::Y,
            stairs: Vec::new(),
        }
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn a_bolt_runs_from_end_to_end() {
        let (a, b) = (DVec3::new(0.0, 40.0, 9.0), DVec3::new(0.0, 0.0, 10.0));
        let p = bolt_path(a, b, 0.16, 5, &mut shape_rng(3));
        assert_eq!(p.len(), 33);
        assert_eq!(p[0], a);
        assert_eq!(*p.last().unwrap(), b);
        // Jagged, but not wild: no kink strays far off the line.
        for q in &p {
            assert!((q.z - 9.5).abs() < 8.0, "{q}");
        }
    }

    #[test]
    fn every_arc_shape_has_faces() {
        let l = tower();
        let s = shapes(&l, &tesla_dims(l.height));
        for m in s.ground.iter().chain(&s.rod).chain(&s.tendril) {
            assert!(m.face_count() > 50);
        }
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn arcs_come_and_go_and_bolts_strike() {
        let mut t = TeslaView::with_tower(Some(tower()));
        let (mut lit, mut dark, mut bolts, mut hops) = (0, 0, 0, 0);
        let mut was_striking = false;
        let mut last_shape = [0; GROUND_SLOTS];
        let frames = 60 * 60;
        for _ in 0..frames {
            t.step(1.0 / 60.0, true);
            let n = t.ground.iter().filter(|s| s.life > 0.0).count();
            if n > 0 {
                lit += 1;
            } else {
                dark += 1;
            }
            for (k, s) in t.ground.iter().enumerate() {
                if s.life > 0.0 && s.shape != last_shape[k] {
                    hops += 1;
                }
                last_shape[k] = s.shape;
            }
            let striking = t.strike.is_some();
            if striking && !was_striking {
                bolts += 1;
            }
            was_striking = striking;
        }
        // A minute: the yard crackles most of the time but not always, the
        // arcs jump about, and a bolt hits a rod every few seconds.
        assert!(lit > frames / 3 && dark > 0, "lit {lit} dark {dark}");
        assert!(hops > 200, "{hops}");
        assert!((6..=18).contains(&bolts), "{bolts}");
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn far_away_it_rests() {
        let mut t = TeslaView::with_tower(Some(tower()));
        for _ in 0..600 {
            t.step(1.0 / 60.0, false);
        }
        assert!(t.strike.is_none());
        assert!(t.ground.iter().all(|s| s.life <= 0.0));
    }
}
