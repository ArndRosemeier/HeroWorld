//! A taxi ride across town, headless: hail one at a stand, get in when it
//! comes down, ride above the roofs to another stand, step out there. The
//! hero's own input does nothing while riding; hopping out drops the hero
//! where the taxi is.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_net::{EntityKind, PlayerId};
use hw_sim::collision::Body;
use hw_sim::taxi::{riding, HOP_OUT};
use hw_sim::{GeneratedTown, Sim, TICK_HZ};

fn town_sim() -> (Sim, PlayerId, hw_core::NetId) {
    let town = GeneratedTown::generate(Seed(3), 20_000.0);
    let mut sim = Sim::for_town(Seed(3), &town);
    let me = PlayerId(1);
    let hero = sim.spawn_hero(me, sim.spawn_point(1));
    (sim, me, hero)
}

fn place(sim: &mut Sim, hero: hw_core::NetId, at: DVec3) {
    sim.set_body(
        hero,
        Body {
            pos: at + DVec3::Y * 0.2,
            vel: DVec3::ZERO,
            grounded: false,
        },
    );
}

#[test]
fn a_taxi_ride_crosses_the_town() {
    let (mut sim, me, hero) = town_sim();
    let stands = sim.taxis.stands.clone();
    assert!(stands.len() >= 4, "{} stands", stands.len());
    // From the stand farthest from the last one, to the last one.
    let to = stands.len() - 1;
    let from = (0..to)
        .max_by(|a, b| {
            let d = |i: usize| stands[i].pos.distance(stands[to].pos);
            d(*a).total_cmp(&d(*b))
        })
        .unwrap();
    let start = stands[from].pos;
    place(&mut sim, hero, start);
    for _ in 0..TICK_HZ {
        sim.step();
    }
    sim.hail_taxi(me, to as u8);
    assert!(sim.taxis.has_ride(me));
    let mut rode = 0u32;
    let mut highest = f64::MIN;
    let mut was_riding = false;
    let mut done_at = None;
    for t in 0..(TICK_HZ * 300) {
        // Pressing keys while riding changes nothing.
        let input = hw_net::InputState {
            move_x: 1.0,
            ..Default::default()
        };
        sim.set_input(me, input);
        sim.step();
        let all = sim.snapshot();
        let mine = all.iter().find(|e| e.id == hero).unwrap();
        let now = riding(mine, &all);
        if now {
            rode += 1;
            highest = highest.max(mine.pos[1]);
            let cab = all.iter().find(|e| e.kind == EntityKind::Taxi).unwrap();
            assert_eq!(cab.owner, Some(me));
        } else if !was_riding && rode == 0 {
            // Until the taxi is there, walk back to where it will land.
            place(&mut sim, hero, start);
        }
        if was_riding && !now {
            done_at = Some(t);
            break;
        }
        was_riding = now;
    }
    assert!(done_at.is_some(), "never arrived");
    let end = sim.position(hero).unwrap();
    let off = DVec2::new(end.x - stands[to].pos.x, end.z - stands[to].pos.z).length();
    assert!(off < 4.0, "let out {off:.1} m from the stand");
    assert!(
        (end.y - stands[to].pos.y).abs() < 3.0,
        "let out at the wrong height"
    );
    let span = DVec2::new(start.x - end.x, start.z - end.z).length();
    assert!(span > 300.0, "only {span:.0} m");
    let secs = rode as f64 / TICK_HZ as f64;
    assert!(secs < 120.0, "{secs:.0} s for {span:.0} m");
    // Above the roofs on the way.
    assert!(
        highest > start.y.max(end.y) + 40.0,
        "flew at {highest:.0} m"
    );
    // The empty taxi flies off and is gone.
    for _ in 0..(TICK_HZ * 20) {
        sim.step();
    }
    assert!(sim.snapshot().iter().all(|e| e.kind != EntityKind::Taxi));
    assert!(!sim.taxis.has_ride(me));
}

#[test]
fn hopping_out_drops_the_hero_where_the_taxi_is() {
    let (mut sim, me, hero) = town_sim();
    let stands = sim.taxis.stands.clone();
    let start = stands[0].pos;
    place(&mut sim, hero, start);
    sim.hail_taxi(me, (stands.len() - 1) as u8);
    let mut in_air = None;
    for _ in 0..(TICK_HZ * 60) {
        sim.step();
        let all = sim.snapshot();
        let mine = all.iter().find(|e| e.id == hero).unwrap();
        if riding(mine, &all) && mine.pos[1] > start.y + 20.0 {
            in_air = Some(DVec3::from_array(mine.pos));
            break;
        }
    }
    let at = in_air.expect("taxi never climbed");
    sim.hail_taxi(me, HOP_OUT);
    sim.step();
    let all = sim.snapshot();
    let mine = all.iter().find(|e| e.id == hero).unwrap();
    assert!(!riding(mine, &all));
    assert!(DVec3::from_array(mine.pos).distance(at) < 4.0);
    // And down they fall.
    for _ in 0..(TICK_HZ * 6) {
        sim.step();
    }
    assert!(sim.position(hero).unwrap().y < at.y - 10.0);
}
