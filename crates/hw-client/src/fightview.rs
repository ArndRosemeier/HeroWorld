//! The fight as the player sees and plays it: Tab targeting nearest the
//! crosshair, the power keys, the target frame with its con colour, health
//! and energy bars, floating numbers, and the feel of every blow (sparks,
//! beams, scorch marks, sounds, camera shake, hit-stop). Knockback and
//! defeat turn characters into Engine ragdolls; defeated villains lie in
//! glowing energy cuffs until the police drone takes them away.
//!
//! The server decides everything; this only shows the entity states and
//! fight events it sends, and turns keys into input.

use engine::prelude::*;
use engine::ui::egui;
use glam::{Mat4, Vec3};
use hw_core::NetId;
use hw_net::{
    EntityKind, EntityState, Fx, FxKind, Power, Sheet, Status, EFFECT_HELD, EFFECT_SHIELD,
};
use hw_sim::combat::{power_spec, EnemyKind, Kit};
use hw_sim::progress::{is_movement, themes};
use std::collections::BTreeMap;

/// Villains further than this cannot be picked with Tab.
pub const TARGET_RANGE: f32 = 60.0;
/// Only villains within this angle of the crosshair are picked (degrees).
pub const TARGET_CONE: f32 = 55.0;
/// Fight events older than this (seconds) are not shown any more.
const LATE: f64 = 0.6;

/// The name and rank shown in the target frame.
pub fn enemy_name(kind: EntityKind) -> (&'static str, &'static str) {
    match EnemyKind::from_entity(kind) {
        Some(k) => {
            let s = hw_sim::combat::enemy_spec(k);
            (s.name, s.rank)
        }
        None => ("", ""),
    }
}

/// City of Heroes' con colours: an enemy's level against yours.
pub fn con_colour(theirs: u8, mine: u8) -> egui::Color32 {
    let d = theirs as i32 - mine as i32;
    match d {
        i32::MIN..=-4 => egui::Color32::from_rgb(150, 150, 150), // grey
        -3 | -2 => egui::Color32::from_rgb(90, 200, 90),         // green
        -1 => egui::Color32::from_rgb(90, 150, 255),             // blue
        0 => egui::Color32::from_rgb(240, 240, 240),             // white
        1 => egui::Color32::from_rgb(250, 220, 60),              // yellow
        2 => egui::Color32::from_rgb(250, 150, 40),              // orange
        3 => egui::Color32::from_rgb(235, 60, 50),               // red
        _ => egui::Color32::from_rgb(180, 80, 230),              // purple
    }
}

fn engine_colour(c: egui::Color32) -> Color {
    rgb(c.r(), c.g(), c.b())
}

/// A number rising from a blow.
struct Floater {
    at: Vec3,
    text: String,
    colour: egui::Color32,
    age: f32,
    big: bool,
    /// Seconds it shows (words said stay longer than numbers).
    life: f32,
    /// Metres a second it rises.
    rise: f32,
}

/// The fight's sounds (clips in `assets/sound`, see its README).
struct Sounds {
    punch: Sound,
    bolt: Sound,
    ray: Sound,
    charge: Sound,
    whoosh: Sound,
    chime: Sound,
    hum: Sound,
    boom: Sound,
    bind: Sound,
    shield: Sound,
    fanfare: Sound,
}

/// The power keys: 1 to 6 (Strike and Energy Bolt also on the left mouse
/// button and Q).
pub const POWER_KEYS: [(Key, Power); 6] = [
    (Key::Digit1, Power::Strike),
    (Key::Digit2, Power::Bolt),
    (Key::Digit3, Power::Hold),
    (Key::Digit4, Power::Shield),
    (Key::Digit5, Power::Lift),
    (Key::Digit6, Power::Shockwave),
];

/// The name a hero sees for a power: its theme's name.
pub fn power_name(p: Power, theme: u8) -> &'static str {
    if p == Power::Strike {
        return "Strike";
    }
    themes(p)[(theme & 1) as usize].name
}

/// A power's colours in a theme as sRGB bytes: (core, edge).
fn theme_bytes(p: Power, theme: u8) -> ([u8; 3], [u8; 3]) {
    match (p, theme & 1) {
        (Power::Bolt, 0) => ([120, 220, 255], [40, 120, 255]),
        (Power::Bolt, _) => ([220, 140, 255], [130, 40, 220]),
        (Power::Hold, 0) => ([140, 200, 255], [60, 90, 220]),
        (Power::Hold, _) => ([255, 210, 110], [220, 120, 30]),
        (Power::Shield, 0) => ([130, 230, 255], [40, 160, 220]),
        (Power::Shield, _) => ([255, 225, 130], [230, 170, 40]),
        (Power::Lift, 0) => ([255, 210, 160], [200, 120, 60]),
        (Power::Lift, _) => ([210, 150, 255], [120, 60, 230]),
        (Power::Shockwave, 0) => ([255, 240, 200], [255, 170, 60]),
        (Power::Shockwave, _) => ([170, 240, 255], [60, 170, 255]),
        _ => ([255, 236, 170], [255, 200, 120]),
    }
}

/// A power's colours in a theme: (core, edge).
pub fn theme_colours(p: Power, theme: u8) -> (Color, Color) {
    let (a, b) = theme_bytes(p, theme);
    (rgb(a[0], a[1], a[2]), rgb(b[0], b[1], b[2]))
}

/// A power's main colour in a theme, for the screen.
pub fn theme_ui_colour(p: Power, theme: u8) -> egui::Color32 {
    let (a, _) = theme_bytes(p, theme);
    egui::Color32::from_rgb(a[0], a[1], a[2])
}

/// What the player pressed this tick, for the server.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FightInput {
    pub power: u8,
    pub target: Option<NetId>,
    pub revive: bool,
    pub hospital: bool,
}

pub struct FightView {
    /// The villain picked with Tab.
    pub target: Option<NetId>,
    sounds: Option<Sounds>,
    floaters: Vec<Floater>,
    /// Camera shake strength (decays).
    shake: f32,
    /// A red edge when our hero is hit.
    hurt: f32,
    /// Characters frozen for a moment by a heavy blow.
    hitstop: BTreeMap<NetId, f32>,
    /// A clip shown instead of walking for a while: (clip, seconds left, once).
    actions: BTreeMap<NetId, (&'static str, f32)>,
    /// Status last seen per fighter.
    statuses: BTreeMap<NetId, Status>,
    /// Energy-cuff rings on the ground under defeated villains.
    cuffs: BTreeMap<NetId, DecalId>,
    /// The ring under the target: decal and where it is.
    marker: Option<(DecalId, Vec3, NetId)>,
    /// Seconds until each power is ready (our estimate; the server decides),
    /// by [`Power::code`] - 1.
    cooldown: [f32; 8],
    /// Glowing rings under held villains.
    holds: BTreeMap<NetId, DecalId>,
    /// Seconds to the next sparkle of each shield.
    sparkle: f32,
    /// A short line in the middle of the screen ("Out of range").
    note: Option<(String, f32)>,
    /// Drone hums, following each drone.
    hums: BTreeMap<NetId, SoundId>,
    /// Ray guns in technicians' hands: character entity -> gun.
    guns: std::collections::HashMap<EntityId, EntityId>,
    /// Next punch from our hero is a cross (alternating with the jab).
    cross: bool,
    clock: f32,
    /// Where robots' guns are this frame (rays start there).
    pub muzzles: BTreeMap<NetId, Vec3>,
}

impl Default for FightView {
    fn default() -> Self {
        Self::new()
    }
}

impl FightView {
    pub fn new() -> Self {
        Self {
            target: None,
            sounds: None,
            floaters: Vec::new(),
            shake: 0.0,
            hurt: 0.0,
            hitstop: BTreeMap::new(),
            actions: BTreeMap::new(),
            statuses: BTreeMap::new(),
            cuffs: BTreeMap::new(),
            marker: None,
            cooldown: [0.0; 8],
            holds: BTreeMap::new(),
            sparkle: 0.0,
            note: None,
            hums: BTreeMap::new(),
            guns: std::collections::HashMap::new(),
            cross: false,
            clock: 0.0,
            muzzles: BTreeMap::new(),
        }
    }

    /// Keys: Tab picks the villain nearest the crosshair (again: the next
    /// one), 1 or left mouse strikes, 2 or Q fires an energy bolt, 3 to 6
    /// the other powers, E held revives a downed teammate, H calls the
    /// medical drone when you are down.
    pub fn input(
        &mut self,
        input: &engine::input::Input,
        me: Option<&EntityState>,
        all: &[EntityState],
        eye: Vec3,
        look: Vec3,
        sheet: Option<&Sheet>,
    ) -> FightInput {
        let kit = sheet.map_or_else(|| Kit::of(&hw_sim::progress::starter()), Kit::of);
        let mut out = FightInput::default();
        // A target that fell or left is dropped.
        if let Some(t) = self.target {
            let ok = all.iter().any(|e| e.id == t && targetable(e));
            if !ok {
                self.target = None;
            }
        }
        if input.pressed(Key::Tab) {
            self.target = pick_target(all, eye, look, self.target);
            if self.target.is_none() {
                self.say("No villain in sight");
            }
        }
        let down = me
            .and_then(|m| m.combat)
            .is_some_and(|c| matches!(c.status, Status::Down | Status::Carried));
        for (key, power) in POWER_KEYS {
            let alt = match power {
                Power::Strike => input.mouse_clicked(MouseButton::Left),
                Power::Bolt => input.pressed(Key::Q),
                _ => false,
            };
            if !(input.pressed(key) || alt) || down {
                continue;
            }
            if !kit.has(power) {
                if input.pressed(key) {
                    self.say(&format!(
                        "You don't have {} yet (B: powers)",
                        power_spec(power).name
                    ));
                }
                continue;
            }
            let spec = power_spec(power);
            let carrying = me
                .and_then(|m| m.combat)
                .is_some_and(|c| c.effects & hw_net::EFFECT_CARRYING != 0);
            let aimed = matches!(power, Power::Strike | Power::Bolt | Power::Hold)
                || (power == Power::Lift && carrying);
            // No target yet: take the one nearest the crosshair.
            if aimed && self.target.is_none() {
                self.target = pick_target(all, eye, look, None);
            }
            if let Some(m) = me {
                if let Some(t) = self
                    .target
                    .filter(|_| aimed)
                    .and_then(|t| all.iter().find(|e| e.id == t))
                {
                    let d = pos(m).distance(pos(t));
                    if d > spec.range as f32 {
                        self.say(if power == Power::Strike {
                            "Too far to punch"
                        } else {
                            "Out of range"
                        });
                    }
                }
                let energy = m.combat.map_or(0.0, |c| c.energy as f32 / 255.0 * 100.0);
                if !carrying && energy + 0.5 < spec.cost {
                    self.say("Not enough energy");
                }
            }
            out.power = power.code();
        }
        out.target = self.target;
        out.revive = input.down(Key::E) && !down;
        out.hospital = input.pressed(Key::H) && down;
        out
    }

    /// A short line in the middle of the screen.
    pub fn say(&mut self, text: &str) {
        self.note = Some((text.to_string(), 1.4));
    }

    /// The clip to show for `id` instead of its walk, if any.
    pub fn action(&self, id: NetId) -> Option<&'static str> {
        self.actions.get(&id).map(|a| a.0)
    }

    /// Is `id` frozen by a heavy blow right now?
    pub fn frozen(&self, id: NetId) -> bool {
        self.hitstop.contains_key(&id)
    }

    /// Shakes the camera (a stomp, a shell landing near).
    pub fn add_shake(&mut self, strength: f32) {
        self.shake = self.shake.max(strength);
    }

    /// Camera shake to add to the eye this frame.
    pub fn shake(&self) -> Vec3 {
        if self.shake <= 0.001 {
            return Vec3::ZERO;
        }
        let t = self.clock * 60.0;
        Vec3::new(t.sin() * 0.9, (t * 1.37).cos(), (t * 0.71).sin() * 0.6) * self.shake * 0.12
    }

    /// Shows what happened in fights since the last frame. `ents` finds the
    /// drawn entity of a fighter.
    pub fn on_fx(
        &mut self,
        world: &mut World,
        events: &[Fx],
        now_tick: f64,
        me: Option<NetId>,
        ents: &BTreeMap<NetId, (EntityId, bool)>,
        all: &[EntityState],
    ) {
        if self.sounds.is_none() {
            self.sounds = Some(make_sounds());
        }
        for e in events {
            if (now_tick - e.tick as f64) * hw_sim::DT > LATE {
                continue;
            }
            // Attacked with nothing targeted: the attacker becomes the target.
            if self.target.is_none() {
                self.target = attacker(e, me, all);
            }
            let at = Vec3::from(e.at);
            let mine = me == Some(e.from);
            let on_me = me == Some(e.to);
            let from_ent = ents.get(&e.from).map(|x| x.0);
            let origin = self
                .muzzles
                .get(&e.from)
                .copied()
                .or_else(|| hand(world, from_ent))
                .unwrap_or_else(|| {
                    all.iter()
                        .find(|s| s.id == e.from)
                        .map_or(at, |s| pos(s) + Vec3::Y * 1.3)
                });
            match e.kind {
                FxKind::Strike => {
                    let clip = if self.cross {
                        "punch cross"
                    } else {
                        "punch jab"
                    };
                    self.cross = !self.cross;
                    self.act(e.from, clip, 0.75);
                    sparks(
                        world,
                        at,
                        (at - origin).normalize_or_zero(),
                        26,
                        rgb(255, 236, 170),
                    );
                    self.play(world, |s| &s.punch, at, 1.0, 1.0);
                    self.blow(e, at, mine, on_me, egui::Color32::from_rgb(255, 240, 140));
                    if mine {
                        self.recharging(Power::Strike);
                    }
                }
                FxKind::Bolt => {
                    let (core, edge) = theme_colours(Power::Bolt, e.theme);
                    self.act(e.from, "spell cast", 0.55);
                    beam(world, origin, at, core, edge);
                    impact(world, at, core, 1.2);
                    scorch(world, at, all, e.to);
                    let pitch = if e.theme == 1 { 0.8 } else { 1.0 };
                    self.play(world, |s| &s.bolt, origin, 1.0, pitch);
                    self.blow(e, at, mine, on_me, theme_ui_colour(Power::Bolt, e.theme));
                    if mine {
                        self.recharging(Power::Bolt);
                    }
                }
                FxKind::Hold => {
                    let (core, edge) = theme_colours(Power::Hold, e.theme);
                    self.act(e.from, "spell cast", 0.55);
                    beam(world, origin, at, core, edge);
                    rings(world, at, core, 1.0);
                    self.play(world, |s| &s.bind, at, 0.9, 1.0);
                    let ui = theme_ui_colour(Power::Hold, e.theme);
                    self.blow(e, at, mine, on_me, ui);
                    self.float(at + Vec3::Y * 0.9, "Held!".into(), ui, false);
                    if mine {
                        self.recharging(Power::Hold);
                    }
                }
                FxKind::Shield => {
                    let (core, _) = theme_colours(Power::Shield, e.theme);
                    shell(world, at, core, 60);
                    if e.from == e.to {
                        self.act(e.from, "spell cast", 0.55);
                        self.play(world, |s| &s.shield, at, 0.9, 1.0);
                    }
                    if mine && e.from == e.to {
                        self.recharging(Power::Shield);
                    }
                    if on_me {
                        self.say("Shielded: blows do half damage");
                    }
                }
                FxKind::Shockwave => {
                    let (core, edge) = theme_colours(Power::Shockwave, e.theme);
                    if e.from == e.to {
                        // The ring itself.
                        let feet = all.iter().find(|x| x.id == e.from).map_or(at, pos);
                        shockwave(
                            world,
                            feet,
                            core,
                            edge,
                            power_spec(Power::Shockwave).area as f32,
                        );
                        self.act(e.from, "spell cast", 0.6);
                        self.play(
                            world,
                            |s| &s.boom,
                            feet,
                            1.0,
                            if e.theme == 1 { 1.3 } else { 1.0 },
                        );
                        if mine {
                            self.shake = self.shake.max(0.8);
                            self.recharging(Power::Shockwave);
                        }
                    } else {
                        impact(world, at, core, 0.8);
                        self.blow(
                            e,
                            at,
                            mine,
                            on_me,
                            theme_ui_colour(Power::Shockwave, e.theme),
                        );
                    }
                }
                FxKind::Lift => {
                    let (core, _) = theme_colours(Power::Lift, e.theme);
                    self.act(e.from, "spell cast", 0.6);
                    rise(world, at - Vec3::Y * 1.5, core);
                    self.play(world, |s| &s.whoosh, at, 0.7, 0.8);
                }
                FxKind::Throw => {
                    self.act(e.from, "punch cross", 0.6);
                    self.play(world, |s| &s.whoosh, at, 1.0, 1.2);
                    if mine {
                        self.recharging(Power::Lift);
                    }
                }
                FxKind::ThrowHit => {
                    let (core, _) = theme_colours(Power::Lift, e.theme);
                    impact(world, at, core, 1.4);
                    sparks(world, at, Vec3::Y, 30, rgb(255, 220, 160));
                    self.play(world, |s| &s.punch, at, 1.0, 0.7);
                    self.blow(e, at, mine, on_me, theme_ui_colour(Power::Lift, e.theme));
                }
                FxKind::Warn => {
                    // A gang warns a hero off its turf: everyone near hears it.
                    let lines = hw_sim::combat::EnemyKind::from_entity(
                        all.iter()
                            .find(|x| x.id == e.from)
                            .map_or(EntityKind::Hero, |x| x.kind),
                    )
                    .map_or(&["Go away!"][..], |k| {
                        hw_sim::factions::warnings(hw_sim::factions::of_enemy(k))
                    });
                    let line = lines[(e.from.0 as usize + e.tick as usize / 7) % lines.len()];
                    let head = all.iter().find(|x| x.id == e.from).map_or(at, pos);
                    self.speak(
                        head + Vec3::Y * 2.3,
                        line.to_string(),
                        egui::Color32::from_rgb(255, 120, 90),
                    );
                    if on_me {
                        self.note = Some(("You're on their turf: back off, or fight.".into(), 3.0));
                    }
                }
                FxKind::Karma => {
                    if on_me {
                        self.float(
                            at + Vec3::Y * 0.6,
                            format!("+{} karma", e.amount),
                            egui::Color32::from_rgb(200, 160, 255),
                            false,
                        );
                    }
                }
                FxKind::LevelUp => {
                    // The level-up moment: a golden column and a fanfare.
                    let feet = all.iter().find(|x| x.id == e.to).map_or(at, pos);
                    level_burst(world, feet);
                    if on_me {
                        self.play(world, |s| &s.fanfare, feet, 1.0, 1.0);
                        self.shake = self.shake.max(0.4);
                    } else {
                        self.play(world, |s| &s.chime, feet, 0.8, 1.2);
                        self.float(
                            feet + Vec3::Y * 2.4,
                            format!("Level {}!", e.amount),
                            egui::Color32::from_rgb(255, 210, 80),
                            true,
                        );
                    }
                }
                FxKind::PunchWindup => {
                    // The telegraph: a wide hook you can see coming.
                    self.act(e.from, "hook", 0.95);
                }
                FxKind::Punch => {
                    if e.amount > 0 {
                        sparks(
                            world,
                            at,
                            (at - origin).normalize_or_zero(),
                            14,
                            rgb(255, 200, 150),
                        );
                        self.play(world, |s| &s.punch, at, 0.9, 0.8);
                        self.blow(e, at, false, on_me, egui::Color32::from_rgb(255, 90, 70));
                    } else {
                        self.play(world, |s| &s.whoosh, at, 0.4, 1.6);
                    }
                }
                FxKind::RayWindup => {
                    self.act(e.from, "pistol aim", 0.75);
                    glow_at(world, origin, rgb(150, 255, 120), 0.25);
                    self.play(world, |s| &s.charge, origin, 0.6, 1.0);
                }
                FxKind::Ray => {
                    self.act(e.from, "pistol shoot", 0.65);
                    let to = if e.amount > 0 {
                        at
                    } else {
                        // A miss sails past.
                        at + (at - origin).normalize_or_zero() * 6.0 + Vec3::Y * 0.6
                    };
                    beam(world, origin, to, rgb(170, 255, 110), rgb(60, 200, 40));
                    if e.amount > 0 {
                        impact(world, at, rgb(170, 255, 120), 0.7);
                        self.blow(e, at, false, on_me, egui::Color32::from_rgb(255, 90, 70));
                    }
                    self.play(world, |s| &s.ray, origin, 0.8, 1.0);
                }
                FxKind::Revived => {
                    rise(world, at, rgb(255, 220, 120));
                    self.play(world, |s| &s.chime, at, 1.0, 1.0);
                    self.float(
                        at + Vec3::Y,
                        "Revived!".into(),
                        egui::Color32::from_rgb(255, 220, 120),
                        true,
                    );
                }
                FxKind::Lifted => {
                    self.play(world, |s| &s.whoosh, at, 0.8, 1.0);
                }
                FxKind::Healed => {
                    rise(world, at, rgb(140, 255, 170));
                    self.play(world, |s| &s.chime, at, 1.0, 1.25);
                    if on_me || mine {
                        self.say("Patched up at the hospital");
                    }
                }
                // The machines' and bosses' blows on heroes: numbers and a
                // shake here, the big effects in `robotview`.
                FxKind::Shell | FxKind::Stomp | FxKind::Pulse | FxKind::Leap
                    if e.from != e.to && e.amount > 0 =>
                {
                    self.blow(e, at, false, on_me, egui::Color32::from_rgb(255, 90, 70));
                }
                FxKind::LeapWindup => self.act(e.from, "jump start", 0.9),
                FxKind::ShellWindup
                | FxKind::Shell
                | FxKind::StompWindup
                | FxKind::Stomp
                | FxKind::PulseWindup
                | FxKind::Pulse
                | FxKind::Leap
                | FxKind::PartBroken
                | FxKind::Rise
                | FxKind::Kneel => {}
            }
        }
    }

    /// A blow landed: numbers, shake, hit-stop, the hit reaction.
    fn blow(&mut self, e: &Fx, at: Vec3, mine: bool, on_me: bool, colour: egui::Color32) {
        if e.amount > 0 {
            self.float(
                at + Vec3::Y * 0.4,
                format!("{}", e.amount),
                colour,
                e.defeat,
            );
        }
        if e.defeat && mine {
            self.float(
                at + Vec3::Y * 1.0,
                "Defeated!".into(),
                egui::Color32::from_rgb(255, 210, 80),
                true,
            );
        }
        if mine {
            self.shake = self.shake.max(if e.knock { 0.55 } else { 0.3 });
        }
        if on_me {
            self.shake = self.shake.max(0.5);
            self.hurt = 1.0;
        }
        // Hit-stop: the struck and the striker freeze for a heartbeat.
        let stop = if e.knock || e.defeat { 0.09 } else { 0.06 };
        self.hitstop.insert(e.to, stop);
        self.hitstop.insert(e.from, stop);
        if !e.knock && !e.defeat && !self.actions.contains_key(&e.to) {
            self.act(e.to, "hit to chest", 0.4);
        }
    }

    fn recharging(&mut self, p: Power) {
        self.cooldown[(p.code() - 1) as usize] = power_spec(p).recharge as f32;
    }

    fn act(&mut self, id: NetId, clip: &'static str, seconds: f32) {
        self.actions.insert(id, (clip, seconds));
    }

    fn float(&mut self, at: Vec3, text: String, colour: egui::Color32, big: bool) {
        self.floaters.push(Floater {
            at,
            text,
            colour,
            age: 0.0,
            big,
            life: 1.3,
            rise: 0.9,
        });
    }

    /// Words someone says, over their head for a few seconds.
    fn speak(&mut self, at: Vec3, text: String, colour: egui::Color32) {
        self.floaters.push(Floater {
            at,
            text,
            colour,
            age: 0.0,
            big: false,
            life: 3.2,
            rise: 0.15,
        });
    }

    fn play(
        &mut self,
        world: &mut World,
        which: impl Fn(&Sounds) -> &Sound,
        at: Vec3,
        volume: f32,
        pitch: f32,
    ) {
        if let Some(s) = &self.sounds {
            world.play_sound(
                which(s),
                PlaySound::at(at)
                    .volume(volume)
                    .pitch(pitch)
                    .range(4.0, 90.0),
            );
        }
    }

    /// Knocks, defeats, revivals and lifts: ragdolls and cuffs follow the
    /// fighters' status. Call after the characters are drawn.
    pub fn statuses(
        &mut self,
        world: &mut World,
        all: &[EntityState],
        ents: &mut BTreeMap<NetId, (EntityId, bool)>,
    ) {
        let mut seen = Vec::new();
        for s in all {
            let Some(c) = s.combat else { continue };
            seen.push(s.id);
            let was = self
                .statuses
                .insert(s.id, c.status)
                .unwrap_or(Status::Standing);
            let Some(&(e, animated)) = ents.get(&s.id) else {
                continue;
            };
            if was == c.status {
                continue;
            }
            let vel = Vec3::from(s.vel);
            let villain = s.kind.is_enemy();
            match (was, c.status) {
                (_, Status::Knocked) if animated => {
                    let knock = engine::ragdoll::Knock::new(Vec3::ZERO)
                        .velocity(vel)
                        .stay_down(true);
                    let _ = world.knock_down(e, knock);
                    self.actions.remove(&s.id);
                }
                (Status::Knocked, Status::Standing) if animated => {
                    let _ = world.set_stay_down(e, false);
                    let _ = world.get_up(e);
                }
                (Status::Down | Status::Knocked, Status::Standing) if animated => {
                    // Revived (or back from hospital): up again.
                    let _ = world.set_stay_down(e, false);
                    let _ = world.get_up(e);
                }
                (_, Status::Down) => {
                    if animated {
                        if world.ragdoll_state(e).is_some() {
                            let _ = world.set_stay_down(e, true);
                        } else {
                            let v = if vel.length() > 0.5 {
                                vel
                            } else {
                                Vec3::Y * 0.5
                            };
                            let knock = engine::ragdoll::Knock::new(Vec3::ZERO)
                                .velocity(v)
                                .stay_down(true);
                            let _ = world.knock_down(e, knock);
                        }
                    }
                    self.actions.remove(&s.id);
                    if villain {
                        // Energy cuffs: a glowing ring where they lie.
                        let p = pos(s) + Vec3::Y * 0.3;
                        let ring = Decal::new(DecalKind::Power, p, Vec3::Y, 2.2)
                            .tint(rgb(90, 220, 255))
                            .emission(3.0)
                            .depth(1.5)
                            .fade(0.3, 0.3);
                        if let Ok(id) = world.spawn_decal(ring) {
                            self.cuffs.insert(s.id, id);
                        }
                        self.float(
                            p + Vec3::Y * 1.2,
                            "Cuffed".into(),
                            egui::Color32::from_rgb(120, 220, 255),
                            false,
                        );
                    }
                }
                (_, Status::Carried) => {
                    // Off the ground: a fresh character hangs from the drone.
                    if let Some(d) = self.cuffs.remove(&s.id) {
                        world.remove_decal(d);
                    }
                    if let Some((e, _)) = ents.remove(&s.id) {
                        self.drop_gun(world, e);
                        world.despawn(e);
                    }
                    self.act(s.id, if villain { "cuffed" } else { "hover" }, 1.0e6);
                }
                _ => {}
            }
        }
        // Forget the gone.
        let gone: Vec<NetId> = self
            .statuses
            .keys()
            .filter(|id| !seen.contains(id))
            .copied()
            .collect();
        for id in gone {
            self.statuses.remove(&id);
            self.actions.remove(&id);
            if let Some(d) = self.cuffs.remove(&id) {
                world.remove_decal(d);
            }
        }
        // Drone hums follow the drones.
        let drones: Vec<&EntityState> =
            all.iter().filter(|e| e.kind == EntityKind::Drone).collect();
        for d in &drones {
            if !self.hums.contains_key(&d.id) {
                if self.sounds.is_none() {
                    self.sounds = Some(make_sounds());
                }
                if let (Some(s), Some(&(e, _))) = (&self.sounds, ents.get(&d.id)) {
                    let id = world.play_sound(
                        &s.hum,
                        PlaySound::following(e)
                            .looped()
                            .volume(0.5)
                            .range(3.0, 70.0)
                            .fade_in(0.5),
                    );
                    self.hums.insert(d.id, id);
                }
            }
        }
        let quiet: Vec<NetId> = self
            .hums
            .keys()
            .filter(|id| !drones.iter().any(|d| d.id == **id))
            .copied()
            .collect();
        for id in quiet {
            if let Some(s) = self.hums.remove(&id) {
                world.stop_sound(s, 0.4);
            }
        }
    }

    /// Puts a ray gun in a technician's right hand (once per character).
    pub fn arm(&mut self, world: &mut World, e: EntityId) {
        if self.guns.contains_key(&e) {
            return;
        }
        if let Ok(gun) = world.place(ray_gun(), Place::new(0.0, 0.0, 0.0)) {
            // The grip in the palm, the barrel along the fingers.
            let local = Place::new(0.0, 0.08, 0.03).with_pitch_deg(-90.0);
            if world.attach_to_bone(gun, e, "hand_r", local).is_ok() {
                self.guns.insert(e, gun);
            } else {
                world.despawn(gun);
            }
        }
    }

    /// Takes a ray gun away with its technician.
    pub fn drop_gun(&mut self, world: &mut World, e: EntityId) {
        if let Some(g) = self.guns.remove(&e) {
            world.despawn(g);
        }
    }

    /// Time passes: numbers rise, shakes and freezes fade, the ring under
    /// the target follows it.
    pub fn update(
        &mut self,
        world: &mut World,
        dt: f32,
        all: &[EntityState],
        me: Option<&EntityState>,
    ) {
        self.clock += dt;
        self.shake = (self.shake - dt * 2.2).max(0.0);
        self.hurt = (self.hurt - dt * 2.0).max(0.0);
        for c in &mut self.cooldown {
            *c = (*c - dt).max(0.0);
        }
        self.hitstop.retain(|_, t| {
            *t -= dt;
            *t > 0.0
        });
        self.actions.retain(|_, a| {
            a.1 -= dt;
            a.1 > 0.0
        });
        for f in &mut self.floaters {
            f.age += dt;
            f.at.y += dt * f.rise;
        }
        self.floaters.retain(|f| f.age < f.life);
        if let Some(n) = &mut self.note {
            n.1 -= dt;
            if n.1 <= 0.0 {
                self.note = None;
            }
        }
        // Held villains stand in glowing rings; shields sparkle.
        let mut held = Vec::new();
        self.sparkle -= dt;
        let sparkle = self.sparkle <= 0.0;
        if sparkle {
            self.sparkle = 0.25;
        }
        for e in all {
            let Some(c) = e.combat else { continue };
            if c.effects & EFFECT_HELD != 0 {
                held.push(e.id);
                if let std::collections::btree_map::Entry::Vacant(slot) = self.holds.entry(e.id) {
                    let ring = Decal::new(DecalKind::Power, pos(e) + Vec3::Y * 0.2, Vec3::Y, 1.9)
                        .tint(rgb(150, 200, 255))
                        .emission(3.5)
                        .depth(1.4)
                        .fade(0.2, 0.3);
                    if let Ok(d) = world.spawn_decal(ring) {
                        slot.insert(d);
                    }
                }
                if sparkle {
                    rings(world, pos(e) + Vec3::Y * 1.1, rgb(160, 210, 255), 0.5);
                }
            }
            if c.effects & EFFECT_SHIELD != 0 && sparkle {
                shell(world, pos(e) + Vec3::Y * 1.0, rgb(140, 220, 255), 14);
            }
        }
        let freed: Vec<NetId> = self
            .holds
            .keys()
            .filter(|id| !held.contains(id))
            .copied()
            .collect();
        for id in freed {
            if let Some(d) = self.holds.remove(&id) {
                world.remove_decal(d);
            }
        }
        // The target ring, in the target's con colour.
        let want = self.target.and_then(|t| all.iter().find(|e| e.id == t));
        let mine = me.and_then(|m| m.combat).map_or(1, |c| c.level);
        match want {
            Some(t) => {
                let p = pos(t) + Vec3::Y * 0.2;
                let moved = self
                    .marker
                    .is_none_or(|(_, at, id)| id != t.id || at.distance(p) > 0.08);
                if moved {
                    if let Some((d, _, _)) = self.marker.take() {
                        world.remove_decal(d);
                    }
                    let colour = engine_colour(con_colour(t.combat.map_or(1, |c| c.level), mine));
                    let ring = Decal::new(DecalKind::Power, p, Vec3::Y, 1.6)
                        .tint(colour)
                        .emission(2.0)
                        .depth(1.2);
                    if let Ok(d) = world.spawn_decal(ring) {
                        self.marker = Some((d, p, t.id));
                    }
                }
            }
            None => {
                if let Some((d, _, _)) = self.marker.take() {
                    world.remove_decal(d);
                }
            }
        }
    }

    /// The fight's screen: bars and power tray, the target frame, floating
    /// numbers and names over villains, and what to do when someone is down.
    pub fn hud(
        &self,
        ctx: &egui::Context,
        view_proj: Mat4,
        me: Option<&EntityState>,
        all: &[EntityState],
        names: &dyn Fn(&EntityState) -> String,
        sheet: Option<&Sheet>,
    ) {
        let screen = ctx.screen_rect();
        let starter = hw_sim::progress::starter();
        let sheet = sheet.unwrap_or(&starter);
        let kit = Kit::of(sheet);
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("fight hud"),
        ));
        let project = |p: Vec3| -> Option<egui::Pos2> {
            let c = view_proj * p.extend(1.0);
            if c.w <= 0.05 {
                return None;
            }
            let n = c.truncate() / c.w;
            if n.x.abs() > 1.2 || n.y.abs() > 1.2 {
                return None;
            }
            Some(egui::pos2(
                screen.left() + (n.x * 0.5 + 0.5) * screen.width(),
                screen.top() + (0.5 - n.y * 0.5) * screen.height(),
            ))
        };
        let mine = me.and_then(|m| m.combat).map_or(1, |c| c.level);
        let font = |s: f32| egui::FontId::proportional(s);
        let dark = egui::Color32::from_black_alpha(170);

        // Over villains: a slim health bar once hurt, the name when targeted.
        for e in all.iter().filter(|e| e.kind.is_enemy()) {
            let Some(c) = e.combat else { continue };
            if !matches!(c.status, Status::Standing | Status::Knocked) {
                continue;
            }
            let targeted = self.target == Some(e.id);
            if c.health == 255 && !targeted {
                continue;
            }
            let Some(p) = project(pos(e) + Vec3::Y * crate::robotview::bar_height(e.kind)) else {
                continue;
            };
            let w = 46.0;
            let r = egui::Rect::from_min_size(p - egui::vec2(w * 0.5, 0.0), egui::vec2(w, 5.0));
            painter.rect_filled(r.expand(1.0), 1.0, dark);
            let f = c.health as f32 / 255.0;
            painter.rect_filled(
                egui::Rect::from_min_size(r.min, egui::vec2(w * f, 5.0)),
                1.0,
                egui::Color32::from_rgb(220, 60, 50),
            );
            if targeted {
                let col = con_colour(c.level, mine);
                // A con-coloured arrow over the target's head.
                let tip = p - egui::vec2(0.0, 5.0);
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        tip,
                        tip + egui::vec2(8.0, -12.0),
                        tip + egui::vec2(-8.0, -12.0),
                    ],
                    col,
                    egui::Stroke::new(1.0_f32, egui::Color32::BLACK),
                ));
            }
        }

        // Floating numbers.
        for f in &self.floaters {
            let Some(p) = project(f.at) else { continue };
            let a = (1.0 - (f.age / f.life).powi(2)).clamp(0.0, 1.0);
            let size =
                if f.big { 26.0 } else { 20.0 } * (1.0 + 0.4 * (1.0 - (f.age * 6.0).min(1.0)));
            let col = f.colour.gamma_multiply(a);
            painter.text(
                p + egui::vec2(2.0, 2.0),
                egui::Align2::CENTER_CENTER,
                &f.text,
                font(size),
                egui::Color32::from_black_alpha((a * 200.0) as u8),
            );
            painter.text(p, egui::Align2::CENTER_CENTER, &f.text, font(size), col);
        }

        // A red edge when hurt.
        if self.hurt > 0.0 {
            let a = (self.hurt * 90.0) as u8;
            let red = egui::Color32::from_rgba_unmultiplied(200, 20, 20, a);
            let t = 26.0;
            for r in [
                egui::Rect::from_min_max(screen.min, egui::pos2(screen.max.x, screen.min.y + t)),
                egui::Rect::from_min_max(egui::pos2(screen.min.x, screen.max.y - t), screen.max),
                egui::Rect::from_min_max(screen.min, egui::pos2(screen.min.x + t, screen.max.y)),
                egui::Rect::from_min_max(egui::pos2(screen.max.x - t, screen.min.y), screen.max),
            ] {
                painter.rect_filled(r, 0.0, red);
            }
        }

        // The target frame, top centre.
        if let Some(t) = self.target.and_then(|t| all.iter().find(|e| e.id == t)) {
            if let Some(c) = t.combat {
                let (mut name, rank) = enemy_name(t.kind);
                if let Some((on, _)) = t.mount {
                    name = crate::robotview::part_name(all, on, t.id);
                }
                let col = con_colour(c.level, mine);
                let w = 300.0;
                let r = egui::Rect::from_min_size(
                    egui::pos2(screen.center().x - w * 0.5, screen.top() + 14.0),
                    egui::vec2(w, 58.0),
                );
                painter.rect_filled(r, 6.0, dark);
                painter.rect_stroke(
                    r,
                    6.0,
                    egui::Stroke::new(2.0_f32, col),
                    egui::StrokeKind::Inside,
                );
                painter.text(
                    r.min + egui::vec2(12.0, 8.0),
                    egui::Align2::LEFT_TOP,
                    name,
                    font(18.0),
                    col,
                );
                painter.text(
                    egui::pos2(r.max.x - 12.0, r.min.y + 10.0),
                    egui::Align2::RIGHT_TOP,
                    format!("{rank} · level {}", c.level),
                    font(13.0),
                    egui::Color32::from_gray(210),
                );
                bar(
                    &painter,
                    egui::Rect::from_min_size(
                        r.min + egui::vec2(12.0, 36.0),
                        egui::vec2(w - 24.0, 12.0),
                    ),
                    c.health,
                    egui::Color32::from_rgb(214, 58, 48),
                );
            }
        }

        // Our hero: health, energy and the power tray, bottom centre.
        if let Some(c) = me.and_then(|m| m.combat) {
            let tray: Vec<(usize, Power)> = POWER_KEYS
                .iter()
                .enumerate()
                .filter(|(_, (_, p))| kit.has(*p))
                .map(|(k, (_, p))| (k, *p))
                .collect();
            let slot_w = 104.0;
            let w = (12.0 + tray.len() as f32 * (slot_w + 6.0) + 6.0).max(340.0);
            let base = egui::pos2(screen.center().x - w * 0.5, screen.bottom() - 112.0);
            let panel = egui::Rect::from_min_size(base, egui::vec2(w, 98.0));
            painter.rect_filled(panel, 8.0, dark);
            let label = |y: f32, t: &str| {
                painter.text(
                    base + egui::vec2(12.0, y),
                    egui::Align2::LEFT_CENTER,
                    t,
                    font(12.0),
                    egui::Color32::from_gray(220),
                );
            };
            label(16.0, "Health");
            bar(
                &painter,
                egui::Rect::from_min_size(
                    base + egui::vec2(66.0, 10.0),
                    egui::vec2(w - 78.0, 12.0),
                ),
                c.health,
                egui::Color32::from_rgb(60, 200, 80),
            );
            label(34.0, "Energy");
            bar(
                &painter,
                egui::Rect::from_min_size(
                    base + egui::vec2(66.0, 28.0),
                    egui::vec2(w - 78.0, 12.0),
                ),
                c.energy,
                egui::Color32::from_rgb(70, 140, 255),
            );
            let energy = c.energy as f32 / 255.0 * 100.0;
            let carrying = c.effects & hw_net::EFFECT_CARRYING != 0;
            for (n, &(k, power)) in tray.iter().enumerate() {
                let spec = power_spec(power);
                let slot = egui::Rect::from_min_size(
                    base + egui::vec2(12.0 + n as f32 * (slot_w + 6.0), 48.0),
                    egui::vec2(slot_w, 40.0),
                );
                let cd = self.cooldown[(power.code() - 1) as usize];
                let ready =
                    cd <= 0.0 && (energy + 0.5 >= spec.cost || carrying && power == Power::Lift);
                let fill = if ready {
                    egui::Color32::from_rgb(52, 60, 92)
                } else {
                    egui::Color32::from_rgb(40, 40, 46)
                };
                painter.rect_filled(slot, 5.0, fill);
                if cd > 0.0 {
                    let f = cd / spec.recharge as f32;
                    let cover = egui::Rect::from_min_max(
                        egui::pos2(slot.min.x, slot.max.y - slot.height() * f),
                        slot.max,
                    );
                    painter.rect_filled(cover, 5.0, egui::Color32::from_black_alpha(140));
                }
                painter.rect_stroke(
                    slot,
                    5.0,
                    egui::Stroke::new(
                        1.5_f32,
                        if ready {
                            egui::Color32::from_rgb(255, 214, 90)
                        } else {
                            egui::Color32::from_gray(90)
                        },
                    ),
                    egui::StrokeKind::Inside,
                );
                painter.text(
                    slot.min + egui::vec2(8.0, 6.0),
                    egui::Align2::LEFT_TOP,
                    format!("{}", k + 1),
                    font(12.0),
                    egui::Color32::from_rgb(255, 214, 90),
                );
                let name = if power == Power::Lift && carrying {
                    "Throw!"
                } else {
                    power_name(power, sheet.theme(power))
                };
                painter.text(
                    slot.min + egui::vec2(20.0, 6.0),
                    egui::Align2::LEFT_TOP,
                    name,
                    font(12.5),
                    egui::Color32::WHITE,
                );
                let cost = if spec.cost > 0.0 {
                    format!("{:.0} energy", spec.cost)
                } else {
                    "free".into()
                };
                painter.text(
                    slot.min + egui::vec2(20.0, 24.0),
                    egui::Align2::LEFT_TOP,
                    cost,
                    font(11.0),
                    egui::Color32::from_gray(180),
                );
            }
            painter.text(
                egui::pos2(screen.center().x, base.y - 8.0),
                egui::Align2::CENTER_BOTTOM,
                &{
                    let mut hint = String::from("Tab: target   Left mouse / 1   Q / 2   B: powers");
                    let moves: Vec<&str> = [Power::SuperJump, Power::Flight]
                        .into_iter()
                        .filter(|p| kit.has(*p) && is_movement(*p))
                        .map(|p| {
                            if p == Power::Flight {
                                "V: fly"
                            } else {
                                "Space: super jump"
                            }
                        })
                        .collect();
                    for m in moves {
                        hint += "   ";
                        hint += m;
                    }
                    hint
                },
                font(12.0),
                egui::Color32::from_white_alpha(150),
            );

            // Down: what now.
            if c.status == Status::Down {
                let p = screen.center() - egui::vec2(0.0, 60.0);
                painter.text(
                    p,
                    egui::Align2::CENTER_CENTER,
                    "You are down!",
                    font(34.0),
                    egui::Color32::from_rgb(255, 90, 70),
                );
                painter.text(
                    p + egui::vec2(0.0, 34.0),
                    egui::Align2::CENTER_CENTER,
                    "A teammate can revive you (hold E next to you)",
                    font(16.0),
                    egui::Color32::WHITE,
                );
                painter.text(
                    p + egui::vec2(0.0, 56.0),
                    egui::Align2::CENTER_CENTER,
                    "H: call the medical drone to fly you to hospital",
                    font(16.0),
                    egui::Color32::from_gray(220),
                );
            }
        }

        // A teammate down nearby.
        if let Some(m) = me.filter(|m| m.combat.is_some_and(|c| c.status == Status::Standing)) {
            for e in all {
                let down = e.kind == EntityKind::Hero
                    && e.id != m.id
                    && e.combat.is_some_and(|c| c.status == Status::Down);
                if !down {
                    continue;
                }
                let d = pos(e).distance(pos(m));
                if let Some(p) = project(pos(e) + Vec3::Y * 0.9) {
                    let text = if d <= hw_sim::combat::REVIVE_REACH as f32 {
                        format!("Hold E to revive {}", names(e))
                    } else {
                        format!("{} is down", names(e))
                    };
                    painter.text(
                        p,
                        egui::Align2::CENTER_BOTTOM,
                        text,
                        font(16.0),
                        egui::Color32::from_rgb(255, 220, 120),
                    );
                }
            }
        }

        if let Some((text, left)) = &self.note {
            let a = left.min(1.0);
            painter.text(
                screen.center() + egui::vec2(0.0, 80.0),
                egui::Align2::CENTER_CENTER,
                text,
                font(18.0),
                egui::Color32::from_rgb(255, 230, 150).gamma_multiply(a),
            );
        }
    }
}

fn bar(painter: &egui::Painter, r: egui::Rect, v: u8, colour: egui::Color32) {
    painter.rect_filled(r, 3.0, egui::Color32::from_gray(30));
    let f = v as f32 / 255.0;
    painter.rect_filled(
        egui::Rect::from_min_size(r.min, egui::vec2(r.width() * f, r.height())),
        3.0,
        colour,
    );
}

fn pos(e: &EntityState) -> Vec3 {
    Vec3::new(e.pos[0] as f32, e.pos[1] as f32, e.pos[2] as f32)
}

/// Can this entity be picked with Tab?
pub fn targetable(e: &EntityState) -> bool {
    e.kind.is_enemy()
        && e.combat
            .is_some_and(|c| matches!(c.status, Status::Standing | Status::Knocked))
}

/// The villain who attacked `me` in this event, if it can be targeted: a
/// blow aimed at our hero (hit or miss) by someone else.
pub fn attacker(e: &Fx, me: Option<NetId>, all: &[EntityState]) -> Option<NetId> {
    let attack = matches!(
        e.kind,
        FxKind::Punch
            | FxKind::Ray
            | FxKind::ThrowHit
            | FxKind::Shell
            | FxKind::Stomp
            | FxKind::Pulse
            | FxKind::Leap
    );
    if !attack || me != Some(e.to) || e.from == e.to {
        return None;
    }
    all.iter()
        .find(|x| x.id == e.from && targetable(x))
        .map(|x| x.id)
}

/// The villain nearest the crosshair (the line from `eye` along `look`),
/// within range and in view; with `current` set, the next one after it.
pub fn pick_target(
    all: &[EntityState],
    eye: Vec3,
    look: Vec3,
    current: Option<NetId>,
) -> Option<NetId> {
    let look = look.normalize_or_zero();
    let mut list: Vec<(f32, NetId)> = all
        .iter()
        .filter(|e| targetable(e))
        .filter_map(|e| {
            let to = pos(e) + Vec3::Y * 1.1 - eye;
            let d = to.length();
            if !(1e-3..=TARGET_RANGE).contains(&d) {
                return None;
            }
            let angle = look.dot(to / d).clamp(-1.0, 1.0).acos().to_degrees();
            // Near ones count as nearer the crosshair: a brute in your face
            // comes before one far off in the middle of the screen.
            (angle <= TARGET_CONE).then_some((angle + d * 0.15, e.id))
        })
        .collect();
    list.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    match current.and_then(|c| list.iter().position(|(_, id)| *id == c)) {
        Some(i) if list.len() > 1 => Some(list[(i + 1) % list.len()].1),
        _ => list.first().map(|x| x.1),
    }
}

/// Where a character's right hand is now, if drawn as one.
fn hand(world: &World, e: Option<EntityId>) -> Option<Vec3> {
    let m = world.bone_transform(e?, "hand_r").ok()?;
    Some(m.w_axis.truncate())
}

pub fn sparks(world: &mut World, at: Vec3, dir: Vec3, count: u32, colour: Color) {
    let p = world.particles_mut();
    let _ = p.burst(
        ParticleEmitter::sparks(at, dir.normalize_or(Vec3::Y), count)
            .with_color(colour)
            .with_emissive_intensity(4.0),
    );
    let _ = p.burst(
        ParticleEmitter::new(at)
            .with_mode(ParticleMode::Burst)
            .with_burst_count(1)
            .with_size(0.55)
            .with_lifetime(0.12)
            .with_spread(Vec3::ZERO)
            .with_color(colour)
            .with_size_over_life(SizeOverLife::Shrink)
            .with_emissive_intensity(6.0),
    );
}

fn impact(world: &mut World, at: Vec3, colour: Color, size: f32) {
    let p = world.particles_mut();
    let _ = p.burst(
        ParticleEmitter::new(at)
            .with_mode(ParticleMode::Burst)
            .with_shape(ParticleShape::Sphere)
            .with_burst_count(40)
            .with_size(0.12 * size)
            .with_lifetime(0.45)
            .with_spread(Vec3::splat(0.15))
            .with_velocity_jitter(Vec3::splat(4.5 * size))
            .with_drag(3.0)
            .with_color(colour)
            .with_secondary_color(rgb(255, 255, 255))
            .with_silhouette(ParticleSilhouette::SparkStreak)
            .with_velocity_stretch(0.06)
            .with_emissive_intensity(6.0),
    );
    let _ = p.burst(
        ParticleEmitter::new(at)
            .with_mode(ParticleMode::Burst)
            .with_burst_count(1)
            .with_size(1.1 * size)
            .with_lifetime(0.2)
            .with_spread(Vec3::ZERO)
            .with_color(colour)
            .with_size_over_life(SizeOverLife::GrowThenFade)
            .with_emissive_intensity(5.0),
    );
}

/// A beam of light from `a` to `b`: a line of glowing motes that flare and
/// fade within a fifth of a second.
fn beam(world: &mut World, a: Vec3, b: Vec3, core: Color, edge: Color) {
    let len = a.distance(b);
    if !len.is_finite() || len < 0.05 {
        return;
    }
    let n = ((len / 0.22) as usize).clamp(2, 260);
    let p = world.particles_mut();
    for i in 0..=n {
        let at = a.lerp(b, i as f32 / n as f32);
        let _ = p.burst(
            ParticleEmitter::new(at)
                .with_mode(ParticleMode::Burst)
                .with_burst_count(2)
                .with_size(0.16)
                .with_size_variation(0.4)
                .with_lifetime(0.22)
                .with_spread(Vec3::splat(0.03))
                .with_velocity_jitter(Vec3::splat(0.25))
                .with_color(core)
                .with_secondary_color(edge)
                .with_size_over_life(SizeOverLife::Shrink)
                .with_seed(i as u32)
                .with_emissive_intensity(7.0),
        );
    }
}

pub fn glow_at(world: &mut World, at: Vec3, colour: Color, size: f32) {
    let _ = world.particles_mut().burst(
        ParticleEmitter::new(at)
            .with_mode(ParticleMode::Burst)
            .with_burst_count(12)
            .with_shape(ParticleShape::Sphere)
            .with_size(0.1 * size / 0.25)
            .with_lifetime(0.6)
            .with_spread(Vec3::splat(size))
            .with_force(ParticleForce::Radial {
                center: at,
                strength: -4.0,
            })
            .with_color(colour)
            .with_emissive_intensity(5.0),
    );
}

/// Rings of light closing round a held villain.
fn rings(world: &mut World, at: Vec3, colour: Color, size: f32) {
    let _ = world.particles_mut().burst(
        ParticleEmitter::new(at)
            .with_mode(ParticleMode::Burst)
            .with_shape(ParticleShape::Ring)
            .with_burst_count((40.0 * size) as u32 + 8)
            .with_size(0.08)
            .with_lifetime(0.7)
            .with_spread(Vec3::new(0.9 * size, 0.05, 0.9 * size))
            .with_force(ParticleForce::Radial {
                center: at,
                strength: -3.0,
            })
            .with_color(colour)
            .with_silhouette(ParticleSilhouette::RuneMote)
            .with_emissive_intensity(5.0),
    );
}

/// A sparkling shell round someone shielded.
fn shell(world: &mut World, at: Vec3, colour: Color, count: u32) {
    let _ = world.particles_mut().burst(
        ParticleEmitter::new(at)
            .with_mode(ParticleMode::Burst)
            .with_shape(ParticleShape::Sphere)
            .with_burst_count(count)
            .with_size(0.07)
            .with_lifetime(0.5)
            .with_spread(Vec3::splat(1.1))
            .with_velocity_jitter(Vec3::splat(0.2))
            .with_color(colour)
            .with_secondary_color(rgb(255, 255, 255))
            .with_size_over_life(SizeOverLife::Shrink)
            .with_emissive_intensity(4.0),
    );
}

/// A shockwave: a ring of light racing out over the ground, dust, a scorch.
pub fn shockwave(world: &mut World, feet: Vec3, core: Color, edge: Color, radius: f32) {
    let p = world.particles_mut();
    let _ = p.burst(
        ParticleEmitter::new(feet + Vec3::Y * 0.3)
            .with_mode(ParticleMode::Burst)
            .with_shape(ParticleShape::Ring)
            .with_burst_count(220)
            .with_size(0.22)
            .with_lifetime(0.45)
            .with_spread(Vec3::new(0.6, 0.05, 0.6))
            .with_force(ParticleForce::Radial {
                center: feet,
                strength: radius * 9.0,
            })
            .with_color(core)
            .with_secondary_color(edge)
            .with_size_over_life(SizeOverLife::GrowThenFade)
            .with_emissive_intensity(6.0),
    );
    let _ = p.burst(
        ParticleEmitter::new(feet + Vec3::Y * 0.2)
            .with_mode(ParticleMode::Burst)
            .with_shape(ParticleShape::Ring)
            .with_burst_count(80)
            .with_size(0.6)
            .with_lifetime(1.2)
            .with_spread(Vec3::new(radius * 0.4, 0.1, radius * 0.4))
            .with_velocity(Vec3::Y * 0.8)
            .with_velocity_jitter(Vec3::splat(1.2))
            .with_drag(1.5)
            .with_color(rgb(190, 180, 160))
            .with_size_over_life(SizeOverLife::GrowThenFade),
    );
    let d = Decal::new(
        DecalKind::Scorch,
        feet + Vec3::Y * 0.2,
        Vec3::Y,
        radius * 0.9,
    )
    .depth(1.0)
    .opacity(0.5)
    .lifetime(20.0)
    .fade(0.0, 4.0);
    let _ = world.spawn_decal(d);
}

/// The level-up moment: a column of golden light round the hero.
fn level_burst(world: &mut World, feet: Vec3) {
    let p = world.particles_mut();
    let _ = p.burst(
        ParticleEmitter::new(feet)
            .with_mode(ParticleMode::Burst)
            .with_shape(ParticleShape::Ring)
            .with_burst_count(160)
            .with_size(0.12)
            .with_lifetime(2.0)
            .with_spread(Vec3::new(0.9, 0.1, 0.9))
            .with_velocity(Vec3::Y * 3.2)
            .with_velocity_jitter(Vec3::new(0.3, 1.2, 0.3))
            .with_color(rgb(255, 214, 90))
            .with_secondary_color(rgb(255, 255, 220))
            .with_silhouette(ParticleSilhouette::RuneMote)
            .with_emissive_intensity(6.0),
    );
    let _ = p.burst(
        ParticleEmitter::new(feet + Vec3::Y * 1.0)
            .with_mode(ParticleMode::Burst)
            .with_burst_count(1)
            .with_size(3.5)
            .with_lifetime(0.5)
            .with_spread(Vec3::ZERO)
            .with_color(rgb(255, 230, 140))
            .with_size_over_life(SizeOverLife::GrowThenFade)
            .with_emissive_intensity(5.0),
    );
}

/// Golden motes rising round someone getting up.
fn rise(world: &mut World, at: Vec3, colour: Color) {
    let _ = world.particles_mut().burst(
        ParticleEmitter::new(at - Vec3::Y * 0.8)
            .with_mode(ParticleMode::Burst)
            .with_shape(ParticleShape::Ring)
            .with_burst_count(60)
            .with_size(0.09)
            .with_lifetime(1.4)
            .with_spread(Vec3::new(0.7, 0.1, 0.7))
            .with_velocity(Vec3::Y * 1.8)
            .with_velocity_jitter(Vec3::new(0.2, 0.6, 0.2))
            .with_color(colour)
            .with_silhouette(ParticleSilhouette::RuneMote)
            .with_emissive_intensity(4.0),
    );
}

/// A scorch mark on the ground where a bolt knocked someone down.
fn scorch(world: &mut World, at: Vec3, all: &[EntityState], to: NetId) {
    let feet = all
        .iter()
        .find(|e| e.id == to)
        .map_or(at - Vec3::Y * 1.1, pos);
    let d = Decal::new(DecalKind::Scorch, feet + Vec3::Y * 0.2, Vec3::Y, 1.3)
        .angle((at.x * 7.0 + at.z * 3.0).sin() * 3.0)
        .depth(1.0)
        .opacity(0.8)
        .lifetime(25.0)
        .fade(0.0, 4.0);
    let _ = world.spawn_decal(d);
}

/// A golden-age ray gun: a chrome bulb, fins and a glowing green tip, the
/// grip along -Y and the barrel along +Z.
fn ray_gun() -> Mesh {
    use crate::citymesh::{MeshBuilder, Surf};
    use glam::DVec3;
    let mut b = MeshBuilder::new();
    let red = rgb(190, 40, 36);
    b.surf = Surf::Metal;
    b.oriented_box(
        DVec3::new(0.0, 0.0, 0.06),
        DVec3::X,
        DVec3::new(0.025, 0.03, 0.09),
        rgb(200, 204, 214),
    );
    b.oriented_box(
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::X,
        DVec3::new(0.04, 0.04, 0.045),
        rgb(200, 204, 214),
    );
    b.surf = Surf::Plain;
    b.oriented_box(
        DVec3::new(0.0, -0.07, -0.01),
        DVec3::X,
        DVec3::new(0.018, 0.05, 0.022),
        red,
    );
    for k in [-1.0, 1.0] {
        b.oriented_box(
            DVec3::new(0.04 * k, 0.0, 0.07),
            DVec3::X,
            DVec3::new(0.006, 0.035, 0.03),
            red,
        );
    }
    b.surf = Surf::Glow;
    b.oriented_box(
        DVec3::new(0.0, 0.0, 0.16),
        DVec3::X,
        DVec3::new(0.018, 0.018, 0.018),
        rgb(150, 255, 110),
    );
    b.surf = Surf::Plain;
    b.finish()
}

/// A police drone: a navy saucer with a red-and-blue light bar, four
/// rotor rings and a claw underneath (the medical one is white with a red
/// cross).
pub fn drone_mesh(medical: bool) -> Mesh {
    use crate::citymesh::{MeshBuilder, Surf};
    use glam::DVec3;
    let mut b = MeshBuilder::new();
    let (body, trim) = if medical {
        (rgb(236, 236, 228), rgb(200, 40, 40))
    } else {
        (rgb(30, 44, 92), rgb(220, 196, 90))
    };
    b.cylinder(
        DVec3::new(0.0, -0.12, 0.0),
        (0.55, 0.75),
        0.18,
        20,
        true,
        body,
    );
    b.cylinder(
        DVec3::new(0.0, 0.06, 0.0),
        (0.75, 0.4),
        0.16,
        20,
        true,
        body,
    );
    b.dome(DVec3::new(0.0, 0.22, 0.0), 0.32, 14, 5, trim);
    b.surf = Surf::Metal;
    for k in 0..4 {
        let a = std::f64::consts::TAU * (k as f64 / 4.0 + 0.125);
        let u = DVec3::new(a.cos(), 0.0, a.sin());
        b.oriented_box(
            u * 0.85,
            u,
            DVec3::new(0.22, 0.03, 0.05),
            rgb(150, 154, 166),
        );
        b.cylinder(
            u * 1.12 + DVec3::Y * 0.02,
            (0.26, 0.26),
            0.05,
            14,
            false,
            rgb(150, 154, 166),
        );
    }
    // The claw that holds the cuffs.
    b.cylinder(
        DVec3::new(0.0, -0.45, 0.0),
        (0.05, 0.05),
        0.35,
        8,
        true,
        rgb(120, 124, 134),
    );
    b.surf = Surf::Glow;
    if medical {
        b.oriented_box(
            DVec3::new(0.0, 0.22, 0.33),
            DVec3::X,
            DVec3::new(0.14, 0.04, 0.012),
            trim,
        );
        b.oriented_box(
            DVec3::new(0.0, 0.22, 0.33),
            DVec3::X,
            DVec3::new(0.04, 0.14, 0.012),
            trim,
        );
    } else {
        b.oriented_box(
            DVec3::new(-0.16, 0.5, 0.0),
            DVec3::X,
            DVec3::new(0.14, 0.05, 0.08),
            rgb(255, 40, 40),
        );
        b.oriented_box(
            DVec3::new(0.16, 0.5, 0.0),
            DVec3::X,
            DVec3::new(0.14, 0.05, 0.08),
            rgb(60, 110, 255),
        );
    }
    b.cylinder(
        DVec3::new(0.0, -0.2, 0.0),
        (0.6, 0.6),
        0.04,
        20,
        false,
        rgb(120, 220, 255),
    );
    b.surf = Surf::Plain;
    b.finish()
}

/// A stand-in for a villain until its character is built.
pub fn villain_placeholder(kind: EntityKind) -> Mesh {
    let (body, head) = match kind {
        EntityKind::Brute => (rgb(90, 80, 60), rgb(110, 160, 80)),
        EntityKind::Greaser => (rgb(28, 28, 32), rgb(224, 186, 150)),
        EntityKind::Gunsel => (rgb(60, 32, 58), rgb(224, 186, 150)),
        _ => (rgb(236, 234, 226), rgb(224, 186, 150)),
    };
    let mut m = Mesh::new();
    let _ = m.add_box((0.0, 0.45, 0.0), (0.36, 0.9, 0.3), rgb(50, 50, 56));
    let _ = m.add_box((0.0, 1.2, 0.0), (0.6, 0.62, 0.34), body);
    let _ = m.add_box((0.0, 1.66, 0.0), (0.26, 0.28, 0.26), head);
    m
}

fn make_sounds() -> Sounds {
    let clip = |name: &str| -> Sound {
        let (_, bytes) = crate::soundscape::FIGHT_CLIPS
            .iter()
            .find(|(n, _)| *n == name)
            .expect("a fight clip");
        Sound::from_bytes(bytes.to_vec()).unwrap_or_else(|e| {
            eprintln!("fight sound {name}: {e}");
            Sound::tone(440.0, 0.05)
        })
    };
    Sounds {
        boom: clip("boom"),
        bind: clip("bind"),
        shield: clip("shield"),
        fanfare: clip("fanfare"),
        punch: clip("punch"),
        bolt: clip("bolt"),
        ray: clip("ray"),
        charge: clip("charge"),
        whoosh: clip("whoosh"),
        chime: clip("chime"),
        hum: clip("power_hum"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hw_net::CombatState;

    fn villain(id: u32, at: [f64; 3]) -> EntityState {
        EntityState {
            id: NetId(id),
            kind: EntityKind::Brute,
            pos: at,
            combat: Some(CombatState {
                health: 255,
                level: 1,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn tab_picks_nearest_the_crosshair_then_cycles() {
        let all = vec![
            villain(1, [10.0, 0.0, 3.0]),
            villain(2, [10.0, 0.0, 0.0]),
            villain(3, [-10.0, 0.0, 0.0]),
        ];
        let eye = Vec3::new(0.0, 1.1, 0.0);
        let look = Vec3::X;
        let first = pick_target(&all, eye, look, None);
        assert_eq!(first, Some(NetId(2)), "dead ahead first");
        let next = pick_target(&all, eye, look, first);
        assert_eq!(next, Some(NetId(1)));
        // Behind the camera is never picked; the cycle wraps.
        assert_eq!(pick_target(&all, eye, look, next), Some(NetId(2)));
    }

    #[test]
    fn an_attacker_becomes_the_target_when_none_is_set() {
        let me = NetId(9);
        let all = vec![villain(1, [5.0, 0.0, 0.0])];
        let mut punch = Fx {
            tick: 0,
            kind: FxKind::Punch,
            from: NetId(1),
            to: me,
            at: [0.0; 3],
            amount: 12,
            knock: false,
            defeat: false,
            theme: 0,
        };
        assert_eq!(attacker(&punch, Some(me), &all), Some(NetId(1)));
        // A blow on someone else, or a shell landing on nobody, picks no one.
        punch.to = NetId(4);
        assert_eq!(attacker(&punch, Some(me), &all), None);
        punch.to = punch.from;
        punch.kind = FxKind::Shell;
        assert_eq!(attacker(&punch, Some(me), &all), None);
        // Warnings are not attacks.
        punch.to = me;
        punch.kind = FxKind::Warn;
        assert_eq!(attacker(&punch, Some(me), &all), None);
        // A beaten villain is not picked.
        punch.kind = FxKind::Ray;
        let mut down = all.clone();
        down[0].combat.as_mut().unwrap().status = Status::Down;
        assert_eq!(attacker(&punch, Some(me), &down), None);
    }

    #[test]
    fn the_beaten_cannot_be_targeted() {
        let mut v = villain(1, [5.0, 0.0, 0.0]);
        v.combat.as_mut().unwrap().status = Status::Down;
        assert_eq!(pick_target(&[v], Vec3::ZERO, Vec3::X, None), None);
    }

    #[test]
    fn con_colours_follow_the_level_gap() {
        assert_eq!(con_colour(1, 1), egui::Color32::from_rgb(240, 240, 240));
        assert_eq!(con_colour(2, 1), egui::Color32::from_rgb(250, 220, 60));
        assert_eq!(con_colour(1, 5), egui::Color32::from_rgb(150, 150, 150));
    }

    #[test]
    fn the_fight_sounds_are_made() {
        let _ = make_sounds();
        let _ = drone_mesh(true);
        let _ = drone_mesh(false);
        let _ = ray_gun();
    }
}
