use super::*;
use crate::GeneratedTown;

fn town() -> (Arc<Area>, Citizens) {
    let t = GeneratedTown::generate(Seed(1), 40_000.0);
    let signals = crate::traffic::Traffic::new(t.area.clone()).signals();
    let c = Citizens::new(t.area.clone(), signals);
    (t.area, c)
}

/// The middle of the town's square: where the shops are.
fn centre(a: &Area) -> DVec2 {
    (a.lo + a.hi) * 0.5
}

#[test]
fn pitches_stand_clear_of_walkers_doors_and_everything_solid() {
    let (area, _) = town();
    let life = StreetLife::new(area.clone());
    let sites = life.sites_near(centre(&area), 900.0);
    let kinds = |k| sites.iter().filter(|s| s.kind == k).count();
    assert!(
        kinds(SiteKind::Pavement) >= 8,
        "{} pavement pitches",
        kinds(SiteKind::Pavement)
    );
    assert!(
        kinds(SiteKind::Square) >= 1,
        "{} square pitches",
        kinds(SiteKind::Square)
    );
    assert!(
        kinds(SiteKind::Door) >= 1,
        "{} club doors",
        kinds(SiteKind::Door)
    );
    for s in &sites {
        assert!(
            !life.in_building(s.at, 0.3),
            "pitch at {:?} in a building",
            s.at
        );
        assert!(
            life.prop_distance(s.at, 3.0) >= PROP_CLEAR - 1e-9,
            "pitch at {:?} against a prop",
            s.at
        );
        let (walk, _) = life.walk_distance(s.at, None);
        assert!(
            walk >= QUEUE_WALK_CLEAR,
            "pitch at {:?} {walk:.2} m from walkers",
            s.at
        );
        for (q, f) in &s.spots {
            assert!(
                !life.in_building(*q, 0.2),
                "a watcher's place in a building"
            );
            assert!(life.walk_distance(*q, None).0 >= QUEUE_WALK_CLEAR - 1e-9);
            assert!(life.prop_distance(*q, 3.0) >= SPOT_PROP_CLEAR - 1e-9);
            assert!((f.length() - 1.0).abs() < 1e-6);
        }
        // Not two pitches on top of each other.
        for o in &sites {
            if o.key != s.key {
                assert!(o.at.distance(s.at) > 3.0);
            }
        }
    }
    // Keys are stable: the same pitches worked out again (a fresh one).
    let again = StreetLife::new(area.clone()).sites_near(centre(&area), 900.0);
    let k1: Vec<u64> = sites.iter().map(|s| s.key).collect();
    let k2: Vec<u64> = again.iter().map(|s| s.key).collect();
    assert_eq!(k1, k2);
}

#[test]
fn same_seed_same_street_life_and_nobody_stands_about_for_nothing() {
    let (area, citizens) = town();
    let a = StreetLife::new(area.clone());
    let b = StreetLife::new(area.clone());
    let at = centre(&area);
    let mut seen_acts = std::collections::BTreeSet::new();
    let mut doing = std::collections::BTreeSet::new();
    let mut people = 0;
    // A day, every twenty game minutes.
    for k in 0..72 {
        let time = k as f64 * 50.0 + 3.0;
        let hour = crate::clock::hour_at(time);
        let sa = a.shows_near(&citizens, at, 900.0, time, hour);
        let sb = b.shows_near(&citizens, at, 900.0, time, hour);
        assert_eq!(sa.len(), sb.len());
        for (x, y) in sa.iter().zip(&sb) {
            assert_eq!((x.key(), x.act), (y.key(), y.act));
            let px = x.people(&a, &citizens, time);
            let py = y.people(&b, &citizens, time);
            assert_eq!(px, py, "same show, same people");
            seen_acts.insert(x.act);
            assert!(x.act.on_at(hour + 3.0) || x.act.on_at(hour - 3.0) || x.act.on_at(hour));
            for p in &px {
                people += 1;
                doing.insert(format!("{:?}", p.doing));
                let flat = DVec2::new(p.pos.x, p.pos.z);
                assert!(p.pos.is_finite() && p.facing.is_finite());
                // Standing still only at their place, doing something.
                if p.speed == 0.0 {
                    let home = std::iter::once(x.site.at)
                        .chain(x.site.spots.iter().map(|s| s.0))
                        .chain(x.site.door)
                        .map(|q| q.distance(flat))
                        .fold(f64::INFINITY, f64::min);
                    assert!(
                        home < 1.2,
                        "{:?} stands about {home:.1} m from any place",
                        p.role
                    );
                }
            }
        }
    }
    assert!(people > 50, "{people} people in a day");
    assert!(seen_acts.len() >= 5, "acts: {seen_acts:?}");
    for d in ["Walking", "Performing", "Watching"] {
        assert!(doing.contains(d), "nobody {d}: {doing:?}");
    }
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn acts_keep_their_hours() {
    let (area, _) = town();
    let life = StreetLife::new(area.clone());
    let sites = life.sites_near(centre(&area), 900.0);
    for s in &sites {
        for slot in 0..48 {
            let time = slot as f64 * SLOT;
            let hour = crate::clock::hour_at(time);
            if let Some(act) = life.act_at(s, slot, hour, time) {
                let mid = crate::clock::hour_at(time + SLOT * 0.5);
                assert!(act.on_at(mid), "{act:?} at {mid:.1}");
                assert!(act.fits(s.kind));
            }
        }
    }
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn the_club_queue_moves_up_and_everyone_goes_in() {
    let (area, citizens) = town();
    let life = StreetLife::new(area.clone());
    let at = centre(&area);
    // Late evening: the clubs are open.
    let mut checked = 0;
    for slot in 0..36i64 {
        let time = slot as f64 * SLOT + 1.0;
        let hour = crate::clock::hour_at(time);
        for show in life.shows_near(&citizens, at, 900.0, time, hour) {
            if show.act != Act::Club || show.slot != slot {
                continue;
            }
            checked += 1;
            assert!(!show.guests.is_empty());
            let mut t = show.on;
            while t < show.gone {
                let ppl = show.people(&life, &citizens, t);
                let mut ids: Vec<u64> = ppl.iter().map(|p| p.id).collect();
                ids.sort_unstable();
                let n = ids.len();
                ids.dedup();
                assert_eq!(ids.len(), n, "two people share an id at {t}");
                let queued: Vec<DVec2> = ppl
                    .iter()
                    .filter(|p| p.doing == Doing::Queueing)
                    .map(|p| DVec2::new(p.pos.x, p.pos.z))
                    .collect();
                assert!(queued.len() <= show.site.spots.len());
                for (i, a) in queued.iter().enumerate() {
                    for b in &queued[i + 1..] {
                        assert!(a.distance(*b) > 0.4, "two guests in one place at {t}");
                    }
                }
                t += 2.0;
            }
            // Everyone in by the time the doorman goes.
            for g in &show.guests {
                assert!(g.enters <= show.off + 1e-6);
                assert!(g.enters > g.joins);
            }
        }
    }
    assert!(checked > 0, "no club open in a day and a half");
}

#[test]
fn what_is_kept_stays_bounded() {
    let (area, citizens) = town();
    let life = StreetLife::new(area.clone());
    // A long walk across the town, a game day long.
    for k in 0..400 {
        let t = k as f64 * 9.0;
        let p = area.lo + (area.hi - area.lo) * DVec2::new(0.2 + k as f64 / 700.0, 0.5);
        let _ = life.shows_near(&citizens, p, 120.0, t, crate::clock::hour_at(t));
    }
    let (patches, shows) = life.held();
    assert!(patches <= KEEP_PATCHES, "{patches} patches");
    assert!(shows <= KEEP_SHOWS, "{shows} shows");
}
