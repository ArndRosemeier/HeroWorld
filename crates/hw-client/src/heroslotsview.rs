//! The hero slot cards on the title: twenty cards in a grid, icon first.
//! A filled card shows the origin's icon, the level on a star, the name and
//! when the hero was last played; the one picked also gets a play and a
//! bin button. An empty card is a plus: click it to make a hero there.
//! Drawing only: the title acts on what it returns (see `heroslots` for the
//! saving).

use crate::heroslots::{ago, Summary, SLOTS};
use crate::uikit::{mix, with_alpha, CREAM, DIM, GOLD, GOLD_LIGHT, HERO_RED, NAVY};
use engine::egui::{pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind};

pub const COLUMNS: usize = 5;
pub const CARD: engine::egui::Vec2 = engine::egui::Vec2::new(132.0, 112.0);
pub const GAP: f32 = 8.0;

/// What a click on the cards asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    /// Pick this hero.
    Select(usize),
    /// Pick this hero and go (double click, or the play button).
    Play(usize),
    /// Make a hero in this empty slot.
    New(usize),
    /// The bin: ask before deleting.
    Delete(usize),
}

/// The size the grid takes, for the card around it.
pub fn grid_size() -> engine::egui::Vec2 {
    let rows = SLOTS.div_ceil(COLUMNS) as f32;
    vec2(
        COLUMNS as f32 * CARD.x + (COLUMNS as f32 - 1.0) * GAP,
        rows * CARD.y + (rows - 1.0) * GAP,
    )
}

/// The twenty cards. `active`: the hero picked. `hover` is where the pointer
/// was last frame's card (for the title's hover sound): the slot it is on
/// now is written back.
pub fn grid(
    ui: &mut engine::egui::Ui,
    slots: &[Option<Summary>],
    active: Option<usize>,
    now: u64,
    hover: &mut Option<usize>,
) -> Action {
    let (area, _) = ui.allocate_exact_size(grid_size(), Sense::hover());
    let mut action = Action::None;
    let mut over = None;
    for slot in 0..SLOTS {
        let (col, row) = (slot % COLUMNS, slot / COLUMNS);
        let r = Rect::from_min_size(
            area.min + vec2(col as f32 * (CARD.x + GAP), row as f32 * (CARD.y + GAP)),
            CARD,
        );
        let id = ui.id().with(("hero slot", slot));
        let card = ui.interact(r, id, Sense::click());
        if card.hovered() {
            over = Some(slot);
        }
        let picked = active == Some(slot);
        match slots.get(slot).and_then(|s| s.as_ref()) {
            None => {
                draw_empty(ui.painter(), r, slot, card.hovered());
                if card.clicked() {
                    action = Action::New(slot);
                }
                card.on_hover_cursor(engine::egui::CursorIcon::PointingHand)
                    .on_hover_text("An empty slot: make a hero here");
            }
            Some(s) => {
                draw_hero(ui.painter(), r, slot, s, picked, card.hovered(), now);
                if card.clicked() {
                    action = Action::Select(slot);
                }
                if card.double_clicked() {
                    action = Action::Play(slot);
                }
                card.on_hover_cursor(engine::egui::CursorIcon::PointingHand);
                if picked {
                    let play_at = pos2(r.right() - 24.0, r.bottom() - 14.0);
                    let bin_at = pos2(r.left() + 24.0, r.bottom() - 14.0);
                    let play = ui.interact(
                        Rect::from_center_size(play_at, vec2(40.0, 26.0)),
                        id.with("play"),
                        Sense::click(),
                    );
                    let bin = ui.interact(
                        Rect::from_center_size(bin_at, vec2(30.0, 26.0)),
                        id.with("bin"),
                        Sense::click(),
                    );
                    play_button(ui.painter(), play_at, play.hovered());
                    bin_button(ui.painter(), bin_at, bin.hovered());
                    if play.clicked() {
                        action = Action::Play(slot);
                    }
                    if bin.clicked() {
                        action = Action::Delete(slot);
                    }
                    play.on_hover_cursor(engine::egui::CursorIcon::PointingHand)
                        .on_hover_text("Play this hero");
                    bin.on_hover_cursor(engine::egui::CursorIcon::PointingHand)
                        .on_hover_text("Delete this hero");
                }
            }
        }
    }
    *hover = over;
    action
}

fn draw_empty(p: &engine::egui::Painter, r: Rect, slot: usize, hot: bool) {
    let col = if hot {
        HERO_RED
    } else {
        with_alpha(NAVY, 0.35)
    };
    p.rect_filled(r, 12.0, with_alpha(NAVY, if hot { 0.10 } else { 0.05 }));
    // A dashed rim.
    let rim = r.shrink(1.5);
    let dash = 7.0;
    let corners = [
        rim.left_top(),
        rim.right_top(),
        rim.right_bottom(),
        rim.left_bottom(),
    ];
    for i in 0..4 {
        let (a, b) = (corners[i], corners[(i + 1) % 4]);
        let len = (b - a).length();
        let dir = (b - a) / len;
        let mut d = 8.0;
        while d + dash < len - 8.0 {
            p.line_segment(
                [a + dir * d, a + dir * (d + dash)],
                Stroke::new(2.0_f32, col),
            );
            d += dash * 2.0;
        }
    }
    let c = r.center() - vec2(0.0, 6.0);
    let arm = if hot { 17.0 } else { 14.0 };
    let w = Stroke::new(5.0_f32, col);
    p.line_segment([c - vec2(arm, 0.0), c + vec2(arm, 0.0)], w);
    p.line_segment([c - vec2(0.0, arm), c + vec2(0.0, arm)], w);
    p.text(
        pos2(r.center().x, r.bottom() - 16.0),
        Align2::CENTER_CENTER,
        format!("Slot {}", slot + 1),
        FontId::proportional(13.0),
        with_alpha(NAVY, 0.55),
    );
}

fn draw_hero(
    p: &engine::egui::Painter,
    r: Rect,
    slot: usize,
    s: &Summary,
    picked: bool,
    hot: bool,
    now: u64,
) {
    let fill = if picked {
        mix(NAVY, GOLD, 0.10)
    } else if hot {
        mix(NAVY, Color32::WHITE, 0.10)
    } else {
        NAVY
    };
    p.rect_filled(
        r.translate(vec2(1.5, 2.5)),
        12.0,
        Color32::from_black_alpha(60),
    );
    p.rect_filled(r, 12.0, fill);
    p.rect_stroke(
        r,
        12.0,
        Stroke::new(
            if picked { 3.5_f32 } else { 1.5 },
            if picked { GOLD } else { with_alpha(GOLD, 0.45) },
        ),
        StrokeKind::Inside,
    );
    // The origin's icon on the left, the level on a star on the right.
    origin_icon(p, s.origin, pos2(r.left() + 30.0, r.top() + 30.0), 17.0);
    if let Some(level) = s.level {
        let star = pos2(r.right() - 46.0, r.top() + 30.0);
        crate::googie::star_icon(p, star, 14.0);
        p.text(
            star + vec2(18.0, 1.0),
            Align2::LEFT_CENTER,
            level.to_string(),
            FontId::proportional(19.0),
            GOLD_LIGHT,
        );
    } else {
        // No origin yet: the hero is still being made.
        p.text(
            pos2(r.right() - 10.0, r.top() + 30.0),
            Align2::RIGHT_CENTER,
            "new",
            FontId::proportional(14.0),
            with_alpha(CREAM, 0.7),
        );
    }
    let name = fit(p, &s.name, 19.0, CARD.x - 16.0);
    p.text(
        pos2(r.center().x, r.top() + 56.0),
        Align2::CENTER_CENTER,
        name,
        FontId::proportional(19.0),
        CREAM,
    );
    p.text(
        pos2(r.center().x, r.top() + 73.0),
        Align2::CENTER_CENTER,
        ago(now, s.last_played),
        FontId::proportional(12.0),
        DIM,
    );
    if !picked {
        p.text(
            pos2(r.right() - 8.0, r.bottom() - 9.0),
            Align2::RIGHT_BOTTOM,
            format!("{}", slot + 1),
            FontId::proportional(11.0),
            with_alpha(CREAM, 0.35),
        );
    }
}

/// `text` cut with "..." until it is at most `width` wide.
fn fit(p: &engine::egui::Painter, text: &str, size: f32, width: f32) -> String {
    let wide = |t: &str| {
        p.layout_no_wrap(t.to_string(), FontId::proportional(size), Color32::WHITE)
            .size()
            .x
    };
    if wide(text) <= width {
        return text.to_string();
    }
    let mut cut: Vec<char> = text.chars().collect();
    while cut.len() > 1 {
        cut.pop();
        let t: String = cut.iter().collect::<String>() + "...";
        if wide(&t) <= width {
            return t;
        }
    }
    "...".into()
}

fn play_button(p: &engine::egui::Painter, c: Pos2, hot: bool) {
    let r = Rect::from_center_size(c, vec2(40.0, 26.0));
    let green = Color32::from_rgb(44, 160, 84);
    p.rect_filled(
        r,
        13.0,
        mix(green, Color32::WHITE, if hot { 0.2 } else { 0.0 }),
    );
    p.rect_stroke(r, 13.0, Stroke::new(2.0_f32, GOLD), StrokeKind::Inside);
    p.add(engine::egui::Shape::convex_polygon(
        vec![
            c + vec2(-5.0, -7.0),
            c + vec2(-5.0, 7.0),
            c + vec2(8.0, 0.0),
        ],
        CREAM,
        Stroke::NONE,
    ));
}

fn bin_button(p: &engine::egui::Painter, c: Pos2, hot: bool) {
    let r = Rect::from_center_size(c, vec2(30.0, 26.0));
    p.rect_filled(
        r,
        13.0,
        mix(HERO_RED, Color32::WHITE, if hot { 0.25 } else { 0.0 }),
    );
    p.rect_stroke(r, 13.0, Stroke::new(2.0_f32, GOLD), StrokeKind::Inside);
    // A bin: lid with a handle, tapering body.
    let w = Stroke::new(2.2_f32, CREAM);
    p.line_segment([c + vec2(-7.0, -5.0), c + vec2(7.0, -5.0)], w);
    p.line_segment([c + vec2(-2.5, -5.0), c + vec2(-2.5, -8.0)], w);
    p.line_segment([c + vec2(2.5, -5.0), c + vec2(2.5, -8.0)], w);
    p.line_segment([c + vec2(-2.5, -8.0), c + vec2(2.5, -8.0)], w);
    p.line_segment([c + vec2(-5.5, -3.0), c + vec2(-4.5, 7.0)], w);
    p.line_segment([c + vec2(5.5, -3.0), c + vec2(4.5, 7.0)], w);
    p.line_segment([c + vec2(-4.5, 7.0), c + vec2(4.5, 7.0)], w);
}

/// The icon of an origin (an index into `hw_sim::progress::ORIGINS`) in a
/// disc; a question mark for a hero who has none yet.
pub fn origin_icon(p: &engine::egui::Painter, origin: Option<u8>, c: Pos2, r: f32) {
    let tint = match origin {
        Some(0) => Color32::from_rgb(46, 170, 96),
        Some(1) => Color32::from_rgb(132, 84, 214),
        Some(2) => Color32::from_rgb(226, 120, 40),
        Some(3) => Color32::from_rgb(52, 118, 224),
        Some(_) => Color32::from_rgb(214, 54, 60),
        None => Color32::from_rgb(110, 116, 140),
    };
    p.circle_filled(c + vec2(1.2, 1.8), r, Color32::from_black_alpha(110));
    p.circle_filled(c, r, tint);
    p.circle_filled(
        c - vec2(0.0, r * 0.35),
        r * 0.55,
        Color32::from_white_alpha(28),
    );
    p.circle_stroke(c, r, Stroke::new(2.0_f32, GOLD));
    let s = r * 0.55;
    let fill = CREAM;
    let line = Stroke::new((r * 0.13).max(1.5), CREAM);
    let poly = |pts: Vec<Pos2>| {
        p.add(engine::egui::Shape::convex_polygon(pts, fill, Stroke::NONE));
    };
    match origin {
        // Lab Accident: a flask.
        Some(0) => {
            poly(vec![
                c + vec2(-s * 0.25, -s),
                c + vec2(s * 0.25, -s),
                c + vec2(s * 0.25, -s * 0.2),
                c + vec2(-s * 0.25, -s * 0.2),
            ]);
            poly(vec![
                c + vec2(-s * 0.25, -s * 0.2),
                c + vec2(s * 0.25, -s * 0.2),
                c + vec2(s * 0.9, s * 0.85),
                c + vec2(-s * 0.9, s * 0.85),
            ]);
            p.circle_filled(c + vec2(s * 0.1, s * 0.45), s * 0.2, tint);
        }
        // Ancient Relic: an amulet, a ring with a gem.
        Some(1) => {
            p.circle_stroke(c + vec2(0.0, -s * 0.45), s * 0.45, line);
            poly(vec![
                c + vec2(0.0, -s * 0.05),
                c + vec2(s * 0.55, s * 0.4),
                c + vec2(0.0, s * 0.95),
                c + vec2(-s * 0.55, s * 0.4),
            ]);
        }
        // Born Mighty: a fist, four knuckles over a palm.
        Some(2) => {
            for i in 0..4 {
                p.circle_filled(
                    c + vec2(-s * 0.72 + i as f32 * s * 0.48, -s * 0.4),
                    s * 0.27,
                    fill,
                );
            }
            p.rect_filled(
                Rect::from_center_size(c + vec2(0.0, s * 0.2), vec2(s * 1.8, s * 1.0)),
                s * 0.25,
                fill,
            );
        }
        // Sworn Guardian: a shield.
        Some(3) => {
            poly(vec![
                c + vec2(-s * 0.85, -s * 0.9),
                c + vec2(s * 0.85, -s * 0.9),
                c + vec2(s * 0.85, s * 0.1),
                c + vec2(0.0, s * 1.0),
                c + vec2(-s * 0.85, s * 0.1),
            ]);
            crate::uikit::star(p, c + vec2(0.0, -s * 0.1), s * 0.5, 0.0, tint);
        }
        // Masked Avenger: a domino mask.
        Some(_) => {
            p.rect_filled(
                Rect::from_center_size(c, vec2(s * 2.0, s * 1.05)),
                s * 0.5,
                fill,
            );
            p.circle_filled(c + vec2(-s * 0.5, 0.0), s * 0.22, tint);
            p.circle_filled(c + vec2(s * 0.5, 0.0), s * 0.22, tint);
        }
        None => {
            p.text(
                c,
                Align2::CENTER_CENTER,
                "?",
                FontId::proportional(r * 1.2),
                CREAM,
            );
        }
    }
}
