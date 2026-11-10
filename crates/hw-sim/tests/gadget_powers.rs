//! The powers of 0.117, headless: the Freeze Ray freezes minions (and the
//! next blow shatters the ice), Teleport never lands inside a wall, the
//! Robot Sidekick keeps at its hero's side and fights, and the Hypno Ray
//! turns a minion on its own side.

use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_net::{EntityKind, FxKind, InputState, PlayerId, Power};
use hw_sim::balance::hero_sheet;
use hw_sim::collision::{Body, Obstacles, Solid};
use hw_sim::combat::{charmable, freezable, teleport_to, EnemyKind, FROZEN_SECONDS};
use hw_sim::{sidekick, Sim, GROUND_Y, TICK_HZ};
use std::sync::Arc;

fn arena(solids: Obstacles) -> Sim {
    let mut sim = Sim::with_solids(Seed(5), Arc::new(solids));
    sim.set_spawn((DVec2::new(35.0, 35.0), DVec2::X));
    sim
}

fn ground(x: f64, z: f64) -> DVec3 {
    DVec3::new(x, GROUND_Y, z)
}

/// Presses `input` once (then only keeps the target and aim) for `ticks`
/// ticks; returns every fight event seen.
fn press(sim: &mut Sim, input: InputState, ticks: u64) -> Vec<hw_net::Fx> {
    let mut seen = Vec::new();
    for k in 0..ticks {
        let i = if k == 0 {
            input
        } else {
            InputState {
                target: input.target,
                aim: input.aim,
                ..Default::default()
            }
        };
        sim.set_input(PlayerId(1), i);
        sim.step();
        seen.extend(sim.fx().iter().copied());
    }
    seen
}

fn use_power(p: Power, target: Option<NetId>, aim: DVec3) -> InputState {
    InputState {
        power: p.code(),
        target,
        aim: aim.as_vec3().to_array(),
        ..Default::default()
    }
}

/// A hero with `powers` at the origin and a group of minions `at` metres
/// down the street, all standing where they are put.
fn street(powers: &[Power], at: f64) -> (Sim, NetId, Vec<NetId>) {
    let mut sim = arena(Obstacles::flat(GROUND_Y));
    let hero = sim.spawn_hero_with(PlayerId(1), ground(0.0, 0.0), hero_sheet(20, powers));
    sim.step();
    sim.add_fight_spot(ground(at, 0.0), DVec2::new(-1.0, 0.0));
    sim.step();
    let foes: Vec<NetId> = sim.enemies().iter().map(|e| e.0).collect();
    (sim, hero, foes)
}

fn put(sim: &mut Sim, id: NetId, at: DVec3) {
    sim.set_body(
        id,
        Body {
            pos: at,
            vel: DVec3::ZERO,
            grounded: true,
        },
    );
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn the_freeze_ray_freezes_minions_and_a_blow_shatters_the_ice() {
    let (mut sim, _hero, foes) = street(&[Power::Freeze, Power::Bolt], 30.0);
    let kind = sim.enemies()[0].1;
    assert!(freezable(kind), "the street group are minions");
    // Two side by side in the cone, the rest far off to the side.
    for (k, &f) in foes.iter().enumerate() {
        let at = match k {
            0 => ground(8.0, 0.0),
            1 => ground(9.0, 0.8),
            _ => ground(9.0, 30.0 + 3.0 * k as f64),
        };
        put(&mut sim, f, at);
    }
    let fx = press(
        &mut sim,
        use_power(Power::Freeze, Some(foes[0]), DVec3::X),
        2,
    );
    assert!(fx.iter().any(|e| e.kind == FxKind::Freeze));
    let tick = sim.tick;
    for &f in &foes[..2] {
        assert!(sim.fighter(f).unwrap().frozen(tick), "caught in the cone");
    }
    for &f in &foes[2..] {
        assert!(!sim.fighter(f).unwrap().frozen(tick), "out of the cone");
    }
    // Frozen, they stay put however long they are angry.
    let before = sim.position(foes[0]).unwrap();
    press(&mut sim, InputState::default(), TICK_HZ as u64);
    let after = sim.position(foes[0]).unwrap();
    assert!(
        before.distance(after) < 0.05,
        "a frozen minion moved {before} -> {after}"
    );
    // A bolt shatters the ice for double damage and frees it.
    let health = sim.fighter(foes[0]).unwrap().health;
    let fx = press(
        &mut sim,
        use_power(Power::Bolt, Some(foes[0]), DVec3::X),
        TICK_HZ as u64 / 2,
    );
    assert!(fx
        .iter()
        .any(|e| e.kind == FxKind::Shatter && e.to == foes[0]));
    let f = sim.fighter(foes[0]).unwrap();
    assert!(!f.frozen(sim.tick));
    assert!(f.health < health, "the shattering blow hurt");
    // The ice of the other melts on its own.
    let rest = (FROZEN_SECONDS * TICK_HZ as f64) as u64;
    press(&mut sim, InputState::default(), rest);
    assert!(!sim.fighter(foes[1]).unwrap().frozen(sim.tick));
}

#[test]
fn teleport_lands_short_of_a_wall_never_inside_it() {
    // A thick wall across the street 15 m ahead.
    let mut solids = Obstacles::flat(GROUND_Y);
    solids.add(Solid {
        origin: DVec2::new(15.0, -20.0),
        along: DVec2::new(0.0, 1.0),
        inward: DVec2::new(1.0, 0.0),
        min: DVec2::new(0.0, 0.0),
        max: DVec2::new(40.0, 2.0),
        y0: GROUND_Y,
        y1: GROUND_Y + 12.0,
    });
    // Straight at it: the hero arrives just this side.
    let to = teleport_to(&solids, ground(0.0, 0.0), DVec3::X).expect("room before the wall");
    assert!(to.x > 10.0 && to.x < 15.0, "landed at {to}");
    for h in [0.2, 1.0, 1.7] {
        assert!(!solids.solid_at(to + DVec3::Y * h));
    }
    // Into the ground: nowhere to go.
    assert!(teleport_to(&solids, ground(0.0, 0.0), DVec3::NEG_Y).is_none());
    // In the game: the hero moves, and pays nothing when there is no room.
    let mut sim = arena(solids);
    let hero = sim.spawn_hero_with(
        PlayerId(1),
        ground(0.0, 0.0),
        hero_sheet(20, &[Power::Teleport]),
    );
    sim.step();
    let fx = press(&mut sim, use_power(Power::Teleport, None, DVec3::X), 2);
    assert!(fx.iter().any(|e| e.kind == FxKind::TeleportIn));
    let at = sim.position(hero).unwrap();
    assert!(at.x > 10.0 && at.x < 15.0, "hero at {at}");
    let energy = sim.fighter(hero).unwrap().energy;
    press(
        &mut sim,
        use_power(Power::Teleport, None, DVec3::NEG_Y),
        TICK_HZ as u64 * 4,
    );
    assert!(
        sim.fighter(hero).unwrap().energy >= energy,
        "no room, no cost"
    );
}

fn sidekick_of(sim: &mut Sim, owner: NetId) -> Option<(NetId, DVec3)> {
    sim.snapshot()
        .into_iter()
        .find(|e| e.kind == EntityKind::Sidekick && e.mount.map(|m| m.0) == Some(owner))
        .map(|e| (e.id, DVec3::from_array(e.pos)))
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn the_sidekick_keeps_at_its_heros_side_and_fights() {
    let (mut sim, hero, foes) = street(&[Power::Sidekick], 60.0);
    let fx = press(&mut sim, use_power(Power::Sidekick, None, DVec3::X), 2);
    assert!(fx.iter().any(|e| e.kind == FxKind::SidekickCall));
    let (pal, _) = sidekick_of(&mut sim, hero).expect("a sidekick");
    // The hero runs off: it follows, and beams back when left far behind.
    put(&mut sim, hero, ground(-60.0, 0.0));
    press(&mut sim, InputState::default(), TICK_HZ as u64 * 3);
    let (_, at) = sidekick_of(&mut sim, hero).unwrap();
    assert!(at.distance(ground(-60.0, 0.0)) < 6.0, "sidekick at {at}");
    // Sent at a minion, it goes and punches it.
    put(&mut sim, foes[0], ground(-50.0, 0.0));
    let health = sim.fighter(foes[0]).unwrap().health;
    let fx = press(
        &mut sim,
        use_power(Power::Sidekick, Some(foes[0]), DVec3::X),
        TICK_HZ as u64 * 6,
    );
    assert!(
        fx.iter().any(|e| e.from == pal && e.to == foes[0]),
        "it went for the minion"
    );
    assert!(sim.fighter(foes[0]).is_none_or(|f| f.health < health));
    // Only one at a time, and it goes when the hero does.
    let count = sim
        .snapshot()
        .iter()
        .filter(|e| e.kind == EntityKind::Sidekick)
        .count();
    assert_eq!(count, 1);
    const { assert!(sidekick::SIZE < 0.8, "drawn small") };
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn the_hypno_ray_turns_a_minion_against_its_friends() {
    let (mut sim, _hero, foes) = street(&[Power::Hypno], 12.0);
    assert!(charmable(sim.enemies()[0].1));
    assert!(
        !charmable(EnemyKind::Colossus),
        "machines have no mind to bend"
    );
    let fx = press(
        &mut sim,
        use_power(Power::Hypno, Some(foes[0]), DVec3::X),
        2,
    );
    assert!(fx
        .iter()
        .any(|e| e.kind == FxKind::Hypno && e.to == foes[0]));
    assert!(sim.fighter(foes[0]).unwrap().charmed(sim.tick));
    // It goes for its old friends, and they for it.
    let fx = press(&mut sim, InputState::default(), TICK_HZ as u64 * 8);
    let others = &foes[1..];
    assert!(
        fx.iter()
            .any(|e| e.from == foes[0] && others.contains(&e.to)),
        "the charmed minion fought its friends"
    );
}
