//! The hour of day. It follows the game clock (the server's ticks), so
//! every player in a session sees the same sky and the same lit windows.

/// The hour a session starts at: mid-afternoon, a sun low enough to model
/// the facades and throw long shadows.
pub const START_HOUR: f64 = 15.5;

/// Real seconds per game hour: a whole day in an hour, dusk about ten
/// minutes into a session.
pub const SECONDS_PER_HOUR: f64 = 150.0;

/// The hour (0 to 24) after `seconds` of game clock.
pub fn hour_at(seconds: f64) -> f64 {
    (START_HOUR + seconds.max(0.0) / SECONDS_PER_HOUR).rem_euclid(24.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn the_day_runs_and_wraps() {
        assert_eq!(hour_at(0.0), START_HOUR);
        assert!((hour_at(SECONDS_PER_HOUR * 2.0) - 17.5).abs() < 1e-9);
        let day = SECONDS_PER_HOUR * 24.0;
        assert!(
            (hour_at(day) - START_HOUR).abs() < 1e-9,
            "a day later, the same hour"
        );
        assert!(hour_at(SECONDS_PER_HOUR * 10.0) < 2.0, "past midnight");
    }
}
