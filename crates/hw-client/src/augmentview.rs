//! Augments on screen: an icon for every effect, the chip an augment is
//! shown as (its frame says how good it is), the sockets under a power in
//! the powers window and the bag below it.
//!
//! The rules live in `hw_sim::augments`; the host checks every change
//! again, so this only asks.

use engine::ui::egui::{self, pos2, vec2, Color32, Pos2, Rect, Shape, Stroke};
use hw_net::{Augment, PowerId, Sheet, MAX_BAG, SOCKETS};
use hw_sim::augments::{self, Effect, Grade};
use hw_sim::powers::{self, PowerDef};
use hw_sim::progress::level_of;

/// A chip in the bag and in a socket (px).
pub const CHIP: f32 = 42.0;
/// The bag: chips in a row, the gap between them, and its width.
const BAG_COLUMNS: usize = 5;
const BAG_GAP: f32 = 5.0;
pub const BAG_WIDTH: f32 = BAG_COLUMNS as f32 * (CHIP + BAG_GAP) - BAG_GAP;
const INK: Color32 = Color32::from_rgb(18, 20, 44);
const LIT: Color32 = Color32::from_rgb(110, 240, 200);
const BETTER: Color32 = Color32::from_rgb(120, 235, 120);

/// The frame of an augment: copper for a common one, aether blue for a
/// rare one, gold for a relic.
pub fn grade_colour(g: Grade) -> Color32 {
    match g {
        Grade::Common => Color32::from_rgb(214, 150, 104),
        Grade::Rare => Color32::from_rgb(96, 196, 255),
        Grade::Relic => Color32::from_rgb(255, 208, 74),
    }
}

/// The colour of an effect's icon, by what kind of thing it does: harm
/// red, control violet, reach and shape teal, defence green, strength
/// amber, travel sky blue.
pub fn effect_colour(e: Effect) -> Color32 {
    use Effect::*;
    match e {
        Damage | Burn | Drain | Exploit | Leap | Wreck | Trail | Stomp => {
            Color32::from_rgb(255, 132, 96)
        }
        Knockdown | Launch | Push | Hold | Freeze | Slow | Confuse | Fear | Charm | Disarm
        | Weaken => Color32::from_rgb(200, 150, 255),
        Range | Area | Lasting | Quick | Thrifty | Light | Spare => {
            Color32::from_rgb(110, 226, 214)
        }
        Heal | Shield | Reflect | Resist | Dodge | Brace | Regen | Ward => {
            Color32::from_rgb(140, 232, 140)
        }
        Might | Haste | Aura => Color32::from_rgb(255, 204, 96),
        Speed | Height => Color32::from_rgb(140, 200, 255),
    }
}

/// Draws effect `e`'s icon round `c`, `r` px to its edge.
pub fn glyph(painter: &egui::Painter, c: Pos2, r: f32, e: Effect, col: Color32) {
    let p = |x: f32, y: f32| c + vec2(x, y) * r;
    let w = (r * 0.17).max(1.4);
    let pen = Stroke::new(w, col);
    let line = |pts: &[(f32, f32)]| {
        let pts: Vec<Pos2> = pts.iter().map(|&(x, y)| p(x, y)).collect();
        painter.add(Shape::line(pts, pen));
    };
    let fill = |pts: &[(f32, f32)]| {
        let pts: Vec<Pos2> = pts.iter().map(|&(x, y)| p(x, y)).collect();
        painter.add(Shape::convex_polygon(pts, col, Stroke::NONE));
    };
    let dot = |x: f32, y: f32, s: f32| {
        painter.circle_filled(p(x, y), s * r, col);
    };
    let ring = |x: f32, y: f32, s: f32| {
        painter.circle_stroke(p(x, y), s * r, pen);
    };
    let shield: [(f32, f32); 6] = [
        (-0.58, -0.62),
        (0.58, -0.62),
        (0.58, 0.05),
        (0.0, 0.78),
        (-0.58, 0.05),
        (-0.58, -0.62),
    ];
    let heart = |y: f32, s: f32| {
        dot(-0.3 * s, y - 0.2 * s, 0.36 * s);
        dot(0.3 * s, y - 0.2 * s, 0.36 * s);
        fill(&[
            (-0.64 * s, y - 0.04 * s),
            (0.64 * s, y - 0.04 * s),
            (0.0, y + 0.72 * s),
        ]);
    };
    match e {
        Effect::Damage => {
            // A burst: eight spikes.
            for k in 0..4 {
                let a = k as f32 * std::f32::consts::FRAC_PI_4;
                let len = if k % 2 == 0 { 0.8 } else { 0.5 };
                let (s, co) = a.sin_cos();
                line(&[(-co * len, -s * len), (co * len, s * len)]);
            }
            dot(0.0, 0.0, 0.24);
        }
        Effect::Burn => {
            // A flame.
            dot(0.0, 0.3, 0.44);
            fill(&[(-0.42, 0.18), (0.42, 0.18), (0.08, -0.82)]);
            painter.circle_filled(p(0.0, 0.38), 0.2 * r, INK);
        }
        Effect::Drain => {
            // Fangs.
            line(&[(-0.7, -0.45), (0.7, -0.45)]);
            fill(&[(-0.55, -0.4), (-0.12, -0.4), (-0.33, 0.6)]);
            fill(&[(0.12, -0.4), (0.55, -0.4), (0.33, 0.6)]);
        }
        Effect::Exploit => {
            // A crosshair.
            ring(0.0, 0.0, 0.5);
            for (x, y) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                line(&[(x * 0.3, y * 0.3), (x * 0.82, y * 0.82)]);
            }
        }
        Effect::Leap => {
            // A hit jumping from one to the next.
            line(&[(-0.6, 0.45), (0.0, -0.45), (0.6, 0.45)]);
            dot(-0.6, 0.45, 0.2);
            dot(0.0, -0.45, 0.2);
            dot(0.6, 0.45, 0.2);
        }
        Effect::Knockdown => {
            // Someone flat on the ground, seeing stars.
            line(&[(-0.8, 0.68), (0.8, 0.68)]);
            dot(-0.52, 0.36, 0.19);
            line(&[(-0.26, 0.4), (0.6, 0.4)]);
            line(&[(-0.2, -0.05), (-0.42, -0.5)]);
            line(&[(0.1, -0.15), (0.1, -0.68)]);
            line(&[(0.4, -0.05), (0.62, -0.5)]);
        }
        Effect::Launch => {
            // Up off the ground.
            line(&[(-0.7, 0.72), (0.7, 0.72)]);
            line(&[(0.0, 0.45), (0.0, -0.3)]);
            fill(&[(-0.36, -0.2), (0.36, -0.2), (0.0, -0.8)]);
        }
        Effect::Push => {
            // Shoved away from a wall.
            line(&[(-0.66, -0.6), (-0.66, 0.6)]);
            line(&[(-0.38, 0.0), (0.3, 0.0)]);
            fill(&[(0.22, -0.36), (0.22, 0.36), (0.8, 0.0)]);
        }
        Effect::Hold => {
            // A padlock.
            ring(0.0, -0.3, 0.32);
            let body = Rect::from_min_max(p(-0.5, -0.1), p(0.5, 0.7));
            painter.rect_filled(body, r * 0.12, col);
            painter.circle_filled(p(0.0, 0.3), 0.13 * r, INK);
        }
        Effect::Freeze => {
            // A snowflake.
            for k in 0..3 {
                let a = k as f32 * std::f32::consts::FRAC_PI_3 + std::f32::consts::FRAC_PI_2;
                let (s, co) = a.sin_cos();
                line(&[(-co * 0.78, -s * 0.78), (co * 0.78, s * 0.78)]);
                for end in [-1.0f32, 1.0] {
                    let (ex, ey) = (co * 0.5 * end, s * 0.5 * end);
                    let (tx, ty) = (-s * 0.2, co * 0.2);
                    let (ox, oy) = (co * 0.2 * end, s * 0.2 * end);
                    line(&[
                        (ex + tx + ox, ey + ty + oy),
                        (ex, ey),
                        (ex - tx + ox, ey - ty + oy),
                    ]);
                }
            }
        }
        Effect::Slow => {
            // An hourglass, the sand run down.
            line(&[
                (-0.48, -0.74),
                (0.48, -0.74),
                (-0.48, 0.74),
                (0.48, 0.74),
                (-0.48, -0.74),
            ]);
            fill(&[(-0.44, 0.7), (0.44, 0.7), (0.0, 0.08)]);
        }
        Effect::Confuse => {
            // A spiral.
            let pts: Vec<(f32, f32)> = (0..=40)
                .map(|k| {
                    let t = k as f32 / 40.0;
                    let a = t * std::f32::consts::TAU * 2.1;
                    let rad = 0.08 + 0.7 * t;
                    (a.cos() * rad, a.sin() * rad)
                })
                .collect();
            line(&pts);
        }
        Effect::Fear => {
            // Wide eyes and chattering teeth.
            ring(-0.36, -0.2, 0.28);
            ring(0.36, -0.2, 0.28);
            dot(-0.36, -0.2, 0.1);
            dot(0.36, -0.2, 0.1);
            line(&[
                (-0.6, 0.62),
                (-0.3, 0.4),
                (0.0, 0.62),
                (0.3, 0.4),
                (0.6, 0.62),
            ]);
        }
        Effect::Charm => heart(0.0, 1.1),
        Effect::Disarm => {
            // A sword snapped in two.
            line(&[(-0.72, 0.72), (-0.1, 0.1)]);
            line(&[(-0.66, 0.22), (-0.22, 0.66)]);
            line(&[(0.22, -0.22), (0.74, -0.74)]);
            line(&[(-0.2, -0.3), (0.3, 0.2)]);
        }
        Effect::Weaken => {
            // A cracked shield.
            line(&shield);
            line(&[(0.12, -0.6), (-0.14, -0.2), (0.14, 0.06), (-0.04, 0.5)]);
        }
        Effect::Range => {
            // A shot that carries far.
            line(&[(-0.78, 0.78), (-0.5, 0.5)]);
            line(&[(-0.3, 0.3), (-0.02, 0.02)]);
            line(&[(0.14, -0.14), (0.36, -0.36)]);
            fill(&[(0.08, -0.5), (0.5, -0.08), (0.8, -0.8)]);
        }
        Effect::Area => {
            ring(0.0, 0.0, 0.72);
            ring(0.0, 0.0, 0.4);
            dot(0.0, 0.0, 0.13);
        }
        Effect::Wreck => {
            // A wrecking ball.
            line(&[(-0.66, -0.76), (0.1, 0.16)]);
            dot(0.22, 0.34, 0.4);
            line(&[(-0.8, -0.76), (-0.3, -0.76)]);
        }
        Effect::Spare => {
            // A house left standing.
            line(&[(-0.78, -0.02), (0.0, -0.74), (0.78, -0.02)]);
            line(&[(-0.54, -0.12), (-0.54, 0.7), (0.54, 0.7), (0.54, -0.12)]);
            fill(&[(-0.14, 0.7), (-0.14, 0.22), (0.14, 0.22), (0.14, 0.7)]);
        }
        Effect::Heal => {
            fill(&[(-0.22, -0.72), (0.22, -0.72), (0.22, 0.72), (-0.22, 0.72)]);
            fill(&[(-0.72, -0.22), (0.72, -0.22), (0.72, 0.22), (-0.72, 0.22)]);
        }
        Effect::Shield => {
            // A shield, a star on it.
            line(&shield);
            dot(0.0, -0.05, 0.17);
        }
        Effect::Reflect => {
            // A blow thrown back off a wall.
            line(&[(0.62, -0.75), (0.62, 0.75)]);
            line(&[(-0.72, -0.62), (0.5, 0.0), (-0.42, 0.47)]);
            fill(&[(-0.26, 0.2), (-0.1, 0.56), (-0.76, 0.66)]);
        }
        Effect::Lasting => {
            // A clock.
            ring(0.0, 0.0, 0.72);
            line(&[(0.0, -0.46), (0.0, 0.0), (0.34, 0.18)]);
        }
        Effect::Quick => {
            // Round again sooner.
            let pts: Vec<(f32, f32)> = (0..=30)
                .map(|k| {
                    let a = -0.5 + k as f32 / 30.0 * 5.0;
                    (a.cos() * 0.62, a.sin() * 0.62)
                })
                .collect();
            line(&pts);
            let a = -0.5f32;
            let (hx, hy) = (a.cos() * 0.62, a.sin() * 0.62);
            fill(&[
                (hx - 0.3, hy - 0.05),
                (hx + 0.3, hy - 0.12),
                (hx + 0.06, hy + 0.36),
            ]);
        }
        Effect::Thrifty => {
            // A battery that lasts.
            line(&[
                (-0.7, -0.4),
                (0.5, -0.4),
                (0.5, 0.4),
                (-0.7, 0.4),
                (-0.7, -0.4),
            ]);
            fill(&[(0.5, -0.18), (0.74, -0.18), (0.74, 0.18), (0.5, 0.18)]);
            fill(&[(-0.52, -0.22), (-0.2, -0.22), (-0.2, 0.22), (-0.52, 0.22)]);
            fill(&[(-0.08, -0.22), (0.24, -0.22), (0.24, 0.22), (-0.08, 0.22)]);
        }
        Effect::Resist => {
            // A solid shield.
            fill(&shield[..5]);
        }
        Effect::Dodge => {
            // Where you were, and where you are.
            painter.circle_stroke(p(-0.38, 0.0), 0.3 * r, Stroke::new(w * 0.6, col));
            dot(0.42, 0.0, 0.32);
            line(&[(-0.02, -0.26), (0.1, -0.26)]);
            line(&[(-0.02, 0.26), (0.1, 0.26)]);
            line(&[(-0.78, -0.56), (0.1, -0.56)]);
            line(&[(-0.78, 0.56), (0.1, 0.56)]);
        }
        Effect::Brace => {
            // A brick wall.
            line(&[
                (-0.74, -0.54),
                (0.74, -0.54),
                (0.74, 0.54),
                (-0.74, 0.54),
                (-0.74, -0.54),
            ]);
            line(&[(-0.74, 0.0), (0.74, 0.0)]);
            line(&[(0.0, -0.54), (0.0, 0.0)]);
            line(&[(-0.37, 0.0), (-0.37, 0.54)]);
            line(&[(0.37, 0.0), (0.37, 0.54)]);
        }
        Effect::Regen => {
            // A sprout.
            line(&[(0.0, 0.78), (0.0, -0.05)]);
            fill(&[(0.0, -0.02), (-0.2, -0.5), (-0.7, -0.6), (-0.5, -0.12)]);
            fill(&[(0.0, 0.2), (0.24, -0.26), (0.72, -0.3), (0.5, 0.14)]);
        }
        Effect::Might => {
            // A dumbbell.
            painter.line_segment([p(-0.46, 0.0), p(0.46, 0.0)], Stroke::new(w * 1.5, col));
            fill(&[(-0.8, -0.46), (-0.44, -0.46), (-0.44, 0.46), (-0.8, 0.46)]);
            fill(&[(0.44, -0.46), (0.8, -0.46), (0.8, 0.46), (0.44, 0.46)]);
        }
        Effect::Haste => {
            // Fast forward.
            fill(&[(-0.76, -0.56), (-0.76, 0.56), (0.0, 0.0)]);
            fill(&[(0.0, -0.56), (0.0, 0.56), (0.76, 0.0)]);
        }
        Effect::Aura => {
            // A light others stand in.
            dot(0.0, 0.0, 0.3);
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::FRAC_PI_4;
                let (s, co) = a.sin_cos();
                line(&[(co * 0.5, s * 0.5), (co * 0.8, s * 0.8)]);
            }
        }
        Effect::Ward => {
            // A dome over two friends.
            let pts: Vec<(f32, f32)> = (0..=20)
                .map(|k| {
                    let a = std::f32::consts::PI * (1.0 + k as f32 / 20.0);
                    (a.cos() * 0.76, 0.56 + a.sin() * 0.98)
                })
                .collect();
            line(&pts);
            line(&[(-0.8, 0.58), (0.8, 0.58)]);
            dot(-0.26, 0.26, 0.17);
            dot(0.26, 0.26, 0.17);
        }
        Effect::Light => {
            // A feather.
            line(&[(-0.62, 0.76), (0.5, -0.62)]);
            fill(&[(-0.3, 0.38), (-0.36, -0.16), (0.3, -0.76), (0.62, -0.7)]);
            fill(&[(-0.3, 0.38), (0.62, -0.7), (0.5, -0.1), (0.1, 0.34)]);
        }
        Effect::Speed => {
            line(&[(-0.8, -0.4), (0.1, -0.4)]);
            line(&[(-0.5, 0.0), (0.3, 0.0)]);
            line(&[(-0.8, 0.4), (0.1, 0.4)]);
            fill(&[(0.3, -0.42), (0.3, 0.42), (0.84, 0.0)]);
        }
        Effect::Height => {
            line(&[(-0.56, -0.04), (0.0, -0.6), (0.56, -0.04)]);
            line(&[(-0.56, 0.6), (0.0, 0.04), (0.56, 0.6)]);
        }
        Effect::Stomp => {
            // Down hard, the ground cracking.
            line(&[(0.0, -0.8), (0.0, 0.05)]);
            fill(&[(-0.34, 0.0), (0.34, 0.0), (0.0, 0.5)]);
            line(&[(-0.8, 0.74), (-0.3, 0.74)]);
            line(&[(0.3, 0.74), (0.8, 0.74)]);
            line(&[(-0.5, 0.5), (-0.74, 0.2)]);
            line(&[(0.5, 0.5), (0.74, 0.2)]);
        }
        Effect::Trail => {
            // A runner and the wake behind.
            dot(0.52, 0.0, 0.24);
            for y in [-0.4f32, 0.0, 0.4] {
                let pts: Vec<(f32, f32)> = (0..=12)
                    .map(|k| {
                        let x = -0.8 + k as f32 / 12.0 * if y == 0.0 { 0.95 } else { 1.1 };
                        (x, y + (x * 9.0).sin() * 0.1)
                    })
                    .collect();
                line(&pts);
            }
        }
    }
}

/// An augment as a chip in `r`: the effect's icon in a frame of its
/// grade's colour, its percent below. `glow` 0..1 lights it up (picked,
/// or it may be dropped here).
pub fn chip(painter: &egui::Painter, r: Rect, a: Augment, t: f32, glow: f32) {
    let Some(e) = Effect::of(a) else {
        return;
    };
    let grade = Grade::of(a);
    let frame = grade_colour(grade);
    let round = r.width() * 0.2;
    painter.rect_filled(
        r.translate(vec2(1.5, 2.0)),
        round,
        Color32::from_black_alpha(140),
    );
    // Deeper and richer inside the better it is.
    let inside = match grade {
        Grade::Common => Color32::from_rgb(34, 30, 44),
        Grade::Rare => Color32::from_rgb(22, 36, 70),
        Grade::Relic => Color32::from_rgb(58, 40, 18),
    };
    painter.rect_filled(r, round, inside);
    let top = Rect::from_min_max(r.min, pos2(r.max.x, r.min.y + r.height() * 0.45));
    painter.rect_filled(top.shrink(2.0), round, Color32::from_white_alpha(10));
    let width = match grade {
        Grade::Common => 1.6,
        Grade::Rare => 2.2,
        Grade::Relic => 2.8,
    };
    painter.rect_stroke(
        r,
        round,
        Stroke::new(width, frame),
        egui::StrokeKind::Inside,
    );
    if grade != Grade::Common {
        painter.rect_stroke(
            r.shrink(width + 1.5),
            round * 0.7,
            Stroke::new(1.0_f32, frame.gamma_multiply(0.45)),
            egui::StrokeKind::Inside,
        );
    }
    let centre = r.center() - vec2(0.0, r.height() * 0.1);
    glyph(painter, centre, r.width() * 0.3, e, effect_colour(e));
    painter.text(
        r.center_bottom() - vec2(0.0, 2.0),
        egui::Align2::CENTER_BOTTOM,
        format!("+{}%", a.percent),
        egui::FontId::proportional((r.width() * 0.25).max(9.0)),
        frame,
    );
    if grade == Grade::Relic {
        // A relic glints.
        let tw = 0.6 + 0.4 * (t * 3.0).sin();
        crate::uikit::star(
            painter,
            r.right_top() + vec2(-5.0, 5.0),
            4.0 + tw * 2.0,
            t * 0.6,
            Color32::from_rgb(255, 244, 200).gamma_multiply(0.6 + 0.4 * tw),
        );
    }
    if glow > 0.0 {
        painter.rect_stroke(
            r.expand(2.0),
            round + 2.0,
            Stroke::new(2.0_f32, LIT.gamma_multiply(glow)),
            egui::StrokeKind::Outside,
        );
    }
}

/// The glint over a villain that carries an augment: a cut gem in the
/// grade's colour, `r` px to its points, twinkling.
pub fn gem(painter: &egui::Painter, c: Pos2, r: f32, grade: Grade, t: f32) {
    let col = grade_colour(grade);
    let tw = 0.5 + 0.5 * (t * 3.0).sin();
    painter.circle_filled(c, r * (1.7 + 0.3 * tw), col.gamma_multiply(0.14 + 0.1 * tw));
    let pts = |s: f32| {
        vec![
            c + vec2(0.0, -r) * s,
            c + vec2(r * 0.8, -r * 0.2) * s,
            c + vec2(0.0, r) * s,
            c + vec2(-r * 0.8, -r * 0.2) * s,
        ]
    };
    painter.add(Shape::convex_polygon(
        pts(1.0),
        col,
        Stroke::new(1.5_f32, INK),
    ));
    // The lit facet.
    painter.add(Shape::convex_polygon(
        vec![
            c + vec2(0.0, -r),
            c + vec2(r * 0.8, -r * 0.2),
            c + vec2(0.0, -r * 0.2),
        ],
        Color32::from_white_alpha(90),
        Stroke::NONE,
    ));
    if grade != Grade::Common {
        crate::uikit::star(
            painter,
            c + vec2(r * 0.7, -r * 0.9),
            r * (0.5 + 0.4 * tw),
            t * 0.8,
            Color32::from_rgb(255, 250, 220).gamma_multiply(0.5 + 0.5 * tw),
        );
    }
}

/// An empty socket in `r` (`glow` as for [`chip`]).
pub fn empty_socket(painter: &egui::Painter, r: Rect, glow: f32) {
    let round = r.width() * 0.2;
    painter.rect_filled(r, round, Color32::from_rgb(20, 22, 36));
    painter.rect_stroke(
        r,
        round,
        Stroke::new(1.5_f32, Color32::from_gray(96)),
        egui::StrokeKind::Inside,
    );
    painter.circle_stroke(
        r.center(),
        r.width() * 0.2,
        Stroke::new(1.5_f32, Color32::from_gray(80)),
    );
    if glow > 0.0 {
        painter.rect_stroke(
            r.expand(2.0),
            round + 2.0,
            Stroke::new(2.5_f32, LIT.gamma_multiply(glow)),
            egui::StrokeKind::Outside,
        );
        painter.circle_filled(r.center(), r.width() * 0.1, LIT.gamma_multiply(glow));
    }
}

/// Three small dots for a power's sockets, right to left from `at`: lit
/// in the grade's colour where one is set, dim where one is free.
pub fn socket_dots(painter: &egui::Painter, at: Pos2, sheet: &Sheet, d: &PowerDef, size: f32) {
    let n = augments::sockets_of(d);
    let set = sheet.sockets(d.id);
    for (k, &a) in set.iter().enumerate().take(n) {
        let c = at - vec2((n - 1 - k) as f32 * size * 2.6, 0.0);
        if a.is_none() {
            painter.circle_filled(c, size, Color32::from_black_alpha(150));
            painter.circle_stroke(c, size, Stroke::new(1.0_f32, Color32::from_gray(120)));
        } else {
            painter.circle_filled(c, size + 0.5, grade_colour(Grade::of(a)));
            painter.circle_stroke(c, size + 0.5, Stroke::new(1.0_f32, INK));
        }
    }
}

/// "Hold +12%", in words.
pub fn title(a: Augment) -> String {
    match Effect::of(a) {
        Some(e) => format!("{} +{}%", e.name(), a.percent),
        None => "?".into(),
    }
}

/// Would `a` make one of the hero's powers better right now: more of an
/// effect a power already has set, or a new one where a socket is free?
pub fn is_better(sheet: &Sheet, a: Augment) -> bool {
    let Some(e) = Effect::of(a) else {
        return false;
    };
    owned(sheet).any(|d| {
        if !augments::fits(e, d) {
            return false;
        }
        let set = sheet.sockets(d.id);
        let n = augments::sockets_of(d);
        match set[..n]
            .iter()
            .find(|s| Effect::of(**s) == Some(e) && !s.is_none())
        {
            Some(same) => same.percent < a.percent,
            None => set[..n].iter().any(|s| s.is_none()),
        }
    })
}

/// The powers the hero has, free ones too.
fn owned(sheet: &Sheet) -> impl Iterator<Item = &'static PowerDef> + '_ {
    powers::POWERS.iter().filter(|d| sheet.has(d.id))
}

/// What the player asked for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AugmentAction {
    /// The augment at this place in the bag into this socket, for good.
    Set { bag: u8, power: PowerId, slot: u8 },
    /// The augment at this place in the bag traded in for karma.
    Trade { bag: u8 },
}

/// What is dragged out of the bag: the place in the bag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BagDrag(usize);

/// The sockets and the bag in the powers window.
#[derive(Default)]
pub struct AugmentUi {
    /// The augment picked in the bag (a click), by its place there.
    picked: Option<usize>,
    /// A set that would destroy what is in the socket, waiting for a yes:
    /// place in the bag, power, socket.
    replace: Option<(usize, PowerId, u8)>,
    /// A trade of something better than common, waiting for a yes.
    trade: Option<usize>,
    /// The bag as it was last frame: when it changes, places in it mean
    /// something else, so what was picked is dropped.
    seen: Vec<Augment>,
}

impl AugmentUi {
    /// The augment in hand: dragged, else picked.
    pub fn in_hand(&self, ctx: &egui::Context, sheet: &Sheet) -> Option<(usize, Augment)> {
        let i = egui::DragAndDrop::payload::<BagDrag>(ctx)
            .map(|d| d.0)
            .or(self.picked)?;
        sheet.bag.get(i).map(|a| (i, *a))
    }

    /// Call once a frame before drawing: forgets places in a bag that
    /// has changed.
    pub fn sync(&mut self, sheet: &Sheet) {
        if self.seen != sheet.bag {
            self.seen = sheet.bag.clone();
            self.picked = None;
            self.replace = None;
            self.trade = None;
        }
    }

    /// Does the augment in hand fit `d` (an owned power lights up then)?
    pub fn fits_in_hand(&self, ctx: &egui::Context, sheet: &Sheet, d: &PowerDef) -> bool {
        self.in_hand(ctx, sheet)
            .and_then(|(_, a)| Effect::of(a))
            .is_some_and(|e| sheet.has(d.id) && augments::fits(e, d))
    }

    /// Power `d`'s sockets: what is set, what it does there, and where
    /// the augment in hand may go.
    pub fn sockets(
        &mut self,
        ui: &mut egui::Ui,
        sheet: &Sheet,
        d: &PowerDef,
        out: &mut Vec<AugmentAction>,
    ) {
        let n = augments::sockets_of(d);
        if n == 0 {
            return;
        }
        let owned = sheet.has(d.id);
        let level = level_of(sheet.lifetime);
        let t = crate::uikit::now(ui.ctx());
        let pulse = 0.65 + 0.35 * (t * 5.0).sin();
        let hand = self.in_hand(ui.ctx(), sheet);
        let set = sheet.sockets(d.id);
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            for (slot, &a) in set.iter().enumerate().take(n) {
                let can = owned
                    && hand.is_some_and(|(_, h)| {
                        augments::can_set_augment(sheet, h, d.id, slot as u8).is_ok()
                    });
                let (r, resp) = ui.allocate_exact_size(vec2(CHIP, CHIP), egui::Sense::click());
                let glow = if can { pulse } else { 0.0 };
                let painter = ui.painter();
                if a.is_none() {
                    empty_socket(painter, r, glow);
                } else {
                    chip(painter, r, a, t, glow);
                }
                if !owned {
                    painter.rect_filled(r, r.width() * 0.2, Color32::from_black_alpha(110));
                }
                // Dropped here, or clicked with one picked.
                let dropped = resp.dnd_release_payload::<BagDrag>().map(|b| b.0);
                let asked = dropped.or(if resp.clicked() { self.picked } else { None });
                if let (Some(bag), true) = (asked, can) {
                    if a.is_none() {
                        out.push(AugmentAction::Set {
                            bag: bag as u8,
                            power: d.id,
                            slot: slot as u8,
                        });
                        self.picked = None;
                    } else {
                        self.replace = Some((bag, d.id, slot as u8));
                    }
                }
                // What it does here, or would.
                let words = if let (true, Some((_, h))) = (can, hand) {
                    let does = augments::describe(d, h, level).unwrap_or_default();
                    let lost = if a.is_none() {
                        String::new()
                    } else {
                        format!("\nReplaces {} (lost for good)", title(a))
                    };
                    (title(h), format!("{does}{lost}"), LIT)
                } else if a.is_none() {
                    (
                        "Empty socket".to_string(),
                        if owned {
                            "drag an augment here".to_string()
                        } else {
                            "yours once you own the power".to_string()
                        },
                        Color32::from_gray(150),
                    )
                } else {
                    (
                        title(a),
                        augments::describe(d, a, level).unwrap_or_default(),
                        grade_colour(Grade::of(a)),
                    )
                };
                ui.vertical(|ui| {
                    ui.set_width(182.0);
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.label(
                        egui::RichText::new(words.0)
                            .size(13.0)
                            .strong()
                            .color(words.2),
                    );
                    ui.label(
                        egui::RichText::new(words.1)
                            .size(11.5)
                            .color(Color32::from_gray(205)),
                    );
                });
                if !a.is_none() {
                    resp.on_hover_ui(|ui| about(ui, a, Some(d), level));
                }
            }
        });
        // A set that destroys one: asked once more.
        if let Some((bag, power, slot)) = self.replace.filter(|r| r.1 == d.id) {
            let old = set[slot as usize];
            let new = sheet.bag.get(bag).copied().unwrap_or(Augment::NONE);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "Replace {} with {}? The old one is lost for good.",
                        title(old),
                        title(new)
                    ))
                    .size(13.0)
                    .color(Color32::from_rgb(255, 190, 150)),
                );
                let yes = egui::Button::new(egui::RichText::new("Replace").strong().color(INK))
                    .fill(grade_colour(Grade::of(new)));
                if ui.add(yes).clicked() {
                    out.push(AugmentAction::Set {
                        bag: bag as u8,
                        power,
                        slot,
                    });
                    self.replace = None;
                    self.picked = None;
                }
                if ui.button("Keep the old one").clicked() {
                    self.replace = None;
                }
            });
        }
        // Every effect that fits this power, as small icons.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            ui.label(
                egui::RichText::new("Takes")
                    .size(11.5)
                    .color(Color32::from_gray(150)),
            );
            for e in augments::fitting(d) {
                let (r, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), egui::Sense::hover());
                let has = set
                    .iter()
                    .any(|s| !s.is_none() && Effect::of(*s) == Some(e));
                let col = if has {
                    effect_colour(e)
                } else {
                    effect_colour(e).gamma_multiply(0.55)
                };
                glyph(ui.painter(), r.center(), 7.0, e, col);
                resp.on_hover_text(format!("{}: {}", e.name(), e.blurb()));
            }
        });
    }

    /// The bag: every augment found and not yet set, to drag onto a
    /// socket or trade in.
    pub fn bag(&mut self, ui: &mut egui::Ui, sheet: &Sheet, out: &mut Vec<AugmentAction>) {
        let t = crate::uikit::now(ui.ctx());
        let level = level_of(sheet.lifetime);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Augments")
                    .size(17.0)
                    .strong()
                    .color(Color32::from_rgb(255, 214, 90)),
            );
            ui.label(
                egui::RichText::new(format!("{} of {}", sheet.bag.len(), MAX_BAG))
                    .size(13.0)
                    .color(Color32::from_gray(190)),
            );
        });
        ui.label(
            egui::RichText::new(
                "Villains drop them. Drag one onto a socket of a power it fits: it stays there for good.",
            )
            .size(12.0)
            .color(Color32::from_gray(170)),
        );
        let per_row = BAG_COLUMNS;
        let gap = BAG_GAP;
        let size = vec2(
            BAG_WIDTH,
            MAX_BAG.div_ceil(per_row) as f32 * (CHIP + gap) - gap,
        );
        let (area, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let dragging = egui::DragAndDrop::payload::<BagDrag>(ui.ctx()).map(|d| d.0);
        for i in 0..MAX_BAG {
            let r = Rect::from_min_size(
                area.min + vec2((i % per_row) as f32, (i / per_row) as f32) * (CHIP + gap),
                vec2(CHIP, CHIP),
            );
            let Some(&a) = sheet.bag.get(i) else {
                let round = CHIP * 0.2;
                ui.painter()
                    .rect_filled(r, round, Color32::from_rgb(22, 24, 38));
                ui.painter().rect_stroke(
                    r,
                    round,
                    Stroke::new(1.0_f32, Color32::from_gray(60)),
                    egui::StrokeKind::Inside,
                );
                continue;
            };
            let id = ui.id().with(("bag chip", i));
            let resp = ui.interact(r, id, egui::Sense::click_and_drag());
            if resp.drag_started() {
                egui::DragAndDrop::set_payload(ui.ctx(), BagDrag(i));
                self.picked = Some(i);
                self.replace = None;
                self.trade = None;
            }
            if resp.clicked() {
                self.picked = if self.picked == Some(i) {
                    None
                } else {
                    Some(i)
                };
                self.replace = None;
                self.trade = None;
            }
            let held = dragging == Some(i);
            let glow = if self.picked == Some(i) { 1.0 } else { 0.0 };
            let painter = ui.painter();
            if held {
                // Its place stays, dim, while it is carried.
                chip(painter, r, a, t, 0.0);
                painter.rect_filled(r, CHIP * 0.2, Color32::from_black_alpha(150));
            } else {
                let lift = if resp.hovered() { -2.0 } else { 0.0 };
                chip(painter, r.translate(vec2(0.0, lift)), a, t, glow);
            }
            if is_better(sheet, a) {
                // An arrow up: this would make a power of yours better.
                let c = r.left_top() + vec2(8.0, 9.0);
                painter.circle_filled(c, 7.0, INK);
                painter.add(Shape::convex_polygon(
                    vec![c + vec2(-4.5, 1.5), c + vec2(4.5, 1.5), c + vec2(0.0, -4.5)],
                    BETTER,
                    Stroke::NONE,
                ));
                painter.rect_filled(
                    Rect::from_center_size(c + vec2(0.0, 3.0), vec2(3.0, 3.5)),
                    0.0,
                    BETTER,
                );
            }
            if !held {
                resp.on_hover_ui(|ui| about(ui, a, None, level));
            }
        }
        // The one carried, under the pointer.
        if let (Some(i), Some(at)) = (dragging, ui.ctx().pointer_interact_pos()) {
            if let Some(&a) = sheet.bag.get(i) {
                let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("bag carried"));
                let painter = ui.ctx().layer_painter(layer);
                chip(
                    &painter,
                    Rect::from_center_size(at, vec2(CHIP, CHIP) * 1.1),
                    a,
                    t,
                    1.0,
                );
            }
        }
        // The one picked: what it is, where it goes, and the trade.
        let Some((i, a)) = self.picked.and_then(|i| sheet.bag.get(i).map(|a| (i, *a))) else {
            return;
        };
        let Some(e) = Effect::of(a) else {
            return;
        };
        let grade = Grade::of(a);
        ui.add_space(4.0);
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(format!("{} {}", grade.name(), title(a)))
                    .size(14.0)
                    .strong()
                    .color(grade_colour(grade)),
            );
            ui.label(
                egui::RichText::new(format!("{}.", e.blurb()))
                    .size(13.0)
                    .color(Color32::from_gray(215)),
            );
            let fits: Vec<&str> = owned(sheet)
                .filter(|d| augments::fits(e, d))
                .map(|d| d.name)
                .collect();
            let text = if fits.is_empty() {
                "Fits none of your powers yet.".to_string()
            } else {
                format!("Fits {}.", fits.join(", "))
            };
            ui.label(egui::RichText::new(text).size(12.5).color(LIT));
            let karma = augments::trade_value(a);
            let sure = self.trade == Some(i);
            let label = if sure {
                format!("Really trade it in? (+{karma} karma)")
            } else {
                format!("Trade in · +{karma} karma")
            };
            if ui.button(label).clicked() {
                // Better than common: asked once more.
                if grade == Grade::Common || sure {
                    out.push(AugmentAction::Trade { bag: i as u8 });
                    self.trade = None;
                    self.picked = None;
                } else {
                    self.trade = Some(i);
                }
            }
        });
    }
}

/// The hover card of an augment: what it is, and what it does on `on`.
fn about(ui: &mut egui::Ui, a: Augment, on: Option<&PowerDef>, level: u8) {
    let Some(e) = Effect::of(a) else {
        return;
    };
    let grade = Grade::of(a);
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(CHIP, CHIP), egui::Sense::hover());
        chip(ui.painter(), r, a, crate::uikit::now(ui.ctx()), 0.0);
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(title(a))
                    .size(15.0)
                    .strong()
                    .color(grade_colour(grade)),
            );
            ui.label(
                egui::RichText::new(format!("{}: {}", grade.name(), e.blurb()))
                    .size(12.5)
                    .color(Color32::from_gray(215)),
            );
            if let Some(d) = on {
                if let Some(does) = augments::describe(d, a, level) {
                    ui.label(
                        egui::RichText::new(format!("On {}: {does}", d.name))
                            .size(12.5)
                            .color(LIT),
                    );
                }
            }
            ui.label(
                egui::RichText::new(format!("Trades in for {} karma", augments::trade_value(a)))
                    .size(11.5)
                    .color(Color32::from_gray(160)),
            );
        });
    });
}

/// How many sockets a chip row needs at most (for layouts).
pub const MOST_SOCKETS: usize = SOCKETS;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_augment_is_better_where_a_socket_is_free_or_holds_less() {
        let mut sheet = hw_sim::progress::starter();
        // Strike is every hero's: damage fits it.
        let strike = PowerId::FREE[0];
        let d = powers::def_of(strike);
        assert!(augments::fits(Effect::Damage, d));
        let weak = augments::make(Effect::Damage, Grade::Common, 1);
        let strong = augments::make(Effect::Damage, Grade::Relic, 20);
        assert!(is_better(&sheet, weak));
        sheet.bag.push(strong);
        augments::set(&mut sheet, 0, strike, 0).unwrap();
        // The same effect, weaker: not better anywhere it is already set
        // stronger (other powers may still have room).
        let only_strike = |s: &Sheet, a| {
            let e = Effect::of(a).unwrap();
            let set = s.sockets(strike);
            set.iter()
                .find(|x| Effect::of(**x) == Some(e) && !x.is_none())
                .is_some_and(|x| x.percent < a.percent)
        };
        assert!(!only_strike(&sheet, weak));
        assert!(title(strong).starts_with("Damage +"));
    }
}
