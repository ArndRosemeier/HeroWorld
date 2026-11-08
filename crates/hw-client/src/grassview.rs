//! Grass blades around the camera: clumps of real blades on every lawn,
//! verge, park and meadow near by, swaying in the wind.
//!
//! The textured lawn ([`crate::lawnmesh`]) carries the grass to the horizon;
//! close up it would still look painted, so clumps of blades stand on it
//! out to about [`FAR`] metres. Each kind of grass ([`Sward`]) has a few
//! clump models: short mown blades for lawns, a park's longer grass with
//! clover and daisies, meadow grass with seed heads and wildflowers, dry
//! tufts for the works' yards. Every model has three levels (all blades
//! close up, fewer and wider ones further out, a few broad ones at the
//! edge) and is drawn instanced; clumps are placed in square tiles that
//! come and go with the camera, and cast no shadows.
//!
//! The blades use Engine's vegetation surface (the far-canopy profile with
//! a per-vertex wind weight), so they bend in the world's wind like the
//! trees.

use engine::prelude::*;
use glam::{DVec2, Mat3, Vec3};
use hw_core::Seed;
use hw_gen::style::Region;
use hw_gen::urban::greenery::{Greenery, Sward};
use hw_gen::urban::world::TownWorld;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Clump models per kind of grass.
const VARIANTS: u8 = 4;
/// Edge of a tile of clumps (m).
const TILE: f64 = 12.0;
/// Clumps stand on a jittered grid this far apart (m).
const SPACING: f64 = 0.72;
/// Detail levels end at these distances (m, at LOD bias 1).
const NEAR: f32 = 10.0;
const MID: f32 = 24.0;
pub const FAR: f32 = 46.0;
/// Clumps keep this far from a kerb or wall (m), so no blade pokes through.
const KEEP_CLEAR: f64 = 0.42;
/// Tiles built per update at most (the rest follow next frame).
const TILES_PER_UPDATE: usize = 6;

/// Shader profile of Engine's far vegetation hulls: lit vertex colour,
/// swaying with a per-vertex wind weight (`strata_strength`).
const PROFILE_VEG_HULL: f32 = 10.0;

type Tile = (i64, i64);

fn tile_of(p: DVec2) -> Tile {
    ((p.x / TILE).floor() as i64, (p.y / TILE).floor() as i64)
}

/// A model's index: kind and variant.
fn model_index(s: Sward, v: u8) -> usize {
    let k = Sward::ALL.iter().position(|x| *x == s).unwrap_or(0);
    k * VARIANTS as usize + v as usize
}

/// The grass blades around the camera.
pub struct GrassView {
    world: Arc<TownWorld>,
    green: Arc<Greenery>,
    /// One LOD set per model.
    sets: Vec<LodId>,
    /// Built tiles: per model, the clumps' places.
    tiles: BTreeMap<Tile, Vec<(usize, Place)>>,
    seed: Seed,
    pub enabled: bool,
}

impl GrassView {
    pub fn spawn(
        world: &mut World,
        town: Arc<TownWorld>,
        green: Arc<Greenery>,
    ) -> EngineResult<Self> {
        let region = green.region;
        let mut sets = Vec::new();
        for s in Sward::ALL {
            for v in 0..VARIANTS {
                let levels = clump_levels(s, v, region)?;
                let id = world.spawn_lod(levels)?;
                for e in world.lod_levels(id)? {
                    world.set_casts_shadow(e, false)?;
                }
                sets.push(id);
            }
        }
        let seed = town.seed.child("grass-blades");
        Ok(Self {
            world: town,
            green,
            sets,
            tiles: BTreeMap::new(),
            seed,
            // `HW_GRASS=0` leaves the blades out (to compare frame rates).
            enabled: std::env::var("HW_GRASS").map_or(true, |v| v != "0"),
        })
    }

    /// Takes the grass out of the world.
    pub fn despawn(self, world: &mut World) {
        for id in self.sets {
            world.despawn_lod(id);
        }
    }

    /// Builds the tiles coming into range around `eye` and drops those
    /// going out of it.
    pub fn update(&mut self, world: &mut World, eye: Vec3) -> EngineResult<()> {
        let reach = if self.enabled {
            (FAR * world.lod_bias().max(0.2)) as f64 + TILE * 0.75
        } else {
            0.0
        };
        let e = DVec2::new(eye.x as f64, eye.z as f64);
        // High above the ground the blades are too small to see.
        let above = eye.y as f64 - self.world.ground(e);
        let reach = if above > FAR as f64 { 0.0 } else { reach };
        let centre = |t: Tile| DVec2::new((t.0 as f64 + 0.5) * TILE, (t.1 as f64 + 0.5) * TILE);
        let mut changed = BTreeSet::new();
        let gone: Vec<Tile> = self
            .tiles
            .keys()
            .copied()
            .filter(|t| centre(*t).distance(e) > reach + TILE)
            .collect();
        for t in gone {
            for (m, _) in self.tiles.remove(&t).unwrap_or_default() {
                changed.insert(m);
            }
        }
        let (a, b) = (
            tile_of(e - DVec2::splat(reach)),
            tile_of(e + DVec2::splat(reach)),
        );
        let mut want: Vec<(f64, Tile)> = (a.0..=b.0)
            .flat_map(|x| (a.1..=b.1).map(move |z| (x, z)))
            .filter(|t| !self.tiles.contains_key(t))
            .map(|t| (centre(t).distance(e), t))
            .filter(|(d, _)| *d < reach)
            .collect();
        want.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
        for (_, t) in want.into_iter().take(TILES_PER_UPDATE) {
            let clumps = self.tile_clumps(t);
            for (m, _) in &clumps {
                changed.insert(*m);
            }
            self.tiles.insert(t, clumps);
        }
        if changed.is_empty() {
            return Ok(());
        }
        let mut places: Vec<Vec<Place>> = vec![Vec::new(); self.sets.len()];
        for clumps in self.tiles.values() {
            for (m, p) in clumps {
                if changed.contains(m) {
                    places[*m].push(*p);
                }
            }
        }
        for m in changed {
            world.set_lod_places(self.sets[m], &places[m])?;
        }
        Ok(())
    }

    /// Forgets every tile (after the ground changed, or to rebuild).
    pub fn clear(&mut self, world: &mut World) -> EngineResult<()> {
        self.tiles.clear();
        for &id in &self.sets {
            world.set_lod_places(id, &[])?;
        }
        Ok(())
    }

    /// Clumps in one tile.
    fn tile_clumps(&self, t: Tile) -> Vec<(usize, Place)> {
        let (w, g) = (&*self.world, &*self.green);
        let lo = DVec2::new(t.0 as f64, t.1 as f64) * TILE;
        let n = (TILE / SPACING).round() as i64;
        let mut out = Vec::new();
        // The kind of grass, per quarter of the tile.
        let q = TILE * 0.5;
        let swards: Vec<Sward> = (0..4)
            .map(|k| {
                let c = lo + DVec2::new((k % 2) as f64 + 0.5, (k / 2) as f64 + 0.5) * q;
                g.sward_at(w, c)
            })
            .collect();
        let tile_seed = self.seed.cell(t.0, t.1);
        let mut rng = tile_seed.rng();
        for j in 0..n {
            for i in 0..n {
                let p = lo
                    + DVec2::new(
                        (i as f64 + 0.5 + rng.range(-0.38, 0.38)) * SPACING,
                        (j as f64 + 0.5 + rng.range(-0.38, 0.38)) * SPACING,
                    );
                let (u, v_pick, yaw, size) = (
                    rng.unit(),
                    rng.unit(),
                    rng.range(0.0, 360.0),
                    rng.range(0.8, 1.2),
                );
                let k =
                    (((p.x - lo.x) / q) as usize).min(1) + 2 * (((p.y - lo.y) / q) as usize).min(1);
                let sward = swards[k];
                let Some(grass) = g.grass(w, p, sward) else {
                    continue;
                };
                if u > grass.density || g.clearance(w, p) < KEEP_CLEAR {
                    continue;
                }
                // Flowers and clover are the rarer variants.
                let v = match v_pick {
                    x if x < 0.36 => 0,
                    x if x < 0.72 => 1,
                    x if x < 0.92 => 2,
                    _ => 3,
                };
                let m = model_index(sward, v);
                let h = w.ground(p);
                // Tilt the clump onto the slope.
                let n = crate::lawnmesh::ground_normal(w, p).as_vec3();
                let local = Mat3::from_rotation_y(-(yaw as f32).to_radians()) * n;
                let pitch = local.z.atan2(local.y).to_degrees();
                let roll = (-local.x).clamp(-1.0, 1.0).asin().to_degrees();
                // Thin grass is short too.
                let s = size * (0.65 + 0.35 * grass.density);
                out.push((
                    m,
                    Place {
                        position: Vec3::new(p.x as f32, (h + 0.02) as f32, p.y as f32),
                        yaw_degrees: yaw as f32,
                        pitch_degrees: pitch,
                        roll_degrees: roll,
                        scale: s as f32,
                        ..Place::default()
                    },
                ));
            }
        }
        out
    }
}

// ------------------------------------------------------------- models

/// A mesh whose every point carries its own wind weight.
struct Veg {
    mesh: Mesh,
    /// Mirrors `mesh`'s points one for one, to hand out their ids.
    ids: Mesh,
}

impl Veg {
    fn new() -> Self {
        Self {
            mesh: Mesh::new(),
            ids: Mesh::new(),
        }
    }

    /// A point swaying with weight `wind` (0 root, 1 a long stem's tip).
    fn point(&mut self, p: Vec3, n: Vec3, c: Color, wind: f32) -> PointId {
        // Engine keeps one surface per mesh through its public calls, but
        // appending a mesh carries that mesh's surface over per point.
        let mut one = Mesh::new();
        one.set_surface_material(SurfaceMaterial {
            roughness: 0.82,
            strata_strength: wind.clamp(0.0, 1.0),
            // Far-hull kind 0: plain lit colour, no dissolving rim.
            noise_gain: 0.0,
            profile: PROFILE_VEG_HULL,
            ..SurfaceMaterial::DEFAULT
        });
        let id = one.add_point(p).expect("finite point");
        one.set_point_color(id, c).expect("point");
        one.set_point_normal(id, n).expect("point");
        self.mesh
            .append_translated(&one, Vec3::ZERO)
            .expect("append point");
        // (Engine asked for a per-point surface call: ENGINE_REQUESTS.md.)
        self.ids.add_point(p).expect("finite point")
    }

    /// A triangle seen from both sides.
    fn tri2(&mut self, a: PointId, b: PointId, c: PointId) {
        self.mesh.add_triangle(a, b, c).expect("tri");
        self.mesh.add_triangle(a, c, b).expect("tri");
    }
}

/// How a vertex `y` metres up sways: a lawn's tips a little, a meadow's
/// stems a lot.
fn wind_at(y: f32) -> f32 {
    (y.max(0.0) * 0.9).sqrt().min(1.0)
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: 1.0,
    }
}

fn scale(c: Color, k: f32) -> Color {
    Color {
        r: (c.r * k).min(1.0),
        g: (c.g * k).min(1.0),
        b: (c.b * k).min(1.0),
        a: 1.0,
    }
}

/// How the blades of one kind of grass grow.
struct Growth {
    /// Blades in the full clump.
    blades: usize,
    /// Clump radius (m).
    radius: f32,
    /// Blade height range (m).
    height: (f32, f32),
    /// Blade width at the root (m).
    width: f32,
    /// How far blades lean over (tip offset over height).
    lean: (f32, f32),
    /// Share of blades turned straw-coloured.
    dry: f32,
}

fn growth(s: Sward) -> Growth {
    match s {
        Sward::Lawn => Growth {
            blades: 84,
            radius: 0.42,
            height: (0.05, 0.11),
            width: 0.012,
            lean: (0.1, 0.4),
            dry: 0.05,
        },
        Sward::Park => Growth {
            blades: 72,
            radius: 0.42,
            height: (0.07, 0.17),
            width: 0.013,
            lean: (0.15, 0.5),
            dry: 0.08,
        },
        Sward::Meadow => Growth {
            blades: 56,
            radius: 0.4,
            height: (0.22, 0.55),
            width: 0.013,
            lean: (0.2, 0.6),
            dry: 0.22,
        },
        Sward::Rough => Growth {
            blades: 30,
            radius: 0.22,
            height: (0.1, 0.32),
            width: 0.01,
            lean: (0.3, 0.8),
            dry: 0.45,
        },
    }
}

/// The three levels of one clump model.
fn clump_levels(s: Sward, variant: u8, region: Region) -> EngineResult<Vec<LodLevel>> {
    Ok(vec![
        LodLevel::new(clump(s, variant, region, 0), NEAR),
        LodLevel::new(clump(s, variant, region, 1), MID),
        LodLevel::new(clump(s, variant, region, 2), FAR),
    ])
}

/// One clump at detail `lod` (0 full, 1 mid, 2 far).
fn clump(s: Sward, variant: u8, region: Region, lod: u8) -> Mesh {
    let gr = growth(s);
    let base = crate::lawnmesh::sward_colour(s, region);
    let base = Color {
        r: base[0],
        g: base[1],
        b: base[2],
        a: 1.0,
    };
    let straw = match s {
        Sward::Meadow | Sward::Rough => Color::rgb(176, 158, 98),
        _ => Color::rgb(150, 146, 84),
    };
    // The same blades at every level (one seed), fewer of them further out.
    let mut rng = Seed(0x6a55).child(model_index(s, variant) as u32).rng();
    let (keep, widen, rows): (usize, f32, &[f32]) = match lod {
        0 => (1, 1.0, &[0.0, 0.38, 0.72, 1.0]),
        1 => (3, 1.75, &[0.0, 0.55, 1.0]),
        _ => (7, 3.2, &[0.0, 1.0]),
    };
    let mut veg = Veg::new();
    for b in 0..gr.blades {
        // Every blade draws the same numbers whether kept or not.
        let r = gr.radius * (rng.unit() as f32).sqrt();
        let a = rng.range(0.0, std::f64::consts::TAU) as f32;
        let h = rng.range(gr.height.0 as f64, gr.height.1 as f64) as f32;
        let lean = rng.range(gr.lean.0 as f64, gr.lean.1 as f64) as f32;
        let la = rng.range(0.0, std::f64::consts::TAU) as f32;
        let tone = rng.range(0.78, 1.18) as f32;
        let dry = rng.chance(gr.dry as f64);
        if b % keep != 0 {
            continue;
        }
        let root = Vec3::new(r * a.cos(), 0.0, r * a.sin());
        let dir = Vec3::new(la.cos(), 0.0, la.sin());
        let side = Vec3::new(-dir.z, 0.0, dir.x);
        let w = gr.width * widen * if s == Sward::Meadow { 1.0 } else { 1.1 };
        let colour = if dry {
            scale(straw, tone)
        } else {
            scale(base, tone)
        };
        // Facing the way it leans, softened towards up so blades light
        // like the lawn under them.
        let face = (Vec3::Y * 0.4 - dir).normalize();
        let n = (Vec3::Y * 0.8 + face * 0.35).normalize();
        let at = |t: f32| {
            root + dir * (h * lean * t * t) + Vec3::Y * (h * (t - 0.18 * lean * lean * t * t))
        };
        let mut prev: Option<(PointId, PointId)> = None;
        for (k, &t) in rows.iter().enumerate() {
            let c = at(t);
            // Darker at the root, lighter and yellower at the tip.
            let col = mix(scale(colour, 0.6 + 0.5 * t), straw, 0.06 * t * t);
            if k + 1 == rows.len() {
                let tip = veg.point(c, n, col, wind_at(c.y));
                if let Some((l, rr)) = prev {
                    veg.tri2(l, rr, tip);
                }
            } else {
                let half = w * 0.5 * (1.0 - t.powf(1.4)).max(0.15);
                let l = veg.point(c - side * half, n, col, wind_at(c.y));
                let rr = veg.point(c + side * half, n, col, wind_at(c.y));
                if let Some((pl, pr)) = prev {
                    veg.tri2(pl, pr, rr);
                    veg.tri2(pl, rr, l);
                }
                prev = Some((l, rr));
            }
        }
    }
    // Clover and daisies in the park, seed heads and wildflowers in the
    // meadow, dandelions on the rough ground.
    let mut rng = Seed(0xf10e).child(model_index(s, variant) as u32).rng();
    let flowers: &[(Color, Color, f32)] = match (s, variant) {
        (Sward::Park, 3) => &[(Color::rgb(246, 244, 236), Color::rgb(236, 196, 40), 0.012)],
        (Sward::Park, 2) => &[(Color::rgb(232, 226, 236), Color::rgb(214, 170, 200), 0.009)],
        (Sward::Meadow, 3) => &[
            (Color::rgb(200, 28, 22), Color::rgb(30, 22, 18), 0.022),
            (Color::rgb(70, 96, 210), Color::rgb(40, 40, 120), 0.016),
            (Color::rgb(240, 236, 226), Color::rgb(232, 190, 40), 0.016),
        ],
        (Sward::Meadow, 2) => &[(Color::rgb(232, 196, 46), Color::rgb(200, 150, 30), 0.014)],
        (Sward::Rough, 3) => &[(Color::rgb(238, 200, 30), Color::rgb(220, 170, 20), 0.013)],
        (Sward::Lawn, 3) => &[(Color::rgb(246, 244, 236), Color::rgb(236, 196, 40), 0.008)],
        _ => &[],
    };
    if !flowers.is_empty() && lod < 2 {
        let count = match s {
            Sward::Meadow => 5,
            Sward::Park => 4,
            _ => 2,
        };
        for k in 0..count {
            let (petal, heart, size) = flowers[k % flowers.len()];
            let r = gr.radius * 0.8 * (rng.unit() as f32).sqrt();
            let a = rng.range(0.0, std::f64::consts::TAU) as f32;
            let top = gr.height.1 * rng.range(0.6, 1.05) as f32 + 0.02;
            let foot = Vec3::new(r * a.cos(), 0.0, r * a.sin());
            let head = foot
                + Vec3::new(
                    rng.range(-0.03, 0.03) as f32,
                    top,
                    rng.range(-0.03, 0.03) as f32,
                );
            flower(
                &mut veg,
                foot,
                head,
                petal,
                heart,
                size * if lod == 0 { 1.0 } else { 1.4 },
                lod == 0,
                base,
            );
        }
    }
    if s == Sward::Meadow && variant < 2 && lod < 2 {
        // Seed heads on tall stems.
        for _ in 0..4 {
            let r = gr.radius * 0.7 * (rng.unit() as f32).sqrt();
            let a = rng.range(0.0, std::f64::consts::TAU) as f32;
            let foot = Vec3::new(r * a.cos(), 0.0, r * a.sin());
            let top = gr.height.1 * rng.range(1.0, 1.35) as f32;
            let tilt = Vec3::new(
                rng.range(-0.08, 0.08) as f32,
                0.0,
                rng.range(-0.08, 0.08) as f32,
            );
            seed_head(&mut veg, foot, foot + tilt + Vec3::Y * top, straw, lod == 0);
        }
    }
    let mut mesh = veg.mesh;
    if mesh.face_count() == 0 {
        // A level must draw something: one tiny blade.
        let mut v = Veg::new();
        let a = v.point(Vec3::new(-0.005, 0.0, 0.0), Vec3::Y, base, 0.0);
        let b = v.point(Vec3::new(0.005, 0.0, 0.0), Vec3::Y, base, 0.0);
        let c = v.point(Vec3::new(0.0, 0.03, 0.0), Vec3::Y, base, 0.1);
        v.tri2(a, b, c);
        mesh = v.mesh;
    }
    mesh
}

/// A flower: a thin stem and a head of petals round a heart, facing up.
#[allow(clippy::too_many_arguments)]
fn flower(
    veg: &mut Veg,
    foot: Vec3,
    head: Vec3,
    petal: Color,
    heart: Color,
    size: f32,
    fine: bool,
    stem: Color,
) {
    let wf = wind_at(foot.y);
    let wh = wind_at(head.y);
    // Stem: one narrow strip.
    let d = (head - foot).normalize();
    let side = d.cross(Vec3::Z).normalize_or(Vec3::X) * 0.0025;
    let s0 = veg.point(foot - side, Vec3::Y, scale(stem, 0.6), wf);
    let s1 = veg.point(foot + side, Vec3::Y, scale(stem, 0.6), wf);
    let s2 = veg.point(head, Vec3::Y, stem, wh);
    veg.tri2(s0, s1, s2);
    // Petals: a ring, cupped a little.
    let n = if fine { 7 } else { 4 };
    let centre = veg.point(head + Vec3::Y * 0.003, Vec3::Y, heart, wh);
    let ring: Vec<PointId> = (0..n)
        .map(|k| {
            let a = k as f32 / n as f32 * std::f32::consts::TAU;
            let p = head + Vec3::new(a.cos() * size, size * 0.25, a.sin() * size);
            veg.point(p, Vec3::Y, petal, wh)
        })
        .collect();
    for k in 0..n {
        veg.tri2(centre, ring[(k + 1) % n], ring[k]);
    }
}

/// A grass seed head: a thin stem and a spindle of seeds.
fn seed_head(veg: &mut Veg, foot: Vec3, top: Vec3, straw: Color, fine: bool) {
    let d = (top - foot).normalize();
    let side = d.cross(Vec3::Z).normalize_or(Vec3::X);
    let w = 0.0022;
    let a = veg.point(foot - side * w, Vec3::Y, scale(straw, 0.55), 0.0);
    let b = veg.point(foot + side * w, Vec3::Y, scale(straw, 0.55), 0.0);
    let start = foot + (top - foot) * 0.78;
    let c = veg.point(start, Vec3::Y, straw, wind_at(start.y));
    veg.tri2(a, b, c);
    // The spindle, seen from both sides and crossed when close.
    let mid = (start + top) * 0.5;
    let sides: &[Vec3] = if fine {
        &[side, d.cross(side)]
    } else {
        &[side]
    };
    for sd in sides {
        let p0 = veg.point(start, Vec3::Y, straw, wind_at(start.y));
        let p1 = veg.point(
            mid - *sd * 0.012,
            Vec3::Y,
            scale(straw, 1.1),
            wind_at(mid.y),
        );
        let p2 = veg.point(top, Vec3::Y, scale(straw, 1.15), wind_at(top.y));
        let p3 = veg.point(
            mid + *sd * 0.012,
            Vec3::Y,
            scale(straw, 1.1),
            wind_at(mid.y),
        );
        veg.tri2(p0, p1, p2);
        veg.tri2(p0, p2, p3);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_clump_model_has_three_levels_with_fewer_triangles_further_out() {
        for s in Sward::ALL {
            for v in 0..VARIANTS {
                let levels = clump_levels(s, v, Region::Continental).unwrap();
                assert_eq!(levels.len(), 3);
                let tris: Vec<usize> = levels
                    .iter()
                    .map(|l| l.mesh.build().triangle_count())
                    .collect();
                assert!(
                    tris[0] > tris[1] && tris[1] >= tris[2],
                    "{s:?} {v}: {tris:?}"
                );
                assert!(tris[0] < 2_000, "{s:?} {v}: {} triangles close up", tris[0]);
            }
        }
    }
}
