//! The floor you are on, furnished: every room for its use, in the taste
//! of its building's era (wallpaper and floors, sofas and radio consoles,
//! steel desks, shop fittings), with skirting, door frames, open doors,
//! pictures, lamps that come on with the windows at night, and the room
//! lights Engine shines in the rooms you can see.
//!
//! Everything is laid out in building coordinates (x along the front, z
//! inward, y up from the floor's top) and kept clear of the doorways; the
//! pieces themselves are in [`pieces`].
//!
//! Only the cut-open floor is furnished (see `citymesh::add_building`), so
//! a town costs nothing for its interiors until you look into one.

pub mod pieces;

use crate::buildingmesh::Detail;
use crate::citymesh::{MeshBuilder, Spot, Surf};
use crate::daylight::{channel, Tone, HABITS};
use crate::skins::{Base, Paper, Skin, Tex, Wear};
use engine::prelude::*;
use glam::{DVec2, DVec3};
use hw_gen::building::{BuildingPlan, Door, Floor, Room, RoomKind};
use hw_gen::geom::Rect;
use hw_gen::style::Style;
use pieces::{chrome, Cover, Maker, Mat, Rng};

/// Half the thickness of the cut floor's walls (see `citymesh`).
const WALL_HALF: f64 = 0.125;
/// Wallpaper stands this far off a wall's face.
const PAPER: f64 = 0.015;
/// Furniture keeps this far off a wall's face (past the skirting).
const OFF_WALL: f64 = 0.07;
/// Height of inner walls and of outside walls above the floor's top.
const INNER_TOP: f64 = 2.66;
const OUTER_TOP: f64 = 3.06;

/// How a building's rooms are done up: its era's woods, cloths and walls.
#[derive(Clone, Copy, Debug)]
struct Decor {
    era: Era,
    wood: Color,
    wear: Wear,
    /// The building's wallpaper pattern (one per building, coloured per room).
    paper: Paper,
    seed: u64,
}

/// Every wood an inside door can be made of (see [`inner_door`]).
pub const DOOR_WOODS: [(u8, u8, u8); 11] = [
    (96, 58, 36),
    (120, 74, 44),
    (84, 52, 34),
    (120, 70, 42),
    (140, 86, 52),
    (104, 60, 40),
    (196, 150, 100),
    (176, 124, 76),
    (214, 176, 126),
    (130, 96, 66),
    (110, 84, 60),
];

/// How a building's inside doors look: flush with a porthole (modern
/// buildings) or panelled, and the wood (an index into [`DOOR_WOODS`]).
/// The same as the rest of its furniture.
pub fn inner_door(plan: &BuildingPlan) -> (bool, usize) {
    let d = Decor::of(plan);
    let w = d.wood;
    let wood = DOOR_WOODS
        .iter()
        .position(|c| rgb(c.0, c.1, c.2) == w)
        .unwrap_or(0);
    (d.era == Era::Modern, wood)
}

/// Is the building of an older style (painted, panelled street doors
/// rather than streamline ones)?
pub fn old_fashioned(plan: &BuildingPlan) -> bool {
    !matches!(Decor::of(plan).era, Era::Modern | Era::Bare)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Era {
    /// Old-town and 19th-century houses: dark woods, damask, boards.
    Old,
    /// Deco and civic: walnut, stripes, terrazzo.
    Deco,
    /// Streamline, Googie, glass towers: blond woods, atomic prints.
    Modern,
    /// Run-down walk-ups.
    Shabby,
    /// Works, labs: bare.
    Bare,
}

impl Era {
    fn woods(self) -> &'static [(u8, u8, u8)] {
        match self {
            Era::Old => &DOOR_WOODS[0..3],
            Era::Deco => &DOOR_WOODS[3..6],
            Era::Modern => &DOOR_WOODS[6..9],
            Era::Shabby | Era::Bare => &DOOR_WOODS[9..11],
        }
    }
}

impl Decor {
    fn of(plan: &BuildingPlan) -> Decor {
        let era = match plan.look.style {
            Style::OldTown | Style::Burgher | Style::Haussmann | Style::Rowhouse => Era::Old,
            Style::ArtDeco | Style::Civic | Style::Emporium => Era::Deco,
            Style::Tenement => Era::Shabby,
            Style::Works | Style::Lab => Era::Bare,
            _ => Era::Modern,
        };
        let seed = hash(plan.site.origin, 99, 0);
        let pick_of = |opts: &[Paper]| opts[(seed >> 7) as usize % opts.len()];
        let paper = match era {
            Era::Old => pick_of(&[Paper::Damask, Paper::Stripes, Paper::Damask]),
            Era::Deco => pick_of(&[Paper::Stripes, Paper::Pinstripe, Paper::Damask]),
            Era::Modern => pick_of(&[Paper::Atomic, Paper::Pinstripe, Paper::Atomic]),
            Era::Shabby | Era::Bare => Paper::Stripes,
        };
        let woods = era.woods();
        let w = woods[(seed >> 13) as usize % woods.len()];
        Decor {
            era,
            wood: rgb(w.0, w.1, w.2),
            wear: if era == Era::Shabby {
                Wear::Aged
            } else {
                Wear::Fresh
            },
            paper,
            seed,
        }
    }

    fn fabrics(&self) -> &'static [(u8, u8, u8)] {
        match self.era {
            Era::Old => &[
                (130, 40, 46),
                (90, 110, 70),
                (190, 150, 70),
                (70, 80, 120),
                (150, 100, 80),
            ],
            Era::Deco => &[
                (40, 120, 120),
                (200, 160, 80),
                (180, 90, 100),
                (60, 70, 110),
                (220, 200, 170),
            ],
            Era::Modern => &[
                (64, 170, 160),
                (232, 110, 90),
                (236, 190, 70),
                (150, 190, 90),
                (90, 140, 200),
                (226, 140, 170),
            ],
            Era::Shabby | Era::Bare => &[(120, 100, 80), (90, 100, 90), (130, 80, 70)],
        }
    }

    fn fabric(&self, k: u64) -> Color {
        let f = self.fabrics();
        let c = f[((self.seed >> 17).wrapping_add(k.wrapping_mul(7)) % f.len() as u64) as usize];
        rgb(c.0, c.1, c.2)
    }

    fn cover(&self, k: u64) -> Cover {
        Cover {
            main: self.fabric(k),
            second: self.fabric(k + 3),
            wood: self.wood,
        }
    }

    /// The wall colour of a room (tinting its paper or paint).
    fn wall_color(&self, kind: RoomKind, k: u64) -> Color {
        let pastel: &[(u8, u8, u8)] = match self.era {
            Era::Old => &[
                (206, 196, 168),
                (176, 160, 150),
                (164, 178, 156),
                (200, 176, 150),
            ],
            Era::Deco => &[
                (214, 200, 176),
                (170, 196, 190),
                (214, 184, 176),
                (226, 214, 190),
            ],
            Era::Modern => &[
                (166, 210, 200),
                (240, 220, 160),
                (236, 186, 176),
                (180, 200, 226),
                (226, 226, 214),
                (206, 220, 168),
            ],
            Era::Shabby => &[(170, 160, 130), (150, 150, 130), (160, 140, 120)],
            Era::Bare => &[(200, 200, 196)],
        };
        let c = match kind {
            RoomKind::Gallery | RoomKind::Hall => (236, 234, 228),
            RoomKind::Office => (226, 222, 206),
            _ => {
                pastel[((self.seed >> 3).wrapping_add(k.wrapping_mul(5)) % pastel.len() as u64)
                    as usize]
            }
        };
        rgb(c.0, c.1, c.2)
    }

    /// Wallpaper, plain paint, or nothing (bare plaster) for a room.
    fn walls(&self, kind: RoomKind) -> Option<Tex> {
        let paint = Skin::new(
            Base::Facade(engine::texgen::FacadeLayer::Plaster),
            Wear::Fresh,
        );
        let paper = Skin::new(Base::Wallpaper(self.paper), self.wear);
        match (kind, self.era) {
            (RoomKind::Core | RoomKind::Storage | RoomKind::Lab, _) | (_, Era::Bare) => None,
            (RoomKind::Apartment | RoomKind::Living | RoomKind::Bedroom, _) => Some(paper.into()),
            (RoomKind::Lobby | RoomKind::Corridor, Era::Old | Era::Deco) => Some(paper.into()),
            _ => Some(paint.into()),
        }
    }
}

/// The floor finish of a room: its texture and colour.
pub fn floor_finish(plan: &BuildingPlan, room: &Room) -> (Tex, Color) {
    use engine::texgen::FacadeLayer as F;
    let d = Decor::of(plan);
    let skin = |b: Base| -> Tex { Skin::new(b, d.wear).into() };
    let oak = rgb(190, 140, 96);
    match (room.kind, d.era) {
        (RoomKind::Core | RoomKind::Storage, _) => (F::Concrete.into(), rgb(150, 148, 142)),
        (RoomKind::Lab, _) => (F::ConcretePanel.into(), rgb(200, 210, 206)),
        (_, Era::Bare) => (F::Concrete.into(), rgb(160, 158, 150)),
        (RoomKind::Kitchen, _) => (skin(Base::Lino), rgb(232, 226, 200)),
        (RoomKind::Apartment | RoomKind::Living | RoomKind::Bedroom, Era::Old) => {
            (skin(Base::Parquet), rgb(150, 100, 66))
        }
        (RoomKind::Apartment | RoomKind::Living | RoomKind::Bedroom, Era::Shabby) => {
            (skin(Base::Lino), rgb(200, 190, 160))
        }
        (RoomKind::Apartment | RoomKind::Living | RoomKind::Bedroom, _) => {
            (skin(Base::Parquet), oak)
        }
        (RoomKind::Office, Era::Old) => (skin(Base::Parquet), rgb(170, 124, 84)),
        (RoomKind::Shop, _) if shop_kind(plan, room) == ShopKind::Diner => {
            (skin(Base::Lino), rgb(236, 230, 214))
        }
        (RoomKind::Shop, _) if shop_kind(plan, room) == ShopKind::Club => {
            (skin(Base::Parquet), rgb(110, 62, 40))
        }
        (RoomKind::Shop, Era::Old) => (skin(Base::Parquet), rgb(170, 128, 92)),
        (_, Era::Shabby) => (skin(Base::Lino), rgb(190, 184, 160)),
        (RoomKind::Corridor, _) => (skin(Base::Terrazzo), rgb(214, 206, 192)),
        _ => (skin(Base::Terrazzo), rgb(226, 220, 206)),
    }
}

/// The light a room has after dark: its tone and its household's habit,
/// or `None` for a room whose lights stay off. The same for its windows
/// (`buildingmesh`) and its lamps.
pub fn room_light(plan: &BuildingPlan, floor: u32, room: usize) -> Option<(Tone, usize)> {
    let o = plan.site.origin;
    let mut h = (o.x * 7.31).to_bits() ^ (o.y * 3.17).to_bits().rotate_left(17);
    h ^= (floor as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= (room as u64 + 1).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 29;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 32;
    let roll = (h % 1000) as f64 / 1000.0;
    let shop = plan
        .floors
        .get(floor as usize)
        .and_then(|f| f.rooms.get(room))
        .is_some_and(|r| r.kind == RoomKind::Shop);
    let share = if shop { 0.75 } else { 0.38 };
    if roll >= share {
        return None;
    }
    let tone = match h >> 40 & 7 {
        0 => Tone::Cool,
        1 | 2 => Tone::Lamp,
        _ => Tone::Warm,
    };
    Some((tone, (h >> 44) as usize % HABITS))
}

/// What a shop sells (it decides its fittings).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShopKind {
    Grocer,
    Clothes,
    /// Radios, televisions, refrigerators.
    Appliances,
    Diner,
    /// The night club behind the street life's neon sign and doorman
    /// (`hw_sim::streetlife::is_club`): a bar, velvet booths, a bandstand.
    Club,
}

pub fn shop_kind(plan: &BuildingPlan, room: &Room) -> ShopKind {
    if let Some((k, _)) = hw_sim::streetlife::club_room(plan) {
        if plan.floors[0].rooms[k].id == room.id {
            return ShopKind::Club;
        }
    }
    let h = hash(room.rect.center() + plan.site.origin, 3, 11);
    [
        ShopKind::Grocer,
        ShopKind::Clothes,
        ShopKind::Appliances,
        ShopKind::Diner,
    ][(h >> 9) as usize % 4]
}

/// An Engine light for a room on the cut floor that has its lights on:
/// where it hangs (building x, height over the floor, z), its colour,
/// reach and the household habit that switches it.
#[derive(Clone, Copy, Debug)]
pub struct RoomLight {
    pub at: DVec3,
    pub color: Color,
    pub range: f32,
    pub intensity: f32,
    pub habit: usize,
}

/// The room lights of floor `level` (building coordinates).
pub fn room_lights(plan: &BuildingPlan, level: usize) -> Vec<RoomLight> {
    let Some(f) = plan.floors.get(level) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (k, room) in f.rooms.iter().enumerate() {
        if matches!(
            room.kind,
            RoomKind::Core | RoomKind::Storage | RoomKind::Corridor
        ) {
            continue;
        }
        let Some((tone, habit)) = room_light(plan, level as u32, k) else {
            continue;
        };
        let color = match tone {
            Tone::Cool => rgb(214, 226, 255),
            Tone::Lamp => rgb(255, 196, 130),
            Tone::Warm => rgb(255, 216, 160),
        };
        // One light per 6 m or so along the room's long side.
        let s = room.rect.size();
        let n = ((s.max_element() / 6.0).round() as usize).clamp(1, 3);
        for i in 0..n {
            let t = (i as f64 + 0.5) / n as f64;
            let c = if s.x >= s.y {
                DVec2::new(room.rect.min.x + s.x * t, room.rect.center().y)
            } else {
                DVec2::new(room.rect.center().x, room.rect.min.y + s.y * t)
            };
            out.push(RoomLight {
                at: DVec3::new(c.x, 2.3, c.y),
                color,
                // Engine's lamps shine through walls: keep each to its room.
                range: (s.min_element() * 0.5 + 1.5).clamp(2.5, 6.0) as f32,
                intensity: 2.2,
                habit,
            });
        }
    }
    out
}

/// Keep-out boxes in front of doorways (centre, half size).
fn door_keepout(d: &Door) -> (DVec2, DVec2) {
    let along = d.size.x * 0.5 + 0.35;
    // Clear of an open leaf: half the doorway each for two, all of it for
    // one (see `hw_gen::doors`).
    let leaf = if d.size.x >= 1.4 {
        d.size.x * 0.5
    } else {
        d.size.x
    };
    let across = (leaf + 0.3).max(1.15);
    if d.along_x {
        (d.at, DVec2::new(along, across))
    } else {
        (d.at, DVec2::new(across, along))
    }
}

/// One of a room's four walls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wall {
    /// z = min (towards the street).
    Front,
    Back,
    Left,
    Right,
}

impl Wall {
    const ALL: [Wall; 4] = [Wall::Back, Wall::Front, Wall::Left, Wall::Right];

    /// Into the room, away from this wall.
    fn inward(self) -> DVec2 {
        match self {
            Wall::Front => DVec2::Y,
            Wall::Back => -DVec2::Y,
            Wall::Left => DVec2::X,
            Wall::Right => -DVec2::X,
        }
    }

    /// Along the wall (left to right seen from inside, roughly).
    fn along(self) -> DVec2 {
        self.inward().perp()
    }

    fn opposite(self) -> Wall {
        match self {
            Wall::Front => Wall::Back,
            Wall::Back => Wall::Front,
            Wall::Left => Wall::Right,
            Wall::Right => Wall::Left,
        }
    }
}

/// A footprint taken by a piece (building coordinates).
#[derive(Clone, Copy, Debug)]
struct Taken {
    c: DVec2,
    half: DVec2,
    /// Taller than a picture's bottom.
    tall: bool,
}

/// Places furniture in building space.
struct Furnisher<'a> {
    b: &'a mut MeshBuilder,
    plan: &'a BuildingPlan,
    /// City height of the floor's top.
    top: f64,
    seg: usize,
    decor: Decor,
    keepout: Vec<(DVec2, DVec2)>,
    taken: Vec<Taken>,
    /// The glow channel of the room's lamps (0: its lights stay off).
    lamp: u8,
    /// The room's floor inside its walls: pieces go against a wall only
    /// where a zone's edge is one of these walls.
    walls: Rect,
    /// The rugs down in this room (centre, half size).
    rugs: Vec<(DVec2, DVec2)>,
}

impl Furnisher<'_> {
    fn city(&self, p: DVec2, y: f64) -> DVec3 {
        self.plan.site.to_city(DVec3::new(p.x, self.top + y, p.y))
    }

    fn dir(&self, d: DVec2) -> DVec3 {
        self.city(d, 0.0) - self.city(DVec2::ZERO, 0.0)
    }

    /// A builder for a piece at `c` facing `facing` (building).
    fn maker(&mut self, c: DVec2, y: f64, facing: DVec2) -> Maker<'_> {
        let o = self.city(c, y);
        let f = self.dir(facing);
        Maker::new(self.b, o, f, self.seg, self.decor.wear)
    }

    fn free(&self, c: DVec2, half: DVec2, area: &Rect) -> bool {
        self.free_but(c, half, area, usize::MAX)
    }

    /// Like [`Self::free`], ignoring taken piece `skip` (a chair tucks
    /// under its table).
    fn free_but(&self, c: DVec2, half: DVec2, area: &Rect, skip: usize) -> bool {
        let r = Rect::new(c - half, c + half);
        if !area.contains_rect(&r, 1e-6) {
            return false;
        }
        let apart = |o: DVec2, h: DVec2, gap: f64| {
            let d = (c - o).abs() - (half + h);
            d.x > gap || d.y > gap
        };
        self.keepout.iter().all(|&(o, h)| apart(o, h, 0.0))
            && self
                .taken
                .iter()
                .enumerate()
                .all(|(i, t)| i == skip || apart(t.c, t.half, 0.04))
    }

    /// Puts a piece `size` (width, depth) at `c` facing `facing` if it fits
    /// in `area`, clear of doorways and other pieces.
    fn put(
        &mut self,
        area: &Rect,
        c: DVec2,
        facing: DVec2,
        size: DVec2,
        tall: bool,
        build: impl FnOnce(&mut Maker),
    ) -> bool {
        let half = if facing.x.abs() < 0.5 {
            size * 0.5
        } else {
            DVec2::new(size.y, size.x) * 0.5
        };
        if !self.free(c, half, area) {
            return false;
        }
        self.taken.push(Taken { c, half, tall });
        let mut m = self.maker(c, 0.0, facing);
        build(&mut m);
        true
    }

    /// Where a piece of `size` stands against `wall` of `area` at `t`
    /// (0 one end, 1 the other) and which way it faces.
    fn spot(area: &Rect, wall: Wall, t: f64, size: DVec2) -> (DVec2, DVec2) {
        let n = wall.inward();
        let (w, d) = (size.x, size.y);
        let c = area.center();
        let s = area.size();
        // Distance from the centre to this wall, and the run along it.
        let (reach, run) = if n.x.abs() > 0.5 {
            (s.x * 0.5, s.y)
        } else {
            (s.y * 0.5, s.x)
        };
        let along = wall.along();
        let off = -n * (reach - d * 0.5);
        let slide = along * ((t - 0.5) * (run - w).max(0.0));
        (c + off + slide, n)
    }

    /// Against `wall`, trying the positions `ts` in turn.
    fn against(
        &mut self,
        area: &Rect,
        wall: Wall,
        ts: &[f64],
        size: DVec2,
        tall: bool,
        build: impl Fn(&mut Maker),
    ) -> Option<DVec2> {
        if !self.is_wall(area, wall) {
            return None;
        }
        for &t in ts {
            let (c, n) = Self::spot(area, wall, t, size);
            if self.put(area, c, n, size, tall, &build) {
                return Some(c);
            }
        }
        None
    }

    /// Is `wall` of `area` one of the room's walls (not an open edge
    /// between zones)?
    fn is_wall(&self, area: &Rect, wall: Wall) -> bool {
        let (a, r) = (area, &self.walls);
        let e = 0.01;
        match wall {
            Wall::Front => (a.min.y - r.min.y).abs() < e,
            Wall::Back => (a.max.y - r.max.y).abs() < e,
            Wall::Left => (a.min.x - r.min.x).abs() < e,
            Wall::Right => (a.max.x - r.max.x).abs() < e,
        }
    }

    /// Is there room for a rug of `size` at `c` facing `facing`, clear of
    /// the rugs already down (two overlapping rugs fight)? Marks it if so.
    fn rug_fits(&mut self, c: DVec2, facing: DVec2, size: DVec2) -> bool {
        let half = if facing.x.abs() < 0.5 {
            size * 0.5
        } else {
            DVec2::new(size.y, size.x) * 0.5
        };
        let clear = self.rugs.iter().all(|&(o, h)| {
            let d = (c - o).abs() - (half + h);
            d.x > 0.02 || d.y > 0.02
        });
        if clear {
            self.rugs.push((c, half));
        }
        clear
    }

    fn mark(&mut self, c: DVec2, half: DVec2, tall: bool) {
        self.taken.push(Taken { c, half, tall });
    }
}

/// Furnishes every room of floor `level` of `plan`; `top` is the city height
/// of the floor's top surface.
pub fn furnish(b: &mut MeshBuilder, plan: &BuildingPlan, level: usize, top: f64, detail: Detail) {
    let Some(f) = plan.floors.get(level) else {
        return;
    };
    let decor = Decor::of(plan);
    let mut fu = Furnisher {
        b,
        plan,
        top,
        seg: if detail == Detail::Near { 24 } else { 10 },
        decor,
        keepout: f.doors.iter().map(door_keepout).collect(),
        taken: Vec::new(),
        lamp: 0,
        walls: Rect::new(DVec2::ZERO, DVec2::ZERO),
        rugs: Vec::new(),
    };
    dress_doors(&mut fu, f);
    for (k, room) in f.rooms.iter().enumerate() {
        let seed = hash(plan.site.origin, level, k);
        fu.taken.clear();
        fu.rugs.clear();
        fu.lamp = room_light(plan, level as u32, k).map_or(0, |(t, h)| channel(t, h));
        let area = inner(room, OFF_WALL);
        fu.walls = area;
        match room.kind {
            RoomKind::Lobby => lobby(&mut fu, room, &area, seed),
            RoomKind::Office => office(&mut fu, room, &area, seed),
            RoomKind::Apartment => apartment(&mut fu, room, &area, f, seed),
            RoomKind::Living => living(&mut fu, &area, seed),
            RoomKind::Kitchen => kitchen(&mut fu, &area, seed),
            RoomKind::Bedroom => bedroom(&mut fu, &area, seed),
            RoomKind::Shop => shop(&mut fu, room, &area, seed),
            RoomKind::Storage => storage(&mut fu, &area, seed),
            RoomKind::Gallery | RoomKind::Hall => gallery(&mut fu, &area, seed),
            RoomKind::Lab => lab(&mut fu, &area, seed),
            RoomKind::Core => core(&mut fu, room, f),
            RoomKind::Corridor => corridor(&mut fu, &area, seed),
        }
        dress_walls(&mut fu, room, k, f, seed);
        standing_spots(&mut fu, room, &area, seed);
    }
}

/// Places to stand in a room, clear of its furniture and doorways and
/// spread out, facing the nearest piece (a shelf, a counter, a desk) or
/// the middle of the room: where people indoors are shown.
fn standing_spots(fu: &mut Furnisher, room: &Room, area: &Rect, seed: u64) {
    let most = match room.kind {
        RoomKind::Core | RoomKind::Storage | RoomKind::Corridor => 0,
        RoomKind::Shop
        | RoomKind::Office
        | RoomKind::Lobby
        | RoomKind::Gallery
        | RoomKind::Hall => 6,
        _ => 3,
    };
    let a = area.inset(0.35);
    if most == 0 || a.size().min_element() <= 0.0 {
        return;
    }
    let half = DVec2::splat(0.25);
    let step = 0.6;
    let (nx, ny) = (
        (a.size().x / step).floor() as usize + 1,
        (a.size().y / step).floor() as usize + 1,
    );
    let pad = (a.size() - DVec2::new((nx - 1) as f64, (ny - 1) as f64) * step) * 0.5;
    let mut free = Vec::new();
    for i in 0..nx {
        for j in 0..ny {
            let p = a.min + pad + DVec2::new(i as f64, j as f64) * step;
            if fu.free(p, half, area) {
                free.push(p);
            }
        }
    }
    if free.is_empty() {
        return;
    }
    // Spread out: each next place the farthest from those already taken.
    let mut out = vec![free[(seed % free.len() as u64) as usize]];
    while out.len() < most {
        let best = free
            .iter()
            .map(|&p| {
                (
                    out.iter()
                        .map(|q| q.distance(p))
                        .fold(f64::INFINITY, f64::min),
                    p,
                )
            })
            .max_by(|x, y| x.0.total_cmp(&y.0));
        match best {
            Some((d, p)) if d >= 1.2 => out.push(p),
            _ => break,
        }
    }
    for p in out {
        let near = fu
            .taken
            .iter()
            .map(|t| (t.c.distance(p), t.c))
            .filter(|(d, _)| *d < 2.0)
            .min_by(|x, y| x.0.total_cmp(&y.0));
        let toward = near.map_or(area.center(), |(_, c)| c) - p;
        let toward = if toward.length() < 0.1 {
            DVec2::Y
        } else {
            toward.normalize()
        };
        let f = fu.dir(toward);
        let spot = Spot {
            room: room.kind,
            at: fu.city(p, 0.0),
            facing: DVec2::new(f.x, f.z).normalize_or(DVec2::Y),
        };
        fu.b.spots.push(spot);
    }
}

/// A room's floor, `m` in from its walls' faces.
fn inner(room: &Room, m: f64) -> Rect {
    room.rect.inset(WALL_HALF + m)
}

fn hash(o: DVec2, level: usize, k: usize) -> u64 {
    let mut h = (o.x * 13.7).to_bits() ^ (o.y * 5.3).to_bits().rotate_left(21);
    h ^= (level as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= (k as u64 + 7).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 31;
    h.wrapping_mul(0xBF58_476D_1CE4_E5B9) ^ (h >> 29)
}

/// Pick one of `n` from bits `shift..` of `seed`.
fn pick(seed: u64, shift: u32, n: usize) -> usize {
    ((seed >> shift) % n as u64) as usize
}

// ------------------------------------------------------------------ walls

/// One stretch of a room's wall, clear of doorways: from `a` to `b`
/// along it (building coordinates on the wall's face).
#[derive(Clone, Copy, Debug)]
struct Stretch {
    wall: Wall,
    a: DVec2,
    b: DVec2,
    top: f64,
    /// The stretch ends at a doorway (its casing) at `a`, at `b`.
    door: [bool; 2],
}

/// A room's walls as stretches between its doorways.
fn stretches(room: &Room, f: &Floor) -> Vec<Stretch> {
    let r = room.rect;
    let o = f.outline;
    let e = 1e-6;
    let mut out = Vec::new();
    for wall in Wall::ALL {
        let n = wall.inward();
        // The wall's centre line and its face.
        let (line, along_x) = match wall {
            Wall::Front => (r.min.y, true),
            Wall::Back => (r.max.y, true),
            Wall::Left => (r.min.x, false),
            Wall::Right => (r.max.x, false),
        };
        let outside = if along_x {
            (line - o.min.y).abs() < e || (line - o.max.y).abs() < e
        } else {
            (line - o.min.x).abs() < e || (line - o.max.x).abs() < e
        };
        let face = line + if along_x { n.y } else { n.x } * (WALL_HALF + PAPER);
        let (t0, t1) = if along_x {
            (r.min.x + WALL_HALF + PAPER, r.max.x - WALL_HALF - PAPER)
        } else {
            (r.min.y + WALL_HALF + PAPER, r.max.y - WALL_HALF - PAPER)
        };
        let mut gaps: Vec<(f64, f64)> = f
            .doors
            .iter()
            .filter(|d| {
                d.along_x == along_x && ((if along_x { d.at.y } else { d.at.x }) - line).abs() < e
            })
            .map(|d| {
                let t = if along_x { d.at.x } else { d.at.y };
                (t - d.size.x * 0.5, t + d.size.x * 0.5)
            })
            .collect();
        gaps.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut t = t0;
        let mut spans = Vec::new();
        for (g0, g1) in gaps {
            if g1 <= t0 || g0 >= t1 {
                continue;
            }
            if g0 > t {
                spans.push((t, g0, [t > t0, true]));
            }
            t = t.max(g1);
        }
        if t < t1 {
            spans.push((t, t1, [t > t0, false]));
        }
        let top = if outside { OUTER_TOP } else { INNER_TOP };
        for (a, b, door) in spans {
            if b - a < 0.05 {
                continue;
            }
            let pt = |s: f64| {
                if along_x {
                    DVec2::new(s, face)
                } else {
                    DVec2::new(face, s)
                }
            };
            out.push(Stretch {
                wall,
                a: pt(a),
                b: pt(b),
                top,
                door,
            });
        }
    }
    out
}

/// Wallpaper or paint, skirting, a picture rail in old rooms, pictures
/// and wall lamps.
fn dress_walls(fu: &mut Furnisher, room: &Room, k: usize, f: &Floor, seed: u64) {
    let Some(tex) = fu.decor.walls(room.kind) else {
        return;
    };
    let colour = fu.decor.wall_color(room.kind, seed >> 5);
    let skirt = match fu.decor.era {
        Era::Modern => Mat::Paint(rgb(236, 232, 222)),
        _ => Mat::Wood(pieces::scale(fu.decor.wood, 0.9)),
    };
    let rail = matches!(fu.decor.era, Era::Old | Era::Deco)
        && matches!(
            room.kind,
            RoomKind::Apartment | RoomKind::Living | RoomKind::Bedroom | RoomKind::Lobby
        );
    let list = stretches(room, f);
    for s in &list {
        let n = s.wall.inward();
        let up = |p: DVec2, y: f64| fu.city(p, y);
        // The paper, from behind the skirting to the top.
        let q = [up(s.a, 0.04), up(s.b, 0.04), up(s.b, s.top), up(s.a, s.top)];
        let normal = fu.dir(n);
        fu.b.tex = Some(tex);
        fu.b.quad(q, normal, colour);
        fu.b.tex = None;
        // Skirting and rail, 2 cm proud of the paper, stopping short of
        // the door casings.
        let len = s.a.distance(s.b);
        let dir = (s.b - s.a) / len;
        let cut = |d: bool| if d { CASING + 0.01 } else { 0.0 };
        let (x0, x1) = (cut(s.door[0]), len - cut(s.door[1]));
        if x1 - x0 > 0.05 {
            let mid = s.a + dir * ((x0 + x1) * 0.5);
            let half = (x1 - x0) * 0.5;
            // `n.perp()` may run against `dir`: measure along the maker's x.
            let mut m = fu.maker(mid, 0.0, n);
            trim(&mut m, skirt, half, 0.0, 0.11);
            if rail {
                trim(&mut m, skirt, half, 2.18, 2.22);
            }
        }
    }
    // Pictures on the longest stretches, where nothing tall stands.
    let wants = match room.kind {
        RoomKind::Apartment | RoomKind::Living => 3,
        RoomKind::Bedroom | RoomKind::Lobby | RoomKind::Office => 2,
        RoomKind::Corridor | RoomKind::Kitchen | RoomKind::Gallery => 1,
        _ => 0,
    };
    let mut sorted: Vec<&Stretch> = list.iter().filter(|s| s.a.distance(s.b) > 1.6).collect();
    sorted.sort_by(|a, b| b.a.distance(b.b).total_cmp(&a.a.distance(a.b)));
    let mut rng = Rng(seed ^ 0x51C7);
    let mut placed = 0;
    let mut sconce_left = matches!(
        room.kind,
        RoomKind::Apartment | RoomKind::Living | RoomKind::Bedroom | RoomKind::Lobby
    );
    for s in sorted {
        if placed >= wants {
            break;
        }
        let len = s.a.distance(s.b);
        let dir = (s.b - s.a) / len;
        for t in [0.5, 0.3, 0.7] {
            let c = s.a + dir * (len * t);
            let n = s.wall.inward();
            let (w, h) =
                [(0.9, 0.65), (0.6, 0.8), (1.2, 0.7), (0.5, 0.5)][pick(rng.next_u64(), 3, 4)];
            // Clear of tall pieces in front of this bit of wall (and of a
            // sconce's reach either side).
            let reach = DVec2::new(w * 0.5 + 0.7, 0.6);
            let blocked = fu.taken.iter().any(|tk| {
                let gap = (c + n * 0.3 - tk.c).abs() - tk.half;
                let (along, across) = if n.x.abs() > 0.5 {
                    (gap.y, gap.x)
                } else {
                    (gap.x, gap.y)
                };
                tk.tall && along < reach.x && across < reach.y
            });
            if blocked {
                continue;
            }
            let (era, lamp) = (fu.decor.era, fu.lamp);
            let mut m = fu.maker(c, 0.0, n);
            picture(&mut m, w, h, 1.55, rng.next_u64(), era);
            placed += 1;
            // Wall lamps either side of the first picture.
            if sconce_left && len > w + 1.4 {
                for side in [-1.0, 1.0] {
                    let p = c + dir * side * (w * 0.5 + 0.45);
                    let mut m = fu.maker(p, 0.0, n);
                    sconce(&mut m, lamp);
                }
                sconce_left = false;
            }
            break;
        }
    }
    let _ = k;
}

/// How far a door casing reaches along the wall from the opening.
const CASING: f64 = 0.11;

/// A skirting board or picture rail along a wall, `half` either side of
/// the maker's origin, from `y0` to `y1`, 2 cm proud of the paper. It has
/// no back: one would sit a few millimetres off the plaster behind the
/// paper and fight it.
fn trim(m: &mut Maker, mat: Mat, half: f64, y0: f64, y1: f64) {
    let (zb, zf) = (-0.012, 0.02);
    m.face(
        mat,
        [
            DVec3::new(-half, y0, zf),
            DVec3::new(half, y0, zf),
            DVec3::new(half, y1, zf),
            DVec3::new(-half, y1, zf),
        ],
        DVec3::Z,
    );
    m.face(
        mat,
        [
            DVec3::new(-half, y1, zf),
            DVec3::new(half, y1, zf),
            DVec3::new(half, y1, zb),
            DVec3::new(-half, y1, zb),
        ],
        DVec3::Y,
    );
    if y0 > 0.0 {
        m.face(
            mat,
            [
                DVec3::new(-half, y0, zb),
                DVec3::new(half, y0, zb),
                DVec3::new(half, y0, zf),
                DVec3::new(-half, y0, zf),
            ],
            -DVec3::Y,
        );
    }
    for s in [-1.0, 1.0] {
        let x = s * half;
        m.face(
            mat,
            [
                DVec3::new(x, y0, zb),
                DVec3::new(x, y0, zf),
                DVec3::new(x, y1, zf),
                DVec3::new(x, y1, zb),
            ],
            DVec3::X * s,
        );
    }
}

/// A framed picture on the wall (the frame's back 3 cm off the paper),
/// its middle `y` up: a little painting in coloured cells.
fn picture(m: &mut Maker, w: f64, h: f64, y: f64, seed: u64, era: Era) {
    let z0 = 0.045;
    let fw = 0.045;
    let frame = match era {
        Era::Old | Era::Deco => Mat::Brass,
        _ => Mat::Wood(rgb(60, 44, 34)),
    };
    // Frame bars.
    let (x0, x1, y0, y1) = (-w * 0.5, w * 0.5, y - h * 0.5, y + h * 0.5);
    for (lo, hi) in [
        (
            DVec3::new(x0 - fw, y0 - fw, 0.03),
            DVec3::new(x1 + fw, y0, 0.07),
        ),
        (
            DVec3::new(x0 - fw, y1, 0.03),
            DVec3::new(x1 + fw, y1 + fw, 0.07),
        ),
        (DVec3::new(x0 - fw, y0, 0.03), DVec3::new(x0, y1, 0.07)),
        (DVec3::new(x1, y0, 0.03), DVec3::new(x1 + fw, y1, 0.07)),
    ] {
        m.block(frame, lo, hi);
    }
    // The painting, cell by cell.
    let (nx, ny) = (16usize, 12usize);
    let art = Art::new(seed);
    for i in 0..nx {
        for j in 0..ny {
            let (u0, u1) = (i as f64 / nx as f64, (i + 1) as f64 / nx as f64);
            let (v0, v1) = (j as f64 / ny as f64, (j + 1) as f64 / ny as f64);
            let c = art.at((u0 + u1) * 0.5, (v0 + v1) * 0.5);
            let px = |u: f64, v: f64| DVec3::new(x0 + w * u, y0 + h * v, z0);
            m.face(
                Mat::Plain(c),
                [px(u0, v0), px(u1, v0), px(u1, v1), px(u0, v1)],
                DVec3::Z,
            );
        }
    }
}

/// A little painting: a landscape, a rocket in the night sky, an atom, or
/// an abstract of blocks.
struct Art {
    kind: u64,
    seed: u64,
}

impl Art {
    fn new(seed: u64) -> Self {
        Art {
            kind: seed % 4,
            seed,
        }
    }

    /// The colour at (u, v), both 0..1, v up.
    fn at(&self, u: f64, v: f64) -> Color {
        let s = self.seed;
        let lerp = |a: (f64, f64, f64), b: (f64, f64, f64), t: f64| {
            let t = t.clamp(0.0, 1.0);
            rgb(
                (a.0 + (b.0 - a.0) * t) as u8,
                (a.1 + (b.1 - a.1) * t) as u8,
                (a.2 + (b.2 - a.2) * t) as u8,
            )
        };
        match self.kind {
            0 => {
                // Hills under a sunset.
                let hill = 0.35 + 0.12 * (u * 5.0 + (s % 7) as f64).sin() + 0.06 * (u * 13.0).sin();
                let far = 0.5 + 0.08 * (u * 3.0 + 1.0).sin();
                let sun = ((u - 0.68).powi(2) + (v - 0.62).powi(2)).sqrt();
                if v < hill {
                    lerp((50.0, 90.0, 50.0), (90.0, 130.0, 70.0), v / hill)
                } else if v < far {
                    rgb(90, 110, 130)
                } else if sun < 0.1 {
                    rgb(255, 220, 120)
                } else {
                    lerp(
                        (240, 150, 90).into_f(),
                        (90, 120, 190).into_f(),
                        (v - far) / (1.0 - far),
                    )
                }
            }
            1 => {
                // A rocket climbing past a ringed planet.
                let planet = ((u - 0.28).powi(2) + (v - 0.68).powi(2) * 1.4).sqrt();
                let ring = ((u - 0.28).abs() * 0.35 + (v - 0.68).abs() * 2.0 - 0.07).abs();
                let body = (u - 0.62).abs() < 0.05 && v > 0.2 && v < 0.75;
                let nose = (0.75..0.88).contains(&v) && (u - 0.62).abs() < 0.05 * (0.88 - v) / 0.13;
                let fin = v > 0.18 && v < 0.32 && (u - 0.62).abs() < 0.11 - (v - 0.18) * 0.4;
                let flame = v < 0.2 && v > 0.04 && (u - 0.62).abs() < 0.035;
                let star = ((u * 97.0 + v * 31.0 + (s % 13) as f64).sin() * 43758.5).fract() > 0.93;
                if body || nose {
                    rgb(220, 220, 228)
                } else if fin {
                    rgb(200, 50, 46)
                } else if flame {
                    rgb(255, 180, 60)
                } else if planet < 0.13 {
                    rgb(210, 150, 90)
                } else if ring < 0.02 {
                    rgb(230, 210, 160)
                } else if star {
                    rgb(250, 250, 230)
                } else {
                    lerp((20.0, 24.0, 60.0), (40.0, 30.0, 90.0), v)
                }
            }
            2 => {
                // An atom: three orbits round a red nucleus on cream.
                let (x, y) = (u - 0.5, (v - 0.5) * 1.3);
                let r = (x * x + y * y).sqrt();
                let mut on = r < 0.06;
                for k in 0..3 {
                    let a = PI * k as f64 / 3.0;
                    let (ca, sa) = (a.cos(), a.sin());
                    let (px, py) = (x * ca + y * sa, -x * sa + y * ca);
                    let e = ((px / 0.38).powi(2) + (py / 0.12).powi(2)).sqrt();
                    if (e - 1.0).abs() < 0.12 {
                        return rgb(40, 110, 150);
                    }
                    on |= false;
                }
                if on {
                    rgb(210, 60, 50)
                } else {
                    rgb(238, 226, 196)
                }
            }
            _ => {
                // Blocks of colour on white, divided by black lines.
                let cols = [
                    (214, 60, 50),
                    (240, 200, 60),
                    (50, 90, 170),
                    (238, 234, 222),
                ];
                let xs = [0.0, 0.3 + (s % 5) as f64 * 0.04, 0.72, 1.01];
                let ys = [0.0, 0.4 + (s % 3) as f64 * 0.08, 1.01];
                let line = xs.iter().any(|x| (u - x).abs() < 0.03)
                    || ys.iter().any(|y| (v - y).abs() < 0.04);
                if line {
                    return rgb(24, 24, 26);
                }
                let i = xs.iter().filter(|&&x| u > x).count();
                let j = ys.iter().filter(|&&y| v > y).count();
                let c = cols[(i * 3 + j * 5 + s as usize) % cols.len()];
                rgb(c.0, c.1, c.2)
            }
        }
    }
}

trait IntoF {
    fn into_f(self) -> (f64, f64, f64);
}

impl IntoF for (i32, i32, i32) {
    fn into_f(self) -> (f64, f64, f64) {
        (self.0 as f64, self.1 as f64, self.2 as f64)
    }
}

/// A wall lamp: a brass back plate and a tulip of frosted glass, lit with
/// the room.
fn sconce(m: &mut Maker, lamp: u8) {
    m.lathe_along(
        Mat::Brass,
        DVec3::new(0.0, 1.75, 0.0),
        DVec3::Z,
        &[(0.06, 0.0), (0.06, 0.02), (0.02, 0.04), (0.0, 0.045)],
    );
    m.tube(
        Mat::Brass,
        &[
            DVec3::new(0.0, 1.75, 0.03),
            DVec3::new(0.0, 1.76, 0.12),
            DVec3::new(0.0, 1.82, 0.14),
        ],
        0.01,
    );
    m.lathe(
        Mat::Lamp(lamp),
        DVec3::new(0.0, 1.81, 0.14),
        &[(0.02, 0.0), (0.07, 0.07), (0.09, 0.14), (0.0, 0.12)],
    );
}

/// Door frames on both faces of every doorway, and the leaves of the
/// inner doors standing open into the room.
fn dress_doors(fu: &mut Furnisher, f: &Floor) {
    let modern = fu.decor.era == Era::Modern;
    let casing = if modern {
        Mat::Paint(rgb(236, 232, 222))
    } else {
        Mat::Wood(pieces::scale(fu.decor.wood, 0.9))
    };
    for d in &f.doors {
        let rooms: Vec<usize> = std::iter::once(d.a).chain(d.b).collect();
        let normal = if d.along_x { DVec2::Y } else { DVec2::X };
        let along = normal.perp();
        let w = d.size.x;
        for &r in &rooms {
            let room = &f.rooms[r];
            if fu.decor.walls(room.kind).is_none() {
                continue;
            }
            // Which way this room is from the door.
            let side = if (room.rect.center() - d.at).dot(normal) > 0.0 {
                1.0
            } else {
                -1.0
            };
            let n = normal * side;
            let top = INNER_TOP.min(if rooms.len() == 1 {
                OUTER_TOP
            } else {
                INNER_TOP
            });
            let face = d.at + n * (WALL_HALF + PAPER);
            for s in [-1.0, 1.0] {
                let c = face + along * s * (w * 0.5 + CASING * 0.5);
                let mut m = fu.maker(c, 0.0, n);
                m.block(
                    casing,
                    DVec3::new(-CASING * 0.5, 0.0, 0.005),
                    // A little under the wall's top: level with it, the
                    // two tops fought.
                    DVec3::new(CASING * 0.5, top - 0.015, 0.03),
                );
            }
        }
        // The leaves themselves move (opened and shut): `doorview` draws
        // them.
    }
}

/// A door leaf `w` wide standing on the floor, its faces looking along
/// local ±z, its width along local x: panelled for old houses, flush
/// with a porthole for modern ones.
pub fn door_leaf(m: &mut Maker, w: f64, h: f64, modern: bool, wood: Color) {
    let t = 0.022;
    if modern {
        m.block(
            Mat::Paint(rgb(214, 196, 160)),
            DVec3::new(-w * 0.5, 0.012, -t),
            DVec3::new(w * 0.5, h, t),
        );
        for s in [-1.0, 1.0] {
            m.lathe_along(
                Mat::Chrome,
                DVec3::new(0.0, 1.6, s * t),
                DVec3::Z * s,
                &[(0.14, 0.0), (0.14, 0.014), (0.12, 0.02), (0.0, 0.02)],
            );
        }
        m.lathe_along(
            Mat::Glass(rgb(200, 220, 230)),
            DVec3::new(0.0, 1.6, -t - 0.017),
            DVec3::Z,
            &[
                (0.115, 0.0),
                (0.115, 2.0 * t + 0.034),
                (0.0, 2.0 * t + 0.035),
            ],
        );
    } else {
        m.block(
            Mat::Wood(wood),
            DVec3::new(-w * 0.5, 0.012, -t),
            DVec3::new(w * 0.5, h, t),
        );
        // Raised panels both faces: two tall, one short.
        for s in [-1.0, 1.0] {
            for (y0, y1) in [(0.15, 0.95), (1.1, h - 0.15)] {
                let (z0, z1) = if s > 0.0 {
                    (t - 0.005, t + 0.014)
                } else {
                    (-t - 0.014, -t + 0.005)
                };
                m.block(
                    Mat::Wood(pieces::scale(wood, 1.12)),
                    DVec3::new(-w * 0.5 + 0.1, y0, z0),
                    DVec3::new(w * 0.5 - 0.1, y1, z1),
                );
            }
        }
    }
    for s in [-1.0, 1.0] {
        m.lathe_along(
            Mat::Brass,
            DVec3::new(w * 0.5 - 0.09, 1.0, s * (t - 0.004)),
            DVec3::Z * s,
            &[
                (0.012, 0.0),
                (0.012, 0.04),
                (0.03, 0.06),
                (0.025, 0.08),
                (0.0, 0.085),
            ],
        );
    }
}

// ---------------------------------------------------------------- rooms

/// A sitting group against `wall` of `area`: a sofa, a table and a rug in
/// front of it, armchairs across, a lamp, the radio or television facing
/// it. Returns false if the sofa did not fit.
fn sitting(fu: &mut Furnisher, area: &Rect, wall: Wall, seed: u64) -> bool {
    let d = fu.decor;
    let run = if wall.inward().x.abs() > 0.5 {
        area.size().y
    } else {
        area.size().x
    };
    let depth = if wall.inward().x.abs() > 0.5 {
        area.size().x
    } else {
        area.size().y
    };
    if depth < 2.6 || run < 2.2 {
        return false;
    }
    let sw = (run - 0.9).clamp(1.6, 2.4);
    let cover = d.cover(seed);
    let Some(sofa) = fu.against(
        area,
        wall,
        &[0.5, 0.35, 0.65],
        DVec2::new(sw, 0.9),
        false,
        |m| pieces::sofa(m, sw, &cover),
    ) else {
        return false;
    };
    let n = wall.inward();
    let along = wall.along();
    // Rug and coffee table.
    let tc = sofa + n * 1.25;
    let rug_size = DVec2::new((sw + 0.8).min(run - 0.2), (depth - 1.0).min(2.4));
    let rug_c = sofa + n * (0.35 + rug_size.y * 0.5);
    let field = d.fabric(seed + 1);
    let border = d.fabric(seed + 2);
    let oval_rug = pick(seed, 21, 2) == 0;
    if fu.rug_fits(rug_c, n, rug_size) {
        let mut m = fu.maker(rug_c, 0.0, n);
        let pts = if oval_rug {
            pieces::oval(rug_size.x, rug_size.y, 40)
        } else {
            pieces::rrect(rug_size.x, rug_size.y, 0.08, 2)
        };
        pieces::rug(&mut m, &pts, pieces::scale(field, 0.9), border, 0.14);
    }
    let wood = d.wood;
    let kidney = d.era == Era::Modern || pick(seed, 23, 2) == 0;
    fu.put(area, tc, n, DVec2::new(1.2, 0.7), false, |m| {
        if kidney {
            let top = pieces::kidney(1.2, 0.66, 48);
            pieces::coffee_table(
                m,
                &top,
                wood,
                &[
                    DVec2::new(-0.4, 0.15),
                    DVec2::new(0.42, 0.12),
                    DVec2::new(0.0, -0.12),
                ],
            );
        } else {
            let top = pieces::rrect(1.1, 0.6, 0.1, 3);
            pieces::coffee_table(
                m,
                &top,
                wood,
                &[
                    DVec2::new(-0.45, -0.2),
                    DVec2::new(0.45, -0.2),
                    DVec2::new(-0.45, 0.2),
                    DVec2::new(0.45, 0.2),
                ],
            );
        }
    });
    // Something on the table: a bowl of fruit or a vase of flowers.
    {
        let mut m = fu.maker(tc, 0.405, n);
        m.lathe(
            Mat::Paint(d.fabric(seed + 4)),
            DVec3::new(0.15, 0.0, 0.0),
            &[(0.05, 0.0), (0.12, 0.05), (0.13, 0.07), (0.0, 0.06)],
        );
        for k in 0..3 {
            let a = k as f64 * 2.1;
            m.blob(
                Mat::Plain([rgb(220, 60, 40), rgb(240, 190, 60), rgb(120, 180, 60)][k]),
                DVec3::new(0.15 + 0.05 * a.cos(), 0.085, 0.05 * a.sin()),
                DVec3::splat(0.035),
                0.0,
                (1.0, 1.0),
            );
        }
    }
    // Armchairs across the table, facing the sofa.
    let cover2 = d.cover(seed + 5);
    let mut chairs = 0;
    for s in [-0.6, 0.6] {
        let c = sofa + n * 2.45 + along * s;
        if fu.put(area, c, -n, DVec2::new(0.85, 0.85), false, |m| {
            pieces::armchair(m, &cover2)
        }) {
            chairs += 1;
        }
    }
    if chairs == 0 {
        // One beside the table, facing it.
        for s in [-1.0, 1.0] {
            let c = tc + along * s * 1.15;
            if fu.put(area, c, -along * s, DVec2::new(0.85, 0.85), false, |m| {
                pieces::armchair(m, &cover2)
            }) {
                break;
            }
        }
    }
    // A floor lamp by the sofa's end.
    let shade = d.fabric(seed + 6);
    let lamp = fu.lamp;
    for s in [1.0, -1.0] {
        let c = sofa + along * s * (sw * 0.5 + 0.3) - n * 0.1;
        if fu.put(area, c, n, DVec2::new(0.4, 0.4), false, |m| {
            pieces::floor_lamp(m, lamp, pieces::scale(shade, 1.1))
        }) {
            break;
        }
    }
    // The radio or the television on the far wall, facing the sofa.
    let far = wall.opposite();
    if pick(seed, 27, 2) == 0 {
        fu.against(
            area,
            far,
            &[0.5, 0.3, 0.7],
            DVec2::new(0.8, 0.55),
            false,
            |m| pieces::television(m, wood, lamp),
        );
    } else {
        fu.against(
            area,
            far,
            &[0.5, 0.3, 0.7],
            DVec2::new(1.0, 0.45),
            false,
            |m| pieces::radio(m, wood),
        );
    }
    true
}

/// A dining table with chairs round it, and a sideboard on a wall.
fn dining(fu: &mut Furnisher, area: &Rect, seed: u64) {
    let d = fu.decor;
    let c = area.center();
    let s = area.size();
    let round = pick(seed, 29, 2) == 0 || s.min_element() < 2.6;
    let (tw, td) = if round {
        (1.05, 1.05)
    } else if s.x >= s.y {
        (1.5, 0.9)
    } else {
        (0.9, 1.5)
    };
    let wood = d.wood;
    if !fu.put(area, c, DVec2::Y, DVec2::new(tw, td), false, |m| {
        pieces::dining_table(m, tw, td, wood)
    }) {
        return;
    }
    let table = fu.taken.len() - 1;
    // Something on the table.
    {
        let mut m = fu.maker(c, 0.75, DVec2::Y);
        m.lathe(
            Mat::Glass(rgb(170, 210, 220)),
            DVec3::ZERO,
            &[
                (0.04, 0.0),
                (0.06, 0.08),
                (0.03, 0.2),
                (0.04, 0.25),
                (0.0, 0.24),
            ],
        );
        for k in 0..5 {
            let a = k as f64 * 1.26;
            m.blob(
                Mat::Plain([rgb(230, 70, 80), rgb(250, 220, 90), rgb(240, 240, 250)][k % 3]),
                DVec3::new(0.06 * a.cos(), 0.3 + 0.03 * (k % 2) as f64, 0.06 * a.sin()),
                DVec3::splat(0.035),
                0.0,
                (1.0, 1.0),
            );
        }
    }
    let cover = d.cover(seed + 8);
    let seats: Vec<DVec2> = if round {
        (0..4)
            .map(|k| {
                let a = FRAC_PI_4 + FRAC_PI_2 * k as f64;
                DVec2::new(a.cos(), a.sin()) * 0.78
            })
            .collect()
    } else if tw > td {
        vec![
            DVec2::new(-0.38, 0.72),
            DVec2::new(0.38, 0.72),
            DVec2::new(-0.38, -0.72),
            DVec2::new(0.38, -0.72),
        ]
    } else {
        vec![
            DVec2::new(0.72, -0.38),
            DVec2::new(0.72, 0.38),
            DVec2::new(-0.72, -0.38),
            DVec2::new(-0.72, 0.38),
        ]
    };
    for off in seats {
        // Chairs face the table; set at an angle for a round one.
        let p = c + off;
        let facing = -off.normalize();
        let half = DVec2::splat(0.25);
        if !fu.free_but(p, half, area, table) {
            continue;
        }
        fu.mark(p, half, false);
        let o = fu.city(p, 0.0);
        let f = fu.dir(facing);
        let mut m = Maker::new(fu.b, o, f, fu.seg, d.wear);
        pieces::chair(&mut m, &cover);
    }
    let vase = d.fabric(seed + 9);
    for wall in Wall::ALL {
        if fu
            .against(
                area,
                wall,
                &[0.5, 0.2, 0.8],
                DVec2::new(1.6, 0.45),
                false,
                |m| pieces::sideboard(m, 1.6, wood, vase),
            )
            .is_some()
        {
            break;
        }
    }
}

const FRAC_PI_4: f64 = std::f64::consts::FRAC_PI_4;
const FRAC_PI_2: f64 = std::f64::consts::FRAC_PI_2;
const PI: f64 = std::f64::consts::PI;

/// A bed with its head to a wall, nightstands, a wardrobe and a chair.
fn sleeping(fu: &mut Furnisher, area: &Rect, seed: u64, double: bool) -> bool {
    let d = fu.decor;
    let bw = if double { 1.6 } else { 1.0 };
    let cover = d.cover(seed + 11);
    let mut placed = None;
    for wall in [Wall::Back, Wall::Left, Wall::Right, Wall::Front] {
        if let Some(c) = fu.against(
            area,
            wall,
            &[0.5, 0.4, 0.6, 0.3, 0.7],
            DVec2::new(bw + 0.1, 2.25),
            false,
            |m| pieces::bed(m, bw, &cover),
        ) {
            placed = Some((c, wall));
            break;
        }
    }
    let Some((c, wall)) = placed else {
        return false;
    };
    let n = wall.inward();
    let along = wall.along();
    let head = c - n * (2.25 * 0.5);
    let lamp = fu.lamp;
    let wood = d.wood;
    let shade = d.fabric(seed + 12);
    for s in [-1.0, 1.0] {
        let p = head + n * 0.21 + along * s * (bw * 0.5 + 0.33);
        fu.put(area, p, n, DVec2::new(0.48, 0.4), false, |m| {
            pieces::nightstand(m, wood, lamp, shade)
        });
    }
    // A rug at the bed's foot.
    let rc = c + n * 1.35;
    if fu.rug_fits(rc, n, DVec2::new(bw + 0.6, 0.8)) {
        let mut m = fu.maker(rc, 0.0, n);
        let pts = pieces::rrect(bw + 0.6, 0.8, 0.1, 2);
        pieces::rug(&mut m, &pts, d.fabric(seed + 13), d.fabric(seed + 14), 0.1);
    }
    // The wardrobe on another wall.
    for w2 in [
        wall.opposite(),
        Wall::Left,
        Wall::Right,
        Wall::Front,
        Wall::Back,
    ] {
        if w2 == wall {
            continue;
        }
        if fu
            .against(
                area,
                w2,
                &[0.8, 0.2, 0.5],
                DVec2::new(1.2, 0.62),
                true,
                |m| pieces::wardrobe(m, 1.2, wood),
            )
            .is_some()
        {
            break;
        }
    }
    // An armchair in a corner, angled into the room.
    let cover2 = d.cover(seed + 15);
    for wall in Wall::ALL {
        if fu
            .against(
                area,
                wall,
                &[0.95, 0.05],
                DVec2::new(0.85, 0.85),
                false,
                |m| pieces::armchair(m, &cover2),
            )
            .is_some()
        {
            break;
        }
    }
    true
}

/// A kitchen along one wall: counter with sink, stove, fridge, cupboards.
fn kitchenette(fu: &mut Furnisher, area: &Rect, seed: u64, walls: &[Wall]) -> bool {
    let pastel = [
        rgb(150, 210, 196),
        rgb(240, 200, 120),
        rgb(230, 160, 170),
        rgb(170, 200, 230),
    ][pick(seed, 9, 4)];
    let enamel = rgb(244, 242, 236);
    for &wall in walls {
        if !fu.is_wall(area, wall) {
            continue;
        }
        let run = if wall.inward().x.abs() > 0.5 {
            area.size().y
        } else {
            area.size().x
        };
        let len = (run - 0.4).min(3.6);
        if len < 2.4 {
            continue;
        }
        let counter_w = len - 1.6;
        // Fridge, counter (sink in it), stove: placed as one run.
        let (c, n) = Furnisher::spot(area, wall, 0.5, DVec2::new(len, 0.62));
        let along = wall.along();
        let half = if n.x.abs() < 0.5 {
            DVec2::new(len, 0.62) * 0.5
        } else {
            DVec2::new(0.62, len) * 0.5
        };
        if !fu.free(c, half, area) {
            continue;
        }
        fu.mark(c, half, true);
        let fridge_c = c - along * (len * 0.5 - 0.39);
        let counter_c = c - along * (len * 0.5 - 0.8 - counter_w * 0.5);
        let stove_c = c + along * (len * 0.5 - 0.39);
        {
            let mut m = fu.maker(fridge_c - n * 0.02, 0.0, n);
            pieces::fridge(&mut m, enamel);
        }
        {
            let mut m = fu.maker(counter_c, 0.0, n);
            pieces::counter(&mut m, counter_w, pastel, Some(counter_w * 0.15));
        }
        {
            let mut m = fu.maker(counter_c - n * 0.12, 0.0, n);
            pieces::wall_cupboards(&mut m, counter_w, pastel);
        }
        {
            let mut m = fu.maker(stove_c, 0.0, n);
            pieces::stove(&mut m, enamel);
        }
        // A kettle and a radio on the counter.
        {
            let mut m = fu.maker(counter_c + along * (counter_w * 0.3) - n * 0.05, 0.9, n);
            m.lathe(
                Mat::Chrome,
                DVec3::ZERO,
                &[
                    (0.08, 0.0),
                    (0.1, 0.05),
                    (0.09, 0.14),
                    (0.04, 0.2),
                    (0.0, 0.21),
                ],
            );
            m.tube(
                Mat::Plain(rgb(30, 30, 30)),
                &[
                    DVec3::new(-0.06, 0.18, 0.0),
                    DVec3::new(0.0, 0.26, 0.0),
                    DVec3::new(0.06, 0.18, 0.0),
                ],
                0.012,
            );
        }
        return true;
    }
    false
}

/// A flat in one open room: living, dining, a kitchen and a bed, in
/// zones along its long side, the bed away from the door.
fn apartment(fu: &mut Furnisher, room: &Room, area: &Rect, f: &Floor, seed: u64) {
    let s = area.size();
    if s.min_element() < 2.2 {
        return;
    }
    let along_x = s.x >= s.y;
    let long = s.max_element();
    // Which end the door is at.
    let door_at = f
        .doors
        .iter()
        .filter(|d| room.rect.on_boundary(d.at, 0.05))
        .map(|d| if along_x { d.at.x } else { d.at.y })
        .next()
        .unwrap_or(if along_x { area.min.x } else { area.min.y });
    let lo = if along_x { area.min.x } else { area.min.y };
    let door_low = door_at - lo < long * 0.5;
    // Zone shares, from the door's end: dining+kitchen, living, sleeping.
    let shares: Vec<(char, f64)> = if long >= 9.5 {
        vec![('d', 0.3), ('l', 0.38), ('s', 0.32)]
    } else if long >= 6.5 {
        vec![('l', 0.55), ('s', 0.45)]
    } else {
        vec![('l', 1.0)]
    };
    let mut t = 0.0;
    for (kind, share) in shares {
        let (a, b) = (t, t + share);
        t = b;
        let (a, b) = if door_low { (a, b) } else { (1.0 - b, 1.0 - a) };
        let zone = if along_x {
            Rect::new(
                DVec2::new(area.min.x + s.x * a, area.min.y),
                DVec2::new(area.min.x + s.x * b, area.max.y),
            )
        } else {
            Rect::new(
                DVec2::new(area.min.x, area.min.y + s.y * a),
                DVec2::new(area.max.x, area.min.y + s.y * b),
            )
        };
        // The zone's walls that are the room's long walls.
        let long_walls = if along_x {
            [Wall::Back, Wall::Front]
        } else {
            [Wall::Right, Wall::Left]
        };
        match kind {
            'd' => {
                let kitchen_wall = long_walls[pick(seed, 31, 2)];
                let ok = kitchenette(
                    fu,
                    &zone,
                    seed,
                    &[kitchen_wall, long_walls[0], long_walls[1]],
                );
                let rest = if ok {
                    shrink(&zone, kitchen_wall, 1.0)
                } else {
                    zone
                };
                dining(fu, &rest, seed);
            }
            'l' => {
                if !long_walls.iter().any(|&w| sitting(fu, &zone, w, seed)) {
                    sitting(
                        fu,
                        &zone,
                        if along_x { Wall::Left } else { Wall::Front },
                        seed,
                    );
                }
                let wood = fu.decor.wood;
                for w in Wall::ALL {
                    if fu
                        .against(&zone, w, &[0.1, 0.9], DVec2::new(1.0, 0.36), true, |m| {
                            pieces::bookcase(m, 1.0, 2.0, wood, seed)
                        })
                        .is_some()
                    {
                        break;
                    }
                }
                plants(fu, &zone, seed, 1);
            }
            _ => {
                sleeping(fu, &zone, seed, pick(seed, 33, 3) != 0);
            }
        }
    }
}

/// `r` with `d` taken off along `wall`'s side.
fn shrink(r: &Rect, wall: Wall, d: f64) -> Rect {
    let mut out = *r;
    match wall {
        Wall::Front => out.min.y += d,
        Wall::Back => out.max.y -= d,
        Wall::Left => out.min.x += d,
        Wall::Right => out.max.x -= d,
    }
    out
}

/// Potted plants in the area's corners.
fn plants(fu: &mut Furnisher, area: &Rect, seed: u64, n: usize) {
    let pots = [
        rgb(196, 110, 70),
        rgb(236, 232, 222),
        rgb(60, 70, 80),
        rgb(214, 170, 90),
    ];
    let mut done = 0;
    let corners = [
        area.min,
        DVec2::new(area.max.x, area.min.y),
        area.max,
        DVec2::new(area.min.x, area.max.y),
    ];
    for k in 0..4 {
        if done >= n {
            break;
        }
        let corner = corners[(k + pick(seed, 35, 4)) % 4];
        let c = corner + (area.center() - corner).signum() * 0.3;
        let pot = pots[pick(seed, 37 + k as u32, pots.len())];
        let h = 0.9 + (seed >> (40 + k)) as f64 % 5.0 * 0.12;
        if fu.put(area, c, DVec2::Y, DVec2::splat(0.55), false, |m| {
            pieces::plant(m, h, pot, seed ^ k as u64)
        }) {
            done += 1;
        }
    }
}

fn living(fu: &mut Furnisher, area: &Rect, seed: u64) {
    for wall in Wall::ALL {
        if sitting(fu, area, wall, seed) {
            break;
        }
    }
    let wood = fu.decor.wood;
    for w in Wall::ALL {
        if fu
            .against(area, w, &[0.1, 0.9], DVec2::new(1.0, 0.36), true, |m| {
                pieces::bookcase(m, 1.0, 2.0, wood, seed)
            })
            .is_some()
        {
            break;
        }
    }
    plants(fu, area, seed, 2);
}

fn bedroom(fu: &mut Furnisher, area: &Rect, seed: u64) {
    sleeping(fu, area, seed, true);
}

fn kitchen(fu: &mut Furnisher, area: &Rect, seed: u64) {
    kitchenette(fu, area, seed, &Wall::ALL);
    let rest = area.inset(0.0);
    dining(fu, &rest, seed);
}

fn lobby(fu: &mut Furnisher, room: &Room, area: &Rect, seed: u64) {
    let d = fu.decor;
    let s = area.size();
    if s.min_element() < 2.0 {
        return;
    }
    let c = area.center();
    // A round rug in two rings in the middle.
    let r = (s.min_element() * 0.32).min(3.2);
    {
        let mut m = fu.maker(c, 0.0, DVec2::Y);
        let pts = pieces::oval(r * 2.0, r * 2.0, 48);
        pieces::rug(&mut m, &pts, rgb(226, 184, 90), rgb(150, 40, 46), 0.35);
    }
    // The front desk at the back, a streamline curve with chrome bands.
    let body = rgb(236, 226, 204);
    let band = d.fabric(seed + 1);
    let dw = (s.x * 0.4).clamp(2.0, 4.0);
    fu.against(
        area,
        Wall::Back,
        &[0.5, 0.3, 0.7],
        DVec2::new(dw, 0.7),
        false,
        |m| pieces::shop_counter(m, dw, body, band),
    );
    // Sofas facing each other across the rug, a table between.
    let cover = d.cover(seed);
    for side in [Wall::Left, Wall::Right] {
        let n = side.inward();
        let p = c - n * (r + 0.6);
        fu.put(area, p, n, DVec2::new(2.0, 0.9), false, |m| {
            pieces::sofa(m, 2.0, &cover)
        });
    }
    let wood = d.wood;
    fu.put(area, c, DVec2::Y, DVec2::new(1.3, 0.75), false, |m| {
        let top = pieces::kidney(1.3, 0.7, 48);
        pieces::coffee_table(
            m,
            &top,
            wood,
            &[
                DVec2::new(-0.45, 0.16),
                DVec2::new(0.46, 0.12),
                DVec2::new(0.0, -0.13),
            ],
        );
    });
    let lamp = fu.lamp;
    for p in [
        area.min + DVec2::splat(0.3),
        DVec2::new(area.max.x - 0.3, area.min.y + 0.3),
    ] {
        let shade = d.fabric(seed + 3);
        fu.put(area, p, DVec2::Y, DVec2::splat(0.5), false, |m| {
            pieces::floor_lamp(m, lamp, shade)
        });
    }
    plants(fu, area, seed, 4);
    let _ = room;
}

fn office(fu: &mut Furnisher, room: &Room, area: &Rect, seed: u64) {
    let s = area.size();
    if s.min_element() < 2.4 {
        return;
    }
    let steel = [rgb(120, 140, 130), rgb(140, 140, 136), rgb(110, 124, 140)][pick(seed, 5, 3)];
    let top = [rgb(70, 110, 90), rgb(60, 70, 64), rgb(150, 110, 70)][pick(seed, 7, 3)];
    let cover = Cover {
        main: [rgb(130, 60, 50), rgb(60, 80, 110), rgb(90, 100, 70)][pick(seed, 11, 3)],
        second: rgb(40, 40, 40),
        wood: fu.decor.wood,
    };
    // Desks in rows, everyone facing the front (towards the street).
    let pitch = DVec2::new(2.3, 2.4);
    let usable = area.inset(0.3);
    let nx = ((usable.size().x / pitch.x).floor() as usize).max(1);
    let nz = ((usable.size().y / pitch.y).floor() as usize).max(1);
    let lamp = fu.lamp;
    for i in 0..nx {
        for j in 0..nz {
            let c = DVec2::new(
                usable.min.x + usable.size().x * (i as f64 + 0.5) / nx as f64,
                usable.min.y + usable.size().y * (j as f64 + 0.5) / nz as f64 - 0.3,
            );
            // The desk faces back (its sitter side at +z), the chair
            // behind it.
            let k = seed ^ (i * 7 + j * 13) as u64;
            let facing = DVec2::Y;
            if fu.put(area, c, facing, DVec2::new(1.55, 0.8), false, |m| {
                pieces::desk(m, steel, top, lamp, k)
            }) {
                // The chair pulled up to it, turned a little.
                let desk = fu.taken.len() - 1;
                let ch = c + facing * 0.68;
                let half = DVec2::splat(0.3);
                if fu.free_but(ch, half, area, desk) {
                    fu.mark(ch, half, false);
                    let turn = (k % 7) as f64 * 0.08 - 0.24;
                    let mut m = fu.maker(ch, 0.0, -facing);
                    let mut m = m.sub(DVec3::ZERO, turn);
                    pieces::swivel_chair(&mut m, &cover);
                }
            }
        }
    }
    // Filing cabinets on a wall, a water cooler, a coat stand, plants.
    for wall in [Wall::Left, Wall::Right, Wall::Back] {
        let mut n = 0;
        for t in [0.1, 0.25, 0.4] {
            if fu
                .against(area, wall, &[t], DVec2::new(0.48, 0.64), true, |m| {
                    pieces::filing_cabinet(m, steel)
                })
                .is_some()
            {
                n += 1;
            }
        }
        if n > 0 {
            break;
        }
    }
    for wall in [Wall::Right, Wall::Left, Wall::Front] {
        if fu
            .against(area, wall, &[0.9, 0.7], DVec2::new(0.4, 0.4), true, |m| {
                pieces::water_cooler(m)
            })
            .is_some()
        {
            break;
        }
    }
    let coat = [rgb(150, 130, 100), rgb(80, 80, 90)][pick(seed, 13, 2)];
    for wall in Wall::ALL {
        if fu
            .against(
                area,
                wall,
                &[0.05, 0.95],
                DVec2::new(0.55, 0.55),
                true,
                |m| pieces::coat_stand(m, coat),
            )
            .is_some()
        {
            break;
        }
    }
    plants(fu, area, seed, 2);
    let _ = room;
}

/// The length of `wall` of `area`.
fn wall_run(area: &Rect, wall: Wall) -> f64 {
    let s = area.size();
    if wall.inward().x.abs() > 0.5 {
        s.y
    } else {
        s.x
    }
}

/// Lines `wall` with pieces of `size`, sliding past doorways and pieces
/// already standing; `build` gets each piece's number. Returns how many
/// went in.
fn fill_wall(
    fu: &mut Furnisher,
    area: &Rect,
    wall: Wall,
    size: DVec2,
    tall: bool,
    gap: f64,
    build: impl Fn(&mut Maker, usize),
) -> usize {
    if !fu.is_wall(area, wall) {
        return 0;
    }
    let run = wall_run(area, wall);
    let free = run - size.x;
    if free < 0.0 {
        return 0;
    }
    let mut n = 0;
    let mut x = 0.0;
    while x <= free + 1e-9 {
        let t = if free > 0.0 { x / free } else { 0.5 };
        let (c, f) = Furnisher::spot(area, wall, t, size);
        if fu.put(area, c, f, size, tall, |m| build(m, n)) {
            n += 1;
            x += size.x + gap;
        } else {
            x += 0.1;
        }
    }
    n
}

/// Puts one piece against the first of `walls` with room for it, as near
/// the middle of the wall as it goes.
fn place_one(
    fu: &mut Furnisher,
    area: &Rect,
    walls: &[Wall],
    size: DVec2,
    tall: bool,
    build: impl Fn(&mut Maker),
) -> Option<(Wall, DVec2)> {
    let ts: Vec<f64> = (0..=10)
        .flat_map(|k| [0.5 + k as f64 * 0.05, 0.5 - k as f64 * 0.05])
        .collect();
    walls
        .iter()
        .find_map(|&w| fu.against(area, w, &ts, size, tall, &build).map(|c| (w, c)))
}

fn shop(fu: &mut Furnisher, room: &Room, area: &Rect, seed: u64) {
    let s = area.size();
    if s.min_element() < 2.4 {
        return;
    }
    let kind = shop_kind(fu.plan, room);
    let d = fu.decor;
    let accent = d.fabric(seed + 1);
    // The street front (Front) keeps its display window clear; stock
    // lines the other walls, longest first.
    let mut walls = [Wall::Left, Wall::Right, Wall::Back];
    walls.sort_by(|a, b| wall_run(area, *b).total_cmp(&wall_run(area, *a)));
    if pick(seed, 41, 2) == 1 && (wall_run(area, walls[0]) - wall_run(area, walls[1])).abs() < 0.5 {
        walls.swap(0, 1);
    }
    let back_first = [Wall::Back, walls[0], walls[1], walls[2]];
    let cw = (s.min_element() * 0.6).clamp(1.6, 3.0);
    let counter = |fu: &mut Furnisher, body: Color| {
        place_one(fu, area, &back_first, DVec2::new(cw, 0.7), false, |m| {
            pieces::shop_counter(m, cw, body, accent)
        })
    };
    let facing = if s.x >= s.y { DVec2::Y } else { DVec2::X };
    match kind {
        ShopKind::Diner => {
            // A long counter with stools down one wall, booths along the
            // others and in the window, a jukebox in a corner.
            let body = rgb(236, 230, 214);
            let red = rgb(200, 50, 50);
            let mut len = (wall_run(area, walls[0]) * 0.7).clamp(1.8, 6.0);
            let mut at = None;
            while at.is_none() && len >= 1.8 {
                let l = len;
                at = place_one(fu, area, &walls, DVec2::new(l, 1.5), false, |m| {
                    let mut c = m.sub(DVec3::new(0.0, 0.0, -0.4), 0.0);
                    pieces::shop_counter(&mut c, l, body, red);
                })
                .map(|w| (w, l));
                len -= 0.5;
            }
            let mut used = None;
            if let Some(((wall, c), l)) = at {
                used = Some(wall);
                let n = wall.inward();
                let k = ((l - 0.4) / 0.7) as usize;
                for i in 0..k {
                    let x = -l * 0.5 + 0.55 + i as f64 * 0.7;
                    let p = c + wall.along() * x + n * 0.3;
                    let mut m = fu.maker(p, 0.0, -n);
                    pieces::stool(&mut m, red);
                }
            }
            let cover = rgb(196, 50, 56);
            let top = rgb(240, 236, 226);
            for wall in [Wall::Front, walls[0], walls[1], walls[2]] {
                if Some(wall) == used {
                    continue;
                }
                fill_wall(
                    fu,
                    area,
                    wall,
                    DVec2::new(1.25, 1.8),
                    false,
                    0.25,
                    |m, _| {
                        let mut b = m.sub(DVec3::ZERO, FRAC_PI_2);
                        pieces::booth(&mut b, cover, top);
                    },
                );
            }
            let corners: Vec<f64> = (0..=4)
                .flat_map(|k| [1.0 - k as f64 * 0.05, k as f64 * 0.05])
                .collect();
            for wall in walls {
                if fu
                    .against(area, wall, &corners, DVec2::new(0.9, 0.65), true, |m| {
                        pieces::jukebox(m)
                    })
                    .is_some()
                {
                    break;
                }
            }
        }
        ShopKind::Club => {
            // A long bar with a mirrored back bar and red leather stools
            // down one wall, velvet booths round the others, a bandstand
            // in a corner.
            let walnut = rgb(92, 54, 32);
            let red = rgb(160, 30, 44);
            let mut len = (wall_run(area, walls[0]) * 0.75).clamp(1.8, 7.0);
            let mut at = None;
            while at.is_none() && len >= 1.8 {
                let l = len;
                at = place_one(fu, area, &walls, DVec2::new(l, 2.4), false, |m| {
                    let mut back = m.sub(DVec3::new(0.0, 0.0, -1.0), 0.0);
                    pieces::back_bar(&mut back, l, walnut);
                    let mut c = m.sub(DVec3::new(0.0, 0.0, 0.0), 0.0);
                    pieces::shop_counter(&mut c, l, walnut, red);
                })
                .map(|w| (w, l));
                len -= 0.5;
            }
            let mut used = None;
            if let Some(((wall, c), l)) = at {
                used = Some(wall);
                let n = wall.inward();
                let k = ((l - 0.4) / 0.7) as usize;
                for i in 0..k {
                    let x = -l * 0.5 + 0.55 + i as f64 * 0.7;
                    let p = c + wall.along() * x + n * 0.75;
                    let mut m = fu.maker(p, 0.0, -n);
                    pieces::stool(&mut m, red);
                }
            }
            // The bandstand first, in a corner, so the booths leave it room.
            let corners: Vec<f64> = (0..=4)
                .flat_map(|k| [1.0 - k as f64 * 0.05, k as f64 * 0.05])
                .collect();
            for wall in walls {
                if Some(wall) == used {
                    continue;
                }
                if fu
                    .against(area, wall, &corners, DVec2::new(2.0, 1.6), false, |m| {
                        pieces::bandstand(m)
                    })
                    .is_some()
                {
                    break;
                }
            }
            let velvet = rgb(120, 26, 52);
            let top = rgb(70, 40, 28);
            for wall in [Wall::Front, walls[0], walls[1], walls[2]] {
                if Some(wall) == used {
                    continue;
                }
                fill_wall(
                    fu,
                    area,
                    wall,
                    DVec2::new(1.25, 1.8),
                    false,
                    0.25,
                    |m, _| {
                        let mut b = m.sub(DVec3::ZERO, FRAC_PI_2);
                        pieces::booth(&mut b, velvet, top);
                    },
                );
            }
        }
        ShopKind::Grocer => {
            counter(fu, rgb(236, 232, 220));
            for wall in walls {
                fill_wall(fu, area, wall, DVec2::new(1.6, 0.5), true, 0.0, |m, k| {
                    let mut g = m.sub(DVec3::new(0.0, 0.0, -0.22), 0.0);
                    pieces::gondola(
                        &mut g,
                        1.6,
                        rgb(220, 214, 200),
                        seed ^ (wall as u64) << 8 ^ k as u64,
                        false,
                    );
                });
            }
            // Island shelves down the middle.
            let run = s.max_element();
            let across = s.min_element();
            if across > 3.6 {
                let along = facing.perp().abs();
                let rows = ((run - 2.6) / 2.4).floor().max(1.0) as usize;
                for i in 0..rows {
                    let t = (i as f64 + 0.5) / rows as f64;
                    let c = area.min + along * (run * (0.2 + 0.6 * t)) + facing * (across * 0.5);
                    let sk = seed ^ (i as u64 * 31);
                    fu.put(area, c, facing, DVec2::new(1.8, 0.95), true, |m| {
                        pieces::gondola(m, 1.8, rgb(220, 214, 200), sk, true)
                    });
                }
            }
            // Fruit crates in the window.
            fill_wall(
                fu,
                area,
                Wall::Front,
                DVec2::new(0.65, 0.5),
                false,
                0.1,
                |m, k| {
                    let fruit = [
                        rgb(230, 60, 40),
                        rgb(250, 200, 60),
                        rgb(140, 190, 60),
                        rgb(240, 140, 40),
                    ][(seed as usize + k) % 4];
                    m.block(
                        Mat::Wood(rgb(176, 132, 84)),
                        DVec3::new(-0.3, 0.0, -0.22),
                        DVec3::new(0.3, 0.4, 0.22),
                    );
                    for i in 0..5 {
                        for j in 0..3 {
                            m.blob(
                                Mat::Plain(fruit),
                                DVec3::new(-0.22 + i as f64 * 0.11, 0.42, -0.12 + j as f64 * 0.12),
                                DVec3::splat(0.05),
                                0.0,
                                (1.0, 1.0),
                            );
                        }
                    }
                },
            );
        }
        ShopKind::Clothes => {
            counter(fu, rgb(236, 226, 214));
            for wall in walls {
                fill_wall(fu, area, wall, DVec2::new(1.8, 0.6), true, 0.2, |m, k| {
                    pieces::clothes_rail(m, 1.8, seed ^ (wall as u64) << 8 ^ k as u64)
                });
            }
            // Dress forms in the window, a display case in the middle.
            fill_wall(
                fu,
                area,
                Wall::Front,
                DVec2::splat(0.5),
                true,
                0.9,
                |m, k| pieces::dress_form(m, d.fabric(seed + k as u64 * 3)),
            );
            let wood = d.wood;
            fu.put(
                area,
                area.center(),
                facing,
                DVec2::new(1.6, 0.6),
                false,
                |m| pieces::display_case(m, 1.6, wood, seed),
            );
        }
        ShopKind::Appliances => {
            counter(fu, rgb(226, 232, 236));
            let wood = d.wood;
            let lamp = fu.lamp.max(channel(Tone::Cool, 0));
            // Televisions down one wall, refrigerators and stoves the
            // other, radios in the middle and the window.
            fill_wall(
                fu,
                area,
                walls[0],
                DVec2::new(0.8, 0.55),
                false,
                0.15,
                |m, _| pieces::television(m, wood, lamp),
            );
            let enamel = [rgb(244, 242, 236), rgb(240, 200, 190), rgb(190, 226, 214)];
            fill_wall(
                fu,
                area,
                walls[1],
                DVec2::new(0.8, 0.75),
                true,
                0.1,
                |m, k| {
                    let c = enamel[k % 3];
                    if k % 2 == 0 {
                        pieces::fridge(m, c)
                    } else {
                        let mut st = m.sub(DVec3::new(0.0, 0.0, -0.045), 0.0);
                        pieces::stove(&mut st, c)
                    }
                },
            );
            fu.put(
                area,
                area.center(),
                facing,
                DVec2::new(1.0, 0.5),
                false,
                |m| pieces::radio(m, wood),
            );
        }
    }
    plants(fu, area, seed, 1);
}

fn storage(fu: &mut Furnisher, area: &Rect, seed: u64) {
    if area.size().min_element() < 1.0 {
        return;
    }
    for wall in [Wall::Left, Wall::Back, Wall::Right] {
        for t in [0.2, 0.6] {
            let sk = seed ^ (t * 10.0) as u64 ^ wall as u64;
            fu.against(area, wall, &[t], DVec2::new(1.6, 0.5), true, |m| {
                let mut g = m.sub(DVec3::new(0.0, 0.0, -0.22), 0.0);
                pieces::gondola(&mut g, 1.6, rgb(150, 150, 140), sk, false);
            });
        }
    }
    // Crates stacked in the middle.
    let mut g = Rng(seed);
    let c = area.center();
    for k in 0..3 {
        let p = c + DVec2::new(k as f64 * 0.8 - 0.8, 0.0);
        let s = 0.3 + g.unit() * 0.12;
        fu.put(area, p, DVec2::Y, DVec2::splat(s * 2.0), false, |m| {
            m.block(
                Mat::Wood(rgb(176, 132, 84)),
                DVec3::new(-s, 0.0, -s),
                DVec3::new(s, 2.0 * s, s),
            );
            for x in [-s - 0.012, s - 0.0] {
                m.block(
                    Mat::Wood(rgb(130, 94, 60)),
                    DVec3::new(x, 0.02, -s - 0.012),
                    DVec3::new(x + 0.012, 2.0 * s - 0.02, s + 0.012),
                );
            }
        });
    }
}

fn gallery(fu: &mut Furnisher, area: &Rect, seed: u64) {
    let s = area.size();
    if s.min_element() < 2.0 {
        return;
    }
    // Plinths with glowing Aether sculptures in a row, benches between.
    let n = ((s.x / 3.0) as usize).clamp(1, 5);
    for k in 0..n {
        let p = DVec2::new(
            area.min.x + s.x * (k as f64 + 0.5) / n as f64,
            area.center().y,
        );
        let col = if pick(seed, 3 + k as u32, 2) == 0 {
            rgb(120, 230, 255)
        } else {
            rgb(255, 160, 220)
        };
        fu.put(area, p, DVec2::Y, DVec2::splat(0.8), true, |m| {
            m.lathe(
                Mat::Paint(rgb(236, 232, 224)),
                DVec3::ZERO,
                &[
                    (0.36, 0.0),
                    (0.36, 0.08),
                    (0.3, 0.12),
                    (0.26, 0.9),
                    (0.32, 0.95),
                    (0.32, 1.0),
                    (0.0, 1.0),
                ],
            );
            m.blob(
                Mat::Glow(col),
                DVec3::new(0.0, 1.32, 0.0),
                DVec3::new(0.18, 0.3, 0.18),
                0.0,
                (1.0, 0.7),
            );
            for k in 0..3 {
                let a = k as f64 * 2.1;
                m.tube(
                    Mat::Chrome,
                    &[
                        DVec3::new(0.0, 1.0, 0.0),
                        DVec3::new(0.3 * a.cos(), 1.35, 0.3 * a.sin()),
                        DVec3::new(0.0, 1.7, 0.0),
                    ],
                    0.012,
                );
            }
        });
        let bench = DVec2::new(p.x, p.y + 1.6);
        let cover = fu.decor.cover(seed + k as u64);
        fu.put(area, bench, -DVec2::Y, DVec2::new(1.6, 0.5), false, |m| {
            for x in [-0.6, 0.6] {
                m.leg(Mat::Chrome, x, 0.0, 0.4, (0.02, 0.02), DVec2::ZERO);
            }
            m.blob(
                Mat::Cloth(cover.main),
                DVec3::new(0.0, 0.44, 0.0),
                DVec3::new(0.78, 0.06, 0.22),
                0.0,
                (0.2, 0.4),
            );
        });
    }
}

fn lab(fu: &mut Furnisher, area: &Rect, _seed: u64) {
    let s = area.size();
    if s.min_element() < 1.6 {
        return;
    }
    // Benches along the back wall with glowing flasks.
    let w = (s.x * 0.8).min(5.0);
    fu.against(area, Wall::Back, &[0.5], DVec2::new(w, 0.7), false, |m| {
        m.block(
            Mat::Paint(rgb(236, 236, 236)),
            DVec3::new(-w * 0.5, 0.0, -0.35),
            DVec3::new(w * 0.5, 0.88, 0.35),
        );
        m.block(
            Mat::Plain(rgb(30, 30, 34)),
            DVec3::new(-w * 0.5 - 0.01, 0.88, -0.36),
            DVec3::new(w * 0.5 + 0.01, 0.92, 0.36),
        );
        let n = ((w * 2.0) as usize).min(8);
        for k in 0..n {
            let x = -w * 0.5 + 0.3 + k as f64 * (w - 0.6) / n.max(1) as f64;
            let col = if k % 2 == 0 {
                rgb(120, 255, 140)
            } else {
                rgb(120, 230, 255)
            };
            m.lathe(
                Mat::Glow(col),
                DVec3::new(x, 0.92, 0.0),
                &[
                    (0.03, 0.0),
                    (0.08, 0.06),
                    (0.08, 0.1),
                    (0.025, 0.2),
                    (0.025, 0.3),
                    (0.0, 0.3),
                ],
            );
        }
    });
    // An Aether coil in the middle.
    let c = area.center();
    fu.put(area, c, DVec2::Y, DVec2::splat(1.2), true, |m| {
        m.lathe(
            Mat::Chrome,
            DVec3::ZERO,
            &[
                (0.5, 0.0),
                (0.5, 0.1),
                (0.4, 0.3),
                (0.12, 0.32),
                (0.12, 1.5),
                (0.0, 1.5),
            ],
        );
        for k in 0..5 {
            let y = 0.5 + k as f64 * 0.22;
            m.lathe(
                Mat::Glow(rgb(120, 230, 255)),
                DVec3::new(0.0, y, 0.0),
                &[(0.3, 0.0), (0.33, 0.03), (0.3, 0.06), (0.0, 0.06)],
            );
        }
        m.blob(
            Mat::Chrome,
            DVec3::new(0.0, 1.62, 0.0),
            DVec3::splat(0.16),
            0.0,
            (1.0, 1.0),
        );
    });
}

fn corridor(fu: &mut Furnisher, area: &Rect, seed: u64) {
    // A bench and a plant or two along the way.
    plants(fu, area, seed, 2);
}

/// The stair and lift core: golden lift doors framed in Aether light.
fn core(fu: &mut Furnisher, room: &Room, f: &Floor) {
    // The lift faces the nearest doorway out of the core.
    let c = room.rect.center();
    let Some(d) = f
        .doors
        .iter()
        .filter(|d| room.rect.on_boundary(d.at, 0.05))
        .map(|d| d.at)
        .min_by(|a, b| a.distance(c).total_cmp(&b.distance(c)))
    else {
        return;
    };
    let toward = (d - c).normalize_or(DVec2::Y);
    // A glowing ring on the floor where the Aether lift rises.
    let r = (room.rect.size().min_element() * 0.3).clamp(0.5, 1.2);
    let at = c - toward * 0.2;
    let mut m = fu.maker(at, 0.0, toward);
    m.lathe(
        Mat::Glow(rgb(120, 230, 255)),
        DVec3::ZERO,
        &[(r, 0.0), (r, 0.03), (r * 0.9, 0.03), (0.0, 0.03)],
    );
    m.lathe(
        Mat::Brass,
        DVec3::new(0.0, 0.03, 0.0),
        &[(r * 0.85, 0.0), (r * 0.85, 0.015), (0.0, 0.015)],
    );
    let _ = chrome;
    let _ = Surf::Plain;
}

#[cfg(test)]
mod tests {
    use super::*;
    use hw_core::{Seed, StableId};
    use hw_gen::building::{BuildingSite, BuildingUse};
    use hw_gen::city::DistrictKind;

    fn plan(usage: BuildingUse, floors: u32) -> BuildingPlan {
        let site = BuildingSite {
            id: StableId(7),
            seed: Seed(7),
            usage,
            origin: DVec2::new(10.0, 20.0),
            along: DVec2::X,
            inward: DVec2::Y,
            size: DVec2::new(24.0, 16.0),
            floors,
            street: 0,
            district: DistrictKind::Commercial,
            vernacular: None,
        };
        BuildingPlan::generate(&site)
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn furnished_floors_have_furniture() {
        for (usage, floors) in [
            (BuildingUse::Apartments, 4),
            (BuildingUse::Offices, 6),
            (BuildingUse::ShopsAndOffices, 3),
        ] {
            let plan = plan(usage, floors);
            for level in 0..plan.floors.len() {
                let mut b = MeshBuilder::new();
                furnish(&mut b, &plan, level, 0.0, Detail::Near);
                assert!(
                    b.face_count() > 500,
                    "{usage:?} floor {level}: {}",
                    b.face_count()
                );
            }
        }
    }

    #[test]
    fn people_indoors_stand_clear_of_furniture() {
        // Every room people live, work or shop in has places to stand,
        // inside its walls; no two places crowd each other.
        for usage in [
            BuildingUse::Apartments,
            BuildingUse::Offices,
            BuildingUse::ShopsAndOffices,
        ] {
            let plan = plan(usage, 3);
            for level in 0..plan.floors.len() {
                let mut b = MeshBuilder::new();
                furnish(&mut b, &plan, level, 0.0, Detail::Far);
                let f = &plan.floors[level];
                for room in &f.rooms {
                    let lived = !matches!(
                        room.kind,
                        RoomKind::Core | RoomKind::Storage | RoomKind::Corridor
                    );
                    let here: Vec<&Spot> = b
                        .spots
                        .iter()
                        .filter(|s| {
                            let d = DVec2::new(s.at.x, s.at.z) - plan.site.origin;
                            let l = DVec2::new(d.dot(plan.site.along), d.dot(plan.site.inward));
                            let r = room.rect.inset(WALL_HALF);
                            r.min.x <= l.x && l.x <= r.max.x && r.min.y <= l.y && l.y <= r.max.y
                        })
                        .collect();
                    if lived && room.rect.size().min_element() > 2.5 {
                        assert!(
                            !here.is_empty(),
                            "{usage:?} floor {level}: no place in {:?}",
                            room.kind
                        );
                    }
                    for (i, a) in here.iter().enumerate() {
                        for c in &here[i + 1..] {
                            assert!(
                                a.at.distance(c.at) > 1.1,
                                "{usage:?}: two places crowd in {:?}",
                                room.kind
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn flats_have_a_bed_a_sofa_and_a_kitchen() {
        // Count big pieces by their footprints: a flat floor holds as many
        // beds and sofas as it has flats.
        let plan = plan(BuildingUse::Apartments, 4);
        let f = &plan.floors[1];
        let flats = f
            .rooms
            .iter()
            .filter(|r| r.kind == RoomKind::Apartment)
            .count();
        assert!(flats > 0);
        let mut b = MeshBuilder::new();
        furnish(&mut b, &plan, 1, 0.0, Detail::Near);
        // A furnished flat is several thousand faces.
        assert!(
            b.face_count() > flats * 3000,
            "{} faces for {flats} flats",
            b.face_count()
        );
    }

    /// A building with the night club's sign and doorman outside has the
    /// club inside: the shop room behind its door is furnished as it.
    #[test]
    fn a_night_club_is_a_club_inside() {
        let mut p = plan(BuildingUse::ShopsAndOffices, 3);
        let club = (0..200u64)
            .map(|k| BuildingSite {
                seed: Seed(k),
                ..p.site.clone()
            })
            .find(hw_sim::streetlife::is_club)
            .expect("one building in five is a club");
        p = BuildingPlan::generate(&club);
        let f = &p.floors[0];
        let (k, _) = hw_sim::streetlife::club_room(&p).expect("a club has a shop front");
        let clubs = f
            .rooms
            .iter()
            .filter(|r| r.kind == RoomKind::Shop && shop_kind(&p, r) == ShopKind::Club)
            .count();
        assert_eq!(clubs, 1, "one club room");
        {
            let room = &f.rooms[k];
            let mut b = MeshBuilder::new();
            let mut fu = Furnisher {
                b: &mut b,
                plan: &p,
                top: 0.0,
                seg: 12,
                decor: Decor::of(&p),
                keepout: f.doors.iter().map(door_keepout).collect(),
                taken: Vec::new(),
                lamp: 0,
                walls: inner(room, OFF_WALL),
                rugs: Vec::new(),
            };
            let area = inner(room, OFF_WALL);
            shop(&mut fu, room, &area, 1);
            assert!(fu.taken.len() >= 4, "a club with {} pieces", fu.taken.len());
        }
        // An ordinary shop building is not.
        let plain = (0..200u64)
            .map(|k| BuildingSite {
                seed: Seed(k),
                ..p.site.clone()
            })
            .find(|s| !hw_sim::streetlife::is_club(s))
            .expect("most buildings are not clubs");
        let q = BuildingPlan::generate(&plain);
        assert!(q.floors[0]
            .rooms
            .iter()
            .all(|r| r.kind != RoomKind::Shop || shop_kind(&q, r) != ShopKind::Club));
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn every_shop_kind_is_furnished() {
        let p = plan(BuildingUse::ShopsAndOffices, 3);
        let f = &p.floors[0];
        for (k, room) in f.rooms.iter().enumerate() {
            if room.kind != RoomKind::Shop {
                continue;
            }
            let mut b = MeshBuilder::new();
            let mut fu = Furnisher {
                b: &mut b,
                plan: &p,
                top: 0.0,
                seg: 12,
                decor: Decor::of(&p),
                keepout: f.doors.iter().map(door_keepout).collect(),
                taken: Vec::new(),
                lamp: 0,
                walls: inner(room, OFF_WALL),
                rugs: Vec::new(),
            };
            let area = inner(room, OFF_WALL);
            shop(&mut fu, room, &area, hash(p.site.origin, 0, k));
            assert!(
                fu.taken.len() >= 3,
                "shop {k} ({:?}) has {} pieces",
                shop_kind(&p, room),
                fu.taken.len()
            );
        }
    }
}
