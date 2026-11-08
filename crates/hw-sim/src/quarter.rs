//! The mad scientist's quarter in the simulation: where his minions hang
//! about. Groups stand on its street corners and at the gate of his Tesla
//! tower, a couple of levels above the team, while a hero is near; they go
//! home when nobody is, and stand there fresh the next time. Technicians
//! keep watch from flat roofs and shoot down at heroes in the street.

use crate::collision::Obstacles;
use glam::{DVec2, DVec3};
use hw_gen::style::Roof;
use hw_gen::urban::landmarks::LandmarkKind;
use hw_gen::urban::world::TownWorld;

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
pub fn lookouts(world: &TownWorld, corners: &[Corner]) -> Vec<Lookout> {
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
pub fn corners(world: &TownWorld) -> Vec<Corner> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use hw_core::Seed;
    use hw_gen::urban::TownParams;

    #[test]
    fn the_quarter_has_corners_and_a_gate() {
        let w = TownWorld::generate(Seed(3), TownParams::sample(Seed(3), 20_000.0));
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
    }
}
