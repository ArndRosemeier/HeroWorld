//! The frayed edge of the mad scientist's quarter as the camera sees it:
//! graffiti on the walls ([`hw_gen::urban::edge::tags`]), and the pieces
//! of the barricades where the streets run in (street props,
//! [`hw_gen::urban::props::BarricadePiece`], drawn by
//! [`crate::propview`] with the meshes here, so they break like the rest).
//!
//! Graffiti are decals, which Engine keeps in a shared pool: only those
//! near the camera are spawned, and they are refreshed as it moves.

use engine::prelude::*;
use glam::Vec3;
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

pub(crate) fn kinds(world: &mut World) -> Option<Vec<DecalKind>> {
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
    /// Whether the town has a villains' quarter at all.
    any: bool,
    decals: Vec<DecalId>,
    /// Where the camera was when the near graffiti were last picked.
    picked_at: Option<Vec3>,
}

impl EdgeView {
    pub fn new(town: &TownWorld) -> Self {
        Self {
            any: town.town.dystopia.is_some(),
            decals: Vec::new(),
            picked_at: None,
        }
    }

    /// Draws the graffiti near the camera, from the buildings the town
    /// has planned there.
    pub fn update(&mut self, world: &mut World, town: &TownWorld) -> EngineResult<()> {
        if !self.any {
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
        let c = glam::DVec2::new(eye.x as f64, eye.z as f64);
        let r = glam::DVec2::splat(TAG_REACH as f64 + 40.0);
        let quarters = town.quarters_in(c - r, c + r);
        let tags: Vec<Tag> = edge::tags(town, quarters.iter().flat_map(|q| q.buildings_with_ids()));
        let mut near: Vec<(f32, &Tag)> = tags
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
    use crate::carmesh::Lod;
    use crate::propmesh::barricade;
    vec![
        LodLevel::new(barricade(piece, Lod::Near), 25.0),
        LodLevel::new(barricade(piece, Lod::Mid), 120.0),
        LodLevel::new(barricade(piece, Lod::Far), 500.0),
    ]
}
