//! Villains follow a hero through a game-scale interior: out of the
//! atrium's floor, round the furniture, up the stairs to the gallery the
//! hero ran to. The building stands alone on flat ground, with its route
//! map ([`hw_sim::floornav`]) like the town's Comet buildings.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_gen::layout::furnish::items;
use hw_gen::layout::office::plan_scheme;
use hw_gen::layout::solid::{pieces, Part};
use hw_gen::layout::{Layout, Scheme, SpaceKind};
use hw_net::{PlayerId, Status};
use hw_sim::collision::{solids_of, Body, Obstacles};
use hw_sim::combat::Fights;
use hw_sim::floornav::{Building, FloorGraph, Floors};
use hw_sim::{Sim, GROUND_Y, TICK_HZ};
use std::sync::Arc;

/// A building on flat ground, its layout's frame the world's (lifted to
/// the ground).
fn building(scheme: Scheme) -> (Sim, Layout) {
    let size = DVec2::new(56.0, 52.0);
    let l = (0..16u64)
        .find_map(|k| plan_scheme(Seed(40 + k), size, 5, scheme))
        .expect("it fits");
    let mut o = Obstacles::flat(GROUND_Y);
    let lift = |f: hw_gen::urban::footprint::Footprint| f.placed(DVec2::ZERO, DVec2::Y, GROUND_Y);
    o.add_all(
        pieces(&l)
            .iter()
            .filter(|p| p.part != Part::LiftDoor)
            .flat_map(|p| solids_of(&lift(p.shape)))
            .chain(
                items(&l)
                    .iter()
                    .filter_map(|i| i.footprint())
                    .flat_map(|f| solids_of(&lift(f))),
            )
            .collect(),
    );
    let mut sim = Sim::with_solids(Seed(5), Arc::new(o));
    sim.set_spawn((DVec2::new(size.x * 0.5, -8.0), DVec2::X));
    sim.world.resource_mut::<Fights>().floors = Some(Arc::new(Floors::of(vec![Building::at(
        size * 0.5,
        DVec2::new(0.0, -1.0),
        GROUND_Y,
        FloorGraph::new(&l),
    )])));
    (sim, l)
}

#[test]
fn villains_follow_a_hero_up_the_stairs() {
    for scheme in [Scheme::Atrium, Scheme::GrandHall, Scheme::SplitLevel] {
        let (mut sim, l) = building(scheme);
        // The group stands in the biggest ground-floor hall; the hero comes
        // in where they see it.
        let hall = l
            .spaces
            .iter()
            .filter(|s| s.within.is_none() && s.y == 0.0 && s.kind != SpaceKind::Shaft)
            .max_by(|a, b| a.rect.area().total_cmp(&b.rect.area()))
            .expect("a hall");
        let c = hall.rect.center();
        let hero = sim.spawn_hero(PlayerId(1), DVec3::new(c.x + 3.0, GROUND_Y, c.y));
        sim.add_fight_spot(DVec3::new(c.x - 3.0, GROUND_Y, c.y), DVec2::X);
        for _ in 0..TICK_HZ * 2 {
            sim.step();
        }
        // Up to a hall on the second floor, out of their sight.
        let up = l
            .spaces
            .iter()
            .filter(|s| s.within.is_none() && s.y > 5.0 && s.kind == SpaceKind::Hall)
            .min_by(|a, b| {
                (a.y, a.rect.center().distance(c))
                    .partial_cmp(&(b.y, b.rect.center().distance(c)))
                    .expect("numbers")
            })
            .expect("an upper hall");
        // A spot in it clear of furniture: its middle or near a door.
        let graph = FloorGraph::new(&l);
        let spot = graph
            .points()
            .into_iter()
            .filter(|p| (p.y - up.y).abs() < 0.01 && up.rect.contains_point(DVec2::new(p.x, p.z)))
            .min_by(|a, b| {
                a.distance(DVec3::new(c.x, a.y, c.y))
                    .total_cmp(&b.distance(DVec3::new(c.x, b.y, c.y)))
            })
            .expect("a point in the hall");
        let at = spot + DVec3::Y * GROUND_Y;
        let mut reached = false;
        for _ in 0..TICK_HZ * 90 {
            sim.set_body(
                hero,
                Body {
                    pos: at,
                    grounded: true,
                    ..Body::default()
                },
            );
            // The hero holds out: the test is about the walk.
            if let Some(mut f) = sim.fighter(hero) {
                f.health = f.max_health;
                sim.set_fighter(hero, f);
            }
            sim.step();
            let near = sim.enemies().into_iter().any(|(_, _, p, f)| {
                f.status == Status::Standing
                    && DVec2::new(p.x - at.x, p.z - at.z).length() < 4.0
                    && (p.y - at.y).abs() < 1.0
            });
            if near {
                reached = true;
                break;
            }
        }
        let where_: Vec<DVec3> = sim.enemies().into_iter().map(|e| e.2).collect();
        assert!(
            reached,
            "{scheme:?}: nobody came up to {at:.1?}; villains at {where_:.1?}"
        );
    }
}
