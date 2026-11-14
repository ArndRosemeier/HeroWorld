//! Milestone 8: a breakable city. Buildings stand as Engine's breakable
//! plans, heroes punch through them by level, collapses come down the same
//! on every machine, a late joiner sees the same damage, and buildings
//! nobody sees are repaired.

use engine::building::{Building, ElementBits};
use engine::place::Place;
use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_net::{InputState, PlayerId, ToClient};
use hw_sim::collision::Body;
use hw_sim::destruction::{self, Damage, Frame};
use hw_sim::{GeneratedTown, Sim, TICK_HZ};
use std::sync::Arc;

fn town() -> GeneratedTown {
    GeneratedTown::generate(Seed(1), 6000.0)
}

/// Every building near the start, by id.
fn ids(t: &GeneratedTown) -> Vec<u64> {
    t.area.building_ids.iter().map(|d| d.0).collect()
}

/// Building `i`'s centre at its base (world).
fn centre(t: &GeneratedTown, i: u64) -> (DVec3, Arc<hw_gen::building::BuildingPlan>, Frame) {
    let (plan, base) = t.obstacles.building(i).expect("a building");
    let s = &plan.site;
    let c = s.origin + s.along * s.size.x * 0.5 + s.inward * s.size.y * 0.5;
    let frame = Frame::of(s, base);
    (DVec3::new(c.x, base, c.y), plan, frame)
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn every_building_stands_whole_and_engine_places_it_where_the_town_does() {
    let t = town();
    let all = ids(&t);
    let n = all.len();
    let mut kinds = std::collections::BTreeSet::new();
    let mut most = 0;
    let start = std::time::Instant::now();
    for &i in &all {
        let (_, plan, frame) = centre(&t, i);
        let pieces = destruction::pieces(&plan);
        most = most.max(pieces.len());
        let bp = Arc::new(destruction::breakable(&plan, &pieces, &frame, |_, _, _| None).unwrap());
        assert_eq!(bp.len(), pieces.len());
        // Nothing broken: nothing falls (every slab is held up).
        let mut b = Building::new(bp.clone());
        let change = b.apply(&ElementBits::new(bp.len()));
        assert!(
            change.dropped.is_empty() && change.collapse.is_none(),
            "building {i} ({:?}, {} floors) falls apart untouched: {} pieces drop",
            plan.site.usage,
            plan.floors.len(),
            change.dropped.len()
        );
        for p in &pieces {
            kinds.insert(format!("{:?}", p.kind));
        }
        // Engine's placement (a turn about +y) puts every element where the
        // town's building space says, mirrored sites included.
        let m = Place::new(
            frame.origin.x as f32,
            frame.base as f32,
            frame.origin.y as f32,
        )
        .with_yaw_deg(frame.yaw().to_degrees() as f32)
        .to_matrix();
        for p in pieces.iter().step_by(17) {
            let c = (p.min + p.max) * 0.5;
            let want = frame.to_world(c);
            let got = m.transform_point3(frame.to_engine(c));
            assert!(
                (got.as_dvec3() - want).length() < 0.02,
                "building {i}: {got} vs {want}"
            );
            assert!((frame.to_building(want) - c).length() < 1e-6);
        }
    }
    for k in ["Wall", "Slab", "Roof"] {
        assert!(kinds.contains(k), "no {k} anywhere: {kinds:?}");
    }
    println!(
        "{n} buildings in {:.2} s, the biggest {most} elements",
        start.elapsed().as_secs_f64()
    );
}

/// A small brick-or-weaker building near the town's start.
fn small_building(t: &GeneratedTown) -> u64 {
    let (spawn, _) = t.world.spawn_line();
    ids(t)
        .into_iter()
        .filter(|&i| {
            let (_, plan, _) = centre(t, i);
            // On level ground, so a hero in front stands at its floor.
            let b = t
                .world
                .building(hw_gen::urban::quarter::DetailId(i))
                .expect("a building");
            // (A house of the old narrow kind: houses of halls break their own
            // way, see buildings_of_halls_break_along_what_stands_in_them.)
            plan.layout.is_none()
                && plan.floors.len() <= 3
                && destruction::material_factor(&plan) <= 1.1
                && b.base - b.low < 0.5
        })
        .min_by(|&a, &b| {
            let d = |i| DVec2::new(centre(t, i).0.x, centre(t, i).0.z).distance(spawn);
            d(a).total_cmp(&d(b))
        })
        .expect("a small house")
}

#[test]
fn a_level_one_hero_brings_down_a_small_building_and_a_late_joiner_sees_the_same() {
    let t = town();
    let i = small_building(&t);
    let (_, plan, frame) = centre(&t, i);
    let mut sim = Sim::for_town(Seed(1), &t);
    let solids = sim.solids();
    let power = destruction::hero_power(1);
    // Punch along the ground floor's front, then the back.
    let ground = &plan.floors[0];
    let mut collapsed = false;
    for z in [0.0, plan.site.size.y] {
        let mut x = 0.4;
        while x < plan.site.size.x && !collapsed {
            let p = frame.to_world(DVec3::new(x, ground.elevation + 1.6, z));
            let r = sim.damage.hit(
                &solids,
                p,
                power.punch_radius,
                power.punch,
                Some(PlayerId(1)),
                1,
            );
            collapsed |= r.collapsed.contains(&i);
            x += 1.0;
        }
    }
    assert!(collapsed, "building {i} still stands");
    assert!(sim.damage.collateral[&PlayerId(1)] > 0.0);
    // A player joining now gets the broken bits and sees the same building:
    // the same fallen elements and rubble.
    let news = sim.damage.snapshot();
    assert!(news
        .iter()
        .all(|m| matches!(m, ToClient::Damage { live: false, .. })));
    let other = town();
    let mut late = Damage::default();
    for m in news {
        if let ToClient::Damage {
            building, broken, ..
        } = m
        {
            late.apply(&other.obstacles, building, &broken, 0);
        }
    }
    let a = &sim.damage.buildings[&i].state;
    let b = &late.buildings[&i].state;
    assert_eq!(a.fallen(), b.fallen());
    assert_eq!(a.mounds(), b.mounds());
    assert!(!a.mounds().is_empty());
    // Collision follows: the upper floors are gone, so the lift is dead and
    // the air where the first floor stood is open.
    if plan.floors.len() > 1 {
        let f = &plan.floors[1];
        let mid = frame.to_world(DVec3::new(
            plan.site.size.x * 0.5,
            f.elevation + 2.0,
            plan.site.size.y * 0.5,
        ));
        assert!(!solids.solid_at(mid));
        assert!(!other.obstacles.solid_at(mid));
    }
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn strong_buildings_need_a_stronger_hero() {
    let t = town();
    // The building with the strongest material.
    let i = ids(&t)
        .into_iter()
        .max_by(|&a, &b| {
            let m = |i| destruction::material_factor(&centre(&t, i).1);
            m(a).total_cmp(&m(b)).then(b.cmp(&a))
        })
        .unwrap();
    let (_, plan, frame) = centre(&t, i);
    assert!(destruction::material_factor(&plan) >= 2.0);
    // The front wall above the ground floor (its middle at 2 m can be the
    // entrance's doorway).
    let wall = frame.to_world(DVec3::new(plan.site.size.x * 0.5, 4.5, 0.0));
    let mut damage = Damage::default();
    let weak = destruction::hero_power(1);
    let r = damage.hit(&t.obstacles, wall, weak.punch_radius, weak.punch, None, 0);
    assert_eq!(r.broken, 0, "a level 1 punch broke a strong wall");
    assert!(!t.obstacles.is_damaged(i));
    let strong = destruction::hero_power(20);
    let r = damage.hit(
        &t.obstacles,
        wall,
        strong.punch_radius,
        strong.punch,
        None,
        0,
    );
    assert!(r.broken > 0, "a level 20 punch did nothing");
    assert!(t.obstacles.is_damaged(i));
    damage.repair(&t.obstacles, i);
    assert!(!t.obstacles.is_damaged(i));
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn heroes_punch_through_walls_and_unseen_buildings_repair_themselves() {
    let t = town();
    let i = small_building(&t);
    let (_, plan, frame) = centre(&t, i);
    let mut sim = Sim::for_town(Seed(1), &t);
    let me = PlayerId(1);
    let hero = sim.spawn_hero(me, DVec3::ZERO);
    // Stand in front of the front wall's first bay at the end away from
    // the street door (a narrow shop building has its door near one
    // end), facing it.
    let door = plan.floors[0]
        .doors
        .iter()
        .filter(|d| d.b.is_none() && d.along_x && d.at.y < 0.01)
        .map(|d| d.at.x)
        .next()
        .unwrap_or(plan.site.size.x * 0.5);
    let x = if door > plan.site.size.x * 0.5 {
        1.7
    } else {
        plan.site.size.x - 1.7
    };
    let at = frame.to_world(DVec3::new(x, 0.0, -0.9));
    let into = frame.to_world(DVec3::new(x, 0.0, 1.0)) - at;
    let aim = DVec3::new(into.x, 0.0, into.z).normalize();
    sim.set_body(
        hero,
        Body {
            pos: at,
            ..Default::default()
        },
    );
    let punch = InputState {
        smash: true,
        aim: aim.as_vec3().to_array(),
        ..Default::default()
    };
    let mut impacts = 0;
    for _ in 0..TICK_HZ * 2 {
        sim.set_input(me, punch);
        sim.step();
        for m in sim.take_news() {
            if matches!(m, ToClient::Impact { .. }) {
                impacts += 1;
            }
        }
    }
    // The first punch breaks the panel; the rest go through the hole.
    assert!(impacts >= 1, "no punch landed");
    assert!(sim.solids().is_damaged(i), "the wall held");
    // Walk through the hole.
    let walk = InputState {
        move_x: aim.x as f32,
        move_z: aim.z as f32,
        ..Default::default()
    };
    for _ in 0..TICK_HZ {
        sim.set_input(me, walk);
        sim.step();
    }
    let p = sim.position(hero).unwrap();
    let inside = frame.to_building(p);
    assert!(inside.z > 0.5, "still outside: {inside}");
    // A save keeps the damage.
    let save = sim.save();
    assert!(!save.damage.is_empty());
    let text = save.to_text();
    assert_eq!(hw_sim::save::WorldSave::from_text(&text).unwrap(), save);
    // Off far away for longer than the repair time: whole again.
    sim.set_body(
        hero,
        Body {
            pos: at + DVec3::new(5000.0, 0.0, 0.0),
            ..Default::default()
        },
    );
    sim.set_input(me, InputState::default());
    let ticks = (destruction::REPAIR_SECONDS * TICK_HZ as f64) as u64 + TICK_HZ as u64 * 2;
    let solids = sim.solids();
    let heroes = [at + DVec3::new(5000.0, 0.0, 0.0)];
    let now = sim.tick + ticks;
    sim.damage.repair_unseen(&solids, &heroes, now, TICK_HZ);
    assert!(!sim.solids().is_damaged(i), "not repaired");
    assert!(sim
        .take_news()
        .iter()
        .any(|m| matches!(m, ToClient::Repaired { building } if *building == i)));
    // Restoring the save brings the damage back.
    sim.restore(&save).unwrap();
    assert!(sim.solids().is_damaged(i));
}

/// A site for an office building of halls, `k` giving its size and seed.
fn halls_site(k: u64) -> hw_gen::building::BuildingSite {
    let (w, d) = (32.0 + (k % 7) as f64 * 3.5, 38.0 + (k % 5) as f64 * 3.0);
    hw_gen::building::BuildingSite {
        id: hw_core::StableId(k),
        seed: Seed(k),
        usage: hw_gen::building::BuildingUse::Offices,
        origin: DVec2::new(10.0, 20.0),
        along: DVec2::X,
        inward: if k.is_multiple_of(2) {
            DVec2::Y
        } else {
            -DVec2::Y
        },
        size: DVec2::new(w, d),
        floors: 3,
        street: 0,
        district: hw_gen::city::DistrictKind::Commercial,
        halls: None,
        vernacular: None,
    }
    .into_halls(2 + (k % 9) as u32)
}

#[test]
fn buildings_of_halls_break_along_what_stands_in_them() {
    use engine::building::ElementKind;
    use hw_gen::layout::solid::Part;
    for k in 0..10u64 {
        let site = halls_site(k);
        let plan = hw_gen::building::BuildingPlan::generate(&site);
        let parts = plan.solids().expect("a building of halls");
        let frame = Frame::of(&plan.site, 0.0);
        let pieces = destruction::pieces(&plan);
        let bp = Arc::new(destruction::breakable(&plan, &pieces, &frame, |_, _, _| None).unwrap());
        // Untouched, it stands.
        let mut b = Building::new(bp.clone());
        let change = b.apply(&ElementBits::new(bp.len()));
        assert!(
            change.dropped.is_empty() && change.collapse.is_none(),
            "{k}: {} pieces drop from an untouched building",
            change.dropped.len()
        );
        // Every solid part is a piece or several (but the lifts' doors),
        // and nothing breaks that is not drawn: no columns of its own.
        let cut = destruction::halls_cut(&plan, &parts);
        assert_eq!(cut.len(), pieces.len());
        for p in parts.iter().filter(|p| p.part != Part::LiftDoor) {
            let at = p.shape.at();
            let (y0, y1) = p.shape.span();
            let c = DVec3::new(at.x, (y0 + y1) * 0.5, at.y);
            assert!(
                pieces
                    .iter()
                    .any(|q| (c - c.clamp(q.min, q.max)).length() < 1e-6),
                "{k}: no piece where a {:?} stands at {c:?}",
                p.part
            );
        }
        let count = |part: Part| parts.iter().filter(|p| p.part == part).count();
        let columns = pieces
            .iter()
            .filter(|p| p.kind == ElementKind::Column)
            .count();
        assert_eq!(columns, count(Part::Column), "{k}");
        let steps = cut.iter().filter(|c| c.0.part == Part::Step).count();
        assert_eq!(steps, count(Part::Step), "{k}: a step is one piece");
        assert!(steps > 0, "{k}: stairs");
        // No piece is longer than a wall panel or a floor tile and a half.
        for (part, p) in &cut {
            let s = p.max - p.min;
            if matches!(
                part.part,
                Part::Outer | Part::Wall | Part::Floor(_) | Part::Roof
            ) {
                assert!(
                    s.x.max(s.z) <= 4.5 + 1e-6,
                    "{k}: a {:?} of {s:?}",
                    part.part
                );
            }
        }
        // A level 1 hero's punch at a window breaks it and no more than
        // what is next to it.
        let glass = pieces
            .iter()
            .position(|p| p.kind == ElementKind::Window && p.floor == 0)
            .expect("a window");
        let at = (pieces[glass].min + pieces[glass].max) * 0.5;
        let power = destruction::hero_power(1);
        let change = b.hit(frame.to_engine(at), power.punch_radius, power.punch);
        assert!(change.broken.iter().any(|e| e.0 as usize == glass), "{k}");
        assert!(change.collapse.is_none() && change.broken.len() < 12, "{k}");
        // The ground floor's outside walls gone: the building comes down.
        let ring: Vec<_> = (0..pieces.len())
            .filter(|&i| pieces[i].floor == 0 && pieces[i].outward != DVec2::ZERO)
            .map(|i| engine::building::ElementId(i as u32))
            .collect();
        let change = b.break_elements(&ring);
        assert!(change.collapse.is_some(), "{k}: it stands on nothing");
    }
}

#[test]
fn furniture_falls_with_the_floor_it_stands_on_and_comes_back_with_it() {
    use engine::building::{ElementId, ElementKind};
    for k in 0..6u64 {
        let site = halls_site(k);
        let plan = hw_gen::building::BuildingPlan::generate(&site);
        let items = plan.furniture();
        assert!(!items.is_empty(), "{k}: furniture");
        let frame = Frame::of(&plan.site, 0.0);
        let pieces = destruction::pieces(&plan);
        let bp = Arc::new(destruction::breakable(&plan, &pieces, &frame, |_, _, _| None).unwrap());
        let mut b = Building::new(bp.clone());
        // Whole: everything stands.
        let lost = destruction::furniture_lost(&items, bp.elements(), &b.gone(), &frame, None);
        assert!(
            lost.iter().all(|l| !l),
            "{k}: furniture lost from a whole building"
        );
        // Every slab above the ground floor broken: what stood on one is
        // lost, what stood on the ground is not.
        let slabs: Vec<ElementId> = (0..bp.len())
            .filter(|&i| bp.elements()[i].kind == ElementKind::Slab && pieces[i].min.y > 0.5)
            .map(|i| ElementId(i as u32))
            .collect();
        assert!(!slabs.is_empty(), "{k}: upper floors");
        b.break_elements(&slabs);
        let lost =
            destruction::furniture_lost(&items, bp.elements(), &b.gone(), &frame, b.collapse());
        let upper = items.iter().filter(|it| it.y > 0.5).count();
        let n = lost.iter().filter(|&&l| l).count();
        assert!(upper > 0 && n > 0, "{k}: {n} lost of {upper} upstairs");
        for (it, l) in items.iter().zip(&lost) {
            if it.y < 0.5 {
                assert!(!l, "{k}: ground furniture lost");
            }
        }
        // Repaired (nothing broken): all back.
        let b = Building::new(bp.clone());
        let lost = destruction::furniture_lost(&items, bp.elements(), &b.gone(), &frame, None);
        assert!(lost.iter().all(|l| !l), "{k}: not back after repair");
    }
}
