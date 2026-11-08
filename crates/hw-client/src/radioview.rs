//! The wrist radio: incidents in town as a list (what, how far, which way
//! as a clock direction, how tough against you), and a beam of light over
//! each one so it can be found from the rooftops. Rescues show on the
//! spot: a crash's two smashed cars smoking, a house's front in flames, and
//! the people still trapped waving for help.

use crate::citymesh::{MeshBuilder, Surf};
use crate::fightview::con_colour;
use engine::prelude::*;
use engine::ui::egui;
use glam::{DVec2, DVec3, Vec3};
use hw_gen::urban::props::CarModel;
use hw_net::ToClient;
use hw_sim::incidents::{victims, IncidentKind, RESCUE_REACH};
use std::collections::BTreeMap;

/// How tall the beam over an incident stands (m).
const BEAM: f64 = 140.0;

/// One incident the radio has heard of.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Call {
    pub kind: IncidentKind,
    pub at: Vec3,
    pub level: u8,
    /// Which way it faces.
    pub facing: DVec2,
    /// People still trapped (a rescue).
    pub left: u8,
    /// Seconds since it came in (new calls flash).
    pub age: f32,
}

/// What stands at a rescue: wrecked cars, flames and smoke, the people.
#[derive(Default)]
struct Scene {
    cars: Vec<LodId>,
    fire: Vec<EmitterId>,
    people: Vec<EntityId>,
}

#[derive(Default)]
pub struct Radio {
    pub calls: BTreeMap<u32, Call>,
    beams: BTreeMap<u32, EntityId>,
    scenes: BTreeMap<u32, Scene>,
    /// The last word on the air: (text, seconds left).
    pub flash: Option<(String, f32)>,
}

/// A beam's colour by kind.
fn beam_colour(kind: IncidentKind) -> Color {
    match kind {
        IncidentKind::Mugging => rgb(255, 220, 90),
        IncidentKind::Robbery => rgb(255, 160, 60),
        IncidentKind::Rampage => rgb(255, 80, 70),
        IncidentKind::Breakout => rgb(150, 255, 110),
        IncidentKind::CarCrash => rgb(120, 200, 255),
        IncidentKind::Fire => rgb(255, 130, 40),
    }
}

impl Radio {
    /// An incident message; returns false for other messages.
    pub fn hear(&mut self, world: &mut World, m: &ToClient) -> bool {
        let ToClient::Incident {
            id,
            kind,
            at,
            level,
            state,
            yaw,
            left,
        } = m
        else {
            return false;
        };
        let Some(kind) = IncidentKind::from_code(*kind) else {
            return true;
        };
        let at = Vec3::from_array(*at);
        let a = hw_sim::props::byte_to_yaw(*yaw);
        let facing = DVec2::new(a.sin(), a.cos());
        if *state == 0 {
            if let Some(c) = self.calls.get_mut(id) {
                // News of a rescue: one more got out.
                if *left < c.left {
                    c.left = *left;
                    self.flash = Some(("Got one out! Well done.".into(), 3.0));
                    self.people(world, *id);
                }
                return true;
            }
            self.calls.insert(
                *id,
                Call {
                    kind,
                    at,
                    level: *level,
                    facing,
                    left: *left,
                    age: 0.0,
                },
            );
            let mut b = MeshBuilder::new();
            b.surf = Surf::Glow;
            // It starts over the rooftops: up close it would stand in the way.
            b.cylinder(
                DVec3::Y * 16.0,
                (0.8, 0.4),
                BEAM,
                8,
                false,
                beam_colour(kind),
            );
            for mesh in b.finish_parts() {
                if let Ok(e) = world.place(mesh, Place::new(at.x, at.y, at.z)) {
                    self.beams.insert(*id, e);
                }
            }
            if kind.trapped() > 0 {
                self.stage(world, *id);
            }
            self.flash = Some((format!("Calling all heroes: {}!", kind.name()), 5.0));
        } else {
            self.calls.remove(id);
            if let Some(e) = self.beams.remove(id) {
                world.despawn(e);
            }
            self.unstage(world, *id);
            let word = match (*state, kind.trapped() > 0) {
                (1, true) => "Everyone is out. You saved them!".to_string(),
                (1, false) => format!("{} handled. Nice work!", kind.name()),
                (_, true) => "The fire brigade got there first.".to_string(),
                _ => format!("{}: they got away.", kind.name()),
            };
            self.flash = Some((word, 4.0));
        }
        true
    }

    /// A rescue's scene: cars or flames, then the people.
    fn stage(&mut self, world: &mut World, id: u32) {
        let Some(c) = self.calls.get(&id).copied() else {
            return;
        };
        let mut scene = Scene::default();
        let f = Vec3::new(c.facing.x as f32, 0.0, c.facing.y as f32);
        let side = Vec3::new(f.z, 0.0, -f.x);
        let yaw = c.facing.x.atan2(c.facing.y).to_degrees() as f32;
        match c.kind {
            IncidentKind::CarCrash => {
                // Two cars nose to nose, knocked askew, smoking.
                for (k, model) in [(-1.0f32, CarModel::Cruiser), (1.0, CarModel::Bubble)] {
                    let p = c.at + side * (2.6 * k);
                    let place = Place::new(p.x, p.y, p.z)
                        .with_yaw_deg(yaw + 108.0 * k)
                        .with_roll_deg(4.0 * k);
                    let levels = crate::propview::moving_car_levels(model, (id % 7) as u8);
                    if let Ok(l) = world.spawn_lod_at(levels, &[place]) {
                        scene.cars.push(l);
                    }
                    // Its wheels, one of them buckled.
                    let turn = glam::Quat::from_rotation_y((yaw + 108.0 * k).to_radians());
                    if let (Some((x, zs, r)), Some(levels)) = (
                        crate::propview::wheel_layout(model),
                        crate::propview::wheel_levels(model),
                    ) {
                        let mut wheels = Vec::new();
                        for (i, z) in zs.iter().enumerate() {
                            for (j, sx) in [-x, x].iter().enumerate() {
                                let w = p + turn * Vec3::new(*sx as f32, r as f32, *z as f32);
                                let buckled = if i + j == 0 { 25.0 } else { 0.0 };
                                wheels.push(
                                    Place::new(w.x, w.y, w.z)
                                        .with_yaw_deg(yaw + 108.0 * k + buckled)
                                        .with_roll_deg(4.0 * k),
                                );
                            }
                        }
                        if let Ok(l) = world.spawn_lod_at(levels, &wheels) {
                            scene.cars.push(l);
                        }
                    }
                }
                let p = world.particles_mut();
                if let Ok(e) = p.start(ParticleEmitter::smoke(c.at + Vec3::Y * 1.0, 1.6)) {
                    scene.fire.push(e);
                }
                let _ = p.burst(ParticleEmitter::sparks(
                    c.at + Vec3::Y * 0.8,
                    Vec3::Y * 5.0,
                    60,
                ));
            }
            _ => {
                // The front of the house ablaze: flames at the door and up
                // the wall, smoke over the roof.
                let wall = c.at - f * 3.0;
                let p = world.particles_mut();
                for (dx, up, size) in [(0.0, 0.5, 2.2), (-3.5, 4.0, 1.8), (3.5, 6.5, 1.8)] {
                    let at = wall + side * dx + Vec3::Y * up;
                    if let Ok(e) = p.start(ParticleEmitter::flames(at, size)) {
                        scene.fire.push(e);
                    }
                }
                for (up, size) in [(8.0, 3.5), (16.0, 5.0)] {
                    if let Ok(e) = p.start(ParticleEmitter::smoke(wall + Vec3::Y * up, size)) {
                        scene.fire.push(e);
                    }
                }
                if let Ok(e) = p.start(ParticleEmitter::embers(wall + Vec3::Y * 3.0, 2.0)) {
                    scene.fire.push(e);
                }
            }
        }
        self.scenes.insert(id, scene);
        self.people(world, id);
    }

    /// The people still trapped, waving.
    fn people(&mut self, world: &mut World, id: u32) {
        let Some(c) = self.calls.get(&id).copied() else {
            return;
        };
        let scene = self.scenes.entry(id).or_default();
        for e in scene.people.drain(..) {
            world.despawn(e);
        }
        let yaw = c.facing.x.atan2(c.facing.y).to_degrees() as f32;
        for (k, p) in victims(c.kind, c.at.as_dvec3(), c.facing, c.left)
            .into_iter()
            .enumerate()
        {
            let place = Place::new(p.x as f32, p.y as f32, p.z as f32).with_yaw_deg(yaw);
            if let Ok(e) = world.place(person(k as u32 + id), place) {
                scene.people.push(e);
            }
        }
    }

    fn unstage(&mut self, world: &mut World, id: u32) {
        if let Some(scene) = self.scenes.remove(&id) {
            for l in scene.cars {
                world.despawn_lod(l);
            }
            let p = world.particles_mut();
            for e in scene.fire {
                let _ = p.stop(e);
            }
            for e in scene.people {
                world.despawn(e);
            }
        }
    }

    /// Is a trapped person within reach of `hero` (to tell the player how
    /// to help)?
    pub fn near_victim(&self, hero: Vec3) -> bool {
        self.calls.values().any(|c| {
            victims(c.kind, c.at.as_dvec3(), c.facing, c.left)
                .last()
                .is_some_and(|v| v.as_vec3().distance(hero) < RESCUE_REACH as f32 + 0.5)
        })
    }

    pub fn update(&mut self, dt: f32) {
        for c in self.calls.values_mut() {
            c.age += dt;
        }
        if let Some((_, t)) = self.flash.as_mut() {
            *t -= dt;
        }
        if self.flash.as_ref().is_some_and(|f| f.1 <= 0.0) {
            self.flash = None;
        }
    }

    /// The radio on screen, top left: each call with its distance, the way
    /// to go (against where the camera looks) and a con colour.
    pub fn panel(&self, ctx: &egui::Context, eye: Vec3, look: Vec3, mine: u8, hero: Option<Vec3>) {
        if hero.is_some_and(|h| self.near_victim(h)) {
            egui::Area::new(egui::Id::new("rescue"))
                .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -170.0))
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_black_alpha(170))
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(10, 5))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("Hold E to help them out")
                                    .color(egui::Color32::from_rgb(255, 230, 140))
                                    .size(17.0)
                                    .strong(),
                            );
                        });
                });
        }
        if self.calls.is_empty() && self.flash.is_none() {
            return;
        }
        let mut calls: Vec<(&u32, &Call)> = self.calls.iter().collect();
        calls.sort_by(|a, b| a.1.at.distance(eye).total_cmp(&b.1.at.distance(eye)));
        egui::Area::new(egui::Id::new("radio"))
            .anchor(egui::Align2::LEFT_TOP, egui::vec2(10.0, 10.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgba_unmultiplied(20, 24, 30, 210))
                    .stroke(egui::Stroke::new(
                        1.5_f32,
                        egui::Color32::from_rgb(255, 196, 60),
                    ))
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("WRIST RADIO")
                                .color(egui::Color32::from_rgb(255, 204, 70))
                                .strong()
                                .size(13.0),
                        );
                        if let Some((text, _)) = &self.flash {
                            ui.label(
                                egui::RichText::new(text)
                                    .color(egui::Color32::from_rgb(255, 240, 200))
                                    .italics(),
                            );
                        }
                        for (_, c) in calls {
                            let d = c.at - eye;
                            let flat = glam::Vec2::new(d.x, d.z);
                            let ahead = glam::Vec2::new(look.x, look.z).normalize_or_zero();
                            let hour = clock(ahead, flat);
                            let col = con_colour(c.level, mine);
                            let blink = c.age < 4.0 && (c.age * 3.0) as i32 % 2 == 0;
                            let name = egui::RichText::new(format!(
                                "{}  {:.0} m at {hour} o'clock",
                                c.kind.name(),
                                flat.length()
                            ))
                            .color(col)
                            .size(15.0);
                            ui.label(if blink { name.strong() } else { name });
                        }
                    });
            });
    }

    pub fn clear(&mut self, world: &mut World) {
        for (_, e) in std::mem::take(&mut self.beams) {
            world.despawn(e);
        }
        let ids: Vec<u32> = self.scenes.keys().copied().collect();
        for id in ids {
            self.unstage(world, id);
        }
        self.calls.clear();
        self.flash = None;
    }
}

/// Someone in trouble: plain clothes in one of a few colours, an arm up
/// waving for help.
fn person(look: u32) -> Mesh {
    const SHIRTS: [(u8, u8, u8); 5] = [
        (70, 130, 190),
        (210, 190, 90),
        (150, 80, 120),
        (90, 150, 90),
        (220, 220, 210),
    ];
    let s = SHIRTS[look as usize % SHIRTS.len()];
    let shirt = rgb(s.0, s.1, s.2);
    let skin = rgb(224, 182, 150);
    let mut m = Mesh::new();
    let _ = m.add_box((0.0, 0.45, 0.0), (0.34, 0.9, 0.26), rgb(60, 62, 80));
    let _ = m.add_box((0.0, 1.17, 0.0), (0.46, 0.56, 0.28), shirt);
    let _ = m.add_box((0.0, 1.6, 0.0), (0.24, 0.26, 0.24), skin);
    // One arm down, one up and waving.
    let _ = m.add_box((-0.3, 1.1, 0.0), (0.12, 0.55, 0.12), shirt);
    let _ = m.add_box((0.3, 1.72, 0.0), (0.12, 0.6, 0.12), shirt);
    let _ = m.add_box((0.3, 2.06, 0.0), (0.12, 0.12, 0.12), skin);
    m
}

/// Which way `to` lies from `ahead` (the way the camera looks), as a
/// pilot would call it: 12 o'clock dead ahead, 3 o'clock to the right.
pub fn clock(ahead: glam::Vec2, to: glam::Vec2) -> u8 {
    if ahead == glam::Vec2::ZERO || to == glam::Vec2::ZERO {
        return 12;
    }
    // y is up and the world right-handed: looking along +z, +x is on the
    // left. `a` is how far `to` turns from `ahead`, positive to the right.
    let a = ahead.perp_dot(to).atan2(ahead.dot(to));
    let h = (12 + (a / (std::f32::consts::PI / 6.0)).round() as i32).rem_euclid(12);
    if h == 0 {
        12
    } else {
        h as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;

    #[test]
    fn calls_give_the_clock_direction() {
        let ahead = Vec2::new(0.0, 1.0);
        assert_eq!(clock(ahead, Vec2::new(0.0, 5.0)), 12);
        assert_eq!(clock(ahead, Vec2::new(0.0, -5.0)), 6);
        assert_eq!(clock(ahead, Vec2::new(5.0, 0.0)), 9);
        assert_eq!(clock(ahead, Vec2::new(-5.0, 0.0)), 3);
    }
}
