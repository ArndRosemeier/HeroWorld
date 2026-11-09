//! The town's landmarks: the Aether Spire on its civic plaza near the old
//! core, Hero Hall (home of the town's own heroes) not far from it, a
//! lighthouse on the shore of a seaside town, the mad scientist's Tesla
//! tower in his quarter, and the rocket port at the town's edge.
//!
//! Each stands on a round paved plinth cleared of buildings, in a spot no
//! road crosses. The plinth's top is level; where the ground falls away its
//! side shows as a stone step.

use super::footprint::{block, disc, Footprint};
use super::growth::CentreKind;
use super::land::Cover;
use super::roadgrid::RoadGrid;
use super::world::{TownWorld, WATER_LEVEL};
use crate::building::BuildingSite;
use glam::DVec2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LandmarkKind {
    /// A golden-age tower crowned with Aether light, on the civic plaza.
    AetherSpire,
    /// A striped lighthouse on the shore.
    Lighthouse,
    /// The mad scientist's tower: an iron lattice under a crackling dome,
    /// in the middle of his quarter.
    TeslaTower,
    /// Hero Hall: a round glass hall under a flying-saucer roof and two
    /// sweeping arches, where the town's own heroes meet newcomers.
    HeroHall,
    /// The airship mooring mast on its airfield at the town's edge, where
    /// the liner docks ([`super::mast`]).
    MooringMast,
    /// The rocket port at the town's edge: a finned rocket beside its
    /// gantry, that now and then goes up and comes back (see
    /// [`super::gantry`]).
    RocketGantry,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Landmark {
    pub kind: LandmarkKind,
    pub pos: DVec2,
    /// Radius of the paved plinth.
    pub plaza: f64,
    /// Top of the plinth.
    pub base: f64,
    /// Lowest ground under the plinth's edge.
    pub low: f64,
    /// Height of the structure above the plinth.
    pub height: f64,
    /// The way it faces (the spire's entrance, the lighthouse's door).
    pub dir: DVec2,
    /// Flights of steps: up the plinth where its side stands higher than a
    /// kerb, and up the spire's base to its doors.
    pub stairs: Vec<Stair>,
}

/// A straight flight of steps, as solid stone under every tread.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stair {
    /// Middle of its top edge, and the way down (unit).
    pub top: DVec2,
    pub down: DVec2,
    pub width: f64,
    /// Height of the landing at the top, and of the ground at the foot.
    pub from: f64,
    pub to: f64,
    /// Treads (the last one is the ground) and the depth of each.
    pub steps: u32,
    pub run: f64,
}

impl Stair {
    /// The flight from `top` going `down`, stepping from `from` to `to` at
    /// most [`RISE`] a step.
    fn new(top: DVec2, down: DVec2, width: f64, from: f64, to: f64) -> Self {
        Stair {
            top,
            down,
            width,
            from,
            to,
            steps: ((from - to) / RISE).ceil().max(1.0) as u32,
            run: RUN,
        }
    }

    /// How far it reaches out from its top edge.
    pub fn length(&self) -> f64 {
        (self.steps - 1) as f64 * self.run
    }

    /// One solid block per tread above the ground, each from the foot of
    /// the flight up to the tread (and back to the top edge).
    pub fn treads(&self) -> Vec<Footprint> {
        let rise = (self.from - self.to) / self.steps as f64;
        (1..self.steps)
            .map(|k| {
                let reach = k as f64 * self.run;
                Footprint::Box {
                    at: self.top + self.down * (reach * 0.5),
                    dir: self.down,
                    half: DVec2::new(self.width * 0.5, reach * 0.5),
                    y0: self.to - 0.6,
                    y1: self.from - k as f64 * rise,
                }
            })
            .collect()
    }
}

/// Steps in a landmark's stairs: rise and depth (m).
const RISE: f64 = 0.17;
const RUN: f64 = 0.32;
/// The spire's base: its stepped octagonal tiers (radius to the corners,
/// height), the shaft's radius, and its four flights up to the doors.
pub const SPIRE_TIERS: [(f64, f64); 3] = [(5.6, 0.9), (4.7, 0.9), (3.9, 1.2)];
pub const SPIRE_SHAFT: f64 = 3.0;
const SPIRE_STEPS: u32 = 16;
const SPIRE_RUN: f64 = 0.3;
/// The middle of an octagon's side, per metre to its corners.
const APOTHEM: f64 = 0.924;
/// Hero Hall: the glass drum's radius and height, the saucer roof's
/// radius and top, where its two arches stand (their feet, local x and z),
/// and the entrance canopy's half width, reach and height.
pub const HALL_R: f64 = 9.0;
pub const HALL_WALL: f64 = 6.0;
pub const HALL_ROOF_R: f64 = 12.5;
pub const HALL_ROOF_TOP: f64 = 9.4;
pub const HALL_ARCH_FEET: [(f64, f64); 4] =
    [(-14.5, 8.0), (14.5, 8.0), (-14.5, -8.0), (14.5, -8.0)];
pub const HALL_CANOPY: (f64, f64, f64) = (4.5, 4.2, 4.0);

/// The Tesla tower's lab: a square blockhouse of this half width, its roof
/// at this height.
pub const TESLA_LAB: f64 = 6.5;
pub const TESLA_LAB_ROOF: f64 = 5.65;
/// Lightning rods on the lab roof's four corners, where the dome's arcs
/// strike: their distance out along each axis, and their tips' height.
pub const TESLA_ROD_AT: f64 = 6.0;
pub const TESLA_ROD_TIP: f64 = 13.0;
/// The rods' radius (with the clamps round them).
pub const TESLA_ROD_R: f64 = 0.3;

/// Where the Tesla tower's rods stand, in its own frame (+z its front).
pub fn tesla_rods() -> [DVec2; 4] {
    [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)]
        .map(|(x, z)| DVec2::new(x, z) * TESLA_ROD_AT)
}

impl Landmark {
    /// From the landmark's own frame (x right, z the way it faces) to the
    /// town's plan.
    pub fn to_plan(&self, l: DVec2) -> DVec2 {
        let x = DVec2::new(self.dir.y, -self.dir.x);
        self.pos + x * l.x + self.dir * l.y
    }

    /// Where the founding hero's statue stands on the spire's plaza, out
    /// along its axis (local z), clear of the steps up to the doors.
    pub fn statue_z(&self) -> f64 {
        let foot = SPIRE_TIERS[2].0 * APOTHEM + (SPIRE_STEPS - 1) as f64 * SPIRE_RUN;
        (self.plaza * 0.62).max(foot + 1.2).min(self.plaza - 1.1)
    }

    /// Its solid parts above the plinth, in the town's frame. The plinth
    /// itself is raised ground (see [`Self::on_plinth`]); the stairs are
    /// in [`Self::stairs`].
    pub fn footprints(&self) -> Vec<Footprint> {
        let local = match self.kind {
            LandmarkKind::AetherSpire => {
                let mut v = Vec::new();
                let mut y = 0.0;
                // Octagonal tiers: a disc a little inside the corners.
                for (r, t) in SPIRE_TIERS {
                    y += t;
                    v.push(disc(r * 0.95, y));
                }
                v.push(Footprint::Disc {
                    at: DVec2::ZERO,
                    r: SPIRE_SHAFT,
                    y0: 0.0,
                    y1: self.height * 0.82,
                });
                v.push(Footprint::Box {
                    at: DVec2::new(0.0, self.statue_z()),
                    dir: DVec2::Y,
                    half: DVec2::new(0.98, 0.98),
                    y0: 0.0,
                    y1: 4.2,
                });
                v
            }
            LandmarkKind::Lighthouse => vec![
                disc(4.6, 3.7),
                Footprint::Disc {
                    at: DVec2::ZERO,
                    r: 2.6,
                    y0: 3.7,
                    y1: self.height - 3.0,
                },
            ],
            LandmarkKind::TeslaTower => {
                let mut v = vec![block(TESLA_LAB, TESLA_LAB, TESLA_LAB_ROOF)];
                for at in tesla_rods() {
                    v.push(Footprint::Disc {
                        at,
                        r: TESLA_ROD_R,
                        y0: TESLA_LAB_ROOF,
                        y1: TESLA_ROD_TIP,
                    });
                }
                v
            }
            LandmarkKind::RocketGantry => super::gantry::footprints(self.plaza),
            LandmarkKind::HeroHall => {
                let mut v = vec![
                    disc(HALL_R, HALL_WALL + 0.4),
                    Footprint::Disc {
                        at: DVec2::ZERO,
                        r: HALL_ROOF_R,
                        y0: HALL_WALL,
                        y1: HALL_ROOF_TOP,
                    },
                ];
                // The arches' feet and the canopy's two columns.
                for (x, z) in HALL_ARCH_FEET {
                    v.push(Footprint::Disc {
                        at: DVec2::new(x, z),
                        r: 0.7,
                        y0: 0.0,
                        y1: 3.0,
                    });
                }
                let (hw, reach, h) = HALL_CANOPY;
                for x in [-hw + 0.4, hw - 0.4] {
                    v.push(Footprint::Disc {
                        at: DVec2::new(x, HALL_R + reach - 0.5),
                        r: 0.2,
                        y0: 0.0,
                        y1: h,
                    });
                }
                v.push(Footprint::Box {
                    at: DVec2::new(0.0, HALL_R + reach * 0.5),
                    dir: DVec2::Y,
                    half: DVec2::new(hw, reach * 0.5 + 0.3),
                    y0: h,
                    y1: h + 0.35,
                });
                v
            }
            LandmarkKind::MooringMast => super::mast::mast_footprints(),
        };
        local
            .into_iter()
            .map(|f| f.placed(self.pos, self.dir, self.base))
            .collect()
    }

    /// Where Hero Hall's heroes stand to greet newcomers (local frame: in a
    /// row on the plaza in front of the canopy, facing out), each with the
    /// way they face, in the town's frame. Empty for other landmarks.
    pub fn hero_spots(&self) -> Vec<(DVec2, DVec2)> {
        if self.kind != LandmarkKind::HeroHall {
            return Vec::new();
        }
        let z = (HALL_R + HALL_CANOPY.1 + 3.0).min(self.plaza - 1.5);
        [-6.0, -3.0, 0.0, 3.0, 6.0]
            .iter()
            .map(|&x| {
                let curve = (x / 6.0f64).powi(2) * 1.2;
                (self.to_plan(DVec2::new(x, z - curve)), self.dir)
            })
            .collect()
    }

    /// The plinth's top where `p` is on it.
    pub fn on_plinth(&self, p: DVec2) -> Option<f64> {
        (p.distance(self.pos) <= self.plaza).then_some(self.base)
    }

    /// Does a circle of radius `r` at `p` reach onto the plinth or its
    /// stairs?
    pub fn covers(&self, p: DVec2, r: f64) -> bool {
        p.distance(self.pos) < self.plaza + r
            || self
                .stairs
                .iter()
                .flat_map(|s| s.treads())
                .any(|t| t.distance(p) < r)
    }

    /// Does a building's footprint come within `margin` of the plinth or
    /// its stairs?
    pub fn overlaps(&self, site: &BuildingSite, margin: f64) -> bool {
        let d = self.pos - site.origin;
        let (x, z) = (d.dot(site.along), d.dot(site.inward));
        let q = DVec2::new(x.clamp(0.0, site.size.x), z.clamp(0.0, site.size.y));
        if DVec2::new(x, z).distance(q) < self.plaza + margin {
            return true;
        }
        let b = Footprint::Box {
            at: site.origin + site.along * site.size.x * 0.5 + site.inward * site.size.y * 0.5,
            dir: site.inward,
            half: site.size * 0.5,
            y0: 0.0,
            y1: 1.0,
        };
        self.stairs.iter().any(|s| {
            let foot = s.top + s.down * s.length();
            [s.top, foot, (s.top + foot) * 0.5]
                .iter()
                .any(|p| b.distance(*p) < s.width * 0.5 + margin)
        })
    }
}

/// The landmark's stairs: up the plinth on each of its four sides where
/// it stands higher than a step (unless the flight would run onto a
/// street or into the water), and the spire's four flights to its doors.
pub(super) fn plan_stairs(w: &TownWorld, roads: &RoadGrid, l: &Landmark) -> Vec<Stair> {
    let mut out = Vec::new();
    let quarter = |k: usize| {
        let d = hw_core::det::dir2(std::f64::consts::FRAC_PI_2 * k as f64);
        l.to_plan(d) - l.pos
    };
    for k in 0..4 {
        let d = quarter(k);
        let top = l.pos + d * (l.plaza - 0.02);
        if l.base - w.ground(top + d * 0.3) <= 0.4 {
            continue;
        }
        // The foot lands lower down the slope: settle the flight's length.
        let mut st = Stair::new(top, d, 4.0, l.base, w.ground(top + d * 0.3));
        for _ in 0..3 {
            let g = w.ground(top + d * (st.length() + 0.3));
            st = Stair::new(top, d, 4.0, l.base, g);
        }
        let foot = top + d * st.length();
        if roads.touches(foot, 0.3, None) || w.ground(foot) < WATER_LEVEL + 0.3 {
            continue;
        }
        out.push(st);
    }
    if l.kind == LandmarkKind::AetherSpire {
        let up = SPIRE_TIERS.iter().map(|t| t.1).sum::<f64>();
        for k in 0..4 {
            let d = quarter(k);
            out.push(Stair {
                top: l.pos + d * (SPIRE_TIERS[2].0 * APOTHEM),
                down: d,
                width: 2.4,
                from: l.base + up,
                to: l.base,
                steps: SPIRE_STEPS,
                run: SPIRE_RUN,
            });
        }
    }
    out
}

/// Picks the landmarks' places (buildings are cleared off them afterwards).
pub fn plan_landmarks(w: &TownWorld) -> Vec<Landmark> {
    let core = w
        .town
        .growth
        .centres
        .iter()
        .find(|c| c.kind == CentreKind::Core)
        .map_or(DVec2::ZERO, |c| c.pos);
    // Only the streets where landmarks may stand: round the core, the
    // villains' quarter and, by the sea, out to the lighthouse.
    let mut lo = core - DVec2::splat(450.0);
    let mut hi = core + DVec2::splat(450.0);
    if w.coastal() {
        let r = (w.half() * 0.8).min(1600.0) + 20.0;
        lo = lo.min(core - DVec2::splat(r));
        hi = hi.max(core + DVec2::splat(r));
    }
    if let Some(d) = &w.town.dystopia {
        let r = (d.radius * 0.7).min(500.0) + 20.0;
        lo = lo.min(d.centre - DVec2::splat(r));
        hi = hi.max(d.centre + DVec2::splat(r));
    }
    let all = w.roads_in(lo, hi);
    let roads = RoadGrid::new(&all, 0.0);
    let mut out = Vec::new();
    // Taller in bigger towns: 35 m in a market town, 90 m in a metropolis.
    let pop = w.town.params.population.max(1000.0);
    let spire_h = (25.0 * (hw_core::det::log10(pop) - 2.3)).clamp(30.0, 95.0);
    for plaza in [26.0, 22.0, 17.0, 14.0, 11.0] {
        if let Some(l) = best_spot(w, &roads, core, 420.0, 6.0, plaza, 3.5, |p| {
            -p.distance(core)
        }) {
            out.push(Landmark {
                kind: LandmarkKind::AetherSpire,
                height: spire_h,
                dir: (core - l.0).normalize_or(DVec2::Y),
                ..landmark_at(w, l.0, plaza)
            });
            break;
        }
    }
    // Hero Hall: near the core too, but on its own plaza, facing the spire.
    if let Some(spire) = out.first().cloned() {
        for plaza in [24.0, 21.0, 19.0] {
            let apart = spire.plaza + plaza + 24.0;
            let spot = best_spot(w, &roads, core, 600.0, 6.0, plaza, 3.0, |p| {
                if p.distance(spire.pos) < apart {
                    f64::NEG_INFINITY
                } else {
                    -p.distance(core) - (p.distance(spire.pos) - apart) * 0.5
                }
            });
            if let Some((p, _)) = spot {
                out.push(Landmark {
                    kind: LandmarkKind::HeroHall,
                    height: HALL_ROOF_TOP,
                    dir: (spire.pos - p).normalize_or(DVec2::Y),
                    ..landmark_at(w, p, plaza)
                });
                break;
            }
        }
    }
    if let Some(d) = &w.town.dystopia {
        // As near the quarter's middle as a clear spot allows, inside it.
        let reach = (d.radius * 0.7).min(500.0);
        let tall = (18.0 * (hw_core::det::log10(pop) - 1.0)).clamp(40.0, 75.0);
        for plaza in [15.0, 12.0, 9.0] {
            let spot = best_spot(w, &roads, d.centre, reach, 6.0, plaza, 4.0, |p| {
                if d.contains(p) {
                    -p.distance(d.centre)
                } else {
                    f64::NEG_INFINITY
                }
            });
            if let Some((p, _)) = spot {
                out.push(Landmark {
                    kind: LandmarkKind::TeslaTower,
                    height: tall,
                    dir: (core - p).normalize_or(DVec2::Y),
                    ..landmark_at(w, p, plaza)
                });
                break;
            }
        }
    }
    if w.coastal() {
        let land = &w.town.land;
        // On the shore, where the sea opens widest, not far from the town.
        let reach = (w.half() * 0.8).min(1600.0);
        let open = |p: DVec2| {
            (0..24)
                .filter(|k| {
                    let a = std::f64::consts::TAU * *k as f64 / 24.0;
                    land.cover_at(p + hw_core::det::dir2(a) * 70.0) == Cover::Sea
                })
                .count() as f64
                / 24.0
        };
        let near_sea = |p: DVec2| {
            (0..12).any(|k| {
                let a = std::f64::consts::TAU * k as f64 / 12.0;
                land.cover_at(p + hw_core::det::dir2(a) * 16.0) == Cover::Sea
            })
        };
        let plaza = 7.5;
        let spot = best_spot(w, &roads, core, reach, 8.0, plaza, 2.5, |p| {
            if !near_sea(p) {
                return f64::NEG_INFINITY;
            }
            open(p) - p.distance(core) / reach * 0.35
        });
        if let Some((p, _)) = spot {
            let sea = (0..24)
                .map(|k| hw_core::det::dir2(std::f64::consts::TAU * k as f64 / 24.0))
                .filter(|d| land.cover_at(p + *d * 40.0) == Cover::Sea)
                .sum::<DVec2>();
            out.push(Landmark {
                kind: LandmarkKind::Lighthouse,
                height: 30.0,
                // The door faces the land.
                dir: (-sea).normalize_or(DVec2::Y),
                ..landmark_at(w, p, plaza)
            });
        }
    }
    if let Some(m) = super::mast::plan_mast(w, &out) {
        out.push(m);
    }
    for l in &mut out {
        l.stairs = plan_stairs(w, &roads, l);
    }
    // The rocket port, out past the town's edge (its stairs are its own:
    // these roads stop short of it).
    if let Some(g) = super::gantry::plan_gantry(w, &out) {
        out.push(g);
    }
    out
}

pub(super) fn landmark_at(w: &TownWorld, p: DVec2, plaza: f64) -> Landmark {
    let (lo, hi) = rim(w, p, plaza);
    Landmark {
        kind: LandmarkKind::AetherSpire,
        pos: p,
        plaza,
        base: hi + 0.3,
        low: lo,
        height: 0.0,
        dir: DVec2::Y,
        stairs: Vec::new(),
    }
}

/// Lowest and highest ground over a disc.
pub(super) fn rim(w: &TownWorld, p: DVec2, r: f64) -> (f64, f64) {
    let mut lo = w.ground(p);
    let mut hi = lo;
    for ring in [0.5, 1.0] {
        for k in 0..16 {
            let a = std::f64::consts::TAU * k as f64 / 16.0;
            let g = w.ground(p + hw_core::det::dir2(a) * r * ring);
            lo = lo.min(g);
            hi = hi.max(g);
        }
    }
    (lo, hi)
}

/// The best scoring spot within `reach` of `centre` (on a `step` grid)
/// where a plinth of radius `plaza` stands on dry, fairly level ground clear
/// of every road.
#[allow(clippy::too_many_arguments)]
pub(super) fn best_spot(
    w: &TownWorld,
    roads: &RoadGrid,
    centre: DVec2,
    reach: f64,
    step: f64,
    plaza: f64,
    slope: f64,
    score: impl Fn(DVec2) -> f64,
) -> Option<(DVec2, f64)> {
    let n = (reach / step) as i64;
    let mut best: Option<(DVec2, f64)> = None;
    for j in -n..=n {
        for i in -n..=n {
            let p = centre + DVec2::new(i as f64, j as f64) * step;
            if p.distance(centre) > reach || p.x.abs() > w.half() || p.y.abs() > w.half() {
                continue;
            }
            let s = score(p);
            if !s.is_finite() || best.is_some_and(|b| s <= b.1) {
                continue;
            }
            let (lo, hi) = rim(w, p, plaza);
            if lo < WATER_LEVEL + 1.2 || hi - lo > slope || roads.touches(p, plaza + 0.8, None) {
                continue;
            }
            best = Some((p, s));
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::urban::params::TownParams;
    use hw_core::Seed;

    #[test]
    fn spire_stands_near_the_core_and_lighthouse_by_the_sea() {
        for s in [1u64, 3, 7] {
            let w = TownWorld::generate(Seed(s), TownParams::sample(Seed(s), 40_000.0));
            let spire = w
                .landmarks
                .iter()
                .find(|l| l.kind == LandmarkKind::AetherSpire)
                .expect("a spire");
            assert!(spire.height >= 30.0);
            let all = w.everything();
            for (_, b) in &all.buildings {
                for l in &w.landmarks {
                    assert!(!l.overlaps(&b.site, 0.5), "a building on the {:?}", l.kind);
                }
            }
            for (_, p) in &all.props {
                for l in &w.landmarks {
                    assert!(!l.covers(p.pos, 0.0), "{:?} on the {:?}", p.kind, l.kind);
                }
            }
            let lh = w
                .landmarks
                .iter()
                .any(|l| l.kind == LandmarkKind::Lighthouse);
            assert_eq!(lh, w.coastal(), "seed {s}: lighthouse {lh}");
            let hall = w
                .landmarks
                .iter()
                .find(|l| l.kind == LandmarkKind::HeroHall)
                .expect("Hero Hall");
            assert!(hall.pos.distance(spire.pos) > spire.plaza + hall.plaza + 20.0);
            assert_eq!(hall.hero_spots().len(), 5);
            for (p, _) in hall.hero_spots() {
                assert!(
                    hall.on_plinth(p).is_some(),
                    "seed {s}: a hero off the plaza"
                );
                for f in hall.footprints() {
                    assert!(f.distance(p) > 0.5, "seed {s}: a hero inside the hall");
                }
            }
            let d = w.town.dystopia.as_ref().expect("a villain quarter");
            let tesla = w
                .landmarks
                .iter()
                .find(|l| l.kind == LandmarkKind::TeslaTower)
                .expect("a Tesla tower");
            assert!(
                d.contains(tesla.pos),
                "seed {s}: the tower outside the quarter"
            );
            // The rocket port stands out past the town's edge, off every
            // street, its pad clear of buildings (checked above).
            let pad = w
                .landmarks
                .iter()
                .find(|l| l.kind == LandmarkKind::RocketGantry)
                .expect("a rocket port");
            assert!(!w.in_a_piece(pad.pos), "seed {s}: the pad in town");
            assert!(
                pad.pos.distance(w.core()) > 300.0,
                "seed {s}: too near the core"
            );
            assert!(pad.plaza >= 21.0 && pad.height >= 40.0);
            // The lightning rods the arcs strike are solid, up on the
            // lab's roof inside its walls.
            let fps = tesla.footprints();
            for at in tesla_rods() {
                let p = tesla.to_plan(at);
                assert!(
                    fps.iter().any(|f| matches!(f, Footprint::Disc { .. })
                        && f.distance(p) < 0.01
                        && f.span().1 >= tesla.base + TESLA_ROD_TIP - 0.01),
                    "seed {s}: a rod nothing stops"
                );
                assert!(at.x.abs() < TESLA_LAB && at.y.abs() < TESLA_LAB);
            }
        }
    }
}
