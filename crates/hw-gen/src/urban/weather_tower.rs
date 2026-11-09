//! The Weather Bureau's tower: a slender white Googie mast on a round glass
//! pavilion, three swept fins at its foot, an observation saucer halfway
//! up, and at the top a crown of cloud cannons round a glass weather globe.
//! It makes its own weather over the blocks in front of it: a cloud, rain,
//! and a rainbow when the sun comes out again (the when is
//! `hw_sim::rainmaker`, the look `hw_client::weatherview`).
//!
//! It stands on high ground a few hundred metres out from the core, clear
//! of the villains' quarter and the other landmarks, so its rain falls on
//! ordinary streets.

use super::footprint::Footprint;
use super::landmarks::{Landmark, LandmarkKind};
use super::roadgrid::RoadGrid;
use super::world::TownWorld;
use glam::DVec2;

/// The round pavilion at the foot: radius, top of its walls, and its
/// overhanging roof's radius and top.
pub const WX_HALL_R: f64 = 8.0;
pub const WX_HALL_WALL: f64 = 5.2;
pub const WX_ROOF_R: f64 = 9.4;
pub const WX_ROOF_TOP: f64 = 5.8;
/// The fins: how far out their feet stand, and how thick they are.
pub const WX_FIN_FOOT: f64 = 12.5;
pub const WX_FIN_T: f64 = 0.5;
/// The fins' directions (degrees from the front, clockwise from above).
pub const WX_FIN_AT: [f64; 3] = [60.0, 180.0, 300.0];
/// The cloud cannons round the crown.
pub const WX_CANNONS: usize = 6;
/// How far the rain reaches from the middle of the shower (m).
pub const WX_RAIN_R: f64 = 58.0;
/// How far in front of the tower the shower falls (m, local +z).
pub const WX_RAIN_AHEAD: f64 = 40.0;
/// The cloud's underside, above the plinth: this much over the top.
pub const WX_CLOUD_OVER: f64 = 24.0;
/// How far from the core it may stand (m).
pub const WX_REACH: f64 = 900.0;
/// Towns smaller than this have no weather bureau.
pub const WX_MIN_POP: f64 = 5_000.0;

/// The heights and sizes of the tower, from its height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WxDims {
    pub h: f64,
    /// The shaft's radius at the roof, at the deck and at the crown.
    pub shaft_r: (f64, f64, f64),
    /// Middle of the observation saucer, its radius, and how far it
    /// reaches below and above its middle.
    pub deck_y: f64,
    pub deck_r: f64,
    pub deck_span: (f64, f64),
    /// Where the fins meet the shaft.
    pub fin_top: f64,
    /// The crown's ring, where the cannons sit, its radius, and how far
    /// out (and up) the cannons' mouths reach.
    pub crown_y: f64,
    pub crown_r: f64,
    pub cannon_reach: f64,
    /// The weather globe's middle and radius.
    pub globe_y: f64,
    pub globe_r: f64,
    /// The anemometer's hub height and arm length.
    pub vane_y: f64,
    pub vane_arm: f64,
}

impl WxDims {
    pub fn of(h: f64) -> Self {
        let globe_r = 1.7;
        WxDims {
            h,
            shaft_r: (2.1, 1.35, 0.9),
            deck_y: h * 0.56,
            deck_r: 6.6,
            deck_span: (1.1, 1.9),
            fin_top: h * 0.4,
            crown_y: h * 0.84,
            crown_r: 3.0,
            cannon_reach: 2.6,
            globe_y: h - globe_r - 1.6,
            globe_r,
            vane_y: h * 0.84 + 3.6,
            vane_arm: 2.0,
        }
    }

    /// The shaft's radius at height `y`.
    pub fn shaft_at(&self, y: f64) -> f64 {
        let (r0, r1, r2) = self.shaft_r;
        if y <= self.deck_y {
            let t = ((y - WX_ROOF_TOP) / (self.deck_y - WX_ROOF_TOP)).clamp(0.0, 1.0);
            r0 + (r1 - r0) * t
        } else {
            let t = ((y - self.deck_y) / (self.crown_y - self.deck_y)).clamp(0.0, 1.0);
            r1 + (r2 - r1) * t
        }
    }

    /// A fin's outer edge (distance from the axis) at height `y`: out at
    /// its foot, sweeping in to meet the shaft at [`Self::fin_top`].
    pub fn fin_edge(&self, y: f64) -> f64 {
        let t = (y / self.fin_top).clamp(0.0, 1.0);
        let r_top = self.shaft_at(self.fin_top) + 0.1;
        r_top + (WX_FIN_FOOT - r_top) * (1.0 - t).powf(1.7)
    }

    /// The cloud's underside above the plinth.
    pub fn cloud_y(&self) -> f64 {
        self.h + WX_CLOUD_OVER
    }
}

/// The tower's height in a town of `pop` people: 44 m in a small town,
/// up to 62 m in a metropolis.
pub fn tower_height(pop: f64) -> f64 {
    (14.0 * (hw_core::det::log10(pop.max(1000.0)) - 0.8)).clamp(44.0, 62.0)
}

/// Where a fin stands (unit direction in the tower's frame).
pub fn fin_dir(k: usize) -> DVec2 {
    hw_core::det::dir2(WX_FIN_AT[k].to_radians())
}

/// Its solid parts above the plinth, in the tower's own frame.
pub fn footprints(l: &Landmark) -> Vec<Footprint> {
    let d = WxDims::of(l.height);
    let disc = |r: f64, y0: f64, y1: f64| Footprint::Disc {
        at: DVec2::ZERO,
        r,
        y0,
        y1,
    };
    let mut v = vec![
        disc(WX_HALL_R, 0.0, WX_ROOF_TOP - 0.3),
        disc(WX_ROOF_R, WX_ROOF_TOP - 0.5, WX_ROOF_TOP),
        disc(d.shaft_r.0, WX_ROOF_TOP, d.deck_y),
        disc(d.deck_r, d.deck_y - d.deck_span.0, d.deck_y + d.deck_span.1),
        disc(d.shaft_r.1, d.deck_y, d.crown_y),
        disc(
            d.crown_r + d.cannon_reach * 0.7,
            d.crown_y - 0.4,
            d.crown_y + 2.4,
        ),
        disc(0.35, d.crown_y, d.globe_y),
        disc(d.globe_r, d.globe_y - d.globe_r, d.globe_y + d.globe_r),
    ];
    // The fins, as thin slabs stacked up their swept edge: outside the
    // pavilion from the ground, over its roof from the shaft out.
    let rows = 12;
    for k in 0..WX_FIN_AT.len() {
        let dir = fin_dir(k);
        for i in 0..rows {
            let (y0, y1) = (
                d.fin_top * i as f64 / rows as f64,
                d.fin_top * (i + 1) as f64 / rows as f64,
            );
            let outer = d.fin_edge(y0);
            let inner = if y1 <= WX_ROOF_TOP {
                WX_HALL_R - 0.2
            } else {
                d.shaft_at(y0) - 0.1
            };
            if outer <= inner + 0.2 {
                continue;
            }
            v.push(Footprint::Box {
                at: dir * ((inner + outer) * 0.5),
                dir: DVec2::new(dir.y, -dir.x),
                half: DVec2::new((outer - inner) * 0.5, WX_FIN_T * 0.5),
                y0,
                y1,
            });
        }
    }
    v
}

/// The middle of the shower, in the town's frame.
pub fn rain_centre(l: &Landmark) -> DVec2 {
    l.to_plan(DVec2::new(0.0, WX_RAIN_AHEAD))
}

/// Picks the tower's place: on high ground 250-900 m out from the core,
/// outside the villains' quarter and well clear of the landmarks already
/// placed. None in a small town or where nothing fits.
pub(super) fn plan(
    w: &TownWorld,
    roads: &RoadGrid,
    core: DVec2,
    others: &[Landmark],
) -> Option<Landmark> {
    let pop = w.town.params.population;
    if pop < WX_MIN_POP {
        return None;
    }
    let h = tower_height(pop);
    for plaza in [16.0, 14.5] {
        let spot = super::landmarks::best_spot(w, roads, core, WX_REACH, 8.0, plaza, 3.0, |p| {
            let r = p.distance(core);
            if r < 250.0
                || w.town.dystopia.as_ref().is_some_and(|d| d.depth(p) > 0.0)
                || others
                    .iter()
                    .any(|o| o.pos.distance(p) < o.plaza + plaza + 160.0)
            {
                return f64::NEG_INFINITY;
            }
            // High ground, not too far out.
            w.ground(p) - (r - 520.0).abs() * 0.03
        });
        if let Some((p, _)) = spot {
            return Some(Landmark {
                kind: LandmarkKind::WeatherTower,
                height: h,
                dir: (core - p).normalize_or(DVec2::Y),
                ..super::landmarks::landmark_at(w, p, plaza)
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::urban::params::TownParams;
    use hw_core::Seed;

    #[test]
    fn the_tower_stands_clear_with_its_parts_solid() {
        let mut found = 0;
        for s in [1u64, 3, 7] {
            let w = TownWorld::generate(Seed(s), TownParams::sample(Seed(s), 40_000.0));
            let Some(t) = w
                .landmarks
                .iter()
                .find(|l| l.kind == LandmarkKind::WeatherTower)
            else {
                continue;
            };
            found += 1;
            assert!((44.0..=62.0).contains(&t.height));
            if let Some(d) = &w.town.dystopia {
                assert!(d.depth(t.pos) <= 0.0, "seed {s}: in the villains' quarter");
            }
            for o in w.landmarks.iter().filter(|o| o.kind != t.kind) {
                assert!(
                    o.pos.distance(t.pos) > o.plaza + t.plaza + 150.0,
                    "seed {s}: crowding the {:?}",
                    o.kind
                );
            }
            let fps = t.footprints();
            let d = WxDims::of(t.height);
            // The shaft, the saucer, the globe and every fin's foot are solid.
            let solid = |local: DVec2, y: f64| {
                let p = t.to_plan(local);
                fps.iter().any(|f| {
                    let (y0, y1) = f.span();
                    f.distance(p) < 0.01 && y >= y0 && y <= y1
                })
            };
            assert!(solid(DVec2::ZERO, t.base + d.deck_y - 4.0));
            assert!(solid(DVec2::new(d.deck_r - 0.3, 0.0), t.base + d.deck_y));
            assert!(solid(DVec2::ZERO, t.base + d.globe_y));
            for k in 0..3 {
                assert!(solid(fin_dir(k) * (WX_FIN_FOOT - 0.5), t.base + 0.5));
                assert!(solid(fin_dir(k) * 3.0, t.base + d.fin_top * 0.7));
            }
            // Everything stands on the plinth.
            assert!(WX_FIN_FOOT < t.plaza - 1.0);
        }
        assert!(found >= 2, "a weather tower in most big towns");
    }

    #[test]
    fn fins_sweep_in_to_the_shaft() {
        let d = WxDims::of(50.0);
        assert!((d.fin_edge(0.0) - WX_FIN_FOOT).abs() < 1e-9);
        assert!(d.fin_edge(d.fin_top) < d.shaft_at(d.fin_top) + 0.2);
        let mut last = f64::INFINITY;
        for i in 0..=20 {
            let r = d.fin_edge(d.fin_top * i as f64 / 20.0);
            assert!(r <= last);
            last = r;
        }
    }
}
