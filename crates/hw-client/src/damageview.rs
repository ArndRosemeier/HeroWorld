//! Damage in the client: a building that takes its first hit leaves its town
//! cell and becomes an Engine breakable building in the same place
//! (`World::spawn_building`), drawn element by element with the building's
//! colours. Engine then plays what the server's broken bits mean: panels
//! vanishing, debris, dust, a floor pancaking or the top toppling, rubble
//! mounds. A repaired building goes back into its cell.
//!
//! Impacts get sparks, dust and a mark on the wall (Engine decals), an
//! energy blast a small explosion.
//!
//! A joined client also keeps its own copy of the damage for collision, so
//! its predicted hero walks through the same holes as on the server; in
//! single player the server's copy is already shared.

use crate::buildingmesh::{exterior, Detail};
use crate::citymesh::MeshBuilder;
use crate::townmesh::TownView;
use engine::building::{BuildingPlan as Breakable, ElementBits, ElementKind};
use engine::decals::{Decal, DecalKind};
use engine::particles::ParticleEmitter;
use engine::prelude::*;
use glam::{DVec2, DVec3};
use hw_gen::building::{BuildingPlan, OpeningKind, Side};
use hw_net::{ImpactKind, ToClient};
use hw_sim::collision::Obstacles;
use hw_sim::destruction::{self, Damage, Frame, Piece};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Seconds marks stay on walls.
const MARK_SECONDS: f32 = 120.0;

pub struct DamageView {
    /// Broken elements per building, as the server said.
    broken: BTreeMap<u64, BTreeSet<u32>>,
    /// Buildings drawn breakable, by Engine entity.
    shown: BTreeMap<u64, (EntityId, Arc<Breakable>)>,
    /// A joined client's copy of the damage, for collision (None in single
    /// player, where the server's is shared).
    mirror: Option<Damage>,
}

impl DamageView {
    /// `joined`: playing in someone else's world.
    pub fn new(joined: bool) -> Self {
        Self {
            broken: BTreeMap::new(),
            shown: BTreeMap::new(),
            mirror: joined.then(Damage::default),
        }
    }

    /// Buildings drawn breakable now.
    pub fn shown(&self) -> usize {
        self.shown.len()
    }

    /// Takes in the server's news.
    pub fn news(
        &mut self,
        world: &mut World,
        town: &mut TownView,
        solids: &Obstacles,
        news: Vec<ToClient>,
    ) {
        for msg in news {
            match msg {
                ToClient::Damage {
                    building,
                    broken,
                    live,
                } => {
                    let i = building;
                    if !town.has_building(i) {
                        continue;
                    }
                    if let Some(m) = self.mirror.as_mut() {
                        m.apply(solids, i, &broken, 0);
                    }
                    self.broken.entry(i).or_default().extend(broken);
                    if let Err(e) = self.show(world, town, solids, i, live) {
                        println!("Could not show the damage of building {i}: {e}");
                    }
                }
                ToClient::Repaired { building } => {
                    let i = building;
                    if let Some(m) = self.mirror.as_mut() {
                        m.repair(solids, i);
                        m.take_news();
                    }
                    self.broken.remove(&i);
                    if let Some((e, _)) = self.shown.remove(&i) {
                        world.despawn(e);
                    }
                    town.set_hidden(world, i, false);
                }
                ToClient::Impact {
                    at,
                    dir,
                    kind,
                    broke,
                } => {
                    let _ = impact(
                        world,
                        Vec3::from_array(at),
                        Vec3::from_array(dir),
                        kind,
                        broke,
                    );
                }
                _ => {}
            }
        }
    }

    /// Draws building `i` breakable with its broken bits; `animate`: play
    /// the breaking.
    fn show(
        &mut self,
        world: &mut World,
        town: &mut TownView,
        solids: &Obstacles,
        i: u64,
        animate: bool,
    ) -> EngineResult<()> {
        if let std::collections::btree_map::Entry::Vacant(slot) = self.shown.entry(i) {
            let Some((plan, base)) = solids.building(i) else {
                return Ok(());
            };
            let frame = Frame::of(&plan.site, base);
            let pieces = destruction::pieces(&plan);
            let (textures, mut looks) = element_meshes(&plan, &pieces, &frame)?;
            let textures: Vec<&Mesh> = textures.iter().collect();
            let bp = Arc::new(destruction::breakable_textured(
                &plan,
                &pieces,
                &frame,
                &textures,
                |i, _, _| looks.get_mut(i).map(std::mem::take).unwrap_or_default(),
            )?);
            let place = Place::new(frame.origin.x as f32, base as f32, frame.origin.y as f32)
                .with_yaw_deg(frame.yaw().to_degrees() as f32);
            let id = world.spawn_building(bp.clone(), place)?;
            world.set_name(id, format!("broken building {i}"));
            slot.insert((id, bp));
            town.set_hidden(world, i, true);
        }
        let Some((id, bp)) = self.shown.get(&i) else {
            return Ok(());
        };
        let mut bits = ElementBits::new(bp.len());
        for &k in self.broken.get(&i).into_iter().flatten() {
            if (k as usize) < bits.len() {
                bits.set(k as usize, true);
            }
        }
        world.apply_building_state(*id, &bits, animate)?;
        Ok(())
    }

    /// Forgets everything (before switching towns).
    pub fn clear(&mut self, world: &mut World) {
        for (_, (e, _)) in std::mem::take(&mut self.shown) {
            world.despawn(e);
        }
        self.broken.clear();
        if let Some(m) = self.mirror.as_mut() {
            *m = Damage::default();
        }
    }
}

fn boxed(center: Vec3, half: Vec3, c: Color) -> EngineResult<Mesh> {
    Mesh::box_at(center, half * 2.0, c)
}

/// Inside walls, slabs and columns.
const PLASTER: [u8; 3] = [226, 220, 206];
const CONCRETE: [u8; 3] = [176, 170, 160];

/// An element's meshes, each with the index of its texture (None: plain).
type Parts = Vec<(Mesh, Option<usize>)>;

/// Every element's look, by element, and the textures they use (meshes
/// carrying only an albedo and its maps). The outside is the town's own
/// detailed, textured mesh of the building (walls, windows, cornices,
/// roof), each triangle given to the outer element it belongs to, so the
/// building looks as it did before it broke; outer panels get a plastered
/// inside face (with the window holes), and inside walls, slabs and
/// columns plain boxes.
fn element_meshes(
    plan: &BuildingPlan,
    pieces: &[Piece],
    frame: &Frame,
) -> EngineResult<(Vec<Mesh>, Vec<Parts>)> {
    let mut out: Vec<Parts> = vec![Vec::new(); pieces.len()];
    let textures = facade(plan, pieces, frame, &mut out)?;
    for (i, p) in pieces.iter().enumerate() {
        let c = frame.to_engine((p.min + p.max) * 0.5);
        let h = ((p.max - p.min) * 0.5).as_vec3();
        let extra = match p.kind {
            ElementKind::Roof => None,
            ElementKind::Wall | ElementKind::Window if p.outward != DVec2::ZERO => {
                Some(backing(plan, frame, p)?)
            }
            ElementKind::Slab => Some(boxed(c, h - Vec3::new(0.004, 0.0, 0.004), rgb3(CONCRETE))?),
            ElementKind::Column => Some(boxed(c, h, rgb3(CONCRETE))?),
            _ => Some(boxed(c, h, rgb3(PLASTER))?),
        };
        if let Some(m) = extra {
            add_part(&mut out[i], m, None)?;
        }
    }
    Ok((textures, out))
}

/// Adds `m` to an element's meshes, merged with the one of the same
/// texture.
fn add_part(parts: &mut Parts, m: Mesh, tex: Option<usize>) -> EngineResult<()> {
    match parts.iter_mut().find(|(_, t)| *t == tex) {
        Some((have, _)) => have.append_translated(&m, Vec3::ZERO),
        None => {
            parts.push((m, tex));
            Ok(())
        }
    }
}

fn rgb3(c: [u8; 3]) -> Color {
    rgb(c[0], c[1], c[2])
}

/// Distance from a point to a piece's box.
fn box_distance(p: &Piece, q: DVec3) -> f64 {
    (q - q.clamp(p.min, p.max)).length()
}

/// The town's mesh of the building's outside, cut up by element; returns
/// the textures it uses (see [`element_meshes`]).
fn facade(
    plan: &BuildingPlan,
    pieces: &[Piece],
    frame: &Frame,
    out: &mut [Parts],
) -> EngineResult<Vec<Mesh>> {
    let mut b = MeshBuilder::new();
    exterior(&mut b, plan, plan.floors.len(), true, Detail::Near);
    // Outer elements, in a grid for quick lookups.
    let outer: Vec<usize> = (0..pieces.len())
        .filter(|&i| pieces[i].outward != DVec2::ZERO || pieces[i].kind == ElementKind::Roof)
        .collect();
    if outer.is_empty() {
        return Ok(Vec::new());
    }
    let mut textures = Vec::new();
    for part in b.finish_parts() {
        let tex = match part.albedo() {
            Some(a) => {
                // Only the texture travels: the plan keeps one copy of it.
                let mut t = Mesh::new();
                t.set_albedo_rgba(a.width, a.height, a.rgba.clone())?;
                if let Some(maps) = part.material_maps() {
                    t.set_material_maps(maps.clone())?;
                }
                textures.push(t);
                Some(textures.len() - 1)
            }
            None => None,
        };
        cut(&part.build(), tex, pieces, frame, &outer, plan, out)?;
    }
    Ok(textures)
}

/// Gives each triangle of `built` (the building's outside, in city space)
/// to the outer element it lies on.
fn cut(
    built: &engine::mesh::BuiltMesh,
    tex: Option<usize>,
    pieces: &[Piece],
    frame: &Frame,
    outer: &[usize],
    plan: &BuildingPlan,
    out: &mut [Parts],
) -> EngineResult<()> {
    const G: f64 = 4.0;
    let cell = |q: DVec3| {
        (
            (q.x / G).floor() as i32,
            (q.y / G).floor() as i32,
            (q.z / G).floor() as i32,
        )
    };
    let mut grid: BTreeMap<(i32, i32, i32), Vec<usize>> = BTreeMap::new();
    for &i in outer {
        let p = &pieces[i];
        let (lo, hi) = (cell(p.min - DVec3::ONE), cell(p.max + DVec3::ONE));
        for x in lo.0..=hi.0 {
            for y in lo.1..=hi.1 {
                for z in lo.2..=hi.2 {
                    grid.entry((x, y, z)).or_default().push(i);
                }
            }
        }
    }
    let site = &plan.site;
    let to_building = |v: Vec3| {
        let d = DVec2::new(v.x as f64, v.z as f64) - site.origin;
        DVec3::new(
            d.dot(site.along),
            v.y as f64 - crate::citymesh::BLOCK_TOP,
            d.dot(site.inward),
        )
    };
    let dir_to_building = |n: Vec3| {
        let d = DVec2::new(n.x as f64, n.z as f64);
        DVec3::new(d.dot(site.along), n.y as f64, d.dot(site.inward))
    };
    let pos: Vec<DVec3> = built.positions.iter().map(|v| to_building(*v)).collect();
    // Triangles per (element, surface).
    let mut surfaces: Vec<engine::mesh::SurfaceMaterial> = Vec::new();
    let mut groups: BTreeMap<(usize, usize), Vec<[u32; 3]>> = BTreeMap::new();
    for tri in built.indices.chunks_exact(3) {
        let t = [tri[0], tri[1], tri[2]];
        let c = (pos[t[0] as usize] + pos[t[1] as usize] + pos[t[2] as usize]) / 3.0;
        let near = grid.get(&cell(c)).map(|v| v.as_slice()).unwrap_or(outer);
        let best = near
            .iter()
            .copied()
            .min_by(|&a, &b| {
                box_distance(&pieces[a], c)
                    .total_cmp(&box_distance(&pieces[b], c))
                    .then(a.cmp(&b))
            })
            .unwrap_or(outer[0]);
        let surface = built
            .surface_overrides
            .get(t[0] as usize)
            .copied()
            .unwrap_or(built.surface);
        let s = match surfaces.iter().position(|x| *x == surface) {
            Some(k) => k,
            None => {
                surfaces.push(surface);
                surfaces.len() - 1
            }
        };
        groups.entry((best, s)).or_default().push(t);
    }
    for ((i, s), tris) in groups {
        let mut m = Mesh::new();
        for t in tris {
            let order = if frame.mirrored {
                [t[0], t[2], t[1]]
            } else {
                t
            };
            let mut ids = [None; 3];
            for (k, &v) in order.iter().enumerate() {
                let v = v as usize;
                let id = m.add_point(frame.to_engine(pos[v]))?;
                let n = dir_to_building(built.normals[v]);
                m.set_point_normal(id, frame.to_engine(n).normalize_or(Vec3::Y))?;
                let c = built.colors[v];
                m.set_point_color(
                    id,
                    Color {
                        r: c.x,
                        g: c.y,
                        b: c.z,
                        a: c.w,
                    },
                )?;
                ids[k] = Some(id);
            }
            if let [Some(a), Some(b), Some(c)] = ids {
                m.add_triangle(a, b, c)?;
            }
        }
        m.set_surface_material(surfaces[s]);
        add_part(&mut out[i], m, tex)?;
    }
    Ok(())
}

/// The inside face of an outer wall panel: plaster around its windows.
fn backing(plan: &BuildingPlan, frame: &Frame, p: &Piece) -> EngineResult<Mesh> {
    let along_x = p.outward.x.abs() < 0.5;
    let (t0, t1) = if along_x {
        (p.min.x, p.max.x)
    } else {
        (p.min.z, p.max.z)
    };
    // The facade stands on the outline (the wall's centre line) and out of
    // it; the inside face fills the inner half.
    let line = if along_x {
        (p.min.z + p.max.z) * 0.5
    } else {
        (p.min.x + p.max.x) * 0.5
    };
    let inward = -p.outward;
    let (y0, y1) = (p.min.y, p.max.y);
    let mut holes: Vec<(f64, f64, f64, f64)> = Vec::new();
    if let Some(f) = plan.floors.get(p.floor as usize) {
        for o in &plan.openings {
            if o.floor as usize != p.floor as usize || o.kind != OpeningKind::Window {
                continue;
            }
            let out = match o.side {
                Side::Front => DVec2::new(0.0, -1.0),
                Side::Right => DVec2::new(1.0, 0.0),
                Side::Back => DVec2::new(0.0, 1.0),
                Side::Left => DVec2::new(-1.0, 0.0),
            };
            if out.dot(p.outward) < 0.9 {
                continue;
            }
            let (a, b) = (
                (o.along - o.width * 0.5).max(t0),
                (o.along + o.width * 0.5).min(t1),
            );
            let (ya, yb) = (
                (f.elevation + o.sill).max(y0),
                (f.elevation + o.sill + o.height).min(y1),
            );
            if b - a > 0.05 && yb - ya > 0.05 {
                holes.push((a, b, ya, yb));
            }
        }
    }
    holes.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut rects = Vec::new();
    let mut t = t0;
    for &(a, b, ya, yb) in &holes {
        if a > t {
            rects.push((t, a, y0, y1));
        }
        rects.push((a, b, y0, ya));
        rects.push((a, b, yb, y1));
        t = t.max(b);
    }
    if t < t1 {
        rects.push((t, t1, y0, y1));
    }
    let mut m = Mesh::new();
    let (d0, d1) = (0.01, OUTER_HALF);
    for (a, b, ya, yb) in rects {
        if b - a < 0.01 || yb - ya < 0.01 {
            continue;
        }
        let mid = line + (d0 + d1) * 0.5;
        let c = if along_x {
            DVec3::new(
                (a + b) * 0.5,
                (ya + yb) * 0.5,
                line + inward.y * (mid - line),
            )
        } else {
            DVec3::new(
                line + inward.x * (mid - line),
                (ya + yb) * 0.5,
                (a + b) * 0.5,
            )
        };
        let h = if along_x {
            DVec3::new((b - a) * 0.5, (yb - ya) * 0.5, (d1 - d0) * 0.5)
        } else {
            DVec3::new((d1 - d0) * 0.5, (yb - ya) * 0.5, (b - a) * 0.5)
        };
        m.append_translated(
            &boxed(frame.to_engine(c), h.as_vec3(), rgb3(PLASTER))?,
            Vec3::ZERO,
        )?;
    }
    Ok(m)
}

/// Half an outer wall's thickness (m).
const OUTER_HALF: f64 = hw_sim::collision::OUTER_WALL * 0.5;

/// Sparks, dust, a mark on the wall and, for a blast, a small explosion.
fn impact(
    world: &mut World,
    at: Vec3,
    dir: Vec3,
    kind: ImpactKind,
    broke: bool,
) -> EngineResult<()> {
    let normal = (-dir).normalize_or(Vec3::Y);
    let dust = rgb(170, 160, 146);
    match kind {
        ImpactKind::Punch => {
            let p = world.particles_mut();
            p.burst(ParticleEmitter::sparks(at, normal * 4.0, 18))?;
            p.burst(ParticleEmitter::dust(
                at,
                if broke { 1.6 } else { 0.5 },
                dust,
            ))?;
            let mut d = Decal::new(DecalKind::Impact, at, normal, 0.9);
            d.lifetime = Some(MARK_SECONDS);
            d.fade_out = 3.0;
            world.spawn_decal(d)?;
        }
        ImpactKind::Blast => {
            world.explosion(at, 0.9)?;
            if broke {
                world
                    .particles_mut()
                    .burst(ParticleEmitter::dust(at, 2.2, dust))?;
            }
            let mut d = Decal::new(DecalKind::Scorch, at, normal, 2.6);
            d.lifetime = Some(MARK_SECONDS);
            d.fade_out = 4.0;
            world.spawn_decal(d)?;
        }
    }
    Ok(())
}
