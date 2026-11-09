# Title screen sounds

Made with OpenRouter on 2026-11-08 (the client builds them into the binary). To make one
again, use the same model and prompt; the outputs differ from run to run.

| File | Model | Prompt |
| --- | --- | --- |
| `title_theme.ogg` | `google/lyria-3-pro-preview` | Instrumental main menu music for a cheerful retro superhero game: 1940s cartoon adventure serial orchestra, a memorable heroic trumpet melody, swinging strings, glockenspiel sparkles, light march drums, playful woodwinds, sunset over a gleaming chrome city. Bright, friendly, grand but fun. Starts gently and builds. No vocals. (Arnd picked it over the first take, 2026-11-08.) |
| `whoosh.ogg` | `bytedance-seed/seed-audio-1-0` | The sound of a fast cartoon whoosh swooping past, a bright airy swish with a rising shimmer at the end |
| `glint.ogg` | `bytedance-seed/seed-audio-1-0` | The sound of a magical sparkle glint, a short twinkling chime like light flashing off polished chrome |
| `click.ogg` | `bytedance-seed/seed-audio-1-0` | The sound of a satisfying chunky retro button click with a tiny bright bell ding, very short |
| `start.ogg` | `bytedance-seed/seed-audio-1-0` | The sound of a heroic rocket takeoff swoosh with a triumphant shimmering sparkle, short and exciting |
| `hover.ogg` | none | a synthesized 90 ms rising blip (sine 520 to 1220 Hz) |


The visitors and their sounds (`fun/`, 0.076). Every picture prompt ended with: "1950s golden-age comic book illustration, bold thick black ink outlines, flat bright retro colours, halftone shading, isolated on a plain pure white background, no text, no frame, full figure centered". The white background was keyed out (flood fill from the corners), the picture cropped and scaled to at most 512 px.

| File | Model | Prompt |
| --- | --- | --- |
| `fun/robot.png` | `meta/muse-image` | A giant goofy retro tin robot head and shoulders peeking up, huge glowing red eyes, riveted steel, antenna, open grille mouth, arms raised menacingly, playful cheesy menace |
| `fun/alien.png` | `meta/muse-image` | A bug-eyed green martian alien face pressed close, huge bulging eyes, tiny antennae, wide open mouth screaming, silly and startling |
| `fun/villain.png` | `meta/muse-image` | A cackling mad scientist supervillain with a cape and monocle, rubbing hands together, dramatic pose, purple and green costume |
| `fun/octo.png` | `meta/muse-image` | A giant purple tentacled space octopus monster rising up, many waving tentacles, big googly eyes, cheeky grin |
| `fun/saucer.png` | `meta/muse-image` | A classic chrome flying saucer UFO with a glass dome and a little alien pilot waving, blinking lights around the rim, seen slightly from below |
| `fun/hero.png` | `meta/muse-image` | A cheerful flying superhero in red and blue with a yellow cape, flying horizontally to the right with one fist forward, big grin |
| `fun/roar.ogg` | `bytedance-seed/seed-audio-1-0` | The sound of a giant clanking robot monster roaring with a metallic distorted growl and heavy servo whine, short and loud |
| `fun/splat.ogg` | `bytedance-seed/seed-audio-1-0` | The sound of a wet cartoon splat against glass followed by a squeaky slide down |
| `fun/thunder.ogg` | `bytedance-seed/seed-audio-1-0` | The sound of a sudden loud thunder crack and rumble |
| `fun/tentacle.ogg` | `bytedance-seed/seed-audio-1-0` | The sound of a slimy squelchy monster rising out of water with a gurgling bubbling growl |
| `fun/ufo.ogg` | `bytedance-seed/seed-audio-1-0` | The sound of a retro flying saucer zooming past, warbling theremin oscillation with a doppler swoosh |
| `fun/whee.ogg` | `bytedance-seed/seed-audio-1-0` | The sound of a cartoon slide whistle going up quickly, playful |
| `fun/boing.ogg` | `bytedance-seed/seed-audio-1-0` | The sound of a cartoon spring boing, bouncy and comical, very short |
| `fun/pow.ogg` | `bytedance-seed/seed-audio-1-0` | The sound of a cartoon punch impact, a punchy comic whack with a little zing, very short |
| `fun/explode.ogg` | `bytedance-seed/seed-audio-1-0` | The sound of a cartoon explosion poof with sparkles and a little tinkle afterwards |

Processing: silence trimmed, loudness normalised (`loudnorm I=-16`), mono Vorbis for the
effects; the theme got a 7 s fade at its end so it loops back into its soft opening.

Fonts (`../fonts`): Luckiest Guy (Apache 2.0) for the logo, Titan One (SIL OFL 1.1) for
buttons, both from Google Fonts; their licences sit next to them.
