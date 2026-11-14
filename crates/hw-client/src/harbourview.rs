//! The harbour alive: its ships, moved from the game clock by the same
//! plan they collide by ([`Harbour::ships_at`], [`Harbour::moored`]), so
//! every player sees them in the same place. The freighter lies at its
//! berth under the cranes, backs out with the tug pulling at its bow, turns
//! and sails a wide loop out to sea and back; coasters pass far out on
//! their lanes; the liner and the launches ride at their moorings. Every
//! ship bobs a little on the swell, and the freighter sounds its horn as it
//! casts off and as it comes home. The light on the breakwater's head
//! flashes red.
//!
//! The harbour's fixed parts ([`models`]) are LOD sets queued with the
//! town's other extras and made on their worker; the ships' meshes are made
//! on a worker thread of their own when the view starts, and the ships are
//! always in the world (their far levels cost a few hundred triangles each)
//! so they can be seen at sea from anywhere along the coast.

use crate::carmesh::Lod;
use crate::harbourmesh as hm;
use engine::lamps::{Lamp, LampId};
use engine::prelude::*;
use glam::{DVec2, DVec3, Vec3};
use hw_gen::urban::harbour::*;
use hw_gen::urban::props::LampStyle;
use hw_gen::urban::world::{TownWorld, WATER_LEVEL};
use std::sync::mpsc::{channel, Receiver};

/// `HW_HARBOUR_T`: game seconds the views hold the harbour's clock at (0:
/// the freighter just moored), if set.
pub fn view_t() -> f64 {
    std::env::var("HW_HARBOUR_T")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(120.0)
}

/// The views that hold the harbour's clock.
const VIEWS: [&str; 5] = [
    "harbour",
    "cargoquay",
    "freighter",
    "shipclose",
    "breakwater",
];

/// Where the harbour's fixed mesh stands: in the middle of mole and
/// breakwater, on the deck.
fn origin(h: &Harbour) -> DVec3 {
    let p = h.to_plan(DVec2::new(60.0, 125.0));
    DVec3::new(p.x, h.deck, p.y)
}

fn place(p: DVec2, y: f64, heading: DVec2) -> Place {
    Place::new(p.x as f32, y as f32, p.y as f32).with_yaw_deg(yaw_of(heading).to_degrees() as f32)
}

/// The harbour's fixed parts as LOD sets and where each stands: the quays,
/// breakwater, towers and light (one mesh per texture near, one silhouette
/// far), the cranes, the bollards, the lamps and the crate stacks. Made
/// without a `World`, on the extras' worker.
pub fn models(town: &TownWorld) -> Vec<(Vec<LodLevel>, Vec<Place>)> {
    let Some(h) = &town.harbour else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let o = origin(h);
    let at = vec![Place::new(o.x as f32, o.y as f32, o.z as f32)];
    let mut far = Some((
        hm::fixed(h, town, o, Lod::Mid)
            .into_iter()
            .next()
            .expect("a mesh"),
        hm::fixed(h, town, o, Lod::Far)
            .into_iter()
            .next()
            .expect("a mesh"),
    ));
    for mesh in hm::fixed(h, town, o, Lod::Near) {
        let mut lod = vec![LodLevel::new(mesh, 520.0)];
        if let Some((m, f)) = far.take() {
            lod.push(LodLevel::new(m, 2200.0));
            lod.push(LodLevel::new(f, 9000.0));
        }
        out.push((lod, at.clone()));
    }
    // The cranes: the portals, and the turning parts with their booms.
    let cranes = h.cranes();
    let portals: Vec<Place> = cranes
        .iter()
        .map(|&(m, o, _)| place(m, h.deck, o))
        .collect();
    let tops: Vec<Place> = cranes
        .iter()
        .map(|&(m, _, b)| place(m, h.deck, b))
        .collect();
    out.push((hm::levels(hm::crane_portal, 260.0, 1000.0, 6000.0), portals));
    out.push((hm::levels(hm::crane_top, 260.0, 1200.0, 7000.0), tops));
    // Bollards along both quays, facing nowhere in particular.
    let bollards: Vec<Place> = h
        .bollards()
        .iter()
        .map(|&b| place(b, h.deck, h.dir))
        .collect();
    out.push((
        hm::levels(hm::mooring_bollard, 50.0, 160.0, 400.0),
        bollards,
    ));
    // The quay's lamps: the town's atomic street lamp, arm out to the berth.
    let lamps: Vec<Place> = h
        .lamps()
        .iter()
        .map(|&(p, arm)| place(p, h.deck, arm))
        .collect();
    out.push((
        hm::levels(
            |lod| crate::propmesh::lamp(LampStyle::Atomic, lod),
            120.0,
            500.0,
            1800.0,
        ),
        lamps,
    ));
    for (x, z, hx, hz, ht) in CRATES {
        let p = h.to_plan(DVec2::new(x, z));
        out.push((
            hm::levels(|lod| hm::crates(hx, hz, ht, lod), 140.0, 500.0, 1400.0),
            vec![place(p, h.deck, h.dir)],
        ));
    }
    out
}

/// What the soundscape hears of the harbour since the last frame: the
/// freighter's horn.
#[derive(Clone, Debug, Default)]
pub struct Heard {
    pub horns: Vec<Vec3>,
}

const KINDS: [ShipKind; 4] = [
    ShipKind::Freighter,
    ShipKind::Liner,
    ShipKind::Tug,
    ShipKind::Launch,
];

pub struct HarbourView {
    harbour: Option<Harbour>,
    /// The ships' meshes from the worker, per kind of [`KINDS`].
    worker: Option<Receiver<Vec<Vec<LodLevel>>>>,
    /// A LOD set per kind of [`KINDS`], once made.
    sets: Vec<LodId>,
    light: Option<LampId>,
    /// Held clock for screenshots.
    held: Option<f64>,
    last_phase: Option<Phase>,
    heard: Heard,
}

impl HarbourView {
    pub fn new(town: &TownWorld) -> Self {
        let harbour = town.harbour.clone();
        let worker = harbour.as_ref().map(|_| {
            let (tx, rx) = channel();
            std::thread::Builder::new()
                .name("harbour ships".into())
                .spawn(move || {
                    engine::parallel::background_thread();
                    let sets: Vec<Vec<LodLevel>> = KINDS
                        .iter()
                        .map(|&k| crate::shipmesh::ship_levels(k))
                        .collect();
                    // Nobody waiting any more (the town was left): fine.
                    let _ = tx.send(sets);
                })
                .expect("harbour ships worker");
            rx
        });
        let held = harbour.as_ref().and_then(|_| {
            let set = std::env::var("HW_HARBOUR_T").is_ok()
                || std::env::var("HW_VIEW").is_ok_and(|v| VIEWS.contains(&v.as_str()));
            set.then(view_t)
        });
        Self {
            harbour,
            worker,
            sets: Vec::new(),
            light: None,
            held,
            last_phase: None,
            heard: Heard::default(),
        }
    }

    pub fn harbour(&self) -> Option<&Harbour> {
        self.harbour.as_ref()
    }

    /// Are the ships in the world yet?
    pub fn ready(&self) -> bool {
        !self.sets.is_empty()
    }

    /// The horns since the last call (handed over once).
    pub fn heard(&mut self) -> Heard {
        std::mem::take(&mut self.heard)
    }

    pub fn clear(&mut self, world: &mut World) {
        for id in self.sets.drain(..) {
            world.despawn_lod(id);
        }
        if let Some(l) = self.light.take() {
            world.remove_lamp(l);
        }
        self.worker = None;
        self.heard = Heard::default();
    }

    /// Moves the ships to game second `game_s`.
    pub fn update(&mut self, world: &mut World, game_s: f64) -> EngineResult<()> {
        let Some(h) = &self.harbour else {
            return Ok(());
        };
        if let Some(rx) = &self.worker {
            match rx.try_recv() {
                Ok(sets) => {
                    self.worker = None;
                    for levels in sets {
                        self.sets.push(world.spawn_lod_at(levels, &[])?);
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.worker = None,
            }
        }
        let t = self.held.unwrap_or(game_s);
        // The light: a red flash every four seconds.
        let head = *h.breakwater().last().expect("breakwater");
        let lantern = Vec3::new(
            head.x as f32,
            (h.crown() + LIGHT_H * 0.84) as f32,
            head.y as f32,
        );
        let flash = t.rem_euclid(4.0) < 0.8;
        let lamp = Lamp::point(
            lantern,
            rgb(255, 60, 40),
            if flash { 60.0 } else { 0.0 },
            70.0,
        );
        match self.light {
            Some(id) => world.set_lamp(id, lamp)?,
            None => self.light = Some(world.add_lamp(lamp)?),
        }
        // The horn: as the freighter casts off and as it turns for home.
        let (pose, phase) = h.freighter_pose(t);
        if let Some(last) = self.last_phase {
            if last != phase
                && matches!(phase, Phase::Backing | Phase::Docked)
                && self.held.is_none()
            {
                let bow = pose.to_plan(DVec2::new(0.0, ShipKind::Freighter.dims().len * 0.2));
                self.heard
                    .horns
                    .push(Vec3::new(bow.x as f32, 14.0, bow.y as f32));
            }
        }
        self.last_phase = Some(phase);
        if self.sets.is_empty() {
            return Ok(());
        }
        // Every ship, at its pose, riding the swell.
        let mut places: Vec<Vec<Place>> = vec![Vec::new(); KINDS.len()];
        for (i, (kind, pose)) in h.moored().into_iter().chain(h.ships_at(t)).enumerate() {
            let k = KINDS.iter().position(|&x| x == kind).expect("a kind");
            places[k].push(ship_place(kind, &pose, t, i));
        }
        for (id, at) in self.sets.iter().zip(&places) {
            world.set_lod_places(*id, at)?;
        }
        Ok(())
    }
}

/// A ship's place at its pose, heaving, pitching and rolling on the swell:
/// small ships more, big ones hardly at all.
fn ship_place(kind: ShipKind, pose: &ShipPose, t: f64, i: usize) -> Place {
    let len = kind.dims().len;
    let k = (20.0 / len).clamp(0.08, 1.0);
    let ph = i as f64 * 1.7;
    let heave = 0.12 * k * (t * 0.9 + ph).sin();
    let pitch = 1.6 * k * (t * 0.7 + ph * 0.6).sin();
    let roll = 2.4 * k * (t * 0.55 + ph * 1.3).sin();
    Place::new(
        pose.at.x as f32,
        (WATER_LEVEL + heave) as f32,
        pose.at.y as f32,
    )
    .with_yaw_deg(pose.yaw.to_degrees() as f32)
    .with_pitch_deg(pitch as f32)
    .with_roll_deg(roll as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swell_is_gentle() {
        let pose = ShipPose {
            at: DVec2::ZERO,
            yaw: 0.0,
            speed: 0.0,
        };
        for kind in KINDS {
            for s in 0..50 {
                let p = ship_place(kind, &pose, s as f64 * 0.37, 3);
                assert!(p.position.y.abs() < 0.2);
            }
        }
    }
}
