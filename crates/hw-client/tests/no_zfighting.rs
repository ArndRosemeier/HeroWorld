//! No differently coloured faces may fight anywhere in a generated city,
//! closed or cut open. On failure, run the `zfight` tool for the full list.

use hw_client::buildingmesh::Detail;
use hw_client::zfight;
use hw_core::Seed;
use hw_sim::GeneratedCity;

#[test]
#[ignore = "slow: scripts/check.sh --full and CI run it"]
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
        let report = zfight::city(city, cut, detail).run();
        assert!(
            report.is_clean(),
            "seed {seed}, cut {cut:?}, {detail:?}: {}\n(cargo run -p hw-client --bin zfight)",
            report.summary(8)
        );
    }
}
