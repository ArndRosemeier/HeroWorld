//! The title's fun: the comic cover it opens on, things to click, and
//! the visitors that turn up now and then (a robot rising, an alien at the
//! glass, the villain in a flash of lightning, a space octopus, a flying
//! saucer, a hero streaking past). All of it comic art drawn with egui over
//! the live town, with its own sounds. Nothing here touches the game.

use super::{ease_out_back, hash01, logo_family, mix, smoothstep, star, with_alpha, NAVY};
use engine::prelude::*;
use engine::ui::egui::{
    self, epaint::Vertex, pos2, vec2, Align2, Color32, FontId, Mesh, Pos2, Rect, Sense, Stroke,
    TextureHandle, Vec2,
};
use std::f32::consts::{PI, TAU};
use std::sync::mpsc::{channel, Receiver};

/// Seconds the cover's circle takes to open onto the town.
pub const REVEAL_SECONDS: f32 = 0.9;
/// When the logo has landed on the cover (the heavy loading may run then).
pub const LANDED: f32 = 1.45;

/// The comic pictures, decoded off the main thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Pic {
    Robot,
    Alien,
    Villain,
    Octo,
    Saucer,
    Hero,
}

const PICS: [(Pic, &str, &[u8]); 6] = [
    (
        Pic::Robot,
        "robot",
        include_bytes!("../../assets/title/fun/robot.png"),
    ),
    (
        Pic::Alien,
        "alien",
        include_bytes!("../../assets/title/fun/alien.png"),
    ),
    (
        Pic::Villain,
        "villain",
        include_bytes!("../../assets/title/fun/villain.png"),
    ),
    (
        Pic::Octo,
        "octo",
        include_bytes!("../../assets/title/fun/octo.png"),
    ),
    (
        Pic::Saucer,
        "saucer",
        include_bytes!("../../assets/title/fun/saucer.png"),
    ),
    (
        Pic::Hero,
        "hero",
        include_bytes!("../../assets/title/fun/hero.png"),
    ),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunSfx {
    Roar,
    Splat,
    Thunder,
    Boing,
    Pow,
    Ufo,
    Whee,
    Tentacle,
    Explode,
}

fn sound_bytes(s: FunSfx) -> &'static [u8] {
    match s {
        FunSfx::Roar => include_bytes!("../../assets/title/fun/roar.ogg"),
        FunSfx::Splat => include_bytes!("../../assets/title/fun/splat.ogg"),
        FunSfx::Thunder => include_bytes!("../../assets/title/fun/thunder.ogg"),
        FunSfx::Boing => include_bytes!("../../assets/title/fun/boing.ogg"),
        FunSfx::Pow => include_bytes!("../../assets/title/fun/pow.ogg"),
        FunSfx::Ufo => include_bytes!("../../assets/title/fun/ufo.ogg"),
        FunSfx::Whee => include_bytes!("../../assets/title/fun/whee.ogg"),
        FunSfx::Tentacle => include_bytes!("../../assets/title/fun/tentacle.ogg"),
        FunSfx::Explode => include_bytes!("../../assets/title/fun/explode.ogg"),
    }
}

const ALL_SFX: [FunSfx; 9] = [
    FunSfx::Roar,
    FunSfx::Splat,
    FunSfx::Thunder,
    FunSfx::Boing,
    FunSfx::Pow,
    FunSfx::Ufo,
    FunSfx::Whee,
    FunSfx::Tentacle,
    FunSfx::Explode,
];

/// Who turns up, and how.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisitKind {
    /// A giant robot rises from the bottom of the screen, roaring.
    Robot,
    /// An alien slams into the glass and slides down it.
    Alien,
    /// Lightning, and the villain is suddenly there, cackling.
    Villain,
    /// A space octopus rises at the side, waving.
    Octo,
    /// A flying saucer zooms past, close.
    Saucer,
    /// A hero streaks across the sky.
    Hero,
}

const VISIT_ORDER: [VisitKind; 9] = [
    VisitKind::Hero,
    VisitKind::Robot,
    VisitKind::Saucer,
    VisitKind::Alien,
    VisitKind::Hero,
    VisitKind::Octo,
    VisitKind::Villain,
    VisitKind::Saucer,
    VisitKind::Robot,
];

impl VisitKind {
    fn pic(self) -> Pic {
        match self {
            VisitKind::Robot => Pic::Robot,
            VisitKind::Alien => Pic::Alien,
            VisitKind::Villain => Pic::Villain,
            VisitKind::Octo => Pic::Octo,
            VisitKind::Saucer => Pic::Saucer,
            VisitKind::Hero => Pic::Hero,
        }
    }

    /// How long the visit lasts if nobody knocks the visitor away.
    fn length(self) -> f32 {
        match self {
            VisitKind::Robot => 4.2,
            VisitKind::Alien => 3.4,
            VisitKind::Villain => 3.0,
            VisitKind::Octo => 4.6,
            VisitKind::Saucer => 3.2,
            VisitKind::Hero => 2.6,
        }
    }

    fn sound(self) -> FunSfx {
        match self {
            VisitKind::Robot => FunSfx::Roar,
            VisitKind::Alien => FunSfx::Splat,
            VisitKind::Villain => FunSfx::Thunder,
            VisitKind::Octo => FunSfx::Tentacle,
            VisitKind::Saucer => FunSfx::Ufo,
            VisitKind::Hero => FunSfx::Whee,
        }
    }

    /// A scare shakes the screen when it lands.
    fn shake(self) -> f32 {
        match self {
            VisitKind::Robot => 1.0,
            VisitKind::Alien => 0.7,
            VisitKind::Villain => 0.8,
            VisitKind::Octo => 0.5,
            VisitKind::Saucer | VisitKind::Hero => 0.0,
        }
    }
}

struct Visit {
    kind: VisitKind,
    start: f32,
    /// Knocked away by a click: when, and which way it spins off.
    knocked: Option<(f32, f32)>,
    /// Left or right side (or direction of travel).
    side: f32,
}

/// A comic sound word: POW!, ZAP!, KABOOM!
struct Word {
    text: &'static str,
    at: Pos2,
    born: f32,
    size: f32,
    tilt: f32,
    color: Color32,
}

/// A twinkle left behind by the pointer or a burst.
struct Twinkle {
    at: Pos2,
    vel: Vec2,
    born: f32,
    life: f32,
    color: Color32,
    size: f32,
}

/// A decoded picture: which one, its size and RGBA bytes.
type Decoded = (Pic, [usize; 2], Vec<u8>);

pub struct Fun {
    still: bool,
    pics: Option<Receiver<Vec<Decoded>>>,
    textures: Vec<(Pic, TextureHandle)>,
    sounds: Vec<(FunSfx, Sound)>,
    pending: Vec<FunSfx>,
    visit: Option<Visit>,
    next_visit: f32,
    visits: usize,
    words: Vec<Word>,
    twinkles: Vec<Twinkle>,
    last_pointer: Option<Pos2>,
    /// When each logo letter was last poked.
    pokes: [f32; 9],
    /// The logo flies apart and back together from this time.
    pub regroup: Option<f32>,
    shake_at: f32,
    shake_amount: f32,
    /// A white flash (lightning) at this time.
    flash: f32,
    rng: u32,
    pub knocked: u32,
}

impl Fun {
    pub fn new(still: bool) -> Self {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let mut out = Vec::new();
            for (pic, name, bytes) in PICS {
                match image::load_from_memory_with_format(bytes, image::ImageFormat::Png) {
                    Ok(img) => {
                        let rgba = img.to_rgba8();
                        let size = [rgba.width() as usize, rgba.height() as usize];
                        out.push((pic, size, rgba.into_raw()));
                    }
                    Err(e) => eprintln!("title picture {name}: {e}"),
                }
            }
            let _ = tx.send(out);
        });
        let sounds = ALL_SFX
            .iter()
            .filter_map(|&s| match Sound::from_bytes(sound_bytes(s).to_vec()) {
                Ok(snd) => Some((s, snd)),
                Err(e) => {
                    eprintln!("title sound {s:?}: {e}");
                    None
                }
            })
            .collect();
        // Visitors come at different moments each time (a whim, not the world).
        let rng = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.subsec_nanos() | 1);
        // HW_TITLE_VISIT=<robot|alien|villain|octo|saucer|hero> brings that
        // visitor as soon as the town shows (for screenshots).
        let forced = std::env::var("HW_TITLE_VISIT").ok().and_then(|v| {
            VISIT_ORDER
                .iter()
                .position(|k| format!("{k:?}").eq_ignore_ascii_case(&v))
        });
        Self {
            still,
            pics: Some(rx),
            textures: Vec::new(),
            sounds,
            pending: Vec::new(),
            visit: None,
            next_visit: if forced.is_some() { 0.0 } else { 9.0 },
            visits: forced.unwrap_or((rng % 3) as usize),
            words: Vec::new(),
            twinkles: Vec::new(),
            last_pointer: None,
            pokes: [f32::NEG_INFINITY; 9],
            regroup: None,
            shake_at: f32::NEG_INFINITY,
            shake_amount: 0.0,
            flash: f32::NEG_INFINITY,
            rng,
            knocked: 0,
        }
    }

    pub fn set_still(&mut self, still: bool) {
        self.still = still;
        if still {
            self.visit = None;
        }
    }

    fn rand(&mut self) -> f32 {
        // xorshift: playful timing only.
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x & 0xFF_FFFF) as f32 / 0x100_0000 as f32
    }

    fn texture(&self, pic: Pic) -> Option<&TextureHandle> {
        self.textures
            .iter()
            .find(|(p, _)| *p == pic)
            .map(|(_, t)| t)
    }

    /// Plays what happened since the last frame.
    pub fn play_pending(&mut self, world: &mut World) {
        for s in self.pending.drain(..) {
            if let Some((_, snd)) = self.sounds.iter().find(|(k, _)| *k == s) {
                let vol = match s {
                    FunSfx::Boing | FunSfx::Pow => 0.7,
                    FunSfx::Whee => 0.6,
                    _ => 0.85,
                };
                world.play_sound(snd, PlaySound::ui().volume(vol));
            }
        }
    }

    pub fn sfx(&mut self, s: FunSfx) {
        self.pending.push(s);
    }

    /// How hard the screen shakes now (0..1), for the camera and the logo.
    pub fn shake(&self, t: f32) -> f32 {
        if self.still {
            return 0.0;
        }
        let k = (t - self.shake_at) / 0.6;
        if (0.0..1.0).contains(&k) {
            self.shake_amount * (1.0 - k) * (1.0 - k)
        } else {
            0.0
        }
    }

    /// A small jitter for things that shake.
    pub fn jitter(&self, t: f32, scale: f32) -> Vec2 {
        let s = self.shake(t);
        if s <= 0.0 {
            return Vec2::ZERO;
        }
        vec2((t * 91.0).sin(), (t * 77.0 + 1.3).cos()) * s * scale
    }

    fn bang(&mut self, t: f32, amount: f32) {
        self.shake_at = t;
        self.shake_amount = amount;
    }

    /// A comic word bursting out at `at`.
    pub fn word(&mut self, t: f32, at: Pos2, size: f32) {
        const WORDS: [&str; 10] = [
            "POW!", "ZAP!", "BAM!", "WHAM!", "KAPOW!", "ZOOM!", "BLAM!", "BOOM!", "THWACK!",
            "SPLAT!",
        ];
        const COLORS: [Color32; 5] = [
            Color32::from_rgb(255, 214, 40),
            Color32::from_rgb(255, 90, 60),
            Color32::from_rgb(110, 230, 255),
            Color32::from_rgb(255, 130, 220),
            Color32::from_rgb(140, 255, 120),
        ];
        let text = WORDS[(self.rand() * WORDS.len() as f32) as usize % WORDS.len()];
        let color = COLORS[(self.rand() * COLORS.len() as f32) as usize % COLORS.len()];
        let tilt = (self.rand() - 0.5) * 0.5;
        self.words.push(Word {
            text,
            at,
            born: t,
            size,
            tilt,
            color,
        });
        if self.words.len() > 8 {
            self.words.remove(0);
        }
    }

    fn burst(&mut self, t: f32, at: Pos2, n: usize) {
        for i in 0..n {
            let a = self.rand() * TAU;
            let speed = 120.0 + self.rand() * 360.0;
            let color = match i % 3 {
                0 => Color32::from_rgb(255, 230, 120),
                1 => Color32::from_rgb(120, 236, 255),
                _ => Color32::from_rgb(255, 150, 225),
            };
            let life = 0.6 + self.rand() * 0.6;
            let size = 4.0 + self.rand() * 7.0;
            self.twinkles.push(Twinkle {
                at,
                vel: vec2(a.cos(), a.sin()) * speed,
                born: t,
                life,
                color,
                size,
            });
        }
    }

    /// The sky or the town was clicked: something goes POW.
    pub fn clicked_scene(&mut self, t: f32, at: Pos2) {
        self.word(t, at, 1.0);
        self.burst(t, at, 18);
        self.sfx(FunSfx::Pow);
    }

    /// A logo letter was clicked: it jumps and spins with a boing. Poke all
    /// nine in a row and the logo blows apart and flies back together.
    pub fn poke_letter(&mut self, t: f32, i: usize, at: Pos2) {
        if i >= self.pokes.len() || t - self.pokes[i] < 0.5 {
            return;
        }
        self.pokes[i] = t;
        self.burst(t, at, 8);
        if self.pokes.iter().all(|&p| t - p < 8.0) && self.regroup.is_none_or(|r| t - r > 3.0) {
            self.regroup = Some(t);
            self.pokes = [f32::NEG_INFINITY; 9];
            self.sfx(FunSfx::Explode);
            self.words.push(Word {
                text: "KABOOM!",
                at,
                born: t,
                size: 1.8,
                tilt: -0.12,
                color: Color32::from_rgb(255, 214, 40),
            });
            self.burst(t, at, 60);
            self.bang(t, 0.8);
        } else {
            self.sfx(FunSfx::Boing);
        }
    }

    /// Extra jump and spin of logo letter `i` from a poke: (offset, spin, squash).
    pub fn letter_extra(&self, i: usize, t: f32, em: f32) -> (Vec2, f32, f32) {
        let k = (t - self.pokes.get(i).copied().unwrap_or(f32::NEG_INFINITY)) / 0.7;
        if !(0.0..1.0).contains(&k) {
            return (Vec2::ZERO, 0.0, 1.0);
        }
        let jump = (k * PI).sin() * em * 0.55;
        let spin = if i.is_multiple_of(2) { TAU } else { -TAU } * smoothstep(k);
        let squash = 1.0 + 0.25 * (k * TAU * 1.5).sin() * (1.0 - k);
        (vec2(0.0, -jump), spin, squash)
    }

    /// Per frame: textures arrive, visitors come and go, twinkles follow the
    /// pointer. `open` is true once the cover has opened onto the town.
    pub fn update(&mut self, ctx: &egui::Context, t: f32, dt: f32, open: bool) {
        if let Some(rx) = &self.pics {
            if let Ok(list) = rx.try_recv() {
                for (pic, size, rgba) in list {
                    let img = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
                    let tex = ctx.load_texture(
                        format!("title-fun-{pic:?}"),
                        img,
                        egui::TextureOptions::LINEAR,
                    );
                    self.textures.push((pic, tex));
                }
                self.pics = None;
            }
        }
        // Twinkles trail the pointer.
        if let Some(p) = ctx.pointer_hover_pos() {
            if let Some(last) = self.last_pointer {
                let moved = (p - last).length();
                if moved > 6.0 && self.twinkles.len() < 160 {
                    let n = (moved / 18.0).ceil().min(4.0) as usize;
                    for k in 0..n {
                        let a = self.rand() * TAU;
                        let q = last + (p - last) * (k as f32 + 0.5) / n as f32;
                        let color = if self.rand() < 0.5 {
                            Color32::from_rgb(120, 236, 255)
                        } else {
                            Color32::from_rgb(255, 225, 130)
                        };
                        let size = 3.0 + self.rand() * 4.0;
                        self.twinkles.push(Twinkle {
                            at: q,
                            vel: vec2(a.cos(), a.sin()) * 30.0 + vec2(0.0, 25.0),
                            born: t,
                            life: 0.55,
                            color,
                            size,
                        });
                    }
                }
            }
            self.last_pointer = Some(p);
        }
        for tw in &mut self.twinkles {
            tw.at += tw.vel * dt;
            tw.vel *= 1.0 - (dt * 2.5).min(0.5);
            tw.vel.y += 120.0 * dt;
        }
        self.twinkles.retain(|tw| t - tw.born < tw.life);
        self.words.retain(|w| t - w.born < 1.3);

        // Visitors, once the town is showing.
        if self.still || !open {
            return;
        }
        if let Some(v) = &self.visit {
            let end = match v.knocked {
                Some((at, _)) => at + 0.9,
                None => v.start + v.kind.length(),
            };
            if t > end {
                self.visit = None;
                self.next_visit = t + 14.0 + self.rand() * 16.0;
            }
        } else if t > self.next_visit && !self.textures.is_empty() {
            let kind = VISIT_ORDER[self.visits % VISIT_ORDER.len()];
            self.visits += 1;
            let side = if self.rand() < 0.5 { -1.0 } else { 1.0 };
            self.visit = Some(Visit {
                kind,
                start: t,
                knocked: None,
                side,
            });
            self.sfx(kind.sound());
            if kind == VisitKind::Villain {
                self.flash = t;
            }
        }
        // The scare lands: the screen shakes.
        if let Some(v) = &self.visit {
            let land = match v.kind {
                VisitKind::Robot => 0.45,
                VisitKind::Alien => 0.18,
                VisitKind::Villain => 0.05,
                VisitKind::Octo => 0.6,
                _ => f32::INFINITY,
            };
            let s = v.kind.shake();
            if v.knocked.is_none() && t - v.start >= land && t - v.start - dt < land && s > 0.0 {
                self.bang(t, s);
            }
        }
    }

    /// Where the visitor is now: (centre, size, rotation, alpha), on a
    /// screen `screen` big.
    fn visit_pose(&self, v: &Visit, t: f32, screen: Rect, aspect: f32) -> (Pos2, Vec2, f32, f32) {
        let (w, h) = (screen.width(), screen.height());
        let u = t - v.start;
        let len = v.kind.length();
        let out = smoothstep((u - (len - 0.6)) / 0.6);
        let (mut c, size, mut rot, mut alpha) = match v.kind {
            VisitKind::Robot => {
                let s = h * 0.95;
                let rise = ease_out_back((u / 0.55).clamp(0.0, 1.0));
                // Right of the button column, so the menu stays usable.
                let x = w * (0.7 + 0.08 * v.side);
                let y = h + s * 0.5 - s * 0.82 * rise + s * 0.9 * out;
                let wobble = (u * 6.0).sin() * 0.03 * (1.0 - out);
                (pos2(x, y), vec2(s * aspect, s), wobble, 1.0)
            }
            VisitKind::Alien => {
                // From far away straight at the glass, then sliding down it.
                let k = (u / 0.2).clamp(0.0, 1.0);
                let s = h * (0.08 + 1.05 * k * k);
                let slide = smoothstep((u - 1.4) / (len - 1.4)) * h * 1.2;
                let x = w * (0.5 + 0.12 * v.side);
                (
                    pos2(x, h * 0.5 + slide),
                    vec2(s * aspect, s),
                    0.05 * v.side,
                    1.0,
                )
            }
            VisitKind::Villain => {
                let s = h * 0.85;
                let x = if v.side > 0.0 { w * 0.8 } else { w * 0.62 };
                let a = 1.0 - out;
                let laugh = (u * 14.0).sin() * 0.025;
                (pos2(x, h - s * 0.48), vec2(s * aspect, s), laugh, a)
            }
            VisitKind::Octo => {
                let s = h * 0.75;
                let rise = ease_out_back((u / 0.7).clamp(0.0, 1.0));
                let x = if v.side > 0.0 { w * 0.84 } else { w * 0.58 };
                let y = h + s * 0.5 - s * 0.7 * rise + s * 0.8 * out;
                let wave = (u * 3.0).sin() * 0.08;
                (pos2(x, y), vec2(s * aspect, s), wave, 1.0)
            }
            VisitKind::Saucer => {
                let k = u / len;
                let s = h * (0.25 + 0.55 * (k * PI).sin());
                let x = if v.side > 0.0 {
                    -s + (w + 2.0 * s) * k
                } else {
                    w + s - (w + 2.0 * s) * k
                };
                let y = h * (0.22 + 0.18 * (k * TAU).sin());
                (pos2(x, y), vec2(s * aspect, s), -0.15 * v.side, 1.0)
            }
            VisitKind::Hero => {
                let k = u / len;
                let s = h * 0.22;
                let x = -s * aspect + (w + 2.0 * s * aspect) * k;
                let y = h * (0.42 - 0.22 * k) + (k * TAU * 1.5).sin() * h * 0.02;
                (pos2(x, y), vec2(s * aspect, s), -0.12, 1.0)
            }
        };
        // Knocked away: spins off over the edge.
        if let Some((at, dir)) = v.knocked {
            let k = ((t - at) / 0.9).clamp(0.0, 1.0);
            c += vec2(dir * w * 0.9 * k, -h * 0.6 * k + h * 1.4 * k * k);
            rot += dir * TAU * 1.5 * k;
            alpha *= 1.0 - smoothstep((k - 0.6) / 0.4);
        }
        (c, size, rot, alpha)
    }

    /// Draws the visitor, the words and the twinkles on top of the title,
    /// and knocks the visitor away when clicked.
    pub fn draw_front(&mut self, ctx: &egui::Context, t: f32) {
        let screen = ctx.screen_rect();
        let mut clicked = None;
        egui::Area::new(egui::Id::new("title fun"))
            .fixed_pos(Pos2::ZERO)
            .order(egui::Order::Foreground)
            .interactable(self.visit.is_some())
            .show(ctx, |ui| {
                let painter = ui.painter().clone();
                // Lightning: a white flash for the villain.
                let f = (t - self.flash) / 0.5;
                if (0.0..1.0).contains(&f) {
                    let a = if f < 0.12 || (0.25..0.35).contains(&f) {
                        0.85
                    } else {
                        0.35 * (1.0 - f)
                    };
                    painter.rect_filled(screen, 0.0, with_alpha(Color32::WHITE, a));
                }
                if let Some(v) = &self.visit {
                    if let Some(tex) = self.texture(v.kind.pic()) {
                        let [tw, th] = tex.size();
                        let aspect = tw as f32 / th.max(1) as f32;
                        let (c, size, rot, alpha) = self.visit_pose(v, t, screen, aspect);
                        let c = c + self.jitter(t, 10.0);
                        if v.kind == VisitKind::Hero || v.kind == VisitKind::Saucer {
                            speed_lines(
                                &painter,
                                c,
                                size,
                                if v.kind == VisitKind::Hero {
                                    1.0
                                } else {
                                    v.side
                                },
                                alpha,
                            );
                        }
                        image_quad(&painter, tex.id(), c, size, rot, alpha);
                        if v.knocked.is_none() && alpha > 0.5 {
                            let hit = Rect::from_center_size(c, size * 0.7);
                            let r =
                                ui.interact(hit, egui::Id::new("title visitor"), Sense::click());
                            if r.clicked() {
                                clicked = Some(r.interact_pointer_pos().unwrap_or(c));
                            }
                            r.on_hover_cursor(egui::CursorIcon::Crosshair);
                        }
                    }
                }
                for tw in &self.twinkles {
                    let k = (t - tw.born) / tw.life;
                    star(
                        &painter,
                        tw.at,
                        tw.size * (1.0 - k * 0.5),
                        with_alpha(tw.color, 1.0 - k),
                    );
                }
                for w in &self.words {
                    comic_word(&painter, w, t);
                }
            });
        if let Some(at) = clicked {
            if let Some(v) = self.visit.as_mut() {
                let dir = if at.x < screen.center().x { 1.0 } else { -1.0 };
                v.knocked = Some((t, dir));
                self.knocked += 1;
            }
            self.word(t, at, 1.5);
            self.burst(t, at, 30);
            self.sfx(FunSfx::Pow);
            self.bang(t, 0.4);
        }
    }
}

/// A textured quad, turned `rot` radians about its centre.
fn image_quad(
    painter: &egui::Painter,
    tex: egui::TextureId,
    c: Pos2,
    size: Vec2,
    rot: f32,
    alpha: f32,
) {
    if alpha <= 0.0 {
        return;
    }
    let (s, co) = rot.sin_cos();
    let half = size * 0.5;
    let mut m = Mesh::with_texture(tex);
    let tint = with_alpha(Color32::WHITE, alpha);
    for (dx, dy, u, v) in [
        (-1.0, -1.0, 0.0, 0.0),
        (1.0, -1.0, 1.0, 0.0),
        (-1.0, 1.0, 0.0, 1.0),
        (1.0, 1.0, 1.0, 1.0),
    ] {
        let d = vec2(dx * half.x, dy * half.y);
        m.vertices.push(Vertex {
            pos: c + vec2(d.x * co - d.y * s, d.x * s + d.y * co),
            uv: pos2(u, v),
            color: tint,
        });
    }
    m.add_triangle(0, 1, 2);
    m.add_triangle(2, 1, 3);
    painter.add(egui::Shape::mesh(m));
}

/// Comic speed lines streaming behind something moving along x.
fn speed_lines(painter: &egui::Painter, c: Pos2, size: Vec2, dir: f32, alpha: f32) {
    for i in 0..7 {
        let y = c.y + (i as f32 - 3.0) * size.y * 0.09;
        let len = size.x * (0.5 + 0.35 * hash01(i, 3));
        let x0 = c.x - dir * size.x * 0.35;
        painter.line_segment(
            [pos2(x0, y), pos2(x0 - dir * len, y)],
            Stroke::new(3.0_f32, with_alpha(Color32::WHITE, 0.55 * alpha)),
        );
    }
}

/// A spiky starburst with a word on it, popping in and fading.
fn comic_word(painter: &egui::Painter, w: &Word, t: f32) {
    let u = t - w.born;
    let pop = ease_out_back((u / 0.25).clamp(0.0, 1.0));
    let fade = 1.0 - smoothstep((u - 0.9) / 0.4);
    if pop <= 0.0 || fade <= 0.0 {
        return;
    }
    let r = 58.0 * w.size * pop;
    let n = 14;
    let pts: Vec<Pos2> = (0..n * 2)
        .map(|k| {
            let a = w.tilt + k as f32 * PI / n as f32;
            let rr = if k % 2 == 0 { r } else { r * 0.62 };
            w.at + vec2(a.cos() * rr * 1.25, a.sin() * rr)
        })
        .collect();
    // Concave star: a fan of triangles from the centre, with a dark rim.
    let mut rim = Mesh::default();
    let mut face = Mesh::default();
    let edge = with_alpha(NAVY, fade);
    let fill = with_alpha(w.color, fade);
    rim.colored_vertex(w.at, edge);
    face.colored_vertex(w.at, fill);
    for p in &pts {
        let d = *p - w.at;
        rim.colored_vertex(w.at + d * 1.09, edge);
        face.colored_vertex(*p, fill);
    }
    for k in 0..pts.len() as u32 {
        let next = (k + 1) % pts.len() as u32;
        rim.add_triangle(0, k + 1, next + 1);
        face.add_triangle(0, k + 1, next + 1);
    }
    painter.add(egui::Shape::mesh(rim));
    painter.add(egui::Shape::mesh(face));
    let font = FontId::new(r * 0.55, logo_family());
    let o = r * 0.05;
    painter.text(
        w.at + vec2(o, o),
        Align2::CENTER_CENTER,
        w.text,
        font.clone(),
        with_alpha(NAVY, fade),
    );
    painter.text(
        w.at,
        Align2::CENTER_CENTER,
        w.text,
        font,
        with_alpha(mix(Color32::WHITE, w.color, 0.15), fade),
    );
}

/// The comic cover the title opens on: red and gold rays turning, halftone
/// dots, a dark rim. `open` (0..1) is how far its circle has opened onto
/// the town behind it.
pub fn cover(painter: &egui::Painter, screen: Rect, t: f32, open: f32, still: bool) {
    if open >= 1.0 {
        return;
    }
    let c = screen.center() + vec2(0.0, -screen.height() * 0.08);
    let far = screen.size().length();
    let hole = far * smoothstep(open) * 0.75;
    let spin = if still { 0.0 } else { t * 0.12 };
    let n = 28;
    let mut m = Mesh::default();
    let inner = hole.max(0.0);
    for i in 0..n {
        let a0 = spin + i as f32 * TAU / n as f32;
        let a1 = a0 + TAU / n as f32;
        let col = if i % 2 == 0 {
            Color32::from_rgb(232, 52, 44)
        } else {
            Color32::from_rgb(255, 176, 40)
        };
        let k = m.vertices.len() as u32;
        let p = |a: f32, r: f32| c + vec2(a.cos(), a.sin()) * r;
        let deep = mix(col, Color32::from_rgb(120, 20, 40), 0.55);
        m.colored_vertex(p(a0, inner), col);
        m.colored_vertex(p(a1, inner), col);
        m.colored_vertex(p(a0, far), deep);
        m.colored_vertex(p(a1, far), deep);
        m.add_triangle(k, k + 1, k + 2);
        m.add_triangle(k + 2, k + 1, k + 3);
    }
    painter.add(egui::Shape::mesh(m));
    // Halftone dots in rings, bigger towards the edge.
    let step = 26.0;
    let mut y = screen.top();
    let mut row = 0;
    while y < screen.bottom() + step {
        let mut x = screen.left() + if row % 2 == 0 { 0.0 } else { step * 0.5 };
        while x < screen.right() + step {
            let p = pos2(x, y);
            let d = (p - c).length();
            if d > inner + 4.0 {
                let r = (d / far * 9.0).clamp(1.0, 7.0);
                painter.circle_filled(p, r, Color32::from_rgba_unmultiplied(90, 10, 30, 70));
            }
            x += step;
        }
        y += step;
        row += 1;
    }
    // The rim of the opening circle: a thick navy ring, like a comic panel.
    if inner > 1.0 {
        painter.circle_stroke(c, inner, Stroke::new(10.0_f32, NAVY));
        painter.circle_stroke(
            c,
            inner + 8.0,
            Stroke::new(4.0_f32, Color32::from_rgb(255, 230, 120)),
        );
    }
}

/// Lines the cover shows while the town is made.
pub fn loading_line(t: f32) -> &'static str {
    const LINES: [&str; 8] = [
        "Polishing the chrome...",
        "Teaching the taxis to fly...",
        "Waking up the villains...",
        "Ironing the capes...",
        "Charging the Aether coils...",
        "Hiding the secret lairs...",
        "Feeding the space octopus...",
        "Counting the citizens...",
    ];
    LINES[((t / 1.6) as usize) % LINES.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pokes_jump_and_settle() {
        let mut f = Fun::new(false);
        f.poke_letter(1.0, 2, Pos2::ZERO);
        let (off, spin, _) = f.letter_extra(2, 1.35, 100.0);
        assert!(off.y < -40.0 && spin.abs() > 0.5);
        assert_eq!(f.letter_extra(2, 3.0, 100.0), (Vec2::ZERO, 0.0, 1.0));
        assert_eq!(f.letter_extra(3, 1.35, 100.0), (Vec2::ZERO, 0.0, 1.0));
    }

    #[test]
    fn nine_pokes_blow_the_logo_apart() {
        let mut f = Fun::new(false);
        for i in 0..9 {
            f.poke_letter(1.0 + i as f32 * 0.6, i, Pos2::ZERO);
        }
        assert!(f.regroup.is_some());
        assert!(f.pending.contains(&FunSfx::Explode));
    }

    #[test]
    fn still_means_no_shaking() {
        let mut f = Fun::new(true);
        f.bang(1.0, 1.0);
        assert_eq!(f.shake(1.1), 0.0);
    }
}
