# HeroWorld game guide

What the game is and does **today**, read from the code on `main` at version 0.069
(2026-11-08; powers and levels updated for 0.077, five new powers and the power tray for 0.117). [DESIGN.md](DESIGN.md) says what HeroWorld is meant to become and
[MILESTONES.md](MILESTONES.md) how we get there; this guide says what is actually built,
with the numbers the code uses, and marks what is still only planned. Several threads are
changing the game in parallel (sounds, autorun and minimap, the level cap, the costume
creator), so check `WHATS_NEW.md` for anything newer than 0.069.

Where a rule lives in the code it is named in `code` so it can be found and changed.

---

## 1. The game in one page

You play a hero in a procedurally generated golden-age town (1950s Googie and streamline
retro-future, art deco towers, old towns at the core). You start as a nobody on foot with
enough karma for one power. Trouble comes to you over the wrist radio: muggings, robberies,
mutant rampages, lab breakouts, fires, car crashes, collapsed buildings and, now and then,
the mad scientist's walking tank. Beating villains and rescuing people earns karma; lifetime
karma sets your level (1 to 20), unspent karma buys powers. Almost everything breaks:
buildings element by element, street props, moving cars. The town repairs itself where
nobody is looking. Nobody dies: villains are cuffed and lifted away by police drones, heroes
are revived by friends or flown to hospital by a medical drone.

You can play alone (the main case) or host up to 10 friends over the internet with a room
code. The game runs a full server inside your client; friends join it.

**Built:** one town per world seed, with everything in this guide.
**Not built yet:** the planet (continents, many cities, travel between them), a day/night
cycle (the hour is fixed), NPC heroes, super speed, collateral costs, dystopia liberation.
See section 14.

---

## 2. Starting out

### Title screen (`hw-client/src/title.rs`)

The town at golden hour, a camera drifting round the Aether Spire, flying taxis crossing,
golden HEROWORLD letters. Buttons: **Play / Continue**, **New hero**, **Your city**,
**Play with friends** (host your world, or join by room code), **Settings**, **What's new**,
**Quit**. The town builds behind the title with a loading bar; pressing Play early starts the
moment it is ready.

**Your city** (also "Change city" in New hero) opens a window: the world number and the
city's size, Hamlet (300 people), Village (3 000), Town (15 000), City (40 000, the default),
Big city (250 000), Metropolis (1 million), Megalopolis (2 million). Nothing is built while the
window is open; **OK** saves the choice in the profile and starts planning that city behind
the title (`citybuild.rs`). Picking again before it is done cancels the plan on its way (a
million-person plan stops within a second) and the next starts only once it has; only the
last choice is ever used. The city you last played is built at start; one bigger than the
default starts the title on a hamlet behind its cover so the window opens at once. Your hero poses on the title; click them to change their looks. A crash report from
the last run is pointed out once on the title.

### New hero

- **Name and world:** a name and a world number (the seed), or "surprise me".
- **Looks:** the hero creator (`creator.rs`, on Actors' Rust generator): body, face, hair
  and costume in a few clicks, or every slider in the detailed view; a dice button. Six
  golden-age costumes to start from: circus strongman, mystery man, sky ace, rocket ranger,
  jungle queen, silver-screen sleuth, in any colours. A hero kit adds a cape, mask, belt
  and chest emblem. A coloured suit piece without a finish of its own is dyed one colour:
  tights, tops and briefs in satin (spandex), gloves and boots in leather
  (`looks::hero_rules`), so no garment's print shows through a costume.
- **How a hero moves** (`heroview.rs`, `poses.rs`): flying fast, the body lies flat with one
  fist ahead and banks into turns like a plane (up to 55 degrees, steeper the faster and
  tighter the turn); slower than 4 m/s it hovers upright, legs together, arms easy at its
  sides. A level up strikes a 2.6 s power pose, a fist to the sky, standing or hovering.
- **Origin** (`progress::ORIGINS`), picked once. It gives 600 points to one affinity
  (section 4):

| Origin | Blurb | Affinity |
| --- | --- | --- |
| Lab Accident | A beaker of raw Aether went off in your face. Science! | Science |
| Ancient Relic | You found an amulet older than the moon. It found you back. | Mystic |
| Born Mighty | You lifted the family car at six. Mostly by accident. | Might |
| Sworn Guardian | You promised someone you would keep this city safe. | Guardian |
| Masked Avenger | The mad scientist's gang crossed the wrong family. | Justice |

- **Start:** 960 karma (exactly one tier-1 power), level 1, no powers except Strike. A hero
  below level 5 (before Flight opens) starts on the **rookies' street**: the side street
  nearest the town centre that lies 120-300 m from a Chrome Jackets hangout and at least
  180 m out of the mad scientist's quarter (and clear of the pirates' landing), off the
  gang's turf, so nobody warns a newcomer off on arrival (`quarter::rookie_start`). Small
  crime is close and frequent there (see Incidents). From level 5, or when a friend of level
  5 or more is already playing, heroes appear on the spawn line near the town centre, beside
  a stack of wooden crates that tumble when you run into them. A town without the quarter
  (a small one) has no Jackets: everyone starts at the centre. A hero placed by a view or
  the debug menu before their hero file arrives stays where they were put.

### What the PC remembers (`profile.rs`)

A small JSON file (`%APPDATA%\HeroWorld` on Windows, `~/.config/heroworld` elsewhere):
name, world seed, the hero file (karma, lifetime karma, affinities, origin, powers and
themes, faction regard), looks, volumes, picture and mouse settings. The hero is always
kept on the player's own PC; joining a friend brings your hero into their world.

The game client keeps **no world save** between runs in single player: damage, broken
props and the clock start fresh each time (the town repairs itself anyway). The dedicated
server `hw-headless` keeps one with `HW_SAVE`, and a co-op host publishes one every 10 s
for host migration (`hw-sim/src/save.rs`).

---

## 3. Controls

From `settings::CONTROLS` (shown in the game under Escape → Controls) and `fightview.rs`:

| Key | What it does |
| --- | --- |
| Right mouse (hold) / P | Look around (P locks the pointer) |
| W A S D | Run |
| R | Autorun: the mouse steers, A/D step aside, S or R stops |
| Space | Jump; climb when flying or in a lift |
| V | Fly (once you own Flight); V again drops you |
| Ctrl / Shift | Sink / faster when flying (Ctrl also sinks in a lift); Shift also runs at full tilt with Super Speed |
| Mouse wheel | Zoom in / out (all the way in: first person) |
| Tab | Target the nearest villain in view; again: the next nearest not yet picked, round and round |
| Left mouse | Strike |
| Q | Energy Bolt |
| 1 to 0, Alt+1 to Alt+0 | The power in that slot of the power tray (see below) |
| B | Powers and karma window (also faction standings) |
| E (hold) | Revive a downed teammate; help trapped people out; pick up the hurt |
| H | Call the medical drone when you are down |
| G | Hail a flying taxi (G again hops out mid-air) |
| M | Town map with the radio's calls |
| N | Minimap on or off |
| O | Free camera |
| K | Knock nearby townspeople over (a ragdoll test, still in the game) |
| Escape | Menu: back, settings, controls, quit; frees a locked mouse first |

On foot the hero steps up and down ledges up to 0.45 m (`collision::STEP`) without
leaving the ground, so slopes, kerbs and stairs never count as a fall; a bigger drop
does. The fall pose shows only after 0.15 s off the ground (`heroview::AIR_GRACE`),
a jump at once.

**The power tray** (`hw-client/src/tray.rs`): twenty slots at the bottom of the screen, 1 to 0
and Alt+1 to Alt+0 (the Alt row shows above the number row once it holds anything, or while
something is dragged). A power you buy takes its usual key if that is free (Strike 1, Bolt 2,
Hold 3, Shield 4, Lift 5, Shockwave 6, Mend 7, Beam 8, Meteor 9, Nova 0), otherwise the first
free slot. Rearrange it with the mouse: drag a power from the powers window (B, the "⠿" handle)
onto a slot, drag a slot onto another (they swap), or drag a slot off the tray (it stays off
until you drag it back). Clicking a slot fires its power. The tray is saved with the profile on
this PC; passive powers never go on it.

Without a target, left mouse punches what is in front of you (walls, props, cars) and Q
fires a blast down the crosshair (up to 90 m), so the same keys fight villains and break the
town.

---

## 4. Karma, levels and powers (`hw-sim/src/progress.rs`, `combat.rs`)

### Karma and levels

Karma is the one currency. Everything earned adds to **lifetime karma** (which sets the
level) and to **unspent karma** (which buys powers). Spending never lowers your level.

- Level 1 → 2 costs 960 karma; each later level costs a quarter of that more, and from
  level 6 another 4% per level above 5 (`karma_to_next(l) = 960 × (1 + 0.25 × (l − 1)) ×
  (1 + 0.04 × max(0, l − 5))`). Cap: level 20 (karma keeps coming after it, for powers).
- Each level-up refills health and energy, with a burst of light and a fanfare.
- Pacing target: level 2 in about ten minutes, level 5 (flight) in about an hour, level 20
  in about ten hours. `tests/powers.rs` plays the first hour (level 5 ≈ 62 minutes for a
  bot); `tests/high_tiers.rs` plays all ten (`cargo test --release -p hw-sim --test
  high_tiers -- --ignored`): level 20 after about 9.8 hours, the first tier-7 power about an
  hour later.

| Level | Lifetime karma | Level | Lifetime karma |
| --- | --- | --- | --- |
| 1 | 0 | 11 | 22 080 |
| 2 | 960 | 12 | 26 246 |
| 3 | 2 160 | 13 | 30 854 |
| 4 | 3 600 | 14 | 35 923 |
| 5 | 5 280 | 15 | 41 472 |
| 6 | 7 200 | 16 | 47 520 |
| 7 | 9 446 | 17 | 54 086 |
| 8 | 12 038 | 18 | 61 190 |
| 9 | 14 995 | 19 | 68 851 |
| 10 | 18 336 | 20 | 77 088 |

### Where karma comes from

- **Beating a villain:** its base karma (table in section 6) × a level factor
  `1 + 0.25 × (villain level − hero level)`, clamped to 0.2–2.0
  (`progress::defeat_karma`). Every hero within 60 m of the defeat gets it.
- **Resolving an incident:** a bonus on top of the villains' own, shared by every hero
  within 120 m (section 7).
- **Careful work:** a hero who wins an incident's fight having broken at most 2 000
  credits of the street (a window or two, a lamp post) since it started gets 15% of that
  bonus on top, as Guardian karma ("Careful work!", `incidents::CAREFUL_COST`).
- **Rescues:** Guardian karma for each person got out, double for handing the hurt one
  to the ambulance.
- **Reviving a teammate** pays no karma but pushes the Guardian affinity.

### Affinities

Five leanings, grown by deeds (`progress::Affinity`): **Guardian** (rescues, revives,
shielding), **Justice** (fighting crime and villains), **Science** (ray guns, force
fields, gadgets), **Mystic** (sigils, the Aether), **Might** (muscle). Half of each deed's
karma goes into the affinity the deed belongs to (a defeat counts for the affinity of the
power that dealt it, otherwise Justice). Powers whose chosen theme belongs to an affinity
get a discount of `30% × my points / (all points + 1200)`, so up to 30% as you lean
harder. Movement powers have no affinity and no discount.

### Buying powers

Press **B**. Each power has two **themes**: the mechanic is the same, the theme changes the
name, look, sound and the affinity (and so the price).

- **Tiers open by level:** tier 1 at level 1, tier 2 at 3, tier 3 at 5, tier 4 at 8,
  tier 5 at 12, tier 6 at 16, tier 7 at 20. Two powers (or more) in each.
- **Price:** `960 × tier price × (1 + 0.25 × powers owned) × (1 − affinity discount)`, with
  tier prices 1, 1.5, 2.25, 2.75, 3.25, 3.75, 4.25 (`progress::TIER_PRICES`). So a first
  tier-1 power costs 960, tier 2 1 440, tier 3 2 160, tier 7 4 080, each before markup.
- **No power ranks:** a power grows with your level automatically (damage ×
  `1 + 0.1 × (level − 1)`, so ×2.9 at level 20).
- **No respec**, but giving a power up refunds 90% of what you paid.
- The server checks every purchase against the hero file.

### The powers

Numbers at level 1 (`combat::power_spec`). Health and energy are 100 at level 1; health
grows with the same ×(1 + 0.1 per level).

| Key | Power (themes) | Tier | What it does |
| --- | --- | --- | --- |
| 1 / LMB | **Strike** (everyone has it) | – | 20 damage, 3 m reach, 0.7 s recharge, no energy. Might. |
| 2 / Q | **Energy Bolt**: *Atomic Ray* (Science) / *Arcane Bolt* (Mystic) | 1 | 36 damage, 40 m, 10 energy, 1.6 s; every hit knocks the target down. |
| passive | **Super Jump**: *Spring Heels* / *Rocket Boots* | 1 | Jump at 16.5 m/s (clears a house) with strong air control. |
| 3 | **Binding Hold**: *Gravity Cuffs* (Science) / *Binding Sigils* (Mystic) | 2 | 12 damage, 30 m, 12 energy, 6 s recharge; pins the villain for 7 s. |
| 4 | **Force Shield**: *Force Field* (Science) / *Guardian Aegis* (Guardian) | 2 | 16 energy, 26 s recharge; for 18 s you and allies within 12 m take half damage. |
| 5 | **Lift and Throw**: *Mighty Heave* (Might) / *Telekinesis* (Mystic) | 2 | First press picks up a car, bench, prop or torn-up rubble within 6 m; second throws it where you aim, at the target if there is one (60 damage, 30 m, 12 energy, 3 s, knockback). Not thrown within 8 s (`CARRY_SECONDS`), it slips free and drops just ahead, and the power recharges as after a throw; it also falls if the hero is knocked down or gives the power up. |
| tray | **Freeze Ray**: *Cryo Ray* (Science) / *Frost Sigil* (Mystic) | 2 | A cone of frost, 16 m long (to the first wall), widening as it goes: 12 damage to every villain in it; 14 energy, 16 s. Minions freeze solid in a block of ice for 3.5 s (they cannot move or attack); tougher villains are chilled for 6 s (half speed, attacks wind up 1.6× slower). The next blow on a frozen villain shatters the ice for double damage. A cone at a burning house douses the fire: the trapped get out 40% quicker and the fire waits 2 more minutes. |
| tray | **Teleport**: *Matter Transmitter* (Science) / *Aether Step* (Mystic) | 3 | Up to 40 m along where you look, stopping short of the first wall, roof or ground, and only where a person fits (never inside anything); 10 energy, 3 s. No room: nothing happens and nothing is spent. |
| tray | **Rescue Bubble**: *Force Bubble* (Science) / *Guardian Orb* (Guardian) | 3 | At a fire or crash, a glowing bubble takes the trapped person nearest your aim (within 30 m) out to the street in 3 s (karma as if freed by hand); the hurt one, or the last one out, floats straight to the ambulance (double karma, the rescue done); 12 energy, 8 s. With nobody to rescue it does not fire. |
| passive / V | **Flight**: *Jet Pack* / *Soaring Cape* | 3 | Fly at 22 m/s, 60 m/s with Shift, climb 10 m/s, ceiling 900 m. |
| 6 | **Shockwave**: *Thunderclap* (Might) / *Sonic Boom* (Science) | 3 | 34 damage to everyone within 8 m, knocks them all down; 26 energy, 12 s. |
| passive / Shift | **Super Speed**: *Lightning Legs* / *Rocket Skates* | 4 | Run at 18 m/s, 32 m/s with Shift; a crackling trail. |
| tray | **Robot Sidekick**: *Tin Pal* (Science; chrome and red, visor) / *Brass Squire* (Guardian; brass and blue, optics) | 4 | Calls a small robot (an Actors robot at 0.62 of a person's size) to your side; 30 energy, 30 s. It keeps behind and beside you, fights whatever you fight or whoever goes for either of you within 18 m, punching for 14 (grows with level) every 1.1 s, and has 60% of your health. Pressed again while it is out (free) it goes for your target. Left 30 m behind, or stuck 2 s behind a wall, it beams back to you. Beaten, it powers down and is taken away 3 s later; one at a time; it goes when you give the power up. |
| 7 | **Mend**: *Vital Ray* (Science) / *Healing Light* (Guardian) | 4 | Heals you and allies within 14 m by 30 (grows with level); 18 energy, 20 s. |
| 8 | **Beam**: *Disintegrator* (Science) / *Spirit Lance* (Mystic) | 5 | 50 damage to every villain in a 36 m line (to the first wall), knocks them all back; 22 energy, 7 s. |
| tray | **Hypno Ray**: *Hypno Spiral* (Science) / *Charm* (Mystic) | 5 | A minion (8 s) or lieutenant (3.2 s) within 30 m fights for you: it attacks the nearest villain within 25 m, and the villain it strikes turns on it (the rest of the group keeps at you); a spiral turns over its head. Bosses and machines are too strong-willed. 20 energy, 30 s. Karma for what it beats is yours. |
| passive | **Vitality**: *Aether Vitality* (Mystic) / *Iron Constitution* (Might) | 5 | 2% of your health back every second, in a fight too. |
| 9 | **Meteor**: *Orbital Ray* (Science) / *Starfall* (Mystic) | 6 | Marks the target's spot; 1.2 s later a strike from the sky does 32 damage to everyone within 5.5 m (walls give no cover) and cracks the street; 30 energy, 18 s. |
| passive | **Invulnerable**: *Steel Hide* (Might) / *Unbreakable Will* (Justice) | 6 | Every blow does half. |
| 0 | **Nova**: *Atomic Nova* (Science) / *Earthshaker* (Might) | 7 | 120 damage to everyone within 16 m, knocks them all down, breaks the street round you; 60 energy, 40 s. |
| passive | **Rally**: *Rallying Cry* (Justice) / *Heroic Aura* (Guardian) | 7 | You and allies within 20 m hit 35% harder, recharge powers 1.4× as fast and get energy back 1.5× as fast. |

Health comes back faster as it grows: regeneration (1 a second in a fight, 14 out of it at
level 1) scales with your maximum health. A typical path: Spring Heels or Energy Bolt at the
start, the other at level 2, a tier-2 power at 3, Flight at 5, then one new power every level
or two; a bot that buys as it goes owns about a dozen at level 20.

---

## 5. Fighting (`hw-sim/src/combat.rs`, `hw-client/src/fightview.rs`)

City of Heroes is the model: groups of minions, tougher lieutenants and bosses, tab
targeting, ragdolls, nobody dies.

- **Targeting:** Tab picks the nearest villain within 60 m that you can see (none behind a
  wall), those in view of the camera first; each further Tab moves on to the next nearest one not picked yet, and after all of
  them it starts over (after 4 s without Tab it starts again from the nearest). An aimed
  power with nothing targeted takes the villain nearest the crosshair, and a villain that
  attacks you becomes your target if you have none. The target frame shows name,
  rank and level in its **con colour** (how tough it is against you).
- **Using powers:** one power at a time (0.5 s lock); a power that cannot fire yet (out of
  range, recharging) waits up to 1 s and then fires. Blows land with sparks, beams, scorch
  marks, floating numbers, a camera jolt and a moment of hit-stop; whoever takes a blow and
  stays up rocks back from it (0.2-0.3 m and a lean, for a third of a second).
- **Line of sight:** no blow goes through a wall, for heroes, sidekicks, Hero Hall's heroes
  and villains alike: punches and aimed powers need a clear line from chest to chest
  (`combat::clear_line`, exact for walls however thin), and a villain's blow misses if its
  foe stepped behind a wall during the windup. Firing at a target out of sight says so.
- **Camera:** the camera stays out of walls: the boom behind the hero is shortened so that a
  small box round the camera (its near plane) touches nothing, it comes in at once and
  eases back out (`hw-client/src/camclip.rs`).
- **Telegraphs:** every villain attack has a visible windup before it lands (0.45 to 0.8 s
  for minions, longer for the big ones), so you can step out of the way.
- **Roles you can read:** a shield bearer's shield clangs and sparks ("Blocked!") when it
  stops a blow; a grenadier's bomb arcs through the air while a red ring marks where it will
  burst; an orderly's mending is a green beam with a "+N". The first time each happens a hint
  says what to do (see section 6).
- **Regeneration:** health 14/s and energy 25/s out of combat; 1/s and 6/s while in
  combat (within 6 s of the last blow given or taken).
- **Knockdown:** knocked fighters tumble as ragdolls for 2.2 s and need 1.4 s to stand; they
  can be hit but cannot act.
- **Defeated villains** lie in glowing energy cuffs; after 2 s a police drone lifts them away.
- **Defeated heroes** lie down. A teammate holds E within 2.5 m for 3 s to revive them at
  half health. Otherwise press H, or wait 20 s, and a medical drone flies them to the
  hospital (beside the spawn line; below level 5, beside the rookies' street). Carrying someone and getting knocked down drops them.
- **Villain AI:** villains notice heroes within their aggro range, walk round walls and
  houses on a grid A* path (`hw-sim/src/nav.rs`), and go home when the fight moves away.
  Ranged ones keep their distance.
- **Co-op level:** everyone fights at the **host's** level (health, damage and new groups
  follow it), so friends of any level can play together. Group size grows with players
  (5 minions alone, +1 per extra hero, up to 8), not their toughness.
- **Beaten groups come back** at their corner 15 s later, but not while a hero stands within
  25 m; ambient groups go home once no hero is within 420 m.

---

## 6. Villains and factions

### Factions (`hw-sim/src/factions.rs`)

Eight factions with a standing toward each other from −100 to 100: Hostile (≤ −60),
Unfriendly (≤ −20), Neutral, Friendly (≥ 20), Allied (≥ 60).

A hero belongs to none. **Each faction starts out regarding a new hero exactly as it regards
ordinary townsfolk**, so a new hero is just another passer-by. Deeds move that regard
(kept on the hero file): beating one of a faction's people costs 2 points with it and gains
1 with every faction that dislikes it (townsfolk and the Patrol included). The B window
shows where you stand.

| Faction | Who | Toward a new hero | Notes |
| --- | --- | --- | --- |
| Townsfolk | everyone on the pavements | Friendly (50) | Heroes cannot target them. |
| City Patrol | the law | Friendly (50) | No units in the game yet; police drones are its face. |
| The Doctor's Lab | the mad scientist's mutants, technicians and robots | Unfriendly (−40) | Hates the Patrol and the Syndicate. |
| Chrome Jackets | greaser street gang | Unfriendly (−30) | At war with the Syndicate. |
| Velvet Syndicate | pinstriped gunsels, organised crime | Neutral (−10) | Keeps the peace when it suits business. |
| Moles | Molehaven's mole folk | Neutral (0) | Hate the Lab and the Empire. Delvers guard Molehaven's lift and sometimes surface in town, frightened: hold E near them for 3 s to talk them down (+6 with Molehaven), or fight them (no friends made). |
| Sky Pirates | an airship crew | Unfriendly (−30) | Moor over the harbour with lookouts on the quay (warn, then fight); raid shops from an airship moored over the street (12 m reach). |
| Iron-Masked Empire | the Iron Legion, from beyond the sea | Unfriendly (−55) | Hates everyone. Lands in iron capsules and holds two townsfolk captive (20 m reach); only once the team is level 2 and has two heroes, or rarely alone. Never attacks an unknown hero on sight. |

**What a villain does about a hero** (`factions::react`):

- **Hostile:** attacks on sight.
- **Unfriendly, on its own turf** (the Doctor's quarter, the Jackets' corners): shouts a
  warning ("The Doctor's property. Leave. Now."), attacks if you come within 9 m or stay
  more than 6 s; a warning is forgotten after 40 s.
- **Unfriendly elsewhere, Neutral or better:** leaves you alone.
- **Always:** a group fights back when you hit one of them, and a crime's crooks turn on
  you when you step within the crime's reach (9 m for a mugging up to 18 m for a breakout).

Turf borders are fixed in 1.0. Co-op: standings are per hero; a group reacts to the least
liked hero near it.

### The villains (`combat::enemy_spec`, looks in `hw-client/src/people.rs`)

Numbers at level 1; health and damage scale with the villain's level like a hero's.

| Villain | Faction | Rank | Health | Hit | Reach | Karma | How it fights |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Mutant Brute | Lab | Minion | 52 | 7 every 2 s | 2.4 m | 7 | Green, huge, glowing eyes; rushes you with big telegraphed hooks. |
| Ray-gun Technician | Lab | Minion | 34 | 6 every 2.6 s | 26 m | 6 | Lab coat and goggles; keeps its distance and shoots. On roofs it reaches 1.6× as far. |
| Chrome Jacket | Jackets | Minion | 44 | 6 every 1.8 s | 2.2 m | 6 | Greaser; fast melee rusher. Muggings, corner hangouts. |
| Syndicate Gunsel | Syndicate | Minion | 36 | 6.5 every 2.4 s | 24 m | 6 | Pinstripes and a ray pistol. Robberies. |
| Robo-Sentry | Lab | Lieutenant | 150 | 10 every 2.4 s | 26 m | 20 | A walking robot with a ray cannon; the dish on its back shields every villain within 9 m. Break the dish first (a weak point). |
| Mutant Alpha | Lab | Boss | 300 | 14 every 2 s | 2.8 m | 45 | Bald, dark, tusked, blazing eyes. Leads rampages and some corners. Every 9 s or so it **crouches (0.9 s) and leaps** 7–24 m onto a hero, hitting everyone within 4.5 m of where it lands. Get out of the way. |
| The Doctor's Colossus | Lab | Colossus | 1 300 core | 20 | 140 m | 400 | The walking tank (below). |
| Delver | Moles | Minion | 50 | 6.5 every 2 s | 2.4 m | 6 | Pale, goggled mole miner with a pick. |
| Buccaneer | Sky Pirates | Minion | 42 | 6.5 every 1.7 s | 2.5 m | 6 | Striped shirt, cutlass. |
| Deck Gunner | Sky Pirates | Minion | 34 | 6.5 every 2.4 s | 26 m | 6 | Long coat, ray musket. |
| Sky Captain | Sky Pirates | Lieutenant | 160 | 11 every 1.9 s | 2.7 m | 22 | Wide hat, red beard, gilded cutlass. Leads raids. |
| Legionary | Empire | Minion | 48 | 7.5 every 2.6 s | 28 m | 8 | Iron mask and cuirass, shock rifle. |
| The Doctor | Lab | Archvillain | 1 400 | 15 every 2 s | 30 m | 220 | Only in the last chapter of his story, in his lab. White lab coat, wild white hair, goggles, a Tesla staff. Throws lightning; when a hero comes within 13 m he **charges an Aether storm** (1.8 s, a green ring on the ground) that hits every hero still inside for 1.6× his blow, every 9 s or so. |
| Praetorian | Empire | Lieutenant | 190 | 12 every 2.2 s | 3 m | 24 | Gilded mask and cuirass, shock lance. Leads landings. |
| Mutant Bulwark | Lab | Minion | 52 | 7 every 2.2 s | 2.4 m | 8 | A brute with a big brass atom shield on its left arm. Aimed blows (bolts, beams) from more than 3.5 m in front mostly clang off the shield (a quarter gets through); blasts, holds and anything from close by or from the side get through. Get close, or go round it. |
| Lab Grenadier | Lab | Minion | 32 | 11 every 4.2 s | 18 m | 7 | Olive overalls and a leather helmet. Lobs a fizzing bomb where you stand: a red ring shows where, and it bursts 1.9 s later (2.6 m round). Step out of the ring. |
| Lab Orderly | Lab | Minion | 30 | 5 every 2.6 s | 20 m | 7 | Mint overalls, white gloves. Every 4 s it patches up a hurt friend within 14 m (14 health at level 1, a green beam); in between it shoots. Take it out first. |
| Weak point | – | Part | 240 | – | – | 25 | A breakable part of a robot: the colossus's knees and cannon, the sentry's dish. Tab to it. |

Brutes, bulwarks, grenadiers, orderlies, technicians, Jackets and gunsels are knocked down by knockback powers; the sentry,
alpha and colossus are "heavy" and are not. Mixed Lab groups are three brutes to two
technicians. **Roles** (`combat::with_roles`): in a Lab group of four or more, one brute
carries a shield (bulwark); a group of five or six also gets a grenadier or an orderly (by
the seed), and seven or more get both, so each group has someone to rush, someone to shoot
and someone to deal with first. Groups with a set crew (a rampage, a corner gang) keep it. One in four of a group is a level above the rest.

The **mad scientist** himself, the Doctor, shows only at the end of his story (below).

### The colossus (`hw-sim/src/robots.rs`)

A set piece in three acts, best with friends. It marches out of the robot works' gate (the Tesla tower's in a town without works) once the team
is level 3 or more, at most every 25 minutes, and the Hero Signal calls everyone. It keeps
to main roads at least 11 m wide (A* along road centre lines), and its feet and hull break
what they walk into. Its health grows with the number of heroes.

1. **Walking, armoured:** blows on the hull glance off (10% gets through). It shells heroes
   with its cannon (a 1.8 s warning, 5 m blast, 10–140 m range, every 7 s) and stomps
   heroes under it (1.2 s warning, 12 m). Its **four glowing knee actuators** and the
   **cannon** are weak points (240 health each).
2. **Down:** with two knees gone it drops on its belly, the clamshell opens and the
   **Aether core** takes full damage. Two robo-sentries per hero drop from the hatch, and
   the core discharges every 7 s (13 m; fly or jump more than 2.5 m up to be clear).
3. **Enraged:** after 25 s it hauls itself up on two legs, limping but firing every 4.5 s.
   Break the other two knees and it goes down for good. Its wreck smokes for 45 s and is
   hauled off.

It has its own battle march, playing within 260 m of it while it stands. An unanswered
colossus goes home after 10 minutes. `tests/colossus.rs` beats it with bots in 2 to 11
minutes.

### Who lives in the deep underground (`hw-sim/src/deepspots.rs`)

The deep plan leaves places for faction groups. Once a hero is more than 8 m below the
street, the places within 160 m are taken in, and each gets its group while a hero is within
45 m: four delvers round a **mole camp** in the Deeps (Molehaven's folk: they leave you be),
three of the Doctor's brutes and technicians guarding **his lab** (two levels up, they warn you
off his property), and two Syndicate **relic hunters** prowling each buried ruin. Places more
than 320 m from every hero are forgotten again.

### Hero Hall and the town's own heroes (`hw-sim/src/npcheroes.rs`)

Hero Hall stands on its own plaza near the Aether Spire, facing it: a round glass hall
under a white flying-saucer roof, two red arches over it, a gold star on a pylon, and a
canopy over the doors with the heroes' star shield. A taxi stand is in front.

Five of the town's own heroes stand in a row on its plaza: Commander Starling (who hands out
the Doctor's story and stays home), Atomic Annie,
the Brass Badger, Lady Lumen and Rocket Rory (looks in `people::npc_hero`). Come within 7 m
and they greet you with tips, one at a time and a new one each time (each hero again after
90 s). When a fight has gone unanswered for 75 s, no player hero is within 120 m of it and none is
on the way to it (see Incidents),
the nearest one at home flies out (up to 70 m over the roofs, 28 m/s), hovers 3.5 m over the
fight and bolts the villains (30 m, every 1.5 s). They are never beaten: below 30% health
they break off and fly home to mend, and they go home when a player hero comes near, so
they never take a player's fight. Villains they beat pay no one karma.

### The Doctor's story (`hw-sim/src/story.rs`, `hw-client/src/storyview.rs`)

The game's spine: a fixed chain of seven chapters that leads from his technicians on the
street to the Doctor himself. Each chapter opens at a level; **Commander Starling** at Hero
Hall hands it out (he no longer flies out to incidents: he keeps the hall). The chain is the
same in every town, but where each chapter plays comes from the town: the street outside
the quarter nearest the hall (at least 150 m from it, 120 m from his corners), his lab
nearest the hall, the robot works' gate (the Tesla tower's in a town without), the news
kiosk over his lab, the tower's plaza, and the great hall of his lab below. A town without
a villain quarter or Hero Hall has no story.

| # | Chapter | Opens at | Where | Who stands there (one hero) | Then hold E |
| --- | --- | --- | --- | --- | --- |
| 1 | Sparks on the Street | 1 | a street outside the quarter | 3 technicians | 3 s: search their crate |
| 2 | The Stolen Cells | 3 | his lab nearest the hall | a Robo-Sentry and 3 (+1 level) | 4 s: read the ledger |
| 3 | The Robot Works | 5 | the works' gate | a Robo-Sentry and 3 (+1) | 5 s: jam the line |
| 4 | The News Kiosk | 8 | the kiosk over his lab | a Mutant Alpha and 3 (+1) | 4 s: read the logbook |
| 5 | The Colossus Walks | 11 | out of the works' gate | the colossus (+1) | – |
| 6 | The Tesla Tower | 14 | the tower's plaza | a Robo-Sentry and 4 (+2) | 6 s: shut the coil down |
| 7 | The Doctor | 16 | his lab's great hall | the Doctor and 3 (+1) | – |

Each friend along adds one to the group. How a chapter goes: once the team is of its
level the radio says so, and the gold **star** (compass, map, radio) and a gold column of
light point to Hero Hall; walking within 9 m of Starling's place is the briefing. Then the
star moves to the chapter's place; within 140 m its group takes its place and fights any
hero. Beaten, the chapter's find is searched by holding **E** within 4 m (a bar shows how
far along). Leaving (420 m away) sends the group home; it stands there fresh next time.
Done, every hero within 200 m earns **30% of what their next level takes** (Justice), and
Starling says what was found and what comes next.

Everything said is text on the **wrist radio** (top middle under the compass, "WRIST RADIO -
Commander Starling"); there are no voices. The radio's own box shows the chapter ("THE
DOCTOR'S STORY - chapter 3 of 7: The Robot Works") and the goal with distance and clock
direction, or "Next lead at level 8" while the next one is not open.

**The reward:** beating the Doctor makes the hero the **Bane of the Doctor**, and their
statue, cast in bronze in the heroic stance, stands on the marble plinth in front of Hero
Hall that waits empty until then (the plinth is solid from the start, the statue once it
stands).

**Saved** in the hero file (`story`, chapters done, and `briefed`). In co-op the team follows
the **host's** story, as it fights at the host's level: anyone near can be briefed or finish
a chapter for the team, and it is written into the host's file and into the files of those
who were there; a guest further along keeps their own place.

---

## 7. Incidents: trouble comes to you (`hw-sim/src/incidents.rs`)

The town makes trouble near the heroes: the first one 20 s into the game, then at least a
minute apart, with two to four open at once (one more per extra hero, up to four). It starts
150 to 800 m from a hero who flies or has Super Speed, and 50 to 350 m (at most a minute's
run; the nearer of two places picked) from a hero on foot. Which kind and where come from
the world seed and a running count, so the same save gives the same sequence.

**Rough streets:** within 450 m of a Chrome Jackets hangout crime comes twice as often (30 s
apart, one more open at once), and more of it is small: muggings 60 and robberies 30 by
weight, rampages, breakouts and pirate raids a third to a half as often as elsewhere. New
heroes start here.

Each shows on the **wrist radio** (top left, `radioview.rs`) with its name, distance, a clock
direction ("at 3 o'clock") and con colour, as a **column of light** in the call's colour
rising from the spot itself (faint at its foot, brightest a few storeys up, fading out 140 m
high, with a ring of the same light on the ground round the spot), on
the **compass** strip at the top of the screen, and on the **map** (M). Rampages, breakouts
and the colossus also light the **Hero Signal**: a searchlight from the Aether Spire throws a
star onto the sky above the trouble, visible from anywhere in town.

A fight is resolved when its group is beaten; a rescue when everyone is out and the hurt one
is with the ambulance. Unanswered (no hero within 60 m) for 4 minutes, it ends: the crooks
get away, or the fire brigade comes. **A hero on the way** (closing in from within 1200 m)
keeps it waiting: while they get nearer it lasts at least their running time from there
(×1.5, +30 s), up to 5 minutes past its usual end, and the town's own heroes leave it to
them (they stop waiting 12 s after the hero stops closing in).

| Incident | Chance | Who / what | Level | Reach | Bonus |
| --- | --- | --- | --- | --- | --- |
| Mugging | 25% | 2 Chrome Jackets and a victim with their hands up, on a side street | team | 9 m | ×1.0 |
| Robbery in progress | 20% | 4 Syndicate gunsels at a shop front (outside the quarter) | +1 | 12 m | ×1.5 |
| Mutant rampage | 15% | a Mutant Alpha leading 3 brutes | +2 | 16 m | ×2.0 |
| Lab breakout | 12% | a Robo-Sentry leading 5 technicians at one of the Doctor's labs | +2 | 18 m | ×2.5 |
| Car crash, people trapped | 14% | two smashed, smoking cars, 2 people (4 s each) | – | – | 3 per person |
| Fire, people inside | 14% | a house in flames, 3 people (3 s each) | – | – | 2.5 per person |
| The colossus is loose! | 8% | the walking tank (level 3+, every 25 min at most) | +3 | everyone | ×1.5 |
| Building collapse | follows a collapse | 3 people in the rubble (5 s each digging) | – | – | 3 per person |

"Bonus" is karma on top of the villains' own: for fights the group's worth again times that
factor, for rescues per person freed. The crooks of a crime mind their own business until a
hero steps within its reach; a mugging's victim walks off once you deal with the muggers.

**Rescues:** stand within 3.5 m of the next trapped person and hold **E**. The last one out
is hurt: the hero who freed them carries them to the **ambulance** parked at a kerb about
50 m away (a red cross on the compass, "Carry them to the ambulance, N m"); within 5 m it
takes them (double Guardian karma). A carrier who is knocked down drops them; anyone lifts
them again by holding E for a second. A building that heroes or the colossus bring down
starts a collapse rescue at its front (one per pile; it does not count against the radio's
limit).

---

## 8. The town (`hw-gen`)

Each world seed gives one town. The planet layer (continents, regions, many towns, roads
between them) is designed but not built; the town stands in endless streamed ground and sea.

- **Size:** any size from hamlet to megalopolis; the player picks it in the title's city
  window (saved in the profile; `HW_POP` overrides, 40 000 by default). Towns plan in fractions of a second; detail is planned per district on demand
  as you approach (`DetailId`, see streaming notes).
- **Growth** (`hw-gen/src/urban/`): a town grows from an old core along roads, rivers and
  coasts, shaped by hills. Sites: river crossings, confluences, estuaries, bays, coasts,
  lake shores, hilltops, valleys, crossroads, islands. Foundings: village, colony,
  fortress, port, mill town. Old walls, swallowed villages, quarters from crooked old lanes
  to grids, baroque axes, garden suburbs, superblocks and retro-future expo layouts.
- **Regions:** each town builds in one tradition. **Northern** (brick, steep slate and tile,
  gables to the street, half-timbering), **Continental** (plaster and stone, tiled gables
  and hips, mansards, half-timbering), **Southern** (stucco, shallow terracotta hips).
  Newer quarters are pure golden-age: Googie and streamline houses, deco and streamline
  flats; 19th-century extensions mix the two.
- **Buildings** are planned inside out: floors and rooms first, the facade from the rooms.
  Uses: offices, shops and offices, apartments, houses, works, pavilions, labs, tenements.
  Every building has a furnished interior (lobbies, offices, flats, shops, labs) and a
  glowing **Aether lift** in its core (Space rises, Ctrl sinks). Buildings on slopes stand
  on stone plinths. Surfaces are textured and weathered by age and quarter.
- **Water:** rivers, lakes and coasts; bridges; granite quays with bollards and steps down
  to the water. Water is a surface you stand on at its level; quay walls block (fly out).
- **Landmarks:** the **Aether Spire** (every town, on a raised plaza near the old centre,
  with steps; the Hero Signal shines from it), the **lighthouse** (seaside towns), the
  **Tesla tower** (in the villain quarter), the **airship mooring mast** (every town, on
  its airfield just past the edge), the **rocket port** (every town, just past its edge;
  see below), the **atomic plant** (out of town, below).
- **The atomic plant** (`hw-gen/src/urban/atomic.rs`, `hw-client/src/atomview.rs`): on a
  38 m paved yard just past the built-up edge (growth time 68-650 m past the edge),
  beside a main road (yard edge 4-40 m from it, facing it), preferring water within 200 m;
  no plant where no spot is clear and level (rim spread at most 5 m). A white reactor drum
  (r 11 m, 9 m walls) under a ribbed dome with a blue glass band, a blue ring at its foot
  and a glass lantern on top (22 m); two hyperbolic cooling towers (34 m, hollow: walk in
  between the 24 legs and look up through the steam); a streamlined turbine hall with a
  rounded nose, ribbon windows (lit at night) and three red fins; two steam mains on a
  trestle; a red-and-white stack (50 m) with a blinking beacon; an atom on a pylon at the
  gate and a saucer-roofed gatehouse. Every part is solid. Taxi stand: "Atomic plant gate".
  Alive within 2 600 m: the reactor's light breathes (3.4 s) and surges every 8-22 s;
  steam rolls out of both towers; within 1 200 m electrons race round the atom, the
  beacon blinks (1.8 s) and every 25-60 s the relief valve on the hall roof blows off for
  2.5-4 s. Sound: turbine hum (200 m), tower steam (260 m), valve roar (500 m). Views:
  `HW_VIEW=atomplant`, `atomgate`, `atomtower`.
- The **weather tower** (towns of 5 000 or more; see "The weather tower" below).
- **Street furniture:** lamps in each quarter's style (iron lanterns, deco double globes,
  atomic saucers), parked golden-age cars, trees by region (lindens and birches north,
  planes and chestnuts in the middle, palms and cypresses south), hedges, fountains,
  benches, bins, hydrants, rocket mailboxes, bollards, kiosks, ad pillars, bronze hero
  statues, traffic lights; taxi pads on tall golden-age roofs; neon signs. Real grass on
  lawns, verges and parks, woods out in the country.
- **Street life** (`hw-gen/src/urban/props.rs`: `shop_fronts`, `street_life`, `fences`):
  more than half the shops put out two to four cafe tables (marble tops, wire chairs,
  striped parasols) against their windows where the pavement leaves room to walk past and
  no lamp, tree or barricade stands in the way (walkers step round one thing at a time), one
  in five a newspaper stand. Round glass phone booths and brass pneumatic post columns
  (capsule in a glass tube, cyan ring; more of them in the old quarters) stand at the kerb
  every 60-140 m; main roads with traffic get bus stops (glass shelter under a boomerang
  roof, round sign on a pylon) every 220-380 m. Front gardens of houses and flats without a
  hedge get a fence (white pickets in old quarters, iron railings on a stone kerb before
  flats, chrome rails in the golden-age suburbs) with a gap for the path and flowers each
  side. All are solid and breakable. Seed 1 at 40 000 people: about 120 cafe tables, 90
  news stands, 175 phone booths, 235 post tubes, 19 bus stops, 160 fence pieces.
- **The old town's dress:** the old core stays a historic town, dressed with the golden
  age: most shops there have striped awnings, most shops hang a round neon-ringed sign on
  an iron bracket over the pavement, and two old houses in five carry an Aether conduit up
  a front corner (a brass pipe with glowing glass sleeves from a junction box to the eaves).
- **No blank walls:** gable ends have attic windows in rows (two or three in tall gables,
  kept clear of half-timbering), stairwells have a window at each landing, storerooms a
  high window where they do not face the street. Cobbled lanes and pavements are mottled
  in patches (worn, sandy, damp) so their texture does not repeat in bands.
- **Light:** the game plays at a fixed mid-afternoon (15.5 h) with long shadows; the title
  at golden hour. `HW_HOUR` sets another hour: at night lamps light the streets, a third of
  the windows glow, the Spire bathes its plaza in cyan, and two moons hang in the sky.
  There is no day/night cycle yet.

### The airfield and its liner (`hw-gen/src/urban/mast.rs`, `hw-client/src/mastview.rs`)

Every town has an airfield: a 22 m apron on open ground just past the last houses, as near
the core as there is room for the moored ship to lie out over the fields (in the 48
directions round the core; 255 m out in a hamlet of 800, 2.2 km in a city of 60 000,
2.7 km at 600 000). On it stand a 56 m red-and-white lattice mast with a glass lift shaft, a
tulip collar ringed with Aether light and a gallery at 45 m; a terminal with a glass front
under a boomerang roof, a walkway canopy to the lift, a pylon with a gold atom, and a
windsock. All of it is solid (legs, shaft, collar, gallery, head, terminal, canopy and its
posts, pylon, windsock pole). Taxi stand: "The airfield".

The liner *Skylark* (96 m, silver, red cheat line, passenger deck with panoramic windows,
four engine pods, rocket fins) keeps a timetable from the game clock, so every player sees
it in the same place: moored 170 s with its nose in the mast's cup (swinging a few
degrees, the head turning with it), cast off 45 s (backs 35 m off, rises 12 m, turns
away), then its round: up to cruising height (above the town's tallest towers, 100 m at
least, 90 m over any hill), over the core, out beyond the town and round wide, at 22 m/s,
turning at most 0.075 rad/s; then a glide in along the mast's axis from 700 m out and
70 m up, slowing evenly to a stop. A cycle is 12-17 minutes; a session opens with it moored,
25 s before it casts off. The liner is solid where the timetable has it: its envelope, the
passenger deck under it, the fins and the engine pods become boxes (at most 4.6 m on a side,
`hw_sim::airships`) that move with it each tick, for heroes within 30 m of the hull. Heroes
bump into it, can land on its back and cannot fly through it; it does not carry a hero
standing on it. The pirates' *Cloudcutter* moored over the harbour is solid the same way
(envelope, gondola, engines, fins). Raid airships over a street are not solid yet.

While moored: the gangway slides out to the nose door (6-12 s), the lift runs up and down
twice, two crates come down on the winch from the cargo hatch onto the baggage cart
(22-50 s, 66-94 s), the cart drives them round the mast into the terminal (112-132 s) and
comes back. The beacon blinks red, a second on, a second off. Sound: the engines' drone
(within 900 m, louder as they work), a horn on the glide in and as it casts off (1.5 km),
the latch clanking, the winch whirring. Views: `HW_VIEW=mast`, `mastapron`, `terminal`,
`liner`; `HW_MAST_T=<s>` holds the liner that many seconds into its cycle (0 just moored).

### The rocket port (`hw-gen/src/urban/gantry.rs`, `hw-sim/src/rocket.rs`, `hw-client/src/rocketview.rs`)

Every town has a rocket port out past its built edge (the first level spot 45 m or more
beyond where its districts end, off every road, at least 60 m clear of the villain quarter
and the other landmarks): a paved round pad 21-26 m across with a 30 m finned rocket on a
3 m concrete launch stand, a 40 m orange lattice gantry with two swing arms (fuel lines at
11 m, the crew walkway at 20 m), a countdown board and a chrome atom on its crane, a domed
blockhouse facing the town, two fuel spheres and four floodlight masts (lit at night). A
taxi stand "The rocket port" goes there.

**Launches** run on the game clock, the same for every player: the first countdown starts
45-120 s into a session (by seed), then one every **480 s**. A launch: venting and the
klaxon (0 s), the arms swing away (8-16 s), the board counts 10 to 1 (20-30 s), the engine
lights and roars on the stand with fire and smoke out of the flame duct (30-33 s), the
rocket climbs at 3 m/s² out of sight (2 600 m, about 42 s), stays away, relights 1 600 m up
(260 s), brakes all the way down and touches down at 300 s in a ring of dust, vents while
it cools and the arms swing back (312-322 s). Between launches the board shows the minutes
left (amber), the last 99 seconds in red. The rocket and its arms are solid wherever they
are (moving solids, like the street cars); the stand, duct, tower, blockhouse, spheres and
masts are footprints. Sound: the engine's roar (heard 3 km away), venting hiss, klaxon,
countdown beeps, ignition, touchdown.

**Mission hooks** (not used yet): `Schedule::scrub` calls off a launch before the engine
lights (the next goes ahead); `gantry::weak_spots` are where a saboteur goes (the umbilical
valves at the gantry's foot, each sphere's valve, the blockhouse door).
Views: `HW_VIEW=rocket`, `rocketpad`, `rocketarm`, `rocketboard`; `HW_ROCKET_T=<s>`
freezes the first launch that many seconds in.

### The weather tower (`hw-gen/src/urban/weather_tower.rs`, `hw-sim/src/rainmaker.rs`, `hw-client/src/weatherview.rs`)

The Weather Bureau's tower stands in every town of 5 000 people or more, on the highest clear
ground 250-900 m out from the core, outside the villain quarter and at least 150 m clear of
the other landmarks: a round glass pavilion, a white mast 44-62 m tall with three swept coral
fins, an observation saucer, and on top six cloud cannons round a glass weather globe under
an anemometer. A barometer dial on the mast reads FAIR (gold), CHANGE or RAIN (blue).

It makes its own weather over the blocks in front of it (a shower 58 m round, centred 40 m
out from the tower), in a round of 300 s on the game clock, so every player sees the same:
fair 0-60 s (globe gold); brewing 60-100 s (the cannons fire puffs of vapour one after
another, every 0.8-1.5 s, and a cloud builds 24 m over the top and darkens); rain 95-200 s
(full from 105 s; thunder every 6-15 s while it pours, the rumble arriving at the speed of
sound); clearing 185-215 s; a rainbow 195-260 s, in daylight only, always turned to whoever
looks at it. The anemometer races and the needle swings to RAIN in the storm; under the cloud
the sky greys over and the sun goes in. No rain falls indoors or underground.

Gameplay: a house fire under the rain (rain over half strength) burns at half pace, so the
people inside hold out twice as long. Sound: the machinery's whirr (160 m), a whoomp per cannon
shot (300 m), the rain's rush (all round you under the shower, 250 m), thunder (1.5 km), a
sparkle as the rainbow comes out. Views: `HW_VIEW=weather`, `weathertop`, `weatherfoot`,
`weatherstreet`; `HW_WEATHER=brew|rain|clear|bow|fair` picks the moment. Debug menu: Places,
"Weather tower".

### The villain quarter (`hw-gen/src/urban/dystopia.rs`, `hw-sim/src/quarter.rs`)

Every town of 3 000 people or more has one district the mad scientist holds: a run of
blocks out from the core along main roads, tenements and labs, with his **Tesla tower** in
the middle (iron lattice, copper dome, green coils). As you go in the picture turns darker
and harder (exposure -0.3, contrast 1.3, saturation 0.62, green shadows and red-hot
highlights, a heavy vignette), with a sooty murk only in the distance (750 m); two lamps in
three are dead, the rest flicker green; neon is broken; dirt and soot everywhere.

**His fixtures** (`hw-gen/src/urban/menace.rs`, drawn with the buildings by
`hw-client/src/menacemesh.rs`, alive in `menaceview.rs`; only labs and tenements carry
them, so they mark the quarter from afar):
- **Smokestacks** (6 labs in 10, 1 tenement in 7; 18-26 m over a lab's roof, 11-16 m over a
  tenement's), brick, iron bands, a red warning band near the top that glows day and night,
  a ladder close up. Dark coal smoke rolls off the 12 nearest within 1 100 m.
- **Searchlights** on a front corner of the roof (a lab in two, a tenement in 16; the 8 most
  spread out of them shine, the rest stand dark): the head turns 0.22-0.4 rad/s and nods 28-56° up; its 320 m beam is faint by day and blazes at
  night. Seen within 2 200 m of the quarter's middle.
- **Tesla coils** on every lab's dome (copper core, five winding rings, a chrome top load
  and a glowing ball); sparks crackle from the top load onto the dome on the 4 nearest within
  350 m (up to 8 at once, 0.15-0.75 s each, a new path every 0.05-0.14 s).
- **Aether pipes** glowing green up both front corners of a lab (now and then one on a
  tenement), in iron collars.
- **Loudspeakers**: three horns on a bracket a storey up a front corner (7 labs in 10, 9
  tenements in 20), a red lamp under them.
- **Posters** on the ground floor's piers ("Obey the Doctor", "Progress through power",
  "The Doctor sees all", "Heroes not welcome"; `hw-client/assets/posters/`): the 50 nearest
  within 130 m.
- **Patrol drones**: 4-10 chrome saucers with a red eye loop over the quarter 48-72 m up at
  9-15 m/s, a green beam swinging over the street under each.
- **At night** red pools under the loudspeakers and green ones round the coils (the 10
  nearest within 260 m), and the 3 nearest drones light the street green as they pass.

Views: `HW_VIEW=menace` looks at the quarter from 500 m outside it (`HW_MENACE_D=` metres),
`HW_VIEW=quarter` from a street inside; add `HW_HOUR=22` for night.

The tower is alive (`hw-client/src/teslaview.rs`, made within 1 200 m, animated within
900 m): up to 3 green arcs at a time crackle from the dome's lip into the yard, each lit
0.15-1.3 s and jumping to a new path every 0.04-0.12 s; 3 tendrils lick off the ball on
the needle; a glow pulse climbs the coil. The charge builds (+0.12 a second), and every
3.5-9 s a bolt cracks down in three strokes onto one of the four lightning rods on the lab
roof (solid, tips 13 m up), with a flash of light and sparks. Sound: a crackling hum from
the dome (heard within 320 m, louder and higher as the charge builds), a thunderclap per
bolt (700 m), a zap for most yard arcs (120 m). Views: `HW_VIEW=tesla`, `tesladome`,
`teslayard`.

His **robot works** stand in the quarter too (part way out from the middle, at least 30 m
clear of the tower; just outside where its blocks are too small), on a 22-25 m yard
(`hw-gen/src/urban/robotworks.rs`, all parts solid), its gate towards the nearest street:
a streamlined assembly hall under a ribbed copper barrel roof with two smokestacks and a
turning robot-head sign ringed with atom orbits, an assembly line under a crane runway, a
dock, a glass foreman's booth on legs, a dispatch shed and a finned gate. It is alive
within 650 m (`hw-client/src/worksview.rs`): every 12 s (2.5 s moving, 9.5 s standing)
the line moves one stop. Robo-Sentries ride pallets out of the hall; a crane lifts a dish
off its rack and lowers it on; two welding arms seam the body in sparks; a second crane
hangs the gun; on the test arch the dish spins, the gun comes up and the lamp goes amber
then green; at the end the robot steps down a ramp and marches into the dispatch shed
(its beacon flashes), and the empty pallet tips over the end drum. In the dock a walking
tank sits half built on its cradle, thighs chained to the frame, sweeping its turret and
nodding its cannon while welders spark at its hips. Sound: the hall's machinery (240 m),
a sizzle per burning torch, clanks, hisses, crane motors, the test's beeps and marching
feet. The robots are the fighting machines' AssetGenerator parts, loaded on a worker
thread near the works. Views: `HW_VIEW=works`, `worksline`, `worksdock`, `workshall`.

- **Street corners:** groups of Lab minions **two levels above the team**, three above at
  the tower's gate. Every fourth corner (and the gate) has a Robo-Sentry in front; others a
  Mutant Alpha. They stand there only while a hero is within 220 m.
- **Rooftop lookouts:** three technicians on flat roofs of two to six floors, 80 m apart,
  shooting down at heroes in the street. Fly up and knock them off.
- **The frayed edge:** in a 70 m band around it, walls carry graffiti, one lamp in five is
  out and one in three flickers, and every street that leads in is half closed by
  barricades (jersey barriers, striped sawhorses, sandbags, burning oil drums). Barricades
  are solid and breakable, and cost nothing to break.
- **Chrome Jackets hangouts:** three greasers on corners in the frayed band (never within
  300 m of where heroes appear), at the team's level. They warn strangers off their turf.

### The town below (`hw-gen/src/urban/underground.rs`)

- **Utility tunnels** under the main roads: concrete, pipes, a glowing Aether conduit along
  the ceiling. Entered through **Aether lifts** in Googie pavilions on the pavements (look
  for the glowing atom signs): step in and hold Ctrl to sink, jump to ride back up.
- **Molehaven**, the mole people's cavern: mushroom houses with round doors, glowing
  toadstools and crystals, a market of striped stalls round a glowing well under an iron
  arch, side caves. Its own lift has an amber sign, and a ramp leads there from the
  tunnels. Short, pale mole folk in goggles keep the stalls, run errands, stroll and chat.
- **The mole drill** (`hw-gen/src/urban/drillrig.rs`, `hw-client/src/drillview.rs`), right
  over Molehaven's great hall (the cavern is planned under it; its bore comes down over the
  well): a riveted red-oxide derrick 23 m tall on its own plinth (radius 15 or 13.5 m), a
  fluted steel drill turning in an iron bore collar that glows with the deep's heat, a
  gearbox with a steam motor under the crown, a green boiler on a brick furnace with a
  flywheel, a banded smokestack, and the mole folk's glowing toadstool lamps. Every part
  is solid. The cycle is a pure function of the game clock (`drillrig::phase`, 36 s, so
  every player sees the same): the drill turns at 0.45 turns a second; 14-21 s into each
  cycle the bore rumbles for 4.5 s (the drill races to 1.35 turns a second and shakes,
  the vents spit steam, the beacon flashes, a whistle blows), then the geyser blows for
  6.5 s (the drill is hauled up 3.2 m, a column of steam roars about 30 m high) and it
  settles for 4 s. Made within 1 000 m, animated within 700 m. Sound: the drill's grind
  (260 m, harder as the pressure builds), the whistle (800 m), the rumble (450 m), the
  blast (700 m). Taxis go there ("The mole drill"). Views: `HW_VIEW=drill`, `drillbore`;
  `HW_DRILL_AT=s` shifts its clock (e.g. 30 to catch an eruption at the start).

### The deep underground (`hw-gen/src/urban/deep/`)

Below the tunnels the town goes on down. It is planned per 256 m tile only when someone
comes near (`TownWorld::deep_tile`, an LRU cache of 384 tiles), so it streams like the rest
of the world and costs nothing where nobody goes. Depths are measured from the lowest
ground nearby, so the layers keep level under hills.

- **The Aether Tube** (about 16 m down): cream enamelled tubes with brass ribs follow the
  main roads (grade at most 1 in 10), meeting at junctions. Every 620 m or so a **station**:
  a 52 by 15 m hall with a barrel vault, sputnik chandeliers, starbursts, red and green tile
  bands, benches, planters and lit destination boards, two gates onto the tube. Most
  stations have a street lift in a red-and-gold pavilion with the station's name; some have
  a lift on down to the Deeps. **Riding:** in a station (its hall or the walkway by it),
  press G and pick a station from the list (those the lines reach, nearest first). A capsule
  glides in (9 m/s), stops at the platform, takes you in after 2 s and runs the way through
  the tubes and across junctions (`TubeNet::path`), speeding up at 6 m/s² to 32 m/s and
  slowing to a stop; you step out in the far station's hall. G on the way gets you out on
  the walkway (`hw-sim/src/tube.rs`). G works as the taxi key everywhere else.
- **The Doctor's lab** (about 30 m down, under the dystopia): a phone kiosk on a back street
  hides the lift. Below: a 44 by 26 m hall with Tesla coils, bubbling vats, consoles, a
  screen wall and a ray cannon; a reactor room, cells with bars, a control room, and an
  escape lift to the Deeps. Three lab guards stand watch (spots for the factions).
- **Buried ruins** (about 34 m down, in about one tile in three, never near a station or
  the lab): a sandstone colonnaded street with fallen columns, rubble and a giant stone head,
  side halls behind arches, a domed rotunda with an oculus and a vault door, braziers and
  glowing glyphs. Two ruin prowlers lurk there; a lift goes on down.
- **The Deeps** (about 52 m down): winding galleries join grottos from tile to tile. Grottos
  are crystal caves, giant-mushroom forests, glowing lakes or fossil halls with great skeletons; some
  hold a mole camp (tents round a fire). Molehaven has a lift down to them.

Everything down there is solid: floors, rock walls and roofs, and every fixture (columns,
vats, coils, tents, crystals) has a footprint the hero bumps into. Lifts work like the
Aether lifts (hold Ctrl to sink, jump to rise). The factions place people at
`TownWorld::deep_spots_in(lo, hi)` (`SpotKind::MoleCamp`, `LairGuard`, `RuinProwler`).

---

## 9. Living streets

### People (`hw-sim/src/citizens.rs`)

Golden-age townsfolk (suits and fedoras, flapper dresses and cloche hats, work clothes) walk
the pavements, wait at corners and chat. How many depends on the place and the hour: busy
shopping streets at midday, quiet back streets, nearly empty at night. They cross at the
lights in the walk phase, step round props, and turn back at barricades. They are a pure
function of seed and time, so every player sees the same crowd. Each building sends people to
the pavement in front of it at the busiest hour: a shop 0.9 plus 0.12 a floor, offices 0.12
plus 0.03 a floor, flats 0.06 plus 0.018 a floor, a house 0.02; at most 0.15 a metre of
street, so a shopping street has someone every seven metres or so.

**Robot street sweepers** (`Citizens::is_sweeper`, `hw-client/src/sweeperview.rs`): 3% of
the people on foot are little golden-age robots (a round cream body, a glass dome with one
eye, two spinning brushes in front). They keep the walkers' routes and hours; the nearest 48
within 140 m of the camera are drawn.

They **react to trouble** (`hw_sim::alarms`): near a fight, mugging, robbery, rampage,
collapse or explosion people run, those further off stop and stare, and they turn to look
when a gang shouts someone off its turf. Knocked down, they tumble as ragdolls and get up.
Heroes cannot target townsfolk.

### Traffic (`hw-sim/src/traffic.rs`)

- **Cars** (finned two-tone cruisers, bubble cars, ground-hugging hover cars) drive lane
  loops through the street network, keeping right, busier downtown and on main roads.
  Junctions have **traffic lights** (36 s cycle with an 8 s all-walk phase), give way inside
  a phase, never enter a junction they cannot leave, follow the car ahead and queue. A car
  stuck a minute turns off and comes back where there is room. Cars stop for heroes, for
  people standing or lying in the road, and short of a fight or collapse.
- **Sky lanes:** hover cars and flying taxis cruise over the main roads at 34, 46 and 58 m,
  banking into turns.
- Cars are solid: you bump into them and can stand on their roofs. Knocked-over lights go
  dark. All players see the same traffic.
- **People in cars** (`hw-client/src/riderview.rs`): every car, hover car and flying taxi
  within 110 m of the camera has a driver behind the wheel, and some carry passengers: 45%
  of cars one to three (a taxi's fares sit in the back, 65% of taxis have one or two). Who
  rides in which car follows from the car, so it is always the same people. They are the
  townspeople's looks in a seated pose ("drive", "ride"), drawn with the people on foot.
  A look whose clothes would poke out of the car sitting down (a long skirt hangs through the
  floor) walks and does not ride (`riderview::fits_seats`, worked out as the look is built).
  Hero taxis have a cabbie.
- **Where it runs:** traffic, people on foot, taxi stands and new incidents live in a 6 km
  square (`AREA_HALF` 3000 m) that follows the heroes: when their middle is 1500 m
  (`AREA_SHIFT`) from its centre, the server makes the next square round them (snapped to
  250 m) in the background and swaps it in once no taxi is out; the old square's wrecks are
  towed. The "Town centre" taxi stand is always on the list.
- **Wrecks** (`wrecks.rs`): punch or blast a moving car and it stops dead in a shower of
  sparks, dented, blackened and smoking, its doors springing open; the lane jams behind it
  until a tow truck you never see clears it once nobody is around. The people inside sit
  stunned for up to 1.4 s, climb out of the door on their side and run (2.9 m/s) onto the
  nearest pavement, where the way is clear of parked cars and street furniture, and along it
  away from the blow, up to 45 m and never past a corner; then they stop and look back. They
  go once the wreck is towed and they are 60 m from the camera, or 260 m away.

### Flying taxis (`hw-sim/src/taxi.rs`)

Press **G**: "TAXI! Where to?" lists up to nine stands: the town centre, the Aether Spire,
the lighthouse or harbour, the airfield, the waterfront, old villages, rooftop pads. A yellow-checkered
taxi drops out of the sky beside you, lands and swings its bubble canopy up; walk in, it
shuts, climbs above the roofs, crosses town and lets you out at the stand. G again hops out
on the way. Rides are free.

---

## 10. Breaking things (`hw-sim/src/destruction.rs`, `props.rs`)

- **Buildings break element by element:** an outer wall panel per window bay per floor,
  slab tiles, roof tiles, inner walls, columns. Glass shatters, walls cave in. Knock out
  enough of a floor and everything above comes down, pancaking or toppling into rubble,
  with dust, sparks and scorch marks. A collapse traps people (section 7).
- **Strength decides what breaks:** each element's strength comes from the building's
  material (stucco 0.8, brick and terracotta 1.0, limestone 1.4, granite 1.8, concrete
  2.2, glass towers 2.4, metal 2.6) and its height (+5% per floor, up to ×2). A hero's
  punch and blast grow 18% per level (`hero_power`): a level 1 hero gets through glass,
  plaster and brick; concrete takes about level 6.
- **Street props break too:** lamp posts and trees fall the way you hit them, parked cars
  land on their roofs, benches, bins and kiosks get crushed, hedges are knocked down,
  broken lamps go dark at night. Bronze statues need a stronger hero. Lift and Throw picks
  props up and throws them.
- **Crates** at the spawn are physics bodies that tumble, roll and land on ledges.
- **The town repairs itself:** a damaged building or prop nobody has been within 300 m of
  for 10 minutes is put back whole, unseen.
- **Collateral** is tallied per player (what broken elements, props and wrecks cost the
  city: window 900, inner wall 400, wall 2 500, slab or roof 3 000, column 6 000). Winning
  an incident's fight with at most 2 000 of it earns the careful-hero bonus (section 4);
  a karma or trust cost for wrecking is not built. The villains' barricades cost nothing.
- Everyone in a co-op game sees the same ruins, including friends who join later.

---

## 11. Playing together (`hw-net`, `hw-sim/src/host.rs`)

- **Host:** title → Play with friends → host. Your world gets a six-letter **room code**
  (top right); hosting also asks your router (UPnP) to forward UDP 7777 and shows your
  internet address. **Join** by room code (through GameHub, no port forwarding needed) or
  by address.
- Up to 10 players. The server in the host's game is the truth; each client predicts only
  its own hero. Friends see your hero as you made them.
- Everyone fights at the host's level; group sizes grow with players; incidents open one
  more per extra hero.
- **Host migration:** if the host leaves a world opened online, GameHub makes another player
  host, the game continues from the save the host published, and everyone comes back where
  they were. Friends who joined by address see the session end.
- **Dedicated server:** `cargo run -p hw-sim --bin hw-headless` (or `Host.bat` in the Windows
  zip) hosts the same town; `HW_SAVE` keeps a world save, `HW_ONLINE=1` lists it on GameHub,
  typing `quit` closes it cleanly.
- Multiplayer has been tested by bots (`tests/soak.rs`: ten bots for an hour) but not yet
  by people (postponed until Arnd has someone to play with).

---

## 12. Screens, sound and settings

- **HUD:** health and energy bars and the power tray (bottom centre), target frame,
  wrist radio (top left), compass (top), version and "What's new" (top right), minimap
  (bottom right: round, turns with the camera so what you look at is up, zooms out as you
  fly higher; marks you, friends, landmarks and radio calls; `minimap.rs`, `mapraster.rs`).
- **Windows:** powers (B: karma, level, shop, give up, faction standings), map (M), menu
  (Escape: settings, controls, quit).
- **Settings:** music and sound volume, reduced motion, picture quality Low to Ultra (each
  says what it shows), "keep it smooth" at 60 fps, VSync (vertical sync; off draws as many frames as the card can), field of view, look
  speed, inverted mouse.
- **Music:** the title theme (a swinging golden-age serial tune) fades out 20 s into the
  game; then 16 Lyria pieces play by scenery and hour (`soundscape.rs`, `Music`; files in
  `assets/music`): downtown, the old town, parks, the waterfront, the villains' quarter,
  Molehaven and flying, each by day, evening, night or morning. One piece, a quiet minute or
  so, then the next; a strong change of scene crossfades. The colossus has its own battle
  march, which takes over. `HW_MUSIC=0` turns the scenery's music off.
- **Sounds** (`soundscape.rs`, clips in `hw-client/assets/sound`): ambience layers that
  crossfade with the place (busy streets by day or night, park, crowd by how many people are
  near, indoors, tunnels, Molehaven, the villains' quarter, wind when flying; the town fades
  out as you climb), engines on the four nearest cars and two nearest fliers, horns from cars
  held up, tyres on hard stops, hover cars whooshing past, sirens a little after robberies,
  explosions and collapses and at waiting ambulances, your footsteps, pigeons, birds, a dog at
  night, café clatter and the town clock. Fight and robot sounds are OpenRouter clips;
  whooshes and title button sounds.
- **Crash reports:** a crash writes `crashes/crash-<ms>.txt` beside the profile; the next
  title points to it once. Send it to Arnd.
- **Windows download:** the GitHub release "windows-latest" (`HeroWorld-windows.zip`),
  rebuilt nightly when the version changed: unzip, double-click `heroworld.exe`. The first
  start downloads the people's bodies and clothes once (about 440 MB) if they are not in
  the zip.

---

## 13. Trying things out: debug switches

**The debug menu: Ctrl+Shift+F12** (single player). Four tabs:

- **Go to**: open incidents on the radio, the town (start, taxi stands, Aether Spire, lighthouse,
  park fountain, lobby of the tallest tower, a half-timbered house, high above the centre), the
  villains (the quarter below the Tesla tower, a rooftop lookout, a villain corner, the Chrome
  Jackets' hangout, the barricade, the graffiti wall, a lab) and underground (a lift, a utility
  tunnel, Molehaven's hall and market).
- **Hero**: set the level, add karma, buy every open power (either theme) or give them all back,
  move each faction's standing, full health, back to the saved hero. A hero changed here is not
  saved (until "Back to my saved hero").
- **Events**: start any incident near you (fights come 150-800 m away, 50-350 m on foot; Go to lists them), bring
  the nearest small house down (a collapse rescue), blast a building, wreck cars, knock over lamps
  and trees or people, break the colossus's weak points one by one.
- **World**: time of day (dawn to midnight), run the world on 10 s / 1 min / 5 min, free camera,
  cut-open view (with floor down / up), town map. The cut-open view has no keys any more, so it cannot be
  switched on by accident.
  Also for picture bugs: "Re-upload meshes/textures", "Check meshes on the GPU" (compares every mesh
  on the card with the game's copy), "Redo window surface", "Rebuild screen buffers", and the
  "Drawing checks" boxes (Engine's `DEBUG_SHADING_FLAGS`: colour by depth, red where depth does not
  match position, hide far things, and more). Findings go to `ENGINE_AUDIT_LOG`; `logs/quality.log`
  records the frame-rate tuner and camera lens. `env.txt` beside the profile sets environment
  variables for a run (`KEY=VALUE` lines).

`HW_DEBUG=<tab>` (`places`, `hero`, `events`, `world`) opens it at start. The environment
variables below start the game in a state worth looking at. On Windows: `set HW_LEVEL=10` then
run, or put them in a `.bat`.

### Hero and progress

| Variable | Effect |
| --- | --- |
| `HW_LEVEL=n` | Start at level n with at least 20 000 karma to spend (not saved). |
| `HW_POWERS=1` | With `HW_LEVEL`: buy every power open at that level. |
| `HW_THEME=1` | With `HW_POWERS`: take the second theme of each. |
| `HW_FLY=1` | Start flying. |
| `HW_STORY=n` | Start with n chapters of the Doctor's story done (not saved); `HW_BRIEFED=1` also briefed on the next. `HW_VIEW=statue` looks at the statue (`HW_STORY=7`). Where each chapter plays: `cargo run --release -p hw-sim --example story_places -- <seed> <people>`. |
| `HW_NAME=...` | Hero name for this run. |
| `HW_PROFILE_DIR=...` | Use another profile folder (a fresh hero). |

### Incidents and fights

| Variable | Effect |
| --- | --- |
| `HW_INCIDENT=1` | The first incident starts at once. |
| `HW_INCIDENT=mugging\|robbery\|rampage\|breakout\|crash\|fire\|colossus` | ...and of that kind. |
| `HW_CARRY=1` | With `HW_INCIDENT=fire\|crash`: start with the hurt one in your arms. |
| `HW_COLOSSUS_BREAK=n` | Break the colossus's first n weak points (2 kneels it). |
| `HW_VIEW=fight` | Places a test group of minions near the spawn (the game starts without one). |
| `HW_AUTOFIGHT=1` | The hero fights on its own (for screenshots); `HW_AUTOFIGHT=17,20` uses only those powers (codes: Freeze 17, Teleport 18, Bubble 19, Sidekick 20, Hypno 21). `HW_CAM_DIST` and `HW_CAM_PITCH` set the hero camera for close-ups. |
| `HW_WRECK=n` | n frames in, the cars near the camera are wrecked. |

### World and camera

| Variable | Effect |
| --- | --- |
| `HW_SEED=n` | World seed (which town). |
| `HW_POP=n` | Town population (default: the profile's city, 40 000 at first; 3 000+ gets a villain quarter). |
| `HW_HOUR=h` | Hour of day, 0–24 (22 for night). |
| `HW_VIEW=...` | Start the camera at a sight: `street`, `sky`, `inside`, `cutaway`, `river`, `quay`, `park`, `lawn`, `props`, `pad`, `timber`, `plinth`, `spire`, `lighthouse`, `tesla`, `tesladome`, `teslayard`, `menace`, `drill`, `drillbore`, `mast`, `mastapron`, `terminal`, `liner`, `rocket`, `rocketpad`, `rocketarm`, `rocketboard`, `atomplant`, `atomgate`, `atomtower`, `works`, `worksline`, `worksdock`, `workshall`, `hall`, `townheroes`, `villain` (with `HW_VILLAIN=delver|buccaneer|gunner|captain|legionary|praetorian`), `airship`, `quarter`, `edge`, `graffiti`, `rooftop`, `hangout`, `traffic`, `skylanes`, `signals`, `wrecked`, `riders` (beside the nearest car's driver; `HW_RIDER=cruiser|bubble|hover|taxi`), `lift`, `tunnel`, `molehaven`, `far`, `market`, `tube`, `station`, `tubeline`, `kiosk`, `lab`, `ruin`, `grotto`, `incident`, `mugging`, `robbery`, `rampage`, `breakout`, `fire`, `crash`, `medic`, `signal`, `map`, `colossus`, `colossus_close`, `sentry`, `alpha`, `citizen`, `powers`, `origin`, `title`. |
| `HW_TAXI=n` | Hail a taxi to stand n at once (`HW_TAXI_SECS` delays it). |
| `HW_WALK=1` | The hero walks on its own. |
| `HW_CUT_LEVEL=n` | Cut buildings open at floor n. |
| `HW_GRASS=0`, `HW_LAWN=0` | Turn grass blades / lawns off. |
| `HW_LOOK=plain` | Turn off the lighting look and colour grade. |
| `HW_PROP=...` | The prop shown by `HW_VIEW=props`. |

### Title, menus, network, crashes

| Variable | Effect |
| --- | --- |
| `HW_TITLE=0` | Skip the title. `HW_TITLE_PANEL=new\|friends\|settings\|creator\|city`, `HW_TITLE_TIME`, `HW_TITLE_AUTOSTART` drive it. |
| `HW_CREATOR_SECTION`, `HW_CREATOR_DETAILED=1` | Open the creator at a section, in the detailed view. |
| `HW_CREATOR_CLIP=hover\|fly\|power` | The creator's hero plays that clip (close-ups of the hero's own poses). |
| `HW_MENU=main\|settings\|controls` | Open the Escape menu. |
| `HW_WHATS_NEW=1` | Open "What's new". |
| `HW_HOST=1`, `HW_JOIN=<code or ip:port>` | Host or join without the title. |
| `HW_NO_UPNP=1` | Do not ask the router to forward the port. |
| `HW_GAMEHUB_URL` | Another GameHub server. |
| `HW_CRASH_TEST=1`, `HW_CRASH_DIR` | Crash at once to try the reporter; move its folder. |
| `HW_PACKS` | Where the Actors packs are. |
| `HW_KEEP_MESHES=1` | Keep every mesh in RAM as well as on the graphics card (the old way, to compare memory). |
| `ENGINE_FRAME_STATS=1` | Engine's frame statistics. |
| `ENGINE_ZFIGHT=1` | Mark z-fighting spots. |
| `ENGINE_SCREENSHOT=...` | Write a screenshot (with `ENGINE_SCREENSHOT_FRAME`). |

Dedicated server: `HW_PORT`, `HW_SEED`, `HW_POP`, `HW_SAVE`, `HW_TICKS`, `HW_ONLINE`.
Showroom for models (`cargo run -p hw-client --bin showroom`): `HW_SHOW`, `HW_LOD`,
`HW_PAINT`, `HW_OPEN`, `HW_YAW`, `HW_PITCH`, `HW_DIST`, `HW_LOOK_Y`.

---

## 14. Built vs planned

| Area | Built now | Planned, not built |
| --- | --- | --- |
| World | One town per seed, any size, regions, water, underground, streamed ground and sea | The planet: continents, many cities, roads and rail, wild land, travel speed by height, fast travel (rocket rails, tubes, teleport booths), moons as places |
| Time | Fixed mid-afternoon; `HW_HOUR` for others | A day/night cycle |
| Zones | One villain quarter (+2/+3 levels), Jackets' band | Level bands across the world, robot armies, liberation that slips back |
| Underground | Utility tunnels, Molehaven, the Aether Tube with stations, the Doctor's lab, buried ruins, the Deeps | Sewers, more lairs, townsfolk riding the tube |
| Heroes | 20 levels, 21 powers in 7 tiers (Freeze Ray, Teleport, Rescue Bubble, Robot Sidekick and Hypno Ray since 0.117), two themes each, a drag-and-drop power tray, 5 origins and affinities | More themes, power roles and budget balancing |
| Looks | Creator with six costumes and sliders | Full costume creator, capes, masks (in progress) |
| Combat | Minions with roles (shield bearer, grenadier, orderly), lieutenant, boss, colossus, drones, revive, hospital; Hero Hall's five heroes give tips and answer ignored fights | Teaming up with the town's heroes, the Doctor in person |
| Factions | Eight factions, regard, turf warnings; mole folk, sky pirates and the Iron Legion with their own incidents and homes | Patrol units, turf changing hands |
| Co-op | Host's level for all | Per-player sidekick and exemplar levels |
| Destruction | Buildings, props, cars, self-repair, collateral tally, careful-hero bonus | Collateral cost, digging and craters |
| Saves | Hero on each PC; world save in `hw-headless` and for host migration | World save for the single-player game |
| Money | Karma only | Dollars for equipment (maybe) |
| UI | HUD, radio, compass, map, minimap, autorun, powers, menu, debug menu (Ctrl+Shift+F12) | |
| Sound | Title theme, colossus march, music by scenery and hour, city soundscape, combat and robot sounds | Streamed music decoding (Engine) |
| Multiplayer | Room codes, UPnP, host migration, soak-tested by bots | A play test with people |
