//! Office buildings at game scale. Three ways to organise one
//! ([`Scheme`]); the seed picks among those that fit the plot.

use super::stairs::{
    self, dog_leg_depth, dog_leg_hall, spiral_hall, DOG_LEG_WIDTH, LIFT_BANK, SPIRAL_SIDE,
};
use super::{
    Builder, Flight, Landing, Layout, Lift, LiftKind, LinkKind, Scheme, SpaceKind, Stair,
    StairKind, ARCH, DOOR, DOUBLE, ENTRANCE, MIN_HALL, MIN_WAY,
};
use crate::geom::Rect;
use glam::{DVec2, DVec3};
use hw_core::{Rng, Seed};

/// The smallest office plot (width along the street, depth) at game scale.
pub const MIN_SIZE: DVec2 = DVec2::new(32.0, 38.0);
/// Width of a gallery or arcade (m).
/// The least plot a shop house fits on (m): a room at least a hall wide
/// at the street, and the stair behind it.
pub const MIN_HOUSE: DVec2 = DVec2::new(12.0, 22.0);
/// The least plot a house of flats fits on (m).
pub const MIN_FLATS: DVec2 = DVec2::new(12.0, 24.0);
/// The smallest plot an apartment block fits on.
pub const MIN_BLOCK: DVec2 = DVec2::new(30.0, 28.0);
/// The least plot of a wide, shallow house (flats or a family's house):
/// the stair (8.5 m) at one end, a living room and a column of rooms.
pub const MIN_ROW: DVec2 = DVec2::new(23.0, 16.0);
/// How deep the landing between a block's two flats is.
const LANDING: f64 = 6.0;
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

/// Plans an office building for a town plot: one whose floors run level
/// through the whole building, so the facade's rows of windows fit it.
pub fn plan_town(seed: Seed, size: DVec2, storeys: u32) -> Layout {
    if size.x < MIN_SIZE.x || size.y < MIN_SIZE.y {
        return shop_house(seed, size.max(MIN_HOUSE), storeys.max(2))
            .expect("a shop house fits every plot of at least MIN_HOUSE");
    }
    let mut order = [Scheme::Atrium, Scheme::GrandHall];
    seed.child("scheme").rng().shuffle(&mut order);
    order
        .iter()
        .find_map(|s| plan_scheme(seed, size, storeys, *s))
        .or_else(|| grand_hall(seed, size.max(MIN_SIZE), storeys.max(2)))
        .expect("a grand hall fits every plot of at least MIN_SIZE")
}

/// Plans a house of flats: an apartment block on a plot of at least
/// [`MIN_BLOCK`], else an old-town house of at least [`MIN_FLATS`].
pub fn plan_flats(seed: Seed, size: DVec2, storeys: u32) -> Layout {
    if size.x >= MIN_BLOCK.x && size.y >= MIN_BLOCK.y {
        if let Some(l) = apartment_block(seed, size, storeys.max(2)) {
            return l;
        }
    }
    if size.y < MIN_FLATS.y {
        return row(seed, size.max(MIN_ROW), storeys.max(2), false)
            .expect("a terrace fits every plot of at least MIN_ROW");
    }
    flats(seed, size.max(MIN_FLATS), storeys.max(2))
        .expect("flats fit every plot of at least MIN_FLATS")
}

/// Plans a family's house (two storeys) on a plot of at least [`MIN_ROW`].
pub fn plan_villa(seed: Seed, size: DVec2) -> Layout {
    row(seed, size.max(MIN_ROW), 2, true).expect("a villa fits every plot of at least MIN_ROW")
}

/// Plans an office building in one scheme, if it fits.
pub fn plan_scheme(seed: Seed, size: DVec2, storeys: u32, scheme: Scheme) -> Option<Layout> {
    let least = match scheme {
        Scheme::ShopHouse => MIN_HOUSE,
        Scheme::Flats => MIN_FLATS,
        Scheme::Block => MIN_BLOCK,
        Scheme::Terrace | Scheme::Villa => MIN_ROW,
        _ => MIN_SIZE,
    };
    if size.x < least.x || size.y < least.y || storeys < 2 {
        return None;
    }
    match scheme {
        Scheme::GrandHall => grand_hall(seed, size, storeys),
        Scheme::Atrium => atrium(seed, size, storeys),
        Scheme::SplitLevel => split_level(seed, size, storeys),
        Scheme::ShopHouse => shop_house(seed, size, storeys),
        Scheme::Flats => flats(seed, size, storeys),
        Scheme::Block => apartment_block(seed, size, storeys),
        Scheme::Terrace => row(seed, size, storeys, false),
        Scheme::Villa => row(seed, size, 2, true),
    }
}

fn snap(v: f64) -> f64 {
    (v * 2.0).round() * 0.5
}

fn rect(x0: f64, z0: f64, x1: f64, z1: f64) -> Rect {
    Rect::from_xz(x0, z0, x1, z1)
}

/// Floor-to-floor heights that differ from storey to storey: a tall ground
/// floor, every upper floor its own height, and a taller top floor on
/// buildings of four storeys or more.
fn varied_heights(rng: &mut Rng, storeys: u32) -> Vec<f64> {
    let ground = *rng.pick(&[7.0, 7.5, 8.0]);
    (0..storeys)
        .map(|i| {
            let upper: f64 = *rng.pick(&[5.5, 6.0, 6.0, 6.5, 7.0]);
            match i {
                0 => ground,
                i if i + 1 == storeys && storeys >= 4 => upper.min(6.5) + 2.0,
                _ => upper,
            }
        })
        .collect()
}

/// What a part of the plan is on one storey.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Span {
    /// Rooms of this storey.
    Own,
    /// Rooms through this storey and the next.
    Tall,
    /// The upper half of the tall rooms from the storey below.
    Void,
}

/// For each of `n` storeys, from storey `from` up: now and then a part is
/// two storeys high.
fn spans(rng: &mut Rng, n: usize, from: usize, chance: f64) -> Vec<Span> {
    let mut out = vec![Span::Own; n];
    let mut i = from;
    while i + 1 < n {
        if rng.chance(chance) {
            out[i] = Span::Tall;
            out[i + 1] = Span::Void;
            i += 2;
        } else {
            i += 1;
        }
    }
    out
}

/// Rooms on `r`, laid out by the dice: one open hall, two halls of unlike
/// size, or a hall with a row of small rooms along one side. Doors join
/// them to each other; the caller joins them to the rest.
fn divide(b: &mut Builder, rng: &mut Rng, r: Rect, y: f64, h: f64, kind: SpaceKind) -> Vec<usize> {
    let s = r.size();
    if s.min_element() < MIN_HALL {
        return vec![b.space(SpaceKind::Service, r, y, h)];
    }
    let along_x = s.x >= s.y;
    let (long, short) = (s.max_element(), s.min_element());
    let (lo, side) = if along_x {
        (r.min.x, r.min.y)
    } else {
        (r.min.y, r.min.x)
    };
    // The part from `a` to `c` the long way, `e` to `f` the short way.
    let part = |a: f64, c: f64, e: f64, f: f64| {
        if along_x {
            rect(a, e, c, f)
        } else {
            rect(e, a, f, c)
        }
    };
    let dice = rng.index(10);
    if dice < 5 && long >= 2.0 * MIN_HALL {
        let f = *rng.pick(&[0.35, 0.5, 0.65]);
        let m = snap(lo + long * f).clamp(lo + MIN_HALL, lo + long - MIN_HALL);
        let other = if kind == SpaceKind::Hall && rng.chance(0.4) {
            SpaceKind::Boardroom
        } else {
            kind
        };
        let (first, second) = if rng.chance(0.5) {
            (kind, other)
        } else {
            (other, kind)
        };
        let a = b.space(first, part(lo, m, side, side + short), y, h);
        let c = b.space(second, part(m, lo + long, side, side + short), y, h);
        b.door(a, c, LinkKind::Double, y, DOUBLE);
        return vec![a, c];
    }
    if dice < 8 && long >= 16.0 && short >= MIN_HALL + 6.0 {
        let deep = if short >= MIN_HALL + 7.0 && rng.chance(0.5) {
            7.0
        } else {
            6.0
        };
        let (h0, h1, r0, r1) = if rng.chance(0.5) {
            (side + deep, side + short, side, side + deep)
        } else {
            (side, side + short - deep, side + short - deep, side + short)
        };
        let hall = b.space(kind, part(lo, lo + long, h0, h1), y, h);
        let mut out = vec![hall];
        for (a, c) in parts(lo, lo + long, (long / 9.0).floor() as usize) {
            let room = b.space(SpaceKind::Service, part(a, c, r0, r1), y, h);
            b.door(room, hall, LinkKind::Door, y, DOOR);
            out.push(room);
        }
        return out;
    }
    vec![b.space(kind, r, y, h)]
}

/// How long the edge is that two spaces share.
fn touch(b: &Builder, a: usize, c: usize) -> f64 {
    b.shared(a, c).map_or(0.0, |(_, _, lo, hi)| hi - lo)
}

/// A door from every one of `rooms` that has the wall for it to `to`, or
/// at least from the one with the most wall. False if none touches it.
fn join_all(b: &mut Builder, rooms: &[usize], to: usize, kind: LinkKind, y: f64, w: f64) -> bool {
    let mut any = false;
    for r in rooms {
        if touch(b, *r, to) >= w + 0.6 {
            b.door(*r, to, kind, y, w);
            any = true;
        }
    }
    if !any {
        let best = rooms
            .iter()
            .copied()
            .max_by(|p, q| touch(b, *p, to).total_cmp(&touch(b, *q, to)));
        if let Some(r) = best.filter(|r| touch(b, *r, to) > 1.0) {
            b.door(r, to, kind, y, w);
            any = true;
        }
    }
    any
}

/// One door between the two groups of rooms, where they share the most
/// wall (if that is enough for it).
fn join(b: &mut Builder, these: &[usize], those: &[usize], kind: LinkKind, y: f64, w: f64) -> bool {
    let best = these
        .iter()
        .flat_map(|p| those.iter().map(move |q| (*p, *q)))
        .max_by(|p, q| touch(b, p.0, p.1).total_cmp(&touch(b, q.0, q.1)));
    match best {
        Some((p, q)) if touch(b, p, q) >= w + 0.6 => {
            b.door(p, q, kind, y, w);
            true
        }
        _ => false,
    }
}

/// From `from` on the storey above, glass down into the tall `rooms`.
fn overlook(b: &mut Builder, from: usize, rooms: &[usize], y: f64) {
    for r in rooms {
        if touch(b, from, *r) > 1.0 {
            b.edge(from, *r, LinkKind::Glass, y);
        }
    }
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
    let hs = varied_heights(&mut rng, n as u32);
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
    // No two storeys alike: now and then the front or a side of the back
    // is two storeys high, and every storey divides its rooms anew.
    let front_span = spans(&mut rng, n, 2, 0.3);
    let back_span = [spans(&mut rng, n, 0, 0.22), spans(&mut rng, n, 0, 0.22)];
    let mut front_below: Vec<usize> = Vec::new();
    let mut back_below: [Vec<usize>; 2] = Default::default();
    // How deep the front band is on each storey: its own from the second
    // floor up, but the same as below where a tall room comes up from there.
    let mut lzs = vec![lz; n];
    for i in 2..n {
        let held = front_span[i] == Span::Void || back_span.iter().any(|s| s[i] == Span::Void);
        lzs[i] = if held {
            lzs[i - 1]
        } else {
            let fits: Vec<f64> = [lz - 4.0, lz, lz, lz + 3.0]
                .into_iter()
                .filter(|z| *z >= 14.0 && *z <= d - bd - 4.0)
                .collect();
            *rng.pick(&fits)
        };
    }
    for i in 0..n {
        let (y, h) = (ys[i], hs[i]);
        let lz = lzs[i];
        let high = |s: Span| match s {
            Span::Tall => h + hs[i + 1],
            _ => h,
        };
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
            _ if front_span[i] == Span::Void => {}
            _ => {
                let kind = if top_floor {
                    SpaceKind::Boardroom
                } else {
                    SpaceKind::Hall
                };
                let r = rect(0.0, 0.0, w, lz);
                front = divide(&mut b, &mut rng, r, y, high(front_span[i]), kind);
                if front_span[i] == Span::Tall {
                    front_below = front.clone();
                }
            }
        }
        // Back part: concourse, halls either side, corners by the stair hall.
        // Now and then the concourse and the rooms on one side of it are
        // one open hall, with a room left over beside the stair hall.
        let sides = [(0.0, sx.0), (sx.1, w)];
        let open_side = rng.index(2);
        let open = i >= 1
            && rng.chance(0.3)
            && back_span[open_side][i] == Span::Own
            && d - bd - lz >= MIN_HALL
            && sides[open_side].1 - sides[open_side].0 >= 0.5;
        let spine = if open {
            let (x0, x1) = sides[open_side];
            b.space(
                SpaceKind::Hall,
                rect(x0.min(sx.0), lz, x1.max(sx.1), d - bd),
                y,
                h,
            )
        } else {
            b.space(SpaceKind::Concourse, rect(sx.0, lz, sx.1, d - bd), y, h)
        };
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
        if i < 2 {
            for f in &front {
                if b.shared(*f, spine).is_some() {
                    b.door(*f, spine, LinkKind::Double, y, DOUBLE);
                }
            }
        } else if front_span[i] == Span::Void {
            overlook(&mut b, spine, &front_below, y);
        } else {
            join_all(&mut b, &front, spine, LinkKind::Double, y, DOUBLE);
        }
        for (side, (x0, x1)) in sides.into_iter().enumerate() {
            if x1 - x0 < 0.5 {
                continue;
            }
            if open && side == open_side {
                let r = rect(x0, d - bd, x1, d);
                let room = b.space(hall_or_service(&r), r, y, h);
                b.door(room, spine, LinkKind::Double, y, DOUBLE);
                continue;
            }
            let span = back_span[side][i];
            if span == Span::Void {
                overlook(&mut b, spine, &back_below[side], y);
                continue;
            }
            let r = rect(x0, lz, x1, d);
            let rooms = divide(&mut b, &mut rng, r, y, high(span), SpaceKind::Hall);
            join_all(&mut b, &rooms, spine, LinkKind::Double, y, DOUBLE);
            // And a door from the front band straight into it.
            join(&mut b, &front, &rooms, LinkKind::Door, y, DOOR);
            if span == Span::Tall {
                back_below[side] = rooms;
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
    let hs = varied_heights(&mut rng, n as u32);
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

    // No two storeys alike: one front corner is now and then two storeys
    // high (never both: the middle of the front is reached through them),
    // bridges cross where the dice say, and every storey divides its
    // wings anew.
    let tall_side = rng.index(2);
    let corner_span = spans(&mut rng, n, 1, 0.3);
    let mut corner_below: Vec<usize> = Vec::new();
    for i in 0..n {
        let (y, h) = (ys[i], hs[i]);
        let ground = i == 0;
        // The ring round the atrium: arcades on the ground, galleries above.
        let ring_kind = if ground {
            SpaceKind::Arcade
        } else {
            SpaceKind::Gallery
        };
        // Now and then a wing has no gallery of its own: one open hall
        // right up to the rail.
        let open_wing = [(); 2].map(|_| !ground && rng.chance(0.3));
        let left = if open_wing[0] {
            b.space(SpaceKind::Hall, rect(0.0, az0, ax0, az1), y, h)
        } else {
            b.space(ring_kind, rect(ax0 - g, az0, ax0, az1), y, h)
        };
        let right = if open_wing[1] {
            b.space(SpaceKind::Hall, rect(ax1, az0, w, az1), y, h)
        } else {
            b.space(ring_kind, rect(ax1, az0, ax1 + g, az1), y, h)
        };
        let back = b.space(ring_kind, rect(ax0, az1 - g, ax1, az1), y, h);
        b.edge(left, back, LinkKind::Open, y);
        b.edge(back, right, LinkKind::Open, y);
        let bridge = (i >= 2 && rng.chance(0.5)).then(|| {
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
        let wings: Vec<Vec<usize>> = [(0.0, ax0 - g, left), (ax1 + g, w, right)]
            .into_iter()
            .zip(open_wing)
            .map(|((x0, x1, ring), open)| {
                if open {
                    return vec![ring];
                }
                let r = rect(x0, az0, x1, az1);
                let wing = divide(&mut b, &mut rng, r, y, h, SpaceKind::Hall);
                join_all(&mut b, &wing, ring, LinkKind::Double, y, DOUBLE);
                wing
            })
            .collect();
        // Front band: a vestibule and shops on the ground, halls above.
        let span_of = |side: usize| {
            if side == tall_side {
                corner_span[i]
            } else {
                Span::Own
            }
        };
        // Now and then the middle and one corner are one hall.
        let merge_side = rng.index(2);
        let merged =
            (!ground && rng.chance(0.3) && span_of(merge_side) == Span::Own).then_some(merge_side);
        let corner = |b: &mut Builder, side: usize, r: Rect| -> Option<usize> {
            if merged == Some(side) {
                return None;
            }
            let span = span_of(side);
            let kind = if ground {
                SpaceKind::Shop
            } else {
                SpaceKind::Hall
            };
            match span {
                Span::Void => None,
                Span::Tall => Some(b.space(kind, r, y, h + hs[i + 1])),
                Span::Own => Some(b.space(kind, r, y, h)),
            }
        };
        let fl = corner(&mut b, 0, rect(0.0, 0.0, ax0, az0));
        let fc = b.space(
            if ground {
                SpaceKind::Vestibule
            } else {
                SpaceKind::Hall
            },
            match merged {
                Some(0) => rect(0.0, 0.0, ax1, az0),
                Some(_) => rect(ax0, 0.0, w, az0),
                None => rect(ax0, 0.0, ax1, az0),
            },
            y,
            h,
        );
        let fr = corner(&mut b, 1, rect(ax1, 0.0, w, az0));
        for (side, c) in [fl, fr].into_iter().enumerate() {
            match c {
                Some(c) => {
                    b.door(c, fc, LinkKind::Double, y, DOUBLE);
                    join(&mut b, &[c], &wings[side], LinkKind::Door, y, DOOR);
                    if corner_span[i] == Span::Tall && side == tall_side {
                        corner_below = vec![c];
                    }
                }
                None if merged == Some(side) => {
                    join(&mut b, &[fc], &wings[side], LinkKind::Door, y, DOOR);
                }
                None => overlook(&mut b, fc, &corner_below, y),
            }
        }
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
        if let (true, Some(fl), Some(fr)) = (ground, fl, fr) {
            b.entrance(fc, cx, ENTRANCE);
            b.entrance(fl, ax0 * 0.5, DOUBLE);
            b.entrance(fr, (ax1 + w) * 0.5, DOUBLE);
        }
        // Back band: the stair hall and rooms either side of it.
        b.door_at(back, hall, LinkKind::Double, y, door_w, Some(door_x));
        for (x0, x1, wing) in [
            (0.0, hall_rect.min.x, &wings[0]),
            (hall_rect.max.x, w, &wings[1]),
        ] {
            if x1 - x0 > 0.5 {
                let r = rect(x0, az1, x1, d);
                let s = b.space(hall_or_service(&r), r, y, h);
                if !join(&mut b, &[s], wing, LinkKind::Door, y, DOOR) {
                    // A narrow one: a door as wide as its wall allows.
                    let best = wing
                        .iter()
                        .copied()
                        .max_by(|p, q| touch(&b, s, *p).total_cmp(&touch(&b, s, *q)))
                        .expect("a wing has rooms");
                    b.door(s, best, LinkKind::Door, y, DOOR);
                }
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

/// A shop house (see [`Scheme::ShopHouse`]): lower storeys than an office
/// building's, the stair in a back corner with a small room beside it,
/// and in front of them one room across the whole house on every storey.
/// Where the house is deep enough, a storey's room is now and then two
/// storeys high, with an arcade under a gallery along its back.
fn shop_house(seed: Seed, size: DVec2, n: u32) -> Option<Layout> {
    let (w, d) = (size.x, size.y);
    let n = n as usize;
    if w < MIN_HOUSE.x || n < 2 {
        return None;
    }
    let mut rng = seed.child("shop house").rng();
    let ground = *rng.pick(&[5.5, 6.0]);
    let hs: Vec<f64> = (0..n)
        .map(|i| match i {
            0 => ground,
            _ => *rng.pick(&[4.5, 5.0, 5.0, 5.5]),
        })
        .collect();
    let ys = levels(&hs);
    let top = ys[n];
    let floors = &ys[..n];
    let bd = dog_leg_depth(floors);
    // Where the front room ends and the stair begins.
    let fz = d - bd;
    if fz < MIN_HALL {
        return None;
    }
    let mut b = Builder::default();
    // The stair in one back corner, the small room in the other.
    let left = rng.chance(0.5);
    let (sx, bx) = if left {
        ((0.0, DOG_LEG_WIDTH), (DOG_LEG_WIDTH, w))
    } else {
        ((w - DOG_LEG_WIDTH, w), (0.0, w - DOG_LEG_WIDTH))
    };
    let hall = dog_leg_hall(&mut b, rect(sx.0, fz, sx.1, d), floors, top, None);
    // A room two storeys high needs depth for its gallery.
    let tall = if fz >= MIN_HALL + GALLERY {
        spans(&mut rng, n, 0, 0.35)
    } else {
        vec![Span::Own; n]
    };
    let gz = fz - GALLERY;
    let mut high: Option<usize> = None;
    for i in 0..n {
        let (y, h) = (ys[i], hs[i]);
        let kind = if i == 0 {
            SpaceKind::Shop
        } else {
            SpaceKind::Hall
        };
        // The room in front, and what of it lies along the stair's wall.
        let (front, by_stair) = match tall[i] {
            Span::Own => {
                let rooms = if i == 0 {
                    vec![b.space(kind, rect(0.0, 0.0, w, fz), y, h)]
                } else {
                    divide(&mut b, &mut rng, rect(0.0, 0.0, w, fz), y, h, kind)
                };
                (rooms.first().copied(), rooms)
            }
            Span::Tall => {
                let room = b.space(kind, rect(0.0, 0.0, w, gz), y, h + hs[i + 1]);
                let under = b.space(SpaceKind::Arcade, rect(0.0, gz, w, fz), y, h);
                b.edge(room, under, LinkKind::Open, y);
                high = Some(room);
                (Some(room), vec![under])
            }
            Span::Void => {
                let gallery = b.space(SpaceKind::Gallery, rect(0.0, gz, w, fz), y, h);
                if let Some(room) = high.take() {
                    rail_except(&mut b, room, gallery, y, Vec::new());
                }
                (None, vec![gallery])
            }
        };
        if let (0, Some(shop)) = (i, front) {
            b.entrance(shop, snap(w * 0.5), DOUBLE);
        }
        if !join_all(&mut b, &by_stair, hall, LinkKind::Double, y, DOUBLE) {
            return None;
        }
        let back = b.space(SpaceKind::Service, rect(bx.0, fz, bx.1, d), y, h);
        if !join_all(&mut b, &by_stair, back, LinkKind::Door, y, DOOR) {
            return None;
        }
    }
    Some(b.finish(Scheme::ShopHouse, size))
}

/// A house of flats (see [`Scheme::Flats`]). The stair stands in one back
/// corner. On the ground floor the house hall runs from the street to it;
/// above, every storey is one flat: the living room across the house at
/// the street, behind it a kitchen to eat in (the way in from the stair),
/// and a bedroom in the other back corner. No two flats alike: the
/// kitchen's depth changes, and a wide house's living room now and then
/// gives a second bedroom away.
fn flats(seed: Seed, size: DVec2, n: u32) -> Option<Layout> {
    let (w, d) = (size.x, size.y);
    let n = n as usize;
    if w < MIN_FLATS.x || n < 2 {
        return None;
    }
    let mut rng = seed.child("flats").rng();
    let hs: Vec<f64> = (0..n)
        .map(|i| match i {
            0 => 5.0,
            _ => *rng.pick(&[4.0, 4.5, 4.5, 5.0]),
        })
        .collect();
    let ys = levels(&hs);
    let top = ys[n];
    let floors = &ys[..n];
    let bd = dog_leg_depth(floors);
    let fz = d - bd;
    if fz < 12.5 {
        return None;
    }
    let mut b = Builder::default();
    let left = rng.chance(0.5);
    let (sx, bx) = if left {
        ((0.0, DOG_LEG_WIDTH), (DOG_LEG_WIDTH, w))
    } else {
        ((w - DOG_LEG_WIDTH, w), (0.0, w - DOG_LEG_WIDTH))
    };
    let hall = dog_leg_hall(&mut b, rect(sx.0, fz, sx.1, d), floors, top, None);
    for i in 0..n {
        let (y, h) = (ys[i], hs[i]);
        let back_rect = rect(bx.0, fz, bx.1, d);
        if i == 0 {
            // The house hall, and a store beside the stair.
            let kz = fz - 5.5;
            let front = b.space(SpaceKind::Vestibule, rect(0.0, 0.0, w, kz), y, h);
            let way = b.space(SpaceKind::Arcade, rect(0.0, kz, w, fz), y, h);
            b.entrance(front, snap(w * 0.5), DOUBLE);
            b.edge(front, way, LinkKind::Open, y);
            b.door(way, hall, LinkKind::Double, y, DOUBLE);
            let store = b.space(SpaceKind::Service, back_rect, y, h);
            b.door(way, store, LinkKind::Door, y, DOOR);
            continue;
        }
        let deep = if fz >= 14.0 && rng.chance(0.5) {
            6.5
        } else {
            5.5
        };
        let kz = fz - deep;
        let kitchen = b.space(SpaceKind::Kitchen, rect(0.0, kz, w, fz), y, h);
        b.door(kitchen, hall, LinkKind::Door, y, DOOR);
        let bedroom = b.space(SpaceKind::Bedroom, back_rect, y, h);
        b.door(kitchen, bedroom, LinkKind::Door, y, DOOR);
        // A second bedroom off the living room, on the side away from the
        // first one, where the house is wide enough for both.
        let second = w >= 14.0 && rng.chance(0.45);
        if second {
            let bw = 5.0;
            let (lx, rx) = if left { (0.0, bw) } else { (w - bw, w) };
            let living_rect = if left {
                rect(bw, 0.0, w, kz)
            } else {
                rect(0.0, 0.0, w - bw, kz)
            };
            let living = b.space(SpaceKind::Living, living_rect, y, h);
            let room = b.space(SpaceKind::Bedroom, rect(lx, 0.0, rx, kz), y, h);
            b.door(living, kitchen, LinkKind::Double, y, DOUBLE);
            b.door(living, room, LinkKind::Door, y, DOOR);
        } else {
            let living = b.space(SpaceKind::Living, rect(0.0, 0.0, w, kz), y, h);
            b.door(living, kitchen, LinkKind::Double, y, DOUBLE);
        }
    }
    Some(b.finish(Scheme::Flats, size))
}

/// An apartment block: the stairs and two lifts at the middle of the back,
/// a landing in front of them on every storey and a flat either side of
/// it. Each flat has its living room at the street, a kitchen to eat in
/// beside the landing and a bedroom in the back corner; now and then a
/// second bedroom off the living room. The ground floor is the lobby.
fn apartment_block(seed: Seed, size: DVec2, n: u32) -> Option<Layout> {
    let (w, d) = (size.x, size.y);
    let n = n as usize;
    if w < MIN_BLOCK.x || n < 2 {
        return None;
    }
    let mut rng = seed.child("block").rng();
    let hs: Vec<f64> = (0..n)
        .map(|i| match i {
            0 => *rng.pick(&[5.0, 5.5, 6.0]),
            _ => *rng.pick(&[4.0, 4.5, 4.5, 5.0]),
        })
        .collect();
    let ys = levels(&hs);
    let top = ys[n];
    let floors = &ys[..n];
    let bd = dog_leg_depth(floors);
    let fz = d - bd;
    let kz = fz - LANDING;
    if kz < 10.0 {
        return None;
    }
    let core = DOG_LEG_WIDTH + LIFT_BANK;
    let cx = snap(w * 0.5);
    let (x0, x1) = (cx - core * 0.5, cx + core * 0.5);
    let mut b = Builder::default();
    let kind = *rng.pick(&[LiftKind::Streamline, LiftKind::Cage]);
    let hall = dog_leg_hall(&mut b, rect(x0, fz, x1, d), floors, top, Some(kind));
    for i in 0..n {
        let (y, h) = (ys[i], hs[i]);
        if i == 0 {
            // The lobby across the whole front, rooms in the back corners.
            let lobby = b.space(SpaceKind::Lobby, rect(0.0, 0.0, w, fz), y, h);
            b.entrance(lobby, cx, ENTRANCE);
            b.door(lobby, hall, LinkKind::Double, y, DOUBLE);
            b.columns(lobby, COLUMNS);
            for r in [rect(0.0, fz, x0, d), rect(x1, fz, w, d)] {
                let room = b.space(SpaceKind::Service, r, y, h);
                b.door(lobby, room, LinkKind::Door, y, DOOR);
            }
            continue;
        }
        let landing = b.space(SpaceKind::Concourse, rect(x0, kz, x1, fz), y, h);
        b.door(landing, hall, LinkKind::Double, y, DOUBLE);
        // The two flats: the left one, and its mirror image.
        for left in [true, false] {
            // (x measured from the flat's outside wall inwards.)
            let fx = |t: f64| if left { t } else { w - t };
            let across =
                |a: f64, c: f64, z0: f64, z1: f64| rect(fx(a).min(fx(c)), z0, fx(a).max(fx(c)), z1);
            let (side, mid) = (
                if left { x0 } else { w - x1 },
                if left { cx } else { w - cx },
            );
            let kitchen = b.space(SpaceKind::Kitchen, across(0.0, side, kz, fz), y, h);
            let bedroom = b.space(SpaceKind::Bedroom, across(0.0, side, fz, d), y, h);
            b.door(kitchen, bedroom, LinkKind::Door, y, DOOR);
            let second = mid >= 16.0 && rng.chance(0.5);
            let bw = if second { *rng.pick(&[5.0, 6.0]) } else { 0.0 };
            let living = b.space(SpaceKind::Living, across(bw, mid, 0.0, kz), y, h);
            if second {
                let room = b.space(SpaceKind::Bedroom, across(0.0, bw, 0.0, kz), y, h);
                b.door(living, room, LinkKind::Door, y, DOOR);
            }
            b.door(
                living,
                kitchen,
                LinkKind::Double,
                y,
                DOUBLE.min(side - bw - 1.0),
            );
            // The flat's front door: off the landing into the kitchen, or
            // into the living room.
            if rng.chance(0.5) {
                b.door(landing, kitchen, LinkKind::Door, y, DOOR);
            } else {
                b.door(landing, living, LinkKind::Door, y, DOOR);
            }
        }
    }
    Some(b.finish(Scheme::Block, size))
}

/// A wide, shallow house. The stair stands at one end of the back with a
/// landing in front of it at the street (the way in on the ground floor);
/// beside them the rooms take the whole depth: a big living room next to
/// the landing, and at the far end a kitchen and a bedroom, one at the
/// street and one at the back.
///
/// As flats ([`Scheme::Terrace`]) every storey is one such flat behind its
/// own door. As a family's house ([`Scheme::Villa`], two storeys) the
/// ground floor is the living room, the kitchen and a store, and upstairs
/// an upper hall runs back from the landing past two bedrooms.
fn row(seed: Seed, size: DVec2, n: u32, family: bool) -> Option<Layout> {
    let (w, d) = (size.x, size.y);
    let n = n as usize;
    if w < MIN_ROW.x || n < 2 {
        return None;
    }
    let mut rng = seed.child("row").rng();
    let hs: Vec<f64> = (0..n)
        .map(|i| match i {
            0 => *rng.pick(&[4.5, 5.0]),
            _ => *rng.pick(&[4.0, 4.5, 4.5, 5.0]),
        })
        .collect();
    let ys = levels(&hs);
    let top = ys[n];
    let floors = &ys[..n];
    let bd = dog_leg_depth(floors);
    let fz = d - bd;
    if fz < MIN_WAY {
        return None;
    }
    let mut b = Builder::default();
    let left = rng.chance(0.5);
    // (x measured from the stair's end of the house.)
    let fx = |t: f64| if left { t } else { w - t };
    let across =
        |a: f64, c: f64, z0: f64, z1: f64| rect(fx(a).min(fx(c)), z0, fx(a).max(fx(c)), z1);
    let sw = DOG_LEG_WIDTH;
    let hall = dog_leg_hall(&mut b, across(0.0, sw, fz, d), floors, top, None);
    for i in 0..n {
        let (y, h) = (ys[i], hs[i]);
        let kind = if i == 0 {
            SpaceKind::Vestibule
        } else {
            SpaceKind::Concourse
        };
        let landing = b.space(kind, across(0.0, sw, 0.0, fz), y, h);
        if i == 0 {
            b.entrance(landing, snap(fx(sw * 0.5)), DOUBLE);
        }
        b.door(landing, hall, LinkKind::Double, y, DOUBLE);
        if family && i > 0 {
            // Upstairs: the upper hall, and the bedrooms off it.
            let ux = sw + MIN_WAY;
            let upper = b.space(SpaceKind::Gallery, across(sw, ux, 0.0, d), y, h);
            b.edge(landing, upper, LinkKind::Open, y);
            let mz = snap(d * *rng.pick(&[0.45, 0.5, 0.55]));
            for (z0, z1) in [(0.0, mz), (mz, d)] {
                let room = b.space(SpaceKind::Bedroom, across(ux, w, z0, z1), y, h);
                b.door(upper, room, LinkKind::Door, y, DOOR);
            }
            continue;
        }
        // The column of two rooms at the far end.
        let cw = if w - sw >= 16.5 {
            *rng.pick(&[5.5, 6.0, 6.5])
        } else {
            5.5
        };
        let mz = snap(d * *rng.pick(&[0.42, 0.5]));
        let living = b.space(SpaceKind::Living, across(sw, w - cw, 0.0, d), y, h);
        // The kitchen at the street or at the back.
        let front = rng.chance(0.5);
        let (kz, oz) = if front {
            ((0.0, mz), (mz, d))
        } else {
            ((mz, d), (0.0, mz))
        };
        let kitchen = b.space(SpaceKind::Kitchen, across(w - cw, w, kz.0, kz.1), y, h);
        let other = if family {
            SpaceKind::Service
        } else {
            SpaceKind::Bedroom
        };
        let room = b.space(other, across(w - cw, w, oz.0, oz.1), y, h);
        b.door(living, kitchen, LinkKind::Double, y, DOUBLE);
        b.door(living, room, LinkKind::Door, y, DOOR);
        if family {
            // The family's own hall: a wide way through.
            b.door(landing, living, LinkKind::Double, y, DOUBLE);
        } else {
            b.door(landing, living, LinkKind::Door, y, DOOR);
        }
    }
    let scheme = if family {
        Scheme::Villa
    } else {
        Scheme::Terrace
    };
    Some(b.finish(scheme, size))
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
