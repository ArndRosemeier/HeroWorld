//! The case file (J): Hero Hall's manila folder of the hero's cases. Every
//! open case has its own divider: typed up on a sheet (where, who, what,
//! Annie's notes in the margin, a mugshot of the gang's boss clipped on,
//! a checklist ticked off as the rooms fall), and the one the radio
//! follows is stamped TRACKED (click another's "track" line, or Enter,
//! to follow that one). Then the Doctor's dossier (chapters solved
//! stamped, the ones to come blacked out) and the cabinet of closed
//! cases, with a tally per gang, scrolled with the wheel. A new case
//! slides it in with a NEW CASE stamp, a closed one with CASE CLOSED.
//! Nothing assumes one case. It only shows what the server said
//! (`missionview`, `storyview`).

use crate::missionview::{Event, Filed, MissionView, State};
use crate::storyview::StoryView;
use egui::epaint::TextShape;
use egui::{pos2, vec2, Align2, Color32, FontFamily, FontId, Pos2, Rect, Stroke};
use engine::ui::egui;
use hw_sim::missions::{CaseKind, CONTACT_NAME, DONE, GANGS, GO, GUARDED};
use hw_sim::story::{CHAPTERS, LOCKED};

/// Seconds the file stays up when a case opens or closes on its own.
const AUTO_SECONDS: f32 = 7.0;
/// Seconds a stamp takes to come down.
const STAMP_SECONDS: f32 = 0.35;
/// Closed cases one look at the cabinet shows (the wheel scrolls).
const CABINET_ROWS: usize = 8;

const MANILA: Color32 = Color32::from_rgb(214, 178, 112);
const MANILA_DARK: Color32 = Color32::from_rgb(170, 132, 74);
const PAPER: Color32 = Color32::from_rgb(246, 239, 220);
const DIVIDER: Color32 = Color32::from_rgb(232, 222, 196);
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
    /// The open case on the sheet (by number); else the tracked one.
    pub selected: Option<u16>,
    /// Seconds it has been up.
    up: f32,
    /// Opened on its own: closes again at this many seconds up.
    auto_until: Option<f32>,
    stamp: Option<(Stamp, f32)>,
    /// The case just closed, shown under its stamp.
    just_closed: Option<Filed>,
    /// Closed cases scrolled past in the cabinet.
    scroll: usize,
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
            self.stamp = None;
            self.just_closed = None;
        }
    }

    /// Open with J: the pointer is let go for the tabs.
    pub fn taking_pointer(&self) -> bool {
        self.open && self.pinned
    }

    /// Every frame: news of the cases (one opened, one closed) and time.
    pub fn update(&mut self, cases: &mut MissionView, dt: f32) {
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
        for e in std::mem::take(&mut cases.events) {
            match e {
                Event::Opened(n) => {
                    self.selected = Some(n);
                    self.just_closed = None;
                    self.pop(Stamp::New);
                }
                Event::Closed(f) => {
                    self.just_closed = Some(f);
                    self.pop(Stamp::Closed);
                }
            }
        }
        if self.selected.is_some_and(|n| !cases.cases.contains_key(&n)) {
            self.selected = None;
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

    /// The open case on the sheet.
    fn shown(&self, cases: &MissionView) -> Option<u16> {
        self.selected
            .filter(|n| cases.cases.contains_key(n))
            .or(cases.tracked)
            .or_else(|| cases.cases.keys().next().copied())
    }

    /// Draws the folder. Only a pinned file takes clicks and keys: Escape
    /// shuts it, Left/Right turn the tabs, Up/Down go through the open
    /// cases, Enter tracks the one shown, the wheel scrolls the cabinet.
    pub fn show(&mut self, ctx: &egui::Context, cases: &mut MissionView, story: &StoryView) {
        if !self.open {
            return;
        }
        let shown = self.shown(cases);
        if self.pinned {
            let (esc, left, right, up, down, enter, wheel) = ctx.input(|i| {
                (
                    i.key_pressed(egui::Key::Escape),
                    i.key_pressed(egui::Key::ArrowLeft),
                    i.key_pressed(egui::Key::ArrowRight),
                    i.key_pressed(egui::Key::ArrowUp),
                    i.key_pressed(egui::Key::ArrowDown),
                    i.key_pressed(egui::Key::Enter),
                    i.raw_scroll_delta.y,
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
            let numbers: Vec<u16> = cases.cases.keys().copied().collect();
            if let (Some(n), true) = (shown, up || down) {
                let at = numbers.iter().position(|x| *x == n).unwrap_or(0);
                let next = if up {
                    (at + numbers.len() - 1) % numbers.len()
                } else {
                    (at + 1) % numbers.len()
                };
                self.selected = Some(numbers[next]);
                self.just_closed = None;
            }
            if let (Some(n), true, Tab::Case) = (shown, enter, self.tab) {
                cases.track(n);
            }
            if self.tab == Tab::Closed && wheel != 0.0 {
                let most = cases.filed.len().saturating_sub(CABINET_ROWS);
                self.scroll = if wheel > 0.0 {
                    self.scroll.saturating_sub(1)
                } else {
                    (self.scroll + 1).min(most)
                };
            }
        }
        let shown = self.shown(cases);
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
        let click = |id: egui::Id, r: Rect| -> bool {
            egui::Area::new(id)
                .order(egui::Order::Tooltip)
                .fixed_pos(r.min)
                .show(ctx, |ui| {
                    ui.allocate_rect(Rect::from_min_size(r.min, r.size()), egui::Sense::click())
                })
                .inner
                .clicked()
        };

        // The folder: its back, the tabs along the top, its front edge.
        painter.rect_filled(
            folder.translate(vec2(6.0, 8.0)),
            10.0,
            Color32::from_black_alpha(90),
        );
        painter.rect_filled(folder, 10.0, MANILA);
        let open_count = cases.cases.len();
        let names = [
            (
                Tab::Case,
                if open_count > 1 {
                    format!("OPEN CASES ({open_count})")
                } else {
                    "OPEN CASE".to_string()
                },
            ),
            (Tab::Doctor, "THE DOCTOR".to_string()),
            (Tab::Closed, format!("CLOSED ({})", cases.filed.len())),
        ];
        let tab_w = 170.0;
        for (k, (tab, name)) in names.iter().enumerate() {
            let x = folder.left() + 24.0 + k as f32 * (tab_w + 8.0);
            let r = Rect::from_min_size(pos2(x, folder.top() - 30.0), vec2(tab_w, 34.0));
            let on = self.tab == *tab;
            painter.rect_filled(r, 7.0, if on { MANILA } else { MANILA_DARK });
            painter.text(
                r.center() - vec2(0.0, 2.0),
                Align2::CENTER_CENTER,
                name,
                bold_font(15.0),
                if on {
                    TYPE
                } else {
                    Color32::from_rgb(90, 70, 40)
                },
            );
            if self.pinned && click(egui::Id::new(("case-file-tab", k)), r) {
                self.tab = *tab;
            }
        }
        painter.text(
            pos2(folder.right() - 22.0, folder.top() + 16.0),
            Align2::RIGHT_TOP,
            "HERO HALL - CASE FILES",
            bold_font(14.0),
            Color32::from_rgb(110, 80, 40),
        );
        // The sheet; on the case tab, dividers for the open cases above it.
        let dividers = self.tab == Tab::Case && open_count > 1 && self.just_closed.is_none();
        let top = if dividers { 72.0 } else { 44.0 };
        let sheet = Rect::from_min_max(folder.min + vec2(26.0, top), folder.max - vec2(26.0, 34.0));
        if dividers {
            let dw = ((sheet.width() - 10.0) / open_count as f32).min(200.0);
            for (k, (n, s)) in cases.cases.iter().enumerate() {
                let r = Rect::from_min_size(
                    pos2(sheet.left() + k as f32 * (dw + 4.0), sheet.top() - 30.0),
                    vec2(dw, 34.0),
                );
                let on = shown == Some(*n);
                painter.rect_filled(r, 4.0, if on { PAPER } else { DIVIDER });
                let tracked = cases.tracked == Some(*n);
                let label = format!(
                    "{}No. {:04}  {}",
                    if tracked { "* " } else { "" },
                    n,
                    short(cases.building(*n))
                );
                painter.text(
                    r.left_center() + vec2(8.0, -3.0),
                    Align2::LEFT_CENTER,
                    label,
                    type_font(13.0),
                    if on { TYPE } else { FADED },
                );
                let _ = s;
                if self.pinned && click(egui::Id::new(("case-file-case", *n)), r) {
                    self.selected = Some(*n);
                }
            }
        }
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
            Tab::Case => match (&self.just_closed, shown.and_then(|n| cases.cases.get(&n))) {
                // A case just closed stays on the sheet under its stamp.
                (Some(f), _) => closed_sheet_for(&mut pen, sheet, f, &type_font),
                (None, Some(s)) => {
                    let tracked = cases.tracked == Some(s.number);
                    let track_line = case_sheet(
                        &mut pen,
                        sheet,
                        s,
                        cases.building(s.number),
                        tracked,
                        &type_font,
                    );
                    if tracked {
                        stamp_shape(
                            &painter,
                            pos2(sheet.left() + 300.0, sheet.top() + 34.0),
                            "TRACKED",
                            type_font(18.0),
                            0.1,
                            0.75,
                        );
                    } else if let Some(r) = track_line {
                        if self.pinned && click(egui::Id::new(("case-file-track", s.number)), r) {
                            cases.track(s.number);
                        }
                    }
                }
                (None, None) => empty_sheet(&mut pen, sheet, cases, &type_font),
            },
            Tab::Doctor => doctor_sheet(&mut pen, sheet, story, &type_font),
            Tab::Closed => cabinet(&mut pen, sheet, cases, self.scroll, &type_font),
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
        let hint = match (self.pinned, self.tab, open_count > 1) {
            (false, _, _) => "J: open the case file",
            (true, Tab::Case, true) => {
                "J or Escape: close   Left/Right: tabs   Up/Down: cases   Enter: track"
            }
            (true, Tab::Closed, _) => "J or Escape: close   Left/Right: tabs   Wheel: scroll",
            _ => "J or Escape: close   Left/Right or click: turn the tabs",
        };
        painter.text(
            pos2(folder.center().x, folder.bottom() - 16.0),
            Align2::CENTER_CENTER,
            hint,
            type_font(13.0),
            Color32::from_rgb(90, 66, 36),
        );
    }
}

/// A building's name without "the ... building" round it, for a divider.
fn short(building: &str) -> &str {
    building
        .trim_end_matches(" building")
        .trim_end_matches(" house")
        .trim_end_matches(" hall")
}

/// No open case: where to get one.
fn empty_sheet(pen: &mut Pen, sheet: Rect, cases: &MissionView, type_font: &dyn Fn(f32) -> FontId) {
    pen.line_font("NO OPEN CASE", type_font(24.0), TYPE);
    pen.gap(10.0);
    pen.para(&format!(
        "The cabinet is quiet. {CONTACT_NAME} keeps the cases on the steps of Hero Hall: \
         stand beside her and hold E, and she'll hand you the next one."
    ));
    pen.gap(16.0);
    if let Some(d) = cases.desk {
        pen.para(&format!(
            "She lets a team work up to {} cases at once, one to a building.",
            d.limit
        ));
        pen.gap(10.0);
    }
    pen.para(
        "Bring a friend if you like. The crooks bring theirs: one more to a room for every \
         hero who comes along.",
    );
    margin_note(
        pen.painter,
        pos2(sheet.right() - 250.0, sheet.top() + 150.0),
        "Coffee's on me\nwhen you're ready! -A.",
    );
}

/// An open case typed up. The "track" line's place when it is not the
/// tracked one (a click there tracks it).
fn case_sheet(
    pen: &mut Pen,
    sheet: Rect,
    s: &State,
    building: &str,
    tracked: bool,
    type_font: &dyn Fn(f32) -> FontId,
) -> Option<Rect> {
    let painter = pen.painter;
    let number = s.number;
    let gang = &GANGS[s.gang.min(GANGS.len() - 1)];
    let flavour = &GANG_FLAVOUR[s.gang.min(GANG_FLAVOUR.len() - 1)];
    pen.line_font(&format!("CASE No. {number:04}"), type_font(26.0), TYPE);
    pen.gap(4.0);
    pen.field("FILED BY", CONTACT_NAME);
    pen.field("LOCATION", &format!("the {building}"));
    pen.field("SUSPECTS", &capitalised(gang.name));
    pen.field("NATURE", nature(s.kind));
    pen.gap(10.0);
    pen.para(&briefing(s.kind, flavour, building));
    pen.gap(12.0);
    pen.line_font("PROGRESS", type_font(17.0), TYPE);
    pen.gap(2.0);
    let ticked = |b: bool| if b { "[X]" } else { "[ ]" };
    pen.line(&format!("{} Get to the {building}", ticked(s.stage != GO)));
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
            let bagged = s.boss == 2;
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
                let done = state == Some(DONE);
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
    let track = (!tracked).then(|| {
        pen.gap(10.0);
        let at = pen.at;
        pen.line_font(
            "> Follow this case on the radio (click, or Enter)",
            type_font(15.0),
            TEAL,
        );
        Rect::from_min_max(at, pos2(at.x + 420.0, pen.at.y))
    });
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
        pos2(card.left() - 10.0, card.bottom() + 36.0),
        note,
    );
    track
}

/// A case just closed, under its CASE CLOSED stamp.
fn closed_sheet_for(pen: &mut Pen, sheet: Rect, f: &Filed, type_font: &dyn Fn(f32) -> FontId) {
    let gang = &GANGS[f.gang.min(GANGS.len() - 1)];
    let flavour = &GANG_FLAVOUR[f.gang.min(GANG_FLAVOUR.len() - 1)];
    pen.line_font(&format!("CASE No. {:04}", f.number), type_font(26.0), TYPE);
    pen.gap(4.0);
    pen.field("FILED BY", CONTACT_NAME);
    pen.field("LOCATION", &format!("the {}", f.building));
    pen.field("SUSPECTS", &capitalised(gang.name));
    pen.field("NATURE", nature(f.kind));
    pen.gap(12.0);
    pen.line(&match f.kind {
        CaseKind::Takeover => format!("[X] {} BAGGED", flavour.boss),
        CaseKind::Evidence => "[X] All three clues found".to_string(),
        CaseKind::Hostages => "[X] Everyone out safe".to_string(),
    });
    pen.line("[X] Karma paid out to the heroes on the scene");
    let card = Rect::from_min_size(
        pos2(sheet.right() - 200.0, sheet.top() + 26.0),
        vec2(150.0, 186.0),
    );
    mugshot(pen.painter, card, flavour, type_font);
}

/// The cabinet: a tally per gang, then the closed cases, newest first.
fn cabinet(
    pen: &mut Pen,
    sheet: Rect,
    cases: &MissionView,
    scroll: usize,
    type_font: &dyn Fn(f32) -> FontId,
) {
    pen.line_font("CLOSED CASES", type_font(24.0), TYPE);
    pen.gap(4.0);
    if cases.filed.is_empty() {
        pen.gap(4.0);
        pen.para(&format!(
            "Nothing in the drawer yet. {CONTACT_NAME} says the first one is always the \
             hardest. The second one too, mostly."
        ));
        return;
    }
    // The tally: cases closed against each gang.
    let tally: Vec<String> = (0..GANGS.len())
        .filter_map(|g| {
            let n = cases.filed.iter().filter(|f| f.gang == g).count();
            (n > 0).then(|| format!("{} {n}", short_gang(g)))
        })
        .collect();
    pen.line_font(
        &format!("Tally: {}", tally.join("   ")),
        type_font(14.0),
        FADED,
    );
    pen.gap(8.0);
    let painter = pen.painter;
    let total = cases.filed.len();
    let scroll = scroll.min(total.saturating_sub(CABINET_ROWS));
    for f in cases.filed.iter().rev().skip(scroll).take(CABINET_ROWS) {
        let y = pen.at.y;
        pen.line(&format!(
            "No. {:04}   the {}   {}",
            f.number,
            f.building,
            nature(f.kind).to_lowercase()
        ));
        pen.line_font(
            &format!(
                "        {}",
                capitalised(GANGS[f.gang.min(GANGS.len() - 1)].name)
            ),
            type_font(14.0),
            FADED,
        );
        let at = pos2(pen.left + pen.width + 90.0, y + 18.0);
        stamp_shape(painter, at, "CLOSED", type_font(18.0), -0.12, 0.85);
        pen.gap(4.0);
    }
    if total > CABINET_ROWS {
        painter.text(
            pos2(sheet.right() - 24.0, sheet.bottom() - 16.0),
            Align2::RIGHT_BOTTOM,
            format!(
                "{}-{} of {total}",
                scroll + 1,
                (scroll + CABINET_ROWS).min(total)
            ),
            type_font(13.0),
            FADED,
        );
    }
}

fn nature(kind: CaseKind) -> &'static str {
    match kind {
        CaseKind::Takeover => "Hostile takeover",
        CaseKind::Evidence => "Evidence wanted",
        CaseKind::Hostages => "Hostages held",
    }
}

/// A gang's short name for the tally.
fn short_gang(g: usize) -> &'static str {
    [
        "Doctor's men",
        "Jackets & Syndicate",
        "Sky pirates",
        "Iron Empire",
        "Zorrak",
    ]
    .get(g)
    .copied()
    .unwrap_or("Others")
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
fn margin_note(painter: &egui::Painter, at: Pos2, text: &str) {
    let font = FontId::new(17.0, family(painter.ctx(), "hw-logo"));
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

    fn filed(number: u16) -> Filed {
        Filed {
            number,
            kind: CaseKind::Hostages,
            gang: 2,
            building: "Daily Comet building".into(),
        }
    }

    #[test]
    fn cases_pop_the_file_and_closed_ones_are_stamped() {
        let mut file = CaseFile::default();
        let mut view = MissionView::default();
        file.update(&mut view, 0.1);
        assert!(!file.open);
        view.events.push(Event::Opened(1));
        file.update(&mut view, 0.1);
        assert!(file.open && file.stamp.is_some_and(|s| s.0 == Stamp::New));
        assert!(view.events.is_empty(), "the file takes its news");
        // It goes again on its own.
        for _ in 0..100 {
            file.update(&mut view, 0.1);
        }
        assert!(!file.open);
        view.events.push(Event::Closed(filed(1)));
        file.update(&mut view, 0.1);
        assert!(file.open && file.stamp.is_some_and(|s| s.0 == Stamp::Closed));
        assert_eq!(file.just_closed.as_ref().map(|f| f.number), Some(1));
        // Pinned with J, it stays, and shows the open cases again.
        file.toggle();
        assert!(file.just_closed.is_none());
        for _ in 0..100 {
            file.update(&mut view, 0.1);
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
