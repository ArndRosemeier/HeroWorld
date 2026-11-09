use super::*;
use crate::urban::TownParams;

fn town(seed: u64, pop: f64) -> TownWorld {
    TownWorld::generate(Seed(seed), TownParams::sample(Seed(seed), pop))
}

/// Tiles round the core, out to `r`.
fn tiles_round(w: &TownWorld, r: f64) -> Vec<TileKey> {
    let (core, _) = w.spawn_line();
    let (a, b) = (
        tile_of(core - DVec2::splat(r)),
        tile_of(core + DVec2::splat(r)),
    );
    let mut out = Vec::new();
    for x in a.0..=b.0 {
        for y in a.1..=b.1 {
            if w.deep_in_land((x, y)) {
                out.push((x, y));
            }
        }
    }
    out
}

#[test]
fn the_tube_runs_under_the_main_roads_with_stations() {
    for seed in [1u64, 3, 11] {
        let w = town(seed, 40_000.0);
        let net = w.tube();
        assert!(
            net.lines.len() > 5,
            "seed {seed}: {} lines",
            net.lines.len()
        );
        assert!(
            net.stations.len() >= 4,
            "seed {seed}: {} stations",
            net.stations.len()
        );
        let names: std::collections::BTreeSet<_> = net.stations.iter().map(|s| &s.name).collect();
        assert_eq!(names.len(), net.stations.len(), "station names repeat");
        for l in &net.lines {
            for (i, p) in l.pts.iter().enumerate() {
                // Under the ground, a good way down, gently graded.
                assert!(
                    l.floors[i] + TUBE_HEIGHT < w.ground(*p) - 6.0,
                    "seed {seed}: tube too shallow at {p}"
                );
                if i > 0 {
                    let d = p.distance(l.pts[i - 1]);
                    assert!((l.floors[i] - l.floors[i - 1]).abs() <= 0.1 * d + 1e-6);
                }
            }
        }
        // Any station can be reached from the first one on its network.
        let mut joined = 0;
        for b in 1..net.stations.len() {
            if let Some(path) = net.path(0, b) {
                joined += 1;
                let (pa, _) = net.platform(0);
                let (pb, _) = net.platform(b);
                assert!(path[0].distance(pa) < 0.5);
                assert!(path.last().unwrap().distance(pb) < 0.5);
            }
        }
        eprintln!(
            "seed {seed}: {} lines, {} stations ({} reachable from the first)",
            net.lines.len(),
            net.stations.len(),
            joined
        );
        assert!(
            joined * 2 >= net.stations.len() - 1,
            "seed {seed}: the lines do not join"
        );
    }
}

#[test]
fn deep_tiles_are_sound() {
    for seed in [1u64, 3] {
        let w = town(seed, 40_000.0);
        let keys = tiles_round(&w, 900.0);
        let mut counts = [0usize; 6];
        for &k in &keys {
            let t = w.deep_tile(k);
            counts[0] += t.stations.len();
            counts[1] += t.entrances.len();
            counts[2] += t.grottos.len();
            counts[3] += t.ruins.len();
            counts[4] += t.lairs.len();
            counts[5] += t.lifts.len();
            for v in &t.voids {
                // Roofed by rock (shafts open to the sky aside).
                assert!(
                    roofed(&w, v, 1.4),
                    "seed {seed}: {:?} in {k:?} breaks the ground",
                    v.kind
                );
                // Out of the shallow underground, bar the lift into Molehaven.
                if v.kind != VoidKind::Shaft {
                    let u = w.core_underground();
                    let (lo, hi) = bounds(v);
                    for i in u.near((lo + hi) * 0.5, (hi - lo).length()) {
                        let o = &u.voids[i];
                        let joined =
                            matches!(v.kind, VoidKind::Gallery) && o.kind == VoidKind::Cavern;
                        assert!(
                            joined || !overlaps(v, o, 0.5),
                            "seed {seed}: deep {:?} cuts into {:?}",
                            v.kind,
                            o.kind
                        );
                    }
                }
            }
            // Lifts reach both ends.
            for l in &t.lifts {
                let foot = DVec3::new(l.pos.x, l.bottom + 1.0, l.pos.y);
                let head = DVec3::new(l.pos.x, l.top + 1.0, l.pos.y);
                assert!(w.deep_open_at(foot), "seed {seed}: lift foot in rock");
                let up = w.deep_open_at(head) || w.core_underground().open_at(head);
                assert!(up, "seed {seed}: lift head in rock");
                assert!(w.deep_floor_for(l.pos, l.bottom + 1.0).is_some());
            }
            for e in &t.entrances {
                let foot = DVec3::new(e.pos.x, e.bottom + 1.0, e.pos.y);
                assert!(w.deep_open_at(foot), "seed {seed}: {} goes nowhere", e.name);
                assert!(e.top - e.bottom > 10.0);
            }
            // Spots stand on open floor with room round them.
            for s in &t.spots {
                let q = DVec2::new(s.at.x, s.at.z);
                let f = w.deep_floor_for(q, s.at.y + 0.5).expect("spot on a floor");
                assert!(
                    (f - s.at.y).abs() < 0.6,
                    "seed {seed}: {:?} off its floor",
                    s.kind
                );
                for a in 0..8 {
                    let p = q + hw_core::det::dir2(a as f64 * 0.785) * 3.0;
                    assert!(
                        w.deep_floor_for(p, s.at.y + 0.5).is_some(),
                        "seed {seed}: {:?} spot by a wall",
                        s.kind
                    );
                    assert!(
                        w.deep_solids_near(p, 0.4).iter().all(|f| {
                            let (y0, y1) = f.span();
                            f.distance(p) > 0.4 || y1 < s.at.y || y0 > s.at.y + 2.0
                        }),
                        "seed {seed}: {:?} spot crowded: {:?}",
                        s.kind,
                        s.at
                    );
                }
            }
        }
        eprintln!(
            "seed {seed}: {} tiles: {} stations, {} street lifts, {} grottos, {} ruins, {} labs, {} inner lifts",
            keys.len(),
            counts[0],
            counts[1],
            counts[2],
            counts[3],
            counts[4],
            counts[5]
        );
        assert!(
            counts[0] >= 2 && counts[1] >= 2,
            "seed {seed}: no stations near the core"
        );
        assert!(counts[2] >= 5, "seed {seed}: few grottos");
        assert!(counts[3] >= 1, "seed {seed}: no ruins");
        assert!(counts[5] >= 2, "seed {seed}: no lifts to the Deeps");
    }
}

#[test]
fn the_doctors_lab_is_under_the_dystopia() {
    let mut found = 0;
    for seed in [1u64, 3, 11] {
        let w = town(seed, 40_000.0);
        let Some(site) = w.lair_site().cloned() else {
            continue;
        };
        let t = w.deep_tile(tile_of(site.centre));
        let Some(l) = t.lairs.first() else {
            panic!("seed {seed}: the lab did not fit");
        };
        found += 1;
        assert!(t.entrances.iter().any(|e| e.kind == EntranceKind::Kiosk));
        assert!(
            t.spots
                .iter()
                .filter(|s| s.kind == SpotKind::LairGuard)
                .count()
                >= 2
        );
        let hall = &t.voids[l.hall];
        assert!(hall.height >= 9.0);
        assert!(t.fixtures.iter().any(|f| f.kind == FixtureKind::Reactor));
    }
    assert!(found >= 1, "no town had a lab");
}

#[test]
fn galleries_meet_across_tile_edges() {
    let w = town(3, 40_000.0);
    let mut met = 0;
    for k in tiles_round(&w, 700.0) {
        for east in [true, false] {
            let Some(p) = caves::portal(&w, k, east) else {
                continue;
            };
            let other = if east { (k.0 + 1, k.1) } else { (k.0, k.1 + 1) };
            let f = deep_floor(&w, p);
            for key in [k, other] {
                let t = w.deep_tile(key);
                let inside = t
                    .at(p)
                    .any(|(_, v)| v.kind == VoidKind::Gallery && (v.floor_at(p) - f).abs() < 0.8);
                assert!(inside, "gallery of {key:?} misses the portal at {p}");
            }
            met += 1;
        }
    }
    assert!(met > 10);
}

#[test]
fn tiles_are_the_same_whatever_the_order() {
    let a = town(11, 40_000.0);
    let b = town(11, 40_000.0);
    let keys = tiles_round(&a, 600.0);
    let first: Vec<_> = keys.iter().map(|&k| a.deep_tile(k)).collect();
    let second: Vec<_> = keys.iter().rev().map(|&k| b.deep_tile(k)).collect();
    for (x, y) in first.iter().zip(second.iter().rev()) {
        assert_eq!(x.voids, y.voids);
        assert_eq!(x.fixtures, y.fixtures);
        assert_eq!(x.spots, y.spots);
        assert_eq!(x.entrances, y.entrances);
    }
}

#[test]
fn the_cache_stays_bounded() {
    let w = town(1, 40_000.0);
    let h = w.half();
    let mut n = 0;
    let mut k = tile_of(DVec2::splat(-h + TILE));
    // A long walk across the land and back.
    while tile_centre(k).x < h - TILE && n < 2 * CACHE {
        w.deep_tile(k);
        k.0 += 1;
        if tile_centre(k).x >= h - TILE {
            k = (tile_of(DVec2::splat(-h + TILE)).0, k.1 + 1);
        }
        n += 1;
    }
    assert!(w.deep_state().planned() <= CACHE);
}
