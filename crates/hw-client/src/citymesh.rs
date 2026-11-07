//! Turns a city plan into Engine meshes: ground, streets, blocks and
//! buildings, either closed (shell with windows) or cut open to show rooms.
//!
//! Everything is plain coloured geometry for now. Meshes are grouped per
//! boulevard cell, so a cell can be rebuilt on its own (for example when the
//! cut-open area moves).

use engine::prelude::*;
use glam::{DVec2, DVec3, Vec3};
use hw_gen::building::{
    BuildingPlan, BuildingSite, BuildingUse, Door, OpeningKind, RoomKind, Side,
};
use hw_gen::city::{BlockUse, CityPlan, DistrictKind, StreetClass};
use hw_gen::geom::Rect;

/// Heights of the ground layers (m). Carriageways sit above sidewalks where
/// they overlap at junctions.
const SIDEWALK_TOP: f64 = 0.22;
const STREET_TOP: f64 = 0.26;
const BLOCK_TOP: f64 = 0.22;
/// How far windows stand out of the wall, against z-fighting.
const GLASS_OFFSET: f64 = 0.05;
/// Wall thickness inside cut-open buildings.
const WALL: f64 = 0.25;
/// Height of walls on the cut floor.
const CUT_WALL_HEIGHT: f64 = 2.4;

/// Corner signs (u, up, v) of one box face.
type BoxFace = [(f64, f64, f64); 4];

/// A mesh plus helpers for shapes in arbitrary orientation.
pub struct MeshBuilder {
    pub mesh: Mesh,
}

impl Default for MeshBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl MeshBuilder {
    pub fn new() -> Self {
        Self { mesh: Mesh::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.mesh.face_count() == 0
    }

    fn point(&mut self, p: DVec3, color: Color) -> PointId {
        let id = self.mesh.add_point(p.as_vec3()).expect("finite point");
        self.mesh.set_point_color(id, color).expect("point exists");
        id
    }

    /// A quad facing `outward` (winding fixed up to match).
    pub fn quad(&mut self, pts: [DVec3; 4], outward: DVec3, color: Color) {
        let n = (pts[1] - pts[0]).cross(pts[2] - pts[0]);
        let order = if n.dot(outward) >= 0.0 {
            [0, 1, 2, 3]
        } else {
            [0, 3, 2, 1]
        };
        let ids = order.map(|i| self.point(pts[i], color));
        self.mesh.add_face(&ids).expect("quad");
    }

    /// A box with centre `c`, unit axes `u` (horizontal) and `v = up × u`
    /// handled for you, and half sizes (along u, up, along v).
    pub fn oriented_box(&mut self, c: DVec3, u: DVec3, half: DVec3, color: Color) {
        let up = DVec3::Y;
        let v = u.cross(up);
        let (hu, hy, hv) = (u * half.x, up * half.y, v * half.z);
        let corner = |su: f64, sy: f64, sv: f64| c + hu * su + hy * sy + hv * sv;
        let faces: [(BoxFace, DVec3); 6] = [
            (
                [(1., -1., -1.), (1., 1., -1.), (1., 1., 1.), (1., -1., 1.)],
                u,
            ),
            (
                [
                    (-1., -1., -1.),
                    (-1., -1., 1.),
                    (-1., 1., 1.),
                    (-1., 1., -1.),
                ],
                -u,
            ),
            (
                [(-1., 1., -1.), (-1., 1., 1.), (1., 1., 1.), (1., 1., -1.)],
                up,
            ),
            (
                [
                    (-1., -1., -1.),
                    (1., -1., -1.),
                    (1., -1., 1.),
                    (-1., -1., 1.),
                ],
                -up,
            ),
            (
                [(-1., -1., 1.), (1., -1., 1.), (1., 1., 1.), (-1., 1., 1.)],
                v,
            ),
            (
                [
                    (-1., -1., -1.),
                    (-1., 1., -1.),
                    (1., 1., -1.),
                    (1., -1., -1.),
                ],
                -v,
            ),
        ];
        for (f, n) in faces {
            self.quad(f.map(|(a, b, d)| corner(a, b, d)), n, color);
        }
    }

    /// A convex ground polygon raised to `top` with sides down to `bottom`.
    pub fn prism(&mut self, pts: &[DVec2], bottom: f64, top: f64, color: Color) {
        let n = pts.len();
        if n < 3 {
            return;
        }
        let at = |p: DVec2, y: f64| DVec3::new(p.x, y, p.y);
        let centre = pts.iter().copied().sum::<DVec2>() / n as f64;
        let c = self.point(at(centre, top), color);
        for i in 0..n {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            let (ia, ib) = (self.point(at(a, top), color), self.point(at(b, top), color));
            // Fan triangle; order it to face up.
            let n3 = (at(a, top) - at(centre, top)).cross(at(b, top) - at(centre, top));
            if n3.y >= 0.0 {
                self.mesh.add_triangle(c, ia, ib).expect("tri");
            } else {
                self.mesh.add_triangle(c, ib, ia).expect("tri");
            }
            let side = (b - a).perp();
            let out = DVec3::new(side.x, 0.0, side.y);
            let out = if out.dot(at((a + b) * 0.5 - centre, 0.0)) >= 0.0 {
                out
            } else {
                -out
            };
            self.quad(
                [at(a, bottom), at(b, bottom), at(b, top), at(a, top)],
                out,
                color,
            );
        }
    }
}

/// Colours of the utopia and the dystopia (sRGB, picked by eye).
pub struct Palette;

impl Palette {
    pub fn grass() -> Color {
        rgb(104, 140, 84)
    }
    fn asphalt() -> Color {
        rgb(62, 64, 70)
    }
    fn boulevard() -> Color {
        rgb(70, 72, 78)
    }
    fn sidewalk() -> Color {
        rgb(196, 190, 176)
    }
    fn plaza() -> Color {
        rgb(214, 196, 170)
    }
    fn park() -> Color {
        rgb(96, 150, 76)
    }

    fn block_ground(kind: DistrictKind) -> Color {
        match kind {
            DistrictKind::Civic => rgb(222, 214, 196),
            DistrictKind::Commercial => rgb(212, 200, 182),
            DistrictKind::Residential => rgb(132, 168, 102),
            DistrictKind::AetherWorks => rgb(150, 154, 158),
            DistrictKind::Expo => rgb(226, 214, 180),
            DistrictKind::Dystopia => rgb(96, 98, 92),
        }
    }

    /// Facade colour: a few deco tones per district, picked by the building's seed.
    fn facade(site: &BuildingSite, kind: DistrictKind) -> Color {
        let tones: &[(u8, u8, u8)] = match kind {
            DistrictKind::Civic => &[
                (238, 230, 210),
                (226, 214, 188),
                (244, 238, 226),
                (214, 206, 196),
            ],
            DistrictKind::Commercial => &[
                (236, 196, 170),
                (232, 214, 160),
                (190, 218, 196),
                (230, 222, 206),
            ],
            DistrictKind::Residential => &[
                (240, 226, 200),
                (214, 228, 222),
                (236, 206, 196),
                (226, 232, 206),
            ],
            DistrictKind::AetherWorks => &[(150, 166, 182), (170, 180, 188), (132, 150, 170)],
            DistrictKind::Expo => &[(248, 246, 240), (246, 214, 150), (176, 214, 236)],
            DistrictKind::Dystopia => &[(98, 100, 92), (86, 90, 86), (112, 104, 94), (78, 82, 80)],
        };
        let t = tones[(site.seed.0 % tones.len() as u64) as usize];
        let base = rgb(t.0, t.1, t.2);
        if site.usage == BuildingUse::Lab {
            rgb(60, 92, 66)
        } else {
            base
        }
    }

    fn glass(kind: DistrictKind) -> Color {
        match kind {
            DistrictKind::Dystopia => rgb(48, 64, 56),
            DistrictKind::AetherWorks => rgb(96, 150, 176),
            _ => rgb(74, 112, 140),
        }
    }

    fn door() -> Color {
        rgb(92, 64, 46)
    }

    fn roof(kind: DistrictKind) -> Color {
        match kind {
            DistrictKind::Dystopia => rgb(70, 72, 68),
            _ => rgb(168, 160, 150),
        }
    }

    /// Floor colour of a room in a cut-open building, so rooms read from above.
    fn room_floor(kind: RoomKind) -> Color {
        match kind {
            RoomKind::Lobby => rgb(214, 180, 120),
            RoomKind::Corridor => rgb(190, 186, 178),
            RoomKind::Core => rgb(120, 120, 128),
            RoomKind::Office => rgb(150, 176, 204),
            RoomKind::Apartment => rgb(206, 170, 136),
            RoomKind::Living => rgb(214, 160, 120),
            RoomKind::Kitchen => rgb(226, 222, 196),
            RoomKind::Bedroom => rgb(196, 168, 200),
            RoomKind::Shop => rgb(236, 150, 120),
            RoomKind::Storage => rgb(140, 130, 116),
            RoomKind::Hall => rgb(170, 184, 160),
            RoomKind::Gallery => rgb(232, 210, 150),
            RoomKind::Lab => rgb(120, 200, 140),
        }
    }

    fn inner_wall() -> Color {
        rgb(240, 236, 228)
    }
}

/// Ground plane, streets, sidewalks, plazas, parks and block grounds.
pub fn ground_mesh(plan: &CityPlan) -> Mesh {
    let mut b = MeshBuilder::new();
    let r = plan.params.radius + 600.0;
    b.mesh
        .add_box(
            (0.0, -0.5, 0.0),
            (r as f32 * 2.0, 1.0, r as f32 * 2.0),
            Palette::grass(),
        )
        .expect("ground");
    for s in &plan.streets {
        let dir = (s.b - s.a).normalize_or_zero();
        if dir == DVec2::ZERO {
            continue;
        }
        let u = DVec3::new(dir.x, 0.0, dir.y);
        let len = s.a.distance(s.b);
        let mid = (s.a + s.b) * 0.5;
        // Sidewalks: the full reach; the carriageway on top. Both run on past the
        // ends by half their width so junctions close.
        let reach = s.reach();
        b.oriented_box(
            DVec3::new(mid.x, SIDEWALK_TOP * 0.5, mid.y),
            u,
            DVec3::new(len * 0.5 + reach, SIDEWALK_TOP * 0.5, reach),
            Palette::sidewalk(),
        );
        let color = if s.class == StreetClass::Boulevard {
            Palette::boulevard()
        } else {
            Palette::asphalt()
        };
        b.oriented_box(
            DVec3::new(mid.x, STREET_TOP * 0.5, mid.y),
            u,
            DVec3::new(len * 0.5 + s.width * 0.5, STREET_TOP * 0.5, s.width * 0.5),
            color,
        );
        if s.class == StreetClass::Boulevard {
            // Planted median.
            b.oriented_box(
                DVec3::new(mid.x, STREET_TOP + 0.15, mid.y),
                u,
                DVec3::new((len * 0.5 - s.width * 0.6).max(1.0), 0.15, 1.8),
                Palette::park(),
            );
        }
    }
    for blk in &plan.blocks {
        let kind = plan.districts[blk.district].kind;
        let color = match blk.usage {
            BlockUse::Plaza => Palette::plaza(),
            BlockUse::Park => Palette::park(),
            BlockUse::Lots => Palette::block_ground(kind),
        };
        b.prism(&blk.poly.pts, 0.0, BLOCK_TOP, color);
    }
    b.mesh
}

/// How to draw one building.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// Closed: walls, windows, roofs.
    Shell,
    /// Floors below `level` closed, floor `level` cut open to show its rooms,
    /// nothing above.
    Cut { level: u32 },
}

/// Adds one building to `b`.
pub fn add_building(b: &mut MeshBuilder, plan: &BuildingPlan, kind: DistrictKind, look: Look) {
    let site = &plan.site;
    let facade = Palette::facade(site, kind);
    let glass = Palette::glass(kind);
    let to_city = |x: f64, y: f64, z: f64| site.to_city(DVec3::new(x, y, z));
    let u = DVec3::new(site.along.x, 0.0, site.along.y);
    let v = DVec3::new(site.inward.x, 0.0, site.inward.y);
    let base = BLOCK_TOP;

    let shown_floors = match look {
        Look::Shell => plan.floors.len(),
        Look::Cut { level } => (level as usize).min(plan.floors.len() - 1),
    };
    // Closed storeys: one box per run of floors with the same outline.
    let mut i = 0;
    while i < shown_floors {
        let outline = plan.floors[i].outline;
        let mut j = i;
        while j + 1 < shown_floors && plan.floors[j + 1].outline == outline {
            j += 1;
        }
        let y0 = plan.floors[i].elevation;
        let y1 = plan.floors[j].elevation + plan.floors[j].height;
        let c = outline.center();
        let s = outline.size();
        b.oriented_box(
            to_city(c.x, base + (y0 + y1) * 0.5, c.y),
            u,
            DVec3::new(s.x * 0.5, (y1 - y0) * 0.5, s.y * 0.5),
            facade,
        );
        // A flat roof slab where the next floor steps back (or at the top),
        // except under a cut-open floor, whose room floors take its place.
        let under_cut = matches!(look, Look::Cut { .. }) && j + 1 == shown_floors;
        if !under_cut {
            b.oriented_box(
                to_city(c.x, base + y1 + 0.15, c.y),
                u,
                DVec3::new(s.x * 0.5 + 0.3, 0.15, s.y * 0.5 + 0.3),
                Palette::roof(kind),
            );
        }
        i = j + 1;
    }
    // Windows and doors of closed storeys, straight from the plan's facade.
    for o in plan
        .openings
        .iter()
        .filter(|o| (o.floor as usize) < shown_floors)
    {
        let f = &plan.floors[o.floor as usize];
        let r = f.outline;
        let (y0, y1) = (
            base + f.elevation + o.sill,
            base + f.elevation + o.sill + o.height,
        );
        let (a0, a1) = (o.along - o.width * 0.5, o.along + o.width * 0.5);
        let (pts, out) = match o.side {
            Side::Front => {
                let z = r.min.y - GLASS_OFFSET;
                ([(a0, z), (a1, z), (a1, z), (a0, z)], -v)
            }
            Side::Back => {
                let z = r.max.y + GLASS_OFFSET;
                ([(a0, z), (a1, z), (a1, z), (a0, z)], v)
            }
            Side::Left => {
                let x = r.min.x - GLASS_OFFSET;
                ([(x, a0), (x, a1), (x, a1), (x, a0)], -u)
            }
            Side::Right => {
                let x = r.max.x + GLASS_OFFSET;
                ([(x, a0), (x, a1), (x, a1), (x, a0)], u)
            }
        };
        let ys = [y0, y0, y1, y1];
        let quad = [0, 1, 2, 3].map(|k| to_city(pts[k].0, ys[k], pts[k].1));
        let color = match o.kind {
            OpeningKind::Window => glass,
            OpeningKind::Door | OpeningKind::Entrance => Palette::door(),
        };
        b.quad(quad, out, color);
    }

    if let Look::Cut { .. } = look {
        let f = &plan.floors[shown_floors];
        let y = base + f.elevation;
        // Room floors, coloured by use.
        for room in &f.rooms {
            let c = room.rect.center();
            let s = room.rect.size();
            b.oriented_box(
                to_city(c.x, y + 0.15, c.y),
                u,
                DVec3::new(s.x * 0.5, 0.15, s.y * 0.5),
                Palette::room_floor(room.kind),
            );
        }
        // Walls: every room edge, with gaps for doors. Shared walls are drawn
        // from both rooms at the same place, which is harmless.
        for (ri, room) in f.rooms.iter().enumerate() {
            let doors: Vec<&Door> = f
                .doors
                .iter()
                .filter(|d| d.a == ri || d.b == Some(ri))
                .collect();
            let outer = f.outline;
            for (p, q) in rect_edges(&room.rect) {
                let along_x = (p.y - q.y).abs() < 1e-9;
                let exterior = on_outline(p, q, &outer);
                let color = if exterior {
                    facade
                } else {
                    Palette::inner_wall()
                };
                let mut gaps: Vec<(f64, f64)> = doors
                    .iter()
                    .filter(|d| d.along_x == along_x && on_segment(d.at, p, q))
                    .map(|d| {
                        let t = if along_x { d.at.x } else { d.at.y };
                        (t - d.size.x * 0.5, t + d.size.x * 0.5)
                    })
                    .collect();
                // Windows on outside walls show as gaps too (cut below the sill
                // they would be wall, so keep it simple: wall up to the sill).
                gaps.sort_by(|a, c| a.0.total_cmp(&c.0));
                let (t0, t1) = if along_x {
                    (p.x.min(q.x), p.x.max(q.x))
                } else {
                    (p.y.min(q.y), p.y.max(q.y))
                };
                let mut t = t0;
                let fixed = if along_x { p.y } else { p.x };
                let mut pieces = Vec::new();
                for (g0, g1) in gaps {
                    if g0 > t {
                        pieces.push((t, g0.min(t1)));
                    }
                    t = t.max(g1);
                }
                if t < t1 {
                    pieces.push((t, t1));
                }
                let height = if exterior {
                    CUT_WALL_HEIGHT + 0.4
                } else {
                    CUT_WALL_HEIGHT
                };
                for (a, z) in pieces {
                    if z - a < 0.05 {
                        continue;
                    }
                    let m = (a + z) * 0.5;
                    let (cx, cz) = if along_x { (m, fixed) } else { (fixed, m) };
                    let half = if along_x {
                        DVec3::new((z - a) * 0.5, height * 0.5, WALL * 0.5)
                    } else {
                        DVec3::new(WALL * 0.5, height * 0.5, (z - a) * 0.5)
                    };
                    b.oriented_box(to_city(cx, y + 0.3 + height * 0.5, cz), u, half, color);
                }
            }
        }
    }
}

fn rect_edges(r: &Rect) -> [(DVec2, DVec2); 4] {
    let (a, b) = (r.min, r.max);
    [
        (a, DVec2::new(b.x, a.y)),
        (DVec2::new(b.x, a.y), b),
        (b, DVec2::new(a.x, b.y)),
        (DVec2::new(a.x, b.y), a),
    ]
}

fn on_outline(p: DVec2, q: DVec2, o: &Rect) -> bool {
    let e = 1e-6;
    ((p.x - o.min.x).abs() < e && (q.x - o.min.x).abs() < e)
        || ((p.x - o.max.x).abs() < e && (q.x - o.max.x).abs() < e)
        || ((p.y - o.min.y).abs() < e && (q.y - o.min.y).abs() < e)
        || ((p.y - o.max.y).abs() < e && (q.y - o.max.y).abs() < e)
}

fn on_segment(x: DVec2, p: DVec2, q: DVec2) -> bool {
    let e = 1e-6;
    let d = q - p;
    let t = (x - p).dot(d) / d.length_squared().max(1e-12);
    (-e..=1.0 + e).contains(&t) && (p + d * t).distance(x) < e
}

/// Camera-space helper: the point where a ray from `origin` along `dir` meets
/// the ground (y = 0), if it points down.
pub fn ground_hit(origin: Vec3, dir: Vec3) -> Option<Vec3> {
    (dir.y < -1e-4).then(|| origin + dir * (-origin.y / dir.y))
}

/// A stand-in figure for a hero (feet at the origin): red for yours, blue
/// for friends.
pub fn hero_mesh(mine: bool) -> Mesh {
    let (body, head) = if mine {
        (rgb(200, 40, 46), rgb(236, 196, 160))
    } else {
        (rgb(46, 92, 200), rgb(236, 196, 160))
    };
    let mut m = Mesh::new();
    m.add_box((0.0, 0.45, 0.0), (0.36, 0.9, 0.3), rgb(40, 40, 60))
        .expect("legs");
    m.add_box((0.0, 1.2, 0.0), (0.56, 0.62, 0.32), body)
        .expect("torso");
    m.add_box((0.0, 1.66, 0.0), (0.26, 0.28, 0.26), head)
        .expect("head");
    m.add_box((0.0, 1.25, 0.2), (0.6, 0.5, 0.06), body)
        .expect("cape");
    m
}
