//! Turns a city plan into Engine meshes: ground, streets, blocks and
//! buildings, either closed (shell with windows) or cut open to show rooms.
//!
//! Everything is plain coloured geometry for now. Meshes are grouped per
//! boulevard cell, so a cell can be rebuilt on its own (for example when the
//! cut-open area moves).

use engine::prelude::*;
use glam::{DVec2, DVec3, Vec3};
use hw_gen::building::{BuildingPlan, Door, RoomKind};
use hw_gen::city::{BlockUse, CityPlan, DistrictKind, StreetClass};
use hw_gen::geom::Rect;

/// Heights of the ground layers (m). Carriageways sit above sidewalks where
/// they overlap at junctions.
const SIDEWALK_TOP: f64 = 0.24;
const STREET_TOP: f64 = 0.26;
/// Boulevards sit a little higher, so they cover side streets at junctions
/// instead of fighting with them.
const BOULEVARD_TOP: f64 = 0.28;
pub const BLOCK_TOP: f64 = 0.22;
/// Wall thickness inside cut-open buildings.
const WALL: f64 = 0.25;
/// Height of walls on the cut floor.
const CUT_WALL_HEIGHT: f64 = 2.4;

/// Corner signs (u, up, v) of one box face.
type BoxFace = [(f64, f64, f64); 4];

/// Which material a face gets. Faces of each kind are gathered in their own
/// mesh and merged at the end, keeping their own surface material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surf {
    Plain,
    /// Smooth and reflective.
    Glass,
    /// Lit from inside: neon, signs, coils.
    Glow,
    Metal,
}

/// A mesh plus helpers for shapes in arbitrary orientation.
pub struct MeshBuilder {
    pub mesh: Mesh,
    glass: Mesh,
    glow: Mesh,
    metal: Mesh,
    /// Material for the faces added next.
    pub surf: Surf,
}

impl Default for MeshBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl MeshBuilder {
    pub fn new() -> Self {
        Self {
            mesh: Mesh::new(),
            glass: Mesh::new(),
            glow: Mesh::new(),
            metal: Mesh::new(),
            surf: Surf::Plain,
        }
    }

    pub fn is_empty(&self) -> bool {
        [&self.mesh, &self.glass, &self.glow, &self.metal]
            .iter()
            .all(|m| m.face_count() == 0)
    }

    fn target(&mut self) -> &mut Mesh {
        match self.surf {
            Surf::Plain => &mut self.mesh,
            Surf::Glass => &mut self.glass,
            Surf::Glow => &mut self.glow,
            Surf::Metal => &mut self.metal,
        }
    }

    /// One mesh with every face, each kind with its own surface material.
    /// Faces added so far, over all materials.
    pub fn face_count(&self) -> usize {
        [&self.mesh, &self.glass, &self.glow, &self.metal]
            .iter()
            .map(|m| m.face_count())
            .sum()
    }

    pub fn finish(mut self) -> Mesh {
        let glass = SurfaceMaterial {
            roughness: 0.06,
            metallic: 0.5,
            ..SurfaceMaterial::DEFAULT
        };
        let glow = SurfaceMaterial {
            roughness: 0.4,
            emission: 1.2,
            ..SurfaceMaterial::DEFAULT
        };
        let metal = SurfaceMaterial {
            roughness: 0.35,
            metallic: 0.85,
            ..SurfaceMaterial::DEFAULT
        };
        for (mut m, surface) in [(self.glass, glass), (self.glow, glow), (self.metal, metal)] {
            if m.face_count() > 0 {
                m.set_surface_material(surface);
                self.mesh
                    .append_translated(&m, Vec3::ZERO)
                    .expect("append part");
            }
        }
        self.mesh
    }

    fn point(&mut self, p: DVec3, color: Color) -> PointId {
        let m = self.target();
        let id = m.add_point(p.as_vec3()).expect("finite point");
        m.set_point_color(id, color).expect("point exists");
        id
    }

    /// A triangle facing `outward`.
    pub fn tri(&mut self, pts: [DVec3; 3], outward: DVec3, color: Color) {
        let n = (pts[1] - pts[0]).cross(pts[2] - pts[0]);
        let order = if n.dot(outward) >= 0.0 {
            [0, 1, 2]
        } else {
            [0, 2, 1]
        };
        let ids = order.map(|i| self.point(pts[i], color));
        self.target()
            .add_triangle(ids[0], ids[1], ids[2])
            .expect("tri");
    }

    /// A convex polygon facing `outward`, as a fan.
    pub fn polygon(&mut self, pts: &[DVec3], outward: DVec3, color: Color) {
        for i in 1..pts.len().saturating_sub(1) {
            self.tri([pts[0], pts[i], pts[i + 1]], outward, color);
        }
    }

    /// A convex solid from a bottom and a top quad (corners in matching order).
    pub fn hexa(&mut self, bottom: [DVec3; 4], top: [DVec3; 4], color: Color) {
        let centre = (bottom.iter().sum::<DVec3>() + top.iter().sum::<DVec3>()) / 8.0;
        let face = |b: &mut Self, q: [DVec3; 4]| {
            let c = q.iter().sum::<DVec3>() / 4.0;
            b.quad(q, c - centre, color);
        };
        face(self, bottom);
        face(self, top);
        for i in 0..4 {
            let j = (i + 1) % 4;
            face(self, [bottom[i], bottom[j], top[j], top[i]]);
        }
    }

    /// An upright cylinder (or a cone when `r_top` is 0) from `c` up `h`.
    pub fn cylinder(
        &mut self,
        c: DVec3,
        (r, r_top): (f64, f64),
        h: f64,
        seg: usize,
        caps: bool,
        color: Color,
    ) {
        let ring = |rad: f64, y: f64| -> Vec<DVec3> {
            (0..seg)
                .map(|i| {
                    let a = std::f64::consts::TAU * i as f64 / seg as f64;
                    c + DVec3::new(rad * a.cos(), y, rad * a.sin())
                })
                .collect()
        };
        let (lo, hi) = (ring(r, 0.0), ring(r_top, h));
        for i in 0..seg {
            let j = (i + 1) % seg;
            let mid = (lo[i] + lo[j] + hi[i] + hi[j]) * 0.25;
            let out = DVec3::new(mid.x - c.x, 0.0, mid.z - c.z);
            if r_top < 1e-6 {
                self.tri([lo[i], lo[j], hi[i]], out + DVec3::Y * 0.01, color);
            } else {
                self.quad([lo[i], lo[j], hi[j], hi[i]], out, color);
            }
        }
        if caps {
            self.polygon(&lo, -DVec3::Y, color);
            if r_top > 1e-6 {
                self.polygon(&hi, DVec3::Y, color);
            }
        }
    }

    /// A dome (half sphere) of radius `r` standing on `c`.
    pub fn dome(&mut self, c: DVec3, r: f64, seg: usize, rings: usize, color: Color) {
        let at = |i: usize, k: usize| {
            let a = std::f64::consts::TAU * (i % seg) as f64 / seg as f64;
            let e = std::f64::consts::FRAC_PI_2 * k as f64 / rings as f64;
            c + DVec3::new(r * e.cos() * a.cos(), r * e.sin(), r * e.cos() * a.sin())
        };
        for k in 0..rings {
            for i in 0..seg {
                let (p0, p1, p2, p3) = (at(i, k), at(i + 1, k), at(i + 1, k + 1), at(i, k + 1));
                let out = (p0 + p1 + p2 + p3) * 0.25 - c;
                if k + 1 == rings {
                    self.tri([p0, p1, p2], out, color);
                } else {
                    self.quad([p0, p1, p2, p3], out, color);
                }
            }
        }
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
        self.target().add_face(&ids).expect("quad");
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
                self.target().add_triangle(c, ia, ib).expect("tri");
            } else {
                self.target().add_triangle(c, ib, ia).expect("tri");
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
        let (color, top) = if s.class == StreetClass::Boulevard {
            (Palette::boulevard(), BOULEVARD_TOP)
        } else {
            (Palette::asphalt(), STREET_TOP)
        };
        b.oriented_box(
            DVec3::new(mid.x, top * 0.5, mid.y),
            u,
            DVec3::new(len * 0.5 + s.width * 0.5, top * 0.5, s.width * 0.5),
            color,
        );
        if s.class == StreetClass::Boulevard {
            // Planted median.
            b.oriented_box(
                DVec3::new(mid.x, top + 0.15, mid.y),
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
    b.finish()
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
pub fn add_building(
    b: &mut MeshBuilder,
    plan: &BuildingPlan,
    _kind: DistrictKind,
    look: Look,
    detail: crate::buildingmesh::Detail,
) {
    let site = &plan.site;
    let facade = rgb(plan.look.wall[0], plan.look.wall[1], plan.look.wall[2]);
    let to_city = |x: f64, y: f64, z: f64| site.to_city(DVec3::new(x, y, z));
    let u = DVec3::new(site.along.x, 0.0, site.along.y);
    let base = BLOCK_TOP;
    let shown_floors = match look {
        Look::Shell => plan.floors.len(),
        Look::Cut { level } => (level as usize).min(plan.floors.len() - 1),
    };
    crate::buildingmesh::exterior(b, plan, shown_floors, look == Look::Shell, detail);

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
        draw_cut_walls(b, f, y, facade, &|x, y, z| to_city(x, y, z), u);
    }
}

/// One wall line on the cut floor: along x (at z = `fixed`) or along z.
#[derive(Clone, Copy, PartialEq)]
struct WallLine {
    along_x: bool,
    fixed: f64,
}

/// Walls of the cut-open floor, each drawn once: the outside walls as one
/// run per side (corners covered by the sides along x), the inside walls
/// merged per line and stopped at the outside walls, all with door gaps.
/// Nothing overlaps, so nothing z-fights.
fn draw_cut_walls(
    b: &mut MeshBuilder,
    f: &hw_gen::building::Floor,
    y: f64,
    facade: Color,
    to_city: &dyn Fn(f64, f64, f64) -> DVec3,
    u: DVec3,
) {
    let o = f.outline;
    let h = WALL * 0.5;
    let e = 1e-6;
    let on = |line: WallLine, d: &Door| {
        d.along_x == line.along_x
            && ((if line.along_x { d.at.y } else { d.at.x }) - line.fixed).abs() < e
    };
    let gaps = |line: WallLine| -> Vec<(f64, f64)> {
        let mut g: Vec<(f64, f64)> = f
            .doors
            .iter()
            .filter(|d| on(line, d))
            .map(|d| {
                let t = if line.along_x { d.at.x } else { d.at.y };
                (t - d.size.x * 0.5, t + d.size.x * 0.5)
            })
            .collect();
        g.sort_by(|a, c| a.0.total_cmp(&c.0));
        g
    };
    let mut draw = |line: WallLine, spans: &[(f64, f64)], height: f64, color: Color| {
        for &(t0, t1) in spans {
            let mut t = t0;
            let mut pieces = Vec::new();
            for (g0, g1) in gaps(line) {
                if g1 <= t0 || g0 >= t1 {
                    continue;
                }
                if g0 > t {
                    pieces.push((t, g0));
                }
                t = t.max(g1);
            }
            if t < t1 {
                pieces.push((t, t1));
            }
            for (a, z) in pieces {
                if z - a < 0.05 {
                    continue;
                }
                let m = (a + z) * 0.5;
                let (cx, cz, half) = if line.along_x {
                    (m, line.fixed, DVec3::new((z - a) * 0.5, height * 0.5, h))
                } else {
                    (line.fixed, m, DVec3::new(h, height * 0.5, (z - a) * 0.5))
                };
                b.oriented_box(to_city(cx, y + 0.3 + height * 0.5, cz), u, half, color);
            }
        }
    };

    // Outside walls: sides along x span the corners, sides along z fit between.
    let outer_h = CUT_WALL_HEIGHT + 0.4;
    for fixed in [o.min.y, o.max.y] {
        let line = WallLine {
            along_x: true,
            fixed,
        };
        draw(line, &[(o.min.x - h, o.max.x + h)], outer_h, facade);
    }
    for fixed in [o.min.x, o.max.x] {
        let line = WallLine {
            along_x: false,
            fixed,
        };
        draw(line, &[(o.min.y + h, o.max.y - h)], outer_h, facade);
    }

    // Inside walls: room edges off the outline, merged per line.
    let mut lines: Vec<(WallLine, Vec<(f64, f64)>)> = Vec::new();
    for room in &f.rooms {
        for (p, q) in rect_edges(&room.rect) {
            if on_outline(p, q, &o) {
                continue;
            }
            let along_x = (p.y - q.y).abs() < 1e-9;
            let line = WallLine {
                along_x,
                fixed: if along_x { p.y } else { p.x },
            };
            let (t0, t1) = if along_x {
                (p.x.min(q.x), p.x.max(q.x))
            } else {
                (p.y.min(q.y), p.y.max(q.y))
            };
            // Stop at the inner face of the outside walls.
            let (lo, hi) = if along_x {
                (o.min.x + h, o.max.x - h)
            } else {
                (o.min.y + h, o.max.y - h)
            };
            let span = (t0.max(lo), t1.min(hi));
            match lines
                .iter_mut()
                .find(|(l, _)| l.along_x == along_x && (l.fixed - line.fixed).abs() < e)
            {
                Some((_, spans)) => spans.push(span),
                None => lines.push((line, vec![span])),
            }
        }
    }
    for (line, mut spans) in lines {
        spans.sort_by(|a, c| a.0.total_cmp(&c.0));
        let mut merged: Vec<(f64, f64)> = Vec::new();
        for s in spans {
            match merged.last_mut() {
                Some(m) if s.0 <= m.1 + e => m.1 = m.1.max(s.1),
                _ => merged.push(s),
            }
        }
        draw(line, &merged, CUT_WALL_HEIGHT, Palette::inner_wall());
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
