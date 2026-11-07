//! A hero takes off from the street, flies up to the roof of the tallest
//! tower, stops flying above it and lands there.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_net::InputState;
use hw_sim::collision::Body;
use hw_sim::{step_hero, GeneratedCity, BUILDING_BASE, FLY_CEILING, FLY_SPEED, TICK_HZ};

fn tallest(city: &GeneratedCity) -> usize {
    (0..city.buildings.len())
        .max_by(|&a, &b| {
            let h = |i: usize| city.buildings[i].floors.last().map_or(0.0, |f| f.elevation);
            h(a).total_cmp(&h(b))
        })
        .unwrap()
}

#[test]
fn take_off_cross_the_city_and_land_on_a_roof() {
    let city = GeneratedCity::generate(Seed(1));
    let solids = &city.obstacles;
    let (start, _) = city.spawn_line();
    let mut body = Body {
        pos: DVec3::new(start.x, solids.floor_under(start, 1e3), start.y),
        ..Default::default()
    };
    for _ in 0..5 {
        step_hero(&mut body, InputState::default(), solids);
    }
    assert!(body.grounded);

    let tower = &city.buildings[tallest(&city)];
    let top = tower.floors.last().unwrap();
    let roof_y = BUILDING_BASE + top.elevation + top.height;
    let c = top.outline.center();
    let target = tower.site.to_city(DVec3::new(c.x, 0.0, c.y));
    let target = DVec2::new(target.x, target.z);

    // Climb straight up well above the tower, then cruise over to it.
    let mut t = 0;
    while body.pos.y < roof_y + 20.0 {
        step_hero(
            &mut body,
            InputState {
                fly: true,
                jump: true,
                boost: true,
                ..Default::default()
            },
            solids,
        );
        t += 1;
        assert!(t < TICK_HZ * 60, "never got above the roof: {}", body.pos);
    }
    loop {
        let d = target - DVec2::new(body.pos.x, body.pos.z);
        if d.length() < 1.0 && body.vel.length() < 0.5 {
            break;
        }
        // Steer, braking within reach.
        let dir = d / d.length().max(4.0);
        step_hero(
            &mut body,
            InputState {
                fly: true,
                move_x: dir.x as f32,
                move_z: dir.y as f32,
                ..Default::default()
            },
            solids,
        );
        assert!(body.vel.length() <= FLY_SPEED + 1e-6 || t < TICK_HZ * 60);
        t += 1;
        assert!(t < TICK_HZ * 240, "never reached the tower: {}", body.pos);
    }
    // Stop flying: fall onto the roof and stand there.
    for _ in 0..TICK_HZ * 5 {
        step_hero(&mut body, InputState::default(), solids);
    }
    assert!(body.grounded, "not standing: {}", body.pos);
    assert!(
        (body.pos.y - roof_y).abs() < 0.05,
        "at {} not on roof {roof_y}",
        body.pos.y
    );
}

#[test]
fn hovering_holds_still_and_the_sky_has_a_lid() {
    let city = GeneratedCity::generate(Seed(1));
    let solids = &city.obstacles;
    let mut body = Body {
        pos: DVec3::new(0.0, 300.0, 0.0),
        ..Default::default()
    };
    let hover = InputState {
        fly: true,
        ..Default::default()
    };
    for _ in 0..TICK_HZ * 3 {
        step_hero(&mut body, hover, solids);
    }
    assert!((body.pos.y - 300.0).abs() < 1e-9 && body.vel.length() < 1e-9);
    for _ in 0..TICK_HZ * 60 {
        step_hero(
            &mut body,
            InputState {
                fly: true,
                jump: true,
                boost: true,
                ..Default::default()
            },
            solids,
        );
    }
    assert!(body.pos.y <= FLY_CEILING);
}
