//! The airship mooring mast and its liner, as meshes: the lattice tower in
//! red and white with its tulip collar, gallery and glass lift shaft; the
//! terminal under its boomerang roof; and the liner *Skylark*, a silver
//! streamliner with a red cheat line, a passenger deck of panoramic windows
//! under its belly, four engine pods and rocket fins. The moving parts
//! (mooring head, gangway, lift car, crates, baggage cart, propellers) are
//! meshes of their own; [`crate::mastview`] moves them.
//!
//! Frames: the mast's has its origin on the plinth's top and +z out towards
//! the moored ship; the ship's has its origin at the tip of its nose, the
//! nose to +z, so the body runs back along -z.

use crate::carmesh::{lathe, surface, tube, Lod, Paint};
use crate::citymesh::{MeshBuilder, Surf};
use crate::landmarkview::strut;
use crate::skins::Tex;
use engine::prelude::*;
use engine::texgen::FacadeLayer;
use glam::DVec3;
use hw_gen::urban::landmarks::Landmark;
use hw_gen::urban::mast::{
    canopy_posts, hull_r, leg_half, CANOPY_Y, CARGO_Z, CUP_Z, DECK_DROP, DECK_FROM, DECK_TO,
    GALLERY_R, GALLERY_Y, HATCH_BACK, LEG_FOOT, MAST_H, MOOR_Y, PYLON, PYLON_H, SHAFT, SHIP_LEN,
    SHIP_R, SOCK, TERMINAL_HALF, TERMINAL_WALL, TERMINAL_Z,
};
use std::f64::consts::{FRAC_PI_2, PI, TAU};

fn plain(c: Color) -> Paint {
    Some((Surf::Plain, c))
}

fn metal(c: Color) -> Paint {
    Some((Surf::Metal, c))
}

fn glass() -> Paint {
    Some((Surf::Glass, rgb(70, 104, 124).with_alpha(0.4)))
}

/// Windows lit after dark (warm, on early).
fn lit() -> Paint {
    Some((
        Surf::Lit(crate::daylight::channel(crate::daylight::Tone::Warm, 0)),
        rgb(64, 92, 110),
    ))
}

const WHITE: (u8, u8, u8) = (238, 234, 224);
const RED: (u8, u8, u8) = (196, 38, 40);
const CREAM: (u8, u8, u8) = (240, 228, 200);
const CHROME: (u8, u8, u8) = (206, 212, 218);
const TEAL: (u8, u8, u8) = (52, 168, 170);

fn c(t: (u8, u8, u8)) -> Color {
    rgb(t.0, t.1, t.2)
}

/// Aether light, as on the spire.
const AETHER: (u8, u8, u8) = (120, 230, 255);

// ------------------------------------------------------------------ mast

/// The mast's fixed parts on its plinth (the plinth itself is the
/// landmark's): legs and girders, the lift shaft, the tulip collar and the
/// gallery, the terminal and its walkway canopy, a windsock and the apron's
/// markings. The mooring head on top turns, so it is [`head`].
pub fn mast(b: &mut MeshBuilder, _l: &Landmark, near: bool) {
    let lod = if near { Lod::Near } else { Lod::Far };
    lattice(b, lod);
    collar(b, lod);
    terminal(b, near);
    if near {
        apron(b);
        windsock(b);
    }
}

/// Where a leg is at height `y`, for the corner (sx, sz).
fn leg_at(sx: f64, sz: f64, y: f64) -> DVec3 {
    let h = leg_half(y);
    DVec3::new(sx * h, y, sz * h)
}

const CORNERS: [(f64, f64); 4] = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)];

fn lattice(b: &mut MeshBuilder, lod: Lod) {
    let white = c(WHITE);
    let red = c(RED);
    let levels = 9;
    let top = GALLERY_Y - 0.6;
    // Concrete piers under the feet.
    b.tex = (lod == Lod::Near).then_some(Tex::from(FacadeLayer::Limestone));
    for (sx, sz) in CORNERS {
        b.oriented_box(
            DVec3::new(sx * LEG_FOOT, 0.3, sz * LEG_FOOT),
            DVec3::X,
            DVec3::new(0.75, 0.3, 0.75),
            rgb(196, 190, 178),
        );
    }
    b.tex = None;
    // The four legs: curving in towards the top.
    for (sx, sz) in CORNERS {
        let path: Vec<DVec3> = (0..=12)
            .map(|i| leg_at(sx, sz, top * i as f64 / 12.0))
            .collect();
        tube(b, &path, (0.3, 0.3), lod.pick(10, 6, 4), metal(white));
    }
    // Ring girders, red and white like any tall mast, and X bracing in
    // each bay.
    for k in 1..levels {
        let y = top * k as f64 / levels as f64;
        let color = if k % 2 == 1 { red } else { white };
        for i in 0..4 {
            let (a, d) = (CORNERS[i], CORNERS[(i + 1) % 4]);
            strut(
                b,
                leg_at(a.0, a.1, y),
                leg_at(d.0, d.1, y),
                lod.pick(0.24, 0.24, 0.3),
                color,
            );
        }
    }
    if lod == Lod::Far {
        return;
    }
    for k in 0..levels {
        let (y0, y1) = (
            top * k as f64 / levels as f64,
            top * (k + 1) as f64 / levels as f64,
        );
        for i in 0..4 {
            let (a, d) = (CORNERS[i], CORNERS[(i + 1) % 4]);
            strut(b, leg_at(a.0, a.1, y0), leg_at(d.0, d.1, y1), 0.1, white);
            strut(b, leg_at(d.0, d.1, y0), leg_at(a.0, a.1, y1), 0.1, white);
        }
        // Struts in to the lift shaft, every other level.
        if k % 2 == 1 {
            for (sx, sz) in CORNERS {
                strut(
                    b,
                    leg_at(sx, sz, y1),
                    DVec3::new(sx * SHAFT, y1, sz * SHAFT),
                    0.12,
                    white,
                );
            }
        }
    }
    // The lift shaft: chrome corner posts and bands, glass between, so the
    // car shows climbing it.
    let chrome = c(CHROME);
    for (sx, sz) in CORNERS {
        tube(
            b,
            &[
                DVec3::new(sx * SHAFT, 0.0, sz * SHAFT),
                DVec3::new(sx * SHAFT, top, sz * SHAFT),
            ],
            (0.09, 0.09),
            6,
            metal(chrome),
        );
    }
    b.surf = Surf::Metal;
    for k in 0..=levels * 2 {
        let y = top * k as f64 / (levels * 2) as f64;
        for i in 0..4 {
            let (a, d) = (CORNERS[i], CORNERS[(i + 1) % 4]);
            let p = DVec3::new(a.0 * SHAFT, y, a.1 * SHAFT);
            let q = DVec3::new(d.0 * SHAFT, y, d.1 * SHAFT);
            strut(b, p, q, 0.08, chrome);
        }
    }
    b.surf = Surf::Glass;
    for i in 0..4 {
        let (a, d) = (CORNERS[i], CORNERS[(i + 1) % 4]);
        let p = DVec3::new(a.0, 0.0, a.1) * SHAFT;
        let q = DVec3::new(d.0, 0.0, d.1) * SHAFT;
        let out = (p + q).normalize();
        // The door side (-z) is open at the foot.
        let y0 = if out.z < -0.5 { 2.5 } else { 0.0 };
        b.quad(
            [
                p + DVec3::Y * y0,
                q + DVec3::Y * y0,
                q + DVec3::Y * top,
                p + DVec3::Y * top,
            ],
            out,
            rgb(120, 160, 176).with_alpha(0.28),
        );
    }
    b.surf = Surf::Plain;
    // A doorway canopy over the lift's door, facing the terminal.
    b.oriented_box(
        DVec3::new(0.0, 2.65, -SHAFT - 0.4),
        DVec3::X,
        DVec3::new(SHAFT + 0.3, 0.12, 0.5),
        red,
    );
}

/// The tulip collar under the gallery, the gallery deck with its railing,
/// the head's turntable and the Aether band.
fn collar(b: &mut MeshBuilder, lod: Lod) {
    let seg = lod.pick(40, 20, 12);
    let white = c(WHITE);
    let red = c(RED);
    let g = GALLERY_Y;
    let profile = [
        (leg_half(g - 7.2) + 0.25, g - 7.2),
        (2.2, g - 6.0),
        (2.75, g - 4.5),
        (3.2, g - 3.0),
        (3.5, g - 1.6),
        (GALLERY_R, g - 0.6),
        (GALLERY_R, g),
        (1.5, g),
        (1.5, g + 0.3),
        (0.0, g + 0.3),
    ];
    let shifted: Vec<(f64, f64)> = profile.iter().map(|&(r, y)| (r, y)).collect();
    lathe(b, DVec3::ZERO, DVec3::Y, &shifted, seg, |k| match k {
        // The deck's edge band and the turntable are red, the rest white.
        5 | 7 => plain(red),
        6 => plain(rgb(150, 146, 138)),
        _ => plain(white),
    });
    // The Aether band round the collar, glowing at night and day.
    b.surf = Surf::Glow;
    b.cylinder(
        DVec3::Y * (g - 2.45),
        (3.36, 3.44),
        0.28,
        seg,
        false,
        c(AETHER),
    );
    b.surf = Surf::Plain;
    if lod == Lod::Far {
        return;
    }
    // Railing round the gallery, open towards +z where the gangway goes.
    let chrome = c(CHROME);
    let r = GALLERY_R - 0.12;
    let gap = 0.42;
    let posts = 28;
    for k in 0..posts {
        let a = TAU * k as f64 / posts as f64;
        let off = wrap(a);
        if off.abs() < gap {
            continue;
        }
        let p = DVec3::new(r * a.sin(), g, r * a.cos());
        tube(b, &[p, p + DVec3::Y * 1.1], (0.04, 0.04), 5, metal(chrome));
    }
    let rail: Vec<DVec3> = (0..=40)
        .map(|i| {
            let a = gap + (TAU - 2.0 * gap) * i as f64 / 40.0;
            DVec3::new(r * a.sin(), g + 1.1, r * a.cos())
        })
        .collect();
    tube(b, &rail, (0.05, 0.05), 6, metal(chrome));
    let mid: Vec<DVec3> = rail.iter().map(|p| *p - DVec3::Y * 0.55).collect();
    tube(b, &mid, (0.03, 0.03), 5, metal(chrome));
}

fn wrap(a: f64) -> f64 {
    (a + PI).rem_euclid(TAU) - PI
}

/// The terminal: a glass front under a boomerang roof, cream stone ends, a
/// walkway canopy on slim posts to the lift, and a Googie pylon carrying a
/// gold atom.
fn terminal(b: &mut MeshBuilder, near: bool) {
    let (hx, hz) = TERMINAL_HALF;
    let z0 = TERMINAL_Z;
    let cream = c(CREAM);
    let white = c(WHITE);
    let red = c(RED);
    let chrome = c(CHROME);
    let h = TERMINAL_WALL;
    // Floor and the stone back and ends.
    b.oriented_box(
        DVec3::new(0.0, 0.12, z0),
        DVec3::X,
        DVec3::new(hx, 0.12, hz),
        rgb(170, 164, 152),
    );
    b.tex = near.then_some(Tex::from(FacadeLayer::Limestone));
    b.oriented_box(
        DVec3::new(0.0, h * 0.5, z0 - hz + 0.2),
        DVec3::X,
        DVec3::new(hx, h * 0.5, 0.2),
        cream,
    );
    for sx in [-1.0, 1.0] {
        b.oriented_box(
            DVec3::new(sx * (hx - 0.2), h * 0.5, z0),
            DVec3::X,
            DVec3::new(0.2, h * 0.5, hz),
            cream,
        );
    }
    b.tex = None;
    // The glass front, mullions every 2 m, a red sill.
    let front = z0 + hz - 0.1;
    b.oriented_box(
        DVec3::new(0.0, 0.3, front),
        DVec3::X,
        DVec3::new(hx - 0.4, 0.3, 0.12),
        red,
    );
    if near {
        b.surf = Surf::Lit(crate::daylight::channel(crate::daylight::Tone::Warm, 1));
        b.quad(
            [
                DVec3::new(-hx + 0.4, 0.6, front),
                DVec3::new(hx - 0.4, 0.6, front),
                DVec3::new(hx - 0.4, h, front),
                DVec3::new(-hx + 0.4, h, front),
            ],
            DVec3::Z,
            rgb(64, 92, 110),
        );
        b.surf = Surf::Metal;
        let n = 8;
        for k in 0..=n {
            let x = -hx + 0.4 + (2.0 * hx - 0.8) * k as f64 / n as f64;
            b.oriented_box(
                DVec3::new(x, h * 0.5 + 0.3, front + 0.05),
                DVec3::X,
                DVec3::new(0.06, h * 0.5 - 0.3, 0.08),
                chrome,
            );
        }
        b.oriented_box(
            DVec3::new(0.0, 2.6, front + 0.05),
            DVec3::X,
            DVec3::new(hx - 0.4, 0.05, 0.08),
            chrome,
        );
        b.surf = Surf::Plain;
        // A dark floor inside, seen through the glass.
        b.oriented_box(
            DVec3::new(0.0, 0.26, z0),
            DVec3::X,
            DVec3::new(hx - 0.4, 0.02, hz - 0.4),
            rgb(90, 70, 60),
        );
    } else {
        b.quad(
            [
                DVec3::new(-hx + 0.4, 0.6, front),
                DVec3::new(hx - 0.4, 0.6, front),
                DVec3::new(hx - 0.4, h, front),
                DVec3::new(-hx + 0.4, h, front),
            ],
            DVec3::Z,
            rgb(70, 96, 112),
        );
    }
    // The boomerang roof: two slabs rising from the middle to the ends,
    // overhanging all round, a red fascia.
    let over = 1.6;
    let (rise, t) = (1.3, 0.28);
    for sx in [-1.0, 1.0] {
        let x1 = sx * (hx + over);
        let y0 = h;
        let y1 = h + rise;
        let (zf, zb) = (z0 + hz + over, z0 - hz - 0.6);
        let bot = [
            DVec3::new(0.0, y0, zb),
            DVec3::new(x1, y1, zb),
            DVec3::new(x1, y1, zf),
            DVec3::new(0.0, y0, zf),
        ];
        let top = bot.map(|p| p + DVec3::Y * t);
        b.hexa(bot, top, white);
        // Fascia along the front edge.
        let f0 = [
            DVec3::new(0.0, y0 - 0.12, zf),
            DVec3::new(x1, y1 - 0.12, zf),
            DVec3::new(x1, y1 - 0.12, zf + 0.12),
            DVec3::new(0.0, y0 - 0.12, zf + 0.12),
        ];
        b.hexa(f0, f0.map(|p| p + DVec3::Y * (t + 0.24)), red);
    }
    // Walkway canopy to the lift: a slab on slim posts with a red edge.
    let (za, zb) = (z0 + hz, -LEG_FOOT);
    let mid = (za + zb) * 0.5;
    b.oriented_box(
        DVec3::new(0.0, CANOPY_Y + 0.17, mid),
        DVec3::X,
        DVec3::new(1.9, 0.17, (zb - za).abs() * 0.5),
        white,
    );
    for sx in [-1.0, 1.0] {
        b.oriented_box(
            DVec3::new(sx * 1.92, CANOPY_Y + 0.17, mid),
            DVec3::X,
            DVec3::new(0.06, 0.22, (zb - za).abs() * 0.5),
            red,
        );
    }
    for z in canopy_posts() {
        for x in [-1.6, 1.6] {
            tube(
                b,
                &[DVec3::new(x, 0.0, z), DVec3::new(x, CANOPY_Y, z)],
                (0.1, 0.1),
                if near { 10 } else { 4 },
                metal(chrome),
            );
        }
    }
    // The pylon: a tall red blade leaning back, a teal stripe, and a gold
    // atom on top.
    let (px, pz) = (PYLON.0, PYLON.1);
    let lean = DVec3::new(0.0, 0.0, -1.2);
    let base = [
        DVec3::new(px - 0.15, 0.0, pz + 0.8),
        DVec3::new(px + 0.15, 0.0, pz + 0.8),
        DVec3::new(px + 0.15, 0.0, pz - 0.8),
        DVec3::new(px - 0.15, 0.0, pz - 0.8),
    ];
    let top = [
        DVec3::new(px - 0.12, PYLON_H, pz + 0.25),
        DVec3::new(px + 0.12, PYLON_H, pz + 0.25),
        DVec3::new(px + 0.12, PYLON_H, pz - 0.25),
        DVec3::new(px - 0.12, PYLON_H, pz - 0.25),
    ]
    .map(|p| p + lean);
    b.hexa(base, top, red);
    let stripe_b = base.map(|p| DVec3::new(p.x * 1.0, p.y, pz + (p.z - pz) * 0.3));
    let stripe_t = top.map(|p| DVec3::new(p.x, p.y, pz + lean.z + (p.z - pz - lean.z) * 0.3));
    let widen = |p: DVec3| DVec3::new(px + (p.x - px) * 1.2, p.y, p.z);
    b.hexa(stripe_b.map(widen), stripe_t.map(widen), c(TEAL));
    let atom = DVec3::new(px, PYLON_H + 1.3, pz) + lean;
    let gold = rgb(222, 178, 70);
    if near {
        for k in 0..3 {
            let a = PI * k as f64 / 3.0;
            let ring: Vec<DVec3> = (0..=28)
                .map(|i| {
                    let t = TAU * i as f64 / 28.0;
                    let (x, y) = (1.15 * t.cos(), 0.42 * t.sin());
                    atom + DVec3::new(x * a.cos() - y * a.sin(), x * a.sin() + y * a.cos(), 0.0)
                })
                .collect();
            tube(b, &ring, (0.05, 0.05), 6, metal(gold));
        }
    }
    b.surf = Surf::Glow;
    b.dome(atom - DVec3::Y * 0.22, 0.24, 12, 4, c(AETHER));
    b.dome(atom + DVec3::Y * 0.02, 0.24, 12, 1, c(AETHER));
    b.surf = Surf::Plain;
}

/// Apron paint: a yellow ring round the mast and the cargo square with
/// hazard stripes, where crates come down.
fn apron(b: &mut MeshBuilder) {
    let y = 0.014;
    let yellow = rgb(236, 196, 52);
    let ring = |b: &mut MeshBuilder, r0: f64, r1: f64| {
        let n = 64;
        for k in 0..n {
            let (a0, a1) = (TAU * k as f64 / n as f64, TAU * (k + 1) as f64 / n as f64);
            let p = |r: f64, a: f64| DVec3::new(r * a.sin(), y, r * a.cos());
            b.quad(
                [p(r0, a0), p(r1, a0), p(r1, a1), p(r0, a1)],
                DVec3::Y,
                yellow,
            );
        }
    };
    ring(b, 8.6, 8.9);
    ring(b, 20.2, 20.5);
    // The cargo square: yellow and black stripes round its edge.
    let (cz, hs) = (CARGO_Z, 2.2);
    let stripes = 10;
    for side in 0..4 {
        let (u, v) = match side {
            0 => (DVec3::X, DVec3::Z),
            1 => (DVec3::Z, -DVec3::X),
            2 => (-DVec3::X, -DVec3::Z),
            _ => (-DVec3::Z, DVec3::X),
        };
        for k in 0..stripes {
            let a = -hs + 2.0 * hs * k as f64 / stripes as f64;
            let d = 2.0 * hs / stripes as f64;
            let o = DVec3::new(0.0, y, cz) + v * hs;
            let col = if k % 2 == 0 { yellow } else { rgb(30, 30, 32) };
            b.quad(
                [
                    o + u * a,
                    o + u * (a + d),
                    o + u * (a + d) - v * 0.35,
                    o + u * a - v * 0.35,
                ],
                DVec3::Y,
                col,
            );
        }
    }
}

/// An airfield windsock on its pole, streaming out with the wind (+z, the
/// way the moored ship lies).
fn windsock(b: &mut MeshBuilder) {
    let p = DVec3::new(SOCK.0, 0.0, SOCK.1);
    let chrome = c(CHROME);
    tube(b, &[p, p + DVec3::Y * 6.2], (0.07, 0.07), 8, metal(chrome));
    let top = p + DVec3::Y * 6.0;
    lathe(
        b,
        top + DVec3::Z * 0.1,
        DVec3::new(0.0, -0.12, 1.0),
        &[
            (0.0, -0.02),
            (0.45, 0.0),
            (0.42, 0.6),
            (0.38, 1.2),
            (0.33, 1.8),
            (0.28, 2.4),
            (0.24, 3.0),
            (0.0, 3.02),
        ],
        14,
        |k| {
            if k % 2 == 1 {
                plain(rgb(240, 240, 236))
            } else {
                plain(rgb(236, 110, 30))
            }
        },
    );
}

// ------------------------------------------------------------ moving parts

/// The mooring head: a turret on the gallery's turntable with a band of
/// portholes, the mooring arm out to the cup the nose locks into, a red
/// cone roof and the beacon's spike. It turns as the moored ship swings.
pub fn head(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let seg = lod.pick(28, 14, 8);
    let g = GALLERY_Y + 0.3;
    let cream = c(CREAM);
    let red = c(RED);
    let chrome = c(CHROME);
    lathe(
        &mut b,
        DVec3::Y * g,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (1.3, 0.0),
            (1.3, 2.4),
            (1.3, 3.2),
            (1.3, MOOR_Y + 1.5 - g),
            (1.45, MOOR_Y + 1.6 - g),
            (0.25, MOOR_Y + 3.8 - g),
            (0.0, MOOR_Y + 3.85 - g),
        ],
        seg,
        |k| match k {
            2 => lit(),
            4 => metal(chrome),
            5 => plain(red),
            _ => plain(cream),
        },
    );
    // The beacon's spike.
    tube(
        &mut b,
        &[DVec3::Y * (MOOR_Y + 3.6), DVec3::Y * (MAST_H - 0.6)],
        (0.06, 0.06),
        6,
        metal(chrome),
    );
    // The arm and the cup, opening towards the ship.
    tube(
        &mut b,
        &[
            DVec3::new(0.0, MOOR_Y, 0.9),
            DVec3::new(0.0, MOOR_Y, CUP_Z - 0.7),
        ],
        (0.32, 0.32),
        lod.pick(12, 8, 6),
        metal(chrome),
    );
    lathe(
        &mut b,
        DVec3::new(0.0, MOOR_Y, CUP_Z - 0.9),
        DVec3::Z,
        &[
            (0.0, 0.0),
            (0.4, 0.0),
            (0.55, 0.4),
            (0.95, 0.85),
            (0.85, 0.9),
            (0.42, 0.5),
            (0.0, 0.5),
        ],
        lod.pick(20, 10, 8),
        |k| if k == 3 { plain(red) } else { metal(chrome) },
    );
    // A brace from the turret down under the arm.
    strut(
        &mut b,
        DVec3::new(0.0, MOOR_Y - 1.6, 1.1),
        DVec3::new(0.0, MOOR_Y - 0.25, CUP_Z - 0.9),
        0.18,
        chrome,
    );
    b.finish()
}

/// The beacon on the head's spike: glowing red, blinked by its tint.
pub fn beacon() -> Mesh {
    let mut b = MeshBuilder::new();
    b.surf = Surf::Glow;
    lathe(
        &mut b,
        DVec3::Y * (MAST_H - 0.6),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.24, 0.1),
            (0.26, 0.35),
            (0.16, 0.55),
            (0.0, 0.6),
        ],
        12,
        |_| Some((Surf::Glow, rgb(255, 40, 30))),
    );
    b.finish()
}

/// Gangway length and how far it slides out.
pub const GANGWAY_LEN: f64 = 3.4;
pub const GANGWAY_IN: f64 = 1.2;
pub const GANGWAY_OUT: f64 = 2.6;

/// The covered gangway from the gallery into the ship's nose: a deck, a
/// glass barrel roof on chrome hoops. In the head's frame with its inner
/// end at z = 0; it slides out along +z.
pub fn gangway() -> Mesh {
    let mut b = MeshBuilder::new();
    let (w, len) = (0.7, GANGWAY_LEN);
    let y = GALLERY_Y;
    let chrome = c(CHROME);
    b.oriented_box(
        DVec3::new(0.0, y + 0.06, len * 0.5),
        DVec3::X,
        DVec3::new(w + 0.05, 0.08, len * 0.5),
        rgb(150, 146, 138),
    );
    let n = 9;
    let arch = |z: f64| -> Vec<DVec3> {
        (0..=n)
            .map(|i| {
                let a = PI * i as f64 / n as f64;
                DVec3::new(w * a.cos(), y + 0.15 + 1.2 * a.sin().powf(0.6) + 0.8, z)
            })
            .map(|p| DVec3::new(p.x, p.y.max(y + 0.15), p.z))
            .collect()
    };
    // Walls up to the arch, then the arch.
    let section = |z: f64| -> Vec<DVec3> {
        let mut v = vec![DVec3::new(w, y + 0.1, z)];
        v.extend(arch(z));
        v.push(DVec3::new(-w, y + 0.1, z));
        v
    };
    let rows: Vec<Vec<DVec3>> = (0..=6).map(|i| section(len * i as f64 / 6.0)).collect();
    surface(&mut b, &rows, false, |_, j| {
        if j == 0 || j == n + 1 {
            plain(c(CREAM))
        } else {
            glass()
        }
    });
    for k in 0..=3 {
        let z = len * k as f64 / 3.0;
        tube(&mut b, &section(z), (0.05, 0.05), 5, metal(chrome));
    }
    b.finish()
}

/// The lift car: a rounded cream cabin with a lit window band; origin at its
/// floor, the door towards -z.
pub fn lift_car() -> Mesh {
    let mut b = MeshBuilder::new();
    let s = SHAFT - 0.12;
    b.oriented_box(
        DVec3::new(0.0, 1.15, 0.0),
        DVec3::X,
        DVec3::new(s, 1.15, s),
        c(CREAM),
    );
    b.surf = Surf::Lit(crate::daylight::channel(crate::daylight::Tone::Warm, 0));
    for (u, n) in [
        (DVec3::X, DVec3::Z),
        (DVec3::Z, -DVec3::X),
        (-DVec3::Z, DVec3::X),
    ] {
        let o = n * (s + 0.01) + DVec3::Y * 1.55;
        b.quad(
            [
                o - u * (s - 0.15) - DVec3::Y * 0.4,
                o + u * (s - 0.15) - DVec3::Y * 0.4,
                o + u * (s - 0.15) + DVec3::Y * 0.4,
                o - u * (s - 0.15) + DVec3::Y * 0.4,
            ],
            n,
            rgb(64, 92, 110),
        );
    }
    b.surf = Surf::Plain;
    b.oriented_box(
        DVec3::new(0.0, 2.36, 0.0),
        DVec3::X,
        DVec3::new(s + 0.04, 0.06, s + 0.04),
        c(RED),
    );
    b.finish()
}

/// A freight crate: aluminium with ribs, a red band and a lifting frame on
/// top; origin at its foot. 1.3 x 1.0 x 1.1 m.
pub const CRATE: (f64, f64, f64) = (0.65, 1.0, 0.55);

pub fn crate_mesh() -> Mesh {
    let mut b = MeshBuilder::new();
    let (hx, h, hz) = CRATE;
    let alu = rgb(186, 190, 194);
    b.surf = Surf::Metal;
    b.oriented_box(
        DVec3::new(0.0, h * 0.5, 0.0),
        DVec3::X,
        DVec3::new(hx, h * 0.5, hz),
        alu,
    );
    b.surf = Surf::Plain;
    b.oriented_box(
        DVec3::new(0.0, h * 0.62, 0.0),
        DVec3::X,
        DVec3::new(hx + 0.02, 0.12, hz + 0.02),
        c(RED),
    );
    for x in [-hx * 0.5, 0.0, hx * 0.5] {
        b.oriented_box(
            DVec3::new(x, h * 0.5, 0.0),
            DVec3::X,
            DVec3::new(0.03, h * 0.5, hz + 0.03),
            rgb(150, 154, 160),
        );
    }
    // The spreader and the hook block above it.
    let chrome = c(CHROME);
    for (sx, sz) in CORNERS {
        strut(
            &mut b,
            DVec3::new(sx * (hx - 0.1), h, sz * (hz - 0.1)),
            DVec3::new(0.0, h + 0.7, 0.0),
            0.03,
            chrome,
        );
    }
    b.oriented_box(
        DVec3::new(0.0, h + 0.8, 0.0),
        DVec3::X,
        DVec3::new(0.12, 0.12, 0.08),
        rgb(236, 196, 52),
    );
    b.finish()
}

/// One metre of winch cable, from y = 0 up; stretched to length.
pub fn cable() -> Mesh {
    let mut b = MeshBuilder::new();
    b.surf = Surf::Metal;
    strut(&mut b, DVec3::ZERO, DVec3::Y, 0.035, rgb(60, 62, 66));
    b.finish()
}

/// Where the cart's two crates ride, on its bed (cart frame, +z ahead).
pub const CART_SLOTS: [DVec3; 2] = [DVec3::new(0.0, 0.55, 0.62), DVec3::new(0.0, 0.55, -0.62)];

/// The baggage tug with its trailer: a rounded little tractor with a seat
/// and a bubble screen ahead, a flatbed behind. Origin at the bed's middle
/// on the ground, heading +z.
pub fn cart() -> Mesh {
    let mut b = MeshBuilder::new();
    let red = c(RED);
    let cream = c(CREAM);
    let dark = rgb(40, 40, 44);
    let chrome = c(CHROME);
    // Flatbed.
    b.oriented_box(
        DVec3::new(0.0, 0.48, 0.0),
        DVec3::X,
        DVec3::new(0.85, 0.07, 1.3),
        rgb(120, 120, 124),
    );
    b.oriented_box(
        DVec3::new(0.0, 0.3, 0.0),
        DVec3::X,
        DVec3::new(0.7, 0.12, 1.2),
        dark,
    );
    // Tug body ahead of the bed.
    let tz = 2.25;
    let body: Vec<Vec<DVec3>> = (0..=6)
        .map(|i| {
            let u = i as f64 / 6.0;
            let z = tz - 0.75 + 1.5 * u;
            let round = (1.0 - (2.0 * u - 1.0).powi(4)).max(0.05).sqrt();
            (0..=8)
                .map(|j| {
                    let a = PI * j as f64 / 8.0;
                    DVec3::new(0.75 * a.cos(), 0.25 + 0.75 * a.sin() * round, z)
                })
                .collect()
        })
        .collect();
    surface(&mut b, &body, false, |_, j| {
        if j == 0 || j == 7 {
            plain(cream)
        } else {
            plain(red)
        }
    });
    b.oriented_box(
        DVec3::new(0.0, 0.25, tz),
        DVec3::X,
        DVec3::new(0.75, 0.02, 0.75),
        dark,
    );
    // Seat and a little bubble screen.
    b.oriented_box(
        DVec3::new(0.0, 1.15, tz - 0.4),
        DVec3::X,
        DVec3::new(0.35, 0.12, 0.25),
        cream,
    );
    b.oriented_box(
        DVec3::new(0.0, 1.45, tz - 0.6),
        DVec3::X,
        DVec3::new(0.35, 0.3, 0.06),
        cream,
    );
    lathe(
        &mut b,
        DVec3::new(0.0, 1.0, tz + 0.2),
        DVec3::new(0.0, 0.6, 0.8),
        &[(0.0, 0.0), (0.45, 0.02), (0.38, 0.35), (0.0, 0.5)],
        12,
        |_| glass(),
    );
    // Wheels: four on the bed, four on the tug.
    for z in [-0.9, 0.9, tz - 0.5, tz + 0.5] {
        for x in [-0.8, 0.8] {
            lathe(
                &mut b,
                DVec3::new(x - 0.1 * f64::signum(x), 0.22, z),
                DVec3::X * f64::signum(x),
                &[(0.0, 0.0), (0.22, 0.0), (0.22, 0.2), (0.0, 0.2)],
                12,
                |_| plain(dark),
            );
        }
    }
    // Drawbar and a chrome bumper.
    strut(
        &mut b,
        DVec3::new(0.0, 0.35, 1.3),
        DVec3::new(0.0, 0.35, tz - 0.75),
        0.08,
        dark,
    );
    tube(
        &mut b,
        &[
            DVec3::new(-0.7, 0.35, tz + 0.8),
            DVec3::new(0.7, 0.35, tz + 0.8),
        ],
        (0.06, 0.06),
        6,
        metal(chrome),
    );
    b.finish()
}

// ------------------------------------------------------------------ ship

/// The nacelles' middles in the ship's frame (port side; starboard mirrors).
pub use hw_gen::urban::mast::{NACELLES, PROP_BACK};

/// Every propeller hub in the ship's frame.
pub fn prop_hubs() -> [DVec3; 4] {
    let n = NACELLES;
    [
        n[0] - DVec3::Z * PROP_BACK,
        DVec3::new(-n[0].x, n[0].y, n[0].z) - DVec3::Z * PROP_BACK,
        n[1] - DVec3::Z * PROP_BACK,
        DVec3::new(-n[1].x, n[1].y, n[1].z) - DVec3::Z * PROP_BACK,
    ]
}

/// The cargo hatch in the passenger deck's floor (ship frame).
pub fn hatch() -> DVec3 {
    DVec3::new(0.0, -keel(HATCH_BACK), -HATCH_BACK)
}

/// Depth of the passenger deck's keel below the axis, `s` back from the
/// nose: rounded at the bridge, faired up into the hull at the back.
fn keel(s: f64) -> f64 {
    let u = ((s - DECK_FROM) / (DECK_TO - DECK_FROM)).clamp(0.0, 1.0);
    hull_r(s) + (DECK_DROP - SHIP_R) * deck_shape(u).1
}

/// The passenger deck's width and depth factors along it (0 bridge .. 1
/// tail end).
fn deck_shape(u: f64) -> (f64, f64) {
    let front = (u / 0.12).min(1.0);
    let front = (1.0 - (1.0 - front).powi(2)).max(0.0).sqrt();
    let back = ((1.0 - u) / 0.3).min(1.0);
    let back = back.powf(0.7);
    (front.max(0.35) * back.max(0.25), front * back)
}

/// A propeller: three blades and a spinner, turning about +z. Origin at the
/// hub.
pub fn propeller(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let wood = rgb(120, 82, 44);
    lathe(
        &mut b,
        DVec3::Z * 0.1,
        -DVec3::Z,
        &[
            (0.0, 0.0),
            (0.28, 0.15),
            (0.32, 0.45),
            (0.2, 0.8),
            (0.0, 1.0),
        ],
        lod.pick(14, 8, 6),
        |_| plain(c(RED)),
    );
    for k in 0..3 {
        let a = TAU * k as f64 / 3.0;
        let d = DVec3::new(a.cos(), a.sin(), 0.0);
        let path: Vec<DVec3> = (0..6).map(|i| d * (0.25 + 0.27 * i as f64)).collect();
        tube(&mut b, &path, (0.17, 0.035), lod.pick(8, 6, 4), plain(wood));
        // Yellow tips.
        tube(
            &mut b,
            &[d * 1.42, d * 1.6],
            (0.17, 0.037),
            lod.pick(8, 6, 4),
            plain(rgb(240, 200, 50)),
        );
    }
    b.finish()
}

/// The envelope's column angles: even round the hull, plus the cheat
/// line's edges so its border runs crisp along grid lines.
fn hull_angles(seg: usize) -> Vec<f64> {
    let mut a: Vec<f64> = (0..seg).map(|j| TAU * j as f64 / seg as f64).collect();
    for e in STRIPE {
        let s = e.asin();
        // Both sides: angle measured from +x round through +y.
        for x in [s, PI - s] {
            a.push(x.rem_euclid(TAU));
        }
    }
    a.sort_by(f64::total_cmp);
    a.dedup_by(|x, y| (*x - *y).abs() < 0.02);
    a
}

/// The cheat line and its cream pinstripe: heights (as a share of the
/// radius) of their edges.
const STRIPE: [f64; 3] = [-0.34, -0.13, -0.06];

/// The liner *Skylark*. Origin at its nose's tip, nose to +z.
pub fn ship(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    envelope(&mut b, lod);
    fins(&mut b, lod);
    deck(&mut b, lod);
    engines(&mut b, lod);
    if lod != Lod::Far {
        emblem(&mut b, lod);
        lights(&mut b);
    }
    b.finish()
}

fn envelope(b: &mut MeshBuilder, lod: Lod) {
    let silver = rgb(204, 208, 214);
    let silver2 = rgb(192, 197, 204);
    let frame = rgb(182, 188, 196);
    let red = c(RED);
    let cream = c(CREAM);
    let chrome = c(CHROME);
    let angles = hull_angles(lod.pick(48, 20, 12));
    let rows = lod.pick(70, 26, 14);
    // Rows bunched towards both ends, where the curve is tight.
    let s_of = |i: usize| {
        let u = i as f64 / rows as f64;
        SHIP_LEN * (0.5 - 0.5 * (PI * u).cos())
    };
    let ss: Vec<f64> = (0..=rows).map(s_of).collect();
    let env: Vec<Vec<DVec3>> = ss
        .iter()
        .map(|&s| {
            let r = hull_r(s);
            angles
                .iter()
                .map(|&a| DVec3::new(r * a.cos(), r * a.sin(), -s))
                .collect()
        })
        .collect();
    let n = angles.len();
    surface(b, &env, true, |i, j| {
        let s = (ss[i] + ss[(i + 1).min(rows)]) * 0.5;
        let a = (angles[j] + if j + 1 < n { angles[j + 1] } else { TAU }) * 0.5;
        let y = a.sin();
        if s < 1.2 {
            return metal(chrome);
        }
        if s < 2.4 {
            return plain(cream);
        }
        if s > SHIP_LEN - 2.5 {
            return metal(chrome);
        }
        // The nose door, under the nose where the gangway docks.
        if lod != Lod::Far && (2.6..4.8).contains(&s) && y < -0.82 && a.cos().abs() < 0.3 {
            return plain(rgb(90, 92, 98));
        }
        if (6.0..80.0).contains(&s) {
            if (STRIPE[0]..STRIPE[1]).contains(&y) {
                return plain(red);
            }
            if lod != Lod::Far && (STRIPE[1]..STRIPE[2]).contains(&y) {
                return plain(cream);
            }
        }
        if lod == Lod::Near {
            // Ring frames every 8 m show through the doped skin.
            let ring = (s / 8.0).fract();
            if !(0.015..0.985).contains(&ring) {
                return metal(frame);
            }
            return metal(if j % 2 == 0 { silver } else { silver2 });
        }
        metal(silver)
    });
}

/// Four rocket fins in a cross, red with chrome leading edges, and the
/// rudder lines.
fn fins(b: &mut MeshBuilder, lod: Lod) {
    let red = c(RED);
    let chrome = c(CHROME);
    for k in 0..4 {
        let a = FRAC_PI_2 * k as f64;
        let out = DVec3::new(a.cos(), a.sin(), 0.0);
        let side = DVec3::new(-a.sin(), a.cos(), 0.0);
        // The lower fin is shorter: a liner moors over people's heads.
        let span = if k == 3 {
            hw_gen::urban::mast::FIN_SPAN_LOW
        } else {
            hw_gen::urban::mast::FIN_SPAN
        };
        let (s0, chord) = (hw_gen::urban::mast::FIN_FROM, 23.0);
        let n = lod.pick(12, 7, 5);
        let section = |sweep: f64, len: f64, reach: f64, thick: f64| -> Vec<DVec3> {
            let mut pts = Vec::new();
            for sgn in [1.0, -1.0] {
                for q in 0..n {
                    let q = if sgn > 0.0 { q } else { n - 1 - q };
                    let u = q as f64 / (n - 1) as f64;
                    let w = thick * (1.0 - u) * (u.sqrt() * 2.0).min(1.0);
                    let s = s0 + sweep + u * len;
                    let r = if reach > 0.0 {
                        hull_r(s0 + sweep + len * 0.5) + reach
                    } else {
                        hull_r(s) - 0.15
                    };
                    pts.push(out * r + side * (w * sgn) - DVec3::Z * s);
                }
            }
            pts
        };
        let root = section(0.0, chord, 0.0, 0.45);
        let mid = section(6.0, chord - 4.0, span * 0.55, 0.3);
        let tip = section(11.0, chord - 9.0, span, 0.14);
        surface(b, &[root, mid, tip], true, |_, j| {
            // The leading edge, both faces, in chrome.
            if lod != Lod::Far && (j < 2 || j + 2 >= 2 * n) {
                metal(chrome)
            } else {
                plain(red)
            }
        });
    }
}

/// The passenger deck under the belly: a long faired car with a band of
/// panoramic windows, a glazed bridge forward, a red skirt and the cargo
/// hatch in its floor.
fn deck(b: &mut MeshBuilder, lod: Lod) {
    let cream = c(CREAM);
    let red = c(RED);
    let rows = lod.pick(30, 12, 6);
    // Section points (x share of width, y share from top to keel).
    let half_sec: [(f64, f64); 7] = [
        (1.0, 0.0),
        (1.0, 0.22),
        (0.99, 0.6),
        (0.95, 0.8),
        (0.84, 0.93),
        (0.6, 0.99),
        (0.25, 1.0),
    ];
    let mut sec: Vec<(f64, f64)> = half_sec.to_vec();
    sec.extend(half_sec.iter().rev().map(|&(x, y)| (-x, y)));
    let m = sec.len();
    let width = hw_gen::urban::mast::DECK_HALF;
    let ss: Vec<f64> = (0..=rows)
        .map(|i| DECK_FROM + (DECK_TO - DECK_FROM) * i as f64 / rows as f64)
        .collect();
    let body: Vec<Vec<DVec3>> = ss
        .iter()
        .map(|&s| {
            let u = (s - DECK_FROM) / (DECK_TO - DECK_FROM);
            let (wf, _) = deck_shape(u);
            let w = width * wf;
            let top = -(hull_r(s).powi(2) - w * w).max(0.0).sqrt() + 0.4;
            let bot = -keel(s).max(-top + 0.2);
            sec.iter()
                .map(|&(x, y)| DVec3::new(x * w, top + (bot - top) * y, -s))
                .collect()
        })
        .collect();
    let hatch_i = ss.iter().position(|&s| s >= HATCH_BACK - 0.8).unwrap_or(0);
    surface(b, &body, false, |i, j| {
        let u = i as f64 / rows as f64;
        let side_win = j == 1 || j + 2 == m;
        let bridge = u < 0.09;
        if bridge && lod != Lod::Far && j < m - 1 && (1..m - 2).contains(&j) {
            return glass();
        }
        if side_win && (0.12..0.84).contains(&u) {
            if lod == Lod::Far {
                return plain(rgb(70, 96, 112));
            }
            // Window bays with slim pillars between.
            if lod == Lod::Near && i % 2 == 1 {
                return plain(cream);
            }
            return lit();
        }
        if j == 2 || j + 3 == m {
            return plain(red);
        }
        // Portholes along the lower flanks, and a red keel strip.
        if lod == Lod::Near && (j == 3 || j + 4 == m) && (0.15..0.8).contains(&u) && i % 2 == 0 {
            return lit();
        }
        if j + 1 == m / 2 {
            return plain(red);
        }
        if lod != Lod::Far && (i == hatch_i || i == hatch_i + 1) && (5..m - 6).contains(&j) {
            return plain(rgb(176, 168, 150));
        }
        plain(cream)
    });
    if lod == Lod::Far {
        return;
    }
    // A chrome rub strip above the windows, and one below.
    for y_share in [0.0, 0.6] {
        for side in [1.0, -1.0] {
            let path: Vec<DVec3> = ss
                .iter()
                .zip(&body)
                .filter(|(s, _)| {
                    let u = (**s - DECK_FROM) / (DECK_TO - DECK_FROM);
                    (0.1..0.9).contains(&u)
                })
                .map(|(_, row)| {
                    let p = if side > 0.0 { row[1] } else { row[m - 2] };
                    let q = if side > 0.0 { row[2] } else { row[m - 3] };
                    p.lerp(q, y_share / 0.6 * 1.0) + DVec3::X * side * 0.03
                })
                .collect();
            tube(b, &path, (0.04, 0.04), 5, metal(c(CHROME)));
        }
    }
}

fn engines(b: &mut MeshBuilder, lod: Lod) {
    let cream = c(CREAM);
    let red = c(RED);
    let chrome = c(CHROME);
    for n in NACELLES {
        for sx in [1.0, -1.0] {
            let p = DVec3::new(n.x * sx, n.y, n.z);
            lathe(
                b,
                p + DVec3::Z * 2.6,
                -DVec3::Z,
                &[
                    (0.0, 0.0),
                    (0.45, 0.15),
                    (0.85, 0.7),
                    (0.95, 1.6),
                    (0.9, 3.0),
                    (0.6, 4.4),
                    (0.3, 5.2),
                    (0.0, 5.4),
                ],
                lod.pick(20, 10, 6),
                |k| match k {
                    0 => metal(chrome),
                    2 => plain(red),
                    _ => plain(cream),
                },
            );
            // Two struts back to the hull.
            let s = -p.z;
            let r = hull_r(s) - 0.3;
            let dir = DVec3::new(p.x, p.y, 0.0).normalize();
            for dz in [1.0, -1.3] {
                let root = DVec3::new(dir.x * r, dir.y * r, p.z + dz);
                let foot = p + DVec3::Z * dz - dir * 0.7;
                tube(
                    b,
                    &[root, foot],
                    (0.12, 0.3),
                    lod.pick(8, 6, 4),
                    metal(chrome),
                );
            }
            if lod == Lod::Far {
                // The turning blades, as a faint disc.
                let hub = p - DVec3::Z * PROP_BACK;
                lathe(
                    b,
                    hub,
                    DVec3::Z,
                    &[(0.0, 0.0), (1.6, 0.0), (1.6, 0.02), (0.0, 0.02)],
                    10,
                    |_| plain(rgb(150, 140, 120)),
                );
            }
        }
    }
}

/// A gold atom on each flank behind the cheat line.
fn emblem(b: &mut MeshBuilder, lod: Lod) {
    let gold = rgb(222, 178, 70);
    let s = 24.0;
    let r = hull_r(s) + 0.06;
    for sx in [1.0, -1.0] {
        let centre = DVec3::new(sx * r * 0.94, r * 0.34, -s);
        let normal = DVec3::new(sx * 0.94, 0.34, 0.0).normalize();
        let u = DVec3::Z;
        let v = normal.cross(u).normalize();
        for k in 0..3 {
            let a = PI * k as f64 / 3.0;
            let ring: Vec<DVec3> = (0..=lod.pick(32, 16, 8))
                .map(|i| {
                    let t = TAU * i as f64 / lod.pick(32, 16, 8) as f64;
                    let (x, y) = (2.4 * t.cos(), 0.85 * t.sin());
                    let (x, y) = (x * a.cos() - y * a.sin(), x * a.sin() + y * a.cos());
                    // Bend onto the hull a little.
                    let p = centre + u * x + v * y;
                    p + normal * (0.08 - (y * y) * 0.006)
                })
                .collect();
            tube(b, &ring, (0.11, 0.05), 5, metal(gold));
        }
        b.surf = Surf::Plain;
        lathe(
            b,
            centre + normal * 0.05,
            normal,
            &[(0.0, 0.0), (0.45, 0.0), (0.4, 0.2), (0.0, 0.3)],
            12,
            |_| plain(c(RED)),
        );
    }
}

/// Navigation lights: red to port, green to starboard on the bridge, white
/// on the tail.
fn lights(b: &mut MeshBuilder) {
    let at = DECK_FROM + 3.0;
    let y = -(hull_r(at) + 1.2);
    let w = hw_gen::urban::mast::DECK_HALF * deck_shape(3.0 / (DECK_TO - DECK_FROM)).0 + 0.08;
    b.surf = Surf::Glow;
    // Port is the left looking forward: -x here (nose to +z, y up).
    b.dome(DVec3::new(-w, y, -at), 0.14, 8, 3, rgb(255, 40, 40));
    b.dome(DVec3::new(w, y, -at), 0.14, 8, 3, rgb(60, 255, 90));
    b.dome(
        DVec3::new(0.0, 0.0, -SHIP_LEN - 0.05),
        0.2,
        8,
        3,
        rgb(255, 250, 230),
    );
    b.surf = Surf::Plain;
}

/// The ship's three levels.
pub fn ship_levels() -> Vec<LodLevel> {
    vec![
        LodLevel::new(ship(Lod::Near), 170.0),
        LodLevel::new(ship(Lod::Mid), 900.0),
        LodLevel::new(ship(Lod::Far), 9000.0),
    ]
}

/// The head's levels.
pub fn head_levels() -> Vec<LodLevel> {
    vec![
        LodLevel::new(head(Lod::Near), 260.0),
        LodLevel::new(head(Lod::Far), 2500.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tris(m: Mesh) -> usize {
        m.face_count()
    }

    #[test]
    #[ignore = "on demand (looks): scripts/check.sh --on-demand"]
    fn the_ship_comes_in_three_levels() {
        let near = tris(ship(Lod::Near));
        let mid = tris(ship(Lod::Mid));
        let far = tris(ship(Lod::Far));
        assert!((5_000..60_000).contains(&near), "near {near}");
        assert!(mid * 2 < near, "mid {mid} vs near {near}");
        assert!(far < 3_000, "far {far}");
    }

    #[test]
    fn the_moving_parts_have_faces() {
        for m in [
            head(Lod::Near),
            beacon(),
            gangway(),
            lift_car(),
            crate_mesh(),
            cable(),
            cart(),
            propeller(Lod::Near),
        ] {
            assert!(m.face_count() >= 6);
        }
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn the_hatch_and_door_are_on_the_ship() {
        let h = hatch();
        assert!(h.y < -SHIP_R && h.y > -DECK_DROP - 0.5, "{h}");
    }
}
