//! No differently coloured faces may fight anywhere in a generated city,
//! closed or cut open. On failure, run the `zfight` tool for the full list.

use hw_client::buildingmesh::Detail;
use hw_client::zfight;
use hw_core::Seed;
use hw_sim::GeneratedCity;

#[test]
#[ignore = "on demand (looks): scripts/check.sh --on-demand"]
fn the_city_has_no_z_fighting() {
    // Seed 1 has a river, seed 2 a river, a coast and a harbour.
    let views = [
        (1, None, Detail::Near),
        (1, None, Detail::Far),
        (1, Some(0), Detail::Near),
        (1, Some(1), Detail::Far),
        (1, Some(3), Detail::Near),
        (2, None, Detail::Far),
    ];
    let mut cities = std::collections::BTreeMap::new();
    for (seed, cut, detail) in views {
        let city = cities
            .entry(seed)
            .or_insert_with(|| GeneratedCity::generate(Seed(seed)));
        let report = zfight::city_report(city, cut, detail);
        assert!(
            report.is_clean(),
            "seed {seed}, cut {cut:?}, {detail:?}: {}\n(cargo run -p hw-client --bin zfight)",
            report.summary(8)
        );
    }
}

/// No road, pavement or corner plate fights another anywhere in a town's
/// streets, from a hamlet to a big city, closed rings and busy junctions
/// included (Arnd found a flickering junction in 0.197).
#[test]
#[ignore = "on demand (looks): scripts/check.sh --on-demand"]
fn town_streets_have_no_z_fighting() {
    for (seed, people) in [
        (1, 300.0),
        (1, 40_000.0),
        (2, 5_000.0),
        (2, 40_000.0),
        (3, 300_000.0),
    ] {
        let town = hw_sim::GeneratedTown::generate(Seed(seed), people);
        let report = zfight::town_roads(&town.world);
        assert!(
            report.is_clean(),
            "seed {seed}, {people} people: {}\n(cargo run --release -p hw-client --bin zfight -- {seed} --town {people})",
            report.summary(8)
        );
    }
}
