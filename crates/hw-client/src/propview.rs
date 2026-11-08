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
use hw_gen::urban::props::{CarModel, LampStyle, Prop, PropKind, Species};
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
    let two = |near: Mesh, near_m: f32, far: Mesh, far_m: f32| {
        vec![LodLevel::new(near, near_m), LodLevel::new(far, far_m)]
    };
    Ok(match kind {
        PropKind::Tree(s, v) => tree_model(species(s), v as u32)?.lod_levels(),
        PropKind::Shrub(v) => shrub_model(v as u32)?.lod_levels(),
        PropKind::Lamp(style) => two(lamp(style, true), 140.0, lamp(style, false), 700.0),
        PropKind::Car(m, paint) => two(car(m, paint, true), 130.0, car(m, paint, false), 650.0),
        PropKind::TrafficLight => two(traffic_light(true), 160.0, traffic_light(false), 500.0),
        PropKind::Fountain => vec![LodLevel::new(fountain(), 900.0)],
        PropKind::Bench => vec![LodLevel::new(bench(), 160.0)],
        PropKind::Bin => vec![LodLevel::new(bin(), 120.0)],
        PropKind::Hydrant => vec![LodLevel::new(hydrant(), 120.0)],
        PropKind::Mailbox => vec![LodLevel::new(mailbox(), 180.0)],
        PropKind::Bollard => vec![LodLevel::new(bollard(), 150.0)],
        PropKind::Kiosk => two(kiosk(true), 200.0, kiosk(false), 600.0),
        PropKind::AdPillar => vec![LodLevel::new(ad_pillar(), 250.0)],
        PropKind::Statue(pose) => vec![LodLevel::new(statue_mesh(pose), 500.0)],
    })
}

// ---------------------------------------------------------------- shapes

fn bx(b: &mut MeshBuilder, lo: DVec3, hi: DVec3, c: Color) {
    let (lo, hi) = (lo.min(hi), lo.max(hi));
    b.oriented_box((lo + hi) * 0.5, DVec3::X, (hi - lo) * 0.5, c);
}

/// A cylinder lying along x, centred on `c`, `w` wide.
fn xcyl(b: &mut MeshBuilder, c: DVec3, r: f64, w: f64, seg: usize, col: Color) {
    let ring = |x: f64| -> Vec<DVec3> {
        (0..seg)
            .map(|i| {
                let a = TAU * i as f64 / seg as f64;
                c + DVec3::new(x, r * a.cos(), r * a.sin())
            })
            .collect()
    };
    let (l, rr) = (ring(-w * 0.5), ring(w * 0.5));
    for i in 0..seg {
        let j = (i + 1) % seg;
        let mid = (l[i] + l[j]) * 0.5;
        let out = DVec3::new(0.0, mid.y - c.y, mid.z - c.z);
        b.quad([l[i], l[j], rr[j], rr[i]], out, col);
    }
    b.polygon(&l, -DVec3::X, col);
    b.polygon(&rr, DVec3::X, col);
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

/// A box tapering from `lo` (x half, z range) at `y0` to `hi` at `y1`.
fn taper(
    b: &mut MeshBuilder,
    (y0, w0, z00, z01): (f64, f64, f64, f64),
    (y1, w1, z10, z11): (f64, f64, f64, f64),
    c: Color,
) {
    let q = |y: f64, w: f64, za: f64, zb: f64| {
        [
            DVec3::new(-w, y, za),
            DVec3::new(w, y, za),
            DVec3::new(w, y, zb),
            DVec3::new(-w, y, zb),
        ]
    };
    b.hexa(q(y0, w0, z00, z01), q(y1, w1, z10, z11), c);
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

const WARM: (u8, u8, u8) = (255, 218, 160);

fn c3(t: (u8, u8, u8)) -> Color {
    rgb(t.0, t.1, t.2)
}

// ---------------------------------------------------------------- lamps

/// A street lamp, its light towards +z (the road).
fn lamp(style: LampStyle, near: bool) -> Mesh {
    let mut b = MeshBuilder::new();
    let seg = if near { 10 } else { 4 };
    match style {
        LampStyle::Lantern => {
            let iron = rgb(36, 42, 38);
            metal(&mut b, |b| {
                b.cylinder(DVec3::ZERO, (0.2, 0.13), 0.7, seg, true, iron);
                b.cylinder(DVec3::Y * 0.7, (0.07, 0.06), 3.1, seg, false, iron);
                b.cylinder(DVec3::Y * 3.75, (0.13, 0.13), 0.1, seg, true, iron);
                b.cylinder(DVec3::Y * 4.45, (0.32, 0.0), 0.35, 4, true, iron);
            });
            glow(&mut b, |b| {
                b.cylinder(DVec3::Y * 3.85, (0.17, 0.24), 0.6, 4, true, c3(WARM))
            });
            if near {
                metal(&mut b, |b| {
                    b.cylinder(DVec3::Y * 4.8, (0.04, 0.0), 0.25, 6, false, iron)
                });
            }
        }
        LampStyle::Deco => {
            let bronze = rgb(58, 76, 64);
            metal(&mut b, |b| {
                b.cylinder(DVec3::ZERO, (0.32, 0.2), 1.0, seg, true, bronze);
                b.cylinder(DVec3::Y, (0.12, 0.08), 4.9, seg, false, bronze);
                bx(
                    b,
                    DVec3::new(-0.95, 5.8, -0.06),
                    DVec3::new(0.95, 5.92, 0.06),
                    bronze,
                );
                b.cylinder(DVec3::Y * 5.9, (0.08, 0.0), 0.7, seg, false, bronze);
            });
            glow(&mut b, |b| {
                for x in [-0.95, 0.95] {
                    ellipsoid(
                        b,
                        DVec3::new(x, 6.05, 0.0),
                        DVec3::splat(0.27),
                        seg,
                        seg / 2 + 1,
                        c3(WARM),
                    );
                }
            });
        }
        LampStyle::Atomic => {
            let chrome = rgb(214, 220, 224);
            let accent = rgb(64, 186, 180);
            metal(&mut b, |b| {
                b.cylinder(DVec3::ZERO, (0.12, 0.06), 7.0, seg, true, chrome);
                bx(
                    b,
                    DVec3::new(-0.04, 6.9, 0.0),
                    DVec3::new(0.04, 7.0, 1.6),
                    chrome,
                );
            });
            let hub = DVec3::new(0.0, 6.88, 1.75);
            b.cylinder(hub, (0.8, 0.8), 0.14, seg + 4, true, accent);
            glow(&mut b, |b| {
                b.cylinder(
                    hub - DVec3::Y * 0.03,
                    (0.62, 0.62),
                    0.03,
                    seg + 4,
                    true,
                    rgb(225, 245, 255),
                );
                ellipsoid(
                    b,
                    hub + DVec3::Y * 0.26,
                    DVec3::splat(0.13),
                    seg,
                    5,
                    rgb(255, 120, 90),
                );
            });
            if near {
                metal(&mut b, |b| {
                    b.cylinder(hub + DVec3::Y * 0.14, (0.05, 0.0), 0.5, 6, false, chrome);
                    // A ring round the pole, as on a ray gun.
                    b.cylinder(DVec3::Y * 2.6, (0.16, 0.16), 0.08, seg, true, accent);
                });
            }
        }
    }
    b.finish()
}

// ---------------------------------------------------------------- cars

/// Two-tone paints: body, then roof and trim.
const PAINTS: [[(u8, u8, u8); 2]; 8] = [
    [(196, 36, 48), (244, 238, 228)],
    [(56, 186, 186), (244, 238, 228)],
    [(240, 150, 172), (250, 244, 238)],
    [(150, 218, 178), (240, 232, 212)],
    [(108, 166, 228), (32, 52, 102)],
    [(244, 206, 58), (40, 40, 44)],
    [(238, 226, 198), (120, 30, 42)],
    [(176, 148, 218), (204, 208, 218)],
];

/// A golden-age car, its nose to +z.
fn car(model: CarModel, paint: u8, near: bool) -> Mesh {
    car_parts(model, paint, near, true)
}

/// Where a model's wheels sit: half the track, the axles along z, the
/// radius. Hover cars have none.
pub fn wheel_layout(model: CarModel) -> Option<(f64, [f64; 2], f64)> {
    match model {
        CarModel::Cruiser => Some((0.95, [-1.6, 1.65], 0.36)),
        CarModel::Bubble => Some((0.8, [-1.05, 1.1], 0.3)),
        CarModel::Hover => None,
    }
}

/// A car driving in traffic: its body (the wheels turn on their own, see
/// [`wheel_levels`]), close up and far.
pub fn moving_car_levels(model: CarModel, paint: u8) -> Vec<LodLevel> {
    vec![
        LodLevel::new(car_parts(model, paint, true, false), 140.0),
        LodLevel::new(car_parts(model, paint, false, false), 700.0),
    ]
}

/// The ambulance that waits for a rescue's hurt: a long 1950s coach-built
/// ambulance, white with a red stripe and crosses, a tall rear compartment
/// with frosted windows and a rounded roof, tail fins, a chrome grille and
/// twin red bubble lights. Nose to +z; close up and far.
pub fn ambulance_levels() -> Vec<LodLevel> {
    let make = |near: bool| {
        let mut b = MeshBuilder::new();
        let seg = if near { 14 } else { 6 };
        let white = rgb(242, 240, 232);
        let red = rgb(196, 30, 40);
        let chrome = rgb(225, 228, 232);
        let window = rgb(40, 62, 78);
        let frosted = rgb(196, 210, 214);
        // The long lower body, its nose rounded off.
        taper(
            &mut b,
            (0.32, 1.0, -3.05, 2.95),
            (0.95, 1.03, -3.12, 2.85),
            white,
        );
        ellipsoid(
            &mut b,
            DVec3::new(0.0, 0.7, 2.75),
            DVec3::new(1.0, 0.36, 0.5),
            seg,
            seg / 2 + 1,
            white,
        );
        // The rear compartment, its roof rounded, running over the cab.
        taper(
            &mut b,
            (0.95, 1.03, -3.1, 0.75),
            (1.78, 0.98, -3.02, 0.7),
            white,
        );
        ellipsoid(
            &mut b,
            DVec3::new(0.0, 1.78, -1.15),
            DVec3::new(0.98, 0.16, 1.92),
            seg,
            seg / 2 + 1,
            white,
        );
        // The cab: a raked windscreen up to the roofline.
        glass(&mut b, |b| {
            taper(b, (0.95, 0.98, 0.75, 1.95), (1.74, 0.92, 0.7, 1.05), window)
        });
        taper(&mut b, (1.74, 0.92, 0.7, 1.05), (1.8, 0.9, 0.7, 1.0), white);
        for sx in [-1.0, 1.0] {
            let x = sx * 1.035;
            // Frosted windows down the side of the compartment.
            glass(&mut b, |b| {
                for (z0, z1) in [(-2.75, -1.35), (-1.15, 0.45)] {
                    bx(
                        b,
                        DVec3::new(x - 0.01, 1.12, z0),
                        DVec3::new(x + 0.01, 1.58, z1),
                        frosted,
                    );
                }
            });
            // The red stripe and a cross between the windows' frames.
            bx(
                &mut b,
                DVec3::new(x - 0.012, 0.8, -3.05),
                DVec3::new(x + 0.012, 0.9, 2.8),
                red,
            );
            bx(
                &mut b,
                DVec3::new(x - 0.015, 1.27, -1.33),
                DVec3::new(x + 0.015, 1.43, -1.17),
                red,
            );
            // Tail fins with red lamps.
            let q = |y: f64, za: f64, zb: f64, w: f64| {
                [
                    DVec3::new(sx * 0.86, y, za),
                    DVec3::new(sx * (0.86 + w), y, za),
                    DVec3::new(sx * (0.86 + w), y, zb),
                    DVec3::new(sx * 0.86, y, zb),
                ]
            };
            b.hexa(q(0.9, -3.12, -1.9, 0.16), q(1.22, -3.22, -2.8, 0.1), white);
        }
        // A red cross on the rear doors and on the roof.
        bx(
            &mut b,
            DVec3::new(-0.08, 1.05, -3.145),
            DVec3::new(0.08, 1.55, -3.12),
            red,
        );
        bx(
            &mut b,
            DVec3::new(-0.25, 1.22, -3.145),
            DVec3::new(0.25, 1.38, -3.12),
            red,
        );
        bx(
            &mut b,
            DVec3::new(-0.1, 1.94, -1.75),
            DVec3::new(0.1, 1.95, -0.95),
            red,
        );
        bx(
            &mut b,
            DVec3::new(-0.4, 1.94, -1.45),
            DVec3::new(0.4, 1.95, -1.25),
            red,
        );
        if near {
            metal(&mut b, |b| {
                // Bumpers, a wide grille with bars, a siren on the roof.
                for z in [-3.18, 3.18] {
                    bx(
                        b,
                        DVec3::new(-1.04, 0.34, z - 0.07),
                        DVec3::new(1.04, 0.5, z + 0.07),
                        chrome,
                    );
                }
                bx(
                    b,
                    DVec3::new(-0.75, 0.5, 3.12),
                    DVec3::new(0.75, 0.74, 3.16),
                    chrome,
                );
                for k in 0..7 {
                    let x = -0.6 + 0.2 * k as f64;
                    bx(
                        b,
                        DVec3::new(x - 0.02, 0.5, 3.15),
                        DVec3::new(x + 0.02, 0.74, 3.2),
                        rgb(150, 152, 160),
                    );
                }
                for sx in [-1.0, 1.0] {
                    bx(
                        b,
                        DVec3::new(sx * 1.04 - 0.01, 0.58, -2.9),
                        DVec3::new(sx * 1.04 + 0.01, 0.64, 2.7),
                        chrome,
                    );
                }
                xcyl(b, DVec3::new(0.0, 1.98, 0.35), 0.1, 0.36, seg, chrome);
            });
            glow(&mut b, |b| {
                for x in [-0.72, 0.72] {
                    ellipsoid(
                        b,
                        DVec3::new(x, 0.78, 3.1),
                        DVec3::splat(0.12),
                        8,
                        4,
                        c3(WARM),
                    );
                }
                for x in [-0.93, 0.93] {
                    bx(
                        b,
                        DVec3::new(x - 0.05, 1.08, -3.26),
                        DVec3::new(x + 0.05, 1.22, -3.16),
                        rgb(255, 50, 40),
                    );
                }
            });
        }
        // Twin red bubble lights over the windscreen.
        glow(&mut b, |b| {
            for x in [-0.45, 0.45] {
                ellipsoid(
                    b,
                    DVec3::new(x, 1.86, 0.55),
                    DVec3::new(0.13, 0.15, 0.13),
                    10,
                    6,
                    rgb(255, 50, 45),
                );
            }
        });
        wheels(
            &mut b,
            0.98,
            &[-1.95, 1.95],
            0.38,
            seg,
            rgb(24, 24, 26),
            chrome,
            near,
        );
        b.finish()
    };
    vec![
        LodLevel::new(make(true), 140.0),
        LodLevel::new(make(false), 700.0),
    ]
}

/// One wheel of `model`, centred, its axle along x: tyre, whitewalls and a
/// spoked chrome hub on both faces, so it can be seen to turn.
pub fn wheel_levels(model: CarModel) -> Option<Vec<LodLevel>> {
    let (_, _, r) = wheel_layout(model)?;
    let make = |near: bool| {
        let mut b = MeshBuilder::new();
        let seg = if near { 14 } else { 6 };
        let tyre = rgb(24, 24, 26);
        let chrome = rgb(225, 228, 232);
        xcyl(&mut b, DVec3::ZERO, r, 0.24, seg, tyre);
        if near {
            for sx in [-1.0, 1.0] {
                let o = sx * 0.125;
                xcyl(
                    &mut b,
                    DVec3::new(o, 0.0, 0.0),
                    r * 0.72,
                    0.01,
                    seg,
                    rgb(240, 238, 230),
                );
                metal(&mut b, |b| {
                    xcyl(b, DVec3::new(o * 1.1, 0.0, 0.0), r * 0.4, 0.02, seg, chrome);
                    // Three spokes across the hub.
                    for k in 0..3 {
                        let a = std::f64::consts::PI * k as f64 / 3.0;
                        let d = DVec3::new(0.0, a.cos(), a.sin());
                        let e = DVec3::new(0.0, -a.sin(), a.cos());
                        let (l, t) = (r * 0.62, 0.035);
                        let q = |x: f64| {
                            let c = DVec3::new(x, 0.0, 0.0);
                            [
                                c - d * l - e * t,
                                c + d * l - e * t,
                                c + d * l + e * t,
                                c - d * l + e * t,
                            ]
                        };
                        b.hexa(q(o * 1.2 - 0.02), q(o * 1.2 + 0.02), rgb(150, 152, 160));
                    }
                });
            }
        }
        b.finish()
    };
    Some(vec![
        LodLevel::new(make(true), 60.0),
        LodLevel::new(make(false), 260.0),
    ])
}

fn car_parts(model: CarModel, paint: u8, near: bool, with_wheels: bool) -> Mesh {
    let mut b = MeshBuilder::new();
    car_into(&mut b, model, paint, near, with_wheels);
    b.finish()
}

fn car_into(b: &mut MeshBuilder, model: CarModel, paint: u8, near: bool, with_wheels: bool) {
    let [body, top] = PAINTS[paint as usize % PAINTS.len()].map(c3);
    let chrome = rgb(225, 228, 232);
    let tyre = rgb(24, 24, 26);
    let window = rgb(40, 62, 78);
    let seg = if near { 12 } else { 6 };
    match model {
        CarModel::Cruiser => {
            // Long, low, with tail fins and a glassy cabin under a
            // contrasting roof.
            taper(b, (0.3, 0.98, -2.55, 2.6), (0.86, 1.0, -2.62, 2.5), body);
            glass(b, |b| {
                taper(b, (0.86, 0.93, -1.1, 1.0), (1.34, 0.8, -0.78, 0.5), window)
            });
            taper(b, (1.34, 0.8, -0.78, 0.5), (1.42, 0.76, -0.72, 0.44), top);
            for x in [-1.0, 1.0] {
                let q = |y: f64, za: f64, zb: f64, w: f64| {
                    [
                        DVec3::new(x * 0.84, y, za),
                        DVec3::new(x * (0.84 + w), y, za),
                        DVec3::new(x * (0.84 + w), y, zb),
                        DVec3::new(x * 0.84, y, zb),
                    ]
                };
                // Fins rising to the tail.
                b.hexa(q(0.86, -2.62, -1.2, 0.16), q(1.2, -2.7, -2.35, 0.1), top);
            }
            if near {
                metal(b, |b| {
                    for z in [-2.66, 2.64] {
                        bx(
                            b,
                            DVec3::new(-1.0, 0.36, z - 0.06),
                            DVec3::new(1.0, 0.52, z + 0.06),
                            chrome,
                        );
                    }
                    for x in [-1.005, 1.005] {
                        bx(
                            b,
                            DVec3::new(x - 0.01, 0.6, -2.3),
                            DVec3::new(x + 0.01, 0.66, 2.3),
                            chrome,
                        );
                    }
                });
                glow(b, |b| {
                    for x in [-0.7, 0.7] {
                        bx(
                            b,
                            DVec3::new(x - 0.13, 0.62, 2.55),
                            DVec3::new(x + 0.13, 0.76, 2.66),
                            c3(WARM),
                        );
                    }
                    for x in [-0.91, 0.91] {
                        bx(
                            b,
                            DVec3::new(x - 0.05, 0.98, -2.74),
                            DVec3::new(x + 0.05, 1.12, -2.64),
                            rgb(255, 50, 40),
                        );
                    }
                });
            }
            if with_wheels {
                wheels(b, 0.95, &[-1.6, 1.65], 0.36, seg, tyre, chrome, near);
            }
        }
        CarModel::Bubble => {
            taper(b, (0.28, 0.8, -1.55, 1.6), (0.62, 0.86, -1.7, 1.75), body);
            ellipsoid(
                b,
                DVec3::new(0.0, 0.7, 0.05),
                DVec3::new(0.87, 0.36, 1.85),
                seg,
                seg / 2 + 1,
                body,
            );
            glass(b, |b| {
                ellipsoid(
                    b,
                    DVec3::new(0.0, 0.98, -0.15),
                    DVec3::new(0.7, 0.48, 0.95),
                    seg,
                    seg / 2 + 1,
                    window,
                )
            });
            if near {
                metal(b, |b| {
                    ellipsoid(
                        b,
                        DVec3::new(0.0, 0.62, 0.05),
                        DVec3::new(0.9, 0.05, 1.9),
                        seg,
                        4,
                        top,
                    )
                });
                glow(b, |b| {
                    for x in [-0.5, 0.5] {
                        ellipsoid(
                            b,
                            DVec3::new(x, 0.74, 1.72),
                            DVec3::splat(0.11),
                            8,
                            4,
                            c3(WARM),
                        );
                    }
                });
            }
            if with_wheels {
                wheels(b, 0.8, &[-1.05, 1.1], 0.3, seg, tyre, chrome, near);
            }
        }
        CarModel::Hover => {
            // Parked a little off the ground, glowing underneath.
            ellipsoid(
                b,
                DVec3::new(0.0, 1.0, 0.0),
                DVec3::new(1.0, 0.32, 2.3),
                seg,
                seg / 2 + 1,
                body,
            );
            ellipsoid(
                b,
                DVec3::new(0.0, 1.13, -0.55),
                DVec3::new(0.62, 0.2, 1.3),
                seg,
                seg / 2,
                top,
            );
            glass(b, |b| {
                ellipsoid(
                    b,
                    DVec3::new(0.0, 1.22, 0.35),
                    DVec3::new(0.62, 0.36, 0.95),
                    seg,
                    seg / 2 + 1,
                    window,
                )
            });
            metal(b, |b| {
                for x in [-1.05, 1.05] {
                    ellipsoid(
                        b,
                        DVec3::new(x, 0.86, -0.4),
                        DVec3::new(0.22, 0.2, 1.3),
                        seg,
                        seg / 2,
                        chrome,
                    );
                }
            });
            let fin = |y: f64, za: f64, zb: f64| {
                [
                    DVec3::new(-0.04, y, za),
                    DVec3::new(0.04, y, za),
                    DVec3::new(0.04, y, zb),
                    DVec3::new(-0.04, y, zb),
                ]
            };
            b.hexa(fin(1.15, -2.2, -1.3), fin(1.85, -2.5, -2.15), top);
            glow(b, |b| {
                ellipsoid(
                    b,
                    DVec3::new(0.0, 0.68, 0.0),
                    DVec3::new(0.8, 0.04, 1.9),
                    seg,
                    4,
                    rgb(120, 232, 255),
                );
                if near {
                    for x in [-1.05, 1.05] {
                        ellipsoid(
                            b,
                            DVec3::new(x, 0.86, -1.68),
                            DVec3::new(0.14, 0.13, 0.06),
                            8,
                            4,
                            rgb(255, 150, 70),
                        );
                    }
                }
            });
        }
    }
}

/// A flying taxi: the hover car in taxi yellow, a black-and-white check
/// along its flanks and a glowing TAXI lamp on the roof.
fn taxi(near: bool) -> Mesh {
    let mut b = MeshBuilder::new();
    car_into(&mut b, CarModel::Hover, 5, near, false);
    if near {
        // The check: two rows of squares along each flank.
        for x in [-1.0f64, 1.0] {
            for k in 0..14 {
                for row in 0..2 {
                    let z = -1.55 + k as f64 * 0.22;
                    let y = 1.02 + row as f64 * 0.11;
                    // How far out the flank bulges at this point.
                    let t = (z / 2.3).clamp(-1.0, 1.0);
                    let w = (1.0 - t * t).max(0.0).sqrt() * 1.0 + 0.012;
                    let dark = (k + row) % 2 == 0;
                    let c = if dark {
                        rgb(22, 22, 24)
                    } else {
                        rgb(245, 245, 240)
                    };
                    bx(
                        &mut b,
                        DVec3::new(x * w - 0.005, y, z),
                        DVec3::new(x * w + 0.005 * x.signum(), y + 0.1, z + 0.21),
                        c,
                    );
                }
            }
        }
    }
    glow(&mut b, |b| {
        bx(
            b,
            DVec3::new(-0.32, 1.52, -0.05),
            DVec3::new(0.32, 1.72, 0.25),
            rgb(255, 214, 70),
        )
    });
    b.finish()
}

/// Flying taxis, close up and far.
pub fn taxi_levels() -> Vec<LodLevel> {
    vec![
        LodLevel::new(taxi(true), 200.0),
        LodLevel::new(taxi(false), 1200.0),
    ]
}

/// A private hover car flying in a sky lane.
pub fn sky_car_levels(paint: u8) -> Vec<LodLevel> {
    vec![
        LodLevel::new(car_parts(CarModel::Hover, paint, true, false), 200.0),
        LodLevel::new(car_parts(CarModel::Hover, paint, false, false), 1200.0),
    ]
}

#[allow(clippy::too_many_arguments)]
fn wheels(
    b: &mut MeshBuilder,
    x: f64,
    zs: &[f64],
    r: f64,
    seg: usize,
    tyre: Color,
    chrome: Color,
    near: bool,
) {
    for &z in zs {
        for sx in [-x, x] {
            xcyl(b, DVec3::new(sx, r, z), r, 0.24, seg, tyre);
            if near {
                // Whitewalls and a chrome hub.
                let o = sx.signum() * 0.125;
                xcyl(
                    b,
                    DVec3::new(sx + o, r, z),
                    r * 0.72,
                    0.01,
                    seg,
                    rgb(240, 238, 230),
                );
                metal(b, |b| {
                    xcyl(
                        b,
                        DVec3::new(sx + o * 1.1, r, z),
                        r * 0.4,
                        0.02,
                        seg,
                        chrome,
                    )
                });
            }
        }
    }
}

// ---------------------------------------------------------------- street

/// Traffic lights facing +z (the traffic coming in).
fn traffic_light(near: bool) -> Mesh {
    let mut b = MeshBuilder::new();
    let pole = rgb(60, 62, 66);
    let housing = rgb(214, 172, 44);
    metal(&mut b, |b| {
        b.cylinder(DVec3::ZERO, (0.1, 0.08), 3.0, 8, true, pole)
    });
    bx(
        &mut b,
        DVec3::new(-0.2, 2.75, -0.15),
        DVec3::new(0.2, 3.85, 0.15),
        housing,
    );
    b.dome(DVec3::new(0.0, 3.85, 0.0), 0.2, 10, 3, housing);
    if near {
        glow(&mut b, |b| {
            for (y, c) in [
                (3.6, rgb(255, 50, 40)),
                (3.3, rgb(255, 180, 40)),
                (3.0, rgb(60, 255, 120)),
            ] {
                bx(
                    b,
                    DVec3::new(-0.11, y - 0.11, 0.15),
                    DVec3::new(0.11, y + 0.11, 0.18),
                    c,
                );
            }
        });
    }
    b.finish()
}

/// A deco bench, its sitters looking along +z.
fn bench() -> Mesh {
    let mut b = MeshBuilder::new();
    let iron = rgb(40, 46, 42);
    let wood = rgb(150, 98, 56);
    metal(&mut b, |b| {
        for x in [-0.8, 0.8] {
            bx(
                b,
                DVec3::new(x - 0.04, 0.0, -0.25),
                DVec3::new(x + 0.04, 0.45, 0.25),
                iron,
            );
            bx(
                b,
                DVec3::new(x - 0.04, 0.45, -0.27),
                DVec3::new(x + 0.04, 0.9, -0.22),
                iron,
            );
        }
    });
    for k in 0..3 {
        let z = -0.2 + k as f64 * 0.15;
        bx(
            &mut b,
            DVec3::new(-0.95, 0.45, z - 0.06),
            DVec3::new(0.95, 0.5, z + 0.06),
            wood,
        );
    }
    for y in [0.62, 0.8] {
        bx(
            &mut b,
            DVec3::new(-0.95, y - 0.06, -0.24),
            DVec3::new(0.95, y + 0.06, -0.2),
            wood,
        );
    }
    b.finish()
}

fn bin() -> Mesh {
    let mut b = MeshBuilder::new();
    let green = rgb(46, 94, 72);
    metal(&mut b, |b| {
        b.cylinder(DVec3::ZERO, (0.26, 0.29), 0.85, 10, true, green);
        b.cylinder(
            DVec3::Y * 0.85,
            (0.31, 0.31),
            0.06,
            10,
            true,
            rgb(200, 204, 206),
        );
    });
    b.finish()
}

/// A chrome-capped red hydrant.
fn hydrant() -> Mesh {
    let mut b = MeshBuilder::new();
    let red = rgb(198, 38, 34);
    b.cylinder(DVec3::ZERO, (0.18, 0.18), 0.07, 10, true, red);
    b.cylinder(DVec3::Y * 0.07, (0.13, 0.12), 0.5, 10, false, red);
    metal(&mut b, |b| {
        b.dome(DVec3::Y * 0.57, 0.13, 10, 3, rgb(220, 224, 228));
        xcyl(
            b,
            DVec3::new(0.0, 0.4, 0.0),
            0.05,
            0.38,
            8,
            rgb(220, 224, 228),
        );
    });
    b.finish()
}

/// A mailbox shaped like a rocket: fins, a red nose, a glowing porthole.
fn mailbox() -> Mesh {
    let mut b = MeshBuilder::new();
    let blue = rgb(40, 82, 168);
    let red = rgb(204, 46, 40);
    b.cylinder(DVec3::Y * 0.25, (0.22, 0.24), 1.0, 12, true, blue);
    b.cylinder(DVec3::Y * 1.25, (0.24, 0.0), 0.5, 12, false, red);
    for k in 0..3 {
        let a = TAU * (k as f64 / 3.0 + 1.0 / 12.0);
        let u = DVec3::new(a.cos(), 0.0, a.sin());
        b.oriented_box(
            u * 0.3 + DVec3::Y * 0.3,
            u,
            DVec3::new(0.14, 0.3, 0.025),
            red,
        );
    }
    bx(
        &mut b,
        DVec3::new(-0.1, 1.0, 0.2),
        DVec3::new(0.1, 1.04, 0.245),
        rgb(20, 20, 24),
    );
    glow(&mut b, |b| {
        bx(
            b,
            DVec3::new(-0.06, 0.72, 0.2),
            DVec3::new(0.06, 0.84, 0.25),
            rgb(255, 210, 120),
        )
    });
    b.finish()
}

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
fn bollard() -> Mesh {
    let mut b = MeshBuilder::new();
    let iron = rgb(30, 34, 36);
    metal(&mut b, |b| {
        b.cylinder(DVec3::ZERO, (0.2, 0.2), 0.08, 10, true, iron);
        b.cylinder(DVec3::Y * 0.08, (0.15, 0.12), 0.42, 10, false, iron);
        b.cylinder(DVec3::Y * 0.5, (0.12, 0.2), 0.1, 10, false, iron);
        b.cylinder(DVec3::Y * 0.6, (0.2, 0.18), 0.06, 10, true, iron);
    });
    b.finish()
}

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
