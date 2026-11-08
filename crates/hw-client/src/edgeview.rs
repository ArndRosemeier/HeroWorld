//! The frayed edge of the mad scientist's quarter as the camera sees it:
//! graffiti on the walls ([`hw_gen::urban::edge::tags`]) and barricades
//! where the streets run in ([`hw_gen::urban::edge::barricades`]).
//!
//! Graffiti are decals, which Engine keeps in a shared pool: only those
//! near the camera are spawned, and they are refreshed as it moves.

use crate::citymesh::{MeshBuilder, Surf};
use engine::prelude::*;
use glam::{DVec3, Vec3};
use hw_core::Seed;
use hw_gen::urban::edge::{self, Barricade, Tag, TAG_IMAGES};
use hw_gen::urban::world::TownWorld;
use std::sync::Mutex;

/// Graffiti this near the camera are drawn (m).
const TAG_REACH: f32 = 140.0;
/// At most this many at once.
const TAG_MOST: usize = 60;
/// Respawn the near set when the camera has moved this far (m).
const TAG_STEP: f32 = 20.0;

const IMAGES: [&[u8]; TAG_IMAGES as usize] = [
    include_bytes!("../assets/graffiti/skull.png"),
    include_bytes!("../assets/graffiti/zap.png"),
    include_bytes!("../assets/graffiti/coil.png"),
    include_bytes!("../assets/graffiti/wild.png"),
];

/// The graffiti's decal kinds: added to the World once per run (it holds
/// only a few custom images).
static KINDS: Mutex<Option<Vec<DecalKind>>> = Mutex::new(None);

fn kinds(world: &mut World) -> Option<Vec<DecalKind>> {
    let mut k = KINDS.lock().ok()?;
    if k.is_none() {
        let mut out = Vec::new();
        for bytes in IMAGES {
            let img = image::load_from_memory(bytes).ok()?.to_rgba8();
            let (w, h) = img.dimensions();
            out.push(
                world
                    .add_decal_image(img.as_raw(), w as usize, h as usize, 0.85)
                    .ok()?,
            );
        }
        *k = Some(out);
    }
    k.clone()
}

pub struct EdgeView {
    tags: Vec<Tag>,
    barricades: Vec<Barricade>,
    built: bool,
    entities: Vec<EntityId>,
    decals: Vec<DecalId>,
    /// Where the camera was when the near graffiti were last picked.
    picked_at: Option<Vec3>,
}

impl EdgeView {
    pub fn new(town: &TownWorld) -> Self {
        Self {
            tags: edge::tags(town),
            barricades: edge::barricades(town),
            built: false,
            entities: Vec::new(),
            decals: Vec::new(),
            picked_at: None,
        }
    }

    pub fn update(&mut self, world: &mut World, town: &TownWorld) -> EngineResult<()> {
        if !self.built {
            self.built = true;
            for b in &self.barricades {
                let y = town.ground(b.at);
                let yaw = b.along.x.atan2(b.along.y).to_degrees() as f32;
                let place = Place::new(b.at.x as f32, y as f32, b.at.y as f32).with_yaw_deg(yaw);
                self.entities.push(world.place(barricade_mesh(b), place)?);
            }
        }
        if self.tags.is_empty() {
            return Ok(());
        }
        let eye = world.camera().eye();
        if self.picked_at.is_some_and(|p| p.distance(eye) < TAG_STEP) {
            return Ok(());
        }
        self.picked_at = Some(eye);
        let Some(kinds) = kinds(world) else {
            return Ok(());
        };
        for d in self.decals.drain(..) {
            world.remove_decal(d);
        }
        let mut near: Vec<(f32, &Tag)> = self
            .tags
            .iter()
            .map(|t| (t.at.as_vec3().distance(eye), t))
            .filter(|(d, _)| *d < TAG_REACH)
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, t) in near.into_iter().take(TAG_MOST) {
            let n = Vec3::new(t.normal.x as f32, 0.0, t.normal.y as f32);
            let d = Decal::new(kinds[t.image as usize], t.at.as_vec3(), n, t.size as f32)
                .depth(0.6)
                .angle(t.angle as f32)
                .opacity(0.92);
            if let Ok(id) = world.spawn_decal(d) {
                self.decals.push(id);
            }
        }
        Ok(())
    }

    pub fn clear(&mut self, world: &mut World) {
        for e in self.entities.drain(..) {
            world.despawn(e);
        }
        for d in self.decals.drain(..) {
            world.remove_decal(d);
        }
        self.picked_at = None;
        self.built = false;
    }
}

/// A barricade across a street, facing +z (into the quarter): concrete
/// barriers and striped sawhorses from both kerbs, leaving one lane open,
/// sandbags along the pavements and a burning oil drum.
fn barricade_mesh(b: &Barricade) -> Mesh {
    let mut m = MeshBuilder::new();
    let mut rng = Seed(b.seed).rng();
    let concrete = rgb(150, 148, 140);
    let wood = rgb(120, 92, 60);
    let yellow = rgb(235, 190, 40);
    let black = rgb(30, 30, 30);
    let sand = rgb(170, 150, 110);
    let h = b.half;
    let reach = (h - 1.8).max(0.0);
    for sx in [-1.0f64, 1.0] {
        if reach > 0.8 {
            // A jersey barrier out from the kerb.
            let (x0, x1) = (sx * h, sx * (h - reach));
            let (lo, hi) = (x0.min(x1), x0.max(x1));
            m.hexa(
                [
                    DVec3::new(lo, 0.0, -0.32),
                    DVec3::new(hi, 0.0, -0.32),
                    DVec3::new(hi, 0.0, 0.32),
                    DVec3::new(lo, 0.0, 0.32),
                ],
                [
                    DVec3::new(lo, 0.85, -0.1),
                    DVec3::new(hi, 0.85, -0.1),
                    DVec3::new(hi, 0.85, 0.1),
                    DVec3::new(lo, 0.85, 0.1),
                ],
                concrete,
            );
            // A sawhorse behind it, striped yellow and black.
            let z = 1.6 + rng.range(-0.3, 0.3);
            for x in [lo + 0.2, hi - 0.2] {
                m.oriented_box(
                    DVec3::new(x, 0.45, z),
                    DVec3::Z,
                    DVec3::new(0.25, 0.45, 0.04),
                    wood,
                );
            }
            let n = ((hi - lo) / 0.45).floor().max(1.0) as usize;
            let w = (hi - lo) / n as f64;
            for k in 0..n {
                let x = lo + w * (k as f64 + 0.5);
                let c = if k % 2 == 0 { yellow } else { black };
                m.oriented_box(
                    DVec3::new(x, 0.9, z),
                    DVec3::Z,
                    DVec3::new(0.04, 0.12, w * 0.5),
                    c,
                );
            }
        }
        // Sandbags along the pavement, two layers.
        let (p0, p1) = (h + 0.3, h + b.pavement - 0.3);
        if p1 > p0 + 0.6 {
            for layer in 0..2 {
                let mut x = p0 + layer as f64 * 0.3;
                while x + 0.6 <= p1 {
                    let c = DVec3::new(sx * (x + 0.3), 0.14 + layer as f64 * 0.26, 0.4);
                    m.oriented_box(c, DVec3::Z, DVec3::new(0.2, 0.13, 0.3), sand);
                    x += 0.62;
                }
            }
        }
    }
    // An oil drum burning on one pavement.
    let side = if rng.chance(0.5) { -1.0 } else { 1.0 };
    let drum = DVec3::new(side * (h + b.pavement * 0.5), 0.0, -1.2);
    m.cylinder(drum, (0.3, 0.3), 0.9, 12, true, rgb(110, 50, 30));
    m.surf = Surf::Glow;
    m.cylinder(
        drum + DVec3::Y * 0.86,
        (0.27, 0.27),
        0.06,
        12,
        true,
        rgb(255, 140, 40),
    );
    m.surf = Surf::Plain;
    m.finish()
}
