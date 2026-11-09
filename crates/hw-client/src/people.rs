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
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

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
/// mugging's victim) is made here.
pub const CITIZEN_CLIPS: &[&str] = &["idle", "walk", "walk formal"];

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
    /// The mutant alpha who leads a rampage: the heaviest brute there is,
    /// darker, bald, with long tusks and blazing eyes (drawn a head and a
    /// half taller). One look, built when an alpha first shows.
    Alpha,
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
        )
    }

    /// Looks it comes in (bosses and lieutenants have one).
    pub fn looks(self) -> u32 {
        match self {
            VillainKind::Alpha | VillainKind::Captain | VillainKind::Praetorian => 1,
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
                let t0 = std::time::Instant::now();
                let mut builder = match CharacterBuilder::load(&packs) {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = tx.send(Err(format!("loading {}: {e}", packs.display())));
                        return;
                    }
                };
                eprintln!("Actors packs loaded in {:.1} s", t0.elapsed().as_secs_f64());
                // What heroes may wear, for the creator.
                builder.allow(&allow(true));
                let _ = cat_tx.send(hero_catalog(builder.catalog()));
                // Heroes before villains before citizens, whatever order
                // they were asked in.
                let mut queue: Vec<Job> = Vec::new();
                loop {
                    while let Ok(j) = rx.try_recv() {
                        queue.push(j);
                    }
                    if queue.is_empty() {
                        match rx.recv() {
                            Ok(j) => queue.push(j),
                            Err(_) => return,
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
                    let t = std::time::Instant::now();
                    let r = build(&mut builder, &job.who, job.seed, job.config.as_ref());
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
            rgba: out,
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
    let (hero, rseed) = match who {
        Who::Hero(name) => (true, name_seed(name)),
        Who::Look(key) => (true, *key as u32),
        Who::Citizen(look) => (
            false,
            (hw_core::Seed(seed).child("look").child(*look as u64).0 >> 32) as u32,
        ),
        Who::Villain(..) => (false, 0),
    };
    let mut allowed = allow(hero);
    if let Who::Villain(kind, _) = who {
        if kind.any_clothes() {
            // Helms, breastplates, pirate boots: the whole wardrobe.
            allowed.clothing = None;
        }
    }
    builder.allow(&allowed);
    let config = match (who, config) {
        (Who::Villain(kind, look), _) => villain(builder, *kind, *look),
        (Who::Citizen(look), _) if *look >= hw_sim::citizens::LOOKS => {
            mole_person(builder, *look - hw_sim::citizens::LOOKS)
        }
        // Made looks, from this PC or a friend's: kept to the hero rules.
        (_, Some(c)) => crate::looks::hero_rules(&hero_catalog(builder.catalog()), c),
        _ => golden_age(builder.catalog(), rseed, hero),
    };
    let clips: &[&str] = match who {
        Who::Hero(_) | Who::Look(_) => HERO_CLIPS,
        Who::Villain(..) => VILLAIN_CLIPS,
        Who::Citizen(_) => CITIZEN_CLIPS,
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
    }
    if matches!(who, Who::Villain(..)) {
        poses::add_cuffed_clip(&mut c.model).map_err(|e| format!("{who:?}: {e}"))?;
    }
    if matches!(who, Who::Citizen(_)) {
        poses::add_hands_up_clip(&mut c.model).map_err(|e| format!("{who:?}: {e}"))?;
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
                halve(map, CROWD_MAP_MIN);
            }
        }
    }
    Ok(Built {
        who: who.clone(),
        model: Arc::new(c.model),
        walk: walk.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
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
            rgba: vec![9; 16],
        };
        halve(&mut small, 2);
        assert_eq!((small.width, small.height, small.rgba.len()), (2, 2, 16));
    }
}
