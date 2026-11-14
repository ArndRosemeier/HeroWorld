//! The single-player server on its own thread.
//!
//! In single player the game runs the [`Host`] inside its own process. On
//! the main thread it would cost the frame its whole tick time (physics,
//! combat, people, traffic). [`HostThread`] owns the host on a thread of its
//! own; the game talks to it only through one ordered command queue, and
//! reads what it publishes.
//!
//! Ordering is guaranteed by construction, never by timing:
//! - Every command (a tick with its input, a purchase, a debug action) goes
//!   through one FIFO channel, so the host applies them exactly in the order
//!   the game sent them.
//! - Each frame the game calls [`HostThread::sync`]: it waits for the view
//!   it asked for in the frame before (taken after every command sent up to
//!   then), and asks for the next one. So frame N always shows the world
//!   after exactly the ticks of frames up to N-1, while the host works
//!   through frame N's ticks in parallel with the rest of frame N.
//! - [`HostThread::call`] runs a closure on the host after every command
//!   sent before it, and waits for its answer (debug tools, screenshots).
//!
//! There is never more than one view asked for and not yet received; an
//! assert checks it.
//!
//! The view lags the ticks sent by one frame, so the game clock for drawing
//! is not the view's tick but [`HostThread::sent_tick`] (every tick sent,
//! applied or not) plus [`TickClock::frac`]: it moves on by exactly each
//! frame's time. Things drawn from time alone (traffic, people, the sky)
//! read it, and glide every frame.

use crate::host::{Host, HostEvent, PlayerInfo};
use hw_core::NetId;
use hw_net::{EntityState, InputState, PlayerId, ToClient};
use std::collections::{BTreeMap, VecDeque};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

type Job = Box<dyn FnOnce(&mut Host, PlayerId) + Send>;

enum Cmd {
    /// One tick, with the local player's input for it.
    Tick(InputState),
    /// Publish the world as it is now.
    View,
    /// Run this on the host.
    Run(Job),
    /// Tell friends we are closing, then stop.
    End(Duration),
}

/// What the host publishes for the game: the world after the commands
/// sent before it was asked for.
#[derive(Default)]
pub struct HostView {
    /// The server tick the world stands at.
    pub tick: u64,
    /// Everything to draw.
    pub entities: Vec<EntityState>,
    /// Our hero.
    pub hero: Option<NetId>,
    /// Everyone playing (sent again only when it changed).
    pub players: BTreeMap<PlayerId, PlayerInfo>,
    /// Seconds the host thread spent on ticks since the last view.
    pub busy: f64,
    /// Ticks run since the last view.
    pub ticks: u32,
    /// Where the server's time went since the last view, in microseconds:
    /// the parts of the step, the host around it and the snapshot.
    pub parts: Vec<(&'static str, u64)>,
    /// Parts of a tick that overran their budget, in words (the hitch log
    /// prints them: a future stall names its cause).
    pub overruns: Vec<String>,
}

/// What a view adds to the game's queues (news, fight events, our sheet).
struct Published {
    view: HostView,
    players_changed: bool,
    news: Vec<ToClient>,
    fx: Vec<hw_net::Fx>,
    sheet: Option<hw_net::Sheet>,
    log: Vec<String>,
}

/// How far behind the ticks sent everything is drawn, in ticks: a frame
/// that sends one tick still has the tick after the drawn time in its
/// view, so it draws between two known ticks.
pub const DRAW_DELAY: f64 = 2.0;

/// Views kept to draw between ticks.
const HISTORY: usize = 6;

/// Paces the fixed server ticks to the frames: each frame adds its time,
/// runs the whole ticks that fit (at most [`TickClock::MAX_TICKS`]; a longer
/// frame drops the rest rather than falling ever further behind) and keeps
/// the part of a tick left over.
#[derive(Clone, Copy, Debug, Default)]
pub struct TickClock {
    left: f64,
}

impl TickClock {
    /// The most ticks one frame runs.
    pub const MAX_TICKS: u32 = 5;

    /// Adds a frame of `dt` seconds; returns how many ticks to run now.
    pub fn advance(&mut self, dt: f64) -> u32 {
        self.left += dt.max(0.0);
        let mut ticks = 0;
        while self.left >= crate::DT && ticks < Self::MAX_TICKS {
            self.left -= crate::DT;
            ticks += 1;
        }
        if ticks == Self::MAX_TICKS {
            self.left = 0.0;
        }
        ticks
    }

    /// How far into the next tick the frame is (0 to 1).
    pub fn frac(&self) -> f64 {
        self.left / crate::DT
    }
}

/// The [`Host`] on its own thread, with the local player `me`.
pub struct HostThread {
    me: PlayerId,
    voice: hw_net::VoiceBox,
    cmds: Sender<Cmd>,
    views: Receiver<Published>,
    thread: Option<JoinHandle<()>>,
    /// A view was asked for and has not arrived yet.
    asked: bool,
    /// The server tick once every tick sent so far has run.
    sent: u64,
    /// `sent` when the view on its way was asked for.
    asked_at: u64,
    /// The last few views' entities by tick, oldest first, to draw them
    /// between ticks ([`Self::entities_at`]).
    history: VecDeque<(u64, Vec<EntityState>)>,
    view: HostView,
    news: Vec<ToClient>,
    fx: Vec<hw_net::Fx>,
    log: Vec<String>,
    sheet: Option<hw_net::Sheet>,
}

impl HostThread {
    /// Moves `host` onto a thread of its own; `me` is the local player.
    /// Returns once the first view is in.
    pub fn spawn(host: Host, me: PlayerId) -> Self {
        let (cmds, rx) = channel::<Cmd>();
        let (tx, views) = channel::<Published>();
        let voice = host.voice_box();
        let thread = std::thread::Builder::new()
            .name("hw-server".into())
            .spawn(move || run(host, me, rx, tx))
            .expect("start the server thread");
        let mut s = Self {
            me,
            voice,
            cmds,
            views,
            thread: Some(thread),
            asked: false,
            sent: 0,
            asked_at: 0,
            history: VecDeque::new(),
            view: HostView::default(),
            news: Vec::new(),
            fx: Vec::new(),
            log: Vec::new(),
            sheet: None,
        };
        s.ask();
        s.receive();
        s.ask();
        s
    }

    pub fn me(&self) -> PlayerId {
        self.me
    }

    /// The team voice mailbox (see `Host::voice_box`); it works from any
    /// thread, without waiting for a tick.
    pub fn voice_box(&self) -> hw_net::VoiceBox {
        self.voice.clone()
    }

    /// One tick with `input` (runs on the server thread, in order).
    pub fn tick(&mut self, input: InputState) {
        self.send(Cmd::Tick(input));
        self.sent += 1;
    }

    /// The server tick once every tick sent so far has run: the game clock
    /// of this frame, a frame ahead of [`Self::view`].
    pub fn sent_tick(&self) -> u64 {
        self.sent
    }

    /// Once a frame, after this frame's ticks: takes the view asked for in
    /// the frame before and asks for the next. Waits if the server is not
    /// there yet. Returns how long it waited.
    pub fn sync(&mut self) -> Duration {
        let t = Instant::now();
        self.receive();
        let waited = t.elapsed();
        self.ask();
        waited
    }

    /// The server tick everything is drawn at this frame, `frac` of a tick
    /// on from the ticks sent: [`DRAW_DELAY`] behind them, so the view
    /// (which lags the ticks sent by this frame's ticks) is past it unless
    /// a frame ran more than one tick.
    pub fn draw_tick(&self, frac: f64) -> f64 {
        self.sent as f64 + frac - DRAW_DELAY
    }

    /// Every entity as it stood at [`Self::draw_tick`], between the two
    /// ticks round it: a hero, a fighter or a taxi moves on every frame,
    /// not in steps of a tick.
    pub fn entities_at(&self, frac: f64) -> Vec<EntityState> {
        crate::predict::blend_at(&self.history, self.draw_tick(frac))
    }

    /// The world as last published.
    pub fn view(&self) -> &HostView {
        &self.view
    }

    /// World news since the last call.
    pub fn take_news(&mut self) -> Vec<ToClient> {
        std::mem::take(&mut self.news)
    }

    /// What the server did with power keys since the last call, in words.
    pub fn take_log(&mut self) -> Vec<String> {
        std::mem::take(&mut self.log)
    }

    /// Fight events since the last call.
    pub fn take_fx(&mut self) -> Vec<hw_net::Fx> {
        std::mem::take(&mut self.fx)
    }

    /// Our hero file, if it changed since the last call.
    pub fn take_sheet(&mut self) -> Option<hw_net::Sheet> {
        self.sheet.take()
    }

    /// Runs `f` on the host once every command sent before has been
    /// applied (no waiting).
    pub fn post(&mut self, f: impl FnOnce(&mut Host, PlayerId) + Send + 'static) {
        self.send(Cmd::Run(Box::new(f)));
    }

    /// Runs `f` on the host once every command sent before has been
    /// applied, and waits for its answer. For debug tools and screenshots:
    /// it stalls the frame for as long as the host is busy.
    pub fn call<R: Send + 'static>(
        &mut self,
        f: impl FnOnce(&mut Host, PlayerId) -> R + Send + 'static,
    ) -> R {
        let (tx, rx) = channel();
        self.send(Cmd::Run(Box::new(move |h: &mut Host, me| {
            let _ = tx.send(f(h, me));
        })));
        match rx.recv() {
            Ok(r) => r,
            Err(_) => self.died(),
        }
    }

    fn send(&mut self, c: Cmd) {
        if self.cmds.send(c).is_err() {
            self.died();
        }
    }

    fn ask(&mut self) {
        assert!(!self.asked, "one view at a time");
        self.send(Cmd::View);
        self.asked = true;
        self.asked_at = self.sent;
    }

    fn receive(&mut self) {
        assert!(self.asked, "a view to wait for");
        let p = match self.views.recv() {
            Ok(p) => p,
            Err(_) => self.died(),
        };
        self.asked = false;
        // The view stands where every tick sent before it was asked for
        // has run; the ticks sent since come on top. (The host's own tick
        // can jump, e.g. when a save is loaded: the clock follows it.)
        let jumped = p.view.tick != self.asked_at;
        self.sent = p.view.tick + (self.sent - self.asked_at);
        if jumped || self.history.back().is_some_and(|(t, _)| p.view.tick < *t) {
            // The host's clock jumped: what came before is another time.
            self.history.clear();
        }
        if self.history.back().is_none_or(|(t, _)| p.view.tick > *t) {
            self.history
                .push_back((p.view.tick, p.view.entities.clone()));
            while self.history.len() > HISTORY {
                self.history.pop_front();
            }
        }
        let players = if p.players_changed {
            p.view.players
        } else {
            std::mem::take(&mut self.view.players)
        };
        self.view = HostView { players, ..p.view };
        self.news.extend(p.news);
        self.fx.extend(p.fx);
        if self.log.len() > 400 {
            self.log.clear();
        }
        self.log.extend(p.log);
        if p.sheet.is_some() {
            self.sheet = p.sheet;
        }
    }

    /// The server thread stopped: pass its panic on.
    fn died(&mut self) -> ! {
        match self.thread.take().map(JoinHandle::join) {
            Some(Err(panic)) => std::panic::resume_unwind(panic),
            _ => panic!("the server thread stopped"),
        }
    }

    /// Tells friends the world is closing (waiting up to `wait` for them to
    /// hear it) and stops the thread.
    pub fn end(&mut self, wait: Duration) {
        let Some(t) = self.thread.take() else { return };
        if self.cmds.send(Cmd::End(wait)).is_ok() {
            if let Err(panic) = t.join() {
                if !std::thread::panicking() {
                    std::panic::resume_unwind(panic);
                }
            }
        }
    }
}

impl Drop for HostThread {
    fn drop(&mut self) {
        self.end(Duration::ZERO);
    }
}

fn run(mut host: Host, me: PlayerId, rx: Receiver<Cmd>, tx: Sender<Published>) {
    // Screenshots: `HW_COLOSSUS_BREAK=<n>` breaks the first n of the
    // colossus's parts (knees, then the cannon) once it is out.
    let colossus_break = std::env::var("HW_COLOSSUS_BREAK")
        .ok()
        .and_then(|v| v.parse::<u8>().ok());
    // `HW_COLOSSUS_RAMPAGE=1`: it smashes its way through town even with
    // no hero near (screenshots).
    if std::env::var("HW_COLOSSUS_RAMPAGE").is_ok_and(|v| v == "1") {
        crate::robots::rampage_unseen(true);
    }
    // `HW_CASE`: cases open at once, the hero at the first (screenshots). A
    // comma-separated list of `<job>[@<place>]` (`hostages`, `crew@street`,
    // `takeover@station`; jobs takeover|evidence|hostages|crew, places
    // building|street|plaza|tunnel|station|grotto|ruin|lair; a building by
    // default, one to a building) `chain` (a whole case, several steps) or `hideout[:<n>]` (crime n of
    // the police board).
    let mut case: Vec<String> = std::env::var("HW_CASE")
        .map(|v| v.split(',').map(|k| k.trim().to_string()).collect())
        .unwrap_or_default();
    // `hideout:<n>@thing`: the hero is put beside the thing to find once
    // the gang stands (screenshots).
    let mut thing_case: Option<u16> = None;
    // `HW_POLICE=<s>`: that many seconds in, a cruiser is called to where
    // the hero stands and comes straight down (screenshots).
    let mut police_at: Option<f64> = std::env::var("HW_POLICE").ok().and_then(|v| v.parse().ok());
    let mut busy = Duration::ZERO;
    let mut ticks = 0u32;
    let mut players: Option<BTreeMap<PlayerId, PlayerInfo>> = None;
    let mut heard = Vec::new();
    while let Ok(c) = rx.recv() {
        match c {
            Cmd::Tick(input) => {
                let t = Instant::now();
                host.sim.set_input(me, input);
                for e in host.tick() {
                    // What friends say reaches our chat window too.
                    if let HostEvent::Chat { player, text, .. } = &e {
                        heard.push(ToClient::Chat {
                            from: *player,
                            text: text.clone(),
                        });
                    }
                    // Team lines from friends reach our Team tab too.
                    if let HostEvent::TeamChat { player, text, .. } = &e {
                        heard.push(ToClient::TeamChat {
                            from: *player,
                            text: text.clone(),
                        });
                    }
                    println!("{e:?}");
                }
                if let Some(n) = colossus_break {
                    host.sim.break_colossus(n);
                }
                if police_at.is_some_and(|s| host.sim.tick as f64 * crate::DT >= s) {
                    let me_at = host
                        .sim
                        .snapshot()
                        .iter()
                        .find(|e| e.kind == hw_net::EntityKind::Hero && e.owner == Some(me))
                        .map(|e| glam::DVec3::from_array(e.pos));
                    if let Some(at) = me_at {
                        police_at = None;
                        host.sim.police.quick = true;
                        // The hero stands in the door.
                        host.sim.call_police(at, glam::DVec2::X);
                    }
                }
                if let Some(n) = thing_case {
                    let sim = &mut host.sim;
                    let clue = sim
                        .missions
                        .case(n)
                        .filter(|r| r.stage == crate::missions::INSIDE)
                        .and_then(|r| r.clues().first().map(|c| c.0));
                    if let (Some(at), Some(hero)) = (clue, sim.hero_of(me)) {
                        thing_case = None;
                        sim.set_body(
                            hero,
                            crate::collision::Body {
                                pos: at + glam::DVec3::new(2.2, 0.3, 0.0),
                                grounded: true,
                                ..Default::default()
                            },
                        );
                    }
                }
                let mut first = true;
                for (i, c) in std::mem::take(&mut case).into_iter().enumerate() {
                    let sim = &mut host.sim;
                    if !sim.missions.available() {
                        continue;
                    }
                    let number =
                        force_case(sim, c.trim_end_matches("@in").trim_end_matches("@thing"), i);
                    // The hero stands at the first one's way in.
                    let hero = sim.hero_of(me);
                    // Out in the open, a little way off: not in the crew's
                    // middle.
                    let inside = c.ends_with("@in");
                    if c.ends_with("@thing") {
                        thing_case = number;
                    }
                    let places = sim.missions.places().cloned();
                    let at = number.and_then(|n| sim.missions.case(n)).map(|r| {
                        let site = r.site();
                        // `@in` (screenshots): in the middle of the building,
                        // on its ground floor.
                        let middle = inside
                            .then(|| {
                                let b = &places.as_ref()?.floors.buildings[site.building?];
                                Some(b.to_town(glam::DVec3::new(
                                    b.graph.size.x * 0.5,
                                    0.6,
                                    b.graph.size.y * 0.35,
                                )))
                            })
                            .flatten();
                        if let Some(m) = middle {
                            m
                        } else if site.building.is_none() && !site.place.underground() {
                            let f = site.facing * 22.0;
                            site.at + glam::DVec3::new(f.x, 0.5, f.y)
                        } else {
                            r.objective()
                        }
                    });
                    if let (true, Some(at), Some(hero)) = (std::mem::take(&mut first), at, hero) {
                        sim.set_body(
                            hero,
                            crate::collision::Body {
                                pos: at,
                                grounded: true,
                                ..Default::default()
                            },
                        );
                    }
                }
                busy += t.elapsed();
                ticks += 1;
            }
            Cmd::View => {
                let changed = players.as_ref() != Some(host.players());
                let now = if changed {
                    players = Some(host.players().clone());
                    host.players().clone()
                } else {
                    BTreeMap::new()
                };
                let snap = Instant::now();
                let entities = host.sim.snapshot();
                let snap_us = snap.elapsed().as_micros() as u64;
                let busy_s = std::mem::take(&mut busy).as_secs_f64();
                let mut parts = host.sim.take_step_times();
                let stepped: u64 = parts.iter().map(|p| p.1).sum();
                parts.push((
                    "srv host around the step us",
                    ((busy_s * 1e6) as u64).saturating_sub(stepped),
                ));
                parts.push(("srv snapshot us", snap_us));
                let view = HostView {
                    tick: host.sim.tick,
                    entities,
                    hero: host.sim.hero_of(me),
                    players: now,
                    busy: busy_s,
                    ticks: std::mem::take(&mut ticks),
                    parts,
                    overruns: host.sim.take_overruns(),
                };
                let p = Published {
                    view,
                    players_changed: changed,
                    news: {
                        let mut news = host.take_local_news();
                        news.append(&mut heard);
                        news
                    },
                    fx: host.take_fx(),
                    sheet: host.take_local_sheet(),
                    log: host.take_power_log(),
                };
                if tx.send(p).is_err() {
                    break;
                }
            }
            Cmd::Run(f) => f(&mut host, me),
            Cmd::End(wait) => {
                host.end_session("the host closed the game", wait);
                break;
            }
        }
    }
}

/// Opens the case `token` of `HW_CASE` (see there), the `i`th asked for.
fn force_case(sim: &mut crate::Sim, token: &str, i: usize) -> Option<u16> {
    use crate::missions::{CaseKind, Place};
    let gang = 1 + i % 2;
    // `hideout` or `hideout:<crime number>`: a crime of the police board.
    if let Some(rest) = token.strip_prefix("hideout") {
        let n = rest
            .strip_prefix(':')
            .and_then(|n| n.parse().ok())
            .unwrap_or(1 + i as u16);
        return sim.missions.force_crime(&mut sim.world, n);
    }
    if token == "chain" {
        return sim.missions.force_plan(&mut sim.world, gang);
    }
    let (job, place) = token.split_once('@').unwrap_or((token, "building"));
    let job = match job {
        "evidence" => CaseKind::Evidence,
        "hostages" => CaseKind::Hostages,
        "crew" => CaseKind::Crew,
        _ => CaseKind::Takeover,
    };
    let place = Place::ALL
        .into_iter()
        .find(|p| format!("{p:?}").eq_ignore_ascii_case(place))
        .unwrap_or(Place::Building);
    if place == Place::Building && job.fits(place) {
        let buildings = sim.missions.buildings();
        if i >= buildings {
            return None;
        }
        return Some(sim.missions.force(&mut sim.world, job, i, gang));
    }
    sim.missions.force_at(&mut sim.world, job, place, gang)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;
    use hw_core::Seed;

    fn walk(k: u32) -> InputState {
        InputState {
            move_x: if k % 40 < 20 { 1.0 } else { -0.5 },
            move_z: 0.7,
            jump: k % 50 == 7,
            ..Default::default()
        }
    }

    /// The threaded host shows exactly what the host does on its own, frame
    /// by frame one frame behind, whatever the timing.
    #[test]
    fn same_world_as_the_host_on_its_own() {
        let mut plain = Host::new(Seed(7));
        let me = plain.join_local("solo");
        let mut threaded = Host::new(Seed(7));
        let me2 = threaded.join_local("solo");
        assert_eq!(me, me2);
        let mut t = HostThread::spawn(threaded, me2);
        let mut expected = vec![plain.sim.snapshot()];
        let mut k = 0;
        for frame in 0..40u32 {
            for _ in 0..(frame % 3) {
                t.tick(walk(k));
                plain.sim.set_input(me, walk(k));
                plain.tick();
                k += 1;
            }
            if frame % 7 == 3 {
                // Slow frames on either side must not change anything.
                std::thread::sleep(Duration::from_millis(3));
            }
            t.sync();
            // This frame shows the world as of the end of the frame before.
            assert_eq!(t.view().entities, expected[frame as usize], "frame {frame}");
            expected.push(plain.sim.snapshot());
        }
        assert_eq!(t.view().hero, plain.sim.hero_of(me));
        assert_eq!(t.view().players.len(), 1);
    }

    /// The game clock for drawing moves on by exactly each frame's time,
    /// whatever the frame rate and however many ticks a frame sends: cars
    /// and people placed by it glide instead of jumping back and forth.
    /// (Until 0.132 it read the view's tick, a frame behind the ticks
    /// sent: every frame that sent a tick stepped a tick back, and the next
    /// leapt on.)
    #[test]
    fn the_drawing_clock_moves_by_each_frame() {
        let mut host = Host::new(Seed(5));
        let me = host.join_local("solo");
        let mut t = HostThread::spawn(host, me);
        let mut clock = TickClock::default();
        let start = t.view().tick;
        let time = |t: &HostThread, c: &TickClock| (t.sent_tick() as f64 + c.frac()) * crate::DT;
        let mut was = time(&t, &clock);
        let mut total = 0.0;
        // 144 Hz, 60 Hz, 50 Hz and uneven frames.
        for frame in 0..300u32 {
            let dt = match frame / 75 {
                0 => 1.0 / 144.0,
                1 => 1.0 / 60.0,
                2 => 1.0 / 50.0,
                _ => [0.004, 0.021, 0.013, 0.031, 0.009][frame as usize % 5],
            };
            for _ in 0..clock.advance(dt) {
                t.tick(InputState::default());
            }
            t.sync();
            total += dt;
            let now = time(&t, &clock);
            assert!(
                (now - was - dt).abs() < 1e-9,
                "frame {frame}: the clock moved {:.4} s in a {dt:.4} s frame",
                now - was
            );
            assert!(
                (now - (start as f64 * crate::DT + total)).abs() < 1e-6,
                "frame {frame}: the clock drifted"
            );
            // The view is never ahead of the ticks sent.
            assert!(t.view().tick <= t.sent_tick());
            was = now;
        }
        // The host's tick jumps (a save loaded): the clock follows it once
        // the view after the jump is in, and keeps counting what was sent.
        t.post(|h, _| h.sim.tick += 1000);
        t.tick(InputState::default());
        t.sync();
        t.tick(InputState::default());
        t.sync();
        assert_eq!(t.sent_tick(), t.call(|h, _| h.sim.tick));
    }

    /// Heroes, fighters and taxis are drawn between ticks, every frame:
    /// exactly where the host had them, blended between the two ticks
    /// round the drawing clock, at any frame rate up to a tick a frame.
    /// (Until 0.133 single player drew them as the last view had them: at
    /// 144 Hz they stood still four frames and leapt on the fifth.)
    #[test]
    fn entities_are_drawn_between_ticks() {
        let mut plain = Host::new(Seed(9));
        let me = plain.join_local("solo");
        let mut threaded = Host::new(Seed(9));
        threaded.join_local("solo");
        let mut t = HostThread::spawn(threaded, me);
        let hero = plain.sim.hero_of(me).expect("a hero");
        let start = t.sent_tick();
        assert_eq!(start, plain.sim.tick);
        // What the host had after each tick from the start.
        let mut by_tick = vec![plain.sim.snapshot()];
        let at = |snap: &[EntityState]| {
            DVec3::from_array(snap.iter().find(|e| e.id == hero).expect("hero").pos)
        };
        let mut clock = TickClock::default();
        let mut k = 0;
        let mut was: Option<DVec3> = None;
        let mut moved_frames = 0;
        for frame in 0..400u32 {
            let dt = match frame / 100 {
                0 => 1.0 / 144.0,
                1 => 1.0 / 60.0,
                2 => 1.0 / 40.0,
                _ => [0.004, 0.021, 0.013, 0.031, 0.009][frame as usize % 5],
            };
            for _ in 0..clock.advance(dt) {
                let input = InputState {
                    move_z: 1.0,
                    move_x: if k % 60 < 30 { 0.4 } else { -0.4 },
                    ..Default::default()
                };
                t.tick(input);
                plain.sim.set_input(me, input);
                plain.tick();
                by_tick.push(plain.sim.snapshot());
                k += 1;
            }
            t.sync();
            let r = t.draw_tick(clock.frac()) - start as f64;
            if r < 0.0 {
                continue;
            }
            let i = r.floor() as usize;
            let f = r - i as f64;
            let want = at(&by_tick[i]).lerp(at(&by_tick[(i + 1).min(by_tick.len() - 1)]), f);
            let drawn = at(&t.entities_at(clock.frac()));
            assert!(
                drawn.distance(want) < 1e-6,
                "frame {frame}: drawn {drawn:?}, the host had {want:?} at tick {r:.3}"
            );
            if let Some(w) = was {
                if drawn.distance(w) > 1e-9 {
                    moved_frames += 1;
                }
            }
            was = Some(drawn);
        }
        assert!(
            moved_frames > 350,
            "the hero moved in only {moved_frames} frames"
        );
    }

    /// A long frame runs at most `MAX_TICKS` and drops the rest.
    #[test]
    fn a_long_frame_runs_a_few_ticks_and_drops_the_rest() {
        let mut c = TickClock::default();
        assert_eq!(c.advance(crate::DT * 0.5), 0);
        assert_eq!(c.advance(crate::DT * 0.75), 1);
        assert!((c.frac() - 0.25).abs() < 1e-9);
        assert_eq!(c.advance(2.0), TickClock::MAX_TICKS);
        assert_eq!(c.frac(), 0.0);
    }

    /// A call sees every command sent before it, and nothing after.
    #[test]
    fn a_call_runs_after_what_was_sent_before() {
        let mut host = Host::new(Seed(3));
        let me = host.join_local("solo");
        let mut t = HostThread::spawn(host, me);
        let start = t.call(|h, _| h.sim.tick);
        for _ in 0..12 {
            t.tick(InputState::default());
        }
        assert_eq!(t.call(|h, _| h.sim.tick), start + 12);
        t.post(|h, _| h.sim.step());
        assert_eq!(t.call(|h, _| h.sim.tick), start + 13);
    }

    #[test]
    #[should_panic(expected = "boom")]
    fn a_panic_on_the_server_reaches_the_game() {
        let mut host = Host::new(Seed(1));
        let me = host.join_local("solo");
        let mut t = HostThread::spawn(host, me);
        t.post(|_, _| panic!("boom"));
        t.sync();
    }
}
