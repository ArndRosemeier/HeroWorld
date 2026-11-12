//! Comic speech bubbles: words people say, in a golden-age comic balloon
//! over their head with a tail pointing at them.
//!
//! One [`SpeechBubbles`] (owned by `FightView` as `speech`) keeps every
//! bubble. Anything that makes someone talk adds a [`Speech`]:
//!
//! ```text
//! fight.speech.add(Speech::say(feet, "Lovely day!", Voice::Folk));
//! fight.speech.add(Speech::shout(feet, "Stop, thief!", Voice::Folk).from(id));
//! fight.bubble(&said); // a chat line: a shout when it carries SHOUT_RANGE
//! ```
//!
//! `at` is where the speaker stands; the bubble sits over their head.
//! `from(id)` makes it follow a networked speaker as they move. The text
//! wraps to the bubble's width and the bubble always grows to hold it. Near
//! bubbles win the spot over the head; others step up or aside so they do
//! not cover each other. A speaker off screen or behind the camera, but near
//! enough to be heard, gets a smaller bubble at the screen's edge with its
//! tail pointing their way.

use crate::chat::{Said, Voice, SAY_RANGE, SHOUT_RANGE};
use engine::ui::egui;
use glam::{Mat4, Vec3};
use hw_core::NetId;

/// Metres above a speaker's feet the tail points at (over the head).
pub const HEAD: f32 = 2.05;
/// Bubbles shown at once (the nearest).
const MAX_SHOWN: usize = 8;
/// Widest a line of text gets before it wraps (points, near the camera).
const WRAP: f32 = 230.0;
/// Widest at the screen's edge.
const EDGE_WRAP: f32 = 170.0;
/// Room between the text and the balloon's edge.
const PAD: egui::Vec2 = egui::vec2(12.0, 8.0);
/// Length of the tail from the balloon to the speaker.
const TAIL: f32 = 22.0;
/// Gap kept between the screen's edge and an edge bubble.
const EDGE_GAP: f32 = 12.0;
/// Room left at the top for the compass.
const TOP_GAP: f32 = 46.0;
/// A new line this close (metres) to a bubble's speaker replaces it.
const SAME_SPEAKER: f32 = 1.2;
/// Outline of the balloon.
const INK: egui::Color32 = egui::Color32::from_rgb(22, 18, 26);
/// Paper colour of a spoken balloon.
const PAPER: egui::Color32 = egui::Color32::from_rgb(255, 251, 234);
/// A shout's burst: yellow paper, red letters.
const SHOUT_PAPER: egui::Color32 = egui::Color32::from_rgb(255, 234, 120);
const SHOUT_INK: egui::Color32 = egui::Color32::from_rgb(170, 24, 20);

/// Something someone says, for [`SpeechBubbles::add`].
#[derive(Clone, Debug, PartialEq)]
pub struct Speech {
    /// Where the speaker stands (feet).
    pub at: Vec3,
    pub text: String,
    pub voice: Voice,
    /// Shouted: a spiky burst, capital letters, heard further.
    pub shout: bool,
    /// The networked speaker to follow as they move.
    pub from: Option<NetId>,
}

impl Speech {
    pub fn say(at: Vec3, text: impl Into<String>, voice: Voice) -> Self {
        Self {
            at,
            text: text.into(),
            voice,
            shout: false,
            from: None,
        }
    }

    pub fn shout(at: Vec3, text: impl Into<String>, voice: Voice) -> Self {
        Self {
            shout: true,
            ..Self::say(at, text, voice)
        }
    }

    /// A chat line as a bubble: shouted when it carries as far as a shout.
    pub fn of(said: &Said) -> Self {
        let mut s = Self::say(said.at, said.text.clone(), said.voice);
        s.shout = said.range >= SHOUT_RANGE;
        s
    }

    /// Follows `id` while it is in the world's state.
    pub fn from(mut self, id: NetId) -> Self {
        self.from = Some(id);
        self
    }

    /// How far away the bubble still shows (as far as it is heard).
    fn range(&self) -> f32 {
        if self.shout {
            SHOUT_RANGE
        } else {
            SAY_RANGE
        }
    }

    /// Seconds a line stays up: long enough to read it.
    fn life(&self) -> f32 {
        (2.4 + 0.06 * self.text.chars().count() as f32).clamp(3.5, 9.0)
    }
}

struct Bubble {
    speech: Speech,
    age: f32,
    life: f32,
}

/// Every speech bubble in the world.
#[derive(Default)]
pub struct SpeechBubbles {
    bubbles: Vec<Bubble>,
}

impl SpeechBubbles {
    /// Shows `speech` over the speaker. A line from someone already talking
    /// replaces their bubble.
    pub fn add(&mut self, speech: Speech) {
        if speech.text.trim().is_empty() {
            return;
        }
        self.bubbles.retain(|b| {
            let same = match (b.speech.from, speech.from) {
                (Some(a), Some(c)) => a == c,
                _ => b.speech.at.distance(speech.at) < SAME_SPEAKER,
            };
            !same
        });
        let life = speech.life();
        self.bubbles.push(Bubble {
            speech,
            age: 0.0,
            life,
        });
    }

    pub fn update(&mut self, dt: f32) {
        for b in &mut self.bubbles {
            b.age += dt;
        }
        self.bubbles.retain(|b| b.age < b.life);
    }

    /// How many bubbles are up.
    pub fn len(&self) -> usize {
        self.bubbles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bubbles.is_empty()
    }

    pub fn clear(&mut self) {
        self.bubbles.clear();
    }

    /// Draws the bubbles. `where_is` gives a followed speaker's feet now.
    pub fn draw(
        &self,
        painter: &egui::Painter,
        view_proj: Mat4,
        where_is: &dyn Fn(NetId) -> Option<Vec3>,
    ) {
        let screen = painter.clip_rect();
        let ctx = painter.ctx();
        let talk_font = family(ctx, "hw-ui");
        let shout_font = family(ctx, "hw-logo");

        // Where each bubble's speaker is on screen, nearest first.
        let mut shown: Vec<(&Bubble, Spot, f32)> = Vec::new();
        for b in &self.bubbles {
            let feet = b.speech.from.and_then(where_is).unwrap_or(b.speech.at);
            let head = feet + Vec3::Y * HEAD;
            let c = view_proj * head.extend(1.0);
            let depth = c.w;
            let dist = depth.abs().max(0.1);
            if dist > b.speech.range() {
                continue;
            }
            shown.push((b, spot(c, screen), dist));
        }
        shown.sort_by(|a, b| a.2.total_cmp(&b.2));
        shown.truncate(MAX_SHOWN);

        let mut taken: Vec<egui::Rect> = Vec::new();
        let mut drawn = Vec::new();
        for (b, at, dist) in shown {
            let s = &b.speech;
            let edge = matches!(at, Spot::Edge(..));
            // Smaller when far, but never too small to read.
            let near = (1.0 - (dist - 8.0) / 32.0).clamp(0.0, 1.0);
            let size = if edge { 14.0 } else { 14.5 + 3.5 * near };
            let size = if s.shout { size + 3.0 } else { size };
            let wrap = if edge {
                EDGE_WRAP
            } else {
                WRAP * (0.8 + 0.2 * near)
            };
            let text = if s.shout {
                s.text.to_uppercase()
            } else {
                s.text.clone()
            };
            let ink = if s.shout { SHOUT_INK } else { INK };
            let font = egui::FontId::new(
                size,
                if s.shout {
                    shout_font.clone()
                } else {
                    talk_font.clone()
                },
            );
            // Centred lines, like a letterer's.
            let mut job = egui::text::LayoutJob::simple(text, font, ink, wrap);
            job.halign = egui::Align::Center;
            let galley = painter.fonts(|f| f.layout_job(job));
            let body = balloon_size(galley.size(), s.shout);
            let (tip, rect) = place(at, body, screen, &taken);
            taken.push(rect.expand(3.0));
            // Fades in quickly, out near the end, and when far.
            let fade_in = (b.age / 0.15).min(1.0);
            let fade_out = ((b.life - b.age) / 0.5).clamp(0.0, 1.0);
            let far = ((s.range() - dist) / (s.range() * 0.2)).clamp(0.0, 1.0);
            let alpha = fade_in * fade_out * far * if edge { 0.92 } else { 1.0 };
            let pop = 0.82 + 0.18 * ease_out_back(fade_in);
            drawn.push((rect, tip, galley, s.shout, s.voice, alpha, pop, b.age));
        }
        // Far ones under near ones.
        for (rect, tip, galley, shout, voice, alpha, pop, age) in drawn.into_iter().rev() {
            let rect = egui::Rect::from_center_size(rect.center(), rect.size() * pop);
            let ring = outline(rect, tip, shout, age);
            let paper = if shout { SHOUT_PAPER } else { PAPER };
            paint_balloon(painter, &ring, rect.center(), paper, voice, alpha);
            let text_at = rect.center() - galley.rect.center().to_vec2();
            if alpha > 0.05 {
                painter.galley(text_at, galley, INK.gamma_multiply(alpha));
            }
        }
    }
}

/// Where a speaker's head is on screen.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Spot {
    /// In view: the point the tail touches.
    Head(egui::Pos2),
    /// Off screen or behind the camera: the direction they are in, from
    /// the screen's centre (y down).
    Edge(egui::Vec2),
}

/// Where the clip-space point `c` lands on `screen`.
fn spot(c: glam::Vec4, screen: egui::Rect) -> Spot {
    let to_screen = |n: glam::Vec2| {
        egui::pos2(
            screen.left() + (n.x * 0.5 + 0.5) * screen.width(),
            screen.top() + (0.5 - n.y * 0.5) * screen.height(),
        )
    };
    if c.w > 0.05 {
        let n = c.truncate().truncate() / c.w;
        let p = to_screen(n);
        if usable(screen).contains(p) {
            return Spot::Head(p);
        }
        let d = p - screen.center();
        return Spot::Edge(if d.length() > 1.0 {
            d.normalized()
        } else {
            egui::vec2(0.0, 1.0)
        });
    }
    // Behind the camera: at the side they are on, a little low (behind
    // reads as "down", like on a compass). The projection is mirrored
    // behind the eye.
    let side = if -c.x >= 0.0 { 1.0 } else { -1.0 };
    Spot::Edge(egui::vec2(side, 0.25).normalized())
}

/// Where bubbles may go: the screen less a margin, and the compass.
fn usable(screen: egui::Rect) -> egui::Rect {
    let mut r = screen.shrink(EDGE_GAP);
    r.min.y = screen.min.y + TOP_GAP;
    r
}

/// The balloon's size for text of `text` size.
fn balloon_size(text: egui::Vec2, shout: bool) -> egui::Vec2 {
    // The rounded balloon is a superellipse through the text box's corners
    // and a bit beyond; a burst's spikes stand outside that.
    let grow = if shout { 1.32 } else { 1.2 };
    (text + PAD * 2.0) * grow
}

/// Where the balloon of `size` goes for a speaker at `at`, clear of
/// `taken`: the tail's tip and the balloon's box.
fn place(
    at: Spot,
    size: egui::Vec2,
    screen: egui::Rect,
    taken: &[egui::Rect],
) -> (egui::Pos2, egui::Rect) {
    let inside = usable(screen);
    let fit = |r: egui::Rect| {
        // Keep the whole balloon on screen.
        let mut d = egui::Vec2::ZERO;
        if r.left() < inside.left() {
            d.x = inside.left() - r.left();
        } else if r.right() > inside.right() {
            d.x = inside.right() - r.right();
        }
        if r.top() < inside.top() {
            d.y = inside.top() - r.top();
        } else if r.bottom() > inside.bottom() {
            d.y = inside.bottom() - r.bottom();
        }
        r.translate(d)
    };
    let (tip, home, steps): (egui::Pos2, egui::Rect, Vec<egui::Vec2>) = match at {
        Spot::Head(p) => {
            // Over the head, a little to the right like a comic panel.
            let c = p + egui::vec2(size.x * 0.12, -TAIL - size.y * 0.5);
            let up = size.y + 6.0;
            let side = size.x * 0.55 + 6.0;
            let steps = vec![
                egui::vec2(0.0, -up),
                egui::vec2(-side * 1.2, 0.0),
                egui::vec2(side, 0.0),
                egui::vec2(0.0, -up * 2.0),
                egui::vec2(-side * 1.2, -up),
                egui::vec2(side, -up),
                egui::vec2(0.0, -up * 3.0),
            ];
            (p, egui::Rect::from_center_size(c, size), steps)
        }
        Spot::Edge(dir) => {
            // The tip on the screen's edge toward the speaker; the balloon
            // just inside it.
            let half = inside.size() * 0.5;
            let k = (half.x / dir.x.abs().max(1e-4)).min(half.y / dir.y.abs().max(1e-4));
            let tip = inside.center() + dir * k;
            let reach = dir.x.abs() * size.x * 0.5 + dir.y.abs() * size.y * 0.5;
            let c = tip - dir * (reach + TAIL * 0.8);
            // Slide along the edge to make room.
            let along = egui::vec2(-dir.y, dir.x);
            let step = along.x.abs() * (size.x + 6.0) + along.y.abs() * (size.y + 6.0);
            let steps = vec![
                along * step,
                -along * step,
                along * step * 2.0,
                -along * step * 2.0,
            ];
            (tip, egui::Rect::from_center_size(c, size), steps)
        }
    };
    let home = fit(home);
    let free = |r: &egui::Rect| !taken.iter().any(|t| t.intersects(*r));
    if free(&home) {
        return (tip, home);
    }
    for s in steps {
        let r = fit(home.translate(s));
        if free(&r) {
            return (tip, r);
        }
    }
    (tip, home)
}

/// The balloon's outline around `rect` with a tail to `tip`: a rounded
/// superellipse, or a spiky burst for a shout. Every point is seen from the
/// centre (the shape is star-shaped), so it fills as a fan.
fn outline(rect: egui::Rect, tip: egui::Pos2, shout: bool, age: f32) -> Vec<egui::Pos2> {
    let c = rect.center();
    let a = rect.width() * 0.5;
    let b = rect.height() * 0.5;
    let n = if shout { 36 } else { 64 };
    // The superellipse |x/a|^4 + |y/b|^4 = 1: round like a balloon, but
    // its corners hold the text box.
    let ring = |t: f32| {
        let (s, co) = t.sin_cos();
        let x = a * co.signum() * co.abs().sqrt();
        let y = b * s.signum() * s.abs().sqrt();
        egui::vec2(x, y)
    };
    let to_tip = tip - c;
    let tau = std::f32::consts::TAU;
    // The ring's parameter where the line to the tip leaves it (the
    // parameter is not the polar angle on a stretched balloon).
    let phi = to_tip.y.atan2(to_tip.x);
    let (sp, cp) = phi.sin_cos();
    let tail_at = (sp.signum() * a * a * sp * sp)
        .atan2(cp.signum() * b * b * cp * cp)
        .rem_euclid(tau);
    let tail_out = to_tip.length() > a.min(b) * 0.9;
    // Where the tail leaves the balloon, and its root: a narrow wedge (in
    // points, so a wide balloon does not get a wide tail), along the ring.
    let k = if shout { 0.9 } else { 0.97 };
    let exit = c + ring(tail_at) * k;
    let along = (ring(tail_at + 0.01) - ring(tail_at - 0.01)).normalized();
    let half = (0.3 * a.min(b)).min(9.0);
    let mut pts = Vec::with_capacity(n + 3);
    for i in 0..n {
        let t = i as f32 / n as f32 * tau;
        let mut v = ring(t);
        if tail_out && (c + v).distance(exit) < half * 1.6 {
            continue;
        }
        if shout {
            // Spikes, jittering a little so the burst looks alive.
            let spike = if i % 2 == 0 {
                1.18 + 0.05 * (age * 9.0 + i as f32 * 1.7).sin()
            } else {
                0.9
            };
            v *= spike;
        }
        pts.push((t, c + v));
    }
    if tail_out {
        // Stop a hair short of the head.
        let tip = tip - to_tip.normalized() * 4.0;
        // Splice the tail in where the ring passes it (the ring is closed,
        // so the start and the end are the same place).
        let at = pts
            .iter()
            .position(|(t, _)| *t > tail_at)
            .unwrap_or(pts.len());
        pts.splice(
            at..at,
            [
                (0.0, exit - along * half),
                (0.0, tip),
                (0.0, exit + along * half),
            ],
        );
    }
    pts.into_iter().map(|(_, p)| p).collect()
}

/// Fills `ring` (seen whole from `centre`) with a drop shadow and an ink
/// outline, with a thin stripe in the speaker's voice colour inside it.
fn paint_balloon(
    painter: &egui::Painter,
    ring: &[egui::Pos2],
    centre: egui::Pos2,
    paper: egui::Color32,
    voice: Voice,
    alpha: f32,
) {
    if alpha <= 0.01 || ring.len() < 3 {
        return;
    }
    let fan = |offset: egui::Vec2, colour: egui::Color32| {
        let mut m = egui::Mesh::default();
        m.colored_vertex(centre + offset, colour);
        for p in ring {
            m.colored_vertex(*p + offset, colour);
        }
        let k = ring.len() as u32;
        for i in 0..k {
            m.add_triangle(0, 1 + i, 1 + (i + 1) % k);
        }
        egui::Shape::mesh(m)
    };
    painter.add(fan(
        egui::vec2(4.0, 4.0),
        egui::Color32::from_black_alpha((110.0 * alpha) as u8),
    ));
    painter.add(fan(egui::Vec2::ZERO, paper.gamma_multiply(alpha)));
    // The voice stripe just inside the outline.
    let inner: Vec<egui::Pos2> = ring.iter().map(|p| centre + (*p - centre) * 0.94).collect();
    painter.add(egui::Shape::closed_line(
        inner,
        egui::Stroke::new(2.0_f32, voice.colour().gamma_multiply(alpha * 0.85)),
    ));
    painter.add(egui::Shape::closed_line(
        ring.to_vec(),
        egui::Stroke::new(2.5_f32, INK.gamma_multiply(alpha)),
    ));
}

/// The named font family when it is installed (the title screen installs
/// them), else the plain one.
fn family(ctx: &egui::Context, name: &str) -> egui::FontFamily {
    let f = egui::FontFamily::Name(name.into());
    if ctx.fonts(|fonts| fonts.families().contains(&f)) {
        f
    } else {
        egui::FontFamily::Proportional
    }
}

fn ease_out_back(x: f32) -> f32 {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (x - 1.0).powi(3) + c1 * (x - 1.0).powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen() -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0))
    }

    /// Is `p` inside the closed polygon `ring`?
    fn inside(ring: &[egui::Pos2], p: egui::Pos2) -> bool {
        let mut hit = false;
        let mut j = ring.len() - 1;
        for i in 0..ring.len() {
            let (a, b) = (ring[i], ring[j]);
            if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
                hit = !hit;
            }
            j = i;
        }
        hit
    }

    #[test]
    fn long_lines_wrap_and_the_balloon_holds_every_word() {
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::background());
            let long = "Gee whiz, mister, you saved my whole family and the cat too! \
                        I'll tell everyone in Rocket City about you, cross my heart!";
            let font = egui::FontId::proportional(18.0);
            let galley = painter.layout(long.into(), font, INK, WRAP);
            assert!(galley.rows.len() > 1, "a long line wraps");
            assert!(
                galley.size().x <= WRAP + 1.0,
                "no wider than the wrap width"
            );
            for shout in [false, true] {
                let size = balloon_size(galley.size(), shout);
                let rect = egui::Rect::from_center_size(egui::pos2(600.0, 300.0), size);
                let tip = egui::pos2(560.0, 300.0 + size.y * 0.5 + TAIL);
                let ring = outline(rect, tip, shout, 0.3);
                let text = egui::Rect::from_center_size(rect.center(), galley.size());
                for corner in [
                    text.left_top(),
                    text.right_top(),
                    text.left_bottom(),
                    text.right_bottom(),
                ] {
                    assert!(
                        inside(&ring, corner),
                        "text corner {corner:?} outside (shout {shout})"
                    );
                }
                assert!(ring.contains(&(tip - (tip - rect.center()).normalized() * 4.0)));
            }
        });
    }

    #[test]
    fn two_speakers_side_by_side_do_not_cover_each_other() {
        let size = egui::vec2(200.0, 70.0);
        let mut taken = Vec::new();
        for x in [600.0, 640.0, 620.0] {
            let (_, r) = place(Spot::Head(egui::pos2(x, 400.0)), size, screen(), &taken);
            assert!(taken.iter().all(|t: &egui::Rect| !t.intersects(r)));
            assert!(screen().contains_rect(r));
            taken.push(r);
        }
    }

    #[test]
    fn a_speaker_behind_the_camera_gets_a_bubble_at_the_edge_pointing_their_way() {
        // Behind and to the right: the edge bubble sits low on the right.
        let s = spot(glam::Vec4::new(-0.8, 0.0, 0.5, -2.0), screen());
        let Spot::Edge(dir) = s else { panic!("{s:?}") };
        assert!(dir.x > 0.8 && dir.y > 0.0, "right and low: {dir:?}");
        let (tip, r) = place(s, egui::vec2(160.0, 60.0), screen(), &[]);
        assert!(screen().contains_rect(r));
        assert!(tip.x > r.right(), "the tail points out to the right");
        // In front and on screen: over the head.
        let s = spot(glam::Vec4::new(0.0, 0.0, 0.5, 2.0), screen());
        assert_eq!(s, Spot::Head(screen().center()));
    }

    #[test]
    fn a_new_line_from_the_same_speaker_replaces_the_old_bubble() {
        let mut b = SpeechBubbles::default();
        b.add(Speech::say(Vec3::ZERO, "Hello", Voice::Folk));
        b.add(Speech::say(Vec3::new(0.5, 0.0, 0.0), "Again", Voice::Folk));
        b.add(Speech::shout(
            Vec3::new(9.0, 0.0, 0.0),
            "Stop, thief!",
            Voice::Folk,
        ));
        assert_eq!(b.len(), 2);
        b.update(20.0);
        assert!(b.is_empty(), "lines go away");
    }
}
