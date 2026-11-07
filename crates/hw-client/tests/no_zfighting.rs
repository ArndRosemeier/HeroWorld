//! No differently coloured faces may fight anywhere in a generated city,
//! closed or cut open. On failure, run the `zfight` tool for the full list.

use hw_client::buildingmesh::Detail;
use hw_client::zfight::{self, hex, source_name};
use hw_core::Seed;
use hw_sim::GeneratedCity;

#[test]
fn the_city_has_no_z_fighting() {
    let city = GeneratedCity::generate(Seed(1));
    let views = [
        (None, Detail::Near),
        (None, Detail::Far),
        (Some(0), Detail::Near),
        (Some(1), Detail::Far),
        (Some(3), Detail::Near),
    ];
    for (cut, detail) in views {
        let clashes = zfight::find(&zfight::city(&city, cut, detail), &zfight::city_options());
        let fighting: Vec<_> = clashes.into_iter().filter(|c| !c.same_color).collect();
        let spots = zfight::spots(&fighting, 10.0);
        let worst: Vec<String> = spots
            .iter()
            .take(8)
            .map(|s| {
                format!(
                    "{:.2} m² at ({:.1}, {:.2}, {:.1}) {} vs {} [{} / {}]",
                    s.area,
                    s.at.x,
                    s.at.y,
                    s.at.z,
                    hex(s.colors[0]),
                    hex(s.colors[1]),
                    source_name(s.sources[0]),
                    source_name(s.sources[1])
                )
            })
            .collect();
        assert!(
            spots.is_empty(),
            "cut {cut:?}, {detail:?}: {} z-fighting spots; worst:\n{}\n(cargo run -p hw-client --bin zfight)",
            spots.len(),
            worst.join("\n")
        );
    }
}
