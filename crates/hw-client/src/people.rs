//! People from Actors: heroes and citizens as animated characters.
//!
//! Characters are built from Actors packs (downloaded by
//! `scripts/fetch-packs`, never committed) on a worker thread, because one
//! takes about half a second. The game asks for what it needs and picks the
//! finished models up when they are ready; until then a hero is drawn as
//! the old placeholder and citizens wait.
//!
//! Looks follow the golden age of the design: people of today's races only
//! (no fantasy folk in the street), dressed in the clothes of the era
//! (Actors' "vintage" wardrobe), heroes also in retro-future suits.

use crate::poses;
use actors_engine::{BuildOptions, CharacterBuilder};
use actors_generator::{
    default_config, random_config, AllowOptions, CharacterCatalog, CharacterConfig, Flags,
    HeightRange, RandomOptions, SeededRandom,
};
use engine::anim::AnimatedModel;
use engine::mesh::AlbedoMap;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::Duration;

/// The people thread lets go of the character builder (and its caches)
/// after this long with nothing to build.
pub const IDLE_DROP: Duration = Duration::from_secs(20);

/// The clips a hero has (baked from the packs), plus "fly" and "hover"
/// made here (see [`poses`]).
pub const HERO_CLIPS: &[&str] = &[
    "idle",
    "walk",
    "jog",
    "sprint",
    "jump start",
    "jump",
    "jump land",
    "punch jab",
    "punch cross",
    "spell cast",
    "fight stance",
    "hit to chest",
];
/// A sidekick walks, runs, jumps and punches.
pub const SIDEKICK_CLIPS: &[&str] = &[
    "idle",
    "walk",
    "jog",
    "sprint",
    "jump start",
    "jump",
    "jump land",
    "punch jab",
    "punch cross",
    "fight stance",
    "hit to chest",
];
/// Villains walk, run, brawl and shoot; "cuffed" (hanging from a police
/// drone) is made here.
pub const VILLAIN_CLIPS: &[&str] = &[
    "idle",
    "walk",
    "jog",
    "sprint",
    "jump start",
    "jump",
    "jump land",
    "fight stance",
    "hook",
    "pistol idle",
    "pistol aim",
    "pistol shoot",
    "hit to chest",
];
/// Citizens walk and stand; a few walk the formal walk. "hands up" (a
/// mugging's victim) is made here, and "clap" for watching street acts;
/// "interact" drops a coin in a busker's hat or buys a paper.
pub const CITIZEN_CLIPS: &[&str] = &["idle", "walk", "walk formal", "interact"];
/// The street life's performers and club guests: their acts' clips are
/// made here (see [`poses::add_street_clips`]) over these.
pub const STREET_CLIPS: &[&str] = &[
    "idle",
    "walk",
    "talk",
    "interact",
    "pick up",
    "arms folded",
    "dance",
    "nod",
    "jog",
];

/// How fast each locomotion clip travels, in m/s (measured from the clips
/// with their travel left in: see the `clip_speeds` test).
pub const WALK_MPS: f32 = 1.45;
pub const JOG_MPS: f32 = 3.6;
pub const SPRINT_MPS: f32 = 6.4;

/// What a finished model is for.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Who {
    /// A stand-in hero for a player whose looks have not arrived, by a
    /// name ("player 2"): the same on every machine.
    Hero(String),
    /// A hero made in the creator: its looks' key (see [`crate::looks`]).
    Look(u64),
    /// One of the citizens' looks.
    Citizen(u32),
    /// One of the mad scientist's villains: its kind and one of
    /// [`VILLAIN_LOOKS`] looks.
    Villain(VillainKind, u32),
    /// One of the town's own heroes from Hero Hall, by index (see
    /// [`hw_sim::npcheroes::NAMES`]).
    NpcHero(u8),
    /// A hero's robot sidekick, by the power's theme (0 Tin Pal, 1 Brass
    /// Squire): an Actors robot, drawn small ([`hw_sim::sidekick::SIZE`]).
    Sidekick(u8),
    /// The hero's statue in front of Hero Hall (the Doctor's story done):
    /// their looks (by key) cast in bronze.
    Statue(u64),
    /// The ambulance medic at rescues: all in white.
    Medic,
    /// One of the street life's performers (see `hw_sim::streetlife`):
    /// the act and which of its performers (the dancers are two).
    Street(hw_sim::streetlife::Act, u8),
    /// A night club guest dressed for the evening, one of
    /// [`hw_sim::streetlife::GUEST_LOOKS`].
    Guest(u8),
}

/// Looks per kind of villain.
pub const VILLAIN_LOOKS: u32 = 3;

/// The villains drawn as Actors characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum VillainKind {
    /// Mutant brute: huge, green-skinned, glowing eyes, in work clothes.
    Brute,
    /// Ray-gun technician: lab coat, tinted goggles, gloves.
    Technician,
    /// A Chrome Jacket: a greaser in a black leather jacket, jeans and
    /// biker boots, slicked hair, shades.
    Greaser,
    /// A Velvet Syndicate gunsel: a pinstripe suit and a tilted fedora.
    Gunsel,
    /// A Molehaven delver: one of the mole folk in a leather tunnelling
    /// cap, heavy gauntlets and earth-brown overalls, carrying a pick.
    Delver,
    /// A sky pirate buccaneer: a sailor's striped shirt, an aviator's
    /// leather cap and goggles, pirate boots, a cutlass.
    Buccaneer,
    /// A sky pirate gunner: an oxblood long coat over a shirt, goggles,
    /// boots, a ray pistol.
    Gunner,
    /// A sky pirate captain: a navy long coat over stripes, a wide hat,
    /// gauntlets and a full beard.
    Captain,
    /// An Iron-Masked legionary: a closed iron helm, gunmetal uniform,
    /// iron pauldrons and gauntlets, a ray rifle.
    Legionary,
    /// An Iron-Masked praetorian: a crested bronze helm, breastplate,
    /// greaves over a blood-red uniform, a shock lance.
    Praetorian,
    /// The mad scientist's shield bearer: a bulkier mutant in iron
    /// shoulder plates and gauntlets, behind a riveted shield.
    Bulwark,
    /// The mad scientist's grenadier: olive work overalls, goggles, a
    /// brass bandolier of bombs.
    Grenadier,
    /// The mad scientist's lab orderly: mint-green scrubs, white gloves
    /// and shoes, a mending ray.
    Orderly,
    /// The mutant alpha who leads a rampage: the heaviest brute there is,
    /// darker, bald, with long tusks and blazing eyes (drawn a head and a
    /// half taller). One look, built when an alpha first shows.
    Alpha,
    /// The Doctor himself: a tall, gaunt old man in a long white lab coat
    /// over a black suit, wild white hair and beard, dark goggles, heavy
    /// rubber gauntlets, eyes glowing Aether green. One look.
    Doctor,
    /// A Zorrak zapper: a slight invader from the moon, pale teal skin, a
    /// bald domed head with great black eyes, in a silver flight suit with
    /// red gauntlets and boots (the fishbowl helmet and its antennae are
    /// hung on its head: `factionview::headgear`).
    Zapper,
    /// A Zorrak grabber: a taller, heavier invader in a gold-trimmed suit.
    Grabber,
    /// A Zorrak overlord: the biggest head of all (the brain shows through
    /// its glass dome), in a royal purple coat over the silver. One look.
    Overlord,
}

impl VillainKind {
    /// The villain drawn for an entity of `kind`, if it is a person.
    pub fn of(kind: hw_net::EntityKind) -> Option<Self> {
        use hw_net::EntityKind as K;
        Some(match kind {
            K::Brute => VillainKind::Brute,
            K::Alpha => VillainKind::Alpha,
            K::Technician => VillainKind::Technician,
            K::Greaser => VillainKind::Greaser,
            K::Gunsel => VillainKind::Gunsel,
            K::Delver => VillainKind::Delver,
            K::Buccaneer => VillainKind::Buccaneer,
            K::Gunner => VillainKind::Gunner,
            K::Captain => VillainKind::Captain,
            K::Legionary => VillainKind::Legionary,
            K::Praetorian => VillainKind::Praetorian,
            K::Doctor => VillainKind::Doctor,
            K::Bulwark => VillainKind::Bulwark,
            K::Grenadier => VillainKind::Grenadier,
            K::Orderly => VillainKind::Orderly,
            K::Zapper => VillainKind::Zapper,
            K::Grabber => VillainKind::Grabber,
            K::Overlord => VillainKind::Overlord,
            _ => return None,
        })
    }

    /// Dressed from the whole wardrobe (iron helms, breastplates), not
    /// only the era's clothes the townsfolk wear.
    pub fn any_clothes(self) -> bool {
        matches!(
            self,
            VillainKind::Buccaneer
                | VillainKind::Gunner
                | VillainKind::Captain
                | VillainKind::Legionary
                | VillainKind::Praetorian
                | VillainKind::Zapper
                | VillainKind::Grabber
                | VillainKind::Overlord
        )
    }

    /// Looks it comes in (bosses and lieutenants have one).
    pub fn looks(self) -> u32 {
        match self {
            VillainKind::Alpha
            | VillainKind::Captain
            | VillainKind::Praetorian
            | VillainKind::Doctor
            | VillainKind::Overlord => 1,
            _ => VILLAIN_LOOKS,
        }
    }

    pub const ALL: [VillainKind; 4] = [
        VillainKind::Brute,
        VillainKind::Technician,
        VillainKind::Greaser,
        VillainKind::Gunsel,
    ];
}

/// A built character.
pub struct Built {
    pub who: Who,
    pub model: Arc<AnimatedModel>,
    /// Its walk clip ("walk" or "walk formal").
    pub walk: String,
    /// A citizen who fits in a car seat (see `riderview::fits_seats`).
    pub sits: bool,
}

/// One character to build.
struct Job {
    who: Who,
    seed: u64,
    /// The looks of a [`Who::Look`].
    config: Option<CharacterConfig>,
    /// A preview in the creator: only the newest one waiting is built.
    preview: bool,
}

/// Builds characters on a worker thread.
pub struct People {
    jobs: Sender<Job>,
    done: Receiver<Result<Built, String>>,
    catalog: Receiver<CharacterCatalog>,
}

/// Where the packs are: `HW_PACKS`, else `assets/packs` here or next to the
/// game's folder.
pub fn packs_dir() -> Option<PathBuf> {
    let has = |p: &Path| p.join("human/manifest.json").is_file();
    if let Ok(p) = std::env::var("HW_PACKS") {
        let p = PathBuf::from(p);
        return has(&p).then_some(p);
    }
    let mut tries = vec![PathBuf::from("assets/packs")];
    tries.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/packs"));
    if let Ok(exe) = std::env::current_exe() {
        for up in exe.ancestors().skip(1).take(4) {
            tries.push(up.join("assets/packs"));
        }
    }
    tries.into_iter().find(|p| has(p))
}

/// The look rules: humans of today's races, adult heights, and the
/// wardrobes the era's clothes are filed in (which garments are worn is
/// narrowed further by [`golden_age`]).
pub fn allow(hero: bool) -> AllowOptions {
    let mut clothing = vec!["vintage".to_string(), "modern".to_string()];
    if hero {
        clothing.push("future".into());
    }
    AllowOptions {
        races: Some(Flags::List(vec!["human".into()])),
        clothing: Some(Flags::List(clothing)),
        height: Some(HeightRange {
            min: Some(1.55),
            max: Some(if hero { 1.98 } else { 1.95 }),
        }),
    }
}

/// A stable number from a name (FNV-1a), so a hero looks the same on
/// every machine and every start.
pub fn name_seed(name: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in name.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

impl People {
    /// Starts the worker on the packs at `packs`.
    pub fn start(packs: PathBuf) -> Self {
        let (jobs, rx) = channel::<Job>();
        let (tx, done) = channel();
        let (cat_tx, catalog) = channel();
        std::thread::Builder::new()
            .name("people".into())
            .spawn(move || {
                let load = || {
                    let t0 = std::time::Instant::now();
                    let b = CharacterBuilder::load(&packs)
                        .map_err(|e| format!("loading {}: {e}", packs.display()));
                    if b.is_ok() {
                        eprintln!("Actors packs loaded in {:.1} s", t0.elapsed().as_secs_f64());
                    }
                    b
                };
                let mut builder = match load() {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        return;
                    }
                };
                // What heroes may wear, for the creator.
                builder.allow(&allow(true));
                let _ = cat_tx.send(hero_catalog(builder.catalog()));
                let mut builder = Some((builder, Halves::default()));
                // Heroes before villains before citizens, whatever order
                // they were asked in.
                let mut queue: Vec<Job> = Vec::new();
                loop {
                    while let Ok(j) = rx.try_recv() {
                        queue.push(j);
                    }
                    if queue.is_empty() {
                        // Idle: the builder's caches (decoded pack textures,
                        // garment meshes: most of a gigabyte after the
                        // start-up looks) go; the next job loads it anew.
                        let next = match builder {
                            Some(_) => rx.recv_timeout(IDLE_DROP),
                            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
                        };
                        match next {
                            Ok(j) => queue.push(j),
                            Err(RecvTimeoutError::Timeout) => builder = None,
                            Err(RecvTimeoutError::Disconnected) => return,
                        }
                        continue;
                    }
                    // Of the creator's previews only the newest counts.
                    if let Some(last) = queue.iter().rposition(|j| j.preview) {
                        let mut i = 0;
                        queue.retain(|j| {
                            i += 1;
                            !j.preview || i - 1 == last
                        });
                    }
                    let i = queue
                        .iter()
                        .position(|j| matches!(j.who, Who::Hero(_) | Who::Look(_)))
                        .or_else(|| queue.iter().position(|j| matches!(j.who, Who::Villain(..))))
                        .unwrap_or(0);
                    let job = queue.remove(i);
                    let (b, halves) = match &mut builder {
                        Some(b) => (&mut b.0, &mut b.1),
                        None => match load() {
                            Ok(b) => {
                                let b = builder.insert((b, Halves::default()));
                                (&mut b.0, &mut b.1)
                            }
                            Err(e) => {
                                if tx.send(Err(e)).is_err() {
                                    return;
                                }
                                continue;
                            }
                        },
                    };
                    let t = std::time::Instant::now();
                    let r = build_sharing(b, halves, &job.who, job.seed, job.config.as_ref());
                    if let Ok(b) = &r {
                        eprintln!(
                            "built {:?} in {:.2} s ({} meshes, {} MB of maps)",
                            b.who,
                            t.elapsed().as_secs_f64(),
                            b.model.meshes.len(),
                            map_bytes(&b.model) >> 20
                        );
                    }
                    if tx.send(r).is_err() {
                        return;
                    }
                }
            })
            .expect("start the people thread");
        Self {
            jobs,
            done,
            catalog,
        }
    }

    /// Asks for a character; `seed` varies citizens between towns.
    pub fn request(&self, who: Who, seed: u64) {
        let _ = self.jobs.send(Job {
            who,
            seed,
            config: None,
            preview: false,
        });
    }

    /// Asks for the bronze statue of a hero with these looks.
    pub fn request_statue(&self, key: u64, config: CharacterConfig) {
        let _ = self.jobs.send(Job {
            who: Who::Statue(key),
            seed: 0,
            config: Some(config),
            preview: false,
        });
    }

    /// Asks for a hero with these looks (`key` from [`crate::looks::key`]);
    /// a `preview` is dropped when a newer one comes before it is built.
    pub fn request_look(&self, key: u64, config: CharacterConfig, preview: bool) {
        let _ = self.jobs.send(Job {
            who: Who::Look(key),
            seed: 0,
            config: Some(config),
            preview,
        });
    }

    /// What heroes can be made of, once the packs are loaded (given once).
    pub fn take_catalog(&self) -> Option<CharacterCatalog> {
        self.catalog.try_recv().ok()
    }

    /// Waits until `n` characters are finished (for screenshots and tests).
    pub fn wait(&self, n: usize) -> Vec<Built> {
        let mut out = Vec::new();
        while out.len() < n {
            match self.done.recv() {
                Ok(Ok(b)) => out.push(b),
                Ok(Err(e)) => {
                    eprintln!("people: {e}");
                    break;
                }
                Err(_) => break,
            }
        }
        out
    }

    /// Characters finished since the last call.
    pub fn finished(&self) -> Vec<Built> {
        let mut out = Vec::new();
        while let Ok(r) = self.done.try_recv() {
            match r {
                Ok(b) => out.push(b),
                Err(e) => eprintln!("people: {e}"),
            }
        }
        out
    }
}

/// Garments people wore in the golden age (1930s to 1950s), whatever
/// Actors files them under; anything else (t-shirts and trainers, mini
/// skirts, lingerie) stays out of the street.
const ERA: &[&str] = &[
    "briefs",
    "socks",
    "stockings",
    "longsleeve",
    "skirt",
    "polo",
    "wool-pants",
    "fisherman-sweater",
    "cargo-pants",
    "jacket-jeans",
    "worksuit",
    "suit-tie",
    "suit-double",
    "suit-dinner",
    "suit-female",
    "suit-3",
    "suit-bowtie",
    "elegant-male",
    "gown",
    "gloves-long",
    "shift-dress",
    "halter-midi",
    "flapper-dress",
    "ruffle-dress",
    "blouse-pencil-skirt",
    "leather-shoes",
    "black-shoes",
    "ballet-flats",
    "t-bar-shoes",
    "work-boots",
    "ankle-boots",
    "riding-boots",
    "buckled-shoes",
    "fedora",
    "fedora-tilted",
    "newsboy-cap",
    "cloche-hat",
    "wide-brim-hat",
    "round-glasses",
];
/// Heroes may also wear the retro-future suits (made for 3026, but they
/// look like what 1939 thought the future would wear), and what a costume
/// needs that Actors has: tights, gloves, boots, goggles, an aviator's cap.
const HERO_EXTRA: &[&str] = &[
    "sneakers",
    "canvas-sneakers",
    "future-",
    "tights",
    "tank-top",
    "turtleneck-halter",
    "open-coat",
    "gloves-short",
    "gloves-medium",
    "gauntlet-gloves",
    "leather-boots",
    "biker-boots",
    "cuffed-boots",
    "cuffed-knee-boots",
    "overknee-boots",
    "tinted-glasses",
    "dark-glasses",
    "leather-helmet",
];

/// May a garment be worn in town? Period clothes (and anything Actors
/// files as vintage, so new period garments walk the streets as soon as the
/// packs have them); heroes also the retro-future suits.
pub fn wearable(catalog: &CharacterCatalog, g: &str, hero: bool) -> bool {
    let vintage = catalog.garments.iter().any(|c| {
        c.id == g
            && c.themes
                .as_ref()
                .is_some_and(|t| t.iter().any(|t| t == "vintage"))
    });
    ERA.contains(&g) || vintage || (hero && HERO_EXTRA.iter().any(|e| g == *e || g.starts_with(e)))
}

/// The catalog the hero creator offers: only what heroes may wear, and
/// only the outfits made of that.
pub fn hero_catalog(catalog: &CharacterCatalog) -> CharacterCatalog {
    let mut c = catalog.clone();
    c.garments.retain(|g| wearable(catalog, &g.id, true));
    // (Not "Nude": heroes go out dressed.)
    c.outfits.retain(|o| {
        !o.garments.is_empty()
            && o.garments
                .iter()
                .chain(o.female.iter().flatten())
                .all(|g| wearable(catalog, g, true))
    });
    let mut all = costumes(&c);
    all.append(&mut c.outfits);
    c.outfits = all;
    c
}

/// A costume: (id, label, description, garments, gender, colours).
type Costume = (
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
    Option<&'static str>,
    &'static [(&'static str, &'static str)],
);

/// Hero costumes made of what Actors has today, in the colours of the
/// pulps.
const COSTUMES: &[Costume] = &[
    (
        "hw-strongman",
        "Circus strongman",
        "Tights, a vest and boots: the classic.",
        &[
            "briefs",
            "tights",
            "tank-top",
            "gloves-short",
            "biker-boots",
        ],
        None,
        &[
            ("briefs", "#d23a2c"),
            ("tights", "#2a4fb8"),
            ("tank-top", "#d23a2c"),
            ("gloves-short", "#d23a2c"),
            ("biker-boots", "#d23a2c"),
        ],
    ),
    (
        "hw-mystery-man",
        "Mystery man",
        "Who knows what evil lurks? A dark suit, hat and gloves.",
        &[
            "suit-double",
            "fedora-tilted",
            "gloves-short",
            "dark-glasses",
            "black-shoes",
        ],
        Some("male"),
        &[
            ("suit-double", "#1d1f2a"),
            ("fedora-tilted", "#1d1f2a"),
            ("gloves-short", "#7a1c1c"),
        ],
    ),
    (
        "hw-aviator",
        "Sky ace",
        "Goggles, gloves and riding boots, ready for the stratosphere.",
        &[
            "briefs",
            "fisherman-sweater",
            "wool-pants",
            "riding-boots",
            "gloves-medium",
            "tinted-glasses",
        ],
        None,
        &[
            ("fisherman-sweater", "#e8dcc0"),
            ("wool-pants", "#6a4a2c"),
            ("riding-boots", "#3a2416"),
            ("gloves-medium", "#6a4a2c"),
        ],
    ),
    (
        "hw-rocket",
        "Rocket ranger",
        "The suit 1939 thought the future would wear.",
        &[
            "briefs",
            "future-vest",
            "future-trousers",
            "future-belt",
            "gloves-medium",
            "leather-boots",
            "tinted-glasses",
        ],
        None,
        &[
            ("future-vest", "#c9cdd6"),
            ("future-trousers", "#c9cdd6"),
            ("future-belt", "#d4a017"),
            ("gloves-medium", "#b8261c"),
            ("leather-boots", "#b8261c"),
        ],
    ),
    (
        "hw-jungle",
        "Jungle queen",
        "A halter dress, long gloves and boots for the lost city.",
        &["briefs", "halter-midi", "gloves-long", "overknee-boots"],
        Some("female"),
        &[
            ("halter-midi", "#2e7d4f"),
            ("gloves-long", "#e8c070"),
            ("overknee-boots", "#5a3a1c"),
        ],
    ),
    (
        "hw-starlet",
        "Silver-screen sleuth",
        "Gloves, a pencil skirt and a hat with a secret.",
        &[
            "blouse-pencil-skirt",
            "cloche-hat",
            "gloves-long",
            "t-bar-shoes",
        ],
        Some("female"),
        &[
            ("blouse-pencil-skirt", "#a8262f"),
            ("cloche-hat", "#1d1f2a"),
            ("gloves-long", "#1d1f2a"),
        ],
    ),
];

/// The costumes above that the catalog can make.
fn costumes(catalog: &CharacterCatalog) -> Vec<actors_generator::OutfitPreset> {
    COSTUMES
        .iter()
        .filter(|(_, _, _, garments, _, _)| garments.iter().all(|g| catalog.garment(g).is_some()))
        .map(
            |(id, label, about, garments, gender, colours)| actors_generator::OutfitPreset {
                id: id.to_string(),
                label: label.to_string(),
                description: Some(about.to_string()),
                garments: garments.iter().map(|g| g.to_string()).collect(),
                female: None,
                gender: gender.map(|g| g.to_string()),
                colors: Some(
                    colours
                        .iter()
                        .map(|(g, c)| (g.to_string(), c.to_string()))
                        .collect(),
                ),
                themes: None,
            },
        )
        .collect()
}

/// One of the town's own heroes (by index into
/// [`hw_sim::npcheroes::NAMES`]): a costume from [`COSTUMES`] in their own
/// colours, on a heroic build. The same on every machine.
pub fn npc_hero(builder: &CharacterBuilder, index: u8) -> CharacterConfig {
    // (costume, female, colours, hair, skin)
    type Npc = (
        &'static str,
        bool,
        &'static [(&'static str, &'static str)],
        &'static str,
        &'static str,
    );
    const NPCS: [Npc; 5] = [
        // Commander Starling: the sky ace, in midnight blue and white.
        (
            "hw-aviator",
            false,
            &[
                ("fisherman-sweater", "#f2ece0"),
                ("wool-pants", "#1e2c5a"),
                ("riding-boots", "#1a1a22"),
                ("gloves-medium", "#1e2c5a"),
            ],
            "#3a2a1c",
            "#d9a982",
        ),
        // Atomic Annie: a rocket suit in atom teal and gold.
        (
            "hw-rocket",
            true,
            &[
                ("future-vest", "#2aa6a0"),
                ("future-trousers", "#2aa6a0"),
                ("future-belt", "#e2b23a"),
                ("gloves-medium", "#e2b23a"),
                ("leather-boots", "#e2b23a"),
            ],
            "#c0392b",
            "#f0c8a8",
        ),
        // The Brass Badger: the strongman, in brass and black.
        (
            "hw-strongman",
            false,
            &[
                ("briefs", "#1c1a18"),
                ("tights", "#b8862c"),
                ("tank-top", "#1c1a18"),
                ("gloves-short", "#b8862c"),
                ("biker-boots", "#1c1a18"),
            ],
            "#1c1a18",
            "#8d5a3c",
        ),
        // Lady Lumen: the sleuth, all in white with gold gloves.
        (
            "hw-starlet",
            true,
            &[
                ("blouse-pencil-skirt", "#f4f1ea"),
                ("cloche-hat", "#f4f1ea"),
                ("gloves-long", "#d9b44a"),
            ],
            "#e8d08a",
            "#e9c3a0",
        ),
        // Rocket Rory: a rocket suit in fire-engine red and silver.
        (
            "hw-rocket",
            false,
            &[
                ("future-vest", "#c8261c"),
                ("future-trousers", "#c9cdd6"),
                ("future-belt", "#c9cdd6"),
                ("gloves-medium", "#c9cdd6"),
                ("leather-boots", "#c8261c"),
            ],
            "#d98a2c",
            "#f2d0b4",
        ),
    ];
    let (costume, female, colours, hair, skin) = NPCS[index as usize % NPCS.len()];
    let name = hw_sim::npcheroes::NAMES[index as usize % hw_sim::npcheroes::NAMES.len()];
    let catalog = builder.catalog();
    let mut c = golden_age(catalog, name_seed(name), true);
    let b = &mut c.body;
    b.gender = if female { 0.0 } else { 1.0 };
    b.age_years = 30.0 + 3.0 * index as f64;
    b.muscle = if female { 0.6 } else { 0.85 };
    b.weight = if costume == "hw-strongman" { 0.7 } else { 0.5 };
    b.height = if female { 0.6 } else { 0.85 };
    c.beard = "none".into();
    c.hair_color = Some(hair.into());
    c.skin = actors_generator::SkinKind::Procedural;
    c.skin_color = Some(skin.into());
    if costume == "hw-strongman" {
        // The circus strongman: bald, broad and big.
        c.hair = None;
        c.body.muscle = 1.0;
        c.body.weight = 0.78;
        c.body.height = 0.95;
        c.body.proportions = 0.4;
    }
    if let Some((_, _, _, garments, _, _)) = COSTUMES.iter().find(|x| x.0 == costume) {
        c.outfit = garments
            .iter()
            .filter(|g| catalog.garment(g).is_some())
            .map(|g| g.to_string())
            .collect();
    }
    c.garment_colors.clear();
    for (g, hex) in colours {
        c.garment_colors.insert(g.to_string(), hex.to_string());
    }
    c
}

/// A random character of `catalog` (what `CharacterBuilder::random` makes).
pub fn random_person(catalog: &CharacterCatalog, seed: u32) -> CharacterConfig {
    random_config(
        catalog,
        &default_config(catalog),
        &RandomOptions::default(),
        &mut SeededRandom::new(seed as f64),
    )
}

/// A random character dressed for the era: the first of a run of seeds
/// whose outfit is all period clothes (and not just underwear), then a hat
/// for most men and some women, and now and then a three-piece suit or a
/// bow tie instead of the plain suit. Same seed, same person.
pub fn golden_age(catalog: &CharacterCatalog, seed: u32, hero: bool) -> CharacterConfig {
    let ok = |g: &str| wearable(catalog, g, hero);
    let mut config = random_person(catalog, seed);
    for k in 0..64u32 {
        let c = random_person(catalog, seed.wrapping_add(k.wrapping_mul(0x9E37_79B9)));
        let dressed = c.outfit.iter().any(|g| g != "briefs" && g != "socks");
        if dressed && c.outfit.iter().all(|g| ok(g)) {
            config = c;
            break;
        }
    }
    let mut rng = hw_core::Seed(seed as u64).child("wardrobe").rng();
    if hero {
        // Heroes are in their prime and in shape.
        let b = &mut config.body;
        b.age_years = rng.range(22.0, 40.0);
        b.muscle = rng.range(0.7, 0.92);
        b.weight = rng.range(0.38, 0.55);
        b.proportions = rng.range(0.7, 0.9);
    }
    let has = |id: &str| catalog.garments.iter().any(|g| g.id == id);
    let male = config.body.gender >= 0.5;
    let swap = |outfit: &mut Vec<String>, from: &str, to: &str| {
        if has(to) {
            for g in outfit.iter_mut() {
                if g == from {
                    *g = to.to_string();
                }
            }
        }
    };
    if male && rng.chance(0.35) {
        let to = if rng.chance(0.5) {
            "suit-3"
        } else {
            "suit-bowtie"
        };
        swap(&mut config.outfit, "suit-tie", to);
    }
    let hat_chance = if male { 0.6 } else { 0.3 };
    let wears_hat = |o: &[String]| {
        o.iter().any(|g| {
            catalog
                .garments
                .iter()
                .any(|c| c.id == *g && c.occupies.iter().any(|l| l == "hat"))
        })
    };
    if !hero && !wears_hat(&config.outfit) && rng.chance(hat_chance) {
        let hats: &[&str] = if male {
            &["fedora", "fedora", "fedora-tilted", "newsboy-cap"]
        } else {
            &["cloche-hat", "wide-brim-hat"]
        };
        let hat = *rng.pick(hats);
        if has(hat) {
            config.outfit.push(hat.to_string());
        }
    }
    config
}

/// A villain of the mad scientist: a person of the town changed by his
/// experiments (the brute) or one of his staff (the technician).
/// A robot sidekick's looks: a sturdy little robot of the 1950s toy shop.
/// Tin Pal (theme 0) is chrome with red trim, a visor and pod ears; Brass
/// Squire (theme 1) is polished brass with blue trim, round optics and
/// antennae. Both have a glowing chest core.
pub fn sidekick(builder: &CharacterBuilder, theme: u8) -> CharacterConfig {
    let mut c = builder.default_config();
    c.robot = true;
    c.body.gender = 0.5;
    c.body.muscle = 0.7;
    c.body.weight = 0.65;
    c.body.age_years = 30.0;
    let r = &mut c.robot_look;
    r.plating = "segmented".into();
    r.thickness = 0.8;
    r.core = true;
    r.glow_strength = 0.85;
    r.finish = "metallic".into();
    if theme == 0 {
        r.head = "visor".into();
        r.ears = "pods".into();
        r.paint = "#d9dde3".into();
        r.trim = "#c8262c".into();
        r.frame = "#2a2d33".into();
        r.metal = "#9aa3ad".into();
        r.glow = "#5fe0ff".into();
    } else {
        r.head = "optics".into();
        r.ears = "antennae".into();
        r.paint = "#c9a24a".into();
        r.trim = "#2c5aa8".into();
        r.frame = "#2b2620".into();
        r.metal = "#8a6a32".into();
        r.glow = "#ffc860".into();
    }
    c
}

/// The ambulance medic: a white tunic suit under a white coat, white
/// gloves and shoes.
pub fn medic(builder: &CharacterBuilder) -> actors_generator::CharacterConfig {
    let seed = name_seed("ambulance medic");
    let mut c = golden_age(builder.catalog(), seed, false);
    let catalog = builder.catalog();
    let keep = |list: &[&str]| -> Vec<String> {
        list.iter()
            .filter(|g| catalog.garments.iter().any(|x| x.id == **g))
            .map(|g| g.to_string())
            .collect()
    };
    c.body.age_years = 38.0;
    c.body.muscle = 0.45;
    c.body.weight = 0.45;
    c.beard = "none".into();
    c.outfit = keep(&["worksuit", "open-coat", "gloves-short", "black-shoes"]);
    for (g, hex) in [
        ("worksuit", "#f4f4f0"),
        ("open-coat", "#fbfbf8"),
        ("gloves-short", "#f2f2ee"),
        ("black-shoes", "#eeeeea"),
    ] {
        c.garment_colors.insert(g.to_string(), hex.to_string());
    }
    c
}

/// A street performer's looks (see `hw_sim::streetlife::Act`): dressed
/// for the act from the era's wardrobe, the same in every town.
pub fn street(
    builder: &CharacterBuilder,
    act: hw_sim::streetlife::Act,
    k: u8,
    seed: u32,
) -> actors_generator::CharacterConfig {
    use hw_sim::streetlife::Act;
    let mut c = golden_age(builder.catalog(), seed, false);
    let mut rng = hw_core::Seed(seed as u64).child("street").rng();
    let catalog = builder.catalog();
    let has = |id: &str| catalog.garments.iter().any(|g| g.id == id);
    let dress =
        |c: &mut actors_generator::CharacterConfig, list: &[&str], colours: &[(&str, &str)]| {
            let o: Vec<String> = list
                .iter()
                .filter(|g| has(g))
                .map(|g| g.to_string())
                .collect();
            // Only a whole outfit: else the era's own clothes stay.
            if o.len() + 1 >= list.len() {
                c.outfit = o;
            }
            for (g, hex) in colours {
                c.garment_colors.insert(g.to_string(), hex.to_string());
            }
        };
    let b = &mut c.body;
    match act {
        Act::Prophet => {
            // A wild-bearded old man in a fisherman's sweater.
            b.gender = 1.0;
            b.age_years = rng.range(58.0, 70.0);
            b.weight = 0.3;
            b.muscle = 0.35;
            c.beard = "full".into();
            c.hair_color = Some("#d8d6cf".into());
            dress(
                &mut c,
                &[
                    "fisherman-sweater",
                    "wool-pants",
                    "work-boots",
                    "round-glasses",
                ],
                &[("fisherman-sweater", "#6e6650"), ("wool-pants", "#3c3a34")],
            );
        }
        Act::Theremin => {
            // A dinner jacket, slicked hair: a concert artist of the ether.
            b.gender = 1.0;
            b.age_years = rng.range(34.0, 48.0);
            b.weight = 0.4;
            c.beard = "none".into();
            c.hair_color = Some("#1c1712".into());
            dress(&mut c, &["suit-dinner", "leather-shoes"], &[]);
        }
        Act::Statue => {
            // A spaceman: flight suit, aviator's cap, gloves, boots (all
            // painted chrome when built).
            b.gender = if k == 0 { 1.0 } else { 0.0 };
            b.age_years = rng.range(24.0, 34.0);
            b.muscle = 0.6;
            b.weight = 0.4;
            c.beard = "none".into();
            dress(
                &mut c,
                &["worksuit", "leather-helmet", "gloves-short", "work-boots"],
                &[],
            );
        }
        Act::Trader => {
            // A Vael star trader: tall and slender, lilac skin, no hair, a
            // high domed head and kind gold eyes that glow a little, in a
            // teal travelling coat over a gold suit.
            b.gender = 0.5;
            b.age_years = rng.range(35.0, 55.0);
            b.height = 1.0;
            b.weight = 0.15;
            b.muscle = 0.3;
            c.race = actors_human::RaceId::Elf;
            c.skin = actors_generator::SkinKind::Procedural;
            c.skin_color = Some("#c6b2ea".into());
            c.eye_color = Some("#ffcc40".into());
            c.eye_glow = 0.5;
            c.hair = None;
            c.brows = None;
            c.beard = "none".into();
            c.stubble = 0.0;
            for (id, v) in [
                ("head/head-scale-vert-decr-incr", 0.5),
                ("eyes/eye-scale-decr-incr", 0.6),
            ] {
                c.details.insert(id.to_string(), v);
            }
            dress(
                &mut c,
                &["worksuit", "open-coat", "gloves-short", "black-shoes"],
                &[
                    ("worksuit", "#d8b860"),
                    ("open-coat", "#1f6a6a"),
                    ("gloves-short", "#d8b860"),
                    ("black-shoes", "#3a2a1a"),
                ],
            );
        }
        Act::Juggler => {
            b.gender = 0.0;
            b.age_years = rng.range(22.0, 30.0);
            b.weight = 0.3;
            b.muscle = 0.55;
            dress(
                &mut c,
                &["polo", "cargo-pants", "ballet-flats", "newsboy-cap"],
                &[
                    ("polo", "#e8c04a"),
                    ("cargo-pants", "#2c4a8a"),
                    ("newsboy-cap", "#c43a30"),
                    ("ballet-flats", "#c43a30"),
                ],
            );
        }
        Act::Spotter => {
            // A jumpy fellow in a denim jacket and glasses (the foil cap and
            // the binoculars are props).
            b.gender = 1.0;
            b.age_years = rng.range(36.0, 52.0);
            b.weight = 0.25;
            b.muscle = 0.3;
            c.beard = "none".into();
            c.stubble = 0.7;
            dress(
                &mut c,
                &["jacket-jeans", "work-boots", "round-glasses"],
                &[],
            );
        }
        Act::Newsboy => {
            b.gender = 1.0;
            b.age_years = 19.0;
            b.weight = 0.25;
            b.muscle = 0.45;
            c.beard = "none".into();
            dress(
                &mut c,
                &["longsleeve", "wool-pants", "work-boots", "newsboy-cap"],
                &[
                    ("longsleeve", "#e9e2cf"),
                    ("wool-pants", "#5a4630"),
                    ("newsboy-cap", "#6a5a44"),
                ],
            );
        }
        Act::Stargazer => {
            // A grey-haired lady astronomer in a thick sweater.
            b.gender = 0.0;
            b.age_years = rng.range(52.0, 64.0);
            b.weight = 0.45;
            c.hair_color = Some("#c9c6c0".into());
            dress(
                &mut c,
                &["fisherman-sweater", "skirt", "ankle-boots", "round-glasses"],
                &[("fisherman-sweater", "#2f3b5a"), ("skirt", "#4a3a2c")],
            );
        }
        Act::Dancers => {
            b.age_years = rng.range(22.0, 30.0);
            b.weight = 0.32;
            b.muscle = 0.55;
            if k == 0 {
                b.gender = 1.0;
                c.beard = "none".into();
                dress(
                    &mut c,
                    &["suit-bowtie", "leather-shoes", "fedora"],
                    &[("suit-bowtie", "#3a3f6a"), ("fedora", "#2a2622")],
                );
            } else {
                b.gender = 0.0;
                dress(
                    &mut c,
                    &["ruffle-dress", "t-bar-shoes"],
                    &[("ruffle-dress", "#c8282e"), ("t-bar-shoes", "#c8282e")],
                );
            }
        }
        Act::Club => {
            // The doorman: big, in a dinner suit.
            b.gender = 1.0;
            b.age_years = rng.range(32.0, 45.0);
            b.muscle = 0.95;
            b.weight = 0.7;
            b.height = 1.0;
            c.beard = "none".into();
            c.hair = None;
            dress(&mut c, &["suit-dinner", "black-shoes"], &[]);
        }
    }
    c
}

/// A night club guest: two men in dinner suits or double-breasted suits,
/// two women in an evening gown with long gloves or a flapper dress.
pub fn guest(builder: &CharacterBuilder, k: u8, seed: u32) -> actors_generator::CharacterConfig {
    let mut c = golden_age(builder.catalog(), seed, false);
    let mut rng = hw_core::Seed(seed as u64).child("guest").rng();
    let catalog = builder.catalog();
    let has = |id: &str| catalog.garments.iter().any(|g| g.id == id);
    let (list, colours): (&[&str], &[(&str, &str)]) = match k % 4 {
        0 => (&["suit-dinner", "leather-shoes"], &[]),
        1 => (
            &["suit-double", "leather-shoes", "fedora"],
            &[("suit-double", "#2a2f44")],
        ),
        2 => (
            &["gown", "gloves-long", "t-bar-shoes"],
            &[("gown", "#1d6b5c"), ("gloves-long", "#f2efe6")],
        ),
        _ => (
            &["flapper-dress", "t-bar-shoes", "cloche-hat"],
            &[("flapper-dress", "#d8b048"), ("cloche-hat", "#1b1b1f")],
        ),
    };
    c.body.gender = if k % 4 < 2 { 1.0 } else { 0.0 };
    c.body.age_years = rng.range(24.0, 42.0);
    if c.body.gender > 0.5 {
        c.beard = "none".into();
    }
    let o: Vec<String> = list
        .iter()
        .filter(|g| has(g))
        .map(|g| g.to_string())
        .collect();
    if o.len() + 1 >= list.len() {
        c.outfit = o;
    }
    for (g, hex) in colours {
        c.garment_colors.insert(g.to_string(), hex.to_string());
    }
    c
}

pub fn villain(
    builder: &CharacterBuilder,
    kind: VillainKind,
    look: u32,
) -> actors_generator::CharacterConfig {
    let seed = name_seed(&format!("villain {kind:?} {look}"));
    let mut c = golden_age(builder.catalog(), seed, false);
    let mut rng = hw_core::Seed(seed as u64).child("villain").rng();
    let catalog = builder.catalog();
    let has = |id: &str| catalog.garments.iter().any(|g| g.id == id);
    let keep = |list: &[&str]| -> Vec<String> {
        list.iter()
            .filter(|g| has(g))
            .map(|g| g.to_string())
            .collect()
    };
    let colour = |c: &mut actors_generator::CharacterConfig, g: &str, hex: &str| {
        c.garment_colors.insert(g.to_string(), hex.to_string());
    };
    c.hair_color = None;
    match kind {
        VillainKind::Brute => {
            let b = &mut c.body;
            b.gender = 1.0;
            b.age_years = rng.range(28.0, 45.0);
            b.muscle = 1.0;
            b.weight = rng.range(0.62, 0.8);
            b.height = rng.range(0.85, 1.0);
            b.proportions = 0.35;
            // The serum: green skin, glowing yellow eyes, small tusks.
            c.skin = actors_generator::SkinKind::Procedural;
            let greens = ["#6f9a4c", "#7aa356", "#5f8c49"];
            c.skin_color = Some(greens[(look as usize) % greens.len()].to_string());
            c.eye_color = Some("#e8ff5a".into());
            c.eye_glow = 0.7;
            c.tusks = 0.25;
            c.beard = "none".into();
            c.outfit = keep(&["tank-top", "cargo-pants", "work-boots"]);
            colour(&mut c, "tank-top", "#c9c2a6");
            colour(
                &mut c,
                "cargo-pants",
                ["#4d4a36", "#3e4a5a", "#5a3e30"][(look as usize) % 3],
            );
        }
        VillainKind::Alpha => {
            let b = &mut c.body;
            b.gender = 1.0;
            b.age_years = 40.0;
            b.muscle = 1.0;
            b.weight = 1.0;
            b.height = 1.0;
            b.proportions = 0.2;
            c.skin = actors_generator::SkinKind::Procedural;
            c.skin_color = Some("#4c7a36".into());
            c.eye_color = Some("#fff04a".into());
            c.eye_glow = 1.0;
            c.tusks = 0.7;
            c.beard = "none".into();
            c.hair = None;
            c.outfit = keep(&["tank-top", "cargo-pants", "work-boots"]);
            colour(&mut c, "tank-top", "#5a2a24");
            colour(&mut c, "cargo-pants", "#2f2b24");
            colour(&mut c, "work-boots", "#2a2622");
        }
        VillainKind::Doctor => {
            let b = &mut c.body;
            b.gender = 1.0;
            b.age_years = 64.0;
            b.muscle = 0.3;
            b.weight = 0.3;
            b.height = 1.0;
            c.hair_color = Some("#ecebe6".into());
            c.beard = "full".into();
            c.eye_color = Some("#7dff9a".into());
            c.eye_glow = 0.8;
            c.outfit = keep(&[
                "worksuit",
                "open-coat",
                "gauntlet-gloves",
                "tinted-glasses",
                "cuffed-knee-boots",
            ]);
            colour(&mut c, "open-coat", "#f2f1ea");
            colour(&mut c, "worksuit", "#18181c");
            colour(&mut c, "gauntlet-gloves", "#2a2c24");
            colour(&mut c, "cuffed-knee-boots", "#101012");
        }
        VillainKind::Bulwark => {
            let b = &mut c.body;
            b.gender = 1.0;
            b.age_years = rng.range(28.0, 45.0);
            b.muscle = 1.0;
            b.weight = rng.range(0.75, 0.9);
            b.height = rng.range(0.9, 1.0);
            b.proportions = 0.3;
            // The serum's green, like the brutes, under iron.
            c.skin = actors_generator::SkinKind::Procedural;
            let greens = ["#5f8c49", "#6f9a4c", "#577f43"];
            c.skin_color = Some(greens[(look as usize) % greens.len()].to_string());
            c.eye_color = Some("#e8ff5a".into());
            c.eye_glow = 0.7;
            c.tusks = 0.35;
            c.beard = "none".into();
            c.outfit = keep(&[
                "tank-top",
                "cargo-pants",
                "med-pauldrons",
                "gauntlet-gloves",
                "work-boots",
            ]);
            colour(&mut c, "tank-top", "#3a3630");
            colour(&mut c, "cargo-pants", "#4a4438");
            colour(&mut c, "med-pauldrons", "#6a5a4a");
            colour(&mut c, "gauntlet-gloves", "#5a4c40");
            colour(&mut c, "work-boots", "#1e1a16");
        }
        VillainKind::Grenadier => {
            let b = &mut c.body;
            b.age_years = rng.range(22.0, 40.0);
            b.muscle = rng.range(0.45, 0.65);
            b.weight = rng.range(0.4, 0.55);
            c.beard = "none".into();
            c.outfit = keep(&[
                "worksuit",
                "leather-helmet",
                "tinted-glasses",
                "gloves-medium",
                "med-belt",
                "work-boots",
            ]);
            colour(&mut c, "worksuit", "#545c3a");
            colour(&mut c, "leather-helmet", "#3a2a1c");
            colour(&mut c, "gloves-medium", "#2e2418");
            // The bandolier: brass.
            colour(&mut c, "med-belt", "#b08a3c");
            colour(&mut c, "work-boots", "#1e1a16");
        }
        VillainKind::Zapper | VillainKind::Grabber | VillainKind::Overlord => {
            moon_invader(&mut c, kind, look, &mut rng);
            let suit = match kind {
                VillainKind::Overlord => vec![
                    "worksuit",
                    "open-coat",
                    "gauntlet-gloves",
                    "cuffed-knee-boots",
                ],
                _ => vec![
                    "worksuit",
                    "gauntlet-gloves",
                    "cuffed-knee-boots",
                    "med-belt",
                ],
            };
            c.outfit = keep(&suit);
            // Silver flight suits, red gauntlets and boots: Zorrak's livery.
            colour(
                &mut c,
                "worksuit",
                ["#b9bec8", "#aeb6c2", "#c4c6cc"][(look as usize) % 3],
            );
            let trim = if kind == VillainKind::Grabber {
                "#c8a040"
            } else {
                "#b8241c"
            };
            colour(&mut c, "gauntlet-gloves", trim);
            colour(&mut c, "cuffed-knee-boots", trim);
            colour(&mut c, "med-belt", "#2a2c34");
            colour(&mut c, "open-coat", "#4a1c6a");
        }
        VillainKind::Orderly => {
            let b = &mut c.body;
            b.age_years = rng.range(24.0, 50.0);
            b.muscle = rng.range(0.3, 0.45);
            b.weight = rng.range(0.35, 0.5);
            c.outfit = keep(&["worksuit", "gloves-short", "round-glasses", "black-shoes"]);
            // Mint scrubs: the one in pale green is the medic.
            colour(&mut c, "worksuit", "#a8dcc4");
            colour(&mut c, "gloves-short", "#f2f2ee");
            colour(&mut c, "black-shoes", "#eeeeea");
        }
        VillainKind::Technician => {
            let b = &mut c.body;
            b.age_years = rng.range(24.0, 55.0);
            b.muscle = rng.range(0.3, 0.5);
            b.weight = rng.range(0.35, 0.55);
            c.outfit = keep(&[
                "worksuit",
                "open-coat",
                "gloves-short",
                "tinted-glasses",
                "black-shoes",
            ]);
            colour(&mut c, "open-coat", "#ecebe2");
            colour(&mut c, "worksuit", "#3a3f48");
            colour(&mut c, "gloves-short", "#2a2a2a");
        }
        VillainKind::Greaser => {
            let b = &mut c.body;
            b.gender = 1.0;
            b.age_years = rng.range(18.0, 30.0);
            b.muscle = rng.range(0.55, 0.8);
            b.weight = rng.range(0.4, 0.55);
            c.beard = "none".into();
            c.hair_color = Some(["#1d1a18", "#2b2018", "#3a2a1c"][(look as usize) % 3].into());
            c.outfit = keep(&["jacket-jeans", "biker-boots"]);
            if look.is_multiple_of(2) {
                c.outfit.extend(keep(&["sunglasses"]));
            }
            // Black leather with a chrome-blue denim.
            colour(&mut c, "jacket-jeans", "#1b1b1f");
            colour(&mut c, "biker-boots", "#141414");
        }
        VillainKind::Gunsel => {
            let b = &mut c.body;
            b.gender = 1.0;
            b.age_years = rng.range(26.0, 50.0);
            b.muscle = rng.range(0.45, 0.65);
            b.weight = rng.range(0.45, 0.6);
            let suit = ["suit-double", "suit-3", "suit-tie"][(look as usize) % 3];
            c.outfit = keep(&[suit, "fedora-tilted", "black-shoes", "gloves-short"]);
            // Deep plum velvet: the Syndicate's colour.
            colour(
                &mut c,
                suit,
                ["#3b1f3a", "#2c2233", "#46243f"][(look as usize) % 3],
            );
            colour(&mut c, "fedora-tilted", "#1e1a1e");
            colour(&mut c, "gloves-short", "#141214");
        }
        VillainKind::Delver => {
            // One of the mole folk, kitted out for the tunnels.
            let mole = mole_person(builder, look + 7);
            c.body = mole.body;
            c.skin = mole.skin;
            c.skin_color = mole.skin_color;
            c.eye_color = mole.eye_color;
            c.eye_glow = mole.eye_glow;
            c.hair_color = mole.hair_color;
            c.body.gender = if look == 1 { 0.0 } else { 1.0 };
            c.body.muscle = rng.range(0.65, 0.85);
            c.beard = if look == 2 {
                "full".into()
            } else {
                "none".into()
            };
            c.outfit = keep(&[
                "worksuit",
                "leather-helmet",
                "tinted-glasses",
                "gauntlet-gloves",
                "work-boots",
            ]);
            colour(
                &mut c,
                "worksuit",
                ["#5a3f28", "#4b3a2a", "#634529"][(look as usize) % 3],
            );
            colour(&mut c, "leather-helmet", "#3b2a1c");
            colour(&mut c, "gauntlet-gloves", "#2e231a");
        }
        VillainKind::Buccaneer => {
            let b = &mut c.body;
            b.gender = if look == 2 { 0.0 } else { 1.0 };
            b.age_years = rng.range(20.0, 40.0);
            b.muscle = rng.range(0.5, 0.75);
            b.weight = rng.range(0.35, 0.55);
            c.hair_color = Some(["#5a2e18", "#1f1a16", "#a8662c"][(look as usize) % 3].into());
            c.beard = if look == 0 {
                "stubble".into()
            } else {
                "none".into()
            };
            c.outfit = keep(&[
                "striped-shirt-jeans",
                "leather-helmet",
                "tinted-glasses",
                "gloves-short",
                "pirate-boots",
            ]);
            colour(&mut c, "leather-helmet", "#6a4426");
            colour(&mut c, "gloves-short", "#4a2e1a");
            colour(&mut c, "pirate-boots", "#2a1a10");
        }
        VillainKind::Gunner => {
            let b = &mut c.body;
            b.gender = if look == 1 { 0.0 } else { 1.0 };
            b.age_years = rng.range(22.0, 45.0);
            b.muscle = rng.range(0.4, 0.6);
            b.weight = rng.range(0.4, 0.55);
            c.hair_color = Some(["#2a1c14", "#c08a4a", "#6a3a20"][(look as usize) % 3].into());
            c.outfit = keep(&[
                "open-coat",
                "tucked-tshirt",
                "wool-pants",
                "gloves-medium",
                "tinted-glasses",
                "pirate-boots",
            ]);
            colour(
                &mut c,
                "open-coat",
                ["#5b2420", "#4a2a1c", "#3c2a3e"][(look as usize) % 3],
            );
            colour(&mut c, "tucked-tshirt", "#e8e0cc");
            colour(&mut c, "wool-pants", "#3a3228");
            colour(&mut c, "gloves-medium", "#3a2416");
            colour(&mut c, "pirate-boots", "#22160e");
        }
        VillainKind::Captain => {
            let b = &mut c.body;
            b.gender = 1.0;
            b.age_years = 46.0;
            b.muscle = 0.75;
            b.weight = 0.6;
            b.height = 0.9;
            // Under the hat: no hair poking through its crown.
            c.hair = None;
            c.hair_color = Some("#7a3a1a".into());
            c.beard = "full".into();
            c.outfit = keep(&[
                "open-coat",
                "striped-shirt-jeans",
                "wide-brim-hat",
                "gauntlet-gloves",
                "pirate-boots",
            ]);
            colour(&mut c, "open-coat", "#1c2848");
            colour(&mut c, "wide-brim-hat", "#16120e");
            colour(&mut c, "gauntlet-gloves", "#3a2414");
            colour(&mut c, "pirate-boots", "#120c08");
        }
        VillainKind::Legionary => {
            let b = &mut c.body;
            b.gender = 1.0;
            b.age_years = rng.range(22.0, 40.0);
            b.muscle = rng.range(0.6, 0.8);
            b.weight = 0.5;
            b.height = rng.range(0.6, 0.8);
            c.beard = "none".into();
            c.outfit = keep(&[
                "worksuit",
                "corinthian-helmet",
                "med-cuirass",
                "med-pauldrons",
                "gauntlet-gloves",
                "cuffed-knee-boots",
                "med-belt",
            ]);
            // Blackened iron over gunmetal: the Empire's livery.
            colour(&mut c, "worksuit", "#25262b");
            // The iron mask: a closed helmet hiding the face.
            colour(&mut c, "corinthian-helmet", "#4a4d54");
            colour(&mut c, "med-cuirass", "#3c3f46");
            colour(&mut c, "med-pauldrons", "#5a5d64");
            colour(&mut c, "gauntlet-gloves", "#4c4f56");
            colour(&mut c, "cuffed-knee-boots", "#141416");
            colour(&mut c, "med-belt", "#7a1418");
        }
        VillainKind::Praetorian => {
            let b = &mut c.body;
            b.gender = 1.0;
            b.age_years = 38.0;
            b.muscle = 1.0;
            b.weight = 0.65;
            b.height = 1.0;
            b.proportions = 0.4;
            c.beard = "none".into();
            c.outfit = keep(&[
                "worksuit",
                "corinthian-helmet",
                "med-cuirass",
                "med-pauldrons",
                "gauntlet-gloves",
                "med-greaves",
                "cuffed-knee-boots",
            ]);
            colour(&mut c, "worksuit", "#4a0e12");
            colour(&mut c, "corinthian-helmet", "#a08848");
            colour(&mut c, "med-cuirass", "#5c5f66");
            colour(&mut c, "med-pauldrons", "#a08848");
            colour(&mut c, "gauntlet-gloves", "#3e4046");
            colour(&mut c, "med-greaves", "#5c5f66");
        }
    }
    c
}

/// An invader from Zorrak, the small moon: a slight body, pale teal or
/// lilac skin with no hair, a great domed head and big glossy black eyes
/// with no whites. The overlord's head is bigger still.
fn moon_invader(
    c: &mut actors_generator::CharacterConfig,
    kind: VillainKind,
    look: u32,
    rng: &mut hw_core::Rng,
) {
    let b = &mut c.body;
    b.gender = 0.5;
    b.age_years = rng.range(30.0, 45.0);
    b.muscle = if kind == VillainKind::Grabber {
        0.6
    } else {
        0.25
    };
    b.weight = if kind == VillainKind::Grabber {
        0.45
    } else {
        0.2
    };
    b.height = match kind {
        VillainKind::Grabber => 0.9,
        VillainKind::Overlord => 0.75,
        _ => rng.range(0.25, 0.45),
    };
    b.proportions = 0.8;
    c.skin = actors_generator::SkinKind::Procedural;
    let skins = ["#9fd8c8", "#b4a8e0", "#a8dcb0"];
    c.skin_color = Some(skins[(look as usize) % skins.len()].to_string());
    c.eye_color = Some("#0a0812".into());
    c.pupil = "none".into();
    c.eye_glow = 0.0;
    c.hair = None;
    c.brows = None;
    c.beard = "none".into();
    c.stubble = 0.0;
    let head = if kind == VillainKind::Overlord {
        1.0
    } else {
        0.7
    };
    for (id, v) in [
        ("head/head-scale-vert-decr-incr", head),
        ("head/head-scale-horiz-decr-incr", head * 0.8),
        ("head/head-scale-depth-decr-incr", head * 0.6),
        ("eyes/eye-scale-decr-incr", 1.0),
    ] {
        c.details.insert(id.to_string(), v);
    }
}

/// One of Molehaven's people: short and stocky, pale from a life
/// underground, big dark goggles against the light, in earth-brown work
/// clothes.
pub fn mole_person(builder: &CharacterBuilder, look: u32) -> actors_generator::CharacterConfig {
    let seed = name_seed(&format!("mole person {look}"));
    let mut c = golden_age(builder.catalog(), seed, false);
    let mut rng = hw_core::Seed(seed as u64).child("mole").rng();
    let catalog = builder.catalog();
    let has = |id: &str| catalog.garments.iter().any(|g| g.id == id);
    let b = &mut c.body;
    b.height = rng.range(0.0, 0.15);
    b.weight = rng.range(0.6, 0.8);
    b.muscle = rng.range(0.5, 0.7);
    b.proportions = 0.3;
    b.age_years = rng.range(25.0, 70.0);
    c.skin = actors_generator::SkinKind::Procedural;
    let skins = ["#e6c9c3", "#dcbcb8", "#d9c4b4", "#e8d2c8"];
    c.skin_color = Some(skins[look as usize % skins.len()].to_string());
    c.eye_color = Some("#c9a0ff".into());
    c.eye_glow = 0.35;
    c.hair_color = Some("#d8d4cc".into());
    c.outfit = ["worksuit", "tinted-glasses", "gloves-short", "work-boots"]
        .iter()
        .filter(|g| has(g))
        .map(|g| g.to_string())
        .collect();
    let browns = ["#6b4a2e", "#5a4632", "#7a5a3a", "#4e3b2a"];
    c.garment_colors.insert(
        "worksuit".into(),
        browns[look as usize % browns.len()].into(),
    );
    c.garment_colors
        .insert("gloves-short".into(), "#3a2c20".into());
    c
}

/// Citizens' maps are halved down to no smaller than this (texels).
const CROWD_MAP_MIN: u32 = 512;

/// Citizens' and villains' half-size maps, by the texels of the full-size
/// map they came from: characters that wear the same garment texture (or
/// eyes, teeth, hair) share one half-size copy instead of each holding its
/// own. Kept with the character builder, and only the half-size maps (which
/// the characters hold anyway); emptied when it grows past [`Halves::MAX`].
#[derive(Default)]
pub struct Halves(BTreeMap<(u64, u32, u32), AlbedoMap>);

impl Halves {
    const MAX: usize = 512;

    /// `map` at half size (see [`halve`]), shared with any character that
    /// was given a map with the same texels.
    fn halve(&mut self, map: &mut AlbedoMap, min: u32) {
        let key = (map.rgba.content_hash(), map.width, map.height);
        if let Some(half) = self.0.get(&key) {
            *map = half.clone();
            return;
        }
        halve(map, min);
        if self.0.len() >= Self::MAX {
            self.0.clear();
        }
        self.0.insert(key, map.clone());
    }
}

/// Halves `map` (2x2 box filter) if both sides stay at least `min`.
fn halve(map: &mut AlbedoMap, min: u32) {
    if map.width / 2 >= min && map.height / 2 >= min {
        let (w, h) = (map.width / 2, map.height / 2);
        let src = map.width as usize;
        let mut out = vec![0u8; (w * h * 4) as usize];
        for y in 0..h as usize {
            for x in 0..w as usize {
                for c in 0..4 {
                    let at = |dx: usize, dy: usize| {
                        map.rgba[((y * 2 + dy) * src + x * 2 + dx) * 4 + c] as u32
                    };
                    let sum = at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1);
                    out[(y * w as usize + x) * 4 + c] = ((sum + 2) / 4) as u8;
                }
            }
        }
        *map = AlbedoMap {
            width: w,
            height: h,
            rgba: out.into(),
        };
    }
}

/// The bytes of a model's texture maps, as decoded.
pub fn map_bytes(model: &AnimatedModel) -> usize {
    model
        .meshes
        .iter()
        .flat_map(|m| {
            [
                m.albedo.as_ref(),
                m.maps.normal.as_ref(),
                m.maps.orm.as_ref(),
                m.maps.emissive.as_ref(),
            ]
        })
        .flatten()
        .map(|a| a.rgba.len())
        .sum()
}

/// Builds one character (on the worker); `config` is a made hero's looks.
pub fn build(
    builder: &mut CharacterBuilder,
    who: &Who,
    seed: u64,
    config: Option<&CharacterConfig>,
) -> Result<Built, String> {
    build_sharing(builder, &mut Halves::default(), who, seed, config)
}

/// [`build`], with half-size maps shared through `halves` (keep it as long
/// as the builder).
pub fn build_sharing(
    builder: &mut CharacterBuilder,
    halves: &mut Halves,
    who: &Who,
    seed: u64,
    config: Option<&CharacterConfig>,
) -> Result<Built, String> {
    let (hero, rseed) = match who {
        Who::Hero(name) => (true, name_seed(name)),
        Who::Look(key) | Who::Statue(key) => (true, *key as u32),
        Who::Citizen(look) => (
            false,
            (hw_core::Seed(seed).child("look").child(*look as u64).0 >> 32) as u32,
        ),
        Who::Villain(..) => (false, 0),
        Who::NpcHero(i) => (true, 7_000 + *i as u32),
        Who::Sidekick(theme) => (false, 9_000 + *theme as u32),
        Who::Medic => (false, 9_500),
        Who::Street(act, k) => (false, name_seed(&format!("street {act:?} {k}"))),
        Who::Guest(k) => (false, name_seed(&format!("club guest {k}"))),
    };
    let mut allowed = allow(hero);
    if matches!(who, Who::Sidekick(_)) {
        allowed.races = Some(Flags::List(vec!["human".into(), "robot".into()]));
    }
    if matches!(who, Who::Street(..) | Who::Guest(_)) {
        // Costumes (a dinner jacket, an aviator's cap): picked by hand,
        // all of the era, from the whole wardrobe.
        allowed.clothing = None;
    }
    if let Who::Villain(kind, _) = who {
        if kind.any_clothes() {
            // Helms, breastplates, pirate boots: the whole wardrobe.
            allowed.clothing = None;
        }
    }
    builder.allow(&allowed);
    let config = match (who, config) {
        (Who::Villain(kind, look), _) => villain(builder, *kind, *look),
        (Who::NpcHero(i), _) => npc_hero(builder, *i),
        (Who::Sidekick(theme), _) => sidekick(builder, *theme),
        (Who::Medic, _) => medic(builder),
        (Who::Street(act, k), _) => street(builder, *act, *k, rseed),
        (Who::Guest(k), _) => guest(builder, *k, rseed),
        (Who::Citizen(look), _) if *look >= hw_sim::citizens::LOOKS => {
            mole_person(builder, *look - hw_sim::citizens::LOOKS)
        }
        // Made looks, from this PC or a friend's: kept to the hero rules.
        (_, Some(c)) => crate::looks::hero_rules(&hero_catalog(builder.catalog()), c),
        _ => golden_age(builder.catalog(), rseed, hero),
    };
    let clips: &[&str] = match who {
        Who::Hero(_) | Who::Look(_) | Who::NpcHero(_) | Who::Statue(_) => HERO_CLIPS,
        Who::Villain(..) => VILLAIN_CLIPS,
        Who::Sidekick(_) => SIDEKICK_CLIPS,
        Who::Citizen(_) | Who::Medic => CITIZEN_CLIPS,
        Who::Street(..) | Who::Guest(_) => STREET_CLIPS,
    };
    let options = BuildOptions {
        clips: clips.iter().map(|s| s.to_string()).collect(),
        // Citizens are many and mostly far: fewer skin layers build faster.
        skin_layers: if hero { 3 } else { 2 },
        ..BuildOptions::default()
    };
    let mut c = builder
        .build(&config, &options)
        .map_err(|e| format!("building {who:?}: {e}"))?;
    let walk = if !hero && rseed % 5 == 0 {
        "walk formal"
    } else {
        "walk"
    };
    if hero {
        poses::add_flight_clips(&mut c.model).map_err(|e| format!("{who:?}: {e}"))?;
        poses::add_heroic_clip(&mut c.model).map_err(|e| format!("{who:?}: {e}"))?;
        poses::add_power_pose_clip(&mut c.model).map_err(|e| format!("{who:?}: {e}"))?;
        // A hand to the temple to switch the police visor.
        poses::add_visor_tap_clips(&mut c.model).map_err(|e| format!("{who:?}: {e}"))?;
        // Carrying someone hurt to the ambulance.
        poses::add_carry_clips(&mut c.model).map_err(|e| format!("{who:?}: {e}"))?;
        poses::swing_cape(&mut c);
    }
    if matches!(who, Who::Villain(..)) {
        poses::add_cuffed_clip(&mut c.model).map_err(|e| format!("{who:?}: {e}"))?;
    }
    if matches!(who, Who::Citizen(_)) {
        poses::add_hands_up_clip(&mut c.model).map_err(|e| format!("{who:?}: {e}"))?;
        // Drivers and passengers (see `riderview`).
        poses::add_seated_clips(&mut c.model).map_err(|e| format!("{who:?}: {e}"))?;
    }
    if matches!(who, Who::Citizen(_) | Who::Street(..) | Who::Guest(_)) {
        // The street acts and their audiences (see `streetview`).
        poses::add_street_clips(&mut c.model).map_err(|e| format!("{who:?}: {e}"))?;
    }
    if matches!(who, Who::Citizen(_) | Who::Medic) {
        // Trapped, hurt and carried at rescues (see `radioview`).
        poses::add_rescue_clips(&mut c.model).map_err(|e| format!("{who:?}: {e}"))?;
    }
    if !hero {
        // Citizens and villains are seen from a few metres at the nearest,
        // and every look is held several times over (its crowd, each full
        // character): half-size maps keep a town's people to a quarter of
        // the memory. Heroes keep theirs.
        for m in &mut c.model.meshes {
            for map in [
                m.albedo.as_mut(),
                m.maps.normal.as_mut(),
                m.maps.orm.as_mut(),
                m.maps.emissive.as_mut(),
            ]
            .into_iter()
            .flatten()
            {
                halves.halve(map, CROWD_MAP_MIN);
            }
        }
    }
    if matches!(who, Who::Statue(_)) {
        for m in &mut c.model.meshes {
            bronze(m);
        }
    }
    if matches!(who, Who::Street(hw_sim::streetlife::Act::Statue, _)) {
        // The living statue: a spaceman painted chrome from cap to boots.
        for m in &mut c.model.meshes {
            chrome(m);
        }
    }
    let sits = matches!(who, Who::Citizen(_)) && crate::riderview::fits_seats(&c.model);
    Ok(Built {
        who: who.clone(),
        model: Arc::new(c.model),
        walk: walk.into(),
        sits,
    })
}

/// Casts a character's mesh in bronze: its colours become shades of bronze
/// by their lightness (so the costume's shapes and the face still read),
/// metal all over, a satin polish, no glow.
pub fn bronze(m: &mut engine::anim::SkinMesh) {
    // sRGB bronze, dark to light.
    metal_cast(m, [58.0, 36.0, 18.0], [214.0, 160.0, 92.0], 64);
}

/// Paints a character's mesh silver all over, like a living statue: its
/// colours become shades of chrome by their lightness, a high polish.
pub fn chrome(m: &mut engine::anim::SkinMesh) {
    metal_cast(m, [70.0, 74.0, 82.0], [236.0, 240.0, 246.0], 30);
}

/// [`bronze`] and [`chrome`]: colours to shades between `dark` and `light`
/// (sRGB) by their lightness, full metal, roughness from `rough` (of 255)
/// up by a quarter of the map's.
fn metal_cast(m: &mut engine::anim::SkinMesh, dark: [f32; 3], light: [f32; 3], rough: u8) {
    let (dark_c, light_c) = (dark, light);
    let cast = |r: u8, g: u8, b: u8| -> [u8; 3] {
        let l = (0.3 * r as f32 + 0.59 * g as f32 + 0.11 * b as f32) / 255.0;
        let t = l.powf(0.8);
        let mix = |k: usize| (dark_c[k] + (light_c[k] - dark_c[k]) * t).round() as u8;
        [mix(0), mix(1), mix(2)]
    };
    if let Some(a) = m.albedo.as_mut() {
        let mut px: Vec<u8> = a.rgba.to_vec();
        for p in px.as_chunks_mut::<4>().0 {
            let c = cast(p[0], p[1], p[2]);
            p[..3].copy_from_slice(&c);
        }
        a.rgba = px.into();
    }
    for c in &mut m.colors {
        let k = cast(
            (c.x.clamp(0.0, 1.0) * 255.0) as u8,
            (c.y.clamp(0.0, 1.0) * 255.0) as u8,
            (c.z.clamp(0.0, 1.0) * 255.0) as u8,
        );
        c.x = k[0] as f32 / 255.0;
        c.y = k[1] as f32 / 255.0;
        c.z = k[2] as f32 / 255.0;
    }
    if let Some(orm) = m.maps.orm.as_mut() {
        let mut px: Vec<u8> = orm.rgba.to_vec();
        for p in px.as_chunks_mut::<4>().0 {
            // Occlusion kept; roughness up from `rough`; full metal.
            p[1] = rough + p[1] / 4;
            p[2] = 255;
        }
        orm.rgba = px.into();
    }
    m.maps.emissive = None;
    m.maps.emissive_factor = [0.0; 3];
    m.maps.metallic = 1.0;
    m.maps.roughness = (rough as f32 / 255.0 + 0.15).min(1.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "on demand (looks): scripts/check.sh --on-demand"]
    fn bronze_keeps_light_and_dark_apart() {
        let mut m = engine::anim::SkinMesh {
            positions: vec![],
            normals: vec![],
            colors: vec![],
            uvs: vec![],
            joints: vec![],
            weights: vec![],
            indices: vec![],
            double_sided: false,
            albedo: Some(AlbedoMap {
                width: 2,
                height: 1,
                rgba: vec![10, 10, 10, 255, 250, 250, 250, 255].into(),
            }),
            maps: Default::default(),
            morphs: vec![],
        };
        bronze(&mut m);
        let a = m.albedo.unwrap();
        assert!(a.rgba[0] < a.rgba[4], "dark stays darker");
        assert!(a.rgba[4] > a.rgba[6], "bronze: more red than blue");
        assert_eq!(m.maps.metallic, 1.0);
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn citizen_maps_come_down_by_half_but_not_below_the_floor() {
        let mut big = AlbedoMap {
            width: 4,
            height: 2,
            rgba: [[0u8, 100, 200, 255], [255, 100, 0, 255]]
                .iter()
                .cycle()
                .take(8)
                .flatten()
                .copied()
                .collect(),
        };
        halve(&mut big, 1);
        assert_eq!((big.width, big.height), (2, 1));
        assert_eq!(&big.rgba[..4], &[128, 100, 100, 255]);
        let mut small = AlbedoMap {
            width: 2,
            height: 2,
            rgba: vec![9; 16].into(),
        };
        halve(&mut small, 2);
        assert_eq!((small.width, small.height, small.rgba.len()), (2, 2, 16));
    }
}
