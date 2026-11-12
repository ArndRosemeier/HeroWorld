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
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// A door counts as open (no longer solid) from this far open.
const OPEN: f64 = 0.85;

/// Town buildings' lifts are known within this of whoever is near (m)
/// beyond what was asked for, and listed again when someone has moved
/// this far.
const AHEAD: f64 = 40.0;
const STEP: f64 = 8.0;
/// The buildings whose lifts are kept.
const KEPT: usize = 96;

/// The lifts of the town buildings of halls near the players: worked out
/// building by building as someone comes near, never for a whole town.
#[derive(Debug, Default)]
struct Near {
    /// Where the list was made for, and how far round each place.
    at: Vec<DVec2>,
    r: f64,
    /// The lifts listed there.
    runs: Vec<Arc<LiftRun>>,
    /// Each building's lifts, by its site's corner.
    made: BTreeMap<(u64, u64), Arc<[Arc<LiftRun>]>>,
}

/// Every lift in town.
#[derive(Clone, Default)]
pub struct Lifts {
    /// The landmarks' lifts: a few, known from the start.
    pub runs: Vec<Arc<LiftRun>>,
    town: Option<Arc<TownWorld>>,
    near: Arc<Mutex<Near>>,
}

impl std::fmt::Debug for Lifts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Lifts({} of landmarks)", self.runs.len())
    }
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
    pub fn new(town: &Arc<TownWorld>) -> Self {
        Self {
            runs: town
                .landmarks
                .iter()
                .flat_map(hw_gen::urban::comet::lifts)
                .map(Arc::new)
                .collect(),
            town: Some(town.clone()),
            near: Default::default(),
        }
    }

    /// The lifts of the town buildings within `r` of any of `near`.
    ///
    /// On the caller's thread: a building's plan takes about a
    /// millisecond, once, as someone first comes within reach of it, and
    /// the lifts must be there in the very tick its walls are (a shaft
    /// without its doors is a hole to fall into).
    fn town_runs(&self, near: &[DVec3], r: f64) -> Vec<Arc<LiftRun>> {
        let Some(town) = self.town.as_ref() else {
            return Vec::new();
        };
        let at: Vec<DVec2> = near.iter().map(|p| DVec2::new(p.x, p.z)).collect();
        let mut n = self.near.lock().expect("lifts lock");
        let fresh = n.r >= r
            && n.at.len() == at.len()
            && n.at.iter().zip(&at).all(|(a, b)| a.distance(*b) < STEP);
        if fresh {
            return n.runs.clone();
        }
        let reach = r + AHEAD + STEP;
        let m = DVec2::splat(reach);
        if at.iter().any(|c| !town.quarters_ready(*c - m, *c + m)) {
            // Not planned yet: nothing stands there to ride in.
            return n.runs.clone();
        }
        if n.made.len() > KEPT {
            n.made.clear();
        }
        let mut runs = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for c in &at {
            for q in town.quarters_in(*c - m, *c + m) {
                for b in &q.buildings {
                    let s = &b.site;
                    if s.halls.is_none() {
                        continue;
                    }
                    let mid = s.origin + (s.along * s.size.x + s.inward * s.size.y) * 0.5;
                    let key = (s.origin.x.to_bits(), s.origin.y.to_bits());
                    if mid.distance(*c) > reach + s.size.length() * 0.5 || !seen.insert(key) {
                        continue;
                    }
                    let made = n.made.entry(key).or_insert_with(|| {
                        s.layout()
                            .iter()
                            .flat_map(hw_gen::layout::lifts::runs)
                            .map(|run| Arc::new(run.in_frame(s.origin, s.along, s.inward, b.base)))
                            .collect()
                    });
                    runs.extend(made.iter().cloned());
                }
            }
        }
        n.at = at;
        n.r = r;
        n.runs = runs.clone();
        runs
    }

    /// The lifts within `r` of any of `near` (in plan).
    pub fn runs_near(&self, near: &[DVec3], r: f64) -> Vec<Arc<LiftRun>> {
        self.runs
            .iter()
            .cloned()
            .chain(self.town_runs(near, r))
            .filter(|run| {
                let c = run.car.at();
                near.iter()
                    .any(|p| DVec2::new(p.x, p.z).distance(c) < r + run.car.reach())
            })
            .collect()
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
        self.runs_near(near, r)
            .iter()
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

    /// A body stands in `run`'s car through a whole round among the
    /// solids `o`: the car carries it to every stop, and the shut door
    /// keeps it out of the shaft while the car is away.
    fn rides(o: &mut Obstacles, run: &LiftRun) {
        let c = run.car.at();
        // Shut doors are solid: while the car is away from the
        // ground floor, nobody walks into the shaft there.
        let away = (0..4000)
            .map(|i| i as f64 * 0.1)
            .find(|t| run.at(*t).open.is_none())
            .expect("it rides");
        o.set_movers(Lifts::solids_of(run, away));
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
            o.set_movers(Lifts::solids_of(run, now));
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
    }

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
                rides(&mut o, &run);
                rode += 1;
            }
        }
        assert!(rode >= 4, "{rode} lifts ridden");
    }

    /// The lifts of a town's buildings of halls stand in their shafts:
    /// found from where a hero is, each carries a body past every floor
    /// of the building's own solids, and its open door lets one in.
    #[test]
    fn the_lifts_of_town_buildings_run_in_their_shafts() {
        use hw_gen::building::BuildingPlan;
        use hw_gen::urban::TownParams;
        let w = Arc::new(TownWorld::generate(
            // (Seed 1 at this size is all old lanes: no building of halls.)
            Seed(2),
            TownParams::sample(Seed(2), 20_000.0),
        ));
        let lifts = Lifts::new(&w);
        let mut rode = 0;
        let halls = w
            .everything()
            .buildings
            .into_iter()
            // (Office buildings: a shop house has only its stair.)
            .filter(|(_, b)| b.site.layout().is_some_and(|l| !l.lifts.is_empty()));
        for (_, b) in halls.take(6) {
            let s = &b.site;
            let plan = BuildingPlan::generate(s);
            let mut o = Obstacles::flat(b.base);
            o.add_all(crate::collision::building_solids(&plan, b.base));
            let mid = s.to_city(DVec3::new(s.size.x * 0.5, b.base, s.size.y * 0.5));
            let inside = |p: DVec2| {
                let d = p - s.origin;
                let (x, z) = (d.dot(s.along), d.dot(s.inward));
                (0.0..s.size.x).contains(&x) && (0.0..s.size.y).contains(&z)
            };
            // As in the game, the town round a hero is planned ahead.
            let (c, m) = (DVec2::new(mid.x, mid.z), DVec2::splat(80.0));
            w.prefetch(&[c], 80.0);
            for _ in 0..3000 {
                if w.quarters_ready(c - m, c + m) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            let here: Vec<_> = lifts
                .runs_near(&[mid], 45.0)
                .into_iter()
                .filter(|r| inside(r.car.at()))
                .collect();
            assert_eq!(
                here.len(),
                s.layout().expect("a plan").lifts.len(),
                "the lifts of the building at {:?}",
                s.origin
            );
            for run in here {
                // With the car waiting at the bottom the doorway is free.
                let open = (0..4000)
                    .map(|i| i as f64 * 0.1)
                    .find(|t| run.at(*t).open == Some((0, 1.0)))
                    .expect("it opens at the bottom");
                o.set_movers(Lifts::solids_of(&run, open));
                let door = run.doors.iter().find(|d| d.0 == 0).expect("a door").1;
                let (dy, _) = door.span();
                assert!(!o.solid_at(DVec3::new(door.at().x, dy + 1.0, door.at().y)));
                rides(&mut o, &run);
                rode += 1;
            }
        }
        assert!(rode >= 4, "{rode} lifts ridden");
    }
}
