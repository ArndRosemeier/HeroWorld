//! Office buildings at game scale. Three ways to organise one
//! ([`Scheme`]); the seed picks among those that fit the plot.

use super::stairs::{
    self, dog_leg_depth, dog_leg_hall, spiral_hall, DOG_LEG_WIDTH, LIFT_BANK, SPIRAL_SIDE,
};
use super::{
    Builder, Flight, Landing, Layout, Lift, LiftKind, LinkKind, Scheme, SpaceKind, Stair,
    StairKind, ARCH, DOOR, DOUBLE, ENTRANCE, MIN_HALL,
};
use crate::geom::Rect;
use glam::{DVec2, DVec3};
use hw_core::{Rng, Seed};

/// The smallest office plot (width along the street, depth) at game scale.
pub const MIN_SIZE: DVec2 = DVec2::new(32.0, 38.0);
/// Width of a gallery or arcade (m).
const GALLERY: f64 = 5.0;
/// Column spacing in open-plan halls (m).
const COLUMNS: f64 = 9.0;

/// Plans an office building on a `size` plot with `storeys` floors.
pub fn plan(seed: Seed, size: DVec2, storeys: u32) -> Layout {
    let mut order = [Scheme::Atrium, Scheme::GrandHall, Scheme::SplitLevel];
    seed.child("scheme").rng().shuffle(&mut order);
    order
        .iter()
        .find_map(|s| plan_scheme(seed, size, storeys, *s))
        .or_else(|| grand_hall(seed, size.max(MIN_SIZE), storeys.max(2)))
        .expect("a grand hall fits every plot of at least MIN_SIZE")
}

/// Plans an office building in one scheme, if it fits.
pub fn plan_scheme(seed: Seed, size: DVec2, storeys: u32, scheme: Scheme) -> Option<Layout> {
    if size.x < MIN_SIZE.x || size.y < MIN_SIZE.y || storeys < 2 {
        return None;
    }
    match scheme {
        Scheme::GrandHall => grand_hall(seed, size, storeys),
        Scheme::Atrium => atrium(seed, size, storeys),
        Scheme::SplitLevel => split_level(seed, size, storeys),
    }
}

fn snap(v: f64) -> f64 {
    (v * 2.0).round() * 0.5
}

fn rect(x0: f64, z0: f64, x1: f64, z1: f64) -> Rect {
    Rect::from_xz(x0, z0, x1, z1)
}

/// Floor-to-floor heights: a tall ground floor, upper floors, and a taller
/// top floor on buildings of four storeys or more.
fn heights(rng: &mut Rng, storeys: u32) -> Vec<f64> {
    let ground = *rng.pick(&[7.0, 7.5, 8.0]);
    let upper = *rng.pick(&[5.5, 6.0, 6.5]);
    (0..storeys)
        .map(|i| match i {
            0 => ground,
            i if i + 1 == storeys && storeys >= 4 => upper + 2.0,
            _ => upper,
        })
        .collect()
}

/// Floor heights from storey heights, and the roof last.
fn levels(hs: &[f64]) -> Vec<f64> {
    let mut ys = vec![0.0];
    for h in hs {
        ys.push(ys.last().copied().unwrap_or(0.0) + h);
    }
    ys
}

/// The stair hall's kind: a dog-leg with a lift bank or a spiral round a
/// cage lift. Returns (spiral, width, depth).
fn stair_hall_size(rng: &mut Rng, floors: &[f64]) -> (bool, f64, f64) {
    if rng.chance(0.4) {
        (true, SPIRAL_SIDE, SPIRAL_SIDE)
    } else {
        (false, DOG_LEG_WIDTH + LIFT_BANK, dog_leg_depth(floors))
    }
}

/// Builds the stair hall; returns it, where along x its doors go and how
/// wide they may be.
fn stair_hall(
    b: &mut Builder,
    rng: &mut Rng,
    spiral: bool,
    r: Rect,
    floors: &[f64],
    top: f64,
) -> (usize, f64, f64) {
    if spiral {
        let (hall, x) = spiral_hall(b, r, floors, top, Some(LiftKind::Cage));
        (hall, x, DOOR)
    } else {
        let kind = *rng.pick(&[LiftKind::Streamline, LiftKind::Cage]);
        let hall = dog_leg_hall(b, r, floors, top, Some(kind));
        (hall, r.center().x, DOUBLE)
    }
}

/// Splits `lo..hi` into `n` equal parts.
fn parts(lo: f64, hi: f64, n: usize) -> Vec<(f64, f64)> {
    let n = n.max(1);
    (0..n)
        .map(|i| {
            let a = lo + (hi - lo) * i as f64 / n as f64;
            let b = lo + (hi - lo) * (i + 1) as f64 / n as f64;
            (snap(a), snap(b))
        })
        .collect()
}

/// Rails along the edge where `gallery` overlooks `host` at `y`, open where
/// stairs or bridges arrive (`gaps`, ranges along the edge).
fn rail_except(b: &mut Builder, host: usize, gallery: usize, y: f64, mut gaps: Vec<(f64, f64)>) {
    let Some((_, _, lo, hi)) = b.shared(host, gallery) else {
        return;
    };
    gaps.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut at = lo;
    for (g0, g1) in gaps {
        let (g0, g1) = (g0.max(lo), g1.min(hi));
        if g1 <= g0 {
            continue;
        }
        if g0 - at > 0.3 {
            b.edge_part(host, gallery, LinkKind::Rail, y, at, g0);
        }
        b.edge_part(host, gallery, LinkKind::Open, y, g0, g1);
        at = g1;
    }
    if hi - at > 0.3 {
        b.edge_part(host, gallery, LinkKind::Rail, y, at, hi);
    }
}

/// The kind of room for a leftover rectangle: a hall if big enough.
fn hall_or_service(r: &Rect) -> SpaceKind {
    if r.size().min_element() >= MIN_HALL {
        SpaceKind::Hall
    } else {
        SpaceKind::Service
    }
}

/// Tries grand stairs in order and keeps the first that fits.
fn first_grand(
    kinds: &[StairKind],
    host: usize,
    zone: Rect,
    y0: f64,
    y1: f64,
    z_a: f64,
    back: bool,
) -> Option<stairs::Grand> {
    kinds
        .iter()
        .find_map(|k| stairs::grand(*k, host, zone, y0, y1, z_a, back))
}

/// A grand lobby two storeys high at the front, with a grand stair up to a
/// gallery; a concourse back to the stair hall; open-plan halls either side.
fn grand_hall(seed: Seed, size: DVec2, n: u32) -> Option<Layout> {
    let (w, d) = (size.x, size.y);
    let n = n as usize;
    let mut rng = seed.child("grand hall").rng();
    let hs = heights(&mut rng, n as u32);
    let ys = levels(&hs);
    let top = ys[n];
    let floors = &ys[..n];
    let (spiral, hw, bd) = stair_hall_size(&mut rng, floors);
    let sw = if w >= 42.0 {
        snap(((w - 22.0) * 0.5).clamp(10.0, 16.0))
    } else {
        0.0
    };
    let lz = snap((d * 0.55).clamp(18.0, 30.0)).min(d - bd - 4.0);
    let gd = GALLERY;
    let front_gallery = sw > 0.0 && rng.chance(0.45);
    let (y1, h01) = (ys[1], hs[0] + hs.get(1).copied().unwrap_or(0.0));
    let cx = snap(w * 0.5);
    let mut b = Builder::default();

    // The lobby, the low strip under its gallery, and the gallery.
    let (lobby, under, gallery, grand) = if front_gallery {
        let lobby = b.space(SpaceKind::Lobby, rect(sw, gd, w - sw, lz), 0.0, h01);
        let zone = rect(sw + 1.0, gd, w - sw - 1.0, lz - 1.0);
        let kinds = [StairKind::Imperial, StairKind::DogLeg];
        let g = first_grand(&kinds, lobby, zone, 0.0, y1, gd, false)?;
        let under = b.space(SpaceKind::Vestibule, rect(sw, 0.0, w - sw, gd), 0.0, hs[0]);
        let gallery = b.space(SpaceKind::Gallery, rect(sw, 0.0, w - sw, gd), y1, hs[1]);
        (lobby, under, gallery, g)
    } else {
        let lobby = b.space(SpaceKind::Lobby, rect(sw, 0.0, w - sw, lz - gd), 0.0, h01);
        let zone = rect(sw + 1.0, 2.0, w - sw - 1.0, lz - gd);
        let mut kinds = vec![StairKind::Straight, StairKind::Twin, StairKind::Horseshoe];
        rng.shuffle(&mut kinds);
        kinds.push(StairKind::DogLeg);
        let g = first_grand(&kinds, lobby, zone, 0.0, y1, lz - gd, true)?;
        let under = b.space(SpaceKind::Arcade, rect(sw, lz - gd, w - sw, lz), 0.0, hs[0]);
        let gallery = b.space(SpaceKind::Gallery, rect(sw, lz - gd, w - sw, lz), y1, hs[1]);
        (lobby, under, gallery, g)
    };
    let arrivals = grand.arrivals.clone();
    b.stairs.push(grand.stair);
    b.edge(lobby, under, LinkKind::Open, 0.0);
    rail_except(&mut b, lobby, gallery, y1, arrivals);
    if front_gallery {
        b.entrance(under, cx, ENTRANCE);
    } else {
        b.entrance(lobby, cx, ENTRANCE);
    }

    // The stair hall at the back, the concourse leading to it.
    let hall_rect = rect(cx - hw * 0.5, d - bd, cx + hw * 0.5, d);
    let (hall, door_x, door_w) = stair_hall(&mut b, &mut rng, spiral, hall_rect, floors, top);
    // The concourse is as wide as the stair hall it leads to.
    let sx = (hall_rect.min.x, hall_rect.max.x);
    for i in 0..n {
        let (y, h) = (ys[i], hs[i]);
        let top_floor = i + 1 == n && n >= 4;
        // Front band.
        let mut front: Vec<usize> = Vec::new();
        match i {
            0 => {
                if sw > 0.0 {
                    for (x0, x1) in [(0.0, sw), (w - sw, w)] {
                        let shop = b.space(SpaceKind::Shop, rect(x0, 0.0, x1, lz), y, h);
                        b.entrance(shop, (x0 + x1) * 0.5, DOUBLE);
                        b.door(shop, lobby, LinkKind::Double, y, DOUBLE);
                        front.push(shop);
                    }
                }
            }
            1 => {
                for (x0, x1) in [(0.0, sw), (w - sw, w)] {
                    if sw > 0.0 {
                        let side = b.space(SpaceKind::Hall, rect(x0, 0.0, x1, lz), y, h);
                        b.door(side, gallery, LinkKind::Double, y, DOUBLE);
                        b.edge(side, lobby, LinkKind::Glass, y);
                        front.push(side);
                    }
                }
            }
            _ => {
                let kind = if top_floor {
                    SpaceKind::Boardroom
                } else {
                    SpaceKind::Hall
                };
                let k = (w / 32.0).ceil() as usize;
                let pieces: Vec<usize> = parts(0.0, w, k)
                    .into_iter()
                    .map(|(x0, x1)| b.space(kind, rect(x0, 0.0, x1, lz), y, h))
                    .collect();
                for p in pieces.windows(2) {
                    b.door(p[0], p[1], LinkKind::Double, y, DOUBLE);
                }
                front.extend(pieces);
            }
        }
        // Back part: concourse, halls either side, corners by the stair hall.
        let spine = b.space(SpaceKind::Concourse, rect(sx.0, lz, sx.1, d - bd), y, h);
        b.door_at(spine, hall, LinkKind::Double, y, door_w, Some(door_x));
        match i {
            0 => b.door(
                spine,
                if front_gallery { lobby } else { under },
                LinkKind::Arch,
                y,
                ARCH,
            ),
            1 if !front_gallery => b.door(spine, gallery, LinkKind::Double, y, DOUBLE),
            _ => {}
        }
        for f in &front {
            if b.shared(*f, spine).is_some() {
                b.door(*f, spine, LinkKind::Double, y, DOUBLE);
            }
        }
        for (x0, x1) in [(0.0, sx.0), (sx.1, w)] {
            if x1 - x0 < 0.5 {
                continue;
            }
            // Halls deeper than 26 m split in two.
            let k = ((d - lz) / 26.0).ceil() as usize;
            let mut prev: Option<usize> = None;
            let mut first = None;
            for (z0, z1) in parts(lz, d, k) {
                let r = rect(x0, z0, x1, z1);
                let s = b.space(hall_or_service(&r), r, y, h);
                if b.shared(s, spine).is_some() {
                    b.door(s, spine, LinkKind::Double, y, DOUBLE);
                }
                if let Some(p) = prev {
                    b.door(p, s, LinkKind::Double, y, DOUBLE);
                }
                first.get_or_insert(s);
                prev = Some(s);
            }
            // The front band piece in front of it.
            if let Some(f) = first {
                for p in &front {
                    if b.shared(*p, f).is_some() && b.shared(*p, spine).is_none() {
                        b.door(*p, f, LinkKind::Door, y, DOOR);
                    }
                }
            }
        }
    }
    // Columns last: they keep clear of every door and stair.
    for s in 0..b.spaces.len() {
        if matches!(
            b.spaces[s].kind,
            SpaceKind::Hall | SpaceKind::Boardroom | SpaceKind::Lobby
        ) {
            b.columns(s, COLUMNS);
        }
    }
    Some(b.finish(Scheme::GrandHall, size))
}

/// An atrium through every storey: galleries on three sides, a grand stair
/// up to the first gallery, bridges across higher up and glass lifts at the
/// front; offices in wings round it; a stair hall behind.
fn atrium(seed: Seed, size: DVec2, n: u32) -> Option<Layout> {
    let (w, d) = (size.x, size.y);
    let n = n as usize;
    if n < 3 {
        return None;
    }
    let mut rng = seed.child("atrium").rng();
    let hs = heights(&mut rng, n as u32);
    let ys = levels(&hs);
    let top = ys[n];
    let floors = &ys[..n];
    let (spiral, hw, bd) = stair_hall_size(&mut rng, floors);
    let g = GALLERY;
    let aw = snap((w * 0.4).clamp(16.0, 28.0)).min(w - 2.0 * g - 2.0 * MIN_HALL);
    if aw < 16.0 || hw > w {
        return None;
    }
    let ax0 = snap((w - aw) * 0.5);
    let ax1 = ax0 + aw;
    let cx = (ax0 + ax1) * 0.5;
    let az0 = snap((d * 0.26).clamp(MIN_HALL, 16.0));
    let az1 = d - bd.max(snap(d * 0.22));
    let void = rect(ax0, az0, ax1, az1 - g);
    if void.size().y < 18.0 {
        return None;
    }
    let mut b = Builder::default();
    let atrium = b.space(SpaceKind::Atrium, void, 0.0, top);
    // Glass lifts at the front of the atrium.
    let lifts: Vec<usize> = [cx - 4.25, cx + 1.25]
        .into_iter()
        .map(|x| {
            b.nested(
                atrium,
                SpaceKind::Shaft,
                rect(x, az0, x + 3.0, az0 + 3.0),
                0.0,
                top,
            )
        })
        .collect();
    for s in &lifts {
        b.lifts.push(Lift {
            kind: LiftKind::Glass,
            shaft: *s,
            stops: floors.to_vec(),
        });
    }
    let lift_gap = (cx - 4.25, cx + 4.25);
    // The grand stair up to the back gallery.
    let zone = rect(ax0 + 0.5, az0 + 3.5, ax1 - 0.5, az1 - g);
    let mut kinds = vec![StairKind::Straight, StairKind::Twin, StairKind::Horseshoe];
    rng.shuffle(&mut kinds);
    kinds.push(StairKind::DogLeg);
    let grand = first_grand(&kinds, atrium, zone, 0.0, ys[1], az1 - g, true)?;
    let arrivals = grand.arrivals.clone();
    b.stairs.push(grand.stair);
    let bridge_z = snap((az0 + 3.0 + az1 - g) * 0.5);

    let hx0 = snap(w * 0.5 - hw * 0.5);
    let hall_rect = rect(hx0, az1, hx0 + hw, d);
    let (hall, door_x, door_w) = stair_hall(&mut b, &mut rng, spiral, hall_rect, floors, top);

    for i in 0..n {
        let (y, h) = (ys[i], hs[i]);
        let ground = i == 0;
        // The ring round the atrium: arcades on the ground, galleries above.
        let ring_kind = if ground {
            SpaceKind::Arcade
        } else {
            SpaceKind::Gallery
        };
        let left = b.space(ring_kind, rect(ax0 - g, az0, ax0, az1), y, h);
        let right = b.space(ring_kind, rect(ax1, az0, ax1 + g, az1), y, h);
        let back = b.space(ring_kind, rect(ax0, az1 - g, ax1, az1), y, h);
        b.edge(left, back, LinkKind::Open, y);
        b.edge(back, right, LinkKind::Open, y);
        let bridge = (i >= 2 && i % 2 == 0).then(|| {
            let r = rect(ax0, bridge_z - 2.5, ax1, bridge_z + 2.5);
            let s = b.nested(atrium, SpaceKind::Bridge, r, y, h);
            b.edge(s, left, LinkKind::Open, y);
            b.edge(s, right, LinkKind::Open, y);
            s
        });
        let bridge_gap = bridge
            .map(|_| vec![(bridge_z - 2.5, bridge_z + 2.5)])
            .unwrap_or_default();
        if ground {
            for s in [left, right, back] {
                b.edge(atrium, s, LinkKind::Open, y);
            }
        } else {
            rail_except(&mut b, atrium, left, y, bridge_gap.clone());
            rail_except(&mut b, atrium, right, y, bridge_gap);
            let gaps = if i == 1 { arrivals.clone() } else { Vec::new() };
            rail_except(&mut b, atrium, back, y, gaps);
        }
        // Wings either side.
        let wings: Vec<usize> = [(0.0, ax0 - g, left), (ax1 + g, w, right)]
            .into_iter()
            .map(|(x0, x1, ring)| {
                let wing = b.space(SpaceKind::Hall, rect(x0, az0, x1, az1), y, h);
                b.door(wing, ring, LinkKind::Double, y, DOUBLE);
                wing
            })
            .collect();
        // Front band: a vestibule and shops on the ground, halls above.
        let fl = b.space(
            if ground {
                SpaceKind::Shop
            } else {
                SpaceKind::Hall
            },
            rect(0.0, 0.0, ax0, az0),
            y,
            h,
        );
        let fc = b.space(
            if ground {
                SpaceKind::Vestibule
            } else {
                SpaceKind::Hall
            },
            rect(ax0, 0.0, ax1, az0),
            y,
            h,
        );
        let fr = b.space(
            if ground {
                SpaceKind::Shop
            } else {
                SpaceKind::Hall
            },
            rect(ax1, 0.0, w, az0),
            y,
            h,
        );
        b.door(fl, fc, LinkKind::Double, y, DOUBLE);
        b.door(fc, fr, LinkKind::Double, y, DOUBLE);
        b.door(fl, wings[0], LinkKind::Door, y, DOOR);
        b.door(fr, wings[1], LinkKind::Door, y, DOOR);
        let face = if ground {
            LinkKind::Open
        } else {
            LinkKind::Glass
        };
        b.edge_part(fc, atrium, face, y, ax0, lift_gap.0);
        b.edge_part(fc, atrium, face, y, lift_gap.1, ax1);
        for s in &lifts {
            let c = b.spaces[*s].rect.center().x;
            b.door_at(*s, fc, LinkKind::Door, y, DOOR, Some(c));
        }
        if ground {
            b.entrance(fc, cx, ENTRANCE);
            b.entrance(fl, ax0 * 0.5, DOUBLE);
            b.entrance(fr, (ax1 + w) * 0.5, DOUBLE);
        }
        // Back band: the stair hall and rooms either side of it.
        b.door_at(back, hall, LinkKind::Double, y, door_w, Some(door_x));
        for (x0, x1, wing) in [
            (0.0, hall_rect.min.x, wings[0]),
            (hall_rect.max.x, w, wings[1]),
        ] {
            if x1 - x0 > 0.5 {
                let r = rect(x0, az1, x1, d);
                let s = b.space(hall_or_service(&r), r, y, h);
                b.door(s, wing, LinkKind::Door, y, DOOR);
            }
        }
    }
    for s in 0..b.spaces.len() {
        if matches!(b.spaces[s].kind, SpaceKind::Hall | SpaceKind::Shop) {
            b.columns(s, COLUMNS);
        }
    }
    Some(b.finish(Scheme::Atrium, size))
}

/// Front and back blocks half a storey apart. Between them a canyon as
/// tall as the building, with landings along both sides and half flights
/// zig-zagging up between them; a paternoster at one end, a lift at the
/// other.
fn split_level(seed: Seed, size: DVec2, n: u32) -> Option<Layout> {
    let (w, d) = (size.x, size.y);
    let n = n as usize;
    if n < 3 || w < 24.0 {
        return None;
    }
    let mut rng = seed.child("split level").rng();
    let storey = *rng.pick(&[6.0, 6.5, 7.0]);
    let (m, rise) = stairs::risers(storey * 0.5);
    let run = Flight::run(m);
    let land = 3.0;
    let cd = 2.0 * land + run;
    let d1 = snap((d - cd) * 0.5);
    let z1 = d1 + cd;
    if d1 < 12.0 || d - z1 < 12.0 {
        return None;
    }
    let top = (n as f64 + 0.5) * storey;
    let mut b = Builder::default();
    let canyon = b.space(SpaceKind::Canyon, rect(0.0, d1, w, z1), 0.0, top);
    let shaft_w = 3.0;
    // Landings: front ones at whole storeys, back ones half a storey up.
    let front_landing: Vec<usize> = (1..n)
        .map(|i| {
            let y = i as f64 * storey;
            b.nested(
                canyon,
                SpaceKind::Landing,
                rect(shaft_w, d1, w, d1 + land),
                y,
                storey.min(top - y),
            )
        })
        .collect();
    let back_landing: Vec<usize> = (1..n)
        .map(|i| {
            let y = (i as f64 + 0.5) * storey;
            b.nested(
                canyon,
                SpaceKind::Landing,
                rect(0.0, z1 - land, w - shaft_w, z1),
                y,
                storey.min(top - y),
            )
        })
        .collect();
    // Half flights in two lanes up the middle.
    let cx = snap(w * 0.5);
    let lane = 3.5;
    let (l1, l2) = (cx - 1.0 - lane * 0.5, cx + 1.0 + lane * 0.5);
    let mut flights = Vec::new();
    for k in 0..(2 * n - 1) {
        let y = k as f64 * storey * 0.5;
        let f = if k % 2 == 0 {
            Flight::Straight {
                from: DVec3::new(l1, y, d1 + land),
                dir: DVec2::Y,
                width: lane,
                risers: m,
                rise,
            }
        } else {
            Flight::Straight {
                from: DVec3::new(l2, y, z1 - land),
                dir: -DVec2::Y,
                width: lane,
                risers: m,
                rise,
            }
        };
        flights.push(f);
    }
    b.stairs.push(Stair {
        kind: StairKind::Split,
        host: canyon,
        flights,
        landings: vec![Landing {
            rect: rect(l1 - lane * 0.5, z1 - land, l2 + lane * 0.5, z1),
            y: storey * 0.5,
        }],
    });
    // A paternoster for the front block, a lift for the back block.
    let pater = b.nested(
        canyon,
        SpaceKind::Shaft,
        rect(0.0, d1, shaft_w, d1 + land),
        0.0,
        top,
    );
    let lift = b.nested(
        canyon,
        SpaceKind::Shaft,
        rect(w - shaft_w, z1 - land, w, z1),
        0.0,
        top,
    );
    let front_ys: Vec<f64> = (0..n).map(|i| i as f64 * storey).collect();
    let mut back_ys = vec![0.0];
    back_ys.extend((1..n).map(|i| (i as f64 + 0.5) * storey));
    b.inner_door(
        pater,
        canyon,
        0.0,
        DVec2::new(shaft_w, d1 + land * 0.5),
        false,
        DOOR,
    );
    for (k, s) in front_landing.iter().enumerate() {
        b.door(pater, *s, LinkKind::Door, front_ys[k + 1], DOOR);
    }
    b.inner_door(
        lift,
        canyon,
        0.0,
        DVec2::new(w - shaft_w, z1 - land * 0.5),
        false,
        DOOR,
    );
    for (k, s) in back_landing.iter().enumerate() {
        b.door(lift, *s, LinkKind::Door, back_ys[k + 1], DOOR);
    }
    b.lifts.push(Lift {
        kind: LiftKind::Paternoster,
        shaft: pater,
        stops: front_ys.clone(),
    });
    b.lifts.push(Lift {
        kind: *rng.pick(&[LiftKind::Freight, LiftKind::Streamline]),
        shaft: lift,
        stops: back_ys.clone(),
    });
    // The blocks.
    let k = (w / 30.0).ceil() as usize;
    for i in 0..n {
        // Front block.
        let y = front_ys[i];
        let pieces = parts(0.0, w, k);
        let mut prev: Option<usize> = None;
        for (j, (x0, x1)) in pieces.iter().enumerate() {
            let kind = match i {
                0 if j == k / 2 => SpaceKind::Lobby,
                0 => SpaceKind::Shop,
                _ => SpaceKind::Hall,
            };
            let s = b.space(kind, rect(*x0, 0.0, *x1, d1), y, storey);
            if i == 0 {
                b.entrance(
                    s,
                    (x0 + x1) * 0.5,
                    if j == k / 2 { ENTRANCE } else { DOUBLE },
                );
            }
            let to = if i == 0 { canyon } else { front_landing[i - 1] };
            b.door_at(s, to, LinkKind::Arch, y, ARCH, Some((x0 + x1) * 0.5));
            if let Some(p) = prev {
                b.door(p, s, LinkKind::Double, y, DOUBLE);
            }
            prev = Some(s);
        }
        // Back block: a tall hall on the ground, half a storey up after.
        let (y, h) = if i == 0 {
            (0.0, storey * 1.5)
        } else {
            (back_ys[i], storey)
        };
        let mut prev: Option<usize> = None;
        for (x0, x1) in &pieces {
            let s = b.space(SpaceKind::Hall, rect(*x0, z1, *x1, d), y, h);
            let to = if i == 0 { canyon } else { back_landing[i - 1] };
            b.door_at(s, to, LinkKind::Arch, y, ARCH, Some((x0 + x1) * 0.5));
            if let Some(p) = prev {
                b.door(p, s, LinkKind::Double, y, DOUBLE);
            }
            prev = Some(s);
        }
    }
    for s in 0..b.spaces.len() {
        if matches!(
            b.spaces[s].kind,
            SpaceKind::Hall | SpaceKind::Lobby | SpaceKind::Shop
        ) {
            b.columns(s, COLUMNS);
        }
    }
    Some(b.finish(Scheme::SplitLevel, size))
}
