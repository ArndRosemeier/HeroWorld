//! A reliable, ordered message channel on top of unreliable packets.
//!
//! The sender numbers each message and repeats it (piggybacked on the
//! packets that go out every tick anyway) until the receiver confirms
//! everything below some number. The receiver hands messages out in order,
//! each exactly once.

use std::collections::{BTreeMap, VecDeque};

/// Most messages waiting at once; a peer that never confirms gets dropped
/// by its timeout long before this fills.
pub const MAX_PENDING: usize = 1024;

#[derive(Clone, Debug)]
pub struct ReliableSender<T> {
    next: u32,
    /// Unconfirmed messages with the tick they last went out.
    pending: VecDeque<(u32, T, Option<u64>)>,
}

impl<T> Default for ReliableSender<T> {
    fn default() -> Self {
        Self {
            next: 0,
            pending: VecDeque::new(),
        }
    }
}

impl<T: Clone> ReliableSender<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues a message. Returns false if too much is unconfirmed already.
    pub fn push(&mut self, msg: T) -> bool {
        if self.pending.len() >= MAX_PENDING {
            return false;
        }
        self.pending.push_back((self.next, msg, None));
        self.next = self.next.wrapping_add(1);
        true
    }

    /// The receiver has everything below `next_expected`.
    pub fn ack(&mut self, next_expected: u32) {
        while self
            .pending
            .front()
            .is_some_and(|(id, _, _)| (next_expected.wrapping_sub(*id) as i32) > 0)
        {
            self.pending.pop_front();
        }
    }

    /// Messages to put in the packet going out at `tick`: those never sent,
    /// and those not confirmed within `resend_after` ticks.
    pub fn due(&mut self, tick: u64, resend_after: u64) -> Vec<(u32, T)> {
        let mut out = Vec::new();
        for (id, msg, sent) in self.pending.iter_mut() {
            if sent.is_none_or(|s| tick >= s + resend_after) {
                *sent = Some(tick);
                out.push((*id, msg.clone()));
            }
        }
        out
    }

    /// Messages not yet confirmed.
    pub fn unconfirmed(&self) -> usize {
        self.pending.len()
    }
}

#[derive(Clone, Debug)]
pub struct ReliableReceiver<T> {
    next: u32,
    early: BTreeMap<u32, T>,
}

impl<T> Default for ReliableReceiver<T> {
    fn default() -> Self {
        Self {
            next: 0,
            early: BTreeMap::new(),
        }
    }
}

impl<T> ReliableReceiver<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes a message from a packet and returns the ones now deliverable,
    /// in order.
    pub fn receive(&mut self, id: u32, msg: T) -> Vec<T> {
        let ahead = id.wrapping_sub(self.next);
        if ahead as i32 >= 0 && (ahead as usize) < MAX_PENDING {
            self.early.entry(id).or_insert(msg);
        }
        let mut out = Vec::new();
        while let Some(m) = self.early.remove(&self.next) {
            out.push(m);
            self.next = self.next.wrapping_add(1);
        }
        out
    }

    /// What to confirm: everything below this has arrived.
    pub fn ack(&self) -> u32 {
        self.next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivers_in_order_once_despite_loss_and_reordering() {
        let mut tx = ReliableSender::new();
        let mut rx = ReliableReceiver::new();
        for i in 0..20u32 {
            tx.push(i);
        }
        let mut got = Vec::new();
        // A lossy link: every third packet is lost.
        for tick in 0..100u64 {
            let mut packet = tx.due(tick, 4);
            if tick % 3 == 0 {
                continue;
            }
            // A packet holds five messages, the oldest first; they arrive reversed.
            packet.truncate(5);
            packet.reverse();
            for (id, m) in packet {
                got.extend(rx.receive(id, m));
            }
            tx.ack(rx.ack());
        }
        assert_eq!(got, (0..20).collect::<Vec<_>>());
        assert_eq!(tx.unconfirmed(), 0);
    }

    #[test]
    fn waits_before_resending() {
        let mut tx = ReliableSender::new();
        tx.push("a");
        assert_eq!(tx.due(10, 5).len(), 1);
        assert!(tx.due(11, 5).is_empty());
        assert_eq!(tx.due(15, 5).len(), 1);
        tx.ack(1);
        assert!(tx.due(30, 5).is_empty());
    }
}
