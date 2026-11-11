//! The street acts' things (see `streetview`): the prophet's soapbox and
//! sandwich board, the theremin, a busker's case with coins in it, the
//! living statue's silver crate, glowing juggling atoms, binoculars and a
//! foil cap, the newsboy's paper and bundle, a brass telescope, a portable
//! radio and the night club's neon sign. Each in three levels of detail,
//! built once on a worker thread and drawn instanced.
//!
//! Every model stands on y = 0 and faces +z (towards the audience); the
//! performer stands behind it at -z unless said otherwise.

use crate::carmesh::{lathe, slab, tube, Lod};
use crate::citymesh::{MeshBuilder, Surf};
use engine::prelude::*;
use glam::DVec3;
use std::f64::consts::{PI, TAU};

/// The things.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Gear {
    Soapbox,
    /// Two boards hung from the shoulders, front and back; its middle at
    /// the chest.
    Board,
    Theremin,
    /// Open on the ground, coins in it.
    CaseOpen,
    /// Shut, held by its handle (origin at the handle).
    CaseShut,
    StatueCrate,
    /// One glowing atom, about its middle.
    Atom,
    /// About the middle between the eyepieces, looking along +z.
    Binoculars,
    /// On the head: origin at the crown.
    FoilCap,
    /// Held up by its bottom edge.
    Paper,
    /// A bundle of papers on the ground.
    Bundle,
    Telescope,
    Radio,
    /// Flat against a wall, its back at z = 0, its middle at the origin.
    ClubSign,
    /// The Vael trader's hover disc: origin at the middle of its top, where
    /// the trader stands.
    Disc,
    /// The trader's floating tray of trinkets: origin on the ground under
    /// its middle.
    Wares,
    /// Loot a crook carries off (see `crimeview`), each held by its handle
    /// or top at the origin, hanging down: a snatched handbag...
    Handbag,
    /// ...a wad of banknotes (origin at its middle)...
    Cash,
    /// ...the protection money's envelope (origin at its middle)...
    Envelope,
    /// ...a tagger's spray can (held round its middle)...
    SprayCan,
    /// ...a robber's swag sack, tied at the neck...
    Sack,
    /// ...and a crate of stolen gadgets (its bottom at the origin).
    LootCrate,
}

impl Gear {
    pub const ALL: [Gear; 22] = [
        Gear::Soapbox,
        Gear::Board,
        Gear::Theremin,
        Gear::CaseOpen,
        Gear::CaseShut,
        Gear::StatueCrate,
        Gear::Atom,
        Gear::Binoculars,
        Gear::FoilCap,
        Gear::Paper,
        Gear::Bundle,
        Gear::Telescope,
        Gear::Radio,
        Gear::ClubSign,
        Gear::Disc,
        Gear::Wares,
        Gear::Handbag,
        Gear::Cash,
        Gear::Envelope,
        Gear::SprayCan,
        Gear::Sack,
        Gear::LootCrate,
    ];

    /// Its three levels (near, mid, far) and how far each is drawn.
    pub fn levels(self) -> Vec<LodLevel> {
        let far = match self {
            Gear::ClubSign => 220.0,
            Gear::Atom
            | Gear::Binoculars
            | Gear::FoilCap
            | Gear::Paper
            | Gear::Cash
            | Gear::Envelope
            | Gear::SprayCan => 70.0,
            _ => 130.0,
        };
        vec![
            LodLevel::new(self.mesh(Lod::Near), 18.0),
            LodLevel::new(self.mesh(Lod::Mid), 45.0),
            LodLevel::new(self.mesh(Lod::Far), far),
        ]
    }

    pub fn mesh(self, lod: Lod) -> Mesh {
        let mut b = MeshBuilder::new();
        match self {
            Gear::Soapbox => crate_box(
                &mut b,
                (0.62, hw_sim::streetlife::SOAPBOX, 0.46),
                WOOD,
                lod,
                false,
            ),
            Gear::Board => board(&mut b, lod),
            Gear::Theremin => theremin(&mut b, lod),
            Gear::CaseOpen => case_open(&mut b, lod),
            Gear::CaseShut => case_shut(&mut b, lod),
            Gear::StatueCrate => crate_box(
                &mut b,
                (0.7, hw_sim::streetlife::CRATE, 0.7),
                SILVER,
                lod,
                true,
            ),
            Gear::Atom => atom(&mut b, lod),
            Gear::Binoculars => binoculars(&mut b, lod),
            Gear::FoilCap => foil_cap(&mut b, lod),
            Gear::Paper => paper(&mut b, lod),
            Gear::Bundle => bundle(&mut b, lod),
            Gear::Telescope => telescope(&mut b, lod),
            Gear::Radio => radio(&mut b, lod),
            Gear::ClubSign => club_sign(&mut b, lod),
            Gear::Disc => disc(&mut b, lod),
            Gear::Wares => wares(&mut b, lod),
            Gear::Handbag => handbag(&mut b, lod),
            Gear::Cash => cash(&mut b, lod),
            Gear::Envelope => envelope(&mut b, lod),
            Gear::SprayCan => spray_can(&mut b, lod),
            Gear::Sack => sack(&mut b, lod),
            Gear::LootCrate => crate_box(&mut b, (0.52, 0.36, 0.4), (120, 128, 98), lod, true),
        }
        b.finish()
    }
}

const WOOD: (u8, u8, u8) = (150, 108, 66);
const SILVER: (u8, u8, u8) = (196, 200, 208);

fn col(c: (u8, u8, u8)) -> Color {
    rgb(c.0, c.1, c.2)
}

fn sides(lod: Lod, near: usize) -> usize {
    lod.pick(near, (near / 2).max(8), 6)
}

fn metal(b: &mut MeshBuilder, f: impl FnOnce(&mut MeshBuilder)) {
    let s = b.surf;
    b.surf = Surf::Metal;
    f(b);
    b.surf = s;
}

fn glow(b: &mut MeshBuilder, f: impl FnOnce(&mut MeshBuilder)) {
    let s = b.surf;
    b.surf = Surf::Glow;
    f(b);
    b.surf = s;
}

/// A slatted crate `size` (width, height, depth): corner posts, boards
/// with dark gaps, a top of planks; `paint` for the statue's silver one.
fn crate_box(
    b: &mut MeshBuilder,
    (w, h, d): (f64, f64, f64),
    c: (u8, u8, u8),
    lod: Lod,
    star: bool,
) {
    let (hw, hd) = (w * 0.5, d * 0.5);
    let dark = rgb(
        (c.0 as f32 * 0.55) as u8,
        (c.1 as f32 * 0.55) as u8,
        (c.2 as f32 * 0.55) as u8,
    );
    let body = col(c);
    // The inside, dark, shows through the gaps.
    slab(
        b,
        DVec3::new(0.0, h * 0.5, 0.0),
        [DVec3::X, DVec3::Y],
        DVec3::new(hw - 0.02, h * 0.5 - 0.01, hd - 0.02),
        dark,
    );
    if lod == Lod::Far {
        slab(
            b,
            DVec3::new(0.0, h * 0.5, 0.0),
            [DVec3::X, DVec3::Y],
            DVec3::new(hw, h * 0.5, hd),
            body,
        );
        return;
    }
    // Boards round the sides, three high.
    let boards = 3;
    let bh = h / boards as f64;
    for k in 0..boards {
        let y = bh * (k as f64 + 0.5);
        let shade = if k % 2 == 0 {
            body
        } else {
            rgb(
                (c.0 as f32 * 0.92) as u8,
                (c.1 as f32 * 0.92) as u8,
                (c.2 as f32 * 0.9) as u8,
            )
        };
        for s in [-1.0, 1.0] {
            slab(
                b,
                DVec3::new(0.0, y, hd * s),
                [DVec3::X, DVec3::Y],
                DVec3::new(hw, bh * 0.44, 0.012),
                shade,
            );
            slab(
                b,
                DVec3::new(hw * s, y, 0.0),
                [DVec3::Z, DVec3::Y],
                DVec3::new(hd, bh * 0.44, 0.012),
                shade,
            );
        }
    }
    // Corner posts and the top planks.
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        slab(
            b,
            DVec3::new((hw - 0.025) * sx, h * 0.5, (hd - 0.025) * sz),
            [DVec3::X, DVec3::Y],
            DVec3::new(0.035, h * 0.5, 0.035),
            dark,
        );
    }
    let planks = lod.pick(5, 3, 1);
    for k in 0..planks {
        let z = -hd + d * (k as f64 + 0.5) / planks as f64;
        slab(
            b,
            DVec3::new(0.0, h + 0.01, z),
            [DVec3::X, DVec3::Y],
            DVec3::new(hw + 0.01, 0.012, d / planks as f64 * 0.45),
            body,
        );
    }
    if star {
        // A red rocket-age star painted on the front.
        let red = rgb(204, 52, 44);
        let c0 = DVec3::new(0.0, h * 0.5, hd + 0.016);
        let pts: Vec<DVec3> = (0..10)
            .map(|k| {
                let a = PI / 2.0 + TAU * k as f64 / 10.0;
                let r = if k % 2 == 0 { 0.16 } else { 0.065 };
                c0 + DVec3::new(r * a.cos(), r * a.sin(), 0.0)
            })
            .collect();
        for k in 0..10 {
            b.tri([c0, pts[(k + 1) % 10], pts[k]], DVec3::Z, red);
        }
    } else if lod == Lod::Near {
        // Stencilled "SOAP" bars, the old way: a dark band.
        slab(
            b,
            DVec3::new(0.0, h * 0.55, hd + 0.016),
            [DVec3::X, DVec3::Y],
            DVec3::new(hw * 0.55, 0.04, 0.003),
            rgb(60, 40, 26),
        );
    }
}

/// The prophet's sandwich board: two cream boards, red borders, a flying
/// saucer with its beam and three warning bars, hung on straps.
fn board(b: &mut MeshBuilder, lod: Lod) {
    let cream = rgb(238, 228, 200);
    let red = rgb(196, 40, 36);
    let ink = rgb(30, 30, 36);
    let (hw, hh) = (0.3, 0.42);
    for s in [-1.0f64, 1.0] {
        let z = 0.19 * s;
        let n = DVec3::Z * s;
        slab(
            b,
            DVec3::new(0.0, -0.12, z),
            [DVec3::X, DVec3::Y],
            DVec3::new(hw, hh, 0.008),
            cream,
        );
        if lod == Lod::Far {
            continue;
        }
        let face = z + 0.0095 * s;
        // Border.
        for (c, half) in [
            (
                DVec3::new(0.0, -0.12 + hh - 0.03, face),
                DVec3::new(hw - 0.01, 0.018, 0.002),
            ),
            (
                DVec3::new(0.0, -0.12 - hh + 0.03, face),
                DVec3::new(hw - 0.01, 0.018, 0.002),
            ),
            (
                DVec3::new(hw - 0.03, -0.12, face),
                DVec3::new(0.018, hh - 0.01, 0.002),
            ),
            (
                DVec3::new(-hw + 0.03, -0.12, face),
                DVec3::new(0.018, hh - 0.01, 0.002),
            ),
        ] {
            slab(b, c, [DVec3::X, DVec3::Y], half, red);
        }
        // The saucer: a lens with a dome, portholes, a green beam.
        let sc = DVec3::new(0.0, 0.1, face);
        let lens: Vec<DVec3> = (0..sides(lod, 20))
            .map(|k| {
                let a = TAU * k as f64 / sides(lod, 20) as f64;
                sc + DVec3::new(0.19 * a.cos(), 0.045 * a.sin(), 0.001 * s)
            })
            .collect();
        b.polygon(&lens, n, rgb(120, 128, 140));
        let dome: Vec<DVec3> = (0..=10)
            .map(|k| {
                let a = PI * k as f64 / 10.0;
                sc + DVec3::new(0.08 * a.cos(), 0.03 + 0.07 * a.sin(), 0.002 * s)
            })
            .collect();
        b.polygon(&dome, n, rgb(150, 210, 230));
        let beam = [
            sc + DVec3::new(-0.05, -0.04, 0.0015 * s),
            sc + DVec3::new(0.05, -0.04, 0.0015 * s),
            sc + DVec3::new(0.16, -0.36, 0.0015 * s),
            sc + DVec3::new(-0.16, -0.36, 0.0015 * s),
        ];
        b.polygon(&beam, n, rgb(120, 220, 110));
        // "THE END IS NIGH": three ink bars under the beam.
        for (k, w) in [0.22, 0.17, 0.2].iter().enumerate() {
            slab(
                b,
                DVec3::new(0.0, -0.32 - 0.06 * k as f64, face + 0.001 * s),
                [DVec3::X, DVec3::Y],
                DVec3::new(*w, 0.016, 0.002),
                ink,
            );
        }
    }
    // Straps over the shoulders.
    if lod != Lod::Far {
        for x in [-0.14, 0.14] {
            tube(
                b,
                &[
                    DVec3::new(x, -0.12 + hh, 0.19),
                    DVec3::new(x, 0.42, 0.06),
                    DVec3::new(x, 0.44, -0.06),
                    DVec3::new(x, -0.12 + hh, -0.19),
                ],
                (0.012, 0.004),
                6,
                Some((Surf::Plain, rgb(70, 48, 30))),
            );
        }
    }
}

/// A theremin on splayed legs: a walnut cabinet, the upright pitch
/// aerial on the player's right, the volume loop on the left, a glowing
/// valve window facing the audience.
fn theremin(b: &mut MeshBuilder, lod: Lod) {
    let walnut = rgb(92, 54, 32);
    let chrome = rgb(214, 218, 224);
    let top = 0.92;
    slab(
        b,
        DVec3::new(0.0, top - 0.11, 0.0),
        [DVec3::X, DVec3::Y],
        DVec3::new(0.29, 0.11, 0.15),
        walnut,
    );
    // A lighter lid.
    slab(
        b,
        DVec3::new(0.0, top + 0.008, 0.0),
        [DVec3::X, DVec3::Y],
        DVec3::new(0.3, 0.01, 0.16),
        rgb(122, 76, 44),
    );
    // Legs.
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        tube(
            b,
            &[
                DVec3::new(0.24 * sx, top - 0.2, 0.11 * sz),
                DVec3::new(0.3 * sx, 0.0, 0.17 * sz),
            ],
            (0.018, 0.018),
            sides(lod, 8),
            Some((Surf::Plain, walnut)),
        );
    }
    metal(b, |b| {
        // The pitch aerial: upright on the player's right (-x).
        tube(
            b,
            &[
                DVec3::new(-0.24, top, -0.02),
                DVec3::new(-0.24, top + 0.56, -0.02),
            ],
            (0.008, 0.008),
            sides(lod, 10),
            Some((Surf::Metal, chrome)),
        );
        lathe(
            b,
            DVec3::new(-0.24, top, -0.02),
            DVec3::Y,
            &[(0.0, 0.0), (0.03, 0.0), (0.022, 0.04), (0.0, 0.045)],
            sides(lod, 12),
            |_| Some((Surf::Metal, chrome)),
        );
        // The volume loop: out to the left (+x), level.
        let loop_pts: Vec<DVec3> = (0..=sides(lod, 18))
            .map(|k| {
                let a = PI * k as f64 / sides(lod, 18) as f64;
                DVec3::new(0.3 + 0.1 - 0.1 * a.cos(), top - 0.06, 0.09 * a.sin() - 0.0)
            })
            .collect();
        tube(b, &loop_pts, (0.007, 0.007), 8, Some((Surf::Metal, chrome)));
    });
    if lod != Lod::Far {
        // The valve window and two knobs, audience side.
        glow(b, |b| {
            slab(
                b,
                DVec3::new(0.0, top - 0.1, 0.152),
                [DVec3::X, DVec3::Y],
                DVec3::new(0.09, 0.035, 0.004),
                rgb(255, 160, 70),
            );
        });
        for x in [-0.18, 0.18] {
            lathe(
                b,
                DVec3::new(x, top - 0.1, 0.15),
                DVec3::Z,
                &[(0.0, 0.0), (0.025, 0.0), (0.022, 0.025), (0.0, 0.028)],
                10,
                |_| Some((Surf::Metal, rgb(40, 34, 30))),
            );
        }
    }
}

/// An open case on the ground: brown leather, red velvet inside, the lid
/// standing up behind, a scatter of coins.
fn case_open(b: &mut MeshBuilder, lod: Lod) {
    let leather = rgb(96, 58, 34);
    let velvet = rgb(150, 26, 38);
    slab(
        b,
        DVec3::new(0.0, 0.04, 0.0),
        [DVec3::X, DVec3::Y],
        DVec3::new(0.3, 0.04, 0.19),
        leather,
    );
    slab(
        b,
        DVec3::new(0.0, 0.081, 0.0),
        [DVec3::X, DVec3::Y],
        DVec3::new(0.27, 0.002, 0.165),
        velvet,
    );
    // The lid, propped open behind.
    let hinge = DVec3::new(0.0, 0.08, -0.19);
    let up = DVec3::new(0.0, 0.94, -0.34).normalize();
    slab(
        b,
        hinge + up * 0.19,
        [DVec3::X, up],
        DVec3::new(0.3, 0.19, 0.02),
        leather,
    );
    if lod != Lod::Far {
        slab(
            b,
            hinge + up * 0.19 + up.cross(DVec3::X) * -0.021,
            [DVec3::X, up],
            DVec3::new(0.27, 0.17, 0.002),
            velvet,
        );
        let gold = rgb(222, 184, 80);
        let silver = rgb(210, 212, 216);
        for k in 0..lod.pick(14, 6, 0) {
            let a = k as f64 * 2.39996;
            let r = 0.04 + 0.11 * ((k * 7 % 11) as f64 / 11.0);
            let p = DVec3::new(r * a.cos() * 1.6, 0.084, r * a.sin() * 0.9);
            metal(b, |b| {
                lathe(
                    b,
                    p,
                    DVec3::Y,
                    &[(0.0, 0.0), (0.012, 0.0), (0.012, 0.003), (0.0, 0.003)],
                    8,
                    |_| Some((Surf::Metal, if k % 3 == 0 { silver } else { gold })),
                )
            });
        }
    }
}

/// A shut case held by the handle: origin at the handle's grip, the case
/// hanging below it, its long side along z.
fn case_shut(b: &mut MeshBuilder, lod: Lod) {
    let leather = rgb(96, 58, 34);
    slab(
        b,
        DVec3::new(0.0, -0.22, 0.0),
        [DVec3::Z, DVec3::Y],
        DVec3::new(0.27, 0.18, 0.06),
        leather,
    );
    if lod != Lod::Far {
        tube(
            b,
            &[
                DVec3::new(0.0, -0.04, -0.06),
                DVec3::new(0.0, 0.0, -0.04),
                DVec3::new(0.0, 0.0, 0.04),
                DVec3::new(0.0, -0.04, 0.06),
            ],
            (0.012, 0.012),
            8,
            Some((Surf::Plain, rgb(50, 32, 20))),
        );
        for z in [-0.18, 0.18] {
            slab(
                b,
                DVec3::new(0.0, -0.05, z),
                [DVec3::Z, DVec3::Y],
                DVec3::new(0.025, 0.012, 0.065),
                rgb(200, 170, 90),
            );
        }
    }
}

/// A juggling atom: a glowing nucleus with two thin orbit rings.
fn atom(b: &mut MeshBuilder, lod: Lod) {
    lathe(
        b,
        DVec3::new(0.0, -0.05, 0.0),
        DVec3::Y,
        &(0..=8)
            .map(|k| {
                let a = PI * k as f64 / 8.0;
                (0.05 * a.sin(), 0.05 - 0.05 * a.cos())
            })
            .collect::<Vec<_>>(),
        sides(lod, 14),
        |_| Some((Surf::Glow, rgb(110, 236, 255))),
    );
    if lod == Lod::Far {
        return;
    }
    for tilt in [0.5f64, -0.5] {
        let n = sides(lod, 22);
        let ring: Vec<DVec3> = (0..=n)
            .map(|k| {
                let a = TAU * k as f64 / n as f64;
                let p = DVec3::new(0.085 * a.cos(), 0.0, 0.085 * a.sin());
                DVec3::new(p.x, p.z * tilt.sin(), p.z * tilt.cos())
            })
            .collect();
        tube(
            b,
            &ring,
            (0.006, 0.006),
            6,
            Some((Surf::Glow, rgb(255, 200, 90))),
        );
    }
}

/// Binoculars: two black barrels with chrome rims, joined at the bridge.
fn binoculars(b: &mut MeshBuilder, lod: Lod) {
    for x in [-0.034, 0.034] {
        lathe(
            b,
            DVec3::new(x, 0.0, -0.04),
            DVec3::Z,
            &[
                (0.0, 0.0),
                (0.017, 0.0),
                (0.02, 0.03),
                (0.026, 0.08),
                (0.026, 0.13),
                (0.0, 0.13),
            ],
            sides(lod, 12),
            |k| {
                if k >= 3 {
                    Some((Surf::Metal, rgb(200, 204, 210)))
                } else {
                    Some((Surf::Plain, rgb(28, 28, 30)))
                }
            },
        );
    }
    slab(
        b,
        DVec3::new(0.0, 0.0, 0.02),
        [DVec3::X, DVec3::Y],
        DVec3::new(0.02, 0.01, 0.025),
        rgb(40, 40, 44),
    );
}

/// A foil cap: a lumpy, crumpled cone of kitchen foil.
fn foil_cap(b: &mut MeshBuilder, lod: Lod) {
    let n = sides(lod, 16);
    let rows: Vec<Vec<DVec3>> = (0..=5)
        .map(|k| {
            let t = k as f64 / 5.0;
            let r = 0.12 * (1.0 - t).powf(0.8) + 0.004;
            (0..n)
                .map(|j| {
                    let a = TAU * j as f64 / n as f64;
                    let bump = 1.0 + 0.12 * ((j * 7 + k * 3) % 5) as f64 / 5.0;
                    DVec3::new(
                        r * bump * a.cos(),
                        -0.06 + 0.2 * t,
                        r * bump * a.sin() * 1.1,
                    )
                })
                .collect()
        })
        .collect();
    crate::carmesh::surface(b, &rows, true, |_, _| {
        Some((Surf::Metal, rgb(214, 216, 222)))
    });
}

/// A newspaper held up: the front page facing +z, a headline in black,
/// a picture, columns of print.
fn paper(b: &mut MeshBuilder, lod: Lod) {
    let page = rgb(232, 226, 210);
    slab(
        b,
        DVec3::new(0.0, 0.2, 0.0),
        [DVec3::X, DVec3::Y],
        DVec3::new(0.15, 0.2, 0.004),
        page,
    );
    if lod == Lod::Far {
        return;
    }
    let ink = rgb(26, 26, 30);
    for s in [-1.0f64, 1.0] {
        let z = 0.005 * s;
        slab(
            b,
            DVec3::new(0.0, 0.34, z),
            [DVec3::X, DVec3::Y],
            DVec3::new(0.13, 0.03, 0.001),
            ink,
        );
        slab(
            b,
            DVec3::new(-0.06, 0.22, z),
            [DVec3::X, DVec3::Y],
            DVec3::new(0.065, 0.06, 0.001),
            rgb(90, 90, 96),
        );
        for k in 0..lod.pick(6, 3, 0) {
            slab(
                b,
                DVec3::new(0.08, 0.27 - 0.03 * k as f64, z),
                [DVec3::X, DVec3::Y],
                DVec3::new(0.05, 0.006, 0.001),
                rgb(120, 118, 112),
            );
            slab(
                b,
                DVec3::new(0.0, 0.12 - 0.022 * k as f64, z),
                [DVec3::X, DVec3::Y],
                DVec3::new(0.13, 0.005, 0.001),
                rgb(130, 128, 120),
            );
        }
    }
}

/// A bundle of papers tied with string, on the ground.
fn bundle(b: &mut MeshBuilder, lod: Lod) {
    let page = rgb(226, 220, 204);
    slab(
        b,
        DVec3::new(0.0, 0.12, 0.0),
        [DVec3::X, DVec3::Y],
        DVec3::new(0.21, 0.12, 0.15),
        page,
    );
    if lod != Lod::Far {
        slab(
            b,
            DVec3::new(0.0, 0.241, 0.0),
            [DVec3::X, DVec3::Y],
            DVec3::new(0.18, 0.001, 0.12),
            rgb(30, 30, 34),
        );
        for x in [-0.08, 0.08] {
            slab(
                b,
                DVec3::new(x, 0.12, 0.0),
                [DVec3::X, DVec3::Y],
                DVec3::new(0.006, 0.122, 0.152),
                rgb(150, 120, 80),
            );
        }
    }
}

/// A brass telescope on a wooden tripod, pointing up over the audience;
/// the eyepiece at the back (-z), at eye height for someone bending.
fn telescope(b: &mut MeshBuilder, lod: Lod) {
    let brass = rgb(200, 158, 80);
    let wood = rgb(118, 80, 46);
    let head = DVec3::new(0.0, 1.05, 0.0);
    for k in 0..3 {
        let a = PI / 2.0 + TAU * k as f64 / 3.0;
        tube(
            b,
            &[head, DVec3::new(0.42 * a.cos(), 0.0, 0.42 * a.sin())],
            (0.02, 0.02),
            sides(lod, 8),
            Some((Surf::Plain, wood)),
        );
    }
    let dir = DVec3::new(0.0, 0.52, 0.85).normalize();
    let back = head - dir * 0.42 + DVec3::Y * 0.06;
    metal(b, |b| {
        lathe(
            b,
            back,
            dir,
            &[
                (0.0, 0.0),
                (0.018, 0.0),
                (0.018, 0.08),
                (0.045, 0.1),
                (0.05, 0.4),
                (0.055, 0.42),
                (0.06, 0.95),
                (0.068, 1.0),
                (0.068, 1.08),
                (0.0, 1.08),
            ],
            sides(lod, 18),
            |k| Some((Surf::Metal, if k == 7 { rgb(60, 50, 40) } else { brass })),
        );
        if lod != Lod::Far {
            // The finder scope on top.
            let up = dir.cross(DVec3::X).normalize() * -1.0;
            lathe(
                b,
                back + dir * 0.5 + up * 0.075,
                dir,
                &[(0.0, 0.0), (0.014, 0.0), (0.014, 0.22), (0.0, 0.22)],
                8,
                |_| Some((Surf::Metal, brass)),
            );
        }
    });
}

/// A portable radio: two-tone cream and cherry red, a chrome grille, a
/// glowing dial and a carrying handle.
fn radio(b: &mut MeshBuilder, lod: Lod) {
    let cream = rgb(238, 226, 200);
    let red = rgb(190, 40, 46);
    slab(
        b,
        DVec3::new(0.0, 0.12, 0.0),
        [DVec3::X, DVec3::Y],
        DVec3::new(0.17, 0.12, 0.07),
        red,
    );
    slab(
        b,
        DVec3::new(0.0, 0.12, 0.071),
        [DVec3::X, DVec3::Y],
        DVec3::new(0.155, 0.105, 0.002),
        cream,
    );
    if lod == Lod::Far {
        return;
    }
    metal(b, |b| {
        for k in 0..lod.pick(7, 4, 0) {
            slab(
                b,
                DVec3::new(-0.04, 0.06 + 0.022 * k as f64, 0.075),
                [DVec3::X, DVec3::Y],
                DVec3::new(0.09, 0.006, 0.003),
                rgb(210, 214, 220),
            );
        }
        tube(
            b,
            &[
                DVec3::new(-0.12, 0.24, 0.0),
                DVec3::new(-0.1, 0.31, 0.0),
                DVec3::new(0.1, 0.31, 0.0),
                DVec3::new(0.12, 0.24, 0.0),
            ],
            (0.01, 0.01),
            8,
            Some((Surf::Metal, rgb(210, 214, 220))),
        );
    });
    glow(b, |b| {
        lathe(
            b,
            DVec3::new(0.1, 0.13, 0.072),
            DVec3::Z,
            &[(0.0, 0.0), (0.035, 0.0), (0.0, 0.004)],
            sides(lod, 16),
            |_| Some((Surf::Glow, rgb(255, 196, 96))),
        );
    });
}

/// The night club's sign: a dark plate with "atom" orbits of neon, a
/// glowing nucleus and a row of chaser bulbs, against the wall.
fn club_sign(b: &mut MeshBuilder, lod: Lod) {
    let plate = rgb(26, 24, 40);
    slab(
        b,
        DVec3::new(0.0, 0.0, 0.03),
        [DVec3::X, DVec3::Y],
        DVec3::new(0.75, 0.32, 0.03),
        plate,
    );
    metal(b, |b| {
        slab(
            b,
            DVec3::new(0.0, 0.0, 0.03),
            [DVec3::X, DVec3::Y],
            DVec3::new(0.78, 0.35, 0.025),
            rgb(200, 204, 212),
        );
    });
    let n = sides(lod, 40);
    for (k, tilt) in [0.0f64, 1.05, -1.05].iter().enumerate() {
        let colour = [rgb(255, 80, 190), rgb(90, 220, 255), rgb(255, 210, 90)][k];
        let ring: Vec<DVec3> = (0..=n)
            .map(|j| {
                let a = TAU * j as f64 / n as f64;
                let (x, y) = (0.62 * a.cos(), 0.17 * a.sin());
                DVec3::new(
                    x * tilt.cos() - y * tilt.sin(),
                    (x * tilt.sin() + y * tilt.cos()) * 0.8,
                    0.075,
                )
            })
            .collect();
        tube(b, &ring, (0.014, 0.014), 6, Some((Surf::Glow, colour)));
    }
    lathe(
        b,
        DVec3::new(0.0, 0.0, 0.06),
        DVec3::Z,
        &(0..=6)
            .map(|k| {
                let a = PI / 2.0 * k as f64 / 6.0;
                (0.075 * a.cos(), 0.075 * a.sin())
            })
            .collect::<Vec<_>>(),
        sides(lod, 16),
        |_| Some((Surf::Glow, rgb(255, 255, 220))),
    );
    if lod == Lod::Near {
        for k in 0..12 {
            let x = -0.7 + 1.4 * k as f64 / 11.0;
            for y in [-0.3, 0.3] {
                lathe(
                    b,
                    DVec3::new(x, y, 0.06),
                    DVec3::Z,
                    &[(0.0, 0.0), (0.016, 0.0), (0.0, 0.016)],
                    8,
                    |_| Some((Surf::Glow, rgb(255, 236, 170))),
                );
            }
        }
    }
}

/// A ball of radius `r` round `at`.
fn orb(b: &mut MeshBuilder, at: DVec3, r: f64, n: usize, surf: Surf, c: Color) {
    let rows = (n / 2).max(3);
    let profile: Vec<(f64, f64)> = (0..=rows)
        .map(|k| {
            let a = PI * k as f64 / rows as f64;
            let rad = if k == 0 || k == rows {
                0.0
            } else {
                r * a.sin()
            };
            (rad, -r * a.cos())
        })
        .collect();
    lathe(b, at, DVec3::Y, &profile, n, |_| Some((surf, c)));
}

/// The Vael trader's hover disc: a little saucer to stand on, pearl white
/// on top with a gold rim, a violet glow ring and a lens underneath where
/// it rides on its light. Its top at the origin.
fn disc(b: &mut MeshBuilder, lod: Lod) {
    let pearl = rgb(236, 232, 244);
    let gold = rgb(214, 176, 84);
    let violet = rgb(190, 120, 255);
    let h = hw_sim::streetlife::HOVER;
    lathe(
        b,
        DVec3::ZERO,
        DVec3::Y,
        &[
            (0.0, -0.2),
            (0.12, -0.2),
            (0.22, -0.16),
            (0.42, -0.08),
            (0.5, -0.02),
            (0.52, 0.0),
            (0.49, 0.025),
            (0.3, 0.03),
            (0.0, 0.03),
        ],
        sides(lod, 40),
        |k| match k {
            0 => Some((Surf::Glow, violet)),
            4 | 5 => Some((Surf::Metal, gold)),
            6 | 7 => Some((Surf::Plain, pearl)),
            _ => Some((Surf::Metal, pearl)),
        },
    );
    if lod == Lod::Far {
        return;
    }
    // The glow ring under the rim, and a soft column of light down to
    // the ground it rides on.
    let n = sides(lod, 32);
    let ring: Vec<DVec3> = (0..=n)
        .map(|k| {
            let a = TAU * k as f64 / n as f64;
            DVec3::new(0.38 * a.cos(), -0.11, 0.38 * a.sin())
        })
        .collect();
    tube(b, &ring, (0.018, 0.018), 6, Some((Surf::Glow, violet)));
    orb(
        b,
        DVec3::new(0.0, -h + 0.05, 0.0),
        0.05,
        sides(lod, 12),
        Surf::Glow,
        violet,
    );
}

/// The trader's floating tray: a gold-rimmed pearl tray on a column of
/// violet light, set out with trinkets from round another sun: glowing
/// crystals, a ringed globe, a little chrome rocket and a star in a jar.
fn wares(b: &mut MeshBuilder, lod: Lod) {
    let pearl = rgb(236, 232, 244);
    let gold = rgb(214, 176, 84);
    let violet = rgb(190, 120, 255);
    let top = 0.98;
    // The tray: a shallow dish.
    lathe(
        b,
        DVec3::Y * top,
        DVec3::Y,
        &[
            (0.0, -0.05),
            (0.3, -0.05),
            (0.38, -0.02),
            (0.4, 0.03),
            (0.37, 0.035),
            (0.0, 0.0),
        ],
        sides(lod, 36),
        |k| match k {
            2 | 3 => Some((Surf::Metal, gold)),
            _ => Some((Surf::Plain, pearl)),
        },
    );
    // Its column of light and the little emitter on the ground.
    lathe(
        b,
        DVec3::ZERO,
        DVec3::Y,
        &[(0.0, 0.0), (0.14, 0.0), (0.1, 0.06), (0.0, 0.07)],
        sides(lod, 20),
        |_| Some((Surf::Metal, gold)),
    );
    lathe(
        b,
        DVec3::Y * 0.07,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.035, 0.0),
            (0.02, top - 0.12),
            (0.0, top - 0.12),
        ],
        sides(lod, 12),
        |_| Some((Surf::Glow, violet)),
    );
    if lod == Lod::Far {
        return;
    }
    let y = top + 0.0;
    // Three crystals, pointed both ends.
    for (x, z, h, c) in [
        (-0.2, 0.08, 0.16, rgb(120, 230, 255)),
        (-0.12, -0.14, 0.12, rgb(255, 120, 200)),
        (0.02, 0.18, 0.1, rgb(140, 255, 150)),
    ] {
        lathe(
            b,
            DVec3::new(x, y, z),
            DVec3::Y,
            &[(0.0, 0.0), (0.035, h * 0.3), (0.03, h * 0.75), (0.0, h)],
            6,
            |_| Some((Surf::Glow, c)),
        );
    }
    // A ringed globe on a little stand.
    let g = DVec3::new(0.16, y + 0.12, -0.06);
    orb(b, g, 0.07, sides(lod, 20), Surf::Plain, rgb(230, 150, 80));
    let n = sides(lod, 28);
    let ring: Vec<DVec3> = (0..=n)
        .map(|k| {
            let a = TAU * k as f64 / n as f64;
            let p = DVec3::new(0.11 * a.cos(), 0.0, 0.11 * a.sin());
            g + DVec3::new(p.x, p.z * 0.35, p.z * 0.94)
        })
        .collect();
    tube(b, &ring, (0.012, 0.004), 6, Some((Surf::Metal, gold)));
    tube(
        b,
        &[DVec3::new(g.x, y, g.z), g - DVec3::Y * 0.06],
        (0.008, 0.008),
        6,
        Some((Surf::Metal, gold)),
    );
    // A little chrome rocket with red fins.
    let r = DVec3::new(0.2, y, 0.16);
    lathe(
        b,
        r,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.025, 0.0),
            (0.03, 0.05),
            (0.026, 0.12),
            (0.012, 0.17),
            (0.0, 0.19),
        ],
        sides(lod, 14),
        |_| Some((Surf::Metal, rgb(214, 218, 226))),
    );
    for k in 0..3 {
        let a = TAU * k as f64 / 3.0;
        let out = DVec3::new(a.cos(), 0.0, a.sin());
        slab(
            b,
            r + out * 0.035 + DVec3::Y * 0.03,
            [out, DVec3::Y],
            DVec3::new(0.02, 0.03, 0.004),
            rgb(200, 40, 40),
        );
    }
    // A star in a jar.
    let j = DVec3::new(-0.02, y, -0.2);
    lathe(
        b,
        j,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.045, 0.0),
            (0.05, 0.08),
            (0.035, 0.11),
            (0.03, 0.13),
            (0.0, 0.13),
        ],
        sides(lod, 16),
        |_| Some((Surf::Glass, rgb(220, 236, 240))),
    );
    orb(
        b,
        j + DVec3::Y * 0.055,
        0.025,
        10,
        Surf::Glow,
        rgb(255, 236, 140),
    );
}

/// A 1950s lady's handbag in red patent leather: a trapezoid body on a
/// gilt clasp frame, a rounded handle; held by the handle.
fn handbag(b: &mut MeshBuilder, lod: Lod) {
    let red = rgb(168, 28, 40);
    let gilt = rgb(214, 178, 92);
    // The body, wider at the bottom, in three bands.
    for (k, (y, w)) in [(-0.2, 0.15), (-0.27, 0.165), (-0.33, 0.17)]
        .iter()
        .enumerate()
    {
        slab(
            b,
            DVec3::new(0.0, *y, 0.0),
            [DVec3::X, DVec3::Y],
            DVec3::new(
                *w,
                if k == 0 { 0.035 } else { 0.033 },
                0.055 + 0.008 * k as f64,
            ),
            red,
        );
    }
    if lod == Lod::Far {
        return;
    }
    metal(b, |b| {
        tube(
            b,
            &[
                DVec3::new(-0.15, -0.165, 0.0),
                DVec3::new(0.15, -0.165, 0.0),
            ],
            (0.012, 0.012),
            sides(lod, 10),
            Some((Surf::Metal, gilt)),
        );
        // The kiss-lock clasp.
        for x in [-0.018, 0.018] {
            lathe(
                b,
                DVec3::new(x, -0.165, 0.0),
                DVec3::Y,
                &[(0.0, 0.0), (0.012, 0.004), (0.012, 0.016), (0.0, 0.026)],
                sides(lod, 10),
                |_| Some((Surf::Metal, gilt)),
            );
        }
    });
    // The handle: an arc from the frame up to the hand.
    let n = lod.pick(10, 6, 3);
    let arc: Vec<DVec3> = (0..=n)
        .map(|k| {
            let a = PI * k as f64 / n as f64;
            DVec3::new(-0.1 * a.cos(), -0.165 + 0.16 * a.sin(), 0.0)
        })
        .collect();
    tube(
        b,
        &arc,
        (0.011, 0.011),
        sides(lod, 8),
        Some((Surf::Plain, red)),
    );
}

/// A wad of green banknotes in a paper band.
fn cash(b: &mut MeshBuilder, lod: Lod) {
    let green = rgb(118, 150, 104);
    slab(
        b,
        DVec3::ZERO,
        [DVec3::X, DVec3::Y],
        DVec3::new(0.078, 0.022, 0.034),
        green,
    );
    if lod == Lod::Far {
        return;
    }
    slab(
        b,
        DVec3::ZERO,
        [DVec3::X, DVec3::Y],
        DVec3::new(0.02, 0.0235, 0.0355),
        rgb(236, 226, 196),
    );
    // A note's edge peeling off the top.
    slab(
        b,
        DVec3::new(0.0, 0.024, 0.006),
        [DVec3::X, DVec3::new(0.0, 0.94, 0.34)],
        DVec3::new(0.076, 0.001, 0.032),
        rgb(132, 164, 116),
    );
}

/// A fat manila envelope, its flap tucked in.
fn envelope(b: &mut MeshBuilder, lod: Lod) {
    let manila = rgb(214, 188, 132);
    slab(
        b,
        DVec3::ZERO,
        [DVec3::X, DVec3::Y],
        DVec3::new(0.12, 0.08, 0.012),
        manila,
    );
    if lod == Lod::Far {
        return;
    }
    // The flap and a string round the button.
    slab(
        b,
        DVec3::new(0.0, 0.05, 0.0125),
        [DVec3::X, DVec3::Y],
        DVec3::new(0.115, 0.03, 0.001),
        rgb(196, 168, 112),
    );
    lathe(
        b,
        DVec3::new(0.0, 0.03, 0.012),
        DVec3::Z,
        &[(0.0, 0.0), (0.012, 0.0), (0.012, 0.004), (0.0, 0.005)],
        sides(lod, 10),
        |_| Some((Surf::Plain, rgb(150, 40, 36))),
    );
}

/// A spray can: a tin body with a domed shoulder, a coloured cap and the
/// nozzle, held round its middle.
fn spray_can(b: &mut MeshBuilder, lod: Lod) {
    let tin = rgb(214, 216, 222);
    let band = rgb(236, 70, 140);
    let n = sides(lod, 16);
    metal(b, |b| {
        lathe(
            b,
            DVec3::new(0.0, -0.09, 0.0),
            DVec3::Y,
            &[
                (0.0, 0.0),
                (0.03, 0.002),
                (0.032, 0.01),
                (0.032, 0.05),
                (0.032, 0.12),
                (0.03, 0.15),
                (0.018, 0.165),
                (0.0, 0.168),
            ],
            n,
            |k| Some((Surf::Metal, if k == 3 { band } else { tin })),
        );
    });
    if lod == Lod::Far {
        return;
    }
    lathe(
        b,
        DVec3::new(0.0, 0.075, 0.0),
        DVec3::Y,
        &[(0.0, 0.0), (0.01, 0.0), (0.01, 0.018), (0.0, 0.02)],
        sides(lod, 10),
        |_| Some((Surf::Plain, rgb(30, 30, 34))),
    );
    tube(
        b,
        &[DVec3::new(0.0, 0.088, 0.0), DVec3::new(0.0, 0.088, 0.014)],
        (0.003, 0.003),
        6,
        Some((Surf::Plain, rgb(30, 30, 34))),
    );
}

/// A burlap swag sack, bulging, tied at the neck with rope; held by the
/// neck.
fn sack(b: &mut MeshBuilder, lod: Lod) {
    let burlap = rgb(172, 146, 104);
    let n = lod.pick(14, 8, 5);
    let profile: Vec<(f64, f64)> = (0..=n)
        .map(|k| {
            let t = k as f64 / n as f64;
            // From the bottom (round) up to the gathered neck.
            let r = if t < 0.75 {
                0.21 * (PI * (t / 0.75) * 0.5 + 0.25).sin().max(0.0) * (1.0 - 0.15 * t)
            } else {
                0.045 + 0.06 * (1.0 - (t - 0.75) / 0.25)
            };
            (if k == 0 || k == n { 0.0 } else { r }, -0.5 + 0.45 * t)
        })
        .collect();
    lathe(b, DVec3::ZERO, DVec3::Y, &profile, sides(lod, 18), |_| {
        Some((Surf::Plain, burlap))
    });
    if lod == Lod::Far {
        return;
    }
    // The rope round the neck, and the gathered top above it.
    let m = sides(lod, 14);
    let ring: Vec<DVec3> = (0..=m)
        .map(|k| {
            let a = TAU * k as f64 / m as f64;
            DVec3::new(0.052 * a.cos(), -0.11, 0.052 * a.sin())
        })
        .collect();
    tube(
        b,
        &ring,
        (0.01, 0.01),
        6,
        Some((Surf::Plain, rgb(110, 84, 50))),
    );
    lathe(
        b,
        DVec3::new(0.0, -0.11, 0.0),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.05, 0.0),
            (0.065, 0.06),
            (0.03, 0.1),
            (0.0, 0.105),
        ],
        sides(lod, 12),
        |_| Some((Surf::Plain, rgb(160, 134, 94))),
    );
    // A dollar sign stencilled on, as a comic robber's sack has.
    let ink = rgb(40, 34, 28);
    for (y, w) in [(-0.24, 0.055), (-0.3, 0.055), (-0.36, 0.055)] {
        slab(
            b,
            DVec3::new(0.0, y, 0.208),
            [DVec3::X, DVec3::Y],
            DVec3::new(w, 0.009, 0.002),
            ink,
        );
    }
    for (x, y) in [(-0.05, -0.27), (0.05, -0.33)] {
        slab(
            b,
            DVec3::new(x, y, 0.208),
            [DVec3::X, DVec3::Y],
            DVec3::new(0.009, 0.03, 0.002),
            ink,
        );
    }
    slab(
        b,
        DVec3::new(0.0, -0.3, 0.212),
        [DVec3::X, DVec3::Y],
        DVec3::new(0.008, 0.095, 0.002),
        ink,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_level_has_faces() {
        for g in Gear::ALL {
            for lod in [Lod::Near, Lod::Mid, Lod::Far] {
                assert!(g.mesh(lod).face_count() > 0, "{g:?} {lod:?}");
            }
        }
    }
}
