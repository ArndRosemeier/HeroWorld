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

use super::solid::{pieces, shared, Part, Piece};
use super::{Flight, Layout, LinkKind, Scheme, Space, SpaceKind};
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
    /// A night club's bar: the counter, and the mirrored back bar at the
    /// wall behind it.
    Bar,
    BarStool,
    /// Two padded benches across a table, end on to the wall.
    Booth,
    /// The band's stage, with its instruments.
    Stage,
    Jukebox,
    /// A laboratory bench with its glowing flasks.
    LabBench,
    /// An Aether coil in its glass: the Doctor's apparatus.
    AetherCoil,
    /// A stamping press: a works' biggest machine.
    Press,
    /// A lathe on its long bed.
    Lathe,
    /// An Aether dynamo: a drum in a housing, humming.
    Dynamo,
    /// A workbench with a vice and tools.
    Workbench,
    /// A stack of crates.
    Crates,
    /// A model rocket on a plinth: an exhibit.
    RocketModel,
    /// A great model of the atom on a plinth.
    AtomModel,
    /// A globe of the world with its moons on a plinth.
    Orrery,
    /// A gang hideout's cot with a rumpled blanket.
    Cot,
    /// A round card table, felt and chips.
    PokerTable,
    /// A heavy safe with its dial.
    Safe,
    /// A pin board on legs: a city map, photographs and red string.
    PinBoard,
    /// A cluster of oil drums.
    Drums,
    /// A low wall of sandbags: cover to crouch behind.
    Sandbags,
}

/// How a building of halls is furnished.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    /// Offices, shops and homes, by the kind of each room.
    Plain,
    /// The Doctor's laboratories.
    Lab,
    /// A works: machines on the floor of every hall.
    Works,
    /// A fair pavilion: exhibits.
    Expo,
    /// A gang's hideout: cots, card tables, safes, planning boards.
    Hideout,
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
            Self::Bar => (BAR_DEPTH, 1.1),
            Self::BarStool => (0.4, 0.78),
            Self::Booth => (1.8, 1.1),
            Self::Stage => (STAGE_DEPTH, STAGE_HEIGHT),
            Self::Jukebox => (0.6, 1.5),
            Self::LabBench => (0.7, 0.92),
            Self::AetherCoil => (1.2, 2.2),
            Self::Press => (1.7, 2.9),
            Self::Lathe => (1.1, 1.5),
            Self::Dynamo => (2.0, 2.3),
            Self::Workbench => (0.8, 0.9),
            Self::Crates => (1.2, 1.5),
            Self::RocketModel => (1.6, 3.0),
            Self::AtomModel => (1.6, 2.5),
            Self::Orrery => (1.6, 2.3),
            Self::Cot => (2.0, 0.5),
            Self::PokerTable => (1.95, 0.78),
            Self::Safe => (0.8, 1.5),
            Self::PinBoard => (0.5, 1.9),
            Self::Drums => (1.1, 1.0),
            Self::Sandbags => (SANDBAGS_DEPTH, 0.85),
        }
    }

    pub fn solid(self) -> bool {
        self != Self::Rug
    }
}

/// What a pavilion shows, with each piece's width (m).
const EXHIBITS: &[(ItemKind, f64)] = &[
    (ItemKind::RocketModel, 1.6),
    (ItemKind::AtomModel, 1.6),
    (ItemKind::Orrery, 1.6),
];

/// A bar from the wall to the front of its counter (m): the back bar,
/// the barman's walk, the counter.
pub const BAR_DEPTH: f64 = 1.9;
/// A sandbag wall from back to front (m).
pub const SANDBAGS_DEPTH: f64 = 0.6;
/// A band's stage: how deep, how high (m).
pub const STAGE_DEPTH: f64 = 3.2;
pub const STAGE_HEIGHT: f64 = 0.45;

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
/// The walk left free along a balcony or a bridge, between what stands
/// at its wall and what stands at its rail (m).
const DECK_WALK: f64 = 1.4;
/// Room kept round where a flight leaves or reaches a balcony (m).
const DECK_APRON: f64 = 2.2;

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
        self.walls[k] && self.at_edge(k, t, kind, w, gap)
    }

    /// Stands a piece with its back to edge `k` (a wall or a rail), `t`
    /// metres along it.
    fn at_edge(&mut self, k: usize, t: f64, kind: ItemKind, w: f64, gap: f64) -> bool {
        self.off_edge(k, t, 0.0, kind, w, gap)
    }

    /// The same `off` metres in from the edge.
    fn off_edge(&mut self, k: usize, t: f64, off: f64, kind: ItemKind, w: f64, gap: f64) -> bool {
        let (o, along, n, len) = self.wall(k);
        if t - w * 0.5 < 0.0 || t + w * 0.5 > len {
            return false;
        }
        let (d, _) = kind.depth_height();
        self.put(kind, o + along * t + n * (off + d * 0.5), n, w, gap)
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

    /// A laboratory: benches in rows back to back, Aether coils in the
    /// aisles between them, cabinets and more benches along the walls.
    fn laboratory(&mut self) {
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
        // Rows across the room, 5 m apart: a row of bench islands, then a
        // row of coils, in turn.
        let rows = (deep / 5.0).floor().max(1.0) as usize;
        let (w, gap) = (3.0, 2.6);
        let n = ((len + gap) / (w + gap)).floor().max(1.0) as usize;
        let lead = (len - (n as f64 * w + (n - 1) as f64 * gap)) * 0.5;
        for r in 0..rows {
            let z = (r as f64 + 0.5) * deep / rows as f64;
            for k in 0..n {
                let c = area.min + u * (lead + k as f64 * (w + gap) + w * 0.5) + v * z;
                if r % 2 == 1 {
                    self.put(ItemKind::AetherCoil, c, v, 1.2, 0.9);
                } else {
                    // Two benches back to back.
                    self.put(ItemKind::LabBench, c - v * 0.36, -v, w, 0.0);
                    self.put(ItemKind::LabBench, c + v * 0.36, v, w, 0.0);
                }
            }
        }
        self.along_walls(
            4.5,
            &[
                (ItemKind::LabBench, 2.4),
                (ItemKind::Cabinet, 0.48),
                (ItemKind::Bookcase, 1.8),
            ],
        );
    }

    /// Big pieces `(kind, width)` on the free floor, one to every `step`
    /// metres each way and at most `most`; `sparse` leaves every other
    /// place free, like a chessboard's.
    fn grid(&mut self, kinds: &[(ItemKind, f64)], step: f64, sparse: bool, most: usize) {
        let r = self.inner.inset(WALK);
        let s = r.size();
        if s.min_element() < 2.0 {
            return;
        }
        let (nx, nz) = (
            (s.x / step).floor().max(1.0) as usize,
            (s.y / step).floor().max(1.0) as usize,
        );
        let mut placed = 0;
        for j in 0..nz {
            for i in 0..nx {
                if (sparse && (i + j) % 2 == 1) || placed >= most {
                    continue;
                }
                let c = r.min
                    + DVec2::new(
                        (i as f64 + 0.5) * s.x / nx as f64,
                        (j as f64 + 0.5) * s.y / nz as f64,
                    );
                // (One after another, from a start of the room's own, so
                // neighbours differ and no two rooms begin alike.)
                let first = (self.i + i + j * 2) % kinds.len();
                for t in 0..kinds.len() {
                    let (kind, w) = kinds[(first + t) % kinds.len()];
                    // (What is too tall for this room stays out of it.)
                    if kind.depth_height().1 + 0.3 > self.s.h {
                        continue;
                    }
                    // Its front towards the nearer long wall's walk.
                    let face = if s.x >= s.y {
                        if j * 2 < nz {
                            -DVec2::Y
                        } else {
                            DVec2::Y
                        }
                    } else if i * 2 < nx {
                        -DVec2::X
                    } else {
                        DVec2::X
                    };
                    if self.put(kind, c, face, w, 1.5) {
                        placed += 1;
                        break;
                    }
                }
            }
        }
    }

    /// A works' hall: machines in rows, benches and crates along the walls.
    fn works(&mut self) {
        self.grid(
            &[
                (ItemKind::Press, 2.2),
                (ItemKind::Lathe, 3.0),
                (ItemKind::Dynamo, 2.0),
                (ItemKind::Lathe, 3.0),
            ],
            6.0,
            false,
            24,
        );
        self.along_walls(
            4.5,
            &[
                (ItemKind::Workbench, 2.4),
                (ItemKind::Crates, 1.6),
                (ItemKind::Workbench, 2.4),
                (ItemKind::Cabinet, 0.48),
            ],
        );
    }

    /// A hideout's room: the room's own furniture where a gang would keep
    /// it (a kitchen to eat in, a lobby to guard), but cots, card tables,
    /// safes, planning boards, crates and drums besides.
    fn hideout(&mut self, kind: SpaceKind, entrance: Option<DVec2>) {
        const CLUTTER: &[(ItemKind, f64)] = &[
            (ItemKind::Cot, 0.9),
            (ItemKind::Crates, 1.6),
            (ItemKind::PinBoard, 2.4),
            (ItemKind::Drums, 1.1),
            (ItemKind::Safe, 1.0),
            (ItemKind::Crates, 1.6),
        ];
        match kind {
            SpaceKind::Lobby => self.lobby(entrance),
            SpaceKind::Atrium => self.lobby(None),
            SpaceKind::Vestibule => self.house_hall(entrance),
            SpaceKind::Kitchen => self.kitchen(),
            SpaceKind::Living => {
                self.grid(&[(ItemKind::PokerTable, 1.95)], 6.0, false, 1);
                self.living();
            }
            SpaceKind::Bedroom => {
                self.grid(&[(ItemKind::Cot, 0.9)], 2.4, false, 4);
            }
            SpaceKind::Gallery | SpaceKind::Arcade | SpaceKind::Concourse => {
                self.walkway();
                return;
            }
            SpaceKind::Hall | SpaceKind::Boardroom | SpaceKind::Shop | SpaceKind::Service => {
                self.grid(
                    &[
                        (ItemKind::PokerTable, 1.95),
                        (ItemKind::Crates, 1.6),
                        (ItemKind::Sandbags, 2.4),
                        (ItemKind::Cot, 0.9),
                        (ItemKind::Drums, 1.1),
                    ],
                    5.5,
                    false,
                    10,
                );
            }
            _ => return,
        }
        self.along_walls(4.0, CLUTTER);
    }

    /// A pavilion's hall: exhibits on the floor, cases and seats along
    /// the walls.
    fn expo(&mut self) {
        self.grid(EXHIBITS, 7.0, false, 12);
        self.along_walls(
            5.0,
            &[
                (ItemKind::Case, 1.6),
                (ItemKind::Sofa, 2.0),
                (ItemKind::Plant, 0.5),
                (ItemKind::Case, 1.6),
            ],
        );
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

    /// A night club: the band's stage at the far wall, a long bar with
    /// stools down one side, a dance floor in the middle with tables for
    /// four round it, booths along the walls that are left, a jukebox.
    fn club(&mut self) {
        let r = self.inner;
        let s = r.size();
        // The stage, as near the middle of the far wall as there is room
        // (a grand stair may stand there), else on a side wall.
        let sw = (s.x * 0.35).clamp(4.0, 9.0).min(s.min_element() - 2.0);
        let spots = [0.5, 0.3, 0.7, 0.2, 0.8];
        let mut stage = None;
        let widths = [sw, sw.min(6.0), sw.min(4.0)];
        'stage: for w in widths {
            for k in [1, 2, 3, 0] {
                let len = self.wall(k).3;
                for t in spots {
                    if self.against(k, len * t, ItemKind::Stage, w, 1.2) {
                        stage = Some(k);
                        break 'stage;
                    }
                }
            }
            // No wall has room (stairs, arcades, doors): out on the floor,
            // its back to the nearest side.
            for (fx, fz) in [(0.5, 0.8), (0.5, 0.2), (0.2, 0.5), (0.8, 0.5), (0.5, 0.65)] {
                let at = r.min + DVec2::new(s.x * fx, s.y * fz);
                let face = if fx != 0.5 {
                    DVec2::X * if fx < 0.5 { 1.0 } else { -1.0 }
                } else {
                    DVec2::Y * if fz < 0.5 { 1.0 } else { -1.0 }
                };
                if self.put(ItemKind::Stage, at, face, w, 1.2) {
                    break 'stage;
                }
            }
        }
        // The bar down a side wall (not the stage's).
        let mut bar = None;
        'bar: for k in [2, 3, 1] {
            if Some(k) == stage {
                continue;
            }
            let len = self.wall(k).3;
            let mut w = (len * 0.5).clamp(4.0, 10.0).min(len - 1.0);
            while w >= 3.0 {
                for t in spots {
                    if self.against(k, len * t, ItemKind::Bar, w, 1.2) {
                        bar = Some((k, len * t, w));
                        break 'bar;
                    }
                }
                w -= 1.0;
            }
        }
        if let Some((k, t, w)) = bar {
            let (o, along, n, _) = self.wall(k);
            let stools = ((w - 0.6) / 0.8).floor() as usize;
            for j in 0..stools {
                let x = t - w * 0.5 + 0.7 + j as f64 * 0.8;
                self.put(
                    ItemKind::BarStool,
                    o + along * x + n * (BAR_DEPTH + 0.45),
                    -n,
                    0.4,
                    0.0,
                );
            }
        }
        // Booths along what wall is left, a jukebox by the way in.
        let len = self.wall(0).3;
        for t in [0.12, 0.88, 0.25, 0.75] {
            if self.against(0, len * t, ItemKind::Jukebox, 0.8, 0.8) {
                break;
            }
        }
        for k in 0..4 {
            let len = self.wall(k).3;
            let n = (len / 2.3).floor() as usize;
            for j in 0..n {
                let t = (j as f64 + 0.5) * len / n as f64;
                self.against(k, t, ItemKind::Booth, 1.25, 0.35);
            }
        }
        // The dance floor, and tables for four round it.
        let c = r.center();
        let d = (s.min_element() * 0.34).clamp(3.5, 8.0);
        // (Only where there is room to dance.)
        let floor = s.min_element() >= 15.0 && self.rug(c, d);
        let ring = d * 0.5 + 2.4;
        let tables = ((ring * std::f64::consts::TAU) / 4.2)
            .floor()
            .clamp(4.0, 10.0) as usize;
        let mut spots: Vec<DVec2> = (0..tables)
            .map(|j| {
                let a = (j as f64 + 0.5) / tables as f64 * std::f64::consts::TAU;
                c + hw_core::det::dir2(a) * ring
            })
            .collect();
        // (A small club has no room for the ring: tables wherever the
        // floor is free, a few.)
        let (nx, nz) = ((s.x / 2.8).floor() as usize, (s.y / 2.8).floor() as usize);
        for iz in 0..nz {
            for ix in 0..nx {
                spots.push(
                    r.min
                        + DVec2::new(
                            (ix as f64 + 0.5) * s.x / nx as f64,
                            (iz as f64 + 0.5) * s.y / nz as f64,
                        ),
                );
            }
        }
        let mut placed = 0;
        for (j, at) in spots.into_iter().enumerate() {
            if j >= tables && placed >= 3 {
                break;
            }
            // (Round the dance floor, not on it.)
            if floor && at.distance(c) < d * 0.5 + 1.2 {
                continue;
            }
            if !self.put(ItemKind::Table, at, DVec2::Y, 1.2, 0.7) {
                continue;
            }
            placed += 1;
            for (off, face) in [
                (DVec2::new(0.0, -0.85), DVec2::Y),
                (DVec2::new(0.0, 0.85), -DVec2::Y),
                (DVec2::new(-0.95, 0.0), DVec2::X),
                (DVec2::new(0.95, 0.0), -DVec2::X),
            ] {
                self.put(ItemKind::Chair, at + off, face, 0.5, 0.0);
            }
        }
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

    /// A gang's balcony or bridge over its hall. A balcony: the walk
    /// along its rail (where the flights come and go), and behind it a
    /// line of sandbags, crates and drums to crouch behind with the gang's
    /// things against the wall. A bridge: sandbags at both rails, the walk
    /// down its middle.
    fn balcony(&mut self) {
        let size = self.inner.size();
        let along_x = size.x >= size.y;
        let (len, across) = if along_x {
            (size.x, size.y)
        } else {
            (size.y, size.x)
        };
        if len < 4.0 {
            return;
        }
        // Its long edges.
        let (a, b) = if along_x { (0, 1) } else { (2, 3) };
        let r = self.inner;
        // The walk, `lo` to `lo + DECK_WALK` across from edge `a`.
        let strip = |lo: f64| {
            if along_x {
                Rect::from_xz(r.min.x, r.min.y + lo, r.max.x, r.min.y + lo + DECK_WALK)
            } else {
                Rect::from_xz(r.min.x + lo, r.min.y, r.min.x + lo + DECK_WALK, r.max.y)
            }
        };
        let (wall, rail) = match (self.walls[a], self.walls[b]) {
            (true, false) => (a, b),
            (false, true) => (b, a),
            (false, false) => {
                if across < DECK_WALK + 2.0 * SANDBAGS_DEPTH {
                    return;
                }
                self.blocked.push(strip((across - DECK_WALK) * 0.5));
                let bags = [(ItemKind::Sandbags, 2.4), (ItemKind::Sandbags, 1.8)];
                for k in [a, b] {
                    self.row(k, 0.0, len, &bags, (1.4, 3.0));
                }
                return;
            }
            (true, true) => return,
        };
        // Across from the wall: its things, the cover, the walk.
        let cover_d = if across >= 4.0 { 1.2 } else { SANDBAGS_DEPTH };
        let by_wall = across - DECK_WALK - cover_d;
        if by_wall < -1e-9 {
            return;
        }
        self.blocked
            .push(strip(if rail == a { 0.0 } else { across - DECK_WALK }));
        let cover: &[(ItemKind, f64)] = if cover_d > 1.0 {
            &[
                (ItemKind::Sandbags, 2.4),
                (ItemKind::Crates, 1.6),
                (ItemKind::Sandbags, 1.8),
                (ItemKind::Drums, 1.1),
            ]
        } else {
            &[(ItemKind::Sandbags, 2.4), (ItemKind::Sandbags, 1.8)]
        };
        self.row(rail, DECK_WALK, len, cover, (1.0, 2.6));
        let things: Vec<(ItemKind, f64)> = [
            (ItemKind::PinBoard, 2.4),
            (ItemKind::Crates, 1.6),
            (ItemKind::Safe, 1.0),
            (ItemKind::Drums, 1.1),
            (ItemKind::Cabinet, 0.48),
        ]
        .into_iter()
        .filter(|(kind, _)| {
            let (d, h) = kind.depth_height();
            d <= by_wall + 1e-9 && h + 0.3 <= self.s.h
        })
        .collect();
        if !things.is_empty() {
            self.row(wall, 0.0, len, &things, (0.6, 1.8));
        }
    }

    /// Pieces picked from `kinds` along edge `k`, a gap between `gaps`
    /// apart, where they fit.
    /// `off` metres in from it.
    fn row(&mut self, k: usize, off: f64, len: f64, kinds: &[(ItemKind, f64)], gaps: (f64, f64)) {
        let mut t = self.rng.range(0.3, 1.5);
        while t < len {
            let (kind, w) = *self.rng.pick(kinds);
            if self.off_edge(k, t + w * 0.5, off, kind, w, 0.5) {
                t += w + self.rng.range(gaps.0, gaps.1);
            } else {
                t += 0.5;
            }
        }
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

/// What stands or hangs in space `s` up to head height: stairs,
/// landings, columns, shafts and their rails, with walking room round
/// them.
fn in_the_way(solid: &[Piece], s: &Space) -> Vec<Rect> {
    let mut blocked = Vec::new();
    for p in solid {
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
        if !s.rect.inset(0.3).contains_point(r.center()) && r.overlap(&s.rect.inset(0.3)) < 0.05 {
            continue;
        }
        blocked.push(r.inset(-CLEAR));
    }
    blocked
}

/// A balcony or bridge `i` in hall `host`, ready to furnish: its edges on
/// the hall's walls are walls, the rest rails; kept clear are its ways
/// in, the flights that leave or reach it and every join with the decks
/// beside it, and its middle (where the route map crosses it).
fn deck_room<'a>(l: &'a Layout, i: usize, host: usize, solid: &[Piece], rng: Rng) -> Room<'a> {
    let s = &l.spaces[i];
    let (r, hr) = (&s.rect, &l.spaces[host].rect);
    let e = 1e-3;
    let walls = [
        (r.min.y - hr.min.y).abs() < e,
        (r.max.y - hr.max.y).abs() < e,
        (r.min.x - hr.min.x).abs() < e,
        (r.max.x - hr.max.x).abs() < e,
    ];
    let mut blocked = in_the_way(solid, s);
    for k in l
        .links
        .iter()
        .filter(|k| (k.a == i || k.b == Some(i)) && (k.y - s.y).abs() < 0.1)
    {
        if k.kind.passable() {
            // Clear from the door right across to the walk at the rail.
            let d = k.apron().inset(-0.8);
            blocked.push(if k.along_x {
                Rect::from_xz(d.min.x, r.min.y, d.max.x, r.max.y)
            } else {
                Rect::from_xz(r.min.x, d.min.y, r.max.x, d.max.y)
            });
        }
    }
    for st in l.stairs.iter().filter(|st| st.host == host) {
        for f in &st.flights {
            if f.y1() > s.y - 1.0 && f.y0() < s.y + 2.3 {
                blocked.push(f.footprint().inset(-CLEAR));
            }
            let Flight::Straight {
                from, dir, risers, ..
            } = *f
            else {
                continue;
            };
            let a = DVec2::new(from.x, from.z);
            for (p, y) in [(a, f.y0()), (a + dir * Flight::run(risers), f.y1())] {
                if (y - s.y).abs() < 0.5 {
                    blocked.push(Rect::new(
                        p - DVec2::splat(DECK_APRON),
                        p + DVec2::splat(DECK_APRON),
                    ));
                }
            }
        }
    }
    // Where it meets another deck at its height: a strip into it, wide.
    for (j, t) in l.spaces.iter().enumerate() {
        if j == i || t.within != Some(host) || (t.y - s.y).abs() > 0.1 {
            continue;
        }
        if let Some((along_x, w, lo, hi)) = shared(r, &t.rect) {
            let (lo, hi) = (lo - 0.4, hi + 0.4);
            let into = if along_x {
                if (w - r.min.y).abs() < e {
                    Rect::from_xz(lo, w, hi, w + 1.8)
                } else {
                    Rect::from_xz(lo, w - 1.8, hi, w)
                }
            } else if (w - r.min.x).abs() < e {
                Rect::from_xz(w, lo, w + 1.8, hi)
            } else {
                Rect::from_xz(w - 1.8, lo, w, hi)
            };
            blocked.push(into);
        }
    }
    let c = r.center();
    blocked.push(Rect::new(c - DVec2::splat(0.7), c + DVec2::splat(0.7)));
    Room {
        i,
        s,
        inner: r.inset(OFF_WALL),
        blocked,
        walls,
        out: Vec::new(),
        rng,
    }
}

/// Every piece of furniture in the building.
pub fn items(l: &Layout) -> Vec<Item> {
    items_for(l, None, Fit::Plain)
}

/// The space a night club takes in a building that is one: the biggest
/// shop or lobby with a way in from the street (the leftmost of equals).
pub fn club_space(l: &Layout) -> Option<usize> {
    let area = |i: usize| {
        let s = l.spaces[i].rect.size();
        s.x * s.y
    };
    l.links
        .iter()
        .filter(|k| k.kind == LinkKind::Entrance)
        .filter(|k| matches!(l.spaces[k.a].kind, SpaceKind::Shop | SpaceKind::Lobby))
        .max_by(|a, b| {
            area(a.a)
                .total_cmp(&area(b.a))
                .then(b.at.x.total_cmp(&a.at.x))
        })
        .map(|k| k.a)
}

/// Every piece of furniture in the building; space `club`, if any, is
/// furnished as a night club, and the halls of a `lab` as laboratories.
pub fn items_for(l: &Layout, club: Option<usize>, fit: Fit) -> Vec<Item> {
    let solid = pieces(l);
    let seed = Seed(l.size.x.to_bits())
        .child(l.size.y.to_bits())
        .child(l.spaces.len() as u64)
        .child("furnish");
    let mut out = Vec::new();
    for (i, s) in l.spaces.iter().enumerate() {
        if let Some(host) = s.within {
            // A gang's den: its balconies and bridges are part of the fight.
            if fit == Fit::Hideout
                && l.scheme == Scheme::Den
                && matches!(s.kind, SpaceKind::Landing | SpaceKind::Bridge)
            {
                let mut room = deck_room(l, i, host, &solid, seed.child(i as u64).rng());
                room.balcony();
                out.append(&mut room.out);
            }
            continue;
        }
        if matches!(
            s.kind,
            SpaceKind::Shaft | SpaceKind::StairHall | SpaceKind::Canyon
        ) || (s.kind == SpaceKind::Vestibule && !l.scheme.homes())
        {
            continue;
        }
        let inner = s.rect.inset(OFF_WALL);
        let mut blocked = in_the_way(&solid, s);
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
            _ if club == Some(i) => room.club(),
            _ if fit == Fit::Hideout => room.hideout(s.kind, entrance),
            SpaceKind::Hall | SpaceKind::Boardroom | SpaceKind::Shop | SpaceKind::Service
                if fit != Fit::Plain =>
            {
                match fit {
                    Fit::Lab => room.laboratory(),
                    Fit::Works => room.works(),
                    _ => room.expo(),
                }
            }
            SpaceKind::Lobby | SpaceKind::Atrium if fit != Fit::Plain => {
                room.lobby(entrance.filter(|_| s.kind == SpaceKind::Lobby));
                match fit {
                    Fit::Lab => room.grid(&[(ItemKind::AetherCoil, 1.2)], 7.0, true, 6),
                    Fit::Works => room.grid(
                        &[
                            (ItemKind::Dynamo, 2.0),
                            (ItemKind::Press, 2.2),
                            (ItemKind::Lathe, 3.0),
                        ],
                        7.0,
                        false,
                        10,
                    ),
                    _ => room.grid(EXHIBITS, 7.0, false, 10),
                }
            }
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
            let size = DVec2::new(12.0 + seed as f64, 8.5 + (seed % 4) as f64);
            out.push(plan_scheme(Seed(90 + seed), size, 2, Scheme::Loft).expect("it fits"));
        }
        out
    }

    /// A night club has its stage, its bar with stools, booths and
    /// tables, in a lobby and in a shop house's shop alike.
    #[test]
    fn a_club_gets_a_stage_a_bar_and_booths() {
        for l in plans() {
            let Some(club) = club_space(&l) else { continue };
            let its: Vec<Item> = items_for(&l, Some(club), Fit::Plain)
                .into_iter()
                .filter(|i| i.space == club)
                .collect();
            let n = |k: ItemKind| its.iter().filter(|i| i.kind == k).count();
            let what = format!("{:?} {}", l.scheme, l.size);
            assert_eq!(n(ItemKind::Stage), 1, "{what}: the stage");
            assert_eq!(n(ItemKind::Bar), 1, "{what}: the bar");
            assert!(n(ItemKind::BarStool) >= 3, "{what}: stools");
            assert!(
                n(ItemKind::Booth) + n(ItemKind::Table) >= 3,
                "{what}: seats"
            );
            assert_eq!(n(ItemKind::Counter) + n(ItemKind::Shelves), 0, "{what}");
        }
    }

    /// A laboratory's halls hold benches and coils, not desks.
    #[test]
    fn a_lab_gets_benches_and_coils() {
        for l in plans() {
            // (A lab is never a house of flats.)
            if l.scheme.homes() {
                continue;
            }
            let its = items_for(&l, None, Fit::Lab);
            let n = |k: ItemKind| its.iter().filter(|i| i.kind == k).count();
            let what = format!("{:?} {}", l.scheme, l.size);
            assert!(n(ItemKind::LabBench) >= 4, "{what}: benches");
            assert_eq!(n(ItemKind::Desk) + n(ItemKind::Shelves), 0, "{what}");
            if l.size.x >= 30.0 {
                assert!(n(ItemKind::AetherCoil) >= 1, "{what}: coils");
            }
        }
    }

    /// A works holds machines and a pavilion exhibits, not desks.
    #[test]
    fn works_get_machines_and_pavilions_exhibits() {
        use ItemKind as K;
        for l in plans() {
            if l.scheme.homes() {
                continue;
            }
            let what = format!("{:?} {}", l.scheme, l.size);
            for (fit, kinds, least) in [
                (Fit::Works, [K::Press, K::Lathe, K::Dynamo], 3),
                (Fit::Expo, [K::RocketModel, K::AtomModel, K::Orrery], 2),
            ] {
                let its = items_for(&l, None, fit);
                let n = |k: K| its.iter().filter(|i| i.kind == k).count();
                let all: usize = kinds.iter().map(|k| n(*k)).sum();
                // (A small works is a workshop with rooms above.)
                let least = if l.scheme == Scheme::ShopHouse {
                    2
                } else {
                    least * 2
                };
                assert!(all >= least, "{what} {fit:?}: only {all} big pieces");
                assert_eq!(n(K::Desk) + n(K::Shelves), 0, "{what} {fit:?}");
                // Nothing stands taller than its room.
                for it in &its {
                    let h = l.spaces[it.space].h;
                    assert!(it.kind.depth_height().1 < h, "{what}: {:?} in {h}", it.kind);
                }
            }
        }
    }

    /// Furniture stands in its room, on its floor, clear of every wall,
    /// stair, column and of the other pieces, and never in a doorway.
    #[test]
    fn furniture_stands_clear_of_everything() {
        for l in plans() {
            let its = items(&l);
            let least = match l.scheme {
                Scheme::Loft => 8,
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
            assert_clear(&l, &its);
        }
    }

    /// A gang's den: its balconies and bridges carry sandbags and crates
    /// at their rails and the gang's things at their walls, clear of
    /// stairs, doorways and each other, and a walk is left along each.
    #[test]
    fn den_balconies_get_cover() {
        let (mut dens, mut decks, mut covered, mut sandbags) = (0, 0, 0, 0);
        for k in 0..60u64 {
            let size = DVec2::new(12.0 + (k % 7) as f64 * 0.5, 22.0 + (k % 6) as f64 * 2.0);
            let Some(l) = plan_scheme(Seed(k), size, 3 + (k % 2) as u32, Scheme::Den) else {
                continue;
            };
            dens += 1;
            let its = items_for(&l, None, Fit::Hideout);
            assert_eq!(
                its,
                items_for(&l, None, Fit::Hideout),
                "the same every time"
            );
            assert_clear(&l, &its);
            for (i, s) in l.spaces.iter().enumerate() {
                if s.within.is_none() || s.rect.size().min_element() < 2.9 {
                    continue;
                }
                decks += 1;
                let mine: Vec<&Item> = its.iter().filter(|it| it.space == i).collect();
                covered += usize::from(!mine.is_empty());
                sandbags += mine
                    .iter()
                    .filter(|it| it.kind == ItemKind::Sandbags)
                    .count();
                // A walk along it: a strip of DECK_WALK somewhere across
                // that nothing stands in.
                let along_x = s.rect.size().x >= s.rect.size().y;
                let (lo, hi) = if along_x {
                    (s.rect.min.y, s.rect.max.y)
                } else {
                    (s.rect.min.x, s.rect.max.x)
                };
                let free = |v: f64| {
                    mine.iter().filter(|it| it.kind.solid()).all(|it| {
                        let (c, h) = if along_x {
                            (it.at.y, it.half().y)
                        } else {
                            (it.at.x, it.half().x)
                        };
                        c + h <= v + 1e-6 || c - h >= v + DECK_WALK - 1e-6
                    })
                };
                let mut v = lo;
                let mut found = false;
                while v + DECK_WALK <= hi + 1e-6 {
                    found |= free(v);
                    v += 0.02;
                }
                assert!(found, "no walk along deck {i} {:?}: {mine:?}", s.rect);
            }
        }
        assert!(dens > 40, "{dens}");
        assert!(
            covered * 3 > decks * 2,
            "{covered} of {decks} decks furnished"
        );
        assert!(sandbags > decks, "{sandbags} sandbags on {decks} decks");
    }

    fn assert_clear(l: &Layout, its: &[Item]) {
        {
            let solids: Vec<(Part, Rect, (f64, f64))> = pieces(l)
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
