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
mutant rampages, lab breakouts, car crashes, collapsed buildings and, now and then,
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
- **Origin** (`progress::ORIGINS`), picked once. It decides the hero's power tree (section 4):

| Origin | Blurb | Tree |
| --- | --- | --- |
| Lab Accident | A beaker of raw Aether went off in your face. Science! | Science: rays, gadgets, robots |
| Ancient Relic | You found an amulet older than the moon. It found you back. | Mystic: sigils, spirits, the void |
| Born Mighty | You lifted the family car at six. Mostly by accident. | Might: fists, stone, rage |
| Sworn Guardian | You promised someone you would keep this city safe. | Guardian: light, shields, healing |
| Masked Avenger | The mad scientist's gang crossed the wrong family. | Justice: gadgets, smoke, wits |

- **Element** (`powers::Element`), picked once with the origin and kept for good (the debug
  menu can switch it). It rides on every blow the hero lands (section 4).
- **First power:** the top-left attack of the origin's tree (Atomic Ray, Arcane Bolt,
  Haymaker, Light Lance or One-Two), bought with the starting karma.
- **Start:** 960 karma (exactly one first-row power), level 1, no powers except the free ones: Strike,
  Super Jump, Sprint (0.251) and the portal (Portal Home and Open Portal). A hero
  below level 5 starts on the **rookies' street**: the side street
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
name, world seed, the hero file (karma, lifetime karma, origin, element, powers,
faction regard), looks, volumes, picture and mouse settings. The hero is always
kept on the player's own PC; joining a friend brings your hero into their world.

The game client keeps **no world save** between runs in single player: damage, broken
props and the clock start fresh each time (the town repairs itself anyway). The dedicated
server `hw-headless` keeps one with `HW_SAVE`, and a co-op host publishes one every 10 s
for host migration (`hw-sim/src/save.rs`).

---

## 3. Controls

From `settings::CONTROLS` (shown in the game under the menu button → Controls) and `fightview.rs`:

| Key | What it does |
| --- | --- |
| Mouse | Look around: by default the pointer is hidden and the mouse always looks (Settings, Mouse: "Always look with the mouse"); with that off, hold the right button. P locks the pointer too |
| Alt (hold) | Shows the pointer for clicking while always-on mouselook is on; windows, menus, chat, map and police computer free it by themselves |
| W A S D | Run |
| R | Autorun: the mouse steers, A/D step aside, S or R stops |
| Space | Jump; climb when flying |
| V | Fly (once you own Flight); V again drops you |
| Ctrl / Shift | Sink / faster when flying Shift also runs at full tilt with Super Speed |
| Mouse wheel | Zoom in / out (all the way in: first person) |
| Tab | Target the nearest villain in view; again: the next nearest not yet picked, round and round |
| Left click on a villain | Target that villain, up to 150 m away (Tab reaches 60 m); within punching reach (3 m) it also punches |
| Left click on a teammate, or F1-F4 | Pick that teammate as your friendly target (the same again lets go); the team frame on the left lists every other hero with a health bar and takes clicks too. Tab never picks a teammate |
| H | Next to a Herobot on Hero Hall's plaza (or one you woke): wake it, or send it home |
| Left mouse | Strike |
| 9 / 0 | Portal Home (mark where your portal leads) / Open Portal (tear it open ahead) |
| Q | The first bolt power of your tree (Atomic Ray, Arcane Bolt, ...) |
| 1 to 0, Alt+1 to Alt+0 | The power in that slot of the power tray (see below) |
| B | Powers and karma window (also faction standings) |
| E | Open or shut the door you stand at (a rescue or a downed teammate near takes E first) |
| E (hold) | Revive a downed teammate; help trapped people out; pick up the hurt |
| H | Call the medical drone when you are down |
| G | Hail a flying taxi (G again hops out mid-air) |
| M | Town map (drag to move, wheel to zoom) with the radio's calls |
| J | Case file: open cases (pick which one the radio follows), the Doctor's dossier, the cabinet of closed cases |
| T | Police visor on or off (a new call switches it on) |
| N | Minimap on or off |
| Menu button, "Arrange the screen" | Drag the minimap, compass, power tray, radio and police monitor; drag a corner to resize (0.5x to 2.5x); Enter or the tick ends, the red arrow resets. Saved in `ui_layout.json` beside the profile. Chat, powers and police windows are draggable and remembered |
| O | Free camera |
| Enter or / | Chat: type a line, Enter says it (in a speech bubble over your head and in the chat window), Escape cancels |
| K | Knock nearby townspeople over (a ragdoll test, still in the game) |
| Escape | Frees a locked mouse and closes the open window; it no longer opens the menu |
| Alt+Enter | Window or fullscreen (borderless); the chat and the mouse stay as they were |
| Menu button (round, top right under the police computer icon; Alt frees the mouse to click it) | Menu: back, settings, controls, arrange the screen, back to the title screen (asks first when hosting), quit. A draggable, resizable HUD piece (`menu` in `ui_layout.json`) |

On foot the hero steps up and down ledges up to 0.45 m (`collision::STEP`) without
leaving the ground, so slopes, kerbs and stairs never count as a fall; a bigger drop
does. Walking into a higher ledge, up to 1 m (`collision::STEP_UP`), climbs it at 6 m/s
(`CLIMB_SPEED`, `Obstacles::walk_body`; a raised doorway sill, a plinth, a low wall): only
for a hero standing on something and not rising, so a jump or fall meets walls as before.
The fall pose shows only after 0.15 s off the ground (`heroview::AIR_GRACE`),
a jump at once.

**The power tray** (`hw-client/src/tray.rs`): twenty slots at the bottom of the screen, 1 to 0
and Alt+1 to Alt+0 (the Alt row shows above the number row once it holds anything, or while
something is dragged). A power you buy takes its usual key if that is free (Strike 1, Bolt 2,
Hold 3, Shield 4, Lift 5, Shockwave 6, Mend 7, Beam 8, Meteor 9, Nova 0; the portal's two
keys come on 9 and 0 for every hero, so Meteor and Nova go elsewhere unless it is moved), otherwise the first
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
  bot before the power trees; about 75 since, to be balanced); `tests/high_tiers.rs`
  plays all ten (`cargo test --release -p hw-sim --test high_tiers -- --ignored`).

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
- **Reviving a teammate** pays no karma.

### Elements (`hw-sim/src/powers.rs`, `combat::element`)

Every hero has one element, picked with the origin and kept for good. It rides on every
blow the hero lands on a villain (Strike and every power), scaled by the blow, and shows
on every hit. Shields, heals and rescues take the element's colour; attacks look like what
they are made of (atomic green, void purple, light gold, ...).

| Element | On every blow |
| --- | --- |
| Fire | Burns on: 15% of the blow again over 3 s. |
| Frost | Slows the villain 15% for 3 s (not a control). |
| Energy | Shoves the villain back 1 m (not the heavy ones). |
| Necrotic | 5% of the blow comes back to the hero as health. |
| Lightning | 20% of the blow leaps to one more villain within 5 m. |
| Psychic | One blow in five (at most once in 4 s): the villain wanders confused for 1 s. |

Control effects (holds, slows, confusion, fear, charm, disarm) last full length on
minions, half on lieutenants, 15% on bosses; machines are never confused, afraid or
charmed (`combat::RANK_RESIST`).

Heroes saved before 0.164 got all the karma they had spent back once, and pick an element
when they next play (the element card); then B to pick powers from their tree.

### Grapple Swing (`lib.rs`: `swing_step`, `grapple_anchor`)

Replaced Glide in 0.279 (Glide was a slow fall that Flight did strictly better at the same
price). Hold **F** and the line hooks the first thing the camera aims at, within 45 m
(level 1) to 70 m (level 20) and 1.2 m over the spot, so a swing carries over a ledge. While
held the hero is pulled toward the hook (38 m/s², softening in the last 6 m), gravity still
works and the stick steers a little (14 m/s²), so the hero swings in an arc and keeps the speed
on letting go (fling off at the top). Top speed 26 (level 1) to 36 m/s (level 20), plus 15%.
It costs no energy and stops no energy recovery. Server and client prediction run the same
`step_moving`; the client draws the line to the same hook. Aimed at empty sky or beyond reach,
F does nothing. Augment "Speed" adds to the swing speed as it does to a super jump's. A hero
who had bought Glide (id 4, now in `powers::RETIRED`) gets the karma back at the next load.

### Buying powers: the trees (`progress.rs`, `powers.rs`)

Press **B**. The window shows the hero's origin tree and, on the second big tab ("Travel powers"), the travel tree every hero can buy from (Flight, Super Jump, Super Speed, Grapple Swing, Wall Run, Sprint, Hero's Return). Boxes are
powers, arrows what they need: owned powers gold, ones to buy now lit blue, too dear grey,
locked dark; helping powers have a green edge, capstones a star. A toggle's details say
what share of energy recovery it takes while on. Click a box for what it
does, what it needs, and Buy or Give up.

- **Every power is one record** (`powers::PowerDef`): where it sits in the tree, what it
  needs, the mechanic it is built on (how it is delivered and looks) and its numbers in
  three groups: on others (damage, range, area, hold, freeze, slow, burn, confuse, fear,
  charm, disarm, weaken, spread, drain, ...), on the hero and friends (heal, shield,
  reflect, resist, regen, might, haste, hide, auras, rescues, sidekicks, ...) and movement
  (travel, where a moving power lands, a trail). A special is just a number that is
  usually zero. **Energy and recharge come from the numbers** (`powers::energy`,
  `powers::recharge`), not from a table.
- **Trees, not levels, gate powers:** a power needs the ones its arrows come from (all of
  them; "A or B" where two arrows join). Powers in an exclusive **fork** ("or" between
  them) are picked once for good; the two capstones at the bottom of each tree are a fork.
  A hero buys only from their origin's tree and the travel tree.
- **Price:** `960 × row price × (1 + 0.25 × powers owned)`, row prices 1, 1.5, 2.25, 2.75,
  3.25, 4.25 (`progress::ROW_PRICES`). By level 20 a hero owns about thirteen powers:
  about half their tree and a few travel powers; karma after the cap buys more.
- **No power ranks:** a power grows with your level automatically (damage ×
  `1 + 0.1 × (level − 1)`, so ×2.9 at level 20).
- **No respec**, but giving a power up refunds 90% of what you paid. A power that another
  owned power needs cannot be given up until that one is.
- **Free powers** (`PowerId::FREE`, 0.251): every hero starts with Strike, Super Jump (2),
  Sprint (3) and the portal. Super Jump and Sprint are boxes of the travel tree that are
  simply already owned (shown as free, never for sale), so Grapple Swing, Flight and Super Speed are
  open from the start (Grapple Swing and Flight need Super Jump, Super Speed needs Sprint) and Wall
  Run only needs Super Jump or Super Speed. They are not counted towards Hero's Return's
  "two travel powers" (those must be bought). Heroes who had bought them earlier get the
  karma they paid back in full the next time the hero file loads (`progress::sanitize`).
- **Strike** is everyone's free punch (20 damage, 3 m, 0.7 s) and carries the element.
  **Q** fires the hero's first bolt power.
- **The portal** (0.192, after MasterWorld's portal stone; `hw-sim/src/gates.rs`,
  `hw-client/src/gateview.rs`): every hero has it, free, as two powers on keys 9 and 0.
  **Portal Home** (124, 2 s recharge) marks home on the floor 1.3 m ahead, its way in facing
  the way the hero faces; it is kept in the hero file (`portal_home`, per town: seed and
  size) and a new mark closes the open portal. A hero with no home in this town (a new hero) starts with one already marked on Hero Hall's plaza, a few steps out from the greeters, facing away from the hall (0.246; `Landmark::portal_home`). **Open Portal** (125, 4 s recharge) tears a
  portal open on the floor 2.7 m ahead, facing the hero; a hero has one open at a time (a new
  one moves its far side). Each opening is an ellipse 2.6 m wide and 3.5 m high, shrunk to
  86%, 74% or 64% where a full one has no room; under that, or with a wall, a floor more
  than 0.55 m out of level or home within 6 m, it fizzles ("No room for a portal there") and costs
  nothing. Both faces work (0.198, as MasterWorld's did): anyone (heroes, not villains)
  walking into a front comes out of the other side's front, and into a back out of the other
  side's back, turned with it and keeping their speed; the camera turns with the hero and looks back
  through the portal for a moment, so the step never cuts. Portals close when their hero
  leaves (16 open at most in a world).
- How it looks: each pair is two Engine portals (front to front, back to back), the other
  place seen through either face, opening from its middle over 1.8 s. Only the nearest open
  portal within 90 m shows its view, one level deep (0.211, as MasterWorld drew it; a view
  inside a view and every portal at once cost most of the frame rate). The edge boils like
  MasterWorld's rift: two layers of moving noise push it in and out round the opening, the view
  fades to violet toward it, a bright flickering line runs along it and sparks drift through
  (custom material `portal rim`, drawn over the view since Engine 0.059); wisps and rune motes
  crawl round it, and a lamp in its colour (home green, far side blue) glows on the street.
  Too far off for the view it is a swirling disc (`portal haze`). Engine 0.058 fixed a ghost of
  the view flickering over it while the camera moved; Engine 0.059 draws only what fits
  through the opening. Marking home sends a beam down to glowing rings on the
  floor; a closing portal sucks its light in; a failed one spits sparks and a dark puff;
  whoever steps through rises in rings of light. `HW_PORTAL=1` runs it on its own
  (`HW_PORTAL_SHOT=<dir>` saves `open.png`, `walking.png` and `through.png`, and with
  `HW_PORTAL_ORBIT=<degrees>` also `orbit.png`, seen from round the back).
- The server checks every purchase against the hero file.
- **Every power must help in a hard fight** (Arnd, 0.177): no power only makes an easy
  thing easier. Rescues need no powers of their own; they stay a plain hold-E job, flavour
  and easy karma for new heroes. The old rescue powers (faster freeing, rescue bubbles)
  became fight powers in the same places in the trees.
- **Toggles** (`PowerDef::toggle`): most defensive powers (resistances, regeneration,
  dodging, bracing, reflection, wards) and the auras are switched on and off with their
  key. While on, each takes its share of the hero's energy recovery
  (`Fields::upkeep`, 15% to 40%; several add up, to none at all), so a hero chooses
  between being hard to hurt and firing powers often. A toggle costs no energy to switch
  and can be switched again after 1 s; holding its key flips it once. The tray shows a
  toggle that is on in green with its share. A downed hero's toggles go off. Second
  Wind, Guardian Angel, Lifeline and the small passives (Scanner Goggles, Opening Move,
  Cuff 'Em, Martial Master, Gadget Belt) are always on.
- **Flight costs energy recovery:** while a hero flies (in the air with Flight on), no
  energy comes back at all, so hovering out of reach and firing down is no longer free.
  Super jump, grapple swing and wall run are not affected.
- **The reworked powers in numbers:** Scanner Goggles +25% on held, frozen, confused or
  afraid villains (adds to a power's own `exploit`); Spirit Sight 1 villain blow in 5
  misses; Brace 30% less while standing still (under 0.5 m/s, on the ground); Second Wind
  40% of health back once dropping below 25%, once in 90 s; Opening Move doubles the first
  blow on a villain not yet fighting the hero; Guardian Orb a 40% shield on the hero for
  8 s; Ring of Judgement 20 damage round the hero and 15% more taken for 8 s; Stasis Cage
  holds a 5 m area 4 s; Ghost Lantern sends villains within 8 m fleeing 4 s; Lifeline
  also gets the hero up on their own 10 s after going down, once in 120 s; Cuff 'Em also
  makes a minion left below 20% of its health give up; Mobile Aegis and Martyr cover
  sidekicks too, and the hero at half (`combat::OWN_WARD`).

### Travel powers in numbers (`hw-sim/src/lib.rs`)

All of them grow with the hero's level, evenly from level 1 to level 20
(`travel_growth`). Running is 7 m/s.

| Power | Level 1 | Level 20 |
| --- | --- | --- |
| Super Jump, full height (Space held) | 14 m (over a three storey house) | 45 m (over a ten storey block) |
| Super Jump, speed through the air | 15 m/s | 22 m/s |
| Sprint | 13 m/s | 17 m/s |
| Super Speed | 24 m/s | 34 m/s |
| Super Speed with Shift | 40 m/s | 58 m/s |

- **Super Jump** takes off at once in the way the hero runs (at least 80% of the leap
  speed) and steers hard in the air (45 m/s², a leap turns round in about half a
  second). Letting go of Space on the way up doubles gravity until the top, so a tap is
  a hop of about half the height (indoors, onto a low roof) and a held jump goes all the
  way. Take-off kicks up dust with a whoosh (`FxKind::Launch`).
- **Hard landing:** a super jumper coming down faster than 18 m/s (`HARD_LANDING`, about
  an eight metre fall) lands with a stomp (`combat::leaps`, `FxKind::Landing`): a ring of
  dust, grit, a crack in the ground, a thump and camera shake, all bigger the higher the
  leap. Villains within 3.5 m (up to 7 m from the top of a level-20 leap) and in sight
  take 7 to 21 damage (`STOMP_DAMAGE`, times the level scale) and are knocked over.

### Augments (`hw-sim/src/augments.rs`, `hw-client/src/augmentview.rs`)

Loot. An augment adds a share of **one effect** to **one power**, for good.

- **What one is** (`hw_net::Augment`): an effect, a percent and a grade. The percent is a
  share of the power's own worth ("magnitude 1", from `powers::value`): 20% Hold on a
  bolt holds for a fifth of what a hold power of the bolt's worth would. Where the power
  already does the thing, it does more of it; where it does not, it starts to, weakly
  (new behaviours have a floor so they show: a hold at least 0.8 s, a shove 1.5 m).
- **Effects** (`Effect`, 39 of them, each with its own icon): on powers that hurt:
  Damage, Burn, Drain, Exploit, Leap (the hit jumps on), Knockdown, Launch, Push, Hold,
  Freeze, Slow, Confuse, Fear, Charm, Disarm, Weaken, Range, Area, Wreck, Spare; on
  heals, shields and buffs: Heal, Shield, Reflect, Lasting; on anything fired: Quick
  (recharge) and Thrifty (energy); on kept powers: Resist, Dodge, Brace, Regen, Might,
  Haste, Aura, Ward, Light (a toggle takes less energy recovery); on travel powers:
  Speed, Height, Stomp, Trail (running at 11 m/s or more hurts villains within 2.5 m).
  `fits` says which effect goes on which power; the window lists them under each power
  ("Takes").
- **Flight takes Speed only.** No augment makes flying cheaper.
- **Yes/no effects are amounts now:** knock, spare, revive, hide, execute, Guardian
  Angel and steadfast are 0 to 1 in `Fields`. A part knockdown staggers
  (`STAGGER_FLOORS`: 0.3 fells a minion, 0.6 a lieutenant, 1.0 a boss; it fades 0.1 a
  second), a part verdict fells minions below that share of health, part stealth
  shortens how far villains see the hero.
- **Sockets:** three per power, or as many as there are effects that fit it (Flight has
  one). The three must be different effects. Free powers have sockets too. Setting is
  for good: an augment cannot be taken out, a new one in its socket destroys the old
  one (the window asks first), and giving a power up loses its augments.
- **Strength** (`strength`): by the level of the villain it came from, level 1 to 20:
  common 8% to 20%, rare 10% to 25%, relic 12% to 30%.
- **Where they come from:** some villains carry one from the moment they appear
  (`carry`, `DROP_CHANCE`: 2.3% of minions, 1 in 5 lieutenants, every boss; a boss's is
  rare or better 7 times in 10). A carrier shows a gem over its head in the grade's
  colour (copper, blue, gold) and has that many percent more health. Every hero who
  helped beat it gets their own copy, with their own effect: 7 in 10 fit a power they
  own (`FITS_OWNED`). A hideout case closed gives each hero there a find, never a common
  one. About 20 an hour (`tests/augments.rs` plays it).
- **The bag** holds 30 (`MAX_BAG`); a drop into a full bag is traded in at once.
- **Giving one away** (`Host::offer_augment` / `answer_offer`, `ToServer::OfferAugment` / `AnswerOffer`,
  `ToClient::AugmentOffer`): in the bag, "Give to a teammate" lists the other players (Herobots too).
  The receiver gets a prompt (Take it / No thanks); the augment stays in the giver's bag until it is
  taken, one offer at a time per giver (a new one replaces the old). Heroes must stand within 30 m
  (`TRADE_REACH`), the offer lapses after 60 s or when either player leaves, and a full bag on the
  receiving side refuses it. Both sides get a line in chat. Herobots always take what they are given.
  Tests: `hw-sim/tests/allies.rs`.
- **Trading in** (`trade_value`): karma to spend (it does not raise the level). A common
  one from a level 1 villain gives 48, from level 20 120; a rare one three times, a
  relic ten times that.
- **Villains keep up** (`combat::villain_growth`): from level 5 on villains have more
  health than a tenth per level, up to 40% more at level 20, and hit harder by half of
  that, to match a hero's augments.
- **On screen:** the powers window (B) shows the bag beside the tree, and every owned
  power's sockets as small chips inside its box in the tree (the free powers are boxes
  below it). Drag an augment from the bag onto the power: it goes into the first free
  socket, or replaces a weaker one of the same effect; drop it on a chip to replace that
  one (the window asks first). Or click it in the bag, then a socket. Powers and sockets
  it fits light up, and a green arrow marks one that would improve a power. Under the
  tree the picked power lists what each of its augments does in numbers. The tray shows
  a dot per socket. A drop flies from the villain to the hero and shows as a card.
- **Saved** in the hero file (`augments`, `bag`; hero file version 3). Travel boosts are
  sent as whole percents (`CombatState::boosts`), so the client predicts them exactly.

### The powers

Every origin tree has 22 or 23 powers in six rows; the travel tree seven. The window says
each one's numbers; the names and links:

#### Common

| Row | Power | What it does | Needs |
| --- | --- | --- | --- |
| 1 | **Super Jump** (free) | leaps over rooftops; lands with a stomp | – |
| 1 | **Sprint** (free) | runs twice as fast | – |
| 2 | **Grapple Swing** | hold F to hook what you aim at and swing from it | Super Jump |
| 2 | **Flight** | flies anywhere | Super Jump |
| 2 | **Super Speed** | runs like the wind | Sprint |
| 3 | **Wall Run** | runs up walls | Super Speed or Super Jump |
| 3 | **Hero's Return** | home to Hero Hall | any 2 of its tree |

#### Science

| Row | Power | What it does | Needs |
| --- | --- | --- | --- |
| 1 | **Atomic Ray** | bolt, knocks down | – |
| 1 | **Scanner Goggles** | 25% more on helpless foes | – |
| 2 | **Ray Burst** | a cone of rays | Atomic Ray |
| 2 | **Gravity Cuffs** | holds one 7 s | Atomic Ray |
| 2 | **Transmitter** | teleports 40 m | Scanner Goggles |
| 2 | **Force Field** | friends take half | Scanner Goggles |
| 3 | **Cryo Ray** | freezes a cone, douses | Ray Burst |
| 3 | **Sonic Boom** | blast, knocks down | Ray Burst, Gravity Cuffs |
| 3 | **Tin Pal** | robot sidekick | Gravity Cuffs, Transmitter |
| 3 | **Stasis Cage** | holds an area 4 s | Transmitter, Force Field |
| 3 | **Vital Ray** | heals friends near | Force Field |
| 4 | **Disintegrator** | beam through a line | Cryo Ray, Sonic Boom |
| 4 | **Gravity Well** | pulls a crowd in | Sonic Boom |
| 4 | **Hypno Spiral** | a minion fights for you | Tin Pal |
| 4 | **Reflector Field** (fork 1) | shield sends 25% back | Vital Ray |
| 4 | **Repulsor Field** (fork 1) | shoves villains out | Vital Ray |
| 5 | **Orbital Ray** | strike from the sky | Disintegrator |
| 5 | **Tractor Beam** | hurls cars from afar | Gravity Well |
| 5 | **Overclocked Pal** (fork 2) | one strong robot | Hypno Spiral |
| 5 | **Twin Pals** (fork 2) | two small robots | Hypno Spiral |
| 6 | **Atomic Nova** (fork 3, capstone) | huge blast, 16 m | Orbital Ray, Tractor Beam |
| 6 | **Singularity** (fork 3, capstone) | a crushing black hole | Tractor Beam, Overclocked Pal or Twin Pals |

#### Mystic

| Row | Power | What it does | Needs |
| --- | --- | --- | --- |
| 1 | **Arcane Bolt** | bolt, knocks down | – |
| 1 | **Spirit Sight** (toggle, 20% of energy recovery) | 1 blow in 5 misses you | – |
| 2 | **Binding Sigils** | roots hold one 7 s | Arcane Bolt |
| 2 | **Hex** | target takes 25% more | Arcane Bolt |
| 2 | **Ghost Lantern** | villains near flee 4 s | Spirit Sight |
| 2 | **Aether Step** | teleports 40 m | Spirit Sight |
| 2 | **Telekinesis** | lift and throw | Spirit Sight |
| 3 | **Frost Sigil** | freezes a cone, douses | Binding Sigils |
| 3 | **Charm** | a minion fights for you | Hex, Binding Sigils |
| 3 | **Spirit Ward** | a strong barrier | Ghost Lantern |
| 3 | **Void Grasp** | pulls in and burns | Aether Step, Telekinesis |
| 3 | **Wraith Form** (toggle, 20% of energy recovery) | takes 20% less | Telekinesis |
| 4 | **Spirit Lance** | beam through a line | Frost Sigil |
| 4 | **Curse of Weakness** (fork 1) | an area takes 20% more | Charm |
| 4 | **Circle of Sigils** (fork 1) | holds an area 4 s | Charm, Spirit Ward |
| 4 | **Soul Siphon** | a cone that heals you | Void Grasp |
| 4 | **Aether Mend** | heals friends near | Wraith Form |
| 5 | **Starfall** | strike from the sky | Spirit Lance |
| 5 | **Familiar** | a spirit hound fights | Curse of Weakness or Circle of Sigils |
| 5 | **Void Tendrils** | holds a crowd 5 s | Soul Siphon |
| 5 | **Aether Vitality** (toggle, 25% of energy recovery) | 2% health a second | Aether Mend |
| 6 | **Eclipse** (fork 2, capstone) | stars rain for 8 s | Starfall, Familiar |
| 6 | **Void Rift** (fork 2, capstone) | swallows a street | Void Tendrils, Aether Vitality |

#### Might

| Row | Power | What it does | Needs |
| --- | --- | --- | --- |
| 1 | **Haymaker** | big punch, knocks down | – |
| 1 | **Brace** (toggle, 15% of energy recovery) | standing still: 30% less | – |
| 2 | **Uppercut** | launches a villain | Haymaker |
| 2 | **Thunderclap** | blast, knocks down | Haymaker |
| 2 | **Mighty Heave** | lift and throw | Haymaker, Brace |
| 2 | **Tough** (toggle, 15% of energy recovery) | takes 15% less | Brace |
| 2 | **Second Wind** | near defeat: 40% health back | Brace |
| 3 | **Shoulder Charge** | dashes, bowls over | Uppercut |
| 3 | **Stomp** | slows everyone round | Thunderclap |
| 3 | **Lamp-post Swing** | sweeps all round you | Mighty Heave |
| 3 | **Boulder Toss** | rips up the street | Mighty Heave, Tough |
| 3 | **Iron Constitution** (toggle, 25% of energy recovery) | 2% health a second | Tough, Second Wind |
| 4 | **Combo Flurry** | three quick blows | Shoulder Charge |
| 4 | **Fissure** | cracks a line | Stomp |
| 4 | **Giant Swing** | flings a villain far | Lamp-post Swing, Boulder Toss |
| 4 | **Berserk** (fork 1, toggle, 20% of energy recovery) | 30% harder, 15% softer | Iron Constitution |
| 4 | **Bulwark Stance** (fork 1, toggle, 15% of energy recovery) | never knocked down | Iron Constitution |
| 5 | **Meteor Leap** | jump-slam from afar | Combo Flurry |
| 5 | **Car Slam** | hurls a car | Fissure, Giant Swing |
| 5 | **Steel Hide** (toggle, 40% of energy recovery) | takes half | Berserk or Bulwark Stance |
| 6 | **Earthshaker** (fork 2, capstone) | huge blast, 16 m | Meteor Leap, Car Slam |
| 6 | **Unstoppable** (fork 2, capstone) | a titan for 20 s | Car Slam, Steel Hide |

#### Guardian

| Row | Power | What it does | Needs |
| --- | --- | --- | --- |
| 1 | **Light Lance** | bolt of light | – |
| 1 | **Guardian Orb** | shields you 40%, 8 s | – |
| 2 | **Shield Bash** | punch, knocks down | Light Lance |
| 2 | **Healing Light** | heals friends near | Light Lance |
| 2 | **Lifeline** (helper) | revives from 25 m; gets you up | Guardian Orb |
| 2 | **Guardian Aegis** | friends take half | Guardian Orb |
| 2 | **Protector's Bond** | a strong barrier | Guardian Orb |
| 3 | **Shield Throw** | bounces to three | Shield Bash |
| 3 | **Halo Burst** | blast that heals | Shield Bash, Healing Light |
| 3 | **Ring of Judgement** | foes round you take 15% more | Lifeline |
| 3 | **Sanctuary** | heals friends, slows foes | Guardian Aegis |
| 3 | **Intercept** | leaps in, shields there | Protector's Bond |
| 4 | **Radiant Beam** | beam through a line | Shield Throw |
| 4 | **Light Pillar** | a burning column | Halo Burst |
| 4 | **Steady Heart** (toggle, 25% of energy recovery) | 2% health a second | Ring of Judgement, Sanctuary |
| 4 | **Reflective Aegis** (fork 1, toggle, 15% of energy recovery) | sends 20% of blows back | Sanctuary |
| 4 | **Mobile Aegis** (fork 1, toggle, 15% of energy recovery) | friends near take 15% less | Intercept |
| 5 | **Heroic Aura** (toggle, 25% of energy recovery) | friends near hit harder | Light Pillar |
| 5 | **Guardian Angel** (fork 2) | survives a fatal blow | Steady Heart |
| 5 | **Martyr** (fork 2, toggle, 25% of energy recovery) | friends near take 25% less | Steady Heart |
| 5 | **Bulwark of Light** | a big barrier | Reflective Aegis or Mobile Aegis |
| 6 | **Dawn** (fork 3, capstone) | heals and revives all | Heroic Aura, Guardian Angel or Martyr |
| 6 | **Last Bastion** (fork 3, capstone) | a dome blows bounce off | Guardian Angel or Martyr, Bulwark of Light |

#### Justice

| Row | Power | What it does | Needs |
| --- | --- | --- | --- |
| 1 | **One-Two** | a quick double blow | – |
| 1 | **Opening Move** | first blow on the unwary: double | – |
| 2 | **Disarm** | gunmen can't shoot 6 s | One-Two |
| 2 | **Combat Roll** | dodges almost all, 2 s | One-Two |
| 2 | **Grapple Line** | swings you 35 m | One-Two, Opening Move |
| 2 | **Smoke Bomb** | confuses, hides you | Opening Move |
| 2 | **Cuff 'Em** | beaten-down minions give up | Opening Move |
| 3 | **Takedown** | crushes held foes | Disarm |
| 3 | **Sweep Kick** | knocks all round down | Combat Roll |
| 3 | **Bolas** | tangles one 4 s | Grapple Line |
| 3 | **Intimidate** | minions flee | Smoke Bomb |
| 3 | **Shadow Cloak** | villains lose you 8 s | Smoke Bomb, Cuff 'Em |
| 4 | **Trusty Partner** | a masked friend fights | Takedown |
| 4 | **Martial Master** (fork 1) | punches leap on | Sweep Kick |
| 4 | **Gadget Belt** (fork 1) | powers recharge 30% faster | Bolas |
| 4 | **Flash Bang** | confuses a crowd | Intimidate |
| 4 | **Unbreakable Will** (toggle, 20% of energy recovery) | steadfast, 20% less | Shadow Cloak |
| 5 | **Zip Line Strike** | dashes through a line | Trusty Partner |
| 5 | **Stun Darts** | leap on to three | Martial Master or Gadget Belt |
| 5 | **Rallying Cry** (toggle, 25% of energy recovery) | friends near hit harder | Flash Bang |
| 5 | **Vanish** | villains lose you 10 s | Unbreakable Will |
| 6 | **Final Verdict** (fork 2, capstone) | minions fall at a blow, 10 s | Zip Line Strike, Stun Darts |
| 6 | **Reign of Fear** (fork 2, capstone) | every villain near flees | Rallying Cry, Vanish |

**Sidekicks** (Tin Pal, Familiar, Trusty Partner and the like; `hw-sim/src/sidekick.rs`)
trail 2.2 m behind and a little to the right while their hero moves, keeping the hero's
pace and hurrying when behind. When the hero stands they walk straight in (never round)
and rest anywhere 1.2-3.4 m away, turned to the hero. Standing idle, after about 3-5 s and
then every 6-14 s, they do a little trick (`hw-client/src/tricks.rs`): wave, hop, look
around, polish their chest core, tilt their head, stretch, or a short jig. Over 30 m away
or stuck 2 s, they beam back.

Villains do not use these records yet (planned). Health comes back faster as it grows:
regeneration (1 a second in a fight, 14 out of it at level 1) scales with your maximum
health.

---

## 5. Fighting (`hw-sim/src/combat.rs`, `hw-client/src/fightview.rs`)

City of Heroes is the model: groups of minions, tougher lieutenants and bosses, tab
targeting, ragdolls, nobody dies.

- **Targeting:** Tab picks the nearest villain within 60 m that you can see (none behind a
  wall), those in view of the camera first; each further Tab moves on to the next nearest one not picked yet, and after all of
  them it starts over (after 4 s without Tab it starts again from the nearest). An aimed
  power with nothing targeted takes the villain nearest the crosshair, and a villain that
  attacks you becomes your target if you have none. A left click on a villain targets it,
  up to 150 m away (`CLICK_RANGE`), if the hero can see it; a click never reaches past the
  first villain under the mouse to one behind. Within punching reach (3 m) the click also
  punches; farther away it only picks, and holding the button does not punch the air. The target frame shows name,
  rank and level in its **con colour** (how tough it is against you).
- **Using powers:** one power at a time (0.5 s lock); a power that cannot fire yet (out of
  range, behind a wall) waits up to 1 s and then fires, and one still recharging waits until
  it is ready (if that is within 3 s, `combat::RECHARGE_WAIT`). A power that does not fire
  always says why in the middle of the screen ("Atomic Ray: out of range", "can't see them
  from here", "still recharging", "Not enough energy"; `FxKind::Fizzle`), also when the key
  is pressed again and again (each press tells why the last one did not fire, once a second
  at most). A power key always goes before a held mouse button or Q, and a held button never
  pushes aside another power waiting to fire. Every power key and what became of it is in the
  debug menu's Log tab (and `logs/debug.log`).
- **Seeing over things:** a fighter sees another if the line from chest to chest is clear, or the
  line from head to head (`combat::in_sight`): a parked car, a hedge or a cafe table hides the
  chest but not the head and shoulders, so heroes and villains shoot each other over them. Blows land with sparks, beams, scorch
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
  combat (within 6 s of the last blow given or taken). Toggles that are on take their
  share of the energy, and none comes back while flying.
- **Knockdown:** knocked fighters tumble as ragdolls for 2.2 s and need 1.4 s to stand; they
  can be hit but cannot act.
- **Defeated villains** lie in glowing energy cuffs; after 2 s a police drone lifts them away.
- **Defeated heroes** lie down. A teammate holds E within 2.5 m for 3 s to revive them at
  half health. Otherwise press H, or wait 20 s, and a medical drone flies them to the
  nearest hospital: it climbs over the roofs (at least 12 m over the hospital's 35 m
  pylon), crosses town at 40 m/s, comes down at 7 m/s onto the landing ring in front of
  the entrance, and puts them in a free recovery pod (six per hospital) for 5 s, health
  filling up, after which they step out at full health inside the Recovery Hall. Without
  a hospital (a bare test ground) they come back on the spawn street as before. Carrying
  someone and getting knocked down drops them.
- **Hospitals** (Aether Mercy): one per town, 250-750 m from the core and clear of the
  villains' quarter; a city has one more per 300 000 people (at most 4, 1.2 km apart, at its
  business districts and old villages). The hall's ground floor is open: desk, pods, healing
  core, waiting corner, couches. Taxi stand "Aether Mercy Hospital".
- **Comet buildings** (the new game-scale interiors, `hw_gen::layout`): up to three near the
  core, one per scheme: the Daily Comet building (atrium, 56 x 52 m, 5 storeys), the Atlas
  Assurance hall (grand hall) and the Zenith Radio house (split levels); a tight town gets
  fewer or smaller ones. Storeys 5.5-8 m high, halls at least 10 m across, steps 0.2 m high
  and 0.35 m deep, rails 1.1 m. Every wall, floor, step, rail and column is one solid box
  the game both draws and collides with (`hw_gen::layout::solid`). Villains find their way
  between rooms and floors by a route map of each plan (see Villain AI). Views:
  `HW_VIEW=comet|cometlobby|cometgallery|cometstairs|comethall|cometlift`, `HW_COMET=0|1|2`.
- **Office buildings in town** (uses Offices and Shops with offices, outside the old town's
  lanes) are buildings of halls too: their plots are 34-50 m wide and 40-52 m deep, their
  storeys about 6 m high (a building as tall as before has fewer of them, 2 to 16), planned as
  an atrium or a grand hall with level floors. Their facades are the town's own; inside are the
  lobby, galleries, open stairs, office halls, lamps and furniture, all solid, with ceilings (no
  cut-open view there). Where such a plot does not fit, the quarter's other use is built. Their
  lifts run like the Comet buildings' (worked out for the buildings within about 70 m of a
  hero). Their windows sit at most 7.5 cm deep in the wall. Some towns have none yet: small
  ones (under about 10 000 people), and bigger ones that are all old lanes.
- **No two storeys alike** (grand hall and atrium plans): every upper storey has its own
  height (5.5 to 7 m) and divides its rooms anew (one open hall, two halls of unlike size, or
  a hall with a row of small rooms 6-7 m deep). About one part in four is two storeys high,
  and the storey above looks down into it through glass. In a grand hall plan the front band
  is 4 m shallower or 3 m deeper on some storeys, and on about a third of them the concourse
  and the rooms on one side are one open hall. In an atrium plan a wing on about a third of
  the storeys has no gallery (one open hall up to the rail), the middle of the front merges
  with a corner now and then, and bridges cross on about half the storeys from the second up.
- **People in those buildings** (`hw_gen::layout::people`): places to stand on every floor,
  on open floor at least 0.55 m from anything solid, 0.9 m from walls and 2.6 m from a
  doorway's middle; one to every 22 m2 of a hall or boardroom, 16 m2 of a shop, 30 m2 of a
  lobby or atrium floor, 40 m2 of a gallery, arcade or concourse. Who stands there follows the
  hours of the people indoors (halls as offices: 60% of places taken from 8 to 18 o'clock); a
  gang's hideout has none (0.284: those inside are the gang);
  the 120 nearest the camera in space are shown.
- **Breaking them** (0.217): a building of halls breaks along what stands in it. Outside walls (pieces up to 4 m, cut at every storey) and windows are each storey's load-bearing ring; inner walls, lift shafts and columns hold the floors up; floors and roofs are tiles of about 3 m; stairs, landings, bridges, rails and glass walls break on their own and carry nothing. The ground floor does not break. Furniture stays solid in a damaged building, below where a collapse starts. The broken look is built on a worker thread, so the hole shows a few tenths of a second after the first blow.
- **Shop houses** (0.218): in the old town's lanes a shop with rooms above is a house of halls too, on a plot 12-15 m wide and 22-28 m deep (homes beside it keep their narrow plots). 2 to 4 storeys, the ground floor 5.5 or 6 m high and each upper one 4.5 to 5.5 m. Every storey is one room across the whole house (the shop at the street, 4 m doors), with a wide stair in one back corner and a small room in the other; no lift. Where the house is deep enough, about a third of the storeys are two storeys high with an arcade under a 5 m gallery along the back. So small towns have these interiors too: a town of 3 000 has about 100 shop houses.
- **Gang dens** (0.273): a hideout on a plot too small for an office building's halls (12 m wide and 22 m deep at least, so nearly every hideout in the old town's lanes) is a **den** (`hw_gen::layout::den`), a house built to be fought through. It is one storey higher than the house it was (3 or 4 storeys). **One great hall** stands across its full width through its whole height (15 to 24 m long, 14 to 28 m high): at the street, at the back, or in the middle with rooms either side (plots 27 m deep or more, three times in four). **Balconies** hang across its two ends, 3 to 5 m deep as the dice say: one at every storey where rooms are behind it, and others between storeys (a storey of 6 m or more is mostly climbed in two) or as lookouts. **Open flights** 2.6 m wide climb along the side walls from a balcony at one end to the next one up at the other, left wall and right wall in turn, so the way to the top crosses the hall each time; a flight that ends short of its balcony gets a landing out to it. **Bridges** 2.6 to 4 m wide (one in five the whole width between the stairs) cross between two balconies of one level. Balconies over one another stand at least 3 m apart. **No two storeys next to each other have the same height**: 4 to 7.5 m, the ground floor at least 5 m. The **rooms** in front of the hall or behind it (5.5 to 9 m deep) are one across the house or two side by side on each storey, by the dice; three times in ten one of two is **two storeys high**, and the room over its neighbour looks down into it through glass. A room opens to its balcony by a door, a pair of doors, a door beside a wall of glass, or not at all: no wall, like a box at the theatre. A den has no lift. Villains walk every balcony, bridge and flight (the route map takes a balcony's meeting with a landing or bridge square, like a doorway). **Balconies and bridges are furnished** (0.284, `furnish::Room::balcony`): along a balcony's rail a walk 1.4 m wide stays free (the flights arrive and leave there); behind it a line of cover (sandbag walls, 0.6 m deep and 0.85 m high, 1.8 or 2.4 m long; on balconies 4 m deep or more crates and oil drums too) with gaps of 1 to 2.6 m; and against the wall, in what depth is left, pin boards, safes, crates, drums and cabinets. Nothing stands within 2.2 m of where a flight leaves or arrives, in a line from a door across to the walk, where the balcony meets a bridge or landing, or at its middle. A bridge 4 m wide or more gets sandbags along both rails and the walk down its middle; narrower bridges and landings stay bare. About four in five balconies and wide bridges carry something. Sandbag walls also stand among the card tables and cots on the hall floors of hideouts. Hideouts on big plots keep the office buildings' grand hall or atrium; those on plots too shallow for a den keep their flats.
- **Houses of flats** (0.220): homes in the old town's lanes are houses of halls as well, on plots 12-15 m wide and 24-28 m deep, 2 to 4 storeys (ground floor 5 m, upper ones 4 to 5 m). The ground floor is the house's hall (4 m front doors, seats, an arcade at the back to the stairs) and a store room. Every upper storey is one flat: a living room across the front, a kitchen to eat in behind it (5.5 or 6.5 m deep, the door to the stairs), a bedroom in the back corner and, in houses 14 m wide or more, now and then a second bedroom off the living room. Flats have sofas, a television, sideboards, a kitchen run with stove and fridge, a table with chairs, beds, nightstands and wardrobes; shops have a counter at the back, rows of shelves and display cases. Where such a house does not fit, the old narrow house stands.
- **Apartment blocks** (0.222): homes outside the old town's lanes stand on plots 30-40 m wide and 28-34 m deep, 3 to 12 storeys (ground floor 5 to 6 m, upper ones 4 to 5 m). The ground floor is a lobby across the whole front with columns and a desk, and a store room in each back corner. Stairs and two lifts stand at the middle of the back; on every storey above a landing 16 m wide and 6 m deep lies in front of them with a flat either side. Each flat has its living room at the street (15 m wide or more, 10 m deep or more), a kitchen to eat in beside the landing, a bedroom in the back corner and, half the time where the block is wide enough, a second bedroom off the living room; the flat's door leads off the landing into the kitchen or the living room. Where a block does not fit, a house of flats (above) stands there, and where that does not fit either, a narrow house of the old kind.
- **Insides are drawn near you** (0.222): the inside of a building of halls (floors, walls, stairs, lamps) is drawn while the camera is within 90 m of its walls and dropped beyond 120 m, made on worker threads, three buildings at a time, the nearest first. Furniture has its own reach (130 m). A town of 3 000 took 1.4 GB less at its peak in our test run.
- **Doors in buildings of halls** (0.224): the way in from the street is a pair of glass doors (1.8 m) in a glazed entrance; every single door between two rooms (2.4 m wide, 3.2 m high: rooms off a lobby, a flat's door on its landing, bedrooms) has a pair of leaves that stand shut and are solid. They swing into the smaller room, never into a landing. E opens and shuts them within 2.6 m; people on foot open them and they shut again behind them. Wider openings (4 m and more) stay open.
- **Wide houses** (0.226): on plots too shallow for a house of flats (less than 24 m deep) homes stand on wide plots of 23-30 m by 16-19 m. The stair is at one end of the back with a landing at the street. A house of flats (2-5 storeys) has one flat on every storey, the ground floor too: a living room the whole depth of the house (9 m wide or more), and a kitchen and a bedroom at the far end. A family's house (two storeys) has the living room, kitchen and a store downstairs and an upper hall with two bedrooms upstairs. Where even that does not fit, a small house of the old kind stands.
- **Sharing a block** (0.228): a building of halls takes at most half the depth of its block (from its street's building line to the far street's), so the row on the other street fits too; where the block is too thin for two rows of its kind, one row takes the whole depth (up to 1.5 times its least depth). Where the wanted building does not fit, homes are tried in its place before a small house of the old kind: an apartment block, a deep house of flats, a wide one.
- **The villain quarter** (0.230): its tenements are houses of flats of the same kinds as the town's homes (block, deep house, wide house), in the quarter's sooty brick. The Doctor's laboratories are buildings of halls: on a big plot the atrium or grand-hall kind, otherwise the narrow kind with a front room at the street (12-22 m by 22-28 m); the dome, coil and stack stand on the roof as before. A fire escape keeps clear of the doors, and is left off where no pair of bays is free.
- **Loft houses** (0.233): where no bigger home fits (plots of 12-17 m by 8.5-11 m, or what a thin block leaves), a loft house stands: two storeys, a living room 7.5-9 m high at one end (at least 9 m wide, the depth of the house), a straight flight of steps along its back wall, and at the other end a kitchen (open to the room or behind double doors) under a bedroom gallery with a rail. A family's house that has no room for the wide kind is a loft house too. Only plots under 12 m wide or 8.5 m deep still get a small house of the old kind (about 45 in a town of 20,000, down from 1,150).
- **The colossus and halls** (0.233): a building of halls' ground floor never breaks, so the colossus's middle keeps 3 m from its footprint: it slides along the wall to the open side (a charge ends), rams the building as before, and for 20 s after a wall stood in its way it follows the main streets towards its hero or goal. One already inside walks out freely.
- **The night club indoors** (0.234): in a building of halls that is a night club (one big shop building in five downtown), the hall behind its door (a shop house's shop, an office building's lobby) is furnished as the club: the band's stage (4-9 m wide, 0.45 m high, with a curtain and footlights) at the far wall or wherever there is room, a bar of 3-10 m with stools down a side wall, booths along the walls that are left, a jukebox by the way in, and tables for four; a hall at least 15 m across has a round dance floor in the middle with the tables round it. All of it is solid.
- **Laboratories indoors** (0.240): in a building of halls that is a laboratory (the villain quarter's), halls, boardrooms, shops and service rooms are work halls: islands of two bench rows back to back (3 m wide, a 2.6 m gap between islands) alternate with rows of Aether coils (1.2 m across, 2.2 m high) every 5 m, with benches, cabinets and bookcases along the walls; the lobby and an atrium keep their lobby furniture and get up to six coils on the free floor. All of it is solid.
- **Works and pavilions indoors** (0.241): works and fair pavilions stand on the offices' big plots as buildings of halls (a small one is a hall on the street with rooms above). In a works every hall has machines on a 6 m grid (stamping press 2.2 x 1.7 m and 2.9 m high, lathe 3 m long, Aether dynamo 2 m square), workbenches, crates and cabinets along the walls, and up to ten machines in the lobby. In a pavilion exhibits stand on a 7 m grid (a model rocket 3 m high, an atom, a globe with its moons, each on a round plinth 1.6 m across), with show cases, sofas and plants along the walls. A piece taller than its room is left out. All of it is solid.
- **Furniture** in those buildings (`hw_gen::layout::furnish`): 850 to 1 400 pieces a
  building, every one solid (rugs excepted). Office halls have desks in blocks of two rows,
  runs of four with 2.6 m gaps and 3.4 m aisles; other halls are archives (rows of shelves)
  or canteens (long tables, six chairs); lobbies have a front desk facing the doors and up
  to four seating groups; galleries have benches, plants and display cases along their real
  walls. Kept clear: 2.8 m at doorways, 3.2 m at the foot and head of stairs, 0.9 m round
  stairs, columns and lift shafts, a 2.2 m walk along a hall's walls.
- **Lifts** in those buildings run on a timetable from the game clock (no call button): a
  car waits 7 s at a floor with its doors open (they take 0.9 s to slide), rides at up to
  3 m/s to the next floor, up to the top and down again; neighbouring cars leave 11 s
  apart. Walk in while the doors are open and stand: the car carries you. Shut doors are
  solid, so nobody falls into a shaft. Server and clients work the cars out from the same
  clock (`hw_gen::layout::lifts`, `hw_sim::lifts`), nothing is sent.
- **Villain AI:** villains notice heroes within their aggro range and in sight, walk round
  walls and houses on a grid A* path (`hw-sim/src/nav.rs`), and go home when the fight
  moves away. Ranged ones keep their distance. A walk counts as free only if the body fits
  (the line is checked at both shoulders, 0.34 m out), and grid cells need that much room
  round them. In a building with a floor plan (the Comet buildings) they use its route map
  (`hw-sim/src/floornav.rs`): points on both sides of every doorway (0.9 m out), the foot
  and head of every flight (0.7 m off the steps), where landings meet, off the corners of
  stairs standing in a room, the street 2 m out of each entrance; any two points on one
  floor area are joined unless a stair is in the way. The route is found again every
  1.5 s; inside a room the grid path goes round desks. Lifts are on the map but villains
  take the stairs for now.
- **Co-op level:** everyone fights at the **host's** level (health, damage and new groups
  follow it), so friends of any level can play together. Group size grows with players
  (5 minions alone, +1 per extra hero, up to 8), not their toughness.
- **Beaten groups come back** at their corner 15 s later, but not while a hero stands within
  25 m; ambient groups go home once no hero is within 420 m.

---

## 6. Villains and factions

### Factions (`hw-sim/src/factions.rs`)

Nine factions with a standing toward each other from −100 to 100: Hostile (≤ −60),
Unfriendly (≤ −20), Neutral, Friendly (≥ 20), Allied (≥ 60).

A hero belongs to none. **Each faction starts out regarding a new hero exactly as it regards
ordinary townsfolk**, so a new hero is just another passer-by. Deeds move that regard
(kept on the hero file): beating one of a faction's people costs 2 points with it and gains
1 with every faction that dislikes it (townsfolk and the Patrol included). The B window
shows where you stand.

| Faction | Who | Toward a new hero | Notes |
| --- | --- | --- | --- |
| Townsfolk | everyone on the pavements | Friendly (50) | Heroes cannot target them. |
| City Patrol | the law | Friendly (50) | Police drones, and (0.272) Sky Patrol cruisers with two officers that come to a closed hideout case. They do not fight yet. |
| The Doctor's Lab | the mad scientist's mutants, technicians and robots | Unfriendly (−40) | Hates the Patrol and the Syndicate. |
| Chrome Jackets | greaser street gang | Unfriendly (−30) | At war with the Syndicate. |
| Velvet Syndicate | pinstriped gunsels, organised crime | Neutral (−10) | Keeps the peace when it suits business. |
| Moles | Molehaven's mole folk | Neutral (0) | Hate the Lab and the Empire. Delvers guard Molehaven's stair. Their surfacing event (talk them down with E for +6 Molehaven) is parked until it is redesigned: it no longer comes up on its own (HW_INCIDENT=moles still forces it). |
| Sky Pirates | an airship crew | Unfriendly (−30) | Moor over the harbour with lookouts on the quay (warn, then fight); raid shops from an airship moored over the street (12 m reach). |
| Iron-Masked Empire | the Iron Legion, from beyond the sea | Unfriendly (−55) | Hates everyone. Lands in iron capsules and holds two townsfolk captive (20 m reach); only once the team is level 2 and has two heroes, or rarely alone. Never attacks an unknown hero on sight. |
| Zorrak Invaders | bug-eyed troopers from Zorrak, the small quick moon | Unfriendly (−55) | Hate everyone, the Empire most. A flying saucer hangs over a street (solid, its lens 13 m up and 4 m over any roof) and beams two townsfolk up through a green tractor beam while its troopers guard them (20 m reach); only once the team is level 2. Never attack an unknown hero on sight. |

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
| The Doctor's Colossus | Lab | Colossus | 1 300 core | 26 | 170 m | 400 | The walking tank (below). |
| Delver | Moles | Minion | 50 | 6.5 every 2 s | 2.4 m | 6 | Pale, goggled mole miner with a pick. |
| Buccaneer | Sky Pirates | Minion | 42 | 6.5 every 1.7 s | 2.5 m | 6 | Striped shirt, cutlass. |
| Deck Gunner | Sky Pirates | Minion | 34 | 6.5 every 2.4 s | 26 m | 6 | Long coat, ray musket. |
| Sky Captain | Sky Pirates | Lieutenant | 160 | 11 every 1.9 s | 2.7 m | 22 | Wide hat, red beard, gilded cutlass. Leads raids. |
| Legionary | Empire | Minion | 48 | 7.5 every 2.6 s | 28 m | 8 | Iron mask and cuirass, shock rifle. |
| The Doctor | Lab | Archvillain | 1 400 | 15 every 2 s | 30 m | 220 | Only in the last chapter of his story, in his lab. White lab coat, wild white hair, goggles, a Tesla staff. Throws lightning; when a hero comes within 13 m he **charges an Aether storm** (1.8 s, a green ring on the ground) that hits every hero still inside for 1.6× his blow, every 9 s or so. |
| Praetorian | Empire | Lieutenant | 190 | 12 every 2.2 s | 3 m | 24 | Gilded mask and cuirass, shock lance. Leads landings. |
| Zorrak Zapper | Invaders | Minion | 42 | 7 every 2.4 s | 26 m | 7 | Slight, pale teal or lilac, a great bald head with big black eyes, a fishbowl helmet with antennae, silver suit, red gauntlets and boots; a ray gun. |
| Zorrak Grabber | Invaders | Minion | 50 | 7 every 2 s | 2.9 m | 7.5 | Taller and heavier, gold trim; a glowing grabber claw on a pole. |
| Zorrak Overlord | Invaders | Lieutenant | 180 | 11 every 2.4 s | 24 m | 24 | The biggest head of all under a tall glass dome with a glowing crest, a purple coat; fires a brain ray from a sceptre from the back row. Leads saucer raids. |
| Mutant Bulwark | Lab | Minion | 52 | 7 every 2.2 s | 2.4 m | 8 | A brute with a big brass atom shield on its left arm. Aimed blows (bolts, beams) from more than 3.5 m in front mostly clang off the shield (a quarter gets through); blasts, holds and anything from close by or from the side get through. Get close, or go round it. |
| Lab Grenadier | Lab | Minion | 32 | 11 every 4.2 s | 18 m | 7 | Olive overalls and a leather helmet. Lobs a fizzing bomb where you stand: a red ring shows where, and it bursts 1.9 s later (2.6 m round). Step out of the ring. |
| Lab Orderly | Lab | Minion | 30 | 5 every 2.6 s | 20 m | 7 | Mint overalls, white gloves. Every 4 s it patches up a hurt friend within 14 m (14 health at level 1, a green beam); in between it shoots. Take it out first. |
| Safe | – | Locked | 200 | – | – | 10 | A gang's safe with a hideout case's thing in it. Tab to it and break it open; it stays where it stands. |
| Weak point | – | Part | 240 | – | – | 25 | A breakable part of a robot: the colossus's knees and cannon, the sentry's dish. Tab to it. |

Brutes, bulwarks, grenadiers, orderlies, technicians, Jackets and gunsels are knocked down by knockback powers; the sentry,
alpha and colossus are "heavy" and are not. Mixed Lab groups are three brutes to two
technicians. **Roles** (`combat::with_roles`): in a Lab group of four or more, one brute
carries a shield (bulwark); a group of five or six also gets a grenadier or an orderly (by
the seed), and seven or more get both, so each group has someone to rush, someone to shoot
and someone to deal with first. Groups with a set crew (a rampage, a corner gang) keep it. One in four of a group is a level above the rest.

The **mad scientist** himself, the Doctor, shows only at the end of his story (below).

### The colossus (`hw-sim/src/robots.rs`)

A set piece in three acts and a rampage, best with friends. It marches out of the robot works' gate (the Tesla tower's in a town without works) once the team
is level 3 or more, at most every 25 minutes, and the Hero Signal calls everyone. It strides
at 3 m/s (2.4 on two legs) and turns 30° a second. Out of every hero's sight (450 m) it keeps
to main roads at least 11 m wide (A* along road centre lines); with a hero in sight it leaves
the streets: it heads straight for the nearest hero within 220 m, through the houses, or from
building to building when nobody is that near. Its feet and hull break what they walk into,
and every 4 s (9 s in a fight) it rams a building within 18 m ahead (1.3 s warning: an orange
ring on the wall; a column of 4.5 m blows from 1.5 to 9 m up, and heroes within 6 m take
1.2× its damage). Its damage is 26 a blow at level 1, and grows with level like everyone's.
Its health grows with the number of heroes.

1. **Walking, armoured:** blows on the hull glance off (5% gets through; the game says so).
   Its moves, each warned on the ground first:
   - **Shells:** 1.8 s warning (red ring), 7 m blast, 10–170 m, every 5 s; 1× damage.
   - **Stomp** on heroes under it: 1.2 s warning, 12 m, every 5 s; 1.5×.
   - **Charge:** at a hero on the ground 24–90 m off, every 16 s: it lowers its head for
     1.8 s while a red lane shows where it runs, then runs it at 9 m/s (up to 80 m, 15 m past
     the hero), bulldozing a lane through buildings; heroes within 7 m of it take 1.8×.
   - **Aether lance:** at a hero 20–110 m off, every 15 s: it plants itself and charges the
     cannon for 2.6 s while a violet line shows on the ground, then a beam from the muzzle rakes
     the line from near to far in 1.2 s; heroes within 2.5 m of the beam take 2.2×, and it
     scores the street and cuts the floors it passes. **Overload:** knock 15% off the
     cannon's health while it charges and the beam fizzles, the cannon loses another 30%, and the
     machine reels for 4 s (no moves).
   Its **four glowing knee actuators** and the **cannon** are weak points (240 health each).
2. **Down:** with two knees gone it drops on its belly, the clamshell opens and the
   **Aether core** takes full damage. Two robo-sentries per hero drop from the hatch, and
   the core discharges every 7 s (13 m, 1.2×; fly or jump more than 2.5 m up to be clear).
3. **Enraged:** after 25 s it hauls itself up on two legs: shells come in salvos of three
   (8 m apart) every 4 s, charges every 11 s, the lance every 10 s.
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

### Cases (`hw-sim/src/missions/`, `hw-client/src/{missionview,police}.rs`)

**Gang hideouts** are the one kind of case for now (the street, plaza, tunnel, Deeps and
chain cases are parked in the code, not offered). They are **found on the police computer**, not
at Atomic Annie (she keeps the hall's steps and no longer hands anything out):

- **The police computer**: a cheesy monitor with antennae in the HUD (top right, under the
  version; a red badge counts the crimes waiting), or key **Y**. It opens a **desktop** of
  icons: **Crime-o-Vision**, Case files (J), Crime scanner (the visor, T), City map (M) and
  Hero powers (B). It takes the pointer; Y or the sputnik button closes it.
- **Crime-o-Vision** (`police.rs`): a glassy overlay whose opacity wavers like a bad picture
  tube (with a rolling band and the odd glitch; steady with reduced motion). It lists the
  **10 crimes** on the board, nearest first. Each row has the crime's **symbol** (payroll robbery: money
  sack; counterfeit ring: banknotes; smuggling den: crate; stolen prototype: ray gun; art
  theft: framed painting; protection racket: ledger), its title and the hideout's name ("Old
  hotel on Juniper Street"), the **job** with an icon (**arrest the boss**: handcuffs, **find
  the thing**: magnifier), the **gang** (a coloured dot and its short name), the gangsters'
  **level** (the team's level plus 0-2; green when easy, gold on par, orange and red when tough)
  the hideout's **size** (one, two or three little houses lit: DEN, HIDEOUT, FORTRESS)
  the **minutes** it stays on the board (red from 3 down), a pulsing **HOT TIP** flag,
  and the **distance** from the hero. **TAKE** opens the case (FULL when 3 are open, ON IT when
  taken). The radio's heading and compass then follow it.
- **The board** (`missions/board.rs`): crime number *n* is a pure function of the world's seed
  and *n* (which hideout, what crime, which job, how many levels over the team, its size,
  whether it is a hot tip). The board holds **10 crimes**; each stays **10 to 30 minutes**
  (`LIFE_SECONDS` 20 min, times 0.5-1.5 by its number), a **hot tip** (one in six) half as long,
  and the next number takes its place when it is closed or its time is up. A crime that is
  taken stays until its case closes. A **hot tip pays half as much again**. A crime **nobody
  took** makes the streets within 450 m of its hideout **rough for 10 minutes**: street crime is
  as common there as round a gang's hangout (`EMBOLDENED_SECONDS`), and the radio says so. The
  gang is picked from those that come out at the team's level. Hideouts are the exception among
  the town's **buildings of halls** (2+ floors, at least 12 m a side): **one tenement in four,
  one shop-and-office block in six** (works keep their machines); the rest are homes and shops
  with their own furniture. Most hideouts are **dens** (0.273, see "Gang dens" under the
  buildings of halls): a great hall with balconies, bridges and open stairs. A hideout shows from the street: its **ground-floor windows are
  boarded up** (planks nailed across, no light behind them), and where cars may park in front of
  it, **the gang's car** stands at the kerb (0.267): a long black Cruiser under a blood-red roof,
  a paint no other car wears (`GANG_PAINT`; about 4 hideouts in 10 have the kerb for one). It is
  a parked car like any other: solid, and it can be thrown.

The team can have up to **3 cases open** at once (`OPEN_LIMIT`; nothing else assumes a number),
never two in one building. Cases are numbered with the crime's number and each is against one of
five gangs (the Doctor's sentries, Jackets or Syndicate, sky pirates, the Iron Legion, the
Zorrak; the last two only from their levels on).

**A hideout case is one step in one building**, in three beats: the lookouts at the door, the
rooms, and a finale that depends on the job.

- **Size** (`board.rs`, `SIZE_ROOMS`): a **den** holds 2 rooms, a **hideout** 4, a **fortress** 6,
  besides the boss's room (never more than the building has: rooms are halls, boardrooms,
  lobbies, shops and concourses at least 8 m a side, and a flat's rooms and back rooms at least
  3.8 m). **3 villains a room** for one hero and one more for each friend. In a town of seed 3
  (20 000 people): 39 dens, 38 hideouts, 10 fortresses.
- **Lookouts**: **2** stand at the way in (`DOOR_SIZE`).
- **The alarm** (`ALARM_SECONDS`): a crew that has fought a hero for **6 s** and still stands
  shouts for help, and the **nearest crew that has heard nothing** comes along. One room, not a
  chain: a crew that was called does not call on, and the boss's guard never leaves the boss. A
  room taken down inside 6 s calls nobody.
- **Arrest the boss** (takeover): the **boss** (named in the case file and on the radio) waits
  on the top floor with 3 guards, **1 level over the gang's**. Below **40 %** health it **breaks
  off once**: for up to 9 s it fights nobody and runs through the building, down the stairs if
  need be, to the crew still standing that is farthest away (with none left: to the way out),
  and that crew takes the hero on (`BOSS_BREAKS`, `BOSS_RUN_SECONDS`, `Enemy::flee_until`). There
  it turns to fight again. Beating the boss closes the case.
- **Find the thing** (evidence): **one real object** the crime is about (a strongbox of cash,
  printing plates, a crate with a glowing canister, the prototype in its case, a painting
  propped up, a ledger), **locked in the gang's safe** in a far corner of a room on a high
  floor. The safe (0.267) is a real thing: 1.1 m wide, 1.5 m high, solid, square to the walls,
  its door towards the room's crew. It is a target like a villain (**Tab** or click it, name
  "Safe", rank "Locked", **200 health** at level 1 and 60 % more per extra hero, 10 karma) that
  never fights back and is never carried off: **break it open** with any attack, from in front of
  its door. When it gives, the door hangs open, the thing shows inside, and **the noise calls the
  nearest crew that has heard nothing** (as a room's alarm does). The thing is also **guarded by
  its room's crew**: it cannot be taken while they stand. Then hold **E** within 2.8 m for 3 s.
  An opened safe stays where it stands until no hero is within 90 m (`SAFE_GONE`). Taking it
  **raises the whole gang**: everyone still standing, the boss and its guard too, comes for the
  hero, and a crew of **2** cuts off the way out at the door (`CUTOFF_SIZE`). The case closes
  when the hero who took it is **3 m outside the building** (or nobody is left to stop them).
- **Hideouts are furnished like hideouts** (`Fit::Hideout`): cots with rumpled blankets,
  card tables with chips and cards, safes, planning boards with a pinned map, photos and red
  string, oil drums and crates, besides the room's own furniture.
- **Standing about is not standing still**: villains that stand guard (hideouts, corners) take a
  few steps to somewhere within 2.8 m now and then (about half of every 8 s), and each has its
  own idle: lounging, keeping a lookout, counting money or sharing a laugh, changing every
  12-20 s.

Out in the open the gang stands in the middle and on open floor round it (parked): a takeover is
the boss with 3 guards plus 2 side crews; hostages, crews and clues as in the code.

The radio, the compass, the map diamond and the teal beacon follow **one tracked case**
(the newest, until you pick another in the case file). Every open case plays on regardless: its
gang stands when a hero comes near and the thing can be taken, tracked or not.

**The way in is marked** (0.284, `missionview::mark_doors`): the street door of every open
case's building (tracked or not) gets a **teal neon tube round the doorway** (0.3 m out each
side, up to the top of the 4.4 m entrance) and **three teal chevrons** on the pavement in front
of it pointing at the door, glowing so they show from up the street and by night, from when the
case is taken until it closes or moves on to another place. The door is the building's street
entrance whose spot 2 m out is the case's place (found on a worker thread).

In a building:
- **Room by room**: each room is its own group with its own sight. A group notices a hero
  only in aggro range and in sight, and only its own room's friends come along, so a floor
  can be cleared one room at a time without bringing the whole gang. Villains who saw you
  follow you through doors and up stairs by the building's route map (see Villain AI).
- The gang moves in when a hero comes within **90 m** of the place (on the flat), and leaves if
  no hero is within **250 m** for **30 s** (the case stays; it comes back when you do).
- **The ending**: when a hideout case closes, whoever of the gang still stands **gives up**
  (down and cuffed, no karma: the police drones take them in), and **someone waits outside the
  door to thank the heroes** (the thing's owner, a neighbour), then walks off.
- **The police come** (0.272, `hw-sim/src/police.rs`): 2 s after the case closes a **Sky Patrol
  cruiser** sets out: a black and white flying car with a red beacon, gold stars, twin fins and
  chrome skids. It flies in above the roofs from 220 m off (42 m/s), sinks straight down (7 m/s)
  and **lands by the door**: in the street as a car would stand (the near lane first, then the
  middle of the road), else on open ground in front of the house. The place is looked for a
  couple of candidates a tick, never all in one tick: the whole 5.5 x 2.8 m box open, on ground
  no steeper than 30 % and even to 12 cm, in no building, under an open sky, and with a clear
  walk for each officer. It stands tipped as the ground lies. While it comes down and stands,
  an alarm of the kind `Police` keeps cars **off the road within 6 m** and waiting **9 m** off,
  on every machine. A cruiser standing still is **solid** (`cruiser_solid`, the box its model is
  built in). The **canopy swings up** (1.2 s) and **two officers** get out, one each side, and walk
  (1.5 m/s) to **posts either side of the door**. **How many cruisers** (0.284,
  `running::cruisers_for`, by the case's dice): a gang of 2 rooms or fewer gets none 3 times in
  10, else one; 3 or 4 rooms none 15 %, one 45 %, two 40 %; more rooms one 20 %, two 40 %,
  three 40 %. They come 5 s apart and stand at least 6.5 m apart. **The officers go in and
  collect evidence** (0.284): from the post they walk into the building the way villains do
  (its route map, round furniture, up the stairs, as bodies that collide) to where the gang held
  out (each officer one or two of the rooms and the boss's place), and there **stand collecting
  for 8 to 14 s** (reaching down, `Officer::busy`; a place they cannot reach within 6 s of no
  progress is looked at from where they got if within 3 m, else skipped), then walk out to the
  post. After 150 s inside they head out whatever is left. When more than one cruiser came, one
  officer of the first **guards the door** facing the street until everyone is out (40 s at
  least). Then they walk back, get in, the canopy shuts and the cruiser climbs away. An officer's
  walking pace is what they really make (pressed against something, they are drawn standing:
  0.284 fixed officers walking on the spot). Called without places to look at (`HW_POLICE`) both
  stand guard 40 s. Where nobody can
  stand by the door, or no ground is free, it **hovers 9 m over the door for 25 s** and nobody
  gets out. In the test town 76 of 87 hideouts have a place to land (0.273: a second police alarm at the posts by the door holds the road there too, since in the old town's lanes a door opens straight onto the road; in 0.272 the officers were not allowed to walk there and only 31 of 87 cruisers landed). One cruiser per place;
  with no hero within 320 m the police are gone at once. They do not fight, cannot be targeted
  and are not saved.
- **Officers** are Actors people (`people::officer`, two looks: a man, a woman): navy shirt and
  tie, navy trousers, black riding boots, white gloves, a gold buckle, and a **peaked cap** with
  a gold badge and cord hung on the head (`policeview::cap`).
- **Reward**: every hero within 150 m gets a share of the karma their next level takes when
  the case closes: **15 %** for a den, **25 %** for a hideout, **40 %** for a fortress, and a
  tenth more of that for each level the gang stands over the team (`share_of`); half as much
  again for a hot tip.

**Adding to cases** (the hooks): a new crime is a `CrimeKind` value (its words, symbol in
`police::crime_icon`, the thing in `pieces::evidence`); a new kind of place is a `Place` value, a
finder function and a line in `sites::SOURCES`; a new job is a `CaseKind` value (how its gang
stands and when it is done in `running.rs`, its words in missionview and casefile). Everything
else (steps, radio, case file, saving) works from those.

**The case file** (J; `hw-client/src/casefile.rs`): Hero Hall's manila folder, typed on a
typewriter. It slides in on its own for 7 s when a case opens (a NEW CASE stamp) or closes
(CASE CLOSED); J opens it for good and frees the pointer (J or Escape shuts it, Left/Right or a
click turns the tabs). Tabs:
- **Open cases**: with more than one open, a divider tab per case down the folder's edge (Up/Down
  or a click picks one; Enter or "Follow this case on the radio" tracks it, and the tracked one
  wears a TRACKED stamp). Each sheet: case number, who filed it, where, the gang, what kind; a briefing (what the
  gang is known for, then the step's job); **the trail** (every step with its place, ticked when
  done, the ones to come blacked out); a progress checklist for the step (rooms cleared, the boss at large or
  bagged, each clue with what it turned out to be, each hostage held, under guard or freed); the
  boss's mugshot clipped on; one of Annie's notes in blue ink in the margin. With no case: how to open the police computer.
- **The Doctor**: Commander Starling's dossier. Chapters done are stamped SOLVED, the current one
  shows its goal (or the level it opens at), the rest are blacked out.
- **Closed**: the cabinet. A tally per gang, then the closed cases newest first (8 at a time;
  the wheel scrolls), each stamped CLOSED. It is kept in the profile on this PC (the newest
  500).

**Saving**: each open case is kept as its number (the crime's), gang and the step reached;
with the world's seed that makes the case again exactly (same seed, same crimes), so nothing
about places is saved. The opened and closed counts go along
(`Sheet.cases`, up to 16 open). The host's file wins: on starting, the server plans the host's
saved cases again and opens them at their step (in stage "go there"); a book from another world
keeps its counts but not its cases. Cases saved by earlier versions come back as the hideout crime of their number. Every
change is written back into every hero's file.

Not yet: villains riding lifts, parks and rooftops as places, more kinds of case (parked until hideouts are good).

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
direction ("at 3 o'clock") and con colour, on the **compass** strip at the top of the
screen, on the **map** (M), and on the **police visor** (`visor.rs`). Nothing glows over an
ordinary call in the world itself. Rampages, breakouts and the colossus also light the **Hero
Signal**: a searchlight from the Aether Spire throws a star onto the sky above the trouble,
visible from anywhere in town.

**The police visor** is a hologram the City Patrol issues to heroes. A new call switches it
on, and it stays on until the player switches it off with T (T again switches it on). The
hero touches two fingers to the right temple to switch it (`visor tap`, 1.1 s, standing or
walking; other clips skip the gesture), and it switches when the fingers get there (0.35 s
in). It materialises as a bright line at eye height that opens out, flickering, into the
visor (switching off takes 0.4 s, the same way back): a faint amber tint, a darker rim with
corner ticks, scan lines with a band rolling down, and the Metro City Patrol star,
see-through, low right of the line of sight. Each call gets corner brackets in its colour on
the spot (bigger when near, pulsing for 8 s while new) with its name and distance in con
colour, or an arrow at the visor's edge when it is out of sight. In third person the visor shows on the hero too: a faint amber band
of light over the eyes (about 20% opaque), from temple to temple, that opens from the eye line and closes into
it with the screen (an Engine custom material, `HOLO_VISOR`; hidden in first person). The
story's objective keeps its gold column.

A fight is resolved when its group is beaten; a rescue when everyone is out and the hurt one
is with the ambulance. Unanswered (no hero within 60 m) for 4 minutes, it ends: the crooks
get away, or the fire brigade comes. **A hero on the way** (closing in from within 1200 m)
keeps it waiting: while they get nearer it lasts at least their running time from there
(×1.5, +30 s), up to 5 minutes past its usual end, and the town's own heroes leave it to
them (they stop waiting 12 s after the hero stops closing in).
A fight's call stands only while its crooks do: with none of them left, or none (standing) within
80 m of it for 30 s, they got away and the call ends, so the visor never marks an empty street.
Crooks start on open ground in sight of their spot (the spot itself moves out of any wall):
where the usual line would put one inside a house, a wall or a closed door, they take the nearest
clear place within 12 m instead.

| Incident | Chance | Who / what | Level | Reach | Bonus |
| --- | --- | --- | --- | --- | --- |
| Mugging | 25% | 2 Chrome Jackets and a victim with their hands up, on a side street | team | 9 m | ×1.0 |
| Robbery in progress | 20% | 4 Syndicate gunsels at a shop front (outside the quarter) | +1 | 12 m | ×1.5 |
| Mutant rampage | 15% | a Mutant Alpha leading 3 brutes | +2 | 16 m | ×2.0 |
| Lab breakout | 12% | a Robo-Sentry leading 5 technicians at one of the Doctor's labs | +2 | 18 m | ×2.5 |
| Car crash, someone trapped | 14% | two smashed, smoking cars, 1 driver pinned by the legs (4 s) | – | – | 6 |
| Fire, people inside | parked (0%; HW_INCIDENT=fire forces it) | a house in flames, 3 people (3 s each) | – | – | 2.5 per person |
| The colossus is loose! | 8% | the walking tank (level 3+, every 25 min at most) | +3 | everyone | ×1.5 |
| Building collapse | follows a collapse | 3 people in the rubble (5 s each digging) | – | – | 3 per person |
| Flying saucer beaming people up | 7% (team level 2+) | a Zorrak Overlord with grabbers and zappers under their saucer, two townsfolk in its tractor beam; the beam goes off and the saucer zips away when it ends (`HW_VIEW=saucer` looks at it) | +2 | 20 m | ×3.0 |

"Bonus" is karma on top of the villains' own: for fights the group's worth again times that
factor, for rescues per person freed. The crooks of a crime mind their own business until a
hero steps within its reach; a mugging's victim (and townsfolk the villains held) first thanks the nearest hero within 30 m,
standing 3 s, then walks off once you deal with the muggers.

**Nobody came** (`make_off`): when a street crime's or a raid's call ends unanswered, its
crooks do not just vanish. The radio says "...: they're getting away!" and the call (on the
visor too) follows them. Muggers, robbers, mutants and technicians run off about 70 m, away
from the nearest hero (the path finding takes them round what is in the way; one still within
45 m of a hero when it gets there runs on); sky pirates climb back up their rope ladders and
the Iron Legion marches its two captives into the capsule. What they took is in their hands:
a mugger has the victim's handbag, two robbers swag sacks, a technician and two pirates a
crate. They are gone when they get there, or after 50 s. A hero who gets within 22 m of one
of them has **caught up**: they stand and fight, the captives stop, and beating the rest wins
the call as usual. If they all get away, the call ends unanswered.

**Rampages and breakouts wreck the street** while nobody stops them: every 7 s one of the
group who is not fighting walks up to a lamp, bin, bench or the like within 18 m of the call
and smashes it (a punch, or a technician's ray from up to 12 m); it falls over and is
repaired off camera later, like anything else broken. While a fight's crooks are at their
crime and not fighting, they menace it: brawlers jab a finger at their victims ("threaten"),
gunmen hold their guns up.

**Rescues:** stand within 3.5 m of the next trapped person and hold **E**; a bar under the
prompt fills while you hold it (4 s for a crash, 5 s digging out of rubble, 3 s at a fire, 60%
of that at a fire put out with a freeze ray). Let go and it drains again. Freed people turn to
their hero, thank them out loud (over their head and in the Local chat) with a hand on the heart
and a wave for about 3 s, then walk off out of the way. The last one out is hurt, and an **ambulance** drives up the street and parks
at a kerb about 50 m away: the hero who freed them carries them there in both arms (knees over one forearm, back over the other) (a red cross
on the compass, "Carry them to the ambulance, N m"); within 5 m the medic in white takes them
(double Guardian karma), the hurt one and the medic say a word of thanks, they are lifted in at the back and the ambulance drives off down the
street. A carrier who is knocked down drops them; anyone lifts them again by holding E for a
second (a bar shows it too). The trapped, the hurt and the medic are townsfolk like the people
on the street. At a crash or in rubble the trapped sit pinned by the legs, tugging to get free and
wincing; at a fire they wave from the door and windows. A building that heroes or the colossus bring down
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
- **Seen from afar** (`hw-client/src/skyline.rs`, 0.194): the town's cells of full buildings
  reach 1.3 km round the camera; past them, out to the 6 km view distance, every building
  stands as its plain mass (the plan's storeys and setbacks, pitched and mansard roofs,
  stepped crowns and spires, a little inside the real walls) with its streets, in 768 m tiles
  made on two worker threads, nearest first (`TownWorld::skyline_with_roads` plans them
  without filling the town's caches). Windows are a shader grid, lit after dark by the
  same household habits as the real ones. A tile draws everything to 2.6 km and only
  buildings of 12 m and more and main roads beyond (dithered cross-fade, Engine LOD set),
  fading out at 6 km; it hides once all its cells are built. `HW_SKYLINE=0` leaves it out.
  Uses: offices, shops and offices, apartments, houses, works, pavilions, labs, tenements.
  Every building has a furnished interior (lobbies, offices, flats, shops, labs) and real
  stairs and timetable lifts (nothing floats you up a shaft; the few old-style narrow
  apartment houses without halls have sealed upper floors, like a family house). Interiors follow the
  building's era: wallpaper (stripes, damask, atomic) and picture rails in old and deco
  houses, painted plaster in modern ones; parquet in flats, lino in kitchens and diners,
  terrazzo in lobbies. Flats have a kitchen, dining table, sofa group,
  bed and wardrobe; offices rows of steel desks; shops are a grocer, a clothes shop, an
  appliance store or a diner (narrow commercial buildings keep a 4 m entrance hall at one
  end and a shop in the rest). After dark the lit rooms light up (lamps on the same habits
  as the lit windows). **People indoors:** at home mornings and evenings (abed from 23:30
  to 6), at work 8-18, shopping while the shops are busy; each stays about a quarter hour. Buildings on slopes stand
  on stone plinths. Surfaces are textured and weathered by age and quarter.
- **Doors** (`hw-gen/src/doors.rs`, `hw-sim/src/doors.rs`, `hw-client/src/doorview.rs`):
  every doorway up to 2 m wide has a real door, shut until someone opens it, and a shut
  door is solid (heroes, villains, thrown crates and lines of sight stop at it). Street
  doors are a pair of painted leaves (panelled with a brass knob and fanlight on older
  houses; streamline with a chrome-ringed porthole, speed lines and a bar handle on newer
  ones); shop and lobby entrances get a pair of chrome-framed glass doors with push bars
  in the middle of fixed glass; doors between rooms are panelled wood or flush with a
  porthole, like the rest of the furniture. Leaves swing 90° into the room behind (0.7 s).
  **E** within 2.6 m of a doorway opens or shuts it (the server checks the distance and
  never shuts a door on someone). People on foot (villains, sidekicks, the town's heroes)
  open a shut door they walk up to (within 1.4 m, heading at it) and it shuts 2.5 s after
  nobody is within 2.2 m. Wider openings (hall wings, the lift core) stay open. Only open
  doors are remembered (in collision, the save, and for players who join). Behind an open
  street door of a building you are not in, a small lit hall shows. View:
  `HW_VIEW=door HW_DOOR=front|glass|inner [HW_DOOR_OPEN=1] [HW_DOOR_SKIP=n]`.
- **Water:** rivers, lakes and coasts, all drawn by Engine's water (a simulated window around the hero, 1.5 km wide in 4 m cells, with waves, reflections, foam and caustics, and a flat far plane at `WATER_LEVEL` to the horizon, since 0.254; the clear lake look inland, the ocean look on coasts); bridges; granite quays with bollards and steps down
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
- **Graveyards** (`hw-gen/src/urban/graveyard.rs`, models `hw-client/src/gravemesh.rs`):
  every town has at least one, `1 + sqrt(people / 25 000)` rounded (at most 12): one for a
  hamlet, 2 at 20 000, 3 at 100 000, 7 at a million. The first is the **old burial ground**
  in the old town (up to 1 500 m² + 0.6 m² per person, at most 4 500 m²): crowded, irregular
  rows, old ringed crosses and arched slabs. Then, every third a **garden of rest** in the
  newest quarters (12 000-32 000 m²: rocket, atom, boomerang and ringed-planet markers on open
  lawn, saucer tombs) and the rest walled **cemeteries** in the 19th-century ring, out where
  its edge was (14 000-48 000 m²). Never in the villains' quarter; spread over the town. Side
  streets are traced round a cemetery's ground, so it gets a block of its size; no building
  stands on it. Each has an iron railing on a stone base, a gate on its longest street side
  (stone pylons with glowing globes, an iron arch with a ring, a star and a crescent moon, the
  leaves open), an avenue of cypresses and lamps from the gate to a rocket monument, cross
  paths every 26-30 m with benches and a mausoleum (a chapel under a parabolic roof, or a
  saucer tomb) at each, and rows of graves facing the gate, each stone with a kerbed plot of
  ivy and flowers in front, some with a lantern; 8 % of plots stand empty, 3 % grow a tree.
  Stones: arched slab, rocket on fins, atom, Googie boomerang, ringed planet, star obelisk,
  ringed cross, and a stone with a little sheet ghost peeking over it (eyes glowing). Graves
  are solid and topple when hit (toughness 160, 4 000 collateral); rails smash (140);
  tombs and the gate do not break. Views: `HW_VIEW=graveyard` (`HW_GRAVE_N`, `HW_GRAVE_BACK`,
  `HW_GRAVE_UP`), `HW_VIEW=prop HW_PROP=Grave|Tomb|GraveGate|GraveRail`; where they lie:
  `cargo run --release -p hw-gen --example graveyards -- <seed> <people>`.
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
away), then its round: up to cruising height (its nose 26 m over the tallest roof, crown
or spire the town could have, over every landmark, 90 m over any hill), over the core, out
beyond the town and round wide, at 22 m/s,
turning at most 0.075 rad/s; then a glide in along the mast's axis from 700 m out and
70 m up, slowing evenly to a stop. Where it is lower than that (climbing away and sinking
back), it looks at what is really under its hull and rises over it, no steeper than 9 %.
A cycle is 12-17 minutes; a session opens with it moored,
25 s before it casts off. The liner is solid where the timetable has it: its envelope, the
passenger deck under it, the fins and the engine pods become boxes (at most 4.6 m on a side,
`hw_sim::airships`) that move with it each tick, for heroes within 30 m of the hull. Heroes
bump into it, can land on its back and cannot fly through it; it does not carry a hero
standing on it. The pirates' *Cloudcutter* moored over the harbour is solid the same way
(envelope, gondola, engines, fins), and so is a raiders' airship moored over a street, and a
Zorrak saucer hanging over its raid (a grid of boxes over its disc and dome).

What an airship clears (`hw_gen::urban::airspace`): every building whose outline (with
2.5 m for eaves, cornices and signs) meets the sky over it, up to its drawn top
(`BuildingSite::top`: walls, pitched roof and chimneys, dome, tower crown with its spire
or radio mast), and every landmark. The pirates' moored ship keeps its gondola 14 m over
that (else a quay spot where it does); a raider turns along the street to where it is
lowest and hangs 4 m clear of it, and when it leaves it rises straight up (3 m/s) over
everything on its way out before it sails off. `tests/airships_clear_roofs.rs` checks the
liner's whole round, the moored ship and raiders in towns of every size.

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
  the ceiling. Entered through **spiral stairs** in Googie pavilions on the pavements (look
  for the glowing atom signs): a 1 m wide ring of terrazzo steps round a pillar, 16 steps and
  3.2 m of depth to a turn, a rail round the hole on three sides, the way in on the fourth.
  You walk down (and back up); the foot opens into the passage to the tunnel. Lawns, grass
  and pavements leave the hole and its paved apron (2.4 m round it) open
  (`TownWorld::street_holes`). Steps have chrome nosing and a smooth underside; a solid
  chrome balustrade (0.95 m) runs on the open side from a rail's height below the street
  down to a metre above the floor, a brass handrail on the pillar, and an Aether strip
  2.2 m over the steps lights the way (one lamp a turn and two at the foot). The same stair
  serves every deep shaft. Views: `HW_VIEW=lift`, `stairtop`, `stairs`, `stairfoot`.
- **Molehaven**, the mole people's town in a cavern 27 m down under the mole drill
  (`hw-gen/src/urban/molehaven.rs`, drawn by `hw-client/src/molehavenview.rs`). Two ways in,
  both walked: **Molehaven's gate**, beside the drill's plinth (stone piers with brass bands
  and caged lanterns, a riveted iron arch studded with bulbs, the mole emblem, a mole's face
  in goggles, on its crown, a beacon on top) over a spiral stair down into the great hall;
  and a **ramp** down from the utility tunnels, under an amber sign hung from the tunnel's
  roof (the emblem and a lit arrow pointing down the ramp). Up to 8 **signposts** with the
  emblem and a brass arrow stand at the back of the pavements within 280 m of the gate,
  pointing at it; the town map and the minimap mark the gate with the mole's face (on the
  minimap's rim when it is off the map), and the taxi stand "Molehaven gate (mole drill)"
  is in front of it.
  The great hall (56 x 40 m, 16 m high): up to ten mushroom houses (limewashed stems,
  spotted caps, arched plank doors with a step and a lamp, round lit windows in brass frames,
  window boxes, stovepipes), a market of five striped stalls with goods round a glowing well,
  benches, lantern posts, barrels, carts and pots between the houses, washing lines, rugs at
  the doors, toadstool lamps, crystals and stalagmites along the walls, two rock columns,
  stalactites, strings of bulbs from the drill's **bore**, which comes down through the roof
  over the well (iron-lined, a riveted collar, struts into the rock, the bit's hot tip) and
  pours a glowing stream into it. Each side chamber has a job: a **workshop** (benches, a
  pump, carts), a **mushroom farm** (rows of glowing pots, giant mushrooms), a **shrine**
  (a carved mole idol, candles, a glow pool) and **cottages** round a yard. Everything that
  stands down there is solid (`MolehavenLayout::footprints`) and keeps clear of the ways in.
  Short, pale mole folk in goggles keep the stalls, run errands, stroll round the well and
  chat in the side chambers. The first side chamber has the **stair down to the Deeps**, a
  railed hole in its floor. Views: `HW_VIEW=molehaven`, `market`, `moleroom`, `molegate`,
  `molesign`, `moleramp`.
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
  blast (700 m). Taxis go there ("Molehaven gate (mole drill)"). Views: `HW_VIEW=drill`, `drillbore`;
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
- **Along the tube** (`hw-gen/src/urban/deep/nooks.rs`, drawn by `hw-client/src/tubenooks.rs`):
  every 75 m of line is a slot for a nook (not within 100 m of a station, 55 m of another
  line or near the lab). Every third slot tries for a **way up**: a short passage to a lift
  under a little green pavilion on the pavement, a lit green sign (arrow up, lift) over the
  mouth. Where the street has no room, the next slots try in turn, so in a 40 000 town a
  way up (exit or station) comes every 300 m or so along a line. The other slots hold, by
  how much the mole folk have made that stretch theirs: a row of **mole homes** (round
  doors in a stone front, lit round windows, lanterns, washing, barrels, a rug), a
  **market cave** the walkway runs straight through (striped stalls both sides, strings of
  bulbs), a **workers' shop** (tiled room, bench, pump with gauges, a mine cart), a
  **mushroom shrine** round a carved mole idol, a **burrow** dug through the tube's wall to
  a den round a fire, or nothing. Mole folk (and a few townsfolk working or shopping) stand
  at their doors and stalls, chat, and walk to the walkway and back (`Citizens::tube_folk`,
  up to 48 within 90 m while the camera is below the ground). By their homes the sound is
  Molehaven's; elsewhere in the tube, the tunnels'.
- **Tube sections:** every 260 m of line has its own paint (six schemes: cream and red,
  mint and chrome, sky blue with tiles, salmon and teal, black-and-white tiles, butter and
  maroon), lamp colour and lamp spacing (`TubeNet::livery`). About a third of the sections
  are the mole folk's: amber lamps, painted glyphs (spirals, claws, mushrooms, suns) and
  rusty patches on the shell, and their nooks.
- **Finding the way up:** while you are in the tube, its stations or nooks, a line above
  the power tray says where the nearest way up is ("Way up to the street: 120 m ahead").
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
vats, coils, tents, crystals) has a footprint the hero bumps into. The ways between layers
are the same spiral stairs (`hw_gen::urban::spiral::Spiral`: pillar and steps are solid
footprints; the deepest take a minute to walk). The factions place people at
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

### The town's animals (`hw-sim/src/citizens/critters.rs`, `hw-client/src/critterview.rs`)

This is another planet, and its towns have their own animals. Like the people they are a
pure function of seed and time, so every player sees the same ones, and they stream in round
the camera: wibbets and drakelets within 150 m, driftbells within 420 m, at most 90 at once.

- **Wibbets**: big-eyed climbers the size of a cat, not quite monkeys, with round ears, a
  ringed tail and two feelers with glowing tips; teal, coral or lilac. They live in troops of
  two to four, only where two or more things stand to climb along a stretch of pavement (about
  one attempt per 45 m of pavement side, in the bright town only, pavement at least 1.3 m
  wide, a troop's stretch 16 m either way). They are restless: a round is 24 s, of which a
  wibbet trots for a few seconds (2.6 to 3.6 m/s), then it runs up or hops onto its next
  place and stays. Nine rounds in ten end up something: **clinging to a lamp post, an
  advertising pillar or the bare trunk of a street tree** (plane, linden, maple, chestnut,
  ginkgo, birch; under the crown, 1.2 to 2.2 m up), or sitting on a bench, bin or hydrant. One
  round in ten ends on the pavement itself, sitting up or standing. In the test town about
  70% of their time is spent up something, 18% under way and 11% on the pavement. A wibbet
  comes down a post head first. Places come from the props' own shapes (`Prop::perch`), so a
  wibbet sits on what is drawn, and its way keeps clear of every prop.
- **They come close.** A wibbet on the pavement (standing or trotting) within 7.5 m of a hero who stands
  about (slower than 1.6 m/s) trots over, stops 1.25 m away and sits up to look at them for
  up to 9 s, then goes back to its round (it waits 25 s before it comes again). It leaves at
  once if the hero walks off or strides at it, and never comes while there is trouble near.
  One a hero walks into hops out of the way. This happens on each player's screen only.
- **Drakelets**: pet dragons the size of a small dog, with a collar and a brass tag; ember,
  jade or azure. 1.4% of the people on foot have one. It flies on a 2.1 m lead, about 2 m up,
  ahead of its owner and from side to side, and circles them when they stand; it keeps clear
  of lamps, trees and signs. When its owner stops to stare or is knocked down, it stays with
  them.
- **Driftbells**: glowing bells with trailing arms, 1 to 1.6 m across, that float 14 to 19 m
  over the wide streets (carriageway of 6.8 m or more, at least 140 m long, no bridge near)
  in blooms of one to three, slowly up and down the street. Rose, aqua or amber.

The models are AssetGenerator sculpts with their own skeletons and clips
(`tools/creatures/make_specs.py`, built by `tools/creatures/build.py` into
`assets/creatures/`), about 14 000 to 18 000 triangles close up, with two lighter levels.
Animals cannot be targeted, hit or harmed.

### Street acts and night life (`hw-sim/src/streetlife.rs`, `hw-client/src/streetview.rs`)

Odd folk and street performers turn up at pitches round town: open ground just off a pavement
(never in a walker's way, at least 1.1 m clear of the walking line and of every prop, and
2.2 m clear of doors), and the plazas round fountains, statues, the Aether Spire and Hero Hall.
Pitches are planned per 96 m patch as the camera comes near; at most 160 patches and 96 acts
are kept, the oldest dropped. Like the townsfolk they are a pure function of seed and time.

Each pitch gets one act per two game hours (300 s of play), or none: a pitch in a busy
shopping street or the old town is used far more often than one in a quiet back street; the
villain quarter has none. An act comes, walks in along the pavement with its things, sets up
(5 s), plays, packs up (5 s) and walks away; nobody stands about with nothing to do.

| Act | Hours | What happens |
| --- | --- | --- |
| Soapbox prophet | 9-20 | on a soapbox in a sandwich board (a flying saucer on it): the saucers are coming |
| Theremin player | 10-23 | in a dinner jacket, playing an eerie tune (heard up to 45 m) |
| Living statue | 10-18:30 | a chrome spaceman on a silver crate, still as stone; come within 2.6 m and it jumps (BOO!) |
| Atom juggler | 11-19 | juggles three glowing atoms |
| Saucer spotter | 8-23:30 | foil cap, binoculars on the sky; whoever stops looks up too |
| Newsboy | 6:30-11, 16-19:30 | cries the headlines by a bundle of papers; people buy one |
| Stargazer | 20-02:30 | a brass telescope; people queue to peer at the stars |
| Swing dancers | 17-00:30 | a couple dancing to a portable radio |
| Vael star trader | 10-19:30 | a friendly alien from round another sun (lilac skin, gold eyes, a teal coat) glides in on a hover disc and sells glowing trinkets from a floating tray: comet glass, ringed globes, toy rockets, a star in a jar; people come up and buy |
| The Orbit Room | 20:30-03 | a night club in a big shop building of the business or civic district: neon atom sign, a doorman, a queue of guests in evening clothes that moves up and goes in, the band muffled through the door. One big shop building in five there is a club (`streetlife::is_club`, by its own seed); its biggest shop room with a front door is the club inside (bar with stools, a mirrored back bar of bottles, velvet booths, a bandstand), and that is the door with the sign. Through the night one doorman hands over to the next at each change of slot |

Up to 2-6 passers-by stop to watch (one or two at a time for the spotter and the newsboy).
They watch, clap now and then, and those of a busker drop a coin in the case before they go.
Within 14 m a hero hears the act's patter; within 6 m the act speaks to the hero; now and then
a watcher cheers. Lines show over heads and in the Local chat. Trouble close by (a fight,
mugging, robbery, rampage, collapse or explosion within its alarm's reach plus 10 m) breaks the
act up: they shout, run off along the pavement, and their things are left standing.

The acts within 110 m of the camera are found on a worker thread twice a second; the nearest
8 are shown, their people within 70 m. Performers and guests are Actors characters with their
own clips (preach, theremin, statue, startle, juggle, binoculars, holler, peer, point up, carry
case; watchers clap and look up). Near the theremin, the radio or the club the scenery's music
keeps quiet.

### Street crime (`hw-sim/src/streetlife/crime.rs`, `hw-sim/src/crimes.rs`, `hw-client/src/crimeview.rs`)

Besides the radio's calls, the gangs commit small crimes in plain view round the town, as a
pure function of seed and game time like the acts (the server and every client work out the
same ones). Places are planned per 96 m patch (at most 160 patches and 512 crimes kept): shop
doors, blank walls, points along the pavements every 26-46 m, and lamps, bins, mailboxes,
hydrants and phone booths. Each place gets at most one crime per game hour (150 s of play),
far more often the rougher the street: none near Hero Hall's plaza or inside the villain
quarter, most along its frayed edge (fading out over 320 m), a little in the old town, rarely
elsewhere. Crooks walk in along the pavement, do it, and walk or run off; the victim was
walking by and goes on afterwards.

| Crime | Hours | Who | What happens |
| --- | --- | --- | --- |
| Bag snatch | 8-23 | 1 Chrome Jacket | follows a lady, grabs her handbag and sprints off with it; she shouts "Stop, thief!" |
| Hassling | 11-02 (roughish streets) | 2 Chrome Jackets | stop a passer-by, threaten and shove them, laugh as they hurry off |
| Shakedown | 8:30-19 | 2 Chrome Jackets | at a shop door: the owner comes out, is threatened, hands over cash; they count it |
| Protection money | 19-04 | 2 Syndicate gunsels | the same, quieter, for an envelope ("Mr. Velvet sends his regards") |
| Gang tagging | 16-05 (rough streets) | 2 Chrome Jackets | one sprays the gang's sign on a wall while the other keeps a lookout; it stays 8 game hours |
| Vandals | 17-05:30 (roughest streets) | 2 mutant brutes | smash a lamp, bin or phone booth (it really breaks, if a hero is within 220 m) |

People nearby stop and stare while it goes on. A hero within 16 m hears the crooks and the
victim. **Stepping in:** a hero who comes within 7 m of anyone in a crime going on (on the
same level) turns it into a fight: its crooks become real villains where they were drawn, in
the same looks, and go for the hero; whatever they were holding stays in their hands (a thief
keeps the bag); the victim cowers and thanks the hero when it is over. Beating them stops it: karma as for
beating them, plus 60% (to every hero within 120 m). If no hero is within 160 m for 20 s, they
leave. A sign half sprayed when a hero stepped in stays as far as it got.

Which crimes are near the camera is found on a worker thread twice a second (the nearest 8
within 110 m, their people within 80 m); crooks and victims are Actors characters with their
own clips (threaten, shove, grab, laugh, count, spray, smash, lookout, lounge; shout, shaken),
and what they hold (handbag, cash, envelope, spray can; swag sacks and crates for raiders
making off) is drawn in the hand.

### Traffic (`hw-sim/src/traffic.rs`)

- **Cars** (finned two-tone cruisers, bubble cars, ground-hugging hover cars) drive lane
  loops through the street network, keeping right, busier downtown and on main roads.
  Junctions have **traffic lights** (36 s cycle with an 8 s all-walk phase), give way inside
  a phase, never enter a junction they cannot leave, follow the car ahead and queue. A car
  stuck a minute turns off and comes back where there is room. Cars stop for heroes, for
  people standing or lying in the road, and short of a fight or collapse.
- **Buses** (`traffic.rs`, model `hw-client/src/carmesh/bus.rs`): every lane loop that
  passes a bus stop runs buses, one per 1600 m of loop (at least one, at most a third of its
  cars). A bus is 11.2 m long, keeps to 10.5 m/s, pulls in beside each shelter on its side of
  the road (stops within 10 m of the lane, not inside a junction), stands 16 s (`DWELL`) with
  its doors open and drives on; cars queue behind it. Four two-tone liveries; streamline body
  with a wraparound windscreen, lit route sign, quad headlamps, an atom on the nose, tail fins
  and a rocket on the roof. Two to fifteen passengers sit inside; on wrecks they leave by the
  nearest kerb-side door.
- **At the stops** (`hw-client/src/busview.rs`): within 90 m of the camera up to five
  townsfolk wait under each shelter, more stroll up every 35 s or so. When a bus calls, up to
  three get off at the middle door and walk off along the pavement; those waiting walk to the
  front door and get on. Only a show on each PC: the server knows the buses, not who waits.
- **Sky lanes:** hover cars and flying taxis cruise over the main roads at 34, 46 and 58 m,
  banking into turns.
- Cars are placed every drawn frame from the game clock (`Session::time`: every server tick
  sent plus the part of a tick the frame is into, `hostthread::TickClock`), between the
  traffic's last two 0.1 s steps, so they glide at any frame rate.
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
- **Herobots** (`hw-sim/src/herobots.rs`, `herobot.rs`): two robots stand on Hero Hall's plaza
  beside the five town heroes, there so one person can test multiplayer alone. Press **H**
  within 8 m of one to wake it, **H** near it again to send it home (it flies or walks back
  to its spot, then leaves). A woken Herobot is a **real player**: its own thread joins the
  host over UDP (single player opens a loopback-only socket the first time, nothing is
  reachable from the network), sends a hero file and then only sees what a player sees (the
  entities near it, the wrist radio's incidents, its own hero file). It takes one of the
  10 places (a full team refuses it), counts for group sizes and incidents like any friend,
  earns karma and levels, and when it is beaten it calls the medical drone after 20 s and is
  flown to the hospital like anyone. Team level stays the host's, as for any friend.
  - Its hero file: all the lifetime karma of the hero who woke it (as if nothing had been
    spent), a random origin and element, and every bit of karma spent at once on random
    powers it can unlock (attacks first). Later karma is spent as it comes; augments it finds
    are set into the first socket they fit, or traded in.
  - What it does: follows the nearest person (not another Herobot): closes up beyond 10 m to about 6 m, and is
    on a 35 m leash (it returns first, whatever it is doing). It only goes for incidents and
    villains within that leash of the person, and heads for incidents the radio
    announces, fights villains that fight or stand at one (rotating through its attack
    powers, a sidekick if it has one, a heal if someone is hurt, defensive toggles on in a
    fight and off after a quiet while), helps a downed hero up (hold revive), walks round
    walls by `nav::find_path`, flies when it owns Flight. No taxis, no missions.
  - Its sheet lives only in memory: nothing of it goes into your hero file or the world save.
  - Everyone sees it as a robot (`HEROBOT_MOUNT` on its hero's `mount`; Herobot 1 chrome with
    red trim, Herobot 2 copper with blue). `HW_VIEW=herobots` looks at them from the plaza.
  - **Teammates on the maps and the visor** (all other heroes, Herobots too): a named dot on
    the minimap (clamped to its rim when far) and on the town map, and in the patrol visor a
    big named bracket with the distance (an edge arrow when out of sight). People blue,
    Herobots teal. The host tells each joined player every other hero's position once a second
    (`ToClient::Mates`), so they show beyond the 500 m of the world updates; `Session::team`.
  - **Friendly target** (`team.rs`, `InputState.ally`): click a teammate in the world, click their row in
    the team frame (left, names and health, DOWN when down), or press F1-F4 (heroes in name order). Ally
    powers (heal, shield, revive: `Delivery::Allies`) then also reach that hero from up to
    `ALLY_REACH` (40 m) away, beyond the power's own radius; without a pick they reach friends inside the
    radius as before. Tab stays villains-only. Herobots pick the most hurt hero within 35 m each tick.
  - Tests: `hw-sim/tests/herobots.rs` (wake, ten places, home; daily: fights, earns karma,
    the drone flies it to the hospital).
- Multiplayer has been tested by bots (`tests/soak.rs`: ten bots for an hour) but not yet
  by people (postponed until Arnd has someone to play with).

---

## 12. Screens, sound and settings

- **HUD:** health and energy bars and the power tray (bottom centre), target frame,
  wrist radio (top left), compass (top), version and "What's new" (top right), minimap
  (bottom right: round, turns with the camera so what you look at is up, zooms out as you
  fly higher; marks you, friends, landmarks and radio calls; `minimap.rs`, `mapraster.rs`).
- **Windows:** powers (B: karma, level, shop, give up, faction standings), map (M), menu
  (the menu button, top right: settings, controls, quit).
- **Town map** (M, or Escape to close; `townmap.rs`): fills the screen, north up, with the
  minimap's detail from the same tiles: every house when close (closer than about 1.6 m a
  pixel), blocks and streets further out (up to about 4 m a pixel), main roads beyond; parks,
  water and the villains' quarter tinted. Landmarks with names, radio calls, the story's
  star, friends and you. Drag to move, wheel to zoom round the pointer (0.35 m a point up to
  the whole town), "Back to me" re-centres; a scale bar bottom left. Only tiles in view are
  made, on two worker threads, nearest first; at most 320 kept. `HW_MAP_MPP` sets the
  zoom it opens at, `HW_VIEW=map` opens it.
- **Chat window** (bottom left, beside the power tray or above it on narrow screens;
  drag its corner to resize; `hw-client/src/chat.rs`): one log with a tab per channel, as in
  EverQuest and City of Heroes. Two channels: **Team** (0.276) and **Local**, what is said
  near the hero. Team is every player of the session, however far apart: click the tab, press
  Enter and type (the line goes to the tab on show). On the wire it is `ToServer::TeamChat` and
  `ToClient::TeamChat` (reliable); the host forwards it to every remote player and to the
  person at the host (`Host::team_chat`, `HostEvent::TeamChat`); a bot says it with
  `Connection::team_chat(text)`. There is no team roster yet: the whole co-op session is the team. Gang warnings carry 70 m (`chat::SHOUT_RANGE`), other speech 35 m (`SAY_RANGE`):
  mole folk thanking you, Hero Hall's heroes giving tips, Commander Starling on the wrist
  radio (always heard), and what players type (Enter or `/` opens the line, Enter says it,
  Escape cancels; your words also show over your hero). The same speaker repeating the same
  words within 6 s is logged once; the log keeps the last 400 lines. Planned channels: police
  radio, combat log (each is one more `chat::Channel`).
- **Team voice** (0.284, `hw-client/src/voice.rs`, `hw-net/src/voice.rs`): push-to-talk. Hold
  Z (the key is chosen in the Voice panel: Z, X, C, H, U or L; not while typing in chat). The
  microphone opens only while someone else is in the session and voice is on. The sound is cut
  into 20 ms Opus frames (48 kHz mono, 24 kbit/s with a little loss protection, about 3 KB/s
  while talking, nothing while the key is up, a 160 ms tail after release) on a worker thread
  and sent as unreliable `ClientMsg::Voice` datagrams of up to 3 frames over the same UDP link
  (or the GameHub relay). The host never decodes: it checks the sizes, allows 100 frames/s per
  player (`voice::MAX_FRAMES_PER_SECOND`), and forwards the packet as `ServerMsg::Voice` to
  every other player and to the person at the host. A listener decodes on a worker thread
  (a lost packet is concealed, a late or repeated one dropped), keeps a 60 ms prefill and at
  most 400 ms per speaker, and the output stream mixes everyone. Both ends talk to the network
  only through a `VoiceBox` (`Session::voice_box()`), so the audio never waits for a tick and a
  bot can send ready Opus frames with `voicebox.send(frames)`. The Voice button in the chat
  window's tab row opens the panel: voice on or off, the key, a team volume, a microphone
  meter, and per player a mute box and a volume slider (kept by name in the profile). Who is
  talking is listed at the left edge. A missing microphone or speakers show as a line in the
  panel, never as a crash. Opus is libopus built by `opusic-sys` with cmake (Windows users
  need CMake; `start.bat` installs it with winget when it is missing).
- **Speech bubbles** (`hw-client/src/speech.rs`, 0.179): what people say shows in a comic
  balloon over their head, tail pointing at them, as far as it is heard (35 m, shouts 70 m).
  Lines wrap at about 230 pt and the balloon grows to hold them; it stays up 2.4 s plus
  0.06 s a letter (3.5 to 9 s). Shouts (anything that carries `SHOUT_RANGE`, such as gang
  warnings) are spiky yellow bursts in red capitals. The nearest 8 show; nearer ones take the
  spot over the head and the others step up or aside. A speaker off screen or behind the
  camera gets a smaller balloon at the screen's edge on their side. A new line from the same
  speaker replaces their balloon. Code: `fight.speech.add(Speech::say(feet, text, voice))`,
  `Speech::shout(..)`, `.from(net_id)` to follow a moving speaker, or `fight.bubble(&said)`
  for a chat line. `HW_SPEECH=1` puts a few round the hero to look at.
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

**The debug menu: Ctrl+Shift+F12** (single player). Five tabs:

- **Go to**: open incidents on the radio, the town (start, taxi stands, Aether Spire, lighthouse,
  park fountain, lobby of the tallest tower, a half-timbered house, high above the centre), the
  villains (the quarter below the Tesla tower, a rooftop lookout, a villain corner, the Chrome
  Jackets' hangout, the barricade, the graffiti wall, a lab) and underground (a lift, a utility
  tunnel, Molehaven's hall and market).
- **Hero**: set the level, add karma, buy the whole tree (the first of each fork) or give them all back, switch the element,
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
- **Log** (`hw-client/src/devlog.rs`): what became of every key press, from the keyboard to the
  server and back: each key the game got (and any the window got that the game did not), each
  power pressed with its target, distance and line of sight on screen, when it went to the
  server, what the server did with it (fired, waits and why, lapsed, dropped), and what the
  server reported back. "Copy all" puts it on the clipboard; every line is also in
  `logs/debug.log` (a new file each start).

`HW_DEBUG=<tab>` (`places`, `hero`, `events`, `world`) opens it at start. The environment
variables below start the game in a state worth looking at. On Windows: `set HW_LEVEL=10` then
run, or put them in a `.bat`.

### Hero and progress

| Variable | Effect |
| --- | --- |
| `HW_LEVEL=n` | Start at level n with at least 20 000 karma to spend (not saved). |
| `HW_POWERS=1` | With `HW_LEVEL`: buy the whole tree (the first of each fork). |
| `HW_ORIGIN=n` | With `HW_LEVEL`: play origin n (0-4), karma given back. |
| `HW_ELEMENT=n` | With `HW_LEVEL`: element n (1 Fire, 2 Frost, 3 Energy, 4 Necrotic, 5 Lightning, 6 Psychic). |
| `HW_AUGMENTS=1` | With `HW_LEVEL`: a bag of augments and a few set (the debug menu has "A bag of augments" too). |
| `HW_CARRY=1` | Every villain carries an augment (to look at the gems and the drops). |
| `HW_FLY=1` | Start flying. |
| `HW_STORY=n` | Start with n chapters of the Doctor's story done (not saved); `HW_BRIEFED=1` also briefed on the next. `HW_VIEW=statue` looks at the statue (`HW_STORY=7`). Where each chapter plays: `cargo run --release -p hw-sim --example story_places -- <seed> <people>`. |
| `HW_NAME=...` | Hero name for this run. |
| `HW_PROFILE_DIR=...` | Use another profile folder (a fresh hero). |

### Incidents and fights

| Variable | Effect |
| --- | --- |
| `HW_INCIDENT=1` | The first incident starts at once. |
| `HW_INCIDENT=mugging\|robbery\|rampage\|breakout\|crash\|fire\|colossus\|moles\|pirates\|iron\|saucer` | ...and of that kind. |
| `HW_TIMEOUT=s` | Calls end unanswered after s seconds (and their crooks make off). |
| `HW_CRIME=snatch\|hassle\|shakedown\|protection\|tagging\|vandals` | Every place of that kind has that street crime, every hour. |
| `HW_VIEW=crime` | With `HW_CRIME`: the nearest such crime, the street clock jumped into it (`HW_CRIME_AT`: 0 crooks arrive, 1 deed done, 2 leaving; default 0.6). |
| `HW_VIEW=getaway` | With `HW_INCIDENT` and `HW_TIMEOUT`: follows the crooks making off. |
| `HW_VISOR=1` | The police visor stays down; `HW_VISOR=0.4` freezes it part way in (screenshots). |
| `HW_POLICE=s` | s seconds in, a Sky Patrol cruiser is called to where the hero stands and sets down at once (screenshots; frame 360 has the officers out); `HW_VIEW=police` looks at it (`HW_POLICE_CAM="side,up,ahead"`, `HW_POLICE_AT=officer` for the first officer). |
| `HW_CARRY=1` | With `HW_INCIDENT=fire\|crash`: start with the hurt one in your arms. |
| `HW_CASE=<job>[@<place>]` | Cases open at once, the hero at the first (a little way off in the open; at the way down underground); with `HW_VIEW=hero`. Jobs `takeover`, `evidence`, `hostages`, `crew`; places `building` (default, one to a building), `street`, `plaza`, `tunnel`, `station`, `grotto`, `ruin`, `lair`. Comma-separated for several (`hostages,crew@street`); `chain` is a whole case as Annie plans it. |
| `HW_CASEFILE=case\|doctor\|closed` | The case file open on that tab (screenshots). |
| `HW_COLOSSUS_BREAK=n` | Break the colossus's first n weak points (2 kneels it). |
| `HW_COLOSSUS_RAMPAGE=1` | The colossus smashes its way through town even with no hero near (screenshots). |
| `HW_VIEW=fight` | Places a test group of minions near the spawn (the game starts without one). |
| `HW_AUTOFIGHT=1` | The hero fights on its own (for screenshots); `HW_AUTOFIGHT=17,20` uses only those powers (power numbers, e.g. 16 Cryo Ray, 18 Tin Pal, 23 Hypno Spiral). `HW_CAM_DIST`, `HW_CAM_PITCH` and `HW_CAM_YAW` (degrees round the hero) set the hero camera for close-ups. |
| `HW_WRECK=n` | n frames in, the cars near the camera are wrecked. |

### World and camera

| Variable | Effect |
| --- | --- |
| `HW_SEED=n` | World seed (which town). |
| `HW_POP=n` | Town population (default: the profile's city, 40 000 at first; 3 000+ gets a villain quarter). |
| `HW_HOUR=h` | Hour of day, 0–24 (22 for night). |
| `HW_SKYLINE=0` | No distant skyline past the built cells (to compare). |
| `HW_VIEW=...` | Start the camera at a sight: `street`, `sky`, `inside`, `cutaway`, `river`, `quay`, `park`, `lawn`, `props`, `pad`, `timber`, `plinth`, `spire`, `lighthouse`, `tesla`, `tesladome`, `teslayard`, `menace`, `drill`, `drillbore`, `mast`, `mastapron`, `terminal`, `liner`, `rocket`, `rocketpad`, `rocketarm`, `rocketboard`, `atomplant`, `atomgate`, `atomtower`, `works`, `worksline`, `worksdock`, `workshall`, `hall`, `townheroes`, `villain` (with `HW_VILLAIN=delver|buccaneer|gunner|captain|legionary|praetorian`), `airship`, `quarter`, `edge`, `graffiti`, `rooftop`, `hangout`, `traffic`, `skylanes`, `signals`, `wrecked`, `riders` (beside the nearest car's driver; `HW_RIDER=cruiser|bubble|hover|taxi`), `lift`, `tunnel`, `molehaven`, `far`, `market`, `tube`, `station`, `tubeline`, `kiosk`, `lab`, `ruin`, `grotto`, `homes`, `tubemarket`, `workshop`, `shrine`, `burrow`, `exit` (a tube exit from inside), `exitstreet` (its pavilion), `incident`, `mugging`, `robbery`, `rampage`, `breakout`, `fire`, `crash`, `medic`, `signal`, `map`, `colossus`, `colossus_close`, `sentry`, `alpha`, `citizen`, `wibbet`, `drakelet`, `driftbell` (the nearest animal of that kind), `act` (a street act: `HW_ACT=prophet|theremin|statue|juggler|spotter|newsboy|stargazer|dancers|club`, `HW_ACT_FAR=1` wider), `powers`, `origin`, `title`. |
| `HW_TAXI=n` | Hail a taxi to stand n at once (`HW_TAXI_SECS` delays it). |
| `HW_WALK=1` | The hero walks on its own. |
| `HW_CUT_LEVEL=n` | Cut buildings open at floor n. |
| `HW_GRASS=0`, `HW_LAWN=0` | Turn grass blades / lawns off. |
| `HW_CRITTERS=0` | No animals (to measure without them). |
| `HW_LOOK=plain` | Turn off the lighting look and colour grade. |
| `HW_PROP=...` | The prop shown by `HW_VIEW=props`. |

### Title, menus, network, crashes

| Variable | Effect |
| --- | --- |
| `HW_TITLE=0` | Skip the title. `HW_TITLE_PANEL=new\|friends\|settings\|creator\|city`, `HW_TITLE_TIME`, `HW_TITLE_AUTOSTART` drive it. |
| `HW_CREATOR_SECTION`, `HW_CREATOR_DETAILED=1` | Open the creator at a section, in the detailed view. |
| `HW_CREATOR_CLIP=hover\|fly\|power` | The creator's hero plays that clip (close-ups of the hero's own poses). |
| `HW_MENU=main\|settings\|controls` | Open the menu (button top right). |
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
| Underground | Utility tunnels, Molehaven, the Aether Tube with stations, exits, mole homes, markets, shrines and burrows along it, the Doctor's lab, buried ruins, the Deeps | Sewers, more lairs, townsfolk riding the tube |
| Heroes | 20 levels, a power tree per origin (22-23 powers each) plus a travel tree, powers as one parametrized record, six elements, a drag-and-drop power tray (0.164), defensive toggles paid for with energy recovery and flight that brings no energy back (0.177) | Villains on the same records, augments, balancing the trees |
| Looks | Creator with six costumes and sliders | Full costume creator, capes, masks (in progress) |
| Combat | Minions with roles (shield bearer, grenadier, orderly), lieutenant, boss, colossus, drones, revive, hospital; Hero Hall's five heroes give tips and answer ignored fights | Teaming up with the town's heroes, the Doctor in person |
| Factions | Nine factions, regard, turf warnings; mole folk, sky pirates, the Iron Legion and the Zorrak invaders with their own incidents and homes | Patrol units, turf changing hands |
| Co-op | Host's level for all | Per-player sidekick and exemplar levels |
| Destruction | Buildings, props, cars, self-repair, collateral tally, careful-hero bonus | Collateral cost, digging and craters |
| Saves | Hero on each PC; world save in `hw-headless` and for host migration | World save for the single-player game |
| Money | Karma only | Dollars for equipment (maybe) |
| UI | HUD, radio, compass, map, minimap, autorun, powers, menu, debug menu (Ctrl+Shift+F12) | |
| Sound | Title theme, colossus march, music by scenery and hour, city soundscape, combat and robot sounds | Streamed music decoding (Engine) |
| Multiplayer | Room codes, UPnP, host migration, soak-tested by bots | A play test with people |
