//! The harbour of a coastal city: it is there, its mole is ground a hero
//! stands on, everything drawn on it is solid, the warehouses are real
//! (destructible) buildings, no street prop or other building stands on
//! the mole, and the ships at sea are solid where they sail.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_gen::urban::harbour::{ShipKind, MOLE_HALF, MOLE_LEN};
use hw_sim::airships::Airships;
use hw_sim::collision::Solid;
use hw_sim::GeneratedTown;

fn inside(s: &Solid, p: DVec3) -> bool {
    let d = DVec2::new(p.x, p.z) - s.origin;
    let l = DVec2::new(d.dot(s.along), d.dot(s.inward));
    l.cmpge(s.min).all() && l.cmple(s.max).all() && p.y >= s.y0 && p.y <= s.y1
}

/// A building's corners and middle.
fn spots(b: &hw_gen::building::BuildingSite) -> [DVec2; 5] {
    let (a, i) = (b.along * b.size.x, b.inward * b.size.y);
    [
        b.origin,
        b.origin + a,
        b.origin + i,
        b.origin + a + i,
        b.origin + (a + i) * 0.5,
    ]
}

#[test]
#[ignore = "daily (slow blocker): scripts/check.sh --daily, and CI's first run of the day"]
fn the_harbour_is_solid_and_clear() {
    for seed in [1, 2] {
        let g = GeneratedTown::generate_on_planet(Seed(seed), 40_000.0);
        let w = &g.world;
        let h = w
            .harbour
            .as_ref()
            .unwrap_or_else(|| panic!("seed {seed}: no harbour"));
        let o = &g.obstacles;
        // The mole is ground at its deck's height, out to its head.
        for z in [20.0, 100.0, MOLE_LEN - 15.0] {
            for x in [-MOLE_HALF + 2.0, 0.0, MOLE_HALF - 2.0] {
                let p = h.to_plan(DVec2::new(x, z));
                assert!(
                    (o.ground_at(p) - h.deck).abs() < 0.01,
                    "seed {seed}: the deck at {x},{z} is {:.2}, not {:.2}",
                    o.ground_at(p),
                    h.deck
                );
            }
        }
        // Everything standing on it stops a hero.
        for f in h.footprints() {
            let c = f.at();
            let (y0, y1) = f.span();
            let y = y0 + (0.5 * (y1 - y0)).min(0.3);
            assert!(
                o.solid_at(DVec3::new(c.x, y, c.y)),
                "seed {seed}: a harbour part at {c:?}, {y:.1} m up, can be walked through"
            );
        }
        // The warehouses are buildings of the town; nothing else is on the mole.
        let all = w.everything();
        let sheds: Vec<_> = all
            .buildings
            .iter()
            .filter(|(_, b)| h.covers(spots(&b.site)[4], -2.0))
            .map(|(_, b)| b.site.id)
            .collect();
        assert_eq!(
            sheds.len(),
            2,
            "seed {seed}: {} warehouses on the mole",
            sheds.len()
        );
        for (_, b) in all
            .buildings
            .iter()
            .filter(|(_, b)| !sheds.contains(&b.site.id))
        {
            assert!(
                !spots(&b.site).iter().any(|p| h.covers(*p, -1.0)),
                "seed {seed}: a building at {:?} stands on the mole",
                b.site.origin
            );
        }
        for (_, p) in &all.props {
            assert!(
                !h.covers(p.pos, -1.0),
                "seed {seed}: a {:?} stands on the mole at {:?}",
                p.kind,
                p.pos
            );
        }
        // The freighter and the tug are solid wherever they sail.
        let ships = Airships::new(w);
        for t in (0..h.cycle() as usize).step_by(97) {
            let t = t as f64;
            for (kind, pose) in h.ships_at(t) {
                if kind == ShipKind::Launch {
                    continue;
                }
                // Off the joins between the hull's boxes, where rounding can fall
                // either side of both.
                let c = pose.to_plan(DVec2::new(0.3, 0.7));
                let mid = DVec3::new(c.x, kind.dims().free * 0.5, c.y);
                let solids = ships.solids_near(&[mid], t);
                assert!(
                    solids.iter().any(|s| inside(s, mid)),
                    "seed {seed}: the {kind:?} at {t} s ({:?}) can be walked through: {} solids, nearest {:?}",
                    pose,
                    solids.len(),
                    solids
                        .iter()
                        .map(|s| s.to_world((s.min + s.max) * 0.5).distance(pose.at))
                        .fold(f64::MAX, f64::min)
                );
            }
        }
    }
}
