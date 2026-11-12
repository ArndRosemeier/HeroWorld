use super::office::{plan, plan_scheme, MIN_SIZE};
use super::*;
use hw_core::Seed;

/// Plot sizes and storeys the tests sweep.
fn plots() -> Vec<(DVec2, u32)> {
    let mut out = Vec::new();
    for w in [32.0, 38.0, 44.0, 52.0, 60.0, 70.0] {
        for d in [38.0, 44.0, 50.0, 60.0] {
            for n in [2, 3, 4, 6, 9] {
                out.push((DVec2::new(w, d), n));
            }
        }
    }
    out
}

#[test]
fn every_office_is_sound() {
    for (i, (size, n)) in plots().into_iter().enumerate() {
        for k in 0..4u64 {
            let seed = Seed(1000 + i as u64 * 7 + k);
            let l = plan(seed, size, n);
            let p = l.problems();
            assert!(
                p.is_empty(),
                "{:?} {size} x{n} seed {seed:?}: {p:#?}",
                l.scheme
            );
        }
    }
}

#[test]
fn every_scheme_is_sound_where_it_fits() {
    let mut fits = [0; 3];
    for (i, (size, n)) in plots().into_iter().enumerate() {
        for (j, s) in [Scheme::GrandHall, Scheme::Atrium, Scheme::SplitLevel]
            .into_iter()
            .enumerate()
        {
            for k in 0..3u64 {
                let seed = Seed(77 + i as u64 * 13 + k);
                if let Some(l) = plan_scheme(seed, size, n, s) {
                    fits[j] += 1;
                    let p = l.problems();
                    assert!(p.is_empty(), "{s:?} {size} x{n} seed {seed:?}: {p:#?}");
                }
            }
        }
    }
    // Each scheme fits a fair share of plots.
    assert!(fits.iter().all(|f| *f > 30), "{fits:?}");
}

#[test]
fn grand_hall_fits_the_smallest_plot() {
    for k in 0..20 {
        for n in 2..10 {
            assert!(
                plan_scheme(Seed(k), MIN_SIZE, n, Scheme::GrandHall).is_some(),
                "seed {k} x{n}"
            );
        }
    }
}

#[test]
fn same_seed_same_layout() {
    let size = DVec2::new(50.0, 48.0);
    assert_eq!(plan(Seed(5), size, 5), plan(Seed(5), size, 5));
}

#[test]
fn rooms_are_game_sized() {
    // Ordinary floors are at least 5.5 m high, the lobby or canyon reaches
    // through two storeys or more, and stairs are real (no magic lift).
    for k in 0..30 {
        let l = plan(Seed(k), DVec2::new(48.0, 46.0), 5);
        assert!(l
            .spaces
            .iter()
            .all(|s| s.within.is_some() || s.h >= 5.5 - 1e-9));
        let tallest = l.spaces.iter().map(|s| s.h).fold(0.0, f64::max);
        assert!(tallest >= 12.0, "{:?}", l.scheme);
        assert!(!l.stairs.is_empty());
    }
}

#[test]
fn the_checks_catch_mistakes() {
    let mut l = plan(Seed(3), DVec2::new(48.0, 46.0), 4);
    assert!(l.problems().is_empty());
    // A floor that floats leaves a gap under it.
    let mut gap = l.clone();
    if let Some(s) = gap
        .spaces
        .iter_mut()
        .find(|s| s.within.is_none() && s.y > 0.0)
    {
        s.y += 1.0;
    }
    assert!(!gap.problems().is_empty());
    // A door between rooms that don't touch.
    l.links.push(Link {
        a: 0,
        b: Some(l.spaces.len() - 1),
        kind: LinkKind::Door,
        y: 0.0,
        at: DVec2::new(1.0, 1.0),
        along_x: true,
        width: DOOR,
    });
    assert!(!l.problems().is_empty());
}

/// No stack of copies: in a tall town office most upper storeys have a
/// plan of their own (rooms, their sizes and heights).
#[test]
fn storeys_differ_from_each_other() {
    use std::collections::BTreeSet;
    let (mut floors, mut kinds) = (0, 0);
    for k in 0..40u64 {
        let l = super::office::plan_town(
            Seed(300 + k),
            DVec2::new(40.0 + (k % 5) as f64 * 4.0, 48.0),
            7,
        );
        let levels = l.levels();
        let plans: BTreeSet<Vec<[i64; 5]>> = levels[1..levels.len() - 1]
            .iter()
            .map(|y| {
                let mut rooms: Vec<[i64; 5]> = l
                    .spaces
                    .iter()
                    .filter(|s| s.within.is_none() && (s.y - y).abs() < 1e-6)
                    .map(|s| {
                        [s.rect.min.x, s.rect.min.y, s.rect.max.x, s.rect.max.y, s.h]
                            .map(|v| (v * 2.0).round() as i64)
                    })
                    .collect();
                rooms.sort();
                rooms
            })
            .collect();
        floors += levels.len() - 2;
        kinds += plans.len();
        assert!(
            plans.len() >= 4,
            "seed {k}: only {} plans on six upper storeys",
            plans.len()
        );
    }
    assert!(
        kinds * 10 >= floors * 9,
        "{kinds} plans on {floors} storeys"
    );
}

/// Every narrow old-town plot gets a sound shop house: a shop at the
/// street, a way up to every storey, and now and then a tall room with a
/// gallery.
#[test]
fn shop_houses_are_sound_on_narrow_plots() {
    use super::office::{plan_town, MIN_HOUSE};
    let (mut tall, mut all) = (0, 0);
    for k in 0..120u64 {
        let size = DVec2::new(
            MIN_HOUSE.x + (k % 7) as f64 * 0.5,
            MIN_HOUSE.y + (k % 13) as f64 * 0.5,
        );
        let storeys = 2 + (k % 3) as u32;
        let l = plan_town(Seed(700 + k), size, storeys);
        assert_eq!(l.scheme, Scheme::ShopHouse);
        assert!(l.problems().is_empty(), "{k} {size:?}: {:?}", l.problems());
        assert_eq!(l.levels().len() as u32, storeys, "{k}");
        assert!(l
            .spaces
            .iter()
            .any(|s| s.kind == SpaceKind::Shop && s.y == 0.0));
        assert!(l.lifts.is_empty());
        all += 1;
        if l.spaces.iter().any(|s| s.kind == SpaceKind::Gallery) {
            tall += 1;
        }
    }
    assert!(
        tall * 6 > all && tall < all,
        "{tall} of {all} have a tall room"
    );
}
