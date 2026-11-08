//! The mad scientist's quarter in the simulation: where his minions hang
//! about. Groups stand on its street corners and at the gate of his Tesla
//! tower, a couple of levels above the team, while a hero is near; they go
//! home when nobody is, and stand there fresh the next time.

use glam::DVec2;
use hw_gen::urban::landmarks::LandmarkKind;
use hw_gen::urban::world::TownWorld;

/// A corner's group stands there while a hero is this near (m).
pub const WAKE: f64 = 220.0;
/// Corners are at least this far apart (m).
const SPACING: f64 = 110.0;
/// Levels above the team: on the corners, and at the tower's gate.
pub const CORNER_EXTRA: u8 = 2;
pub const GATE_EXTRA: u8 = 3;

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
    }
}
