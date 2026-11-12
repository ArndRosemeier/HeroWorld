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
/// The skin reaches this far under a wall or a plinth (m), so what is left
/// of the clipped edge's wobble stays hidden.
const UNDER_WALL: f64 = 0.08;
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

/// The part of triangle `t` (indices into `corners`) where the field is
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

/// Each grid square the lawn's edge crosses is cut into this many by this
/// many smaller ones. The field (the distance to a wall) is only straight
/// close to a wall, so over a 2 m square the clipped edge strayed up to half
/// a metre from it near corners and neighbouring walls: bare ground showed
/// as a sawtooth between the grass and the house.
const EDGE_SPLIT: usize = 4;

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
    // The finer corners of squares that are cut, by their place on the
    // fine grid (the coarse corners are on it too).
    let fine_step = step / EDGE_SPLIT as f64;
    let fine_w = n * EDGE_SPLIT + 1;
    let mut fine: std::collections::HashMap<usize, Corner> = std::collections::HashMap::new();
    // Per texture: its mesh and the fine-grid corners already in it.
    let mut parts: Vec<(Layer, Mesh, std::collections::HashMap<usize, PointId>)> = Vec::new();
    let noise = hw_core::Noise::new(w.seed.child("lawn-tint"));
    for j in 0..n {
        for i in 0..n {
            let q = [idx(i, j), idx(i + 1, j), idx(i + 1, j + 1), idx(i, j + 1)];
            // (The field changes by at most 1 per metre: a square whose
            // corners are all deeper than half its diagonal is all wall.)
            if q.iter().all(|&k| corners[k].f < -step * 0.75) {
                continue;
            }
            let cut = q.iter().any(|&k| corners[k].f <= 0.0);
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
            // The squares to draw: the whole one, or its finer pieces. Each is
            // four corners (key on the fine grid, corner).
            let k = if cut { EDGE_SPLIT } else { 1 };
            for sj in 0..k {
                for si in 0..k {
                    let at = [(si, sj), (si + 1, sj), (si + 1, sj + 1), (si, sj + 1)];
                    let mut keys = [0usize; 4];
                    let mut cs = [corners[0]; 4];
                    for (m, &(a, b)) in at.iter().enumerate() {
                        let (fi, fj) = (
                            i * EDGE_SPLIT + a * (EDGE_SPLIT / k),
                            j * EDGE_SPLIT + b * (EDGE_SPLIT / k),
                        );
                        keys[m] = fj * fine_w + fi;
                        cs[m] = if fi % EDGE_SPLIT == 0 && fj % EDGE_SPLIT == 0 {
                            corners[idx(fi / EDGE_SPLIT, fj / EDGE_SPLIT)]
                        } else {
                            *fine.entry(keys[m]).or_insert_with(|| {
                                let p = lo + DVec2::new(fi as f64, fj as f64) * fine_step;
                                Corner {
                                    p,
                                    f: field(w, g, p),
                                }
                            })
                        };
                    }
                    for tri in [[0, 1, 2], [0, 2, 3]] {
                        let poly = clip(tri, &cs);
                        if poly.len() < 3 {
                            continue;
                        }
                        let ids: Vec<PointId> = poly
                            .iter()
                            .map(|c| match *c {
                                Cut::Grid(m) => match shared.get(&keys[m]) {
                                    Some(&id) => id,
                                    None => {
                                        let id = add(mesh, cs[m].p);
                                        shared.insert(keys[m], id);
                                        id
                                    }
                                },
                                Cut::Edge(p) => add(mesh, p),
                            })
                            .collect();
                        // The clipped polygon is convex: a fan, facing up.
                        for m in 1..ids.len() - 1 {
                            mesh.add_triangle(ids[0], ids[m + 1], ids[m]).expect("tri");
                        }
                    }
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
    (road + UNDER_KERB).min(other + UNDER_WALL)
}

/// The ground's normal at `p`.
pub fn ground_normal(w: &TownWorld, p: DVec2) -> DVec3 {
    let e = 0.75;
    let gx = w.ground(p + DVec2::X * e) - w.ground(p - DVec2::X * e);
    let gz = w.ground(p + DVec2::Y * e) - w.ground(p - DVec2::Y * e);
    DVec3::new(-gx, 2.0 * e, -gz).normalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Is `p` inside the lawn skin drawn with grid squares of `step`?
    fn covered(w: &TownWorld, g: &Greenery, lo: DVec2, step: f64, p: DVec2) -> bool {
        let at = |step: f64, p: DVec2| {
            let q = (p - lo) / step;
            let (i, j) = (q.x.floor() as i64, q.y.floor() as i64);
            let c = |i: i64, j: i64| {
                let p = lo + DVec2::new(i as f64, j as f64) * step;
                Corner {
                    p,
                    f: field(w, g, p),
                }
            };
            [c(i, j), c(i + 1, j), c(i + 1, j + 1), c(i, j + 1)]
        };
        let mut corners = at(step, p);
        let cut = corners.iter().any(|c| c.f <= 0.0);
        if corners.iter().all(|c| c.f < -step * 0.75) {
            return false;
        }
        if cut {
            corners = at(step / EDGE_SPLIT as f64, p);
        }
        for tri in [[0, 1, 2], [0, 2, 3]] {
            let poly: Vec<DVec2> = clip(tri, &corners)
                .iter()
                .map(|k| match *k {
                    Cut::Grid(k) => corners[k].p,
                    Cut::Edge(p) => p,
                })
                .collect();
            if poly.len() < 3 {
                continue;
            }
            let side = |a: DVec2, b: DVec2| (b - a).perp_dot(p - a);
            let n = poly.len();
            let all_in = (0..n).all(|k| side(poly[k], poly[(k + 1) % n]) >= -1e-9);
            let all_in_rev = (0..n).all(|k| side(poly[k], poly[(k + 1) % n]) <= 1e-9);
            if all_in || all_in_rev {
                return true;
            }
        }
        false
    }

    /// The lawn reaches right up to a house wall (Arnd saw pale triangles of
    /// bare ground between the grass and the wall in 0.242): points just
    /// outside walls, away from the street, are on the skin.
    #[test]
    fn the_lawn_meets_house_walls() {
        let town = hw_sim::GeneratedTown::generate(hw_core::Seed(1), 300.0);
        let w = &town.world;
        let g = Greenery::new(w);
        let lo = DVec2::splat(-400.0);
        let (mut gaps, mut total) = (0, 0);
        for q in w.quarters_in(lo, -lo) {
            for b in &q.buildings {
                let c = b.site.corners();
                for k in 0..4 {
                    let (a, e) = (c[k], c[(k + 1) % 4]);
                    let out = (e - a).perp().normalize();
                    for t in [0.3, 0.5, 0.7] {
                        for off in [0.1, 0.3] {
                            for sgn in [1.0, -1.0] {
                                let p = a.lerp(e, t) + out * sgn * off;
                                let f = field(w, &g, p);
                                if g.clearances(w, p).0 < 0.5 || !(0.05..=0.5).contains(&f) {
                                    continue;
                                }
                                total += 1;
                                gaps += usize::from(!covered(w, &g, lo, 2.0, p));
                            }
                        }
                    }
                }
            }
        }
        assert!(total > 1000, "only {total} spots by walls");
        // A few spots by corners where a street meets a wall may stay bare.
        assert!(
            gaps * 200 <= total,
            "{gaps} of {total} spots by walls are bare"
        );
    }

    /// The lawn also reaches right up to the pavements, at bends, junction
    /// plates and road ends: ground a hand's width outside a pavement, clear
    /// of walls, is on the skin.
    #[test]
    fn the_lawn_meets_pavements() {
        let town = hw_sim::GeneratedTown::generate(hw_core::Seed(1), 300.0);
        let w = &town.world;
        let g = Greenery::new(w);
        let lo = DVec2::splat(-300.0);
        let (mut gaps, mut total) = (0, 0);
        for j in 0..800 {
            for i in 0..800 {
                let p = lo + DVec2::new(i as f64 + 0.37, j as f64 + 0.61) * 0.75;
                let (road, other) = g.clearances(w, p);
                if !(0.05..0.3).contains(&road) || other < 1.0 {
                    continue;
                }
                total += 1;
                gaps += usize::from(!covered(w, &g, lo, 2.0, p));
            }
        }
        assert!(total > 1000, "only {total} spots by pavements");
        assert!(
            gaps * 200 <= total,
            "{gaps} of {total} spots by pavements are bare"
        );
    }
}
