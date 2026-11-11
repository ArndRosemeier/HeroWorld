//! Stairs and the halls they stand in: grand stairs up to a gallery
//! (straight, twin, imperial, horseshoe), stairwells (dog-leg with a lift
//! bank, or a spiral round a cage lift).

use super::{Builder, Flight, Landing, Lift, LiftKind, SpaceKind, Stair, StairKind, RISE, TREAD};
use crate::geom::Rect;
use glam::{DVec2, DVec3};
use std::f64::consts::{FRAC_PI_2, PI};

/// Risers for a climb of `dy`, and their height.
pub(crate) fn risers(dy: f64) -> (u32, f64) {
    let n = (dy / RISE - 1e-9).ceil().max(1.0) as u32;
    (n, dy / n as f64)
}

/// A straight flight.
fn straight(x: f64, y: f64, z: f64, dir: DVec2, width: f64, risers: u32, rise: f64) -> Flight {
    Flight::Straight {
        from: DVec3::new(x, y, z),
        dir,
        width,
        risers,
        rise,
    }
}

/// A grand stair from a hall's floor up to a gallery, and where it arrives
/// on the gallery's edge (x ranges).
pub(crate) struct Grand {
    pub stair: Stair,
    pub arrivals: Vec<(f64, f64)>,
}

/// Width of a grand stair's main flight, and of twin and side flights (m).
const GRAND_WIDTH: f64 = 5.0;
const SIDE_WIDTH: f64 = 3.5;
/// Depth of a landing (m).
const LANDING: f64 = 3.0;
/// Floor kept free at a stair's foot (m).
const FOOT: f64 = 2.0;

/// Plans a grand stair of `kind` in `zone` (free floor of `host`, standing
/// at `y0`) up to `y1`, arriving on the edge `z_a` of a gallery that lies
/// behind it (`back`: at z > z_a) or in front. None if it doesn't fit.
pub(crate) fn grand(
    kind: StairKind,
    host: usize,
    zone: Rect,
    y0: f64,
    y1: f64,
    z_a: f64,
    back: bool,
) -> Option<Grand> {
    let s = if back { 1.0 } else { -1.0 };
    // Distance from the gallery edge into the hall, and back to z.
    let z = |u: f64| z_a - s * u;
    let room = if back {
        z_a - zone.min.y
    } else {
        zone.max.y - z_a
    };
    let up = DVec2::new(0.0, s);
    let cx = zone.center().x;
    let width = zone.size().x;
    let (n, rise) = risers(y1 - y0);
    let mut flights = Vec::new();
    let mut landings = Vec::new();
    let mut arrivals = Vec::new();
    match kind {
        StairKind::Straight | StairKind::Twin => {
            let n1 = n / 2;
            let n2 = n - n1;
            let (l1, l2) = (Flight::run(n1), Flight::run(n2));
            let depth = l2 + LANDING + l1;
            if depth + FOOT > room {
                return None;
            }
            let xs: Vec<(f64, f64)> = if kind == StairKind::Straight {
                if width < GRAND_WIDTH + 4.0 {
                    return None;
                }
                vec![(cx, GRAND_WIDTH)]
            } else {
                if width < 2.0 * SIDE_WIDTH + 8.0 {
                    return None;
                }
                let off = SIDE_WIDTH * 0.5 + 0.5;
                vec![
                    (zone.min.x + off, SIDE_WIDTH),
                    (zone.max.x - off, SIDE_WIDTH),
                ]
            };
            for (x, w) in xs {
                flights.push(straight(x, y0, z(depth), up, w, n1, rise));
                let ym = y0 + n1 as f64 * rise;
                flights.push(straight(x, ym, z(l2), up, w, n2, rise));
                let (a, b) = (z(l2), z(l2 + LANDING));
                landings.push(Landing {
                    rect: Rect::from_xz(x - w * 0.5, a.min(b), x + w * 0.5, a.max(b)),
                    y: ym,
                });
                arrivals.push((x - w * 0.5, x + w * 0.5));
            }
        }
        StairKind::Imperial => {
            // Two side flights arrive; a shorter central flight climbs away
            // from the gallery to the landing, so its foot stands clear of
            // the gallery's edge.
            let ns = (n + 10) / 2;
            if ns + 4 > n {
                return None;
            }
            let nc = n - ns;
            let (ls, lc) = (Flight::run(ns), Flight::run(nc));
            let total = GRAND_WIDTH + 2.0 * (SIDE_WIDTH + 0.5);
            if ls + LANDING + FOOT > room || total + 4.0 > width {
                return None;
            }
            let ym = y0 + nc as f64 * rise;
            flights.push(straight(cx, y0, z(ls - lc), -up, GRAND_WIDTH, nc, rise));
            let off = GRAND_WIDTH * 0.5 + 0.5 + SIDE_WIDTH * 0.5;
            for x in [cx - off, cx + off] {
                flights.push(straight(x, ym, z(ls), up, SIDE_WIDTH, ns, rise));
                arrivals.push((x - SIDE_WIDTH * 0.5, x + SIDE_WIDTH * 0.5));
            }
            let (a, b) = (z(ls), z(ls + LANDING));
            landings.push(Landing {
                rect: Rect::from_xz(cx - total * 0.5, a.min(b), cx + total * 0.5, a.max(b)),
                y: ym,
            });
        }
        StairKind::DogLeg => {
            // A return stair: a short flight away from the gallery, a
            // landing, a long one back up beside it.
            let ns = (n + 10) / 2;
            if ns + 4 > n {
                return None;
            }
            let nc = n - ns;
            let (ls, lc) = (Flight::run(ns), Flight::run(nc));
            let lane = 4.0;
            let total = 2.0 * lane + 1.0;
            if ls + LANDING + FOOT > room || total + 4.0 > width {
                return None;
            }
            let ym = y0 + nc as f64 * rise;
            let (xa, xb) = (cx - (lane + 1.0) * 0.5, cx + (lane + 1.0) * 0.5);
            flights.push(straight(xa, y0, z(ls - lc), -up, lane, nc, rise));
            flights.push(straight(xb, ym, z(ls), up, lane, ns, rise));
            arrivals.push((xb - lane * 0.5, xb + lane * 0.5));
            let (a, b) = (z(ls), z(ls + LANDING));
            landings.push(Landing {
                rect: Rect::from_xz(cx - total * 0.5, a.min(b), cx + total * 0.5, a.max(b)),
                y: ym,
            });
        }
        StairKind::Horseshoe => {
            // Two arcs sweep round from the front to meet at a landing on the
            // gallery's edge.
            let r_out = (width * 0.5 - 1.0).min(9.0);
            let r_in = r_out - SIDE_WIDTH;
            if r_in < 4.0 {
                return None;
            }
            let rm = (r_in + r_out) * 0.5;
            let sweep = n as f64 * TREAD / rm;
            let gap = 22f64.to_radians();
            let lead = sweep - (FRAC_PI_2 - gap);
            if !(0.0..=FRAC_PI_2 * 0.95).contains(&lead) {
                return None;
            }
            let c = DVec2::new(cx, z(r_out));
            // The arcs' foot: as far from the gallery as their lowest step.
            let reach = r_out + r_out * lead.sin();
            if reach + FOOT > room {
                return None;
            }
            // The landing's angle: towards the gallery.
            let to = if back { FRAC_PI_2 } else { -FRAC_PI_2 };
            for side in [1.0, -1.0] {
                // side 1: the arc on +x; it turns towards `to` from below.
                let end = to - side * s * gap;
                let start = if side > 0.0 { -s * lead } else { PI + s * lead };
                let sweep = signed_turn(start, end);
                flights.push(Flight::Arc {
                    centre: c,
                    r_in,
                    r_out,
                    a0: start,
                    sweep,
                    y0,
                    risers: n,
                    rise,
                });
            }
            let half = r_in * gap.sin();
            let (a, b) = (z(0.0), z(r_out - r_in * gap.cos()));
            landings.push(Landing {
                rect: Rect::from_xz(cx - half, a.min(b), cx + half, a.max(b)),
                y: y1,
            });
            // Between each arc's top edge (a radius) and that landing a
            // wedge is left: strips of landing fill it, each reaching down
            // to where the radius crosses its inner side (their corners lie
            // over the corner of the top tread or two).
            let wide = r_out * gap.sin();
            let strips = 4;
            for side in [1.0, -1.0] {
                for i in 0..strips {
                    let x0 = half + (wide - half) * i as f64 / strips as f64;
                    let x1 = half + (wide - half) * (i + 1) as f64 / strips as f64;
                    let (a, b) = (z(0.0), z(r_out - x0 / gap.tan()));
                    let (p, q) = (cx + side * x0, cx + side * x1);
                    landings.push(Landing {
                        rect: Rect::from_xz(p.min(q), a.min(b), p.max(q), a.max(b)),
                        y: y1,
                    });
                }
            }
            arrivals.push((cx - half, cx + half));
        }
        _ => return None,
    }
    let stair = Stair {
        kind,
        host,
        flights,
        landings,
    };
    let inside = stair
        .flights
        .iter()
        .all(|f| zone.contains_rect(&f.footprint(), 1e-6));
    inside.then_some(Grand { stair, arrivals })
}

/// The short way round from angle `a` to `b`, signed.
fn signed_turn(a: f64, b: f64) -> f64 {
    let tau = std::f64::consts::TAU;
    let d = (b - a).rem_euclid(tau);
    if d > PI {
        d - tau
    } else {
        d
    }
}

/// Lane width of a stairwell's flights, and the gap between lanes (m).
const LANE: f64 = 3.5;
const LANE_GAP: f64 = 1.0;
/// Width of a dog-leg stair (two lanes and margins), of its lift bank (m).
pub(crate) const DOG_LEG_WIDTH: f64 = 2.0 * LANE + LANE_GAP + 0.5;
pub(crate) const LIFT_BANK: f64 = 7.5;
/// A lift shaft's side (m).
const SHAFT: f64 = 3.5;
/// A spiral stair hall's side (m).
pub(crate) const SPIRAL_SIDE: f64 = 12.0;

/// Risers of each half flight of a dog-leg for storeys `ys`.
fn dog_leg_risers(ys: &[f64]) -> u32 {
    let most = ys.windows(2).map(|w| w[1] - w[0]).fold(0.0, f64::max);
    risers(most * 0.5).0
}

/// How deep a dog-leg stair hall must be for storeys `ys`.
pub(crate) fn dog_leg_depth(ys: &[f64]) -> f64 {
    (2.0 * 2.5 + Flight::run(dog_leg_risers(ys)) + 0.5).ceil()
}

/// A stair hall at `rect` rising through the floors `ys` (the first is
/// the ground, 0) up to `top`: two lanes of a dog-leg on the left, and a
/// bank of two lifts on the right if `lift` (the rect must be
/// [`DOG_LEG_WIDTH`] wide, plus [`LIFT_BANK`] with lifts). People come in
/// along the front edge (z = rect.min.y) at every floor.
pub(crate) fn dog_leg_hall(
    b: &mut Builder,
    rect: Rect,
    ys: &[f64],
    top: f64,
    lift: Option<LiftKind>,
) -> usize {
    let hall = b.space(SpaceKind::StairHall, rect, 0.0, top);
    let x0 = rect.min.x;
    let z0 = rect.min.y;
    let z1 = rect.max.y;
    let n = dog_leg_risers(ys);
    let run = Flight::run(n);
    let start = z1 - 2.5 - run;
    let lane1 = x0 + 0.25 + LANE * 0.5;
    let lane2 = lane1 + LANE + LANE_GAP;
    let stair_x1 = x0 + DOG_LEG_WIDTH;
    let mut flights = Vec::new();
    let mut landings = Vec::new();
    for w in ys.windows(2) {
        let (ya, yb) = (w[0], w[1]);
        let rise = (yb - ya) / (2 * n) as f64;
        let ym = ya + n as f64 * rise;
        flights.push(straight(lane1, ya, start, DVec2::Y, LANE, n, rise));
        flights.push(straight(lane2, ym, z1 - 2.5, -DVec2::Y, LANE, n, rise));
        landings.push(Landing {
            rect: Rect::from_xz(x0, z1 - 2.5, stair_x1, z1),
            y: ym,
        });
    }
    for &y in &ys[1..] {
        landings.push(Landing {
            rect: Rect::from_xz(x0, z0, stair_x1, start),
            y,
        });
        if lift.is_some() {
            landings.push(Landing {
                rect: Rect::from_xz(stair_x1, z0, rect.max.x, z1 - SHAFT),
                y,
            });
        }
    }
    b.stairs.push(Stair {
        kind: StairKind::DogLeg,
        host: hall,
        flights,
        landings,
    });
    if let Some(kind) = lift {
        let mid = (stair_x1 + rect.max.x) * 0.5;
        for x in [mid - SHAFT, mid] {
            let shaft = b.nested(
                hall,
                SpaceKind::Shaft,
                Rect::from_xz(x, z1 - SHAFT, x + SHAFT, z1),
                0.0,
                top,
            );
            for &y in ys {
                b.inner_door(
                    shaft,
                    hall,
                    y,
                    DVec2::new(x + SHAFT * 0.5, z1 - SHAFT),
                    true,
                    2.0,
                );
            }
            b.lifts.push(Lift {
                kind,
                shaft,
                stops: ys.to_vec(),
            });
        }
    }
    hall
}

/// A square stair hall at `rect` ([`SPIRAL_SIDE`] a side) with a spiral
/// stair round an open eye and a cage lift in the eye. The landing at each
/// floor is the front-left quarter; people come in on the front edge left
/// of the centre (returns where along x the door must be).
pub(crate) fn spiral_hall(
    b: &mut Builder,
    rect: Rect,
    ys: &[f64],
    top: f64,
    lift: Option<LiftKind>,
) -> (usize, f64) {
    let hall = b.space(SpaceKind::StairHall, rect, 0.0, top);
    let c = rect.center();
    let r_out = rect.size().min_element() * 0.5 - 0.5;
    let r_in = 2.0;
    let eye = 1.4;
    let most = ys.windows(2).map(|w| w[1] - w[0]).fold(0.0, f64::max);
    let n = risers(most).0;
    let mut flights = Vec::new();
    let mut landings = Vec::new();
    for w in ys.windows(2) {
        flights.push(Flight::Arc {
            centre: c,
            r_in,
            r_out,
            a0: -FRAC_PI_2,
            sweep: 1.5 * PI,
            y0: w[0],
            risers: n,
            rise: (w[1] - w[0]) / n as f64,
        });
    }
    for &y in &ys[1..] {
        landings.push(Landing {
            rect: Rect::from_xz(rect.min.x, rect.min.y, c.x, c.y - eye),
            y,
        });
        landings.push(Landing {
            rect: Rect::from_xz(rect.min.x, c.y - eye, c.x - eye, c.y),
            y,
        });
    }
    b.stairs.push(Stair {
        kind: StairKind::Spiral,
        host: hall,
        flights,
        landings,
    });
    if let Some(kind) = lift {
        let shaft = b.nested(
            hall,
            SpaceKind::Shaft,
            Rect::from_xz(c.x - eye, c.y - eye, c.x + eye, c.y + eye),
            0.0,
            top,
        );
        for &y in ys {
            b.inner_door(
                shaft,
                hall,
                y,
                DVec2::new(c.x - eye * 0.5, c.y - eye),
                true,
                1.4,
            );
        }
        b.lifts.push(Lift {
            kind,
            shaft,
            stops: ys.to_vec(),
        });
    }
    (hall, (rect.min.x + c.x) * 0.5)
}
