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

use crate::host::{Host, PlayerInfo};
use hw_core::NetId;
use hw_net::{EntityState, InputState, PlayerId, ToClient};
use std::collections::BTreeMap;
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
}

/// What a view adds to the game's queues (news, fight events, our sheet).
struct Published {
    view: HostView,
    players_changed: bool,
    news: Vec<ToClient>,
    fx: Vec<hw_net::Fx>,
    sheet: Option<hw_net::Sheet>,
}

/// The [`Host`] on its own thread, with the local player `me`.
pub struct HostThread {
    me: PlayerId,
    cmds: Sender<Cmd>,
    views: Receiver<Published>,
    thread: Option<JoinHandle<()>>,
    /// A view was asked for and has not arrived yet.
    asked: bool,
    view: HostView,
    news: Vec<ToClient>,
    fx: Vec<hw_net::Fx>,
    sheet: Option<hw_net::Sheet>,
}

impl HostThread {
    /// Moves `host` onto a thread of its own; `me` is the local player.
    /// Returns once the first view is in.
    pub fn spawn(host: Host, me: PlayerId) -> Self {
        let (cmds, rx) = channel::<Cmd>();
        let (tx, views) = channel::<Published>();
        let thread = std::thread::Builder::new()
            .name("hw-server".into())
            .spawn(move || run(host, me, rx, tx))
            .expect("start the server thread");
        let mut s = Self {
            me,
            cmds,
            views,
            thread: Some(thread),
            asked: false,
            view: HostView::default(),
            news: Vec::new(),
            fx: Vec::new(),
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

    /// One tick with `input` (runs on the server thread, in order).
    pub fn tick(&mut self, input: InputState) {
        self.send(Cmd::Tick(input));
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

    /// The world as last published.
    pub fn view(&self) -> &HostView {
        &self.view
    }

    /// World news since the last call.
    pub fn take_news(&mut self) -> Vec<ToClient> {
        std::mem::take(&mut self.news)
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
    }

    fn receive(&mut self) {
        assert!(self.asked, "a view to wait for");
        let p = match self.views.recv() {
            Ok(p) => p,
            Err(_) => self.died(),
        };
        self.asked = false;
        let players = if p.players_changed {
            p.view.players
        } else {
            std::mem::take(&mut self.view.players)
        };
        self.view = HostView { players, ..p.view };
        self.news.extend(p.news);
        self.fx.extend(p.fx);
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
    let mut busy = Duration::ZERO;
    let mut ticks = 0u32;
    let mut players: Option<BTreeMap<PlayerId, PlayerInfo>> = None;
    while let Ok(c) = rx.recv() {
        match c {
            Cmd::Tick(input) => {
                let t = Instant::now();
                host.sim.set_input(me, input);
                for e in host.tick() {
                    println!("{e:?}");
                }
                if let Some(n) = colossus_break {
                    host.sim.break_colossus(n);
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
                let view = HostView {
                    tick: host.sim.tick,
                    entities: host.sim.snapshot(),
                    hero: host.sim.hero_of(me),
                    players: now,
                    busy: std::mem::take(&mut busy).as_secs_f64(),
                    ticks: std::mem::take(&mut ticks),
                };
                let p = Published {
                    view,
                    players_changed: changed,
                    news: host.take_local_news(),
                    fx: host.take_fx(),
                    sheet: host.take_local_sheet(),
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

#[cfg(test)]
mod tests {
    use super::*;
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
