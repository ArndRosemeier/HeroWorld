//! The meshes of the mad scientist's quarter
//! ([`hw_gen::urban::menace`]): what is built into his buildings
//! (smokestacks with a red warning band, searchlight pedestals, pipes
//! glowing with Aether, loudspeaker brackets, the Tesla coils on the labs'
//! domes), and the models that move ([`crate::menaceview`]): the
//! searchlights' heads and beams, the coils' arcs and the patrol drones.
//!
//! Everything round is lathed or swept with shared normals, so it shades
//! smooth (the AGENTS.md quality bar).

use crate::atomview::torus;
use crate::buildingmesh::Detail;
use crate::carmesh::{lathe, tube};
use crate::citymesh::{MeshBuilder, Surf};
use engine::prelude::*;
use glam::DVec3;
use hw_gen::building::BuildingSite;
use hw_gen::urban::menace::{Fixtures, Pipe, Searchlight, Speakers, Stack, PIPE_OUT, PIPE_R};
use std::f64::consts::{FRAC_PI_2, PI};

/// The Doctor's Aether, glowing green.
pub const AETHER: (u8, u8, u8) = (120, 255, 170);
/// Warning red: stack bands, the drones' eyes, the speakers' lamps.
pub const WARNING: (u8, u8, u8) = (255, 40, 28);
/// A searchlight's beam.
pub const BEAM: (u8, u8, u8) = (196, 255, 214);
/// The searchlight's lamp pivot over the pedestal's foot (see
/// [`Searchlight::head`]).
const HEAD_UP: f64 = 2.3;
/// How long a searchlight's beam is (m): the instance is scaled to it.
pub const BEAM_LEN: f64 = 320.0;
/// How wide the beam opens (half angle, degrees).
const BEAM_HALF: f64 = 2.2;

pub fn c3(c: (u8, u8, u8)) -> Color {
    rgb(c.0, c.1, c.2)
}

/// Building coordinates for one building.
struct Frame<'a> {
    site: &'a BuildingSite,
    base: f64,
}

impl Frame<'_> {
    fn p(&self, q: DVec3) -> DVec3 {
        Fixtures::to_world(self.site, self.base, q)
    }

    fn dir(&self, d: DVec3) -> DVec3 {
        let (a, i) = (self.site.along, self.site.inward);
        DVec3::new(a.x, 0.0, a.y) * d.x + DVec3::Y * d.y + DVec3::new(i.x, 0.0, i.y) * d.z
    }
}

/// Round parts' sides, by detail.
fn sides(detail: Detail, near: usize) -> usize {
    match detail {
        Detail::Near => near,
        Detail::Far => (near / 2).max(8),
        Detail::Distant => (near / 4).max(6),
    }
}

/// The fixtures built into one building (`base`: where its ground floor
/// stands, as for the building itself). `roofed`: the roof is drawn (not
/// cut open), so what stands on it is too.
pub fn building(
    b: &mut MeshBuilder,
    site: &BuildingSite,
    base: f64,
    f: &Fixtures,
    roofed: bool,
    detail: Detail,
) {
    let fr = Frame { site, base };
    let (tex, up, weather) = (b.tex.take(), b.up_tex.take(), b.weather.take());
    if roofed {
        if let Some(s) = &f.stack {
            stack(b, &fr, s, detail);
        }
        if let Some(l) = &f.light {
            pedestal(b, &fr, l, detail);
        }
    }
    if detail != Detail::Distant {
        for p in &f.pipes {
            pipe(b, &fr, p, detail);
        }
    }
    if detail == Detail::Near {
        if let Some(s) = &f.speakers {
            speakers(b, &fr, s);
        }
    }
    b.surf = Surf::Plain;
    (b.tex, b.up_tex, b.weather) = (tex, up, weather);
}

/// A tall brick smokestack, tapering, on a concrete plinth, with iron bands,
/// an iron crown round its sooty mouth and a band of red warning lamps
/// under the crown that glows day and night.
fn stack(b: &mut MeshBuilder, fr: &Frame, s: &Stack, detail: Detail) {
    let seg = sides(detail, 36);
    let h = s.height;
    let (r0, r1) = (s.r, s.top_r());
    let r_at = |t: f64| r0 + (r1 - r0) * (t / h).clamp(0.0, 1.0);
    let foot = fr.p(DVec3::new(s.at.x, s.foot, s.at.y));
    let brick = rgb(104, 56, 42);
    let concrete = rgb(122, 120, 112);
    let iron = rgb(74, 70, 66);
    let soot = rgb(42, 38, 35);
    // Profile from the plinth's foot (a little under the deck) up the
    // shaft, over the crown and down into the dark mouth.
    let prof: Vec<(f64, f64, Surf, Color)> = vec![
        (r0 + 0.45, -0.25, Surf::Plain, concrete),
        (r0 + 0.45, 1.1, Surf::Plain, concrete),
        (r0 + 0.02, 1.1, Surf::Plain, brick),
        (r_at(h * 0.5), h * 0.5, Surf::Plain, brick),
        (r_at(h - 1.0), h - 1.0, Surf::Metal, iron),
        (r1 + 0.14, h - 0.85, Surf::Metal, iron),
        (r1 + 0.14, h - 0.2, Surf::Metal, iron),
        (r1 + 0.06, h, Surf::Plain, soot),
        (r1 - 0.2, h, Surf::Plain, soot),
        (r1 - 0.2, h - 2.6, Surf::Plain, soot),
        (0.0, h - 2.6, Surf::Plain, soot),
    ];
    let pts: Vec<(f64, f64)> = prof.iter().map(|p| (p.0, p.1)).collect();
    b.tex = Some(engine::texgen::FacadeLayer::BrickRed.into());
    lathe(b, foot, DVec3::Y, &pts, seg, |i| {
        Some((prof[i].2, prof[i].3))
    });
    b.tex = None;
    // Iron bands round the shaft.
    if detail != Detail::Distant {
        for t in [h * 0.3, h * 0.62] {
            let r = r_at(t);
            let band = [
                (r - 0.02, t - 0.18),
                (r + 0.07, t - 0.14),
                (r + 0.07, t + 0.14),
                (r - 0.02, t + 0.18),
            ];
            lathe(b, foot, DVec3::Y, &band, seg, |_| Some((Surf::Metal, iron)));
        }
    }
    // The warning band: red lamps that mark it for the air traffic, and
    // mark the Doctor's ground for everyone else.
    let (t0, t1) = (h - 2.1, h - 1.75);
    let (ra, rb) = (r_at(t0), r_at(t1));
    let band = [
        (ra - 0.02, t0),
        (ra + 0.07, t0 + 0.05),
        (rb + 0.07, t1 - 0.05),
        (rb - 0.02, t1),
    ];
    lathe(b, foot, DVec3::Y, &band, seg, |_| {
        Some((Surf::Glow, c3(WARNING)))
    });
    // A ladder up the side facing the street (close up).
    if detail == Detail::Near {
        let out = -fr.dir(DVec3::Z);
        let side = fr.dir(DVec3::X);
        let rail = |b: &mut MeshBuilder, k: f64| {
            let path: Vec<DVec3> = [1.2, h * 0.5, h - 1.7]
                .iter()
                .map(|&t| foot + DVec3::Y * t + out * (r_at(t) + 0.22) + side * (0.25 * k))
                .collect();
            tube(b, &path, (0.03, 0.03), 6, Some((Surf::Metal, iron)));
        };
        rail(b, -1.0);
        rail(b, 1.0);
        let mut t = 1.6;
        while t < h - 1.8 {
            let c = foot + DVec3::Y * t + out * (r_at(t) + 0.22);
            tube(
                b,
                &[c - side * 0.25, c + side * 0.25],
                (0.022, 0.022),
                5,
                Some((Surf::Metal, iron)),
            );
            t += 0.6;
        }
    }
}

/// The searchlight's pedestal and turntable (the head turns: it is the
/// view's, see [`searchlight_levels`]).
fn pedestal(b: &mut MeshBuilder, fr: &Frame, l: &Searchlight, detail: Detail) {
    let seg = sides(detail, 28);
    let foot = fr.p(DVec3::new(l.at.x, l.foot, l.at.y));
    let steel = rgb(70, 74, 72);
    let up = l.head - l.foot;
    let prof = [
        (0.75, -0.2),
        (0.75, 0.25),
        (0.42, 0.45),
        (0.3, up - 1.0),
        (0.62, up - 0.85),
        (0.62, up - 0.7),
        (0.0, up - 0.7),
    ];
    lathe(b, foot, DVec3::Y, &prof, seg, |_| {
        Some((Surf::Metal, steel))
    });
}

/// A pipe of green Aether up the front: out of the wall, up, and back in,
/// in iron collars, with glowing glass between them.
fn pipe(b: &mut MeshBuilder, fr: &Frame, p: &Pipe, detail: Detail) {
    let seg = sides(detail, 14);
    let out = -PIPE_OUT;
    let bend = 0.35;
    let mut path = Vec::new();
    // The elbow out of the wall, turning up...
    for k in 0..=4 {
        let a = FRAC_PI_2 * k as f64 / 4.0;
        path.push(DVec3::new(
            p.x,
            p.from + bend - bend * a.cos(),
            out * a.sin(),
        ));
    }
    // ... and the one back in at the top.
    for k in 0..=4 {
        let a = FRAC_PI_2 * k as f64 / 4.0;
        path.push(DVec3::new(p.x, p.to - bend + bend * a.sin(), out * a.cos()));
    }
    // The bends start and end in the wall itself.
    path[0].z = 0.05;
    let n = path.len();
    path[n - 1].z = 0.05;
    let pts: Vec<DVec3> = path.iter().map(|q| fr.p(*q)).collect();
    tube(
        b,
        &pts,
        (PIPE_R, PIPE_R),
        seg,
        Some((Surf::Glow, c3(AETHER))),
    );
    // Collars and brackets every couple of metres.
    let iron = rgb(40, 42, 40);
    let mut y = p.from + bend + 0.4;
    while y < p.to - bend - 0.3 {
        let c = fr.p(DVec3::new(p.x, y, out));
        let axis = DVec3::Y;
        lathe(
            b,
            c - axis * 0.09,
            axis,
            &[
                (PIPE_R + 0.01, 0.0),
                (PIPE_R + 0.045, 0.02),
                (PIPE_R + 0.045, 0.16),
                (PIPE_R + 0.01, 0.18),
            ],
            seg,
            |_| Some((Surf::Metal, iron)),
        );
        if detail == Detail::Near {
            let w = fr.p(DVec3::new(p.x, y, 0.0));
            tube(
                b,
                &[w + (c - w) * 0.05, c + (c - w).normalize() * -PIPE_R],
                (0.03, 0.03),
                6,
                Some((Surf::Metal, iron)),
            );
        }
        y += 2.2;
    }
}

/// Three horn loudspeakers on an iron bracket over the street, and a red
/// lamp under them.
fn speakers(b: &mut MeshBuilder, fr: &Frame, s: &Speakers) {
    let iron = rgb(46, 48, 46);
    let horn = rgb(120, 126, 118);
    let reach = 1.2;
    let wall = fr.p(DVec3::new(s.x, s.y, 0.02));
    let tip = fr.p(DVec3::new(s.x + s.side * 0.35, s.y, -reach));
    // The arm out of the wall, and a strut under it.
    tube(b, &[wall, tip], (0.05, 0.05), 8, Some((Surf::Metal, iron)));
    let low = fr.p(DVec3::new(s.x, s.y - 0.9, 0.02));
    let mid = fr.p(DVec3::new(s.x + s.side * 0.2, s.y, -reach * 0.6));
    tube(b, &[low, mid], (0.035, 0.035), 8, Some((Surf::Metal, iron)));
    tube(
        b,
        &[wall - DVec3::Y * 0.12, wall + DVec3::Y * 0.12],
        (0.12, 0.12),
        10,
        Some((Surf::Metal, iron)),
    );
    // Horns: up the street, down the street and straight out.
    let aims = [
        DVec3::new(1.0, -0.25, -0.35),
        DVec3::new(-1.0, -0.25, -0.35),
        DVec3::new(0.0, -0.35, -1.0),
    ];
    for a in aims {
        let axis = fr.dir(a).normalize();
        let throat = tip - axis * 0.15;
        let prof = [
            (0.0, -0.12),
            (0.1, -0.12),
            (0.1, 0.05),
            (0.07, 0.1),
            (0.11, 0.35),
            (0.2, 0.55),
            (0.34, 0.68),
            (0.36, 0.7),
            (0.3, 0.66),
            (0.08, 0.36),
            (0.0, 0.34),
        ];
        lathe(b, throat, axis, &prof, 20, |i| {
            Some((Surf::Metal, if i >= 7 { iron } else { horn }))
        });
    }
    // The red lamp under the horns.
    sphere(b, tip - DVec3::Y * 0.3, 0.11, 12, (Surf::Glow, c3(WARNING)));
}

/// Where the red lamp under a building's loudspeakers hangs (for the
/// lamp the view lights there at night).
pub fn speaker_lamp(site: &BuildingSite, base: f64, s: &Speakers) -> DVec3 {
    Fixtures::to_world(site, base, DVec3::new(s.x + s.side * 0.35, s.y - 0.3, -1.2))
}

/// A ball.
fn sphere(b: &mut MeshBuilder, c: DVec3, r: f64, seg: usize, paint: (Surf, Color)) {
    let rings = (seg / 2).max(4);
    let prof: Vec<(f64, f64)> = (0..=rings)
        .map(|i| {
            let a = PI * i as f64 / rings as f64;
            (r * a.sin(), r * (1.0 - a.cos()))
        })
        .collect();
    lathe(b, c - DVec3::Y * r, DVec3::Y, &prof, seg, |_| Some(paint));
}

/// A lab's Tesla coil on its dome: a copper core wound with rings, under a
/// fat chrome top load and a glowing ball. `apex`: the dome's crown (city
/// coordinates); the top load sits [`COIL_LOAD`] over it.
pub fn lab_coil(b: &mut MeshBuilder, apex: DVec3, detail: Detail) {
    use hw_gen::urban::menace::COIL_LOAD;
    let seg = sides(detail, 32);
    let copper = rgb(186, 112, 62);
    let dark = rgb(120, 70, 40);
    let chrome = rgb(200, 204, 206);
    // The core, from a flared foot sunk into the dome.
    let core = [
        (0.0, -0.4),
        (0.55, -0.4),
        (0.55, 0.05),
        (0.3, 0.35),
        (0.24, COIL_LOAD - 0.3),
        (0.0, COIL_LOAD - 0.3),
    ];
    lathe(b, apex, DVec3::Y, &core, seg, |i| {
        Some((Surf::Metal, if i < 2 { dark } else { copper }))
    });
    // The winding: copper rings shrinking up the core.
    let (e1, e2) = (DVec3::X, DVec3::Z);
    let rings = if detail == Detail::Near { 5 } else { 3 };
    for k in 0..rings {
        let t = (k as f64 + 0.5) / rings as f64;
        let y = 0.6 + t * (COIL_LOAD - 1.6);
        let big = 1.05 - 0.45 * t;
        torus(
            b,
            apex + DVec3::Y * y,
            e1,
            e2,
            big,
            0.09,
            (seg, 8),
            (Surf::Metal, copper),
        );
    }
    // The top load: a fat chrome doughnut, and the ball on its needle.
    torus(
        b,
        apex + DVec3::Y * COIL_LOAD,
        e1,
        e2,
        0.85,
        0.36,
        (seg, if detail == Detail::Near { 16 } else { 8 }),
        (Surf::Metal, chrome),
    );
    let needle = [(0.07, COIL_LOAD - 0.3), (0.03, COIL_LOAD + 1.3)];
    lathe(b, apex, DVec3::Y, &needle, 8, |_| {
        Some((Surf::Metal, chrome))
    });
    sphere(
        b,
        apex + DVec3::Y * (COIL_LOAD + 1.75),
        0.5,
        seg.min(24),
        (Surf::Glow, c3(AETHER)),
    );
}

/// The turning part of a searchlight: a yoke and the drum, its glass
/// glowing. Origin at the pivot, the beam along +z.
fn searchlight_head(seg: usize, near: bool) -> Mesh {
    let mut b = MeshBuilder::new();
    let steel = rgb(70, 74, 72);
    let black = rgb(30, 32, 32);
    // The drum: a fat barrel with a hood at the back.
    let prof = [
        (0.0, -0.8),
        (0.35, -0.8),
        (0.58, -0.55),
        (0.62, -0.2),
        (0.62, 0.6),
        (0.7, 0.62),
        (0.7, 0.78),
        (0.6, 0.8),
    ];
    lathe(&mut b, DVec3::ZERO, DVec3::Z, &prof, seg, |i| {
        Some((Surf::Metal, if i >= 4 { black } else { steel }))
    });
    // The glass: a shallow lens, glowing.
    let lens = [(0.6, 0.79), (0.4, 0.83), (0.0, 0.85)];
    lathe(&mut b, DVec3::ZERO, DVec3::Z, &lens, seg, |_| {
        Some((Surf::Glow, c3(BEAM)))
    });
    // The yoke: two arms down to the turntable, with trunnions.
    for s in [-1.0, 1.0] {
        tube(
            &mut b,
            &[
                DVec3::new(s * 0.62, 0.0, 0.0),
                DVec3::new(s * 0.8, 0.0, 0.0),
                DVec3::new(s * 0.8, -0.75, 0.0),
                DVec3::new(s * 0.45, -(HEAD_UP - 1.4) - 0.0, 0.0),
            ],
            (0.07, 0.09),
            if near { 10 } else { 6 },
            Some((Surf::Metal, steel)),
        );
    }
    b.finish()
}

/// A searchlight's head, close up, middle and far.
/// (Meshes and their reach, to make [`LodLevel`]s of.)
pub fn searchlight_levels() -> Vec<(Mesh, f32)> {
    vec![
        (searchlight_head(32, true), 40.0),
        (searchlight_head(16, false), 220.0),
        (searchlight_head(8, false), 1500.0),
    ]
}

/// A beam of light: a long, slowly widening cone along +z from the
/// origin, bright at the lamp and fading to nothing at its end. One unit
/// long (the instance is scaled to [`BEAM_LEN`] or a drone's beam).
pub fn beam(colour: Color, half_deg: f64, start_r: f64, alpha: f32) -> Mesh {
    const SIDES: usize = 24;
    let spread = half_deg.to_radians().tan();
    let mut m = Mesh::new();
    let rows: [(f64, f32); 6] = [
        (0.0, 0.0),
        (0.004, 1.0),
        (0.1, 0.75),
        (0.35, 0.45),
        (0.7, 0.18),
        (1.0, 0.0),
    ];
    let mut rings: Vec<Vec<PointId>> = Vec::new();
    for &(t, k) in &rows {
        let r = start_r + spread * t;
        let ring = (0..SIDES)
            .map(|i| {
                let a = std::f32::consts::TAU * i as f32 / SIDES as f32;
                let out = Vec3::new(a.cos(), a.sin(), 0.0);
                let id = m
                    .add_point(out * r as f32 + Vec3::Z * t as f32)
                    .expect("finite point");
                m.set_point_color(
                    id,
                    Color {
                        a: (alpha * k).clamp(0.0, 1.0),
                        ..colour
                    },
                )
                .expect("point exists");
                m.set_point_normal(id, out).expect("point exists");
                id
            })
            .collect();
        rings.push(ring);
    }
    for w in rings.windows(2) {
        for i in 0..SIDES {
            let j = (i + 1) % SIDES;
            m.add_triangle(w[0][i], w[0][j], w[1][j]).expect("tri");
            m.add_triangle(w[0][i], w[1][j], w[1][i]).expect("tri");
        }
    }
    m.set_surface_material(SurfaceMaterial {
        roughness: 0.4,
        emission: 0.9,
        ..SurfaceMaterial::DEFAULT
    });
    m
}

/// A searchlight's beam (scale it by [`BEAM_LEN`]).
pub fn searchlight_beam() -> Mesh {
    beam(c3(BEAM), BEAM_HALF, 0.55 / BEAM_LEN, 0.2)
}

/// The patrol drone: a chrome saucer with a smoked glass dome, a red eye
/// in a hooded socket at the front, three tail fins, a whip antenna and a
/// green lamp underneath. About 3 m across. Origin at its middle, facing
/// +z. `seg`: sides of the round parts.
fn drone(seg: usize, near: bool) -> Mesh {
    let mut b = MeshBuilder::new();
    let chrome = rgb(196, 200, 204);
    let band = rgb(60, 66, 64);
    let glass = rgb(40, 60, 56);
    // The saucer: a lens, its rim a dark band.
    let n = if near { 10 } else { 5 };
    let mut prof: Vec<(f64, f64)> = vec![(0.0, -0.42)];
    for i in 1..=n {
        let a = FRAC_PI_2 * i as f64 / n as f64;
        prof.push((1.5 * a.sin(), -0.42 * a.cos()));
    }
    let rim = prof.len() - 1;
    prof.push((1.55, 0.0));
    prof.push((1.5, 0.1));
    let top = prof.len() - 1;
    for i in 1..=n {
        let a = FRAC_PI_2 * i as f64 / n as f64;
        prof.push((1.5 * a.cos().max(0.0) + 0.0, 0.1 + 0.28 * a.sin()));
    }
    let last = prof.len() - 1;
    prof[last].0 = 0.0;
    lathe(&mut b, DVec3::ZERO, DVec3::Y, &prof, seg, |i| {
        Some((Surf::Metal, if i >= rim && i < top { band } else { chrome }))
    });
    // The dome on top: smoked glass.
    let d = 0.75;
    let dome: Vec<(f64, f64)> = (0..=n)
        .map(|i| {
            let a = FRAC_PI_2 * i as f64 / n as f64;
            (d * a.cos(), 0.3 + 0.55 * a.sin())
        })
        .collect();
    lathe(&mut b, DVec3::ZERO, DVec3::Y, &dome, seg, |_| {
        Some((Surf::Glass, glass))
    });
    // The eye: a hooded socket at the front, the lens glowing red.
    let socket = [
        (0.0, -0.2),
        (0.32, -0.2),
        (0.36, 0.1),
        (0.4, 0.18),
        (0.3, 0.2),
    ];
    let eye_at = DVec3::new(0.0, -0.05, 1.32);
    lathe(&mut b, eye_at, DVec3::Z, &socket, seg / 2, |_| {
        Some((Surf::Metal, band))
    });
    sphere(
        &mut b,
        eye_at + DVec3::Z * 0.12,
        0.26,
        (seg / 2).max(8),
        (Surf::Glow, c3(WARNING)),
    );
    // Tail fins, swept back and up.
    for k in [-1.0, 0.0, 1.0] {
        let yaw: f64 = k * 0.55;
        let back = DVec3::new(yaw.sin(), 0.0, -yaw.cos());
        let side = DVec3::new(back.z, 0.0, -back.x);
        let a = back * 1.1 + DVec3::Y * 0.1;
        let c = back * 1.95 + DVec3::Y * 0.75;
        let pts = [
            a,
            back * 1.6 + DVec3::Y * 0.1,
            c,
            back * 1.5 + DVec3::Y * 0.7,
        ];
        b.hexa(
            pts.map(|p| p - side * 0.03),
            pts.map(|p| p + side * 0.03),
            chrome,
        );
        b.surf = Surf::Glow;
        b.oriented_box(c, back, DVec3::new(0.08, 0.05, 0.04), c3(WARNING));
        b.surf = Surf::Plain;
    }
    // The whip antenna with its blinking tip.
    if near {
        tube(
            &mut b,
            &[DVec3::new(0.3, 0.8, -0.2), DVec3::new(0.4, 1.9, -0.5)],
            (0.015, 0.015),
            6,
            Some((Surf::Metal, band)),
        );
        sphere(
            &mut b,
            DVec3::new(0.4, 1.95, -0.5),
            0.06,
            8,
            (Surf::Glow, c3(WARNING)),
        );
    }
    // The lamp underneath, glowing green.
    let lamp = [(0.0, -0.47), (0.42, -0.47), (0.45, -0.42)];
    lathe(&mut b, DVec3::ZERO, DVec3::Y, &lamp, seg, |_| {
        Some((Surf::Glow, c3(AETHER)))
    });
    b.finish()
}

/// The patrol drone, close up, middle and far.
/// (Meshes and their reach, to make [`LodLevel`]s of.)
pub fn drone_levels() -> Vec<(Mesh, f32)> {
    vec![
        (drone(48, true), 45.0),
        (drone(20, false), 260.0),
        (drone(10, false), 2500.0),
    ]
}

/// A drone's beam down onto the street (scale it by its length).
pub fn drone_beam() -> Mesh {
    beam(c3(AETHER), 11.0, 0.45 / 60.0, 0.3)
}

/// A crackling arc from a coil's top load down onto its dome: a jagged
/// glowing thread, the shape `k` of a few. In the coil's frame: origin
/// at the middle of the top load, the arc running out along +z; `r` the
/// dome's radius.
pub fn coil_arc(r: f64, k: u64) -> Mesh {
    use hw_gen::urban::menace::COIL_LOAD;
    let mut rng = hw_core::Seed(0xa7c0 + k).rng();
    let from = DVec3::new(0.0, 0.0, 1.1);
    // A point on the dome, out and down.
    let e = rng.range(25f64, 55.0).to_radians();
    let centre = DVec3::new(0.0, -(COIL_LOAD + r), 0.0);
    let to = centre + DVec3::new(0.0, r * e.sin(), r * e.cos());
    let mut pts = vec![from, to];
    let mut amp = from.distance(to) * 0.22;
    for _ in 0..4 {
        let mut next = Vec::with_capacity(pts.len() * 2);
        for w in pts.windows(2) {
            next.push(w[0]);
            let m = (w[0] + w[1]) * 0.5
                + DVec3::new(
                    rng.range(-amp, amp),
                    rng.range(-amp, amp) * 0.6,
                    rng.range(-amp, amp) * 0.5,
                );
            next.push(m);
        }
        next.push(*pts.last().expect("a point"));
        pts = next;
        amp *= 0.5;
    }
    let mut b = MeshBuilder::new();
    tube(
        &mut b,
        &pts,
        (0.04, 0.04),
        5,
        Some((Surf::Glow, rgb(220, 255, 235))),
    );
    // A fork off the middle.
    let mid = pts[pts.len() / 2];
    let end = mid
        + DVec3::new(
            rng.range(-0.8, 0.8),
            -rng.range(0.4, 1.2),
            rng.range(0.2, 0.9),
        );
    tube(
        &mut b,
        &[mid, (mid + end) * 0.5 + DVec3::X * 0.15, end],
        (0.025, 0.025),
        4,
        Some((Surf::Glow, rgb(200, 255, 220))),
    );
    let mut m = b.finish();
    m.set_surface_material(SurfaceMaterial {
        roughness: 0.4,
        emission: 3.0,
        ..SurfaceMaterial::DEFAULT
    });
    m
}

/// A dome radius rounded to the arcs' buckets (shapes are made per bucket).
pub fn arc_bucket(r: f64) -> u32 {
    (r * 2.0).round().max(1.0) as u32
}

/// The radius a bucket stands for.
pub fn bucket_r(k: u32) -> f64 {
    k as f64 * 0.5
}
