//! The police computer: a cheesy retro terminal in the HUD (the monitor
//! icon, or Y) that opens a desktop with every police tool as an icon,
//! and Crime-o-Vision, its crime watch: a glassy overlay, its opacity
//! wavering like a bad picture tube, listing the gang hideouts to bust
//! with a symbol for each crime, the distance, the gang and the level of
//! the gangsters. Taking one opens the case (the server's rules, see
//! `hw_sim::missions`); this only shows the board and says what was
//! chosen.

use crate::missionview::{Board, CrimeView};
use crate::uikit::{self, mix, with_alpha, AETHER, CREAM, DEEP, GOLD, GOLD_DARK, HERO_RED, NAVY};
use engine::ui::egui::{
    self, epaint::CornerRadius, pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Sense, Shape,
    Stroke, StrokeKind,
};
use glam::Vec3;
use hw_sim::missions::board::CrimeKind;
use hw_sim::missions::CaseKind;

/// The glass: Crime-o-Vision's green-teal.
const GLASS: Color32 = Color32::from_rgb(90, 225, 210);

/// A tool on the police computer's desktop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum App {
    CrimeOVision,
    CaseFile,
    Visor,
    Map,
    Powers,
}

impl App {
    pub const ALL: [App; 5] = [
        App::CrimeOVision,
        App::CaseFile,
        App::Visor,
        App::Map,
        App::Powers,
    ];

    pub fn name(self) -> &'static str {
        match self {
            App::CrimeOVision => "Crime-o-Vision",
            App::CaseFile => "Case files",
            App::Visor => "Crime scanner",
            App::Map => "City map",
            App::Powers => "Hero powers",
        }
    }

    /// The key that opens it without the computer.
    pub fn key(self) -> &'static str {
        match self {
            App::CrimeOVision => "Y",
            App::CaseFile => "J",
            App::Visor => "T",
            App::Map => "M",
            App::Powers => "B",
        }
    }
}

/// What the hero chose on the computer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Take crime number `n`.
    Take(u16),
    /// Open a tool (the game does it: it owns the windows).
    Open(App),
}

#[derive(Default)]
pub struct Police {
    /// The desktop.
    pub desktop: bool,
    /// Crime-o-Vision.
    pub crimes: bool,
    /// The pointer was freed for us and goes back when we close.
    pub relock: bool,
}

impl Police {
    /// The computer key: closes what is open, else opens the desktop.
    pub fn toggle(&mut self) {
        if self.desktop || self.crimes {
            self.desktop = false;
            self.crimes = false;
        } else {
            self.desktop = true;
        }
    }

    pub fn taking_pointer(&self) -> bool {
        self.desktop || self.crimes
    }

    /// The monitor icon in the HUD; true when clicked.
    pub fn icon(&self, ctx: &egui::Context, board: &Board) -> bool {
        let mut clicked = false;
        egui::Area::new(egui::Id::new("police-icon"))
            .anchor(Align2::RIGHT_TOP, vec2(-10.0, 62.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let (r, resp) = ui.allocate_exact_size(vec2(64.0, 58.0), Sense::click());
                let t = uikit::now(ctx);
                let hot = resp.hovered();
                let free = board.crimes.iter().filter(|c| !c.taken).count();
                monitor(ui.painter(), r, t, hot, free > 0);
                if free > 0 {
                    // A red badge: crimes waiting.
                    let c = r.right_top() + vec2(-6.0, 8.0);
                    ui.painter().circle_filled(c, 10.0, HERO_RED);
                    ui.painter()
                        .circle_stroke(c, 10.0, Stroke::new(2.0_f32, CREAM));
                    ui.painter().text(
                        c,
                        Align2::CENTER_CENTER,
                        free.min(99).to_string(),
                        FontId::proportional(12.0),
                        CREAM,
                    );
                }
                clicked = resp.on_hover_text("Police computer (Y)").clicked();
            });
        clicked
    }

    /// The desktop and Crime-o-Vision, whichever is open. `hero` is where
    /// the hero stands, for the distances.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        board: &Board,
        hero: Option<Vec3>,
        hero_level: u8,
    ) -> Vec<Action> {
        let mut out = Vec::new();
        if self.desktop {
            let mut open = true;
            let win = egui::Window::new("police desktop")
                .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                .resizable(false)
                .collapsible(false);
            crate::googie::window(ctx, win, "Police computer", Some(&mut open), |ui| {
                ui.label(
                    egui::RichText::new("METRO POLICE DEPT.  -  officer login: HERO")
                        .color(AETHER)
                        .size(12.0),
                );
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
                    ui.set_max_width(5.0 * 112.0 + 4.0 * 12.0);
                    for app in App::ALL {
                        if tile(ui, app, board) {
                            out.push(Action::Open(app));
                        }
                    }
                });
            });
            if !open {
                self.desktop = false;
            }
        }
        for a in &out {
            if *a == Action::Open(App::CrimeOVision) {
                self.crimes = true;
                self.desktop = false;
            }
        }
        if self.crimes {
            crime_o_vision(ctx, self, board, hero, hero_level, &mut out);
        }
        out
    }
}

/// One desktop icon with its name and key; true when clicked.
fn tile(ui: &mut egui::Ui, app: App, board: &Board) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(112.0, 118.0), Sense::click());
    let t = uikit::now(ui.ctx());
    let p = ui.painter();
    let hot = resp.hovered();
    let fill = if hot {
        Color32::from_rgb(40, 52, 120)
    } else {
        with_alpha(NAVY, 0.9)
    };
    p.rect_filled(r, CornerRadius::same(12), fill);
    p.rect_stroke(
        r,
        CornerRadius::same(12),
        Stroke::new(
            if hot { 2.5_f32 } else { 1.5 },
            if hot { GOLD } else { GOLD_DARK },
        ),
        StrokeKind::Inside,
    );
    app_icon(p, app, pos2(r.center().x, r.top() + 46.0), 30.0, t);
    p.text(
        pos2(r.center().x, r.bottom() - 30.0),
        Align2::CENTER_CENTER,
        app.name(),
        uikit::ui_font(ui.ctx(), 14.0),
        CREAM,
    );
    p.text(
        pos2(r.center().x, r.bottom() - 12.0),
        Align2::CENTER_CENTER,
        format!("key {}", app.key()),
        FontId::proportional(11.0),
        mix(AETHER, DEEP, 0.3),
    );
    if app == App::CrimeOVision {
        let free = board.crimes.iter().filter(|c| !c.taken).count();
        if free > 0 {
            let c = r.right_top() + vec2(-12.0, 12.0);
            p.circle_filled(c, 10.0, HERO_RED);
            p.text(
                c,
                Align2::CENTER_CENTER,
                free.min(99).to_string(),
                FontId::proportional(12.0),
                CREAM,
            );
        }
    }
    resp.clicked()
}

/// The picture tube's wavering: a steady ~85% with slow and fast
/// ripples and now and then a glitch.
fn flicker(ctx: &egui::Context) -> f32 {
    if uikit::reduced_motion(ctx) {
        return 0.9;
    }
    let t = uikit::now(ctx);
    let glitch = if (t * 0.9).fract() < 0.035 {
        -0.22
    } else {
        0.0
    };
    (0.84 + 0.06 * (t * 4.3).sin() + 0.04 * (t * 13.1 + 1.0).sin() + glitch).clamp(0.5, 1.0)
}

const WIDTH: f32 = 780.0;
const ROW: f32 = 62.0;

/// Crime-o-Vision: the crime watch overlay.
fn crime_o_vision(
    ctx: &egui::Context,
    police: &mut Police,
    board: &Board,
    hero: Option<Vec3>,
    hero_level: u8,
    out: &mut Vec<Action>,
) {
    let alpha = flicker(ctx);
    let t = uikit::now(ctx);
    // Nearest first.
    let mut crimes: Vec<(&CrimeView, f32)> = board
        .crimes
        .iter()
        .map(|c| (c, hero.map_or(0.0, |h| flat(h, c.at))))
        .collect();
    crimes.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.number.cmp(&b.0.number)));
    let height = 150.0 + ROW * crimes.len().max(1) as f32;
    egui::Area::new(egui::Id::new("crime-o-vision"))
        .anchor(Align2::CENTER_CENTER, vec2(0.0, -56.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(WIDTH, height), Sense::hover());
            let p = ui.painter().clone();
            glass(&p, rect, alpha, t);
            // The title: a red sign plate, with an eye on the tube.
            let font = crate::googie::sign_font(ctx, 28.0);
            crate::googie::sign(
                &p,
                rect.left_top() + vec2(26.0, 14.0),
                "CRIME-O-VISION",
                font,
                with_alpha(HERO_RED, alpha),
                0.0,
                t,
            );
            eye(&p, rect.right_top() + vec2(-98.0, 38.0), 22.0, alpha, t);
            p.text(
                rect.left_top() + vec2(30.0, 74.0),
                Align2::LEFT_CENTER,
                "METRO POLICE DEPT.   GANG HIDEOUT WATCH",
                FontId::monospace(12.0),
                with_alpha(GLASS, alpha),
            );
            p.text(
                rect.right_top() + vec2(-30.0, 74.0),
                Align2::RIGHT_CENTER,
                format!("CASES OPEN {} / {}", board.open, board.limit.max(1)),
                FontId::monospace(12.0),
                with_alpha(GOLD, alpha),
            );
            // Close.
            let x = Rect::from_center_size(rect.right_top() + vec2(-34.0, 36.0), vec2(30.0, 30.0));
            let xr = ui.interact(x, egui::Id::new("cov-close"), Sense::click());
            crate::googie::sputnik(
                &p,
                x.center(),
                10.0,
                t * 0.5,
                with_alpha(if xr.hovered() { HERO_RED } else { GOLD }, alpha),
                with_alpha(AETHER, alpha),
            );
            if xr.clicked() {
                police.crimes = false;
            }
            if crimes.is_empty() {
                p.text(
                    rect.center() + vec2(0.0, 20.0),
                    Align2::CENTER_CENTER,
                    "No crimes reported. Enjoy the peace and quiet.",
                    uikit::ui_font(ctx, 17.0),
                    with_alpha(CREAM, alpha),
                );
            }
            for (i, (c, dist)) in crimes.iter().enumerate() {
                let r = Rect::from_min_size(
                    rect.left_top() + vec2(22.0, 96.0 + i as f32 * ROW),
                    vec2(WIDTH - 44.0, ROW - 6.0),
                );
                if let Some(a) = row(ui, &p, r, c, *dist, hero_level, board, alpha, t) {
                    out.push(a);
                }
            }
            p.text(
                rect.center_bottom() + vec2(0.0, -16.0),
                Align2::CENTER_CENTER,
                "Take a case, then follow the teal marker on your wrist radio.",
                FontId::monospace(12.0),
                with_alpha(mix(GLASS, DEEP, 0.2), alpha),
            );
        });
}

fn flat(a: Vec3, b: Vec3) -> f32 {
    ((a.x - b.x).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

fn distance_text(m: f32) -> String {
    if m < 1000.0 {
        format!("{} m", (m / 10.0).round() as i32 * 10)
    } else {
        format!("{:.1} km", m / 1000.0)
    }
}

/// The short names the board writes for the gangs.
pub fn gang_tag(gang: usize) -> &'static str {
    [
        "Technicians",
        "Chrome Jackets",
        "Sky Pirates",
        "Iron Empire",
        "Zorrak",
    ]
    .get(gang)
    .copied()
    .unwrap_or("Gang")
}

fn gang_colour(gang: usize) -> Color32 {
    [
        Color32::from_rgb(120, 236, 255),
        Color32::from_rgb(255, 130, 190),
        Color32::from_rgb(255, 170, 70),
        Color32::from_rgb(200, 200, 215),
        Color32::from_rgb(150, 255, 110),
    ][gang.min(4)]
}

/// One crime: its symbol, what and where, the job, gang, level,
/// distance, and the button.
#[allow(clippy::too_many_arguments)]
fn row(
    ui: &mut egui::Ui,
    p: &egui::Painter,
    r: Rect,
    c: &CrimeView,
    dist: f32,
    hero_level: u8,
    board: &Board,
    alpha: f32,
    t: f32,
) -> Option<Action> {
    let a = |col: Color32| with_alpha(col, alpha);
    p.rect_filled(
        r,
        CornerRadius::same(10),
        a(Color32::from_rgba_unmultiplied(10, 40, 50, 150)),
    );
    p.rect_stroke(
        r,
        CornerRadius::same(10),
        Stroke::new(
            1.2_f32,
            a(with_alpha(GLASS, if c.taken { 0.9 } else { 0.4 })),
        ),
        StrokeKind::Inside,
    );
    // The crime's symbol on a round plate.
    let ic = pos2(r.left() + 36.0, r.center().y);
    p.circle_filled(ic, 24.0, a(Color32::from_rgb(18, 26, 70)));
    p.circle_stroke(ic, 24.0, Stroke::new(2.0_f32, a(GOLD)));
    crime_icon(p, c.crime, ic, 17.0, alpha);
    // What and where.
    let x = r.left() + 74.0;
    p.text(
        pos2(x, r.top() + 17.0),
        Align2::LEFT_CENTER,
        c.crime.title(),
        uikit::ui_font(ui.ctx(), 17.0),
        a(CREAM),
    );
    p.text(
        pos2(x, r.top() + 39.0),
        Align2::LEFT_CENTER,
        &c.name,
        FontId::proportional(13.0),
        a(mix(CREAM, DEEP, 0.3)),
    );
    // The job: cuffs or a magnifier.
    let jc = pos2(r.left() + 340.0, r.top() + 20.0);
    job_icon(p, c.job, jc, 11.0, alpha);
    p.text(
        jc + vec2(18.0, 0.0),
        Align2::LEFT_CENTER,
        match c.job {
            CaseKind::Evidence => "FIND THE THING",
            _ => "ARREST THE BOSS",
        },
        FontId::monospace(12.0),
        a(GOLD),
    );
    // The gang, as a coloured badge.
    let gc = pos2(r.left() + 340.0, r.top() + 44.0);
    p.circle_filled(gc, 7.0, a(gang_colour(c.gang)));
    p.circle_stroke(gc, 7.0, Stroke::new(1.5_f32, a(DEEP)));
    p.text(
        gc + vec2(18.0, 0.0),
        Align2::LEFT_CENTER,
        gang_tag(c.gang),
        FontId::proportional(13.0),
        a(CREAM),
    );
    // Level: a star chip, green when easy, red when tough.
    let diff = c.level as i32 - hero_level as i32;
    let tone = match diff {
        i32::MIN..=-2 => Color32::from_rgb(110, 220, 120),
        -1..=0 => GOLD,
        1 => Color32::from_rgb(255, 150, 60),
        _ => HERO_RED,
    };
    let lc = pos2(r.left() + 520.0, r.top() + 20.0);
    crate::googie::star_icon(p, lc + vec2(-22.0, 0.0), 11.0);
    p.text(
        lc + vec2(-6.0, 0.0),
        Align2::LEFT_CENTER,
        format!("Lv {}", c.level),
        uikit::ui_font(ui.ctx(), 16.0),
        a(tone),
    );
    // How big the hideout is: one, two or three little houses lit, each
    // taller than the last.
    for k in 0..3u8 {
        let h = 7.0 + 4.0 * k as f32;
        let x = lc.x - 30.0 + 12.0 * k as f32;
        let base = r.top() + 51.0;
        let house = Rect::from_min_max(pos2(x, base - h), pos2(x + 9.0, base));
        let lit = k <= c.size;
        p.rect_filled(
            house,
            CornerRadius::same(1),
            a(if lit {
                AETHER
            } else {
                with_alpha(AETHER, 0.18)
            }),
        );
        if lit {
            p.rect_filled(
                Rect::from_center_size(house.center_top() + vec2(0.0, 3.5), vec2(3.0, 3.0)),
                CornerRadius::ZERO,
                a(DEEP),
            );
        }
    }
    p.text(
        pos2(lc.x + 10.0, r.top() + 45.0),
        Align2::LEFT_CENTER,
        ["DEN", "HIDEOUT", "FORTRESS"][(c.size as usize).min(2)],
        FontId::monospace(10.0),
        a(with_alpha(AETHER, 0.9)),
    );
    // Time left on the board, and a hot tip's flag.
    if c.minutes != 255 {
        let soon = c.minutes <= 3;
        p.text(
            pos2(r.left() + 585.0, r.top() + 45.0),
            Align2::LEFT_CENTER,
            format!("{} MIN", c.minutes),
            FontId::monospace(11.0),
            a(if soon {
                HERO_RED
            } else {
                with_alpha(CREAM, 0.7)
            }),
        );
    }
    if c.hot {
        let pulse = if uikit::reduced_motion(ui.ctx()) {
            1.0
        } else {
            0.7 + 0.3 * (t * 5.0).sin()
        };
        let tag = Rect::from_min_size(pos2(r.left() + 236.0, r.top() + 8.0), vec2(72.0, 18.0));
        p.rect_filled(tag, CornerRadius::same(9), a(with_alpha(HERO_RED, pulse)));
        crate::googie::bolt_icon(p, tag.left_center() + vec2(10.0, 0.0), 6.0);
        p.text(
            tag.center() + vec2(7.0, 0.0),
            Align2::CENTER_CENTER,
            "HOT TIP",
            FontId::monospace(10.0),
            a(CREAM),
        );
    }
    // Distance.
    p.text(
        pos2(r.left() + 585.0, r.top() + 20.0),
        Align2::LEFT_CENTER,
        distance_text(dist),
        FontId::monospace(15.0),
        a(AETHER),
    );
    // The button.
    let b = Rect::from_center_size(pos2(r.right() - 44.0, r.center().y), vec2(76.0, 34.0));
    let full = board.open >= board.limit && !c.taken;
    let resp = ui.interact(b, egui::Id::new(("cov-take", c.number)), Sense::click());
    let (label, fill) = if c.taken {
        ("ON IT", mix(GLASS, DEEP, 0.6))
    } else if full {
        ("FULL", Color32::from_rgb(80, 80, 96))
    } else if resp.hovered() {
        ("TAKE", mix(HERO_RED, Color32::WHITE, 0.2))
    } else {
        ("TAKE", HERO_RED)
    };
    p.rect_filled(b, CornerRadius::same(8), a(fill));
    p.rect_stroke(
        b,
        CornerRadius::same(8),
        Stroke::new(2.0_f32, a(GOLD)),
        StrokeKind::Inside,
    );
    p.text(
        b.center(),
        Align2::CENTER_CENTER,
        label,
        uikit::ui_font(ui.ctx(), 15.0),
        a(CREAM),
    );
    if c.taken {
        // A blinking dot: someone is on it.
        let on = if uikit::reduced_motion(ui.ctx()) {
            1.0
        } else {
            0.5 + 0.5 * (t * 6.0).sin()
        };
        p.circle_filled(b.left_top() + vec2(8.0, 8.0), 4.0, a(with_alpha(GLASS, on)));
    }
    (resp.clicked() && !c.taken && !full).then_some(Action::Take(c.number))
}

/// The tube: a dark glassy panel with a teal rim, scanlines and a bright
/// band rolling down it.
fn glass(p: &egui::Painter, r: Rect, alpha: f32, t: f32) {
    let pts = crate::googie::outline(r, 26.0, 14.0);
    let shadow: Vec<Pos2> = pts.iter().map(|q| *q + vec2(6.0, 7.0)).collect();
    p.add(Shape::convex_polygon(
        shadow,
        Color32::from_black_alpha((120.0 * alpha) as u8),
        Stroke::NONE,
    ));
    crate::googie::gradient_polygon(
        p,
        &pts,
        with_alpha(Color32::from_rgb(10, 44, 58), 0.9 * alpha),
        with_alpha(Color32::from_rgb(6, 16, 36), 0.9 * alpha),
    );
    crate::googie::formica(p, r.shrink(24.0), 77, 0.08 * alpha);
    let mut y = r.top() + 8.0;
    while y < r.bottom() - 8.0 {
        p.line_segment(
            [pos2(r.left() + 10.0, y), pos2(r.right() - 10.0, y)],
            Stroke::new(1.0_f32, Color32::from_black_alpha((46.0 * alpha) as u8)),
        );
        y += 4.0;
    }
    // The rolling band.
    let band = r.top() + ((t * 0.18) % 1.0) * r.height();
    p.rect_filled(
        Rect::from_min_max(
            pos2(r.left() + 8.0, band),
            pos2(r.right() - 8.0, band + 26.0),
        ),
        CornerRadius::ZERO,
        with_alpha(GLASS, 0.05 * alpha),
    );
    p.add(Shape::closed_line(
        pts,
        Stroke::new(3.0_f32, with_alpha(GLASS, alpha)),
    ));
    p.add(Shape::closed_line(
        crate::googie::outline(r.shrink(6.0), 22.0, 10.0),
        Stroke::new(1.0_f32, with_alpha(GOLD, 0.7 * alpha)),
    ));
}

/// An eye in a ring: Crime-o-Vision's mark.
fn eye(p: &egui::Painter, c: Pos2, r: f32, alpha: f32, t: f32) {
    let a = |col: Color32| with_alpha(col, alpha);
    let look = (t * 1.3).sin() * r * 0.25;
    let pts: Vec<Pos2> = (0..=16)
        .map(|k| {
            let u = k as f32 / 16.0 * 2.0 - 1.0;
            pos2(c.x + u * r * 1.5, c.y - (1.0 - u * u) * r * 0.62)
        })
        .chain((0..=16).rev().map(|k| {
            let u = k as f32 / 16.0 * 2.0 - 1.0;
            pos2(c.x + u * r * 1.5, c.y + (1.0 - u * u) * r * 0.62)
        }))
        .collect();
    p.add(Shape::convex_polygon(
        pts.clone(),
        a(CREAM),
        Stroke::new(2.0_f32, a(GOLD)),
    ));
    p.circle_filled(c + vec2(look, 0.0), r * 0.5, a(GLASS));
    p.circle_filled(c + vec2(look, 0.0), r * 0.24, a(DEEP));
    p.circle_filled(
        c + vec2(look - r * 0.12, -r * 0.14),
        r * 0.09,
        a(Color32::WHITE),
    );
}

/// The monitor on the HUD: a rounded tube with antennae, knobs, a
/// scanning eye and a blinking lamp.
fn monitor(p: &egui::Painter, r: Rect, t: f32, hot: bool, alert: bool) {
    let body = Rect::from_min_max(r.min + vec2(4.0, 14.0), r.max - vec2(4.0, 2.0));
    // Antennae.
    p.line_segment(
        [
            pos2(body.center().x - 4.0, body.top()),
            pos2(body.center().x - 18.0, r.top() + 2.0),
        ],
        Stroke::new(2.0_f32, GOLD_DARK),
    );
    p.line_segment(
        [
            pos2(body.center().x + 4.0, body.top()),
            pos2(body.center().x + 16.0, r.top() + 4.0),
        ],
        Stroke::new(2.0_f32, GOLD_DARK),
    );
    p.circle_filled(pos2(body.center().x - 18.0, r.top() + 2.0), 2.5, GOLD);
    p.circle_filled(pos2(body.center().x + 16.0, r.top() + 4.0), 2.5, GOLD);
    p.rect_filled(
        body.translate(vec2(3.0, 4.0)),
        CornerRadius::same(10),
        Color32::from_black_alpha(130),
    );
    p.rect_filled(
        body,
        CornerRadius::same(10),
        if hot {
            Color32::from_rgb(52, 62, 140)
        } else {
            NAVY
        },
    );
    p.rect_stroke(
        body,
        CornerRadius::same(10),
        Stroke::new(if hot { 3.0_f32 } else { 2.5 }, GOLD),
        StrokeKind::Inside,
    );
    let screen = Rect::from_min_max(
        body.min + vec2(6.0, 6.0),
        pos2(body.right() - 18.0, body.bottom() - 6.0),
    );
    p.rect_filled(screen, CornerRadius::same(7), Color32::from_rgb(8, 40, 46));
    p.rect_stroke(
        screen,
        CornerRadius::same(7),
        Stroke::new(1.5_f32, GLASS),
        StrokeKind::Inside,
    );
    eye(
        p,
        screen.center(),
        6.5,
        0.6 + 0.4 * (t * 2.0).sin().abs(),
        t,
    );
    // Knobs and the lamp.
    p.circle_filled(
        pos2(body.right() - 9.0, body.top() + 12.0),
        3.5,
        GOLD_LIGHT_ISH,
    );
    p.circle_filled(
        pos2(body.right() - 9.0, body.top() + 23.0),
        3.5,
        GOLD_LIGHT_ISH,
    );
    let lamp = if alert && (t * 2.5).fract() < 0.5 {
        HERO_RED
    } else {
        Color32::from_rgb(90, 30, 30)
    };
    p.circle_filled(pos2(body.right() - 9.0, body.bottom() - 9.0), 3.0, lamp);
}

const GOLD_LIGHT_ISH: Color32 = Color32::from_rgb(255, 226, 140);

/// A desktop icon, centred at `c`, about `2r` across.
fn app_icon(p: &egui::Painter, app: App, c: Pos2, r: f32, t: f32) {
    match app {
        App::CrimeOVision => {
            p.circle_filled(c, r, Color32::from_rgb(8, 40, 46));
            p.circle_stroke(c, r, Stroke::new(2.5_f32, GLASS));
            eye(p, c, r * 0.6, 1.0, t);
        }
        App::CaseFile => {
            // A manila folder with a tab and a red stamp.
            let b = Rect::from_center_size(c + vec2(0.0, 3.0), vec2(r * 1.8, r * 1.4));
            let manila = Color32::from_rgb(232, 196, 120);
            p.rect_filled(
                Rect::from_min_size(b.left_top() + vec2(0.0, -6.0), vec2(r * 0.8, 8.0)),
                CornerRadius {
                    nw: 3,
                    ne: 3,
                    sw: 0,
                    se: 0,
                },
                mix(manila, Color32::BLACK, 0.15),
            );
            p.rect_filled(b, CornerRadius::same(4), manila);
            p.rect_stroke(
                b,
                CornerRadius::same(4),
                Stroke::new(1.5_f32, GOLD_DARK),
                StrokeKind::Inside,
            );
            p.circle_stroke(
                b.center() + vec2(r * 0.25, 0.0),
                r * 0.38,
                Stroke::new(2.5_f32, HERO_RED),
            );
        }
        App::Visor => {
            // The visor: a wide band with two glowing lenses.
            let b = Rect::from_center_size(c, vec2(r * 2.0, r * 0.95));
            p.rect_filled(b, CornerRadius::same(10), Color32::from_rgb(30, 34, 80));
            p.rect_stroke(
                b,
                CornerRadius::same(10),
                Stroke::new(2.0_f32, GOLD),
                StrokeKind::Inside,
            );
            for s in [-1.0_f32, 1.0] {
                let l = c + vec2(s * r * 0.5, 0.0);
                p.circle_filled(l, r * 0.3, GLASS);
                p.circle_stroke(l, r * 0.3, Stroke::new(1.5_f32, CREAM));
                p.circle_filled(l + vec2(-r * 0.08, -r * 0.08), r * 0.07, Color32::WHITE);
            }
        }
        App::Map => {
            // A folded map: three panels, a route and a pin.
            let b = Rect::from_center_size(c, vec2(r * 1.9, r * 1.4));
            let paper = Color32::from_rgb(240, 228, 190);
            for k in 0..3 {
                let w = b.width() / 3.0;
                let panel = Rect::from_min_size(
                    b.left_top() + vec2(k as f32 * w, if k == 1 { 3.0 } else { 0.0 }),
                    vec2(w, b.height() - if k == 1 { 3.0 } else { 0.0 }),
                );
                p.rect_filled(
                    panel,
                    CornerRadius::ZERO,
                    mix(paper, Color32::BLACK, 0.06 * k as f32),
                );
                p.rect_stroke(
                    panel,
                    CornerRadius::ZERO,
                    Stroke::new(1.0_f32, GOLD_DARK),
                    StrokeKind::Inside,
                );
            }
            p.line_segment(
                [
                    b.left_bottom() + vec2(4.0, -5.0),
                    b.center() + vec2(2.0, 0.0),
                ],
                Stroke::new(2.0_f32, HERO_RED),
            );
            p.line_segment(
                [b.center() + vec2(2.0, 0.0), b.right_top() + vec2(-6.0, 6.0)],
                Stroke::new(2.0_f32, HERO_RED),
            );
            p.circle_filled(b.right_top() + vec2(-6.0, 6.0), 3.5, HERO_RED);
        }
        App::Powers => crate::googie::bolt_icon(p, c, r),
    }
}

/// A crime's symbol, centred at `c`, about `2r` across.
pub fn crime_icon(p: &egui::Painter, kind: CrimeKind, c: Pos2, r: f32, alpha: f32) {
    let a = |col: Color32| with_alpha(col, alpha);
    let ink = Stroke::new(1.6_f32, a(DEEP));
    match kind {
        CrimeKind::Payroll => {
            // A money sack tied at the neck, with a dollar sign.
            p.circle_filled(
                c + vec2(0.0, r * 0.25),
                r * 0.85,
                a(Color32::from_rgb(150, 190, 90)),
            );
            p.circle_stroke(c + vec2(0.0, r * 0.25), r * 0.85, ink);
            p.add(Shape::convex_polygon(
                vec![
                    c + vec2(-r * 0.3, -r * 0.45),
                    c + vec2(r * 0.3, -r * 0.45),
                    c + vec2(r * 0.18, -r * 0.1),
                    c + vec2(-r * 0.18, -r * 0.1),
                ],
                a(Color32::from_rgb(150, 190, 90)),
                ink,
            ));
            p.line_segment(
                [c + vec2(-r * 0.34, -r * 0.5), c + vec2(r * 0.34, -r * 0.5)],
                Stroke::new(2.5_f32, a(HERO_RED)),
            );
            p.text(
                c + vec2(0.0, r * 0.3),
                Align2::CENTER_CENTER,
                "$",
                FontId::proportional(r * 1.0),
                a(DEEP),
            );
        }
        CrimeKind::Counterfeit => {
            // A banknote with a portrait and a corner mark, a second
            // one peeking out behind.
            let n = Rect::from_center_size(c, vec2(r * 1.9, r * 1.1));
            p.rect_filled(
                n.translate(vec2(-r * 0.15, -r * 0.2)),
                CornerRadius::same(2),
                a(Color32::from_rgb(120, 170, 110)),
            );
            p.rect_filled(
                n.translate(vec2(r * 0.1, r * 0.12)),
                CornerRadius::same(2),
                a(Color32::from_rgb(170, 220, 150)),
            );
            p.rect_stroke(
                n.translate(vec2(r * 0.1, r * 0.12)),
                CornerRadius::same(2),
                ink,
                StrokeKind::Inside,
            );
            p.circle_stroke(c + vec2(r * 0.1, r * 0.12), r * 0.3, ink);
            p.text(
                c + vec2(r * 0.1, r * 0.12),
                Align2::CENTER_CENTER,
                "?",
                FontId::proportional(r * 0.6),
                a(HERO_RED),
            );
        }
        CrimeKind::Smuggling => {
            // A crate with a cross-strapped lid and a stencilled arrow.
            let b = Rect::from_center_size(c, vec2(r * 1.7, r * 1.5));
            p.rect_filled(b, CornerRadius::same(2), a(Color32::from_rgb(190, 140, 80)));
            p.rect_stroke(b, CornerRadius::same(2), ink, StrokeKind::Inside);
            p.line_segment(
                [b.left_top(), b.right_bottom()],
                Stroke::new(2.0_f32, a(Color32::from_rgb(120, 80, 40))),
            );
            p.line_segment(
                [b.right_top(), b.left_bottom()],
                Stroke::new(2.0_f32, a(Color32::from_rgb(120, 80, 40))),
            );
            p.line_segment([b.left_center(), b.right_center()], ink);
        }
        CrimeKind::Prototype => {
            // A retro ray gun: a fin, a barrel and a glowing tip.
            p.add(Shape::convex_polygon(
                vec![
                    c + vec2(-r * 0.9, -r * 0.2),
                    c + vec2(r * 0.55, -r * 0.35),
                    c + vec2(r * 0.55, r * 0.1),
                    c + vec2(-r * 0.4, r * 0.25),
                ],
                a(Color32::from_rgb(200, 210, 230)),
                ink,
            ));
            p.add(Shape::convex_polygon(
                vec![
                    c + vec2(-r * 0.4, r * 0.25),
                    c + vec2(-r * 0.05, r * 0.2),
                    c + vec2(-r * 0.3, r * 0.95),
                ],
                a(HERO_RED),
                ink,
            ));
            p.circle_filled(c + vec2(r * 0.75, -r * 0.12), r * 0.28, a(AETHER));
            p.circle_stroke(c + vec2(r * 0.75, -r * 0.12), r * 0.28, ink);
            p.circle_filled(c + vec2(-r * 0.55, -r * 0.1), r * 0.15, a(GOLD));
        }
        CrimeKind::Art => {
            // A gilt frame round a landscape: a hill and a sun.
            let b = Rect::from_center_size(c, vec2(r * 1.8, r * 1.5));
            p.rect_filled(b, CornerRadius::same(2), a(GOLD));
            let inner = b.shrink(r * 0.2);
            p.rect_filled(
                inner,
                CornerRadius::ZERO,
                a(Color32::from_rgb(120, 190, 230)),
            );
            p.add(Shape::convex_polygon(
                vec![
                    inner.left_bottom(),
                    inner.left_bottom() + vec2(r * 0.5, -r * 0.55),
                    inner.right_bottom() + vec2(-r * 0.2, 0.0),
                    inner.right_bottom(),
                ],
                a(Color32::from_rgb(80, 150, 80)),
                Stroke::NONE,
            ));
            p.circle_filled(
                inner.right_top() + vec2(-r * 0.3, r * 0.3),
                r * 0.16,
                a(Color32::from_rgb(255, 230, 120)),
            );
            p.rect_stroke(b, CornerRadius::same(2), ink, StrokeKind::Inside);
        }
        CrimeKind::Racket => {
            // A ledger with a fist-sized red stamp and ruled lines.
            let b = Rect::from_center_size(c, vec2(r * 1.5, r * 1.8));
            p.rect_filled(b, CornerRadius::same(3), a(Color32::from_rgb(110, 40, 40)));
            p.rect_stroke(b, CornerRadius::same(3), ink, StrokeKind::Inside);
            let page = b.shrink2(vec2(r * 0.18, r * 0.2));
            p.rect_filled(page, CornerRadius::same(1), a(CREAM));
            for k in 0..4 {
                let y = page.top() + r * 0.28 + k as f32 * r * 0.28;
                p.line_segment(
                    [pos2(page.left() + 3.0, y), pos2(page.right() - 3.0, y)],
                    Stroke::new(1.0_f32, a(Color32::from_rgb(120, 140, 200))),
                );
            }
            p.text(
                c + vec2(r * 0.15, r * 0.35),
                Align2::CENTER_CENTER,
                "$",
                FontId::proportional(r * 0.8),
                a(HERO_RED),
            );
        }
    }
}

/// A job's symbol: handcuffs or a magnifying glass.
fn job_icon(p: &egui::Painter, job: CaseKind, c: Pos2, r: f32, alpha: f32) {
    let a = |col: Color32| with_alpha(col, alpha);
    match job {
        CaseKind::Evidence => {
            p.circle_filled(
                c + vec2(-r * 0.15, -r * 0.15),
                r * 0.7,
                a(with_alpha(GLASS, 0.35)),
            );
            p.circle_stroke(
                c + vec2(-r * 0.15, -r * 0.15),
                r * 0.7,
                Stroke::new(2.0_f32, a(GOLD)),
            );
            p.line_segment(
                [c + vec2(r * 0.35, r * 0.35), c + vec2(r, r)],
                Stroke::new(3.0_f32, a(GOLD)),
            );
        }
        _ => {
            // Handcuffs: two rings and a link.
            for s in [-1.0_f32, 1.0] {
                p.circle_stroke(
                    c + vec2(s * r * 0.6, 0.0),
                    r * 0.5,
                    Stroke::new(2.4_f32, a(GOLD)),
                );
            }
            p.line_segment(
                [c + vec2(-r * 0.1, 0.0), c + vec2(r * 0.1, 0.0)],
                Stroke::new(2.4_f32, a(GOLD)),
            );
        }
    }
}
