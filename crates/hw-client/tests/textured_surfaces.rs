//! Nothing big is drawn as flat paint: the walls, roofs, trim and floors of
//! every building, in the golden-age city and in towns of every region,
//! closed and cut open, carry a texture. Glass, metal and glowing parts
//! are surfaces of their own and do not count.

use hw_client::buildingmesh::Detail;
use hw_client::citymesh::{add_building, Look, MeshBuilder};
use hw_core::Seed;
use hw_sim::{GeneratedCity, GeneratedTown};

/// Most of a building's plain faces (by area) that may be bare.
const MAX_BARE: f64 = 0.03;

fn check(name: &str, b: &MeshBuilder, worst: &mut Vec<(f64, String)>) {
    let share = b.bare_share();
    if share > MAX_BARE {
        worst.push((share, name.to_string()));
    }
}

#[test]
#[ignore = "on demand (looks): scripts/check.sh --on-demand"]
fn buildings_are_textured() {
    let mut bad = Vec::new();
    let city = GeneratedCity::generate(Seed(1));
    for (i, plan) in city.buildings.iter().enumerate().step_by(7) {
        let lot = city.plan.buildings[i].0;
        let kind = city.plan.districts[city.plan.blocks[city.plan.lots[lot].block].district].kind;
        for (look, detail) in [
            (Look::Shell, Detail::Near),
            (Look::Shell, Detail::Far),
            (Look::Cut { level: 0 }, Detail::Near),
        ] {
            let mut b = MeshBuilder::new();
            add_building(&mut b, plan, kind, look, detail);
            check(
                &format!("city building {i} {look:?} {detail:?}"),
                &b,
                &mut bad,
            );
        }
    }
    // Towns of the three regions (the region comes from the seed).
    for seed in [1, 2, 3, 4] {
        let town = GeneratedTown::generate(Seed(seed), 8000.0);
        for i in town.area.building_ids.iter().step_by(11).map(|id| id.0) {
            let Some((plan, _)) = town.obstacles.building(i) else {
                continue;
            };
            let kind = plan.site.district;
            for look in [Look::Shell, Look::Cut { level: 0 }] {
                let mut b = MeshBuilder::new();
                add_building(&mut b, &plan, kind, look, Detail::Near);
                check(&format!("town {seed} building {i} {look:?}"), &b, &mut bad);
            }
        }
    }
    bad.sort_by(|a, b| b.0.total_cmp(&a.0));
    assert!(
        bad.is_empty(),
        "{} buildings have more than {:.0}% of their surface untextured; worst:\n{}",
        bad.len(),
        MAX_BARE * 100.0,
        bad.iter()
            .take(10)
            .map(|(s, n)| format!("  {n}: {:.0}% bare", s * 100.0))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
