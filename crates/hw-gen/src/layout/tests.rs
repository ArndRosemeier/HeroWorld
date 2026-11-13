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

/// Every old-town plot big enough gets a sound house of flats: a house
/// hall at the street, and on every storey above a flat of a few large
/// rooms, each with a way to the stair.
#[test]
fn houses_of_flats_are_sound() {
    use super::office::{plan_flats, MIN_FLATS};
    let mut second = 0;
    for k in 0..120u64 {
        let size = DVec2::new(
            MIN_FLATS.x + (k % 7) as f64 * 0.5,
            MIN_FLATS.y + (k % 9) as f64 * 0.5,
        );
        let storeys = 2 + (k % 3) as u32;
        let l = plan_flats(Seed(800 + k), size, storeys);
        assert!(l.problems().is_empty(), "{k} {size:?}: {:?}", l.problems());
        let levels = l.levels();
        assert_eq!(levels.len() as u32, storeys, "{k}");
        for y in &levels[1..] {
            let on = |kind: SpaceKind| {
                l.spaces
                    .iter()
                    .filter(|s| s.kind == kind && (s.y - y).abs() < 1e-6)
                    .count()
            };
            assert_eq!((on(SpaceKind::Living), on(SpaceKind::Kitchen)), (1, 1));
            assert!((1..=2).contains(&on(SpaceKind::Bedroom)), "{k}");
            second += on(SpaceKind::Bedroom) - 1;
        }
        // Few rooms, and large ones.
        for s in l.spaces.iter().filter(|s| s.kind == SpaceKind::Living) {
            let size = s.rect.size();
            assert!(
                size.x >= 7.0 && size.y >= 6.0,
                "{k}: a living room of {size:?}"
            );
        }
    }
    assert!(second > 10, "{second} second bedrooms");
}

/// Apartment blocks: sound on every plot from the smallest, two flats of
/// few large rooms on every storey above the lobby, lifts to all floors.
#[test]
fn apartment_blocks_are_sound() {
    use super::office::{plan_flats, MIN_BLOCK};
    let mut second = 0;
    for k in 0..160u64 {
        let size = DVec2::new(
            MIN_BLOCK.x + (k % 11) as f64 * 1.5,
            MIN_BLOCK.y + (k % 7) as f64,
        );
        let storeys = 2 + (k % 9) as u32;
        let l = plan_flats(Seed(900 + k), size, storeys);
        assert_eq!(l.scheme, Scheme::Block, "{k} {size:?}");
        assert!(l.problems().is_empty(), "{k} {size:?}: {:?}", l.problems());
        let levels = l.levels();
        assert_eq!(levels.len() as u32, storeys, "{k}");
        assert_eq!(l.lifts.len(), 2);
        assert!(l.lifts.iter().all(|x| x.stops.len() as u32 == storeys));
        for y in &levels[1..] {
            let on = |kind: SpaceKind| {
                l.spaces
                    .iter()
                    .filter(|s| s.kind == kind && (s.y - y).abs() < 1e-6)
                    .count()
            };
            assert_eq!((on(SpaceKind::Living), on(SpaceKind::Kitchen)), (2, 2));
            assert!((2..=4).contains(&on(SpaceKind::Bedroom)), "{k}");
            second += on(SpaceKind::Bedroom) - 2;
        }
        for s in l.spaces.iter().filter(|s| s.kind == SpaceKind::Living) {
            let size = s.rect.size();
            assert!(
                size.x >= 9.0 && size.y >= 10.0,
                "{k}: a living room of {size:?}"
            );
        }
    }
    assert!(second > 40, "{second} second bedrooms");
}

/// Wide, shallow houses: a flat on every storey, or a family's house.
#[test]
fn terraces_and_villas_are_sound() {
    use super::office::{plan_flats, plan_villa, MIN_ROW};
    for k in 0..160u64 {
        let size = DVec2::new(
            MIN_ROW.x + (k % 15) as f64 * 0.5,
            MIN_ROW.y + (k % 7) as f64 * 0.5,
        );
        let storeys = 2 + (k % 4) as u32;
        let on = |l: &Layout, kind: SpaceKind, y: f64| {
            l.spaces
                .iter()
                .filter(|s| s.kind == kind && (s.y - y).abs() < 1e-6)
                .count()
        };
        let l = plan_flats(Seed(1200 + k), size, storeys);
        assert_eq!(l.scheme, Scheme::Terrace, "{k} {size:?}");
        assert!(l.problems().is_empty(), "{k} {size:?}: {:?}", l.problems());
        let levels = l.levels();
        assert_eq!(levels.len() as u32, storeys, "{k}");
        for &y in &levels {
            // One whole flat on every storey, the ground floor too.
            let rooms = [SpaceKind::Living, SpaceKind::Kitchen, SpaceKind::Bedroom];
            assert_eq!(rooms.map(|r| on(&l, r, y)), [1, 1, 1], "{k}");
        }
        let v = plan_villa(Seed(1200 + k), size);
        assert_eq!(v.scheme, Scheme::Villa, "{k} {size:?}");
        assert!(v.problems().is_empty(), "{k} {size:?}: {:?}", v.problems());
        let levels = v.levels();
        assert_eq!(levels.len(), 2, "{k}");
        assert_eq!(on(&v, SpaceKind::Living, 0.0), 1);
        assert_eq!(on(&v, SpaceKind::Kitchen, 0.0), 1);
        assert_eq!(on(&v, SpaceKind::Bedroom, levels[1]), 2);
        for s in l.spaces.iter().chain(&v.spaces) {
            let size = s.rect.size();
            if s.kind == SpaceKind::Living {
                assert!(size.x >= 9.0 && size.y >= 16.0, "{k}: living {size:?}");
            }
        }
    }
}

/// A loft house fits every plot from its least size up: one tall living
/// room, a kitchen under a bedroom gallery, and a way up to it.
#[test]
fn loft_houses_fit_small_plots() {
    use super::office::{plan_loft, MIN_LOFT};
    for k in 0..300u64 {
        let size = DVec2::new(
            MIN_LOFT.x + (k % 23) as f64 * 0.5,
            MIN_LOFT.y + (k % 16) as f64 * 0.5,
        );
        let l = plan_loft(Seed(1500 + k), size);
        assert_eq!(l.scheme, Scheme::Loft, "{k} {size:?}");
        assert!(l.problems().is_empty(), "{k} {size:?}: {:?}", l.problems());
        assert_eq!(l.levels().len(), 2, "{k}");
        let living = l
            .spaces
            .iter()
            .find(|s| s.kind == SpaceKind::Living)
            .expect("a living room");
        assert!(
            living.h >= 7.5,
            "{k}: the living room is {} m high",
            living.h
        );
        assert!(living.rect.size().min_element() >= 8.5, "{k}");
    }
}

/// Every plot a den fits on gets a sound one: a great hall through the
/// whole height, balconies and bridges in it, every storey its own
/// height, and the storeys' floors level through the house.
#[test]
fn dens_are_sound() {
    use super::den::{plan, MIN_DEN};
    use std::collections::BTreeSet;
    let (mut bridges, mut between, mut tall, mut all) = (0, 0, 0, 0);
    let mut shapes = BTreeSet::new();
    for k in 0..400u64 {
        let size = DVec2::new(
            MIN_DEN.x + (k % 9) as f64 * 0.5 + (k % 4) as f64 * 3.0,
            MIN_DEN.y + (k % 23) as f64 * 0.5,
        );
        let storeys = 3 + (k % 2) as u32;
        let l = plan(Seed(900 + k), size, storeys).expect("a den fits");
        assert_eq!(l, plan(Seed(900 + k), size, storeys).expect("a den fits"));
        let p = l.problems();
        assert!(p.is_empty(), "{k} {size:?} x{storeys}: {p:#?}");
        let levels = l.levels();
        assert_eq!(levels.len() as u32, storeys, "{k}");
        // The hall: the whole height and width of the house.
        let hall = &l.spaces[0];
        assert_eq!(hall.kind, SpaceKind::Hall);
        assert!(hall.h >= 12.0 && hall.h == l.height(), "{k}: {}", hall.h);
        assert_eq!(hall.rect.size().x, size.x);
        // No two storeys next to each other the same height.
        for w in levels.windows(3) {
            assert!(
                ((w[2] - w[1]) - (w[1] - w[0])).abs() > 0.4,
                "{k}: {levels:?}"
            );
        }
        // Balconies stand a storey's headroom apart where one hangs
        // over another.
        let decks: Vec<&Space> = l.spaces.iter().filter(|s| s.within == Some(0)).collect();
        for (i, a) in decks.iter().enumerate() {
            for c in &decks[i + 1..] {
                if a.rect.overlap(&c.rect) > 1e-3 {
                    assert!((a.y - c.y).abs() >= 3.0 - 1e-6, "{k}: {} {}", a.y, c.y);
                }
            }
        }
        all += 1;
        let ends = |s: &Space| s.rect.min.y > hall.rect.min.y && s.rect.max.y < hall.rect.max.y;
        let in_band = |s: &Space| s.rect.min.x > 1.0 && s.rect.max.x < size.x - 1.0;
        if decks.iter().any(|s| ends(s) && in_band(s)) {
            bridges += 1;
        }
        if decks
            .iter()
            .any(|s| levels.iter().all(|y| (s.y - y).abs() > 0.5))
        {
            between += 1;
        }
        if l.spaces
            .iter()
            .any(|s| s.within.is_none() && s.kind != SpaceKind::Hall && s.h > 8.0)
        {
            tall += 1;
        }
        shapes.insert((
            (hall.rect.min.y * 2.0) as i64,
            (hall.rect.max.y * 2.0) as i64,
            decks.len(),
            l.spaces.len(),
            (l.height() * 2.0) as i64,
        ));
    }
    assert!(bridges * 2 > all, "{bridges} of {all} have a bridge");
    assert!(
        between * 3 > all,
        "{between} of {all} have a balcony between storeys"
    );
    assert!(
        tall * 5 > all,
        "{tall} of {all} have a room two storeys high"
    );
    assert!(
        shapes.len() * 10 > all * 9,
        "{} shapes of {all}",
        shapes.len()
    );
}
