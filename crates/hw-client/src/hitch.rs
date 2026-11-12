//! The hitch guard: notices frames that come late enough to be seen as a
//! stutter, and writes what was going on into `logs/hitches.log` (beside
//! `start.bat`; `HW_HITCH_LOG=<file>` puts it elsewhere, `HW_HITCH_LOG=0`
//! turns it off), so each hitch can be reproduced and fixed.
//!
//! A report says how long the frame took and where the time went: the
//! game's own sections (marked with [`mark`]), what the game counted that
//! frame ([`count`]: cells built, models uploaded...), the work running on
//! other threads ([`job`]), where the hero was (position, camera, hour,
//! the ready-made `HW_*` line to start there), memory, the frames before,
//! and what happened in the seconds before ([`event`]). Engine writes its
//! own block for the same frame just above ours when its part (animation,
//! GPU uploads, drawing) was slow.
//!
//! A frame is a hitch when it takes longer than `HW_HITCH_MS` (default
//! 25 ms) and also well over the usual frame time, so a slow PC that runs
//! evenly at 30 frames a second does not fill the log; or when the game's
//! update alone does (which shows even where drawing is slow anyway). At most
//! [`REPORTS_PER_MINUTE`] reports a minute are written in full; a summary
//! line every minute (every five when all is smooth) counts the rest.
//!
//! Everything lives on the main thread (the frame loop's), in a thread
//! local, so any view can call [`count`] or [`event`] without plumbing.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::PathBuf;
use std::time::Instant;

/// Full reports at most this many a minute (the rest are only counted).
pub const REPORTS_PER_MINUTE: u32 = 30;
/// A frame must also take this many times the usual frame time.
const RATIO: f32 = 1.75;
/// Frames longer than this are a pause (window hidden, a breakpoint, the
/// PC asleep), not something to fix in the game.
const PAUSE_MS: f32 = 2000.0;
/// Seconds of events shown before a hitch.
const EVENT_SECONDS: f32 = 6.0;
/// The log stops taking full reports past this size (a new start begins
/// a new file; the last one is kept as `hitches.prev.log`).
const MAX_LOG_BYTES: u64 = 16 << 20;

/// A stretch is slow when a window of this many seconds averages under
/// [`SLOW_FPS`]: it is written as one `SLOW STRETCH` report (single hitches
/// are capped per minute; a long slow time is not a hitch but a state).
const WINDOW_SECONDS: f32 = 3.0;
const SLOW_FPS: f32 = 20.0;

thread_local! {
    static GUARD: RefCell<Option<Guard>> = const { RefCell::new(None) };
}

/// Starts the guard (once, on the main thread, before the first frame):
/// opens a new log and writes its header. `lines` are facts for the
/// header (seed, settings...).
pub fn install(lines: &[(&str, String)]) {
    let Some(path) = log_path() else {
        return;
    };
    let floor = std::env::var("HW_HITCH_MS")
        .ok()
        .and_then(|s| s.parse::<f32>().ok())
        .filter(|ms| ms.is_finite() && *ms > 0.0)
        .unwrap_or(25.0);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if path.exists() {
        let _ = std::fs::rename(&path, path.with_extension("prev.log"));
    }
    let mut head = format!(
        "HeroWorld {} hitch log, started {}\n",
        hw_core::version(),
        clock_time()
    );
    let _ = writeln!(
        head,
        "  system: {} {}, {} threads",
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::thread::available_parallelism().map_or(0, |n| n.get())
    );
    let _ = writeln!(
        head,
        "  a hitch: a frame over {floor:.0} ms and over {RATIO}x the usual frame (HW_HITCH_MS)"
    );
    for (k, v) in lines {
        let _ = writeln!(head, "  {k}: {v}");
    }
    head.push('\n');
    if std::fs::write(&path, head).is_err() {
        eprintln!("hitch log {} could not be written", path.display());
        return;
    }
    eprintln!("hitch log: {}", path.display());
    GUARD.with(|g| *g.borrow_mut() = Some(Guard::new(path, floor)));
}

/// The log's path, if the guard runs.
pub fn path() -> Option<PathBuf> {
    GUARD.with(|g| g.borrow().as_ref().map(|g| g.path.clone()))
}

fn log_path() -> Option<PathBuf> {
    match std::env::var_os("HW_HITCH_LOG") {
        Some(v) if v == "0" || v.is_empty() => None,
        Some(v) => Some(PathBuf::from(v)),
        // Beside start.bat (the working folder), or in the profile folder
        // if that cannot be written.
        None => {
            let here = PathBuf::from("logs");
            if std::fs::create_dir_all(&here).is_ok() {
                Some(here.join("hitches.log"))
            } else {
                crate::profile::Profile::dir().map(|d| d.join("logs").join("hitches.log"))
            }
        }
    }
}

/// Frames between two resource counts.
const CENSUS_FRAMES: u64 = 120;

/// Call first thing every frame. The frame before is judged here, so a
/// report also covers Engine's work after the game's update (animation,
/// uploads, drawing). The update's time ends when the returned value is
/// dropped (at the end of the frame closure, early returns included).
#[must_use = "keep it until the end of the frame"]
pub fn begin(world: &mut engine::prelude::World, frame_time: f32) -> FrameScope {
    with(|g| {
        g.begin(frame_time);
        if g.frame_no.is_multiple_of(CENSUS_FRAMES) {
            g.census = Some(world.resource_census());
        }
        // Engine's own block (animation, uploads, drawing) goes into the
        // same file, while the usual frame is well inside Engine's limit
        // (else every frame would be one) and the minute's reports last.
        let on = !g.full
            && g.minute.reported < REPORTS_PER_MINUTE
            && g.usual_ms.is_some_and(|u| u * RATIO <= g.engine_ms);
        if on != g.engine_log {
            g.engine_log = on;
            world.set_hitch_log(on.then(|| g.path.clone()));
        }
    });
    FrameScope(())
}

/// Ends the update's time when dropped (see [`begin`]).
pub struct FrameScope(());

impl Drop for FrameScope {
    fn drop(&mut self) {
        with(|g| g.end_update());
    }
}

/// The code from here on, up to the next mark, belongs to `section`.
pub fn mark(section: &'static str) {
    with(|g| g.mark(section));
}

/// Counts something the frame did (cells built, models uploaded, ...).
pub fn count(what: &'static str, n: u64) {
    if n > 0 {
        with(|g| match g.counts.iter_mut().find(|(k, _)| *k == what) {
            Some(c) => c.1 += n,
            None => g.counts.push((what, n)),
        });
    }
}

/// Something worth knowing when a hitch follows soon after (an area
/// moved, a town swapped in, a teleport...).
pub fn event(text: impl Into<String>) {
    let text = text.into();
    with(|g| {
        let t = g.started.elapsed().as_secs_f32();
        g.events.push_back((t, text));
        while g.events.len() > 64 {
            g.events.pop_front();
        }
    });
}

/// Work running on another thread (a town being generated, the next
/// area, characters being built): `None` when it is not running.
pub fn job(name: &'static str, state: Option<String>) {
    with(|g| {
        g.jobs.retain(|(k, _)| *k != name);
        if let Some(s) = state {
            g.jobs.push((name, s));
        }
    });
}

/// A line straight into the log (milestones of the start, ...).
pub fn line(text: impl Into<String>) {
    let text = text.into();
    with(|g| {
        if !g.full {
            let t = g.started.elapsed().as_secs_f32();
            g.write(&format!("{t:7.2} s  {text}\n"));
        }
    });
}

/// Facts about the moment (where the hero is, the hour...), kept until
/// set again. `what` again replaces the old value.
pub fn context(what: &'static str, value: impl Into<String>) {
    let value = value.into();
    with(|g| match g.context.iter_mut().find(|(k, _)| *k == what) {
        Some(c) => c.1 = value,
        None => g.context.push((what, value)),
    });
}

/// Starts gathering every frame's times and sections (for a benchmark,
/// see `bench.rs`) until [`take_stats`].
pub fn start_stats() {
    with(|g| g.stats = Some(Stats::default()));
}

/// The frames gathered since [`start_stats`], and stops gathering.
pub fn take_stats() -> Option<Stats> {
    let mut out = None;
    with(|g| out = g.stats.take());
    out
}

/// Every frame's times, and the game's sections summed over them.
#[derive(Clone, Debug, Default)]
pub struct Stats {
    /// Whole frames and the game's update in them (ms).
    pub wall_ms: Vec<f32>,
    pub update_ms: Vec<f32>,
    /// Each section: its total over all frames and its worst frame (ms).
    pub sections: Vec<(&'static str, f64, f32)>,
    /// What the frames counted, summed.
    pub counts: Vec<(&'static str, u64)>,
    pub hitches: u32,
}

impl Stats {
    fn add(&mut self, wall_ms: f32, r: &FrameRecord, hitch: bool) {
        self.wall_ms.push(wall_ms);
        self.update_ms.push(r.update_ms);
        for &(name, ms) in &r.sections {
            match self.sections.iter_mut().find(|(k, _, _)| *k == name) {
                Some(s) => {
                    s.1 += f64::from(ms);
                    s.2 = s.2.max(ms);
                }
                None => self.sections.push((name, f64::from(ms), ms)),
            }
        }
        for &(name, n) in &r.counts {
            match self.counts.iter_mut().find(|(k, _)| *k == name) {
                Some(c) => c.1 += n,
                None => self.counts.push((name, n)),
            }
        }
        self.hitches += u32::from(hitch);
    }
}

fn with(f: impl FnOnce(&mut Guard)) {
    GUARD.with(|g| {
        if let Some(g) = g.borrow_mut().as_mut() {
            f(g);
        }
    });
}

/// One frame's sections and counts.
#[derive(Default)]
struct FrameRecord {
    sections: Vec<(&'static str, f32)>,
    counts: Vec<(&'static str, u64)>,
    update_ms: f32,
}

struct Guard {
    path: PathBuf,
    floor_ms: f32,
    started: Instant,
    /// When this frame's update started, and the section running now.
    frame_start: Option<Instant>,
    section: Option<(&'static str, Instant)>,
    sections: Vec<(&'static str, f32)>,
    counts: Vec<(&'static str, u64)>,
    update_ms: f32,
    frame_no: u64,
    /// The usual frame and update times (slow averages of frames that
    /// were not hitches).
    usual_ms: Option<f32>,
    usual_update_ms: Option<f32>,
    recent: VecDeque<(f32, f32)>,
    events: VecDeque<(f32, String)>,
    jobs: Vec<(&'static str, String)>,
    context: Vec<(&'static str, String)>,
    minute: Minute,
    last_memory: Option<(f32, Memory)>,
    /// Engine's resource census (counts of what the world and the GPU
    /// hold), taken now and then; the minute's summary writes it, so a
    /// count that keeps climbing over a session shows.
    census: Option<engine::census::ResourceCensus>,
    full: bool,
    /// Engine's hitch limit (`ENGINE_HITCH_MS`, work per frame), and
    /// whether Engine writes into our log now.
    engine_ms: f32,
    engine_log: bool,
    /// Every frame, while a benchmark runs.
    stats: Option<Stats>,
    /// The frames of the current window (see [`WINDOW_SECONDS`]).
    window: Window,
    /// A slow stretch has been reported and has not ended.
    slow_since: Option<(f32, u32)>,
}

/// Frames summed over a window, for the slow-stretch report.
#[derive(Default)]
struct Window {
    frames: u32,
    wall_ms: f64,
    update_ms: f64,
    worst_ms: f32,
    sections: Vec<(&'static str, f64)>,
    counts: Vec<(&'static str, u64)>,
}

impl Window {
    fn add(&mut self, wall_ms: f32, r: &FrameRecord) {
        self.frames += 1;
        self.wall_ms += f64::from(wall_ms);
        self.update_ms += f64::from(r.update_ms);
        self.worst_ms = self.worst_ms.max(wall_ms);
        for &(name, ms) in &r.sections {
            match self.sections.iter_mut().find(|(k, _)| *k == name) {
                Some(e) => e.1 += f64::from(ms),
                None => self.sections.push((name, f64::from(ms))),
            }
        }
        for &(name, n) in &r.counts {
            match self.counts.iter_mut().find(|(k, _)| *k == name) {
                Some(e) => e.1 += n,
                None => self.counts.push((name, n)),
            }
        }
    }
}

/// Frames and hitches since the last summary line.
#[derive(Default)]
struct Minute {
    since: f32,
    frames: u32,
    total_ms: f64,
    worst_ms: f32,
    hitches: u32,
    reported: u32,
    pauses: u32,
}

impl Guard {
    fn new(path: PathBuf, floor_ms: f32) -> Self {
        Self {
            path,
            floor_ms,
            started: Instant::now(),
            frame_start: None,
            section: None,
            sections: Vec::with_capacity(48),
            counts: Vec::with_capacity(16),
            update_ms: 0.0,
            frame_no: 0,
            usual_ms: None,
            usual_update_ms: None,
            recent: VecDeque::with_capacity(16),
            events: VecDeque::new(),
            jobs: Vec::new(),
            context: Vec::new(),
            minute: Minute::default(),
            last_memory: None,
            census: None,
            full: false,
            engine_ms: std::env::var("ENGINE_HITCH_MS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(33.0),
            engine_log: false,
            stats: None,
            window: Window::default(),
            slow_since: None,
        }
    }

    fn begin(&mut self, frame_time: f32) {
        let now = Instant::now();
        if let Some(start) = self.frame_start {
            let wall_ms = (now - start).as_secs_f32() * 1000.0;
            let record = FrameRecord {
                sections: std::mem::take(&mut self.sections),
                counts: std::mem::take(&mut self.counts),
                update_ms: self.update_ms,
            };
            self.judge(wall_ms, record, frame_time);
            self.frame_no += 1;
        }
        self.frame_start = Some(now);
        self.section = Some(("frame start", now));
        self.update_ms = 0.0;
    }

    fn mark(&mut self, section: &'static str) {
        let now = Instant::now();
        self.close_section(now);
        self.section = Some((section, now));
    }

    fn close_section(&mut self, now: Instant) {
        if let Some((name, t)) = self.section.take() {
            let ms = (now - t).as_secs_f32() * 1000.0;
            match self.sections.iter_mut().find(|(k, _)| *k == name) {
                Some(s) => s.1 += ms,
                None => self.sections.push((name, ms)),
            }
        }
    }

    fn end_update(&mut self) {
        let now = Instant::now();
        self.close_section(now);
        if let Some(start) = self.frame_start {
            self.update_ms = (now - start).as_secs_f32() * 1000.0;
        }
    }

    /// The frame before: a hitch? Then write it.
    fn judge(&mut self, wall_ms: f32, record: FrameRecord, frame_time: f32) {
        let usual = self.usual_ms.unwrap_or(wall_ms);
        let m = &mut self.minute;
        m.since += wall_ms / 1000.0;
        m.frames += 1;
        m.total_ms += wall_ms as f64;
        m.worst_ms = m.worst_ms.max(wall_ms);
        let pause = wall_ms >= PAUSE_MS;
        // The game's update alone can give it away too (when drawing is
        // slow anyway, as on a PC without a real GPU).
        let usual_update = self.usual_update_ms.unwrap_or(record.update_ms);
        let slow_update =
            record.update_ms > self.floor_ms && record.update_ms > usual_update * RATIO;
        let late = !pause && wall_ms > self.floor_ms && wall_ms > usual * RATIO;
        let hitch = late || slow_update;
        if let Some(st) = self.stats.as_mut() {
            if !pause {
                st.add(wall_ms, &record, hitch);
            }
        }
        if !pause {
            self.window.add(wall_ms, &record);
            if self.window.wall_ms >= f64::from(WINDOW_SECONDS) * 1000.0 {
                self.close_window();
            }
        }
        let m = &mut self.minute;
        if hitch {
            m.hitches += 1;
        } else if pause {
            m.pauses += 1;
        }
        // Only even frames teach the usual times.
        if !late && !pause {
            self.usual_ms = Some(match self.usual_ms {
                None => wall_ms,
                Some(u) => u + (wall_ms - u) * 0.05,
            });
        }
        if !slow_update && !pause {
            self.usual_update_ms = Some(match self.usual_update_ms {
                None => record.update_ms,
                Some(u) => u + (record.update_ms - u) * 0.05,
            });
        }
        if hitch && m.reported < REPORTS_PER_MINUTE && !self.full {
            m.reported += 1;
            let text = self.report(wall_ms, usual, usual_update, &record, frame_time);
            self.write(&text);
        } else if pause && m.reported < REPORTS_PER_MINUTE && !self.full {
            // Usually the window was away, but at the start it is the
            // window standing still: one line says whose time it was.
            m.reported += 1;
            let line = format!(
                "PAUSE {wall_ms:.0} ms at {} game time {frame_time:.1} s, frame {}: game update {:.1} ms, Engine after it {:.0} ms\n",
                clock_time(),
                self.frame_no,
                record.update_ms,
                (wall_ms - record.update_ms).max(0.0)
            );
            self.write(&line);
        }
        self.recent.push_back((wall_ms, record.update_ms));
        while self.recent.len() > 10 {
            self.recent.pop_front();
        }
        let every = if self.minute.hitches > 0 || self.minute.pauses > 0 {
            60.0
        } else {
            300.0
        };
        if self.minute.since >= every {
            let line = self.summary();
            self.write(&line);
            self.minute = Minute::default();
        }
    }

    /// A window of frames is full: a slow one is written as a whole (what
    /// every frame spent on average), the end of a slow stretch as a line.
    fn close_window(&mut self) {
        let w = std::mem::take(&mut self.window);
        let frames = f64::from(w.frames.max(1));
        let avg = w.wall_ms / frames;
        let fps = 1000.0 / avg.max(1e-3);
        if fps >= f64::from(SLOW_FPS) {
            if let Some((t, n)) = self.slow_since.take() {
                let line = format!(
                    "SLOW STRETCH OVER at {} after {:.0} s ({n} windows)\n\n",
                    clock_time(),
                    self.started.elapsed().as_secs_f32() - t
                );
                if !self.full {
                    self.write(&line);
                }
            }
            return;
        }
        let now = self.started.elapsed().as_secs_f32();
        let (since, n) = self
            .slow_since
            .map_or((now - WINDOW_SECONDS, 1), |(t, n)| (t, n + 1));
        self.slow_since = Some((since, n));
        if self.full {
            return;
        }
        let mut s = String::with_capacity(2048);
        let _ = writeln!(
            s,
            "SLOW STRETCH at {}: {fps:.1} fps over the last {:.1} s ({} frames, avg {avg:.0} ms, worst {:.0} ms), slow for {:.0} s so far",
            clock_time(),
            w.wall_ms / 1000.0,
            w.frames,
            w.worst_ms,
            now - since
        );
        let update = w.update_ms / frames;
        let _ = writeln!(
            s,
            "  per frame: game update {update:.1} ms, Engine after it {:.1} ms (animation, uploads, drawing, waiting for the GPU)",
            (avg - update).max(0.0)
        );
        let mut sections = w.sections.clone();
        sections.sort_by(|a, b| b.1.total_cmp(&a.1));
        let shown: Vec<String> = sections
            .iter()
            .map(|(k, ms)| (k, ms / frames))
            .filter(|(_, ms)| *ms >= 0.3)
            .take(10)
            .map(|(k, ms)| format!("{k} {ms:.1}"))
            .collect();
        if !shown.is_empty() {
            let _ = writeln!(s, "  sections (avg ms per frame): {}", shown.join(", "));
        }
        let mut counts = w.counts.clone();
        counts.sort_by_key(|a| std::cmp::Reverse(a.1));
        let c: Vec<String> = counts
            .iter()
            .map(|(k, n)| {
                if k.ends_with(" us") {
                    format!("{k} {:.1} ms/frame", *n as f64 / 1000.0 / frames)
                } else {
                    format!("{k} {:.1}/frame", *n as f64 / frames)
                }
            })
            .collect();
        if !c.is_empty() {
            let _ = writeln!(s, "  counts: {}", c.join(", "));
        }
        if !self.jobs.is_empty() {
            let j: Vec<String> = self
                .jobs
                .iter()
                .map(|(k, v)| format!("{k} ({v})"))
                .collect();
            let _ = writeln!(s, "  other threads: {}", j.join(", "));
        }
        for (k, v) in &self.context {
            let _ = writeln!(s, "  {k}: {v}");
        }
        if let Some(mem) = Memory::now() {
            let _ = writeln!(
                s,
                "  memory: {:.0} MB in use, {:.0} MB committed",
                mem.resident as f64 / 1048576.0,
                mem.committed as f64 / 1048576.0
            );
        }
        if let Some(c) = &self.census {
            let _ = writeln!(s, "  holds: {c}");
        }
        let events: Vec<&(f32, String)> = self
            .events
            .iter()
            .filter(|(t, _)| now - t <= WINDOW_SECONDS + EVENT_SECONDS)
            .collect();
        if !events.is_empty() {
            let _ = writeln!(s, "  just before:");
            for (t, e) in events {
                let _ = writeln!(s, "    {:.2} s ago: {e}", now - t);
            }
        }
        s.push('\n');
        self.write(&s);
    }

    fn report(
        &mut self,
        wall_ms: f32,
        usual_ms: f32,
        usual_update_ms: f32,
        r: &FrameRecord,
        frame_time: f32,
    ) -> String {
        let mut s = String::with_capacity(2048);
        let engine_ms = (wall_ms - r.update_ms).max(0.0);
        let _ = writeln!(
            s,
            "HITCH {wall_ms:.1} ms (usual {usual_ms:.1}) at {} game time {frame_time:.1} s, frame {}",
            clock_time(),
            self.frame_no
        );
        let _ = writeln!(
            s,
            "  game update {:.1} ms (usual {usual_update_ms:.1}), Engine after it {engine_ms:.1} ms (animation, uploads, drawing, waiting for the GPU)",
            r.update_ms
        );
        let mut sections = r.sections.clone();
        sections.sort_by(|a, b| b.1.total_cmp(&a.1));
        let shown: Vec<String> = sections
            .iter()
            .filter(|(_, ms)| *ms >= 0.5)
            .take(10)
            .map(|(k, ms)| format!("{k} {ms:.1}"))
            .collect();
        if !shown.is_empty() {
            let _ = writeln!(s, "  sections (ms): {}", shown.join(", "));
        }
        if !r.counts.is_empty() {
            let c: Vec<String> = r.counts.iter().map(|(k, n)| format!("{k} {n}")).collect();
            let _ = writeln!(s, "  this frame: {}", c.join(", "));
        }
        if !self.jobs.is_empty() {
            let j: Vec<String> = self
                .jobs
                .iter()
                .map(|(k, v)| format!("{k} ({v})"))
                .collect();
            let _ = writeln!(s, "  other threads: {}", j.join(", "));
        }
        for (k, v) in &self.context {
            let _ = writeln!(s, "  {k}: {v}");
        }
        let now = self.started.elapsed().as_secs_f32();
        if let Some(mem) = Memory::now() {
            let change = match self.last_memory {
                Some((t, before)) => format!(
                    ", {:+.0} MB and {} page faults in the {:.0} s since the last report",
                    (mem.resident as f64 - before.resident as f64) / 1048576.0,
                    mem.faults.saturating_sub(before.faults),
                    now - t
                ),
                None => String::new(),
            };
            let _ = writeln!(
                s,
                "  memory: {:.0} MB in use, {:.0} MB committed{change}",
                mem.resident as f64 / 1048576.0,
                mem.committed as f64 / 1048576.0
            );
            self.last_memory = Some((now, mem));
        }
        if !self.recent.is_empty() {
            let f: Vec<String> = self
                .recent
                .iter()
                .map(|(w, u)| format!("{w:.0}/{u:.0}"))
                .collect();
            let _ = writeln!(s, "  frames before (ms, whole/update): {}", f.join(" "));
        }
        let events: Vec<&(f32, String)> = self
            .events
            .iter()
            .filter(|(t, _)| now - t <= EVENT_SECONDS)
            .collect();
        if !events.is_empty() {
            let _ = writeln!(s, "  just before:");
            for (t, e) in events {
                let _ = writeln!(s, "    {:.2} s ago: {e}", now - t);
            }
        }
        s.push('\n');
        s
    }

    fn summary(&self) -> String {
        let m = &self.minute;
        let avg = m.total_ms / m.frames.max(1) as f64;
        let mut s = format!(
            "-- {}: {:.0} s, {} frames, {:.0} fps on average, worst {:.0} ms, {} hitches",
            clock_time(),
            m.since,
            m.frames,
            1000.0 / avg.max(1e-3),
            m.worst_ms,
            m.hitches
        );
        if m.hitches > m.reported {
            let _ = write!(s, " ({} not written out)", m.hitches - m.reported);
        }
        if m.pauses > 0 {
            let _ = write!(s, ", {} pauses", m.pauses);
        }
        if let Some(mem) = Memory::now() {
            let _ = write!(s, ", {:.0} MB in use", mem.resident as f64 / 1048576.0);
        }
        if let Some(c) = &self.census {
            let _ = write!(s, "\n   holds: {c}");
        }
        s.push_str("\n\n");
        s
    }

    fn write(&mut self, text: &str) {
        let big = std::fs::metadata(&self.path).map_or(0, |m| m.len()) > MAX_LOG_BYTES;
        if big {
            self.full = true;
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(&self.path) {
            let _ = f.write_all(text.as_bytes());
            if big && !text.starts_with("--") {
                let _ = f.write_all(b"(log full: only summaries from here on)\n\n");
            }
        }
    }
}

/// The local time of day, from the system clock (UTC: there is no time
/// zone without a dependency), with milliseconds.
pub(crate) fn clock_time() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs() % 86_400;
    format!(
        "{:02}:{:02}:{:02}.{:03} UTC",
        secs / 3600,
        secs / 60 % 60,
        secs % 60,
        d.subsec_millis()
    )
}

/// The game's memory, as the system counts it.
#[derive(Clone, Copy, Debug)]
pub struct Memory {
    /// Bytes in RAM (the working set).
    pub resident: u64,
    /// Bytes the system promised the game (RAM and page file).
    pub committed: u64,
    /// Page faults so far (a jump means the system was paging).
    pub faults: u64,
}

impl Memory {
    #[cfg(windows)]
    pub fn now() -> Option<Self> {
        #[repr(C)]
        struct Counters {
            cb: u32,
            page_fault_count: u32,
            peak_working_set_size: usize,
            working_set_size: usize,
            quota_peak_paged_pool_usage: usize,
            quota_paged_pool_usage: usize,
            quota_peak_non_paged_pool_usage: usize,
            quota_non_paged_pool_usage: usize,
            pagefile_usage: usize,
            peak_pagefile_usage: usize,
        }
        #[link(name = "kernel32")]
        extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn K32GetProcessMemoryInfo(
                process: *mut std::ffi::c_void,
                counters: *mut Counters,
                cb: u32,
            ) -> i32;
        }
        let cb = std::mem::size_of::<Counters>() as u32;
        let mut c = Counters {
            cb,
            page_fault_count: 0,
            peak_working_set_size: 0,
            working_set_size: 0,
            quota_peak_paged_pool_usage: 0,
            quota_paged_pool_usage: 0,
            quota_peak_non_paged_pool_usage: 0,
            quota_non_paged_pool_usage: 0,
            pagefile_usage: 0,
            peak_pagefile_usage: 0,
        };
        // SAFETY: the pseudo handle needs no closing, and `c` is a
        // PROCESS_MEMORY_COUNTERS of the size we pass.
        let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, cb) };
        (ok != 0).then_some(Self {
            resident: c.working_set_size as u64,
            committed: c.pagefile_usage as u64,
            faults: c.page_fault_count as u64,
        })
    }

    #[cfg(target_os = "linux")]
    pub fn now() -> Option<Self> {
        let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
        let mut it = statm.split_whitespace().map(|v| v.parse::<u64>().ok());
        let (size, resident) = (it.next()??, it.next()??);
        // Field 12 of /proc/self/stat (after the name in brackets): major faults.
        let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
        let after = stat.rsplit_once(')')?.1;
        let faults = after.split_whitespace().nth(9)?.parse().ok()?;
        Some(Self {
            resident: resident * 4096,
            committed: size * 4096,
            faults,
        })
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    pub fn now() -> Option<Self> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guard(dir: &std::path::Path) -> Guard {
        let path = dir.join("hitches.log");
        std::fs::write(&path, "").unwrap();
        Guard::new(path, 25.0)
    }

    fn frame(g: &mut Guard, wall: f32) {
        frame_with(g, wall, wall * 0.6);
    }

    fn frame_with(g: &mut Guard, wall: f32, update: f32) {
        let r = FrameRecord {
            sections: vec![("town", update * 0.8), ("people", 0.2)],
            counts: vec![("cells built", 2)],
            update_ms: update,
        };
        g.judge(wall, r, 1.0);
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("hw-hitch-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn a_late_frame_is_written_with_its_sections_and_context() {
        let dir = scratch("late");
        let mut g = guard(&dir);
        g.context.push(("hero", "at 10, 2, 30".into()));
        g.events.push_back((0.0, "area moved".into()));
        for _ in 0..60 {
            frame(&mut g, 16.7);
        }
        frame(&mut g, 80.0);
        let log = std::fs::read_to_string(&g.path).unwrap();
        assert!(log.contains("HITCH 80.0 ms"), "{log}");
        assert!(log.contains("town 38.4"), "{log}");
        assert!(log.contains("cells built 2"), "{log}");
        assert!(log.contains("hero: at 10, 2, 30"), "{log}");
        assert!(log.contains("area moved"), "{log}");
        assert_eq!(log.matches("HITCH").count(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn an_even_slow_frame_rate_is_not_a_hitch() {
        let dir = scratch("even");
        let mut g = guard(&dir);
        for _ in 0..200 {
            frame(&mut g, 34.0);
        }
        let log = std::fs::read_to_string(&g.path).unwrap();
        assert!(!log.contains("HITCH"), "{log}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn a_storm_of_hitches_is_counted_not_written_out() {
        let dir = scratch("storm");
        let mut g = guard(&dir);
        for i in 0..2000 {
            frame(&mut g, if i % 3 == 0 { 90.0 } else { 16.0 });
        }
        let log = std::fs::read_to_string(&g.path).unwrap();
        // About 80 s of frames: two minutes' worth of reports at most.
        assert!(log.matches("HITCH").count() <= 2 * REPORTS_PER_MINUTE as usize);
        assert!(log.contains("not written out"), "{log}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_long_slow_stretch_is_written_as_one_report_and_its_end() {
        let dir = scratch("stretch");
        let mut g = guard(&dir);
        for _ in 0..60 {
            frame(&mut g, 16.0);
        }
        // About 5 fps for ten seconds: every frame a hitch, but one report
        // a window, with what the frames spent on average.
        for _ in 0..50 {
            g.counts.push(("srv combat us", 120_000));
            frame(&mut g, 200.0);
        }
        for _ in 0..400 {
            frame(&mut g, 16.0);
        }
        let log = std::fs::read_to_string(&g.path).unwrap();
        assert!(log.contains("SLOW STRETCH at"), "{log}");
        assert!(log.contains("SLOW STRETCH OVER"), "{log}");
        assert!(log.contains("sections (avg ms per frame): town"), "{log}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn a_pause_is_not_a_hitch() {
        let dir = scratch("pause");
        let mut g = guard(&dir);
        for _ in 0..30 {
            frame(&mut g, 16.0);
        }
        // The window was hidden: the game itself was quick.
        frame_with(&mut g, 5000.0, 2.0);
        let log = std::fs::read_to_string(&g.path).unwrap();
        assert!(!log.contains("HITCH"), "{log}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn a_slow_update_is_a_hitch_even_when_drawing_is_slow() {
        let dir = scratch("update");
        let mut g = guard(&dir);
        for _ in 0..60 {
            frame_with(&mut g, 400.0, 6.0);
        }
        frame_with(&mut g, 430.0, 40.0);
        let log = std::fs::read_to_string(&g.path).unwrap();
        assert!(log.contains("game update 40.0 ms"), "{log}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn stats_gather_every_frame_while_on() {
        let dir = scratch("stats");
        let mut g = guard(&dir);
        frame(&mut g, 16.0);
        g.stats = Some(Stats::default());
        for _ in 0..60 {
            frame(&mut g, 16.0);
        }
        frame(&mut g, 80.0);
        let st = g.stats.take().unwrap();
        assert_eq!(st.wall_ms.len(), 61);
        assert_eq!(st.hitches, 1);
        let town = st.sections.iter().find(|s| s.0 == "town").unwrap();
        assert!((town.2 - 38.4).abs() < 0.01, "{town:?}");
        assert_eq!(st.counts, vec![("cells built", 122)]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn memory_can_be_read() {
        if cfg!(any(windows, target_os = "linux")) {
            let m = Memory::now().expect("memory");
            assert!(m.resident > 0 && m.committed >= m.resident / 2);
        }
    }
}
