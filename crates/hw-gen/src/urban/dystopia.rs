//! The mad scientist's quarter: one coherent district of the town that the
//! villains hold, out from the core towards the edge, bounded by main roads.
//!
//! A run of neighbouring [`Piece`]s, grown from one seed piece until it
//! covers the town's share of dystopia ([`super::TownParams::dystopia`]).
//! Around it lies a band where the utopia frays: graffiti, flickering
//! lights, barricades.

use super::{contains, inner_point, CentreKind, Era, Growth, Piece};
use crate::water::segment_distance;
use glam::DVec2;
use hw_core::Seed;
use std::collections::BTreeMap;

/// Towns smaller than this have no villain quarter.
pub const MIN_POPULATION: f64 = 3000.0;
/// Width of the frayed band around the quarter (m).
pub const BAND: f64 = 70.0;

/// Where a point lies with respect to the quarter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Zone {
    /// The golden-age town.
    Utopia,
    /// The frayed band around the quarter.
    Edge,
    /// Inside the mad scientist's quarter.
    Dystopia,
}

/// The villains' quarter.
#[derive(Clone, Debug, PartialEq)]
pub struct Dystopia {
    /// Indices into [`super::Town::pieces`].
    pub pieces: Vec<usize>,
    /// The middle of it: the Tesla tower goes near here.
    pub centre: DVec2,
    /// How far it reaches from `centre` (where no main road bounds it, its
    /// edge runs along side streets).
    pub radius: f64,
    /// The outlines of its pieces, for [`Self::zone`].
    outlines: Vec<Vec<DVec2>>,
}

impl Dystopia {
    /// Picks the quarter, or none for a small town.
    pub fn plan(
        seed: Seed,
        pieces: &[Piece],
        growth: &Growth,
        share: f64,
        pop: f64,
    ) -> Option<Self> {
        if pop < MIN_POPULATION || pieces.is_empty() {
            return None;
        }
        let core = growth
            .centres
            .iter()
            .find(|c| c.kind == CentreKind::Core)
            .map_or(DVec2::ZERO, |c| c.pos);
        let centres: Vec<Option<DVec2>> = pieces.iter().map(|p| inner_point(&p.face.pts)).collect();
        let far = centres
            .iter()
            .flatten()
            .map(|c| c.distance(core))
            .fold(0.0, f64::max);
        let total: f64 = pieces.iter().map(|p| p.face.area).sum();
        let want = total * share.clamp(0.04, 0.25);
        // The seed piece: out from the core, not in the old town.
        let mut rng = seed.child("dystopia").rng();
        let candidates: Vec<usize> = (0..pieces.len())
            .filter(|&i| {
                centres[i].is_some_and(|c| {
                    let d = c.distance(core) / far.max(1.0);
                    (0.4..0.85).contains(&d) && pieces[i].era != Era::Old
                })
            })
            .collect();
        let first = if candidates.is_empty() {
            (0..pieces.len()).find(|&i| centres[i].is_some())?
        } else {
            *rng.pick(&candidates)
        };
        let centre = centres[first]?;
        // As far as a round quarter of that area would reach, a little more
        // (it is cut by the town's edge and by water).
        let radius = (want / std::f64::consts::PI).sqrt() * 1.2;
        // The pieces within reach, joined to the seed piece by main roads
        // they share.
        let mut by_edge: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (i, p) in pieces.iter().enumerate() {
            for (e, _) in &p.face.edges {
                by_edge.entry(*e).or_default().push(i);
            }
        }
        let near = |i: usize| {
            let o = &pieces[i].face.pts;
            contains(o, centre) || outline_distance(o, centre) < radius
        };
        let mut chosen = vec![first];
        let mut k = 0;
        while k < chosen.len() {
            let i = chosen[k];
            k += 1;
            for (e, _) in &pieces[i].face.edges {
                for &j in &by_edge[e] {
                    if !chosen.contains(&j) && near(j) {
                        chosen.push(j);
                    }
                }
            }
        }
        chosen.sort_unstable();
        let outlines = chosen.iter().map(|&i| pieces[i].face.pts.clone()).collect();
        Some(Dystopia {
            pieces: chosen,
            centre,
            radius,
            outlines,
        })
    }

    /// Is `p` inside the quarter?
    pub fn contains(&self, p: DVec2) -> bool {
        p.distance(self.centre) <= self.radius && self.outlines.iter().any(|o| contains(o, p))
    }

    /// Distance from `p` to the quarter (0 inside; near enough elsewhere).
    pub fn distance(&self, p: DVec2) -> f64 {
        if self.contains(p) {
            return 0.0;
        }
        let round = p.distance(self.centre) - self.radius;
        if round > BAND * 2.0 {
            return round;
        }
        let pieces = self
            .outlines
            .iter()
            .map(|o| {
                if contains(o, p) {
                    0.0
                } else {
                    outline_distance(o, p)
                }
            })
            .fold(f64::MAX, f64::min);
        pieces.max(round).max(0.0)
    }

    pub fn zone(&self, p: DVec2) -> Zone {
        match self.distance(p) {
            d if d <= 0.0 => Zone::Dystopia,
            d if d < BAND => Zone::Edge,
            _ => Zone::Utopia,
        }
    }

    /// How deep in the quarter `p` is, 0 (the utopia) to 1 (well inside),
    /// for fading fog and colour in and out.
    pub fn depth(&self, p: DVec2) -> f64 {
        if self.contains(p) {
            // Full a little way in from its edge.
            let piece = self
                .outlines
                .iter()
                .filter(|o| contains(o, p))
                .map(|o| outline_distance(o, p))
                .fold(0.0, f64::max);
            let inner = piece.min(self.radius - p.distance(self.centre));
            0.5 + 0.5 * (inner / 40.0).clamp(0.0, 1.0)
        } else {
            0.5 * (1.0 - self.distance(p) / BAND).max(0.0)
        }
    }
}

fn outline_distance(o: &[DVec2], p: DVec2) -> f64 {
    (0..o.len())
        .map(|k| segment_distance(p, o[k], o[(k + 1) % o.len()]))
        .fold(f64::MAX, f64::min)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::urban::{Town, TownParams};

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn one_compact_quarter_out_from_the_core() {
        for s in [1u64, 3, 7] {
            let params = TownParams::sample(Seed(s), 40_000.0);
            let share = params.dystopia;
            let town = Town::generate(Seed(s), params);
            let d = town.dystopia.as_ref().expect("a villain quarter");
            let total: f64 = town.pieces.iter().map(|p| p.face.area).sum();
            let area: f64 = d.pieces.iter().map(|&i| town.pieces[i].face.area).sum();
            let frac = area / total;
            assert!(
                frac > 0.01 && frac < share.clamp(0.04, 0.25) * 2.0,
                "seed {s}: {frac:.3} of the town"
            );
            assert!(d.contains(d.centre));
            assert_eq!(d.zone(d.centre), Zone::Dystopia);
            assert!(d.depth(d.centre) > 0.5);
            // Far away is the utopia.
            let away = d.centre + DVec2::new(d.radius + BAND * 3.0, 0.0);
            assert_eq!(d.zone(away), Zone::Utopia);
            assert_eq!(d.depth(away), 0.0);
        }
        let small = Town::generate(Seed(2), TownParams::sample(Seed(2), 800.0));
        assert!(small.dystopia.is_none(), "a hamlet with a villain quarter");
    }
}
