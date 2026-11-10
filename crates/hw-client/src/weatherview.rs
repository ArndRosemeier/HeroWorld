//! The weather tower, alive. Round after round ([`hw_sim::rainmaker`],
//! from the game clock, so every player sees the same shower): the cannons
//! on its crown fire puffs of vapour one after another, a cloud builds
//! over the blocks in front of it and darkens, rain pours down under it
//! with thunder now and then, then it eases off, the cloud thins and
//! drifts apart, and a rainbow stands over the wet streets. The globe on
//! top glows gold in fair weather and blue in the rain, the anemometer
//! spins faster in the storm, and the barometer's needle on the mast
//! swings from FAIR to RAIN and back.
//!
//! The cloud is soft lit puffs (Engine particles, sent in small bursts so
//! each takes the cloud's colour of the moment), the rain streaks around
//! the camera and over the whole shower, with a curtain of rain hanging
//! under the cloud for the view from afar. Shapes are built once on a
//! worker thread and shown as instanced entities: moving them is giving
//! them a place, so nothing is rebuilt while the weather plays. Everything
//! live is made within [`LOAD`] of the tower and dropped past [`DROP`].

use crate::citymesh::{MeshBuilder, Surf};
use crate::weathermodel::{self as model, cannon, dial, CANNON_LEN};
use engine::prelude::*;
use glam::{DVec2, DVec3, Vec3};
use hw_gen::urban::landmarks::{Landmark, LandmarkKind};
use hw_gen::urban::weather_tower::{rain_centre, WxDims, WX_CANNONS, WX_RAIN_R};
use hw_gen::urban::world::TownWorld;
use hw_sim::rainmaker::{shower, Shower};
use std::sync::mpsc::{channel, Receiver};

/// The live parts are made within this distance of the tower (m).
const LOAD: f32 = 1600.0;
/// ... and dropped past this.
const DROP: f32 = 1800.0;
/// The rainbow's radius (m) and how far beyond the shower it stands.
const BOW_R: f64 = 82.0;
const BOW_BEYOND: f64 = 70.0;
/// How fast rain falls (m/s).
const DROP_SPEED: f32 = 15.0;
/// Seconds a cloud puff lives.
const PUFF_LIFE: f32 = 16.0;

/// What the soundscape hears of the tower this frame.
#[derive(Clone, Debug, Default)]
pub struct Heard {
    /// The machinery's whirr (at the crown), and how hard it runs (0..1).
    pub whirr: Option<Vec3>,
    pub spin: f32,
    /// The rain: the nearest point of the shower to the camera, and how
    /// hard it rains there (0..1).
    pub rain: Option<(Vec3, f32)>,
    /// Since the last frame: cannons fired, thunder (where the flash was),
    /// and the rainbow coming out.
    pub puffs: Vec<Vec3>,
    pub thunder: Vec<Vec3>,
    pub bow: Option<Vec3>,
}

/// A tiny xorshift: flashes and puffs need variety, not a seed (nothing in
/// the world depends on them).
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
}

/// The shapes, built off the main thread.
pub struct Parts {
    anemo: Mesh,
    needle: Mesh,
    globe: Mesh,
    mouths: Mesh,
    curtain: Mesh,
    bow: Mesh,
}

fn glow(f: impl FnOnce(&mut MeshBuilder)) -> Mesh {
    let mut b = MeshBuilder::new();
    b.surf = Surf::Glow;
    f(&mut b);
    b.surf = Surf::Plain;
    b.finish()
}

/// Every shape for the tower `l`. Pure data: runs on a worker.
pub fn parts(l: &Landmark) -> Parts {
    let d = WxDims::of(l.height);
    let mut anemo = MeshBuilder::new();
    model::anemometer(&mut anemo, &d);
    let mut needle = MeshBuilder::new();
    model::needle(&mut needle, dial(&d).1);
    Parts {
        anemo: anemo.finish(),
        needle: needle.finish(),
        globe: glow(|b| model::globe(b, &d, true)),
        mouths: glow(|b| model::mouths(b, &d, true)),
        curtain: curtain(&d),
        bow: rainbow(),
    }
}

/// A hash of two numbers to 0..1 (the curtain's streaks).
fn hash01(a: u32, b: u32) -> f32 {
    let mut h = a.wrapping_mul(0x9e37_79b9) ^ b.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    (h & 0xffff) as f32 / 65535.0
}

/// The curtain of rain under the cloud, seen from afar: two rings of
/// faint grey streaks from below the ground up into the cloud, thickest
/// low down, fading out at the top and round their edges. Its origin is
/// the shower's middle at the plinth's height.
fn curtain(d: &WxDims) -> Mesh {
    let mut m = Mesh::new();
    let top = d.cloud_y() as f32 + 4.0;
    let bottom = -25.0f32;
    let seg = 120u32;
    let rows = 6u32;
    for (ring, (r, a_max)) in [(WX_RAIN_R * 0.82, 0.5f32), (WX_RAIN_R * 0.45, 0.32)]
        .into_iter()
        .enumerate()
    {
        let r = r as f32;
        let mut ids = Vec::new();
        for i in 0..=seg {
            let a = std::f32::consts::TAU * i as f32 / seg as f32;
            // Streaks: each column of the curtain its own thickness.
            let s = 0.25 + 0.75 * hash01(i % seg, ring as u32).powf(1.6);
            let col: Vec<PointId> = (0..=rows)
                .map(|k| {
                    let t = k as f32 / rows as f32;
                    let y = bottom + (top - bottom) * t;
                    // Wavering a little, as rain does in the wind.
                    let rr = r * (1.0 + 0.04 * (a * 5.0 + t * 3.0).sin());
                    let p = Vec3::new(a.sin() * rr, y, a.cos() * rr);
                    // Even down to the ground, fading out into the cloud.
                    let fade = 1.0 - ((t - 0.7) / 0.3).clamp(0.0, 1.0);
                    let alpha = (a_max * s * fade).clamp(0.004, 1.0);
                    let id = m.add_point(p).expect("point");
                    let shade = 0.5 + 0.15 * t;
                    m.set_point_color(
                        id,
                        Color {
                            r: shade * 0.92,
                            g: shade * 0.95,
                            b: shade,
                            a: alpha,
                        },
                    )
                    .expect("colour");
                    m.set_point_normal(id, Vec3::new(a.sin(), 0.0, a.cos()))
                        .expect("normal");
                    id
                })
                .collect();
            ids.push(col);
        }
        for i in 0..seg as usize {
            for k in 0..rows as usize {
                m.add_quad(ids[i][k], ids[i + 1][k], ids[i + 1][k + 1], ids[i][k + 1])
                    .expect("quad");
            }
        }
    }
    m.set_surface_material(SurfaceMaterial {
        roughness: 0.9,
        ..SurfaceMaterial::DEFAULT
    });
    m
}

/// The rainbow: a half circle in the x-y plane round the origin, facing
/// +z, seven bands from red outside to violet inside with soft edges, its
/// feet fading out, and a faint second bow outside it with its colours
/// the other way round.
fn rainbow() -> Mesh {
    // Linear colours, from outside in.
    const BANDS: [[f32; 3]; 7] = [
        [1.0, 0.08, 0.05],
        [1.0, 0.35, 0.02],
        [1.0, 0.85, 0.05],
        [0.12, 0.85, 0.12],
        [0.05, 0.35, 1.0],
        [0.18, 0.08, 0.75],
        [0.45, 0.1, 0.8],
    ];
    let mut m = Mesh::new();
    let seg = 96u32;
    // (outer radius, width, strength, colours reversed)
    for (r_out, width, k, rev) in [
        (BOW_R, 7.5, 0.85f32, false),
        (BOW_R * 1.16, 8.5, 0.28, true),
    ] {
        let rows = 22u32;
        let mut ids = Vec::new();
        for i in 0..=seg {
            let a = std::f64::consts::PI * i as f64 / seg as f64;
            // The feet (low down at either end) fade into the air.
            let lift = a.sin() as f32;
            let foot = (lift * 3.0).clamp(0.0, 1.0);
            let col: Vec<PointId> = (0..=rows)
                .map(|j| {
                    let t = j as f32 / rows as f32;
                    let r = r_out - width * t as f64;
                    let p = Vec3::new((a.cos() * r) as f32, (a.sin() * r) as f32, 0.0);
                    let x = if rev { 1.0 - t } else { t } * (BANDS.len() - 1) as f32;
                    let (i0, f) = (x.floor() as usize, x.fract());
                    let i1 = (i0 + 1).min(BANDS.len() - 1);
                    let c: Vec<f32> = (0..3)
                        .map(|c| BANDS[i0][c] * (1.0 - f) + BANDS[i1][c] * f)
                        .collect();
                    // Soft at both edges.
                    // (sin π is a hair below 0: no NaN from its power.)
                    let edge = (std::f32::consts::PI * t).sin().max(0.0).powf(0.8);
                    let id = m.add_point(p).expect("point");
                    m.set_point_color(
                        id,
                        Color {
                            r: c[0],
                            g: c[1],
                            b: c[2],
                            a: (k * edge * foot).clamp(0.004, 1.0),
                        },
                    )
                    .expect("colour");
                    // Facing up: lit evenly by the sun and sky from any side.
                    m.set_point_normal(id, Vec3::Y).expect("normal");
                    id
                })
                .collect();
            ids.push(col);
        }
        for i in 0..seg as usize {
            for j in 0..rows as usize {
                m.add_quad(ids[i][j], ids[i + 1][j], ids[i + 1][j + 1], ids[i][j + 1])
                    .expect("quad");
            }
        }
    }
    m.set_surface_material(SurfaceMaterial {
        roughness: 1.0,
        emission: 0.5,
        ..SurfaceMaterial::DEFAULT
    });
    m
}

struct Live {
    origin: Vec3,
    yaw: f32,
    d: WxDims,
    anemo: EntityId,
    needle: EntityId,
    globe: EntityId,
    mouths: EntityId,
    curtain: EntityId,
    bow: EntityId,
    lamp_globe: LampId,
    lamp_flash: LampId,
    /// Rain emitters over the shower and round the camera, and how hard
    /// they were started for.
    rain: Vec<EmitterId>,
    near: Option<EmitterId>,
    tier: u8,
}

pub struct WeatherView {
    tower: Option<Landmark>,
    pending: Option<Receiver<Parts>>,
    parts: Option<Parts>,
    live: Option<Live>,
    rng: Rng,
    /// The anemometer's turn (degrees) and the needle's swing.
    spin: f32,
    needle: f32,
    /// Seconds to the next cloud burst, the next cannon shot, and the
    /// next thunder flash; how long the flash still shows.
    cloud_in: f32,
    cannon_in: f32,
    next_cannon: usize,
    shot: f32,
    flash_in: f32,
    flash: f32,
    flash_at: Vec3,
    bow_was: bool,
    /// Seconds added to the game clock (screenshots: `HW_WEATHER`).
    shift: f64,
    now: Shower,
    wet: f32,
    heard: Heard,
}

/// `HW_WEATHER=brew|rain|clear|bow|fair`: the round starts at that point
/// (screenshots of each stage).
fn shift_from_env() -> f64 {
    match std::env::var("HW_WEATHER").as_deref() {
        Ok("brew") => 75.0,
        Ok("rain") => 140.0,
        Ok("clear") => 192.0,
        Ok("bow") => 225.0,
        Ok("fair") => 20.0,
        _ => 0.0,
    }
}

impl WeatherView {
    pub fn new(town: &TownWorld) -> Self {
        let tower = town
            .landmarks
            .iter()
            .find(|l| l.kind == LandmarkKind::WeatherTower)
            .cloned();
        // The shapes are made on a worker; nothing shows until they are in.
        let pending = tower.clone().map(|l| {
            let (tx, rx) = channel();
            std::thread::Builder::new()
                .name("weather parts".into())
                .spawn(move || {
                    let _ = tx.send(parts(&l));
                })
                .expect("spawn the weather parts worker");
            rx
        });
        Self {
            tower,
            pending,
            parts: None,
            live: None,
            rng: Rng(0x7ea7_4e12_0000_0001),
            spin: 0.0,
            needle: 40.0,
            cloud_in: 0.0,
            cannon_in: 0.0,
            next_cannon: 0,
            shot: 0.0,
            flash_in: 6.0,
            flash: 0.0,
            flash_at: Vec3::ZERO,
            bow_was: false,
            shift: 0.0,
            now: Shower::default(),
            wet: 0.0,
            heard: Heard::default(),
        }
        .with_env_shift()
    }

    fn with_env_shift(mut self) -> Self {
        self.shift = shift_from_env();
        self
    }

    /// What the soundscape should hear this frame (one-off sounds are
    /// handed over once).
    pub fn heard(&mut self) -> Heard {
        let h = &mut self.heard;
        Heard {
            whirr: h.whirr,
            spin: h.spin,
            rain: h.rain,
            puffs: std::mem::take(&mut h.puffs),
            thunder: std::mem::take(&mut h.thunder),
            bow: h.bow.take(),
        }
    }

    /// How far the camera is under the dark cloud (0..1): the sky clouds
    /// over and the sun goes in.
    pub fn wet(&self) -> f32 {
        self.wet
    }

    /// The weather now, if the town has a tower.
    pub fn shower(&self) -> Option<Shower> {
        self.tower.as_ref().map(|_| self.now)
    }

    /// Is the live part made (for the memory checks and tests)?
    pub fn is_live(&self) -> bool {
        self.live.is_some()
    }

    fn spawn(world: &mut World, l: &Landmark, p: &Parts) -> EngineResult<Live> {
        let d = WxDims::of(l.height);
        let origin = Vec3::new(l.pos.x as f32, l.base as f32, l.pos.y as f32);
        let yaw = l.dir.x.atan2(l.dir.y).to_degrees() as f32;
        let make = |world: &mut World, mesh: &Mesh, shadow: bool| -> EngineResult<EntityId> {
            let id = world.spawn_instanced(mesh.clone());
            world.set_casts_shadow(id, shadow)?;
            Ok(id)
        };
        let globe_at = origin + Vec3::Y * d.globe_y as f32;
        Ok(Live {
            origin,
            yaw,
            d,
            anemo: make(world, &p.anemo, true)?,
            needle: make(world, &p.needle, false)?,
            globe: make(world, &p.globe, false)?,
            mouths: make(world, &p.mouths, false)?,
            curtain: make(world, &p.curtain, false)?,
            bow: make(world, &p.bow, false)?,
            lamp_globe: world.add_lamp(Lamp::point(globe_at, rgb(255, 210, 120), 30.0, 40.0))?,
            lamp_flash: world.add_lamp(Lamp::point(globe_at, rgb(220, 230, 255), 0.0, 10.0))?,
            rain: Vec::new(),
            near: None,
            tier: 0,
        })
    }

    pub fn clear(&mut self, world: &mut World) {
        if let Some(mut live) = self.live.take() {
            for id in [
                live.anemo,
                live.needle,
                live.globe,
                live.mouths,
                live.curtain,
                live.bow,
            ] {
                world.despawn(id);
            }
            world.remove_lamp(live.lamp_globe);
            world.remove_lamp(live.lamp_flash);
            Self::stop_rain(world, &mut live);
        }
        self.heard = Heard::default();
        self.wet = 0.0;
    }

    fn stop_rain(world: &mut World, live: &mut Live) {
        let p = world.particles_mut();
        for id in live.rain.drain(..).chain(live.near.take()) {
            let _ = p.stop(id);
        }
        live.tier = 0;
    }

    /// A frame: `seconds` is the game clock, `hour` the time of day, and
    /// `sheltered` says the camera is indoors or underground (no rain
    /// falls there).
    pub fn update(
        &mut self,
        world: &mut World,
        seconds: f64,
        hour: f32,
        dt: f32,
        sheltered: bool,
    ) -> EngineResult<()> {
        let Some(l) = self.tower.clone() else {
            return Ok(());
        };
        if self.parts.is_none() {
            // Wait for the worker: the shapes are in, or nothing shows.
            if let Some(rx) = &self.pending {
                if let Ok(p) = rx.try_recv() {
                    self.parts = Some(p);
                    self.pending = None;
                }
            }
        }
        self.now = shower(seconds + self.shift);
        let eye = world.camera().eye();
        let foot = Vec3::new(l.pos.x as f32, l.base as f32, l.pos.y as f32);
        let far = foot.distance(eye);
        if far > DROP {
            self.clear(world);
            return Ok(());
        }
        let Some(parts) = &self.parts else {
            return Ok(());
        };
        if self.live.is_none() {
            if far > LOAD {
                return Ok(());
            }
            self.live = Some(Self::spawn(world, &l, parts)?);
        }
        let dt = dt.clamp(0.0, 0.1);
        let mut live = self.live.take().expect("live");
        let drawn = self.draw(world, &mut live, &l, eye, hour, dt, sheltered);
        self.live = Some(live);
        drawn
    }

    /// The tower's world point at `local` (its own frame).
    fn world_at(live: &Live, local: DVec3) -> Vec3 {
        let a = (live.yaw as f64).to_radians();
        let (s, c) = a.sin_cos();
        let x = local.x * c + local.z * s;
        let z = -local.x * s + local.z * c;
        live.origin + Vec3::new(x as f32, local.y as f32, z as f32)
    }

    #[allow(clippy::too_many_arguments)]
    fn draw(
        &mut self,
        world: &mut World,
        live: &mut Live,
        l: &Landmark,
        eye: Vec3,
        hour: f32,
        dt: f32,
        sheltered: bool,
    ) -> EngineResult<()> {
        let s = self.now;
        let d = live.d;
        let day = 1.0 - crate::lighting::night(hour);
        let tint = |r: f32, g: f32, b: f32, a: f32| Color { r, g, b, a };
        let at = || Place::new(live.origin.x, live.origin.y, live.origin.z).with_yaw_deg(live.yaw);
        let centre2 = rain_centre(l);
        let centre = Vec3::new(centre2.x as f32, l.base as f32, centre2.y as f32);
        let cloud_y = live.origin.y + d.cloud_y() as f32;
        // How far the camera is in under the shower.
        let flat = DVec2::new(eye.x as f64, eye.z as f64).distance(centre2);
        let inside = 1.0 - smooth(WX_RAIN_R * 0.75, WX_RAIN_R * 1.25, flat) as f32;
        self.wet = inside * (s.dark * s.cloud) as f32;

        // The anemometer: idle in fair weather, racing in the rain.
        let spin_rate = 90.0 + 520.0 * (s.rain as f32).max(0.6 * s.dark as f32);
        self.spin = (self.spin + spin_rate * dt).rem_euclid(360.0);
        let hub = Self::world_at(live, DVec3::Y * d.vane_y);
        world.set_instances(
            live.anemo,
            &[Place::new(hub.x, hub.y, hub.z).with_yaw_deg(live.yaw + self.spin)],
        )?;
        // The needle: up-left (FAIR) to up-right (RAIN), easing; a tremble
        // in the storm.
        let want = 38.0 - 76.0 * (s.dark.max(s.rain)) as f32
            + 2.5 * (self.spin.to_radians() * 3.0).sin() * s.rain as f32;
        self.needle += (want - self.needle) * (dt * 1.5).min(1.0);
        let (dc, _) = dial(&d);
        let pivot = Self::world_at(live, dc);
        world.set_instances(
            live.needle,
            &[Place::new(pivot.x, pivot.y, pivot.z)
                .with_yaw_deg(live.yaw)
                .with_roll_deg(self.needle)],
        )?;
        // The globe: gold in fair weather, cool blue in the rain, every
        // colour of the bow while it shows.
        let wetness = s.dark.max(s.rain) as f32;
        let (mut gr, mut gg, mut gb) = (
            1.0 + (0.35 - 1.0) * wetness,
            0.78 + (0.6 - 0.78) * wetness,
            0.35 + (1.0 - 0.35) * wetness,
        );
        if s.bow > 0.0 {
            let h = (self.spin / 60.0).rem_euclid(6.0);
            let (r, g, b) = hue(h);
            let k = s.bow as f32;
            gr += (r - gr) * k * 0.7;
            gg += (g - gg) * k * 0.7;
            gb += (b - gb) * k * 0.7;
        }
        let pulse = 1.0 + 0.08 * (self.spin.to_radians() * 0.5).sin();
        let gk = (1.1 + 0.6 * (1.0 - day)) * pulse;
        world.set_instances(
            live.globe,
            &[at().with_tint(tint(gr * gk, gg * gk, gb * gk, 1.0))],
        )?;
        let globe_at = Self::world_at(live, DVec3::Y * d.globe_y);
        world.set_lamp(
            live.lamp_globe,
            Lamp::point(
                globe_at,
                Color::rgb01_unchecked(gr.min(1.0), gg.min(1.0), gb.min(1.0)),
                25.0 + 35.0 * (1.0 - day),
                45.0,
            ),
        )?;

        // The cannons: while brewing, one after another round the crown,
        // each with a whoomp of vapour into the sky.
        self.shot = (self.shot - dt).max(0.0);
        if s.spray {
            self.cannon_in -= dt;
            if self.cannon_in <= 0.0 {
                self.cannon_in = self.rng.range(0.8, 1.5);
                let k = self.next_cannon;
                self.next_cannon = (k + 1 + (self.rng.f() * 2.0) as usize) % WX_CANNONS;
                let (base, axis) = cannon(&d, k);
                let mouth = Self::world_at(live, base + axis * CANNON_LEN);
                let dir = Self::world_at(live, base + axis * (CANNON_LEN + 1.0)) - mouth;
                world.particles_mut().burst(
                    ParticleEmitter::new(mouth)
                        .with_mode(ParticleMode::Burst)
                        .with_burst_count(36)
                        .with_silhouette(ParticleSilhouette::Puff)
                        .with_blend(ParticleBlend::Alpha)
                        .with_shape(ParticleShape::Sphere)
                        .with_spread(Vec3::splat(0.5))
                        .with_velocity(dir * 20.0)
                        .with_velocity_jitter(Vec3::new(2.5, 3.5, 2.5))
                        .with_acceleration(Vec3::new(0.4, 1.2, 0.2))
                        .with_lifetime(7.0)
                        .with_size(2.4)
                        .with_size_variation(0.45)
                        .with_size_over_life(SizeOverLife::Expand)
                        .with_color(Color::rgb01_unchecked(0.96, 0.97, 1.0).with_alpha(0.75))
                        .with_secondary_color(
                            Color::rgb01_unchecked(0.9, 0.92, 0.96).with_alpha(0.2),
                        )
                        .with_turbulence(0.6)
                        .with_drag(0.55)
                        .with_spin(0.4),
                )?;
                self.shot = 0.45;
                self.heard.puffs.push(mouth);
            }
        } else {
            self.cannon_in = 0.0;
        }
        let mk = 0.35 + 0.25 * (1.0 - day) + 2.2 * self.shot + 0.3 * s.spray as u8 as f32;
        world.set_instances(
            live.mouths,
            &[at().with_tint(tint(mk * 0.8, mk * 0.95, mk, 1.0))],
        )?;

        // The cloud: soft puffs sent a few at a time, white while it
        // gathers, grey as it fills with rain, white again as it clears.
        self.cloud_in -= dt;
        if s.cloud > 0.02 && self.cloud_in <= 0.0 {
            self.cloud_in = 0.3;
            // A flat dark base, billows piling up above it, lighter as
            // they climb into the sun.
            let reach = (WX_RAIN_R * (0.55 + 0.65 * s.cloud)) as f32;
            for (dy, spread, n, size, lift) in [
                (0.0, Vec3::new(reach, 2.5, reach), 5.0, 18.0, 0.0),
                (
                    12.0,
                    Vec3::new(reach * 0.7, 6.0, reach * 0.7),
                    4.0,
                    21.0,
                    0.25,
                ),
                (
                    26.0,
                    Vec3::new(reach * 0.4, 7.0, reach * 0.4),
                    2.0,
                    22.0,
                    0.45,
                ),
            ] {
                let grey = (0.97 - 0.55 * s.dark as f32 * (1.0 - lift)).min(1.0);
                let c = Color::rgb01_unchecked(grey * 0.97, grey * 0.98, grey);
                // Fewer puffs as the cloud thins (a burst now and then).
                let want = n * s.cloud as f32;
                let count = want.floor() as u32 + (self.rng.f() < want.fract()) as u32;
                if count == 0 {
                    continue;
                }
                world.particles_mut().burst(
                    ParticleEmitter::new(centre + Vec3::Y * (cloud_y - centre.y + 8.0 + dy))
                        .with_mode(ParticleMode::Burst)
                        .with_burst_count(count)
                        .with_silhouette(ParticleSilhouette::Puff)
                        .with_blend(ParticleBlend::Alpha)
                        .with_shape(ParticleShape::Sphere)
                        .with_spread(spread)
                        .with_velocity(Vec3::new(0.6, 0.0, 0.3))
                        .with_velocity_jitter(Vec3::new(0.6, 0.25, 0.6))
                        .with_lifetime(PUFF_LIFE)
                        .with_size(size * (0.7 + 0.3 * s.cloud as f32))
                        .with_size_variation(0.4)
                        .with_size_over_life(SizeOverLife::FadeInOut)
                        .with_color(c.with_alpha(0.85))
                        .with_secondary_color(c.with_alpha(0.55))
                        .with_turbulence(0.15)
                        .with_drag(0.1)
                        .with_spin(0.05),
                )?;
            }
        }

        // Rain: streaks over the whole shower, thicker round the camera
        // when it stands in it; none indoors or underground.
        let tier = if sheltered || s.rain < 0.12 {
            0
        } else if s.rain < 0.6 {
            1
        } else {
            2
        };
        if tier != live.tier {
            Self::stop_rain(world, live);
            if tier > 0 {
                let fall = (cloud_y - centre.y + 20.0) / DROP_SPEED;
                let rate = if tier == 2 { 2400.0 } else { 900.0 };
                let id = world.particles_mut().start(
                    rain_drops(centre.with_y(cloud_y), WX_RAIN_R as f32 * 0.8, rate, fall)
                        .with_seed(11),
                )?;
                live.rain.push(id);
                if inside > 0.0 {
                    let id = world.particles_mut().start(
                        rain_drops(eye + Vec3::Y * 20.0, 26.0, rate * 1.6, 40.0 / DROP_SPEED)
                            .with_seed(12),
                    )?;
                    live.near = Some(id);
                }
            }
            live.tier = tier;
        }
        if let Some(id) = live.near {
            if inside > 0.3 {
                world
                    .particles_mut()
                    .set_emitter_position(id, eye + Vec3::Y * 20.0)?;
            } else {
                let _ = world.particles_mut().stop(id);
                live.near = None;
            }
        } else if tier > 0 && inside > 0.3 {
            let rate = if tier == 2 { 2400.0 } else { 900.0 };
            let id = world.particles_mut().start(
                rain_drops(eye + Vec3::Y * 20.0, 26.0, rate * 1.6, 40.0 / DROP_SPEED).with_seed(12),
            )?;
            live.near = Some(id);
        }
        // The curtain under the cloud, for the view from outside it.
        let veil = (s.rain as f32) * (1.0 - 0.6 * inside);
        let curtain = if veil > 0.01 {
            vec![Place::new(centre.x, centre.y, centre.z)
                .with_yaw_deg(self.spin * 0.02)
                .with_tint(tint(1.0, 1.0, 1.0, veil))]
        } else {
            Vec::new()
        };
        world.set_instances(live.curtain, &curtain)?;

        // Thunder now and then in the heart of the storm: a flash in the
        // cloud lighting the street, the rumble after it.
        self.flash = (self.flash - dt).max(0.0);
        if s.rain > 0.6 {
            self.flash_in -= dt;
            if self.flash_in <= 0.0 {
                self.flash_in = self.rng.range(6.0, 15.0);
                self.flash = 0.35;
                let a = self.rng.range(0.0, std::f32::consts::TAU);
                let r = self.rng.range(0.0, WX_RAIN_R as f32 * 0.6);
                self.flash_at = centre.with_y(cloud_y + 4.0) + Vec3::new(a.sin(), 0.0, a.cos()) * r;
                world.particles_mut().burst(
                    ParticleEmitter::new(self.flash_at)
                        .with_mode(ParticleMode::Burst)
                        .with_burst_count(4)
                        .with_silhouette(ParticleSilhouette::SoftOrb)
                        .with_blend(ParticleBlend::Additive)
                        .with_shape(ParticleShape::Sphere)
                        .with_spread(Vec3::new(14.0, 4.0, 14.0))
                        .with_lifetime(0.3)
                        .with_size(26.0)
                        .with_size_over_life(SizeOverLife::FadeInOut)
                        .with_color(Color::rgb01_unchecked(0.75, 0.8, 1.0).with_alpha(0.5))
                        .with_secondary_color(
                            Color::rgb01_unchecked(0.6, 0.65, 1.0).with_alpha(0.2),
                        )
                        .with_emissive_intensity(2.5),
                )?;
                self.heard.thunder.push(self.flash_at);
            }
        }
        // A double flicker: bright, a dip, bright again.
        let f = self.flash;
        let flick = if f > 0.22 || (0.08..0.16).contains(&f) {
            1.0
        } else {
            0.0
        };
        world.set_lamp(
            live.lamp_flash,
            Lamp::point(
                self.flash_at - Vec3::Y * 12.0,
                rgb(215, 225, 255),
                3500.0 * flick,
                220.0,
            ),
        )?;

        // The rainbow: in the sun after the rain, standing beyond the
        // shower and turned to whoever looks at it.
        let bow = (s.bow as f32) * day.powf(1.5);
        let places = if bow > 0.01 {
            let mut away = DVec2::new(
                centre.x as f64 - eye.x as f64,
                centre.z as f64 - eye.z as f64,
            );
            if away.length() < 1.0 {
                away = l.dir;
            }
            let away = away.normalize();
            let p = centre2 + away * BOW_BEYOND;
            let yaw = away.x.atan2(away.y).to_degrees() as f32;
            vec![Place::new(p.x as f32, centre.y - 14.0, p.y as f32)
                .with_yaw_deg(yaw)
                .with_tint(tint(1.0, 1.0, 1.0, bow))]
        } else {
            Vec::new()
        };
        world.set_instances(live.bow, &places)?;
        let showing = bow > 0.3;
        if showing && !self.bow_was {
            self.heard.bow = Some(centre.with_y(centre.y + 30.0));
        }
        self.bow_was = showing;

        // What the soundscape hears.
        self.heard.whirr = Some(Self::world_at(live, DVec3::Y * d.crown_y));
        self.heard.spin = (spin_rate - 90.0) / 520.0;
        self.heard.rain = (s.rain > 0.02).then(|| {
            let e = DVec2::new(eye.x as f64, eye.z as f64);
            let toward = e - centre2;
            let p = if toward.length() <= WX_RAIN_R {
                e
            } else {
                centre2 + toward.normalize() * WX_RAIN_R
            };
            let y = if toward.length() <= WX_RAIN_R {
                eye.y
            } else {
                centre.y + 3.0
            };
            (Vec3::new(p.x as f32, y, p.y as f32), s.rain as f32)
        });
        Ok(())
    }
}

/// Falling rain from a disc of radius `r` round `at`.
fn rain_drops(at: Vec3, r: f32, rate: f32, life: f32) -> ParticleEmitter {
    ParticleEmitter::new(at)
        .with_mode(ParticleMode::Continuous)
        .with_rate(rate)
        .with_silhouette(ParticleSilhouette::SparkStreak)
        .with_blend(ParticleBlend::Alpha)
        .with_shape(ParticleShape::Sphere)
        .with_spread(Vec3::new(r, 1.0, r))
        .with_velocity(Vec3::new(0.6, -DROP_SPEED, 0.3))
        .with_velocity_jitter(Vec3::new(0.2, 1.5, 0.2))
        .with_lifetime(life.max(0.5))
        .with_size(0.022)
        .with_size_variation(0.3)
        .with_size_over_life(SizeOverLife::Constant)
        .with_velocity_stretch(0.045)
        .with_color(Color::rgb01_unchecked(0.72, 0.78, 0.86).with_alpha(0.55))
        .with_secondary_color(Color::rgb01_unchecked(0.8, 0.85, 0.92).with_alpha(0.4))
        .with_lighting(0.8)
        .with_turbulence(0.0)
        .with_drag(0.0)
}

fn smooth(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A bright hue round the colour wheel (`h` 0..6).
fn hue(h: f32) -> (f32, f32, f32) {
    let x = 1.0 - ((h % 2.0) - 1.0).abs();
    match h as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tower() -> Landmark {
        Landmark {
            kind: LandmarkKind::WeatherTower,
            pos: DVec2::ZERO,
            plaza: 16.0,
            base: 3.0,
            low: 1.0,
            height: 52.0,
            dir: DVec2::Y,
            stairs: Vec::new(),
        }
    }

    #[test]
    fn every_part_has_faces_and_the_veils_are_see_through() {
        let p = parts(&tower());
        for m in [&p.anemo, &p.needle, &p.globe, &p.mouths, &p.curtain, &p.bow] {
            assert!(m.face_count() > 20, "{}", m.face_count());
        }
        for m in [&p.curtain, &p.bow] {
            let b = m.build();
            assert_eq!(b.opaque_index_count, 0, "a veil drawn solid");
        }
        // One NaN in a see-through colour blows the whole picture out.
        for m in [&p.anemo, &p.needle, &p.globe, &p.mouths, &p.curtain, &p.bow] {
            let b = m.build();
            assert!(
                b.colors.iter().all(|c| c.is_finite()),
                "a colour that is not a number"
            );
            assert!(b.positions.iter().all(|v| v.is_finite()));
        }
    }

    #[test]
    #[ignore = "on demand (looks): scripts/check.sh --on-demand"]
    fn hues_go_round() {
        assert_eq!(hue(0.0), (1.0, 0.0, 0.0));
        assert_eq!(hue(2.0), (0.0, 1.0, 0.0));
        assert_eq!(hue(4.0), (0.0, 0.0, 1.0));
    }
}
