//! The fight as the player sees and plays it: Tab targeting nearest the
//! crosshair, the power keys, the target frame with its con colour, health
//! and energy bars, floating numbers, and the feel of every blow (sparks,
//! beams, scorch marks, sounds, camera shake, hit-stop). Knockback and
//! defeat turn characters into Engine ragdolls; defeated villains lie in
//! glowing energy cuffs until the police drone takes them away.
//!
//! The server decides everything; this only shows the entity states and
//! fight events it sends, and turns keys into input.

use crate::chat::{self, Said, Voice};
use crate::tray::{self, Tray};
use engine::prelude::*;
use engine::ui::egui;
use glam::{Mat4, Vec3};
use hw_core::NetId;
use hw_net::{
    fizzle, EntityKind, EntityState, Fx, FxKind, Power, Sheet, Status, EFFECT_CHARMED,
    EFFECT_CHILLED, EFFECT_FROZEN, EFFECT_HELD, EFFECT_RALLIED, EFFECT_SHIELD, EFFECT_SUPER_SPEED,
};
use hw_sim::combat::{power_spec, EnemyKind, Kit, POWERS};
use hw_sim::progress::{is_movement, themes};
use std::collections::BTreeMap;

/// Why `power` (in `theme`) did not fire, in a few words for the middle
/// of the screen (`why`: a [`fizzle`] reason).
pub fn fizzle_text(power: Power, theme: u8, why: u16) -> String {
    let name = themes(power)[theme.min(1) as usize].name;
    match why {
        fizzle::OUT_OF_RANGE if power == Power::Strike => "Too far to punch".into(),
        fizzle::OUT_OF_RANGE => format!("{name}: out of range"),
        fizzle::NOT_SEEN => format!("{name}: can't see them from here"),
        fizzle::NO_TARGET if power == Power::Strike => "Nobody close enough to punch".into(),
        fizzle::NO_TARGET => format!("{name}: no target (Tab picks one)"),
        fizzle::RECHARGING => format!("{name} is still recharging"),
        fizzle::ENERGY => format!("Not enough energy for {name}"),
        fizzle::NO_SPOT if power == Power::Teleport => "Nowhere to land there".into(),
        fizzle::NO_SPOT => "Nobody to rescue nearby".into(),
        fizzle::BUSY => format!("{name}: too soon after the last power"),
        _ => format!("{name} did not fire"),
    }
}

/// Villains further than this cannot be picked with Tab.
pub const TARGET_RANGE: f32 = 60.0;
/// Clicking a villain picks it out to this far (m): further than Tab, for
/// one you can see down the street.
pub const CLICK_RANGE: f32 = 150.0;
/// Villains within this angle of the crosshair count as in view (degrees):
/// Tab goes through them first, and aimed powers with no target pick only
/// from them.
pub const TARGET_CONE: f32 = 55.0;
/// After this many seconds without Tab, the next Tab starts the cycle again
/// from the nearest villain.
const CYCLE_RESET: f32 = 4.0;
/// Fight events older than this (seconds) are not shown any more.
const LATE: f64 = 0.6;
/// A struck fighter rocks back this far (m) and leans this much (degrees)
/// from a heavy blow (lighter ones less), over this many seconds.
const RECOIL_DISTANCE: f32 = 0.32;
const RECOIL_LEAN: f32 = 14.0;
const RECOIL_SECONDS: f32 = 0.32;
/// The force field's size round a shielded person (m; a little taller).
const SHIELD_RADIUS: f32 = 1.05;
/// A blow doing this much damage or more counts as heavy.
const HEAVY_BLOW: u16 = 25;

/// A grenadier's bomb in the air, as drawn.
struct Lobbed {
    ent: EntityId,
    from: Vec3,
    to: Vec3,
    /// Seconds since thrown, and in the air.
    age: f32,
    flight: f32,
}

impl Lobbed {
    /// Where it is `t` seconds after the throw: a high arc.
    fn at(&self, t: f32) -> Vec3 {
        let u = (t / self.flight).clamp(0.0, 1.0);
        let rise = 2.5 + self.from.distance(self.to) * 0.25;
        self.from.lerp(self.to, u) + Vec3::Y * (4.0 * rise * u * (1.0 - u))
    }
}

/// The recoil's shape over time, 0 to 1: a snap back, then easing home.
fn recoil_shape(t: f32) -> f32 {
    const SNAP: f32 = 0.05;
    if t < SNAP {
        t / SNAP
    } else {
        (1.0 - (t - SNAP) / (RECOIL_SECONDS - SNAP))
            .max(0.0)
            .powi(2)
    }
}

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

/// The power tray's width at the bottom of a screen `screen_w` points
/// wide, and one slot's width.
fn tray_width(screen_w: f32) -> (f32, f32) {
    let room = (screen_w - 40.0 - 18.0) / 10.0 - 6.0;
    let slot_w = room.clamp(64.0, 104.0);
    (12.0 + 10.0 * (slot_w + 6.0) + 6.0, slot_w)
}

/// Room for a window bottom left beside the power tray (points across),
/// and how far up from the bottom the tray and its key hints reach, for a
/// window that has to sit above it instead. `two_rows`: the Alt row shows.
pub fn beside_tray(screen_w: f32, two_rows: bool) -> (f32, f32) {
    let (w, _) = tray_width(screen_w);
    let rows = if two_rows { 2.0 } else { 1.0 };
    (
        (screen_w - w) * 0.5 - 20.0,
        112.0 + (rows - 1.0) * ROW_STEP + 26.0,
    )
}

/// A speaker's name for the chat log: their kind ("Mutant Brute"), or
/// `otherwise` when not known.
fn speaker_name(speaker: Option<&EntityState>, otherwise: &str) -> String {
    let name = speaker.map_or("", |s| enemy_name(s.kind).0);
    if name.is_empty() {
        otherwise.to_string()
    } else {
        name.to_string()
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
    freeze: Sound,
    shatter: Sound,
    teleport: Sound,
    bubble: Sound,
    hypno: Sound,
    robot: Sound,
}

/// How far the tray's second row sits above the first (px).
const ROW_STEP: f32 = 46.0;

/// Powers aimed at the target (the others work round the hero).
pub fn aimed(p: Power) -> bool {
    matches!(
        p,
        Power::Strike
            | Power::Bolt
            | Power::Hold
            | Power::Beam
            | Power::Meteor
            | Power::Freeze
            | Power::Hypno
    )
}

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
        (Power::Mend, 0) => ([150, 255, 170], [40, 200, 110]),
        (Power::Mend, _) => ([255, 236, 150], [255, 190, 70]),
        (Power::Beam, 0) => ([255, 120, 200], [230, 40, 120]),
        (Power::Beam, _) => ([210, 240, 255], [120, 170, 255]),
        (Power::Meteor, 0) => ([255, 255, 235], [140, 220, 255]),
        (Power::Meteor, _) => ([255, 190, 110], [255, 90, 40]),
        (Power::Nova, 0) => ([255, 250, 200], [255, 140, 40]),
        (Power::Nova, _) => ([230, 200, 150], [170, 110, 60]),
        (Power::Rally, 0) => ([255, 120, 100], [230, 50, 50]),
        (Power::Rally, _) => ([255, 225, 140], [240, 180, 60]),
        (Power::Vitality, 0) => ([200, 160, 255], [120, 80, 230]),
        (Power::Vitality, _) => ([255, 170, 130], [220, 100, 70]),
        (Power::Invulnerable, 0) => ([220, 230, 240], [150, 165, 185]),
        (Power::Invulnerable, _) => ([140, 190, 255], [60, 110, 230]),
        (Power::SuperSpeed, 0) => ([255, 240, 120], [255, 200, 40]),
        (Power::SuperSpeed, _) => ([255, 150, 90], [230, 80, 40]),
        (Power::Freeze, 0) => ([200, 240, 255], [90, 180, 255]),
        (Power::Freeze, _) => ([220, 250, 255], [150, 130, 255]),
        (Power::Teleport, 0) => ([180, 255, 230], [40, 210, 190]),
        (Power::Teleport, _) => ([235, 190, 255], [150, 70, 240]),
        (Power::Bubble, 0) => ([170, 225, 255], [255, 170, 230]),
        (Power::Bubble, _) => ([255, 225, 140], [255, 190, 70]),
        (Power::Sidekick, 0) => ([200, 230, 255], [230, 60, 50]),
        (Power::Sidekick, _) => ([255, 215, 120], [60, 110, 220]),
        (Power::Hypno, 0) => ([255, 120, 220], [170, 110, 255]),
        (Power::Hypno, _) => ([255, 190, 235], [210, 120, 255]),
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
    /// A click picked a villain out of punching reach: the held left
    /// button must not also punch.
    pub picked: bool,
    pub target: Option<NetId>,
    pub revive: bool,
    pub hospital: bool,
}

pub struct FightView {
    /// The villain picked with Tab.
    pub target: Option<NetId>,
    /// Villains Tab has already gone through in this cycle, and the clock
    /// time of the last Tab.
    cycled: Vec<NetId>,
    last_tab: f32,
    sounds: Option<Sounds>,
    floaters: Vec<Floater>,
    /// Words said out loud since the game last took them, for the chat
    /// window's Local channel (see `chat.rs`).
    pub said: Vec<crate::chat::Said>,
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
    cooldown: [f32; POWERS],
    /// Glowing rings under held villains.
    holds: BTreeMap<NetId, DecalId>,
    /// Our hero stands in a Rally aura (powers recharge sooner).
    rallied: bool,
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
    /// A tray slot clicked with the mouse: fired like its key.
    clicked: Option<Power>,
    /// The ice round each frozen villain.
    ices: BTreeMap<NetId, EntityId>,
    /// The force field round each shielded character.
    shields: BTreeMap<NetId, EntityId>,
    /// Rescue bubbles in the air.
    pub bubbles: crate::powerfx::Bubbles,
    /// Rescue bubbles started this frame: their number, where, and whether
    /// for the hurt one (the radio puts the person in, see
    /// `Radio::into_bubble`).
    pub bubbled: Vec<(u32, Vec3, bool)>,
    /// Grenadiers' bombs in the air.
    lobs: Vec<Lobbed>,
    /// The hint about shields was shown.
    told_shield: bool,
    /// The solid town: Tab and aimed powers pick only villains in sight.
    pub solids: Option<std::sync::Arc<hw_sim::collision::Obstacles>>,
    /// A short recoil per struck villain: (direction, seconds since).
    recoils: BTreeMap<NetId, (Vec3, f32)>,
}

impl Default for FightView {
    fn default() -> Self {
        Self::new()
    }
}

/// The hero's powers as the tray shows them: what they own, where each sits,
/// and whether Alt (the second row) is held.
#[derive(Clone, Copy)]
pub struct Bar<'a> {
    pub sheet: Option<&'a Sheet>,
    pub tray: &'a Tray,
    pub alt: bool,
    /// The ray under the mouse (from the eye), when a click could pick a
    /// villain (not over a window).
    pub click_ray: Option<(Vec3, Vec3)>,
}

impl FightView {
    pub fn new() -> Self {
        Self {
            target: None,
            cycled: Vec::new(),
            last_tab: f32::NEG_INFINITY,
            sounds: None,
            said: Vec::new(),
            floaters: Vec::new(),
            shake: 0.0,
            hurt: 0.0,
            hitstop: BTreeMap::new(),
            actions: BTreeMap::new(),
            statuses: BTreeMap::new(),
            cuffs: BTreeMap::new(),
            marker: None,
            cooldown: [0.0; POWERS],
            holds: BTreeMap::new(),
            rallied: false,
            sparkle: 0.0,
            note: None,
            hums: BTreeMap::new(),
            guns: std::collections::HashMap::new(),
            cross: false,
            clock: 0.0,
            muzzles: BTreeMap::new(),
            clicked: None,
            ices: BTreeMap::new(),
            shields: BTreeMap::new(),
            bubbles: Default::default(),
            bubbled: Vec::new(),
            lobs: Vec::new(),
            told_shield: false,
            solids: None,
            recoils: BTreeMap::new(),
        }
    }

    /// Can our hero (or, with none, the camera at `eye`) see `e`, with no
    /// wall between chest and chest? The server checks the same line.
    pub fn sees(&self, me: Option<&EntityState>, eye: Vec3, e: &EntityState) -> bool {
        let Some(solids) = &self.solids else {
            return true;
        };
        let chest = |p: Vec3| p + Vec3::Y * (hw_sim::collision::BODY_HEIGHT as f32 * 0.6);
        let from = me.map_or(eye, |m| chest(pos(m)));
        hw_sim::combat::in_sight(solids, from.as_dvec3(), chest(pos(e)).as_dvec3())
    }

    /// Keys: Tab picks the nearest villain in view, and each further Tab the
    /// next nearest one not picked yet in this cycle, 1 or left mouse strikes, 2 or Q fires an energy bolt, 3 to 6
    /// the other powers, E held revives a downed teammate, H calls the
    /// medical drone when you are down.
    pub fn input(
        &mut self,
        input: &engine::input::Input,
        me: Option<&EntityState>,
        all: &[EntityState],
        eye: Vec3,
        look: Vec3,
        powers: Bar,
    ) -> FightInput {
        let Bar {
            sheet,
            tray,
            alt,
            click_ray,
        } = powers;
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
            if self.clock - self.last_tab > CYCLE_RESET {
                self.cycled.clear();
            }
            self.last_tab = self.clock;
            let from = me.map_or(eye, pos);
            let mut cycled = std::mem::take(&mut self.cycled);
            let seen = |e: &EntityState| self.sees(me, eye, e);
            let next = cycle_target(all, from, eye, look, self.target, &mut cycled, seen);
            self.cycled = cycled;
            self.target = next;
            if self.target.is_none() {
                self.say("No villain in sight");
            }
        }
        let down = me
            .and_then(|m| m.combat)
            .is_some_and(|c| matches!(c.status, Status::Down | Status::Carried));
        self.rallied = me
            .and_then(|m| m.combat)
            .is_some_and(|c| c.effects & EFFECT_RALLIED != 0);
        // A tray slot's key (Alt for the second row), and Strike and Energy
        // Bolt on the left mouse button and Q wherever they sit.
        let mut fired: Vec<(Power, bool)> = tray
            .filled()
            .filter(|&(n, _)| tray::alt(n) == alt && input.pressed(tray::key(n)))
            .map(|(_, p)| (p, true))
            .chain(self.clicked.take().map(|p| (p, true)))
            .collect();
        // A click on a villain picks it (up close it also punches it); a
        // click anywhere else punches as before.
        let mut picked = false;
        if input.mouse_clicked(MouseButton::Left) {
            let under = click_ray
                .and_then(|(from, dir)| villain_under(all, from, dir, |e| self.sees(me, eye, e)));
            match under {
                Some(id) => {
                    if self.target != Some(id) {
                        crate::devlog::add(format!("client: clicked villain {id:?}: target"));
                    }
                    self.target = Some(id);
                    let near = me
                        .zip(all.iter().find(|e| e.id == id))
                        .is_some_and(|(m, t)| {
                            pos(m).distance(pos(t)) <= power_spec(Power::Strike).range as f32
                        });
                    if near {
                        fired.push((Power::Strike, false));
                    } else {
                        picked = true;
                    }
                }
                None => fired.push((Power::Strike, false)),
            }
        }
        out.picked = picked;
        if input.pressed(Key::Q) {
            fired.push((Power::Bolt, false));
        }
        for (power, keyed) in fired {
            if down {
                crate::devlog::add(format!(
                    "client: {power:?} pressed, not sent: our hero is down"
                ));
                continue;
            }
            if !kit.has(power) {
                crate::devlog::add(format!(
                    "client: {power:?} pressed, not sent: not in the kit"
                ));
                if keyed {
                    self.say(&format!(
                        "You don't have {} yet (B: powers)",
                        power_spec(power).name
                    ));
                }
                continue;
            }
            let spec = power_spec(power);
            let theme = kit.theme(power).min(1);
            let carrying = me
                .and_then(|m| m.combat)
                .is_some_and(|c| c.effects & hw_net::EFFECT_CARRYING != 0);
            let aimed = aimed(power) || (power == Power::Lift && carrying);
            // No target yet: take the one nearest the crosshair.
            if aimed && self.target.is_none() {
                self.target = pick_target(all, eye, look, |e| self.sees(me, eye, e));
            }
            if power == Power::Hypno {
                let immune = self
                    .target
                    .and_then(|t| all.iter().find(|e| e.id == t))
                    .and_then(|t| hw_sim::combat::EnemyKind::from_entity(t.kind))
                    .is_some_and(|k| !hw_sim::combat::charmable(k));
                if immune {
                    self.say("Too strong-willed to hypnotise");
                }
            }
            if let Some(m) = me {
                if let Some(t) = self
                    .target
                    .filter(|_| aimed)
                    .and_then(|t| all.iter().find(|e| e.id == t))
                {
                    let d = pos(m).distance(pos(t));
                    if d > spec.range as f32 {
                        self.say(&fizzle_text(power, theme, fizzle::OUT_OF_RANGE));
                    } else if !self.sees(me, eye, t) {
                        self.say(&fizzle_text(power, theme, fizzle::NOT_SEEN));
                    }
                }
                let energy = m.combat.map_or(0.0, |c| c.energy as f32 / 255.0 * 100.0);
                // Longer than the server waits for it: it will not fire.
                let left = self.cooldown[(power.code() - 1) as usize];
                if !carrying && left as f64 > hw_sim::combat::RECHARGE_WAIT {
                    self.say(&format!(
                        "{} recharges in {:.0} s",
                        themes(power)[theme as usize].name,
                        left.ceil()
                    ));
                } else if !carrying && energy + 0.5 < spec.cost {
                    self.say(&fizzle_text(power, theme, fizzle::ENERGY));
                }
            }
            let about = match self.target.and_then(|t| all.iter().find(|e| e.id == t)) {
                Some(t) => format!(
                    "target {:?} {:?} {:.1} m away, {:?}, in sight here {}",
                    t.id,
                    t.kind,
                    me.map_or(f32::NAN, |m| pos(m).distance(pos(t))),
                    t.combat.map(|c| c.status),
                    self.sees(me, eye, t)
                ),
                None => "no target".into(),
            };
            crate::devlog::add(format!(
                "client: {power:?} pressed ({}), sent as power {} ({about})",
                if keyed { "key" } else { "button or Q" },
                power.code()
            ));
            out.power = power.code();
        }
        out.target = self.target;
        out.revive = input.down(Key::E) && !down;
        out.hospital = input.pressed(Key::H) && down;
        out
    }

    /// Notes a power that did not fire in the debug log with where everyone
    /// stood (server and screen), so a report can be traced.
    fn log_fizzle(&self, text: &str, e: &Fx, me: Option<NetId>, all: &[EntityState]) {
        use std::fmt::Write as _;
        let mine = me.and_then(|m| all.iter().find(|x| x.id == m));
        let mut line = format!("hero at server {:.1?}", e.at);
        if let Some(m) = mine {
            let _ = write!(line, ", on screen {:.1?}", m.pos);
        }
        match self.target.and_then(|t| all.iter().find(|x| x.id == t)) {
            Some(t) => {
                let d = mine.map_or(f32::NAN, |m| pos(m).distance(pos(t)));
                let seen = self.sees(mine, pos(t), t);
                let _ = write!(
                    line,
                    "; target {:?} {:?} at {:.1?}, {d:.1} m, clear line on screen {seen}",
                    t.id, t.kind, t.pos
                );
            }
            None => line.push_str("; no target"),
        }
        crate::devlog::add(format!("client: told \"{text}\": {line}"));
    }

    /// A short line in the middle of the screen.
    pub fn say(&mut self, text: &str) {
        self.note = Some((text.to_string(), 1.4));
    }

    /// The clip to show for `id` instead of its walk, if any.
    /// How far a struck fighter is pushed back from its place now, and
    /// how far it leans back (degrees, as a negative pitch).
    pub fn recoil(&self, id: NetId) -> Option<(Vec3, f32)> {
        let &(push, t) = self.recoils.get(&id)?;
        let k = recoil_shape(t);
        Some((push * RECOIL_DISTANCE * k, -RECOIL_LEAN * push.length() * k))
    }

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
            let late = (now_tick - e.tick as f64) * hw_sim::DT > LATE;
            if me == Some(e.from) && me != Some(e.to) {
                crate::devlog::add(format!(
                    "client: server says {:?} from us at {:?} (tick {}, {} dmg){}",
                    e.kind,
                    e.to,
                    e.tick,
                    e.amount,
                    if late { ", too late to show" } else { "" }
                ));
            }
            if late {
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
            // Whoever takes a blow and stays up rocks back from it.
            let struck = matches!(
                e.kind,
                FxKind::Strike
                    | FxKind::Bolt
                    | FxKind::Punch
                    | FxKind::Ray
                    | FxKind::ThrowHit
                    | FxKind::Shell
                    | FxKind::Stomp
                    | FxKind::Pulse
                    | FxKind::Leap
                    | FxKind::Beam
                    | FxKind::Meteor
                    | FxKind::Nova
                    | FxKind::Shockwave
                    | FxKind::Shatter
                    | FxKind::Bomb
            );
            if struck && e.amount > 0 && e.from != e.to && !e.knock && !e.defeat {
                let d = at - origin;
                let away = Vec3::new(d.x, 0.0, d.z).normalize_or_zero();
                let heavy = e.amount >= HEAVY_BLOW;
                self.recoils
                    .insert(e.to, (away * if heavy { 1.0 } else { 0.6 }, 0.0));
            }
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
                    if mine {
                        self.say("Press 5 again to throw it where you aim");
                    }
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
                FxKind::Mend => {
                    let (core, edge) = theme_colours(Power::Mend, e.theme);
                    let feet = all.iter().find(|x| x.id == e.to).map_or(at, pos);
                    rise(world, feet + Vec3::Y * 0.9, core);
                    glow_at(world, at, edge, 0.6);
                    if e.from == e.to {
                        self.act(e.from, "spell cast", 0.6);
                        self.play(world, |s| &s.chime, at, 1.0, 0.85);
                        if mine {
                            self.recharging(Power::Mend);
                        }
                    } else {
                        beam(world, origin, at, core, edge);
                    }
                    if (on_me || mine) && e.amount > 0 {
                        self.float(
                            at + Vec3::Y * 0.7,
                            format!("+{}", e.amount),
                            theme_ui_colour(Power::Mend, e.theme),
                            false,
                        );
                    }
                }
                FxKind::Beam => {
                    let (core, edge) = theme_colours(Power::Beam, e.theme);
                    if e.from == e.to {
                        // The beam itself, out to where it stopped: a thick
                        // core inside a wider sheath, and a flash at the end.
                        self.act(e.from, "spell cast", 0.7);
                        beam(world, origin, at, core, edge);
                        let side = (at - origin).normalize_or_zero().cross(Vec3::Y) * 0.12;
                        beam(world, origin + side, at + side, edge, core);
                        beam(world, origin - side, at - side, edge, core);
                        impact(world, at, core, 1.0);
                        let pitch = if e.theme == 1 { 0.6 } else { 0.75 };
                        self.play(world, |s| &s.bolt, origin, 1.0, pitch);
                        if mine {
                            self.shake = self.shake.max(0.3);
                            self.recharging(Power::Beam);
                        }
                    } else {
                        impact(world, at, core, 1.1);
                        scorch(world, at, all, e.to);
                        self.blow(e, at, mine, on_me, theme_ui_colour(Power::Beam, e.theme));
                    }
                }
                FxKind::MeteorCall => {
                    // A target ring on the ground and a glint high above.
                    let (core, edge) = theme_colours(Power::Meteor, e.theme);
                    self.act(e.from, "spell cast", 0.6);
                    let r = power_spec(Power::Meteor).area as f32;
                    let ring = Decal::new(DecalKind::Power, at + Vec3::Y * 0.2, Vec3::Y, r * 2.0)
                        .tint(edge)
                        .emission(3.0)
                        .depth(1.5)
                        .lifetime(power_spec(Power::Meteor).lasts as f32 + 0.2)
                        .fade(0.1, 0.2);
                    let _ = world.spawn_decal(ring);
                    rings(world, at + Vec3::Y * 0.4, core, r / 2.0);
                    glow_at(world, at + Vec3::Y * 40.0, core, 1.5);
                    self.play(world, |s| &s.charge, at, 1.0, 0.6);
                    if mine {
                        self.recharging(Power::Meteor);
                    }
                }
                FxKind::Meteor => {
                    let (core, edge) = theme_colours(Power::Meteor, e.theme);
                    if e.from == e.to {
                        // The strike: a column from the sky, a ring over the
                        // ground and a crater.
                        let top = at + Vec3::Y * 45.0;
                        beam(world, top, at, core, edge);
                        for k in 0..4 {
                            let a = k as f32 * std::f32::consts::FRAC_PI_2;
                            let off = Vec3::new(a.cos(), 0.0, a.sin()) * 0.35;
                            beam(world, top + off, at + off, edge, core);
                        }
                        shockwave(world, at, core, edge, power_spec(Power::Meteor).area as f32);
                        impact(world, at + Vec3::Y * 0.5, core, 2.4);
                        self.play(world, |s| &s.boom, at, 1.0, 0.7);
                        self.shake = self.shake.max(if mine { 1.0 } else { 0.5 });
                    } else {
                        impact(world, at, core, 0.9);
                        self.blow(e, at, mine, on_me, theme_ui_colour(Power::Meteor, e.theme));
                    }
                }
                FxKind::Nova => {
                    let (core, edge) = theme_colours(Power::Nova, e.theme);
                    if e.from == e.to {
                        let feet = all.iter().find(|x| x.id == e.from).map_or(at, pos);
                        let r = power_spec(Power::Nova).area as f32;
                        // A small sun, then two rings racing out.
                        impact(world, feet + Vec3::Y * 1.2, core, 4.0);
                        level_burst(world, feet);
                        shockwave(world, feet, core, edge, r);
                        shockwave(world, feet, edge, core, r * 0.6);
                        self.act(e.from, "spell cast", 0.9);
                        self.play(world, |s| &s.boom, feet, 1.0, 0.55);
                        self.play(world, |s| &s.boom, feet, 0.7, 0.8);
                        if mine {
                            self.shake = self.shake.max(1.2);
                            self.recharging(Power::Nova);
                        } else {
                            self.shake = self.shake.max(0.5);
                        }
                    } else {
                        impact(world, at, core, 1.0);
                        self.blow(e, at, mine, on_me, theme_ui_colour(Power::Nova, e.theme));
                    }
                }
                FxKind::Freeze => {
                    let (core, edge) = theme_colours(Power::Freeze, e.theme);
                    if e.from == e.to {
                        // The cone, out to where it stopped.
                        self.act(e.from, "spell cast", 0.6);
                        crate::powerfx::frost_cone(
                            world,
                            origin,
                            at,
                            core,
                            edge,
                            power_spec(Power::Freeze).area as f32,
                        );
                        self.play(
                            world,
                            |s| &s.freeze,
                            origin,
                            1.0,
                            if e.theme == 1 { 0.85 } else { 1.0 },
                        );
                        if mine {
                            self.recharging(Power::Freeze);
                        }
                    } else {
                        crate::powerfx::chill(world, at);
                        let ui = theme_ui_colour(Power::Freeze, e.theme);
                        self.blow(e, at, mine, on_me, ui);
                        if e.knock {
                            self.float(at + Vec3::Y * 0.9, "Frozen!".into(), ui, false);
                        }
                    }
                }
                FxKind::Shatter => {
                    crate::powerfx::shatter(world, at);
                    self.play(world, |s| &s.shatter, at, 1.0, 1.0);
                    self.float(
                        at + Vec3::Y * 1.1,
                        "Shattered!".into(),
                        egui::Color32::from_rgb(200, 240, 255),
                        false,
                    );
                }
                FxKind::TeleportOut | FxKind::TeleportIn => {
                    let (core, edge) = theme_colours(Power::Teleport, e.theme);
                    let arriving = e.kind == FxKind::TeleportIn;
                    crate::powerfx::teleport(world, at, core, edge, arriving);
                    if arriving {
                        self.play(
                            world,
                            |s| &s.teleport,
                            at,
                            1.0,
                            if e.theme == 1 { 0.8 } else { 1.0 },
                        );
                        if mine {
                            self.recharging(Power::Teleport);
                        }
                    }
                }
                FxKind::Bubble => {
                    let gold = e.theme == 1;
                    let (core, edge) = theme_colours(Power::Bubble, e.theme);
                    self.act(e.from, "spell cast", 0.6);
                    beam(world, origin, at + Vec3::Y * 1.0, core, edge);
                    // Who goes in is the rescue's (see `Radio::into_bubble`).
                    if let Some(key) = self.bubbles.start(world, at, gold, e.from.0) {
                        self.bubbled.push((key, at, e.defeat));
                    }
                    self.play(world, |s| &s.bubble, at, 1.0, if gold { 0.85 } else { 1.0 });
                    if mine {
                        self.recharging(Power::Bubble);
                        self.say(if e.defeat {
                            "Off to the ambulance!"
                        } else {
                            "Floated to safety!"
                        });
                    }
                }
                FxKind::BubbleLand => {
                    self.bubbles.land(e.from.0, at);
                }
                FxKind::SidekickCall => {
                    let (core, edge) = theme_colours(Power::Sidekick, e.theme);
                    if e.from == e.to {
                        // Called: it beams in at the hero's side.
                        self.act(e.from, "spell cast", 0.5);
                        crate::powerfx::teleport(world, at, core, edge, true);
                        self.play(world, |s| &s.robot, at, 1.0, 1.0);
                        if mine {
                            self.recharging(Power::Sidekick);
                        }
                    } else if all
                        .iter()
                        .any(|x| x.id == e.to && x.kind == EntityKind::Sidekick)
                        && at.distance(all.iter().find(|x| x.id == e.to).map_or(at, pos)) > 3.0
                    {
                        // Sent at a villain: a cheerful beep from the hero.
                        self.play(world, |s| &s.robot, at, 0.6, 1.25);
                    } else {
                        // Beamed back to its hero's side.
                        crate::powerfx::teleport(world, at, core, edge, true);
                        self.play(world, |s| &s.teleport, at, 0.6, 1.3);
                    }
                }
                FxKind::Hypno => {
                    let (core, edge) = theme_colours(Power::Hypno, e.theme);
                    self.act(e.from, "spell cast", 0.6);
                    beam(world, origin, at, core, edge);
                    crate::powerfx::spiral(world, at + Vec3::Y * 0.9, core, edge, 1.0);
                    self.play(
                        world,
                        |s| &s.hypno,
                        at,
                        1.0,
                        if e.theme == 1 { 1.2 } else { 1.0 },
                    );
                    let ui = theme_ui_colour(Power::Hypno, e.theme);
                    self.float(at + Vec3::Y * 1.0, "Hypnotised!".into(), ui, false);
                    if mine {
                        self.recharging(Power::Hypno);
                        if self.target == Some(e.to) {
                            self.target = None;
                        }
                    }
                }
                FxKind::Block => {
                    // Clang: the blow glances off the shield.
                    sparks(
                        world,
                        at,
                        (origin - at).normalize_or_zero(),
                        30,
                        rgb(220, 235, 255),
                    );
                    self.play(world, |s| &s.punch, at, 1.0, 1.7);
                    self.play(world, |s| &s.shield, at, 0.6, 1.4);
                    self.float(
                        at + Vec3::Y * 0.5,
                        "Blocked!".into(),
                        egui::Color32::from_rgb(200, 215, 235),
                        mine,
                    );
                    if mine && !self.told_shield {
                        self.told_shield = true;
                        self.note = Some((
                            "The shield stops blows from afar: get in close, or go round it".into(),
                            5.0,
                        ));
                    }
                }
                FxKind::Lob => {
                    // A bomb lobbed high, and a red ring where it will burst.
                    self.act(e.from, "hook", 0.8);
                    let flight = e.amount as f32 / 1000.0;
                    let ring = Decal::new(
                        DecalKind::Power,
                        at + Vec3::Y * 0.2,
                        Vec3::Y,
                        hw_sim::combat::BOMB_AREA as f32 * 2.0,
                    )
                    .tint(rgb(255, 70, 40))
                    .emission(4.0)
                    .depth(1.5)
                    .lifetime(flight + 0.1)
                    .fade(0.15, 0.1);
                    let _ = world.spawn_decal(ring);
                    rings(world, at + Vec3::Y * 0.3, rgb(255, 90, 50), 0.8);
                    self.play(world, |s| &s.whoosh, origin, 0.8, 0.8);
                    if let Ok(ent) = world.place(
                        crate::factionview::bomb_mesh(),
                        Place::new(origin.x, origin.y, origin.z),
                    ) {
                        self.lobs.push(Lobbed {
                            ent,
                            from: origin,
                            to: at + Vec3::Y * 0.15,
                            age: 0.0,
                            flight,
                        });
                    }
                    if on_me {
                        self.say("Bomb! Step out of the red ring");
                    }
                }
                FxKind::Bomb => {
                    let fire = rgb(255, 140, 50);
                    if e.from == e.to {
                        impact(world, at + Vec3::Y * 0.5, fire, 2.0);
                        shockwave(
                            world,
                            at,
                            fire,
                            rgb(255, 220, 120),
                            hw_sim::combat::BOMB_AREA as f32,
                        );
                        let d = Decal::new(DecalKind::Scorch, at + Vec3::Y * 0.2, Vec3::Y, 2.4)
                            .angle((at.x * 5.0 + at.z * 3.0).sin() * 3.0)
                            .depth(1.0)
                            .opacity(0.85)
                            .lifetime(25.0)
                            .fade(0.0, 4.0);
                        let _ = world.spawn_decal(d);
                        self.play(world, |s| &s.boom, at, 1.0, 1.25);
                        let near = me
                            .and_then(|m| all.iter().find(|x| x.id == m))
                            .is_some_and(|m| pos(m).distance(at) < 12.0);
                        if near {
                            self.shake = self.shake.max(0.6);
                        }
                    } else {
                        self.blow(e, at, mine, on_me, egui::Color32::from_rgb(255, 150, 60));
                    }
                }
                FxKind::Patch => {
                    // The orderly's mending ray: green, and health back.
                    let green = rgb(120, 255, 160);
                    self.act(e.from, "pistol shoot", 0.5);
                    beam(world, origin, at, green, rgb(220, 255, 230));
                    rings(world, at, green, 0.7);
                    self.play(world, |s| &s.chime, at, 0.7, 1.5);
                    if e.amount > 0 {
                        self.float(
                            at + Vec3::Y * 0.4,
                            format!("+{}", e.amount),
                            egui::Color32::from_rgb(120, 240, 150),
                            false,
                        );
                    }
                }
                FxKind::Fizzle => {
                    // A power we asked for did not fire: say why.
                    if on_me {
                        if let Some(p) = Power::from_code(e.theme) {
                            let theme = ((e.amount >> 8) as u8).min(1);
                            let text = fizzle_text(p, theme, e.amount & 0xff);
                            self.log_fizzle(&text, e, me, all);
                            self.say(&text);
                        }
                    }
                }
                FxKind::Careful => {
                    // The fight is won and the street still stands.
                    if on_me {
                        self.float(
                            at + Vec3::Y * 1.3,
                            "Careful work!".into(),
                            egui::Color32::from_rgb(140, 230, 170),
                            true,
                        );
                        self.note = Some((
                            format!("Careful work: the street is spared. +{} karma", e.amount),
                            4.0,
                        ));
                        self.play(world, |s| &s.chime, at, 0.7, 1.1);
                    }
                }
                FxKind::HypnoEnd => {
                    self.float(
                        at + Vec3::Y * 1.0,
                        "Snaps out of it!".into(),
                        egui::Color32::from_rgb(255, 170, 120),
                        false,
                    );
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
                    let speaker = all.iter().find(|x| x.id == e.from);
                    let head = speaker.map_or(at, pos);
                    self.speak(
                        head + Vec3::Y * 2.3,
                        line.to_string(),
                        egui::Color32::from_rgb(255, 120, 90),
                    );
                    self.said.push(Said {
                        at: head,
                        range: chat::SHOUT_RANGE,
                        who: speaker_name(speaker, "A gang member"),
                        text: line.to_string(),
                        voice: Voice::Villain,
                    });
                    if on_me {
                        self.note = Some(("You're on their turf: back off, or fight.".into(), 3.0));
                    }
                }
                FxKind::Calmed => {
                    // Mole folk a hero talked down thank them and go home.
                    const THANKS: [&str; 3] = [
                        "You are kind, surface friend. We go home now.",
                        "The drills frightened us... thank you.",
                        "Molehaven will hear of you, friend.",
                    ];
                    let line = THANKS[(e.from.0 as usize + e.tick as usize / 7) % THANKS.len()];
                    let speaker = all.iter().find(|x| x.id == e.from);
                    let head = speaker.map_or(at, pos);
                    self.speak(
                        head + Vec3::Y * 2.3,
                        line.to_string(),
                        egui::Color32::from_rgb(200, 170, 255),
                    );
                    self.said.push(Said {
                        at: head,
                        range: chat::SAY_RANGE,
                        who: speaker_name(speaker, "A mole person"),
                        text: line.to_string(),
                        voice: Voice::Folk,
                    });
                    if on_me {
                        self.note = Some(("Molehaven likes you better now.".into(), 4.0));
                    }
                }
                FxKind::Thanked => {
                    // Someone saved (a mugging's victim, captives) thanks
                    // their hero, a hand on the heart, before going.
                    const THANKS: [&str; 6] = [
                        "Thank you, hero! I was so scared!",
                        "You showed those hoods! Thank you!",
                        "Gee whiz, thanks! I'll tell everyone about you!",
                        "My hero! Thank you so much!",
                        "They took nothing, thanks to you. Bless you!",
                        "Swell work! This town's lucky to have you.",
                    ];
                    let line = THANKS[e.amount as usize % THANKS.len()];
                    let speaker = all.iter().find(|x| x.id == e.from);
                    let head = speaker.map_or(at, pos);
                    self.act(e.from, "thanks", hw_sim::incidents::THANK_SECONDS as f32);
                    self.speak(head + Vec3::Y * 2.3, line.to_string(), Voice::Folk.colour());
                    self.said.push(Said {
                        at: head,
                        range: chat::SAY_RANGE,
                        who: speaker_name(speaker, "A townsperson"),
                        text: line.to_string(),
                        voice: Voice::Folk,
                    });
                    if on_me {
                        self.note = Some(("They're grateful.".into(), 3.0));
                    }
                }
                FxKind::Tip => {
                    // One of Hero Hall's heroes gives a newcomer a tip.
                    let who = all
                        .iter()
                        .find(|x| x.id == e.from)
                        .and_then(|x| x.mount)
                        .map_or(0, |m| m.1 as usize);
                    let name = hw_sim::npcheroes::NAMES[who % hw_sim::npcheroes::NAMES.len()];
                    let tip =
                        hw_sim::npcheroes::TIPS[e.amount as usize % hw_sim::npcheroes::TIPS.len()];
                    let head = all.iter().find(|x| x.id == e.from).map_or(at, pos);
                    self.speak(
                        head + Vec3::Y * 2.4,
                        format!("{name}: {tip}"),
                        egui::Color32::from_rgb(255, 214, 110),
                    );
                    self.said.push(Said {
                        at: head,
                        range: chat::SAY_RANGE,
                        who: name.to_string(),
                        text: tip.to_string(),
                        voice: Voice::Ally,
                    });
                    if on_me {
                        self.note = Some((format!("{name}: {tip}"), 7.0));
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
        let haste = if self.rallied {
            hw_sim::combat::RALLY_HASTE
        } else {
            1.0
        };
        self.cooldown[(p.code() - 1) as usize] = (power_spec(p).recharge / haste) as f32;
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

    /// Words someone says, over their head for a few seconds (`head` is
    /// where they stand; the words float a little above it).
    pub fn speech(&mut self, head: Vec3, text: &str) {
        self.speak(
            head + Vec3::Y * 2.3,
            text.to_string(),
            crate::chat::Voice::Me.colour(),
        );
    }

    /// Words someone says (`head`: where they stand), over their head in
    /// their voice's colour.
    pub fn say_at(&mut self, head: Vec3, text: &str, voice: crate::chat::Voice) {
        self.speak(head + Vec3::Y * 2.3, text.to_string(), voice.colour());
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
            // Set down by the drone (in hospital, or revived): the hanging
            // pose ends. Before the body check, since the body carried off
            // was despawned and a fresh one may not be drawn yet; a pose left
            // behind would hold every later clip off for good.
            if was == Status::Carried && c.status != Status::Carried {
                self.actions.remove(&s.id);
            }
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

    /// Puts a fighter's own weapon in its right hand: a delver's pick, a
    /// pirate's cutlass, a praetorian's lance (once per character).
    pub fn wield(&mut self, world: &mut World, e: EntityId, kind: EntityKind) {
        if self.guns.contains_key(&e) {
            return;
        }
        let Some(mesh) = crate::factionview::weapon(kind) else {
            return;
        };
        if let Ok(w) = world.place(mesh, Place::new(0.0, 0.0, 0.0)) {
            let local = crate::factionview::weapon_grip();
            let bone = crate::factionview::weapon_bone(kind);
            if world.attach_to_bone(w, e, bone, local).is_ok() {
                self.guns.insert(e, w);
            } else {
                world.despawn(w);
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
        self.recoils.retain(|_, r| {
            r.1 += dt;
            r.1 < RECOIL_SECONDS
        });
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
        let mut frozen = Vec::new();
        let mut shielded = Vec::new();
        self.bubbles.update(world, dt);
        // Bombs fly their arcs and are gone when they burst (the burst is
        // the server's Bomb event).
        self.lobs.retain_mut(|l| {
            l.age += dt;
            if l.age >= l.flight {
                world.despawn(l.ent);
                return false;
            }
            let p = l.at(l.age);
            let spin = l.age * 540.0;
            let _ = world.set_place(
                l.ent,
                Place::new(p.x, p.y, p.z)
                    .with_yaw_deg(spin * 0.5)
                    .with_pitch_deg(spin),
            );
            true
        });
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
            // Shielded: a force field round them, and sparks off it now and then.
            if c.effects & EFFECT_SHIELD != 0 {
                shielded.push(e.id);
                let p = pos(e) + Vec3::Y * 1.0;
                let place = Place::new(p.x, p.y, p.z).with_stretch(Vec3::new(
                    SHIELD_RADIUS,
                    SHIELD_RADIUS * 1.15,
                    SHIELD_RADIUS,
                ));
                match self.shields.get(&e.id) {
                    Some(&field) => {
                        let _ = world.set_place(field, place);
                    }
                    None => {
                        if let Ok(field) = world.place(crate::powerfx::shield_sphere(), place) {
                            let _ = world.set_casts_shadow(field, false);
                            crate::powerfx::dress_field(world, field);
                            self.shields.insert(e.id, field);
                        }
                    }
                }
                if sparkle {
                    shell(world, p, rgb(140, 220, 255), 6);
                }
            }
            // Frozen solid: a block of ice round them.
            if c.effects & EFFECT_FROZEN != 0 {
                frozen.push(e.id);
                let p = pos(e);
                let yaw = (e.id.0 % 360) as f32;
                let place = Place::new(p.x, p.y, p.z).with_yaw_deg(yaw);
                match self.ices.get(&e.id) {
                    Some(&ice) => {
                        let _ = world.set_place(ice, place);
                    }
                    None => {
                        if let Ok(ice) = world.place(crate::powerfx::ice_block(), place) {
                            crate::powerfx::dress_ice(world, ice);
                            self.ices.insert(e.id, ice);
                        }
                    }
                }
            }
            if c.effects & EFFECT_CHILLED != 0 && sparkle {
                crate::powerfx::chill(world, pos(e) + Vec3::Y * 1.0);
            }
            // Hypnotised: spirals turning over the head.
            if c.effects & EFFECT_CHARMED != 0 && sparkle {
                crate::powerfx::spiral(
                    world,
                    pos(e) + Vec3::Y * 2.15,
                    rgb(255, 120, 220),
                    rgb(170, 110, 255),
                    0.55,
                );
            }
            // A Rally aura: warm motes circling the feet of everyone in it.
            if c.effects & EFFECT_RALLIED != 0 && sparkle {
                rings(world, pos(e) + Vec3::Y * 0.15, rgb(255, 190, 110), 0.45);
            }
            // A speedster at full tilt leaves a crackling trail.
            if c.effects & EFFECT_SUPER_SPEED != 0 {
                let v = Vec3::new(e.vel[0], 0.0, e.vel[2]);
                if v.length() > hw_sim::RUN_SPEED as f32 * 1.6 {
                    sparks(
                        world,
                        pos(e) + Vec3::Y * 0.9,
                        -v.normalize(),
                        4,
                        rgb(255, 230, 120),
                    );
                }
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
        // Thawed (or shattered, or gone): the ice goes.
        let thawed: Vec<NetId> = self
            .ices
            .keys()
            .filter(|id| !frozen.contains(id))
            .copied()
            .collect();
        for id in thawed {
            if let Some(ice) = self.ices.remove(&id) {
                world.despawn(ice);
            }
        }
        // Shields down (or gone): the force field goes.
        let dropped: Vec<NetId> = self
            .shields
            .keys()
            .filter(|id| !shielded.contains(id))
            .copied()
            .collect();
        for id in dropped {
            if let Some(field) = self.shields.remove(&id) {
                world.despawn(field);
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
        &mut self,
        ctx: &egui::Context,
        view_proj: Mat4,
        me: Option<&EntityState>,
        all: &[EntityState],
        names: &dyn Fn(&EntityState) -> String,
        powers: Bar,
    ) -> Option<tray::Change> {
        let Bar { sheet, tray, .. } = powers;
        let mut changed = None;
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

        // Our hero: health, energy and the power tray, bottom centre. The
        // tray is ten slots (1 to 0), and a second row (Alt) above it once
        // that holds anything or a power is being dragged.
        if let Some(c) = me.and_then(|m| m.combat) {
            let dragging = egui::DragAndDrop::has_payload_of_type::<tray::Drag>(ctx);
            let rows = if tray.second_row() || dragging { 2 } else { 1 };
            let (w, slot_w) = tray_width(screen.width());
            let extra = (rows - 1) as f32 * ROW_STEP;
            let base = egui::pos2(screen.center().x - w * 0.5, screen.bottom() - 112.0 - extra);
            let panel = egui::Rect::from_min_size(base, egui::vec2(w, 98.0 + extra));
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
            let shown = if rows == 2 { tray::SLOTS } else { 10 };
            for k in 0..shown {
                // The Alt row sits above the number row.
                let row = if tray::alt(k) { 0.0 } else { (rows - 1) as f32 };
                let slot = egui::Rect::from_min_size(
                    base + egui::vec2(
                        12.0 + (k % 10) as f32 * (slot_w + 6.0),
                        48.0 + row * ROW_STEP,
                    ),
                    egui::vec2(slot_w, 40.0),
                );
                let power = tray.slots[k].filter(|p| kit.has(*p));
                // Drag and drop, and a click fires it.
                let resp = egui::Area::new(egui::Id::new(("tray slot", k)))
                    .fixed_pos(slot.min)
                    .order(egui::Order::Middle)
                    .show(ctx, |ui| {
                        ui.allocate_exact_size(slot.size(), egui::Sense::click_and_drag())
                            .1
                    })
                    .inner;
                if let Some(p) = power {
                    resp.dnd_set_drag_payload(tray::Drag::Slot(k));
                    if resp.clicked() {
                        self.clicked = Some(p);
                    }
                    if resp.dragged() {
                        if let Some(at) = ctx.pointer_interact_pos() {
                            let fg = ctx.layer_painter(egui::LayerId::new(
                                egui::Order::Tooltip,
                                egui::Id::new("tray drag"),
                            ));
                            let r = egui::Rect::from_center_size(at, egui::vec2(slot_w, 26.0));
                            fg.rect_filled(r, 5.0, egui::Color32::from_rgb(52, 60, 92));
                            fg.text(
                                r.center(),
                                egui::Align2::CENTER_CENTER,
                                power_name(p, sheet.theme(p)),
                                font(12.5),
                                egui::Color32::WHITE,
                            );
                        }
                    }
                }
                if let Some(d) = resp.dnd_release_payload::<tray::Drag>() {
                    if *d != tray::Drag::Slot(k) {
                        changed = Some(tray::Change::Drop(*d, k));
                    }
                }
                let hovered = resp.dnd_hover_payload::<tray::Drag>().is_some();
                let Some(power) = power else {
                    // An empty slot: a dim outline and its key.
                    painter.rect_stroke(
                        slot,
                        5.0,
                        egui::Stroke::new(
                            1.0_f32,
                            if hovered {
                                egui::Color32::from_rgb(255, 214, 90)
                            } else {
                                egui::Color32::from_gray(60)
                            },
                        ),
                        egui::StrokeKind::Inside,
                    );
                    painter.text(
                        slot.min + egui::vec2(8.0, 6.0),
                        egui::Align2::LEFT_TOP,
                        tray::label(k),
                        font(12.0),
                        egui::Color32::from_gray(110),
                    );
                    continue;
                };
                let spec = power_spec(power);
                let cd = self.cooldown[(power.code() - 1) as usize];
                let ready =
                    cd <= 0.0 && (energy + 0.5 >= spec.cost || carrying && power == Power::Lift);
                let fill = if hovered {
                    egui::Color32::from_rgb(80, 86, 130)
                } else if ready {
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
                        if ready || hovered {
                            egui::Color32::from_rgb(255, 214, 90)
                        } else {
                            egui::Color32::from_gray(90)
                        },
                    ),
                    egui::StrokeKind::Inside,
                );
                let key = tray::label(k);
                painter.text(
                    slot.min + egui::vec2(8.0, 6.0),
                    egui::Align2::LEFT_TOP,
                    &key,
                    font(12.0),
                    egui::Color32::from_rgb(255, 214, 90),
                );
                let name_x = if tray::alt(k) { 40.0 } else { 20.0 };
                let name = if power == Power::Lift && carrying {
                    "Throw!"
                } else {
                    power_name(power, sheet.theme(power))
                };
                painter.text(
                    slot.min + egui::vec2(name_x, 6.0),
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
                    slot.min + egui::vec2(name_x, 24.0),
                    egui::Align2::LEFT_TOP,
                    cost,
                    font(11.0),
                    egui::Color32::from_gray(180),
                );
            }
            // A slot dragged off the tray and let go: it leaves the tray.
            if changed.is_none() && ctx.input(|i| i.pointer.any_released()) {
                if let Some(d) = egui::DragAndDrop::payload::<tray::Drag>(ctx) {
                    let over = ctx
                        .pointer_interact_pos()
                        .is_some_and(|p| panel.contains(p));
                    if let (tray::Drag::Slot(k), false) = (*d, over) {
                        changed = Some(tray::Change::Clear(k));
                    }
                }
            }
            painter.text(
                egui::pos2(screen.center().x, base.y - 8.0),
                egui::Align2::CENTER_BOTTOM,
                &{
                    let mut hint = String::from("Tab: target   Left mouse / 1   Q / 2   B: powers");
                    let moves: Vec<&str> = [Power::SuperJump, Power::Flight, Power::SuperSpeed]
                        .into_iter()
                        .filter(|p| kit.has(*p) && is_movement(*p))
                        .map(|p| match p {
                            Power::Flight => "V: fly",
                            Power::SuperSpeed => "Shift: super speed",
                            _ => "Space: super jump",
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
        changed
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
/// The villain under a click: the nearest one whose body the ray from
/// `from` along `dir` passes through, within [`CLICK_RANGE`], that `seen`
/// allows (none behind a wall). Bodies are upright cylinders a little
/// wider than the drawn figure, so a click need not be pixel-exact; the
/// big robots' are bigger.
pub fn villain_under(
    all: &[EntityState],
    from: Vec3,
    dir: Vec3,
    seen: impl Fn(&EntityState) -> bool,
) -> Option<NetId> {
    let dir = dir.normalize_or_zero();
    all.iter()
        .filter(|e| targetable(e))
        .filter_map(|e| {
            let (radius, height, base) = match e.kind {
                EntityKind::Colossus => (4.0, 14.0, 0.0),
                EntityKind::WeakPoint => (1.4, 2.8, -1.4),
                EntityKind::Sentry => (1.1, 3.2, 0.0),
                _ => (0.6, 2.1, 0.0),
            };
            let p = pos(e);
            let t = ray_cylinder(from, dir, p + Vec3::Y * base, radius, height)?;
            (t <= CLICK_RANGE).then_some((t, e))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        // The first one under the mouse, and only if the hero sees it: a
        // click never reaches past a villain to the one behind.
        .filter(|(_, e)| seen(e))
        .map(|(_, e)| e.id)
}

/// Where a ray first meets an upright cylinder standing on `foot`: the
/// distance along the (unit) ray, if it does.
fn ray_cylinder(from: Vec3, dir: Vec3, foot: Vec3, radius: f32, height: f32) -> Option<f32> {
    let o = glam::Vec2::new(from.x - foot.x, from.z - foot.z);
    let d = glam::Vec2::new(dir.x, dir.z);
    let a = d.length_squared();
    // Where the ray is within the circle, seen from above.
    let (t0, t1) = if a < 1e-9 {
        if o.length() > radius {
            return None;
        }
        (0.0, f32::MAX)
    } else {
        let b = o.dot(d);
        let c = o.length_squared() - radius * radius;
        let disc = b * b - a * c;
        if disc < 0.0 {
            return None;
        }
        let r = disc.sqrt();
        ((-b - r) / a, (-b + r) / a)
    };
    // And where it is between the feet and the top.
    let (y0, y1) = (foot.y - from.y, foot.y + height - from.y);
    let (h0, h1) = if dir.y.abs() < 1e-9 {
        if (y0..=y1).contains(&0.0) {
            (f32::MIN, f32::MAX)
        } else {
            return None;
        }
    } else {
        let (u, v) = (y0 / dir.y, y1 / dir.y);
        (u.min(v), u.max(v))
    };
    let enter = t0.max(h0).max(0.0);
    let leave = t1.min(h1);
    (enter <= leave).then_some(enter)
}

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
/// within range and in view (and in sight: `seen`): what an aimed power
/// hits when nothing is targeted.
pub fn pick_target(
    all: &[EntityState],
    eye: Vec3,
    look: Vec3,
    seen: impl Fn(&EntityState) -> bool,
) -> Option<NetId> {
    let look = look.normalize_or_zero();
    all.iter()
        .filter(|e| targetable(e) && seen(e))
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
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .map(|x| x.1)
}

/// Tab, City of Heroes style: the villains within range of `from` (our
/// hero), those in view of the camera (`eye` looking along `look`) first,
/// each group nearest first. Each call picks the first one not yet in
/// `cycled` (and not `current`) and adds it there; once all have had their
/// turn the cycle starts over, wrapping round to the nearest again.
pub fn cycle_target(
    all: &[EntityState],
    from: Vec3,
    eye: Vec3,
    look: Vec3,
    current: Option<NetId>,
    cycled: &mut Vec<NetId>,
    seen: impl Fn(&EntityState) -> bool,
) -> Option<NetId> {
    let look = look.normalize_or_zero();
    let mut list: Vec<(bool, f32, NetId)> = all
        .iter()
        // Never one behind a wall: nothing could reach it.
        .filter(|e| targetable(e) && seen(e))
        .filter_map(|e| {
            let d = pos(e).distance(from);
            if d > TARGET_RANGE {
                return None;
            }
            let to = pos(e) + Vec3::Y * 1.1 - eye;
            let seen = to.length() < 1e-3
                || look
                    .dot(to.normalize())
                    .clamp(-1.0, 1.0)
                    .acos()
                    .to_degrees()
                    <= TARGET_CONE;
            Some((!seen, d, e.id))
        })
        .collect();
    list.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
    // Forget the ones that fell or left; the current target counts as had.
    cycled.retain(|id| list.iter().any(|x| x.2 == *id));
    if let Some(c) = current.filter(|c| !cycled.contains(c)) {
        cycled.push(c);
    }
    let fresh = |cycled: &[NetId]| {
        list.iter()
            .map(|x| x.2)
            .find(|id| !cycled.contains(id) && Some(*id) != current)
    };
    let next = fresh(cycled).or_else(|| {
        // Everyone had a turn: start over with the nearest (not the one we
        // are on, unless it is the only one).
        cycled.clear();
        fresh(cycled).or_else(|| list.first().map(|x| x.2))
    })?;
    cycled.push(next);
    Some(next)
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
        EntityKind::Delver => (rgb(90, 63, 40), rgb(230, 201, 195)),
        EntityKind::Buccaneer | EntityKind::Gunner => (rgb(91, 36, 32), rgb(224, 186, 150)),
        EntityKind::Captain => (rgb(28, 40, 72), rgb(224, 186, 150)),
        EntityKind::Legionary | EntityKind::Praetorian => (rgb(37, 38, 43), rgb(108, 111, 118)),
        EntityKind::Bulwark => (rgb(70, 60, 52), rgb(110, 160, 80)),
        EntityKind::Grenadier => (rgb(84, 92, 58), rgb(224, 186, 150)),
        EntityKind::Orderly => (rgb(170, 220, 200), rgb(224, 186, 150)),
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
        freeze: clip("freeze"),
        shatter: clip("shatter"),
        teleport: clip("teleport"),
        bubble: clip("bubble"),
        hypno: clip("hypno"),
        robot: clip("robot"),
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
    fn aimed_powers_pick_nearest_the_crosshair() {
        let all = vec![
            villain(1, [10.0, 0.0, 3.0]),
            villain(2, [10.0, 0.0, 0.0]),
            villain(3, [-10.0, 0.0, 0.0]),
        ];
        let eye = Vec3::new(0.0, 1.1, 0.0);
        assert_eq!(
            pick_target(&all, eye, Vec3::X, |_| true),
            Some(NetId(2)),
            "dead ahead"
        );
        // Behind the camera is never picked.
        assert_eq!(pick_target(&all[2..], eye, Vec3::X, |_| true), None);
    }

    #[test]
    fn tab_cycles_nearest_first_through_everyone_in_range() {
        let all = vec![
            villain(1, [12.0, 0.0, 3.0]),
            villain(2, [6.0, 0.0, 0.0]),
            villain(3, [-4.0, 0.0, 0.0]), // nearest, but behind the camera
            villain(4, [20.0, 0.0, -2.0]),
            villain(5, [90.0, 0.0, 0.0]), // out of range
        ];
        let from = Vec3::ZERO;
        let eye = Vec3::new(-3.0, 2.0, 0.0);
        let mut cycled = Vec::new();
        let mut target = None;
        let mut seen = Vec::new();
        for _ in 0..8 {
            target = cycle_target(&all, from, eye, Vec3::X, target, &mut cycled, |_| true);
            seen.push(target.unwrap().0);
        }
        // In view nearest first, then the one behind, then round again.
        assert_eq!(seen, [2, 1, 4, 3, 2, 1, 4, 3]);
    }

    #[test]
    fn tab_moves_on_even_when_the_order_shifts() {
        // The camera turns to the new target after every Tab, which
        // reorders the list; Tab must still go on to someone new.
        let all = vec![
            villain(1, [10.0, 0.0, 0.0]),
            villain(2, [0.0, 0.0, 11.0]),
            villain(3, [-12.0, 0.0, 0.0]),
        ];
        let mut cycled = Vec::new();
        let mut target = None;
        let mut seen = Vec::new();
        let mut look = Vec3::X;
        for _ in 0..3 {
            target = cycle_target(
                &all,
                Vec3::ZERO,
                Vec3::ZERO,
                look,
                target,
                &mut cycled,
                |_| true,
            );
            let t = all.iter().find(|e| Some(e.id) == target).unwrap();
            look = pos(t).normalize();
            seen.push(target.unwrap().0);
        }
        seen.sort();
        assert_eq!(seen, [1, 2, 3], "everyone once before any repeats");
    }

    #[test]
    fn a_struck_villain_rocks_back_and_returns() {
        let mut fight = FightView::new();
        fight.recoils.insert(NetId(1), (Vec3::X, 0.0));
        assert_eq!(recoil_shape(0.0), 0.0);
        assert!((recoil_shape(0.05) - 1.0).abs() < 1e-4, "full push at once");
        assert!(recoil_shape(0.2) < 0.5, "easing back");
        assert_eq!(recoil_shape(RECOIL_SECONDS), 0.0, "home again");
        fight.recoils.insert(NetId(1), (Vec3::X, 0.05));
        let (push, lean) = fight.recoil(NetId(1)).expect("recoiling");
        assert!(
            push.x > 0.3 && lean < -10.0,
            "pushed {push}, leaning {lean}"
        );
        assert_eq!(fight.recoil(NetId(2)), None);
    }

    #[test]
    fn a_click_picks_the_villain_under_the_mouse_far_beyond_tab() {
        let eye = Vec3::new(0.0, 3.0, -4.0);
        let at = |x: f32, z: f32| (Vec3::new(x, 1.2, z) - eye).normalize();
        let all = [
            villain(1, [0.0, 0.0, 6.0]),
            villain(2, [0.0, 0.0, 12.0]),
            villain(3, [3.0, 0.0, 120.0]),
        ];
        // The nearer of two in a line.
        assert_eq!(
            villain_under(&all, eye, at(0.0, 6.0), |_| true),
            Some(NetId(1))
        );
        // One far down the street, twice Tab's reach.
        const { assert!(120.0 > TARGET_RANGE) };
        assert_eq!(
            villain_under(&all, eye, at(3.0, 120.0), |_| true),
            Some(NetId(3))
        );
        // Between them: nobody.
        assert_eq!(villain_under(&all, eye, at(1.5, 9.0), |_| true), None);
        // Not one hidden from the hero.
        assert_eq!(
            villain_under(&all, eye, at(0.0, 6.0), |e| e.id != NetId(1)),
            None,
            "the click went to the hidden one, not past it"
        );
        // Too far even for a click.
        let far = [villain(4, [0.0, 0.0, 200.0])];
        assert_eq!(villain_under(&far, eye, at(0.0, 200.0), |_| true), None);
    }

    #[test]
    fn villains_behind_a_wall_are_never_picked() {
        let all = vec![villain(1, [4.0, 0.0, 0.0]), villain(2, [9.0, 0.0, 0.0])];
        let behind = |e: &EntityState| e.id != NetId(1);
        assert_eq!(
            pick_target(&all, Vec3::ZERO, Vec3::X, behind),
            Some(NetId(2))
        );
        let mut cycled = Vec::new();
        for _ in 0..3 {
            let t = cycle_target(
                &all,
                Vec3::ZERO,
                Vec3::ZERO,
                Vec3::X,
                None,
                &mut cycled,
                behind,
            );
            assert_eq!(t, Some(NetId(2)));
        }
        let none = |_: &EntityState| false;
        assert_eq!(pick_target(&all, Vec3::ZERO, Vec3::X, none), None);
    }

    #[test]
    fn tab_after_an_auto_target_moves_on() {
        // The attacker became the target on its own: Tab goes to another.
        let all = vec![villain(1, [5.0, 0.0, 0.0]), villain(2, [8.0, 0.0, 0.0])];
        let mut cycled = Vec::new();
        let next = cycle_target(
            &all,
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::X,
            Some(NetId(1)),
            &mut cycled,
            |_| true,
        );
        assert_eq!(next, Some(NetId(2)));
        // A lone villain stays the target.
        let mut cycled = Vec::new();
        let one = &all[..1];
        let t = cycle_target(
            one,
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::X,
            Some(NetId(1)),
            &mut cycled,
            |_| true,
        );
        assert_eq!(t, Some(NetId(1)));
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
        assert_eq!(pick_target(&[v], Vec3::ZERO, Vec3::X, |_| true), None);
        let mut cycled = Vec::new();
        assert_eq!(
            cycle_target(
                &[v],
                Vec3::ZERO,
                Vec3::ZERO,
                Vec3::X,
                None,
                &mut cycled,
                |_| true
            ),
            None
        );
    }

    #[test]
    fn a_hero_set_down_from_the_drone_animates_again() {
        // Down, carried off to hospital, standing again: the drone's
        // hanging pose must not outlive the ride (it held every clip off).
        let mut world = World::new();
        let mut fight = FightView::new();
        let id = NetId(7);
        let mut hero = EntityState {
            id,
            kind: EntityKind::Hero,
            combat: Some(CombatState {
                health: 255,
                level: 1,
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut ents = BTreeMap::new();
        let body = world
            .place(crate::citymesh::hero_mesh(true), Place::new(0.0, 0.0, 0.0))
            .expect("body");
        ents.insert(id, (body, false));
        fight.statuses(&mut world, &[hero], &mut ents);
        for status in [Status::Down, Status::Carried] {
            hero.combat.as_mut().unwrap().status = status;
            fight.statuses(&mut world, &[hero], &mut ents);
        }
        assert_eq!(fight.action(id), Some("hover"), "hangs from the drone");
        assert!(ents.is_empty(), "the body on the ground is gone");
        hero.combat.as_mut().unwrap().status = Status::Standing;
        fight.statuses(&mut world, &[hero], &mut ents);
        assert_eq!(fight.action(id), None, "walks, idles and fights again");
    }

    #[test]
    #[ignore = "on demand (looks): scripts/check.sh --on-demand"]
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
