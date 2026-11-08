//! The town's props in the game: one instanced LOD set per model, so tens
//! of thousands of lamps, trees and cars cost a few draws.
//!
//! Trees and shrubs come from Engine's vegetation. Everything else is made
//! here, in the golden age's taste: lanterns, deco double lamps and atomic
//! saucer lamps; finned two-tone cruisers, bubble cars and hover cars;
//! rocket mailboxes, chrome hydrants, deco benches.

use crate::citymesh::{MeshBuilder, Surf};
use engine::prelude::*;
use engine::vegetation::{shrub_model, tree_model, TreeSpecies};
use glam::DVec3;
use hw_gen::urban::props::{CarModel, Prop, PropKind, Species};
use hw_gen::urban::world::TownWorld;
use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::TAU;

/// The props of a town, spawned once for the whole town (Engine's LOD sets
/// draw only what is near enough).
pub struct PropView {
    pub sets: Vec<LodId>,
    pub count: usize,
    /// Per set: every instance's place as it stands, and its model's bounds.
    places: Vec<Vec<Place>>,
    bounds: Vec<(Vec3, Vec3)>,
    /// Per prop: its set and instance.
    slot: Vec<(usize, usize)>,
    /// Sets whose places changed since the last [`PropView::flush`].
    dirty: BTreeSet<usize>,
}

impl PropView {
    pub fn spawn(world: &mut World, town: &TownWorld) -> EngineResult<Self> {
        let mut groups: BTreeMap<PropKind, Vec<Place>> = BTreeMap::new();
        let mut at = Vec::with_capacity(town.props.len());
        for p in &town.props {
            let g = groups.entry(p.kind).or_default();
            at.push(g.len());
            g.push(place(p));
        }
        // Growing trees takes a moment: grow every model at once.
        let kinds: Vec<PropKind> = groups.keys().copied().collect();
        let levels: Vec<EngineResult<Vec<LodLevel>>> = std::thread::scope(|s| {
            let handles: Vec<_> = kinds
                .iter()
                .map(|k| s.spawn(move || model_levels(*k)))
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("model thread"))
                .collect()
        });
        let mut sets = Vec::new();
        let mut bounds = Vec::new();
        for (kind, levels) in kinds.iter().zip(levels) {
            let levels = levels?;
            bounds.push(
                levels
                    .first()
                    .and_then(|l| l.mesh.bounds())
                    .unwrap_or((Vec3::ZERO, Vec3::ONE)),
            );
            sets.push(world.spawn_lod_at(levels, &groups[kind])?);
        }
        let slot = town
            .props
            .iter()
            .zip(at)
            .map(|(p, j)| (kinds.binary_search(&p.kind).unwrap_or(0), j))
            .collect();
        Ok(Self {
            sets,
            count: town.props.len(),
            places: kinds
                .iter()
                .map(|k| groups.remove(k).unwrap_or_default())
                .collect(),
            bounds,
            slot,
            dirty: BTreeSet::new(),
        })
    }

    /// Draws prop `i` broken, fallen the way `fell` says (a yaw byte, see
    /// `hw_sim::props`), or whole again (`None`).
    pub fn set_broken(&mut self, town: &TownWorld, i: usize, fell: Option<u8>) {
        let (Some(p), Some(&(k, j))) = (town.props.get(i), self.slot.get(i)) else {
            return;
        };
        let mut at = place(p);
        if let Some(fell) = fell {
            at = broken_place(p, at, self.bounds[k], fell);
        }
        self.places[k][j] = at;
        self.dirty.insert(k);
    }

    /// Sends changed places to Engine.
    pub fn flush(&mut self, world: &mut World) -> EngineResult<()> {
        for k in std::mem::take(&mut self.dirty) {
            world.set_lod_places(self.sets[k], &self.places[k])?;
        }
        Ok(())
    }
}

/// Where a broken prop lies: posts and trees fallen over (their foot where
/// it stood, laid along `fell`), cars on their roofs, benches, bins, shrubs
/// and kiosks crushed flat.
fn broken_place(p: &Prop, whole: Place, (_, hi): (Vec3, Vec3), fell: u8) -> Place {
    use hw_sim::props::{byte_to_yaw, toughness, Breaks};
    let Some((_, how)) = toughness(p.kind) else {
        return whole;
    };
    let yaw = byte_to_yaw(fell);
    let s = p.scale as f32;
    match how {
        Breaks::Topples => {
            // Pitch +90 turns the model's +y onto its +z, which the yaw turns
            // along the fall. Trees lean on
            // their crowns rather than lying flat.
            // (Its +z, the front, ends up underneath.)
            let tree = matches!(p.kind, PropKind::Tree(..));
            let pitch = if tree { 76.0 } else { 90.0 };
            let lift = if tree { 0.0 } else { hi.z.max(0.05) * s };
            Place {
                position: whole.position + Vec3::Y * lift,
                yaw_degrees: yaw.to_degrees() as f32,
                pitch_degrees: pitch,
                ..whole
            }
        }
        Breaks::Flips => {
            let shove = Vec3::new(yaw.sin() as f32, 0.0, yaw.cos() as f32) * 0.6;
            Place {
                position: whole.position + shove + Vec3::Y * (hi.y * s),
                roll_degrees: 180.0,
                ..whole
            }
        }
        Breaks::Smashed => Place {
            stretch: Vec3::new(1.15, 0.18, 1.15),
            tint: Color::rgb(140, 133, 128),
            ..whole
        },
    }
}

fn place(p: &Prop) -> Place {
    let yaw = p.dir.x.atan2(p.dir.y).to_degrees();
    Place::new(p.pos.x as f32, p.y as f32, p.pos.y as f32)
        .with_yaw_deg(yaw as f32)
        .with_scale(p.scale as f32)
}

fn species(s: Species) -> TreeSpecies {
    match s {
        Species::Plane => TreeSpecies::Plane,
        Species::Linden => TreeSpecies::Linden,
        Species::Maple => TreeSpecies::Maple,
        Species::Oak => TreeSpecies::Oak,
        Species::Birch => TreeSpecies::Birch,
        Species::Pine => TreeSpecies::Pine,
        Species::Palm => TreeSpecies::Palm,
        Species::Cypress => TreeSpecies::Cypress,
        Species::Chestnut => TreeSpecies::Chestnut,
        Species::Ginkgo => TreeSpecies::Ginkgo,
    }
}

/// The detail levels of one kind of prop.
pub fn model_levels(kind: PropKind) -> EngineResult<Vec<LodLevel>> {
    use crate::propmesh as pm;
    let three = |make: &dyn Fn(Lod) -> Mesh, d: [f32; 3]| {
        vec![
            LodLevel::new(make(Lod::Near), d[0]),
            LodLevel::new(make(Lod::Mid), d[1]),
            LodLevel::new(make(Lod::Far), d[2]),
        ]
    };
    let two = |near: Mesh, near_m: f32, far: Mesh, far_m: f32| {
        vec![LodLevel::new(near, near_m), LodLevel::new(far, far_m)]
    };
    Ok(match kind {
        PropKind::Tree(s, v) => tree_model(species(s), v as u32)?.lod_levels(),
        PropKind::Shrub(v) => shrub_model(v as u32)?.lod_levels(),
        PropKind::Lamp(style) => three(&|l| pm::lamp(style, l), [30.0, 140.0, 700.0]),
        PropKind::Car(m, paint) => parked_car_levels(m, paint),
        PropKind::TrafficLight => three(&pm::traffic_light, [30.0, 160.0, 500.0]),
        PropKind::Fountain => vec![LodLevel::new(fountain(), 900.0)],
        PropKind::Bench => three(&pm::bench, [25.0, 80.0, 160.0]),
        PropKind::Bin => three(&pm::bin, [20.0, 60.0, 120.0]),
        PropKind::Hydrant => three(&pm::hydrant, [20.0, 60.0, 120.0]),
        PropKind::Mailbox => three(&pm::mailbox, [25.0, 80.0, 180.0]),
        PropKind::Bollard => three(&pm::bollard, [20.0, 60.0, 150.0]),
        PropKind::Kiosk => two(kiosk(true), 200.0, kiosk(false), 600.0),
        PropKind::AdPillar => vec![LodLevel::new(ad_pillar(), 250.0)],
        PropKind::Statue(pose) => vec![LodLevel::new(statue_mesh(pose), 500.0)],
        PropKind::Barricade(piece) => crate::edgeview::piece_levels(piece),
    })
}

// ---------------------------------------------------------------- shapes

fn bx(b: &mut MeshBuilder, lo: DVec3, hi: DVec3, c: Color) {
    let (lo, hi) = (lo.min(hi), lo.max(hi));
    b.oriented_box((lo + hi) * 0.5, DVec3::X, (hi - lo) * 0.5, c);
}

/// A closed ellipsoid.
fn ellipsoid(b: &mut MeshBuilder, c: DVec3, r: DVec3, seg: usize, rings: usize, col: Color) {
    let at = |i: usize, k: usize| {
        let a = TAU * (i % seg) as f64 / seg as f64;
        let e = std::f64::consts::PI * (k as f64 / rings as f64 - 0.5);
        c + DVec3::new(
            r.x * e.cos() * a.cos(),
            r.y * e.sin(),
            r.z * e.cos() * a.sin(),
        )
    };
    for k in 0..rings {
        for i in 0..seg {
            let (p0, p1, p2, p3) = (at(i, k), at(i + 1, k), at(i + 1, k + 1), at(i, k + 1));
            let out = (p0 + p1 + p2 + p3) * 0.25 - c;
            if k == 0 {
                b.tri([p0, p2, p3], out, col);
            } else if k + 1 == rings {
                b.tri([p0, p1, p2], out, col);
            } else {
                b.quad([p0, p1, p2, p3], out, col);
            }
        }
    }
}

fn glow(b: &mut MeshBuilder, f: impl FnOnce(&mut MeshBuilder)) {
    b.surf = Surf::Glow;
    f(b);
    b.surf = Surf::Plain;
}

fn metal(b: &mut MeshBuilder, f: impl FnOnce(&mut MeshBuilder)) {
    b.surf = Surf::Metal;
    f(b);
    b.surf = Surf::Plain;
}

fn glass(b: &mut MeshBuilder, f: impl FnOnce(&mut MeshBuilder)) {
    b.surf = Surf::Glass;
    f(b);
    b.surf = Surf::Plain;
}

fn c3(t: (u8, u8, u8)) -> Color {
    rgb(t.0, t.1, t.2)
}

// ---------------------------------------------------------------- lamps

// ---------------------------------------------------------------- cars

pub use crate::carmesh::wheel_layout;
use crate::carmesh::{ambulance_into, car_into, taxi_into, wheel, Lod};

fn car_mesh(model: CarModel, paint: u8, lod: Lod, wheels: bool) -> Mesh {
    let mut b = MeshBuilder::new();
    car_into(&mut b, model, paint, lod, wheels);
    b.finish()
}

/// A parked golden-age car, its nose to +z: close up, down the street, far.
fn parked_car_levels(model: CarModel, paint: u8) -> Vec<LodLevel> {
    vec![
        LodLevel::new(car_mesh(model, paint, Lod::Near, true), 35.0),
        LodLevel::new(car_mesh(model, paint, Lod::Mid, true), 140.0),
        LodLevel::new(car_mesh(model, paint, Lod::Far, true), 650.0),
    ]
}

/// A car driving in traffic: its body (the wheels turn on their own, see
/// [`wheel_levels`], as far as traffic draws them), then far off with
/// wheels of its own.
pub fn moving_car_levels(model: CarModel, paint: u8) -> Vec<LodLevel> {
    vec![
        LodLevel::new(car_mesh(model, paint, Lod::Near, false), 35.0),
        LodLevel::new(car_mesh(model, paint, Lod::Mid, false), 120.0),
        LodLevel::new(car_mesh(model, paint, Lod::Far, true), 700.0),
    ]
}

/// The ambulance that waits for a rescue's hurt: a long 1950s coach-built
/// ambulance (see [`crate::carmesh::ambulance_into`]). Nose to +z, wheels
/// on the ground; close up, down the street and far.
pub fn ambulance_levels() -> Vec<LodLevel> {
    let make = |lod: Lod| {
        let mut b = MeshBuilder::new();
        ambulance_into(&mut b, lod);
        b.finish()
    };
    vec![
        LodLevel::new(make(Lod::Near), 40.0),
        LodLevel::new(make(Lod::Mid), 160.0),
        LodLevel::new(make(Lod::Far), 700.0),
    ]
}

/// One wheel of `model`, centred, its axle along x: tyre, whitewalls and a
/// chrome hubcap with a spinner on both faces, so it can be seen to turn.
pub fn wheel_levels(model: CarModel) -> Option<Vec<LodLevel>> {
    let (_, _, r) = wheel_layout(model)?;
    let make = |lod: Lod| {
        let mut b = MeshBuilder::new();
        wheel(&mut b, DVec3::ZERO, r, 0.22, lod);
        b.finish()
    };
    Some(vec![
        LodLevel::new(make(Lod::Near), 35.0),
        LodLevel::new(make(Lod::Mid), 260.0),
    ])
}

/// Flying taxis: close up, in the next lane, far.
pub fn taxi_levels() -> Vec<LodLevel> {
    let make = |lod: Lod| {
        let mut b = MeshBuilder::new();
        taxi_into(&mut b, lod);
        b.finish()
    };
    vec![
        LodLevel::new(make(Lod::Near), 50.0),
        LodLevel::new(make(Lod::Mid), 250.0),
        LodLevel::new(make(Lod::Far), 1200.0),
    ]
}

/// A private hover car flying in a sky lane.
pub fn sky_car_levels(paint: u8) -> Vec<LodLevel> {
    vec![
        LodLevel::new(car_mesh(CarModel::Hover, paint, Lod::Near, false), 50.0),
        LodLevel::new(car_mesh(CarModel::Hover, paint, Lod::Mid, false), 250.0),
        LodLevel::new(car_mesh(CarModel::Hover, paint, Lod::Far, false), 1200.0),
    ]
}

// ---------------------------------------------------------------- street

/// A round news kiosk: cream walls, a glowing sign band, a red saucer roof
/// and an atom on a mast.
fn kiosk(near: bool) -> Mesh {
    let mut b = MeshBuilder::new();
    let seg = if near { 16 } else { 8 };
    let cream = rgb(236, 226, 200);
    let red = rgb(196, 54, 44);
    b.cylinder(DVec3::ZERO, (1.25, 1.25), 0.5, seg, true, red);
    b.cylinder(DVec3::Y * 0.5, (1.2, 1.2), 1.7, seg, false, cream);
    // The counter window, dark glass towards the street.
    glass(&mut b, |b| {
        bx(
            b,
            DVec3::new(-0.75, 1.0, 1.0),
            DVec3::new(0.75, 1.9, 1.23),
            rgb(50, 70, 90),
        )
    });
    // The sign band all round.
    glow(&mut b, |b| {
        b.cylinder(
            DVec3::Y * 2.2,
            (1.24, 1.24),
            0.45,
            seg,
            false,
            rgb(255, 210, 120),
        )
    });
    b.cylinder(DVec3::Y * 2.65, (1.2, 1.9), 0.25, seg, false, red);
    b.cylinder(DVec3::Y * 2.9, (1.9, 0.0), 0.7, seg, true, red);
    if near {
        metal(&mut b, |b| {
            b.cylinder(
                DVec3::Y * 3.5,
                (0.04, 0.04),
                0.8,
                4,
                false,
                rgb(200, 206, 214),
            )
        });
        glow(&mut b, |b| {
            ellipsoid(
                b,
                DVec3::Y * 4.4,
                DVec3::splat(0.14),
                8,
                4,
                rgb(140, 230, 255),
            )
        });
    }
    b.finish()
}

/// A round advertising pillar in green iron, posters all round, a dome cap.
fn ad_pillar() -> Mesh {
    let mut b = MeshBuilder::new();
    let green = rgb(40, 82, 60);
    b.cylinder(DVec3::ZERO, (0.62, 0.62), 0.3, 14, true, green);
    // Posters: bands of colour round the drum.
    let posters = [
        (232, 196, 90),
        (210, 70, 60),
        (90, 150, 200),
        (240, 236, 220),
        (220, 120, 170),
        (120, 190, 120),
    ];
    let seg = 12;
    for i in 0..seg {
        let (a0, a1) = (
            TAU * i as f64 / seg as f64,
            TAU * (i + 1) as f64 / seg as f64,
        );
        let at = |a: f64, y: f64| DVec3::new(0.55 * a.cos(), y, 0.55 * a.sin());
        let mid = (a0 + a1) * 0.5;
        let out = DVec3::new(mid.cos(), 0.0, mid.sin());
        let c = posters[(i * 5 + i / 3) % posters.len()];
        b.quad(
            [at(a0, 0.3), at(a1, 0.3), at(a1, 2.5), at(a0, 2.5)],
            out,
            c3(c),
        );
    }
    b.cylinder(DVec3::Y * 2.5, (0.62, 0.66), 0.15, 14, false, green);
    b.dome(DVec3::Y * 2.65, 0.66, 14, 4, green);
    metal(&mut b, |b| {
        b.cylinder(
            DVec3::Y * 3.25,
            (0.06, 0.0),
            0.35,
            6,
            false,
            rgb(214, 170, 72),
        )
    });
    b.finish()
}

/// A bronze hero on a stone plinth, facing +z.
pub fn statue_mesh(pose: u8) -> Mesh {
    let mut b = MeshBuilder::new();
    statue(&mut b, pose);
    b.finish()
}

/// A bronze hero on a stone plinth: fist to the sky (0), hands on hips (1)
/// or taking off, one arm forward (2).
pub fn statue(b: &mut MeshBuilder, pose: u8) {
    let stone = rgb(200, 194, 182);
    let bronze = rgb(126, 92, 52);
    bx(
        b,
        DVec3::new(-1.0, 0.0, -1.0),
        DVec3::new(1.0, 0.25, 1.0),
        stone,
    );
    bx(
        b,
        DVec3::new(-0.8, 0.25, -0.8),
        DVec3::new(0.8, 1.6, 0.8),
        stone,
    );
    bx(
        b,
        DVec3::new(-0.9, 1.6, -0.9),
        DVec3::new(0.9, 1.75, 0.9),
        stone,
    );
    // A gold plaque in front.
    metal(b, |b| {
        bx(
            b,
            DVec3::new(-0.45, 0.7, 0.8),
            DVec3::new(0.45, 1.15, 0.83),
            rgb(214, 170, 72),
        )
    });
    let y0 = 1.75;
    let fly = pose == 2;
    metal(b, |b| {
        // Legs apart (one bent back when taking off).
        for side in [-1.0, 1.0] {
            let back = if fly && side > 0.0 { -0.35 } else { 0.0 };
            ellipsoid(
                b,
                DVec3::new(0.2 * side, y0 + 0.55, back),
                DVec3::new(0.13, 0.55, 0.14),
                8,
                5,
                bronze,
            );
        }
        // Torso, chest out; head.
        ellipsoid(
            b,
            DVec3::new(0.0, y0 + 1.45, 0.0),
            DVec3::new(0.36, 0.48, 0.22),
            10,
            6,
            bronze,
        );
        ellipsoid(
            b,
            DVec3::new(0.0, y0 + 2.1, 0.02),
            DVec3::new(0.13, 0.16, 0.14),
            8,
            5,
            bronze,
        );
        // Arms.
        let arm = |b: &mut MeshBuilder, side: f64, to: DVec3| {
            let sh = DVec3::new(0.42 * side, y0 + 1.75, 0.0);
            // Three beads from shoulder to fist.
            for t in [0.15, 0.5, 0.85] {
                let p = sh.lerp(to, t);
                ellipsoid(b, p, DVec3::splat(0.1 + 0.02 * (1.0 - t)), 6, 4, bronze);
            }
        };
        match pose {
            0 => {
                arm(b, 1.0, DVec3::new(0.55, y0 + 2.6, 0.05));
                arm(b, -1.0, DVec3::new(-0.38, y0 + 1.2, 0.1));
            }
            1 => {
                arm(b, 1.0, DVec3::new(0.3, y0 + 1.2, -0.05));
                arm(b, -1.0, DVec3::new(-0.3, y0 + 1.2, -0.05));
            }
            _ => {
                arm(b, 1.0, DVec3::new(0.35, y0 + 2.35, 0.6));
                arm(b, -1.0, DVec3::new(-0.45, y0 + 1.25, -0.2));
            }
        }
        // Fists.
        let fist = match pose {
            0 => DVec3::new(0.55, y0 + 2.62, 0.05),
            1 => DVec3::new(0.3, y0 + 1.15, -0.05),
            _ => DVec3::new(0.35, y0 + 2.4, 0.65),
        };
        ellipsoid(b, fist, DVec3::splat(0.12), 6, 4, bronze);
        // The cape, flowing back.
        let sway = if fly { 0.9 } else { 0.45 };
        b.quad(
            [
                DVec3::new(-0.38, y0 + 1.85, -0.2),
                DVec3::new(0.38, y0 + 1.85, -0.2),
                DVec3::new(0.55, y0 + 0.35, -0.2 - sway),
                DVec3::new(-0.55, y0 + 0.35, -0.2 - sway),
            ],
            -DVec3::Z + DVec3::Y * 0.3,
            bronze,
        );
        b.quad(
            [
                DVec3::new(-0.38, y0 + 1.85, -0.18),
                DVec3::new(0.38, y0 + 1.85, -0.18),
                DVec3::new(0.55, y0 + 0.35, -0.18 - sway),
                DVec3::new(-0.55, y0 + 0.35, -0.18 - sway),
            ],
            DVec3::Z - DVec3::Y * 0.3,
            bronze,
        );
    });
}

/// A cast-iron mooring bollard: a squat post with a flared cap.
/// A round basin with a bowl on a column and a glowing atom on top.
fn fountain() -> Mesh {
    let mut b = MeshBuilder::new();
    let stone = rgb(208, 200, 184);
    b.cylinder(DVec3::ZERO, (3.2, 3.2), 0.6, 32, true, stone);
    glass(&mut b, |b| {
        b.cylinder(
            DVec3::Y * 0.6,
            (2.95, 2.95),
            0.02,
            32,
            true,
            rgb(70, 130, 160),
        )
    });
    b.cylinder(DVec3::Y * 0.62, (0.3, 0.25), 1.2, 12, false, stone);
    b.cylinder(DVec3::Y * 1.8, (0.25, 1.1), 0.4, 16, true, stone);
    glow(&mut b, |b| {
        ellipsoid(
            b,
            DVec3::Y * 2.55,
            DVec3::splat(0.25),
            12,
            6,
            rgb(140, 230, 255),
        );
        for k in 0..3 {
            let a = TAU * k as f64 / 3.0;
            let r = DVec3::new(0.55 * a.cos(), 0.12 * (a * 2.0).sin(), 0.55 * a.sin());
            ellipsoid(
                b,
                DVec3::Y * 2.55 + r,
                DVec3::splat(0.07),
                8,
                4,
                rgb(255, 160, 220),
            );
        }
    });
    b.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_made_model_has_faces() {
        use hw_gen::urban::props::{CarModel::*, LampStyle::*};
        let kinds = [
            PropKind::Lamp(Lantern),
            PropKind::Lamp(Deco),
            PropKind::Lamp(Atomic),
            PropKind::Car(Cruiser, 0),
            PropKind::Car(Bubble, 3),
            PropKind::Car(Hover, 7),
            PropKind::TrafficLight,
            PropKind::Bench,
            PropKind::Bin,
            PropKind::Hydrant,
            PropKind::Mailbox,
            PropKind::Fountain,
            PropKind::Bollard,
            PropKind::Kiosk,
            PropKind::AdPillar,
            PropKind::Statue(0),
            PropKind::Statue(1),
            PropKind::Statue(2),
        ];
        for k in kinds {
            let levels = model_levels(k).unwrap();
            assert!(levels.iter().all(|l| l.mesh.face_count() > 0), "{k:?}");
        }
    }
}
