//! The mad scientist's quarter in the simulation: where his minions hang
//! about. Groups stand on its street corners and at the gate of his Tesla
//! tower, a couple of levels above the team, while a hero is near; they go
//! home when nobody is, and stand there fresh the next time. Technicians
//! keep watch from flat roofs and shoot down at heroes in the street.
//!
//! The Chrome Jackets hang out in the frayed band around the quarter
//! (the walls they tag with graffiti): small groups of greasers on street
//! corners, who warn a stranger off their turf (see [`crate::factions`]).

use crate::collision::Obstacles;
use glam::{DVec2, DVec3};
use hw_gen::style::Roof;
use hw_gen::urban::area::Area;
use hw_gen::urban::landmarks::LandmarkKind;

/// A corner's group stands there while a hero is this near (m).
pub const WAKE: f64 = 220.0;
/// Corners are at least this far apart (m).
const SPACING: f64 = 110.0;
/// Levels above the team: on the corners, and at the tower's gate.
pub const CORNER_EXTRA: u8 = 2;
pub const GATE_EXTRA: u8 = 3;

/// Rooftop lookouts are at least this far apart (m).
const ROOF_SPACING: f64 = 80.0;
/// How many technicians watch from a roof.
pub const ROOF_GROUP: u32 = 3;

/// A rooftop lookout: the middle of a flat roof's front edge (x, z) and
/// the way out to its street.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lookout {
    pub at: DVec2,
    pub facing: DVec2,
    /// How deep the building is.
    pub depth: f64,
}

/// Where a lookout's villains stand: on the roof a metre behind its front
/// edge (found from the solids, since fronts are set back and roofs step),
/// or none if the building is too low to shoot down from.
pub fn perch(solids: &Obstacles, l: &Lookout) -> Option<DVec3> {
    let ground = solids.ground_at(l.at);
    let mut k = 0.0;
    while k < l.depth * 0.5 {
        let p = l.at - l.facing * k;
        if solids.floor_under(p, 10_000.0) > ground + 5.0 {
            let stand = p - l.facing * 1.0;
            let roof = solids.floor_under(stand, 10_000.0);
            return (roof > ground + 5.0).then_some(DVec3::new(stand.x, roof, stand.y));
        }
        k += 0.5;
    }
    None
}

/// Flat roofs in the quarter (the middles of their fronts), low enough to shoot down from (two to six
/// floors), away from the corners and from each other; nearest the middle
/// first.
pub fn lookouts(world: &Area, corners: &[Corner]) -> Vec<Lookout> {
    let Some(d) = &world.town.dystopia else {
        return Vec::new();
    };
    let mut found: Vec<Lookout> = world
        .buildings
        .iter()
        .filter(|b| (2..=6).contains(&b.site.floors) && b.site.size.min_element() >= 9.0)
        .filter_map(|b| {
            let s = &b.site;
            // The middle of its front (see [`perch`] for the spot on top).
            let at = s.origin + s.along * s.size.x * 0.5;
            if !d.contains(at + s.inward * s.size.y * 0.5) {
                return None;
            }
            let look = s.look();
            (look.roof == Roof::Flat && !look.taxi_pad).then_some(Lookout {
                at,
                facing: -s.inward,
                depth: s.size.y,
            })
        })
        .collect();
    found.sort_by(|a, b| {
        a.at.distance(d.centre)
            .total_cmp(&b.at.distance(d.centre))
            .then(a.at.x.total_cmp(&b.at.x))
            .then(a.at.y.total_cmp(&b.at.y))
    });
    let most = (corners.len() / 2).clamp(2, 10);
    let mut out: Vec<Lookout> = Vec::new();
    for l in found {
        if out.len() >= most {
            break;
        }
        if out.iter().all(|o| o.at.distance(l.at) >= ROOF_SPACING)
            && corners.iter().all(|c| c.at.distance(l.at) >= 30.0)
        {
            out.push(l);
        }
    }
    out
}

/// One corner: where, which way it faces (towards where heroes come from),
/// and how many levels above the team its villains stand.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Corner {
    pub at: DVec2,
    pub facing: DVec2,
    pub extra: u8,
}

/// The quarter's corners, in a fixed order (none if the town has no
/// villain quarter).
pub fn corners(world: &Area) -> Vec<Corner> {
    let Some(d) = &world.town.dystopia else {
        return Vec::new();
    };
    let mut out = Vec::new();
    // The tower's gate first: the toughest group.
    if let Some(l) = world
        .landmarks
        .iter()
        .find(|l| l.kind == LandmarkKind::TeslaTower)
    {
        out.push(Corner {
            at: l.pos + l.dir * (l.plaza + 3.0),
            facing: l.dir,
            extra: GATE_EXTRA,
        });
    }
    // Street ends and crossings inside the quarter.
    let area: f64 = d
        .pieces
        .iter()
        .map(|&i| world.town.pieces[i].face.area)
        .sum::<f64>()
        .min(std::f64::consts::PI * d.radius * d.radius);
    let most = ((area / 40_000.0) as usize).clamp(3, 24);
    let mut ends: Vec<DVec2> = world
        .roads
        .iter()
        .flat_map(|r| [r.pts[0], r.pts[r.pts.len() - 1]])
        .filter(|p| d.contains(*p))
        .collect();
    // Nearest the middle first, ties by position: the same every time.
    ends.sort_by(|a, b| {
        a.distance(d.centre)
            .total_cmp(&b.distance(d.centre))
            .then(a.x.total_cmp(&b.x))
            .then(a.y.total_cmp(&b.y))
    });
    for p in ends {
        if out.len() > most {
            break;
        }
        if out.iter().all(|c| c.at.distance(p) >= SPACING) {
            out.push(Corner {
                at: p,
                facing: (p - d.centre).normalize_or(DVec2::X),
                extra: CORNER_EXTRA,
            });
        }
    }
    out
}

/// Jacket hangouts are at least this far apart (m), and at most this many.
const HANGOUT_SPACING: f64 = 130.0;
const MOST_HANGOUTS: usize = 6;
/// Greasers at a hangout.
pub const HANGOUT_GROUP: u32 = 3;

/// Where the Chrome Jackets hang out: street ends and crossings in the
/// band round the quarter, nearest the quarter first, facing out of it.
/// None near `keep_clear` (where heroes appear) within `clear` m.
pub fn hangouts(world: &Area, keep_clear: DVec2, clear: f64) -> Vec<Corner> {
    use hw_gen::urban::dystopia::Zone;
    let Some(d) = &world.town.dystopia else {
        return Vec::new();
    };
    // On the pavement a few metres in from a street's end, facing the
    // street (out of the traffic).
    let mut spots: Vec<(DVec2, DVec2)> = world
        .roads
        .iter()
        .filter(|r| r.pts.len() >= 2)
        .flat_map(|r| {
            let n = r.pts.len();
            [(r.pts[0], r.pts[1]), (r.pts[n - 1], r.pts[n - 2])].map(|(end, next)| {
                let inward = (next - end).normalize_or(DVec2::X);
                let side = inward.perp();
                let at = end + inward * 6.0 + side * (r.half + r.pavement * 0.5);
                (at, -side)
            })
        })
        .filter(|(p, _)| d.zone(*p) == Zone::Edge && p.distance(keep_clear) > clear)
        .collect();
    spots.sort_by(|(a, _), (b, _)| {
        a.distance(d.centre)
            .total_cmp(&b.distance(d.centre))
            .then(a.x.total_cmp(&b.x))
            .then(a.y.total_cmp(&b.y))
    });
    let mut out: Vec<Corner> = Vec::new();
    for (p, facing) in spots {
        if out.len() >= MOST_HANGOUTS {
            break;
        }
        if out.iter().all(|c| c.at.distance(p) >= HANGOUT_SPACING) {
            out.push(Corner {
                at: p,
                facing,
                extra: 0,
            });
        }
    }
    out
}

/// Heroes below this level start near the Chrome Jackets' corners, where
/// small crime is common and close (flight, which makes the whole town
/// near, opens at this level).
pub const ROOKIE_LEVEL: u8 = 5;
/// The rookies' street is this far from the nearest hangout (m): the
/// Jackets' muggings are round the corner, but the street is off their
/// turf, so nobody warns a newcomer off on arrival.
const ROOKIE_NEAR: f64 = 120.0;
const ROOKIE_FAR: f64 = 300.0;
/// ... and at least this far out of the mad scientist's quarter (m), clear
/// of its corners and rooftop lookouts.
const ROOKIE_QUARTER_CLEAR: f64 = 180.0;

/// Where new heroes start: the side street nearest `home` (the town's
/// core) that lies within a few minutes' walk of a Jacket hangout, off
/// their turf and out of the quarter, and the way along it; none if the
/// town has no gang (a small town: then heroes start at its core). Never
/// within `clear` m of any of `avoid` (another gang's roost).
pub fn rookie_start(
    world: &Area,
    hangouts: &[Corner],
    home: DVec2,
    avoid: &[DVec2],
    clear: f64,
) -> Option<(DVec2, DVec2)> {
    let d = world.town.dystopia.as_ref()?;
    if hangouts.is_empty() {
        return None;
    }
    let mut best: Option<(f64, DVec2, DVec2)> = None;
    for r in world.roads.iter().filter(|r| !r.main) {
        for w in r.pts.windows(2) {
            let m = (w[0] + w[1]) * 0.5;
            if w[0].distance(w[1]) < 8.0 || world.world.ground(m) < hw_gen::urban::world::DECK {
                continue;
            }
            let gang = hangouts
                .iter()
                .map(|h| h.at.distance(m))
                .fold(f64::INFINITY, f64::min);
            if !(ROOKIE_NEAR..ROOKIE_FAR).contains(&gang)
                || d.distance(m) < ROOKIE_QUARTER_CLEAR
                || avoid.iter().any(|a| a.distance(m) < clear)
            {
                continue;
            }
            let score = m.distance(home);
            let better =
                best.is_none_or(|b| score < b.0 || (score == b.0 && (m.x, m.y) < (b.1.x, b.1.y)));
            if better {
                best = Some((score, m, (w[1] - w[0]).normalize()));
            }
        }
    }
    best.map(|b| (b.1, b.2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hw_core::Seed;
    use hw_gen::urban::TownParams;

    #[test]
    fn the_quarter_has_corners_and_a_gate() {
        let world = std::sync::Arc::new(hw_gen::urban::world::TownWorld::generate(
            Seed(3),
            TownParams::sample(Seed(3), 20_000.0),
        ));
        let centre = world.town.dystopia.as_ref().unwrap().centre;
        let w = Area::around(world, centre, hw_gen::urban::area::AREA_HALF);
        let c = corners(&w);
        assert!(c.len() >= 3, "{} corners", c.len());
        assert_eq!(c[0].extra, GATE_EXTRA);
        let d = w.town.dystopia.as_ref().unwrap();
        for k in &c[1..] {
            assert!(d.contains(k.at));
        }
        assert_eq!(c, corners(&w));
        let roofs = lookouts(&w, &c);
        assert!(roofs.len() >= 2, "{} rooftop lookouts", roofs.len());
        for r in &roofs {
            assert!(d.contains(r.at - r.facing * r.depth * 0.5));
        }
        assert_eq!(roofs, lookouts(&w, &c));
        let far = DVec2::new(1e6, 1e6);
        let jackets = hangouts(&w, far, 0.0);
        assert!(jackets.len() >= 2, "{} hangouts", jackets.len());
        for j in &jackets {
            assert_eq!(d.zone(j.at), hw_gen::urban::dystopia::Zone::Edge);
        }
        assert_eq!(jackets, hangouts(&w, far, 0.0));
        // New heroes start a short walk from a hangout, off its turf and
        // out of the quarter.
        let core = w.world.spawn_line().0;
        let near = hangouts(&w, core, 300.0);
        let (start, _) = rookie_start(&w, &near, core, &[], 0.0).expect("a rookie start");
        let gang = near
            .iter()
            .map(|h| h.at.distance(start))
            .fold(f64::INFINITY, f64::min);
        assert!(
            (ROOKIE_NEAR..ROOKIE_FAR).contains(&gang),
            "{gang:.0} m from the Jackets"
        );
        assert!(d.distance(start) >= ROOKIE_QUARTER_CLEAR);
        assert_eq!(
            Some(start),
            rookie_start(&w, &near, core, &[], 0.0).map(|s| s.0)
        );
    }
}
