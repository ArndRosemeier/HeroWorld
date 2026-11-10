//! Feet stand on what is drawn. A hero and a townsperson walk, the way the
//! server moves them, across a street and its kerbs, a plaza's stairs, a
//! lawn, a slope, a building's ground floor and a utility tunnel, in a
//! headless Engine world holding what the game draws there (town cells,
//! props, landmarks, the underground, the streamed terrain, grass). At
//! every frame no sole may be more than [`SINK`] into the drawn ground,
//! and standing still the lower sole is within [`FLOAT`] of it.
//!
//! Slow (a part of the town is built and streamed in, about a minute in a
//! debug build): a slow test, so the quick check leaves it out and
//! `scripts/check.sh --full` and CI run it on every push. Skipped
//! (with a note) without the Actors packs (`scripts/fetch-packs.sh`).

use engine::anim::FootIk;
use engine::prelude::*;
use glam::{DVec2, DVec3, Vec3};
use hw_client::grassview::GrassView;
use hw_client::landmarkview::LandmarkView;
use hw_client::people::{self, Who};
use hw_client::propview::PropView;
use hw_client::townmesh::TownView;
use hw_client::undergroundview::UndergroundView;
use hw_core::Seed;
use hw_gen::urban::greenery::Greenery;
use hw_gen::urban::underground::VoidKind;
use hw_net::InputState;
use hw_sim::collision::{Body, Inside, Obstacles};
use hw_sim::{step_moving, GeneratedTown, Moves, DT};
use std::sync::Arc;

/// Most a sole may sink into what is drawn (m).
const SINK: f32 = 0.02;
/// Most the lower sole may hang above it while standing (m).
const FLOAT: f32 = 0.02;
/// Most the server's feet may be off what is drawn (m): foot IK makes up
/// the rest, and needs room to spare.
const ORIGIN_OFF: f32 = 0.25;
/// Frames to settle after a character appears somewhere new.
const SETTLE: usize = 20;

/// One walk: where it starts (feet on the ground there), which way, how
/// far, and the building and floor it is inside, if any.
struct Route {
    name: String,
    start: DVec2,
    /// Height to stand at or below at the start (an upper floor, a tunnel).
    below: Option<f64>,
    dir: DVec2,
    metres: f64,
    inside: Option<Inside>,
}

#[derive(Default)]
struct Worst {
    sink: f32,
    /// Where the deepest sole was.
    at: Vec3,
    float: f32,
    bare: usize,
    frames: usize,
    /// Furthest the server's feet were from the drawn ground (what foot
    /// IK makes up for; it reaches [`FootIk::max_step`]).
    origin: f32,
    /// The server's feet and the drawn ground there, at the worst.
    origin_at: (Vec3, f32),
}

fn routes(town: &GeneratedTown) -> Vec<Route> {
    let w = &town.world;
    let solids = &town.obstacles;
    let (spawn, _) = w.spawn_line();
    let mut out = Vec::new();

    // Across a street with traffic: carriageway, kerb, pavement.
    let m = DVec2::splat(150.0);
    let roads = w.roads_in(spawn - m, spawn + m);
    if let Some(r) = roads.iter().find(|r| r.has_traffic() && r.pts.len() >= 2) {
        let (a, b) = (r.pts[0], r.pts[1]);
        let mid = a.lerp(b, 0.5);
        let across = (b - a).normalize().perp();
        out.push(Route {
            name: "street, kerb and pavement".into(),
            start: mid,
            below: None,
            dir: across,
            metres: r.half + r.pavement + 1.5,
            inside: None,
        });
        out.push(Route {
            name: "along the street".into(),
            start: mid - (b - a).normalize() * 4.0,
            below: None,
            dir: (b - a).normalize(),
            metres: 8.0,
            inside: None,
        });
    }

    // Up a landmark's steps onto its plaza.
    if let Some((l, s)) = w
        .landmarks
        .iter()
        .flat_map(|l| l.stairs.iter().map(move |s| (l, s)))
        .min_by(|a, b| a.0.pos.distance(spawn).total_cmp(&b.0.pos.distance(spawn)))
    {
        let flight = s.steps as f64 * s.run;
        out.push(Route {
            name: format!("up the {:?}'s stairs", l.kind),
            start: s.top + s.down * (flight + 2.0),
            below: None,
            dir: -s.down,
            metres: flight + 4.0,
            inside: None,
        });
    }

    // A lawn, and the steepest open slope near the start.
    let green = Greenery::new(w);
    let mut lawn = None;
    let mut slope: Option<(f64, DVec2, DVec2)> = None;
    for i in -40..=40 {
        for j in -40..=40 {
            let p = spawn + DVec2::new(i as f64, j as f64) * 8.0;
            if green.on_road(w, p, 4.0) || green.in_building(w, p, 4.0) {
                continue;
            }
            if solids.solid_at(DVec3::new(p.x, w.surface(p) + 1.0, p.y)) {
                continue;
            }
            let sward = green.sward_at(w, p);
            if lawn.is_none() && green.grass(w, p, sward).is_some() {
                lawn = Some(p);
            }
            let e = 1.0;
            let g = DVec2::new(
                w.ground(p + DVec2::X * e) - w.ground(p - DVec2::X * e),
                w.ground(p + DVec2::Y * e) - w.ground(p - DVec2::Y * e),
            ) / (2.0 * e);
            let steep = g.length();
            if steep < 0.4 && slope.is_none_or(|s| steep > s.0) && w.ground(p) > 1.5 {
                slope = Some((steep, p, g.normalize_or(DVec2::X)));
            }
        }
    }
    if let Some(p) = lawn {
        out.push(Route {
            name: "a lawn".into(),
            start: p,
            below: None,
            dir: DVec2::new(0.6, 0.8),
            metres: 3.0,
            inside: None,
        });
    }
    if let Some((steep, p, up)) = slope {
        out.push(Route {
            name: format!("up a slope of {:.0}%", steep * 100.0),
            start: p - up * 2.0,
            below: None,
            dir: up,
            metres: 4.0,
            inside: None,
        });
    }

    // A building's ground floor, from the middle.
    let q = w.quarters_in(spawn - m, spawn + m);
    if let Some((id, b)) = q
        .iter()
        .flat_map(|q| q.buildings_with_ids().collect::<Vec<_>>())
        .filter(|(_, b)| b.site.size.min_element() > 10.0)
        .min_by(|a, b| {
            let c = |b: &hw_gen::urban::world::WorldBuilding| {
                b.site.origin
                    + b.site.along * b.site.size.x * 0.5
                    + b.site.inward * b.site.size.y * 0.5
            };
            c(a.1).distance(spawn).total_cmp(&c(b.1).distance(spawn))
        })
    {
        let s = &b.site;
        let mid = s.origin + s.along * s.size.x * 0.5 + s.inward * s.size.y * 0.5;
        out.push(Route {
            name: "a building's ground floor".into(),
            start: mid,
            below: Some(b.base + 1.0),
            dir: s.along,
            metres: 3.0,
            inside: Some(Inside {
                building: id.0,
                level: 0,
            }),
        });
    }

    // Along a utility tunnel.
    if let Some(v) = w
        .underground()
        .voids
        .iter()
        .find(|v| v.kind == VoidKind::Tunnel && v.max.x - v.min.x > 20.0)
    {
        let mid = v.world((v.min + v.max) * 0.5);
        out.push(Route {
            name: "a utility tunnel".into(),
            start: mid,
            below: Some(v.floor_at(mid) + 1.0),
            dir: v.along,
            metres: 6.0,
            inside: None,
        });
    }
    out
}

/// Where the server stands a body put down at `p` (no higher than `below`).
fn stand(solids: &Obstacles, p: DVec2, below: Option<f64>) -> DVec3 {
    let top = below.unwrap_or_else(|| solids.ground_at(p) + 1.0);
    DVec3::new(p.x, solids.floor_under(p, top), p.y)
}

fn v3(p: DVec3) -> Vec3 {
    Vec3::new(p.x as f32, p.y as f32, p.z as f32)
}

/// Each sole's height above what is drawn under it (`None`: nothing drawn).
fn soles_above(world: &mut World, e: EntityId) -> [(Vec3, Option<f32>); 2] {
    let soles = world.sole_points(e).expect("legs");
    soles.map(|s| {
        (
            s,
            world.ground_below(s + Vec3::Y * 0.3, 1.2).map(|g| s.y - g),
        )
    })
}

fn measure(world: &mut World, e: EntityId, worst: &mut Worst, standing: bool) {
    let above = soles_above(world, e);
    worst.frames += 1;
    let mut low = f32::MAX;
    for (at, a) in above {
        match a {
            Some(a) => {
                if -a > worst.sink {
                    worst.sink = -a;
                    worst.at = at;
                }
                low = low.min(a);
            }
            None => worst.bare += 1,
        }
    }
    if standing && low < f32::MAX {
        worst.float = worst.float.max(low);
    }
}

#[test]
#[ignore = "on demand (looks): scripts/check.sh --on-demand"]
fn feet_stand_on_what_is_drawn() {
    let Some(dir) = people::packs_dir() else {
        eprintln!("no Actors packs: skipped (run scripts/fetch-packs.sh)");
        return;
    };
    let mut builder = actors_engine::CharacterBuilder::load(dir).expect("load the packs");
    let town = GeneratedTown::generate(Seed(1), 20_000.0);
    let routes = routes(&town);
    let names: Vec<&str> = routes.iter().map(|r| r.name.as_str()).collect();
    for want in [
        "street",
        "stairs",
        "lawn",
        "slope",
        "ground floor",
        "tunnel",
    ] {
        assert!(
            names.iter().any(|n| n.contains(want)),
            "no route over {want}: {names:?}"
        );
    }
    let people = [
        people::build(&mut builder, &Who::Hero("feet".into()), 0, None).expect("hero"),
        people::build(&mut builder, &Who::Citizen(3), 1, None).expect("townsperson"),
    ];

    // What the game draws, once.
    let mut world = World::new();
    let w = town.world.clone();
    let green = Arc::new(Greenery::new(&w));
    let mut props = PropView::spawn(&mut world, w.clone()).expect("props");
    LandmarkView::spawn(&mut world, &w).expect("landmarks");
    let mut under = UndergroundView::new(w.clone());
    let mut grass = GrassView::spawn(&mut world, w.clone(), green.clone()).expect("grass");
    hw_client::extras::set_town_stream(&mut world, &w, green, &[], &[]).expect("stream");
    let mut cells = TownView::new(w.clone());

    let mut failures = Vec::new();
    let mut report = Vec::new();
    for r in &routes {
        let start = stand(&town.obstacles, r.start, r.below);
        // The town around the walk, as the game would have it.
        world.look_orbit(v3(start), 6.0, 0.0, -20.0);
        let eye = r.start;
        cells.inside = r.inside;
        cells.build_around(&mut world, eye, 40.0);
        props.update(eye, f64::INFINITY);
        props.flush(&mut world).expect("props in");
        grass.update(&mut world, v3(start)).expect("grass");
        under.prime(&mut world, &w, v3(start)).expect("underground");
        assert!(world.prepare_stream(60.0), "terrain streamed in");
        for (k, person) in people.iter().enumerate() {
            let mut worst = Worst::default();
            let mut body = Body {
                pos: start,
                ..Default::default()
            };
            let yaw = r.dir.x.atan2(r.dir.y).to_degrees() as f32;
            let e = world
                .spawn_animated_shared(person.model.clone(), Place::new(0.0, 0.0, 0.0))
                .expect("spawn");
            world.set_animation_blend(e, 0.2).expect("blend");
            let place = |b: &Body| {
                let p = v3(b.pos);
                Place::new(p.x, p.y, p.z).with_yaw_deg(yaw)
            };
            // Standing at the start, walking the route, standing at the end.
            let walk_ticks = (r.metres / (person_speed(k) * DT)).ceil() as usize;
            let stand_ticks = (1.5 / DT) as usize;
            // A hero runs at the game's run speed, a townsperson walks.
            let gait = if k == 0 { "jog" } else { person.walk.as_str() };
            let phases = [
                ("idle", stand_ticks, 0.0),
                (gait, walk_ticks, 1.0),
                ("idle", stand_ticks, 0.0),
            ];
            let mut shown = "";
            let mut since = 0usize;
            world.set_place(e, place(&body)).expect("place");
            for (clip, ticks, go) in phases {
                for t in 0..ticks {
                    let input = InputState {
                        move_x: (r.dir.x * go) as f32 * if k == 1 { 0.5 } else { 1.0 },
                        move_z: (r.dir.y * go) as f32 * if k == 1 { 0.5 } else { 1.0 },
                        ..Default::default()
                    };
                    step_moving(&mut body, input, &town.obstacles, Moves::ON_FOOT);
                    let moving = DVec2::new(body.vel.x, body.vel.z).length() > 0.3;
                    let clip = if moving { clip } else { "idle" };
                    if clip != shown {
                        world.play_animation(e, clip).expect("clip");
                        world.set_foot_ik(e, Some(FootIk::default())).expect("legs");
                        shown = clip;
                    }
                    world.set_place(e, place(&body)).expect("place");
                    world.tick_animations(DT as f32);
                    since += 1;
                    if since > SETTLE && !moving_up_or_down(&body) {
                        let feet = v3(body.pos);
                        if let Some(g) = world.ground_below(feet + Vec3::Y * 0.6, 1.2) {
                            if (feet.y - g).abs() > worst.origin {
                                worst.origin = (feet.y - g).abs();
                                worst.origin_at = (feet, g);
                            }
                        }
                    }
                    if since > SETTLE {
                        // Standing: the last part of a still phase.
                        let still = go == 0.0 && t > ticks / 2;
                        measure(&mut world, e, &mut worst, still);
                    }
                }
            }
            world.despawn(e);
            let who = if k == 0 { "hero" } else { "townsperson" };
            report.push(format!(
                "{who} {}: sinks {:.3} m at most (at {:.2}), floats {:.3} m standing, {} soles over nothing, server feet {:.3} m off ({:.2} over {:.2}), {} frames",
                r.name, worst.sink, worst.at, worst.float, worst.bare, worst.origin, worst.origin_at.0, worst.origin_at.1, worst.frames
            ));
            if worst.sink > SINK
                || worst.float > FLOAT
                || worst.bare > 0
                || worst.origin > ORIGIN_OFF
            {
                failures.push(report.last().cloned().unwrap_or_default());
            }
        }
    }
    eprintln!("{}", report.join("\n"));
    assert!(
        failures.is_empty(),
        "feet off the drawn ground:\n{}",
        failures.join("\n")
    );
}

/// Falling or stepping (between ticks the server's feet are in the air).
fn moving_up_or_down(b: &Body) -> bool {
    !b.grounded || b.vel.y.abs() > 0.1
}

/// How fast each one goes (m/s): a hero's run, a townsperson at half.
fn person_speed(k: usize) -> f64 {
    let run = hw_sim::RUN_SPEED;
    if k == 1 {
        run * 0.5
    } else {
        run
    }
}
