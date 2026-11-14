//! A Herobot's brain: a stand-in for a player who plays by itself.
//!
//! It is an ordinary client. It joins over UDP with [`hw_net::Connection`],
//! sends a hero file, and from then on sees only what a player sees: the
//! entities near its hero and the reliable news (incidents, its own hero
//! file). It answers with an [`InputState`] every tick and with the same
//! messages a player sends (buy a power, set an augment, chat), so the server
//! treats it exactly like a person: karma, levels, the hospital drone, the
//! player limit, the team.
//!
//! What it does: it follows the hero who woke it; it heads for the incidents
//! the wrist radio announces and fights the villains it finds there, with the
//! powers it owns; a downed hero it passes is helped up; and when it is
//! downed itself it calls the medical drone after 20 s. It spends its
//! karma on powers as soon as it has it, and sets the augments it finds.
//!
//! The brain is `drive`n once per tick by whoever owns the connection: the
//! thread of [`spawn`] in the game, or a test, tick for tick with the host.

use crate::powers::{self, Delivery, Element, Kit};
use crate::{nav, progress, TICK_HZ};
use glam::{DVec2, DVec3};
use hw_net::{
    ConnState, Connection, EntityKind, EntityState, InputState, PlayerId, PowerId, Sheet, Status,
    ToClient, ToServer, EFFECT_CAN_FLY,
};
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long a downed Herobot waits for a teammate before it calls the
/// medical drone itself (s).
pub const CALL_DRONE_AFTER: f64 = 20.0;
/// How far from the hero it follows (m): it stays about this near.
const FOLLOW_AT: f64 = 6.0;
/// Farther than this from the hero it follows, it closes up again (m).
const FOLLOW_START: f64 = 10.0;
/// It never strays farther than this from them, fighting or not (m).
const LEASH: f64 = 35.0;
/// Villains this near that fight, or stand near an open incident, are
/// attacked (m).
const SEE_VILLAIN: f64 = 70.0;
/// A downed hero this near is helped (m).
const SEE_DOWNED: f64 = 120.0;
/// Incidents this near are searched out (m).
const SEE_INCIDENT: f64 = 900.0;
/// A villain this near an incident counts as part of it (m).
const INCIDENT_RADIUS: f64 = 45.0;
/// Closer than this to the home spot, it is home (m).
const HOME_REACH: f64 = 3.0;
/// Ticks between two power presses.
const PRESS_EVERY: u64 = 6;
/// Reach of a punch (m).
const PUNCH_REACH: f64 = 3.0;
/// Reach of a hold-to-revive (m).
const REVIVE_REACH: f64 = 2.2;

/// What the game tells a running Herobot from outside.
#[derive(Default)]
pub struct Ctl {
    /// Go back to the plaza, then leave.
    pub home: AtomicBool,
    /// Leave now (the game is closing).
    pub stop: AtomicBool,
}

/// Everything a Herobot needs to start.
pub struct Params {
    pub addr: SocketAddr,
    pub name: String,
    pub slot: u8,
    /// The karma it plays with: all the waking hero ever earned.
    pub lifetime: u32,
    pub seed: u64,
    /// The plaza spot it goes back to.
    pub home: DVec3,
    /// The solid town, for finding a way round walls (the same data every
    /// client makes from the seed).
    pub solids: Arc<crate::collision::Obstacles>,
    pub ctl: Arc<Ctl>,
}

/// A small deterministic random number source (xorshift).
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn roll(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: u64) -> u64 {
        self.roll() % n.max(1)
    }
}

/// How much a Herobot wants a power when it shops: attacks first.
fn want(d: &powers::PowerDef) -> u64 {
    match d.delivery() {
        Delivery::Melee
        | Delivery::Ranged
        | Delivery::Line
        | Delivery::Cone
        | Delivery::Burst
        | Delivery::Sky => 5,
        Delivery::Allies | Delivery::Summon => 3,
        Delivery::Passive => 3,
        Delivery::Grab | Delivery::Blink | Delivery::Rescue => 1,
    }
}

/// Spends the karma of `sheet` on powers it can unlock, chosen at random
/// (attacks first). Returns what it bought.
pub fn spend(sheet: &mut Sheet, rng: &mut Rng) -> Vec<PowerId> {
    let mut bought = Vec::new();
    loop {
        let mut options: Vec<(PowerId, u64)> = powers::POWERS
            .iter()
            .filter(|d| !powers::is_free(d.id) && progress::can_buy(sheet, d.id).is_ok())
            .map(|d| {
                let mut w = want(d);
                // A hero without an attack buys one before anything else.
                let has_attack = sheet.powers.iter().any(|o| powers::def_of(o.power).aimed());
                if !has_attack && !d.aimed() {
                    w = 0;
                }
                if d.base == hw_net::Power::Flight {
                    w += 3;
                }
                (d.id, w)
            })
            .filter(|(_, w)| *w > 0)
            .collect();
        if options.is_empty() {
            break;
        }
        options.sort_by_key(|(id, _)| *id);
        let total: u64 = options.iter().map(|(_, w)| *w).sum();
        let mut pick = rng.below(total);
        let mut chosen = options[0].0;
        for (id, w) in &options {
            if pick < *w {
                chosen = *id;
                break;
            }
            pick -= *w;
        }
        if progress::buy(sheet, chosen).is_err() {
            break;
        }
        bought.push(chosen);
    }
    bought
}

/// A Herobot's first hero file: all of `lifetime` as karma (the waking
/// hero's, as if nothing had been spent), a random origin and element, and
/// all of it spent on powers it can unlock.
pub fn make_sheet(lifetime: u32, seed: u64) -> Sheet {
    let mut rng = Rng::new(seed);
    let origin = rng.below(progress::ORIGINS.len() as u64) as u8;
    let element = Element::from_code(1 + rng.below(6) as u8);
    let mut sheet = progress::new_hero(origin, element);
    sheet.lifetime = lifetime;
    // At least what a new hero starts with, so level 1 has a first power.
    sheet.karma = lifetime.max(progress::START_KARMA);
    spend(&mut sheet, &mut rng);
    sheet
}

/// Whether `Drive` wants to be called again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drive {
    Running,
    /// Left the world (home, or the connection ended).
    Done,
}

/// The Herobot's thinking, one tick at a time.
pub struct Brain {
    home: DVec3,
    solids: Arc<crate::collision::Obstacles>,
    ctl: Arc<Ctl>,
    lifetime: u32,
    seed: u64,
    rng: Rng,
    ticks: u64,
    sheet: Sheet,
    kit: Kit,
    sent_sheet: bool,
    /// The newest hero file the server sent (we wait for it after a buy).
    buy_wait: u64,
    incidents: BTreeMap<u32, DVec3>,
    path: Vec<DVec3>,
    path_goal: DVec3,
    path_at: u64,
    last_pos: DVec3,
    last_check: u64,
    unstick_until: u64,
    unstick_dir: DVec2,
    down_since: Option<u64>,
    last_press: u64,
    rot: usize,
    idle_since: u64,
    /// Ticks since we started going home; the farewell once there.
    leaving: Option<u64>,
    /// Counters for tests and the log.
    pub fights: u64,
    pub revives: u64,
    pub purchases: u64,
}

impl Brain {
    pub fn new(p: &Params) -> Self {
        let sheet = make_sheet(p.lifetime, p.seed);
        Brain {
            home: p.home,
            solids: p.solids.clone(),
            ctl: p.ctl.clone(),
            lifetime: p.lifetime,
            seed: p.seed,
            rng: Rng::new(p.seed ^ 0xA5A5),
            ticks: 0,
            kit: Kit::of(&sheet),
            sheet,
            sent_sheet: false,
            buy_wait: 0,
            incidents: BTreeMap::new(),
            path: Vec::new(),
            path_goal: DVec3::ZERO,
            path_at: 0,
            last_pos: DVec3::ZERO,
            last_check: 0,
            unstick_until: 0,
            unstick_dir: DVec2::ZERO,
            down_since: None,
            last_press: 0,
            rot: 0,
            idle_since: 0,
            leaving: None,
            fights: 0,
            revives: 0,
            purchases: 0,
        }
    }

    /// The hero file as the server last told us.
    pub fn sheet(&self) -> &Sheet {
        &self.sheet
    }

    /// The hero file this Herobot first sent.
    pub fn first_sheet(&self) -> Sheet {
        make_sheet(self.lifetime, self.seed)
    }

    /// One tick: reads the news and the world, sends the input.
    pub fn drive(&mut self, conn: &mut Connection) -> Drive {
        self.ticks += 1;
        let player = match conn.state {
            ConnState::Connecting => {
                conn.update(InputState::default());
                return Drive::Running;
            }
            ConnState::Rejected(_) | ConnState::Ended(_) | ConnState::TakingOver => {
                return Drive::Done;
            }
            ConnState::Connected { player, .. } => player,
        };
        if self.ctl.stop.load(Ordering::Relaxed) {
            conn.close();
            return Drive::Done;
        }
        if !self.sent_sheet {
            conn.send(ToServer::Sheet(self.first_sheet()));
            conn.chat("Herobot reporting for duty!");
            self.sent_sheet = true;
        }
        self.read_news(conn);
        self.shop(conn);
        let world = conn.mirror.states();
        let me = conn.hero().and_then(|h| conn.mirror.get(h));
        let input = match me {
            Some(me) => self.decide(player, &me, &world),
            None => InputState::default(),
        };
        // Once home, say goodbye and leave: the server puts the robot back
        // on the plaza when the connection closes.
        if let Some(since) = self.leaving {
            if self.ticks > since + TICK_HZ as u64 / 2 {
                conn.chat("Back on the plaza!");
                conn.update(InputState::default());
                conn.close();
                return Drive::Done;
            }
        }
        conn.update(input);
        Drive::Running
    }

    fn read_news(&mut self, conn: &mut Connection) {
        for m in conn.take_news() {
            match m {
                ToClient::Sheet(s) => {
                    self.kit = Kit::of(&s);
                    self.sheet = s;
                    self.buy_wait = 0;
                }
                ToClient::Incident { id, at, state, .. } => {
                    if state == 0 {
                        self.incidents
                            .insert(id, DVec3::new(at[0] as f64, at[1] as f64, at[2] as f64));
                    } else {
                        self.incidents.remove(&id);
                    }
                }
                _ => {}
            }
        }
    }

    /// Buys powers with karma earned since, and sets augments found (what
    /// a player does in the powers window).
    fn shop(&mut self, conn: &mut Connection) {
        if self.ticks < self.buy_wait || !self.sent_sheet {
            return;
        }
        let mut sheet = self.sheet.clone();
        let bought = spend(&mut sheet, &mut self.rng);
        for p in &bought {
            conn.send(ToServer::Buy { power: *p });
            self.purchases += 1;
        }
        // An augment in the bag goes into the first socket it fits; one
        // that fits nowhere is traded in for karma.
        if let Some(a) = sheet.bag.first().copied() {
            let mut placed = false;
            let owned: Vec<PowerId> = sheet.powers.iter().map(|o| o.power).collect();
            'find: for power in owned {
                for slot in 0..hw_net::SOCKETS as u8 {
                    let taken = sheet
                        .augments
                        .iter()
                        .any(|s| s.power == power && s.slot == slot);
                    if !taken && crate::augments::can_set_augment(&sheet, a, power, slot).is_ok() {
                        conn.send(ToServer::SetAugment {
                            bag: 0,
                            power,
                            slot,
                        });
                        placed = true;
                        break 'find;
                    }
                }
            }
            if !placed {
                conn.send(ToServer::TradeAugment { bag: 0 });
            }
        }
        // Wait for the server's word before shopping again.
        if !bought.is_empty() || !sheet.bag.is_empty() {
            self.buy_wait = self.ticks + TICK_HZ as u64 * 2;
        }
    }

    fn decide(
        &mut self,
        me_player: PlayerId,
        me: &EntityState,
        world: &[EntityState],
    ) -> InputState {
        let mut input = InputState::default();
        let Some(c) = me.combat else {
            return input;
        };
        let pos = DVec3::from_array(me.pos);
        match c.status {
            Status::Down => {
                let since = *self.down_since.get_or_insert(self.ticks);
                // The drone comes on its own after 20 s; calling it is
                // what a player would do.
                if self.ticks - since >= (CALL_DRONE_AFTER * TICK_HZ as f64) as u64 {
                    input.hospital = true;
                }
                self.path.clear();
                return input;
            }
            Status::Carried | Status::Knocked => {
                self.path.clear();
                return input;
            }
            Status::Standing => {
                self.down_since = None;
            }
        }
        let can_fly = c.effects & EFFECT_CAN_FLY != 0;
        let others = |e: &&EntityState| e.id != me.id;
        let heroes: Vec<&EntityState> = world
            .iter()
            .filter(others)
            .filter(|e| e.kind == EntityKind::Hero)
            .collect();
        let at = |e: &EntityState| DVec3::from_array(e.pos);

        // Going home overrides everything but a downed hero's call.
        if self.ctl.home.load(Ordering::Relaxed) && self.leaving.is_none() {
            let d = flat_distance(pos, self.home);
            if d <= HOME_REACH && pos.y - self.home.y < 3.0 {
                self.leaving = Some(self.ticks);
                return input;
            }
            self.travel(&mut input, pos, self.home, HOME_REACH, can_fly);
            return input;
        }
        if self.leaving.is_some() {
            return input;
        }

        // Whom it stays by: the nearest person, else the nearest hero (a
        // Herobot keeps with people before it keeps with another Herobot).
        let is_bot = |e: &&EntityState| e.mount.is_some_and(|m| m.1 >= hw_net::HEROBOT_MOUNT);
        let leader = nearest(heroes.iter().copied().filter(|e| !is_bot(e)).collect(), pos)
            .or_else(|| nearest(heroes.clone(), pos));
        let anchor = leader.map(at);
        // On the leash: back to them first, whatever is going on.
        if let (Some(a), Some(l)) = (anchor, leader) {
            let down = l.combat.is_some_and(|c| c.status == Status::Down);
            if !down && (flat_distance(a, pos) > LEASH || (a.y - pos.y).abs() > 30.0) {
                self.travel(&mut input, pos, a, FOLLOW_AT, can_fly);
                return input;
            }
        }
        let near_anchor = |p: DVec3, r: f64| anchor.is_none_or(|a| flat_distance(a, p) < r);

        // A downed hero near: help them up.
        let downed = heroes
            .iter()
            .filter(|e| e.combat.is_some_and(|c| c.status == Status::Down))
            .filter(|e| at(e).distance(pos) < SEE_DOWNED)
            .filter(|e| near_anchor(at(e), LEASH + 10.0))
            .min_by(|a, b| {
                at(a)
                    .distance(pos)
                    .total_cmp(&at(b).distance(pos))
                    .then(a.id.cmp(&b.id))
            });
        if let Some(d) = downed {
            let goal = at(d);
            if goal.distance(pos) <= REVIVE_REACH {
                input.revive = true;
                self.revives += 1;
                aim_at(&mut input, pos, goal);
            } else {
                self.travel(&mut input, pos, goal, 1.2, can_fly);
            }
            return input;
        }

        // Villains that fight, or that stand at an incident.
        let incidents: Vec<DVec3> = self.incidents.values().copied().collect();
        let foe = world
            .iter()
            .filter(|e| e.kind.is_enemy())
            .filter(|e| {
                e.combat
                    .is_some_and(|c| matches!(c.status, Status::Standing | Status::Knocked))
            })
            .filter(|e| at(e).distance(pos) < SEE_VILLAIN)
            .filter(|e| near_anchor(at(e), LEASH))
            .filter(|e| {
                e.combat.is_some_and(|c| c.target.is_some())
                    || incidents
                        .iter()
                        .any(|i| flat_distance(*i, at(e)) < INCIDENT_RADIUS)
            })
            .min_by(|a, b| {
                at(a)
                    .distance(pos)
                    .total_cmp(&at(b).distance(pos))
                    .then(a.id.cmp(&b.id))
            });
        let hurt_friend = heroes.iter().any(|e| {
            e.combat
                .is_some_and(|c| c.status == Status::Standing && c.health < 130)
                && at(e).distance(pos) < 20.0
        });
        if let Some(f) = foe {
            self.idle_since = self.ticks;
            self.fight(
                &mut input,
                me,
                &c,
                pos,
                f,
                hurt_friend,
                world,
                me_player,
                can_fly,
            );
            return input;
        }
        // Nobody to fight: switch defences off to get energy back.
        self.toggles(&mut input, &c, false);

        // A friend in need, and a power for it.
        if hurt_friend || c.health < 110 {
            if let Some(p) = self.pick(|d| d.delivery() == Delivery::Allies) {
                self.press(&mut input, p);
            }
        }

        // Search: the nearest incident, else stay by the nearest hero.
        let incident = incidents
            .iter()
            .filter(|i| flat_distance(**i, pos) < SEE_INCIDENT)
            .filter(|i| near_anchor(**i, LEASH + 10.0))
            .min_by(|a, b| {
                flat_distance(**a, pos)
                    .total_cmp(&flat_distance(**b, pos))
                    .then(a.x.total_cmp(&b.x))
            })
            .copied();
        if let Some(i) = incident {
            if flat_distance(i, pos) > 15.0 {
                self.travel(&mut input, pos, i, 8.0, can_fly);
                return input;
            }
        }
        if let Some(goal) = anchor {
            if flat_distance(goal, pos) > FOLLOW_START || goal.y - pos.y > 15.0 {
                self.travel(&mut input, pos, goal, FOLLOW_AT, can_fly);
            }
        }
        input
    }

    #[allow(clippy::too_many_arguments)]
    fn fight(
        &mut self,
        input: &mut InputState,
        me: &EntityState,
        c: &hw_net::CombatState,
        pos: DVec3,
        foe: &EntityState,
        hurt_friend: bool,
        world: &[EntityState],
        me_player: PlayerId,
        can_fly: bool,
    ) {
        self.fights += 1;
        let goal = DVec3::from_array(foe.pos);
        let dist = goal.distance(pos);
        input.target = Some(foe.id);
        aim_at(input, pos, goal);
        let shots = self.shots();
        // How near it wants to be: far for shooters, close for a puncher.
        let stand = shots
            .iter()
            .map(|(_, r)| *r)
            .fold(0.0f64, f64::max)
            .clamp(0.0, 22.0);
        let stand = if shots.is_empty() {
            PUNCH_REACH * 0.7
        } else {
            stand * 0.7
        };
        let sight =
            crate::combat::in_sight(&self.solids, pos + DVec3::Y * 1.1, goal + DVec3::Y * 1.1);
        if !sight {
            // A wall (or a roof) between us: go round it, close up.
            self.travel(input, pos, goal, 2.0, can_fly);
            aim_at(input, pos, goal);
            input.target = Some(foe.id);
            return;
        }
        if dist > stand + 2.0 {
            self.travel(input, pos, goal, stand, can_fly);
            // `travel` clears aim; look at the villain again.
            aim_at(input, pos, goal);
        } else if dist < stand * 0.4 && stand > 6.0 {
            // Too close for a shooter: back off a step.
            let away = flat_dir(goal, pos);
            input.move_x = away.x as f32;
            input.move_z = away.y as f32;
        }
        let energy = c.energy;
        self.toggles(input, c, energy > 150);
        if self.ticks < self.last_press + PRESS_EVERY || energy < 25 {
            return;
        }
        // A sidekick, if the power is owned and none stands by.
        if self
            .kit
            .owned()
            .any(|p| powers::def_of(p).delivery() == Delivery::Summon)
            && !world
                .iter()
                .any(|e| e.kind == EntityKind::Sidekick && e.owner == Some(me_player))
        {
            if let Some(p) = self.pick(|d| d.delivery() == Delivery::Summon) {
                self.press(input, p);
                return;
            }
        }
        if (hurt_friend || c.health < 120) && self.ticks.is_multiple_of(4) {
            if let Some(p) = self.pick(|d| d.delivery() == Delivery::Allies) {
                self.press(input, p);
                return;
            }
        }
        let usable: Vec<PowerId> = shots
            .iter()
            .filter(|(_, r)| dist <= *r)
            .map(|(p, _)| *p)
            .collect();
        if !usable.is_empty() {
            self.rot = self.rot.wrapping_add(1);
            let p = usable[self.rot % usable.len()];
            self.press(input, p);
        } else if self.kit.blast.is_some() && dist <= 40.0 {
            input.blast = true;
            self.last_press = self.ticks;
        } else if dist <= PUNCH_REACH + 1.0 {
            input.smash = true;
            self.last_press = self.ticks;
        }
        let _ = me;
    }

    /// The attack powers owned, with how far each reaches.
    fn shots(&self) -> Vec<(PowerId, f64)> {
        let mut out: Vec<(PowerId, f64)> = self
            .kit
            .owned()
            .filter_map(|p| {
                let d = powers::get(p)?;
                if d.passive() || d.toggle || powers::is_free(p) {
                    return None;
                }
                let reach = match d.delivery() {
                    Delivery::Ranged | Delivery::Line | Delivery::Cone | Delivery::Sky => {
                        d.fields.range.max(8.0)
                    }
                    Delivery::Burst => 9.0,
                    Delivery::Melee => PUNCH_REACH,
                    _ => return None,
                };
                Some((p, reach))
            })
            .collect();
        out.sort_by_key(|(p, _)| *p);
        out
    }

    /// An owned power of the kind `f` picks.
    fn pick(&self, f: impl Fn(&powers::PowerDef) -> bool) -> Option<PowerId> {
        let mut found: Vec<PowerId> = self
            .kit
            .owned()
            .filter(|p| !powers::is_free(*p))
            .filter(|p| powers::get(*p).is_some_and(|d| !d.passive() && !d.toggle && f(d)))
            .collect();
        found.sort();
        found.first().copied()
    }

    fn press(&mut self, input: &mut InputState, p: PowerId) {
        input.power = p.0;
        self.last_press = self.ticks;
    }

    /// Switches defensive toggles on (`on`) or off, one press at a time.
    fn toggles(&mut self, input: &mut InputState, c: &hw_net::CombatState, on: bool) {
        if self.ticks < self.last_press + PRESS_EVERY * 2 || input.power != 0 {
            return;
        }
        for p in self.kit.owned() {
            let Some(d) = powers::get(p) else { continue };
            if !d.toggle {
                continue;
            }
            let is_on = (c.toggles >> p.0) & 1 == 1;
            // Off only after a quiet while, so a fight's lulls keep them.
            let quiet = self.ticks > self.idle_since + TICK_HZ as u64 * 6;
            if (on && !is_on) || (!on && is_on && quiet) {
                self.press(input, p);
                return;
            }
        }
    }

    /// Moves toward `goal`, stopping `stop` metres short: flies when far
    /// and able, otherwise walks round walls and jumps over what blocks.
    fn travel(
        &mut self,
        input: &mut InputState,
        pos: DVec3,
        goal: DVec3,
        stop: f64,
        can_fly: bool,
    ) {
        let flat = DVec2::new(goal.x - pos.x, goal.z - pos.z);
        let d = flat.length();
        let climb = goal.y - pos.y;
        if can_fly && (d > 40.0 || climb > 12.0) {
            let dir = flat.normalize_or_zero();
            let alt = goal.y + if d > 30.0 { 25.0 } else { 2.0 };
            let dy = ((alt - pos.y) / 10.0).clamp(-1.0, 1.0);
            let go = if d > stop { 1.0 } else { 0.0 };
            let v = DVec3::new(dir.x * go, dy, dir.y * go);
            let v = if v.length() > 1.0 { v.normalize() } else { v };
            input.move_x = v.x as f32;
            input.move_y = v.y as f32;
            input.move_z = v.z as f32;
            input.fly = true;
            input.boost = d > 100.0;
            return;
        }
        if d <= stop {
            return;
        }
        // A way round walls, now and then.
        let stale = self.ticks > self.path_at + TICK_HZ as u64 * 3;
        if self.path.is_empty() || goal.distance(self.path_goal) > 6.0 || stale {
            self.path = nav::find_path(&self.solids, pos, goal).unwrap_or_else(|| vec![goal]);
            self.path_goal = goal;
            self.path_at = self.ticks;
        }
        while self
            .path
            .first()
            .is_some_and(|p| flat_distance(*p, pos) < 1.2 && self.path.len() > 1)
        {
            self.path.remove(0);
        }
        let next = self.path.first().copied().unwrap_or(goal);
        let mut dir = flat_dir(pos, next);
        // Stuck: no progress for a second means a sidestep and a jump.
        if self.ticks >= self.last_check + TICK_HZ as u64 {
            if pos.distance(self.last_pos) < 0.6 && d > stop + 2.0 {
                let a = self.rng.below(360) as f64 * std::f64::consts::PI / 180.0;
                self.unstick_dir = DVec2::new(a.cos(), a.sin());
                self.unstick_until = self.ticks + TICK_HZ as u64 / 2;
                self.path.clear();
            }
            self.last_pos = pos;
            self.last_check = self.ticks;
        }
        if self.ticks < self.unstick_until {
            dir = self.unstick_dir;
            input.jump = self.ticks.is_multiple_of(8);
        } else if next.y - pos.y > 1.0 {
            input.jump = self.ticks.is_multiple_of(8);
        }
        input.move_x = dir.x as f32;
        input.move_z = dir.y as f32;
        input.boost = d > 40.0;
    }
}

/// The nearest of `list` that is not being carried off.
fn nearest(list: Vec<&EntityState>, pos: DVec3) -> Option<&EntityState> {
    list.into_iter()
        .filter(|e| e.combat.is_some_and(|c| c.status != Status::Carried))
        .min_by(|a, b| {
            DVec3::from_array(a.pos)
                .distance(pos)
                .total_cmp(&DVec3::from_array(b.pos).distance(pos))
                .then(a.id.cmp(&b.id))
        })
}

fn flat_distance(a: DVec3, b: DVec3) -> f64 {
    DVec2::new(a.x - b.x, a.z - b.z).length()
}

/// The way from `a` to `b` on the ground, as a unit vector (x, z).
fn flat_dir(a: DVec3, b: DVec3) -> DVec2 {
    DVec2::new(b.x - a.x, b.z - a.z).normalize_or_zero()
}

/// Aims chest to chest.
fn aim_at(input: &mut InputState, from: DVec3, to: DVec3) {
    let v = (to + DVec3::Y * 1.2 - (from + DVec3::Y * 1.4)).normalize_or_zero();
    input.aim = [v.x as f32, v.y as f32, v.z as f32];
}

/// Runs a Herobot on a thread of its own, in real time, until it is home
/// or the game ends.
pub fn spawn(p: Params) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name(p.name.clone())
        .spawn(move || run(p))
        .expect("a thread for the Herobot")
}

fn run(p: Params) {
    engine::parallel::background_thread();
    let Ok(mut conn) = Connection::open(p.addr, &p.name) else {
        return;
    };
    let mut brain = Brain::new(&p);
    let step = Duration::from_secs_f64(1.0 / TICK_HZ as f64);
    let mut next = Instant::now();
    loop {
        if brain.drive(&mut conn) == Drive::Done {
            break;
        }
        next += step;
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else if now - next > Duration::from_secs(1) {
            next = now;
        }
    }
    // Let the goodbye out.
    for _ in 0..3 {
        conn.update(InputState::default());
        std::thread::sleep(Duration::from_millis(30));
    }
}
