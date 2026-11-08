//! The title screen: the first thing anyone sees.
//!
//! A live backdrop, not a picture: the town itself at golden hour, seen from
//! a camera drifting slowly round its Aether Spire, with flying taxis
//! crossing and two moons up. Over it the HEROWORLD logo as a golden-age
//! comic title (chrome-gold letters with a red block shadow that swoop in
//! with a whoosh, a glint running over them now and then, Aether sparks and
//! a turning sunburst behind), a few big friendly buttons, the title theme,
//! and the loading bar of the town being built behind it all.
//!
//! Everything is drawn with egui shapes: the letters are the logo font's
//! glyphs cut into strips, so each strip can take its own colour.

use crate::creator::Creator;
use crate::profile::{clean_name, Profile};
use actors_generator::{CharacterCatalog, CharacterConfig};
use engine::anim::AnimatedModel;
use engine::prelude::*;
use engine::ui::egui::{
    self, epaint::Vertex, pos2, vec2, Align2, Color32, CornerRadius, FontFamily, FontId, Mesh,
    Pos2, Rect, Sense, Stroke, StrokeKind, Vec2,
};
use glam::{DVec2, Vec3};
use hw_gen::urban::landmarks::LandmarkKind;
use hw_gen::urban::props::{CarModel, PropKind};
use hw_gen::urban::world::TownWorld;
use hw_sim::collision::Obstacles;
use std::f32::consts::TAU;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

/// The hour the title is set at: the sun low and gold over the town.
pub const TITLE_HOUR: f32 = 17.85;
/// Seconds per full circle of the title camera.
const ORBIT_TIME: f32 = 480.0;

const LOGO_FONT: &[u8] = include_bytes!("../assets/fonts/LuckiestGuy-Regular.ttf");
const UI_FONT: &[u8] = include_bytes!("../assets/fonts/TitanOne-Regular.ttf");
const THEME: &[u8] = include_bytes!("../assets/title/title_theme.ogg");

const NAVY: Color32 = Color32::from_rgb(22, 26, 64);
const CREAM: Color32 = Color32::from_rgb(255, 246, 222);
const HERO_RED: Color32 = Color32::from_rgb(226, 52, 48);
const GOLD: Color32 = Color32::from_rgb(255, 198, 64);
const AETHER: Color32 = Color32::from_rgb(120, 236, 255);

/// What the player picked on the title.
#[derive(Clone, Debug, PartialEq)]
pub enum Choice {
    /// Play the remembered hero in the remembered world.
    Continue,
    /// A new hero: this name and these looks (none without the Actors
    /// packs), in the world with this seed.
    NewHero {
        name: String,
        seed: u64,
        look: Option<Box<CharacterConfig>>,
    },
    /// Open our world to friends (and play in it).
    Host,
    /// Join a friend's world by room code or address.
    Join(String),
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Panel {
    None,
    NewHero,
    Friends,
    Settings,
}

/// The sounds of the title (small, decoded at start).
struct Sounds {
    whoosh: Option<Sound>,
    glint: Option<Sound>,
    hover: Option<Sound>,
    click: Option<Sound>,
    start: Option<Sound>,
}

impl Sounds {
    fn load() -> Self {
        let s = |b: &'static [u8]| match Sound::from_bytes(b.to_vec()) {
            Ok(s) => Some(s),
            Err(e) => {
                eprintln!("title sound: {e}");
                None
            }
        };
        Self {
            whoosh: s(include_bytes!("../assets/title/whoosh.ogg")),
            glint: s(include_bytes!("../assets/title/glint.ogg")),
            hover: s(include_bytes!("../assets/title/hover.ogg")),
            click: s(include_bytes!("../assets/title/click.ogg")),
            start: s(include_bytes!("../assets/title/start.ogg")),
        }
    }
}

#[derive(Clone, Copy)]
enum Sfx {
    Whoosh,
    Glint,
    Hover,
    Click,
    Start,
}

/// A flying taxi crossing the sky behind the title.
struct Taxi {
    entity: EntityId,
    dir: DVec2,
    side: f64,
    height: f64,
    speed: f64,
    phase: f64,
}

pub struct Title {
    pub profile: Profile,
    clock: f32,
    last_time: Option<f32>,
    frames: u32,
    still: bool,
    panel: Panel,
    new_name: String,
    new_seed: String,
    join_code: String,
    /// A line under the buttons: what is happening, or what went wrong.
    pub status: Option<(String, bool)>,
    whats_new: bool,
    hovered: Option<egui::Id>,
    sounds: Sounds,
    music: Option<Receiver<EngineResult<Sound>>>,
    whooshed: bool,
    next_glint: f32,
    /// A choice waiting for the town to finish building.
    queued: Option<Choice>,
    taxis: Vec<Taxi>,
    lean: Vec2,
    /// The camera's turn round the spire (stops while the creator is open).
    orbit: f32,
    /// What heroes can be made of (from the Actors packs, once loaded).
    catalog: Option<Arc<CharacterCatalog>>,
    /// No Actors packs: heroes start without the creator.
    pub no_people: bool,
    /// The creator, while it is open; true when it changes the looks of
    /// the hero we have (else it makes a new one).
    creator: Option<(Creator, bool)>,
    /// A picture setting changed on the Settings card: apply it.
    picture_changed: bool,
    /// Your hero in the foreground: the model shown and the one wanted.
    hero: Option<ShownHero>,
    /// Looks wanted for the hero in the foreground, not yet asked for.
    asked: Option<u64>,
    /// Where the hero stands on the screen (to click or turn them).
    hero_rect: Option<Rect>,
    spin: f32,
}

/// The hero in the title's foreground, on a gold pedestal.
struct ShownHero {
    key: u64,
    entity: EntityId,
    pedestal: EntityId,
    clip: &'static str,
}

impl Title {
    pub fn new(profile: Profile) -> Self {
        // The theme is three minutes: decode it off the main thread.
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(Sound::from_bytes(THEME.to_vec()));
        });
        let start = std::env::var("HW_TITLE_TIME")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(0.0);
        Self {
            new_name: profile.name.clone(),
            new_seed: profile.seed.to_string(),
            still: profile.reduced_motion,
            profile,
            clock: start,
            last_time: None,
            frames: 0,
            // HW_TITLE_PANEL opens a panel at start (for screenshots).
            panel: match std::env::var("HW_TITLE_PANEL").as_deref() {
                Ok("new") => Panel::NewHero,
                Ok("friends") => Panel::Friends,
                Ok("settings") => Panel::Settings,
                _ => Panel::None,
            },
            join_code: String::new(),
            status: None,
            whats_new: false,
            hovered: None,
            sounds: Sounds::load(),
            music: Some(rx),
            whooshed: start > 0.5,
            next_glint: 5.0,
            queued: None,
            taxis: Vec::new(),
            lean: Vec2::ZERO,
            orbit: start,
            catalog: None,
            no_people: false,
            creator: None,
            picture_changed: false,
            hero: None,
            asked: None,
            hero_rect: None,
            spin: 0.0,
        }
    }

    /// The hero catalog arrived (the packs are loaded): the creator opens.
    pub fn set_catalog(&mut self, catalog: CharacterCatalog) {
        let catalog = Arc::new(catalog);
        if std::env::var("HW_TITLE_PANEL").as_deref() == Ok("creator") {
            let look = self.current_look(&catalog);
            self.creator = Some((Creator::new(catalog.clone(), &look), false));
            if let Ok(s) = std::env::var("HW_CREATOR_SECTION") {
                if let Some(c) = self.creator.as_mut() {
                    c.0.section = match s.as_str() {
                        "face" => crate::creator::Section::Face,
                        "costume" => crate::creator::Section::Costume,
                        "colours" => crate::creator::Section::Colours,
                        _ => crate::creator::Section::Body,
                    };
                    c.0.set_detailed(std::env::var_os("HW_CREATOR_DETAILED").is_some());
                }
            }
        }
        // A hero from before the creator: the clothes the game dressed
        // them in by name, worked out once.
        if self.profile.look.is_none() {
            self.profile.look = Some(self.current_look(&catalog));
        }
        self.catalog = Some(catalog);
    }

    /// The looks of the hero we have, or (before the creator existed) the
    /// ones the game dressed them in by their name.
    fn current_look(&self, catalog: &CharacterCatalog) -> CharacterConfig {
        self.profile.look.clone().unwrap_or_else(|| {
            Creator::random_look(catalog, crate::people::name_seed(&self.profile.name))
        })
    }

    /// The looks the foreground hero should wear now: the creator's, or
    /// the hero we have.
    fn shown_look(&self) -> Option<CharacterConfig> {
        match (&self.creator, &self.catalog) {
            (Some((c, _)), _) => Some(c.look()),
            (None, Some(cat)) => Some(self.current_look(cat)),
            _ => None,
        }
    }

    /// Looks to build for the foreground hero, once each: (key, looks).
    pub fn wanted_look(&mut self) -> Option<(u64, CharacterConfig)> {
        let look = self.shown_look()?;
        let key = crate::looks::key(&look);
        if self.asked == Some(key) || self.hero.as_ref().is_some_and(|h| h.key == key) {
            return None;
        }
        self.asked = Some(key);
        Some((key, look))
    }

    /// The key of the looks the foreground hero should wear.
    pub fn wanted_key(&self) -> Option<u64> {
        self.shown_look().map(|l| crate::looks::key(&l))
    }

    /// A built hero arrived: shown if it is the one wanted.
    pub fn hero_built(
        &mut self,
        world: &mut World,
        key: u64,
        model: Arc<AnimatedModel>,
    ) -> EngineResult<()> {
        if self.wanted_key() != Some(key) {
            return Ok(());
        }
        self.drop_hero(world);
        let entity = world.spawn_animated_shared(model, Place::new(0.0, -900.0, 0.0))?;
        world.set_animation_blend(entity, 0.3)?;
        let pedestal = world.place(pedestal_mesh()?, Place::new(0.0, -900.0, 0.0))?;
        self.hero = Some(ShownHero {
            key,
            entity,
            pedestal,
            clip: "",
        });
        Ok(())
    }

    fn drop_hero(&mut self, world: &mut World) {
        if let Some(h) = self.hero.take() {
            world.despawn(h.entity);
            world.despawn(h.pedestal);
        }
    }

    /// Puts the foreground hero where the camera sees them: big beside the
    /// creator, smaller right of the buttons on the title.
    fn place_hero(&mut self, world: &mut World, aspect: f32, screen: Rect) -> EngineResult<()> {
        let creating = self.creator.is_some();
        let (heroic, spin) = match &self.creator {
            Some((c, _)) => (c.heroic, c.spin),
            None => (true, self.spin),
        };
        let Some(h) = self.hero.as_mut() else {
            self.hero_rect = None;
            return Ok(());
        };
        let cam = world.camera().clone();
        let eye = cam.eye();
        let fwd = (cam.target() - eye).normalize_or(Vec3::Z);
        let right = fwd.cross(Vec3::Y).normalize_or(Vec3::X);
        let up = right.cross(fwd);
        let th = (cam.fov_y_degrees().to_radians() * 0.5).tan();
        // How much of the screen's height the hero takes, and where.
        let (fill, x_ndc, y_ndc) = if creating {
            (0.66, 0.36, 0.0)
        } else {
            (0.46, 0.5, -0.1)
        };
        let d = 1.9 / (fill * 2.0 * th);
        let middle = eye + fwd * d + right * (x_ndc * th * aspect * d) + up * (y_ndc * th * d);
        let feet = middle - Vec3::Y * 0.9;
        let to_eye = eye - feet;
        let yaw = to_eye.x.atan2(to_eye.z).to_degrees() + spin;
        world.set_place(
            h.entity,
            Place::new(feet.x, feet.y, feet.z).with_yaw_deg(yaw),
        )?;
        world.set_place(
            h.pedestal,
            Place::new(feet.x, feet.y, feet.z).with_yaw_deg(yaw),
        )?;
        let clip = if heroic { "heroic" } else { "idle" };
        if h.clip != clip {
            h.clip = clip;
            world.play_animation(h.entity, clip)?;
        }
        // Its place on the screen: the box from the pedestal to the head.
        let vp = cam.view_projection(aspect);
        let to_screen = |p: Vec3| {
            let c = vp * p.extend(1.0);
            let n = c.truncate() / c.w.max(1e-4);
            pos2(
                screen.left() + (n.x * 0.5 + 0.5) * screen.width(),
                screen.top() + (0.5 - n.y * 0.5) * screen.height(),
            )
        };
        let a = to_screen(feet - Vec3::Y * 0.3);
        let b = to_screen(feet + Vec3::Y * 1.95);
        let half = (a.y - b.y).abs() * 0.22;
        self.hero_rect = Some(Rect::from_min_max(
            pos2(a.x.min(b.x) - half, b.y.min(a.y)),
            pos2(a.x.max(b.x) + half, a.y.max(b.y)),
        ));
        Ok(())
    }

    /// Sets the scene once: golden hour, the moons, sound levels.
    pub fn enter(&mut self, world: &mut World) {
        world.set_physical_sky(Some(PhysicalSky::at_hour(TITLE_HOUR)));
        self.apply_volumes(world);
    }

    fn apply_volumes(&self, world: &mut World) {
        crate::settings::apply_volumes(world, &self.profile);
    }

    /// The camera's place this frame: (eye, yaw, pitch in degrees).
    pub fn camera(&self, town: &TownWorld, solids: &Obstacles) -> (Vec3, f32, f32) {
        let (c, base, h) = focus(town);
        let r = h * 1.5 + 110.0;
        let turn = if self.still {
            0.0
        } else {
            self.orbit * TAU / ORBIT_TIME
        };
        let a = (0.8 + turn) as f64;
        let p = c + DVec2::new(a.cos(), a.sin()) * r;
        let ground = town.ground(p).max(hw_gen::urban::world::WATER_LEVEL);
        let mut y = (base + h * 0.55).max(ground + 40.0);
        // Never inside a building: rise until clear.
        while solids.solid_at(glam::DVec3::new(p.x, y, p.y)) && y < base + h * 2.0 {
            y += 6.0;
        }
        let d = c - p;
        let yaw_to = d.x.atan2(d.y).to_degrees() as f32;
        let pitch_to = ((base + h * 0.42 - y) / d.length()).atan().to_degrees() as f32;
        // The spire stands right of centre; the buttons get the left.
        let yaw = yaw_to + 16.0 - self.lean.x * 3.0;
        let pitch = pitch_to + 2.0 - self.lean.y * 1.5;
        (Vec3::new(p.x as f32, y as f32, p.y as f32), yaw, pitch)
    }

    /// Flying taxis cross between the camera and the spire.
    fn update_taxis(&mut self, world: &mut World, town: &TownWorld) -> EngineResult<()> {
        let (c, base, h) = focus(town);
        if self.taxis.is_empty() {
            for i in 0..7u8 {
                let mesh = crate::propview::model_levels(PropKind::Car(CarModel::Hover, i))?
                    .into_iter()
                    .next()
                    .map(|l| l.mesh);
                let Some(mesh) = mesh else { continue };
                let entity = world.place(mesh, Place::new(c.x as f32, -500.0, c.y as f32))?;
                let fi = i as f64;
                let ang = fi * 2.4 + 0.3;
                self.taxis.push(Taxi {
                    entity,
                    dir: DVec2::new(ang.cos(), ang.sin()),
                    side: (fi - 3.0) * 38.0,
                    height: h * (0.22 + 0.09 * (i % 5) as f64) + 10.0,
                    speed: 20.0 + 4.0 * (i % 3) as f64,
                    phase: fi * 190.0,
                });
            }
        }
        let t = self.clock as f64;
        for taxi in &self.taxis {
            let span = 1500.0;
            let s = (t * taxi.speed + taxi.phase).rem_euclid(span) - span * 0.5;
            let p = c + taxi.dir * s + taxi.dir.perp() * taxi.side;
            let bob = (t * 1.3 + taxi.phase).sin() * 0.6;
            let yaw = taxi.dir.x.atan2(taxi.dir.y).to_degrees() as f32;
            let place = Place::new(p.x as f32, (base + taxi.height + bob) as f32, p.y as f32)
                .with_yaw_deg(yaw)
                .with_roll_deg((t * 0.9 + taxi.phase).sin() as f32 * 4.0);
            world.set_place(taxi.entity, place)?;
        }
        Ok(())
    }

    /// Leaves the title: the taxis go, the music plays on (the caller fades it).
    pub fn leave(&mut self, world: &mut World) {
        for t in self.taxis.drain(..) {
            world.despawn(t.entity);
        }
        self.drop_hero(world);
    }

    fn play(&self, world: &mut World, s: Sfx) {
        let (sound, vol) = match s {
            Sfx::Whoosh => (&self.sounds.whoosh, 0.9),
            Sfx::Glint => (&self.sounds.glint, 0.45),
            Sfx::Hover => (&self.sounds.hover, 0.35),
            Sfx::Click => (&self.sounds.click, 0.7),
            Sfx::Start => (&self.sounds.start, 0.9),
        };
        if let Some(sound) = sound {
            world.play_sound(sound, PlaySound::ui().volume(vol));
        }
    }

    /// One frame of the title. `loading` is how far the town is built
    /// (0..1) and what is being done. Returns the player's choice, once the
    /// town is ready for it.
    pub fn frame(
        &mut self,
        world: &mut World,
        frame: &Frame,
        town: &TownWorld,
        solids: &Obstacles,
        loading: (f32, &str),
    ) -> EngineResult<Option<Choice>> {
        let now = frame.time;
        let dt = self.last_time.map_or(0.0, |t| (now - t).clamp(0.0, 0.1));
        self.last_time = Some(now);
        self.clock += dt;
        // The camera holds still while you make your hero.
        if self.creator.is_none() {
            self.orbit += dt;
        }
        self.frames += 1;
        if self.frames == 1 {
            install_fonts(frame.ui.ctx());
        }
        // The theme starts with a soft fade as soon as it is decoded.
        if let Some(rx) = &self.music {
            if let Ok(r) = rx.try_recv() {
                match r {
                    Ok(s) => world.play_music(MusicTrack::new().layer(s), 4.0),
                    Err(e) => eprintln!("title music: {e}"),
                }
                self.music = None;
            }
        }
        if !self.still {
            self.update_taxis(world, town)?;
        }
        let (eye, yaw, pitch) = self.camera(town, solids);
        world.look_first_person(eye, yaw, pitch);
        let screen = frame.ui.ctx().screen_rect();
        let aspect = screen.width() / screen.height().max(1.0);
        self.place_hero(world, aspect, screen)?;

        let mut sfx = Vec::new();
        let mut choice = None;
        // The fonts are there from the second frame on.
        if self.frames >= 2 {
            let ready = loading.0 >= 1.0;
            choice = self.draw(frame.ui.ctx(), loading, ready, &mut sfx);
            if self.panel == Panel::Settings {
                self.apply_volumes(world);
            }
            if std::mem::take(&mut self.picture_changed) {
                crate::settings::apply(world, &self.profile);
            }
            if let Some(c) = choice.take() {
                if c == Choice::Quit {
                    world.request_exit();
                } else if ready {
                    choice = Some(c);
                } else {
                    self.queued = Some(c);
                    self.status = Some(("Starting as soon as the city is ready...".into(), false));
                }
            }
            if ready && choice.is_none() {
                choice = self.queued.take();
            }
            // HW_TITLE_AUTOSTART=<s> presses Continue by itself (for tests).
            let auto = std::env::var("HW_TITLE_AUTOSTART")
                .ok()
                .and_then(|s| s.parse::<u32>().ok());
            if ready && choice.is_none() && auto.is_some_and(|f| self.frames >= f) {
                choice = Some(Choice::Continue);
            }
        }
        // The whoosh as the letters fly in, and a glint now and then.
        if !self.whooshed && self.clock > 0.35 && self.frames >= 2 {
            self.whooshed = true;
            if !self.still {
                sfx.push(Sfx::Whoosh);
            }
        }
        if !self.still && self.clock > self.next_glint {
            self.next_glint = self.clock + 7.5;
            sfx.push(Sfx::Glint);
        }
        if choice.is_some() {
            sfx.push(Sfx::Start);
            self.profile.save();
        }
        for s in sfx {
            self.play(world, s);
        }
        Ok(choice)
    }

    fn draw(
        &mut self,
        ctx: &egui::Context,
        loading: (f32, &str),
        ready: bool,
        sfx: &mut Vec<Sfx>,
    ) -> Option<Choice> {
        let mut choice = None;
        let screen = ctx.screen_rect();
        let (w, h) = (screen.width(), screen.height());
        if let Some(p) = ctx.pointer_hover_pos() {
            let target = vec2(p.x / w * 2.0 - 1.0, p.y / h * 2.0 - 1.0);
            self.lean += (target - self.lean) * 0.03;
        }
        let t = if self.still { 30.0 } else { self.clock };
        // Phone-narrow to wide: the column takes the left of the screen.
        let left = (w * 0.06).clamp(16.0, 120.0);
        if self.creator.is_some() {
            return self.creator_screen(ctx, left, sfx);
        }
        let em = (w * 0.085).min(h * 0.15).max(40.0);
        let logo_top = (h * 0.07).max(16.0);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let painter = ui.painter().clone();
                // A soft dark wash on the left keeps the column readable.
                wash(&painter, screen);
                let logo = logo_layout(ctx, em, pos2(left, logo_top));
                sunburst(&painter, logo.rect, t, self.still);
                sparks(&painter, logo.rect, t);
                draw_logo(&painter, &logo, em, t, self.still);
                let tag_y = logo.rect.bottom() + em * 0.28;
                shadow_text(
                    &painter,
                    pos2(left + em * 0.08, tag_y),
                    Align2::LEFT_TOP,
                    "A whole planet needs a hero.",
                    FontId::new((em * 0.24).max(16.0), ui_family()),
                    CREAM,
                );

                // The buttons.
                let bw = (em * 3.3).clamp(240.0, 380.0);
                let bh = (em * 0.5).clamp(44.0, 62.0);
                let gap = bh * 0.28;
                let mut y = tag_y + em * 0.62;
                let played = self.profile.played;
                let (main_label, main_sub) = if played {
                    (
                        "Continue",
                        format!("{} in world {}", self.profile.name, self.profile.seed),
                    )
                } else {
                    (
                        "Play",
                        format!("as {} in world {}", self.profile.name, self.profile.seed),
                    )
                };
                let buttons: [(&str, Option<String>, bool); 6] = [
                    (main_label, Some(main_sub), true),
                    ("New hero", None, false),
                    ("Play with friends", None, false),
                    ("Settings", None, false),
                    ("What's new", None, false),
                    ("Quit", None, false),
                ];
                let mut column = Rect::from_min_size(pos2(left, y), vec2(bw, 0.0));
                for (i, (label, sub, primary)) in buttons.iter().enumerate() {
                    let height = if *primary { bh * 1.3 } else { bh };
                    let rect = Rect::from_min_size(pos2(left, y), vec2(bw, height));
                    // Buttons pop in one after another once the title has landed.
                    let pop = if self.still {
                        1.0
                    } else {
                        ease_out_back(((t - 1.3 - i as f32 * 0.08) / 0.45).clamp(0.0, 1.0))
                    };
                    let id = egui::Id::new(("title button", i));
                    let r = big_button(
                        ui,
                        &painter,
                        rect,
                        id,
                        label,
                        sub.as_deref(),
                        *primary,
                        pop,
                        t,
                    );
                    if r.hovered() && self.hovered != Some(id) {
                        self.hovered = Some(id);
                        sfx.push(Sfx::Hover);
                    } else if !r.hovered() && self.hovered == Some(id) {
                        self.hovered = None;
                    }
                    if r.clicked() {
                        sfx.push(Sfx::Click);
                        let toggle = |p: &mut Panel, to: Panel| {
                            *p = if *p == to { Panel::None } else { to };
                        };
                        match i {
                            0 => choice = Some(Choice::Continue),
                            1 => toggle(&mut self.panel, Panel::NewHero),
                            2 => toggle(&mut self.panel, Panel::Friends),
                            3 => toggle(&mut self.panel, Panel::Settings),
                            4 => self.whats_new = true,
                            _ => choice = Some(Choice::Quit),
                        }
                    }
                    y += height + gap;
                    column.max.y = y;
                }
                if let Some((msg, bad)) = &self.status {
                    shadow_text(
                        &painter,
                        pos2(left, y + 2.0),
                        Align2::LEFT_TOP,
                        msg,
                        FontId::proportional(16.0),
                        if *bad {
                            Color32::from_rgb(255, 150, 140)
                        } else {
                            CREAM
                        },
                    );
                }

                // Small print, and the loading bar.
                shadow_text(
                    &painter,
                    pos2(left, h - 14.0),
                    Align2::LEFT_BOTTOM,
                    &format!(
                        "HeroWorld {}   ·   built on Engine   ·   music made with Lyria",
                        hw_core::version()
                    ),
                    FontId::proportional(13.0),
                    Color32::from_rgb(214, 220, 232),
                );
                loading_bar(&painter, screen, loading, ready, t);

                // The open panel, to the right of the buttons.
                let panel_pos = pos2(column.right() + bw * 0.12, column.top());
                if self.panel != Panel::None {
                    if let Some(c) = self.side_panel(ctx, panel_pos, sfx) {
                        choice = Some(c);
                    }
                }
            });
        // Your hero: drag to turn them, click to change their looks.
        if let (Some(rect), Some(cat)) = (self.hero_rect, self.catalog.clone()) {
            if self.hero.is_some() && self.panel == Panel::None {
                egui::Area::new(egui::Id::new("title hero"))
                    .fixed_pos(rect.min)
                    .order(egui::Order::Background)
                    .show(ctx, |ui| {
                        let r = ui.interact(
                            rect,
                            egui::Id::new("title hero hit"),
                            Sense::click_and_drag(),
                        );
                        self.spin += r.drag_delta().x * 0.5;
                        if r.clicked() {
                            sfx.push(Sfx::Click);
                            let look = self.current_look(&cat);
                            self.creator = Some((Creator::new(cat.clone(), &look), true));
                        }
                        let tip = if r.hovered() {
                            CREAM
                        } else {
                            with_alpha(CREAM, 0.75)
                        };
                        shadow_text(
                            ui.painter(),
                            pos2(rect.center().x, rect.bottom() + 6.0),
                            Align2::CENTER_TOP,
                            "Click your hero to change their looks",
                            FontId::proportional(15.0),
                            tip,
                        );
                        r.on_hover_cursor(egui::CursorIcon::PointingHand);
                    });
            }
        }
        whats_new_modal(ctx, &mut self.whats_new);
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.panel = Panel::None;
        }
        choice
    }

    /// The creator: a tall card on the left, your hero big on the right.
    fn creator_screen(
        &mut self,
        ctx: &egui::Context,
        left: f32,
        sfx: &mut Vec<Sfx>,
    ) -> Option<Choice> {
        let mut choice = None;
        let screen = ctx.screen_rect();
        let (w, h) = (screen.width(), screen.height());
        let top = (h * 0.03).max(10.0);
        let card_w = (w * 0.4).clamp(300.0, 470.0);
        let card_h = h - 2.0 * top;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| wash(ui.painter(), screen));
        let (creator, existing) = self.creator.as_mut()?;
        let existing = *existing;
        let mut close = false;
        egui::Area::new(egui::Id::new("creator card"))
            .fixed_pos(pos2(left.min(40.0), top))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                comic_card_style(ui);
                egui::Frame::new()
                    .fill(CREAM)
                    .stroke(Stroke::new(3.0_f32, NAVY))
                    .corner_radius(14.0)
                    .inner_margin(egui::Margin::same(18))
                    .show(ui, |ui| {
                        ui.set_width(card_w - 36.0);
                        ui.set_height(card_h - 36.0);
                        ui.label(
                            egui::RichText::new(if existing {
                                "Your hero's looks"
                            } else {
                                "Make your hero"
                            })
                            .font(FontId::new(28.0, ui_family()))
                            .color(HERO_RED),
                        );
                        if !existing {
                            ui.horizontal(|ui| {
                                ui.label("Name");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.new_name)
                                        .font(FontId::proportional(18.0))
                                        .desired_width(f32::INFINITY),
                                );
                            });
                        }
                        creator.tabs(ui);
                        ui.separator();
                        let footer = 112.0;
                        egui::ScrollArea::vertical()
                            .max_height((ui.available_height() - footer).max(80.0))
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                creator.ui(ui);
                            });
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui
                                .button("Roll the dice")
                                .on_hover_text("A whole new hero")
                                .clicked()
                            {
                                sfx.push(Sfx::Click);
                                creator.roll(surprise_seed() as u32);
                            }
                            ui.add_space(8.0);
                            ui.selectable_value(&mut creator.heroic, false, "At ease");
                            ui.selectable_value(&mut creator.heroic, true, "Heroic");
                        });
                        ui.horizontal(|ui| {
                            if ui.button("Back").clicked() {
                                sfx.push(Sfx::Click);
                                close = true;
                            }
                            let name = clean_name(&self.new_name);
                            let seed = self.new_seed.trim().parse::<u64>().ok();
                            if existing {
                                if ui.button("Keep these looks").clicked() {
                                    sfx.push(Sfx::Click);
                                    self.profile.look = Some(creator.look());
                                    self.profile.save();
                                    close = true;
                                }
                            } else {
                                let ok = !name.is_empty() && seed.is_some();
                                let go = ui.add_enabled(
                                    ok,
                                    egui::Button::new(
                                        egui::RichText::new("Start the adventure!").color(CREAM),
                                    )
                                    .fill(HERO_RED),
                                );
                                if go.clicked() {
                                    if let Some(seed) = seed {
                                        choice = Some(Choice::NewHero {
                                            name,
                                            seed,
                                            look: Some(Box::new(creator.look())),
                                        });
                                    }
                                }
                            }
                        });
                    });
            });
        // Drag the hero round; a note while their new clothes are made.
        if let Some(rect) = self.hero_rect {
            egui::Area::new(egui::Id::new("creator hero"))
                .fixed_pos(rect.min)
                .order(egui::Order::Background)
                .show(ctx, |ui| {
                    let r = ui.interact(rect, egui::Id::new("creator hero hit"), Sense::drag());
                    creator.spin += r.drag_delta().x * 0.5;
                    shadow_text(
                        ui.painter(),
                        pos2(rect.center().x, rect.bottom() + 6.0),
                        Align2::CENTER_TOP,
                        "Drag to turn",
                        FontId::proportional(15.0),
                        with_alpha(CREAM, 0.8),
                    );
                    r.on_hover_cursor(egui::CursorIcon::Grab);
                });
        }
        let ready = self
            .hero
            .as_ref()
            .is_some_and(|h| Some(h.key) == Some(crate::looks::key(&creator.look())));
        if !ready {
            let at = self.hero_rect.map_or(pos2(w * 0.68, h * 0.5), |r| {
                pos2(r.center().x, r.top() - 10.0)
            });
            egui::Area::new(egui::Id::new("creator tailoring"))
                .fixed_pos(pos2(0.0, 0.0))
                .order(egui::Order::Foreground)
                .interactable(false)
                .show(ctx, |ui| {
                    shadow_text(
                        ui.painter(),
                        at,
                        Align2::CENTER_BOTTOM,
                        "The tailor is at work...",
                        FontId::new(20.0, ui_family()),
                        GOLD,
                    );
                });
        }
        if close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.creator = None;
        }
        choice
    }

    /// The panel a button opened: a cream comic card.
    fn side_panel(&mut self, ctx: &egui::Context, pos: Pos2, sfx: &mut Vec<Sfx>) -> Option<Choice> {
        let mut choice = None;
        egui::Area::new(egui::Id::new("title panel"))
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                comic_card_style(ui);
                egui::Frame::new()
                    .fill(CREAM)
                    .stroke(Stroke::new(3.0_f32, NAVY))
                    .corner_radius(14.0)
                    .inner_margin(egui::Margin::same(20))
                    .show(ui, |ui| {
                        ui.set_width(360.0);
                        let heading = |ui: &mut egui::Ui, s: &str| {
                            ui.label(
                                egui::RichText::new(s)
                                    .font(FontId::new(26.0, ui_family()))
                                    .color(HERO_RED),
                            );
                        };
                        match self.panel {
                            Panel::NewHero => {
                                heading(ui, "New hero");
                                ui.label("Everyone starts as a normal person. What do they call you?");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.new_name)
                                        .font(FontId::proportional(20.0))
                                        .desired_width(f32::INFINITY),
                                );
                                ui.label("Your world. Every number is a different planet.");
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.new_seed)
                                            .font(FontId::proportional(20.0))
                                            .desired_width(150.0),
                                    );
                                    if ui.button("Surprise me").clicked() {
                                        sfx.push(Sfx::Click);
                                        self.new_seed = surprise_seed().to_string();
                                    }
                                });
                                let seed = self.new_seed.trim().parse::<u64>().ok();
                                if seed.is_none() {
                                    ui.colored_label(HERO_RED, "The world is a number, like 7 or 2026.");
                                }
                                ui.add_space(4.0);
                                let name = clean_name(&self.new_name);
                                let ok = seed.is_some() && !name.is_empty();
                                if self.no_people {
                                    // No Actors packs: no creator either.
                                    if ui
                                        .add_enabled(ok, egui::Button::new("Start the adventure"))
                                        .clicked()
                                    {
                                        if let Some(seed) = seed {
                                            choice = Some(Choice::NewHero { name, seed, look: None });
                                        }
                                    }
                                } else if let Some(cat) = self.catalog.clone() {
                                    if ui
                                        .add_enabled(ok, egui::Button::new("Make your hero"))
                                        .clicked()
                                    {
                                        sfx.push(Sfx::Click);
                                        let look = Creator::random_look(
                                            &cat,
                                            crate::people::name_seed(&name) ^ surprise_seed() as u32,
                                        );
                                        self.creator = Some((Creator::new(cat, &look), false));
                                    }
                                } else {
                                    ui.add_enabled(false, egui::Button::new("Make your hero"));
                                    ui.label("The wardrobe is being unpacked...");
                                }
                            }
                            Panel::Friends => {
                                heading(ui, "Play with friends");
                                ui.label("Open your world: friends join with the room code shown at the top right.");
                                if ui.button("Host my world").clicked() {
                                    sfx.push(Sfx::Click);
                                    choice = Some(Choice::Host);
                                }
                                ui.separator();
                                ui.label("Join a friend: type their room code.");
                                let r = ui.add(
                                    egui::TextEdit::singleline(&mut self.join_code)
                                        .font(FontId::new(24.0, ui_family()))
                                        .hint_text("ABC123")
                                        .char_limit(64)
                                        .desired_width(f32::INFINITY),
                                );
                                let enter = r.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                let code = self.join_code.trim().to_string();
                                let go = ui
                                    .add_enabled(!code.is_empty(), egui::Button::new("Join"))
                                    .clicked();
                                if (go || enter) && !code.is_empty() {
                                    sfx.push(Sfx::Click);
                                    choice = Some(Choice::Join(code.to_uppercase_code()));
                                }
                            }
                            Panel::Settings => {
                                heading(ui, "Settings");
                                if crate::settings::settings_ui(ui, &mut self.profile, HERO_RED) {
                                    self.picture_changed = true;
                                }
                                if ui
                                    .checkbox(
                                        &mut self.profile.reduced_motion,
                                        "Keep the title still (less motion)",
                                    )
                                    .changed()
                                {
                                    self.still = self.profile.reduced_motion;
                                }
                                ui.collapsing("Controls", crate::settings::controls_ui);
                                if ui.button("Done").clicked() {
                                    sfx.push(Sfx::Click);
                                    self.profile.save();
                                    self.panel = Panel::None;
                                }
                            }
                            Panel::None => {}
                        }
                    });
            });
        choice
    }
}

/// The cream comic card's look: navy text, gold buttons with a navy rim.
fn comic_card_style(ui: &mut egui::Ui) {
    ui.style_mut().visuals = egui::Visuals::light();
    ui.visuals_mut().override_text_color = Some(NAVY);
    ui.style_mut()
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(17.0));
    ui.style_mut()
        .text_styles
        .insert(egui::TextStyle::Button, FontId::new(18.0, ui_family()));
    ui.style_mut().spacing.button_padding = vec2(14.0, 6.0);
    // Gold buttons with a navy rim, like the big ones.
    let w = &mut ui.visuals_mut().widgets;
    for (state, fill) in [
        (&mut w.inactive, GOLD),
        (&mut w.hovered, Color32::from_rgb(255, 220, 110)),
        (&mut w.active, Color32::from_rgb(240, 170, 40)),
    ] {
        state.weak_bg_fill = fill;
        state.bg_fill = fill;
        state.bg_stroke = Stroke::new(2.0_f32, NAVY);
        state.corner_radius = CornerRadius::same(10);
        state.fg_stroke = Stroke::new(2.0_f32, NAVY);
    }
    ui.style_mut().spacing.item_spacing = vec2(10.0, 10.0);
}

/// The gold pedestal the hero stands on in the title's foreground.
fn pedestal_mesh() -> EngineResult<engine::mesh::Mesh> {
    let mut m = engine::mesh::Mesh::cylinder(
        Vec3::new(0.0, -0.06, 0.0),
        Vec3::Y,
        0.48,
        0.12,
        48,
        Color::rgb(222, 170, 60),
    )?;
    let rim = engine::mesh::Mesh::cylinder(
        Vec3::new(0.0, -0.2, 0.0),
        Vec3::Y,
        0.4,
        0.16,
        48,
        Color::rgb(36, 40, 84),
    )?;
    m.append_translated(&rim, Vec3::ZERO)?;
    Ok(m)
}

/// Room codes are typed in any case; addresses stay as typed.
trait CodeCase {
    fn to_uppercase_code(&self) -> String;
}

impl CodeCase for String {
    fn to_uppercase_code(&self) -> String {
        if self.len() == 6 && self.chars().all(|c| c.is_ascii_alphanumeric()) {
            self.to_ascii_uppercase()
        } else {
            self.clone()
        }
    }
}

/// A new world number from the clock (a player's whim, not world generation).
fn surprise_seed() -> u64 {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(7, |d| d.as_nanos() as u64);
    hw_core::Seed(n).0 % 100_000 + 2
}

/// The spire the camera circles: (centre, plinth height, height). A town
/// without one gets a point above its start.
pub fn focus(town: &TownWorld) -> (DVec2, f64, f64) {
    let (spawn, _) = town.spawn_line();
    match crate::landmarkview::nearest(town, LandmarkKind::AetherSpire, spawn) {
        Some(l) => (l.pos, l.base, l.height),
        None => (spawn, town.ground(spawn), 70.0),
    }
}

fn logo_family() -> FontFamily {
    FontFamily::Name("hw-logo".into())
}

pub fn ui_family() -> FontFamily {
    FontFamily::Name("hw-ui".into())
}

/// Adds the logo and button fonts (they can be used from the next frame).
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for (name, bytes) in [("hw-logo", LOGO_FONT), ("hw-ui", UI_FONT)] {
        fonts
            .font_data
            .insert(name.into(), Arc::new(egui::FontData::from_static(bytes)));
        let mut family = vec![name.to_string()];
        family.extend(
            fonts
                .families
                .get(&FontFamily::Proportional)
                .cloned()
                .unwrap_or_default(),
        );
        fonts.families.insert(FontFamily::Name(name.into()), family);
    }
    ctx.set_fonts(fonts);
}

fn ease_out_back(x: f32) -> f32 {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (x - 1.0).powi(3) + c1 * (x - 1.0).powi(2)
}

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// A number in 0..1 from two integers (fixed sparkle patterns).
fn hash01(a: u32, b: u32) -> f32 {
    let mut x = a.wrapping_mul(0x9E37_79B9) ^ b.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297A_2D39);
    x ^= x >> 15;
    (x & 0xFF_FFFF) as f32 / 0x100_0000 as f32
}

fn with_alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

fn shadow_text(
    painter: &egui::Painter,
    pos: Pos2,
    align: Align2,
    text: &str,
    font: FontId,
    color: Color32,
) {
    let o = (font.size * 0.07).max(1.5);
    painter.text(
        pos + vec2(o, o),
        align,
        text,
        font.clone(),
        Color32::from_black_alpha(170),
    );
    painter.text(pos, align, text, font, color);
}

/// A dark wash from the left edge, so the column reads over a bright sky.
fn wash(painter: &egui::Painter, screen: Rect) {
    let mut m = Mesh::default();
    let w = (screen.width() * 0.55).max(420.0);
    let dark = Color32::from_rgba_unmultiplied(8, 10, 30, 150);
    let clear = Color32::TRANSPARENT;
    let (t, b) = (screen.top(), screen.bottom());
    m.colored_vertex(pos2(0.0, t), dark);
    m.colored_vertex(pos2(w, t), clear);
    m.colored_vertex(pos2(0.0, b), dark);
    m.colored_vertex(pos2(w, b), clear);
    m.add_triangle(0, 1, 2);
    m.add_triangle(2, 1, 3);
    painter.add(egui::Shape::mesh(m));
}

/// One glyph of the logo font: where it sits in its galley, its place in
/// the font atlas (0..1), and how far it advances.
struct Glyph {
    pos: Rect,
    uv: Rect,
}

struct LogoLetter {
    glyph: Glyph,
    /// Where its centre lands once it has flown in.
    at: Pos2,
    /// Its tilt on the arch (radians).
    tilt: f32,
}

struct Logo {
    letters: Vec<LogoLetter>,
    rect: Rect,
}

fn glyph(ctx: &egui::Context, ch: char, font: &FontId) -> Option<(Glyph, f32)> {
    let galley = ctx.fonts(|f| f.layout_no_wrap(ch.to_string(), font.clone(), Color32::WHITE));
    let [tw, th] = ctx.fonts(|f| f.font_image_size());
    let row = galley.rows.first()?;
    let m = &row.row.visuals.mesh;
    let first = m.vertices.first()?;
    let (mut pmin, mut pmax, mut umin, mut umax) = (first.pos, first.pos, first.uv, first.uv);
    for v in &m.vertices {
        pmin = pmin.min(v.pos);
        pmax = pmax.max(v.pos);
        umin = umin.min(v.uv);
        umax = umax.max(v.uv);
    }
    let off = row.pos.to_vec2();
    let size = vec2(tw as f32, th as f32);
    Some((
        Glyph {
            pos: Rect::from_min_max(pmin + off, pmax + off),
            uv: Rect::from_min_max(
                pos2(umin.x / size.x, umin.y / size.y),
                pos2(umax.x / size.x, umax.y / size.y),
            ),
        },
        galley.size().x,
    ))
}

/// HEROWORLD laid out on a gentle arch, its left edge at `origin`.
fn logo_layout(ctx: &egui::Context, em: f32, origin: Pos2) -> Logo {
    let font = FontId::new(em, logo_family());
    let mut letters = Vec::new();
    let mut x = origin.x;
    let mut parts = Vec::new();
    for ch in "HEROWORLD".chars() {
        if let Some((g, adv)) = glyph(ctx, ch, &font) {
            parts.push((g, x, adv));
            x += adv * 0.96;
        }
    }
    let width = (x - origin.x).max(1.0);
    let mut rect = Rect::NOTHING;
    for (g, x0, _) in parts {
        let cx = x0 + g.pos.center().x;
        let u = (cx - origin.x) / width * 2.0 - 1.0;
        let lift = em * 0.16 * (1.0 - u * u);
        let at = pos2(cx, origin.y + g.pos.center().y - lift);
        let tilt = -u * 0.07;
        let half = g.pos.size() * 0.5;
        rect = rect.union(Rect::from_center_size(at, half * 2.0));
        letters.push(LogoLetter { glyph: g, at, tilt });
    }
    Logo { letters, rect }
}

/// The sunburst behind the logo: slow-turning rays fading outwards.
fn sunburst(painter: &egui::Painter, logo: Rect, t: f32, still: bool) {
    let c = logo.center() + vec2(0.0, logo.height() * 0.1);
    let r = logo.width() * 0.75;
    let n = 22;
    let spin = if still { 0.0 } else { t * 0.05 };
    let grow = smoothstep((t - 0.9) / 0.8);
    if grow <= 0.0 {
        return;
    }
    let mut m = Mesh::default();
    for i in 0..n {
        let a0 = spin + i as f32 * TAU / n as f32;
        let a1 = a0 + TAU / n as f32 * 0.5;
        let k = m.vertices.len() as u32;
        let strength = if i % 2 == 0 { 0.32 } else { 0.2 } * grow;
        m.colored_vertex(c, with_alpha(GOLD, strength));
        let rr = r * (0.6 + 0.4 * grow);
        m.colored_vertex(
            c + vec2(a0.cos(), a0.sin() * 0.62) * rr,
            with_alpha(Color32::from_rgb(255, 150, 60), 0.0),
        );
        m.colored_vertex(
            c + vec2(a1.cos(), a1.sin() * 0.62) * rr,
            with_alpha(Color32::from_rgb(255, 150, 60), 0.0),
        );
        m.add_triangle(k, k + 1, k + 2);
    }
    painter.add(egui::Shape::mesh(m));
    // A warm glow right behind the letters.
    for (s, a) in [(0.55, 0.10), (0.38, 0.12), (0.22, 0.14)] {
        painter.add(egui::Shape::ellipse_filled(
            c,
            vec2(logo.width() * s, logo.height() * s * 1.6),
            with_alpha(Color32::from_rgb(255, 190, 90), a * grow),
        ));
    }
}

/// Aether sparks drifting round the logo, twinkling.
fn sparks(painter: &egui::Painter, logo: Rect, t: f32) {
    let c = logo.center();
    for i in 0..34u32 {
        let born = 1.1 + hash01(i, 9) * 0.8;
        let show = smoothstep((t - born) / 0.6);
        if show <= 0.0 {
            continue;
        }
        let dir = if i % 2 == 0 { 1.0 } else { -1.0 };
        let a = hash01(i, 1) * TAU + t * 0.12 * dir * (0.5 + hash01(i, 2));
        let rx = logo.width() * (0.42 + 0.22 * hash01(i, 3));
        let ry = logo.height() * (0.7 + 0.5 * hash01(i, 4));
        let p = c + vec2(a.cos() * rx, a.sin() * ry);
        let tw = 0.5 + 0.5 * (t * (2.0 + 3.0 * hash01(i, 5)) + i as f32).sin();
        let size = (3.0 + 6.0 * hash01(i, 6)) * (0.6 + 0.6 * tw);
        let col = match i % 3 {
            0 => AETHER,
            1 => Color32::from_rgb(255, 160, 225),
            _ => Color32::from_rgb(255, 230, 150),
        };
        star(painter, p, size, with_alpha(col, show * (0.35 + 0.65 * tw)));
    }
}

/// A four-point twinkle with a soft glow.
fn star(painter: &egui::Painter, p: Pos2, size: f32, col: Color32) {
    painter.circle_filled(p, size * 0.9, col.gamma_multiply(0.25));
    let thin = size * 0.18;
    for (a, b) in [
        (vec2(size, 0.0), vec2(0.0, thin)),
        (vec2(0.0, size), vec2(thin, 0.0)),
    ] {
        painter.add(egui::Shape::convex_polygon(
            vec![p - a, p - b, p + a, p + b],
            col,
            Stroke::NONE,
        ));
    }
    painter.circle_filled(
        p,
        thin * 1.2,
        Color32::WHITE.gamma_multiply(col.a() as f32 / 255.0),
    );
}

/// The chrome-gold face colour at height `v` (0 top, 1 bottom) of a letter.
fn chrome(v: f32) -> Color32 {
    const STOPS: [(f32, [u8; 3]); 6] = [
        (0.0, [255, 255, 236]),
        (0.36, [255, 226, 128]),
        (0.5, [214, 132, 32]),
        (0.53, [255, 236, 170]),
        (0.8, [255, 196, 70]),
        (1.0, [255, 150, 40]),
    ];
    for w in STOPS.windows(2) {
        let ((a, ca), (b, cb)) = (w[0], w[1]);
        if v <= b {
            let k = ((v - a) / (b - a)).clamp(0.0, 1.0);
            let c = |i: usize| (ca[i] as f32 + (cb[i] as f32 - ca[i] as f32) * k) as u8;
            return Color32::from_rgb(c(0), c(1), c(2));
        }
    }
    Color32::from_rgb(255, 150, 40)
}

/// How a letter stands this frame: flying in, then bobbing a little.
fn letter_pose(i: usize, t: f32, still: bool, em: f32) -> (Vec2, f32, f32, f32) {
    if still {
        return (Vec2::ZERO, 1.0, 0.0, 1.0);
    }
    let start = 0.35 + i as f32 * 0.075;
    let k = ((t - start) / 0.6).clamp(0.0, 1.0);
    let e = ease_out_back(k);
    let from = vec2(-em * 1.2, em * 1.8);
    let offset = from * (1.0 - e);
    let scale = 0.25 + 0.75 * e;
    let spin = -0.6 * (1.0 - e);
    let bob = if k >= 1.0 {
        (t * 2.0 + i as f32 * 0.7).sin() * em * 0.015
    } else {
        0.0
    };
    (offset + vec2(0.0, bob), scale, spin, smoothstep(k * 3.0))
}

fn draw_logo(painter: &egui::Painter, logo: &Logo, em: f32, t: f32, still: bool) {
    let mut mesh = Mesh::with_texture(egui::TextureId::default());
    // The glint: a bright band sweeping left to right every 7.5 s.
    let period = 7.5;
    let phase = (t - 5.0).rem_euclid(period) / 1.1;
    let glint_x = if still || t < 5.0 || phase > 1.0 {
        None
    } else {
        Some(logo.rect.left() - em + phase * (logo.rect.width() + em * 2.0))
    };
    for (i, l) in logo.letters.iter().enumerate() {
        let (offset, scale, spin, alpha) = letter_pose(i, t, still, em);
        if alpha <= 0.0 {
            continue;
        }
        let rot = l.tilt + spin;
        let (s, c) = rot.sin_cos();
        let centre = l.glyph.pos.center();
        let at = l.at + offset;
        let xf = |p: Pos2| -> Pos2 {
            let d = (p - centre) * scale;
            at + vec2(d.x * c - d.y * s, d.x * s + d.y * c)
        };
        let g = &l.glyph;
        let lp = |u: f32, v: f32| {
            pos2(
                g.pos.left() + g.pos.width() * u,
                g.pos.top() + g.pos.height() * v,
            )
        };
        let luv = |u: f32, v: f32| {
            pos2(
                g.uv.left() + g.uv.width() * u,
                g.uv.top() + g.uv.height() * v,
            )
        };
        let solid = |mesh: &mut Mesh, off: Vec2, col: Color32| {
            let corners = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)];
            let k = mesh.vertices.len() as u32;
            for (u, v) in corners {
                mesh.vertices.push(Vertex {
                    pos: xf(lp(u, v)) + off,
                    uv: luv(u, v),
                    color: col,
                });
            }
            mesh.add_triangle(k, k + 1, k + 2);
            mesh.add_triangle(k + 2, k + 1, k + 3);
        };
        let a = alpha;
        // Drop shadow.
        solid(
            &mut mesh,
            vec2(em * 0.09, em * 0.14) * scale,
            with_alpha(Color32::BLACK, 0.45 * a),
        );
        // Block shadow: the letter pushed back, deep red to dark.
        let steps = 9;
        let depth = vec2(em * 0.012, em * 0.018) * scale;
        let ring = |mesh: &mut Mesh, base: Vec2, col: Color32| {
            let r = em * 0.045 * scale;
            for k in 0..12 {
                let ang = k as f32 * TAU / 12.0;
                solid(mesh, base + vec2(ang.cos(), ang.sin()) * r, col);
            }
        };
        ring(&mut mesh, depth * steps as f32, with_alpha(NAVY, a));
        for k in (1..=steps).rev() {
            let f = k as f32 / steps as f32;
            let col = mix(
                Color32::from_rgb(200, 40, 40),
                Color32::from_rgb(90, 12, 28),
                f,
            );
            solid(&mut mesh, depth * k as f32, with_alpha(col, a));
        }
        ring(&mut mesh, Vec2::ZERO, with_alpha(NAVY, a));
        // The face: strips of chrome gold, with the glint on top.
        let (nu, nv) = (4, 14);
        let k0 = mesh.vertices.len() as u32;
        for vi in 0..=nv {
            for ui in 0..=nu {
                let (u, v) = (ui as f32 / nu as f32, vi as f32 / nv as f32);
                let p = xf(lp(u, v));
                let mut col = chrome(v);
                if let Some(gx) = glint_x {
                    let d = (p.x + (p.y - logo.rect.top()) * 0.45) - gx;
                    let g = (-(d * d) / (em * em * 0.05)).exp();
                    col = mix(col, Color32::WHITE, g * 0.9);
                }
                mesh.vertices.push(Vertex {
                    pos: p,
                    uv: luv(u, v),
                    color: with_alpha(col, a),
                });
            }
        }
        let row = (nu + 1) as u32;
        for vi in 0..nv as u32 {
            for ui in 0..nu as u32 {
                let i0 = k0 + vi * row + ui;
                mesh.add_triangle(i0, i0 + 1, i0 + row);
                mesh.add_triangle(i0 + row, i0 + 1, i0 + row + 1);
            }
        }
    }
    painter.add(egui::Shape::mesh(mesh));
    // A sparkle where the glint is.
    if let Some(gx) = glint_x {
        let p = pos2(
            gx - logo.rect.height() * 0.2,
            logo.rect.top() + logo.rect.height() * 0.3,
        );
        if logo.rect.x_range().contains(p.x) {
            star(painter, p, em * 0.22, Color32::WHITE);
        }
    }
}

/// A big friendly button. `pop` (0..1, may overshoot) is how far it has popped in.
#[allow(clippy::too_many_arguments)]
fn big_button(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    rect: Rect,
    id: egui::Id,
    label: &str,
    sub: Option<&str>,
    primary: bool,
    pop: f32,
    t: f32,
) -> egui::Response {
    let resp = ui.interact(rect, id, Sense::click());
    if pop <= 0.01 {
        return resp;
    }
    let hover = ui.ctx().animate_bool_with_time(id, resp.hovered(), 0.12);
    let press = if resp.is_pointer_button_down_on() {
        0.04
    } else {
        0.0
    };
    let scale = pop * (1.0 + 0.05 * hover - press);
    let r = Rect::from_center_size(rect.center() + vec2(hover * 6.0, 0.0), rect.size() * scale);
    let round = CornerRadius::same((r.height() * 0.5).min(255.0) as u8);
    let alpha = pop.clamp(0.0, 1.0);
    let (face, text) = if primary {
        (mix(HERO_RED, Color32::from_rgb(255, 90, 70), hover), CREAM)
    } else {
        (mix(CREAM, Color32::WHITE, hover), NAVY)
    };
    if primary {
        // A soft gold pulse round the main button.
        let pulse = 0.5 + 0.5 * (t * 2.4).sin();
        painter.rect_filled(
            r.expand(6.0 + 3.0 * pulse),
            CornerRadius::same((r.height() * 0.5 + 8.0).min(255.0) as u8),
            with_alpha(GOLD, (0.18 + 0.2 * pulse) * alpha),
        );
    }
    painter.rect_filled(
        r.translate(vec2(0.0, 5.0)),
        round,
        with_alpha(Color32::BLACK, 0.45 * alpha),
    );
    painter.rect_filled(r, round, with_alpha(face, alpha));
    painter.rect_stroke(
        r,
        round,
        Stroke::new(3.0_f32, with_alpha(NAVY, alpha)),
        StrokeKind::Outside,
    );
    let size = r.height() * if sub.is_some() { 0.36 } else { 0.42 };
    let ty = if sub.is_some() {
        r.center().y - r.height() * 0.1
    } else {
        r.center().y
    };
    painter.text(
        pos2(r.center().x, ty),
        Align2::CENTER_CENTER,
        label,
        FontId::new(size, ui_family()),
        with_alpha(text, alpha),
    );
    if let Some(sub) = sub {
        painter.text(
            pos2(r.center().x, r.center().y + r.height() * 0.24),
            Align2::CENTER_CENTER,
            sub,
            FontId::proportional((r.height() * 0.2).max(12.0)),
            with_alpha(text, 0.85 * alpha),
        );
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The loading bar at the bottom right, striped and comic-styled.
fn loading_bar(painter: &egui::Painter, screen: Rect, loading: (f32, &str), ready: bool, t: f32) {
    let w = (screen.width() * 0.26).clamp(220.0, 420.0);
    let h = 22.0;
    let margin = (screen.width() * 0.04).clamp(16.0, 60.0);
    let bar = Rect::from_min_size(
        pos2(screen.right() - margin - w, screen.bottom() - margin - h),
        vec2(w, h),
    );
    let label = if ready {
        "The city is ready!".to_string()
    } else {
        format!("{} {:.0}%", loading.1, loading.0 * 100.0)
    };
    shadow_text(
        painter,
        pos2(bar.right(), bar.top() - 8.0),
        Align2::RIGHT_BOTTOM,
        &label,
        FontId::new(17.0, ui_family()),
        CREAM,
    );
    let round = CornerRadius::same(11);
    painter.rect_filled(
        bar.translate(vec2(0.0, 3.0)),
        round,
        Color32::from_black_alpha(120),
    );
    painter.rect_filled(bar, round, Color32::from_rgb(40, 46, 90));
    let fill = Rect::from_min_size(bar.min, vec2(w * loading.0.clamp(0.0, 1.0), h));
    if fill.width() > 2.0 {
        painter.rect_filled(fill, round, GOLD);
        // Moving stripes on the fill.
        let clip = painter.with_clip_rect(fill);
        let step = 16.0;
        let shift = (t * 30.0) % step;
        let mut x = fill.left() - h - step + shift;
        while x < fill.right() {
            clip.add(egui::Shape::convex_polygon(
                vec![
                    pos2(x, fill.bottom()),
                    pos2(x + h, fill.top()),
                    pos2(x + h + step * 0.45, fill.top()),
                    pos2(x + step * 0.45, fill.bottom()),
                ],
                Color32::from_rgba_unmultiplied(255, 240, 180, 90),
                Stroke::NONE,
            ));
            x += step;
        }
    }
    painter.rect_stroke(bar, round, Stroke::new(3.0_f32, NAVY), StrokeKind::Outside);
}

/// The list of what changed, newest first (the title's and the game's
/// "What's new" button).
pub fn whats_new_modal(ctx: &egui::Context, open: &mut bool) {
    if !*open {
        return;
    }
    let modal = egui::Modal::new(egui::Id::new("What's new")).show(ctx, |ui| {
        ui.set_max_width(540.0);
        ui.heading("What's new");
        ui.separator();
        egui::ScrollArea::vertical()
            .max_height(420.0)
            .show(ui, |ui| {
                for line in hw_core::WHATS_NEW
                    .lines()
                    .skip_while(|l| !l.starts_with("## "))
                {
                    if let Some(h) = line.strip_prefix("## ") {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new(h).strong().size(16.0));
                    } else if let Some(item) = line.strip_prefix("- ") {
                        ui.label(format!("•  {item}"));
                    } else if !line.trim().is_empty() {
                        ui.label(line);
                    }
                }
            });
        ui.add_space(6.0);
        if ui.button("Close").clicked() {
            *open = false;
        }
    });
    if modal.should_close() {
        *open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chrome_runs_light_to_gold() {
        let top = chrome(0.0);
        let horizon = chrome(0.5);
        assert!(top.g() > horizon.g(), "the horizon line is darker");
        assert_eq!(chrome(2.0), Color32::from_rgb(255, 150, 40));
    }

    #[test]
    fn letters_land_and_stand_still_when_asked() {
        let em = 100.0;
        let (off, scale, spin, alpha) = letter_pose(3, 10.0, false, em);
        assert!(off.length() < em * 0.02 && (scale - 1.0).abs() < 1e-4 && spin.abs() < 1e-4);
        assert_eq!(alpha, 1.0);
        assert_eq!(letter_pose(0, 0.0, true, em), (Vec2::ZERO, 1.0, 0.0, 1.0));
        // Before its turn a letter is not drawn.
        assert_eq!(letter_pose(8, 0.1, false, em).3, 0.0);
    }

    #[test]
    fn room_codes_are_upper_case_addresses_are_not() {
        assert_eq!("abc12x".to_string().to_uppercase_code(), "ABC12X");
        assert_eq!(
            "host.lan:7777".to_string().to_uppercase_code(),
            "host.lan:7777"
        );
    }
}
