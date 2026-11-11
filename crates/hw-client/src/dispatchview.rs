//! Dispatch beyond the wrist radio: the compass along the top of the screen
//! with a mark for each call, the town map (M) with the calls on it, and the
//! Hero Signal, a searchlight from the Aether Spire throwing the hero emblem
//! onto the sky over the worst trouble in town.

use crate::citymesh::{MeshBuilder, Surf};
use crate::fightview::con_colour;
use crate::landmarkview::strut;
use crate::radioview::Radio;
use engine::prelude::*;
use engine::ui::egui;
use glam::{DVec3, Vec3};
use hw_gen::urban::landmarks::LandmarkKind;
use hw_gen::urban::world::TownWorld;
use hw_sim::incidents::IncidentKind;
use std::f32::consts::{PI, TAU};

/// How high over the trouble the emblem shines (m).
pub const SIGNAL_HEIGHT: f64 = 260.0;
/// The emblem's radius (m).
const EMBLEM: f64 = 34.0;

/// Does this call light the Hero Signal? The worst trouble does, and the
/// colossus above all.
pub fn lights_signal(kind: IncidentKind) -> bool {
    matches!(
        kind,
        IncidentKind::Rampage
            | IncidentKind::Breakout
            | IncidentKind::Colossus
            | IncidentKind::IronLanding
            | IncidentKind::SaucerRaid
    )
}

/// The Hero Signal in the sky.
#[derive(Default)]
pub struct Signal {
    /// The call it shows, and its meshes.
    lit: Option<(u32, Vec<EntityId>)>,
}

impl Signal {
    /// Points the signal at the worst open call, or puts it out.
    pub fn update(&mut self, world: &mut World, town: &TownWorld, radio: &Radio) {
        let want = radio
            .calls
            .iter()
            .filter(|(_, c)| lights_signal(c.kind))
            .max_by_key(|(id, c)| (c.level, std::cmp::Reverse(**id)))
            .map(|(id, c)| (*id, c.at));
        if want.map(|w| w.0) == self.lit.as_ref().map(|l| l.0) {
            return;
        }
        self.clear(world);
        let Some((id, at)) = want else {
            return;
        };
        let spire = town
            .landmarks
            .iter()
            .find(|l| l.kind == LandmarkKind::AetherSpire);
        let from = spire.map_or(at.as_dvec3() + DVec3::Y * 40.0, |l| {
            DVec3::new(l.pos.x, l.base + l.height * 0.84, l.pos.y)
        });
        let sky = DVec3::new(at.x as f64, at.y as f64 + SIGNAL_HEIGHT, at.z as f64);
        // The beam meets the disc at its near rim, so the star stays clear.
        let back = DVec3::new(from.x - sky.x, 0.0, from.z - sky.z).normalize_or_zero();
        let end = sky + back * EMBLEM * 0.92 - DVec3::Y * 1.0;
        let mut b = MeshBuilder::new();
        let light = rgb(255, 236, 170);
        // The searchlight's beam, widening as it goes.
        b.surf = Surf::Glow;
        let steps = 6;
        for k in 0..steps {
            let t0 = k as f64 / steps as f64;
            let t1 = (k + 1) as f64 / steps as f64;
            let w = 1.0 + 10.0 * t1;
            strut(&mut b, from.lerp(end, t0), from.lerp(end, t1), w, light);
        }
        // The lit disc on the clouds, and the emblem on it in shadow: a
        // five-pointed star, the hero's mark.
        b.cylinder(sky, (EMBLEM, EMBLEM), 1.0, 40, true, light);
        b.surf = Surf::Plain;
        let star: Vec<DVec3> = (0..10)
            .map(|k| {
                let a = std::f64::consts::TAU * k as f64 / 10.0;
                let r = if k % 2 == 0 {
                    EMBLEM * 0.75
                } else {
                    EMBLEM * 0.3
                };
                sky + DVec3::new(a.sin() * r, -0.3, a.cos() * r)
            })
            .collect();
        let dark = rgb(40, 34, 30);
        for k in 0..10 {
            b.tri(
                [sky + DVec3::Y * -0.3, star[k], star[(k + 1) % 10]],
                -DVec3::Y,
                dark,
            );
        }
        let mut ids = Vec::new();
        for mesh in b.finish_parts() {
            ids.push(world.spawn(mesh));
        }
        self.lit = Some((id, ids));
    }

    pub fn clear(&mut self, world: &mut World) {
        if let Some((_, ids)) = self.lit.take() {
            for e in ids {
                world.despawn(e);
            }
        }
    }

    pub fn is_lit(&self) -> bool {
        self.lit.is_some()
    }
}

/// Compass bearing of a direction on the ground, 0 north (-z), clockwise
/// (east is +x).
fn bearing(d: glam::Vec2) -> f32 {
    d.x.atan2(-d.y).rem_euclid(TAU)
}

/// The compass strip along the top: N, E, S, W and a mark for each call.
pub fn compass(ctx: &egui::Context, eye: Vec3, look: Vec3, radio: &Radio, mine: u8) {
    let width = 520.0;
    let height = 30.0;
    let fov = PI * 0.9;
    let heading = bearing(glam::Vec2::new(look.x, look.z));
    let screen = ctx.screen_rect();
    let left = screen.center().x - width * 0.5;
    let top = 8.0;
    let rect = egui::Rect::from_min_size(egui::pos2(left, top), egui::vec2(width, height));
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("compass"),
    ));
    painter.rect_filled(rect, 4.0, egui::Color32::from_black_alpha(140));
    let x_of = |b: f32| {
        let rel = (b - heading + PI).rem_euclid(TAU) - PI;
        (rel.abs() < fov * 0.5).then(|| rect.center().x + rel / fov * width)
    };
    for k in 0..24 {
        let b = TAU * k as f32 / 24.0;
        let Some(x) = x_of(b) else { continue };
        let label = match k {
            0 => Some("N"),
            6 => Some("E"),
            12 => Some("S"),
            18 => Some("W"),
            _ => None,
        };
        match label {
            Some(t) => {
                painter.text(
                    egui::pos2(x, rect.center().y),
                    egui::Align2::CENTER_CENTER,
                    t,
                    egui::FontId::proportional(16.0),
                    egui::Color32::from_rgb(255, 214, 90),
                );
            }
            None => {
                painter.line_segment(
                    [
                        egui::pos2(x, rect.bottom() - 8.0),
                        egui::pos2(x, rect.bottom() - 2.0),
                    ],
                    egui::Stroke::new(1.0_f32, egui::Color32::from_gray(180)),
                );
            }
        }
    }
    // The ambulance we carry someone to: a white cross.
    if let Some(m) = radio.carrying() {
        let b = bearing(glam::Vec2::new(m.x - eye.x, m.z - eye.z));
        let rel = (b - heading + PI).rem_euclid(TAU) - PI;
        let x = x_of(b).unwrap_or(if rel < 0.0 {
            rect.left() + 8.0
        } else {
            rect.right() - 8.0
        });
        let c = egui::pos2(x, rect.top() + 9.0);
        painter.rect_filled(
            egui::Rect::from_center_size(c, egui::vec2(16.0, 16.0)),
            2.0,
            egui::Color32::from_rgb(200, 30, 36),
        );
        for size in [egui::vec2(10.0, 3.0), egui::vec2(3.0, 10.0)] {
            painter.rect_filled(
                egui::Rect::from_center_size(c, size),
                0.0,
                egui::Color32::WHITE,
            );
        }
    }
    // The story's objective: a gold star.
    if let Some(at) = radio.story.as_ref().and_then(|s| s.at) {
        let b = bearing(glam::Vec2::new(at.x - eye.x, at.z - eye.z));
        let rel = (b - heading + PI).rem_euclid(TAU) - PI;
        let x = x_of(b).unwrap_or(if rel < 0.0 {
            rect.left() + 8.0
        } else {
            rect.right() - 8.0
        });
        star(&painter, egui::pos2(x, rect.top() + 11.0), 9.0);
    }
    // The case's objective: a teal diamond.
    if let Some(at) = radio.case.as_ref().and_then(|s| s.at) {
        let b = bearing(glam::Vec2::new(at.x - eye.x, at.z - eye.z));
        let rel = (b - heading + PI).rem_euclid(TAU) - PI;
        let x = x_of(b).unwrap_or(if rel < 0.0 {
            rect.left() + 8.0
        } else {
            rect.right() - 8.0
        });
        diamond(&painter, egui::pos2(x, rect.top() + 11.0), 7.0);
    }
    for c in radio.calls.values() {
        let d = glam::Vec2::new(c.at.x - eye.x, c.at.z - eye.z);
        let b = bearing(d);
        // Off the strip: pinned to its edge, pointing the way.
        let rel = (b - heading + PI).rem_euclid(TAU) - PI;
        let x = x_of(b).unwrap_or(if rel < 0.0 {
            rect.left() + 6.0
        } else {
            rect.right() - 6.0
        });
        let col = con_colour(c.level, mine);
        let y = rect.top() + 6.0;
        painter.add(egui::Shape::convex_polygon(
            vec![
                egui::pos2(x - 6.0, y - 4.0),
                egui::pos2(x + 6.0, y - 4.0),
                egui::pos2(x, y + 6.0),
            ],
            col,
            egui::Stroke::new(1.0_f32, egui::Color32::BLACK),
        ));
        painter.text(
            egui::pos2(x, rect.bottom() + 8.0),
            egui::Align2::CENTER_CENTER,
            format!("{:.0} m", d.length()),
            egui::FontId::proportional(12.0),
            col,
        );
    }
}

/// The story's mark: a five-pointed gold star with a dark rim.
pub fn star(painter: &egui::Painter, c: egui::Pos2, r: f32) {
    let pts: Vec<egui::Pos2> = (0..10)
        .map(|k| {
            let a = k as f32 * PI / 5.0 - PI * 0.5;
            let rr = if k % 2 == 0 { r } else { r * 0.45 };
            c + egui::vec2(a.cos(), a.sin()) * rr
        })
        .collect();
    // Not convex: a fan of triangles from the middle.
    for k in 0..10 {
        painter.add(egui::Shape::convex_polygon(
            vec![c, pts[k], pts[(k + 1) % 10]],
            crate::radioview::STORY_GOLD,
            egui::Stroke::NONE,
        ));
    }
    let mut rim = pts.clone();
    rim.push(pts[0]);
    painter.add(egui::Shape::line(
        rim,
        egui::Stroke::new(1.2_f32, egui::Color32::from_rgb(90, 50, 0)),
    ));
}

/// The case's mark: a teal diamond with a dark rim.
pub fn diamond(painter: &egui::Painter, c: egui::Pos2, r: f32) {
    let pts = vec![
        c + egui::vec2(0.0, -r),
        c + egui::vec2(r, 0.0),
        c + egui::vec2(0.0, r),
        c + egui::vec2(-r, 0.0),
    ];
    painter.add(egui::Shape::convex_polygon(
        pts,
        crate::missionview::CASE_TEAL,
        egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(10, 40, 40)),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn bearings_run_clockwise_from_north() {
        let b = |x: f32, z: f32| bearing(glam::Vec2::new(x, z)).to_degrees().round();
        assert_eq!(b(0.0, -1.0), 0.0);
        assert_eq!(b(1.0, 0.0), 90.0);
        assert_eq!(b(0.0, 1.0), 180.0);
        assert_eq!(b(-1.0, 0.0), 270.0);
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn only_the_worst_trouble_lights_the_signal() {
        assert!(lights_signal(IncidentKind::Breakout));
        assert!(!lights_signal(IncidentKind::Mugging));
    }
}
