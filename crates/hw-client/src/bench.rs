//! `HW_BENCH=street|flight`: a benchmark run. The hero goes the same way
//! every time (along the town's main roads from the start: running on the
//! pavement, or flying above the roofs), the game measures every frame,
//! then writes a report to `logs/bench-<route>.txt` (and adds it to
//! `logs/bench.log`, so runs can be compared) and closes.
//!
//! The report gives frame times (median, 95th and 99th percentile, worst),
//! how much of each frame was the game's update and how much Engine's
//! drawing after it, the update's sections from the hitch guard (see
//! `hitch.rs`) by average and worst, what the frames counted, and memory.
//!
//! `HW_BENCH_SECS` sets how long the measured run is (default 60 s); the
//! first `HW_BENCH_WARMUP` seconds (default 20) the hero stands at the
//! start while the town around it loads, and are not measured.
//!
//! `HW_BENCH_LAPS=N` makes it a long run: the hero goes the route
//! (`HW_BENCH_SECS` long) and back N times, over the same ground each lap.
//! The report then has a line per lap with the frame time, memory and
//! Engine's resource census, and names every count that kept growing from
//! the second lap to the last: on the same ground, nothing should (a frame
//! rate that falls over a session is something piling up). Hold the hour
//! (`HW_HOUR`) so night does not change the picture.
//! `scripts/bench.bat` (or `bench.sh`) runs both routes, once plain and once
//! with Engine's GPU and CPU timings (`ENGINE_GPU_PROFILE_EVERY`,
//! `ENGINE_CPU_TRACE`), which go to the console.

use glam::DVec2;
use hw_gen::urban::world::TownWorld;
use std::fmt::Write as _;

/// Which way the benchmark hero goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// Running along the pavement of the main roads.
    Street,
    /// Flying along the main roads, above the roofs.
    Flight,
}

impl Route {
    pub fn name(self) -> &'static str {
        match self {
            Route::Street => "street",
            Route::Flight => "flight",
        }
    }

    /// Metres a second.
    pub fn speed(self) -> f64 {
        match self {
            Route::Street => hw_sim::RUN_SPEED,
            Route::Flight => 40.0,
        }
    }

    /// Metres above the ground.
    pub fn height(self) -> f64 {
        match self {
            Route::Street => 0.3,
            Route::Flight => 70.0,
        }
    }
}

/// Where the hero is this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    /// Still warming up: stay at the start.
    Wait,
    /// Here, heading `dir` (unit, on the ground plane), `height` above
    /// the ground.
    Go { at: DVec2, dir: DVec2, height: f64 },
    /// Done: write the report and close.
    Done,
}

pub struct Bench {
    pub route: Route,
    path: Vec<DVec2>,
    /// Distance along `path` at each point.
    along: Vec<f64>,
    warmup: f32,
    secs: f32,
    /// Game time the measured run started.
    started: Option<f32>,
    /// Memory at the start of the run, the most during it, and the times it
    /// was read.
    memory_start: Option<u64>,
    memory_peak: u64,
    memory_read_at: f32,
    /// Out-and-back laps (`HW_BENCH_LAPS`), 0 for one way.
    laps: u32,
    /// Measured frames so far, and each finished lap.
    frames: usize,
    lap_records: Vec<Lap>,
}

/// One finished lap of a long run.
#[derive(Clone, Debug)]
pub struct Lap {
    /// Frames measured before the lap ended.
    pub end_frame: usize,
    pub memory_mb: f64,
    pub census: engine::census::ResourceCensus,
}

impl Bench {
    /// The benchmark `HW_BENCH` asks for, if any.
    pub fn from_env(world: &TownWorld, spawn: DVec2) -> Option<Self> {
        let route = match std::env::var("HW_BENCH").ok()?.trim() {
            "street" => Route::Street,
            "flight" => Route::Flight,
            other => {
                eprintln!("HW_BENCH={other}: not a route (street or flight)");
                return None;
            }
        };
        let num = |name: &str, default: f32| {
            std::env::var(name)
                .ok()
                .and_then(|s| s.trim().parse::<f32>().ok())
                .filter(|v| v.is_finite() && *v >= 0.0)
                .unwrap_or(default)
        };
        let secs = num("HW_BENCH_SECS", 60.0).max(1.0);
        let laps = num("HW_BENCH_LAPS", 0.0).round() as u32;
        let warmup = num("HW_BENCH_WARMUP", 20.0);
        let length = route.speed() * f64::from(secs) + 50.0;
        let roads: Vec<&[DVec2]> = world
            .main_roads
            .iter()
            .map(|r| r.pts.as_slice())
            .filter(|p| p.len() >= 2)
            .collect();
        let mut path = route_along(&roads, spawn, length);
        if route == Route::Street {
            // On a pavement, not in the traffic.
            let side = world
                .main_roads
                .iter()
                .map(|r| r.half + r.pavement * 0.5)
                .next()
                .unwrap_or(8.0);
            path = offset(&path, side);
        }
        if path.len() < 2 {
            eprintln!("HW_BENCH: no main road to follow");
            return None;
        }
        let mut along = vec![0.0];
        for w in path.windows(2) {
            along.push(along.last().unwrap() + w[0].distance(w[1]));
        }
        Some(Self {
            route,
            path,
            along,
            warmup,
            secs,
            started: None,
            memory_start: None,
            memory_peak: 0,
            memory_read_at: f32::NEG_INFINITY,
            laps,
            frames: 0,
            lap_records: Vec::new(),
        })
    }

    /// The route's length in metres.
    pub fn length(&self) -> f64 {
        *self.along.last().unwrap_or(&0.0)
    }

    /// Where the hero is at game time `time` (seconds since the window
    /// opened). Starts and ends the hitch guard's gathering.
    pub fn step(&mut self, time: f32, world: &engine::prelude::World) -> Step {
        if time < self.warmup {
            return Step::Wait;
        }
        let start = *self.started.get_or_insert_with(|| {
            crate::hitch::start_stats();
            time
        });
        let t = time - start;
        if time - self.memory_read_at >= 1.0 {
            self.memory_read_at = time;
            if let Some(m) = crate::hitch::Memory::now() {
                self.memory_start.get_or_insert(m.resident);
                self.memory_peak = self.memory_peak.max(m.resident);
            }
        }
        self.frames += 1;
        if self.laps > 0 {
            return self.lap_step(t, world);
        }
        if t >= self.secs {
            return Step::Done;
        }
        let (at, dir) = self.at(self.route.speed() * f64::from(t));
        Step::Go {
            at,
            dir,
            height: self.route.height(),
        }
    }

    /// A long run: out along the route and back, `laps` times.
    fn lap_step(&mut self, t: f32, world: &engine::prelude::World) -> Step {
        let leg = (self.route.speed() * f64::from(self.secs)).min(self.length());
        let d = self.route.speed() * f64::from(t);
        let lap = (d / (2.0 * leg)).floor() as usize;
        if lap > self.lap_records.len() {
            self.lap_records.push(Lap {
                end_frame: self.frames,
                memory_mb: crate::hitch::Memory::now()
                    .map_or(0.0, |m| m.resident as f64 / 1048576.0),
                census: world.resource_census(),
            });
        }
        if lap >= self.laps as usize {
            return Step::Done;
        }
        let pos = d - lap as f64 * 2.0 * leg;
        let (at, dir) = if pos <= leg {
            self.at(pos)
        } else {
            let (at, dir) = self.at(2.0 * leg - pos);
            (at, -dir)
        };
        Step::Go {
            at,
            dir,
            height: self.route.height(),
        }
    }

    /// The point `d` metres along the route, and the way ahead there.
    fn at(&self, d: f64) -> (DVec2, DVec2) {
        let d = d.clamp(0.0, self.length());
        let i = self
            .along
            .partition_point(|&a| a <= d)
            .clamp(1, self.path.len() - 1);
        let (a, b) = (self.path[i - 1], self.path[i]);
        let seg = self.along[i] - self.along[i - 1];
        let k = if seg > 1e-9 {
            (d - self.along[i - 1]) / seg
        } else {
            0.0
        };
        // Turn smoothly: look at a point a little further on.
        let ahead = self.point(d + 12.0);
        let dir = (ahead - a.lerp(b, k)).normalize_or((b - a).normalize_or(DVec2::Y));
        (a.lerp(b, k), dir)
    }

    fn point(&self, d: f64) -> DVec2 {
        let d = d.clamp(0.0, self.length());
        let i = self
            .along
            .partition_point(|&a| a <= d)
            .clamp(1, self.path.len() - 1);
        let (a, b) = (self.path[i - 1], self.path[i]);
        let seg = self.along[i] - self.along[i - 1];
        if seg > 1e-9 {
            a.lerp(b, (d - self.along[i - 1]) / seg)
        } else {
            a
        }
    }

    /// The report, from the hitch guard's frames and facts about the run.
    pub fn report(&self, stats: &crate::hitch::Stats, facts: &[(&str, String)]) -> String {
        let mut s = String::with_capacity(4096);
        let _ = writeln!(
            s,
            "HeroWorld {} benchmark \"{}\", {}",
            hw_core::version(),
            self.route.name(),
            crate::hitch::clock_time()
        );
        let _ = writeln!(
            s,
            "  machine: {} {}, {} threads",
            std::env::consts::OS,
            std::env::consts::ARCH,
            std::thread::available_parallelism().map_or(0, |n| n.get())
        );
        for (k, v) in facts {
            let _ = writeln!(s, "  {k}: {v}");
        }
        let _ = writeln!(
            s,
            "  route: {:.0} s, {:.0} m along the main roads at {:.0} m/s, {:.1} m up",
            self.secs,
            (self.route.speed() * f64::from(self.secs)).min(self.length()),
            self.route.speed(),
            self.route.height()
        );
        let wall = Summary::of(&stats.wall_ms);
        let update = Summary::of(&stats.update_ms);
        let engine: Vec<f32> = stats
            .wall_ms
            .iter()
            .zip(&stats.update_ms)
            .map(|(w, u)| (w - u).max(0.0))
            .collect();
        let engine = Summary::of(&engine);
        let n = stats.wall_ms.len();
        let _ = writeln!(
            s,
            "  frames: {n}, {:.1} fps on average, {} hitches",
            1000.0 / wall.mean.max(1e-3),
            stats.hitches
        );
        let _ = writeln!(s, "  frame (ms):        {}", wall.line());
        let _ = writeln!(s, "  game update (ms):  {}", update.line());
        let _ = writeln!(
            s,
            "  Engine after (ms): {}  (animation, uploads, drawing, waiting for the GPU)",
            engine.line()
        );
        let over = |ms: f32| stats.wall_ms.iter().filter(|&&w| w > ms).count();
        let _ = writeln!(
            s,
            "  frames over 16.7 ms: {}, over 33 ms: {}, over 50 ms: {}",
            over(16.7),
            over(33.3),
            over(50.0)
        );
        if let Some(m0) = self.memory_start {
            let now = crate::hitch::Memory::now().map_or(0, |m| m.resident);
            let mb = |b: u64| b as f64 / 1048576.0;
            let _ = writeln!(
                s,
                "  memory: {:.0} MB at the start, {:.0} MB at the end, {:.0} MB at most",
                mb(m0),
                mb(now),
                mb(self.memory_peak.max(now))
            );
        }
        let mut sections = stats.sections.clone();
        sections.sort_by(|a, b| b.1.total_cmp(&a.1));
        let _ = writeln!(
            s,
            "  game update by section (ms per frame on average / worst frame):"
        );
        for (name, total, worst) in sections.iter().filter(|x| x.1 / n.max(1) as f64 >= 0.01) {
            let _ = writeln!(
                s,
                "    {name:<18} {:6.2} / {worst:6.1}",
                total / n.max(1) as f64
            );
        }
        if !self.lap_records.is_empty() {
            s.push_str(&self.laps_report(stats));
        }
        if !stats.counts.is_empty() {
            let mut counts = stats.counts.clone();
            counts.sort_by_key(|c| std::cmp::Reverse(c.1));
            let per_s = |c: u64| c as f64 / f64::from(self.secs);
            let c: Vec<String> = counts
                .iter()
                .map(|(k, c)| format!("{k} {c} ({:.1}/s)", per_s(*c)))
                .collect();
            let _ = writeln!(s, "  counted: {}", c.join(", "));
        }
        s.push('\n');
        s
    }

    /// A long run's lines: each lap, then what grew from the second lap to
    /// the last (the first lap includes the town loading around the start).
    fn laps_report(&self, stats: &crate::hitch::Stats) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "  laps (out and back over the same ground):");
        let mut from = 0;
        let mut means = Vec::new();
        for (i, lap) in self.lap_records.iter().enumerate() {
            let end = lap.end_frame.min(stats.wall_ms.len());
            let w = &stats.wall_ms[from.min(end)..end];
            let mean = w.iter().map(|&x| f64::from(x)).sum::<f64>() / w.len().max(1) as f64;
            means.push(mean);
            let _ = writeln!(
                s,
                "    lap {}: {} frames, {:.1} ms a frame on average, {:.0} MB in use\n      holds: {}",
                i + 1,
                w.len(),
                mean,
                lap.memory_mb,
                lap.census
            );
            from = end;
        }
        let grew = lap_growth(&self.lap_records, &means);
        if self.lap_records.len() < 3 {
            let _ = writeln!(s, "  growth: needs three laps or more to judge");
        } else if grew.is_empty() {
            let _ = writeln!(s, "  growth: none (frame time and every count level off)");
        } else {
            let _ = writeln!(s, "  growth from lap 2 to the last: {}", grew.join(", "));
        }
        s
    }

    /// Writes the report to `logs/bench-<route>.txt` and adds it to
    /// `logs/bench.log`; returns the first path.
    pub fn write(&self, report: &str) -> Option<std::path::PathBuf> {
        use std::io::Write as _;
        let dir = crate::hitch::path()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| "logs".into());
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("bench-{}.txt", self.route.name()));
        std::fs::write(&path, report).ok()?;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("bench.log"))
        {
            let _ = f.write_all(report.as_bytes());
        }
        Some(path)
    }
}

/// Median, percentiles, worst and mean of some frame times.
struct Summary {
    mean: f32,
    median: f32,
    p95: f32,
    p99: f32,
    worst: f32,
}

impl Summary {
    fn of(v: &[f32]) -> Self {
        let mut v: Vec<f32> = v.to_vec();
        v.sort_by(f32::total_cmp);
        let pick = |q: f32| {
            if v.is_empty() {
                0.0
            } else {
                v[((v.len() - 1) as f32 * q).round() as usize]
            }
        };
        Self {
            mean: v.iter().sum::<f32>() / v.len().max(1) as f32,
            median: pick(0.5),
            p95: pick(0.95),
            p99: pick(0.99),
            worst: pick(1.0),
        }
    }

    fn line(&self) -> String {
        format!(
            "average {:5.1}, median {:5.1}, 95% {:5.1}, 99% {:5.1}, worst {:6.1}",
            self.mean, self.median, self.p95, self.p99, self.worst
        )
    }
}

/// A way along `roads` from the point of them nearest `from`, at least
/// `length` metres long where the roads allow: on along the nearest road
/// in its longer direction, then on to a road that meets its end (one not
/// taken yet), turning back where none does.
pub fn route_along(roads: &[&[DVec2]], from: DVec2, length: f64) -> Vec<DVec2> {
    let Some((first, idx)) = nearest_point(roads, from) else {
        return Vec::new();
    };
    let road = roads[first];
    let fwd: f64 = road[idx..].windows(2).map(|w| w[0].distance(w[1])).sum();
    let back: f64 = road[..=idx].windows(2).map(|w| w[0].distance(w[1])).sum();
    let mut path: Vec<DVec2> = if fwd >= back {
        road[idx..].to_vec()
    } else {
        road[..=idx].iter().rev().copied().collect()
    };
    let mut used = vec![false; roads.len()];
    used[first] = true;
    let len = |p: &[DVec2]| -> f64 { p.windows(2).map(|w| w[0].distance(w[1])).sum() };
    let mut guard = 0;
    while len(&path) < length && guard < 64 {
        guard += 1;
        let end = *path.last().unwrap();
        // A road starting or ending near here, not taken yet.
        let next = roads
            .iter()
            .enumerate()
            .filter(|(i, _)| !used[*i])
            .filter_map(|(i, r)| {
                let (a, b) = (r[0], r[r.len() - 1]);
                let (da, db) = (a.distance(end), b.distance(end));
                (da.min(db) < 40.0).then_some((i, da <= db, da.min(db)))
            })
            .min_by(|a, b| a.2.total_cmp(&b.2));
        match next {
            Some((i, from_start, _)) => {
                used[i] = true;
                let r = roads[i];
                if from_start {
                    path.extend(r.iter().skip(1));
                } else {
                    path.extend(r.iter().rev().skip(1));
                }
            }
            None => {
                // A dead end: back the way we came.
                let back: Vec<DVec2> = path.iter().rev().skip(1).copied().collect();
                path.extend(back);
            }
        }
    }
    path.dedup_by(|a, b| a.distance(*b) < 0.01);
    path
}

fn nearest_point(roads: &[&[DVec2]], p: DVec2) -> Option<(usize, usize)> {
    roads
        .iter()
        .enumerate()
        .flat_map(|(i, r)| {
            r.iter()
                .enumerate()
                .map(move |(j, q)| (i, j, q.distance(p)))
        })
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(i, j, _)| (i, j))
}

/// The path moved `d` metres sideways (onto a pavement).
fn offset(path: &[DVec2], d: f64) -> Vec<DVec2> {
    let n = path.len();
    (0..n)
        .map(|i| {
            let a = path[i.saturating_sub(1)];
            let b = path[(i + 1).min(n - 1)];
            let dir = (b - a).normalize_or(DVec2::Y);
            path[i] + DVec2::new(dir.y, -dir.x) * d
        })
        .collect()
}

/// What grew from the second lap to the last by more than a tenth (and
/// more than a little): `name a -> b`. `means` are the laps' frame times.
pub fn lap_growth(laps: &[Lap], means: &[f64]) -> Vec<String> {
    let mut out = Vec::new();
    if laps.len() < 3 {
        return out;
    }
    let (a, b) = (&laps[1], &laps[laps.len() - 1]);
    let grows = |x: f64, y: f64, slack: f64| y > x * 1.1 && y - x > slack;
    if let (Some(&x), Some(&y)) = (means.get(1), means.last()) {
        if grows(x, y, 1.0) {
            out.push(format!("frame time {x:.1} -> {y:.1} ms"));
        }
    }
    if grows(a.memory_mb, b.memory_mb, 200.0) {
        out.push(format!(
            "memory {:.0} -> {:.0} MB",
            a.memory_mb, b.memory_mb
        ));
    }
    for ((name, x), (_, y)) in a.census.fields().into_iter().zip(b.census.fields()) {
        if grows(x as f64, y as f64, 8.0) {
            out.push(format!("{name} {x} -> {y}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_route_goes_on_to_a_road_that_meets_the_end() {
        let a = [DVec2::new(0.0, 0.0), DVec2::new(100.0, 0.0)];
        let b = [DVec2::new(100.0, 0.0), DVec2::new(100.0, 300.0)];
        let roads: Vec<&[DVec2]> = vec![&a, &b];
        let p = route_along(&roads, DVec2::new(1.0, 1.0), 350.0);
        assert_eq!(p.first(), Some(&DVec2::new(0.0, 0.0)));
        assert!(p.contains(&DVec2::new(100.0, 300.0)), "{p:?}");
    }

    #[test]
    fn a_dead_end_turns_back() {
        let a = [DVec2::new(0.0, 0.0), DVec2::new(100.0, 0.0)];
        let roads: Vec<&[DVec2]> = vec![&a];
        let p = route_along(&roads, DVec2::ZERO, 250.0);
        let len: f64 = p.windows(2).map(|w| w[0].distance(w[1])).sum();
        assert!(len >= 250.0, "{len}");
    }

    #[test]
    fn the_offset_keeps_to_one_side_and_its_distance() {
        let p = [DVec2::new(0.0, 0.0), DVec2::new(0.0, 100.0)];
        let o = offset(&p, 5.0);
        assert!((o[0].distance(p[0]) - 5.0).abs() < 1e-9);
        // Both points on the same side.
        assert_eq!(o[0].x.signum(), o[1].x.signum());
    }

    #[test]
    fn percentiles_pick_from_the_sorted_frames() {
        let v: Vec<f32> = (1..=100).map(|i| i as f32).collect();
        let s = Summary::of(&v);
        assert_eq!(s.worst, 100.0);
        assert_eq!(s.median, 51.0);
        assert!((s.p95 - 95.0).abs() <= 1.0);
        assert!((s.mean - 50.5).abs() < 1e-3);
    }

    #[test]
    fn lap_growth_names_what_keeps_climbing() {
        let lap = |entities: usize, textures: usize, mb: f64| Lap {
            end_frame: 0,
            memory_mb: mb,
            census: engine::census::ResourceCensus {
                entities,
                textures,
                ..Default::default()
            },
        };
        // Flat (lap 1 is loading and ignored): nothing.
        let flat = [
            lap(100, 5, 1000.0),
            lap(8000, 90, 4000.0),
            lap(8010, 92, 4050.0),
        ];
        assert!(lap_growth(&flat, &[30.0, 16.0, 16.5]).is_empty());
        // Textures and memory and frame time keep climbing: all three named.
        let leak = [
            lap(100, 5, 1000.0),
            lap(8000, 90, 4000.0),
            lap(8000, 900, 5000.0),
        ];
        let grew = lap_growth(&leak, &[30.0, 16.0, 25.0]);
        assert_eq!(grew.len(), 3, "{grew:?}");
        assert!(grew.iter().any(|g| g.starts_with("textures 90 -> 900")));
    }
}
