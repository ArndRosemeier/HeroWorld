//! The Daily Comet: what the town's paper prints about a hero's deeds
//! (see `fameview` for when, `paper` for the front page on screen).
//!
//! A headline is written from a deed, the hero's fame and name, and where
//! it happened. A hero the town hardly knows is "a mystery hero" in print;
//! from "local hero" on the paper prints the name. Golden-age tabloid
//! talk: short, loud, in capitals.

use hw_sim::fame::Fame;
use hw_sim::incidents::IncidentKind;

/// Something a hero did that the paper prints.
#[derive(Clone, Debug, PartialEq)]
pub enum Deed {
    /// A call on the wrist radio answered: a crime stopped, people saved.
    Incident(IncidentKind),
    /// A street crime broken up.
    StreetCrime,
    /// One of the police computer's cases closed: the gang's name.
    Case(String),
    /// A chapter of the Doctor's story finished (how many are done now,
    /// of how many).
    Chapter(u8, u8),
    /// A new step of fame.
    Fame(Fame),
}

/// The picture on the front page: an ink silhouette (see `paper`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Picture {
    Star,
    Fist,
    Flame,
    Car,
    Robot,
    Saucer,
    Airship,
    Cuffs,
    Bag,
    Helmet,
    Mole,
    Rubble,
}

impl Picture {
    pub const ALL: [Picture; 12] = [
        Picture::Star,
        Picture::Fist,
        Picture::Flame,
        Picture::Car,
        Picture::Robot,
        Picture::Saucer,
        Picture::Airship,
        Picture::Cuffs,
        Picture::Bag,
        Picture::Helmet,
        Picture::Mole,
        Picture::Rubble,
    ];

    pub fn code(self) -> u8 {
        Self::ALL.iter().position(|p| *p == self).unwrap_or(0) as u8
    }

    pub fn from_code(c: u8) -> Self {
        Self::ALL.get(c as usize).copied().unwrap_or(Picture::Star)
    }
}

/// One front page.
#[derive(Clone, Debug, PartialEq)]
pub struct Edition {
    /// The line over the headline ("EXTRA!").
    pub kicker: String,
    /// The headline, in capitals.
    pub head: String,
    /// The line under it.
    pub sub: String,
    pub picture: Picture,
    /// The paper's number (it counts up with every edition about the hero).
    pub number: u32,
}

/// The most editions a hero's scrapbook keeps (the oldest go first).
pub const SCRAPBOOK_KEEP: usize = 24;

impl Edition {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "kicker": self.kicker,
            "head": self.head,
            "sub": self.sub,
            "picture": self.picture.code(),
            "number": self.number,
        })
    }

    pub fn from_json(v: &serde_json::Value) -> Option<Self> {
        let text = |k: &str, most: usize| -> Option<String> {
            Some(v[k].as_str()?.chars().take(most).collect())
        };
        Some(Self {
            kicker: text("kicker", 40)?,
            head: text("head", 120)?,
            sub: text("sub", 200)?,
            picture: Picture::from_code(v["picture"].as_u64()?.min(255) as u8),
            number: v["number"].as_u64().unwrap_or(1).min(u32::MAX as u64) as u32,
        })
    }
}

/// What the paper calls a hero it cannot name yet.
const STRANGERS: [&str; 4] = [
    "MYSTERY HERO",
    "MASKED STRANGER",
    "UNKNOWN HERO",
    "CAPED NEWCOMER",
];

fn pick<'a>(list: &[&'a str], k: u64) -> &'a str {
    list[(k % list.len() as u64) as usize]
}

/// The front page for `deed` by the hero `name` of `fame`, at `place` (a
/// street's name; empty if unknown). `k` picks among the ways of saying
/// it (any number: the edition's), `number` is the paper's.
pub fn write(deed: &Deed, fame: Fame, name: &str, place: &str, k: u64, number: u32) -> Edition {
    let named = fame.named_in_print() && !name.trim().is_empty();
    let hero = if named {
        name.trim().to_uppercase()
    } else {
        pick(&STRANGERS, k / 7).to_string()
    };
    let who = if named {
        name.trim().to_string()
    } else {
        "A hero nobody can name".to_string()
    };
    let at = if place.is_empty() {
        String::new()
    } else {
        format!(" on {place}")
    };
    let at_caps = at.to_uppercase();
    let kicker = pick(
        &[
            "EXTRA!",
            "EXTRA! EXTRA!",
            "LATE EDITION",
            "STOP PRESS",
            "EVENING SPECIAL",
        ],
        k,
    );
    let (kicker, head, sub, picture) = match deed {
        Deed::Incident(kind) => {
            let (heads, sub, picture): (&[&str], String, Picture) = match kind {
                IncidentKind::Mugging => (
                    &[
                        "{H} STOPS MUGGERS{AT}",
                        "MUGGERS MEET THEIR MATCH{AT}",
                        "{H} TO THE RESCUE{AT}",
                    ],
                    format!("{who} sends two thugs packing. Victim: \"I never even saw them land.\""),
                    Picture::Fist,
                ),
                IncidentKind::Robbery => (
                    &[
                        "{H} FOILS HOLD-UP{AT}",
                        "ROBBERS ROUTED{AT}",
                        "HOLD-UP GANG NABBED BY {H}",
                    ],
                    format!("{who} walks in, the gang walks out in cuffs. The till is safe."),
                    Picture::Bag,
                ),
                IncidentKind::Rampage => (
                    &[
                        "{H} HALTS MUTANT RAMPAGE",
                        "BRUTES BEATEN{AT}",
                        "MUTANTS MASHED BY {H}",
                    ],
                    format!("The Doctor's brutes smashed up the street{at}. {who} smashed back."),
                    Picture::Fist,
                ),
                IncidentKind::Breakout => (
                    &[
                        "LAB BREAKOUT BROKEN UP",
                        "{H} ROUNDS UP RAY-GUN GANG",
                        "TECHNICIANS TAKEN{AT}",
                    ],
                    format!("Ray guns blazing{at}. {who} was faster."),
                    Picture::Fist,
                ),
                IncidentKind::CarCrash => (
                    &[
                        "{H} PULLS DRIVER FROM WRECK",
                        "CRASH{AT}: ALL SAFE",
                        "SAVED FROM THE WRECKAGE BY {H}",
                    ],
                    format!("Two cars met{at}. {who} met the ambulance half way."),
                    Picture::Car,
                ),
                IncidentKind::Fire => (
                    &[
                        "{H} SAVES ALL FROM BLAZE",
                        "OUT OF THE FLAMES{AT}",
                        "INFERNO{AT}: NOBODY HURT",
                    ],
                    format!("{who} went in through the smoke and came out with everyone."),
                    Picture::Flame,
                ),
                IncidentKind::Colossus => (
                    &[
                        "COLOSSUS FALLS!",
                        "{H} TOPPLES WALKING TANK",
                        "THE DOCTOR'S GIANT: SCRAP",
                    ],
                    format!("The walking tank marched on the town. {who} sent it back in pieces."),
                    Picture::Robot,
                ),
                IncidentKind::Collapse => (
                    &[
                        "DUG OUT ALIVE BY {H}",
                        "RESCUE FROM THE RUBBLE{AT}",
                        "{H} FREES THE TRAPPED",
                    ],
                    format!("A building came down{at}. {who} lifted it off them."),
                    Picture::Rubble,
                ),
                IncidentKind::Surfacing => (
                    &[
                        "MOLE FOLK GO HOME IN PEACE",
                        "{H} CALMS MOLE FOLK{AT}",
                        "PANIC FROM BELOW: OVER",
                    ],
                    format!("Frightened mole folk came up{at}. {who} saw them safely home."),
                    Picture::Mole,
                ),
                IncidentKind::PirateRaid => (
                    &[
                        "SKY PIRATES REPELLED!",
                        "{H} CUTS PIRATES' ROPES",
                        "AIRSHIP RAIDERS ROUTED{AT}",
                    ],
                    format!("They came down the ropes{at}. {who} sent them back up, empty-handed."),
                    Picture::Airship,
                ),
                IncidentKind::IronLanding => (
                    &[
                        "IRON LEGION THROWN BACK",
                        "{H} FREES CAPTIVES{AT}",
                        "EMPIRE'S CAPSULE: A DENT{AT}",
                    ],
                    format!("The Iron-Masked Empire came for our people. {who} said no."),
                    Picture::Helmet,
                ),
                IncidentKind::SaucerRaid => (
                    &[
                        "SAUCER SENT PACKING!",
                        "{H} FOILS ABDUCTION{AT}",
                        "ZORRAK GO HOME, SAYS {H}",
                    ],
                    format!("A saucer hung over the street{at}. {who} kept everyone's feet on the ground."),
                    Picture::Saucer,
                ),
            };
            (kicker, pick(heads, k / 5).to_string(), sub, picture)
        }
        Deed::StreetCrime => (
            kicker,
            pick(
                &[
                    "{H} COLLARS STREET CROOKS",
                    "CAUGHT IN THE ACT{AT}",
                    "NOT ON {H}'S STREET",
                ],
                k / 5,
            )
            .to_string(),
            format!("Crooks at work{at} looked up, and there stood {who}."),
            Picture::Cuffs,
        ),
        Deed::Case(gang) => (
            "CASE CLOSED",
            pick(
                &[
                    "{H} BUSTS HIDEOUT",
                    "GANG'S DEN RAIDED{AT}",
                    "{H} CRACKS THE CASE",
                ],
                k / 5,
            )
            .to_string(),
            format!("{who} went in after {gang}. The police only had to bring the wagon."),
            Picture::Cuffs,
        ),
        Deed::Chapter(done, of) if done >= of => (
            "THE DOCTOR IS FINISHED",
            "{H} BRINGS DOWN THE DOCTOR!".to_string(),
            format!("The town can sleep again. A statue of {who} is to stand at Hero Hall."),
            Picture::Star,
        ),
        Deed::Chapter(done, of) => (
            "THE DOCTOR'S TRAIL",
            pick(
                &[
                    "{H} CLOSES IN ON THE DOCTOR",
                    "ANOTHER OF THE DOCTOR'S PLOTS FOILED",
                    "THE DOCTOR LOSES AGAIN",
                ],
                k / 5,
            )
            .to_string(),
            format!("Hero Hall: \"{done} of his {of} schemes are undone. {who} is on his heels.\""),
            Picture::Fist,
        ),
        Deed::Fame(step) => {
            let (head, sub): (&str, String) = match step {
                Fame::Nobody => ("WHO WAS THAT?", "Readers, write in.".to_string()),
                Fame::Noticed => (
                    "WHO IS THE {H}?",
                    "A new hero is on our streets. Nobody knows the name. Yet.".to_string(),
                ),
                Fame::Local => (
                    "A NAME TO REMEMBER: {H}",
                    format!("The stranger has a name. {who}: this town's own hero."),
                ),
                Fame::Favourite => (
                    "THE TOWN LOVES {H}",
                    format!("Capes sell out in every shop. Children want to be {who}."),
                ),
                Fame::Legend => (
                    "{H}: A LEGEND IN OUR TIME",
                    format!("They will tell their grandchildren they saw {who} fly by."),
                ),
            };
            ("THE TALK OF THE TOWN", head.to_string(), sub, Picture::Star)
        }
    };
    let head = head.replace("{H}", &hero).replace("{AT}", &at_caps);
    Edition {
        kicker: kicker.to_string(),
        head,
        sub,
        picture,
        number,
    }
}

/// What a newsboy shouts about an edition.
pub fn cry(e: &Edition) -> String {
    let head = e.head.trim_end_matches(['!', '?', '.']);
    format!("Extra! Extra! {head}! Read all about it!")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nobody_is_not_named_and_a_local_hero_is() {
        let deed = Deed::Incident(IncidentKind::Robbery);
        for k in 0..40 {
            let e = write(&deed, Fame::Noticed, "Captain Comet", "Orbit Street", k, 1);
            assert!(!e.head.contains("CAPTAIN COMET"), "{}", e.head);
            assert!(!e.sub.contains("Captain Comet"), "{}", e.sub);
            let e = write(&deed, Fame::Local, "Captain Comet", "Orbit Street", k, 1);
            assert!(
                e.head.contains("CAPTAIN COMET") || e.sub.contains("Captain Comet"),
                "{} / {}",
                e.head,
                e.sub
            );
        }
    }

    #[test]
    fn every_deed_makes_a_headline_with_nothing_left_to_fill_in() {
        let mut deeds: Vec<Deed> = IncidentKind::ALL.into_iter().map(Deed::Incident).collect();
        deeds.push(Deed::StreetCrime);
        deeds.push(Deed::Case("the Chrome Jackets".into()));
        deeds.push(Deed::Chapter(2, 7));
        deeds.push(Deed::Chapter(7, 7));
        deeds.extend(Fame::ALL.into_iter().map(Deed::Fame));
        for d in &deeds {
            for fame in Fame::ALL {
                for place in ["", "Comet Avenue"] {
                    for k in 0..30 {
                        let e = write(d, fame, "Nova", place, k, 3);
                        for text in [&e.kicker, &e.head, &e.sub] {
                            assert!(!text.is_empty(), "{d:?}");
                            assert!(!text.contains('{') && !text.contains('}'), "{text}");
                            assert!(!text.contains("  "), "{text}");
                        }
                        assert_eq!(e.head, e.head.to_uppercase(), "{}", e.head);
                        assert!(e.head.chars().count() <= 60, "{}", e.head);
                    }
                }
            }
        }
    }

    #[test]
    fn an_edition_comes_back_from_the_hero_file() {
        let e = write(
            &Deed::Incident(IncidentKind::Fire),
            Fame::Favourite,
            "Nova",
            "Maple Lane",
            4,
            17,
        );
        assert_eq!(Edition::from_json(&e.to_json()), Some(e.clone()));
        assert!(cry(&e).starts_with("Extra! Extra! "));
        assert!(!cry(&e).contains("!!"));
    }
}
