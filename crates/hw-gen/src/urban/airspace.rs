//! What airships have to keep clear of: the highest thing drawn under a
//! stretch of sky, buildings with their roofs and crowns
//! ([`crate::building::BuildingSite::top`]) and landmarks alike.
//!
//! Straight from the town plan, so the server and every client agree
//! wherever they stand and whatever they have loaded. Only the quarters
//! under the box are planned: asking about one stretch of sky over a
//! megacity costs no more than over a village.

use super::footprint::Footprint;
use super::landmarks::LandmarkKind;
use super::world::TownWorld;
use glam::DVec2;

/// An upright box of sky in plan: its middle, the way its length runs
/// (unit), and its half length and half width (m).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyBox {
    pub mid: DVec2,
    pub along: DVec2,
    pub half_len: f64,
    pub half_wide: f64,
}

impl SkyBox {
    pub fn new(mid: DVec2, along: DVec2, half_len: f64, half_wide: f64) -> Self {
        SkyBox {
            mid,
            along: along.normalize_or(DVec2::Y),
            half_len,
            half_wide,
        }
    }

    /// Its four corners.
    pub fn corners(&self) -> [DVec2; 4] {
        let (a, c) = (
            self.along * self.half_len,
            self.along.perp() * self.half_wide,
        );
        [
            self.mid - a - c,
            self.mid + a - c,
            self.mid + a + c,
            self.mid - a + c,
        ]
    }

    /// Its extent along the unit vector `u`, measured from the origin.
    fn extent(&self, u: DVec2) -> (f64, f64) {
        let c = self.mid.dot(u);
        let e = self.half_len * self.along.dot(u).abs()
            + self.half_wide * self.along.perp().dot(u).abs();
        (c - e, c + e)
    }

    /// Does it overlap the convex outline `pts` grown by `grow` (m)?
    /// Separating axes: the box's two and the outline's edges.
    pub fn meets_outline(&self, pts: &[DVec2], grow: f64) -> bool {
        let n = pts.len();
        let mut axes = vec![self.along, self.along.perp()];
        for k in 0..n {
            let e = pts[(k + 1) % n] - pts[k];
            if e.length_squared() > 1e-12 {
                axes.push(e.normalize().perp());
            }
        }
        axes.iter().all(|&u| {
            let (a0, a1) = self.extent(u);
            let (b0, b1) = pts
                .iter()
                .map(|p| p.dot(u))
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), d| {
                    (lo.min(d), hi.max(d))
                });
            a0 < b1 + grow && b0 - grow < a1
        })
    }

    /// Does it overlap the footprint `f` grown by `grow` (m)?
    pub fn meets_footprint(&self, f: &Footprint, grow: f64) -> bool {
        let mut axes = vec![self.along, self.along.perp()];
        match *f {
            Footprint::Box { dir, .. } => {
                axes.push(dir);
                axes.push(dir.perp());
            }
            Footprint::Disc { at, .. } => {
                // The way from the disc to the nearest point of the box.
                let d = at - self.mid;
                let l = DVec2::new(d.dot(self.along), d.dot(self.along.perp()));
                let near = l.clamp(
                    DVec2::new(-self.half_len, -self.half_wide),
                    DVec2::new(self.half_len, self.half_wide),
                );
                let p = self.mid + self.along * near.x + self.along.perp() * near.y;
                if (at - p).length_squared() > 1e-12 {
                    axes.push((at - p).normalize());
                }
            }
        }
        axes.iter().all(|&u| {
            let (a0, a1) = self.extent(u);
            let (b0, b1) = f.extent(DVec2::ZERO, u);
            a0 < b1 + grow && b0 - grow < a1
        })
    }
}

/// How far round a building's walls its eaves, cornices, signs and corner
/// tower may reach (m).
pub const OVERHANG: f64 = 2.5;

/// The town plan's box round `sky` to look in.
fn plan_box(sky: &SkyBox) -> (DVec2, DVec2) {
    let k = sky.corners();
    let reach = DVec2::splat(40.0);
    let lo = k.iter().fold(DVec2::INFINITY, |a, &p| a.min(p));
    let hi = k.iter().fold(DVec2::NEG_INFINITY, |a, &p| a.max(p));
    (lo - reach, hi + reach)
}

/// Is the town under `sky` planned already, so [`highest_under`] costs
/// next to nothing (for a frame that must not wait)?
pub fn planned(w: &TownWorld, sky: &SkyBox) -> bool {
    let (lo, hi) = plan_box(sky);
    w.quarters_ready(lo, hi)
}

/// The highest point drawn anywhere under `sky` (m, the town's heights),
/// or -infinity over open ground: every building's roof and crown, every
/// landmark but those of the `skip` kinds (an airship's own mast).
pub fn highest_under(w: &TownWorld, sky: &SkyBox, skip: &[LandmarkKind]) -> f64 {
    let (lo, hi) = plan_box(sky);
    let mut top = f64::NEG_INFINITY;
    for q in w.quarters_in(lo, hi) {
        for b in &q.buildings {
            if sky.meets_outline(&b.site.corners(), OVERHANG) {
                top = top.max(b.base + b.site.top());
            }
        }
    }
    for l in &w.landmarks {
        if skip.contains(&l.kind) {
            continue;
        }
        let parts = l.footprints();
        if parts.iter().any(|f| sky.meets_footprint(f, 1.0)) {
            // The drawn height (the spire's needle, the lighthouse's lamp)
            // can stand over the solid parts.
            let solid = parts.iter().map(|f| f.span().1).fold(l.base, f64::max);
            top = top.max(solid.max(l.base + l.height));
        }
    }
    top
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::urban::footprint::Footprint;

    #[test]
    fn boxes_meet_what_they_overlap_and_nothing_else() {
        let sky = SkyBox::new(DVec2::new(10.0, 0.0), DVec2::new(1.0, 1.0), 20.0, 5.0);
        // A long building crossing the box with no corner inside it and the
        // box's middle outside it: the case a corner test misses.
        let across = [
            DVec2::new(-30.0, 14.0),
            DVec2::new(40.0, -56.0),
            DVec2::new(44.0, -52.0),
            DVec2::new(-26.0, 18.0),
        ];
        assert!(sky.meets_outline(&across, 0.0));
        // Well to one side: clear.
        let aside = across.map(|p| p + DVec2::new(-30.0, -30.0));
        assert!(!sky.meets_outline(&aside, 0.0));
        // Just beyond the end, within the grow.
        let end = [
            DVec2::new(26.5, 15.0),
            DVec2::new(30.0, 15.0),
            DVec2::new(30.0, 20.0),
            DVec2::new(26.5, 20.0),
        ];
        assert!(!sky.meets_outline(&end, 0.0));
        assert!(sky.meets_outline(&end, 3.0));
        let disc = |x: f64, y: f64| Footprint::Disc {
            at: DVec2::new(x, y),
            r: 2.0,
            y0: 0.0,
            y1: 10.0,
        };
        assert!(sky.meets_footprint(&disc(10.0, 0.0), 0.0));
        assert!(!sky.meets_footprint(&disc(0.0, 20.0), 0.0));
        // Off a corner, out along its diagonal: within reach of both of the
        // box's sides' lines, yet clear of the box.
        let c = sky.corners()[2] + (sky.along + sky.along.perp()).normalize() * 2.6;
        assert!(!sky.meets_footprint(&disc(c.x, c.y), 0.0));
    }
}
