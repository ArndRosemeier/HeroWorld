//! Biomes: what grows where, from temperature and rain.

/// The kind of ground cover an area has. Order is stable: it indexes tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum Biome {
    #[default]
    Ocean,
    Lake,
    /// Ice sheets and glaciers.
    Ice,
    Tundra,
    /// Cold conifer forest.
    Taiga,
    /// Dry cold plains.
    Steppe,
    Grassland,
    /// Dry summers, scrub and open woods.
    Shrubland,
    TemperateForest,
    Desert,
    Savanna,
    /// Tropical forest with a dry season.
    SeasonalForest,
    Rainforest,
    /// Bare rock and snow above the trees.
    Alpine,
}

impl Biome {
    pub const ALL: [Biome; 14] = [
        Biome::Ocean,
        Biome::Lake,
        Biome::Ice,
        Biome::Tundra,
        Biome::Taiga,
        Biome::Steppe,
        Biome::Grassland,
        Biome::Shrubland,
        Biome::TemperateForest,
        Biome::Desert,
        Biome::Savanna,
        Biome::SeasonalForest,
        Biome::Rainforest,
        Biome::Alpine,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Biome::Ocean => "ocean",
            Biome::Lake => "lake",
            Biome::Ice => "ice",
            Biome::Tundra => "tundra",
            Biome::Taiga => "taiga",
            Biome::Steppe => "steppe",
            Biome::Grassland => "grassland",
            Biome::Shrubland => "shrubland",
            Biome::TemperateForest => "temperate forest",
            Biome::Desert => "desert",
            Biome::Savanna => "savanna",
            Biome::SeasonalForest => "seasonal forest",
            Biome::Rainforest => "rainforest",
            Biome::Alpine => "alpine",
        }
    }

    /// A map colour (sRGB).
    pub fn color(self) -> [u8; 3] {
        match self {
            Biome::Ocean => [28, 62, 112],
            Biome::Lake => [60, 110, 170],
            Biome::Ice => [236, 242, 248],
            Biome::Tundra => [170, 180, 160],
            Biome::Taiga => [52, 96, 74],
            Biome::Steppe => [176, 176, 104],
            Biome::Grassland => [130, 166, 78],
            Biome::Shrubland => [150, 150, 90],
            Biome::TemperateForest => [60, 124, 58],
            Biome::Desert => [226, 204, 150],
            Biome::Savanna => [182, 168, 84],
            Biome::SeasonalForest => [78, 138, 56],
            Biome::Rainforest => [24, 104, 54],
            Biome::Alpine => [150, 146, 140],
        }
    }

    /// How much of the ground trees cover, 0 to 1 (open grass to closed canopy).
    pub fn tree_cover(self) -> f64 {
        match self {
            Biome::Ocean | Biome::Lake | Biome::Ice | Biome::Desert | Biome::Alpine => 0.0,
            Biome::Tundra | Biome::Steppe => 0.02,
            Biome::Grassland => 0.06,
            Biome::Shrubland => 0.2,
            Biome::Savanna => 0.15,
            Biome::Taiga => 0.7,
            Biome::TemperateForest => 0.8,
            Biome::SeasonalForest => 0.85,
            Biome::Rainforest => 0.95,
        }
    }
}

/// The biome of land with mean temperature `t` (°C), `rain` (mm a year) and
/// `height` metres above the sea.
pub fn classify(t: f64, rain: f64, height: f64) -> Biome {
    if t < -8.0 {
        return Biome::Ice;
    }
    if height > 1800.0 + (t.max(-5.0) + 5.0) * 60.0 && t < 3.0 || (height > 3600.0 && t < 6.0) {
        return Biome::Alpine;
    }
    if t < 0.0 {
        return Biome::Tundra;
    }
    if t < 7.0 {
        return if rain > 350.0 {
            Biome::Taiga
        } else {
            Biome::Steppe
        };
    }
    if t < 18.0 {
        return match rain {
            r if r < 200.0 => Biome::Desert,
            r if r < 350.0 => Biome::Steppe,
            r if r < 600.0 => {
                if t > 12.0 {
                    Biome::Shrubland
                } else {
                    Biome::Grassland
                }
            }
            _ => Biome::TemperateForest,
        };
    }
    match rain {
        r if r < 250.0 => Biome::Desert,
        r if r < 650.0 => Biome::Savanna,
        r if r < 1600.0 => Biome::SeasonalForest,
        _ => Biome::Rainforest,
    }
}
