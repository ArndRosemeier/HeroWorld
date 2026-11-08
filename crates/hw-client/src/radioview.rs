//! The wrist radio: incidents in town as a list (what, how far, which way
//! as a clock direction, how tough against you), and a beam of light over each one so it can be
//! found from the rooftops.

use crate::citymesh::{MeshBuilder, Surf};
use crate::fightview::con_colour;
use engine::prelude::*;
use engine::ui::egui;
use glam::{DVec3, Vec3};
use hw_net::ToClient;
use hw_sim::incidents::IncidentKind;
use std::collections::BTreeMap;

/// How tall the beam over an incident stands (m).
const BEAM: f64 = 140.0;

/// One incident the radio has heard of.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Call {
    pub kind: IncidentKind,
    pub at: Vec3,
    pub level: u8,
    /// Seconds since it came in (new calls flash).
    pub age: f32,
}

#[derive(Default)]
pub struct Radio {
    pub calls: BTreeMap<u32, Call>,
    beams: BTreeMap<u32, EntityId>,
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
        } = m
        else {
            return false;
        };
        let Some(kind) = IncidentKind::from_code(*kind) else {
            return true;
        };
        if *state == 0 {
            let at = Vec3::from_array(*at);
            self.calls.insert(
                *id,
                Call {
                    kind,
                    at,
                    level: *level,
                    age: 0.0,
                },
            );
            let mut b = MeshBuilder::new();
            b.surf = Surf::Glow;
            b.cylinder(
                DVec3::Y * 0.5,
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
            self.flash = Some((format!("Calling all heroes: {}!", kind.name()), 5.0));
        } else {
            self.calls.remove(id);
            if let Some(e) = self.beams.remove(id) {
                world.despawn(e);
            }
            let word = if *state == 1 {
                format!("{} handled. Nice work!", kind.name())
            } else {
                format!("{}: they got away.", kind.name())
            };
            self.flash = Some((word, 4.0));
        }
        true
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
    pub fn panel(&self, ctx: &egui::Context, eye: Vec3, look: Vec3, mine: u8) {
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
        self.calls.clear();
        self.flash = None;
    }
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
