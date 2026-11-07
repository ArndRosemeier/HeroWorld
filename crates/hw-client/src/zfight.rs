//! The city's z-fighting check, on Engine's finder ([`engine::zfight`]).
//!
//! The `zfight` tool runs it on a whole generated city and the test
//! `tests/no_zfighting.rs` keeps the city clean. In the running game,
//! `ENGINE_ZFIGHT=1` marks clashes around the camera with magenta posts.

pub use engine::zfight::{hex, Clash, Options, ZFightCheck, ZFightReport};

/// The options for a city: stricter than Engine's default (1 cm), and the
/// bottoms of things resting on the ground (or the water) are hidden.
pub fn city_options() -> Options {
    Options {
        gap: 0.01,
        ..Options::ground_at(0.0)
    }
}

/// Every triangle of a generated city as the game draws it at `detail`: the
/// ground (named "ground") and each building ("building N"), closed, or cut
/// open at floor `cut` where it has one.
pub fn city(
    generated: &hw_sim::GeneratedCity,
    cut: Option<u32>,
    detail: crate::buildingmesh::Detail,
) -> ZFightCheck {
    use crate::citymesh::{add_building, ground_mesh, Look, MeshBuilder};
    let plan = &generated.plan;
    let mut check = ZFightCheck::new().options(city_options());
    // The water and its bed lie metres below everything and only under
    // each other; leaving them out keeps the check fast.
    for (name, mesh) in ground_mesh(plan) {
        if name == "ground" {
            check = check.with(name, &mesh);
        }
    }
    for (i, b) in generated.buildings.iter().enumerate() {
        let lot = plan.buildings[i].0;
        let kind = plan.districts[plan.blocks[plan.lots[lot].block].district].kind;
        let look = match cut {
            Some(level) if (level as usize) < b.floors.len() => Look::Cut { level },
            _ => Look::Shell,
        };
        let mut m = MeshBuilder::new();
        add_building(&mut m, b, kind, look, detail);
        check = check.with(format!("building {i}"), &m.finish());
    }
    check
}
