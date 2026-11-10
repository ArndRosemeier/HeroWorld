//! Where the utopia frays: the band around the mad scientist's quarter
//! (and the quarter itself) gets graffiti on its walls, and the streets
//! running into the quarter are half shut by barricades.
//!
//! Pure data from the town; the client draws it.

use super::dystopia::Zone;
use super::quarter::DetailId;
use super::roadgrid::RoadGrid;
use super::world::{TownWorld, WorldBuilding, WorldRoad};
use glam::{DVec2, DVec3};

/// How many graffiti images there are to pick from.
pub const TAG_IMAGES: u8 = 4;

/// A piece of graffiti on a wall.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tag {
    /// The middle of it, on the wall.
    pub at: DVec3,
    /// Out of the wall.
    pub normal: DVec2,
    /// Which image (below [`TAG_IMAGES`]).
    pub image: u8,
    /// Across (m).
    pub size: f64,
    /// A little askew (radians).
    pub angle: f64,
}

/// A barricade across a street where it runs into the quarter: barriers
/// from both kerbs leave one lane open down the middle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Barricade {
    /// The middle of the street.
    pub at: DVec2,
    /// Along the street, into the quarter.
    pub along: DVec2,
    /// Half the carriageway's width, and the pavement's.
    pub half: f64,
    pub pavement: f64,
    pub seed: u64,
}

/// The graffiti on the town's walls, nearest the quarter most.
/// (Of the buildings given, by id.)
pub fn tags<'a>(
    world: &TownWorld,
    buildings: impl IntoIterator<Item = (DetailId, &'a WorldBuilding)>,
) -> Vec<Tag> {
    let Some(d) = &world.town.dystopia else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, b) in buildings {
        let s = &b.site;
        let front = s.origin + s.along * s.size.x * 0.5;
        let p = match d.zone(front) {
            Zone::Edge => 0.6,
            Zone::Dystopia => 0.45,
            Zone::Utopia => continue,
        };
        let mut rng = world.seed.child("graffiti").child(i.0).rng();
        if !rng.chance(p) || s.size.x < 5.0 {
            continue;
        }
        // One, sometimes two, low on the front where a hand can reach.
        let n = if s.size.x > 14.0 && rng.chance(0.4) {
            2
        } else {
            1
        };
        for k in 0..n {
            let size = rng.range(1.4, 2.6).min(s.size.x * 0.4);
            let t = (k as f64 + rng.range(0.25, 0.75)) / n as f64;
            let along = (t * s.size.x).clamp(size * 0.5 + 0.3, s.size.x - size * 0.5 - 0.3);
            let q = s.origin + s.along * along;
            out.push(Tag {
                at: DVec3::new(q.x, b.base + rng.range(1.3, 2.2), q.y),
                normal: -s.inward,
                image: rng.range_int(0, TAG_IMAGES as i64 - 1) as u8,
                size,
                angle: rng.range(-0.12, 0.12),
            });
        }
    }
    out
}

/// The barricades where streets run into the quarter.
/// (Of the roads `all`: the barricades on them, wherever they are.)
pub fn barricades(world: &TownWorld, all: &[WorldRoad]) -> Vec<Barricade> {
    let Some(d) = &world.town.dystopia else {
        return Vec::new();
    };
    let mut out: Vec<Barricade> = Vec::new();
    let roads = RoadGrid::new(all, 0.0);
    for (ri, r) in all.iter().enumerate() {
        for w in r.pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let (ia, ib) = (d.contains(a), d.contains(b));
            if ia == ib {
                continue;
            }
            // Where it crosses the edge, to a few centimetres.
            let (mut out_p, mut in_p) = if ia { (b, a) } else { (a, b) };
            for _ in 0..24 {
                let m = (out_p + in_p) * 0.5;
                if d.contains(m) {
                    in_p = m;
                } else {
                    out_p = m;
                }
            }
            let along = if ia { a - b } else { b - a }.normalize_or(DVec2::X);
            // A few metres in, past the crossing street's mouth: clear of
            // every other street, so nothing drives or walks through it.
            let across = along.perp();
            let reach = r.half + r.pavement;
            let clear = |p: DVec2| {
                (-2..=2).all(|k| !roads.touches(p + across * reach * k as f64 * 0.5, 2.0, Some(ri)))
            };
            let Some(at) = (0..14)
                .map(|k| in_p + along * (4.0 + 2.0 * k as f64))
                .find(|p| clear(*p))
            else {
                continue;
            };
            if out.iter().any(|o| o.at.distance(at) < 15.0) {
                continue;
            }
            out.push(Barricade {
                at,
                along,
                half: r.half,
                pavement: r.pavement,
                seed: world.seed.child("barricade").child(r.id.0).0,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::urban::TownParams;
    use hw_core::Seed;

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn the_quarters_edge_is_tagged_and_barricaded() {
        let w = TownWorld::generate(Seed(3), TownParams::sample(Seed(3), 20_000.0));
        let d = w.town.dystopia.as_ref().unwrap();
        let all = w.everything();
        let tags = tags(&w, all.buildings.iter().map(|(i, b)| (*i, b)));
        assert!(tags.len() >= 10, "{} tags", tags.len());
        for t in &tags {
            let p = DVec2::new(t.at.x, t.at.z);
            assert!(d.distance(p) < super::super::dystopia::BAND + 1.0);
            assert!(t.image < TAG_IMAGES);
        }
        let bars = barricades(&w, &all.roads);
        assert!(bars.len() >= 2, "{} barricades", bars.len());
        for b in &bars {
            assert!(d.contains(b.at), "a barricade outside the quarter");
        }
        assert_eq!(
            tags,
            super::tags(&w, all.buildings.iter().map(|(i, b)| (*i, b)))
        );
        assert_eq!(bars, barricades(&w, &all.roads));
    }
}
