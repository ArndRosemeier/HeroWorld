use super::*;
use crate::urban::TownParams;
use std::collections::BTreeMap;

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

#[test]
fn the_tube_has_nooks_ways_up_and_people() {
    for seed in [1u64, 3] {
        let w = town(seed, 40_000.0);
        let net = w.tube();
        let keys = tiles_round(&w, 900.0);
        let mut kinds: BTreeMap<NookKind, usize> = BTreeMap::new();
        let mut folk = 0;
        let mut ups: BTreeMap<usize, Vec<f64>> = BTreeMap::new();
        for &k in &keys {
            let t = w.deep_tile(k);
            for n in &t.nooks {
                *kinds.entry(n.kind).or_default() += 1;
                // Its room opens off the walkway: the mouth is in the tube and
                // the room's middle on a floor.
                let f = w.deep_floor_for(n.centre, n.floor + 0.5);
                if n.kind != NookKind::Exit {
                    assert!(
                        f.is_some_and(|f| (f - n.floor).abs() < 0.6),
                        "seed {seed}: {:?} at {} has no floor",
                        n.kind,
                        n.centre
                    );
                }
                let mouth = DVec3::new(n.mouth.x, n.floor + 1.0, n.mouth.y);
                assert!(
                    w.deep_open_at(mouth),
                    "seed {seed}: {:?} mouth shut",
                    n.kind
                );
                if n.kind == NookKind::Exit {
                    ups.entry(n.line)
                        .or_default()
                        .push(net.lines[n.line].arcs[0] + (n.slot as f64 + 0.5) * SLOT);
                    // The way from the walkway to the lift is open.
                    for s in 0..=20 {
                        let q = n.mouth.lerp(n.centre, s as f64 / 20.0);
                        assert!(
                            w.deep_open_at(DVec3::new(q.x, n.floor + 1.0, q.y)),
                            "seed {seed}: exit passage blocked at {q}"
                        );
                    }
                }
            }
            // The people stand on open floor, clear of the furniture, and
            // their errands keep clear too.
            for p in &t.folk {
                let a = DVec2::new(p.at.x, p.at.z);
                let way: Vec<DVec2> = match p.to {
                    Some(b) => (0..=16).map(|s| a.lerp(b, s as f64 / 16.0)).collect(),
                    None => vec![a],
                };
                for q in way {
                    let f = w.deep_floor_for(q, p.at.y + 0.5);
                    assert!(
                        f.is_some_and(|f| (f - p.at.y).abs() < 0.6),
                        "seed {seed}: someone off the floor at {q}"
                    );
                    assert!(
                        w.deep_open_at(DVec3::new(q.x, p.at.y + 1.7, q.y)),
                        "seed {seed}: someone's head in the rock at {q}"
                    );
                    for s in w.deep_solids_near(q, 0.3) {
                        let (y0, y1) = s.span();
                        assert!(
                            s.distance(q) > 0.3 || y1 < p.at.y + 0.1 || y0 > p.at.y + 1.8,
                            "seed {seed}: someone in a fixture at {q} ({p:?}; {:?})",
                            t.fixtures
                                .iter()
                                .filter(|f| f.plan().distance(q) < 2.0)
                                .map(|f| (f.kind, f.plan()))
                                .collect::<Vec<_>>()
                        );
                    }
                }
            }
            folk += t.folk.len();
        }
        eprintln!("seed {seed}: nooks {kinds:?}, {folk} people");
        for kind in [NookKind::Exit, NookKind::Homes, NookKind::Workshop] {
            assert!(
                kinds.get(&kind).copied().unwrap_or(0) >= 5,
                "seed {seed}: few {kind:?}"
            );
        }
        assert!(kinds.len() >= 5, "seed {seed}: little variety: {kinds:?}");
        let lived: usize = kinds
            .iter()
            .filter(|(k, _)| **k != NookKind::Exit)
            .map(|x| x.1)
            .sum();
        assert!(
            folk >= 2 * lived,
            "seed {seed}: {folk} people in {lived} nooks"
        );
        // Ways up along each line come often: exits and stations together.
        let mut gaps = Vec::new();
        for (li, l) in net.lines.iter().enumerate() {
            let core = w.spawn_line().0;
            if l.pts.iter().all(|p| p.distance(core) > 700.0) {
                continue;
            }
            let mut at: Vec<f64> = ups.get(&li).cloned().unwrap_or_default();
            for s in net.stations.iter().filter(|s| s.line == li) {
                at.push(l.arcs[s.at]);
            }
            at.push(0.0);
            at.push(*l.arcs.last().unwrap());
            at.sort_by(f64::total_cmp);
            // Only the stretches inside the tiles looked at.
            for x in at.windows(2) {
                let (mid, _, _) = net.along(li, (x[0] + x[1]) * 0.5);
                if mid.distance(core) < 600.0 {
                    gaps.push(x[1] - x[0]);
                }
            }
        }
        gaps.sort_by(f64::total_cmp);
        let p90 = gaps[gaps.len() * 9 / 10];
        eprintln!("seed {seed}: ways up every {:.0} m (90th percentile)", p90);
        assert!(
            p90 < 520.0,
            "seed {seed}: ways up too far apart: {p90:.0} m"
        );
    }
}
