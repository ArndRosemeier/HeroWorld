//! The gang's safes in the hideouts (a hideout case's thing is locked in
//! one): each drawn where the server has it, shut until its door is
//! broken, then open. What is drawn is `interior::pieces::vault`, whose
//! box is the solid the server (and a joined player's own hero) bumps
//! into: `hw_sim::missions::safe_solid`.

use crate::citymesh::MeshBuilder;
use crate::interior::pieces::{vault, Maker};
use engine::prelude::*;
use glam::{DVec2, DVec3};
use hw_core::NetId;
use hw_net::{EntityKind, EntityState, Status};
use hw_sim::missions::{safe_middle, safe_solid, SAFE_HEIGHT};
use std::collections::BTreeMap;

/// One safe: shut or open (a few hundred boxes and turned parts, made in
/// well under a millisecond, so on the frame's own thread: there is one
/// per case).
pub fn safe_mesh(open: bool) -> Mesh {
    let mut b = MeshBuilder::new();
    let mut m = Maker::new(&mut b, DVec3::ZERO, DVec3::Z, 24, crate::skins::Wear::Fresh);
    vault(&mut m, if open { 1.0 } else { 0.0 });
    b.finish()
}

/// Where a safe entity's box stands: its middle on the floor and the way
/// its door faces.
pub fn stands(e: &EntityState) -> (DVec3, DVec2) {
    let yaw = hw_sim::robots::rotation_yaw(e.rot);
    let facing = DVec2::new(yaw.sin(), yaw.cos());
    let front = DVec3::new(e.pos[0], e.pos[1], e.pos[2]);
    (safe_middle(front, facing), facing)
}

/// The solid boxes of the safes in `all` (for a joined player's hero: the
/// host puts its own into the world).
pub fn solids(all: &[EntityState]) -> Vec<hw_sim::collision::Solid> {
    all.iter()
        .filter(|e| e.kind == EntityKind::Safe)
        .map(|e| {
            let (at, facing) = stands(e);
            safe_solid(at, facing)
        })
        .collect()
}

#[derive(Default)]
pub struct SafeView {
    drawn: BTreeMap<NetId, (EntityId, bool)>,
}

impl SafeView {
    /// Draws every safe in `all`, and forgets the ones that are gone.
    pub fn update(&mut self, world: &mut World, all: &[EntityState]) -> EngineResult<()> {
        let safes: Vec<&EntityState> = all.iter().filter(|e| e.kind == EntityKind::Safe).collect();
        let gone: Vec<NetId> = self
            .drawn
            .keys()
            .filter(|id| !safes.iter().any(|s| s.id == **id))
            .copied()
            .collect();
        for id in gone {
            if let Some((e, _)) = self.drawn.remove(&id) {
                world.despawn(e);
            }
        }
        for s in safes {
            let open = s.combat.is_some_and(|c| c.status == Status::Down);
            let was = self.drawn.get(&s.id).map(|d| d.1);
            if was == Some(open) {
                continue;
            }
            if let Some((e, _)) = self.drawn.remove(&s.id) {
                world.despawn(e);
            }
            let (at, facing) = stands(s);
            let yaw = facing.x.atan2(facing.y).to_degrees() as f32;
            let e = world.place(
                safe_mesh(open),
                Place::new(at.x as f32, at.y as f32, at.z as f32).with_yaw_deg(yaw),
            )?;
            self.drawn.insert(s.id, (e, open));
            // The door gives: a flash and sparks at the lock.
            if was == Some(false) && open {
                let lock = Vec3::new(s.pos[0] as f32, s.pos[1] as f32, s.pos[2] as f32)
                    + Vec3::Y * (SAFE_HEIGHT as f32 * 0.55);
                crate::fightview::glow_at(world, lock, rgb(255, 214, 120), 0.5);
                crate::fightview::glow_at(world, lock, rgb(255, 250, 235), 0.25);
            }
        }
        Ok(())
    }

    pub fn clear(&mut self, world: &mut World) {
        for (_, (e, _)) in std::mem::take(&mut self.drawn) {
            world.despawn(e);
        }
    }
}
