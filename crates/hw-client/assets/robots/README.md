# Robot sounds

Made with OpenRouter on 2026-11-08 with `bytedance-seed/seed-audio-1-0`; the client builds them
into the binary (`robotview.rs`). To make one again use the same model and prompt; the outputs
differ from run to run.

| File | Plays when | Prompt |
| --- | --- | --- |
| `footfall.ogg` | each footfall, the kneel | The sound of a gigantic iron robot foot slamming onto a city street, a huge deep boom with crunching asphalt and rattling metal, heavy and short |
| `cannon.ogg` | the cannon fires | The sound of a huge tank cannon firing, a deep thunderous blast with a metallic ring and a rushing whoosh of the shell leaving |
| `klaxon.ogg` | a shell is coming (where it lands) | The sound of a retro 1950s science fiction robot alarm klaxon, a loud pulsing mechanical warning horn, two blasts |
| `servo.ogg` | stomp windup, getting up | The sound of a giant hydraulic robot servo motor straining, deep grinding whine with hissing pneumatic pistons and clanking gears |
| `hum.ogg` | core charging | The sound of a powerful electric generator charging up, a rising crackling electric hum building to a high whine, retro science fiction |
| `zap.ogg` | core discharge | The sound of a massive electric discharge shockwave, a huge crackling zap of lightning with a booming burst, retro science fiction |
| `partbreak.ogg` | a weak point blown off | The sound of a big robot joint exploding, a sharp metal explosion with shattering steel parts and sparks fizzing |
| `collapse.ogg` | it goes down on its belly | The sound of a colossal metal war machine crashing down onto its belly, an enormous groaning metal crash with a deep impact boom and debris |

Processing: silence trimmed, loudness normalised (`loudnorm I=-16:TP=-1.5`), mono Vorbis
(q 4), cut to length with a 0.5 s fade out.
