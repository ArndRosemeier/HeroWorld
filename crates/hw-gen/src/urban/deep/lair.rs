//! The Doctor's lab under the dystopia quarter: a newspaper kiosk on a back
//! street that is really a stair, a vestibule and a long corridor, the great
//! lab hall with Tesla coils, bubbling vats and a ray cannon, a reactor, a
//! row of cells and a control room. An escape stair drops to the Deeps.

use super::{
    datum, overlaps, room, run, tile_of, tube::spot_free, DeepEntrance, DeepLamp, DeepSpot,
    EntranceKind, Fixture, FixtureKind, Lair, Plan, SpotKind, LAIR_DEPTH,
};
use crate::urban::spiral::Spiral;
use crate::urban::underground::{Void, VoidKind, SHAFT_HALF};
use crate::urban::world::TownWorld;
use glam::{DVec2, DVec3};

/// Where the lab is, as far as the macro plan tells: the tile that plans it
/// works out the kiosk and the rooms.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LairSite {
    pub centre: DVec2,
}

/// The lab reaches this far from its site (for other layers to keep off).
pub(crate) const LAIR_REACH: f64 = 150.0;

impl LairSite {
    pub fn plan(w: &TownWorld) -> Option<LairSite> {
        let d = w.town.dystopia.as_ref()?;
        let mut r = w.seed.child("lair").rng();
        let a = r.range(0.0, std::f64::consts::TAU);
        let off = r.range(0.0, (d.radius * 0.3).min(60.0));
        Some(LairSite {
            centre: d.centre + hw_core::det::dir2(a) * off,
        })
    }
}

/// The rooms, in the lab's own frame (x away from the kiosk).
struct Layout {
    voids: Vec<Void>,
    hall: usize,
    reactor: usize,
    cells: usize,
    control: usize,
}

fn layout(o: DVec2, dir: DVec2, f: f64) -> Layout {
    let at = |x: f64, y: f64| o + dir * x + dir.perp() * y;
    let k = VoidKind::Lair;
    let mut v = vec![
        // Vestibule under the kiosk.
        room(k, at(3.0, 0.0), dir, DVec2::new(5.0, 4.0), f, 4.5),
    ];
    v.extend(run(k, at(7.0, 0.0), at(30.0, 0.0), 2.0, f, f, 4.0));
    let hall = v.len();
    v.push(room(k, at(52.0, 0.0), dir, DVec2::new(22.0, 13.0), f, 10.0));
    v.extend(run(k, at(73.0, 0.0), at(80.0, 0.0), 2.5, f, f, 5.0));
    let reactor = v.len();
    v.push(room(k, at(90.0, 0.0), dir, DVec2::new(11.0, 11.0), f, 10.0));
    v.extend(run(k, at(52.0, 12.0), at(52.0, 16.0), 1.6, f, f, 3.4));
    let cells = v.len();
    v.push(room(k, at(52.0, 19.0), dir, DVec2::new(8.0, 4.0), f, 4.5));
    v.extend(run(k, at(60.0, -12.0), at(60.0, -16.0), 1.6, f, f, 3.4));
    let control = v.len();
    v.push(room(k, at(60.0, -19.0), dir, DVec2::new(8.0, 4.5), f, 5.0));
    Layout {
        voids: v,
        hall,
        reactor,
        cells,
        control,
    }
}

pub(super) fn plan_tile(plan: &mut Plan) {
    let w = plan.w;
    let Some(site) = w.lair_site().cloned() else {
        return;
    };
    if tile_of(site.centre) != plan.key() {
        return;
    }
    let net = w.tube();
    // A kiosk spot near the site, and a way for the rooms to run.
    let mut found = None;
    'search: for ring in [0.0, 10.0, 20.0, 30.0, 40.0, 55.0, 70.0] {
        let n = if ring == 0.0 { 1 } else { 12 };
        for a in 0..n {
            let c = site.centre
                + hw_core::det::dir2(std::f64::consts::TAU * a as f64 / n as f64) * ring;
            if w.ground(c) < crate::urban::world::DECK + 1.0 || !spot_free(w, c, 2.6) {
                continue;
            }
            let f = datum(w, c) - LAIR_DEPTH;
            for d in 0..8 {
                let dir = hw_core::det::dir2(std::f64::consts::TAU * d as f64 / 8.0 + 0.2);
                let l = layout(c, dir, f);
                // The kiosk's door faces away from the lab: the stair's top
                // landing is by it, its foot opens into the vestibule.
                let spiral = Spiral::new(c, -dir, w.ground(c), f);
                let shaft = spiral.shaft(None);
                let tubes = net.stretch_voids_near(c + dir * 50.0, 120.0);
                let fits = l.voids.iter().chain(std::iter::once(&shaft)).all(|v| {
                    plan.roofed(v, 3.0)
                        && plan.clear_of_shallow(v, 2.0)
                        && tubes.iter().all(|(_, _, t)| !overlaps(v, t, 2.0))
                });
                if fits {
                    found = Some((c, dir, f, l, shaft, spiral));
                    break 'search;
                }
            }
        }
    }
    let Some((c, dir, f, l, shaft, spiral)) = found else {
        return;
    };
    let base = plan.t.voids.len();
    let at = |x: f64, y: f64| c + dir * x + dir.perp() * y;
    let p3 = |x: f64, y: f64| {
        let q = at(x, y);
        DVec3::new(q.x, f, q.y)
    };
    plan.t.voids.extend(l.voids);
    plan.t.voids.push(shaft);
    plan.t.entrances.push(DeepEntrance {
        kind: EntranceKind::Kiosk,
        name: "News kiosk".into(),
        pos: c,
        door: -dir,
        top: w.ground(c),
        bottom: f,
        spiral,
    });
    plan.t.fixtures.push(Fixture {
        kind: FixtureKind::Kiosk,
        at: DVec3::new(c.x, w.ground(c), c.y),
        dir: -dir,
        size: DVec3::new(SHAFT_HALF + 0.3, 2.9, 0.0),
        look: 0,
    });
    plan.t.lairs.push(Lair {
        name: "The Doctor's Lab".into(),
        hall: base + l.hall,
        reactor: base + l.reactor,
        cells: base + l.cells,
        control: base + l.control,
        floor: f,
    });
    let mut fix = |kind: FixtureKind, x: f64, y: f64, d: DVec2, size: DVec3, look: u32| {
        plan.t.fixtures.push(Fixture {
            kind,
            at: p3(x, y),
            dir: d,
            size,
            look,
        });
    };
    let side = dir.perp();
    // Tesla coils.
    for (k, (x, y)) in [(38.0, 9.0), (38.0, -9.0), (64.0, 9.0)]
        .into_iter()
        .enumerate()
    {
        fix(
            FixtureKind::TeslaCoil,
            x,
            y,
            dir,
            DVec3::new(0.9, 7.0, 0.0),
            k as u32,
        );
    }
    // Vats along the cells' wall, leaving the way to the cells open.
    for (k, x) in [44.0, 48.0, 56.0, 60.0].into_iter().enumerate() {
        fix(
            FixtureKind::Vat,
            x,
            10.5,
            -side,
            DVec3::new(1.1, 3.2, 0.0),
            k as u32,
        );
    }
    // The doomsday ray cannon in the middle.
    fix(
        FixtureKind::RayCannon,
        52.0,
        -2.0,
        dir,
        DVec3::new(2.8, 8.0, 0.0),
        0,
    );
    // Crates stacked in the hall's corners.
    for (k, (x, y)) in [(32.5, 11.0), (32.5, -11.0), (34.4, -11.2), (71.5, 11.0)]
        .into_iter()
        .enumerate()
    {
        fix(
            FixtureKind::Crate,
            x,
            y,
            dir,
            DVec3::new(0.7, 1.4, 0.7),
            k as u32,
        );
    }
    // The reactor.
    fix(
        FixtureKind::Reactor,
        90.0,
        0.0,
        dir,
        DVec3::new(3.2, 8.0, 0.0),
        0,
    );
    // Cells: bars across the room and between the cages.
    fix(
        FixtureKind::CellBars,
        52.0,
        18.4,
        side,
        DVec3::new(7.9, 4.4, 0.0),
        0,
    );
    for x in [49.3, 54.7] {
        fix(
            FixtureKind::CellBars,
            x,
            20.7,
            dir,
            DVec3::new(2.3, 4.4, 0.0),
            1,
        );
    }
    // The control room: consoles before a wall of screens.
    for (k, x) in [55.0, 60.0, 65.0].into_iter().enumerate() {
        fix(
            FixtureKind::Console,
            x,
            -21.4,
            side,
            DVec3::new(1.5, 1.1, 0.5),
            k as u32,
        );
    }
    fix(
        FixtureKind::ScreenWall,
        60.0,
        -23.4,
        side,
        DVec3::new(7.0, 3.6, 0.05),
        0,
    );
    // Lights: cold lab lights, coil and vat glows, the reactor's heart.
    for x in [36.0, 52.0, 68.0] {
        for y in [-7.0, 7.0] {
            let q = at(x, y);
            plan.t.lamps.push(DeepLamp {
                pos: DVec3::new(q.x, f + 8.0, q.y),
                color: [0.75, 1.0, 0.85],
                strength: 16.0,
                range: 22.0,
            });
        }
    }
    for (x, y, col, s) in [
        (38.0, 9.0, [0.6, 0.55, 1.0], 6.0),
        (38.0, -9.0, [0.6, 0.55, 1.0], 6.0),
        (64.0, 9.0, [0.6, 0.55, 1.0], 6.0),
        (52.0, 9.0, [0.4, 1.0, 0.4], 5.0),
        (90.0, 0.0, [1.0, 0.45, 0.2], 10.0),
        (52.0, 19.0, [1.0, 0.3, 0.25], 3.0),
        (60.0, -19.0, [0.4, 0.8, 1.0], 4.0),
        (3.0, 0.0, [1.0, 0.3, 0.25], 3.5),
        (18.0, 0.0, [1.0, 0.3, 0.25], 3.0),
    ] {
        let q = at(x, y);
        plan.t.lamps.push(DeepLamp {
            pos: DVec3::new(q.x, f + 3.0, q.y),
            color: col,
            strength: s,
            range: 12.0,
        });
    }
    // Guards: in the hall and by the reactor.
    for (x, y, face) in [
        (44.0, -6.0, -dir),
        (60.0, 4.0, -dir),
        (84.0, 5.5, (at(90.0, 0.0) - at(84.0, 5.5)).normalize()),
    ] {
        plan.t.spots.push(DeepSpot {
            kind: SpotKind::LairGuard,
            at: p3(x, y),
            facing: face,
        });
    }
    // The escape stair down to the Deeps, in the hall's far corner.
    plan.lift_to_deeps(at(70.0, -10.0), f);
}
