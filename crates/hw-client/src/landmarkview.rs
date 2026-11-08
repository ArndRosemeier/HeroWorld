//! The town's landmarks in the game: the Aether Spire on its plaza and the
//! lighthouse on the shore, both in the 1939 World's Fair's taste, and the
//! mad scientist's Tesla tower, which is not.

use crate::citymesh::{MeshBuilder, Surf};
use engine::prelude::*;
use engine::texgen::{FacadeLayer, GroundLayer, Layer};
use glam::{DVec2, DVec3};
use hw_gen::urban::landmarks::{Landmark, LandmarkKind};
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
                sets.push(world.spawn_lod_at(lod, &[place])?);
            }
        }
        Ok(Self { sets })
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
                b.append(s, glam::Vec3::new(0.0, 0.0, (l.plaza * 0.62) as f32));
            }
        }
        LandmarkKind::Lighthouse => lighthouse(&mut b, l.height, near),
        LandmarkKind::TeslaTower => tesla(&mut b, l, near),
    }
    b.finish_parts()
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
    b.tex = tex(Layer::from(FacadeLayer::Limestone));
    b.cylinder(
        DVec3::Y * -drop,
        (l.plaza, l.plaza),
        drop - 0.02,
        seg,
        false,
        stone,
    );
    b.tex = tex(Layer::from(GroundLayer::Paving));
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
fn strut(b: &mut MeshBuilder, a: DVec3, c: DVec3, w: f64, color: Color) {
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
    let seg = if near { 24 } else { 10 };
    let iron = rgb(58, 62, 58);
    let rust = rgb(96, 66, 48);
    let concrete = rgb(118, 122, 112);
    let copper = rgb(70, 110, 92);
    // The lab: a squat concrete blockhouse round the tower's feet, with slit
    // windows lit green from inside.
    b.oriented_box(
        DVec3::Y * 2.6,
        DVec3::X,
        DVec3::new(6.5, 2.6, 6.5),
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
    let top = h * 0.78;
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
    // A green-glowing coil in the lattice's heart.
    with(b, Surf::Glow, |b| {
        b.cylinder(
            DVec3::Y * 6.0,
            (0.6, 0.6),
            top - 6.0,
            seg / 2,
            false,
            c3(ARC),
        )
    });
    // Rings of coil round the top of the lattice.
    with(b, Surf::Metal, |b| {
        for t in [0.55, 0.75, 0.95] {
            let y = top * t;
            let r = r_of(y) + 0.5;
            b.cylinder(DVec3::Y * y, (r, r), 0.4, seg, false, copper);
        }
    });
    // The dome: a great dark mushroom cap with a glowing rim.
    let dr = (h * 0.16).clamp(6.0, 11.0);
    with(b, Surf::Metal, |b| {
        b.cylinder(DVec3::Y * top, (1.8, dr), dr * 0.35, seg, false, iron);
        b.dome(DVec3::Y * (top + dr * 0.35), dr, seg, 6, copper);
    });
    with(b, Surf::Glow, |b| {
        b.cylinder(
            DVec3::Y * (top + dr * 0.35 - 0.2),
            (dr + 0.15, dr + 0.15),
            0.4,
            seg,
            false,
            c3(ARC),
        );
        // The ball on the needle at the very top.
        b.dome(DVec3::Y * (top + dr * 1.35 + 1.5), 1.0, seg / 2, 3, c3(ARC));
        b.dome(
            DVec3::Y * (top + dr * 1.35 + 1.5),
            -1.0,
            seg / 2,
            3,
            c3(ARC),
        );
    });
    with(b, Surf::Metal, |b| {
        b.cylinder(
            DVec3::Y * (top + dr * 1.3),
            (0.2, 0.05),
            1.6,
            6,
            false,
            iron,
        )
    });
    if !near {
        return;
    }
    // Frozen arcs: jagged green bolts from the dome's rim down to the yard.
    with(b, Surf::Glow, |b| {
        for k in 0..5 {
            let a = TAU * (k as f64 * 0.37 + 0.1);
            let d = DVec3::new(a.sin(), 0.0, a.cos());
            let side = DVec3::new(d.z, 0.0, -d.x);
            let start = d * dr + DVec3::Y * (top + dr * 0.3);
            let mut p = start;
            let end = d * (l.plaza - 1.0) + DVec3::Y * 0.2;
            let n = 9;
            for i in 1..=n {
                let t = i as f64 / n as f64;
                // A zigzag, each kink a different size.
                let (jag, out) = if i == n {
                    (0.0, 0.0)
                } else {
                    let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
                    (
                        sign * (1.0 + ((k * 7 + i * 3) % 4) as f64 * 0.6),
                        (((k * 5 + i * 2) % 3) as f64 - 1.0) * 1.2,
                    )
                };
                let q = start.lerp(end, t) + side * jag + d * out;
                strut(b, p, q, 0.12, c3(ARC));
                p = q;
            }
        }
    });
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

/// The landmark of a kind nearest a point (for the screenshot views).
pub fn nearest(town: &TownWorld, kind: LandmarkKind, from: DVec2) -> Option<&Landmark> {
    town.landmarks
        .iter()
        .filter(|l| l.kind == kind)
        .min_by(|a, b| a.pos.distance(from).total_cmp(&b.pos.distance(from)))
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
        ] {
            let l = Landmark {
                kind,
                pos: DVec2::ZERO,
                plaza: 14.0,
                base: 3.0,
                low: 1.0,
                height: 60.0,
                dir: DVec2::Y,
            };
            for near in [true, false] {
                let parts = model(&l, near);
                assert!(parts.iter().map(|m| m.face_count()).sum::<usize>() > 100);
            }
        }
    }
}
