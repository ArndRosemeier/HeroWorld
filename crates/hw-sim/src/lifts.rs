//! The lifts of the game-scale interiors as moving solids: each car's
//! floor and roof where its timetable has it, and every landing door that
//! is not open. Made from the game clock alone
//! ([`hw_gen::layout::lifts`]), so the host and a joined player's own
//! prediction agree.
//!
//! A body rides by standing on the car: the floor rises less than a step
//! a tick, and going down the feet stay on it.

use crate::collision::{solids_of, Solid};
use glam::{DVec2, DVec3};
use hw_gen::layout::lifts::{LiftRun, CAR_FLOOR, CAR_LIP, CAR_ROOM};
use hw_gen::urban::footprint::Footprint;
use hw_gen::urban::world::TownWorld;

/// A door counts as open (no longer solid) from this far open.
const OPEN: f64 = 0.85;

/// Every lift in town.
#[derive(Clone, Debug, Default)]
pub struct Lifts {
    pub runs: Vec<LiftRun>,
}

fn lifted(f: Footprint, y0: f64, y1: f64) -> Footprint {
    match f {
        Footprint::Box { at, dir, half, .. } => Footprint::Box {
            at,
            dir,
            half,
            y0,
            y1,
        },
        Footprint::Disc { at, r, .. } => Footprint::Disc { at, r, y0, y1 },
    }
}

impl Lifts {
    pub fn new(town: &TownWorld) -> Self {
        Self {
            runs: town
                .landmarks
                .iter()
                .flat_map(hw_gen::urban::comet::lifts)
                .collect(),
        }
    }

    /// One lift's solids at `now`.
    pub fn solids_of(run: &LiftRun, now: f64) -> Vec<Solid> {
        let at = run.at(now);
        let mut out = Vec::new();
        out.extend(solids_of(&lifted(
            run.car,
            at.y - CAR_FLOOR,
            at.y + CAR_LIP,
        )));
        out.extend(solids_of(&lifted(
            run.car,
            at.y + CAR_ROOM,
            at.y + CAR_ROOM + 0.15,
        )));
        for (k, door) in &run.doors {
            let open = at.open.is_some_and(|(s, o)| s == *k && o >= OPEN);
            if !open {
                out.extend(solids_of(door));
            }
        }
        out
    }

    /// The solids of the lifts within `r` of any of `near` (in plan).
    pub fn solids_near(&self, near: &[DVec3], r: f64, now: f64) -> Vec<Solid> {
        self.runs
            .iter()
            .filter(|run| {
                let c = run.car.at();
                near.iter()
                    .any(|p| DVec2::new(p.x, p.z).distance(c) < r + run.car.reach())
            })
            .flat_map(|run| Self::solids_of(run, now))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::{Body, Obstacles};
    use hw_core::Seed;
    use hw_gen::layout::lifts::runs;
    use hw_gen::layout::office::plan;
    use hw_gen::layout::solid::{pieces, Part};

    /// A body steps into a waiting car, rides it to the top and back, and
    /// can step out at every floor; shut doors keep it out of the shaft.
    #[test]
    fn a_body_rides_a_lift_to_every_floor() {
        let mut rode = 0;
        for seed in 0..4 {
            let l = plan(Seed(seed), DVec2::new(56.0, 52.0), 5);
            let mut o = Obstacles::flat(0.0);
            o.add_all(
                pieces(&l)
                    .iter()
                    .filter(|p| p.part != Part::LiftDoor)
                    .flat_map(|p| solids_of(&p.shape))
                    .collect(),
            );
            for run in runs(&l) {
                let c = run.car.at();
                // Shut doors are solid: while the car is away from the
                // ground floor, nobody walks into the shaft there.
                let away = (0..4000)
                    .map(|i| i as f64 * 0.1)
                    .find(|t| run.at(*t).open.is_none())
                    .expect("it rides");
                o.set_movers(Lifts::solids_of(&run, away));
                let door = run.doors.iter().find(|d| d.0 == 0).expect("a door").1;
                let (dy, _) = door.span();
                assert!(o.solid_at(DVec3::new(door.at().x, dy + 1.0, door.at().y)));
                // Standing in the car from the start of a wait at the
                // bottom: it carries the body to every stop.
                let start = (0..4000)
                    .map(|i| i as f64 * 0.1)
                    .find(|t| run.at(*t).open == Some((0, 1.0)))
                    .expect("it opens at the bottom");
                let mut b = Body {
                    pos: DVec3::new(c.x, run.stops[0], c.y),
                    grounded: true,
                    ..Body::default()
                };
                let mut reached = vec![false; run.stops.len()];
                for i in 0..(400 * 30) {
                    let now = start + i as f64 / 30.0;
                    o.set_movers(Lifts::solids_of(&run, now));
                    let fall = if b.grounded {
                        0.0
                    } else {
                        b.vel.y - 9.8 / 30.0
                    };
                    b.vel = DVec3::new(0.0, fall, 0.0);
                    o.move_body(&mut b, 1.0 / 30.0);
                    let at = run.at(now);
                    assert!(
                        (b.pos.y - at.y).abs() < 0.5,
                        "{:?}: the body at {:.2} left the car at {:.2}",
                        run.kind,
                        b.pos.y,
                        at.y
                    );
                    if let Some((k, open)) = at.open {
                        if open >= 1.0 && (b.pos.y - run.stops[k] - CAR_LIP).abs() < 0.02 {
                            reached[k] = true;
                        }
                    }
                }
                assert!(reached.iter().all(|r| *r), "{:?}: {reached:?}", run.kind);
                rode += 1;
            }
        }
        assert!(rode >= 4, "{rode} lifts ridden");
    }
}
