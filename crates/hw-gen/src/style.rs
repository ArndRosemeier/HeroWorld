//! What a building looks like: its architectural style and the facade,
//! roof and ornament choices that follow from it.
//!
//! The style comes from the district, the building's use and its height,
//! then seeded choices vary everything inside the style, so two neighbours
//! of the same style still differ. All of it is data; the client turns it
//! into geometry. The look is the golden-age future: art deco towers,
//! streamline moderne, Googie, brick tenements in the dystopia.

use crate::building::BuildingUse;
use crate::city::DistrictKind;
use hw_core::{Rng, Seed};

/// An sRGB colour, picked by eye.
pub type Rgb = [u8; 3];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Style {
    /// Stone towers with vertical piers, setbacks and a crown or spire.
    ArtDeco,
    /// Rounded corner, horizontal bands and ribbon windows; pastel render.
    Streamline,
    /// Glass and metal towers of tomorrow: curtain walls, fins, masts.
    Skyline,
    /// Civic halls: a columned base, a dome or a stepped crown.
    Civic,
    /// Brick commercial blocks with big shop windows and awnings.
    Emporium,
    /// Apartments with balconies and a garden roof.
    GardenFlats,
    /// Houses with swooping roofs, stone walls and wide glass.
    Googie,
    /// Aether works: brick or metal halls, sawtooth roofs, chimneys.
    Works,
    /// Fair pavilions: bold colours, domes, pylons and canopies.
    Pavilion,
    /// Dystopian walk-ups: dark brick, fire escapes, water tanks.
    Tenement,
    /// The mad scientist's labs: concrete, portholes, a dome and a coil.
    Lab,
    /// Old-town houses: plastered stone under steep tiled gables and hips.
    OldTown,
    /// Narrow brick merchants' houses, gable to the street (the north).
    Burgher,
    /// Stone apartment blocks with balconies under a zinc mansard.
    Haussmann,
    /// Brick terraces: a cornice, a flat roof or a slate mansard.
    Rowhouse,
    /// Southern stucco under shallow terracotta hips, arches and loggias.
    Villa,
    /// Family houses with pitched roofs and a chimney.
    Cottage,
}

/// Which way a pitched roof's ridge runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ridge {
    /// Along the longer side.
    Long,
    /// Along the street front (eaves to the street).
    Front,
    /// Front to back (a gable to the street).
    Deep,
}

/// How the land builds, by climate and tradition (one per town).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Region {
    /// Brick, steep slate and tile gables, gables to the street.
    Northern,
    /// Plaster and stone, tiled gables and hips, mansards.
    Continental,
    /// Stucco, shallow terracotta hips.
    Southern,
}

impl Region {
    pub const ALL: [Region; 3] = [Region::Northern, Region::Continental, Region::Southern];
}

/// The kind of quarter a building stands in, for towns with a past.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Quarter {
    /// The old town inside (or where once stood) the walls.
    OldTown,
    /// The planned and grown extensions: terraces and apartment blocks.
    Terraces,
    /// Houses with gardens.
    Suburb,
}

/// Tradition and quarter together: buildings with this build in the old
/// ways of their region (the golden-age styles still mix in).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Vernacular {
    pub region: Region,
    pub quarter: Quarter,
}

/// What the main walls are made of (colour now, texture later).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Material {
    Limestone,
    Granite,
    Brick,
    Terracotta,
    Stucco,
    Concrete,
    Metal,
    Glass,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WindowKind {
    /// One opening per bay.
    Punched,
    /// Arched head.
    Arched,
    /// Wide openings joined into a band along each floor.
    Ribbon,
    /// Floor-to-ceiling glass between thin mullions.
    Curtain,
    /// Round.
    Porthole,
}

/// Vertical members between bays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Piers {
    None,
    /// Shallow pilasters, broken by every floor band.
    Pilasters,
    /// Deep piers running the full height of each tier (art deco).
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cornice {
    None,
    /// One projecting band.
    Simple,
    /// Three stepped bands.
    Stepped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Roof {
    /// Flat behind a parapet.
    Flat,
    Gable,
    Hip,
    Mansard,
    Dome,
    /// A tilted Googie slab that sweeps up at the front.
    Swoop,
    Sawtooth,
}

/// What stands on top of a tower.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Crown {
    None,
    /// Shrinking tiers.
    Stepped,
    /// Shrinking tiers and a needle.
    Spire,
    /// A radio mast with rings.
    Mast,
    /// A dome on a drum.
    Dome,
    /// Upright fins round a lantern.
    Fins,
}

/// The full look of one building.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub style: Style,
    pub material: Material,
    pub wall: Rgb,
    /// Piers, bands, cornices, frames around the entrance.
    pub trim: Rgb,
    /// Awnings, signs, canopies, crown details.
    pub accent: Rgb,
    pub roof_color: Rgb,
    /// The base course and steps.
    pub base: Rgb,
    pub glass: Rgb,
    pub frame: Rgb,
    /// Facade bay width target (m): one window per bay.
    pub bay: f64,
    pub window: WindowKind,
    /// Window width as a share of the bay.
    pub window_share: f64,
    /// How far windows sit back in the wall (m).
    pub reveal: f64,
    pub piers: Piers,
    /// Bands along every floor line.
    pub floor_bands: bool,
    /// Height of a stone base course (0: none).
    pub plinth: f64,
    pub cornice: Cornice,
    pub roof: Roof,
    pub crown: Crown,
    /// Parapet height on flat roofs (0: none).
    pub parapet: f64,
    /// A round tower on the front-right corner (streamline).
    pub corner_tower: bool,
    pub balconies: bool,
    pub fire_escape: bool,
    pub awnings: bool,
    /// A projecting slab over the entrance.
    pub canopy: bool,
    /// An upright neon sign on the front.
    pub sign: bool,
    pub water_tank: bool,
    /// Vents, boxes and a stair house on the roof.
    pub rooftop_gear: bool,
    /// Tall decorative fins (Googie pylons, deco fins).
    pub fins: bool,
    /// Rise over run of a pitched roof.
    pub pitch: f64,
    pub ridge: Ridge,
    /// How far a pitched roof reaches past the walls (m).
    pub eaves: f64,
    pub chimneys: bool,
    /// Dormer windows in a mansard or a steep roof.
    pub dormers: bool,
}

const LIMESTONE: [Rgb; 4] = [
    [228, 218, 196],
    [214, 204, 182],
    [236, 228, 210],
    [204, 196, 178],
];
const GRANITE: [Rgb; 3] = [[150, 146, 142], [120, 116, 114], [168, 160, 150]];
const BRICK: [Rgb; 5] = [
    [150, 72, 56],
    [128, 64, 50],
    [168, 96, 70],
    [112, 74, 60],
    [176, 120, 88],
];
const DARK_BRICK: [Rgb; 3] = [[92, 52, 44], [80, 60, 54], [104, 66, 56]];
const TERRACOTTA: [Rgb; 3] = [[196, 132, 96], [210, 160, 120], [184, 116, 84]];
const PASTELS: [Rgb; 8] = [
    [240, 230, 208],
    [244, 214, 180],
    [214, 232, 222],
    [206, 222, 240],
    [246, 218, 214],
    [236, 236, 228],
    [250, 238, 196],
    [222, 214, 238],
];
const BOLD: [Rgb; 6] = [
    [226, 92, 70],
    [64, 148, 196],
    [244, 196, 64],
    [92, 176, 140],
    [236, 128, 168],
    [128, 112, 208],
];
const CONCRETE: [Rgb; 3] = [[176, 174, 168], [150, 150, 146], [196, 192, 184]];
const METAL: [Rgb; 3] = [[188, 196, 204], [150, 160, 172], [214, 210, 196]];
const GOLD: [Rgb; 4] = [
    [212, 172, 96],
    [184, 136, 72],
    [200, 196, 188],
    [160, 104, 72],
];
const PLASTER: [Rgb; 7] = [
    [226, 214, 188],
    [214, 196, 160],
    [232, 222, 200],
    [200, 176, 140],
    [222, 200, 170],
    [236, 226, 206],
    [206, 190, 170],
];
const SOUTH: [Rgb; 6] = [
    [238, 226, 200],
    [232, 206, 170],
    [222, 190, 150],
    [240, 234, 220],
    [226, 176, 132],
    [236, 214, 160],
];
const CLAY_TILE: [Rgb; 4] = [[168, 84, 56], [150, 70, 48], [182, 104, 70], [140, 78, 58]];
const SLATE: [Rgb; 3] = [[70, 74, 82], [84, 86, 92], [60, 62, 70]];
const ZINC: [Rgb; 2] = [[120, 130, 136], [104, 112, 120]];
const SHINGLE: [Rgb; 4] = [[80, 70, 66], [96, 84, 74], [70, 72, 76], [110, 64, 52]];
const GLASS: [Rgb; 4] = [
    [64, 104, 128],
    [72, 124, 140],
    [52, 84, 108],
    [96, 132, 150],
];

fn pick(rng: &mut Rng, colors: &[Rgb]) -> Rgb {
    *rng.pick(colors)
}

/// Mixes a colour a little towards white or black, so neighbours of one
/// palette still differ.
fn vary(rng: &mut Rng, c: Rgb, amount: f64) -> Rgb {
    let k = rng.range(-amount, amount);
    c.map(|v| {
        let v = v as f64;
        let t = if k >= 0.0 { 255.0 } else { 0.0 };
        (v + (t - v) * k.abs()).round().clamp(0.0, 255.0) as u8
    })
}

impl Style {
    /// The style for a building of this district, use and height.
    pub fn choose(district: DistrictKind, usage: BuildingUse, floors: u32, rng: &mut Rng) -> Style {
        use Style::*;
        let tall = floors >= 10;
        match (district, usage) {
            (_, BuildingUse::House) => Googie,
            (_, BuildingUse::Works) => Works,
            (_, BuildingUse::Pavilion) => Pavilion,
            (_, BuildingUse::Lab) => Lab,
            (_, BuildingUse::Tenement) => Tenement,
            (DistrictKind::Civic, _) if tall => *rng.pick(&[ArtDeco, ArtDeco, Skyline]),
            (DistrictKind::Civic, _) => *rng.pick(&[Civic, ArtDeco, Streamline]),
            (DistrictKind::Commercial, _) if tall => *rng.pick(&[ArtDeco, Skyline, Streamline]),
            (DistrictKind::Commercial, _) => *rng.pick(&[Emporium, Emporium, Streamline, ArtDeco]),
            (DistrictKind::Residential, _) => *rng.pick(&[GardenFlats, Streamline, Emporium]),
            (DistrictKind::Expo, _) => Pavilion,
            (DistrictKind::AetherWorks, _) => Works,
            (DistrictKind::Dystopia, _) => Tenement,
        }
    }
}

impl Look {
    /// The look of a building; the same site always gets the same look.
    pub fn choose(seed: Seed, district: DistrictKind, usage: BuildingUse, floors: u32) -> Look {
        let mut rng = seed.child("look").rng();
        let style = Style::choose(district, usage, floors, &mut rng);
        Look::styled(style, floors, &mut rng)
    }

    /// The look of a building in a town with a past: its region's old ways
    /// in old towns, terraces and suburbs, the golden-age styles mixed in
    /// (and alone for towers, works, labs and the dystopia).
    pub fn vernacular(
        seed: Seed,
        v: Vernacular,
        district: DistrictKind,
        usage: BuildingUse,
        floors: u32,
    ) -> Look {
        use Region::*;
        use Style::*;
        let mut rng = seed.child("look").rng();
        let r = &mut rng;
        let special = matches!(
            usage,
            BuildingUse::Works | BuildingUse::Pavilion | BuildingUse::Lab | BuildingUse::Tenement
        ) || floors >= 9;
        let style = if special {
            Style::choose(district, usage, floors, r)
        } else {
            let house = usage == BuildingUse::House;
            let (styles, w): (&[Style], &[f64]) = match (v.quarter, v.region) {
                (Quarter::Suburb, Southern) if house => {
                    (&[Villa, Cottage, Googie], &[0.55, 0.25, 0.2])
                }
                (Quarter::Suburb, _) if house => (&[Cottage, Googie], &[0.8, 0.2]),
                (Quarter::Suburb, Southern) => {
                    (&[Villa, GardenFlats, Streamline], &[0.5, 0.3, 0.2])
                }
                (Quarter::Suburb, _) => (&[Rowhouse, GardenFlats, Streamline], &[0.4, 0.4, 0.2]),
                (Quarter::OldTown, Northern) => (&[Burgher, OldTown], &[0.7, 0.3]),
                (Quarter::OldTown, Continental) => (&[OldTown, Burgher], &[0.85, 0.15]),
                (Quarter::OldTown, Southern) => (&[Villa, OldTown], &[0.75, 0.25]),
                (Quarter::Terraces, Northern) => (
                    &[Rowhouse, Burgher, Emporium, GardenFlats, Streamline],
                    &[0.4, 0.15, 0.2, 0.15, 0.1],
                ),
                (Quarter::Terraces, Continental) => (
                    &[
                        Haussmann,
                        OldTown,
                        Rowhouse,
                        Emporium,
                        Streamline,
                        GardenFlats,
                    ],
                    &[0.45, 0.1, 0.1, 0.15, 0.1, 0.1],
                ),
                (Quarter::Terraces, Southern) => (
                    &[Villa, Haussmann, Streamline, GardenFlats],
                    &[0.5, 0.15, 0.2, 0.15],
                ),
            };
            styles[r.weighted(w)]
        };
        let mut look = Look::styled(style, floors, r);
        // Northern old towns build in brick, southern ones in stucco.
        if style == OldTown && v.region == Northern && r.chance(0.6) {
            look.material = Material::Brick;
            look.wall = pick(r, &BRICK);
        }
        if style == Cottage && v.region == Southern {
            look.roof_color = pick(r, &CLAY_TILE);
            look.wall = pick(r, &SOUTH);
        }
        look
    }

    /// The look of a building of `style`, varied by `rng`.
    pub fn styled(style: Style, floors: u32, rng: &mut Rng) -> Look {
        let r = rng;
        let glass = pick(r, &GLASS);
        let base = Look {
            style,
            material: Material::Stucco,
            wall: [230, 226, 216],
            trim: [200, 196, 188],
            accent: pick(r, &BOLD),
            roof_color: [96, 98, 102],
            base: pick(r, &GRANITE),
            glass,
            frame: [60, 62, 66],
            bay: 3.2,
            window: WindowKind::Punched,
            window_share: 0.55,
            reveal: 0.2,
            piers: Piers::None,
            floor_bands: false,
            plinth: 0.0,
            cornice: Cornice::None,
            roof: Roof::Flat,
            crown: Crown::None,
            parapet: 0.9,
            corner_tower: false,
            balconies: false,
            fire_escape: false,
            awnings: false,
            canopy: false,
            sign: false,
            water_tank: false,
            rooftop_gear: true,
            fins: false,
            pitch: 0.6,
            ridge: Ridge::Long,
            eaves: 0.5,
            chimneys: false,
            dormers: false,
        };
        let look = match style {
            Style::ArtDeco => {
                let stone = r.chance(0.7);
                Look {
                    material: if stone {
                        Material::Limestone
                    } else {
                        Material::Terracotta
                    },
                    wall: if stone {
                        pick(r, &LIMESTONE)
                    } else {
                        pick(r, &TERRACOTTA)
                    },
                    trim: pick(r, &GOLD),
                    frame: [48, 44, 40],
                    bay: r.range(2.8, 3.4),
                    window_share: r.range(0.5, 0.6),
                    reveal: 0.3,
                    piers: Piers::Full,
                    plinth: r.range(1.2, 2.4),
                    cornice: Cornice::Stepped,
                    crown: if floors >= 14 {
                        *r.pick(&[
                            Crown::Spire,
                            Crown::Spire,
                            Crown::Stepped,
                            Crown::Mast,
                            Crown::Fins,
                        ])
                    } else {
                        Crown::Stepped
                    },
                    canopy: true,
                    sign: r.chance(0.2),
                    ..base
                }
            }
            Style::Streamline => Look {
                material: Material::Stucco,
                wall: pick(r, &PASTELS),
                trim: [250, 250, 246],
                frame: [70, 76, 84],
                bay: r.range(3.4, 4.2),
                window: WindowKind::Ribbon,
                window_share: r.range(0.78, 0.9),
                reveal: 0.12,
                floor_bands: true,
                cornice: Cornice::Simple,
                corner_tower: true,
                canopy: true,
                awnings: r.chance(0.4),
                sign: r.chance(0.5),
                fins: r.chance(0.3),
                parapet: 1.2,
                ..base
            },
            Style::Skyline => Look {
                material: Material::Glass,
                wall: pick(r, &METAL),
                trim: pick(r, &METAL),
                frame: [180, 188, 196],
                bay: r.range(1.6, 2.0),
                window: WindowKind::Curtain,
                window_share: 0.92,
                reveal: 0.05,
                floor_bands: true,
                piers: Piers::Pilasters,
                crown: *r.pick(&[Crown::Mast, Crown::Fins, Crown::Stepped, Crown::Spire]),
                canopy: true,
                parapet: 1.4,
                ..base
            },
            Style::Civic => Look {
                material: Material::Limestone,
                wall: pick(r, &LIMESTONE),
                trim: [244, 240, 228],
                frame: [52, 48, 44],
                bay: r.range(3.6, 4.4),
                window: WindowKind::Arched,
                window_share: 0.5,
                reveal: 0.35,
                piers: Piers::Full,
                plinth: r.range(2.0, 3.0),
                cornice: Cornice::Stepped,
                crown: *r.pick(&[Crown::Dome, Crown::Dome, Crown::Stepped]),
                canopy: false,
                ..base
            },
            Style::Emporium => Look {
                material: Material::Brick,
                wall: pick(r, &BRICK),
                trim: pick(r, &LIMESTONE),
                frame: *r.pick(&[[40, 60, 52], [36, 36, 40], [96, 40, 36], [232, 228, 216]]),
                bay: r.range(2.6, 3.2),
                window: *r.pick(&[WindowKind::Punched, WindowKind::Punched, WindowKind::Arched]),
                window_share: r.range(0.45, 0.55),
                reveal: 0.22,
                piers: Piers::Pilasters,
                floor_bands: r.chance(0.5),
                plinth: 0.0,
                cornice: *r.pick(&[Cornice::Simple, Cornice::Stepped]),
                awnings: true,
                sign: r.chance(0.6),
                water_tank: floors >= 5 && r.chance(0.4),
                ..base
            },
            Style::GardenFlats => Look {
                material: *r.pick(&[Material::Stucco, Material::Brick]),
                wall: if r.chance(0.5) {
                    pick(r, &PASTELS)
                } else {
                    pick(r, &BRICK)
                },
                trim: [244, 242, 236],
                frame: [240, 240, 236],
                bay: r.range(3.2, 3.8),
                window_share: r.range(0.5, 0.62),
                reveal: 0.16,
                floor_bands: r.chance(0.6),
                cornice: Cornice::Simple,
                balconies: true,
                canopy: true,
                ..base
            },
            Style::Googie => Look {
                material: Material::Stucco,
                wall: pick(r, &PASTELS),
                trim: [120, 112, 104],
                frame: [230, 230, 226],
                bay: r.range(3.0, 4.0),
                window: WindowKind::Curtain,
                window_share: r.range(0.6, 0.8),
                reveal: 0.1,
                plinth: r.range(0.6, 1.0),
                roof: *r.pick(&[Roof::Swoop, Roof::Swoop, Roof::Gable, Roof::Hip]),
                parapet: 0.0,
                rooftop_gear: false,
                fins: r.chance(0.35),
                ..base
            },
            Style::Works => {
                let metal = r.chance(0.35);
                Look {
                    material: if metal {
                        Material::Metal
                    } else {
                        Material::Brick
                    },
                    wall: if metal {
                        pick(r, &METAL)
                    } else {
                        pick(r, &BRICK)
                    },
                    trim: pick(r, &CONCRETE),
                    frame: [56, 64, 60],
                    bay: r.range(4.5, 6.0),
                    window: WindowKind::Curtain,
                    window_share: r.range(0.6, 0.75),
                    reveal: 0.2,
                    piers: Piers::Pilasters,
                    roof: *r.pick(&[Roof::Sawtooth, Roof::Sawtooth, Roof::Gable, Roof::Flat]),
                    parapet: 0.6,
                    water_tank: r.chance(0.3),
                    ..base
                }
            }
            Style::Pavilion => Look {
                material: Material::Stucco,
                wall: pick(r, &PASTELS),
                trim: pick(r, &BOLD),
                frame: [236, 236, 232],
                bay: r.range(4.0, 6.0),
                window: WindowKind::Curtain,
                window_share: r.range(0.7, 0.85),
                reveal: 0.12,
                floor_bands: true,
                roof: *r.pick(&[Roof::Dome, Roof::Swoop, Roof::Flat]),
                crown: Crown::None,
                canopy: true,
                fins: true,
                rooftop_gear: false,
                ..base
            },
            Style::Tenement => Look {
                material: Material::Brick,
                wall: pick(r, &DARK_BRICK),
                trim: pick(r, &CONCRETE),
                frame: [44, 40, 38],
                glass: [44, 56, 60],
                bay: r.range(2.4, 2.9),
                window_share: r.range(0.42, 0.5),
                reveal: 0.18,
                cornice: Cornice::Simple,
                fire_escape: true,
                water_tank: r.chance(0.7),
                awnings: r.chance(0.3),
                sign: r.chance(0.3),
                ..base
            },
            Style::Lab => Look {
                material: Material::Concrete,
                wall: pick(r, &CONCRETE),
                trim: [90, 96, 92],
                frame: [70, 74, 72],
                glass: [60, 120, 96],
                accent: [120, 255, 170],
                bay: r.range(3.0, 3.6),
                window: WindowKind::Porthole,
                window_share: 0.42,
                reveal: 0.25,
                floor_bands: true,
                roof: Roof::Flat,
                crown: Crown::Dome,
                ..base
            },
            Style::OldTown => Look {
                material: Material::Stucco,
                wall: pick(r, &PLASTER),
                trim: pick(r, &LIMESTONE),
                frame: *r.pick(&[[72, 56, 44], [52, 70, 60], [232, 228, 216], [60, 64, 80]]),
                roof_color: if r.chance(0.7) {
                    pick(r, &CLAY_TILE)
                } else {
                    pick(r, &SLATE)
                },
                bay: r.range(2.3, 2.9),
                window_share: r.range(0.38, 0.46),
                reveal: 0.22,
                floor_bands: r.chance(0.3),
                plinth: r.range(0.4, 0.9),
                cornice: Cornice::Simple,
                roof: if r.chance(0.75) {
                    Roof::Gable
                } else {
                    Roof::Hip
                },
                parapet: 0.0,
                rooftop_gear: false,
                awnings: r.chance(0.2),
                pitch: r.range(0.8, 1.15),
                ridge: if r.chance(0.3) {
                    Ridge::Deep
                } else {
                    Ridge::Front
                },
                eaves: 0.35,
                chimneys: true,
                dormers: r.chance(0.4),
                ..base
            },
            Style::Burgher => Look {
                material: Material::Brick,
                wall: pick(r, &BRICK),
                trim: [236, 232, 220],
                frame: [236, 234, 226],
                roof_color: if r.chance(0.5) {
                    pick(r, &SLATE)
                } else {
                    pick(r, &CLAY_TILE)
                },
                bay: r.range(2.1, 2.5),
                window_share: r.range(0.46, 0.55),
                reveal: 0.15,
                floor_bands: r.chance(0.4),
                cornice: Cornice::None,
                roof: Roof::Gable,
                parapet: 0.0,
                rooftop_gear: false,
                pitch: r.range(1.1, 1.5),
                ridge: if r.chance(0.75) {
                    Ridge::Deep
                } else {
                    Ridge::Front
                },
                eaves: 0.2,
                chimneys: true,
                dormers: r.chance(0.3),
                ..base
            },
            Style::Haussmann => Look {
                material: Material::Limestone,
                wall: pick(r, &LIMESTONE),
                trim: [240, 236, 224],
                frame: [52, 50, 48],
                roof_color: pick(r, &ZINC),
                bay: r.range(2.6, 3.1),
                window: if r.chance(0.3) {
                    WindowKind::Arched
                } else {
                    WindowKind::Punched
                },
                window_share: r.range(0.45, 0.52),
                reveal: 0.25,
                floor_bands: true,
                plinth: r.range(0.8, 1.4),
                cornice: Cornice::Stepped,
                roof: Roof::Mansard,
                balconies: r.chance(0.8),
                awnings: r.chance(0.3),
                parapet: 0.0,
                rooftop_gear: false,
                chimneys: true,
                dormers: true,
                ..base
            },
            Style::Rowhouse => {
                let mansard = r.chance(0.35);
                Look {
                    material: Material::Brick,
                    wall: if r.chance(0.3) {
                        [120, 84, 70]
                    } else {
                        pick(r, &BRICK)
                    },
                    trim: pick(r, &LIMESTONE),
                    frame: *r.pick(&[[40, 40, 44], [236, 234, 226], [48, 66, 56]]),
                    roof_color: pick(r, &SLATE),
                    bay: r.range(2.0, 2.5),
                    window_share: r.range(0.44, 0.52),
                    reveal: 0.2,
                    plinth: r.range(0.6, 1.2),
                    cornice: Cornice::Stepped,
                    roof: if mansard { Roof::Mansard } else { Roof::Flat },
                    parapet: if mansard { 0.0 } else { 0.5 },
                    rooftop_gear: false,
                    chimneys: true,
                    dormers: mansard,
                    ..base
                }
            }
            Style::Villa => Look {
                material: Material::Stucco,
                wall: pick(r, &SOUTH),
                trim: [244, 240, 230],
                frame: *r.pick(&[[56, 96, 70], [60, 84, 120], [120, 72, 50], [236, 232, 220]]),
                roof_color: pick(r, &CLAY_TILE),
                bay: r.range(2.6, 3.2),
                window: if r.chance(0.35) {
                    WindowKind::Arched
                } else {
                    WindowKind::Punched
                },
                window_share: r.range(0.4, 0.5),
                reveal: 0.25,
                cornice: Cornice::Simple,
                roof: if r.chance(0.7) {
                    Roof::Hip
                } else {
                    Roof::Gable
                },
                balconies: r.chance(0.5),
                parapet: 0.0,
                rooftop_gear: false,
                pitch: r.range(0.32, 0.5),
                ridge: Ridge::Front,
                eaves: 0.6,
                chimneys: r.chance(0.3),
                ..base
            },
            Style::Cottage => Look {
                material: *r.pick(&[Material::Brick, Material::Stucco, Material::Stucco]),
                wall: if r.chance(0.4) {
                    pick(r, &BRICK)
                } else {
                    pick(r, &PLASTER)
                },
                trim: [244, 242, 236],
                frame: *r.pick(&[[244, 242, 236], [60, 64, 72], [56, 90, 70], [120, 40, 36]]),
                roof_color: if r.chance(0.5) {
                    pick(r, &SHINGLE)
                } else if r.chance(0.6) {
                    pick(r, &CLAY_TILE)
                } else {
                    pick(r, &SLATE)
                },
                bay: r.range(2.8, 3.6),
                window_share: r.range(0.42, 0.55),
                reveal: 0.12,
                roof: if r.chance(0.6) {
                    Roof::Gable
                } else {
                    Roof::Hip
                },
                parapet: 0.0,
                rooftop_gear: false,
                canopy: r.chance(0.5),
                pitch: r.range(0.55, 0.95),
                ridge: *r.pick(&[Ridge::Long, Ridge::Front, Ridge::Front, Ridge::Deep]),
                eaves: r.range(0.4, 0.7),
                chimneys: true,
                dormers: r.chance(0.25),
                ..base
            },
        };
        let wall = vary(r, look.wall, 0.05);
        Look { wall, ..look }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_district_gets_several_styles() {
        for d in DistrictKind::ALL {
            let mut seen = std::collections::BTreeSet::new();
            for i in 0..200u64 {
                let usage = match d {
                    DistrictKind::AetherWorks => BuildingUse::Works,
                    DistrictKind::Expo => BuildingUse::Pavilion,
                    DistrictKind::Dystopia => BuildingUse::Tenement,
                    DistrictKind::Residential => BuildingUse::Apartments,
                    DistrictKind::Commercial => BuildingUse::ShopsAndOffices,
                    DistrictKind::Civic => BuildingUse::Offices,
                };
                let floors = (i % 30) as u32 + 1;
                seen.insert(format!(
                    "{:?}",
                    Look::choose(Seed(i), d, usage, floors).style
                ));
            }
            let single = matches!(
                d,
                DistrictKind::AetherWorks | DistrictKind::Expo | DistrictKind::Dystopia
            );
            assert!(single || seen.len() >= 3, "{d:?}: {seen:?}");
        }
    }

    #[test]
    fn same_seed_same_look() {
        let a = Look::choose(Seed(5), DistrictKind::Civic, BuildingUse::Offices, 20);
        let b = Look::choose(Seed(5), DistrictKind::Civic, BuildingUse::Offices, 20);
        assert_eq!(a, b);
    }

    #[test]
    fn old_towns_and_suburbs_have_pitched_roofs() {
        for region in Region::ALL {
            for quarter in [Quarter::OldTown, Quarter::Suburb] {
                let v = Vernacular { region, quarter };
                let usage = if quarter == Quarter::Suburb {
                    BuildingUse::House
                } else {
                    BuildingUse::Apartments
                };
                let pitched = (0..200)
                    .filter(|&i| {
                        let l = Look::vernacular(Seed(i), v, DistrictKind::Residential, usage, 3);
                        matches!(l.roof, Roof::Gable | Roof::Hip | Roof::Mansard)
                    })
                    .count();
                assert!(
                    pitched > 140,
                    "{region:?} {quarter:?}: {pitched} of 200 pitched"
                );
            }
        }
    }
}
