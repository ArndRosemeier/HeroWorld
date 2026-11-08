//! What a building's outside is made of: the textures of its walls, base,
//! trim, frames, accents and roof, and how worn and dirty it is, from its
//! style, region, quarter, district and seed.
//!
//! Old towns are worn, the planned terraces aged, the suburbs and the
//! golden-age quarters kept fresh; the works are sooty and the dystopia
//! filthy. Southern stucco bleaches; northern brick blackens.

use engine::prelude::{rgb, Color};
use engine::texgen::{FacadeLayer as F, MedievalLayer as M, RoofLayer as R};
use hw_gen::building::BuildingPlan;
use hw_gen::city::DistrictKind;
use hw_gen::style::{Material, Quarter, Region, Roof, Style};

use crate::skins::{Base, Skin, Tex, Wear};

/// The textures of one building.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Facade {
    pub wall: Tex,
    /// The stone base course and steps.
    pub base: Tex,
    /// Cornices, bands, pilasters, sills and surrounds.
    pub trim: Tex,
    /// Window and door frames.
    pub frame: Tex,
    /// Signs, canopies, awnings, fins.
    pub accent: Tex,
    /// Pitched and mansard roofs.
    pub roof: Tex,
    /// A flat roof's covering and its colour.
    pub flat_roof: (Tex, Color),
    /// Half-timbering: beams on the walls, their texture and colour.
    pub timber: Option<(Tex, Color)>,
    pub wear: Wear,
    /// Street dirt and streaks, 0 to 1 (see `citymesh::Weather`).
    pub dirt: f32,
}

fn skin(base: Base, wear: Wear) -> Tex {
    Skin::new(base, wear).into()
}

/// How worn a building is.
pub fn wear_of(plan: &BuildingPlan) -> Wear {
    let site = &plan.site;
    let style = plan.look.style;
    let mut r = site.seed.child("wear").rng();
    let base = match (site.district, style) {
        (DistrictKind::Dystopia, _) | (_, Style::Tenement | Style::Lab) => Wear::Grimy,
        (DistrictKind::AetherWorks, _) | (_, Style::Works) => Wear::Worn,
        _ => match site.vernacular.map(|v| v.quarter) {
            Some(Quarter::OldTown) => Wear::Worn,
            Some(Quarter::Terraces) => Wear::Aged,
            Some(Quarter::Suburb) => Wear::Aged,
            // The golden-age city is new and kept.
            None => Wear::Fresh,
        },
    };
    // One in three a step cleaner (restored, repainted) or dirtier.
    let roll = r.unit();
    let step = if roll < 0.2 {
        -1
    } else if roll > 0.8 {
        1
    } else {
        0
    };
    let w = base.step(step);
    // The dystopia never gets clean, the golden age never filthy.
    // Only soot and neglect make filth: towns stop at worn.
    match (base, w) {
        (Wear::Grimy, w) => w.max(Wear::Worn),
        (Wear::Worn, w) if site.district != DistrictKind::AetherWorks && style != Style::Works => {
            w.min(Wear::Worn)
        }
        (Wear::Fresh, w) => w.min(Wear::Aged),
        (_, w) => w,
    }
}

/// The wall texture for a building's material and style.
fn wall_base(plan: &BuildingPlan) -> Base {
    let look = &plan.look;
    let site = &plan.site;
    let region = site.vernacular.map(|v| v.region);
    let [r, g, b] = look.wall.map(|v| v as f64);
    let roll = site.seed.child("wall skin").unit();
    match look.material {
        Material::Limestone => match (look.style, roll) {
            (Style::Civic | Style::ArtDeco, _) => Base::Facade(F::Limestone),
            (_, x) if x < 0.3 => Base::Facade(F::Sandstone),
            _ => Base::Facade(F::Limestone),
        },
        Material::Granite => Base::Facade(F::Granite),
        Material::Brick => {
            if r + g + b < 300.0 {
                Base::Facade(F::BrickBrown)
            } else if g > r * 0.75 {
                Base::Facade(F::BrickYellow)
            } else if look.style == Style::Burgher && roll < 0.35 {
                // Old hand-made brick in the old merchants' houses.
                Base::Medieval(M::Brick)
            } else if look.style == Style::Emporium && roll < 0.25 {
                Base::Facade(F::BrickWhite)
            } else {
                Base::Facade(F::BrickRed)
            }
        }
        Material::Terracotta => Base::Facade(F::Sandstone),
        Material::Stucco => match (look.style, region) {
            // Half-timbering and limewash need their beams and courses to
            // follow the storeys, which a repeating tile cannot do: plaster.
            (Style::OldTown, _) => Base::Facade(F::Plaster),
            // (Half-timbered houses are plaster too, with beams on top: see
            // `timber_of`.)
            // Clapboard family houses in the north.
            (Style::Cottage, Some(Region::Northern)) if roll < 0.5 => Base::Facade(F::WoodSiding),
            (Style::Cottage, _) if roll < 0.2 => Base::Facade(F::WoodSiding),
            (Style::Haussmann | Style::Civic, _) => Base::Facade(F::Plaster),
            _ => Base::Facade(F::Stucco),
        },
        Material::Concrete => match look.style {
            Style::GardenFlats | Style::Skyline => Base::Facade(F::ConcretePanel),
            _ => Base::Facade(F::Concrete),
        },
        Material::Metal | Material::Glass => match look.style {
            Style::Works => Base::Facade(F::MetalPanel),
            Style::Skyline if roll < 0.4 => Base::Facade(F::MetalPanel),
            // Porcelain enamel: the cladding of the golden age.
            _ => Base::Enamel,
        },
    }
}

/// Half-timbering for some old-town houses of the north and the middle:
/// dark oak mostly, ox-blood red or soot black now and then.
fn timber_of(plan: &BuildingPlan, wear: Wear) -> Option<(Tex, Color)> {
    let region = plan.site.vernacular.map(|v| v.region)?;
    let roll = plan.site.seed.child("timber").unit();
    let share = match region {
        Region::Northern => 0.4,
        Region::Continental => 0.3,
        Region::Southern => 0.0,
    };
    if plan.look.style != Style::OldTown || plan.look.material != Material::Stucco || roll >= share
    {
        return None;
    }
    let paint = plan.site.seed.child("timber paint").unit();
    let color = if paint < 0.65 {
        rgb(74, 52, 36)
    } else if paint < 0.85 {
        rgb(112, 44, 34)
    } else {
        rgb(44, 42, 40)
    };
    Some((skin(Base::Medieval(M::Oak), wear), color))
}

/// The base course: the region's stone.
fn base_base(plan: &BuildingPlan, wall: Base) -> Base {
    let region = plan.site.vernacular.map(|v| v.region);
    match (plan.look.style, region) {
        (Style::OldTown | Style::Cottage, Some(Region::Northern)) => Base::Medieval(M::Rubble),
        (Style::Googie | Style::Pavilion | Style::Streamline, _) => Base::Facade(F::Granite),
        (Style::Tenement | Style::Works | Style::Lab, _) => Base::Facade(F::Concrete),
        _ if wall == Base::Facade(F::Granite) => Base::Facade(F::Granite),
        (_, Some(Region::Southern)) => Base::Facade(F::Sandstone),
        _ => Base::Facade(F::Limestone),
    }
}

/// Cornices, bands and surrounds.
fn trim_base(plan: &BuildingPlan) -> Base {
    match plan.look.style {
        Style::Googie | Style::Pavilion | Style::Streamline | Style::Skyline => Base::Enamel,
        Style::Cottage => Base::PaintedWood,
        Style::Works | Style::Lab | Style::GardenFlats => Base::Facade(F::Concrete),
        _ => Base::CastStone,
    }
}

/// Window and door frames.
fn frame_base(plan: &BuildingPlan) -> Base {
    match plan.look.style {
        Style::OldTown
        | Style::Burgher
        | Style::Cottage
        | Style::Rowhouse
        | Style::Villa
        | Style::Haussmann
        | Style::Tenement => Base::PaintedWood,
        _ => Base::Enamel,
    }
}

/// The covering of pitched and mansard roofs.
fn roof_base(plan: &BuildingPlan) -> Base {
    let look = &plan.look;
    let [r, g, _] = look.roof_color.map(|v| v as f64);
    let roll = plan.site.seed.child("roof skin").unit();
    let region = plan.site.vernacular.map(|v| v.region);
    if r > g + 40.0 {
        Base::Roof(R::ClayTile)
    } else if look.roof == Roof::Mansard && look.style == Style::Haussmann {
        Base::Roof(R::Zinc)
    } else if look.style == Style::Works {
        Base::Roof(R::Corrugated)
    } else if look.style == Style::Cottage {
        if region == Some(Region::Northern) && roll < 0.15 {
            Base::Medieval(M::Thatch)
        } else if roll < 0.3 {
            Base::Medieval(M::Shingle)
        } else {
            Base::Roof(R::AsphaltShingle)
        }
    } else {
        Base::Roof(R::Slate)
    }
}

/// The covering of a flat roof, and its colour.
fn flat_roof(plan: &BuildingPlan) -> (Base, Color) {
    let roll = plan.site.seed.child("flat roof").unit();
    match plan.look.style {
        Style::GardenFlats => (Base::Roof(R::GreenRoof), rgb(150, 160, 120)),
        Style::Works | Style::Tenement | Style::Lab => (Base::Roof(R::Tar), grey(95)),
        _ if roll < 0.5 => (Base::Roof(R::GravelRoof), grey(150)),
        _ => (Base::Roof(R::Tar), grey(110)),
    }
}

fn grey(v: u8) -> Color {
    rgb(v, v, v)
}

/// Everything at once.
pub fn facade(plan: &BuildingPlan) -> Facade {
    let wear = wear_of(plan);
    let wall = wall_base(plan);
    let base = base_base(plan, wall);
    let trim = trim_base(plan);
    let frame = frame_base(plan);
    let (flat, flat_c) = flat_roof(plan);
    // Southern sun bleaches: a little less dirt there.
    let region = plan.site.vernacular.map(|v| v.region);
    let sun = if region == Some(Region::Southern) {
        0.8
    } else {
        1.0
    };
    let dirt = match wear {
        Wear::Fresh => 0.12,
        Wear::Aged => 0.3,
        Wear::Worn => 0.55,
        Wear::Grimy => 0.85,
    } * sun;
    Facade {
        wall: skin(wall, wear),
        base: skin(base, wear),
        // Trim and frames are painted and mended more often.
        trim: skin(trim, wear.step(-1)),
        frame: skin(frame, wear.step(-1)),
        accent: skin(Base::Enamel, wear.step(-1)),
        roof: skin(roof_base(plan), wear),
        flat_roof: (skin(flat, wear), flat_c),
        timber: timber_of(plan, wear),
        wear,
        dirt,
    }
}
