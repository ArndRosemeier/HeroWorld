//! The Comet buildings (`hw_gen::urban::comet`): offices with game-scale
//! interiors, drawn from the very parts bodies collide with.
//!
//! Every wall, floor, step, rail and column here is one solid part of
//! `hw_gen::layout::solid`: nothing is drawn that cannot be stood on or
//! stops a hero, except the lamps hanging from the ceilings.
//!
//! Built in the landmark's own frame (origin on the plinth's top in the
//! middle of the building, +z out of the entrance).

use crate::citymesh::{MeshBuilder, Surf};
use crate::interior::pieces;
use crate::skins::{Base, Skin, Tex, Wear};
use engine::prelude::*;
use glam::{DVec2, DVec3};
use hw_gen::layout::solid::{Part, Piece, RAIL, SLAB, WALL};
use hw_gen::layout::{Layout, Scheme, SpaceKind};
use hw_gen::urban::comet;
use hw_gen::urban::footprint::Footprint;
use hw_gen::urban::landmarks::Landmark;

/// The house colours of the three schemes: (cladding, band, accent).
fn livery(s: Scheme) -> (Color, Color, Color) {
    match s {
        Scheme::Atrium => (rgb(238, 230, 210), rgb(40, 132, 140), rgb(228, 176, 70)),
        Scheme::GrandHall => (rgb(226, 214, 196), rgb(150, 62, 48), rgb(214, 170, 90)),
        Scheme::SplitLevel => (rgb(214, 226, 226), rgb(46, 84, 150), rgb(236, 120, 60)),
    }
}

fn skin(b: &mut MeshBuilder, near: bool, base: Base) {
    b.surf = Surf::Plain;
    b.tex = near.then(|| Tex::from(Skin::new(base, Wear::Fresh)));
}

fn plain(b: &mut MeshBuilder, s: Surf) {
    b.surf = s;
    b.tex = None;
}

/// A part's box: (centre, the unit way of its x half, half sizes as
/// `oriented_box` takes them).
fn frame(at: DVec2, dir: DVec2, half: DVec2, y0: f64, y1: f64) -> (DVec3, DVec3, DVec3) {
    (
        DVec3::new(at.x, (y0 + y1) * 0.5, at.y),
        DVec3::new(dir.y, 0.0, -dir.x),
        DVec3::new(half.x, (y1 - y0) * 0.5, half.y),
    )
}

/// The part's box between heights `y0` and `y1`, grown by `grow` in plan.
fn put(b: &mut MeshBuilder, f: &Footprint, (y0, y1): (f64, f64), grow: f64, color: Color) {
    if let Footprint::Box { at, dir, half, .. } = *f {
        let (c, u, h) = frame(at, dir, half + DVec2::splat(grow), y0, y1);
        b.oriented_box(c, u, h, color);
    }
}

/// The floor finish of a room: (texture, tint).
fn finish(kind: SpaceKind, accent: Color) -> (Base, Color) {
    match kind {
        SpaceKind::Lobby | SpaceKind::Atrium => (Base::Terrazzo, rgb(236, 226, 204)),
        SpaceKind::Hall => (Base::Parquet, rgb(176, 124, 76)),
        SpaceKind::Service => (Base::Lino, rgb(206, 214, 204)),
        SpaceKind::StairHall | SpaceKind::Concourse => (Base::Terrazzo, rgb(214, 208, 198)),
        // Galleries, arcades, the vestibule: a quiet shade of the house accent.
        _ => (Base::Terrazzo, rgb(232, 224, 208).lerp(accent, 0.3)),
    }
}

/// The whole building into `b`.
pub fn building(b: &mut MeshBuilder, l: &Landmark, near: bool) {
    let Some(plan) = comet::layout(l) else { return };
    let parts = comet::parts(l);
    let (clad, band, accent) = livery(plan.scheme);
    let plaster = rgb(238, 232, 218);
    let ceiling = rgb(244, 242, 234);
    let stone = rgb(222, 214, 198);
    let chrome = pieces::chrome();
    let brass = pieces::brass();
    let glass = rgb(150, 190, 200);
    let top = plan.height();

    for Piece { part, shape } in &parts {
        let (y0, y1) = shape.span();
        if !near
            && !matches!(
                part,
                Part::Outer | Part::Roof | Part::Window | Part::Canopy | Part::Fin
            )
        {
            continue;
        }
        match *part {
            Part::Outer => outer(b, shape, near, (clad, band, plaster), top),
            Part::Wall => {
                skin(b, near, Base::CastStone);
                put(b, shape, (y0, y1), 0.0, plaster);
            }
            Part::Floor(kind) => {
                // The finish on top, the ceiling of the room below under it.
                let (base, tint) = finish(kind, accent);
                let split = y1 - 0.08;
                skin(b, near, base);
                put(b, shape, (split, y1), 0.0, tint);
                plain(b, Surf::Plain);
                put(b, shape, (y0, split), 0.0, ceiling);
            }
            Part::Deck => {
                let split = y1 - 0.08;
                skin(b, near, Base::Terrazzo);
                put(b, shape, (split, y1), 0.0, rgb(226, 218, 202));
                plain(b, Surf::Plain);
                put(b, shape, (y0, split), 0.0, ceiling);
            }
            Part::Roof => {
                skin(b, near, Base::CastStone);
                put(b, shape, (y0, y1), 0.0, rgb(150, 150, 146));
            }
            Part::Glass => {
                plain(b, Surf::Glass);
                put(b, shape, (y0, y1), 0.0, glass);
            }
            Part::Window => {
                if near {
                    plain(b, Surf::Glass);
                    put(b, shape, (y0, y1), 0.0, glass);
                } else {
                    plain(b, Surf::Plain);
                    put(b, shape, (y0, y1), 0.12, rgb(52, 78, 96));
                }
            }
            Part::Step => {
                // The tread, with a brass nosing strip let into its top.
                skin(b, near, Base::Terrazzo);
                put(b, shape, (y0, y1), 0.0, stone);
            }
            Part::Rail => {
                // A glass panel under a chrome handrail.
                plain(b, Surf::Glass);
                put(b, shape, (y0, y1 - 0.06), 0.0, glass);
                plain(b, Surf::Metal);
                put(b, shape, (y1 - 0.06, y1), 0.025, chrome);
            }
            Part::Column => {
                if let Footprint::Disc { at, r, .. } = *shape {
                    column(
                        b,
                        DVec3::new(at.x, y0, at.y),
                        r,
                        y1 - y0,
                        (stone, band, brass),
                    );
                }
            }
            Part::Shaft => {
                skin(b, near, Base::Enamel);
                put(b, shape, (y0, y1), 0.0, band);
            }
            Part::Canopy => {
                // A cream slab with a chrome edge and a glowing soffit.
                skin(b, near, Base::Enamel);
                put(b, shape, (y0 + 0.06, y1), 0.0, clad);
                plain(b, Surf::Metal);
                put(b, shape, (y0 + 0.14, y1 - 0.1), 0.05, chrome);
                plain(b, Surf::Glow);
                put(b, shape, (y0, y0 + 0.06), -0.5, rgb(255, 236, 190));
            }
            Part::Fin => {
                // The sign fin: the house colour, a strip of light up its
                // outer edge and a chrome cap.
                skin(b, near, Base::Enamel);
                put(b, shape, (y0, y1 - 0.25), 0.0, band);
                plain(b, Surf::Metal);
                put(b, shape, (y1 - 0.25, y1), 0.06, chrome);
                if let Footprint::Box { at, dir, half, .. } = *shape {
                    // Local z is out of the entrance: the edge is the
                    // part's far end that way.
                    let (zc, zh) = if dir.x.abs() < 0.5 {
                        (at.y, half.y)
                    } else {
                        (at.y, half.x)
                    };
                    let (xh,) = (half.x.min(half.y),);
                    plain(b, Surf::Glow);
                    b.oriented_box(
                        DVec3::new(at.x, (y0 + y1) * 0.5, zc + zh + 0.04),
                        DVec3::X,
                        DVec3::new(xh * 0.45, (y1 - y0) * 0.5 - 0.6, 0.05),
                        accent,
                    );
                }
            }
            Part::LiftDoor => {
                plain(b, Surf::Metal);
                put(b, shape, (y0, y1), 0.0, brass);
            }
        }
    }
    plain(b, Surf::Plain);
    if near {
        lamps(b, &plan, brass);
    }
    let _ = (RAIL, WALL);
}

/// An outside wall: cladding outside, plaster inside, a band of colour at
/// the foot and under the roof.
fn outer(
    b: &mut MeshBuilder,
    f: &Footprint,
    near: bool,
    (clad, band, plaster): (Color, Color, Color),
    top: f64,
) {
    let Footprint::Box {
        at,
        dir,
        half,
        y0,
        y1,
    } = *f
    else {
        return;
    };
    // The thin way through the wall, pointing out of the building.
    let x = DVec2::new(dir.y, -dir.x);
    let (through, thin_x) = if half.x < half.y {
        (x, true)
    } else {
        (dir, false)
    };
    let out = if at.dot(through) >= 0.0 {
        through
    } else {
        -through
    };
    let quarter = half.x.min(half.y) * 0.5;
    let layer = |b: &mut MeshBuilder, side: f64, lo: f64, hi: f64, color: Color| {
        if hi - lo < 0.01 {
            return;
        }
        let h = if thin_x {
            DVec2::new(quarter, half.y)
        } else {
            DVec2::new(half.x, quarter)
        };
        let (c, u, hh) = frame(at + out * (quarter * side), dir, h, lo, hi);
        b.oriented_box(c, u, hh, color);
    };
    // Outside: a dark plinth course, the cladding, a band under the roof.
    skin(b, near, Base::Enamel);
    let foot = 1.0f64.clamp(y0, y1);
    let crown = (top - 1.4).clamp(y0, y1);
    layer(b, 1.0, y0, foot, band);
    layer(b, 1.0, foot, crown, clad);
    layer(b, 1.0, crown, y1, band);
    if near {
        skin(b, near, Base::CastStone);
        layer(b, -1.0, y0, y1, plaster);
    }
}

/// A round column on a low base under a flared capital with a brass ring.
fn column(
    b: &mut MeshBuilder,
    foot: DVec3,
    r: f64,
    h: f64,
    (stone, band, brass): (Color, Color, Color),
) {
    use crate::carmesh::lathe;
    b.surf = Surf::Plain;
    b.tex = Some(Tex::from(Skin::new(Base::Terrazzo, Wear::Fresh)));
    let shaft = r * 0.82;
    let profile = [
        (r, 0.0),
        (r, 0.32),
        (shaft, 0.4),
        (shaft, h - 0.7),
        (r * 0.9, h - 0.3),
        (r, h - 0.12),
        (r, h),
    ];
    lathe(b, foot, DVec3::Y, &profile, 24, |_| {
        Some((Surf::Plain, stone))
    });
    b.tex = None;
    b.surf = Surf::Metal;
    lathe(
        b,
        foot + DVec3::Y * 0.4,
        DVec3::Y,
        &[(shaft + 0.025, 0.0), (shaft + 0.025, 0.07)],
        24,
        |_| Some((Surf::Metal, brass)),
    );
    lathe(
        b,
        foot + DVec3::Y * (h - 0.78),
        DVec3::Y,
        &[(shaft + 0.025, 0.0), (shaft + 0.025, 0.07)],
        24,
        |_| Some((Surf::Metal, brass)),
    );
    b.surf = Surf::Plain;
    let _ = band;
}

/// Where the ceiling lamps hang, in the landmark's frame: (the saucer's
/// middle, its radius, the length of its rod).
fn lamp_spots(plan: &Layout) -> Vec<(DVec3, f64, f64)> {
    let mut out = Vec::new();
    for s in plan
        .spaces
        .iter()
        .filter(|s| s.within.is_none() && s.kind != SpaceKind::Shaft)
    {
        let size = s.rect.size();
        // The ceiling: the slab of the floor above, or the roof.
        let roofed = !plan.spaces.iter().any(|t| {
            t.within.is_none() && (t.y - s.top()).abs() < 1e-3 && t.rect.overlap(&s.rect) > 1.0
        });
        let ceil = if roofed { s.top() } else { s.top() - SLAB };
        let tall = s.h > 10.0;
        let (r, drop) = if tall { (1.3, 2.2) } else { (0.75, 0.9) };
        let spacing = if tall { 9.0 } else { 8.0 };
        let (nx, nz) = (
            (size.x / spacing).round().max(1.0) as usize,
            (size.y / spacing).round().max(1.0) as usize,
        );
        for i in 0..nx {
            for j in 0..nz {
                let p = s.rect.min
                    + size * DVec2::new((i as f64 + 0.5) / nx as f64, (j as f64 + 0.5) / nz as f64);
                let q = comet::point_to_local(plan.size, p);
                out.push((DVec3::new(q.x, ceil - drop, q.y), r, drop));
            }
        }
    }
    out
}

/// Saucer lamps hanging from every ceiling, big ones in the tall rooms.
fn lamps(b: &mut MeshBuilder, plan: &Layout, brass: Color) {
    use crate::carmesh::lathe;
    let warm = rgb(255, 236, 190);
    for (hang, r, drop) in lamp_spots(plan) {
        b.surf = Surf::Metal;
        b.cylinder(hang, (0.03, 0.03), drop, 6, false, brass);
        lathe(
            b,
            hang,
            DVec3::Y,
            &[(0.05, 0.22), (r * 0.5, 0.16), (r, 0.0)],
            24,
            |_| Some((Surf::Metal, brass)),
        );
        lathe(
            b,
            hang,
            DVec3::Y,
            &[(r * 0.96, -0.01), (r * 0.6, -0.1), (0.0, -0.14)],
            24,
            |_| Some((Surf::Glow, warm)),
        );
        b.surf = Surf::Plain;
    }
}

/// The light of the ceiling lamps: on by day and night, the ones nearest
/// the eye (Engine lights a limited number of lamps at once).
pub struct CometLights {
    /// Every lamp of every building, in the town's frame.
    spots: Vec<Lamp>,
    lit: Vec<LampId>,
    at: Option<Vec3>,
}

/// How many lamps shine at once, how near the eye they must be (m), and
/// how far the eye moves before they are picked again.
const LIT: usize = 24;
const LIT_REACH: f32 = 70.0;
const LIT_STEP: f32 = 3.0;

impl CometLights {
    pub fn new(town: &hw_gen::urban::world::TownWorld) -> Self {
        let mut spots = Vec::new();
        for l in &town.landmarks {
            let Some(plan) = comet::layout(l) else {
                continue;
            };
            for (hang, r, _) in lamp_spots(&plan) {
                let p = l.to_plan(DVec2::new(hang.x, hang.z));
                let big = r > 1.0;
                spots.push(Lamp::point(
                    Vec3::new(p.x as f32, (l.base + hang.y - 0.4) as f32, p.y as f32),
                    rgb(255, 232, 196),
                    if big { 9.0 } else { 5.0 },
                    if big { 22.0 } else { 13.0 },
                ));
            }
        }
        Self {
            spots,
            lit: Vec::new(),
            at: None,
        }
    }

    pub fn update(&mut self, world: &mut World, eye: Vec3) -> EngineResult<()> {
        if self.at.is_some_and(|a| a.distance(eye) < LIT_STEP) {
            return Ok(());
        }
        self.at = Some(eye);
        let mut near: Vec<(f32, Lamp)> = self
            .spots
            .iter()
            .map(|l| (l.position.distance(eye), *l))
            .filter(|(d, _)| *d < LIT_REACH)
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.truncate(LIT);
        for (k, (_, lamp)) in near.iter().enumerate() {
            match self.lit.get(k) {
                Some(&id) => world.set_lamp(id, *lamp)?,
                None => self.lit.push(world.add_lamp(*lamp)?),
            }
        }
        for id in self.lit.drain(near.len()..) {
            world.remove_lamp(id);
        }
        Ok(())
    }
}

/// Where to stand and what to look at for a view of the building (town
/// frame, heights above the plinth): `lobby` (in at the entrance), `gallery`
/// (on the highest gallery or bridge, looking down), `stairs` (at the foot
/// of the grand stair), `hall` (in the biggest office hall), anything else
/// from across the plaza.
pub fn view(l: &Landmark, which: &str) -> Option<(DVec3, DVec3)> {
    use hw_gen::layout::{Flight, LinkKind};
    let plan = comet::layout(l)?;
    let town = |p: DVec2, y: f64| {
        let q = l.to_plan(comet::point_to_local(plan.size, p));
        DVec3::new(q.x, l.base + y, q.y)
    };
    let tall = plan
        .spaces
        .iter()
        .filter(|s| {
            matches!(
                s.kind,
                SpaceKind::Lobby | SpaceKind::Atrium | SpaceKind::Canyon
            )
        })
        .max_by(|a, b| a.h.total_cmp(&b.h))?;
    let heart = tall.rect.center();
    Some(match which {
        "lobby" => {
            let door = plan.links.iter().find(|k| k.kind == LinkKind::Entrance)?;
            let eye = door.at + DVec2::new(0.0, 2.5);
            (
                town(eye, 1.9),
                town(heart, tall.y + (tall.h * 0.3).min(6.0)),
            )
        }
        "gallery" => {
            let g = plan
                .spaces
                .iter()
                .filter(|s| s.kind == SpaceKind::Gallery && s.rect.center().distance(heart) > 3.0)
                .max_by(|a, b| {
                    a.y.total_cmp(&b.y)
                        .then(a.rect.area().total_cmp(&b.rect.area()))
                })?;
            // At the rail (head over it), looking across and a little down.
            let edge = g.rect.inset(0.5);
            let eye = heart.clamp(edge.min, edge.max);
            let far = heart + (heart - eye);
            (
                town(eye, g.y + 2.3),
                town(far, g.y - eye.distance(far) * 0.3),
            )
        }
        "stairs" => {
            let st = plan.stairs.first()?;
            let Flight::Straight { from, dir, .. } = *st.flights.first()? else {
                return None;
            };
            let foot = DVec2::new(from.x, from.z);
            let room = plan.spaces[st.host].rect.inset(1.2);
            let eye = (foot - dir * 7.0 + dir.perp() * 3.5).clamp(room.min, room.max);
            (
                town(eye, from.y + 2.2),
                town(foot + dir * 4.0, from.y + 3.0),
            )
        }
        "hall" => {
            let h = plan
                .spaces
                .iter()
                .filter(|s| s.kind == SpaceKind::Hall)
                .max_by(|a, b| a.rect.area().total_cmp(&b.rect.area()))?;
            let eye = h.rect.min + DVec2::splat(1.5);
            (town(eye, h.y + 2.0), town(h.rect.center(), h.y + 2.0))
        }
        _ => {
            let from = l.to_plan(DVec2::new(l.plaza * 0.7, l.plaza + 22.0));
            (
                DVec3::new(from.x, l.base + 13.0, from.y),
                DVec3::new(l.pos.x, l.base + plan.height() * 0.45, l.pos.y),
            )
        }
    })
}
