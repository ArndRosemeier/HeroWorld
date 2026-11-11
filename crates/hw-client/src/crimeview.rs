//! Street crime on screen (the plan is `hw_sim::streetlife::crime`): a
//! greaser snatching a lady's handbag and running, two hassling a passer-by,
//! a shopkeeper shaken down at his door, the Syndicate's gunsels collecting
//! protection money, a gang spraying its sign on a wall while one keeps a
//! lookout, brutes smashing a lamp post. Each person is an Actors
//! character with clips made for it (`poses::add_menace_clips`,
//! `poses::add_victim_clips`); what they carry off is drawn in their hand.
//!
//! The crimes are a pure function of the town and the clock, the same on
//! the server, which turns the crooks into real villains, where they are
//! drawn and in the same looks, when a hero steps in (then it tells us,
//! and we stop drawing them). Which crimes are near the camera is worked
//! out on a worker thread; their people (a few dozen at most) are placed
//! here each frame.

use crate::chat::{Said, Voice, SAY_RANGE, SHOUT_RANGE};
use crate::people::{People, VillainKind, Who, JOG_MPS, SPRINT_MPS, WALK_MPS};
use crate::streetgear::Gear;
use engine::prelude::*;
use glam::{DVec2, Vec3, Vec3Swizzles};
use hw_core::Seed;
use hw_net::protocol::loot;
use hw_sim::streetlife::crime::{Crime, CrimeKind, CrimePerson, CrimeRole, Deed, Loot, Tag};
use hw_sim::streetlife::StreetLife;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

/// Crimes are looked for within this distance of the camera...
const REACH: f64 = 110.0;
/// ...and at most this many are shown, the nearest first.
const MOST: usize = 8;
/// Their people are drawn within this distance of the camera.
const PEOPLE_REACH: f64 = 80.0;
/// Gang signs are drawn within this distance of the camera...
const TAG_REACH: f64 = 140.0;
/// ...at most this many.
const TAG_MOST: usize = 24;
/// A hero this near hears what is said.
const HEAR: f64 = 16.0;

/// One person on screen.
struct Person {
    e: EntityId,
    who: Who,
    clip: &'static str,
    rate: f32,
}

/// A sign on a wall: its decal, and how far sprayed it was drawn (eighths).
struct Sign {
    decal: DecalId,
    step: u8,
}

/// A request to the worker: the crimes near `at` at `time`.
struct Ask {
    at: DVec2,
    time: f64,
}

/// The worker's answer.
struct Answer {
    crimes: Vec<Arc<Crime>>,
    tags: Vec<Tag>,
}

pub struct CrimeView {
    life: Arc<StreetLife>,
    ask: Option<Sender<Ask>>,
    answers: Option<Receiver<Answer>>,
    waiting: bool,
    refresh: f64,
    crimes: Vec<Arc<Crime>>,
    tags: Vec<Tag>,
    shown: BTreeMap<u64, Person>,
    asked: BTreeSet<Who>,
    signs: BTreeMap<(u64, i64), Sign>,
    /// Crimes the server says are taken over (a hero stepped in) or over,
    /// and the game time it said so: their people are not drawn, and a
    /// half-sprayed sign stays as it was.
    taken: BTreeMap<(u64, i64), f64>,
    /// When each crime last said something, and how many lines.
    talk: BTreeMap<(u64, i64), (f64, u64)>,
    /// What was said this frame: for the floating words and the chat.
    pub said: Vec<Said>,
}

impl CrimeView {
    pub fn new(life: Arc<StreetLife>) -> Self {
        Self {
            life,
            ask: None,
            answers: None,
            waiting: false,
            refresh: 0.0,
            crimes: Vec::new(),
            tags: Vec::new(),
            shown: BTreeMap::new(),
            asked: BTreeSet::new(),
            signs: BTreeMap::new(),
            taken: BTreeMap::new(),
            talk: BTreeMap::new(),
            said: Vec::new(),
        }
    }

    /// How many crimes and people are on screen.
    pub fn counts(&self) -> (usize, usize) {
        (self.crimes.len(), self.shown.len())
    }

    /// The crimes near the camera now (for `HW_VIEW=crime`).
    pub fn crimes(&self) -> &[Arc<Crime>] {
        &self.crimes
    }

    /// The street life the crimes come from.
    pub fn life(&self) -> &Arc<StreetLife> {
        &self.life
    }

    /// News from the server: a crime stepped in on (1), stopped (2) or
    /// over (3). Returns whether it was crime news.
    pub fn hear(&mut self, m: &hw_net::ToClient, time: f64) -> bool {
        let hw_net::ToClient::Crime { place, slot, state } = m else {
            return false;
        };
        if *state >= 1 {
            self.taken.entry((*place, *slot)).or_insert(time);
        }
        true
    }

    /// Moves the crimes near `camera` to `time`; their loot, and the loot
    /// real villains carry off (`carried`: their bodies and
    /// [`hw_net::protocol::loot`] codes), goes into `places`.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        world: &mut World,
        models: &BTreeMap<Who, Arc<AnimatedModel>>,
        people: Option<&People>,
        camera: DVec2,
        hero: Option<Vec3>,
        carried: &[(EntityId, u16)],
        time: f64,
        dt: f64,
        places: &mut BTreeMap<Gear, Vec<Place>>,
    ) -> EngineResult<()> {
        self.ask_worker(camera, time, dt)?;
        let hero2 = hero.map(|h| DVec2::new(h.x as f64, h.z as f64));
        let mut seen = BTreeSet::new();
        for crime in self.crimes.clone() {
            let key = crime.key();
            if self.taken.contains_key(&key) || !crime.active(time) {
                continue;
            }
            if crime.place.at.distance(camera) > PEOPLE_REACH + 20.0 {
                continue;
            }
            let folk = crime.people(&self.life, time);
            for p in &folk {
                if p.pos.xz().distance(camera) > PEOPLE_REACH {
                    continue;
                }
                let Some(who) = who_of(p.role) else {
                    continue;
                };
                let Some(model) = models.get(&who) else {
                    // The citizens' looks are all asked for at the start
                    // (they become crowds that must not be made twice):
                    // wait for them. Villains on demand.
                    if matches!(who, Who::Villain(..)) && self.asked.insert(who.clone()) {
                        if let Some(people) = people {
                            people.request(who, 0);
                        }
                    }
                    continue;
                };
                let clip = clip_of(p);
                let rate = rate_of(clip, p.speed);
                let yaw = p.facing.x.atan2(p.facing.y).to_degrees() as f32;
                let place =
                    Place::new(p.pos.x as f32, p.pos.y as f32, p.pos.z as f32).with_yaw_deg(yaw);
                let person = match self.shown.get_mut(&p.id) {
                    Some(s) if s.who == who => {
                        world.set_place(s.e, place)?;
                        s
                    }
                    _ => {
                        if let Some(old) = self.shown.remove(&p.id) {
                            world.despawn(old.e);
                        }
                        let e = world.spawn_animated_shared(model.clone(), place)?;
                        world.set_animation_blend(e, 0.25)?;
                        self.shown.entry(p.id).or_insert(Person {
                            e,
                            who,
                            clip: "",
                            rate: 0.0,
                        })
                    }
                };
                if person.clip != clip {
                    if let Err(err) = world.play_animation(person.e, clip) {
                        eprintln!("crime {:?}: {err}", person.who);
                    }
                    let on_ground = matches!(clip, "idle" | "walk" | "jog" | "sprint");
                    let _ = world.set_foot_ik(person.e, on_ground.then(FootIk::default));
                    person.clip = clip;
                }
                if (person.rate - rate).abs() > 0.02 {
                    world.set_animation_speed(person.e, rate)?;
                    person.rate = rate;
                }
                seen.insert(p.id);
                if let Some(l) = p.holding {
                    hold(world, person.e, gear_of(l), p.pos.as_vec3(), places);
                }
            }
            self.speak(&crime, &folk, hero2, time);
        }
        let gone: Vec<u64> = self
            .shown
            .keys()
            .filter(|id| !seen.contains(id))
            .copied()
            .collect();
        for id in gone {
            if let Some(p) = self.shown.remove(&id) {
                world.despawn(p.e);
            }
        }
        // Real villains making off with something.
        for &(e, what) in carried {
            if let Some(g) = loot_gear(what) {
                let feet = world
                    .world_transform(e)
                    .map(|m| m.w_axis.truncate())
                    .unwrap_or(Vec3::ZERO);
                hold(world, e, g, feet, places);
            }
        }
        self.signs(world, camera, time);
        // Forget crimes long gone.
        let keys: BTreeSet<(u64, i64)> = self.crimes.iter().map(|c| c.key()).collect();
        self.talk.retain(|k, _| keys.contains(k));
        let back = hw_sim::streetlife::crime::TAG_LIFE + 2.0 * hw_sim::streetlife::crime::SLOT;
        self.taken.retain(|_, t| time - *t < back);
        Ok(())
    }

    /// Takes everything off screen (a new town, a new square).
    pub fn clear(&mut self, world: &mut World) {
        for (_, p) in std::mem::take(&mut self.shown) {
            world.despawn(p.e);
        }
        for (_, s) in std::mem::take(&mut self.signs) {
            world.remove_decal(s.decal);
        }
        self.crimes.clear();
        self.tags.clear();
        self.talk.clear();
        self.refresh = 0.0;
    }

    /// Gang signs near the camera, growing while they are sprayed. One a
    /// hero interrupted stays as far as it got.
    fn signs(&mut self, world: &mut World, camera: DVec2, time: f64) {
        let Some(kinds) = crate::edgeview::kinds(world) else {
            return;
        };
        let mut want: Vec<(Tag, u8)> = Vec::new();
        for t in &self.tags {
            let upto = self.taken.get(&t.key).map_or(time, |&s| s.min(time));
            let step = (t.done(upto) * 8.0).ceil() as u8;
            if step > 0 && DVec2::new(t.at.x, t.at.z).distance(camera) < TAG_REACH {
                want.push((*t, step));
            }
        }
        want.sort_by(|a, b| {
            let d = |t: &Tag| DVec2::new(t.at.x, t.at.z).distance(camera);
            d(&a.0).total_cmp(&d(&b.0))
        });
        want.truncate(TAG_MOST);
        let keep: BTreeSet<(u64, i64)> = want.iter().map(|w| w.0.key).collect();
        let stale: Vec<(u64, i64)> = self
            .signs
            .keys()
            .filter(|k| !keep.contains(k))
            .copied()
            .collect();
        for k in stale {
            if let Some(s) = self.signs.remove(&k) {
                world.remove_decal(s.decal);
            }
        }
        for (t, step) in want {
            if self.signs.get(&t.key).is_some_and(|s| s.step == step) {
                continue;
            }
            if let Some(old) = self.signs.remove(&t.key) {
                world.remove_decal(old.decal);
            }
            let n = Vec3::new(t.normal.x as f32, 0.0, t.normal.y as f32);
            let image = kinds[t.image as usize % kinds.len()];
            // Sprayed from the left: its width grows, its paint thickens.
            let k = step as f32 / 8.0;
            let d = Decal::new(image, t.at.as_vec3(), n, t.size as f32 * (0.35 + 0.65 * k))
                .depth(0.6)
                .angle((t.key.0 % 7) as f32 * 0.05 - 0.15)
                .opacity(0.35 + 0.57 * k);
            if let Ok(decal) = world.spawn_decal(d) {
                self.signs.insert(t.key, Sign { decal, step });
            }
        }
    }

    /// Asks the worker for the crimes near the camera twice a second, and
    /// takes its answer in.
    fn ask_worker(&mut self, camera: DVec2, time: f64, dt: f64) -> EngineResult<()> {
        if self.ask.is_none() {
            let (ask, asks) = channel::<Ask>();
            let (tell, answers) = channel();
            let life = self.life.clone();
            std::thread::Builder::new()
                .name("hw-crime".into())
                .spawn(move || {
                    while let Ok(mut a) = asks.recv() {
                        // Only the newest question matters.
                        while let Ok(newer) = asks.try_recv() {
                            a = newer;
                        }
                        let mut crimes = life.crimes_near(a.at, REACH, a.time, 2.0);
                        crimes.truncate(MOST);
                        let tags = life.tags_near(a.at, TAG_REACH, a.time);
                        if tell.send(Answer { crimes, tags }).is_err() {
                            return;
                        }
                    }
                })
                .map_err(|e| EngineError::InvalidValue(e.to_string()))?;
            self.ask = Some(ask);
            self.answers = Some(answers);
        }
        if let Some(r) = &self.answers {
            while let Ok(a) = r.try_recv() {
                self.crimes = a.crimes;
                self.tags = a.tags;
                self.waiting = false;
            }
        }
        self.refresh -= dt;
        if self.refresh <= 0.0 && !self.waiting {
            self.refresh = 0.5;
            if let Some(ask) = &self.ask {
                self.waiting = ask.send(Ask { at: camera, time }).is_ok();
            }
        }
        self.crimes.retain(|c| time < c.gone);
        Ok(())
    }

    /// What the crooks and their victim say, to a hero close enough to
    /// hear: a line as it starts, and now and then while it goes on.
    fn speak(&mut self, crime: &Crime, folk: &[CrimePerson], hero: Option<DVec2>, time: f64) {
        let Some(hero) = hero else {
            return;
        };
        if crime.place.at.distance(hero) > HEAR || time < crime.on {
            return;
        }
        let (next, count) = self.talk.entry(crime.key()).or_insert((0.0, 0));
        if time < *next {
            return;
        }
        *count += 1;
        let n = *count;
        *next = time + 6.0 + (Seed(crime.seed()).child(n).0 % 5) as f64;
        // Crooks and victim take turns.
        let victim = folk.iter().find(|p| matches!(p.role, CrimeRole::Victim(_)));
        let crook = folk.iter().find(|p| matches!(p.role, CrimeRole::Crook(..)));
        let (who, lines, voice, title) = match (n % 2 == 0, victim, crook) {
            (true, Some(v), _) => (
                v,
                victim_lines(crime.kind, v.deed),
                Voice::Folk,
                "Passer-by",
            ),
            (_, _, Some(c)) => (
                c,
                crook_lines(crime.kind, crime.done <= time),
                Voice::Villain,
                crook_title(crime.kind),
            ),
            _ => return,
        };
        if lines.is_empty() {
            return;
        }
        let text = lines[(Seed(crime.seed()).child(n + 100).0 % lines.len() as u64) as usize];
        let shout = matches!(who.deed, Deed::Shouting | Deed::Smashing | Deed::Laughing);
        self.said.push(Said {
            at: who.pos.as_vec3() + Vec3::Y * 0.2,
            range: if shout { SHOUT_RANGE } else { SAY_RANGE },
            who: title.to_string(),
            text: text.to_string(),
            voice,
        });
    }
}

/// Which built character someone is.
fn who_of(role: CrimeRole) -> Option<Who> {
    match role {
        CrimeRole::Crook(kind, look, _) => {
            let v = VillainKind::of(kind.entity_kind())?;
            Some(Who::Villain(v, look as u32 % v.looks()))
        }
        CrimeRole::Victim(look) => Some(Who::Citizen(look)),
    }
}

/// The clip someone plays.
fn clip_of(p: &CrimePerson) -> &'static str {
    let crook = matches!(p.role, CrimeRole::Crook(..));
    match p.deed {
        Deed::Walking => "walk",
        Deed::Hurrying if crook => "jog",
        Deed::Running if crook => "sprint",
        // Townsfolk have no run: a brisk walk.
        Deed::Hurrying | Deed::Running => "walk",
        Deed::Lounging if crook => "lounge",
        Deed::Watching if crook => "lookout",
        Deed::Lounging | Deed::Watching => "idle",
        Deed::Threatening => "threaten",
        Deed::Shoving => "shove",
        Deed::Grabbing => "grab",
        Deed::Laughing => "laugh",
        Deed::Counting => "count",
        Deed::Spraying => "spray",
        Deed::Smashing => "smash",
        Deed::Cowering => "hands up",
        Deed::Shouting => "shout",
        Deed::HandingOver => "interact",
        Deed::Shaken => "shaken",
    }
}

/// How fast a clip plays: walks and runs match the speed.
fn rate_of(clip: &str, speed: f64) -> f32 {
    let s = speed as f32;
    match clip {
        "walk" => (s / WALK_MPS).clamp(0.6, 2.2),
        "jog" => (s / JOG_MPS).clamp(0.6, 1.6),
        "sprint" => (s / SPRINT_MPS).clamp(0.6, 1.5),
        _ => 1.0,
    }
}

fn gear_of(l: Loot) -> Gear {
    match l {
        Loot::Handbag => Gear::Handbag,
        Loot::Cash => Gear::Cash,
        Loot::Envelope => Gear::Envelope,
        Loot::SprayCan => Gear::SprayCan,
    }
}

/// The thing a villain carries off ([`hw_net::protocol::loot`]).
fn loot_gear(what: u16) -> Option<Gear> {
    Some(match what {
        loot::HANDBAG => Gear::Handbag,
        loot::CASH => Gear::Cash,
        loot::ENVELOPE => Gear::Envelope,
        loot::SPRAY_CAN => Gear::SprayCan,
        loot::SACK => Gear::Sack,
        loot::CRATE => Gear::LootCrate,
        _ => return None,
    })
}

/// `g` in body `e`'s right hand (a crate in both, before the chest).
fn hold(world: &World, e: EntityId, g: Gear, feet: Vec3, places: &mut BTreeMap<Gear, Vec<Place>>) {
    let Ok(m) = world.world_transform(e) else {
        return;
    };
    let (_, rot, _) = m.to_scale_rotation_translation();
    let (yaw, _, _) = rot.to_euler(glam::EulerRot::YXZ);
    let fwd = rot * Vec3::Z;
    let at = if g == Gear::LootCrate {
        // Held in front of the chest.
        let chest = world
            .bone_transform(e, "spine_03")
            .map(|b| b.w_axis.truncate())
            .unwrap_or(feet + Vec3::Y * 1.2);
        chest + Vec3::new(fwd.x, 0.0, fwd.z).normalize_or_zero() * 0.38 - Vec3::Y * 0.42
    } else {
        match world.bone_transform(e, "hand_r") {
            Ok(h) => h.w_axis.truncate(),
            Err(_) => return,
        }
    };
    if at.distance(feet) > 2.6 {
        return;
    }
    let at = match g {
        // Wads and envelopes sit in the palm, a little out.
        Gear::Cash | Gear::Envelope | Gear::SprayCan => at + fwd * 0.04 - Vec3::Y * 0.04,
        _ => at,
    };
    places
        .entry(g)
        .or_default()
        .push(Place::new(at.x, at.y, at.z).with_yaw_deg(yaw.to_degrees()));
}

fn crook_title(kind: CrimeKind) -> &'static str {
    match kind {
        CrimeKind::Protection => "Syndicate gunsel",
        CrimeKind::Vandals => "Mutant brute",
        _ => "Chrome Jacket",
    }
}

/// What the crooks say, before the deed and after it.
fn crook_lines(kind: CrimeKind, done: bool) -> &'static [&'static str] {
    match (kind, done) {
        (CrimeKind::Snatch, false) => &["Nice purse, lady.", "Don't mind me, toots."],
        (CrimeKind::Snatch, true) => &["So long, sucker!", "Thanks for the donation!"],
        (CrimeKind::Hassle, false) => &[
            "Where you think you're goin', square?",
            "This is Jacket turf, daddy-o. Toll's a buck.",
            "Hey, nice hat. Shame if somethin' happened to it.",
        ],
        (CrimeKind::Hassle, true) => &["Go on, scram!", "Ha! Look at him run!"],
        (CrimeKind::Shakedown, false) => &[
            "Nice little shop. Real flammable, these old places.",
            "The Jackets look after this street. For a fee.",
            "Empty the till, pops. Nice and slow.",
        ],
        (CrimeKind::Shakedown, true) => &["Pleasure doin' business.", "Same time next week, pops."],
        (CrimeKind::Protection, false) => &[
            "Mr. Velvet sends his regards.",
            "Insurance, friend. Accidents happen in this town.",
            "The envelope. Don't make me ask twice.",
        ],
        (CrimeKind::Protection, true) => {
            &["A wise investment.", "Mr. Velvet appreciates your custom."]
        }
        (CrimeKind::Tagging, false) => &[
            "Make it big. Let 'em know whose street this is.",
            "Keep your eyes peeled for the fuzz.",
        ],
        (CrimeKind::Tagging, true) => &["Now that's art!", "Jackets rule, squares drool!"],
        (CrimeKind::Vandals, false) => &["SMASH!", "Brute hate lamp!", "Shiny thing go CRUNCH!"],
        (CrimeKind::Vandals, true) => &["Ha! Ha! Ha!", "More! MORE!"],
    }
}

/// What the victim says.
fn victim_lines(kind: CrimeKind, deed: Deed) -> &'static [&'static str] {
    match (kind, deed) {
        (_, Deed::Shouting) => &["Stop, thief!", "Help! Police! He took my bag!"],
        (CrimeKind::Shakedown | CrimeKind::Protection, Deed::HandingOver) => &[
            "Here, take it. Just leave us alone.",
            "That's all I made this week...",
        ],
        (CrimeKind::Shakedown | CrimeKind::Protection, _) => &[
            "Please, business has been slow...",
            "I already paid last week!",
        ],
        (_, Deed::Cowering) => &["P-please, I don't want any trouble!", "Leave me alone!"],
        (_, Deed::Shaken) => &[
            "Somebody ought to do something...",
            "Where's a hero when you need one?",
        ],
        _ => &[],
    }
}
