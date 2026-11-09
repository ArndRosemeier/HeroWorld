//! The town's light and look: sun shadows that reach across the town, the
//! colour grade and bloom for each hour, and how much of it each quality
//! level can afford.
//!
//! The look follows the usual lighting-artist recipe. By day: a warm sun,
//! slightly cooler shade, a little more colour and contrast than the raw
//! render (golden-age postcards, not a grey overcast photo). At night: a
//! cool, dim moonlit base, so the warm street lamps and lit windows are the
//! focal points, and bloom low enough that they glow without washing out.
//!
//! `HW_LOOK=plain` switches the grade, bloom and shadow changes off, to
//! compare with Engine's defaults.

use engine::prelude::*;
use engine::quality::Quality;
use glam::Vec3;

/// True unless `HW_LOOK=plain`.
fn styled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("HW_LOOK").map_or(true, |v| !v.eq_ignore_ascii_case("plain")))
}

/// Sun shadows for a town seen from street level and from the air. Engine's
/// default ends at 120 m, so from a rooftop or in flight whole blocks stand
/// unshadowed and the town looks like a model; these reach a few hundred
/// metres, with bigger maps so the near cascade stays as sharp.
pub fn shadows(q: Quality) -> ShadowSettings {
    let (ends, size) = match q {
        Quality::Low => ([10.0, 35.0, 140.0], 1024),
        Quality::Medium => ([14.0, 60.0, 300.0], 2048),
        Quality::High => ([16.0, 80.0, 500.0], 2048),
        Quality::Ultra => ([20.0, 100.0, 700.0], 4096),
    };
    ShadowSettings {
        cascade_end_m: ends,
        map_size: size,
        ..ShadowSettings::default()
    }
}

/// Lamps that cast shadows at once (street lamps, kiosks, landmarks).
pub fn lamp_shadows(q: Quality) -> usize {
    match q {
        Quality::Low => 1,
        Quality::Medium => 3,
        Quality::High => 6,
        Quality::Ultra => 8,
    }
}

/// Ambient occlusion sized for buildings and streets: a little wider than
/// Engine's 1 m, so walls sit on the pavement and doorways and eaves read.
fn occlusion() -> AmbientOcclusion {
    AmbientOcclusion::new(1.6, 1.15).unwrap_or_default()
}

/// Sets what the quality level allows (after `World::set_quality`).
pub fn apply_quality(world: &mut World, q: Quality) {
    if !styled() {
        return;
    }
    world.set_shadows(Some(shadows(q)));
    world.set_lamp_shadow_budget(lamp_shadows(q));
    if world.ambient_occlusion().is_some() {
        world.set_ambient_occlusion(Some(occlusion()));
    }
}

/// How much of the night look applies at `hour`: 0 by day, 1 from dusk to
/// dawn, blending through the golden and blue hours.
pub fn night(hour: f32) -> f32 {
    let h = hour.rem_euclid(24.0);
    let smooth = |a: f32, b: f32, x: f32| {
        let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    // Dusk 17.5 -> 19.5, dawn 4.5 -> 6.5.
    smooth(17.5, 19.5, h).max(1.0 - smooth(4.5, 6.5, h))
}

/// How golden the light is: 1 around sunrise and sunset, 0 at noon and
/// deep night.
pub fn golden(hour: f32) -> f32 {
    let h = hour.rem_euclid(24.0);
    let bump = |c: f32, w: f32| (1.0 - ((h - c) / w).abs()).clamp(0.0, 1.0);
    bump(18.0, 1.8).max(bump(6.0, 1.8))
}

/// Daylight: warm highlights, a touch of blue in the shade, a little more
/// colour and bite than the raw render. Restrained: the facades' own
/// colours carry the picture.
pub fn day_grade() -> ColorGrade {
    ColorGrade {
        exposure: -0.05,
        contrast: 1.1,
        saturation: 1.14,
        temperature: 0.1,
        tint: 0.0,
        shadows: Vec3::new(0.93, 0.97, 1.05),
        highlights: Vec3::new(1.04, 1.01, 0.95),
        vignette: 0.12,
    }
}

/// Golden hour: warmer and richer still.
pub fn golden_grade() -> ColorGrade {
    ColorGrade {
        exposure: 0.0,
        contrast: 1.12,
        saturation: 1.2,
        temperature: 0.22,
        tint: 0.03,
        shadows: Vec3::new(0.9, 0.93, 1.08),
        highlights: Vec3::new(1.06, 1.0, 0.9),
        vignette: 0.16,
    }
}

/// Night: moonlit blue shade, warm lights kept warm, darker corners.
pub fn night_grade() -> ColorGrade {
    ColorGrade {
        exposure: -0.2,
        contrast: 1.12,
        saturation: 1.05,
        temperature: -0.08,
        tint: 0.0,
        shadows: Vec3::new(0.82, 0.9, 1.12),
        highlights: Vec3::new(1.06, 1.0, 0.92),
        vignette: 0.22,
    }
}

/// The grade at `hour`.
pub fn grade(hour: f32) -> ColorGrade {
    let day = day_grade().lerp(&golden_grade(), golden(hour) * (1.0 - night(hour)));
    day.lerp(&night_grade(), night(hour))
}

/// Bloom at `hour`: a soft halo by day (chrome, the sun on glass), more at
/// night so lamps and windows glow. Engine sets the exposure itself from
/// the sky, so only threshold, knee and strength change here.
pub fn bloom(world: &World, hour: f32) -> BloomSettings {
    let n = night(hour);
    let b = world.bloom();
    b.with_threshold(1.0 - 0.25 * n)
        .and_then(|b| b.with_soft_knee(0.6))
        .and_then(|b| b.with_intensity(0.3 + 0.2 * n))
        .unwrap_or(b)
}

/// The sky at `hour`, with the look that goes with it. Use this instead
/// of `World::set_physical_sky`.
pub fn set_sky(world: &mut World, hour: f32) {
    set_sky_wet(world, hour, 0.0);
}

/// The sky at `hour` with the camera `wet` (0..1) under a rain cloud (the
/// weather tower's): the sky greys over and the sun goes in.
pub fn set_sky_wet(world: &mut World, hour: f32, wet: f32) {
    let w = wet.clamp(0.0, 1.0);
    let mix = |a: f32, b: f32| a + (b - a) * w;
    let sky = PhysicalSky {
        // A few more clouds than Engine's clear sky: something for the
        // sunset to colour and for the eye to rest on above the roofs.
        weather: SkyWeather {
            cover: mix(0.42, 0.97),
            density: mix(SkyWeather::CLEAR.density, 0.9),
            dark: mix(0.0, 0.45),
            sun: mix(1.0, 0.22),
            fog: mix(0.0, 0.15),
            rain: mix(0.0, 0.6),
        },
        ..PhysicalSky::at_hour(hour)
    };
    world.set_physical_sky(Some(if styled() {
        sky
    } else {
        PhysicalSky::at_hour(hour)
    }));
    crate::daylight::update(world, hour);
    if !styled() {
        return;
    }
    let _ = world.set_color_grade(grade(hour));
    let b = bloom(world, hour);
    world.set_bloom(b);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grades_are_valid_all_day() {
        for i in 0..96 {
            grade(i as f32 * 0.25).validate().unwrap();
        }
    }

    #[test]
    fn night_and_day() {
        assert_eq!(night(12.0), 0.0);
        assert_eq!(night(22.0), 1.0);
        assert_eq!(night(2.0), 1.0);
        assert!(night(18.5) > 0.0 && night(18.5) < 1.0);
        assert!(golden(18.0) > 0.9 && golden(12.0) == 0.0);
    }

    #[test]
    fn better_quality_shadows_reach_farther() {
        let ends: Vec<f32> = Quality::ALL
            .iter()
            .map(|q| shadows(*q).cascade_end_m[2])
            .collect();
        assert!(ends.windows(2).all(|w| w[0] < w[1]));
        assert!(Quality::ALL.iter().all(|q| lamp_shadows(*q) <= 8));
    }
}
