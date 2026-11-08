//! Lawns and meadows on the town's ground: a textured skin draped over
//! Engine's terrain wherever grass grows ([`hw_gen::urban::greenery`]),
//! cut cleanly along the kerbs, the walls and the sand.
//!
//! Engine's streamed ground shades flat ground a plain noisy green. Over it
//! lies, a few centimetres up, a grid of lawn (mown grass with clover) or
//! meadow (long grass with flowers), tinted by the kind of grass and the
//! region's climate. Each grid square is split in two triangles and each
//! triangle clipped where the distance to the nearest road, wall or beach
//! crosses zero, so the edges follow the kerbs rather than the grid. The
//! skin reaches a little way under the pavements, which stand higher.

use crate::citymesh::layer_mean;
use engine::prelude::*;
use engine::texgen::{Layer, NaturalLayer};
use glam::{DVec2, DVec3};
use hw_gen::style::Region;
use hw_gen::urban::greenery::{climate, Greenery, Sward};
use hw_gen::urban::world::TownWorld;

/// How far the skin floats over the ground (m): under every road surface
/// (at least [`hw_gen::urban::world::ROAD_LIFT`] + 0.02 up), over Engine's.
pub const LIFT: f64 = 0.035;
/// The skin reaches this far under a pavement (m).
const UNDER_KERB: f64 = 0.45;
/// Side of the squares in which the kind of grass is looked up (m).
const SWARD_CELL: f64 = 8.0;

/// The texture a kind of grass is drawn with.
fn texture(s: Sward) -> Layer {
    match s {
        // The lawn texture's dry patches repeat visibly every tile; the
        // wild grass is even, and the tint makes it a lawn or a meadow.
        Sward::Lawn | Sward::Park | Sward::Meadow => NaturalLayer::WildGrass.into(),
        Sward::Rough => NaturalLayer::ChalkTurf.into(),
    }
}

/// The average colour of a kind of grass in a region (linear).
pub fn sward_colour(s: Sward, region: Region) -> [f32; 3] {
    let lush = match s {
        Sward::Lawn => Color::rgb(84, 118, 48),
        Sward::Park => Color::rgb(90, 122, 50),
        Sward::Meadow => Color::rgb(104, 118, 56),
        Sward::Rough => Color::rgb(118, 112, 66),
    };
    let dry = match s {
        Sward::Lawn | Sward::Park => Color::rgb(128, 128, 66),
        Sward::Meadow => Color::rgb(160, 142, 84),
        Sward::Rough => Color::rgb(150, 128, 86),
    };
    let (_, d) = climate(region);
    // Lawns are watered: they keep greener than the open country.
    let d = match s {
        Sward::Lawn | Sward::Park => d * 0.6,
        _ => d,
    } as f32;
    [
        lush.r + (dry.r - lush.r) * d,
        lush.g + (dry.g - lush.g) * d,
        lush.b + (dry.b - lush.b) * d,
    ]
}

/// The kinds of grass over an area, looked up once per [`SWARD_CELL`].
struct SwardMap {
    lo: DVec2,
    n: usize,
    cells: Vec<Sward>,
}

impl SwardMap {
    fn new(w: &TownWorld, g: &Greenery, lo: DVec2, hi: DVec2) -> Self {
        let n = ((hi - lo).max_element() / SWARD_CELL).ceil() as usize + 1;
        let mut cells = Vec::with_capacity(n * n);
        for j in 0..n {
            for i in 0..n {
                let p = lo + DVec2::new(i as f64 + 0.5, j as f64 + 0.5) * SWARD_CELL;
                cells.push(g.sward_at(w, p));
            }
        }
        SwardMap { lo, n, cells }
    }

    fn at(&self, p: DVec2) -> Sward {
        let q = ((p - self.lo) / SWARD_CELL).floor();
        let i = (q.x.max(0.0) as usize).min(self.n - 1);
        let j = (q.y.max(0.0) as usize).min(self.n - 1);
        self.cells[j * self.n + i]
    }
}

/// One vertex of the grid: where, the field (positive where grass grows).
#[derive(Clone, Copy)]
struct Corner {
    p: DVec2,
    f: f64,
}

/// A corner of a clipped triangle: a grid corner (shared between squares)
/// or a point on an edge where the field crosses zero.
#[derive(Clone, Copy)]
enum Cut {
    Grid(usize),
    Edge(DVec2),
}

/// The part of triangle `t` (grid corner indices) where the field is
/// positive: convex, 0 to 4 corners, interpolated along the edges.
fn clip(t: [usize; 3], corners: &[Corner]) -> Vec<Cut> {
    let mut out = Vec::with_capacity(4);
    for k in 0..3 {
        let (ia, ib) = (t[k], t[(k + 1) % 3]);
        let (a, b) = (corners[ia], corners[ib]);
        if a.f > 0.0 {
            out.push(Cut::Grid(ia));
        }
        if (a.f > 0.0) != (b.f > 0.0) {
            let s = a.f / (a.f - b.f);
            out.push(Cut::Edge(a.p.lerp(b.p, s)));
        }
    }
    out
}

/// The lawn and meadow skin over the square `lo`..`hi`, with grid squares of
/// `step` metres: one mesh per texture.
pub fn lawn(w: &TownWorld, g: &Greenery, lo: DVec2, hi: DVec2, step: f64) -> Vec<Mesh> {
    let n = ((hi - lo).max_element() / step).round().max(1.0) as usize;
    let swards = SwardMap::new(w, g, lo, hi);
    // The field at every grid corner: grass reaches a little way under the
    // pavements (hidden by them), but not into the sand or the walls.
    let corners: Vec<Corner> = (0..=n)
        .flat_map(|j| (0..=n).map(move |i| (i, j)))
        .map(|(i, j)| {
            let p = lo + DVec2::new(i as f64, j as f64) * step;
            Corner {
                p,
                f: field(w, g, p),
            }
        })
        .collect();
    let idx = |i: usize, j: usize| j * (n + 1) + i;
    // Per texture: its mesh and the grid corners already in it.
    let mut parts: Vec<(Layer, Mesh, std::collections::HashMap<usize, PointId>)> = Vec::new();
    let noise = hw_core::Noise::new(w.seed.child("lawn-tint"));
    for j in 0..n {
        for i in 0..n {
            let q = [idx(i, j), idx(i + 1, j), idx(i + 1, j + 1), idx(i, j + 1)];
            if q.iter().all(|&k| corners[k].f <= 0.0) {
                continue;
            }
            let sward = swards.at((corners[q[0]].p + corners[q[2]].p) * 0.5);
            let layer = texture(sward);
            let slot = match parts.iter().position(|(l, _, _)| *l == layer) {
                Some(k) => k,
                None => {
                    parts.push((layer, Mesh::new(), Default::default()));
                    parts.len() - 1
                }
            };
            let (_, mesh, shared) = &mut parts[slot];
            let base = sward_colour(sward, g.region);
            let mean = layer_mean(layer.into());
            let add = |mesh: &mut Mesh, p: DVec2| -> PointId {
                let h = w.ground(p);
                let id = mesh
                    .add_point(DVec3::new(p.x, h + LIFT, p.y).as_vec3())
                    .expect("finite point");
                mesh.set_point_normal(id, ground_normal(w, p).as_vec3())
                    .expect("point");
                // Broad patches a little lighter or darker.
                let v = 1.0 + 0.09 * noise.fbm(p.x * 0.05, p.y * 0.05, 2) as f32;
                let col = Color {
                    r: (base[0] * v / mean[0]).min(1.0),
                    g: (base[1] * v / mean[1]).min(1.0),
                    b: (base[2] * v / mean[2]).min(1.0),
                    a: 1.0,
                };
                mesh.set_point_color(id, col).expect("point");
                id
            };
            for tri in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
                let poly = clip(tri, &corners);
                if poly.len() < 3 {
                    continue;
                }
                let ids: Vec<PointId> = poly
                    .iter()
                    .map(|c| match *c {
                        Cut::Grid(k) => match shared.get(&k) {
                            Some(&id) => id,
                            None => {
                                let id = add(mesh, corners[k].p);
                                shared.insert(k, id);
                                id
                            }
                        },
                        Cut::Edge(p) => add(mesh, p),
                    })
                    .collect();
                // The clipped polygon is convex: a fan, facing up.
                for k in 1..ids.len() - 1 {
                    mesh.add_triangle(ids[0], ids[k + 1], ids[k]).expect("tri");
                }
            }
        }
    }
    parts
        .into_iter()
        .filter(|(_, m, _)| m.face_count() > 0)
        .map(|(l, mut m, _)| {
            m.set_surface_material(SurfaceMaterial {
                roughness: 0.95,
                ..SurfaceMaterial::DEFAULT
            });
            let t = engine::texgen::layer(l);
            // A wider tile repeats less obviously across a park.
            t.apply_scaled(&mut m, t.tile_metres * 1.6)
                .expect("texture the lawn");
            m
        })
        .collect()
}

/// Positive where the skin is drawn: it may reach a little way under the
/// pavements (they stand higher), but not into walls or the sand.
fn field(w: &TownWorld, g: &Greenery, p: DVec2) -> f64 {
    let (road, other) = g.clearances(w, p);
    (road + UNDER_KERB).min(other)
}

/// The ground's normal at `p`.
pub fn ground_normal(w: &TownWorld, p: DVec2) -> DVec3 {
    let e = 0.75;
    let gx = w.ground(p + DVec2::X * e) - w.ground(p - DVec2::X * e);
    let gz = w.ground(p + DVec2::Y * e) - w.ground(p - DVec2::Y * e);
    DVec3::new(-gx, 2.0 * e, -gz).normalize()
}
