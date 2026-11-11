//! A sidekick's little tricks while its hero stands about: now and then it
//! waves up at them, hops, has a look round, polishes its chest core, tips
//! its head, stretches or does a jig. Only while it stands idle and out of
//! a fight; the clips are made in [`crate::poses::add_sidekick_clips`].
//!
//! Which trick and when comes from the sidekick's id and how many it has
//! done, so it is the same on every machine and never random per frame.

/// The tricks: clip, seconds it plays, played once (else looped for that
/// long).
pub const TRICKS: &[(&str, f32, bool)] = &[
    ("wave", 2.4, true),
    ("hop", 1.5, true),
    ("look around", 3.6, true),
    ("polish", 2.8, true),
    ("head tilt", 2.0, true),
    ("stretch", 2.6, true),
    ("dance", 3.4, false),
];

/// Seconds standing before the first trick, and between tricks.
pub const FIRST_AFTER: f32 = 3.0;
pub const GAP_MIN: f32 = 6.0;
pub const GAP_MAX: f32 = 14.0;

/// One sidekick's tricks.
#[derive(Clone, Debug, Default)]
pub struct Tricks {
    /// Seconds standing idle so far.
    still: f32,
    /// When (in `still` seconds) the next trick starts.
    next: f32,
    /// The trick playing and the seconds left of it.
    playing: Option<(usize, f32)>,
    /// Tricks done, to vary the next, and the last one.
    done: u32,
    last: Option<usize>,
}

/// A small hash for the choices.
fn mix(a: u64, b: u64) -> u64 {
    let mut x = a.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ b.wrapping_add(0xD1B5_4A32_D192_ED03);
    x ^= x >> 31;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^ (x >> 29)
}

impl Tricks {
    /// One frame. `idle`: it stands, out of a fight. Returns the trick to
    /// show (clip, played once) or `None` for its plain idle.
    pub fn step(&mut self, id: u64, idle: bool, dt: f32) -> Option<(&'static str, bool)> {
        if !idle {
            self.still = 0.0;
            self.next = 0.0;
            self.playing = None;
            return None;
        }
        if self.still == 0.0 {
            self.next = FIRST_AFTER + (mix(id, self.done as u64) % 1000) as f32 / 1000.0 * 2.0;
        }
        self.still += dt;
        if let Some((i, left)) = &mut self.playing {
            *left -= dt;
            if *left > 0.0 {
                let (clip, _, once) = TRICKS[*i];
                return Some((clip, once));
            }
            self.playing = None;
            let r = (mix(id, 1000 + self.done as u64) % 1000) as f32 / 1000.0;
            self.next = self.still + GAP_MIN + (GAP_MAX - GAP_MIN) * r;
        }
        if self.still >= self.next {
            // Never the same trick twice running.
            let n = TRICKS.len() as u64;
            let mut i = (mix(id, self.done as u64 + 7) % n) as usize;
            if Some(i) == self.last {
                i = (i + 1) % TRICKS.len();
            }
            self.done += 1;
            self.last = Some(i);
            self.playing = Some((i, TRICKS[i].1));
            let (clip, _, once) = TRICKS[i];
            return Some((clip, once));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn tricks_come_now_and_then_and_stop_when_it_moves() {
        let mut t = Tricks::default();
        let dt = 1.0 / 60.0;
        let mut shown = Vec::new();
        let mut busy = 0;
        for _ in 0..60 * 60 {
            if let Some((clip, _)) = t.step(42, true, dt) {
                busy += 1;
                if shown.last() != Some(&clip) {
                    shown.push(clip);
                }
            }
        }
        // A minute standing: a handful of tricks, mostly plain idle.
        assert!((3..=9).contains(&shown.len()), "{shown:?}");
        assert!(busy < 60 * 30, "busy {busy} frames of 3600");
        assert!(shown.windows(2).all(|w| w[0] != w[1]));
        // Walking off ends a trick at once.
        assert_eq!(t.step(42, false, dt), None);
        assert_eq!(t.step(42, true, dt), None);
    }
}
