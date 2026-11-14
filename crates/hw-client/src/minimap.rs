//! The minimap: a round street map in the bottom right corner that turns
//! with the camera (what you look at is up), zooms out as you fly higher,
//! and marks you, your friends, the landmarks and every call on the radio.
//! N hides it; M opens the town map (`townmap.rs`), drawn from the same tiles.
//!
//! The map is drawn from tiles of map pixels (`mapraster.rs`) made by a
//! worker thread round the player and forgotten when far, so it streams
//! with the world: a megacity's map costs no more memory than a village's.
//! The round picture itself is painted on a second thread ([`Painter`]):
//! one picture is asked for at a time, and the markers are drawn with the
//! same view the picture was painted for, so map and marks always agree.

use crate::fightview::con_colour;
use crate::mapraster::{self, MapTile, Surroundings, TileKey};
use crate::radioview::Radio;
use engine::prelude::*;
use engine::ui::egui;
use glam::DVec2;
use hw_gen::urban::world::TownWorld;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

/// Another hero of the session, as the maps and the patrol visor show them.
#[derive(Clone, Debug, PartialEq)]
pub struct Mate {
    pub name: String,
    /// The player.
    pub player: hw_net::PlayerId,
    /// Where they stand (town frame).
    pub pos: glam::DVec3,
    /// A Herobot.
    pub bot: bool,
    /// Their hero's entity, when it is in the mirror (not for a mate beyond
    /// the interest radius): what a click picks as the friendly target.
    pub id: Option<hw_core::NetId>,
    /// Health in percent, and whether they are down.
    pub health: u8,
    pub down: bool,
}

impl Mate {
    /// Their colour on the maps: bright blue for people, teal for Herobots.
    pub fn colour(&self) -> egui::Color32 {
        if self.bot {
            egui::Color32::from_rgb(60, 225, 200)
        } else {
            egui::Color32::from_rgb(90, 165, 255)
        }
    }
}

/// The map's radius on screen (points).
const RADIUS: f32 = 92.0;
/// Its key in the saved screen layout.
const LAYOUT_KEY: &str = "minimap";
/// Gap to the screen's edges (points).
const MARGIN: f32 = 30.0;
/// Metres from the middle to the rim when on foot.
const WALK_REACH: f64 = 110.0;
/// The most it zooms out to, high in the sky (m).
const MAX_REACH: f64 = 2400.0;
/// Tiles kept in memory at most (each 16 KB); the farthest go first.
const MAX_TILES: usize = 160;

/// The minimap and its tiles.
pub struct Minimap {
    pub shown: bool,
    tiles: BTreeMap<TileKey, Arc<MapTile>>,
    asked: BTreeSet<TileKey>,
    ask: Sender<TileKey>,
    made: Receiver<MapTile>,
    texture: Option<egui::TextureHandle>,
    /// Pictures to paint, and painted ones (with the view they show).
    paint: Sender<Paint>,
    painted: Receiver<(Paint, egui::ColorImage)>,
    /// A picture asked for and not back yet.
    painting: bool,
    /// The view the shown picture was painted for.
    showing: Option<Paint>,
    /// The reach shown (m), eased towards the wanted one.
    reach: f64,
}

/// One picture of the map: the tiles it is painted from, where it is
/// centred, which way is up, and its scale.
#[derive(Clone)]
struct Paint {
    tiles: BTreeMap<TileKey, Arc<MapTile>>,
    side: usize,
    at: DVec2,
    fwd: DVec2,
    right: DVec2,
    reach: f64,
}

impl Minimap {
    pub fn new(town: Arc<TownWorld>) -> Self {
        let (ask, asks) = channel::<TileKey>();
        let (done, made) = channel();
        std::thread::Builder::new()
            .name("minimap".into())
            .spawn(move || {
                engine::parallel::background_thread();
                while let Ok(key) = asks.recv() {
                    if done.send(mapraster::raster(&town, key)).is_err() {
                        break;
                    }
                }
            })
            .expect("minimap thread");
        let (paint, jobs) = channel::<Paint>();
        let (done, painted) = channel();
        std::thread::Builder::new()
            .name("minimap-paint".into())
            .spawn(move || {
                engine::parallel::background_thread();
                let mut image = Vec::new();
                while let Ok(job) = jobs.recv() {
                    let mpp = job.reach / (job.side as f64 * 0.5);
                    compose(
                        &job.tiles,
                        &mut image,
                        job.side,
                        job.at,
                        job.fwd,
                        job.right,
                        mpp,
                        level_for(job.reach),
                    );
                    let img = egui::ColorImage::from_rgba_unmultiplied([job.side; 2], &image);
                    // Tiles are let go here, not on the main thread.
                    let job = Paint {
                        tiles: BTreeMap::new(),
                        ..job
                    };
                    if done.send((job, img)).is_err() {
                        break;
                    }
                }
            })
            .expect("minimap painter thread");
        Self {
            shown: std::env::var("HW_MINIMAP").map_or(true, |v| v != "0"),
            tiles: BTreeMap::new(),
            asked: BTreeSet::new(),
            ask,
            made,
            texture: None,
            paint,
            painted,
            painting: false,
            showing: None,
            reach: WALK_REACH,
        }
    }

    /// Takes in finished tiles, asks for the ones round `at` that `reach`
    /// metres of map needs, and forgets the farthest beyond the cap.
    fn stream(&mut self, at: DVec2, reach: f64) {
        while let Ok(t) = self.made.try_recv() {
            self.asked.remove(&t.key);
            self.tiles.insert(t.key, Arc::new(t));
        }
        // Level 0 always (the soundscape reads it), plus the level the
        // map shows at.
        let level = level_for(reach);
        let mut want = keys_round(0, at, WALK_REACH.max(64.0));
        if level > 0 {
            want.extend(keys_round(level, at, reach));
        }
        want.sort_by(|a, b| dist(a, at).total_cmp(&dist(b, at)));
        for k in want {
            if !self.tiles.contains_key(&k) && self.asked.insert(k) {
                let _ = self.ask.send(k);
            }
        }
        if self.tiles.len() > MAX_TILES {
            let mut keys: Vec<TileKey> = self.tiles.keys().copied().collect();
            keys.sort_by(|a, b| dist(b, at).total_cmp(&dist(a, at)));
            for k in keys.into_iter().take(self.tiles.len() - MAX_TILES) {
                self.tiles.remove(&k);
            }
        }
    }

    /// How many tiles are in memory (for tests and the memory checks).
    pub fn tiles_held(&self) -> usize {
        self.tiles.len()
    }

    /// What lies within `r` metres of `p`, from the map (None until its
    /// tiles are made).
    pub fn surroundings(&self, p: DVec2, r: f64) -> Option<Surroundings> {
        mapraster::surroundings(|k| self.tiles.get(&k).map(|t| &**t), p, r)
    }

    /// Streams the tiles and draws the map. `at`: the hero (or camera) on
    /// the ground plan; `look`: where the camera looks; `height`: metres
    /// above the ground; `mates`: the other heroes.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        ctx: &egui::Context,
        town: &TownWorld,
        at: DVec2,
        look: Vec3,
        height: f64,
        mates: &[Mate],
        radio: &Radio,
        mine: u8,
        toggle: bool,
    ) {
        if toggle {
            self.shown = !self.shown;
        }
        let want = (WALK_REACH + height.max(0.0) * 1.6).min(MAX_REACH);
        self.reach += (want - self.reach) * 0.08;
        self.stream(at, self.reach);
        if self.shown || crate::layout::editing() {
            self.draw(ctx, town, at, look, mates, radio, mine);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw(
        &mut self,
        ctx: &egui::Context,
        town: &TownWorld,
        at: DVec2,
        look: Vec3,
        mates: &[Mate],
        radio: &Radio,
        mine: u8,
    ) {
        let screen = ctx.screen_rect();
        let home = egui::pos2(
            screen.right() - MARGIN - RADIUS,
            screen.bottom() - MARGIN - RADIUS,
        );
        // Where the player put it, and how big (see `layout.rs`): scaled
        // about its bottom right corner, so growing it fills the corner.
        let box_ = egui::Rect::from_center_size(home, egui::vec2(RADIUS + 8.0, RADIUS + 8.0) * 2.0);
        let anchor = box_.right_bottom();
        let tf = crate::layout::transform(ctx, LAYOUT_KEY, anchor);
        let centre = tf.mul_pos(home);
        let radius = RADIUS * tf.scaling;
        crate::layout::overlay(ctx, LAYOUT_KEY, "Minimap", anchor, box_);
        let ppp = ctx.pixels_per_point();
        let side = ((radius * 2.0 * ppp).round() as usize).clamp(64, 768);
        let fwd = DVec2::new(look.x as f64, look.z as f64)
            .try_normalize()
            .unwrap_or(DVec2::NEG_Y);
        let right = DVec2::new(-fwd.y, fwd.x);
        // The picture painted since: shown from now on, with its view.
        match self.painted.try_recv() {
            Ok((view, img)) => {
                self.painting = false;
                self.showing = Some(view);
                match &mut self.texture {
                    Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                    None => {
                        self.texture =
                            Some(ctx.load_texture("minimap", img, egui::TextureOptions::LINEAR))
                    }
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                panic!("the minimap painter stopped")
            }
        }
        // The next picture, of the view now (one at a time).
        if !self.painting {
            let job = Paint {
                tiles: self.tiles.clone(),
                side,
                at,
                fwd,
                right,
                reach: self.reach,
            };
            self.painting = self.paint.send(job).is_ok();
        }
        // Marks go where the shown picture has things.
        let Some(view) = &self.showing else { return };
        let (at, fwd, right, reach) = (view.at, view.fwd, view.right, view.reach);
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("minimap"),
        ));
        let rect = egui::Rect::from_center_size(centre, egui::vec2(radius * 2.0, radius * 2.0));
        painter.circle_filled(
            centre + egui::vec2(2.0, 3.0),
            radius + 5.0,
            egui::Color32::from_black_alpha(90),
        );
        if let Some(t) = &self.texture {
            painter.image(
                t.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        // A 1950s starburst clock round a gold chrome bezel.
        crate::googie::starburst_ring(&painter, centre, radius);
        // A gold chrome bezel with hour ticks, Googie style.
        painter.circle_stroke(
            centre,
            radius + 2.5,
            egui::Stroke::new(5.0_f32, crate::uikit::GOLD),
        );
        painter.circle_stroke(
            centre,
            radius + 1.0,
            egui::Stroke::new(1.0_f32, crate::uikit::GOLD_LIGHT),
        );
        painter.circle_stroke(
            centre,
            radius + 5.5,
            egui::Stroke::new(2.0_f32, crate::uikit::DEEP),
        );
        painter.circle_stroke(
            centre,
            radius,
            egui::Stroke::new(1.0_f32, crate::uikit::GOLD_DARK),
        );
        for k in 0..12 {
            let a = k as f32 * std::f32::consts::TAU / 12.0;
            let d = egui::vec2(a.sin(), -a.cos());
            painter.line_segment(
                [centre + d * (radius + 0.5), centre + d * (radius + 4.5)],
                egui::Stroke::new(1.5_f32, crate::uikit::GOLD_DARK),
            );
        }
        // Map metres to the screen.
        let scale = radius as f64 / reach;
        let to_screen = |p: DVec2| -> (egui::Pos2, bool) {
            let d = p - at;
            let (u, v) = (d.dot(right) * scale, -d.dot(fwd) * scale);
            let len = (u * u + v * v).sqrt();
            let inside = len <= radius as f64 - 6.0;
            let k = if inside {
                1.0
            } else {
                (radius as f64 - 6.0) / len.max(1e-9)
            };
            (centre + egui::vec2((u * k) as f32, (v * k) as f32), inside)
        };
        // North on the rim.
        let (n, _) = to_screen(at + DVec2::new(0.0, -1e6));
        let n = centre + (n - centre).normalized() * (radius + 2.5);
        painter.circle_filled(n, 9.0, crate::uikit::HERO_RED);
        painter.circle_stroke(n, 9.0, egui::Stroke::new(1.5_f32, crate::uikit::DEEP));
        painter.text(
            n + egui::vec2(0.0, 0.5),
            egui::Align2::CENTER_CENTER,
            "N",
            crate::uikit::ui_font(ctx, 11.5),
            crate::uikit::CREAM,
        );
        for l in &town.landmarks {
            let (p, inside) = to_screen(l.pos);
            if inside {
                let c = crate::townmap::landmark_colour(l.kind);
                painter.circle_filled(p, 4.5, c);
                painter.circle_stroke(
                    p,
                    4.5,
                    egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(40, 40, 50)),
                );
            }
        }
        // Molehaven's gate, kept on the rim when it is off the map, so the
        // way there always shows.
        if let Some(g) = town.underground().entrances.iter().find(|e| e.molehaven) {
            let (p, inside) = to_screen(g.pos);
            crate::townmap::mole_icon(&painter, p, if inside { 7.0 } else { 5.5 });
        }
        for c in radio.calls.values() {
            let (p, inside) = to_screen(DVec2::new(c.at.x as f64, c.at.z as f64));
            let r = if inside { 5.5 } else { 4.0 };
            painter.circle_filled(p, r, con_colour(c.level, mine));
            painter.circle_stroke(p, r, egui::Stroke::new(1.2_f32, egui::Color32::BLACK));
        }
        for m in mates {
            let (p, _) = to_screen(DVec2::new(m.pos.x, m.pos.z));
            let col = m.colour();
            painter.circle_filled(p, 5.0, col);
            painter.circle_stroke(p, 5.0, egui::Stroke::new(1.5_f32, egui::Color32::WHITE));
            // Their name beside the dot, toward the middle of the map so it
            // stays inside when the dot is clamped to the rim.
            let side = if p.x > centre.x {
                egui::Align2::RIGHT_CENTER
            } else {
                egui::Align2::LEFT_CENTER
            };
            let dx = if p.x > centre.x { -8.0 } else { 8.0 };
            let font = crate::uikit::ui_font(ctx, 11.0);
            painter.text(
                p + egui::vec2(dx + 0.8, 0.8),
                side,
                &m.name,
                font.clone(),
                egui::Color32::BLACK,
            );
            painter.text(p + egui::vec2(dx, 0.0), side, &m.name, font, col);
        }
        // You, in the middle, looking up the map.
        let tip = centre + egui::vec2(0.0, -9.0);
        painter.add(egui::Shape::convex_polygon(
            vec![
                tip,
                centre + egui::vec2(6.0, 6.0),
                centre + egui::vec2(0.0, 3.0),
                centre + egui::vec2(-6.0, 6.0),
            ],
            egui::Color32::from_rgb(220, 40, 40),
            egui::Stroke::new(1.3_f32, egui::Color32::WHITE),
        ));
    }
}

/// The level whose pixels suit showing `reach` metres to the rim.
fn level_for(reach: f64) -> u8 {
    let mpp = reach / (RADIUS as f64 * 1.5);
    if mpp <= 1.6 {
        0
    } else if mpp <= 6.0 {
        1
    } else {
        2
    }
}

/// The tiles at `level` within `r` metres of `at` (a square round it).
fn keys_round(level: u8, at: DVec2, r: f64) -> Vec<TileKey> {
    let a = TileKey::at(level, at - DVec2::splat(r));
    let b = TileKey::at(level, at + DVec2::splat(r));
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

#[allow(clippy::too_many_arguments)]
/// Paints the round map into `image` (`side` pixels square, RGBA): each
/// pixel looks up the map where it falls, the finest tile there first.
fn compose(
    tiles: &BTreeMap<TileKey, Arc<MapTile>>,
    image: &mut Vec<u8>,
    side: usize,
    at: DVec2,
    fwd: DVec2,
    right: DVec2,
    mpp: f64,
    first: u8,
) {
    image.resize(side * side * 4, 0);
    let half = side as f64 * 0.5;
    // A one-entry cache of the last tile used: neighbouring pixels share it.
    let mut last: Option<&MapTile> = None;
    for j in 0..side {
        for i in 0..side {
            let (u, v) = (i as f64 + 0.5 - half, j as f64 + 0.5 - half);
            let r = (u * u + v * v).sqrt();
            let o = (j * side + i) * 4;
            if r > half {
                image[o..o + 4].copy_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            // Four samples a pixel, so edges are smooth at any angle.
            let mut sum = [0u32; 3];
            for (du, dv) in [(-0.25, -0.25), (0.25, -0.25), (-0.25, 0.25), (0.25, 0.25)] {
                let p = at + right * ((u + du) * mpp) - fwd * ((v + dv) * mpp);
                let byte = match last.filter(|t| covers(t, p)) {
                    Some(t) => Some(t.at(p)),
                    None => (first..3).chain((0..first).rev()).find_map(|l| {
                        let t: &MapTile = tiles.get(&TileKey::at(l, p))?;
                        last = Some(t);
                        Some(t.at(p))
                    }),
                };
                let c = byte.map_or([200, 192, 170], mapraster::colour);
                for k in 0..3 {
                    sum[k] += c[k] as u32;
                }
            }
            let rgb = sum.map(|c| (c / 4) as u8);
            // A soft rim: the last pixel fades out.
            let a = ((half - r).clamp(0.0, 1.0) * 255.0) as u8;
            image[o..o + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], a]);
        }
    }
}

/// Does tile `t` hold `p`?
fn covers(t: &MapTile, p: DVec2) -> bool {
    let lo = t.key.lo();
    let s = t.key.span();
    p.x >= lo.x && p.x < lo.x + s && p.y >= lo.y && p.y < lo.y + s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn it_zooms_out_as_you_climb() {
        assert_eq!(level_for(WALK_REACH), 0);
        assert_eq!(level_for(600.0), 1);
        assert_eq!(level_for(MAX_REACH), 2);
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn the_tiles_round_a_spot_cover_it() {
        let at = DVec2::new(300.0, -50.0);
        let keys = keys_round(0, at, 110.0);
        assert!(keys.contains(&TileKey::at(0, at)));
        assert!(keys.contains(&TileKey::at(0, at + DVec2::new(109.0, 109.0))));
        assert!(keys.len() <= 9);
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn up_on_the_map_is_where_the_camera_looks() {
        // An empty map still paints a round disc, clear outside it.
        let mut img = Vec::new();
        let tiles = BTreeMap::new();
        compose(
            &tiles,
            &mut img,
            64,
            DVec2::ZERO,
            DVec2::NEG_Y,
            DVec2::X,
            1.0,
            0,
        );
        assert_eq!(img[3], 0, "corners are clear");
        let mid = (32 * 64 + 32) * 4;
        assert_eq!(img[mid + 3], 255);
    }
}
