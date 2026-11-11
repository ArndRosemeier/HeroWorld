//! The city's z-fighting check, on Engine's finder ([`engine::zfight`]).
//!
//! The `zfight` tool runs it on a whole generated city and the test
//! `tests/no_zfighting.rs` keeps the city clean. In the running game,
//! `ENGINE_ZFIGHT=1` marks clashes around the camera with magenta posts.

pub use engine::zfight::{hex, Clash, Options, ZFightCheck, ZFightReport};

/// The options for a city: stricter than Engine's default (1 cm), and the
/// bottoms of things resting on the ground (or the water) are hidden.
pub fn city_options() -> Options {
    Options {
        gap: 0.01,
        ..Options::ground_at(0.0)
    }
}

/// Every triangle of a generated city as the game draws it at `detail`: the
/// ground (named "ground") and each building ("building N"), closed, or cut
/// open at floor `cut` where it has one.
pub fn city(
    generated: &hw_sim::GeneratedCity,
    cut: Option<u32>,
    detail: crate::buildingmesh::Detail,
) -> ZFightCheck {
    use crate::citymesh::{add_building, ground_mesh, Look, MeshBuilder};
    let plan = &generated.plan;
    let mut check = ZFightCheck::new().options(city_options());
    // The water and its bed lie metres below everything and only under
    // each other; leaving them out keeps the check fast.
    for (name, mesh) in ground_mesh(plan) {
        if name == "ground" {
            check = check.with(name, &mesh);
        }
    }
    for (i, b) in generated.buildings.iter().enumerate() {
        let lot = plan.buildings[i].0;
        let kind = plan.districts[plan.blocks[plan.lots[lot].block].district].kind;
        let look = match cut {
            Some(level) if (level as usize) < b.floors.len() => Look::Cut { level },
            _ => Look::Shell,
        };
        let mut m = MeshBuilder::new();
        add_building(&mut m, b, kind, look, detail);
        check = check.with(format!("building {i}"), &m.finish());
    }
    check
}

/// The streets of a town checked square by square ([`TILE`]): every road
/// and road-end plate reaching into a square, as the game draws them
/// ([`crate::townmesh::road_meshes`]). Clashes found in two squares are
/// counted once.
pub fn town_roads(world: &hw_gen::urban::world::TownWorld) -> ZFightReport {
    use glam::DVec2;
    let half = world.half();
    let mut out = ZFightReport {
        triangles: 0,
        clashes: Vec::new(),
        sources: Vec::new(),
        notes: Vec::new(),
        elapsed: std::time::Duration::ZERO,
    };
    let mut seen = std::collections::BTreeSet::new();
    let n = (2.0 * half / TILE).ceil().max(1.0) as i64;
    for tx in 0..n {
        for tz in 0..n {
            let t0 = DVec2::splat(-half) + DVec2::new(tx as f64, tz as f64) * TILE;
            let t1 = t0 + DVec2::splat(TILE);
            let mut check = ZFightCheck::new().options(city_options());
            for (name, mesh) in crate::townmesh::road_meshes(world, t0, t1) {
                check.add_built(name, &within(&mesh.build(), t0, t1), glam::Mat4::IDENTITY);
            }
            let r = check.run();
            merge(&mut out, &mut seen, r);
        }
    }
    out.clashes.sort_by(|a, b| b.area.total_cmp(&a.area));
    out.notes.sort();
    out.notes.dedup();
    out
}

/// Clash position, the names of both sources and their colours: a clash
/// found again in a neighbouring square has the same key.
type ClashKey = ([i32; 3], [String; 2], [[u8; 3]; 2]);

/// Adds `r` to `out`, leaving out clashes already in it.
fn merge(out: &mut ZFightReport, seen: &mut std::collections::BTreeSet<ClashKey>, r: ZFightReport) {
    out.triangles += r.triangles;
    out.elapsed += r.elapsed;
    out.notes.extend(r.notes.iter().cloned());
    for c in &r.clashes {
        let names = [r.source_name(c.sources[0]), r.source_name(c.sources[1])];
        let key = (
            (c.at * 1000.0).round().as_ivec3().to_array(),
            names.map(str::to_string),
            c.colors,
        );
        if !seen.insert(key) {
            continue;
        }
        let mut c = *c;
        c.sources = names.map(|n| match out.sources.iter().position(|s| s == n) {
            Some(k) => k as u32,
            None => {
                out.sources.push(n.to_string());
                (out.sources.len() - 1) as u32
            }
        });
        out.clashes.push(c);
    }
}

/// Side of the squares [`city_report`] checks the city in (metres).
pub const TILE: f64 = 300.0;

/// The whole city checked as [`city`] would, square by square: each square
/// takes every building that reaches into it, so any two buildings that
/// touch meet in some square and nothing is left unchecked. A furnished
/// city cut open is tens of millions of triangles, more than fits in
/// memory at once. Clashes found in two squares are counted once; the
/// triangle count is of every square's triangles.
pub fn city_report(
    generated: &hw_sim::GeneratedCity,
    cut: Option<u32>,
    detail: crate::buildingmesh::Detail,
) -> ZFightReport {
    use crate::citymesh::{add_building, ground_mesh, Look, MeshBuilder};
    use glam::DVec2;
    let plan = &generated.plan;
    let boxes: Vec<(DVec2, DVec2)> = generated
        .buildings
        .iter()
        .map(|b| {
            let s = &b.site;
            let corners = [
                s.origin,
                s.origin + s.along * s.size.x,
                s.origin + s.inward * s.size.y,
                s.origin + s.along * s.size.x + s.inward * s.size.y,
            ];
            let lo = corners
                .iter()
                .fold(DVec2::splat(f64::INFINITY), |a, c| a.min(*c));
            let hi = corners
                .iter()
                .fold(DVec2::splat(f64::NEG_INFINITY), |a, c| a.max(*c));
            // A little more for what stands proud of the walls (cornices,
            // awnings, signs).
            (lo - DVec2::splat(3.0), hi + DVec2::splat(3.0))
        })
        .collect();
    let (lo, hi) = boxes.iter().fold(
        (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
        |(a, b), (l, h)| (a.min(*l), b.max(*h)),
    );
    // The ground is built once and each square takes its own triangles
    // (and those reaching into it): the whole ground in every square was
    // most of the work.
    let ground: Vec<engine::mesh::BuiltMesh> = ground_mesh(plan)
        .into_iter()
        .filter(|(name, _)| name == "ground")
        .map(|(_, m)| m.build())
        .collect();
    let mut out = ZFightReport {
        triangles: 0,
        clashes: Vec::new(),
        sources: Vec::new(),
        notes: Vec::new(),
        elapsed: std::time::Duration::ZERO,
    };
    let mut seen = std::collections::BTreeSet::new();
    let (nx, nz) = (
        ((hi.x - lo.x) / TILE).ceil().max(1.0) as i64,
        ((hi.y - lo.y) / TILE).ceil().max(1.0) as i64,
    );
    for tx in 0..nx {
        for tz in 0..nz {
            let t0 = lo + DVec2::new(tx as f64, tz as f64) * TILE;
            let t1 = t0 + DVec2::splat(TILE);
            let inside: Vec<usize> = (0..boxes.len())
                .filter(|&i| {
                    let (l, h) = boxes[i];
                    l.x <= t1.x && h.x >= t0.x && l.y <= t1.y && h.y >= t0.y
                })
                .collect();
            if inside.is_empty() {
                continue;
            }
            let mut check = ZFightCheck::new().options(city_options());
            for g in &ground {
                check.add_built(
                    "ground",
                    &within(g, t0 - 1.0, t1 + 1.0),
                    glam::Mat4::IDENTITY,
                );
            }
            for i in inside {
                let b = &generated.buildings[i];
                let lot = plan.buildings[i].0;
                let kind = plan.districts[plan.blocks[plan.lots[lot].block].district].kind;
                let look = match cut {
                    Some(level) if (level as usize) < b.floors.len() => Look::Cut { level },
                    _ => Look::Shell,
                };
                let mut m = MeshBuilder::new();
                add_building(&mut m, b, kind, look, detail);
                check = check.with(format!("building {i}"), &m.finish());
            }
            let r = check.run();
            merge(&mut out, &mut seen, r);
        }
    }
    out.clashes.sort_by(|a, b| b.area.total_cmp(&a.area));
    out.notes.sort();
    out.notes.dedup();
    out
}

/// The triangles of `mesh` that reach into the square `lo`..`hi` (x, z).
fn within(
    mesh: &engine::mesh::BuiltMesh,
    lo: glam::DVec2,
    hi: glam::DVec2,
) -> engine::mesh::BuiltMesh {
    let keep = |ix: &[u32; 3]| {
        let p = ix.map(|i| mesh.positions[i as usize]);
        let (x0, x1) = (
            p.iter().map(|v| v.x).fold(f32::INFINITY, f32::min),
            p.iter().map(|v| v.x).fold(f32::NEG_INFINITY, f32::max),
        );
        let (z0, z1) = (
            p.iter().map(|v| v.z).fold(f32::INFINITY, f32::min),
            p.iter().map(|v| v.z).fold(f32::NEG_INFINITY, f32::max),
        );
        x1 as f64 >= lo.x && x0 as f64 <= hi.x && z1 as f64 >= lo.y && z0 as f64 <= hi.y
    };
    let (opaque, blended) = mesh
        .indices
        .split_at(mesh.opaque_index_count.min(mesh.indices.len()));
    let pick = |part: &[u32]| -> Vec<u32> {
        part.as_chunks::<3>()
            .0
            .iter()
            .filter(|ix| ix.iter().all(|&i| (i as usize) < mesh.positions.len()) && keep(ix))
            .flatten()
            .copied()
            .collect()
    };
    let mut indices = pick(opaque);
    let opaque_index_count = indices.len();
    indices.extend(pick(blended));
    engine::mesh::BuiltMesh {
        indices,
        opaque_index_count,
        ..mesh.clone()
    }
}
