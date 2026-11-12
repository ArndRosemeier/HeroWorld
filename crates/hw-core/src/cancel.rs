//! Stopping a long job from outside: the job checks a shared flag between
//! its steps and gives up with [`Cancelled`] once it is set.
//!
//! Cancelling is a request, not a guarantee of timing: a job may finish
//! before it next looks. Whoever cancels must therefore still tell a stale
//! result from a wanted one (by what it was made for), never rely on the
//! job having stopped by some time.
//!
//! The same shared handle also carries how far the job has got, so a
//! window can show it: the job names the stage it is in ([`Cancel::stage`],
//! with the part of the whole bar that stage fills) and, inside a stage that
//! loops over many units, how many are done ([`Cancel::set_fraction`]).

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

/// The stage a job is in: its name and the part of the whole bar it fills.
#[derive(Clone, Copy, Debug)]
struct Band {
    label: &'static str,
    from: f32,
    to: f32,
}

#[derive(Debug)]
struct Shared {
    stop: AtomicBool,
    band: Mutex<Band>,
    /// How far through the stage (0..1), as `f32` bits.
    inside: AtomicU32,
}

/// A shared "please stop" flag, and how far the job has got. Clones share
/// both.
#[derive(Clone, Debug)]
pub struct Cancel(Arc<Shared>);

impl Default for Cancel {
    fn default() -> Self {
        Cancel(Arc::new(Shared {
            stop: AtomicBool::new(false),
            band: Mutex::new(Band {
                label: "",
                from: 0.0,
                to: 0.0,
            }),
            inside: AtomicU32::new(0),
        }))
    }
}

/// A job stopped because it was cancelled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("cancelled")
    }
}

impl std::error::Error for Cancelled {}

impl Cancel {
    /// A flag nobody has set yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks every job holding a clone to stop at its next check.
    pub fn cancel(&self) {
        self.0.stop.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.stop.load(Ordering::Acquire)
    }

    /// The job enters a stage called `label`, which fills the bar from
    /// `from` to `to` (both 0..1, `from` not below where the last one ended).
    pub fn stage(&self, label: &'static str, from: f32, to: f32) {
        *self.0.band.lock().expect("progress band") = Band { label, from, to };
        self.set_fraction(0.0);
    }

    /// How far through the current stage the job is (0..1).
    pub fn set_fraction(&self, f: f32) {
        self.0
            .inside
            .store(f.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// The whole job's progress (0..1) and the name of its stage ("" before
    /// the first one). Safe to call from any thread, any time.
    pub fn progress(&self) -> (f32, &'static str) {
        let b = *self.0.band.lock().expect("progress band");
        let f = f32::from_bits(self.0.inside.load(Ordering::Relaxed));
        (b.from + (b.to - b.from) * f, b.label)
    }

    /// `Err(Cancelled)` once cancelled: `cancel.check()?` between steps.
    pub fn check(&self) -> Result<(), Cancelled> {
        if self.is_cancelled() {
            Err(Cancelled)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_the_flag() {
        let a = Cancel::new();
        let b = a.clone();
        assert_eq!(b.check(), Ok(()));
        a.cancel();
        assert!(b.is_cancelled());
        assert_eq!(b.check(), Err(Cancelled));
    }

    #[test]
    fn progress_follows_stage_and_fraction() {
        let a = Cancel::new();
        let b = a.clone();
        assert_eq!(b.progress(), (0.0, ""));
        a.stage("roads", 0.2, 0.6);
        assert_eq!(b.progress(), (0.2, "roads"));
        a.set_fraction(0.5);
        let (p, l) = b.progress();
        assert!((p - 0.4).abs() < 1e-6 && l == "roads");
    }
}
