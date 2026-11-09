//! The buried city: here and there under the land, a piece of an older,
//! Aether-lit town lies sealed in the rock: a colonnaded street with halls
//! off it, and a domed vault at its end with a great round door nobody has
//! opened. An ancient lift still runs down to the Deeps.

use super::{
    datum, lair::LAIR_REACH, name, overlaps, room, run, tile_centre, DeepLamp, DeepSpot, Fixture,
    FixtureKind, Plan, Ruin, SpotKind, RUIN_DEPTH,
};
use crate::urban::underground::{Void, VoidKind};
use glam::{DVec2, DVec3};

/// Share of tiles with a ruin.
const CHANCE: f64 = 0.3;

pub(super) fn plan_tile(plan: &mut Plan) {
    let w = plan.w;
    let seed = plan.seed.child("ruin");
    let mut r = seed.rng();
    if !r.chance(CHANCE) {
        return;
    }
    let centre = tile_centre(plan.key());
    let o = centre + DVec2::new(r.range(-35.0, 35.0), r.range(-35.0, 35.0));
    let dir = hw_core::det::dir2(r.range(0.0, std::f64::consts::TAU));
    if w.lair_site()
        .is_some_and(|l| l.centre.distance(o) < LAIR_REACH)
    {
        return;
    }
    let net = w.tube();
    // Stations' lifts to the Deeps run down through this depth.
    if net
        .stations
        .iter()
        .any(|s| s.hall_centre().distance(o) < 140.0)
    {
        return;
    }
    let f = datum(w, o) - RUIN_DEPTH;
    let len = r.range(60.0, 84.0);
    let half_len = len * 0.5;
    let at = |x: f64, y: f64| o + dir * x + dir.perp() * y;
    let k = VoidKind::Ruin;
    let mut voids: Vec<Void> = Vec::new();
    let street = voids.len();
    voids.push(room(k, o, dir, DVec2::new(half_len, 7.0), f, 11.0));
    // Halls off the street, on alternate sides.
    let n = r.range_int(2, 5) as usize;
    let mut halls = Vec::new();
    let mut hall_at = Vec::new();
    for i in 0..n {
        let x = -half_len + len * (i as f64 + 0.5) / n as f64;
        let s = if i % 2 == 0 { 1.0 } else { -1.0 };
        let hy = r.range(5.0, 6.5);
        let hx = r.range(6.0, 8.0).min(len / n as f64 * 0.5 - 1.0).max(4.5);
        let c = at(x, s * (7.0 + 2.0 + hy));
        voids.extend(run(k, at(x, s * 6.0), at(x, s * 9.5), 1.8, f, f, 5.0));
        halls.push(voids.len());
        hall_at.push((x, s, hy, hx));
        voids.push(room(k, c, dir, DVec2::new(hx, hy), f, 8.0));
    }
    // The domed vault at the far end, through an arch.
    voids.extend(run(
        k,
        at(half_len - 1.0, 0.0),
        at(half_len + 4.0, 0.0),
        3.0,
        f,
        f,
        7.0,
    ));
    let rot_x = half_len + 3.0 + 11.0;
    let rotunda = voids.len();
    voids.push(room(
        k,
        at(rot_x, 0.0),
        dir,
        DVec2::new(11.0, 11.0),
        f,
        12.0,
    ));
    let tubes = net.stretch_voids_near(o, 140.0);
    let ok = voids.iter().all(|v| {
        plan.roofed(v, 3.0)
            && plan.clear_of_shallow(v, 4.0)
            && tubes.iter().all(|(_, _, t)| !overlaps(v, t, 2.0))
    });
    if !ok {
        return;
    }
    let base = plan.t.voids.len();
    plan.t.voids.extend(voids);
    const FIRST: &[&str] = &["Lost", "Sunken", "Forgotten", "Buried", "Old", "Sleeping"];
    const SECOND: &[&str] = &[
        "Aethera", "Lumeria", "Kesh", "Orivane", "Talos", "Zanthar", "Ilios", "Mora",
    ];
    plan.t.ruins.push(Ruin {
        name: name(seed.child("name"), FIRST, SECOND),
        street: base + street,
        halls: halls.iter().map(|h| base + h).collect(),
        rotunda: base + rotunda,
        floor: f,
    });
    let p3 = |x: f64, y: f64| {
        let q = at(x, y);
        DVec3::new(q.x, f, q.y)
    };
    let mut fixtures = Vec::new();
    // Columns down both sides, some broken, a few fallen across the way.
    let cols = (len / 5.0).floor() as usize;
    for i in 0..=cols {
        let x = -half_len + 2.5 + (len - 5.0) * i as f64 / cols.max(1) as f64;
        for s in [1.0, -1.0] {
            let y = s * 5.6;
            let roll = r.unit();
            if roll < 0.62 {
                fixtures.push(Fixture {
                    kind: FixtureKind::Column,
                    at: p3(x, y),
                    dir,
                    size: DVec3::new(0.62, 11.0, 0.0),
                    look: (i * 2) as u32,
                });
            } else {
                let h = r.range(1.4, 5.5);
                fixtures.push(Fixture {
                    kind: FixtureKind::BrokenColumn,
                    at: p3(x, y),
                    dir,
                    size: DVec3::new(0.62, h, 0.0),
                    look: (i * 2 + 1) as u32,
                });
                if roll > 0.85 {
                    // The rest of it lies along the street's edge.
                    let l = r.range(3.0, 5.0);
                    fixtures.push(Fixture {
                        kind: FixtureKind::FallenColumn,
                        at: p3(x + l * 0.5 + 0.9, s * 4.4),
                        dir: dir.perp(),
                        size: DVec3::new(l * 0.5, 1.2, 0.6),
                        look: i as u32,
                    });
                }
            }
        }
    }
    // Rubble heaps between the columns.
    for i in 0..r.range_int(3, 7) {
        let x = r.range(-half_len + 4.0, half_len - 4.0);
        let s = if i % 2 == 0 { 1.0 } else { -1.0 };
        let y = s * r.range(3.6, 4.4);
        let sz = DVec3::new(r.range(0.8, 1.6), r.range(0.5, 1.2), r.range(0.6, 0.9));
        let clear = fixtures
            .iter()
            .all(|f: &Fixture| f.plan().distance(at(x, y)) > sz.x + 1.6);
        if clear {
            fixtures.push(Fixture {
                kind: FixtureKind::Rubble,
                at: p3(x, y),
                dir: hw_core::det::dir2(r.range(0.0, std::f64::consts::TAU)),
                size: sz,
                look: i as u32,
            });
        }
    }
    // A colossal stone head half sunk at the street's near end.
    fixtures.push(Fixture {
        kind: FixtureKind::GiantHead,
        at: p3(-half_len + 6.0, 0.0),
        dir: dir.rotate(hw_core::det::dir2(r.range(-0.6, 0.6))),
        size: DVec3::new(2.2, 3.6, 0.0),
        look: 0,
    });
    // Aether braziers in the halls' back corners.
    for (k, &(x, s, hy, hx)) in hall_at.iter().enumerate() {
        for e in [-1.0, 1.0] {
            let p = (x + e * (hx - 1.4), s * (9.0 + hy * 2.0 - 1.4));
            fixtures.push(Fixture {
                kind: FixtureKind::Brazier,
                at: p3(p.0, p.1),
                dir,
                size: DVec3::new(0.55, 1.3, 0.0),
                look: k as u32,
            });
            let q = at(p.0, p.1);
            plan.t.lamps.push(DeepLamp {
                pos: DVec3::new(q.x, f + 2.0, q.y),
                color: [0.3, 1.0, 0.85],
                strength: 4.5,
                range: 10.0,
            });
        }
    }
    // A ring of columns under the dome, and the vault door in its back wall.
    for i in 0..8 {
        let a = std::f64::consts::TAU * i as f64 / 8.0 + std::f64::consts::FRAC_PI_8;
        let d = hw_core::det::dir2(a) * 8.0;
        fixtures.push(Fixture {
            kind: FixtureKind::Column,
            at: p3(rot_x + d.x, d.y),
            dir,
            size: DVec3::new(0.5, 7.4, 0.0),
            look: 100 + i,
        });
    }
    fixtures.push(Fixture {
        kind: FixtureKind::VaultDoor,
        at: p3(rot_x + 10.95, 0.0),
        dir: -dir,
        size: DVec3::new(2.6, 0.0, 0.0),
        look: 0,
    });
    // The glyph bands glow faintly down the street; the dome's eye glows.
    let bands = (len / 12.0).ceil() as usize;
    for i in 0..bands {
        let x = -half_len + 6.0 + (len - 12.0) * i as f64 / bands.max(2).saturating_sub(1) as f64;
        let q = at(x, 0.0);
        plan.t.lamps.push(DeepLamp {
            pos: DVec3::new(q.x, f + 7.5, q.y),
            color: [0.35, 0.95, 0.9],
            strength: 3.5,
            range: 13.0,
        });
    }
    let q = at(rot_x, 0.0);
    plan.t.lamps.push(DeepLamp {
        pos: DVec3::new(q.x, f + 10.5, q.y),
        color: [0.5, 1.0, 0.9],
        strength: 7.0,
        range: 16.0,
    });
    plan.t.fixtures.extend(fixtures);
    plan.t.spots.push(DeepSpot {
        kind: SpotKind::RuinProwler,
        at: p3(0.0, 0.0),
        facing: dir,
    });
    plan.t.spots.push(DeepSpot {
        kind: SpotKind::RuinProwler,
        at: p3(rot_x - 2.0, 0.0),
        facing: dir,
    });
    // The ancient lift, in the last hall.
    if let Some(&(x, s, hy, _)) = hall_at.last() {
        plan.lift_to_deeps(at(x, s * (9.0 + hy)), f);
    }
}
