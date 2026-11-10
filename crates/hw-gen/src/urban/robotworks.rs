//! The mad scientist's robot works, as plain data: where its parts stand
//! on its yard, in the landmark's own frame (x right, z the way it faces,
//! y up from the plinth's top).
//!
//! A streamlined assembly hall stands on the yard's left. Robo-sentries
//! come out of its door on pallets, one station at a time down an assembly
//! line under a crane runway: a crane fits the dish, welding arms seam the
//! body, a second crane hangs the gun, a test arch wakes the robot, and it
//! steps off down a ramp and marches into the dispatch shed. In front of the line a dock holds a
//! walking tank half built on its cradle, and the gate the finished ones
//! march out of faces the street. The client draws and animates it all
//! (`hw-client` `worksview`); here are only the numbers both sides share,
//! and the solid parts.

use super::footprint::Footprint;
use glam::DVec2;

/// The yard's radius, biggest first: the layout below fits inside the
/// smallest.
pub const WORKS_PLAZAS: [f64; 3] = [25.0, 23.5, 22.0];

/// The assembly hall: the middle of its plan, half its width (x), half the
/// length of its straight sides (z; the ends are half round), the top of
/// its walls and of its barrel roof.
pub const HALL_AT: DVec2 = DVec2::new(-14.0, 0.0);
pub const HALL_HW: f64 = 5.5;
pub const HALL_HL: f64 = 6.5;
pub const HALL_WALL: f64 = 6.2;
pub const HALL_TOP: f64 = 9.6;
/// The door the line comes out of, in the hall's right wall: half its
/// width, and its height.
pub const DOOR_HW: f64 = 1.9;
pub const DOOR_H: f64 = 3.6;

/// The line: it runs along +x at this z, from the hall's wall to its end
/// drum.
pub const LINE_Z: f64 = 0.0;
pub const LINE_FROM: f64 = HALL_AT.x + HALL_HW;
pub const LINE_TO: f64 = 14.6;
/// The belt's top, and half its width.
pub const BELT_TOP: f64 = 0.85;
pub const BELT_HW: f64 = 1.05;
/// Where a pallet stops: inside the hall's door, at each station, and at
/// the end, where the robot steps off.
pub const STOPS: [f64; 7] = [-11.0, -7.0, -3.0, 1.0, 5.0, 9.0, 13.0];
/// Which stop does what.
pub const STOP_DISH: usize = 2;
pub const STOP_WELD: usize = 3;
pub const STOP_GUN: usize = 4;
pub const STOP_TEST: usize = 5;
pub const STOP_END: usize = 6;
/// The ramp the finished robot steps down, off the line's back (-z) side
/// at the last stop: where it starts and ends (z), and its half width.
pub const RAMP_Z: (f64, f64) = (-BELT_HW - 0.15, -3.3);
pub const RAMP_HW: f64 = 0.8;
/// The dispatch shed the robots march into: its middle, half its width
/// (x) and depth (z), and its height; its door faces the line (+z).
pub const SHED_AT: DVec2 = DVec2::new(9.0, -13.0);
pub const SHED_HALF: (f64, f64) = (3.0, 3.0);
pub const SHED_H: f64 = 4.6;

/// The crane runway over the line: its two rails' z, their top, and where
/// its columns stand along x.
pub const RUNWAY_Z: f64 = 4.0;
pub const RUNWAY_Y: f64 = 7.4;
pub const RUNWAY_COLUMNS: [f64; 4] = [-8.0, -0.5, 7.0, 14.5];
/// Each crane's parts rack: across the line from its stop (z), and a step
/// along it (x) from the stop.
pub const RACK_Z: f64 = 2.75;
pub const RACK_STEP: f64 = 1.6;
/// The welding arms: their pedestals' distance from the line.
pub const WELDER_Z: f64 = 2.1;
/// The test arch: half its span and its height.
pub const ARCH_HW: f64 = 1.9;
pub const ARCH_H: f64 = 3.9;

/// The dock: the middle of the walking tank on its cradle, the cradle's
/// top (where the hull's pivot sits), and its frame's legs: half spans in
/// x and z, and the frame's top.
pub const DOCK_AT: DVec2 = DVec2::new(0.0, 11.6);
pub const DOCK_CRADLE: f64 = 3.6;
pub const DOCK_HX: f64 = 7.0;
pub const DOCK_HZ: f64 = 5.6;
pub const DOCK_TOP: f64 = 15.0;
/// The work platforms along the hull's sides: their x (each side), half
/// their length (z) and their deck's top.
pub const DOCK_DECK_X: f64 = 6.0;
pub const DOCK_DECK_HL: f64 = 4.6;
pub const DOCK_DECK_Y: f64 = 4.5;
/// The hull's half sizes (from its model) and the turret's radius and top
/// above the hull's pivot.
pub const HULL_HALF: (f64, f64) = (5.1, 5.2);
pub const HULL_UP: f64 = 5.3;
pub const TURRET_R: f64 = 2.4;
pub const TURRET_TOP: f64 = 8.3;

/// The foreman's booth: a glass cab on legs on the yard's right, looking
/// over the dock and the line: its middle, the cab's radius, its floor and
/// its roof.
pub const BOOTH_AT: DVec2 = DVec2::new(13.0, 10.0);
pub const BOOTH_R: f64 = 2.3;
pub const BOOTH_FLOOR: f64 = 4.2;
pub const BOOTH_ROOF: f64 = 7.0;

/// The gate: its two pylons' x and z, and their height.
pub const GATE_X: f64 = 6.0;
pub const GATE_Z: f64 = 19.6;
pub const GATE_H: f64 = 8.5;
/// The sign over the hall: its mast's foot (on the roof's ridge) and the
/// robot head's middle.
pub const SIGN_UP: f64 = 17.5;

/// The works' solid parts, in its own frame (see the module docs).
pub fn footprints() -> Vec<Footprint> {
    let bx = |x: f64, z: f64, hx: f64, hz: f64, y0: f64, y1: f64| Footprint::Box {
        at: DVec2::new(x, z),
        dir: DVec2::Y,
        half: DVec2::new(hx, hz),
        y0,
        y1,
    };
    let post = |x: f64, z: f64, r: f64, y0: f64, y1: f64| Footprint::Disc {
        at: DVec2::new(x, z),
        r,
        y0,
        y1,
    };
    let mut v = Vec::new();
    // The hall: its middle and its two round ends.
    v.push(bx(HALL_AT.x, HALL_AT.y, HALL_HW, HALL_HL, 0.0, HALL_TOP));
    for s in [-1.0, 1.0] {
        v.push(post(
            HALL_AT.x,
            HALL_AT.y + s * HALL_HL,
            HALL_HW,
            0.0,
            HALL_WALL,
        ));
    }
    // The line, the ramp off it, and the dispatch shed.
    let (a, b) = (LINE_FROM, LINE_TO);
    v.push(bx(
        (a + b) * 0.5,
        LINE_Z,
        (b - a) * 0.5,
        BELT_HW,
        0.0,
        BELT_TOP,
    ));
    v.push(bx(
        STOPS[STOP_END],
        (RAMP_Z.0 + RAMP_Z.1) * 0.5,
        RAMP_HW,
        (RAMP_Z.0 - RAMP_Z.1) * 0.5,
        0.0,
        0.35,
    ));
    v.push(bx(
        SHED_AT.x,
        SHED_AT.y,
        SHED_HALF.0,
        SHED_HALF.1,
        0.0,
        SHED_H,
    ));
    // The runway: its columns and its two rails.
    for x in RUNWAY_COLUMNS {
        for s in [-1.0, 1.0] {
            v.push(post(x, s * RUNWAY_Z, 0.32, 0.0, RUNWAY_Y));
        }
    }
    let (x0, x1) = (RUNWAY_COLUMNS[0], RUNWAY_COLUMNS[3]);
    for s in [-1.0, 1.0] {
        v.push(bx(
            (x0 + x1) * 0.5,
            s * RUNWAY_Z,
            (x1 - x0) * 0.5 + 0.3,
            0.3,
            RUNWAY_Y - 0.7,
            RUNWAY_Y,
        ));
    }
    // The parts racks.
    for stop in [STOP_DISH, STOP_GUN] {
        v.push(bx(STOPS[stop] + RACK_STEP, RACK_Z, 0.9, 0.55, 0.0, 1.5));
    }
    // The welding arms' pedestals.
    for s in [-1.0, 1.0] {
        v.push(post(STOPS[STOP_WELD], s * WELDER_Z, 0.45, 0.0, 1.4));
    }
    // The test arch's legs.
    for s in [-1.0, 1.0] {
        v.push(post(STOPS[STOP_TEST], s * ARCH_HW, 0.22, 0.0, ARCH_H));
    }
    // The dock: its legs, the cradle and the half-built tank on it.
    let d = DOCK_AT;
    for sx in [-1.0, 1.0] {
        for sz in [-1.0, 1.0] {
            v.push(post(
                d.x + sx * DOCK_HX,
                d.y + sz * DOCK_HZ,
                0.45,
                0.0,
                DOCK_TOP,
            ));
        }
    }
    v.push(bx(d.x, d.y, 3.2, 3.2, 0.0, DOCK_CRADLE - 0.1));
    for s in [-1.0, 1.0] {
        let x = d.x + s * DOCK_DECK_X;
        v.push(bx(
            x,
            d.y,
            0.6,
            DOCK_DECK_HL,
            DOCK_DECK_Y - 0.25,
            DOCK_DECK_Y,
        ));
        for z in [-1.0, 0.0, 1.0] {
            v.push(post(
                x,
                d.y + z * (DOCK_DECK_HL - 0.3),
                0.12,
                0.0,
                DOCK_DECK_Y,
            ));
        }
    }
    v.push(bx(
        d.x,
        d.y,
        HULL_HALF.0 * 0.92,
        HULL_HALF.1 * 0.92,
        DOCK_CRADLE - 0.1,
        DOCK_CRADLE + HULL_UP,
    ));
    v.push(post(
        d.x,
        d.y,
        TURRET_R,
        DOCK_CRADLE + HULL_UP,
        DOCK_CRADLE + TURRET_TOP,
    ));
    // The booth: its legs and its cab.
    for k in 0..3 {
        let a = std::f64::consts::TAU * (k as f64 + 0.5) / 3.0;
        let p = BOOTH_AT + hw_core::det::dir2(a) * (BOOTH_R * 0.7);
        v.push(post(p.x, p.y, 0.2, 0.0, BOOTH_FLOOR));
    }
    v.push(post(
        BOOTH_AT.x,
        BOOTH_AT.y,
        BOOTH_R,
        BOOTH_FLOOR,
        BOOTH_ROOF + 0.8,
    ));
    // The gate's pylons.
    for s in [-1.0, 1.0] {
        v.push(post(s * GATE_X, GATE_Z, 0.7, 0.0, GATE_H));
    }
    v
}

/// The farthest any solid part reaches from the works' middle.
pub fn reach() -> f64 {
    footprints()
        .iter()
        .map(|f| match *f {
            Footprint::Disc { at, r, .. } => at.length() + r,
            Footprint::Box { at, dir, half, .. } => {
                let x = DVec2::new(dir.y, -dir.x);
                [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)]
                    .iter()
                    .map(|(a, b)| (at + x * half.x * a + dir * half.y * b).length())
                    .fold(0.0, f64::max)
            }
        })
        .fold(0.0, f64::max)
}

// The layout's parts keep clear of one another (checked when compiling).
// The line comes out of the hall's wall and ends short of its end drum.
const _: () = assert!(STOPS[0] < LINE_FROM && STOPS[1] > LINE_FROM);
const _: () = assert!(STOPS[STOP_END] + 1.0 < LINE_TO);
// The shed stands clear of the runway's back columns.
const _: () = assert!(SHED_AT.y + SHED_HALF.1 < -RUNWAY_Z - 2.0);
// The dock keeps clear of the runway and the gate; the tank walks out
// between the pylons; the hall and the booth stand clear of the dock.
const _: () = assert!(DOCK_AT.y - DOCK_HZ - 0.45 > RUNWAY_Z + 1.3);
const _: () = assert!(DOCK_AT.y - HULL_HALF.1 > RUNWAY_Z + 0.3);
const _: () = assert!(DOCK_AT.y + DOCK_HZ + 0.45 < GATE_Z - 0.7);
const _: () = assert!(GATE_X - 0.7 > HULL_HALF.0 + DOCK_AT.x.abs());
const _: () = assert!(HALL_AT.x + HALL_HW < DOCK_AT.x - DOCK_HX - 1.0);
const _: () = assert!(BOOTH_AT.x - BOOTH_R > DOCK_AT.x + DOCK_HX + 0.8);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn everything_fits_on_the_smallest_yard() {
        let r = WORKS_PLAZAS[WORKS_PLAZAS.len() - 1];
        assert!(reach() < r - 0.4, "reach {}", reach());
        // The hall's far corners and round ends too.
        for (x, z) in [
            (HALL_AT.x - HALL_HW, HALL_HL),
            (HALL_AT.x, HALL_HL + HALL_HW),
        ] {
            assert!(DVec2::new(x, z).length() < r - 0.4);
        }
    }
}
