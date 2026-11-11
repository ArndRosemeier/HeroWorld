//! What the street acts say (see `streetview`): their own patter, and a
//! line for a hero who comes close. Golden-age comic talk: saucers, the
//! Aether, the Doctor, the mole folk, the rocket port, the sky pirates.

use hw_sim::streetlife::Act;

/// What someone says, by moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Moment {
    /// Patter for whoever is near.
    Own,
    /// A hero walks up.
    Hero,
    /// The living statue lunges at someone who came too close.
    Boo,
    /// Packing up.
    Leave,
    /// Trouble near: the act breaks up.
    Panic,
}

/// The lines of `act` for `moment` (empty: nothing to say).
pub fn lines(act: Act, moment: Moment) -> &'static [&'static str] {
    use Moment::*;
    match (act, moment) {
        (Act::Prophet, Own) => &[
            "The saucers are coming! Repent!",
            "They read your thoughts through the Aether wires!",
            "The mole folk know! Ask the mole folk!",
            "The Doctor is only the beginning!",
            "Look up! LOOK UP! They are watching!",
            "The rocket port is a beacon! A BEACON for them!",
            "Your toaster is listening, madam!",
            "I have seen the lights over the harbour!",
            "The end is nigh! Nigh-ish! Soon!",
            "Turn off your television sets! That is how they get in!",
        ],
        (Act::Prophet, Hero) => &[
            "You! Caped one! Will you save us from the saucers?",
            "Even heroes cannot stop what is coming!",
            "A hero! Tell them, hero! Tell them I am right!",
        ],
        (Act::Prophet, Leave) => &[
            "The end will keep till tomorrow.",
            "Same time tomorrow. If there IS a tomorrow!",
        ],
        (Act::Prophet, Panic) => &["IT HAS BEGUN!", "I TOLD YOU! I TOLD ALL OF YOU!"],
        (Act::Theremin, Own) => &[
            "This next one is called 'Moonlight over the Aether Spire'.",
            "No hands touch it. It is all in the air, friends.",
            "Played this at the World's Fair, you know.",
            "A little something from the stars.",
            "Coins in the case, if it moved you.",
        ],
        (Act::Theremin, Hero) => &[
            "A hero! This one is for you.",
            "Careful, your powers make it squeal.",
        ],
        (Act::Theremin, Leave) => &["Thank you, thank you. Goodnight, Earth."],
        (Act::Theremin, Panic) => &["Not the theremin!", "My instrument!"],
        (Act::Statue, Boo) => &[
            "BOO!",
            "Greetings, Earthling!",
            "Take me to your leader!",
            "Gotcha!",
        ],
        (Act::Statue, Leave) => &["Phew. My arm.", "Same time tomorrow, Earthlings."],
        (Act::Statue, Panic) => &["Abort mission!", "Back to the rocket!"],
        (Act::Juggler, Own) => &[
            "Three atoms, no waiting!",
            "Don't worry, they're only a little radioactive.",
            "And now, without looking!",
            "Who wants to try? No? Smart.",
            "Split one and we all go home early!",
        ],
        (Act::Juggler, Hero) => &["Catch!", "Can you juggle with heat vision? I bet you can."],
        (Act::Juggler, Leave) => &["Thanks, everyone! Mind the fallout!"],
        (Act::Juggler, Panic) => &["Atoms down! Everybody run!"],
        (Act::Spotter, Own) => &[
            "There! Over the Tesla tower! ...no. A pigeon.",
            "The foil keeps the rays out. Science.",
            "Saw three of them last Tuesday. In formation.",
            "That airship? Too slow. Too SLOW. Suspicious.",
            "They land in the park at night. I've measured the circles.",
            "Nobody believes me. Nobody ever believes me.",
        ],
        (Act::Spotter, Hero) => &[
            "Psst. You fly. Seen anything up there? Saucers?",
            "Are you from up there? You can tell me.",
        ],
        (Act::Spotter, Leave) => &["Shift's over. Keep watching the skies!"],
        (Act::Spotter, Panic) => &["It's an invasion! I KNEW it!"],
        (Act::Newsboy, Own) => &[
            "Extra! Extra! Saucer seen over the harbour!",
            "Extra! Mole folk open a hat shop downtown!",
            "Read all about it! Doctor's tower sparks again!",
            "Rocket port sets new record! Read all about it!",
            "Extra! Sky pirates seen over the rooftops!",
            "Get your Daily Comet! Hero saves the day!",
            "Extra! Extra! Robot wins the bake-off!",
        ],
        (Act::Newsboy, Hero) => &[
            "Hey! You're on the front page!",
            "Paper, hero? Free for you!",
        ],
        (Act::Newsboy, Leave) => &["Sold out! See you tomorrow!"],
        (Act::Newsboy, Panic) => &["Now THAT'S a headline!"],
        (Act::Stargazer, Own) => &[
            "Come and see the rings of Saturn! No charge.",
            "That bright one is Mars. Red as a fire engine.",
            "The Aether glow is low tonight. Perfect seeing.",
            "Look, look! The moon's craters, sharp as you like.",
            "One day we'll all go up there, from the rocket port.",
        ],
        (Act::Stargazer, Hero) => &[
            "A hero! Have you been up there? What is it like?",
            "Mind your cape, dear, you'll fog the lens.",
        ],
        (Act::Stargazer, Leave) => &["Clouds coming. Goodnight, stars."],
        (Act::Stargazer, Panic) => &["My telescope!"],
        (Act::Dancers, Own) => &[
            "Swing it!",
            "One more, darling!",
            "Turn it up!",
            "Watch this step!",
        ],
        (Act::Dancers, Hero) => &["Hey hero, can you dance?", "Come on, fly us a twirl!"],
        (Act::Dancers, Leave) => &["My feet! Home time."],
        (Act::Dancers, Panic) => &["Grab the radio!"],
        (Act::Club, Own) => &[
            "One at a time, folks.",
            "Ties, gentlemen. Ties.",
            "The Orbit Room. Finest jazz this side of Mars.",
            "You're on the list. Go on in.",
        ],
        (Act::Club, Hero) => &[
            "Capes are fine. Masks too. Go on in, hero.",
            "No flying in the lounge, hero.",
        ],
        (Act::Club, Leave) => &["That's it for tonight."],
        (Act::Club, Panic) => &["Everybody inside! Now!"],
        _ => &[],
    }
}

/// What a watcher says now and then: applause, a comment.
pub const WATCHER: &[&str] = &[
    "Bravo!",
    "Encore!",
    "Marvellous!",
    "How does he DO that?",
    "Isn't it wonderful?",
    "Ha! Again!",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_act_has_patter_or_a_moment() {
        for act in Act::ALL {
            let any = [Moment::Own, Moment::Hero, Moment::Boo, Moment::Leave]
                .iter()
                .any(|m| !lines(act, *m).is_empty());
            assert!(any, "{act:?} says nothing");
        }
    }
}
