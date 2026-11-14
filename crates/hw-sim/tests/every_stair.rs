//! Every stair down from the street walks down: a hero put on the top
//! landing follows the steps to the foot, stepping down onto each one, so
//! nothing solid is left over the hole (Arnd 2026-11-14: some stairs had an
//! invisible floor over them: the ways up from the tube, which stand far
//! from any station, kept the street's ground over their shafts).

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_gen::urban::spiral::Spiral;
use hw_net::{InputState, PlayerId};
use hw_sim::collision::Body;
use hw_sim::{GeneratedTown, Sim, TICK_HZ};

fn put(sim: &mut Sim, hero: hw_core::NetId, at: DVec3) {
    sim.set_body(
        hero,
        Body {
            pos: at,
            vel: DVec3::ZERO,
            grounded: false,
        },
    );
}

fn run(sim: &mut Sim, me: PlayerId, input: InputState, ticks: u32) {
    sim.set_input(me, input);
    for _ in 0..ticks {
        sim.step();
    }
}

/// Walks the hero down `sp` from its top landing; `Err` says where it
/// stuck or stood above a step.
fn walk_down(sim: &mut Sim, me: PlayerId, hero: hw_core::NetId, sp: &Spiral) -> Result<(), String> {
    let route = sp.route();
    let r0 = route[0];
    put(sim, hero, DVec3::new(r0.x, r0.y + 0.2, r0.z));
    run(sim, me, InputState::default(), TICK_HZ / 3);
    for (k, t) in route.iter().enumerate() {
        let mut tries = 0;
        loop {
            let p = sim.position(hero).ok_or("no hero")?;
            let d = DVec2::new(t.x - p.x, t.z - p.z);
            if d.length() < 0.35 {
                // On the step, not over it.
                if p.y > t.y + 0.45 {
                    return Err(format!(
                        "standing at {:.2} over step {k} (top {:.2}) at {:.1},{:.1}",
                        p.y, t.y, t.x, t.z
                    ));
                }
                break;
            }
            tries += 1;
            if tries > 600 {
                return Err(format!("stuck on the way to step {k} {t:?}: at {p:?}"));
            }
            let dir = d.normalize() * 0.4;
            let input = InputState {
                move_x: dir.x as f32,
                move_z: dir.y as f32,
                ..Default::default()
            };
            run(sim, me, input, 1);
        }
    }
    run(sim, me, InputState::default(), TICK_HZ / 2);
    let p = sim.position(hero).ok_or("no hero")?;
    if (p.y - sp.bottom).abs() > 0.5 {
        return Err(format!(
            "ended at {:.2}, the foot is at {:.2}",
            p.y, sp.bottom
        ));
    }
    Ok(())
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn every_street_stair_walks_down() {
    let town = GeneratedTown::generate(Seed(1), 40_000.0);
    let w = town.world.clone();
    let mut sim = Sim::for_town(Seed(1), &town);
    let me = PlayerId(1);
    let hero = sim.spawn_hero(me, sim.spawn_point(1));
    let h = w.half();
    let holes = w.street_holes(DVec2::splat(-h), DVec2::splat(h));
    let shallow = w.underground().entrances.len();
    assert!(shallow > 0, "no stairs into the shallow underground");
    assert!(
        holes.len() > shallow,
        "no stairs into the deep underground ({} holes)",
        holes.len()
    );
    // Ways up from the tube stand far from stations: the case that broke.
    let net = w.tube();
    assert!(
        holes.iter().any(|x| !net.station_within(x.pos, 140.0)
            && w.lair_centre().is_none_or(|l| l.distance(x.pos) > 200.0)
            && !w.underground().entrances.iter().any(|e| e.pos == x.pos)),
        "no way up from the tube among the holes"
    );
    let mut failed = Vec::new();
    for x in &holes {
        if let Err(e) = walk_down(&mut sim, me, hero, &x.spiral) {
            failed.push(format!("stair at {:.0},{:.0}: {e}", x.pos.x, x.pos.y));
        }
    }
    assert!(
        failed.is_empty(),
        "{} of {} stairs do not walk down:\n{}",
        failed.len(),
        holes.len(),
        failed.join("\n")
    );
}

/// What a hero standing at `at` (feet) bumps its head or sides on: the
/// first solid among a ring of points round the body, shoulders to head.
fn bump(town: &GeneratedTown, at: DVec3) -> Option<DVec3> {
    let o = &town.obstacles;
    for h in [0.5, 0.9, 1.3, 1.7, 1.85] {
        for k in 0..9 {
            let (dx, dz) = if k == 8 {
                (0.0, 0.0)
            } else {
                let a = k as f64 * std::f64::consts::FRAC_PI_4;
                (0.38 * a.cos(), 0.38 * a.sin())
            };
            let p = DVec3::new(at.x + dx, at.y + h, at.z + dz);
            if o.solid_at(p) {
                return Some(p);
            }
        }
    }
    None
}

#[test]
#[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
fn every_stair_has_room_for_a_hero_on_every_step() {
    let town = GeneratedTown::generate(Seed(1), 40_000.0);
    let w = town.world.clone();
    let h = w.half();
    let mut stairs: Vec<(String, Spiral)> = w
        .street_holes(DVec2::splat(-h), DVec2::splat(h))
        .iter()
        .map(|x| (format!("street {:.0},{:.0}", x.pos.x, x.pos.y), x.spiral))
        .collect();
    let u = w.underground().clone();
    if let Some(spot) = hw_gen::urban::molehaven::deep_lift_spot(&u) {
        for t in w.deep_near(spot, 10.0) {
            for l in &t.lifts {
                if l.pos.distance(spot) < 0.5 {
                    stairs.push(("molehaven".into(), l.spiral));
                }
            }
        }
    }
    assert!(stairs.iter().any(|(n, _)| n == "molehaven"));
    let mut failed = Vec::new();
    for (name, sp) in &stairs {
        for (k, p) in sp.route().iter().enumerate() {
            if let Some(hit) = bump(&town, *p) {
                failed.push(format!(
                    "{name}: step {k} (feet {:.2}, top {:.2}, foot {:.2}): solid {:.2} up, at local ({:.2}, {:.2}) of the shaft; feet at local ({:.2}, {:.2})",
                    p.y,
                    sp.top,
                    sp.bottom,
                    hit.y - p.y,
                    (DVec2::new(hit.x, hit.z) - sp.c).dot(DVec2::new(sp.dir.y, -sp.dir.x)),
                    (DVec2::new(hit.x, hit.z) - sp.c).dot(sp.dir),
                    (DVec2::new(p.x, p.z) - sp.c).dot(DVec2::new(sp.dir.y, -sp.dir.x)),
                    (DVec2::new(p.x, p.z) - sp.c).dot(sp.dir)
                ));
                break;
            }
        }
    }
    assert!(
        failed.is_empty(),
        "{} of {} stairs bump a hero:\n{}",
        failed.len(),
        stairs.len(),
        failed
            .iter()
            .take(30)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// A hero climbs ledges up to `STEP_UP` when it walks into them, so no
/// rail or parapet may be that low: it would be a step up onto the
/// balustrade and out over the shaft's edge (found at a run: heroes
/// stepped onto the rail, rose on the next risers and ended outside the
/// shaft, in the rock).
#[test]
fn rails_are_too_high_to_climb() {
    use hw_gen::urban::spiral::{PARAPET_HEIGHT, RAIL_HEIGHT};
    use hw_sim::collision::STEP_UP;
    const { assert!(RAIL_HEIGHT > STEP_UP + 0.05, "balustrade too low") };
    const { assert!(PARAPET_HEIGHT > STEP_UP + 0.05, "parapet too low") };
}
