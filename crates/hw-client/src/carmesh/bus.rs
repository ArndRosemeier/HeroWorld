//! The streamliner bus: a long two-tone coach of the golden age with a
//! wraparound windscreen, a lit route sign over it, skylights along the
//! roof's shoulders, a chrome rocket riding the roof, fluted chrome skirts,
//! fender skirts over the rear wheels, tail fins with rocket lamps and an
//! atom on its nose. Two doors on the kerb side (its -x), each a pair of
//! glazed leaves that slide apart ([`bus_door_leaf`]); inside, rows of
//! seats a passenger can be put in ([`bus_seats`]).
//!
//! Same frame as the cars: nose to +z, +x its left (the driver's side),
//! wheels standing on y = 0.

use super::*;

/// Half its length and its widest half width (m).
pub const BUS_HALF_LEN: f64 = 5.6;
const BUS_W: f64 = 1.27;
/// The floor inside, and the top of a seat's cushion.
pub const BUS_FLOOR: f64 = 0.78;
const BUS_CUSHION: f64 = BUS_FLOOR + 0.45;
/// The doors on the kerb side: middle along z and width (m).
pub const BUS_DOORS: [(f64, f64); 2] = [(4.05, 0.95), (0.0, 1.1)];
/// A door's opening, bottom to top.
const DOOR_Y: [f64; 2] = [0.47, 2.5];
/// Wheels: x of their middles, the axles' z and the radius.
pub const BUS_WHEELS: (f64, [f64; 2], f64) = (1.02, [-3.35, 2.75], 0.5);
/// The window band, the windscreen's and the route sign's (heights).
const WIN: [f64; 2] = [1.6, 2.45];
const SCREEN: [f64; 2] = [1.45, 2.3];
const SIGN: [f64; 2] = [2.33, 2.52];
/// Skylights in the roof's shoulders (heights).
const SKY: [f64; 2] = [2.74, 2.86];
/// Belt lines: the chrome strip under the windows.
const BELT: [f64; 2] = [1.3, 1.37];
/// Where the side windows run (z), and how far apart their pillars stand.
const WIN_Z: [f64; 2] = [-4.95, 4.55];
const PANE: f64 = 1.06;
const PILLAR: f64 = 0.1;

/// The bus liveries: main colour, then belt and roof.
pub const BUS_LIVERY: [[(u8, u8, u8); 2]; 4] = [
    [(196, 38, 46), (244, 236, 220)],
    [(30, 136, 150), (244, 238, 226)],
    [(234, 144, 34), (250, 244, 232)],
    [(56, 92, 168), (216, 222, 230)],
];

fn bus_kit(livery: u8) -> Kit {
    let [a, b] = BUS_LIVERY[livery as usize % BUS_LIVERY.len()];
    Kit {
        body: rgb(a.0, a.1, a.2),
        top: rgb(b.0, b.1, b.2),
        seam: rgb(
            (a.0 as f32 * 0.4) as u8,
            (a.1 as f32 * 0.4) as u8,
            (a.2 as f32 * 0.4) as u8,
        ),
        ..Kit::new(0)
    }
}

fn bus_section(z: f64) -> Section {
    let nose = smooth(4.2, 5.6, z);
    let tail = smooth(-4.7, -5.6, z);
    Section {
        w: BUS_W - 0.12 * nose.powf(1.5) - 0.06 * tail,
        bot: 0.42 + 0.12 * nose + 0.1 * tail,
        mid: 1.05,
        top: 2.95 - 0.35 * nose * nose - 0.15 * tail,
        n_up: 4.5 - 1.5 * nose,
        n_dn: 5.0,
        tumble: 0.05,
        tuck: 0.06,
    }
}

/// The body's loft.
fn bus_body() -> Body<'static> {
    let mut stations = vec![];
    // Pillars between the side windows.
    let mut z = WIN_Z[0];
    while z < WIN_Z[1] {
        stations.extend([z, z + PILLAR]);
        z += PANE;
    }
    for (dz, w) in BUS_DOORS {
        stations.extend([dz - w * 0.5, dz + w * 0.5]);
    }
    let bands = [0.55, BELT[0], BELT[1], WIN[0], WIN[1], 2.55, SKY[0], SKY[1]]
        .iter()
        .map(|&y| band_of(&bus_section(0.0), y))
        .collect();
    Body {
        z: [-BUS_HALF_LEN, BUS_HALF_LEN],
        end_r: [0.45, 0.55],
        section: &bus_section,
        arches: vec![
            Arch {
                z: BUS_WHEELS.1[1],
                cy: BUS_WHEELS.2,
                r: 0.62,
                skirt: None,
                x_in: 0.82,
            },
            Arch {
                z: BUS_WHEELS.1[0],
                cy: BUS_WHEELS.2,
                r: 0.62,
                skirt: Some(0.74),
                x_in: 0.82,
            },
        ],
        stations,
        bands,
    }
}

/// The sine on a section that reaches height `y`.
fn band_of(sec: &Section, y: f64) -> f64 {
    if y >= sec.mid {
        ((y - sec.mid) / (sec.top - sec.mid))
            .clamp(0.0, 1.0)
            .powf(sec.n_up / 2.0)
    } else {
        -((sec.mid - y) / (sec.mid - sec.bot))
            .clamp(0.0, 1.0)
            .powf(sec.n_dn / 2.0)
    }
}

/// The height a quad of the body sits at (its ends' rounding included).
fn height_of(body: &Body, at: &At) -> f64 {
    let [z0, z1] = body.z;
    let [r0, r1] = body.end_r;
    let (a, bz) = (z0 + r0, z1 - r1);
    let sec = if at.z > bz {
        let th = ((at.z - bz) / r1).clamp(0.0, 1.0).asin();
        body.sec(bz).inset(r1 * (1.0 - th.cos()))
    } else if at.z < a {
        let th = ((a - at.z) / r0).clamp(0.0, 1.0).asin();
        body.sec(a).inset(r0 * (1.0 - th.cos()))
    } else {
        body.sec(at.z)
    };
    let e = 2.0 / if at.s >= 0.0 { sec.n_up } else { sec.n_dn };
    let f = at.s.abs().powf(e);
    if at.s >= 0.0 {
        sec.mid + (sec.top - sec.mid) * f
    } else {
        sec.mid - (sec.mid - sec.bot) * f
    }
}

/// How far forward the nose reaches at (`x`, `y`), or back the tail with
/// `rear` (as a positive z, the tail's mirrored).
fn end_z(body: &Body, x: f64, y: f64, rear: bool) -> f64 {
    let [z0, z1] = body.z;
    let [r0, r1] = body.end_r;
    let (base, r) = if rear { (z0 + r0, r0) } else { (z1 - r1, r1) };
    let sec = body.sec(base);
    let inside = |d: f64| {
        let s = sec.inset(d);
        y > s.bot && y < s.top && x.abs() <= s.x_at(y)
    };
    // The largest rounding angle at which the point is still inside.
    let (mut lo, mut hi) = (0.0, FRAC_PI_2);
    if !inside(0.0) {
        return base.abs();
    }
    for _ in 0..30 {
        let m = 0.5 * (lo + hi);
        if inside(r * (1.0 - f64::cos(m))) {
            lo = m;
        } else {
            hi = m;
        }
    }
    base.abs() + r * lo.sin()
}

/// Whether `z` falls on a pillar between side windows.
fn on_pillar(z: f64) -> bool {
    let u = (z - WIN_Z[0]).rem_euclid(PANE);
    u < PILLAR
}

/// The kerb-side door whose opening `z` falls in.
fn in_door(z: f64) -> bool {
    BUS_DOORS.iter().any(|&(dz, w)| (z - dz).abs() < w * 0.5)
}

/// Where a sitter's hips go in each seat, the driver first (the bus's
/// frame, as [`Seat::hip`]).
pub fn bus_seats() -> Vec<Seat> {
    let y = BUS_CUSHION + HIP_OVER_CUSHION;
    let mut out = vec![Seat {
        hip: DVec3::new(0.62, y, DRIVER_Z - 0.22 + 0.1),
        driver: true,
    }];
    for (z, x) in seat_spots() {
        out.push(Seat {
            hip: DVec3::new(x, y, z - 0.22 + 0.1),
            driver: false,
        });
    }
    out
}

/// The driver's cushion's middle along the bus.
const DRIVER_Z: f64 = 3.98;

/// Every passenger seat's cushion middle: (z, x).
fn seat_spots() -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for z in [1.9, 1.05, 0.2, -0.65, -1.5, -2.35, -4.3] {
        for x in [0.9, 0.42, -0.42, -0.9] {
            // Clear of the middle door on the kerb side.
            if x < 0.0 && (-0.95..0.95).contains(&z) {
                continue;
            }
            out.push((z, x));
        }
    }
    // The bench across the back.
    for x in [0.9, 0.45, 0.0, -0.45, -0.9] {
        out.push((-4.98, x));
    }
    out
}

/// The roof's height inside over (`x`, `z`) (for checking sitters' heads).
pub fn bus_roof_over(x: f64, z: f64) -> f64 {
    let body = bus_body();
    let sec = body.sec(z);
    let top = sec.y_at(x.abs().min(sec.w - 0.05));
    top - 0.04
}

/// The streamliner bus in `livery` into `b`; `wheels` adds the wheels
/// (far away: closer, moving buses turn theirs on their own). Its doors are
/// open holes: draw the leaves ([`bus_door_leaf`]) shut or open.
pub fn bus_into(b: &mut MeshBuilder, livery: u8, lod: Lod, wheels: bool) {
    let k = bus_kit(livery);
    let body = bus_body();
    let far = lod == Lod::Far;
    let window = if far {
        Kit::plain(rgb(38, 56, 68))
    } else {
        k.glazed()
    };
    let [z0, z1] = body.z;
    let nose_start = z1 - body.end_r[1] - 0.15;
    body.build(b, lod, |at| {
        if at.well {
            return Kit::plain(k.under);
        }
        let y = height_of(&body, &at);
        let kerb = at.c < -0.3;
        // Door openings (far off the doors are shut in the body).
        if kerb && in_door(at.z) && y < DOOR_Y[1] && !far {
            return None;
        }
        if y < 0.55 {
            return Kit::plain(k.under);
        }
        // The lower half (round the wheel arches too) is the main colour.
        if at.s < 0.0 {
            return Kit::plain(k.body);
        }
        // The nose: open where the windscreen goes (a sheet of its own,
        // [`windscreen`]); the paint round it is laid over the rounding's
        // quads as bands of its own, so their steps never show.
        if at.z > WIN_Z[1] + 0.05 && y > SCREEN[0] - 0.18 && y < SCREEN[1] + 0.08 {
            return if far {
                Kit::plain(rgb(38, 56, 68))
            } else {
                None
            };
        }
        let _ = z0;
        if y < BELT[0] {
            return Kit::plain(k.body);
        }
        if y < BELT[1] {
            return k.metal();
        }
        let side = at.c.abs() > 0.25;
        if side && y > WIN[0] && y < WIN[1] && at.z > WIN_Z[0] && at.z < WIN_Z[1] {
            if on_pillar(at.z) && at.z <= nose_start - 0.1 {
                return Kit::plain(k.top);
            }
            return window;
        }
        if side && (WIN[1]..2.55).contains(&y) && at.z > WIN_Z[0] - 0.2 && at.z < WIN_Z[1] + 0.05 {
            return k.metal();
        }
        if !far && y > SKY[0] && y < SKY[1] && at.z > -4.6 && at.z < 3.3 && !on_pillar(at.z) {
            return window;
        }
        Kit::plain(k.top)
    });
    if !far {
        bus_inside(b, &body, &k, lod);
    }
    bus_trim(b, &body, &k, lod);
    if wheels {
        let (x, zs, r) = BUS_WHEELS;
        for z in zs {
            for sx in [-x, x] {
                wheel(b, DVec3::new(sx, r, z), r, 0.3, lod);
            }
        }
    }
}

/// The wraparound windscreen: a sheet of glass over the nose from one
/// side to the other, in a chrome frame, and the lit route sign over it.
fn windscreen(b: &mut MeshBuilder, body: &Body, k: &Kit, lod: Lod) {
    let side_z = WIN_Z[1] - 0.05;
    let bz = body.z[1] - body.end_r[1];
    // Around the nose at height y: from the kerb side's back edge, round
    // the front, to the driver's side, `off` out from the skin.
    let around = |y: f64, off: f64| -> Vec<DVec3> {
        let n = lod.pick(24, 12, 6);
        let w = body.sec(bz).x_at(y);
        let mut pts = vec![DVec3::new(-(body.sec(side_z).x_at(y) + off), y, side_z)];
        for i in 0..=n {
            let t = -1.0 + 2.0 * i as f64 / n as f64;
            // Bunched at the corners, where the nose turns.
            let x = t.signum() * t.abs().powf(0.6) * (w - 0.01);
            let z = end_z(body, x, y, false);
            // Out along the turn of the corner.
            let lean = (x / w).clamp(-1.0, 1.0);
            pts.push(DVec3::new(
                x + lean * off,
                y,
                z + off * (1.0 - lean.abs()).sqrt(),
            ));
        }
        pts.push(DVec3::new(body.sec(side_z).x_at(y) + off, y, side_z));
        pts
    };
    let rows: Vec<Vec<DVec3>> = (0..=lod.pick(6, 3, 1))
        .map(|i| {
            let f = i as f64 / lod.pick(6, 3, 1) as f64;
            around(SCREEN[0] + (SCREEN[1] - SCREEN[0]) * f, 0.012)
        })
        .collect();
    let glass = if lod == Lod::Far {
        Kit::plain(rgb(38, 56, 68))
    } else {
        k.glazed()
    };
    surface(b, &rows, false, |_, _| glass);
    if lod == Lod::Far {
        return;
    }
    // The paint round the glass, in rows of its own over the body's nose:
    // main colour, the chrome belt and the top colour below, the top
    // colour up to the roof above.
    let top = body.sec(bz).top - 0.06;
    let below = [
        (1.06, Kit::plain(k.body)),
        (BELT[0], Kit::plain(k.body)),
        (BELT[0], k.metal()),
        (BELT[1], k.metal()),
        (BELT[1], Kit::plain(k.top)),
        (SCREEN[0], Kit::plain(k.top)),
    ];
    for pair in below.chunks(2) {
        let rows = [around(pair[0].0, 0.006), around(pair[1].0, 0.006)];
        let c = pair[0].1;
        surface(b, &rows, false, |_, _| c);
    }
    let brow: Vec<Vec<DVec3>> = (0..=lod.pick(8, 3, 1))
        .map(|i| {
            let f = i as f64 / lod.pick(8, 3, 1) as f64;
            around(SCREEN[1] + (top - SCREEN[1]) * f, 0.006)
        })
        .collect();
    surface(b, &brow, false, |_, _| Kit::plain(k.top));
    let seg = lod.pick(8, 6, 4);
    for y in SCREEN {
        tube(b, &around(y, 0.02), (0.022, 0.022), seg, k.metal());
    }
    // The route sign: a lit strip across the front over the glass.
    let sign = |y: f64, off: f64| -> Vec<DVec3> {
        (0..=10)
            .map(|i| {
                let x = -0.62 + 0.124 * i as f64;
                DVec3::new(x, y, end_z(body, x, y, false) + off)
            })
            .collect()
    };
    let rows = vec![sign(SIGN[0], 0.03), sign(SIGN[1], 0.03)];
    b.surf = Surf::Glow;
    surface(b, &rows, false, |_, _| Kit::glow(rgb(255, 214, 140)));
    b.surf = Surf::Plain;
    let mut frame = sign(SIGN[0] - 0.02, 0.035);
    frame.extend(sign(SIGN[1] + 0.02, 0.035).into_iter().rev());
    frame.push(frame[0]);
    tube(b, &frame, (0.016, 0.016), 6, k.metal());
}

/// Chrome, lamps, fins, the roof rocket, the nose's atom.
fn bus_trim(b: &mut MeshBuilder, body: &Body, k: &Kit, lod: Lod) {
    windscreen(b, body, k, lod);
    let seg = lod.pick(12, 8, 4);
    let nose = |x: f64, y: f64| end_z(body, x, y, false);
    let tail = |x: f64, y: f64| -end_z(body, x, y, true);
    let w = |z: f64, y: f64| body.shape(z).x_at(y);
    // Bumpers wrapped round both ends.
    for (yb, front) in [(0.6, true), (0.62, false)] {
        let n = lod.pick(15, 9, 5);
        let path: Vec<DVec3> = (0..=n)
            .map(|i| {
                let x = (-1.0 + 2.0 * i as f64 / n as f64) * (BUS_W - 0.1);
                let z = if front {
                    nose(x, yb) + 0.09
                } else {
                    tail(x, yb) - 0.09
                };
                DVec3::new(x, yb, z)
            })
            .collect();
        tube(b, &path, (0.08, 0.1), seg, k.metal());
    }
    // Quad headlamps in chrome bezels.
    for sx in [-1.0, 1.0] {
        for x in [0.56, 0.84] {
            let y = 0.98;
            lamp_pod(
                b,
                lod,
                DVec3::new(sx * x, y, nose(sx * x, y) - 0.05),
                0.1,
                k,
                k.top,
            );
        }
    }
    if lod == Lod::Far {
        bus_fins(b, body, k, lod);
        return;
    }
    // The grille: chrome bars across the nose under the atom.
    let bars = lod.pick(4, 2, 0);
    for i in 0..bars {
        let y = 0.72 + 0.07 * i as f64;
        let path: Vec<DVec3> = (0..=8)
            .map(|j| {
                let x = -0.4 + 0.1 * j as f64;
                DVec3::new(x, y, nose(x, y) + 0.012)
            })
            .collect();
        tube(b, &path, (0.016, 0.016), 6, k.metal());
    }
    // Bullets in the bumper.
    for sx in [-0.42, 0.42] {
        let z = nose(sx, 0.62) + 0.12;
        lathe(
            b,
            DVec3::new(sx, 0.62, z),
            DVec3::Z,
            &[
                (0.0, -0.05),
                (0.085, -0.03),
                (0.08, 0.07),
                (0.05, 0.16),
                (0.0, 0.2),
            ],
            seg,
            |_| k.metal(),
        );
    }
    // The atom on the nose: a glowing core, three chrome orbits.
    let (ay, az) = (1.18, nose(0.0, 1.18) + 0.03);
    let c = DVec3::new(0.0, ay, az);
    lathe(
        b,
        c,
        DVec3::Z,
        &[(0.0, -0.02), (0.06, -0.01), (0.055, 0.03), (0.0, 0.06)],
        seg,
        |_| Kit::glow(rgb(120, 230, 255)),
    );
    for t in 0..3 {
        let a = PI * t as f64 / 3.0;
        let (sa, ca) = a.sin_cos();
        let ring: Vec<DVec3> = (0..20)
            .map(|i| {
                let u = TAU * i as f64 / 20.0;
                let (x, y) = (0.26 * u.cos(), 0.08 * u.sin());
                c + DVec3::new(x * ca - y * sa, x * sa + y * ca, 0.01)
            })
            .collect();
        tube(b, &spline(&ring, 1, true), (0.012, 0.012), 6, k.metal());
    }
    // Fluted chrome along the skirts, between the wheels.
    let flutes = lod.pick(3, 1, 0);
    for sx in [-1.0f64, 1.0] {
        let runs: Vec<[f64; 2]> = if sx < 0.0 {
            vec![[-2.6, -0.7], [0.7, 2.0]]
        } else {
            vec![[-2.6, 2.0]]
        };
        for run in runs {
            for i in 0..flutes {
                let y = 0.7 + 0.1 * i as f64;
                let n = ((run[1] - run[0]) / 0.5).ceil() as usize;
                let path: Vec<DVec3> = (0..=n)
                    .map(|j| {
                        let z = run[0] + (run[1] - run[0]) * j as f64 / n as f64;
                        DVec3::new(sx * (w(z, y) + 0.005), y, z)
                    })
                    .collect();
                tube(b, &path, (0.018, 0.022), 6, k.metal());
            }
        }
    }
    // Door frames: chrome round each opening on the kerb side.
    for (dz, dw) in BUS_DOORS {
        let x = -(w(dz, 1.5) + 0.01);
        let (za, zb) = (dz - dw * 0.5, dz + dw * 0.5);
        let frame = [
            DVec3::new(x, DOOR_Y[0], za),
            DVec3::new(x, DOOR_Y[1], za),
            DVec3::new(x, DOOR_Y[1], zb),
            DVec3::new(x, DOOR_Y[0], zb),
        ];
        tube(b, &frame, (0.02, 0.02), 6, k.metal());
        // Steps up into it.
        for (i, y) in [0.42, 0.6].into_iter().enumerate() {
            let depth = 0.32 - 0.12 * i as f64;
            slab(
                b,
                DVec3::new(-BUS_W + 0.1 + depth, y, dz),
                [DVec3::X, DVec3::Y],
                DVec3::new(depth, 0.03, dw * 0.5 - 0.03),
                rgb(60, 60, 64),
            );
        }
    }
    // Bug-eye mirrors on arms at the front corners.
    if lod == Lod::Near {
        for sx in [-1.0, 1.0] {
            let root = DVec3::new(sx * 1.08, 2.25, 4.75);
            let tip = DVec3::new(sx * 1.42, 2.05, 5.05);
            tube(
                b,
                &[root, root + DVec3::new(sx * 0.2, 0.05, 0.15), tip],
                (0.02, 0.02),
                8,
                k.metal(),
            );
            lathe(
                b,
                tip + DVec3::new(0.0, -0.05, 0.0),
                -DVec3::Z,
                &[
                    (0.0, -0.05),
                    (0.09, -0.04),
                    (0.1, 0.02),
                    (0.07, 0.06),
                    (0.0, 0.07),
                ],
                12,
                |_| k.metal(),
            );
        }
        // Engine louvres on the tail.
        for i in 0..4 {
            let y = 0.86 + 0.09 * i as f64;
            let path: Vec<DVec3> = (0..=6)
                .map(|j| {
                    let x = -0.55 + 0.183 * j as f64;
                    DVec3::new(x, y, tail(x, y) - 0.01)
                })
                .collect();
            tube(b, &path, (0.014, 0.02), 6, k.metal());
        }
    }
    // The chrome rocket riding the roof, with a glowing nose cone.
    let roof = body.shape(0.0).top;
    lathe(
        b,
        DVec3::new(0.0, roof + 0.17, -3.3),
        DVec3::Z,
        &[
            (0.0, -0.1),
            (0.1, -0.05),
            (0.16, 0.3),
            (0.18, 1.2),
            (0.18, 3.6),
            (0.15, 4.6),
            (0.1, 5.1),
            (0.06, 5.35),
            (0.0, 5.45),
        ],
        lod.pick(20, 10, 6),
        |i| {
            if i >= 6 {
                Kit::glow(rgb(255, 120, 70))
            } else {
                k.metal()
            }
        },
    );
    // Its feet and three little fins.
    for z in [-2.6, 1.4] {
        slab(
            b,
            DVec3::new(0.0, roof + 0.03, z),
            [DVec3::X, DVec3::Y],
            DVec3::new(0.05, 0.08, 0.3),
            k.chrome,
        );
    }
    for a in [0.0f64, 2.1, -2.1] {
        let (sa, ca) = a.sin_cos();
        let up = DVec3::new(sa, ca, 0.0);
        let side = DVec3::new(ca, -sa, 0.0) * 0.012;
        let c = DVec3::new(0.0, roof + 0.17, 0.0);
        let tri = [
            c + up * 0.16 + DVec3::Z * -2.5,
            c + up * 0.17 + DVec3::Z * -3.25,
            c + up * 0.46 + DVec3::Z * -3.5,
        ];
        for sg in [-1.0, 1.0] {
            let pts = tri.map(|p| p + side * sg);
            b.polygon(&pts, side * sg, k.body);
        }
    }
    bus_fins(b, body, k, lod);
}

/// Tail fins on the roof's back corners, rocket tail lamps at their ends.
fn bus_fins(b: &mut MeshBuilder, body: &Body, k: &Kit, lod: Lod) {
    let base = |z: f64| body.shape(z.max(-5.1)).y_at(1.0);
    for sx in [-1.0, 1.0] {
        fin(
            b,
            lod,
            sx * 1.0,
            [-2.4, -5.12],
            0.5,
            0.07,
            0.18,
            &base,
            Kit::plain(k.body),
        );
        let at = DVec3::new(sx * 1.04, base(-5.1) + 0.22, -5.05);
        lathe(
            b,
            at,
            -DVec3::Z,
            &[
                (0.0, -0.1),
                (0.07, -0.08),
                (0.08, 0.04),
                (0.06, 0.14),
                (0.03, 0.2),
                (0.0, 0.22),
            ],
            lod.pick(14, 8, 5),
            |i| if i < 2 { k.metal() } else { Kit::glow(k.red) },
        );
        // Low tail lamps by the bumper.
        lamp_pod_rear(
            b,
            lod,
            DVec3::new(sx * 0.9, 0.95, -end_z(body, 0.9, 0.95, true) + 0.05),
            k,
        );
    }
}

fn lamp_pod_rear(b: &mut MeshBuilder, lod: Lod, at: DVec3, k: &Kit) {
    lathe(
        b,
        at,
        -DVec3::Z,
        &[
            (0.0, -0.1),
            (0.09, -0.1),
            (0.09, 0.0),
            (0.08, 0.03),
            (0.06, 0.05),
            (0.0, 0.06),
        ],
        lod.pick(14, 8, 5),
        |i| if i < 3 { k.metal() } else { Kit::glow(k.red) },
    );
}

/// The inside: its lining, the floor, wheel boxes, seats in rows, the
/// driver's seat, dash and wheel, and chrome poles.
fn bus_inside(b: &mut MeshBuilder, body: &Body, k: &Kit, lod: Lod) {
    let lining = rgb(222, 214, 196);
    let ceiling = rgb(240, 236, 226);
    let floor = rgb(70, 66, 64);
    // Coarser than the outside: the lining is seen only through the glass.
    let coarse = if lod == Lod::Near { Lod::Mid } else { Lod::Far };
    inside(|| {
        body.build(b, coarse, |at| {
            let y = height_of(body, &at);
            if y < BUS_FLOOR - 0.05 || at.well {
                return None;
            }
            // No lining where glass or a door is.
            let side = at.c.abs() > 0.25;
            let glazed = side && y > WIN[0] - 0.1 && y < WIN[1] + 0.1;
            if at.c < -0.3 && in_door(at.z) && y < DOOR_Y[1] + 0.1
                || glazed && at.z > WIN_Z[0] && at.z < WIN_Z[1]
                || at.z > WIN_Z[1] - 0.1 && y > SCREEN[0] - 0.16 && y < SCREEN[1] + 0.16
                || y > SKY[0] - 0.08 && y < SKY[1] + 0.08
            {
                return None;
            }
            Kit::plain(if y > 2.5 { ceiling } else { lining })
        })
    });
    // The floor.
    let fz = [-5.3, 5.05];
    let fw = body.sec(0.0).x_at(BUS_FLOOR) - 0.04;
    slab(
        b,
        DVec3::new(0.0, BUS_FLOOR - 0.02, (fz[0] + fz[1]) * 0.5),
        [DVec3::X, DVec3::Y],
        DVec3::new(fw, 0.02, (fz[1] - fz[0]) * 0.5),
        floor,
    );
    // Wheel boxes over the arches.
    for z in BUS_WHEELS.1 {
        for sx in [-1.0, 1.0] {
            slab(
                b,
                DVec3::new(sx * (fw - 0.2), 0.96, z),
                [DVec3::X, DVec3::Y],
                DVec3::new(0.2, 0.2, 0.62),
                lining,
            );
        }
    }
    // Seats: a cushion and a back each, in the livery's colour.
    let seat = Kit::plain(k.body);
    let frame = rgb(170, 172, 178);
    let one = |b: &mut MeshBuilder, z: f64, x: f64, wide: f64| {
        slab(
            b,
            DVec3::new(x, (BUS_FLOOR + BUS_CUSHION - 0.1) * 0.5, z),
            [DVec3::X, DVec3::Y],
            DVec3::new(wide * 0.6, (BUS_CUSHION - 0.1 - BUS_FLOOR) * 0.5, 0.04),
            frame,
        );
        cushion(
            b,
            coarse,
            DVec3::new(x, BUS_CUSHION - 0.06, z),
            DVec3::new(wide, 0.06, 0.22),
            0.0,
            seat,
        );
        cushion(
            b,
            coarse,
            DVec3::new(x, BUS_CUSHION + 0.32, z - 0.22),
            DVec3::new(wide, 0.34, 0.05),
            -0.14,
            seat,
        );
    };
    for (z, x) in seat_spots() {
        one(b, z, x, 0.21);
    }
    one(b, DRIVER_Z, 0.62, 0.24);
    // The dash and the big wheel.
    let (dz, dy) = (4.75, 1.4);
    cushion(
        b,
        lod,
        DVec3::new(0.25, dy, dz),
        DVec3::new(0.85, 0.09, 0.16),
        0.3,
        Kit::plain(k.top),
    );
    if lod == Lod::Near {
        steering_wheel(b, DVec3::new(0.62, 1.55, 4.38), 0.24, k);
        // Poles to hold on to.
        for z in [3.35, 0.75, -0.75, -3.0] {
            for x in [0.2, -0.2] {
                tube(
                    b,
                    &[
                        DVec3::new(x, BUS_FLOOR, z),
                        DVec3::new(x, bus_roof_over(x, z), z),
                    ],
                    (0.02, 0.02),
                    8,
                    k.metal(),
                );
            }
        }
    }
}

/// One leaf of door `door` of a bus ([`BUS_DOORS`]), shut, in its own
/// frame: its middle at the origin (x across the bus, the outside to -x),
/// half its door wide. Glazed above, the livery's colours below. Leaves
/// stand at [`bus_door_leaves`] and slide apart to open.
pub fn bus_door_leaf(livery: u8, door: usize, lod: Lod) -> Mesh {
    let k = bus_kit(livery);
    let mut b = MeshBuilder::new();
    let half = BUS_DOORS[door % BUS_DOORS.len()].1 * 0.25 - 0.012;
    let t = 0.025;
    let bands: [(f64, f64, Paint); 5] = [
        (DOOR_Y[0], BELT[0], Kit::plain(k.body)),
        (BELT[0], BELT[1], k.metal()),
        (BELT[1], WIN[0] - 0.05, Kit::plain(k.top)),
        (WIN[0] - 0.05, WIN[1] - 0.05, k.glazed()),
        (WIN[1] - 0.05, DOOR_Y[1] - 0.02, Kit::plain(k.top)),
    ];
    for (y0, y1, p) in bands {
        let (y0, y1) = (y0 - DOOR_Y[0] - 1.0, y1 - DOOR_Y[0] - 1.0);
        let Some((surf, col)) = p else {
            continue;
        };
        b.surf = surf;
        b.hexa(
            [
                DVec3::new(-t, y0, -half),
                DVec3::new(t, y0, -half),
                DVec3::new(t, y0, half),
                DVec3::new(-t, y0, half),
            ],
            [
                DVec3::new(-t, y1, -half),
                DVec3::new(t, y1, -half),
                DVec3::new(t, y1, half),
                DVec3::new(-t, y1, half),
            ],
            col,
        );
    }
    b.surf = Surf::Plain;
    if lod != Lod::Far {
        // A chrome edge and a grab bar.
        let x = -t - 0.01;
        let y0 = -1.0;
        let y1 = DOOR_Y[1] - DOOR_Y[0] - 1.02;
        tube(
            &mut b,
            &[
                DVec3::new(x, y0, -half),
                DVec3::new(x, y1, -half),
                DVec3::new(x, y1, half),
                DVec3::new(x, y0, half),
            ],
            (0.012, 0.012),
            6,
            k.metal(),
        );
    }
    b.finish()
}

/// Where each door leaf of a bus stands, in its frame, its doors `open`
/// (0 shut, 1 open): (door, place). The leaves of each door slide apart
/// along the side. A leaf's origin is a metre over the opening's bottom.
pub fn bus_door_leaves(open: f64) -> Vec<(usize, DVec3)> {
    let open = open.clamp(0.0, 1.0);
    let mut out = Vec::new();
    for (door, (dz, w)) in BUS_DOORS.into_iter().enumerate() {
        let x = -(bus_section(dz).x_at(1.5) + 0.03 + 0.06 * open.min(0.15) / 0.15);
        let quarter = w * 0.25;
        for sz in [-1.0, 1.0] {
            let slide = sz * (quarter + quarter * 1.7 * open);
            out.push((door, DVec3::new(x, DOOR_Y[0] + 1.0, dz + slide)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bus_seats_have_headroom_and_stand_on_the_floor() {
        for s in bus_seats() {
            assert!(s.hip.y > BUS_FLOOR + 0.4);
            assert!(bus_roof_over(s.hip.x, s.hip.z) - s.hip.y > 1.0);
            assert!(s.hip.x.abs() < BUS_W - 0.3);
        }
    }

    #[test]
    fn the_nose_reaches_forward_of_the_body() {
        let body = bus_body();
        let z = end_z(&body, 0.0, 1.2, false);
        assert!(z > 5.0 && z <= BUS_HALF_LEN + 1e-6, "{z}");
        let r = end_z(&body, 0.0, 1.2, true);
        assert!(r > 5.0 && r <= BUS_HALF_LEN + 1e-6, "{r}");
    }
}
