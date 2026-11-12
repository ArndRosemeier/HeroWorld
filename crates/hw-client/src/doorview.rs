//! Doors you open and shut: every door near the camera drawn as instanced
//! leaves that swing on their hinges when the server opens or shuts them
//! (see `hw_sim::doors`), with the fixed glass of shop and lobby entrances
//! beside and above their glass doors.
//!
//! Which doors are drawn comes with the town's cells (`townmesh`): a cell
//! drawn in full detail lists the doors it shows (street doors of closed
//! buildings, every door of a floor drawn open), worked out on its worker
//! thread with the rest of the cell, and forgotten with it. Far cells keep
//! their painted doors in the facade.
//!
//! A building nobody is inside is drawn as a closed shell: behind a street
//! door standing open there is a small lit hall (a vestibule) to look into,
//! so an open door never shows a hole into nothing.

use crate::citymesh::{Look, MeshBuilder, ROOM_FLOOR_TOP};
use crate::interior::pieces::{Maker, Mat};
use crate::skins::Wear;
use engine::prelude::*;
use glam::{DVec2, DVec3, Vec2, Vec3};
use hw_gen::building::BuildingPlan;
use hw_gen::doors::DoorKind;
use hw_sim::collision::Obstacles;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::{channel, Receiver};

/// Seconds a door takes to swing open or shut.
const SWING_S: f32 = 0.7;
/// Leaves are modelled this wide and tall, and stretched to each door.
const LEAF_W: f64 = 0.87;
const LEAF_H: f64 = 2.7;
/// The fixed glass beside and over entrance doors: a 1 m square, stretched.
const PANE: f64 = 1.0;
/// Colours of street doors: older buildings (panelled), then streamline.
const CLASSIC: [(u8, u8, u8); 3] = [(46, 84, 62), (112, 38, 42), (40, 58, 92)];
const STREAMLINE: [(u8, u8, u8); 4] = [
    (64, 172, 166),
    (226, 108, 88),
    (224, 176, 60),
    (236, 226, 198),
];

/// What an instanced model draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Model {
    /// A panelled street door, painted (index into [`CLASSIC`]).
    Classic(u8),
    /// A streamline street door: porthole, chrome speed lines
    /// ([`STREAMLINE`]).
    Streamline(u8),
    /// A chrome-framed glass door with a push bar.
    Glass,
    /// An inside door: flush (`true`) or panelled, in a wood
    /// ([`crate::interior::DOOR_WOODS`]).
    Inner(bool, u8),
    /// Fixed glass in a chrome frame.
    Pane,
    /// A small lit hall behind a street door of a building drawn closed.
    Vestibule,
}

impl Model {
    fn all() -> Vec<Model> {
        let mut out = Vec::new();
        out.extend((0..CLASSIC.len() as u8).map(Model::Classic));
        out.extend((0..STREAMLINE.len() as u8).map(Model::Streamline));
        out.push(Model::Glass);
        for w in 0..crate::interior::DOOR_WOODS.len() as u8 {
            out.push(Model::Inner(false, w));
        }
        out.push(Model::Inner(true, 0));
        out.push(Model::Pane);
        out.push(Model::Vestibule);
        out
    }

    /// Its detail levels: (mesh, out to this distance).
    fn levels(self) -> Vec<LodLevel> {
        let make = |seg: usize, fine: bool| {
            let mut b = MeshBuilder::new();
            {
                let mut m = Maker::new(&mut b, DVec3::ZERO, DVec3::Z, seg, Wear::Fresh);
                self.build(&mut m, fine);
            }
            b.finish()
        };
        let (near, mid, far) = match self {
            Model::Vestibule => (40.0, 120.0, 260.0),
            _ => (18.0, 60.0, 260.0),
        };
        vec![
            LodLevel::new(make(24, true), near),
            LodLevel::new(make(10, true), mid),
            LodLevel::new(make(6, false), far),
        ]
    }

    /// Builds the model in `m`'s frame: a leaf hinged at the origin, LEAF_W
    /// along +x, LEAF_H up, its faces looking along ±z; a pane from x 0..1,
    /// y 0..1; the vestibule's mouth at the origin, its room along +z.
    fn build(self, m: &mut Maker, fine: bool) {
        let (w, h) = (LEAF_W, LEAF_H);
        match self {
            Model::Classic(c) => {
                let c = CLASSIC[c as usize];
                classic(m, w, h, rgb(c.0, c.1, c.2), fine);
            }
            Model::Streamline(c) => {
                let c = STREAMLINE[c as usize];
                streamline(m, w, h, rgb(c.0, c.1, c.2), fine);
            }
            Model::Glass => glass_door(m, w, h, fine),
            Model::Inner(flush, wood) => {
                let c = crate::interior::DOOR_WOODS[wood as usize];
                let mut s = m.sub(DVec3::new(w * 0.5, 0.0, 0.0), 0.0);
                if fine {
                    crate::interior::door_leaf(&mut s, w, h, flush, rgb(c.0, c.1, c.2));
                } else {
                    let col = if flush {
                        rgb(214, 196, 160)
                    } else {
                        rgb(c.0, c.1, c.2)
                    };
                    s.block(
                        Mat::Paint(col),
                        DVec3::new(-w * 0.5, 0.012, -0.022),
                        DVec3::new(w * 0.5, h, 0.022),
                    );
                }
            }
            Model::Pane => pane(m, fine),
            Model::Vestibule => vestibule(m, fine),
        }
    }
}

/// A slab leaf `w` by `h`, `t` half thick, in `mat`.
fn slab(m: &mut Maker, mat: Mat, w: f64, h: f64, t: f64) {
    m.block(mat, DVec3::new(0.0, 0.01, -t), DVec3::new(w, h, t));
}

/// A painted street door of an older house: raised panels both faces, a
/// brass knob, letter plate and kick plate, a small glazed light at the
/// top.
fn classic(m: &mut Maker, w: f64, h: f64, paint: Color, fine: bool) {
    let t = 0.028;
    slab(m, Mat::Paint(paint), w, h, t);
    if !fine {
        return;
    }
    let lighter = Color {
        r: (paint.r * 1.18).min(1.0),
        g: (paint.g * 1.18).min(1.0),
        b: (paint.b * 1.18).min(1.0),
        a: 1.0,
    };
    for s in [-1.0, 1.0] {
        let (z0, z1) = if s > 0.0 {
            (t - 0.004, t + 0.016)
        } else {
            (-t - 0.016, -t + 0.004)
        };
        // Two tall panels, one short one under them.
        for (y0, y1) in [(0.18, 0.86), (1.0, h - 0.62)] {
            m.block(
                Mat::Paint(lighter),
                DVec3::new(0.12, y0, z0),
                DVec3::new(w - 0.12, y1, z1),
            );
        }
        // The glazed light at the top, in a frame.
        m.block(
            Mat::Paint(lighter),
            DVec3::new(0.1, h - 0.52, z0),
            DVec3::new(w - 0.1, h - 0.14, z1),
        );
        let (gz0, gz1) = if s > 0.0 {
            (t + 0.016, t + 0.019)
        } else {
            (-t - 0.019, -t - 0.016)
        };
        m.block(
            Mat::Glass(rgb(250, 226, 160)),
            DVec3::new(0.16, h - 0.46, gz0),
            DVec3::new(w - 0.16, h - 0.2, gz1),
        );
        // Kick plate.
        m.block(
            Mat::Brass,
            DVec3::new(0.04, 0.02, if s > 0.0 { t } else { -t - 0.004 }),
            DVec3::new(w - 0.04, 0.16, if s > 0.0 { t + 0.004 } else { -t }),
        );
        // Knob on its rose.
        m.lathe_along(
            Mat::Brass,
            DVec3::new(w - 0.1, 1.02, s * t),
            DVec3::Z * s,
            &[
                (0.032, 0.0),
                (0.032, 0.008),
                (0.012, 0.012),
                (0.012, 0.04),
                (0.034, 0.058),
                (0.03, 0.074),
                (0.0, 0.08),
            ],
        );
    }
    // Letter plate on the street side.
    m.block(
        Mat::Brass,
        DVec3::new(w * 0.5 - 0.16, 0.9, -t - 0.012),
        DVec3::new(w * 0.5 + 0.16, 0.96, -t),
    );
}

/// A streamline street door: smooth enamel, a round porthole ringed in
/// chrome, three chrome speed lines sweeping to the long bar handle, a
/// chrome kick plate.
fn streamline(m: &mut Maker, w: f64, h: f64, paint: Color, fine: bool) {
    let t = 0.028;
    slab(m, Mat::Paint(paint), w, h, t);
    if !fine {
        return;
    }
    let port = DVec3::new(w * 0.46, 1.72, 0.0);
    for s in [-1.0, 1.0] {
        // Porthole: a chrome ring and the glass in it.
        m.lathe_along(
            Mat::Chrome,
            DVec3::new(port.x, port.y, s * t),
            DVec3::Z * s,
            &[
                (0.2, 0.0),
                (0.2, 0.008),
                (0.19, 0.022),
                (0.165, 0.026),
                (0.15, 0.018),
                (0.15, 0.0),
            ],
        );
        // Speed lines.
        for k in 0..3 {
            let y = 0.92 + k as f64 * 0.07;
            let x0 = 0.1 + k as f64 * 0.06;
            let (z0, z1) = if s > 0.0 {
                (t - 0.002, t + 0.008)
            } else {
                (-t - 0.008, -t + 0.002)
            };
            m.block(
                Mat::Chrome,
                DVec3::new(x0, y, z0),
                DVec3::new(w - 0.16, y + 0.022, z1),
            );
        }
        // The long bar handle on two posts.
        let z = s * (t + 0.05);
        m.tube(
            Mat::Chrome,
            &[DVec3::new(w - 0.1, 0.78, z), DVec3::new(w - 0.1, 1.38, z)],
            0.016,
        );
        for y in [0.84, 1.32] {
            m.tube(
                Mat::Chrome,
                &[DVec3::new(w - 0.1, y, s * t), DVec3::new(w - 0.1, y, z)],
                0.011,
            );
        }
        // Kick plate.
        m.block(
            Mat::Chrome,
            DVec3::new(0.03, 0.02, if s > 0.0 { t } else { -t - 0.004 }),
            DVec3::new(w - 0.03, 0.2, if s > 0.0 { t + 0.004 } else { -t }),
        );
    }
    m.lathe_along(
        Mat::Glass(rgb(214, 232, 236)),
        DVec3::new(port.x, port.y, -t - 0.004),
        DVec3::Z,
        &[
            (0.152, 0.0),
            (0.152, 2.0 * t + 0.008),
            (0.0, 2.0 * t + 0.009),
        ],
    );
}

/// A glass door in a slim chrome frame, a deep chrome bottom rail, a
/// round-ended push bar across it and a chrome starburst on the glass.
fn glass_door(m: &mut Maker, w: f64, h: f64, fine: bool) {
    let t = 0.024;
    let stile = 0.055;
    let (rail_b, rail_t) = (0.24, 0.07);
    // The frame.
    m.block(
        Mat::Chrome,
        DVec3::new(0.0, 0.01, -t),
        DVec3::new(stile, h, t),
    );
    m.block(
        Mat::Chrome,
        DVec3::new(w - stile, 0.01, -t),
        DVec3::new(w, h, t),
    );
    m.block(
        Mat::Chrome,
        DVec3::new(stile, 0.01, -t),
        DVec3::new(w - stile, rail_b, t),
    );
    m.block(
        Mat::Chrome,
        DVec3::new(stile, h - rail_t, -t),
        DVec3::new(w - stile, h, t),
    );
    // The glass: a little greenish, like plate glass on edge.
    m.block(
        Mat::Glass(rgb(196, 222, 216)),
        DVec3::new(stile, rail_b, -0.006),
        DVec3::new(w - stile, h - rail_t, 0.006),
    );
    if !fine {
        return;
    }
    for s in [-1.0, 1.0] {
        // Push bar on round posts.
        let z = s * (t + 0.06);
        m.tube(
            Mat::Chrome,
            &[DVec3::new(0.12, 1.04, z), DVec3::new(w - 0.1, 1.04, z)],
            0.018,
        );
        for x in [0.16, w - 0.14] {
            m.tube(
                Mat::Chrome,
                &[DVec3::new(x, 1.04, s * 0.006), DVec3::new(x, 1.04, z)],
                0.012,
            );
        }
    }
    // A chrome starburst on the glass at eye height: eight rays.
    let c = DVec3::new(w * 0.5, 1.75, 0.0);
    for k in 0..8 {
        let a = k as f64 * std::f64::consts::FRAC_PI_4;
        let len = if k % 2 == 0 { 0.17 } else { 0.1 };
        let d = DVec3::new(a.cos(), a.sin(), 0.0);
        m.tube(
            Mat::Chrome,
            &[
                c + d * 0.035 + DVec3::Z * 0.008,
                c + d * len + DVec3::Z * 0.008,
            ],
            0.005,
        );
    }
    m.lathe_along(
        Mat::Chrome,
        c + DVec3::Z * 0.006,
        DVec3::Z,
        &[(0.03, 0.0), (0.03, 0.006), (0.0, 0.01)],
    );
}

/// Fixed glass in a chrome frame, 1 m square (stretched to fit).
fn pane(m: &mut Maker, fine: bool) {
    let t = 0.02;
    let f = 0.04;
    m.block(
        Mat::Glass(rgb(196, 222, 216)),
        DVec3::new(0.0, 0.0, -0.006),
        DVec3::new(PANE, PANE, 0.006),
    );
    if !fine {
        return;
    }
    for (lo, hi) in [
        (DVec3::new(0.0, 0.0, -t), DVec3::new(f, PANE, t)),
        (DVec3::new(PANE - f, 0.0, -t), DVec3::new(PANE, PANE, t)),
        (DVec3::new(f, 0.0, -t), DVec3::new(PANE - f, f, t)),
        (DVec3::new(f, PANE - f, -t), DVec3::new(PANE - f, PANE, t)),
    ] {
        m.block(Mat::Chrome, lo, hi);
    }
}

/// The hall behind an open street door of a building drawn closed: a
/// terrazzo floor with a border, cream walls with a wainscot, a ceiling
/// with a glowing round lamp, a door at the back. Its faces look inwards.
fn vestibule(m: &mut Maker, fine: bool) {
    let (hw, d, h) = (1.4, 3.2, 3.1);
    let z0 = 0.02;
    let up = DVec3::Y;
    let floor = Mat::Plain(rgb(214, 206, 190));
    let wall = Mat::Plain(rgb(236, 224, 198));
    let low = Mat::Plain(rgb(150, 112, 80));
    // Floor and ceiling.
    m.face(
        floor,
        [
            DVec3::new(-hw, 0.002, z0),
            DVec3::new(-hw, 0.002, d),
            DVec3::new(hw, 0.002, d),
            DVec3::new(hw, 0.002, z0),
        ],
        up,
    );
    m.face(
        Mat::Plain(rgb(242, 238, 228)),
        [
            DVec3::new(-hw, h, z0),
            DVec3::new(hw, h, z0),
            DVec3::new(hw, h, d),
            DVec3::new(-hw, h, d),
        ],
        -up,
    );
    // Walls: left, right, back.
    let walls = [
        (
            [
                DVec3::new(-hw, 0.0, z0),
                DVec3::new(-hw, h, z0),
                DVec3::new(-hw, h, d),
                DVec3::new(-hw, 0.0, d),
            ],
            DVec3::X,
        ),
        (
            [
                DVec3::new(hw, 0.0, z0),
                DVec3::new(hw, 0.0, d),
                DVec3::new(hw, h, d),
                DVec3::new(hw, h, z0),
            ],
            -DVec3::X,
        ),
        (
            [
                DVec3::new(-hw, 0.0, d),
                DVec3::new(-hw, h, d),
                DVec3::new(hw, h, d),
                DVec3::new(hw, 0.0, d),
            ],
            -DVec3::Z,
        ),
    ];
    for (q, n) in walls {
        m.face(wall, q, n);
    }
    if !fine {
        return;
    }
    // A terrazzo border and a darker wainscot round the walls.
    m.face(
        Mat::Plain(rgb(168, 150, 120)),
        [
            DVec3::new(-hw + 0.25, 0.004, 0.5),
            DVec3::new(-hw + 0.25, 0.004, d - 0.3),
            DVec3::new(hw - 0.25, 0.004, d - 0.3),
            DVec3::new(hw - 0.25, 0.004, 0.5),
        ],
        up,
    );
    m.face(
        floor,
        [
            DVec3::new(-hw + 0.33, 0.006, 0.58),
            DVec3::new(-hw + 0.33, 0.006, d - 0.38),
            DVec3::new(hw - 0.33, 0.006, d - 0.38),
            DVec3::new(hw - 0.33, 0.006, 0.58),
        ],
        up,
    );
    for (q, n) in walls {
        // The lower part of each wall, a little off it.
        let off = n * 0.004;
        let low_q = [q[0], q[1], q[2], q[3]].map(|p| {
            let y = if p.y > 0.5 { 0.95 } else { 0.0 };
            DVec3::new(p.x, y, p.z) + off
        });
        m.face(low, low_q, n);
    }
    // A door at the back, ajar-dark, in a casing.
    m.face(
        Mat::Plain(rgb(96, 64, 42)),
        [
            DVec3::new(-0.5, 0.0, d - 0.01),
            DVec3::new(-0.5, 2.3, d - 0.01),
            DVec3::new(0.5, 2.3, d - 0.01),
            DVec3::new(0.5, 0.0, d - 0.01),
        ],
        -DVec3::Z,
    );
    m.lathe_along(
        Mat::Brass,
        DVec3::new(0.38, 1.0, d - 0.01),
        -DVec3::Z,
        &[(0.02, 0.0), (0.02, 0.03), (0.03, 0.05), (0.0, 0.06)],
    );
    // The lamp: a glowing disc in a chrome ring.
    m.lathe_along(
        Mat::Chrome,
        DVec3::new(0.0, h - 0.002, d * 0.5),
        -DVec3::Y,
        &[(0.32, 0.0), (0.32, 0.03), (0.27, 0.05), (0.0, 0.05)],
    );
    m.lathe_along(
        Mat::Glow(rgb(255, 232, 186)),
        DVec3::new(0.0, h - 0.05, d * 0.5),
        -DVec3::Y,
        &[(0.26, 0.0), (0.2, 0.06), (0.0, 0.08)],
    );
}

/// One leaf of a door, in the city.
#[derive(Clone, Copy, Debug)]
pub struct LeafSpot {
    /// The hinge's foot.
    pub hinge: Vec3,
    /// From the hinge to the free edge, shut and open (x, z; unit).
    pub shut: Vec2,
    pub open: Vec2,
    /// The model stretched to this leaf.
    pub stretch: Vec3,
}

/// One door a cell shows.
#[derive(Clone, Debug)]
pub struct DoorSpot {
    pub building: u64,
    pub key: u32,
    pub model: Model,
    pub leaves: Vec<LeafSpot>,
    /// The doorway's centre on its floor.
    pub at: Vec3,
    pub kind: DoorKind,
    /// The vestibule's place, for a street door of a floor drawn closed.
    pub vestibule: Option<Place>,
}

/// A fixed pane a cell shows.
#[derive(Clone, Copy, Debug)]
pub struct PaneSpot {
    pub place: Place,
}

/// The doors and panes of one building as drawn with `look`, standing on
/// `base` (its ground in the city).
pub fn door_spots(
    plan: &BuildingPlan,
    id: u64,
    base: f64,
    look: Look,
) -> (Vec<DoorSpot>, Vec<PaneSpot>) {
    let n = plan.floors.len();
    // The floor drawn open, if any; floors above it are not drawn.
    let open_floor = match look {
        Look::Shell => None,
        Look::Cut { level } => Some((level as usize).min(n.saturating_sub(1))),
    };
    let site = &plan.site;
    let city = |p: DVec2, y: f64| site.to_city(DVec3::new(p.x, y, p.y)).as_vec3();
    let dir = |d: DVec2| {
        let w = site.along * d.x + site.inward * d.y;
        Vec2::new(w.x as f32, w.y as f32)
    };
    let yaw_of = |d: Vec2| (-d.y).atan2(d.x).to_degrees();
    let old = crate::interior::old_fashioned(plan);
    let (flush, wood) = crate::interior::inner_door(plan);
    let paint = (plan.site.seed.0 >> 9) as usize;
    let mut doors = Vec::new();
    let mut panes = Vec::new();
    for d in plan.door_leaves() {
        let Some(k) = plan.floors.iter().position(|f| f.level == d.level) else {
            continue;
        };
        let drawn_open = open_floor == Some(k);
        let shown = match open_floor {
            // (A building of halls is never cut open: its inside is
            // drawn whole, with every door.)
            None => d.outside || plan.layout.is_some(),
            Some(o) => k == o || (k < o && d.outside),
        };
        if !shown {
            continue;
        }
        let f = &plan.floors[k];
        let floor = base + f.elevation;
        let (y, top) = if drawn_open {
            // The cut floor: on its floor finish, under its walls' top.
            (floor + ROOM_FLOOR_TOP + 0.008, d.top.min(3.0))
        } else {
            (floor + 0.005, d.top)
        };
        let model = match d.kind {
            DoorKind::Glass => Model::Glass,
            DoorKind::Front if old => Model::Classic((paint % CLASSIC.len()) as u8),
            DoorKind::Front => Model::Streamline((paint % STREAMLINE.len()) as u8),
            DoorKind::Inner if flush => Model::Inner(true, 0),
            DoorKind::Inner => Model::Inner(false, wood as u8),
        };
        let height = (d.height - (y - floor - 0.005)).min(top - (y - floor)) - 0.01;
        let leaves = d
            .leaves
            .iter()
            .map(|l| LeafSpot {
                hinge: city(l.hinge, y),
                shut: dir(l.shut),
                open: dir(l.open),
                stretch: Vec3::new((l.width / LEAF_W) as f32, (height / LEAF_H) as f32, 1.0),
            })
            .collect();
        // Fixed glass beside the doors, and over them up to the doorway's
        // top (or the cut walls' top).
        let into = dir(d.into);
        let along = dir(d.along);
        let pane_at = |t0: f64, t1: f64, y0: f64, y1: f64| {
            let p = city(d.point(t0, d.plane), y0);
            PaneSpot {
                place: Place::new(p.x, p.y, p.z)
                    .with_yaw_deg(yaw_of(along))
                    .with_stretch(Vec3::new((t1 - t0) as f32, (y1 - y0) as f32, 1.0)),
            }
        };
        for &(a, b) in &d.panes {
            panes.push(pane_at(a, b, y, floor + top));
        }
        let over = floor + top - (y + height);
        if d.kind == DoorKind::Glass && over > 0.05 {
            panes.push(pane_at(-d.half, d.half, y + height, floor + top));
        }
        // (Not in a building of halls: its lobby is really there.)
        let vestibule = (d.outside && !drawn_open && plan.layout.is_none()).then(|| {
            let p = city(d.point(0.0, 0.0), floor + 0.004);
            // Its +z into the building.
            Place::new(p.x, p.y, p.z).with_yaw_deg(into.x.atan2(into.y).to_degrees())
        });
        doors.push(DoorSpot {
            building: id,
            key: d.key,
            model,
            leaves,
            at: city(d.at, floor),
            kind: d.kind,
            vestibule,
        });
    }
    (doors, panes)
}

/// Where the model of a leaf `a` of the way open stands.
fn leaf_place(l: &LeafSpot, a: f32) -> Place {
    let t = a * std::f32::consts::FRAC_PI_2;
    let d = l.shut * t.cos() + l.open * t.sin();
    Place::new(l.hinge.x, l.hinge.y, l.hinge.z)
        .with_yaw_deg((-d.y).atan2(d.x).to_degrees())
        .with_stretch(l.stretch)
}

/// Eases a door's swing: quick out, soft into the stop.
fn ease(a: f32) -> f32 {
    1.0 - (1.0 - a) * (1.0 - a)
}

/// A model and its detail levels.
type ModelLevels = (Model, Vec<LodLevel>);

/// The doors near the camera, drawn and swinging.
pub struct DoorView {
    /// Models being built on a worker thread, then one LOD set each.
    making: Option<Receiver<Vec<ModelLevels>>>,
    sets: BTreeMap<Model, LodId>,
    /// Doors shown by cell generation (see [`Self::update`]).
    seen: u64,
    /// How far open each door is drawn (0 shut .. 1 open), for doors not
    /// shut.
    swing: BTreeMap<(u64, u32), f32>,
    /// Door versions per building as last read from collision.
    versions: BTreeMap<u64, u64>,
    /// Doors standing open (or opening) now.
    open: BTreeSet<(u64, u32)>,
    /// Something moved since the places were last set.
    dirty: bool,
    /// Door sounds.
    bank: crate::soundscape::Bank,
}

macro_rules! clips {
    ($($name:literal),* $(,)?) => {
        &[$(($name, include_bytes!(concat!("../assets/sound/", $name, ".ogg")) as &[u8])),*]
    };
}

static DOOR_CLIPS: &[(&str, &[u8])] = clips!["door_open", "door_shut"];

impl Default for DoorView {
    fn default() -> Self {
        Self::new()
    }
}

impl DoorView {
    /// Starts building the door models on a worker thread.
    pub fn new() -> Self {
        let (tx, rx) = channel();
        std::thread::Builder::new()
            .name("door-models".into())
            .spawn(move || {
                engine::parallel::background_thread();
                let models: Vec<ModelLevels> =
                    Model::all().into_iter().map(|m| (m, m.levels())).collect();
                let _ = tx.send(models);
            })
            .expect("door model thread");
        Self {
            making: Some(rx),
            sets: BTreeMap::new(),
            seen: u64::MAX,
            swing: BTreeMap::new(),
            versions: BTreeMap::new(),
            open: BTreeSet::new(),
            dirty: true,
            bank: crate::soundscape::Bank::new(DOOR_CLIPS),
        }
    }

    /// Plays the sound of a door the server just opened or shut.
    pub fn heard(
        &mut self,
        world: &mut World,
        town: &crate::townmesh::TownView,
        b: u64,
        key: u32,
        open: bool,
        now: f64,
    ) {
        let Some(d) = town.door_spots().find(|d| d.building == b && d.key == key) else {
            return;
        };
        let name = if open { "door_open" } else { "door_shut" };
        let volume = if d.kind == DoorKind::Glass { 0.55 } else { 0.8 };
        if let Some(s) = self.bank.get(name, now) {
            world.play_sound(
                &s,
                PlaySound::at(d.at + Vec3::Y * 1.2)
                    .volume(volume)
                    .range(2.0, 30.0),
            );
        }
    }

    /// One frame: takes in the models once built, follows the doors the
    /// town shows and whether each is open (from collision, which the
    /// server's news keeps), swings them, and puts the instances in place.
    pub fn update(
        &mut self,
        world: &mut World,
        town: &crate::townmesh::TownView,
        solids: &Obstacles,
        dt: f32,
        now: f64,
    ) -> EngineResult<()> {
        if let Some(rx) = &self.making {
            if let Ok(models) = rx.try_recv() {
                for (m, levels) in models {
                    let id = world.spawn_lod_at(levels, &[])?;
                    self.sets.insert(m, id);
                }
                self.making = None;
                self.dirty = true;
            }
        }
        // Prepared sounds stay warm while doors are about.
        self.bank.sweep(now);
        if town.door_generation() != self.seen {
            self.seen = town.door_generation();
            self.dirty = true;
            let shown: BTreeSet<u64> = town.door_spots().map(|d| d.building).collect();
            self.versions.retain(|b, _| shown.contains(b));
            self.open.retain(|(b, _)| shown.contains(b));
            self.swing.retain(|(b, _), _| shown.contains(b));
        }
        // Which doors stand open: read again for buildings whose doors
        // changed (one lookup per building, not per door).
        let mut changed: BTreeSet<u64> = BTreeSet::new();
        for d in town.door_spots() {
            if changed.contains(&d.building) {
                continue;
            }
            let v = solids.door_version(d.building);
            if self.versions.get(&d.building) != Some(&v) {
                self.versions.insert(d.building, v);
                changed.insert(d.building);
            }
        }
        if !changed.is_empty() {
            for d in town.door_spots().filter(|d| changed.contains(&d.building)) {
                let k = (d.building, d.key);
                if solids.door_open(d.building, d.key) {
                    self.open.insert(k);
                    self.swing.entry(k).or_insert(0.0);
                } else {
                    self.open.remove(&k);
                }
            }
        }
        // Swing.
        let step = dt / SWING_S;
        let mut moving = false;
        self.swing.retain(|k, a| {
            let target = if self.open.contains(k) { 1.0 } else { 0.0 };
            if *a != target {
                moving = true;
                *a = if target > *a {
                    (*a + step).min(1.0)
                } else {
                    (*a - step).max(0.0)
                };
            }
            *a > 0.0 || target > 0.0
        });
        if moving {
            self.dirty = true;
        }
        if self.dirty && !self.sets.is_empty() {
            self.dirty = false;
            self.place(world, town)?;
        }
        Ok(())
    }

    fn place(&mut self, world: &mut World, town: &crate::townmesh::TownView) -> EngineResult<()> {
        let mut places: BTreeMap<Model, Vec<Place>> =
            self.sets.keys().map(|m| (*m, Vec::new())).collect();
        for d in town.door_spots() {
            let a = self.swing.get(&(d.building, d.key)).copied().unwrap_or(0.0);
            let list = places.entry(d.model).or_default();
            for l in &d.leaves {
                list.push(leaf_place(l, ease(a)));
            }
            // Drawn shut too: the slits beside the leaves and the
            // portholes show the hall, not the sky through the empty shell.
            if let Some(v) = d.vestibule {
                places.entry(Model::Vestibule).or_default().push(v);
            }
        }
        places
            .entry(Model::Pane)
            .or_default()
            .extend(town.pane_spots().map(|p| p.place));
        for (m, list) in places {
            if let Some(&id) = self.sets.get(&m) {
                world.set_lod_places(id, &list)?;
            }
        }
        Ok(())
    }
}

/// The door the hero would use with E: the nearest within reach, if any.
pub fn door_at_hand(solids: &Obstacles, hero: DVec3) -> Option<hw_sim::collision::DoorAt> {
    solids
        .doors_near(hero, hw_sim::doors::REACH)
        .into_iter()
        .next()
}

/// The words for E at a door.
pub fn prompt(open: bool, kind: DoorKind) -> &'static str {
    match (open, kind) {
        (false, DoorKind::Glass) => "E  Push the door open",
        (false, _) => "E  Open the door",
        (true, _) => "E  Shut the door",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn a_leaf_turns_from_shut_to_open_about_its_hinge() {
        let l = LeafSpot {
            hinge: Vec3::new(2.0, 0.0, 5.0),
            shut: Vec2::new(1.0, 0.0),
            open: Vec2::new(0.0, 1.0),
            stretch: Vec3::ONE,
        };
        // The model's +x (its free edge) points along the leaf.
        for (a, want) in [(0.0, Vec3::X), (1.0, Vec3::Z)] {
            let p = leaf_place(&l, a);
            let yaw = p.yaw_degrees.to_radians();
            let x = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
            assert!((x - want).length() < 1e-5, "{a}: {x}");
        }
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn a_closed_building_shows_its_street_doors_and_an_open_floor_all_its_doors() {
        let city = hw_gen::city::CityPlan::generate(
            hw_core::Seed(3),
            &hw_gen::city::CityParams::default(),
        );
        let plan = city
            .buildings
            .iter()
            .map(|(_, s)| BuildingPlan::generate(s))
            .find(|p| {
                p.floors.len() >= 3 && p.door_leaves().iter().filter(|d| !d.outside).count() > 3
            })
            .expect("a building with rooms");
        let (shell, _) = door_spots(&plan, 1, 0.0, Look::Shell);
        assert!(!shell.is_empty() && shell.iter().all(|d| d.vestibule.is_some()));
        assert!(shell.iter().all(|d| d.kind != DoorKind::Inner));
        let (cut, _) = door_spots(&plan, 1, 0.0, Look::Cut { level: 0 });
        assert!(cut.iter().any(|d| d.kind == DoorKind::Inner));
        assert!(
            cut.iter().all(|d| d.vestibule.is_none()),
            "the floor is drawn open"
        );
    }
}
