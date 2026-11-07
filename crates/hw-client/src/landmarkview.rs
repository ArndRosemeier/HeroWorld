//! The town's landmarks in the game: the Aether Spire on its plaza and the
//! lighthouse on the shore, both in the 1939 World's Fair's taste.

use crate::citymesh::{MeshBuilder, Surf};
use engine::prelude::*;
use engine::texgen::{FacadeLayer, GroundLayer, Layer};
use glam::{DVec2, DVec3};
use hw_gen::urban::landmarks::{Landmark, LandmarkKind};
use hw_gen::urban::world::TownWorld;
use std::f64::consts::TAU;

/// Aether light, the same cyan as the taxi pads' rims.
const AETHER: (u8, u8, u8) = (120, 230, 255);

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
        LandmarkKind::AetherSpire => spire(&mut b, l.height, near),
        LandmarkKind::Lighthouse => lighthouse(&mut b, l.height, near),
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
        for kind in [LandmarkKind::AetherSpire, LandmarkKind::Lighthouse] {
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
