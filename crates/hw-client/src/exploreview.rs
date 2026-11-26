//! The sights in the game (`hw_gen::urban::sights`, the server's rules in
//! `hw_sim::explore`): the Aether springs and waystone rings out in the
//! country, the wisps that hang over roofs, landmarks, rims and grottos,
//! and the gates of the flight and rooftop courses; what the hero sees and
//! hears of a find or a run, and the marks on the maps.
//!
//! The models are built on a worker thread once the town's sights are
//! planned ([`Parts`]); the main thread only spawns them. Everything that
//! changes while playing (wisps bobbing and vanishing, glyphs brightening
//! while the stones sing, the gates of a run) is an instanced entity moved
//! by its places: nothing is built while it plays.

use crate::exploremesh::{self as em, Detail, Materials, AETHER};
use crate::soundscape::{Bank, EXPLORE_CLIPS};
use crate::uikit;
use engine::egui::{self, pos2, vec2, Align2, Color32, Pos2, Rect, Stroke};
use engine::prelude::*;
use glam::{DVec2, DVec3};
use hw_core::NetId;
use hw_gen::urban::sights::{
    Course, CourseKind, Sights, WispPlace, HOOP_RADIUS, RING_RADIUS, TERRACES,
};
use hw_gen::urban::world::TownWorld;
use hw_net::protocol::{course, sight};
use hw_net::{FindBook, Fx, FxKind};
use std::collections::BTreeSet;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

/// Wisps are drawn within this distance of the camera (m).
const WISP_SIGHT: f32 = 900.0;
/// Start gates are drawn within this distance (m).
const GATE_SIGHT: f32 = 4000.0;
/// After the waystones sing, what lies near shows on the maps this long (s)...
const HINT_S: f64 = 300.0;
/// ... this far from the ring (m), this many things at most.
const HINT_REACH: f64 = 2600.0;
const HINTS: usize = 6;
/// Lamps kept for wisps near the camera.
const WISP_LAMPS: usize = 4;

fn v3(p: DVec3) -> Vec3 {
    Vec3::new(p.x as f32, p.y as f32, p.z as f32)
}

fn yaw_of(d: DVec2) -> f32 {
    d.x.atan2(d.y).to_degrees() as f32
}

fn grey(k: f32) -> Color {
    let v = (k.clamp(0.0, 1.0) * 255.0) as u8;
    Color::rgb(v, v, v)
}

fn scaled(c: (u8, u8, u8), k: f32) -> Color {
    let k = k.clamp(0.0, 1.0);
    let s = |v: u8| (v as f32 * k) as u8;
    Color::rgb(s(c.0), s(c.1), s(c.2))
}

/// A wisp's colour by where it hangs.
fn wisp_colour(p: WispPlace) -> (u8, u8, u8) {
    match p {
        WispPlace::Roof => AETHER,
        WispPlace::Landmark => (255, 214, 120),
        WispPlace::Wild => (150, 255, 190),
        WispPlace::Deep => (200, 150, 255),
    }
}

/// The key of the world a book of finds belongs to (as the server makes it).
pub fn world_key(seed: hw_core::Seed, population: f64) -> u64 {
    seed.0 ^ population.to_bits().rotate_left(17)
}

/// Everything the worker makes: the meshes and where the fixed ones stand.
pub struct Parts {
    sights: Arc<Sights>,
    /// Spring rock, altars, standing stones: levels and places.
    lods: Vec<(Vec<LodLevel>, Vec<Place>)>,
    pool: (Mesh, Vec<Place>),
    crystal: (Mesh, Vec<Place>),
    altar_sign: (Mesh, Vec<Place>),
    /// Glyphs by stone shape: the mesh, and each stone's ring and place.
    glyphs: Vec<(Mesh, Vec<(usize, Place)>)>,
    heart: Mesh,
    halo: Mesh,
    ring: Mesh,
    ring_burst: Mesh,
    ring_band: Mesh,
    hoop: Mesh,
    hoop_burst: Mesh,
    hoop_band: Mesh,
    shaft: Mesh,
}

impl Parts {
    /// Builds everything (slow: waits for the sights' plan; for a worker).
    pub fn build(town: &TownWorld) -> Parts {
        let sights = town.sights().clone();
        let wild = &sights.wild;
        let levels = |f: &dyn Fn(Detail) -> Mesh, reach: [f32; 3]| -> Vec<LodLevel> {
            Detail::ALL
                .iter()
                .zip(reach)
                .map(|(d, r)| LodLevel::new(f(*d), r))
                .collect()
        };
        let mut lods = Vec::new();
        let spring_places: Vec<Place> = wild
            .springs
            .iter()
            .map(|s| {
                Place::new(s.pos.x as f32, s.base as f32, s.pos.y as f32)
                    .with_yaw_deg(yaw_of(s.dir))
            })
            .collect();
        lods.push((
            levels(&em::spring_rock, [70.0, 260.0, 3500.0]),
            spring_places.clone(),
        ));
        let crystals: Vec<Place> = wild
            .springs
            .iter()
            .flat_map(|s| {
                s.crystals()
                    .into_iter()
                    .map(|c| em::crystal_place(s, &c))
                    .collect::<Vec<_>>()
            })
            .collect();
        let ring_places: Vec<Place> = wild
            .stones
            .iter()
            .map(|s| {
                Place::new(s.pos.x as f32, s.base as f32, s.pos.y as f32)
                    .with_yaw_deg(yaw_of(s.dir))
            })
            .collect();
        lods.push((
            levels(&em::altar, [50.0, 200.0, 2500.0]),
            ring_places.clone(),
        ));
        let mut stones: Vec<Vec<Place>> = vec![Vec::new(); em::STONE_SHAPES];
        let mut glyph_at: Vec<Vec<(usize, Place)>> = vec![Vec::new(); em::STONE_SHAPES];
        for (i, s) in wild.stones.iter().enumerate() {
            for (k, m) in s.monoliths(|p| town.ground(p)).into_iter().enumerate() {
                let at = s.to_plan(m.at);
                let face = s.dir.perp() * -m.face.x + s.dir * m.face.y;
                // Each leans a little its own way, as old stones do.
                let h = (i * 7 + k * 13) as f32;
                let place = Place::new(at.x as f32, (s.base + m.foot) as f32, at.y as f32)
                    .with_yaw_deg(yaw_of(face))
                    .with_pitch_deg((h * 1.7).sin() * 2.2)
                    .with_roll_deg((h * 2.3).cos() * 1.8)
                    .with_stretch(Vec3::new(m.half.x as f32, m.height as f32, m.half.y as f32));
                let shape = (i + k) % em::STONE_SHAPES;
                stones[shape].push(place);
                glyph_at[shape].push((i, place));
            }
        }
        let mut glyphs = Vec::new();
        for (shape, (places, marks)) in stones.into_iter().zip(glyph_at).enumerate() {
            lods.push((
                levels(&|d| em::monolith(shape, d), [45.0, 180.0, 3000.0]),
                places,
            ));
            glyphs.push((em::glyphs(shape), marks));
        }
        Parts {
            lods,
            pool: (em::spring_pool(), spring_places),
            crystal: (em::crystal(), crystals),
            altar_sign: (em::altar_sign(), ring_places),
            glyphs,
            heart: em::ball(12, 8),
            halo: em::ball(20, 12),
            ring: em::flight_ring(),
            ring_burst: em::start_burst(RING_RADIUS),
            ring_band: em::finish_band(RING_RADIUS, false),
            hoop: em::roof_hoop(),
            hoop_burst: em::start_burst(HOOP_RADIUS),
            hoop_band: em::finish_band(HOOP_RADIUS, true),
            shaft: em::shaft(),
            sights,
        }
    }
}

struct Live {
    sights: Arc<Sights>,
    mats: Materials,
    lods: Vec<LodId>,
    fixed: Vec<EntityId>,
    glyphs: Vec<(EntityId, Vec<(usize, Place)>)>,
    altar_sign: (EntityId, Vec<Place>),
    heart: EntityId,
    halo: EntityId,
    ring: EntityId,
    ring_burst: EntityId,
    ring_band: EntityId,
    hoop: EntityId,
    hoop_burst: EntityId,
    hoop_band: EntityId,
    shaft: EntityId,
    spring_lamp: LampId,
    stones_lamp: LampId,
    wisp_lamps: Vec<LampId>,
}

/// A run under way, as the server told it.
#[derive(Clone, Copy, Debug)]
struct Run {
    course: u8,
    next: u8,
    /// The frame time it started.
    started: f64,
}

/// A picture on a card or the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    Spring,
    Stones,
    FlightStart,
    RoofStart,
    /// Something not yet found, heard of from the waystones.
    Hint,
    Wisp,
}

/// A mark for the maps.
#[derive(Clone, Debug)]
pub struct MapMark {
    pub at: DVec2,
    pub kind: MarkKind,
    pub name: String,
    /// A course's best medal (0 none, 3 gold).
    pub medal: u8,
}

/// What the hero has found, for the town map's corner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    pub wisps: (usize, usize),
    pub springs: (usize, usize),
    pub stones: (usize, usize),
    pub medals: (usize, usize),
}

struct Card {
    kind: MarkKind,
    medal: u8,
    title: String,
    sub: String,
    born: f64,
    until: f64,
}

/// A hint: which kind of thing and which one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hint {
    Wisp(u16),
    Spring(u8),
    Stones(u8),
}

struct Cue {
    clip: &'static str,
    at: Vec3,
    volume: f32,
    pitch: f32,
    until: f64,
}

pub struct ExploreView {
    key: u64,
    making: Option<Receiver<Parts>>,
    live: Option<Live>,
    run: Option<Run>,
    /// Wisps gathered since the book last came from the server.
    taken: BTreeSet<u16>,
    last_count: usize,
    /// How brightly each ring of stones sings (0..1).
    sing: Vec<f32>,
    hints: Vec<Hint>,
    hints_until: f64,
    cards: Vec<Card>,
    bank: Bank,
    cues: Vec<Cue>,
    now: f64,
    motes_at: f64,
}

impl ExploreView {
    /// Starts the worker that builds the models; `key` is the world's
    /// ([`world_key`]).
    pub fn new(town: &Arc<TownWorld>, key: u64) -> Self {
        let (tx, rx) = channel();
        let t = town.clone();
        let _ = std::thread::Builder::new()
            .name("sights-models".into())
            .spawn(move || {
                engine::parallel::background_thread();
                let _ = tx.send(Parts::build(&t));
            });
        // `HW_RUN=<course>`: a run under way, for screenshots.
        let run = std::env::var("HW_RUN")
            .ok()
            .and_then(|v| v.parse::<u8>().ok())
            .map(|course| Run {
                course,
                next: 1,
                started: 0.0,
            });
        Self {
            key,
            making: Some(rx),
            live: None,
            run,
            taken: BTreeSet::new(),
            last_count: 0,
            sing: Vec::new(),
            hints: Vec::new(),
            hints_until: 0.0,
            cards: Vec::new(),
            bank: Bank::new(EXPLORE_CLIPS),
            cues: Vec::new(),
            now: 0.0,
            motes_at: 0.0,
        }
    }

    /// Are the models in the world?
    pub fn is_live(&self) -> bool {
        self.live.is_some()
    }

    fn sights(&self) -> Option<&Arc<Sights>> {
        self.live.as_ref().map(|l| &l.sights)
    }

    fn has_wisp(&self, finds: &FindBook, i: usize) -> bool {
        self.taken.contains(&(i as u16)) || (finds.world == self.key && finds.has_wisp(i))
    }

    fn has_spring(&self, finds: &FindBook, i: usize) -> bool {
        finds.world == self.key && finds.has_spring(i)
    }

    fn has_stones(&self, finds: &FindBook, i: usize) -> bool {
        finds.world == self.key && finds.has_stones(i)
    }

    fn best(&self, finds: &FindBook, c: usize) -> Option<u16> {
        (finds.world == self.key)
            .then(|| finds.best(c as u8))
            .flatten()
    }

    fn spawn(world: &mut World, p: Parts) -> EngineResult<Live> {
        let mats = em::install_materials(world)?;
        let mut lods = Vec::new();
        for (levels, places) in p.lods {
            lods.push(world.spawn_lod_at(levels, &places)?);
        }
        let glow = |world: &mut World, mesh: Mesh, m: CustomMaterialId| -> EngineResult<EntityId> {
            let id = world.spawn_instanced(mesh);
            world.set_casts_shadow(id, false)?;
            world.set_custom_material(id, m)?;
            Ok(id)
        };
        let mut fixed = Vec::new();
        let pool = glow(world, p.pool.0, mats.pool)?;
        world.set_instances(pool, &p.pool.1)?;
        let crystal = glow(world, p.crystal.0, mats.crystal)?;
        world.set_instances(crystal, &p.crystal.1)?;
        fixed.extend([pool, crystal]);
        let mut glyphs = Vec::new();
        for (mesh, at) in p.glyphs {
            glyphs.push((glow(world, mesh, mats.glyph)?, at));
        }
        let altar_sign = (glow(world, p.altar_sign.0, mats.glyph)?, p.altar_sign.1);
        let plain = |world: &mut World, mesh: Mesh| -> EngineResult<EntityId> {
            let id = world.spawn_instanced(mesh);
            world.set_casts_shadow(id, false)?;
            Ok(id)
        };
        let off = Vec3::new(0.0, -1.0e4, 0.0);
        let lamp =
            |world: &mut World| world.add_lamp(Lamp::point(off, rgb(120, 230, 255), 0.0, 8.0));
        Ok(Live {
            heart: glow(world, p.heart, mats.heart)?,
            halo: glow(world, p.halo, mats.halo)?,
            ring: glow(world, p.ring, mats.gate)?,
            ring_burst: glow(world, p.ring_burst, mats.glyph)?,
            ring_band: plain(world, p.ring_band)?,
            hoop: glow(world, p.hoop, mats.gate)?,
            hoop_burst: glow(world, p.hoop_burst, mats.glyph)?,
            hoop_band: plain(world, p.hoop_band)?,
            shaft: glow(world, p.shaft, mats.shaft)?,
            spring_lamp: lamp(world)?,
            stones_lamp: lamp(world)?,
            wisp_lamps: (0..WISP_LAMPS)
                .map(|_| lamp(world))
                .collect::<EngineResult<_>>()?,
            sights: p.sights,
            mats,
            lods,
            fixed,
            glyphs,
            altar_sign,
        })
    }

    pub fn clear(&mut self, world: &mut World) {
        self.making = None;
        if let Some(l) = self.live.take() {
            for id in l.lods {
                world.despawn_lod(id);
            }
            for id in l
                .fixed
                .into_iter()
                .chain(l.glyphs.into_iter().map(|g| g.0))
                .chain([
                    l.altar_sign.0,
                    l.heart,
                    l.halo,
                    l.ring,
                    l.ring_burst,
                    l.ring_band,
                    l.hoop,
                    l.hoop_burst,
                    l.hoop_band,
                    l.shaft,
                ])
            {
                world.despawn(id);
            }
            for id in l
                .wisp_lamps
                .into_iter()
                .chain([l.spring_lamp, l.stones_lamp])
            {
                world.remove_lamp(id);
            }
        }
    }

    fn cue(&mut self, clip: &'static str, at: Vec3, volume: f32, pitch: f32) {
        self.cues.push(Cue {
            clip,
            at,
            volume,
            pitch,
            until: self.now + 1.5,
        });
    }

    fn card(&mut self, kind: MarkKind, medal: u8, title: String, sub: String, seconds: f64) {
        // One card per kind at a time: a new wisp replaces the last one's.
        self.cards.retain(|c| c.kind != kind);
        self.cards.push(Card {
            kind,
            medal,
            title,
            sub,
            born: self.now,
            until: self.now + seconds,
        });
        if self.cards.len() > 3 {
            self.cards.remove(0);
        }
    }

    /// What happened among the sights this frame: the events about `mine`.
    pub fn on_fx(
        &mut self,
        world: &mut World,
        events: &[Fx],
        mine: Option<NetId>,
        finds: &FindBook,
    ) {
        let Some(sights) = self.sights().cloned() else {
            return;
        };
        for e in events {
            if Some(e.to) != mine {
                continue;
            }
            let at = Vec3::from_array(e.at);
            match e.kind {
                FxKind::Sight => match e.theme {
                    sight::WISP => {
                        let i = e.amount as usize;
                        let Some(w) = sights.wisps.get(i) else {
                            continue;
                        };
                        self.taken.insert(e.amount);
                        let n = (0..sights.wisps.len())
                            .filter(|k| self.has_wisp(finds, *k))
                            .count();
                        let col = wisp_colour(w.place);
                        wisp_burst(world, at, col);
                        // The chime climbs towards each tenth wisp's prize.
                        let step = (n % hw_sim::explore::WISPS_A_PRIZE) as f32;
                        let step = if step == 0.0 { 10.0 } else { step };
                        self.cue("sight_wisp", at, 0.8, 0.9 + step * 0.05);
                        let total = sights.wisps.len();
                        let sub = if n == total {
                            "Every one found!".to_string()
                        } else if n % hw_sim::explore::WISPS_A_PRIZE == 0 {
                            "The Aether gives you an augment".to_string()
                        } else {
                            let to =
                                hw_sim::explore::WISPS_A_PRIZE - n % hw_sim::explore::WISPS_A_PRIZE;
                            format!("{to} more for an augment")
                        };
                        if n % hw_sim::explore::WISPS_A_PRIZE == 0 || n == total {
                            self.cue("fanfare", at, 0.5, 1.0);
                        }
                        self.card(MarkKind::Wisp, 0, format!("Wisp  {n} / {total}"), sub, 4.0);
                    }
                    sight::SPRING => {
                        spring_burst(world, at);
                        self.cue("sight_spring", at, 0.6, 1.0);
                        if e.knock {
                            self.cue("fanfare", at, 0.6, 1.0);
                            let i = e.amount as usize;
                            let n = (0..sights.wild.springs.len())
                                .filter(|k| *k == i || self.has_spring(finds, *k))
                                .count();
                            self.card(
                                MarkKind::Spring,
                                0,
                                "Aether spring found".into(),
                                format!(
                                    "{n} / {}   Its water restores you",
                                    sights.wild.springs.len()
                                ),
                                7.0,
                            );
                        }
                    }
                    sight::STONES => {
                        let i = e.amount as usize;
                        if let Some(s) = self.sing.get_mut(i) {
                            *s = 1.0;
                        }
                        stones_burst(world, at);
                        self.cue("sight_stones", at, 0.9, 1.0);
                        let Some(ring) = sights.wild.stones.get(i) else {
                            continue;
                        };
                        self.hints = self.hints_from(&sights, ring.pos, finds);
                        self.hints_until = self.now + HINT_S;
                        if e.knock {
                            self.cue("fanfare", at, 0.6, 0.9);
                        }
                        let n = (0..sights.wild.stones.len())
                            .filter(|k| *k == i || self.has_stones(finds, *k))
                            .count();
                        let sub = if self.hints.is_empty() {
                            format!(
                                "{n} / {}   Nothing unfound lies near",
                                sights.wild.stones.len()
                            )
                        } else {
                            format!(
                                "{n} / {}   Your map shows what they sing of",
                                sights.wild.stones.len()
                            )
                        };
                        self.card(
                            MarkKind::Stones,
                            0,
                            format!("{} sing", ring.name),
                            sub,
                            if e.knock { 7.0 } else { 4.0 },
                        );
                    }
                    _ => {}
                },
                FxKind::Course => {
                    let Some(c) = sights.courses.get(e.theme as usize) else {
                        continue;
                    };
                    let kind = match c.kind {
                        CourseKind::Flight => MarkKind::FlightStart,
                        CourseKind::Rooftop => MarkKind::RoofStart,
                    };
                    match e.element {
                        course::START => {
                            self.run = Some(Run {
                                course: e.theme,
                                next: 1,
                                started: self.now,
                            });
                            self.cards.retain(|c| {
                                !matches!(c.kind, MarkKind::FlightStart | MarkKind::RoofStart)
                            });
                            gate_burst(world, at, (255, 214, 110));
                            self.cue("whoosh", at, 0.8, 1.2);
                            self.cue("sight_gate", at, 0.7, 0.85);
                        }
                        course::GATE => {
                            if let Some(r) = self.run.as_mut() {
                                r.next = e.amount as u8 + 1;
                            }
                            gate_burst(world, at, AETHER);
                            let k = e.amount as f32 / c.gates.len().max(1) as f32;
                            self.cue("sight_gate", at, 0.8, 0.95 + 0.35 * k);
                        }
                        course::FINISH => {
                            self.run = None;
                            let secs = e.amount as f64 / 10.0;
                            let medal = c.medal(secs);
                            gate_burst(world, at, (255, 214, 110));
                            self.cue("fanfare", at, 0.8, if medal > 0 { 1.0 } else { 0.8 });
                            let sub = if e.defeat {
                                match medal {
                                    3 => "Gold! The Aether gives you an augment",
                                    2 => "Silver medal",
                                    _ => "Bronze medal",
                                }
                            } else if e.knock {
                                "Your best time"
                            } else if medal == 0 {
                                "No medal this time"
                            } else {
                                "Finished"
                            };
                            self.card(
                                kind,
                                medal,
                                format!("{}  {}", c.name, clock(secs)),
                                sub.into(),
                                8.0,
                            );
                        }
                        _ => {
                            self.run = None;
                            self.cue("teleport", at, 0.5, 0.6);
                            self.card(kind, 0, c.name.clone(), "The run is over".into(), 4.0);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// The unfound things nearest `from`.
    fn hints_from(&self, sights: &Sights, from: DVec2, finds: &FindBook) -> Vec<Hint> {
        let mut all: Vec<(f64, Hint)> = Vec::new();
        for (i, s) in sights.wild.springs.iter().enumerate() {
            if !self.has_spring(finds, i) {
                // Springs are the prize: they count as nearer than they are.
                all.push((s.pos.distance(from) * 0.4, Hint::Spring(i as u8)));
            }
        }
        for (i, s) in sights.wild.stones.iter().enumerate() {
            if !self.has_stones(finds, i) && s.pos.distance(from) > 1.0 {
                all.push((s.pos.distance(from) * 0.6, Hint::Stones(i as u8)));
            }
        }
        for (i, w) in sights.wisps.iter().enumerate() {
            if !self.has_wisp(finds, i) {
                all.push((
                    DVec2::new(w.at.x, w.at.z).distance(from),
                    Hint::Wisp(i as u16),
                ));
            }
        }
        all.retain(|(d, _)| *d <= HINT_REACH);
        all.sort_by(|a, b| a.0.total_cmp(&b.0));
        all.truncate(HINTS);
        all.into_iter().map(|(_, h)| h).collect()
    }

    /// A frame: `eye` the camera, `hero` the local hero's feet, `finds`
    /// their book, `now` the frame clock (s).
    pub fn update(
        &mut self,
        world: &mut World,
        eye: Vec3,
        hero: Option<Vec3>,
        finds: &FindBook,
        now: f64,
        dt: f32,
    ) -> EngineResult<()> {
        self.now = now;
        // The clips are asked for early, so the first find is heard.
        for (name, _) in EXPLORE_CLIPS {
            let _ = self.bank.get(name, now);
        }
        let cues = std::mem::take(&mut self.cues);
        for c in cues {
            match self.bank.get(c.clip, now) {
                Some(s) => {
                    world.play_sound(
                        &s,
                        PlaySound::at(c.at)
                            .volume(c.volume)
                            .pitch(c.pitch)
                            .range(6.0, 120.0),
                    );
                }
                None if c.until > now => self.cues.push(c),
                None => {}
            }
        }
        if self.live.is_none() {
            let Some(parts) = self.making.as_ref().and_then(|rx| rx.try_recv().ok()) else {
                return Ok(());
            };
            self.making = None;
            self.sing = vec![0.0; parts.sights.wild.stones.len()];
            self.live = Some(Self::spawn(world, parts)?);
        }
        // The server's book has caught up (or another hero plays now).
        let count = if finds.world == self.key {
            finds.wisp_count()
        } else {
            0
        };
        if count != self.last_count {
            if count < self.last_count {
                self.taken.clear();
            } else {
                let key = self.key;
                self.taken
                    .retain(|i| !(finds.world == key && finds.has_wisp(*i as usize)));
            }
            self.last_count = count;
        }
        if now > self.hints_until {
            self.hints.clear();
        }
        let key = self.key;
        let taken = &self.taken;
        let has_wisp =
            |i: usize| taken.contains(&(i as u16)) || (finds.world == key && finds.has_wisp(i));
        self.hints.retain(|h| match *h {
            Hint::Wisp(i) => !has_wisp(i as usize),
            Hint::Spring(i) => !(finds.world == key && finds.has_spring(i as usize)),
            Hint::Stones(i) => !(finds.world == key && finds.has_stones(i as usize)),
        });
        for s in &mut self.sing {
            *s = (*s - dt / 6.0).max(0.0);
        }
        let live = self.live.as_ref().expect("made above");
        let sights = &live.sights;
        let t = now as f32;

        // Wisps: those not gathered, near enough to see.
        let mut hearts = Vec::new();
        let mut halos = Vec::new();
        let mut nearest: Vec<(f32, Vec3, (u8, u8, u8))> = Vec::new();
        for (i, w) in sights.wisps.iter().enumerate() {
            if has_wisp(i) {
                continue;
            }
            let base = v3(w.at);
            let d = base.distance(eye);
            if d > WISP_SIGHT {
                continue;
            }
            let phase = i as f32 * 1.93;
            let at = base
                + Vec3::new(
                    (t * 0.7 + phase).sin() * 0.12,
                    (t * 1.3 + phase).sin() * 0.22,
                    (t * 0.9 + phase * 1.7).cos() * 0.12,
                );
            let col = wisp_colour(w.place);
            // From afar the halo swells, so a wisp twinkles across a valley.
            let far = 1.0 + (d / 70.0).min(5.0);
            let pulse = 1.0 + 0.12 * (t * 2.4 + phase).sin();
            hearts.push(
                Place::new(at.x, at.y, at.z)
                    .with_scale(0.11 * far.sqrt())
                    .with_tint(scaled(col, 1.0)),
            );
            halos.push(
                Place::new(at.x, at.y, at.z)
                    .with_scale(0.4 * far * pulse)
                    .with_tint(scaled(col, 1.0)),
            );
            nearest.push((d, at, col));
        }
        world.set_instances(live.heart, &hearts)?;
        world.set_instances(live.halo, &halos)?;
        nearest.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (k, lamp) in live.wisp_lamps.iter().enumerate() {
            let l = match nearest.get(k).filter(|n| n.0 < 120.0) {
                Some((_, at, c)) => Lamp::point(*at, rgb(c.0, c.1, c.2), 3.5, 9.0),
                None => Lamp::point(Vec3::new(0.0, -1.0e4, 0.0), rgb(0, 0, 0), 0.0, 1.0),
            };
            world.set_lamp(*lamp, l)?;
        }

        // The nearest spring's light, and motes rising from its pool.
        let spring = sights
            .wild
            .springs
            .iter()
            .map(|s| v3(s.pool()))
            .min_by(|a, b| a.distance(eye).total_cmp(&b.distance(eye)));
        let breath = 0.85 + 0.15 * (t * 0.9).sin();
        match spring.filter(|p| p.distance(eye) < 400.0) {
            Some(p) => {
                world.set_lamp(
                    live.spring_lamp,
                    Lamp::point(
                        p + Vec3::Y * 1.3,
                        rgb(AETHER.0, AETHER.1, AETHER.2),
                        2.5 * breath,
                        14.0,
                    ),
                )?;
                if p.distance(eye) < 120.0 && now >= self.motes_at {
                    self.motes_at = now + 0.35;
                    spring_motes(world, p);
                }
            }
            None => world.set_lamp(
                live.spring_lamp,
                Lamp::point(Vec3::new(0.0, -1.0e4, 0.0), rgb(0, 0, 0), 0.0, 1.0),
            )?,
        }

        // The stones: glyphs low, bright while they sing.
        for (id, at) in &live.glyphs {
            let places: Vec<Place> = at
                .iter()
                .map(|(ring, p)| p.with_tint(grey(0.3 + 0.7 * self.sing[*ring])))
                .collect();
            world.set_instances(*id, &places)?;
        }
        let signs: Vec<Place> = live
            .altar_sign
            .1
            .iter()
            .enumerate()
            .map(|(i, p)| p.with_tint(grey(0.35 + 0.65 * self.sing[i])))
            .collect();
        world.set_instances(live.altar_sign.0, &signs)?;
        let ring = sights
            .wild
            .stones
            .iter()
            .enumerate()
            .map(|(i, s)| (i, v3(s.altar())))
            .min_by(|a, b| a.1.distance(eye).total_cmp(&b.1.distance(eye)));
        match ring.filter(|r| r.1.distance(eye) < 300.0) {
            Some((i, p)) => world.set_lamp(
                live.stones_lamp,
                Lamp::point(
                    p + Vec3::Y * 1.6,
                    rgb(AETHER.0, AETHER.1, AETHER.2),
                    2.5 + 16.0 * self.sing[i],
                    18.0,
                ),
            )?,
            None => world.set_lamp(
                live.stones_lamp,
                Lamp::point(Vec3::new(0.0, -1.0e4, 0.0), rgb(0, 0, 0), 0.0, 1.0),
            )?,
        }

        // Gates: every course's start; on a run, the gates still to come.
        if let Some(r) = self.run {
            // A run the server no longer knows of (we left its world).
            if sights.courses.get(r.course as usize).is_none() {
                self.run = None;
            }
        }
        let gold = (255, 214, 110);
        let (mut rings, mut ring_bursts, mut ring_bands) = (Vec::new(), Vec::new(), Vec::new());
        let (mut hoops, mut hoop_bursts, mut hoop_bands, mut shafts) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let around = hero.unwrap_or(eye);
        for (ci, c) in sights.courses.iter().enumerate() {
            let run = self.run.filter(|r| r.course as usize == ci);
            let from = run.map_or(0, |r| r.next as usize);
            let to = if run.is_some() { c.gates.len() } else { 1 };
            for (k, g) in c.gates.iter().enumerate().take(to).skip(from) {
                let at = v3(g.at);
                if run.is_none() && at.distance(around) > GATE_SIGHT {
                    continue;
                }
                let next = run.is_some() && k == from;
                let last = k + 1 == c.gates.len();
                let tint = if run.is_none() {
                    scaled(gold, 1.0)
                } else if next {
                    Color::rgb(215, 250, 255)
                } else {
                    scaled(AETHER, 0.32)
                };
                let swell = if next {
                    1.0 + 0.03 * (t * 6.0).sin()
                } else {
                    1.0
                };
                match c.kind {
                    CourseKind::Flight => {
                        let p = Place::new(at.x, at.y, at.z).with_yaw_deg(yaw_of(g.dir));
                        rings.push(p.with_scale(swell).with_tint(tint));
                        if run.is_none() {
                            ring_bursts.push(p.with_roll_deg(t * 6.0).with_tint(scaled(gold, 0.9)));
                        }
                        if last {
                            ring_bands.push(p);
                        }
                    }
                    CourseKind::Rooftop => {
                        let p = Place::new(at.x, at.y, at.z);
                        hoops.push(p.with_scale(swell).with_tint(tint));
                        let tall = (g.at.y - g.floor).max(0.5) as f32;
                        let r = HOOP_RADIUS as f32 * 0.94;
                        shafts.push(p.with_stretch(Vec3::new(r, tall, r)).with_tint(
                            if run.is_none() {
                                scaled(gold, 0.8)
                            } else if next {
                                scaled(AETHER, 1.0)
                            } else {
                                scaled(AETHER, 0.25)
                            },
                        ));
                        if run.is_none() {
                            hoop_bursts.push(
                                p.with_pitch_deg(90.0)
                                    .with_yaw_deg(t * 6.0)
                                    .with_tint(scaled(gold, 0.9)),
                            );
                        }
                        if last {
                            hoop_bands.push(p);
                        }
                    }
                }
            }
        }
        world.set_instances(live.ring, &rings)?;
        world.set_instances(live.ring_burst, &ring_bursts)?;
        world.set_instances(live.ring_band, &ring_bands)?;
        world.set_instances(live.hoop, &hoops)?;
        world.set_instances(live.hoop_burst, &hoop_bursts)?;
        world.set_instances(live.hoop_band, &hoop_bands)?;
        world.set_instances(live.shaft, &shafts)?;
        let _ = live.mats;
        Ok(())
    }

    /// The marks for the maps: every course's start, the springs and rings
    /// found, and what the waystones sang of.
    pub fn marks(&self, finds: &FindBook) -> Vec<MapMark> {
        let Some(sights) = self.sights() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (i, c) in sights.courses.iter().enumerate() {
            let g = c.gates[0].at;
            out.push(MapMark {
                at: DVec2::new(g.x, g.z),
                kind: match c.kind {
                    CourseKind::Flight => MarkKind::FlightStart,
                    CourseKind::Rooftop => MarkKind::RoofStart,
                },
                name: c.name.clone(),
                medal: self.best(finds, i).map_or(0, |t| c.medal(t as f64 / 10.0)),
            });
        }
        for (i, s) in sights.wild.springs.iter().enumerate() {
            if self.has_spring(finds, i) {
                out.push(MapMark {
                    at: s.pos,
                    kind: MarkKind::Spring,
                    name: "Aether spring".into(),
                    medal: 0,
                });
            }
        }
        for (i, s) in sights.wild.stones.iter().enumerate() {
            if self.has_stones(finds, i) {
                out.push(MapMark {
                    at: s.pos,
                    kind: MarkKind::Stones,
                    name: s.name.clone(),
                    medal: 0,
                });
            }
        }
        for h in &self.hints {
            let at = match *h {
                Hint::Wisp(i) => sights
                    .wisps
                    .get(i as usize)
                    .map(|w| DVec2::new(w.at.x, w.at.z)),
                Hint::Spring(i) => sights.wild.springs.get(i as usize).map(|s| s.pos),
                Hint::Stones(i) => sights.wild.stones.get(i as usize).map(|s| s.pos),
            };
            if let Some(at) = at {
                out.push(MapMark {
                    at,
                    kind: MarkKind::Hint,
                    name: String::new(),
                    medal: 0,
                });
            }
        }
        out
    }

    /// What the hero has found so far.
    pub fn tally(&self, finds: &FindBook) -> Option<Tally> {
        let s = self.sights()?;
        let medals: usize = s
            .courses
            .iter()
            .enumerate()
            .map(|(i, c)| {
                self.best(finds, i)
                    .map_or(0, |t| c.medal(t as f64 / 10.0) as usize)
            })
            .sum();
        Some(Tally {
            wisps: (
                (0..s.wisps.len())
                    .filter(|i| self.has_wisp(finds, *i))
                    .count(),
                s.wisps.len(),
            ),
            springs: (
                (0..s.wild.springs.len())
                    .filter(|i| self.has_spring(finds, *i))
                    .count(),
                s.wild.springs.len(),
            ),
            stones: (
                (0..s.wild.stones.len())
                    .filter(|i| self.has_stones(finds, *i))
                    .count(),
                s.wild.stones.len(),
            ),
            medals: (medals, s.courses.len() * 3),
        })
    }

    /// The run's panel and the cards of finds.
    pub fn ui(&mut self, ctx: &egui::Context, finds: &FindBook) {
        let now = self.now;
        self.cards.retain(|c| c.until > now);
        let screen = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new("explore hud"),
        ));
        let mut y = screen.top() + screen.height() * 0.13;
        if let (Some(r), Some(sights)) = (self.run, self.sights()) {
            if let Some(c) = sights.courses.get(r.course as usize) {
                let best = self.best(finds, r.course as usize);
                run_panel(&painter, ctx, pos2(screen.center().x, y), c, &r, now, best);
                y += 92.0;
            }
        }
        for c in &self.cards {
            let age = (now - c.born) as f32;
            let left = (c.until - now) as f32;
            let a = (age / 0.25).min(1.0).min(left / 0.6).clamp(0.0, 1.0);
            let rise = (1.0 - (age / 0.25).min(1.0)) * 14.0;
            card(
                &painter,
                ctx,
                pos2(screen.center().x, y + rise),
                c,
                a,
                now as f32,
            );
            y += 64.0;
        }
    }
}

/// "1:23.4".
pub fn clock(seconds: f64) -> String {
    let tenths = (seconds * 10.0).round() as u64;
    format!("{}:{:02}.{}", tenths / 600, tenths / 10 % 60, tenths % 10)
}

const MEDALS: [Color32; 3] = [
    Color32::from_rgb(205, 127, 60),
    Color32::from_rgb(206, 214, 226),
    Color32::from_rgb(255, 198, 64),
];

/// A medal: a disc on a ribbon; hollow when not yet won.
pub fn medal_disc(painter: &egui::Painter, p: Pos2, r: f32, medal: u8, won: bool, alpha: f32) {
    let col = uikit::with_alpha(MEDALS[(medal.clamp(1, 3) - 1) as usize], alpha);
    let ink = uikit::with_alpha(uikit::DEEP, alpha);
    if won {
        for (dx, c) in [(-0.45, uikit::HERO_RED), (0.45, uikit::AETHER)] {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    p + vec2(dx * r - 0.3 * r, -r * 1.7),
                    p + vec2(dx * r + 0.3 * r, -r * 1.7),
                    p + vec2(dx * r * 0.4 + 0.3 * r, -r * 0.5),
                    p + vec2(dx * r * 0.4 - 0.3 * r, -r * 0.5),
                ],
                uikit::with_alpha(c, alpha),
                Stroke::NONE,
            ));
        }
        painter.circle_filled(p, r, col);
        painter.circle_stroke(p, r, Stroke::new(1.5_f32, ink));
        uikit::star(
            painter,
            p,
            r * 0.62,
            0.0,
            uikit::with_alpha(Color32::WHITE, alpha * 0.9),
        );
    } else {
        painter.circle_filled(p, r, uikit::with_alpha(uikit::DEEP, alpha * 0.6));
        painter.circle_stroke(p, r, Stroke::new(1.6_f32, col));
    }
}

/// A sight's sign, `r` in radius round `p`.
pub fn draw_mark(painter: &egui::Painter, p: Pos2, r: f32, kind: MarkKind, t: f32, alpha: f32) {
    let aether = uikit::with_alpha(uikit::AETHER, alpha);
    let ink = uikit::with_alpha(uikit::DEEP, alpha);
    let gold = uikit::with_alpha(uikit::GOLD, alpha);
    match kind {
        MarkKind::Spring => {
            // A drop over three ripples.
            painter.circle_filled(p, r, ink);
            painter.circle_stroke(p, r, Stroke::new(1.4_f32, aether));
            painter.circle_filled(p + vec2(0.0, r * 0.12), r * 0.4, aether);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    p + vec2(0.0, -r * 0.72),
                    p + vec2(r * 0.36, r * 0.0),
                    p + vec2(-r * 0.36, r * 0.0),
                ],
                aether,
                Stroke::NONE,
            ));
        }
        MarkKind::Stones => {
            // Three standing stones.
            painter.circle_filled(p, r, ink);
            painter.circle_stroke(p, r, Stroke::new(1.4_f32, aether));
            for (dx, h) in [(-0.45, 0.7), (0.0, 1.0), (0.45, 0.6)] {
                let w = r * 0.26;
                painter.rect_filled(
                    Rect::from_min_max(
                        p + vec2(dx * r - w * 0.5, r * 0.5 - h * r),
                        p + vec2(dx * r + w * 0.5, r * 0.5),
                    ),
                    w * 0.3,
                    aether,
                );
            }
        }
        MarkKind::FlightStart => {
            // A ring with wings.
            painter.circle_filled(p, r, ink);
            painter.circle_stroke(p, r * 0.62, Stroke::new((r * 0.24).max(1.5), gold));
            for s in [-1.0_f32, 1.0] {
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        p + vec2(s * r * 0.75, -r * 0.1),
                        p + vec2(s * r * 1.5, -r * 0.5),
                        p + vec2(s * r * 0.95, r * 0.3),
                    ],
                    gold,
                    Stroke::NONE,
                ));
            }
        }
        MarkKind::RoofStart => {
            // A hoop over a roof's edge, with a leap's arc.
            painter.circle_filled(p, r, ink);
            painter.circle_stroke(p, r, Stroke::new(1.4_f32, gold));
            painter.rect_filled(
                Rect::from_min_max(p + vec2(-r * 0.6, r * 0.2), p + vec2(-r * 0.1, r * 0.62)),
                0.0,
                gold,
            );
            painter.rect_filled(
                Rect::from_min_max(p + vec2(r * 0.2, r * 0.0), p + vec2(r * 0.62, r * 0.62)),
                0.0,
                gold,
            );
            let pts: Vec<Pos2> = (0..=8)
                .map(|i| {
                    let u = i as f32 / 8.0;
                    p + vec2(-r * 0.4 + u * r * 0.8, r * 0.05 - (u * (1.0 - u)) * r * 2.2)
                })
                .collect();
            painter.add(egui::Shape::line(pts, Stroke::new(1.4_f32, gold)));
        }
        MarkKind::Hint => {
            // A star that beats: something waits here.
            let beat = 0.75 + 0.25 * (t * 4.0).sin();
            painter.circle_filled(
                p,
                r * 1.5 * beat,
                uikit::with_alpha(uikit::AETHER, alpha * 0.25),
            );
            uikit::star(painter, p, r * 1.25 * beat, t * 0.6, ink);
            uikit::star(painter, p, r * beat, t * 0.6, aether);
        }
        MarkKind::Wisp => {
            painter.circle_filled(p, r, uikit::with_alpha(uikit::AETHER, alpha * 0.3));
            painter.circle_filled(p, r * 0.5, aether);
            uikit::star(
                painter,
                p,
                r * 1.05,
                t * 0.8,
                uikit::with_alpha(Color32::WHITE, alpha * 0.9),
            );
        }
    }
}

fn plate(painter: &egui::Painter, r: Rect, alpha: f32, edge: Color32) {
    painter.rect_filled(
        r.translate(vec2(3.0, 3.0)),
        10.0,
        Color32::from_black_alpha((90.0 * alpha) as u8),
    );
    painter.rect_filled(r, 10.0, uikit::with_alpha(uikit::NAVY, alpha * 0.92));
    painter.rect_stroke(
        r,
        10.0,
        Stroke::new(2.0_f32, uikit::with_alpha(edge, alpha)),
        egui::StrokeKind::Inside,
    );
}

fn card(painter: &egui::Painter, ctx: &egui::Context, top: Pos2, c: &Card, alpha: f32, t: f32) {
    let title = uikit::ui_font(ctx, 19.0);
    let sub = uikit::ui_font(ctx, 13.5);
    let wt = painter
        .layout_no_wrap(c.title.clone(), title.clone(), Color32::WHITE)
        .size()
        .x;
    let ws = painter
        .layout_no_wrap(c.sub.clone(), sub.clone(), Color32::WHITE)
        .size()
        .x;
    let w = wt.max(ws) + 78.0;
    let r = Rect::from_min_size(pos2(top.x - w * 0.5, top.y), vec2(w, 54.0));
    let edge = if matches!(c.kind, MarkKind::FlightStart | MarkKind::RoofStart) {
        uikit::GOLD
    } else {
        uikit::AETHER
    };
    plate(painter, r, alpha, edge);
    let icon = pos2(r.left() + 30.0, r.center().y);
    if c.medal > 0 {
        medal_disc(painter, icon + vec2(0.0, 5.0), 13.0, c.medal, true, alpha);
    } else {
        draw_mark(painter, icon, 15.0, c.kind, t, alpha);
    }
    painter.text(
        pos2(r.left() + 60.0, r.top() + 8.0),
        Align2::LEFT_TOP,
        &c.title,
        title,
        uikit::with_alpha(uikit::CREAM, alpha),
    );
    painter.text(
        pos2(r.left() + 60.0, r.top() + 31.0),
        Align2::LEFT_TOP,
        &c.sub,
        sub,
        uikit::with_alpha(uikit::DIM, alpha),
    );
}

/// A run's panel: the course, the clock, the gates passed, and the three
/// medal times (those already out of reach struck dim).
fn run_panel(
    painter: &egui::Painter,
    ctx: &egui::Context,
    top: Pos2,
    c: &Course,
    r: &Run,
    now: f64,
    best: Option<u16>,
) {
    let took = (now - r.started).max(0.0);
    let w = 330.0;
    let rect = Rect::from_min_size(pos2(top.x - w * 0.5, top.y), vec2(w, 82.0));
    plate(painter, rect, 1.0, uikit::GOLD);
    let kind = match c.kind {
        CourseKind::Flight => MarkKind::FlightStart,
        CourseKind::Rooftop => MarkKind::RoofStart,
    };
    draw_mark(
        painter,
        pos2(rect.left() + 26.0, rect.top() + 22.0),
        12.0,
        kind,
        0.0,
        1.0,
    );
    uikit::fit_text(
        painter,
        pos2(rect.left() + 48.0, rect.top() + 10.0),
        &c.name,
        uikit::ui_font(ctx, 15.0),
        uikit::CREAM,
        w - 150.0,
    );
    painter.text(
        pos2(rect.right() - 14.0, rect.top() + 6.0),
        Align2::RIGHT_TOP,
        clock(took),
        uikit::logo_font(ctx, 26.0),
        uikit::GOLD_LIGHT,
    );
    // The gates as beads: passed gold, the next beating, the rest hollow.
    let n = c.gates.len();
    let step = ((w - 40.0) / n.max(1) as f32).min(18.0);
    let x0 = rect.left() + 22.0;
    let y = rect.top() + 46.0;
    for k in 0..n {
        let p = pos2(x0 + step * k as f32, y);
        if k < r.next as usize {
            painter.circle_filled(p, 4.5, uikit::GOLD);
        } else if k == r.next as usize {
            let beat = 4.5 + 1.5 * ((now * 6.0).sin() as f32);
            painter.circle_filled(p, beat, uikit::AETHER);
        } else {
            painter.circle_stroke(p, 4.0, Stroke::new(1.3_f32, uikit::DIM));
        }
    }
    // Medal times.
    let font = uikit::ui_font(ctx, 12.5);
    for (k, par) in c.pars.iter().enumerate() {
        let medal = 3 - k as u8;
        let x = rect.left() + 26.0 + 86.0 * k as f32;
        let open = took <= *par;
        medal_disc(
            painter,
            pos2(x, rect.top() + 67.0),
            6.5,
            medal,
            false,
            if open { 1.0 } else { 0.35 },
        );
        painter.text(
            pos2(x + 11.0, rect.top() + 67.0),
            Align2::LEFT_CENTER,
            clock(*par),
            font.clone(),
            if open {
                uikit::CREAM
            } else {
                uikit::with_alpha(uikit::DIM, 0.5)
            },
        );
    }
    if let Some(b) = best {
        painter.text(
            pos2(rect.right() - 14.0, rect.top() + 67.0),
            Align2::RIGHT_CENTER,
            format!("best {}", clock(b as f64 / 10.0)),
            font,
            uikit::AETHER,
        );
    }
}

/// The explorer's tally, for a corner of the town map.
pub fn draw_tally(
    painter: &egui::Painter,
    ctx: &egui::Context,
    corner: Pos2,
    t: &Tally,
    time: f32,
) {
    let rows: [(MarkKind, (usize, usize)); 4] = [
        (MarkKind::Wisp, t.wisps),
        (MarkKind::Spring, t.springs),
        (MarkKind::Stones, t.stones),
        (MarkKind::FlightStart, t.medals),
    ];
    let w = 92.0 * rows.len() as f32 + 12.0;
    let r = Rect::from_min_size(pos2(corner.x, corner.y - 44.0), vec2(w, 36.0));
    plate(painter, r, 1.0, uikit::GOLD_DARK);
    let font = uikit::ui_font(ctx, 14.0);
    for (k, (kind, (n, of))) in rows.into_iter().enumerate() {
        let x = r.left() + 22.0 + 92.0 * k as f32;
        if kind == MarkKind::FlightStart {
            medal_disc(painter, pos2(x, r.center().y + 3.0), 8.0, 3, true, 1.0);
        } else {
            draw_mark(painter, pos2(x, r.center().y), 9.0, kind, time, 1.0);
        }
        painter.text(
            pos2(x + 15.0, r.center().y),
            Align2::LEFT_CENTER,
            format!("{n} / {of}"),
            font.clone(),
            if n == of && of > 0 {
                uikit::GOLD_LIGHT
            } else {
                uikit::CREAM
            },
        );
    }
}

fn motes(at: Vec3, colour: (u8, u8, u8), count: u32) -> ParticleEmitter {
    ParticleEmitter::new(at)
        .with_mode(ParticleMode::Burst)
        .with_burst_count(count)
        .with_shape(ParticleShape::Sphere)
        .with_color(rgb(colour.0, colour.1, colour.2))
        .with_secondary_color(rgb(255, 255, 255))
        .with_silhouette(ParticleSilhouette::RuneMote)
        .with_emissive_intensity(5.0)
}

fn wisp_burst(world: &mut World, at: Vec3, colour: (u8, u8, u8)) {
    let p = world.particles_mut();
    let _ = p.burst(
        motes(at, colour, 26)
            .with_size(0.07)
            .with_lifetime(1.1)
            .with_spread(Vec3::splat(0.25))
            .with_velocity(Vec3::Y * 1.2)
            .with_velocity_jitter(Vec3::splat(2.6)),
    );
    let _ = p.burst(
        ParticleEmitter::new(at)
            .with_mode(ParticleMode::Burst)
            .with_burst_count(1)
            .with_size(1.6)
            .with_lifetime(0.45)
            .with_spread(Vec3::ZERO)
            .with_color(rgb(colour.0, colour.1, colour.2))
            .with_size_over_life(SizeOverLife::GrowThenFade)
            .with_emissive_intensity(5.0),
    );
}

fn spring_motes(world: &mut World, pool: Vec3) {
    let _ = world.particles_mut().burst(
        motes(pool + Vec3::Y * 0.1, AETHER, 3)
            .with_size(0.05)
            .with_lifetime(3.2)
            .with_spread(Vec3::new(2.2, 0.05, 2.2))
            .with_velocity(Vec3::Y * 0.7)
            .with_velocity_jitter(Vec3::new(0.12, 0.3, 0.12)),
    );
}

fn spring_burst(world: &mut World, at: Vec3) {
    let _ = world.particles_mut().burst(
        motes(at + Vec3::Y * 0.2, AETHER, 60)
            .with_size(0.07)
            .with_lifetime(2.0)
            .with_spread(Vec3::new(1.6, 0.1, 1.6))
            .with_velocity(Vec3::Y * 2.6)
            .with_velocity_jitter(Vec3::new(0.5, 1.2, 0.5)),
    );
}

fn stones_burst(world: &mut World, altar: Vec3) {
    let r = hw_gen::urban::sights::RING_R as f32;
    let _ = world.particles_mut().burst(
        motes(altar + Vec3::Y * 1.2, AETHER, 90)
            .with_size(0.08)
            .with_lifetime(2.6)
            .with_spread(Vec3::new(r, 1.4, r))
            .with_velocity(Vec3::Y * 0.9)
            .with_velocity_jitter(Vec3::new(0.2, 0.6, 0.2))
            .with_force(ParticleForce::Radial {
                center: altar + Vec3::Y * 1.5,
                strength: -2.2,
            }),
    );
}

fn gate_burst(world: &mut World, at: Vec3, colour: (u8, u8, u8)) {
    let _ = world.particles_mut().burst(
        motes(at, colour, 40)
            .with_size(0.09)
            .with_lifetime(0.9)
            .with_spread(Vec3::splat(1.5))
            .with_velocity_jitter(Vec3::splat(5.0)),
    );
}

/// `HW_VIEW` cameras on the sights (eye, yaw and pitch in degrees):
/// `spring`, `springclose`, `stones`, `stonesclose`, `wisp`, `race`,
/// `ringclose`, `roofrun`, `hoopclose`. `HW_SIGHT=<n>` picks which one of its kind.
pub fn view(town: &TownWorld, name: &str) -> Option<(Vec3, f32, f32)> {
    let n: usize = std::env::var("HW_SIGHT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let look = |eye: DVec3, at: DVec3| {
        let d = at - eye;
        let yaw = d.x.atan2(d.z).to_degrees() as f32;
        let pitch = (d.y / DVec2::new(d.x, d.z).length().max(1e-6))
            .atan()
            .to_degrees() as f32;
        Some((v3(eye), yaw, pitch))
    };
    let up = |p: DVec2, y: f64| DVec3::new(p.x, y, p.y);
    match name {
        "spring" | "springclose" => {
            let wild = town.wild_sights();
            let s = wild.springs.get(n % wild.springs.len().max(1))?;
            if name == "spring" {
                let e = s.pos + s.dir * 12.5 + s.dir.perp() * 4.0;
                look(
                    up(e, town.ground(e).max(s.base) + 3.4),
                    up(s.pos, s.base + 1.9),
                )
            } else {
                let c = s
                    .crystals()
                    .into_iter()
                    .max_by(|a, b| a.height.total_cmp(&b.height))?;
                let at = s.to_plan(c.at);
                let e = s.pos + (s.pos - at).normalize_or(s.dir) * 2.4;
                look(
                    up(e, s.base + TERRACES[3].1 + 1.6),
                    up(at, s.base + c.foot + c.height * 0.4),
                )
            }
        }
        "stones" | "stonesclose" => {
            let wild = town.wild_sights();
            let s = wild.stones.get(n % wild.stones.len().max(1))?;
            if name == "stones" {
                let e = s.pos + s.dir * 15.0 + s.dir.perp() * 3.0;
                look(
                    up(e, town.ground(e).max(s.base) + 2.4),
                    up(s.pos, s.base + 1.8),
                )
            } else {
                let m = s.monoliths(|p| town.ground(p))[3];
                let at = s.to_plan(m.at);
                let e = at + (s.pos - at).normalize_or(DVec2::Y) * 4.2;
                look(
                    up(e, s.base + 1.7),
                    up(at, s.base + m.foot + m.height * 0.5),
                )
            }
        }
        "wisp" => {
            let sights = town.sights();
            let roofs: Vec<_> = sights
                .wisps
                .iter()
                .filter(|w| w.place == WispPlace::Roof)
                .collect();
            let w = roofs.get(n % roofs.len().max(1))?;
            look(w.at + DVec3::new(2.6, 0.9, 2.2), w.at)
        }
        "race" | "ringclose" | "roofrun" | "hoopclose" => {
            let sights = town.sights();
            let want = if matches!(name, "race" | "ringclose") {
                CourseKind::Flight
            } else {
                CourseKind::Rooftop
            };
            let all: Vec<_> = sights.courses.iter().filter(|c| c.kind == want).collect();
            let c = all.get(n % all.len().max(1))?;
            let g = c.gates[0];
            let side = DVec3::new(g.dir.y, 0.0, -g.dir.x);
            let fwd = DVec3::new(g.dir.x, 0.0, g.dir.y);
            match name {
                "race" => look(
                    g.at - fwd * 30.0 + side * 7.0 + DVec3::Y * 3.0,
                    g.at + fwd * 60.0,
                ),
                "ringclose" => look(
                    g.at + side * (RING_RADIUS - 3.5) - fwd * 4.0 + DVec3::Y * 1.0,
                    g.at + side * RING_RADIUS + DVec3::Y * 2.5,
                ),
                "roofrun" => look(
                    g.at - fwd * 13.0 + side * 5.0 + DVec3::Y * 5.0,
                    g.at + fwd * 8.0,
                ),
                _ => look(
                    g.at - fwd * (HOOP_RADIUS + 3.0) + DVec3::Y * 0.6,
                    g.at - fwd * HOOP_RADIUS,
                ),
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clock_reads_minutes_seconds_and_tenths() {
        assert_eq!(clock(0.0), "0:00.0");
        assert_eq!(clock(83.44), "1:23.4");
        assert_eq!(clock(59.96), "1:00.0");
    }

    #[test]
    fn the_world_key_is_the_servers() {
        let town = hw_sim::GeneratedTown::generate(hw_core::Seed(3), 600.0);
        let sim = hw_sim::Sim::for_town(hw_core::Seed(3), &town);
        assert_eq!(
            world_key(hw_core::Seed(3), town.population()),
            sim.world_key()
        );
    }
}
