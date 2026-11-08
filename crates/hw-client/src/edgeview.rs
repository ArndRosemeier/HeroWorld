//! The frayed edge of the mad scientist's quarter as the camera sees it:
//! graffiti on the walls ([`hw_gen::urban::edge::tags`]), and the pieces
//! of the barricades where the streets run in (street props,
//! [`hw_gen::urban::props::BarricadePiece`], drawn by
//! [`crate::propview`] with the meshes here, so they break like the rest).
//!
//! Graffiti are decals, which Engine keeps in a shared pool: only those
//! near the camera are spawned, and they are refreshed as it moves.

use crate::citymesh::{MeshBuilder, Surf};
use engine::prelude::*;
use glam::{DVec3, Vec3};
use hw_gen::urban::edge::{self, Tag, TAG_IMAGES};
use hw_gen::urban::props::BarricadePiece;
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
    decals: Vec<DecalId>,
    /// Where the camera was when the near graffiti were last picked.
    picked_at: Option<Vec3>,
}

impl EdgeView {
    pub fn new(town: &TownWorld) -> Self {
        Self {
            tags: edge::tags(town),
            decals: Vec::new(),
            picked_at: None,
        }
    }

    pub fn update(&mut self, world: &mut World) -> EngineResult<()> {
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
        for d in self.decals.drain(..) {
            world.remove_decal(d);
        }
        self.picked_at = None;
    }
}

/// One barricade piece, facing +z, its length across x (see
/// [`BarricadePiece::block`]): close up and far.
pub fn piece_levels(piece: BarricadePiece) -> Vec<LodLevel> {
    vec![
        LodLevel::new(piece_mesh(piece, true), 120.0),
        LodLevel::new(piece_mesh(piece, false), 500.0),
    ]
}

fn piece_mesh(piece: BarricadePiece, near: bool) -> Mesh {
    let mut m = MeshBuilder::new();
    let seg = if near { 14 } else { 6 };
    let (hx, hz, h) = piece.block();
    match piece {
        BarricadePiece::Jersey => {
            // Concrete, wide at the foot, narrow on top, with a red and
            // white band.
            let q = |y: f64, d: f64| {
                [
                    DVec3::new(-hx, y, -d),
                    DVec3::new(hx, y, -d),
                    DVec3::new(hx, y, d),
                    DVec3::new(-hx, y, d),
                ]
            };
            let concrete = rgb(158, 156, 148);
            m.hexa(q(0.0, hz), q(0.25, hz * 0.95), concrete);
            m.hexa(q(0.25, hz * 0.95), q(h, hz * 0.32), concrete);
            if near {
                let n = 5;
                let w = hx * 2.0 / n as f64;
                for k in 0..n {
                    let x = -hx + w * (k as f64 + 0.5);
                    let c = if k % 2 == 0 {
                        rgb(200, 40, 40)
                    } else {
                        rgb(236, 234, 226)
                    };
                    m.oriented_box(
                        DVec3::new(x, h - 0.12, 0.0),
                        DVec3::Z,
                        DVec3::new(hz * 0.33, 0.08, w * 0.5),
                        c,
                    );
                }
            }
        }
        BarricadePiece::Sawhorse => {
            let wood = rgb(120, 92, 60);
            for x in [-hx + 0.15, hx - 0.15] {
                for dz in [-1.0, 1.0] {
                    let foot = DVec3::new(x, 0.0, dz * hz);
                    let top = DVec3::new(x, h - 0.15, 0.0);
                    crate::landmarkview::strut(&mut m, foot, top, 0.07, wood);
                }
            }
            // The plank, striped yellow and black.
            let n = 6;
            let w = hx * 2.0 / n as f64;
            for k in 0..n {
                let x = -hx + w * (k as f64 + 0.5);
                let c = if k % 2 == 0 {
                    rgb(235, 190, 40)
                } else {
                    rgb(30, 30, 30)
                };
                m.oriented_box(
                    DVec3::new(x, h - 0.12, 0.0),
                    DVec3::Z,
                    DVec3::new(0.04, 0.12, w * 0.5),
                    c,
                );
            }
        }
        BarricadePiece::Sandbags => {
            let sand = [rgb(170, 150, 110), rgb(156, 138, 100)];
            for layer in 0..3 {
                let shift = if layer % 2 == 0 { 0.0 } else { 0.2 };
                let mut x = -hx + 0.2 + shift;
                let mut k = 0;
                while x + 0.2 <= hx + 1e-6 {
                    let c = DVec3::new(x, 0.1 + layer as f64 * 0.18, 0.0);
                    m.oriented_box(
                        c,
                        DVec3::Z,
                        DVec3::new(hz * 0.9, 0.1, 0.2),
                        sand[(k + layer) % 2],
                    );
                    x += 0.4;
                    k += 1;
                }
            }
        }
        BarricadePiece::Drum => {
            m.cylinder(DVec3::ZERO, (hx, hx), h - 0.04, seg, true, rgb(110, 50, 30));
            for y in [0.25, 0.6] {
                m.cylinder(
                    DVec3::Y * y,
                    (hx + 0.015, hx + 0.015),
                    0.04,
                    seg,
                    false,
                    rgb(80, 40, 26),
                );
            }
            // The fire inside.
            m.surf = Surf::Glow;
            m.cylinder(
                DVec3::Y * (h - 0.06),
                (hx * 0.9, hx * 0.9),
                0.06,
                seg,
                true,
                rgb(255, 140, 40),
            );
            m.surf = Surf::Plain;
        }
    }
    m.finish()
}
