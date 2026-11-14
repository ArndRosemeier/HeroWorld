//! Googie shapes for the game's screens: the space-age diner and motel-sign
//! look of the 1950s, so a window reads as a designed object, not a box.
//!
//! - Panels have two opposite corners cut on a slant and the other two
//!   rounded, an enamel gradient, a gold rim with a turquoise pinstripe, and
//!   a faint formica pattern of boomerangs, sparkles and dots.
//! - Titles sit on a red sign plate pointed like a pennant, in a diner-sign
//!   script, with chrome speed lines trailing off it.
//! - Sputnik starbursts (a ball with rods of two lengths) mark corners and
//!   turn slowly; an atom with orbiting electrons crowns the power tray.
//!
//! [`window`] and [`modal`] wrap egui's windows in all of that; [`plate`]
//! is the small version for prompts and status lines.

use crate::uikit::{
    self, mix, with_alpha, AETHER, CREAM, DEEP, GOLD, GOLD_DARK, GOLD_LIGHT, HERO_RED, NAVY,
};
use engine::ui::egui::{
    self, epaint::CornerRadius, pos2, vec2, Align2, Color32, FontFamily, FontId, Margin, Mesh,
    Pos2, Rect, Sense, Shape, Stroke,
};
use std::f32::consts::{PI, TAU};

/// Turquoise: the pinstripe and the pattern, the Googie accent.
pub const TEAL: Color32 = Color32::from_rgb(64, 206, 196);

/// The diner-sign script (Pacifico) for titles; Titan One before it is in.
pub fn sign_font(ctx: &egui::Context, size: f32) -> FontId {
    let f = FontFamily::Name("hw-sign".into());
    if ctx.fonts(|fonts| fonts.families().contains(&f)) {
        FontId::new(size, f)
    } else {
        uikit::ui_font(ctx, size)
    }
}

/// The outline of a Googie panel: top left and bottom right cut on a
/// slant (`cut`), the other two corners rounded (`round`). Clockwise from
/// the top edge; convex.
pub fn outline(r: Rect, cut: f32, round: f32) -> Vec<Pos2> {
    let cut = cut.min(r.width() * 0.4).min(r.height() * 0.4).max(0.0);
    let round = round.min(r.width() * 0.4).min(r.height() * 0.4).max(0.0);
    let mut p = vec![pos2(r.left() + cut, r.top())];
    arc(
        &mut p,
        pos2(r.right() - round, r.top() + round),
        round,
        -PI * 0.5,
        0.0,
    );
    p.push(pos2(r.right(), r.bottom() - cut));
    p.push(pos2(r.right() - cut, r.bottom()));
    arc(
        &mut p,
        pos2(r.left() + round, r.bottom() - round),
        round,
        PI * 0.5,
        PI,
    );
    p.push(pos2(r.left(), r.top() + cut));
    p
}

fn arc(p: &mut Vec<Pos2>, c: Pos2, rad: f32, from: f32, to: f32) {
    let n = 6;
    for k in 0..=n {
        let a = from + (to - from) * k as f32 / n as f32;
        p.push(c + vec2(a.cos(), a.sin()) * rad);
    }
}

/// A convex polygon filled from `top` colour to `bottom` colour.
pub fn gradient_polygon(painter: &egui::Painter, pts: &[Pos2], top: Color32, bottom: Color32) {
    if pts.len() < 3 {
        return;
    }
    let (y0, y1) = pts
        .iter()
        .fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.y), b.max(p.y)));
    let at = |y: f32| mix(top, bottom, (y - y0) / (y1 - y0).max(1.0));
    let c = pts.iter().fold(vec2(0.0, 0.0), |s, p| s + p.to_vec2()) / pts.len() as f32;
    let mut m = Mesh::default();
    m.colored_vertex(c.to_pos2(), at(c.y));
    for p in pts {
        m.colored_vertex(*p, at(p.y));
    }
    let n = pts.len() as u32;
    for k in 0..n {
        m.add_triangle(0, 1 + k, 1 + (k + 1) % n);
    }
    painter.add(Shape::mesh(m));
}

/// A Googie panel: block shadow, enamel gradient, formica pattern, gold
/// rim, turquoise pinstripe. `seed` varies the pattern from panel to panel.
pub fn panel(painter: &egui::Painter, r: Rect, cut: f32, alpha: f32, seed: u32) {
    let pts = outline(r, cut, cut * 0.55);
    let shadow: Vec<Pos2> = pts.iter().map(|p| *p + vec2(5.0, 6.0)).collect();
    painter.add(Shape::convex_polygon(
        shadow,
        Color32::from_black_alpha((150.0 * alpha) as u8),
        Stroke::NONE,
    ));
    gradient_polygon(
        painter,
        &pts,
        with_alpha(Color32::from_rgb(38, 46, 104), 0.95 * alpha),
        with_alpha(Color32::from_rgb(16, 18, 48), 0.95 * alpha),
    );
    formica(painter, r.shrink(cut * 0.5 + 4.0), seed, 0.11 * alpha);
    painter.add(Shape::closed_line(
        pts,
        Stroke::new(3.0_f32, with_alpha(GOLD, alpha)),
    ));
    painter.add(Shape::closed_line(
        outline(
            r.shrink(5.5),
            (cut - 3.0).max(0.0),
            (cut * 0.55 - 3.0).max(0.0),
        ),
        Stroke::new(1.0_f32, with_alpha(TEAL, 0.55 * alpha)),
    ));
}

fn hash(a: u32, b: u32) -> f32 {
    let mut x = a.wrapping_mul(0x9E37_79B9) ^ b.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297A_2D39);
    x ^= x >> 15;
    (x & 0xFF_FFFF) as f32 / 0x100_0000 as f32
}

/// The 1950s kitchen-counter pattern: boomerangs, four-point sparkles and
/// dots scattered (always the same for a seed) over `r`, faint.
pub fn formica(painter: &egui::Painter, r: Rect, seed: u32, alpha: f32) {
    if r.width() < 20.0 || r.height() < 20.0 || alpha <= 0.0 {
        return;
    }
    let cell = 74.0;
    let (nx, ny) = (
        (r.width() / cell).ceil() as u32,
        (r.height() / cell).ceil() as u32,
    );
    let clip = painter;
    for j in 0..ny {
        for i in 0..nx {
            let k = j * 97 + i;
            let h = |n: u32| hash(seed.wrapping_add(k), n);
            let c = pos2(
                r.left() + (i as f32 + 0.15 + 0.7 * h(1)) * cell,
                r.top() + (j as f32 + 0.15 + 0.7 * h(2)) * cell,
            );
            // Nothing pokes out of the panel (the pattern is not clipped).
            if !r.shrink(16.0).contains(c) {
                continue;
            }
            let col = if h(3) < 0.5 { TEAL } else { GOLD };
            match (h(4) * 3.0) as u32 {
                0 => boomerang(
                    clip,
                    c,
                    13.0 + 6.0 * h(5),
                    h(6) * TAU,
                    with_alpha(col, alpha),
                ),
                1 => uikit::star(clip, c, 6.0 + 3.0 * h(5), h(6), with_alpha(CREAM, alpha)),
                _ => {
                    for d in 0..3 {
                        let a = h(7) * TAU + d as f32 * 2.1;
                        clip.circle_filled(
                            c + vec2(a.cos(), a.sin()) * 7.0,
                            2.0,
                            with_alpha(col, alpha),
                        );
                    }
                }
            }
        }
    }
}

/// A boomerang: two tapering arms from an elbow at `c`.
pub fn boomerang(painter: &egui::Painter, c: Pos2, size: f32, angle: f32, col: Color32) {
    for side in [-1.0_f32, 1.0] {
        let a = angle + side * 0.95;
        let d = vec2(a.cos(), a.sin());
        let n = vec2(-d.y, d.x);
        let tip = c + d * size;
        painter.add(Shape::convex_polygon(
            vec![
                c + n * size * 0.16,
                tip + n * size * 0.05,
                tip - n * size * 0.05,
                c - n * size * 0.16,
            ],
            col,
            Stroke::NONE,
        ));
    }
    painter.circle_filled(c, size * 0.16, col);
}

/// A sputnik starburst: a ball with eight rods, long ones ending in balls.
pub fn sputnik(painter: &egui::Painter, c: Pos2, r: f32, turn: f32, col: Color32, core: Color32) {
    for k in 0..8 {
        let a = turn + k as f32 * TAU / 8.0;
        let d = vec2(a.cos(), a.sin());
        let long = k % 2 == 0;
        let len = if long { r } else { r * 0.62 };
        painter.line_segment(
            [c + d * r * 0.2, c + d * len],
            Stroke::new((r * 0.11).max(1.2), col),
        );
        if long {
            painter.circle_filled(c + d * len, (r * 0.13).max(1.6), col);
        }
    }
    painter.circle_filled(c, r * 0.3, col);
    painter.circle_filled(c, r * 0.18, core);
}

/// An atom: three orbits round a nucleus, an electron racing round each.
pub fn atom(painter: &egui::Painter, c: Pos2, r: f32, t: f32, col: Color32, glow: Color32) {
    for k in 0..3 {
        let tilt = k as f32 * PI / 3.0;
        let (s, co) = tilt.sin_cos();
        let at = |a: f32| {
            let (x, y) = (a.cos() * r, a.sin() * r * 0.36);
            c + vec2(x * co - y * s, x * s + y * co)
        };
        let ring: Vec<Pos2> = (0..32).map(|i| at(i as f32 * TAU / 32.0)).collect();
        painter.add(Shape::closed_line(ring, Stroke::new(1.4_f32, col)));
        let e = at(t * (2.2 + k as f32 * 0.5) + k as f32 * 2.0);
        painter.circle_filled(e, r * 0.16, with_alpha(glow, 0.35));
        painter.circle_filled(e, r * 0.09, glow);
    }
    painter.circle_filled(c, r * 0.22, HERO_RED);
    painter.circle_filled(c - vec2(r * 0.06, r * 0.06), r * 0.08, GOLD_LIGHT);
}

/// The pennant shape of a sign plate: a slanted left end, a point on the
/// right.
fn pennant_points(r: Rect) -> Vec<Pos2> {
    let h = r.height();
    let lean = h * 0.32;
    vec![
        pos2(r.left() + lean, r.top()),
        pos2(r.right() - h * 0.5, r.top()),
        pos2(r.right(), r.center().y),
        pos2(r.right() - h * 0.5, r.bottom()),
        pos2(r.left(), r.bottom()),
    ]
}

/// A sign plate: a red pennant with a gold rim and its words on it, chrome
/// speed lines trailing off its point (`trail` long; 0 for none). `left_top`
/// is its top left. Returns the plate.
pub fn sign(
    painter: &egui::Painter,
    left_top: Pos2,
    text: &str,
    font: FontId,
    fill: Color32,
    trail: f32,
    t: f32,
) -> Rect {
    let galley = painter.layout_no_wrap(text.to_owned(), font.clone(), CREAM);
    let size = font.size;
    let h = (galley.size().y + size * 0.15).max(size * 1.35);
    let w = galley.size().x + size * 0.8 + h * 0.8;
    let r = Rect::from_min_size(left_top, vec2(w, h));
    let pts = pennant_points(r);
    if trail > 0.0 {
        speed_lines(painter, pos2(r.right() - h * 0.25, r.center().y), trail, t);
    }
    painter.add(Shape::convex_polygon(
        pts.iter().map(|p| *p + vec2(3.0, 4.0)).collect(),
        Color32::from_black_alpha(140),
        Stroke::NONE,
    ));
    gradient_polygon(
        painter,
        &pts,
        mix(fill, Color32::WHITE, 0.18),
        mix(fill, Color32::BLACK, 0.2),
    );
    painter.add(Shape::closed_line(pts, Stroke::new(2.5_f32, GOLD)));
    let text_at = pos2(
        r.left() + h * 0.32 + size * 0.35,
        r.center().y - galley.size().y * 0.5,
    );
    let shade = painter.layout_no_wrap(text.to_owned(), font, DEEP);
    painter.galley(text_at + vec2(1.5, 2.0), shade, DEEP);
    painter.galley(text_at, galley, CREAM);
    r
}

/// Three chrome lines streaking off to the right, a gleam running along.
fn speed_lines(painter: &egui::Painter, from: Pos2, length: f32, t: f32) {
    for (k, (dy, len, col)) in [(-6.0, 1.0, GOLD), (0.0, 0.78, TEAL), (6.0, 0.56, GOLD_DARK)]
        .into_iter()
        .enumerate()
    {
        let a = from + vec2(0.0, dy);
        let b = a + vec2(length * len, 0.0);
        painter.add(Shape::convex_polygon(
            vec![a + vec2(0.0, -1.6), b, a + vec2(0.0, 1.6)],
            col,
            Stroke::NONE,
        ));
        let g = ((t * 0.45 + k as f32 * 0.13) % 1.6) / 1.6;
        if g < 0.6 {
            let x = a.x + (g / 0.6) * length * len;
            painter.circle_filled(pos2(x, a.y), 1.6, with_alpha(Color32::WHITE, 0.8));
        }
    }
}

/// The corner mark of a window: a boomerang on its bottom left.
fn trims(painter: &egui::Painter, body: Rect) {
    boomerang(
        painter,
        body.left_bottom() + vec2(18.0, -16.0),
        14.0,
        -PI * 0.5 + 0.3,
        with_alpha(TEAL, 0.9),
    );
}

/// The window's painter, free to paint past its content (the rim, the
/// plate and the shadow lie outside it).
fn wide(ui: &egui::Ui) -> egui::Painter {
    // (`with_clip_rect` would only narrow the window's clip.)
    egui::Painter::new(ui.ctx().clone(), ui.layer_id(), ui.ctx().screen_rect())
}

/// How much room the sign plate and the rim take round a window's content.
const MARGIN: Margin = Margin {
    left: 22,
    right: 22,
    top: 58,
    bottom: 20,
};

/// Paints a Googie window round `content` (the content's rectangle) under
/// what is already in `ui`, with its title plate; returns the body.
fn dress(ui: &egui::Ui, bg: egui::layers::ShapeIdx, content: Rect, title: &str, seed: u32) -> Rect {
    let ctx = ui.ctx().clone();
    let t = uikit::now(&ctx);
    let outer = Rect::from_min_max(
        content.min - vec2(MARGIN.left as f32, MARGIN.top as f32),
        content.max + vec2(MARGIN.right as f32, MARGIN.bottom as f32),
    );
    let body = Rect::from_min_max(outer.min + vec2(0.0, 20.0), outer.max - vec2(6.0, 7.0));
    // Everything goes into a list painted where the placeholder is, so it
    // lies under the content.
    // Painted on a scratch layer and taken back, then put where the
    // placeholder is, so it lies under the content.
    let scratch = egui::LayerId::new(egui::Order::Background, egui::Id::new("googie scratch"));
    let layer = egui::Painter::new(ctx.clone(), scratch, ctx.screen_rect().expand(200.0));
    let before = ctx.graphics_mut(|g| g.entry(scratch).next_idx());
    let still = uikit::reduced_motion(&ctx);
    panel(&layer, body, 24.0, 1.0, seed);
    trims(&layer, body);
    sign(
        &layer,
        outer.min + vec2(14.0, 0.0),
        title,
        sign_font(&ctx, 22.0),
        HERO_RED,
        (body.width() * 0.42).min(260.0),
        if still { 0.0 } else { t },
    );
    let shapes: Vec<Shape> = ctx.graphics_mut(|g| {
        let list = g.entry(scratch);
        let after = list.next_idx();
        let mut out = Vec::new();
        for k in before.0..after.0 {
            let idx = egui::layers::ShapeIdx(k);
            list.mutate_shape(idx, |c| {
                out.push(std::mem::replace(&mut c.shape, Shape::Noop));
            });
        }
        out
    });
    wide(ui).set(bg, Shape::Vec(shapes));
    body
}

/// The close button on a window's top right: a red disc with a cream
/// cross on a turning sputnik. True when clicked.
fn close_button(ui: &mut egui::Ui, body: Rect, id: egui::Id) -> bool {
    let c = body.right_top() + vec2(-24.0, 22.0);
    let r = Rect::from_center_size(c, vec2(26.0, 26.0));
    // Its own little area: the window's content may not take clicks out
    // on its rim.
    let resp = egui::Area::new(id.with("close area"))
        .order(egui::Order::Foreground)
        .fixed_pos(r.min)
        .show(ui.ctx(), |ui| {
            ui.allocate_exact_size(r.size(), Sense::click()).1
        })
        .inner;
    let t = uikit::now(ui.ctx());
    let turn = if uikit::reduced_motion(ui.ctx()) {
        0.2
    } else {
        t * 0.5
    };
    let hover = ui
        .ctx()
        .animate_bool_with_time(id.with("close hover"), resp.hovered(), 0.12);
    let p = &wide(ui);
    sputnik(p, c, 19.0 + 2.0 * hover, turn, with_alpha(GOLD, 0.9), GOLD);
    p.circle_filled(c + vec2(1.5, 2.0), 10.5, Color32::from_black_alpha(120));
    p.circle_filled(
        c,
        10.5,
        mix(HERO_RED, Color32::from_rgb(255, 96, 80), hover),
    );
    p.circle_stroke(c, 10.5, Stroke::new(1.5_f32, DEEP));
    let s = 4.2;
    for d in [vec2(s, s), vec2(s, -s)] {
        p.line_segment([c - d, c + d], Stroke::new(2.4_f32, CREAM));
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

/// A Googie window: `win` is the egui window to show (placed with
/// `layout::window` under the title as its key, sized, as usual; its title bar and frame are replaced), `title` goes on its sign
/// plate, `open` (if any) gets a close button. Returns what `add` returned,
/// when it was shown.
pub fn window<R>(
    ctx: &egui::Context,
    win: egui::Window<'_>,
    title: &str,
    open: Option<&mut bool>,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<R> {
    if open.as_ref().is_some_and(|o| !**o) {
        return None;
    }
    let id = egui::Id::new(("googie window", title));
    let seed = (hash(title.len() as u32, title.bytes().map(u32::from).sum()) * 1e6) as u32;
    let mut closed = false;
    let shown = win
        .title_bar(false)
        .frame(egui::Frame::NONE.inner_margin(MARGIN))
        .show(ctx, |ui| {
            let bg = wide(ui).add(Shape::Noop);
            let out = add(ui);
            let content = ui.min_rect();
            let body = dress(ui, bg, content, title, seed);
            if open.is_some() {
                closed = close_button(ui, body, id);
            }
            out
        });
    if closed {
        if let Some(o) = open {
            *o = false;
        }
    }
    // The window may be dragged: its place is kept (see `layout.rs`; the
    // caller gave it the saved place under the same name as the title).
    if let Some(s) = &shown {
        crate::layout::remember(ctx, title, s.response.rect, true, false);
    }
    shown.and_then(|s| s.inner)
}

/// Engine's modal (menu button, What's new) as a Googie window: egui's own
/// frame and Engine's heading are hidden while it is drawn, and `add` gets
/// the game's style back.
pub fn modal(
    ui: &engine::ui::UiFrame,
    title: &str,
    open: &mut bool,
    add: impl FnOnce(&mut egui::Ui, &mut bool),
) {
    if !*open {
        return;
    }
    let ctx = ui.ctx().clone();
    let saved = (*ctx.style()).clone();
    ctx.style_mut(|s| {
        s.visuals.window_fill = Color32::TRANSPARENT;
        s.visuals.window_stroke = Stroke::NONE;
        s.visuals.popup_shadow = egui::Shadow::NONE;
        s.spacing.window_margin = MARGIN;
        // Engine writes the title as a heading and a line under it: the
        // sign plate says it instead.
        s.text_styles
            .insert(egui::TextStyle::Heading, FontId::proportional(0.5));
        s.visuals.widgets.noninteractive.bg_stroke = Stroke::NONE;
        s.spacing.item_spacing.y = 0.0;
    });
    let seed = title.len() as u32 * 7919;
    let mut body = None;
    ui.modal(title, open, |panel, open| {
        let ui = panel.ui();
        let bg = wide(ui).add(Shape::Noop);
        ui.set_style(saved.clone());
        ui.add_space(-6.0);
        add(ui, open);
        let content = ui.min_rect();
        body = Some(dress(ui, bg, content, title, seed));
    });
    ctx.set_style(saved);
    let _ = body;
}

/// The small plate for a prompt or a status line: a leaf-shaped navy card
/// (two corners round, two sharp) with a gold rim, a sputnik on its left
/// end. `alert`: a red rim, for what wants the hero now.
pub fn plate<R>(ui: &mut egui::Ui, alert: bool, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let frame = egui::Frame::new()
        .fill(with_alpha(NAVY, 0.94))
        .stroke(Stroke::new(
            if alert { 2.5_f32 } else { 2.0 },
            if alert { HERO_RED } else { GOLD },
        ))
        .corner_radius(leaf(16))
        .inner_margin(Margin {
            left: 26,
            right: 16,
            top: 8,
            bottom: 8,
        })
        .shadow(egui::Shadow {
            offset: [4, 5],
            blur: 0,
            spread: 0,
            color: Color32::from_black_alpha(130),
        });
    let shown = frame.show(ui, add);
    let r = shown.response.rect;
    let t = uikit::now(ui.ctx());
    let turn = if uikit::reduced_motion(ui.ctx()) {
        0.2
    } else {
        t * 0.4
    };
    sputnik(
        ui.painter(),
        pos2(r.left() + 13.0, r.center().y),
        9.0,
        turn,
        if alert { HERO_RED } else { GOLD },
        AETHER,
    );
    shown.inner
}

/// Leaf corners: top left and bottom right round, the others nearly sharp.
pub fn leaf(round: u8) -> CornerRadius {
    CornerRadius {
        nw: round,
        ne: 3,
        sw: 3,
        se: round,
    }
}

/// A heart (health) in a disc.
pub fn heart_icon(painter: &egui::Painter, c: Pos2, r: f32) {
    badge(painter, c, r, Color32::from_rgb(214, 50, 60));
    let s = r * 0.42;
    let col = CREAM;
    painter.circle_filled(c + vec2(-s * 0.5, -s * 0.2), s * 0.55, col);
    painter.circle_filled(c + vec2(s * 0.5, -s * 0.2), s * 0.55, col);
    painter.add(Shape::convex_polygon(
        vec![
            c + vec2(-s * 1.02, 0.0),
            c + vec2(s * 1.02, 0.0),
            c + vec2(0.0, s * 1.15),
        ],
        col,
        Stroke::NONE,
    ));
}

/// A lightning bolt (energy) in a disc.
pub fn bolt_icon(painter: &egui::Painter, c: Pos2, r: f32) {
    badge(painter, c, r, Color32::from_rgb(40, 110, 220));
    let s = r * 0.62;
    let pts = [
        vec2(0.15, -1.0),
        vec2(-0.55, 0.15),
        vec2(-0.05, 0.15),
        vec2(-0.2, 1.0),
        vec2(0.55, -0.2),
        vec2(0.05, -0.2),
    ];
    let p: Vec<Pos2> = pts.iter().map(|v| c + *v * s).collect();
    // Two convex halves.
    painter.add(Shape::convex_polygon(
        vec![p[0], p[1], p[2], p[5]],
        GOLD_LIGHT,
        Stroke::NONE,
    ));
    painter.add(Shape::convex_polygon(
        vec![p[5], p[2], p[3], p[4]],
        GOLD_LIGHT,
        Stroke::NONE,
    ));
}

/// A karma star (level) in a disc.
pub fn star_icon(painter: &egui::Painter, c: Pos2, r: f32) {
    badge(painter, c, r, Color32::from_rgb(130, 80, 210));
    uikit::star(painter, c, r * 0.7, 0.0, GOLD_LIGHT);
}

fn badge(painter: &egui::Painter, c: Pos2, r: f32, fill: Color32) {
    painter.circle_filled(c + vec2(1.5, 2.0), r, Color32::from_black_alpha(130));
    painter.circle_filled(c, r, fill);
    painter.circle_filled(
        c - vec2(0.0, r * 0.35),
        r * 0.55,
        Color32::from_white_alpha(28),
    );
    painter.circle_stroke(c, r, Stroke::new(2.0_f32, GOLD));
}

/// The power tray's dashboard: a wide Googie panel with a fin rising from
/// the middle of its top edge, an atom spinning in the fin.
pub fn dashboard(painter: &egui::Painter, r: Rect, t: f32, still: bool) {
    let fin_w = 190.0;
    let fin_h = 30.0;
    let c = r.center().x;
    let fin = vec![
        pos2(c - fin_w * 0.5, r.top() + 4.0),
        pos2(c - fin_w * 0.28, r.top() - fin_h),
        pos2(c + fin_w * 0.28, r.top() - fin_h),
        pos2(c + fin_w * 0.5, r.top() + 4.0),
    ];
    painter.add(Shape::convex_polygon(
        fin.iter().map(|p| *p + vec2(5.0, 6.0)).collect(),
        Color32::from_black_alpha(150),
        Stroke::NONE,
    ));
    panel(painter, r, 18.0, 1.0, 4242);
    gradient_polygon(
        painter,
        &fin,
        Color32::from_rgb(44, 54, 120),
        Color32::from_rgb(34, 40, 96),
    );
    painter.add(Shape::line(fin.clone(), Stroke::new(3.0_f32, GOLD)));
    // Chrome strips either side of the fin, like a car's dash.
    for side in [-1.0_f32, 1.0] {
        let x0 = c + side * (fin_w * 0.5 + 8.0);
        let x1 = c + side * (r.width() * 0.5 - 40.0);
        let y = r.top() + 1.5;
        painter.line_segment([pos2(x0, y), pos2(x1, y)], Stroke::new(1.0_f32, GOLD_LIGHT));
    }
    let tt = if still { 0.0 } else { t };
    atom(
        painter,
        pos2(c, r.top() - fin_h * 0.4),
        20.0,
        tt,
        with_alpha(TEAL, 0.95),
        AETHER,
    );
    sputnik(
        painter,
        pos2(c - fin_w * 0.36, r.top() - 4.0),
        7.0,
        tt * 0.4,
        GOLD,
        AETHER,
    );
    sputnik(
        painter,
        pos2(c + fin_w * 0.36, r.top() - 4.0),
        7.0,
        -tt * 0.4,
        GOLD,
        AETHER,
    );
}

/// A compass strip: a long plate with pointed ends, like a hood badge.
pub fn badge_strip(painter: &egui::Painter, r: Rect) {
    let h = r.height();
    let pts = vec![
        pos2(r.left(), r.center().y),
        pos2(r.left() + h * 0.8, r.top()),
        pos2(r.right() - h * 0.8, r.top()),
        pos2(r.right(), r.center().y),
        pos2(r.right() - h * 0.8, r.bottom()),
        pos2(r.left() + h * 0.8, r.bottom()),
    ];
    painter.add(Shape::convex_polygon(
        pts.iter().map(|p| *p + vec2(3.0, 4.0)).collect(),
        Color32::from_black_alpha(130),
        Stroke::NONE,
    ));
    gradient_polygon(
        painter,
        &pts,
        Color32::from_rgb(40, 48, 108),
        Color32::from_rgb(16, 18, 48),
    );
    painter.add(Shape::closed_line(pts, Stroke::new(2.5_f32, GOLD)));
    // Teal pinstripes along the top and bottom.
    for y in [r.top() + 4.0, r.bottom() - 4.0] {
        painter.line_segment(
            [pos2(r.left() + h * 0.9, y), pos2(r.right() - h * 0.9, y)],
            Stroke::new(1.0_f32, with_alpha(TEAL, 0.6)),
        );
    }
}

/// The 1950s starburst clock round the minimap: gold rods, long ones with
/// a ball, all round the bezel.
pub fn starburst_ring(painter: &egui::Painter, c: Pos2, radius: f32) {
    for k in 0..24 {
        let a = k as f32 * TAU / 24.0 + TAU / 48.0;
        let d = vec2(a.sin(), -a.cos());
        let long = k % 2 == 0;
        let len = if long { 14.0 } else { 8.0 };
        let from = c + d * (radius + 6.0);
        painter.line_segment(
            [from + vec2(1.0, 1.5), from + d * len + vec2(1.0, 1.5)],
            Stroke::new(2.0_f32, Color32::from_black_alpha(110)),
        );
        painter.line_segment([from, from + d * len], Stroke::new(2.0_f32, GOLD));
        if long {
            painter.circle_filled(from + d * len, 2.6, GOLD_LIGHT);
        }
    }
}

/// Where a power tray slot's corners round: Googie leaves.
pub fn slot_corners() -> CornerRadius {
    leaf(11)
}

/// Text on the dashboard: Titan One with a drop shadow.
pub fn label(
    painter: &egui::Painter,
    ctx: &egui::Context,
    at: Pos2,
    align: Align2,
    text: &str,
    size: f32,
    col: Color32,
) {
    uikit::shadow_text(painter, at, align, text, uikit::ui_font(ctx, size), col);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_outlines_stay_in_their_rectangle() {
        let r = Rect::from_min_size(pos2(10.0, 20.0), vec2(300.0, 120.0));
        for p in outline(r, 24.0, 13.0) {
            assert!(r.expand(0.01).contains(p), "{p:?}");
        }
        // Tiny rectangles do not turn inside out.
        let small = Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 8.0));
        for p in outline(small, 24.0, 13.0) {
            assert!(small.expand(0.01).contains(p), "{p:?}");
        }
    }

    #[test]
    fn the_pattern_is_the_same_for_a_seed() {
        assert_eq!(hash(7, 3), hash(7, 3));
        assert_ne!(hash(7, 3), hash(8, 3));
    }
}
