//! Buildings on the blocks of one piece.
//!
//! Buildings stand along the streets around a block, fronts to the street,
//! the way towns fill their blocks: a closed row of narrow houses in an old
//! town, apartment rows round a courtyard in the extensions, detached houses
//! with front gardens in a garden suburb, towers set back on lawns in a
//! superblock, sheds in the works. Each block edge is walked and lots are
//! cut along it; a building is kept only if it stays clear of every street
//! around the block (by that street's half width and pavement) and of its
//! neighbours. Corners go to the busier street, which is walked first.
//!
//! Like the streets, a piece's buildings come from its own seed, so they can
//! be made one piece at a time.

use super::graph::Face;
use super::growth::{simplify, CentreKind, Era};
use super::roads::RoadClass;
use super::streets::{piece_seed, PieceStreets, StreetStyle};
use super::{inner_point, Piece, Town, OUTLINE};
use crate::building::{BuildingSite, BuildingUse, CORE_SIZE, CORRIDOR_WIDTH};
use crate::city::{DistrictKind, BUILDING_GAP};
use crate::style::{Quarter, Region, Vernacular};
use crate::water::segment_distance;
use glam::DVec2;
use hw_core::{Rng, Seed, StableId};

/// The narrowest building front the building planner accepts (m).
pub const MIN_WIDTH: f64 = 10.0;
/// The shallowest building it accepts (m).
pub const MIN_DEPTH: f64 = CORE_SIZE.y + CORRIDOR_WIDTH;

/// How a kind of quarter builds.
#[derive(Clone, Copy, Debug)]
struct Programme {
    district: DistrictKind,
    /// Uses, with weights.
    uses: [(BuildingUse, f64); 2],
    width: (f64, f64),
    depth: (f64, f64),
    floors: (u32, u32),
    /// From the pavement to the front.
    setback: (f64, f64),
    /// Between neighbours along a street.
    gap: (f64, f64),
}

/// Half width plus pavement of a side street in this style.
pub fn side_reach(style: StreetStyle) -> f64 {
    match style {
        StreetStyle::Lanes => 5.0,
        StreetStyle::Garden => 7.5,
        StreetStyle::Superblock | StreetStyle::Industrial => 10.0,
        StreetStyle::RetroFuture => 11.0,
        _ => 8.5,
    }
}

/// Half width plus pavement of a main road.
pub fn road_reach(class: RoadClass) -> f64 {
    match class {
        RoadClass::Country => 6.5,
        RoadClass::Boulevard => 16.0,
        RoadClass::Avenue => 12.0,
        RoadClass::Quay => 10.0,
    }
}

/// The buildings of one piece, given its streets and blocks.
pub fn plan_buildings(
    seed: Seed,
    town: &Town,
    index: usize,
    piece: &Piece,
    streets: &PieceStreets,
) -> Vec<BuildingSite> {
    if streets.style == StreetStyle::Park {
        return Vec::new();
    }
    let region = region_of(seed);
    let seed = piece_seed(seed, piece).child("buildings");
    let mut rng = seed.rng();
    let p = &town.params;
    let centre = inner_point(&piece.face.pts).unwrap_or(piece.face.pts[0]);
    // The draw that once scattered dystopia anywhere keeps the rest of the
    // sequence; the villains now hold one quarter, block by block.
    let _ = rng.chance(p.dystopia);
    let quarter = town.dystopia.as_ref().filter(|d| d.pieces.contains(&index));
    let business = town
        .growth
        .centres
        .iter()
        .filter(|c| c.kind == CentreKind::Business)
        .map(|c| c.pos.distance(centre))
        .fold(f64::MAX, f64::min);
    let core = town
        .growth
        .centres
        .iter()
        .find(|c| c.kind == CentreKind::Core)
        .map_or(f64::MAX, |c| c.pos.distance(centre));
    let s = p.scale();
    let prog = programme(
        streets.style,
        piece.era,
        false,
        business < s * 0.12 + 150.0,
        core < s * 0.15 + 120.0,
        p.population,
        &mut rng,
    );
    // The old town and its extensions build in the old ways; everything
    // since is the golden age's own retro-future.
    let vernacular = (piece.era != Era::Modern)
        .then(|| quarter_of(streets.style, &prog))
        .flatten()
        .map(|quarter| Vernacular { region, quarter });
    // The villains' blocks (this programme draws nothing from `rng`).
    let villains = quarter.map(|_| {
        programme(
            streets.style,
            piece.era,
            true,
            false,
            false,
            p.population,
            &mut rng,
        )
    });
    let mut main = main_roads(town, &piece.face);
    if piece.era == Era::Old {
        // Even the high street of an old town is narrow.
        for m in &mut main {
            m.1 = m.1.min(7.0);
        }
    }
    let side = side_reach(streets.style);
    let mut out = Vec::new();
    for (k, block) in streets.blocks.iter().enumerate() {
        let bseed = seed.child(k as u32);
        let held = quarter
            .zip(villains.as_ref())
            .filter(|(d, _)| inner_point(block).is_some_and(|c| d.contains(c)));
        match held {
            Some((_, v)) => fill_block(block, &main, side, v, None, bseed, &mut out),
            None => fill_block(block, &main, side, &prog, vernacular, bseed, &mut out),
        }
    }
    out
}

/// How a town builds by tradition: one region per town, from its seed.
pub fn region_of(town_seed: Seed) -> Region {
    let mut rng = town_seed.child("region").rng();
    *rng.pick(&Region::ALL)
}

/// The kind of quarter a programme builds, if it builds in the old ways
/// (business districts, towers, works, the expo and the dystopia do not).
fn quarter_of(style: StreetStyle, prog: &Programme) -> Option<Quarter> {
    use DistrictKind::*;
    if matches!(prog.district, Dystopia | AetherWorks | Expo)
        || prog.uses[0].0 == BuildingUse::Offices
    {
        return None;
    }
    match style {
        StreetStyle::Lanes => Some(Quarter::OldTown),
        StreetStyle::Garden => Some(Quarter::Suburb),
        StreetStyle::Organic if prog.uses[0].0 == BuildingUse::House => Some(Quarter::Suburb),
        StreetStyle::Organic | StreetStyle::Grid | StreetStyle::Baroque => Some(Quarter::Terraces),
        _ => None,
    }
}

/// The main roads around a piece: (polyline, reach); the town's edge has no
/// road and takes reach 0.
fn main_roads(town: &Town, face: &Face) -> Vec<(Vec<DVec2>, f64)> {
    face.edges
        .iter()
        .map(|(e, _)| {
            let e = &town.graph.edges[*e];
            let reach = match e.tag {
                OUTLINE => 0.0,
                t => road_reach(match t {
                    0 => RoadClass::Country,
                    1 => RoadClass::Boulevard,
                    2 => RoadClass::Avenue,
                    _ => RoadClass::Quay,
                }),
            };
            (e.pts.clone(), reach)
        })
        .collect()
}

fn programme(
    style: StreetStyle,
    era: Era,
    dystopia: bool,
    business: bool,
    core: bool,
    population: f64,
    rng: &mut Rng,
) -> Programme {
    use BuildingUse::*;
    use DistrictKind::*;
    // Towers grow with the town: a market town tops out at a few floors.
    let big = (hw_core::det::log10(population.max(1000.0)) - 3.5).clamp(0.0, 3.5);
    let tall = |lo: f64, hi: f64| ((lo * (0.5 + big)) as u32, (hi * (0.4 + big)) as u32);
    let pr = |district, uses, width, depth, floors: (u32, u32), setback, gap| Programme {
        district,
        uses,
        width,
        depth,
        floors: (floors.0.max(1), floors.1.max(floors.0.max(1))),
        setback,
        gap,
    };
    if dystopia && style != StreetStyle::Industrial {
        return pr(
            Dystopia,
            [(Tenement, 0.8), (Lab, 0.2)],
            (14.0, 26.0),
            (12.0, 18.0),
            (3 + big as u32, 5 + 2 * big as u32),
            (0.0, 1.0),
            (BUILDING_GAP, 2.0),
        );
    }
    if business && era != Era::Old && style != StreetStyle::Industrial {
        return pr(
            Commercial,
            [(Offices, 0.6), (ShopsAndOffices, 0.4)],
            (22.0, 40.0),
            (20.0, 32.0),
            tall(6.0, 14.0),
            (2.0, 6.0),
            (4.0, 10.0),
        );
    }
    match style {
        StreetStyle::Lanes => pr(
            if core { Civic } else { Residential },
            if core {
                [(ShopsAndOffices, 0.7), (Apartments, 0.3)]
            } else {
                [(Apartments, 0.6), (ShopsAndOffices, 0.4)]
            },
            // Narrow, deep burgage plots in an unbroken row.
            (MIN_WIDTH, 13.0),
            (14.0, 24.0),
            (2, 3 + (big as u32).min(2)),
            (0.0, 0.0),
            (BUILDING_GAP, BUILDING_GAP + 0.2),
        ),
        StreetStyle::Organic if era == Era::Modern => {
            if rng.chance(0.6) {
                pr(
                    Residential,
                    [(House, 1.0), (Apartments, 0.0)],
                    (MIN_WIDTH, 14.0),
                    (MIN_DEPTH, 13.0),
                    (1, 2),
                    (3.0, 7.0),
                    (3.0, 6.0),
                )
            } else {
                pr(
                    Residential,
                    [(Apartments, 0.8), (ShopsAndOffices, 0.2)],
                    (16.0, 28.0),
                    (12.0, 16.0),
                    (3, 4 + big as u32),
                    (2.0, 5.0),
                    (4.0, 8.0),
                )
            }
        }
        StreetStyle::Organic => pr(
            if core { Commercial } else { Residential },
            [(Apartments, 0.7), (ShopsAndOffices, 0.3)],
            (12.0, 22.0),
            (11.0, 16.0),
            (3, 4 + big as u32),
            (0.0, 1.5),
            (BUILDING_GAP, 2.5),
        ),
        StreetStyle::Grid | StreetStyle::Baroque => pr(
            if core { Commercial } else { Residential },
            [(Apartments, 0.65), (ShopsAndOffices, 0.35)],
            (16.0, 30.0),
            (15.0, 22.0),
            (4, 5 + 2 * big as u32),
            (0.0, 0.5),
            (BUILDING_GAP, 1.5),
        ),
        StreetStyle::Garden => pr(
            Residential,
            [(House, 1.0), (Apartments, 0.0)],
            (MIN_WIDTH, 13.0),
            (MIN_DEPTH, 12.0),
            (1, 2),
            (5.0, 9.0),
            (4.0, 8.0),
        ),
        StreetStyle::Superblock => pr(
            Residential,
            [(Apartments, 0.85), (Offices, 0.15)],
            (24.0, 40.0),
            (16.0, 22.0),
            tall(6.0, 12.0),
            (8.0, 16.0),
            (14.0, 24.0),
        ),
        StreetStyle::RetroFuture => pr(
            Expo,
            [(Pavilion, 0.55), (Offices, 0.45)],
            (26.0, 48.0),
            (22.0, 36.0),
            (1, 3 + big as u32 * 3),
            (6.0, 14.0),
            (10.0, 20.0),
        ),
        StreetStyle::Industrial => pr(
            AetherWorks,
            [(Works, 0.85), (Lab, 0.15)],
            (28.0, 60.0),
            (24.0, 40.0),
            (1, 3),
            (3.0, 8.0),
            (6.0, 12.0),
        ),
        StreetStyle::Park => unreachable!("parks have no buildings"),
    }
}

/// An edge of a block: its ends and how far buildings keep from it.
#[derive(Clone, Copy)]
struct Side {
    a: DVec2,
    b: DVec2,
    reach: f64,
    /// Main roads are walked first and take the corners.
    rank: f64,
}

fn fill_block(
    block: &[DVec2],
    main: &[(Vec<DVec2>, f64)],
    side_reach: f64,
    prog: &Programme,
    vernacular: Option<Vernacular>,
    seed: Seed,
    out: &mut Vec<BuildingSite>,
) {
    let mut ring = block.to_vec();
    ring.push(block[0]);
    let mut ring = simplify(&ring, 1.5);
    ring.pop();
    if ring.len() < 3 || super::graph::signed_area(&ring) < MIN_WIDTH * MIN_DEPTH {
        return;
    }
    let n = ring.len();
    let sides: Vec<Side> = (0..n)
        .map(|k| {
            let (a, b) = (ring[k], ring[(k + 1) % n]);
            let m = (a + b) * 0.5;
            // On a main road (or the town's edge) if it lies along one.
            let on_main = main
                .iter()
                .filter(|(pts, _)| polyline_distance(pts, m) < 3.0)
                .map(|(_, r)| *r)
                .fold(None, |acc: Option<f64>, r| {
                    Some(acc.map_or(r, |x| x.max(r)))
                });
            let (reach, rank) = match on_main {
                Some(r) if r > 0.0 => (r, 1.0 + r),
                // The edge of town: keep a little back, but nothing fronts it.
                Some(_) => (2.0, -1.0),
                None => (side_reach, 1.0),
            };
            Side { a, b, reach, rank }
        })
        .collect();
    // Street fronts: runs of sides along one street, turning gently, walked
    // as one curve so buildings keep an even rhythm round a bend.
    let turn = |k: usize| {
        let (p, q) = (&sides[(k + n - 1) % n], &sides[k]);
        let (u, v) = (
            (p.b - p.a).normalize_or_zero(),
            (q.b - q.a).normalize_or_zero(),
        );
        u.dot(v) < 0.87 || (p.reach - q.reach).abs() > 0.1 || (p.rank > 0.0) != (q.rank > 0.0)
    };
    let breaks: Vec<usize> = (0..n).filter(|k| turn(*k)).collect();
    let starts = if breaks.is_empty() { vec![0] } else { breaks };
    let mut fronts: Vec<Vec<usize>> = starts
        .iter()
        .enumerate()
        .map(|(i, &s0)| {
            let s1 = if starts.len() == 1 {
                s0 + n
            } else {
                starts[(i + 1) % starts.len()]
            };
            let s1 = if s1 <= s0 { s1 + n } else { s1 };
            (s0..s1).map(|k| k % n).collect::<Vec<usize>>()
        })
        .filter(|f| sides[f[0]].rank > 0.0)
        .collect();
    let flen = |f: &[usize]| {
        f.iter()
            .map(|k| sides[*k].a.distance(sides[*k].b))
            .sum::<f64>()
    };
    fronts.sort_by(|x, y| {
        (sides[y[0]].rank * flen(y))
            .total_cmp(&(sides[x[0]].rank * flen(x)))
            .then(x[0].cmp(&y[0]))
    });
    let mut rng = seed.rng();
    let first = out.len();
    for front in fronts {
        let mut line: Vec<DVec2> = vec![sides[front[0]].a];
        line.extend(front.iter().map(|k| sides[*k].b));
        let len = flen(&front);
        let reach = sides[front[0]].reach;
        let mut t = 0.0;
        let mut setback = rng.range(prog.setback.0, prog.setback.1);
        while t + 2.0 <= len {
            let w = rng.range(prog.width.0, prog.width.1);
            let mut depth = rng.range(prog.depth.0, prog.depth.1);
            let mut placed = false;
            // Shrink a building that does not fit before giving up the spot.
            for _ in 0..3 {
                let width = w.max(MIN_WIDTH);
                let p0 = point_at(&line, t);
                let along = (point_at(&line, t + width) - p0).normalize_or_zero();
                if along == DVec2::ZERO {
                    break;
                }
                let inward = along.perp();
                let origin = p0 + inward * (reach + setback);
                let corners = [
                    origin,
                    origin + along * width,
                    origin + along * width + inward * depth,
                    origin + inward * depth,
                ];
                if fits(&ring, &sides, &front, &corners)
                    && out[first..]
                        .iter()
                        .all(|o| apart(o, &corners, along, inward))
                {
                    let id_seed = seed.child("b").child(out.len() as u32 - first as u32);
                    let use_w: Vec<f64> = prog.uses.iter().map(|u| u.1).collect();
                    let usage = prog.uses[rng.weighted(&use_w)].0;
                    let mut floors =
                        rng.range_int(prog.floors.0 as i64, prog.floors.1 as i64 + 1) as u32;
                    if usage == BuildingUse::House {
                        floors = floors.min(2);
                    }
                    // Narrow buildings stay low.
                    if width < 16.0 || depth < 16.0 {
                        floors = floors.min(6);
                    }
                    out.push(BuildingSite {
                        id: StableId::from_seed(id_seed),
                        seed: id_seed,
                        usage,
                        origin,
                        along,
                        inward,
                        size: DVec2::new(width, depth),
                        floors,
                        street: 0,
                        district: prog.district,
                        vernacular,
                    });
                    t += width + rng.range(prog.gap.0, prog.gap.1);
                    placed = true;
                    break;
                }
                depth = (depth * 0.75).max(MIN_DEPTH);
                setback *= 0.5;
            }
            if !placed {
                t += 1.5;
            }
        }
    }
}

/// The point at arc length `t` along a polyline, carried straight on past
/// its end.
fn point_at(line: &[DVec2], t: f64) -> DVec2 {
    let mut rest = t;
    for w in line.windows(2) {
        let l = w[0].distance(w[1]);
        if rest <= l {
            return w[0] + (w[1] - w[0]).normalize_or_zero() * rest;
        }
        rest -= l;
    }
    let (a, b) = (line[line.len() - 2], line[line.len() - 1]);
    b + (b - a).normalize_or_zero() * rest
}

/// Does a building with these corners stand inside the block, clear of every
/// street around it? `own` are the sides of the street it fronts.
fn fits(ring: &[DVec2], sides: &[Side], own: &[usize], c: &[DVec2; 4]) -> bool {
    let mid = (c[0] + c[2]) * 0.5;
    if !super::contains(ring, mid) || c.iter().any(|q| !super::contains(ring, *q)) {
        return false;
    }
    sides.iter().enumerate().all(|(k, s)| {
        // A building on a bend may cut a little into its own street's room.
        let need = if own.contains(&k) {
            s.reach - 1.0
        } else {
            s.reach
        };
        (0..4).all(|i| segment_gap(c[i], c[(i + 1) % 4], s.a, s.b) >= need)
    })
}

/// Are two footprints at least the building gap apart (separating axes)?
fn apart(o: &BuildingSite, c: &[DVec2; 4], along: DVec2, inward: DVec2) -> bool {
    let oc = o.corners();
    [o.along, o.inward, along, inward].iter().any(|ax| {
        let span = |c: &[DVec2; 4]| {
            c.iter()
                .map(|p| p.dot(*ax))
                .fold((f64::MAX, f64::MIN), |(l, h), v| (l.min(v), h.max(v)))
        };
        let ((a0, a1), (b0, b1)) = (span(&oc), span(c));
        a1 + BUILDING_GAP <= b0 + 1e-6 || b1 + BUILDING_GAP <= a0 + 1e-6
    })
}

/// Distance between two segments (0 if they cross).
fn segment_gap(p: DVec2, q: DVec2, a: DVec2, b: DVec2) -> f64 {
    let (d1, d2) = ((q - p).perp_dot(a - p), (q - p).perp_dot(b - p));
    let (d3, d4) = ((b - a).perp_dot(p - a), (b - a).perp_dot(q - a));
    if d1 * d2 < 0.0 && d3 * d4 < 0.0 {
        return 0.0;
    }
    segment_distance(p, a, b)
        .min(segment_distance(q, a, b))
        .min(segment_distance(a, p, q))
        .min(segment_distance(b, p, q))
}

fn polyline_distance(pts: &[DVec2], p: DVec2) -> f64 {
    pts.windows(2)
        .map(|w| segment_distance(p, w[0], w[1]))
        .fold(f64::MAX, f64::min)
}

#[cfg(test)]
mod tests {
    use super::super::params::TownParams;
    use super::super::streets::plan_piece;
    use super::*;

    #[test]
    fn blocks_fill_with_sound_buildings() {
        let mut uses = std::collections::BTreeSet::new();
        for s in 0..3u64 {
            let p = TownParams::sample(Seed(s), 40_000.0 + s as f64 * 20_000.0);
            let town = Town::generate(Seed(s), p);
            let mut total = 0;
            for (i, pc) in town.pieces.iter().enumerate() {
                let streets = plan_piece(Seed(s), &town, pc);
                let b = plan_buildings(Seed(s), &town, i, pc, &streets);
                for site in &b {
                    assert!(site.size.x >= MIN_WIDTH - 1e-9 && site.size.y >= MIN_DEPTH - 1e-9);
                    // Every building stands on dry land inside the piece.
                    let c = site.origin
                        + site.along * site.size.x * 0.5
                        + site.inward * site.size.y * 0.5;
                    assert!(super::super::contains(&pc.face.pts, c), "seed {s}");
                    assert!(!town.land.cover_at(c).is_water(), "seed {s}: in the water");
                }
                for (i, x) in b.iter().enumerate() {
                    for y in &b[i + 1..] {
                        assert!(
                            apart(x, &y.corners(), y.along, y.inward),
                            "seed {s}: buildings overlap"
                        );
                    }
                }
                uses.extend(b.iter().map(|x| format!("{:?}", x.usage)));
                total += b.len();
                // Same seed, same buildings.
                assert_eq!(b, plan_buildings(Seed(s), &town, i, pc, &streets));
            }
            assert!(total > 100, "seed {s}: only {total} buildings");
        }
        assert!(uses.len() >= 3, "{uses:?}");
    }
}
