//! A hero walks in from the street, takes the lift and steps out on an
//! upper floor, using only the movement code and the generated city.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_gen::building::{BuildingPlan, BuildingUse, RoomKind};
use hw_net::InputState;
use hw_sim::collision::{Body, Inside};
use hw_sim::{step_hero, GeneratedCity, BUILDING_BASE, TICK_HZ};

struct Walk<'a> {
    city: &'a GeneratedCity,
    plan: &'a BuildingPlan,
    body: Body,
}

impl Walk<'_> {
    fn world(&self, x: f64, z: f64) -> DVec2 {
        let p = self.plan.site.to_city(DVec3::new(x, 0.0, z));
        DVec2::new(p.x, p.z)
    }

    fn tick(&mut self, input: InputState) {
        step_hero(&mut self.body, input, &self.city.obstacles);
    }

    /// Opens the nearest shut door within reach, as E does.
    fn open_door(&mut self, what: &str) {
        let d = self
            .city
            .obstacles
            .doors_near(self.body.pos, hw_sim::doors::REACH)
            .into_iter()
            .find(|d| !d.open)
            .unwrap_or_else(|| panic!("no shut door at {what} ({})", self.body.pos));
        assert!(self.city.obstacles.set_door(d.building, d.key, true));
    }

    /// Runs towards a point (building coordinates) for `ticks`; where it
    /// got to.
    fn run_at(&mut self, x: f64, z: f64, ticks: u32) -> DVec3 {
        let target = self.world(x, z);
        for _ in 0..ticks {
            let here = DVec2::new(self.body.pos.x, self.body.pos.z);
            let d = target - here;
            let dir = d / d.length().max(1.0);
            self.tick(InputState {
                move_x: dir.x as f32,
                move_z: dir.y as f32,
                ..Default::default()
            });
        }
        self.body.pos
    }

    /// Runs towards a point (building coordinates) until within 0.35 m.
    fn go(&mut self, x: f64, z: f64, what: &str) {
        let target = self.world(x, z);
        for _ in 0..TICK_HZ * 20 {
            let here = DVec2::new(self.body.pos.x, self.body.pos.z);
            let d = target - here;
            if d.length() < 0.35 {
                // Stop.
                for _ in 0..10 {
                    self.tick(InputState::default());
                }
                return;
            }
            let dir = d / d.length().max(1.0);
            self.tick(InputState {
                move_x: dir.x as f32,
                move_z: dir.y as f32,
                ..Default::default()
            });
        }
        panic!(
            "stuck on the way to {what}: at {}, target {target}",
            self.body.pos
        );
    }
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn walk_in_ride_the_lift_and_step_out_upstairs() {
    let city = GeneratedCity::generate(Seed(1));
    let (index, plan) = city
        .buildings
        .iter()
        .enumerate()
        .find(|(_, p)| {
            p.site.usage == BuildingUse::Offices && p.floors.len() >= 4 && p.core.is_some()
        })
        .expect("an office tower");
    let core = plan.core.unwrap();
    let ground = &plan.floors[0];
    let lobby = ground
        .rooms
        .iter()
        .position(|r| r.kind == RoomKind::Lobby)
        .expect("lobby");
    let entrance = ground
        .doors
        .iter()
        .find(|d| d.a == lobby && d.b.is_none())
        .expect("entrance");
    let to_corridor = ground
        .doors
        .iter()
        .find(|d| d.a == lobby && d.b.is_some())
        .expect("lobby door");
    let corridor_z = to_corridor.at.y + 1.6;

    let mut w = Walk {
        city: &city,
        plan,
        body: Body::default(),
    };
    let start = w.world(entrance.at.x, -2.0);
    w.body.pos = DVec3::new(start.x, 0.5, start.y);
    for _ in 0..10 {
        w.tick(InputState::default());
    }

    // The entrance's glass doors are shut: running at them, the hero
    // stays outside. Opened (E), the way is free.
    let stopped = w.run_at(entrance.at.x, 1.5, TICK_HZ * 3);
    assert!(
        city.obstacles.inside(stopped).is_none(),
        "ran through the shut entrance to {stopped}"
    );
    w.open_door("the entrance");
    w.go(entrance.at.x, 1.5, "the lobby");
    assert_eq!(
        city.obstacles.inside(w.body.pos),
        Some(Inside {
            building: hw_sim::collision::FIXED | index as u64,
            level: 0
        })
    );
    w.go(
        to_corridor.at.x,
        to_corridor.at.y - 0.8,
        "the lobby's inner door",
    );
    w.open_door("the lobby's inner door");
    w.go(to_corridor.at.x, corridor_z, "the corridor");
    w.go(core.center().x, corridor_z, "the corridor by the lift");
    w.go(core.center().x, core.center().y, "the lift");
    assert!(
        city.obstacles.in_lift(w.body.pos),
        "not in the lift at {}",
        w.body.pos
    );

    // Ride up to the third floor (level 2) and hover there.
    let level2 = BUILDING_BASE + plan.floors[2].elevation;
    for _ in 0..TICK_HZ * 10 {
        if w.body.pos.y >= level2 + 0.2 {
            break;
        }
        w.tick(InputState {
            jump: true,
            ..Default::default()
        });
    }
    assert!(
        w.body.pos.y >= level2 + 0.2,
        "lift stuck at {}",
        w.body.pos.y
    );
    w.tick(InputState::default());

    // Step out into the corridor upstairs.
    w.go(core.center().x, corridor_z, "the upstairs corridor");
    for _ in 0..TICK_HZ {
        w.tick(InputState::default());
    }
    assert!(w.body.grounded);
    assert!(
        (w.body.pos.y - level2).abs() < 1e-6,
        "standing at {} not {level2}",
        w.body.pos.y
    );
    assert_eq!(
        city.obstacles.inside(w.body.pos),
        Some(Inside {
            building: hw_sim::collision::FIXED | index as u64,
            level: 2
        })
    );
}

#[test]
fn windows_and_walls_keep_you_out() {
    let city = GeneratedCity::generate(Seed(1));
    let plan = city
        .buildings
        .iter()
        .find(|p| p.site.usage == BuildingUse::Offices && p.floors.len() >= 2)
        .unwrap();
    // Run at the middle of the left wall from outside: no door there.
    let s = &plan.site;
    let mid = s.size.y * 0.5;
    let outside = s.to_city(DVec3::new(-2.0, 0.5, mid));
    let mut body = Body {
        pos: outside,
        ..Default::default()
    };
    let dir = s.along;
    for _ in 0..TICK_HZ * 3 {
        step_hero(
            &mut body,
            InputState {
                move_x: dir.x as f32,
                move_z: dir.y as f32,
                ..Default::default()
            },
            &city.obstacles,
        );
    }
    assert!(
        city.obstacles.inside(body.pos).is_none(),
        "walked through a wall to {}",
        body.pos
    );
}
