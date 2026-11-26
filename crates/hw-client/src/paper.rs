//! The Daily Comet on screen: a front page that spins in when the paper
//! prints something about our hero (the film newsreel's spinning paper),
//! and the scrapbook of press cuttings (I) with how well the town knows
//! the hero.
//!
//! A page is drawn from an [`Edition`] with egui shapes, turned and scaled
//! as one piece: newsprint, the masthead, the kicker in red, the headline,
//! a halftone picture with an ink silhouette, and the line under it.

use crate::googie;
use crate::headlines::{Edition, Picture};
use crate::uikit::{self, with_alpha};
use egui::epaint::TextShape;
use egui::{pos2, vec2, Color32, FontFamily, FontId, Pos2, Rect, Shape, Stroke, Vec2};
use engine::ui::egui;
use hw_sim::fame::Fame;
use std::collections::VecDeque;

/// A page's size at scale 1 (points).
pub const PAGE: Vec2 = vec2(300.0, 400.0);

const NEWSPRINT: Color32 = Color32::from_rgb(238, 229, 205);
const NEWSPRINT_DARK: Color32 = Color32::from_rgb(214, 203, 176);
const INK: Color32 = Color32::from_rgb(28, 26, 30);
const RED_INK: Color32 = Color32::from_rgb(184, 38, 34);
const HALFTONE: Color32 = Color32::from_rgb(196, 188, 170);

/// Seconds a front page spins in, stays and goes.
const SPIN: f32 = 0.75;
const HOLD: f32 = 5.5;
const LEAVE: f32 = 0.45;
/// At least this long between two front pages.
const GAP: f32 = 4.0;
/// The most front pages waiting (older ones are only in the scrapbook).
const WAITING: usize = 2;

/// Where a page is drawn: its middle, its scale, its turn (radians) and
/// how solid it is.
#[derive(Clone, Copy, Debug)]
pub struct Lay {
    pub centre: Pos2,
    pub scale: f32,
    pub angle: f32,
    pub alpha: f32,
}

impl Lay {
    /// A point of the page (from its middle, y down) on screen.
    fn at(&self, x: f32, y: f32) -> Pos2 {
        let (s, c) = self.angle.sin_cos();
        let (x, y) = (x * self.scale, y * self.scale);
        self.centre + vec2(x * c - y * s, x * s + y * c)
    }

    fn quad(&self, r: Rect) -> Vec<Pos2> {
        vec![
            self.at(r.min.x, r.min.y),
            self.at(r.max.x, r.min.y),
            self.at(r.max.x, r.max.y),
            self.at(r.min.x, r.max.y),
        ]
    }

    fn fill(&self, painter: &egui::Painter, r: Rect, col: Color32) {
        painter.add(Shape::convex_polygon(
            self.quad(r),
            with_alpha(col, self.alpha),
            Stroke::NONE,
        ));
    }
}

fn family(ctx: &egui::Context, name: &str) -> FontFamily {
    let f = FontFamily::Name(name.into());
    if ctx.fonts(|fonts| fonts.families().contains(&f)) {
        f
    } else {
        FontFamily::Proportional
    }
}

/// Text centred on (`x`, top `y`) of the page, wrapped at `width`.
/// Returns its height on the page (at scale 1).
#[allow(clippy::too_many_arguments)]
fn text(
    painter: &egui::Painter,
    lay: &Lay,
    x: f32,
    y: f32,
    width: f32,
    words: &str,
    font: FontId,
    col: Color32,
) -> f32 {
    let size = font.size;
    let galley = centred(painter, words, font, lay.scale, width, col);
    let h = galley.rect.height() / lay.scale.max(0.01);
    let _ = size;
    painter.add(
        TextShape::new(lay.at(x, y), galley, with_alpha(col, lay.alpha))
            .with_angle(lay.angle)
            .with_override_text_color(with_alpha(col, lay.alpha)),
    );
    h
}

fn centred(
    painter: &egui::Painter,
    words: &str,
    font: FontId,
    scale: f32,
    width: f32,
    col: Color32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple(
        words.to_string(),
        FontId::new((font.size * scale).max(1.0), font.family),
        col,
        width * scale,
    );
    job.halign = egui::Align::Center;
    painter.ctx().fonts(|f| f.layout_job(job))
}

/// The headline's size: the biggest that fits `width` by `height`.
fn headline_size(painter: &egui::Painter, head: &str, family: &FontFamily, width: f32) -> f32 {
    for size in [36.0, 32.0, 28.0, 25.0, 22.0, 19.0] {
        let g = centred(
            painter,
            head,
            FontId::new(size, family.clone()),
            1.0,
            width,
            INK,
        );
        if g.rect.height() <= 104.0 && g.rect.width() <= width + 1.0 {
            return size;
        }
    }
    17.0
}

/// A piece of a silhouette, in a box from -1 to 1 (y down).
enum Prim {
    Poly(Vec<[f32; 2]>),
    Disc([f32; 2], f32),
    /// The same, in the paper's colour (cut out of the ink).
    Cut(Vec<[f32; 2]>),
    Hole([f32; 2], f32),
}

fn oval(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<[f32; 2]> {
    (0..20)
        .map(|k| {
            let a = k as f32 / 20.0 * std::f32::consts::TAU;
            [cx + rx * a.cos(), cy + ry * a.sin()]
        })
        .collect()
}

fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<[f32; 2]> {
    vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
}

fn silhouette(p: Picture) -> Vec<Prim> {
    use Prim::*;
    match p {
        Picture::Star => {
            // Five points: a pentagon and five triangles.
            let pt = |k: usize, r: f32| {
                let a = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::TAU / 10.0;
                [r * a.cos(), r * a.sin()]
            };
            let mut out = vec![Poly((0..5).map(|k| pt(2 * k + 1, 0.42)).collect())];
            for k in 0..5 {
                out.push(Poly(vec![
                    pt(2 * k, 1.0),
                    pt(2 * k + 1, 0.42),
                    pt((2 * k + 9) % 10, 0.42),
                ]));
            }
            out
        }
        Picture::Fist => vec![
            // The burst behind it.
            Poly(vec![[-1.0, -0.1], [-0.55, -0.02], [-0.55, 0.08]]),
            Poly(vec![[-0.9, -0.75], [-0.5, -0.3], [-0.58, -0.22]]),
            Poly(vec![[-0.9, 0.7], [-0.5, 0.28], [-0.58, 0.2]]),
            // Fist, knuckles to the left, and the forearm.
            Poly(rect(-0.4, -0.42, 0.3, 0.42)),
            Disc([-0.4, -0.3], 0.14),
            Disc([-0.42, -0.1], 0.14),
            Disc([-0.42, 0.1], 0.14),
            Disc([-0.4, 0.3], 0.14),
            Poly(vec![[0.3, -0.34], [1.0, -0.26], [1.0, 0.26], [0.3, 0.34]]),
            // The thumb, folded over.
            Cut(vec![[-0.2, 0.04], [0.22, 0.0], [0.24, 0.1], [-0.18, 0.14]]),
        ],
        Picture::Flame => vec![
            Disc([0.0, 0.35], 0.58),
            Poly(vec![[-0.56, 0.2], [0.56, 0.2], [0.12, -1.0]]),
            Poly(vec![[-0.5, 0.3], [-0.1, 0.0], [-0.42, -0.55]]),
            Hole([0.02, 0.5], 0.3),
            Cut(vec![[-0.27, 0.42], [0.31, 0.42], [0.05, -0.2]]),
        ],
        Picture::Car => vec![
            Poly(vec![
                [-0.95, 0.05],
                [0.9, 0.05],
                [0.98, 0.42],
                [-0.98, 0.42],
            ]),
            Poly(vec![
                [-0.5, -0.38],
                [0.25, -0.38],
                [0.55, 0.06],
                [-0.62, 0.06],
            ]),
            // The fin.
            Poly(vec![[-0.98, -0.2], [-0.72, 0.06], [-0.98, 0.06]]),
            Cut(vec![
                [-0.4, -0.28],
                [-0.1, -0.28],
                [-0.1, 0.02],
                [-0.48, 0.02],
            ]),
            Cut(vec![
                [-0.02, -0.28],
                [0.22, -0.28],
                [0.4, 0.02],
                [-0.02, 0.02],
            ]),
            Disc([-0.55, 0.45], 0.24),
            Disc([0.55, 0.45], 0.24),
            Hole([-0.55, 0.45], 0.1),
            Hole([0.55, 0.45], 0.1),
        ],
        Picture::Robot => vec![
            Poly(rect(-0.3, -0.95, 0.3, -0.45)),
            Poly(rect(-0.03, -1.0, 0.03, -0.9)),
            Hole([-0.13, -0.72], 0.08),
            Hole([0.13, -0.72], 0.08),
            Poly(rect(-0.48, -0.38, 0.48, 0.35)),
            Poly(rect(-0.78, -0.36, -0.56, 0.3)),
            Poly(rect(0.56, -0.36, 0.78, 0.3)),
            Poly(rect(-0.4, 0.4, -0.1, 1.0)),
            Poly(rect(0.1, 0.4, 0.4, 1.0)),
            Hole([0.0, -0.05], 0.16),
        ],
        Picture::Saucer => vec![
            Disc([0.0, -0.28], 0.36),
            Poly(oval(0.0, -0.1, 0.98, 0.26)),
            Hole([-0.5, -0.1], 0.07),
            Hole([0.0, -0.06], 0.07),
            Hole([0.5, -0.1], 0.07),
            // The beam.
            Poly(vec![[-0.16, 0.2], [0.16, 0.2], [0.5, 1.0], [-0.5, 1.0]]),
            Cut(vec![[-0.1, 0.3], [0.1, 0.3], [0.3, 0.92], [-0.3, 0.92]]),
        ],
        Picture::Airship => vec![
            Poly(oval(0.0, -0.15, 0.98, 0.36)),
            Poly(vec![[0.62, -0.62], [0.98, -0.7], [0.98, -0.2], [0.7, -0.2]]),
            Poly(vec![[0.62, 0.32], [0.98, 0.4], [0.98, -0.1], [0.7, -0.1]]),
            Poly(rect(-0.3, 0.2, 0.25, 0.42)),
            Cut(rect(-0.22, 0.26, -0.1, 0.34)),
            Cut(rect(0.0, 0.26, 0.12, 0.34)),
        ],
        Picture::Cuffs => vec![
            Disc([-0.5, 0.1], 0.46),
            Hole([-0.5, 0.1], 0.3),
            Disc([0.5, 0.1], 0.46),
            Hole([0.5, 0.1], 0.3),
            Disc([-0.1, -0.3], 0.09),
            Disc([0.1, -0.3], 0.09),
            Disc([0.0, -0.42], 0.09),
        ],
        Picture::Bag => vec![
            Disc([0.0, 0.3], 0.62),
            Poly(vec![[-0.22, -0.3], [0.22, -0.3], [0.4, 0.0], [-0.4, 0.0]]),
            Poly(vec![
                [-0.36, -0.75],
                [0.36, -0.75],
                [0.2, -0.3],
                [-0.2, -0.3],
            ]),
            Cut(rect(-0.26, -0.34, 0.26, -0.26)),
            Hole([0.0, 0.32], 0.22),
            Disc([0.0, 0.32], 0.12),
        ],
        Picture::Helmet => vec![
            Disc([0.0, -0.1], 0.72),
            Poly(rect(-0.72, -0.1, 0.72, 0.75)),
            Poly(rect(-0.06, -1.0, 0.06, -0.7)),
            Cut(rect(-0.5, 0.0, 0.5, 0.14)),
            Cut(rect(-0.05, 0.14, 0.05, 0.6)),
        ],
        Picture::Mole => vec![
            Disc([0.0, 0.1], 0.7),
            // The tunnelling cap and its lamp.
            Poly(vec![
                [-0.66, -0.2],
                [0.66, -0.2],
                [0.4, -0.72],
                [-0.4, -0.72],
            ]),
            Disc([0.0, -0.74], 0.16),
            Hole([-0.28, 0.08], 0.2),
            Hole([0.28, 0.08], 0.2),
            Disc([-0.28, 0.08], 0.08),
            Disc([0.28, 0.08], 0.08),
            Cut(vec![[-0.1, 0.42], [0.1, 0.42], [0.0, 0.6]]),
        ],
        Picture::Rubble => vec![
            Poly(vec![[-0.95, 0.9], [-0.7, 0.2], [-0.2, 0.3], [-0.1, 0.9]]),
            Poly(vec![[-0.3, 0.9], [0.0, -0.1], [0.5, 0.1], [0.6, 0.9]]),
            Poly(vec![[0.4, 0.9], [0.62, 0.4], [0.95, 0.5], [0.98, 0.9]]),
            Poly(vec![[-0.6, -0.1], [-0.3, -0.5], [-0.1, -0.2], [-0.35, 0.1]]),
            // A hand reaching in.
            Poly(vec![[0.3, -0.95], [0.5, -0.95], [0.42, -0.3], [0.3, -0.3]]),
            Disc([0.36, -0.28], 0.12),
        ],
    }
}

/// Draws the front page `e` as `lay` says. `fine`: with the halftone's
/// dots (the big page); small pages get a flat tone.
pub fn page(painter: &egui::Painter, lay: &Lay, e: &Edition, fine: bool) {
    let ctx = painter.ctx().clone();
    let (w, h) = (PAGE.x, PAGE.y);
    let (l, t) = (-w / 2.0, -h / 2.0);
    // A shadow, the sheet, and a second sheet peeping out under it.
    let sheet = Rect::from_min_size(pos2(l, t), PAGE);
    lay.fill(
        painter,
        sheet.translate(vec2(7.0, 9.0)),
        Color32::from_black_alpha(110),
    );
    lay.fill(painter, sheet.translate(vec2(3.0, 3.0)), NEWSPRINT_DARK);
    lay.fill(painter, sheet, NEWSPRINT);
    // Masthead.
    let mast = FontId::new(30.0, family(&ctx, "hw-sign"));
    text(
        painter,
        lay,
        0.0,
        t + 4.0,
        w - 20.0,
        "The Daily Comet",
        mast,
        INK,
    );
    let small = FontId::new(8.5, family(&ctx, "hw-type"));
    lay.fill(
        painter,
        Rect::from_min_max(pos2(l + 12.0, t + 52.0), pos2(-l - 12.0, t + 54.0)),
        INK,
    );
    text(
        painter,
        lay,
        0.0,
        t + 56.5,
        w - 24.0,
        &format!(
            "No. {}     ALL THE NEWS UNDER BOTH MOONS     FIVE CENTS",
            e.number
        ),
        small.clone(),
        INK,
    );
    lay.fill(
        painter,
        Rect::from_min_max(pos2(l + 12.0, t + 69.0), pos2(-l - 12.0, t + 70.0)),
        INK,
    );
    // Kicker and headline.
    let kicker = FontId::new(13.0, family(&ctx, "hw-ui"));
    text(
        painter,
        lay,
        0.0,
        t + 75.0,
        w - 24.0,
        &e.kicker,
        kicker,
        RED_INK,
    );
    let head_family = family(&ctx, "hw-logo");
    let size = headline_size(painter, &e.head, &head_family, w - 28.0);
    let head_h = text(
        painter,
        lay,
        0.0,
        t + 93.0,
        w - 28.0,
        &e.head,
        FontId::new(size, head_family),
        INK,
    );
    // The picture: a halftone box with the silhouette.
    let top = t + 93.0 + head_h.min(108.0) + 8.0;
    let bottom = (-t - 86.0).max(top + 70.0);
    let frame = Rect::from_min_max(pos2(l + 16.0, top), pos2(-l - 16.0, bottom));
    lay.fill(painter, frame.expand(1.5), INK);
    lay.fill(painter, frame, HALFTONE);
    if fine {
        let step = 7.0;
        let dot = with_alpha(Color32::from_rgb(150, 142, 126), lay.alpha);
        let mut y = frame.top() + step * 0.5;
        let mut row = 0;
        while y < frame.bottom() {
            let mut x = frame.left() + step * if row % 2 == 0 { 0.5 } else { 1.0 };
            while x < frame.right() - 1.0 {
                // Lighter towards the middle, like a flash photograph.
                let d = ((x / frame.width()).powi(2)
                    + ((y - frame.center().y) / frame.height()).powi(2))
                .sqrt();
                let r = (0.9 + 2.6 * d).min(2.6) * lay.scale;
                painter.circle_filled(lay.at(x, y), r, dot);
                x += step;
            }
            y += step;
            row += 1;
        }
    }
    let c = frame.center();
    let r = frame.height().min(frame.width()) * 0.42;
    let ink = with_alpha(INK, lay.alpha);
    let cut = with_alpha(HALFTONE, lay.alpha);
    let at = |p: &[f32; 2]| lay.at(c.x + p[0] * r, c.y + p[1] * r);
    for prim in silhouette(e.picture) {
        match prim {
            Prim::Poly(pts) => {
                painter.add(Shape::convex_polygon(
                    pts.iter().map(at).collect(),
                    ink,
                    Stroke::NONE,
                ));
            }
            Prim::Cut(pts) => {
                painter.add(Shape::convex_polygon(
                    pts.iter().map(at).collect(),
                    cut,
                    Stroke::NONE,
                ));
            }
            Prim::Disc(p, rad) => {
                painter.circle_filled(at(&p), rad * r * lay.scale, ink);
            }
            Prim::Hole(p, rad) => {
                painter.circle_filled(at(&p), rad * r * lay.scale, cut);
            }
        }
    }
    // The line under it, and columns of small print.
    let body = FontId::new(10.5, family(&ctx, "hw-type"));
    let sub_h = text(painter, lay, 0.0, bottom + 7.0, w - 30.0, &e.sub, body, INK);
    let mut y = bottom + 7.0 + sub_h + 7.0;
    let grey = Color32::from_rgb(150, 144, 130);
    let mut k = e.number.wrapping_mul(2_654_435_761);
    while y < -t - 10.0 {
        for col in 0..3 {
            k = k.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let x0 = l + 16.0 + col as f32 * 91.0;
            let len = 60.0 + (k >> 24) as f32 / 255.0 * 24.0;
            lay.fill(
                painter,
                Rect::from_min_size(pos2(x0, y), vec2(len, 1.6)),
                grey,
            );
        }
        y += 5.5;
    }
}

/// The front pages flashed on screen, one after another.
#[derive(Default)]
pub struct Paper {
    waiting: VecDeque<Edition>,
    showing: Option<(Edition, f32)>,
    rest: f32,
    /// A page landed this frame (for its sound).
    pub landed: bool,
}

impl Paper {
    /// A new edition is out.
    pub fn print(&mut self, e: Edition) {
        self.waiting.push_back(e);
        while self.waiting.len() > WAITING {
            self.waiting.pop_front();
        }
    }

    /// Whether a page is on screen or about to be.
    pub fn busy(&self) -> bool {
        self.showing.is_some() || !self.waiting.is_empty()
    }

    /// Draws the page on show. `quiet`: hold new pages back (a fight is
    /// on, a window is open).
    pub fn show(&mut self, ctx: &egui::Context, dt: f32, quiet: bool) {
        self.landed = false;
        self.rest = (self.rest - dt).max(0.0);
        if self.showing.is_none() && self.rest <= 0.0 && !quiet {
            if let Some(e) = self.waiting.pop_front() {
                self.showing = Some((e, 0.0));
            }
        }
        let Some((e, age)) = &mut self.showing else {
            return;
        };
        let before = *age;
        *age += dt;
        let t = *age;
        self.landed = before < SPIN && t >= SPIN;
        if t > SPIN + HOLD + LEAVE {
            self.showing = None;
            self.rest = GAP;
            return;
        }
        let still = uikit::reduced_motion(ctx);
        let screen = ctx.screen_rect();
        let scale = (screen.height() / 1080.0).clamp(0.6, 1.4) * 0.8;
        let home = pos2(
            screen.right() - PAGE.x * scale * 0.5 - 34.0,
            screen.top() + PAGE.y * scale * 0.5 + 196.0,
        );
        let tilt = -0.06;
        let lay = if t < SPIN {
            let k = t / SPIN;
            let ease = 1.0 - (1.0 - k).powi(3);
            if still {
                Lay {
                    centre: home,
                    scale,
                    angle: tilt,
                    alpha: ease,
                }
            } else {
                Lay {
                    centre: screen.center() + (home - screen.center()) * ease,
                    scale: scale * (0.06 + 0.94 * ease),
                    angle: tilt + (1.0 - ease) * std::f32::consts::TAU * 2.5,
                    alpha: (k * 4.0).min(1.0),
                }
            }
        } else if t < SPIN + HOLD {
            // It settles with a little bounce.
            let k = ((t - SPIN) / 0.25).min(1.0);
            let bump = if still {
                0.0
            } else {
                (1.0 - k) * 0.06 * (k * 9.0).cos()
            };
            Lay {
                centre: home,
                scale: scale * (1.0 + bump),
                angle: tilt,
                alpha: 1.0,
            }
        } else {
            let k = (t - SPIN - HOLD) / LEAVE;
            Lay {
                centre: home + vec2(60.0 * k * k, -20.0 * k),
                scale: scale * (1.0 - 0.15 * k),
                angle: tilt + 0.25 * k * k,
                alpha: 1.0 - k,
            }
        };
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("daily comet"),
        ));
        page(&painter, &lay, e, true);
        ctx.request_repaint();
    }
}

/// The scrapbook (I): how well the town knows the hero, and the front
/// pages they made, newest first.
#[derive(Default)]
pub struct Scrapbook {
    pub open: bool,
    /// The pointer was locked when it opened: lock it again after.
    pub relock: bool,
}

impl Scrapbook {
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    pub fn show(&mut self, ctx: &egui::Context, fame: Fame, level: u8, cuttings: &[Edition]) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        let win = crate::layout::window_centered(
            ctx,
            egui::Window::new("Press cuttings"),
            "Press cuttings",
            ctx.screen_rect().center(),
        )
        .collapsible(false)
        .resizable(false);
        googie::window(ctx, win, "Press cuttings", Some(&mut open), |ui| {
            ui.set_width(3.0 * 168.0 + 20.0);
            fame_strip(ui, fame, level);
            ui.add_space(8.0);
            if cuttings.is_empty() {
                ui.label(
                    egui::RichText::new(
                        "Nothing in the papers yet. Do something the town will talk about.",
                    )
                    .color(uikit::DIM)
                    .size(14.0),
                );
                return;
            }
            let scale = 0.52;
            let cell = PAGE * scale + vec2(12.0, 14.0);
            egui::ScrollArea::vertical()
                .max_height(2.0 * cell.y + 8.0)
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                        for (k, e) in cuttings.iter().rev().enumerate() {
                            let (r, _) = ui.allocate_exact_size(cell, egui::Sense::hover());
                            if !ui.is_rect_visible(r) {
                                continue;
                            }
                            // Pasted in a little crooked, each its own way.
                            let tilt = [(-0.03), 0.02, -0.012, 0.035, -0.02, 0.01][k % 6];
                            let lay = Lay {
                                centre: r.center(),
                                scale,
                                angle: tilt,
                                alpha: 1.0,
                            };
                            page(&ui.painter().with_clip_rect(r.expand(6.0)), &lay, e, false);
                        }
                    });
                });
        });
        self.open = open;
    }
}

/// The hero's fame as a row of stars, its name and what comes next.
fn fame_strip(ui: &mut egui::Ui, fame: Fame, level: u8) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(4.0 * 30.0, 30.0), egui::Sense::hover());
        let t = uikit::now(ui.ctx());
        for k in 0..4u8 {
            let c = pos2(r.left() + 15.0 + k as f32 * 30.0, r.center().y);
            let lit = fame.rank() > k;
            let col = if lit {
                uikit::GOLD
            } else {
                Color32::from_gray(70)
            };
            let turn = if lit && !uikit::reduced_motion(ui.ctx()) {
                0.12 * (t * 1.3 + k as f32).sin()
            } else {
                0.0
            };
            star5(ui.painter(), c, 12.0, turn, col);
        }
        ui.add_space(8.0);
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(fame.title())
                    .font(uikit::ui_font(ui.ctx(), 20.0))
                    .color(uikit::CREAM),
            );
            let next = match fame.next_level() {
                Some(l) if l > level => format!("The town knows you better at level {l}."),
                Some(_) => "The townsfolk must think well of you first.".to_string(),
                None => "Everybody knows your name.".to_string(),
            };
            ui.label(egui::RichText::new(next).color(uikit::DIM).size(13.0));
        });
    });
}

/// A five-pointed star.
pub fn star5(painter: &egui::Painter, c: Pos2, r: f32, turn: f32, col: Color32) {
    let pt = |k: usize, rad: f32| {
        let a = turn - std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::TAU / 10.0;
        c + vec2(rad * a.cos(), rad * a.sin())
    };
    painter.add(Shape::convex_polygon(
        (0..5).map(|k| pt(2 * k + 1, r * 0.42)).collect(),
        col,
        Stroke::NONE,
    ));
    for k in 0..5 {
        painter.add(Shape::convex_polygon(
            vec![
                pt(2 * k, r),
                pt(2 * k + 1, r * 0.42),
                pt((2 * k + 9) % 10, r * 0.42),
            ],
            col,
            Stroke::NONE,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edition(n: u32) -> Edition {
        Edition {
            kicker: "EXTRA!".into(),
            head: format!("HEADLINE {n}"),
            sub: "Sub".into(),
            picture: Picture::Star,
            number: n,
        }
    }

    #[test]
    fn pages_come_one_after_another_and_old_ones_wait_only_in_the_scrapbook() {
        let ctx = egui::Context::default();
        let mut paper = Paper::default();
        for n in 1..=5 {
            paper.print(edition(n));
        }
        let mut seen = Vec::new();
        let mut landed = 0;
        for _ in 0..2000 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                paper.show(ctx, 0.05, false);
            });
            landed += paper.landed as u32;
            if let Some((e, _)) = &paper.showing {
                if seen.last() != Some(&e.number) {
                    seen.push(e.number);
                }
            }
        }
        // Only the newest two were kept waiting.
        assert_eq!(seen, vec![4, 5]);
        assert_eq!(landed, 2);
        assert!(!paper.busy());
    }

    #[test]
    fn a_page_waits_while_it_must_be_quiet() {
        let ctx = egui::Context::default();
        let mut paper = Paper::default();
        paper.print(edition(1));
        for _ in 0..50 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| paper.show(ctx, 0.1, true));
        }
        assert!(paper.showing.is_none() && paper.busy());
        let _ = ctx.run(egui::RawInput::default(), |ctx| paper.show(ctx, 0.1, false));
        assert!(paper.showing.is_some());
    }

    #[test]
    fn every_picture_has_a_silhouette_inside_its_box() {
        for p in Picture::ALL {
            let prims = silhouette(p);
            assert!(!prims.is_empty(), "{p:?}");
            for prim in prims {
                let pts: Vec<[f32; 2]> = match prim {
                    Prim::Poly(v) | Prim::Cut(v) => {
                        assert!(v.len() >= 3, "{p:?}");
                        v
                    }
                    Prim::Disc(c, r) | Prim::Hole(c, r) => {
                        vec![[c[0] - r, c[1] - r], [c[0] + r, c[1] + r]]
                    }
                };
                for q in pts {
                    assert!(q[0].abs() <= 1.12 && q[1].abs() <= 1.12, "{p:?} {q:?}");
                }
            }
        }
    }
}
