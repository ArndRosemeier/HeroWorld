//! Furniture for a layout: what stands in every room, as plain data.
//!
//! The rooms are game-scale, the furniture is not: a desk is a desk. So a
//! hall is furnished in blocks with wide aisles between them, a fight has
//! room to move and things to jump on and duck behind. Every piece is
//! solid ([`Item::footprint`]) except the rugs; the game draws each one
//! where its footprint is.
//!
//! Kept clear: the aprons of doorways, the feet and heads of stairs, the
//! stairs themselves, columns, lift shafts and landings low enough to
//! bump into.

use super::solid::{pieces, Part};
use super::{Flight, Layout, LinkKind, Space, SpaceKind};
use crate::geom::Rect;
use crate::urban::footprint::Footprint;
use glam::DVec2;
use hw_core::{Rng, Seed};

/// A kind of furniture. Sizes are (width, depth, height) in metres, the
/// front being the side one uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemKind {
    /// A steel office desk; its sitter's side is the front.
    Desk,
    /// A swivel chair at a desk.
    SwivelChair,
    /// A plain chair at a table.
    Chair,
    /// A filing cabinet.
    Cabinet,
    /// A tall bookcase.
    Bookcase,
    /// A two-sided run of storage shelves.
    Shelves,
    Sofa,
    Armchair,
    CoffeeTable,
    /// A long table (meetings, the canteen).
    Table,
    /// The front desk of a lobby.
    Counter,
    /// A glass display case.
    Case,
    /// A big plant in a pot.
    Plant,
    WaterCooler,
    /// A round rug: lies flat, stops nobody.
    Rug,
    /// A double bed, its head to the wall.
    Bed,
    Wardrobe,
    Nightstand,
    /// A run of kitchen counter with its sink.
    KitchenRun,
    Fridge,
    Stove,
    Sideboard,
    /// A television set in its cabinet.
    Television,
}

impl ItemKind {
    /// Depth and height; the width is the item's own.
    pub fn depth_height(self) -> (f64, f64) {
        match self {
            Self::Desk => (0.8, 0.76),
            Self::SwivelChair => (0.6, 0.95),
            Self::Chair => (0.5, 0.9),
            Self::Cabinet => (0.64, 1.32),
            Self::Bookcase => (0.4, 2.0),
            Self::Shelves => (0.95, 1.6),
            Self::Sofa => (0.9, 0.8),
            Self::Armchair => (0.9, 0.8),
            Self::CoffeeTable => (0.75, 0.42),
            Self::Table => (1.0, 0.75),
            Self::Counter => (0.7, 1.05),
            Self::Case => (0.6, 1.0),
            Self::Plant => (0.5, 1.4),
            Self::WaterCooler => (0.4, 1.2),
            Self::Rug => (0.0, 0.0),
            Self::Bed => (2.1, 0.6),
            Self::Wardrobe => (0.62, 2.0),
            Self::Nightstand => (0.42, 0.55),
            Self::KitchenRun => (0.62, 0.92),
            Self::Fridge => (0.7, 1.6),
            Self::Stove => (0.66, 0.92),
            Self::Sideboard => (0.5, 0.85),
            Self::Television => (0.5, 1.0),
        }
    }

    pub fn solid(self) -> bool {
        self != Self::Rug
    }
}

/// One piece of furniture, in the layout's frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Item {
    pub kind: ItemKind,
    /// Its middle on the floor (x, z).
    pub at: DVec2,
    /// The floor it stands on.
    pub y: f64,
    /// The way its front faces: one of the four axes.
    pub face: DVec2,
    /// Its width (a rug's diameter).
    pub w: f64,
    /// For its looks: colours, what lies on it.
    pub seed: u64,
    /// The space it stands in.
    pub space: usize,
}

impl Item {
    /// Half its size in plan, along x and z.
    pub fn half(&self) -> DVec2 {
        let (d, _) = self.kind.depth_height();
        let d = if self.kind == ItemKind::Rug {
            self.w
        } else {
            d
        };
        if self.face.x.abs() < 0.5 {
            DVec2::new(self.w, d) * 0.5
        } else {
            DVec2::new(d, self.w) * 0.5
        }
    }

    /// What it blocks, if anything.
    pub fn footprint(&self) -> Option<Footprint> {
        let (_, h) = self.kind.depth_height();
        self.kind.solid().then(|| Footprint::Box {
            at: self.at,
            dir: DVec2::Y,
            half: self.half(),
            y0: self.y,
            y1: self.y + h,
        })
    }
}

/// Walking room kept round stairs, columns and shafts (m).
const CLEAR: f64 = 0.9;
/// Room kept at the foot and the head of a flight (m).
const STAIR_APRON: f64 = 3.2;
/// How far a piece stands off a wall's middle line (m): half the wall and
/// its skirting.
const OFF_WALL: f64 = 0.24;
/// The walk left free along the walls of a hall (m).
const WALK: f64 = 2.2;

/// Places pieces in one space.
struct Room<'a> {
    i: usize,
    s: &'a Space,
    /// The floor inside the walls.
    inner: Rect,
    /// Nothing stands here.
    blocked: Vec<Rect>,
    /// Which edges are real walls to stand things against (min z, max z,
    /// min x, max x).
    walls: [bool; 4],
    out: Vec<Item>,
    rng: Rng,
}

impl Room<'_> {
    fn fits(&self, c: DVec2, half: DVec2, gap: f64) -> bool {
        let r = Rect::new(c - half, c + half);
        self.inner.contains_rect(&r, 1e-6)
            && self.blocked.iter().all(|b| b.overlap(&r) < 1e-9)
            && self.out.iter().filter(|o| o.kind.solid()).all(|o| {
                let d = (c - o.at).abs() - (half + o.half());
                d.x > gap || d.y > gap
            })
    }

    /// Puts a piece down if there is room, `gap` clear of the others.
    fn put(&mut self, kind: ItemKind, at: DVec2, face: DVec2, w: f64, gap: f64) -> bool {
        let item = Item {
            kind,
            at,
            y: self.s.y,
            face,
            w,
            seed: self.rng.next_u64(),
            space: self.i,
        };
        if kind.solid() && !self.fits(at, item.half(), gap) {
            return false;
        }
        self.out.push(item);
        true
    }

    /// A rug only where nothing is in its way (it lies under furniture,
    /// not under stairs).
    fn rug(&mut self, at: DVec2, d: f64) -> bool {
        let half = DVec2::splat(d * 0.5);
        let r = Rect::new(at - half, at + half);
        if !self.inner.contains_rect(&r, 1e-6) || self.blocked.iter().any(|b| b.overlap(&r) > 1e-9)
        {
            return false;
        }
        self.put(ItemKind::Rug, at, DVec2::Y, d, 0.0)
    }

    /// The four walls as (a point on it, its inward normal, its length).
    fn wall(&self, k: usize) -> (DVec2, DVec2, DVec2, f64) {
        let r = &self.inner;
        let s = r.size();
        match k {
            0 => (r.min, DVec2::X, DVec2::Y, s.x),
            1 => (DVec2::new(r.min.x, r.max.y), DVec2::X, -DVec2::Y, s.x),
            2 => (r.min, DVec2::Y, DVec2::X, s.y),
            _ => (DVec2::new(r.max.x, r.min.y), DVec2::Y, -DVec2::X, s.y),
        }
    }

    /// Stands a piece with its back to wall `k`, `t` metres along it.
    fn against(&mut self, k: usize, t: f64, kind: ItemKind, w: f64, gap: f64) -> bool {
        if !self.walls[k] {
            return false;
        }
        let (o, along, n, len) = self.wall(k);
        if t - w * 0.5 < 0.0 || t + w * 0.5 > len {
            return false;
        }
        let (d, _) = kind.depth_height();
        self.put(kind, o + along * t + n * (d * 0.5), n, w, gap)
    }

    /// A seating group round `c`: two sofas (or a sofa and armchairs)
    /// across a low table, on a rug.
    fn seats(&mut self, c: DVec2, along_x: bool) -> bool {
        let (u, v) = if along_x {
            (DVec2::X, DVec2::Y)
        } else {
            (DVec2::Y, DVec2::X)
        };
        let reach = DVec2::splat(2.6);
        if !self.fits(c, reach, 0.3) {
            return false;
        }
        self.rug(c, 4.6);
        self.put(ItemKind::CoffeeTable, c, v, 1.3, 0.1);
        self.put(ItemKind::Sofa, c - v * 1.45, v, 2.0, 0.1);
        if self.rng.chance(0.5) {
            self.put(ItemKind::Sofa, c + v * 1.45, -v, 2.0, 0.1);
        } else {
            for s in [-0.6, 0.6] {
                self.put(ItemKind::Armchair, c + v * 1.45 + u * s, -v, 0.9, 0.1);
            }
        }
        for s in [-1.0, 1.0] {
            self.put(ItemKind::Plant, c + u * (s * 2.2) - v * 1.5, v, 0.5, 0.1);
        }
        true
    }

    /// A seating group at `c`, or as near it as there is room.
    fn seats_near(&mut self, c: DVec2, along_x: bool) -> bool {
        for step in 0..5 {
            let d = step as f64 * 0.6;
            for off in [
                DVec2::ZERO,
                DVec2::X * d,
                DVec2::X * -d,
                DVec2::Y * d,
                DVec2::Y * -d,
                DVec2::new(d, d),
                DVec2::new(-d, d),
                DVec2::new(d, -d),
                DVec2::new(-d, -d),
            ] {
                if (step > 0 || off == DVec2::ZERO) && self.seats(c + off, along_x) {
                    return true;
                }
            }
        }
        false
    }

    /// Plants and the odd cabinet along the real walls, every `step`.
    fn along_walls(&mut self, step: f64, kinds: &[(ItemKind, f64)]) {
        for k in 0..4 {
            let len = self.wall(k).3;
            let n = (len / step).floor() as usize;
            for j in 0..n {
                let t = (j as f64 + 0.5) * len / n as f64;
                let (kind, w) = *self.rng.pick(kinds);
                self.against(k, t, kind, w, 0.5);
            }
        }
    }

    /// Desks in blocks of two rows back to back, wide aisles between.
    fn offices(&mut self) {
        let area = self.inner.inset(WALK);
        let s = area.size();
        if s.min_element() < 3.0 {
            return;
        }
        // Rows run along the longer side.
        let long_x = s.x >= s.y;
        let (u, v) = if long_x {
            (DVec2::X, DVec2::Y)
        } else {
            (DVec2::Y, DVec2::X)
        };
        let (len, deep) = if long_x { (s.x, s.y) } else { (s.y, s.x) };
        // A block: two rows of desks facing each other's backs, 3.4 m deep
        // with its chairs; then an aisle.
        let (block, aisle) = (3.6, 3.4);
        let blocks = ((deep + aisle) / (block + aisle)).floor() as usize;
        // Along a row: runs of four desks, then a gap to pass through.
        let (pitch, run, gap) = (2.0, 4, 2.6);
        let run_len = run as f64 * pitch;
        let runs = ((len + gap) / (run_len + gap)).floor().max(1.0) as usize;
        let used = runs as f64 * run_len + (runs - 1) as f64 * gap;
        let lead = ((len - used) * 0.5).max(0.0);
        let spare =
            (deep - (blocks as f64 * block + blocks.saturating_sub(1) as f64 * aisle)) * 0.5;
        for b in 0..blocks {
            let mid = spare + b as f64 * (block + aisle) + block * 0.5;
            for r in 0..runs {
                for i in 0..run {
                    let t = lead + r as f64 * (run_len + gap) + (i as f64 + 0.5) * pitch;
                    if t + pitch * 0.5 > len + 1e-6 {
                        continue;
                    }
                    for side in [-1.0, 1.0] {
                        // The desk's sitter faces the block's middle line;
                        // the chair is on the outside.
                        let c = area.min + u * t + v * (mid + side * 0.45);
                        if self.put(ItemKind::Desk, c, v * side, 1.55, 0.0) {
                            self.put(
                                ItemKind::SwivelChair,
                                c + v * (side * 0.75),
                                -v * side,
                                0.6,
                                0.0,
                            );
                        }
                    }
                }
            }
        }
        self.along_walls(
            5.5,
            &[
                (ItemKind::Cabinet, 0.48),
                (ItemKind::Cabinet, 0.48),
                (ItemKind::Plant, 0.5),
                (ItemKind::WaterCooler, 0.4),
            ],
        );
    }

    /// Rows of bookcases and shelves with aisles: the archive.
    fn archive(&mut self) {
        let area = self.inner.inset(WALK);
        let s = area.size();
        if s.min_element() < 3.0 {
            return;
        }
        let long_x = s.x >= s.y;
        let (u, v) = if long_x {
            (DVec2::X, DVec2::Y)
        } else {
            (DVec2::Y, DVec2::X)
        };
        let (len, deep) = if long_x { (s.x, s.y) } else { (s.y, s.x) };
        let rows = (deep / 3.6).floor() as usize;
        let (w, gap) = (2.4, 2.8);
        let n = ((len + gap) / (w * 2.0 + gap)).floor().max(1.0) as usize;
        for r in 0..rows {
            let z = (r as f64 + 0.5) * deep / rows as f64;
            for k in 0..n {
                let t0 = (len - (n as f64 * w * 2.0 + (n - 1) as f64 * gap)) * 0.5
                    + k as f64 * (w * 2.0 + gap);
                for i in 0..2 {
                    let c = area.min + u * (t0 + (i as f64 + 0.5) * w) + v * z;
                    self.put(ItemKind::Shelves, c, v, w, 0.0);
                }
            }
        }
        self.along_walls(4.0, &[(ItemKind::Cabinet, 0.48), (ItemKind::Bookcase, 1.8)]);
    }

    /// Long tables with chairs: the canteen, the meeting hall.
    fn tables(&mut self) {
        let area = self.inner.inset(WALK);
        let s = area.size();
        if s.min_element() < 3.0 {
            return;
        }
        let (w, pitch_x, pitch_z) = (2.4, 5.6, 4.6);
        let nx = (s.x / pitch_x).floor() as usize;
        let nz = (s.y / pitch_z).floor() as usize;
        for i in 0..nx {
            for j in 0..nz {
                let c = area.min
                    + DVec2::new(
                        (i as f64 + 0.5) * s.x / nx as f64,
                        (j as f64 + 0.5) * s.y / nz as f64,
                    );
                if !self.put(ItemKind::Table, c, DVec2::Y, w, 1.0) {
                    continue;
                }
                for k in 0..3 {
                    let x = (k as f64 - 1.0) * 0.8;
                    for side in [-1.0, 1.0] {
                        self.put(
                            ItemKind::Chair,
                            c + DVec2::new(x, side * 0.8),
                            DVec2::Y * -side,
                            0.5,
                            0.0,
                        );
                    }
                }
            }
        }
        self.along_walls(6.0, &[(ItemKind::Plant, 0.5), (ItemKind::WaterCooler, 0.4)]);
    }

    /// The lobby: the front desk facing the way in, seats to the sides.
    fn lobby(&mut self, entrance: Option<DVec2>) {
        let r = self.inner;
        let s = r.size();
        if let Some(e) = entrance {
            // The way in is on the min-z side: the desk looks at it from
            // well inside, in line with the doors or beside them.
            let w = (s.x * 0.12).clamp(3.0, 6.0);
            let z = r.min.y + (s.y * 0.45).clamp(7.0, 12.0);
            for dx in [0.0, -6.0, 6.0, -11.0, 11.0] {
                if self.put(
                    ItemKind::Counter,
                    DVec2::new(e.x + dx, z),
                    -DVec2::Y,
                    w,
                    1.5,
                ) {
                    break;
                }
            }
        }
        // Seating groups on a loose grid.
        let (nx, nz) = (
            (s.x / 11.0).floor().max(1.0) as usize,
            (s.y / 11.0).floor().max(1.0) as usize,
        );
        let mut groups = 0;
        for j in 0..nz {
            for i in 0..nx {
                if groups >= 4 {
                    break;
                }
                let c = r.min
                    + DVec2::new(
                        (i as f64 + 0.5) * s.x / nx as f64,
                        (j as f64 + 0.5) * s.y / nz as f64,
                    );
                // Not in the straight way from the doors.
                if entrance.is_some_and(|e| (c.x - e.x).abs() < 5.0) {
                    continue;
                }
                let along_x = self.rng.chance(0.5);
                if self.seats(c, along_x) {
                    groups += 1;
                }
            }
        }
        self.along_walls(
            7.0,
            &[
                (ItemKind::Plant, 0.5),
                (ItemKind::Case, 1.6),
                (ItemKind::Sofa, 2.0),
            ],
        );
    }

    /// A shop: the counter at the back facing the door, shelves in rows
    /// across the floor, display cases along the walls.
    fn shop(&mut self) {
        let r = self.inner;
        let s = r.size();
        let w = (s.x * 0.3).clamp(2.4, 4.0);
        let mid = r.center().x;
        self.put(
            ItemKind::Counter,
            DVec2::new(mid, r.max.y - 2.2),
            -DVec2::Y,
            w,
            1.0,
        );
        // Rows of shelves either side of the way in, from 4 m inside.
        let rows = ((s.y - 9.0) / 3.2).floor().max(0.0) as usize;
        for j in 0..rows {
            let z = r.min.y + 4.5 + j as f64 * 3.2;
            for side in [-1.0, 1.0] {
                let len = ((s.x * 0.5 - 3.4) * 0.5).floor() * 2.0;
                if len >= 2.0 {
                    let x = mid + side * (1.7 + len * 0.5);
                    self.put(ItemKind::Shelves, DVec2::new(x, z), DVec2::Y, len, 0.9);
                }
            }
        }
        self.along_walls(4.5, &[(ItemKind::Case, 1.6), (ItemKind::Plant, 0.5)]);
    }

    /// A living room: sofas round a low table, the television and a
    /// sideboard at the walls, books, plants.
    fn living(&mut self) {
        let r = self.inner;
        let s = r.size();
        let along_x = s.x >= s.y;
        let c = r.center();
        // Two groups in a long room, else one.
        let step = if along_x { DVec2::X } else { DVec2::Y } * (s.max_element() * 0.25);
        let spots = if s.max_element() >= 12.0 {
            vec![c - step, c + step]
        } else {
            vec![c]
        };
        let mut seated = false;
        for (k, p) in spots.into_iter().enumerate() {
            if k == 1 && seated && self.rng.chance(0.5) {
                // A dining table instead of the second group.
                if self.put(ItemKind::Table, p, DVec2::Y, 2.4, 0.8) {
                    for x in [-0.8, 0.0, 0.8] {
                        for side in [-1.0, 1.0] {
                            let at = p + DVec2::new(x, side * 0.8);
                            self.put(ItemKind::Chair, at, DVec2::Y * -side, 0.5, 0.0);
                        }
                    }
                }
                continue;
            }
            seated |= self.seats_near(p, !along_x);
        }
        // One television, in the middle of a wall.
        let first = self.rng.index(4);
        for k in (0..4).map(|k| (k + first) % 4) {
            let len = self.wall(k).3;
            if self.against(k, len * 0.5, ItemKind::Television, 1.0, 0.4) {
                break;
            }
        }
        self.along_walls(
            3.6,
            &[
                (ItemKind::Sideboard, 1.8),
                (ItemKind::Bookcase, 1.6),
                (ItemKind::Plant, 0.5),
                (ItemKind::Armchair, 0.9),
            ],
        );
    }

    /// Fridge, counter, stove and more counter in a row at wall `k`, past
    /// its doorways: how many of the four found room.
    fn kitchen_run(&mut self, k: usize) -> usize {
        let len = self.wall(k).3;
        let mut t = 0.1;
        let mut n = 0;
        for (kind, w) in [
            (ItemKind::Fridge, 0.7),
            (ItemKind::KitchenRun, 2.4),
            (ItemKind::Stove, 0.7),
            (ItemKind::KitchenRun, 1.8),
        ] {
            while t + w <= len - 0.1 {
                if self.against(k, t + w * 0.5, kind, w, 0.0) {
                    n += 1;
                    t += w + 0.02;
                    break;
                }
                t += 0.3;
            }
        }
        n
    }

    /// The hall of a house of flats: somewhere to sit and wait, things
    /// along the walls, the way from the door to the stairs clear.
    fn house_hall(&mut self, entrance: Option<DVec2>) {
        let r = self.inner;
        let s = r.size();
        let ex = entrance.map_or(r.center().x, |e| e.x);
        for side in [-1.0, 1.0] {
            let c = DVec2::new(ex + side * (s.x * 0.25).max(4.4), r.min.y + s.y * 0.5);
            self.seats_near(c, false);
        }
        self.along_walls(
            4.0,
            &[
                (ItemKind::Sideboard, 1.8),
                (ItemKind::Plant, 0.5),
                (ItemKind::Sofa, 2.0),
                (ItemKind::Case, 1.6),
            ],
        );
    }

    /// A kitchen to eat in: a run of counter, stove and fridge at one
    /// wall, the table in the middle.
    fn kitchen(&mut self) {
        // The run goes to the wall that takes all of it: the longest
        // first, past its doorways.
        let mut order: Vec<usize> = (0..4).filter(|k| self.walls[*k]).collect();
        order.sort_by(|a, b| self.wall(*b).3.total_cmp(&self.wall(*a).3));
        let mut best = (0, None);
        for k in order.iter().copied() {
            let from = self.out.len();
            let n = self.kitchen_run(k);
            self.out.truncate(from);
            if n > best.0 {
                best = (n, Some(k));
            }
            if n == 4 {
                break;
            }
        }
        if let Some(k) = best.1 {
            self.kitchen_run(k);
        }
        // The table as near the middle as it finds room, either way round.
        let c = self.inner.center();
        let s = self.inner.size();
        let (u, v) = if s.x >= s.y {
            (DVec2::X, DVec2::Y)
        } else {
            (DVec2::Y, DVec2::X)
        };
        let reach = (s.max_element() * 0.5).floor() as i32;
        // Chairs along both sides; in a narrow kitchen at the two ends.
        'table: for sides in [true, false] {
            for step in 0..=reach {
                for side in [1.0, -1.0] {
                    for (a, b) in [(u, v), (v, u)] {
                        let p = c + u * (step as f64 * side);
                        // (Chairs need their room too.)
                        let need = if sides {
                            a.abs() * 1.25 + b.abs() * 1.05
                        } else {
                            a.abs() * 1.75 + b.abs() * 0.55
                        };
                        if !self.fits(p, need, 0.1) || !self.put(ItemKind::Table, p, b, 2.4, 0.1) {
                            continue;
                        }
                        if sides {
                            for x in [-0.8, 0.0, 0.8] {
                                for side in [-1.0, 1.0] {
                                    let at = p + a * x + b * (side * 0.8);
                                    self.put(ItemKind::Chair, at, b * -side, 0.5, 0.0);
                                }
                            }
                        } else {
                            for end in [-1.0, 1.0] {
                                self.put(ItemKind::Chair, p + a * (end * 1.5), a * -end, 0.5, 0.0);
                            }
                        }
                        break 'table;
                    }
                }
            }
        }
        self.along_walls(5.0, &[(ItemKind::Sideboard, 1.8), (ItemKind::Plant, 0.5)]);
    }

    /// A bedroom: the bed's head to a wall with a nightstand each side,
    /// wardrobes at another, an armchair.
    fn bedroom(&mut self) {
        let mut bed = None;
        // The wall furthest from the doors first: try each in turn.
        for k in [1, 3, 2, 0] {
            let len = self.wall(k).3;
            for t in [len * 0.5, len * 0.35, len * 0.65] {
                if self.against(k, t, ItemKind::Bed, 1.7, 0.6) {
                    for side in [-1.15, 1.15] {
                        self.against(k, t + side, ItemKind::Nightstand, 0.5, 0.0);
                    }
                    bed = Some(k);
                    break;
                }
            }
            if bed.is_some() {
                break;
            }
        }
        self.along_walls(
            2.6,
            &[
                (ItemKind::Wardrobe, 1.8),
                (ItemKind::Wardrobe, 1.2),
                (ItemKind::Armchair, 0.9),
                (ItemKind::Plant, 0.5),
            ],
        );
    }

    /// A walkway: benches and plants along its real walls.
    fn walkway(&mut self) {
        self.along_walls(
            8.0,
            &[
                (ItemKind::Sofa, 2.0),
                (ItemKind::Plant, 0.5),
                (ItemKind::Case, 1.6),
                (ItemKind::Plant, 0.5),
            ],
        );
    }
}

/// Every piece of furniture in the building.
pub fn items(l: &Layout) -> Vec<Item> {
    let solid = pieces(l);
    let seed = Seed(l.size.x.to_bits())
        .child(l.size.y.to_bits())
        .child(l.spaces.len() as u64)
        .child("furnish");
    let mut out = Vec::new();
    for (i, s) in l.spaces.iter().enumerate() {
        if s.within.is_some()
            || matches!(
                s.kind,
                SpaceKind::Shaft | SpaceKind::StairHall | SpaceKind::Canyon
            )
            || (s.kind == SpaceKind::Vestibule && !l.scheme.homes())
        {
            continue;
        }
        let inner = s.rect.inset(OFF_WALL);
        // What stands or hangs in the room up to head height: stairs,
        // landings, columns, shafts and their rails.
        let mut blocked: Vec<Rect> = Vec::new();
        for p in &solid {
            if !matches!(
                p.part,
                Part::Step | Part::Deck | Part::Column | Part::Shaft | Part::Glass | Part::Rail
            ) {
                continue;
            }
            let (y0, y1) = p.shape.span();
            if y1 < s.y + 0.1 || y0 > s.y + 2.3 {
                continue;
            }
            let (x0, x1) = p.shape.extent(DVec2::ZERO, DVec2::X);
            let (z0, z1) = p.shape.extent(DVec2::ZERO, DVec2::Y);
            let r = Rect::from_xz(x0, z0, x1, z1);
            // The room's own walls of glass and rails are its edge, not
            // something in it.
            if !s.rect.inset(0.3).contains_point(r.center()) && r.overlap(&s.rect.inset(0.3)) < 0.05
            {
                continue;
            }
            blocked.push(r.inset(-CLEAR));
        }
        // Doorways, and whatever one can fall or step through.
        let mut walls = [true; 4];
        let mut entrance = None;
        for k in l
            .links
            .iter()
            .filter(|k| (k.a == i || k.b == Some(i)) && (k.y - s.y).abs() < 0.1)
        {
            let edge = if k.along_x {
                if (k.at.y - s.rect.min.y).abs() < 0.01 {
                    0
                } else {
                    1
                }
            } else if (k.at.x - s.rect.min.x).abs() < 0.01 {
                2
            } else {
                3
            };
            match k.kind {
                // Nothing stands at a rail or a glass wall, nor where the
                // room simply runs on.
                LinkKind::Rail | LinkKind::Glass | LinkKind::Open => walls[edge] = false,
                _ => {}
            }
            if k.kind.passable() {
                // (Homes are closer set than halls.)
                let clear = if l.scheme.homes() { 0.4 } else { 1.2 };
                blocked.push(k.apron().inset(-clear));
            }
            if k.kind == LinkKind::Entrance {
                entrance = Some(k.at);
            }
        }
        // Links of the storeys above cut the same wall (a tall room's
        // galleries): no bookcase under a rail up there either.
        for st in l
            .stairs
            .iter()
            .filter(|st| l.spaces[st.host].within.unwrap_or(st.host) == i || st.host == i)
        {
            for f in &st.flights {
                blocked.push(f.footprint().inset(-CLEAR));
                // Its two ends, wide.
                let ends: [(DVec2, f64); 2] = match *f {
                    Flight::Straight {
                        from, dir, risers, ..
                    } => {
                        let a = DVec2::new(from.x, from.z);
                        [(a, f.y0()), (a + dir * Flight::run(risers), f.y1())]
                    }
                    Flight::Arc {
                        centre,
                        r_in,
                        r_out,
                        a0,
                        sweep,
                        ..
                    } => {
                        let r = (r_in + r_out) * 0.5;
                        [
                            (centre + hw_core::det::dir2(a0) * r, f.y0()),
                            (centre + hw_core::det::dir2(a0 + sweep) * r, f.y1()),
                        ]
                    }
                };
                for (p, y) in ends {
                    if (y - s.y).abs() < 0.5 {
                        blocked.push(Rect::new(
                            p - DVec2::splat(STAIR_APRON),
                            p + DVec2::splat(STAIR_APRON),
                        ));
                    }
                }
            }
        }
        // Lift doors opening into this room.
        for k in l.links.iter().filter(|k| {
            (k.y - s.y).abs() < 0.1
                && [Some(k.a), k.b]
                    .into_iter()
                    .flatten()
                    .any(|j| l.spaces[j].kind == SpaceKind::Shaft && l.spaces[j].within == Some(i))
        }) {
            blocked.push(k.apron().inset(-1.6));
        }
        let mut room = Room {
            i,
            s,
            inner,
            blocked,
            walls,
            out: Vec::new(),
            rng: seed.child(i as u64).rng(),
        };
        match s.kind {
            SpaceKind::Lobby => room.lobby(entrance),
            SpaceKind::Atrium => room.lobby(None),
            SpaceKind::Hall => match room.rng.index(4) {
                0 => room.archive(),
                1 => room.tables(),
                _ => room.offices(),
            },
            SpaceKind::Boardroom => room.tables(),
            SpaceKind::Shop => room.shop(),
            SpaceKind::Living => room.living(),
            SpaceKind::Kitchen => room.kitchen(),
            SpaceKind::Bedroom => room.bedroom(),
            SpaceKind::Vestibule => room.house_hall(entrance),
            SpaceKind::Service => room.archive(),
            SpaceKind::Gallery | SpaceKind::Arcade | SpaceKind::Concourse => room.walkway(),
            _ => {}
        }
        out.append(&mut room.out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::office::plan_scheme;
    use crate::layout::Scheme;

    fn plans() -> Vec<Layout> {
        let mut out = Vec::new();
        for (k, scheme) in [Scheme::Atrium, Scheme::GrandHall, Scheme::SplitLevel]
            .into_iter()
            .enumerate()
        {
            for seed in 0..6u64 {
                if let Some(l) =
                    plan_scheme(Seed(seed * 7 + k as u64), DVec2::new(56.0, 52.0), 5, scheme)
                {
                    out.push(l);
                }
            }
        }
        assert!(out.len() >= 9);
        // Old-town houses: shop houses and flats.
        for seed in 0..8u64 {
            let size = DVec2::new(12.0 + (seed % 4) as f64, 24.0 + (seed % 5) as f64);
            for scheme in [Scheme::ShopHouse, Scheme::Flats] {
                out.push(plan_scheme(Seed(40 + seed), size, 3, scheme).expect("it fits"));
            }
            let size = DVec2::new(
                30.0 + (seed % 4) as f64 * 3.0,
                28.0 + (seed % 5) as f64 * 1.5,
            );
            out.push(plan_scheme(Seed(60 + seed), size, 5, Scheme::Block).expect("it fits"));
            let size = DVec2::new(23.0 + seed as f64, 16.0 + (seed % 4) as f64);
            for scheme in [Scheme::Terrace, Scheme::Villa] {
                out.push(plan_scheme(Seed(80 + seed), size, 3, scheme).expect("it fits"));
            }
        }
        out
    }

    /// Furniture stands in its room, on its floor, clear of every wall,
    /// stair, column and of the other pieces, and never in a doorway.
    #[test]
    fn furniture_stands_clear_of_everything() {
        for l in plans() {
            let its = items(&l);
            let least = match l.scheme {
                Scheme::ShopHouse | Scheme::Flats | Scheme::Terrace | Scheme::Villa => 20,
                Scheme::Block => 100,
                _ => 150,
            };
            assert!(
                its.len() > least,
                "{:?}: only {} pieces",
                l.scheme,
                its.len()
            );
            assert_eq!(its, items(&l), "the same every time");
            let solids: Vec<(Part, Rect, (f64, f64))> = pieces(&l)
                .iter()
                .filter(|p| !matches!(p.part, Part::Floor(_) | Part::Roof))
                .map(|p| {
                    let (x0, x1) = p.shape.extent(DVec2::ZERO, DVec2::X);
                    let (z0, z1) = p.shape.extent(DVec2::ZERO, DVec2::Y);
                    (p.part, Rect::from_xz(x0, z0, x1, z1), p.shape.span())
                })
                .collect();
            for (n, it) in its.iter().enumerate() {
                let Some(f) = it.footprint() else { continue };
                let s = &l.spaces[it.space];
                let r = Rect::new(it.at - it.half(), it.at + it.half());
                assert!(s.rect.contains_rect(&r, 1e-6), "{it:?} outside its room");
                let (y0, y1) = f.span();
                assert!((y0 - s.y).abs() < 1e-9 && y1 < s.top());
                for (part, o, (a, b)) in &solids {
                    // A deck's top at the floor's own height is a floor.
                    let touches = o.overlap(&r) > 1e-6 && *a < y1 - 1e-6 && *b > y0 + 0.05;
                    assert!(
                        !touches,
                        "{:?}: {it:?} stands in a {part:?} at {o:?}",
                        l.scheme
                    );
                }
                for k in l.links.iter().filter(|k| {
                    k.kind.passable()
                        && (k.a == it.space || k.b == Some(it.space))
                        && (k.y - s.y).abs() < 0.1
                }) {
                    assert!(k.apron().overlap(&r) < 1e-9, "{it:?} in a doorway");
                }
                for o in its[n + 1..]
                    .iter()
                    .filter(|o| o.kind.solid() && o.space == it.space)
                {
                    let q = Rect::new(o.at - o.half(), o.at + o.half());
                    assert!(q.overlap(&r) < 1e-9, "{it:?} and {o:?} overlap");
                }
            }
        }
    }
}
