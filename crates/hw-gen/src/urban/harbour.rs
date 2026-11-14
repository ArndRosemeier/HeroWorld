//! The harbour: a broad mole built out from the shore into the sea, with
//! quays along both its sides, transit sheds down its middle, portal
//! cranes on rails along the cargo quay, a harbour master's tower on its
//! head, a detached breakwater sheltering the cargo basin with a light on
//! its head, and the ships: a passenger liner moored along one side, tugs
//! and launches at their pontoons, a freighter that comes and goes on a
//! timetable, and coasters passing out at sea.
//!
//! It is all built over the water, so it never takes a street or a house
//! from the town: only its root reaches back onto the shore, up to the
//! first road along the shore (or a little way inland where none runs).
//! Where it stands is pure data, and so is every ship's place at any second
//! of the game clock ([`Harbour::ships_at`]), the same for every player.
//!
//! Frames: the harbour's own frame has +z out to sea along the mole, x
//! towards the cargo side (the breakwater and the freighter's berth), y
//! up; see [`Harbour::to_plan`]. A ship's own frame has its origin on the
//! waterline in the middle of its length, +z towards its bow, x to
//! starboard, y up.

use super::footprint::Footprint;
use super::land::Cover;
use super::roadgrid::RoadGrid;
use super::world::{TownWorld, WATER_LEVEL};
use crate::building::{BuildingSite, BuildingUse};
use crate::city::DistrictKind;
use crate::water::segment_distance;
use glam::DVec2;
use hw_core::{det, Seed, StableId};
use std::f64::consts::{PI, TAU};

/// Half the mole's width, and how far it reaches out from the shore.
pub const MOLE_HALF: f64 = 30.0;
pub const MOLE_LEN: f64 = 230.0;
/// The mole's head has its corners cut off by this much.
pub const CHAMFER: f64 = 10.0;
/// How many points across the mole its land end is set at.
pub const ROOT_SAMPLES: usize = 13;
/// How far the mole reaches back onto the shore at most, where no road
/// meets it first.
pub const ROOT_MAX: f64 = 16.0;
/// The fenders along a quay: a ship lies this far off the quay's edge.
pub const FENDER: f64 = 1.4;
/// The transit sheds down the mole's middle: half their width, and their
/// ends (local z).
pub const SHED_HALF: f64 = 13.0;
pub const SHEDS: [(f64, f64); 2] = [(22.0, 92.0), (106.0, 176.0)];
/// The cargo cranes: where they stand along the cargo quay (local z), the
/// way each boom is slewed from straight out over the water (radians,
/// towards +z), and the portal's legs (local x, inner and outer).
pub const CRANES: [(f64, f64); 3] = [(50.0, -0.26), (90.0, -0.35), (130.0, 0.3)];
pub const CRANE_LEGS: (f64, f64) = (MOLE_HALF - 12.5, MOLE_HALF - 2.5);
/// The portal's height (its deck), where the boom is hinged (above the
/// deck, out from the portal's middle), the boom's length and its lift.
pub const PORTAL_H: f64 = 8.0;
pub const BOOM_HINGE: (f64, f64) = (11.5, 2.0);
pub const BOOM_LEN: f64 = 32.0;
pub const BOOM_LIFT: f64 = 0.61;
/// The crane's machine house on the portal: half width, half length, height.
pub const CRANE_HOUSE: (f64, f64, f64) = (3.0, 3.6, 4.6);
/// The harbour master's tower on the mole's head: local z, its shaft's
/// radius, the saucer cab's radius, floor and roof, and the mast's top.
pub const TOWER_Z: f64 = 204.0;
pub const TOWER_SHAFT: f64 = 2.6;
pub const TOWER_CAB: (f64, f64, f64) = (6.5, 15.0, 19.0);
pub const TOWER_TOP: f64 = 27.0;
/// The passenger side's gangway towers (local z) and their size: half
/// width and depth, height, and the gangway's height.
pub const GANGWAYS: [f64; 2] = [95.0, 140.0];
pub const GANGWAY_TOWER: (f64, f64, f64) = (2.2, 2.2, 11.0);
pub const GANGWAY_Y: f64 = 8.6;
/// The breakwater: its crown line (local), the crown's half width and its
/// top above the quay, the parapet on its sea side, and the slope of its
/// rubble (metres out per metre down).
pub const BREAKWATER: [(f64, f64); 4] = [
    (140.0, 64.0),
    (140.0, 190.0),
    (128.0, 232.0),
    (108.0, 252.0),
];
pub const CROWN_HALF: f64 = 4.0;
pub const CROWN_UP: f64 = 1.6;
pub const PARAPET: f64 = 1.3;
pub const RUBBLE_SLOPE: f64 = 1.5;
/// The light on the breakwater's head: radius and height above the crown.
pub const LIGHT_R: f64 = 1.5;
pub const LIGHT_H: f64 = 10.0;
/// The pontoons near the mole's root: (local x of the inner edge, z from,
/// z to); the cargo side's for the tug, the passenger side's for launches.
pub const PONTOON_W: f64 = 3.2;
pub const PONTOONS: [(f64, f64, f64); 2] =
    [(MOLE_HALF, 6.0, 32.0), (-MOLE_HALF - PONTOON_W, 6.0, 30.0)];
/// A pontoon's deck above the water.
pub const PONTOON_UP: f64 = 0.5;
/// Bollards along the quays: every this many metres.
pub const BOLLARD_STEP: f64 = 15.0;
/// Lamp posts down the mole: local x and the z of each.
pub const LAMP_X: f64 = 15.5;
pub const LAMPS_Z: [f64; 5] = [14.0, 60.0, 99.0, 140.0, 186.0];
pub const LAMP_H: f64 = 9.0;
/// Stacks of crates on the passenger side between the sheds and the
/// gangways: (local x, z, half x, half z, height).
pub const CRATES: [(f64, f64, f64, f64, f64); 4] = [
    (-19.0, 30.0, 2.6, 5.0, 2.6),
    (-19.0, 46.0, 2.6, 4.0, 5.2),
    (-19.5, 180.0, 2.2, 3.4, 2.6),
    (19.0, 186.0, 2.6, 4.4, 2.6),
];

// ------------------------------------------------------------------ ships

/// What kind of ship.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ShipKind {
    /// A streamlined cargo liner: forecastle, three hatches, derrick
    /// posts, the bridge house and a raked funnel aft.
    Freighter,
    /// The ocean liner: four decks of promenades, a rounded bridge front,
    /// two teardrop funnels.
    Liner,
    /// A harbour tug: low, broad, a big wheelhouse and funnel.
    Tug,
    /// A motor launch with a little cabin.
    Launch,
}

/// A ship's main sizes (m).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShipDims {
    pub len: f64,
    pub beam: f64,
    /// Deck above the water amidships.
    pub free: f64,
    /// Keel below the water.
    pub draft: f64,
    /// How much higher the deck rises at the bow.
    pub sheer: f64,
    /// How far the stem leans forward from the keel to the deck.
    pub rake: f64,
}

/// One block of a ship's upper works, in its own frame: from `z0` to `z1`
/// along it, `half` wide each side, from `y0` to `y1` above the water.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Block {
    pub z0: f64,
    pub z1: f64,
    pub half: f64,
    pub y0: f64,
    pub y1: f64,
}

/// A funnel or a mast: at (x, z), radius, from `y0` to `y1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stack {
    pub x: f64,
    pub z: f64,
    pub r: f64,
    pub y0: f64,
    pub y1: f64,
}

impl ShipKind {
    pub fn dims(self) -> ShipDims {
        match self {
            ShipKind::Freighter => ShipDims {
                len: 118.0,
                beam: 17.0,
                free: 6.5,
                draft: 6.5,
                sheer: 2.4,
                rake: 7.0,
            },
            ShipKind::Liner => ShipDims {
                len: 156.0,
                beam: 22.0,
                free: 9.0,
                draft: 8.0,
                sheer: 2.8,
                rake: 10.0,
            },
            ShipKind::Tug => ShipDims {
                len: 26.0,
                beam: 8.4,
                free: 2.4,
                draft: 3.0,
                sheer: 1.3,
                rake: 1.6,
            },
            ShipKind::Launch => ShipDims {
                len: 10.0,
                beam: 3.2,
                free: 1.0,
                draft: 0.8,
                sheer: 0.4,
                rake: 0.8,
            },
        }
    }

    /// The upper works' blocks (decks, the bridge, a wheelhouse).
    pub fn blocks(self) -> Vec<Block> {
        let b = |z0, z1, half, y0, y1| Block {
            z0,
            z1,
            half,
            y0,
            y1,
        };
        match self {
            ShipKind::Freighter => vec![
                // Forecastle.
                b(40.0, 57.0, 6.6, 6.5, 8.8),
                // The bridge house aft, three tiers, the bridge with wings.
                b(-40.0, -17.0, 7.2, 6.5, 9.3),
                b(-36.0, -19.0, 6.4, 9.3, 12.0),
                b(-25.0, -19.5, 8.3, 12.0, 14.6),
                // Poop.
                b(-57.0, -46.0, 6.0, 6.5, 8.6),
            ],
            ShipKind::Liner => vec![
                b(-62.0, 58.0, 10.6, 9.0, 11.9),
                b(-54.0, 48.0, 9.6, 11.9, 14.8),
                b(-44.0, 40.0, 8.4, 14.8, 17.6),
                b(24.0, 36.0, 10.4, 17.6, 20.2),
            ],
            ShipKind::Tug => vec![b(-3.0, 5.0, 2.7, 2.4, 5.4), b(-9.0, -3.0, 2.2, 2.4, 3.6)],
            ShipKind::Launch => vec![b(-1.2, 2.0, 1.15, 1.0, 2.0)],
        }
    }

    /// Funnels and masts.
    pub fn stacks(self) -> Vec<Stack> {
        let s = |x, z, r, y0, y1| Stack { x, z, r, y0, y1 };
        match self {
            ShipKind::Freighter => vec![
                s(0.0, -31.0, 2.4, 12.0, 19.5),
                // Derrick posts (kept under the cranes' booms).
                s(0.0, 30.0, 0.45, 6.5, 19.0),
                s(0.0, -2.0, 0.45, 6.5, 19.0),
            ],
            ShipKind::Liner => vec![
                s(0.0, 10.0, 3.3, 17.6, 27.5),
                s(0.0, -16.0, 3.3, 17.6, 27.5),
                s(0.0, 44.0, 0.4, 20.2, 33.0),
            ],
            ShipKind::Tug => vec![s(0.0, -5.0, 1.05, 3.6, 7.8)],
            ShipKind::Launch => Vec::new(),
        }
    }
}

/// Half the hull's width at `u` along it (-0.5 stern .. 0.5 bow) and `h`
/// up its side (0 keel .. 1 deck): a full, boxy middle body, a fine bow
/// and a rounded cruiser stern, the sections narrowing to the keel.
pub fn hull_half(kind: ShipKind, u: f64, h: f64) -> f64 {
    let d = kind.dims();
    let (bow, stern) = (0.32, 0.17);
    let u = u.clamp(-0.5, 0.5);
    let h = h.clamp(0.0, 1.0);
    let (plan, end) = if u > 0.5 - bow {
        let t = (u - (0.5 - bow)) / bow;
        (det::powf((1.0 - t * t).max(0.0), 0.75), t)
    } else if u < -0.5 + stern {
        let t = ((-0.5 + stern) - u) / stern;
        ((1.0 - t * t).max(0.0).sqrt(), t)
    } else {
        (1.0, 0.0)
    };
    // The bilge: amidships nearly square, at the ends a deep V.
    let k = 0.14 + 0.7 * end * end;
    let section = 1.0 - k * (1.0 - h).powi(2) - 0.06 * (1.0 - h).powi(8);
    d.beam * 0.5 * plan * section.max(0.0)
}

/// The deck's height above the water at `u` along the hull: rising to the
/// bow (and a little to the stern).
pub fn deck_y(kind: ShipKind, u: f64) -> f64 {
    let d = kind.dims();
    let fwd = ((u - 0.05) / 0.45).clamp(0.0, 1.0);
    let aft = ((-u - 0.3) / 0.2).clamp(0.0, 1.0);
    d.free + d.sheer * fwd * fwd + d.sheer * 0.25 * aft * aft
}

/// How far forward of `u * len` the stem stands at height share `h` (the
/// raked bow), zero aft.
pub fn stem_lean(kind: ShipKind, u: f64, h: f64) -> f64 {
    let d = kind.dims();
    let near = ((u - 0.38) / 0.12).clamp(0.0, 1.0);
    d.rake * det::powf(h.clamp(0.0, 1.0), 1.6) * near * near
}

/// Where a ship is and the way it faces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShipPose {
    /// The middle of its waterline, in the town's plan.
    pub at: DVec2,
    /// Heading (radians, 0 = +z, turning towards +x).
    pub yaw: f64,
    /// Speed through the water (m/s, negative going astern).
    pub speed: f64,
}

impl ShipPose {
    /// The way the bow points.
    pub fn forward(&self) -> DVec2 {
        let (s, c) = det::sin_cos(self.yaw);
        DVec2::new(s, c)
    }

    /// From the ship's frame (x starboard, z forward) to the town's plan.
    pub fn to_plan(&self, l: DVec2) -> DVec2 {
        let f = self.forward();
        let x = DVec2::new(f.y, -f.x);
        self.at + x * l.x + f * l.y
    }
}

/// Heading of a plan direction (0 = +z, turning towards +x).
pub fn yaw_of(d: DVec2) -> f64 {
    det::atan2(d.x, d.y)
}

/// The ship's solid parts at `pose`: its hull sliced into short upright
/// boxes (each at most [`TILE`] on a side, so a moving ship counts as a
/// set of small movers), its upper works and its funnels. `y` is the
/// water's level.
pub fn ship_solids(kind: ShipKind, pose: &ShipPose, y: f64) -> Vec<Footprint> {
    let d = kind.dims();
    let fwd = pose.forward();
    let mut out = Vec::new();
    let slices = (d.len / TILE).ceil() as usize;
    let long = d.len / slices as f64;
    for i in 0..slices {
        let (z0, z1) = (
            -d.len * 0.5 + long * i as f64,
            -d.len * 0.5 + long * (i + 1) as f64,
        );
        let (u0, u1) = (z0 / d.len, z1 / d.len);
        let half = [u0, (u0 + u1) * 0.5, u1]
            .into_iter()
            .map(|u| hull_half(kind, u, 1.0))
            .fold(0.0, f64::max);
        if half < 0.2 {
            continue;
        }
        let top = deck_y(kind, (u0 + u1) * 0.5) + 1.0;
        let n = ((2.0 * half) / TILE).ceil().max(1.0) as usize;
        let across = 2.0 * half / n as f64;
        for k in 0..n {
            let x = -half + across * (k as f64 + 0.5);
            out.push(Footprint::Box {
                at: pose.to_plan(DVec2::new(x, (z0 + z1) * 0.5)),
                dir: fwd,
                half: DVec2::new(across * 0.5, long * 0.5),
                y0: y - d.draft,
                y1: y + top,
            });
        }
    }
    for b in kind.blocks() {
        let nz = ((b.z1 - b.z0) / TILE).ceil().max(1.0) as usize;
        let nx = ((2.0 * b.half) / TILE).ceil().max(1.0) as usize;
        let (lz, lx) = ((b.z1 - b.z0) / nz as f64, 2.0 * b.half / nx as f64);
        for i in 0..nz {
            for k in 0..nx {
                let c = DVec2::new(
                    -b.half + lx * (k as f64 + 0.5),
                    b.z0 + lz * (i as f64 + 0.5),
                );
                out.push(Footprint::Box {
                    at: pose.to_plan(c),
                    dir: fwd,
                    half: DVec2::new(lx * 0.5, lz * 0.5),
                    y0: y + b.y0,
                    y1: y + b.y1,
                });
            }
        }
    }
    for s in kind.stacks() {
        out.push(Footprint::Disc {
            at: pose.to_plan(DVec2::new(s.x, s.z)),
            r: s.r,
            y0: y + s.y0,
            y1: y + s.y1,
        });
    }
    out
}

/// The largest side of a ship's solid box (m).
pub const TILE: f64 = 4.6;

// ---------------------------------------------------------------- harbour

/// The harbour's plan.
#[derive(Clone, Debug, PartialEq)]
pub struct Harbour {
    /// Where the mole meets the shore (the middle of its width), and the
    /// way out to sea along it.
    pub root: DVec2,
    pub dir: DVec2,
    /// Towards the cargo side (+x in the harbour's frame).
    pub right: DVec2,
    /// The quays' top.
    pub deck: f64,
    /// The mole's land end: local z at [`ROOT_SAMPLES`] points evenly
    /// across its width (x from -[`MOLE_HALF`] to +[`MOLE_HALF`]), where
    /// it meets the road along the shore or runs a little way onto land.
    pub root_line: Vec<f64>,
    /// The freighter's way out and back, in the town's plan, from where it
    /// has turned round off the mole's head to its berth: points every
    /// [`PATH_STEP`] metres or so.
    pub sail: Vec<DVec2>,
    /// The ships passing out at sea: (kind, a point on its lane, the way
    /// it sails, speed, start offset along the lane).
    pub traffic: Vec<Passing>,
}

/// A ship passing out at sea along a straight lane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Passing {
    pub kind: ShipKind,
    pub mid: DVec2,
    pub dir: DVec2,
    pub speed: f64,
    pub offset: f64,
}

/// What the freighter is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Moored at its berth, the cranes at work.
    Docked,
    /// Backing out of the basin stern first, the tug pulling.
    Backing,
    /// Turning round off the mole's head.
    Turning,
    /// Out to sea, round and back in to the berth.
    Sailing,
}

/// Spacing of the points of the freighter's way.
pub const PATH_STEP: f64 = 4.0;
/// Seconds moored, backing out and turning round.
pub const DOCKED_S: f64 = 420.0;
pub const BACKING_S: f64 = 250.0;
pub const TURNING_S: f64 = 100.0;
/// How far it backs out from its berth (m).
pub const BACK_OUT: f64 = 270.0;
/// Its speed out at sea (m/s) and how far it takes to get up to it.
pub const SEA_SPEED: f64 = 7.0;
const SPEED_UP: f64 = 500.0;
/// The tug comes out this long before the freighter backs out, and goes
/// home this long after it has turned round.
pub const TUG_OUT_S: f64 = 90.0;
pub const TUG_HOME_S: f64 = 110.0;
/// The tug pulls from this far behind the freighter's stern.
pub const TOW: f64 = 26.0;
/// How far out at sea the coasters pass by, and how far along their lanes
/// they sail before they come round again (they are out of sight then).
pub const LANE_SPAN: f64 = 9000.0;

impl Harbour {
    /// From the harbour's frame (x to the cargo side, z out to sea) to the
    /// town's plan.
    pub fn to_plan(&self, l: DVec2) -> DVec2 {
        self.root + self.right * l.x + self.dir * l.y
    }

    /// From the town's plan to the harbour's frame.
    pub fn to_local(&self, p: DVec2) -> DVec2 {
        let d = p - self.root;
        DVec2::new(d.dot(self.right), d.dot(self.dir))
    }

    /// A heading in the harbour's frame (0 = out along the mole, turning
    /// towards the cargo side) as a heading in the town's plan.
    pub fn yaw(&self, local: f64) -> f64 {
        let (s, c) = det::sin_cos(local);
        yaw_of(self.right * s + self.dir * c)
    }

    /// The mole's outline in the town's plan (land end to head, the head's
    /// corners cut).
    pub fn mole_outline(&self) -> Vec<DVec2> {
        let (h, z1, c) = (MOLE_HALF, MOLE_LEN, CHAMFER);
        let mut pts: Vec<(f64, f64)> = (0..ROOT_SAMPLES)
            .map(|i| (root_x(i), self.root_line[i]))
            .collect();
        pts.extend([(h, z1 - c), (h - c, z1), (-h + c, z1), (-h, z1 - c)]);
        pts.iter()
            .map(|&(x, z)| self.to_plan(DVec2::new(x, z)))
            .collect()
    }

    /// The mole's land end at local `x` (between the root line's points).
    pub fn land_end_at(&self, x: f64) -> f64 {
        let f = ((x + MOLE_HALF) / (2.0 * MOLE_HALF)).clamp(0.0, 1.0) * (ROOT_SAMPLES - 1) as f64;
        let i = (f.floor() as usize).min(ROOT_SAMPLES - 2);
        let t = f - i as f64;
        self.root_line[i] + (self.root_line[i + 1] - self.root_line[i]) * t
    }

    /// Is the local point on the mole's deck?
    fn on_mole(&self, l: DVec2) -> bool {
        l.x.abs() <= MOLE_HALF
            && l.y >= self.land_end_at(l.x)
            && l.y <= MOLE_LEN
            && (MOLE_HALF - l.x.abs()) + (MOLE_LEN - l.y) >= CHAMFER
    }

    /// The breakwater's crown line in the town's plan.
    pub fn breakwater(&self) -> Vec<DVec2> {
        BREAKWATER
            .iter()
            .map(|&(x, z)| self.to_plan(DVec2::new(x, z)))
            .collect()
    }

    /// Height of the breakwater's crown.
    pub fn crown(&self) -> f64 {
        self.deck + CROWN_UP
    }

    /// How far from the crown line the rubble reaches at the sea bed `bed`.
    pub fn toe(&self, bed: f64) -> f64 {
        CROWN_HALF + (self.crown() - bed).max(0.0) * RUBBLE_SLOPE
    }

    /// Distance in plan from the breakwater's crown line.
    pub fn breakwater_distance(&self, p: DVec2) -> f64 {
        let pts = self.breakwater();
        pts.windows(2)
            .map(|w| segment_distance(p, w[0], w[1]))
            .fold(f64::MAX, f64::min)
    }

    /// The raised ground the harbour makes at `p`: the mole's deck (and its
    /// the breakwater's crown, a pontoon. None elsewhere.
    pub fn deck_at(&self, p: DVec2) -> Option<f64> {
        let l = self.to_local(p);
        if self.on_mole(l) {
            return Some(self.deck);
        }
        if self.breakwater_distance(p) <= CROWN_HALF {
            return Some(self.crown());
        }
        for (x0, z0, z1) in PONTOONS {
            if l.x >= x0 && l.x <= x0 + PONTOON_W && l.y >= z0 && l.y <= z1 {
                return Some(WATER_LEVEL + PONTOON_UP);
            }
        }
        None
    }

    /// Does a circle of radius `r` at `p` reach onto the harbour (its mole,
    /// pontoons or breakwater)?
    pub fn covers(&self, p: DVec2, r: f64) -> bool {
        let l = self.to_local(p);
        let back = self.land_end_at(l.x);
        let mole = l.x.abs() <= MOLE_HALF + PONTOON_W + r && l.y >= back - r && l.y <= MOLE_LEN + r;
        mole || self.breakwater_distance(p) <= CROWN_HALF + r
    }

    /// Does a building's footprint come within `margin` of the harbour?
    pub fn overlaps(&self, site: &BuildingSite, margin: f64) -> bool {
        let c = site.corners();
        let mid = c.iter().sum::<DVec2>() * 0.25;
        // Test the corners, the middle and points along the edges.
        let mut pts = vec![mid];
        for k in 0..4 {
            let (a, b) = (c[k], c[(k + 1) % 4]);
            for i in 0..8 {
                pts.push(a.lerp(b, i as f64 / 8.0));
            }
        }
        pts.iter().any(|p| self.covers(*p, margin))
    }

    /// The sheds down the mole's middle, as buildings (they break like
    /// every other building). Their fronts face the cargo quay.
    pub fn sheds(&self, seed: Seed) -> Vec<BuildingSite> {
        // The front runs along the cargo side; inward (along.perp) points
        // across the mole to the passenger side.
        let along = if (-self.dir).perp().dot(self.right) < 0.0 {
            -self.dir
        } else {
            self.dir
        };
        SHEDS
            .iter()
            .enumerate()
            .map(|(i, &(z0, z1))| {
                let start = if along.dot(self.dir) < 0.0 { z1 } else { z0 };
                let s = seed.child("harbour").child("shed").child(i as u32);
                BuildingSite {
                    id: StableId::from_seed(s),
                    seed: s,
                    usage: BuildingUse::Works,
                    origin: self.to_plan(DVec2::new(SHED_HALF, start)),
                    along,
                    inward: along.perp(),
                    size: DVec2::new(z1 - z0, 2.0 * SHED_HALF),
                    floors: 1,
                    street: 0,
                    district: DistrictKind::AetherWorks,
                    halls: None,
                    vernacular: None,
                }
            })
            .collect()
    }

    /// Where each crane stands: its portal's middle in plan, the way out
    /// over the water, and the boom's heading (plan, unit).
    pub fn cranes(&self) -> Vec<(DVec2, DVec2, DVec2)> {
        CRANES
            .iter()
            .map(|&(z, slew)| {
                let mid = (CRANE_LEGS.0 + CRANE_LEGS.1) * 0.5;
                let (s, c) = det::sin_cos(slew);
                (
                    self.to_plan(DVec2::new(mid, z)),
                    self.right,
                    (self.right * c + self.dir * s).normalize(),
                )
            })
            .collect()
    }

    /// The bollards along both quays (plan).
    pub fn bollards(&self) -> Vec<DVec2> {
        let mut out = Vec::new();
        let n = ((MOLE_LEN - CHAMFER - 8.0) / BOLLARD_STEP) as usize;
        for side in [-1.0, 1.0] {
            for k in 0..=n {
                let z = 8.0 + BOLLARD_STEP * k as f64;
                // Not where a pontoon or a gangway lands.
                let pontoon = PONTOONS.iter().any(|&(x0, z0, z1)| {
                    (x0 + PONTOON_W * 0.5).signum() == side && z > z0 - 1.0 && z < z1 + 1.0
                });
                let gangway = side < 0.0 && GANGWAYS.iter().any(|g| (z - g).abs() < 4.0);
                if !pontoon && !gangway {
                    out.push(self.to_plan(DVec2::new(side * (MOLE_HALF - 0.7), z)));
                }
            }
        }
        out
    }

    /// The lamp posts down the mole (plan), each with the way its arm
    /// reaches: out towards its quay. None stands under a crane.
    pub fn lamps(&self) -> Vec<(DVec2, DVec2)> {
        let mut out = Vec::new();
        for z in LAMPS_Z {
            for x in [-LAMP_X, LAMP_X] {
                if !(x > 0.0 && CRANES.iter().any(|c| (c.0 - z).abs() < 7.0)) {
                    out.push((self.to_plan(DVec2::new(x, z)), self.right * x.signum()));
                }
            }
        }
        out
    }

    /// The harbour's fixed solid parts above its decks (the sheds are
    /// buildings, the decks raised ground): cranes, the tower, the gangway
    /// towers, the light, bollards, lamps, crates, the parapet, and the
    /// ships that never leave (the liner, the launches).
    pub fn footprints(&self) -> Vec<Footprint> {
        let deck = self.deck;
        let mut v = Vec::new();
        let local = |x: f64, z: f64| self.to_plan(DVec2::new(x, z));
        // Cranes.
        for (mid, out, boom) in self.cranes() {
            let along = out.perp();
            for lx in [CRANE_LEGS.0, CRANE_LEGS.1] {
                for lz in [-4.0, 4.0] {
                    let at = mid + out * (lx - (CRANE_LEGS.0 + CRANE_LEGS.1) * 0.5) + along * lz;
                    v.push(Footprint::Disc {
                        at,
                        r: 0.55,
                        y0: deck,
                        y1: deck + PORTAL_H,
                    });
                }
            }
            v.push(Footprint::Box {
                at: mid,
                dir: out,
                half: DVec2::new(5.0, 6.0),
                y0: deck + PORTAL_H - 1.2,
                y1: deck + PORTAL_H,
            });
            // The house turns with the boom.
            v.push(Footprint::Box {
                at: mid - boom * 0.6,
                dir: boom,
                half: DVec2::new(CRANE_HOUSE.0, CRANE_HOUSE.1),
                y0: deck + PORTAL_H,
                y1: deck + PORTAL_H + CRANE_HOUSE.2,
            });
            for (at, y) in boom_points(mid, boom, deck) {
                v.push(Footprint::Box {
                    at,
                    dir: boom,
                    half: DVec2::new(0.8, 1.6),
                    y0: y - 0.8,
                    y1: y + 0.8,
                });
            }
        }
        // The harbour master's tower.
        let t = local(0.0, TOWER_Z);
        v.push(Footprint::Disc {
            at: t,
            r: TOWER_SHAFT + 1.2,
            y0: deck,
            y1: deck + 3.4,
        });
        v.push(Footprint::Disc {
            at: t,
            r: TOWER_SHAFT,
            y0: deck,
            y1: deck + TOWER_CAB.1,
        });
        v.push(Footprint::Disc {
            at: t,
            r: TOWER_CAB.0,
            y0: deck + TOWER_CAB.1,
            y1: deck + TOWER_CAB.2,
        });
        v.push(Footprint::Disc {
            at: t,
            r: 0.4,
            y0: deck + TOWER_CAB.2,
            y1: deck + TOWER_TOP,
        });
        // Gangway towers and their bridges out to the liner's side.
        for z in GANGWAYS {
            let (hx, hz, h) = GANGWAY_TOWER;
            v.push(Footprint::Box {
                at: local(-MOLE_HALF + hx + 0.6, z),
                dir: self.dir,
                half: DVec2::new(hx, hz),
                y0: deck,
                y1: deck + h,
            });
            let reach = FENDER + 1.0;
            v.push(Footprint::Box {
                at: local(-MOLE_HALF - reach * 0.5 + 0.6, z),
                dir: self.dir,
                half: DVec2::new(reach * 0.5 + 0.6, 1.1),
                y0: deck + GANGWAY_Y - 0.3,
                y1: deck + GANGWAY_Y + 2.4,
            });
        }
        // Bollards and lamp posts.
        for b in self.bollards() {
            v.push(Footprint::Disc {
                at: b,
                r: 0.32,
                y0: deck,
                y1: deck + 0.75,
            });
        }
        for (at, _) in self.lamps() {
            v.push(Footprint::Disc {
                at,
                r: 0.22,
                y0: deck,
                y1: deck + LAMP_H,
            });
        }
        for (x, z, hx, hz, h) in CRATES {
            v.push(Footprint::Box {
                at: local(x, z),
                dir: self.dir,
                half: DVec2::new(hx, hz),
                y0: deck,
                y1: deck + h,
            });
        }
        // The breakwater's parapet along its sea side, and its light.
        let pts = self.breakwater();
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let d = (b - a).normalize();
            // Sea side: away from the mole.
            let mut n = d.perp();
            if n.dot(self.right) < 0.0 {
                n = -n;
            }
            let len = a.distance(b);
            let k = (len / 12.0).ceil() as usize;
            for i in 0..k {
                let m = a.lerp(b, (i as f64 + 0.5) / k as f64) + n * (CROWN_HALF - 0.5);
                v.push(Footprint::Box {
                    at: m,
                    dir: n,
                    half: DVec2::new(len / k as f64 * 0.5, 0.5),
                    y0: self.crown(),
                    y1: self.crown() + PARAPET,
                });
            }
        }
        let head = *pts.last().expect("breakwater");
        v.push(Footprint::Disc {
            at: head,
            r: LIGHT_R,
            y0: self.crown(),
            y1: self.crown() + LIGHT_H,
        });
        // The ships that stay.
        for (kind, pose) in self.moored() {
            v.extend(ship_solids(kind, &pose, WATER_LEVEL));
        }
        v
    }

    /// The ships always at their moorings: the liner, two launches. The
    /// tug and the freighter come and go ([`Self::ships_at`]).
    pub fn moored(&self) -> Vec<(ShipKind, ShipPose)> {
        let liner = ShipKind::Liner.dims();
        let launch = ShipKind::Launch.dims();
        let pontoon = PONTOONS[1];
        let mut out = vec![(
            ShipKind::Liner,
            ShipPose {
                at: self.to_plan(DVec2::new(-(MOLE_HALF + FENDER + liner.beam * 0.5), 118.0)),
                yaw: self.yaw(0.0),
                speed: 0.0,
            },
        )];
        for (k, z) in [12.0, 24.0].into_iter().enumerate() {
            out.push((
                ShipKind::Launch,
                ShipPose {
                    at: self.to_plan(DVec2::new(pontoon.0 - 0.5 - launch.beam * 0.5, z)),
                    yaw: self.yaw(if k == 0 { 0.0 } else { PI }),
                    speed: 0.0,
                },
            ));
        }
        out
    }

    /// The freighter's berth (heading in, bow to the shore).
    pub fn freighter_berth(&self) -> ShipPose {
        let d = ShipKind::Freighter.dims();
        ShipPose {
            at: self.to_plan(DVec2::new(MOLE_HALF + FENDER + d.beam * 0.5, 100.0)),
            yaw: self.yaw(PI),
            speed: 0.0,
        }
    }

    /// Where the freighter turns round, off the mole's head.
    fn turn_point(&self) -> DVec2 {
        let b = self.to_local(self.freighter_berth().at);
        self.to_plan(DVec2::new(b.x, b.y + BACK_OUT))
    }

    /// The tug at its pontoon.
    pub fn tug_berth(&self) -> ShipPose {
        let d = ShipKind::Tug.dims();
        let (x0, z0, z1) = PONTOONS[0];
        ShipPose {
            at: self.to_plan(DVec2::new(
                x0 + PONTOON_W + 0.6 + d.beam * 0.5,
                (z0 + z1) * 0.5,
            )),
            yaw: self.yaw(PI),
            speed: 0.0,
        }
    }

    /// Length of the freighter's way out and back.
    fn sail_len(&self) -> f64 {
        self.sail.windows(2).map(|w| w[0].distance(w[1])).sum()
    }

    /// Seconds it sails out and back: speeding up over the first
    /// [`SPEED_UP`] metres, slowing down over the last, cruising between.
    pub fn sailing_s(&self) -> f64 {
        let l = self.sail_len();
        let ramp = SPEED_UP.min(l * 0.5);
        4.0 * ramp / SEA_SPEED + (l - 2.0 * ramp) / SEA_SPEED
    }

    /// The freighter's whole round (s).
    pub fn cycle(&self) -> f64 {
        DOCKED_S + BACKING_S + TURNING_S + self.sailing_s()
    }

    /// Where the freighter is in its round at game second `t`: the phase
    /// and the seconds into it.
    pub fn phase_at(&self, t: f64) -> (Phase, f64) {
        let mut c = t.rem_euclid(self.cycle());
        for (p, d) in [
            (Phase::Docked, DOCKED_S),
            (Phase::Backing, BACKING_S),
            (Phase::Turning, TURNING_S),
        ] {
            if c < d {
                return (p, c);
            }
            c -= d;
        }
        (Phase::Sailing, c)
    }

    /// The point `s` metres along the way out and back, and the way it
    /// heads there.
    fn along_sail(&self, s: f64) -> (DVec2, DVec2) {
        let mut left = s.max(0.0);
        for w in self.sail.windows(2) {
            let l = w[0].distance(w[1]);
            if left <= l && l > 1e-9 {
                let d = (w[1] - w[0]) / l;
                return (w[0] + d * left, d);
            }
            left -= l;
        }
        let n = self.sail.len();
        (
            self.sail[n - 1],
            (self.sail[n - 1] - self.sail[n - 2]).normalize_or(self.dir),
        )
    }

    /// How far along its way out and back the freighter is `t` seconds
    /// after it set off, and its speed then. Its speed rises and falls
    /// along a smoothstep, so it never jerks.
    fn sailed(&self, t: f64) -> (f64, f64) {
        let l = self.sail_len();
        let ramp = SPEED_UP.min(l * 0.5);
        let tr = 2.0 * ramp / SEA_SPEED;
        let cruise = (l - 2.0 * ramp) / SEA_SPEED;
        let ease = |u: f64| {
            (
                2.0 * ramp * (u * u * u - 0.5 * u * u * u * u),
                SEA_SPEED * u * u * (3.0 - 2.0 * u),
            )
        };
        if t < tr {
            ease((t / tr).clamp(0.0, 1.0))
        } else if t < tr + cruise {
            (ramp + (t - tr) * SEA_SPEED, SEA_SPEED)
        } else {
            let r = 1.0 - ((t - tr - cruise) / tr).clamp(0.0, 1.0);
            let (left, v) = ease(r);
            (l - left, v)
        }
    }

    /// The freighter at game second `t`.
    pub fn freighter_pose(&self, t: f64) -> (ShipPose, Phase) {
        let berth = self.freighter_berth();
        let (phase, s) = self.phase_at(t);
        let out = self.dir;
        let pose = match phase {
            Phase::Docked => berth,
            Phase::Backing => {
                let u = s / BACKING_S;
                let e = u * u * (3.0 - 2.0 * u);
                let v = BACK_OUT * 6.0 * u * (1.0 - u) / BACKING_S;
                ShipPose {
                    at: berth.at + out * (BACK_OUT * e),
                    yaw: berth.yaw,
                    speed: -v,
                }
            }
            Phase::Turning => {
                let u = s / TURNING_S;
                let e = u * u * (3.0 - 2.0 * u);
                // From bow-in to bow-out, swinging the bow round through
                // the cargo side.
                ShipPose {
                    at: self.turn_point(),
                    yaw: self.yaw(PI - PI * e),
                    speed: 0.0,
                }
            }
            Phase::Sailing => {
                let (d, v) = self.sailed(s);
                let (p, _) = self.along_sail(d);
                // Heading along the chord over a ship's length round it, so
                // it swings smoothly through the bends.
                let (a, _) = self.along_sail(d - 12.0);
                let (b, _) = self.along_sail(d + 12.0);
                let dir = if d < 12.0 {
                    self.dir
                } else {
                    (b - a).normalize_or(self.dir)
                };
                ShipPose {
                    at: p,
                    yaw: yaw_of(dir),
                    speed: v,
                }
            }
        };
        (pose, phase)
    }

    /// The tug at game second `t`: at its pontoon, out to the freighter's
    /// stern, pulling it out and round, and home again round the mole's
    /// head.
    pub fn tug_pose(&self, t: f64) -> ShipPose {
        let home = self.tug_berth();
        let (phase, s) = self.phase_at(t);
        let tow = |ship: &ShipPose| {
            let d = ShipKind::Freighter.dims();
            ShipPose {
                at: ship.to_plan(DVec2::new(0.0, -d.len * 0.5 - TOW)),
                yaw: ship.yaw + PI,
                speed: -ship.speed,
            }
        };
        // Its lane, outside the freighter's beam.
        let lane = MOLE_HALF + FENDER + ShipKind::Freighter.dims().beam + 12.0;
        let home_z = self.to_local(home.at).y;
        match phase {
            Phase::Docked if s > DOCKED_S - TUG_OUT_S => {
                let to = tow(&self.freighter_berth());
                let to_z = self.to_local(to.at).y;
                let via = [
                    home.at,
                    self.to_plan(DVec2::new(lane, home_z + 14.0)),
                    self.to_plan(DVec2::new(lane, to_z - 34.0)),
                    to.at,
                ];
                let u = ((s - (DOCKED_S - TUG_OUT_S)) / (TUG_OUT_S - 10.0)).min(1.0);
                along_poly(&via, u, to.yaw)
            }
            Phase::Docked => home,
            Phase::Backing | Phase::Turning => tow(&self.freighter_pose(t).0),
            Phase::Sailing if s < TUG_HOME_S => {
                let start = tow(&ShipPose {
                    yaw: self.yaw(0.0),
                    at: self.turn_point(),
                    speed: 0.0,
                });
                let via = [
                    start.at,
                    self.to_plan(DVec2::new(lane, MOLE_LEN + 30.0)),
                    self.to_plan(DVec2::new(lane, home_z + 30.0)),
                    self.to_plan(DVec2::new(
                        home.at.dot(self.right) - self.root.dot(self.right),
                        home_z + 14.0,
                    )),
                    home.at,
                ];
                // It sets off the way it was facing (back to the shore).
                let mut p = along_poly(&via, s / TUG_HOME_S, home.yaw);
                if s < 12.0 {
                    let k = s / 12.0;
                    p.yaw = start.yaw + wrap(p.yaw - start.yaw) * k;
                }
                p
            }
            Phase::Sailing => home,
        }
    }

    /// The coasters out at sea at game second `t`.
    pub fn passing_at(&self, t: f64) -> Vec<(ShipKind, ShipPose)> {
        self.traffic
            .iter()
            .map(|p| {
                let x = (p.offset + p.speed * t).rem_euclid(2.0 * LANE_SPAN) - LANE_SPAN;
                (
                    p.kind,
                    ShipPose {
                        at: p.mid + p.dir * x,
                        yaw: yaw_of(p.dir),
                        speed: p.speed,
                    },
                )
            })
            .collect()
    }

    /// Every ship that moves, where it is at game second `t`: the
    /// freighter, the tug and the coasters.
    pub fn ships_at(&self, t: f64) -> Vec<(ShipKind, ShipPose)> {
        let mut out = vec![
            (ShipKind::Freighter, self.freighter_pose(t).0),
            (ShipKind::Tug, self.tug_pose(t)),
        ];
        out.extend(self.passing_at(t));
        out
    }
}

/// The point an eased share `u` of the way along the polyline `via`,
/// heading along it, turning to `end_yaw` over the last stretch.
fn along_poly(via: &[DVec2], u: f64, end_yaw: f64) -> ShipPose {
    let u = u.clamp(0.0, 1.0);
    let e = u * u * (3.0 - 2.0 * u);
    let total: f64 = via.windows(2).map(|w| w[0].distance(w[1])).sum();
    let mut left = total * e;
    let n = via.len();
    for (i, w) in via.windows(2).enumerate() {
        let l = w[0].distance(w[1]);
        if left <= l && l > 1e-9 {
            let d = (w[1] - w[0]) / l;
            let mut yaw = yaw_of(d);
            if i + 2 == n {
                // The last stretch: come round to the end heading.
                let k = (left / l).clamp(0.0, 1.0);
                yaw += wrap(end_yaw - yaw) * k * k * (3.0 - 2.0 * k);
            }
            return ShipPose {
                at: w[0] + d * left,
                yaw,
                speed: 2.5,
            };
        }
        left -= l;
    }
    ShipPose {
        at: via[n - 1],
        yaw: end_yaw,
        speed: 0.0,
    }
}

/// An angle brought into -π..π.
fn wrap(a: f64) -> f64 {
    (a + PI).rem_euclid(TAU) - PI
}

/// Local x of the root line's point `i`.
fn root_x(i: usize) -> f64 {
    -MOLE_HALF + 2.0 * MOLE_HALF * i as f64 / (ROOT_SAMPLES - 1) as f64
}

/// Points along a crane's boom (plan, height), every few metres, from its
/// hinge out to its tip.
pub fn boom_points(mid: DVec2, boom: DVec2, deck: f64) -> Vec<(DVec2, f64)> {
    let (s, c) = det::sin_cos(BOOM_LIFT);
    let hinge = mid + boom * BOOM_HINGE.1;
    let y0 = deck + BOOM_HINGE.0;
    let n = (BOOM_LEN / 3.2).ceil() as usize;
    (0..n)
        .map(|k| {
            let d = BOOM_LEN * (k as f64 + 0.5) / n as f64;
            (hinge + boom * (d * c), y0 + d * s)
        })
        .collect()
}

/// The freighter's way out and back in its harbour's frame, from the turn
/// point to the berth, as control points: straight out, a wide loop to
/// one side (`loop_side` +1 the cargo side, -1 the other) and back in
/// along its berth's line.
fn sail_controls(turn: DVec2, berth: DVec2, loop_side: f64) -> Vec<DVec2> {
    let x0 = turn.x;
    let lx = |x: f64| x0 + loop_side * x;
    vec![
        turn,
        DVec2::new(x0, turn.y + 260.0),
        DVec2::new(x0, turn.y + 820.0),
        DVec2::new(lx(170.0), turn.y + 1180.0),
        DVec2::new(lx(480.0), turn.y + 1260.0),
        DVec2::new(lx(730.0), turn.y + 1020.0),
        DVec2::new(lx(680.0), turn.y + 600.0),
        DVec2::new(lx(400.0), turn.y + 280.0),
        DVec2::new(berth.x, berth.y + 440.0),
        DVec2::new(berth.x, berth.y + 220.0),
        berth,
    ]
}

/// The freighter's way through its control points: a smooth curve,
/// relaxed until it bends no tighter than a ship that size can turn, its
/// first and last stretches (the turn point, the straight run in to the
/// berth) kept where they are.
fn sail_path(ctl: &[DVec2]) -> Vec<DVec2> {
    let mut path = smooth_path(ctl, PATH_STEP);
    let n = path.len();
    let keep_end = (240.0 / PATH_STEP) as usize;
    if n < keep_end + 10 {
        return path;
    }
    for _ in 0..400 {
        let prev = path.clone();
        for i in 3..n - keep_end {
            path[i] = prev[i] * 0.5 + (prev[i - 1] + prev[i + 1]) * 0.25;
        }
    }
    // Even spacing again.
    smooth_path(&path, PATH_STEP)
}

/// A Catmull-Rom curve through `pts`, resampled every `step` metres.
fn smooth_path(pts: &[DVec2], step: f64) -> Vec<DVec2> {
    let n = pts.len();
    let get = |i: isize| pts[i.clamp(0, n as isize - 1) as usize];
    let mut dense = Vec::new();
    for s in 0..(n - 1) as isize {
        let (p0, p1, p2, p3) = (get(s - 1), get(s), get(s + 1), get(s + 2));
        let per = ((p1.distance(p2) / 2.0).ceil() as usize).max(2);
        for k in 0..per {
            let t = k as f64 / per as f64;
            let (t2, t3) = (t * t, t * t * t);
            dense.push(
                0.5 * ((2.0 * p1)
                    + (-p0 + p2) * t
                    + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
                    + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3),
            );
        }
    }
    dense.push(pts[n - 1]);
    // Even spacing.
    let mut out = vec![dense[0]];
    let mut carry = 0.0;
    for w in dense.windows(2) {
        let l = w[0].distance(w[1]);
        if l < 1e-9 {
            continue;
        }
        let mut s = step - carry;
        while s <= l {
            out.push(w[0].lerp(w[1], s / l));
            s += step;
        }
        carry = l - (s - step);
    }
    if out.last().is_some_and(|p| p.distance(pts[n - 1]) > 1e-6) {
        out.push(pts[n - 1]);
    }
    out
}

/// Picks the harbour's place: a stretch of open sea shore, as near the
/// town's core as there is room for the mole, the breakwater, the basin
/// and the freighter's way out, away from rivers, bridges and landmarks.
pub fn plan_harbour(w: &TownWorld) -> Option<Harbour> {
    if !w.coastal() {
        return None;
    }
    let core = w.core();
    let land = &w.town.land;
    let reach = (w.half() * 0.8).min(1600.0);
    let lo = core - DVec2::splat(reach + MOLE_LEN);
    let hi = core + DVec2::splat(reach + MOLE_LEN);
    let roads = w.roads_in(lo, hi);
    let grid = RoadGrid::new(&roads, 0.0);
    let bridges = w.bridges_in(lo, hi);
    let sea = |p: DVec2, depth: f64| {
        w.ground(p) < WATER_LEVEL - depth
            && !matches!(land.cover_at(p), Cover::River(_) | Cover::Lake)
            && (w.water_level(p) - WATER_LEVEL).abs() < 1e-6
    };
    // Shore points: land with sea close by, nearest the core first.
    let step = 20.0;
    let n = (reach / step) as i64;
    let mut shore: Vec<(f64, DVec2)> = Vec::new();
    for j in -n..=n {
        for i in -n..=n {
            let p = core + DVec2::new(i as f64, j as f64) * step;
            if p.distance(core) > reach || w.ground(p) < WATER_LEVEL + 0.2 {
                continue;
            }
            let wet = (0..8).any(|k| sea(p + det::dir2(TAU * k as f64 / 8.0) * step, 0.5));
            if wet {
                shore.push((p.distance(core), p));
            }
        }
    }
    shore.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.x.total_cmp(&b.1.x)));
    for &(_, p) in &shore {
        // Out to the open sea: the mean way to wet points round about.
        let out = (0..32)
            .map(|k| det::dir2(TAU * k as f64 / 32.0))
            .filter(|d| sea(p + *d * 90.0, 1.0))
            .sum::<DVec2>();
        if out.length() < 4.0 {
            continue;
        }
        let out = out.normalize();
        for turn in [0.0, 0.2, -0.2, 0.4, -0.4] {
            let (s, c) = det::sin_cos(turn);
            let dir = DVec2::new(out.x * c - out.y * s, out.x * s + out.y * c);
            for side in [1.0, -1.0] {
                if let Some(h) = try_site(w, &grid, &bridges, &sea, p, dir, side) {
                    return Some(h);
                }
            }
        }
    }
    None
}

/// Can the harbour stand off the shore point `p`, its mole out along `dir`,
/// the cargo side to `side`?
fn try_site(
    w: &TownWorld,
    roads: &RoadGrid,
    bridges: &[super::world::Bridge],
    sea: &dyn Fn(DVec2, f64) -> bool,
    p: DVec2,
    dir: DVec2,
    side: f64,
) -> Option<Harbour> {
    // The shoreline along the mole's middle.
    let mut root = p;
    let mut k = 0;
    while w.ground(root) >= WATER_LEVEL && k < 40 {
        root += dir * 1.0;
        k += 1;
    }
    if k >= 40 {
        return None;
    }
    let right = DVec2::new(dir.y, -dir.x) * side;
    let to = |x: f64, z: f64| root + right * x + dir * z;
    // The mole over open water from a little way out.
    for z in (14..=MOLE_LEN as i64).step_by(8) {
        for xi in -5..=5 {
            let q = to(MOLE_HALF * xi as f64 / 5.0, z as f64);
            if !sea(q, if z > 40 { 2.0 } else { 0.4 }) {
                return None;
            }
        }
    }
    // Its root, across its width: back to the first road along the shore,
    // or a few metres onto land where no road comes near.
    let mut root_line = vec![0.0; ROOT_SAMPLES];
    let mut road_y = Vec::new();
    for (i, end) in root_line.iter_mut().enumerate() {
        let x = root_x(i);
        let mut met = None;
        let mut z = 12.0;
        while z > -ROOT_MAX {
            if roads.touches(to(x, z), 0.3, None) {
                met = Some(z);
                break;
            }
            z -= 0.5;
        }
        *end = match met {
            Some(z) => {
                road_y.push(w.road_height(to(x, z - 0.5)));
                z + 0.5
            }
            None => {
                // The shoreline here, then a little way up the beach.
                let mut s = 12.0;
                while s > -ROOT_MAX && w.ground(to(x, s)) < WATER_LEVEL + 0.2 {
                    s -= 0.5;
                }
                (s - 4.0).max(-ROOT_MAX)
            }
        };
        if *end > 8.0 {
            return None;
        }
    }
    // No road over the mole beyond its root.
    for (i, end) in root_line.iter().enumerate() {
        let mut z = end + 2.5;
        while z <= MOLE_LEN {
            if roads.touches(to(root_x(i), z), 0.6, None) {
                return None;
            }
            z += 3.0;
        }
    }
    // The deck: level with the road it meets (the ground at its land end
    // where no road does), never under a high tide's splash.
    let deck = if road_y.is_empty() {
        root_line
            .iter()
            .enumerate()
            .map(|(i, z)| w.ground(to(root_x(i), *z)))
            .fold(f64::MIN, f64::max)
            + 0.05
    } else {
        let hi = road_y.iter().copied().fold(f64::MIN, f64::max);
        let lo = road_y.iter().copied().fold(f64::MAX, f64::min);
        if hi - lo > 0.6 {
            return None;
        }
        hi
    }
    .max(WATER_LEVEL + 1.4);
    if deck > WATER_LEVEL + 4.5 {
        return None;
    }
    // Nothing of the land under the root stands above the deck.
    for (i, end) in root_line.iter().enumerate() {
        let mut z = *end;
        while z < 14.0 {
            if w.ground(to(root_x(i), z)) > deck + 0.25 {
                return None;
            }
            z += 1.0;
        }
    }
    let mut h = Harbour {
        root,
        dir,
        right,
        deck,
        root_line,
        sail: Vec::new(),
        traffic: Vec::new(),
    };
    // The breakwater over open water, its rubble too.
    for wnd in BREAKWATER.windows(2) {
        let (a, b) = (DVec2::from(wnd[0]), DVec2::from(wnd[1]));
        let n = (a.distance(b) / 6.0).ceil() as usize;
        let d = (b - a).normalize();
        for i in 0..=n {
            let c = a.lerp(b, i as f64 / n as f64);
            for s in [-1.0, 0.0, 1.0] {
                let q = c + d.perp() * s * (CROWN_HALF + 14.0);
                if !sea(to(q.x, q.y), 3.0) {
                    return None;
                }
            }
        }
    }
    // The basins: the liner's side and the cargo basin, open water.
    let liner = ShipKind::Liner.dims();
    for z in (30..=210).step_by(10) {
        for x in [
            -(MOLE_HALF + 4.0),
            -(MOLE_HALF + FENDER + liner.beam + 8.0),
            MOLE_HALF + 6.0,
            MOLE_HALF + 60.0,
            MOLE_HALF + 90.0,
        ] {
            if !sea(to(x, z as f64), if z < 60 { 1.2 } else { 2.5 }) {
                return None;
            }
        }
    }
    // Away from the landmarks and the bridges.
    // (A road along the shore that the mole meets may run over the water
    // in places: those bridges only count out beyond the root.)
    let near_thing = |q: DVec2, z: f64, r: f64| {
        w.landmarks.iter().any(|l| l.covers(q, r + 25.0))
            || (z >= 20.0
                && bridges
                    .iter()
                    .any(|b| segment_distance(q, b.a, b.b) < b.half + r + 10.0))
    };
    for z in (-16..=MOLE_LEN as i64 + 10).step_by(12) {
        for x in [-MOLE_HALF - 6.0, 0.0, MOLE_HALF + 6.0, 140.0] {
            if near_thing(to(x, z as f64), z as f64, 6.0) {
                return None;
            }
        }
    }
    // The freighter's way out and back: open, deep water all along, with
    // room on both sides for its beam; the turn's circle too.
    let berth = h.to_local(h.freighter_berth().at);
    let turn = DVec2::new(berth.x, berth.y + BACK_OUT);
    // The tug swings round with it, out behind the stern.
    let half_len = ShipKind::Freighter.dims().len * 0.5 + TOW + 8.0;
    for k in 0..24 {
        let a = TAU * k as f64 / 24.0;
        let q = turn + det::dir2(a) * half_len;
        if !sea(to(q.x, q.y), 3.0) {
            return None;
        }
    }
    let mut ok_loop = None;
    for loop_side in [1.0, -1.0] {
        let ctl = sail_controls(turn, berth, loop_side);
        let path: Vec<DVec2> = sail_path(&ctl);
        let clear = path.iter().step_by(5).all(|q| {
            let n = (q.x - berth.x).abs() < 1.0 && q.y < MOLE_LEN + 60.0;
            // The last stretch in along the berth is checked with the basin.
            n || [-12.0, 0.0, 12.0].iter().all(|s| {
                let pp = to(q.x + s, q.y);
                sea(pp, 4.0)
            })
        });
        if clear {
            ok_loop = Some(path.iter().map(|q| to(q.x, q.y)).collect::<Vec<_>>());
            break;
        }
    }
    h.sail = ok_loop?;
    // Coasters out at sea: lanes along the coast, kept where the sea is
    // open from end to end of what can be seen.
    let along = right;
    let mut traffic = Vec::new();
    for (k, (z, kind, speed)) in [
        (1700.0, ShipKind::Freighter, 5.5),
        (2600.0, ShipKind::Liner, 7.5),
        (3600.0, ShipKind::Freighter, 6.0),
    ]
    .into_iter()
    .enumerate()
    {
        let mid = to(0.0, z);
        let open = (-25..=25).all(|i| sea(mid + along * (i as f64 * 200.0), 5.0));
        if open && traffic.len() < 2 {
            let d = if k % 2 == 0 { along } else { -along };
            traffic.push(Passing {
                kind,
                mid,
                dir: d,
                speed,
                offset: LANE_SPAN * (0.3 + 0.5 * k as f64),
            });
        }
    }
    h.traffic = traffic;
    Some(h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_harbour() -> Harbour {
        let mut h = Harbour {
            root: DVec2::new(10.0, -20.0),
            dir: DVec2::new(0.6, 0.8),
            right: DVec2::new(0.8, -0.6),
            deck: 1.8,
            root_line: vec![-6.0; ROOT_SAMPLES],
            sail: Vec::new(),
            traffic: vec![Passing {
                kind: ShipKind::Freighter,
                mid: DVec2::new(0.0, 3000.0),
                dir: DVec2::X,
                speed: 6.0,
                offset: 100.0,
            }],
        };
        let berth = h.to_local(h.freighter_berth().at);
        let turn = DVec2::new(berth.x, berth.y + BACK_OUT);
        h.sail = sail_path(&sail_controls(turn, berth, 1.0))
            .iter()
            .map(|q| h.to_plan(*q))
            .collect();
        h
    }

    #[test]
    fn the_mole_is_raised_ground_and_covers_its_outline() {
        let h = test_harbour();
        assert_eq!(h.deck_at(h.to_plan(DVec2::new(0.0, 100.0))), Some(1.8));
        assert_eq!(h.deck_at(h.to_plan(DVec2::new(0.0, -5.0))), Some(1.8));
        assert_eq!(h.deck_at(h.to_plan(DVec2::new(0.0, -7.0))), None);
        assert_eq!(
            h.deck_at(h.to_plan(DVec2::new(MOLE_HALF - 1.0, MOLE_LEN - 1.0))),
            None
        );
        assert!(h.covers(h.to_plan(DVec2::new(0.0, 50.0)), 0.0));
        assert!(!h.covers(h.to_plan(DVec2::new(80.0, 50.0)), 0.0));
        let crown = h.to_plan(DVec2::from(BREAKWATER[1]));
        assert_eq!(h.deck_at(crown), Some(h.crown()));
    }

    #[test]
    fn the_freighter_goes_round_and_comes_back_smoothly() {
        let h = test_harbour();
        let cycle = h.cycle();
        assert!(cycle > DOCKED_S + 300.0 && cycle < 3600.0, "cycle {cycle}");
        let berth = h.freighter_berth();
        let (p0, ph) = h.freighter_pose(1.0);
        assert_eq!(ph, Phase::Docked);
        assert!(p0.at.distance(berth.at) < 1e-9);
        // No jumps anywhere in the round, and back where it began.
        let mut last = h.freighter_pose(0.0).0;
        let mut t = 0.0;
        while t < cycle + 5.0 {
            t += 0.5;
            let (p, _) = h.freighter_pose(t);
            let moved = p.at.distance(last.at);
            assert!(moved < SEA_SPEED * 0.5 + 0.2, "jump of {moved} m at {t}");
            let turned = (p.yaw - last.yaw + PI).rem_euclid(TAU) - PI;
            assert!(turned.abs() < 0.05, "turned {turned} at {t}");
            last = p;
        }
        // The tug: no jumps either.
        let mut last = h.tug_pose(0.0);
        let mut t = 0.0;
        while t < cycle {
            t += 0.5;
            let p = h.tug_pose(t);
            assert!(p.at.distance(last.at) < 3.0, "tug jumps at {t}");
            last = p;
        }
    }

    #[test]
    fn ship_solids_lie_inside_the_hull() {
        for kind in [
            ShipKind::Freighter,
            ShipKind::Liner,
            ShipKind::Tug,
            ShipKind::Launch,
        ] {
            let d = kind.dims();
            let pose = ShipPose {
                at: DVec2::new(5.0, 7.0),
                yaw: 0.7,
                speed: 0.0,
            };
            let s = ship_solids(kind, &pose, 0.0);
            assert!(!s.is_empty());
            for f in &s {
                if let Footprint::Box { half, .. } = f {
                    assert!(half.x <= TILE * 0.5 + 1e-9 && half.y <= TILE * 0.5 + 1e-9);
                }
                assert!(f.at().distance(pose.at) < d.len * 0.5 + 1.0, "{kind:?}");
            }
            // The middle of the deck is solid, the water beside it is not.
            let deck = pose.to_plan(DVec2::ZERO);
            assert!(s.iter().any(|f| f.distance(deck) < 1e-9));
            let beside = pose.to_plan(DVec2::new(d.beam * 0.5 + 1.0, 0.0));
            assert!(s.iter().all(|f| f.distance(beside) > 0.3), "{kind:?}");
        }
    }

    #[test]
    fn coasters_wrap_round_out_of_sight() {
        let h = test_harbour();
        let a = h.passing_at(0.0)[0].1.at;
        let b = h.passing_at(10.0)[0].1.at;
        assert!((a.distance(b) - 60.0).abs() < 1e-6);
    }
}
