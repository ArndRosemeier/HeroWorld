//! The mad scientist's quarter as the camera sees it: the air turns a sick
//! green-grey and thick, the colour drains out of the town, and his Tesla
//! tower crackles ([`crate::teslaview`]) and his robot works build
//! robots ([`crate::worksview`]). Around it, walls are tagged and
//! the streets in are barricaded ([`crate::edgeview`]).
//!
//! One fog volume around the camera carries the look; how strongly follows
//! how deep the camera is in the quarter
//! ([`hw_gen::urban::dystopia::Dystopia::depth`]).

use engine::prelude::*;
use glam::{DVec2, Vec3};
use hw_gen::urban::world::TownWorld;

/// The air deep in the quarter: a sooty green-grey.
const SMOG: (u8, u8, u8) = (66, 74, 64);
/// How far you see deep in the quarter (m): a little murk in the distance,
/// not a haze that flattens the street in front of you.
const SMOG_VISIBILITY: f32 = 750.0;
/// Above this height over the ground, the look fades out (m).
const CEILING: f64 = 260.0;

/// The look deep in the quarter: darker and harder, the colour drained
/// but for his sick green in the shadows and a hot red in the lights,
/// heavy at the edges of the screen.
pub fn grade() -> ColorGrade {
    ColorGrade {
        exposure: -0.3,
        contrast: 1.3,
        saturation: 0.62,
        temperature: -0.02,
        tint: -0.18,
        shadows: Vec3::new(0.78, 0.96, 0.84),
        highlights: Vec3::new(1.08, 0.97, 0.9),
        vignette: 0.5,
    }
}

pub struct DystopiaView {
    volume: Option<FogVolumeId>,
    /// The Tesla tower's arcs, glow and bolts.
    tesla: crate::teslaview::TeslaView,
    /// Graffiti and barricades at its edge.
    edge: crate::edgeview::EdgeView,
    /// The robot works' line and dock.
    works: crate::worksview::WorksView,
    /// Smoke, searchlights, drones, sparks, posters and the night's light.
    menace: crate::menaceview::MenaceView,
}

impl DystopiaView {
    pub fn new(town: &std::sync::Arc<TownWorld>) -> Self {
        Self {
            menace: crate::menaceview::MenaceView::new(town),
            volume: None,
            tesla: crate::teslaview::TeslaView::new(town),
            edge: crate::edgeview::EdgeView::new(town),
            works: crate::worksview::WorksView::new(town),
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

    pub fn update(
        &mut self,
        world: &mut World,
        town: &TownWorld,
        dt: f32,
        hour: f32,
    ) -> EngineResult<()> {
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
        self.edge.update(world, town)?;
        self.tesla.update(world, dt)?;
        self.works.update(world, dt)?;
        self.menace.update(world, dt, hour)?;
        Ok(())
    }

    /// What the soundscape hears of the Tesla tower this frame.
    pub fn heard(&mut self) -> crate::teslaview::Heard {
        self.tesla.heard()
    }

    /// What the soundscape hears of the robot works this frame.
    pub fn works_heard(&mut self) -> crate::worksview::Heard {
        self.works.heard()
    }

    /// Waits for the quarter's fixtures (screenshots: the first frame
    /// shows them).
    pub fn wait(&mut self) {
        self.menace.wait();
    }

    pub fn clear(&mut self, world: &mut World) {
        self.menace.clear(world);
        self.tesla.clear(world);
        self.works.clear(world);
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
