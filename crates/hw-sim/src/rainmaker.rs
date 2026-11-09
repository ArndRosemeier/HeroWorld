//! The weather tower's day: when it brews its cloud, rains, clears and
//! shows its rainbow. A pure function of the game clock, so every player
//! (and the server, which slows fires under the rain) sees the same shower
//! at the same moment.
//!
//! One round takes [`CYCLE`] seconds:
//!
//! | from (s) | what |
//! | --- | --- |
//! | 0 | fair: the globe glows gold, the vanes turn idly |
//! | 60 | brewing: the cannons fire vapour up, the cloud grows and darkens |
//! | 95 | rain over the blocks in front of the tower, thunder now and then |
//! | 185 | clearing: the rain eases off, the cloud thins and whitens |
//! | 200 | the rainbow, while the last of the cloud drifts apart |
//! | 260 | fair again |

use glam::DVec2;
use hw_gen::urban::landmarks::Landmark;
use hw_gen::urban::weather_tower::{rain_centre, WX_RAIN_R};

/// Seconds in one round of the tower's weather.
pub const CYCLE: f64 = 300.0;
const BREW: f64 = 60.0;
const RAIN: f64 = 95.0;
const CLEAR: f64 = 185.0;
const BOW: f64 = 200.0;
const FAIR: f64 = 260.0;

/// The tower's weather at one moment, each part 0..1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Shower {
    /// How much cloud there is (its size and thickness).
    pub cloud: f64,
    /// How dark it is underneath (a rain cloud is grey).
    pub dark: f64,
    /// How hard it rains.
    pub rain: f64,
    /// How bright the rainbow is.
    pub bow: f64,
    /// The cannons are firing vapour.
    pub spray: bool,
    /// Where in the round (seconds from its start).
    pub at: f64,
}

fn smooth(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The weather `seconds` into the game clock.
pub fn shower(seconds: f64) -> Shower {
    let t = seconds.rem_euclid(CYCLE);
    let cloud = smooth(BREW, RAIN + 5.0, t) * (1.0 - smooth(BOW - 5.0, FAIR - 20.0, t));
    let dark = smooth(BREW + 15.0, RAIN + 10.0, t) * (1.0 - smooth(CLEAR - 5.0, BOW + 5.0, t));
    let rain = smooth(RAIN, RAIN + 10.0, t) * (1.0 - smooth(CLEAR, BOW, t));
    let bow = smooth(BOW - 5.0, BOW + 10.0, t) * (1.0 - smooth(FAIR - 15.0, FAIR, t));
    Shower {
        cloud,
        dark,
        rain,
        bow,
        spray: (BREW..RAIN + 5.0).contains(&t),
        at: t,
    }
}

/// How hard it rains at `p` (0..1): the tower's shower, softening over
/// its last few metres.
pub fn rain_at(tower: &Landmark, p: DVec2, seconds: f64) -> f64 {
    let d = p.distance(rain_centre(tower));
    shower(seconds).rain * (1.0 - smooth(WX_RAIN_R - 10.0, WX_RAIN_R, d))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_round_brews_rains_clears_and_shows_a_bow() {
        let s = |t: f64| shower(t);
        assert_eq!(s(10.0).cloud, 0.0);
        assert_eq!(s(10.0).rain, 0.0);
        assert!(s(70.0).spray && s(70.0).cloud > 0.0 && s(70.0).rain == 0.0);
        assert!(s(140.0).rain == 1.0 && s(140.0).cloud == 1.0 && s(140.0).dark == 1.0);
        assert!(s(140.0).bow == 0.0, "no rainbow in the rain");
        assert!(s(230.0).bow == 1.0 && s(230.0).rain == 0.0);
        assert_eq!(
            s(280.0),
            Shower {
                at: 280.0,
                ..Default::default()
            }
        );
        // Round after round, the same.
        assert_eq!(s(140.0 + CYCLE * 7.0).rain, 1.0);
        // Never a jump: everything changes smoothly second to second.
        for i in 0..(CYCLE as usize * 10) {
            let (a, b) = (s(i as f64 * 0.1), s(i as f64 * 0.1 + 0.1));
            for (x, y) in [
                (a.cloud, b.cloud),
                (a.rain, b.rain),
                (a.bow, b.bow),
                (a.dark, b.dark),
            ] {
                assert!((x - y).abs() < 0.03, "at {}", i as f64 * 0.1);
            }
        }
    }
}
