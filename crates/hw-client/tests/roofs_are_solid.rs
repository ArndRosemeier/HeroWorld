//! Every roof is solid: what the game draws on top of a building, a body
//! cannot go through. For buildings of every roof and crown in towns of
//! several sizes, the roof is drawn exactly as the game draws it up close
//! and every upward-facing surface of it (a pitched roof's slopes, a
//! dome, a Googie swoop, the sawtooth, the decks, the crown's steps and the
//! parapets' tops) is probed from just below: the solids a hero bumps into
//! (`hw_sim::collision`) must hold a point there. A roof that loses its
//! collision, or whose collision no longer follows its shape, fails here
//! and names the building's style, roof and crown.

use glam::{DVec2, DVec3};
use hw_client::buildingmesh::{exterior, Detail};
use hw_client::citymesh::{MeshBuilder, BLOCK_TOP};
use hw_core::Seed;
use hw_gen::building::BuildingPlan;
use hw_gen::style::{Cornice, Crown, Ridge, Roof};
use hw_sim::collision::{building_solids, Obstacles};
use hw_sim::GeneratedTown;
use std::collections::BTreeMap;

/// How far under the drawn surface a probe sits (m). Solids are boxes, so
/// a slope is a staircase; it may stand this much under the surface.
const UNDER: f64 = 0.2;
/// The smallest roof surface probed (m²): roofs are big, chimneys, vents
/// and the dormers' tops are decoration that heroes may pass through.
const BIGGEST_DECOR: f64 = 9.0;
/// Probe spacing over a surface (m).
const STEP: f64 = 0.7;

/// One probe that found nothing solid.
#[derive(Clone, Debug)]
struct Gap {
    at: DVec3,
    /// How high the drawn surface is over the building's ground floor.
    up: f64,
}

/// Probes the roof of `plan` standing at `base`; the gaps found, and how
/// many probes were made.
fn probe(o: &Obstacles, plan: &BuildingPlan, base: f64) -> (Vec<Gap>, usize) {
    let mut b = MeshBuilder::new();
    exterior(&mut b, plan, plan.floors.len(), true, Detail::Near);
    let shift = base - BLOCK_TOP;
    let ground = plan.floors[0].elevation;
    let mut gaps = Vec::new();
    let mut probes = 0;
    for part in b.finish_parts() {
        let built = part.build();
        for t in built.indices.as_chunks::<3>().0.iter() {
            let p = |i: u32| built.positions[i as usize].as_dvec3();
            let (a, bb, c) = (p(t[0]), p(t[1]), p(t[2]));
            let n = (bb - a).cross(c - a);
            let area = n.length() * 0.5;
            if area < BIGGEST_DECOR || n.normalize_or_zero().y < 0.2 {
                continue;
            }
            // Only the roof: above the walls' top, not terraces and the
            // floors' own cornices (those the walls' slabs hold).
            let top_of_walls = plan.height();
            if a.y.min(bb.y).min(c.y) - BLOCK_TOP < top_of_walls - 0.5 {
                continue;
            }
            let reach = (bb - a)
                .length()
                .max((c - a).length())
                .max((c - bb).length());
            let k = ((reach / STEP).ceil() as usize).clamp(1, 40);
            for i in 0..=k {
                for j in 0..=(k - i) {
                    let (u, v) = (i as f64 / k as f64, j as f64 / k as f64);
                    // Stay off the edges: the edge of a roof is the wall's.
                    if u < 0.1 || v < 0.1 || 1.0 - u - v < 0.1 {
                        continue;
                    }
                    let q = a * (1.0 - u - v) + bb * u + c * v;
                    probes += 1;
                    let at = DVec3::new(q.x, q.y + shift - UNDER, q.z);
                    // Solids are boxes: a slope is a staircase, a thin
                    // slab (a pad's deck) is a thin box. Held if any of
                    // three probes just under the surface is inside one.
                    let held = [UNDER, UNDER * 0.5, UNDER * 1.5]
                        .iter()
                        .any(|d| o.solid_at(DVec3::new(q.x, q.y + shift - d, q.z)));
                    if !held {
                        gaps.push(Gap {
                            at,
                            up: q.y - BLOCK_TOP - ground,
                        });
                    }
                }
            }
        }
    }
    (gaps, probes)
}

/// The buildings to test: one of each look, so every roof and crown is
/// there, up to `per` of each.
fn picks(town: &GeneratedTown, per: usize) -> Vec<(BuildingPlan, f64, DVec2)> {
    let all = town.world.everything();
    let o = &town.obstacles;
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut out = Vec::new();
    for (_, wb) in &all.buildings {
        let site = &wb.site;
        let mid = site.origin + site.along * site.size.x * 0.5 + site.inward * site.size.y * 0.5;
        let look = site.look();
        let key = format!(
            "{:?} {:?} {:?} {}",
            look.roof, look.crown, look.corner_tower, look.taxi_pad
        );
        let n = seen.entry(key).or_default();
        if *n >= per {
            continue;
        }
        let Some(id) = o
            .buildings_near(mid, 1.0)
            .into_iter()
            .find(|&i| o.building_bounds(i).is_some())
        else {
            continue;
        };
        let Some((plan, base)) = o.building(id) else {
            continue;
        };
        if plan.site.origin != site.origin {
            continue;
        }
        *n += 1;
        out.push((plan.as_ref().clone(), base, mid));
    }
    out
}

/// Collects the gaps of a plan under a name.
fn collect(
    worst: &mut BTreeMap<String, (usize, usize, Gap)>,
    name: String,
    gaps: Vec<Gap>,
    probes: usize,
) {
    if let Some(g) = gaps.first() {
        let e = worst.entry(name).or_insert((0, 0, g.clone()));
        e.0 += gaps.len();
        e.1 += probes;
    }
}

fn report(worst: &BTreeMap<String, (usize, usize, Gap)>) {
    for (k, (g, p, at)) in worst {
        eprintln!(
            "{k}: {g} of {p} probes hold nothing, e.g. {:?} ({:.1} m up)",
            at.at, at.up
        );
    }
    assert!(
        worst.is_empty(),
        "roofs a body can pass through: {worst:#?}"
    );
}

/// The same building with every roof and crown the styles have, and the
/// extras that stand on a roof (corner tower, taxi pad, dormers, ridge
/// directions, parapets, cornices): a roof that gets a new shape without
/// its collision is found here, whether or not a town happens to have one.
fn variants(plan: &BuildingPlan) -> Vec<(String, BuildingPlan)> {
    let mut out = Vec::new();
    let mut add = |name: String, f: &dyn Fn(&mut BuildingPlan)| {
        let mut p = plan.clone();
        f(&mut p);
        out.push((name, p));
    };
    for roof in [
        Roof::Flat,
        Roof::Gable,
        Roof::Hip,
        Roof::Mansard,
        Roof::Dome,
        Roof::Swoop,
        Roof::Sawtooth,
    ] {
        add(format!("{roof:?} roof"), &|p| p.look.roof = roof);
        for ridge in [Ridge::Long, Ridge::Front, Ridge::Deep] {
            add(format!("{roof:?} roof, {ridge:?} ridge, dormers"), &|p| {
                p.look.roof = roof;
                p.look.ridge = ridge;
                p.look.dormers = true;
                p.look.pitch = 1.2;
            });
        }
        for cornice in [Cornice::None, Cornice::Stepped] {
            add(format!("{roof:?} roof, {cornice:?} cornice"), &|p| {
                p.look.roof = roof;
                p.look.cornice = cornice;
            });
        }
    }
    for crown in [
        Crown::None,
        Crown::Stepped,
        Crown::Spire,
        Crown::Mast,
        Crown::Dome,
        Crown::Fins,
    ] {
        add(format!("flat roof, {crown:?} crown"), &|p| {
            p.look.roof = Roof::Flat;
            p.look.crown = crown;
        });
    }
    add("flat roof, taxi pad".into(), &|p| {
        p.look.roof = Roof::Flat;
        p.look.crown = Crown::None;
        p.look.taxi_pad = true;
    });
    add("flat roof, corner tower, parapet, gear".into(), &|p| {
        p.look.roof = Roof::Flat;
        p.look.crown = Crown::None;
        p.look.corner_tower = true;
        p.look.parapet = 1.1;
        p.look.rooftop_gear = true;
    });
    out
}

/// Real buildings from a town: a few of buildings of halls and of the old
/// plans, the largest first (a taxi pad needs 12 m).
fn samples(town: &GeneratedTown) -> Vec<(BuildingPlan, f64)> {
    let all = town.world.everything();
    let o = &town.obstacles;
    let mut found: Vec<(BuildingPlan, f64)> = Vec::new();
    for (_, wb) in &all.buildings {
        let site = &wb.site;
        if site.size.min_element() < 14.0 {
            continue;
        }
        let mid = site.origin + site.along * site.size.x * 0.5 + site.inward * site.size.y * 0.5;
        let Some(id) = o.buildings_near(mid, 1.0).into_iter().find(|&i| {
            o.building(i)
                .is_some_and(|(p, _)| p.site.origin == site.origin)
        }) else {
            continue;
        };
        let (plan, base) = o.building(id).expect("found above");
        let halls = plan.layout.is_some();
        if found
            .iter()
            .filter(|(p, _)| p.layout.is_some() == halls)
            .count()
            < 3
        {
            found.push((plan.as_ref().clone(), base));
        }
        if found.len() >= 6 {
            break;
        }
    }
    found
}

/// Every roof and crown, on real plans of both kinds, against the solids
/// `building_solids` makes for exactly that look.
#[test]
fn every_roof_look_is_solid() {
    let town = GeneratedTown::generate(Seed(1), 60_000.0);
    let plans = samples(&town);
    assert!(
        plans.iter().any(|(p, _)| p.layout.is_some())
            && plans.iter().any(|(p, _)| p.layout.is_none()),
        "no plans of both kinds to try"
    );
    let mut worst = BTreeMap::new();
    let mut probed = 0;
    for (plan, _) in &plans {
        for (name, p) in variants(plan) {
            // Standing on flat ground, as the mesh has it.
            let mut o = Obstacles::flat(0.0);
            o.add_all(building_solids(&p, BLOCK_TOP));
            let (gaps, n) = probe(&o, &p, BLOCK_TOP);
            probed += n;
            collect(
                &mut worst,
                format!("{name}, halls {}", p.layout.is_some()),
                gaps,
                n,
            );
        }
    }
    eprintln!("{probed} probes over {} plans", plans.len());
    assert!(probed > 100_000, "only {probed} probes: nothing was drawn");
    report(&worst);
}

/// The roofs of real buildings as the town's own colliders have them,
/// overhangs outside the site included.
#[test]
fn the_roofs_of_a_town_are_solid() {
    let town = GeneratedTown::generate(Seed(2), 20_000.0);
    let mut worst = BTreeMap::new();
    for (plan, base, _) in picks(&town, 1) {
        let (gaps, n) = probe(&town.obstacles, &plan, base);
        let look = &plan.look;
        collect(
            &mut worst,
            format!(
                "{:?} roof, {:?} crown, halls {}",
                look.roof,
                look.crown,
                plan.layout.is_some()
            ),
            gaps,
            n,
        );
    }
    report(&worst);
}
