//! Flying taxis a hero can hail.
//!
//! A player picks one of the town's taxi stands (rooftop pads, landmarks,
//! the waterfront, the old villages the town swallowed) and a taxi drops
//! out of the sky to pick them up beside where they stand. Once they are in,
//! it climbs straight up to an express height above every roof on the way,
//! crosses the town, comes down at the stand and lets them out, then climbs
//! away. Hopping out on the way drops the hero wherever the taxi is (heroes
//! fly; normal people would not try this).
//!
//! The taxi is a replicated entity ([`EntityKind::Taxi`]) whose `owner` is
//! the passenger; a riding hero sits at the taxi's [`SEAT`], moved by the
//! taxi rather than by its own input.

use crate::collision::Obstacles;
use crate::{LastChange, Motion, Net, Orientation};
use bevy_ecs::prelude::*;
use glam::{DVec2, DVec3};
use hw_core::NetId;
use hw_gen::building::BuildingPlan;
use hw_gen::urban::area::Area;
use hw_gen::urban::growth::CentreKind;
use hw_gen::urban::landmarks::LandmarkKind;
use hw_gen::water::segment_distance;
use hw_net::PlayerId;
use std::sync::Arc;

/// Where a riding hero sits, relative to the taxi's underside.
pub const SEAT: DVec3 = DVec3::new(0.0, 0.55, 0.0);
/// Most stands a town offers.
pub const MAX_STANDS: usize = 9;
/// The hail asks for no stand: get out now (or send the taxi away).
pub const HOP_OUT: u8 = 255;
/// Speeds (m/s): climbing and sinking, and crossing town.
const CLIMB: f64 = 9.0;
const CRUISE: f64 = 48.0;
/// How long a taxi waits for its fare, in seconds.
const WAIT: f64 = 25.0;
/// Seconds the canopy takes to swing open or shut.
pub const CANOPY: f64 = 1.2;
/// A hero this close to a waiting taxi gets in.
const BOARD: f64 = 6.0;
/// Express height: this far above the tallest roof near the way (m).
const CLEAR: f64 = 25.0;

/// A place a taxi can take you.
#[derive(Clone, Debug, PartialEq)]
pub struct Stand {
    pub name: String,
    /// Where the passenger steps out (feet).
    pub pos: DVec3,
}

/// A hero in a taxi: moved by the taxi, not by its own input.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Riding;

/// A flying taxi (replicated as [`hw_net::EntityKind::Taxi`]).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaxiCab;

/// One straight, eased stretch of a taxi's flight.
#[derive(Clone, Copy, Debug)]
struct Leg {
    a: DVec3,
    b: DVec3,
    secs: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Stage {
    /// Coming down to pick the fare up.
    Coming,
    /// Hovering beside the fare, waiting since this tick.
    Waiting(u64),
    /// The fare is in.
    Riding,
    /// Down at the stand, the canopy opening to let the fare out.
    Alighting,
    /// Off again, empty.
    Leaving,
}

struct Ride {
    owner: PlayerId,
    taxi: Entity,
    dest: usize,
    stage: Stage,
    legs: Vec<Leg>,
    /// Tick the current legs began.
    since: u64,
    yaw: f64,
    /// The canopy is open until this many seconds into the current legs
    /// (replicated as the taxi's `grounded`: parked, canopy up).
    open: f64,
}

/// The town's taxi stands and the taxis flying fares.
#[derive(Default)]
pub struct Taxis {
    pub stands: Vec<Stand>,
    town: Option<Arc<Area>>,
    rides: Vec<Ride>,
}

impl Taxis {
    pub fn new(town: Arc<Area>) -> Self {
        Self {
            stands: stands(&town),
            town: Some(town),
            rides: Vec::new(),
        }
    }

    /// Any taxi out on a fare (the stands must not change under it)?
    pub fn busy(&self) -> bool {
        !self.rides.is_empty()
    }

    /// The stands of another square of the town.
    pub fn set_town(&mut self, town: Arc<Area>) {
        self.stands = stands(&town);
        self.town = Some(town);
    }

    /// Is `owner` riding, or waiting for, a taxi?
    pub fn has_ride(&self, owner: PlayerId) -> bool {
        self.rides
            .iter()
            .any(|r| r.owner == owner && r.stage != Stage::Leaving)
    }

    /// The stand `owner`'s taxi is going to.
    pub fn destination(&self, owner: PlayerId) -> Option<&Stand> {
        let r = self
            .rides
            .iter()
            .find(|r| r.owner == owner && r.stage != Stage::Leaving)?;
        self.stands.get(r.dest)
    }
}

/// A player hails a taxi to stand `dest`, or with [`HOP_OUT`] gets out (or
/// sends a coming taxi away). `hero` is their hero, `id` the new taxi's.
#[allow(clippy::too_many_arguments)]
pub fn hail(
    world: &mut World,
    taxis: &mut Taxis,
    id: NetId,
    solids: &Obstacles,
    owner: PlayerId,
    hero: Entity,
    dest: u8,
    tick: u64,
) {
    let Some(at) = world.get::<Motion>(hero).map(|m| m.0.pos) else {
        return;
    };
    if dest == HOP_OUT {
        let Some(r) = taxis
            .rides
            .iter_mut()
            .find(|r| r.owner == owner && r.stage != Stage::Leaving)
        else {
            return;
        };
        let pos = world.get::<Motion>(r.taxi).map_or(at, |m| m.0.pos);
        let vel = world.get::<Motion>(r.taxi).map_or(DVec3::ZERO, |m| m.0.vel);
        if matches!(r.stage, Stage::Riding | Stage::Alighting) {
            if let Ok(mut e) = world.get_entity_mut(hero) {
                e.remove::<Riding>();
                if let Some(mut m) = e.get_mut::<Motion>() {
                    // Out of the side door.
                    let side = DVec3::new(hw_core::det::cos(r.yaw), 0.0, -hw_core::det::sin(r.yaw));
                    m.0.pos = pos + side * 1.6;
                    m.0.vel = vel;
                    m.0.grounded = false;
                }
            }
        }
        r.stage = Stage::Leaving;
        r.legs = vec![leg(pos, pos + DVec3::new(0.0, 80.0, 0.0), CLIMB * 1.5)];
        r.since = tick;
        r.open = 0.0;
        return;
    }
    if taxis.has_ride(owner) || dest as usize >= taxis.stands.len() {
        return;
    }
    // Hover beside the hero where there is room, a little off the ground.
    let mut spot = at;
    for k in 0..8 {
        let a = std::f64::consts::TAU * k as f64 / 8.0;
        let p = at + DVec3::new(hw_core::det::cos(a), 0.0, hw_core::det::sin(a)) * 3.0;
        let clear = (0..4).all(|j| !solids.solid_at(p + DVec3::Y * (0.3 + j as f64 * 0.5)));
        if clear {
            spot = p;
            break;
        }
    }
    let floor = solids.floor_under(DVec2::new(spot.x, spot.z), at.y + 1.0);
    let hover = DVec3::new(spot.x, floor.max(at.y - 1.0) + 0.35, spot.z);
    let from = hover + DVec3::new(0.0, 75.0, 0.0);
    let yaw = hw_core::det::atan2(at.x - hover.x, at.z - hover.z) + std::f64::consts::FRAC_PI_2;
    let taxi = world
        .spawn((
            Net(id),
            Motion(crate::collision::Body {
                pos: from,
                vel: DVec3::ZERO,
                grounded: false,
            }),
            Orientation(yaw_quat(yaw)),
            LastChange(tick),
            TaxiCab,
            Fare(owner),
        ))
        .id();
    taxis.rides.push(Ride {
        owner,
        taxi,
        dest: dest as usize,
        stage: Stage::Coming,
        legs: vec![leg(from, hover, CLIMB * 1.4)],
        since: tick,
        yaw,
        open: 0.0,
    });
}

/// Who a taxi is for (replicated as its `owner`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fare(pub PlayerId);

/// One tick of every taxi: flying its legs, picking up, dropping off.
pub fn step(world: &mut World, taxis: &mut Taxis, solids: &Obstacles, tick: u64, dt: f64) {
    let heroes: Vec<(Entity, PlayerId, DVec3)> = world
        .query::<(Entity, &crate::Hero, &Motion)>()
        .iter(world)
        .map(|(e, h, m)| (e, h.owner, m.0.pos))
        .collect();
    let town = taxis.town.clone();
    let stands = taxis.stands.clone();
    let mut done = Vec::new();
    for (i, r) in taxis.rides.iter_mut().enumerate() {
        let hero = heroes.iter().find(|h| h.1 == r.owner).copied();
        // A fare who left the game sends the taxi away.
        if hero.is_none() && r.stage != Stage::Leaving {
            r.stage = Stage::Leaving;
            let p = world.get::<Motion>(r.taxi).map_or(DVec3::ZERO, |m| m.0.pos);
            r.legs = vec![
                hold(p, CANOPY),
                leg(p, p + DVec3::new(0.0, 80.0, 0.0), CLIMB * 1.5),
            ];
            r.since = tick;
            r.open = 0.0;
        }
        let t = (tick - r.since) as f64 * dt;
        let total: f64 = r.legs.iter().map(|l| l.secs).sum();
        let (pos, vel) = fly(&r.legs, t, dt);
        let flat = DVec2::new(vel.x, vel.z);
        if flat.length() > 1.0 {
            r.yaw = hw_core::det::atan2(flat.x, flat.y);
        }
        if let Ok(mut e) = world.get_entity_mut(r.taxi) {
            if let Some(mut m) = e.get_mut::<Motion>() {
                m.set_if_neq(Motion(crate::collision::Body {
                    pos,
                    vel,
                    grounded: t < r.open,
                }));
            }
            if let Some(mut o) = e.get_mut::<Orientation>() {
                o.set_if_neq(Orientation(yaw_quat(r.yaw)));
            }
        }
        let arrived = t >= total;
        match r.stage {
            Stage::Coming if arrived => {
                r.stage = Stage::Waiting(tick);
                // Parked: the canopy swings up for the fare.
                r.legs = vec![hold(pos, f64::MAX)];
                r.since = tick;
                r.open = f64::MAX;
            }
            Stage::Waiting(since) => {
                let Some((he, _, hp)) = hero else { continue };
                // In once the canopy is up.
                let opened = (tick - since) as f64 * dt >= CANOPY;
                if opened && hp.distance(pos) < BOARD {
                    // In you get: up, across above the roofs, and down.
                    let Some(stand) = stands.get(r.dest) else {
                        continue;
                    };
                    let drop = stand.pos + DVec3::new(0.0, 0.35, 0.0);
                    let high = express_height(town.as_deref(), pos, drop);
                    let up = DVec3::new(pos.x, high, pos.z);
                    let over = DVec3::new(drop.x, high, drop.z);
                    // The canopy shuts before it lifts off.
                    r.legs = vec![
                        hold(pos, CANOPY),
                        leg(pos, up, CLIMB),
                        leg(up, over, CRUISE),
                        leg(over, drop, CLIMB),
                    ];
                    r.since = tick;
                    r.open = 0.0;
                    r.stage = Stage::Riding;
                    world.entity_mut(he).insert(Riding);
                    seat(world, he, pos, DVec3::ZERO);
                } else if (tick - since) as f64 * dt > WAIT {
                    r.stage = Stage::Leaving;
                    r.legs = vec![
                        hold(pos, CANOPY),
                        leg(pos, pos + DVec3::new(0.0, 80.0, 0.0), CLIMB * 1.5),
                    ];
                    r.since = tick;
                    r.open = 0.0;
                }
            }
            Stage::Riding => {
                let Some((he, _, _)) = hero else { continue };
                if arrived {
                    // Down at the stand: the canopy opens first.
                    r.stage = Stage::Alighting;
                    r.legs = vec![hold(pos, CANOPY)];
                    r.since = tick;
                    r.open = f64::MAX;
                }
                seat(world, he, pos, vel);
            }
            Stage::Alighting => {
                let Some((he, _, _)) = hero else { continue };
                if arrived {
                    // Out at the stand, the taxi off again.
                    let side = DVec3::new(hw_core::det::cos(r.yaw), 0.0, -hw_core::det::sin(r.yaw));
                    let out = stands.get(r.dest).map_or(pos, |s| s.pos) + side * 1.4;
                    let floor = solids.floor_under(DVec2::new(out.x, out.z), out.y + 0.5);
                    if let Ok(mut e) = world.get_entity_mut(he) {
                        e.remove::<Riding>();
                        if let Some(mut m) = e.get_mut::<Motion>() {
                            m.0 = crate::collision::Body {
                                pos: DVec3::new(out.x, floor.max(out.y - 2.0), out.z),
                                vel: DVec3::ZERO,
                                grounded: true,
                            };
                        }
                    }
                    // Open while the fare steps out, then shut, and away.
                    r.stage = Stage::Leaving;
                    r.legs = vec![
                        hold(pos, 2.0 + CANOPY),
                        leg(pos, pos + DVec3::new(0.0, 80.0, 0.0), CLIMB * 1.5),
                    ];
                    r.since = tick;
                    r.open = 2.0;
                } else {
                    seat(world, he, pos, vel);
                }
            }
            Stage::Leaving if arrived => done.push(i),
            _ => {}
        }
    }
    for i in done.into_iter().rev() {
        let r = taxis.rides.remove(i);
        world.despawn(r.taxi);
    }
}

/// Every taxi in the air (to put back after a load, nothing; to clear).
pub fn clear(world: &mut World, taxis: &mut Taxis) {
    for r in taxis.rides.drain(..) {
        world.despawn(r.taxi);
    }
    let riding: Vec<Entity> = world
        .query_filtered::<Entity, With<Riding>>()
        .iter(world)
        .collect();
    for e in riding {
        world.entity_mut(e).remove::<Riding>();
    }
}

/// Is this hero sitting in one of the taxis among `all`?
pub fn riding(hero: &hw_net::EntityState, all: &[hw_net::EntityState]) -> bool {
    let p = DVec3::from_array(hero.pos);
    all.iter().any(|t| {
        t.kind == hw_net::EntityKind::Taxi
            && t.owner == hero.owner
            && (DVec3::from_array(t.pos) + SEAT).distance(p) < 0.6
    })
}

fn seat(world: &mut World, hero: Entity, taxi: DVec3, vel: DVec3) {
    if let Some(mut m) = world.get_mut::<Motion>(hero) {
        m.set_if_neq(Motion(crate::collision::Body {
            pos: taxi + SEAT,
            vel,
            grounded: false,
        }));
    }
}

/// Standing still at `p` for `secs` (the canopy opening or closing).
fn hold(p: DVec3, secs: f64) -> Leg {
    Leg { a: p, b: p, secs }
}

fn leg(a: DVec3, b: DVec3, speed: f64) -> Leg {
    Leg {
        a,
        b,
        secs: a.distance(b) / speed + 1.5,
    }
}

/// Where the legs have got to after `t` seconds, and how fast.
fn fly(legs: &[Leg], t: f64, dt: f64) -> (DVec3, DVec3) {
    let at = |t: f64| {
        let mut left = t.max(0.0);
        for l in legs {
            if left <= l.secs {
                let u = left / l.secs;
                let e = u * u * (3.0 - 2.0 * u);
                return l.a.lerp(l.b, e);
            }
            left -= l.secs;
        }
        legs.last().map_or(DVec3::ZERO, |l| l.b)
    };
    let p = at(t);
    (p, (at(t + dt) - p) / dt)
}

fn yaw_quat(yaw: f64) -> [f32; 4] {
    let (s, c) = hw_core::det::sin_cos(yaw * 0.5);
    [0.0, s as f32, 0.0, c as f32]
}

/// A height to cross from `a` to `b` above every roof and landmark near
/// the straight way, in round tens of metres.
pub(crate) fn express_height(town: Option<&Area>, a: DVec3, b: DVec3) -> f64 {
    let mut top = a.y.max(b.y) + 40.0;
    if let Some(t) = town {
        let (a2, b2) = (DVec2::new(a.x, a.z), DVec2::new(b.x, b.z));
        for bd in &t.buildings {
            let s = &bd.site;
            let c = s.origin + s.along * s.size.x * 0.5 + s.inward * s.size.y * 0.5;
            if segment_distance(c, a2, b2) < 60.0 + s.size.max_element() {
                top = top.max(bd.base + s.height() + 12.0 + CLEAR);
            }
        }
        for l in &t.landmarks {
            if segment_distance(l.pos, a2, b2) < 80.0 {
                top = top.max(l.base + l.height + CLEAR);
            }
        }
    }
    (top / 10.0).ceil() * 10.0
}

/// The town's taxi stands: its centre, landmarks, the waterfront, the old
/// villages, and rooftop pads spread over the town. Every machine works out
/// the same list (the client shows it as "Where to?").
pub fn stands(world: &Area) -> Vec<Stand> {
    let mut out: Vec<Stand> = Vec::new();
    let (core, _) = world.spawn_line();
    let street = |p: DVec2| DVec3::new(p.x, world.road_height(p), p.y);
    out.push(Stand {
        name: "Town centre".into(),
        pos: street(core),
    });
    for l in &world.landmarks {
        let name = match l.kind {
            LandmarkKind::AetherSpire => "Aether Spire plaza",
            LandmarkKind::Lighthouse => "The lighthouse",
            LandmarkKind::HeroHall => "Hero Hall",
            LandmarkKind::MooringMast => "The airfield",
            LandmarkKind::RocketGantry => "The rocket port",
            LandmarkKind::AtomicPlant => "Atomic plant gate",
            LandmarkKind::MoleDrill => "The mole drill",
            LandmarkKind::WeatherTower => "The weather tower",
            LandmarkKind::Hospital => "Aether Mercy Hospital",
            LandmarkKind::CometBuilding => hw_gen::urban::comet::name(l),
            // No taxi will take you there.
            LandmarkKind::TeslaTower | LandmarkKind::RobotWorks => continue,
        };
        // On the airfield's apron beside the mast, clear of the ship's
        // cargo square and the terminal.
        let p = if l.kind == LandmarkKind::MooringMast {
            l.to_plan(DVec2::new(l.plaza - 3.0, 0.0))
        } else {
            l.pos + l.dir * (l.plaza - 3.0).max(2.0)
        };
        if out.iter().all(|s| s.name != name) {
            out.push(Stand {
                name: name.into(),
                pos: DVec3::new(p.x, l.base, p.y),
            });
        }
    }
    // The steps down to the water nearest the centre.
    if let Some(st) = world
        .water_steps
        .iter()
        .min_by(|a, b| a.pos.distance(core).total_cmp(&b.pos.distance(core)))
    {
        let p = st.pos - st.out * 3.0;
        out.push(Stand {
            name: if world.coastal() {
                "Harbour steps".into()
            } else {
                "River steps".into()
            },
            pos: DVec3::new(p.x, st.top, p.y),
        });
    }
    // Old villages the town grew over (the nearest street to each).
    let mut villages: Vec<DVec2> = world
        .town
        .growth
        .centres
        .iter()
        .filter(|c| c.kind == CentreKind::Village)
        .map(|c| c.pos)
        .collect();
    villages.sort_by(|a, b| b.distance(core).total_cmp(&a.distance(core)));
    for v in villages.into_iter().take(2) {
        if let Some(p) = nearest_street(world, v) {
            out.push(Stand {
                name: format!("Old village, {}", compass(p - core)),
                pos: street(p),
            });
        }
    }
    // Rooftop pads, spread out: the farthest from what is chosen already.
    let pads: Vec<(DVec2, f64)> = world
        .buildings
        .iter()
        .filter(|b| b.site.look().taxi_pad && b.site.size.min_element() >= 15.0)
        .map(|b| {
            let s = &b.site;
            (
                s.origin + s.along * s.size.x * 0.5 + s.inward * s.size.y * 0.5,
                b.base,
            )
        })
        .collect();
    let mut chosen: Vec<DVec2> = out.iter().map(|s| DVec2::new(s.pos.x, s.pos.z)).collect();
    while out.len() < MAX_STANDS {
        let best = pads.iter().max_by(|a, b| {
            let d = |p: DVec2| {
                chosen
                    .iter()
                    .map(|c| c.distance(p))
                    .fold(f64::MAX, f64::min)
            };
            d(a.0).total_cmp(&d(b.0))
        });
        let Some(&(c, _)) = best else { break };
        if chosen.iter().any(|q| q.distance(c) < 150.0) {
            break;
        }
        let b = world
            .buildings
            .iter()
            .find(|b| {
                let s = &b.site;
                (s.origin + s.along * s.size.x * 0.5 + s.inward * s.size.y * 0.5).distance(c) < 1e-6
            })
            .expect("pad's building");
        let plan = BuildingPlan::generate(&b.site);
        // On the pad's deck, over the middle of the top floor.
        let top = plan
            .floors
            .last()
            .map_or(b.site.size * 0.5, |f| f.outline.center());
        let p = b.site.origin + b.site.along * top.x + b.site.inward * top.y;
        out.push(Stand {
            name: format!("Rooftop pad, {}", compass(p - core)),
            pos: DVec3::new(p.x, b.base + plan.height() + 1.6, p.y),
        });
        chosen.push(c);
    }
    out.truncate(MAX_STANDS);
    out
}

fn nearest_street(world: &Area, p: DVec2) -> Option<DVec2> {
    let mut best: Option<(f64, DVec2)> = None;
    for r in &world.roads {
        for w in r.pts.windows(2) {
            let m = (w[0] + w[1]) * 0.5;
            let d = m.distance(p);
            if world.ground(m) > hw_gen::urban::world::DECK && best.is_none_or(|b| d < b.0) {
                best = Some((d, m));
            }
        }
    }
    best.map(|b| b.1)
}

/// "north", "south-west", ... for a direction in the town's plan (north is
/// -y, east +x).
fn compass(d: DVec2) -> &'static str {
    if d.length() < 1.0 {
        return "centre";
    }
    let a = hw_core::det::atan2(d.x, -d.y)
        .to_degrees()
        .rem_euclid(360.0);
    const NAMES: [&str; 8] = [
        "north",
        "north-east",
        "east",
        "south-east",
        "south",
        "south-west",
        "west",
        "north-west",
    ];
    NAMES[(((a + 22.5) / 45.0) as usize) % 8]
}
