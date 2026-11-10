//! The mole folk's drill rig: a riveted iron derrick over a bore down to
//! Molehaven, with a giant fluted drill turning in it, a boiler and its
//! smokestack beside. Now and then the bore rumbles and the steam of the
//! deep bursts out of it as a geyser.
//!
//! It stands on its own plinth right over Molehaven's great hall (the
//! cavern is planned under it, see [`super::underground`]), so the bore
//! comes down over the well in the middle of the mole folk's market.
//!
//! All sizes are in the rig's own frame: origin on the plinth's top, +z
//! the way it faces.

use super::footprint::Footprint;
use glam::DVec2;

/// The bore's iron collar: outer radius and height.
pub const COLLAR_R: f64 = 3.4;
pub const COLLAR_H: f64 = 1.5;
/// The derrick's four legs stand at (±LEG_FOOT, ±LEG_FOOT) and lean in to
/// (±LEG_TOP_AT, ±LEG_TOP_AT) under the crown.
pub const LEG_FOOT: f64 = 6.2;
pub const LEG_TOP_AT: f64 = 1.9;
/// The crown block's deck, on top of the legs.
pub const CROWN_Y: f64 = 23.0;
/// The gearbox the drill hangs from: its bottom and top.
pub const GEAR_Y0: f64 = 15.5;
pub const GEAR_Y1: f64 = 18.7;
/// The drill bit: a fluted cone, widest at its top.
pub const BIT_R: f64 = 2.5;
pub const BIT_LEN: f64 = 7.0;
/// Where the bit's tip sits while it bores (inside the collar), and how
/// far it is hauled up while the geyser blows.
pub const BIT_TIP_Y: f64 = 0.35;
pub const BIT_LIFT: f64 = 3.2;
/// The kelly bar from the bit up into the gearbox.
pub const KELLY_R: f64 = 0.55;
/// The boiler: lying across the back of the plinth (local -z), its axis
/// along x at this height, this radius and half length.
pub const BOILER_Z: f64 = -10.2;
pub const BOILER_Y: f64 = 2.15;
pub const BOILER_R: f64 = 1.55;
pub const BOILER_HALF: f64 = 3.6;
/// The smokestack beside the boiler: where, how wide, how tall.
pub const STACK_AT: (f64, f64) = (5.3, -10.2);
pub const STACK_R: f64 = 0.65;
pub const STACK_TOP: f64 = 14.0;
/// The flywheel of the boiler's engine, at the boiler's far end (local
/// -x): its centre and radius; it turns about the x axis.
pub const FLYWHEEL: (f64, f64, f64) = (-5.45, 1.65, 1.4);
/// The steam pipe from the boiler's dome up the back left leg into the
/// gearbox's motor: the bends it runs through (x, y, z), and its radius.
pub const PIPE: [(f64, f64, f64); 7] = [
    (-1.6, BOILER_Y + BOILER_R + 0.5, BOILER_Z),
    (-1.6, 6.0, BOILER_Z + 0.6),
    (-1.6, 6.2, -7.0),
    (-4.2, 6.8, -4.2),
    (-3.0, 14.0, -3.0),
    (-2.9, 16.4, -1.2),
    (-2.9, 17.1, 0.0),
];
pub const PIPE_R: f64 = 0.2;
/// The gearbox's steam motor, on its -x side: centre, radius, half length
/// (along x).
pub const MOTOR: (f64, f64, f64) = (-2.35, 17.1, 0.0);
pub const MOTOR_R: f64 = 0.75;
pub const MOTOR_HALF: f64 = 0.75;

/// The mole folk's glowing toadstool lamps round the plinth's edge (a
/// plinth of radius `plaza`), clear of the way in at the front.
pub fn lamps(plaza: f64) -> [DVec2; 4] {
    let r = plaza - 1.6;
    [35.0f64, -35.0, 100.0, -100.0].map(|deg| {
        let a = deg.to_radians();
        DVec2::new(a.sin(), a.cos()) * r
    })
}
/// A lamp's stem: radius and height.
pub const LAMP_STEM: (f64, f64) = (0.16, 2.3);

/// The rig's height to the beacon on the crown.
pub const HEIGHT: f64 = CROWN_Y + 3.0;
/// The plinth radii to try, largest first.
pub const PLAZAS: [f64; 2] = [15.0, 13.5];

/// Where a leg is at height `y` (k: 0..4, round the rig).
pub fn leg_at(k: usize, y: f64) -> DVec2 {
    let s = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)][k % 4];
    let t = (y / CROWN_Y).clamp(0.0, 1.0);
    let r = LEG_FOOT + (LEG_TOP_AT - LEG_FOOT) * t;
    DVec2::new(s.0, s.1) * r
}

/// Its solid parts in its own frame, on a plinth of radius `plaza`.
pub fn footprints(plaza: f64) -> Vec<Footprint> {
    let disc = |at: DVec2, r: f64, y0: f64, y1: f64| Footprint::Disc { at, r, y0, y1 };
    let mut v = vec![
        // The collar, the bit (up to where it is hauled), the kelly bar.
        // (The collar with the vents on its sides.)
        disc(DVec2::ZERO, COLLAR_R + 0.5, 0.0, COLLAR_H + 0.4),
        disc(
            DVec2::ZERO,
            BIT_R + 0.1,
            COLLAR_H,
            BIT_TIP_Y + BIT_LEN + BIT_LIFT,
        ),
        disc(DVec2::ZERO, KELLY_R + 0.1, COLLAR_H, GEAR_Y0),
        // The gearbox and the crown block.
        Footprint::Box {
            at: DVec2::ZERO,
            dir: DVec2::Y,
            half: DVec2::new(1.7, 1.7),
            y0: GEAR_Y0,
            y1: GEAR_Y1,
        },
        Footprint::Box {
            at: DVec2::ZERO,
            dir: DVec2::Y,
            half: DVec2::splat(LEG_TOP_AT + 0.6),
            y0: CROWN_Y - 0.5,
            y1: CROWN_Y + 0.4,
        },
        // The boiler on its saddles, and the smokestack.
        Footprint::Box {
            at: DVec2::new(0.0, BOILER_Z),
            dir: DVec2::Y,
            half: DVec2::new(BOILER_HALF + 0.5, BOILER_R + 0.1),
            y0: 0.0,
            y1: BOILER_Y + BOILER_R + 0.6,
        },
        disc(
            DVec2::new(STACK_AT.0, STACK_AT.1),
            STACK_R + 0.25,
            0.0,
            STACK_TOP,
        ),
    ];
    // The flywheel on its bearings, the motor on the gearbox.
    v.push(Footprint::Box {
        at: DVec2::new(FLYWHEEL.0, BOILER_Z),
        dir: DVec2::Y,
        half: DVec2::new(0.55, FLYWHEEL.2 + 0.1),
        y0: 0.0,
        y1: FLYWHEEL.1 + FLYWHEEL.2 + 0.1,
    });
    v.push(Footprint::Box {
        at: DVec2::new(MOTOR.0, MOTOR.2),
        dir: DVec2::Y,
        half: DVec2::new(MOTOR_HALF + 0.2, MOTOR_R + 0.1),
        y0: MOTOR.1 - MOTOR_R - 0.1,
        y1: MOTOR.1 + MOTOR_R + 0.1,
    });
    // The steam pipe: a short post every half metre along it.
    for w in PIPE.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2) + (b.2 - a.2).powi(2)).sqrt();
        let n = (len / 0.5).ceil().max(1.0) as usize;
        for i in 0..n {
            let (t0, t1) = (i as f64 / n as f64, (i + 1) as f64 / n as f64);
            let at = |t: f64| {
                (
                    a.0 + (b.0 - a.0) * t,
                    a.1 + (b.1 - a.1) * t,
                    a.2 + (b.2 - a.2) * t,
                )
            };
            let (p, q) = (at(t0), at(t1));
            v.push(disc(
                DVec2::new((p.0 + q.0) * 0.5, (p.2 + q.2) * 0.5),
                PIPE_R + 0.15,
                p.1.min(q.1) - PIPE_R - 0.1,
                p.1.max(q.1) + PIPE_R + 0.1,
            ));
        }
    }
    // The toadstool lamps' stems.
    for at in lamps(plaza) {
        v.push(disc(at, LAMP_STEM.0 + 0.1, 0.0, LAMP_STEM.1));
    }
    // The legs lean in: a short post per stretch of each (wide enough for
    // the ladder up the front right one).
    let bits = 8;
    for k in 0..4 {
        for i in 0..bits {
            let (y0, y1) = (
                CROWN_Y * i as f64 / bits as f64,
                CROWN_Y * (i + 1) as f64 / bits as f64,
            );
            let at = (leg_at(k, y0) + leg_at(k, y1)) * 0.5;
            v.push(disc(at, 0.75, y0, y1));
        }
    }
    v
}

/// Where the rig is in its cycle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    /// Boring: the drill turns steadily, a wisp of steam at the collar.
    Drill,
    /// The pressure builds: the ground rumbles, the drill races and
    /// shakes, steam spits from the collar (0..1 through it).
    Rumble(f64),
    /// The geyser blows: the drill is hauled up and a column of steam
    /// roars out of the bore (0..1 through it).
    Erupt(f64),
    /// The drill goes back down and the steam drifts off (0..1).
    Settle(f64),
}

/// One cycle's length (s); the eruption comes at a different moment in
/// each.
pub const CYCLE: f64 = 36.0;
pub const RUMBLE_S: f64 = 4.5;
pub const ERUPT_S: f64 = 6.5;
pub const SETTLE_S: f64 = 4.0;

/// Where the rig is at `t` seconds of game clock: a pure function, so
/// everyone who knows the clock sees the same geyser.
pub fn phase(t: f64) -> Phase {
    let t = t.max(0.0);
    let k = (t / CYCLE).floor();
    let into = t - k * CYCLE;
    // Each cycle the build-up starts somewhere between 14 and 21 s in.
    let h = hash01(k as u64 ^ 0x6d6f_6c65);
    let start = 14.0 + 7.0 * h;
    let s = into - start;
    if s < 0.0 {
        Phase::Drill
    } else if s < RUMBLE_S {
        Phase::Rumble(s / RUMBLE_S)
    } else if s < RUMBLE_S + ERUPT_S {
        Phase::Erupt((s - RUMBLE_S) / ERUPT_S)
    } else if s < RUMBLE_S + ERUPT_S + SETTLE_S {
        Phase::Settle((s - RUMBLE_S - ERUPT_S) / SETTLE_S)
    } else {
        Phase::Drill
    }
}

/// A number in 0..1 from `x` (splitmix64).
fn hash01(x: u64) -> f64 {
    let mut z = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

/// How high the bit's tip is over the plinth at a phase.
pub fn bit_tip(p: Phase) -> f64 {
    let ease = |x: f64| x * x * (3.0 - 2.0 * x);
    BIT_TIP_Y
        + BIT_LIFT
            * match p {
                Phase::Drill | Phase::Rumble(_) => 0.0,
                Phase::Erupt(f) => ease((f / 0.15).min(1.0)),
                Phase::Settle(f) => 1.0 - ease(f),
            }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_geyser_blows_once_a_cycle() {
        let (mut erupts, mut was) = (0, false);
        let mut t = 0.0;
        while t < CYCLE * 10.0 {
            let now = matches!(phase(t), Phase::Erupt(_));
            if now && !was {
                erupts += 1;
            }
            was = now;
            // The bit stays inside its collar and under the gearbox.
            let tip = bit_tip(phase(t));
            assert!((BIT_TIP_Y..=BIT_TIP_Y + BIT_LIFT + 1e-9).contains(&tip));
            assert!(tip + BIT_LEN < GEAR_Y0);
            t += 0.05;
        }
        assert_eq!(erupts, 10);
    }

    #[test]
    #[ignore = "on demand (looks): scripts/check.sh --on-demand"]
    fn the_legs_lean_in_under_the_crown() {
        for k in 0..4 {
            assert!((leg_at(k, 0.0).length() - LEG_FOOT * 2f64.sqrt()).abs() < 1e-9);
            assert!(leg_at(k, CROWN_Y).length() < leg_at(k, 0.0).length());
            // Clear of the bit as it rises.
            assert!(leg_at(k, BIT_TIP_Y + BIT_LEN + BIT_LIFT).x.abs() > BIT_R + 0.5);
        }
    }
}
