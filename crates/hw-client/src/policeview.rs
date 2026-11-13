//! The police on screen: the Sky Patrol cruiser the server flies in (see
//! [`hw_sim::police`]) with its canopy swinging up and its beacon turning,
//! and the officers' peaked caps.
//!
//! The cruiser's model is [`crate::carmesh::patrol_into`]; what a hero
//! bumps into is [`hw_sim::police::cruiser_solid`], the box that model is
//! built in. Officers are Actors people ([`crate::people::officer`]); the
//! cap is hung on the head bone like the villains' helmets.

use crate::carmesh::{inside, lathe, surface, tube, Lod, Paint};
use crate::citymesh::{MeshBuilder, Surf};
use engine::lamps::LampId;
use engine::prelude::*;
use glam::DVec3;
use hw_core::NetId;
use hw_net::{EntityKind, EntityState};
use std::collections::BTreeMap;
use std::f64::consts::{PI, TAU};

fn plain(c: Color) -> Paint {
    Some((Surf::Plain, c))
}

fn metal(c: Color) -> Paint {
    Some((Surf::Metal, c))
}

/// Where the cap sits on the head bone, and how far it tips forward
/// (degrees).
pub const CAP_AT: (f32, f32, f32) = (0.0, 0.068, 0.004);
pub const CAP_TIP: f32 = -4.0;
/// The cap's size on the head (the mesh is a small head's).
pub const CAP_SIZE: f32 = 1.12;

/// An officer's peaked cap: a black band round the head, a navy crown
/// wider than the band and raised at the front, a patent-black peak, a
/// gold cord over the peak and a gold badge on the crown's front. Origin
/// at the middle of the band's lower edge, front to +z.
pub fn cap() -> Mesh {
    let mut b = MeshBuilder::new();
    let navy = rgb(26, 38, 74);
    let black = rgb(16, 16, 20);
    let gold = rgb(214, 172, 70);
    let seg = 40;
    // A head is longer than it is wide: so is the band.
    let ring = |rx: f64, rz: f64, y: f64, lift: f64| -> Vec<DVec3> {
        (0..seg)
            .map(|k| {
                let a = TAU * k as f64 / seg as f64;
                // The crown stands taller at the front: its top is a
                // flat sheet tipped up there.
                let z = rz * a.cos();
                let front = z / (0.106 * 1.33) * 0.5 + 0.5;
                DVec3::new(rx * a.sin(), y + lift * front, z)
            })
            .collect()
    };
    let (rx, rz) = (0.088, 0.106);
    let rows = vec![
        ring(rx * 0.96, rz * 0.96, -0.004, 0.0),
        ring(rx, rz, 0.0, 0.0),
        ring(rx * 1.01, rz * 1.01, 0.036, 0.0),
        ring(rx * 1.03, rz * 1.03, 0.044, 0.004),
        ring(rx * 1.32, rz * 1.26, 0.068, 0.03),
        ring(rx * 1.4, rz * 1.33, 0.078, 0.034),
        ring(rx * 1.36, rz * 1.3, 0.088, 0.036),
        ring(rx * 0.9, rz * 0.88, 0.094, 0.036),
        ring(rx * 0.3, rz * 0.3, 0.096, 0.036),
        ring(0.0, 0.0, 0.096, 0.036),
    ];
    surface(&mut b, &rows, true, |i, _| match i {
        0..=2 => metal(black),
        _ => plain(navy),
    });
    // The peak: a stiff crescent out from the band's front, dipping.
    let peak = |under: bool| -> Vec<Vec<DVec3>> {
        let arc = 22;
        (0..=4)
            .map(|j| {
                let t = j as f64 / 4.0;
                (0..=arc)
                    .map(|k| {
                        let u = k as f64 / arc as f64;
                        let a = (u - 0.5) * PI * 0.92;
                        // Widest in the middle, to nothing at its ends.
                        let reach = 0.07 * (a.cos()).powf(0.7) * t;
                        let r = 1.0 + reach / rz;
                        DVec3::new(
                            rx * 1.01 * a.sin() * (1.0 + reach * 4.0),
                            0.004 - reach * 0.42 - if under { 0.004 } else { 0.0 },
                            rz * 1.01 * a.cos() * r,
                        )
                    })
                    .collect()
            })
            .collect()
    };
    surface(&mut b, &peak(false), false, |_, _| metal(black));
    inside(|| {
        surface(&mut b, &peak(true), false, |_, _| plain(black));
    });
    // The cord over the peak, a button at each end.
    let cord: Vec<DVec3> = (0..=16)
        .map(|k| {
            let a = (k as f64 / 16.0 - 0.5) * PI * 0.86;
            DVec3::new(rx * 1.05 * a.sin(), 0.012, rz * 1.05 * a.cos())
        })
        .collect();
    tube(&mut b, &cord, (0.0045, 0.0045), 6, metal(gold));
    for end in [cord[0], cord[16]] {
        lathe(
            &mut b,
            end,
            DVec3::new(end.x.signum(), 0.0, 0.25).normalize(),
            &[(0.0, -0.002), (0.008, 0.0), (0.007, 0.004), (0.0, 0.006)],
            10,
            |_| metal(gold),
        );
    }
    // The badge: a gold shield with a domed boss on the crown's front.
    let front = DVec3::new(0.0, 0.066, rz * 1.17);
    let out = DVec3::new(0.0, 0.5, 1.0).normalize();
    lathe(
        &mut b,
        front,
        out,
        &[
            (0.0, 0.0),
            (0.026, 0.0),
            (0.027, 0.003),
            (0.02, 0.006),
            (0.012, 0.007),
            (0.009, 0.012),
            (0.0, 0.014),
        ],
        5,
        |_| metal(gold),
    );
    b.finish()
}

/// One cruiser on screen.
struct Shown {
    body: EntityId,
    canopy: EntityId,
    /// How far the canopy is open, 0 to 1.
    open: f32,
    /// The beacon's lamps: red, sweeping round.
    lamps: [LampId; 2],
}

/// The cruisers the server has out, drawn.
#[derive(Default)]
pub struct PoliceView {
    shown: BTreeMap<NetId, Shown>,
    meshes: Option<(Mesh, Mesh)>,
    clock: f32,
}

/// How far the canopy swings up (degrees).
const CANOPY_SWING: f32 = 52.0;

impl PoliceView {
    /// Draws every cruiser in `all` where the server has it, and takes
    /// away the ones that are gone.
    pub fn update(&mut self, world: &mut World, all: &[EntityState], dt: f32) -> EngineResult<()> {
        self.clock += dt;
        let cars: Vec<&EntityState> = all
            .iter()
            .filter(|e| e.kind == EntityKind::PoliceCar)
            .collect();
        let gone: Vec<NetId> = self
            .shown
            .keys()
            .filter(|id| !cars.iter().any(|c| c.id == **id))
            .copied()
            .collect();
        for id in gone {
            if let Some(s) = self.shown.remove(&id) {
                for l in s.lamps {
                    world.remove_lamp(l);
                }
                world.despawn(s.canopy);
                world.despawn(s.body);
            }
        }
        if cars.is_empty() {
            return Ok(());
        }
        let (hinge, beacon) = {
            let (_, hinge, _) = crate::carmesh::patrol_door(Lod::Far);
            (hinge.as_vec3(), crate::carmesh::patrol_beacon().as_vec3())
        };
        for c in cars {
            // Physics-style: the server's rotation tips it as the ground
            // lies where it stands.
            let (y, x, z) = glam::Quat::from_array(c.rot).to_euler(glam::EulerRot::YXZ);
            let place = Place::new(c.pos[0] as f32, c.pos[1] as f32, c.pos[2] as f32)
                .with_yaw_deg(y.to_degrees())
                .with_pitch_deg(x.to_degrees())
                .with_roll_deg(z.to_degrees());
            if !self.shown.contains_key(&c.id) {
                let (body, canopy) = self.meshes.get_or_insert_with(|| {
                    let mut b = MeshBuilder::new();
                    crate::carmesh::patrol_into(&mut b, Lod::Near, false);
                    (b.finish(), crate::carmesh::patrol_door(Lod::Near).0)
                });
                let body = world.place(body.clone(), place)?;
                let canopy = world.place(canopy.clone(), Place::new(0.0, 0.0, 0.0))?;
                world.attach(canopy, body, Place::new(hinge.x, hinge.y, hinge.z))?;
                let red = rgb(255, 40, 30);
                let lamps = [
                    world.add_lamp(Lamp::point(beacon, red, 0.0, 26.0))?,
                    world.add_lamp(Lamp::point(beacon, red, 0.0, 26.0))?,
                ];
                self.shown.insert(
                    c.id,
                    Shown {
                        body,
                        canopy,
                        open: 0.0,
                        lamps,
                    },
                );
            }
            let Some(s) = self.shown.get_mut(&c.id) else {
                continue;
            };
            world.set_place(s.body, place)?;
            // The canopy swings up on its front hinge while the server
            // has it open (`grounded`), and shuts before it flies.
            let step = dt / hw_sim::police::CANOPY as f32;
            s.open = if c.grounded {
                (s.open + step).min(1.0)
            } else {
                (s.open - step).max(0.0)
            };
            let eased = s.open * s.open * (3.0 - 2.0 * s.open);
            world.set_place(
                s.canopy,
                Place::new(hinge.x, hinge.y, hinge.z).with_pitch_deg(eased * CANOPY_SWING),
            )?;
            // The beacon: two red beams sweeping round, on the canopy.
            let turn = glam::Quat::from_array(c.rot);
            let lift = glam::Quat::from_rotation_x((eased * CANOPY_SWING).to_radians());
            let dome = hinge + lift * (beacon - hinge);
            let at = Vec3::new(c.pos[0] as f32, c.pos[1] as f32, c.pos[2] as f32) + turn * dome;
            let a = self.clock * 7.0 + c.id.0 as f32;
            for (k, &l) in s.lamps.iter().enumerate() {
                let a = a + k as f32 * std::f32::consts::PI;
                let dir = turn * Vec3::new(a.sin(), -0.12, a.cos());
                world.set_lamp(
                    l,
                    Lamp::spot(at + dir * 0.2, dir, 38.0, rgb(255, 40, 30), 55.0, 30.0),
                )?;
            }
        }
        Ok(())
    }
}
