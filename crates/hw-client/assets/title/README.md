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

Processing: silence trimmed, loudness normalised (`loudnorm I=-16`), mono Vorbis for the
effects; the theme got a 7 s fade at its end so it loops back into its soft opening.

Fonts (`../fonts`): Luckiest Guy (Apache 2.0) for the logo, Titan One (SIL OFL 1.1) for
buttons, both from Google Fonts; their licences sit next to them.
