//! Finds z-fighting: triangles that lie in (almost) the same plane, face the
//! same way and overlap, so the depth buffer cannot decide between them and
//! the surface flickers as the camera moves.
//!
//! Use it on any mesh: [`triangles`] reads an Engine mesh, [`find`] reports
//! the clashes. Clashes between different colours are visible flicker;
//! clashes between faces of the same colour are only doubled geometry, so
//! they are reported apart ([`Clash::same_color`]).
//!
//! The `zfight` tool runs it on a whole generated city, the test
//! `tests/no_zfighting.rs` keeps the city clean, and `HW_ZFIGHT=1` marks
//! clashes in the running game with magenta posts.

use engine::BuiltMesh;
use glam::{DVec2, DVec3};
use std::collections::{BTreeMap, HashMap};

/// One triangle with its colour and where it came from.
#[derive(Clone, Copy, Debug)]
pub struct Tri {
    pub p: [DVec3; 3],
    /// sRGB 0..255 (the mesh's linear colours converted back).
    pub color: [u8; 3],
    /// Caller's label (for example the building index).
    pub source: u32,
}

/// Two overlapping coplanar triangles.
#[derive(Clone, Copy, Debug)]
pub struct Clash {
    /// Middle of the overlap (world position).
    pub at: DVec3,
    pub normal: DVec3,
    /// Overlapping area (m²).
    pub area: f64,
    /// Distance between the two planes (m).
    pub gap: f64,
    pub colors: [[u8; 3]; 2],
    pub sources: [u32; 2],
    /// Indices of the two triangles in the input.
    pub tris: [usize; 2],
    pub same_color: bool,
}

/// How close is "the same plane", and how big an overlap counts.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Planes nearer than this (m) fight. Engine's reversed-Z depth resolves
    /// a few millimetres close up, far less at a distance; 1 cm is safe.
    pub gap: f64,
    /// Overlaps smaller than this (m²) are ignored (edges that only touch).
    pub min_area: f64,
    /// Overlaps thinner than this (m) are ignored too: long edges that only
    /// touch, off by float rounding far from the origin. Far below a pixel.
    pub min_width: f64,
    /// Downward faces at or below this height are skipped: the bottoms of
    /// things standing on the ground, which nobody sees.
    pub hidden_below: f64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            gap: 0.01,
            min_area: 1e-3,
            min_width: 1e-3,
            hidden_below: f64::NEG_INFINITY,
        }
    }
}

/// The triangles of an Engine mesh (opaque and transparent), labelled `source`.
pub fn triangles(mesh: &BuiltMesh, source: u32) -> Vec<Tri> {
    let to_srgb = |c: f32| {
        let c = c.clamp(0.0, 1.0);
        let s = if c <= 0.003_130_8 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        };
        (s * 255.0).round() as u8
    };
    mesh.indices
        .chunks_exact(3)
        .map(|ix| {
            let v = |k: usize| mesh.positions[ix[k] as usize].as_dvec3();
            let c = mesh
                .colors
                .get(ix[0] as usize)
                .copied()
                .unwrap_or(glam::Vec4::ONE);
            Tri {
                p: [v(0), v(1), v(2)],
                color: [to_srgb(c.x), to_srgb(c.y), to_srgb(c.z)],
                source,
            }
        })
        .collect()
}

struct Prepared {
    n: DVec3,
    d: f64,
    index: usize,
}

/// All clashes among `tris`, biggest overlap first.
pub fn find(tris: &[Tri], opt: &Options) -> Vec<Clash> {
    // Group by facing direction (normals rounded to about 0.1°), then by
    // plane distance, then by position inside the plane.
    let mut by_normal: BTreeMap<[i32; 3], Vec<Prepared>> = BTreeMap::new();
    for (index, t) in tris.iter().enumerate() {
        let c = (t.p[1] - t.p[0]).cross(t.p[2] - t.p[0]);
        let len = c.length();
        if len < 1e-9 {
            continue;
        }
        let n = c / len;
        if n.y < -0.5 && t.p.iter().all(|p| p.y <= opt.hidden_below) {
            continue;
        }
        let key = (n * 512.0).round().as_ivec3().to_array();
        by_normal.entry(key).or_default().push(Prepared {
            n,
            d: n.dot(t.p[0]),
            index,
        });
    }
    let mut out = Vec::new();
    for group in by_normal.values_mut() {
        group.sort_by(|a, b| a.d.total_cmp(&b.d));
        // Runs of planes chained within `gap` of each other.
        let mut start = 0;
        while start < group.len() {
            let mut end = start + 1;
            // Plane distances from the origin carry rounding error far from
            // it, so runs are cut loosely; pairs are measured exactly below.
            while end < group.len() && group[end].d - group[end - 1].d <= opt.gap + 0.05 {
                end += 1;
            }
            if end - start > 1 {
                clashes_in_run(tris, &group[start..end], opt, &mut out);
            }
            start = end;
        }
    }
    out.sort_by(|a, b| b.area.total_cmp(&a.area));
    out
}

/// Pairs within one run of nearly coplanar triangles, found through a grid
/// in the plane.
fn clashes_in_run(tris: &[Tri], run: &[Prepared], opt: &Options, out: &mut Vec<Clash>) {
    let n = run[0].n;
    let u = n.any_orthonormal_vector();
    let v = n.cross(u);
    // Work relative to a point of the run: precise far from the origin.
    let o = tris[run[0].index].p[0];
    let flat: Vec<[DVec2; 3]> = run
        .iter()
        .map(|r| {
            let mut q = tris[r.index]
                .p
                .map(|p| DVec2::new((p - o).dot(u), (p - o).dot(v)));
            if cross2(q[1] - q[0], q[2] - q[0]) < 0.0 {
                q.swap(1, 2);
            }
            q
        })
        .collect();
    let bounds: Vec<(DVec2, DVec2)> = flat
        .iter()
        .map(|q| (q[0].min(q[1]).min(q[2]), q[0].max(q[1]).max(q[2])))
        .collect();
    const CELL: f64 = 8.0;
    let cell = |x: f64| (x / CELL).floor() as i64;
    let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (k, (lo, hi)) in bounds.iter().enumerate() {
        for cx in cell(lo.x)..=cell(hi.x) {
            for cy in cell(lo.y)..=cell(hi.y) {
                grid.entry((cx, cy)).or_default().push(k);
            }
        }
    }
    for ((cx, cy), members) in &grid {
        for (a_pos, &a) in members.iter().enumerate() {
            for &b in &members[a_pos + 1..] {
                let (la, ha) = bounds[a];
                let (lb, hb) = bounds[b];
                let lo = la.max(lb);
                let hi = ha.min(hb);
                if lo.x >= hi.x || lo.y >= hi.y {
                    continue;
                }
                // Test each pair once: in the cell holding the overlap's corner.
                if (cell(lo.x), cell(lo.y)) != (*cx, *cy) {
                    continue;
                }
                let gap = plane_gap(&tris[run[a].index], run[a].n, &tris[run[b].index]).min(
                    plane_gap(&tris[run[b].index], run[b].n, &tris[run[a].index]),
                );
                if gap > opt.gap {
                    continue;
                }
                let poly = clip(&flat[a], &flat[b]);
                let (area, centre) = area_and_centre(&poly);
                if area < opt.min_area || area < opt.min_width * longest_side(&poly) {
                    continue;
                }
                let (ta, tb) = (&tris[run[a].index], &tris[run[b].index]);
                let d = n.dot(ta.p[0] - o);
                out.push(Clash {
                    at: o + u * centre.x + v * centre.y + n * d,
                    normal: n,
                    area,
                    gap,
                    colors: [ta.color, tb.color],
                    sources: [ta.source, tb.source],
                    tris: [run[a].index, run[b].index],
                    same_color: ta.color == tb.color,
                });
            }
        }
    }
}

/// How far triangle `b` is from the plane of triangle `a` (normal `n`),
/// measured near the triangles, where it is precise.
fn plane_gap(a: &Tri, n: DVec3, b: &Tri) -> f64 {
    b.p.iter()
        .map(|p| n.dot(*p - a.p[0]).abs())
        .fold(0.0, f64::max)
}

/// The longest side of a polygon (an overlap's area over it is its width).
fn longest_side(poly: &[DVec2]) -> f64 {
    (0..poly.len())
        .map(|i| poly[i].distance(poly[(i + 1) % poly.len()]))
        .fold(0.0, f64::max)
}

fn cross2(a: DVec2, b: DVec2) -> f64 {
    a.x * b.y - a.y * b.x
}

/// The part of triangle `a` inside triangle `b` (both counter-clockwise).
fn clip(a: &[DVec2; 3], b: &[DVec2; 3]) -> Vec<DVec2> {
    let mut poly: Vec<DVec2> = a.to_vec();
    for k in 0..3 {
        let (e0, e1) = (b[k], b[(k + 1) % 3]);
        let inside = |p: DVec2| cross2(e1 - e0, p - e0) >= -1e-12;
        let mut next = Vec::with_capacity(poly.len() + 1);
        for i in 0..poly.len() {
            let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
            let (ip, iq) = (inside(p), inside(q));
            if ip {
                next.push(p);
            }
            if ip != iq {
                let dp = cross2(e1 - e0, p - e0);
                let dq = cross2(e1 - e0, q - e0);
                next.push(p + (q - p) * (dp / (dp - dq)));
            }
        }
        poly = next;
        if poly.len() < 3 {
            return Vec::new();
        }
    }
    poly
}

fn area_and_centre(poly: &[DVec2]) -> (f64, DVec2) {
    if poly.len() < 3 {
        return (0.0, DVec2::ZERO);
    }
    let mut area = 0.0;
    let mut c = DVec2::ZERO;
    for i in 0..poly.len() {
        let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
        let w = cross2(p, q);
        area += w;
        c += (p + q) * w;
    }
    if area.abs() < 1e-15 {
        return (0.0, poly[0]);
    }
    (area * 0.5, c / (3.0 * area))
}

/// A square of the map and a colour pair.
type SpotKey = (i64, i64, i64, [[u8; 3]; 2]);

/// Clashes grouped into spots: one per colour pair and square of `size`
/// metres, with the summed area, biggest first.
pub fn spots(clashes: &[Clash], size: f64) -> Vec<Clash> {
    let mut by: BTreeMap<SpotKey, Clash> = BTreeMap::new();
    for c in clashes {
        let mut colors = c.colors;
        colors.sort();
        let k = (c.at / size).floor().as_i64vec3();
        by.entry((k.x, k.y, k.z, colors))
            .and_modify(|s| s.area += c.area)
            .or_insert(Clash { colors, ..*c });
    }
    let mut out: Vec<Clash> = by.into_values().collect();
    out.sort_by(|a, b| b.area.total_cmp(&a.area));
    out
}

/// The options for a city: bottoms resting on the ground are hidden.
pub fn city_options() -> Options {
    Options {
        hidden_below: 0.5,
        ..Options::default()
    }
}

/// Source label of the ground mesh in [`city`].
pub const GROUND: u32 = u32::MAX;

/// Every triangle of a generated city as the game draws it at `detail`: the ground
/// (labelled [`GROUND`]) and each building (labelled with its index), closed,
/// or cut open at floor `cut` where it has one.
pub fn city(
    generated: &hw_sim::GeneratedCity,
    cut: Option<u32>,
    detail: crate::buildingmesh::Detail,
) -> Vec<Tri> {
    use crate::citymesh::{add_building, ground_mesh, Look, MeshBuilder};
    let plan = &generated.plan;
    let mut out = triangles(&ground_mesh(plan).build(), GROUND);
    for (i, b) in generated.buildings.iter().enumerate() {
        let lot = plan.buildings[i].0;
        let kind = plan.districts[plan.blocks[plan.lots[lot].block].district].kind;
        let look = match cut {
            Some(level) if (level as usize) < b.floors.len() => Look::Cut { level },
            _ => Look::Shell,
        };
        let mut m = MeshBuilder::new();
        add_building(&mut m, b, kind, look, detail);
        out.extend(triangles(&m.finish().build(), i as u32));
    }
    out
}

/// A label for a source in [`city`].
pub fn source_name(source: u32) -> String {
    if source == GROUND {
        "ground".into()
    } else {
        format!("building {source}")
    }
}

/// Adds a magenta post at every z-fighting spot in `mesh` (for
/// `HW_ZFIGHT=1`) and returns how many spots there were.
pub fn mark(mesh: &mut engine::Mesh) -> usize {
    let tris = triangles(&mesh.build(), 0);
    let fighting: Vec<Clash> = find(&tris, &city_options())
        .into_iter()
        .filter(|c| !c.same_color)
        .collect();
    let spots = spots(&fighting, 2.0);
    let magenta = engine::prelude::rgb(255, 0, 255);
    for s in &spots {
        let p = s.at.as_vec3();
        let _ = mesh.add_box((p.x, p.y + 1.5, p.z), (0.3, 3.0, 0.3), magenta);
    }
    spots.len()
}

pub fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(y: f64, x0: f64, x1: f64, color: [u8; 3], source: u32) -> [Tri; 2] {
        let p = |x: f64, z: f64| DVec3::new(x, y, z);
        [
            Tri {
                p: [p(x0, 0.0), p(x0, 1.0), p(x1, 1.0)],
                color,
                source,
            },
            Tri {
                p: [p(x0, 0.0), p(x1, 1.0), p(x1, 0.0)],
                color,
                source,
            },
        ]
    }

    #[test]
    fn overlapping_coplanar_faces_clash() {
        let mut t = quad(0.0, 0.0, 2.0, [255, 0, 0], 0).to_vec();
        t.extend(quad(0.002, 1.0, 3.0, [0, 0, 255], 1));
        let c = find(&t, &Options::default());
        let area: f64 = c.iter().map(|c| c.area).sum();
        assert!((area - 1.0).abs() < 1e-9, "overlap {area}");
        assert!(c
            .iter()
            .all(|c| !c.same_color && c.sources == [0, 1] || c.sources == [1, 0]));
        let s = spots(&c, 10.0);
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn rounding_slivers_along_long_edges_do_not() {
        // Two 50 m strips sharing an edge, off by 0.1 mm: 0.005 m², but
        // only a hair wide.
        let strip = |x0: f64, x1: f64, color, source| {
            let p = |x: f64, z: f64| DVec3::new(x, 0.0, z);
            [
                Tri {
                    p: [p(x0, 0.0), p(x0, 50.0), p(x1, 50.0)],
                    color,
                    source,
                },
                Tri {
                    p: [p(x0, 0.0), p(x1, 50.0), p(x1, 0.0)],
                    color,
                    source,
                },
            ]
        };
        let mut t = strip(0.0, 1.0001, [255, 0, 0], 0).to_vec();
        t.extend(strip(1.0, 2.0, [0, 0, 255], 1));
        assert!(find(&t, &Options::default()).is_empty());
    }

    #[test]
    fn touching_separated_or_opposite_faces_do_not() {
        // Side by side.
        let mut t = quad(0.0, 0.0, 1.0, [255, 0, 0], 0).to_vec();
        t.extend(quad(0.0, 1.0, 2.0, [0, 0, 255], 1));
        // 5 cm above.
        t.extend(quad(0.05, 0.0, 2.0, [0, 255, 0], 2));
        // Same plane, facing down (the bottom of something resting here).
        let mut down = quad(0.0, 0.0, 2.0, [9, 9, 9], 3);
        for tri in &mut down {
            tri.p.swap(1, 2);
        }
        t.extend(down);
        assert!(find(&t, &Options::default()).is_empty());
    }
}
