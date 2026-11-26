//! Where a case's steps play: every kind of place the town has that a
//! gang can hold, and how to find one.
//!
//! **Hook for new places.** A new kind of place (a park, a rooftop, the
//! harbour, a train yard) is one [`Place`] value, one finder function and
//! one line in [`SOURCES`]. The finder picks one place of its kind with
//! the random numbers it is given and says where the gang stands
//! ([`Site::at`]), which way it faces, a few more spots of open floor
//! round it ([`Site::around`], for side groups, clues and hostages), its
//! name, and for a place under the ground the way down from the street.
//! Then give it a weight in the gangs' [`super::Gang::haunts`] and say in
//! [`super::CaseKind::fits`] which jobs it takes. Nothing else changes:
//! the steps, the radio, the case file and saving work from [`Site`].
//!
//! Finders run on the case planner's worker thread, never on the server's
//! tick, and only read the town (the same town on every machine), so the
//! same seed gives the same cases.

use crate::floornav::Floors;
use glam::{DVec2, DVec3};
use hw_core::Rng;
use hw_gen::urban::area::Area;
use hw_gen::urban::deep::{DeepTile, EntranceKind, GrottoKind};
use hw_gen::urban::landmarks::{Landmark, LandmarkKind};
use hw_gen::urban::underground::{Void, VoidKind};
use hw_gen::urban::world::TownWorld;
use std::sync::Arc;

/// What kind of place a step plays in (its code goes on the wire and
/// picks the words for it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Place {
    /// A building with a floor plan (the Comet buildings, and every
    /// building converted to one): its rooms, floor after floor.
    Building,
    /// The pavement of a street.
    Street,
    /// A landmark's paved plaza.
    Plaza,
    /// A utility tunnel under a main road.
    Tunnel,
    /// An Aether Tube station's hall.
    Station,
    /// A cave chamber of the Deeps.
    Grotto,
    /// The buried city.
    Ruin,
    /// The Doctor's own lab.
    Lair,
    /// A flat roof over the streets.
    Rooftop,
    /// The pavement in front of a shop's door.
    Shopfront,
    /// The air over a main road (a getaway car's way).
    Road,
}

impl Place {
    pub const ALL: [Place; 11] = [
        Place::Building,
        Place::Street,
        Place::Plaza,
        Place::Tunnel,
        Place::Station,
        Place::Grotto,
        Place::Ruin,
        Place::Lair,
        Place::Rooftop,
        Place::Shopfront,
        Place::Road,
    ];

    pub fn code(self) -> u8 {
        Place::ALL.iter().position(|p| *p == self).unwrap_or(0) as u8
    }

    pub fn from_code(c: u8) -> Self {
        Place::ALL
            .get(c as usize)
            .copied()
            .unwrap_or(Place::Building)
    }

    /// What goes before its name: on a street, in a tunnel, at a plaza.
    pub fn preposition(self) -> &'static str {
        match self {
            Place::Street | Place::Rooftop => "on",
            Place::Road => "over",
            Place::Tunnel | Place::Grotto | Place::Ruin | Place::Lair => "in",
            Place::Building | Place::Plaza | Place::Station | Place::Shopfront => "at",
        }
    }

    /// Below the street: the radio leads to the way down first.
    pub fn underground(self) -> bool {
        matches!(
            self,
            Place::Tunnel | Place::Station | Place::Grotto | Place::Ruin | Place::Lair
        )
    }
}

/// One place a step plays in. Its name reads after "at", "on" or "in"
/// (see [`Place::preposition`]): "the Daily Comet building", "Juniper
/// Avenue", "the crystal grotto".
#[derive(Clone, Debug, PartialEq)]
pub struct Site {
    pub place: Place,
    pub name: String,
    /// Where the gang stands (feet); a building's way in.
    pub at: DVec3,
    /// The way it faces (where heroes come from).
    pub facing: DVec2,
    /// Under the ground: the way down from the street (feet, at the top).
    pub way_down: Option<DVec3>,
    /// More open floor round it, nearest the gang first.
    pub around: Vec<DVec3>,
    /// A building with a floor plan, by index in [`Floors::buildings`].
    pub building: Option<usize>,
}

/// Open cases this near each other share a place (m).
pub const SAME_PLACE: f64 = 60.0;

impl Site {
    /// Would a case here get in the way of one at `other`?
    pub fn clashes(&self, other: &Site) -> bool {
        match (self.building, other.building) {
            (Some(a), Some(b)) => a == b,
            _ => self.at.distance(other.at) < SAME_PLACE,
        }
    }
}

/// What the finders may look at.
#[derive(Clone, Default)]
pub struct Places {
    pub town: Option<Arc<TownWorld>>,
    /// The streets round the start.
    pub area: Option<Arc<Area>>,
    pub floors: Arc<Floors>,
    /// The buildings' names, by index.
    pub names: Vec<String>,
    /// The hideouts' indices in `floors`, in the order crimes use them.
    pub hideouts: Vec<usize>,
    /// How many rooms a gang can hold in each building, by index.
    pub rooms: Vec<u8>,
    /// No case on the street this near these (Hero Hall, where heroes
    /// appear).
    pub keep_clear: Vec<DVec2>,
    /// The streets of `area` as people walk them (a courier's way to a
    /// hideout, a getaway car's way out), made with the area.
    pub walks: Option<Arc<crate::streets::Streets>>,
}

/// Open-air cases keep this far from [`Places::keep_clear`] (m).
pub const KEEP_CLEAR: f64 = 180.0;

impl Places {
    /// The way on foot from `from` to `to` by the pavements of the
    /// streets round the heroes: points on open pavement, each on ground
    /// that goes on from the last (never down a bank or off a bridge).
    pub fn walk(&self, from: DVec3, to: DVec3) -> Option<Vec<DVec3>> {
        let area = self.area.as_ref()?;
        let walks = self.walks.as_ref()?;
        let (a, b) = (DVec2::new(from.x, from.z), DVec2::new(to.x, to.z));
        let route = walks.pavement_route(a, b)?;
        let mut way: Vec<DVec3> = Vec::new();
        let mut last = from;
        for p in route {
            if p.distance(a) < 8.0 || p.distance(b) < 8.0 || !area.contains(p) {
                continue;
            }
            let at = DVec3::new(p.x, area.ground(p), p.y);
            let run = DVec2::new(at.x - last.x, at.z - last.z).length().max(1.0);
            // Steeper than stairs: not the pavement (a bank, a quay).
            if (at.y - last.y).abs() > run * 0.7 + 0.5 {
                continue;
            }
            last = at;
            way.push(at);
        }
        // The streets must come near both ends, and not go the long way round.
        let ends = [(way.first(), from), (way.last(), to)];
        if ends
            .iter()
            .any(|(p, e)| p.is_none_or(|p| p.distance(*e) > 60.0))
        {
            return None;
        }
        let along: f64 = std::iter::once(from)
            .chain(way.iter().copied())
            .chain(std::iter::once(to))
            .collect::<Vec<_>>()
            .windows(2)
            .map(|w| w[0].distance(w[1]))
            .sum();
        (along < a.distance(b) * 2.2 + 80.0).then_some(way)
    }

    fn clear(&self, p: DVec2) -> bool {
        self.keep_clear.iter().all(|k| k.distance(p) > KEEP_CLEAR)
    }
}

/// One kind of place and its finder.
pub struct Source {
    pub place: Place,
    /// One place of this kind, picked with `rng`; None when the town has
    /// none (or none fit).
    pub find: fn(&Places, &mut Rng) -> Option<Site>,
}

/// Every kind of place cases use. Add new ones here (see the module
/// notes).
pub const SOURCES: &[Source] = &[
    Source {
        place: Place::Building,
        find: building,
    },
    Source {
        place: Place::Street,
        find: street,
    },
    Source {
        place: Place::Plaza,
        find: plaza,
    },
    Source {
        place: Place::Tunnel,
        find: tunnel,
    },
    Source {
        place: Place::Station,
        find: station,
    },
    Source {
        place: Place::Grotto,
        find: grotto,
    },
    Source {
        place: Place::Ruin,
        find: ruin,
    },
    Source {
        place: Place::Lair,
        find: lair,
    },
    Source {
        place: Place::Rooftop,
        find: rooftop,
    },
    Source {
        place: Place::Shopfront,
        find: shopfront,
    },
    Source {
        place: Place::Road,
        find: road,
    },
];

/// A place of kind `place`, if the town has one.
pub fn find(places: &Places, place: Place, rng: &mut Rng) -> Option<Site> {
    SOURCES
        .iter()
        .find(|s| s.place == place)
        .and_then(|s| (s.find)(places, rng))
}

/// A building with a floor plan.
fn building(places: &Places, rng: &mut Rng) -> Option<Site> {
    let n = places.floors.buildings.len();
    if n == 0 {
        return None;
    }
    let k = rng.index(n);
    Some(building_site(places, k))
}

/// Building `k` as a site.
pub fn building_site(places: &Places, k: usize) -> Site {
    let b = &places.floors.buildings[k];
    Site {
        place: Place::Building,
        name: format!(
            "the {}",
            places
                .names
                .get(k)
                .map_or("office building", |n| n.as_str())
        ),
        at: b.to_town(b.graph.front()),
        facing: b.dir,
        way_down: None,
        around: Vec::new(),
        building: Some(k),
    }
}

/// What a hideout building is called: a tenement as it is, a shop-and-office block as one of a few shabby kinds (by its seed).
pub fn hideout_noun(site: &hw_gen::building::BuildingSite) -> &'static str {
    use hw_gen::building::BuildingUse::*;
    match site.usage {
        Tenement => "tenement",
        _ => {
            const KINDS: [&str; 5] = [
                "shuttered shop block",
                "derelict office block",
                "closed-down arcade",
                "old hotel",
                "boarded-up emporium",
            ];
            KINDS[site.seed.child("noun").rng().index(KINDS.len())]
        }
    }
}

/// The name of the street nearest `p` in `area`.
pub fn street_near(area: &Area, p: DVec2) -> String {
    area.roads
        .iter()
        .filter(|r| r.pts.len() >= 2)
        .min_by(|a, b| {
            let d = |r: &hw_gen::urban::world::WorldRoad| {
                r.pts
                    .windows(2)
                    .map(|w| hw_gen::water::segment_distance(p, w[0], w[1]))
                    .fold(f64::MAX, f64::min)
            };
            d(a).total_cmp(&d(b))
        })
        .map_or_else(|| street_name(p), |r| street_name(r.pts[0]))
}

const STREET_FIRST: [&str; 16] = [
    "Juniper", "Orbit", "Chrome", "Maple", "Atlas", "Comet", "Larkspur", "Beacon", "Sputnik",
    "Willow", "Halcyon", "Crescent", "Copper", "Meridian", "Rocket", "Sycamore",
];
const STREET_LAST: [&str; 6] = ["Street", "Avenue", "Boulevard", "Lane", "Parkway", "Drive"];

/// A street's name, made up from where it starts (the town's maps carry
/// none, and every machine makes the same one).
pub fn street_name(start: DVec2) -> String {
    let h = (start.x.round() as i64)
        .wrapping_mul(73_856_093)
        .wrapping_add((start.y.round() as i64).wrapping_mul(19_349_663));
    let h = hw_core::Seed(h as u64).child("street").rng().next_u64();
    format!(
        "{} {}",
        STREET_FIRST[(h % 16) as usize],
        STREET_LAST[((h >> 8) % 6) as usize]
    )
}

/// The pavement of a street near the start, clear of Hero Hall.
fn street(places: &Places, rng: &mut Rng) -> Option<Site> {
    let area = places.area.as_ref()?;
    let roads: Vec<_> = area
        .roads
        .iter()
        .filter(|r| r.pts.len() >= 2 && r.pavement >= 1.0)
        .collect();
    if roads.is_empty() {
        return None;
    }
    for _ in 0..16 {
        let r = roads[rng.index(roads.len())];
        let i = rng.index(r.pts.len() - 1);
        let (a, b) = (r.pts[i], r.pts[i + 1]);
        let len = a.distance(b);
        if len < 40.0 {
            continue;
        }
        let d = (b - a) / len;
        let side = if rng.chance(0.5) { d.perp() } else { -d.perp() };
        let kerb = r.half + r.pavement * 0.5;
        let mid = a + d * len * rng.range(0.35, 0.65);
        let at = mid + side * kerb;
        if !places.clear(at) || !area.contains(at) {
            continue;
        }
        let y = |p: DVec2| DVec3::new(p.x, area.ground(p), p.y);
        let around = [9.0, -9.0, 18.0, -18.0, 26.0, -26.0]
            .iter()
            .map(|&s| y(at + d * s))
            .collect();
        return Some(Site {
            place: Place::Street,
            name: street_name(r.pts[0]),
            at: y(at),
            facing: -side,
            way_down: None,
            around,
            building: None,
        });
    }
    None
}

/// What the town calls a landmark (for its plaza).
pub fn landmark_name(k: LandmarkKind) -> Option<&'static str> {
    Some(match k {
        LandmarkKind::AetherSpire => "Aether Spire",
        LandmarkKind::Lighthouse => "lighthouse",
        LandmarkKind::TeslaTower => "Tesla tower",
        LandmarkKind::MooringMast => "airship mast",
        LandmarkKind::RocketGantry => "rocket port",
        LandmarkKind::AtomicPlant => "atomic plant",
        LandmarkKind::MoleDrill => "mole drill",
        LandmarkKind::WeatherTower => "Weather Bureau tower",
        LandmarkKind::RobotWorks => "robot works",
        LandmarkKind::HeroHall | LandmarkKind::Hospital | LandmarkKind::CometBuilding => {
            return None
        }
    })
}

/// A landmark's plaza.
fn plaza(places: &Places, rng: &mut Rng) -> Option<Site> {
    let town = places.town.as_ref()?;
    let list: Vec<&Landmark> = town
        .landmarks
        .iter()
        .filter(|l| landmark_name(l.kind).is_some() && l.plaza >= 14.0 && places.clear(l.pos))
        .collect();
    if list.is_empty() {
        return None;
    }
    let l = list[rng.index(list.len())];
    let r = l.plaza * 0.72;
    let on = |a: f64| {
        let d = hw_core::det::dir2(a.to_radians()).rotate(l.dir);
        let p = l.pos + d * r;
        DVec3::new(p.x, l.base, p.y)
    };
    Some(Site {
        place: Place::Plaza,
        name: format!("the {}'s plaza", landmark_name(l.kind)?),
        at: on(0.0),
        facing: l.dir,
        way_down: None,
        around: [35.0, -35.0, 70.0, -70.0, 105.0, -105.0]
            .iter()
            .map(|&a| on(a))
            .collect(),
        building: None,
    })
}

/// Points of open floor spread over a void, two metres in from its walls.
fn void_points(v: &Void, n: usize) -> Vec<DVec3> {
    let (lo, hi) = (v.min + DVec2::splat(2.0), v.max - DVec2::splat(2.0));
    if hi.x <= lo.x || hi.y <= lo.y {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (fx, fy) in [
        (0.2, 0.25),
        (0.8, 0.75),
        (0.8, 0.25),
        (0.2, 0.75),
        (0.5, 0.15),
        (0.5, 0.85),
        (0.05, 0.5),
        (0.95, 0.5),
    ] {
        let l = DVec2::new(lo.x + (hi.x - lo.x) * fx, lo.y + (hi.y - lo.y) * fy);
        let p = v.world(l);
        out.push(DVec3::new(p.x, v.floor_at(p), p.y));
        if out.len() == n {
            break;
        }
    }
    out
}

/// A void's middle, on its floor.
fn middle(v: &Void) -> DVec3 {
    let p = v.world((v.min + v.max) * 0.5);
    DVec3::new(p.x, v.floor_at(p), p.y)
}

/// A utility tunnel under a main road, the way down nearest it.
fn tunnel(places: &Places, rng: &mut Rng) -> Option<Site> {
    let town = places.town.as_ref()?;
    let under = town.underground();
    let tunnels: Vec<&Void> = under
        .voids
        .iter()
        .filter(|v| v.kind == VoidKind::Tunnel && v.max.x - v.min.x > 70.0)
        .collect();
    let ways: Vec<_> = under.entrances.iter().filter(|e| !e.molehaven).collect();
    if tunnels.is_empty() || ways.is_empty() {
        return None;
    }
    let v = tunnels[rng.index(tunnels.len())];
    let x = rng.range(v.min.x + 25.0, v.max.x - 25.0);
    let y = (v.min.y + v.max.y) * 0.5;
    let on = |dx: f64| {
        let p = v.world(DVec2::new((x + dx).clamp(v.min.x + 2.0, v.max.x - 2.0), y));
        DVec3::new(p.x, v.floor_at(p), p.y)
    };
    let at = on(0.0);
    let e = ways.iter().min_by(|a, b| {
        a.pos
            .distance(DVec2::new(at.x, at.z))
            .total_cmp(&b.pos.distance(DVec2::new(at.x, at.z)))
    })?;
    Some(Site {
        place: Place::Tunnel,
        name: format!("the tunnels under {}", street_name(v.origin)),
        at,
        facing: v.along,
        way_down: Some(DVec3::new(e.pos.x, e.top, e.pos.y)),
        around: [8.0, -8.0, 16.0, -16.0, 24.0, -24.0]
            .iter()
            .map(|&d| on(d))
            .collect(),
        building: None,
    })
}

/// The street's way down nearest `p` among the deep tiles `tiles`: a tube
/// pavilion, or a little green exit.
fn way_down(tiles: &[Arc<DeepTile>], p: DVec2, kinds: &[EntranceKind]) -> Option<DVec3> {
    tiles
        .iter()
        .flat_map(|t| t.entrances.iter())
        .filter(|e| kinds.contains(&e.kind))
        .min_by(|a, b| a.pos.distance(p).total_cmp(&b.pos.distance(p)))
        .map(|e| DVec3::new(e.pos.x + e.door.x * 3.0, e.top, e.pos.y + e.door.y * 3.0))
}

const TUBE_WAYS: [EntranceKind; 2] = [EntranceKind::Tube, EntranceKind::Exit];

/// An Aether Tube station's hall.
fn station(places: &Places, rng: &mut Rng) -> Option<Site> {
    let town = places.town.as_ref()?;
    let net = town.tube();
    if net.stations.is_empty() {
        return None;
    }
    let k = rng.index(net.stations.len());
    let st = &net.stations[k];
    let tiles = town.deep_near(st.pos, 0.0);
    let (t, hall) = tiles
        .iter()
        .find_map(|t| t.stations.iter().find(|h| h.station == k).map(|h| (t, h)))?;
    let v = &t.voids[hall.hall];
    let name = if st.name.to_lowercase().ends_with("station") {
        st.name.clone()
    } else {
        format!("{} station", st.name)
    };
    Some(Site {
        place: Place::Station,
        name,
        at: middle(v),
        facing: v.along,
        way_down: way_down(&tiles, st.pos, &[EntranceKind::Tube]),
        around: void_points(v, 6),
        building: None,
    })
}

/// The deep tiles round a tube station with a lift down to the Deeps (or
/// any station), and that station's place.
fn deeps(town: &TownWorld, rng: &mut Rng) -> Option<(Vec<Arc<DeepTile>>, DVec2)> {
    let net = town.tube();
    let lifts: Vec<_> = net.stations.iter().filter(|s| s.deep_lift).collect();
    let list: Vec<_> = if lifts.is_empty() {
        net.stations.iter().collect()
    } else {
        lifts
    };
    if list.is_empty() {
        return None;
    }
    let st = list[rng.index(list.len())];
    Some((town.deep_near(st.pos, 300.0), st.pos))
}

fn grotto_name(k: GrottoKind) -> &'static str {
    match k {
        GrottoKind::Crystal => "the crystal grotto",
        GrottoKind::Mushroom => "the mushroom grotto",
        GrottoKind::GlowLake => "the glow lake grotto",
        GrottoKind::Fossil => "the fossil grotto",
    }
}

/// A cave chamber of the Deeps.
fn grotto(places: &Places, rng: &mut Rng) -> Option<Site> {
    let town = places.town.as_ref()?;
    let (tiles, from) = deeps(town, rng)?;
    let all: Vec<_> = tiles
        .iter()
        .flat_map(|t| t.grottos.iter().map(move |g| (t, g)))
        .collect();
    if all.is_empty() {
        return None;
    }
    let (t, g) = all[rng.index(all.len())];
    let v = &t.voids[g.void];
    Some(Site {
        place: Place::Grotto,
        name: grotto_name(g.kind).into(),
        at: DVec3::new(g.centre.x, g.floor, g.centre.y),
        facing: v.along,
        way_down: way_down(&tiles, from, &TUBE_WAYS),
        around: void_points(v, 6),
        building: None,
    })
}

/// The buried city: its domed vault, its street and halls round it.
fn ruin(places: &Places, rng: &mut Rng) -> Option<Site> {
    let town = places.town.as_ref()?;
    let (tiles, from) = deeps(town, rng)?;
    let all: Vec<_> = tiles
        .iter()
        .flat_map(|t| t.ruins.iter().map(move |r| (t, r)))
        .collect();
    if all.is_empty() {
        return None;
    }
    let (t, r) = all[rng.index(all.len())];
    let vault = &t.voids[r.rotunda];
    let mut around = void_points(vault, 2);
    around.extend(void_points(&t.voids[r.street], 3));
    for &h in &r.halls {
        around.extend(void_points(&t.voids[h], 1));
    }
    Some(Site {
        place: Place::Ruin,
        name: format!("the ruins of {}", r.name),
        at: middle(vault),
        facing: vault.along,
        way_down: way_down(&tiles, from, &TUBE_WAYS),
        around,
        building: None,
    })
}

/// The Doctor's lab, down from the newspaper kiosk.
fn lair(places: &Places, _rng: &mut Rng) -> Option<Site> {
    let town = places.town.as_ref()?;
    let c = town.lair_centre()?;
    let tiles = town.deep_near(c, 160.0);
    let t = tiles.iter().find(|t| !t.lairs.is_empty())?;
    let l = &t.lairs[0];
    let hall = &t.voids[l.hall];
    let mut around = void_points(hall, 3);
    for k in [l.reactor, l.control, l.cells] {
        around.push(middle(&t.voids[k]));
    }
    Some(Site {
        place: Place::Lair,
        name: "the Doctor's lab".into(),
        at: middle(hall),
        facing: -hall.along,
        way_down: way_down(&tiles, c, &[EntranceKind::Kiosk]),
        around,
        building: None,
    })
}

/// What a building is called from the street, by what it is used for.
fn building_noun(site: &hw_gen::building::BuildingSite) -> &'static str {
    use hw_gen::building::BuildingUse::*;
    match site.usage {
        Offices => "office tower",
        ShopsAndOffices => "shop block",
        Apartments => "apartment house",
        House => "house",
        Works => "works",
        Pavilion => "pavilion",
        Lab => "laboratory",
        Tenement => "tenement",
    }
}

/// A roof's own reach in from its edges for where people stand (m).
const ROOF_INSET: f64 = 3.5;

/// A flat roof two to six floors up, wide enough to fight on. The points
/// are on the roof as the plan has it; the case puts them on what is
/// really there ([`Site::place`] says it is a roof).
fn rooftop(places: &Places, rng: &mut Rng) -> Option<Site> {
    let area = places.area.as_ref()?;
    if area.buildings.is_empty() {
        return None;
    }
    for _ in 0..40 {
        let b = &area.buildings[rng.index(area.buildings.len())];
        let s = &b.site;
        if !(2..=6).contains(&s.floors) || s.size.min_element() < 15.0 {
            continue;
        }
        let look = s.look();
        if look.roof != hw_gen::style::Roof::Flat || look.taxi_pad || s.is_hideout() {
            continue;
        }
        let mid = s.origin + (s.along * s.size.x + s.inward * s.size.y) * 0.5;
        if !places.clear(mid) || !area.contains(mid) {
            continue;
        }
        let y = b.base + s.height();
        let half = s.size * 0.5 - DVec2::splat(ROOF_INSET);
        let on = |fx: f64, fz: f64| {
            let p = mid + s.along * (half.x * fx) + s.inward * (half.y * fz);
            DVec3::new(p.x, y, p.y)
        };
        return Some(Site {
            place: Place::Rooftop,
            name: format!(
                "the roof of the {} on {}",
                building_noun(s),
                street_near(area, s.origin)
            ),
            at: on(0.0, 0.0),
            facing: -s.inward,
            way_down: None,
            around: vec![
                on(0.0, -0.8),
                on(-0.8, 0.6),
                on(0.8, 0.6),
                on(0.8, -0.6),
                on(-0.8, -0.6),
                on(0.0, 0.8),
            ],
            building: None,
        });
    }
    None
}

/// What a shop worth a heist is (by its seed).
const SHOPS: [&str; 6] = [
    "jeweller's",
    "savings bank",
    "furrier's",
    "pawnbroker's",
    "department store",
    "payroll office",
];

/// The pavement in front of a shop block's street door (not a hideout's).
fn shopfront(places: &Places, rng: &mut Rng) -> Option<Site> {
    let area = places.area.as_ref()?;
    if area.buildings.is_empty() {
        return None;
    }
    for _ in 0..40 {
        let b = &area.buildings[rng.index(area.buildings.len())];
        let s = &b.site;
        if s.usage != hw_gen::building::BuildingUse::ShopsAndOffices || s.is_hideout() {
            continue;
        }
        let front = s.origin + s.along * s.size.x * 0.5 - s.inward * 2.0;
        if !places.clear(front) || !area.contains(front) {
            continue;
        }
        if area
            .town
            .dystopia
            .as_ref()
            .is_some_and(|d| d.contains(front))
        {
            continue;
        }
        return Some(Site {
            place: Place::Shopfront,
            name: format!(
                "the {} on {}",
                SHOPS[s.seed.child("shop").rng().index(SHOPS.len())],
                street_near(area, s.origin)
            ),
            at: DVec3::new(front.x, area.ground(front), front.y),
            facing: -s.inward,
            way_down: None,
            around: Vec::new(),
            building: None,
        });
    }
    None
}

/// A stretch of a main road near the start: where a getaway car is seen.
fn road(places: &Places, rng: &mut Rng) -> Option<Site> {
    let area = places.area.as_ref()?;
    let roads: Vec<_> = area
        .roads
        .iter()
        .filter(|r| r.main && r.pts.len() >= 2 && (r.half + r.pavement) * 2.0 >= 11.0)
        .collect();
    if roads.is_empty() {
        return None;
    }
    for _ in 0..16 {
        let r = roads[rng.index(roads.len())];
        let i = rng.index(r.pts.len() - 1);
        let (a, b) = (r.pts[i], r.pts[i + 1]);
        let at = a.lerp(b, rng.range(0.2, 0.8));
        if !places.clear(at) || !area.contains(at) {
            continue;
        }
        return Some(Site {
            place: Place::Road,
            name: street_name(r.pts[0]),
            at: DVec3::new(at.x, area.ground(at), at.y),
            facing: (b - a).normalize_or(DVec2::X),
            way_down: None,
            around: Vec::new(),
            building: None,
        });
    }
    None
}

/// How far from the hideout a bagman to tail sets out (m).
pub const TAIL_FROM: (f64, f64) = (140.0, 260.0);

/// Where a bagman sets out for the hideout at `hideout`: the pavement of
/// a street a few blocks from it.
pub fn tail_start(places: &Places, hideout: &Site, rng: &mut Rng) -> Option<Site> {
    let area = places.area.as_ref()?;
    let h = DVec2::new(hideout.at.x, hideout.at.z);
    // To the hideout's front door.
    let door = hideout.building.map_or(hideout.at, |k| {
        let b = &places.floors.buildings[k];
        b.to_town(b.graph.front())
    });
    let roads: Vec<_> = area
        .roads
        .iter()
        .filter(|r| r.pts.len() >= 2 && r.pavement >= 1.0)
        .collect();
    if roads.is_empty() {
        return None;
    }
    for _ in 0..24 {
        let r = roads[rng.index(roads.len())];
        let i = rng.index(r.pts.len() - 1);
        let (a, b) = (r.pts[i], r.pts[i + 1]);
        let len = a.distance(b);
        if len < 12.0 {
            continue;
        }
        let d = (b - a) / len;
        let side = if rng.chance(0.5) { d.perp() } else { -d.perp() };
        let at = a.lerp(b, rng.range(0.25, 0.75)) + side * (r.half + r.pavement * 0.5);
        let far = at.distance(h);
        if far < TAIL_FROM.0 || far > TAIL_FROM.1 || !places.clear(at) || !area.contains(at) {
            continue;
        }
        let from = DVec3::new(at.x, area.ground(at), at.y);
        // His way there, by the pavements: without one, not from here.
        let Some(way) = places.walk(from, door) else {
            continue;
        };
        return Some(Site {
            place: Place::Street,
            name: street_name(r.pts[0]),
            at: from,
            facing: way
                .first()
                .map_or(h - at, |p| DVec2::new(p.x, p.z) - at)
                .normalize_or(DVec2::X),
            way_down: None,
            around: way,
            building: None,
        });
    }
    None
}
