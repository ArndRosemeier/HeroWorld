//! The mad scientist's quarter, alive: smoke rolling off his stacks,
//! searchlights sweeping the sky, patrol drones circling overhead with a
//! green beam on the street, sparks crackling off the coils on his labs,
//! his posters on the piers and, after dark, red and green pools of light
//! (fixtures from [`hw_gen::urban::menace`], their fixed parts drawn with
//! the buildings by [`crate::menacemesh`]).
//!
//! The quarter's fixtures are gathered once, on a worker thread (it plans
//! the quarter's units), with the models; after that a frame only moves
//! instances, starts or stops a few emitters and lamps near the camera,
//! and respawns the near posters when the camera has moved.

use crate::menacemesh::{self, c3, AETHER, BEAM_LEN, WARNING};
use engine::prelude::*;
use glam::{DVec2, DVec3, Vec3};
use hw_gen::urban::menace::{self, POSTER_IMAGES};
use hw_gen::urban::world::TownWorld;
use std::collections::BTreeMap;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};

/// Searchlights, stacks and drones this near the quarter's middle are
/// shown (m): further, the quarter is a smudge on the horizon.
const SHOW: f32 = 2200.0;
/// Searchlights at most.
const LIGHTS_MOST: usize = 8;
/// Smoke from the stacks this near the camera (m), at most this many.
const SMOKE_REACH: f32 = 1100.0;
const SMOKE_MOST: usize = 12;
/// Coils that crackle: this near, at most this many.
const ARC_REACH: f32 = 350.0;
const ARC_MOST: usize = 4;
/// Arc shapes made per dome size.
const ARC_SHAPES: u64 = 5;
/// Posters this near the camera (m), at most this many, picked again when
/// the camera has moved this far.
const POSTER_REACH: f32 = 130.0;
const POSTER_MOST: usize = 50;
const POSTER_STEP: f32 = 20.0;
/// Night lamps: speakers' red lamps and the coils' green glow this near,
/// and the drones' beams this near.
const LAMP_REACH: f32 = 260.0;
const LAMP_MOST: usize = 10;
const DRONE_LAMPS: usize = 3;
/// A drone's beam down onto the street (m).
const DRONE_BEAM: f32 = 70.0;

const POSTERS: [&[u8]; POSTER_IMAGES as usize] = [
    include_bytes!("../assets/posters/obey.png"),
    include_bytes!("../assets/posters/power.png"),
    include_bytes!("../assets/posters/eye.png"),
    include_bytes!("../assets/posters/heroes.png"),
];

/// The posters' decal kinds: added to the World once per run.
static KINDS: Mutex<Option<Vec<DecalKind>>> = Mutex::new(None);

fn kinds(world: &mut World) -> Option<Vec<DecalKind>> {
    let mut k = KINDS.lock().ok()?;
    if k.is_none() {
        let mut out = Vec::new();
        for bytes in POSTERS {
            let img = image::load_from_memory(bytes).ok()?.to_rgba8();
            let (w, h) = img.dimensions();
            out.push(
                world
                    .add_decal_image(img.as_raw(), w as usize, h as usize, 0.9)
                    .ok()?,
            );
        }
        *k = Some(out);
    }
    k.clone()
}

/// A searchlight in the town.
#[derive(Clone, Copy, Debug)]
struct Light {
    pivot: Vec3,
    phase: f32,
    speed: f32,
}

/// A poster in the town.
#[derive(Clone, Copy, Debug)]
struct PosterAt {
    at: Vec3,
    normal: Vec3,
    size: f32,
    image: u8,
    angle: f32,
}

/// A drone's beat: a lazy loop round a point of the quarter.
#[derive(Clone, Copy, Debug)]
struct Patrol {
    centre: DVec2,
    radius: f64,
    height: f64,
    /// Radians a second round the loop (the sign says which way).
    rate: f64,
    phase: f64,
}

impl Patrol {
    /// Where the drone is at `t`, which way it heads (yaw, degrees) and
    /// how it banks (roll, degrees).
    fn at(&self, t: f64) -> (Vec3, f32, f32) {
        let a = self.phase + self.rate * t;
        let r = self.radius;
        let pos = |a: f64| {
            self.centre
                + DVec2::new(a.cos(), a.sin()) * r
                + DVec2::new((2.0 * a).sin(), 0.0) * r * 0.25
        };
        let p = pos(a);
        let ahead = pos(a + 0.05 * self.rate.signum());
        let d = ahead - p;
        let yaw = d.x.atan2(d.y).to_degrees();
        let bob = (t * 0.8 + self.phase * 3.0).sin() * 1.2;
        let roll = -self.rate.signum() * 12.0;
        (
            Vec3::new(p.x as f32, (self.height + bob) as f32, p.y as f32),
            yaw as f32,
            roll as f32,
        )
    }
}

/// Everything in the quarter, in the town, and the models: the worker's
/// result.
struct Built {
    centre: Vec3,
    stacks: Vec<(Vec3, f32)>,
    lights: Vec<Light>,
    /// Which of them shine.
    lit: Vec<bool>,
    /// A coil's top load, and its dome's arc bucket.
    coils: Vec<(Vec3, u32)>,
    speakers: Vec<Vec3>,
    posters: Vec<PosterAt>,
    drones: Vec<Patrol>,
    head: Vec<(Mesh, f32)>,
    beam: Mesh,
    drone: Vec<(Mesh, f32)>,
    drone_beam: Mesh,
    /// Per bucket, its arc shapes.
    arcs: BTreeMap<u32, Vec<Mesh>>,
}

fn gather(town: &TownWorld) -> Option<Built> {
    let d = town.town.dystopia.clone()?;
    let reach = DVec2::splat(d.radius + 30.0);
    let mut b = Built {
        centre: Vec3::new(
            d.centre.x as f32,
            town.ground(d.centre) as f32,
            d.centre.y as f32,
        ),
        stacks: Vec::new(),
        lights: Vec::new(),
        lit: Vec::new(),
        coils: Vec::new(),
        speakers: Vec::new(),
        posters: Vec::new(),
        drones: Vec::new(),
        head: menacemesh::searchlight_levels(),
        beam: menacemesh::searchlight_beam(),
        drone: menacemesh::drone_levels(),
        drone_beam: menacemesh::drone_beam(),
        arcs: BTreeMap::new(),
    };
    let f32v = |p: DVec3| p.as_vec3();
    for q in town.quarters_in(d.centre - reach, d.centre + reach) {
        for (_, wb) in q.buildings_with_ids() {
            let site = &wb.site;
            if !menace::carries(site) {
                continue;
            }
            let f = menace::fixtures(site);
            let at = |p: DVec3| f32v(menace::Fixtures::to_world(site, wb.base, p));
            if let Some(s) = &f.stack {
                b.stacks.push((at(s.mouth()), s.top_r() as f32));
            }
            if let Some(l) = &f.light {
                b.lights.push(Light {
                    pivot: at(DVec3::new(l.at.x, l.head, l.at.y)),
                    phase: l.phase as f32,
                    speed: l.speed as f32,
                });
            }
            if let Some(c) = f.coil {
                let r = site.size.x.min(site.size.y) * menace::DOME_SHARE;
                let k = menacemesh::arc_bucket(r);
                b.coils.push((at(c), k));
                b.arcs.entry(k).or_insert_with(|| {
                    (0..ARC_SHAPES)
                        .map(|i| menacemesh::coil_arc(menacemesh::bucket_r(k), i + k as u64 * 16))
                        .collect()
                });
            }
            if let Some(s) = &f.speakers {
                b.speakers
                    .push(f32v(menacemesh::speaker_lamp(site, wb.base, s)));
            }
            let normal = -DVec3::new(site.inward.x, 0.0, site.inward.y);
            for p in &f.posters {
                b.posters.push(PosterAt {
                    at: at(DVec3::new(p.x, p.y, 0.0)),
                    normal: normal.as_vec3(),
                    size: p.size as f32,
                    image: p.image,
                    angle: p.angle as f32,
                });
            }
        }
    }
    // A handful of searchlights shine, spread over the quarter (all of them
    // would light it up like a fair); the rest stand dark, aimed at the sky.
    b.lit = spread(&b.lights, LIGHTS_MOST);
    // The patrols: loops of a few hundred metres over the rooftops.
    let mut rng = town.seed.child("drones").rng();
    let n = ((d.radius / 45.0) as usize).clamp(4, 10);
    for k in 0..n {
        let a = std::f64::consts::TAU * k as f64 / n as f64 + rng.range(-0.3, 0.3);
        let off = d.radius * rng.range(0.1, 0.55);
        let centre = d.centre + DVec2::new(a.cos(), a.sin()) * off;
        let radius = (d.radius * rng.range(0.2, 0.45)).max(40.0);
        let speed = rng.range(9.0, 15.0);
        b.drones.push(Patrol {
            centre,
            radius,
            height: town.ground(centre) + rng.range(48.0, 72.0),
            rate: speed / radius * if rng.chance(0.5) { 1.0 } else { -1.0 },
            phase: rng.range(0.0, std::f64::consts::TAU),
        });
    }
    Some(b)
}

/// At most `n` of `lights`, as far apart as can be (each next one the
/// farthest from those picked; the order they came in breaks ties, so the
/// same quarter always gets the same ones).
fn spread(lights: &[Light], n: usize) -> Vec<bool> {
    let mut lit = vec![lights.len() <= n; lights.len()];
    if lights.len() <= n {
        return lit;
    }
    lit[0] = true;
    for _ in 1..n {
        let far = |i: usize| {
            (0..lights.len())
                .filter(|&k| lit[k])
                .map(|k| lights[k].pivot.distance(lights[i].pivot))
                .fold(f32::MAX, f32::min)
        };
        let next = (0..lights.len())
            .filter(|&i| !lit[i])
            .fold((-1.0f32, 0), |a, i| {
                let d = far(i);
                if d > a.0 {
                    (d, i)
                } else {
                    a
                }
            });
        lit[next.1] = true;
    }
    lit
}

/// The Engine side of it.
struct Live {
    heads: LodId,
    beams: EntityId,
    drones: LodId,
    drone_beams: EntityId,
    arcs: BTreeMap<u32, Vec<EntityId>>,
}

/// An arc crackling on a coil.
#[derive(Clone, Copy, Default)]
struct Spark {
    coil: usize,
    shape: usize,
    yaw: f32,
    life: f32,
    hop: f32,
}

pub struct MenaceView {
    job: Option<Receiver<Option<Built>>>,
    built: Option<Built>,
    live: Option<Live>,
    clock: f64,
    rng: u64,
    smoke: BTreeMap<usize, EmitterId>,
    lamps: Vec<LampId>,
    posters: Vec<DecalId>,
    posters_at: Option<Vec3>,
    arcs: Vec<Spark>,
}

/// A linear colour with alpha (particles take linear colours).
fn lin(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color { r, g, b, a }
}

/// Dark coal smoke from a stack's mouth.
fn smoke(at: Vec3, r: f32) -> ParticleEmitter {
    ParticleEmitter::smoke(at, r * 2.4)
        .with_rate(9.0)
        .with_velocity(Vec3::new(0.9, 3.4, 0.4))
        .with_lifetime(22.0)
        .with_color(lin(0.08, 0.08, 0.07, 0.6))
        .with_secondary_color(lin(0.22, 0.24, 0.2, 0.18))
}

impl MenaceView {
    pub fn new(town: &Arc<TownWorld>) -> Self {
        let job = town.town.dystopia.as_ref().map(|_| {
            let (tx, rx) = channel();
            let town = town.clone();
            std::thread::Builder::new()
                .name("villain quarter".into())
                .spawn(move || {
                    let _ = tx.send(gather(&town));
                })
                .expect("a thread for the villain quarter");
            rx
        });
        Self {
            job,
            built: None,
            live: None,
            clock: 0.0,
            rng: 0x6d2b_79f5_1f12_3a45,
            smoke: BTreeMap::new(),
            lamps: Vec::new(),
            posters: Vec::new(),
            posters_at: None,
            arcs: vec![Spark::default(); ARC_MOST * 2],
        }
    }

    /// Waits for the worker (screenshots and tests, where the first frame
    /// has to show it).
    pub fn wait(&mut self) {
        if let Some(rx) = self.job.take() {
            self.built = rx.recv().ok().flatten();
        }
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Is the live part made (for memory checks)?
    pub fn is_live(&self) -> bool {
        self.live.is_some()
    }

    pub fn update(&mut self, world: &mut World, dt: f32, hour: f32) -> EngineResult<()> {
        if let Some(rx) = &self.job {
            match rx.try_recv() {
                Ok(b) => {
                    self.built = b;
                    self.job = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return Ok(()),
                Err(_) => self.job = None,
            }
        }
        let Some(built) = self.built.take() else {
            return Ok(());
        };
        let out = self.step(world, &built, dt, hour);
        self.built = Some(built);
        out
    }

    fn step(&mut self, world: &mut World, b: &Built, dt: f32, hour: f32) -> EngineResult<()> {
        let eye = world.camera().eye();
        let flat = |p: Vec3| Vec3::new(p.x - eye.x, 0.0, p.z - eye.z).length();
        if flat(b.centre) > SHOW + 300.0 {
            self.clear(world);
            return Ok(());
        }
        if self.live.is_none() {
            self.live = Some(Self::spawn(world, b)?);
        }
        self.clock += dt.clamp(0.0, 0.1) as f64;
        let t = self.clock;
        let dark = crate::nightlights::dark_at(hour);
        // Beams are faint by day and blaze at night.
        let glow = if dark { 1.0 } else { 0.05 };
        let tint = |k: f32| Color::rgb01_unchecked(k, k, k);
        let live = self.live.as_ref().expect("live");
        // Searchlights: turning, nodding.
        let mut heads = Vec::with_capacity(b.lights.len());
        let mut beams = Vec::with_capacity(b.lights.len());
        for (l, &on) in b.lights.iter().zip(&b.lit) {
            let p = Place::new(l.pivot.x, l.pivot.y, l.pivot.z);
            if !on {
                // Dark, parked looking up.
                heads.push(p.with_yaw_deg(l.phase.to_degrees()).with_pitch_deg(-60.0));
                continue;
            }
            let yaw = (l.phase + l.speed * t as f32).to_degrees();
            let pitch = -(42.0 + 14.0 * (0.31 * t as f32 + l.phase * 2.0).sin());
            let p = p.with_yaw_deg(yaw).with_pitch_deg(pitch);
            heads.push(p);
            beams.push(p.with_scale(BEAM_LEN as f32).with_tint(tint(glow)));
        }
        world.set_lod_places(live.heads, &heads)?;
        world.set_instances(live.beams, &beams)?;
        // Drones on their beats, each with its beam swinging over the street.
        let mut drones = Vec::with_capacity(b.drones.len());
        let mut dbeams = Vec::with_capacity(b.drones.len());
        let mut drone_at = Vec::with_capacity(b.drones.len());
        for (k, d) in b.drones.iter().enumerate() {
            let (p, yaw, roll) = d.at(t);
            drones.push(
                Place::new(p.x, p.y, p.z)
                    .with_yaw_deg(yaw)
                    .with_roll_deg(roll),
            );
            let sweep = 25.0 * (0.5 * t as f32 + k as f32).sin();
            let pitch = 68.0;
            let q = Place::new(p.x, p.y - 0.5, p.z)
                .with_yaw_deg(yaw + sweep)
                .with_pitch_deg(pitch);
            dbeams.push(q.with_scale(DRONE_BEAM).with_tint(tint(glow)));
            let a = (yaw + sweep).to_radians();
            let pr = pitch.to_radians();
            let dir = Vec3::new(a.sin() * pr.cos(), -pr.sin(), a.cos() * pr.cos());
            drone_at.push((p, dir));
        }
        world.set_lod_places(live.drones, &drones)?;
        world.set_instances(live.drone_beams, &dbeams)?;
        // Arcs on the near coils.
        self.crackle(world, b, eye, dt)?;
        // Smoke off the near stacks.
        let mut near: Vec<(f32, usize)> = b
            .stacks
            .iter()
            .enumerate()
            .map(|(i, (m, _))| (m.distance(eye), i))
            .filter(|(d, _)| *d < SMOKE_REACH)
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.truncate(SMOKE_MOST);
        let keep: Vec<usize> = near.iter().map(|(_, i)| *i).collect();
        let gone: Vec<usize> = self
            .smoke
            .keys()
            .copied()
            .filter(|i| !keep.contains(i))
            .collect();
        for i in gone {
            if let Some(id) = self.smoke.remove(&i) {
                world.particles_mut().stop(id)?;
            }
        }
        for i in keep {
            if let std::collections::btree_map::Entry::Vacant(e) = self.smoke.entry(i) {
                let (m, r) = b.stacks[i];
                e.insert(world.particles_mut().start(smoke(m, r))?);
            }
        }
        // Night: red and green pools.
        let mut want: Vec<Lamp> = Vec::new();
        if dark {
            let mut fixed: Vec<(f32, Lamp)> = b
                .speakers
                .iter()
                .map(|&p| (p, Lamp::point(p, c3(WARNING), 9.0, 14.0)))
                .chain(
                    b.coils
                        .iter()
                        .map(|&(p, _)| (p, Lamp::point(p, c3(AETHER), 30.0, 36.0).flicker(0.35))),
                )
                .map(|(p, l)| (p.distance(eye), l))
                .filter(|(d, _)| *d < LAMP_REACH)
                .collect();
            fixed.sort_by(|a, b| a.0.total_cmp(&b.0));
            want.extend(fixed.into_iter().take(LAMP_MOST).map(|(_, l)| l));
            let mut ds: Vec<(f32, Vec3, Vec3)> = drone_at
                .iter()
                .map(|&(p, d)| (p.distance(eye), p, d))
                .collect();
            ds.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (_, p, d) in ds.into_iter().take(DRONE_LAMPS) {
                want.push(Lamp::spot(
                    p - Vec3::Y * 0.6,
                    d,
                    22.0,
                    c3(AETHER),
                    90.0,
                    110.0,
                ));
            }
        }
        for (k, lamp) in want.iter().enumerate() {
            match self.lamps.get(k) {
                Some(&id) => world.set_lamp(id, *lamp)?,
                None => self.lamps.push(world.add_lamp(*lamp)?),
            }
        }
        for id in self.lamps.drain(want.len()..) {
            world.remove_lamp(id);
        }
        self.paste(world, b, eye);
        Ok(())
    }

    /// Sparks off the coils near the camera.
    fn crackle(&mut self, world: &mut World, b: &Built, eye: Vec3, dt: f32) -> EngineResult<()> {
        let mut near: Vec<(f32, usize)> = b
            .coils
            .iter()
            .enumerate()
            .map(|(i, (p, _))| (p.distance(eye), i))
            .filter(|(d, _)| *d < ARC_REACH)
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.truncate(ARC_MOST);
        for k in 0..self.arcs.len() {
            let mut a = self.arcs[k];
            a.life -= dt;
            a.hop -= dt;
            let alive = near.iter().any(|(_, i)| *i == a.coil);
            if a.life <= 0.0 || !alive {
                // A new arc now and then, on one of the near coils.
                if !near.is_empty() && self.rand() < dt * 2.5 {
                    let pick = ((self.rand() * near.len() as f32) as usize).min(near.len() - 1);
                    a.coil = near[pick].1;
                    a.life = 0.15 + self.rand() * 0.6;
                    a.hop = 0.0;
                } else {
                    a.life = 0.0;
                }
            }
            if a.life > 0.0 && a.hop <= 0.0 {
                a.shape = (self.rand() * ARC_SHAPES as f32) as usize % ARC_SHAPES as usize;
                a.yaw = self.rand() * 360.0;
                a.hop = 0.05 + self.rand() * 0.09;
            }
            self.arcs[k] = a;
        }
        let live = self.live.as_ref().expect("live");
        let mut places: BTreeMap<(u32, usize), Vec<Place>> = BTreeMap::new();
        for a in self.arcs.iter().filter(|a| a.life > 0.0) {
            let (p, k) = b.coils[a.coil];
            let bright = 0.7 + 0.6 * ((self.clock * 41.0 + a.yaw as f64).sin() as f32).abs();
            places.entry((k, a.shape)).or_default().push(
                Place::new(p.x, p.y, p.z)
                    .with_yaw_deg(a.yaw)
                    .with_tint(Color::rgb01_unchecked(bright, bright, bright)),
            );
        }
        for (k, ids) in &live.arcs {
            for (s, id) in ids.iter().enumerate() {
                let v = places.remove(&(*k, s)).unwrap_or_default();
                world.set_instances(*id, &v)?;
            }
        }
        Ok(())
    }

    /// The posters near the camera.
    fn paste(&mut self, world: &mut World, b: &Built, eye: Vec3) {
        if self
            .posters_at
            .is_some_and(|p| p.distance(eye) < POSTER_STEP)
        {
            return;
        }
        self.posters_at = Some(eye);
        let Some(kinds) = kinds(world) else {
            return;
        };
        for d in self.posters.drain(..) {
            world.remove_decal(d);
        }
        let mut near: Vec<(f32, &PosterAt)> = b
            .posters
            .iter()
            .map(|p| (p.at.distance(eye), p))
            .filter(|(d, _)| *d < POSTER_REACH)
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, p) in near.into_iter().take(POSTER_MOST) {
            let d = Decal::new(kinds[p.image as usize], p.at, p.normal, p.size)
                .depth(0.5)
                .angle(p.angle)
                .opacity(0.97);
            if let Ok(id) = world.spawn_decal(d) {
                self.posters.push(id);
            }
        }
    }

    fn spawn(world: &mut World, b: &Built) -> EngineResult<Live> {
        let quiet = |world: &mut World, id: EntityId| world.set_casts_shadow(id, false);
        let levels = |v: &[(Mesh, f32)]| -> Vec<LodLevel> {
            v.iter()
                .map(|(m, d)| LodLevel::new(m.clone(), *d))
                .collect()
        };
        let heads = world.spawn_lod_at(levels(&b.head), &[])?;
        let beams = world.spawn_instanced(b.beam.clone());
        quiet(world, beams)?;
        let drones = world.spawn_lod_at(levels(&b.drone), &[])?;
        let drone_beams = world.spawn_instanced(b.drone_beam.clone());
        quiet(world, drone_beams)?;
        let mut arcs = BTreeMap::new();
        for (k, shapes) in &b.arcs {
            let mut ids = Vec::new();
            for m in shapes {
                let id = world.spawn_instanced(m.clone());
                quiet(world, id)?;
                ids.push(id);
            }
            arcs.insert(*k, ids);
        }
        Ok(Live {
            heads,
            beams,
            drones,
            drone_beams,
            arcs,
        })
    }

    pub fn clear(&mut self, world: &mut World) {
        if let Some(live) = self.live.take() {
            world.despawn_lod(live.heads);
            world.despawn_lod(live.drones);
            world.despawn(live.beams);
            world.despawn(live.drone_beams);
            for id in live.arcs.values().flatten() {
                world.despawn(*id);
            }
        }
        for (_, id) in std::mem::take(&mut self.smoke) {
            let _ = world.particles_mut().stop(id);
        }
        for id in self.lamps.drain(..) {
            world.remove_lamp(id);
        }
        for d in self.posters.drain(..) {
            world.remove_decal(d);
        }
        self.posters_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "on demand (looks): scripts/check.sh --on-demand"]
    fn the_lit_searchlights_spread_out() {
        let at = |x: f32| Light {
            pivot: Vec3::new(x, 0.0, 0.0),
            phase: 0.0,
            speed: 0.3,
        };
        let lights: Vec<Light> = [0.0, 1.0, 2.0, 50.0, 51.0, 100.0].map(at).to_vec();
        let lit = spread(&lights, 3);
        assert_eq!(lit, vec![true, false, false, true, false, true]);
        assert!(spread(&lights, 10).iter().all(|l| *l));
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn a_drone_flies_its_loop_smoothly() {
        let p = Patrol {
            centre: DVec2::new(100.0, -50.0),
            radius: 80.0,
            height: 60.0,
            rate: 0.15,
            phase: 1.0,
        };
        let mut last = p.at(0.0).0;
        for i in 1..400 {
            let (q, yaw, _) = p.at(i as f64 * 0.1);
            assert!(q.distance(last) < 3.0, "a jump at {i}");
            assert!(yaw.is_finite());
            assert!((q.y - 60.0).abs() < 2.0);
            last = q;
        }
    }
}
