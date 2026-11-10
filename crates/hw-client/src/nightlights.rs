//! Lights after dark: street lamps, kiosks and the landmarks' Aether glow
//! become Engine lamps when the sun is low.
//!
//! A town has thousands of street lamps and Engine draws at most a few
//! hundred, so only the ones nearest the camera are lit; the set follows
//! the camera in steps. Far lamps still glow (their bulbs are emissive), they
//! just don't light the street.

use crate::propview::PropView;
use engine::lamps::{Lamp, LampId};
use engine::prelude::*;
use glam::{DVec2, Vec3};
use hw_gen::urban::landmarks::LandmarkKind;
use hw_gen::urban::props::{LampStyle, Prop, PropKind};
use hw_gen::urban::quarter::DetailId;
use hw_gen::urban::world::TownWorld;
use std::sync::Arc;

/// Most lamps lit at once (Engine draws up to 256).
const POOL: usize = 140;
/// Street lamps farther than this are not lit.
const REACH: f32 = 260.0;
/// The lit set is picked again when the camera has moved this far.
const STEP: f32 = 24.0;

/// One light the town could switch on.
#[derive(Clone, Copy, Debug)]
/// A landmark's light: lit from much farther away than street lamps.
struct Source {
    lamp: Lamp,
}

pub struct NightLights {
    /// The landmarks' lights (street lamps come from the props loaded).
    sources: Vec<Source>,
    town: Arc<TownWorld>,
    lit: Vec<LampId>,
    at: Option<(i32, i32, u64)>,
}

/// A street lamp: a wide cone down from its head, casting shadows. A cone
/// needs one shadow view where a point lamp needs six (a cube), and the
/// heads are shaded, so the light falls on the street and the lower
/// facades; the windows' own glow lights the walls above.
fn street(at: Vec3, color: Color, intensity: f32, range: f32) -> Lamp {
    Lamp::spot(at, Vec3::NEG_Y, STREET_CONE_DEG, color, intensity, range).shadows(true)
}

/// How wide a street lamp's cone is (degrees): from 4 to 6.6 m up it
/// reaches 15 to 25 m across the street.
const STREET_CONE_DEG: f32 = 150.0;

/// The light of a street prop, if it has one.
fn prop_light(town: &TownWorld, id: DetailId, p: &Prop) -> Option<Lamp> {
    // Lamps go dark or flicker by their number, the same on every machine.
    let i = id.index();
    // In the villains' quarter two lamps in three are dead, and the
    // rest flicker a sick green.
    let held = town
        .town
        .dystopia
        .as_ref()
        .is_some_and(|d| d.contains(p.pos));
    if held && !i.is_multiple_of(3) {
        return None;
    }
    // In the band around it the town frays: one lamp in five is
    // out, and one in three flickers.
    let fraying = !held
        && town
            .town
            .dystopia
            .as_ref()
            .is_some_and(|d| d.zone(p.pos) == hw_gen::urban::dystopia::Zone::Edge);
    if fraying && i % 5 == 1 {
        return None;
    }
    let at = |fwd: f64, up: f64| {
        let q = p.pos + p.dir * fwd;
        Vec3::new(q.x as f32, (p.y + up * p.scale) as f32, q.y as f32)
    };
    let lamp = match p.kind {
        PropKind::Lamp(LampStyle::Lantern) => street(at(0.0, 4.1), rgb(255, 196, 132), 10.0, 16.0),
        PropKind::Lamp(LampStyle::Deco) => street(at(0.0, 5.7), rgb(255, 214, 170), 14.0, 20.0),
        PropKind::Lamp(LampStyle::Atomic) => street(at(1.75, 6.6), rgb(205, 236, 255), 16.0, 22.0),
        PropKind::Kiosk => Lamp::point(at(0.0, 2.4), rgb(255, 210, 120), 4.0, 7.0),
        PropKind::Fountain => Lamp::point(at(0.0, 2.6), rgb(140, 230, 255), 5.0, 9.0),
        PropKind::PhoneBooth => Lamp::point(at(0.0, 2.25), rgb(255, 214, 140), 3.0, 6.0),
        PropKind::BusStop => Lamp::point(at(0.5, 2.3), rgb(255, 232, 190), 5.0, 8.0),
        _ => return None,
    };
    Some(if held {
        Lamp {
            color: rgb(170, 230, 140),
            flicker: 0.7,
            ..lamp
        }
    } else if fraying && i.is_multiple_of(3) {
        Lamp {
            flicker: 0.5,
            ..lamp
        }
    } else {
        lamp
    })
}

impl NightLights {
    pub fn new(town: Arc<TownWorld>) -> Self {
        let mut sources = Vec::new();
        for l in &town.landmarks {
            let at = |up: f64| Vec3::new(l.pos.x as f32, (l.base + up) as f32, l.pos.y as f32);
            match l.kind {
                LandmarkKind::AetherSpire => {
                    let aether = rgb(120, 230, 255);
                    // The crown, and the coils lighting the plaza.
                    sources.push(Source {
                        lamp: Lamp::point(at(l.height * 0.84 + 1.2), aether, 70.0, 70.0),
                    });
                    sources.push(Source {
                        lamp: Lamp::point(at(l.height * 0.3), aether, 30.0, 40.0),
                    });
                }
                LandmarkKind::Lighthouse => sources.push(Source {
                    lamp: Lamp::point(at(l.height - 4.5), rgb(255, 236, 170), 45.0, 45.0),
                }),
                LandmarkKind::HeroHall => {
                    // The lobby glowing through the glass, and the star.
                    sources.push(Source {
                        lamp: Lamp::point(at(3.0), rgb(255, 222, 170), 35.0, 35.0),
                    });
                    sources.push(Source {
                        lamp: Lamp::point(at(l.height + 8.4), rgb(255, 210, 120), 25.0, 25.0),
                    });
                }
                LandmarkKind::MooringMast => {
                    // The terminal glowing, and floodlight up the mast.
                    let t = l.to_plan(DVec2::new(0.0, hw_gen::urban::mast::TERMINAL_Z + 6.0));
                    sources.push(Source {
                        lamp: Lamp::point(
                            Vec3::new(t.x as f32, l.base as f32 + 3.0, t.y as f32),
                            rgb(255, 222, 170),
                            30.0,
                            30.0,
                        ),
                    });
                    sources.push(Source {
                        lamp: Lamp::point(
                            at(hw_gen::urban::mast::GALLERY_Y - 3.0),
                            rgb(200, 236, 255),
                            45.0,
                            45.0,
                        ),
                    });
                }
                LandmarkKind::WeatherTower => {
                    // Floodlights washing up the mast from the pavilion's
                    // roof, and the saucer's windows. The globe's light
                    // changes with the weather: see `weatherview`.
                    let d = hw_gen::urban::weather_tower::WxDims::of(l.height);
                    sources.push(Source {
                        lamp: Lamp::point(at(7.5), rgb(230, 238, 255), 45.0, 40.0),
                    });
                    sources.push(Source {
                        lamp: Lamp::point(at(d.deck_y), rgb(255, 214, 160), 30.0, 30.0),
                    });
                }
                LandmarkKind::RocketGantry => {
                    // Floodlights on the masts, lighting up the rocket.
                    let rocket = hw_gen::urban::gantry::ROCKET_H * 0.45;
                    for m in hw_gen::urban::gantry::masts(l.plaza) {
                        let p = l.to_plan(m * 0.55);
                        let up = hw_gen::urban::gantry::STAND_H + rocket * 0.6;
                        sources.push(Source {
                            lamp: Lamp::point(
                                Vec3::new(p.x as f32, (l.base + up) as f32, p.y as f32),
                                rgb(236, 240, 255),
                                60.0,
                                38.0,
                            ),
                        });
                    }
                }
                // Its glow breathes: see `atomview`. Floodlights on the yard.
                LandmarkKind::AtomicPlant => {
                    for (x, z) in [(-20.0, 20.0), (20.0, 22.0)] {
                        let p = l.to_plan(glam::DVec2::new(x, z));
                        sources.push(Source {
                            lamp: Lamp::point(
                                Vec3::new(p.x as f32, (l.base + 9.0) as f32, p.y as f32),
                                rgb(255, 226, 180),
                                40.0,
                                40.0,
                            ),
                        });
                    }
                }
                // Its beacon and the glow in its bore: see `drillview`.
                LandmarkKind::MoleDrill => {}
                // Their lights pulse with what they do: see `teslaview`
                // and `worksview`.
                LandmarkKind::TeslaTower | LandmarkKind::RobotWorks => {}
            }
        }
        Self {
            sources,
            town,
            lit: Vec::new(),
            at: None,
        }
    }

    /// Lights on (`dark`) or off, the lit set following `eye`, from the
    /// landmarks and the street props loaded around it (broken ones dark),
    /// and `rooms`: the lit rooms of floors drawn open, with a key that
    /// changes when they do (see `TownView::room_lamps`).
    pub fn update(
        &mut self,
        world: &mut World,
        eye: Vec3,
        dark: bool,
        props: &PropView,
        rooms: (u64, &[Lamp]),
    ) -> EngineResult<()> {
        if !dark {
            self.clear(world);
            return Ok(());
        }
        let key = (
            (eye.x / STEP).round() as i32,
            (eye.z / STEP).round() as i32,
            props.generation ^ rooms.0.rotate_left(17),
        );
        if self.at == Some(key) {
            return Ok(());
        }
        self.at = Some(key);
        let flat = |p: Vec3| DVec2::new((p.x - eye.x) as f64, (p.z - eye.z) as f64).length() as f32;
        let centre = DVec2::new(eye.x as f64, eye.z as f64);
        let street: Vec<Lamp> = props
            .near(centre, REACH as f64 + 2.0)
            .filter(|(id, _)| !props.is_broken(id.0))
            .filter_map(|(id, p)| prop_light(&self.town, id, p))
            .collect();
        let mut near: Vec<(f32, Lamp)> = self
            .sources
            .iter()
            .filter_map(|s| {
                let d = flat(s.lamp.position);
                // Landmarks first, then the nearest.
                (d < 2000.0).then_some((-1.0, s.lamp))
            })
            // The rooms we look into are next to the camera: before the
            // street.
            .chain(rooms.1.iter().map(|l| (0.0, *l)))
            .chain(street.into_iter().filter_map(|l| {
                let d = flat(l.position);
                (d < REACH).then_some((d, l))
            }))
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.truncate(POOL);
        for (k, (_, lamp)) in near.iter().enumerate() {
            match self.lit.get(k) {
                Some(&id) => world.set_lamp(id, *lamp)?,
                None => self.lit.push(world.add_lamp(*lamp)?),
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

    /// A prop broke or was put back: pick the lit set again.
    pub fn refresh(&mut self) {
        self.at = None;
    }

    /// How many landmark lights the town has.
    pub fn sources(&self) -> usize {
        self.sources.len()
    }
}

/// Is it dark enough for lamps at `hour` (the sun near or below the
/// horizon)?
pub fn dark_at(hour: f32) -> bool {
    engine::sky::sky_path(hour).y < 0.12
}
