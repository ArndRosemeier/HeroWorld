//! A long trip across a big city keeps the server's memory bounded: town
//! detail, collision tiles and colliders come and go with the hero, and
//! what damage leaves behind (breakable plans, changed solids) is gone
//! again once it is repaired.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_sim::collision::KEEP_TILES;
use hw_sim::destruction::{self, Damage};
use hw_sim::{GeneratedTown, TICK_HZ};

#[test]
fn a_long_trip_across_a_big_city_keeps_memory_bounded() {
    let t = GeneratedTown::generate(Seed(2), 400_000.0);
    let solids = &t.obstacles;
    let mut damage = Damage::default();
    let power = destruction::hero_power(20);
    let start = t.world.core();
    let span = (t.world.half() * 0.8).min(8000.0);
    let (mut most_tiles, mut most_live, mut most_quarters, mut hits) = (0, 0, 0, 0);
    let mut tick = 0u64;
    let mut x = -span;
    while x <= span {
        let p = start + DVec2::new(x, -x * 0.3);
        let g = t.world.ground(p);
        // Walk the ground round here, and punch the nearest walls.
        for k in 0..40 {
            let a = k as f64 * 0.157;
            let q = p + DVec2::new(a.cos(), a.sin()) * (k as f64 * 3.0);
            solids.floor_under(q, g + 50.0);
        }
        for id in solids.buildings_near(p, 40.0).into_iter().take(3) {
            if let Some((plan, base)) = solids.building(id) {
                let s = &plan.site;
                let front = s.origin + s.along * s.size.x * 0.5;
                damage.hit(
                    solids,
                    DVec3::new(front.x, base + 1.5, front.y),
                    power.punch_radius,
                    power.punch,
                    None,
                    tick,
                );
                hits += 1;
            }
        }
        tick += TICK_HZ as u64 * 30;
        let here = [DVec3::new(p.x, g, p.y)];
        damage.repair_unseen(solids, &here, tick, TICK_HZ);
        let (tiles, live, _) = solids.kept();
        most_tiles = most_tiles.max(tiles);
        most_live = most_live.max(live);
        most_quarters = most_quarters.max(t.world.quarters_kept());
        x += 400.0;
    }
    assert!(hits > 20, "only {hits} blows landed");
    assert!(most_tiles <= KEEP_TILES, "{most_tiles} collision tiles");
    assert!(most_live < 20_000, "{most_live} live colliders");
    assert!(most_quarters <= 160, "{most_quarters} quarters");
    // Far away for longer than the repair time: everything is whole and
    // nothing of the damage is kept.
    let far = [DVec3::new(1e7, 0.0, 1e7)];
    let later = tick + (destruction::REPAIR_SECONDS * TICK_HZ as f64) as u64 + 1;
    damage.repair_unseen(solids, &far, later, TICK_HZ);
    assert!(damage.buildings.is_empty());
    assert_eq!(damage.plans_kept(), 0, "breakable plans left behind");
    assert_eq!(solids.kept().2, 0, "changed solids left behind");
}
