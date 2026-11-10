//! The police visor: a hologram the City Patrol issues to licensed heroes.
//! It shows where trouble is without lighting up the town: when a call
//! comes in, when the hero gets near one, or when the player presses T, it
//! materialises in front of the hero's eyes (a bright line at eye height
//! that opens out, flickering, into a faint amber screen with a rim, scan
//! lines and the Patrol's star), marks each call with corner brackets, its
//! name and distance (an arrow at the edge when it is out of sight), and
//! dissolves again the same way.
//!
//! Drawn with egui over the 3D view; no entities, so nothing in the world
//! glows over a call (the Hero Signal still does, for the worst trouble).

use crate::fightview::con_colour;
use crate::radioview::{call_colour, Radio};
use engine::ui::egui;
use glam::{Mat4, Vec2, Vec3};
use std::f32::consts::{PI, TAU};

/// How long a new call keeps the visor down (s).
pub const NEW_CALL_SECONDS: f32 = 8.0;
/// Within this of a call (flat, m) the visor stays down.
pub const NEAR_CALL: f32 = 150.0;
/// How long T keeps it down (s).
pub const ASKED_SECONDS: f32 = 10.0;
/// Seconds to materialise.
const IN_SECONDS: f32 = 0.55;
/// Seconds to dissolve.
const OUT_SECONDS: f32 = 0.4;
/// Its amber.
const AMBER: (u8, u8, u8) = (255, 196, 92);

/// Why the visor is down, for the first line on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    NewCall,
    Near,
    Asked,
}

/// The visor's state: how far it has materialised and why it is wanted.
#[derive(Default)]
pub struct Visor {
    /// 0 gone, 1 fully there.
    pub shown: f32,
    /// Seconds T still keeps it down.
    asked: f32,
    /// The player put it away: it stays away until a new call comes in.
    quiet: bool,
    /// The newest call id seen (a newer one ends `quiet`).
    newest: Option<u32>,
    /// Running seconds (flicker, scan band).
    time: f32,
    /// Was it coming in (true) or going out the last frame.
    rising: bool,
    /// `HW_VISOR=<0..1>`: kept down and frozen that far along (screenshots).
    pin: Option<f32>,
}

impl Visor {
    /// A visor; `HW_VISOR=1` keeps it down, `HW_VISOR=0.4` freezes it
    /// part way through materialising (for screenshots).
    pub fn from_env() -> Visor {
        let pin = std::env::var("HW_VISOR")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .map(|v| v.clamp(0.0, 1.0));
        Visor {
            pin,
            ..Visor::default()
        }
    }

    /// Why it should be down now, if at all.
    pub fn wanted(&self, radio: &Radio, hero: Option<Vec3>) -> Option<Why> {
        if self.asked > 0.0 {
            return Some(Why::Asked);
        }
        if self.quiet {
            return None;
        }
        if radio.calls.values().any(|c| c.age < NEW_CALL_SECONDS) {
            return Some(Why::NewCall);
        }
        let near = hero.is_some_and(|h| {
            radio
                .calls
                .values()
                .any(|c| Vec2::new(c.at.x - h.x, c.at.z - h.z).length() < NEAR_CALL)
        });
        near.then_some(Why::Near)
    }

    /// One frame: `toggle` is T pressed. Returns why it is wanted.
    pub fn update(
        &mut self,
        dt: f32,
        toggle: bool,
        radio: &Radio,
        hero: Option<Vec3>,
    ) -> Option<Why> {
        self.time += dt;
        self.asked = (self.asked - dt).max(0.0);
        let newest = radio.calls.keys().max().copied();
        if newest.is_some() && newest > self.newest {
            self.quiet = false;
        }
        self.newest = self.newest.max(newest);
        if radio.calls.is_empty() {
            self.quiet = false;
        }
        if toggle {
            if self.wanted(radio, hero).is_some() {
                self.asked = 0.0;
                self.quiet = true;
            } else {
                self.asked = ASKED_SECONDS;
            }
        }
        if let Some(pin) = self.pin {
            self.asked = ASKED_SECONDS;
            self.shown = pin;
            self.rising = true;
            return self.wanted(radio, hero);
        }
        let why = self.wanted(radio, hero);
        self.rising = why.is_some();
        self.shown = if self.rising {
            (self.shown + dt / IN_SECONDS).min(1.0)
        } else {
            (self.shown - dt / OUT_SECONDS).max(0.0)
        };
        why
    }

    /// Draws it over the 3D view (under the HUD's own panels).
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        ctx: &egui::Context,
        view_proj: Mat4,
        eye: Vec3,
        radio: &Radio,
        mine: u8,
        why: Option<Why>,
    ) {
        if self.shown <= 0.0 {
            return;
        }
        let screen = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("police visor"),
        ));
        let open = Opening::at(self.shown, self.rising, self.time);
        let centre = screen.center();
        // The bright line at eye height it grows from (and shrinks into).
        if open.line > 0.0 {
            let half = screen.width() * 0.5 * open.line;
            let glow = (255.0 * open.line_glow) as u8;
            for (w, a) in [(7.0_f32, 0.25_f32), (2.0, 1.0)] {
                painter.line_segment(
                    [
                        egui::pos2(centre.x - half, centre.y),
                        egui::pos2(centre.x + half, centre.y),
                    ],
                    egui::Stroke::new(w, amber((glow as f32 * a) as u8)),
                );
            }
        }
        if open.height <= 0.0 {
            return;
        }
        // The screen opens out from the line: everything is clipped to it.
        let half_h = screen.height() * 0.5 * open.height;
        let clip = egui::Rect::from_min_max(
            egui::pos2(screen.left(), centre.y - half_h),
            egui::pos2(screen.right(), centre.y + half_h),
        );
        let p = painter.with_clip_rect(clip);
        let a = open.alpha;
        let fade = |x: f32| (x * a).round().clamp(0.0, 255.0) as u8;
        // The glass: a faint amber over everything, darker at the rim.
        p.rect_filled(screen, 0.0, amber(fade(26.0)));
        vignette(&p, screen, fade(150.0));
        // The rim: a rounded frame, with ticks at the corners.
        let rim = screen.shrink(22.0);
        p.rect_stroke(
            rim,
            38.0,
            egui::Stroke::new(3.0_f32, amber(fade(150.0))),
            egui::StrokeKind::Middle,
        );
        for (cx, cy, sx, sy) in [
            (rim.left(), rim.top(), 1.0, 1.0),
            (rim.right(), rim.top(), -1.0, 1.0),
            (rim.left(), rim.bottom(), 1.0, -1.0),
            (rim.right(), rim.bottom(), -1.0, -1.0),
        ] {
            let c = egui::pos2(cx + sx * 12.0, cy + sy * 12.0);
            let stroke = egui::Stroke::new(2.5_f32, amber(fade(200.0)));
            p.line_segment([c, c + egui::vec2(sx * 46.0, 0.0)], stroke);
            p.line_segment([c, c + egui::vec2(0.0, sy * 46.0)], stroke);
        }
        // Scan lines, and a brighter band rolling down.
        let lines = amber(fade(14.0));
        let mut y = clip.top() - clip.top().rem_euclid(4.0);
        while y < clip.bottom() {
            p.line_segment(
                [egui::pos2(screen.left(), y), egui::pos2(screen.right(), y)],
                egui::Stroke::new(1.0_f32, lines),
            );
            y += 4.0;
        }
        let band = screen.top() + (self.time / 3.5).fract() * (screen.height() + 160.0) - 80.0;
        p.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(screen.left(), band - 40.0),
                egui::pos2(screen.right(), band + 40.0),
            ),
            0.0,
            amber(fade(12.0)),
        );
        // The Patrol's star, see-through, low right of the line of sight.
        badge(
            &p,
            egui::pos2(
                centre.x + screen.width() * 0.3,
                centre.y + screen.height() * 0.18,
            ),
            70.0,
            fade(60.0),
        );
        // A word on why it came down, top right of the line of sight.
        let calls = radio.calls.len();
        let head = match why {
            _ if calls == 0 => "PATROL VISOR - NO CALLS".to_string(),
            Some(Why::NewCall) => format!("PATROL VISOR - NEW CALL ({calls} OPEN)"),
            Some(Why::Near) => "PATROL VISOR - CALL NEARBY".to_string(),
            _ => format!(
                "PATROL VISOR - {calls} CALL{} OPEN",
                if calls == 1 { "" } else { "S" }
            ),
        };
        shadowed(
            &p,
            egui::pos2(rim.right() - 70.0, rim.top() + 54.0),
            egui::Align2::RIGHT_TOP,
            &head,
            14.0,
            amber(fade(240.0)),
            fade(160.0),
        );
        shadowed(
            &p,
            egui::pos2(rim.right() - 70.0, rim.top() + 74.0),
            egui::Align2::RIGHT_TOP,
            "T: put away",
            12.0,
            amber(fade(200.0)),
            fade(140.0),
        );
        // The calls: brackets on the spot, or an arrow at the rim.
        // Distances as the wrist radio gives them: from the eye.
        let from = eye;
        let jitter = open.jitter;
        for c in radio.calls.values() {
            let flat = Vec2::new(c.at.x - from.x, c.at.z - from.z).length();
            let (r, g, b) = call_colour(c.kind);
            let tint = egui::Color32::from_rgba_unmultiplied(r, g, b, fade(230.0));
            let con = con_colour(c.level, mine);
            let label_col =
                egui::Color32::from_rgba_unmultiplied(con.r(), con.g(), con.b(), fade(235.0));
            let label = format!("{}  {:.0} m", c.kind.name().to_uppercase(), flat);
            // Fresh calls pulse for their first seconds.
            let pulse = if c.age < NEW_CALL_SECONDS {
                1.0 + 0.25 * (c.age * TAU * 1.5).sin().abs()
            } else {
                1.0
            };
            match on_screen(view_proj, c.at + Vec3::Y * 1.6, screen) {
                Some(at) => {
                    let at = at + egui::vec2(jitter, 0.0);
                    let s = (1400.0 / flat.max(1.0)).clamp(14.0, 56.0) * pulse;
                    brackets(&p, at, s, egui::Stroke::new(2.0_f32, tint));
                    let d = 4.0;
                    p.add(egui::Shape::convex_polygon(
                        vec![
                            at + egui::vec2(0.0, -d),
                            at + egui::vec2(d, 0.0),
                            at + egui::vec2(0.0, d),
                            at + egui::vec2(-d, 0.0),
                        ],
                        tint,
                        egui::Stroke::NONE,
                    ));
                    shadowed(
                        &p,
                        at + egui::vec2(0.0, s + 6.0),
                        egui::Align2::CENTER_TOP,
                        &label,
                        14.0,
                        label_col,
                        fade(150.0),
                    );
                }
                None => {
                    let dir = off_screen_dir(view_proj, c.at);
                    let r = egui::vec2(screen.width() * 0.42, screen.height() * 0.38);
                    let tip =
                        centre + egui::vec2(dir.x * r.x, dir.y * r.y) + egui::vec2(jitter, 0.0);
                    let fwd = egui::vec2(dir.x, dir.y);
                    let side = egui::vec2(-fwd.y, fwd.x);
                    let size = 14.0 * pulse;
                    p.add(egui::Shape::convex_polygon(
                        vec![
                            tip + fwd * size,
                            tip - fwd * size * 0.6 + side * size * 0.8,
                            tip - fwd * size * 0.6 - side * size * 0.8,
                        ],
                        tint,
                        egui::Stroke::NONE,
                    ));
                    let at = tip - fwd * (size + 14.0);
                    shadowed(
                        &p,
                        at,
                        egui::Align2::CENTER_CENTER,
                        &label,
                        12.0,
                        label_col,
                        fade(150.0),
                    );
                }
            }
        }
    }
}

/// How far the hologram is along: the line, then the screen opening out.
struct Opening {
    /// Width of the eye-height line, 0..1 of the screen.
    line: f32,
    /// How bright the line is, 0..1.
    line_glow: f32,
    /// Height of the open screen, 0..1.
    height: f32,
    /// How solid the screen is (flickers while it forms), 0..1.
    alpha: f32,
    /// Sideways shake of its marks while it forms (px).
    jitter: f32,
}

impl Opening {
    /// `shown` 0..1, `rising` whether it is coming or going, `time` for
    /// the flicker.
    fn at(shown: f32, rising: bool, time: f32) -> Opening {
        // A quarter of the way: the line grows; then the screen opens.
        let line_part = 0.25;
        let line = (shown / line_part).min(1.0);
        let rest = ((shown - line_part) / (1.0 - line_part)).clamp(0.0, 1.0);
        let height = ease(rest);
        // The flicker: strong while forming, gone once settled.
        let unsettled = 1.0 - rest;
        let n = flicker(time);
        let alpha = (rest * (1.0 - unsettled * 0.6 * n)).clamp(0.0, 1.0);
        let jitter = if rest < 1.0 {
            (n - 0.5) * 10.0 * unsettled
        } else {
            0.0
        };
        // The line burns bright while the screen opens and fades once it is
        // full; going out it flares as the screen collapses into it.
        let line_glow = if rising {
            (1.0 - rest * 0.85) * line
        } else {
            (1.0 - rest * 0.6) * line
        };
        Opening {
            line: if rest >= 1.0 { 0.0 } else { line },
            line_glow,
            height,
            alpha,
            jitter,
        }
    }
}

fn ease(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// A jumpy 0..1 value, new 30 times a second.
fn flicker(time: f32) -> f32 {
    let k = (time * 30.0) as u32;
    let mut x = k.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65535.0
}

fn amber(alpha: u8) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(AMBER.0, AMBER.1, AMBER.2, alpha)
}

/// The glass darkens and warms toward its rim.
fn vignette(p: &egui::Painter, screen: egui::Rect, alpha: u8) {
    let inner = screen.shrink2(egui::vec2(screen.width() * 0.16, screen.height() * 0.16));
    let edge = egui::Color32::from_rgba_unmultiplied(40, 26, 8, alpha);
    let clear = egui::Color32::TRANSPARENT;
    let mut m = egui::Mesh::default();
    let outer = [
        screen.left_top(),
        screen.right_top(),
        screen.right_bottom(),
        screen.left_bottom(),
    ];
    let inner_pts = [
        inner.left_top(),
        inner.right_top(),
        inner.right_bottom(),
        inner.left_bottom(),
    ];
    for i in 0..4 {
        m.colored_vertex(outer[i], edge);
        m.colored_vertex(inner_pts[i], clear);
    }
    for i in 0..4u32 {
        let j = (i + 1) % 4;
        let (o0, i0, o1, i1) = (i * 2, i * 2 + 1, j * 2, j * 2 + 1);
        m.add_triangle(o0, o1, i1);
        m.add_triangle(o0, i1, i0);
    }
    p.add(egui::Shape::mesh(m));
}

/// The City Patrol's star in a ring, see-through.
fn badge(p: &egui::Painter, c: egui::Pos2, r: f32, alpha: u8) {
    let col = amber(alpha);
    let star: Vec<egui::Pos2> = (0..10)
        .map(|i| {
            let a = -PI / 2.0 + PI * i as f32 / 5.0;
            let rr = if i % 2 == 0 { r * 0.78 } else { r * 0.34 };
            c + egui::vec2(a.cos() * rr, a.sin() * rr)
        })
        .collect();
    // Filled as a fan from the middle (a star is not convex).
    let mut m = egui::Mesh::default();
    let fill = amber(alpha / 2);
    m.colored_vertex(c, fill);
    for &q in &star {
        m.colored_vertex(q, fill);
    }
    for i in 0..10u32 {
        m.add_triangle(0, 1 + i, 1 + (i + 1) % 10);
    }
    p.add(egui::Shape::mesh(m));
    p.add(egui::Shape::closed_line(
        star,
        egui::Stroke::new(1.5_f32, col),
    ));
    p.circle_stroke(c, r, egui::Stroke::new(2.0_f32, col));
    p.circle_stroke(c, r * 1.12, egui::Stroke::new(1.0_f32, col));
    p.text(
        c + egui::vec2(0.0, r * 1.32),
        egui::Align2::CENTER_TOP,
        "METRO CITY PATROL",
        egui::FontId::monospace(12.0),
        col,
    );
}

/// Four corner brackets round `at`.
fn brackets(p: &egui::Painter, at: egui::Pos2, s: f32, stroke: egui::Stroke) {
    let l = s * 0.45;
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let c = at + egui::vec2(sx * s, sy * s);
        p.line_segment([c, c - egui::vec2(sx * l, 0.0)], stroke);
        p.line_segment([c, c - egui::vec2(0.0, sy * l)], stroke);
    }
}

/// Text with a soft dark copy under it, so it reads over a bright sky.
fn shadowed(
    p: &egui::Painter,
    at: egui::Pos2,
    align: egui::Align2,
    text: &str,
    size: f32,
    col: egui::Color32,
    shade: u8,
) {
    let font = egui::FontId::monospace(size);
    p.text(
        at + egui::vec2(1.0, 1.0),
        align,
        text,
        font.clone(),
        egui::Color32::from_black_alpha(shade),
    );
    p.text(at, align, text, font, col);
}

/// Where a point shows on screen, if it is in front and inside the visor.
fn on_screen(view_proj: Mat4, p: Vec3, screen: egui::Rect) -> Option<egui::Pos2> {
    let c = view_proj * p.extend(1.0);
    if c.w <= 0.05 {
        return None;
    }
    let n = c.truncate() / c.w;
    if n.x.abs() > 0.86 || n.y.abs() > 0.8 {
        return None;
    }
    Some(egui::pos2(
        screen.left() + (n.x * 0.5 + 0.5) * screen.width(),
        screen.top() + (0.5 - n.y * 0.5) * screen.height(),
    ))
}

/// Which way to point at something out of sight: a unit vector in screen
/// terms (x right, y down).
fn off_screen_dir(view_proj: Mat4, p: Vec3) -> Vec2 {
    let c = view_proj * p.extend(1.0);
    let mut v = Vec2::new(c.x, -c.y);
    if c.w < 0.0 {
        // Behind: clip space mirrors, so turn it back.
        v = -v;
    }
    if v.length_squared() < 1e-6 {
        // Straight behind: point down, toward "turn round".
        return Vec2::Y;
    }
    v.normalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_forms_and_dissolves_through_a_line() {
        // Just started: only the line.
        let o = Opening::at(0.1, true, 0.0);
        assert!(o.line > 0.0 && o.height == 0.0);
        // Fully there: no line, full and steady.
        let o = Opening::at(1.0, true, 0.37);
        assert_eq!(o.line, 0.0);
        assert_eq!(o.height, 1.0);
        assert_eq!(o.alpha, 1.0);
        assert_eq!(o.jitter, 0.0);
        // Half way out: the screen is closing onto the line.
        let o = Opening::at(0.5, false, 0.2);
        assert!(o.height > 0.0 && o.height < 1.0 && o.line > 0.0);
    }

    #[test]
    fn t_brings_it_down_and_puts_it_away() {
        let radio = Radio::default();
        let mut v = Visor::default();
        assert_eq!(v.update(0.1, false, &radio, None), None);
        assert_eq!(v.update(0.1, true, &radio, None), Some(Why::Asked));
        for _ in 0..10 {
            v.update(0.1, false, &radio, None);
        }
        assert_eq!(v.shown, 1.0);
        assert_eq!(v.update(0.1, true, &radio, None), None);
        for _ in 0..10 {
            v.update(0.1, false, &radio, None);
        }
        assert_eq!(v.shown, 0.0);
    }
}
