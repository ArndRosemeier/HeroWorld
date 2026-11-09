//! Stopping a long job from outside: the job checks a shared flag between
//! its steps and gives up with [`Cancelled`] once it is set.
//!
//! Cancelling is a request, not a guarantee of timing: a job may finish
//! before it next looks. Whoever cancels must therefore still tell a stale
//! result from a wanted one (by what it was made for), never rely on the
//! job having stopped by some time.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A shared "please stop" flag. Clones share it.
#[derive(Clone, Debug, Default)]
pub struct Cancel(Arc<AtomicBool>);

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
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
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
}
