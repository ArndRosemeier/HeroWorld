//! The Doctor's story: the spine of the game. Seven chapters, each opening
//! at a level, lead the heroes from a street brawl with his technicians to
//! the Doctor himself in his lab under the quarter. Commander Starling at
//! Hero Hall hands each one out; the wrist radio carries every word of it
//! (text only, no voices). The chain is fixed, but where each chapter plays
//! comes from the town: its labs, the robot works, the news kiosk over the
//! Doctor's lab, the Tesla tower.
//!
//! A chapter goes: open (the team is of its level) -> report to Hero Hall
//! for the briefing -> go there -> fight (the group stands there once a
//! hero comes near) -> search (hold the help key at the find, if the
//! chapter has one) -> done: karma, and the next lead. The last one puts
//! the hero's statue on the empty plinth in front of Hero Hall and gives
//! them the title [`TITLE`].
//!
//! Progress is in the hero file ([`hw_net::Sheet::story`]). In co-op the
//! team follows the host's story, as it fights at the host's level; heroes
//! who were there when a chapter ends have it in their own file too.
//!
//! The chapters are data ([`CHAPTERS`]): faction chains and cases made up
//! for a town can come later as more lists of the same [`Chapter`].

use crate::combat::{self, EnemyKind, Fighter, Fights, Spot};
use crate::progress::{self, Affinity};
use crate::{Control, Hero, Motion, Net, Progress, DT, GROUND_Y, TICK_HZ};
use bevy_ecs::prelude::*;
use glam::{DVec2, DVec3};
use hw_core::NetId;
use hw_gen::building::BuildingUse;
use hw_gen::urban::area::Area;
use hw_gen::urban::deep::EntranceKind;
use hw_gen::urban::landmarks::LandmarkKind;
use hw_gen::urban::world::TownWorld;
use hw_net::ToClient;

/// Where a chapter plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// A street just outside the Doctor's quarter, on the side of Hero
    /// Hall, clear of his corners.
    Street,
    /// One of his labs in the quarter.
    Lab,
    /// The robot works' gate (or the Tesla tower's, in a town without).
    Works,
    /// The newspaper kiosk over his lab.
    Kiosk,
    /// The Tesla tower's plaza.
    Tower,
    /// The great hall of his lab, down the kiosk's lift.
    Lair,
}

/// Who stands there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Group {
    /// How many for one hero (one more for each friend).
    pub size: u32,
    pub only: Option<EnemyKind>,
    pub lead: Option<EnemyKind>,
    /// Levels above the team.
    pub extra: u8,
}

/// One chapter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chapter {
    pub title: &'static str,
    /// The team's level it opens at.
    pub level: u8,
    pub place: Place,
    pub group: Group,
    /// The goal, as the radio shows it while going there and fighting.
    pub goal: &'static str,
    /// Commander Starling's briefing at the hall.
    pub brief: &'static [&'static str],
    /// Seconds of holding the help key at the find (0: none), and what to
    /// do there.
    pub search: f64,
    pub search_goal: &'static str,
    /// What the heroes learn at the end (the radio).
    pub found: &'static [&'static str],
}

/// Who hands the chapters out (one of Hero Hall's heroes, by index).
pub const CONTACT: u8 = 0;
pub const CONTACT_NAME: &str = "Commander Starling";
/// What the hero is called once the Doctor is beaten.
pub const TITLE: &str = "Bane of the Doctor";

const BRUTE_TECH: Option<EnemyKind> = None;

pub const CHAPTERS: [Chapter; 7] = [
    Chapter {
        title: "Sparks on the Street",
        level: 1,
        place: Place::Street,
        group: Group { size: 3, only: Some(EnemyKind::Technician), lead: None, extra: 0 },
        goal: "Stop the Doctor's technicians",
        brief: &[
            "Glad you came, rookie. Something's rotten at the edge of the Doctor's quarter.",
            "His technicians broke out of a lab with a crate of something hot. Stop them.",
            "Then search that crate. I want to know what he's up to.",
        ],
        search: 3.0,
        search_goal: "Hold E to search their crate",
        found: &[
            "A punch card with a lightning seal: an order for forty Aether cells.",
            "Forty! That's enough to light half the town. Report back to Hero Hall.",
        ],
    },
    Chapter {
        title: "The Stolen Cells",
        level: 3,
        place: Place::Lab,
        group: Group { size: 4, only: BRUTE_TECH, lead: Some(EnemyKind::Sentry), extra: 1 },
        goal: "Raid the lab hoarding the stolen cells",
        brief: &[
            "Those cells were stolen from the Aether Works, and they've turned up in one of his labs.",
            "A robo-sentry guards it. Smash it, get inside the yard, and find his shipping ledger.",
        ],
        search: 4.0,
        search_goal: "Hold E to read the shipping ledger",
        found: &[
            "The ledger: every cell went on to the robot works. He's building something big.",
            "Report back to Hero Hall. We'll need you stronger for the works.",
        ],
    },
    Chapter {
        title: "The Robot Works",
        level: 5,
        place: Place::Works,
        group: Group { size: 4, only: BRUTE_TECH, lead: Some(EnemyKind::Sentry), extra: 1 },
        goal: "Break into the robot works",
        brief: &[
            "The robot works runs day and night now. Whatever he's building, it eats Aether.",
            "Fight your way to the gate and jam the assembly line. Mind the sentries.",
        ],
        search: 5.0,
        search_goal: "Hold E to jam the assembly line",
        found: &[
            "The line's jammed. On the foreman's desk: a map to a news kiosk.",
            "A news kiosk? Report back to Hero Hall. Something doesn't add up.",
        ],
    },
    Chapter {
        title: "The News Kiosk",
        level: 8,
        place: Place::Kiosk,
        group: Group { size: 4, only: BRUTE_TECH, lead: Some(EnemyKind::Alpha), extra: 1 },
        goal: "Find out who guards the news kiosk",
        brief: &[
            "That kiosk on the back street sells no papers. Mutants stand round it all day.",
            "Clear them off and look inside. My guess: it goes down, not in.",
        ],
        search: 4.0,
        search_goal: "Hold E to read the lift's logbook",
        found: &[
            "The kiosk is a lift. The logbook: 'Colossus ready. Aether tower at full charge.'",
            "He means to march that walking tank on the town. Come back to Hero Hall.",
        ],
    },
    Chapter {
        title: "The Colossus Walks",
        level: 11,
        place: Place::Works,
        group: Group { size: 1, only: Some(EnemyKind::Colossus), lead: None, extra: 1 },
        goal: "Stop the Doctor's colossus",
        brief: &[
            "It's out. His colossus is marching from the works, and it's coming for the town.",
            "Break its plates, then hit the core when it kneels. We're all counting on you.",
        ],
        search: 0.0,
        search_goal: "",
        found: &[
            "It's down! Without the tank, the Tesla tower is all he has left.",
            "Report back to Hero Hall when you're ready to take the tower.",
        ],
    },
    Chapter {
        title: "The Tesla Tower",
        level: 14,
        place: Place::Tower,
        group: Group { size: 5, only: BRUTE_TECH, lead: Some(EnemyKind::Sentry), extra: 2 },
        goal: "Take the Tesla tower's plaza",
        brief: &[
            "The tower feeds his whole quarter, his lab included.",
            "Take the plaza and shut the great coil down. With the lights out, he'll have to show himself.",
        ],
        search: 6.0,
        search_goal: "Hold E to shut the great coil down",
        found: &[
            "The coil is dark! The whole quarter flickered... and something moved under the kiosk.",
            "He's in his lab. Come to Hero Hall: this is the last of it.",
        ],
    },
    Chapter {
        title: "The Doctor",
        level: 16,
        place: Place::Lair,
        group: Group { size: 4, only: BRUTE_TECH, lead: Some(EnemyKind::Doctor), extra: 1 },
        goal: "Beat the Doctor in his lab",
        brief: &[
            "Take the kiosk's lift down to his lab. He'll be in the great hall with his coils.",
            "He throws lightning, and when you get close he storms: step out of the green ring.",
            "Bring him in, hero. The whole town is behind you.",
        ],
        search: 0.0,
        search_goal: "",
        found: &[
            "The Doctor is beaten and on his way to the cells. The town is safe... for now.",
            "Come to Hero Hall. There's a plinth out front that's been waiting for you.",
        ],
    },
];

/// Stages of the next chapter, as [`ToClient::Story`] carries them.
pub const LOCKED: u8 = 0;
pub const REPORT: u8 = 1;
pub const GO: u8 = 2;
pub const FIGHT: u8 = 3;
pub const SEARCH: u8 = 4;
/// Every chapter done.
pub const FINISHED: u8 = 5;

/// A hero this near the contact's place is briefed (m).
pub const BRIEF_REACH: f64 = 9.0;
/// The chapter's group stands there once a hero is this near (m).
pub const WAKE: f64 = 140.0;
/// A hero this near the find can search it (m).
pub const SEARCH_REACH: f64 = 4.0;
/// Heroes this near share the chapter's reward (m).
pub const REWARD: f64 = 200.0;
/// The first chapter's street is at least this far from Hero Hall (m).
pub const HALL_CLEAR: f64 = 150.0;
/// The reward: this share of what the hero's next level takes.
pub const REWARD_SHARE: f64 = 0.3;

/// Where the team stands in the story, and the chapter's fight.
#[derive(Default)]
pub struct Story {
    /// Where each chapter plays (feet on the ground, which way it faces);
    /// none for all if the town has no villain quarter.
    places: Vec<(DVec3, DVec2)>,
    /// Where the contact stands on Hero Hall's plaza.
    hall: Option<DVec3>,
    /// The top of the statue's plinth, and the way it faces.
    statue: Option<(DVec3, DVec2)>,
    /// The team's chapters done, and briefed on the next.
    pub done: u8,
    pub briefed: bool,
    pub stage: u8,
    /// The chapter's fight spot (in [`Fights::spots`]), while its group
    /// stands.
    spot: Option<usize>,
    /// Seconds the help key has been held at the find.
    held: f64,
    last_sent: Option<ToClient>,
    news: Vec<ToClient>,
}

/// A chapter's number in its list, if there is one.
pub fn chapter(done: u8) -> Option<&'static Chapter> {
    CHAPTERS.get(done as usize)
}

impl Story {
    /// Finds where every chapter plays in `world` (the town), from `area`
    /// (a square holding the villain quarter) and the solids. None if the
    /// town has no quarter or no Hero Hall.
    pub fn plan(world: &TownWorld, area: &Area, solids: &crate::collision::Obstacles) -> Self {
        let mut s = Story::default();
        let Some(hall) = world
            .landmarks
            .iter()
            .find(|l| l.kind == LandmarkKind::HeroHall)
        else {
            return s;
        };
        let Some(d) = world.town.dystopia.as_ref() else {
            return s;
        };
        let contact = hall.hero_spots()[CONTACT as usize].0;
        s.hall = Some(DVec3::new(contact.x, hall.base, contact.y));
        s.statue = hall.statue_top();
        let on_ground = |p: DVec2| DVec3::new(p.x, solids.ground_at(p).max(GROUND_Y), p.y);
        let home = hall.pos;
        let corners: Vec<DVec2> = crate::quarter::corners(area).iter().map(|c| c.at).collect();
        // A street outside the quarter, nearest the hall, clear of the
        // Doctor's corners (the group faces along it).
        // Not on the hall's doorstep either: a walk away at least.
        let streets: Vec<(DVec2, DVec2)> = area
            .roads
            .iter()
            .filter(|r| !r.main)
            .flat_map(|r| r.pts.windows(2).map(|w| ((w[0] + w[1]) * 0.5, w[1] - w[0])))
            .filter(|(p, _)| {
                let out = p.distance(d.centre) - d.radius;
                (40.0..300.0).contains(&out)
                    && !d.contains(*p)
                    && corners.iter().all(|c| c.distance(*p) > 120.0)
            })
            .collect();
        let nearest = |min: f64| {
            streets
                .iter()
                .filter(|(p, _)| p.distance(home) > min)
                .min_by(|a, b| a.0.distance(home).total_cmp(&b.0.distance(home)))
                .copied()
        };
        let street = nearest(HALL_CLEAR).or_else(|| nearest(0.0));
        // His labs in the quarter: the one nearest the hall.
        let lab = area
            .buildings
            .iter()
            .filter(|b| b.site.usage == BuildingUse::Lab)
            .map(|b| {
                let s = &b.site;
                (
                    s.origin + s.along * s.size.x * 0.5 - s.inward * 4.0,
                    -s.inward,
                )
            })
            .filter(|(p, _)| d.contains(*p))
            .min_by(|a, b| a.0.distance(home).total_cmp(&b.0.distance(home)));
        let landmark = |k: LandmarkKind| world.landmarks.iter().find(|l| l.kind == k);
        let tower =
            landmark(LandmarkKind::TeslaTower).map(|l| (l.pos + l.dir * (l.plaza * 0.75), l.dir));
        // Out of the works' gate onto its street (where the colossus comes
        // out, see `incidents`).
        let works = landmark(LandmarkKind::RobotWorks).map(|l| {
            let front = l.pos + l.dir * (l.plaza + 14.0);
            (front, l.dir)
        });
        let lair = world.lair_centre().and_then(|c| {
            world
                .deep_near(c, 160.0)
                .into_iter()
                .find(|t| !t.lairs.is_empty())
        });
        let kiosk = lair.as_ref().and_then(|t| {
            t.entrances
                .iter()
                .find(|e| e.kind == EntranceKind::Kiosk)
                .map(|e| (e.pos + e.door * 6.0, e.door))
        });
        let hall_room = lair.as_ref().map(|t| {
            let l = &t.lairs[0];
            let v = &t.voids[l.hall];
            let c = v.world((v.min + v.max) * 0.5);
            (DVec3::new(c.x, l.floor, c.y), -v.along)
        });
        let (Some(street), Some(tower)) = (street, tower) else {
            return s;
        };
        let lab = lab.unwrap_or(street);
        let works = works.unwrap_or(tower);
        let kiosk = kiosk.unwrap_or(lab);
        for c in &CHAPTERS {
            let (p, f) = match c.place {
                Place::Street => street,
                Place::Lab => lab,
                Place::Works => works,
                Place::Kiosk => kiosk,
                Place::Tower => tower,
                Place::Lair => match hall_room {
                    Some(h) => {
                        s.places.push(h);
                        continue;
                    }
                    None => tower,
                },
            };
            s.places.push((on_ground(p), f.normalize_or(DVec2::X)));
        }
        s
    }

    /// Does this town have the story (a villain quarter and Hero Hall)?
    pub fn available(&self) -> bool {
        self.places.len() == CHAPTERS.len() && self.hall.is_some()
    }

    /// Where chapter `k` plays.
    pub fn place(&self, k: usize) -> Option<(DVec3, DVec2)> {
        self.places.get(k).copied()
    }

    /// The chapter's fight spot while its group stands.
    pub fn spot(&self) -> Option<usize> {
        self.spot
    }

    /// Where the contact stands.
    pub fn hall(&self) -> Option<DVec3> {
        self.hall
    }

    /// Where to go now, for the radio.
    pub fn objective(&self) -> Option<DVec3> {
        match self.stage {
            REPORT | FINISHED => self.hall,
            GO | FIGHT | SEARCH => self.place(self.done as usize).map(|p| p.0),
            _ => None,
        }
    }

    /// Every tick: searching at the find. Once a second: the team's story,
    /// briefings, the fight, chapters done.
    pub(crate) fn step(&mut self, world: &mut World, tick: u64) {
        if !self.available() {
            return;
        }
        let heroes = heroes(world);
        if heroes.is_empty() {
            return;
        }
        if self.stage == SEARCH {
            self.search(world, tick, &heroes);
        }
        if tick.is_multiple_of(TICK_HZ as u64) {
            self.settle(world, tick, &heroes);
        }
        let held = match chapter(self.done) {
            Some(c) if self.stage == SEARCH && c.search > 0.0 => {
                ((self.held / c.search).clamp(0.0, 1.0) * 255.0) as u8
            }
            _ => 0,
        };
        let level = chapter(self.done).map_or(0, |c| c.level);
        let at = self.objective().unwrap_or(DVec3::ZERO).as_vec3().to_array();
        let news = ToClient::Story {
            done: self.done,
            stage: self.stage,
            at,
            level,
            // Only whole steps of the bar, so it is not sent every tick.
            held: held & !15,
        };
        if self.last_sent.as_ref() != Some(&news) {
            self.last_sent = Some(news.clone());
            self.news.push(news);
        }
    }

    /// The team's place in the story (the host's), the stage it is at.
    fn settle(&mut self, world: &mut World, tick: u64, heroes: &[Seen]) {
        let host = &heroes[0];
        let (done, briefed) = (host.story.min(CHAPTERS.len() as u8), host.briefed);
        if done != self.done {
            // The host changed (or a save was loaded): start that chapter.
            self.end_fight(world);
            self.held = 0.0;
        }
        self.done = done;
        self.briefed = briefed;
        let Some(c) = chapter(done) else {
            self.stage = FINISHED;
            return;
        };
        let team = world.resource::<Fights>().team_level;
        if team < c.level {
            self.stage = LOCKED;
            return;
        }
        if !self.briefed {
            self.stage = REPORT;
            let hall = self.hall.expect("available");
            if heroes.iter().any(|h| h.at.distance(hall) < BRIEF_REACH) {
                self.briefed = true;
                set_sheets(world, |s| {
                    if s.story == done {
                        s.briefed = true;
                    }
                });
                self.stage = GO;
            }
            return;
        }
        let (at, facing) = self.place(done as usize).expect("available");
        let near = heroes
            .iter()
            .map(|h| h.at.distance(at))
            .fold(f64::INFINITY, f64::min);
        match self.stage {
            SEARCH => {
                if near > combat::SLEEP {
                    self.stage = GO;
                    self.held = 0.0;
                }
            }
            FIGHT => {
                let spot = self.spot.expect("a fight");
                let (groups, members) = {
                    let s = &world.resource::<Fights>().spots[spot];
                    (s.groups, s.members)
                };
                if groups > 0 && members == 0 {
                    self.spot = None;
                    if c.search > 0.0 {
                        self.stage = SEARCH;
                        self.held = 0.0;
                    } else {
                        self.finish(world, heroes);
                    }
                } else if near > combat::SLEEP {
                    // Left behind: the group goes home and stands there
                    // fresh when the heroes come back.
                    self.end_fight(world);
                    self.stage = GO;
                }
            }
            _ => {
                self.stage = GO;
                if near < WAKE {
                    self.start_fight(world, tick, c, at, facing, heroes.len());
                    self.stage = FIGHT;
                }
            }
        }
    }

    /// The chapter's group takes its place.
    fn start_fight(
        &mut self,
        world: &mut World,
        tick: u64,
        c: &Chapter,
        at: DVec3,
        facing: DVec2,
        heroes: usize,
    ) {
        let g = c.group;
        let size = if g.only == Some(EnemyKind::Colossus) {
            1
        } else {
            g.size + heroes.saturating_sub(1) as u32
        };
        let spot = Spot {
            extra: g.extra,
            size: Some(size),
            only: g.only,
            once: true,
            lead: g.lead,
            provoke: Some(f64::INFINITY),
            ..Spot::corner(at, facing)
        };
        let index = {
            let mut f = world.resource_mut::<Fights>();
            f.spots.push(spot);
            f.spots.len() - 1
        };
        combat::spawn_group(world, tick, index, heroes);
        self.spot = Some(index);
    }

    /// The chapter's group leaves (its spot stays, empty, never refilled).
    fn end_fight(&mut self, world: &mut World) {
        let Some(spot) = self.spot.take() else {
            return;
        };
        let mut gone: Vec<(NetId, Entity)> = world
            .query::<(Entity, &Net, &combat::Enemy)>()
            .iter(world)
            .filter(|(_, _, en)| en.spot == spot)
            .map(|(e, n, _)| (n.0, e))
            .collect();
        gone.sort_by_key(|g| g.0);
        for (id, e) in gone {
            world.resource_mut::<Fights>().paths.remove(&id);
            world.despawn(e);
        }
        let mut f = world.resource_mut::<Fights>();
        f.spots[spot].members = 0;
        f.spots[spot].respawn_at = None;
    }

    /// Heroes holding the help key at the find.
    fn search(&mut self, world: &mut World, _tick: u64, heroes: &[Seen]) {
        let Some(c) = chapter(self.done) else {
            return;
        };
        let (at, _) = self.place(self.done as usize).expect("available");
        let searching = heroes
            .iter()
            .any(|h| h.help && h.at.distance(at) < SEARCH_REACH);
        if !searching {
            self.held = (self.held - DT).max(0.0);
            return;
        }
        self.held += DT;
        if self.held + 1e-9 >= c.search {
            self.held = 0.0;
            self.finish(world, heroes);
        }
    }

    /// The chapter is done: karma for the heroes near, and the next lead.
    fn finish(&mut self, world: &mut World, heroes: &[Seen]) {
        let Some(c) = chapter(self.done) else {
            return;
        };
        let (at, _) = self.place(self.done as usize).expect("available");
        let done = self.done + 1;
        let near: Vec<NetId> = heroes
            .iter()
            .filter(|h| h.at.distance(at) < REWARD)
            .map(|h| h.id)
            .collect();
        let mut rewards = Vec::new();
        for h in heroes.iter().filter(|h| near.contains(&h.id)) {
            let karma = (progress::karma_to_next(h.level) as f64 * REWARD_SHARE).round() as u32;
            rewards.push(combat::Reward {
                hero: h.id,
                karma,
                deed: Some(Affinity::Justice),
                lean: 0,
            });
        }
        world.resource_mut::<Fights>().rewards.extend(rewards);
        // The host's file moves on (the team follows it), and so do the
        // files of everyone who was there.
        let host = heroes[0].id;
        for (n, mut p) in world.query::<(&Net, &mut Progress)>().iter_mut(world) {
            if n.0 != host && !near.contains(&n.0) {
                continue;
            }
            if p.sheet.story < done {
                p.sheet.story = done;
                p.sheet.briefed = false;
                p.changed = true;
            }
        }
        let _ = c;
        self.done = done;
        self.briefed = false;
        self.held = 0.0;
        self.stage = if chapter(done).is_some() {
            REPORT
        } else {
            FINISHED
        };
    }

    /// The hero's statue on its plinth, once the story is done: solid like
    /// everything drawn.
    pub fn statue_solids(&self) -> Vec<crate::collision::Solid> {
        match self.statue {
            Some((top, facing)) if self.stage == FINISHED => vec![statue_solid(top, facing)],
            _ => Vec::new(),
        }
    }

    /// For a player who joins.
    pub fn snapshot(&self) -> Vec<ToClient> {
        self.last_sent.iter().cloned().collect()
    }

    pub fn take_news(&mut self) -> Vec<ToClient> {
        std::mem::take(&mut self.news)
    }
}

/// The statue standing on the plinth whose top is `top` (see
/// [`hw_gen::urban::landmarks::HALL_FIGURE`]).
pub fn statue_solid(top: DVec3, facing: DVec2) -> crate::collision::Solid {
    use hw_gen::urban::landmarks::HALL_FIGURE;
    let f = facing.normalize_or(DVec2::Y);
    let r = HALL_FIGURE.0;
    crate::collision::Solid {
        origin: DVec2::new(top.x, top.z),
        along: f.perp(),
        inward: f,
        min: DVec2::splat(-r),
        max: DVec2::splat(r),
        y0: top.y,
        y1: top.y + HALL_FIGURE.1,
    }
}

/// A hero as the story sees them: the host first.
struct Seen {
    id: NetId,
    at: DVec3,
    level: u8,
    story: u8,
    briefed: bool,
    /// Holding the help key, on their feet.
    help: bool,
}

fn heroes(world: &mut World) -> Vec<Seen> {
    let mut v: Vec<(crate::PlayerId, Seen)> = world
        .query::<(&Hero, &Net, &Motion, &Fighter, &Control, &Progress)>()
        .iter(world)
        .map(|(h, n, m, f, c, p)| {
            (
                h.owner,
                Seen {
                    id: n.0,
                    at: m.0.pos,
                    level: f.level,
                    story: p.sheet.story,
                    briefed: p.sheet.briefed,
                    help: c.0.revive && f.standing(),
                },
            )
        })
        .collect();
    v.sort_by_key(|x| x.0);
    v.into_iter().map(|x| x.1).collect()
}

fn set_sheets(world: &mut World, f: impl Fn(&mut hw_net::Sheet)) {
    for mut p in world.query::<&mut Progress>().iter_mut(world) {
        let before = p.sheet.clone();
        f(&mut p.sheet);
        if p.sheet != before {
            p.changed = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapters_open_in_order_and_fit_the_radio() {
        let mut last = 0;
        for c in &CHAPTERS {
            assert!(c.level > last || last == 0 && c.level == 1, "{}", c.title);
            assert!(c.level <= progress::MAX_LEVEL);
            last = c.level;
            assert!(!c.brief.is_empty() && !c.found.is_empty());
            for line in c.brief.iter().chain(c.found) {
                assert!(line.len() <= 110, "too long for the radio: {line}");
            }
            assert_eq!(c.search > 0.0, !c.search_goal.is_empty(), "{}", c.title);
        }
        assert_eq!(CHAPTERS[0].level, 1, "a new hero starts the story at once");
        assert_eq!(
            CHAPTERS.last().unwrap().group.lead,
            Some(EnemyKind::Doctor),
            "the story ends with the Doctor himself"
        );
    }
}
