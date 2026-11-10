//! The wrist radio: incidents in town as a list (what, how far, which way
//! as a clock direction, how tough against you); the police visor
//! ([`crate::visor`]) marks where each one is. Rescues show on the
//! spot: a crash's two smashed cars smoking, a house's front in flames, and
//! the people still trapped (pinned by the legs in a wreck or rubble,
//! waving for help at a fire). The last one out is hurt: an
//! ambulance pulls up down the street, and the hero carries them there.

use crate::fightview::con_colour;
use engine::prelude::*;
use engine::ui::egui;
use glam::{DVec2, DVec3, Vec3};
use hw_gen::urban::props::CarModel;
use hw_net::ToClient;
use hw_sim::incidents::{victims, IncidentKind, MEDIC_REACH, RESCUE_REACH};
use std::collections::BTreeMap;
use std::sync::Arc;

/// How tall the light over an incident stands (m).
const BEAM: f64 = 140.0;

/// One incident the radio has heard of.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Call {
    pub kind: IncidentKind,
    pub at: Vec3,
    pub level: u8,
    /// Which way it faces.
    pub facing: DVec2,
    /// People still trapped (a rescue).
    pub left: u8,
    /// Seconds since it came in (new calls flash).
    pub age: f32,
    /// A rescue's hurt person, once out.
    pub casualty: Option<Casualty>,
    /// How far along holding the help key there is, as the server last
    /// said (sixteenths), and the bar as shown (0-1, eased between).
    pub held: u8,
    pub bar: f32,
    /// A fire put out by a freeze ray.
    pub doused: bool,
}

/// A rescue's hurt person and the ambulance waiting for them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Casualty {
    /// Where the ambulance stands, and its yaw (radians).
    pub medic: Vec3,
    pub yaw: f32,
    /// 0 lying at `at`, 1 carried by `carrier`, 2 with the medic.
    pub state: u8,
    pub carrier: u32,
    pub at: Vec3,
}

/// What the player can do about a rescue just now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Help {
    /// Next to someone trapped.
    FreeThem,
    /// Next to someone lying hurt.
    LiftThem,
    /// Carrying someone hurt: on to the ambulance.
    CarryThem,
    /// By frightened mole folk: talk them down.
    TalkDown,
}

/// Someone at a rescue on screen: an Actors townsperson (`real`) or, until
/// their looks are built (or with no packs), a plain stand-in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shown {
    pub e: EntityId,
    pub real: bool,
}

/// The looks rescues draw their people from: the townsfolk's (by look)
/// and the ambulance medic's, as the people worker finishes them.
#[derive(Default, Clone)]
pub struct Cast {
    looks: BTreeMap<u32, Arc<AnimatedModel>>,
    medic: Option<Arc<AnimatedModel>>,
}

impl Cast {
    /// A finished character: kept if rescues use it.
    pub fn add(&mut self, who: &crate::people::Who, model: &Arc<AnimatedModel>) {
        match who {
            crate::people::Who::Citizen(l) if *l < hw_sim::citizens::LOOKS => {
                self.looks.insert(*l, model.clone());
            }
            crate::people::Who::Medic => self.medic = Some(model.clone()),
            _ => {}
        }
    }

    fn model(&self, look: Option<u32>) -> Option<&Arc<AnimatedModel>> {
        match look {
            Some(l) => self.looks.get(&l),
            None => self.medic.as_ref(),
        }
    }

    /// Whether `look` (`None`: the medic) can be drawn as itself yet.
    pub fn ready(&self, look: Option<u32>) -> bool {
        self.model(look).is_some()
    }

    /// Puts someone at `place` playing `clip` ("wave", "trapped", "beckon", "lie",
    /// "carried", "idle", "walk"): look `look` of the townsfolk, or the
    /// medic for `None`.
    pub fn spawn(
        &self,
        world: &mut World,
        look: Option<u32>,
        clip: &str,
        place: Place,
    ) -> Option<Shown> {
        if let Some(m) = self.model(look) {
            let e = world.spawn_animated_shared(m.clone(), place).ok()?;
            let _ = world.set_animation_blend(e, 0.25);
            let _ = world.play_animation(e, clip);
            return Some(Shown { e, real: true });
        }
        let mesh = match (look, clip) {
            (None, _) => person(MEDIC_LOOK),
            (Some(l), "lie" | "carried") => lying(l),
            (Some(l), _) => person(l),
        };
        let e = world.place(mesh, place).ok()?;
        Some(Shown { e, real: false })
    }
}

/// What the still trapped at a `kind` of rescue play: "trapped" (sat
/// pinned by the legs, tugging to get free) at a crash or in rubble,
/// "wave" (calling for help) at a fire.
fn trapped_clip(kind: IncidentKind) -> &'static str {
    match kind {
        IncidentKind::CarCrash | IncidentKind::Collapse => "trapped",
        _ => "wave",
    }
}

/// The townsfolk look of the `k`th person trapped at incident `id` (the
/// first, `k` 0, is the last out: the hurt one).
pub fn rescued_look(id: u32, k: usize) -> u32 {
    (hw_core::Seed(id as u64).child("rescued").child(k as u64).0 % hw_sim::citizens::LOOKS as u64)
        as u32
}

/// The ambulance at a rescue: where it stands (or will), its yaw
/// (radians), the way it came (while it is still pulling up) and the way
/// it will leave, both along the street lane.
struct Van {
    lod: LodId,
    at: Vec3,
    yaw: f32,
    route: Option<Route>,
    out: Vec<Vec3>,
}

/// A drive along a line of points: how far along it is and how fast.
struct Route {
    pts: Vec<Vec3>,
    along: Vec<f32>,
    s: f32,
    v: f32,
}

impl Route {
    fn new(pts: Vec<Vec3>) -> Self {
        let mut along = vec![0.0];
        for w in pts.windows(2) {
            along.push(along.last().copied().unwrap_or(0.0) + w[0].distance(w[1]));
        }
        Self {
            pts,
            along,
            s: 0.0,
            v: 0.0,
        }
    }

    fn len(&self) -> f32 {
        self.along.last().copied().unwrap_or(0.0)
    }

    /// The point `s` along, and the way it heads there (looking a few
    /// metres ahead, so turns are smooth).
    fn at(&self, s: f32) -> (Vec3, Vec3) {
        let point = |s: f32| {
            let s = s.clamp(0.0, self.len());
            let i = self
                .along
                .partition_point(|&a| a <= s)
                .saturating_sub(1)
                .min(self.pts.len().saturating_sub(2));
            let (a, b) = (self.pts[i], self.pts[(i + 1).min(self.pts.len() - 1)]);
            let l = (self.along[(i + 1).min(self.along.len() - 1)] - self.along[i]).max(1e-4);
            a.lerp(b, ((s - self.along[i]) / l).clamp(0.0, 1.0))
        };
        let p = point(s);
        let d = point(s + 2.5) - point(s - 2.5);
        (p, Vec3::new(d.x, 0.0, d.z).normalize_or(Vec3::Z))
    }
}

/// An ambulance with its hurt on board, pulling out and driving off along
/// the street; it goes once out of sight. The medic walks round to the
/// cab and the hurt one is lifted in first.
struct Leaving {
    lod: LodId,
    route: Route,
    /// Seconds since the hand-over: it pulls out at [`LOAD_SECONDS`].
    t: f32,
    load: f32,
    medic: Option<(Shown, Vec3, Vec3)>,
    hurt: Option<(Shown, Vec3, Vec3)>,
    /// Seconds out of sight.
    unseen: f32,
}

/// Someone got out walking off from the scene, gone once out of sight.
struct Walker {
    who: Shown,
    at: Vec3,
    dir: Vec3,
    age: f32,
    unseen: f32,
    /// Seconds still to stand thanking the hero at `hero` first.
    thank: f32,
    hero: Option<Vec3>,
}

/// How long someone got out stands thanking their hero (s).
const THANK_SECONDS: f32 = 2.8;

/// What the people got out say to their hero.
const THANKS: [&str; 8] = [
    "Thank you, hero! I thought I was done for!",
    "Gee, thanks! You're the real deal!",
    "My hero! I won't ever forget this!",
    "Swell work, pal. Thank you, thank you!",
    "Bless you! I owe you my life!",
    "Holy smokes, you did it! Thanks a million!",
    "Wait till I tell the folks at home about you!",
    "You came! Oh, thank you!",
];
/// The hurt one, lifted into the ambulance.
const HURT_THANKS: [&str; 3] = [
    "Thank you... you saved my life.",
    "I'll be all right now... thanks to you, hero.",
    "Don't know who you are... but thank you.",
];
/// The medic, taking them over.
const MEDIC_WORDS: [&str; 3] = [
    "Nice work, hero! We'll take it from here.",
    "Good job. They'll pull through, thanks to you.",
    "Easy does it... we've got them. Well done!",
];

/// Who someone at a rescue is, in the chat log.
fn rescued_name(kind: IncidentKind) -> &'static str {
    match kind {
        IncidentKind::CarCrash => "A driver",
        IncidentKind::Fire => "A resident",
        IncidentKind::Collapse => "A survivor",
        _ => "A townsperson",
    }
}

/// How long the hurt one takes to be lifted in and the medic to get round
/// to the cab before the ambulance pulls out (s).
const LOAD_SECONDS: f32 = 2.6;
/// An ambulance's speed on the street (m/s) and how hard it speeds up
/// and brakes (m/s²).
const VAN_SPEED: f32 = 12.0;
const VAN_ACCEL: f32 = 2.6;
/// How far back along the street an ambulance comes from (m).
const VAN_FROM: f64 = 70.0;
/// How far a freed person walks off before standing (s).
const WALK_SECONDS: f32 = 7.0;

/// Whether `p` is in the camera's view (roughly: within its cone and not
/// far off): things leave the scene only out of sight.
fn in_sight(world: &World, p: Vec3) -> bool {
    let cam = world.camera();
    let eye = cam.eye();
    let d = p + Vec3::Y * 1.0 - eye;
    let far = d.length();
    if far > 260.0 {
        return false;
    }
    if far < 4.0 {
        return true;
    }
    let look = (cam.target() - eye).normalize_or(Vec3::Z);
    // The widest a view gets (a wide screen's corners) and a margin.
    look.dot(d / far) > 0.3
}

/// What stands at a rescue: wrecked cars, flames and smoke, the people.
#[derive(Default)]
struct Scene {
    cars: Vec<LodId>,
    fire: Vec<EmitterId>,
    /// The people still trapped, in the order of [`victims`], and where
    /// each stands.
    people: Vec<(Shown, Place)>,
    /// The ambulance and its medic.
    van: Option<Van>,
    medic: Option<Shown>,
    /// The hurt person, lying or in someone's arms.
    hurt: Option<Shown>,
    /// The hurt one floated off in a rescue bubble (it lands later).
    bubbled: bool,
    /// The hurt one was handed over before the ambulance got here: it
    /// leaves as soon as it is.
    leave: bool,
    /// An ambulance to bring: where it parks, its yaw (radians), and
    /// whether it drives up (news as it happens) or is simply there.
    van_due: Option<(Vec3, f32, bool)>,
    /// A pirate airship over the raid: where it hangs, its yaw (degrees),
    /// and the rope ladders down from it.
    ship: Option<(LodId, Vec3, f32)>,
    ladders: Vec<EntityId>,
}

/// An airship sailing off once its raid is over: where it was, its yaw
/// (degrees), seconds since it set off.
struct Departing {
    lod: LodId,
    from: Vec3,
    yaw: f32,
    /// How high it rises before it sails off, clear of the roofs ahead.
    clear: f64,
    age: f32,
}

/// How long a departing airship stays in sight (s).

#[derive(Default)]
pub struct Radio {
    /// The town's solids: where an airship can hang clear of the roofs.
    pub solids: Option<std::sync::Arc<hw_sim::collision::Obstacles>>,
    /// The town plan: what an airship has to clear.
    pub town: Option<std::sync::Arc<hw_gen::urban::world::TownWorld>>,
    departing: Vec<Departing>,
    /// Seconds the radio has run (an airship's gentle sway).
    clock: f32,
    pub calls: BTreeMap<u32, Call>,
    scenes: BTreeMap<u32, Scene>,
    /// The last word on the air: (text, seconds left).
    pub flash: Option<(String, f32)>,
    /// Our hero's id (to know what we carry).
    pub me: Option<u32>,
    /// The Doctor's story, shown on the radio and the compass (see
    /// [`crate::storyview`]).
    pub story: Option<StoryMark>,
    /// Who rescues draw their people from.
    pub cast: Cast,
    leaving: Vec<Leaving>,
    walkers: Vec<Walker>,
    /// Where our hero is (people thank them), set each frame.
    pub hero_at: Option<Vec3>,
    /// What people at rescues said this frame (thanks): shown over their
    /// heads and logged in the chat.
    pub said: Vec<crate::chat::Said>,
    /// Our hero is holding the help key at a rescue just now.
    holding: bool,
}

/// The story's line on the radio: its heading, the goal, and where to go.
#[derive(Clone, Debug, PartialEq)]
pub struct StoryMark {
    pub heading: String,
    pub goal: String,
    pub at: Option<Vec3>,
}

/// The story's colour on the radio, the compass and the map.
pub const STORY_GOLD: egui::Color32 = egui::Color32::from_rgb(255, 200, 60);

/// A call's colour by kind, sRGB (its marks on the police visor).
pub fn call_colour(kind: IncidentKind) -> (u8, u8, u8) {
    match kind {
        IncidentKind::Mugging => (255, 220, 90),
        IncidentKind::Robbery => (255, 160, 60),
        IncidentKind::Rampage => (255, 80, 70),
        IncidentKind::Breakout => (150, 255, 110),
        IncidentKind::CarCrash => (120, 200, 255),
        IncidentKind::Fire => (255, 130, 40),
        IncidentKind::Colossus => (255, 40, 60),
        IncidentKind::Collapse => (230, 210, 170),
        IncidentKind::Surfacing => (200, 150, 255),
        IncidentKind::PirateRaid => (90, 230, 210),
        IncidentKind::IronLanding => (255, 60, 30),
    }
}

/// A column of light rising from a spot, faint at the foot (so it never
/// stands in the way up close), full a few storeys up and fading out high
/// over the rooftops, with a ring of the same light on the ground round the
/// spot. Glowing and see-through, in `colour`; origin on the ground. Only
/// the story's objective (gold) has one: calls show on the police visor
/// ([`crate::visor`]) instead.
pub fn beacon_lit(colour: Color) -> Mesh {
    const SIDES: usize = 32;
    let mut m = Mesh::new();
    let point = |m: &mut Mesh, p: Vec3, n: Vec3, alpha: f32| {
        let id = m.add_point(p).expect("finite point");
        let c = Color {
            a: alpha.clamp(0.0, 1.0),
            ..colour
        };
        m.set_point_color(id, c).expect("point exists");
        m.set_point_normal(id, n).expect("point exists");
        id
    };
    // The column: a slow taper, its light by height.
    let rows: [(f32, f32, f32); 7] = [
        (0.0, 1.5, 0.22),
        (2.5, 1.35, 0.3),
        (8.0, 1.15, 0.6),
        (20.0, 1.0, 0.72),
        ((BEAM * 0.6) as f32, 0.8, 0.6),
        ((BEAM * 0.85) as f32, 0.65, 0.3),
        (BEAM as f32, 0.5, 0.0),
    ];
    let rings: Vec<Vec<PointId>> = rows
        .iter()
        .map(|&(y, r, alpha)| {
            (0..SIDES)
                .map(|i| {
                    let a = std::f32::consts::TAU * i as f32 / SIDES as f32;
                    let out = Vec3::new(a.cos(), 0.0, a.sin());
                    point(&mut m, out * r + Vec3::Y * y, out, alpha)
                })
                .collect()
        })
        .collect();
    for w in rings.windows(2) {
        for i in 0..SIDES {
            let j = (i + 1) % SIDES;
            // Counter-clockwise seen from outside.
            m.add_triangle(w[0][i], w[1][j], w[0][j]).expect("tri");
            m.add_triangle(w[0][i], w[1][i], w[1][j]).expect("tri");
        }
    }
    // The ring on the ground: bright at its middle, soft at both edges.
    let lift = 0.12;
    let band: [(f32, f32); 3] = [(2.6, 0.0), (3.0, 0.85), (3.4, 0.0)];
    let circles: Vec<Vec<PointId>> = band
        .iter()
        .map(|&(r, alpha)| {
            (0..SIDES * 2)
                .map(|i| {
                    let a = std::f32::consts::TAU * i as f32 / (SIDES * 2) as f32;
                    let p = Vec3::new(a.cos() * r, lift, a.sin() * r);
                    point(&mut m, p, Vec3::Y, alpha)
                })
                .collect()
        })
        .collect();
    let n = SIDES * 2;
    for w in circles.windows(2) {
        for i in 0..n {
            let j = (i + 1) % n;
            // Facing up.
            m.add_triangle(w[0][i], w[0][j], w[1][j]).expect("tri");
            m.add_triangle(w[0][i], w[1][j], w[1][i]).expect("tri");
        }
    }
    m.set_surface_material(SurfaceMaterial {
        roughness: 0.4,
        emission: 2.0,
        ..SurfaceMaterial::DEFAULT
    });
    m
}

impl Radio {
    /// Where rescues' hurt people lie in the open (cars stop for them).
    pub fn lying(&self) -> impl Iterator<Item = Vec3> + '_ {
        self.calls
            .values()
            .filter_map(|c| c.casualty)
            .filter(|c| c.state == 0)
            .map(|c| c.at)
    }

    /// An incident message; returns false for other messages.
    pub fn hear(&mut self, world: &mut World, m: &ToClient) -> bool {
        if let ToClient::Casualty {
            id,
            medic,
            yaw,
            state,
            carrier,
            at,
        } = m
        {
            let c = Casualty {
                medic: Vec3::from_array(*medic),
                yaw: hw_sim::props::byte_to_yaw(*yaw) as f32,
                state: *state,
                carrier: *carrier,
                at: Vec3::from_array(*at),
            };
            self.casualty(world, *id, c);
            return true;
        }
        let ToClient::Incident {
            id,
            kind,
            at,
            level,
            state,
            yaw,
            left,
            held,
        } = m
        else {
            return false;
        };
        let Some(kind) = IncidentKind::from_code(*kind) else {
            return true;
        };
        let at = Vec3::from_array(*at);
        let a = hw_sim::props::byte_to_yaw(*yaw);
        let facing = DVec2::new(a.sin(), a.cos());
        let doused = *state == hw_sim::incidents::DOUSED;
        if *state == 0 || doused {
            if let Some(c) = self.calls.get_mut(id) {
                c.held = *held;
                if doused && !c.doused {
                    c.doused = true;
                    self.put_out(world, *id);
                    return true;
                }
                let c = self.calls.get_mut(id).expect("just found");
                // News of a rescue: one more got out.
                if *left < c.left && kind == IncidentKind::Surfacing {
                    c.left = *left;
                    self.flash = Some((
                        "Talked them down. Molehaven will remember this.".into(),
                        5.0,
                    ));
                } else if *left < c.left {
                    c.left = *left;
                    c.bar = 0.0;
                    self.flash = Some(("Got one out! Well done.".into(), 3.0));
                    self.got_out(world, *id);
                }
                return true;
            }
            self.calls.insert(
                *id,
                Call {
                    kind,
                    at,
                    level: *level,
                    facing,
                    left: *left,
                    age: 0.0,
                    casualty: None,
                    held: *held,
                    bar: 0.0,
                    doused,
                },
            );
            if kind.trapped() > 0 {
                self.stage(world, *id);
                if doused {
                    self.put_out(world, *id);
                }
            } else {
                self.set_piece(world, *id);
            }
            self.flash = Some((format!("Calling all heroes: {}!", kind.name()), 5.0));
        } else {
            self.calls.remove(id);
            self.unstage(world, *id);
            let word = match (*state, kind.trapped() > 0) {
                (1, true) => "Everyone is out. You saved them!".to_string(),
                (1, false) => format!("{} handled. Nice work!", kind.name()),
                (_, true) if kind == IncidentKind::Collapse => {
                    "The rescue crews dug them out.".to_string()
                }
                (_, true) => "The fire brigade got there first.".to_string(),
                _ => format!("{}: they got away.", kind.name()),
            };
            self.flash = Some((word, 4.0));
        }
        true
    }

    /// The new factions' trouble on the spot: the hole mole folk came up
    /// by, the pirates' airship over the shop with ladders down, the iron
    /// capsule in its crater.
    fn set_piece(&mut self, world: &mut World, id: u32) {
        use crate::factionview as fv;
        let Some(c) = self.calls.get(&id).copied() else {
            return;
        };
        let mut scene = Scene::default();
        let f = Vec3::new(c.facing.x as f32, 0.0, c.facing.y as f32);
        let yaw = c.facing.x.atan2(c.facing.y).to_degrees() as f32;
        match c.kind {
            IncidentKind::Surfacing => {
                let p = c.at;
                let place = Place::new(p.x, p.y, p.z).with_yaw_deg(yaw);
                if let Ok(l) =
                    world.spawn_lod_at(fv::broken_ground_levels(1.6, false, true), &[place])
                {
                    scene.cars.push(l);
                }
                let _ = world.particles_mut().burst(ParticleEmitter::dust(
                    p + Vec3::Y * 0.6,
                    3.0,
                    Color::rgb(120, 96, 70),
                ));
            }
            IncidentKind::IronLanding => {
                // It struck just behind where its legionaries stand.
                let p = c.at - f * 5.5;
                let place = Place::new(p.x, p.y, p.z).with_yaw_deg(yaw);
                for levels in [
                    fv::broken_ground_levels(2.2, true, false),
                    fv::iron_capsule_levels(),
                ] {
                    if let Ok(l) = world.spawn_lod_at(levels, &[place]) {
                        scene.cars.push(l);
                    }
                }
                let d = Decal::new(DecalKind::Scorch, p + Vec3::Y * 0.3, Vec3::Y, 7.0)
                    .depth(1.5)
                    .opacity(0.9)
                    .lifetime(600.0)
                    .fade(0.0, 6.0);
                let _ = world.spawn_decal(d);
                let ps = world.particles_mut();
                if let Ok(e) = ps.start(
                    ParticleEmitter::smoke(p + Vec3::Y * 2.5, 1.4)
                        .with_color(Color::rgba(60, 58, 56, 140)),
                ) {
                    scene.fire.push(e);
                }
                let _ = ps.burst(ParticleEmitter::dust(
                    p + Vec3::Y * 0.5,
                    6.0,
                    Color::rgb(110, 104, 96),
                ));
                let _ = ps.burst(ParticleEmitter::sparks(
                    p + Vec3::Y * 1.5,
                    Vec3::Y * 6.0,
                    80,
                ));
            }
            IncidentKind::PirateRaid => {
                let (at, ship_yaw) = self.mooring(c.at);
                let place = Place::new(at.x, at.y, at.z).with_yaw_deg(ship_yaw);
                if let Ok(l) = world.spawn_lod_at(fv::airship_levels(), &[place]) {
                    scene.ship = Some((l, at, ship_yaw));
                }
                // Two ladders from the gondola's rail down to the street.
                let a = ship_yaw.to_radians();
                let along = Vec3::new(a.sin(), 0.0, a.cos());
                let side = Vec3::new(along.z, 0.0, -along.x);
                for (k, z) in [(-1.0f32, -2.0f32), (1.0, 3.0)] {
                    let top = at + along * z + side * (1.9 * k) - Vec3::Y * fv::GONDOLA_DECK as f32;
                    let foot = c.at + along * (z * 0.5) + side * (2.4 * k) + Vec3::Y * 0.05;
                    let mesh = fv::rope_ladder(DVec3::ZERO, (foot - top).as_dvec3());
                    if let Ok(e) = world.place(mesh, Place::new(top.x, top.y, top.z)) {
                        scene.ladders.push(e);
                    }
                }
            }
            _ => {}
        }
        self.scenes.insert(id, scene);
    }

    /// Where a raiding airship hangs over `street`, and its yaw (degrees):
    /// see [`hw_sim::roosts::raid_mooring`] (the server's raider is solid
    /// there too).
    fn mooring(&self, street: Vec3) -> (Vec3, f32) {
        let Some(w) = self.town.as_ref() else {
            let keel = (crate::factionview::GONDOLA_DECK + 2.6) as f32;
            return (street + Vec3::Y * (keel + 14.0), 0.0);
        };
        let (at, yaw) = hw_sim::roosts::raid_mooring(w, street.as_dvec3());
        (at.as_vec3(), yaw.to_degrees() as f32)
    }

    /// The first raiding airship there is: where it hangs and its yaw
    /// (degrees). For screenshots.
    pub fn ship_at(&self) -> Option<(Vec3, f32)> {
        self.scenes
            .values()
            .find_map(|s| s.ship.map(|(_, at, yaw)| (at, yaw)))
    }

    /// Departing airships sail off and rise; a moored one sways a little.
    pub fn animate(&mut self, world: &mut World, dt: f32) {
        self.clock += dt;
        let t = self.clock;
        for s in self.scenes.values() {
            if let Some((l, at, yaw)) = s.ship {
                let bob = Vec3::Y * (t * 0.5).sin() * 0.35;
                let place = Place::new(at.x, at.y + bob.y, at.z)
                    .with_yaw_deg(yaw + (t * 0.23).sin() * 1.5)
                    .with_roll_deg((t * 0.41).sin() * 1.2);
                let _ = world.set_lod_places(l, &[place]);
            }
        }
        for d in self.departing.iter_mut() {
            d.age += dt;
            // Up clear of the roofs ahead, then off.
            let Some(p) = hw_sim::roosts::raid_departure(
                d.from.as_dvec3(),
                (d.yaw as f64).to_radians(),
                d.clear,
                d.age as f64,
            ) else {
                continue;
            };
            let p = p.as_vec3();
            let place = Place::new(p.x, p.y, p.z)
                .with_yaw_deg(d.yaw)
                .with_pitch_deg(-3.0);
            let _ = world.set_lod_places(d.lod, &[place]);
        }
        let mut gone = Vec::new();
        self.departing.retain(|d| {
            let away = hw_sim::roosts::raid_departure(
                d.from.as_dvec3(),
                (d.yaw as f64).to_radians(),
                d.clear,
                d.age as f64,
            );
            if away.is_none() {
                gone.push(d.lod);
                false
            } else {
                true
            }
        });
        for l in gone {
            world.despawn_lod(l);
        }
    }

    /// Ambulances pulling up and driving off, people walking away from a
    /// rescue, and stand-ins swapped for the townsfolk as their looks come
    /// in. `lanes` gives a stretch of street lane (see
    /// [`hw_sim::traffic::Traffic::street_stretch`]) for an ambulance to
    /// drive.
    pub fn rescues(
        &mut self,
        world: &mut World,
        dt: f32,
        lanes: &dyn Fn(DVec2, DVec2) -> Option<Vec<DVec3>>,
    ) {
        self.bring_vans(world, lanes);
        self.pull_up(world, dt);
        self.drive_off(world, dt);
        self.walk(world, dt);
        self.recast(world);
    }

    /// Ambulances asked for: placed at the kerb, or coming along the street.
    fn bring_vans(
        &mut self,
        world: &mut World,
        lanes: &dyn Fn(DVec2, DVec2) -> Option<Vec<DVec3>>,
    ) {
        let ids: Vec<u32> = (self.scenes.iter())
            .filter(|(_, s)| s.van_due.is_some())
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            let call_at = self.calls.get(&id).map(|c| c.at);
            let Some(scene) = self.scenes.get_mut(&id) else {
                continue;
            };
            let Some((at, yaw, arrive)) = scene.van_due.take() else {
                continue;
            };
            let f = Vec3::new(yaw.sin(), 0.0, yaw.cos());
            let stretch = lanes(
                DVec2::new(at.x as f64, at.z as f64),
                DVec2::new(f.x as f64, f.z as f64),
            );
            let (inn, out): (Vec<Vec3>, Vec<Vec3>) = match stretch {
                Some(pts) if pts.len() > 50 => {
                    // The nearest point is VAN_FROM back (2 m apart); the
                    // van leaves the lane and joins it 12 m either side.
                    let mid = ((VAN_FROM / 2.0) as usize).min(pts.len());
                    let pts: Vec<Vec3> = pts.iter().map(|p| p.as_vec3()).collect();
                    let inn = pts[..mid.saturating_sub(6)]
                        .iter()
                        .copied()
                        .chain([at])
                        .collect();
                    let out = std::iter::once(at)
                        .chain(pts[(mid + 6).min(pts.len())..].iter().copied())
                        .collect();
                    (inn, out)
                }
                // No lane here: straight along the kerb.
                _ => (
                    (0..=10)
                        .map(|k| at - f * (VAN_FROM as f32 * (1.0 - k as f32 / 10.0)))
                        .collect(),
                    (0..=40).map(|k| at + f * (k as f32 * 10.0)).collect(),
                ),
            };
            let route = arrive.then(|| Route::new(inn));
            let place = match &route {
                Some(r) => {
                    let (p, d) = r.at(0.0);
                    Place::new(p.x, p.y, p.z).with_yaw_deg(d.x.atan2(d.z).to_degrees())
                }
                None => Place::new(at.x, at.y, at.z).with_yaw_deg(yaw.to_degrees()),
            };
            let Ok(lod) = world.spawn_lod_at(crate::propview::ambulance_levels(), &[place]) else {
                continue;
            };
            let parked = route.is_none();
            scene.van = Some(Van {
                lod,
                at,
                yaw,
                route,
                out,
            });
            if parked && scene.medic.is_none() {
                scene.medic = self
                    .cast
                    .spawn(world, None, "beckon", medic_place(at, yaw, call_at));
            }
            if scene.leave {
                scene.leave = false;
                if let Some(l) = depart(world, scene) {
                    self.leaving.push(l);
                }
            }
        }
    }

    /// Ambulances coming up the street brake to a stop at the kerb; the
    /// medic gets out at the back.
    fn pull_up(&mut self, world: &mut World, dt: f32) {
        for (id, scene) in self.scenes.iter_mut() {
            let call_at = self.calls.get(id).map(|c| c.at);
            let Some(van) = scene.van.as_mut() else {
                continue;
            };
            let Some(r) = van.route.as_mut() else {
                continue;
            };
            let rest = r.len() - r.s;
            r.v = VAN_SPEED.min((2.0 * VAN_ACCEL * rest).sqrt()).max(0.8);
            r.s = (r.s + r.v * dt).min(r.len());
            let (p, d) = r.at(r.s);
            let parked = r.s >= r.len() - 0.01;
            let place = if parked {
                Place::new(van.at.x, van.at.y, van.at.z).with_yaw_deg(van.yaw.to_degrees())
            } else {
                Place::new(p.x, p.y, p.z).with_yaw_deg(d.x.atan2(d.z).to_degrees())
            };
            let _ = world.set_lod_places(van.lod, &[place]);
            if parked {
                van.route = None;
                if scene.medic.is_none() {
                    scene.medic = self.cast.spawn(
                        world,
                        None,
                        "beckon",
                        medic_place(van.at, van.yaw, call_at),
                    );
                }
            }
        }
    }

    /// Ambulances loading up and driving off; gone once out of sight.
    fn drive_off(&mut self, world: &mut World, dt: f32) {
        let mut keep = Vec::with_capacity(self.leaving.len());
        for mut l in std::mem::take(&mut self.leaving) {
            l.t += dt;
            let (_, ahead) = l.route.at(0.0);
            if let Some((m, from, to)) = l.medic {
                // Round to the cab, and in.
                let k = l.t / (l.load * 0.85);
                if k >= 1.0 {
                    world.despawn(m.e);
                    l.medic = None;
                } else {
                    let p = from.lerp(to, k);
                    let d = to - from;
                    let place = Place::new(p.x, p.y, p.z).with_yaw_deg(d.x.atan2(d.z).to_degrees());
                    let _ = world.set_place(m.e, place);
                }
            }
            if let Some((h, from, to)) = l.hurt {
                // Lifted in at the back doors.
                let k = l.t / 0.9;
                if k >= 1.0 {
                    world.despawn(h.e);
                    l.hurt = None;
                } else {
                    let e = k * k * (3.0 - 2.0 * k);
                    let p = from.lerp(to, e);
                    let yaw = ahead.x.atan2(ahead.z).to_degrees() + 90.0;
                    let _ = world.set_place(h.e, Place::new(p.x, p.y, p.z).with_yaw_deg(yaw));
                }
            }
            let r = &mut l.route;
            if l.t >= l.load {
                // Away under the lights, braking only at the end of the
                // stretch it knows.
                r.v = (r.v + VAN_ACCEL * dt).min(VAN_SPEED);
                let rest = r.len() - r.s;
                r.v = r.v.min((2.0 * VAN_ACCEL * rest).sqrt());
                r.s = (r.s + r.v * dt).min(r.len());
            }
            let (p, d) = r.at(r.s);
            let place = Place::new(p.x, p.y, p.z).with_yaw_deg(d.x.atan2(d.z).to_degrees());
            let _ = world.set_lod_places(l.lod, &[place]);
            if in_sight(world, p) {
                l.unseen = 0.0;
            } else {
                l.unseen += dt;
            }
            if l.t > l.load && l.unseen > 0.6 {
                world.despawn_lod(l.lod);
                for (s, _, _) in l.medic.into_iter().chain(l.hurt) {
                    world.despawn(s.e);
                }
                continue;
            }
            keep.push(l);
        }
        self.leaving = keep;
    }

    /// People out of a rescue walk away, then stand; gone once out of
    /// sight.
    fn walk(&mut self, world: &mut World, dt: f32) {
        let mut keep = Vec::with_capacity(self.walkers.len());
        for mut w in std::mem::take(&mut self.walkers) {
            if w.thank > 0.0 {
                // Facing their hero, thanking them; then off they go.
                w.thank -= dt;
                let to = w.hero.map_or(w.dir, |h| h - w.at);
                let yaw = to.x.atan2(to.z).to_degrees();
                let _ = world.set_place(
                    w.who.e,
                    Place::new(w.at.x, w.at.y, w.at.z).with_yaw_deg(yaw),
                );
                if w.thank <= 0.0 && w.who.real {
                    let _ = world.play_animation(w.who.e, "walk");
                }
                w.unseen = 0.0;
                keep.push(w);
                continue;
            }
            let was = w.age;
            w.age += dt;
            if was < WALK_SECONDS {
                let next = w.at + w.dir * crate::people::WALK_MPS * dt;
                let ahead = next + w.dir * 0.4 + Vec3::Y;
                let blocked = (self.solids.as_ref()).is_some_and(|s| s.solid_at(ahead.as_dvec3()));
                if blocked {
                    w.age = WALK_SECONDS;
                } else {
                    w.at = next;
                }
                if w.age >= WALK_SECONDS && w.who.real {
                    let _ = world.play_animation(w.who.e, "idle");
                }
            }
            let yaw = w.dir.x.atan2(w.dir.z).to_degrees();
            let _ = world.set_place(
                w.who.e,
                Place::new(w.at.x, w.at.y, w.at.z).with_yaw_deg(yaw),
            );
            if in_sight(world, w.at) {
                w.unseen = 0.0;
            } else {
                w.unseen += dt;
            }
            if w.unseen > 0.6 && w.age > 1.0 {
                world.despawn(w.who.e);
                continue;
            }
            keep.push(w);
        }
        self.walkers = keep;
    }

    /// Stand-ins become the townsfolk once their looks are built.
    fn recast(&mut self, world: &mut World) {
        for (id, scene) in self.scenes.iter_mut() {
            let clip = self.calls.get(id).map_or("wave", |c| trapped_clip(c.kind));
            for (k, (who, place)) in scene.people.iter_mut().enumerate() {
                let look = Some(rescued_look(*id, k));
                if who.real || !self.cast.ready(look) {
                    continue;
                }
                if let Some(n) = self.cast.spawn(world, look, clip, *place) {
                    world.despawn(who.e);
                    *who = n;
                }
            }
            let cas = self.calls.get(id).and_then(|c| c.casualty);
            if let (Some(h), Some(c)) = (scene.hurt, cas) {
                let look = Some(rescued_look(*id, 0));
                if !h.real && self.cast.ready(look) {
                    let clip = if c.state == 1 { "carried" } else { "lie" };
                    let place = Place::new(c.at.x, c.at.y, c.at.z);
                    if let Some(n) = self.cast.spawn(world, look, clip, place) {
                        world.despawn(h.e);
                        scene.hurt = Some(n);
                    }
                }
            }
        }
    }

    /// A rescue's scene: cars or flames, then the people.
    fn stage(&mut self, world: &mut World, id: u32) {
        let Some(c) = self.calls.get(&id).copied() else {
            return;
        };
        let mut scene = Scene::default();
        let f = Vec3::new(c.facing.x as f32, 0.0, c.facing.y as f32);
        let side = Vec3::new(f.z, 0.0, -f.x);
        let yaw = c.facing.x.atan2(c.facing.y).to_degrees() as f32;
        match c.kind {
            IncidentKind::CarCrash => {
                // Two cars nose to nose, knocked askew, smoking.
                for (k, model) in [(-1.0f32, CarModel::Cruiser), (1.0, CarModel::Bubble)] {
                    let p = c.at + side * (2.6 * k);
                    let place = Place::new(p.x, p.y, p.z)
                        .with_yaw_deg(yaw + 108.0 * k)
                        .with_roll_deg(4.0 * k);
                    let levels = crate::propview::moving_car_levels(model, (id % 7) as u8);
                    if let Ok(l) = world.spawn_lod_at(levels, &[place]) {
                        scene.cars.push(l);
                    }
                    // Its wheels, one of them buckled.
                    let turn = glam::Quat::from_rotation_y((yaw + 108.0 * k).to_radians());
                    if let (Some((x, zs, r)), Some(levels)) = (
                        crate::propview::wheel_layout(model),
                        crate::propview::wheel_levels(model),
                    ) {
                        let mut wheels = Vec::new();
                        for (i, z) in zs.iter().enumerate() {
                            for (j, sx) in [-x, x].iter().enumerate() {
                                let w = p + turn * Vec3::new(*sx as f32, r as f32, *z as f32);
                                let buckled = if i + j == 0 { 25.0 } else { 0.0 };
                                wheels.push(
                                    Place::new(w.x, w.y, w.z)
                                        .with_yaw_deg(yaw + 108.0 * k + buckled)
                                        .with_roll_deg(4.0 * k),
                                );
                            }
                        }
                        if let Ok(l) = world.spawn_lod_at(levels, &wheels) {
                            scene.cars.push(l);
                        }
                    }
                }
                let p = world.particles_mut();
                if let Ok(e) = p.start(ParticleEmitter::smoke(c.at + Vec3::Y * 1.0, 1.6)) {
                    scene.fire.push(e);
                }
                let _ = p.burst(ParticleEmitter::sparks(
                    c.at + Vec3::Y * 0.8,
                    Vec3::Y * 5.0,
                    60,
                ));
            }
            IncidentKind::Collapse => {
                // The rubble still settling: dust hanging over it.
                let p = world.particles_mut();
                let pile = c.at - f * 2.5 + Vec3::Y * 0.8;
                for (dx, size) in [(-2.5, 2.4), (2.5, 2.0)] {
                    // Pale plaster dust, not a fire's black smoke.
                    let dust = ParticleEmitter::smoke(pile + side * dx, size)
                        .with_color(Color::rgba(150, 140, 125, 120))
                        .with_secondary_color(Color::rgba(200, 192, 178, 50));
                    if let Ok(e) = p.start(dust) {
                        scene.fire.push(e);
                    }
                }
                let _ = p.burst(ParticleEmitter::dust(pile, 4.0, Color::rgb(158, 148, 133)));
            }
            _ => {
                // The front of the house ablaze: flames at the door and up
                // the wall, smoke over the roof.
                let wall = c.at - f * 3.0;
                let p = world.particles_mut();
                for (dx, up, size) in [(0.0, 0.5, 2.2), (-3.5, 4.0, 1.8), (3.5, 6.5, 1.8)] {
                    let at = wall + side * dx + Vec3::Y * up;
                    if let Ok(e) = p.start(ParticleEmitter::flames(at, size)) {
                        scene.fire.push(e);
                    }
                }
                for (up, size) in [(8.0, 3.5), (16.0, 5.0)] {
                    if let Ok(e) = p.start(ParticleEmitter::smoke(wall + Vec3::Y * up, size)) {
                        scene.fire.push(e);
                    }
                }
                if let Ok(e) = p.start(ParticleEmitter::embers(wall + Vec3::Y * 3.0, 2.0)) {
                    scene.fire.push(e);
                }
            }
        }
        self.scenes.insert(id, scene);
        self.people(world, id);
    }

    /// The people still trapped: pinned at a crash or in rubble, waving
    /// for help at a fire.
    fn people(&mut self, world: &mut World, id: u32) {
        let Some(c) = self.calls.get(&id).copied() else {
            return;
        };
        let clip = trapped_clip(c.kind);
        // The pinned face the wreck or the rubble, their legs under it.
        let turn = if clip == "trapped" { 180.0 } else { 0.0 };
        let yaw = c.facing.x.atan2(c.facing.y).to_degrees() as f32 + turn;
        let mut shown = Vec::new();
        for (k, p) in victims(c.kind, c.at.as_dvec3(), c.facing, c.left)
            .into_iter()
            .enumerate()
        {
            let place = Place::new(p.x as f32, p.y as f32, p.z as f32).with_yaw_deg(yaw);
            if let Some(s) = self
                .cast
                .spawn(world, Some(rescued_look(id, k)), clip, place)
            {
                shown.push((s, place));
            }
        }
        let scene = self.scenes.entry(id).or_default();
        for (old, _) in std::mem::replace(&mut scene.people, shown) {
            world.despawn(old.e);
        }
    }

    /// One more got out by hand: they walk off out of the way (the last
    /// one out is the hurt one, whose news follows).
    fn got_out(&mut self, world: &mut World, id: u32) {
        let Some(c) = self.calls.get(&id).copied() else {
            return;
        };
        let Some(scene) = self.scenes.get_mut(&id) else {
            return;
        };
        let f = Vec3::new(c.facing.x as f32, 0.0, c.facing.y as f32).normalize_or(Vec3::Z);
        let mut out = Vec::new();
        while scene.people.len() > c.left as usize {
            let Some((who, place)) = scene.people.pop() else {
                break;
            };
            if c.left == 0 && scene.people.is_empty() {
                world.despawn(who.e);
                continue;
            }
            let p = place.position;
            // Down to the street from a window or a door, then off: a
            // crash's away from the cars, a house's out from its front.
            let dir = if c.kind == IncidentKind::CarCrash && (p - c.at).dot(f) < 0.0 {
                -f
            } else {
                f
            };
            let at = Vec3::new(p.x, c.at.y, p.z);
            let line = THANKS[(id as usize * 3 + scene.people.len()) % THANKS.len()];
            let name = rescued_name(c.kind);
            self.walkers.push(Walker {
                who,
                at,
                dir,
                age: 0.0,
                unseen: 0.0,
                thank: 0.0,
                hero: None,
            });
            out.push((self.walkers.len() - 1, name, line));
        }
        for (w, name, line) in out {
            self.start_thanks(world, w, name, line);
        }
    }

    /// Someone set down out of a rescue bubble at `feet`: they walk on
    /// the way it carried them.
    pub fn walk_off(&mut self, world: &mut World, who: Shown, feet: Vec3, dir: Vec3) {
        let dir = Vec3::new(dir.x, 0.0, dir.z).normalize_or(Vec3::Z);
        let k = (feet.x.abs() * 7.0 + feet.z.abs() * 3.0) as usize;
        self.walkers.push(Walker {
            who,
            at: feet,
            dir,
            age: 0.0,
            unseen: 0.0,
            thank: 0.0,
            hero: None,
        });
        let w = self.walkers.len() - 1;
        self.start_thanks(world, w, "A townsperson", THANKS[k % THANKS.len()]);
    }

    /// Walker `w` turns to our hero (when near), thanks them out loud with
    /// a hand on the heart and a wave, then walks off.
    fn start_thanks(&mut self, world: &mut World, w: usize, name: &str, line: &str) {
        let hero = self.hero_at;
        let Some(wk) = self.walkers.get_mut(w) else {
            return;
        };
        let near = hero.filter(|h| h.distance(wk.at) < 40.0);
        if near.is_none() {
            if wk.who.real {
                let _ = world.play_animation(wk.who.e, "walk");
            }
            return;
        }
        wk.hero = near;
        wk.thank = THANK_SECONDS;
        if wk.who.real {
            let _ = world.play_animation(wk.who.e, "thanks");
        }
        self.said.push(crate::chat::Said {
            at: wk.at,
            range: crate::chat::SAY_RANGE,
            who: name.to_string(),
            text: line.to_string(),
            voice: crate::chat::Voice::Folk,
        });
    }

    /// A rescue bubble forms round the person at `at` (`hurt`: the hurt
    /// one, or the last out): the one standing there goes into it. Gives
    /// who, and how high over the bubble's foot to hold them.
    pub fn into_bubble(&mut self, world: &mut World, at: Vec3, hurt: bool) -> Option<(Shown, f32)> {
        let near = |p: Vec3| Vec3::new(p.x - at.x, 0.0, p.z - at.z).length() < 1.5;
        let mut found: Option<(u32, Shown)> = None;
        for (id, scene) in self.scenes.iter_mut() {
            if hurt {
                if let Some(h) = scene.hurt.take() {
                    scene.bubbled = true;
                    found = Some((*id, h));
                    break;
                }
            }
            if scene.people.last().is_some_and(|(_, p)| near(p.position)) {
                let (who, _) = scene.people.pop().expect("just looked");
                scene.bubbled |= hurt;
                found = Some((*id, who));
                break;
            }
        }
        if found.is_none() && hurt {
            // Handed over already (the news came first): the one on their
            // way into the ambulance floats there instead, and it waits.
            for l in self.leaving.iter_mut().filter(|l| l.t < 0.5) {
                if let Some((h, from, _)) = l.hurt {
                    if near(from) {
                        l.hurt = None;
                        l.load = crate::powerfx::FLOAT_SECONDS + 0.6;
                        found = Some((0, h));
                        break;
                    }
                }
            }
        }
        if found.is_none() {
            // Out already (the news came first): from among those walking off.
            if let Some(i) = self.walkers.iter().position(|w| w.age < 1.0 && near(w.at)) {
                let w = self.walkers.remove(i);
                found = Some((0, w.who));
            }
        }
        let who = match found {
            Some((_, who)) => who,
            None => {
                // Nobody drawn there: the nearest rescue's person.
                let (id, c) = self
                    .calls
                    .iter()
                    .filter(|(_, c)| c.kind.trapped() > 0)
                    .min_by(|a, b| a.1.at.distance(at).total_cmp(&b.1.at.distance(at)))?;
                if c.at.distance(at) > 40.0 {
                    return None;
                }
                let k = if hurt { 0 } else { c.left as usize };
                let place = Place::new(at.x, at.y, at.z);
                let clip = if hurt { "carried" } else { "wave" };
                let who = self
                    .cast
                    .spawn(world, Some(rescued_look(*id, k)), clip, place)?;
                if hurt {
                    if let Some(s) = self.scenes.get_mut(id) {
                        s.bubbled = true;
                    }
                }
                who
            }
        };
        if who.real {
            let _ = world.play_animation(who.e, if hurt { "carried" } else { "idle" });
        }
        // The hips at the bubble's middle for the curled-up hurt one.
        let lift = if hurt && who.real { 1.0 } else { 0.05 };
        Some((who, lift))
    }

    /// News of a rescue's hurt person: the ambulance arrives, they lie
    /// there, are lifted, or are handed over.
    fn casualty(&mut self, world: &mut World, id: u32, c: Casualty) {
        let Some(call) = self.calls.get_mut(&id) else {
            return;
        };
        // News as it happens (not a late joiner's snapshot): the
        // ambulance drives up rather than standing there already.
        let live = call.age > 1.0;
        let was = call.casualty.map(|k| k.state);
        call.casualty = Some(c);
        let scene = self.scenes.entry(id).or_default();
        if scene.van.is_none() && scene.van_due.is_none() && was.is_none() {
            scene.van_due = Some((c.medic, c.yaw, live && c.state != 2));
        }
        match c.state {
            2 => {
                self.flash = Some(("With the medic now. They'll pull through!".into(), 4.0));
                self.hand_over(world, id);
            }
            _ => {
                let clip = if c.state == 1 { "carried" } else { "lie" };
                let place = Place::new(c.at.x, c.at.y, c.at.z);
                match scene.hurt {
                    None => {
                        scene.hurt = self
                            .cast
                            .spawn(world, Some(rescued_look(id, 0)), clip, place);
                    }
                    Some(h) if h.real && was != Some(c.state) => {
                        let _ = world.play_animation(h.e, clip);
                    }
                    _ => {}
                }
                if let (0, Some(h)) = (c.state, scene.hurt) {
                    let lift = if h.real { 0.0 } else { 0.15 };
                    let _ = world.set_place(h.e, Place::new(c.at.x, c.at.y + lift, c.at.z));
                }
                if c.state == 1 && Some(c.carrier) == self.me {
                    self.flash = Some(("They're hurt: get them to the ambulance!".into(), 5.0));
                }
            }
        }
    }

    /// The hurt one is with the medic: they are lifted in at the back, the
    /// medic gets in at the front and the ambulance drives off.
    fn hand_over(&mut self, world: &mut World, id: u32) {
        // A word of thanks from the stretcher, and one from the medic.
        let medic = self
            .calls
            .get(&id)
            .and_then(|c| c.casualty)
            .map(|k| k.medic);
        if let (Some(m), Some(h)) = (medic, self.hero_at) {
            if m.distance(h) < 40.0 {
                let k = id as usize;
                for (who, text, up) in [
                    ("The hurt one", HURT_THANKS[k % HURT_THANKS.len()], 0.0),
                    ("The medic", MEDIC_WORDS[k % MEDIC_WORDS.len()], 0.7),
                ] {
                    self.said.push(crate::chat::Said {
                        at: m + Vec3::Y * up,
                        range: crate::chat::SAY_RANGE,
                        who: who.to_string(),
                        text: text.to_string(),
                        voice: crate::chat::Voice::Folk,
                    });
                }
            }
        }
        let Some(scene) = self.scenes.get_mut(&id) else {
            return;
        };
        if scene.van.is_none() {
            // Not here yet: it leaves as soon as it is.
            scene.leave = true;
            return;
        }
        if let Some(l) = depart(world, scene) {
            self.leaving.push(l);
        }
    }

    /// Puts each hurt person someone carries into their arms; `hero` gives
    /// a hero's feet and yaw (degrees).
    pub fn carry(&self, world: &mut World, hero: impl Fn(u32) -> Option<(Vec3, f32)>) {
        for (id, c) in &self.calls {
            let Some(cas) = c.casualty.filter(|k| k.state == 1) else {
                continue;
            };
            let (Some(h), Some((feet, yaw))) =
                (self.scenes.get(id).and_then(|s| s.hurt), hero(cas.carrier))
            else {
                continue;
            };
            let a = yaw.to_radians();
            let ahead = Vec3::new(a.sin(), 0.0, a.cos());
            let place = if h.real {
                // Across the hero's arms (see `poses::add_carry_clips`): the
                // hips just in front of the chest, legs to the hero's left.
                let p = feet + ahead * 0.4 + Vec3::Y * 0.98;
                Place::new(p.x, p.y, p.z).with_yaw_deg(yaw + 90.0)
            } else {
                let p = feet + ahead * 0.45 + Vec3::Y * 1.0;
                Place::new(p.x, p.y, p.z).with_yaw_deg(yaw)
            };
            let _ = world.set_place(h.e, place);
        }
    }

    /// Whether hero `id` is carrying someone hurt (their arms are full).
    pub fn is_carrying(&self, id: u32) -> bool {
        self.calls
            .values()
            .filter_map(|c| c.casualty)
            .any(|k| k.state == 1 && k.carrier == id)
    }

    fn unstage(&mut self, world: &mut World, id: u32) {
        if let Some(mut scene) = self.scenes.remove(&id) {
            // The pirates climb aboard and the airship sails off.
            if let Some((lod, from, yaw)) = scene.ship {
                let clear = self.town.as_ref().map_or(from.y as f64, |w| {
                    hw_sim::roosts::raid_sail_height(w, from.as_dvec3(), (yaw as f64).to_radians())
                });
                self.departing.push(Departing {
                    lod,
                    from,
                    yaw,
                    clear,
                    age: 0.0,
                });
            }
            for e in scene.ladders.drain(..) {
                world.despawn(e);
            }
            // The ambulance never just vanishes: it drives off.
            if let Some(l) = depart(world, &mut scene) {
                self.leaving.push(l);
            }
            for s in scene.medic.into_iter().chain(scene.hurt) {
                world.despawn(s.e);
            }
            for l in scene.cars {
                world.despawn_lod(l);
            }
            let p = world.particles_mut();
            for e in scene.fire {
                let _ = p.stop(e);
            }
            for (s, _) in scene.people {
                world.despawn(s.e);
            }
        }
    }

    /// A fire put out with a freeze ray: the flames die, a white steam
    /// hangs over the house front.
    fn put_out(&mut self, world: &mut World, id: u32) {
        let Some(c) = self.calls.get(&id).copied() else {
            return;
        };
        let Some(scene) = self.scenes.get_mut(&id) else {
            return;
        };
        let p = world.particles_mut();
        for e in scene.fire.drain(..) {
            let _ = p.stop(e);
        }
        let f = Vec3::new(c.facing.x as f32, 0.0, c.facing.y as f32);
        let steam = ParticleEmitter::smoke(c.at - f * 3.0 + Vec3::Y * 4.0, 2.5)
            .with_color(Color::rgba(235, 240, 245, 90))
            .with_secondary_color(Color::rgba(255, 255, 255, 30));
        if let Ok(e) = p.start(steam) {
            scene.fire.push(e);
        }
    }

    /// What our hero at `hero` can do for a rescue just now, if anything.
    pub fn help(&self, hero: Vec3) -> Option<Help> {
        self.help_at(hero).map(|h| h.0)
    }

    /// [`Self::help`], and at which call.
    fn help_at(&self, hero: Vec3) -> Option<(Help, u32)> {
        let reach = RESCUE_REACH as f32 + 0.5;
        if let Some((id, _)) = self.calls.iter().find(|(_, c)| {
            self.me
                .is_some_and(|me| c.casualty.is_some_and(|k| k.state == 1 && k.carrier == me))
        }) {
            return Some((Help::CarryThem, *id));
        }
        self.calls.iter().find_map(|(id, c)| {
            let help = match c.casualty {
                _ if c.kind == IncidentKind::Surfacing => (c.left > 0
                    && c.at.distance(hero) < hw_sim::incidents::CALM_REACH as f32 + 2.5)
                    .then_some(Help::TalkDown),
                Some(k) if k.state == 0 && k.at.distance(hero) < reach => Some(Help::LiftThem),
                _ => victims(c.kind, c.at.as_dvec3(), c.facing, c.left)
                    .last()
                    .is_some_and(|v| v.as_vec3().distance(hero) < reach)
                    .then_some(Help::FreeThem),
            };
            help.map(|h| (h, *id))
        })
    }

    /// The ambulance we are carrying someone to.
    pub fn carrying(&self) -> Option<Vec3> {
        let me = self.me?;
        self.calls
            .values()
            .filter_map(|c| c.casualty)
            .find(|k| k.state == 1 && k.carrier == me)
            .map(|k| k.medic)
    }

    /// Time passes; `holding` is where our hero is while the help key is
    /// held down (the bar fills between the server's steps).
    pub fn update(&mut self, dt: f32, holding: Option<Vec3>) {
        let here = holding
            .and_then(|h| self.help_at(h))
            .filter(|h| h.0 != Help::CarryThem);
        self.holding = here.is_some();
        for (id, c) in self.calls.iter_mut() {
            c.age += dt;
            let target = c.held as f32 / 16.0;
            if here.is_some_and(|h| h.1 == *id) {
                let need = if c.kind == IncidentKind::Surfacing {
                    hw_sim::incidents::CALM_SECONDS
                } else if c.casualty.is_some_and(|k| k.state == 0) {
                    hw_sim::incidents::LIFT_SECONDS
                } else if c.doused {
                    c.kind.rescue_seconds() * hw_sim::incidents::DOUSED_QUICKER
                } else {
                    c.kind.rescue_seconds()
                };
                // Ahead of the server's word by at most one step.
                c.bar = (c.bar + dt / need as f32).clamp(target, (target + 1.0 / 16.0).min(1.0));
            } else {
                c.bar += (target - c.bar) * (1.0 - (-dt * 8.0).exp());
            }
        }
        if let Some((_, t)) = self.flash.as_mut() {
            *t -= dt;
        }
        if self.flash.as_ref().is_some_and(|f| f.1 <= 0.0) {
            self.flash = None;
        }
    }

    /// The radio on screen, top left: each call with its distance, the way
    /// to go (against where the camera looks) and a con colour.
    pub fn panel(&self, ctx: &egui::Context, eye: Vec3, look: Vec3, mine: u8, hero: Option<Vec3>) {
        if let Some((help, id)) = hero.and_then(|h| self.help_at(h)) {
            let held = self.holding;
            let text = match help {
                Help::FreeThem if held => "Getting them out... keep holding E".to_string(),
                Help::FreeThem => "Hold E to help them out".to_string(),
                Help::LiftThem if held => "Lifting them... keep holding E".to_string(),
                Help::LiftThem => "Hold E to lift them".to_string(),
                Help::TalkDown if held => "Talking them down... keep holding E".to_string(),
                Help::TalkDown => "Hold E to talk them down (or fight them)".to_string(),
                Help::CarryThem => {
                    let d = match (hero, self.carrying()) {
                        (Some(h), Some(m)) => {
                            glam::Vec2::new(m.x - h.x, m.z - h.z).length() - MEDIC_REACH as f32
                        }
                        _ => 0.0,
                    };
                    format!(
                        "Carry them to the ambulance (+ on the compass), {:.0} m",
                        d.max(0.0)
                    )
                }
            };
            egui::Area::new(egui::Id::new("rescue"))
                // Words only: the pointer over it must not take the keys.
                .interactable(false)
                .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -170.0))
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_black_alpha(170))
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(10, 5))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(text)
                                    .color(egui::Color32::from_rgb(255, 230, 140))
                                    .size(17.0)
                                    .strong(),
                            );
                            // How far along: fills while E is held.
                            if help != Help::CarryThem {
                                let bar = self.calls.get(&id).map_or(0.0, |c| c.bar);
                                let fill = if held {
                                    egui::Color32::from_rgb(255, 200, 70)
                                } else {
                                    egui::Color32::from_rgb(150, 125, 70)
                                };
                                ui.add(
                                    egui::ProgressBar::new(bar)
                                        .desired_width(280.0)
                                        .desired_height(12.0)
                                        .fill(fill),
                                );
                            }
                        });
                });
        }
        if self.calls.is_empty() && self.flash.is_none() && self.story.is_none() {
            return;
        }
        let mut calls: Vec<(&u32, &Call)> = self.calls.iter().collect();
        calls.sort_by(|a, b| a.1.at.distance(eye).total_cmp(&b.1.at.distance(eye)));
        egui::Area::new(egui::Id::new("radio"))
            // Words only: the pointer over it must not take the keys.
            .interactable(false)
            .anchor(egui::Align2::LEFT_TOP, egui::vec2(10.0, 10.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgba_unmultiplied(20, 24, 30, 210))
                    .stroke(egui::Stroke::new(
                        1.5_f32,
                        egui::Color32::from_rgb(255, 196, 60),
                    ))
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("WRIST RADIO")
                                .color(egui::Color32::from_rgb(255, 204, 70))
                                .strong()
                                .size(13.0),
                        );
                        if let Some(st) = &self.story {
                            ui.label(
                                egui::RichText::new(&st.heading)
                                    .color(STORY_GOLD)
                                    .size(12.0),
                            );
                            let mut goal = st.goal.clone();
                            if let Some(at) = st.at {
                                let d = at - eye;
                                let flat = glam::Vec2::new(d.x, d.z);
                                let ahead = glam::Vec2::new(look.x, look.z).normalize_or_zero();
                                goal += &format!(
                                    "  {:.0} m at {} o'clock",
                                    flat.length(),
                                    clock(ahead, flat)
                                );
                            }
                            ui.label(
                                egui::RichText::new(goal)
                                    .color(STORY_GOLD)
                                    .size(15.0)
                                    .strong(),
                            );
                            ui.separator();
                        }
                        if let Some((text, _)) = &self.flash {
                            ui.label(
                                egui::RichText::new(text)
                                    .color(egui::Color32::from_rgb(255, 240, 200))
                                    .italics(),
                            );
                        }
                        for (_, c) in calls {
                            let d = c.at - eye;
                            let flat = glam::Vec2::new(d.x, d.z);
                            let ahead = glam::Vec2::new(look.x, look.z).normalize_or_zero();
                            let hour = clock(ahead, flat);
                            let col = con_colour(c.level, mine);
                            let blink = c.age < 4.0 && (c.age * 3.0) as i32 % 2 == 0;
                            let name = egui::RichText::new(format!(
                                "{}  {:.0} m at {hour} o'clock",
                                c.kind.name(),
                                flat.length()
                            ))
                            .color(col)
                            .size(15.0);
                            ui.label(if blink { name.strong() } else { name });
                        }
                    });
            });
    }

    pub fn clear(&mut self, world: &mut World) {
        let ids: Vec<u32> = self.scenes.keys().copied().collect();
        for id in ids {
            self.unstage(world, id);
        }
        for d in std::mem::take(&mut self.departing) {
            world.despawn_lod(d.lod);
        }
        for l in std::mem::take(&mut self.leaving) {
            world.despawn_lod(l.lod);
            for (s, _, _) in l.medic.into_iter().chain(l.hurt) {
                world.despawn(s.e);
            }
        }
        for w in std::mem::take(&mut self.walkers) {
            world.despawn(w.who.e);
        }
        self.calls.clear();
        self.flash = None;
    }
}

/// Where the medic waits by the back doors of an ambulance at `at` (yaw
/// in radians), facing the trouble at `call`.
fn medic_place(at: Vec3, yaw: f32, call: Option<Vec3>) -> Place {
    let f = Vec3::new(yaw.sin(), 0.0, yaw.cos());
    let left = Vec3::new(f.z, 0.0, -f.x);
    let p = at - f * 3.6 + left * 1.1;
    let to = call.map_or(-f, |c| c - p);
    Place::new(p.x, p.y, p.z).with_yaw_deg(to.x.atan2(to.z).to_degrees())
}

/// The ambulance at `scene` drives off, the hurt one lifted in at the
/// back and the medic walking round to the cab first; none if it has not
/// come yet.
fn depart(world: &mut World, scene: &mut Scene) -> Option<Leaving> {
    let van = scene.van.take()?;
    let f = Vec3::new(van.yaw.sin(), 0.0, van.yaw.cos());
    let left = Vec3::new(f.z, 0.0, -f.x);
    // Still pulling up: on from where it is.
    let now = van.route.as_ref().map_or(van.at, |r| r.at(r.s).0);
    let mut pts = vec![now];
    if now.distance(van.at) > 0.5 {
        pts.push(van.at);
    }
    pts.extend(van.out.iter().skip(1).copied());
    let medic = scene.medic.take().map(|m| {
        if m.real {
            let _ = world.play_animation(m.e, "walk");
        }
        (
            m,
            van.at - f * 3.6 + left * 1.1,
            van.at + f * 0.9 + left * 1.15,
        )
    });
    let hurt = scene.hurt.take().map(|h| {
        let from = world
            .world_transform(h.e)
            .map(|m| m.w_axis.truncate())
            .unwrap_or(van.at);
        (h, from, van.at - f * 3.2 + Vec3::Y * 0.9)
    });
    // A bubble's hurt one is still floating over: wait for them.
    let load = if scene.bubbled {
        crate::powerfx::FLOAT_SECONDS + 0.6
    } else {
        LOAD_SECONDS
    };
    Some(Leaving {
        lod: van.lod,
        route: Route::new(pts),
        t: 0.0,
        load,
        medic,
        hurt,
        unseen: 0.0,
    })
}

/// The medic's look in [`person`]: all in white.
const MEDIC_LOOK: u32 = 4;

/// Someone hurt, lying on their back along x (carried across the arms).
fn lying(look: u32) -> Mesh {
    let shirt = [rgb(70, 130, 190), rgb(210, 190, 90), rgb(150, 80, 120)][look as usize % 3];
    let mut m = Mesh::new();
    let _ = m.add_box((-0.55, 0.12, 0.0), (0.9, 0.22, 0.34), rgb(60, 62, 80));
    let _ = m.add_box((0.25, 0.14, 0.0), (0.62, 0.26, 0.46), shirt);
    let _ = m.add_box((0.7, 0.14, 0.0), (0.26, 0.24, 0.24), rgb(224, 182, 150));
    // A bandage round the head.
    let _ = m.add_box((0.7, 0.2, 0.0), (0.12, 0.26, 0.27), rgb(245, 245, 240));
    m
}

/// Someone in trouble: plain clothes in one of a few colours, an arm up
/// waving for help.
pub fn person(look: u32) -> Mesh {
    const SHIRTS: [(u8, u8, u8); 5] = [
        (70, 130, 190),
        (210, 190, 90),
        (150, 80, 120),
        (90, 150, 90),
        (220, 220, 210),
    ];
    let s = SHIRTS[look as usize % SHIRTS.len()];
    let shirt = rgb(s.0, s.1, s.2);
    let skin = rgb(224, 182, 150);
    let mut m = Mesh::new();
    let _ = m.add_box((0.0, 0.45, 0.0), (0.34, 0.9, 0.26), rgb(60, 62, 80));
    let _ = m.add_box((0.0, 1.17, 0.0), (0.46, 0.56, 0.28), shirt);
    let _ = m.add_box((0.0, 1.6, 0.0), (0.24, 0.26, 0.24), skin);
    // One arm down, one up and waving.
    let _ = m.add_box((-0.3, 1.1, 0.0), (0.12, 0.55, 0.12), shirt);
    let _ = m.add_box((0.3, 1.72, 0.0), (0.12, 0.6, 0.12), shirt);
    let _ = m.add_box((0.3, 2.06, 0.0), (0.12, 0.12, 0.12), skin);
    m
}

/// Which way `to` lies from `ahead` (the way the camera looks), as a
/// pilot would call it: 12 o'clock dead ahead, 3 o'clock to the right.
pub fn clock(ahead: glam::Vec2, to: glam::Vec2) -> u8 {
    if ahead == glam::Vec2::ZERO || to == glam::Vec2::ZERO {
        return 12;
    }
    // y is up and the world right-handed: looking along +z, +x is on the
    // left. `a` is how far `to` turns from `ahead`, positive to the right.
    let a = ahead.perp_dot(to).atan2(ahead.dot(to));
    let h = (12 + (a / (std::f32::consts::PI / 6.0)).round() as i32).rem_euclid(12);
    if h == 0 {
        12
    } else {
        h as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn calls_give_the_clock_direction() {
        let ahead = Vec2::new(0.0, 1.0);
        assert_eq!(clock(ahead, Vec2::new(0.0, 5.0)), 12);
        assert_eq!(clock(ahead, Vec2::new(0.0, -5.0)), 6);
        assert_eq!(clock(ahead, Vec2::new(5.0, 0.0)), 9);
        assert_eq!(clock(ahead, Vec2::new(-5.0, 0.0)), 3);
    }
}
