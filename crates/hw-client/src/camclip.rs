//! Keeps the hero camera out of walls. The camera sits on a boom behind the
//! hero's head; the boom is shortened so that a small box round the camera
//! (as wide as the near plane, so no wall is cut open at the screen's
//! edges) stays clear of walls, floors and roofs. Coming in is instant,
//! going back out is eased, so the view never pops through a corner.

use glam::{DVec2, DVec3, Vec3};
use hw_sim::collision::Obstacles;

/// Half the size of the box kept clear round the camera (m).
pub const CLEARANCE: f32 = 0.3;
/// Closest the boom starts looking for walls (m): nearer than this the
/// camera is at the head anyway.
const START: f32 = 0.3;
/// Step along the boom (m): walls between steps are still found.
const STEP: f32 = 0.4;
/// Shorter than this, the camera goes into the head (first person).
pub const FIRST_PERSON: f32 = 0.4;
/// How quickly the boom grows back once a wall is out of the way (1/s).
const EASE_OUT: f32 = 5.0;

/// Is `p` below the ground or floor there (with room for the near plane)?
fn under_floor(solids: &Obstacles, p: DVec3) -> bool {
    p.y < solids.ground_for(DVec2::new(p.x, p.z), p.y) + 0.2
}

/// Does a wall stand on the segment from `a` to `b`, or is `b` under the
/// floor? Parts above `open_above` (the ceiling of the floor the hero is
/// on, inside a building drawn cut open) never count.
fn blocked(solids: &Obstacles, a: Vec3, b: Vec3, open_above: Option<f64>) -> bool {
    let (mut a, mut b) = (a.as_dvec3(), b.as_dvec3());
    if let Some(y) = open_above {
        if a.y > y && b.y > y {
            return false;
        }
        // Only the part below the cut.
        let cut = |p: DVec3, q: DVec3| p + (q - p) * ((y - p.y) / (q.y - p.y));
        if a.y > y {
            a = cut(b, a);
        } else if b.y > y {
            b = cut(a, b);
            return solids.line_blocked(a, b);
        }
    }
    solids.line_blocked(a, b) || under_floor(solids, b)
}

/// How long the boom from `head` back along `-dir` may be, up to `wanted`:
/// short of the first wall that the camera box would touch. Walls are
/// found exactly along each edge of the box, so a thin inner wall is never
/// stepped over.
pub fn clear_length(
    solids: &Obstacles,
    head: Vec3,
    dir: Vec3,
    wanted: f32,
    open_above: Option<f64>,
) -> f32 {
    let dir = dir.normalize_or(Vec3::Z);
    // Two directions across the boom: the near plane's sides and its top
    // and bottom.
    let side = dir.cross(Vec3::Y).normalize_or(Vec3::X);
    let up = side.cross(dir).normalize_or(Vec3::Y);
    // The box widens with the boom: right behind the head it is narrow,
    // so a wall the hero leans on does not push the camera in.
    let probes = |t: f32| {
        let p = head - dir * t;
        let r = CLEARANCE * (t / 1.0).min(1.0);
        [p, p + side * r, p - side * r, p + up * r, p - up * r]
    };
    let mut last = probes(START);
    if last.iter().any(|&q| blocked(solids, head, q, open_above)) {
        return 0.0;
    }
    let mut t = START;
    while t < wanted {
        let next_t = (t + STEP).min(wanted);
        let next = probes(next_t);
        if (0..next.len()).any(|i| blocked(solids, last[i], next[i], open_above)) {
            return (t - 0.05).max(0.0);
        }
        last = next;
        t = next_t;
    }
    wanted
}

/// The boom's length as drawn: in at once when a wall comes between, out
/// again smoothly.
#[derive(Clone, Copy, Debug, Default)]
pub struct Boom {
    shown: Option<f32>,
}

impl Boom {
    /// Moves the drawn length toward `clear` (from [`clear_length`]) over
    /// `dt` seconds; returns the length to draw.
    pub fn follow(&mut self, clear: f32, dt: f32) -> f32 {
        let shown = match self.shown {
            Some(s) if clear > s => s + (clear - s) * (1.0 - (-dt * EASE_OUT).exp()),
            _ => clear,
        };
        self.shown = Some(shown);
        shown
    }
}

/// Where the camera goes for a boom of `length` from `head` back along
/// `-dir`: into the head when the boom is very short.
pub fn eye(head: Vec3, dir: Vec3, length: f32) -> Vec3 {
    if length < FIRST_PERSON {
        head
    } else {
        head - dir.normalize_or(Vec3::Z) * length
    }
}

/// A spot near `wanted` (feet) with the street clear for `room` metres
/// all round at body height: for a screenshot that puts the hero in a
/// fight, so the hero does not start inside a wall or face-first in one.
pub fn open_spot(solids: &Obstacles, wanted: DVec2, room: f64, near_y: f64) -> DVec2 {
    let clear = |p: DVec2| {
        let y = solids.ground_for(p, near_y + 1.0);
        // Every quarter metre out to `room` along twelve directions, so a
        // thin wall inside the circle counts too.
        let steps = (room / 0.25).ceil() as usize;
        (0..12).all(|k| {
            let a = k as f64 * std::f64::consts::TAU / 12.0;
            let way = DVec2::new(a.cos(), a.sin());
            (0..=steps).all(|n| {
                let q = p + way * (room * n as f64 / steps as f64);
                [0.5, 1.5]
                    .iter()
                    .all(|h| !solids.solid_at(DVec3::new(q.x, y + h, q.y)))
            })
        })
    };
    // Rings of candidates, nearest first.
    for ring in 0..12 {
        let r = ring as f64 * 1.5;
        let n = if ring == 0 { 1 } else { 8 * ring };
        for k in 0..n {
            let a = k as f64 * std::f64::consts::TAU / n as f64;
            let p = wanted + DVec2::new(a.cos(), a.sin()) * r;
            if clear(p) {
                return p;
            }
        }
    }
    wanted
}

#[cfg(test)]
mod tests {
    use super::*;
    use hw_sim::collision::Solid;

    /// A wall 0.3 m thick across the way, `at` metres behind the head
    /// (the camera looks along +x, so the boom goes toward -x).
    fn walled(at: f64) -> Obstacles {
        let mut s = Obstacles::flat(0.0);
        s.add(Solid {
            origin: DVec2::new(-at - 0.3, -20.0),
            along: DVec2::new(0.0, 1.0),
            inward: DVec2::new(1.0, 0.0),
            min: DVec2::new(0.0, 0.0),
            max: DVec2::new(40.0, 0.3),
            y0: 0.0,
            y1: 10.0,
        });
        s
    }

    const HEAD: Vec3 = Vec3::new(0.0, 1.6, 0.0);

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn the_boom_stops_short_of_a_wall_behind() {
        let s = walled(3.0);
        let len = clear_length(&s, HEAD, Vec3::X, 8.0, None);
        assert!(len < 3.0 && len > 2.2, "boom {len}");
        // Nothing behind: the whole way.
        let open = Obstacles::flat(0.0);
        assert_eq!(clear_length(&open, HEAD, Vec3::X, 8.0, None), 8.0);
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn a_wall_beside_the_boom_keeps_the_near_plane_out() {
        // A wall running along the boom, 0.2 m to its side from 1 m back:
        // the centre line is clear, the side of the camera box is not.
        let mut s = Obstacles::flat(0.0);
        s.add(Solid {
            origin: DVec2::new(-20.0, 0.2),
            along: DVec2::new(1.0, 0.0),
            inward: DVec2::new(0.0, 1.0),
            min: DVec2::new(0.0, 0.0),
            max: DVec2::new(19.0, 2.0),
            y0: 0.0,
            y1: 10.0,
        });
        let len = clear_length(&s, HEAD, Vec3::X, 8.0, None);
        assert!(len < 1.2, "boom {len} runs along inside the wall's skin");
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn the_ground_behind_pulls_the_camera_in() {
        // Looking up steeply: the boom goes down into the street.
        let dir = Vec3::new(0.5, 0.8, 0.0).normalize();
        let len = clear_length(&Obstacles::flat(0.0), HEAD, dir, 8.0, None);
        assert!(len < 2.0, "boom {len} goes under the street");
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn the_cut_open_floor_above_does_not_count() {
        let s = walled(1.0);
        // Looking down from above: the boom rises through the wall, which
        // counts only when it is drawn.
        let dir = Vec3::new(0.3, -0.95, 0.0).normalize();
        assert_eq!(clear_length(&s, HEAD, dir, 8.0, Some(2.0)), 8.0);
        assert!(clear_length(&s, HEAD, dir, 8.0, None) < 4.0);
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn the_boom_snaps_in_and_eases_out() {
        let mut b = Boom::default();
        assert_eq!(b.follow(6.0, 0.016), 6.0);
        assert_eq!(b.follow(1.5, 0.016), 1.5, "in at once");
        let next = b.follow(6.0, 0.016);
        assert!(next > 1.5 && next < 2.0, "out slowly: {next}");
        let mut len = next;
        for _ in 0..120 {
            len = b.follow(6.0, 0.016);
        }
        assert!(len > 5.9, "back out after two seconds: {len}");
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn a_fight_starts_in_the_open() {
        let s = walled(0.0);
        // Wanted right at the wall: moved off it.
        let p = open_spot(&s, DVec2::new(-0.1, 0.0), 1.5, 0.0);
        assert!(!s.solid_at(DVec3::new(p.x, 1.0, p.y)));
        assert!(p.x > 1.4 || p.x < -1.7, "spot {p} still touches the wall");
    }
}
