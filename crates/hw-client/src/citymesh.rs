//! Turns a city plan into Engine meshes: ground, streets, blocks and
//! buildings, either closed (shell with windows) or cut open to show rooms.
//!
//! Everything is plain coloured geometry for now. Meshes are grouped per
//! boulevard cell, so a cell can be rebuilt on its own (for example when the
//! cut-open area moves).

use engine::prelude::*;
use engine::texgen::Layer;
use glam::{DVec2, DVec3, Vec3};
use hw_gen::building::{BuildingPlan, Door, RoomKind};
use hw_gen::city::{BlockUse, CityPlan, DistrictKind, StreetClass};
use hw_gen::geom::{EdgeTag, Poly, Rect};
use hw_gen::water::{self, WATER_LEVEL};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Heights of the ground layers (m). Carriageways sit above sidewalks where
/// they overlap at junctions.
const SIDEWALK_TOP: f64 = 0.24;
const STREET_TOP: f64 = 0.26;
/// Boulevards sit a little higher, so they cover side streets at junctions
/// instead of fighting with them.
const BOULEVARD_TOP: f64 = 0.28;
pub const BLOCK_TOP: f64 = 0.22;
/// Sand on a beach, below the promenade.
const SAND_TOP: f64 = 0.08;
/// Depth of rivers and the sea near the shore (m).
pub const RIVER_DEPTH: f64 = 3.0;
/// The depth that vertex alpha 1 stands for in the water material.
pub const WATER_DEPTH_SCALE: f64 = 9.0;
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
    /// Plain faces with a texture, one mesh per texture.
    textured: Vec<(Layer, Mesh)>,
    /// Material for the faces added next.
    pub surf: Surf,
    /// Texture for the plain faces added next ([`Self::finish_parts`]
    /// keeps it; [`Self::finish`] drops textures).
    pub tex: Option<Layer>,
    /// No textures at all: faces keep their own colours (for meshes that
    /// cannot carry a texture, like a breakable building's).
    pub flat: bool,
}

/// The average colour of a texture layer (linear), so vertex colours can
/// be divided by it: a textured face then keeps the colour asked for on
/// average, with the texture's grain on top.
fn layer_mean(l: Layer) -> [f32; 3] {
    static MEANS: OnceLock<Mutex<HashMap<Layer, [f32; 3]>>> = OnceLock::new();
    let means = MEANS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(m) = means.lock().expect("means").get(&l) {
        return *m;
    }
    let t = engine::texgen::layer(l);
    let mut sum = [0.0f64; 3];
    let n = (t.albedo.len() / 4).max(1);
    for px in t.albedo.chunks_exact(4) {
        let c = Color::rgb(px[0], px[1], px[2]);
        sum[0] += c.r as f64;
        sum[1] += c.g as f64;
        sum[2] += c.b as f64;
    }
    let m = sum.map(|v| ((v / n as f64) as f32).max(0.02));
    means.lock().expect("means").insert(l, m);
    m
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
            textured: Vec::new(),
            surf: Surf::Plain,
            tex: None,
            flat: false,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.face_count() == 0
    }

    fn target(&mut self) -> &mut Mesh {
        let tex = if self.flat { None } else { self.tex };
        match self.surf {
            Surf::Plain => match tex {
                None => &mut self.mesh,
                Some(l) => {
                    let i = match self.textured.iter().position(|(k, _)| *k == l) {
                        Some(i) => i,
                        None => {
                            self.textured.push((l, Mesh::new()));
                            self.textured.len() - 1
                        }
                    };
                    &mut self.textured[i].1
                }
            },
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
            .sum::<usize>()
            + self
                .textured
                .iter()
                .map(|(_, m)| m.face_count())
                .sum::<usize>()
    }

    /// Adds everything `other` holds, moved by `offset`, keeping its
    /// materials and textures apart.
    pub fn append(&mut self, other: MeshBuilder, offset: Vec3) {
        for (to, from) in [
            (&mut self.mesh, other.mesh),
            (&mut self.glass, other.glass),
            (&mut self.glow, other.glow),
            (&mut self.metal, other.metal),
        ] {
            if from.face_count() > 0 {
                to.append_translated(&from, offset).expect("append part");
            }
        }
        for (l, m) in other.textured {
            let (s, t) = (self.surf, self.tex);
            self.surf = Surf::Plain;
            self.tex = Some(l);
            self.target()
                .append_translated(&m, offset)
                .expect("append textured part");
            self.surf = s;
            self.tex = t;
        }
    }

    /// One mesh per texture, the untextured faces (every material) first.
    pub fn finish_parts(mut self) -> Vec<Mesh> {
        let textured = std::mem::take(&mut self.textured);
        let mut out = Vec::new();
        let plain = self.finish();
        if plain.face_count() > 0 {
            out.push(plain);
        }
        for (l, mut m) in textured {
            if m.face_count() == 0 {
                continue;
            }
            engine::texgen::layer(l)
                .apply_triplanar(&mut m)
                .expect("texture a part");
            out.push(m);
        }
        out
    }

    /// One mesh with every face; textures are dropped (the faces keep
    /// their colours divided by the texture's average).
    pub fn finish(mut self) -> Mesh {
        for (_, m) in std::mem::take(&mut self.textured) {
            self.mesh
                .append_translated(&m, Vec3::ZERO)
                .expect("append part");
        }
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
        let tex = if self.flat { None } else { self.tex };
        let color = match (self.surf, tex) {
            (Surf::Plain, Some(l)) => {
                let m = layer_mean(l);
                Color {
                    r: (color.r / m[0]).min(1.0),
                    g: (color.g / m[1]).min(1.0),
                    b: (color.b / m[2]).min(1.0),
                    a: color.a,
                }
            }
            _ => color,
        };
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
    fn sand() -> Color {
        rgb(226, 206, 150)
    }
    fn quay() -> Color {
        rgb(150, 142, 128)
    }
    fn bridge() -> Color {
        rgb(176, 160, 136)
    }
    fn riverbed() -> Color {
        rgb(70, 74, 60)
    }
    pub fn water() -> Color {
        rgb(40, 92, 118)
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

/// Land, quays, beaches, streets, bridges, plazas, parks and block grounds,
/// then the river bed and the water, as named meshes (the water takes
/// Engine's water material).
pub fn ground_mesh(plan: &CityPlan) -> Vec<(String, Mesh)> {
    let mut b = MeshBuilder::new();
    let bed = WATER_LEVEL - RIVER_DEPTH;
    let sea_side = |p: DVec2| plan.water.is_water(p);
    // The land down to the water's edge, with quay walls into the water and
    // beaches sloping into the sea.
    for land in &plan.land {
        let top: Vec<DVec3> = land.pts.iter().map(|p| DVec3::new(p.x, 0.0, p.y)).collect();
        b.polygon(&top, DVec3::Y, Palette::grass());
        for i in 0..land.len() {
            let (p, q) = land.edge(i);
            let out = -land.inward(i);
            if !sea_side((p + q) * 0.5 + out * 0.5) {
                continue;
            }
            let o = DVec3::new(out.x, 0.0, out.y);
            let (p3, q3) = (DVec3::new(p.x, 0.0, p.y), DVec3::new(q.x, 0.0, q.y));
            let on_coast = plan
                .water
                .coast
                .as_ref()
                .is_some_and(|c| c.land_distance((p + q) * 0.5).abs() < 1.0);
            if on_coast {
                // Sand sloping under the sea.
                let run = o * 14.0 + DVec3::Y * (bed - 0.5);
                b.quad(
                    [p3, q3, q3 + run, p3 + run],
                    o + DVec3::Y * 2.0,
                    Palette::sand(),
                );
            } else {
                let down = DVec3::Y * bed;
                b.quad([p3, q3, q3 + down, p3 + down], o, Palette::quay());
            }
        }
    }
    // Promenades along the rivers; beaches and promenades along the coast.
    // Strips of neighbouring reaches overlap at bends, so they are drawn as
    // tops at their own heights, with sides only where a quay meets the
    // river (the beach runs on down into the sea).
    let slab = |b: &mut MeshBuilder, poly: &Poly, y: f64, color: Color, sides: bool| {
        let top: Vec<DVec3> = poly.pts.iter().map(|p| DVec3::new(p.x, y, p.y)).collect();
        b.polygon(&top, DVec3::Y, color);
        for i in 0..poly.len() {
            let (p, q) = poly.edge(i);
            let out = -poly.inward(i);
            if sides && sea_side((p + q) * 0.5 + out * 0.5) {
                let o = DVec3::new(out.x, 0.0, out.y);
                let (p0, q0) = (DVec3::new(p.x, 0.0, p.y), DVec3::new(q.x, 0.0, q.y));
                b.quad([p0, q0, q0 + DVec3::Y * y, p0 + DVec3::Y * y], o, color);
            }
        }
    };
    for (strip, street) in &plan.waterfront {
        let st = &plan.streets[*street as usize];
        if st.class == StreetClass::Shore {
            let dir = (st.b - st.a).normalize_or_zero();
            let n = dir.perp();
            let at = st.a + n * (water::SHORE_WIDTH - water::PROMENADE_WIDTH);
            if let Some(sand) = strip.clip(at, -n, EdgeTag::Inner) {
                slab(&mut b, &sand, SAND_TOP, Palette::sand(), false);
            }
            if let Some(walk) = strip.clip(at, n, EdgeTag::Inner) {
                slab(&mut b, &walk, SIDEWALK_TOP, Palette::sidewalk(), false);
            }
        } else {
            slab(&mut b, strip, SIDEWALK_TOP, Palette::sidewalk(), true);
        }
    }
    for s in plan.streets.iter().filter(|s| !s.class.is_water()) {
        let dir = (s.b - s.a).normalize_or_zero();
        if dir == DVec2::ZERO || s.a.distance(s.b) < 1.0 {
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
        if s.class == StreetClass::Boulevard && s.through {
            // Planted median, except on bridges.
            for (a, z) in median_spans(plan, s) {
                let (m, half) = ((a + z) * 0.5, (z - a) * 0.5);
                if half < 1.0 {
                    continue;
                }
                let c = s.a + dir * m;
                b.oriented_box(
                    DVec3::new(c.x, top + 0.15, c.y),
                    u,
                    DVec3::new(half, 0.15, 1.8),
                    Palette::park(),
                );
            }
        }
    }
    for br in &plan.bridges {
        bridge(&mut b, plan, br);
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
    vec![
        ("ground".into(), b.finish()),
        ("riverbed".into(), riverbed(plan)),
        ("water".into(), water_mesh(plan)),
    ]
}

/// Water surface and bed reach this far from the centre (m); the land of
/// the plan reaches a little further.
fn water_radius(plan: &CityPlan) -> f64 {
    plan.params.radius + plan.params.country() - 300.0
}

/// One sheet at `y` under all the land: a disc, and with a coast the open
/// sea beyond it out to the horizon.
fn sheet(b: &mut MeshBuilder, plan: &CityPlan, y: f64, color: Color) {
    let r = water_radius(plan);
    let disc: Vec<DVec2> = (0..72)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / 72.0;
            DVec2::new(r * a.cos(), r * a.sin())
        })
        .collect();
    let disc = Poly::new(disc, EdgeTag::Inner);
    let to3 = |p: &Poly| -> Vec<DVec3> { p.pts.iter().map(|q| DVec3::new(q.x, y, q.y)).collect() };
    match &plan.water.coast {
        None => b.polygon(&to3(&disc), DVec3::Y, color),
        Some(c) => {
            // Split where the whole coast lies further out, so the two
            // pieces never overlap (the water is see-through).
            let n = c.seaward;
            let cut = c.pts.iter().map(|p| p.dot(n)).fold(f64::MAX, f64::min) - 50.0;
            if let Some(inner) = disc.clip(n * cut, -n, EdgeTag::Inner) {
                b.polygon(&to3(&inner), DVec3::Y, color);
            }
            let far = 20_000.0;
            let t = n.perp();
            let sea = Poly::new(
                vec![
                    n * cut - t * far,
                    n * cut + t * far,
                    n * far + t * far,
                    n * far - t * far,
                ],
                EdgeTag::Inner,
            );
            b.polygon(&to3(&sea), DVec3::Y, color);
        }
    }
}

fn water_mesh(plan: &CityPlan) -> Mesh {
    let mut b = MeshBuilder::new();
    // Vertex alpha carries the depth for Engine's water shader.
    let depth = (RIVER_DEPTH / WATER_DEPTH_SCALE).clamp(0.0, 1.0);
    let c = Palette::water().with_alpha(depth as f32);
    sheet(&mut b, plan, WATER_LEVEL, c);
    b.finish()
}

fn riverbed(plan: &CityPlan) -> Mesh {
    let mut b = MeshBuilder::new();
    sheet(&mut b, plan, WATER_LEVEL - RIVER_DEPTH, Palette::riverbed());
    b.finish()
}

/// Parts of a boulevard (as distances from `a`) not on a bridge.
fn median_spans(plan: &CityPlan, s: &hw_gen::city::Street) -> Vec<(f64, f64)> {
    let len = s.a.distance(s.b);
    let dir = (s.b - s.a) / len.max(1e-9);
    let mut cuts: Vec<(f64, f64)> = plan
        .bridges
        .iter()
        .filter(|br| plan.streets[br.street as usize] == *s)
        .map(|br| {
            let (x, y) = ((br.a - s.a).dot(dir), (br.b - s.a).dot(dir));
            (x.min(y) - 4.0, x.max(y) + 4.0)
        })
        .collect();
    cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out = Vec::new();
    let mut t = s.width * 0.6;
    for (a, z) in cuts {
        if a > t {
            out.push((t, a));
        }
        t = t.max(z);
    }
    if len - s.width * 0.6 > t {
        out.push((t, len - s.width * 0.6));
    }
    out
}

/// The stonework of a bridge: a deck slab under the road, parapets and
/// piers standing on the river bed. The road itself is the boulevard.
fn bridge(b: &mut MeshBuilder, plan: &CityPlan, br: &hw_gen::city::Bridge) {
    let s = &plan.streets[br.street as usize];
    let dir = (br.b - br.a).normalize_or_zero();
    if dir == DVec2::ZERO {
        return;
    }
    let u = DVec3::new(dir.x, 0.0, dir.y);
    let v = DVec2::new(-dir.y, dir.x);
    let len = br.a.distance(br.b);
    let mid = (br.a + br.b) * 0.5;
    let half_w = s.reach();
    let stone = Palette::bridge();
    // Deck: under the road, a little wider.
    // Its top stays 2 cm under the ground, where it runs onto the banks.
    b.oriented_box(
        DVec3::new(mid.x, -0.72, mid.y),
        u,
        DVec3::new(len * 0.5, 0.7, half_w + 0.4),
        stone,
    );
    // Parapets on the sidewalks' outer edge.
    for side in [-1.0, 1.0] {
        let c = mid + v * side * (half_w - 0.25);
        b.oriented_box(
            DVec3::new(c.x, SIDEWALK_TOP + 0.5, c.y),
            u,
            DVec3::new(len * 0.5, 0.5, 0.25),
            stone,
        );
    }
    // Piers every 25 m or so, from the bed to the deck.
    let bed = WATER_LEVEL - RIVER_DEPTH;
    let n = (len / 25.0).floor() as usize;
    for k in 1..n.max(1) {
        let c = br.a + dir * (len * k as f64 / n.max(1) as f64);
        let h = -1.4 - bed;
        b.oriented_box(
            DVec3::new(c.x, bed + h * 0.5, c.y),
            u,
            DVec3::new(1.6, h * 0.5, half_w + 0.2),
            stone,
        );
    }
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
        // Room floors, coloured by use. Each stops 1 mm short of its
        // neighbours: shared edges, rounded in city space, would overlap.
        for room in &f.rooms {
            let c = room.rect.center();
            let s = room.rect.size() - glam::DVec2::splat(0.002);
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
/// A wooden crate, 1 m on a side, centred on its origin (a physics body).
pub fn crate_mesh() -> Mesh {
    let wood = rgb(176, 124, 72);
    let band = rgb(120, 80, 44);
    let mut m = Mesh::new();
    m.add_box((0.0, 0.0, 0.0), (0.96, 0.96, 0.96), wood)
        .expect("crate");
    // Dark planks round the edges, standing just proud of the faces.
    for (y, h) in [(-0.44, 0.1), (0.44, 0.1)] {
        m.add_box((0.0, y, 0.0), (1.0, h, 1.0), band).expect("band");
    }
    for (x, z) in [(-0.44, -0.44), (0.44, -0.44), (-0.44, 0.44), (0.44, 0.44)] {
        m.add_box((x, 0.0, z), (0.1, 0.78, 0.1), band)
            .expect("post");
    }
    m
}

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
