//! Indoor cases on screen: every open case, one of them tracked (its
//! goal on the wrist radio, a teal mark on the compass and the map, a
//! teal light over its building while the heroes head there), sparkles
//! over the clues inside, Atomic Annie's words on the air (text only),
//! the prompt at her desk and the bar at a clue or a hostage, and the
//! cases closed. The rules are the server's (`hw_sim::missions`); this
//! only shows what it says. Nothing here assumes one case.

use crate::radioview::{beacon_lit, Radio, StoryMark};
use crate::storyview::{read_time, Line};
use engine::prelude::*;
use engine::ui::egui;
use glam::Vec3;
use hw_gen::urban::landmarks::LandmarkKind;
use hw_gen::urban::world::TownWorld;
use hw_net::ToClient;
use hw_sim::missions::{
    CaseKind, ASK_REACH, CLOSED, CLUES, CLUE_REACH, CONTACT_NAME, DONE, GANGS, GO, GUARDED,
    HOSTAGES, IDLE, INSIDE,
};
use std::collections::{BTreeMap, VecDeque};

/// The cases' colour on the radio, the compass and the map.
pub const CASE_TEAL: egui::Color32 = egui::Color32::from_rgb(90, 225, 210);

/// What the server last said of one case.
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub number: u16,
    pub stage: u8,
    pub kind: CaseKind,
    pub gang: usize,
    pub at: Vec3,
    pub cleared: u8,
    pub groups: u8,
    pub boss: u8,
    /// Clues or hostages: where, and guarded, ready or done.
    pub clues: Vec<(Vec3, u8)>,
    /// How far along holding the help key is (0-1).
    pub held: f32,
}

impl State {
    pub fn found(&self) -> usize {
        self.clues.iter().filter(|c| c.1 == DONE).count()
    }
}

/// Annie's desk, as the server last said.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Desk {
    pub at: Vec3,
    pub closed: u16,
    pub open: u8,
    pub limit: u8,
    pub can_take: bool,
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
    pub desk: Option<Desk>,
    /// The case the radio, the compass and the map follow.
    pub tracked: Option<u16>,
    /// Where each case plays, by name.
    buildings: BTreeMap<u16, &'static str>,
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
}

impl MissionView {
    /// A case or desk message; false for other messages.
    pub fn hear(&mut self, m: &ToClient) -> bool {
        match m {
            ToClient::CaseDesk {
                at,
                closed,
                open,
                limit,
                can_take,
            } => {
                self.desk = Some(Desk {
                    at: Vec3::from_array(*at),
                    closed: *closed,
                    open: *open,
                    limit: *limit,
                    can_take: *can_take,
                });
                true
            }
            ToClient::Case {
                number,
                stage,
                kind,
                gang,
                at,
                cleared,
                groups,
                boss,
                clues,
                held,
            } => {
                let next = State {
                    number: *number,
                    stage: *stage,
                    kind: CaseKind::from_code(*kind),
                    gang: (*gang as usize).min(GANGS.len() - 1),
                    at: Vec3::from_array(*at),
                    cleared: *cleared,
                    groups: *groups,
                    boss: *boss,
                    clues: clues
                        .iter()
                        .map(|(p, f)| (Vec3::from_array(*p), *f))
                        .collect(),
                    held: *held as f32 / 255.0,
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

    /// The building case `number` plays in, by name.
    pub fn building(&self, number: u16) -> &'static str {
        self.buildings.get(&number).copied().unwrap_or("building")
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
    pub fn update(&mut self, world: &mut World, town: &TownWorld, radio: &mut Radio, dt: f32) {
        for (before, now) in std::mem::take(&mut self.pending) {
            let n = now.number;
            if now.stage == GO || !self.buildings.contains_key(&n) {
                if let Some(b) = building_name(town, now.at) {
                    self.buildings.insert(n, b);
                }
            }
            let building = self.building(n);
            let said = lines(before.as_ref(), &now, building, self.cases.len());
            self.heard.extend(said.iter().cloned());
            self.lines.extend(said);
            match now.stage {
                CLOSED => {
                    let b = before.as_ref().unwrap_or(&now);
                    let filed = Filed {
                        number: n,
                        kind: b.kind,
                        gang: b.gang,
                        building: building.to_string(),
                    };
                    self.filed.push(filed.clone());
                    let over = self.filed.len().saturating_sub(CABINET_KEEP);
                    self.filed.drain(..over);
                    self.events.push(Event::Closed(filed));
                    self.buildings.remove(&n);
                }
                IDLE => {
                    self.buildings.remove(&n);
                }
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
            let mut m = mark(s, self.building(s.number))?;
            if self.cases.len() > 1 {
                m.heading = format!("{}   ({} open)", m.heading, self.cases.len());
            }
            Some(m)
        });
        // The light only outdoors, while heading there: inside it would
        // stand through the floors.
        let want = self.tracked().filter(|s| s.stage == GO).map(|s| s.at);
        if want != self.beam.map(|b| b.1) {
            if let Some((e, _)) = self.beam.take() {
                world.despawn(e);
            }
            if let Some(at) = want {
                let teal = rgb(90, 225, 210);
                if let Ok(e) = world.place(beacon_lit(teal), Place::new(at.x, at.y, at.z)) {
                    self.beam = Some((e, at));
                }
            }
        }
        self.sparkle -= dt;
        if self.sparkle <= 0.0 {
            self.sparkle = 0.45;
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
                            *at + Vec3::Y * 0.7,
                            rgb(120, 255, 235),
                            0.35,
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

    /// What to say at the hero's feet: Annie's desk, or a clue or hostage
    /// in reach (with the bar), from whichever case it belongs to.
    pub fn prompt(&self, hero: Vec3) -> Option<(String, Option<f32>)> {
        if let Some(d) = self
            .desk
            .filter(|d| hero.distance(d.at) < ASK_REACH as f32 + 2.5)
        {
            return Some(if d.can_take {
                (format!("Hold E: ask {CONTACT_NAME} for a case"), None)
            } else if d.open >= d.limit {
                (
                    format!(
                        "{CONTACT_NAME}: \"You've got {} cases on the go, hon. Close one first.\"",
                        d.open
                    ),
                    None,
                )
            } else {
                (
                    format!(
                        "{CONTACT_NAME}: \"Nothing new right now. Every building on my list is taken.\""
                    ),
                    None,
                )
            });
        }
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
                (CaseKind::Hostages, _) => ("Hold E to untie them".to_string(), Some(s.held)),
                _ => ("Hold E to search here".to_string(), Some(s.held)),
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
        self.desk = None;
        self.tracked = None;
        self.pending.clear();
        self.buildings.clear();
        self.lines.clear();
        self.shown = 0.0;
    }
}

/// The name of the building with a floor plan nearest `at`.
fn building_name(town: &TownWorld, at: Vec3) -> Option<&'static str> {
    let p = glam::DVec2::new(at.x as f64, at.z as f64);
    town.landmarks
        .iter()
        .filter(|l| l.kind == LandmarkKind::CometBuilding)
        .min_by(|a, b| a.pos.distance(p).total_cmp(&b.pos.distance(p)))
        .filter(|l| l.pos.distance(p) < 120.0)
        .map(hw_gen::urban::comet::name)
}

/// The case on the radio: its heading, the goal, and where to go.
pub fn mark(s: &State, building: &str) -> Option<StoryMark> {
    let heading = format!("CASE No. {} - the {building}   (J: case file)", s.number);
    let rooms = if s.groups > 0 {
        format!(" (rooms cleared {} of {})", s.cleared, s.groups)
    } else {
        String::new()
    };
    let goal = match (s.stage, s.kind) {
        (GO, _) => format!("Go to the {building}"),
        (INSIDE, CaseKind::Takeover) => format!("Beat their boss on the top floor{rooms}"),
        (INSIDE, CaseKind::Evidence) => {
            format!(
                "Find the clues: {} of {}{rooms}",
                s.found(),
                s.clues.len().max(CLUES)
            )
        }
        (INSIDE, CaseKind::Hostages) => {
            format!(
                "Free the hostages: {} of {}{rooms}",
                s.found(),
                s.clues.len().max(HOSTAGES)
            )
        }
        _ => return None,
    };
    Some(StoryMark {
        heading,
        goal,
        at: Some(s.at),
    })
}

/// What the radio says of a case going from `before` (None: new to us)
/// to `now`; `open` cases are left after it.
pub fn lines(before: Option<&State>, now: &State, building: &str, open: usize) -> Vec<Line> {
    let say = |text: String| Line {
        who: CONTACT_NAME,
        text,
    };
    let gang = GANGS[now.gang].name;
    let gang_cap = crate::casefile::capitalised(gang);
    let mut out = Vec::new();
    match now.stage {
        CLOSED => {
            out.push(say(format!(
                "The {building} is clear. Case closed, swell work!"
            )));
            out.push(say(match open {
                0 => "Come and see me at Hero Hall when you want the next one.".to_string(),
                1 => "One case still open: check your file (J).".to_string(),
                n => format!("{n} cases still open: check your file (J)."),
            }));
            return out;
        }
        IDLE => return out,
        _ => {}
    }
    let Some(b) = before else {
        // A new case: the briefing.
        match now.kind {
            CaseKind::Takeover => {
                out.push(say(format!(
                    "Trouble at the {building}: {gang} have taken the whole place over."
                )));
                out.push(say(
                    "Their boss sits on the top floor. Bring them down, and the rest will scatter."
                        .to_string(),
                ));
            }
            CaseKind::Evidence => {
                out.push(say(format!(
                    "{gang_cap} are holed up in the {building}, and they're hiding something.",
                )));
                out.push(say(
                    "Find three clues for me. They'll be tucked away in the rooms, upstairs too."
                        .to_string(),
                ));
            }
            CaseKind::Hostages => {
                out.push(say(format!(
                    "{gang_cap} have taken the {building}, and they're holding people inside.",
                )));
                out.push(say(
                    "Three of them, on different floors. Beat their guards, then untie them."
                        .to_string(),
                ));
            }
        }
        return out;
    };
    match (b.stage, now.stage) {
        (GO, INSIDE) => out.push(say(format!(
            "That's the {building}. Each room has its own crew: take them one at a time."
        ))),
        (INSIDE, GO) => out.push(say(format!(
            "They've regrouped in the {building} while you were away. Go back when you're ready."
        ))),
        (INSIDE, INSIDE) if now.found() > b.found() => {
            let left = now.clues.len() - now.found();
            out.push(say(match (now.kind, left) {
                (CaseKind::Hostages, 0) => "Everyone's out safe!".to_string(),
                (CaseKind::Hostages, _) => format!("They're free! {left} more held inside."),
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

    fn st(stage: u8, kind: CaseKind, number: u16, found: usize) -> State {
        State {
            number,
            stage,
            kind,
            gang: 2,
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
            held: 0.0,
        }
    }

    #[test]
    fn the_radio_follows_a_case() {
        let b = "Daily Comet building";
        let brief = lines(None, &st(GO, CaseKind::Evidence, 1, 0), b, 1);
        assert!(brief[0].text.contains(b) && brief[0].text.to_lowercase().contains(GANGS[2].name));
        let one = lines(
            Some(&st(INSIDE, CaseKind::Evidence, 1, 0)),
            &st(INSIDE, CaseKind::Evidence, 1, 1),
            b,
            1,
        );
        assert!(one[0].text.contains("2 more"));
        let closed = lines(
            Some(&st(INSIDE, CaseKind::Takeover, 1, 0)),
            &st(CLOSED, CaseKind::Takeover, 1, 0),
            b,
            2,
        );
        assert!(closed[0].text.contains("closed") && closed[1].text.contains("2 cases"));
        let m = mark(&st(INSIDE, CaseKind::Evidence, 4, 1), b).expect("a mark");
        assert!(
            m.goal.contains("1 of 3") && m.goal.contains("1 of 6") && m.heading.contains("No. 4"),
            "{} / {}",
            m.heading,
            m.goal
        );
        assert!(mark(&st(CLOSED, CaseKind::Evidence, 1, 0), b).is_none());
        let freed = lines(
            Some(&st(INSIDE, CaseKind::Hostages, 1, 1)),
            &st(INSIDE, CaseKind::Hostages, 1, 2),
            b,
            1,
        );
        assert!(freed[0].text.contains("1 more"), "{}", freed[0].text);
        let m = mark(&st(INSIDE, CaseKind::Hostages, 1, 2), b).expect("a mark");
        assert!(m.goal.contains("hostages: 2 of 3"), "{}", m.goal);
        for l in brief.iter().chain(&one).chain(&closed).chain(&freed) {
            assert!(l.text.len() <= 110, "too long for the radio: {}", l.text);
        }
    }

    fn case_msg(number: u16, stage: u8) -> ToClient {
        ToClient::Case {
            number,
            stage,
            kind: 2,
            gang: 1,
            at: [number as f32, 0.0, 0.0],
            cleared: 0,
            groups: 6,
            boss: 1,
            clues: Vec::new(),
            held: 0,
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
