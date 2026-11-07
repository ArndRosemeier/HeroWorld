//! The parameters of a settlement: one set that covers every kind of place,
//! from a hamlet at a ford to a megalopolis on a bay.
//!
//! [`TownParams::sample`] draws every parameter from the seed with realistic
//! odds (organic growth is the majority; planned cities still have organic
//! parts). The planet layer will fix the setting and the population; presets
//! and tests fix whatever else they need by setting fields directly.

use hw_core::{det, Rng, Seed};

/// Where the settlement was founded, which decides its water and terrain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Site {
    /// A ford or bridge over a river.
    RiverCrossing,
    /// Where two rivers meet.
    Confluence,
    /// Where a river reaches the sea.
    Estuary,
    /// A sheltered bay.
    Bay,
    /// An open coast.
    Coast,
    /// The shore of a lake.
    LakeShore,
    /// A defensible hill.
    Hilltop,
    /// A valley or a pass between hills.
    Valley,
    /// A crossing of roads on open land.
    Crossroads,
    /// An island (in the sea or a river mouth).
    Island,
}

impl Site {
    pub const ALL: [Site; 10] = [
        Site::RiverCrossing,
        Site::Confluence,
        Site::Estuary,
        Site::Bay,
        Site::Coast,
        Site::LakeShore,
        Site::Hilltop,
        Site::Valley,
        Site::Crossroads,
        Site::Island,
    ];

    /// Does the sea reach this site?
    pub fn is_coastal(self) -> bool {
        matches!(self, Site::Estuary | Site::Bay | Site::Coast | Site::Island)
    }
}

/// How the place began.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Founding {
    /// A village that grew without a plan.
    Village,
    /// A planned colony, on a grid from the first day.
    Colony,
    /// A fortress or a walled market town.
    Fortress,
    /// A trading port.
    Port,
    /// A mill, mine or works town along its water or seam.
    MillTown,
}

/// How a planned extension is laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlanStyle {
    /// Straight streets at right angles.
    Grid,
    /// Grand axes radiating from squares and monuments.
    Baroque,
    /// Curving streets, crescents and greens.
    GardenCity,
    /// Big superblocks with towers and parkland.
    Superblock,
    /// HeroWorld's retro-future: expo axes, rings and plazas.
    RetroFuture,
}

impl PlanStyle {
    pub const ALL: [PlanStyle; 5] = [
        PlanStyle::Grid,
        PlanStyle::Baroque,
        PlanStyle::GardenCity,
        PlanStyle::Superblock,
        PlanStyle::RetroFuture,
    ];
}

/// Every parameter of a settlement.
#[derive(Clone, Debug, PartialEq)]
pub struct TownParams {
    // --- Setting (later from the planet).
    pub site: Site,
    /// 0 flat plain .. 1 mountainous.
    pub relief: f64,
    /// Rivers through the town: 0..=3. The first is the main river.
    pub rivers: u8,
    /// Width of the main river (m).
    pub river_width: f64,

    /// Directions (radians, 0 = east, towards +z = south positive) of the
    /// roads to neighbouring places. Empty: the town picks its own. The
    /// planet layer fills these so roads meet their neighbours.
    pub links: Vec<f64>,

    // --- Size (later from the city's importance on the planet).
    pub population: f64,

    // --- History.
    pub founding: Founding,
    /// Rings of former walls, each later a ring road.
    pub walls: u8,
    /// How much of the growth was laid out on purpose, 0..1.
    pub planned: f64,
    /// Relative weights of the plan styles of planned quarters.
    pub plan_styles: [f64; 5],
    /// Villages swallowed by the growing town; each keeps a small centre.
    pub villages: usize,
    /// Business districts apart from the old core (big cities only).
    pub business_districts: usize,

    // --- Growth.
    /// 0 compact .. 1 stretched out along roads, valleys and shores.
    pub sprawl: f64,

    // --- Form.
    /// People per km² of built-up land, on average.
    pub density: f64,
    /// Share of land kept green (parks, gardens, commons), 0..1.
    pub green: f64,

    // --- Theme (HeroWorld).
    /// Share of the town fallen to the villains, 0..1.
    pub dystopia: f64,
}

impl TownParams {
    /// Draws a settlement of `population` from the seed, with realistic odds.
    pub fn sample(seed: Seed, population: f64) -> TownParams {
        let mut rng = seed.child("town-params").rng();
        let pop = population.max(10.0);
        // log10: 1.7 hamlet .. 7.5 megalopolis.
        let lp = det::log10(pop);
        let big = ((lp - 3.0) / 4.0).clamp(0.0, 1.0);

        let site = Site::ALL[rng.weighted(&[
            30.0, // river crossing: the commonest reason for a town
            8.0 + 6.0 * big,
            6.0 + 8.0 * big,
            6.0 + 8.0 * big,
            7.0,
            6.0,
            5.0 * (1.0 - big * 0.6),
            7.0,
            12.0 * (1.0 - big * 0.8),
            2.0 + 2.0 * big,
        ])];
        // Hardly any town stands on a dead-flat plain; most have rolling
        // ground, some real hills.
        let relief = if rng.chance(0.05) {
            rng.range(0.0, 0.08)
        } else {
            let u = rng.unit();
            0.15 + 0.85 * u * u.sqrt()
        };
        let rivers = match site {
            Site::Confluence => 2 + rng.chance(0.15) as u8,
            Site::RiverCrossing | Site::Estuary => 1 + rng.chance(0.15 + 0.2 * big) as u8,
            Site::Valley | Site::LakeShore => rng.chance(0.75) as u8,
            Site::Island => rng.chance(0.5) as u8,
            _ => {
                if rng.chance(0.4 + 0.5 * big) {
                    1
                } else {
                    0
                }
            }
        };
        // Big towns tend to sit on big rivers, but not always.
        let river_width = 6.0 * det::powf(10.0, rng.range(0.0, 0.8) + big * rng.range(0.4, 1.0));

        let founding = [
            Founding::Village,
            Founding::Colony,
            Founding::Fortress,
            Founding::Port,
            Founding::MillTown,
        ][rng.weighted(&[
            55.0,
            12.0,
            14.0,
            if site.is_coastal() { 18.0 } else { 0.0 },
            8.0,
        ])];
        let walls = if founding == Founding::Colony {
            rng.chance(0.2) as u8
        } else {
            let p = 0.15
                + 0.5 * big
                + if founding == Founding::Fortress {
                    0.4
                } else {
                    0.0
                };
            (rng.chance(p) as u8)
                + (rng.chance(p * big * 0.6) as u8)
                + (rng.chance(p * big * 0.25) as u8)
        };
        // Mostly organic, with a long tail of planned towns.
        let planned = match founding {
            Founding::Colony => rng.range(0.55, 0.95),
            _ => (rng.unit() * rng.unit() * 0.9 + big * rng.range(0.0, 0.25)).min(0.9),
        };
        let mut plan_styles = [
            rng.range(1.0, 4.0),
            rng.range(0.0, 2.0) * big,
            rng.range(0.0, 2.0),
            rng.range(0.0, 1.5) * big,
            rng.range(0.5, 2.0),
        ];
        if founding == Founding::Colony {
            plan_styles[0] += 4.0;
        }
        let villages = if pop < 3000.0 {
            0
        } else {
            ((lp - 3.4) * rng.range(1.5, 4.5)).round().max(0.0) as usize
        };
        let business_districts = if pop < 300_000.0 {
            0
        } else {
            ((lp - 5.4) * rng.range(1.0, 3.0)).round().max(0.0) as usize
        };
        let sprawl = rng.range(0.1, 0.9);
        // Hamlets are loose, old cores tight, megacities somewhere between.
        let density = det::powf(10.0, rng.range(3.2, 3.9) + big * rng.range(-0.1, 0.25));
        let green = rng.range(0.05, 0.3);
        let dystopia = rng.range(0.05, 0.25) * (0.3 + big);
        TownParams {
            site,
            relief,
            rivers,
            river_width,
            links: Vec::new(),
            population: pop,
            founding,
            walls,
            planned,
            plan_styles,
            villages,
            business_districts,
            sprawl,
            density,
            green,
            dystopia,
        }
    }

    /// Built-up area (m²) the population needs.
    pub fn built_area(&self) -> f64 {
        self.population / self.density * 1e6
    }

    /// Radius (m) of a disc of the built-up area: a scale for everything
    /// else, never the town's outline.
    pub fn scale(&self) -> f64 {
        (self.built_area() / std::f64::consts::PI).sqrt().max(60.0)
    }
}

/// Named points in the parameter space, for tests and special places.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    MedievalHillTown,
    ColonialGridPort,
    RiverValleyMillTown,
    BaroqueCapital,
    GardenCity,
    SprawlingMegalopolis,
}

impl Preset {
    pub const ALL: [Preset; 6] = [
        Preset::MedievalHillTown,
        Preset::ColonialGridPort,
        Preset::RiverValleyMillTown,
        Preset::BaroqueCapital,
        Preset::GardenCity,
        Preset::SprawlingMegalopolis,
    ];

    /// The preset's parameters, the rest sampled from the seed.
    pub fn params(self, seed: Seed) -> TownParams {
        let mut rng: Rng = seed.child("preset").rng();
        let (pop, f): (f64, fn(&mut TownParams, &mut Rng)) = match self {
            Preset::MedievalHillTown => (12_000.0, |p, r| {
                p.site = Site::Hilltop;
                p.relief = r.range(0.5, 0.9);
                p.founding = Founding::Fortress;
                p.walls = 1 + r.chance(0.4) as u8;
                p.planned = r.range(0.0, 0.1);
            }),
            Preset::ColonialGridPort => (400_000.0, |p, r| {
                p.site = Site::Bay;
                p.relief = r.range(0.0, 0.3);
                p.founding = Founding::Colony;
                p.planned = r.range(0.7, 0.9);
                p.plan_styles = [6.0, 0.5, 0.5, 0.5, 1.0];
                p.walls = 0;
            }),
            Preset::RiverValleyMillTown => (40_000.0, |p, r| {
                p.site = Site::Valley;
                p.relief = r.range(0.5, 0.8);
                p.rivers = 1;
                p.founding = Founding::MillTown;
                p.sprawl = r.range(0.7, 0.9);
                p.planned = r.range(0.1, 0.3);
            }),
            Preset::BaroqueCapital => (2_000_000.0, |p, r| {
                p.site = Site::RiverCrossing;
                p.rivers = 1;
                p.relief = r.range(0.1, 0.4);
                p.founding = Founding::Fortress;
                p.walls = 2;
                p.planned = r.range(0.35, 0.55);
                p.plan_styles = [1.0, 5.0, 1.0, 0.5, 1.0];
            }),
            Preset::GardenCity => (60_000.0, |p, r| {
                p.site = Site::Crossroads;
                p.relief = r.range(0.1, 0.4);
                p.founding = Founding::Village;
                p.planned = r.range(0.5, 0.8);
                p.plan_styles = [1.0, 0.5, 6.0, 0.0, 1.0];
                p.green = r.range(0.3, 0.45);
                p.density = r.range(1500.0, 2500.0);
            }),
            Preset::SprawlingMegalopolis => (15_000_000.0, |p, r| {
                p.site = Site::Estuary;
                p.rivers = 2;
                p.sprawl = r.range(0.75, 0.95);
                p.villages = r.range_int(20, 35) as usize;
                p.business_districts = r.range_int(3, 6) as usize;
            }),
        };
        let mut p = TownParams::sample(seed, pop);
        f(&mut p, &mut rng);
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampled_towns_vary_and_lean_organic() {
        let mut planned = 0;
        let mut sites = std::collections::BTreeSet::new();
        for s in 0..400 {
            let pop = det::powf(10.0, 2.0 + (s % 12) as f64 * 0.5);
            let p = TownParams::sample(Seed(s), pop);
            planned += (p.planned > 0.5) as u32;
            sites.insert(format!("{:?}", p.site));
            assert!(p.scale() > 50.0);
        }
        assert!(
            (20..150).contains(&planned),
            "{planned} of 400 mostly planned"
        );
        assert_eq!(sites.len(), Site::ALL.len());
        assert_eq!(
            TownParams::sample(Seed(7), 1e5),
            TownParams::sample(Seed(7), 1e5)
        );
        let flat = (0..400)
            .filter(|s| TownParams::sample(Seed(*s), 1e5).relief < 0.1)
            .count();
        assert!(flat < 40, "{flat} of 400 flat");
    }

    #[test]
    fn size_follows_population() {
        let a = TownParams::sample(Seed(1), 100.0).scale();
        let b = TownParams::sample(Seed(1), 1e7).scale();
        assert!(a < 400.0 && b > 10_000.0, "{a} {b}");
    }
}
