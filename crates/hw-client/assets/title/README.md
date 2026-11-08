# Title screen sounds

Made with OpenRouter on 2026-11-08 (the client builds them into the binary). To make one
again, use the same model and prompt; the outputs differ from run to run.

| File | Model | Prompt |
| --- | --- | --- |
| `title_theme.ogg` | `google/lyria-3-pro-preview` | Instrumental title theme for a playful golden-age comic superhero video game set in a sunny retro-futurist utopia city (1939 World's Fair, flying cars, ray guns). Heroic, warm and inviting, a little whimsical. Soft opening with shimmering strings and a gentle theremin-like lead, then a bold, catchy brass fanfare melody over a bouncy march rhythm with snare and timpani, full orchestra, big band swing touches. Optimistic, uplifting, adventurous. No vocals. |
| `whoosh.ogg` | `bytedance-seed/seed-audio-1-0` | The sound of a fast cartoon whoosh swooping past, a bright airy swish with a rising shimmer at the end |
| `glint.ogg` | `bytedance-seed/seed-audio-1-0` | The sound of a magical sparkle glint, a short twinkling chime like light flashing off polished chrome |
| `click.ogg` | `bytedance-seed/seed-audio-1-0` | The sound of a satisfying chunky retro button click with a tiny bright bell ding, very short |
| `start.ogg` | `bytedance-seed/seed-audio-1-0` | The sound of a heroic rocket takeoff swoosh with a triumphant shimmering sparkle, short and exciting |
| `hover.ogg` | none | a synthesized 90 ms rising blip (sine 520 to 1220 Hz) |

Processing: silence trimmed, loudness normalised (`loudnorm I=-16`), mono Vorbis for the
effects; the theme got a 7 s fade at its end so it loops back into its soft opening.

Fonts (`../fonts`): Luckiest Guy (Apache 2.0) for the logo, Titan One (SIL OFL 1.1) for
buttons, both from Google Fonts; their licences sit next to them.
