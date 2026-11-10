//! The doors that close a building's doorways: which doorways get leaves,
//! where they hang and which way they swing. Pure data from the plan, so
//! the server's collision and every client's drawing agree on them.
//!
//! Street doors are a pair of painted leaves; a glazed entrance (a shop's
//! front, an office lobby) gets a pair of glass doors in its middle with
//! fixed glass beside and above them; doors between rooms keep the leaves
//! they always had. Wide openings (a hall's wings, the lift core) stay open
//! archways. Leaves swing into the room behind the doorway (never into a
//! corridor or out onto the street).

use crate::building::{BuildingPlan, Door, Floor, RoomKind, ENTRANCE_SIZE};
use glam::DVec2;

/// What closes a doorway.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DoorKind {
    /// A street door: two painted leaves.
    Front,
    /// The glass doors in the middle of a glazed entrance.
    Glass,
    /// A door between two rooms.
    Inner,
}

/// One leaf: a slab that turns about a hinge at one end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leaf {
    /// The hinge's foot (building x, z).
    pub hinge: DVec2,
    /// From the hinge to the leaf's free edge when shut (unit).
    pub shut: DVec2,
    /// The same when open: into the room it swings into (unit).
    pub open: DVec2,
    pub width: f64,
}

/// The door in one doorway.
#[derive(Clone, Debug, PartialEq)]
pub struct DoorLeaves {
    /// Names the door within its building (see [`door_key`]).
    pub key: u32,
    pub level: u32,
    pub kind: DoorKind,
    /// Centre of the doorway on the wall line (x, z), as [`Door::at`].
    pub at: DVec2,
    /// Along the wall (unit, x or z).
    pub along: DVec2,
    /// Into the room the leaves swing into (unit).
    pub into: DVec2,
    /// Half the wall's thickness (m).
    pub wall_half: f64,
    /// The leaves' plane: this far from the wall line towards `into`.
    pub plane: f64,
    /// Height of the leaves above the floor.
    pub height: f64,
    /// Height of the doorway (fixed glass fills it above the leaves).
    pub top: f64,
    /// Half the width the leaves close (m).
    pub half: f64,
    pub leaves: Vec<Leaf>,
    /// Fixed glass beside the doors: (from, to) along `along`, measured
    /// from `at`.
    pub panes: Vec<(f64, f64)>,
    /// A door to the outside.
    pub outside: bool,
    /// The room the leaves swing into (index on the floor).
    pub room: usize,
}

/// The width of a glazed entrance's glass doors (m).
pub const GLASS_DOORS: f64 = 1.8;
/// Height of a glass door (m).
pub const GLASS_HEIGHT: f64 = 2.6;
/// Height of a door between rooms (m): under the cut-open walls' top.
pub const INNER_HEIGHT: f64 = 2.64;
/// Doorways wider than this between rooms stay open.
const WIDEST_INNER: f64 = 2.0;
/// Below this width a doorway has one leaf, above it two.
const DOUBLE_FROM: f64 = 1.4;
/// The hinge stands this far in from the opening's edge (clear of the
/// casings round it).
const HINGE_IN: f64 = 0.03;
/// Half the thickness of an outside and an inside wall (m), as collision
/// has them.
const OUTER_HALF: f64 = 0.15;
const INNER_HALF: f64 = 0.1;
/// Where inner leaves hang: just off the wall's face (its paper) on the
/// room's side.
const INNER_PLANE: f64 = 0.15;

/// The key of door `index` on floor `level` of a building.
pub const fn door_key(level: u32, index: usize) -> u32 {
    (level << 12) | index as u32
}

impl DoorLeaves {
    /// A point of the doorway in building (x, z): `t` along the wall from
    /// its centre, `d` from the wall line into the room.
    pub fn point(&self, t: f64, d: f64) -> DVec2 {
        self.at + self.along * t + self.into * d
    }
}

impl BuildingPlan {
    /// Every door that closes a doorway, floor by floor.
    pub fn door_leaves(&self) -> Vec<DoorLeaves> {
        let mut out = Vec::new();
        for f in &self.floors {
            for (i, d) in f.doors.iter().enumerate() {
                out.extend(leaves_of(f, i, d, self.look.reveal));
            }
        }
        out
    }

    /// The door with `key`, if that doorway has one.
    pub fn door(&self, key: u32) -> Option<DoorLeaves> {
        let f = self.floors.iter().find(|f| f.level == key >> 12)?;
        let i = (key & 0xFFF) as usize;
        leaves_of(f, i, f.doors.get(i)?, self.look.reveal)
    }
}

fn leaves_of(f: &Floor, index: usize, d: &Door, reveal: f64) -> Option<DoorLeaves> {
    let normal = if d.along_x { DVec2::Y } else { DVec2::X };
    let along = if d.along_x { DVec2::X } else { DVec2::Y };
    let (kind, room) = match d.b {
        None if d.size.x >= ENTRANCE_SIZE.x => (DoorKind::Glass, d.a),
        None => (DoorKind::Front, d.a),
        Some(b) => {
            if d.size.x > WIDEST_INNER {
                return None;
            }
            let into = if f.rooms[d.a].kind == RoomKind::Corridor {
                b
            } else {
                d.a
            };
            (DoorKind::Inner, into)
        }
    };
    let r = &f.rooms[room];
    if r.kind == RoomKind::Core {
        return None;
    }
    let into = if (r.rect.center() - d.at).dot(normal) > 0.0 {
        normal
    } else {
        -normal
    };
    let outside = d.b.is_none();
    let wall_half = if outside { OUTER_HALF } else { INNER_HALF };
    let width = match kind {
        DoorKind::Glass => GLASS_DOORS.min(d.size.x - 0.6),
        _ => d.size.x,
    };
    let half = width * 0.5;
    let double = width >= DOUBLE_FROM;
    let leaf_w = if double {
        half - HINGE_IN
    } else {
        width - 2.0 * HINGE_IN
    };
    // Leaves between rooms must swing within the room: the opening inside
    // its wall, and room in front of it for a leaf.
    let inner = r.rect.inset(wall_half + 0.05);
    let (lo, hi, depth) = if d.along_x {
        (inner.min.x, inner.max.x, inner.size().y)
    } else {
        (inner.min.y, inner.max.y, inner.size().x)
    };
    let mid = d.at.dot(along);
    // (A street door always gets its door: an open doorway there would
    // show the street a hole into the building.)
    if !outside && (mid - half < lo || mid + half > hi || depth < leaf_w + 0.1) {
        return None;
    }
    let plane = match kind {
        DoorKind::Inner => INNER_PLANE,
        // Behind the facade's reveal, not deeper than half of it.
        _ => (reveal * 0.5).clamp(0.03, 0.12),
    };
    let (height, top) = match kind {
        DoorKind::Glass => (GLASS_HEIGHT.min(d.size.y), d.size.y),
        DoorKind::Front => (d.size.y - 0.02, d.size.y),
        DoorKind::Inner => (INNER_HEIGHT, d.size.y),
    };
    let hinge = |s: f64| d.at + along * s * (half - HINGE_IN) + into * plane;
    let leaves = if double {
        [-1.0, 1.0]
            .into_iter()
            .map(|s| Leaf {
                hinge: hinge(s),
                shut: -along * s,
                open: into,
                width: leaf_w,
            })
            .collect()
    } else {
        // One leaf, hinged at the end nearer the room's corner, so it
        // opens against the wall.
        let corner = if (r.rect.center() - d.at).dot(along) > 0.0 {
            -1.0
        } else {
            1.0
        };
        vec![Leaf {
            hinge: hinge(corner),
            shut: -along * corner,
            open: into,
            width: leaf_w,
        }]
    };
    let panes = if kind == DoorKind::Glass {
        let w = d.size.x * 0.5;
        vec![(-w, -half), (half, w)]
    } else {
        Vec::new()
    };
    Some(DoorLeaves {
        key: door_key(f.level, index),
        level: f.level,
        kind,
        at: d.at,
        along,
        into,
        wall_half,
        plane,
        height,
        top,
        half,
        leaves,
        panes,
        outside,
        room,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::city::CityPlan;

    fn plans() -> Vec<BuildingPlan> {
        let city = CityPlan::generate(hw_core::Seed(3), &crate::city::CityParams::default());
        city.buildings
            .iter()
            .take(120)
            .map(|(_, s)| BuildingPlan::generate(s))
            .collect()
    }

    #[test]
    fn street_doors_and_entrances_get_doors_and_leaves_stay_in_their_room() {
        let mut front = 0;
        let mut glass = 0;
        let mut inner = 0;
        for p in plans() {
            for d in p.door_leaves() {
                match d.kind {
                    DoorKind::Front => front += 1,
                    DoorKind::Glass => glass += 1,
                    DoorKind::Inner => inner += 1,
                }
                assert_eq!(p.door(d.key).as_ref(), Some(&d), "found again by key");
                let f = p.floors.iter().find(|f| f.level == d.level).unwrap();
                let room = f.rooms[d.room].rect.inset(-1e-6);
                let inside = |p: DVec2| {
                    p.x >= room.min.x && p.x <= room.max.x && p.y >= room.min.y && p.y <= room.max.y
                };
                for l in &d.leaves {
                    // Open, the leaf's free edge stands in its room.
                    let tip = l.hinge + l.open * l.width;
                    assert!(d.outside || inside(tip), "leaf opens into its room");
                    // Shut, it stays in the doorway.
                    let shut = l.hinge + l.shut * l.width;
                    assert!((shut - d.at).dot(d.along).abs() <= d.half + 1e-9);
                }
            }
            // Every street door has one.
            for f in &p.floors {
                for (i, door) in f.doors.iter().enumerate() {
                    if door.b.is_none() {
                        assert!(p.door(door_key(f.level, i)).is_some(), "street door closes");
                    }
                }
            }
        }
        assert!(
            front > 20 && glass > 5 && inner > 50,
            "{front} {glass} {inner}"
        );
    }
}
