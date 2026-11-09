//! The Deeps: natural caves under the whole land. Every tile has a node,
//! most a grotto there, and winding galleries out to its edges, where they
//! meet the next tile's at the same spot and depth (the meeting points are
//! a function of the edge alone). The lifts from the layers above come down
//! into small chambers joined to the node.

use super::{
    deep_floor, room, run, tile_centre, DeepLamp, DeepSpot, Fixture, FixtureKind, Grotto,
    GrottoKind, Plan, SpotKind, TileKey, TILE,
};
use crate::urban::underground::VoidKind;
use crate::urban::world::TownWorld;
use glam::{DVec2, DVec3};
use hw_core::Seed;

/// Half width and height of a gallery where it crosses a tile edge.
pub const GALLERY_HALF: f64 = 2.8;
pub const GALLERY_HEIGHT: f64 = 5.5;
/// Share of tile edges a gallery crosses.
const OPEN: f64 = 0.72;
/// Share of nodes with a grotto.
const GROTTO: f64 = 0.8;

/// The spot where a gallery crosses the edge on the +x side (`east`) or
/// +y side of tile `k`, if one does.
pub fn portal(w: &TownWorld, k: TileKey, east: bool) -> Option<DVec2> {
    let other = if east { (k.0 + 1, k.1) } else { (k.0, k.1 + 1) };
    if !w.deep_in_land(k) || !w.deep_in_land(other) {
        return None;
    }
    let s = w
        .seed
        .child("deep-edge")
        .child(if east { "e" } else { "n" })
        .cell(k.0, k.1);
    let mut r = s.rng();
    if !r.chance(OPEN) {
        return None;
    }
    let t = r.range(0.2, 0.8);
    let p = if east {
        DVec2::new((k.0 + 1) as f64 * TILE, (k.1 as f64 + t) * TILE)
    } else {
        DVec2::new((k.0 as f64 + t) * TILE, (k.1 + 1) as f64 * TILE)
    };
    Some(p)
}

/// The four edge crossings of tile `k`.
fn portals(w: &TownWorld, k: TileKey) -> Vec<DVec2> {
    [
        portal(w, k, true),
        portal(w, k, false),
        portal(w, (k.0 - 1, k.1), true),
        portal(w, (k.0, k.1 - 1), false),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn node(seed: Seed, k: TileKey) -> DVec2 {
    let mut r = seed.child("node").rng();
    tile_centre(k) + DVec2::new(r.range(-55.0, 55.0), r.range(-55.0, 55.0))
}

/// A winding gallery from `a` to `b` through a bend, floors from the
/// datum, the last stretch `GALLERY_HALF` wide so it meets its neighbour's.
fn gallery(plan: &mut Plan, seed: Seed, a: DVec2, b: DVec2, half_a: f64) {
    let w = plan.w;
    let mut r = seed.rng();
    let d = b - a;
    let len = d.length();
    if len < 4.0 {
        return;
    }
    let bend = a + d * r.range(0.35, 0.65) + d.perp() / len * r.range(-25.0, 25.0);
    let (fa, fb) = (deep_floor(w, a), deep_floor(w, b));
    let mut fm = deep_floor(w, bend);
    let (l0, l1) = (a.distance(bend), bend.distance(b));
    if (fm - fa).abs() > 0.3 * l0 || (fb - fm).abs() > 0.3 * l1 {
        fm = (fa + fb) * 0.5;
    }
    let height = GALLERY_HEIGHT + r.range(0.0, 1.5);
    plan.t
        .voids
        .extend(run(VoidKind::Gallery, a, bend, half_a, fa, fm, height));
    plan.t.voids.extend(run(
        VoidKind::Gallery,
        bend,
        b,
        GALLERY_HALF,
        fm,
        fb,
        GALLERY_HEIGHT,
    ));
    // Glow-worm crystals on the walls now and then.
    for (p, q, fp, fq, h) in [(a, bend, fa, fm, half_a), (bend, b, fm, fb, GALLERY_HALF)] {
        let l = p.distance(q);
        let n = (l / 28.0).floor() as usize;
        for i in 0..n {
            let t = (i as f64 + 0.5) / n as f64;
            let s = if i % 2 == 0 { 1.0 } else { -1.0 };
            let dir = (q - p) / l;
            let at = p.lerp(q, t) + dir.perp() * s * (h - 0.45);
            let f = fp + (fq - fp) * t;
            // Not inside a grotto: it has its own growths.
            if plan
                .t
                .voids
                .iter()
                .any(|v| v.kind == VoidKind::Grotto && v.covers(at, 1.0))
            {
                continue;
            }
            plan.t.fixtures.push(Fixture {
                kind: FixtureKind::Crystal,
                at: DVec3::new(at.x, f, at.y),
                dir: -dir.perp() * s,
                size: DVec3::new(0.35, 0.9, 0.0),
                look: i as u32,
            });
            plan.t.lamps.push(DeepLamp {
                pos: DVec3::new(at.x, f + 1.2, at.y),
                color: [0.45, 0.85, 1.0],
                strength: 2.5,
                range: 8.0,
            });
        }
    }
}

pub(super) fn plan_tile(plan: &mut Plan) {
    let w = plan.w;
    let k = plan.key();
    let seed = plan.seed.child("caves");
    let n = node(seed, k);
    let nf = deep_floor(w, n);
    // The grotto at the node.
    let mut r = seed.child("grotto").rng();
    if r.chance(GROTTO) {
        let kind = *r.pick(&[
            GrottoKind::Crystal,
            GrottoKind::Mushroom,
            GrottoKind::GlowLake,
            GrottoKind::Fossil,
        ]);
        let dir = hw_core::det::dir2(r.range(0.0, std::f64::consts::TAU));
        let half = DVec2::new(r.range(15.0, 22.0), r.range(11.0, 15.0));
        let height = r.range(10.0, 12.0);
        let vi = plan.t.voids.len();
        let v = room(VoidKind::Grotto, n, dir, half, nf, height);
        plan.t.voids.push(v);
        plan.t.grottos.push(Grotto {
            kind,
            void: vi,
            centre: n,
            floor: nf,
        });
        dress(plan, seed.child("dress"), kind, n, dir, half, nf);
    } else {
        plan.t.voids.push(room(
            VoidKind::Gallery,
            n,
            DVec2::X,
            DVec2::splat(5.0),
            nf,
            6.0,
        ));
    }
    for (i, p) in portals(w, k).into_iter().enumerate() {
        let half = r.range(2.4, 3.4);
        gallery(plan, seed.child("gallery").child(i), n, p, half);
    }
    // Lift feet: a small chamber each, and a gallery to the node.
    let bottoms = std::mem::take(&mut plan.bottoms);
    for (i, c) in bottoms.into_iter().enumerate() {
        let f = deep_floor(w, c);
        plan.t.voids.push(room(
            VoidKind::Gallery,
            c,
            DVec2::X,
            DVec2::splat(4.2),
            f,
            5.0,
        ));
        gallery(plan, seed.child("foot").child(i), c, n, 2.4);
    }
    // Molehaven's way down: a lift from a side chamber.
    if let Some(cav) = plan.under.cavern.as_ref() {
        if super::tile_of(cav.centre) == k {
            if let Some(&ci) = cav.chambers.get(1) {
                let ch = &plan.under.voids[ci];
                let c = ch.world(ch.max - DVec2::splat(2.6));
                let f = deep_floor(w, c);
                plan.inner_lift(c, f, cav.floor);
                plan.t.voids.push(room(
                    VoidKind::Gallery,
                    c,
                    DVec2::X,
                    DVec2::splat(4.2),
                    f,
                    5.0,
                ));
                gallery(plan, seed.child("molehaven"), c, n, 2.4);
            }
        }
    }
}

/// Fills a grotto: its kind's growths round the walls, a clear middle, and
/// a mole camp in some.
fn dress(plan: &mut Plan, seed: Seed, kind: GrottoKind, c: DVec2, dir: DVec2, half: DVec2, f: f64) {
    let mut r = seed.rng();
    let at = |x: f64, y: f64| c + dir * x + dir.perp() * y;
    let p3 = |q: DVec2| DVec3::new(q.x, f, q.y);
    let mut fixtures: Vec<Fixture> = Vec::new();
    // A point in the ring near the walls (60-90% of the way out).
    let ring = |r: &mut hw_core::seed::Rng| {
        let a = r.range(0.0, std::f64::consts::TAU);
        let t = r.range(0.62, 0.86);
        at(
            hw_core::det::cos(a) * half.x * t,
            hw_core::det::sin(a) * half.y * t,
        )
    };
    let place = |fx: &mut Vec<Fixture>, f: Fixture| {
        let reach = f.size.x.max(0.5);
        if fx
            .iter()
            .all(|o| o.plan().distance(f.plan()) > reach + o.size.x.max(0.5) + 0.8)
        {
            fx.push(f);
        }
    };
    let (lamp_col, n_main) = match kind {
        GrottoKind::Crystal => ([0.6, 0.5, 1.0], 14),
        GrottoKind::Mushroom => ([0.5, 1.0, 0.7], 9),
        GrottoKind::GlowLake => ([0.3, 0.9, 1.0], 0),
        GrottoKind::Fossil => ([1.0, 0.8, 0.55], 0),
    };
    for i in 0..n_main {
        let p = ring(&mut r);
        let fx = match kind {
            GrottoKind::Crystal => Fixture {
                kind: FixtureKind::Crystal,
                at: p3(p),
                dir: (c - p).normalize_or(dir),
                size: DVec3::new(r.range(0.7, 1.4), r.range(1.8, 4.6), 0.0),
                look: i,
            },
            _ => Fixture {
                kind: FixtureKind::GiantMushroom,
                at: p3(p),
                dir,
                size: DVec3::new(r.range(1.6, 3.4), r.range(3.0, 6.2), 0.0),
                look: i,
            },
        };
        place(&mut fixtures, fx);
    }
    match kind {
        GrottoKind::GlowLake => {
            fixtures.push(Fixture {
                kind: FixtureKind::GlowPool,
                at: p3(c),
                dir,
                size: DVec3::new(half.x * 0.5, 0.0, half.y * 0.45),
                look: 0,
            });
        }
        GrottoKind::Fossil => {
            fixtures.push(Fixture {
                kind: FixtureKind::Skeleton,
                at: p3(at(0.0, half.y - 3.2)),
                dir: dir.perp(),
                size: DVec3::new(8.5, 3.6, 2.4),
                look: 0,
            });
        }
        _ => {}
    }
    for i in 0..r.range_int(4, 9) {
        let p = ring(&mut r);
        place(
            &mut fixtures,
            Fixture {
                kind: FixtureKind::Stalagmite,
                at: p3(p),
                dir,
                size: DVec3::new(r.range(0.45, 0.9), r.range(1.4, 3.6), 0.0),
                look: i as u32,
            },
        );
    }
    // Light from the growths and the middle.
    for i in 0..6 {
        let a = std::f64::consts::TAU * i as f64 / 6.0;
        let q = at(
            hw_core::det::cos(a) * half.x * 0.7,
            hw_core::det::sin(a) * half.y * 0.7,
        );
        plan.t.lamps.push(DeepLamp {
            pos: DVec3::new(q.x, f + 2.4, q.y),
            color: lamp_col,
            strength: 4.0,
            range: 11.0,
        });
    }
    plan.t.lamps.push(DeepLamp {
        pos: DVec3::new(c.x, f + 4.5, c.y),
        color: lamp_col,
        strength: 5.0,
        range: 16.0,
    });
    // A mole camp: tents round a fire in the middle.
    if kind != GrottoKind::GlowLake && r.chance(0.6) {
        let fire = at(2.5, 0.0);
        fixtures.push(Fixture {
            kind: FixtureKind::Campfire,
            at: p3(fire),
            dir,
            size: DVec3::new(0.7, 0.5, 0.0),
            look: 0,
        });
        for (i, a) in [-1.1f64, 0.0, 1.1].into_iter().enumerate() {
            let p = fire + dir.rotate(hw_core::det::dir2(a)) * 6.5;
            let t = Fixture {
                kind: FixtureKind::Tent,
                at: p3(p),
                dir: (fire - p).normalize_or(dir),
                size: DVec3::new(1.4, 2.1, 1.6),
                look: i as u32,
            };
            fixtures.retain(|o| o.plan().distance(p) > o.size.x.max(0.5) + 2.4);
            fixtures.push(t);
        }
        plan.t.lamps.push(DeepLamp {
            pos: DVec3::new(fire.x, f + 1.0, fire.y),
            color: [1.0, 0.6, 0.25],
            strength: 6.0,
            range: 12.0,
        });
        // Keep the camp's middle clear.
        let spot = at(-1.0, 0.0);
        fixtures.retain(|o| {
            matches!(o.kind, FixtureKind::Tent | FixtureKind::Campfire)
                || o.plan().distance(spot) > 5.0 + o.size.x * 0.6
        });
        plan.t.spots.push(DeepSpot {
            kind: SpotKind::MoleCamp,
            at: p3(spot),
            facing: dir,
        });
    }
    plan.t.fixtures.extend(fixtures);
}
