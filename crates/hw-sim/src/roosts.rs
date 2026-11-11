//! Where the new factions live in a town. The sky pirates moor their
//! airship over the harbour, a rope ladder down to the quay, where a few
//! of them keep watch and warn strangers off; the delvers of Molehaven
//! stand guard inside the cavern, by the lift down from the street. (The
//! Iron-Masked Empire has no home here: it comes from beyond the sea; nor
//! have the invaders from Zorrak, whose saucers come down from the moon,
//! see [`saucer_hover`].)
//!
//! Server and clients both work these out from the town alone, so the
//! airship everyone sees hangs over the pirates the server placed.

use glam::{DVec2, DVec3};
use hw_gen::urban::underground::Underground;
use hw_gen::urban::world::TownWorld;

/// The pirates' airship *Cloudcutter* (drawn in the client's
/// `factionview`): its envelope's length and widest radius (m).
pub const AIRSHIP_LENGTH: f64 = 46.0;
pub const AIRSHIP_RADIUS: f64 = 7.5;
/// Where the gondola's deck is below the envelope's axis (m): rope
/// ladders hang from here. The gondola's hull is 18 m long, 2 m to each
/// side, 2.4 m deep under its deck.
pub const GONDOLA_DECK: f64 = AIRSHIP_RADIUS + 2.2;

/// Radius of the envelope at `t` (0 nose to 1 tail): a blunt nose, the
/// widest a third of the way back, a long taper to the tail.
pub fn envelope_r(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    let nose = (t / 0.33).min(1.0);
    let tail = ((1.0 - t) / 0.67).min(1.0);
    let n = (1.0 - (1.0 - nose).powi(2)).max(0.0).sqrt();
    let k = hw_core::det::powf(tail, 0.75);
    AIRSHIP_RADIUS * n * k.max(0.0)
}

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
    let mut best: Option<PirateRoost> = None;
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
            // High enough to clear whatever stands under it. A spot where
            // it hangs at its usual height wins; else the lowest.
            let roofs = roofs_under(w, c, along, SHIP_HALF.0 + 6.0, SHIP_HALF.1 + 6.0);
            let roost = PirateRoost {
                quay: q.pts[k],
                out,
                top: q.top[k],
                ship_y: (q.top[k] + SHIP_UP).max(roofs + SHIP_KEEL),
            };
            if roost.ship_y <= roost.top + SHIP_UP {
                return Some(roost);
            }
            let lift = roost.ship_y - roost.top;
            if best.is_none_or(|b: PirateRoost| lift < b.ship_y - b.top) {
                best = Some(roost);
            }
        }
    }
    best
}

/// The highest thing drawn with any part in the box centred on `c`,
/// `half_len` along `along` and `half_wide` across it (or -infinity over
/// open ground): roofs with their crowns and spires, landmarks. What an
/// airship hanging there has to clear. Straight from the town plan, so
/// server and clients agree whatever is loaded.
pub fn roofs_under(w: &TownWorld, c: DVec2, along: DVec2, half_len: f64, half_wide: f64) -> f64 {
    use hw_gen::urban::airspace::{highest_under, SkyBox};
    highest_under(w, &SkyBox::new(c, along, half_len, half_wide), &[])
}

/// A raiding airship's keel: from its axis to under its gondola (m).
const RAID_KEEL: f64 = GONDOLA_DECK + 2.6;
/// Seconds a departing raider sails on, once clear of the roofs ahead.
pub const RAID_SAIL_S: f64 = 40.0;
/// How fast it rises straight up first (m/s).
const RAID_RISE: f64 = 3.0;

/// Where the sky pirates' airship hangs over `street` while they raid it,
/// and its heading (radians, 0 = +z, towards +x): turned the way along
/// which what stands under it is lowest, its gondola clear of every roof
/// and wall there, and never lower than a ladder's climb over the street.
pub fn raid_mooring(w: &TownWorld, street: DVec3) -> (DVec3, f64) {
    let half = AIRSHIP_LENGTH * 0.5 + 2.0;
    let r = AIRSHIP_RADIUS + 2.0;
    let c = DVec2::new(street.x, street.z);
    let (mut best, mut best_yaw) = (f64::INFINITY, 0.0);
    for k in 0..8 {
        let yaw = (k as f64 * 22.5).to_radians();
        let along = hw_core::det::dir2(std::f64::consts::FRAC_PI_2 - yaw);
        let top = roofs_under(w, c, along, half, r).max(street.y);
        if top < best - 0.5 {
            best = top;
            best_yaw = yaw;
        }
    }
    let y = (best + AIRSHIP_RADIUS + 4.0)
        .max(best + RAID_KEEL + 4.0)
        .max(street.y + RAID_KEEL + 14.0);
    (DVec3::new(street.x, y, street.z), best_yaw)
}

/// A raider leaving `from` heading `yaw`: the height it rises to before it
/// sails off, clear of everything on its way out. Where the town ahead is
/// not planned yet (far from every hero) it does not wait for it: it
/// rises over the tallest tower the town could have instead.
pub fn raid_sail_height(w: &TownWorld, from: DVec3, yaw: f64) -> f64 {
    use hw_gen::urban::airspace::{highest_under, planned, SkyBox};
    let along = hw_core::det::dir2(std::f64::consts::FRAC_PI_2 - yaw);
    let reach = 0.35 * RAID_SAIL_S * RAID_SAIL_S;
    let mid = DVec2::new(from.x, from.z) + along * (reach * 0.5);
    let sky = SkyBox::new(
        mid,
        along,
        reach * 0.5 + AIRSHIP_LENGTH,
        AIRSHIP_RADIUS + 3.0,
    );
    let top = if planned(w, &sky) {
        highest_under(w, &sky, &[])
    } else {
        w.ground(DVec2::new(from.x, from.z))
            + hw_gen::urban::mast::skyline(w.town.params.population)
    };
    from.y.max(top + RAID_KEEL + 4.0)
}

/// Where a departing raider is `age` seconds after the pirates climb
/// aboard: straight up to `clear` (from [`raid_sail_height`]), then off
/// along its heading, slowly at first, then under full steam, still
/// climbing. `None` once it is gone.
pub fn raid_departure(from: DVec3, yaw: f64, clear: f64, age: f64) -> Option<DVec3> {
    let rise = (clear - from.y).max(0.0) / RAID_RISE;
    if age > rise + RAID_SAIL_S {
        return None;
    }
    let along = hw_core::det::dir2(std::f64::consts::FRAC_PI_2 - yaw);
    if age < rise {
        return Some(from + DVec3::Y * (age * RAID_RISE));
    }
    let t = age - rise;
    let s = (t * t * 0.35).min(t * 14.0);
    Some(DVec3::new(
        from.x + along.x * s,
        clear + t * 1.2,
        from.z + along.y * s,
    ))
}

/// A Zorrak flying saucer, as it is drawn (the client's `factionview`)
/// and as it is solid ([`crate::airships`]): its profile from the middle
/// out, `(radius, height)` about its rim's plane, under side from the
/// beam's lens out to the rim, then the top in to the glass dome's crown.
pub const SAUCER_UNDER: [(f64, f64); 7] = [
    (0.0, -1.5),
    (1.1, -1.5),
    (2.2, -1.25),
    (4.5, -0.95),
    (6.4, -0.5),
    (7.5, -0.15),
    (7.8, 0.0),
];
pub const SAUCER_TOP: [(f64, f64); 9] = [
    (7.8, 0.0),
    (7.6, 0.2),
    (6.2, 0.62),
    (4.2, 1.0),
    (3.3, 1.12),
    (3.0, 1.35),
    (2.6, 2.3),
    (1.5, 2.95),
    (0.0, 3.15),
];
/// The saucer's widest radius (m).
pub const SAUCER_RADIUS: f64 = 7.8;
/// From the rim's plane down to the beam's lens (m).
pub const SAUCER_UNDER_DEPTH: f64 = 1.5;
/// How high its lens hangs over the street at least, and over any roof
/// under it (m).
const SAUCER_OVER_STREET: f64 = 13.0;
const SAUCER_OVER_ROOFS: f64 = 4.0;

/// Height of a profile at radius `r` (by straight lines between its points).
fn profile_at(p: &[(f64, f64)], r: f64) -> f64 {
    let r = r.abs();
    for w in p.windows(2) {
        let (lo, hi) = if w[0].0 <= w[1].0 {
            (w[0], w[1])
        } else {
            (w[1], w[0])
        };
        if r >= lo.0 && r <= hi.0 {
            let span = hi.0 - lo.0;
            if span < 1e-9 {
                return lo.1.max(hi.1);
            }
            return lo.1 + (hi.1 - lo.1) * (r - lo.0) / span;
        }
    }
    0.0
}

/// How far the saucer reaches below and above its rim's plane at radius
/// `r` from its middle: `(under, over)`, or `None` past its rim.
pub fn saucer_span(r: f64) -> Option<(f64, f64)> {
    (r <= SAUCER_RADIUS).then(|| (profile_at(&SAUCER_UNDER, r), profile_at(&SAUCER_TOP, r)))
}

/// Where a Zorrak saucer hangs over `street` while its troopers raid it:
/// the middle of its rim, straight over the street, its lens a good way up
/// and clear of every roof and spire under it.
pub fn saucer_hover(w: &TownWorld, street: DVec3) -> DVec3 {
    let c = DVec2::new(street.x, street.z);
    let r = SAUCER_RADIUS + 2.0;
    let roofs = roofs_under(w, c, DVec2::X, r, r).max(street.y);
    let lens = (street.y + SAUCER_OVER_STREET).max(roofs + SAUCER_OVER_ROOFS);
    DVec3::new(street.x, lens + SAUCER_UNDER_DEPTH, street.z)
}

/// Seconds a departing saucer takes to be gone.
pub const SAUCER_GONE_S: f64 = 7.0;

/// Where a saucer that hung at `from` is `age` seconds after its raid
/// ended: a short hop up, a hover while it makes up its mind, then off up
/// into the sky along `dir` (a flat unit direction) faster and faster.
/// `None` once it is gone.
pub fn saucer_departure(from: DVec3, dir: DVec2, age: f64) -> Option<DVec3> {
    if age > SAUCER_GONE_S {
        return None;
    }
    let hop = 4.0 * (age / 1.2).min(1.0);
    let t = (age - 1.8).max(0.0);
    let off = 9.0 * t * t * t;
    let up = hop + 6.0 * t * t;
    Some(from + DVec3::new(dir.x * off, up, dir.y * off))
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
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
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
