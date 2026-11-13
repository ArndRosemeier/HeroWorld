//! The mad scientist's machines on screen: the robo-sentry and the walking
//! tank, built from AssetGenerator parts (`assets/robots/*.glb`, made by
//! `tools/robots/build.py` from `assets/robots/specs`) hung on Engine
//! pivots, and walked with procedural gaits: the sentry swings its legs, the
//! colossus steps its four feet in turn with two-bone legs (the same knees
//! the server puts its weak points on, [`hw_sim::robots`]).
//!
//! A weak point that breaks blows its part off (an explosion, the part in
//! flying pieces, smoke from the stump after). The colossus's attacks are
//! telegraphed: a red ring where a shell will land, a ring round its feet
//! before a stomp, a green ring round the core before a discharge, a red
//! lane before it charges, a violet line before its Aether lance rakes it,
//! an orange ring on a wall before it rams the building.

use crate::fightview::{self, FightView};
use engine::prelude::*;
use glam::{DVec3, Vec3};
use hw_core::NetId;
use hw_net::{EntityKind, EntityState, Fx, FxKind, Status};
use hw_net::{EFFECT_ARMOURED, EFFECT_KNEELING};
use hw_sim::robots::{
    self as geo, foot_rest, hip, hull_height, knee, rotation_yaw, CANNON, DISH, SHIN, STEP_ORDER,
    STRIDE, THIGH, TRUNNION, TURRET_UP,
};
use std::collections::{BTreeMap, HashMap};
use std::f32::consts::PI;
use std::path::{Path, PathBuf};

/// Every part, by its file name.
const PARTS: [&str; 14] = [
    "sentry_body",
    "sentry_dish",
    "sentry_thigh",
    "sentry_shin",
    "sentry_arm",
    "sentry_gun",
    "colossus_hull",
    "colossus_turret",
    "colossus_cannon",
    "colossus_thigh",
    "colossus_shin",
    "colossus_knee",
    "colossus_shell",
    "colossus_core",
];

/// The colossus's gait: one cycle of all four feet, in metres walked.
const CYCLE: f32 = (STRIDE / 0.75) as f32;
/// The sentry's gait cycle (m).
const SENTRY_CYCLE: f32 = 1.5;
/// The sentry's hips over the ground.
const SENTRY_HIP: f32 = 1.2;

/// Where the robot parts are: `assets/robots` here, in the repo, or next
/// to the game.
pub fn robots_dir() -> Option<PathBuf> {
    let has = |p: &Path| p.join("colossus_hull.glb").is_file();
    let mut tries = vec![PathBuf::from("assets/robots")];
    tries.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/robots"));
    if let Ok(exe) = std::env::current_exe() {
        for up in exe.ancestors().skip(1).take(4) {
            tries.push(up.join("assets/robots"));
        }
    }
    tries.into_iter().find(|p| has(p))
}

/// The loaded parts: each one mesh per material, and once the world has
/// them, a LOD model per mesh that every robot's copy shares (meshes and
/// maps uploaded once, however many robots walk about).
struct Kit {
    parts: HashMap<&'static str, Vec<Mesh>>,
    models: HashMap<&'static str, Vec<LodId>>,
}

/// Camera distances (m) where a part's close, middle and far levels end:
/// the colossus is seen from across town, a sentry from down the street.
fn lod_ends(name: &str) -> [f32; 3] {
    if name.starts_with("colossus") {
        [70.0, 260.0, 2_000.0]
    } else {
        [28.0, 100.0, 600.0]
    }
}

/// Shares of the close level's triangles at the middle and far levels.
const LOD_SHARE: [f32; 2] = [0.22, 0.05];

impl Kit {
    /// The LOD models of every part, made the first time.
    fn ensure_models(&mut self, world: &mut World) -> EngineResult<()> {
        if !self.models.is_empty() {
            return Ok(());
        }
        for (name, meshes) in &self.parts {
            let ends = lod_ends(name);
            let mut ids = Vec::new();
            for m in meshes {
                let levels = vec![
                    LodLevel::new(m.clone(), ends[0]),
                    LodLevel::new(m.simplified(LOD_SHARE[0]), ends[1]),
                    LodLevel::new(m.simplified(LOD_SHARE[1]), ends[2]),
                ];
                ids.push(world.spawn_lod(levels)?);
            }
            self.models.insert(name, ids);
        }
        // The meshes now live in the world.
        self.parts.values_mut().for_each(Vec::clear);
        Ok(())
    }
}

impl Kit {
    fn load() -> Option<Kit> {
        let dir = robots_dir()?;
        let base = dir.canonicalize().ok()?;
        let mut parts = HashMap::new();
        for name in PARTS {
            let path = base.join(format!("{name}.glb"));
            match Model::load_scene_with(&path, &base, &EngineLimits::default()) {
                Ok(scene) => {
                    parts.insert(name, scene.parts);
                }
                Err(e) => {
                    eprintln!("robot part {name}: {e}");
                    return None;
                }
            }
        }
        Some(Kit {
            parts,
            models: HashMap::new(),
        })
    }
}

/// One part on screen: its pivot and what hangs on it (LOD sets sharing
/// the kit's models, or a stand-in box).
#[derive(Clone)]
struct Part {
    pivot: EntityId,
    sets: Vec<LodId>,
    meshes: Vec<EntityId>,
}

/// One robot on screen.
struct Drawn {
    kind: EntityKind,
    root: EntityId,
    parts: HashMap<String, Part>,
    /// Metres walked (the gait), where it was last frame.
    walked: f32,
    last: Vec3,
    /// How much it is walking (0 still .. 1 striding), smoothed.
    moving: f32,
    yaw: f32,
    aim: f32,
    kneel: f32,
    hatch: f32,
    /// Parts already blown off (bit per part).
    broken: u8,
    /// Seconds to the next puff of smoke from a broken part.
    smoke: f32,
    /// The gun arm's raise (0 down .. 1 aimed) and recoil.
    raise: f32,
    kick: f32,
    /// A stomp or a discharge charging: seconds left.
    rear: f32,
    charge: f32,
    /// The lance charging in the barrel: seconds left.
    lance: f32,
    /// Beaten: seconds since, and the fire on its back.
    dead: Option<f32>,
    fire: Option<Fire>,
}

/// The Aether lance raking its line: the colossus, the line on the
/// ground, seconds since it fired, and when the last fire was lit.
struct Rake {
    from: NetId,
    near: Vec3,
    far: Vec3,
    t: f32,
    lit: f32,
}

/// A shell in the air: from, to, seconds flown, seconds it takes.
struct Shell {
    from: Vec3,
    to: Vec3,
    t: f32,
    span: f32,
}

struct Sounds {
    thud: Sound,
    cannon: Sound,
    klaxon: Sound,
    zap: Sound,
    hum: Sound,
    servo: Sound,
    crash: Sound,
    collapse: Sound,
}

/// The machines on screen.
#[derive(Default)]
pub struct RobotView {
    kit: Option<Kit>,
    tried: bool,
    drawn: BTreeMap<NetId, Drawn>,
    shells: Vec<Shell>,
    rakes: Vec<Rake>,
    /// Seconds before the next tip about its armour, and whether the
    /// first sight of one was told.
    tip_in: f32,
    told: bool,
    /// Warning rings on the ground: decal and seconds left.
    rings: Vec<(DecalId, f32)>,
    sounds: Option<Sounds>,
    music: Music,
}

/// How near (m) the camera must be to a standing colossus for its fight
/// music, and how far away it fades again.
const MUSIC_NEAR: f32 = 260.0;
const MUSIC_FAR: f32 = 340.0;

/// The colossus fight's music: decoded off the main thread when a colossus
/// comes near (three minutes of music is large once decoded), dropped again
/// when the fight is over.
#[derive(Default)]
struct Music {
    loading: Option<std::sync::mpsc::Receiver<EngineResult<Sound>>>,
    playing: bool,
}

const COLOSSUS_THEME: &[u8] = include_bytes!("../assets/robots/colossus_theme.ogg");

fn pos(e: &EntityState) -> Vec3 {
    Vec3::new(e.pos[0] as f32, e.pos[1] as f32, e.pos[2] as f32)
}

fn v3(d: DVec3) -> Vec3 {
    d.as_vec3()
}

/// Turns `from` towards `to` (radians) by at most `max`.
fn turn(from: f32, to: f32, max: f32) -> f32 {
    let mut d = (to - from) % (2.0 * PI);
    if d > PI {
        d -= 2.0 * PI;
    } else if d < -PI {
        d += 2.0 * PI;
    }
    from + d.clamp(-max, max)
}

impl RobotView {
    pub fn new() -> Self {
        Self::default()
    }

    /// Where a robot's gun is (ray beams start there).
    pub fn muzzles(&self, world: &World) -> BTreeMap<NetId, Vec3> {
        let mut out = BTreeMap::new();
        for (id, d) in &self.drawn {
            let part = match d.kind {
                EntityKind::Sentry => "gun",
                EntityKind::Colossus => "cannon",
                _ => continue,
            };
            let Some(p) = d.parts.get(part) else { continue };
            if let Ok(m) = world.world_transform(p.pivot) {
                let tip = match d.kind {
                    EntityKind::Sentry => Vec3::new(0.0, -1.05, 0.0),
                    _ => Vec3::new(0.0, 0.0, 6.8),
                };
                out.insert(*id, m.transform_point3(tip));
            }
        }
        out
    }

    /// Draws every robot in `all` where the server has it, and forgets the
    /// ones that are gone. Call once a frame after the fight's effects.
    pub fn update(&mut self, world: &mut World, all: &[EntityState], dt: f32) -> EngineResult<()> {
        if !self.tried {
            self.tried = true;
            self.kit = Kit::load();
            if self.kit.is_none() {
                eprintln!("robot parts not found (assets/robots); robots are drawn as boxes");
            }
            self.sounds = make_sounds();
        }
        let robots: Vec<&EntityState> = all
            .iter()
            .filter(|e| matches!(e.kind, EntityKind::Sentry | EntityKind::Colossus))
            .collect();
        // Gone: take them off the screen.
        let gone: Vec<NetId> = self
            .drawn
            .keys()
            .filter(|id| !robots.iter().any(|r| r.id == **id))
            .copied()
            .collect();
        for id in gone {
            if let Some(d) = self.drawn.remove(&id) {
                if let Some(f) = d.fire {
                    world.stop_fire(f);
                }
                world.despawn(d.root);
            }
        }
        for r in robots {
            if !self.drawn.contains_key(&r.id) {
                let d = self.build(world, r)?;
                self.drawn.insert(r.id, d);
            }
            // Which of its parts are broken (their weak points are down).
            let mut broken = 0u8;
            for w in all.iter().filter(|w| w.kind == EntityKind::WeakPoint) {
                if let (Some((on, part)), Some(c)) = (w.mount, w.combat) {
                    if on == r.id && c.status == Status::Down {
                        broken |= 1 << part;
                    }
                }
            }
            let foe = r
                .combat
                .and_then(|c| c.target)
                .and_then(|t| all.iter().find(|e| e.id == t))
                .map(pos);
            match r.kind {
                EntityKind::Sentry => self.pose_sentry(world, r, foe, broken, dt)?,
                _ => self.pose_colossus(world, r, foe, broken, dt)?,
            }
        }
        // Shells in the air leave a glowing trail.
        for s in &mut self.shells {
            s.t += dt;
            let k = (s.t / s.span).min(1.0);
            let arc = s.from.lerp(s.to, k) + Vec3::Y * (4.0 * 14.0 * k * (1.0 - k));
            let _ = world.particles_mut().burst(
                ParticleEmitter::new(arc)
                    .with_mode(ParticleMode::Burst)
                    .with_burst_count(3)
                    .with_size(0.5)
                    .with_lifetime(0.35)
                    .with_spread(Vec3::splat(0.1))
                    .with_color(rgb(255, 200, 90))
                    .with_secondary_color(rgb(255, 90, 30))
                    .with_size_over_life(SizeOverLife::Shrink)
                    .with_emissive_intensity(6.0),
            );
        }
        self.shells.retain(|s| s.t < s.span);
        // The lance charging: light gathers at the muzzle.
        let muzzles = self.muzzles(world);
        for (id, d) in &mut self.drawn {
            if d.lance <= 0.0 {
                continue;
            }
            d.lance = (d.lance - dt).max(0.0);
            if let Some(&m) = muzzles.get(id) {
                let k = 1.0 - d.lance / geo::LANCE_WINDUP as f32;
                fightview::glow_at(world, m, rgb(200, 120, 255), 0.6 + 2.2 * k);
            }
        }
        // The lance raking its line.
        for r in &mut self.rakes {
            r.t += dt;
            let s = (r.t / geo::LANCE_SECONDS as f32).min(1.0);
            let g = r.near.lerp(r.far, s);
            let from = muzzles.get(&r.from).copied().unwrap_or(g + Vec3::Y * 15.0);
            lance_beam(world, from, g);
            fightview::glow_at(world, from, rgb(230, 170, 255), 2.4);
            if r.t - r.lit > 0.12 {
                r.lit = r.t;
                let _ = world.explosion(g, 1.6);
                let scar = Decal::new(DecalKind::Scorch, g + Vec3::Y * 0.2, Vec3::Y, 2.2)
                    .depth(0.8)
                    .fade(0.1, 8.0);
                let _ = world.spawn_decal(scar);
            }
        }
        self.rakes.retain(|r| r.t < geo::LANCE_SECONDS as f32);
        self.tip_in = (self.tip_in - dt).max(0.0);
        for (id, left) in &mut self.rings {
            *left -= dt;
            if *left <= 0.0 {
                world.remove_decal(*id);
            }
        }
        self.rings.retain(|r| r.1 > 0.0);
        Ok(())
    }

    fn part(
        &self,
        world: &mut World,
        parent: EntityId,
        name: &str,
        place: Place,
    ) -> EngineResult<Part> {
        let pivot = world.spawn_pivot(Place::default());
        world.attach(pivot, parent, place)?;
        let mut meshes = Vec::new();
        let mut sets = Vec::new();
        match self.kit.as_ref().and_then(|k| k.models.get(name)) {
            Some(list) => {
                for &model in list {
                    let set = world.spawn_lod_like(model, &[])?;
                    world.attach_lod(set, pivot, Place::default())?;
                    sets.push(set);
                }
            }
            None => {
                // No parts on disk: a grey box keeps the game playable.
                let mut m = Mesh::new();
                let _ = m.add_box((0.0, 0.0, 0.0), (0.4, 0.4, 0.4), rgb(90, 110, 95));
                let e = world.place(m, Place::default())?;
                world.attach(e, pivot, Place::default())?;
                meshes.push(e);
            }
        }
        Ok(Part {
            pivot,
            sets,
            meshes,
        })
    }

    fn pivot(world: &mut World, parent: EntityId, place: Place) -> EngineResult<EntityId> {
        let p = world.spawn_pivot(Place::default());
        world.attach(p, parent, place)?;
        Ok(p)
    }

    fn build(&mut self, world: &mut World, r: &EntityState) -> EngineResult<Drawn> {
        if let Some(kit) = self.kit.as_mut() {
            kit.ensure_models(world)?;
        }
        let at = pos(r);
        let yaw = rotation_yaw(r.rot) as f32;
        let root = world.spawn_pivot(Place::new(at.x, at.y, at.z).with_yaw_deg(yaw.to_degrees()));
        let mut parts = HashMap::new();
        match r.kind {
            EntityKind::Sentry => {
                let body =
                    self.part(world, root, "sentry_body", Place::new(0.0, SENTRY_HIP, 0.0))?;
                let b = body.pivot;
                parts.insert("body".into(), body);
                parts.insert(
                    "dish".into(),
                    self.part(world, b, "sentry_dish", Place::new(0.0, 0.8, -0.5))?,
                );
                // Facing +Z, its left is +X.
                parts.insert(
                    "arm".into(),
                    self.part(world, b, "sentry_arm", Place::new(0.5, 0.9, 0.0))?,
                );
                parts.insert(
                    "gun".into(),
                    self.part(world, b, "sentry_gun", Place::new(-0.5, 0.9, 0.0))?,
                );
                for (side, x) in [("l", 0.25f32), ("r", -0.25)] {
                    let thigh =
                        self.part(world, root, "sentry_thigh", Place::new(x, SENTRY_HIP, 0.0))?;
                    let shin = self.part(
                        world,
                        thigh.pivot,
                        "sentry_shin",
                        Place::new(0.0, -0.5, 0.0),
                    )?;
                    parts.insert(format!("thigh_{side}"), thigh);
                    parts.insert(format!("shin_{side}"), shin);
                }
            }
            _ => {
                let hull = self.part(
                    world,
                    root,
                    "colossus_hull",
                    Place::new(0.0, hull_height(0.0) as f32, 0.0),
                )?;
                let h = hull.pivot;
                parts.insert("hull".into(), hull);
                let turret = self.part(
                    world,
                    h,
                    "colossus_turret",
                    Place::new(0.0, TURRET_UP as f32, 0.0),
                )?;
                let cannon = self.part(
                    world,
                    turret.pivot,
                    "colossus_cannon",
                    Place::new(0.0, TRUNNION.0 as f32, TRUNNION.1 as f32),
                )?;
                parts.insert("turret".into(), turret);
                parts.insert("cannon".into(), cannon);
                parts.insert(
                    "hatch".into(),
                    self.part(world, h, "colossus_shell", Place::new(0.0, 0.0, 2.0))?,
                );
                parts.insert(
                    "core".into(),
                    self.part(
                        world,
                        h,
                        "colossus_core",
                        Place::new(0.0, -geo::CORE_DOWN as f32, 0.0),
                    )?,
                );
                for k in 0..4 {
                    let hy = Self::pivot(world, root, Place::default())?;
                    let thigh = self.part(world, hy, "colossus_thigh", Place::default())?;
                    let ky = Self::pivot(world, root, Place::default())?;
                    let shin = self.part(world, ky, "colossus_shin", Place::default())?;
                    let kn = self.part(world, ky, "colossus_knee", Place::default())?;
                    parts.insert(
                        format!("hipyaw{k}"),
                        Part {
                            pivot: hy,
                            sets: Vec::new(),
                            meshes: Vec::new(),
                        },
                    );
                    parts.insert(
                        format!("kneeyaw{k}"),
                        Part {
                            pivot: ky,
                            sets: Vec::new(),
                            meshes: Vec::new(),
                        },
                    );
                    parts.insert(format!("thigh{k}"), thigh);
                    parts.insert(format!("shin{k}"), shin);
                    parts.insert(format!("knee{k}"), kn);
                }
            }
        }
        Ok(Drawn {
            kind: r.kind,
            root,
            parts,
            walked: 0.0,
            last: at,
            moving: 0.0,
            yaw,
            aim: yaw,
            kneel: 0.0,
            hatch: 0.0,
            broken: 0,
            smoke: 0.0,
            raise: 0.0,
            kick: 0.0,
            rear: 0.0,
            lance: 0.0,
            charge: 0.0,
            dead: None,
            fire: None,
        })
    }

    fn set(world: &mut World, d: &Drawn, name: &str, place: Place) -> EngineResult<()> {
        if let Some(p) = d.parts.get(name) {
            world.set_place(p.pivot, place)?;
        }
        Ok(())
    }

    /// Blows a part off: an explosion, the part in pieces.
    fn blow_off(world: &mut World, d: &mut Drawn, name: &str, size: f32) {
        let Some(p) = d.parts.get_mut(name) else {
            return;
        };
        let at = world
            .world_transform(p.pivot)
            .map(|m| m.w_axis.truncate())
            .unwrap_or(d.last);
        let _ = world.explosion(at, size);
        // The part goes; plain metal chunks fly. (Engine's shatter keeps a
        // textured, normal-mapped part's maps on pieces that have no UVs or
        // tangents for them, which renders as white glare and black specks.)
        for e in std::mem::take(&mut p.meshes) {
            world.despawn(e);
        }
        for set in std::mem::take(&mut p.sets) {
            world.despawn_lod(set);
        }
        for (k, colour) in [rgb(38, 72, 52), rgb(150, 118, 52), rgb(60, 62, 66)]
            .into_iter()
            .enumerate()
        {
            let mut m = Mesh::new();
            let s = size * (0.45 - 0.1 * k as f32);
            let off =
                Vec3::new(0.3 * k as f32 - 0.3, 0.2 * k as f32, 0.15 - 0.15 * k as f32) * size;
            if m.add_box(off, Vec3::new(s, s * 0.6, s * 0.8), colour)
                .is_err()
            {
                continue;
            }
            m.set_surface_material(SurfaceMaterial {
                roughness: 0.45,
                metallic: 0.8,
                ..SurfaceMaterial::default()
            });
            let Ok(e) = world.place(m, Place::new(at.x, at.y, at.z)) else {
                continue;
            };
            let shatter = Shatter {
                pieces: 6,
                speed: 6.0 + size,
                keep_seconds: Some(20.0),
                seed: k as u32 + 1,
                density: 7.8,
                ..Shatter::default()
            };
            if world.shatter(e, at, shatter).is_err() {
                world.despawn(e);
            }
        }
    }

    fn pose_sentry(
        &mut self,
        world: &mut World,
        r: &EntityState,
        foe: Option<Vec3>,
        broken: u8,
        dt: f32,
    ) -> EngineResult<()> {
        let Some(d) = self.drawn.get_mut(&r.id) else {
            return Ok(());
        };
        let at = pos(r);
        let step = Vec3::new(at.x - d.last.x, 0.0, at.z - d.last.z).length();
        d.walked += step;
        d.last = at;
        let speed = if dt > 0.0 { step / dt } else { 0.0 };
        d.moving += ((speed / 1.6).min(1.0) - d.moving) * (dt * 6.0).min(1.0);
        let status = r.combat.map_or(Status::Standing, |c| c.status);
        // Face the foe while it fights and stands still, else where it walks.
        let vel = Vec3::from(r.vel);
        let want = match foe {
            Some(f) if d.moving < 0.4 => Some((f.x - at.x).atan2(f.z - at.z)),
            _ if vel.length() > 0.3 => Some(vel.x.atan2(vel.z)),
            _ => None,
        };
        if let Some(w) = want {
            d.yaw = turn(d.yaw, w, 4.0 * dt);
        }
        if broken & (1 << DISH) != 0 && d.broken & (1 << DISH) == 0 {
            d.broken |= 1 << DISH;
            Self::blow_off(world, d, "dish", 1.2);
        }
        let down = matches!(status, Status::Down | Status::Carried);
        world.set_place(
            d.root,
            Place::new(at.x, at.y, at.z).with_yaw_deg(d.yaw.to_degrees()),
        )?;
        let phase = d.walked / SENTRY_CYCLE * 2.0 * PI;
        let m = d.moving;
        let swing = 26.0 * m;
        let bob = 0.05 * m * phase.sin().abs();
        if down {
            // Switched off: slumped forward on its knees.
            Self::set(
                world,
                d,
                "body",
                Place::new(0.0, 0.75, 0.25).with_pitch_deg(25.0),
            )?;
            for side in ["l", "r"] {
                Self::set(
                    world,
                    d,
                    &format!("thigh_{side}"),
                    Place::new(if side == "l" { 0.25 } else { -0.25 }, 0.75, 0.0)
                        .with_pitch_deg(-80.0),
                )?;
                Self::set(
                    world,
                    d,
                    &format!("shin_{side}"),
                    Place::new(0.0, -0.5, 0.0).with_pitch_deg(85.0),
                )?;
            }
            Self::set(
                world,
                d,
                "gun",
                Place::new(-0.5, 0.9, 0.0).with_pitch_deg(-10.0),
            )?;
            Self::set(
                world,
                d,
                "arm",
                Place::new(0.5, 0.9, 0.0).with_pitch_deg(-10.0),
            )?;
            return Ok(());
        }
        Self::set(
            world,
            d,
            "body",
            Place::new(0.0, SENTRY_HIP + bob, 0.0).with_roll_deg(3.0 * m * phase.sin()),
        )?;
        for (side, x, off) in [("l", 0.25f32, 0.0f32), ("r", -0.25, PI)] {
            let p = phase + off;
            Self::set(
                world,
                d,
                &format!("thigh_{side}"),
                Place::new(x, SENTRY_HIP + bob, 0.0).with_pitch_deg(-swing * p.sin()),
            )?;
            Self::set(
                world,
                d,
                &format!("shin_{side}"),
                Place::new(0.0, -0.5, 0.0).with_pitch_deg(38.0 * m * p.cos().max(0.0)),
            )?;
        }
        // The gun arm comes up to aim at its foe and kicks when it fires.
        let aiming = foe.is_some();
        d.raise += ((aiming as u8 as f32) - d.raise) * (dt * 5.0).min(1.0);
        d.kick = (d.kick - dt * 3.0).max(0.0);
        let mut gun_pitch = -12.0 * m * phase.sin();
        if let Some(f) = foe {
            let from = at + Vec3::Y * (SENTRY_HIP + 0.9);
            let flat = Vec3::new(f.x - from.x, 0.0, f.z - from.z).length().max(0.5);
            let up = (f.y + 1.1 - from.y).atan2(flat).to_degrees();
            gun_pitch += (-(90.0 + up) - gun_pitch) * d.raise;
        }
        Self::set(
            world,
            d,
            "gun",
            Place::new(-0.5, 0.9, 0.0).with_pitch_deg(gun_pitch + d.kick * 18.0),
        )?;
        Self::set(
            world,
            d,
            "arm",
            Place::new(0.5, 0.9, 0.0).with_pitch_deg(12.0 * m * phase.sin()),
        )?;
        // A broken dish smokes.
        if d.broken != 0 {
            d.smoke -= dt;
            if d.smoke <= 0.0 {
                d.smoke = 0.25;
                let top = at + Vec3::Y * 2.3;
                smoke_puff(world, top, 0.4);
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn pose_colossus(
        &mut self,
        world: &mut World,
        r: &EntityState,
        foe: Option<Vec3>,
        broken: u8,
        dt: f32,
    ) -> EngineResult<()> {
        let Some(d) = self.drawn.get_mut(&r.id) else {
            return Ok(());
        };
        let at = pos(r);
        let step = Vec3::new(at.x - d.last.x, 0.0, at.z - d.last.z).length();
        d.walked += step;
        d.last = at;
        let speed = if dt > 0.0 { step / dt } else { 0.0 };
        d.moving += ((speed / 1.2).min(1.0) - d.moving) * (dt * 3.0).min(1.0);
        d.yaw = rotation_yaw(r.rot) as f32;
        let c = r.combat.unwrap_or_default();
        let dead = c.status == Status::Down;
        // Down on its belly, the hatch open, the turret on its foe: the same
        // rates as the server's.
        let kneel_to = if c.effects & EFFECT_KNEELING != 0 || dead {
            1.0
        } else {
            0.0
        };
        let rate = if kneel_to > d.kneel { 0.6 } else { 0.25 };
        d.kneel += (kneel_to - d.kneel).clamp(-rate * dt, rate * dt);
        let open = c.effects & EFFECT_ARMOURED == 0 && !dead;
        d.hatch += ((open as u8 as f32) - d.hatch) * (dt * 1.5).min(1.0);
        let want_aim = foe.map_or(d.yaw, |f| (f.x - at.x).atan2(f.z - at.z));
        d.aim = turn(d.aim, want_aim, 30f32.to_radians() * dt);
        d.kick = (d.kick - dt * 1.5).max(0.0);
        d.rear = (d.rear - dt).max(0.0);
        d.charge = (d.charge - dt).max(0.0);

        // Parts that broke this frame blow off.
        for part in 0..5u8 {
            if broken & (1 << part) != 0 && d.broken & (1 << part) == 0 {
                d.broken |= 1 << part;
                if part == CANNON {
                    Self::blow_off(world, d, "cannon", 3.0);
                } else {
                    Self::blow_off(world, d, &format!("knee{part}"), 2.5);
                }
            }
        }
        // Beaten: explosions run over it, then it burns.
        if dead {
            let t = d.dead.get_or_insert(0.0);
            let before = *t;
            *t += dt;
            let now = *t;
            for (k, when) in [0.0f32, 0.5, 1.1, 1.6, 2.4].into_iter().enumerate() {
                if before <= when && now > when {
                    let a = k as f32 * 2.1;
                    let p = at
                        + Vec3::new(
                            a.sin() * 3.5,
                            hull_height(d.kneel as f64) as f32 + 1.5,
                            a.cos() * 3.5,
                        );
                    let _ = world.explosion(p, 3.5 + k as f32 * 0.5);
                }
            }
            if before <= 2.4 && now > 2.4 {
                Self::blow_off(world, d, "core", 4.0);
                Self::blow_off(world, d, "turret", 3.0);
            }
            if d.fire.is_none() && now > 1.0 {
                let top = at + Vec3::Y * (hull_height(1.0) as f32 + 3.4);
                d.fire = world.start_fire(top, 3.0).ok();
            }
        }

        world.set_place(
            d.root,
            Place::new(at.x, at.y, at.z).with_yaw_deg(d.yaw.to_degrees()),
        )?;
        let kneel = d.kneel as f64;
        let phase = d.walked / CYCLE;
        let m = d.moving;
        let bob = 0.18 * m * (phase * 4.0 * 2.0 * PI).sin();
        let hull_y = hull_height(kneel) as f32 + bob;
        // It rears before a stomp, and lists when it is down on broken legs.
        let rear = if d.rear > 0.0 {
            -6.0 * (d.rear / 1.2).min(1.5)
        } else {
            0.0
        };
        let list = if dead { 7.0 } else { 0.0 };
        Self::set(
            world,
            d,
            "hull",
            Place::new(0.0, hull_y, 0.0)
                .with_pitch_deg(rear)
                .with_roll_deg(list + 1.5 * m * (phase * 2.0 * PI).sin()),
        )?;
        Self::set(
            world,
            d,
            "turret",
            Place::new(0.0, TURRET_UP as f32, 0.0).with_yaw_deg((d.aim - d.yaw).to_degrees()),
        )?;
        Self::set(
            world,
            d,
            "cannon",
            Place::new(0.0, TRUNNION.0 as f32, TRUNNION.1 as f32 - d.kick * 1.2)
                .with_pitch_deg(-6.0 - d.kick * 6.0),
        )?;
        Self::set(
            world,
            d,
            "hatch",
            Place::new(0.0, -0.6 * d.hatch, 2.0).with_pitch_deg(-75.0 * d.hatch),
        )?;
        let glow = 1.0 + 0.25 * d.charge + 0.06 * self_clock().sin();
        Self::set(
            world,
            d,
            "core",
            Place::new(0.0, -geo::CORE_DOWN as f32 * (0.3 + 0.7 * d.hatch), 0.0).with_scale(glow),
        )?;
        // The legs: each foot on its own step of the cycle, two-bone knees.
        for k in 0..4usize {
            let order = STEP_ORDER.iter().position(|&x| x == k).unwrap_or(0) as f32;
            let p = (phase + order / 4.0).rem_euclid(1.0);
            let lame = d.broken & (1 << k) != 0;
            let (along, lift) = if lame {
                // A dead leg drags behind.
                (-0.3 * STRIDE as f32, 0.0)
            } else if p < 0.25 {
                let t = p / 0.25;
                ((-0.5 + t) * STRIDE as f32, 1.6 * (t * PI).sin())
            } else {
                let t = (p - 0.25) / 0.75;
                ((0.5 - t) * STRIDE as f32, 0.0)
            };
            let mut foot = foot_rest(k, kneel);
            foot.z += (along * m) as f64;
            foot.y += (lift * m) as f64;
            if lame {
                foot.y = 0.0;
            }
            let mut hp = hip(k, kneel);
            hp.y += bob as f64;
            let kn = knee(hp, foot);
            let dir = DVec3::new(foot.x - hp.x, 0.0, foot.z - hp.z).normalize_or(DVec3::X);
            let yaw = (-dir.z).atan2(dir.x).to_degrees() as f32;
            let flat = |v: DVec3| v.x * dir.x + v.z * dir.z;
            let thigh = kn - hp;
            let roll_t = (thigh.y.atan2(flat(thigh)) - THIGH.1.atan2(THIGH.0)).to_degrees() as f32;
            let shin = foot - kn;
            let roll_s = (shin.y.atan2(flat(shin)) - SHIN.1.atan2(SHIN.0)).to_degrees() as f32;
            Self::set(
                world,
                d,
                &format!("hipyaw{k}"),
                Place::new(hp.x as f32, hp.y as f32, hp.z as f32).with_yaw_deg(yaw),
            )?;
            Self::set(
                world,
                d,
                &format!("thigh{k}"),
                Place::default().with_roll_deg(roll_t),
            )?;
            Self::set(
                world,
                d,
                &format!("kneeyaw{k}"),
                Place::new(kn.x as f32, kn.y as f32, kn.z as f32).with_yaw_deg(yaw),
            )?;
            Self::set(
                world,
                d,
                &format!("shin{k}"),
                Place::default().with_roll_deg(roll_s),
            )?;
            // A foot coming down: dust and a thud.
            if !lame && m > 0.3 {
                let before = (p - step / CYCLE).rem_euclid(1.0);
                if before < 0.25 && p >= 0.25 {
                    let w = geo::to_world(
                        DVec3::new(at.x as f64, at.y as f64, at.z as f64),
                        d.yaw as f64,
                        foot,
                    );
                    let w = v3(w);
                    dust(world, w, 2.4);
                    if let Some(s) = &self.sounds {
                        world.play_sound(
                            &s.thud,
                            PlaySound::at(w).volume(0.9).pitch(0.8).range(6.0, 260.0),
                        );
                    }
                }
            }
        }
        // Broken parts smoke and spark.
        if d.broken != 0 && !dead {
            d.smoke -= dt;
            if d.smoke <= 0.0 {
                d.smoke = 0.18;
                let c = geo::Colossus {
                    yaw: d.yaw as f64,
                    aim: d.aim as f64,
                    kneel,
                    ..blank()
                };
                let base = DVec3::new(at.x as f64, at.y as f64, at.z as f64);
                for part in 0..5u8 {
                    if d.broken & (1 << part) != 0 {
                        let p = v3(geo::colossus_part(base, &c, part));
                        smoke_puff(world, p, 1.4);
                        fightview::sparks(world, p, Vec3::Y, 10, rgb(255, 210, 120));
                    }
                }
            }
        }
        Ok(())
    }

    /// The machines' part of what happened in fights: warnings, shells,
    /// stomps, discharges, leaps, parts blown off.
    pub fn on_fx(
        &mut self,
        world: &mut World,
        fight: &mut FightView,
        events: &[Fx],
        all: &[EntityState],
        muzzles: &BTreeMap<NetId, Vec3>,
        me: Option<NetId>,
    ) {
        self.tips(fight, events, all, me);
        for e in events {
            let at = Vec3::from(e.at);
            let whole = e.from == e.to;
            let on_me = me == Some(e.to);
            match e.kind {
                FxKind::ShellWindup => {
                    // The cannon fires: a flash, a boom, a shell arcing in,
                    // and a red ring where it will land.
                    let from = muzzles.get(&e.from).copied().unwrap_or(at + Vec3::Y * 30.0);
                    let _ = world
                        .particles_mut()
                        .burst(ParticleEmitter::fireball(from, 1.4));
                    if let Some(d) = self.drawn.get_mut(&e.from) {
                        d.kick = 1.0;
                    }
                    self.shells.push(Shell {
                        from,
                        to: at,
                        t: 0.0,
                        span: hw_sim::robots::SHELL_WINDUP as f32,
                    });
                    self.ring(world, at, geo::SHELL_AREA as f32, rgb(255, 50, 30), 2.0);
                    if let Some(s) = &self.sounds {
                        world.play_sound(
                            &s.cannon,
                            PlaySound::at(from).volume(1.4).range(15.0, 500.0),
                        );
                        world
                            .play_sound(&s.klaxon, PlaySound::at(at).volume(0.6).range(6.0, 140.0));
                    }
                    let _ = all;
                }
                FxKind::Shell if whole => {
                    let _ = world.explosion(at, 3.0);
                    fight.add_shake(0.6);
                }
                FxKind::StompWindup => {
                    if let Some(d) = self.drawn.get_mut(&e.from) {
                        d.rear = 1.2;
                    }
                    self.ring(world, at, geo::STOMP_AREA as f32, rgb(255, 120, 30), 1.6);
                    if let Some(s) = &self.sounds {
                        world.play_sound(&s.servo, PlaySound::at(at).volume(1.0).range(8.0, 160.0));
                    }
                }
                FxKind::Stomp if whole => {
                    fightview::shockwave(
                        world,
                        at,
                        rgb(255, 210, 150),
                        rgb(200, 150, 90),
                        geo::STOMP_AREA as f32,
                    );
                    dust(world, at, 8.0);
                    fight.add_shake(1.0);
                    if let Some(s) = &self.sounds {
                        world.play_sound(
                            &s.thud,
                            PlaySound::at(at).volume(1.4).pitch(0.6).range(8.0, 300.0),
                        );
                    }
                }
                FxKind::PulseWindup => {
                    if let Some(d) = self.drawn.get_mut(&e.from) {
                        d.charge = 1.6;
                    }
                    let ground = all.iter().find(|s| s.id == e.from).map_or(at, pos);
                    self.ring(
                        world,
                        ground,
                        geo::PULSE_AREA as f32,
                        rgb(90, 255, 140),
                        1.8,
                    );
                    fightview::glow_at(world, at, rgb(120, 255, 150), 2.5);
                    if let Some(s) = &self.sounds {
                        world.play_sound(&s.hum, PlaySound::at(at).volume(1.0).range(8.0, 160.0));
                    }
                }
                FxKind::Pulse if whole => {
                    let ground = all.iter().find(|s| s.id == e.from).map_or(at, pos);
                    fightview::shockwave(
                        world,
                        ground,
                        rgb(140, 255, 160),
                        rgb(40, 200, 90),
                        geo::PULSE_AREA as f32,
                    );
                    fightview::sparks(world, at, Vec3::Y, 80, rgb(150, 255, 170));
                    fight.add_shake(0.5);
                    if let Some(s) = &self.sounds {
                        world.play_sound(&s.zap, PlaySound::at(at).volume(1.2).range(8.0, 200.0));
                    }
                }
                FxKind::LeapWindup => {
                    self.ring(
                        world,
                        at,
                        hw_sim::combat::LEAP_AREA as f32,
                        rgb(255, 60, 40),
                        2.0,
                    );
                }
                FxKind::Leap if whole => {
                    fightview::shockwave(
                        world,
                        at,
                        rgb(255, 200, 150),
                        rgb(180, 120, 80),
                        hw_sim::combat::LEAP_AREA as f32,
                    );
                    dust(world, at, 3.5);
                    fight.add_shake(0.6);
                    if let Some(s) = &self.sounds {
                        world.play_sound(
                            &s.thud,
                            PlaySound::at(at).volume(1.1).pitch(1.1).range(5.0, 120.0),
                        );
                    }
                }
                FxKind::ChargeWindup => {
                    // Head down, servos whining: a red lane to where it runs.
                    let from = all.iter().find(|s| s.id == e.from).map_or(at, pos);
                    if let Some(d) = self.drawn.get_mut(&e.from) {
                        d.rear = geo::CHARGE_WINDUP as f32;
                    }
                    self.lane(
                        world,
                        from,
                        at,
                        geo::CHARGE_WIDTH as f32 * 0.7,
                        rgb(255, 40, 30),
                        3.6,
                    );
                    if let Some(s) = &self.sounds {
                        world.play_sound(
                            &s.servo,
                            PlaySound::at(from)
                                .volume(1.4)
                                .pitch(0.6)
                                .range(10.0, 300.0),
                        );
                        world.play_sound(
                            &s.klaxon,
                            PlaySound::at(from).volume(1.0).range(10.0, 260.0),
                        );
                    }
                    if near_me(all, me, from, 160.0) {
                        fight.say("It's charging! Get out of the red lane!");
                    }
                }
                FxKind::Charge if whole => {
                    dust(world, at, 10.0);
                    fight.add_shake(0.8);
                    if let Some(s) = &self.sounds {
                        world.play_sound(
                            &s.thud,
                            PlaySound::at(at).volume(1.5).pitch(0.5).range(10.0, 400.0),
                        );
                    }
                }
                FxKind::LanceWindup => {
                    // The lance charges: a violet line where it will rake.
                    let from = all.iter().find(|s| s.id == e.from).map_or(at, pos);
                    if let Some(d) = self.drawn.get_mut(&e.from) {
                        d.lance = geo::LANCE_WINDUP as f32;
                    }
                    let way = Vec3::new(at.x - from.x, 0.0, at.z - from.z).normalize_or_zero();
                    let near = from + way * geo::LANCE_NEAR as f32;
                    self.lane(
                        world,
                        near,
                        at,
                        geo::LANCE_WIDTH as f32,
                        rgb(190, 80, 255),
                        4.0,
                    );
                    if let Some(s) = &self.sounds {
                        world.play_sound(
                            &s.hum,
                            PlaySound::at(from)
                                .volume(1.5)
                                .pitch(0.7)
                                .range(10.0, 300.0),
                        );
                    }
                    if near_me(all, me, from, 160.0) {
                        fight.say("Aether lance! Leave the violet line, or hit the cannon to overload it!");
                    }
                }
                FxKind::Lance if whole => {
                    let from = all.iter().find(|s| s.id == e.from).map_or(at, pos);
                    let way = Vec3::new(at.x - from.x, 0.0, at.z - from.z).normalize_or_zero();
                    let near = from + way * geo::LANCE_NEAR as f32;
                    if let Some(d) = self.drawn.get_mut(&e.from) {
                        d.lance = 0.0;
                        d.kick = 1.0;
                    }
                    self.rakes.push(Rake {
                        from: e.from,
                        near,
                        far: at,
                        t: 0.0,
                        lit: -1.0,
                    });
                    fight.add_shake(0.7);
                    if let Some(s) = &self.sounds {
                        world.play_sound(
                            &s.zap,
                            PlaySound::at(near)
                                .volume(1.6)
                                .pitch(0.6)
                                .range(15.0, 500.0),
                        );
                    }
                }
                FxKind::SmashWindup => {
                    let from = all.iter().find(|s| s.id == e.from).map_or(at, pos);
                    if let Some(d) = self.drawn.get_mut(&e.from) {
                        d.rear = geo::SMASH_WINDUP as f32;
                    }
                    self.ring(world, at, geo::SMASH_AREA as f32, rgb(255, 140, 30), 1.8);
                    if let Some(s) = &self.sounds {
                        world.play_sound(
                            &s.servo,
                            PlaySound::at(from).volume(1.2).range(10.0, 260.0),
                        );
                    }
                }
                FxKind::Smash if whole => {
                    let _ = world.explosion(at + Vec3::Y * 4.0, 3.0);
                    dust(world, at, 9.0);
                    fight.add_shake(0.9);
                    if let Some(s) = &self.sounds {
                        world.play_sound(
                            &s.crash,
                            PlaySound::at(at).volume(1.6).pitch(0.8).range(10.0, 450.0),
                        );
                    }
                }
                FxKind::Overload => {
                    if let Some(d) = self.drawn.get_mut(&e.from) {
                        d.lance = 0.0;
                    }
                    let _ = world.explosion(at, 2.5);
                    fightview::sparks(world, at, Vec3::Y, 120, rgb(220, 150, 255));
                    fight.add_shake(0.6);
                    if let Some(s) = &self.sounds {
                        world.play_sound(
                            &s.zap,
                            PlaySound::at(at).volume(1.4).pitch(1.4).range(10.0, 300.0),
                        );
                    }
                    fight.say("Cannon overloaded! It's reeling: hit it hard!");
                }
                FxKind::PartBroken => {
                    if let Some(s) = &self.sounds {
                        world
                            .play_sound(&s.crash, PlaySound::at(at).volume(1.3).range(10.0, 300.0));
                    }
                    let name = part_name(all, e.from, e.to);
                    fight.say(&format!("{name} destroyed!"));
                }
                FxKind::Kneel => {
                    dust(world, at, 12.0);
                    fight.add_shake(1.0);
                    fight.say("It's down! Hit the core!");
                    if let Some(s) = &self.sounds {
                        world.play_sound(
                            &s.collapse,
                            PlaySound::at(at).volume(1.5).range(10.0, 400.0),
                        );
                    }
                }
                FxKind::Rise => {
                    fight.say("It's getting up again!");
                    if let Some(s) = &self.sounds {
                        world.play_sound(
                            &s.servo,
                            PlaySound::at(at).volume(1.3).pitch(0.7).range(10.0, 300.0),
                        );
                    }
                    let _ = on_me;
                }
                _ => {}
            }
        }
    }

    /// Plays the colossus fight's music while a colossus still standing is
    /// near the camera, and fades it out when it falls or the camera leaves.
    /// Call once a frame after [`RobotView::update`].
    pub fn music(&mut self, world: &mut World, all: &[EntityState]) {
        let eye = world.camera().eye();
        let reach = if self.music.playing || self.music.loading.is_some() {
            MUSIC_FAR
        } else {
            MUSIC_NEAR
        };
        let fight = all.iter().any(|e| {
            e.kind == EntityKind::Colossus
                && e.combat.is_none_or(|c| c.status != Status::Down)
                && pos(e).distance(eye) < reach
        });
        if !fight {
            self.music.loading = None;
            if self.music.playing {
                world.stop_music(6.0);
                self.music.playing = false;
            }
            return;
        }
        if self.music.playing {
            return;
        }
        match &self.music.loading {
            None => {
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    engine::parallel::background_thread();
                    let _ = tx.send(Sound::from_bytes(COLOSSUS_THEME.to_vec()));
                });
                self.music.loading = Some(rx);
            }
            Some(rx) => {
                if let Ok(s) = rx.try_recv() {
                    self.music.loading = None;
                    match s {
                        Ok(s) => {
                            world.play_music(MusicTrack::new().layer(s), 2.0);
                            self.music.playing = true;
                        }
                        Err(e) => eprintln!("colossus music: {e}"),
                    }
                }
            }
        }
    }

    /// Is the colossus fight's music on (so nothing else fades it)?
    pub fn music_on(&self) -> bool {
        self.music.playing || self.music.loading.is_some()
    }

    /// A robo-sentry on screen: where it stands and the way it faces
    /// (radians), for the `HW_VIEW=sentry` close-up.
    pub fn sentry_at(&self) -> Option<(Vec3, f32)> {
        self.drawn
            .values()
            .find(|d| d.kind == EntityKind::Sentry)
            .map(|d| (d.last, d.yaw))
    }

    /// Tips for the colossus fight: on first sight, and when my blows glance
    /// off its armour.
    fn tips(
        &mut self,
        fight: &mut FightView,
        events: &[Fx],
        all: &[EntityState],
        me: Option<NetId>,
    ) {
        let Some(me) = me else { return };
        let Some(mine) = all.iter().find(|e| e.id == me).map(pos) else {
            return;
        };
        let near = all
            .iter()
            .find(|e| e.kind == EntityKind::Colossus && pos(e).distance(mine) < 200.0);
        let Some(tank) = near else { return };
        let armoured = tank
            .combat
            .is_some_and(|c| c.effects & EFFECT_ARMOURED != 0 && c.status != Status::Down);
        if !self.told {
            self.told = true;
            self.tip_in = 12.0;
            fight.say(
                "The colossus! Its armour turns blows: break the glowing knees and the cannon.",
            );
            return;
        }
        let glanced = events.iter().any(|e| e.from == me && e.to == tank.id);
        if armoured && glanced && self.tip_in <= 0.0 {
            self.tip_in = 15.0;
            fight.say("Armour! Your blows glance off: aim for the glowing knees.");
        }
    }

    /// A warning lane on the ground from `a` to `b`, `half` wide each side.
    fn lane(
        &mut self,
        world: &mut World,
        a: Vec3,
        b: Vec3,
        half: f32,
        colour: Color,
        seconds: f32,
    ) {
        let len = Vec3::new(b.x - a.x, 0.0, b.z - a.z).length();
        let n = ((len / (half * 1.4)).ceil() as usize).clamp(1, 40);
        for k in 0..=n {
            let p = a.lerp(b, k as f32 / n as f32);
            self.ring(world, p, half, colour, seconds);
        }
    }

    fn ring(&mut self, world: &mut World, at: Vec3, radius: f32, colour: Color, seconds: f32) {
        // On the ground only: a deep, bright decal would paint the robot and
        // everyone standing in it white.
        let d = Decal::new(DecalKind::Power, at + Vec3::Y * 0.2, Vec3::Y, radius)
            .tint(colour)
            .emission(1.4)
            .depth(0.9)
            .fade(0.15, 0.2);
        if let Ok(id) = world.spawn_decal(d) {
            self.rings.push((id, seconds));
        }
    }
}

fn blank() -> geo::Colossus {
    geo::Colossus {
        heroes: 1,
        ..geo::Colossus::default()
    }
}

/// Is my hero within `reach` of `at`?
fn near_me(all: &[EntityState], me: Option<NetId>, at: Vec3, reach: f32) -> bool {
    all.iter()
        .find(|e| Some(e.id) == me)
        .is_some_and(|e| pos(e).distance(at) < reach)
}

/// The Aether lance's beam: a thick violet column from the muzzle down to
/// where it scores the ground.
fn lance_beam(world: &mut World, a: Vec3, b: Vec3) {
    let len = a.distance(b);
    if !len.is_finite() || len < 0.05 {
        return;
    }
    let n = ((len / 0.5) as usize).clamp(2, 200);
    let p = world.particles_mut();
    for i in 0..=n {
        let at = a.lerp(b, i as f32 / n as f32);
        let _ = p.burst(
            ParticleEmitter::new(at)
                .with_mode(ParticleMode::Burst)
                .with_burst_count(2)
                .with_size(0.75)
                .with_size_variation(0.4)
                .with_lifetime(0.12)
                .with_spread(Vec3::splat(0.15))
                .with_velocity_jitter(Vec3::splat(0.6))
                .with_color(rgb(240, 200, 255))
                .with_secondary_color(rgb(150, 50, 255))
                .with_size_over_life(SizeOverLife::Shrink)
                .with_seed(i as u32)
                .with_emissive_intensity(9.0),
        );
    }
}

/// A slow clock for glows that breathe.
fn self_clock() -> f32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64());
    (t % 1000.0) as f32 * 2.5
}

/// What the target frame calls a weak point.
pub fn part_name(all: &[EntityState], robot: NetId, point: NetId) -> &'static str {
    let kind = all.iter().find(|e| e.id == robot).map(|e| e.kind);
    let part = all
        .iter()
        .find(|e| e.id == point)
        .and_then(|e| e.mount)
        .map_or(0, |m| m.1);
    match (kind, part) {
        (Some(EntityKind::Sentry), _) => "Shield projector",
        (_, CANNON) => "Main cannon",
        _ => "Knee actuator",
    }
}

/// Over a weak point's head and the like: how high the health bar floats.
pub fn bar_height(kind: EntityKind) -> f32 {
    match kind {
        EntityKind::Colossus => hull_height(0.0) as f32 + 6.0,
        EntityKind::Sentry => 3.0,
        EntityKind::Alpha => 2.9,
        EntityKind::WeakPoint => 1.9,
        EntityKind::Safe => 1.75,
        _ => 2.15,
    }
}

fn smoke_puff(world: &mut World, at: Vec3, size: f32) {
    let _ = world.particles_mut().burst(
        ParticleEmitter::smoke(at, size)
            .with_mode(ParticleMode::Burst)
            .with_burst_count(2),
    );
}

fn dust(world: &mut World, at: Vec3, radius: f32) {
    let _ = world.particles_mut().burst(
        ParticleEmitter::dust(at + Vec3::Y * 0.3, radius, rgb(150, 140, 120))
            .with_lifetime(3.0)
            .with_acceleration(Vec3::new(0.0, 0.3, 0.0)),
    );
}

fn make_sounds() -> Option<Sounds> {
    // Made with OpenRouter (prompts in assets/robots/README.md).
    let load = |name: &str, b: &'static [u8]| match Sound::from_bytes(b.to_vec()) {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("robot sound {name}: {e}");
            None
        }
    };
    Some(Sounds {
        thud: load("footfall", include_bytes!("../assets/robots/footfall.ogg"))?,
        cannon: load("cannon", include_bytes!("../assets/robots/cannon.ogg"))?,
        klaxon: load("klaxon", include_bytes!("../assets/robots/klaxon.ogg"))?,
        zap: load("zap", include_bytes!("../assets/robots/zap.ogg"))?,
        hum: load("hum", include_bytes!("../assets/robots/hum.ogg"))?,
        servo: load("servo", include_bytes!("../assets/robots/servo.ogg"))?,
        crash: load(
            "partbreak",
            include_bytes!("../assets/robots/partbreak.ogg"),
        )?,
        collapse: load("collapse", include_bytes!("../assets/robots/collapse.ogg"))?,
    })
}
