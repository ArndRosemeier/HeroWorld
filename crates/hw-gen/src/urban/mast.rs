//! The airship mooring mast: a golden-age lattice tower on its own airfield
//! at the edge of town, a little terminal at its foot, and the liner
//! *Skylark* that sails in over the rooftops, noses onto the mast, unloads
//! and casts off again on its round.
//!
//! Like Lakehurst, the mast stands in open country, just past the last
//! houses, as near the town's core as open ground allows: so the moored
//! ship hangs over fields and never over a roof, in a hamlet or a
//! megacity alike. Its route is pure data too ([`MastRoute`]): where the
//! ship is at any second of the game clock, the same for every player.
//!
//! Frames: the mast's own frame has +z the way the moored ship lies (out,
//! away from the town), x to its right, y up from the plinth's top; see
//! [`super::landmarks::Landmark::to_plan`].

use super::airspace::{highest_under, SkyBox};
use super::footprint::{block, Footprint};
use super::landmarks::{Landmark, LandmarkKind};
use super::roadgrid::RoadGrid;
use super::world::{TownWorld, WATER_LEVEL};
use glam::{DVec2, DVec3};
use hw_core::det;
use std::f64::consts::{PI, TAU};

/// The plinth's radius: the apron round the mast.
pub const MAST_PLAZA: f64 = 22.0;
/// Height of the moored ship's axis (the mooring cup) above the plinth.
pub const MOOR_Y: f64 = 50.0;
/// The top of the beacon, the mast's full height.
pub const MAST_H: f64 = MOOR_Y + 6.5;
/// The lattice: half the square between its four legs at the foot and up
/// at the gallery.
pub const LEG_FOOT: f64 = 4.2;
pub const LEG_TOP: f64 = 1.5;
/// The legs' radius (with their bracing), for collision.
pub const LEG_R: f64 = 0.45;
/// The lift shaft up the middle: its half width.
pub const SHAFT: f64 = 0.9;
/// The gallery round the top, where passengers step across: its deck
/// height and radius.
pub const GALLERY_Y: f64 = MOOR_Y - 5.0;
pub const GALLERY_R: f64 = 3.6;
/// The mooring cup, where the ship's nose locks on: out along +z.
pub const CUP_Z: f64 = 2.3;
/// The terminal at the mast's foot, across from the ship: centre (local
/// z), half width and depth, and its walls' height (the canopy and
/// boomerang roof sit above).
pub const TERMINAL_Z: f64 = -14.5;
pub const TERMINAL_HALF: (f64, f64) = (8.0, 4.0);
pub const TERMINAL_WALL: f64 = 4.4;
/// Where crates come down under the ship's cargo hatch (local z on the
/// apron, clear of everything).
pub const CARGO_Z: f64 = CUP_Z + HATCH_BACK;

/// The liner: length and widest radius of its envelope (m).
pub const SHIP_LEN: f64 = 96.0;
pub const SHIP_R: f64 = 10.0;
/// Where the passenger deck runs under the belly (metres back from the
/// nose), and the cargo hatch in its floor.
pub const DECK_FROM: f64 = 11.0;
pub const DECK_TO: f64 = 40.0;
pub const HATCH_BACK: f64 = 14.5;
/// From the axis down to the passenger deck's floor (its keel).
pub const DECK_DROP: f64 = SHIP_R + 2.6;

/// The engine pods' middles in the ship's frame (origin at the nose's tip,
/// nose to +z; starboard pods, port mirrors them), and how far behind a
/// pod's middle its propeller turns (radius 1.6).
pub const NACELLES: [DVec3; 2] = [DVec3::new(8.8, -7.4, -34.0), DVec3::new(10.4, -3.4, -58.0)];
pub const PROP_BACK: f64 = 3.0;
/// The passenger deck's half width.
pub const DECK_HALF: f64 = 3.3;
/// The tail fins: where their roots start (back from the nose), and how
/// far the upper and side fins and the lower fin reach out from the hull.
pub const FIN_FROM: f64 = 70.0;
pub const FIN_SPAN: f64 = 8.0;
pub const FIN_SPAN_LOW: f64 = 6.0;

/// Radius of the envelope `s` metres back from the nose: an elliptic nose
/// to its widest at three tenths, then a long taper to a short tail cone.
pub fn hull_r(s: f64) -> f64 {
    let t = (s / SHIP_LEN).clamp(0.0, 1.0);
    if t < 0.3 {
        let n = t / 0.3;
        SHIP_R * (n * (2.0 - n)).max(0.0).sqrt()
    } else {
        let k = (1.0 - t) / 0.7;
        SHIP_R * det::powf((k * (2.0 - k)).max(0.0), 0.8).max(0.0) * 0.94 + SHIP_R * 0.06 * k
    }
}

/// The mast's solid parts in its own frame: the four legs (stepped up their
/// slant), the lift shaft, the gallery and the mooring head on top, and
/// the terminal across the apron.
pub fn mast_footprints() -> Vec<Footprint> {
    let mut v = Vec::new();
    const STEPS: usize = 8;
    for (sx, sz) in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
        for k in 0..STEPS {
            let (y0, y1) = (
                GALLERY_Y * k as f64 / STEPS as f64,
                GALLERY_Y * (k + 1) as f64 / STEPS as f64,
            );
            let h = leg_half((y0 + y1) * 0.5);
            v.push(Footprint::Disc {
                at: DVec2::new(sx * h, sz * h),
                r: LEG_R,
                y0,
                y1,
            });
        }
    }
    v.push(block(SHAFT, SHAFT, GALLERY_Y));
    v.push(Footprint::Disc {
        at: DVec2::ZERO,
        r: GALLERY_R,
        y0: GALLERY_Y - 0.6,
        y1: GALLERY_Y,
    });
    v.push(Footprint::Disc {
        at: DVec2::ZERO,
        r: 1.4,
        y0: GALLERY_Y,
        y1: MAST_H,
    });
    v.push(Footprint::Box {
        at: DVec2::new(0.0, TERMINAL_Z),
        dir: DVec2::Y,
        half: DVec2::new(TERMINAL_HALF.0, TERMINAL_HALF.1),
        y0: 0.0,
        y1: TERMINAL_WALL + 1.2,
    });
    // The canopy from the terminal to the mast's foot: its roof slab.
    v.push(Footprint::Box {
        at: DVec2::new(0.0, (TERMINAL_Z + TERMINAL_HALF.1 - LEG_FOOT) * 0.5),
        dir: DVec2::Y,
        half: DVec2::new(1.9, (TERMINAL_Z + TERMINAL_HALF.1 + LEG_FOOT).abs() * 0.5),
        y0: CANOPY_Y,
        y1: CANOPY_Y + 0.35,
    });
    for z in canopy_posts() {
        for x in [-1.6, 1.6] {
            v.push(Footprint::Disc {
                at: DVec2::new(x, z),
                r: 0.16,
                y0: 0.0,
                y1: CANOPY_Y,
            });
        }
    }
    // The tulip collar under the gallery, in two steps.
    for (r, y0, y1) in [(2.3, -6.0, -3.0), (3.3, -3.0, -0.6)] {
        v.push(Footprint::Disc {
            at: DVec2::ZERO,
            r,
            y0: GALLERY_Y + y0,
            y1: GALLERY_Y + y1,
        });
    }
    v.push(Footprint::Disc {
        at: DVec2::new(PYLON.0, PYLON.1 - 0.6),
        r: 0.9,
        y0: 0.0,
        y1: PYLON_H,
    });
    v.push(Footprint::Disc {
        at: DVec2::new(SOCK.0, SOCK.1),
        r: 0.12,
        y0: 0.0,
        y1: 6.2,
    });
    v
}

/// The terminal's pylon: its foot (local x, z) and height; and the
/// windsock's pole.
pub const PYLON: (f64, f64) = (-TERMINAL_HALF.0 - 1.6, TERMINAL_Z + 2.4);
pub const PYLON_H: f64 = 11.0;
pub const SOCK: (f64, f64) = (15.0, -8.0);

/// The walkway canopy's underside.
pub const CANOPY_Y: f64 = 3.2;

/// Where the walkway canopy's posts stand (local z), both sides.
pub fn canopy_posts() -> Vec<f64> {
    let (a, b) = (TERMINAL_Z + TERMINAL_HALF.1 + 0.6, -LEG_FOOT - 0.9);
    let n = 3;
    (0..n)
        .map(|k| a + (b - a) * k as f64 / (n - 1) as f64)
        .collect()
}

/// Half the square between the legs at height `y` above the plinth.
pub fn leg_half(y: f64) -> f64 {
    let t = (y / GALLERY_Y).clamp(0.0, 1.0);
    // A gentle curve in, as the Googie towers have: wide-footed, slim on top.
    LEG_FOOT + (LEG_TOP - LEG_FOOT) * (1.0 - (1.0 - t).powi(2))
}

/// The highest a town's buildings can reach over their ground, from its
/// size (the tallest towers `plots` builds, a floor to spare, and the
/// tallest crown [`crate::building::BuildingSite::top`] puts on them): what
/// the liner has to clear on its round, without planning the town under it.
pub fn skyline(population: f64) -> f64 {
    let big = (det::log10(population.max(1000.0)) - 3.5).clamp(0.0, 3.5);
    let floors = 14.0 * (0.4 + big);
    let towers = crate::building::HALL_HEIGHT + crate::building::FLOOR_HEIGHT * floors;
    towers + 46.0
}

/// From the liner's nose down to under its passenger deck, engine pods and
/// lower fin, with the tail as low as its steepest pitch puts it, and room
/// to spare (m): how far over a roof the nose stays.
pub const LINER_KEEL: f64 = DECK_DROP + SHIP_LEN * 0.08 + 6.0;
/// Half the liner's width across its engine pods and side fins, with room
/// to spare (m).
pub const LINER_HALF_WIDE: f64 = 12.0 + 6.0;
/// The steepest the round climbs or sinks to clear a roof (m per m).
const CLEAR_SLOPE: f64 = 0.09;

/// The sky the liner's hull fills with its nose at `nose` heading `h`
/// (from +z towards +x), with room to spare.
pub fn liner_sky(nose: DVec2, h: f64) -> SkyBox {
    let fwd = det::dir2(PI * 0.5 - h);
    SkyBox::new(
        nose - fwd * (SHIP_LEN * 0.5),
        fwd,
        SHIP_LEN * 0.5 + 6.0,
        LINER_HALF_WIDE,
    )
}

/// Picks the mast's place: on open ground just past the town's edge, as
/// near the core as there is room for the apron and for the moored ship to
/// lie out over the fields and glide in to it, clear of every road.
pub fn plan_mast(w: &TownWorld, others: &[Landmark]) -> Option<Landmark> {
    let core = w.core();
    let half = w.half();
    let mut best: Option<(f64, DVec2, DVec2)> = None;
    const DIRS: usize = 48;
    for k in 0..DIRS {
        let d = det::dir2(TAU * k as f64 / DIRS as f64);
        // The first open ground going out: no piece of town within the
        // apron's reach (a road between two pieces is not the edge).
        let open = |p: DVec2| {
            (0..12).all(|j| {
                let a = TAU * j as f64 / 12.0;
                !w.in_a_piece(p + det::dir2(a) * (MAST_PLAZA + 18.0))
            }) && !w.in_a_piece(p)
        };
        let mut r = 80.0;
        let mut edge = None;
        while r < half {
            if open(core + d * r) {
                edge = Some(r);
                break;
            }
            r += 25.0;
        }
        let Some(edge) = edge else { continue };
        for extra in [0.0, 40.0, 90.0, 160.0] {
            let p = core + d * (edge + extra);
            let score = -p.distance(core);
            if best.is_some_and(|b| score <= b.0) {
                break;
            }
            if !open(p) || !site_fits(w, p, d, others) {
                continue;
            }
            best = Some((score, p, d));
            break;
        }
    }
    let (_, p, d) = best?;
    let (lo, hi) = rim(w, p, MAST_PLAZA);
    Some(Landmark {
        kind: LandmarkKind::MooringMast,
        pos: p,
        plaza: MAST_PLAZA,
        base: hi + 0.3,
        low: lo,
        height: MAST_H,
        dir: d,
        stairs: Vec::new(),
    })
}

/// Lowest and highest ground over a disc.
fn rim(w: &TownWorld, p: DVec2, r: f64) -> (f64, f64) {
    let mut lo = w.ground(p);
    let mut hi = lo;
    for ring in [0.5, 1.0] {
        for k in 0..16 {
            let a = TAU * k as f64 / 16.0;
            let g = w.ground(p + det::dir2(a) * r * ring);
            lo = lo.min(g);
            hi = hi.max(g);
        }
    }
    (lo, hi)
}

/// How far out the liner starts its last glide in, along the mast's axis.
pub const APPROACH: f64 = 700.0;
/// How high above the cup it starts that glide.
pub const APPROACH_UP: f64 = 70.0;

/// Can the mast stand at `p` with its ship lying out along `d`?
fn site_fits(w: &TownWorld, p: DVec2, d: DVec2, others: &[Landmark]) -> bool {
    let half = w.half();
    let reach = SHIP_LEN + 30.0;
    if (p + d * reach).abs().max_element() > half - 5.0 || p.abs().max_element() > half - 40.0 {
        return false;
    }
    let (lo, hi) = rim(w, p, MAST_PLAZA);
    if lo < WATER_LEVEL + 1.2 || hi - lo > 3.0 {
        return false;
    }
    if others
        .iter()
        .any(|l| l.pos.distance(p) < l.plaza + MAST_PLAZA + 30.0)
    {
        return false;
    }
    let base = hi + 0.3;
    let side = d.perp();
    // Under the moored ship (as it swings a little): no town, and the
    // ground well below its keel.
    for i in 0..=8 {
        let s = SHIP_LEN * i as f64 / 8.0;
        for j in [-1.0, 0.0, 1.0] {
            let q = p + d * (CUP_Z + s) + side * (j * (SHIP_R + 8.0 + s * 0.08));
            if w.in_a_piece(q) || w.ground(q) > base + MOOR_Y - DECK_DROP - 12.0 {
                return false;
            }
        }
    }
    // Under the glide in: the ground well below it.
    for i in 0..=14 {
        let u = i as f64 / 14.0;
        let q = p + d * (CUP_Z + APPROACH * u);
        let y = base + MOOR_Y + APPROACH_UP * u * u;
        if w.ground(q) > y - DECK_DROP - 20.0 {
            return false;
        }
    }
    let lo2 = p - DVec2::splat(MAST_PLAZA + 40.0);
    let hi2 = p + DVec2::splat(MAST_PLAZA + 40.0);
    let roads = w.roads_in(lo2, hi2);
    let grid = RoadGrid::new(&roads, 0.0);
    !grid.touches(p, MAST_PLAZA + 9.0, None)
}

// ------------------------------------------------------------------ route

/// What the liner is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Moored at the mast: gangway out, crates coming down.
    Docked,
    /// Backing off the mast and turning away.
    CastOff,
    /// On its round over the town and away.
    Flight,
    /// Gliding in along the mast's axis, nose first.
    Approach,
}

/// Where the liner is and how it sits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShipPose {
    /// The tip of its nose, in the town's frame (x, height, plan y as z).
    pub nose: DVec3,
    /// Heading (radians, 0 = +z, turning towards +x) and nose-up pitch
    /// and bank (radians).
    pub yaw: f64,
    pub pitch: f64,
    pub roll: f64,
    /// How hard the engines work (0 idle .. 1 full ahead).
    pub thrust: f64,
    pub phase: Phase,
    /// Seconds into the phase.
    pub t: f64,
}

impl ShipPose {
    /// The way the nose points.
    pub fn forward(&self) -> DVec3 {
        let (s, c) = det::sin_cos(self.yaw);
        let (ps, pc) = det::sin_cos(self.pitch);
        DVec3::new(s * pc, ps, c * pc)
    }
}

/// Seconds moored, backing off, and gliding in.
pub const DOCKED_S: f64 = 170.0;
pub const CASTOFF_S: f64 = 45.0;
/// Cruising speed (m/s) and the speed it starts its round at.
pub const CRUISE: f64 = 22.0;
const START_SPEED: f64 = 2.0;
/// Seconds it takes to come up to cruising speed.
const SPEED_UP_S: f64 = 35.0;
/// How far it backs off the mast before turning away, and how much it rises.
const BACK_OFF: f64 = 35.0;
const BACK_UP: f64 = 12.0;

/// The liner's round: moored, cast off, out over the town's core and away
/// beyond it, round wide and back in to the mast. A pure function of the
/// game clock.
#[derive(Clone, Debug)]
pub struct MastRoute {
    /// The nose's tip when moored.
    pub cup: DVec3,
    /// The mast's axis (out, the way the moored ship lies).
    pub dir: DVec2,
    /// Which way it turns away (unit, across the axis).
    side: DVec2,
    /// The round as flown, every [`TRACK_DT`] seconds: where the nose is,
    /// the heading and how fast it is turning.
    track: Vec<(DVec3, f64, f64)>,
    /// Seconds of the round, and of the glide in.
    fly_s: f64,
    approach_s: f64,
    /// Where in its cycle the liner is at game second 0.
    offset: f64,
    /// Cruising height above the plan's zero.
    pub cruise: f64,
}

/// Seconds between the round's samples.
const TRACK_DT: f64 = 0.5;
/// The fastest the liner turns (rad/s): a wide, stately curve.
const TURN_RATE: f64 = 0.075;

impl MastRoute {
    pub fn new(w: &TownWorld, mast: &Landmark) -> Self {
        let dir = mast.dir;
        let cup2 = mast.to_plan(DVec2::new(0.0, CUP_Z));
        let cup = DVec3::new(cup2.x, mast.base + MOOR_Y, cup2.y);
        let core = w.core();
        let to_core = core - mast.pos;
        // Turn away on the side the core lies.
        let side = if to_core.dot(dir.perp()) >= 0.0 {
            dir.perp()
        } else {
            -dir.perp()
        };
        // Over every roof the town could have, every landmark and every
        // hill on the way.
        let over_roofs = (skyline(w.town.params.population) + LINER_KEEL).max(90.0);
        let over_landmarks = w
            .landmarks
            .iter()
            .filter(|l| l.kind != LandmarkKind::MooringMast)
            .map(|l| l.base + l.height + LINER_KEEL)
            .fold(f64::NEG_INFINITY, f64::max);
        let ground_under = |sky: &SkyBox| {
            sky.corners()
                .into_iter()
                .chain([sky.mid])
                .map(|p| w.ground(p))
                .fold(f64::NEG_INFINITY, f64::max)
        };
        let mut cruise = (w.ground(core) + over_roofs)
            .max(over_landmarks)
            .max(cup.y + APPROACH_UP + 20.0);
        let mut track = Vec::new();
        for _ in 0..3 {
            track = Self::fly(w, mast, cup, dir, side, cruise);
            let top = track
                .iter()
                .map(|&(p, h, _)| ground_under(&liner_sky(DVec2::new(p.x, p.z), h)) + over_roofs)
                .fold(f64::NEG_INFINITY, f64::max);
            if top <= cruise {
                break;
            }
            cruise = top;
        }
        // Climbing away from the mast and sinking back to it, the liner is
        // lower than any roof could be: there, look at what is really
        // under it and rise over it, no steeper than [`CLEAR_SLOPE`].
        let mut need = Vec::new();
        for (i, &(p, h, _)) in track.iter().enumerate() {
            let sky = liner_sky(DVec2::new(p.x, p.z), h);
            if p.y >= (ground_under(&sky) + over_roofs).max(over_landmarks) {
                continue;
            }
            let top = highest_under(w, &sky, &[LandmarkKind::MooringMast]) + LINER_KEEL;
            if top > p.y {
                need.push((i, top));
            }
        }
        if !need.is_empty() {
            let mut along = Vec::with_capacity(track.len());
            let mut s = 0.0;
            for i in 0..track.len() {
                if i > 0 {
                    let (a, b) = (track[i - 1].0, track[i].0);
                    s += DVec2::new(b.x - a.x, b.z - a.z).length();
                }
                along.push(s);
            }
            for i in 0..track.len() {
                let lift = need
                    .iter()
                    .map(|&(j, y)| y - CLEAR_SLOPE * (along[i] - along[j]).abs())
                    .fold(f64::NEG_INFINITY, f64::max);
                track[i].0.y = track[i].0.y.max(lift);
            }
        }
        let fly_s = (track.len() - 1) as f64 * TRACK_DT;
        // From where the round ends, slowing evenly to a stop at the cup.
        let entry = track.last().map_or(cup, |t| t.0);
        let approach_s = 2.0 * entry.distance(cup) / CRUISE;
        // A session opens with the liner moored, about to cast off: the
        // first minutes show it leaving over the town.
        let offset = DOCKED_S - 25.0;
        MastRoute {
            cup,
            dir,
            side,
            track,
            fly_s,
            approach_s,
            offset,
            cruise,
        }
    }

    /// Flies the round: steering for each waypoint in turn, no faster
    /// round than [`TURN_RATE`], up to cruising speed and height and down
    /// again to the start of the glide in, lined up on the mast's axis.
    fn fly(
        w: &TownWorld,
        mast: &Landmark,
        cup: DVec3,
        dir: DVec2,
        side: DVec2,
        cruise: f64,
    ) -> Vec<(DVec3, f64, f64)> {
        let cup2 = DVec2::new(cup.x, cup.z);
        let core = w.core();
        let away = (core - mast.pos).normalize_or(-dir);
        let span = mast.pos.distance(core).max(400.0);
        let start = cup2 + dir * BACK_OFF;
        let beyond = core + away * (span * 0.5 + 900.0);
        let end = cup2 + dir * APPROACH;
        let marks = [
            start + side * 500.0 - dir * 100.0,
            core,
            beyond + away.perp() * 700.0,
            mast.pos + dir * (APPROACH + 1500.0) - side * 1300.0,
            cup2 + dir * (APPROACH + 1100.0),
            end,
        ];
        let mut rest = vec![0.0; marks.len()];
        for k in (0..marks.len() - 1).rev() {
            rest[k] = rest[k + 1] + marks[k].distance(marks[k + 1]);
        }
        let low = cup.y + BACK_UP;
        let land = cup.y + APPROACH_UP;
        let mut p = start;
        let mut h = det::atan2(side.x, side.y);
        let mut t = 0.0;
        let mut flown: f64 = 0.0;
        let mut k = 0;
        let mut out: Vec<(DVec3, f64, f64)> = Vec::new();
        // Never more than this long, whatever the town (a safety stop).
        while t < 3600.0 {
            let v = START_SPEED + (CRUISE - START_SPEED) * (t / SPEED_UP_S).min(1.0);
            let target = marks[k];
            let to = target - p;
            let last = k == marks.len() - 1;
            // Height: up to cruise over the first stretch, down to the glide's
            // start over the last.
            // (The way left: to this mark, then from mark to mark, so the
            // height does not jump as it turns for the next.)
            let left = to.length() + rest[k];
            let up = (flown / 700.0).min(1.0);
            let up = up * up * (3.0 - 2.0 * up);
            let down = (left / 1400.0).min(1.0);
            let down = down * down * (3.0 - 2.0 * down);
            let y = low + (cruise - low) * up;
            let y = land + (y - land) * down;
            let want = det::atan2(to.x, to.y);
            let turn = wrap(want - h).clamp(-TURN_RATE * TRACK_DT, TURN_RATE * TRACK_DT);
            out.push((DVec3::new(p.x, y, p.y), h, turn / TRACK_DT));
            if last && to.length() < v * TRACK_DT * 1.5 {
                break;
            }
            if !last && to.length() < 160.0 {
                k += 1;
            }
            h += turn;
            let step = det::dir2(PI * 0.5 - h) * (v * TRACK_DT);
            // dir2(pi/2 - h) is (sin h, cos h): heading h from +y towards +x.
            p += step;
            flown += v * TRACK_DT;
            t += TRACK_DT;
        }
        out
    }

    /// Seconds in one whole cycle.
    pub fn period(&self) -> f64 {
        DOCKED_S + CASTOFF_S + self.fly_s + self.approach_s
    }

    /// Seconds into the cycle at game second `g`.
    pub fn cycle_t(&self, g: f64) -> f64 {
        (g + self.offset).rem_euclid(self.period())
    }

    /// The game second (on or after `g`) at which the cycle is `t` in.
    pub fn when(&self, g: f64, t: f64) -> f64 {
        g + (t - self.cycle_t(g)).rem_euclid(self.period())
    }

    /// When each phase starts in the cycle.
    pub fn phase_start(&self, p: Phase) -> f64 {
        match p {
            Phase::Docked => 0.0,
            Phase::CastOff => DOCKED_S,
            Phase::Flight => DOCKED_S + CASTOFF_S,
            Phase::Approach => DOCKED_S + CASTOFF_S + self.fly_s,
        }
    }

    /// The moored heading: nose to the mast.
    fn moored_yaw(&self) -> f64 {
        det::atan2(-self.dir.x, -self.dir.y)
    }

    /// Where the liner is at game second `g`.
    pub fn pose(&self, g: f64) -> ShipPose {
        let t = self.cycle_t(g);
        let moored = self.moored_yaw();
        let pose = |nose, yaw, pitch, roll, thrust, phase, t| ShipPose {
            nose,
            yaw,
            pitch,
            roll,
            thrust,
            phase,
            t,
        };
        let axis = DVec3::new(self.dir.x, 0.0, self.dir.y);
        if t < DOCKED_S {
            // Weathervaning a little round the mast, easing in and out so
            // it meets the glide in and the cast off without a jump.
            let ease = det::sin(PI * t / DOCKED_S);
            let sway = (3.5f64.to_radians() * det::sin(TAU * t / 41.0)
                + 1.5f64.to_radians() * det::sin(TAU * t / 17.0))
                * ease;
            let bob = 0.25 * det::sin(TAU * t / 23.0) * ease;
            return pose(
                self.cup + DVec3::Y * bob,
                moored + sway,
                0.004 * det::sin(TAU * t / 29.0) * ease,
                0.006 * det::sin(TAU * t / 13.0) * ease,
                0.05,
                Phase::Docked,
                t,
            );
        }
        let t = t - DOCKED_S;
        if t < CASTOFF_S {
            // Backing off along the axis, rising, the nose swinging round
            // to turn away.
            let u = t / CASTOFF_S;
            let e = u * u * (3.0 - 2.0 * u);
            let nose = self.cup + axis * (BACK_OFF * e) + DVec3::Y * (BACK_UP * e);
            // The turn comes in the second half.
            let turn = ((u - 0.35) / 0.65).clamp(0.0, 1.0);
            let turn = turn * turn * (3.0 - 2.0 * turn);
            let away = det::atan2(self.side.x, self.side.y);
            let yaw = moored + wrap(away - moored) * turn;
            let pitch = 0.035 * det::sin(PI * u);
            return pose(nose, yaw, pitch, 0.0, 0.4, Phase::CastOff, t);
        }
        let t = t - CASTOFF_S;
        if t < self.fly_s {
            let f = t / TRACK_DT;
            let i = (f as usize).min(self.track.len() - 2);
            let u = f - i as f64;
            let (a, ha, ra) = self.track[i];
            let (b, hb, rb) = self.track[i + 1];
            let nose = a.lerp(b, u);
            let yaw = ha + wrap(hb - ha) * u;
            let climb = (b.y - a.y) / (a.distance(b).max(1e-6));
            let pitch = (climb * 0.8).clamp(-0.08, 0.08);
            // Banking a little into its turns.
            let roll = (-(ra + (rb - ra) * u) * 0.6).clamp(-0.06, 0.06);
            return pose(nose, yaw, pitch, roll, 1.0, Phase::Flight, t);
        }
        let t = t - self.fly_s;
        // From the round's end, slowing evenly to a stop at the cup, sinking
        // to it, swinging the last few degrees onto the axis.
        let (entry, entry_yaw, _) = *self.track.last().expect("a round");
        let u = (t / self.approach_s).clamp(0.0, 1.0);
        let gone = 1.0 - (1.0 - u) * (1.0 - u);
        let flat = |p: DVec3| DVec2::new(p.x, p.z);
        let start_out = (flat(entry) - flat(self.cup)).dot(self.dir);
        let start_off = (flat(entry) - flat(self.cup)).dot(self.dir.perp());
        let left = start_out * (1.0 - gone);
        // Off the axis by what the round left over, closing early.
        let k = 1.0 - (1.0 - gone).powi(3);
        let off = start_off * (1.0 - k);
        let plan = flat(self.cup) + self.dir * left + self.dir.perp() * off;
        let y = self.cup.y + (entry.y - self.cup.y) * (1.0 - gone) * (1.0 - gone);
        let nose = DVec3::new(plan.x, y, plan.y);
        let yaw = moored + wrap(entry_yaw - moored) * (1.0 - k);
        // Nose a touch down while it sinks.
        let pitch = -0.03 * (1.0 - gone) * (1.0 - u * 0.5);
        pose(
            nose,
            yaw,
            pitch,
            0.0,
            0.25 + 0.5 * (1.0 - u),
            Phase::Approach,
            t,
        )
    }
}

/// An angle wrapped into -pi..pi.
fn wrap(a: f64) -> f64 {
    (a + PI).rem_euclid(TAU) - PI
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::urban::params::TownParams;
    use hw_core::Seed;

    #[test]
    fn the_hull_is_round_and_closes() {
        assert_eq!(hull_r(0.0), 0.0);
        let widest = (0..=96).map(|s| hull_r(s as f64)).fold(0.0, f64::max);
        assert!((widest - SHIP_R).abs() < 0.05, "{widest}");
        assert!(hull_r(SHIP_LEN) < 0.2);
        assert!(hull_r(3.5) > 3.5, "the nose door is under a blunt nose");
    }

    #[test]
    fn the_route_is_smooth_and_comes_home() {
        let w = TownWorld::generate(Seed(2), TownParams::sample(Seed(2), 20_000.0));
        let mast = w
            .landmarks
            .iter()
            .find(|l| l.kind == LandmarkKind::MooringMast)
            .expect("a mooring mast");
        let r = MastRoute::new(&w, mast);
        let period = r.period();
        assert!((400.0..2400.0).contains(&period), "period {period}");
        let mut last = r.pose(0.0);
        let mut phases = std::collections::BTreeSet::new();
        let step = 0.25;
        let mut g = 0.0;
        while g < period * 1.05 {
            g += step;
            let p = r.pose(g);
            phases.insert(format!("{:?}", p.phase));
            // No jumps: the nose never moves faster than a fast airship.
            let v = p.nose.distance(last.nose) / step;
            assert!(v < CRUISE * 1.4, "{v} m/s at {g} ({:?})", p.phase);
            let turn = wrap(p.yaw - last.yaw).abs() / step;
            assert!(turn < 0.35, "turning {turn} rad/s at {g} ({:?})", p.phase);
            // Never down among the roofs away from the mast.
            let plan = DVec2::new(p.nose.x, p.nose.z);
            if p.phase == Phase::Flight && plan.distance(mast.pos) > 300.0 {
                assert!(p.nose.y > w.ground(plan) + 60.0, "low at {g}");
            }
            last = p;
        }
        assert_eq!(phases.len(), 4, "{phases:?}");
        // Moored, its nose is in the cup (bobbing a hand's width).
        let t0 = r.when(0.0, 10.0);
        assert!(r.pose(t0).nose.distance(r.cup) < 0.3);
        // A session opens with it moored.
        assert_eq!(r.pose(0.0).phase, Phase::Docked);
    }

    #[test]
    fn towns_of_every_size_get_a_mast() {
        for (seed, pop) in [(1u64, 800.0), (3, 6_000.0), (5, 60_000.0), (7, 600_000.0)] {
            let t = std::time::Instant::now();
            let w = TownWorld::generate(Seed(seed), TownParams::sample(Seed(seed), pop));
            let m = w
                .landmarks
                .iter()
                .find(|l| l.kind == LandmarkKind::MooringMast);
            let r = m.map(|m| MastRoute::new(&w, m));
            eprintln!(
                "seed {seed} pop {pop}: mast {:?} from core {:?} half {:.0} period {:?} cruise {:?} in {:.1}s",
                m.map(|m| m.pos),
                m.map(|m| m.pos.distance(w.core()) as i64),
                w.half(),
                r.as_ref().map(|r| r.period() as i64),
                r.as_ref().map(|r| r.cruise as i64),
                t.elapsed().as_secs_f64()
            );
            assert!(m.is_some(), "seed {seed} pop {pop}: no mooring mast");
        }
    }
}
