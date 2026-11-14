//! The karma multiplier: a world setting the host picks when starting a
//! game (0.1 to 10, normally 1). Every karma a hero earns (fights, crimes,
//! rescues, missions, the story, augments traded in) is scaled by it on the
//! server, so in a shared world the host's value is everyone's. Karma
//! handed back (giving a power up) and the debug menu's grants are not
//! earnings and stay as they are.

/// The slowest rate: a tenth of the karma.
pub const MIN: f32 = 0.1;
/// The fastest rate: ten times the karma.
pub const MAX: f32 = 10.0;
/// The normal rate.
pub const DEFAULT: f32 = 1.0;

/// `rate` made usable: within [`MIN`], [`MAX`] (not-a-number gives [`DEFAULT`]).
pub fn clamp(rate: f32) -> f32 {
    if rate.is_finite() {
        rate.clamp(MIN, MAX)
    } else {
        DEFAULT
    }
}

/// `karma` earned at `rate`, rounded; an earning is never rounded to nothing.
pub fn scale(karma: u32, rate: f32) -> u32 {
    if karma == 0 {
        return 0;
    }
    ((karma as f64 * clamp(rate) as f64).round() as u32).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rate_scales_earnings() {
        assert_eq!(scale(7, 1.0), 7);
        assert_eq!(scale(7, 10.0), 70);
        assert_eq!(scale(7, 0.1), 1);
        assert_eq!(scale(100, 0.1), 10);
        assert_eq!(scale(0, 5.0), 0);
        assert_eq!(scale(10, 50.0), 100, "past the top it is the top");
        assert_eq!(clamp(f32::NAN), DEFAULT);
        assert_eq!(clamp(0.0), MIN);
    }
}
