//! Molehaven drawn: the gate over its stair on the street, the signposts
//! pointing there, the sign over the ramp down from the utility tunnels,
//! and the town in the cavern: mushroom houses with round doors, lit
//! windows and stovepipes, the market round the well, the drill's bore
//! coming down through the roof, rock columns and stalactites, and the
//! side chambers' workshop, mushroom farm, shrine and cottages.
//!
//! Pure mesh building from `hw_gen`'s [`MolehavenLayout`] (what is drawn is
//! what collides): runs on the underground view's worker.

use crate::carmesh::{inside, lathe, slab, spline, surface, tube};
use crate::citymesh::{MeshBuilder, Surf};
use crate::skins::Tex;
use crate::tilestream::Glow;
use crate::undergroundview::{mushroom, sphere, v3};
use engine::prelude::*;
use engine::texgen::{FacadeLayer, MedievalLayer, NaturalLayer};
use glam::{DVec2, DVec3};
use hw_gen::urban::deep::FixtureKind;
use hw_gen::urban::molehaven::{
    MoleHouse, MolehavenLayout, RampSign, Signpost, Stalactite, ARM, BORE_R, GATE_ARCH,
    GATE_EMBLEM, GATE_PIER, HALL_HEIGHT, POST_H, RAMP_SIGN, TOADSTOOL,
};
use hw_gen::urban::underground::Entrance;
use std::f64::consts::{PI, TAU};

/// Molehaven's amber, on its gate, signs and lamps.
pub const AMBER: (u8, u8, u8) = (255, 170, 80);

fn amber() -> Color {
    rgb(AMBER.0, AMBER.1, AMBER.2)
}
fn brass() -> Color {
    rgb(200, 158, 78)
}
fn iron() -> Color {
    rgb(58, 52, 48)
}
fn enamel() -> Color {
    rgb(26, 74, 82)
}

/// A small hash for looks.
fn h(x: u32, k: u32) -> u32 {
    let mut v = x.wrapping_mul(0x9E37_79B1) ^ k.wrapping_mul(0x85EB_CA77);
    v ^= v >> 15;
    v = v.wrapping_mul(0x2C1B_3C6D);
    v ^= v >> 12;
    v
}

#[allow(clippy::too_many_arguments)]
/// A flat polygon round `c` in the plane of `s` (across) and `u` (up),
/// facing `n`: an ellipse `rx` by `ry`.
fn oval(b: &mut MeshBuilder, c: DVec3, s: DVec3, u: DVec3, n: DVec3, rx: f64, ry: f64, col: Color) {
    let pts: Vec<DVec3> = (0..32)
        .map(|k| {
            let a = TAU * k as f64 / 32.0;
            c + s * (a.cos() * rx) + u * (a.sin() * ry)
        })
        .collect();
    // The polygon's winding follows `s` x `u`; turn it to face `n`.
    if s.cross(u).dot(n) >= 0.0 {
        b.polygon(&pts, n, col);
    } else {
        let mut r = pts;
        r.reverse();
        b.polygon(&r, n, col);
    }
}

#[allow(clippy::too_many_arguments)]
/// A ring of tube round `c` in the plane of `s` and `u`.
fn hoop(
    b: &mut MeshBuilder,
    c: DVec3,
    s: DVec3,
    u: DVec3,
    r: f64,
    thick: f64,
    surf: Surf,
    col: Color,
) {
    let ring: Vec<DVec3> = (0..=40)
        .map(|k| {
            let a = TAU * k as f64 / 40.0;
            c + s * (a.cos() * r) + u * (a.sin() * r)
        })
        .collect();
    tube(b, &ring, (thick, thick), 8, Some((surf, col)));
}

/// Molehaven's emblem, the same on both faces: a brass-rimmed enamel disc
/// with a lit ring and a mole's face in goggles. `n` is one face's normal,
/// `u` its up.
pub fn emblem(b: &mut MeshBuilder, c: DVec3, n: DVec3, u: DVec3, r: f64) {
    let s = u.cross(n).normalize();
    hoop(b, c, s, u, r, r * 0.07, Surf::Metal, brass());
    hoop(b, c, s, u, r * 0.86, r * 0.03, Surf::Glow, amber());
    for side in [1.0, -1.0] {
        let nn = n * side;
        let at = |x: f64, y: f64, z: f64| c + s * x + u * y + nn * z;
        oval(
            b,
            at(0.0, 0.0, 0.03),
            s,
            u,
            nn,
            r * 0.84,
            r * 0.84,
            enamel(),
        );
        // The head, the ears, the pale snout and its pink nose.
        for ex in [-1.0, 1.0] {
            oval(
                b,
                at(ex * r * 0.36, r * 0.3, 0.045),
                s,
                u,
                nn,
                r * 0.13,
                r * 0.13,
                rgb(96, 70, 54),
            );
        }
        oval(
            b,
            at(0.0, -r * 0.02, 0.05),
            s,
            u,
            nn,
            r * 0.5,
            r * 0.42,
            rgb(110, 82, 62),
        );
        oval(
            b,
            at(0.0, -r * 0.24, 0.06),
            s,
            u,
            nn,
            r * 0.2,
            r * 0.13,
            rgb(214, 170, 150),
        );
        b.surf = Surf::Plain;
        sphere(b, at(0.0, -r * 0.2, 0.08), r * 0.07, rgb(236, 120, 140));
        // Goggles: brass rims round lit amber lenses.
        for ex in [-1.0, 1.0] {
            let e = at(ex * r * 0.19, r * 0.08, 0.065);
            b.surf = Surf::Glow;
            oval(b, e, s, u, nn, r * 0.09, r * 0.09, rgb(255, 196, 110));
            b.surf = Surf::Plain;
            hoop(b, e, s, u, r * 0.11, r * 0.025, Surf::Metal, brass());
        }
        // Whiskers.
        for ex in [-1.0, 1.0] {
            for wy in [-0.2, -0.27] {
                let a = at(ex * r * 0.12, r * wy, 0.065);
                let z = at(ex * r * 0.4, r * (wy - 0.04), 0.065);
                tube(
                    b,
                    &[a, z],
                    (r * 0.008, r * 0.008),
                    4,
                    Some((Surf::Plain, rgb(230, 220, 200))),
                );
            }
        }
    }
}

// ------------------------------------------------------------ the gate

/// The gate over Molehaven's stair: stone piers with brass bands and caged
/// lanterns, a riveted iron arch studded with bulbs, the emblem on its
/// crown. Sizes from `hw_gen::urban::molehaven` (its footprints match).
pub fn gate(b: &mut MeshBuilder, e: &Entrance, glows: &mut Vec<Glow>) {
    let sp = &e.spiral;
    let side = DVec2::new(sp.dir.y, -sp.dir.x);
    let (pier, ph, py) = GATE_PIER;
    let top = e.top;
    let at = |x: f64, z: f64, y: f64| v3(sp.c + side * x + sp.dir * z, top + y);
    let sx3 = v3(side, 0.0);
    let dz3 = v3(sp.dir, 0.0);
    for sx in [-1.0, 1.0] {
        let c = at(pier.x * sx, pier.y, 0.0);
        // A plinth, the shaft and a stepped cap, in dressed stone.
        b.tex = Some(FacadeLayer::Sandstone.into());
        slab(
            b,
            c + DVec3::Y * 0.2,
            [sx3, DVec3::Y],
            DVec3::new(ph + 0.08, 0.4, ph + 0.08),
            rgb(170, 150, 120),
        );
        slab(
            b,
            c + DVec3::Y * (py * 0.5),
            [sx3, DVec3::Y],
            DVec3::new(ph, py * 0.5, ph),
            rgb(186, 164, 130),
        );
        slab(
            b,
            c + DVec3::Y * (py + 0.06),
            [sx3, DVec3::Y],
            DVec3::new(ph + 0.1, 0.08, ph + 0.1),
            rgb(170, 150, 120),
        );
        b.tex = None;
        // Brass bands with rivets.
        for y in [0.75, py - 0.45] {
            b.surf = Surf::Metal;
            slab(
                b,
                c + DVec3::Y * y,
                [sx3, DVec3::Y],
                DVec3::new(ph + 0.03, 0.07, ph + 0.03),
                brass(),
            );
            b.surf = Surf::Plain;
        }
        // A caged lantern on a bracket, out over the door side.
        let out = dz3;
        let arm0 = c + DVec3::Y * 2.35 + dz3 * ph;
        let lamp = arm0 + out * 0.42 - DVec3::Y * 0.12;
        tube(
            b,
            &[
                arm0,
                arm0 + out * 0.3 + DVec3::Y * 0.12,
                lamp + DVec3::Y * 0.24,
            ],
            (0.025, 0.025),
            6,
            Some((Surf::Metal, iron())),
        );
        lathe(
            b,
            lamp - DVec3::Y * 0.22,
            DVec3::Y,
            &[
                (0.0, 0.0),
                (0.1, 0.0),
                (0.13, 0.06),
                (0.13, 0.36),
                (0.16, 0.4),
                (0.04, 0.5),
                (0.0, 0.52),
            ],
            12,
            |k| {
                if (2..=3).contains(&k) {
                    Some((Surf::Glow, amber()))
                } else {
                    Some((Surf::Metal, iron()))
                }
            },
        );
        glows.push(Glow {
            pos: lamp + out * 0.3,
            color: amber(),
            strength: 3.0,
            range: 8.0,
        });
    }
    // The arch: a riveted iron band on a semicircle, a brass beading inside,
    // and marquee bulbs along it.
    let arc = |r: f64, n: usize| -> Vec<DVec3> {
        (0..=n)
            .map(|k| {
                let a = PI * k as f64 / n as f64;
                at(-a.cos() * r, pier.y, py + a.sin() * r)
            })
            .collect()
    };
    tube(
        b,
        &arc(GATE_ARCH, 40),
        (0.2, 0.28),
        10,
        Some((Surf::Metal, rgb(120, 52, 36))),
    );
    tube(
        b,
        &arc(GATE_ARCH - 0.3, 40),
        (0.05, 0.05),
        8,
        Some((Surf::Metal, brass())),
    );
    b.surf = Surf::Glow;
    for k in 1..12 {
        let a = PI * k as f64 / 12.0;
        for zs in [1.0, -1.0] {
            let p = at(
                -a.cos() * GATE_ARCH,
                pier.y + zs * 0.29,
                py + a.sin() * GATE_ARCH,
            );
            sphere(b, p, 0.065, rgb(255, 214, 140));
        }
    }
    b.surf = Surf::Plain;
    // The emblem on the crown, in a brass ring.
    let c = at(0.0, pier.y, GATE_EMBLEM.0);
    emblem(b, c, dz3, DVec3::Y, GATE_EMBLEM.1);
    glows.push(Glow {
        pos: c + dz3 * 1.2,
        color: amber(),
        strength: 6.0,
        range: 12.0,
    });
    // A beacon on top, seen down the street at night.
    let tip = c + DVec3::Y * (GATE_EMBLEM.1 + 0.05);
    tube(
        b,
        &[tip, tip + DVec3::Y * 0.5],
        (0.04, 0.04),
        6,
        Some((Surf::Metal, brass())),
    );
    b.surf = Surf::Glow;
    sphere(b, tip + DVec3::Y * 0.62, 0.16, amber());
    b.surf = Surf::Plain;
}

// ------------------------------------------------------------ signs

/// A signpost on the street: an iron post, a brass arrow pointing the way,
/// the emblem on top, a little lamp over it.
pub fn signpost(b: &mut MeshBuilder, s: &Signpost, glows: &mut Vec<Glow>) {
    let base = v3(s.pos, s.ground);
    let post = rgb(36, 58, 50);
    lathe(
        b,
        base,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.2, 0.0),
            (0.2, 0.08),
            (0.12, 0.22),
            (0.075, 0.4),
            (0.06, POST_H - 0.6),
            (0.075, POST_H - 0.55),
            (0.0, POST_H - 0.54),
        ],
        14,
        |k| {
            if k == 4 {
                Some((Surf::Metal, brass()))
            } else {
                Some((Surf::Metal, post))
            }
        },
    );
    // The arrow: an enamelled blade with a brass edge, read from both sides.
    let p = v3(s.point, 0.0);
    let n = v3(s.point.perp(), 0.0);
    let y = s.ground + ARM.1;
    let (len, hh) = (ARM.0, 0.17);
    let shape: Vec<(f64, f64)> = vec![
        (0.05, -hh),
        (len - 0.32, -hh),
        (len - 0.32, -hh - 0.1),
        (len, 0.0),
        (len - 0.32, hh + 0.1),
        (len - 0.32, hh),
        (0.05, hh),
    ];
    for side in [1.0, -1.0] {
        let mut pts: Vec<DVec3> = shape
            .iter()
            .map(|&(x, yy)| base + p * x + n * (0.03 * side) + DVec3::Y * (y - s.ground + yy))
            .collect();
        if side < 0.0 {
            pts.reverse();
        }
        b.polygon(&pts, n * side, amber());
    }
    let mut rim: Vec<DVec3> = shape
        .iter()
        .map(|&(x, yy)| base + p * x + DVec3::Y * (y - s.ground + yy))
        .collect();
    rim.push(rim[0]);
    tube(b, &rim, (0.025, 0.025), 6, Some((Surf::Metal, brass())));
    // A collar where the arm meets the post.
    lathe(
        b,
        v3(s.pos, y - 0.25),
        DVec3::Y,
        &[(0.0, 0.0), (0.09, 0.0), (0.09, 0.5), (0.0, 0.5)],
        12,
        |_| Some((Surf::Metal, brass())),
    );
    // The emblem on top, square to the arrow, so both read together.
    let c = v3(s.pos, s.ground + POST_H - 0.2);
    emblem(b, c, n, DVec3::Y, 0.36);
    b.surf = Surf::Glow;
    sphere(b, c + DVec3::Y * 0.48, 0.08, amber());
    b.surf = Surf::Plain;
    glows.push(Glow {
        pos: c + DVec3::Y * 0.5,
        color: amber(),
        strength: 1.5,
        range: 5.0,
    });
}

/// The sign hung in the tunnel where the ramp goes down to Molehaven: an
/// enamel plate on chains, the emblem and a lit arrow pointing down the
/// ramp, on both faces.
pub fn ramp_sign(b: &mut MeshBuilder, s: &RampSign, glows: &mut Vec<Glow>) {
    let n = v3(s.face, 0.0);
    let across = v3(s.face.perp(), 0.0);
    let (hw, hh) = RAMP_SIGN;
    let c = v3(s.at, s.bottom + hh);
    slab(
        b,
        c,
        [across, DVec3::Y],
        DVec3::new(hw, hh, 0.035),
        enamel(),
    );
    let mut rim: Vec<DVec3> = [
        (-1.0, -1.0),
        (1.0, -1.0),
        (1.0, 1.0),
        (-1.0, 1.0),
        (-1.0, -1.0),
    ]
    .iter()
    .map(|&(x, y)| c + across * (x * hw) + DVec3::Y * (y * hh))
    .collect();
    rim.dedup();
    tube(b, &rim, (0.03, 0.03), 6, Some((Surf::Metal, brass())));
    // Chains to the roof.
    for x in [-hw * 0.8, hw * 0.8] {
        let a = c + across * x + DVec3::Y * hh;
        tube(
            b,
            &[a, a + DVec3::Y * 0.6],
            (0.015, 0.015),
            4,
            Some((Surf::Metal, iron())),
        );
    }
    // Which way along the plate is down the ramp.
    let way = if s.point.dot(s.face.perp()) >= 0.0 {
        1.0
    } else {
        -1.0
    };
    for side in [1.0, -1.0] {
        let nn = n * side;
        emblem_face(b, c - across * (way * hw * 0.5) + nn * 0.04, nn, hh * 0.78);
        // A lit arrow pointing along the plate and down, towards the ramp.
        let o = c + across * (way * hw * 0.42) + nn * 0.045;
        let dir = (across * way - DVec3::Y * 0.45).normalize();
        let up = nn.cross(dir).normalize();
        let shaft = [
            (-0.28, -0.05),
            (0.08, -0.05),
            (0.08, -0.13),
            (0.28, 0.0),
            (0.08, 0.13),
            (0.08, 0.05),
            (-0.28, 0.05),
        ];
        let mut pts: Vec<DVec3> = shaft.iter().map(|&(x, y)| o + dir * x + up * y).collect();
        if (dir.cross(up)).dot(nn) < 0.0 {
            pts.reverse();
        }
        b.surf = Surf::Glow;
        b.polygon(&pts, nn, amber());
        b.surf = Surf::Plain;
    }
    glows.push(Glow {
        pos: c - DVec3::Y * 0.8,
        color: amber(),
        strength: 3.0,
        range: 8.0,
    });
}

/// One face of the emblem, flat on a plate (for signs).
fn emblem_face(b: &mut MeshBuilder, c: DVec3, n: DVec3, r: f64) {
    let u = DVec3::Y;
    let s = u.cross(n).normalize();
    let at = |x: f64, y: f64, z: f64| c + s * x + u * y + n * z;
    hoop(b, c, s, u, r, r * 0.06, Surf::Glow, amber());
    // A pale field, so the brown face reads on the dark plate.
    oval(
        b,
        at(0.0, 0.0, 0.002),
        s,
        u,
        n,
        r * 0.94,
        r * 0.94,
        rgb(236, 220, 178),
    );
    for ex in [-1.0, 1.0] {
        oval(
            b,
            at(ex * r * 0.36, r * 0.3, 0.005),
            s,
            u,
            n,
            r * 0.13,
            r * 0.13,
            rgb(96, 70, 54),
        );
    }
    oval(
        b,
        at(0.0, -r * 0.02, 0.01),
        s,
        u,
        n,
        r * 0.5,
        r * 0.42,
        rgb(110, 82, 62),
    );
    oval(
        b,
        at(0.0, -r * 0.24, 0.015),
        s,
        u,
        n,
        r * 0.2,
        r * 0.13,
        rgb(214, 170, 150),
    );
    b.surf = Surf::Glow;
    for ex in [-1.0, 1.0] {
        oval(
            b,
            at(ex * r * 0.19, r * 0.08, 0.02),
            s,
            u,
            n,
            r * 0.09,
            r * 0.09,
            rgb(255, 196, 110),
        );
    }
    b.surf = Surf::Plain;
    sphere(b, at(0.0, -r * 0.2, 0.03), r * 0.07, rgb(236, 120, 140));
}

// ------------------------------------------------------------ the cavern

/// Everything in the cavern.
pub fn cavern(b: &mut MeshBuilder, l: &MolehavenLayout, glows: &mut Vec<Glow>) {
    let f = l.floor;
    for hs in &l.houses {
        house(b, hs, f, glows);
    }
    for &t in &l.toadstools {
        let k = (t.x * 3.1 + t.y * 1.7).abs() as u32;
        let glow = if k.is_multiple_of(2) {
            rgb(120, 220, 255)
        } else {
            rgb(190, 130, 255)
        };
        mushroom(b, t, f, TOADSTOOL.0, TOADSTOOL.1, 0.8, glow, glow);
    }
    for fx in &l.fixtures {
        crate::deepview::fixture(b, fx, glows);
        let p = v3(fx.plan(), fx.at.y);
        let light = match fx.kind {
            FixtureKind::Lantern => {
                Some((DVec3::Y * (fx.size.y - 0.2), rgb(255, 190, 110), 3.0, 7.0))
            }
            FixtureKind::Idol => Some((
                DVec3::Y * 1.2 + v3(fx.dir, 0.0) * 1.2,
                rgb(255, 150, 70),
                4.0,
                8.0,
            )),
            FixtureKind::GlowPool => Some((DVec3::Y * 0.6, rgb(100, 255, 200), 3.0, 7.0)),
            FixtureKind::Stall => Some((
                DVec3::Y * 2.2 + v3(fx.dir, 0.0) * 0.6,
                rgb(255, 200, 130),
                2.5,
                6.0,
            )),
            FixtureKind::Workbench => Some((DVec3::Y * 2.6, rgb(255, 230, 190), 4.0, 8.0)),
            _ => None,
        };
        if let Some((off, color, strength, range)) = light {
            glows.push(Glow {
                pos: p + off,
                color,
                strength,
                range,
            });
        }
    }
    for &(at, r) in &l.pillars {
        pillar(b, at, r, f);
    }
    for s in &l.stalactites {
        stalactite(b, s);
    }
    well(b, l, glows);
    bore(b, l, glows);
    // Strings of lit bulbs from the bore's collar down to the houses' caps.
    let hub = v3(l.well.0, l.bore - 0.2);
    for (k, hs) in l.houses.iter().filter(|h| h.hall).enumerate() {
        if k % 2 == 1 {
            continue;
        }
        let to = (hs.at - l.well.0).normalize_or(DVec2::X);
        let a = hub + v3(to, 0.0) * (BORE_R + 0.5);
        let c = v3(hs.at, f + hs.height + hs.cap * 0.5);
        // Not across a stair coming down through the roof.
        let crosses = l.ways.iter().any(|&(q, r)| {
            (0..=20).any(|i| {
                let p = a.lerp(c, i as f64 / 20.0);
                DVec2::new(p.x, p.z).distance(q) < r
            })
        });
        if crosses {
            continue;
        }
        bulbs(b, a, c, 1.6 + (k % 3) as f64 * 0.4, hs.look);
    }
}

/// A sagging wire from `a` to `c` with lit bulbs on it.
fn bulbs(b: &mut MeshBuilder, a: DVec3, c: DVec3, sag: f64, look: u32) {
    let pt = |t: f64| a.lerp(c, t) - DVec3::Y * (sag * (PI * t).sin());
    let line: Vec<DVec3> = (0..=30).map(|i| pt(i as f64 / 30.0)).collect();
    tube(
        b,
        &line,
        (0.012, 0.012),
        4,
        Some((Surf::Plain, rgb(40, 36, 32))),
    );
    let cols = [
        rgb(255, 200, 110),
        rgb(255, 140, 90),
        rgb(160, 255, 200),
        rgb(200, 170, 255),
    ];
    let n = (a.distance(c) / 1.2) as usize;
    b.surf = Surf::Glow;
    for i in 1..n {
        let q = pt(i as f64 / n as f64) - DVec3::Y * 0.08;
        sphere(b, q, 0.075, cols[(h(look, i as u32) % 4) as usize]);
    }
    b.surf = Surf::Plain;
}

/// Radius of a mushroom house's stem at height `y` over its foot (the
/// profile `mushroom` lathes).
fn stem_r(hs: &MoleHouse, y: f64) -> f64 {
    let (r, hh) = (hs.stem, hs.height);
    let prof = [
        (r * 1.35, 0.0),
        (r * 1.2, hh * 0.06),
        (r * 1.02, hh * 0.2),
        (r * 1.05, hh * 0.5),
        (r * 0.92, hh * 0.85),
        (r * 0.95, hh),
    ];
    for w in prof.windows(2) {
        let ((r0, y0), (r1, y1)) = (w[0], w[1]);
        if y <= y1 {
            let t = ((y - y0) / (y1 - y0).max(1e-6)).clamp(0.0, 1.0);
            return r0 + (r1 - r0) * t;
        }
    }
    r * 0.95
}

/// A mushroom house: plastered stem, spotted cap, an arched door with a
/// step and a lamp, round lit windows in brass frames, a stovepipe up
/// through the cap.
fn house(b: &mut MeshBuilder, hs: &MoleHouse, f: f64, glows: &mut Vec<Glow>) {
    let caps = [
        rgb(186, 58, 44),
        rgb(204, 120, 44),
        rgb(132, 64, 150),
        rgb(56, 116, 160),
        rgb(170, 70, 90),
    ];
    let cap = caps[(h(hs.look, 1) % caps.len() as u32) as usize];
    b.tex = Some(Tex::Layer(MedievalLayer::Limewash.into()));
    mushroom(
        b,
        hs.at,
        f,
        hs.stem,
        hs.height,
        hs.cap,
        cap,
        rgb(255, 200, 130),
    );
    b.tex = None;
    let door = hs.door.unwrap_or(DVec2::X);
    let s = door.perp();
    // The door: planks under a round arch, in a timber frame, a step over
    // the stem's flared foot.
    let (dw, dh, d0) = (0.58, 2.05, 0.28);
    let face = stem_r(hs, 1.0) + 0.03;
    let dc = hs.at + door * face;
    let at = |x: f64, y: f64, z: f64| v3(dc + s * x + door * z, f + y);
    let outline = |inset: f64, n: usize| -> Vec<(f64, f64)> {
        let w = dw - inset;
        let spring = d0 + dh - dw;
        let mut o = vec![(-w, d0 + inset), (w, d0 + inset)];
        for k in 0..=n {
            let a = PI * k as f64 / n as f64;
            o.push((w * a.cos(), spring + w * a.sin()));
        }
        o
    };
    let paints = [
        rgb(46, 110, 64),
        rgb(160, 52, 40),
        rgb(40, 84, 140),
        rgb(214, 170, 52),
        rgb(110, 70, 140),
    ];
    let paint = paints[(h(hs.look, 2) % paints.len() as u32) as usize];
    let pts: Vec<DVec3> = outline(0.0, 16)
        .iter()
        .map(|&(x, y)| at(x, y, 0.0))
        .collect();
    b.tex = Some(Tex::Layer(MedievalLayer::Boards.into()));
    b.polygon(&pts, v3(door, 0.0), paint);
    b.tex = None;
    // A thick frame to hide the curve behind it.
    let mut frame: Vec<DVec3> = outline(-0.08, 16)
        .iter()
        .map(|&(x, y)| at(x, y, -0.02))
        .collect();
    frame.push(at(-dw - 0.08, d0 - 0.08, -0.02));
    let fr: Vec<DVec3> = frame.into_iter().skip(1).collect();
    tube(
        b,
        &fr,
        (0.13, 0.13),
        8,
        Some((Surf::Plain, rgb(98, 66, 40))),
    );
    // Iron straps, a brass knob and a little round window in the door.
    for y in [0.75, 1.55] {
        slab(
            b,
            at(0.0, y, 0.02),
            [v3(s, 0.0), DVec3::Y],
            DVec3::new(dw - 0.06, 0.035, 0.012),
            iron(),
        );
    }
    b.surf = Surf::Metal;
    sphere(b, at(dw * 0.62, 1.15, 0.06), 0.05, brass());
    b.surf = Surf::Glow;
    oval(
        b,
        at(0.0, d0 + dh - dw * 0.7, 0.03),
        v3(s, 0.0),
        DVec3::Y,
        v3(door, 0.0),
        0.15,
        0.15,
        rgb(255, 200, 120),
    );
    b.surf = Surf::Plain;
    hoop(
        b,
        at(0.0, d0 + dh - dw * 0.7, 0.04),
        v3(s, 0.0),
        DVec3::Y,
        0.17,
        0.03,
        Surf::Metal,
        brass(),
    );
    // The step.
    b.tex = Some(FacadeLayer::Sandstone.into());
    slab(
        b,
        at(0.0, d0 * 0.5, 0.25),
        [v3(s, 0.0), DVec3::Y],
        DVec3::new(dw + 0.25, d0 * 0.5, 0.42),
        rgb(150, 132, 110),
    );
    b.tex = None;
    // A lamp on a bracket beside the door.
    let lx = dw + 0.45;
    let la = at(lx, 2.2, -0.05);
    let lamp = la + v3(door, 0.0) * 0.32 - DVec3::Y * 0.1;
    tube(
        b,
        &[
            la,
            la + v3(door, 0.0) * 0.25 + DVec3::Y * 0.08,
            lamp + DVec3::Y * 0.18,
        ],
        (0.022, 0.022),
        6,
        Some((Surf::Metal, iron())),
    );
    lathe(
        b,
        lamp - DVec3::Y * 0.16,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.07, 0.0),
            (0.09, 0.04),
            (0.09, 0.24),
            (0.11, 0.27),
            (0.03, 0.34),
            (0.0, 0.35),
        ],
        10,
        |k| {
            if (2..=3).contains(&k) {
                Some((Surf::Glow, rgb(255, 196, 110)))
            } else {
                Some((Surf::Metal, iron()))
            }
        },
    );
    glows.push(Glow {
        pos: lamp + v3(door, 0.0) * 0.5,
        color: rgb(255, 190, 120),
        strength: 2.5,
        range: 6.5,
    });
    // Round windows round the stem, lit warm, in brass frames with bars.
    let wy = (hs.height * 0.62).max(2.4).min(hs.height - 0.45);
    for (k, ang) in [1.1f64, -1.25, 2.6, -2.5].into_iter().enumerate() {
        if k >= 2 && hs.stem < 1.8 {
            break;
        }
        let dirw = door * ang.cos() + s * ang.sin();
        let r = stem_r(hs, wy) + 0.02;
        let c = v3(hs.at + dirw * r, f + wy);
        let n = v3(dirw, 0.0);
        let ws = v3(dirw.perp(), 0.0);
        b.surf = Surf::Glow;
        oval(b, c, ws, DVec3::Y, n, 0.3, 0.3, rgb(255, 196, 110));
        b.surf = Surf::Plain;
        hoop(
            b,
            c + n * 0.01,
            ws,
            DVec3::Y,
            0.33,
            0.055,
            Surf::Metal,
            brass(),
        );
        slab(
            b,
            c + n * 0.02,
            [ws, DVec3::Y],
            DVec3::new(0.3, 0.02, 0.012),
            rgb(60, 40, 26),
        );
        slab(
            b,
            c + n * 0.02,
            [ws, DVec3::Y],
            DVec3::new(0.02, 0.3, 0.012),
            rgb(60, 40, 26),
        );
        // A window box with tiny glowing mushrooms under some.
        if h(hs.look, 10 + k as u32).is_multiple_of(2) {
            let bx = c - DVec3::Y * 0.42 + n * 0.14;
            slab(
                b,
                bx,
                [ws, DVec3::Y],
                DVec3::new(0.34, 0.08, 0.12),
                rgb(120, 82, 50),
            );
            b.surf = Surf::Glow;
            for j in 0..3 {
                sphere(
                    b,
                    bx + ws * (-0.2 + 0.2 * j as f64) + DVec3::Y * 0.12,
                    0.06,
                    rgb(140, 230, 255),
                );
            }
            b.surf = Surf::Plain;
        }
    }
    // A stovepipe up through the cap, with a conical hat.
    if !h(hs.look, 3).is_multiple_of(3) {
        let a = (h(hs.look, 4) % 628) as f64 / 100.0;
        let off = DVec2::new(a.cos(), a.sin()) * hs.cap * 0.45;
        let dome = hs.cap * 0.55;
        let t = (off.length() / hs.cap).min(0.99);
        let surface_y = f + hs.height + dome * (1.0 - t * t).sqrt();
        let base = v3(hs.at + off, surface_y - 0.3);
        let tip = base + DVec3::Y * 1.5;
        lathe(
            b,
            base,
            DVec3::Y,
            &[
                (0.0, 0.0),
                (0.16, 0.0),
                (0.16, 1.5),
                (0.2, 1.5),
                (0.2, 1.62),
                (0.0, 1.62),
            ],
            14,
            |_| Some((Surf::Metal, rgb(70, 64, 58))),
        );
        lathe(
            b,
            tip + DVec3::Y * 0.25,
            DVec3::Y,
            &[(0.0, 0.0), (0.34, 0.0), (0.0, 0.26)],
            14,
            |_| Some((Surf::Metal, rgb(70, 64, 58))),
        );
        tube(
            b,
            &[tip + DVec3::Y * 0.12, tip + DVec3::Y * 0.25],
            (0.02, 0.02),
            4,
            Some((Surf::Metal, iron())),
        );
    }
}

/// Points round a lumpy ring (rock): radius `r` at angle steps, nudged by
/// a few slow waves so no two rings line up.
fn lumpy(c: DVec3, r: f64, n: usize, seed: f64) -> Vec<DVec3> {
    (0..n)
        .map(|k| {
            let a = TAU * k as f64 / n as f64;
            let w = 1.0
                + 0.12 * (a * 3.0 + seed).sin()
                + 0.07 * (a * 5.0 + seed * 1.7 + c.y * 0.4).sin()
                + 0.05 * (a * 2.0 - c.y * 0.9).cos();
            c + DVec3::new(a.cos() * r * w, 0.0, a.sin() * r * w)
        })
        .collect()
}

/// A rock column: wide at the floor and the roof, a waist between.
fn pillar(b: &mut MeshBuilder, at: DVec2, r: f64, f: f64) {
    let top = f + HALL_HEIGHT + 3.0;
    let prof = [
        (r * 1.0, 0.0),
        (r * 0.92, 1.2),
        (r * 0.8, 3.5),
        (r * 0.7, 7.0),
        (r * 0.78, 10.5),
        (r * 1.05, 13.5),
        (r * 1.6, 16.0),
        (r * 2.4, 19.0),
    ];
    let seed = at.x * 0.37 + at.y * 0.11;
    let rows: Vec<Vec<DVec3>> = prof
        .iter()
        .map(|&(rr, y)| lumpy(v3(at, (f + y).min(top)), rr, 28, seed))
        .collect();
    b.tex = Some(Tex::Layer(NaturalLayer::Limestone.into()));
    surface(b, &rows, true, |_, _| {
        Some((Surf::Plain, rgb(150, 136, 118)))
    });
    b.tex = None;
    // A ring of glowing fungus round its waist.
    b.surf = Surf::Glow;
    for k in 0..9 {
        let a = TAU * k as f64 / 9.0 + seed;
        let y = 2.0 + (k % 3) as f64 * 0.7;
        let rr = r * (0.92 - 0.12 * (y - 1.2) / 2.3);
        let p = v3(at, f + y) + DVec3::new(a.cos(), 0.0, a.sin()) * (rr + 0.04);
        let n = DVec3::new(a.cos(), 0.0, a.sin());
        lathe(
            b,
            p - n * 0.05,
            n,
            &[(0.0, 0.0), (0.16, 0.0), (0.14, 0.06), (0.0, 0.09)],
            10,
            |_| Some((Surf::Glow, rgb(120, 230, 210))),
        );
    }
    b.surf = Surf::Plain;
}

/// A stalactite: a lumpy cone from the roof to a tip, some with a glowing
/// drop.
fn stalactite(b: &mut MeshBuilder, s: &Stalactite) {
    let len = s.top - s.tip;
    let seed = s.at.x * 0.7 + s.at.y * 0.3;
    let rows: Vec<Vec<DVec3>> = [
        (s.r * 2.2, s.top + 0.5),
        (s.r * 1.4, s.top - len * 0.25),
        (s.r, s.top - len * 0.45),
        (s.r * 0.6, s.top - len * 0.7),
        (s.r * 0.25, s.top - len * 0.9),
        (0.03, s.tip),
    ]
    .iter()
    .map(|&(r, y)| lumpy(v3(s.at, y), r, 16, seed))
    .collect();
    b.tex = Some(Tex::Layer(NaturalLayer::Limestone.into()));
    surface(b, &rows, true, |_, _| {
        Some((Surf::Plain, rgb(160, 146, 128)))
    });
    b.tex = None;
    if ((seed.abs() * 10.0) as u32).is_multiple_of(3) {
        b.surf = Surf::Glow;
        sphere(b, v3(s.at, s.tip - 0.08), 0.06, rgb(120, 230, 255));
        b.surf = Surf::Plain;
    }
}

/// The well: a moulded stone ring round glowing water, the stream that
/// falls into it from the bore, a splash ring.
fn well(b: &mut MeshBuilder, l: &MolehavenLayout, glows: &mut Vec<Glow>) {
    let (c, _) = l.well;
    let f = l.floor;
    b.tex = Some(FacadeLayer::Sandstone.into());
    lathe(
        b,
        v3(c, f),
        DVec3::Y,
        &[
            (1.75, 0.75),
            (1.8, 0.0),
            (2.3, 0.0),
            (2.25, 0.12),
            (2.2, 0.7),
            (2.32, 0.76),
            (2.34, 0.9),
            (2.2, 0.98),
            (1.85, 0.98),
            (1.75, 0.9),
        ],
        48,
        |_| Some((Surf::Plain, rgb(130, 118, 100))),
    );
    b.tex = None;
    // Brass bands round it.
    b.surf = Surf::Metal;
    lathe(
        b,
        v3(c, f + 0.3),
        DVec3::Y,
        &[(2.27, 0.0), (2.31, 0.02), (2.31, 0.1), (2.27, 0.12)],
        48,
        |_| Some((Surf::Metal, brass())),
    );
    b.surf = Surf::Plain;
    lathe(
        b,
        v3(c, f + 0.75),
        DVec3::Y,
        &[(0.0, 0.0), (1.8, 0.0), (0.0, 0.001)],
        48,
        |_| Some((Surf::Glow, rgb(90, 220, 255))),
    );
    // The stream from the bore, and rings where it lands.
    let top = l.bore + 0.4;
    let path: Vec<DVec3> = (0..=12)
        .map(|k| {
            let t = k as f64 / 12.0;
            v3(c, top + (f + 0.76 - top) * t)
        })
        .collect();
    tube(
        b,
        &path,
        (0.18, 0.18),
        14,
        Some((Surf::Glow, rgb(130, 230, 255))),
    );
    for (r, y) in [(0.45, 0.78), (0.8, 0.77)] {
        hoop(
            b,
            v3(c, f + y),
            DVec3::X,
            DVec3::Z,
            r,
            0.025,
            Surf::Glow,
            rgb(200, 250, 255),
        );
    }
    glows.push(Glow {
        pos: v3(c, f + 2.0),
        color: rgb(90, 220, 255),
        strength: 8.0,
        range: 14.0,
    });
}

/// The drill's bore through the roof over the well: an iron-lined shaft
/// with a riveted collar, struts into the rock, the bit's tip glowing hot
/// up in it.
fn bore(b: &mut MeshBuilder, l: &MolehavenLayout, glows: &mut Vec<Glow>) {
    let (c, _) = l.well;
    let y0 = l.bore;
    let y1 = l.floor + HALL_HEIGHT + 3.4;
    let r = BORE_R;
    // The lining, seen from inside.
    inside(|| {
        lathe(
            b,
            v3(c, y0),
            DVec3::Y,
            &[(r, 0.0), (r, y1 - y0)],
            40,
            |_| Some((Surf::Metal, rgb(78, 66, 58))),
        );
    });
    // Rings down the lining.
    for k in 1..5 {
        let y = y0 + (y1 - y0) * k as f64 / 5.0;
        inside(|| {
            lathe(
                b,
                v3(c, y),
                DVec3::Y,
                &[(r - 0.08, 0.0), (r - 0.08, 0.12)],
                40,
                |_| Some((Surf::Metal, rgb(120, 58, 40))),
            );
        });
    }
    // The collar: a flanged, riveted ring round the bore's mouth.
    lathe(
        b,
        v3(c, y0 - 0.35),
        DVec3::Y,
        &[
            (r - 0.05, 0.0),
            (r + 0.7, 0.0),
            (r + 0.75, 0.08),
            (r + 0.75, 0.3),
            (r + 0.3, 0.42),
            (r + 0.3, 0.9),
            (r - 0.05, 0.9),
            (r - 0.05, 0.0),
        ],
        48,
        |k| match k {
            1..=2 => Some((Surf::Metal, brass())),
            _ => Some((Surf::Metal, rgb(120, 52, 36))),
        },
    );
    b.surf = Surf::Metal;
    for k in 0..24 {
        let a = TAU * k as f64 / 24.0;
        let n = DVec3::new(a.cos(), 0.0, a.sin());
        sphere(b, v3(c, y0 - 0.25) + n * (r + 0.76), 0.05, brass());
    }
    b.surf = Surf::Plain;
    // Struts up into the rock.
    for k in 0..6 {
        let a = TAU * k as f64 / 6.0 + 0.3;
        let n = DVec3::new(a.cos(), 0.0, a.sin());
        let a0 = v3(c, y0 + 0.2) + n * (r + 0.4);
        let a1 = a0 + n * 3.4 + DVec3::Y * 4.2;
        tube(
            b,
            &[a0, a1],
            (0.12, 0.12),
            8,
            Some((Surf::Metal, rgb(90, 60, 48))),
        );
    }
    // The bit's tip, hot from boring, and the glow at the top that closes
    // the shaft.
    lathe(
        b,
        v3(c, y0 + 1.4),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.5, 0.5),
            (1.0, 1.1),
            (1.4, 1.9),
            (1.6, 2.6),
            (1.6, 3.0),
            (0.0, 3.0),
        ],
        32,
        |k| {
            if k == 0 {
                Some((Surf::Glow, rgb(255, 120, 50)))
            } else {
                Some((Surf::Metal, rgb(150, 150, 156)))
            }
        },
    );
    // Flutes up the bit.
    for k in 0..4 {
        let pts: Vec<DVec3> = (0..=16)
            .map(|i| {
                let t = i as f64 / 16.0;
                let a = TAU * (k as f64 / 4.0 + t * 0.9);
                let rr = 0.3 + 1.25 * t;
                v3(c, y0 + 1.6 + t * 2.4) + DVec3::new(a.cos() * rr, 0.0, a.sin() * rr)
            })
            .collect();
        tube(
            b,
            &spline(&pts, 2, false),
            (0.07, 0.07),
            6,
            Some((Surf::Metal, rgb(176, 176, 182))),
        );
    }
    lathe(
        b,
        v3(c, y1),
        DVec3::Y,
        &[(0.0, 0.0), (r + 0.1, 0.0), (0.0, 0.01)],
        40,
        |_| Some((Surf::Glow, rgb(255, 120, 50))),
    );
    glows.push(Glow {
        pos: v3(c, y0 - 0.5),
        color: rgb(255, 140, 70),
        strength: 6.0,
        range: 14.0,
    });
}
