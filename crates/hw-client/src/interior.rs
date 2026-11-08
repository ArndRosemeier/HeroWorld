//! Furniture for the floor you are on: each room furnished for its use, in
//! the golden age's taste (streamlined sofas, chrome desks, radio cabinets,
//! glowing Aether lifts).
//!
//! Everything is placed in building coordinates (x along the front, z
//! inward, y up from the floor's top) and kept clear of the doorways.

use crate::citymesh::{MeshBuilder, Surf};
use crate::skins::Tex;
use engine::prelude::*;
use glam::{DVec2, DVec3};
use hw_gen::building::{BuildingPlan, Floor, Room, RoomKind};

/// Furniture keeps this far from a doorway's middle.
const DOOR_CLEAR: f64 = 1.3;

/// Places furniture in building space.
struct Furnisher<'a> {
    b: &'a mut MeshBuilder,
    plan: &'a BuildingPlan,
    /// City height of the floor's top.
    top: f64,
    u: DVec3,
    doors: Vec<DVec2>,
}

impl Furnisher<'_> {
    fn city(&self, p: DVec3) -> DVec3 {
        self.plan.site.to_city(DVec3::new(p.x, p.y + self.top, p.z))
    }

    /// An axis-aligned box from `lo` to `hi` (building space, y from the floor).
    /// Things standing on the floor have no bottom face: bottoms of things
    /// side by side (a sofa on a rug) would share one plane and fight.
    fn bx(&mut self, lo: DVec3, hi: DVec3, c: Color) {
        let (lo, hi) = (lo.min(hi), lo.max(hi));
        if lo.y > 1e-9 {
            let centre = self.city((lo + hi) * 0.5);
            self.b.oriented_box(centre, self.u, (hi - lo) * 0.5, c);
            return;
        }
        let p = |x: f64, y: f64, z: f64| self.city(DVec3::new(x, y, z));
        let d = |v: DVec3| self.city(v) - self.city(DVec3::ZERO);
        let (x0, x1, y0, y1, z0, z1) = (lo.x, hi.x, lo.y, hi.y, lo.z, hi.z);
        let faces = [
            (
                [p(x0, y1, z0), p(x1, y1, z0), p(x1, y1, z1), p(x0, y1, z1)],
                DVec3::Y,
            ),
            (
                [p(x0, y0, z0), p(x1, y0, z0), p(x1, y1, z0), p(x0, y1, z0)],
                -DVec3::Z,
            ),
            (
                [p(x0, y0, z1), p(x1, y0, z1), p(x1, y1, z1), p(x0, y1, z1)],
                DVec3::Z,
            ),
            (
                [p(x0, y0, z0), p(x0, y0, z1), p(x0, y1, z1), p(x0, y1, z0)],
                -DVec3::X,
            ),
            (
                [p(x1, y0, z0), p(x1, y0, z1), p(x1, y1, z1), p(x1, y1, z0)],
                DVec3::X,
            ),
        ];
        let faces = faces.map(|(q, n)| (q, d(n)));
        for (q, n) in faces {
            self.b.quad(q, n, c);
        }
    }

    fn cyl(&mut self, at: DVec3, r: (f64, f64), h: f64, seg: usize, c: Color) {
        let p = self.city(at);
        if at.y > 1e-9 {
            self.b.cylinder(p, r, h, seg, true, c);
            return;
        }
        // On the floor: sides and top only.
        self.b.cylinder(p, r, h, seg, false, c);
        let top: Vec<DVec3> = (0..seg)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / seg as f64;
                p + DVec3::new(r.1 * a.cos(), h, r.1 * a.sin())
            })
            .collect();
        self.b.polygon(&top, DVec3::Y, c);
    }

    fn surf(&mut self, s: Surf, f: impl FnOnce(&mut Self)) {
        self.b.surf = s;
        f(self);
        self.b.surf = Surf::Plain;
    }

    /// Is a footprint (centre, half size) clear of the doorways?
    fn clear(&self, c: DVec2, half: DVec2) -> bool {
        self.doors.iter().all(|d| {
            let q = (*d - c).abs() - half;
            q.max(DVec2::ZERO).length() > DOOR_CLEAR
        })
    }
}

/// Furnishes every room of floor `level` of `plan`; `top` is the city height
/// of the floor's top surface, `u` the building's x axis in the city.
pub fn furnish(b: &mut MeshBuilder, plan: &BuildingPlan, level: usize, top: f64, u: DVec3) {
    let Some(f) = plan.floors.get(level) else {
        return;
    };
    b.tex_of = Some(furniture_tex);
    let mut fu = Furnisher {
        b,
        plan,
        top,
        u,
        doors: f.doors.iter().map(|d| d.at).collect(),
    };
    for (k, room) in f.rooms.iter().enumerate() {
        let seed = hash(plan.site.origin, level, k);
        match room.kind {
            RoomKind::Lobby => lobby(&mut fu, room, seed),
            RoomKind::Office => office(&mut fu, room, seed),
            RoomKind::Apartment | RoomKind::Living => living(&mut fu, room, seed),
            RoomKind::Kitchen => kitchen(&mut fu, room, seed),
            RoomKind::Bedroom => bedroom(&mut fu, room, seed),
            RoomKind::Shop => shop(&mut fu, room, seed),
            RoomKind::Storage => storage(&mut fu, room, seed),
            RoomKind::Gallery | RoomKind::Hall => gallery(&mut fu, room, seed),
            RoomKind::Lab => lab(&mut fu, room, seed),
            RoomKind::Core => core(&mut fu, room, f),
            RoomKind::Corridor => {}
        }
    }
    fu.b.tex_of = None;
}

/// Furniture's texture by its colour: wood is oak, upholstery cloth,
/// the rest painted.
fn furniture_tex(c: Color) -> Option<Tex> {
    use crate::skins::{Base, Skin, Wear};
    let base = if c == wood() || c == rgb(176, 132, 84) {
        Base::Medieval(engine::texgen::MedievalLayer::Oak)
    } else if FABRICS.iter().any(|&f| c3(f) == c) {
        Base::Fabric
    } else {
        Base::PaintedWood
    };
    Some(Skin::new(base, Wear::Fresh).into())
}

fn hash(o: DVec2, level: usize, k: usize) -> u64 {
    let mut h = (o.x * 13.7).to_bits() ^ (o.y * 5.3).to_bits().rotate_left(21);
    h ^= (level as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= (k as u64 + 7).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 31;
    h.wrapping_mul(0xBF58_476D_1CE4_E5B9) ^ (h >> 29)
}

/// Pick one of `n` from bits `shift..` of `seed`.
fn pick(seed: u64, shift: u32, n: usize) -> usize {
    ((seed >> shift) % n as u64) as usize
}

const CHROME: (u8, u8, u8) = (200, 206, 214);
const AETHER: (u8, u8, u8) = (120, 230, 255);
const WARM: (u8, u8, u8) = (255, 214, 150);

fn c3(t: (u8, u8, u8)) -> Color {
    rgb(t.0, t.1, t.2)
}

/// Upholstery in golden-age colours.
const FABRICS: [(u8, u8, u8); 6] = [
    (196, 70, 64),
    (64, 150, 140),
    (226, 180, 80),
    (120, 90, 160),
    (90, 140, 200),
    (220, 130, 150),
];

fn fabric(seed: u64, shift: u32) -> Color {
    c3(FABRICS[pick(seed, shift, FABRICS.len())])
}

fn wood() -> Color {
    rgb(122, 78, 46)
}

/// A rug: a thin slab just above the floor.
fn rug(fu: &mut Furnisher, c: DVec2, half: DVec2, col: Color) {
    fu.bx(
        DVec3::new(c.x - half.x, 0.0, c.y - half.y),
        DVec3::new(c.x + half.x, 0.02, c.y + half.y),
        col,
    );
}

/// A potted plant: a pot and a round green crown.
fn plant(fu: &mut Furnisher, c: DVec2) {
    fu.cyl(
        DVec3::new(c.x, 0.0, c.y),
        (0.25, 0.3),
        0.5,
        10,
        rgb(196, 110, 70),
    );
    let p = fu.city(DVec3::new(c.x, 1.05, c.y));
    fu.b.dome(p - DVec3::Y * 0.3, 0.55, 10, 4, rgb(70, 130, 70));
}

/// A floor lamp with a glowing shade.
fn floor_lamp(fu: &mut Furnisher, c: DVec2) {
    fu.surf(Surf::Metal, |fu| {
        fu.cyl(
            DVec3::new(c.x, 0.0, c.y),
            (0.18, 0.18),
            0.04,
            10,
            c3(CHROME),
        );
        fu.cyl(
            DVec3::new(c.x, 0.04, c.y),
            (0.025, 0.025),
            1.5,
            6,
            c3(CHROME),
        );
    });
    fu.surf(Surf::Glow, |fu| {
        fu.cyl(DVec3::new(c.x, 1.5, c.y), (0.3, 0.16), 0.28, 10, c3(WARM))
    });
}

/// Room interior, `m` in from the walls' faces (walls stand half their
/// thickness into the room; furniture keeps a hand's breadth off them).
fn inner(room: &Room, m: f64) -> (DVec2, DVec2) {
    let m = m + 0.16;
    (
        room.rect.min + DVec2::splat(m),
        room.rect.max - DVec2::splat(m),
    )
}

fn lobby(fu: &mut Furnisher, room: &Room, seed: u64) {
    let (lo, hi) = inner(room, 0.6);
    let size = hi - lo;
    if size.min_element() < 2.0 {
        return;
    }
    let c = (lo + hi) * 0.5;
    // A round deco rug in two rings.
    let r = (size.min_element() * 0.35).min(4.0);
    fu.cyl(
        DVec3::new(c.x, 0.0, c.y),
        (r, r),
        0.02,
        32,
        rgb(150, 40, 46),
    );
    fu.cyl(
        DVec3::new(c.x, 0.02, c.y),
        (r * 0.6, r * 0.6),
        0.01,
        32,
        rgb(226, 184, 90),
    );
    // The front desk at the back, chrome trim and a glowing band.
    let desk = DVec2::new(c.x, hi.y - 0.6);
    let half = DVec2::new((size.x * 0.25).clamp(1.0, 2.2), 0.45);
    if fu.clear(desk, half) {
        fu.bx(
            DVec3::new(desk.x - half.x, 0.0, desk.y - half.y),
            DVec3::new(desk.x + half.x, 1.05, desk.y + half.y),
            rgb(232, 222, 200),
        );
        fu.surf(Surf::Metal, |fu| {
            fu.bx(
                DVec3::new(desk.x - half.x - 0.02, 1.05, desk.y - half.y - 0.02),
                DVec3::new(desk.x + half.x + 0.02, 1.1, desk.y + half.y + 0.02),
                c3(CHROME),
            )
        });
        fu.surf(Surf::Glow, |fu| {
            fu.bx(
                DVec3::new(desk.x - half.x, 0.7, desk.y - half.y - 0.03),
                DVec3::new(desk.x + half.x, 0.8, desk.y - half.y),
                c3(AETHER),
            )
        });
    }
    // Palms in the corners, benches along the sides.
    for corner in [lo, DVec2::new(hi.x, lo.y)] {
        let p = corner + (c - corner).signum() * 0.4;
        if fu.clear(p, DVec2::splat(0.5)) {
            plant(fu, p);
        }
    }
    let bench = fabric(seed, 3);
    for x in [lo.x + 0.4, hi.x - 0.4] {
        let p = DVec2::new(x, c.y);
        let h = DVec2::new(0.35, (size.y * 0.2).clamp(0.6, 1.2));
        if fu.clear(p, h) {
            fu.bx(
                DVec3::new(p.x - h.x, 0.0, p.y - h.y),
                DVec3::new(p.x + h.x, 0.45, p.y + h.y),
                bench,
            );
        }
    }
}

fn office(fu: &mut Furnisher, room: &Room, seed: u64) {
    let (lo, hi) = inner(room, 1.0);
    let size = hi - lo;
    if size.min_element() < 1.6 {
        return;
    }
    let steel = rgb(96, 120, 116);
    let nx = ((size.x / 3.0).floor() as usize).max(1);
    let nz = ((size.y / 2.8).floor() as usize).max(1);
    let chair = fabric(seed, 5);
    for i in 0..nx {
        for j in 0..nz {
            let c = DVec2::new(
                lo.x + size.x * (i as f64 + 0.5) / nx as f64,
                lo.y + size.y * (j as f64 + 0.5) / nz as f64,
            );
            let h = DVec2::new(0.75, 0.4);
            if !fu.clear(c, h + DVec2::new(0.0, 0.6)) {
                continue;
            }
            // Desk: a top on two pedestals.
            fu.bx(
                DVec3::new(c.x - h.x, 0.72, c.y - h.y),
                DVec3::new(c.x + h.x, 0.76, c.y + h.y),
                wood(),
            );
            for s in [-1.0, 1.0] {
                fu.bx(
                    DVec3::new(c.x + s * h.x - 0.2 * s - 0.2, 0.0, c.y - h.y + 0.05),
                    DVec3::new(c.x + s * h.x - 0.2 * s + 0.2, 0.72, c.y + h.y - 0.05),
                    steel,
                );
            }
            // A green banker's lamp and a typewriter.
            fu.surf(Surf::Glow, |fu| {
                fu.cyl(
                    DVec3::new(c.x + 0.5, 0.76, c.y - 0.15),
                    (0.12, 0.08),
                    0.12,
                    8,
                    rgb(120, 220, 140),
                )
            });
            fu.bx(
                DVec3::new(c.x - 0.25, 0.76, c.y - 0.2),
                DVec3::new(c.x + 0.15, 0.9, c.y + 0.1),
                rgb(40, 44, 48),
            );
            // The chair in front.
            let ch = DVec2::new(c.x, c.y + h.y + 0.45);
            fu.bx(
                DVec3::new(ch.x - 0.25, 0.42, ch.y - 0.25),
                DVec3::new(ch.x + 0.25, 0.5, ch.y + 0.25),
                chair,
            );
            fu.bx(
                DVec3::new(ch.x - 0.25, 0.5, ch.y + 0.18),
                DVec3::new(ch.x + 0.25, 1.0, ch.y + 0.25),
                chair,
            );
            fu.surf(Surf::Metal, |fu| {
                fu.cyl(
                    DVec3::new(ch.x, 0.0, ch.y),
                    (0.04, 0.04),
                    0.42,
                    6,
                    c3(CHROME),
                )
            });
        }
    }
    // Filing cabinets along the back wall.
    let n = ((size.x / 1.2) as usize).min(4);
    for k in 0..n {
        let x = lo.x - 0.6 + 0.3 + k as f64 * 0.6;
        let p = DVec2::new(x, hi.y + 0.7);
        if fu.clear(p, DVec2::new(0.3, 0.3)) {
            fu.bx(
                DVec3::new(x - 0.28, 0.0, hi.y + 0.45),
                DVec3::new(x + 0.28, 1.3, hi.y + 0.95),
                steel,
            );
        }
    }
}

fn living(fu: &mut Furnisher, room: &Room, seed: u64) {
    let (lo, hi) = inner(room, 0.35);
    let size = hi - lo;
    if size.min_element() < 2.2 {
        return;
    }
    let c = (lo + hi) * 0.5;
    rug(fu, c, size * 0.3, fabric(seed, 11));
    // A streamlined sofa against the back wall.
    let sofa = fabric(seed, 3);
    let s = DVec2::new(c.x, hi.y - 0.45);
    let hw = (size.x * 0.3).clamp(0.8, 1.2);
    if fu.clear(s, DVec2::new(hw, 0.45)) {
        fu.bx(
            DVec3::new(s.x - hw, 0.0, s.y - 0.45),
            DVec3::new(s.x + hw, 0.42, s.y + 0.45),
            sofa,
        );
        fu.bx(
            DVec3::new(s.x - hw, 0.42, s.y + 0.2),
            DVec3::new(s.x + hw, 0.85, s.y + 0.45),
            sofa,
        );
        for e in [-1.0, 1.0] {
            let x = s.x + e * hw;
            fu.cyl(DVec3::new(x, 0.0, s.y), (0.3, 0.3), 0.6, 12, sofa);
        }
    }
    // A kidney coffee table and an armchair.
    fu.cyl(DVec3::new(c.x, 0.35, c.y), (0.6, 0.6), 0.05, 16, wood());
    fu.surf(Surf::Metal, |fu| {
        fu.cyl(DVec3::new(c.x, 0.0, c.y), (0.06, 0.06), 0.35, 6, c3(CHROME))
    });
    let a = DVec2::new(lo.x + 0.5, c.y);
    if fu.clear(a, DVec2::splat(0.45)) {
        let col = fabric(seed, 7);
        fu.bx(
            DVec3::new(a.x - 0.4, 0.0, a.y - 0.4),
            DVec3::new(a.x + 0.4, 0.42, a.y + 0.4),
            col,
        );
        fu.bx(
            DVec3::new(a.x - 0.4, 0.42, a.y - 0.4),
            DVec3::new(a.x - 0.25, 0.9, a.y + 0.4),
            col,
        );
    }
    // A radio cabinet with a glowing dial.
    let r = DVec2::new(hi.x - 0.3, c.y);
    if fu.clear(r, DVec2::new(0.3, 0.45)) {
        fu.bx(
            DVec3::new(r.x - 0.25, 0.0, r.y - 0.45),
            DVec3::new(r.x + 0.25, 1.0, r.y + 0.45),
            wood(),
        );
        fu.surf(Surf::Glow, |fu| {
            fu.bx(
                DVec3::new(r.x - 0.27, 0.7, r.y - 0.2),
                DVec3::new(r.x - 0.25, 0.85, r.y + 0.2),
                c3(WARM),
            )
        });
    }
    let l = DVec2::new(lo.x + 0.3, hi.y - 0.3);
    if fu.clear(l, DVec2::splat(0.3)) {
        floor_lamp(fu, l);
    }
}

fn kitchen(fu: &mut Furnisher, room: &Room, seed: u64) {
    let (lo, hi) = inner(room, 0.0);
    let size = hi - lo;
    if size.min_element() < 2.0 {
        return;
    }
    let pastel = [rgb(150, 210, 196), rgb(240, 200, 120), rgb(230, 160, 170)][pick(seed, 9, 3)];
    // A counter along the back wall, pastel fronts, a chrome edge.
    let (x0, x1) = (lo.x + 0.1, hi.x - 0.9);
    let mut x = x0;
    while x + 0.6 <= x1 {
        let p = DVec2::new(x + 0.3, hi.y - 0.3);
        if fu.clear(p, DVec2::splat(0.3)) {
            fu.bx(
                DVec3::new(x, 0.0, hi.y - 0.6),
                DVec3::new(x + 0.6, 0.88, hi.y),
                pastel,
            );
            fu.bx(
                DVec3::new(x, 0.88, hi.y - 0.62),
                DVec3::new(x + 0.6, 0.92, hi.y),
                rgb(236, 232, 224),
            );
        }
        x += 0.6;
    }
    // A rounded refrigerator in the corner.
    let f = DVec2::new(hi.x - 0.45, hi.y - 0.4);
    if fu.clear(f, DVec2::splat(0.4)) {
        fu.bx(
            DVec3::new(f.x - 0.38, 0.0, f.y - 0.35),
            DVec3::new(f.x + 0.38, 1.6, f.y + 0.35),
            rgb(244, 242, 236),
        );
        let p = fu.city(DVec3::new(f.x, 1.6, f.y));
        fu.b.dome(p, 0.36, 10, 3, rgb(244, 242, 236));
    }
    // A table with two chairs.
    let t = DVec2::new((lo.x + hi.x) * 0.5, lo.y + size.y * 0.35);
    if fu.clear(t, DVec2::splat(0.7)) {
        fu.cyl(
            DVec3::new(t.x, 0.72, t.y),
            (0.5, 0.5),
            0.04,
            14,
            rgb(230, 90, 80),
        );
        fu.surf(Surf::Metal, |fu| {
            fu.cyl(DVec3::new(t.x, 0.0, t.y), (0.05, 0.05), 0.72, 6, c3(CHROME))
        });
        for s in [-1.0, 1.0] {
            let ch = DVec2::new(t.x + s * 0.75, t.y);
            fu.bx(
                DVec3::new(ch.x - 0.2, 0.42, ch.y - 0.2),
                DVec3::new(ch.x + 0.2, 0.48, ch.y + 0.2),
                pastel,
            );
        }
    }
}

fn bedroom(fu: &mut Furnisher, room: &Room, seed: u64) {
    let (lo, hi) = inner(room, 0.05);
    let size = hi - lo;
    if size.min_element() < 2.2 {
        return;
    }
    let c = (lo + hi) * 0.5;
    // The bed, head to the back wall.
    let half = DVec2::new(0.75, 1.0);
    let bed = DVec2::new(c.x, hi.y - half.y);
    if fu.clear(bed, half) {
        fu.bx(
            DVec3::new(bed.x - half.x, 0.0, bed.y - half.y),
            DVec3::new(bed.x + half.x, 0.35, bed.y + half.y),
            wood(),
        );
        fu.bx(
            DVec3::new(bed.x - half.x + 0.05, 0.35, bed.y - half.y + 0.05),
            DVec3::new(bed.x + half.x - 0.05, 0.55, bed.y + half.y - 0.05),
            fabric(seed, 13),
        );
        fu.bx(
            DVec3::new(bed.x - half.x + 0.1, 0.55, bed.y + half.y - 0.45),
            DVec3::new(bed.x + half.x - 0.1, 0.68, bed.y + half.y - 0.1),
            rgb(244, 240, 232),
        );
        fu.bx(
            DVec3::new(bed.x - half.x, 0.0, bed.y + half.y - 0.08),
            DVec3::new(bed.x + half.x, 1.1, bed.y + half.y),
            wood(),
        );
        // Nightstand and lamp.
        let n = DVec2::new(bed.x + half.x + 0.35, hi.y - 0.25);
        if fu.clear(n, DVec2::splat(0.25)) {
            fu.bx(
                DVec3::new(n.x - 0.22, 0.0, n.y - 0.22),
                DVec3::new(n.x + 0.22, 0.55, n.y + 0.22),
                wood(),
            );
            fu.surf(Surf::Glow, |fu| {
                fu.cyl(DVec3::new(n.x, 0.55, n.y), (0.14, 0.08), 0.22, 8, c3(WARM))
            });
        }
    }
    // A wardrobe by the side wall.
    let w = DVec2::new(lo.x + 0.3, lo.y + 0.7);
    if fu.clear(w, DVec2::new(0.3, 0.6)) {
        fu.bx(
            DVec3::new(w.x - 0.3, 0.0, w.y - 0.6),
            DVec3::new(w.x + 0.3, 2.0, w.y + 0.6),
            wood(),
        );
    }
}

fn shop(fu: &mut Furnisher, room: &Room, seed: u64) {
    let (lo, hi) = inner(room, 0.0);
    let size = hi - lo;
    if size.min_element() < 2.0 {
        return;
    }
    // Shelves along both side walls, stocked in bright boxes.
    for x in [lo.x + 0.25, hi.x - 0.25] {
        let mut z = lo.y + 0.6;
        while z + 1.0 < hi.y - 0.2 {
            let p = DVec2::new(x, z + 0.5);
            if fu.clear(p, DVec2::new(0.25, 0.5)) {
                fu.bx(
                    DVec3::new(x - 0.25, 0.0, z),
                    DVec3::new(x + 0.25, 1.8, z + 1.0),
                    rgb(236, 228, 210),
                );
                for (k, y) in [0.45, 0.95, 1.45].iter().enumerate() {
                    let col = fabric(seed, 3 + k as u32 * 4 + (z * 3.0) as u32 % 5);
                    fu.bx(
                        DVec3::new(x - 0.2, *y, z + 0.1),
                        DVec3::new(x + 0.2, y + 0.3, z + 0.9),
                        col,
                    );
                }
            }
            z += 1.1;
        }
    }
    // The counter, with a chrome till.
    let c = DVec2::new((lo.x + hi.x) * 0.5, hi.y - 1.2);
    let h = DVec2::new((size.x * 0.25).clamp(0.6, 1.5), 0.35);
    if fu.clear(c, h) {
        fu.bx(
            DVec3::new(c.x - h.x, 0.0, c.y - h.y),
            DVec3::new(c.x + h.x, 1.0, c.y + h.y),
            rgb(190, 60, 52),
        );
        fu.surf(Surf::Metal, |fu| {
            fu.bx(
                DVec3::new(c.x + h.x - 0.5, 1.0, c.y - 0.2),
                DVec3::new(c.x + h.x - 0.1, 1.3, c.y + 0.2),
                c3(CHROME),
            )
        });
    }
}

fn storage(fu: &mut Furnisher, room: &Room, seed: u64) {
    let (lo, hi) = inner(room, 0.3);
    if (hi - lo).min_element() < 1.0 {
        return;
    }
    let n = 2 + pick(seed, 5, 4);
    for k in 0..n {
        let t = (k as f64 + 0.5) / n as f64;
        let p = DVec2::new(lo.x + 0.4, lo.y + (hi.y - lo.y) * t);
        let s = 0.3 + 0.1 * pick(seed, 9 + k as u32, 3) as f64;
        if fu.clear(p, DVec2::splat(s)) {
            fu.bx(
                DVec3::new(p.x - s, 0.0, p.y - s),
                DVec3::new(p.x + s, 2.0 * s, p.y + s),
                rgb(176, 132, 84),
            );
        }
    }
}

fn gallery(fu: &mut Furnisher, room: &Room, seed: u64) {
    let (lo, hi) = inner(room, 1.2);
    let size = hi - lo;
    if size.min_element() < 1.0 {
        return;
    }
    // Plinths with glowing Aether sculptures in a row.
    let n = ((size.x / 3.0) as usize).clamp(1, 5);
    for k in 0..n {
        let p = DVec2::new(
            lo.x + size.x * (k as f64 + 0.5) / n as f64,
            (lo.y + hi.y) * 0.5,
        );
        if !fu.clear(p, DVec2::splat(0.4)) {
            continue;
        }
        fu.bx(
            DVec3::new(p.x - 0.35, 0.0, p.y - 0.35),
            DVec3::new(p.x + 0.35, 1.0, p.y + 0.35),
            rgb(236, 232, 224),
        );
        let col = if pick(seed, 3 + k as u32, 2) == 0 {
            c3(AETHER)
        } else {
            rgb(255, 160, 220)
        };
        fu.surf(Surf::Glow, |fu| {
            let at = fu.city(DVec3::new(p.x, 1.0, p.y));
            fu.b.dome(at, 0.3, 12, 4, col);
        });
    }
}

fn lab(fu: &mut Furnisher, room: &Room, _seed: u64) {
    let (lo, hi) = inner(room, 0.4);
    let size = hi - lo;
    if size.min_element() < 1.6 {
        return;
    }
    // Benches along the back wall with glowing flasks.
    let b = DVec2::new((lo.x + hi.x) * 0.5, hi.y - 0.35);
    let h = DVec2::new(size.x * 0.4, 0.35);
    if fu.clear(b, h) {
        fu.bx(
            DVec3::new(b.x - h.x, 0.0, b.y - h.y),
            DVec3::new(b.x + h.x, 0.9, b.y + h.y),
            rgb(236, 236, 236),
        );
        let n = ((h.x * 2.0) as usize).min(6);
        fu.surf(Surf::Glow, |fu| {
            for k in 0..n {
                let x = b.x - h.x + 0.3 + k as f64 * (2.0 * h.x - 0.6) / n.max(1) as f64;
                let col = if k % 2 == 0 {
                    rgb(120, 255, 140)
                } else {
                    c3(AETHER)
                };
                fu.cyl(DVec3::new(x, 0.9, b.y), (0.08, 0.03), 0.3, 8, col);
            }
        });
    }
    // An Aether coil in the middle.
    let c = (lo + hi) * 0.5;
    if fu.clear(c, DVec2::splat(0.6)) {
        fu.surf(Surf::Metal, |fu| {
            fu.cyl(DVec3::new(c.x, 0.0, c.y), (0.5, 0.4), 0.3, 12, c3(CHROME));
            fu.cyl(DVec3::new(c.x, 0.3, c.y), (0.12, 0.12), 1.2, 8, c3(CHROME));
        });
        fu.surf(Surf::Glow, |fu| {
            for k in 0..4 {
                let y = 0.5 + k as f64 * 0.25;
                fu.cyl(DVec3::new(c.x, y, c.y), (0.3, 0.3), 0.06, 12, c3(AETHER));
            }
        });
    }
}

/// The stair and lift core: golden lift doors framed in Aether light.
fn core(fu: &mut Furnisher, room: &Room, f: &Floor) {
    // The lift doors face the nearest doorway out of the core.
    let c = room.rect.center();
    let Some(d) = f
        .doors
        .iter()
        .filter(|d| room.rect.on_boundary(d.at, 0.05))
        .map(|d| d.at)
        .min_by(|a, b| a.distance(c).total_cmp(&b.distance(c)))
    else {
        return;
    };
    let toward = (d - c).normalize_or(DVec2::Y);
    // A glowing ring on the floor where the Aether lift rises.
    let r = (room.rect.size().min_element() * 0.3).clamp(0.5, 1.2);
    let at = c - toward * 0.2;
    fu.surf(Surf::Glow, |fu| {
        fu.cyl(DVec3::new(at.x, 0.0, at.y), (r, r), 0.03, 20, c3(AETHER))
    });
    fu.cyl(
        DVec3::new(at.x, 0.03, at.y),
        (r * 0.85, r * 0.85),
        0.01,
        20,
        rgb(214, 170, 72),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use hw_core::{Seed, StableId};
    use hw_gen::building::{BuildingSite, BuildingUse};
    use hw_gen::city::DistrictKind;

    #[test]
    fn furnished_floors_have_furniture() {
        for (usage, floors) in [
            (BuildingUse::Apartments, 4),
            (BuildingUse::Offices, 6),
            (BuildingUse::ShopsAndOffices, 3),
        ] {
            let site = BuildingSite {
                id: StableId(7),
                seed: Seed(7),
                usage,
                origin: DVec2::new(10.0, 20.0),
                along: DVec2::X,
                inward: DVec2::Y,
                size: DVec2::new(24.0, 16.0),
                floors,
                street: 0,
                district: DistrictKind::Commercial,
                vernacular: None,
            };
            let plan = BuildingPlan::generate(&site);
            let mut b = MeshBuilder::new();
            for level in 0..plan.floors.len() {
                furnish(&mut b, &plan, level, 0.0, DVec3::X);
            }
            assert!(b.face_count() > 50, "{usage:?}: {}", b.face_count());
        }
    }
}
