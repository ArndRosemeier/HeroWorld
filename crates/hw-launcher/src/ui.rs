//! The launcher window: a comic cover with the logo, one big button
//! (Install, Update or Play), friends, settings and what's new.

use crate::config::{saves_dir, Config};
use crate::install::{Install, Plan, Step};
use crate::manifest::{newer, release_url, Manifest};
use crate::net::GitHub;
use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId,
    Pos2, Rect, RichText, Sense, Shape, Stroke, StrokeKind, Vec2,
};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

const LOGO_FONT: &[u8] = include_bytes!("../../hw-client/assets/fonts/LuckiestGuy-Regular.ttf");
const UI_FONT: &[u8] = include_bytes!("../../hw-client/assets/fonts/TitanOne-Regular.ttf");

/// The game's website.
pub const WEBSITE: &str = "https://apps.futuremagic.de/HeroWorld/";

const NAVY: Color32 = Color32::from_rgb(22, 26, 64);
const DEEP: Color32 = Color32::from_rgb(12, 14, 40);
const CREAM: Color32 = Color32::from_rgb(255, 246, 222);
const RED: Color32 = Color32::from_rgb(226, 52, 48);
const GOLD: Color32 = Color32::from_rgb(255, 198, 64);
const AETHER: Color32 = Color32::from_rgb(120, 236, 255);
const INK: Color32 = Color32::from_rgb(40, 34, 52);

/// What the worker tells the window.
enum Msg {
    Step(Step),
    /// The newest release, and what it would take to get there.
    Checked(Result<(Manifest, Plan), String>),
    /// An install, update or repair finished.
    Done(Result<Manifest, String>),
    /// The launcher replaced itself: restart.
    SelfUpdated(Result<(), String>),
}

#[derive(Clone, Debug, PartialEq)]
enum State {
    /// Asking GitHub what is new.
    Checking,
    /// Knows the newest release and what an update needs.
    Ready,
    /// Downloading or unpacking.
    Working(Option<Step>),
    /// Could not reach GitHub or the update failed: the message.
    Failed(String),
}

pub struct Launcher {
    config: Config,
    state: State,
    latest: Option<Manifest>,
    plan: Option<Plan>,
    installed: Option<Manifest>,
    rx: Receiver<Msg>,
    tx: Sender<Msg>,
    cancel: Arc<AtomicBool>,
    game: Option<Child>,
    settings_open: bool,
    folder_text: String,
    room: String,
    start: Instant,
    notice: Option<String>,
}

impl Launcher {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        fonts(&cc.egui_ctx);
        let config = Config::load();
        let (tx, rx) = channel();
        let mut l = Launcher {
            folder_text: config.install_dir.display().to_string(),
            room: config.last_room.clone(),
            installed: Install::new(&config.install_dir).installed(),
            config,
            state: State::Checking,
            latest: None,
            plan: None,
            rx,
            tx,
            cancel: Arc::new(AtomicBool::new(false)),
            game: None,
            settings_open: false,
            start: Instant::now(),
            notice: None,
        };
        l.check(&cc.egui_ctx, false);
        l
    }

    fn install(&self) -> Install {
        Install::new(&self.config.install_dir)
    }

    /// Ask GitHub for the newest build and work out what is missing
    /// (`full`: hash every file, for Repair).
    fn check(&mut self, ctx: &egui::Context, full: bool) {
        self.state = if full {
            State::Working(None)
        } else {
            State::Checking
        };
        let (tx, ctx, dir) = (
            self.tx.clone(),
            ctx.clone(),
            self.config.install_dir.clone(),
        );
        let cancel = self.fresh_cancel();
        std::thread::spawn(move || {
            let gh = GitHub::new(&release_url());
            let result = gh.manifest().and_then(|m| {
                let mut on_step = |s: Step| {
                    let _ = tx.send(Msg::Step(s));
                    ctx.request_repaint();
                };
                let plan = Install::new(&dir).plan(&m, full, &cancel, &mut on_step)?;
                Ok((m, plan))
            });
            let _ = tx.send(Msg::Checked(result));
            ctx.request_repaint();
        });
    }

    /// Download and unpack what the plan says.
    fn update(&mut self, ctx: &egui::Context) {
        let (Some(m), Some(plan)) = (self.latest.clone(), self.plan.clone()) else {
            return;
        };
        self.state = State::Working(None);
        let (tx, ctx, dir) = (
            self.tx.clone(),
            ctx.clone(),
            self.config.install_dir.clone(),
        );
        let cancel = self.fresh_cancel();
        std::thread::spawn(move || {
            let gh = GitHub::new(&release_url());
            let mut on_step = |s: Step| {
                let _ = tx.send(Msg::Step(s));
                ctx.request_repaint();
            };
            let r = Install::new(&dir)
                .apply(&m, &plan, &gh, &cancel, &mut on_step)
                .map(|()| m);
            let _ = tx.send(Msg::Done(r));
            ctx.request_repaint();
        });
    }

    fn update_self(&mut self, ctx: &egui::Context) {
        let Some(m) = self.latest.clone() else { return };
        self.state = State::Working(None);
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let r = GitHub::new(&release_url()).update_self(&m);
            let _ = tx.send(Msg::SelfUpdated(r));
            ctx.request_repaint();
        });
    }

    fn fresh_cancel(&mut self) -> Arc<AtomicBool> {
        self.cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.cancel.clone()
    }

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Step(s) => {
                    if matches!(self.state, State::Working(_) | State::Checking) {
                        self.state = State::Working(Some(s));
                    }
                }
                Msg::Checked(Ok((m, plan))) => {
                    self.latest = Some(m);
                    self.plan = Some(plan);
                    self.state = State::Ready;
                }
                Msg::Checked(Err(e)) => self.state = State::Failed(e),
                Msg::Done(Ok(m)) => {
                    self.installed = Some(m);
                    self.plan = Some(Plan::default());
                    self.state = State::Ready;
                    self.notice = Some("All set. Have fun, hero!".into());
                }
                Msg::Done(Err(e)) => {
                    self.state = State::Failed(e);
                    self.installed = self.install().installed();
                }
                Msg::SelfUpdated(Ok(())) => {
                    // Start the new launcher and leave.
                    if let Ok(exe) = std::env::current_exe() {
                        let _ = Command::new(exe).spawn();
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                Msg::SelfUpdated(Err(e)) => self.state = State::Failed(e),
            }
        }
        if let Some(child) = &mut self.game {
            if !matches!(child.try_wait(), Ok(None)) {
                self.game = None;
            }
        }
    }

    fn game_running(&self) -> bool {
        self.game.is_some()
    }

    /// Start the game: alone, hosting, or joining a room.
    fn play(&mut self, ctx: &egui::Context, how: Play) {
        let install = self.install();
        let mut cmd = Command::new(install.exe());
        cmd.current_dir(&install.dir);
        match &how {
            Play::Alone => {}
            Play::Host => {
                cmd.env("HW_HOST", "1");
            }
            Play::Join(code) => {
                cmd.env("HW_JOIN", code);
            }
        }
        match cmd.spawn() {
            Ok(child) => {
                self.game = Some(child);
                if self.config.close_on_play {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            Err(e) => self.state = State::Failed(format!("could not start the game: {e}")),
        }
    }

    fn installed_ok(&self) -> bool {
        self.installed.is_some() && self.install().exe().exists()
    }
}

enum Play {
    Alone,
    Host,
    Join(String),
}

impl eframe::App for Launcher {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll(ctx);
        let t = self.start.elapsed().as_secs_f32();
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(DEEP))
            .show(ctx, |ui| {
                let screen = ui.max_rect();
                let painter = ui.painter().clone();
                backdrop(&painter, screen, t);
                let pad = 28.0;
                let left_w = (screen.width() * 0.5).max(360.0);
                // The logo, top left.
                let logo_at = pos2(screen.left() + pad, screen.top() + pad);
                let logo = logo(&painter, logo_at, 64.0, t);
                stamp(&painter, logo.right_top() + vec2(-10.0, 4.0), 22.0, t);
                let version = match &self.installed {
                    Some(i) => format!(
                        "A golden-age superhero sandbox  ·  installed: {}",
                        i.version
                    ),
                    None => "A golden-age superhero sandbox".to_string(),
                };
                text(
                    &painter,
                    pos2(logo_at.x + 4.0, logo.bottom() + 10.0),
                    Align2::LEFT_TOP,
                    &version,
                    16.0,
                    CREAM,
                );

                // The big button and what it says under it.
                let col = Rect::from_min_size(
                    pos2(screen.left() + pad, logo.bottom() + 52.0),
                    vec2(left_w - pad * 1.5, screen.height()),
                );
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(col));
                self.main_column(&mut child, ctx, t);

                // What's new, on the right, as a comic panel.
                let panel = Rect::from_min_max(
                    pos2(screen.left() + left_w + pad * 0.5, screen.top() + pad),
                    pos2(screen.right() - pad, screen.bottom() - pad),
                );
                self.whats_new(ui, &painter, panel);
            });
        if self.settings_open {
            self.settings(ctx);
        }
    }
}

impl Launcher {
    fn main_column(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, t: f32) {
        let painter = ui.painter().clone();
        let width = ui.max_rect().width().min(420.0);
        let installed = self.installed_ok();
        let latest = self.latest.clone();
        let plan = self.plan.clone();
        let busy = matches!(self.state, State::Working(_));
        // What the big button does.
        let (label, sub, action): (String, String, Option<Action>) = match (&self.state, &plan) {
            (State::Working(_), _) => ("Working...".into(), String::new(), None),
            (State::Checking, _) if installed => (
                "PLAY".into(),
                "checking for updates...".into(),
                Some(Action::Play),
            ),
            (State::Checking, _) => (
                "Looking...".into(),
                "asking GitHub for the newest build".into(),
                None,
            ),
            (State::Ready, Some(p)) if !p.is_empty() => {
                let v = latest
                    .as_ref()
                    .map(|m| m.version.clone())
                    .unwrap_or_default();
                let mb = p.bytes() as f64 / 1e6;
                if installed {
                    (
                        format!("UPDATE to {v}"),
                        format!("{mb:.0} MB to download"),
                        Some(Action::Update),
                    )
                } else {
                    (
                        "INSTALL".into(),
                        format!("HeroWorld {v}, {mb:.0} MB"),
                        Some(Action::Update),
                    )
                }
            }
            (State::Ready, _) if installed => {
                ("PLAY".into(), "up to date".into(), Some(Action::Play))
            }
            (State::Ready, _) => ("INSTALL".into(), String::new(), Some(Action::Update)),
            (State::Failed(_), _) if installed => (
                "PLAY".into(),
                "offline: playing what you have".into(),
                Some(Action::Play),
            ),
            (State::Failed(_), _) => ("TRY AGAIN".into(), String::new(), Some(Action::Retry)),
        };
        let (rect, resp) = ui.allocate_exact_size(vec2(width, 86.0), Sense::click());
        let enabled = action.is_some() && !(action == Some(Action::Update) && self.game_running());
        big_button(&painter, rect, &resp, &label, &sub, enabled, t);
        if enabled && resp.clicked() {
            match action {
                Some(Action::Play) => self.play(ctx, Play::Alone),
                Some(Action::Update) => self.update(ctx),
                Some(Action::Retry) => self.check(ctx, false),
                None => {}
            }
        }
        ui.add_space(14.0);

        // Progress, errors and notices.
        match &self.state {
            State::Working(step) => {
                let (frac, line) = match step {
                    Some(Step::Checking { done, total }) => (
                        *done as f32 / (*total).max(1) as f32,
                        "Checking the game files...".to_string(),
                    ),
                    Some(Step::Downloading {
                        bundle,
                        done,
                        total,
                    }) => (
                        *done as f32 / (*total).max(1) as f32,
                        format!(
                            "Downloading {} ({:.0} of {:.0} MB)",
                            bundle_name(bundle),
                            *done as f64 / 1e6,
                            *total as f64 / 1e6
                        ),
                    ),
                    Some(Step::Unpacking { bundle }) => {
                        (1.0, format!("Unpacking {}...", bundle_name(bundle)))
                    }
                    None => (0.0, "Getting ready...".into()),
                };
                progress_bar(ui, width, frac, &line, t);
                if ui.button("Stop").clicked() {
                    self.cancel.store(true, Ordering::Relaxed);
                }
            }
            State::Failed(e) => {
                ui.label(
                    RichText::new(format!("Oops: {e}")).color(Color32::from_rgb(255, 160, 150)),
                );
            }
            _ => {
                if let Some(n) = &self.notice {
                    ui.label(RichText::new(n).color(GOLD));
                }
            }
        }
        if self.game_running() {
            ui.label(RichText::new("HeroWorld is running.").color(AETHER));
        }

        // A newer launcher.
        if let Some(m) = &latest {
            if newer(&m.launcher.version, env!("CARGO_PKG_VERSION")) && !busy {
                ui.add_space(6.0);
                if ui
                    .button(format!(
                        "A new launcher is out ({}): update it",
                        m.launcher.version
                    ))
                    .clicked()
                {
                    self.update_self(ctx);
                }
            }
        }

        // Friends.
        ui.add_space(18.0);
        ui.label(
            RichText::new("PLAY WITH FRIENDS")
                .font(FontId::new(18.0, logo_family()))
                .color(GOLD),
        );
        ui.add_space(4.0);
        ui.add_enabled_ui(installed && !busy, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .button("Host a world")
                    .on_hover_text(
                        "Opens your world to friends; the room code shows top right in the game.",
                    )
                    .clicked()
                {
                    self.play(ctx, Play::Host);
                }
                ui.add_space(12.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.room)
                        .hint_text("room code")
                        .desired_width(120.0),
                );
                let code = self.room.trim().to_string();
                if ui
                    .add_enabled(!code.is_empty(), egui::Button::new("Join"))
                    .clicked()
                {
                    self.config.last_room = code.clone();
                    self.config.save();
                    self.play(ctx, Play::Join(code));
                }
            });
        });

        // Small links.
        ui.add_space(22.0);
        ui.horizontal_wrapped(|ui| {
            if ui.link("Settings").clicked() {
                self.settings_open = true;
            }
            ui.label("·");
            if ui
                .add_enabled(
                    !busy && latest.is_some() && !self.game_running(),
                    egui::Link::new("Repair"),
                )
                .on_hover_text("Checks every game file and downloads what is broken or missing.")
                .clicked()
            {
                self.notice = None;
                self.check(ctx, true);
            }
            ui.label("·");
            if ui.link("Saves folder").clicked() {
                open_folder(&saves_dir());
            }
            ui.label("·");
            if ui.link("Website").clicked() {
                ctx.open_url(egui::OpenUrl::new_tab(WEBSITE));
            }
        });
        ui.add_space(6.0);
        ui.label(
            RichText::new(format!("Launcher {}", env!("CARGO_PKG_VERSION")))
                .size(12.0)
                .color(Color32::from_gray(170)),
        );
    }

    fn whats_new(&self, ui: &mut egui::Ui, painter: &egui::Painter, panel: Rect) {
        comic_panel(painter, panel);
        let inner = panel.shrink(22.0);
        text(
            painter,
            inner.left_top(),
            Align2::LEFT_TOP,
            "WHAT'S NEW",
            30.0,
            RED,
        );
        let body = Rect::from_min_max(inner.left_top() + vec2(0.0, 44.0), inner.right_bottom());
        let source = self
            .latest
            .as_ref()
            .or(self.installed.as_ref())
            .map(|m| m.whats_new.clone())
            .unwrap_or_default();
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
        child.style_mut().visuals.override_text_color = Some(INK);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                if source.is_empty() {
                    ui.label("The list of changes appears once the launcher has talked to GitHub.");
                }
                let mut shown = 0;
                for line in source.lines().skip_while(|l| !l.starts_with("## ")) {
                    if let Some(h) = line.strip_prefix("## ") {
                        shown += 1;
                        if shown > 30 {
                            break;
                        }
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(h)
                                .font(FontId::new(18.0, logo_family()))
                                .color(NAVY),
                        );
                    } else if let Some(item) = line.strip_prefix("- ") {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new("★")
                                    .font(FontId::new(14.0, ui_family()))
                                    .color(RED),
                            );
                            ui.label(RichText::new(plain(item)).font(FontId::proportional(14.5)));
                        });
                    } else if !line.trim().is_empty() {
                        ui.label(RichText::new(plain(line)).font(FontId::proportional(14.5)));
                    }
                }
            });
    }

    fn settings(&mut self, ctx: &egui::Context) {
        let mut open = true;
        egui::Window::new("Settings")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Install folder");
                ui.add(egui::TextEdit::singleline(&mut self.folder_text).desired_width(420.0));
                ui.horizontal(|ui| {
                    if ui.button("Use the default").clicked() {
                        self.folder_text = crate::config::default_install_dir().display().to_string();
                    }
                    if ui.button("Open it").clicked() {
                        open_folder(&self.config.install_dir);
                    }
                });
                ui.label(
                    RichText::new("Pointing it at a folder you unzipped earlier keeps every file that is still current.")
                        .size(12.0),
                );
                ui.add_space(8.0);
                if ui
                    .checkbox(&mut self.config.close_on_play, "Close the launcher when the game starts")
                    .changed()
                {
                    self.config.save();
                }
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Graphics, sound and controls are in the game's own Settings.").size(12.0),
                );
                ui.add_space(8.0);
                let changed = self.folder_text.trim() != self.config.install_dir.display().to_string();
                if ui
                    .add_enabled(changed && !matches!(self.state, State::Working(_)), egui::Button::new("Save folder"))
                    .clicked()
                {
                    self.config.install_dir = self.folder_text.trim().into();
                    self.config.save();
                    self.installed = self.install().installed();
                    self.check(ctx, false);
                }
            });
        if !open {
            self.settings_open = false;
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Play,
    Update,
    Retry,
}

/// Markdown emphasis (`*Skylark*`, `**bold**`, `` `code` ``) as plain text.
fn plain(s: &str) -> String {
    s.replace("**", "").replace(['*', '`'], "")
}

fn bundle_name(b: &str) -> &str {
    match b {
        "game" => "the game",
        "packs" => "the people",
        "robots" => "the robots",
        "music" => "the music",
        other => other,
    }
}

fn open_folder(path: &std::path::Path) {
    let _ = std::fs::create_dir_all(path);
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = Command::new(program).arg(path).spawn();
}

// ---- Look -------------------------------------------------------------

fn fonts(ctx: &egui::Context) {
    let mut f = FontDefinitions::default();
    f.font_data
        .insert("logo".into(), Arc::new(FontData::from_static(LOGO_FONT)));
    f.font_data
        .insert("ui".into(), Arc::new(FontData::from_static(UI_FONT)));
    f.families
        .insert(logo_family(), vec!["logo".into(), "ui".into()]);
    // Titan One for buttons and labels; long text keeps the plain font
    // (`FontId::proportional`), which reads better at small sizes.
    f.families
        .insert(ui_family(), vec!["ui".into(), "logo".into()]);
    ctx.set_fonts(f);
    ctx.style_mut(|s| {
        s.text_styles
            .insert(egui::TextStyle::Body, FontId::new(15.0, ui_family()));
        s.text_styles
            .insert(egui::TextStyle::Button, FontId::new(16.0, ui_family()));
        s.spacing.button_padding = vec2(14.0, 7.0);
        let v = &mut s.visuals;
        v.override_text_color = Some(CREAM);
        v.hyperlink_color = GOLD;
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(52, 60, 120);
        v.widgets.inactive.bg_fill = Color32::from_rgb(52, 60, 120);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(76, 88, 170);
        v.widgets.active.weak_bg_fill = RED;
        v.widgets.inactive.corner_radius = CornerRadius::same(14);
        v.widgets.hovered.corner_radius = CornerRadius::same(14);
        v.widgets.active.corner_radius = CornerRadius::same(14);
        v.extreme_bg_color = Color32::from_rgb(30, 34, 80);
        v.window_fill = NAVY;
        v.window_stroke = Stroke::new(3.0_f32, GOLD);
    });
}

fn ui_family() -> FontFamily {
    FontFamily::Name("ui".into())
}

fn logo_family() -> FontFamily {
    FontFamily::Name("logo".into())
}

fn text(p: &egui::Painter, at: Pos2, align: Align2, s: &str, size: f32, col: Color32) {
    let font = FontId::new(size, logo_family());
    let o = (size * 0.06).max(1.5);
    p.text(
        at + vec2(o, o),
        align,
        s,
        font.clone(),
        Color32::from_black_alpha(150),
    );
    p.text(at, align, s, font, col);
}

/// Turning sunburst rays and halftone dots on deep navy.
fn backdrop(p: &egui::Painter, screen: Rect, t: f32) {
    let centre = pos2(screen.left() + screen.width() * 0.22, screen.top() + 70.0);
    let r = screen.width().max(screen.height()) * 1.6;
    let rays = 18;
    let turn = t * 0.05;
    for i in 0..rays {
        let a0 = turn + i as f32 * std::f32::consts::TAU / rays as f32;
        let a1 = a0 + std::f32::consts::TAU / rays as f32 * 0.5;
        p.add(Shape::convex_polygon(
            vec![
                centre,
                centre + vec2(a0.cos(), a0.sin()) * r,
                centre + vec2(a1.cos(), a1.sin()) * r,
            ],
            Color32::from_rgba_unmultiplied(60, 70, 150, 70),
            Stroke::NONE,
        ));
    }
    // Halftone dots, bigger towards the bottom right.
    let step = 22.0;
    let mut y = screen.top();
    while y < screen.bottom() {
        let mut x = screen.left()
            + if ((y / step) as i32) % 2 == 0 {
                0.0
            } else {
                step * 0.5
            };
        while x < screen.right() {
            let f =
                ((x - screen.left()) / screen.width() + (y - screen.top()) / screen.height()) * 0.5;
            let rad = 1.0 + 3.2 * f * f;
            p.circle_filled(
                pos2(x, y),
                rad,
                Color32::from_rgba_unmultiplied(120, 140, 255, 26),
            );
            x += step;
        }
        y += step;
    }
}

/// HEROWORLD in gold with a red block shadow; returns where it is.
fn logo(p: &egui::Painter, at: Pos2, size: f32, t: f32) -> Rect {
    let font = FontId::new(size, logo_family());
    let galley = p.layout_no_wrap("HEROWORLD".into(), font.clone(), GOLD);
    let rect = Rect::from_min_size(at, galley.size());
    for k in (1..=7).rev() {
        let d = vec2(k as f32 * 0.9, k as f32 * 1.3);
        let col = if k == 7 {
            NAVY
        } else {
            Color32::from_rgb(170 - k * 12, 30, 34)
        };
        p.text(at + d, Align2::LEFT_TOP, "HEROWORLD", font.clone(), col);
    }
    p.text(at, Align2::LEFT_TOP, "HEROWORLD", font, GOLD);
    // A glint sweeping over it now and then.
    let phase = (t % 6.0) / 0.9;
    if phase < 1.0 {
        let x = rect.left() + phase * rect.width();
        let c = pos2(x, rect.top() + rect.height() * 0.35);
        let s = size * 0.18;
        p.line_segment(
            [c - vec2(s, 0.0), c + vec2(s, 0.0)],
            Stroke::new(2.0_f32, Color32::WHITE),
        );
        p.line_segment(
            [c - vec2(0.0, s), c + vec2(0.0, s)],
            Stroke::new(2.0_f32, Color32::WHITE),
        );
    }
    rect
}

/// The red ALPHA rubber stamp, the same as on the title screen.
fn stamp(p: &egui::Painter, corner: Pos2, size: f32, t: f32) {
    let angle = -0.21_f32 + 0.015 * (t * 1.3).sin();
    let galley = p.layout_no_wrap("ALPHA".into(), FontId::new(size, logo_family()), RED);
    let half = galley.size() * 0.5 + vec2(size * 0.35, size * 0.12);
    let centre = corner + vec2(-half.x * 0.5, -size * 0.2);
    let (s, c) = angle.sin_cos();
    let rot = |v: Vec2| vec2(v.x * c - v.y * s, v.x * s + v.y * c);
    let frame = |inset: f32| -> Vec<Pos2> {
        let h = half - vec2(inset, inset);
        [
            vec2(-h.x, -h.y),
            vec2(h.x, -h.y),
            vec2(h.x, h.y),
            vec2(-h.x, h.y),
        ]
        .into_iter()
        .map(|v| centre + rot(v))
        .collect()
    };
    p.add(Shape::convex_polygon(
        frame(0.0),
        CREAM,
        Stroke::new(size * 0.09_f32, RED),
    ));
    p.add(Shape::closed_line(
        frame(size * 0.16),
        Stroke::new(size * 0.035_f32, RED),
    ));
    let at = centre + rot(-galley.size() * 0.5);
    p.add(egui::epaint::TextShape::new(at, galley, RED).with_angle(angle));
}

fn comic_panel(p: &egui::Painter, r: Rect) {
    p.rect_filled(
        r.translate(vec2(7.0, 9.0)),
        CornerRadius::same(6),
        Color32::from_black_alpha(140),
    );
    p.rect_filled(r, CornerRadius::same(6), CREAM);
    p.rect_stroke(
        r,
        CornerRadius::same(6),
        Stroke::new(4.0_f32, NAVY),
        StrokeKind::Inside,
    );
}

fn big_button(
    p: &egui::Painter,
    rect: Rect,
    resp: &egui::Response,
    label: &str,
    sub: &str,
    enabled: bool,
    t: f32,
) {
    let hover = enabled && resp.hovered();
    let press = enabled && resp.is_pointer_button_down_on();
    let r = rect.shrink(if press { 3.0 } else { 0.0 });
    let round = CornerRadius::same((r.height() * 0.5) as u8);
    if enabled {
        let pulse = 0.5 + 0.5 * (t * 2.4).sin();
        p.rect_filled(
            r.expand(5.0 + 3.0 * pulse),
            CornerRadius::same((r.height() * 0.5 + 8.0) as u8),
            Color32::from_rgba_unmultiplied(255, 198, 64, (40.0 + 40.0 * pulse) as u8),
        );
    }
    p.rect_filled(
        r.translate(vec2(0.0, 6.0)),
        round,
        Color32::from_black_alpha(130),
    );
    let face = match (enabled, hover) {
        (false, _) => Color32::from_rgb(110, 110, 130),
        (true, true) => Color32::from_rgb(255, 90, 70),
        (true, false) => RED,
    };
    p.rect_filled(r, round, face);
    p.rect_stroke(r, round, Stroke::new(3.5_f32, NAVY), StrokeKind::Outside);
    let has_sub = !sub.is_empty();
    let y = if has_sub {
        r.center().y - 10.0
    } else {
        r.center().y
    };
    text(
        p,
        pos2(r.center().x, y),
        Align2::CENTER_CENTER,
        label,
        36.0,
        CREAM,
    );
    if has_sub {
        p.text(
            pos2(r.center().x, r.bottom() - 18.0),
            Align2::CENTER_CENTER,
            sub,
            FontId::new(14.0, ui_family()),
            CREAM,
        );
    }
    if enabled {
        resp.ctx.set_cursor_icon(if hover {
            egui::CursorIcon::PointingHand
        } else {
            egui::CursorIcon::Default
        });
    }
}

fn progress_bar(ui: &mut egui::Ui, width: f32, frac: f32, line: &str, t: f32) {
    ui.label(line);
    let (bar, _) = ui.allocate_exact_size(vec2(width, 22.0), Sense::hover());
    let p = ui.painter();
    let round = CornerRadius::same(11);
    p.rect_filled(bar, round, Color32::from_rgb(40, 46, 90));
    let fill = Rect::from_min_size(
        bar.min,
        vec2(bar.width() * frac.clamp(0.0, 1.0), bar.height()),
    );
    if fill.width() > 2.0 {
        p.rect_filled(fill, round, GOLD);
        let clip = p.with_clip_rect(fill);
        let step = 16.0;
        let mut x = fill.left() - bar.height() - step + (t * 30.0) % step;
        while x < fill.right() {
            clip.add(Shape::convex_polygon(
                vec![
                    pos2(x, fill.bottom()),
                    pos2(x + bar.height(), fill.top()),
                    pos2(x + bar.height() + step * 0.45, fill.top()),
                    pos2(x + step * 0.45, fill.bottom()),
                ],
                Color32::from_rgba_unmultiplied(255, 240, 180, 90),
                Stroke::NONE,
            ));
            x += step;
        }
    }
    p.rect_stroke(bar, round, Stroke::new(3.0_f32, NAVY), StrokeKind::Outside);
}

/// The window icon: a gold "H" on a sunburst (`icon.png`; `icon.ico` is
/// the same picture for the exe, see `build.rs`).
pub fn icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../icon.png")).unwrap_or_default()
}
