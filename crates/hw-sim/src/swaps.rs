//! Swapping augments between two heroes.
//!
//! Two heroes who stand together each lay augments from their bags on the
//! table; when both say yes to the same table, the augments change bags in
//! one step. The server keeps the table: a change to it takes back both
//! heroes' yes and raises its count, and a yes names the count it was
//! said to, so nobody says yes to something they have not seen. Augments
//! set into sockets are for good and never swapped.
//!
//! Either hero may also put back what the other laid down ("not that
//! one"), so a swap can be for a selection of what is offered.
//!
//! A Herobot never opens a swap. Asked to one, it lays its whole bag on
//! the table (what it could use itself it has set into its powers long
//! since) and says yes to whatever the player makes of it.

use crate::herobots::BotHero;
use crate::{Hero, Sim};
use hw_net::{Augment, PlayerId, Sheet, Swap, SwapState, MAX_BAG, SWAP_MOST};

/// Two heroes swap within this (m)...
pub const REACH: f64 = 12.0;
/// ...and an open swap ends when they are further apart than this.
pub const LEASH: f64 = 20.0;

#[derive(Clone, Debug, PartialEq)]
struct Table {
    /// The two heroes, and what each gives.
    who: [PlayerId; 2],
    give: [Vec<Augment>; 2],
    yes: [bool; 2],
    rev: u16,
}

impl Table {
    fn side(&self, p: PlayerId) -> Option<usize> {
        self.who.iter().position(|w| *w == p)
    }

    fn seen_by(&self, side: usize, state: SwapState) -> Swap {
        let other = 1 - side;
        Swap {
            with: self.who[other],
            rev: self.rev,
            state,
            mine: self.give[side].clone(),
            theirs: self.give[other].clone(),
            my_yes: self.yes[side],
            their_yes: self.yes[other],
        }
    }

    fn changed(&mut self) {
        self.rev = self.rev.wrapping_add(1);
        self.yes = [false; 2];
    }
}

/// The swaps going on, and what to tell their heroes.
#[derive(Default)]
pub struct Swaps {
    tables: Vec<Table>,
    news: Vec<(PlayerId, Swap)>,
}

impl Swaps {
    fn of(&self, p: PlayerId) -> Option<usize> {
        self.tables.iter().position(|t| t.side(p).is_some())
    }

    fn tell(&mut self, i: usize, state: SwapState) {
        let t = &self.tables[i];
        for side in 0..2 {
            self.news.push((t.who[side], t.seen_by(side, state)));
        }
    }

    /// Ends table `i`, telling both why.
    fn end(&mut self, i: usize, state: SwapState) {
        self.tell(i, state);
        self.tables.remove(i);
    }

    /// Tells `p` alone that a swap with `with` is not to be had.
    fn refuse(&mut self, p: PlayerId, with: PlayerId, state: SwapState) {
        self.news.push((
            p,
            Swap {
                with,
                rev: 0,
                state,
                mine: Vec::new(),
                theirs: Vec::new(),
                my_yes: false,
                their_yes: false,
            },
        ));
    }

    /// How many swaps are open.
    pub fn open(&self) -> usize {
        self.tables.len()
    }
}

/// Is every augment of `give` in `bag`, counted (two alike need two)?
pub fn holds(bag: &[Augment], give: &[Augment]) -> bool {
    let mut left = bag.to_vec();
    give.iter().all(|a| {
        left.iter()
            .position(|b| b == a)
            .map(|i| left.swap_remove(i))
            .is_some()
    })
}

/// `give` without what `bag` no longer holds.
fn still_held(bag: &[Augment], give: &[Augment]) -> Vec<Augment> {
    let mut left = bag.to_vec();
    give.iter()
        .filter(|a| {
            left.iter()
                .position(|b| b == *a)
                .map(|i| left.swap_remove(i))
                .is_some()
        })
        .copied()
        .collect()
}

/// Takes `give` out of the bag and puts `get` in.
fn exchange(sheet: &mut Sheet, give: &[Augment], get: &[Augment]) {
    for a in give {
        if let Some(i) = sheet.bag.iter().position(|b| b == a) {
            sheet.bag.remove(i);
        }
    }
    sheet.bag.extend_from_slice(get);
}

impl Sim {
    fn swap_place(&mut self, p: PlayerId) -> Option<glam::DVec3> {
        self.hero_positions()
            .into_iter()
            .find(|(q, _)| *q == p)
            .map(|(_, at)| at)
    }

    fn swap_apart(&mut self, a: PlayerId, b: PlayerId) -> Option<f64> {
        Some(self.swap_place(a)?.distance(self.swap_place(b)?))
    }

    fn is_herobot(&mut self, p: PlayerId) -> bool {
        self.world
            .query::<(&Hero, &BotHero)>()
            .iter(&self.world)
            .any(|(h, _)| h.owner == p)
    }

    /// `from` lays `give` (augments from their bag) on the table of a swap
    /// with `with`, opening it if there is none. Both heroes' yes is taken
    /// back.
    pub fn swap_offer(&mut self, from: PlayerId, with: PlayerId, give: Vec<Augment>) {
        if from == with {
            return;
        }
        let mine = self.swaps.of(from);
        if let Some(i) = mine {
            if self.swaps.tables[i].side(with).is_none() {
                // Swapping with someone else now: the old table is off.
                self.swaps.end(i, SwapState::Off);
            }
        }
        let i = match self.swaps.of(from) {
            Some(i) => i,
            None => {
                let state = match self.swap_apart(from, with) {
                    None => Some(SwapState::Busy),
                    Some(_) if self.swaps.of(with).is_some() => Some(SwapState::Busy),
                    Some(d) if d > REACH => Some(SwapState::Far),
                    Some(_) => None,
                };
                if let Some(state) = state {
                    self.swaps.refuse(from, with, state);
                    return;
                }
                // A Herobot lays out all it has.
                let theirs = if self.is_herobot(with) {
                    let mut bag = self.sheet(with).map(|s| s.bag).unwrap_or_default();
                    bag.truncate(SWAP_MOST);
                    bag
                } else {
                    Vec::new()
                };
                self.swaps.tables.push(Table {
                    who: [from, with],
                    give: [Vec::new(), theirs],
                    yes: [false; 2],
                    rev: 0,
                });
                self.swaps.tables.len() - 1
            }
        };
        let bag = self.sheet(from).map(|s| s.bag).unwrap_or_default();
        let ok = give.len() <= SWAP_MOST && holds(&bag, &give);
        let t = &mut self.swaps.tables[i];
        let side = t.side(from).unwrap_or(0);
        if ok && t.give[side] != give {
            t.give[side] = give;
            t.changed();
        }
        // Told either way: a refused change shows the table as it stands.
        self.herobots_say_yes(i);
        self.swaps.tell(i, SwapState::Open);
    }

    /// A Herobot at table `i` is content with it as it stands.
    fn herobots_say_yes(&mut self, i: usize) {
        for side in 0..2 {
            let who = self.swaps.tables[i].who[side];
            if self.is_herobot(who) {
                self.swaps.tables[i].yes[side] = true;
            }
        }
    }

    /// `from` puts `augment` back: off the other hero's side of the table
    /// (it stays theirs). Both heroes' yes is taken back.
    pub fn swap_put_back(&mut self, from: PlayerId, augment: Augment) {
        let Some(i) = self.swaps.of(from) else {
            return;
        };
        let t = &mut self.swaps.tables[i];
        let other = 1 - t.side(from).unwrap_or(0);
        if let Some(k) = t.give[other].iter().position(|a| *a == augment) {
            t.give[other].remove(k);
            t.changed();
        }
        self.herobots_say_yes(i);
        self.swaps.tell(i, SwapState::Open);
    }

    /// `from` says yes to their swap as it stood at count `rev`. With
    /// both heroes' yes the augments change bags.
    pub fn swap_yes(&mut self, from: PlayerId, rev: u16) {
        let Some(i) = self.swaps.of(from) else {
            return;
        };
        let t = &mut self.swaps.tables[i];
        let side = t.side(from).unwrap_or(0);
        if t.rev != rev {
            // Said to a table that has changed since: it does not count.
            return;
        }
        t.yes[side] = true;
        if !(t.yes[0] && t.yes[1]) {
            self.swaps.tell(i, SwapState::Open);
            return;
        }
        let t = t.clone();
        let bags = [
            self.sheet(t.who[0]).map(|s| s.bag),
            self.sheet(t.who[1]).map(|s| s.bag),
        ];
        let [Some(a), Some(b)] = bags else {
            self.swaps.end(i, SwapState::Off);
            return;
        };
        if !holds(&a, &t.give[0]) || !holds(&b, &t.give[1]) {
            // Something laid on the table is gone: `mind_swaps` takes it
            // off the table next tick; nothing changes hands now.
            let t = &mut self.swaps.tables[i];
            t.yes = [false; 2];
            self.herobots_say_yes(i);
            self.swaps.tell(i, SwapState::Open);
            return;
        }
        let room = |bag: &[Augment], out: usize, into: usize| bag.len() - out + into <= MAX_BAG;
        if !room(&a, t.give[0].len(), t.give[1].len())
            || !room(&b, t.give[1].len(), t.give[0].len())
        {
            let t = &mut self.swaps.tables[i];
            t.changed();
            self.herobots_say_yes(i);
            self.swaps.tell(i, SwapState::Full);
            return;
        }
        if t.give[0].is_empty() && t.give[1].is_empty() {
            self.swaps.end(i, SwapState::Off);
            return;
        }
        self.change_sheet(t.who[0], |s| exchange(s, &t.give[0], &t.give[1]));
        self.change_sheet(t.who[1], |s| exchange(s, &t.give[1], &t.give[0]));
        self.swaps.end(i, SwapState::Done);
    }

    /// `from` calls their swap off (or has left the world).
    pub fn swap_off(&mut self, from: PlayerId) {
        if let Some(i) = self.swaps.of(from) {
            self.swaps.end(i, SwapState::Off);
        }
    }

    /// Every tick: a swap ends when its heroes part or one is gone, and
    /// an augment that has left its bag (set into a socket, traded in)
    /// leaves the table.
    pub(crate) fn mind_swaps(&mut self) {
        let mut i = 0;
        while i < self.swaps.tables.len() {
            let who = self.swaps.tables[i].who;
            let end = match self.swap_apart(who[0], who[1]) {
                None => Some(SwapState::Off),
                Some(d) if d > LEASH => Some(SwapState::Far),
                Some(_) => None,
            };
            if let Some(state) = end {
                self.swaps.end(i, state);
                continue;
            }
            let mut changed = false;
            for (side, hero) in who.into_iter().enumerate() {
                let bag = self.sheet(hero).map(|s| s.bag).unwrap_or_default();
                let t = &mut self.swaps.tables[i];
                let kept = still_held(&bag, &t.give[side]);
                if kept != t.give[side] {
                    t.give[side] = kept;
                    changed = true;
                }
            }
            if changed {
                self.swaps.tables[i].changed();
                self.herobots_say_yes(i);
                self.swaps.tell(i, SwapState::Open);
            }
            i += 1;
        }
    }

    /// What to tell which player about their swap, since the last call.
    pub fn take_swap_news(&mut self) -> Vec<(PlayerId, Swap)> {
        std::mem::take(&mut self.swaps.news)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(effect: u8) -> Augment {
        Augment {
            effect,
            percent: 15,
            grade: 0,
        }
    }

    #[test]
    fn what_is_given_must_be_in_the_bag_as_often() {
        let bag = [a(1), a(2), a(1)];
        assert!(holds(&bag, &[a(1), a(1)]));
        assert!(holds(&bag, &[]));
        assert!(!holds(&bag, &[a(2), a(2)]));
        assert!(!holds(&bag, &[a(3)]));
        assert_eq!(still_held(&bag, &[a(2), a(2), a(1)]), vec![a(2), a(1)]);
    }

    #[test]
    fn an_exchange_takes_out_and_puts_in() {
        let mut s = Sheet {
            bag: vec![a(1), a(2), a(1)],
            ..Default::default()
        };
        exchange(&mut s, &[a(1)], &[a(7), a(8)]);
        assert_eq!(s.bag, vec![a(2), a(1), a(7), a(8)]);
    }
}
