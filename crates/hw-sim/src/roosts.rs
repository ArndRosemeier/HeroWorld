//! Where the new factions live in a town. The sky pirates moor their
//! airship over the harbour, a rope ladder down to the quay, where a few
//! of them keep watch and warn strangers off; the delvers of Molehaven
//! stand guard inside the cavern, by the lift down from the street. (The
//! Iron-Masked Empire has no home here: it comes from beyond the sea.)
//!
//! Server and clients both work these out from the town alone, so the
//! airship everyone sees hangs over the pirates the server placed.

use glam::{DVec2, DVec3};
use hw_gen::urban::underground::Underground;
use hw_gen::urban::world::TownWorld;

/// How far out over the water the airship hangs from the quay (m), and how
/// high above the quay its axis is at least; it keeps [`SHIP_KEEL`] over
/// any roof under it.
pub const SHIP_OUT: f64 = 16.0;
pub const SHIP_UP: f64 = 30.0;
/// From the airship's axis down to under its gondola, with a margin (m).
pub const SHIP_KEEL: f64 = 14.0;
/// The airship's half length and half width, for what is under it (m).
const SHIP_HALF: (f64, f64) = (24.0, 8.0);
/// How far from the town's core the pirates look for a quay (m): only
/// those quarters are planned, so a megacity costs no more than a village.
pub const ROOST_SEARCH: f64 = 450.0;
/// Pirates on the quay, and how near a hero wakes them (m).
pub const ROOST_GROUP: u32 = 4;
/// Delvers at Molehaven's gate.
pub const GATE_GROUP: u32 = 3;

/// The sky pirates' landing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PirateRoost {
    /// On the quay's edge, where the ladder comes down.
    pub quay: DVec2,
    /// Out over the water (unit).
    pub out: DVec2,
    /// The street's height there.
    pub top: f64,
    /// Height of the airship's axis: clear of every roof under it.
    pub ship_y: f64,
}

impl PirateRoost {
    /// Where the airship's envelope is centred.
    pub fn ship(&self) -> DVec3 {
        let p = self.quay + self.out * SHIP_OUT;
        DVec3::new(p.x, self.ship_y, p.y)
    }

    /// The airship's heading (radians, 0 = +z): along the quay.
    pub fn ship_yaw(&self) -> f64 {
        let along = self.out.perp();
        hw_core::det::atan2(along.x, along.y)
    }

    /// Where the lookouts stand: a few metres in from the edge, facing
    /// the town.
    pub fn watch(&self) -> (DVec3, DVec2) {
        let p = self.quay - self.out * 5.0;
        (DVec3::new(p.x, self.top, p.y), -self.out)
    }
}

/// The pirates' landing: a point on a quay wall near the core with open
/// water in front of it wide enough for the airship to hang over, the
/// longest walls first, nearest their middles.
pub fn pirate_roost(w: &TownWorld) -> Option<PirateRoost> {
    use hw_gen::urban::world::WATER_LEVEL;
    let wet = |p: DVec2| w.ground(p) < WATER_LEVEL + 0.05;
    let core = w.core();
    let near = w.quarters_in(
        core - DVec2::splat(ROOST_SEARCH),
        core + DVec2::splat(ROOST_SEARCH),
    );
    let mut walls: Vec<&hw_gen::urban::waterfront::QuayWall> = near
        .iter()
        .flat_map(|q| q.quays.iter())
        .filter(|q| q.pts.len() >= 8)
        .collect();
    walls.sort_by(|a, b| {
        b.pts
            .len()
            .cmp(&a.pts.len())
            .then(a.pts[0].x.total_cmp(&b.pts[0].x))
            .then(a.pts[0].y.total_cmp(&b.pts[0].y))
    });
    for q in walls {
        let n = q.pts.len();
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by_key(|&k| (k as i64 - n as i64 / 2).abs());
        for k in order {
            let out = q.out[k].normalize_or(DVec2::X);
            let along = out.perp();
            let c = q.pts[k] + out * SHIP_OUT;
            // Its middle and ends over the water, its lookouts on land.
            let over_water = (-2..=2).all(|i| wet(c + along * (8.0 * i as f64)));
            if !over_water || wet(q.pts[k] - out * 5.0) {
                continue;
            }
            // High enough to clear whatever stands under it.
            let roofs = roofs_under(w, c, along, SHIP_HALF.0 + 6.0, SHIP_HALF.1 + 6.0);
            return Some(PirateRoost {
                quay: q.pts[k],
                out,
                top: q.top[k],
                ship_y: (q.top[k] + SHIP_UP).max(roofs + SHIP_KEEL),
            });
        }
    }
    None
}

/// The highest roof with any part in the box centred on `c`, `half_len`
/// along `along` and `half_wide` across it (or -infinity over open ground):
/// what an airship hanging there has to clear. Straight from the town plan,
/// so server and clients agree whatever is loaded.
pub fn roofs_under(w: &TownWorld, c: DVec2, along: DVec2, half_len: f64, half_wide: f64) -> f64 {
    let across = along.perp();
    let inside = |p: DVec2| {
        let d = p - c;
        d.dot(along).abs() < half_len && d.dot(across).abs() < half_wide
    };
    let reach = DVec2::splat(half_len.max(half_wide) + 40.0);
    w.quarters_in(c - reach, c + reach)
        .iter()
        .flat_map(|q| q.buildings.iter())
        .filter(|b| {
            let k = b.site.corners();
            // A corner inside, the middle inside, or the box's middle
            // inside the building.
            k.iter().any(|&p| inside(p)) || inside(b.site.origin) || hw_gen::urban::contains(&k, c)
        })
        .map(|b| w.ground(b.site.origin) + b.site.height())
        .fold(f64::NEG_INFINITY, f64::max)
}

/// Where Molehaven's guards stand: on open cavern floor as near the lift
/// down from the street as there is room for them (clear of the well, the
/// stalls and the mushroom houses), facing the lift.
pub fn mole_gate(u: &Underground) -> Option<(DVec3, DVec2)> {
    use hw_gen::urban::underground::VoidKind;
    let cavern = u.cavern.as_ref()?;
    let lift = u.entrances.iter().find(|e| e.molehaven)?;
    let layout = hw_gen::urban::underground::molehaven_layout(u);
    let free = |p: DVec2| {
        layout.as_ref().is_none_or(|l| {
            p.distance(l.well.0) > l.well.1 + 4.0
                && l.stalls.iter().all(|s| p.distance(s.0) > 4.0)
                && l.houses.iter().all(|h| p.distance(h.at) > h.cap + 3.0)
        })
    };
    let mut best: Option<(f64, DVec2, f64)> = None;
    for v in cavern
        .chambers
        .iter()
        .filter_map(|&i| u.voids.get(i))
        .filter(|v| v.kind == VoidKind::Cavern)
    {
        let size = v.max - v.min;
        let (nx, ny) = ((size.x / 2.0) as i32, (size.y / 2.0) as i32);
        for i in 0..=nx {
            for j in 0..=ny {
                let l = v.min + DVec2::new(i as f64 * 2.0, j as f64 * 2.0);
                let p = v.world(l);
                if !v.covers(p, -3.0) || !free(p) {
                    continue;
                }
                let d = p.distance(lift.pos);
                // Not in the lift's own doorway.
                if d < 6.0 {
                    continue;
                }
                if best.is_none_or(|b| d < b.0 - 1e-9) {
                    best = Some((d, p, v.floor_at(p)));
                }
            }
        }
    }
    let (_, p, y) = best?;
    Some((
        DVec3::new(p.x, y, p.y),
        (lift.pos - p).normalize_or(DVec2::X),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hw_core::Seed;
    use hw_gen::urban::TownParams;

    #[test]
    fn towns_by_the_water_have_a_pirate_landing_and_a_mole_gate() {
        let mut roosts = 0;
        let mut gates = 0;
        for seed in [1u64, 2, 3, 4] {
            let w = TownWorld::generate(Seed(seed), TownParams::sample(Seed(seed), 20_000.0));
            if let Some(r) = pirate_roost(&w) {
                roosts += 1;
                // The airship hangs over water, the lookouts stand on land.
                let s = r.ship();
                assert!(w.ground(DVec2::new(s.x, s.z)) < hw_gen::urban::world::WATER_LEVEL + 0.1);
                assert!(s.y >= r.top + SHIP_UP);
                let (watch, _) = r.watch();
                assert!(w.ground(DVec2::new(watch.x, watch.z)) > hw_gen::urban::world::WATER_LEVEL);
            }
            if mole_gate(w.underground()).is_some() {
                gates += 1;
            }
        }
        // Some towns have no harbour wide enough (or no water at all).
        assert!(roosts >= 2, "{roosts} towns of 4 have a pirate landing");
        assert!(gates >= 2, "{gates} towns of 4 have a mole gate");
    }
}
