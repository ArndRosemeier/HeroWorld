//! What each client gets told about the world, and how a client rebuilds it.
//!
//! - [`ClientView`] (server side, one per client): which entities the client
//!   has confirmed in which state. Every tick it picks the entities that are
//!   near the player (the game says which, with a priority) and changed since
//!   the client last confirmed them, most urgent first, up to a byte budget.
//!   Entities that leave the player's range are removed on the client.
//! - [`AckWindow`] (client side): which updates arrived, sent back with every
//!   input.
//! - [`Mirror`] (client side): the world as the updates describe it. Each
//!   update carries whole entity states, so any update can be applied on its
//!   own and in any order; older states never overwrite newer ones.

use crate::protocol::EntityState;
use hw_core::NetId;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// How many earlier ticks an ack can confirm besides the newest.
pub const ACK_WINDOW: u64 = 32;

/// An entity the game wants a client to know about.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    pub state: EntityState,
    /// The tick the entity last changed.
    pub changed: u64,
    /// Lower goes first (distance to the player, 0 for the player's hero).
    pub priority: f64,
}

#[derive(Clone, Debug)]
struct Sent {
    tick: u64,
    entities: Vec<(NetId, u64)>,
    removed: Vec<NetId>,
}

/// The server's record of one client's copy of the world.
#[derive(Clone, Debug, Default)]
pub struct ClientView {
    /// The change tick of the newest state the client confirmed, per entity.
    acked: BTreeMap<NetId, u64>,
    /// Entities sent to the client and not removed since.
    known: BTreeSet<NetId>,
    last_sent: BTreeMap<NetId, u64>,
    /// Removals not confirmed yet, with the tick they started.
    removing: BTreeMap<NetId, u64>,
    history: VecDeque<Sent>,
}

impl ClientView {
    pub fn new() -> Self {
        Self::default()
    }

    /// The client confirmed the update of `tick` and those of the ticks
    /// before it whose bits are set (bit 0 = `tick - 1`).
    pub fn on_ack(&mut self, tick: u64, bits: u32) {
        let confirmed = |t: u64| {
            t == tick || (t < tick && tick - t <= ACK_WINDOW && bits >> (tick - t - 1) & 1 == 1)
        };
        let mut keep = VecDeque::new();
        for sent in self.history.drain(..) {
            if confirmed(sent.tick) {
                for (id, changed) in sent.entities {
                    if self.known.contains(&id) {
                        let a = self.acked.entry(id).or_insert(changed);
                        *a = (*a).max(changed);
                    }
                }
                for id in sent.removed {
                    if self.removing.get(&id).is_some_and(|&t| t <= sent.tick) {
                        self.removing.remove(&id);
                    }
                }
            } else if sent.tick + ACK_WINDOW >= tick {
                // Not confirmed yet, but may still be.
                keep.push_back(sent);
            }
        }
        self.history = keep;
    }

    /// The update for `tick`: the candidates the client needs, most urgent
    /// first, and the entities it should forget, within `budget` bytes (for
    /// entities and removals together). Candidates must have distinct IDs.
    pub fn build(
        &mut self,
        tick: u64,
        candidates: &[Candidate],
        budget: usize,
    ) -> (Vec<EntityState>, Vec<NetId>) {
        let visible: BTreeSet<NetId> = candidates.iter().map(|c| c.state.id).collect();
        let gone: Vec<NetId> = self.known.difference(&visible).copied().collect();
        for id in gone {
            self.known.remove(&id);
            self.acked.remove(&id);
            self.last_sent.remove(&id);
            self.removing.insert(id, tick);
            for sent in &mut self.history {
                sent.entities.retain(|(e, _)| *e != id);
            }
        }
        let mut wanted: Vec<(f64, &Candidate)> = Vec::new();
        for c in candidates {
            let id = c.state.id;
            self.removing.remove(&id);
            if self.acked.get(&id).is_some_and(|&a| a >= c.changed) {
                continue;
            }
            let age = self
                .last_sent
                .get(&id)
                .map_or(1_000.0, |&t| tick.saturating_sub(t) as f64);
            wanted.push((c.priority.max(0.0) / (1.0 + age), c));
        }
        wanted.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.state.id.cmp(&b.1.state.id)));

        let mut room = budget;
        let mut removed = Vec::new();
        for &id in self.removing.keys() {
            if room < crate::protocol::REMOVED_BYTES {
                break;
            }
            room -= crate::protocol::REMOVED_BYTES;
            removed.push(id);
        }
        let mut entities = Vec::new();
        let mut record = Vec::new();
        for (_, c) in wanted {
            let size = c.state.wire_size();
            if size > room {
                continue;
            }
            room -= size;
            entities.push(c.state);
            record.push((c.state.id, c.changed));
            self.known.insert(c.state.id);
            self.last_sent.insert(c.state.id, tick);
        }
        self.history.push_back(Sent {
            tick,
            entities: record,
            removed: removed.clone(),
        });
        while self.history.len() > ACK_WINDOW as usize + 2 {
            self.history.pop_front();
        }
        (entities, removed)
    }

    /// Entities the client has been sent and not told to forget.
    pub fn known(&self) -> usize {
        self.known.len()
    }
}

/// The client's record of which updates arrived.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AckWindow {
    pub newest: u64,
    pub bits: u32,
}

impl AckWindow {
    pub fn record(&mut self, tick: u64) {
        if tick > self.newest {
            let shift = tick - self.newest;
            self.bits = if shift > 32 {
                0
            } else {
                let moved = if shift == 32 { 0 } else { self.bits << shift };
                // The old newest becomes bit shift - 1 (if there was one).
                if self.newest > 0 {
                    moved | 1 << (shift - 1)
                } else {
                    moved
                }
            };
            self.newest = tick;
        } else if tick < self.newest && self.newest - tick <= ACK_WINDOW {
            self.bits |= 1 << (self.newest - tick - 1);
        }
    }
}

/// How long a removed entity's ID blocks late, older states (ticks).
const TOMBSTONE_TICKS: u64 = 90;

/// The client's copy of the world near its player.
#[derive(Clone, Debug, Default)]
pub struct Mirror {
    entities: BTreeMap<NetId, (u64, EntityState)>,
    removed: BTreeMap<NetId, u64>,
    /// Newest update tick applied.
    pub tick: u64,
}

impl Mirror {
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies the update of `tick`.
    pub fn apply(&mut self, tick: u64, entities: &[EntityState], removed: &[NetId]) {
        for id in removed {
            if self.entities.get(id).is_some_and(|(t, _)| *t < tick) {
                self.entities.remove(id);
            }
            let r = self.removed.entry(*id).or_insert(tick);
            *r = (*r).max(tick);
        }
        for e in entities {
            if self.removed.get(&e.id).is_some_and(|&r| r >= tick) {
                continue;
            }
            if self.entities.get(&e.id).is_some_and(|(t, _)| *t >= tick) {
                continue;
            }
            self.removed.remove(&e.id);
            self.entities.insert(e.id, (tick, *e));
        }
        self.tick = self.tick.max(tick);
        let now = self.tick;
        self.removed.retain(|_, t| *t + TOMBSTONE_TICKS > now);
    }

    /// Every entity, sorted by ID.
    pub fn states(&self) -> Vec<EntityState> {
        self.entities.values().map(|(_, e)| *e).collect()
    }

    pub fn get(&self, id: NetId) -> Option<EntityState> {
        self.entities.get(&id).map(|(_, e)| *e)
    }

    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(id: u32, x: f64) -> EntityState {
        EntityState {
            id: NetId(id),
            pos: [x, 0.0, 0.0],
            ..Default::default()
        }
    }

    fn cand(id: u32, x: f64, changed: u64) -> Candidate {
        Candidate {
            state: at(id, x),
            changed,
            priority: x.abs(),
        }
    }

    #[test]
    fn ack_window_marks_earlier_ticks() {
        let mut w = AckWindow::default();
        w.record(10);
        w.record(12);
        w.record(11);
        w.record(9);
        assert_eq!(w.newest, 12);
        // 11 -> bit 0, 10 -> bit 1, 9 -> bit 2.
        assert_eq!(w.bits, 0b111);
        w.record(50);
        assert_eq!(w.bits, 0);
    }

    #[test]
    fn unchanged_entities_stop_once_confirmed() {
        let mut view = ClientView::new();
        let c = [cand(1, 0.0, 5), cand(2, 10.0, 5)];
        let (e, _) = view.build(5, &c, 1000);
        assert_eq!(e.len(), 2);
        // Not confirmed yet: sent again.
        let (e, _) = view.build(6, &c, 1000);
        assert_eq!(e.len(), 2);
        view.on_ack(6, 0b1);
        let (e, _) = view.build(7, &c, 1000);
        assert!(e.is_empty());
        // Entity 2 moves: only it goes out.
        let c = [cand(1, 0.0, 5), cand(2, 11.0, 8)];
        let (e, _) = view.build(8, &c, 1000);
        assert_eq!(e.iter().map(|e| e.id.0).collect::<Vec<_>>(), vec![2]);
    }

    #[test]
    fn leaving_range_removes_until_confirmed() {
        let mut view = ClientView::new();
        view.build(1, &[cand(1, 0.0, 1), cand(2, 5.0, 1)], 1000);
        view.on_ack(1, 0);
        let (_, r) = view.build(2, &[cand(1, 0.0, 1)], 1000);
        assert_eq!(r, vec![NetId(2)]);
        let (_, r) = view.build(3, &[cand(1, 0.0, 1)], 1000);
        assert_eq!(r, vec![NetId(2)]);
        view.on_ack(3, 0);
        let (_, r) = view.build(4, &[cand(1, 0.0, 1)], 1000);
        assert!(r.is_empty());
        // Coming back sends it whole again, though it did not change.
        let (e, _) = view.build(5, &[cand(1, 0.0, 1), cand(2, 5.0, 1)], 1000);
        assert_eq!(e.iter().map(|e| e.id.0).collect::<Vec<_>>(), vec![2]);
    }

    #[test]
    fn a_tight_budget_sends_the_nearest_first_and_starves_nobody() {
        let mut view = ClientView::new();
        let size = at(1, 0.0).wire_size();
        let c: Vec<Candidate> = (0..10).map(|i| cand(i, i as f64 * 10.0, 1)).collect();
        let mut seen = BTreeSet::new();
        let (e, _) = view.build(1, &c, size * 3);
        assert_eq!(
            e.iter().map(|e| e.id.0).collect::<Vec<_>>(),
            vec![0, 1, 2],
            "nearest first"
        );
        for t in 2..12 {
            // Everything keeps changing; the far ones still get their turn.
            let c: Vec<Candidate> = (0..10).map(|i| cand(i, i as f64 * 10.0, t)).collect();
            let (e, _) = view.build(t, &c, size * 3);
            seen.extend(e.iter().map(|e| e.id.0));
        }
        assert_eq!(seen.len(), 10);
    }

    #[test]
    fn mirror_ignores_late_and_stale_states() {
        let mut m = Mirror::new();
        m.apply(5, &[at(1, 5.0)], &[]);
        m.apply(4, &[at(1, 4.0)], &[]);
        assert_eq!(m.get(NetId(1)).unwrap().pos[0], 5.0);
        m.apply(7, &[], &[NetId(1)]);
        assert!(m.is_empty());
        // A packet from before the removal arrives late: ignored.
        m.apply(6, &[at(1, 6.0)], &[]);
        assert!(m.is_empty());
        m.apply(9, &[at(1, 9.0)], &[]);
        assert_eq!(m.get(NetId(1)).unwrap().pos[0], 9.0);
    }

    /// The server's view and a client's mirror over a lossy, reordering
    /// link: once the link calms down, the mirror equals the truth.
    #[test]
    fn view_and_mirror_agree_over_a_bad_link() {
        let mut view = ClientView::new();
        let mut mirror = Mirror::new();
        let mut acks = AckWindow::default();
        let mut in_flight: Vec<(u64, Vec<EntityState>, Vec<NetId>)> = Vec::new();
        let mut truth: Vec<Candidate> = Vec::new();
        for tick in 1..400u64 {
            // Entities appear, move now and then, and go away.
            truth = (0..20u32)
                .filter(|i| tick < 300 || i % 3 != 0)
                .filter(|i| !(tick / 50 + *i as u64).is_multiple_of(4) || tick >= 300)
                .map(|i| {
                    let moving = tick < 300 && (tick + i as u64).is_multiple_of(7);
                    let x = if tick < 300 {
                        (i as u64 * 10 + tick / 7) as f64
                    } else {
                        i as f64 * 10.0 + 299.0 / 7.0
                    };
                    let prev = truth.iter().find(|c| c.state.id.0 == i);
                    let changed = match prev {
                        Some(p) if p.state.pos[0] == x && !moving => p.changed,
                        _ => tick,
                    };
                    Candidate {
                        state: at(i, x),
                        changed,
                        priority: i as f64,
                    }
                })
                .collect();
            let (e, r) = view.build(tick, &truth, 300);
            in_flight.push((tick, e, r));
            // Deliver some packets, out of order, losing every fifth.
            if tick % 3 == 0 || tick > 350 {
                in_flight.reverse();
                for (t, e, r) in in_flight.drain(..) {
                    if tick < 350 && t % 5 == 0 {
                        continue;
                    }
                    mirror.apply(t, &e, &r);
                    acks.record(t);
                }
                view.on_ack(acks.newest, acks.bits);
            }
        }
        let want: Vec<EntityState> = {
            let mut v: Vec<EntityState> = truth.iter().map(|c| c.state).collect();
            v.sort_by_key(|e| e.id);
            v
        };
        assert_eq!(mirror.states(), want);
    }
}
