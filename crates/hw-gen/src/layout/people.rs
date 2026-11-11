//! Where people stand in a building at game scale: places on open floor,
//! clear of walls, furniture, stairs and doorways, a few to every room by
//! its use. Who stands there, and when, is the game's business.

use super::furnish::{items, Item};
use super::solid::{pieces, Part};
use super::{Layout, LinkKind, SpaceKind};
use crate::urban::footprint::Footprint;
use glam::{DVec2, DVec3};
use hw_core::Seed;

/// A place to stand.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stand {
    /// In the layout's frame: (x, floor height, z).
    pub at: DVec3,
    /// The way the one standing there looks.
    pub face: DVec2,
    /// The room it is in (an index into the layout's spaces).
    pub space: usize,
}

/// Candidate places lie on a grid this wide (m); a body needs this much
/// room to the next thing, and this much to a wall.
const GRID: f64 = 1.6;
const ROOM: f64 = 0.55;
const OFF_WALL: f64 = 0.9;
/// Nobody stands within this of the middle of a doorway.
const DOORWAY: f64 = 2.6;

/// Floor area to one place, by the room's use; None where nobody stands.
fn area_each(kind: SpaceKind) -> Option<f64> {
    match kind {
        SpaceKind::Hall | SpaceKind::Boardroom => Some(22.0),
        SpaceKind::Shop => Some(16.0),
        SpaceKind::Lobby | SpaceKind::Atrium | SpaceKind::Vestibule => Some(30.0),
        SpaceKind::Gallery | SpaceKind::Arcade | SpaceKind::Concourse => Some(40.0),
        SpaceKind::Bridge
        | SpaceKind::Landing
        | SpaceKind::StairHall
        | SpaceKind::Shaft
        | SpaceKind::Canyon
        | SpaceKind::Service => None,
    }
}

/// Every place to stand in the building, room by room.
pub fn standing(l: &Layout) -> Vec<Stand> {
    let solid = pieces(l);
    let furniture: Vec<(Item, Footprint)> = items(l)
        .into_iter()
        .filter_map(|i| i.footprint().map(|f| (i, f)))
        .collect();
    let seed = Seed(l.size.x.to_bits())
        .child(l.size.y.to_bits())
        .child(l.spaces.len() as u64)
        .child("standing");
    let mut out = Vec::new();
    for (i, s) in l.spaces.iter().enumerate() {
        let Some(each) = area_each(s.kind) else {
            continue;
        };
        if s.within.is_some() {
            continue;
        }
        // What stands in the room up to head height.
        let near = |f: &Footprint| {
            let (y0, y1) = f.span();
            let (c, r) = (f.at(), f.reach() + ROOM);
            y1 > s.y + 0.15
                && y0 < s.y + 1.9
                && c.x > s.rect.min.x - r
                && c.x < s.rect.max.x + r
                && c.y > s.rect.min.y - r
                && c.y < s.rect.max.y + r
        };
        let mut things: Vec<Footprint> = solid
            .iter()
            .filter(|p| !matches!(p.part, Part::Floor(_) | Part::Roof))
            .map(|p| p.shape)
            .filter(near)
            .collect();
        let mine: Vec<&(Item, Footprint)> = furniture.iter().filter(|(_, f)| near(f)).collect();
        things.extend(mine.iter().map(|(_, f)| *f));
        let doors: Vec<DVec2> = l
            .links
            .iter()
            .filter(|k| {
                (k.a == i || k.b == Some(i))
                    && (k.y - s.y).abs() < 0.5
                    && !matches!(k.kind, LinkKind::Rail | LinkKind::Glass)
            })
            .map(|k| k.at)
            .collect();
        let inner = s.rect.inset(OFF_WALL);
        let size = inner.size();
        if size.min_element() <= 0.0 {
            continue;
        }
        let room = seed.child(i as u64);
        let keep = (GRID * GRID / each).min(1.0);
        let (nx, nz) = ((size.x / GRID) as u32, (size.y / GRID) as u32);
        for ix in 0..=nx {
            for iz in 0..=nz {
                let h = room.child(((ix as u64) << 32) | iz as u64);
                if h.child("here").unit() >= keep {
                    continue;
                }
                let jitter = DVec2::new(h.child("x").unit(), h.child("z").unit()) - 0.5;
                let p = (inner.min + DVec2::new(ix as f64, iz as f64) * GRID + jitter * 0.5)
                    .clamp(inner.min, inner.max);
                if things.iter().any(|f| f.distance(p) < ROOM)
                    || doors.iter().any(|d| d.distance(p) < DOORWAY)
                {
                    continue;
                }
                // Turned to the nearest piece of furniture (at a desk, a
                // counter, a shelf), or any way at all.
                let by = mine
                    .iter()
                    .map(|(_, f)| (f.distance(p), f.at()))
                    .filter(|(d, _)| *d < 1.6)
                    .min_by(|a, b| a.0.total_cmp(&b.0));
                let face = match by {
                    Some((_, at)) => (at - p).normalize_or(DVec2::Y),
                    None => hw_core::det::dir2(h.child("turn").unit() * std::f64::consts::TAU),
                };
                out.push(Stand {
                    at: DVec3::new(p.x, s.y, p.y),
                    face,
                    space: i,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::office::plan_town;

    /// Halls, shops and lobbies all have places, on their floors, in
    /// their rooms and clear of everything solid.
    #[test]
    fn people_stand_on_open_floor() {
        for k in 0..6u64 {
            let l = plan_town(Seed(40 + k), DVec2::new(40.0 + 3.0 * k as f64, 46.0), 4);
            let stands = standing(&l);
            assert!(stands.len() > 60, "seed {k}: {} places", stands.len());
            let solid = pieces(&l);
            for st in &stands {
                let s = &l.spaces[st.space];
                let p = DVec2::new(st.at.x, st.at.z);
                assert_eq!(st.at.y, s.y);
                assert_eq!(p.clamp(s.rect.min, s.rect.max), p);
                assert!((st.face.length() - 1.0).abs() < 1e-9);
                for part in &solid {
                    if matches!(part.part, Part::Floor(_) | Part::Roof) {
                        continue;
                    }
                    let (y0, y1) = part.shape.span();
                    assert!(
                        y1 <= s.y + 0.15
                            || y0 >= s.y + 1.9
                            || part.shape.distance(p) >= ROOM - 1e-9,
                        "seed {k}: a place at {p:?} in {:?} is in a {:?}",
                        s.kind,
                        part.part
                    );
                }
            }
            for kind in [SpaceKind::Hall, SpaceKind::Shop] {
                assert!(
                    stands.iter().any(|st| l.spaces[st.space].kind == kind)
                        || !l.spaces.iter().any(|s| s.kind == kind)
                );
            }
            assert_eq!(stands, standing(&l));
        }
    }
}
