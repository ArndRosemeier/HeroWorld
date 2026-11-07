//! Identifiers.
//!
//! - [`StableId`]: for things the world generator creates (districts, lots,
//!   buildings, rooms). Derived from the thing's seed path, so every machine
//!   that generates the same world agrees on it without talking.
//! - [`NetId`]: for things the server spawns at run time (heroes, mobs,
//!   debris). Handed out by the server's [`NetIdAllocator`].

use crate::seed::Seed;

/// An ID for a generated thing, equal on every machine with the same seed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StableId(pub u64);

impl StableId {
    pub fn from_seed(seed: Seed) -> Self {
        // Hash once more so IDs and seeds of the same path are not equal values
        // that could be mixed up by accident.
        Self(crate::seed::combine(seed.0, 0x1d5_7ab1e))
    }
}

/// An ID for a run-time entity, unique within one world session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NetId(pub u32);

/// Hands out [`NetId`]s in order and never reuses one in a session. On host
/// migration the new host resumes from the highest ID it has seen.
#[derive(Clone, Debug)]
pub struct NetIdAllocator {
    next: u32,
}

impl Default for NetIdAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl NetIdAllocator {
    /// IDs start at 1; 0 is never handed out, so it can mean "none" on the wire.
    pub fn new() -> Self {
        Self { next: 1 }
    }

    /// Continue after `highest_seen`, as a new host does.
    pub fn resume_after(highest_seen: NetId) -> Self {
        Self {
            next: highest_seen
                .0
                .checked_add(1)
                .expect("NetId space exhausted"),
        }
    }

    pub fn allocate(&mut self) -> NetId {
        let id = NetId(self.next);
        self.next = self.next.checked_add(1).expect("NetId space exhausted");
        id
    }

    /// The ID the next call to [`Self::allocate`] returns.
    pub fn peek(&self) -> NetId {
        NetId(self.next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_ids_follow_the_path() {
        let w = Seed(77);
        let a = StableId::from_seed(w.child("building").child(4u32));
        let b = StableId::from_seed(w.child("building").child(4u32));
        let c = StableId::from_seed(w.child("building").child(5u32));
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_ne!(a.0, w.child("building").child(4u32).0);
    }

    #[test]
    fn net_ids_are_unique_and_skip_zero() {
        let mut alloc = NetIdAllocator::new();
        let ids: Vec<_> = (0..5).map(|_| alloc.allocate()).collect();
        assert_eq!(ids[0], NetId(1));
        assert!(ids.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn resume_continues_after_highest() {
        let mut alloc = NetIdAllocator::resume_after(NetId(41));
        assert_eq!(alloc.peek(), NetId(42));
        assert_eq!(alloc.allocate(), NetId(42));
        assert_eq!(alloc.allocate(), NetId(43));
    }
}
