//! The mooring mast alive: the liner *Skylark* on its round over the town
//! ([`hw_gen::urban::mast::MastRoute`], a pure function of the game clock,
//! so every player sees it in the same place), its propellers turning,
//! and while it is moored the mast's work: the head turning with the ship
//! as it swings, the gangway sliding out to its nose, the lift running
//! passengers up and down, two crates winched down from the cargo hatch
//! onto the baggage cart, and the cart trundling them off to the terminal.
//! The red beacon on top blinks all the while.
//!
//! The ship is always in the world (its far level costs a few hundred
//! triangles), so it can be seen sailing over the town from anywhere; the
//! mast's moving parts are made when the camera comes within [`LOAD`] of
//! the mast and dropped past [`DROP`]. Everything is an instanced entity or
//! a LOD set moved by its places: nothing is built while it plays.

use crate::carmesh::Lod;
use crate::mastmesh as mm;
use engine::prelude::*;
use glam::{DMat4, DQuat, DVec2, DVec3, Vec3};
use hw_gen::urban::landmarks::{Landmark, LandmarkKind};
use hw_gen::urban::mast::{
    MastRoute, Phase, ShipPose, CARGO_Z, DOCKED_S, GALLERY_Y, TERMINAL_HALF, TERMINAL_Z,
};
use hw_gen::urban::world::TownWorld;

/// The mast's moving parts are made within this distance (m) ...
const LOAD: f32 = 1500.0;
/// ... and dropped past this.
const DROP: f32 = 1700.0;
/// Propellers are drawn this near the ship.
const PROPS_NEAR: f32 = 420.0;

/// `HW_MAST_T`: seconds into the liner's cycle the views start at (0 just
/// moored), if set.
pub fn view_t() -> f64 {
    std::env::var("HW_MAST_T")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(400.0)
}

/// What the soundscape hears of the airfield this frame.
#[derive(Clone, Debug, Default)]
pub struct Heard {
    /// The ship's engines: where, and how hard they work (0..1).
    pub engines: Option<(Vec3, f32)>,
    /// The winch running, where.
    pub winch: Option<Vec3>,
    /// One-off sounds since the last frame: the horn, the mooring latch.
    pub horns: Vec<Vec3>,
    pub clanks: Vec<Vec3>,
}

/// Seconds into the moored spell when each thing happens.
mod cue {
    /// The gangway slides out, and back in before casting off.
    pub const GANGWAY_OUT: (f64, f64) = (6.0, 12.0);
    pub const GANGWAY_IN: (f64, f64) = (150.0, 156.0);
    /// Lift runs: (start, end, from height share, to height share).
    pub const LIFT: [(f64, f64, f64, f64); 4] = [
        (14.0, 36.0, 0.0, 1.0),
        (48.0, 70.0, 1.0, 0.0),
        (84.0, 106.0, 0.0, 1.0),
        (118.0, 140.0, 1.0, 0.0),
    ];
    /// Each crate: lowered (start, end), then the hook goes back up.
    pub const CRATES: [(f64, f64); 2] = [(22.0, 50.0), (66.0, 94.0)];
    pub const HOOK_UP_S: f64 = 14.0;
    /// The cart: off to the terminal, in through its door, back out.
    pub const CART_GO: (f64, f64) = (112.0, 132.0);
    pub const CART_BACK: (f64, f64) = (140.0, 160.0);
    /// The horn: on the glide in and as it casts off.
    pub const HORN_APPROACH: f64 = 8.0;
    pub const HORN_CASTOFF: f64 = 2.0;
}

/// The baggage cart's way from the cargo square round the mast to the
/// terminal's side door (mast frame).
fn cart_path() -> Vec<DVec2> {
    let door = DVec2::new(TERMINAL_HALF.0 + 1.8, TERMINAL_Z);
    vec![
        DVec2::new(0.0, CARGO_Z),
        DVec2::new(5.0, CARGO_Z - 1.0),
        DVec2::new(9.5, 10.5),
        DVec2::new(11.5, 1.0),
        DVec2::new(11.0, -8.0),
        door + DVec2::new(1.5, 2.0),
        door,
    ]
}

/// The point `d` metres along a path, and the way it heads there.
fn along(path: &[DVec2], d: f64) -> (DVec2, DVec2) {
    let mut left = d.max(0.0);
    for w in path.windows(2) {
        let len = w[0].distance(w[1]);
        if left <= len {
            let dir = (w[1] - w[0]).normalize_or(DVec2::Y);
            return (w[0] + dir * left, dir);
        }
        left -= len;
    }
    let n = path.len();
    (
        path[n - 1],
        (path[n - 1] - path[n - 2]).normalize_or(DVec2::Y),
    )
}

fn path_len(path: &[DVec2]) -> f64 {
    path.windows(2).map(|w| w[0].distance(w[1])).sum()
}

fn smooth(u: f64) -> f64 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// How far between `a` and `b` (seconds) `t` is, eased.
fn ease(t: f64, (a, b): (f64, f64)) -> f64 {
    smooth((t - a) / (b - a))
}

/// What the mast's parts are doing `t` seconds into the cycle (0 moored).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Works {
    /// How far the gangway is out (0..1).
    pub gangway: f64,
    /// The lift car's height share (0 foot .. 1 gallery).
    pub lift: f64,
    /// Each crate: on the hook at this share of the drop (0 hatch .. 1
    /// down), on the cart, or not there.
    pub crates: [Crate; 2],
    /// The hook's share of its drop, while it is down or coming up.
    pub hook: Option<f64>,
    /// How far along its way the cart is (m), and going back.
    pub cart: f64,
    pub cart_back: bool,
    /// The winch is running.
    pub winch: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Crate {
    Gone,
    Hanging(f64),
    OnCart,
}

/// The mast's choreography at cycle second `t`.
pub fn works(t: f64) -> Works {
    let docked = t < DOCKED_S;
    let gangway = if docked {
        ease(t, cue::GANGWAY_OUT) * (1.0 - ease(t, cue::GANGWAY_IN))
    } else {
        0.0
    };
    let mut lift = 0.0;
    for (a, b, from, to) in cue::LIFT {
        if t >= a {
            lift = from + (to - from) * ease(t, (a, b));
        }
    }
    if !docked {
        lift = 0.0;
    }
    let len = path_len(&cart_path());
    let (cart, cart_back) = if t < cue::CART_GO.0 {
        (0.0, false)
    } else if t < cue::CART_BACK.0 {
        (len * ease(t, cue::CART_GO), false)
    } else {
        (len * (1.0 - ease(t, cue::CART_BACK)), true)
    };
    let mut crates = [Crate::Gone; 2];
    let mut hook = None;
    let mut winch = false;
    for (k, (a, b)) in cue::CRATES.into_iter().enumerate() {
        crates[k] = if t < a {
            Crate::Gone
        } else if t < b {
            hook = Some((t - a) / (b - a));
            winch = true;
            Crate::Hanging((t - a) / (b - a))
        } else if t < cue::CART_GO.1 {
            if t < b + cue::HOOK_UP_S {
                hook = Some(1.0 - (t - b) / cue::HOOK_UP_S);
                winch = true;
            }
            Crate::OnCart
        } else {
            Crate::Gone
        };
    }
    Works {
        gangway,
        lift,
        crates,
        hook,
        cart,
        cart_back,
        winch,
    }
}

/// The ship's transform (nose origin) from its pose.
fn ship_matrix(p: &ShipPose) -> DMat4 {
    let q = DQuat::from_rotation_y(p.yaw)
        * DQuat::from_rotation_x(-p.pitch)
        * DQuat::from_rotation_z(p.roll);
    DMat4::from_rotation_translation(q, p.nose)
}

fn ship_place(p: &ShipPose, extra_roll: f64, at: DVec3) -> Place {
    Place::new(at.x as f32, at.y as f32, at.z as f32)
        .with_yaw_deg(p.yaw.to_degrees() as f32)
        .with_pitch_deg(-p.pitch.to_degrees() as f32)
        .with_roll_deg((p.roll + extra_roll).to_degrees() as f32)
}

struct Live {
    head: LodId,
    beacon: EntityId,
    gangway: EntityId,
    lift: EntityId,
    crates: EntityId,
    cable: EntityId,
    cart: EntityId,
    lamp: LampId,
}

pub struct MastView {
    mast: Option<Landmark>,
    route: Option<MastRoute>,
    ship: Option<LodId>,
    props: Option<EntityId>,
    live: Option<Live>,
    spin: f64,
    /// `HW_MAST_T=<s>` holds the liner that many seconds into its cycle
    /// (0 just moored), for screenshots; the clock does not run then.
    held: Option<f64>,
    /// The last frame's cycle second, to catch cues passing.
    last_t: Option<f64>,
    heard: Heard,
}

impl MastView {
    pub fn new(town: &TownWorld) -> Self {
        let mast = town
            .landmarks
            .iter()
            .find(|l| l.kind == LandmarkKind::MooringMast)
            .cloned();
        let route = mast.as_ref().map(|m| MastRoute::new(town, m));
        let held = route.as_ref().and_then(|r| {
            let set = std::env::var("HW_MAST_T").is_ok()
                || std::env::var("HW_VIEW").is_ok_and(|v| v == "liner");
            set.then(|| r.when(0.0, view_t()))
        });
        Self {
            held,
            mast,
            route,
            ship: None,
            props: None,
            live: None,
            spin: 0.0,
            last_t: None,
            heard: Heard::default(),
        }
    }

    /// The route, for views and tests.
    pub fn route(&self) -> Option<&MastRoute> {
        self.route.as_ref()
    }

    /// Is the mast's live part made (for the memory checks)?
    pub fn is_live(&self) -> bool {
        self.live.is_some()
    }

    /// What the soundscape should hear this frame (one-offs handed over once).
    pub fn heard(&mut self) -> Heard {
        let horns = std::mem::take(&mut self.heard.horns);
        let clanks = std::mem::take(&mut self.heard.clanks);
        Heard {
            horns,
            clanks,
            ..self.heard.clone()
        }
    }

    fn spawn_live(world: &mut World) -> EngineResult<Live> {
        let make = |world: &mut World, mesh: Mesh| world.spawn_instanced(mesh);
        let head = world.spawn_lod_at(mm::head_levels(), &[])?;
        let beacon = make(world, mm::beacon());
        world.set_casts_shadow(beacon, false)?;
        let lamp = world.add_lamp(Lamp::point(Vec3::ZERO, rgb(255, 40, 30), 0.0, 40.0))?;
        Ok(Live {
            head,
            beacon,
            gangway: make(world, mm::gangway()),
            lift: make(world, mm::lift_car()),
            crates: make(world, mm::crate_mesh()),
            cable: make(world, mm::cable()),
            cart: make(world, mm::cart()),
            lamp,
        })
    }

    pub fn clear(&mut self, world: &mut World) {
        self.drop_live(world);
        if let Some(id) = self.ship.take() {
            world.despawn_lod(id);
        }
        if let Some(id) = self.props.take() {
            world.despawn(id);
        }
        self.heard = Heard::default();
        self.last_t = None;
    }

    fn drop_live(&mut self, world: &mut World) {
        if let Some(l) = self.live.take() {
            world.despawn_lod(l.head);
            for e in [l.beacon, l.gangway, l.lift, l.crates, l.cable, l.cart] {
                world.despawn(e);
            }
            world.remove_lamp(l.lamp);
        }
    }

    /// Moves everything to game second `game_s`.
    pub fn update(&mut self, world: &mut World, game_s: f64, dt: f32) -> EngineResult<()> {
        let (Some(mast), Some(route)) = (self.mast.clone(), self.route.clone()) else {
            return Ok(());
        };
        let eye = world.camera().eye();
        let game_s = self.held.unwrap_or(game_s);
        let pose = route.pose(game_s);
        let ship_m = ship_matrix(&pose);
        // The ship, always.
        let place = ship_place(&pose, 0.0, pose.nose);
        match self.ship {
            Some(id) => world.set_lod_places(id, &[place])?,
            None => self.ship = Some(world.spawn_lod_at(mm::ship_levels(), &[place])?),
        }
        // Its propellers, near it.
        let rpm = 0.6 + 2.4 * pose.thrust;
        self.spin =
            (self.spin + rpm * std::f64::consts::TAU * dt as f64).rem_euclid(std::f64::consts::TAU);
        let mid =
            ship_m.transform_point3(DVec3::new(0.0, 0.0, -hw_gen::urban::mast::SHIP_LEN * 0.5));
        let near_ship =
            Vec3::new(mid.x as f32, mid.y as f32, mid.z as f32).distance(eye) < PROPS_NEAR;
        if self.props.is_none() {
            self.props = Some(world.spawn_instanced(mm::propeller(Lod::Near)));
        }
        let props: Vec<Place> = if near_ship {
            mm::prop_hubs()
                .iter()
                .enumerate()
                .map(|(k, h)| {
                    let at = ship_m.transform_point3(*h);
                    // Port and starboard turn opposite ways.
                    let s = if k % 2 == 0 { self.spin } else { -self.spin };
                    ship_place(&pose, s, at)
                })
                .collect()
        } else {
            Vec::new()
        };
        if let Some(id) = self.props {
            world.set_instances(id, &props)?;
        }
        let engines = Vec3::new(mid.x as f32, mid.y as f32, mid.z as f32);
        self.heard.engines = Some((engines, pose.thrust as f32));
        // Cues passing this frame.
        let t = route.cycle_t(game_s);
        let cup = route.cup;
        let cup32 = Vec3::new(cup.x as f32, cup.y as f32, cup.z as f32);
        let nose = Vec3::new(pose.nose.x as f32, pose.nose.y as f32, pose.nose.z as f32);
        if let Some(last) = self.last_t {
            // Only small steps count (a jump of the clock is not a cue).
            if t > last && t - last < 1.0 {
                let crossed = |c: f64| last < c && t >= c;
                let app = route.phase_start(Phase::Approach);
                let off = route.phase_start(Phase::CastOff);
                if crossed(app + cue::HORN_APPROACH) || crossed(off + cue::HORN_CASTOFF) {
                    self.heard.horns.push(nose);
                }
                // The latch: as the nose meets the cup, and as it lets go.
                if crossed(0.05) || crossed(off + 0.5) {
                    self.heard.clanks.push(cup32);
                }
                if crossed(cue::GANGWAY_OUT.1) || crossed(cue::GANGWAY_IN.1) {
                    self.heard.clanks.push(cup32 - Vec3::Y * 4.0);
                }
            }
        }
        self.last_t = Some(t);
        // The mast's moving parts, near it.
        let origin = Vec3::new(mast.pos.x as f32, mast.base as f32, mast.pos.y as f32);
        let far = origin.distance(eye);
        if far > DROP {
            self.drop_live(world);
            self.heard.winch = None;
            return Ok(());
        }
        if self.live.is_none() {
            if far > LOAD {
                self.heard.winch = None;
                return Ok(());
            }
            self.live = Some(Self::spawn_live(world)?);
        }
        let live = self.live.as_ref().expect("live");
        let w = works(t);
        let mast_yaw = mast.dir.x.atan2(mast.dir.y);
        // The head follows the moored ship's swing; free, it rests.
        let moored = (-mast.dir.x).atan2(-mast.dir.y);
        let swing = if pose.phase == Phase::Docked {
            wrap(pose.yaw - moored)
        } else {
            0.0
        };
        let head_yaw = mast_yaw + swing;
        let at_mast = |yaw: f64| {
            Place::new(origin.x, origin.y, origin.z).with_yaw_deg(yaw.to_degrees() as f32)
        };
        world.set_lod_places(live.head, &[at_mast(head_yaw)])?;
        // Blinking red: a second on, a second off.
        let on = (game_s % 2.0) < 1.0;
        let k = if on { 1.6 } else { 0.08 };
        world.set_instances(
            live.beacon,
            &[at_mast(head_yaw).with_tint(Color::rgb01_unchecked(k, k, k))],
        )?;
        let top = Vec3::new(0.0, hw_gen::urban::mast::MAST_H as f32 - 0.3, 0.0);
        world.set_lamp(
            live.lamp,
            Lamp::point(
                origin + top,
                rgb(255, 40, 30),
                if on { 25.0 } else { 0.0 },
                40.0,
            ),
        )?;
        // The gangway slides out along the head's axis.
        let slide = mm::GANGWAY_IN + (mm::GANGWAY_OUT - mm::GANGWAY_IN) * w.gangway;
        let to_world = |local: DVec3, yaw: f64| -> Vec3 {
            let (s, c) = yaw.sin_cos();
            let x = local.x * c + local.z * s;
            let z = -local.x * s + local.z * c;
            origin + Vec3::new(x as f32, local.y as f32, z as f32)
        };
        let g = to_world(DVec3::new(0.0, 0.0, slide), head_yaw);
        world.set_instances(
            live.gangway,
            &[Place::new(g.x, g.y, g.z).with_yaw_deg(head_yaw.to_degrees() as f32)],
        )?;
        // The lift car in its shaft.
        let car_top = GALLERY_Y - 2.5;
        let y = 0.05 + (car_top - 0.05) * w.lift;
        world.set_instances(
            live.lift,
            &[Place::new(origin.x, origin.y + y as f32, origin.z)
                .with_yaw_deg(mast_yaw.to_degrees() as f32)],
        )?;
        // The cart.
        let path = cart_path();
        let (p, d) = along(&path, w.cart);
        let heading = if w.cart_back { -d } else { d };
        let cart_yaw = mast_yaw + heading.x.atan2(heading.y);
        let cart_at = to_world(DVec3::new(p.x, 0.0, p.y), mast_yaw);
        world.set_instances(
            live.cart,
            &[Place::new(cart_at.x, cart_at.y, cart_at.z)
                .with_yaw_deg(cart_yaw.to_degrees() as f32)],
        )?;
        // Crates: on the hook from the hatch down to the cart, or riding it.
        let hatch = ship_m.transform_point3(mm::hatch());
        let hatch = Vec3::new(hatch.x as f32, hatch.y as f32, hatch.z as f32);
        let slot = |k: usize| -> Vec3 {
            let s = mm::CART_SLOTS[k];
            let (sn, cs) = (cart_yaw.sin(), cart_yaw.cos());
            let x = s.x * cs + s.z * sn;
            let z = -s.x * sn + s.z * cs;
            cart_at + Vec3::new(x as f32, s.y as f32, z as f32)
        };
        let hang = |k: usize, u: f64| -> Vec3 {
            let below = slot(k);
            let top = hatch - Vec3::Y * (mm::CRATE.1 as f32 + 1.0);
            let u = smooth(u) as f32;
            top.lerp(below, u)
        };
        let mut crates = Vec::new();
        for (k, c) in w.crates.iter().enumerate() {
            match c {
                Crate::Hanging(u) => crates.push(hang(k, *u)),
                Crate::OnCart => crates.push(slot(k)),
                Crate::Gone => {}
            }
        }
        let crate_places: Vec<Place> = crates
            .iter()
            .map(|p| Place::new(p.x, p.y, p.z).with_yaw_deg(cart_yaw.to_degrees() as f32))
            .collect();
        world.set_instances(live.crates, &crate_places)?;
        // The cable from the hatch down to the hook.
        let cable: Vec<Place> = match (w.hook, pose.phase) {
            (Some(u), Phase::Docked) => {
                let k = if t < cue::CRATES[1].0 { 0 } else { 1 };
                let hook = hang(k, u) + Vec3::Y * (mm::CRATE.1 as f32 + 0.8);
                let len = (hatch.y - hook.y).max(0.05);
                vec![Place::new(hook.x, hook.y, hook.z).with_stretch(Vec3::new(1.0, len, 1.0))]
            }
            _ => Vec::new(),
        };
        world.set_instances(live.cable, &cable)?;
        self.heard.winch = (w.winch && pose.phase == Phase::Docked).then_some(hatch);
        Ok(())
    }
}

fn wrap(a: f64) -> f64 {
    use std::f64::consts::{PI, TAU};
    (a + PI).rem_euclid(TAU) - PI
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn the_cart_keeps_off_the_mast_and_terminal() {
        use hw_gen::urban::mast::mast_footprints;
        let path = cart_path();
        let len = path_len(&path);
        let fps = mast_footprints();
        let mut d = 0.0;
        while d < len - 2.5 {
            let (p, _) = along(&path, d);
            for f in &fps {
                if f.span().0 < 1.5 {
                    assert!(f.distance(p) > 1.0, "the cart hits {f:?} at {p}");
                }
            }
            d += 0.5;
        }
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn crates_come_down_then_ride_off() {
        let mut lowered = [false; 2];
        let mut gone_again = false;
        let mut t = 0.0;
        while t < DOCKED_S + 40.0 {
            let w = works(t);
            for (k, c) in w.crates.iter().enumerate() {
                if matches!(c, Crate::OnCart) {
                    lowered[k] = true;
                }
            }
            if lowered == [true, true] && w.crates == [Crate::Gone; 2] {
                gone_again = true;
            }
            // Never two things on the hook at once.
            let hanging = w
                .crates
                .iter()
                .filter(|c| matches!(c, Crate::Hanging(_)))
                .count();
            assert!(hanging <= 1);
            t += 0.5;
        }
        assert!(lowered == [true, true] && gone_again);
        // Both cart runs end where they began or at the door, inside the moored spell.
        assert!(cue::CART_BACK.1 < DOCKED_S + hw_gen::urban::mast::CASTOFF_S);
        assert_eq!(works(0.0).gangway, 0.0);
        assert!(works(100.0).gangway > 0.99);
    }
}
