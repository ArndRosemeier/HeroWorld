# Sounds

The town's sounds (`soundscape.rs`) and the fight's (`fightview.rs`), built into the game.
Decoded when first heard and forgotten after a minute of quiet.

## Made for HeroWorld

Made with OpenRouter on 2026-11-09 with `bytedance-seed/seed-audio-1-0` (Seed Audio renders a
sound, not speech, when the text starts with "The sound of"). Two to five takes each; a Gemini
model listened to every take blind and scored it against the prompt, and the best was kept.
Raw takes and scores: project files `heroworld/sounds/raw/`. To make one again use the same
model and prompt; the outputs differ from run to run.

Processing (`process.py`, kept beside the raw takes): silence trimmed, DC removed, mono Ogg
Vorbis (q 4), one-shots levelled to about -18 dBFS RMS with a short fade out, loops evened out
(2 s windows, at most +/-6 dB) and their tail crossfaded into their head so they loop without a
seam.

| File | Kind | Prompt |
| --- | --- | --- |
| `amb_villains.ogg` | loop | The sound of an eerie run-down industrial quarter, a low electrical hum, crackling sparks of a distant Tesla coil, clanking machinery and dripping pipes, continuous ambience |
| `amb_molehaven.ogg` | loop | The sound of a large echoing cave with distant rumbling machinery, slow water drips falling into puddles and a low hum, steady background ambience without voices |
| `v8_idle.ogg` | loop | The sound of an old big car engine idling at rest, a low rough rumbling purr, steady |
| `v8_cruise.ogg` | loop | The sound of a big 1950s American car driving steadily along a city street, smooth V8 engine rumble and tyre hiss, continuous |
| `hover_hum.ogg` | loop | The sound of a steady buzzing electric hum of a neon tube, constant (the neon-hum take, an octave down, low-passed, with a slow throb) |
| `siren.ogg` | loop | The sound of a 1950s ambulance mechanical siren wailing, a rising and falling motor wail, continuous |
| `power_hum.ogg` | loop | The sound of a steady buzzing electric hum of a neon tube, constant |
| `horn_1.ogg` | shot | The sound of a 1950s American car horn, two short bright brassy honks |
| `horn_long.ogg` | shot | The sound of a 1950s American car horn held down, one long brassy two-tone blare |
| `horn_2.ogg` | shot | The sound of a 1950s American car horn held down, one long brassy two-tone blare |
| `skid.ogg` | shot | The sound of car tyres screeching to a sudden stop on asphalt |
| `flyby.ogg` | shot | The sound of a retro science fiction flying car whooshing past overhead, a rising and falling jet hum |
| `clock_bell.ogg` | shot | The sound of a town hall clock tower bell striking three deep bronze strokes that ring out over a city |
| `pigeons.ogg` | shot | The sound of pigeons cooing softly on a city square, a few wing flaps |
| `dog_far.ogg` | shot | The sound of a dog barking twice in the distance in a quiet neighbourhood at night |
| `punch.ogg` | shot | The sound of a hard comic book punch landing, a meaty thwack with a deep thump |
| `bolt.ogg` | shot | The sound of a retro science fiction energy bolt fired from a hand, a crackling electric zap and whoosh |
| `ray.ogg` | shot | The sound of a toy laser ray gun firing, a quick wobbling electronic pew pew zap |
| `charge.ogg` | shot | The sound of an electric generator powering up, a whine rising quickly in pitch for two seconds |
| `whoosh.ogg` | shot | The sound of a stick swung fast through the air, a short sharp swish |
| `chime.ogg` | shot | The sound of a small glass wind chime tinkling once, light and sparkling |
| `boom.ogg` | shot | The sound of a powerful shockwave blast, a deep booming thud with rushing air |
| `bind.ogg` | shot | The sound of an energy binding snapping shut around someone, a crackling electric lock with a fizzing hum |
| `shield.ogg` | shot | The sound of a force shield switching on, a shimmering electric whoomp with a glassy ring |
| `fanfare.ogg` | shot | The sound of a short triumphant brass fanfare, bright trumpets, about two seconds |
| `tesla_buzz.ogg` | loop | The sound of a giant Tesla coil running continuously, a loud buzzing electric hum with rapid crackling sizzling sparks and snapping arcs, steady |
| `tesla_crack_1.ogg` | shot | The sound of a single huge electric arc discharge striking a lightning rod, a violent sharp crack and snap like close thunder with a fizzing sizzle tail |
| `tesla_crack_2.ogg` | shot | (the same prompt, another take) |
| `tesla_zap.ogg` | shot | The sound of a short burst of electric sparks crackling, a quick high voltage zap and sizzle |

## From Scale

Taken from Scale's library (`public/sounds`, see Scale's `docs/SOUNDS.md`): made locally with
Stability AI's Stable Audio 3 Small-SFX, used under the Stability AI Community License. Only the
ones that fit a 1950s town (no modern horns or engines):

- `amb_city_day`
- `amb_city_night`
- `amb_park`
- `amb_crowd`
- `amb_interior`
- `amb_wind_flight`
- `amb_tunnel (Scale's amb_sewer)`
- `step_concrete_1-4`
- `step_grass_1-2`
- `bird_flutter`
- `terrace_clatter`
- `alarm_bell`
- `scream_crowd_1`
- `crowd_gasp`
