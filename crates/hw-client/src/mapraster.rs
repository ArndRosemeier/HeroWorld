//! The town drawn as a map, in square tiles of map pixels: what lies at each
//! spot (water, park, road, building...). The minimap draws them, and the
//! soundscape reads them to hear what is around (how built up, how green,
//! how near the water).
//!
//! Tiles are made on demand round the player and dropped when far (see
//! `minimap.rs`), so a map of a megacity costs no more than a village's.
//! Pure data: no Engine here.

use glam::DVec2;
use hw_gen::urban::streets::StreetStyle;
use hw_gen::urban::world::{TownWorld, WorldRoad, WATER_LEVEL};

/// What a map pixel shows. The top bit marks the villains' quarter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Px {
    /// Open land.
    Land = 0,
    Water = 1,
    /// A park or green.
    Park = 2,
    /// Built-up land between streets (the coarser maps, which leave out
    /// single houses).
    Block = 3,
    Pavement = 4,
    SideRoad = 5,
    MainRoad = 6,
    Building = 7,
}

/// The villains' quarter bit on a pixel.
pub const VILLAINS: u8 = 0x80;

impl Px {
    pub fn of(byte: u8) -> Px {
        match byte & 0x7f {
            1 => Px::Water,
            2 => Px::Park,
            3 => Px::Block,
            4 => Px::Pavement,
            5 => Px::SideRoad,
            6 => Px::MainRoad,
            7 => Px::Building,
            _ => Px::Land,
        }
    }
}

/// Pixels along a tile's side, at every level.
pub const TILE_PX: usize = 128;

/// Metres a tile spans at each level: 1, 4 and 16 m a pixel. Level 0 shows
/// every house; level 1 streets and blocks; level 2 main roads only.
pub const LEVEL_SPAN: [f64; 3] = [128.0, 512.0, 2048.0];

/// A tile's name: its level and its place in that level's grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TileKey {
    pub level: u8,
    pub x: i32,
    pub z: i32,
}

impl TileKey {
    /// The tile at `level` that holds `p`.
    pub fn at(level: u8, p: DVec2) -> TileKey {
        let s = LEVEL_SPAN[level as usize];
        TileKey {
            level,
            x: (p.x / s).floor() as i32,
            z: (p.y / s).floor() as i32,
        }
    }

    /// Its low corner (smallest x and z).
    pub fn lo(&self) -> DVec2 {
        let s = LEVEL_SPAN[self.level as usize];
        DVec2::new(self.x as f64 * s, self.z as f64 * s)
    }

    pub fn span(&self) -> f64 {
        LEVEL_SPAN[self.level as usize]
    }
}

/// One tile of map pixels, rows along +z.
#[derive(Clone, Debug)]
pub struct MapTile {
    pub key: TileKey,
    pub px: Vec<u8>,
}

impl MapTile {
    /// The pixel at a point inside the tile.
    pub fn at(&self, p: DVec2) -> u8 {
        let k = self.key;
        let m = (k.span() / TILE_PX as f64).max(1e-9);
        let q = (p - k.lo()) / m;
        let i = (q.x.floor().max(0.0) as usize).min(TILE_PX - 1);
        let j = (q.y.floor().max(0.0) as usize).min(TILE_PX - 1);
        self.px[j * TILE_PX + i]
    }
}

/// A road as the map needs it.
struct MapRoad {
    pts: Vec<DVec2>,
    half: f64,
    pavement: f64,
    main: bool,
}

/// The buildings whose footprint reaches into `lo..hi`, as corners. Plans
/// the districts there if they are not yet (fine on the map's worker).
fn buildings_in(w: &TownWorld, lo: DVec2, hi: DVec2) -> Vec<[DVec2; 4]> {
    w.quarters_in(lo, hi)
        .iter()
        .flat_map(|q| q.buildings.iter().map(|b| b.site.corners()))
        .filter(|c| overlaps(c, lo, hi))
        .collect()
}

/// The roads that reach into `lo..hi` (main roads only if `main_only`,
/// which plans nothing).
fn roads_in(w: &TownWorld, lo: DVec2, hi: DVec2, main_only: bool) -> Vec<MapRoad> {
    let reach = |r: &WorldRoad| {
        let m = r.half + r.pavement;
        overlaps(&r.pts, lo - DVec2::splat(m), hi + DVec2::splat(m))
    };
    let to_map = |r: &WorldRoad| MapRoad {
        pts: r.pts.clone(),
        half: r.half,
        pavement: r.pavement,
        main: r.main,
    };
    if main_only {
        w.main_roads
            .iter()
            .filter(|r| reach(r))
            .map(to_map)
            .collect()
    } else {
        let m = DVec2::splat(30.0);
        w.roads_in(lo - m, hi + m)
            .iter()
            .filter(|r| reach(r))
            .map(to_map)
            .collect()
    }
}

/// Outlines of the parks that reach into `lo..hi`.
fn parks_in(w: &TownWorld, lo: DVec2, hi: DVec2) -> Vec<Vec<DVec2>> {
    w.quarters_in(lo, hi)
        .iter()
        .filter_map(|q| q.streets.as_ref())
        .filter(|s| s.plan.style == StreetStyle::Park)
        .map(|s| s.piece.face.pts.clone())
        .filter(|pts| overlaps(pts, lo, hi))
        .collect()
}

/// Does the box round `pts` meet `lo..hi`?
fn overlaps(pts: &[DVec2], lo: DVec2, hi: DVec2) -> bool {
    let (mut a, mut b) = (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN));
    for p in pts {
        a = a.min(*p);
        b = b.max(*p);
    }
    a.x <= hi.x && b.x >= lo.x && a.y <= hi.y && b.y >= lo.y
}

/// Distance from `p` to the segment `a..b`.
fn seg_dist(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// Is `p` inside the four corners (a convex quad)?
fn in_quad(c: &[DVec2; 4], p: DVec2) -> bool {
    let mut sign = 0.0;
    for k in 0..4 {
        let (a, b) = (c[k], c[(k + 1) % 4]);
        let s = (b - a).perp_dot(p - a);
        if s.abs() < 1e-12 {
            continue;
        }
        if sign == 0.0 {
            sign = s.signum();
        } else if s.signum() != sign {
            return false;
        }
    }
    true
}

/// Draws one tile. Takes a few milliseconds; run it off the main thread.
pub fn raster(w: &TownWorld, key: TileKey) -> MapTile {
    let lo = key.lo();
    let span = key.span();
    let hi = lo + DVec2::splat(span);
    let m = span / TILE_PX as f64;
    let level = key.level;
    let buildings = if level == 0 {
        buildings_in(w, lo, hi)
    } else {
        Vec::new()
    };
    let roads = roads_in(w, lo, hi, level == 2);
    // The farthest level plans no districts: main roads and town outline only.
    let parks = if level < 2 {
        parks_in(w, lo, hi)
    } else {
        Vec::new()
    };
    let blocks: Vec<&[DVec2]> = if level > 0 {
        w.town
            .pieces
            .iter()
            .map(|p| p.face.pts.as_slice())
            .filter(|pts| overlaps(pts, lo, hi))
            .collect()
    } else {
        Vec::new()
    };
    let villains = w.town.dystopia.as_ref();
    // Road segments, each with its reach on the map (roads at least a
    // pixel and a half wide, so a far side street still shows).
    let mut segs: Vec<(DVec2, DVec2, f64, f64, bool)> = Vec::new();
    for r in &roads {
        let (half, pave) = if level == 0 {
            (r.half, r.half + r.pavement)
        } else {
            let h = r.half.max(m * 0.75);
            (h, h)
        };
        for s in r.pts.windows(2) {
            segs.push((s[0], s[1], half, pave, r.main));
        }
    }
    let mut px = vec![0u8; TILE_PX * TILE_PX];
    for j in 0..TILE_PX {
        for i in 0..TILE_PX {
            let p = lo + DVec2::new((i as f64 + 0.5) * m, (j as f64 + 0.5) * m);
            let mut road: Option<Px> = None;
            for &(a, b, half, pave, main) in &segs {
                let d = seg_dist(p, a, b);
                if d < half {
                    road = Some(if main { Px::MainRoad } else { Px::SideRoad });
                    if main {
                        break;
                    }
                } else if d < pave && road.is_none() {
                    road = Some(Px::Pavement);
                }
            }
            let what = if buildings.iter().any(|c| in_quad(c, p)) {
                Px::Building
            } else if let Some(r) = road {
                r
            } else if w.ground(p) < WATER_LEVEL {
                Px::Water
            } else if parks.iter().any(|pts| hw_gen::urban::contains(pts, p)) {
                Px::Park
            } else if blocks.iter().any(|pts| hw_gen::urban::contains(pts, p)) {
                Px::Block
            } else {
                Px::Land
            };
            let mut byte = what as u8;
            if villains.is_some_and(|d| d.contains(p)) {
                byte |= VILLAINS;
            }
            px[j * TILE_PX + i] = byte;
        }
    }
    MapTile { key, px }
}

/// What lies round a spot, as shares of the ground (0..1).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Surroundings {
    pub built: f32,
    pub park: f32,
    pub water: f32,
    pub roads: f32,
    pub villains: f32,
}

/// Counts the pixels of `tiles` (level-0 tiles) on a ring of samples within
/// `r` metres of `p`. Spots no tile covers are left out.
pub fn surroundings<'a>(
    tiles: impl Fn(TileKey) -> Option<&'a MapTile>,
    p: DVec2,
    r: f64,
) -> Option<Surroundings> {
    let mut s = Surroundings::default();
    let mut n = 0.0f32;
    let step = (r / 8.0).max(2.0);
    let k = (r / step).ceil() as i32;
    for j in -k..=k {
        for i in -k..=k {
            let d = DVec2::new(i as f64 * step, j as f64 * step);
            if d.length() > r {
                continue;
            }
            let q = p + d;
            let Some(t) = tiles(TileKey::at(0, q)) else {
                continue;
            };
            let b = t.at(q);
            n += 1.0;
            match Px::of(b) {
                Px::Building | Px::Block => s.built += 1.0,
                Px::Park => s.park += 1.0,
                Px::Water => s.water += 1.0,
                Px::MainRoad | Px::SideRoad | Px::Pavement => s.roads += 1.0,
                Px::Land => {}
            }
            if b & VILLAINS != 0 {
                s.villains += 1.0;
            }
        }
    }
    (n > 0.0).then(|| Surroundings {
        built: s.built / n,
        park: s.park / n,
        water: s.water / n,
        roads: s.roads / n,
        villains: s.villains / n,
    })
}

/// A map pixel's colour (sRGB): a printed street map of the 1950s, warm
/// paper and gold main roads.
pub fn colour(byte: u8) -> [u8; 3] {
    let c = match Px::of(byte) {
        Px::Land => [226, 216, 186],
        Px::Water => [104, 164, 210],
        Px::Park => [146, 186, 104],
        Px::Block => [206, 188, 156],
        Px::Pavement => [238, 230, 212],
        Px::SideRoad => [252, 250, 242],
        Px::MainRoad => [246, 200, 96],
        Px::Building => [168, 138, 110],
    };
    if byte & VILLAINS != 0 {
        // The villains' quarter: a sickly green-grey cast.
        [
            (c[0] as u16 * 7 / 10) as u8,
            (c[1] as u16 * 8 / 10 + 20) as u8,
            (c[2] as u16 * 6 / 10) as u8,
        ]
    } else {
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn tile_keys_hold_their_points() {
        for level in 0..3u8 {
            for p in [DVec2::new(5.0, -3.0), DVec2::new(-700.0, 1300.0)] {
                let k = TileKey::at(level, p);
                let lo = k.lo();
                assert!(p.x >= lo.x && p.x < lo.x + k.span());
                assert!(p.y >= lo.y && p.y < lo.y + k.span());
            }
        }
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn quads_and_segments() {
        let c = [
            DVec2::new(0.0, 0.0),
            DVec2::new(4.0, 0.0),
            DVec2::new(4.0, 2.0),
            DVec2::new(0.0, 2.0),
        ];
        assert!(in_quad(&c, DVec2::new(1.0, 1.0)));
        assert!(!in_quad(&c, DVec2::new(5.0, 1.0)));
        let d = seg_dist(DVec2::new(1.0, 3.0), DVec2::ZERO, DVec2::new(4.0, 0.0));
        assert!((d - 3.0).abs() < 1e-9);
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn a_town_tile_shows_houses_and_streets() {
        use hw_core::Seed;
        use hw_gen::urban::TownParams;
        let w = TownWorld::generate(Seed(5), TownParams::sample(Seed(5), 20_000.0));
        // The tile round the first building, at every level.
        let core = w.town.growth.centres[0].pos;
        let q = w.quarters_in(core - DVec2::splat(150.0), core + DVec2::splat(150.0));
        let at = q
            .iter()
            .find_map(|q| q.buildings.first())
            .expect("houses round the core")
            .site
            .origin;
        let t0 = raster(&w, TileKey::at(0, at));
        let count = |t: &MapTile, k: Px| t.px.iter().filter(|b| Px::of(**b) == k).count();
        assert!(count(&t0, Px::Building) > 50, "houses on the close map");
        assert!(
            count(&t0, Px::SideRoad) + count(&t0, Px::MainRoad) > 50,
            "streets"
        );
        let t1 = raster(&w, TileKey::at(1, at));
        assert_eq!(count(&t1, Px::Building), 0, "no single houses further out");
        assert!(count(&t1, Px::Block) > 100, "blocks instead");
        let t2 = raster(&w, TileKey::at(2, at));
        assert_eq!(count(&t2, Px::SideRoad), 0, "main roads only, far out");
        // And the soundscape hears a built-up place there.
        let s = surroundings(|k| (k == t0.key).then_some(&t0), at, 20.0).expect("covered");
        assert!(s.built + s.roads > 0.3, "{s:?}");
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn the_villains_quarter_is_tinted() {
        let plain = colour(Px::Building as u8);
        let grim = colour(Px::Building as u8 | VILLAINS);
        assert_ne!(plain, grim);
        assert_eq!(Px::of(Px::Park as u8 | VILLAINS), Px::Park);
    }
}
