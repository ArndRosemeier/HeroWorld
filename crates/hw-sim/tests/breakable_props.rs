//! Milestone 8, street props: lamps and trees fall over the way they were
//! hit, cars flip, a statue needs a stronger hero, a late joiner and a save
//! see the same, and props nobody sees are put back.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_gen::urban::props::PropKind;
use hw_net::{InputState, PlayerId, ToClient};
use hw_sim::collision::Body;
use hw_sim::destruction::{self, REPAIR_SECONDS};
use hw_sim::props::{byte_to_yaw, toughness, PropDamage};
use hw_sim::{GeneratedTown, Sim, TICK_HZ};

fn nearest(t: &GeneratedTown, want: impl Fn(PropKind) -> bool) -> u64 {
    let (spawn, _) = t.world.spawn_line();
    let a = &t.area;
    let k = (0..a.props.len())
        .filter(|&i| want(a.props[i].kind))
        .min_by(|&x, &y| {
            let d = |i: usize| a.props[i].pos.distance(spawn);
            d(x).total_cmp(&d(y))
        })
        .expect("a prop");
    a.prop_ids[k].0
}

fn prop(t: &GeneratedTown, i: u64) -> hw_gen::urban::props::Prop {
    t.world
        .prop(hw_gen::urban::quarter::DetailId(i))
        .expect("a prop")
}

#[test]
fn a_punch_fells_a_lamp_away_from_the_hero_and_everyone_sees_it() {
    let t = GeneratedTown::generate(Seed(1), 6000.0);
    let i = nearest(&t, |k| matches!(k, PropKind::Lamp(_)));
    let lamp = prop(&t, i);
    let mut sim = Sim::for_town(Seed(1), &t);
    let me = PlayerId(1);
    let hero = sim.spawn_hero(me, DVec3::ZERO);
    // Stand 1 m west of the post, punching east.
    let at = DVec3::new(lamp.pos.x - 1.0, lamp.y, lamp.pos.y);
    sim.set_body(
        hero,
        Body {
            pos: at,
            ..Default::default()
        },
    );
    sim.set_input(
        me,
        InputState {
            smash: true,
            aim: [1.0, 0.0, 0.0],
            ..Default::default()
        },
    );
    sim.step();
    let news = sim.take_news();
    let fell = news
        .iter()
        .find_map(|m| match m {
            ToClient::PropsBroken { props, live: true } => {
                props.iter().find(|p| p.0 == i).map(|p| p.1)
            }
            _ => None,
        })
        .expect("the lamp did not break");
    assert!(news
        .iter()
        .any(|m| matches!(m, ToClient::Impact { broke: true, .. })));
    // It falls east, away from the hero.
    let yaw = byte_to_yaw(fell);
    assert!(yaw.sin() > 0.7, "fell towards {:?}", (yaw.sin(), yaw.cos()));
    assert!(sim.damage.collateral[&me] > 0.0);
    // A late joiner gets it.
    let snap = sim.street.snapshot();
    assert!(snap.iter().any(|m| matches!(m,
        ToClient::PropsBroken { props, live: false } if props.contains(&(i, fell)))));
    // A save keeps it, and restores it.
    let save = sim.save();
    assert!(save.props.contains(&(i, fell)));
    let back = hw_sim::save::WorldSave::from_text(&save.to_text()).unwrap();
    assert_eq!(back, save);
    let mut other = Sim::for_town(Seed(1), &t);
    other.restore(&back).unwrap();
    assert_eq!(other.street.broken[&i].fell, fell);
    // Far away for long enough: the lamp stands again.
    let far = [at + DVec3::new(5000.0, 0.0, 0.0)];
    let later = sim.tick + (REPAIR_SECONDS * TICK_HZ as f64) as u64 + 1;
    sim.street.repair_unseen(&far, sim.tick, TICK_HZ);
    sim.street.repair_unseen(&far, later, TICK_HZ);
    assert!(!sim.street.broken.contains_key(&i));
    assert!(sim.take_news().iter().any(|m| matches!(m,
        ToClient::PropsRepaired { props } if props.contains(&i))));
}

#[test]
fn cars_flip_and_statues_need_a_stronger_hero() {
    let t = GeneratedTown::generate(Seed(1), 6000.0);
    let mut street = PropDamage::new(t.world.clone());
    let weak = destruction::hero_power(1);
    let strong = destruction::hero_power(20);
    let car = nearest(&t, |k| matches!(k, PropKind::Car(..)));
    let c = prop(&t, car);
    let side = DVec2::new(c.dir.y, -c.dir.x);
    let fist = DVec3::new(c.pos.x, c.y + 0.9, c.pos.y) + DVec3::new(side.x, 0.0, side.y) * 1.8;
    let (n, cost) = street.hit(
        fist,
        -DVec3::new(side.x, 0.0, side.y),
        weak.punch_radius,
        weak.punch,
        0,
    );
    assert!(n >= 1 && cost > 0.0, "a level 1 punch left the car whole");
    assert!(street.broken.contains_key(&car));

    let Some(statue) = (0..t.area.props.len())
        .find(|&i| matches!(t.area.props[i].kind, PropKind::Statue(_)))
        .map(|i| t.area.prop_ids[i].0)
    else {
        return; // a town without a statue: nothing to check
    };
    let s = prop(&t, statue);
    let at = DVec3::new(s.pos.x, s.y + 1.5, s.pos.y);
    assert!(toughness(s.kind).unwrap().0 > weak.punch);
    street.hit(at, DVec3::X, weak.punch_radius, weak.punch, 0);
    assert!(
        !street.broken.contains_key(&statue),
        "a level 1 punch felled a statue"
    );
    street.hit(at, DVec3::X, strong.punch_radius, strong.punch, 0);
    assert!(
        street.broken.contains_key(&statue),
        "a level 20 punch did not"
    );
}

#[test]
fn a_blast_down_the_street_hits_the_first_lamp_in_the_way() {
    let t = GeneratedTown::generate(Seed(1), 6000.0);
    let i = nearest(&t, |k| matches!(k, PropKind::Lamp(_)));
    let lamp = prop(&t, i);
    let street = PropDamage::new(t.world.clone());
    let from = DVec3::new(lamp.pos.x - 20.0, lamp.y + 1.5, lamp.pos.y);
    let hit = street
        .raycast(from, DVec3::X, 40.0)
        .expect("the ray missed the lamp");
    assert!(hit.x < lamp.pos.x + 0.5 && hit.x > lamp.pos.x - 21.0);
}
