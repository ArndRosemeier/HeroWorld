//! A layout as solid parts: walls with their doorways, floor slabs, roofs,
//! steps, landings, rails, columns, glass. The game draws these very boxes
//! and bodies collide with them, so what is seen is what stops you.
//!
//! Parts are in the layout's own frame (x along the front, z inwards, y up
//! from the ground floor), as [`Footprint`]s with (x, z) for their plan.

use super::{Flight, Layout, Link, LinkKind, Space, SpaceKind, TREAD};
use crate::geom::Rect;
use crate::urban::footprint::Footprint;
use glam::DVec2;

/// Wall and slab thickness (m).
pub const WALL: f64 = 0.3;
pub const SLAB: f64 = 0.4;
/// Height of a balustrade (m).
pub const RAIL: f64 = 1.1;
/// Clear height of a door, a pair of doors, an arch and the entrance (m).
pub const DOOR_H: f64 = 3.2;
pub const DOUBLE_H: f64 = 3.8;
pub const ARCH_H: f64 = 5.0;
pub const ENTRANCE_H: f64 = 4.4;
/// Windows: one per bay this wide, this much glass, from this sill, to
/// this far under the ceiling (m).
pub const BAY: f64 = 3.6;
pub const PANE: f64 = 2.4;
pub const SILL: f64 = 1.1;
pub const HEAD: f64 = 0.9;
/// How thick a tread is (the steps are slabs: one can walk under a flight).
pub const TREAD_T: f64 = 0.5;

/// What a part is (the game picks its look by this).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// A wall between two rooms.
    Wall,
    /// An outside wall.
    Outer,
    /// A floor, with the kind of room standing on it.
    Floor(SpaceKind),
    Roof,
    /// A glass wall between rooms.
    Glass,
    /// A window in an outside wall.
    Window,
    Step,
    /// A stair landing, a bridge's or a gallery landing's deck.
    Deck,
    Rail,
    Column,
    /// A lift shaft's wall.
    Shaft,
    /// A lift's landing doors (shut until its car stands behind them).
    LiftDoor,
    /// The canopy over the entrance.
    Canopy,
    /// The sign fin standing up the front beside the entrance.
    Fin,
}

/// One solid part.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub part: Part,
    pub shape: Footprint,
}

fn boxed(part: Part, r: Rect, y0: f64, y1: f64) -> Piece {
    Piece {
        part,
        shape: Footprint::Box {
            at: r.center(),
            dir: DVec2::Y,
            half: r.size() * 0.5,
            y0,
            y1,
        },
    }
}

/// A hole in a wall: from `u0` to `u1` along it, from `v0` up to `v1`.
#[derive(Clone, Copy, Debug)]
struct Hole {
    u0: f64,
    u1: f64,
    v0: f64,
    v1: f64,
}

/// The wall on the line `w` (z if `along_x`, else x) from `lo` to `hi`
/// along it and `v0` to `v1` up, as boxes round its holes.
#[allow(clippy::too_many_arguments)]
fn wall(
    out: &mut Vec<Piece>,
    part: Part,
    thick: f64,
    along_x: bool,
    w: f64,
    (lo, hi): (f64, f64),
    (v0, v1): (f64, f64),
    holes: &[Hole],
) {
    let mut cuts = vec![lo, hi];
    for h in holes {
        cuts.push(h.u0.clamp(lo, hi));
        cuts.push(h.u1.clamp(lo, hi));
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    for c in cuts.windows(2) {
        let (a, b) = (c[0], c[1]);
        let mid = (a + b) * 0.5;
        // What is left of this strip between the holes over it.
        let mut gaps: Vec<(f64, f64)> = holes
            .iter()
            .filter(|h| h.u0 < mid && mid < h.u1)
            .map(|h| (h.v0.max(v0), h.v1.min(v1)))
            .filter(|g| g.1 > g.0)
            .collect();
        gaps.sort_by(|p, q| p.0.total_cmp(&q.0));
        let mut y = v0;
        let mut put = |y0: f64, y1: f64| {
            if y1 - y0 > 0.02 {
                let r = if along_x {
                    Rect::from_xz(a, w - thick * 0.5, b, w + thick * 0.5)
                } else {
                    Rect::from_xz(w - thick * 0.5, a, w + thick * 0.5, b)
                };
                out.push(boxed(part, r, y0, y1));
            }
        };
        for (g0, g1) in gaps {
            put(y, g0);
            y = y.max(g1);
        }
        put(y, v1);
    }
}

/// The edge two rectangles share: (along_x, wall line, from, to).
fn shared(r: &Rect, s: &Rect) -> Option<(bool, f64, f64, f64)> {
    let e = 1e-3;
    for (ra, sb) in [(r.max.y, s.min.y), (r.min.y, s.max.y)] {
        if (ra - sb).abs() < e {
            let (lo, hi) = (r.min.x.max(s.min.x), r.max.x.min(s.max.x));
            if hi > lo + e {
                return Some((true, ra, lo, hi));
            }
        }
    }
    for (ra, sb) in [(r.max.x, s.min.x), (r.min.x, s.max.x)] {
        if (ra - sb).abs() < e {
            let (lo, hi) = (r.min.y.max(s.min.y), r.max.y.min(s.max.y));
            if hi > lo + e {
                return Some((false, ra, lo, hi));
            }
        }
    }
    None
}

/// Where a link's opening lies along its wall.
fn span(l: &Link) -> (f64, f64) {
    let (a, b) = l.ends();
    if l.along_x {
        (a.x.min(b.x), a.x.max(b.x))
    } else {
        (a.y.min(b.y), a.y.max(b.y))
    }
}

fn door_height(k: LinkKind) -> f64 {
    match k {
        LinkKind::Door => DOOR_H,
        LinkKind::Double => DOUBLE_H,
        LinkKind::Arch => ARCH_H,
        LinkKind::Entrance => ENTRANCE_H,
        _ => f64::INFINITY,
    }
}

/// The roof's parapet (m), how far the entrance canopy reaches out and to
/// either side of the doors, and the sign fin: how far it stands out, how
/// thick it is and how far over the roof it rises.
pub const PARAPET: f64 = 1.1;
pub const CANOPY: (f64, f64) = (3.6, 2.0);
pub const FIN: (f64, f64, f64) = (2.6, 0.7, 5.0);

/// An opening in an outside wall that someone else decided (a town
/// building's facade): on the wall line `w` (z if `along_x`, else x), from
/// `u0` to `u1` along it and `v0` up to `v1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShellHole {
    pub along_x: bool,
    pub w: f64,
    pub u: (f64, f64),
    pub v: (f64, f64),
    /// A window: glass stands in it.
    pub glass: bool,
}

/// Every solid part of the layout.
pub fn pieces(l: &Layout) -> Vec<Piece> {
    build(l, None)
}

/// Every solid part of the layout inside a shell that has its own windows
/// and its own entrance: the outside walls are cut by `holes` instead of a
/// window per bay, and nothing is hung on the front.
pub fn pieces_in_shell(l: &Layout, holes: &[ShellHole]) -> Vec<Piece> {
    build(l, Some(holes))
}

fn build(l: &Layout, shell: Option<&[ShellHole]>) -> Vec<Piece> {
    let mut out = Vec::new();
    let top: Vec<(usize, &Space)> = l
        .spaces
        .iter()
        .enumerate()
        .filter(|(_, s)| s.within.is_none())
        .collect();
    let full = Rect::new(DVec2::ZERO, l.size);
    let root = |i: usize| l.spaces[i].within.unwrap_or(i);

    // Floors and roofs.
    for &(_, s) in &top {
        let (y0, y1) = if s.y < 1e-6 {
            (-0.2, 0.04)
        } else {
            (s.y - SLAB, s.y)
        };
        out.push(boxed(Part::Floor(s.kind), s.rect, y0, y1));
        let c = s.rect.center();
        let covered = top.iter().any(|(_, t)| {
            (t.y - s.top()).abs() < 1e-3
                && t.rect.min.x <= c.x
                && c.x <= t.rect.max.x
                && t.rect.min.y <= c.y
                && c.y <= t.rect.max.y
        });
        if !covered {
            out.push(boxed(Part::Roof, s.rect, s.top(), s.top() + SLAB));
        }
    }

    // Walls between rooms, with their doorways, rails and glass.
    for (i, &(a, sa)) in top.iter().enumerate() {
        for &(b, sb) in &top[i + 1..] {
            let Some((along_x, w, lo, hi)) = shared(&sa.rect, &sb.rect) else {
                continue;
            };
            let (v0, v1) = (sa.y.max(sb.y), sa.top().min(sb.top()));
            if v1 - v0 < 0.05 {
                continue;
            }
            let mut holes = Vec::new();
            for k in l.links.iter().filter(|k| {
                // A landing or bridge in a tall room opens through the
                // room's wall.
                let (p, q) = (root(k.a), k.b.map(root));
                ((p == a && q == Some(b)) || (p == b && q == Some(a)))
                    && k.y >= v0 - 1e-3
                    && k.y < v1
            }) {
                let (u0, u1) = span(k);
                match k.kind {
                    LinkKind::Glass => {
                        holes.push(Hole { u0, u1, v0, v1 });
                        wall(
                            &mut out,
                            Part::Glass,
                            0.1,
                            along_x,
                            w,
                            (u0, u1),
                            (v0, v1),
                            &[],
                        );
                    }
                    LinkKind::Rail => {
                        holes.push(Hole { u0, u1, v0, v1 });
                        wall(
                            &mut out,
                            Part::Rail,
                            0.12,
                            along_x,
                            w,
                            (u0, u1),
                            (k.y, k.y + RAIL),
                            &[],
                        );
                    }
                    LinkKind::Open => holes.push(Hole { u0, u1, v0, v1 }),
                    kind => holes.push(Hole {
                        u0,
                        u1,
                        v0: k.y,
                        v1: (k.y + door_height(kind)).min(v1 - 0.4),
                    }),
                }
            }
            wall(
                &mut out,
                Part::Wall,
                WALL,
                along_x,
                w,
                (lo, hi),
                (v0, v1),
                &holes,
            );
        }
    }

    // Outside walls with the entrances and a window in every bay.
    for &(a, s) in &top {
        let r = &s.rect;
        let edges = [
            (
                true,
                r.min.y,
                r.min.x,
                r.max.x,
                (r.min.y - full.min.y).abs() < 1e-3,
            ),
            (
                true,
                r.max.y,
                r.min.x,
                r.max.x,
                (r.max.y - full.max.y).abs() < 1e-3,
            ),
            (
                false,
                r.min.x,
                r.min.y,
                r.max.y,
                (r.min.x - full.min.x).abs() < 1e-3,
            ),
            (
                false,
                r.max.x,
                r.min.y,
                r.max.y,
                (r.max.x - full.max.x).abs() < 1e-3,
            ),
        ];
        for (along_x, w, lo, hi, outside) in edges {
            if !outside {
                continue;
            }
            let mut holes = Vec::new();
            let mut doors = Vec::new();
            for k in l.links.iter().filter(|k| {
                k.a == a && k.b.is_none() && k.along_x == along_x && {
                    let c = if along_x { k.at.y } else { k.at.x };
                    (c - w).abs() < 1e-3
                }
            }) {
                let (u0, u1) = span(k);
                doors.push((u0, u1));
                holes.push(Hole {
                    u0,
                    u1,
                    v0: k.y,
                    v1: k.y + door_height(k.kind),
                });
            }
            let glazed = !matches!(s.kind, SpaceKind::Service | SpaceKind::Shaft);
            let n = ((hi - lo) / BAY).floor() as i32;
            let (sill, head) = (s.y + SILL, s.top() - HEAD);
            for h in shell.unwrap_or(&[]).iter().filter(|h| {
                h.along_x == along_x
                    && (h.w - w).abs() < 1e-3
                    && h.u.0 < hi
                    && lo < h.u.1
                    && h.v.0 < s.top()
                    && s.y < h.v.1
            }) {
                let (u0, u1) = (h.u.0.max(lo), h.u.1.min(hi));
                let (v0, v1) = (h.v.0.max(s.y), h.v.1.min(s.top()));
                holes.push(Hole { u0, u1, v0, v1 });
                if h.glass {
                    wall(
                        &mut out,
                        Part::Window,
                        0.08,
                        along_x,
                        w,
                        (u0, u1),
                        (v0, v1),
                        &[],
                    );
                }
            }
            if shell.is_none() && glazed && n > 0 && head - sill > 1.0 {
                let bay = (hi - lo) / n as f64;
                for i in 0..n {
                    let c = lo + (i as f64 + 0.5) * bay;
                    let (u0, u1) = (c - PANE * 0.5, c + PANE * 0.5);
                    if doors.iter().any(|d| u0 < d.1 + 0.5 && d.0 - 0.5 < u1) {
                        continue;
                    }
                    holes.push(Hole {
                        u0,
                        u1,
                        v0: sill,
                        v1: head,
                    });
                    wall(
                        &mut out,
                        Part::Window,
                        0.08,
                        along_x,
                        w,
                        (u0, u1),
                        (sill, head),
                        &[],
                    );
                }
            }
            // Under the open sky the wall goes on up as the parapet.
            let c = s.rect.center();
            let roofed = !top
                .iter()
                .any(|(_, t)| (t.y - s.top()).abs() < 1e-3 && t.rect.contains_point(c));
            let up = if roofed { PARAPET } else { 0.0 };
            wall(
                &mut out,
                Part::Outer,
                WALL,
                along_x,
                w,
                (lo, hi),
                (s.y, s.top() + up),
                &holes,
            );
        }
    }

    // Bridges, landings in tall rooms, lift shafts.
    for (i, s) in l.spaces.iter().enumerate() {
        let Some(host) = s.within else { continue };
        match s.kind {
            SpaceKind::Shaft => shaft(&mut out, l, i),
            _ => {
                out.push(boxed(Part::Deck, s.rect, s.y - SLAB, s.y));
                deck_rails(&mut out, l, i, host);
            }
        }
    }

    // Stairs.
    for st in &l.stairs {
        for la in &st.landings {
            out.push(boxed(Part::Deck, la.rect, la.y - SLAB, la.y));
        }
        for f in &st.flights {
            steps(&mut out, f, &l.spaces[st.host].rect);
        }
    }

    // The entrance: a canopy on two posts, and the sign fin beside it.
    let main = l
        .links
        .iter()
        .filter(|k| k.kind == LinkKind::Entrance)
        .min_by(|a, b| {
            let mid = l.size.x * 0.5;
            (a.at.x - mid).abs().total_cmp(&(b.at.x - mid).abs())
        })
        .map(|k| k.at);
    for k in l
        .links
        .iter()
        .filter(|k| k.kind == LinkKind::Entrance && shell.is_none())
    {
        let (u0, u1) = span(k);
        let (reach, side) = CANOPY;
        let y = k.y + ENTRANCE_H + 0.5;
        out.push(boxed(
            Part::Canopy,
            Rect::from_xz(u0 - side, -reach, u1 + side, -WALL * 0.5),
            y,
            y + 0.45,
        ));
        for x in [u0 - side + 0.5, u1 + side - 0.5] {
            out.push(Piece {
                part: Part::Column,
                shape: Footprint::Disc {
                    at: DVec2::new(x, -reach + 0.5),
                    r: 0.22,
                    y0: k.y,
                    y1: y,
                },
            });
        }
        let (deep, thick, over) = FIN;
        let x = u1 + side + 1.2;
        if Some(k.at) == main && x + thick < l.size.x {
            out.push(boxed(
                Part::Fin,
                Rect::from_xz(x, -deep, x + thick, -WALL * 0.5),
                y + 0.45,
                l.height() + over,
            ));
        }
    }

    for c in &l.columns {
        out.push(Piece {
            part: Part::Column,
            shape: Footprint::Disc {
                at: c.at,
                r: c.r,
                y0: c.y,
                y1: c.y + c.h,
            },
        });
    }
    out
}

/// Clear height of a lift's landing doors, and how thick its shaft's
/// walls are (m).
pub const LIFT_DOOR_H: f64 = 3.0;
pub const SHAFT_WALL: f64 = 0.2;

/// A lift shaft: four walls just inside its outline, with a doorway at
/// every stop and the landing doors in it.
fn shaft(out: &mut Vec<Piece>, l: &Layout, i: usize) {
    let s = &l.spaces[i];
    let r = &s.rect;
    let glass = l
        .lifts
        .iter()
        .any(|k| k.shaft == i && k.kind == super::LiftKind::Glass);
    let part = if glass { Part::Glass } else { Part::Shaft };
    let t = SHAFT_WALL * 0.5;
    let edges = [
        (true, r.min.y, r.min.y + t, r.min.x, r.max.x),
        (true, r.max.y, r.max.y - t, r.min.x, r.max.x),
        (false, r.min.x, r.min.x + t, r.min.y, r.max.y),
        (false, r.max.x, r.max.x - t, r.min.y, r.max.y),
    ];
    for (along_x, edge, w, lo, hi) in edges {
        let mut holes = Vec::new();
        for k in l.links.iter().filter(|k| {
            (k.a == i || k.b == Some(i)) && k.along_x == along_x && {
                let c = if along_x { k.at.y } else { k.at.x };
                (c - edge).abs() < 1e-3
            }
        }) {
            let (u0, u1) = span(k);
            let h = Hole {
                u0,
                u1,
                v0: k.y,
                v1: k.y + LIFT_DOOR_H,
            };
            wall(
                out,
                Part::LiftDoor,
                0.08,
                along_x,
                w,
                (u0, u1),
                (h.v0, h.v1),
                &[],
            );
            holes.push(h);
        }
        wall(
            out,
            part,
            SHAFT_WALL,
            along_x,
            w,
            (lo, hi),
            (s.y, s.top()),
            &holes,
        );
    }
}

/// Rails round a bridge or landing `i` standing in `host`: along every
/// edge that looks into the void, open where a flight meets it, a link
/// leaves it or the host's wall is.
fn deck_rails(out: &mut Vec<Piece>, l: &Layout, i: usize, host: usize) {
    let s = &l.spaces[i];
    let hr = &l.spaces[host].rect;
    let r = &s.rect;
    let edges = [
        (
            true,
            r.min.y,
            r.min.x,
            r.max.x,
            (r.min.y - hr.min.y).abs() < 1e-3,
        ),
        (
            true,
            r.max.y,
            r.min.x,
            r.max.x,
            (r.max.y - hr.max.y).abs() < 1e-3,
        ),
        (
            false,
            r.min.x,
            r.min.y,
            r.max.y,
            (r.min.x - hr.min.x).abs() < 1e-3,
        ),
        (
            false,
            r.max.x,
            r.min.y,
            r.max.y,
            (r.max.x - hr.max.x).abs() < 1e-3,
        ),
    ];
    for (along_x, w, lo, hi, on_wall) in edges {
        if on_wall {
            continue;
        }
        let mut holes: Vec<Hole> = Vec::new();
        let mut gap = |u0: f64, u1: f64| {
            holes.push(Hole {
                u0,
                u1,
                v0: s.y,
                v1: s.y + RAIL,
            })
        };
        // Links on this edge (a lift's door, a way onto a gallery).
        for k in l.links.iter().filter(|k| {
            (k.a == i || k.b == Some(i)) && k.along_x == along_x && (k.y - s.y).abs() < 1e-3 && {
                let c = if along_x { k.at.y } else { k.at.x };
                (c - w).abs() < 1e-3
            }
        }) {
            let (u0, u1) = span(k);
            gap(u0, u1);
        }
        // Flights that start or end on this edge.
        for st in l.stairs.iter().filter(|st| st.host == host) {
            for f in &st.flights {
                let Flight::Straight {
                    from,
                    dir,
                    width,
                    risers,
                    ..
                } = *f
                else {
                    continue;
                };
                let foot = DVec2::new(from.x, from.z);
                let head = foot + dir * Flight::run(risers);
                for (p, y) in [(foot, f.y0()), (head, f.y1())] {
                    let (c, u) = if along_x { (p.y, p.x) } else { (p.x, p.y) };
                    if (y - s.y).abs() < 1e-3 && (c - w).abs() < 0.2 {
                        gap(u - width * 0.5, u + width * 0.5);
                    }
                }
            }
        }
        // Other nested spaces touching this edge at the same height (a
        // shaft beside a landing).
        for t in l.spaces.iter().filter(|t| t.within == Some(host)) {
            if let Some((ax, tw, a, b)) = shared(r, &t.rect) {
                if ax == along_x && (tw - w).abs() < 1e-3 && t.y <= s.y && t.top() > s.y {
                    gap(a, b);
                }
            }
        }
        wall(
            out,
            Part::Rail,
            0.12,
            along_x,
            w,
            (lo, hi),
            (s.y, s.y + RAIL),
            &holes,
        );
    }
}

/// How thick a stair's side rail is (m).
pub const STAIR_RAIL: f64 = 0.06;

/// One slab per tread, and a rail panel at each end of it that does not
/// run along the room's wall (so the balustrade climbs in steps).
fn steps(out: &mut Vec<Piece>, f: &Flight, host: &Rect) {
    let by_wall = |p: DVec2| {
        p.x - host.min.x < 0.4
            || host.max.x - p.x < 0.4
            || p.y - host.min.y < 0.4
            || host.max.y - p.y < 0.4
    };
    match *f {
        Flight::Straight {
            from,
            dir,
            width,
            risers,
            rise,
        } => {
            let a = DVec2::new(from.x, from.z);
            for k in 0..risers.saturating_sub(1) {
                let top = from.y + (k + 1) as f64 * rise;
                out.push(Piece {
                    part: Part::Step,
                    shape: Footprint::Box {
                        at: a + dir * ((k as f64 + 0.5) * TREAD),
                        dir,
                        half: DVec2::new(width * 0.5, TREAD * 0.5),
                        y0: top - TREAD_T,
                        y1: top,
                    },
                });
                for side in [-1.0, 1.0] {
                    let at = a
                        + dir * ((k as f64 + 0.5) * TREAD)
                        + dir.perp() * (side * (width - STAIR_RAIL) * 0.5);
                    if by_wall(at) {
                        continue;
                    }
                    out.push(Piece {
                        part: Part::Rail,
                        shape: Footprint::Box {
                            at,
                            dir,
                            half: DVec2::new(STAIR_RAIL * 0.5, TREAD * 0.5),
                            y0: top,
                            y1: top + RAIL,
                        },
                    });
                }
            }
        }
        Flight::Arc {
            centre,
            r_in,
            r_out,
            a0,
            sweep,
            y0,
            risers,
            rise,
        } => {
            let step = sweep / risers as f64;
            // Rings a metre or so wide, so a tread is a wedge, not a slab
            // as deep at the newel as at the wall.
            let rings = ((r_out - r_in) / 1.2).ceil().max(1.0) as u32;
            let ring = (r_out - r_in) / rings as f64;
            for k in 0..risers {
                let a = a0 + step * (k as f64 + 0.5);
                let radial = hw_core::det::dir2(a);
                let top = y0 + (k + 1) as f64 * rise;
                for j in 0..rings {
                    // The innermost ring reaches in a little: a straight
                    // edge cuts the corner of a round one.
                    let r0 = r_in + ring * j as f64 - if j == 0 { 0.08 } else { 0.0 };
                    let r1 = r_in + ring * (j + 1) as f64;
                    out.push(Piece {
                        part: Part::Step,
                        shape: Footprint::Box {
                            at: centre + radial * ((r0 + r1) * 0.5),
                            // Its depth runs round the arc, its width out
                            // along the radius.
                            dir: radial.perp(),
                            half: DVec2::new(
                                (r1 - r0) * 0.5,
                                r1 * hw_core::det::sin(step.abs() * 0.5)
                                    / hw_core::det::cos(step.abs() * 0.5),
                            ),
                            y0: top - TREAD_T,
                            y1: top,
                        },
                    });
                }
                // The outer rail (the inner side is the newel or a shaft).
                let at = centre + radial * (r_out - STAIR_RAIL * 0.5);
                if !by_wall(at) {
                    out.push(Piece {
                        part: Part::Rail,
                        shape: Footprint::Box {
                            at,
                            dir: radial.perp(),
                            half: DVec2::new(
                                STAIR_RAIL * 0.5,
                                r_out * hw_core::det::sin(step.abs() * 0.5),
                            ),
                            y0: top,
                            y1: top + RAIL,
                        },
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::office::plan_scheme;
    use crate::layout::Scheme;
    use hw_core::Seed;

    /// Is the point inside a solid part?
    fn solid_at(ps: &[Piece], p: DVec2, y: f64) -> bool {
        ps.iter().any(|q| {
            let (y0, y1) = q.shape.span();
            y > y0 && y < y1 && q.shape.distance(p) < 1e-9
        })
    }

    fn layouts() -> Vec<Layout> {
        let mut v = Vec::new();
        for (k, s) in [Scheme::GrandHall, Scheme::Atrium, Scheme::SplitLevel]
            .into_iter()
            .enumerate()
        {
            for seed in 0..6 {
                let size = DVec2::new(56.0, 52.0);
                v.extend(plan_scheme(Seed(seed * 3 + k as u64), size, 5, s));
            }
        }
        assert!(v.len() >= 12);
        v
    }

    #[test]
    fn doorways_are_open_and_walls_are_shut() {
        for l in layouts() {
            let ps = pieces(&l);
            let shaft_door = |k: &Link| {
                [Some(k.a), k.b]
                    .into_iter()
                    .flatten()
                    .any(|s| l.spaces[s].kind == SpaceKind::Shaft)
            };
            for k in &l.links {
                let n = if k.along_x { DVec2::Y } else { DVec2::X };
                let chest = k.y + 1.2;
                let open = !solid_at(&ps, k.at, chest)
                    && !solid_at(&ps, k.at + n * 0.1, chest)
                    && !solid_at(&ps, k.at - n * 0.1, chest);
                match k.kind {
                    LinkKind::Rail | LinkKind::Glass => {
                        assert!(solid_at(&ps, k.at, k.y + 0.5), "{:?} is not solid", k.kind)
                    }
                    _ if shaft_door(k) => {
                        assert!(
                            solid_at(&ps, k.at + n * 0.1, chest)
                                || solid_at(&ps, k.at - n * 0.1, chest)
                        )
                    }
                    _ => assert!(
                        open,
                        "{:?} at {:?} +{} between {:?} and {:?} is blocked by {:?}",
                        k.kind,
                        k.at,
                        k.y,
                        l.spaces[k.a],
                        k.b.map(|b| &l.spaces[b]),
                        ps.iter()
                            .filter(|q| {
                                let (y0, y1) = q.shape.span();
                                chest > y0 && chest < y1 && q.shape.distance(k.at) < 0.2
                            })
                            .collect::<Vec<_>>()
                    ),
                }
            }
            // The outside wall is shut away from the entrances (knee
            // height: under every window).
            let d = l.size.y;
            for i in 0..40 {
                let x = l.size.x * (i as f64 + 0.5) / 40.0;
                assert!(
                    solid_at(&ps, DVec2::new(x, d), 0.6),
                    "a hole in the back wall"
                );
            }
        }
    }

    #[test]
    fn every_floor_has_a_slab_and_every_step_a_tread() {
        for l in layouts() {
            let mut ps = pieces(&l);
            ps.retain(|q| q.part != Part::Rail);
            for s in l.spaces.iter().filter(|s| s.kind != SpaceKind::Shaft) {
                let c = s.rect.center();
                assert!(solid_at(&ps, c, s.y - 0.05), "no floor under {:?}", s.kind);
            }
            for st in &l.stairs {
                for f in &st.flights {
                    let fp = f.footprint();
                    for i in 0..20 {
                        for j in 0..20 {
                            let t = (DVec2::new(i as f64, j as f64) + 0.5) / 20.0;
                            let p = fp.min + fp.size() * t;
                            if let Some(h) = f.height_at(p) {
                                // The last riser steps onto the landing.
                                if (h - f.y1()).abs() < 1e-6 {
                                    continue;
                                }
                                assert!(
                                    solid_at(&ps, p, h - 0.05) && !solid_at(&ps, p, h + 0.3),
                                    "{:?}: no tread at {p:?} +{h:.2}",
                                    st.kind
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
