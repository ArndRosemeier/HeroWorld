//! The game's comic look for everything on screen: one place for the
//! colours, the fonts, the frames and the little motions, so the HUD, the
//! tray and every window read as parts of the same golden-age set.
//!
//! The style is the title screen's, taken indoors: deep navy enamel panels
//! with a gold chrome rim and a hard comic block shadow, cream lettering,
//! headings and buttons in the title's Titan One, gold buttons with a navy
//! rim, hero red for what is picked, Aether blue for what glows. Things
//! move a little: a glint runs round a window's rim now and then, the
//! Googie stars on its corners turn, gauges have stripes that roll and a
//! sheen that sweeps, and a low gauge throbs.
//!
//! - [`install`] sets egui's style for play (every `egui::Window`, button,
//!   tab, text field and tooltip takes it from there).
//! - [`adorn`] adds the stars and the glint to a window once it is drawn.
//! - [`panel`], [`gauge`], [`slot`], [`ribbon`] paint HUD pieces with an
//!   `egui::Painter`; [`card`] is the frame for small HUD boxes.

use engine::ui::egui::{
    self, epaint::CornerRadius, pos2, vec2, Align2, Color32, FontFamily, FontId, Margin, Mesh,
    Pos2, Rect, Shadow, Stroke, StrokeKind,
};

/// Deep navy: panels and the ink of the title.
pub const NAVY: Color32 = Color32::from_rgb(22, 26, 64);
/// A darker navy: wells, gauges' backs, text fields.
pub const DEEP: Color32 = Color32::from_rgb(12, 14, 38);
/// Cream: text and the title's cards.
pub const CREAM: Color32 = Color32::from_rgb(255, 246, 222);
/// Hero red: what is picked, alarms, the ribbons.
pub const HERO_RED: Color32 = Color32::from_rgb(226, 52, 48);
/// Gold chrome: rims, buttons, keys.
pub const GOLD: Color32 = Color32::from_rgb(255, 198, 64);
/// Pale gold: a rim's highlight, hovered buttons.
pub const GOLD_LIGHT: Color32 = Color32::from_rgb(255, 226, 140);
/// Dark gold: a rim's shade, pressed buttons.
pub const GOLD_DARK: Color32 = Color32::from_rgb(196, 132, 30);
/// Aether blue: what glows.
pub const AETHER: Color32 = Color32::from_rgb(120, 236, 255);
/// Muted cream, for small print.
pub const DIM: Color32 = Color32::from_rgb(196, 190, 176);

/// Panels are this opaque (the street shows through a little).
const PANEL_ALPHA: f32 = 0.93;
/// How far the comic block shadow falls.
const SHADOW: egui::Vec2 = vec2(5.0, 6.0);

/// The title's lettering (Titan One), once `title::install_fonts` ran;
/// the plain font before that.
pub fn ui_font(ctx: &egui::Context, size: f32) -> FontId {
    FontId::new(size, family(ctx, "hw-ui"))
}

/// The logo's lettering (Luckiest Guy): big comic words.
pub fn logo_font(ctx: &egui::Context, size: f32) -> FontId {
    FontId::new(size, family(ctx, "hw-logo"))
}

fn family(ctx: &egui::Context, name: &str) -> FontFamily {
    let f = FontFamily::Name(name.into());
    if ctx.fonts(|fonts| fonts.families().contains(&f)) {
        f
    } else {
        FontFamily::Proportional
    }
}

/// Seconds since the game started, for the motions.
pub fn now(ctx: &egui::Context) -> f32 {
    ctx.input(|i| i.time) as f32
}

pub fn with_alpha(c: Color32, a: f32) -> Color32 {
    let a = a.clamp(0.0, 1.0);
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (c.a() as f32 * a) as u8)
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(
        l(a.r(), b.r()),
        l(a.g(), b.g()),
        l(a.b(), b.b()),
        l(a.a(), b.a()),
    )
}

/// egui's style for play: every window, button and tab in the comic look.
/// Call once the fonts are in (it is cheap; calling it each frame is fine).
pub fn install(ctx: &egui::Context) {
    let ui = family(ctx, "hw-ui");
    ctx.style_mut(|s| {
        use egui::TextStyle;
        s.text_styles = [
            (TextStyle::Small, FontId::proportional(11.5)),
            (TextStyle::Body, FontId::proportional(14.5)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
            (TextStyle::Button, FontId::new(15.0, ui.clone())),
            (TextStyle::Heading, FontId::new(21.0, ui)),
        ]
        .into();
        s.spacing.button_padding = vec2(12.0, 5.0);
        s.spacing.item_spacing = vec2(8.0, 6.0);
        s.spacing.window_margin = Margin::symmetric(16, 12);
        s.spacing.scroll.bar_width = 8.0;
        s.visuals = visuals();
    });
}

fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.panel_fill = with_alpha(NAVY, PANEL_ALPHA);
    v.window_fill = with_alpha(NAVY, PANEL_ALPHA);
    v.window_stroke = Stroke::new(3.0_f32, GOLD);
    v.window_corner_radius = CornerRadius::same(14);
    v.window_shadow = Shadow {
        offset: [SHADOW.x as i8, SHADOW.y as i8],
        blur: 0,
        spread: 0,
        color: Color32::from_black_alpha(150),
    };
    v.popup_shadow = Shadow {
        offset: [3, 4],
        blur: 0,
        spread: 0,
        color: Color32::from_black_alpha(140),
    };
    v.menu_corner_radius = CornerRadius::same(10);
    v.faint_bg_color = Color32::from_rgb(30, 36, 82);
    v.extreme_bg_color = DEEP;
    v.code_bg_color = DEEP;
    v.hyperlink_color = AETHER;
    v.warn_fg_color = GOLD;
    v.error_fg_color = Color32::from_rgb(255, 110, 90);
    v.selection.bg_fill = HERO_RED;
    v.selection.stroke = Stroke::new(1.5_f32, CREAM);
    v.window_highlight_topmost = false;
    v.striped = false;
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;
    // Red buttons with cream lettering, like the title's main button; a
    // gold rim when the pointer is on one. Labels stay cream (egui takes a
    // label's colour from the same table).
    let w = &mut v.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, with_alpha(GOLD, 0.45));
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, CREAM);
    w.noninteractive.bg_fill = NAVY;
    w.noninteractive.weak_bg_fill = NAVY;
    for (state, fill, rim, text) in [
        (&mut w.inactive, Color32::from_rgb(196, 44, 42), DEEP, CREAM),
        (
            &mut w.hovered,
            Color32::from_rgb(236, 72, 60),
            GOLD,
            Color32::WHITE,
        ),
        (
            &mut w.active,
            Color32::from_rgb(160, 30, 30),
            GOLD_LIGHT,
            Color32::WHITE,
        ),
        (&mut w.open, Color32::from_rgb(160, 30, 30), GOLD, CREAM),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(2.0_f32, rim);
        state.fg_stroke = Stroke::new(1.5_f32, text);
        state.corner_radius = CornerRadius::same(9);
        state.expansion = 0.0;
    }
    w.hovered.expansion = 1.5;
    v
}

/// A small HUD box (a prompt, a status line): navy enamel, a gold rim, a
/// block shadow.
pub fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(with_alpha(NAVY, PANEL_ALPHA))
        .stroke(Stroke::new(2.0_f32, GOLD))
        .corner_radius(10.0)
        .inner_margin(Margin::symmetric(14, 8))
        .shadow(Shadow {
            offset: [4, 5],
            blur: 0,
            spread: 0,
            color: Color32::from_black_alpha(130),
        })
}

/// Like [`card`], with a red rim: something that wants the hero now.
pub fn alert_card() -> egui::Frame {
    card().stroke(Stroke::new(2.5_f32, HERO_RED))
}

/// A HUD panel painted by hand: block shadow, navy enamel with a soft sheen
/// at the top, a gold chrome rim with a pale inner line.
pub fn panel(painter: &egui::Painter, r: Rect, round: f32, alpha: f32) {
    let cr = CornerRadius::same(round as u8);
    painter.rect_filled(
        r.translate(SHADOW),
        cr,
        Color32::from_black_alpha((140.0 * alpha) as u8),
    );
    painter.rect_filled(r, cr, with_alpha(NAVY, PANEL_ALPHA * alpha));
    // A sheen across the top third, like enamel under a lamp.
    let sheen = Rect::from_min_max(
        r.min + vec2(3.0, 3.0),
        pos2(r.max.x - 3.0, r.min.y + r.height() * 0.38),
    );
    vertical_wash(
        painter,
        sheen,
        with_alpha(Color32::from_rgb(90, 110, 200), 0.22 * alpha),
        Color32::TRANSPARENT,
    );
    painter.rect_stroke(
        r,
        cr,
        Stroke::new(3.0_f32, with_alpha(GOLD, alpha)),
        StrokeKind::Inside,
    );
    painter.rect_stroke(
        r.shrink(3.5),
        CornerRadius::same((round - 3.0).max(0.0) as u8),
        Stroke::new(1.0_f32, with_alpha(GOLD_LIGHT, 0.35 * alpha)),
        StrokeKind::Inside,
    );
}

/// A rectangle going from `top` to `bottom` colour (square corners).
pub fn vertical_wash(painter: &egui::Painter, r: Rect, top: Color32, bottom: Color32) {
    let mut m = Mesh::default();
    m.colored_vertex(r.left_top(), top);
    m.colored_vertex(r.right_top(), top);
    m.colored_vertex(r.left_bottom(), bottom);
    m.colored_vertex(r.right_bottom(), bottom);
    m.add_triangle(0, 1, 2);
    m.add_triangle(2, 1, 3);
    painter.add(egui::Shape::mesh(m));
}

/// A gauge (health, energy, a villain's health): a dark well with a gold
/// rim, the fill lit from above with rolling stripes, a sheen sweeping
/// across now and then; below `low` (0..1) it throbs.
pub fn gauge(painter: &egui::Painter, r: Rect, value: f32, colour: Color32, t: f32, low: f32) {
    let v = value.clamp(0.0, 1.0);
    let h = r.height();
    let round = CornerRadius::same((h * 0.5).min(255.0) as u8);
    painter.rect_filled(r, round, DEEP);
    let fill = Rect::from_min_size(r.min, vec2(r.width() * v, h));
    if fill.width() > 1.0 {
        let throb = if v < low {
            0.5 + 0.5 * (t * 7.0).sin()
        } else {
            0.0
        };
        let face = mix(colour, Color32::WHITE, 0.25 * throb);
        painter.rect_filled(fill, round, face);
        let clip = painter.with_clip_rect(fill.intersect(painter.clip_rect()));
        // Lit from above: a pale band on the upper half.
        clip.rect_filled(
            Rect::from_min_max(
                fill.min + vec2(h * 0.3, h * 0.12),
                pos2(fill.max.x - h * 0.3, fill.min.y + h * 0.42),
            ),
            CornerRadius::same((h * 0.15) as u8),
            Color32::from_white_alpha(70),
        );
        // Stripes rolling along.
        let step = (h * 1.3).max(8.0);
        let mut x = fill.left() - h - step + (t * 18.0) % step;
        while x < fill.right() {
            clip.add(egui::Shape::convex_polygon(
                vec![
                    pos2(x, fill.bottom()),
                    pos2(x + h, fill.top()),
                    pos2(x + h + step * 0.4, fill.top()),
                    pos2(x + step * 0.4, fill.bottom()),
                ],
                Color32::from_white_alpha(26),
                Stroke::NONE,
            ));
            x += step;
        }
        // A sheen sweeping across every few seconds.
        let k = (t / 3.7).fract();
        if k < 0.35 {
            let sx = fill.left() + (k / 0.35) * (fill.width() + 40.0) - 20.0;
            clip.add(egui::Shape::convex_polygon(
                vec![
                    pos2(sx - 6.0, fill.bottom()),
                    pos2(sx + 4.0, fill.top()),
                    pos2(sx + 14.0, fill.top()),
                    pos2(sx + 4.0, fill.bottom()),
                ],
                Color32::from_white_alpha(90),
                Stroke::NONE,
            ));
        }
        if throb > 0.0 {
            painter.rect_stroke(
                r.expand(1.5 + 1.5 * throb),
                round,
                Stroke::new(2.0_f32, with_alpha(HERO_RED, 0.4 + 0.5 * throb)),
                StrokeKind::Outside,
            );
        }
    }
    painter.rect_stroke(
        r,
        round,
        Stroke::new(1.5_f32, GOLD_DARK),
        StrokeKind::Outside,
    );
}

/// A [`gauge`] laid out in a `Ui` like a progress bar (a held E, a search).
pub fn gauge_in(ui: &mut egui::Ui, width: f32, height: f32, value: f32, colour: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(width, height), egui::Sense::hover());
    let t = now(ui.ctx());
    gauge(ui.painter(), r.shrink(1.5), value, colour, t, 0.0);
}

/// How a tray slot looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotLook {
    /// Nothing in it.
    Empty,
    /// Ready to fire.
    Ready,
    /// Recharging or short of energy.
    Waiting,
    /// A toggle that is on.
    On,
}

/// A tray slot: a jukebox button. Gold chrome rim when it can fire, the
/// key in a red disc, a gloss across the top; `hover` lifts it, `recharge`
/// (0..1 left) darkens it from the top like a shutter. Returns the slot's
/// top left as drawn (lifted when hovered) and how far in the name starts.
/// `phase`: the clock, offset per slot so ready slots glint one by one.
pub fn slot(
    painter: &egui::Painter,
    r: Rect,
    key: &str,
    look: SlotLook,
    hover: bool,
    recharge: f32,
    phase: f32,
) -> (Pos2, f32) {
    let r = if hover {
        r.translate(vec2(0.0, -2.0))
    } else {
        r
    };
    let cr = CornerRadius::same(7);
    if look == SlotLook::Empty {
        painter.rect_filled(r, cr, with_alpha(DEEP, 0.6));
        painter.rect_stroke(
            r,
            cr,
            Stroke::new(1.0_f32, if hover { GOLD } else { with_alpha(GOLD, 0.25) }),
            StrokeKind::Inside,
        );
        let name_x = key_disc(
            painter,
            r,
            key,
            with_alpha(CREAM, 0.35),
            Color32::TRANSPARENT,
        );
        return (r.min, name_x);
    }
    painter.rect_filled(
        r.translate(vec2(2.0, 3.0)),
        cr,
        Color32::from_black_alpha(120),
    );
    let (top, bottom) = match look {
        SlotLook::On => (
            Color32::from_rgb(54, 150, 110),
            Color32::from_rgb(20, 80, 60),
        ),
        SlotLook::Ready => (
            Color32::from_rgb(64, 78, 160),
            Color32::from_rgb(26, 32, 84),
        ),
        _ => (Color32::from_rgb(54, 56, 72), Color32::from_rgb(30, 30, 40)),
    };
    let top = if hover {
        mix(top, Color32::WHITE, 0.15)
    } else {
        top
    };
    painter.rect_filled(r, cr, bottom);
    vertical_wash(
        painter,
        Rect::from_min_max(r.min + vec2(2.0, 2.0), pos2(r.max.x - 2.0, r.center().y)),
        top,
        bottom,
    );
    // Gloss.
    painter.rect_filled(
        Rect::from_min_max(
            r.min + vec2(4.0, 3.0),
            pos2(r.max.x - 4.0, r.min.y + r.height() * 0.28),
        ),
        CornerRadius::same(4),
        Color32::from_white_alpha(22),
    );
    if recharge > 0.0 {
        let cover = Rect::from_min_max(
            r.min,
            pos2(r.max.x, r.min.y + r.height() * recharge.clamp(0.0, 1.0)),
        );
        painter.rect_filled(cover, cr, Color32::from_black_alpha(150));
        // The shutter's edge.
        painter.line_segment(
            [
                pos2(r.min.x + 2.0, cover.max.y),
                pos2(r.max.x - 2.0, cover.max.y),
            ],
            Stroke::new(1.5_f32, with_alpha(AETHER, 0.7)),
        );
    }
    let rim = match look {
        SlotLook::Ready | SlotLook::On => GOLD,
        _ => Color32::from_gray(96),
    };
    painter.rect_stroke(
        r,
        cr,
        Stroke::new(
            if hover { 2.5_f32 } else { 2.0 },
            if hover { GOLD_LIGHT } else { rim },
        ),
        StrokeKind::Inside,
    );
    // A glint across ready slots now and then, each at its own moment.
    if look == SlotLook::Ready {
        let k = (phase / 5.0).fract();
        if k < 0.12 {
            let clip = painter.with_clip_rect(r.shrink(2.0).intersect(painter.clip_rect()));
            let x = r.left() + (k / 0.12) * (r.width() + 30.0) - 15.0;
            clip.add(egui::Shape::convex_polygon(
                vec![
                    pos2(x - 8.0, r.bottom()),
                    pos2(x + 6.0, r.top()),
                    pos2(x + 14.0, r.top()),
                    pos2(x, r.bottom()),
                ],
                Color32::from_white_alpha(60),
                Stroke::NONE,
            ));
        }
    }
    let disc = if look == SlotLook::Waiting {
        Color32::from_gray(80)
    } else {
        HERO_RED
    };
    let name_x = key_disc(painter, r, key, CREAM, disc);
    (r.min, name_x)
}

/// The key in a pill at the slot's top left; returns how far in from the
/// slot's left edge the name can start.
fn key_disc(painter: &egui::Painter, r: Rect, key: &str, text: Color32, fill: Color32) -> f32 {
    let galley = painter.layout_no_wrap(key.to_owned(), FontId::proportional(10.5), text);
    let w = (galley.size().x + 9.0).max(16.0);
    let k = Rect::from_min_size(r.min + vec2(4.0, 4.0), vec2(w, 16.0));
    if fill != Color32::TRANSPARENT {
        painter.rect_filled(k, CornerRadius::same(8), fill);
        painter.rect_stroke(
            k,
            CornerRadius::same(8),
            Stroke::new(1.0_f32, DEEP),
            StrokeKind::Outside,
        );
    }
    painter.galley(k.center() - galley.size() * 0.5, galley, text);
    k.right() - r.left() + 4.0
}

/// A red pennant with a heading on it, its tail notched: tops a HUD panel
/// (the target's name). Returns the pennant's rectangle.
pub fn ribbon(
    painter: &egui::Painter,
    ctx: &egui::Context,
    left_mid: Pos2,
    text: &str,
    size: f32,
    fill: Color32,
) -> Rect {
    let font = ui_font(ctx, size);
    let galley = painter.layout_no_wrap(text.to_owned(), font, CREAM);
    let h = size * 1.45;
    let w = galley.size().x + size * 1.6;
    let r = Rect::from_min_size(left_mid - vec2(0.0, h * 0.5), vec2(w, h));
    let notch = h * 0.35;
    let pts = vec![
        r.left_top(),
        r.right_top(),
        pos2(r.right() - notch, r.center().y),
        r.right_bottom(),
        r.left_bottom(),
    ];
    let shadow: Vec<Pos2> = pts.iter().map(|p| *p + vec2(3.0, 3.0)).collect();
    painter.add(egui::Shape::convex_polygon(
        shadow,
        Color32::from_black_alpha(130),
        Stroke::NONE,
    ));
    // The notch makes it concave: two halves.
    let c = r.center();
    painter.add(egui::Shape::convex_polygon(
        vec![
            r.left_top(),
            r.right_top(),
            pos2(r.right() - notch, c.y),
            pos2(r.left(), c.y),
        ],
        fill,
        Stroke::NONE,
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![
            pos2(r.left(), c.y),
            pos2(r.right() - notch, c.y),
            r.right_bottom(),
            r.left_bottom(),
        ],
        fill,
        Stroke::NONE,
    ));
    painter.add(egui::Shape::closed_line(pts, Stroke::new(2.0_f32, DEEP)));
    let text_at = pos2(r.left() + size * 0.55, c.y);
    painter.galley(
        text_at - vec2(0.0, galley.size().y * 0.5) + vec2(1.5, 1.5),
        painter.layout_no_wrap(
            text.to_owned(),
            ui_font(ctx, size),
            Color32::from_black_alpha(160),
        ),
        Color32::BLACK,
    );
    painter.galley(text_at - vec2(0.0, galley.size().y * 0.5), galley, CREAM);
    r
}

/// A four-pointed Googie star.
pub fn star(painter: &egui::Painter, p: Pos2, size: f32, turn: f32, col: Color32) {
    let (s, c) = turn.sin_cos();
    let at = |dx: f32, dy: f32| p + vec2(dx * c - dy * s, dx * s + dy * c);
    let thin = size * 0.22;
    for (a, b) in [((size, 0.0), (0.0, thin)), ((0.0, size), (thin, 0.0))] {
        painter.add(egui::Shape::convex_polygon(
            vec![at(a.0, a.1), at(b.0, b.1), at(-a.0, -a.1), at(-b.0, -b.1)],
            col,
            Stroke::NONE,
        ));
    }
}

/// The finishing touches on a window just drawn (pass its rectangle): a
/// Googie star turning on two corners and a glint running along the top rim
/// every few seconds. Drawn on a layer above the window.
pub fn adorn(ctx: &egui::Context, id: &str, r: Rect) {
    let t = now(ctx);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new(("adorn", id)),
    ));
    let still = reduced_motion(ctx);
    let turn = if still { 0.3 } else { t * 0.6 };
    star(
        &painter,
        r.left_top() + vec2(6.0, 6.0),
        11.0,
        turn,
        GOLD_LIGHT,
    );
    star(
        &painter,
        r.right_bottom() - vec2(6.0, 6.0),
        9.0,
        -turn * 1.3,
        AETHER,
    );
    if still {
        return;
    }
    let k = (t / 6.0 + hash(id) * 0.7).fract();
    if k < 0.18 {
        let x = r.left() + 14.0 + (k / 0.18) * (r.width() - 28.0);
        let y = r.top() + 1.5;
        painter.add(egui::Shape::convex_polygon(
            vec![
                pos2(x - 22.0, y),
                pos2(x, y - 1.6),
                pos2(x + 22.0, y),
                pos2(x, y + 1.6),
            ],
            with_alpha(Color32::WHITE, 0.9),
            Stroke::NONE,
        ));
        star(&painter, pos2(x, y), 6.0, t * 4.0, Color32::WHITE);
    }
}

fn hash(s: &str) -> f32 {
    let mut h: u32 = 2166136261;
    for b in s.bytes() {
        h = (h ^ b as u32).wrapping_mul(16777619);
    }
    (h % 1000) as f32 / 1000.0
}

/// The player asked for less motion (the title's setting): the stars stand
/// still and nothing glints.
pub fn reduced_motion(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(egui::Id::new("hw reduced motion")))
        .unwrap_or(false)
}

pub fn set_reduced_motion(ctx: &egui::Context, on: bool) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("hw reduced motion"), on));
}

/// A comic word over the screen, with a block shadow ("You are down!").
pub fn shout(
    painter: &egui::Painter,
    ctx: &egui::Context,
    at: Pos2,
    text: &str,
    size: f32,
    col: Color32,
) {
    let font = logo_font(ctx, size);
    let o = (size * 0.08).max(2.0);
    painter.text(
        at + vec2(o, o),
        Align2::CENTER_CENTER,
        text,
        font.clone(),
        DEEP,
    );
    painter.text(at, Align2::CENTER_CENTER, text, font, col);
}

/// One line of text cut to `max_width` with an ellipsis (a power's name on
/// its tray slot).
pub fn fit_text(
    painter: &egui::Painter,
    at: Pos2,
    text: &str,
    font: FontId,
    col: Color32,
    max_width: f32,
) {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, col);
    job.wrap = egui::text::TextWrapping {
        max_width: max_width.max(1.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let galley = painter.layout_job(job);
    painter.galley(at, galley, col);
}

/// Text with a soft drop shadow, readable on any sky.
pub fn shadow_text(
    painter: &egui::Painter,
    at: Pos2,
    align: Align2,
    text: &str,
    font: FontId,
    col: Color32,
) {
    let o = (font.size * 0.08).max(1.2);
    painter.text(
        at + vec2(o, o),
        align,
        text,
        font.clone(),
        Color32::from_black_alpha((180.0 * col.a() as f32 / 255.0) as u8),
    );
    painter.text(at, align, text, font, col);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixing_runs_from_one_colour_to_the_other() {
        assert_eq!(mix(NAVY, GOLD, 0.0), NAVY);
        assert_eq!(mix(NAVY, GOLD, 1.0), GOLD);
        assert_eq!(with_alpha(GOLD, 0.0).a(), 0);
    }

    #[test]
    fn the_style_reads_cream_on_navy() {
        let v = visuals();
        assert_eq!(v.widgets.noninteractive.text_color(), CREAM);
        assert_eq!(v.widgets.inactive.text_color(), CREAM);
        assert_eq!(v.window_stroke.color, GOLD);
    }
}
