//! Airships never pass through buildings: the liner on every second of its
//! round, the sky pirates' ship at its mooring, their raiders over the
//! streets they raid and sailing off, in towns of every size.
//! Checked against what is drawn (roofs, crowns, spires and landmarks, as
//! `hw_gen::urban::airspace` bounds them) with the same solids heroes
//! bump into.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_gen::urban::airspace::{highest_under, SkyBox};
use hw_gen::urban::landmarks::LandmarkKind;
use hw_gen::urban::mast::liner_sky;
use hw_gen::urban::world::TownWorld;
use hw_gen::urban::TownParams;
use hw_sim::airships::{liner_solids, pirate_solids, Airships};
use hw_sim::collision::Solid;
use hw_sim::roosts::{pirate_roost, raid_departure, raid_mooring, raid_sail_height};

/// The plan box of a solid.
fn sky_of(s: &Solid) -> SkyBox {
    let mid = s.to_world((s.min + s.max) * 0.5);
    let half = (s.max - s.min) * 0.5;
    SkyBox::new(mid, s.inward, half.y, half.x)
}

/// Solids that dip into something drawn under them: (how deep, where).
fn clashes(w: &TownWorld, solids: &[Solid], skip: &[LandmarkKind]) -> Vec<(f64, DVec2)> {
    solids
        .iter()
        .filter_map(|s| {
            let top = highest_under(w, &sky_of(s), skip);
            (top > s.y0).then(|| (top - s.y0, sky_of(s).mid))
        })
        .collect()
}

/// Every few seconds of the liner's round: nothing under its hull.
fn liner_clears(seed: u64, population: f64) {
    let w = TownWorld::generate(Seed(seed), TownParams::sample(Seed(seed), population));
    let a = Airships::new(&w);
    let Some(r) = a.liner() else {
        panic!("seed {seed} pop {population}: no liner");
    };
    let skip = [LandmarkKind::MooringMast];
    let mut worst: Option<(f64, f64, DVec2)> = None;
    let mut low: f64 = f64::INFINITY;
    let mut g = 0.0;
    while g < r.period() {
        let p = r.pose(g);
        let solids = liner_solids(&p);
        let bottom = solids.iter().map(|s| s.y0).fold(f64::INFINITY, f64::min);
        // The whole hull's sky first; only where something stands under it,
        // each of its boxes.
        let top = highest_under(&w, &liner_sky(DVec2::new(p.nose.x, p.nose.z), p.yaw), &skip);
        low = low.min(bottom - top);
        if top > bottom {
            for (d, at) in clashes(&w, &solids, &skip) {
                if worst.is_none_or(|(wd, _, _)| d > wd) {
                    worst = Some((d, g, at));
                }
            }
        }
        g += 2.0;
    }
    eprintln!("seed {seed} pop {population}: lowest clearance {low:.1} m");
    assert!(
        worst.is_none(),
        "seed {seed} pop {population}: the liner goes {:.1} m into a roof at second {:.0}, at {:?}",
        worst.unwrap().0,
        worst.unwrap().1,
        worst.unwrap().2
    );
}

/// The pirates' ship at its mooring: nothing under or beside it.
fn pirate_clears(seed: u64, population: f64) -> bool {
    let w = TownWorld::generate(Seed(seed), TownParams::sample(Seed(seed), population));
    let Some(r) = pirate_roost(&w) else {
        return false;
    };
    let c = clashes(&w, &pirate_solids(r.ship(), r.ship_yaw()), &[]);
    assert!(
        c.is_empty(),
        "seed {seed} pop {population}: the pirate ship at {:?} is inside a building: {c:?}",
        r.ship()
    );
    true
}

#[test]
fn the_liner_clears_every_roof_on_its_round() {
    for (seed, pop) in [(2u64, 20_000.0), (3, 6_000.0)] {
        liner_clears(seed, pop);
    }
}

#[test]
#[ignore = "slow: scripts/check.sh --full and CI run it"]
fn the_liner_clears_every_roof_in_towns_of_every_size() {
    for (seed, pop) in [(1u64, 800.0), (5, 60_000.0), (7, 600_000.0)] {
        liner_clears(seed, pop);
    }
}

#[test]
fn the_pirate_ship_hangs_clear_of_every_building() {
    let mut moored = 0;
    for seed in [1u64, 2, 3, 4] {
        moored += pirate_clears(seed, 20_000.0) as u32;
    }
    assert!(moored >= 2, "{moored} towns of 4 have a pirate landing");
}

#[test]
#[ignore = "slow: scripts/check.sh --full and CI run it"]
fn the_pirate_ship_hangs_clear_in_towns_of_every_size() {
    for (seed, pop) in [
        (5u64, 1_500.0),
        (6, 80_000.0),
        (8, 400_000.0),
        (9, 20_000.0),
    ] {
        pirate_clears(seed, pop);
    }
}

/// Raiders over streets all round the core, and on their way out.
fn raiders_clear(seed: u64, population: f64) {
    let w = TownWorld::generate(Seed(seed), TownParams::sample(Seed(seed), population));
    let core = w.core();
    let roads = w.roads_in(core - DVec2::splat(350.0), core + DVec2::splat(350.0));
    let mut streets = Vec::new();
    for r in roads.iter().step_by(3) {
        let p = r.pts[r.pts.len() / 2];
        streets.push(DVec3::new(p.x, w.road_height(p), p.y));
        if streets.len() >= 24 {
            break;
        }
    }
    assert!(streets.len() >= 8, "seed {seed}: {} streets", streets.len());
    for street in streets {
        let (at, yaw) = raid_mooring(&w, street);
        let c = clashes(&w, &pirate_solids(at, yaw), &[]);
        assert!(
            c.is_empty(),
            "seed {seed} pop {population}: a raider over {street:?} at {at:?} is inside a building: {c:?}"
        );
        // Sailing off, the town ahead planned (as it is round a hero).
        let ahead = DVec2::new(at.x, at.z);
        w.quarters_in(ahead - DVec2::splat(700.0), ahead + DVec2::splat(700.0));
        let clear = raid_sail_height(&w, at, yaw);
        let mut t = 0.0;
        while let Some(p) = raid_departure(at, yaw, clear, t) {
            let c = clashes(&w, &pirate_solids(p, yaw), &[]);
            assert!(
                c.is_empty(),
                "seed {seed} pop {population}: a raider leaving {at:?} hits a building {t} s out: {c:?}"
            );
            t += 1.0;
        }
        assert!(t > 30.0, "it sails off for a while");
    }
}

#[test]
fn raiders_hang_and_sail_clear_of_every_building() {
    raiders_clear(2, 20_000.0);
}

#[test]
#[ignore = "slow: scripts/check.sh --full and CI run it"]
fn raiders_clear_every_building_in_towns_of_every_size() {
    for (seed, pop) in [(1u64, 800.0), (5, 60_000.0), (7, 600_000.0)] {
        raiders_clear(seed, pop);
    }
}
