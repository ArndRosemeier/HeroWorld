//! The Sky Patrol cruiser: the police's flying car. A long black-and-white
//! hull with a needle prow behind a chrome push bar, a bubble canopy with
//! a red beacon and two chrome siren horns on top, gold stars on its
//! doors and its bonnet, two jet nacelles with intake spikes, a twin tail
//! with red rocket lamps, searchlights at the canopy's shoulders and
//! chrome landing skids. It shares the hover car's cabin (two rows of
//! seats under the same canopy), so officers sit in it as riders do.
//!
//! Same frame as the cars: nose to +z, +y up; landed, its skids stand on
//! y = 0 (the hull hangs [`DROP`] lower than the hover car's).

use super::*;

/// The box a landed patrol cruiser fills: half its width (nacelles and
/// all), where its middle is along its length, half its length, and its
/// height to the top of its fins (the beacon stands a little higher).
pub const PATROL_BLOCK: (f64, f64, f64, f64) = (
    hw_sim::police::CRUISER_HALF.x,
    hw_sim::police::CRUISER_MID,
    hw_sim::police::CRUISER_HALF.y,
    hw_sim::police::CRUISER_HEIGHT,
);
/// The hull hangs this much lower than the hover car's: landed, it sits
/// on short skids.
const DROP: f64 = 0.36;

const BLACK: (u8, u8, u8) = (17, 18, 23);
const WHITE: (u8, u8, u8) = (240, 240, 234);
const GOLD: (u8, u8, u8) = (232, 184, 62);
/// The white door panel along the flanks and over the deck (z).
const PANEL: [f64; 2] = [-1.05, 1.3];

/// The hover car's hull, drawn out into a needle prow.
fn patrol_section(z: f64) -> Section {
    let s = hover_section(z);
    let e = smooth(1.9, 3.05, z);
    Section {
        w: s.w * (1.0 - 0.72 * e),
        top: s.top - 0.07 * e,
        bot: s.bot + 0.08 * e,
        ..s
    }
}

fn kit() -> Kit {
    Kit {
        body: rgb(BLACK.0, BLACK.1, BLACK.2),
        top: rgb(WHITE.0, WHITE.1, WHITE.2),
        seam: rgb(8, 8, 10),
        ..Kit::new(0)
    }
}

fn gold() -> Paint {
    Some((Surf::Metal, rgb(GOLD.0, GOLD.1, GOLD.2)))
}

/// A five-pointed star of radius `r`: its points in a plane, the first
/// straight "up" (+v), as (u, v) pairs, centre first.
fn star(r: f64) -> Vec<DVec2> {
    let mut out = vec![DVec2::ZERO];
    for k in 0..=10 {
        let a = FRAC_PI_2 + TAU * k as f64 / 10.0;
        let rr = if k % 2 == 0 { r } else { r * 0.4 };
        out.push(DVec2::new(a.cos() * rr, a.sin() * rr));
    }
    out
}

/// Lays a star as a fan of triangles, each corner put on the car by `put`.
fn lay_star(b: &mut MeshBuilder, r: f64, n: DVec3, put: impl Fn(DVec2) -> DVec3) {
    let pts: Vec<DVec3> = star(r).into_iter().map(put).collect();
    b.surf = Surf::Metal;
    for w in pts[1..].windows(2) {
        b.tri([pts[0], w[0], w[1]], n, rgb(GOLD.0, GOLD.1, GOLD.2));
    }
    b.surf = Surf::Plain;
}

/// The beacon on the canopy: a red dome on a chrome saddle between two
/// chrome siren horns. It rides the canopy (and tilts up with it).
fn beacon(b: &mut MeshBuilder, k: &Kit, lod: Lod) {
    let top = hover_canopy_base(0.15) + HOVER_CANOPY;
    let seg = lod.pick(24, 12, 6);
    tube(
        b,
        &[
            DVec3::new(-0.36, top + 0.005, 0.15),
            DVec3::new(0.36, top + 0.005, 0.15),
        ],
        (0.09, 0.03),
        lod.pick(12, 6, 4),
        k.metal(),
    );
    lathe(
        b,
        DVec3::new(0.0, top + 0.02, 0.15),
        DVec3::Y,
        &[
            (0.0, 0.0),
            (0.135, 0.0),
            (0.14, 0.025),
            (0.125, 0.04),
            (0.125, 0.11),
            (0.11, 0.17),
            (0.075, 0.215),
            (0.03, 0.238),
            (0.022, 0.25),
            (0.012, 0.3),
            (0.0, 0.31),
        ],
        seg,
        |i| match i {
            0..=2 => k.metal(),
            3..=6 => Kit::glow(rgb(255, 40, 30)),
            _ => k.metal(),
        },
    );
    if lod != Lod::Near {
        return;
    }
    // The sirens: two trumpets facing ahead.
    for sx in [-1.0, 1.0] {
        lathe(
            b,
            DVec3::new(sx * 0.27, top + 0.1, -0.02),
            DVec3::Z,
            &[
                (0.0, 0.0),
                (0.04, 0.0),
                (0.045, 0.03),
                (0.032, 0.1),
                (0.04, 0.19),
                (0.07, 0.27),
                (0.085, 0.29),
                (0.088, 0.3),
                (0.07, 0.285),
                (0.03, 0.2),
                (0.0, 0.18),
            ],
            lod.pick(18, 10, 6),
            |i| match i {
                8.. => Kit::plain(rgb(20, 20, 22)),
                _ => k.metal(),
            },
        );
        tube(
            b,
            &[
                DVec3::new(sx * 0.27, top + 0.0, 0.1),
                DVec3::new(sx * 0.27, top + 0.07, 0.1),
            ],
            (0.018, 0.018),
            6,
            k.metal(),
        );
    }
}

/// Where the beacon's red dome glows, on the cruiser with its canopy shut.
pub fn patrol_beacon() -> DVec3 {
    DVec3::new(
        0.0,
        hover_canopy_base(0.15) + HOVER_CANOPY + 0.14 - DROP,
        0.15,
    )
}

/// The patrol cruiser's canopy with its beacon, as its own model in its
/// hinge's frame (see [`taxi_door`]): the mesh, the hinge's place on the
/// car and its axis.
pub fn patrol_door(lod: Lod) -> (Mesh, DVec3, DVec3) {
    let k = kit();
    let mut part = MeshBuilder::new();
    let hinge = hover_canopy(&mut part, &k, lod, false);
    beacon(&mut part, &k, lod);
    let mut b = MeshBuilder::new();
    b.append(part, -hinge.as_vec3());
    (b.finish(), hinge - DVec3::Y * DROP, DVec3::X)
}

/// The Sky Patrol cruiser. `canopy` false leaves its canopy and beacon
/// off, for one whose canopy is drawn on its own by [`patrol_door`].
pub fn patrol_into(out: &mut MeshBuilder, lod: Lod, canopy: bool) {
    let mut part = MeshBuilder::new();
    build(&mut part, lod, canopy);
    out.append(part, glam::Vec3::new(0.0, -DROP as f32, 0.0));
}

fn build(b: &mut MeshBuilder, lod: Lod, canopy: bool) {
    let k = kit();
    let body = Body {
        z: [-2.35, 3.05],
        end_r: [0.18, 0.16],
        section: &patrol_section,
        arches: vec![],
        stations: vec![PANEL[0], PANEL[1], -0.95, 0.85, -2.15, 2.2],
        bands: vec![-0.9, -0.55, 0.26, 0.32, 0.6, 0.65, HOVER_HOLE],
    };
    let hole = hover_hole(lod);
    body.build(b, lod, |at| {
        if hole(&at) {
            return None;
        }
        if at.s < -0.9 || at.well {
            return Kit::plain(k.under);
        }
        let panel = at.z > PANEL[0] && at.z < PANEL[1];
        // A gold line along the black of the hull, a chrome one round the deck.
        // (The gold line stops at the doors: they carry the star.)
        if at.side() && at.s > 0.26 && at.s < 0.32 && at.z > -2.15 && at.z < 2.2 && !panel {
            return gold();
        }
        if at.s > 0.6 && at.s < 0.65 && at.z > -2.15 && at.z < 2.2 {
            return k.metal();
        }
        // White doors and a white deck round the canopy; black ends.
        if panel && (at.s > 0.65 || (at.side() && at.s > -0.55)) {
            return Kit::plain(k.top);
        }
        Kit::plain(k.body)
    });
    let deck = |z: f64, x: f64| body.shape(z).y_at(x);
    hover_interior(b, &body, &k, lod);
    if canopy {
        hover_canopy(b, &k, lod, false);
        beacon(b, &k, lod);
    }

    // The stars: one on each door, a big one on the bonnet.
    if lod != Lod::Far {
        for sx in [-1.0, 1.0] {
            let c = DVec2::new(0.15, 1.02);
            lay_star(b, 0.15, DVec3::X * sx, |p| {
                let (z, y) = (c.x + p.x * sx, c.y + p.y);
                // Out along the flank's own slope there, so no point of
                // it sinks into the hull where the side curves under.
                let sec = body.sec(z);
                let slope = (sec.x_at(y + 0.02) - sec.x_at(y - 0.02)) / 0.04;
                let n = DVec2::new(1.0, -slope).normalize();
                DVec3::new(sx * (sec.x_at(y) + n.x * 0.008), y + n.y * 0.008, z)
            });
        }
        lay_star(b, 0.24, DVec3::Y, |p| {
            let (x, z) = (p.x, 1.82 + p.y);
            DVec3::new(x, deck(z, x) + 0.006, z)
        });
    }

    // The engines: a jet nacelle each side on a short pylon, a chrome
    // spike in its intake, its tail glowing.
    let seg = lod.pick(24, 12, 6);
    for sx in [-1.0, 1.0] {
        let c = DVec3::new(sx * 1.1, 0.92, -1.3);
        lathe(
            b,
            c,
            DVec3::Z,
            &[
                (0.0, -1.1),
                (0.15, -1.1),
                (0.19, -1.06),
                (0.235, -0.85),
                (0.262, -0.45),
                (0.262, 0.2),
                (0.245, 0.6),
                (0.215, 0.82),
                (0.2, 0.87),
                (0.17, 0.86),
                (0.13, 0.72),
                (0.0, 0.72),
            ],
            seg,
            |i| match i {
                0 => Kit::glow(rgb(255, 150, 70)),
                1 | 2 => k.metal(),
                3 => Kit::plain(k.body),
                4 | 5 => Kit::plain(k.top),
                6 => Kit::plain(k.body),
                7 | 8 => k.metal(),
                _ => Kit::plain(rgb(12, 12, 14)),
            },
        );
        lathe(
            b,
            c + DVec3::Z * 0.7,
            DVec3::Z,
            &[
                (0.0, 0.0),
                (0.075, 0.0),
                (0.06, 0.12),
                (0.025, 0.3),
                (0.0, 0.36),
            ],
            lod.pick(16, 8, 5),
            |_| k.metal(),
        );
        if lod != Lod::Far {
            for z in [-1.55, -0.95] {
                tube(
                    b,
                    &[
                        DVec3::new(sx * (body.sec(z).w - 0.12), 0.97, z),
                        DVec3::new(sx * 0.92, 0.93, z - 0.02),
                    ],
                    (0.05, 0.1),
                    lod.pick(12, 8, 4),
                    Kit::plain(k.body),
                );
            }
        }
        if lod == Lod::Near {
            // A gold band round the nacelle.
            lathe(
                b,
                c,
                DVec3::Z,
                &[(0.264, 0.05), (0.266, 0.06), (0.266, 0.12), (0.264, 0.13)],
                seg,
                |_| gold(),
            );
        }
        // A canted fin on the nacelle's tail.
        let nb = |_z: f64| 1.12;
        fin(
            b,
            lod,
            sx * 1.1,
            [-1.6, -2.3],
            0.3,
            0.03,
            0.7,
            &nb,
            Kit::plain(k.top),
        );
    }

    // The twin tail: two tall fins leaning out, a red rocket lamp at each
    // one's tip and at its foot.
    for sx in [-1.0, 1.0] {
        let x = sx * 0.4;
        let base = |z: f64| deck(z.max(-2.25), 0.4);
        fin(
            b,
            lod,
            x,
            [-0.95, -2.5],
            0.58,
            0.045,
            0.38,
            &base,
            Kit::plain(k.top),
        );
        let tip = DVec3::new(x + sx * 0.38 * 0.6, base(-2.5) + 0.5, -2.48);
        lathe(
            b,
            tip,
            -DVec3::Z,
            &[
                (0.0, -0.16),
                (0.035, -0.12),
                (0.05, -0.02),
                (0.052, 0.0),
                (0.045, 0.03),
                (0.02, 0.09),
                (0.0, 0.1),
            ],
            lod.pick(14, 8, 5),
            |i| match i {
                0..=2 => k.metal(),
                _ => Kit::glow(k.red),
            },
        );
        let z = -2.26;
        let y = body.sec(z).mid + 0.02;
        lathe(
            b,
            DVec3::new(sx * 0.3, y, z - 0.02),
            -DVec3::Z,
            &[
                (0.0, -0.12),
                (0.085, -0.12),
                (0.095, 0.0),
                (0.09, 0.03),
                (0.07, 0.05),
                (0.03, 0.11),
                (0.0, 0.12),
            ],
            lod.pick(18, 10, 6),
            |i| match i {
                0 => Kit::plain(k.body),
                1 | 2 => k.metal(),
                _ => Kit::glow(k.red),
            },
        );
    }

    // The blue glow it rides on.
    b.surf = Surf::Glow;
    disc(
        b,
        DVec3::new(0.0, body.sec(0.0).bot - 0.012, 0.1),
        (DVec3::X * 0.7, DVec3::Z * 1.9),
        lod.pick(32, 16, 8),
        -DVec3::Y,
        rgb(110, 190, 255),
    );
    b.surf = Surf::Plain;

    // Headlamps in the prow, either side of a red pursuit lamp.
    for sx in [-1.0, 1.0] {
        let z = 2.5;
        let y = body.sec(z).mid + 0.02;
        let x = body.sec(z).x_at(y) * 0.62;
        lamp_pod(b, lod, DVec3::new(sx * x, y, z + 0.16), 0.085, &k, k.body);
    }
    if lod == Lod::Far {
        return;
    }
    let mid = body.sec(2.9).mid + 0.05;
    lathe(
        b,
        DVec3::new(0.0, mid + 0.1, 2.72),
        DVec3::Z,
        &[
            (0.0, -0.1),
            (0.055, -0.1),
            (0.062, 0.0),
            (0.05, 0.03),
            (0.02, 0.07),
            (0.0, 0.075),
        ],
        lod.pick(16, 8, 5),
        |i| match i {
            0 | 1 => k.metal(),
            _ => Kit::glow(k.red),
        },
    );

    // The push bar round the prow, with two chrome bullets.
    let bar_y = body.sec(2.6).mid - 0.02;
    let zs = [2.25, 2.5, 2.72, 2.9, 3.0];
    let side = |sx: f64| -> Vec<DVec3> {
        zs.iter()
            .map(|&z| DVec3::new(sx * (body.sec(z).x_at(bar_y) + 0.07), bar_y, z + 0.05))
            .collect()
    };
    let mut path = side(1.0);
    path.push(DVec3::new(0.0, bar_y, 3.2));
    path.extend(side(-1.0).into_iter().rev());
    let path = spline(&path, lod.pick(5, 2, 1), false);
    tube(b, &path, (0.04, 0.05), lod.pick(12, 6, 4), k.metal());
    for sx in [-1.0, 1.0].into_iter().filter(|_| lod == Lod::Near) {
        lathe(
            b,
            DVec3::new(sx * 0.17, bar_y, 3.08),
            DVec3::Z,
            &[
                (0.0, -0.06),
                (0.05, -0.06),
                (0.055, 0.02),
                (0.035, 0.1),
                (0.0, 0.16),
            ],
            lod.pick(14, 8, 5),
            |_| k.metal(),
        );
    }

    // Searchlights at the canopy's shoulders, on chrome stalks.
    for sx in [-1.0, 1.0] {
        let foot = DVec3::new(sx * 0.7, deck(1.0, 0.7) - 0.02, 1.0);
        let head = foot + DVec3::new(sx * 0.04, 0.2, 0.02);
        tube(
            b,
            &[foot, head],
            (0.018, 0.018),
            lod.pick(8, 4, 4),
            k.metal(),
        );
        lamp_pod(
            b,
            lod,
            head + DVec3::new(0.0, 0.03, 0.12),
            0.07,
            &k,
            k.chrome,
        );
    }

    // The landing skids: a chrome runner each side on two struts.
    for sx in [-1.0, 1.0] {
        let x = sx * 0.6;
        let run = spline(
            &[
                DVec3::new(x, DROP + 0.24, 1.62),
                DVec3::new(x, DROP + 0.09, 1.4),
                DVec3::new(x, DROP + 0.04, 1.1),
                DVec3::new(x, DROP + 0.04, -1.1),
                DVec3::new(x, DROP + 0.1, -1.36),
            ],
            lod.pick(5, 2, 1),
            false,
        );
        tube(b, &run, (0.04, 0.04), lod.pick(10, 5, 4), k.metal());
        for z in [0.85, -0.8] {
            tube(
                b,
                &[
                    DVec3::new(x, DROP + 0.05, z),
                    DVec3::new(sx * 0.46, body.sec(z).bot + 0.06, z),
                ],
                (0.03, 0.03),
                lod.pick(8, 4, 4),
                k.metal(),
            );
        }
    }
    if lod == Lod::Near {
        // A long whip aerial on the tail.
        tube(
            b,
            &[
                DVec3::new(0.0, deck(-1.5, 0.0), -1.5),
                DVec3::new(0.0, 2.0, -1.95),
                DVec3::new(0.0, 2.4, -2.55),
            ],
            (0.005, 0.005),
            5,
            k.metal(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "on demand (looks): scripts/check.sh --on-demand"]
    fn the_patrol_cruiser_is_detailed_close_and_light_far() {
        let tris = |lod: Lod| {
            let mut b = MeshBuilder::new();
            patrol_into(&mut b, lod, true);
            b.finish().build().triangle_count()
        };
        let (near, mid, far) = (tris(Lod::Near), tris(Lod::Mid), tris(Lod::Far));
        assert!((5_000..=30_000).contains(&near), "close up: {near}");
        assert!(mid * 3 <= near, "mid level {mid} vs {near}");
        assert!(far <= 2_500, "far level has {far} triangles");
    }

    #[test]
    fn the_patrol_cruiser_stands_on_its_skids_inside_its_box() {
        let mut b = MeshBuilder::new();
        patrol_into(&mut b, Lod::Near, true);
        let (lo, hi) = b.finish().bounds().expect("bounds");
        assert!(lo.y.abs() < 0.02, "lowest point {}", lo.y);
        let (hw, mid, hl, _) = PATROL_BLOCK;
        assert!(hi.x as f64 <= hw && -lo.x as f64 <= hw, "{lo} {hi}");
        // The push bar's tip and the aerial may reach a little past it.
        assert!((hi.z as f64) < mid + hl + 0.2 && (lo.z as f64) > mid - hl - 0.2);
    }
}
