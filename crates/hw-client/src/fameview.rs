//! Fame on screen (the rules are `hw_sim::fame`): the town answering a
//! hero it has come to know.
//!
//! - People in the street stop for a hero they know, turn, and wave, clap
//!   or cheer (`crowdview` holds them; here are their words).
//! - A few hold out an autograph book: stand still beside them and our
//!   hero signs it.
//! - Fans go about in the hero's colours, some of them children in
//!   home-made capes (looks built on the people worker, see
//!   `people::fan`).
//! - The Daily Comet prints what the hero did (`headlines`), the front
//!   page spins in (`paper`), newsboys shout it, and the cuttings are kept
//!   in the hero's file.
//!
//! All of it is looks: nothing here changes the game the server plays.

use crate::chat::{Said, Voice, SAY_RANGE, SHOUT_RANGE};
use crate::crowdview::{CrowdView, Greeted, Star};
use crate::headlines::{self, Deed, Edition, SCRAPBOOK_KEEP};
use crate::paper::{Paper, Scrapbook};
use crate::people::{People, FAN_LOOKS};
use actors_generator::CharacterConfig;
use engine::prelude::*;
use engine::ui::egui;
use glam::{DVec2, Mat4, Vec3};
use hw_core::NetId;
use hw_gen::urban::area::Area;
use hw_net::{EntityKind, EntityState, Sheet, ToClient};
use hw_sim::fame::{self, Fame, Greeting};
use hw_sim::incidents::IncidentKind;
use std::collections::BTreeSet;

/// A deed counts as ours when our hero was this near where it ended (m).
const THERE: f32 = 90.0;
/// The town's own heroes from Hero Hall are local heroes to the townsfolk.
const NPC_FAME: Fame = Fame::Local;
/// At least this long between two things said to a hero (s); a request
/// for an autograph and the thanks for one are always said.
const TALK_GAP: f64 = 2.6;
/// The autograph book's size (m): wide, thick, tall.
const BOOK: Vec3 = Vec3::new(0.15, 0.022, 0.2);

/// Our hero is signing a book.
#[derive(Clone, Copy, Debug)]
struct Signing {
    citizen: u64,
    /// The town's clock when it is done.
    until: f64,
    /// Where the one with the book stands.
    at: Vec3,
}

/// A deed heard of, to be printed: what, and where.
struct Heard {
    deed: Deed,
    at: Option<Vec3>,
}

#[derive(Default)]
pub struct FameView {
    /// How well the town knows our hero.
    pub fame: Option<Fame>,
    /// The looks our fans' crowds were asked for (their key).
    fan_key: Option<u64>,
    signing: Option<Signing>,
    /// Someone waits with a book for our hero: where their head is.
    seeker: Option<Vec3>,
    book: Option<EntityId>,
    talk_next: f64,
    /// What was said this frame: bubbles and the chat.
    pub said: Vec<Said>,
    pub paper: Paper,
    pub scrapbook: Scrapbook,
    /// The hero's press cuttings, oldest first (kept in the hero file).
    pub cuttings: Vec<Edition>,
    /// The newest edition, for the newsboys.
    pub latest: Option<Edition>,
    heard: Vec<Heard>,
    incidents: BTreeSet<u32>,
    cases: BTreeSet<u16>,
    story_done: Option<u8>,
    /// Somebody cheered or clapped this frame (for the sound), and where.
    pub cheered: Option<(Vec3, Greeting)>,
}

impl FameView {
    /// Another hero is played (or the same one again): their cuttings,
    /// and nothing remembered of the last one's day.
    pub fn load(&mut self, cuttings: Vec<Edition>) {
        self.latest = cuttings.last().cloned();
        self.cuttings = cuttings;
        self.fame = None;
        self.heard.clear();
        self.incidents.clear();
        self.cases.clear();
        self.story_done = None;
        self.signing = None;
        self.paper = Paper::default();
    }

    /// The looks our fans are dressed after, if they are asked for.
    pub fn fan_key(&self) -> Option<u64> {
        self.fan_key
    }

    /// Our hero is signing an autograph book, and the yaw (degrees) to
    /// the one who holds it.
    pub fn signing(&self, hero: Vec3) -> Option<f32> {
        let s = self.signing?;
        let d = s.at - hero;
        Some(d.x.atan2(d.z).to_degrees())
    }

    /// News from the server: deeds worth a headline are noted (`hero`:
    /// where ours stands). Takes nothing away from the other views.
    pub fn hear(&mut self, m: &ToClient, hero: Option<Vec3>) {
        match m {
            ToClient::Incident {
                id,
                kind,
                at,
                state: 1,
                ..
            } => {
                let at = Vec3::from_array(*at);
                let there = hero.is_some_and(|h| h.distance(at) < THERE);
                let kind = IncidentKind::from_code(*kind);
                if let (true, Some(kind), true) = (there, kind, self.incidents.insert(*id)) {
                    self.heard.push(Heard {
                        deed: Deed::Incident(kind),
                        at: Some(at),
                    });
                }
            }
            ToClient::Crime { state: 2, .. } => self.heard.push(Heard {
                deed: Deed::StreetCrime,
                at: hero,
            }),
            ToClient::Case {
                number,
                stage: 3,
                gang,
                at,
                ..
            } => {
                if self.cases.insert(*number) {
                    let gangs = &hw_sim::missions::GANGS;
                    let gang = gangs[(*gang as usize).min(gangs.len() - 1)].name;
                    self.heard.push(Heard {
                        deed: Deed::Case(gang.to_string()),
                        at: Some(Vec3::from_array(*at)),
                    });
                }
            }
            ToClient::Story { done, .. } => {
                if self.story_done.is_some_and(|d| *done > d) {
                    self.heard.push(Heard {
                        deed: Deed::Chapter(*done, hw_sim::story::CHAPTERS.len() as u8),
                        at: None,
                    });
                }
                self.story_done = Some(*done);
            }
            _ => {}
        }
    }

    /// The heroes the townsfolk may stop for, from this frame's entities.
    pub fn stars(&self, heroes: &[EntityState], mine: Option<NetId>) -> Vec<Star> {
        heroes
            .iter()
            .filter_map(|h| {
                let fame = match h.kind {
                    EntityKind::Hero if Some(h.id) == mine => self.fame?,
                    // A Herobot is nobody's hero yet; a friend is as well
                    // known as their level says.
                    EntityKind::Hero if h.mount.is_some_and(|m| m.1 >= hw_net::HEROBOT_MOUNT) => {
                        return None
                    }
                    EntityKind::Hero => Fame::of_level(h.combat.map_or(1, |c| c.level)),
                    EntityKind::NpcHero => NPC_FAME,
                    _ => return None,
                };
                // Down, or in the air: nobody to stop for.
                let up = h
                    .combat
                    .is_none_or(|c| c.status == hw_net::Status::Standing);
                (up && h.grounded && fame > Fame::Nobody).then(|| Star {
                    id: h.id.0,
                    at: glam::DVec3::from_array(h.pos),
                    speed: Vec3::from_array(h.vel).length() as f64,
                    fame,
                })
            })
            .collect()
    }

    /// Every frame, after the crowd moved: our hero's fame from their
    /// `sheet`, the fans' looks, the autograph book, what people say, and
    /// the paper. `me`: our hero's number, feet and speed. `time` is the
    /// town's clock.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        world: &mut World,
        crowd: &mut CrowdView,
        people: Option<&People>,
        look: Option<&CharacterConfig>,
        sheet: &Sheet,
        name: &str,
        area: &Area,
        me: Option<(NetId, Vec3, f32)>,
        time: f64,
    ) -> EngineResult<()> {
        self.said.clear();
        self.cheered = None;
        // Fame: a new step is news.
        let now = Fame::of(sheet);
        if self.fame.is_some_and(|before| now > before) {
            self.heard.push(Heard {
                deed: Deed::Fame(now),
                at: None,
            });
        }
        self.fame = Some(now);
        // The fans' looks follow our hero's: asked for once per look.
        let want = (now.fans() > 0.0)
            .then_some(look)
            .flatten()
            .filter(|_| people.is_some())
            .map(crate::looks::key);
        if want != self.fan_key {
            crowd.drop_fan_looks(world);
            self.fan_key = want;
            if let (Some(key), Some(look), Some(people)) = (want, look, people) {
                for k in 0..FAN_LOOKS {
                    people.request_fan(key, k, look.clone());
                }
            }
        }
        self.autographs(world, crowd, me, look, time)?;
        // What those who stopped say.
        let mine = me.map(|m| m.0 .0);
        for g in std::mem::take(&mut crowd.greeted) {
            if matches!(g.how, Greeting::Clap | Greeting::Cheer) && !g.signed {
                self.cheered = Some((g.at.as_vec3(), g.how));
            }
            // They speak to our own hero; a friend hears their own.
            if Some(g.star) != mine {
                continue;
            }
            let always = g.how == Greeting::Autograph;
            if !always && time < self.talk_next {
                continue;
            }
            let Some((text, shout)) = line(&g, now, name) else {
                continue;
            };
            self.talk_next = time + TALK_GAP;
            self.said.push(Said {
                at: g.at.as_vec3(),
                range: if shout { SHOUT_RANGE } else { SAY_RANGE },
                who: if g.fan { "Fan" } else { "Passer-by" }.to_string(),
                text,
                voice: Voice::Folk,
            });
        }
        // The paper: deeds heard since the last frame.
        for h in std::mem::take(&mut self.heard) {
            let place = h.at.map_or(String::new(), |p| {
                hw_sim::missions::sites::street_near(area, DVec2::new(p.x as f64, p.z as f64))
            });
            let number = self.cuttings.last().map_or(0, |e| e.number) + 1;
            // A hero the town does not know makes no headlines: who would
            // they be about?
            if now == Fame::Nobody && !matches!(h.deed, Deed::Chapter(..)) {
                continue;
            }
            let k = hw_core::Seed(number as u64).child(name).0;
            let e = headlines::write(&h.deed, now, name, &place, k, number);
            self.cuttings.push(e.clone());
            let extra = self.cuttings.len().saturating_sub(SCRAPBOOK_KEEP);
            self.cuttings.drain(..extra);
            self.latest = Some(e.clone());
            self.paper.print(e);
        }
        Ok(())
    }

    /// The one with the autograph book, the book in their hand, and our
    /// hero signing it when they stand still beside them.
    fn autographs(
        &mut self,
        world: &mut World,
        crowd: &mut CrowdView,
        me: Option<(NetId, Vec3, f32)>,
        look: Option<&CharacterConfig>,
        time: f64,
    ) -> EngineResult<()> {
        let seeker = me.and_then(|(id, ..)| crowd.autograph_seeker(world, id.0));
        self.seeker = seeker.map(|(_, at)| at + Vec3::Y * 1.95);
        if let Some(s) = self.signing {
            let gone =
                me.is_none_or(|(_, at, _)| at.distance(s.at) > fame::SIGN_REACH as f32 + 1.0);
            if time >= s.until {
                crowd.signed(s.citizen, time);
                self.signing = None;
            } else if gone || seeker.is_none_or(|(c, _)| c != s.citizen) {
                self.signing = None;
            }
        } else if let (Some((citizen, at)), Some((_, hero, speed))) = (seeker, me) {
            let near = hero.distance(at) < fame::SIGN_REACH as f32;
            if near && (speed as f64) < fame::SIGN_STILL {
                self.signing = Some(Signing {
                    citizen,
                    until: time + fame::SIGN_SECONDS,
                    at,
                });
            }
        }
        // The book: in the seeker's left hand (held out flat), or in our
        // hero's while they sign.
        let held = match (seeker, self.signing, me) {
            (Some((citizen, at)), None, Some((_, hero, _))) => {
                let to = (hero - at).normalize_or(Vec3::Z);
                let hand = crowd
                    .bone_of(world, citizen, "hand_l")
                    .unwrap_or(at + Vec3::Y * 1.15 + to * 0.38);
                Some((hand + to * 0.07 + Vec3::Y * 0.03, to))
            }
            (_, Some(s), Some((_, hero, _))) => {
                let to = (s.at - hero).normalize_or(Vec3::Z);
                Some((hero + Vec3::Y * 1.12 + to * 0.34, to))
            }
            _ => None,
        };
        match held {
            Some((at, to)) => {
                let place =
                    Place::new(at.x, at.y, at.z).with_yaw_deg(to.x.atan2(to.z).to_degrees());
                match self.book {
                    Some(e) => world.set_place(e, place)?,
                    None => {
                        let cover = look.map_or(rgb(170, 40, 40), |l| {
                            hex(&crate::people::hero_colours(l).0).unwrap_or(rgb(170, 40, 40))
                        });
                        let e = world.spawn(book(cover)?);
                        world.set_place(e, place)?;
                        self.book = Some(e);
                    }
                }
            }
            None => {
                if let Some(e) = self.book.take() {
                    world.despawn(e);
                }
            }
        }
        Ok(())
    }

    /// The marks on screen: a pencil badge over the one who waits with a
    /// book (it fills as our hero signs), the front page, the scrapbook.
    /// `quiet`: hold a new front page back (a fight is on).
    #[allow(clippy::too_many_arguments)]
    pub fn hud(
        &mut self,
        ctx: &egui::Context,
        view_proj: Mat4,
        sheet: &Sheet,
        time: f64,
        dt: f32,
        quiet: bool,
    ) {
        if let Some(head) = self.seeker {
            if let Some(p) = project(view_proj, ctx.screen_rect(), head) {
                let painter = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Background,
                    egui::Id::new("autograph mark"),
                ));
                let done = self.signing.map_or(0.0, |s| {
                    (1.0 - (s.until - time) / fame::SIGN_SECONDS).clamp(0.0, 1.0) as f32
                });
                let t = crate::uikit::now(ctx);
                let bob = if crate::uikit::reduced_motion(ctx) {
                    0.0
                } else {
                    3.0 * (t * 3.0).sin()
                };
                pencil_badge(&painter, p + egui::vec2(0.0, bob), 15.0, done);
            }
        }
        self.paper.show(ctx, dt, quiet || self.scrapbook.open);
        let fame = self.fame.unwrap_or(Fame::Nobody);
        let level = hw_sim::progress::level_of(sheet.lifetime);
        self.scrapbook.show(ctx, fame, level, &self.cuttings);
    }

    /// Front pages for screenshots (`HW_HEADLINE=1`): the game does not
    /// save them.
    pub fn demo_headline(&mut self, name: &str) {
        let fame = self.fame.unwrap_or(Fame::Nobody).max(Fame::Local);
        let deed = Deed::Incident(hw_sim::incidents::IncidentKind::Robbery);
        // A few older pages for the scrapbook.
        use hw_sim::incidents::IncidentKind as K;
        let old = [
            Deed::Fame(Fame::Noticed),
            Deed::StreetCrime,
            Deed::Incident(K::Fire),
            Deed::Fame(Fame::Local),
            Deed::Incident(K::Rampage),
            Deed::Chapter(2, 7),
            Deed::Incident(K::CarCrash),
        ];
        for (i, d) in old.iter().enumerate() {
            let f = if i < 3 { Fame::Noticed } else { fame };
            let n = 34 + i as u32;
            self.cuttings
                .push(headlines::write(d, f, name, "Rocket Row", n as u64, n));
        }
        let e = headlines::write(&deed, fame, name, "Comet Avenue", 3, 41);
        self.cuttings.push(e.clone());
        self.latest = Some(e.clone());
        self.paper.print(e);
    }

    /// Takes everything off screen (a new town, back to the title).
    pub fn clear(&mut self, world: &mut World, crowd: &mut CrowdView) {
        if let Some(e) = self.book.take() {
            world.despawn(e);
        }
        crowd.drop_fan_looks(world);
        self.fan_key = None;
        self.signing = None;
        self.seeker = None;
        self.heard.clear();
        self.said.clear();
    }
}

/// A point of the world on screen, if it is in front of the camera.
fn project(view_proj: Mat4, screen: egui::Rect, p: Vec3) -> Option<egui::Pos2> {
    let c = view_proj * p.extend(1.0);
    if c.w <= 0.05 {
        return None;
    }
    let n = c.truncate() / c.w;
    (n.x.abs() < 1.1 && n.y.abs() < 1.1).then(|| {
        egui::pos2(
            screen.left() + (n.x * 0.5 + 0.5) * screen.width(),
            screen.top() + (0.5 - n.y * 0.5) * screen.height(),
        )
    })
}

/// "Sign here": a gold disc with a pencil, ringed by how far the signing
/// is (0 to 1).
fn pencil_badge(painter: &egui::Painter, c: egui::Pos2, r: f32, done: f32) {
    use crate::uikit::{CREAM, DEEP, GOLD, HERO_RED};
    use egui::{vec2, Shape, Stroke};
    painter.circle_filled(c + vec2(1.5, 2.0), r, egui::Color32::from_black_alpha(130));
    painter.circle_filled(c, r, DEEP);
    painter.circle_stroke(c, r, Stroke::new(2.0_f32, GOLD));
    // The pencil, point down to the left.
    let dir = vec2(-0.64, 0.77);
    let side = vec2(dir.y, -dir.x);
    let (tip, top) = (c + dir * r * 0.62, c - dir * r * 0.6);
    let w = r * 0.17;
    let neck = tip - dir * r * 0.32;
    painter.add(Shape::convex_polygon(
        vec![
            neck + side * w,
            top + side * w,
            top - side * w,
            neck - side * w,
        ],
        GOLD,
        Stroke::NONE,
    ));
    painter.add(Shape::convex_polygon(
        vec![tip, neck + side * w, neck - side * w],
        CREAM,
        Stroke::NONE,
    ));
    let rubber = top + dir * r * 0.2;
    painter.add(Shape::convex_polygon(
        vec![
            rubber + side * w,
            top + side * w,
            top - side * w,
            rubber - side * w,
        ],
        HERO_RED,
        Stroke::NONE,
    ));
    if done > 0.0 {
        let n = (40.0 * done).ceil() as usize;
        let pts: Vec<egui::Pos2> = (0..=n)
            .map(|k| {
                let a = -std::f32::consts::FRAC_PI_2
                    + std::f32::consts::TAU * done * k as f32 / n.max(1) as f32;
                c + vec2(a.cos(), a.sin()) * (r + 4.0)
            })
            .collect();
        painter.add(Shape::line(pts, Stroke::new(3.5_f32, CREAM)));
    }
}

/// An autograph book: a cloth cover a little bigger than its block of
/// pages, lying flat (its long side along +Z).
fn book(cover: Color) -> EngineResult<Mesh> {
    let paper = rgb(244, 238, 220);
    let mut m = Shape::box_at((0.0, 0.0, 0.0), (BOOK.x, BOOK.y, BOOK.z), cover)?;
    let pages = Shape::box_at(
        (0.006, 0.0, 0.0),
        (BOOK.x - 0.006, BOOK.y * 0.72, BOOK.z - 0.012),
        paper,
    )?;
    m.append_translated(&pages, Vec3::ZERO)?;
    // The open page on top, where the name goes.
    let page = Shape::box_at(
        (0.0, BOOK.y * 0.5 + 0.001, 0.0),
        (BOOK.x - 0.02, 0.002, BOOK.z - 0.02),
        paper,
    )?;
    m.append_translated(&page, Vec3::ZERO)?;
    Ok(m)
}

/// A colour from "#rrggbb".
fn hex(s: &str) -> Option<Color> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some(rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// What someone who stopped says, and whether they shout it; `None` when
/// they only wave. Named from "local hero" on.
fn line(g: &Greeted, fame: Fame, name: &str) -> Option<(String, bool)> {
    let named = fame.named_in_print() && !name.trim().is_empty();
    let k = hw_core::Seed(g.citizen).child(g.how as u64).0;
    let pick = |list: &[&str]| list[(k >> 8) as usize % list.len()].replace("{N}", name.trim());
    // Not everyone has something to say.
    let speaks = |share: f64| g.fan || hw_core::Seed(k).child("speaks").unit() < share;
    if g.signed {
        return Some((
            pick(if named {
                &[
                    "Golly, thanks, {N}!",
                    "Thank you! I'll never wash this book!",
                    "Wait till the others see this!",
                    "\"To my biggest fan.\" Oh, {N}!",
                ]
            } else {
                &["Thank you! Thank you!", "Wait till the others see this!"]
            }),
            false,
        ));
    }
    if g.fan && g.how != Greeting::Autograph && k.is_multiple_of(3) {
        return Some((
            pick(&[
                "I made the cape myself!",
                "I've got your emblem on my lunch box!",
                "Look, I'm wearing your colours!",
                "I cut out every story about you!",
            ]),
            false,
        ));
    }
    match g.how {
        Greeting::Wave => speaks(0.4).then(|| {
            (
                pick(if named {
                    &[
                        "It's {N}!",
                        "Hello, {N}!",
                        "{N}! Over here!",
                        "Keep it up, {N}!",
                        "Afternoon, {N}!",
                    ]
                } else {
                    &[
                        "Hey, aren't you that hero?",
                        "Look, it's the one from the paper!",
                        "Nice work the other day!",
                        "I know you! You're... don't tell me...",
                    ]
                }),
                false,
            )
        }),
        Greeting::Clap => speaks(0.45).then(|| {
            (
                pick(if named {
                    &[
                        "Well done, {N}!",
                        "Bravo, {N}!",
                        "This town is lucky to have you, {N}!",
                        "Good show!",
                    ]
                } else {
                    &["Bravo!", "Well done, whoever you are!", "Good show!"]
                }),
                false,
            )
        }),
        Greeting::Cheer => speaks(0.6).then(|| {
            (
                pick(if named {
                    &[
                        "HOORAY FOR {N}!",
                        "{N}! {N}! {N}!",
                        "WE LOVE YOU, {N}!",
                        "THREE CHEERS FOR {N}!",
                    ]
                } else {
                    &["HOORAY!", "THREE CHEERS!"]
                })
                .to_uppercase(),
                true,
            )
        }),
        Greeting::Autograph => Some((
            pick(if named {
                &[
                    "{N}! Would you sign my book? Please?",
                    "An autograph, {N}? Oh, please!",
                    "My family will never believe this. Sign here, {N}?",
                    "Just your name, {N}! Right here!",
                ]
            } else {
                &["Would you sign my book? Please?"]
            }),
            false,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hero(id: u32, level: u8) -> EntityState {
        EntityState {
            id: NetId(id),
            kind: EntityKind::Hero,
            grounded: true,
            combat: Some(hw_net::CombatState {
                level,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn nobody_stops_for_a_new_hero_and_friends_count_by_their_level() {
        let mut f = FameView::default();
        let heroes = [hero(1, 1), hero(2, 12)];
        // Our own fame is not known yet: only the friend is a star.
        let stars = f.stars(&heroes, Some(NetId(1)));
        assert_eq!(stars.len(), 1);
        assert_eq!(stars[0].fame, Fame::Favourite);
        f.fame = Some(Fame::Nobody);
        assert_eq!(f.stars(&heroes, Some(NetId(1))).len(), 1);
        f.fame = Some(Fame::Legend);
        assert_eq!(f.stars(&heroes, Some(NetId(1))).len(), 2);
        // A hero in the air is nobody to stop for.
        let mut flying = hero(1, 20);
        flying.grounded = false;
        assert!(f.stars(&[flying], Some(NetId(1))).is_empty());
    }

    #[test]
    fn a_deed_far_away_or_heard_twice_makes_no_headline() {
        let mut f = FameView::default();
        let news = |id: u32, state: u8| ToClient::Incident {
            id,
            kind: IncidentKind::Robbery.code(),
            at: [10.0, 0.0, 0.0],
            level: 1,
            state,
            yaw: 0,
            left: 0,
            held: 0,
        };
        f.hear(&news(1, 0), Some(Vec3::ZERO));
        assert!(f.heard.is_empty(), "an open call is no deed");
        f.hear(&news(1, 1), Some(Vec3::new(500.0, 0.0, 0.0)));
        assert!(f.heard.is_empty(), "someone else's deed");
        f.hear(&news(2, 1), Some(Vec3::ZERO));
        f.hear(&news(2, 1), Some(Vec3::ZERO));
        assert_eq!(f.heard.len(), 1);
        // The story's first news is where it stands, not a deed.
        let story = |done: u8| ToClient::Story {
            done,
            stage: 0,
            at: [0.0; 3],
            level: 1,
            held: 0,
        };
        f.hear(&story(2), None);
        assert_eq!(f.heard.len(), 1);
        f.hear(&story(3), None);
        assert_eq!(f.heard.len(), 2);
    }

    #[test]
    fn people_name_a_local_hero_and_not_a_stranger() {
        let g = |citizen: u64, how: Greeting| Greeted {
            citizen,
            star: 1,
            at: glam::DVec3::ZERO,
            how,
            fan: false,
            signed: false,
        };
        let mut named = 0;
        for id in 0..400 {
            for how in [Greeting::Wave, Greeting::Clap, Greeting::Autograph] {
                if let Some((t, _)) = line(&g(id, how), Fame::Noticed, "Nova") {
                    assert!(!t.contains("Nova") && !t.contains("{N}"), "{t}");
                }
                if let Some((t, _)) = line(&g(id, how), Fame::Favourite, "Nova") {
                    assert!(!t.contains("{N}"), "{t}");
                    named += t.contains("Nova") as u32;
                }
            }
            // Someone with a book always asks.
            assert!(line(&g(id, Greeting::Autograph), Fame::Favourite, "Nova").is_some());
        }
        assert!(named > 200, "{named}");
    }
}
