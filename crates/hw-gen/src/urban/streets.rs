//! Side streets and blocks inside one piece of town.
//!
//! Each piece between main roads gets a street style from its age, the
//! town's planned share and its plan styles: crooked lanes in an old town,
//! a grid or baroque axes in a planned extension, curving streets with
//! cul-de-sacs in a garden suburb, big blocks at the docks. The streets are
//! traced along a direction field that lines up with the main roads around
//! the piece, so every quarter meets its neighbours at its own angle.
//!
//! A piece is planned from its own seed (keyed by where it lies), so pieces
//! can be planned one at a time as the player comes near.

use super::graph::{Graph, TaggedLine};
use super::growth::{simplify, Era};
use super::params::PlanStyle;
use super::tensor::{Basis, Field};
use super::trace::{trace, TraceSpec};
use super::{contains, Piece, Town};
use glam::DVec2;
use hw_core::{det, Noise, Seed};

/// How a piece's streets are laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StreetStyle {
    /// Crooked lanes and small squares (old towns).
    Lanes,
    /// Streets grown without a plan, gently curving.
    Organic,
    /// A planned grid.
    Grid,
    /// A grid with axes radiating from a square.
    Baroque,
    /// Curving streets, crescents and cul-de-sacs.
    Garden,
    /// Few streets, big blocks.
    Superblock,
    /// Rings and axes around a plaza.
    RetroFuture,
    /// Docks, works and yards.
    Industrial,
    /// A park: no streets.
    Park,
}

/// The streets and blocks of one piece.
#[derive(Clone, Debug, PartialEq)]
pub struct PieceStreets {
    pub style: StreetStyle,
    pub streets: Vec<Vec<DVec2>>,
    /// Blocks: the land between the streets (centre lines; road widths
    /// are taken off later).
    pub blocks: Vec<Vec<DVec2>>,
}

struct Look {
    /// Spacing of main streets and of cross streets (m).
    dsep: (f64, f64),
    wiggle: f64,
    wiggle_len: f64,
    dead_ends: f64,
    grid: f64,
    radial: f64,
}

fn look(style: StreetStyle) -> Look {
    let l = |dsep, wiggle, wiggle_len, dead_ends, grid, radial| Look {
        dsep,
        wiggle,
        wiggle_len,
        dead_ends,
        grid,
        radial,
    };
    match style {
        StreetStyle::Lanes => l((72.0, 48.0), 0.6, 130.0, 0.25, 0.0, 0.0),
        StreetStyle::Organic => l((95.0, 62.0), 0.35, 260.0, 0.12, 0.3, 0.0),
        StreetStyle::Grid => l((115.0, 72.0), 0.02, 400.0, 0.02, 3.0, 0.0),
        StreetStyle::Baroque => l((105.0, 80.0), 0.03, 400.0, 0.02, 1.5, 2.0),
        StreetStyle::Garden => l((95.0, 70.0), 0.6, 320.0, 0.45, 0.0, 0.0),
        StreetStyle::Superblock => l((230.0, 180.0), 0.04, 500.0, 0.05, 2.0, 0.0),
        StreetStyle::RetroFuture => l((120.0, 100.0), 0.0, 500.0, 0.02, 1.0, 2.5),
        StreetStyle::Industrial => l((210.0, 120.0), 0.04, 500.0, 0.1, 2.0, 0.0),
        StreetStyle::Park => l((1e9, 1e9), 0.0, 1.0, 0.0, 0.0, 0.0),
    }
}

/// The seed of a piece, from where it lies (stable when other pieces change).
pub fn piece_seed(seed: Seed, piece: &Piece) -> Seed {
    let c = super::inner_point(&piece.face.pts).unwrap_or(piece.face.pts[0]);
    seed.child("piece")
        .child(c.x.round() as i64)
        .child(c.y.round() as i64)
}

/// Plans the side streets and blocks of `piece`.
pub fn plan_piece(seed: Seed, town: &Town, piece: &Piece) -> PieceStreets {
    let seed = piece_seed(seed, piece);
    let mut rng = seed.rng();
    let outline = &piece.face.pts;
    let style = choose_style(&mut rng, town, piece);
    let lk = look(style);
    let k = rng.range(0.85, 1.2);
    let (dmaj, dmin) = (lk.dsep.0 * k, lk.dsep.1 * k);
    let area = piece.face.area;
    let mut streets = Vec::new();
    if style != StreetStyle::Park && area > dmaj * dmin * 1.6 {
        // Line up with the main roads around the piece.
        let longest = outline
            .iter()
            .zip(outline.iter().cycle().skip(1))
            .max_by(|a, b| a.0.distance(*a.1).total_cmp(&b.0.distance(*b.1)))
            .map(|(a, b)| *b - *a)
            .unwrap_or(DVec2::X);
        let centre = super::inner_point(outline).unwrap_or(outline[0]);
        let mut ring = outline.clone();
        ring.push(outline[0]);
        let mut basis = vec![Basis::Line {
            pts: simplify(&ring, 3.0),
            reach: dmaj * 2.0,
            weight: 1.0,
        }];
        // A faint grid along the longest side keeps the field alive in
        // the middle of big pieces; planned styles make it strong.
        basis.push(Basis::Grid {
            center: centre,
            angle: det::atan2(longest.y, longest.x),
            reach: 1e6,
            weight: (lk.grid * 0.3).max(0.05),
        });
        if lk.radial > 0.0 {
            basis.push(Basis::Radial {
                center: centre,
                reach: area.sqrt() * 0.6,
                weight: lk.radial,
            });
        }
        let wig = lk.wiggle;
        let field = Field {
            basis,
            noise: Noise::new(seed.child("wiggle")),
            wiggle_len: lk.wiggle_len,
            wiggle: Box::new(move |_| wig),
        };
        let inside = |q: DVec2| contains(outline, q);
        let dsep = |_: DVec2, major: bool| if major { dmaj } else { dmin };
        let spec = TraceSpec {
            field: &field,
            inside: &inside,
            dsep: &dsep,
            step: (dmin / 6.0).clamp(4.0, 15.0),
            max_len: area.sqrt() * 6.0,
            min_len: dmin * 0.6,
            join: 1.3,
            dead_ends: lk.dead_ends,
            random_seeds: 0,
            max_turn_cos: super::trace::STREET_TURN_COS,
        };
        let mut seeds = vec![centre];
        let (lo, hi) = bounds(outline);
        // Close enough that thin pieces get a seed too.
        let st = dmin * 0.8;
        let mut y = lo.y + st * 0.5;
        while y < hi.y {
            let mut x = lo.x + st * 0.5;
            while x < hi.x {
                let q = DVec2::new(x, y);
                if inside(q) {
                    seeds.push(q);
                }
                x += st;
            }
            y += st;
        }
        seeds.reverse();
        let lines = trace(
            &spec,
            &[ring.clone()],
            &seeds,
            &mut seed.child("trace").rng(),
        );
        streets = lines.into_iter().map(|l| simplify(&l.pts, 1.0)).collect();
    }
    // Blocks: the faces of the outline plus the streets. A loop of streets
    // that touches nothing would sit inside its block like a hole, so it is
    // tied to the outline by two short streets first.
    let mut blocks = Vec::new();
    for round in 0..3 {
        let mut tagged = vec![TaggedLine {
            pts: {
                let mut r = outline.clone();
                r.push(outline[0]);
                r
            },
            tag: 1,
        }];
        tagged.extend(streets.iter().map(|s| TaggedLine {
            pts: s.clone(),
            tag: 0,
        }));
        let faces: Vec<Vec<DVec2>> = Graph::build(&tagged, 1.0)
            .faces(dmin.min(60.0) * 4.0)
            .into_iter()
            // A street that strays past the outline must not make a block
            // outside the piece.
            .filter(|f| super::inner_point(&f.pts).is_some_and(|c| contains(outline, c)))
            .map(|f| f.pts)
            .collect();
        let inner = |b: &Vec<DVec2>| {
            super::inner_point(b).is_some_and(|c| {
                faces.iter().any(|o| {
                    !std::ptr::eq(o, b)
                        && super::graph::signed_area(o) > super::graph::signed_area(b)
                        && contains(o, c)
                })
            })
        };
        let islands: Vec<&Vec<DVec2>> = faces.iter().filter(|b| inner(b)).collect();
        if islands.is_empty() || round == 2 {
            // Last resort: drop any block that still holds another.
            let holds = |o: &Vec<DVec2>| {
                faces.iter().any(|b| {
                    !std::ptr::eq(o, b)
                        && super::graph::signed_area(o) > super::graph::signed_area(b)
                        && super::inner_point(b).is_some_and(|c| contains(o, c))
                })
            };
            blocks = faces.iter().filter(|o| !holds(o)).cloned().collect();
            break;
        }
        for isl in islands {
            let near = |q: DVec2| nearest_on(outline, q);
            let (k0, _) = isl
                .iter()
                .enumerate()
                .map(|(k, q)| (k, q.distance(near(*q))))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .expect("non-empty");
            let k1 = (k0 + isl.len() / 2) % isl.len();
            for k in [k0, k1] {
                let q = isl[k];
                let t = near(q);
                streets.push(vec![q, t + (t - q).normalize_or_zero() * 0.5]);
            }
        }
    }
    PieceStreets {
        style,
        streets,
        blocks,
    }
}

/// The nearest point on a closed outline.
fn nearest_on(ring: &[DVec2], p: DVec2) -> DVec2 {
    let n = ring.len();
    (0..n)
        .map(|k| {
            let (a, b) = (ring[k], ring[(k + 1) % n]);
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
            a + ab * t
        })
        .min_by(|x, y| x.distance(p).total_cmp(&y.distance(p)))
        .unwrap_or(p)
}

fn bounds(pts: &[DVec2]) -> (DVec2, DVec2) {
    pts.iter().fold(
        (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN)),
        |(lo, hi), q| (lo.min(*q), hi.max(*q)),
    )
}

fn choose_style(rng: &mut hw_core::Rng, town: &Town, piece: &Piece) -> StreetStyle {
    let p = &town.params;
    let g = &town.growth.time;
    let near_water = piece.face.pts.iter().step_by(4).any(|q| {
        [DVec2::X, DVec2::Y, -DVec2::X, -DVec2::Y]
            .iter()
            .any(|d| town.land.cover_at(*q + *d * g.cell * 1.5).is_water())
    });
    if piece.era != Era::Old && near_water && rng.chance(0.3) {
        return StreetStyle::Industrial;
    }
    if piece.era != Era::Old && rng.chance(p.green * 0.25) {
        return StreetStyle::Park;
    }
    let planned_chance = match piece.era {
        Era::Old => {
            if p.founding == super::params::Founding::Colony {
                0.9
            } else {
                0.0
            }
        }
        Era::Extension => (p.planned * 1.5).min(0.95),
        Era::Modern => p.planned,
    };
    if rng.chance(planned_chance) {
        let mut w = p.plan_styles;
        if piece.era == Era::Old {
            w = [1.0, 0.0, 0.0, 0.0, 0.0];
        }
        match piece.era {
            // Superblocks and the retro-future are modern ideas.
            Era::Extension => {
                w[3] = 0.0;
                w[4] = 0.0;
            }
            // Baroque axes belong to the age of kings.
            Era::Modern => w[1] *= 0.2,
            Era::Old => {}
        }
        return match PlanStyle::ALL[rng.weighted(&w)] {
            PlanStyle::Grid => StreetStyle::Grid,
            PlanStyle::Baroque => StreetStyle::Baroque,
            PlanStyle::GardenCity => StreetStyle::Garden,
            PlanStyle::Superblock => StreetStyle::Superblock,
            PlanStyle::RetroFuture => StreetStyle::RetroFuture,
        };
    }
    match piece.era {
        Era::Old => StreetStyle::Lanes,
        Era::Extension => StreetStyle::Organic,
        Era::Modern => {
            if rng.chance(0.55) {
                StreetStyle::Garden
            } else {
                StreetStyle::Organic
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::params::TownParams;
    use super::*;

    #[test]
    #[ignore = "on demand (looks): scripts/check.sh --on-demand"]
    fn pieces_fill_with_blocks_in_many_styles() {
        let mut styles = std::collections::BTreeSet::new();
        for s in 0..4u64 {
            let p = TownParams::sample(Seed(s), 30_000.0 + s as f64 * 15_000.0);
            let town = Town::generate(Seed(s), p);
            assert!(!town.pieces.is_empty(), "seed {s}");
            for pc in &town.pieces {
                let plan = plan_piece(Seed(s), &town, pc);
                styles.insert(format!("{:?}", plan.style));
                let covered: f64 = plan
                    .blocks
                    .iter()
                    .map(|b| super::super::graph::signed_area(b))
                    .sum();
                assert!(
                    covered <= pc.face.area * 1.05,
                    "seed {s}: blocks {covered} exceed piece {}",
                    pc.face.area
                );
            }
            // Same seed, same streets.
            let a = plan_piece(Seed(s), &town, &town.pieces[0]);
            let b = plan_piece(Seed(s), &town, &town.pieces[0]);
            assert_eq!(a, b);
        }
        assert!(styles.len() >= 3, "{styles:?}");
    }
}
