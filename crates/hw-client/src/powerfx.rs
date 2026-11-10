//! The looks of the newer powers: the freeze ray's cone and the ice a
//! minion is frozen in, the shards when it shatters, the teleport's
//! shimmer, the hypno ray's spirals, the rescue bubble that floats people
//! out of trouble, and the sidekick beaming in.
//!
//! Meshes here are small (a few hundred triangles) and made once, the first
//! time they are wanted; building one takes microseconds, so it stays on
//! the main thread with the rest of the fight's effects.
//!
//! The bubble's soap film and the ice's frost and glint are Engine custom
//! materials (WGSL run inside the lit shader, so they are still lit and
//! fogged). [`install_materials`] adds them once at the start; without it
//! (tools, the showroom) the meshes keep their plain vertex colours.

use engine::prelude::*;
use glam::Vec3;
use std::collections::BTreeMap;
use std::sync::OnceLock;

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::rgb(r, g, b)
}

/// A deterministic jitter in -1..1 from a counter.
fn wobble(k: u32) -> f32 {
    let x = k.wrapping_mul(2_654_435_761).rotate_left(13) ^ 0x5bd1_e995;
    (x.wrapping_mul(0x27d4_eb2d) >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
}

/// The rescue bubble's film: swirling rainbow bands that drain slowly
/// downwards, nearly clear where you look straight through and bright at
/// the rim (Fresnel), with a light that never sits still. The vertex
/// colour (blue-pink or gold) tints it. `params[0].x`: glow.
pub const BUBBLE_FILM: &str = r#"
fn material(m: MaterialIn, base: MaterialOut) -> MaterialOut {
    var o = base;
    let n = normalize(m.normal);
    let facing = clamp(abs(dot(n, m.view_dir)), 0.0, 1.0);
    let rim = pow(1.0 - facing, 3.0);
    let p = normalize(m.object_pos + vec3<f32>(0.0, 0.0001, 0.0));
    let swirl = sin(p.x * 5.0 + m.time * 0.9) * 0.5
        + sin(p.z * 4.0 - m.time * 0.7 + p.y * 3.0) * 0.5;
    let thick = p.y * 1.2 + swirl * 0.6 + m.time * 0.15 + (1.0 - facing) * 2.0;
    let film = 0.5 + 0.5 * cos(6.2831 * (vec3<f32>(thick) + vec3<f32>(0.0, 0.33, 0.67)));
    let tint = mix(film, base.color.rgb, 0.6);
    o.color = vec4<f32>(tint, clamp(0.05 + 0.55 * rim + base.color.a * 0.3, 0.0, 0.8));
    o.roughness = 0.03;
    o.metallic = 0.1;
    o.emission = tint * (0.08 + 0.9 * rim) * m.params[0].x;
    let w = vec3<f32>(
        sin(m.time * 2.1 + p.y * 6.0),
        sin(m.time * 1.7 + p.z * 6.0),
        sin(m.time * 1.9 + p.x * 6.0)
    ) * 0.06;
    o.normal = normalize(n + w);
    return o;
}
"#;

/// Clear ice with cloudy frost inside (thicker toward the foot), a white
/// rim where you look along the surface, and a glint that sweeps up it.
/// `params[0].x`: glow.
pub const ICE_FROST: &str = r#"
fn material(m: MaterialIn, base: MaterialOut) -> MaterialOut {
    var o = base;
    let n = normalize(m.normal);
    let facing = clamp(abs(dot(n, m.view_dir)), 0.0, 1.0);
    let rim = pow(1.0 - facing, 2.5);
    let q = m.object_pos;
    let frost = 0.5 + 0.5 * sin(q.x * 9.0 + sin(q.y * 7.0) * 2.0) * sin(q.z * 8.0 + q.y * 3.0);
    let cloud = clamp(0.22 - q.y * 0.09, 0.0, 0.22) + frost * 0.08;
    let white = vec3<f32>(0.9, 0.97, 1.0);
    o.color = vec4<f32>(
        mix(base.color.rgb, white, clamp(cloud + rim * 0.45, 0.0, 1.0)),
        clamp(base.color.a * 0.8 + cloud * 0.4 + rim * 0.3, 0.0, 0.9)
    );
    o.roughness = mix(0.05, 0.35, clamp(frost * cloud * 2.0, 0.0, 1.0));
    let sweep = fract(m.time * 0.35 + (q.x + q.z) * 0.1);
    let band = smoothstep(0.06, 0.0, abs(q.y / 2.5 - sweep));
    o.emission = vec3<f32>(0.6, 0.85, 1.0) * (rim * 0.35 + band * 0.8) * m.params[0].x;
    return o;
}
"#;

/// A force field: a honeycomb of light that shows at the rim (Fresnel)
/// and where ripples run down the shell, nearly clear in the middle so the
/// one inside stays visible. The vertex colour tints it. `params[0].x`: glow.
pub const FORCE_FIELD: &str = r#"
fn hex_edge(uv: vec2<f32>) -> f32 {
    let r = vec2<f32>(1.0, 1.732);
    let h = r * 0.5;
    let a = uv - r * floor(uv / r) - h;
    let bb = uv - h;
    let b = bb - r * floor(bb / r) - h;
    var g = b;
    if dot(a, a) < dot(b, b) {
        g = a;
    }
    let q = abs(g);
    return max(q.x * 0.866 + q.y * 0.5, q.y);
}

fn material(m: MaterialIn, base: MaterialOut) -> MaterialOut {
    var o = base;
    let n = normalize(m.normal);
    let facing = clamp(abs(dot(n, m.view_dir)), 0.0, 1.0);
    let rim = pow(1.0 - facing, 2.5);
    let d = normalize(m.object_pos + vec3<f32>(0.0, 0.0001, 0.0));
    let uv = vec2<f32>(atan2(d.z, d.x) * 3.8, d.y * 5.5);
    let edge = smoothstep(0.40, 0.49, hex_edge(uv));
    let ripple = pow(0.5 + 0.5 * sin(d.y * 14.0 + m.time * 5.0), 4.0);
    let shimmer = 0.85 + 0.15 * sin(m.time * 23.0 + d.x * 9.0);
    let lit = rim * 1.3 + edge * (0.35 + 1.1 * ripple);
    o.color = vec4<f32>(base.color.rgb, clamp(0.03 + rim * 0.55 + edge * (0.12 + 0.3 * ripple), 0.0, 0.85));
    o.roughness = 0.1;
    o.metallic = 0.0;
    o.emission = base.color.rgb * lit * shimmer * m.params[0].x;
    return o;
}
"#;

/// The Tesla tower's glowing coil, rim and ball: bright bands that climb
/// the coil, plasma veins that writhe over it and a nervous flicker. Adds
/// to the mesh's own glow, scaled by its colour (the instance tint the
/// tower's pulse rides on). `params[0].x`: strength.
pub const TESLA_PLASMA: &str = r#"
fn material(m: MaterialIn, base: MaterialOut) -> MaterialOut {
    var o = base;
    let p = m.object_pos;
    let t = m.time;
    let n = normalize(m.normal);
    let band = pow(0.5 + 0.5 * sin(p.y * 3.0 - t * 6.0), 6.0);
    let vein = abs(sin(p.x * 2.3 + sin(p.y * 1.7 + t * 2.0) * 2.0 + t * 1.3)
        * sin(p.z * 2.1 - t * 1.1 + sin(p.y * 2.2) * 1.5));
    let veins = pow(1.0 - vein, 6.0);
    let flick = 0.75 + 0.25 * sin(t * 37.0 + p.y * 5.0);
    let rim = pow(1.0 - clamp(abs(dot(n, m.view_dir)), 0.0, 1.0), 2.0);
    let lit = (band * 1.2 + veins * 1.4 + rim * 0.6) * flick;
    // The mesh's own glow follows its colour: dim it between the bands
    // and veins so they stand out, and add their light on top.
    o.color = vec4<f32>(base.color.rgb * (0.3 + 0.7 * clamp(lit, 0.0, 1.0)), base.color.a);
    o.emission = base.color.rgb * lit * m.params[0].x;
    return o;
}
"#;

/// The tower's arcs (they run along +z): bright pulses race down each
/// bolt and it stutters, as real sparks do. Scaled by the instance tint.
/// `params[0].x`: strength.
pub const TESLA_ARC: &str = r#"
fn material(m: MaterialIn, base: MaterialOut) -> MaterialOut {
    var o = base;
    let z = m.object_pos.z;
    let run = pow(0.5 + 0.5 * sin(z * 1.5 - m.time * 90.0), 3.0);
    let k = floor(m.time * 30.0);
    let stutter = 0.55 + 0.45 * step(0.3, fract(sin(k * 12.9898 + floor(z * 0.5) * 78.233) * 43758.547));
    o.color = vec4<f32>(base.color.rgb * (0.5 + 0.5 * run) * stutter, base.color.a);
    o.emission = base.color.rgb * (0.4 + 2.2 * run) * stutter * m.params[0].x;
    return o;
}
"#;

/// The custom materials of the power effects, once added.
#[derive(Clone, Copy, Debug)]
struct FxMaterials {
    bubble: CustomMaterialId,
    ice: CustomMaterialId,
    field: CustomMaterialId,
    plasma: CustomMaterialId,
    arc: CustomMaterialId,
}

/// Set once by [`install_materials`] on the main thread at the start, read
/// on the main thread afterwards.
static MATERIALS: OnceLock<FxMaterials> = OnceLock::new();

/// The bubble film material, ready to add.
pub fn bubble_material() -> CustomMaterial {
    CustomMaterial::new("bubble film", BUBBLE_FILM)
        .see_through()
        .param(0, [1.0, 0.0, 0.0, 0.0])
}

/// The ice material, ready to add.
pub fn ice_material() -> CustomMaterial {
    CustomMaterial::new("ice frost", ICE_FROST)
        .see_through()
        .param(0, [1.0, 0.0, 0.0, 0.0])
}

/// The shield's force field material, ready to add.
pub fn field_material() -> CustomMaterial {
    CustomMaterial::new("force field", FORCE_FIELD)
        .see_through()
        .param(0, [1.0, 0.0, 0.0, 0.0])
}

/// The Tesla coil's plasma material, ready to add (solid: it only adds glow).
pub fn plasma_material() -> CustomMaterial {
    CustomMaterial::new("tesla plasma", TESLA_PLASMA).param(0, [1.0, 0.0, 0.0, 0.0])
}

/// The Tesla arcs' material, ready to add.
pub fn arc_material() -> CustomMaterial {
    CustomMaterial::new("tesla arc", TESLA_ARC).param(0, [1.0, 0.0, 0.0, 0.0])
}

/// Adds the power effects' materials to the world. Call once, on the first
/// frame: Engine builds a material's pipelines when it first draws it, so
/// adding them at the start keeps that work out of a fight.
pub fn install_materials(world: &mut World) -> EngineResult<()> {
    if MATERIALS.get().is_some() {
        return Ok(());
    }
    let bubble = world.add_material(bubble_material())?;
    let ice = world.add_material(ice_material())?;
    let field = world.add_material(field_material())?;
    let plasma = world.add_material(plasma_material())?;
    let arc = world.add_material(arc_material())?;
    let _ = MATERIALS.set(FxMaterials {
        bubble,
        ice,
        field,
        plasma,
        arc,
    });
    Ok(())
}

/// Gives a freshly placed rescue bubble its film (when installed).
pub fn dress_bubble(world: &mut World, id: EntityId) {
    if let Some(m) = MATERIALS.get() {
        let _ = world.set_custom_material(id, m.bubble);
    }
}

/// Gives a freshly placed shield shell its force field (when installed).
pub fn dress_field(world: &mut World, id: EntityId) {
    if let Some(m) = MATERIALS.get() {
        let _ = world.set_custom_material(id, m.field);
    }
}

/// Gives a Tesla tower's glowing part its plasma (when installed).
pub fn dress_plasma(world: &mut World, id: EntityId) {
    if let Some(m) = MATERIALS.get() {
        let _ = world.set_custom_material(id, m.plasma);
    }
}

/// Gives a Tesla arc shape its racing pulses (when installed).
pub fn dress_arc(world: &mut World, id: EntityId) {
    if let Some(m) = MATERIALS.get() {
        let _ = world.set_custom_material(id, m.arc);
    }
}

/// A shield's shell: a smooth unit sphere (place it with a scale) in the
/// shield's colour; the force field material draws the honeycomb. Centre
/// at the origin.
pub fn shield_sphere() -> Mesh {
    static MESH: OnceLock<Mesh> = OnceLock::new();
    let unit = MESH.get_or_init(|| {
        let (around, down) = (48u32, 28u32);
        let mut m = Mesh::new();
        let mut ids = Vec::new();
        for j in 0..=down {
            let v = j as f32 / down as f32 * std::f32::consts::PI;
            for i in 0..=around {
                let u = i as f32 / around as f32 * std::f32::consts::TAU;
                let n = Vec3::new(v.sin() * u.cos(), v.cos(), v.sin() * u.sin());
                let id = m.add_point(n).expect("point");
                let _ = m.set_point_normal(id, n);
                let _ = m.set_point_color(
                    id,
                    Color {
                        a: 0.18,
                        ..rgb(140, 220, 255)
                    },
                );
                ids.push(id);
            }
        }
        let at = |i: u32, j: u32| ids[(j * (around + 1) + i) as usize];
        for j in 0..down {
            for i in 0..around {
                let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
                let _ = m.add_triangle(a, b, c);
                let _ = m.add_triangle(a, c, d);
            }
        }
        m.set_surface_material(SurfaceMaterial {
            roughness: 0.1,
            emission: 0.3,
            ..SurfaceMaterial::DEFAULT
        });
        m
    });
    unit.clone()
}

/// Gives a freshly placed ice block its frost (when installed).
pub fn dress_ice(world: &mut World, id: EntityId) {
    if let Some(m) = MATERIALS.get() {
        let _ = world.set_custom_material(id, m.ice);
    }
}

/// A block of clear ice round a person: a rough slab, wider at the foot,
/// with crystal points breaking out of the top and sides. Origin at the
/// feet. See-through, glinting.
pub fn ice_block() -> Mesh {
    static MESH: OnceLock<Mesh> = OnceLock::new();
    MESH.get_or_init(|| {
        let mut m = Mesh::new();
        let ice = |a: f32| Color {
            a,
            ..rgb(196, 232, 255)
        };
        // The slab: rings of corners from the foot up, each nudged.
        const SIDES: usize = 9;
        let rows: [(f32, f32); 6] = [
            (0.0, 0.78),
            (0.12, 0.84),
            (0.7, 0.74),
            (1.35, 0.7),
            (1.95, 0.6),
            (2.25, 0.32),
        ];
        let mut ring_ids = Vec::new();
        for (r, &(y, rad)) in rows.iter().enumerate() {
            let mut ids = Vec::new();
            for i in 0..SIDES {
                let k = (r * SIDES + i) as u32;
                let a = std::f32::consts::TAU * (i as f32 + 0.3 * wobble(k)) / SIDES as f32;
                let rr = rad * (1.0 + 0.12 * wobble(k + 101));
                let yy = y + if r == 0 { 0.0 } else { 0.08 * wobble(k + 202) };
                let p = Vec3::new(a.cos() * rr, yy, a.sin() * rr * 0.85);
                let id = m.add_point(p).expect("point");
                let _ = m.set_point_color(id, ice(0.42 + 0.1 * wobble(k + 303).abs()));
                ids.push(id);
            }
            ring_ids.push(ids);
        }
        for w in ring_ids.windows(2) {
            for i in 0..SIDES {
                let j = (i + 1) % SIDES;
                let _ = m.add_triangle(w[0][i], w[1][j], w[0][j]);
                let _ = m.add_triangle(w[0][i], w[1][i], w[1][j]);
            }
        }
        // The top: a jagged crown closing the slab.
        let top = m.add_point(Vec3::new(0.05, 2.5, -0.04)).expect("point");
        let _ = m.set_point_color(top, ice(0.5));
        let last = ring_ids.last().expect("rows");
        for i in 0..SIDES {
            let j = (i + 1) % SIDES;
            let _ = m.add_triangle(last[i], top, last[j]);
        }
        // Crystal points breaking out: thin pyramids.
        for k in 0..7u32 {
            let a = std::f32::consts::TAU * (k as f32 / 7.0 + 0.05 * wobble(k + 900));
            let y = 0.4 + 1.8 * (wobble(k + 950) * 0.5 + 0.5);
            let out = Vec3::new(a.cos(), 0.35 + 0.3 * wobble(k + 970), a.sin()).normalize();
            let base = Vec3::new(a.cos() * 0.55, y, a.sin() * 0.47);
            let len = 0.35 + 0.25 * wobble(k + 990).abs();
            let side = out.cross(Vec3::Y).normalize_or(Vec3::X) * 0.12;
            let up = out.cross(side).normalize_or(Vec3::Y) * 0.12;
            let tip = m.add_point(base + out * len).expect("point");
            let _ = m.set_point_color(tip, ice(0.62));
            let corners: Vec<PointId> = [side + up, side - up, -side - up, -side + up]
                .iter()
                .map(|c| {
                    let id = m.add_point(base + *c).expect("point");
                    let _ = m.set_point_color(id, ice(0.4));
                    id
                })
                .collect();
            for i in 0..4 {
                let _ = m.add_triangle(corners[i], corners[(i + 1) % 4], tip);
            }
        }
        m.set_surface_material(SurfaceMaterial {
            roughness: 0.04,
            metallic: 0.25,
            emission: 0.18,
            ..SurfaceMaterial::DEFAULT
        });
        m
    })
    .clone()
}

/// The rescue bubble: a soap-film sphere with a rainbow sheen, faint in
/// the middle and stronger toward its rim bands. Centre at the origin.
pub fn bubble_sphere(radius: f32, gold: bool) -> Mesh {
    let (around, down) = (40u32, 24u32);
    let mut m = Mesh::new();
    let mut ids = Vec::new();
    for j in 0..=down {
        let v = j as f32 / down as f32 * std::f32::consts::PI;
        for i in 0..=around {
            let u = i as f32 / around as f32 * std::f32::consts::TAU;
            let n = Vec3::new(v.sin() * u.cos(), v.cos(), v.sin() * u.sin());
            let id = m.add_point(n * radius).expect("point");
            let _ = m.set_point_normal(id, n);
            // Film colours drift round the sphere.
            let t = (v * 2.0 + u * 0.5).sin() * 0.5 + 0.5;
            let c = if gold {
                lerp(rgb(255, 214, 120), rgb(255, 250, 220), t)
            } else {
                lerp(rgb(150, 220, 255), rgb(255, 170, 230), t)
            };
            let rim = 1.0 - v.sin().powf(4.0) * 0.5;
            let _ = m.set_point_color(
                id,
                Color {
                    a: 0.16 + 0.14 * rim,
                    ..c
                },
            );
            ids.push(id);
        }
    }
    let at = |i: u32, j: u32| ids[(j * (around + 1) + i) as usize];
    for j in 0..down {
        for i in 0..around {
            let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
            let _ = m.add_triangle(a, b, c);
            let _ = m.add_triangle(a, c, d);
        }
    }
    m.set_surface_material(SurfaceMaterial {
        roughness: 0.02,
        metallic: 0.4,
        emission: 0.55,
        ..SurfaceMaterial::DEFAULT
    });
    m
}

fn lerp(a: Color, b: Color, t: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

/// The freeze ray: a widening cone of frost from `from` to `to`, with
/// glittering crystals and a cold mist.
pub fn frost_cone(world: &mut World, from: Vec3, to: Vec3, core: Color, edge: Color, widen: f32) {
    let len = from.distance(to);
    if !len.is_finite() || len < 0.1 {
        return;
    }
    let dir = (to - from) / len;
    let n = ((len / 0.45) as usize).clamp(4, 60);
    let p = world.particles_mut();
    for i in 0..=n {
        let f = i as f32 / n as f32;
        let at = from + dir * (len * f);
        let wide = 0.15 + len * f * widen;
        let _ = p.burst(
            ParticleEmitter::new(at)
                .with_mode(ParticleMode::Burst)
                .with_shape(ParticleShape::Sphere)
                .with_burst_count(5)
                .with_size(0.14 + 0.25 * f)
                .with_size_variation(0.5)
                .with_lifetime(0.45 + 0.3 * f)
                .with_spread(Vec3::splat(wide))
                .with_velocity(dir * 5.0)
                .with_velocity_jitter(Vec3::splat(0.6))
                .with_drag(2.5)
                .with_color(core)
                .with_secondary_color(edge)
                .with_size_over_life(SizeOverLife::GrowThenFade)
                .with_seed(i as u32 * 7 + 1)
                .with_emissive_intensity(3.0),
        );
        // Crystals glinting in the cold.
        let _ = p.burst(
            ParticleEmitter::new(at)
                .with_mode(ParticleMode::Burst)
                .with_burst_count(3)
                .with_size(0.05)
                .with_lifetime(0.7)
                .with_spread(Vec3::splat(wide * 0.8))
                .with_velocity_jitter(Vec3::splat(0.4))
                .with_color(rgb(255, 255, 255))
                .with_silhouette(ParticleSilhouette::RuneMote)
                .with_seed(i as u32 * 13 + 5)
                .with_emissive_intensity(6.0),
        );
    }
    // Cold mist hanging where it went.
    let _ = p.burst(
        ParticleEmitter::smoke(to, 1.2)
            .with_mode(ParticleMode::Burst)
            .with_burst_count(10)
            .with_color(Color::rgba(220, 240, 255, 90))
            .with_secondary_color(Color::rgba(255, 255, 255, 30)),
    );
}

/// Ice breaking: shards flying out, glittering dust, a puff of cold.
pub fn shatter(world: &mut World, at: Vec3) {
    let p = world.particles_mut();
    let _ = p.burst(
        ParticleEmitter::new(at)
            .with_mode(ParticleMode::Burst)
            .with_shape(ParticleShape::Sphere)
            .with_burst_count(70)
            .with_size(0.13)
            .with_size_variation(0.6)
            .with_lifetime(0.9)
            .with_spread(Vec3::new(0.5, 0.9, 0.5))
            .with_velocity(Vec3::Y * 2.0)
            .with_velocity_jitter(Vec3::splat(6.0))
            .with_acceleration(Vec3::Y * -14.0)
            .with_color(rgb(210, 240, 255))
            .with_secondary_color(rgb(255, 255, 255))
            .with_silhouette(ParticleSilhouette::SparkStreak)
            .with_velocity_stretch(0.03)
            .with_emissive_intensity(2.5),
    );
    let _ = p.burst(
        ParticleEmitter::new(at)
            .with_mode(ParticleMode::Burst)
            .with_burst_count(40)
            .with_size(0.05)
            .with_lifetime(1.2)
            .with_spread(Vec3::splat(0.8))
            .with_velocity_jitter(Vec3::splat(1.5))
            .with_color(rgb(255, 255, 255))
            .with_silhouette(ParticleSilhouette::RuneMote)
            .with_emissive_intensity(6.0),
    );
    let _ = p.burst(
        ParticleEmitter::smoke(at, 1.0)
            .with_mode(ParticleMode::Burst)
            .with_burst_count(8)
            .with_color(Color::rgba(230, 245, 255, 110))
            .with_secondary_color(Color::rgba(255, 255, 255, 30)),
    );
}

/// Frost motes drifting off someone chilled.
pub fn chill(world: &mut World, at: Vec3) {
    let _ = world.particles_mut().burst(
        ParticleEmitter::new(at)
            .with_mode(ParticleMode::Burst)
            .with_shape(ParticleShape::Sphere)
            .with_burst_count(10)
            .with_size(0.06)
            .with_lifetime(0.9)
            .with_spread(Vec3::new(0.35, 0.8, 0.35))
            .with_velocity(Vec3::Y * -0.4)
            .with_velocity_jitter(Vec3::splat(0.25))
            .with_color(rgb(190, 230, 255))
            .with_secondary_color(rgb(255, 255, 255))
            .with_silhouette(ParticleSilhouette::RuneMote)
            .with_emissive_intensity(3.0),
    );
}

/// A teleport: a column of light that draws in (leaving) or bursts out
/// (arriving), with a ring on the ground.
pub fn teleport(world: &mut World, feet: Vec3, core: Color, edge: Color, arriving: bool) {
    let p = world.particles_mut();
    let _ = p.burst(
        ParticleEmitter::new(feet + Vec3::Y * 1.0)
            .with_mode(ParticleMode::Burst)
            .with_shape(ParticleShape::Ring)
            .with_burst_count(140)
            .with_size(0.09)
            .with_lifetime(0.6)
            .with_spread(Vec3::new(0.7, 1.0, 0.7))
            .with_velocity(Vec3::Y * if arriving { -2.5 } else { 3.5 })
            .with_force(ParticleForce::Radial {
                center: feet + Vec3::Y * 1.0,
                strength: if arriving { 4.0 } else { -6.0 },
            })
            .with_color(core)
            .with_secondary_color(edge)
            .with_silhouette(ParticleSilhouette::RuneMote)
            .with_emissive_intensity(7.0),
    );
    let _ = p.burst(
        ParticleEmitter::new(feet + Vec3::Y * 1.0)
            .with_mode(ParticleMode::Burst)
            .with_burst_count(1)
            .with_size(if arriving { 2.6 } else { 1.8 })
            .with_lifetime(0.3)
            .with_spread(Vec3::ZERO)
            .with_color(core)
            .with_size_over_life(SizeOverLife::GrowThenFade)
            .with_emissive_intensity(6.0),
    );
    let ring = Decal::new(DecalKind::Power, feet + Vec3::Y * 0.2, Vec3::Y, 2.4)
        .tint(edge)
        .emission(3.0)
        .depth(1.2)
        .lifetime(0.8)
        .fade(0.05, 0.6);
    let _ = world.spawn_decal(ring);
}

/// Hypnotic spirals: motes winding round a point (a ray's end, a head).
pub fn spiral(world: &mut World, at: Vec3, core: Color, edge: Color, size: f32) {
    let p = world.particles_mut();
    for k in 0..3 {
        let _ = p.burst(
            ParticleEmitter::new(at + Vec3::Y * (k as f32 * 0.08))
                .with_mode(ParticleMode::Burst)
                .with_shape(ParticleShape::Ring)
                .with_burst_count(18)
                .with_size(0.06 * size + 0.02)
                .with_lifetime(0.6)
                .with_spread(Vec3::new(
                    0.35 * size * (k + 1) as f32,
                    0.02,
                    0.35 * size * (k + 1) as f32,
                ))
                .with_force(ParticleForce::Vortex {
                    axis: Vec3::Y,
                    strength: 9.0,
                })
                .with_color(if k % 2 == 0 { core } else { edge })
                .with_silhouette(ParticleSilhouette::RuneMote)
                .with_seed(k as u32 + 1)
                .with_emissive_intensity(5.0),
        );
    }
}

/// A rescue bubble on its way: up out of the place, over, and gently down
/// where it was sent, with the person inside.
struct Floating {
    sphere: EntityId,
    /// Who is inside, how high over the bubble's foot they are held, and
    /// whether they walk on once set down (the hurt go to the ambulance).
    person: Option<(crate::radioview::Shown, f32, bool)>,
    from: Vec3,
    to: Option<Vec3>,
    age: f32,
    gold: bool,
    hero: u32,
    /// Where its centre is now.
    at: Vec3,
}

/// The bubbles in the air.
#[derive(Default)]
pub struct Bubbles {
    list: BTreeMap<u32, Floating>,
    next: u32,
    /// People set down out of bubbles that walk on: who, their feet, and
    /// which way the bubble carried them.
    pub landed: Vec<(crate::radioview::Shown, Vec3, Vec3)>,
}

/// How long a bubble floats (s), and how high it rises on the way (m).
pub const FLOAT_SECONDS: f32 = hw_sim::combat::BUBBLE_SECONDS as f32;
const FLOAT_HEIGHT: f32 = 4.0;
/// Its size round a person (m).
const RADIUS: f32 = 1.25;

impl Bubbles {
    /// A bubble forms round someone at `at` (their feet), sent by `hero`;
    /// gives its number (see [`Self::hold`]).
    pub fn start(&mut self, world: &mut World, at: Vec3, gold: bool, hero: u32) -> Option<u32> {
        let centre = at + Vec3::Y * 1.05;
        let sphere = world
            .place(
                bubble_sphere(RADIUS, gold),
                Place::new(centre.x, centre.y, centre.z),
            )
            .ok()?;
        dress_bubble(world, sphere);
        self.next = self.next.wrapping_add(1);
        self.list.insert(
            self.next,
            Floating {
                sphere,
                person: None,
                from: at,
                to: None,
                age: 0.0,
                gold,
                hero,
                at: centre,
            },
        );
        Some(self.next)
    }

    /// Bubble `key` holds `who`, `lift` metres over its foot; `walk_on`:
    /// set down, they walk off (see [`Self::landed`]).
    pub fn hold(
        &mut self,
        world: &mut World,
        key: u32,
        who: crate::radioview::Shown,
        lift: f32,
        walk_on: bool,
    ) {
        match self.list.get_mut(&key) {
            Some(f) => f.person = Some((who, lift, walk_on)),
            None => world.despawn(who.e),
        }
    }

    /// Where the newest of `hero`'s bubbles is going.
    pub fn land(&mut self, hero: u32, to: Vec3) {
        if let Some(f) = self
            .list
            .values_mut()
            .rev()
            .find(|f| f.hero == hero && f.to.is_none())
        {
            f.to = Some(to);
        }
    }

    pub fn update(&mut self, world: &mut World, dt: f32) {
        let mut done = Vec::new();
        for (k, f) in self.list.iter_mut() {
            f.age += dt;
            let to = f.to.unwrap_or(f.from + Vec3::Y * 0.5);
            let t = (f.age / FLOAT_SECONDS).clamp(0.0, 1.0);
            // Ease in and out, rising in an arc, and a gentle bob.
            let s = t * t * (3.0 - 2.0 * t);
            let feet = f.from.lerp(to, s)
                + Vec3::Y
                    * (FLOAT_HEIGHT * (std::f32::consts::PI * t).sin()
                        + 0.12 * (f.age * 5.0).sin());
            let spin = f.age * 40.0;
            let centre = feet + Vec3::Y * 1.05;
            f.at = centre;
            let _ = world.set_place(
                f.sphere,
                Place::new(centre.x, centre.y, centre.z).with_yaw_deg(spin),
            );
            if let Some((p, lift, _)) = f.person {
                let _ = world.set_place(
                    p.e,
                    Place::new(feet.x, feet.y + lift, feet.z).with_yaw_deg(spin * 0.5),
                );
            }
            if t >= 1.0 {
                done.push(*k);
            }
        }
        for k in done {
            let Some(f) = self.list.remove(&k) else {
                continue;
            };
            // It pops: a ring of film droplets.
            let at = f.at;
            world.despawn(f.sphere);
            match f.person {
                Some((p, _, true)) => {
                    let to = f.to.unwrap_or(f.from);
                    let _ = world.set_place(p.e, Place::new(to.x, to.y, to.z));
                    self.landed.push((p, to, to - f.from));
                }
                Some((p, _, false)) => world.despawn(p.e),
                None => {}
            }
            let c = if f.gold {
                rgb(255, 220, 140)
            } else {
                rgb(170, 225, 255)
            };
            let _ = world.particles_mut().burst(
                ParticleEmitter::new(at)
                    .with_mode(ParticleMode::Burst)
                    .with_shape(ParticleShape::Sphere)
                    .with_burst_count(60)
                    .with_size(0.06)
                    .with_lifetime(0.6)
                    .with_spread(Vec3::splat(RADIUS))
                    .with_velocity_jitter(Vec3::splat(2.0))
                    .with_acceleration(Vec3::Y * -6.0)
                    .with_color(c)
                    .with_secondary_color(rgb(255, 255, 255))
                    .with_emissive_intensity(4.0),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The WGSL of both materials compiles inside Engine's lit shader (no
    /// GPU needed: Engine checks it with naga).
    #[test]
    fn power_materials_compile() {
        bubble_material().check().expect("bubble film");
        ice_material().check().expect("ice frost");
        field_material().check().expect("force field");
        plasma_material().check().expect("tesla plasma");
        arc_material().check().expect("tesla arc");
    }
}
