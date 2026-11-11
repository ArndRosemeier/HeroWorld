//! Client-side smoothing of server state.
//!
//! - [`Predictor`]: your own hero moves the moment you press a key. The client
//!   runs the same movement code as the server on its own inputs; when a
//!   snapshot says which input the server has applied, the client starts
//!   from the server's state and replays the inputs still in flight. Small
//!   disagreements fade out visually instead of snapping.
//! - [`Interpolator`]: other heroes are drawn a little in the past, between
//!   two snapshots, so they glide instead of jumping 30 times a second.

use crate::collision::{Body, Obstacles};
use crate::{step_moving, Moves, DT};
use glam::DVec3;
use hw_core::NetId;
use hw_net::{EntityState, InputState};
use std::collections::VecDeque;
use std::sync::Arc;

/// How fast a visual correction fades (fraction left after one second).
const ERROR_LEFT_PER_SECOND: f64 = 0.0005;
/// Corrections larger than this snap instead of fading (teleports, spawns).
const SNAP_DISTANCE: f64 = 4.0;

pub struct Predictor {
    solids: Arc<Obstacles>,
    pending: VecDeque<(u32, InputState)>,
    /// The predicted state, one tick ahead per pending input.
    pub body: Body,
    /// Visual offset still being faded out.
    error: DVec3,
    started: bool,
    /// Our hero sits in a taxi: the server moves it, nothing is predicted.
    pub riding: bool,
    /// What our hero's movement powers allow (from the server's state).
    pub moves: Moves,
}

impl Predictor {
    pub fn new(solids: Arc<Obstacles>) -> Self {
        Self {
            solids,
            pending: VecDeque::new(),
            body: Body::default(),
            error: DVec3::ZERO,
            started: false,
            riding: false,
            moves: Moves::ON_FOOT,
        }
    }

    /// Forgets everything (a new host: sequence numbers start again).
    pub fn reset(&mut self) {
        *self = Self::new(self.solids.clone());
    }

    /// Call each tick after sending `input` with sequence number `seq`.
    pub fn input(&mut self, seq: u32, input: InputState) {
        if !self.started {
            return;
        }
        self.pending.push_back((seq, input));
        // Never keep more than two seconds in flight.
        while self.pending.len() > 60 {
            self.pending.pop_front();
        }
        if !self.riding {
            step_moving(&mut self.body, input, &self.solids, self.moves);
        }
    }

    /// Call when a snapshot arrives: `ack` is the last input the server
    /// applied, `server` our hero's state in that snapshot.
    pub fn reconcile(&mut self, ack: u32, server: &EntityState) {
        let server_body = Body {
            pos: DVec3::from_array(server.pos),
            vel: DVec3::new(
                server.vel[0] as f64,
                server.vel[1] as f64,
                server.vel[2] as f64,
            ),
            grounded: server.grounded,
        };
        if let Some(c) = server.combat {
            self.moves = Moves::from_view(&c);
        }
        if !self.started {
            self.body = server_body;
            self.started = true;
            return;
        }
        while self
            .pending
            .front()
            .is_some_and(|(s, _)| s.wrapping_sub(ack) as i32 <= 0)
        {
            self.pending.pop_front();
        }
        if self.riding {
            self.pending.clear();
            self.body = server_body;
            self.error = DVec3::ZERO;
            return;
        }
        let shown = self.body.pos + self.error;
        self.body = server_body;
        for (_, input) in &self.pending {
            step_moving(&mut self.body, *input, &self.solids, self.moves);
        }
        self.error = shown - self.body.pos;
        if self.error.length() > SNAP_DISTANCE {
            self.error = DVec3::ZERO;
        }
    }

    /// Where to draw our hero this frame.
    pub fn render_pos(&mut self, frame_dt: f64) -> DVec3 {
        self.error *= ERROR_LEFT_PER_SECOND.powf(frame_dt);
        self.body.pos + self.error
    }

    pub fn started(&self) -> bool {
        self.started
    }

    pub fn in_flight(&self) -> usize {
        self.pending.len()
    }
}

/// Snapshots kept to draw other entities slightly in the past.
pub struct Interpolator {
    buffer: VecDeque<(u64, Vec<EntityState>)>,
    /// The server tick being drawn (fractional).
    render_tick: f64,
    /// How far behind the newest snapshot to draw, in ticks.
    pub delay: f64,
}

impl Default for Interpolator {
    fn default() -> Self {
        Self::new()
    }
}

impl Interpolator {
    /// The server tick being drawn (fractional): the game clock others see.
    pub fn render_tick(&self) -> f64 {
        self.render_tick
    }

    pub fn new() -> Self {
        Self {
            buffer: VecDeque::new(),
            render_tick: 0.0,
            delay: 3.0,
        }
    }

    pub fn push(&mut self, tick: u64, entities: Vec<EntityState>) {
        if self.buffer.back().is_some_and(|(t, _)| tick <= *t) {
            return;
        }
        if self.buffer.is_empty() {
            self.render_tick = tick as f64 - self.delay;
        }
        self.buffer.push_back((tick, entities));
        while self.buffer.len() > 32 {
            self.buffer.pop_front();
        }
    }

    /// Advances the clock by a frame and returns every entity at the
    /// render time.
    pub fn sample(&mut self, frame_dt: f64) -> Vec<EntityState> {
        let Some(&(newest, _)) = self.buffer.back() else {
            return Vec::new();
        };
        self.render_tick += frame_dt / DT;
        // Drift gently towards the target delay; jump if far off.
        let target = newest as f64 - self.delay;
        let off = target - self.render_tick;
        if off.abs() > 10.0 {
            self.render_tick = target;
        } else {
            self.render_tick += off * (1.0 - 0.02f64.powf(frame_dt));
        }
        blend_at(&self.buffer, self.render_tick)
    }

    /// The newest known state of one entity.
    pub fn latest(&self, id: NetId) -> Option<EntityState> {
        self.buffer.back()?.1.iter().find(|e| e.id == id).copied()
    }
}

/// Farther than this in a tick is a jump (a respawn, a ride's start or a
/// teleport), drawn at once: the fastest hero flies 2 m a tick.
pub const SNAP_PER_TICK: f64 = 8.0;

/// Every entity of `buffer` (snapshots by server tick, oldest first) at
/// tick `t`, between the two snapshots round it: before the oldest or
/// after the newest, as that one has it. An entity only in the later
/// snapshot, or one that jumped, is drawn where that has it.
pub fn blend_at(buffer: &VecDeque<(u64, Vec<EntityState>)>, t: f64) -> Vec<EntityState> {
    let after = buffer.iter().position(|(tick, _)| *tick as f64 >= t);
    let (a, b) = match after {
        None => return buffer.back().map(|b| b.1.clone()).unwrap_or_default(),
        Some(0) => return buffer[0].1.clone(),
        Some(i) => (&buffer[i - 1], &buffer[i]),
    };
    let ticks = (b.0 - a.0) as f64;
    let f = (t - a.0 as f64) / ticks;
    b.1.iter()
        .map(|eb| match a.1.iter().find(|ea| ea.id == eb.id) {
            Some(ea) => {
                let pa = DVec3::from_array(ea.pos);
                let pb = DVec3::from_array(eb.pos);
                if pa.distance(pb) > SNAP_PER_TICK * ticks {
                    return *eb;
                }
                EntityState {
                    pos: pa.lerp(pb, f).to_array(),
                    rot: nlerp(ea.rot, eb.rot, f as f32),
                    ..*eb
                }
            }
            None => *eb,
        })
        .collect()
}

/// Blends two rotations (quaternions x, y, z, w) the short way round.
fn nlerp(a: [f32; 4], b: [f32; 4], f: f32) -> [f32; 4] {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let s = if dot < 0.0 { -1.0 } else { 1.0 };
    let q: Vec<f32> = a.iter().zip(b).map(|(x, y)| x + (y * s - x) * f).collect();
    let len = q.iter().map(|v| v * v).sum::<f32>().sqrt();
    if len < 1e-6 {
        return b;
    }
    [q[0] / len, q[1] / len, q[2] / len, q[3] / len]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Sim, GROUND_Y, TICK_HZ};
    use hw_core::Seed;
    use hw_net::PlayerId;

    /// A server and a predicting client, with `lag` ticks of latency each
    /// way. The prediction must match the server once inputs are applied.
    #[test]
    fn prediction_matches_the_server() {
        let solids = Arc::new(Obstacles::flat(GROUND_Y));
        let mut sim = Sim::with_solids(Seed(1), solids.clone());
        let me = PlayerId(1);
        sim.spawn_hero(me, DVec3::new(0.0, GROUND_Y, 0.0));
        let hero = sim.hero_of(me).unwrap();
        let mut pred = Predictor::new(solids);
        let lag = 4usize;
        let mut to_server: VecDeque<(u32, InputState)> = VecDeque::new();
        let mut to_client: VecDeque<(u32, EntityState)> = VecDeque::new();
        let mut applied = 0u32;
        for t in 0..(TICK_HZ * 4) {
            let input = InputState {
                move_x: if t < 60 { 1.0 } else { 0.0 },
                move_z: if t >= 40 { 1.0 } else { 0.0 },
                jump: t % 25 == 0,
                // Take off, climb, then drop back down.
                fly: (70..100).contains(&t),
                move_y: if (70..85).contains(&t) { 0.6 } else { 0.0 },
                boost: (80..90).contains(&t),
                ..Default::default()
            };
            let seq = t + 1;
            to_server.push_back((seq, input));
            pred.input(seq, input);
            if to_server.len() > lag {
                let (s, i) = to_server.pop_front().unwrap();
                sim.set_input(me, i);
                applied = s;
            }
            sim.step();
            let state = sim.snapshot().into_iter().find(|e| e.id == hero).unwrap();
            to_client.push_back((applied, state));
            if to_client.len() > lag {
                let (ack, st) = to_client.pop_front().unwrap();
                pred.reconcile(ack, &st);
            }
        }
        // Let everything arrive with no new input.
        while let Some((s, i)) = to_server.pop_front() {
            sim.set_input(me, i);
            applied = s;
            sim.step();
        }
        let state = sim.snapshot().into_iter().find(|e| e.id == hero).unwrap();
        pred.reconcile(applied, &state);
        assert_eq!(pred.in_flight(), 0);
        assert!(pred.body.pos.distance(DVec3::from_array(state.pos)) < 1e-6);
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn interpolation_glides_between_snapshots() {
        let mut interp = Interpolator::new();
        let at = |x: f64| EntityState {
            id: NetId(1),
            pos: [x, 0.0, 0.0],
            vel: [0.0; 3],
            grounded: true,
            owner: None,
            ..Default::default()
        };
        // Snapshots arrive once per tick and frames come twice per tick.
        let mut xs = Vec::new();
        for tick in 0..60u64 {
            interp.push(tick, vec![at(tick as f64)]);
            for _ in 0..2 {
                xs.push((tick, interp.sample(DT * 0.5)[0].pos[0]));
            }
        }
        // Settled: drawn about `delay` ticks behind the newest, moving smoothly.
        let (tick, x) = *xs.last().unwrap();
        assert!(
            (x - (tick as f64 - interp.delay)).abs() < 0.6,
            "x = {x} at tick {tick}"
        );
        for w in xs[40..].windows(2) {
            let step = w[1].1 - w[0].1;
            assert!(step > 0.2 && step < 0.8, "uneven step {step}");
        }
    }
}
