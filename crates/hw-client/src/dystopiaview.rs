//! The mad scientist's quarter as the camera sees it: the air turns a sick
//! green-grey and thick, the colour drains out of the town, and his Tesla
//! tower spits green sparks. Around it, walls are tagged and the streets
//! in are barricaded ([`crate::edgeview`]).
//!
//! One fog volume around the camera carries the look; how strongly follows
//! how deep the camera is in the quarter
//! ([`hw_gen::urban::dystopia::Dystopia::depth`]).

use crate::landmarkview::ARC;
use engine::prelude::*;
use glam::{DVec2, Vec3};
use hw_gen::urban::landmarks::LandmarkKind;
use hw_gen::urban::world::TownWorld;

/// The air deep in the quarter.
const SMOG: (u8, u8, u8) = (104, 116, 96);
/// How far you see deep in the quarter (m).
const SMOG_VISIBILITY: f32 = 260.0;
/// Above this height over the ground, the look fades out (m).
const CEILING: f64 = 260.0;

/// The look deep in the quarter: desaturated, a green cast, darker, heavy
/// at the edges of the screen.
pub fn grade() -> ColorGrade {
    ColorGrade {
        exposure: -0.3,
        contrast: 1.1,
        saturation: 0.45,
        temperature: -0.05,
        tint: -0.3,
        shadows: Vec3::new(0.85, 0.98, 0.86),
        highlights: Vec3::new(0.96, 1.04, 0.9),
        vignette: 0.4,
    }
}

pub struct DystopiaView {
    volume: Option<FogVolumeId>,
    /// The tower's top, where the sparks fly.
    crown: Option<Vec3>,
    clock: f32,
    sparks: u32,
    /// Graffiti and barricades at its edge.
    edge: crate::edgeview::EdgeView,
}

impl DystopiaView {
    pub fn new(town: &TownWorld) -> Self {
        let crown = town
            .landmarks
            .iter()
            .find(|l| l.kind == LandmarkKind::TeslaTower)
            .map(|l| {
                Vec3::new(
                    l.pos.x as f32,
                    (l.base + l.height * 0.8) as f32,
                    l.pos.y as f32,
                )
            });
        Self {
            volume: None,
            crown,
            clock: 0.0,
            sparks: 0,
            edge: crate::edgeview::EdgeView::new(town),
        }
    }

    /// How strongly the quarter's look applies at `eye` (0..1).
    pub fn strength(town: &TownWorld, eye: Vec3) -> f32 {
        let Some(d) = &town.town.dystopia else {
            return 0.0;
        };
        let p = DVec2::new(eye.x as f64, eye.z as f64);
        let depth = d.depth(p);
        if depth <= 0.0 {
            return 0.0;
        }
        let above = eye.y as f64 - town.ground(p);
        let high = (1.0 - (above - CEILING * 0.5) / (CEILING * 0.5)).clamp(0.0, 1.0);
        (depth * high) as f32
    }

    pub fn update(&mut self, world: &mut World, town: &TownWorld, dt: f32) -> EngineResult<()> {
        let eye = world.camera().eye();
        // The volume stays on the camera wherever it goes (at strength 0 it
        // looks exactly like the town's own air), so crossing the edge
        // blends by place, not by Engine easing a new volume in.
        if town.town.dystopia.is_some() {
            let s = Self::strength(town, eye);
            // The town's own air, thickened and greened as far as we are in.
            let (color, density) = match world.haze() {
                Some(h) => (h.color, 1.978 / h.visibility_m.max(1.0)),
                None => (rgb(SMOG.0, SMOG.1, SMOG.2), 0.0),
            };
            let thick = 1.978 / SMOG_VISIBILITY;
            let fog = color.lerp(rgb(SMOG.0, SMOG.1, SMOG.2), s);
            let visibility = 1.978 / (density + (thick - density) * s).max(1e-5);
            let volume = FogVolume::new(eye, Vec3::splat(20_000.0))
                .grade(world.color_grade().lerp(&grade(), s))
                .fog(fog, visibility.max(1.0))
                .priority(10);
            match self.volume {
                Some(id) => world.set_fog_volume(id, volume)?,
                None => {
                    // The look already blends by place; no easing on top.
                    world.set_zone_transition(0.0)?;
                    self.volume = Some(world.add_fog_volume(volume)?);
                }
            }
        }
        self.edge.update(world)?;
        // The tower sparks now and then while anyone is near enough to see.
        if let Some(crown) = self.crown {
            self.clock += dt;
            let gap = 0.7 + (self.sparks % 5) as f32 * 0.45;
            if self.clock >= gap && crown.distance(eye) < 900.0 {
                self.clock = 0.0;
                self.sparks = self.sparks.wrapping_add(1);
                let a = self.sparks as f32 * 2.399;
                let dir = Vec3::new(a.sin(), -0.4, a.cos()) * 7.0;
                world.particles_mut().burst(
                    ParticleEmitter::sparks(crown, dir, 50)
                        .with_color(rgb(ARC.0, ARC.1, ARC.2))
                        .with_secondary_color(rgb(230, 255, 210)),
                )?;
            }
        }
        Ok(())
    }

    pub fn clear(&mut self, world: &mut World) {
        self.edge.clear(world);
        if let Some(id) = self.volume.take() {
            world.remove_fog_volume(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_look_is_a_valid_grade() {
        grade().validate().unwrap();
        ColorGrade::NEUTRAL.lerp(&grade(), 0.5).validate().unwrap();
    }
}
