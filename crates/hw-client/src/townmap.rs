//! The town map (M): the whole screen, north up, with the minimap's detail
//! (houses, streets, parks, water and the villains' quarter, from the same
//! `mapraster.rs` tiles), the landmarks with their names, every call on the
//! radio, the story's star, your friends and you. Drag it with the mouse to
//! look elsewhere; the wheel zooms round the pointer.
//!
//! Only the tiles in view are made, by two worker threads, nearest the
//! middle of the view first; asks that scroll out of view before a worker
//! reaches them are dropped. Each tile becomes its own texture, so panning
//! and zooming redraw nothing: the main thread only places finished
//! pictures. At most [`MAX_TILES`] are kept (the farthest from the view go
//! first), so a megacity's map costs no more than a village's.

use crate::dispatchview::{diamond, star};
use crate::fightview::con_colour;
use crate::mapraster::{self, TileKey, LEVEL_SPAN, TILE_PX};
use crate::radioview::Radio;
use engine::ui::egui;
use glam::{DVec2, Vec3};
use hw_gen::urban::landmarks::LandmarkKind;
use hw_gen::urban::world::TownWorld;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Condvar, Mutex};

/// Tiles kept at most (a 64 KB texture each).
pub const MAX_TILES: usize = 320;
/// Closest zoom: metres a screen point shows.
const MIN_MPP: f64 = 0.35;
/// Zoom when the map first opens: houses show.
const START_MPP: f64 = 0.7;
/// Finished tiles turned into textures in one frame at most.
const UPLOADS_PER_FRAME: usize = 24;
/// Gap round the map to the window's edges (points).
const MARGIN: f32 = 28.0;

/// Work for the tile makers: the tiles still to make, the nearest last
/// (so `pop` takes it), and whether to stop.
#[derive(Default)]
struct Queue {
    todo: Vec<TileKey>,
    stop: bool,
}

type Shared = Arc<(Mutex<Queue>, Condvar)>;

/// The map window's state, its tiles and its workers.
pub struct TownMap {
    pub open: bool,
    /// The middle of the view (m on the ground plan) and its zoom (m a point).
    centre: DVec2,
    mpp: f64,
    tiles: BTreeMap<TileKey, egui::TextureHandle>,
    /// Asked for and not back yet (queued or being made).
    asked: BTreeSet<TileKey>,
    queue: Shared,
    made: Receiver<(TileKey, egui::ColorImage)>,
    /// The pointer was locked when the map opened: lock it again after.
    pub relock: bool,
    /// Not yet centred on the hero since it opened.
    to_centre: bool,
}

impl TownMap {
    pub fn new(town: Arc<TownWorld>) -> Self {
        let queue: Shared = Arc::default();
        let (done, made) = channel();
        for n in 0..2 {
            let (queue, town, done) = (queue.clone(), town.clone(), done.clone());
            std::thread::Builder::new()
                .name(format!("townmap-{n}"))
                .spawn(move || {
                    engine::parallel::background_thread();
                    loop {
                        let key = {
                            let (lock, ready) = &*queue;
                            let mut q = lock.lock().expect("town map queue");
                            loop {
                                if q.stop {
                                    return;
                                }
                                if let Some(k) = q.todo.pop() {
                                    break k;
                                }
                                q = ready.wait(q).expect("town map queue");
                            }
                        };
                        let img = picture(&mapraster::raster(&town, key));
                        if done.send((key, img)).is_err() {
                            return;
                        }
                    }
                })
                .expect("town map thread");
        }
        Self {
            open: false,
            centre: DVec2::ZERO,
            // HW_MAP_MPP: the zoom it opens at (screenshots of each level).
            mpp: std::env::var("HW_MAP_MPP")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(START_MPP),
            tiles: BTreeMap::new(),
            asked: BTreeSet::new(),
            queue,
            made,
            relock: false,
            to_centre: true,
        }
    }

    /// How many tiles are in memory (for the memory checks).
    pub fn tiles_held(&self) -> usize {
        self.tiles.len()
    }

    /// Opens or shuts the map; it opens on `hero`.
    pub fn toggle(&mut self, hero: Option<Vec3>) {
        self.open = !self.open;
        if self.open {
            self.to_centre = true;
            self.centre_on(hero);
        } else {
            self.forget_asks();
        }
    }

    /// Centres the view on the hero, once one is known.
    fn centre_on(&mut self, hero: Option<Vec3>) {
        if let Some(h) = hero.filter(|_| self.to_centre) {
            self.centre = DVec2::new(h.x as f64, h.z as f64);
            self.to_centre = false;
        }
    }

    /// Drops the asks no worker has started on.
    fn forget_asks(&mut self) {
        let mut q = self.queue.0.lock().expect("town map queue");
        for k in q.todo.drain(..) {
            self.asked.remove(&k);
        }
    }

    /// Takes in finished tiles, asks for the ones `view` needs (`level`
    /// and the coarsest level as a backdrop), and forgets the farthest
    /// beyond the cap.
    fn stream(&mut self, ctx: &egui::Context, lo: DVec2, hi: DVec2, level: u8) {
        for _ in 0..UPLOADS_PER_FRAME {
            let Ok((key, img)) = self.made.try_recv() else {
                break;
            };
            self.asked.remove(&key);
            let name = format!("townmap {} {} {}", key.level, key.x, key.z);
            let tex = ctx.load_texture(name, img, egui::TextureOptions::LINEAR);
            self.tiles.insert(key, tex);
        }
        let mid = (lo + hi) * 0.5;
        let mut want = keys_in(level, lo, hi);
        if level < 2 {
            want.extend(keys_in(2, lo, hi));
        }
        // The workers take from the end: the coarse backdrop (cheap, and it
        // covers the whole view) goes first, then the detail nearest the
        // middle.
        want.retain(|k| !self.tiles.contains_key(k));
        want.sort_by(|a, b| {
            (a.level == 2 && level < 2)
                .cmp(&(b.level == 2 && level < 2))
                .then(dist(b, mid).total_cmp(&dist(a, mid)))
        });
        {
            let (lock, ready) = &*self.queue;
            let mut q = lock.lock().expect("town map queue");
            // What a worker took is being made: not asked again.
            let queued: BTreeSet<TileKey> = q.todo.iter().copied().collect();
            self.asked.retain(|k| !queued.contains(k));
            want.retain(|k| !self.asked.contains(k));
            self.asked.extend(want.iter().copied());
            let changed = q.todo != want;
            q.todo = want;
            if changed {
                ready.notify_all();
            }
        }
        if self.tiles.len() > MAX_TILES {
            let mut keys: Vec<TileKey> = self.tiles.keys().copied().collect();
            keys.sort_by(|a, b| dist(b, mid).total_cmp(&dist(a, mid)));
            for k in keys.into_iter().take(self.tiles.len() - MAX_TILES) {
                self.tiles.remove(&k);
            }
        }
    }

    /// Draws the map when it is open, and takes the mouse for panning and
    /// zooming. `mates`: the other heroes on the ground plan.
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        town: &TownWorld,
        hero: Option<Vec3>,
        look: Vec3,
        mates: &[DVec2],
        radio: &Radio,
        mine: u8,
    ) {
        if !self.open {
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.toggle(None);
            return;
        }
        self.centre_on(hero);
        let screen = ctx.screen_rect();
        let rect = screen.shrink(MARGIN);
        if rect.width() < 100.0 || rect.height() < 100.0 {
            return;
        }
        // Zoomed out at most to the whole town with room round it.
        let max_mpp =
            (town_span(town) * 1.4 / rect.width().min(rect.height()) as f64).max(MIN_MPP * 8.0);
        self.mpp = self.mpp.clamp(MIN_MPP, max_mpp);
        let ppp = ctx.pixels_per_point();
        egui::Area::new(egui::Id::new("town map"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .show(ctx, |ui| {
                let (resp, painter) = ui.allocate_painter(rect.size(), egui::Sense::drag());
                let rect = resp.rect;
                // Drag to pan; the wheel zooms round the pointer.
                let drag = resp.drag_delta();
                self.centre -= DVec2::new(drag.x as f64, drag.y as f64) * self.mpp;
                if let Some(at) = resp.hover_pos() {
                    let wheel = ui.input(|i| i.smooth_scroll_delta.y) as f64;
                    if wheel != 0.0 {
                        let off = at - rect.center();
                        let off = DVec2::new(off.x as f64, off.y as f64);
                        let under = self.centre + off * self.mpp;
                        self.mpp = (self.mpp * (-wheel * 0.003).exp()).clamp(MIN_MPP, max_mpp);
                        self.centre = under - off * self.mpp;
                    }
                }
                let half = DVec2::new(rect.width() as f64, rect.height() as f64) * 0.5 * self.mpp;
                let (lo, hi) = (self.centre - half, self.centre + half);
                let level = level_for(self.mpp / ppp as f64);
                self.stream(ui.ctx(), lo, hi, level);
                self.paint(&painter, rect, town, hero, look, mates, radio, mine, level);
                // Back to the hero.
                let button = egui::Rect::from_min_size(
                    egui::pos2(rect.right() - 128.0, rect.top() + 8.0),
                    egui::vec2(118.0, 26.0),
                );
                if hero.is_some() && ui.put(button, egui::Button::new("Back to me")).clicked() {
                    self.to_centre = true;
                    self.centre_on(hero);
                }
            });
    }

    #[allow(clippy::too_many_arguments)]
    fn paint(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        town: &TownWorld,
        hero: Option<Vec3>,
        look: Vec3,
        mates: &[DVec2],
        radio: &Radio,
        mine: u8,
        level: u8,
    ) {
        let painter = painter.with_clip_rect(rect);
        let (centre, mpp) = (self.centre, self.mpp);
        let to_screen = |p: DVec2| {
            let q = (p - centre) / mpp;
            rect.center() + egui::vec2(q.x as f32, q.y as f32)
        };
        painter.rect_filled(rect, 8.0, egui::Color32::from_rgb(200, 192, 170));
        // The coarse backdrop first, the detail over it.
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        for l in (level..3).rev() {
            for (k, tex) in self.tiles.range(
                TileKey {
                    level: l,
                    x: i32::MIN,
                    z: i32::MIN,
                }..=TileKey {
                    level: l,
                    x: i32::MAX,
                    z: i32::MAX,
                },
            ) {
                let a = to_screen(k.lo());
                let b = to_screen(k.lo() + DVec2::splat(k.span()));
                let r = egui::Rect::from_min_max(a, b);
                if r.intersects(rect) {
                    painter.image(tex.id(), r, uv, egui::Color32::WHITE);
                }
            }
        }
        // Marks first, then their names, most important first; a name
        // that would cover one already written moves to another side of
        // its mark, or is left out until you zoom in.
        let mut names: Vec<(egui::Pos2, f32, String, egui::Color32)> = Vec::new();
        if let Some((st, at)) = radio.story.as_ref().and_then(|s| s.at.map(|a| (s, a))) {
            let p = to_screen(DVec2::new(at.x as f64, at.z as f64));
            star(&painter, p, 10.0);
            names.push((
                p,
                11.0,
                st.goal.clone(),
                egui::Color32::from_rgb(110, 70, 0),
            ));
        }
        if let Some((st, at)) = radio.case.as_ref().and_then(|s| s.at.map(|a| (s, a))) {
            let p = to_screen(DVec2::new(at.x as f64, at.z as f64));
            diamond(&painter, p, 8.0);
            names.push((p, 9.0, st.goal.clone(), egui::Color32::from_rgb(0, 95, 90)));
        }
        for c in radio.calls.values() {
            let p = to_screen(DVec2::new(c.at.x as f64, c.at.z as f64));
            let col = con_colour(c.level, mine);
            painter.circle_filled(p, 7.0, col);
            painter.circle_stroke(p, 7.0, egui::Stroke::new(1.5_f32, egui::Color32::BLACK));
            names.push((p, 9.0, c.kind.name().to_string(), egui::Color32::BLACK));
        }
        for l in &town.landmarks {
            let p = to_screen(l.pos);
            if !rect.expand(80.0).contains(p) {
                continue;
            }
            painter.circle_filled(p, 5.0, landmark_colour(l.kind));
            painter.circle_stroke(
                p,
                5.0,
                egui::Stroke::new(1.2_f32, egui::Color32::from_rgb(40, 40, 50)),
            );
            let name = match l.kind {
                LandmarkKind::CometBuilding => hw_gen::urban::comet::name(l),
                k => landmark_name(k),
            };
            names.push((
                p,
                7.0,
                name.to_string(),
                egui::Color32::from_rgb(40, 40, 50),
            ));
        }
        // Molehaven's gate: the mole's face, and the town's name.
        if let Some(g) = town.underground().entrances.iter().find(|e| e.molehaven) {
            let p = to_screen(g.pos);
            if rect.expand(80.0).contains(p) {
                mole_icon(&painter, p, 9.0);
                names.push((
                    p,
                    10.0,
                    "Molehaven".into(),
                    egui::Color32::from_rgb(120, 60, 10),
                ));
            }
        }
        // The villains' quarter's name, under its middle.
        if let Some(d) = &town.town.dystopia {
            let c = to_screen(d.centre + DVec2::new(0.0, d.radius * 0.3));
            names.push((
                c,
                0.0,
                "Mad scientist's quarter".into(),
                egui::Color32::from_rgb(50, 95, 30),
            ));
        }
        for m in mates {
            let p = to_screen(*m);
            painter.circle_filled(p, 5.0, egui::Color32::from_rgb(60, 140, 255));
            painter.circle_stroke(p, 5.0, egui::Stroke::new(1.2_f32, egui::Color32::WHITE));
        }
        let mut hero_box = None;
        if let Some(h) = hero {
            let p = to_screen(DVec2::new(h.x as f64, h.z as f64));
            let d = glam::Vec2::new(look.x, look.z).normalize_or(glam::Vec2::Y);
            let tip = p + egui::vec2(d.x, d.y) * 13.0;
            let side = egui::vec2(-d.y, d.x) * 7.0;
            painter.add(egui::Shape::convex_polygon(
                vec![tip, p + side, p - side],
                egui::Color32::from_rgb(220, 40, 40),
                egui::Stroke::new(1.5_f32, egui::Color32::WHITE),
            ));
            hero_box = Some(egui::Rect::from_center_size(p, egui::vec2(26.0, 26.0)));
        }
        let mut taken: Vec<egui::Rect> = hero_box.into_iter().collect();
        for (at, gap, text, col) in names {
            place_name(&painter, &mut taken, at, gap, &text, col);
        }
        // A scale bar, bottom left.
        let metres = nice_length(mpp * 120.0);
        let len = (metres / mpp) as f32;
        let a = rect.left_bottom() + egui::vec2(16.0, -18.0);
        let b = a + egui::vec2(len, 0.0);
        let ink = egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(60, 45, 25));
        painter.line_segment([a, b], ink);
        painter.line_segment([a, a - egui::vec2(0.0, 6.0)], ink);
        painter.line_segment([b, b - egui::vec2(0.0, 6.0)], ink);
        let text = if metres >= 1000.0 {
            format!("{} km", metres / 1000.0)
        } else {
            format!("{metres} m")
        };
        label(
            &painter,
            a - egui::vec2(0.0, 9.0),
            egui::Align2::LEFT_BOTTOM,
            &text,
            ink.color,
        );
        painter.rect_stroke(
            rect,
            8.0,
            egui::Stroke::new(3.0_f32, egui::Color32::from_rgb(120, 90, 50)),
            egui::StrokeKind::Inside,
        );
        let title = egui::Rect::from_min_size(
            rect.left_top() + egui::vec2(10.0, 8.0),
            egui::vec2(470.0, 26.0),
        );
        painter.rect_filled(
            title,
            6.0,
            egui::Color32::from_rgba_unmultiplied(240, 232, 210, 220),
        );
        painter.text(
            title.left_center() + egui::vec2(8.0, 0.0),
            egui::Align2::LEFT_CENTER,
            "TOWN MAP   drag to move, wheel to zoom   (M to close)   N up",
            egui::FontId::proportional(15.0),
            egui::Color32::from_rgb(90, 60, 30),
        );
    }
}

impl Drop for TownMap {
    fn drop(&mut self) {
        let (lock, ready) = &*self.queue;
        if let Ok(mut q) = lock.lock() {
            q.stop = true;
        }
        ready.notify_all();
    }
}

/// Writes `text` beside the mark at `at` (`gap` points from it: right,
/// left, above or below, the first side clear of `taken`), or leaves it
/// out when every side is covered. `gap` 0: centred under `at`.
fn place_name(
    painter: &egui::Painter,
    taken: &mut Vec<egui::Rect>,
    at: egui::Pos2,
    gap: f32,
    text: &str,
    col: egui::Color32,
) {
    let size = painter
        .layout_no_wrap(text.to_string(), egui::FontId::proportional(13.0), col)
        .size()
        + egui::vec2(2.0, 2.0);
    let sides = if gap == 0.0 {
        vec![egui::pos2(at.x - size.x * 0.5, at.y + 2.0)]
    } else {
        vec![
            egui::pos2(at.x + gap, at.y - size.y * 0.5),
            egui::pos2(at.x - gap - size.x, at.y - size.y * 0.5),
            egui::pos2(at.x - size.x * 0.5, at.y - gap - size.y),
            egui::pos2(at.x - size.x * 0.5, at.y + gap),
        ]
    };
    for min in sides {
        let r = egui::Rect::from_min_size(min, size);
        if taken.iter().all(|t| !t.intersects(r)) {
            label(
                painter,
                min + egui::vec2(1.0, 1.0),
                egui::Align2::LEFT_TOP,
                text,
                col,
            );
            taken.push(r);
            return;
        }
    }
}

/// Text with a pale halo, readable on any part of the map.
fn label(
    painter: &egui::Painter,
    at: egui::Pos2,
    align: egui::Align2,
    text: &str,
    col: egui::Color32,
) {
    let font = egui::FontId::proportional(13.0);
    let halo = egui::Color32::from_rgba_unmultiplied(250, 245, 230, 200);
    for d in [
        egui::vec2(1.0, 0.0),
        egui::vec2(-1.0, 0.0),
        egui::vec2(0.0, 1.0),
        egui::vec2(0.0, -1.0),
    ] {
        painter.text(at + d, align, text, font.clone(), halo);
    }
    painter.text(at, align, text, font, col);
}

/// Molehaven's mark on the maps: a mole's face in goggles on an amber
/// disc, `r` points across.
pub fn mole_icon(painter: &egui::Painter, p: egui::Pos2, r: f32) {
    let ink = egui::Stroke::new(1.3_f32, egui::Color32::from_rgb(60, 36, 20));
    painter.circle_filled(p, r, egui::Color32::from_rgb(255, 176, 84));
    painter.circle_stroke(p, r, ink);
    let fur = egui::Color32::from_rgb(110, 78, 56);
    for ex in [-1.0f32, 1.0] {
        painter.circle_filled(p + egui::vec2(ex * r * 0.42, -r * 0.42), r * 0.2, fur);
    }
    painter.add(egui::Shape::ellipse_filled(
        p + egui::vec2(0.0, r * 0.05),
        egui::vec2(r * 0.58, r * 0.5),
        fur,
    ));
    for ex in [-1.0f32, 1.0] {
        let e = p + egui::vec2(ex * r * 0.24, -r * 0.08);
        painter.circle_filled(e, r * 0.15, egui::Color32::from_rgb(255, 214, 120));
        painter.circle_stroke(
            e,
            r * 0.15,
            egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(200, 150, 60)),
        );
    }
    painter.circle_filled(
        p + egui::vec2(0.0, r * 0.28),
        r * 0.12,
        egui::Color32::from_rgb(236, 120, 140),
    );
}

/// A landmark's name on the map.
pub fn landmark_name(kind: LandmarkKind) -> &'static str {
    match kind {
        LandmarkKind::AetherSpire => "Aether Spire",
        LandmarkKind::Lighthouse => "Lighthouse",
        LandmarkKind::TeslaTower => "Tesla tower",
        LandmarkKind::HeroHall => "Hero Hall",
        LandmarkKind::MooringMast => "Airfield",
        LandmarkKind::RocketGantry => "Rocket port",
        LandmarkKind::AtomicPlant => "Atomic plant",
        LandmarkKind::MoleDrill => "Mole drill",
        LandmarkKind::WeatherTower => "Weather tower",
        LandmarkKind::RobotWorks => "Robot works",
        LandmarkKind::Hospital => "Hospital",
        LandmarkKind::CometBuilding => "Comet building",
    }
}

/// A landmark's dot colour, the same on the minimap and the town map.
pub fn landmark_colour(kind: LandmarkKind) -> egui::Color32 {
    match kind {
        LandmarkKind::AetherSpire => egui::Color32::from_rgb(120, 200, 255),
        LandmarkKind::Lighthouse => egui::Color32::from_rgb(250, 250, 250),
        LandmarkKind::TeslaTower => egui::Color32::from_rgb(140, 220, 90),
        LandmarkKind::HeroHall => egui::Color32::from_rgb(240, 190, 70),
        LandmarkKind::MooringMast => egui::Color32::from_rgb(230, 70, 60),
        LandmarkKind::RocketGantry => egui::Color32::from_rgb(235, 90, 60),
        LandmarkKind::AtomicPlant => egui::Color32::from_rgb(90, 175, 255),
        LandmarkKind::MoleDrill => egui::Color32::from_rgb(200, 120, 60),
        LandmarkKind::WeatherTower => egui::Color32::from_rgb(110, 180, 235),
        LandmarkKind::RobotWorks => egui::Color32::from_rgb(200, 60, 50),
        LandmarkKind::Hospital => egui::Color32::from_rgb(120, 255, 200),
        LandmarkKind::CometBuilding => egui::Color32::from_rgb(255, 214, 120),
    }
}

/// A tile as a picture (on a worker).
fn picture(t: &mapraster::MapTile) -> egui::ColorImage {
    let rgb: Vec<u8> = t.px.iter().flat_map(|b| mapraster::colour(*b)).collect();
    egui::ColorImage::from_rgb([TILE_PX, TILE_PX], &rgb)
}

/// The level whose pixels suit `m_per_px` metres a screen pixel. The big
/// map fills the screen, so it changes level sooner than the minimap.
fn level_for(m_per_px: f64) -> u8 {
    let lvl1 = LEVEL_SPAN[1] / TILE_PX as f64;
    if m_per_px <= lvl1 * 0.25 {
        0
    } else if m_per_px <= lvl1 * 1.5 {
        1
    } else {
        2
    }
}

/// The tiles at `level` that meet `lo..hi`.
fn keys_in(level: u8, lo: DVec2, hi: DVec2) -> Vec<TileKey> {
    let a = TileKey::at(level, lo);
    let b = TileKey::at(level, hi);
    let mut out = Vec::new();
    for z in a.z..=b.z {
        for x in a.x..=b.x {
            out.push(TileKey { level, x, z });
        }
    }
    out
}

/// From a tile's middle to `at` (m).
fn dist(k: &TileKey, at: DVec2) -> f64 {
    (k.lo() + DVec2::splat(k.span() * 0.5)).distance(at)
}

/// How wide the town is, roads to the edge (m).
fn town_span(town: &TownWorld) -> f64 {
    let (mut lo, mut hi) = (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN));
    for piece in &town.town.pieces {
        for p in &piece.face.pts {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
    }
    if lo.x > hi.x {
        1000.0
    } else {
        (hi - lo).max_element().max(1000.0)
    }
}

/// A round length near `m` for the scale bar: 1, 2 or 5 times a power of ten.
fn nice_length(m: f64) -> f64 {
    let p = 10f64.powf(m.max(1.0).log10().floor());
    [5.0, 2.0, 1.0]
        .into_iter()
        .map(|k| k * p)
        .find(|&l| l <= m)
        .unwrap_or(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn the_view_picks_finer_tiles_as_you_zoom_in() {
        assert_eq!(level_for(START_MPP), 0);
        assert_eq!(level_for(2.0), 1);
        assert_eq!(level_for(20.0), 2);
        let keys = keys_in(0, DVec2::new(-10.0, -10.0), DVec2::new(120.0, 10.0));
        assert_eq!(keys.len(), 4);
        assert_eq!(nice_length(130.0), 100.0);
        assert_eq!(nice_length(2600.0), 2000.0);
    }

    #[test]
    fn its_workers_stop_when_it_goes() {
        use hw_core::Seed;
        use hw_gen::urban::TownParams;
        let w = Arc::new(TownWorld::generate(
            Seed(3),
            TownParams::sample(Seed(3), 2_000.0),
        ));
        let map = TownMap::new(w.clone());
        let q = Arc::downgrade(&map.queue);
        drop(map);
        // Both workers let go of the queue once they see the stop.
        for _ in 0..200 {
            if q.strong_count() == 0 {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("the town map's workers kept running");
    }
}
