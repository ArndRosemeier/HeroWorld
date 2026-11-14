//! Travel powers, headless: a super jump clears rooftops and grows with the
//! level, a tap is a shorter hop, sprint and super speed are clearly faster
//! than running, and a hard landing stomps the villains round the hero.

use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_net::{FxKind, InputState, PlayerId, Status};
use hw_sim::balance::hero_sheet;
use hw_sim::collision::{Body, Obstacles};
use hw_sim::{step_moving, Moves, Sim, DT, GROUND_Y, RUN_SPEED, TICK_HZ};
use std::sync::Arc;

fn ground(x: f64, z: f64) -> DVec3 {
    DVec3::new(x, GROUND_Y, z)
}

fn jumper(level: u8) -> Moves {
    Moves {
        super_jump: true,
        level,
        ..Moves::ON_FOOT
    }
}

/// The highest a hero gets over flat ground holding jump for `hold` ticks.
fn peak(moves: Moves, hold: u32) -> f64 {
    let solids = Obstacles::flat(GROUND_Y);
    let mut b = Body {
        pos: ground(0.0, 0.0),
        vel: DVec3::ZERO,
        grounded: true,
    };
    let mut top: f64 = 0.0;
    for k in 0..TICK_HZ * 8 {
        let input = InputState {
            jump: k < hold,
            ..Default::default()
        };
        step_moving(&mut b, input, &solids, moves);
        top = top.max(b.pos.y - GROUND_Y);
    }
    top
}

/// Top speed on flat ground running straight ahead for three seconds.
fn top_speed(moves: Moves, boost: bool) -> f64 {
    let solids = Obstacles::flat(GROUND_Y);
    let mut b = Body {
        pos: ground(0.0, 0.0),
        vel: DVec3::ZERO,
        grounded: true,
    };
    for _ in 0..TICK_HZ * 3 {
        let input = InputState {
            move_x: 1.0,
            boost,
            ..Default::default()
        };
        step_moving(&mut b, input, &solids, moves);
    }
    DVec2::new(b.vel.x, b.vel.z).length()
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn a_super_jump_clears_rooftops_and_grows_with_the_level() {
    let held = |level| peak(jumper(level), TICK_HZ * 8);
    let (low, high) = (held(1), held(20));
    // Over a three storey house at level 1, a ten storey block at level 20.
    assert!(low > 13.0, "level 1: {low:.1} m");
    assert!(high > 43.0, "level 20: {high:.1} m");
    assert!(held(10) > low + 10.0 && held(10) < high - 10.0);
    // A tap is a hop of about half the height, still well over a plain jump.
    let tap = peak(jumper(20), 2);
    assert!(
        tap < high * 0.65 && tap > high * 0.4,
        "tap {tap:.1} of {high:.1}"
    );
    assert!(peak(Moves::ON_FOOT, 2) < 2.0);
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn a_super_jump_takes_off_at_leap_speed_and_steers() {
    let solids = Obstacles::flat(GROUND_Y);
    let mut b = Body {
        pos: ground(0.0, 0.0),
        vel: DVec3::ZERO,
        grounded: true,
    };
    // From a standstill, jump while pushing forward: off at once.
    let go = InputState {
        jump: true,
        move_x: 1.0,
        ..Default::default()
    };
    step_moving(&mut b, go, &solids, jumper(1));
    assert!(!b.grounded && b.vel.x >= 0.8 * hw_sim::super_leap_speed(1) - 1e-9);
    // Half a second of steering the other way turns the leap round.
    for _ in 0..TICK_HZ / 2 {
        let back = InputState {
            jump: true,
            move_x: -1.0,
            ..Default::default()
        };
        step_moving(&mut b, back, &solids, jumper(1));
    }
    assert!(b.vel.x < 0.0, "still {:.1} m/s forward", b.vel.x);
    let _ = DT;
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn sprint_and_super_speed_are_clearly_faster_and_grow() {
    let sprint = |level| {
        top_speed(
            Moves {
                sprint: true,
                level,
                ..Moves::ON_FOOT
            },
            false,
        )
    };
    let speed = |level, boost| {
        top_speed(
            Moves {
                speed: true,
                level,
                ..Moves::ON_FOOT
            },
            boost,
        )
    };
    let run = top_speed(Moves::ON_FOOT, false);
    assert!((run - RUN_SPEED).abs() < 0.1);
    assert!(sprint(1) > run * 1.8, "sprint {:.1}", sprint(1));
    assert!(sprint(20) > sprint(1) + 3.0);
    assert!(speed(1, false) > sprint(20), "super speed beats any sprint");
    assert!(speed(20, false) > speed(1, false) + 8.0);
    assert!(speed(20, true) > speed(1, true) + 15.0);
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn a_hard_landing_stomps_the_villains_round_the_hero() {
    let mut sim = Sim::with_solids(Seed(9), Arc::new(Obstacles::flat(GROUND_Y)));
    sim.set_spawn((DVec2::new(35.0, 35.0), DVec2::X));
    let hero = sim.spawn_hero_with(PlayerId(1), ground(0.0, 0.0), hero_sheet(10, &[]));
    sim.step();
    sim.add_fight_spot(ground(30.0, 0.0), DVec2::new(-1.0, 0.0));
    sim.step();
    let foes: Vec<NetId> = sim.enemies().iter().map(|e| e.0).collect();
    assert!(foes.len() >= 2);
    // Two minions where the hero comes down, the rest far off.
    for (k, &f) in foes.iter().enumerate() {
        let at = match k {
            0 => ground(1.5, 0.0),
            1 => ground(-1.0, 2.0),
            _ => ground(40.0, 30.0 + 3.0 * k as f64),
        };
        sim.set_body(
            f,
            Body {
                pos: at,
                vel: DVec3::ZERO,
                grounded: true,
            },
        );
    }
    // The hero drops in from a leap's height.
    sim.set_body(
        hero,
        Body {
            pos: ground(0.0, 0.0) + DVec3::Y * 25.0,
            vel: DVec3::ZERO,
            grounded: false,
        },
    );
    let mut fx = Vec::new();
    for _ in 0..TICK_HZ * 3 {
        sim.set_input(PlayerId(1), InputState::default());
        sim.step();
        fx.extend(sim.fx().iter().copied());
    }
    let own = fx
        .iter()
        .find(|e| e.kind == FxKind::Landing && e.from == hero && e.to == hero)
        .expect("a hard landing");
    assert!(own.knock, "the stomp caught someone");
    for &f in &foes[..2] {
        let hit = fx
            .iter()
            .find(|e| e.kind == FxKind::Landing && e.to == f)
            .unwrap_or_else(|| panic!("{f:?} was not stomped"));
        assert!(hit.amount > 0 && (hit.knock || hit.defeat));
    }
    for &f in &foes[2..] {
        assert!(!fx.iter().any(|e| e.kind == FxKind::Landing && e.to == f));
        assert_eq!(sim.fighter(f).unwrap().status, Status::Standing);
    }
    // Dropped in, not jumped: no take-off shows.
    assert!(!fx.iter().any(|e| e.kind == FxKind::Launch));
}

fn swinger() -> Moves {
    Moves {
        grapple: true,
        super_jump: true,
        level: 5,
        ..Moves::ON_FOOT
    }
}

/// Where a hero ends up 1.5 s after starting at rest 30 m up, `hold`ing the
/// grapple key aimed forward-down (45 degrees) with `moves`.
fn drop_with_line(moves: Moves, hold: bool, aim: [f32; 3]) -> Body {
    let solids = Obstacles::flat(GROUND_Y);
    let mut b = Body {
        pos: ground(0.0, 0.0) + DVec3::Y * 30.0,
        vel: DVec3::ZERO,
        grounded: false,
    };
    for _ in 0..(TICK_HZ * 3 / 2) {
        let input = InputState {
            grapple: hold,
            aim,
            ..Default::default()
        };
        step_moving(&mut b, input, &solids, moves);
    }
    b
}

#[test]
fn the_line_pulls_a_hero_toward_what_they_aim_at() {
    let s = std::f32::consts::FRAC_1_SQRT_2;
    let down_ahead = [s, -s, 0.0];
    let swing = drop_with_line(swinger(), true, down_ahead);
    let fall = drop_with_line(swinger(), false, down_ahead);
    assert!(fall.pos.x.abs() < 0.01, "no key, no line: {:?}", fall.pos);
    assert!(
        swing.pos.x > 8.0,
        "the line carried the hero ahead: {:?}",
        swing.pos
    );
    // The speed is held under what the line allows.
    assert!(swing.vel.length() <= hw_sim::grapple_speed(5) * 1.15 + 1e-6);
}

#[test]
fn the_line_needs_the_power_and_something_to_hook() {
    let s = std::f32::consts::FRAC_1_SQRT_2;
    // Without Grapple Swing the key does nothing.
    let plain = drop_with_line(Moves::ON_FOOT, true, [s, -s, 0.0]);
    assert!(plain.pos.x.abs() < 0.01, "{:?}", plain.pos);
    // Aimed at empty sky, nothing is hooked.
    let sky = drop_with_line(swinger(), true, [s, s, 0.0]);
    assert!(sky.pos.x.abs() < 0.01, "{:?}", sky.pos);
    // Out of reach (level 5 reaches under 60 m): aimed nearly flat, the
    // ground is far beyond the line.
    let far = drop_with_line(swinger(), true, [1.0, -0.01, 0.0]);
    assert!(far.pos.x.abs() < 0.01, "{:?}", far.pos);
}

#[test]
fn glide_is_retired_and_its_karma_comes_back() {
    use hw_net::{OwnedPower, PowerId};
    let mut s = hw_sim::progress::new_hero(0, hw_sim::powers::Element::Frost);
    let before = s.karma;
    s.powers.push(OwnedPower {
        power: PowerId(4),
        paid: 1440,
    });
    let s = hw_sim::progress::sanitize(s);
    assert!(!s.powers.iter().any(|p| p.power == PowerId(4)));
    assert_eq!(s.karma, before + 1440);
    // Grapple Swing is in the travel tree, next to Flight.
    let d = hw_sim::powers::get(PowerId(9)).expect("Grapple Swing");
    assert_eq!(d.name, "Grapple Swing");
}
