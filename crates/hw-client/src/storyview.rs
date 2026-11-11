//! The Doctor's story on screen: the chapter and its goal at the top of the
//! wrist radio, a gold star on the compass and the map, a gold light over
//! where to go, Commander Starling's words as radio transmissions (text
//! only), and the search prompt with its bar. The rules are the server's
//! (`hw_sim::story`); this only shows what it says.

use crate::radioview::{beacon_lit, Radio, StoryMark, STORY_GOLD};
use engine::prelude::*;
use engine::ui::egui;
use glam::Vec3;
use hw_net::ToClient;
use hw_sim::story::{
    chapter, CHAPTERS, CONTACT_NAME, FIGHT, FINISHED, GO, LOCKED, REPORT, SEARCH, SEARCH_REACH,
    TITLE,
};
use std::collections::VecDeque;

/// What the server last said.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct State {
    pub done: u8,
    pub stage: u8,
    pub at: Vec3,
    pub level: u8,
    /// How far along holding the help key is (0-1).
    pub held: f32,
}

/// One line on the air: who speaks, and what.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub who: &'static str,
    pub text: String,
}

#[derive(Default)]
pub struct StoryView {
    pub state: Option<State>,
    /// Lines still to show, and how long the first has been up (s).
    pub lines: VecDeque<Line>,
    /// Lines said since the game last took them, for the chat log.
    pub heard: Vec<Line>,
    shown: f32,
    /// The gold light over where to go, and where it stands.
    beam: Option<(EntityId, Vec3)>,
    /// The statue asked for (its looks' key), and the one standing.
    statue_asked: Option<u64>,
    statue: Option<(u64, EntityId)>,
}

/// Seconds a line stays up: long enough to read.
pub fn read_time(text: &str) -> f32 {
    2.5 + text.len() as f32 / 16.0
}

impl StoryView {
    /// A story message; false for other messages.
    pub fn hear(&mut self, m: &ToClient) -> bool {
        let ToClient::Story {
            done,
            stage,
            at,
            level,
            held,
        } = m
        else {
            return false;
        };
        let next = State {
            done: *done,
            stage: *stage,
            at: Vec3::from_array(*at),
            level: *level,
            held: *held as f32 / 255.0,
        };
        let said = lines(self.state, next);
        self.heard.extend(said.iter().cloned());
        self.lines.extend(said);
        self.state = Some(next);
        true
    }

    /// Every frame: the light over the goal, the line on the air.
    pub fn update(&mut self, world: &mut World, radio: &mut Radio, dt: f32) {
        radio.story = self.state.map(mark);
        let want = self
            .state
            .filter(|s| matches!(s.stage, REPORT | GO | FIGHT | SEARCH))
            .map(|s| s.at);
        if want != self.beam.map(|b| b.1) {
            if let Some((e, _)) = self.beam.take() {
                world.despawn(e);
            }
            if let Some(at) = want {
                let gold = rgb(255, 200, 60);
                if let Ok(e) = world.place(beacon_lit(gold), Place::new(at.x, at.y, at.z)) {
                    self.beam = Some((e, at));
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

    /// The line on the air (top middle, under the compass) and the search
    /// prompt with its bar.
    pub fn panel(&self, ctx: &egui::Context, hero: Option<Vec3>) {
        if let Some(l) = self.lines.front() {
            egui::Area::new(egui::Id::new("story-line"))
                // Words only: the pointer over it must not take the keys.
                .interactable(false)
                .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 52.0))
                .show(ctx, |ui| {
                    crate::uikit::card().show(ui, |ui| {
                        ui.set_max_width(620.0);
                        ui.label(
                            egui::RichText::new(format!("WRIST RADIO - {}", l.who))
                                .color(STORY_GOLD)
                                .font(crate::uikit::ui_font(ui.ctx(), 13.0)),
                        );
                        ui.label(
                            egui::RichText::new(&l.text)
                                .color(egui::Color32::from_rgb(255, 244, 214))
                                .size(17.0),
                        );
                    });
                });
        }
        let Some(s) = self.state.filter(|s| s.stage == SEARCH) else {
            return;
        };
        let near = hero.is_some_and(|h| h.distance(s.at) < SEARCH_REACH as f32 + 1.5);
        let Some(c) = chapter(s.done).filter(|_| near) else {
            return;
        };
        egui::Area::new(egui::Id::new("story-search"))
            // Words only: the pointer over it must not take the keys.
            .interactable(false)
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -210.0))
            .show(ctx, |ui| {
                crate::uikit::card().show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(c.search_goal)
                            .color(STORY_GOLD)
                            .size(17.0)
                            .strong(),
                    );
                    crate::uikit::gauge_in(ui, 260.0, 15.0, s.held, STORY_GOLD);
                });
            });
    }

    /// The story done: our hero's bronze statue on the plinth in front of
    /// Hero Hall, in the heroic stance, built on the people worker.
    pub fn statue(
        &mut self,
        world: &mut World,
        people: Option<&crate::people::People>,
        models: &std::collections::BTreeMap<crate::people::Who, std::sync::Arc<AnimatedModel>>,
        look: Option<&actors_generator::CharacterConfig>,
        town: &hw_gen::urban::world::TownWorld,
    ) -> EngineResult<()> {
        let done = self.state.is_some_and(|s| s.stage == FINISHED);
        let key = look.map(crate::looks::key).filter(|_| done);
        if self.statue.is_some_and(|s| Some(s.0) != key) {
            if let Some((_, e)) = self.statue.take() {
                world.despawn(e);
            }
        }
        let (Some(key), Some(look)) = (key, look) else {
            return Ok(());
        };
        if self.statue.is_some() {
            return Ok(());
        }
        let who = crate::people::Who::Statue(key);
        let Some(model) = models.get(&who) else {
            if self.statue_asked != Some(key) {
                if let Some(p) = people {
                    p.request_statue(key, look.clone());
                    self.statue_asked = Some(key);
                }
            }
            return Ok(());
        };
        let Some((top, facing)) = town.landmarks.iter().find_map(|l| l.statue_top()) else {
            return Ok(());
        };
        let e = world.spawn_animated_shared(
            model.clone(),
            Place::new(top.x as f32, top.y as f32, top.z as f32)
                .with_yaw_deg(facing.x.atan2(facing.y).to_degrees() as f32),
        )?;
        world.play_animation(e, "heroic")?;
        // Cast in bronze: it holds still.
        world.set_animation_speed(e, 0.0)?;
        self.statue = Some((key, e));
        Ok(())
    }

    /// The statue's solid, for our own hero's collisions when we joined a
    /// friend (the host adds it to the server's).
    pub fn statue_solids(
        &self,
        town: &hw_gen::urban::world::TownWorld,
    ) -> Vec<hw_sim::collision::Solid> {
        if !self.state.is_some_and(|s| s.stage == FINISHED) {
            return Vec::new();
        }
        town.landmarks
            .iter()
            .filter_map(|l| l.statue_top())
            .map(|(top, f)| hw_sim::story::statue_solid(top, f))
            .collect()
    }

    pub fn clear(&mut self, world: &mut World) {
        if let Some((e, _)) = self.beam.take() {
            world.despawn(e);
        }
        if let Some((_, e)) = self.statue.take() {
            world.despawn(e);
        }
        self.state = None;
        self.lines.clear();
        self.shown = 0.0;
    }
}

/// The story's lines on the radio for a state.
pub fn mark(s: State) -> StoryMark {
    let total = CHAPTERS.len();
    let heading = match chapter(s.done) {
        Some(c) => format!(
            "THE DOCTOR'S STORY - chapter {} of {total}: {}",
            s.done + 1,
            c.title
        ),
        None => "THE DOCTOR'S STORY - the end".to_string(),
    };
    let (goal, at) = match s.stage {
        LOCKED => (format!("Next lead at level {}", s.level), None),
        REPORT => (format!("Report to {CONTACT_NAME} at Hero Hall"), Some(s.at)),
        GO | FIGHT => (
            chapter(s.done).map_or("", |c| c.goal).to_string(),
            Some(s.at),
        ),
        SEARCH => (
            chapter(s.done)
                .map_or("", |c| c.search_goal)
                .replace("Hold E to ", "Then "),
            Some(s.at),
        ),
        _ => (format!("You are the {TITLE}"), None),
    };
    StoryMark { heading, goal, at }
}

/// What the radio says going from `before` to `now`.
pub fn lines(before: Option<State>, now: State) -> Vec<Line> {
    let say = |text: &str| Line {
        who: CONTACT_NAME,
        text: text.to_string(),
    };
    let mut out = Vec::new();
    let Some(b) = before else {
        // Just joined: only a new hero gets the call; the rest see the
        // goal on the radio.
        if now.done == 0 && now.stage == REPORT {
            out.push(say(
                "This is Commander Starling at Hero Hall. Come and see me: the Doctor is up to something.",
            ));
            out.push(say("Follow the gold star on your compass."));
        }
        return out;
    };
    if now.done > b.done {
        // A chapter done: what was found, and what comes next.
        for k in b.done..now.done {
            if let Some(c) = chapter(k) {
                out.extend(c.found.iter().map(|t| say(t)));
            }
        }
        match now.stage {
            LOCKED => out.push(say(&format!(
                "My next lead needs a stronger hero. Keep the streets safe till level {}.",
                now.level
            ))),
            FINISHED => out.push(Line {
                who: "Hero Hall",
                text: format!("The town calls you the {TITLE}. Your statue stands at Hero Hall."),
            }),
            _ => {}
        }
        return out;
    }
    if now.done != b.done {
        return out;
    }
    match (b.stage, now.stage) {
        (LOCKED, REPORT) => out.push(say("I've got a new lead on the Doctor. Come to Hero Hall.")),
        (REPORT, GO) => {
            if let Some(c) = chapter(now.done) {
                out.extend(c.brief.iter().map(|t| say(t)));
            }
        }
        (GO, FIGHT) => out.push(say("That's them. Go get 'em!")),
        (FIGHT, SEARCH) => out.push(say(
            "They're down. Quick, before more come: search the place.",
        )),
        (FIGHT, GO) | (SEARCH, GO) => {
            out.push(say("They've pulled back. Come back when you're ready."))
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(done: u8, stage: u8) -> State {
        State {
            done,
            stage,
            at: Vec3::ZERO,
            level: 3,
            held: 0.0,
        }
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn the_radio_tells_the_story_as_it_moves_on() {
        let first = lines(None, st(0, REPORT));
        assert!(first[0].text.contains("Hero Hall"));
        let brief = lines(Some(st(0, REPORT)), st(0, GO));
        assert_eq!(brief.len(), CHAPTERS[0].brief.len());
        let found = lines(Some(st(0, SEARCH)), st(1, LOCKED));
        assert!(found.len() > CHAPTERS[0].found.len());
        assert!(found.last().unwrap().text.contains("level 3"));
        let end = lines(Some(st(6, FIGHT)), st(7, FINISHED));
        assert!(end.last().unwrap().text.contains(TITLE));
        // Nothing new: nothing said.
        assert!(lines(Some(st(2, FIGHT)), st(2, FIGHT)).is_empty());
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn the_radio_names_the_chapter_and_the_goal() {
        let m = mark(st(2, GO));
        assert!(m.heading.contains("chapter 3 of 7"));
        assert_eq!(m.goal, CHAPTERS[2].goal);
        assert!(m.at.is_some());
        let m = mark(st(2, SEARCH));
        assert!(m.goal.starts_with("Then "), "{}", m.goal);
        let m = mark(st(3, LOCKED));
        assert!(m.at.is_none() && m.goal.contains("level 3"));
    }
}
