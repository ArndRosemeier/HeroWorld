//! Indoor cases on screen: the case and its goal on the wrist radio, a
//! teal mark on the compass and the map, a teal light over the building
//! while the heroes head there, sparkles over the clues inside, Atomic
//! Annie's words on the air (text only), the prompt to ask her for a case
//! and the search bar at a clue. The rules are the server's
//! (`hw_sim::missions`); this only shows what it says.

use crate::radioview::{beacon_lit, Radio, StoryMark};
use crate::storyview::{read_time, Line};
use engine::prelude::*;
use engine::ui::egui;
use glam::Vec3;
use hw_gen::urban::landmarks::LandmarkKind;
use hw_gen::urban::world::TownWorld;
use hw_net::ToClient;
use hw_sim::missions::{
    CaseKind, ASK_REACH, CLUES, CLUE_REACH, CONTACT_NAME, GANGS, GO, IDLE, INSIDE,
};
use std::collections::VecDeque;

/// The cases' colour on the radio, the compass and the map.
pub const CASE_TEAL: egui::Color32 = egui::Color32::from_rgb(90, 225, 210);

/// What the server last said.
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub stage: u8,
    pub closed: u16,
    pub kind: CaseKind,
    pub gang: usize,
    pub at: Vec3,
    pub cleared: u8,
    pub groups: u8,
    pub boss: u8,
    pub clues: Vec<(Vec3, bool)>,
    /// How far along holding the help key is (0-1).
    pub held: f32,
}

impl State {
    fn found(&self) -> usize {
        self.clues.iter().filter(|c| c.1).count()
    }
}

#[derive(Default)]
pub struct MissionView {
    pub state: Option<State>,
    /// News not yet put into words (the words need the town).
    pending: Vec<(Option<State>, State)>,
    /// Where the case plays: the building's name.
    building: Option<&'static str>,
    /// Lines still to show, and how long the first has been up (s).
    pub lines: VecDeque<Line>,
    /// Lines said since the game last took them, for the chat log.
    pub heard: Vec<Line>,
    shown: f32,
    /// The light over the building, and where it stands.
    beam: Option<(EntityId, Vec3)>,
    /// Seconds to the next sparkle over the clues.
    sparkle: f32,
}

impl MissionView {
    /// A case message; false for other messages.
    pub fn hear(&mut self, m: &ToClient) -> bool {
        let ToClient::Case {
            stage,
            closed,
            kind,
            gang,
            at,
            cleared,
            groups,
            boss,
            clues,
            held,
        } = m
        else {
            return false;
        };
        let next = State {
            stage: *stage,
            closed: *closed,
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
        self.pending.push((self.state.clone(), next.clone()));
        self.state = Some(next);
        true
    }

    /// Every frame: the words for the news, the light over the building,
    /// the sparkles over the clues, the line on the air.
    pub fn update(&mut self, world: &mut World, town: &TownWorld, radio: &mut Radio, dt: f32) {
        for (before, now) in std::mem::take(&mut self.pending) {
            if now.stage != IDLE {
                self.building = building_name(town, now.at).or(self.building);
            }
            let said = lines(before.as_ref(), &now, self.building.unwrap_or("building"));
            self.heard.extend(said.iter().cloned());
            self.lines.extend(said);
        }
        radio.case = self
            .state
            .as_ref()
            .and_then(|s| mark(s, self.building.unwrap_or("building")));
        // The light only outdoors, while heading there: inside it would
        // stand through the floors.
        let want = self.state.as_ref().filter(|s| s.stage == GO).map(|s| s.at);
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
            if let Some(s) = self.state.as_ref().filter(|s| s.stage == INSIDE) {
                for (at, found) in &s.clues {
                    if !found {
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

    /// The line on the air (under the story's), the prompt at the contact
    /// and the search bar at a clue.
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
        let (Some(s), Some(h)) = (self.state.as_ref(), hero) else {
            return;
        };
        let prompt = match s.stage {
            IDLE if h.distance(s.at) < ASK_REACH as f32 + 2.5 => {
                Some((format!("Hold E: ask {CONTACT_NAME} for a case"), None))
            }
            INSIDE => s
                .clues
                .iter()
                .find(|(at, found)| !found && h.distance(*at) < CLUE_REACH as f32 + 1.5)
                .map(|_| ("Hold E to search here".to_string(), Some(s.held))),
            _ => None,
        };
        let Some((text, bar)) = prompt else {
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

    pub fn clear(&mut self, world: &mut World) {
        if let Some((e, _)) = self.beam.take() {
            world.despawn(e);
        }
        self.state = None;
        self.pending.clear();
        self.building = None;
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
    let heading = format!("CASE - the {building}");
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
        _ => return None,
    };
    Some(StoryMark {
        heading,
        goal,
        at: Some(s.at),
    })
}

/// What the radio says going from `before` to `now`.
pub fn lines(before: Option<&State>, now: &State, building: &str) -> Vec<Line> {
    let say = |text: String| Line {
        who: CONTACT_NAME,
        text,
    };
    let gang = GANGS[now.gang].name;
    let mut out = Vec::new();
    let Some(b) = before else {
        return out;
    };
    if now.closed > b.closed {
        out.push(say(
            "Case closed! Swell work. Come and see me at Hero Hall when you want the next one."
                .to_string(),
        ));
        return out;
    }
    match (b.stage, now.stage) {
        (IDLE, GO) => match now.kind {
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
                    "{gang} are holed up in the {building}, and they're hiding something.",
                )));
                out.push(say(
                    "Find three clues for me. They'll be tucked away in the rooms, upstairs too."
                        .to_string(),
                ));
            }
        },
        (GO, INSIDE) => out.push(say(
            "That's the place. Each room has its own crew: take them one at a time.".to_string(),
        )),
        (INSIDE, GO) => out.push(say(
            "They've regrouped while you were away. Go back when you're ready.".to_string(),
        )),
        (INSIDE, INSIDE) if now.found() > b.found() => {
            let left = now.clues.len() - now.found();
            out.push(say(if left == 0 {
                "That's all of them!".to_string()
            } else {
                format!("Got one! {left} more to find.")
            }));
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(stage: u8, kind: CaseKind, closed: u16, found: usize) -> State {
        State {
            stage,
            closed,
            kind,
            gang: 2,
            at: Vec3::ZERO,
            cleared: 1,
            groups: 6,
            boss: 1,
            clues: (0..3).map(|k| (Vec3::ZERO, k < found)).collect(),
            held: 0.0,
        }
    }

    #[test]
    fn the_radio_follows_a_case() {
        let b = "Daily Comet building";
        let brief = lines(
            Some(&st(IDLE, CaseKind::Evidence, 0, 0)),
            &st(GO, CaseKind::Evidence, 0, 0),
            b,
        );
        assert!(brief[0].text.contains(b) && brief[0].text.contains(GANGS[2].name));
        let one = lines(
            Some(&st(INSIDE, CaseKind::Evidence, 0, 0)),
            &st(INSIDE, CaseKind::Evidence, 0, 1),
            b,
        );
        assert!(one[0].text.contains("2 more"));
        let closed = lines(
            Some(&st(INSIDE, CaseKind::Takeover, 0, 0)),
            &st(IDLE, CaseKind::Takeover, 1, 0),
            b,
        );
        assert!(closed[0].text.contains("closed"));
        let m = mark(&st(INSIDE, CaseKind::Evidence, 0, 1), b).expect("a mark");
        assert!(
            m.goal.contains("1 of 3") && m.goal.contains("1 of 6"),
            "{}",
            m.goal
        );
        assert!(mark(&st(IDLE, CaseKind::Evidence, 0, 0), b).is_none());
        for l in brief.iter().chain(&one).chain(&closed) {
            assert!(l.text.len() <= 110, "too long for the radio: {}", l.text);
        }
    }
}
