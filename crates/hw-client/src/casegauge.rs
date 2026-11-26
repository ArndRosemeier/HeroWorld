//! The little gauges of a case that runs against something: sacks of loot
//! going into a getaway car, a courier growing suspicious, a clock. Drawn
//! as a symbol and a bar (and the clock's digits), on the wrist radio and
//! under the case's mark in the patrol visor.

use engine::egui;
use engine::egui::{
    pos2, vec2, Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Stroke,
};

/// What a gauge measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GaugeKind {
    /// Loot aboard the getaway car: full, they take off.
    Loot,
    /// How suspicious the courier is: full, he has seen you.
    Suspicion,
    /// Only a clock (a car to catch, a place to get to).
    Clock,
}

/// A case's gauge: what, how full (0 to 1), and the clock if one runs.
#[derive(Clone, Debug, PartialEq)]
pub struct Gauge {
    pub kind: GaugeKind,
    pub level: f32,
    pub clock: Option<String>,
}

/// How wide and high a gauge draws (points).
pub const SIZE: egui::Vec2 = egui::vec2(190.0, 18.0);

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t.clamp(0.0, 1.0)) as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

fn faded(c: Color32, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (c.a() as f32 * alpha) as u8)
}

/// The bar's colour at `level`: calm, then amber, then red.
pub fn tone(kind: GaugeKind, level: f32) -> Color32 {
    let calm = match kind {
        GaugeKind::Loot => Color32::from_rgb(235, 190, 70),
        _ => Color32::from_rgb(110, 220, 130),
    };
    let amber = Color32::from_rgb(255, 160, 50);
    let red = Color32::from_rgb(240, 60, 50);
    if level < 0.5 {
        mix(calm, amber, level * 2.0)
    } else {
        mix(amber, red, (level - 0.5) * 2.0)
    }
}

/// A money sack, tied at the neck.
fn sack(p: &Painter, c: Pos2, r: f32, col: Color32, ink: Color32) {
    p.circle_filled(c + vec2(0.0, r * 0.22), r * 0.78, col);
    p.add(egui::Shape::convex_polygon(
        vec![
            c + vec2(-r * 0.42, -r * 0.95),
            c + vec2(r * 0.42, -r * 0.95),
            c + vec2(r * 0.2, -r * 0.4),
            c + vec2(-r * 0.2, -r * 0.4),
        ],
        col,
        Stroke::NONE,
    ));
    p.line_segment(
        [c + vec2(-r * 0.3, -r * 0.42), c + vec2(r * 0.3, -r * 0.42)],
        Stroke::new(1.6_f32, ink),
    );
    p.text(
        c + vec2(0.0, r * 0.25),
        Align2::CENTER_CENTER,
        "$",
        FontId::proportional(r * 1.1),
        ink,
    );
}

/// An eye; wide open when `wide`.
fn eye(p: &Painter, c: Pos2, r: f32, col: Color32, ink: Color32, wide: bool) {
    let h = if wide { 0.75 } else { 0.5 };
    let lid = |s: f32| -> Vec<Pos2> {
        (0..=10)
            .map(|k| {
                let u = k as f32 / 10.0;
                c + vec2(
                    (u * 2.0 - 1.0) * r * 1.2,
                    s * r * h * (std::f32::consts::PI * u).sin(),
                )
            })
            .collect()
    };
    let mut ring = lid(-1.0);
    ring.extend(lid(1.0).into_iter().rev());
    p.add(egui::Shape::convex_polygon(
        ring,
        col,
        Stroke::new(1.2_f32, ink),
    ));
    p.circle_filled(c, r * 0.36, ink);
}

/// A stopwatch.
fn watch(p: &Painter, c: Pos2, r: f32, col: Color32, ink: Color32) {
    p.rect_filled(
        Rect::from_center_size(c + vec2(0.0, -r * 0.95), vec2(r * 0.4, r * 0.35)),
        CornerRadius::same(1),
        col,
    );
    p.circle_filled(c + vec2(0.0, r * 0.1), r * 0.8, col);
    p.line_segment(
        [c + vec2(0.0, r * 0.1), c + vec2(r * 0.35, -r * 0.3)],
        Stroke::new(1.6_f32, ink),
    );
}

/// Draws `g` in `r` (about [`SIZE`]), faded by `alpha`; `time` makes a
/// full gauge and a short clock pulse.
pub fn draw(p: &Painter, r: Rect, g: &Gauge, alpha: f32, time: f32) {
    let ink = faded(Color32::from_rgb(20, 22, 34), alpha);
    let level = g.level.clamp(0.0, 1.0);
    let urgent = level >= 0.75
        || g.clock
            .as_deref()
            .is_some_and(|c| c.starts_with("0:") && c.len() == 4 && &c[2..] < "20");
    let pulse = if urgent {
        0.72 + 0.28 * (time * 7.0).sin()
    } else {
        1.0
    };
    let col = faded(tone(g.kind, level), alpha * pulse);
    let ic = pos2(r.left() + r.height() * 0.5, r.center().y);
    let ir = r.height() * 0.5;
    match g.kind {
        GaugeKind::Loot => sack(p, ic, ir, col, ink),
        GaugeKind::Suspicion => eye(p, ic, ir, col, ink, level >= 0.6),
        GaugeKind::Clock => watch(p, ic, ir, col, ink),
    }
    let mut left = r.left() + r.height() + 6.0;
    let mut right = r.right();
    if let Some(c) = &g.clock {
        let font = FontId::monospace(r.height() * 0.85);
        if g.kind == GaugeKind::Clock {
            p.text(pos2(left, r.center().y), Align2::LEFT_CENTER, c, font, col);
            return;
        }
        right -= r.height() * 2.6;
        watch(
            p,
            pos2(right + r.height() * 0.45, r.center().y),
            ir * 0.75,
            col,
            ink,
        );
        p.text(
            pos2(right + r.height() * 0.95, r.center().y),
            Align2::LEFT_CENTER,
            c,
            font,
            col,
        );
        right -= 6.0;
    }
    left = left.min(right - 20.0);
    let bar = Rect::from_min_max(
        pos2(left, r.center().y - r.height() * 0.28),
        pos2(right, r.center().y + r.height() * 0.28),
    );
    p.rect_filled(
        bar,
        CornerRadius::same(4),
        faded(Color32::from_black_alpha(170), alpha),
    );
    let fill = Rect::from_min_max(
        bar.min,
        pos2(bar.left() + bar.width() * level, bar.bottom()),
    );
    p.rect_filled(fill, CornerRadius::same(4), col);
    p.rect_stroke(
        bar,
        CornerRadius::same(4),
        Stroke::new(1.0_f32, faded(Color32::from_white_alpha(120), alpha)),
        egui::StrokeKind::Inside,
    );
    // Sacks come one at a time: a tick for each.
    if g.kind == GaugeKind::Loot {
        let n = hw_sim::missions::getaway::TRIPS;
        for k in 1..n {
            let x = bar.left() + bar.width() * k as f32 / n as f32;
            p.line_segment(
                [pos2(x, bar.top()), pos2(x, bar.bottom())],
                Stroke::new(1.0_f32, faded(Color32::from_black_alpha(150), alpha)),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gauge_goes_from_calm_to_red() {
        for kind in [GaugeKind::Loot, GaugeKind::Suspicion] {
            let (a, b) = (tone(kind, 0.0), tone(kind, 1.0));
            assert!(b.r() > 200 && b.g() < 100, "{kind:?} full is red: {b:?}");
            assert!(a.g() > 150, "{kind:?} empty is calm: {a:?}");
        }
    }
}
