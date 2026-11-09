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
use glam::{DVec2, DVec3, Vec3};
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

/// The town map (M): main roads, the water's edge left out, the villains'
/// quarter, landmarks, you, and every call on the radio.
pub fn map(
    ctx: &egui::Context,
    open: &mut bool,
    town: &TownWorld,
    hero: Option<Vec3>,
    look: Vec3,
    radio: &Radio,
    mine: u8,
) {
    if !*open {
        return;
    }
    let screen = ctx.screen_rect();
    let side = (screen.height() - 120.0)
        .min(screen.width() - 80.0)
        .max(200.0);
    let rect = egui::Rect::from_center_size(screen.center(), egui::vec2(side, side));
    let painter = ctx
        .layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("map"),
        ))
        .with_clip_rect(rect);
    // The built-up town fits (the country roads run off the edge).
    let (mut lo, mut hi) = (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN));
    for piece in &town.town.pieces {
        for p in &piece.face.pts {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
    }
    if lo.x > hi.x {
        lo = DVec2::splat(-500.0);
        hi = DVec2::splat(500.0);
    }
    let centre = (lo + hi) * 0.5;
    let span = (hi - lo).max_element().max(200.0) * 1.08;
    let scale = side as f64 / span;
    let to_screen = |p: DVec2| {
        let q = (p - centre) * scale;
        egui::pos2(rect.center().x + q.x as f32, rect.center().y + q.y as f32)
    };
    painter.rect_filled(rect, 8.0, egui::Color32::from_rgb(232, 222, 196));
    painter.rect_stroke(
        rect,
        8.0,
        egui::Stroke::new(3.0_f32, egui::Color32::from_rgb(120, 90, 50)),
        egui::StrokeKind::Inside,
    );
    // The villains' quarter, shaded.
    if let Some(d) = &town.town.dystopia {
        for &i in &d.pieces {
            let pts: Vec<egui::Pos2> = town.town.pieces[i]
                .face
                .pts
                .iter()
                .map(|p| to_screen(*p))
                .collect();
            painter.add(egui::Shape::closed_line(
                pts,
                egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(90, 140, 60)),
            ));
        }
        // Under the quarter, clear of the tower's name.
        let bottom = d
            .pieces
            .iter()
            .flat_map(|&i| town.town.pieces[i].face.pts.iter())
            .filter(|p| p.distance(d.centre) <= d.radius)
            .map(|p| p.y)
            .fold(d.centre.y, f64::max);
        let c = to_screen(DVec2::new(d.centre.x, bottom));
        painter.text(
            c + egui::vec2(0.0, 4.0),
            egui::Align2::CENTER_TOP,
            "Mad scientist's quarter",
            egui::FontId::proportional(13.0),
            egui::Color32::from_rgb(60, 110, 40),
        );
    }
    for r in &town.main_roads {
        let pts: Vec<egui::Pos2> = r.pts.iter().map(|p| to_screen(*p)).collect();
        let (w, c) = if r.main {
            (2.5_f32, egui::Color32::from_rgb(150, 120, 90))
        } else {
            (1.0_f32, egui::Color32::from_rgb(190, 170, 140))
        };
        painter.add(egui::Shape::line(pts, egui::Stroke::new(w, c)));
    }
    for l in &town.landmarks {
        let name = match l.kind {
            LandmarkKind::AetherSpire => "Aether Spire",
            LandmarkKind::Lighthouse => "Lighthouse",
            LandmarkKind::TeslaTower => "Tesla tower",
            LandmarkKind::HeroHall => "Hero Hall",
            LandmarkKind::MooringMast => "Airfield",
            LandmarkKind::RocketGantry => "Rocket port",
            LandmarkKind::AtomicPlant => "Atomic plant",
        };
        let p = to_screen(l.pos);
        painter.circle_filled(p, 4.0, egui::Color32::from_rgb(60, 60, 70));
        painter.text(
            p + egui::vec2(6.0, 0.0),
            egui::Align2::LEFT_CENTER,
            name,
            egui::FontId::proportional(12.0),
            egui::Color32::from_rgb(50, 50, 60),
        );
    }
    for c in radio.calls.values() {
        let p = to_screen(DVec2::new(c.at.x as f64, c.at.z as f64));
        let col = con_colour(c.level, mine);
        painter.circle_filled(p, 7.0, col);
        painter.circle_stroke(p, 7.0, egui::Stroke::new(1.5_f32, egui::Color32::BLACK));
        painter.text(
            p + egui::vec2(9.0, -9.0),
            egui::Align2::LEFT_BOTTOM,
            c.kind.name(),
            egui::FontId::proportional(12.0),
            egui::Color32::BLACK,
        );
    }
    if let Some(h) = hero {
        let p = to_screen(DVec2::new(h.x as f64, h.z as f64));
        let d = glam::Vec2::new(look.x, look.z).normalize_or(glam::Vec2::Y);
        let tip = p + egui::vec2(d.x, d.y) * 12.0;
        let side = egui::vec2(-d.y, d.x) * 6.0;
        painter.add(egui::Shape::convex_polygon(
            vec![tip, p + side, p - side],
            egui::Color32::from_rgb(220, 40, 40),
            egui::Stroke::new(1.5_f32, egui::Color32::WHITE),
        ));
    }
    painter.text(
        rect.left_top() + egui::vec2(12.0, 10.0),
        egui::Align2::LEFT_TOP,
        "TOWN MAP   (M to close)   N up",
        egui::FontId::proportional(15.0),
        egui::Color32::from_rgb(90, 60, 30),
    );
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        *open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearings_run_clockwise_from_north() {
        let b = |x: f32, z: f32| bearing(glam::Vec2::new(x, z)).to_degrees().round();
        assert_eq!(b(0.0, -1.0), 0.0);
        assert_eq!(b(1.0, 0.0), 90.0);
        assert_eq!(b(0.0, 1.0), 180.0);
        assert_eq!(b(-1.0, 0.0), 270.0);
    }

    #[test]
    fn only_the_worst_trouble_lights_the_signal() {
        assert!(lights_signal(IncidentKind::Breakout));
        assert!(!lights_signal(IncidentKind::Mugging));
    }
}
