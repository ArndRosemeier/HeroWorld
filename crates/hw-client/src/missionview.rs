//! Cases on screen: every open case, one of them tracked (its goal on
//! the wrist radio, a teal mark on the compass and the map, a teal light
//! over its place, or its way down, while the heroes head there),
//! sparkles over the clues, Police HQ's words on the air, the bar at a
//! clue or a hostage, the police computer's board of crimes, and the
//! cases closed. A case is a chain of steps, each a job at a place; the
//! words come from the place's kind and name as the server sends them.
//! The rules are the server's (`hw_sim::missions`); this only shows what
//! it says. Nothing here assumes one case, or one kind of place.

use crate::radioview::{beacon_lit, Radio, StoryMark};
use crate::storyview::{read_time, Line};
use engine::prelude::*;
use engine::ui::egui;
use glam::Vec3;
use hw_gen::urban::world::TownWorld;
use hw_net::ToClient;
use hw_sim::missions::board::CrimeKind;
use hw_sim::missions::Place;
use hw_sim::missions::{
    CaseKind, CLOSED, CLUES, CLUE_REACH, DONE, GANGS, GO, GUARDED, HOSTAGES, IDLE, INSIDE,
};

/// Who speaks on the wrist radio.
pub const DISPATCH: &str = "Police HQ";
use std::collections::{BTreeMap, VecDeque};

/// The cases' colour on the radio, the compass and the map.
pub const CASE_TEAL: egui::Color32 = egui::Color32::from_rgb(90, 225, 210);

/// One step of a case: a job at a place.
#[derive(Clone, Debug, PartialEq)]
pub struct StepView {
    pub place: Place,
    pub job: CaseKind,
    pub at: Vec3,
    /// Reads after [`Place::preposition`]: "the crystal grotto".
    pub name: String,
}

impl StepView {
    /// "on Juniper Avenue", "in the crystal grotto".
    pub fn at_place(&self) -> String {
        format!("{} {}", self.place.preposition(), self.name)
    }
}

/// What the server last said of one case.
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub number: u16,
    pub stage: u8,
    /// The job of the step it is at.
    pub kind: CaseKind,
    pub gang: usize,
    /// The step it is at, of its steps.
    pub step: usize,
    pub steps: Vec<StepView>,
    pub at: Vec3,
    pub cleared: u8,
    pub groups: u8,
    pub boss: u8,
    /// Clues or hostages: where, and guarded, ready or done.
    pub clues: Vec<(Vec3, u8)>,
    /// How far along holding the help key is (0-1).
    pub held: f32,
    /// What the gang is up to, and the gangsters' level.
    pub crime: CrimeKind,
    pub level: u8,
    /// How big the hideout is (0 small, 1 middling, 2 big) and the alarms
    /// its gang has raised.
    pub size: u8,
    pub alarms: u8,
}

impl State {
    /// The thing is taken and must be got out of the building.
    pub fn carrying(&self) -> bool {
        self.stage == INSIDE
            && self.kind == CaseKind::Evidence
            && self.in_building()
            && !self.clues.is_empty()
            && self.found() == self.clues.len()
    }

    pub fn found(&self) -> usize {
        self.clues.iter().filter(|c| c.1 == DONE).count()
    }

    /// The step it is at.
    pub fn here(&self) -> Option<&StepView> {
        self.steps.get(self.step)
    }

    /// Where the step it is at plays ("the Daily Comet building").
    pub fn place_name(&self) -> String {
        self.here()
            .map_or_else(|| "the scene".to_string(), |s| s.name.clone())
    }

    pub fn in_building(&self) -> bool {
        self.here().is_some_and(|s| s.place == Place::Building)
    }
}

/// One crime on the police computer.
#[derive(Clone, Debug, PartialEq)]
pub struct CrimeView {
    pub number: u16,
    pub crime: CrimeKind,
    pub job: CaseKind,
    pub gang: usize,
    pub level: u8,
    /// 0 small, 1 middling, 2 big.
    pub size: u8,
    /// A hot tip, and the minutes it stays on the board (255: taken).
    pub hot: bool,
    pub minutes: u8,
    pub at: Vec3,
    pub name: String,
    pub taken: bool,
}

/// The police computer's board, as the server last said.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Board {
    pub crimes: Vec<CrimeView>,
    pub limit: u8,
    pub open: u8,
    /// Crimes that went untaken so far, and whether a board came yet.
    pub missed: u16,
    pub heard: bool,
}

/// A case that was closed, for the cabinet.
#[derive(Clone, Debug, PartialEq)]
pub struct Filed {
    pub number: u16,
    pub kind: CaseKind,
    pub gang: usize,
    pub building: String,
}

/// The most closed cases the cabinet keeps (the oldest go first).
pub const CABINET_KEEP: usize = 500;

impl Filed {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "number": self.number,
            "kind": self.kind.code(),
            "gang": self.gang,
            "building": self.building,
        })
    }

    pub fn from_json(v: &serde_json::Value) -> Option<Self> {
        Some(Self {
            number: u16::try_from(v["number"].as_u64()?).ok()?,
            kind: CaseKind::from_code(u8::try_from(v["kind"].as_u64()?).ok()?),
            gang: (v["gang"].as_u64()? as usize).min(GANGS.len().saturating_sub(1)),
            building: v["building"].as_str()?.chars().take(60).collect(),
        })
    }
}

/// Something for the case file to stamp.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Opened(u16),
    Closed(Filed),
}

#[derive(Default)]
pub struct MissionView {
    /// The open cases, by number.
    pub cases: BTreeMap<u16, State>,
    pub board: Board,
    /// The case the radio, the compass and the map follow.
    pub tracked: Option<u16>,
    /// Cases closed, oldest first.
    pub filed: Vec<Filed>,
    /// News for the case file, taken by it.
    pub events: Vec<Event>,
    /// News not yet put into words (the words need the town).
    pending: Vec<(Option<State>, State)>,
    /// Lines still to show, and how long the first has been up (s).
    pub lines: VecDeque<Line>,
    /// Lines said since the game last took them, for the chat log.
    pub heard: Vec<Line>,
    shown: f32,
    /// The light over the tracked case's building, and where it stands.
    beam: Option<(EntityId, Vec3)>,
    /// Seconds to the next sparkle over the clues.
    sparkle: f32,
    /// The things to find, standing in the rooms: by case and clue.
    things: BTreeMap<(u16, usize), EntityId>,
    /// Counts the glints, to vary where they appear.
    sparkle_n: u32,
}

/// The mesh of the thing a gang hides (see `interior::pieces::evidence`).
fn thing_mesh(kind: CrimeKind) -> Mesh {
    let mut b = crate::citymesh::MeshBuilder::new();
    let mut m = crate::interior::pieces::Maker::new(
        &mut b,
        glam::DVec3::ZERO,
        glam::DVec3::Z,
        24,
        crate::skins::Wear::Fresh,
    );
    crate::interior::pieces::evidence(&mut m, kind);
    b.finish()
}

impl MissionView {
    /// A case or desk message; false for other messages.
    pub fn hear(&mut self, m: &ToClient) -> bool {
        match m {
            ToClient::CaseBoard {
                crimes,
                limit,
                open,
                missed,
                lost,
            } => {
                // A crime nobody took (not on first hearing of the board).
                if self.board.heard && *missed != self.board.missed && !lost.is_empty() {
                    let line = Line {
                        who: DISPATCH,
                        text: format!(
                            "Too late for the {lost}: the gang got away with it. The streets round there will be rougher for a while."
                        ),
                    };
                    self.heard.push(line.clone());
                    self.lines.push_back(line);
                }
                self.board = Board {
                    heard: true,
                    missed: *missed,
                    crimes: crimes
                        .iter()
                        .map(|c| CrimeView {
                            number: c.number,
                            crime: CrimeKind::from_code(c.crime),
                            job: CaseKind::from_code(c.job),
                            gang: (c.gang as usize).min(GANGS.len() - 1),
                            level: c.level,
                            size: c.size,
                            hot: c.hot,
                            minutes: c.minutes,
                            at: Vec3::from_array(c.at),
                            name: c.name.clone(),
                            taken: c.taken,
                        })
                        .collect(),
                    limit: *limit,
                    open: *open,
                };
                true
            }
            ToClient::Case {
                number,
                stage,
                gang,
                step,
                steps,
                at,
                cleared,
                groups,
                boss,
                clues,
                held,
                crime,
                level,
                size,
                alarms,
            } => {
                let steps: Vec<StepView> = steps
                    .iter()
                    .map(|c| StepView {
                        place: Place::from_code(c.place),
                        job: CaseKind::from_code(c.job),
                        at: Vec3::from_array(c.at),
                        name: c.name.clone(),
                    })
                    .collect();
                let step = (*step as usize).min(steps.len().saturating_sub(1));
                let next = State {
                    number: *number,
                    stage: *stage,
                    kind: steps.get(step).map_or(CaseKind::Takeover, |s| s.job),
                    gang: (*gang as usize).min(GANGS.len() - 1),
                    step,
                    steps,
                    at: Vec3::from_array(*at),
                    cleared: *cleared,
                    groups: *groups,
                    boss: *boss,
                    clues: clues
                        .iter()
                        .map(|(p, f)| (Vec3::from_array(*p), *f))
                        .collect(),
                    held: *held as f32 / 255.0,
                    crime: CrimeKind::from_code(*crime),
                    level: *level,
                    size: *size,
                    alarms: *alarms,
                };
                let before = if matches!(next.stage, CLOSED | IDLE) {
                    self.cases.remove(number)
                } else {
                    self.cases.insert(*number, next.clone())
                };
                self.pending.push((before, next));
                true
            }
            _ => false,
        }
    }

    /// Where case `number` plays now, by name.
    pub fn place_name(&self, number: u16) -> String {
        self.cases
            .get(&number)
            .map_or_else(|| "the scene".to_string(), |s| s.place_name())
    }

    /// The tracked case.
    pub fn tracked(&self) -> Option<&State> {
        self.tracked.and_then(|n| self.cases.get(&n))
    }

    /// Follow case `number` on the radio, the compass and the map.
    pub fn track(&mut self, number: u16) {
        if self.cases.contains_key(&number) {
            self.tracked = Some(number);
        }
    }

    /// Every frame: the words for the news, the light over the tracked
    /// building, the sparkles over the clues, the line on the air.
    pub fn update(&mut self, world: &mut World, _town: &TownWorld, radio: &mut Radio, dt: f32) {
        for (before, now) in std::mem::take(&mut self.pending) {
            let n = now.number;
            let said = lines(before.as_ref(), &now, self.cases.len());
            self.heard.extend(said.iter().cloned());
            self.lines.extend(said);
            match now.stage {
                CLOSED => {
                    let b = before.as_ref().unwrap_or(&now);
                    let filed = Filed {
                        number: n,
                        kind: b.kind,
                        gang: b.gang,
                        building: b.place_name(),
                    };
                    self.filed.push(filed.clone());
                    let over = self.filed.len().saturating_sub(CABINET_KEEP);
                    self.filed.drain(..over);
                    self.events.push(Event::Closed(filed));
                }
                IDLE => {}
                _ if before.is_none() => {
                    // A new case is the one to follow.
                    self.tracked = Some(n);
                    self.events.push(Event::Opened(n));
                }
                _ => {}
            }
        }
        if self.tracked.is_none_or(|t| !self.cases.contains_key(&t)) {
            self.tracked = self.cases.keys().next_back().copied();
        }
        radio.case = self.tracked().and_then(|s| {
            let mut m = mark(s)?;
            if self.cases.len() > 1 {
                m.heading = format!("{}   ({} open)", m.heading, self.cases.len());
            }
            Some(m)
        });
        // The light only while heading there: inside it would stand
        // through the floors.
        // Under the ground only over the way down, never up through the
        // street from below.
        let want = self
            .tracked()
            .filter(|s| s.stage == GO)
            .filter(|s| {
                s.here()
                    .is_none_or(|h| !h.place.underground() || h.at.distance(s.at) > 1.0)
            })
            .map(|s| s.at);
        if want != self.beam.map(|b| b.1) {
            if let Some((e, _)) = self.beam.take() {
                world.despawn(e);
            }
            if let Some(at) = want {
                let teal = rgb(90, 225, 210);
                if let Ok(e) = world.place(
                    beacon_lit(teal),
                    engine::prelude::Place::new(at.x, at.y, at.z),
                ) {
                    self.beam = Some((e, at));
                }
            }
        }
        // The things to find: real objects on the floor, until found.
        let wanted: Vec<((u16, usize), Vec3, CrimeKind)> = self
            .cases
            .values()
            .filter(|s| s.stage == INSIDE && s.kind == CaseKind::Evidence)
            .flat_map(|s| {
                s.clues
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.1 != DONE)
                    .map(move |(i, c)| ((s.number, i), c.0, s.crime))
            })
            .collect();
        let gone: Vec<(u16, usize)> = self
            .things
            .keys()
            .filter(|k| !wanted.iter().any(|w| w.0 == **k))
            .copied()
            .collect();
        for k in gone {
            if let Some(e) = self.things.remove(&k) {
                world.despawn(e);
            }
        }
        for (key, at, crime) in wanted {
            if self.things.contains_key(&key) {
                continue;
            }
            let yaw = (key.0 as f32 * 71.0 + key.1 as f32 * 40.0) % 360.0;
            if let Ok(e) = world.place(
                thing_mesh(crime),
                engine::prelude::Place::new(at.x, at.y + 0.02, at.z)
                    .with_yaw_deg(yaw)
                    .with_scale(1.7),
            ) {
                self.things.insert(key, e);
            }
        }
        self.sparkle -= dt;
        if self.sparkle <= 0.0 {
            self.sparkle = 0.45;
            self.sparkle_n = self.sparkle_n.wrapping_add(1);
            // Clues sparkle; hostages are there to see.
            for s in self
                .cases
                .values()
                .filter(|s| s.stage == INSIDE && s.kind == CaseKind::Evidence)
            {
                for (at, state) in &s.clues {
                    if *state != DONE {
                        crate::fightview::glow_at(
                            world,
                            // Small glints on the object itself, not a
                            // bubble round it: it must read as the thing.
                            *at + Vec3::new(
                                (self.sparkle_n % 5) as f32 * 0.12 - 0.24,
                                0.25 + (self.sparkle_n % 3) as f32 * 0.18,
                                0.0,
                            ),
                            rgb(255, 236, 170),
                            0.07,
                        );
                    }
                }
            }
        }
        if let Some(l) = self.lines.front() {
            self.shown += dt;
            if self.shown >= read_time(&l.text) {
                self.lines.pop_front();
                self.shown = 0.0;
            }
        }
    }

    /// What to say at the hero's feet: a clue or hostage in reach (with
    /// the bar), from whichever case it belongs to.
    pub fn prompt(&self, hero: Vec3) -> Option<(String, Option<f32>)> {
        self.cases
            .values()
            .filter(|s| s.stage == INSIDE)
            .flat_map(|s| s.clues.iter().map(move |c| (s, c)))
            .filter(|(_, (at, state))| {
                *state != DONE && hero.distance(*at) < CLUE_REACH as f32 + 1.5
            })
            .min_by(|a, b| hero.distance(a.1 .0).total_cmp(&hero.distance(b.1 .0)))
            .map(|(s, (_, state))| match (s.kind, *state) {
                (CaseKind::Hostages, GUARDED) => ("Beat their guards first".to_string(), None),
                (_, GUARDED) => (
                    format!("Beat the crew guarding {} first", s.crime.thing()),
                    None,
                ),
                (CaseKind::Hostages, _) => ("Hold E to untie them".to_string(), Some(s.held)),
                _ => (format!("Hold E to take {}", s.crime.thing()), Some(s.held)),
            })
    }

    /// The line on the air (under the story's) and the prompt.
    pub fn panel(&self, ctx: &egui::Context, hero: Option<Vec3>) {
        if let Some(l) = self.lines.front() {
            egui::Area::new(egui::Id::new("case-line"))
                // Words only: the pointer over it must not take the keys.
                .interactable(false)
                .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 140.0))
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgba_unmultiplied(10, 26, 26, 225))
                        .stroke(egui::Stroke::new(1.5_f32, CASE_TEAL))
                        .corner_radius(6.0)
                        .inner_margin(egui::Margin::symmetric(14, 8))
                        .show(ui, |ui| {
                            ui.set_max_width(620.0);
                            ui.label(
                                egui::RichText::new(format!("WRIST RADIO - {}", l.who))
                                    .color(CASE_TEAL)
                                    .size(12.0)
                                    .strong(),
                            );
                            ui.label(
                                egui::RichText::new(&l.text)
                                    .color(egui::Color32::from_rgb(220, 255, 250))
                                    .size(17.0),
                            );
                        });
                });
        }
        let Some((text, bar)) = hero.and_then(|h| self.prompt(h)) else {
            return;
        };
        egui::Area::new(egui::Id::new("case-prompt"))
            // Words only: the pointer over it must not take the keys.
            .interactable(false)
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -250.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_black_alpha(180))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.set_max_width(560.0);
                        ui.label(
                            egui::RichText::new(text)
                                .color(CASE_TEAL)
                                .size(17.0)
                                .strong(),
                        );
                        if let Some(b) = bar {
                            ui.add(
                                egui::ProgressBar::new(b)
                                    .desired_width(260.0)
                                    .fill(CASE_TEAL),
                            );
                        }
                    });
            });
    }

    /// A new town: the server's cases go (the cabinet stays: it is the
    /// hero's).
    pub fn clear(&mut self, world: &mut World) {
        if let Some((e, _)) = self.beam.take() {
            world.despawn(e);
        }
        self.cases.clear();
        self.board = Board::default();
        for (_, e) in std::mem::take(&mut self.things) {
            world.despawn(e);
        }
        self.tracked = None;
        self.pending.clear();
        self.lines.clear();
        self.shown = 0.0;
    }
}

/// What a step asks, in a few words for the radio's goal line.
fn goal(s: &State) -> String {
    let groups = if s.groups > 0 {
        if s.in_building() {
            format!(" (rooms cleared {} of {})", s.cleared, s.groups)
        } else {
            format!(" (crews beaten {} of {})", s.cleared, s.groups)
        }
    } else {
        String::new()
    };
    match s.kind {
        _ if s.carrying() => format!("Get out with {}!", s.crime.thing()),
        CaseKind::Takeover if s.boss == 3 => {
            format!("{} is making a run for it: after them!", boss_name(s.gang))
        }
        CaseKind::Takeover if s.in_building() => {
            format!(
                "Arrest {} on the top floor{groups}",
                lower_the(boss_name(s.gang))
            )
        }
        CaseKind::Takeover => format!("Beat their boss{groups}"),
        CaseKind::Crew => format!("Beat the crew: {} of {}", s.cleared, s.groups.max(1)),
        CaseKind::Evidence if s.clues.len() <= 1 && CLUES == 1 => {
            format!("Find {}{groups}", s.crime.thing())
        }
        CaseKind::Evidence => format!(
            "Find the clues: {} of {}{groups}",
            s.found(),
            s.clues.len().max(CLUES)
        ),
        CaseKind::Hostages => format!(
            "Free the hostages: {} of {}{groups}",
            s.found(),
            s.clues.len().max(HOSTAGES)
        ),
    }
}

/// The gang's boss as the files name them, to start a sentence ("The
/// Praetorian", "Cap'n Blackbarrel").
fn boss_name(gang: usize) -> String {
    let f = &crate::casefile::GANG_FLAVOUR[gang.min(crate::casefile::GANG_FLAVOUR.len() - 1)];
    crate::casefile::capitalised(f.boss)
}

/// A name inside a sentence: "The Praetorian" reads "the Praetorian".
fn lower_the(name: String) -> String {
    match name.strip_prefix("The ") {
        Some(rest) => format!("the {rest}"),
        None => name,
    }
}

/// The case on the radio: its heading, the goal, and where to go.
pub fn mark(s: &State) -> Option<StoryMark> {
    let here = s.here()?;
    let steps = if s.steps.len() > 1 {
        format!(" - step {} of {}", s.step + 1, s.steps.len())
    } else {
        String::new()
    };
    let heading = format!(
        "CASE No. {} - {}{steps}   (J: case file)",
        s.number,
        crate::casefile::capitalised(&here.name)
    );
    let goal = match s.stage {
        GO if here.place.underground() => format!("Go down to {}", here.name),
        GO => format!("Go to {}", here.name),
        INSIDE => goal(s),
        _ => return None,
    };
    Some(StoryMark {
        heading,
        goal,
        at: Some(s.at),
    })
}

/// Annie's words for a step starting: where, and what to do there.
fn brief(gang: &str, step: &StepView, last: bool, crime: CrimeKind) -> Vec<String> {
    let at = step.at_place();
    let gang_cap = crate::casefile::capitalised(gang);
    let building = step.place == Place::Building;
    match step.job {
        CaseKind::Crew => vec![
            format!("A crew of {gang} hangs about {at}."),
            "Shake them down and they'll talk.".to_string(),
        ],
        CaseKind::Evidence if !last => vec![
            format!("{gang_cap} have been busy {at}."),
            "Search the place: whatever they left behind will tell us more.".to_string(),
        ],
        CaseKind::Takeover if building => vec![
            format!("{}: {gang} are holed up {at}.", crime.title()),
            "Arrest their boss upstairs. The rest will scatter.".to_string(),
        ],
        CaseKind::Takeover => vec![
            format!("{gang_cap} have dug in {at}, boss and all."),
            "Bring the boss down, and the rest will scatter.".to_string(),
        ],
        CaseKind::Evidence if building => vec![
            format!("{}: {gang} are holed up {at}.", crime.title()),
            format!(
                "Find {} before they move it. Look upstairs too.",
                crime.thing()
            ),
        ],
        CaseKind::Evidence => vec![
            format!("{gang_cap} are hiding something {at}."),
            "Find three clues for me. Hold E to search.".to_string(),
        ],
        CaseKind::Hostages if building => vec![
            format!(
                "{gang_cap} have taken {}, and they're holding people inside.",
                step.name
            ),
            "Three of them, on different floors. Beat their guards, then untie them.".to_string(),
        ],
        CaseKind::Hostages => vec![
            format!("{gang_cap} are holding people {at}."),
            "Beat their guards, then untie them.".to_string(),
        ],
    }
}

/// What the radio says of a case going from `before` (None: new to us)
/// to `now`; `open` cases are left after it.
pub fn lines(before: Option<&State>, now: &State, open: usize) -> Vec<Line> {
    let say = |text: String| Line {
        who: DISPATCH,
        text,
    };
    let gang = GANGS[now.gang].name;
    let mut out = Vec::new();
    let place = now.place_name();
    match now.stage {
        CLOSED => {
            out.push(say(format!(
                "{} is clear. Case closed, swell work!",
                crate::casefile::capitalised(&place)
            )));
            if now.in_building() && now.steps.len() == 1 {
                out.push(say(match now.kind {
                    CaseKind::Evidence => format!(
                        "The rest have given up: the police drones are taking them in. Somebody's outside to thank you for {}.",
                        now.crime.thing()
                    ),
                    _ => "The rest have given up without their boss: the police drones are taking them in."
                        .to_string(),
                }));
            }
            out.push(say(match open {
                0 => "More crimes are on the police computer when you want one.".to_string(),
                1 => "One case still open: check your file (J).".to_string(),
                n => format!("{n} cases still open: check your file (J)."),
            }));
            return out;
        }
        IDLE => return out,
        _ => {}
    }
    let Some(here) = now.here() else {
        return out;
    };
    let last = now.step + 1 >= now.steps.len();
    let Some(b) = before else {
        // A new case: the briefing.
        if now.steps.len() > 1 {
            out.push(say(format!(
                "Something's brewing with {gang}. Let's pull the thread."
            )));
        }
        out.extend(brief(gang, here, last, now.crime).into_iter().map(say));
        return out;
    };
    if now.step > b.step {
        // A lead done: it points to the next place.
        let was = &b.kind;
        out.push(say(match was {
            CaseKind::Crew => format!("They talked! The trail leads {}.", here.at_place()),
            _ => format!("That's what we needed. It points {}.", here.at_place()),
        }));
        out.extend(
            brief(gang, here, last, now.crime)
                .into_iter()
                .skip(1)
                .map(say),
        );
        return out;
    }
    match (b.stage, now.stage) {
        (GO, INSIDE) if now.in_building() => out.push(say(format!(
            "That's {place}. Lookouts at the door, a crew in each room: take a room down fast, or it calls the next."
        ))),
        (GO, INSIDE) if here.place.underground() => out.push(say(
            "You're down there. Watch yourself, sound carries in the dark.".to_string(),
        )),
        (GO, INSIDE) => out.push(say("There they are. Go get 'em!".to_string())),
        (INSIDE, GO) => out.push(say(format!(
            "They've regrouped {} while you were away. Go back when you're ready.",
            here.at_place()
        ))),
        (INSIDE, INSIDE) if now.carrying() && !b.carrying() => {
            out.push(say(format!(
                "You've got {}! They know: the whole gang is coming, and a crew is at the door. Get out!",
                now.crime.thing()
            )));
        }
        (INSIDE, INSIDE) if now.boss == 3 && b.boss != 3 => out.push(say(format!(
            "{} is making a run for it and yelling for help! Don't let them get their breath back.",
            boss_name(now.gang)
        ))),
        (INSIDE, INSIDE) if now.alarms != b.alarms => out.push(say(
            "They've raised the alarm: the next room is coming. Finish them fast next time!"
                .to_string(),
        )),
        (INSIDE, INSIDE) if now.found() > b.found() => {
            let left = now.clues.len() - now.found();
            out.push(say(match (now.kind, left) {
                (CaseKind::Hostages, 0) => "Everyone's out safe!".to_string(),
                (CaseKind::Hostages, _) => format!("They're free! {left} more held."),
                (_, 0) => "That's all of them!".to_string(),
                _ => format!("Got one! {left} more to find."),
            }));
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(place: Place, job: CaseKind, name: &str) -> StepView {
        StepView {
            place,
            job,
            at: Vec3::ZERO,
            name: name.into(),
        }
    }

    fn st(stage: u8, kind: CaseKind, number: u16, found: usize) -> State {
        State {
            number,
            stage,
            kind,
            gang: 2,
            step: 0,
            steps: vec![step(Place::Building, kind, "the Daily Comet building")],
            at: Vec3::ZERO,
            cleared: 1,
            groups: 6,
            boss: 1,
            clues: (0..3)
                .map(|k| {
                    (
                        Vec3::ZERO,
                        if k < found {
                            DONE
                        } else {
                            hw_sim::missions::READY
                        },
                    )
                })
                .collect(),
            size: 0,
            alarms: 0,
            held: 0.0,
            crime: CrimeKind::Payroll,
            level: 5,
        }
    }

    #[test]
    fn the_radio_follows_a_case() {
        let b = "the Daily Comet building";
        let brief = lines(None, &st(GO, CaseKind::Evidence, 1, 0), 1);
        assert!(brief[0].text.contains(b) && brief[0].text.to_lowercase().contains(GANGS[2].name));
        let one = lines(
            Some(&st(INSIDE, CaseKind::Evidence, 1, 0)),
            &st(INSIDE, CaseKind::Evidence, 1, 1),
            1,
        );
        assert!(one[0].text.contains("2 more"));
        let closed = lines(
            Some(&st(INSIDE, CaseKind::Takeover, 1, 0)),
            &st(CLOSED, CaseKind::Takeover, 1, 0),
            2,
        );
        assert!(closed[0].text.contains("closed"));
        // The ending of a hideout, then what is left to do.
        assert!(closed[1].text.contains("given up"), "{}", closed[1].text);
        assert!(closed.last().unwrap().text.contains("2 cases"));
        let m = mark(&st(INSIDE, CaseKind::Evidence, 4, 1)).expect("a mark");
        assert!(
            m.goal.contains("1 of 3") && m.goal.contains("1 of 6") && m.heading.contains("No. 4"),
            "{} / {}",
            m.heading,
            m.goal
        );
        assert!(mark(&st(CLOSED, CaseKind::Evidence, 1, 0)).is_none());
        let freed = lines(
            Some(&st(INSIDE, CaseKind::Hostages, 1, 1)),
            &st(INSIDE, CaseKind::Hostages, 1, 2),
            1,
        );
        assert!(freed[0].text.contains("1 more"), "{}", freed[0].text);
        let m = mark(&st(INSIDE, CaseKind::Hostages, 1, 2)).expect("a mark");
        assert!(m.goal.contains("hostages: 2 of 3"), "{}", m.goal);
        for l in brief.iter().chain(&one).chain(&closed).chain(&freed) {
            assert!(l.text.len() <= 110, "too long for the radio: {}", l.text);
        }
    }

    #[test]
    fn a_chain_leads_from_place_to_place() {
        let mut a = st(GO, CaseKind::Crew, 3, 0);
        a.steps = vec![
            step(Place::Street, CaseKind::Crew, "Juniper Avenue"),
            step(Place::Station, CaseKind::Hostages, "Orbit Square station"),
        ];
        let brief = lines(None, &a, 1);
        assert!(brief.iter().any(|l| l.text.contains("on Juniper Avenue")));
        let m = mark(&a).expect("a mark");
        assert!(m.heading.contains("step 1 of 2"), "{}", m.heading);
        assert_eq!(m.goal, "Go to Juniper Avenue");
        let mut b = a.clone();
        b.stage = INSIDE;
        let mut c = b.clone();
        c.step = 1;
        c.stage = GO;
        c.kind = CaseKind::Hostages;
        let next = lines(Some(&b), &c, 1);
        assert!(
            next[0].text.contains("talked") && next[0].text.contains("at Orbit Square station"),
            "{}",
            next[0].text
        );
        assert_eq!(
            mark(&c).expect("a mark").goal,
            "Go down to Orbit Square station"
        );
        for l in brief.iter().chain(&next) {
            assert!(l.text.len() <= 110, "too long for the radio: {}", l.text);
        }
    }

    fn case_msg(number: u16, stage: u8) -> ToClient {
        ToClient::Case {
            number,
            stage,
            gang: 1,
            step: 0,
            steps: vec![hw_net::CaseStep {
                place: 0,
                job: 2,
                at: [number as f32, 0.0, 0.0],
                name: "the Daily Comet building".into(),
            }],
            at: [number as f32, 0.0, 0.0],
            cleared: 0,
            groups: 6,
            boss: 1,
            clues: Vec::new(),
            held: 0,
            crime: 0,
            level: 3,
            size: 0,
            alarms: 0,
        }
    }
    #[test]
    fn several_cases_are_kept_apart_and_the_newest_is_tracked() {
        let mut v = MissionView::default();
        v.hear(&case_msg(1, GO));
        v.hear(&case_msg(2, GO));
        assert_eq!(v.cases.len(), 2);
        // Pending news goes into words in `update`; without a world, by hand.
        for (before, now) in std::mem::take(&mut v.pending) {
            if before.is_none() && now.stage == GO {
                v.tracked = Some(now.number);
            }
        }
        assert_eq!(v.tracked, Some(2));
        v.track(1);
        assert_eq!(v.tracked().map(|s| s.number), Some(1));
        v.hear(&case_msg(1, CLOSED));
        assert_eq!(v.cases.len(), 1);
        assert!(v.tracked().is_none(), "the closed case is not followed");
    }
}
