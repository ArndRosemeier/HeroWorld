//! Breaking a robo-sentry's shield dish must not take the sentry itself
//! out of the fight.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_net::{InputState, PlayerId, Status};
use hw_sim::collision::Obstacles;
use hw_sim::combat::{EnemyKind, Kit};
use hw_sim::{Sim, GROUND_Y};
use std::sync::Arc;

#[test]
fn breaking_the_dish_leaves_the_sentry_standing() {
    let mut sim = Sim::with_solids(Seed(5), Arc::new(Obstacles::flat(GROUND_Y)));
    sim.set_spawn((DVec2::ZERO, DVec2::X));
    let me = sim.spawn_hero(PlayerId(1), DVec3::new(0.0, GROUND_Y, 0.0));
    let old: Vec<_> = hw_sim::balance::CLASSIC
        .iter()
        .filter(|c| c.1 <= 4)
        .map(|c| c.0)
        .collect();
    sim.set_fighter(me, hw_sim::combat::Fighter::hero_at(8, Kit::with(&old)));
    sim.add_fight_spot_of(
        DVec3::new(14.0, GROUND_Y, 0.0),
        DVec2::new(-1.0, 0.0),
        EnemyKind::Sentry,
        1,
    );
    let mut dish_down_at = None;
    for tick in 0..60 * 120 {
        let foes = sim.enemies();
        let at = sim.position(me).unwrap();
        let dish = foes
            .iter()
            .find(|e| e.1 == EnemyKind::WeakPoint)
            .map(|e| (e.0, e.2, e.3.status));
        let mut input = InputState::default();
        if let Some((id, p, st)) = dish {
            if st == Status::Standing || st == Status::Knocked {
                input.target = Some(id);
                input.aim = (p - at).normalize_or_zero().as_vec3().to_array();
                input.blast = tick % 4 == 0;
            } else if dish_down_at.is_none() {
                dish_down_at = Some(tick);
            }
        }
        sim.set_input(PlayerId(1), input);
        sim.step();
        if let Some(t) = dish_down_at {
            if tick > t + 60 * 10 {
                break;
            }
        }
    }
    let t = dish_down_at.expect("the dish was never broken");
    let _ = t;
    let sentry = sim
        .enemies()
        .into_iter()
        .find(|e| e.1 == EnemyKind::Sentry)
        .expect("the sentry was carried off with its dish");
    assert!(
        sentry.3.health > 0.0 && sentry.3.status != Status::Down,
        "sentry status {:?}, health {}",
        sentry.3.status,
        sentry.3.health
    );
}
