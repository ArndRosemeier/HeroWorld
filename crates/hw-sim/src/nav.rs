//! Villains find their way round walls: A* on a one-metre grid over the
//! solid city near them, then a straightened path. The server may use only
//! Engine's headless physics, so this works on [`Obstacles`] directly; it
//! is small because villains only ever walk a street corner's worth.

use crate::collision::Obstacles;
use glam::{DVec2, DVec3};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Grid spacing (m).
const CELL: f64 = 1.0;
/// The grid reaches this far past the start and the goal (m).
const MARGIN: f64 = 14.0;
/// At most this many cells across (paths longer than this are not looked for).
const MAX_SPAN: i32 = 120;
/// Highest step up or down between neighbouring cells (m).
const MAX_STEP: f64 = 0.6;

/// Can a person stand at `p` (x, z) on ground about `near_y` high? Returns
/// the height of the floor there.
fn standable(solids: &Obstacles, p: DVec2, near_y: f64) -> Option<f64> {
    let y = solids.floor_under(p, near_y + MAX_STEP);
    if (y - near_y).abs() > 3.0 {
        return None;
    }
    let free = solids.passable_at(DVec3::new(p.x, y + 0.5, p.y))
        && solids.passable_at(DVec3::new(p.x, y + 1.5, p.y));
    free.then_some(y)
}

/// Is the straight walk from `a` to `b` (feet) free? Checks every half metre
/// at knee and head height. Doors count as open: people open a closed one
/// as they come to it (see [`crate::doors`]).
pub fn walkable_line(solids: &Obstacles, a: DVec3, b: DVec3) -> bool {
    let d = b - a;
    let n = (DVec2::new(d.x, d.z).length() / 0.5).ceil().max(1.0) as usize;
    (1..=n).all(|i| {
        let p = a + d * (i as f64 / n as f64);
        solids.passable_at(p + DVec3::Y * 0.5) && solids.passable_at(p + DVec3::Y * 1.5)
    })
}

/// A walk from `from` to `to` (feet positions) round walls: the corners to
/// head for, ending at `to`. None if there is no way within reach.
pub fn find_path(solids: &Obstacles, from: DVec3, to: DVec3) -> Option<Vec<DVec3>> {
    if walkable_line(solids, from, to) {
        return Some(vec![to]);
    }
    let lo = DVec2::new(from.x.min(to.x), from.z.min(to.z)) - DVec2::splat(MARGIN);
    let hi = DVec2::new(from.x.max(to.x), from.z.max(to.z)) + DVec2::splat(MARGIN);
    let w = (((hi.x - lo.x) / CELL).ceil() as i32).min(MAX_SPAN);
    let h = (((hi.y - lo.y) / CELL).ceil() as i32).min(MAX_SPAN);
    let cell_of = |p: DVec3| -> Option<(i32, i32)> {
        let c = ((p.x - lo.x) / CELL, (p.z - lo.y) / CELL);
        let (x, z) = (c.0.floor() as i32, c.1.floor() as i32);
        (x >= 0 && z >= 0 && x < w && z < h).then_some((x, z))
    };
    let centre = |x: i32, z: i32| lo + DVec2::new((x as f64 + 0.5) * CELL, (z as f64 + 0.5) * CELL);
    let (sx, sz) = cell_of(from)?;
    let (gx, gz) = cell_of(to)?;
    let n = (w * h) as usize;
    let idx = |x: i32, z: i32| (z * w + x) as usize;
    // Height of each cell's floor once known (NaN: not yet; -inf: blocked).
    let mut floor = vec![f64::NAN; n];
    let mut cost = vec![f64::INFINITY; n];
    let mut came = vec![usize::MAX; n];
    let mut open = BinaryHeap::new();
    floor[idx(sx, sz)] = from.y;
    cost[idx(sx, sz)] = 0.0;
    let goal = centre(gx, gz);
    let heur = |x: i32, z: i32| centre(x, z).distance(goal);
    // Scores in millimetres so the heap orders integers (ties by cell index:
    // the same result on every machine).
    let key = |f: f64| (f * 1000.0) as u64;
    open.push(Reverse((key(heur(sx, sz)), idx(sx, sz))));
    let mut found = false;
    let mut expanded = 0;
    while let Some(Reverse((_, i))) = open.pop() {
        let (x, z) = ((i as i32) % w, (i as i32) / w);
        if (x, z) == (gx, gz) {
            found = true;
            break;
        }
        expanded += 1;
        if expanded > 6000 {
            break;
        }
        for (dx, dz) in [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ] {
            let (nx, nz) = (x + dx, z + dz);
            if nx < 0 || nz < 0 || nx >= w || nz >= h {
                continue;
            }
            let j = idx(nx, nz);
            if floor[j].is_nan() {
                floor[j] = standable(solids, centre(nx, nz), floor[i]).unwrap_or(f64::NEG_INFINITY);
            }
            if floor[j] == f64::NEG_INFINITY || (floor[j] - floor[i]).abs() > MAX_STEP {
                continue;
            }
            // No cutting corners past a wall.
            if dx != 0 && dz != 0 {
                let a = idx(x + dx, z);
                let b = idx(x, z + dz);
                for k in [a, b] {
                    if floor[k].is_nan() {
                        let (kx, kz) = ((k as i32) % w, (k as i32) / w);
                        floor[k] = standable(solids, centre(kx, kz), floor[i])
                            .unwrap_or(f64::NEG_INFINITY);
                    }
                }
                if floor[a] == f64::NEG_INFINITY || floor[b] == f64::NEG_INFINITY {
                    continue;
                }
            }
            let step = if dx != 0 && dz != 0 { 1.414 } else { 1.0 } * CELL;
            let c = cost[i] + step;
            if c < cost[j] {
                cost[j] = c;
                came[j] = i;
                open.push(Reverse((key(c + heur(nx, nz)), j)));
            }
        }
    }
    if !found {
        return None;
    }
    let mut cells = vec![idx(gx, gz)];
    while let Some(&c) = cells.last() {
        let p = came[c];
        if p == usize::MAX {
            break;
        }
        cells.push(p);
    }
    cells.reverse();
    let points: Vec<DVec3> = cells
        .iter()
        .map(|&c| {
            let (x, z) = ((c as i32) % w, (c as i32) / w);
            let p = centre(x, z);
            DVec3::new(p.x, floor[c], p.y)
        })
        .collect();
    // Straighten: from each corner, go to the farthest point in plain view.
    let mut out = Vec::new();
    let mut at = from;
    let mut k = 0;
    while k < points.len() {
        let mut far = k;
        for j in (k..points.len()).rev() {
            if walkable_line(solids, at, points[j]) {
                far = j;
                break;
            }
        }
        out.push(points[far]);
        at = points[far];
        k = far + 1;
    }
    if let Some(last) = out.last_mut() {
        if walkable_line(solids, *last, to) {
            *last = to;
        } else {
            out.push(to);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::Solid;

    /// A box from `a` to `b` (x, z), 3 m high.
    fn wall(a: (f64, f64), b: (f64, f64)) -> Solid {
        Solid {
            origin: DVec2::ZERO,
            along: DVec2::X,
            inward: DVec2::Y,
            min: DVec2::new(a.0, a.1),
            max: DVec2::new(b.0, b.1),
            y0: -1.0,
            y1: 3.0,
        }
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn walks_round_a_wall() {
        let mut solids = Obstacles::flat(0.0);
        // A wall 12 m long across the way, 3 m high.
        solids.add(wall((4.0, -6.0), (5.0, 6.0)));
        let from = DVec3::new(0.0, 0.0, 0.0);
        let to = DVec3::new(10.0, 0.0, 0.0);
        assert!(!walkable_line(&solids, from, to));
        let path = find_path(&solids, from, to).expect("a way round");
        assert_eq!(*path.last().unwrap(), to);
        let mut at = from;
        for p in &path {
            assert!(walkable_line(&solids, at, *p), "{at} to {p}");
            at = *p;
        }
        assert!(path.len() >= 2 && path.len() <= 4, "{path:?}");
        // Repeatable.
        assert_eq!(find_path(&solids, from, to), Some(path));
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn a_walled_in_goal_has_no_path() {
        let mut solids = Obstacles::flat(0.0);
        for (a, b) in [
            ((8.0, -3.0), (9.0, 3.0)),
            ((11.0, -3.0), (12.0, 3.0)),
            ((8.0, -3.0), (12.0, -2.0)),
            ((8.0, 2.0), (12.0, 3.0)),
        ] {
            solids.add(wall(a, b));
        }
        assert_eq!(
            find_path(&solids, DVec3::ZERO, DVec3::new(10.0, 0.0, 0.0)),
            None
        );
    }
}
