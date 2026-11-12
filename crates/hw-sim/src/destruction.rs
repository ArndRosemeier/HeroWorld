//! A breakable city: buildings that break element by element and collapse.
//!
//! Every building is planned by `hw-gen` as floors and rooms. Here that plan
//! becomes Engine's breakable [`Breakable`] plan ([`pieces`], [`breakable`]):
//! an outer wall panel per window bay per floor (split around doors, with a
//! lintel over each), slab tiles under every floor, roof tiles, the inner
//! walls in pieces, and columns where a big hall needs them. Each element has
//! a strength from the building's material and height, so a level 1 hero
//! punches through brick and glass but not through a concrete tower
//! ([`hero_power`]).
//!
//! The server owns the truth: the broken bits of every damaged building
//! ([`Damage`]). Engine's [`Building`] derives what fell, the collapse
//! (pancake or topple) and the rubble mounds from those bits alone, so a
//! client that applies the same bits sees the same building, and so does a
//! player who joins later. Collision follows: a damaged building's solids
//! become its standing elements plus its rubble ([`Obstacles::set_damage`]).
//!
//! Buildings nobody has been near for a while are repaired off camera
//! ([`REPAIR_SECONDS`], [`REPAIR_DISTANCE`]).
//!
//! Plans are made only for buildings that get hit, so a whole city costs
//! nothing until something breaks.

use crate::collision::{Obstacles, Solid, INNER_WALL, OUTER_WALL, SLAB};
use engine::building::{
    Building, BuildingPlan as Breakable, Element, ElementBits, ElementId, ElementKind,
    Floor as BreakableFloor,
};
use engine::color::Color;
use engine::glam::Vec3;
use engine::mesh::Mesh;
use glam::{DVec2, DVec3};
use hw_gen::building::{bay_lines, BuildingPlan, BuildingSite, Floor};
use hw_gen::geom::Rect;
use hw_gen::layout::solid::{self, Part};
use hw_gen::style::{Material, WindowKind};
use hw_gen::urban::footprint::Footprint;
use hw_net::{PlayerId, ToClient};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Slab and roof tiles are about this big (m); Engine's support rule counts
/// in tiles of this size.
pub const TILE: f64 = 3.0;
/// Inner walls are cut into pieces no longer than this (m).
pub const INNER_PIECE: f64 = 4.0;
/// A damaged building is repaired when nobody has come within this distance
/// (plus four times its height: tall ones are seen from further) for
/// [`REPAIR_SECONDS`].
pub const REPAIR_DISTANCE: f64 = 300.0;
pub const REPAIR_SECONDS: f64 = 600.0;
/// Inner walls carry little: their strength (before the material).
const INNER_STRENGTH: f32 = 90.0;

/// One breakable piece of a building, in the building's own space (x along
/// the front, z inwards, y up from its base).
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub kind: ElementKind,
    /// The storey it belongs to; slabs: the storey they are the floor of
    /// (the roof: one above the top storey).
    pub floor: u16,
    pub min: DVec3,
    pub max: DVec3,
    /// Outer panels: the facade's outward direction in building (x, z).
    pub outward: DVec2,
    pub strength: f32,
}

impl Piece {
    fn center(&self) -> DVec3 {
        (self.min + self.max) * 0.5
    }

    fn half(&self) -> DVec3 {
        (self.max - self.min) * 0.5
    }

    /// An inner wall (it holds slabs up, but is not part of the facade).
    pub fn is_inner_wall(&self) -> bool {
        self.kind == ElementKind::Wall && self.outward == DVec2::ZERO
    }
}

/// How hard a building's elements are: its main material, and taller
/// buildings are built stronger (steel frames).
pub fn material_factor(plan: &BuildingPlan) -> f32 {
    let base = match plan.look.material {
        Material::Stucco => 0.8,
        Material::Brick | Material::Terracotta => 1.0,
        Material::Limestone => 1.4,
        Material::Granite => 1.8,
        Material::Concrete => 2.2,
        Material::Glass => 2.4,
        Material::Metal => 2.6,
    };
    let tall = (1.0 + 0.05 * plan.floors.len().saturating_sub(1) as f32).min(2.0);
    base * tall
}

/// The punch and blast power of a hero of `level` (roughly kJ, Engine's
/// strength scale), and their reach. Each level hits 18% harder: a level 1
/// hero breaks glass, plaster and brick; concrete takes about level 6.
pub fn hero_power(level: u8) -> HeroPower {
    let k = 1.18f32.powi(level.max(1) as i32 - 1);
    HeroPower {
        punch: 450.0 * k,
        punch_radius: 1.3,
        blast: 650.0 * k,
        blast_radius: 2.2,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeroPower {
    pub punch: f32,
    pub punch_radius: f32,
    pub blast: f32,
    pub blast_radius: f32,
}

/// The breakable pieces of a building, in a fixed order (the order is the
/// element numbering every machine shares).
pub fn pieces(plan: &BuildingPlan) -> Vec<Piece> {
    if let Some(parts) = plan.solids() {
        let mut out: Vec<Piece> = halls_cut(plan, &parts).into_iter().map(|c| c.1).collect();
        hang_unheld(&mut out, plan.floors.len());
        return out;
    }
    let m = material_factor(plan);
    let mut out = Vec::new();
    let n = plan.floors.len();
    for (k, f) in plan.floors.iter().enumerate() {
        let y0 = f.elevation;
        let y1 = f.elevation + f.height - SLAB;
        if k > 0 {
            // The slab under this floor covers the floor below (its terrace
            // too).
            let below = plan.floors[k - 1].outline;
            tiles(
                &mut out,
                below,
                y0 - SLAB,
                y0,
                ElementKind::Slab,
                k as u16,
                m,
            );
        }
        outer_walls(&mut out, plan, f, k as u16, y0, y1, m);
        inner_walls(&mut out, f, k as u16, y0, y1, m);
    }
    if let Some(top) = plan.floors.last() {
        let y = top.elevation + top.height;
        tiles(
            &mut out,
            top.outline,
            y - SLAB,
            y,
            ElementKind::Roof,
            n as u16,
            m,
        );
    }
    add_columns(&mut out, plan, m);
    out
}

/// The ground floor of a building of halls does not break: there is
/// nothing drawn under it.
const GROUND_STRENGTH: f32 = 1.0e9;

/// The parts of a building of halls ([`BuildingPlan::solids`], or the
/// same parts as they are drawn) cut into breakable pieces, in a fixed
/// order: each piece with the part of the building it is, so what breaks
/// is what stands there and what is drawn. Outside walls and windows are
/// the storeys' load-bearing ring, inner and shaft walls and columns hold
/// the floors up, floors and roofs are slab tiles; stairs, decks, rails
/// and glass carry nothing and fall with nothing but their storey. Long
/// walls are cut every [`INNER_PIECE`], floors into tiles of about
/// [`TILE`], and everything where a storey ends. The lifts' landing doors
/// are left out (they move with their cars).
pub fn halls_cut(plan: &BuildingPlan, parts: &[solid::Piece]) -> Vec<(solid::Piece, Piece)> {
    let m = material_factor(plan);
    let n = plan.floors.len();
    let size = plan.site.size;
    let top = plan.floors.last().map_or(0.0, |f| f.elevation + f.height);
    // The storey a height lies in.
    let storey = |y: f64| {
        plan.floors
            .iter()
            .rposition(|f| f.elevation <= y + 1e-3)
            .unwrap_or(0) as u16
    };
    // The storey a slab at `y` is the floor of (the roof: one past the top).
    let level = |y: f64| {
        if y > top - 1e-3 {
            n as u16
        } else {
            storey(y)
        }
    };
    let slab = ElementKind::Slab.default_strength() * m;
    let mut out = Vec::new();
    for p in parts {
        let (y0, y1) = p.shape.span();
        let (x0, x1) = p.shape.extent(DVec2::ZERO, DVec2::X);
        let (z0, z1) = p.shape.extent(DVec2::ZERO, DVec2::Y);
        let along_x = x1 - x0 >= z1 - z0;
        // (kind, strength, the longest piece along the part, tiles?, cut
        // at storeys?, on the facade?)
        let (kind, strength, len, tiled, storeys, facade) = match p.part {
            Part::LiftDoor => continue,
            Part::Outer => {
                let k = ElementKind::Wall;
                (k, k.default_strength() * m, INNER_PIECE, false, true, true)
            }
            Part::Window => {
                let k = ElementKind::Window;
                (
                    k,
                    k.default_strength() * m,
                    f64::INFINITY,
                    false,
                    true,
                    true,
                )
            }
            Part::Floor(_) if y0 < 1e-6 => {
                (ElementKind::Slab, GROUND_STRENGTH, TILE, true, false, false)
            }
            Part::Floor(_) => (ElementKind::Slab, slab, TILE, true, false, false),
            Part::Roof => {
                let k = ElementKind::Roof;
                (k, k.default_strength() * m, TILE, true, false, false)
            }
            Part::Wall | Part::Shaft => {
                let k = ElementKind::Wall;
                (k, k.default_strength() * m, INNER_PIECE, false, true, false)
            }
            Part::Column => {
                let k = ElementKind::Column;
                (
                    k,
                    k.default_strength() * m,
                    f64::INFINITY,
                    false,
                    false,
                    false,
                )
            }
            Part::Glass => (
                ElementKind::Partition,
                ElementKind::Window.default_strength() * m,
                INNER_PIECE,
                false,
                true,
                false,
            ),
            Part::Rail => {
                let k = ElementKind::Partition;
                (
                    k,
                    k.default_strength() * m,
                    INNER_PIECE,
                    false,
                    false,
                    false,
                )
            }
            Part::Deck => (ElementKind::Partition, slab, TILE, true, false, false),
            Part::Step | Part::Canopy | Part::Fin => (
                ElementKind::Partition,
                slab,
                f64::INFINITY,
                false,
                false,
                false,
            ),
        };
        let outward = if !facade {
            DVec2::ZERO
        } else if along_x {
            DVec2::new(
                0.0,
                if (z0 + z1) * 0.5 < size.y * 0.5 {
                    -1.0
                } else {
                    1.0
                },
            )
        } else {
            DVec2::new(
                if (x0 + x1) * 0.5 < size.x * 0.5 {
                    -1.0
                } else {
                    1.0
                },
                0.0,
            )
        };
        // Only a box standing square to the building can be cut.
        let square = matches!(p.shape, Footprint::Box { dir, .. } if dir.x.abs() < 1e-9 || dir.y.abs() < 1e-9);
        let even = |lo: f64, hi: f64, count: usize| -> Vec<(f64, f64)> {
            let step = (hi - lo) / count as f64;
            (0..count)
                .map(|k| (lo + step * k as f64, lo + step * (k + 1) as f64))
                .collect()
        };
        let whole = |lo: f64, hi: f64| vec![(lo, hi)];
        let by_len =
            |lo: f64, hi: f64| even(lo, hi, ((hi - lo) / len - 1e-9).ceil().max(1.0) as usize);
        let by_tile = |lo: f64, hi: f64| even(lo, hi, ((hi - lo) / len).round().max(1.0) as usize);
        let (xs, zs) = match (square, tiled, along_x) {
            (false, _, _) => (whole(x0, x1), whole(z0, z1)),
            (true, true, _) => (by_tile(x0, x1), by_tile(z0, z1)),
            (true, false, true) => (by_len(x0, x1), whole(z0, z1)),
            (true, false, false) => (whole(x0, x1), by_len(z0, z1)),
        };
        let mut ys = vec![y0];
        if square && storeys {
            ys.extend(
                plan.floors
                    .iter()
                    .map(|f| f.elevation)
                    .filter(|e| *e > y0 + 0.05 && *e < y1 - 0.05),
            );
        }
        ys.push(y1);
        let one = xs.len() * zs.len() * (ys.len() - 1) == 1;
        for y in ys.windows(2) {
            let floor = match p.part {
                Part::Floor(_) => level(y[1]),
                Part::Roof => level(y[0]),
                _ => storey((y[0] + y[1]) * 0.5),
            };
            for &(xa, xb) in &xs {
                for &(za, zb) in &zs {
                    if xb - xa < 1e-3 || zb - za < 1e-3 || y[1] - y[0] < 1e-3 {
                        continue;
                    }
                    let shape = if one {
                        p.shape
                    } else {
                        Footprint::Box {
                            at: DVec2::new((xa + xb) * 0.5, (za + zb) * 0.5),
                            dir: DVec2::Y,
                            half: DVec2::new((xb - xa) * 0.5, (zb - za) * 0.5),
                            y0: y[0],
                            y1: y[1],
                        }
                    };
                    out.push((
                        solid::Piece {
                            part: p.part,
                            shape,
                        },
                        Piece {
                            kind,
                            floor,
                            min: DVec3::new(xa, y[0], za),
                            max: DVec3::new(xb, y[1], zb),
                            outward,
                            strength,
                        },
                    ));
                }
            }
        }
    }
    out
}

/// Slab and roof tiles that nothing holds up by Engine's rule (the middle
/// of a wide hall's ceiling, a floor over a room wider than itself) hang
/// from the frame instead: they carry nothing and never fall on their
/// own, and break as hard as before. A building of halls gets no columns
/// that are not drawn.
fn hang_unheld(out: &mut [Piece], storeys: usize) {
    for s in 1..=storeys {
        for k in unheld(out, s) {
            out[k].kind = ElementKind::Partition;
        }
    }
}

fn tiles(out: &mut Vec<Piece>, r: Rect, y0: f64, y1: f64, kind: ElementKind, floor: u16, m: f32) {
    let s = r.size();
    if s.x < 0.05 || s.y < 0.05 {
        return;
    }
    let nx = (s.x / TILE).round().max(1.0) as usize;
    let nz = (s.y / TILE).round().max(1.0) as usize;
    let (tx, tz) = (s.x / nx as f64, s.y / nz as f64);
    for i in 0..nx {
        for j in 0..nz {
            let x0 = r.min.x + tx * i as f64;
            let z0 = r.min.y + tz * j as f64;
            out.push(Piece {
                kind,
                floor,
                min: DVec3::new(x0, y0, z0),
                max: DVec3::new(x0 + tx, y1, z0 + tz),
                outward: DVec2::ZERO,
                strength: kind.default_strength() * m,
            });
        }
    }
}

/// Door gaps on a wall line: (from, to, height) along the line.
fn door_gaps(f: &Floor, along_x: bool, fixed: f64) -> Vec<(f64, f64, f64)> {
    let mut gaps: Vec<(f64, f64, f64)> = f
        .doors
        .iter()
        .filter(|d| d.along_x == along_x)
        .filter(|d| ((if along_x { d.at.y } else { d.at.x }) - fixed).abs() < 1e-6)
        .map(|d| {
            let c = if along_x { d.at.x } else { d.at.y };
            (c - d.size.x * 0.5, c + d.size.x * 0.5, d.size.y)
        })
        .collect();
    gaps.sort_by(|a, b| a.0.total_cmp(&b.0));
    gaps
}

/// A stretch of wall from `a` to `b` along a line, minus its doors: the
/// pieces beside the doors, and a lintel over each door.
#[allow(clippy::too_many_arguments)]
fn wall_run(
    out: &mut Vec<Piece>,
    (a, b): (f64, f64),
    gaps: &[(f64, f64, f64)],
    along_x: bool,
    fixed: f64,
    half: f64,
    (y0, y1): (f64, f64),
    kind: ElementKind,
    floor: u16,
    outward: DVec2,
    strength: f32,
    max_len: f64,
) {
    let push = |t0: f64, t1: f64, ya: f64, yb: f64, out: &mut Vec<Piece>| {
        if t1 - t0 < 0.05 || yb - ya < 0.1 {
            return;
        }
        // Long stretches are cut into equal pieces.
        let n = ((t1 - t0) / max_len).ceil().max(1.0) as usize;
        let step = (t1 - t0) / n as f64;
        for k in 0..n {
            let (p0, p1) = (t0 + step * k as f64, t0 + step * (k + 1) as f64);
            let (min, max) = if along_x {
                (
                    DVec3::new(p0, ya, fixed - half),
                    DVec3::new(p1, yb, fixed + half),
                )
            } else {
                (
                    DVec3::new(fixed - half, ya, p0),
                    DVec3::new(fixed + half, yb, p1),
                )
            };
            out.push(Piece {
                kind,
                floor,
                min,
                max,
                outward,
                strength,
            });
        }
    };
    let mut t = a;
    for &(g0, g1, h) in gaps {
        if g1 <= a || g0 >= b {
            continue;
        }
        if g0 > t {
            push(t, g0, y0, y1, out);
        }
        // The lintel over the door.
        push(g0.max(a), g1.min(b), y0 + h, y1, out);
        t = t.max(g1);
    }
    if t < b {
        push(t, b, y0, y1, out);
    }
}

fn outer_walls(
    out: &mut Vec<Piece>,
    plan: &BuildingPlan,
    f: &Floor,
    k: u16,
    y0: f64,
    y1: f64,
    m: f32,
) {
    let o = f.outline;
    let h = OUTER_WALL * 0.5;
    let kind = if plan.look.window == WindowKind::Curtain {
        ElementKind::Window
    } else {
        ElementKind::Wall
    };
    let strength = kind.default_strength() * m;
    // Front and back run the full width over the corners; the sides fit
    // between them, so corner pieces meet instead of overlapping.
    let sides = [
        (true, o.min.y, o.min.x, o.max.x, h, DVec2::new(0.0, -1.0)),
        (false, o.max.x, o.min.y, o.max.y, -h, DVec2::new(1.0, 0.0)),
        (true, o.max.y, o.min.x, o.max.x, h, DVec2::new(0.0, 1.0)),
        (false, o.min.x, o.min.y, o.max.y, -h, DVec2::new(-1.0, 0.0)),
    ];
    for (along_x, fixed, t0, t1, ext, outward) in sides {
        let lines = bay_lines(t0, t1, plan.look.bay.max(1.0));
        let gaps = door_gaps(f, along_x, fixed);
        let last = lines.len().saturating_sub(2);
        for (i, w) in lines.windows(2).enumerate() {
            let a = if i == 0 { w[0] - ext } else { w[0] };
            let b = if i == last { w[1] + ext } else { w[1] };
            wall_run(
                out,
                (a, b),
                &gaps,
                along_x,
                fixed,
                h,
                (y0, y1),
                kind,
                k,
                outward,
                strength,
                f64::INFINITY,
            );
        }
    }
}

/// A wall line inside a floor: where it lies across, and the stretches of
/// room edge along it.
type WallLine = (f64, Vec<(f64, f64)>);

fn inner_walls(out: &mut Vec<Piece>, f: &Floor, k: u16, y0: f64, y1: f64, m: f32) {
    let o = f.outline;
    let eps = 1e-6;
    // Every room edge inside the outline, merged per wall line.
    let mut lines: BTreeMap<(bool, i64), WallLine> = BTreeMap::new();
    for room in &f.rooms {
        let r = room.rect;
        let edges = [
            (true, r.min.y, r.min.x, r.max.x),
            (true, r.max.y, r.min.x, r.max.x),
            (false, r.min.x, r.min.y, r.max.y),
            (false, r.max.x, r.min.y, r.max.y),
        ];
        for (along_x, fixed, t0, t1) in edges {
            let outer = if along_x {
                (fixed - o.min.y).abs() < eps || (fixed - o.max.y).abs() < eps
            } else {
                (fixed - o.min.x).abs() < eps || (fixed - o.max.x).abs() < eps
            };
            if outer {
                continue;
            }
            let key = (along_x, (fixed * 1000.0).round() as i64);
            lines
                .entry(key)
                .or_insert_with(|| (fixed, Vec::new()))
                .1
                .push((t0, t1));
        }
    }
    let strength = INNER_STRENGTH * m.sqrt();
    let h = INNER_WALL * 0.5;
    let oh = OUTER_WALL * 0.5;
    for ((along_x, _), (fixed, mut spans)) in lines {
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut merged: Vec<(f64, f64)> = Vec::new();
        for (a, b) in spans {
            match merged.last_mut() {
                Some(last) if a <= last.1 + eps => last.1 = last.1.max(b),
                _ => merged.push((a, b)),
            }
        }
        // Stop at the inner face of the outer walls.
        let (lo, hi) = if along_x {
            (o.min.x + oh, o.max.x - oh)
        } else {
            (o.min.y + oh, o.max.y - oh)
        };
        let gaps = door_gaps(f, along_x, fixed);
        for (a, b) in merged {
            let (a, b) = (a.max(lo), b.min(hi));
            if b - a < 0.05 {
                continue;
            }
            wall_run(
                out,
                (a, b),
                &gaps,
                along_x,
                fixed,
                h,
                (y0, y1),
                ElementKind::Wall,
                k,
                DVec2::ZERO,
                strength,
                INNER_PIECE,
            );
        }
    }
}

/// The box of a piece as Engine sees it (slab tiles a hair smaller, so
/// neighbours do not share faces).
fn element_box(p: &Piece) -> (DVec3, DVec3) {
    let mut half = p.half();
    if matches!(p.kind, ElementKind::Slab | ElementKind::Roof) {
        half.x -= 0.004;
        half.z -= 0.004;
    }
    (p.center(), half)
}

/// The slab tiles of storey `s` (`out` indices) that nothing holds up by
/// Engine's rule: a tile is held within 0.6 m of a wall, window or column
/// of the storey below, or two tiles from a held tile.
fn unheld(out: &[Piece], s: usize) -> Vec<usize> {
    let holds = |p: &Piece| {
        matches!(
            p.kind,
            ElementKind::Wall | ElementKind::Window | ElementKind::Column
        )
    };
    let gap2 = |a: &(DVec3, DVec3), b: &(DVec3, DVec3)| {
        DVec2::new(
            (a.0.x - b.0.x).abs() - (a.1.x + b.1.x),
            (a.0.z - b.0.z).abs() - (a.1.z + b.1.z),
        )
    };
    let reach = 0.6;
    let ids: Vec<usize> = (0..out.len())
        .filter(|&i| slab_like(out[i].kind) && out[i].floor as usize == s)
        .collect();
    let tiles: Vec<(DVec3, DVec3)> = ids.iter().map(|&i| element_box(&out[i])).collect();
    let holders: Vec<(DVec3, DVec3)> = out
        .iter()
        .filter(|p| holds(p) && p.floor as usize + 1 == s)
        .map(element_box)
        .collect();
    let mut dist = vec![u8::MAX; tiles.len()];
    let mut queue = std::collections::VecDeque::new();
    for (k, t) in tiles.iter().enumerate() {
        if holders
            .iter()
            .any(|h| gap2(h, t).max(DVec2::ZERO).length() < reach)
        {
            dist[k] = 0;
            queue.push_back(k);
        }
    }
    while let Some(k) = queue.pop_front() {
        if dist[k] >= 2 {
            continue;
        }
        for m in 0..tiles.len() {
            if dist[m] == u8::MAX && gap2(&tiles[k], &tiles[m]).max_element() < 0.05 {
                dist[m] = dist[k] + 1;
                queue.push_back(m);
            }
        }
    }
    (0..tiles.len())
        .filter(|&k| dist[k] == u8::MAX)
        .map(|k| ids[k])
        .collect()
}

/// Columns wherever a slab tile would have nothing holding it up, so an
/// undamaged building stands by Engine's rules (big halls, wide lobbies).
fn add_columns(out: &mut Vec<Piece>, plan: &BuildingPlan, m: f32) {
    for s in 1..=plan.floors.len() {
        let below = s - 1;
        let Some(storey) = plan.floors.get(below) else {
            continue;
        };
        while let Some(&k) = unheld(out, s).first() {
            let c = out[k].center();
            let y0 = storey.elevation;
            let y1 = storey.elevation + storey.height - SLAB;
            out.push(Piece {
                kind: ElementKind::Column,
                floor: below as u16,
                min: DVec3::new(c.x - 0.3, y0, c.z - 0.3),
                max: DVec3::new(c.x + 0.3, y1, c.z + 0.3),
                outward: DVec2::ZERO,
                strength: ElementKind::Column.default_strength() * m,
            });
        }
    }
}

fn slab_like(k: ElementKind) -> bool {
    matches!(k, ElementKind::Slab | ElementKind::Roof)
}

/// Building space and Engine's building space. Engine places a building
/// with a turn about +y, which keeps handedness; a site whose inward
/// direction is mirrored gets its z flipped.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub origin: DVec2,
    pub along: DVec2,
    pub inward: DVec2,
    pub base: f64,
    pub mirrored: bool,
}

impl Frame {
    pub fn of(site: &BuildingSite, base: f64) -> Self {
        Self {
            origin: site.origin,
            along: site.along,
            inward: site.inward,
            base,
            mirrored: site.inward.dot(site.along.perp()) < 0.0,
        }
    }

    fn z(&self) -> f64 {
        if self.mirrored {
            -1.0
        } else {
            1.0
        }
    }

    /// Building space → Engine's building space.
    pub fn to_engine(&self, p: DVec3) -> Vec3 {
        Vec3::new(p.x as f32, p.y as f32, (p.z * self.z()) as f32)
    }

    /// A direction in building (x, z) → Engine's building space.
    pub fn dir_to_engine(&self, d: DVec2) -> Vec3 {
        Vec3::new(d.x as f32, 0.0, (d.y * self.z()) as f32)
    }

    /// Engine's building space → building space.
    pub fn from_engine(&self, p: Vec3) -> DVec3 {
        DVec3::new(p.x as f64, p.y as f64, p.z as f64 * self.z())
    }

    /// A world point (local frame) → building space.
    pub fn to_building(&self, p: DVec3) -> DVec3 {
        let d = DVec2::new(p.x, p.z) - self.origin;
        DVec3::new(d.dot(self.along), p.y - self.base, d.dot(self.inward))
    }

    /// Building space → world (local frame).
    pub fn to_world(&self, p: DVec3) -> DVec3 {
        let g = self.origin + self.along * p.x + self.inward * p.z;
        DVec3::new(g.x, p.y + self.base, g.y)
    }

    /// Engine's turn for this building: +x goes along the front.
    pub fn yaw(&self) -> f64 {
        hw_core::det::atan2(-self.along.y, self.along.x)
    }
}

/// Engine's breakable plan for a building. `dress` gives an element its look
/// (in Engine's building space, see [`Frame`]); `None` draws a grey box,
/// which is all a server needs.
pub fn breakable(
    plan: &BuildingPlan,
    pieces: &[Piece],
    frame: &Frame,
    mut dress: impl FnMut(usize, &Piece, &Element) -> Option<Mesh>,
) -> engine::error::EngineResult<Breakable> {
    breakable_textured(plan, pieces, frame, &[], |i, p, e| {
        dress(i, p, e).map(|m| vec![(m, None)]).unwrap_or_default()
    })
}

/// [`breakable`] with textures: `textures` are meshes carrying the albedo
/// and maps (registered on the plan in that order), and `dress` gives each
/// element its meshes, each with the index of its texture (`None`: vertex
/// colours only). An element with no meshes is a grey box (in a building
/// of halls it shows nothing).
pub fn breakable_textured(
    plan: &BuildingPlan,
    pieces: &[Piece],
    frame: &Frame,
    textures: &[&Mesh],
    mut dress: impl FnMut(usize, &Piece, &Element) -> Vec<(Mesh, Option<usize>)>,
) -> engine::error::EngineResult<Breakable> {
    let floors = plan
        .floors
        .iter()
        .map(|f| BreakableFloor {
            y0: f.elevation as f32,
            y1: (f.elevation + f.height) as f32,
        })
        .collect();
    let mut out = Breakable::new(floors, plan.site.seed.0)?;
    let mut registered = Vec::with_capacity(textures.len());
    for t in textures {
        registered.push(out.add_texture_of(t)?);
    }
    let grey = Color::rgb01_unchecked(0.6, 0.6, 0.6);
    let mut elements = Vec::with_capacity(pieces.len());
    for (i, p) in pieces.iter().enumerate() {
        let (c, h) = element_box(p);
        let mut e = Element::new(
            p.kind,
            p.floor,
            frame.to_engine(c),
            Vec3::new(h.x as f32, h.y as f32, h.z as f32),
        )
        .with_strength(p.strength);
        if p.outward != DVec2::ZERO {
            e = e.with_outward(frame.dir_to_engine(p.outward));
        }
        let mut parts = dress(i, p, &e);
        // (A building of halls is drawn from its own parts or not at all:
        // a piece nothing is drawn of shows nothing.)
        if parts.is_empty() && plan.layout.is_none() {
            parts.push((Mesh::box_at(e.center, e.half * 2.0, grey)?, None));
        }
        elements.push((e, parts));
    }
    out.add_all(elements.iter().map(|(e, parts)| {
        let parts = parts
            .iter()
            .map(|(m, t)| (m, t.and_then(|t| registered.get(t).copied().flatten())))
            .collect();
        (e.clone(), parts)
    }))?;
    Ok(out)
}

/// What a hit did to the city.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HitResult {
    /// Elements broken directly, over all buildings.
    pub broken: usize,
    /// Elements that came down with them (unsupported or collapsed).
    pub fell: usize,
    /// Buildings that started to collapse.
    pub collapsed: Vec<u64>,
}

/// One damaged building.
#[derive(Clone, Debug)]
pub struct Damaged {
    pub state: Building,
    pub frame: Frame,
    /// The last tick someone was near enough to see it.
    pub last_seen: u64,
}

/// The city's damage: every damaged building's broken bits, and the news
/// for clients. The server runs it with [`Damage::hit`]; a client mirrors it
/// with [`Damage::apply`] (for its collision; Engine draws its own copy).
#[derive(Debug, Default)]
pub struct Damage {
    plans: BTreeMap<u64, Arc<Breakable>>,
    pub buildings: BTreeMap<u64, Damaged>,
    /// News for clients since the last [`Damage::take_news`].
    news: Vec<ToClient>,
    /// Collateral: what the broken elements cost, per player who broke them.
    pub collateral: BTreeMap<PlayerId, f64>,
}

/// What breaking an element costs the city (credits; for the collateral
/// tally incidents will use).
fn element_cost(kind: ElementKind) -> f64 {
    match kind {
        ElementKind::Window => 900.0,
        ElementKind::Partition => 400.0,
        ElementKind::Wall => 2_500.0,
        ElementKind::Column => 6_000.0,
        ElementKind::Slab | ElementKind::Roof => 3_000.0,
    }
}

impl Damage {
    /// The breakable plan of building `i` (made on first use).
    pub fn plan(&mut self, solids: &Obstacles, i: u64) -> Option<(Arc<Breakable>, Frame)> {
        let (plan, base) = solids.building(i)?;
        let frame = Frame::of(&plan.site, base);
        if let Some(p) = self.plans.get(&i) {
            return Some((p.clone(), frame));
        }
        let pieces = pieces(&plan);
        let b = Arc::new(breakable(&plan, &pieces, &frame, |_, _, _| None).ok()?);
        self.plans.insert(i, b.clone());
        Some((b, frame))
    }

    /// Hits the city at `at` (world, local frame): every building within
    /// reach takes the blow. `by`: who struck (for the collateral tally).
    pub fn hit(
        &mut self,
        solids: &Obstacles,
        at: DVec3,
        radius: f32,
        power: f32,
        by: Option<PlayerId>,
        tick: u64,
    ) -> HitResult {
        let mut result = HitResult::default();
        let r = radius as f64;
        for i in solids.buildings_near(DVec2::new(at.x, at.z), r) {
            let Some((plan, base)) = solids.building(i) else {
                continue;
            };
            let frame = Frame::of(&plan.site, base);
            let local = frame.to_building(at);
            let s = plan.site.size;
            if local.x < -r
                || local.z < -r
                || local.x > s.x + r
                || local.z > s.y + r
                || local.y < -r
                || local.y > plan.height() + r
            {
                continue;
            }
            let Some((bp, frame)) = self.plan(solids, i) else {
                continue;
            };
            let entry = self.buildings.entry(i).or_insert_with(|| Damaged {
                state: Building::new(bp.clone()),
                frame,
                last_seen: tick,
            });
            let change = entry.state.hit(frame.to_engine(local), radius, power);
            entry.last_seen = tick;
            if change.broken.is_empty() {
                if entry.state.broken().count() == 0 {
                    self.buildings.remove(&i);
                }
                continue;
            }
            result.broken += change.broken.len();
            result.fell += change.dropped.len() + change.collapsed.len();
            if change.collapse.is_some() {
                result.collapsed.push(i);
            }
            let cost: f64 = change
                .broken
                .iter()
                .chain(&change.dropped)
                .chain(&change.collapsed)
                .filter_map(|id| bp.element(*id))
                .map(|e| element_cost(e.kind))
                .sum();
            if let Some(p) = by {
                *self.collateral.entry(p).or_default() += cost;
            }
            let ids: Vec<u32> = change.broken.iter().map(|e| e.0).collect();
            for chunk in ids.chunks(hw_net::protocol::MAX_DAMAGE_IDS) {
                self.news.push(ToClient::Damage {
                    building: i,
                    broken: chunk.to_vec(),
                    live: true,
                });
            }
            self.update_solids(solids, i);
        }
        result
    }

    /// Takes over broken elements from the server or a save.
    pub fn apply(&mut self, solids: &Obstacles, i: u64, broken: &[u32], tick: u64) {
        let Some((bp, frame)) = self.plan(solids, i) else {
            return;
        };
        let entry = self.buildings.entry(i).or_insert_with(|| Damaged {
            state: Building::new(bp.clone()),
            frame,
            last_seen: tick,
        });
        let mut bits = entry.state.broken().clone();
        for &id in broken {
            if (id as usize) < bits.len() {
                bits.set(id as usize, true);
            }
        }
        entry.state.apply(&bits);
        self.update_solids(solids, i);
    }

    /// Makes building `i` whole again.
    pub fn repair(&mut self, solids: &Obstacles, i: u64) {
        self.plans.remove(&i);
        if self.buildings.remove(&i).is_some() {
            solids.set_damage(i, None);
            self.news.push(ToClient::Repaired { building: i });
        }
    }

    /// Repairs buildings nobody has been near for [`REPAIR_SECONDS`].
    /// `watchers`: where the heroes are.
    pub fn repair_unseen(&mut self, solids: &Obstacles, watchers: &[DVec3], tick: u64, hz: u32) {
        let due = (REPAIR_SECONDS * hz as f64) as u64;
        let mut fix = Vec::new();
        for (i, d) in self.buildings.iter_mut() {
            let Some((plan, _)) = solids.building(*i) else {
                continue;
            };
            let c = plan.site.origin
                + plan.site.along * plan.site.size.x * 0.5
                + plan.site.inward * plan.site.size.y * 0.5;
            let far = REPAIR_DISTANCE + 4.0 * plan.height();
            if watchers
                .iter()
                .any(|w| DVec2::new(w.x, w.z).distance(c) < far)
            {
                d.last_seen = tick;
            } else if tick.saturating_sub(d.last_seen) >= due {
                fix.push(*i);
            }
        }
        for i in fix {
            self.repair(solids, i);
        }
        // Breakable plans are kept only for damaged buildings: a blow that
        // broke nothing, or a building repaired, leaves none behind.
        let buildings = &self.buildings;
        self.plans.retain(|i, _| buildings.contains_key(i));
    }

    /// Breakable plans kept now (for the memory tests).
    pub fn plans_kept(&self) -> usize {
        self.plans.len()
    }

    /// Collision for what is left of building `i`: its standing elements
    /// and its rubble mounds.
    fn update_solids(&self, solids: &Obstacles, i: u64) {
        let Some(d) = self.buildings.get(&i) else {
            return;
        };
        let f = d.frame;
        let gone = d.state.gone();
        let plan = d.state.plan();
        let solid = |c: DVec3, h: DVec3| -> Solid {
            let c = f.from_engine(c.as_vec3());
            Solid {
                origin: f.origin,
                along: f.along,
                inward: f.inward,
                min: DVec2::new(c.x - h.x, c.z - h.z),
                max: DVec2::new(c.x + h.x, c.z + h.z),
                y0: f.base + c.y - h.y,
                y1: f.base + c.y + h.y,
            }
        };
        let mut out = Vec::new();
        for (k, e) in plan.elements().iter().enumerate() {
            if !gone.get(k) {
                out.push(solid(e.center.as_dvec3(), e.half.as_dvec3()));
            }
        }
        // Rubble: a heap of three steps, low enough to walk up.
        for m in d.state.mounds() {
            let steps = 3;
            for s in 0..steps {
                let k = 1.0 - s as f64 / steps as f64;
                let r = m.radius as f64 * k * 0.8;
                let h = m.height as f64 / steps as f64;
                let y = m.center.y as f64 + h * s as f64;
                out.push(solid(
                    DVec3::new(m.center.x as f64, y + h * 0.5, m.center.z as f64),
                    DVec3::new(r, h * 0.5, r),
                ));
            }
        }
        // The furniture of a building of halls stays where it stands (it
        // is drawn on its own), below where a collapse starts.
        if let Some((halls, base)) = solids.building(i).filter(|(p, _)| p.layout.is_some()) {
            let fell_from = d
                .state
                .collapse()
                .map_or(f64::INFINITY, |c| c.base_y as f64);
            let site = &halls.site;
            let turn = |v: DVec2| site.along * v.x + site.inward * v.y;
            for shape in halls.furniture().iter().filter_map(|i| i.footprint()) {
                if shape.span().0 >= fell_from - 1e-3 {
                    continue;
                }
                out.extend(
                    crate::collision::solids_of(&shape)
                        .into_iter()
                        .map(|s| Solid {
                            origin: site.origin + turn(s.origin),
                            along: turn(s.along),
                            inward: turn(s.inward),
                            y0: base + s.y0,
                            y1: base + s.y1,
                            ..s
                        }),
                );
            }
        }
        solids.set_damage(i, Some(out));
    }

    /// Adds news for clients (impacts).
    pub fn tell(&mut self, msg: ToClient) {
        self.news.push(msg);
    }

    /// News for clients since the last call.
    pub fn take_news(&mut self) -> Vec<ToClient> {
        std::mem::take(&mut self.news)
    }

    /// Every damaged building as messages, for a player who joins.
    pub fn snapshot(&self) -> Vec<ToClient> {
        let mut out = Vec::new();
        for (i, d) in &self.buildings {
            let ids: Vec<u32> = d.state.broken().iter_set().map(|k| k as u32).collect();
            for chunk in ids.chunks(hw_net::protocol::MAX_DAMAGE_IDS) {
                out.push(ToClient::Damage {
                    building: *i,
                    broken: chunk.to_vec(),
                    live: false,
                });
            }
        }
        out
    }

    /// Broken elements per damaged building (for saves and tests).
    pub fn broken(&self) -> Vec<(u64, Vec<u32>)> {
        self.buildings
            .iter()
            .map(|(i, d)| (*i, d.state.broken().iter_set().map(|k| k as u32).collect()))
            .collect()
    }

    /// Forgets all damage (before a save is restored).
    pub fn clear(&mut self, solids: &Obstacles) {
        for i in std::mem::take(&mut self.buildings).into_keys() {
            solids.set_damage(i, None);
        }
        self.plans.clear();
        self.news.clear();
    }

    /// The broken bits of building `i`, if it is damaged.
    pub fn bits(&self, i: u64) -> Option<&ElementBits> {
        self.buildings.get(&i).map(|d| d.state.broken())
    }

    /// Is element `id` of building `i` still standing (tests)?
    pub fn standing(&self, i: u64, id: u32) -> bool {
        self.buildings
            .get(&i)
            .is_none_or(|d| d.state.is_standing(ElementId(id)))
    }
}

/// The first solid point along a ray from `from` in direction `dir` (unit),
/// up to `max` metres: buildings, bridges and the ground.
pub fn raycast(solids: &Obstacles, from: DVec3, dir: DVec3, max: f64) -> Option<DVec3> {
    let step = 0.2;
    let mut t = 0.0;
    while t < max {
        let p = from + dir * t;
        if solids.solid_at(p) || p.y < solids.ground_at(DVec2::new(p.x, p.z)) {
            return Some(p);
        }
        t += step;
    }
    None
}
