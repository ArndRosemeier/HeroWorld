//! The power tray: which key fires which power. Twenty slots, 1 to 0 and
//! Alt+1 to Alt+0, filled with the hero's active powers. Strike keeps 1; a
//! power just bought takes the first free slot. The
//! player rearranges it by drag and drop: a power from the powers window (B)
//! onto a slot, a slot onto another (the two swap), or a slot off the tray
//! (it leaves; drag it back from B). Remembered in the profile on this PC.

use engine::input::Key;
use hw_net::{PowerId, Sheet};
use hw_sim::combat::Kit;
use hw_sim::powers;
use serde_json::Value;

/// Slots in the tray: two rows of ten.
pub const SLOTS: usize = 20;

/// The number keys in slot order (1 first, 0 last).
const DIGITS: [Key; 10] = [
    Key::Digit1,
    Key::Digit2,
    Key::Digit3,
    Key::Digit4,
    Key::Digit5,
    Key::Digit6,
    Key::Digit7,
    Key::Digit8,
    Key::Digit9,
    Key::Digit0,
];

/// The key a slot listens to; slots from 10 on also need Alt.
pub fn key(slot: usize) -> Key {
    DIGITS[slot % 10]
}

/// Does this slot need Alt held?
pub fn alt(slot: usize) -> bool {
    slot >= 10
}

/// The slot's label on the tray and in the powers window.
pub fn label(slot: usize) -> String {
    let d = (slot + 1) % 10;
    if alt(slot) {
        format!("Alt {d}")
    } else {
        d.to_string()
    }
}

/// Does this power go on the tray (it is fired with a key)?
pub fn keyed(p: PowerId) -> bool {
    powers::get(p).is_some_and(|d| !d.passive())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tray {
    pub slots: [Option<PowerId>; SLOTS],
    /// Powers the tray has seen bought: only new ones are put on by
    /// themselves, so one dragged off stays off.
    pub known: Vec<PowerId>,
}

impl Default for Tray {
    fn default() -> Self {
        Self {
            slots: [None; SLOTS],
            known: Vec::new(),
        }
    }
}

/// What is being dragged onto the tray.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drag {
    /// A power from the powers window.
    Power(PowerId),
    /// The power in a tray slot.
    Slot(usize),
}

/// A change made on the tray with the mouse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// Something dropped on a slot.
    Drop(Drag, usize),
    /// A slot dragged off the tray and let go.
    Clear(usize),
}

impl Tray {
    /// The slot a power sits in.
    pub fn slot_of(&self, p: PowerId) -> Option<usize> {
        self.slots.iter().position(|s| *s == Some(p))
    }

    /// Brings the tray in line with the hero file: powers given up leave it,
    /// new ones take their usual key or the first free slot. True if it
    /// changed.
    pub fn sync(&mut self, sheet: &Sheet) -> bool {
        let kit = Kit::of(sheet);
        let before = (self.slots, self.known.clone());
        // Given up: forgotten, so buying it again puts it back on.
        self.known.retain(|p| kit.has(*p));
        // Gone (or never keyed, from an older or edited file): off the tray,
        // and each power only once.
        let mut seen = Vec::new();
        for s in self.slots.iter_mut() {
            if let Some(p) = *s {
                if !kit.has(p) || !keyed(p) || seen.contains(&p) {
                    *s = None;
                } else {
                    seen.push(p);
                }
            }
        }
        // Strike first, the portal's two buttons, then the rest in the
        // order they were bought.
        let owned = PowerId::FREE
            .into_iter()
            .chain(sheet.powers.iter().map(|o| o.power));
        for p in owned {
            if !keyed(p) || self.known.contains(&p) {
                continue;
            }
            self.known.push(p);
            if self.slot_of(p).is_some() {
                continue;
            }
            // Strike keeps 1; the portal's buttons go to 9 and 0, out of
            // the way of the fighting powers.
            let usual = match p {
                PowerId::STRIKE => Some(0),
                PowerId::GATE_MARK => Some(8),
                PowerId::GATE_OPEN => Some(9),
                _ => None,
            };
            let free = usual
                .filter(|&u| self.slots[u].is_none())
                .or_else(|| self.slots.iter().position(|s| s.is_none()));
            if let Some(f) = free {
                self.slots[f] = Some(p);
            }
        }
        (self.slots, self.known.clone()) != before
    }

    /// Applies a drag and drop.
    pub fn apply(&mut self, c: Change) {
        match c {
            Change::Drop(Drag::Power(p), to) => self.put(p, to),
            Change::Drop(Drag::Slot(from), to) => {
                if from < SLOTS && to < SLOTS {
                    self.slots.swap(from, to);
                }
            }
            Change::Clear(n) => {
                if n < SLOTS {
                    self.slots[n] = None;
                }
            }
        }
    }

    /// Moves a power to a slot; what was there takes the power's old slot.
    pub fn put(&mut self, p: PowerId, slot: usize) {
        if slot >= SLOTS {
            return;
        }
        let from = self.slot_of(p);
        let there = self.slots[slot];
        self.slots[slot] = Some(p);
        if let Some(f) = from.filter(|&f| f != slot) {
            self.slots[f] = there;
        }
    }

    /// The powers on the tray with their slots, in slot order.
    pub fn filled(&self) -> impl Iterator<Item = (usize, PowerId)> + '_ {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(n, s)| s.map(|p| (n, p)))
    }

    /// Does the second row (Alt) hold anything?
    pub fn second_row(&self) -> bool {
        self.slots[10..].iter().any(|s| s.is_some())
    }

    pub fn to_json(&self) -> Value {
        let code = |p: Option<PowerId>| Value::from(p.map_or(0, |p| p.0));
        serde_json::json!({
            "slots": self.slots.iter().map(|s| code(*s)).collect::<Vec<_>>(),
            "known": self.known.iter().map(|p| code(Some(*p))).collect::<Vec<_>>(),
        })
    }

    /// A tray from the profile; anything odd is left empty (`sync` fills it).
    pub fn from_json(v: &Value) -> Self {
        let power = |c: &Value| {
            c.as_u64()
                .and_then(|c| u8::try_from(c).ok())
                .map(PowerId)
                .filter(|&p| powers::get(p).is_some())
        };
        let mut t = Tray::default();
        if let Some(a) = v["slots"].as_array() {
            for (n, c) in a.iter().take(SLOTS).enumerate() {
                t.slots[n] = power(c);
            }
        }
        if let Some(a) = v["known"].as_array() {
            t.known = a.iter().filter_map(power).collect();
        }
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hw_net::OwnedPower;

    fn sheet(powers: &[PowerId]) -> Sheet {
        let mut s = hw_sim::progress::starter();
        s.powers = powers
            .iter()
            .map(|&power| OwnedPower { power, paid: 0 })
            .collect();
        s
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn strike_first_then_the_powers_in_order() {
        let mut t = Tray::default();
        t.sync(&sheet(&[PowerId(10), PowerId(30)]));
        assert_eq!(t.slots[0], Some(PowerId::STRIKE));
        assert_eq!(t.slots[1], Some(PowerId(10)));
        assert_eq!(t.slots[2], Some(PowerId(30)));
        assert_eq!(t.slot_of(PowerId(2)), None, "passive powers need no key");
        // Every hero's portal sits on 9 and 0.
        assert_eq!(t.slots[8], Some(PowerId::GATE_MARK));
        assert_eq!(t.slots[9], Some(PowerId::GATE_OPEN));
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn moving_swaps_and_giving_up_frees_the_slot() {
        let mut t = Tray::default();
        t.sync(&sheet(&[PowerId(10), PowerId(13)]));
        t.put(PowerId(13), 1);
        assert_eq!(t.slots[1], Some(PowerId(13)));
        assert_eq!(t.slots[2], Some(PowerId(10)));
        t.put(PowerId(10), 15);
        assert_eq!(t.slots[2], None);
        assert!(t.second_row());
        // Dragged off, it stays off.
        t.apply(Change::Clear(15));
        assert!(!t.sync(&sheet(&[PowerId(10), PowerId(13)])));
        assert_eq!(t.slot_of(PowerId(10)), None);
        t.apply(Change::Drop(Drag::Power(PowerId(10)), 15));
        t.apply(Change::Drop(Drag::Slot(15), 1));
        assert_eq!(t.slots[1], Some(PowerId(10)));
        assert_eq!(t.slots[15], Some(PowerId(13)));
        assert!(t.sync(&sheet(&[PowerId(13)])));
        assert_eq!(t.slot_of(PowerId(10)), None);
        // Bought again: back on, in the first free slot.
        t.sync(&sheet(&[PowerId(13), PowerId(10)]));
        assert_eq!(t.slot_of(PowerId(10)), Some(1));
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn survives_the_profile() {
        let mut t = Tray::default();
        t.sync(&sheet(&[PowerId(10), PowerId(21)]));
        t.put(PowerId(21), 12);
        let back = Tray::from_json(&t.to_json());
        assert_eq!(back, t);
        assert_eq!(label(12), "Alt 3");
        assert_eq!(label(9), "0");
    }
}
