//! The town's landmarks in the game: the Aether Spire on its plaza and the
//! lighthouse on the shore, both in the 1939 World's Fair's taste, and the
//! mad scientist's Tesla tower, which is not.

use crate::carmesh::{lathe, tube};
use crate::citymesh::{MeshBuilder, Surf};
use crate::skins::Tex;
use engine::prelude::*;
use engine::texgen::{FacadeLayer, GroundLayer};
use glam::{DVec2, DVec3};
use hw_gen::urban::landmarks::{
    tesla_rods, Landmark, LandmarkKind, TESLA_LAB, TESLA_LAB_ROOF, TESLA_ROD_TIP,
};
use hw_gen::urban::world::TownWorld;
use std::f64::consts::TAU;

/// Aether light, the same cyan as the taxi pads' rims.
const AETHER: (u8, u8, u8) = (120, 230, 255);
/// The mad scientist's arcs: a sick green.
pub const ARC: (u8, u8, u8) = (150, 255, 110);

pub struct LandmarkView {
    pub sets: Vec<LodId>,
}

impl LandmarkView {
    pub fn spawn(world: &mut World, town: &TownWorld) -> EngineResult<Self> {
        let mut sets = Vec::new();
        for (lod, place) in Self::models(town) {
            sets.push(world.spawn_lod_at(lod, &[place])?);
        }
        Ok(Self { sets })
    }

    /// Every landmark's LOD sets and where each stands. Pure data, no
    /// `World`: the start makes them on a worker thread.
    pub fn models(town: &TownWorld) -> Vec<(Vec<LodLevel>, Place)> {
        let mut out = Vec::new();
        for l in &town.landmarks {
            let place = Place::new(l.pos.x as f32, l.base as f32, l.pos.y as f32)
                .with_yaw_deg(l.dir.x.atan2(l.dir.y).to_degrees() as f32);
            // Near: every part with its textures. Far: the silhouette in one
            // untextured mesh, seen across the whole town.
            let mut far = Some(model(l, false).into_iter().next().expect("a mesh"));
            for mesh in model(l, true) {
                let mut lod = vec![LodLevel::new(mesh, 700.0)];
                if let Some(f) = far.take() {
                    lod.push(LodLevel::new(f, 6000.0));
                }
                out.push((lod, place));
            }
        }
        out
    }
}

/// One mesh per texture (plain colours first), in the landmark's frame:
/// origin on the plinth's top, +z the way it faces.
pub fn model(l: &Landmark, near: bool) -> Vec<Mesh> {
    let mut b = MeshBuilder::new();
    plinth(&mut b, l, near);
    match l.kind {
        LandmarkKind::AetherSpire => {
            spire(&mut b, l.height, near);
            // The town's founding hero greets you on the way to the spire.
            if near {
                let mut s = MeshBuilder::new();
                crate::propview::statue(&mut s, 0);
                b.append(s, glam::Vec3::new(0.0, 0.0, l.statue_z() as f32));
            }
        }
        LandmarkKind::Lighthouse => lighthouse(&mut b, l.height, near),
        LandmarkKind::TeslaTower => tesla(&mut b, l, near),
        LandmarkKind::HeroHall => hero_hall(&mut b, near),
        LandmarkKind::MooringMast => crate::mastmesh::mast(&mut b, l, near),
        LandmarkKind::RocketGantry => crate::rocketmesh::gantry(&mut b, l, near),
        LandmarkKind::AtomicPlant => crate::atomview::plant(&mut b, near),
        LandmarkKind::MoleDrill => crate::drillview::model(&mut b, l, near),
        LandmarkKind::WeatherTower => crate::weathermodel::tower(&mut b, l, near),
        LandmarkKind::RobotWorks => crate::worksview::model_into(&mut b, near),
        LandmarkKind::Hospital => crate::hospitalmesh::hospital(&mut b, near),
    }
    stairs(&mut b, l, near);
    b.finish_parts()
}

/// The flights of steps up the plinth and the spire's base: one stone
/// block per tread, from the same numbers people climb them by.
fn stairs(b: &mut MeshBuilder, l: &Landmark, near: bool) {
    let right = DVec2::new(l.dir.y, -l.dir.x);
    let local = |v: DVec2| (v.dot(right), v.dot(l.dir));
    let stone = rgb(204, 196, 180);
    b.tex = near.then_some(Tex::from(FacadeLayer::Limestone));
    for st in &l.stairs {
        let rise = (st.from - st.to) / st.steps as f64;
        let across = DVec2::new(st.down.y, -st.down.x);
        let (ux, uz) = local(across);
        for k in 1..st.steps {
            // This tread alone (no overlap with the next), down to the foot.
            let mid = st.top + st.down * ((k as f64 - 0.5) * st.run);
            let (x, z) = local(mid - l.pos);
            let (y0, y1) = (st.to - 0.6 - l.base, st.from - k as f64 * rise - l.base);
            b.oriented_box(
                DVec3::new(x, (y0 + y1) * 0.5, z),
                DVec3::new(ux, 0.0, uz),
                DVec3::new(st.width * 0.5, (y1 - y0) * 0.5, st.run * 0.5),
                stone,
            );
        }
    }
    b.tex = None;
}

fn c3(t: (u8, u8, u8)) -> Color {
    rgb(t.0, t.1, t.2)
}

fn with(b: &mut MeshBuilder, s: Surf, f: impl FnOnce(&mut MeshBuilder)) {
    b.surf = s;
    f(b);
    b.surf = Surf::Plain;
}

/// The paved round plinth, its side a stone step down to the lowest ground.
fn plinth(b: &mut MeshBuilder, l: &Landmark, near: bool) {
    let seg = if near { 48 } else { 16 };
    let drop = l.base - l.low + 0.4;
    let stone = rgb(196, 188, 172);
    let tex = |layer| near.then_some(layer);
    b.tex = tex(Tex::from(FacadeLayer::Limestone));
    b.cylinder(
        DVec3::Y * -drop,
        (l.plaza, l.plaza),
        drop - 0.02,
        seg,
        false,
        stone,
    );
    b.tex = tex(Tex::from(GroundLayer::Paving));
    b.cylinder(
        DVec3::Y * -0.02,
        (l.plaza, l.plaza),
        0.02,
        seg,
        true,
        rgb(214, 206, 192),
    );
    b.tex = None;
    if !near {
        return;
    }
    // A kerb ring of darker stone.
    b.cylinder(
        DVec3::Y * 0.0,
        (l.plaza, l.plaza - 0.05),
        0.12,
        seg,
        false,
        rgb(150, 144, 134),
    );
    if l.kind == LandmarkKind::AetherSpire {
        // Aether conduits: glowing lines in the paving, out from the spire.
        with(b, Surf::Glow, |b| {
            for k in 0..8 {
                let a = TAU * (k as f64 + 0.5) / 8.0;
                let d = DVec3::new(a.sin(), 0.0, a.cos());
                let side = DVec3::new(d.z, 0.0, -d.x) * 0.12;
                let (r0, r1) = (6.0, l.plaza - 0.6);
                let y = 0.015;
                b.quad(
                    [
                        d * r0 - side + DVec3::Y * y,
                        d * r1 - side + DVec3::Y * y,
                        d * r1 + side + DVec3::Y * y,
                        d * r0 + side + DVec3::Y * y,
                    ],
                    DVec3::Y,
                    c3(AETHER),
                );
            }
        });
    }
}

/// The Aether Spire: a stepped base, four fins, a tapering white shaft ringed
/// with Aether coils, an observation saucer and a glowing crown.
fn spire(b: &mut MeshBuilder, h: f64, near: bool) {
    let seg = if near { 24 } else { 8 };
    let white = rgb(236, 232, 220);
    let cream = rgb(222, 210, 180);
    let gold = rgb(214, 170, 72);
    let chrome = rgb(200, 206, 214);
    // Stepped octagonal base.
    let mut y = 0.0;
    for (r, t) in [(5.6, 0.9), (4.7, 0.9), (3.9, 1.2)] {
        b.cylinder(DVec3::Y * y, (r, r), t, 8, true, cream);
        y += t;
    }
    // Doors on four sides: dark glass with a gold frame.
    if near {
        for k in 0..4 {
            let a = TAU * k as f64 / 4.0;
            let d = DVec3::new(a.sin(), 0.0, a.cos());
            let side = DVec3::new(d.z, 0.0, -d.x);
            let at = d * 3.25 + DVec3::Y * y;
            b.oriented_box(at + DVec3::Y * 1.5, side, DVec3::new(0.9, 1.5, 0.12), gold);
            with(b, Surf::Glass, |b| {
                b.oriented_box(
                    at + DVec3::Y * 1.4 + d * 0.05,
                    side,
                    DVec3::new(0.7, 1.3, 0.12),
                    rgb(40, 60, 80),
                )
            });
        }
    }
    // The shaft.
    let shaft0 = y;
    let shaft_h = h * 0.82 - y;
    let r_of = |t: f64| 3.0 + (0.7 - 3.0) * t;
    b.cylinder(DVec3::Y * shaft0, (3.0, 0.7), shaft_h, seg, false, white);
    // Four fins: tall tapering buttresses, chrome edged.
    for k in 0..4 {
        let a = TAU * (k as f64 + 0.5) / 4.0;
        let d = DVec3::new(a.sin(), 0.0, a.cos());
        let top = shaft0 + shaft_h * 0.55;
        let r_top = r_of(0.55);
        let pts = [
            d * 2.6 + DVec3::Y * shaft0,
            d * 7.5 + DVec3::Y * shaft0,
            d * (r_top + 0.6) + DVec3::Y * top,
            d * (r_top - 0.2) + DVec3::Y * (top + 2.0),
        ];
        let side = DVec3::new(d.z, 0.0, -d.x) * 0.35;
        let q = |p: DVec3, s: f64| p + side * s;
        b.hexa(
            [
                q(pts[0], -1.0),
                q(pts[1], -1.0),
                q(pts[1], 1.0),
                q(pts[0], 1.0),
            ],
            [
                q(pts[3], -1.0),
                q(pts[2], -1.0),
                q(pts[2], 1.0),
                q(pts[3], 1.0),
            ],
            white,
        );
        if near {
            // A red racing stripe down the fin's outer edge.
            let e0 = pts[1] + DVec3::Y * 0.02;
            let e1 = pts[2];
            let out = (e1 - e0).cross(side).normalize();
            let out = if out.dot(d) < 0.0 { -out } else { out };
            let o = out * 0.03;
            b.quad(
                [
                    q(e0, -0.4) + o,
                    q(e0, 0.4) + o,
                    q(e1, 0.4) + o,
                    q(e1, -0.4) + o,
                ],
                out,
                rgb(196, 52, 44),
            );
        }
    }
    // Aether coils round the shaft.
    let coils = if near { 6 } else { 3 };
    with(b, Surf::Glow, |b| {
        for k in 1..=coils {
            let t = 0.12 + 0.6 * k as f64 / coils as f64;
            let r = r_of(t) + 0.25;
            b.cylinder(
                DVec3::Y * (shaft0 + shaft_h * t),
                (r, r),
                0.35,
                seg,
                false,
                c3(AETHER),
            );
        }
    });
    // The observation saucer.
    let sy = shaft0 + shaft_h * 0.78;
    let sr = 5.5_f64.min(h * 0.08);
    with(b, Surf::Metal, |b| {
        b.cylinder(DVec3::Y * (sy - 0.9), (1.0, sr), 0.9, seg, false, chrome);
        b.cylinder(DVec3::Y * (sy + 0.6), (sr, 1.2), 0.9, seg, false, chrome);
    });
    with(b, Surf::Glow, |b| {
        b.cylinder(DVec3::Y * sy, (sr, sr), 0.6, seg, false, rgb(255, 214, 140))
    });
    // The crown: a needle with a glowing Aether orb and its orbiting sparks.
    let top = shaft0 + shaft_h;
    with(b, Surf::Metal, |b| {
        b.cylinder(DVec3::Y * top, (0.7, 0.12), h - top, seg / 2, false, gold)
    });
    with(b, Surf::Glow, |b| {
        b.dome(DVec3::Y * (top + 1.2), 1.4, seg, 4, c3(AETHER));
        b.dome(DVec3::Y * (top + 1.2), -1.4, seg, 4, c3(AETHER));
        if near {
            for k in 0..3 {
                let a = TAU * k as f64 / 3.0;
                let p = DVec3::new(2.6 * a.cos(), 1.2 + 0.6 * (2.0 * a).sin(), 2.6 * a.sin());
                b.dome(DVec3::Y * top + p, 0.35, 8, 3, rgb(255, 150, 220));
            }
        }
    });
}

/// A striped lighthouse: red and white bands, a chrome gallery, a glass
/// lantern room with its glowing lamp, and a red dome.
fn lighthouse(b: &mut MeshBuilder, h: f64, near: bool) {
    let seg = if near { 24 } else { 10 };
    let white = rgb(240, 238, 230);
    let red = rgb(190, 44, 40);
    let chrome = rgb(196, 202, 210);
    // A round keeper's house at its foot, streamlined.
    b.cylinder(DVec3::ZERO, (4.6, 4.6), 3.4, seg, false, white);
    b.cylinder(DVec3::Y * 3.4, (4.9, 4.9), 0.3, seg, true, red);
    if near {
        // Porthole windows and a band.
        with(b, Surf::Glass, |b| {
            for k in 0..6 {
                let a = TAU * (k as f64 + 0.5) / 6.0;
                let d = DVec3::new(a.sin(), 0.0, a.cos());
                let side = DVec3::new(d.z, 0.0, -d.x);
                b.oriented_box(
                    d * 4.58 + DVec3::Y * 2.0,
                    side,
                    DVec3::new(0.45, 0.45, 0.05),
                    rgb(60, 90, 110),
                );
            }
        });
        b.oriented_box(
            DVec3::new(0.0, 1.2, 4.6),
            DVec3::X,
            DVec3::new(0.7, 1.2, 0.08),
            red,
        );
    }
    // The tower in bands.
    let tower = h - 6.0;
    let bands = 6;
    let r_of = |y: f64| 2.6 + (1.7 - 2.6) * (y / tower);
    for k in 0..bands {
        let y0 = tower * k as f64 / bands as f64;
        let y1 = tower * (k + 1) as f64 / bands as f64;
        b.cylinder(
            DVec3::Y * y0,
            (r_of(y0), r_of(y1)),
            y1 - y0,
            seg,
            false,
            if k % 2 == 0 { white } else { red },
        );
    }
    // Gallery and railing.
    with(b, Surf::Metal, |b| {
        b.cylinder(DVec3::Y * tower, (2.9, 2.9), 0.3, seg, true, chrome);
        if near {
            b.cylinder(
                DVec3::Y * (tower + 1.0),
                (2.85, 2.85),
                0.08,
                seg,
                false,
                chrome,
            );
            for k in 0..12 {
                let a = TAU * k as f64 / 12.0;
                let p = DVec3::new(2.8 * a.cos(), tower + 0.3, 2.8 * a.sin());
                b.cylinder(p, (0.04, 0.04), 0.75, 4, false, chrome);
            }
        }
    });
    // Lantern room: glass round a glowing lamp, under a red dome.
    with(b, Surf::Glow, |b| {
        b.cylinder(
            DVec3::Y * (tower + 0.6),
            (0.9, 0.9),
            1.8,
            seg / 2,
            true,
            rgb(255, 236, 170),
        )
    });
    with(b, Surf::Glass, |b| {
        b.cylinder(
            DVec3::Y * (tower + 0.3),
            (1.6, 1.6),
            2.6,
            seg,
            false,
            rgb(170, 210, 220),
        )
    });
    b.dome(DVec3::Y * (tower + 2.9), 1.75, seg, 5, red);
    with(b, Surf::Metal, |b| {
        b.cylinder(DVec3::Y * (tower + 4.5), (0.1, 0.0), 1.3, 6, false, chrome)
    });
}

/// A square beam from `a` to `c`, `w` thick.
pub fn strut(b: &mut MeshBuilder, a: DVec3, c: DVec3, w: f64, color: Color) {
    let d = (c - a).normalize_or(DVec3::Y);
    let u = if d.y.abs() > 0.9 { DVec3::X } else { DVec3::Y };
    let s = d.cross(u).normalize() * w * 0.5;
    let t = d.cross(s).normalize() * w * 0.5;
    let quad = |p: DVec3| [p - s - t, p + s - t, p + s + t, p - s + t];
    b.hexa(quad(a), quad(c), color);
}

/// The Tesla tower: a blockhouse lab at the foot, a tapering iron lattice,
/// a great dark dome on top ringed with green coils, a fence of wire round
/// the yard, and dead conduits running off into the quarter.
fn tesla(b: &mut MeshBuilder, l: &Landmark, near: bool) {
    let h = l.height;
    let seg = if near { 48 } else { 10 };
    let iron = rgb(58, 62, 58);
    let rust = rgb(96, 66, 48);
    let concrete = rgb(118, 122, 112);
    let copper = rgb(70, 110, 92);
    // The lab: a squat concrete blockhouse round the tower's feet, with slit
    // windows lit green from inside.
    b.oriented_box(
        DVec3::Y * 2.6,
        DVec3::X,
        DVec3::new(TESLA_LAB, 2.6, TESLA_LAB),
        concrete,
    );
    b.oriented_box(DVec3::Y * 5.4, DVec3::X, DVec3::new(6.9, 0.25, 6.9), iron);
    if near {
        with(b, Surf::Glow, |b| {
            for k in 0..4 {
                let a = TAU * k as f64 / 4.0;
                let d = DVec3::new(a.sin(), 0.0, a.cos());
                let side = DVec3::new(d.z, 0.0, -d.x);
                for j in [-1.0, 1.0] {
                    b.oriented_box(
                        d * 6.52 + side * j * 3.0 + DVec3::Y * 3.6,
                        side,
                        DVec3::new(1.3, 0.22, 0.05),
                        c3(ARC),
                    );
                }
            }
        });
        // A riveted iron door facing the way in.
        b.oriented_box(
            DVec3::new(0.0, 1.5, 6.55),
            DVec3::X,
            DVec3::new(1.2, 1.5, 0.1),
            rust,
        );
    }
    // The lattice: four legs leaning in, cross-braced in bays.
    let top = tesla_dims(h).top;
    let r_of = |y: f64| 5.0 + (1.6 - 5.0) * (y / top);
    let leg = |k: usize, y: f64| {
        let a = TAU * (k as f64 + 0.5) / 4.0;
        DVec3::new(a.sin() * r_of(y), y, a.cos() * r_of(y))
    };
    let w = if near { 0.45 } else { 0.8 };
    for k in 0..4 {
        strut(b, leg(k, 0.0), leg(k, top), w, iron);
    }
    let bays = if near { 8 } else { 4 };
    for i in 0..bays {
        let (y0, y1) = (
            top * i as f64 / bays as f64,
            top * (i + 1) as f64 / bays as f64,
        );
        for k in 0..4 {
            let j = (k + 1) % 4;
            strut(b, leg(k, y1), leg(j, y1), w * 0.7, iron);
            if near {
                strut(b, leg(k, y0), leg(j, y1), w * 0.5, rust);
                strut(b, leg(j, y0), leg(k, y1), w * 0.5, rust);
            }
        }
    }
    let d = tesla_dims(h);
    // The coil in the lattice's heart: a copper winding round a glowing
    // core. Near, the core is Tesla view's (it pulses); far, it is here.
    if near {
        let turns = 22.0;
        let pts: Vec<DVec3> = (0..=(turns as usize * 14))
            .map(|i| {
                let t = i as f64 / (turns * 14.0);
                let a = TAU * turns * t;
                DVec3::new(
                    a.sin() * (COIL_R + 0.14),
                    COIL_FOOT + (top - 0.5 - COIL_FOOT) * t,
                    a.cos() * (COIL_R + 0.14),
                )
            })
            .collect();
        tube(
            b,
            &pts,
            (0.07, 0.07),
            6,
            Some((Surf::Metal, rgb(176, 104, 58))),
        );
    } else {
        tesla_core(b, &d, 0.0, 1.0, false);
    }
    // Rings of coil round the top of the lattice.
    with(b, Surf::Metal, |b| {
        for t in [0.55, 0.75, 0.95] {
            let y = top * t;
            let r = r_of(y) + 0.5;
            let (r0, r1) = (r - 0.18, r + 0.18);
            lathe(
                b,
                DVec3::Y * y,
                DVec3::Y,
                &[(r0, 0.0), (r1, 0.0), (r1, 0.4), (r0, 0.4), (r0, 0.0)],
                seg * 2,
                |_| Some((Surf::Metal, copper)),
            );
        }
    });
    // The dome: a great mushroom cap of riveted copper on a flaring iron
    // bell, its rim rolled into a fat lip, smooth all round.
    tesla_dome(b, &d, near, iron, copper);
    // The needle, and its ball (near, the ball is Tesla view's).
    lathe(
        b,
        DVec3::Y * (d.rim_y + d.cap_h - 0.3),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.45, 0.0),
            (0.35, 0.5),
            (0.12, 0.7),
            (0.06, d.ball_y - d.rim_y - d.cap_h - 0.4),
            (0.0, d.ball_y - d.rim_y - d.cap_h - 0.4),
        ],
        if near { 24 } else { 8 },
        |_| Some((Surf::Metal, iron)),
    );
    if !near {
        tesla_rim(b, &d, false);
        tesla_ball(b, &d, false);
        return;
    }
    // Lightning rods on the lab roof's corners, where the big arcs strike:
    // an iron mast in insulator rings, a copper ball on top.
    for at in tesla_rods() {
        let foot = DVec3::new(at.x, TESLA_LAB_ROOF - 0.25, at.y);
        let tip = TESLA_ROD_TIP - TESLA_LAB_ROOF;
        lathe(
            b,
            foot,
            DVec3::Y,
            &[
                (0.0, 0.0),
                (0.3, 0.0),
                (0.3, 0.35),
                (0.12, 0.45),
                (0.09, tip - 0.6),
                (0.0, tip - 0.6),
            ],
            16,
            |_| Some((Surf::Metal, iron)),
        );
        for k in 0..4 {
            let y = 1.0 + k as f64 * 0.32;
            lathe(
                b,
                foot + DVec3::Y * y,
                DVec3::Y,
                &[
                    (0.0, 0.0),
                    (0.1, 0.0),
                    (0.26, 0.08),
                    (0.26, 0.14),
                    (0.1, 0.22),
                    (0.0, 0.22),
                ],
                16,
                |_| Some((Surf::Plain, rgb(214, 206, 188))),
            );
        }
        let ball = |a: f64| -> (f64, f64) { (0.42 * a.sin(), 0.42 * (1.0 - a.cos())) };
        let prof: Vec<(f64, f64)> = (0..=10)
            .map(|i| ball(std::f64::consts::PI * i as f64 / 10.0))
            .collect();
        lathe(
            b,
            foot + DVec3::Y * (tip - 0.7),
            DVec3::Y,
            &prof,
            20,
            |_| Some((Surf::Metal, copper_bright())),
        );
    }
    // A wire fence round the yard: posts, and a mesh of rusty strands.
    let fence = l.plaza - 0.6;
    let posts = 28;
    for k in 0..posts {
        let a = TAU * k as f64 / posts as f64;
        let p = DVec3::new(a.sin() * fence, 0.0, a.cos() * fence);
        // A gap for the gate, towards the way in (+z).
        if a.cos() > 0.97 {
            continue;
        }
        b.cylinder(p, (0.06, 0.06), 2.6, 4, false, iron);
        let a1 = TAU * (k + 1) as f64 / posts as f64;
        if a1.cos() > 0.97 {
            continue;
        }
        let q = DVec3::new(a1.sin() * fence, 0.0, a1.cos() * fence);
        for y in [0.6, 1.3, 2.0, 2.5] {
            strut(b, p + DVec3::Y * y, q + DVec3::Y * y, 0.03, rust);
        }
    }
    // Dead conduits: dark pipes from the lab out across the yard.
    for k in 0..3 {
        let a = TAU * (k as f64 + 0.25) / 3.0;
        let d = DVec3::new(a.sin(), 0.0, a.cos());
        strut(
            b,
            d * 6.6 + DVec3::Y * 0.35,
            d * (l.plaza - 0.3) + DVec3::Y * 0.35,
            0.5,
            rust,
        );
    }
}

/// The Tesla tower's coil core: its radius and where it starts.
const COIL_R: f64 = 0.5;
const COIL_FOOT: f64 = 6.0;

/// The heights and sizes of a Tesla tower's top, from its height.
#[derive(Clone, Copy, Debug)]
pub struct TeslaDims {
    /// Top of the lattice, where the dome's bell starts.
    pub top: f64,
    /// The dome's radius at its rim.
    pub dr: f64,
    /// Height of the rim's lip.
    pub rim_y: f64,
    /// How high the cap rises over the rim.
    pub cap_h: f64,
    /// The ball on the needle.
    pub ball_y: f64,
}

pub fn tesla_dims(h: f64) -> TeslaDims {
    let top = h * 0.78;
    let dr = (h * 0.16).clamp(6.0, 11.0);
    let rim_y = top + dr * 0.35;
    let cap_h = dr * 0.8;
    TeslaDims {
        top,
        dr,
        rim_y,
        cap_h,
        ball_y: rim_y + cap_h + 2.4,
    }
}

/// Bright copper (the rods' balls, the coil's winding).
fn copper_bright() -> Color {
    rgb(184, 115, 60)
}

/// The glowing core of the coil, from `t0` to `t1` of its length. `live`:
/// a hair larger, for Tesla view's pulsing copy over the far model's.
pub fn tesla_core(b: &mut MeshBuilder, d: &TeslaDims, t0: f64, t1: f64, live: bool) {
    let r = COIL_R + if live { 0.03 } else { 0.0 };
    let len = d.top - 0.5 - COIL_FOOT;
    lathe(
        b,
        DVec3::Y * COIL_FOOT,
        DVec3::Y,
        &[(r, len * t0), (r, len * t1)],
        20,
        |_| Some((Surf::Glow, c3(ARC))),
    );
}

/// The glowing ring round the dome's lip.
pub fn tesla_rim(b: &mut MeshBuilder, d: &TeslaDims, live: bool) {
    let r = 0.2 + if live { 0.02 } else { 0.0 };
    let n = 12;
    let prof: Vec<(f64, f64)> = (0..=n)
        .map(|i| {
            let a = TAU * i as f64 / n as f64;
            (d.dr + 0.12 + r * a.cos(), r * a.sin())
        })
        .collect();
    lathe(b, DVec3::Y * d.rim_y, DVec3::Y, &prof, 96, |_| {
        Some((Surf::Glow, c3(ARC)))
    });
}

/// The glowing ball on the needle at the very top.
pub fn tesla_ball(b: &mut MeshBuilder, d: &TeslaDims, live: bool) {
    let r = 1.0 + if live { 0.03 } else { 0.0 };
    let prof: Vec<(f64, f64)> = (0..=12)
        .map(|i| {
            let a = std::f64::consts::PI * i as f64 / 12.0;
            (r * a.sin(), r * (1.0 - a.cos()))
        })
        .collect();
    lathe(b, DVec3::Y * (d.ball_y - r), DVec3::Y, &prof, 32, |_| {
        Some((Surf::Glow, c3(ARC)))
    });
}

/// The dome: a flaring iron bell from the lattice's top out to a rolled
/// copper lip, and over it a riveted copper cap, all lathed smooth; near,
/// with ribs down the cap and gussets under the bell.
fn tesla_dome(b: &mut MeshBuilder, d: &TeslaDims, near: bool, iron: Color, copper: Color) {
    let seg = if near { 96 } else { 24 };
    let fine = if near { 1 } else { 3 };
    let mut prof = vec![(0.0, d.top), (1.9, d.top)];
    // The bell: out and up, flaring ever faster towards the lip.
    let lip_c = (d.dr - 0.45, d.rim_y);
    let lip_r = 0.5;
    let lip = |deg: f64| {
        let a = deg.to_radians();
        (lip_c.0 + lip_r * a.cos(), lip_c.1 + lip_r * a.sin())
    };
    let start = lip(-100.0);
    let n_bell = 16 / fine;
    let bell = |u: f64| {
        (
            1.9 + (start.0 - 1.9) * u,
            d.top + (start.1 - d.top) * u.powf(2.4),
        )
    };
    for i in 1..=n_bell {
        prof.push(bell(i as f64 / n_bell as f64));
    }
    let under = prof.len() - 1;
    // The rolled lip.
    let n_lip = 12 / fine;
    for i in 1..=n_lip {
        prof.push(lip(-100.0 + 200.0 * i as f64 / n_lip as f64));
    }
    let lipped = prof.len() - 1;
    // The cap: half an ellipse from the lip's top to the needle.
    let from = lip(100.0);
    let (rx, ry) = (from.0, d.rim_y + d.cap_h - from.1);
    let cap = |phi: f64| (rx * phi.cos(), from.1 + ry * phi.sin());
    let n_cap = 24 / fine;
    for i in 1..=n_cap {
        let phi = std::f64::consts::FRAC_PI_2 * i as f64 / n_cap as f64;
        let (r, y) = cap(phi);
        prof.push((r.max(0.0), y));
    }
    if let Some(last) = prof.last_mut() {
        last.0 = 0.0;
    }
    lathe(b, DVec3::ZERO, DVec3::Y, &prof, seg, |i| {
        if i < under {
            Some((Surf::Metal, iron))
        } else if i < lipped {
            Some((Surf::Metal, copper_bright().lerp(copper, 0.55)))
        } else {
            Some((Surf::Metal, copper))
        }
    });
    if !near {
        return;
    }
    // Ribs down the cap's seams, standing proud of the plates.
    let ribs = 24;
    for k in 0..ribs {
        let a = TAU * (k as f64 + 0.5) / ribs as f64;
        let (s, c) = a.sin_cos();
        let path: Vec<DVec3> = (0..=14)
            .map(|i| {
                let phi = 1.45 * i as f64 / 14.0;
                let (r, y) = cap(phi);
                let n = DVec2::new(phi.cos() / rx, phi.sin() / ry).normalize() * 0.06;
                DVec3::new(s * (r + n.x), y + n.y, c * (r + n.x))
            })
            .collect();
        tube(b, &path, (0.07, 0.07), 6, Some((Surf::Metal, copper)));
    }
    // A riveted band round the cap low down.
    let (r, y) = cap(0.35);
    let band: Vec<(f64, f64)> = (0..=8)
        .map(|i| {
            let t = std::f64::consts::PI * i as f64 / 8.0;
            (r + 0.02 + 0.08 * t.sin(), y - 0.18 + 0.36 * i as f64 / 8.0)
        })
        .collect();
    lathe(b, DVec3::ZERO, DVec3::Y, &band, 96, |_| {
        Some((Surf::Metal, copper_bright().lerp(copper, 0.4)))
    });
    // Gussets under the bell, from the neck out to the lip.
    for k in 0..12 {
        let a = TAU * k as f64 / 12.0;
        let (s, c) = a.sin_cos();
        let path: Vec<DVec3> = (0..=10)
            .map(|i| {
                let (r, y) = bell(0.04 + 0.9 * i as f64 / 10.0);
                DVec3::new(s * r, y - 0.12, c * r)
            })
            .collect();
        tube(b, &path, (0.09, 0.16), 6, Some((Surf::Metal, iron)));
    }
}

/// The landmark of a kind nearest a point (for the screenshot views).
pub fn nearest(town: &TownWorld, kind: LandmarkKind, from: DVec2) -> Option<&Landmark> {
    town.landmarks
        .iter()
        .filter(|l| l.kind == kind)
        .min_by(|a, b| a.pos.distance(from).total_cmp(&b.pos.distance(from)))
}

/// Hero Hall: a round hall of glass between gold fins under a white
/// flying-saucer roof, two red arches sweeping over it from the plaza, a
/// cantilevered canopy over the doors with the heroes' gold star shield,
/// and a slender pylon on the roof carrying a gold star. Origin on the
/// plinth's top, the doors towards +z.
fn hero_hall(b: &mut MeshBuilder, near: bool) {
    use crate::carmesh::{lathe, tube, Paint};
    use hw_gen::urban::landmarks::{
        HALL_ARCH_FEET, HALL_CANOPY, HALL_R, HALL_ROOF_R, HALL_ROOF_TOP, HALL_WALL,
    };
    let plain = |c: Color| -> Paint { Some((Surf::Plain, c)) };
    let metal = |c: Color| -> Paint { Some((Surf::Metal, c)) };
    let white = rgb(240, 236, 226);
    let cream = rgb(226, 214, 186);
    let gold = rgb(222, 176, 70);
    let red = rgb(196, 40, 40);
    let glass = rgb(38, 74, 98);
    let seg = if near { 64 } else { 20 };
    // A low cream base ring the glass stands on.
    lathe(
        b,
        DVec3::ZERO,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (HALL_R + 0.35, 0.0),
            (HALL_R + 0.35, 0.45),
            (HALL_R + 0.1, 0.55),
            (0.0, 0.55),
        ],
        seg,
        |_| plain(cream),
    );
    // The glass drum, with gold fins between the panes and doors in front.
    let fins = if near { 24 } else { 8 };
    let door_a = 0.0f64; // local +z is angle 0 (sin, cos)
    let ring = |r: f64, y: f64, n: usize| -> Vec<DVec3> {
        (0..n)
            .map(|j| {
                let a = TAU * j as f64 / n as f64;
                DVec3::new(r * a.sin(), y, r * a.cos())
            })
            .collect()
    };
    let rows = vec![ring(HALL_R, 0.55, seg), ring(HALL_R, HALL_WALL, seg)];
    b.grid(&rows, true, true, |_, _| Some((Surf::Glass, glass)));
    // A gold transom ring round the glass at door height.
    lathe(
        b,
        DVec3::Y * 3.3,
        DVec3::Y,
        &[
            (HALL_R + 0.02, 0.0),
            (HALL_R + 0.12, 0.04),
            (HALL_R + 0.12, 0.18),
            (HALL_R + 0.02, 0.22),
        ],
        seg,
        |_| metal(gold),
    );
    for k in 0..fins {
        let a = TAU * (k as f64 + 0.5) / fins as f64;
        // No fin across the doors.
        if (a - door_a).abs() < 0.25 || (TAU - a).abs() < 0.25 {
            continue;
        }
        let out = DVec3::new(a.sin(), 0.0, a.cos());
        let side = DVec3::new(out.z, 0.0, -out.x);
        let depth = if near { 0.55 } else { 0.4 };
        b.surf = Surf::Metal;
        b.oriented_box(
            out * (HALL_R + depth * 0.5) + DVec3::Y * (HALL_WALL * 0.5 + 0.3),
            side,
            DVec3::new(0.09, HALL_WALL * 0.5 - 0.25, depth * 0.5),
            gold,
        );
        b.surf = Surf::Plain;
    }
    // The saucer roof: a shallow white dome over a deep overhang, a gold
    // band round its rim, a pale underside.
    lathe(
        b,
        DVec3::Y * (HALL_WALL - 0.05),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (HALL_R - 0.1, 0.0),
            (HALL_ROOF_R - 1.0, 0.25),
            (HALL_ROOF_R, 0.6),
            (HALL_ROOF_R + 0.05, 0.85),
            (HALL_ROOF_R - 0.4, 1.15),
            (HALL_ROOF_R - 3.0, 1.9),
            (HALL_ROOF_R - 7.0, 2.9),
            (2.2, HALL_ROOF_TOP - HALL_WALL - 0.1),
            (0.0, HALL_ROOF_TOP - HALL_WALL),
        ],
        seg,
        |k| match k {
            0 | 1 => plain(cream),
            3 => metal(gold),
            _ => plain(white),
        },
    );
    // Lights round the underside of the overhang.
    if near {
        with(b, Surf::Glow, |b| {
            for k in 0..32 {
                let a = TAU * k as f64 / 32.0;
                let p = DVec3::new(a.sin(), 0.0, a.cos()) * (HALL_ROOF_R - 1.2)
                    + DVec3::Y * (HALL_WALL + 0.12);
                b.oriented_box(
                    p,
                    DVec3::X,
                    DVec3::new(0.12, 0.04, 0.12),
                    rgb(255, 236, 190),
                );
            }
        });
    }
    // The pylon and its star.
    let top = DVec3::Y * HALL_ROOF_TOP;
    lathe(
        b,
        top - DVec3::Y * 0.2,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.7, 0.0),
            (0.45, 1.0),
            (0.2, 6.0),
            (0.12, 7.2),
            (0.0, 7.3),
        ],
        if near { 20 } else { 8 },
        |_| plain(white),
    );
    for (r, y) in [(0.9, 2.2), (0.7, 3.6), (0.5, 5.0)] {
        lathe(
            b,
            top + DVec3::Y * y,
            DVec3::Y,
            &[(0.0, 0.0), (r, 0.05), (r, 0.15), (0.0, 0.2)],
            if near { 20 } else { 8 },
            |_| metal(gold),
        );
    }
    star_upright(b, top + DVec3::Y * 8.4, 1.3, 0.25, gold);
    // Two arches sweeping over the roof from the plaza, front to back on
    // each side, leaning in over it: the hall's Googie signature.
    let arch_n = if near { 28 } else { 10 };
    for side in [-1.0, 1.0] {
        let feet: Vec<(f64, f64)> = HALL_ARCH_FEET
            .iter()
            .copied()
            .filter(|(x, _)| x.signum() == side)
            .collect();
        let (f0, f1) = (feet[0], feet[1]);
        let path: Vec<DVec3> = (0..=arch_n)
            .map(|i| {
                let u = i as f64 / arch_n as f64;
                let z = f0.1 + (f1.1 - f0.1) * u;
                let lift = (std::f64::consts::PI * u).sin();
                // In over the roof at the top, out at the feet.
                let x = f0.0 * (1.0 - 0.45 * lift);
                DVec3::new(x, lift.powf(0.8) * 15.5 + 0.2, z)
            })
            .collect();
        tube(
            b,
            &path,
            (0.42, 0.42),
            if near { 16 } else { 6 },
            plain(red),
        );
        if near {
            for (x, z) in [f0, f1] {
                lathe(
                    b,
                    DVec3::new(x, 0.0, z),
                    DVec3::Y,
                    &[(0.0, 0.0), (0.75, 0.0), (0.7, 0.35), (0.5, 0.5), (0.0, 0.5)],
                    16,
                    |_| metal(gold),
                );
            }
        }
    }
    // The plinth waiting for the statue of the hero who brings the Doctor
    // in: a white marble drum with a moulded foot and cap, a gold band, and
    // a bronze plaque on its front (see `storyview` for the statue).
    {
        use hw_gen::urban::landmarks::{HALL_PLINTH, HALL_STATUE};
        let (r, h) = HALL_PLINTH;
        let at = DVec3::new(HALL_STATUE.0, 0.0, HALL_STATUE.1);
        let marble = rgb(236, 232, 222);
        lathe(
            b,
            at,
            DVec3::Y,
            &[
                (0.0, 0.0),
                (r + 0.15, 0.0),
                (r + 0.15, 0.18),
                (r + 0.02, 0.3),
                (r, 0.34),
                (r * 0.94, 0.5),
                (r * 0.94, h - 0.32),
                (r, h - 0.26),
                (r, h - 0.2),
                (r + 0.1, h - 0.12),
                (r + 0.1, h),
                (0.0, h),
            ],
            if near { 40 } else { 14 },
            |k| {
                if k == 7 || k == 8 {
                    metal(gold)
                } else {
                    plain(marble)
                }
            },
        );
        if near {
            // The plaque: bronze, on the front, raised a little.
            b.oriented_box(
                at + DVec3::new(0.0, h * 0.55, r * 0.94 + 0.03),
                DVec3::X,
                DVec3::new(0.42, 0.26, 0.03),
                rgb(150, 104, 52),
            );
        }
    }
    // The canopy over the doors on two slim columns, the heroes' shield on
    // its front edge.
    let (hw, reach, h) = HALL_CANOPY;
    let front = HALL_R + reach;
    let slab: Vec<Vec<DVec3>> = [h, h + 0.35]
        .iter()
        .map(|&y| {
            let n = 12;
            (0..=n)
                .map(|j| {
                    let t = j as f64 / n as f64 * 2.0 - 1.0;
                    // A boomerang edge: swept back at the ends.
                    let z = front - 0.9 * t * t;
                    DVec3::new(t * hw, y, z)
                })
                .chain(std::iter::once(DVec3::new(hw, y, HALL_R - 0.4)))
                .chain(std::iter::once(DVec3::new(-hw, y, HALL_R - 0.4)))
                .collect()
        })
        .collect();
    b.grid(&slab, true, true, |_, _| Some((Surf::Plain, white)));
    let n = slab[0].len();
    b.polygon(&slab[1], DVec3::Y, white);
    b.polygon(&slab[0][..n], -DVec3::Y, cream);
    for x in [-hw + 0.4, hw - 0.4] {
        b.cylinder(
            DVec3::new(x, 0.0, front - 0.5 - 0.9 * (x / hw).powi(2)),
            (0.16, 0.16),
            h,
            if near { 12 } else { 6 },
            false,
            gold,
        );
    }
    if near {
        // Glass doors with gold frames.
        for x in [-1.1, 1.1] {
            b.oriented_box(
                DVec3::new(x, 1.75, HALL_R + 0.05),
                DVec3::X,
                DVec3::new(1.0, 1.25, 0.06),
                gold,
            );
            with(b, Surf::Glass, |b| {
                b.oriented_box(
                    DVec3::new(x, 1.7, HALL_R + 0.1),
                    DVec3::X,
                    DVec3::new(0.85, 1.1, 0.06),
                    rgb(50, 70, 90),
                )
            });
        }
        // The shield: a gold disc with a red ring and a white star.
        let at = DVec3::new(0.0, h + 1.15, front - 0.2);
        lathe(
            b,
            at,
            DVec3::Z,
            &[(0.0, 0.0), (1.0, 0.0), (1.0, 0.12), (0.0, 0.16)],
            32,
            |_| metal(gold),
        );
        lathe(
            b,
            at + DVec3::Z * 0.14,
            DVec3::Z,
            &[(0.8, 0.0), (0.9, 0.0), (0.9, 0.04), (0.8, 0.04)],
            32,
            |_| plain(red),
        );
        let mut s = MeshBuilder::new();
        star_upright(&mut s, DVec3::ZERO, 0.62, 0.06, white);
        b.append(s, glam::Vec3::new(0.0, (h + 1.15) as f32, front as f32));
        b.oriented_box(
            DVec3::new(0.0, h + 0.42, front - 0.6),
            DVec3::X,
            DVec3::new(0.12, 0.08, 0.08),
            gold,
        );
    }
}

/// A five-pointed star standing up (in the x-y plane, facing +z) at `c`.
fn star_upright(b: &mut MeshBuilder, c: DVec3, r: f64, t: f64, color: Color) {
    let pts: Vec<DVec3> = (0..10)
        .map(|k| {
            let a = TAU * k as f64 / 10.0;
            let rr = if k % 2 == 0 { r } else { r * 0.42 };
            c + DVec3::new(rr * a.sin(), rr * a.cos(), 0.0)
        })
        .collect();
    for side in [1.0, -1.0] {
        let tip = c + DVec3::Z * (t * side);
        for k in 0..10 {
            let j = (k + 1) % 10;
            b.tri([tip, pts[k], pts[j]], DVec3::Z * side, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn landmark_models_have_faces() {
        for kind in [
            LandmarkKind::AetherSpire,
            LandmarkKind::Lighthouse,
            LandmarkKind::TeslaTower,
            LandmarkKind::HeroHall,
            LandmarkKind::MooringMast,
            LandmarkKind::RocketGantry,
            LandmarkKind::AtomicPlant,
            LandmarkKind::MoleDrill,
            LandmarkKind::WeatherTower,
            LandmarkKind::RobotWorks,
        ] {
            let l = Landmark {
                kind,
                pos: DVec2::ZERO,
                plaza: 14.0,
                base: 3.0,
                low: 1.0,
                height: 60.0,
                dir: DVec2::Y,
                stairs: Vec::new(),
            };
            for near in [true, false] {
                let parts = model(&l, near);
                assert!(parts.iter().map(|m| m.face_count()).sum::<usize>() > 100);
            }
        }
    }
}
