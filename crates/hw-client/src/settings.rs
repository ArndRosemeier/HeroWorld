//! The player's settings: picture quality, frame rate, the mouse and the
//! view, and the controls screen. Kept in the profile ([`Profile`]); shown
//! on the title's Settings card and in the game's menu (Escape).

use crate::profile::Profile;
use engine::prelude::*;
use engine::quality::Quality;
use engine::ui::egui;

/// The frame rate "Keep it smooth" holds by trading distant detail.
pub const SMOOTH_FPS: f32 = 60.0;
/// Turning speed of the look at a sensitivity of 1 (degrees per pixel).
const LOOK_DEGREES: f32 = 0.15;

/// The quality preset in the profile.
pub fn quality(p: &Profile) -> Quality {
    Quality::from_name(&p.quality).unwrap_or_default()
}

/// What a quality level shows, in a line for the settings card.
pub fn quality_hint(q: Quality) -> String {
    let reach = crate::lighting::shadows(q).cascade_end_m[2];
    let rest = match q {
        Quality::Low => "no soft shading or bounced light, detail swaps close",
        Quality::Medium => "soft shading in corners, reflections",
        Quality::High => "plus light bouncing off walls and streets",
        Quality::Ultra => "plus sharper shadows and detail kept half again as far",
    };
    format!(
        "Sun shadows reach {reach:.0} m, {} lamps cast shadows; {rest}.",
        crate::lighting::lamp_shadows(q)
    )
}

/// How far the town is drawn (m).
pub const VIEW_DISTANCE: f32 = 6000.0;

/// Engine's startup settings from the profile, so the world starts at the
/// player's quality, frame-rate cap, render scale and vsync (Engine checks
/// them before the window opens). [`apply`] still runs on the first frame
/// for what Settings does not cover (lighting per quality, field of view)
/// and again whenever the player changes a setting.
pub fn startup(p: &Profile) -> engine::Settings {
    let mut s = engine::Settings::default()
        .quality(quality(p))
        .vsync(p.vsync)
        .render_scale(p.render_scale.clamp(0.5, 1.0))
        .view_distance(VIEW_DISTANCE);
    if p.smooth {
        s = s.target_fps(SMOOTH_FPS);
    }
    s
}

/// Puts the profile's volumes into the world's sound buses.
pub fn apply_volumes(world: &mut World, p: &Profile) {
    use engine::audio::Bus;
    let a = world.audio_mut();
    a.set_bus_volume(Bus::Music, p.music);
    a.set_bus_volume(Bus::Ui, p.sounds);
    a.set_bus_volume(Bus::Effects, p.sounds);
    a.set_bus_volume(Bus::Ambience, p.sounds);
}

/// Puts the profile's picture settings into the world.
pub fn apply(world: &mut World, p: &Profile) {
    world.set_quality(quality(p));
    crate::lighting::apply_quality(world, quality(p));
    // Skip what hides behind buildings where it pays: from the street most
    // of the town does; Engine switches the test off by itself where it
    // hides little (high in the air, open country). `ENGINE_OCCLUSION=0|1`
    // compares.
    world.set_occlusion_mode(engine::OcclusionMode::Auto);
    // Draw the scene smaller on slow cards; TAA upscales it to the window.
    world.set_render_scale(p.render_scale);
    let _ = world.set_target_fps(p.smooth.then_some(SMOOTH_FPS));
    world.set_vsync(p.vsync);
    let near = world.camera().near();
    world.set_camera_lens(p.fov.clamp(40.0, 100.0), near);
}

/// Degrees the view turns per pixel of mouse movement, and the sign of
/// vertical look (-1 when inverted).
pub fn look(p: &Profile) -> (f32, f32) {
    (
        LOOK_DEGREES * p.look_speed.clamp(0.2, 4.0),
        if p.invert_y { -1.0 } else { 1.0 },
    )
}

/// The settings as widgets. Returns true when a picture setting changed
/// (the caller applies them; volumes it applies itself).
/// `accent` colours the headings.
pub fn settings_ui(ui: &mut egui::Ui, p: &mut Profile, accent: egui::Color32) -> bool {
    let mut changed = false;
    let gold = accent;
    ui.label(egui::RichText::new("Sound").strong().color(gold));
    ui.horizontal(|ui| {
        ui.label("Music");
        ui.add(egui::Slider::new(&mut p.music, 0.0..=1.0).show_value(false));
    });
    ui.horizontal(|ui| {
        ui.label("Sounds");
        ui.add(egui::Slider::new(&mut p.sounds, 0.0..=1.0).show_value(false));
    });
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Picture").strong().color(gold));
    ui.horizontal(|ui| {
        ui.label("Quality");
        let now = quality(p);
        for q in Quality::ALL {
            let name = match q {
                Quality::Low => "Low",
                Quality::Medium => "Medium",
                Quality::High => "High",
                Quality::Ultra => "Ultra",
            };
            if ui.selectable_label(now == q, name).clicked() && now != q {
                p.quality = q.name().into();
                changed = true;
            }
        }
    });
    ui.label(egui::RichText::new(quality_hint(quality(p))).weak());
    changed |= ui
        .checkbox(
            &mut p.smooth,
            format!("Keep it smooth ({SMOOTH_FPS:.0} fps: far detail gives way first)"),
        )
        .changed();
    changed |= ui.checkbox(&mut p.vsync, "VSync (vertical sync)").changed();
    if !p.vsync {
        ui.label(
            egui::RichText::new("Off: as many frames as the card can draw; the picture may tear.")
                .weak(),
        );
    }
    ui.horizontal(|ui| {
        ui.label("Field of view");
        changed |= ui
            .add(egui::Slider::new(&mut p.fov, 45.0..=90.0).suffix("°"))
            .changed();
    });
    ui.horizontal(|ui| {
        ui.label("Render resolution");
        let mut pct = (p.render_scale * 100.0).round();
        if ui
            .add(
                egui::Slider::new(&mut pct, 50.0..=100.0)
                    .step_by(5.0)
                    .suffix("%"),
            )
            .changed()
        {
            p.render_scale = pct / 100.0;
            changed = true;
        }
    });
    if p.render_scale < 1.0 {
        ui.label(
            egui::RichText::new(
                "Drawn smaller and sharpened up to the window: faster, a little softer.",
            )
            .weak(),
        );
    }
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Mouse").strong().color(gold));
    ui.horizontal(|ui| {
        ui.label("Look speed");
        ui.add(egui::Slider::new(&mut p.look_speed, 0.3..=3.0).show_value(false));
    });
    ui.checkbox(&mut p.invert_y, "Invert up and down");
    changed
}

/// Bigger type and roomy rows; the colours and frames are the game's own
/// comic look (`uikit::install`).
fn menu_style(ui: &mut egui::Ui) {
    use egui::{FontId, TextStyle};
    let button = crate::uikit::ui_font(ui.ctx(), 18.0);
    let st = ui.style_mut();
    st.text_styles
        .insert(TextStyle::Body, FontId::proportional(17.0));
    st.text_styles.insert(TextStyle::Button, button);
    st.spacing.item_spacing = egui::vec2(10.0, 8.0);
    st.spacing.button_padding = egui::vec2(14.0, 7.0);
    st.spacing.slider_width = 220.0;
    ui.set_min_width(440.0);
}

/// What each key does.
pub const CONTROLS: &[(&str, &str)] = &[
    (
        "Right mouse (hold) / P",
        "Look around (P locks the pointer)",
    ),
    ("W A S D", "Run"),
    ("R", "Autorun (the mouse steers; S or R stops)"),
    ("Space", "Jump; climb when flying or in a lift"),
    ("V", "Fly (once you have Flight); V again drops you"),
    ("Ctrl / Shift", "Sink / faster when flying"),
    (
        "Mouse wheel",
        "Zoom in / out (all the way in: first person)",
    ),
    ("Tab", "Pick the villain nearest the crosshair"),
    ("Left mouse / 1", "Strike"),
    ("Q / 2", "Energy bolt"),
    ("3 to 6", "Your other powers"),
    ("B", "Powers and karma"),
    ("E", "Open or shut a door"),
    ("E (hold)", "Revive a downed teammate"),
    ("H", "Call the medical drone when you are down"),
    ("G", "Hail a flying taxi (G again hops out)"),
    ("M", "Town map (the wrist radio's incidents)"),
    ("N", "Minimap on or off"),
    ("O", "Free camera"),
    (
        "Enter or /",
        "Chat: type a line, Enter says it, Escape cancels",
    ),
    ("Escape", "This menu; frees the mouse when it is locked"),
    (
        "Ctrl+Shift+F12",
        "Debug menu: go anywhere, levels, powers, events, time, cut-open view",
    ),
];

pub fn controls_ui(ui: &mut egui::Ui) {
    egui::Grid::new("controls")
        .num_columns(2)
        .spacing([18.0, 4.0])
        .striped(true)
        .show(ui, |ui| {
            for (keys, what) in CONTROLS {
                ui.label(egui::RichText::new(*keys).strong());
                ui.label(*what);
                ui.end_row();
            }
        });
}

/// The game's menu (Escape): resume, settings, controls, quit.
pub struct GameMenu {
    pub open: bool,
    page: Page,
    /// The profile being changed (read when the menu opens, saved when it
    /// closes, so the hero file written meanwhile is kept).
    profile: Option<Profile>,
    /// Look speed and up-down sign in use (see [`look`]).
    pub look: (f32, f32),
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Page {
    #[default]
    Main,
    Settings,
    Controls,
}

impl GameMenu {
    /// `HW_MENU=main|settings|controls` opens it at once (screenshots).
    pub fn new(p: &Profile) -> Self {
        let shown = std::env::var("HW_MENU").ok();
        let page = match shown.as_deref() {
            Some("settings") => Page::Settings,
            Some("controls") => Page::Controls,
            _ => Page::Main,
        };
        Self {
            open: shown.is_some(),
            page,
            profile: shown.is_some().then(|| p.clone()),
            look: look(p),
        }
    }

    /// Opens on Escape (when the mouse is free: Escape first frees a locked
    /// mouse), draws the menu and applies what is changed. Call every frame
    /// of play. Returns true when the player quits.
    pub fn show(&mut self, ui: &engine::ui::UiFrame, world: &mut World) -> bool {
        let escape = ui.ctx().input(|i| i.key_pressed(egui::Key::Escape));
        if escape && !self.open && !world.pointer_lock() {
            self.open = true;
            self.page = Page::Main;
            self.profile = Some(Profile::load());
        }
        let Some(p) = self.profile.as_mut() else {
            return false;
        };
        let mut quit = false;
        let mut picture = false;
        let mut open = self.open;
        let page = &mut self.page;
        crate::googie::modal(ui, "HeroWorld", &mut open, |ui, open| {
            menu_style(ui);
            match *page {
                Page::Main => {
                    ui.vertical_centered_justified(|ui| {
                        if ui.button("Back to the game").clicked() {
                            *open = false;
                        }
                        if ui.button("Settings").clicked() {
                            *page = Page::Settings;
                        }
                        if ui.button("Controls").clicked() {
                            *page = Page::Controls;
                        }
                        ui.add_space(8.0);
                        if ui.button("Quit HeroWorld").clicked() {
                            quit = true;
                        }
                    });
                }
                Page::Settings => {
                    picture = settings_ui(ui, p, egui::Color32::from_rgb(255, 214, 90));
                    if ui.button("Back").clicked() {
                        *page = Page::Main;
                    }
                }
                Page::Controls => {
                    controls_ui(ui);
                    if ui.button("Back").clicked() {
                        *page = Page::Main;
                    }
                }
            }
        });
        self.open = open;
        self.look = look(p);
        apply_volumes(world, p);
        if picture {
            apply(world, p);
        }
        if !self.open || quit {
            // Keep what was set, over the latest file (the hero file may
            // have been saved while the menu was open).
            let mut latest = Profile::load();
            latest.music = p.music;
            latest.sounds = p.sounds;
            latest.quality = p.quality.clone();
            latest.smooth = p.smooth;
            latest.vsync = p.vsync;
            latest.render_scale = p.render_scale;
            latest.fov = p.fov;
            latest.look_speed = p.look_speed;
            latest.invert_y = p.invert_y;
            latest.save();
            self.profile = None;
        }
        quit
    }
}
