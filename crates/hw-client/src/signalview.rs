//! The town's traffic lights as they change: every light shows what its
//! junction's cycle in [`hw_sim::traffic::Traffic`] gives the cars it faces,
//! red, amber or green, so the cars that stop and go agree with the lamps.
//!
//! The light's model ([`crate::propmesh::traffic_light`]) has dark lenses; a lit lens is
//! drawn over the one that shows, from three instanced sets (one per
//! colour) whose places change as the lights do.

use crate::carmesh::{lathe, Lod};
use crate::citymesh::{MeshBuilder, Surf};
use crate::propmesh::TRAFFIC_LAMPS;
use engine::prelude::*;
use glam::{DVec2, DVec3};
use hw_gen::urban::props::PropKind;
use hw_gen::urban::world::TownWorld;
use hw_sim::traffic::{Signal, Traffic};
use std::collections::BTreeSet;

/// How often the lamps are looked at (s).
const STEP: f64 = 0.25;

struct Light {
    prop: usize,
    place: Place,
    /// Its junction and the phase of the cars it faces; none if it stands
    /// at no junction with lights.
    of: Option<(u32, u8)>,
    shows: Option<Signal>,
}

pub struct SignalView {
    lights: Vec<Light>,
    /// Red, amber and green lenses.
    sets: Vec<LodId>,
    broken: BTreeSet<usize>,
    resolved: bool,
    last: f64,
    dirty: bool,
}

impl SignalView {
    pub fn new(town: &TownWorld) -> Self {
        let lights = town
            .props
            .iter()
            .enumerate()
            .filter(|(_, p)| p.kind == PropKind::TrafficLight)
            .map(|(i, p)| Light {
                prop: i,
                place: Place::new(p.pos.x as f32, p.y as f32, p.pos.y as f32)
                    .with_yaw_deg(p.dir.x.atan2(p.dir.y).to_degrees() as f32)
                    .with_scale(p.scale as f32),
                of: None,
                shows: None,
            })
            .collect();
        Self {
            lights,
            sets: Vec::new(),
            broken: BTreeSet::new(),
            resolved: false,
            last: f64::NEG_INFINITY,
            dirty: true,
        }
    }

    /// A broken light shows nothing; a repaired one comes back on.
    pub fn set_broken(&mut self, prop: u32, broken: bool) {
        let prop = prop as usize;
        if !self.lights.iter().any(|l| l.prop == prop) {
            return;
        }
        let changed = if broken {
            self.broken.insert(prop)
        } else {
            self.broken.remove(&prop)
        };
        self.dirty |= changed;
    }

    /// Lights the lamps as the junctions' cycles stand at `time`.
    pub fn update(&mut self, world: &mut World, traffic: &Traffic, time: f64) -> EngineResult<()> {
        if self.lights.is_empty() {
            return Ok(());
        }
        if !self.resolved {
            self.resolved = true;
            for l in &mut self.lights {
                let p = l.place.position;
                let yaw = (l.place.yaw_degrees as f64).to_radians();
                let facing = DVec2::new(yaw.sin(), yaw.cos());
                l.of = traffic.light_of(DVec2::new(p.x as f64, p.z as f64), facing);
            }
        }
        if self.sets.is_empty() {
            for k in 0..3 {
                self.sets.push(world.spawn_lod_at(lens_levels(k), &[])?);
            }
        }
        if (time - self.last).abs() < STEP && !self.dirty {
            return Ok(());
        }
        self.last = time;
        for l in &mut self.lights {
            let now = l.of.map(|(j, phase)| traffic.signal(j, phase, time));
            if now != l.shows {
                l.shows = now;
                self.dirty = true;
            }
        }
        if !std::mem::take(&mut self.dirty) {
            return Ok(());
        }
        let mut places: [Vec<Place>; 3] = Default::default();
        for l in &self.lights {
            if self.broken.contains(&l.prop) {
                continue;
            }
            let k = match l.shows {
                Some(Signal::Red) => 0,
                Some(Signal::Amber) => 1,
                Some(Signal::Green) => 2,
                None => continue,
            };
            places[k].push(l.place);
        }
        for (set, p) in self.sets.iter().zip(places.iter()) {
            world.set_lod_places(*set, p)?;
        }
        Ok(())
    }

    /// How many lights show each colour (red, amber, green); for tests and
    /// the screenshot view.
    pub fn counts(&self) -> [usize; 3] {
        let mut n = [0; 3];
        for l in &self.lights {
            match l.shows {
                Some(Signal::Red) => n[0] += 1,
                Some(Signal::Amber) => n[1] += 1,
                Some(Signal::Green) => n[2] += 1,
                None => {}
            }
        }
        n
    }

    pub fn clear(&mut self, world: &mut World) {
        for s in self.sets.drain(..) {
            world.despawn_lod(s);
        }
        self.dirty = true;
    }
}

/// A lit lens of colour `k` (0 red, 1 amber, 2 green): the light's dark
/// dome ([`TRAFFIC_LAMPS`]) a hair larger and in front, glowing.
fn lens_levels(k: usize) -> Vec<LodLevel> {
    let c = [rgb(255, 46, 36), rgb(255, 176, 30), rgb(70, 255, 140)][k];
    let (y, front, r) = TRAFFIC_LAMPS[k];
    let lens = |lod: Lod| {
        let mut m = MeshBuilder::new();
        // The dome's own profile (radius, out) from its front minus 0.04,
        // grown 5 % and brought 5 mm forward.
        let profile: Vec<(f64, f64)> = [
            (0.0, 0.0),
            (r * 1.1, 0.0),
            (r * 1.1, 0.015),
            (r * 0.95, 0.02),
            (r * 0.6, 0.035),
            (0.0, 0.04),
        ]
        .iter()
        .map(|&(a, t)| (a * 1.05, t * 1.05))
        .collect();
        let at = DVec3::new(0.0, y, front - 0.04 + 0.005);
        let seg = lod.pick(18, 9, 6);
        lathe(&mut m, at, DVec3::Z, &profile, seg, |_| {
            Some((Surf::Glow, c))
        });
        m.finish()
    };
    // The same distances as the light itself.
    vec![
        LodLevel::new(lens(Lod::Near), 30.0),
        LodLevel::new(lens(Lod::Mid), 160.0),
        LodLevel::new(lens(Lod::Far), 500.0),
    ]
}
