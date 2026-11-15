//! What a person is doing, as a name both the planner (where people stand
//! and what is at hand), the simulation (who does what, when) and the
//! client (which clip to play) agree on.
//!
//! Nobody just stands: every place anyone is shown at has an [`Act`], and
//! the one standing there takes up a different one every minute or so (see
//! `hw_sim::citizens::Citizens::indoors`).

/// Something a person does while they stay where they are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Act {
    /// Typing at a desk.
    Type,
    /// Writing at a desk or a table.
    Write,
    /// Reading a book or papers, sitting.
    SitRead,
    /// A cup in the hand, sitting (a cup of tea, a meal).
    SitSip,
    /// Leaning back on a sofa, an armchair, a bench.
    SitRelax,
    /// Talking, sitting.
    SitTalk,
    /// Talking, standing, with the hands.
    Talk,
    /// On the telephone.
    Phone,
    /// A cup, a glass, a bottle in the hand.
    Sip,
    /// Reading, standing.
    Read,
    /// Looking over what a shelf, a rack or a case holds, reaching for it.
    Browse,
    /// Serving at a counter, a till, a stall.
    Serve,
    /// Cooking: stirring at a stove, working at a counter.
    Cook,
    /// Wiping a counter, a table, a surface.
    Wipe,
    /// Working with the hands at a bench or a machine.
    Tinker,
    /// Looking at an exhibit, a picture, the room, hands behind the back.
    Admire,
    /// Dancing.
    Dance,
    /// Leaning on a bar or a rail, arms folded.
    Lean,
}

impl Act {
    /// Every act.
    pub const ALL: [Act; 18] = [
        Act::Type,
        Act::Write,
        Act::SitRead,
        Act::SitSip,
        Act::SitRelax,
        Act::SitTalk,
        Act::Talk,
        Act::Phone,
        Act::Sip,
        Act::Read,
        Act::Browse,
        Act::Serve,
        Act::Cook,
        Act::Wipe,
        Act::Tinker,
        Act::Admire,
        Act::Dance,
        Act::Lean,
    ];

    /// Done sitting down (on a chair or a seat), not on the feet.
    pub fn seated(self) -> bool {
        matches!(
            self,
            Act::Type | Act::Write | Act::SitRead | Act::SitSip | Act::SitRelax | Act::SitTalk
        )
    }

    /// What else the same person takes up at the same place now and then,
    /// by `pick` (any number): the act itself most often.
    pub fn alternatives(self) -> &'static [Act] {
        match self {
            Act::Type => &[Act::Type, Act::Type, Act::Write, Act::SitRead, Act::SitSip],
            Act::Write => &[Act::Write, Act::Type, Act::SitRead, Act::SitSip],
            Act::SitRead => &[Act::SitRead, Act::SitSip, Act::SitRead],
            Act::SitSip => &[Act::SitSip, Act::SitTalk, Act::SitRead],
            Act::SitRelax => &[Act::SitRelax, Act::SitRead, Act::SitSip, Act::SitRelax],
            Act::SitTalk => &[Act::SitTalk, Act::SitTalk, Act::SitSip],
            Act::Talk => &[Act::Talk, Act::Talk, Act::Talk, Act::Sip],
            Act::Phone => &[Act::Phone, Act::Read, Act::Sip],
            Act::Sip => &[Act::Sip, Act::Talk, Act::Phone],
            Act::Read => &[Act::Read, Act::Phone, Act::Read, Act::Sip],
            Act::Browse => &[Act::Browse, Act::Browse, Act::Read, Act::Browse],
            Act::Serve => &[Act::Serve, Act::Serve, Act::Wipe, Act::Phone, Act::Serve],
            Act::Cook => &[Act::Cook, Act::Cook, Act::Wipe, Act::Sip],
            Act::Wipe => &[Act::Wipe, Act::Wipe, Act::Sip],
            Act::Tinker => &[Act::Tinker, Act::Tinker, Act::Read, Act::Tinker],
            Act::Admire => &[Act::Admire, Act::Read, Act::Admire, Act::Phone],
            Act::Dance => &[Act::Dance],
            Act::Lean => &[Act::Lean, Act::Sip, Act::Lean, Act::Talk],
        }
    }

    /// One of [`Self::alternatives`], by `pick`.
    pub fn variant(self, pick: u64) -> Act {
        let a = self.alternatives();
        a[(pick % a.len() as u64) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The alternatives of a seated act are seated, and of a standing act
    /// stand (nobody changes from sitting to standing by changing what
    /// they do), and every act is among its own.
    #[test]
    fn alternatives_keep_the_posture() {
        for a in Act::ALL {
            assert!(a.alternatives().contains(&a), "{a:?}");
            for b in a.alternatives() {
                assert_eq!(a.seated(), b.seated(), "{a:?} -> {b:?}");
            }
        }
    }
}
