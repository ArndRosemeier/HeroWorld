//! Buildings, planned from the inside out.
//!
//! A [`BuildingSite`] is what the city plan decides: where the building
//! stands, how big it is, what it is for and how tall. [`BuildingPlan::generate`]
//! turns a site into floors and rooms first, and only then derives the facade
//! from them: a window wherever a room that wants daylight meets an outside
//! wall, a door wherever an entrance is. Outside and inside always match.
//!
//! Building coordinates: x runs along the street front (0 at the front-left
//! corner, seen from the street), z runs inwards (0 at the front wall), y up
//! from the ground floor. Everything is more spacious than real life, so a
//! third-person camera and a flying hero fit.

use crate::city::DistrictKind;
use crate::geom::Rect;
use crate::layout::{office, solid, Layout};
use crate::style::{Crown, Look, Roof, Vernacular};
use glam::{DVec2, DVec3};
use hw_core::{Seed, StableId};

/// Ground floor, floor to floor (m).
pub const GROUND_FLOOR_HEIGHT: f64 = 5.5;
/// Upper floors, floor to floor (m).
pub const FLOOR_HEIGHT: f64 = 4.2;
/// Halls (works, pavilions), floor to floor (m).
pub const HALL_HEIGHT: f64 = 9.0;
/// Corridor width (m).
pub const CORRIDOR_WIDTH: f64 = 3.2;
/// Stair and lift core, along the front and in depth (m).
pub const CORE_SIZE: DVec2 = DVec2::new(8.0, 6.5);
/// Shallowest room strip next to a corridor (m).
pub const MIN_STRIP_DEPTH: f64 = 6.5;
/// Narrowest room (m).
pub const MIN_ROOM_WIDTH: f64 = 4.0;
/// Door width and height (m).
pub const DOOR_SIZE: DVec2 = DVec2::new(1.8, 2.8);
/// Main entrance width and height (m).
pub const ENTRANCE_SIZE: DVec2 = DVec2::new(3.6, 3.6);
/// Facade bay: one window per this much wall (m).
pub const WINDOW_BAY: f64 = 3.2;
/// Width of the entrance hall beside a narrow building's shop (m).
pub const SIDE_HALL: f64 = 4.0;

/// What a building is for. It decides the room programme.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuildingUse {
    /// Offices above a lobby (civic towers).
    Offices,
    /// Shops on the ground floor, offices above.
    ShopsAndOffices,
    /// Flats above a lobby.
    Apartments,
    /// A family house (googie bungalow): one or two floors, no corridor core.
    House,
    /// A workshop hall, offices above if taller.
    Works,
    /// A fair pavilion: one great hall.
    Pavilion,
    /// The mad scientist's labs.
    Lab,
    /// Run-down flats in the dystopia.
    Tenement,
}

impl BuildingUse {
    /// Does the ground floor hold one open hall?
    fn hall_ground(self) -> bool {
        matches!(self, Self::Works | Self::Pavilion)
    }

    pub fn ground_height(self) -> f64 {
        if self.hall_ground() {
            HALL_HEIGHT
        } else {
            GROUND_FLOOR_HEIGHT
        }
    }

    /// The room kind and target width along the corridor for an upper floor.
    fn upper_room(self) -> (RoomKind, f64) {
        match self {
            Self::Offices | Self::ShopsAndOffices | Self::Works => (RoomKind::Office, 7.5),
            Self::Apartments => (RoomKind::Apartment, 11.0),
            Self::Tenement => (RoomKind::Apartment, 8.0),
            Self::House => (RoomKind::Bedroom, 5.0),
            Self::Pavilion => (RoomKind::Gallery, 12.0),
            Self::Lab => (RoomKind::Lab, 9.0),
        }
    }

    /// Room kind and width for ground-floor rooms on the street side.
    fn ground_front_room(self) -> (RoomKind, f64) {
        match self {
            Self::ShopsAndOffices => (RoomKind::Shop, 10.0),
            Self::House => (RoomKind::Living, 7.0),
            Self::Lab => (RoomKind::Lab, 9.0),
            other => other.upper_room(),
        }
    }

    /// Room kind and width for ground-floor rooms at the back.
    fn ground_back_room(self) -> (RoomKind, f64) {
        match self {
            Self::ShopsAndOffices => (RoomKind::Storage, 8.0),
            Self::House => (RoomKind::Kitchen, 5.0),
            other => other.ground_front_room(),
        }
    }
}

/// What a room is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RoomKind {
    Lobby,
    Corridor,
    /// Stairs and lift; the same shaft on every floor.
    Core,
    Office,
    Apartment,
    Living,
    Kitchen,
    Bedroom,
    Shop,
    Storage,
    Hall,
    Gallery,
    Lab,
}

impl RoomKind {
    /// Rooms that get windows on every outside wall they touch.
    pub fn wants_daylight(self) -> bool {
        !matches!(self, Self::Core | Self::Storage)
    }
}

/// A room on one floor: an axis-aligned rectangle in building coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct Room {
    pub id: StableId,
    pub kind: RoomKind,
    pub rect: Rect,
}

/// A doorway between two rooms of a floor, or to the outside.
#[derive(Clone, Debug, PartialEq)]
pub struct Door {
    /// Room index on the floor.
    pub a: usize,
    /// The other room, or None for a door to the outside.
    pub b: Option<usize>,
    /// Centre of the doorway on the wall line (x, z).
    pub at: DVec2,
    /// True if the wall it sits in runs along x.
    pub along_x: bool,
    /// Width and height.
    pub size: DVec2,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Floor {
    pub level: u32,
    /// Height of the floor slab's top above the building's ground.
    pub elevation: f64,
    /// Floor to floor height.
    pub height: f64,
    /// Outline of this floor (upper floors may step back).
    pub outline: Rect,
    /// Rooms; together they tile the outline exactly.
    pub rooms: Vec<Room>,
    pub doors: Vec<Door>,
}

/// Which outside wall of a floor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    /// z = outline.min.y, facing the street.
    Front,
    /// x = outline.max.x
    Right,
    /// z = outline.max.y
    Back,
    /// x = outline.min.x
    Left,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OpeningKind {
    Window,
    Door,
    Entrance,
}

/// A hole in an outside wall, derived from the rooms behind it.
#[derive(Clone, Debug, PartialEq)]
pub struct Opening {
    pub floor: u32,
    pub side: Side,
    pub kind: OpeningKind,
    /// The room behind it (index on its floor).
    pub room: usize,
    /// Centre along the wall, in building x (front/back) or z (left/right).
    pub along: f64,
    pub width: f64,
    /// Bottom of the opening above the floor's elevation.
    pub sill: f64,
    pub height: f64,
}

/// Where and what the city wants built. Lives in the city plan; cheap.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildingSite {
    pub id: StableId,
    pub seed: Seed,
    pub usage: BuildingUse,
    /// Front-left corner on the ground, in city-local (x, z).
    pub origin: DVec2,
    /// Unit vector along the front, in city-local (x, z).
    pub along: DVec2,
    /// Unit vector from the front inwards, in city-local (x, z).
    pub inward: DVec2,
    /// Footprint: width along the front, depth inwards.
    pub size: DVec2,
    pub floors: u32,
    /// Index of the street the front faces.
    pub street: u32,
    /// The district it stands in (it decides the style).
    pub district: DistrictKind,
    /// Set on a building planned at game scale by [`crate::layout`] (halls,
    /// galleries, open stairs and lifts): the height of its roof. `floors`
    /// then counts its tall storeys.
    pub halls: Option<f64>,
    /// Builds in its region's old ways (towns); `None` for the
    /// golden-age styles alone.
    pub vernacular: Option<Vernacular>,
}

/// The deepest the windows of a building of halls sit in its walls (m).
pub const HALL_REVEAL: f64 = 0.075;

impl BuildingSite {
    /// The building's look (cheap: no plan needed).
    pub fn look(&self) -> Look {
        let mut look = match self.vernacular {
            Some(v) => Look::vernacular(self.seed, v, self.district, self.usage, self.floors),
            None => Look::choose(self.seed, self.district, self.usage, self.floors),
        };
        // A taxi pad needs a roof wide enough to land on.
        look.taxi_pad &= self.size.min_element() >= 15.0;
        if self.halls.is_some() {
            // The halls' plaster is drawn behind the window panes, and the
            // rooms are planned up to the walls' inner faces: a deep
            // reveal would bring the plaster into the rooms.
            look.reveal = look.reveal.min(HALL_REVEAL);
        }
        look
    }

    /// Building (x, y, z) → city-local (x, y, z).
    pub fn to_city(&self, p: DVec3) -> DVec3 {
        let g = self.origin + self.along * p.x + self.inward * p.z;
        DVec3::new(g.x, p.y, g.y)
    }

    /// Rotation about local +y (radians) that turns building axes into
    /// city-local axes: building +x → `along`, building +z → `inward`.
    pub fn yaw(&self) -> f64 {
        // Rotating +x by yaw about +y gives (cos, 0, -sin).
        hw_core::det::atan2(-self.along.y, self.along.x)
    }

    /// The four ground corners in city-local (x, z).
    pub fn corners(&self) -> [DVec2; 4] {
        let (w, d) = (self.along * self.size.x, self.inward * self.size.y);
        [
            self.origin,
            self.origin + w,
            self.origin + w + d,
            self.origin + d,
        ]
    }

    /// Total height to the roof.
    pub fn height(&self) -> f64 {
        if let Some(h) = self.halls {
            return h;
        }
        self.usage.ground_height() + FLOOR_HEIGHT * self.floors.saturating_sub(1) as f64
    }

    /// The highest point of the building as drawn, above its ground floor:
    /// the walls' [`Self::height`] and whatever stands on the roof (a
    /// pitched roof and its chimneys, a dome, a tower's crown and spire, a
    /// water tank, the works chimney). An upper bound from the look alone,
    /// no plan needed; it follows the client's roofs and crowns
    /// (`buildingmesh::roof`, `crown`): keep the two in step. What an
    /// airship has to clear ([`crate::urban::airspace`]).
    pub fn top(&self) -> f64 {
        let look = self.look();
        let w = self.size.min_element();
        let roof = match look.roof {
            Roof::Flat => {
                // Three shrinking tiers (4.2 + 3.5 + 2.8 m and trims), then
                // what tops them.
                let tiers = 11.4;
                let crown = match look.crown {
                    Crown::None => 0.0,
                    Crown::Stepped => tiers,
                    Crown::Fins => tiers + 12.0,
                    Crown::Spire => tiers + 32.0 + 0.5,
                    Crown::Mast => 3.0 + 26.0 + 0.6,
                    // The dome, its lantern and finial, or the Doctor's coil.
                    Crown::Dome => 3.3 + w * 0.32 + crate::urban::menace::COIL_LOAD + 4.0,
                };
                // Rooftop gear, a water tank or a taxi pad's beacons.
                crown.max(look.parapet).max(7.0)
            }
            // The pitch is capped at 14 m; chimneys stand over the ridge.
            Roof::Gable | Roof::Hip => 14.0 + 1.9,
            Roof::Mansard => 2.6 + 2.0,
            Roof::Dome => 1.6 + w * 0.42 + 0.5,
            Roof::Swoop => 3.2,
            // The works chimney.
            Roof::Sawtooth => 17.0,
        };
        // A corner tower's drum stands 2.6 m over the roof deck.
        self.height() + roof.max(3.0)
    }
}

/// One tier of a building's mass: floors that share an outline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tier {
    /// The floors' outline, in building (x, z).
    pub outline: Rect,
    /// Its floors, as [`Floor::level`]s: `first..end`.
    pub first: u32,
    pub end: u32,
    /// Bottom and top above the building's ground.
    pub bottom: f64,
    pub top: f64,
}

impl BuildingSite {
    /// Makes this an office building of halls with `storeys` tall storeys
    /// (its plot must be at least [`office::MIN_SIZE`]).
    pub fn into_halls(mut self, storeys: u32) -> Self {
        self.floors = storeys.max(2);
        self.halls =
            Some(office::plan_town(self.seed.child("layout"), self.size, self.floors).height());
        self
    }

    /// The floor plan of a building of halls ([`Self::halls`]), in building
    /// (x, z): the same for everyone who asks.
    pub fn layout(&self) -> Option<Layout> {
        self.halls
            .map(|_| office::plan_town(self.seed.child("layout"), self.size, self.floors))
    }

    /// The building's mass as tiers of floors, upper ones stepped back as
    /// in its plan (same outlines and floor heights as
    /// [`BuildingPlan::generate`], at a small part of the cost: no rooms).
    /// For drawing it from afar.
    pub fn massing(&self) -> Vec<Tier> {
        let full = Rect::from_xz(0.0, 0.0, self.size.x, self.size.y);
        if let Some(top) = self.halls {
            return vec![Tier {
                outline: full,
                first: 0,
                end: self.floors,
                bottom: 0.0,
                top,
            }];
        }
        let mut out: Vec<Tier> = Vec::new();
        let mut elevation = 0.0;
        for (level, outline) in Planner::new(self).outlines(full).into_iter().enumerate() {
            let level = level as u32;
            let height = if level == 0 {
                self.usage.ground_height()
            } else {
                FLOOR_HEIGHT
            };
            match out.last_mut() {
                Some(t) if t.outline == outline => {
                    t.end = level + 1;
                    t.top = elevation + height;
                }
                _ => out.push(Tier {
                    outline,
                    first: level,
                    end: level + 1,
                    bottom: elevation,
                    top: elevation + height,
                }),
            }
            elevation += height;
        }
        out
    }
}

/// The full plan of one building: floors, rooms, doors and the facade.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildingPlan {
    pub site: BuildingSite,
    /// The stair and lift shaft, the same on every floor (None for houses
    /// and single-floor buildings).
    pub core: Option<Rect>,
    pub floors: Vec<Floor>,
    pub openings: Vec<Opening>,
    /// The floor plan of a building of halls: its walls, stairs, lifts and
    /// furniture come from this, and `floors` is its outline level by
    /// level (for the facade and for whoever asks what room a point is in).
    pub layout: Option<Layout>,
    /// Style, materials, roof and ornament.
    pub look: Look,
}

impl BuildingPlan {
    pub fn generate(site: &BuildingSite) -> BuildingPlan {
        Planner::new(site).plan()
    }

    /// The solid parts of a building of halls, in building coordinates:
    /// its layout's floors, walls, stairs and shafts, the outside walls cut
    /// where the facade has its windows and doors.
    pub fn solids(&self) -> Option<Vec<solid::Piece>> {
        self.shell_parts(0.0)
    }

    /// [`Self::solids`] with every facade opening `narrower` by that much
    /// on each side: what the game draws behind the facade, so the
    /// plaster's edge round a window is not in the plane of the facade's
    /// own reveal.
    pub fn shell_parts(&self, narrower: f64) -> Option<Vec<solid::Piece>> {
        let l = self.layout.as_ref()?;
        let holes: Vec<solid::ShellHole> = self
            .openings
            .iter()
            .map(|o| {
                let f = &self.floors[o.floor as usize];
                let (along_x, w) = match o.side {
                    Side::Front => (true, f.outline.min.y),
                    Side::Back => (true, f.outline.max.y),
                    Side::Left => (false, f.outline.min.x),
                    Side::Right => (false, f.outline.max.x),
                };
                let v0 = f.elevation + o.sill;
                solid::ShellHole {
                    along_x,
                    w,
                    u: (
                        o.along - o.width * 0.5 + narrower,
                        o.along + o.width * 0.5 - narrower,
                    ),
                    v: (v0 + narrower, v0 + o.height - narrower),
                    glass: o.kind == OpeningKind::Window,
                }
            })
            .collect();
        Some(solid::pieces_in_shell(l, &holes))
    }

    /// The furniture of a building of halls, in building coordinates
    /// (solid, and drawn where it is solid).
    pub fn furniture(&self) -> Vec<crate::layout::furnish::Item> {
        self.layout
            .as_ref()
            .map(crate::layout::furnish::items)
            .unwrap_or_default()
    }

    pub fn height(&self) -> f64 {
        self.floors
            .last()
            .map(|f| f.elevation + f.height)
            .unwrap_or(0.0)
    }
}

struct Planner<'a> {
    site: &'a BuildingSite,
    look: Look,
}

/// How a floor is organised around its corridor.
#[derive(Clone, Copy)]
struct Bands {
    /// Rooms between the front wall and the corridor (z range).
    front: (f64, f64),
    corridor: (f64, f64),
    /// Rooms between the corridor and the back wall; None if single-loaded.
    back: Option<(f64, f64)>,
}

impl<'a> Planner<'a> {
    fn new(site: &'a BuildingSite) -> Self {
        Self {
            site,
            look: site.look(),
        }
    }

    /// Bay lines along x for an outline (rooms and windows follow them).
    fn x_lines(&self, o: &Rect) -> Vec<f64> {
        bay_lines(o.min.x, o.max.x, self.look.bay)
    }

    /// A building of halls: one [`Floor`] per storey of its layout, every
    /// space that stands or rises through that storey a room of it, and
    /// the way in from the street its only door (inside, the layout's
    /// openings are wide and have no leaves).
    fn hall_plan(&self, l: Layout) -> BuildingPlan {
        let s = self.site;
        let full = Rect::from_xz(0.0, 0.0, s.size.x, s.size.y);
        let mut ys = l.levels();
        ys.push(l.height());
        let mut floors = Vec::new();
        for (level, w) in ys.windows(2).enumerate() {
            let (e, top) = (w[0], w[1]);
            let seed = s.seed.child("floor").child(level as u32);
            // Space index -> room index on this floor.
            let mut index = vec![None; l.spaces.len()];
            let mut rooms = Vec::new();
            for (i, sp) in l.spaces.iter().enumerate() {
                if sp.within.is_none() && sp.y <= e + 1e-3 && sp.top() > e + 1e-3 {
                    use crate::layout::SpaceKind as K;
                    let kind = match sp.kind {
                        K::Vestibule | K::Lobby => RoomKind::Lobby,
                        K::Atrium | K::Canyon => RoomKind::Hall,
                        K::Gallery | K::Arcade | K::Concourse | K::Bridge | K::Landing => {
                            RoomKind::Corridor
                        }
                        K::Hall | K::Boardroom => RoomKind::Office,
                        K::Shop => RoomKind::Shop,
                        K::StairHall | K::Shaft => RoomKind::Core,
                        K::Service => RoomKind::Storage,
                    };
                    index[i] = Some(rooms.len());
                    rooms.push(room(seed, rooms.len() as u32, kind, sp.rect));
                }
            }
            let doors = l
                .links
                .iter()
                .filter(|k| k.b.is_none() && (k.y - e).abs() < 1e-3)
                .filter_map(|k| {
                    let a = index[l.spaces[k.a].within.unwrap_or(k.a)]?;
                    Some(Door {
                        a,
                        b: None,
                        at: k.at,
                        along_x: k.along_x,
                        size: DVec2::new(k.width, crate::layout::solid::ENTRANCE_H),
                    })
                })
                .collect();
            floors.push(Floor {
                level: level as u32,
                elevation: e,
                height: top - e,
                outline: full,
                rooms,
                doors,
            });
        }
        let openings = facade(&floors, &self.look, true);
        BuildingPlan {
            site: s.clone(),
            core: None,
            floors,
            openings,
            layout: Some(l),
            look: self.look.clone(),
        }
    }

    fn plan(&self) -> BuildingPlan {
        let s = self.site;
        if let Some(l) = s.layout() {
            return self.hall_plan(l);
        }
        let full = Rect::from_xz(0.0, 0.0, s.size.x, s.size.y);
        let outlines = self.outlines(full);
        let double_loaded = s.size.y >= 2.0 * MIN_STRIP_DEPTH + CORRIDOR_WIDTH;
        let bands_of = |o: &Rect| -> Bands {
            if double_loaded {
                let mid = (o.min.y + o.max.y) * 0.5;
                let c = (mid - CORRIDOR_WIDTH * 0.5, mid + CORRIDOR_WIDTH * 0.5);
                Bands {
                    front: (o.min.y, c.0),
                    corridor: c,
                    back: Some((c.1, o.max.y)),
                }
            } else {
                let c = (o.max.y - CORRIDOR_WIDTH, o.max.y);
                Bands {
                    front: (o.min.y, c.0),
                    corridor: c,
                    back: None,
                }
            }
        };
        let needs_core = s.floors > 1 && s.usage != BuildingUse::House;
        // The shaft sits behind the corridor (or in front of it when
        // single-loaded), centred along the front, and is identical on every floor.
        let core = needs_core.then(|| {
            let b = bands_of(&outlines[0]);
            let x0 = s.size.x * 0.5 - CORE_SIZE.x * 0.5;
            match b.back {
                Some(_) => Rect::from_xz(
                    x0,
                    b.corridor.1,
                    x0 + CORE_SIZE.x,
                    b.corridor.1 + CORE_SIZE.y,
                ),
                None => Rect::from_xz(
                    x0,
                    b.corridor.0 - CORE_SIZE.y,
                    x0 + CORE_SIZE.x,
                    b.corridor.0,
                ),
            }
        });

        let mut floors = Vec::with_capacity(s.floors as usize);
        let mut elevation = 0.0;
        for (level, outline) in outlines.iter().enumerate() {
            let level = level as u32;
            let height = if level == 0 {
                s.usage.ground_height()
            } else {
                FLOOR_HEIGHT
            };
            let seed = s.seed.child("floor").child(level);
            let floor = if level == 0 && s.usage.hall_ground() {
                self.hall_floor(level, elevation, height, *outline, core, seed)
            } else if s.usage == BuildingUse::House {
                self.house_floor(level, elevation, height, *outline, seed)
            } else {
                self.corridor_floor(
                    level,
                    elevation,
                    height,
                    *outline,
                    bands_of(outline),
                    core,
                    seed,
                )
            };
            floors.push(floor);
            elevation += height;
        }
        let openings = facade(&floors, &self.look, false);
        BuildingPlan {
            site: s.clone(),
            core,
            floors,
            openings,
            layout: None,
            look: self.look.clone(),
        }
    }

    /// Each floor's outline. Tall towers step back (art deco setbacks) while
    /// the core and a room strip either side still fit.
    fn outlines(&self, full: Rect) -> Vec<Rect> {
        let s = self.site;
        let mut rng = s.seed.child("setbacks").rng();
        let tower = s.floors >= 8
            && s.halls.is_none()
            && matches!(s.usage, BuildingUse::Offices | BuildingUse::Apartments)
            && s.size.y >= 2.0 * MIN_STRIP_DEPTH + CORRIDOR_WIDTH;
        let mut out = Vec::with_capacity(s.floors as usize);
        let mut current = full;
        let mut next_step = if tower {
            (s.floors as f64 * rng.range(0.35, 0.55)) as u32
        } else {
            u32::MAX
        };
        let min_w = CORE_SIZE.x + 2.0 * MIN_ROOM_WIDTH + 2.0;
        let min_d = 2.0 * MIN_STRIP_DEPTH.max(CORE_SIZE.y) + CORRIDOR_WIDTH;
        for level in 0..s.floors {
            if level == next_step && level > 0 {
                let step = rng.range(3.0, 5.0);
                let cand = current.inset(step);
                let size = cand.size();
                if size.x >= min_w && size.y >= min_d {
                    current = cand;
                }
                next_step += ((s.floors - level) as f64 * rng.range(0.3, 0.5)).max(2.0) as u32;
            }
            out.push(current);
        }
        out
    }

    /// One great hall filling the floor, with the core cut out of it if any.
    fn hall_floor(
        &self,
        level: u32,
        elevation: f64,
        height: f64,
        outline: Rect,
        core: Option<Rect>,
        seed: Seed,
    ) -> Floor {
        let mut rooms = Vec::new();
        let mut doors = Vec::new();
        match core {
            None => rooms.push(room(seed, 0, RoomKind::Hall, outline)),
            Some(c) => {
                // Hall in front of the core, and two hall wings beside it, merged as
                // three rectangles that tile the floor: front part, left, right; the core
                // sits at the back.
                let front = Rect::from_xz(outline.min.x, outline.min.y, outline.max.x, c.min.y);
                let left = Rect::from_xz(outline.min.x, c.min.y, c.min.x, outline.max.y);
                let right = Rect::from_xz(c.max.x, c.min.y, outline.max.x, outline.max.y);
                let behind = Rect::from_xz(c.min.x, c.max.y, c.max.x, outline.max.y);
                rooms.push(room(seed, 0, RoomKind::Hall, front));
                rooms.push(room(seed, 1, RoomKind::Core, c));
                doors.push(Door {
                    a: 0,
                    b: Some(1),
                    at: DVec2::new(c.center().x, c.min.y),
                    along_x: true,
                    size: DOOR_SIZE,
                });
                for (i, r) in [left, right, behind].into_iter().enumerate() {
                    if r.size().x > 0.5 && r.size().y > 0.5 {
                        let idx = rooms.len();
                        let kind = if i == 2 {
                            RoomKind::Storage
                        } else {
                            RoomKind::Hall
                        };
                        rooms.push(room(seed, 2 + i as u32, kind, r));
                        // Wings open onto the front hall over their whole width; a
                        // wide doorway stands for that. The room behind the core opens
                        // onto the left wing.
                        let d = if i == 2 {
                            Door {
                                a: idx,
                                b: Some(2),
                                at: DVec2::new(r.min.x, r.center().y),
                                along_x: false,
                                size: DOOR_SIZE,
                            }
                        } else {
                            Door {
                                a: idx,
                                b: Some(0),
                                at: DVec2::new(r.center().x, r.min.y),
                                along_x: true,
                                size: DVec2::new((r.size().x - 1.0).max(DOOR_SIZE.x), height - 1.0),
                            }
                        };
                        doors.push(d);
                    }
                }
            }
        }
        if level == 0 {
            doors.push(Door {
                a: 0,
                b: None,
                at: DVec2::new(outline.center().x, outline.min.y),
                along_x: true,
                size: DVec2::new(ENTRANCE_SIZE.x * 1.5, ENTRANCE_SIZE.y * 1.4),
            });
        }
        Floor {
            level,
            elevation,
            height,
            outline,
            rooms,
            doors,
        }
    }

    /// A house: the floor split across its width into rooms that open into
    /// each other; the front room has the front door.
    fn house_floor(
        &self,
        level: u32,
        elevation: f64,
        height: f64,
        outline: Rect,
        seed: Seed,
    ) -> Floor {
        let (kind_front, w) = if level == 0 {
            self.site.usage.ground_front_room()
        } else {
            self.site.usage.upper_room()
        };
        let mut rng = seed.child("rooms").rng();
        let cuts = partition(
            outline.min.x,
            outline.max.x,
            w,
            &self.x_lines(&outline),
            &mut rng,
        );
        let mut rooms = Vec::new();
        let mut doors = Vec::new();
        for (i, (x0, x1)) in cuts.iter().enumerate() {
            let kind = if level == 0 && i > 0 {
                self.site.usage.ground_back_room().0
            } else {
                kind_front
            };
            rooms.push(room(
                seed,
                i as u32,
                kind,
                Rect::from_xz(*x0, outline.min.y, *x1, outline.max.y),
            ));
            if i > 0 {
                doors.push(Door {
                    a: i - 1,
                    b: Some(i),
                    at: DVec2::new(*x0, outline.center().y),
                    along_x: false,
                    size: DOOR_SIZE,
                });
            }
        }
        if level == 0 {
            doors.push(Door {
                a: 0,
                b: None,
                at: DVec2::new(rooms[0].rect.center().x, outline.min.y),
                along_x: true,
                size: DOOR_SIZE,
            });
        } else {
            // Upper floor of a house: an open stair from the room below. We model it
            // as a door in the floor, so reachability tests see the link.
        }
        Floor {
            level,
            elevation,
            height,
            outline,
            rooms,
            doors,
        }
    }

    /// The common layout: a corridor along the building, room strips on one or
    /// both sides, the core behind the corridor and a lobby on the ground floor.
    #[allow(clippy::too_many_arguments)]
    fn corridor_floor(
        &self,
        level: u32,
        elevation: f64,
        height: f64,
        outline: Rect,
        bands: Bands,
        core: Option<Rect>,
        seed: Seed,
    ) -> Floor {
        let usage = self.site.usage;
        let mut rng = seed.child("rooms").rng();
        let mut rooms: Vec<Room> = Vec::new();
        let mut doors: Vec<Door> = Vec::new();
        let (x0, x1) = (outline.min.x, outline.max.x);

        let corridor = rooms.len();
        rooms.push(room(
            seed,
            0,
            RoomKind::Corridor,
            Rect::from_xz(x0, bands.corridor.0, x1, bands.corridor.1),
        ));

        // Strip next to the corridor that holds the core: the core room spans the
        // strip's whole depth at the core's x range (the shaft itself is `core`).
        let core_strip_is_back = bands.back.is_some();
        let mut add_strip =
            |z: (f64, f64), front_side: bool, rooms: &mut Vec<Room>, doors: &mut Vec<Door>| {
                let mut gaps: Vec<(f64, f64)> = vec![(x0, x1)];
                let holds_core = core.is_some() && (front_side != core_strip_is_back);
                if let (true, Some(c)) = (holds_core, core) {
                    gaps = vec![(x0, c.min.x), (c.max.x, x1)];
                    let idx = rooms.len();
                    rooms.push(room(
                        seed,
                        idx as u32,
                        RoomKind::Core,
                        Rect::from_xz(c.min.x, z.0, c.max.x, z.1),
                    ));
                    doors.push(corridor_door(
                        idx, corridor, c.min.x, c.max.x, front_side, z,
                    ));
                }
                if level == 0 && front_side {
                    // The lobby goes in the widest free stretch of the front strip, around
                    // its middle. If nothing is wide enough (a narrow building whose core
                    // fills the front), the core room itself takes the entrance.
                    let widest = gaps
                        .iter()
                        .copied()
                        .max_by(|a, b| (a.1 - a.0).total_cmp(&(b.1 - b.0)))
                        .filter(|(a, b)| b - a >= MIN_ROOM_WIDTH);
                    match widest {
                        Some((ga, gb)) => {
                            let mid = (ga + gb) * 0.5;
                            let half = ((x1 - x0) * 0.12).clamp(3.5, 8.0).min((gb - ga) * 0.5);
                            // A shop front too narrow for a shop each side of a
                            // central lobby gets a slim entrance hall at one
                            // end, and the rest is the shop.
                            let side_hall = usage.ground_front_room().0 == RoomKind::Shop
                                && gb - ga < 2.0 * half + 2.0 * MIN_ROOM_WIDTH
                                && gb - ga >= SIDE_HALL + MIN_ROOM_WIDTH;
                            let (mut l0, mut l1) = if side_hall {
                                (ga, ga + SIDE_HALL)
                            } else {
                                (mid - half, mid + half)
                            };
                            // Lobby walls stand on bay lines, and no slivers are left
                            // beside it: it takes them in.
                            let lines = self.x_lines(&outline);
                            let snap = |x: f64| {
                                lines
                                    .iter()
                                    .copied()
                                    .filter(|l| *l >= ga - 1e-9 && *l <= gb + 1e-9)
                                    .min_by(|a, b| (a - x).abs().total_cmp(&(b - x).abs()))
                                    .unwrap_or(x)
                            };
                            let (s0, s1) = (snap(l0), snap(l1));
                            if s1 - s0 >= MIN_ROOM_WIDTH {
                                l0 = s0;
                                l1 = s1;
                            }
                            if l0 - ga < MIN_ROOM_WIDTH {
                                l0 = ga;
                            }
                            if gb - l1 < MIN_ROOM_WIDTH {
                                l1 = gb;
                            }
                            let mut out = Vec::new();
                            for (a, b) in gaps {
                                if b <= l0 || a >= l1 {
                                    out.push((a, b));
                                } else {
                                    if l0 - a > 0.01 {
                                        out.push((a, l0));
                                    }
                                    if b - l1 > 0.01 {
                                        out.push((l1, b));
                                    }
                                }
                            }
                            gaps = out;
                            let idx = rooms.len();
                            rooms.push(room(
                                seed,
                                idx as u32,
                                RoomKind::Lobby,
                                Rect::from_xz(l0, z.0, l1, z.1),
                            ));
                            doors.push(corridor_door(idx, corridor, l0, l1, front_side, z));
                            let (door_x, door_w) = if side_hall {
                                ((l0 + l1) * 0.5, l1 - l0 - 0.6)
                            } else {
                                (mid, 2.0 * half - 0.6)
                            };
                            doors.push(Door {
                                a: idx,
                                b: None,
                                at: DVec2::new(door_x, z.0),
                                along_x: true,
                                size: DVec2::new(ENTRANCE_SIZE.x.min(door_w), ENTRANCE_SIZE.y),
                            });
                        }
                        None => {
                            let core_room = rooms.iter().rposition(|r| r.kind == RoomKind::Core);
                            let (idx, at_x) = match core_room {
                                Some(i) => (i, rooms[i].rect.center().x),
                                None => (corridor, (x0 + x1) * 0.5),
                            };
                            let entrance = Door {
                                a: idx,
                                b: None,
                                at: DVec2::new(at_x, z.0),
                                along_x: true,
                                size: DOOR_SIZE,
                            };
                            doors.push(entrance);
                        }
                    }
                }
                let (kind, target) = match (level, front_side) {
                    (0, true) => usage.ground_front_room(),
                    (0, false) => usage.ground_back_room(),
                    _ => usage.upper_room(),
                };
                for (a, b) in gaps {
                    if b - a < 0.01 {
                        continue;
                    }
                    let parts = partition(a, b, target, &self.x_lines(&outline), &mut rng);
                    let n = parts.len();
                    for (k, (p0, p1)) in parts.into_iter().enumerate() {
                        let idx = rooms.len();
                        // A sliver beside the core or lobby narrower than a room becomes
                        // a storage cupboard.
                        let kind = if p1 - p0 < MIN_ROOM_WIDTH {
                            RoomKind::Storage
                        } else {
                            kind
                        };
                        rooms.push(room(
                            seed,
                            idx as u32,
                            kind,
                            Rect::from_xz(p0, z.0, p1, z.1),
                        ));
                        doors.push(corridor_door(idx, corridor, p0, p1, front_side, z));
                        // Shops open onto the street too.
                        if kind == RoomKind::Shop && front_side {
                            doors.push(Door {
                                a: idx,
                                b: None,
                                at: DVec2::new((p0 + p1) * 0.5, z.0),
                                along_x: true,
                                size: DVec2::new(2.4, 3.0),
                            });
                        }
                        let _ = (k, n);
                    }
                }
            };
        add_strip(bands.front, true, &mut rooms, &mut doors);
        if let Some(back) = bands.back {
            add_strip(back, false, &mut rooms, &mut doors);
        }
        Floor {
            level,
            elevation,
            height,
            outline,
            rooms,
            doors,
        }
    }
}

fn room(seed: Seed, index: u32, kind: RoomKind, rect: Rect) -> Room {
    Room {
        id: StableId::from_seed(seed.child("room").child(index)),
        kind,
        rect,
    }
}

/// A door from a strip room (x range p0..p1, z range z) into the corridor.
fn corridor_door(
    room: usize,
    corridor: usize,
    p0: f64,
    p1: f64,
    front_side: bool,
    z: (f64, f64),
) -> Door {
    // Front strip rooms meet the corridor at their back edge, back strip rooms at
    // their front edge.
    let wall_z = if front_side { z.1 } else { z.0 };
    let width = DOOR_SIZE.x.min((p1 - p0) - 0.4).max(0.8);
    Door {
        a: room,
        b: Some(corridor),
        at: DVec2::new((p0 + p1) * 0.5, wall_z),
        along_x: true,
        size: DVec2::new(width, DOOR_SIZE.y),
    }
}

/// Bay lines along a wall from `a` to `b`: evenly spaced, as close to
/// `bay` apart as fits. Rooms are cut on them and windows sit between them,
/// so a facade has a regular rhythm.
pub fn bay_lines(a: f64, b: f64, bay: f64) -> Vec<f64> {
    let n = ((b - a) / bay).round().max(1.0) as usize;
    (0..=n).map(|k| a + (b - a) * k as f64 / n as f64).collect()
}

/// Splits `a..b` into pieces near `target` wide (never narrower than
/// MIN_ROOM_WIDTH unless the whole span is), with seeded variation; cuts
/// move to the nearest bay line in `lines` where that keeps rooms wide enough.
fn partition(
    a: f64,
    b: f64,
    target: f64,
    lines: &[f64],
    rng: &mut hw_core::Rng,
) -> Vec<(f64, f64)> {
    let len = b - a;
    let n = ((len / target).round() as usize)
        .clamp(1, (len / MIN_ROOM_WIDTH).floor().max(1.0) as usize);
    let weights: Vec<f64> = (0..n).map(|_| rng.range(0.8, 1.2)).collect();
    let total: f64 = weights.iter().sum();
    let mut cuts = Vec::with_capacity(n + 1);
    cuts.push(a);
    let mut x = a;
    for w in weights.iter().take(n - 1) {
        x += len * w / total;
        let snapped = lines
            .iter()
            .copied()
            .min_by(|p, q| (p - x).abs().total_cmp(&(q - x).abs()))
            .filter(|l| (l - x).abs() < target * 0.5)
            .unwrap_or(x);
        let prev = *cuts.last().unwrap();
        if snapped - prev >= MIN_ROOM_WIDTH * 0.75 && b - snapped >= MIN_ROOM_WIDTH * 0.75 {
            cuts.push(snapped);
        } else if x - prev >= MIN_ROOM_WIDTH * 0.75 && b - x >= MIN_ROOM_WIDTH * 0.75 {
            cuts.push(x);
        }
    }
    cuts.push(b);
    cuts.windows(2).map(|w| (w[0], w[1])).collect()
}

/// Derives every outside opening from the rooms: the outside doors, and a
/// window in every facade bay where a room that wants daylight lies behind
/// (shaped by the style: punched, ribbon, curtain glass, portholes; shop
/// fronts and lobbies get big glass on the street).
/// `tall`: a building of halls, whose windows grow with its storeys.
fn facade(floors: &[Floor], look: &Look, tall: bool) -> Vec<Opening> {
    use crate::style::WindowKind;
    const EPS: f64 = 1e-6;
    // Stairs get windows too (no wall of a town house stands blank), and
    // storerooms a high one where they do not face the street.
    let lit = |kind: RoomKind, side: Side| {
        kind.wants_daylight()
            || kind == RoomKind::Core
            || (kind == RoomKind::Storage && side != Side::Front)
    };
    let mut out = Vec::new();
    for f in floors {
        let o = f.outline;
        let sides = [
            (Side::Front, o.min.x, o.max.x),
            (Side::Back, o.min.x, o.max.x),
            (Side::Left, o.min.y, o.max.y),
            (Side::Right, o.min.y, o.max.y),
        ];
        // The room on a side at a point along it, with its span there.
        let room_at = |side: Side, t: f64| -> Option<(usize, f64, f64)> {
            f.rooms.iter().enumerate().find_map(|(ri, r)| {
                let rr = r.rect;
                let (touches, a, b) = match side {
                    Side::Front => ((rr.min.y - o.min.y).abs() < EPS, rr.min.x, rr.max.x),
                    Side::Back => ((rr.max.y - o.max.y).abs() < EPS, rr.min.x, rr.max.x),
                    Side::Left => ((rr.min.x - o.min.x).abs() < EPS, rr.min.y, rr.max.y),
                    Side::Right => ((rr.max.x - o.max.x).abs() < EPS, rr.min.y, rr.max.y),
                };
                (touches && t > a - EPS && t < b + EPS).then_some((ri, a, b))
            })
        };
        for (side, s0, s1) in sides {
            // Outside doors on this side.
            let mut taken: Vec<(f64, f64)> = Vec::new();
            for d in f.doors.iter().filter(|d| d.b.is_none()) {
                let on_wall = match side {
                    Side::Front => d.along_x && (d.at.y - o.min.y).abs() < EPS,
                    Side::Back => d.along_x && (d.at.y - o.max.y).abs() < EPS,
                    Side::Left => !d.along_x && (d.at.x - o.min.x).abs() < EPS,
                    Side::Right => !d.along_x && (d.at.x - o.max.x).abs() < EPS,
                };
                if !on_wall {
                    continue;
                }
                let along = if d.along_x { d.at.x } else { d.at.y };
                let kind = if d.size.x >= ENTRANCE_SIZE.x {
                    OpeningKind::Entrance
                } else {
                    OpeningKind::Door
                };
                out.push(Opening {
                    floor: f.level,
                    side,
                    kind,
                    room: d.a,
                    along,
                    width: d.size.x,
                    sill: 0.0,
                    height: d.size.y,
                });
                taken.push((along - d.size.x * 0.5 - 0.4, along + d.size.x * 0.5 + 0.4));
            }
            let lines = bay_lines(s0, s1, look.bay);
            let mut bays: Vec<(f64, f64)> = lines.windows(2).map(|w| (w[0], w[1])).collect();
            // A room no bay centre falls in (a corridor end, a narrow room)
            // still gets one window in its middle.
            for (ri, r) in f.rooms.iter().enumerate() {
                if !lit(r.kind, side) {
                    continue;
                }
                let rr = r.rect;
                let (touches, a, b) = match side {
                    Side::Front => ((rr.min.y - o.min.y).abs() < EPS, rr.min.x, rr.max.x),
                    Side::Back => ((rr.max.y - o.max.y).abs() < EPS, rr.min.x, rr.max.x),
                    Side::Left => ((rr.min.x - o.min.x).abs() < EPS, rr.min.y, rr.max.y),
                    Side::Right => ((rr.max.x - o.max.x).abs() < EPS, rr.min.y, rr.max.y),
                };
                let covered = bays.iter().any(|(b0, b1)| {
                    let c = (b0 + b1) * 0.5;
                    // A window fits only well inside the room.
                    c > a + 0.75 && c < b - 0.75
                });
                if touches && !covered && room_at(side, (a + b) * 0.5).map(|x| x.0) == Some(ri) {
                    let half = ((b - a) * 0.5).min(look.bay * 0.5);
                    bays.push(((a + b) * 0.5 - half, (a + b) * 0.5 + half));
                }
            }
            for (b0, b1) in bays {
                let c = (b0 + b1) * 0.5;
                let Some((ri, ra, rb)) = room_at(side, c) else {
                    continue;
                };
                let kind = f.rooms[ri].kind;
                if !lit(kind, side) {
                    continue;
                }
                let street_glass = side == Side::Front
                    && f.level == 0
                    && matches!(kind, RoomKind::Shop | RoomKind::Lobby);
                let bay = b1 - b0;
                let (mut width, sill, height) = if street_glass {
                    (bay * 0.86, 0.45, (f.height - 1.5).min(3.8))
                } else if kind == RoomKind::Core {
                    // A stair window, half a floor up at the landing.
                    let sill = f.height * 0.42;
                    (
                        (bay * look.window_share * 0.8).max(0.9),
                        sill,
                        (f.height - sill - 0.6).clamp(0.8, 1.7),
                    )
                } else if kind == RoomKind::Storage {
                    // A high, small window over the shelves.
                    let height = 0.9_f64.min(f.height - 2.4);
                    (
                        (bay * look.window_share).min(1.4),
                        f.height - height - 0.7,
                        height,
                    )
                } else {
                    match look.window {
                        WindowKind::Curtain => (bay * look.window_share, 0.15, f.height - 1.0),
                        WindowKind::Ribbon => (
                            bay * look.window_share,
                            0.95,
                            (f.height - 2.2).clamp(1.6, if tall { f.height * 0.5 } else { 2.6 }),
                        ),
                        WindowKind::Porthole => {
                            let d = (bay * look.window_share).clamp(1.0, 1.8);
                            (d, 1.1, d)
                        }
                        WindowKind::Punched | WindowKind::Arched => (
                            bay * look.window_share,
                            0.9,
                            (f.height - 1.9).clamp(1.8, if tall { f.height * 0.6 } else { 3.2 }),
                        ),
                    }
                };
                // Keep clear of the room's own walls.
                let room_room = ((c - ra).min(rb - c) - 0.3) * 2.0;
                width = width.min(room_room);
                if width < 0.8 {
                    continue;
                }
                if taken
                    .iter()
                    .any(|(t0, t1)| c + width * 0.5 > *t0 && c - width * 0.5 < *t1)
                {
                    continue;
                }
                out.push(Opening {
                    floor: f.level,
                    side,
                    kind: OpeningKind::Window,
                    room: ri,
                    along: c,
                    width,
                    sill,
                    height,
                });
            }
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn site(usage: BuildingUse, w: f64, d: f64, floors: u32, seed: u64) -> BuildingSite {
        BuildingSite {
            id: StableId(seed),
            seed: Seed(seed),
            usage,
            origin: DVec2::new(10.0, 20.0),
            along: DVec2::X,
            inward: DVec2::Y,
            size: DVec2::new(w, d),
            floors,
            street: 0,
            district: DistrictKind::Commercial,
            halls: None,
            vernacular: None,
        }
    }

    #[test]
    fn buildings_of_halls_are_planned_level_by_level() {
        for k in 0..40u64 {
            let (w, d) = (32.0 + (k % 7) as f64 * 3.5, 38.0 + (k % 5) as f64 * 3.0);
            let n = 2 + (k % 11) as u32;
            let usage = if k % 2 == 0 {
                BuildingUse::Offices
            } else {
                BuildingUse::ShopsAndOffices
            };
            let s = site(usage, w, d, 3, k).into_halls(n);
            let plan = BuildingPlan::generate(&s);
            let l = plan.layout.as_ref().expect("a layout");
            assert!(l.problems().is_empty(), "{k}: {:?}", l.problems());
            assert_eq!(plan.floors.len() as u32, s.floors, "{k}: {:?}", l.scheme);
            assert!((plan.height() - s.height()).abs() < 1e-9);
            assert_eq!(s.massing().len(), 1);
            for f in &plan.floors {
                // Rooms tile every storey, so every point is in a room.
                let area: f64 = f
                    .rooms
                    .iter()
                    .map(|r| r.rect.size().x * r.rect.size().y)
                    .sum();
                assert!((area - w * d).abs() < 1e-6, "{k} level {}", f.level);
                assert!(f.height >= 5.5 - 1e-9);
            }
            // One way in from the street, and windows on every storey.
            assert!(plan.floors[0].doors.iter().any(|d| d.b.is_none()));
            for f in &plan.floors {
                assert!(plan
                    .openings
                    .iter()
                    .any(|o| o.floor == f.level && o.kind == OpeningKind::Window));
            }
            assert_eq!(plan, BuildingPlan::generate(&s));
        }
    }

    fn all_sites() -> Vec<BuildingSite> {
        let mut v = Vec::new();
        let uses = [
            BuildingUse::Offices,
            BuildingUse::ShopsAndOffices,
            BuildingUse::Apartments,
            BuildingUse::House,
            BuildingUse::Works,
            BuildingUse::Pavilion,
            BuildingUse::Lab,
            BuildingUse::Tenement,
        ];
        let mut k = 0;
        for u in uses {
            for (w, d) in [(12.0, 10.0), (24.0, 16.0), (40.0, 24.0), (60.0, 40.0)] {
                for floors in [1, 2, 5, 14, 30] {
                    if u == BuildingUse::House && floors > 2 {
                        continue;
                    }
                    k += 1;
                    v.push(site(u, w, d, floors, k));
                }
            }
        }
        v
    }

    pub(crate) fn check_plan(p: &BuildingPlan) {
        assert_eq!(p.floors.len(), p.site.floors as usize);
        let full = Rect::from_xz(0.0, 0.0, p.site.size.x, p.site.size.y);
        for f in &p.floors {
            assert!(full.contains_rect(&f.outline, 1e-9));
            if let Some(c) = p.core {
                assert!(
                    f.outline.contains_rect(&c, 1e-9),
                    "core outside floor {}",
                    f.level
                );
                let cores = f
                    .rooms
                    .iter()
                    .filter(|r| r.kind == RoomKind::Core && r.rect.contains_rect(&c, 1e-9));
                assert_eq!(cores.count(), 1, "floor {} lacks the core", f.level);
            }
            // Rooms tile the outline: inside it, disjoint, areas add up.
            let mut sum = 0.0;
            for (i, r) in f.rooms.iter().enumerate() {
                assert!(r.rect.size().x > 0.0 && r.rect.size().y > 0.0);
                assert!(
                    f.outline.contains_rect(&r.rect, 1e-9),
                    "room {i} outside floor"
                );
                for s in &f.rooms[i + 1..] {
                    assert!(
                        r.rect.overlap(&s.rect) < 1e-6,
                        "rooms overlap on floor {}",
                        f.level
                    );
                }
                sum += r.rect.area();
            }
            assert!(
                (sum - f.outline.area()).abs() < 1e-6,
                "rooms leave gaps on floor {}",
                f.level
            );
            // Doors sit on walls of the rooms they join.
            for d in &f.doors {
                assert!(f.rooms[d.a].rect.on_boundary(d.at, 1e-6));
                match d.b {
                    Some(b) => assert!(f.rooms[b].rect.on_boundary(d.at, 1e-6)),
                    None => assert!(f.outline.on_boundary(d.at, 1e-6)),
                }
            }
        }
        // Every room can be reached from the street through doors, the core and
        // (in houses) the stair between floors.
        let mut seen: Vec<Vec<bool>> = p
            .floors
            .iter()
            .map(|f| vec![false; f.rooms.len()])
            .collect();
        let mut queue: Vec<(usize, usize)> = Vec::new();
        for d in &p.floors[0].doors {
            if d.b.is_none() && !seen[0][d.a] {
                seen[0][d.a] = true;
                queue.push((0, d.a));
            }
        }
        assert!(!queue.is_empty(), "no way in");
        while let Some((fl, r)) = queue.pop() {
            let f = &p.floors[fl];
            let mut next: Vec<(usize, usize)> = Vec::new();
            for d in &f.doors {
                match d.b {
                    Some(b) if d.a == r => next.push((fl, b)),
                    Some(b) if b == r => next.push((fl, d.a)),
                    _ => {}
                }
            }
            let vertical = match p.core {
                Some(c) => f.rooms[r].rect.contains_rect(&c, 1e-9),
                None => p.site.usage == BuildingUse::House && r == 0,
            };
            if vertical {
                for nf in [fl.wrapping_sub(1), fl + 1] {
                    if let Some(f2) = p.floors.get(nf) {
                        let target = match p.core {
                            Some(c) => f2.rooms.iter().position(|x| x.rect.contains_rect(&c, 1e-9)),
                            None => Some(0),
                        };
                        if let Some(t) = target {
                            next.push((nf, t));
                        }
                    }
                }
            }
            for (nf, nr) in next {
                if !seen[nf][nr] {
                    seen[nf][nr] = true;
                    queue.push((nf, nr));
                }
            }
        }
        for (fl, s) in seen.iter().enumerate() {
            for (r, ok) in s.iter().enumerate() {
                assert!(
                    ok,
                    "room {r} on floor {fl} ({:?}) unreachable",
                    p.floors[fl].rooms[r].kind
                );
            }
        }
        // Inside-out: every room that wants daylight and touches an outside wall
        // has a window or door there, and every opening belongs to a real room.
        for f in &p.floors {
            for (ri, r) in f.rooms.iter().enumerate() {
                let o = f.outline;
                let touches = (r.rect.min.y - o.min.y).abs() < 1e-6
                    || (r.rect.max.y - o.max.y).abs() < 1e-6
                    || (r.rect.min.x - o.min.x).abs() < 1e-6
                    || (r.rect.max.x - o.max.x).abs() < 1e-6;
                let lit = p
                    .openings
                    .iter()
                    .any(|w| w.floor == f.level && w.room == ri);
                if r.kind.wants_daylight() && touches && r.rect.size().min_element() >= 2.4 {
                    assert!(
                        lit,
                        "{:?} room {ri} on floor {} has no window",
                        r.kind, f.level
                    );
                }
            }
        }
        for w in &p.openings {
            let f = &p.floors[w.floor as usize];
            assert!(w.room < f.rooms.len());
            assert!(
                w.sill + w.height <= f.height + 1e-9,
                "opening taller than its floor"
            );
        }
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn every_programme_and_size_is_sound() {
        for s in all_sites() {
            let p = BuildingPlan::generate(&s);
            check_plan(&p);
        }
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn same_site_same_plan() {
        for s in all_sites() {
            assert_eq!(BuildingPlan::generate(&s), BuildingPlan::generate(&s));
        }
    }

    /// The mass drawn from afar is the plan's floors, stacked in tiers.
    #[test]
    fn massing_follows_the_plan() {
        for s in [
            site(BuildingUse::Offices, 50.0, 34.0, 30, 3),
            site(BuildingUse::House, 10.0, 9.0, 2, 4),
        ] {
            let p = BuildingPlan::generate(&s);
            let tiers = s.massing();
            assert_eq!(tiers.last().unwrap().end as usize, p.floors.len());
            assert!((tiers.last().unwrap().top - p.height()).abs() < 1e-9);
            for f in &p.floors {
                let t = tiers
                    .iter()
                    .find(|t| (t.first..t.end).contains(&f.level))
                    .unwrap();
                assert_eq!(t.outline, f.outline);
                assert!(f.elevation >= t.bottom - 1e-9 && f.elevation + f.height <= t.top + 1e-9);
            }
        }
    }

    #[test]
    #[ignore = "on demand (looks): scripts/check.sh --on-demand"]
    fn towers_step_back() {
        let p = BuildingPlan::generate(&site(BuildingUse::Offices, 50.0, 34.0, 30, 3));
        let first = p.floors.first().unwrap().outline.area();
        let top = p.floors.last().unwrap().outline.area();
        assert!(top < first, "no setback: {first} vs {top}");
        assert!((p.height() - (GROUND_FLOOR_HEIGHT + 29.0 * FLOOR_HEIGHT)).abs() < 1e-9);
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn shops_open_to_the_street() {
        let p = BuildingPlan::generate(&site(BuildingUse::ShopsAndOffices, 40.0, 24.0, 4, 5));
        let shop_doors = p
            .openings
            .iter()
            .filter(|o| o.floor == 0 && o.side == Side::Front && o.kind == OpeningKind::Door)
            .count();
        assert!(shop_doors >= 2);
        assert_eq!(
            p.openings
                .iter()
                .filter(|o| o.kind == OpeningKind::Entrance)
                .count(),
            1
        );
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn site_transform() {
        let mut s = site(BuildingUse::Offices, 20.0, 10.0, 3, 1);
        s.along = DVec2::new(0.0, 1.0);
        s.inward = DVec2::new(-1.0, 0.0);
        let p = s.to_city(DVec3::new(20.0, 5.0, 10.0));
        assert!(p.distance(DVec3::new(0.0, 5.0, 40.0)) < 1e-9);
        // Yaw turns building +x into `along`.
        let yaw = s.yaw();
        let (sn, c) = hw_core::det::sin_cos(yaw);
        let x_axis = DVec2::new(c, -sn);
        assert!(x_axis.distance(s.along) < 1e-9);
    }
}
