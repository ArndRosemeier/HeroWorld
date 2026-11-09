//! Lit windows that follow the hour: every room that has its lights on
//! after dark sits on one of Engine's glow channels, by the colour of its
//! light and its household's habit (when the lights go on in the evening,
//! when they go off at night, who is up before dawn). Switching a channel
//! costs nothing, so the town's windows come on one by one at dusk and go
//! dark again late at night without rebuilding a single building.

use engine::prelude::*;

/// The colour of a room's light.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// A television's or a fluorescent tube's blue-white.
    Cool,
    /// An orange lampshade.
    Lamp,
    /// Warm incandescent bulbs, the most common.
    Warm,
}

const TONES: [Tone; 3] = [Tone::Cool, Tone::Lamp, Tone::Warm];

/// Households by when they switch the lights on and off.
pub const HABITS: usize = 5;

/// The glow channel of a room with this light and habit (1 to 15).
pub fn channel(tone: Tone, habit: usize) -> u8 {
    let t = TONES.iter().position(|t| *t == tone).unwrap_or(2);
    1 + (t * HABITS + habit.min(HABITS - 1)) as u8
}

/// The surface of a window pane on `channel`: glass, glowing when its room
/// is lit.
pub fn pane_surface(channel: u8) -> SurfaceMaterial {
    SurfaceMaterial {
        roughness: 0.06,
        metallic: 0.5,
        emission: 1.2,
        ..SurfaceMaterial::DEFAULT
    }
    .on_glow_channel(channel)
}

fn tone_color(t: Tone) -> Color {
    match t {
        Tone::Cool => rgb(190, 214, 255),
        Tone::Lamp => rgb(255, 190, 120),
        Tone::Warm => rgb(255, 214, 150),
    }
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How brightly a household with `habit` has its lights on at `hour`
/// (0 off .. 1 on). Evenings: on between about 17:30 and 19:15, off between
/// 22:00 and 1:30. The two earliest habits are up again before dawn.
pub fn lights_on(habit: usize, hour: f32) -> f32 {
    let g = habit.min(HABITS - 1) as f32;
    let h = hour.rem_euclid(24.0);
    let dusk = 17.4 + 0.45 * g;
    let bed = 22.0 + 0.9 * g;
    // Hours since noon, so an evening that runs past midnight is one span.
    let since_noon = (h - 12.0).rem_euclid(24.0);
    let evening = smooth(dusk - 12.0, dusk - 11.7, since_noon)
        * (1.0 - smooth(bed - 12.0, bed - 11.7, since_noon));
    let morning = if habit < 2 {
        let up = 5.4 + 0.4 * g;
        smooth(up, up + 0.3, h) * (1.0 - smooth(7.2, 7.6, h))
    } else {
        0.0
    };
    evening.max(morning)
}

/// Switches every window channel for `hour`.
pub fn update(world: &mut World, hour: f32) {
    for tone in TONES {
        for habit in 0..HABITS {
            let _ = world.set_glow_channel(
                channel(tone, habit),
                lights_on(habit, hour),
                Some(tone_color(tone)),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_cover_one_to_fifteen() {
        let mut seen = std::collections::BTreeSet::new();
        for t in TONES {
            for h in 0..HABITS {
                seen.insert(channel(t, h));
            }
        }
        assert_eq!(seen.len(), 15);
        assert_eq!(*seen.first().unwrap(), 1);
        assert_eq!(*seen.last().unwrap(), 15);
    }

    #[test]
    fn windows_light_up_in_the_evening_and_go_dark_at_night() {
        let lit = |hour: f32| (0..HABITS).filter(|&g| lights_on(g, hour) > 0.5).count();
        assert_eq!(lit(13.0), 0, "noon: nobody");
        assert_eq!(lit(16.0), 0);
        assert!(lit(18.0) >= 1 && lit(18.0) < HABITS, "dusk: some");
        assert_eq!(lit(20.5), HABITS, "evening: everyone");
        assert!(lit(23.5) < HABITS, "bedtime: fewer");
        assert_eq!(lit(3.0), 0, "small hours: dark");
        assert!(lit(6.0) >= 1, "early risers");
        assert_eq!(lit(9.0), 0);
    }
}
