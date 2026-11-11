//! Lifts that run: each car keeps a timetable, like a paternoster with
//! stops. It waits at a floor with its doors open, shuts them, rides to
//! the next floor, up to the top and down again. Where a car is comes
//! from the game clock alone, so the server and every player agree
//! without a word on the wire.

use super::solid::{LIFT_DOOR_H, SHAFT_WALL};
use super::{Layout, LiftKind};
use crate::geom::Rect;
use crate::urban::footprint::Footprint;
use glam::DVec2;

/// How long a car waits at a floor, how long its doors take to open or
/// shut (within the wait), and how fast it rides (m/s).
pub const WAIT: f64 = 7.0;
pub const DOORS: f64 = 0.9;
pub const SPEED: f64 = 3.0;
/// The car's floor and roof (m thick), and its headroom.
pub const CAR_FLOOR: f64 = 0.25;
pub const CAR_ROOM: f64 = 2.9;
/// The car's floor stands this much over the landing (the ground floor's
/// finish is that thick: the car must not sink under it).
pub const CAR_LIP: f64 = 0.05;

/// One lift: its car's plan, the floors it stops at and its doors there.
#[derive(Clone, Debug, PartialEq)]
pub struct LiftRun {
    pub kind: LiftKind,
    /// The car's floor in plan: a box, heights unused.
    pub car: Footprint,
    /// Floor heights, lowest first.
    pub stops: Vec<f64>,
    /// The landing doors: (which stop, the doorway as a thin box at its
    /// height).
    pub doors: Vec<(usize, Footprint)>,
    /// Seconds its timetable is shifted by.
    pub phase: f64,
}

/// Where a car is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiftAt {
    /// The car's floor.
    pub y: f64,
    /// The stop it waits at, and how far its doors are open there (0 to 1).
    pub open: Option<(usize, f64)>,
}

impl LiftRun {
    /// The stops in the order of one round: up to the top and back down.
    fn round(&self) -> Vec<usize> {
        let n = self.stops.len();
        let mut v: Vec<usize> = (0..n).collect();
        v.extend((1..n.saturating_sub(1)).rev());
        v
    }

    /// Where the car is `now` seconds into the game.
    pub fn at(&self, now: f64) -> LiftAt {
        let round = self.round();
        if round.len() < 2 {
            return LiftAt {
                y: self.stops.first().copied().unwrap_or(0.0),
                open: Some((0, 1.0)),
            };
        }
        let ride = |a: usize, b: usize| (self.stops[a] - self.stops[b]).abs() / SPEED;
        let legs: Vec<(usize, usize, f64)> = (0..round.len())
            .map(|i| {
                let (a, b) = (round[i], round[(i + 1) % round.len()]);
                (a, b, ride(a, b))
            })
            .collect();
        let total: f64 = legs.iter().map(|l| WAIT + l.2).sum();
        let mut t = (now + self.phase).rem_euclid(total);
        for (a, b, ride) in legs {
            if t < WAIT {
                let open = (t / DOORS).min((WAIT - t) / DOORS).clamp(0.0, 1.0);
                return LiftAt {
                    y: self.stops[a],
                    open: Some((a, open)),
                };
            }
            t -= WAIT;
            if t < ride {
                // Ease out of one floor and into the next.
                let u = t / ride;
                let s = u * u * (3.0 - 2.0 * u);
                return LiftAt {
                    y: self.stops[a] + (self.stops[b] - self.stops[a]) * s,
                    open: None,
                };
            }
            t -= ride;
        }
        LiftAt {
            y: self.stops[round[0]],
            open: Some((round[0], 0.0)),
        }
    }

    /// The same lift standing somewhere: its plan moved into the frame of
    /// a thing at `pos` facing `dir`, its heights lifted by `y`.
    pub fn placed(&self, pos: DVec2, dir: DVec2, y: f64) -> LiftRun {
        LiftRun {
            kind: self.kind,
            car: self.car.placed(pos, dir, y),
            stops: self.stops.iter().map(|s| s + y).collect(),
            doors: self
                .doors
                .iter()
                .map(|(k, d)| (*k, d.placed(pos, dir, y)))
                .collect(),
            phase: self.phase,
        }
    }

    /// The same lift in a frame given by its two axes (a building site's
    /// `origin`, `along` and `inward`, which may be a mirrored frame), its
    /// heights lifted by `y`.
    pub fn in_frame(&self, origin: DVec2, along: DVec2, inward: DVec2, y: f64) -> LiftRun {
        let to = |f: &Footprint| match *f {
            Footprint::Disc { at, r, y0, y1 } => Footprint::Disc {
                at: origin + along * at.x + inward * at.y,
                r,
                y0: y + y0,
                y1: y + y1,
            },
            Footprint::Box {
                at,
                dir,
                half,
                y0,
                y1,
            } => Footprint::Box {
                at: origin + along * at.x + inward * at.y,
                dir: (along * dir.x + inward * dir.y).normalize_or(inward),
                half,
                y0: y + y0,
                y1: y + y1,
            },
        };
        LiftRun {
            kind: self.kind,
            car: to(&self.car),
            stops: self.stops.iter().map(|s| s + y).collect(),
            doors: self.doors.iter().map(|(k, d)| (*k, to(d))).collect(),
            phase: self.phase,
        }
    }
}

fn thin(r: Rect, y0: f64, y1: f64) -> Footprint {
    Footprint::Box {
        at: r.center(),
        dir: DVec2::Y,
        half: r.size() * 0.5,
        y0,
        y1,
    }
}

/// Every lift of the layout, in its own frame.
pub fn runs(l: &Layout) -> Vec<LiftRun> {
    let mut out = Vec::new();
    for (n, lift) in l.lifts.iter().enumerate() {
        let s = &l.spaces[lift.shaft];
        let r = &s.rect;
        let mut doors = Vec::new();
        for k in l
            .links
            .iter()
            .filter(|k| k.a == lift.shaft || k.b == Some(lift.shaft))
        {
            let Some(stop) = lift.stops.iter().position(|y| (y - k.y).abs() < 1e-3) else {
                continue;
            };
            let (a, b) = k.ends();
            // In the middle of the shaft wall it opens through.
            let t = SHAFT_WALL * 0.5;
            let rect = if k.along_x {
                let z = if (k.at.y - r.min.y).abs() < 1e-3 {
                    r.min.y + t
                } else {
                    r.max.y - t
                };
                Rect::from_xz(a.x.min(b.x), z - 0.04, a.x.max(b.x), z + 0.04)
            } else {
                let x = if (k.at.x - r.min.x).abs() < 1e-3 {
                    r.min.x + t
                } else {
                    r.max.x - t
                };
                Rect::from_xz(x - 0.04, a.y.min(b.y), x + 0.04, a.y.max(b.y))
            };
            doors.push((stop, thin(rect, k.y, k.y + LIFT_DOOR_H)));
        }
        out.push(LiftRun {
            kind: lift.kind,
            car: thin(r.inset(SHAFT_WALL), 0.0, 0.0),
            stops: lift.stops.clone(),
            doors,
            // Neighbouring cars leave at different times.
            phase: n as f64 * 11.0,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::office::plan;
    use hw_core::Seed;

    #[test]
    fn cars_stop_at_every_floor_with_a_door_and_never_jump() {
        let mut seen = 0;
        for seed in 0..8 {
            let l = plan(Seed(seed), DVec2::new(56.0, 52.0), 5);
            for run in runs(&l) {
                assert!(run.stops.len() >= 2);
                for k in 0..run.stops.len() {
                    assert!(
                        run.doors.iter().any(|d| d.0 == k),
                        "{:?}: no door at stop {k}",
                        run.kind
                    );
                }
                let mut opened = vec![false; run.stops.len()];
                let mut last = run.at(0.0).y;
                for i in 0..20_000 {
                    let a = run.at(i as f64 / 30.0);
                    assert!((a.y - last).abs() < 0.2, "the car jumps");
                    last = a.y;
                    if let Some((k, open)) = a.open {
                        assert!((a.y - run.stops[k]).abs() < 1e-9);
                        opened[k] |= open > 0.99;
                    }
                }
                assert!(opened.iter().all(|o| *o), "a floor it never opens at");
                seen += 1;
            }
        }
        assert!(seen > 8, "{seen} lifts");
    }
}
