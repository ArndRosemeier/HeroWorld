//! Where the player put the screen's pieces: the minimap, the compass, the
//! power tray, the wrist radio, the police monitor, and the windows (chat,
//! powers, police computer, debug menu).
//!
//! Two kinds of thing:
//!
//! - **HUD pieces** are drawn where the game says, then moved and scaled
//!   as a whole by an egui layer transform ([`apply`]): the piece itself
//!   needs no code for it. Each has a key, an anchor point (the point of
//!   its own box that stays put when it is scaled) and the player's offset
//!   and scale. "Arrange the screen" (Escape menu) shows a gold frame
//!   round each piece: drag it to move, drag its corner to resize
//!   ([`overlay`]), and a bar with Done and Reset ([`edit_bar`]).
//! - **Windows** are plain egui windows that can be dragged (and some
//!   resized) at any time; [`window`] gives them the saved place and
//!   [`remember`] saves where they ended up.
//!
//! Everything is saved to `ui_layout.json` beside the hero's profile, so it
//! comes back next time. Offsets are from the piece's default place, so a
//! different window size keeps the layout sensible; a piece can never be
//! dragged completely off screen. Reset deletes the file's contents and
//! puts everything back.

use crate::{googie, profile::Profile, uikit};
use engine::ui::egui::{self, emath::TSTransform, Align2, Id, LayerId, Order, Pos2, Rect, Sense};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// Smallest and largest size of a piece, against its default.
pub const MIN_SCALE: f32 = 0.5;
pub const MAX_SCALE: f32 = 2.5;
/// How much of a piece must stay on screen (points).
const KEEP: f32 = 44.0;

/// One HUD piece's setting: how far it was moved (points) and its size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Item {
    pub dx: f32,
    pub dy: f32,
    pub scale: f32,
}

impl Default for Item {
    fn default() -> Self {
        Item {
            dx: 0.0,
            dy: 0.0,
            scale: 1.0,
        }
    }
}

/// A window's saved place: its pivot point and, if it was resized, its size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    pub at: Pos2,
    pub size: Option<egui::Vec2>,
}

#[derive(Default)]
struct State {
    loaded: bool,
    editing: bool,
    items: BTreeMap<String, Item>,
    windows: BTreeMap<String, Spot>,
    /// Windows' places as last seen this session.
    seen: BTreeMap<String, Spot>,
}

static STATE: Mutex<State> = Mutex::new(State {
    loaded: false,
    editing: false,
    items: BTreeMap::new(),
    windows: BTreeMap::new(),
    seen: BTreeMap::new(),
});

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if !s.loaded {
        s.loaded = true;
        if let Some(text) = path().and_then(|p| std::fs::read_to_string(p).ok()) {
            parse(&text, &mut s);
        }
    }
    f(&mut s)
}

fn path() -> Option<PathBuf> {
    Profile::dir().map(|d| d.join("ui_layout.json"))
}

fn num(v: &Value, k: &str) -> Option<f32> {
    v.get(k)
        .and_then(Value::as_f64)
        .map(|x| x as f32)
        .filter(|x| x.is_finite())
}

/// Reads a saved layout. Anything odd is skipped: a layout file must never
/// stop the game from starting.
fn parse(text: &str, s: &mut State) {
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        return;
    };
    if let Some(items) = v.get("items").and_then(Value::as_object) {
        for (k, i) in items {
            let item = Item {
                dx: num(i, "dx").unwrap_or(0.0).clamp(-8000.0, 8000.0),
                dy: num(i, "dy").unwrap_or(0.0).clamp(-8000.0, 8000.0),
                scale: num(i, "scale").unwrap_or(1.0).clamp(MIN_SCALE, MAX_SCALE),
            };
            s.items.insert(k.clone(), item);
        }
    }
    if let Some(ws) = v.get("windows").and_then(Value::as_object) {
        for (k, w) in ws {
            let (Some(x), Some(y)) = (num(w, "x"), num(w, "y")) else {
                continue;
            };
            let size = match (num(w, "w"), num(w, "h")) {
                (Some(w), Some(h)) if w >= 100.0 && h >= 60.0 => Some(egui::vec2(w, h)),
                _ => None,
            };
            s.windows.insert(
                k.clone(),
                Spot {
                    at: egui::pos2(x, y),
                    size,
                },
            );
        }
    }
}

fn to_text(s: &State) -> String {
    let items: serde_json::Map<String, Value> = s
        .items
        .iter()
        .map(|(k, i)| (k.clone(), json!({"dx": i.dx, "dy": i.dy, "scale": i.scale})))
        .collect();
    let windows: serde_json::Map<String, Value> = s
        .windows
        .iter()
        .map(|(k, w)| {
            let mut o = json!({"x": w.at.x, "y": w.at.y});
            if let Some(z) = w.size {
                o["w"] = json!(z.x);
                o["h"] = json!(z.y);
            }
            (k.clone(), o)
        })
        .collect();
    serde_json::to_string_pretty(&json!({"items": items, "windows": windows})).unwrap_or_default()
}

/// Writes the layout (a few hundred bytes, only when something was set down
/// or reset, so it is not worth a thread).
fn save(s: &State) {
    let Some(p) = path() else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&p, to_text(s)) {
        eprintln!("could not save the screen layout: {e}");
    }
}

// ---------------------------------------------------------------- edit mode

/// True while the player is arranging the screen.
pub fn editing() -> bool {
    with(|s| s.editing)
}

pub fn set_editing(on: bool) {
    with(|s| s.editing = on);
}

/// Puts every piece and window back where the game had them.
pub fn reset(ctx: &egui::Context) {
    with(|s| {
        s.items.clear();
        s.windows.clear();
        s.seen.clear();
        save(s);
    });
    // Windows forget where they were dragged to (and take their default).
    ctx.memory_mut(|m| m.reset_areas());
}

/// The saved setting of a HUD piece (the default when never touched).
pub fn item(key: &str) -> Item {
    with(|s| s.items.get(key).copied().unwrap_or_default())
}

fn set_item(key: &str, it: Item) {
    with(|s| {
        if it == Item::default() {
            s.items.remove(key);
        } else {
            s.items.insert(key.to_string(), it);
        }
    });
}

/// The transform of a piece: scaled about `anchor`, then moved.
pub fn transform_of(it: Item, anchor: Pos2) -> TSTransform {
    let s = it.scale.clamp(MIN_SCALE, MAX_SCALE);
    TSTransform::new(
        anchor.to_vec2() + egui::vec2(it.dx, it.dy) - anchor.to_vec2() * s,
        s,
    )
}

pub fn transform(key: &str, anchor: Pos2) -> TSTransform {
    transform_of(item(key), anchor)
}

/// Moves and scales the egui layers a piece is drawn on. Call before they
/// are drawn, every frame. Returns the transform (default place to where
/// it is now) for what needs it in numbers.
pub fn apply(ctx: &egui::Context, key: &str, anchor: Pos2, layers: &[LayerId]) -> TSTransform {
    let tf = transform(key, anchor);
    for l in layers {
        ctx.set_transform_layer(*l, tf);
    }
    tf
}

/// Keeps `it` so that at least [`KEEP`] points of `local` (as it would then
/// be) show on screen.
fn on_screen(mut it: Item, anchor: Pos2, local: Rect, screen: Rect) -> Item {
    let g = transform_of(it, anchor).mul_rect(local);
    let keep = KEEP.min(g.width()).min(g.height());
    let push_right = (screen.left() + keep - g.right()).max(0.0);
    let push_left = (g.left() - (screen.right() - keep)).max(0.0);
    let push_down = (screen.top() + keep - g.bottom()).max(0.0);
    let push_up = (g.top() - (screen.bottom() - keep)).max(0.0);
    it.dx += push_right - push_left;
    it.dy += push_down - push_up;
    it
}

/// While arranging: the gold frame round a piece, with its move and resize
/// handles. `local` is the piece's box where the game draws it (before the
/// transform), `anchor` the same point given to [`apply`].
pub fn overlay(ctx: &egui::Context, key: &str, name: &str, anchor: Pos2, local: Rect) {
    if !editing() {
        return;
    }
    let screen = ctx.screen_rect();
    let mut it = item(key);
    let g = transform_of(it, anchor).mul_rect(local);
    let order = Order::Foreground;
    // The body: drag it anywhere to move the piece.
    let body = egui::Area::new(Id::new(("layout move", key)))
        .order(order)
        .fixed_pos(g.min)
        .show(ctx, |ui| {
            let (r, resp) = ui.allocate_exact_size(g.size(), Sense::drag());
            let hot = resp.hovered() || resp.dragged();
            let p = ui.painter();
            p.rect_filled(
                r,
                6.0,
                uikit::with_alpha(uikit::GOLD, if hot { 0.22 } else { 0.1 }),
            );
            p.rect_stroke(
                r,
                6.0,
                egui::Stroke::new(if hot { 2.5_f32 } else { 1.8_f32 }, uikit::GOLD),
                egui::StrokeKind::Inside,
            );
            // The move glyph, top left: four little arrows.
            let c = r.min + egui::vec2(15.0, 15.0).min(r.size() * 0.5);
            p.circle_filled(c, 11.0, uikit::NAVY);
            p.circle_stroke(c, 11.0, egui::Stroke::new(1.5_f32, uikit::GOLD));
            for d in [egui::Vec2::X, egui::Vec2::Y, -egui::Vec2::X, -egui::Vec2::Y] {
                let tip = c + d * 7.5;
                let side = egui::vec2(-d.y, d.x);
                p.line_segment([c, tip], egui::Stroke::new(1.4_f32, uikit::CREAM));
                p.line_segment(
                    [tip, tip - d * 3.0 + side * 2.4],
                    egui::Stroke::new(1.4_f32, uikit::CREAM),
                );
                p.line_segment(
                    [tip, tip - d * 3.0 - side * 2.4],
                    egui::Stroke::new(1.4_f32, uikit::CREAM),
                );
            }
            resp.on_hover_cursor(egui::CursorIcon::Grab)
                .on_hover_text(name)
        })
        .inner;
    let mut moved = false;
    if body.dragged() {
        let d = body.drag_delta();
        it.dx += d.x;
        it.dy += d.y;
        moved = true;
    }
    // The corner: drag it to resize; the top left corner stays where it is.
    let corner = Rect::from_min_size(
        g.right_bottom() - egui::vec2(26.0, 26.0),
        egui::vec2(26.0, 26.0),
    );
    let grip = egui::Area::new(Id::new(("layout size", key)))
        .order(order)
        .fixed_pos(corner.min)
        .show(ctx, |ui| {
            let (r, resp) = ui.allocate_exact_size(corner.size(), Sense::drag());
            let p = ui.painter();
            let hot = resp.hovered() || resp.dragged();
            let tri = vec![r.right_top(), r.right_bottom(), r.left_bottom()];
            p.add(egui::Shape::convex_polygon(
                tri,
                if hot { uikit::GOLD_LIGHT } else { uikit::GOLD },
                egui::Stroke::new(1.5_f32, uikit::DEEP),
            ));
            for k in 1..=2 {
                let o = k as f32 * 5.0;
                p.line_segment(
                    [
                        r.right_bottom() - egui::vec2(o + 3.0, 3.0),
                        r.right_bottom() - egui::vec2(3.0, o + 3.0),
                    ],
                    egui::Stroke::new(1.5_f32, uikit::DEEP),
                );
            }
            resp.on_hover_cursor(egui::CursorIcon::ResizeNwSe)
                .on_hover_text("Resize")
        })
        .inner;
    if grip.dragged() {
        if let Some(at) = ctx.pointer_interact_pos() {
            // The new size is where the pointer is, measured from the
            // top left corner; the larger of the two ratios, so a diagonal
            // drag feels even.
            let sx = (at.x - g.min.x) / local.width().max(1.0);
            let sy = (at.y - g.min.y) / local.height().max(1.0);
            let s = sx.max(sy).clamp(MIN_SCALE, MAX_SCALE);
            // Keep the top left corner of the piece where it is.
            let a = anchor.to_vec2();
            it.scale = s;
            it.dx = g.min.x - a.x - s * (local.min.x - a.x);
            it.dy = g.min.y - a.y - s * (local.min.y - a.y);
            moved = true;
        }
    }
    if moved {
        it = on_screen(it, anchor, local, screen);
        set_item(key, it);
    }
    if body.drag_stopped() || grip.drag_stopped() {
        with(|s| save(s));
    }
}

/// While arranging: the bar with Done and Reset, top middle.
pub fn edit_bar(ctx: &egui::Context) {
    if !editing() {
        return;
    }
    let mut done = false;
    let mut reset_all = false;
    egui::Area::new(Id::new("layout bar"))
        .order(Order::Foreground)
        .anchor(Align2::CENTER_TOP, egui::vec2(0.0, 52.0))
        .show(ctx, |ui| {
            googie::plate(ui, false, |ui| {
                ui.horizontal(|ui| {
                    // Four-arrow glyph: what this mode is.
                    let (r, _) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), Sense::hover());
                    let p = ui.painter();
                    for d in [egui::Vec2::X, egui::Vec2::Y, -egui::Vec2::X, -egui::Vec2::Y] {
                        let tip = r.center() + d * 11.0;
                        let side = egui::vec2(-d.y, d.x);
                        p.line_segment([r.center(), tip], egui::Stroke::new(2.0_f32, uikit::GOLD));
                        p.line_segment(
                            [tip, tip - d * 5.0 + side * 4.0],
                            egui::Stroke::new(2.0_f32, uikit::GOLD),
                        );
                        p.line_segment(
                            [tip, tip - d * 5.0 - side * 4.0],
                            egui::Stroke::new(2.0_f32, uikit::GOLD),
                        );
                    }
                    reset_all = icon_button(ui, Glyph::Reset, "Put everything back where it was");
                    done = icon_button(ui, Glyph::Check, "Done");
                });
            });
        });
    if reset_all {
        reset(ctx);
    }
    if done || ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
        set_editing(false);
        with(|s| save(s));
    }
}

#[derive(Clone, Copy)]
enum Glyph {
    Check,
    Reset,
}

/// A round button with a drawn glyph (no words); true when clicked.
fn icon_button(ui: &mut egui::Ui, glyph: Glyph, tip: &str) -> bool {
    let (r, resp) = ui.allocate_exact_size(egui::vec2(34.0, 34.0), Sense::click());
    let hot = resp.hovered();
    let p = ui.painter();
    let c = r.center();
    p.circle_filled(
        c,
        16.0,
        match glyph {
            Glyph::Check => uikit::mix(
                egui::Color32::from_rgb(40, 150, 80),
                egui::Color32::WHITE,
                if hot { 0.2 } else { 0.0 },
            ),
            Glyph::Reset => uikit::mix(
                uikit::HERO_RED,
                egui::Color32::WHITE,
                if hot { 0.2 } else { 0.0 },
            ),
        },
    );
    p.circle_stroke(c, 16.0, egui::Stroke::new(2.0_f32, uikit::GOLD));
    let w = egui::Stroke::new(3.0_f32, uikit::CREAM);
    match glyph {
        Glyph::Check => {
            p.line_segment([c + egui::vec2(-7.0, 0.0), c + egui::vec2(-2.0, 6.0)], w);
            p.line_segment([c + egui::vec2(-2.0, 6.0), c + egui::vec2(8.0, -6.0)], w);
        }
        Glyph::Reset => {
            // An open circle with an arrow head: "again".
            let n = 14;
            let pts: Vec<Pos2> = (0..=n)
                .map(|k| {
                    let a = -1.2 + k as f32 / n as f32 * 4.9;
                    c + egui::vec2(a.cos(), a.sin()) * 8.0
                })
                .collect();
            p.add(egui::Shape::line(pts.clone(), w));
            let tip = pts[0];
            p.line_segment([tip, tip + egui::vec2(-7.0, -1.0)], w);
            p.line_segment([tip, tip + egui::vec2(-1.0, 7.0)], w);
        }
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(tip)
        .clicked()
}

// ------------------------------------------------------------------ windows

/// The window `w` at its saved place, else with its pivot at `fallback`.
/// `pivot` is the point of the window that `fallback` and the saved place
/// name (the middle for a centred window, the bottom left for the chat).
pub fn window<'a>(
    w: egui::Window<'a>,
    key: &str,
    pivot: Align2,
    fallback: Pos2,
) -> egui::Window<'a> {
    let saved = with(|s| s.windows.get(key).copied());
    let at = saved.map_or(fallback, |s| s.at);
    w.pivot(pivot).default_pos(at).movable(true)
}

/// A resizable window's saved size, if it was resized (give it to
/// `default_size`, after the window's other settings).
pub fn size_of(key: &str) -> Option<egui::Vec2> {
    with(|s| s.windows.get(key).and_then(|w| w.size))
}

/// Saves where a window ended up (after it was shown). `rect` is its
/// response's rect. Written when it was moved or resized and the pointer is
/// up again.
pub fn remember(ctx: &egui::Context, key: &str, pivot: Align2, rect: Rect, resizable: bool) {
    if ctx.input(|i| i.pointer.any_down()) {
        return;
    }
    let at = pivot.pos_in_rect(&rect);
    let size = resizable.then(|| rect.size());
    let now = Spot { at, size };
    with(|s| {
        // Where it was last seen this session, else its saved place. The
        // first sight of a window in its default place is no setting: only
        // a window that was moved is kept.
        let prev = s.seen.get(key).or_else(|| s.windows.get(key)).copied();
        let Some(old) = prev else {
            s.seen.insert(key.to_string(), now);
            return;
        };
        let differs = (old.at - at).length() > 0.5
            || match (old.size, size) {
                (Some(a), Some(b)) => (a - b).length() > 0.5,
                (None, None) => false,
                _ => true,
            };
        if differs {
            s.seen.insert(key.to_string(), now);
            s.windows.insert(key.to_string(), now);
            save(s);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen() -> Rect {
        Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 720.0))
    }

    #[test]
    fn the_default_is_no_change() {
        let a = egui::pos2(1200.0, 700.0);
        let tf = transform_of(Item::default(), a);
        let p = egui::pos2(1100.0, 650.0);
        assert!((tf.mul_pos(p) - p).length() < 1e-3);
    }

    #[test]
    fn scaling_keeps_the_anchor_still() {
        let a = egui::pos2(1200.0, 700.0);
        let it = Item {
            dx: 0.0,
            dy: 0.0,
            scale: 2.0,
        };
        let tf = transform_of(it, a);
        assert!((tf.mul_pos(a) - a).length() < 1e-3);
        let p = egui::pos2(1100.0, 650.0);
        assert!((tf.mul_pos(p) - egui::pos2(1000.0, 600.0)).length() < 1e-3);
        // And a move shifts everything, the anchor too.
        let moved = transform_of(
            Item {
                dx: -30.0,
                dy: 10.0,
                ..it
            },
            a,
        );
        assert!((moved.mul_pos(a) - (a + egui::vec2(-30.0, 10.0))).length() < 1e-3);
    }

    #[test]
    fn a_piece_cannot_be_dragged_off_the_screen() {
        let a = egui::pos2(100.0, 100.0);
        let local = Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(180.0, 180.0));
        for (dx, dy) in [(9000.0, 0.0), (-9000.0, 0.0), (0.0, 9000.0), (0.0, -9000.0)] {
            let it = on_screen(Item { dx, dy, scale: 1.0 }, a, local, screen());
            let g = transform_of(it, a).mul_rect(local);
            let seen = g.intersect(screen());
            assert!(
                seen.width() >= KEEP - 0.5 && seen.height() >= KEEP - 0.5,
                "{dx} {dy}: {g:?}"
            );
        }
        // A piece already on screen is left alone.
        let ok = Item {
            dx: 20.0,
            dy: 30.0,
            scale: 1.3,
        };
        assert_eq!(on_screen(ok, a, local, screen()), ok);
    }

    #[test]
    fn a_saved_layout_reads_back_and_bad_files_are_ignored() {
        let mut s = State::default();
        s.items.insert(
            "minimap".into(),
            Item {
                dx: -40.0,
                dy: -12.5,
                scale: 1.5,
            },
        );
        s.windows.insert(
            "chat".into(),
            Spot {
                at: egui::pos2(10.0, 600.0),
                size: Some(egui::vec2(400.0, 200.0)),
            },
        );
        let text = to_text(&s);
        let mut back = State::default();
        parse(&text, &mut back);
        assert_eq!(back.items, s.items);
        assert_eq!(back.windows, s.windows);
        // Garbage must not stop the game: nothing is read, nothing panics.
        let mut none = State::default();
        parse("{ not json", &mut none);
        parse(
            r#"{"items": {"a": {"scale": 99.0, "dx": 1e30}}, "windows": {"w": {"x": 1}}}"#,
            &mut none,
        );
        assert_eq!(none.items["a"].scale, MAX_SCALE);
        assert_eq!(none.items["a"].dx, 8000.0);
        assert!(none.windows.is_empty());
    }
}
