//! The case file (J): Hero Hall's manila folder of the hero's cases. The
//! open case typed up on a sheet (where, who, what, Annie's notes in the
//! margin, a mugshot of the gang's boss clipped on, a checklist ticked off
//! as the rooms fall), the Doctor's dossier (chapters solved stamped,
//! the ones to come blacked out) and the cabinet of closed cases. A new
//! case slides it in with a NEW CASE stamp, a closed one with CASE CLOSED.
//! It only shows what the server said (`missionview`, `storyview`).

use crate::missionview::{MissionView, State};
use crate::storyview::StoryView;
use egui::epaint::TextShape;
use egui::{pos2, vec2, Align2, Color32, FontFamily, FontId, Pos2, Rect, Stroke};
use engine::ui::egui;
use hw_sim::missions::{CaseKind, CONTACT_NAME, DONE, GANGS, GO, GUARDED, IDLE};
use hw_sim::story::{CHAPTERS, LOCKED};

/// Seconds the file stays up when a case opens or closes on its own.
const AUTO_SECONDS: f32 = 7.0;
/// Seconds a stamp takes to come down.
const STAMP_SECONDS: f32 = 0.35;

const MANILA: Color32 = Color32::from_rgb(214, 178, 112);
const MANILA_DARK: Color32 = Color32::from_rgb(170, 132, 74);
const PAPER: Color32 = Color32::from_rgb(246, 239, 220);
const TYPE: Color32 = Color32::from_rgb(38, 34, 30);
const FADED: Color32 = Color32::from_rgb(120, 108, 92);
const STAMP_RED: Color32 = Color32::from_rgb(196, 40, 36);
const INK_BLUE: Color32 = Color32::from_rgb(34, 70, 160);
const TEAL: Color32 = Color32::from_rgb(20, 120, 112);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Case,
    Doctor,
    Closed,
}

/// A case in the cabinet.
#[derive(Clone, Debug, PartialEq)]
pub struct Closed {
    pub number: u16,
    pub kind: CaseKind,
    pub gang: usize,
    pub building: &'static str,
}

/// A stamp coming down on the sheet.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Stamp {
    New,
    Closed,
}

#[derive(Default)]
pub struct CaseFile {
    pub open: bool,
    /// Opened with J (it takes the pointer); else it came up on its own.
    pinned: bool,
    pub tab: Tab,
    /// Seconds it has been up.
    up: f32,
    /// Opened on its own: closes again at this many seconds up.
    auto_until: Option<f32>,
    stamp: Option<(Stamp, f32)>,
    /// The case just closed, shown under its stamp.
    just_closed: Option<Closed>,
    last: Option<State>,
    last_building: Option<&'static str>,
    pub history: Vec<Closed>,
    /// The pointer was let go for the file.
    pub relock: bool,
}

impl CaseFile {
    /// J: open or shut (a file that came up on its own stays, pinned).
    pub fn toggle(&mut self) {
        if self.open && self.pinned {
            self.open = false;
            self.pinned = false;
        } else {
            if !self.open {
                self.up = 0.0;
            }
            self.open = true;
            self.pinned = true;
            self.auto_until = None;
        }
    }

    /// Open with J: the pointer is let go for the tabs.
    pub fn taking_pointer(&self) -> bool {
        self.open && self.pinned
    }

    /// Every frame: news of the case (a new one, one closed) and time.
    pub fn update(&mut self, cases: &MissionView, dt: f32) {
        if self.open {
            self.up += dt;
        }
        if let Some((_, t)) = &mut self.stamp {
            *t += dt;
        }
        if let (Some(until), false) = (self.auto_until, self.pinned) {
            if self.up >= until {
                self.open = false;
                self.auto_until = None;
                self.stamp = None;
                self.just_closed = None;
            }
        }
        let now = cases.state.as_ref();
        if now != self.last.as_ref() {
            if let (Some(before), Some(now)) = (self.last.as_ref(), now) {
                if now.closed > before.closed {
                    let c = Closed {
                        number: now.closed,
                        kind: before.kind,
                        gang: before.gang,
                        building: self.last_building.unwrap_or("building"),
                    };
                    self.history.push(c.clone());
                    self.just_closed = Some(c);
                    self.pop(Stamp::Closed);
                } else if before.stage == IDLE && now.stage == GO {
                    self.just_closed = None;
                    self.pop(Stamp::New);
                }
            }
            self.last = now.cloned();
        }
        if let Some(b) = cases.building() {
            self.last_building = Some(b);
        }
    }

    /// Up on its own, on the case tab, with a stamp coming down.
    fn pop(&mut self, stamp: Stamp) {
        if !self.open {
            self.up = 0.0;
        }
        self.open = true;
        self.tab = Tab::Case;
        self.stamp = Some((stamp, 0.0));
        if !self.pinned {
            self.auto_until = Some(self.up + AUTO_SECONDS);
        }
    }

    /// Draws the folder. Only a pinned file takes clicks (tabs) and keys
    /// (Escape shuts it, arrows turn the tabs).
    pub fn show(&mut self, ctx: &egui::Context, cases: &MissionView, story: &StoryView) {
        if !self.open {
            return;
        }
        if self.pinned {
            let (esc, left, right) = ctx.input(|i| {
                (
                    i.key_pressed(egui::Key::Escape),
                    i.key_pressed(egui::Key::ArrowLeft),
                    i.key_pressed(egui::Key::ArrowRight),
                )
            });
            if esc {
                self.open = false;
                self.pinned = false;
                return;
            }
            let tabs = [Tab::Case, Tab::Doctor, Tab::Closed];
            let k = tabs.iter().position(|t| *t == self.tab).unwrap_or(0);
            if left {
                self.tab = tabs[(k + 2) % 3];
            }
            if right {
                self.tab = tabs[(k + 1) % 3];
            }
        }
        // The typewriter comes with the title screen's fonts; a game started
        // straight into the street gets them now (from the next frame).
        if family(ctx, "hw-type") == FontFamily::Proportional {
            crate::title::install_fonts(ctx);
        }
        let screen = ctx.screen_rect();
        // Slides up from below as it opens.
        let slide = 1.0 - ease_out_back((self.up / 0.35).min(1.0));
        let w = (screen.width() - 40.0).min(900.0);
        let h = (screen.height() - 120.0).min(600.0);
        let folder = Rect::from_center_size(
            screen.center() + vec2(0.0, 20.0 + slide * screen.height() * 0.6),
            vec2(w, h),
        );
        let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("case-file"));
        let painter = ctx.layer_painter(layer);
        let type_font = |size: f32| FontId::new(size, family(ctx, "hw-type"));
        let bold_font = |size: f32| FontId::new(size, family(ctx, "hw-logo"));

        // The folder: its back, the tabs along the top, its front edge.
        painter.rect_filled(
            folder.translate(vec2(6.0, 8.0)),
            10.0,
            Color32::from_black_alpha(90),
        );
        painter.rect_filled(folder, 10.0, MANILA);
        let names = [
            (Tab::Case, "OPEN CASE"),
            (Tab::Doctor, "THE DOCTOR"),
            (Tab::Closed, "CLOSED"),
        ];
        let tab_w = 150.0;
        for (k, (tab, name)) in names.iter().enumerate() {
            let x = folder.left() + 24.0 + k as f32 * (tab_w + 8.0);
            let r = Rect::from_min_size(pos2(x, folder.top() - 30.0), vec2(tab_w, 34.0));
            let on = self.tab == *tab;
            painter.rect_filled(r, 7.0, if on { MANILA } else { MANILA_DARK });
            painter.text(
                r.center() - vec2(0.0, 2.0),
                Align2::CENTER_CENTER,
                *name,
                bold_font(15.0),
                if on {
                    TYPE
                } else {
                    Color32::from_rgb(90, 70, 40)
                },
            );
            if self.pinned {
                let id = egui::Id::new(("case-file-tab", k));
                let hit = egui::Area::new(id)
                    .order(egui::Order::Tooltip)
                    .fixed_pos(r.min)
                    .show(ctx, |ui| {
                        ui.allocate_rect(Rect::from_min_size(r.min, r.size()), egui::Sense::click())
                    })
                    .inner;
                if hit.clicked() {
                    self.tab = *tab;
                }
            }
        }
        painter.text(
            pos2(folder.right() - 22.0, folder.top() + 16.0),
            Align2::RIGHT_TOP,
            "HERO HALL - CASE FILES",
            bold_font(14.0),
            Color32::from_rgb(110, 80, 40),
        );
        // The sheet.
        let sheet =
            Rect::from_min_max(folder.min + vec2(26.0, 44.0), folder.max - vec2(26.0, 34.0));
        painter.rect_filled(
            sheet.translate(vec2(3.0, 3.0)),
            2.0,
            Color32::from_black_alpha(40),
        );
        painter.rect_filled(sheet, 2.0, PAPER);
        // A red margin line, like a typist's ledger.
        painter.line_segment(
            [
                pos2(sheet.left() + 46.0, sheet.top()),
                pos2(sheet.left() + 46.0, sheet.bottom()),
            ],
            Stroke::new(1.0_f32, Color32::from_rgb(220, 150, 150)),
        );
        let mut pen = Pen {
            painter: &painter,
            at: pos2(sheet.left() + 62.0, sheet.top() + 20.0),
            left: sheet.left() + 62.0,
            width: sheet.width() * 0.62,
            font: type_font(16.0),
        };
        match self.tab {
            Tab::Case => self.case_sheet(&mut pen, sheet, cases, &type_font),
            Tab::Doctor => doctor_sheet(&mut pen, sheet, story, &type_font),
            Tab::Closed => self.closed_sheet(&mut pen, &type_font),
        }
        // The stamp coming down.
        if let (Tab::Case, Some((stamp, t))) = (self.tab, self.stamp) {
            let k = (t / STAMP_SECONDS).min(1.0);
            let scale = 1.0 + (1.0 - k) * 1.6;
            let alpha = (k * 1.4).min(1.0);
            let (text, at) = match stamp {
                Stamp::New => ("NEW CASE", pos2(sheet.right() - 170.0, sheet.top() + 70.0)),
                Stamp::Closed => ("CASE CLOSED", sheet.center() + vec2(40.0, 10.0)),
            };
            rubber_stamp(&painter, ctx, at, text, 34.0 * scale, -0.22, alpha);
        }
        // How to work it.
        let hint = if self.pinned {
            "J or Escape: close   Left/Right or click: turn the tabs"
        } else {
            "J: open the case file"
        };
        painter.text(
            pos2(folder.center().x, folder.bottom() - 16.0),
            Align2::CENTER_CENTER,
            hint,
            type_font(13.0),
            Color32::from_rgb(90, 66, 36),
        );
    }

    fn case_sheet(
        &self,
        pen: &mut Pen,
        sheet: Rect,
        cases: &MissionView,
        type_font: &dyn Fn(f32) -> FontId,
    ) {
        let painter = pen.painter;
        // A case just closed stays on the sheet under its stamp.
        let shown: Option<(State, &str)> = match (&self.just_closed, cases.state.as_ref()) {
            (Some(c), Some(s)) if s.stage == IDLE => {
                let mut st = s.clone();
                st.kind = c.kind;
                st.gang = c.gang;
                Some((st, c.building))
            }
            (_, Some(s)) if s.stage != IDLE => {
                Some((s.clone(), cases.building().unwrap_or("building")))
            }
            _ => None,
        };
        let Some((s, building)) = shown else {
            pen.line_font("NO OPEN CASE", type_font(24.0), TYPE);
            pen.gap(10.0);
            pen.para(&format!(
                "The cabinet is quiet. {CONTACT_NAME} keeps the cases on the steps of Hero Hall: \
                 stand beside her and hold E, and she'll hand you the next one."
            ));
            pen.gap(16.0);
            pen.para(
                "Bring a friend if you like. The crooks bring theirs: one more to a room for every \
                 hero who comes along.",
            );
            margin_note(
                painter,
                type_font,
                pos2(sheet.right() - 250.0, sheet.top() + 150.0),
                "Coffee's on me\nwhen you're ready! -A.",
            );
            return;
        };
        let number = if self.just_closed.is_some() && s.stage == IDLE {
            s.closed
        } else {
            s.closed + 1
        };
        let gang = &GANGS[s.gang.min(GANGS.len() - 1)];
        let flavour = &GANG_FLAVOUR[s.gang.min(GANG_FLAVOUR.len() - 1)];
        pen.line_font(&format!("CASE No. {number:04}"), type_font(26.0), TYPE);
        pen.gap(4.0);
        pen.field("FILED BY", CONTACT_NAME);
        pen.field("LOCATION", &format!("the {building}"));
        pen.field("SUSPECTS", &capitalised(gang.name));
        pen.field(
            "NATURE",
            match s.kind {
                CaseKind::Takeover => "Hostile takeover",
                CaseKind::Evidence => "Evidence wanted",
                CaseKind::Hostages => "Hostages held",
            },
        );
        pen.gap(10.0);
        pen.para(&briefing(s.kind, flavour, building));
        pen.gap(12.0);
        pen.line_font("PROGRESS", type_font(17.0), TYPE);
        pen.gap(2.0);
        let ticked = |b: bool| if b { "[X]" } else { "[ ]" };
        match s.stage {
            GO => pen.line(&format!("{} Get to the {building}", ticked(false))),
            _ => pen.line(&format!("{} Get to the {building}", ticked(true))),
        }
        if s.groups > 0 {
            pen.line(&format!(
                "{} Rooms cleared: {} of {}",
                ticked(s.cleared >= s.groups),
                s.cleared,
                s.groups
            ));
        }
        match s.kind {
            CaseKind::Takeover => {
                let bagged = s.boss == 2 || (self.just_closed.is_some() && s.stage == IDLE);
                pen.line(&format!(
                    "{} {} on the top floor: {}",
                    ticked(bagged),
                    flavour.boss,
                    if bagged { "BAGGED" } else { "at large" }
                ));
            }
            CaseKind::Evidence | CaseKind::Hostages => {
                let hostage = s.kind == CaseKind::Hostages;
                let n = if s.clues.is_empty() { 3 } else { s.clues.len() };
                for k in 0..n {
                    let state = s.clues.get(k).map(|c| c.1);
                    let done =
                        state == Some(DONE) || (self.just_closed.is_some() && s.stage == IDLE);
                    let what = match (hostage, done, state) {
                        (true, true, _) => "freed, and thankful".to_string(),
                        (true, false, Some(GUARDED)) => "held, under guard".to_string(),
                        (true, false, Some(_)) => "held, guards down: untie them!".to_string(),
                        (true, false, None) => "somewhere inside".to_string(),
                        (false, true, _) => {
                            CLUE_FINDS[(number as usize + k) % CLUE_FINDS.len()].to_string()
                        }
                        (false, false, _) => "not found".to_string(),
                    };
                    let label = if hostage { "Hostage" } else { "Clue" };
                    pen.line(&format!("{} {label} #{}: {what}", ticked(done), k + 1));
                }
            }
        }
        // The boss's mugshot, clipped on at the right.
        let card = Rect::from_min_size(
            pos2(sheet.right() - 200.0, sheet.top() + 26.0),
            vec2(150.0, 186.0),
        );
        mugshot(painter, card, flavour, type_font);
        // Annie's note in the margin, in her blue ink.
        let note = flavour.notes[(number as usize) % flavour.notes.len()];
        margin_note(
            painter,
            type_font,
            pos2(card.left() - 10.0, card.bottom() + 36.0),
            note,
        );
    }

    fn closed_sheet(&self, pen: &mut Pen, type_font: &dyn Fn(f32) -> FontId) {
        pen.line_font("CLOSED CASES", type_font(24.0), TYPE);
        pen.gap(8.0);
        if self.history.is_empty() {
            pen.para(&format!(
                "Nothing in the drawer yet. {CONTACT_NAME} says the first one is always the \
                 hardest. The second one too, mostly."
            ));
            return;
        }
        let painter = pen.painter;
        // Newest on top; as many as fit.
        for c in self.history.iter().rev().take(9) {
            let y = pen.at.y;
            pen.line(&format!(
                "No. {:04}   the {}   {}",
                c.number,
                c.building,
                match c.kind {
                    CaseKind::Takeover => "takeover",
                    CaseKind::Evidence => "evidence",
                    CaseKind::Hostages => "hostages",
                }
            ));
            pen.line_font(
                &format!(
                    "        {}",
                    capitalised(GANGS[c.gang.min(GANGS.len() - 1)].name)
                ),
                type_font(14.0),
                FADED,
            );
            let at = pos2(pen.left + pen.width + 90.0, y + 18.0);
            stamp_shape(painter, at, "CLOSED", type_font(18.0), -0.12, 0.85);
            pen.gap(6.0);
        }
    }
}

/// The Doctor's dossier: what is solved, what is open, the rest blacked out.
fn doctor_sheet(pen: &mut Pen, sheet: Rect, story: &StoryView, type_font: &dyn Fn(f32) -> FontId) {
    let painter = pen.painter;
    rubber_stamp(
        painter,
        painter.ctx(),
        pos2(sheet.right() - 150.0, sheet.top() + 60.0),
        "TOP SECRET",
        30.0,
        0.18,
        0.9,
    );
    pen.line_font("DOSSIER: \"THE DOCTOR\"", type_font(24.0), TYPE);
    pen.gap(2.0);
    pen.line_font(
        "Commander Starling's own file. Do not remove from Hero Hall.",
        type_font(14.0),
        FADED,
    );
    pen.gap(10.0);
    let done = story.state.map_or(0, |s| s.done as usize);
    let stage = story.state.map(|s| s.stage);
    let level = story.state.map_or(1, |s| s.level);
    for (k, c) in CHAPTERS.iter().enumerate() {
        let y = pen.at.y;
        if k < done {
            pen.line(&format!("Chapter {}: {}", k + 1, c.title));
            stamp_shape(
                painter,
                pos2(pen.left + pen.width + 70.0, y + 10.0),
                "SOLVED",
                type_font(17.0),
                -0.1 + 0.05 * (k % 3) as f32,
                0.9,
            );
        } else if k == done {
            pen.line_font(
                &format!("Chapter {}: {}", k + 1, c.title),
                type_font(17.0),
                TEAL,
            );
            let what = if stage == Some(LOCKED) || level < c.level {
                format!(
                    "        Opens at level {}. Starling says: train up.",
                    c.level
                )
            } else {
                format!("        {}", c.goal)
            };
            pen.line_font(&what, type_font(14.0), FADED);
        } else {
            // Classified: the title blacked out with a marker.
            let width = 120.0 + (c.title.len() as f32) * 7.0;
            let r = Rect::from_min_size(pos2(pen.left + 104.0, y + 3.0), vec2(width, 16.0));
            pen.line(&format!("Chapter {}:", k + 1));
            painter.rect_filled(r, 2.0, Color32::from_rgb(24, 22, 22));
        }
        pen.gap(5.0);
    }
    if done >= CHAPTERS.len() {
        rubber_stamp(
            painter,
            painter.ctx(),
            sheet.center() + vec2(60.0, 40.0),
            "FILE CLOSED",
            40.0,
            -0.2,
            1.0,
        );
    } else {
        pen.gap(8.0);
        pen.para("Everything past here is CLASSIFIED until you earn it.");
    }
}

/// Typing on the sheet, line by line.
struct Pen<'a> {
    painter: &'a egui::Painter,
    at: Pos2,
    left: f32,
    width: f32,
    font: FontId,
}

impl Pen<'_> {
    fn line(&mut self, text: &str) {
        let f = self.font.clone();
        self.line_font(text, f, TYPE);
    }

    fn line_font(&mut self, text: &str, font: FontId, color: Color32) {
        let g = self
            .painter
            .layout(text.to_string(), font, color, self.width + 120.0);
        let h = g.size().y;
        self.painter.galley(self.at, g, color);
        self.at.y += h + 2.0;
    }

    fn field(&mut self, name: &str, value: &str) {
        let f = self.font.clone();
        let g = self
            .painter
            .layout_no_wrap(format!("{name}:"), f.clone(), FADED);
        self.painter.galley(self.at, g, FADED);
        let g = self
            .painter
            .layout(value.to_string(), f, TYPE, self.width - 110.0);
        let h = g.size().y;
        self.painter.galley(self.at + vec2(110.0, 0.0), g, TYPE);
        self.at.y += h + 2.0;
    }

    fn para(&mut self, text: &str) {
        let g = self
            .painter
            .layout(text.to_string(), self.font.clone(), TYPE, self.width);
        let h = g.size().y;
        self.painter.galley(self.at, g, TYPE);
        self.at.y += h + 2.0;
        self.at.x = self.left;
    }

    fn gap(&mut self, h: f32) {
        self.at.y += h;
    }
}

/// A rubber stamp: red capitals in a double frame, turned, a little worn.
fn rubber_stamp(
    painter: &egui::Painter,
    ctx: &egui::Context,
    at: Pos2,
    text: &str,
    size: f32,
    angle: f32,
    alpha: f32,
) {
    stamp_shape(
        painter,
        at,
        text,
        FontId::new(size, family(ctx, "hw-logo")),
        angle,
        alpha,
    );
}

fn stamp_shape(
    painter: &egui::Painter,
    at: Pos2,
    text: &str,
    font: FontId,
    angle: f32,
    alpha: f32,
) {
    let red = STAMP_RED.gamma_multiply(alpha * 0.85);
    let g = painter.layout_no_wrap(text.to_string(), font, red);
    let size = g.size();
    let pad = vec2(10.0, 4.0);
    let half = size * 0.5 + pad;
    let rot = egui::emath::Rot2::from_angle(angle);
    let corners = |grow: f32| {
        let h = half + vec2(grow, grow);
        [
            vec2(-h.x, -h.y),
            vec2(h.x, -h.y),
            vec2(h.x, h.y),
            vec2(-h.x, h.y),
        ]
        .map(|v| at + rot * v)
        .to_vec()
    };
    painter.add(egui::Shape::closed_line(
        corners(0.0),
        Stroke::new(2.5_f32, red),
    ));
    painter.add(egui::Shape::closed_line(
        corners(4.0),
        Stroke::new(1.2_f32, red),
    ));
    // The text's top left, turned round the middle with the frame.
    let top_left = at + rot * (-size * 0.5);
    painter.add(TextShape::new(top_left, g, red).with_angle(angle));
}

/// Annie's note: blue ink, a little slanted.
fn margin_note(painter: &egui::Painter, type_font: &dyn Fn(f32) -> FontId, at: Pos2, text: &str) {
    let font = FontId::new(17.0, family(painter.ctx(), "hw-logo"));
    let _ = type_font;
    let g = painter.layout(text.to_string(), font, INK_BLUE, 240.0);
    painter.add(TextShape::new(at, g, INK_BLUE).with_angle(-0.06));
}

/// The boss's photo, paper-clipped on: a grey silhouette before a height
/// chart, their name typed under it.
fn mugshot(
    painter: &egui::Painter,
    card: Rect,
    f: &GangFlavour,
    type_font: &dyn Fn(f32) -> FontId,
) {
    painter.rect_filled(
        card.translate(vec2(3.0, 4.0)),
        2.0,
        Color32::from_black_alpha(50),
    );
    painter.rect_filled(card, 2.0, Color32::from_rgb(250, 248, 240));
    let photo = Rect::from_min_max(
        card.min + vec2(10.0, 10.0),
        pos2(card.max.x - 10.0, card.min.y + 136.0),
    );
    painter.rect_filled(photo, 0.0, Color32::from_rgb(96, 100, 104));
    // The height chart behind them.
    for k in 0..6 {
        let y = photo.top() + 12.0 + k as f32 * 20.0;
        painter.line_segment(
            [pos2(photo.left(), y), pos2(photo.right(), y)],
            Stroke::new(1.0_f32, Color32::from_rgb(130, 134, 138)),
        );
    }
    let c = f.tint;
    let base = pos2(photo.center().x, photo.bottom());
    // Shoulders and head.
    painter.add(egui::Shape::convex_polygon(
        vec![
            base + vec2(-48.0, 0.0),
            base + vec2(-42.0, -34.0),
            base + vec2(-14.0, -48.0),
            base + vec2(14.0, -48.0),
            base + vec2(42.0, -34.0),
            base + vec2(48.0, 0.0),
        ],
        c,
        Stroke::NONE,
    ));
    painter.circle_filled(base + vec2(0.0, -72.0), 24.0, c);
    // Their look: a hat, a mask's eyes, a helmet's crest.
    match f.mark {
        Mark::Hat => {
            painter.rect_filled(
                Rect::from_center_size(base + vec2(0.0, -92.0), vec2(64.0, 7.0)),
                2.0,
                Color32::from_rgb(30, 30, 34),
            );
            painter.rect_filled(
                Rect::from_center_size(base + vec2(0.0, -104.0), vec2(38.0, 22.0)),
                3.0,
                Color32::from_rgb(30, 30, 34),
            );
        }
        Mark::Visor => {
            painter.rect_filled(
                Rect::from_center_size(base + vec2(0.0, -74.0), vec2(40.0, 8.0)),
                3.0,
                Color32::from_rgb(255, 80, 60),
            );
        }
        Mark::Crest => {
            painter.rect_filled(
                Rect::from_center_size(base + vec2(0.0, -102.0), vec2(8.0, 22.0)),
                3.0,
                Color32::from_rgb(170, 40, 40),
            );
            painter.rect_filled(
                Rect::from_center_size(base + vec2(0.0, -74.0), vec2(34.0, 6.0)),
                1.0,
                Color32::from_rgb(30, 30, 30),
            );
        }
        Mark::Dome => {
            painter.circle_stroke(
                base + vec2(0.0, -76.0),
                34.0,
                Stroke::new(3.0_f32, Color32::from_rgba_unmultiplied(190, 255, 240, 160)),
            );
            painter.circle_filled(base + vec2(-8.0, -76.0), 4.0, Color32::from_rgb(20, 20, 20));
            painter.circle_filled(base + vec2(8.0, -76.0), 4.0, Color32::from_rgb(20, 20, 20));
        }
        Mark::Antenna => {
            painter.line_segment(
                [base + vec2(0.0, -96.0), base + vec2(0.0, -118.0)],
                Stroke::new(3.0_f32, Color32::from_rgb(60, 60, 64)),
            );
            painter.circle_filled(
                base + vec2(0.0, -120.0),
                5.0,
                Color32::from_rgb(255, 210, 60),
            );
        }
    }
    painter.text(
        pos2(card.center().x, card.bottom() - 36.0),
        Align2::CENTER_CENTER,
        "WANTED",
        type_font(15.0),
        STAMP_RED,
    );
    painter.text(
        pos2(card.center().x, card.bottom() - 17.0),
        Align2::CENTER_CENTER,
        f.boss,
        type_font(13.0),
        TYPE,
    );
    // The paper clip over its top edge.
    let clip = pos2(card.center().x + 30.0, card.top() - 8.0);
    let pts: Vec<Pos2> = [
        vec2(0.0, 30.0),
        vec2(0.0, 2.0),
        vec2(4.0, -4.0),
        vec2(10.0, -4.0),
        vec2(14.0, 2.0),
        vec2(14.0, 36.0),
        vec2(10.0, 40.0),
        vec2(6.0, 36.0),
        vec2(6.0, 8.0),
    ]
    .iter()
    .map(|v| clip + *v)
    .collect();
    painter.add(egui::Shape::line(
        pts,
        Stroke::new(2.2_f32, Color32::from_rgb(150, 156, 164)),
    ));
}

#[derive(Clone, Copy, Debug)]
enum Mark {
    Hat,
    Visor,
    Crest,
    Dome,
    Antenna,
}

/// What Hero Hall's files say about a gang.
pub struct GangFlavour {
    /// Their boss as the file names them.
    pub boss: &'static str,
    /// What they are known for (the briefing's middle).
    pub known: &'static str,
    /// Annie's notes in the margin, one per case.
    pub notes: &'static [&'static str],
    tint: Color32,
    mark: Mark,
}

/// By the gang's index in [`GANGS`].
pub const GANG_FLAVOUR: [GangFlavour; 5] = [
    GangFlavour {
        boss: "the Sentry Mk. II",
        known: "The Doctor's technicians never do anything without a reason, and the reason is \
                usually worse than the thing.",
        notes: &[
            "Mind the sparks,\nhon!",
            "Bring rubber\nboots.",
            "If it beeps,\nhit it.",
        ],
        tint: Color32::from_rgb(70, 74, 80),
        mark: Mark::Antenna,
    },
    GangFlavour {
        boss: "\"Knuckles\" the Alpha",
        known: "Chrome Jackets do the shoving and the Velvet Syndicate do the shooting. They \
                split the take sixty-forty and argue about it all the way home.",
        notes: &[
            "Knuckles hates\nbeing called\n'Knuckles'. Do it.",
            "Watch the stairs!",
            "Jackets talk.\nSyndicate shoots.",
        ],
        tint: Color32::from_rgb(52, 50, 56),
        mark: Mark::Hat,
    },
    GangFlavour {
        boss: "Cap'n Blackbarrel",
        known: "Sky pirates came down from their airship for loot that doesn't float away. \
                They board a building like a ship: floor by floor, cutlass first.",
        notes: &[
            "Arr. Sorry.\nCouldn't help it.",
            "They hate\nheights indoors.",
            "Duck the\ngunners!",
        ],
        tint: Color32::from_rgb(60, 52, 46),
        mark: Mark::Crest,
    },
    GangFlavour {
        boss: "the Praetorian",
        known: "The Iron-Masked Empire marches in step and claims every floor it stands on \
                \"for the Empire\". Nobody knows which empire. Nobody has asked twice.",
        notes: &[
            "Shields up\nmeans hit\nharder.",
            "They march\nin threes.",
            "Don't salute\nback!",
        ],
        tint: Color32::from_rgb(64, 64, 70),
        mark: Mark::Visor,
    },
    GangFlavour {
        boss: "the Overlord of Zorrak",
        known: "The Zorrak say they come in peace. They also say \"specimen\" a great deal, \
                and they've brought grabbers.",
        notes: &[
            "Don't let them\nprobe you,\nhon.",
            "Saucers park\non the roof?",
            "Take me to\nyour leader! Ha.",
        ],
        tint: Color32::from_rgb(70, 96, 82),
        mark: Mark::Dome,
    },
];

/// What a found clue turns out to be.
const CLUE_FINDS: [&str; 8] = [
    "a ledger in code",
    "a torn blueprint",
    "a matchbook, Orbit Room",
    "a coded telegram",
    "a key with a tag",
    "a photograph, faces scratched out",
    "a smudged map",
    "a receipt for 40 tons of rivets",
];

fn briefing(kind: CaseKind, f: &GangFlavour, building: &str) -> String {
    let job = match kind {
        CaseKind::Takeover => format!(
            "They have taken the {building} room by room. Their boss sits on the top floor: \
             bring them down and the rest will scatter."
        ),
        CaseKind::Evidence => format!(
            "They are holed up in the {building} and hiding something. Three clues, tucked \
             away on different floors. Hold E to search."
        ),
        CaseKind::Hostages => format!(
            "They are holding three people in the {building}, on different floors. Beat each \
             room's guards, then hold E to untie them."
        ),
    };
    format!("{} {job}", f.known)
}

pub fn capitalised(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// The named font family when it is installed, else the plain one.
fn family(ctx: &egui::Context, name: &str) -> FontFamily {
    let f = FontFamily::Name(name.into());
    if ctx.fonts(|fonts| fonts.families().contains(&f)) {
        f
    } else {
        FontFamily::Proportional
    }
}

fn ease_out_back(x: f32) -> f32 {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (x - 1.0).powi(3) + c1 * (x - 1.0).powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use hw_sim::missions::INSIDE;

    fn st(stage: u8, closed: u16) -> State {
        State {
            stage,
            closed,
            kind: CaseKind::Hostages,
            gang: 2,
            at: Vec3::ZERO,
            cleared: 0,
            groups: 6,
            boss: 1,
            clues: Vec::new(),
            held: 0.0,
        }
    }

    #[test]
    fn a_case_pops_the_file_and_a_closed_one_is_filed() {
        let mut file = CaseFile::default();
        let mut view = MissionView::default();
        view.state = Some(st(IDLE, 0));
        file.update(&view, 0.1);
        assert!(!file.open);
        view.state = Some(st(GO, 0));
        file.update(&view, 0.1);
        assert!(file.open && file.stamp.is_some_and(|s| s.0 == Stamp::New));
        // It goes again on its own.
        for _ in 0..100 {
            file.update(&view, 0.1);
        }
        assert!(!file.open);
        view.state = Some(st(INSIDE, 0));
        file.update(&view, 0.1);
        view.state = Some(st(IDLE, 1));
        file.update(&view, 0.1);
        assert!(file.open && file.stamp.is_some_and(|s| s.0 == Stamp::Closed));
        assert_eq!(file.history.len(), 1);
        assert_eq!(file.history[0].number, 1);
        assert_eq!(file.history[0].kind, CaseKind::Hostages);
        // Pinned with J, it stays.
        file.toggle();
        for _ in 0..100 {
            file.update(&view, 0.1);
        }
        assert!(file.open);
        file.toggle();
        assert!(!file.open);
    }

    #[test]
    fn every_gang_has_its_file() {
        assert_eq!(GANG_FLAVOUR.len(), GANGS.len());
        for f in &GANG_FLAVOUR {
            assert!(!f.notes.is_empty());
        }
    }
}
