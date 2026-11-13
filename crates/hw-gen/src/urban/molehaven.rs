//! Molehaven, the mole folk's town in a cavern under the drill rig, as
//! plain data: where its mushroom houses, market, well and everything
//! about them stand, what each side chamber is for, the rock hanging from
//! the roof, and the gate on the street with signposts pointing to it.
//!
//! Everything drawn here is solid where [`MolehavenLayout::footprints`]
//! says (what is drawn is what collides), and keeps clear of the ways in:
//! the stair from the street, the ramp from the utility tunnels and the
//! stair on down to the Deeps.

use super::deep::{Fixture, FixtureKind};
use super::footprint::Footprint;
use super::spiral::Spiral;
use super::underground::{Entrance, Underground, VoidKind, SHAFT_HALF};
use glam::{DVec2, DVec3};
use hw_core::det;
use std::f64::consts::TAU;

/// The great hall's height (the void's ceiling over its floor).
pub const HALL_HEIGHT: f64 = 16.0;
/// How far the drill's bore reaches down through the hall's roof, and its
/// iron lining's radius.
pub const BORE_R: f64 = 3.0;

/// What a side chamber off the great hall is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoleRoom {
    /// Workbenches, a pump, carts of ore.
    Workshop,
    /// Rows of glowing mushrooms in pots, giant ones in the corners.
    Farm,
    /// A carved mole idol, candles, a glowing pool.
    Shrine,
    /// Little mushroom cottages round a yard, washing between them.
    Cottages,
}

/// A mushroom house.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoleHouse {
    pub at: DVec2,
    /// Stem radius and height, and the cap's radius.
    pub stem: f64,
    pub height: f64,
    pub cap: f64,
    /// The way its round door faces.
    pub door: Option<DVec2>,
    /// In the great hall (its people go to the market) or a side chamber.
    pub hall: bool,
    /// A number for its looks (cap colour, chimney, windows).
    pub look: u32,
}

impl MoleHouse {
    /// The solid stem's radius (the flared foot).
    pub fn foot(&self) -> f64 {
        self.stem * 1.35
    }
}

/// A rock icicle hanging from the hall's roof.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stalactite {
    pub at: DVec2,
    /// Its root, up in the rock, and its tip.
    pub top: f64,
    pub tip: f64,
    pub r: f64,
}

/// Where Molehaven's furniture stands, for drawing it, for bumping into it
/// and for its people to walk round it.
#[derive(Clone, Debug, PartialEq)]
pub struct MolehavenLayout {
    pub floor: f64,
    /// The well in the hall's middle: centre and radius.
    pub well: (DVec2, f64),
    pub houses: Vec<MoleHouse>,
    /// Market stalls round the well: centre, and the way the counter faces
    /// (out from the well). Each is also a [`FixtureKind::Stall`] fixture.
    pub stalls: Vec<(DVec2, DVec2)>,
    /// Everything else that stands about: lanterns, barrels, carts,
    /// benches, pots, the idol, rugs, washing, stalagmites, crystals.
    pub fixtures: Vec<Fixture>,
    /// Rock columns from floor to roof: centre and radius.
    pub pillars: Vec<(DVec2, f64)>,
    pub stalactites: Vec<Stalactite>,
    /// Toadstool lamps: where each stands.
    pub toadstools: Vec<DVec2>,
    /// The side chambers: index into the underground's voids, and its job.
    pub rooms: Vec<(usize, MoleRoom)>,
    /// Where neighbours stand and talk: (feet, facing).
    pub chats: Vec<(DVec2, DVec2)>,
    /// The drill's bore through the roof over the well: its bottom.
    pub bore: f64,
    /// The ways in that must stay open (stairs, ramps): circles in plan.
    pub ways: Vec<(DVec2, f64)>,
}

/// A toadstool lamp's stem radius and height.
pub const TOADSTOOL: (f64, f64) = (0.15, 2.2);

impl MolehavenLayout {
    /// Every solid part down there.
    pub fn footprints(&self) -> Vec<Footprint> {
        let f = self.floor;
        let mut out = Vec::new();
        for h in &self.houses {
            out.push(Footprint::Disc {
                at: h.at,
                r: h.foot(),
                y0: f,
                y1: f + h.height,
            });
            // The cap overhead: a body flying into it stops.
            out.push(Footprint::Disc {
                at: h.at,
                r: h.cap,
                y0: f + h.height - 0.15,
                y1: f + h.height + h.cap * 0.55,
            });
        }
        out.push(Footprint::Disc {
            at: self.well.0,
            r: self.well.1,
            y0: f,
            y1: f + 0.98,
        });
        for fx in &self.fixtures {
            out.extend(fx.footprints());
        }
        for &(at, r) in &self.pillars {
            out.push(Footprint::Disc {
                at,
                r,
                y0: f,
                y1: f + HALL_HEIGHT + 3.0,
            });
        }
        for s in &self.stalactites {
            out.push(Footprint::Disc {
                at: s.at,
                r: s.r * 0.6,
                y0: s.tip,
                y1: s.top,
            });
        }
        for &t in &self.toadstools {
            out.push(Footprint::Disc {
                at: t,
                r: TOADSTOOL.0 + 0.05,
                y0: f,
                y1: f + TOADSTOOL.1,
            });
        }
        out
    }

    /// Is a circle of radius `r` at `p` on the floor clear of everything
    /// that stands there?
    pub fn clear(&self, p: DVec2, r: f64) -> bool {
        self.footprints()
            .iter()
            .filter(|fp| fp.span().0 < self.floor + 2.0)
            .all(|fp| fp.distance(p) > r)
    }
}

/// A small hash for looks.
fn mix(a: u64, b: u64) -> u32 {
    let mut x = a
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(b.wrapping_mul(0xC2B2_AE3D_27D4_EB4F));
    x ^= x >> 29;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 32;
    (x & 0xFFFF_FFFF) as u32
}

/// Where the stair on down to the Deeps comes up in Molehaven (the deep
/// layer plans it there; its foot here must stay clear).
pub fn deep_lift_spot(u: &Underground) -> Option<DVec2> {
    deep_stair(u).map(|(c, _)| c)
}

/// The stair down to the Deeps from the first side chamber: its centre and
/// the way its door faces (into the chamber, along its back wall).
pub fn deep_stair(u: &Underground) -> Option<(DVec2, DVec2)> {
    let cav = u.cavern.as_ref()?;
    let &ci = cav.chambers.get(1)?;
    let ch = &u.voids[ci];
    Some((ch.world(ch.max - DVec2::splat(2.6)), -ch.along))
}

/// Is `p` over the hole of Molehaven's stair down to the Deeps (`grow`
/// metres round it too)? The cavern has no floor there.
pub fn over_deep_stair(u: &Underground, p: DVec2, grow: f64) -> bool {
    deep_stair(u).is_some_and(|(c, dir)| {
        let d = p - c;
        let k = SHAFT_HALF + grow;
        d.dot(dir).abs() <= k && d.dot(dir.perp()).abs() <= k
    })
}

/// Places that must stay open: the ways in and out (circles).
fn ways(u: &Underground) -> Vec<(DVec2, f64)> {
    let Some(c) = u.cavern.as_ref() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in &u.entrances {
        if (e.bottom - c.floor).abs() < 1.0 {
            // The stair's foot opens all round: the shaft and a walk round it.
            out.push((e.pos, SHAFT_HALF * 1.42 + 2.6));
        }
    }
    if let Some(p) = deep_lift_spot(u) {
        out.push((p, 5.4));
    }
    // Passages that open into the chambers at their floor (the ramp from the
    // tunnels, a stair's passage): where they come in.
    let chambers: Vec<_> = c.chambers.iter().map(|&i| &u.voids[i]).collect();
    for v in &u.voids {
        if v.kind != VoidKind::Passage {
            continue;
        }
        let (a, b) = (
            v.world(DVec2::new(v.min.x, 0.0)),
            v.world(DVec2::new(v.max.x, 0.0)),
        );
        let len = a.distance(b);
        let n = (len / 1.5).ceil().max(1.0) as usize;
        for k in 0..=n {
            let p = a.lerp(b, k as f64 / n as f64);
            if (v.floor_at(p) - c.floor).abs() > 1.5 {
                continue;
            }
            // Only its mouth: further in (a stair's passage may run on to
            // the hall's middle) it is the chamber's open floor.
            if chambers.iter().any(|ch| ch.covers(p, 4.0))
                && !chambers.iter().any(|ch| ch.covers(p, -4.0))
            {
                out.push((p, 2.0 + 2.6));
            }
        }
    }
    out
}

/// Molehaven's houses, market, well and furniture, if the town has the
/// cavern.
pub fn molehaven_layout(u: &Underground) -> Option<MolehavenLayout> {
    let c = u.cavern.as_ref()?;
    let hall = u.voids[*c.chambers.first()?].clone();
    let f = c.floor;
    let seed = (c.centre.x * 7.0).to_bits() ^ (c.centre.y * 3.0).to_bits();
    let keep = ways(u);
    let mut l = MolehavenLayout {
        floor: f,
        well: (c.centre, 2.34),
        houses: Vec::new(),
        stalls: Vec::new(),
        fixtures: Vec::new(),
        pillars: Vec::new(),
        stalactites: Vec::new(),
        toadstools: Vec::new(),
        rooms: Vec::new(),
        chats: Vec::new(),
        bore: f + HALL_HEIGHT - 0.6,
        ways: keep.clone(),
    };
    // What stands already, as circles on the floor: nothing new may touch.
    let mut taken: Vec<(DVec2, f64)> = vec![(c.centre, l.well.1 + 2.6)];
    let open = |p: DVec2, r: f64, taken: &[(DVec2, f64)]| {
        keep.iter().all(|&(q, k)| p.distance(q) > r + k)
            && taken.iter().all(|&(q, k)| p.distance(q) > r + k + 0.3)
            && c.chambers.iter().any(|&i| u.voids[i].covers(p, -(r + 0.9)))
    };
    let hall_l = |x: f64, y: f64| hall.world(DVec2::new(x, y));
    let (hx, hy) = (hall.max.x, hall.max.y);
    // Ten houses round the hall, doors to its middle (fewer where a way in
    // comes through).
    for k in 0..10 {
        let a = TAU * k as f64 / 10.0 + 0.3;
        let at = hall_l(det::cos(a) * (hx - 6.0), det::sin(a) * (hy - 6.0));
        let stem = 2.0 + (k % 3) as f64 * 0.4;
        let h = MoleHouse {
            at,
            stem,
            height: 3.6 + (k % 2) as f64,
            cap: stem * 2.1,
            door: Some((c.centre - at).normalize_or(DVec2::X)),
            hall: true,
            look: mix(seed, k),
        };
        if open(at, h.foot() + 1.0, &taken) {
            taken.push((at, h.foot()));
            l.houses.push(h);
        }
    }
    // Two rock columns at the hall's ends, holding up the roof.
    for s in [1.0, -1.0] {
        let at = hall_l(s * (hx - 3.6), 0.0);
        if open(at, 1.8, &taken) {
            taken.push((at, 1.8));
            l.pillars.push((at, 1.8));
        }
    }
    // Market stalls round the well.
    for k in 0..5 {
        let a = TAU * k as f64 / 5.0 + 0.6;
        let d = DVec2::new(det::cos(a), det::sin(a));
        let at = c.centre + d * 9.0;
        if !open(at, 1.6, &taken) {
            continue;
        }
        taken.push((at, 1.6));
        l.stalls.push((at, d));
        l.fixtures.push(Fixture {
            kind: FixtureKind::Stall,
            at: DVec3::new(at.x, f, at.y),
            dir: d,
            size: DVec3::new(1.5, 2.6, 0.75),
            look: mix(seed, 100 + k),
        });
    }
    // Benches between the stalls, facing the well.
    for k in [0u64, 2, 4] {
        let a = TAU * (k as f64 + 0.5) / 5.0 + 0.6;
        let d = DVec2::new(det::cos(a), det::sin(a));
        let at = c.centre + d * 7.2;
        if open(at, 1.1, &taken) {
            taken.push((at, 1.05));
            l.fixtures.push(Fixture {
                kind: FixtureKind::Bench,
                at: DVec3::new(at.x, f, at.y),
                dir: -d,
                size: DVec3::new(1.0, 0.9, 0.3),
                look: mix(seed, 150 + k),
            });
        }
    }
    // Between neighbouring houses, a little out of the way: lantern posts,
    // barrels, pots and carts in turn; washing on a line between the stems.
    let hall_houses: Vec<MoleHouse> = l.houses.clone();
    for (k, pair) in hall_houses
        .iter()
        .zip(hall_houses.iter().cycle().skip(1))
        .enumerate()
    {
        let (a, b) = pair;
        if a.at.distance(b.at) > 16.0 {
            continue;
        }
        let mid = (a.at + b.at) * 0.5;
        let out = (mid - c.centre).normalize_or(DVec2::X);
        let along = (b.at - a.at).normalize_or(DVec2::X);
        let p = mid + out * 1.6;
        let look = mix(seed, 200 + k as u64);
        let (kind, size) = match k % 4 {
            0 => (FixtureKind::Lantern, DVec3::new(0.12, 2.6, 0.0)),
            1 => (FixtureKind::Barrel, DVec3::new(0.38, 0.95, 0.0)),
            2 => (FixtureKind::MushroomPot, DVec3::new(0.4, 0.7, 0.0)),
            _ => (FixtureKind::Cart, DVec3::new(0.55, 1.0, 0.8)),
        };
        let reach = if kind == FixtureKind::Cart {
            1.0
        } else {
            size.x
        };
        if open(p, reach, &taken) {
            taken.push((p, reach));
            l.fixtures.push(Fixture {
                kind,
                at: DVec3::new(p.x, f, p.y),
                dir: if kind == FixtureKind::Cart {
                    along
                } else {
                    -out
                },
                size,
                look,
            });
            // A second barrel beside the first.
            if kind == FixtureKind::Barrel {
                let q = p + along * 0.85;
                if open(q, 0.34, &taken) {
                    taken.push((q, 0.34));
                    l.fixtures.push(Fixture {
                        kind,
                        at: DVec3::new(q.x, f, q.y),
                        dir: -out,
                        size: DVec3::new(0.34, 0.85, 0.0),
                        look: look ^ 7,
                    });
                }
            }
        }
        if k % 2 == 1 {
            let (ea, eb) = (a.at + along * a.stem, b.at - along * b.stem);
            let half = ea.distance(eb) * 0.5;
            if half > 1.5 {
                let m = (ea + eb) * 0.5;
                l.fixtures.push(Fixture {
                    kind: FixtureKind::Washing,
                    at: DVec3::new(m.x, f + 3.0, m.y),
                    dir: along,
                    size: DVec3::new(half, 0.0, 0.0),
                    look,
                });
            }
        }
    }
    // A rug before each door.
    for (k, h) in hall_houses.iter().enumerate() {
        let Some(d) = h.door else { continue };
        let p = h.at + d * (h.foot() + 0.9);
        l.fixtures.push(Fixture {
            kind: FixtureKind::Rug,
            at: DVec3::new(p.x, f + 0.01, p.y),
            dir: d.perp(),
            size: DVec3::new(0.9, 0.0, 0.6),
            look: mix(seed, 300 + k as u64),
        });
    }
    // Toadstool lamps round the hall, under the plan's lamps.
    for lamp in u.lamps.iter().filter(|p| (p.y - f).abs() < 4.0) {
        let p = DVec2::new(lamp.x, lamp.z);
        if open(p, 0.3, &taken) {
            taken.push((p, 0.25));
            l.toadstools.push(p);
        }
    }
    // Crystals and stalagmites along the walls.
    for k in 0..22 {
        let a = TAU * k as f64 / 22.0 + 0.11;
        let p = hall_l(det::cos(a) * (hx - 1.6), det::sin(a) * (hy - 1.6));
        let (kind, size) = if k % 2 == 0 {
            let h = 1.4 + (k % 4) as f64 * 0.6;
            (FixtureKind::Crystal, DVec3::new(h * 0.25, h, 0.0))
        } else {
            let h = 1.6 + (mix(seed, 400 + k) % 100) as f64 * 0.025;
            (FixtureKind::Stalagmite, DVec3::new(0.5 + h * 0.1, h, 0.0))
        };
        let reach = size.x;
        if open(p, reach, &taken) {
            taken.push((p, reach));
            l.fixtures.push(Fixture {
                kind,
                at: DVec3::new(p.x, f, p.y),
                dir: DVec2::X,
                size,
                look: mix(seed, 500 + k),
            });
        }
    }
    // Stalactites hang from the roof, clear of the bore (the roof's rock
    // lies up to 3 m above the void's ceiling: their roots are in it).
    for k in 0..34u64 {
        let h = mix(seed, 600 + k);
        let x = ((h & 0xFFFF) as f64 / 65535.0 * 2.0 - 1.0) * (hx - 3.0);
        let y = (((h >> 16) & 0xFFFF) as f64 / 65535.0 * 2.0 - 1.0) * (hy - 3.0);
        let at = hall_l(x, y);
        // Not over a way in either: a stair down from the street comes
        // through the roof there.
        if at.distance(c.centre) < BORE_R + 3.0
            || keep.iter().any(|&(q, k)| at.distance(q) < k + 1.5)
        {
            continue;
        }
        let len = 3.5 + (h % 7) as f64 * 0.6;
        l.stalactites.push(Stalactite {
            at,
            top: f + HALL_HEIGHT + 3.0,
            tip: f + HALL_HEIGHT + 3.0 - len,
            r: 0.45 + (h % 5) as f64 * 0.12,
        });
    }
    // The side chambers: each has a job.
    let jobs = [
        MoleRoom::Workshop,
        MoleRoom::Farm,
        MoleRoom::Shrine,
        MoleRoom::Cottages,
    ];
    for (n, &i) in c.chambers.iter().skip(1).enumerate() {
        let job = jobs[n % jobs.len()];
        l.rooms.push((i, job));
        room(
            u,
            i,
            job,
            f,
            seed ^ (n as u64 + 1),
            &open,
            &mut taken,
            &mut l,
        );
    }
    Some(l)
}

/// Is a circle (centre, radius) clear of the ways in and of what stands
/// already (`taken`), and on the cavern's floor?
type Open<'a> = dyn Fn(DVec2, f64, &[(DVec2, f64)]) -> bool + 'a;

/// One side chamber's furniture, by its job.
#[allow(clippy::too_many_arguments)]
fn room(
    u: &Underground,
    i: usize,
    job: MoleRoom,
    f: f64,
    seed: u64,
    open: &Open<'_>,
    taken: &mut Vec<(DVec2, f64)>,
    l: &mut MolehavenLayout,
) {
    let v = &u.voids[i];
    let centre = u.cavern.as_ref().map_or(v.origin, |c| c.centre);
    // The chamber's own frame: `back` from its middle to the wall away from
    // the hall, `across` along that wall.
    let mid_l = (v.min + v.max) * 0.5;
    let half = (v.max - v.min) * 0.5;
    let to_hall = v.local(centre) - mid_l;
    let (back_l, depth, width) = if to_hall.x.abs() > to_hall.y.abs() {
        (DVec2::new(-to_hall.x.signum(), 0.0), half.x, half.y)
    } else {
        (DVec2::new(0.0, -to_hall.y.signum()), half.y, half.x)
    };
    let across_l = back_l.perp();
    let at = |b: f64, a: f64| v.world(mid_l + back_l * b + across_l * a);
    let back = (at(1.0, 0.0) - at(0.0, 0.0)).normalize_or(DVec2::X);
    let across = back.perp();
    let mut k = 0u64;
    let mut put = |l: &mut MolehavenLayout,
                   taken: &mut Vec<(DVec2, f64)>,
                   kind: FixtureKind,
                   p: DVec2,
                   dir: DVec2,
                   size: DVec3,
                   reach: f64|
     -> bool {
        k += 1;
        if reach > 0.0 && !open(p, reach, taken) {
            return false;
        }
        if reach > 0.0 {
            taken.push((p, reach));
        }
        l.fixtures.push(Fixture {
            kind,
            at: DVec3::new(p.x, f, p.y),
            dir,
            size,
            look: mix(seed, k),
        });
        true
    };
    let wall = depth - 1.6;
    match job {
        MoleRoom::Workshop => {
            for a in [-3.0, 3.0] {
                put(
                    l,
                    taken,
                    FixtureKind::Workbench,
                    at(wall, a),
                    -back,
                    DVec3::new(1.4, 0.95, 0.45),
                    1.5,
                );
            }
            put(
                l,
                taken,
                FixtureKind::Pump,
                at(wall - 0.4, width - 2.4),
                -back,
                DVec3::new(0.7, 2.2, 0.0),
                0.8,
            );
            for (b, a) in [(0.0, -(width - 2.6)), (-3.5, -(width - 2.6))] {
                put(
                    l,
                    taken,
                    FixtureKind::Cart,
                    at(b, a),
                    back,
                    DVec3::new(0.55, 1.0, 0.8),
                    1.0,
                );
            }
            for (b, a) in [(wall, -(width - 1.6)), (wall - 0.9, -(width - 1.4))] {
                put(
                    l,
                    taken,
                    FixtureKind::Barrel,
                    at(b, a),
                    back,
                    DVec3::new(0.38, 0.95, 0.0),
                    0.4,
                );
            }
            for (b, a) in [(2.0, width - 1.8), (-2.0, width - 1.8)] {
                put(
                    l,
                    taken,
                    FixtureKind::Crate,
                    at(b, a),
                    back,
                    DVec3::new(0.42, 0.75, 0.42),
                    0.6,
                );
            }
            for a in [-6.5, 6.5] {
                put(
                    l,
                    taken,
                    FixtureKind::Lantern,
                    at(wall - 1.0, a),
                    -back,
                    DVec3::new(0.12, 2.6, 0.0),
                    0.2,
                );
            }
        }
        MoleRoom::Farm => {
            // Rows of pots in the back half, giant mushrooms in the corners.
            let rows = ((depth - 2.0) / 2.2).floor().clamp(2.0, 4.0) as i32;
            let cols = ((width * 2.0 - 4.0) / 1.8).floor().clamp(3.0, 8.0) as i32;
            for r in 0..rows {
                for c in 0..cols {
                    let a = -(cols - 1) as f64 * 0.9 + c as f64 * 1.8;
                    let b = wall - 0.6 - r as f64 * 2.2;
                    if b < -1.0 {
                        continue;
                    }
                    put(
                        l,
                        taken,
                        FixtureKind::MushroomPot,
                        at(b, a),
                        -back,
                        DVec3::new(0.4, 0.7, 0.0),
                        0.42,
                    );
                }
            }
            for a in [-(width - 2.0), width - 2.0] {
                put(
                    l,
                    taken,
                    FixtureKind::GiantMushroom,
                    at(wall - 0.4, a),
                    -back,
                    DVec3::new(1.2, 2.8, 0.0),
                    0.6,
                );
                put(
                    l,
                    taken,
                    FixtureKind::GiantMushroom,
                    at(-depth + 3.0, a),
                    -back,
                    DVec3::new(1.0, 2.3, 0.0),
                    0.5,
                );
            }
            put(
                l,
                taken,
                FixtureKind::Barrel,
                at(-1.5, width - 1.6),
                back,
                DVec3::new(0.38, 0.95, 0.0),
                0.4,
            );
            put(
                l,
                taken,
                FixtureKind::Cart,
                at(-1.5, -(width - 2.2)),
                back,
                DVec3::new(0.55, 1.0, 0.8),
                1.0,
            );
        }
        MoleRoom::Shrine => {
            put(
                l,
                taken,
                FixtureKind::Idol,
                at(wall - 0.2, 0.0),
                -back,
                DVec3::new(0.9, 2.6, 0.0),
                1.0,
            );
            put(
                l,
                taken,
                FixtureKind::Candles,
                at(wall - 1.9, 0.0),
                -back,
                DVec3::new(0.5, 0.3, 0.3),
                0.0,
            );
            put(
                l,
                taken,
                FixtureKind::Rug,
                at(wall - 4.0, 0.0),
                across,
                DVec3::new(1.4, 0.0, 0.9),
                0.0,
            );
            put(
                l,
                taken,
                FixtureKind::GlowPool,
                at(-1.0, width - 3.4),
                back,
                DVec3::new(1.4, 0.0, 1.0),
                0.0,
            );
            for a in [-3.2, 3.2] {
                put(
                    l,
                    taken,
                    FixtureKind::GiantMushroom,
                    at(wall - 0.6, a),
                    -back,
                    DVec3::new(1.1, 2.6, 0.0),
                    0.55,
                );
                put(
                    l,
                    taken,
                    FixtureKind::Lantern,
                    at(wall - 4.0, a * 0.8),
                    -back,
                    DVec3::new(0.12, 2.4, 0.0),
                    0.2,
                );
            }
            for (b, a) in [(wall - 1.0, -(width - 1.8)), (wall - 2.6, width - 1.8)] {
                put(
                    l,
                    taken,
                    FixtureKind::Crystal,
                    at(b, a),
                    -back,
                    DVec3::new(0.5, 2.0, 0.0),
                    0.4,
                );
            }
            put(
                l,
                taken,
                FixtureKind::MushroomPot,
                at(wall - 1.4, 1.6),
                -back,
                DVec3::new(0.35, 0.6, 0.0),
                0.4,
            );
            put(
                l,
                taken,
                FixtureKind::MushroomPot,
                at(wall - 1.4, -1.6),
                -back,
                DVec3::new(0.35, 0.6, 0.0),
                0.4,
            );
        }
        MoleRoom::Cottages => {
            // Three small houses round a yard, doors to its middle.
            let yard = at(-1.0, 0.0);
            for (b, a) in [
                (wall - 1.6, -(width - 4.2)),
                (wall - 1.2, width - 4.2),
                (-depth + 4.6, width - 4.0),
            ] {
                let p = at(b, a);
                let stem = 1.5;
                let h = MoleHouse {
                    at: p,
                    stem,
                    height: 3.0,
                    cap: 3.4,
                    door: Some((yard - p).normalize_or(-back)),
                    hall: false,
                    look: mix(seed, 50 + l.houses.len() as u64),
                };
                if open(p, h.foot() + 0.6, taken) {
                    taken.push((p, h.foot()));
                    l.houses.push(h);
                }
            }
            let (wa, wb) = (at(wall - 1.0, -2.6), at(wall - 1.0, 2.6));
            l.fixtures.push(Fixture {
                kind: FixtureKind::Washing,
                at: DVec3::new((wa.x + wb.x) * 0.5, f + 2.6, (wa.y + wb.y) * 0.5),
                dir: across,
                size: DVec3::new(2.6, 0.0, 0.0),
                look: mix(seed, 77),
            });
            put(
                l,
                taken,
                FixtureKind::Bench,
                at(wall - 3.4, 0.0),
                -back,
                DVec3::new(1.0, 0.9, 0.3),
                1.1,
            );
            put(
                l,
                taken,
                FixtureKind::Barrel,
                at(wall - 0.6, 0.0),
                -back,
                DVec3::new(0.38, 0.95, 0.0),
                0.4,
            );
            put(
                l,
                taken,
                FixtureKind::Lantern,
                at(-1.0, -2.5),
                back,
                DVec3::new(0.12, 2.4, 0.0),
                0.2,
            );
            put(
                l,
                taken,
                FixtureKind::Rug,
                yard,
                across,
                DVec3::new(1.2, 0.0, 0.8),
                0.0,
            );
        }
    }
    // Every chamber: strings of lamps overhead, and crystals and
    // stalagmites along its side walls and back corners.
    l.fixtures.push(Fixture {
        kind: FixtureKind::LampString,
        at: DVec3::new(0.0, f + 4.4, 0.0) + {
            let m = at(0.0, 0.0);
            DVec3::new(m.x, 0.0, m.y)
        },
        dir: back,
        size: DVec3::new(depth - 1.5, 0.0, width - 1.5),
        look: mix(seed, 90),
    });
    for k in 0..10u64 {
        let h = mix(seed, 120 + k);
        let t = (h % 1000) as f64 / 1000.0;
        // Along one side wall (k even: one side, odd: the other), from the
        // hall's end to the back corner.
        let side = if k % 2 == 0 { 1.0 } else { -1.0 };
        let p = at(-depth * 0.6 + t * (depth * 1.6 - 1.4), side * (width - 1.1));
        let (kind, size) = if k % 3 == 0 {
            let s = 1.6 + (h >> 12) as f64 % 5.0 * 0.4;
            (FixtureKind::Stalagmite, DVec3::new(0.5 + s * 0.1, s, 0.0))
        } else {
            let s = 1.2 + (h >> 12) as f64 % 4.0 * 0.5;
            (FixtureKind::Crystal, DVec3::new(s * 0.25, s, 0.0))
        };
        put(l, taken, kind, p, DVec2::X, size, size.x);
    }
    // Two neighbours talking, a little in from the hall.
    let a = at(-depth * 0.35, -0.5);
    let b = a + across * 1.0 + back * 0.3;
    if open(a, 0.5, taken) && open(b, 0.5, taken) {
        taken.push((a, 0.4));
        taken.push((b, 0.4));
        l.chats.push((a, (b - a).normalize()));
        l.chats.push((b, (a - b).normalize()));
    }
}

/// The gate over Molehaven's stair, in the stair's own frame (x across the
/// door, z towards it, metres from the shaft's centre and the ground):
/// its stone piers' centres and half size, and their height.
pub const GATE_PIER: (DVec2, f64, f64) =
    (DVec2::new(SHAFT_HALF + 0.55, SHAFT_HALF - 0.2), 0.42, 3.0);
/// The arch over the door between the piers: its springing height and
/// radius (its crown is at `GATE_PIER.2 + GATE_ARCH`).
pub const GATE_ARCH: f64 = SHAFT_HALF + 0.55;
/// The mole emblem on the arch's crown: its centre above the ground and
/// radius.
pub const GATE_EMBLEM: (f64, f64) = (GATE_PIER.2 + GATE_ARCH + 0.95, 0.95);

/// The gate's solid parts: the piers, the arch (as blocks along it), the
/// emblem.
pub fn gate_footprints(e: &Entrance) -> Vec<Footprint> {
    let sp: &Spiral = &e.spiral;
    let side = DVec2::new(sp.dir.y, -sp.dir.x);
    let (pier, ph, py) = GATE_PIER;
    let mut out = Vec::new();
    for sx in [-1.0, 1.0] {
        out.push(Footprint::Box {
            at: sp.c + side * (pier.x * sx) + sp.dir * pier.y,
            dir: sp.dir,
            half: DVec2::splat(ph),
            y0: e.top - 0.2,
            y1: e.top + py,
        });
    }
    let n = 7;
    for k in 0..n {
        let a = std::f64::consts::PI * (k as f64 + 0.5) / n as f64;
        let x = -det::cos(a) * GATE_ARCH;
        let y = py + det::sin(a) * GATE_ARCH;
        out.push(Footprint::Box {
            at: sp.c + side * x + sp.dir * pier.y,
            dir: sp.dir,
            half: DVec2::new(0.55, 0.3),
            y0: e.top + y - 0.3,
            y1: e.top + y + 0.3,
        });
    }
    out.push(Footprint::Disc {
        at: sp.c + sp.dir * pier.y,
        r: 0.25,
        y0: e.top + GATE_EMBLEM.0 - GATE_EMBLEM.1,
        y1: e.top + GATE_EMBLEM.0 + GATE_EMBLEM.1,
    });
    out
}

/// A signpost on the street pointing the way to Molehaven's gate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Signpost {
    pub pos: DVec2,
    /// The ground at its foot.
    pub ground: f64,
    /// The way its arrow points (unit, plan).
    pub point: DVec2,
}

/// A signpost's height, and its arm's length and height.
pub const POST_H: f64 = 3.1;
pub const ARM: (f64, f64) = (1.05, 2.45);

impl Signpost {
    pub fn footprints(&self) -> Vec<Footprint> {
        vec![
            Footprint::Disc {
                at: self.pos,
                r: 0.1,
                y0: self.ground,
                y1: self.ground + POST_H,
            },
            Footprint::Box {
                at: self.pos + self.point * (ARM.0 * 0.5 + 0.05),
                dir: self.point,
                half: DVec2::new(0.06, ARM.0 * 0.5),
                y0: self.ground + ARM.1 - 0.3,
                y1: self.ground + ARM.1 + 0.3,
            },
        ]
    }
}

/// The sign hung in a utility tunnel where the ramp down to Molehaven
/// leaves it: where it hangs (centre, its bottom) and the way down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RampSign {
    pub at: DVec2,
    pub bottom: f64,
    /// The way its face looks (along the tunnel; it is read from both).
    pub face: DVec2,
    /// The way down the ramp.
    pub point: DVec2,
}

/// The tunnel sign's half width and height.
pub const RAMP_SIGN: (f64, f64) = (0.75, 0.55);

impl RampSign {
    pub fn footprints(&self) -> Vec<Footprint> {
        vec![Footprint::Box {
            at: self.at,
            dir: self.face,
            half: DVec2::new(RAMP_SIGN.0, 0.05),
            y0: self.bottom,
            y1: self.bottom + RAMP_SIGN.1 * 2.0,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::urban::world::TownWorld;
    use crate::urban::TownParams;
    use hw_core::Seed;

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn molehaven_is_signposted_furnished_and_its_ways_in_are_open() {
        for seed in [1u64, 3, 11] {
            let w = TownWorld::generate(Seed(seed), TownParams::sample(Seed(seed), 40_000.0));
            let u = w.underground();
            let gate = u.entrances.iter().find(|e| e.molehaven).expect("a gate");
            // Signposts on the streets round it, each pointing at it.
            assert!(
                u.signs.len() >= 3,
                "seed {seed}: {} signposts",
                u.signs.len()
            );
            for s in &u.signs {
                let to = (gate.pos - s.pos).normalize();
                assert!(s.point.dot(to) > 0.999, "seed {seed}: a sign points away");
                assert!(s.pos.distance(gate.pos) < 300.0);
            }
            let c = u.cavern.as_ref().expect("Molehaven");
            // Where a ramp from the tunnels fits (not in every town), a sign
            // hangs over it.
            assert_eq!(c.ramp.is_some(), c.ramp_sign.is_some(), "seed {seed}");
            let l = molehaven_layout(u).expect("a layout");
            assert!(
                l.houses.iter().filter(|h| h.hall).count() >= 7,
                "seed {seed}: houses"
            );
            assert!(
                l.fixtures.len() >= 40,
                "seed {seed}: {} fixtures",
                l.fixtures.len()
            );
            assert!(l.rooms.len() >= 2, "seed {seed}: {} rooms", l.rooms.len());
            // Nothing stands in a way in: the stair's foot, the ramp, the
            // stair on down to the Deeps.
            let low: Vec<Footprint> = l
                .footprints()
                .into_iter()
                .filter(|f| f.span().0 < l.floor + 2.2)
                .collect();
            for (p, r) in ways(u) {
                for f in &low {
                    assert!(
                        f.distance(p) > r - 2.6 + 0.3,
                        "seed {seed}: {f:?} stands in a way in at {p:?}"
                    );
                }
            }
            // Nothing hangs over the stairs either, at any height.
            for e in u
                .entrances
                .iter()
                .filter(|e| (e.bottom - l.floor).abs() < 1.0)
            {
                for f in l.footprints() {
                    assert!(
                        f.distance(e.pos) > SHAFT_HALF * 1.42 + 0.3,
                        "seed {seed}: {f:?} in the stair's shaft"
                    );
                }
            }
            // Everything stands on the cavern's floor, inside it.
            for f in &low {
                let p = f.at();
                assert!(
                    c.chambers.iter().any(|&i| u.voids[i].covers(p, 0.0)),
                    "seed {seed}: {f:?} out in the rock"
                );
            }
        }
    }
}
