//! The Herobots' side of the server: two robots that stand on Hero Hall's
//! plaza next to the town's heroes, until a player wakes one.
//!
//! A woken Herobot is a player like any other: a client (see
//! [`crate::herobot`]) joins over UDP with a hero file, earns karma, levels
//! up, is carried to the hospital by the drone when it is beaten and counts
//! towards the ten players. This module only keeps what is *not* a player:
//! the idle robot on the plaza, which slot is out, and the mark that makes
//! a woken one's hero draw as a robot on every client.

use crate::{Body, Ids};
use crate::{Hero, LastChange, Motion, Net, Orientation, Sim};
use bevy_ecs::prelude::*;
use glam::{DVec2, DVec3};
use hw_core::NetId;
use hw_net::PlayerId;

/// How many Herobots there are.
pub const SLOTS: usize = 2;
/// How near a player stands to wake a Herobot or send it home (m).
pub const REACH: f64 = 8.0;

/// A Herobot's name as the other players see it (slot 0 is "Herobot 1").
pub fn name(slot: u8) -> String {
    format!("Herobot {}", slot as usize + 1)
}

/// The slot a name belongs to, if it is a Herobot's.
pub fn slot_of_name(name: &str) -> Option<u8> {
    (0..SLOTS as u8).find(|&s| name == self::name(s))
}

/// A Herobot standing on the plaza, not woken.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct IdleBot {
    pub slot: u8,
}

/// Marks the hero of a woken Herobot.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct BotHero {
    pub slot: u8,
}

/// Where each Herobot stands when it is not out.
#[derive(Resource, Default, Clone, Debug)]
pub struct Plaza {
    pub spots: Vec<(DVec3, DVec2)>,
}

fn yaw_quat(dir: DVec2) -> [f32; 4] {
    glam::Quat::from_rotation_y(hw_core::det::atan2(dir.x, dir.y) as f32).to_array()
}

/// Puts the idle Herobots on their spots (Hero Hall's plaza).
pub fn spawn(world: &mut World, tick: u64, spots: &[(DVec3, DVec2)]) {
    world.insert_resource(Plaza {
        spots: spots.iter().take(SLOTS).copied().collect(),
    });
    for slot in 0..spots.len().min(SLOTS) as u8 {
        stand(world, tick, slot);
    }
}

/// Stands the Herobot of `slot` on its spot, unless it already does.
pub fn stand(world: &mut World, tick: u64, slot: u8) -> bool {
    let Some(&(home, facing)) = world
        .get_resource::<Plaza>()
        .and_then(|p| p.spots.get(slot as usize))
    else {
        return false;
    };
    if world
        .query::<&IdleBot>()
        .iter(world)
        .any(|b| b.slot == slot)
    {
        return false;
    }
    let id = world.resource_mut::<Ids>().0.allocate();
    world.spawn((
        Net(id),
        Motion(Body {
            pos: home,
            vel: DVec3::ZERO,
            grounded: true,
        }),
        Orientation(yaw_quat(facing)),
        IdleBot { slot },
        LastChange(tick),
    ));
    true
}

impl Sim {
    /// Where the idle Herobot of `slot` stands, if it is on the plaza.
    pub fn herobot_idle_at(&mut self, slot: u8) -> Option<DVec3> {
        self.world
            .query::<(&IdleBot, &Motion)>()
            .iter(&self.world)
            .find(|(b, _)| b.slot == slot)
            .map(|(_, m)| m.0.pos)
    }

    /// Where the Herobot of `slot` stands when it is at home, and which
    /// way it faces.
    pub fn herobot_home(&self, slot: u8) -> Option<(DVec3, DVec2)> {
        self.world
            .get_resource::<Plaza>()
            .and_then(|p| p.spots.get(slot as usize).copied())
    }

    /// Takes the idle Herobot of `slot` off the plaza (it is woken).
    /// False if it was not there.
    pub fn herobot_leave(&mut self, slot: u8) -> bool {
        let found: Vec<Entity> = self
            .world
            .query::<(Entity, &IdleBot)>()
            .iter(&self.world)
            .filter(|(_, b)| b.slot == slot)
            .map(|(e, _)| e)
            .collect();
        let any = !found.is_empty();
        for e in found {
            self.world.despawn(e);
        }
        any
    }

    /// Puts the Herobot of `slot` back on the plaza.
    pub fn herobot_stand(&mut self, slot: u8) {
        let tick = self.tick;
        stand(&mut self.world, tick, slot);
    }

    /// Marks `owner`'s hero as the hero of the Herobot of `slot`.
    pub fn tag_herobot(&mut self, owner: PlayerId, slot: u8) {
        let found: Vec<Entity> = self
            .world
            .query::<(Entity, &Hero)>()
            .iter(&self.world)
            .filter(|(_, h)| h.owner == owner)
            .map(|(e, _)| e)
            .collect();
        for e in found {
            self.world.entity_mut(e).insert(BotHero { slot });
        }
    }

    /// Which of the Herobots (by slot) have a hero in the world now.
    pub fn herobots_out(&mut self) -> Vec<(u8, NetId)> {
        self.world
            .query::<(&BotHero, &Net)>()
            .iter(&self.world)
            .map(|(b, n)| (b.slot, n.0))
            .collect()
    }
}
