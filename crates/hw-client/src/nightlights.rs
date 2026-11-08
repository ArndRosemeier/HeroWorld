//! Lights after dark: street lamps, kiosks and the landmarks' Aether glow
//! become Engine lamps when the sun is low.
//!
//! A town has thousands of street lamps and Engine draws at most a few
//! hundred, so only the ones nearest the camera are lit; the set follows
//! the camera in steps. Far lamps still glow (their bulbs are emissive), they
//! just don't light the street.

use engine::lamps::{Lamp, LampId};
use engine::prelude::*;
use glam::{DVec2, Vec3};
use hw_gen::urban::landmarks::LandmarkKind;
use hw_gen::urban::props::{LampStyle, PropKind};
use hw_gen::urban::world::TownWorld;
use std::collections::BTreeSet;

/// Most lamps lit at once (Engine draws up to 256).
const POOL: usize = 140;
/// Street lamps farther than this are not lit.
const REACH: f32 = 260.0;
/// The lit set is picked again when the camera has moved this far.
const STEP: f32 = 24.0;

/// One light the town could switch on.
#[derive(Clone, Copy, Debug)]
struct Source {
    lamp: Lamp,
    /// Landmarks: lit from much farther away.
    big: bool,
    /// The street prop it hangs on (a broken lamp goes dark).
    prop: Option<u32>,
}

pub struct NightLights {
    sources: Vec<Source>,
    lit: Vec<LampId>,
    at: Option<(i32, i32)>,
    /// Broken props: their lights stay off.
    off: BTreeSet<u32>,
}

impl NightLights {
    pub fn new(town: &TownWorld) -> Self {
        let mut sources = Vec::new();
        for (i, p) in town.props.iter().enumerate() {
            let at = |fwd: f64, up: f64| {
                let q = p.pos + p.dir * fwd;
                Vec3::new(q.x as f32, (p.y + up * p.scale) as f32, q.y as f32)
            };
            let lamp = match p.kind {
                PropKind::Lamp(LampStyle::Lantern) => {
                    Lamp::point(at(0.0, 4.1), rgb(255, 196, 132), 10.0, 16.0).shadows(true)
                }
                PropKind::Lamp(LampStyle::Deco) => {
                    Lamp::point(at(0.0, 5.7), rgb(255, 214, 170), 14.0, 20.0).shadows(true)
                }
                PropKind::Lamp(LampStyle::Atomic) => {
                    Lamp::point(at(1.75, 6.6), rgb(205, 236, 255), 16.0, 22.0).shadows(true)
                }
                PropKind::Kiosk => Lamp::point(at(0.0, 2.4), rgb(255, 210, 120), 4.0, 7.0),
                PropKind::Fountain => Lamp::point(at(0.0, 2.6), rgb(140, 230, 255), 5.0, 9.0),
                _ => continue,
            };
            sources.push(Source {
                lamp,
                big: false,
                prop: Some(i as u32),
            });
        }
        for l in &town.landmarks {
            let at = |up: f64| Vec3::new(l.pos.x as f32, (l.base + up) as f32, l.pos.y as f32);
            match l.kind {
                LandmarkKind::AetherSpire => {
                    let aether = rgb(120, 230, 255);
                    // The crown, and the coils lighting the plaza.
                    sources.push(Source {
                        lamp: Lamp::point(at(l.height * 0.84 + 1.2), aether, 70.0, 70.0),
                        big: true,
                        prop: None,
                    });
                    sources.push(Source {
                        lamp: Lamp::point(at(l.height * 0.3), aether, 30.0, 40.0),
                        big: true,
                        prop: None,
                    });
                }
                LandmarkKind::Lighthouse => sources.push(Source {
                    lamp: Lamp::point(at(l.height - 4.5), rgb(255, 236, 170), 45.0, 45.0),
                    big: true,
                    prop: None,
                }),
            }
        }
        Self {
            sources,
            lit: Vec::new(),
            at: None,
            off: BTreeSet::new(),
        }
    }

    /// Lights on (`dark`) or off, the lit set following `eye`.
    pub fn update(&mut self, world: &mut World, eye: Vec3, dark: bool) -> EngineResult<()> {
        if !dark {
            self.clear(world);
            return Ok(());
        }
        let key = ((eye.x / STEP).round() as i32, (eye.z / STEP).round() as i32);
        if self.at == Some(key) {
            return Ok(());
        }
        self.at = Some(key);
        let flat = |p: Vec3| DVec2::new((p.x - eye.x) as f64, (p.z - eye.z) as f64).length() as f32;
        let mut near: Vec<(f32, &Source)> = self
            .sources
            .iter()
            .filter(|s| s.prop.is_none_or(|i| !self.off.contains(&i)))
            .filter_map(|s| {
                let d = flat(s.lamp.position);
                let reach = if s.big { 2000.0 } else { REACH };
                // Landmarks first, then the nearest.
                (d < reach).then_some((if s.big { -1.0 } else { d }, s))
            })
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.truncate(POOL);
        for (k, (_, s)) in near.iter().enumerate() {
            match self.lit.get(k) {
                Some(&id) => world.set_lamp(id, s.lamp)?,
                None => self.lit.push(world.add_lamp(s.lamp)?),
            }
        }
        for id in self.lit.drain(near.len()..) {
            world.remove_lamp(id);
        }
        Ok(())
    }

    /// Switch every lamp off (daytime, or the town is going away).
    pub fn clear(&mut self, world: &mut World) {
        for id in self.lit.drain(..) {
            world.remove_lamp(id);
        }
        self.at = None;
    }

    /// Prop `i` broke (`true`) or was put back: its light goes out or on.
    pub fn set_broken(&mut self, i: u32, broken: bool) {
        let changed = if broken {
            self.off.insert(i)
        } else {
            self.off.remove(&i)
        };
        if changed {
            // Pick the lit set again next update.
            self.at = None;
        }
    }

    /// How many lights the town has.
    pub fn sources(&self) -> usize {
        self.sources.len()
    }
}

/// Is it dark enough for lamps at `hour` (the sun near or below the
/// horizon)?
pub fn dark_at(hour: f32) -> bool {
    engine::sky::sky_path(hour).y < 0.12
}
