//! The new factions' things: the sky pirates' airship, the Iron-Masked
//! Empire's drop capsule, the hole mole folk dig up through a street, and
//! the weapons their fighters carry (a delver's pick, a pirate's cutlass,
//! a praetorian's shock lance).
//!
//! Everything is built the way the cars are: lofted and lathed surfaces
//! with smooth normals ([`MeshBuilder::grid`] through `carmesh`'s helpers),
//! hard edges only where the real thing has them (broken asphalt, fins,
//! planks). Big things come in three levels.
//!
//! Units are metres, +y up; each model says where its origin is.

use crate::carmesh::{inside, lathe, spline, surface, tube, Lod, Paint};
use crate::citymesh::{MeshBuilder, Surf};
use engine::prelude::*;
use glam::{DVec2, DVec3};
use std::f64::consts::{PI, TAU};

fn plain(c: Color) -> Paint {
    Some((Surf::Plain, c))
}

fn metal(c: Color) -> Paint {
    Some((Surf::Metal, c))
}

fn glow(c: Color) -> Paint {
    Some((Surf::Glow, c))
}

/// A small deterministic jitter in -1..1 from two numbers.
fn jitter(a: u32, b: u32) -> f64 {
    let mut h = a.wrapping_mul(0x9E37_79B9) ^ b.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h as f64 / u32::MAX as f64) * 2.0 - 1.0
}

// ---------------------------------------------------------------- airship

/// The airship's envelope: length and widest radius (m).
pub const AIRSHIP_LENGTH: f64 = 46.0;
pub const AIRSHIP_RADIUS: f64 = 7.5;
/// Where the gondola's deck is below the envelope's axis (m): rope
/// ladders hang from here.
pub const GONDOLA_DECK: f64 = AIRSHIP_RADIUS + 2.2;

/// Radius of the envelope at `t` (0 nose to 1 tail): a blunt nose, the
/// widest a third of the way back, a long taper to the tail.
fn envelope_r(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    let nose = (t / 0.33).min(1.0);
    let tail = ((1.0 - t) / 0.67).min(1.0);
    let n = (1.0 - (1.0 - nose).powi(2)).max(0.0).sqrt();
    let k = tail.powf(0.75);
    AIRSHIP_RADIUS * n * k.max(0.0)
}

/// The sky pirates' airship *Cloudcutter*: a patched canvas envelope with
/// brass ribs and a red tail, a wooden galleon of a gondola slung beneath
/// on cables, two brass engines with propellers, a black flag at the
/// stern. Origin at the envelope's middle, nose to +z.
pub fn airship(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let seg = lod.pick(36, 18, 10);
    let rows = lod.pick(44, 22, 10);
    let canvas = rgb(232, 220, 190);
    let patch = rgb(206, 188, 150);
    let red = rgb(160, 30, 28);
    let brass = rgb(196, 160, 82);
    let dark = rgb(40, 30, 24);
    let half = AIRSHIP_LENGTH * 0.5;
    // The envelope, in gores: canvas with darker patches here and there,
    // a red band round the tail.
    let ring = |t: f64| -> Vec<DVec3> {
        let r = envelope_r(t);
        let z = half - t * AIRSHIP_LENGTH;
        (0..seg)
            .map(|j| {
                let a = TAU * j as f64 / seg as f64;
                DVec3::new(r * a.cos(), r * a.sin(), z)
            })
            .collect()
    };
    let env: Vec<Vec<DVec3>> = (0..=rows)
        .map(|i| {
            // Rows bunch at the nose and tail where the curve is tight.
            let u = i as f64 / rows as f64;
            ring(0.5 - 0.5 * (PI * u).cos())
        })
        .collect();
    surface(&mut b, &env, true, |i, j| {
        let t = i as f64 / rows as f64;
        if i == 0 {
            return metal(brass);
        }
        if (0.78..0.84).contains(&t) {
            return plain(red);
        }
        if lod != Lod::Far && jitter(i as u32 / 3, j as u32 / 2) > 0.62 {
            return plain(patch);
        }
        plain(canvas)
    });
    // Brass ribs round the envelope.
    if lod != Lod::Far {
        for k in 1..8 {
            let t = k as f64 / 8.0;
            let r = envelope_r(t) + 0.06;
            let z = half - t * AIRSHIP_LENGTH;
            let n = seg.max(12);
            let path: Vec<DVec3> = (0..=n)
                .map(|j| {
                    let a = TAU * j as f64 / n as f64;
                    DVec3::new(r * a.cos(), r * a.sin(), z)
                })
                .collect();
            tube(&mut b, &path, (0.09, 0.09), 6, metal(brass));
        }
    }
    // Four tail fins, red with a canvas stripe.
    for k in 0..4 {
        let a = TAU * k as f64 / 4.0 + PI * 0.25;
        let out = DVec3::new(a.cos(), a.sin(), 0.0);
        let root_t = 0.8;
        let root_z = half - root_t * AIRSHIP_LENGTH;
        let chord = 9.0;
        let span = 5.5;
        let section = |z0: f64, len: f64, r: f64, thick: f64| -> Vec<DVec3> {
            // A thin aerofoil: round leading edge, sharp trailing edge.
            let n = lod.pick(10, 6, 4);
            let side = DVec3::new(-a.sin(), a.cos(), 0.0);
            let mut pts = Vec::new();
            for s in 0..n {
                let u = s as f64 / (n - 1) as f64;
                let w = thick * (1.0 - u) * (u.sqrt() * 2.0).min(1.0);
                pts.push(out * r + side * w + DVec3::Z * (z0 - u * len));
            }
            for s in (0..n).rev() {
                let u = s as f64 / (n - 1) as f64;
                let w = thick * (1.0 - u) * (u.sqrt() * 2.0).min(1.0);
                pts.push(out * r - side * w + DVec3::Z * (z0 - u * len));
            }
            pts
        };
        let r0 = envelope_r(root_t) - 0.3;
        let root = section(root_z, chord, r0, 0.35);
        let tip = section(root_z - 2.8, chord - 3.4, r0 + span, 0.12);
        let stripe = lod.pick(10, 6, 4);
        surface(&mut b, &[root, tip], true, |_, j| {
            if lod != Lod::Far && (j + 2) % stripe < 2 {
                plain(canvas)
            } else {
                plain(red)
            }
        });
    }
    gondola(&mut b, lod, dark, brass);
    b.finish()
}

/// The gondola: a wooden galleon hull slung under the envelope, with a
/// glazed wheelhouse forward, brass rail, engines on outriggers, cables up
/// to the envelope and a black flag astern.
fn gondola(b: &mut MeshBuilder, lod: Lod, dark: Color, brass: Color) {
    let wood = rgb(120, 74, 40);
    let wood2 = rgb(98, 58, 30);
    let deck_y = -GONDOLA_DECK;
    let len = 18.0;
    let beam = 2.0;
    let depth = 2.4;
    let rows = lod.pick(22, 10, 5);
    let seg = lod.pick(16, 10, 6);
    // Cross-sections of a ship's hull from bow (+z) to stern: a U that
    // narrows to the stem, the keel rising at both ends.
    let hull: Vec<Vec<DVec3>> = (0..=rows)
        .map(|i| {
            let u = i as f64 / rows as f64;
            let z = len * 0.5 - u * len;
            let w = beam * (1.0 - (2.0 * u - 1.0).powi(4)).max(0.02).powf(0.6);
            let d = depth * (1.0 - (2.0 * u - 1.0).powi(6) * 0.6);
            (0..=seg)
                .map(|j| {
                    // From the port gunwale down round the keel to starboard.
                    let a = PI * j as f64 / seg as f64;
                    let x = -w * a.cos();
                    let y = deck_y - d * a.sin().powf(0.8);
                    DVec3::new(x, y, z)
                })
                .collect()
        })
        .collect();
    surface(b, &hull, false, |_, j| {
        // Planks: alternate strakes, a dark wale near the top.
        if j == 1 || j + 2 == seg {
            plain(dark)
        } else if lod != Lod::Far && j % 2 == 0 {
            plain(wood2)
        } else {
            plain(wood)
        }
    });
    // The deck, and its inside face (seen from the roofs below it is the
    // hull; from above, planks).
    let deck: Vec<Vec<DVec3>> = hull
        .iter()
        .map(|r| vec![r[0], *r.last().expect("a section")])
        .collect();
    b.grid(&deck, false, true, |_, _| plain(wood));
    if lod == Lod::Far {
        return;
    }
    // A brass rail along both gunwales.
    for side in [0usize, seg] {
        let path: Vec<DVec3> = hull.iter().map(|r| r[side] + DVec3::Y * 0.9).collect();
        tube(b, &spline(&path, 2, false), (0.05, 0.05), 6, metal(brass));
        for (k, r) in hull.iter().enumerate().step_by(2) {
            let p = r[side];
            tube(b, &[p, p + DVec3::Y * 0.9], (0.035, 0.035), 5, metal(brass));
            let _ = k;
        }
    }
    // The wheelhouse forward: a rounded cabin with big windows.
    let cab_z = 3.5;
    let cab: Vec<Vec<DVec3>> = (0..=8)
        .map(|i| {
            let u = i as f64 / 8.0;
            let z = cab_z + 2.5 - u * 5.0;
            let w = 1.5 * (1.0 - (2.0 * u - 1.0).powi(4) * 0.3);
            (0..=10)
                .map(|j| {
                    let a = PI * j as f64 / 10.0;
                    DVec3::new(-w * a.cos(), deck_y + 2.1 * a.sin().powf(0.4), z)
                })
                .collect()
        })
        .collect();
    surface(b, &cab, false, |i, j| {
        if (3..7).contains(&j) || j == 0 || j == 9 {
            if (1..7).contains(&i) && (j == 1 || j == 8 || (4..6).contains(&j)) {
                return Some((Surf::Glass, rgb(70, 110, 130).with_alpha(0.35)));
            }
            plain(wood2)
        } else if (1..7).contains(&i) && (j == 1 || j == 2 || j == 7 || j == 8) {
            Some((Surf::Glass, rgb(70, 110, 130).with_alpha(0.35)))
        } else {
            plain(rgb(210, 196, 160))
        }
    });
    // A lamp in the wheelhouse.
    b.surf = Surf::Glow;
    b.oriented_box(
        DVec3::new(0.0, deck_y + 1.7, cab_z),
        DVec3::X,
        DVec3::new(0.12, 0.12, 0.12),
        rgb(255, 210, 140),
    );
    b.surf = Surf::Plain;
    // Cables from the gunwales up to the envelope.
    for z in [-6.0, -1.0, 4.0, 8.0] {
        for x in [-1.0, 1.0] {
            let top_t = (0.5 - z / AIRSHIP_LENGTH).clamp(0.0, 1.0);
            let r = envelope_r(top_t);
            let top = DVec3::new(x * r * 0.45, -r * 0.89, z);
            let foot = DVec3::new(x * 1.6, deck_y + 0.9, z);
            tube(b, &[foot, top], (0.03, 0.03), 4, plain(dark));
        }
    }
    // Engines on outriggers, brass with dark cowls, propellers astern.
    let prop_n = lod.pick(3, 3, 2);
    for x in [-1.0, 1.0] {
        let c = DVec3::new(x * 4.6, deck_y + 0.8, -2.0);
        tube(
            b,
            &[DVec3::new(x * 1.8, deck_y + 0.6, -2.0), c],
            (0.08, 0.08),
            6,
            metal(dark),
        );
        lathe(
            b,
            c + DVec3::Z * 1.4,
            -DVec3::Z,
            &[
                (0.0, 0.0),
                (0.35, 0.15),
                (0.6, 0.6),
                (0.65, 1.4),
                (0.55, 2.4),
                (0.25, 2.9),
                (0.0, 3.0),
            ],
            lod.pick(20, 12, 8),
            |k| if k == 2 { metal(dark) } else { metal(brass) },
        );
        for p in 0..prop_n {
            let a = TAU * p as f64 / prop_n as f64 + x;
            let d = DVec3::new(a.cos(), a.sin(), 0.0);
            let hub = c + DVec3::Z * -1.65;
            let blade: Vec<DVec3> = (0..5).map(|s| hub + d * (0.15 + 0.32 * s as f64)).collect();
            tube(b, &blade, (0.14, 0.025), 6, plain(rgb(110, 70, 36)));
        }
    }
    // The mast astern with the black flag: crossed bones and a skull.
    let mast = DVec3::new(0.0, deck_y, -len * 0.5 + 1.5);
    tube(
        b,
        &[mast, mast + DVec3::Y * 4.2],
        (0.07, 0.07),
        6,
        plain(wood2),
    );
    let fl = mast + DVec3::Y * 4.0;
    let flag: Vec<Vec<DVec3>> = (0..=6)
        .map(|i| {
            let u = i as f64 / 6.0;
            let wave = (u * 5.0).sin() * 0.18 * u;
            vec![
                fl + DVec3::new(wave, 0.0, -u * 2.6),
                fl + DVec3::new(wave, -1.6, -u * 2.6),
            ]
        })
        .collect();
    b.grid(&flag, false, true, |_, _| plain(rgb(20, 20, 22)));
    inside(|| b.grid(&flag, false, true, |_, _| plain(rgb(20, 20, 22))));
    let mid = fl + DVec3::new(0.0, -0.85, -1.3);
    for side in [0.06, -0.06] {
        let off = DVec3::X * side;
        b.surf = Surf::Plain;
        b.oriented_box(
            mid + off - DVec3::Y * 0.15,
            DVec3::new(0.0, 0.6, 0.8).normalize(),
            DVec3::new(0.02, 0.05, 0.45),
            rgb(236, 232, 220),
        );
        b.oriented_box(
            mid + off - DVec3::Y * 0.15,
            DVec3::new(0.0, -0.6, 0.8).normalize(),
            DVec3::new(0.02, 0.05, 0.45),
            rgb(236, 232, 220),
        );
        let skull = mid + off + DVec3::Y * 0.22;
        lathe(
            b,
            skull - DVec3::X * 0.02 * side.signum(),
            DVec3::X * side.signum(),
            &[(0.0, 0.0), (0.2, 0.0), (0.2, 0.01), (0.0, 0.02)],
            14,
            |_| plain(rgb(236, 232, 220)),
        );
    }
}

/// A rope ladder from `top` down to `foot`: two ropes and wooden rungs.
pub fn rope_ladder(top: DVec3, foot: DVec3) -> Mesh {
    let mut b = MeshBuilder::new();
    let rope = rgb(170, 140, 96);
    let rung = rgb(110, 72, 40);
    let down = foot - top;
    let side = DVec3::new(down.z, 0.0, -down.x).normalize_or(DVec3::X) * 0.25;
    let side = if side.length_squared() < 1e-6 {
        DVec3::X * 0.25
    } else {
        side
    };
    // Sagging a little, as a rope does.
    let sag = |s: f64| top + down * s + DVec3::new(0.3, 0.0, 0.2) * (PI * s).sin();
    for k in [-1.0, 1.0] {
        let path: Vec<DVec3> = (0..=10).map(|i| sag(i as f64 / 10.0) + side * k).collect();
        tube(&mut b, &path, (0.025, 0.025), 5, plain(rope));
    }
    let n = (down.length() / 0.4) as usize;
    for i in 1..n {
        let p = sag(i as f64 / n as f64);
        tube(
            &mut b,
            &[p - side, p + side],
            (0.022, 0.022),
            5,
            plain(rung),
        );
    }
    b.finish()
}

/// The airship in three levels: close, down the street, across the town.
pub fn airship_levels() -> Vec<LodLevel> {
    vec![
        LodLevel::new(airship(Lod::Near), 90.0),
        LodLevel::new(airship(Lod::Mid), 400.0),
        LodLevel::new(airship(Lod::Far), 4000.0),
    ]
}

/// The sky pirates' airship moored over the harbour, a rope ladder down to
/// their lookouts on the quay (see `hw_sim::roosts`). Returns its LOD set
/// and the ladder.
pub fn spawn_roost(
    world: &mut World,
    town: &hw_gen::urban::world::TownWorld,
) -> EngineResult<(Vec<LodId>, Vec<EntityId>)> {
    let Some(r) = hw_sim::roosts::pirate_roost(town) else {
        return Ok((Vec::new(), Vec::new()));
    };
    let ship = r.ship();
    let yaw = r.ship_yaw();
    let place = Place::new(ship.x as f32, ship.y as f32, ship.z as f32)
        .with_yaw_deg(yaw.to_degrees() as f32);
    let lod = world.spawn_lod_at(airship_levels(), &[place])?;
    // From the gondola's rail on the quay side down to the quay's edge.
    let side = DVec3::new(-r.out.x, 0.0, -r.out.y);
    let top = ship - DVec3::Y * GONDOLA_DECK + side * 1.9;
    let foot = DVec3::new(r.quay.x, r.top + 0.05, r.quay.y) + side * 0.6;
    let mesh = rope_ladder(DVec3::ZERO, foot - top);
    let ladder = world.place(mesh, Place::new(top.x as f32, top.y as f32, top.z as f32))?;
    Ok((vec![lod], vec![ladder]))
}

// ----------------------------------------------------------- iron capsule

/// The Iron-Masked Empire's drop capsule, crashed nose-down and tilted: a
/// riveted iron shell with brass bands, stabiliser fins, the open hatch
/// glowing red inside, and the Empire's iron mask on its flank. Origin at
/// the ground where it struck, the hatch towards +z.
pub fn iron_capsule(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let seg = lod.pick(40, 20, 10);
    let iron = rgb(64, 66, 72);
    let band = rgb(168, 132, 70);
    let red = rgb(255, 50, 30);
    let tilt = DVec3::new(0.12, 1.0, -0.18).normalize();
    // Half a metre of it is in the ground.
    let base = -tilt * 0.6;
    let profile = [
        (0.0, 0.0),
        (0.7, 0.25),
        (1.25, 0.8),
        (1.5, 1.4),
        (1.52, 1.5),
        (1.6, 1.55),
        (1.6, 1.7),
        (1.52, 1.75),
        (1.5, 3.2),
        (1.58, 3.25),
        (1.58, 3.4),
        (1.48, 3.45),
        (1.25, 4.0),
        (0.75, 4.45),
        (0.3, 4.62),
        (0.0, 4.65),
    ];
    lathe(&mut b, base, tilt, &profile, seg, |k| match k {
        4..=6 | 8..=10 => metal(band),
        _ => metal(iron),
    });
    if lod != Lod::Far {
        // Rivets round the bands.
        let (u, v) = {
            let helper = DVec3::X;
            let u = tilt.cross(helper).normalize();
            (u, tilt.cross(u).normalize())
        };
        let n = lod.pick(28, 14, 0);
        for h in [1.62, 3.32] {
            for k in 0..n {
                let a = TAU * k as f64 / n as f64;
                let p = base + tilt * h + (u * a.cos() + v * a.sin()) * 1.6;
                b.surf = Surf::Metal;
                b.oriented_box(
                    p,
                    u * a.cos() + v * a.sin(),
                    DVec3::new(0.04, 0.04, 0.04),
                    rgb(120, 96, 52),
                );
            }
        }
        b.surf = Surf::Plain;
        // Four stabiliser fins near the top (it fell tail-up).
        for k in 0..4 {
            let a = TAU * k as f64 / 4.0 + PI * 0.25;
            let out = u * a.cos() + v * a.sin();
            let side = tilt.cross(out).normalize();
            let fin: Vec<Vec<DVec3>> = [(1.45, 3.0, 4.3, 0.1), (2.4, 3.6, 4.45, 0.04)]
                .iter()
                .map(|&(r, lo, hi, th)| {
                    vec![
                        base + tilt * lo + out * r + side * th,
                        base + tilt * hi + out * r + side * th,
                        base + tilt * hi + out * r - side * th,
                        base + tilt * lo + out * r - side * th,
                    ]
                })
                .collect();
            surface(&mut b, &fin, true, |_, _| metal(iron));
        }
        // The Empire's mask on its flank (+x side): an iron face with
        // glowing slits for eyes.
        let face_n = (u * 0.0 + DVec3::X - tilt * DVec3::X.dot(tilt)).normalize();
        let face_up = tilt;
        let face_side = face_up.cross(face_n).normalize();
        let centre = base + tilt * 2.45 + face_n * 1.5;
        let mask: Vec<Vec<DVec3>> = (0..=6)
            .map(|i| {
                let y = -0.6 + 1.2 * i as f64 / 6.0;
                let w = 0.5 * (1.0 - (y / 0.62).powi(2)).max(0.0).sqrt() + 0.02;
                (0..=6)
                    .map(|j| {
                        let x = -w + 2.0 * w * j as f64 / 6.0;
                        let bulge = 0.08 * (1.0 - (x / 0.55).powi(2)) * (1.0 - (y / 0.7).powi(2));
                        centre + face_side * x + face_up * y + face_n * (0.04 + bulge)
                    })
                    .collect()
            })
            .collect();
        b.grid(&mask, false, false, |_, _| metal(rgb(150, 152, 160)));
        b.surf = Surf::Glow;
        for k in [-1.0, 1.0] {
            b.oriented_box(
                centre + face_side * (0.2 * k) + face_up * 0.12 + face_n * 0.14,
                face_side,
                DVec3::new(0.13, 0.03, 0.03),
                red,
            );
        }
        b.surf = Surf::Plain;
    }
    // The hatch, blown open: a dark doorway glowing red inside, the door
    // hanging from its lower hinge.
    let (u, v) = {
        let u = tilt.cross(DVec3::X).normalize();
        (u, tilt.cross(u).normalize())
    };
    let front = -v;
    let _ = u;
    let door_c = base + tilt * 2.4 + front * 1.52;
    let side = tilt.cross(front).normalize();
    b.surf = Surf::Glow;
    b.oriented_box(
        door_c - front * 0.04,
        side,
        DVec3::new(0.55, 0.85, 0.02),
        rgb(150, 24, 16),
    );
    b.surf = Surf::Metal;
    let hinge = door_c - tilt * 0.85 + front * 0.05;
    let open = (front * 0.85 - tilt * 0.5).normalize();
    b.oriented_box(
        hinge + open * 0.85,
        side,
        DVec3::new(0.58, 0.04, 0.85),
        rgb(70, 72, 78),
    );
    b.surf = Surf::Plain;
    b.finish()
}

pub fn iron_capsule_levels() -> Vec<LodLevel> {
    vec![
        LodLevel::new(iron_capsule(Lod::Near), 60.0),
        LodLevel::new(iron_capsule(Lod::Mid), 220.0),
        LodLevel::new(iron_capsule(Lod::Far), 1200.0),
    ]
}

// --------------------------------------------------------------- mole hole

/// Broken street round a hole: slabs of asphalt heaved up and tilted
/// outward, a ring of dug earth, the dark shaft going down, timber props
/// and a ladder the mole folk climbed up by. Origin at the hole's middle
/// on the street; `scorch` blackens the slabs (a capsule's crater).
pub fn broken_ground(lod: Lod, radius: f64, scorch: bool, ladder: bool) -> Mesh {
    let mut b = MeshBuilder::new();
    let asphalt = if scorch {
        rgb(50, 47, 45)
    } else {
        rgb(58, 58, 60)
    };
    let under = rgb(110, 100, 86);
    let earth = rgb(92, 66, 42);
    let earth2 = rgb(74, 52, 34);
    let seg = lod.pick(36, 18, 10);
    // The heap of dug earth round the rim.
    let r = radius;
    let rim: Vec<(f64, f64)> = vec![
        (r * 1.75, -0.05),
        (r * 1.5, 0.22),
        (r * 1.25, 0.5),
        (r * 1.08, 0.55),
        (r * 0.98, 0.32),
        (r * 0.95, 0.0),
    ];
    let rows: Vec<Vec<DVec3>> = rim
        .iter()
        .map(|&(rad, y)| {
            (0..seg)
                .map(|j| {
                    let a = TAU * j as f64 / seg as f64;
                    let wob = 1.0 + 0.08 * jitter(j as u32, 3);
                    DVec3::new(rad * wob * a.cos(), y * wob, rad * wob * a.sin())
                })
                .collect()
        })
        .collect();
    surface(&mut b, &rows, true, |i, j| {
        if lod != Lod::Far && (i + j) % 5 == 0 {
            plain(earth2)
        } else {
            plain(earth)
        }
    });
    // The shaft: its wall seen from inside, its floor lost in the dark.
    let shaft: Vec<(f64, f64)> = vec![(r * 0.95, 0.0), (r * 0.9, -1.2), (r * 0.85, -2.6)];
    let rows: Vec<Vec<DVec3>> = shaft
        .iter()
        .map(|&(rad, y)| {
            (0..seg)
                .map(|j| {
                    let a = TAU * j as f64 / seg as f64;
                    DVec3::new(rad * a.cos(), y, rad * a.sin())
                })
                .collect()
        })
        .collect();
    inside(|| {
        surface(&mut b, &rows, true, |i, _| {
            plain(if i == 0 { earth } else { earth2 })
        })
    });
    let floor: Vec<DVec3> = (0..seg)
        .map(|j| {
            let a = TAU * j as f64 / seg as f64;
            DVec3::new(r * 0.85 * a.cos(), -2.6, r * 0.85 * a.sin())
        })
        .collect();
    b.polygon(&floor, DVec3::Y, rgb(12, 10, 9));
    if lod == Lod::Far {
        return b.finish();
    }
    // Broken slabs of road heaved up round it, each tipped outward: jagged
    // plates a hand thick, grey on top, pale where they broke.
    let n = lod.pick(16, 9, 0);
    for k in 0..n {
        let a = TAU * (k as f64 + 0.4 * jitter(k, 1)) / n as f64;
        let out = DVec3::new(a.cos(), 0.0, a.sin());
        let along = DVec3::new(-a.sin(), 0.0, a.cos());
        let tip = (0.25 + 0.3 * jitter(k, 2).abs()) * if k % 4 == 0 { 1.6 } else { 1.0 };
        let up = (DVec3::Y * tip.cos() + out * tip.sin()).normalize();
        let face_out = up.cross(along).normalize();
        let size = 0.4 + 0.25 * jitter(k, 4).abs();
        let c = out * (r * 1.1 + size * 0.6) + DVec3::Y * (0.12 + 0.18 * jitter(k, 6).abs());
        let corners: Vec<DVec3> = (0..6)
            .map(|i| {
                let t = TAU * (i as f64 + 0.3 * jitter(k * 7 + i, 9)) / 6.0;
                let rr = size * (0.7 + 0.3 * jitter(k * 7 + i, 10).abs());
                c + along * (rr * 1.3 * t.cos()) + face_out * (rr * t.sin())
            })
            .collect();
        let th = up * 0.1;
        let top: Vec<DVec3> = corners.iter().map(|p| *p + th).collect();
        b.polygon(&top, up, asphalt);
        b.polygon(&corners, -up, under);
        for i in 0..6 {
            let j = (i + 1) % 6;
            let mid = (corners[i] + corners[j]) * 0.5 - c;
            b.quad([corners[i], corners[j], top[j], top[i]], mid.normalize(), under);
        }
    }
    // Timber props and a ladder for the mole folk.
    let timber = rgb(112, 80, 48);
    for k in 0..3 {
        let a = TAU * k as f64 / 3.0 + 0.4;
        let out = DVec3::new(a.cos(), 0.0, a.sin());
        let foot = out * r * 0.85 + DVec3::Y * -1.6;
        let top = out * r * 1.05 + DVec3::Y * 0.4;
        tube(&mut b, &[foot, top], (0.08, 0.08), 6, plain(timber));
    }
    if ladder {
        let out = DVec3::new(0.0, 0.0, -1.0);
        let side = DVec3::X * 0.22;
        let foot = out * r * 0.5 + DVec3::Y * -2.5;
        let top = out * r * 0.95 + DVec3::Y * 1.2;
        for k in [-1.0, 1.0] {
            tube(
                &mut b,
                &[foot + side * k, top + side * k],
                (0.035, 0.035),
                6,
                plain(timber),
            );
        }
        for i in 1..10 {
            let p = foot + (top - foot) * (i as f64 / 10.0);
            tube(
                &mut b,
                &[p - side, p + side],
                (0.025, 0.025),
                5,
                plain(timber),
            );
        }
        // A miner's lamp hung on the ladder.
        let lamp = top - (top - foot) * 0.12 + side * 1.4;
        lathe(
            &mut b,
            lamp,
            DVec3::Y,
            &[
                (0.0, -0.1),
                (0.07, -0.08),
                (0.08, 0.06),
                (0.03, 0.12),
                (0.0, 0.13),
            ],
            12,
            |k| {
                if k == 1 {
                    glow(rgb(255, 200, 120))
                } else {
                    metal(rgb(150, 120, 60))
                }
            },
        );
    }
    b.finish()
}

pub fn broken_ground_levels(radius: f64, scorch: bool, ladder: bool) -> Vec<LodLevel> {
    vec![
        LodLevel::new(broken_ground(Lod::Near, radius, scorch, ladder), 50.0),
        LodLevel::new(broken_ground(Lod::Mid, radius, scorch, ladder), 160.0),
        LodLevel::new(broken_ground(Lod::Far, radius, scorch, ladder), 500.0),
    ]
}

// ----------------------------------------------------------------- weapons

/// What a fighter carries in the right hand, the grip about the origin
/// and the weapon running up +y (the way a fist holds a stick), the edge
/// or point forward along +z.
pub fn weapon(kind: hw_net::EntityKind) -> Option<Mesh> {
    use hw_net::EntityKind as K;
    let mut b = MeshBuilder::new();
    match kind {
        K::Delver => pick(&mut b),
        K::Buccaneer => cutlass(&mut b, false),
        K::Captain => cutlass(&mut b, true),
        K::Praetorian => lance(&mut b),
        _ => return None,
    }
    Some(b.finish())
}

/// A miner's pick: an ash handle and a curved iron head.
fn pick(b: &mut MeshBuilder) {
    let ash = rgb(150, 112, 70);
    let iron = rgb(80, 82, 88);
    lathe(
        b,
        DVec3::Y * -0.18,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.024, 0.0),
            (0.02, 0.4),
            (0.024, 0.8),
            (0.03, 0.86),
            (0.0, 0.87),
        ],
        12,
        |_| plain(ash),
    );
    let head: Vec<DVec3> = (0..=8)
        .map(|i| {
            let u = i as f64 / 8.0 * 2.0 - 1.0;
            DVec3::new(0.0, 0.66 - 0.07 * u * u, u * 0.3)
        })
        .collect();
    tube(b, &head, (0.03, 0.025), 8, metal(iron));
    // Points.
    for (z, dir) in [(0.3, 1.0), (-0.3, -1.0)] {
        lathe(
            b,
            DVec3::new(0.0, 0.59, z),
            DVec3::new(0.0, -0.3, dir).normalize(),
            &[(0.03, 0.0), (0.0, 0.1)],
            8,
            |_| metal(iron),
        );
    }
}

/// A pirate's cutlass: a leather grip, a brass basket guard, a broad
/// curved blade. The captain's is longer, with a gilded guard.
fn cutlass(b: &mut MeshBuilder, captain: bool) {
    let steel = rgb(206, 210, 218);
    let brass = if captain {
        rgb(222, 184, 80)
    } else {
        rgb(176, 140, 70)
    };
    lathe(
        b,
        DVec3::Y * -0.06,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.03, 0.0),
            (0.022, 0.03),
            (0.022, 0.13),
            (0.03, 0.15),
            (0.0, 0.16),
        ],
        10,
        |k| {
            if k == 0 || k == 4 {
                metal(brass)
            } else {
                plain(rgb(60, 36, 22))
            }
        },
    );
    // The basket guard: a shell round the knuckles.
    let guard: Vec<Vec<DVec3>> = (0..=6)
        .map(|i| {
            let a = PI * (0.1 + 0.8 * i as f64 / 6.0);
            (0..=6)
                .map(|j| {
                    let y = -0.06 + 0.2 * j as f64 / 6.0;
                    let r = 0.07 + 0.02 * (PI * j as f64 / 6.0).sin();
                    DVec3::new(-r * a.sin() * 0.6, y, r * a.cos() + 0.04)
                })
                .collect()
        })
        .collect();
    b.grid(&guard, false, true, |_, _| metal(brass));
    inside(|| b.grid(&guard, false, true, |_, _| metal(brass)));
    let len = if captain { 0.82 } else { 0.7 };
    let blade: Vec<DVec3> = (0..=10)
        .map(|i| {
            let u = i as f64 / 10.0;
            DVec3::new(0.0, 0.14 + u * len, 0.03 + 0.12 * u * u)
        })
        .collect();
    // Broad near the point, as a cutlass is: a flat ellipse, wide along z.
    let rows: Vec<Vec<DVec3>> = blade
        .iter()
        .enumerate()
        .map(|(i, &p)| {
            let u = i as f64 / 10.0;
            let w = if i == 10 { 0.002 } else { 0.018 + 0.01 * u };
            let t = 0.004;
            (0..8)
                .map(|j| {
                    let a = TAU * j as f64 / 8.0;
                    p + DVec3::new(t * a.sin(), 0.0, w * a.cos())
                })
                .collect()
        })
        .collect();
    surface(b, &rows, true, |_, _| metal(steel));
}

/// A praetorian's shock lance: an iron shaft, a brass collar, a forked
/// head crackling blue-white.
fn lance(b: &mut MeshBuilder) {
    let iron = rgb(70, 72, 80);
    let brass = rgb(176, 140, 70);
    let arc = rgb(150, 210, 255);
    lathe(
        b,
        DVec3::Y * -0.7,
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.035, 0.0),
            (0.03, 0.05),
            (0.025, 1.9),
            (0.045, 1.95),
            (0.05, 2.1),
            (0.03, 2.15),
            (0.0, 2.16),
        ],
        12,
        |k| {
            if (4..6).contains(&k) {
                metal(brass)
            } else {
                metal(iron)
            }
        },
    );
    for k in [-1.0, 1.0] {
        let path = [
            DVec3::new(0.0, 1.42, 0.0),
            DVec3::new(0.0, 1.55, 0.06 * k),
            DVec3::new(0.0, 1.75, 0.07 * k),
            DVec3::new(0.0, 1.85, 0.03 * k),
        ];
        tube(b, &path, (0.016, 0.016), 6, metal(iron));
    }
    lathe(
        b,
        DVec3::Y * 1.5,
        DVec3::Y,
        &[(0.0, 0.0), (0.03, 0.05), (0.022, 0.3), (0.0, 0.42)],
        10,
        |_| glow(arc),
    );
}

/// Where the hand bone holds a weapon (see [`weapon`]).
pub fn weapon_grip() -> Place {
    Place::new(0.0, 0.08, 0.03)
}

/// Unit vector in the ground plane from a yaw (radians, 0 = +z).
pub fn flat_dir(yaw: f64) -> DVec2 {
    DVec2::new(yaw.sin(), yaw.cos())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_new_things_are_built_at_full_quality() {
        let tris = |m: Mesh| m.build().triangle_count();
        let near = tris(airship(Lod::Near));
        let mid = tris(airship(Lod::Mid));
        let far = tris(airship(Lod::Far));
        assert!((5_000..40_000).contains(&near), "airship near {near}");
        assert!(mid * 2 < near, "airship mid {mid} vs near {near}");
        assert!(far < 2_000, "airship far {far}");
        let capsule = tris(iron_capsule(Lod::Near));
        assert!((1_500..15_000).contains(&capsule), "capsule {capsule}");
        let hole = tris(broken_ground(Lod::Near, 1.6, false, true));
        assert!((600..10_000).contains(&hole), "hole {hole}");
        for k in [
            hw_net::EntityKind::Delver,
            hw_net::EntityKind::Buccaneer,
            hw_net::EntityKind::Captain,
            hw_net::EntityKind::Praetorian,
        ] {
            let w = weapon(k).expect("a weapon");
            assert!(tris(w) > 100, "{k:?}");
        }
        assert!(weapon(hw_net::EntityKind::Gunner).is_none());
    }
}
