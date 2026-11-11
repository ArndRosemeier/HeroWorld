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

/// How far an upright part sinks into a floor it ends on, and how far
/// inside an outside wall's middle its cladding meets its plaster (m).
const SINK: f64 = 0.02;
const CLAD_SEAM: f64 = 0.06;

/// How far a lift shaft's walls are drawn inside its outline (m).
const SHAFT_IN: f64 = 0.065;

/// A box in plan from corner to corner (heights are given where it is put).
fn block(lo: DVec2, hi: DVec2) -> Footprint {
    Footprint::Box {
        at: (lo + hi) * 0.5,
        dir: DVec2::Y,
        half: (hi - lo) * 0.5,
        y0: 0.0,
        y1: 1.0,
    }
}

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

/// [`put`], but only what lies within `half` of the middle in plan: what
/// is inside the building ends at the inner face of its outside walls.
fn put_within(
    b: &mut MeshBuilder,
    f: &Footprint,
    span: (f64, f64),
    grow: f64,
    color: Color,
    half: DVec2,
) {
    let Footprint::Box {
        at, dir, half: h, ..
    } = *f
    else {
        return;
    };
    if dir.x.abs() > 1e-6 && dir.y.abs() > 1e-6 {
        // Askew (a stair that is not square to the house): never at a wall.
        return put(b, f, span, grow, color);
    }
    let h = if dir.x.abs() > 0.5 {
        DVec2::new(h.y, h.x)
    } else {
        h
    } + DVec2::splat(grow);
    let (lo, hi) = ((at - h).max(-half), (at + h).min(half));
    if lo.x < hi.x && lo.y < hi.y {
        put(b, &block(lo, hi), span, 0.0, color);
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
    build(b, l, near, &|_| true);
}

/// A look of a kind of furniture: the kind, its width in centimetres and
/// which of the house's three colour sets it wears.
type Look = (hw_gen::layout::furnish::ItemKind, u32, u8);

fn look(it: &hw_gen::layout::furnish::Item) -> Look {
    // One colour set per room, so a room's seats and desks match.
    let room = (it.space as u64).wrapping_mul(0x9E37_79B9) >> 7;
    (it.kind, (it.w * 100.0).round() as u32, (room % 3) as u8)
}

/// One piece of furniture at the maker's origin, its front to +z, in the
/// house colours of `scheme`.
fn piece(m: &mut crate::interior::pieces::Maker, scheme: Scheme, (kind, w, c): Look) {
    use crate::interior::pieces::{self as pc, Cover};
    use hw_gen::layout::furnish::ItemKind as K;
    let (_, band, accent) = livery(scheme);
    let c = c as usize;
    let w = w as f64 / 100.0;
    let walnut = rgb(104, 62, 38);
    let blond = rgb(196, 150, 100);
    let steel = [rgb(120, 140, 130), rgb(140, 140, 136), rgb(110, 124, 140)][c];
    let top = [rgb(70, 110, 90), rgb(60, 70, 64), rgb(150, 110, 70)][c];
    let cloth = [
        band,
        accent,
        rgb(60, 70, 110),
        rgb(150, 100, 80),
        rgb(90, 110, 70),
    ];
    let cover = Cover {
        main: cloth[c],
        second: cloth[c + 2],
        wood: walnut,
    };
    let seed = c as u64 + 1;
    match kind {
        K::Desk => pc::desk(m, steel, top, 0, seed),
        K::SwivelChair => pc::swivel_chair(m, &cover),
        K::Chair => pc::chair(m, &cover),
        K::Cabinet => pc::filing_cabinet(m, steel),
        K::Bookcase => pc::bookcase(m, w, 2.0, walnut, seed),
        K::Shelves => pc::gondola(m, w, rgb(150, 150, 140), seed, true),
        K::Sofa => pc::sofa(m, w, &cover),
        K::Armchair => pc::armchair(m, &cover),
        K::CoffeeTable => {
            let top = pc::kidney(1.3, 0.7, 32);
            let legs = [
                DVec2::new(-0.45, 0.16),
                DVec2::new(0.46, 0.12),
                DVec2::new(0.0, -0.13),
            ];
            pc::coffee_table(m, &top, blond, &legs);
        }
        K::Table => pc::dining_table(m, w, 1.0, walnut),
        K::Counter => pc::shop_counter(m, w, rgb(236, 226, 204), band),
        K::Case => pc::display_case(m, w, walnut, seed),
        K::Plant => pc::plant(
            m,
            1.1 + c as f64 * 0.15,
            [rgb(170, 90, 60), rgb(226, 220, 204), accent][c],
            seed,
        ),
        K::WaterCooler => pc::water_cooler(m),
        K::Rug => {
            let pts = pc::oval(w, w, 40);
            pc::rug(m, &pts, accent, band, 0.35);
        }
    }
}

/// The furniture's levels of detail: sides of round parts, and how far
/// from the eye each level is drawn (m).
const FURNITURE_SEGS: [usize; 3] = [24, 10, 6];
const FURNITURE_REACH: [f32; 3] = [12.0, 32.0, 90.0];

/// A look's mesh with round parts of `seg` sides.
fn piece_mesh(scheme: Scheme, look: Look, seg: usize) -> Mesh {
    // The display case's chrome frame only clears its glass top when its
    // round parts are fine; it is a few hundred triangles either way.
    let seg = if look.0 == hw_gen::layout::furnish::ItemKind::Case {
        24
    } else {
        seg
    };
    let mut b = MeshBuilder::new();
    let mut m =
        crate::interior::pieces::Maker::new(&mut b, DVec3::ZERO, DVec3::Z, seg, Wear::Fresh);
    piece(&mut m, scheme, look);
    b.finish()
}

/// The furniture of every Comet building (`hw_gen::layout::furnish`): one
/// model per look, close-up to far, and where each of them stands. Every
/// piece is drawn where its solid box is.
pub fn furniture_models(
    town: &hw_gen::urban::world::TownWorld,
) -> Vec<(Vec<LodLevel>, Vec<Place>)> {
    use hw_gen::urban::landmarks::LandmarkKind;
    let mut out = Vec::new();
    for l in town
        .landmarks
        .iter()
        .filter(|l| l.kind == LandmarkKind::CometBuilding)
    {
        let Some(plan) = comet::layout(l) else {
            continue;
        };
        let mut places: std::collections::BTreeMap<(u8, u32, u8), (Look, Vec<Place>)> =
            Default::default();
        for it in comet::furniture(l) {
            let lk = look(&it);
            let p = l.to_plan(it.at);
            let f = l.to_plan(it.at + it.face) - p;
            // Chairs stand a little askew, as people leave them.
            let askew = match it.kind {
                hw_gen::layout::furnish::ItemKind::SwivelChair => (it.seed % 7) as f64 * 5.0 - 15.0,
                hw_gen::layout::furnish::ItemKind::Chair => (it.seed % 5) as f64 * 3.0 - 6.0,
                _ => 0.0,
            };
            let yaw = f.x.atan2(f.y).to_degrees() + askew;
            places
                .entry((lk.0 as u8, lk.1, lk.2))
                .or_insert_with(|| (lk, Vec::new()))
                .1
                .push(
                    Place::new(p.x as f32, (l.base + it.y) as f32, p.y as f32)
                        .with_yaw_deg(yaw as f32),
                );
        }
        for (lk, at) in places.into_values() {
            let levels = FURNITURE_SEGS
                .into_iter()
                .zip(FURNITURE_REACH)
                .map(|(seg, reach)| LodLevel::new(piece_mesh(plan.scheme, lk, seg), reach))
                .collect();
            out.push((levels, at));
        }
    }
    out
}

/// The building's parts that `keep` lets through (all of them in the game;
/// the z-fighting check takes them kind by kind to name what clashes).
fn build(b: &mut MeshBuilder, l: &Landmark, near: bool, keep: &dyn Fn(&Part) -> bool) {
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

    // Where floors lie: an upright part that ends on one sinks a little
    // into it, so its end face is never in the floor's own plane.
    let bounds = |f: &Footprint| {
        let (x0, x1) = f.extent(DVec2::ZERO, DVec2::X);
        let (z0, z1) = f.extent(DVec2::ZERO, DVec2::Y);
        (DVec2::new(x0, z0), DVec2::new(x1, z1))
    };
    let floors: Vec<(DVec2, DVec2, f64)> = parts
        .iter()
        .filter(|p| matches!(p.part, Part::Floor(_) | Part::Deck))
        .map(|p| {
            let (lo, hi) = bounds(&p.shape);
            (lo, hi, p.shape.span().1)
        })
        .collect();
    let on_floor = |f: &Footprint, y: f64| {
        let (lo, hi) = bounds(f);
        floors.iter().any(|(a, c, top)| {
            (top - y).abs() < 1e-3
                && lo.x < c.x - 1e-3
                && a.x < hi.x - 1e-3
                && lo.y < c.y - 1e-3
                && a.y < hi.y - 1e-3
        })
    };

    let inside = plan.size * 0.5 - DVec2::splat(WALL * 0.5);
    // The lift shafts' outlines: their walls are drawn a little inside.
    let shafts: Vec<(DVec2, DVec2)> = plan
        .spaces
        .iter()
        .filter(|s| s.kind == SpaceKind::Shaft)
        .map(|s| {
            let r = Footprint::Box {
                at: s.rect.center(),
                dir: DVec2::Y,
                half: s.rect.size() * 0.5,
                y0: 0.0,
                y1: 1.0,
            };
            bounds(&comet::to_local(plan.size, r))
        })
        .collect();

    for Piece { part, shape } in &parts {
        let (mut y0, mut y1) = shape.span();
        // No two kinds of part end in one plane: each sinks into the floors
        // it touches by its own depth and is trimmed or stretched in plan
        // by its own few centimetres. Stretched strips of one wall overlap
        // each other, which shows nothing: they look the same.
        let (lo, hi) = bounds(shape);
        let size = hi - lo;
        let shaft_wall = match part {
            Part::Shaft => true,
            Part::Glass => size.min_element() > 0.15,
            _ => false,
        };
        let sink = match part {
            Part::Wall => Some(SINK),
            Part::Outer => Some(SINK + 0.015),
            // (An outside wall's plaster takes SINK + 0.03.)
            Part::Glass | Part::Rail if !shaft_wall => Some(SINK + 0.045),
            Part::Glass | Part::Shaft => Some(SINK + 0.06),
            _ => None,
        };
        if let Some(sink) = sink {
            if on_floor(shape, y0) {
                y0 -= sink;
            }
            if on_floor(shape, y1) {
                y1 -= sink;
            } else if shaft_wall {
                // Up into the roof, past the tops of the walls beside it.
                y1 += 0.03;
            } else if *part == Part::Glass {
                y1 += 0.015;
            }
        }
        if *part == Part::Step && on_floor(shape, y1) {
            // The top step lies under the landing it reaches.
            y1 -= 0.015;
        }
        let (along, across) = match part {
            Part::Wall => (0.015, -0.012),
            Part::Glass if !shaft_wall => (-0.015, -0.02),
            Part::Rail => (-0.015, -0.012),
            _ => (0.0, 0.0),
        };
        let trimmed;
        let shape = if shaft_wall {
            let c = (lo + hi) * 0.5;
            let Some((a, e)) = shafts.iter().find(|(a, e)| {
                a.x - 0.5 < c.x && c.x < e.x + 0.5 && a.y - 0.5 < c.y && c.y < e.y + 0.5
            }) else {
                continue;
            };
            let (lo, hi) = (
                lo.max(*a + DVec2::splat(SHAFT_IN)),
                hi.min(*e - DVec2::splat(SHAFT_IN)),
            );
            trimmed = block(lo, hi);
            &trimmed
        } else if let (
            true,
            Footprint::Box {
                at,
                dir,
                half,
                y0,
                y1,
            },
        ) = (along != 0.0 || across != 0.0, *shape)
        {
            let grow = if half.x < half.y {
                DVec2::new(across, along)
            } else {
                DVec2::new(along, across)
            };
            trimmed = Footprint::Box {
                at,
                dir,
                half: half + grow,
                y0,
                y1,
            };
            &trimmed
        } else {
            shape
        };
        if !keep(part) {
            continue;
        }
        if !near
            && !matches!(
                part,
                Part::Outer | Part::Roof | Part::Window | Part::Canopy | Part::Fin
            )
        {
            continue;
        }
        // What is indoors stops at the outside walls' inner faces.
        let indoors = !matches!(part, Part::Window | Part::Canopy | Part::Fin);
        // Each kind reaches its own little way into the plaster, so no
        // two kinds end in one plane there.
        let reach = match part {
            Part::Wall => 0.015,
            Part::Shaft => 0.03,
            Part::Glass | Part::Rail => 0.045,
            Part::Step => 0.06,
            _ => 0.0,
        };
        let put = |b: &mut MeshBuilder, f: &Footprint, span: (f64, f64), grow: f64, c: Color| {
            if indoors {
                put_within(b, f, span, grow, c, inside + DVec2::splat(reach));
            } else {
                put(b, f, span, grow, c);
            }
        };
        match *part {
            Part::Outer => outer(b, shape, near, (clad, band, plaster), top, (y0, y1)),
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
            // The landing doors slide: see `LiftView`.
            Part::LiftDoor => {}
        }
    }
    plain(b, Surf::Plain);
    if near {
        lamps(b, &plan, brass);
    }
    let _ = RAIL;
}

/// An outside wall: cladding outside, plaster inside, a band of colour at
/// the foot and under the roof.
fn outer(
    b: &mut MeshBuilder,
    f: &Footprint,
    near: bool,
    (clad, band, plaster): (Color, Color, Color),
    top: f64,
    (y0, y1): (f64, f64),
) {
    let Footprint::Box { at, dir, half, .. } = *f else {
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
    // The cladding is the thicker layer: where the two meet lies inside
    // the floors' edges, which end on the wall's middle line.
    let thin = half.x.min(half.y);
    let seam = -CLAD_SEAM;
    let layer = |b: &mut MeshBuilder, side: f64, lo: f64, hi: f64, color: Color| {
        if hi - lo < 0.01 {
            return;
        }
        let (a, c) = if side > 0.0 {
            (seam, thin)
        } else {
            (-thin, seam)
        };
        let t = (c - a) * 0.5;
        let h = if thin_x {
            DVec2::new(t, half.y)
        } else {
            DVec2::new(half.x, t)
        };
        let (c, u, hh) = frame(at + out * ((a + c) * 0.5), dir, h, lo, hi);
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
        // A little lower than the cladding, so at a corner the two walls'
        // layers never end in one plane.
        layer(b, -1.0, y0 - 0.015, y1 - 0.015, plaster);
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
        "lift" => {
            // In front of the first lift's ground-floor doors, a little
            // to one side.
            let runs = hw_gen::layout::lifts::runs(&plan);
            let run = runs.first()?;
            let (_, d) = run.doors.iter().find(|d| d.0 == 0)?;
            let out = (d.at() - run.car.at()).normalize_or_zero();
            let eye = d.at() + out * 5.5 + out.perp() * 1.6;
            (
                town(eye, run.stops[0] + 1.8),
                town(d.at(), run.stops[0] + 1.6),
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

/// The lifts' cars and landing doors, moving by the game clock (the
/// server makes them solid from the same timetable: `hw_sim::lifts`).
pub struct LiftView {
    lifts: hw_sim::lifts::Lifts,
    /// Per lift: its car, and per door its two leaves.
    spawned: Vec<Option<(EntityId, Vec<[EntityId; 2]>)>>,
}

/// Lifts are drawn within this of the eye (m).
const LIFT_REACH: f64 = 160.0;

impl LiftView {
    pub fn new(town: &hw_gen::urban::world::TownWorld) -> Self {
        let lifts = hw_sim::lifts::Lifts::new(town);
        let spawned = vec![None; lifts.runs.len()];
        Self { lifts, spawned }
    }

    /// The lifts as solids, for a joined player's own hero.
    pub fn lifts(&self) -> &hw_sim::lifts::Lifts {
        &self.lifts
    }

    pub fn update(&mut self, world: &mut World, eye: Vec3, now: f64) -> EngineResult<()> {
        use hw_gen::layout::lifts::{CAR_FLOOR, CAR_LIP, CAR_ROOM};
        use hw_gen::layout::solid::LIFT_DOOR_H;
        let eye2 = DVec2::new(eye.x as f64, eye.z as f64);
        let brass = pieces::brass();
        for (run, slot) in self.lifts.runs.iter().zip(&mut self.spawned) {
            let Footprint::Box { at, dir, half, .. } = run.car else {
                continue;
            };
            if at.distance(eye2) > LIFT_REACH {
                if let Some((car, doors)) = slot.take() {
                    world.despawn(car);
                    for d in doors.into_iter().flatten() {
                        world.despawn(d);
                    }
                }
                continue;
            }
            // A door's long way in plan, its width and its middle.
            let door = |f: &Footprint| {
                let Footprint::Box { at, dir, half, .. } = *f else {
                    return (DVec2::X, 0.0, DVec2::ZERO);
                };
                let x = DVec2::new(dir.y, -dir.x);
                if half.x >= half.y {
                    (x, half.x * 2.0, at)
                } else {
                    (dir, half.y * 2.0, at)
                }
            };
            if slot.is_none() {
                // The car: floor, roof with a light under it, corner posts.
                let mut b = MeshBuilder::new();
                let u = DVec3::new(dir.y, 0.0, -dir.x);
                skin(&mut b, true, Base::Lino);
                b.oriented_box(
                    DVec3::Y * ((CAR_LIP - CAR_FLOOR) * 0.5),
                    u,
                    DVec3::new(half.x, (CAR_LIP + CAR_FLOOR) * 0.5, half.y),
                    rgb(196, 60, 50),
                );
                plain(&mut b, Surf::Metal);
                b.oriented_box(
                    DVec3::Y * (CAR_ROOM + 0.075),
                    u,
                    DVec3::new(half.x, 0.075, half.y),
                    brass,
                );
                for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                    let p = DVec2::new(dir.y, -dir.x) * (sx * (half.x - 0.08))
                        + dir * (sz * (half.y - 0.08));
                    b.cylinder(
                        DVec3::new(p.x, CAR_LIP, p.y),
                        (0.05, 0.05),
                        CAR_ROOM - CAR_LIP,
                        12,
                        false,
                        brass,
                    );
                }
                plain(&mut b, Surf::Glow);
                b.oriented_box(
                    DVec3::Y * (CAR_ROOM - 0.03),
                    u,
                    DVec3::new(half.x * 0.6, 0.03, half.y * 0.6),
                    rgb(255, 236, 190),
                );
                let car = world.spawn(b.finish());
                let mut doors = Vec::new();
                for (_, d) in &run.doors {
                    let (a, w, _) = door(d);
                    let mut leaf = MeshBuilder::new();
                    plain(&mut leaf, Surf::Metal);
                    leaf.oriented_box(
                        DVec3::Y * (LIFT_DOOR_H * 0.5),
                        DVec3::new(a.x, 0.0, a.y),
                        DVec3::new(w * 0.25, LIFT_DOOR_H * 0.5, 0.03),
                        brass,
                    );
                    let mesh = leaf.finish();
                    doors.push([world.spawn(mesh.clone()), world.spawn(mesh)]);
                }
                *slot = Some((car, doors));
            }
            let Some((car, doors)) = slot else { continue };
            let state = run.at(now);
            world.set_place(*car, Place::new(at.x as f32, state.y as f32, at.y as f32))?;
            for ((k, d), leaves) in run.doors.iter().zip(doors.iter()) {
                let (a, w, mid) = door(d);
                let open = state.open.filter(|(s, _)| s == k).map_or(0.0, |(_, o)| o);
                let (y0, _) = d.span();
                for (leaf, side) in leaves.iter().zip([-1.0, 1.0]) {
                    // The doors rise into the shaft wall over the doorway
                    // (to the side they would stick out past its corner).
                    let p = mid + a * (side * w * 0.25);
                    let y = y0 + open * LIFT_DOOR_H * 0.98;
                    world.set_place(*leaf, Place::new(p.x as f32, y as f32, p.y as f32))?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hw_gen::urban::landmarks::LandmarkKind;

    fn plan_of(l: &Landmark) -> Scheme {
        comet::layout(l).expect("a plan").scheme
    }

    /// No two faces of a Comet building lie in one plane over each other.
    #[test]
    #[ignore = "on demand (looks): scripts/check.sh --on-demand"]
    fn the_comet_buildings_have_no_z_fighting() {
        let mut bad = Vec::new();
        // Each scheme on a few sites: the site is the plan's seed.
        for (v, k) in (0..3u8).flat_map(|v| (0..4).map(move |k| (v, k))) {
            let l = Landmark {
                kind: LandmarkKind::CometBuilding,
                pos: DVec2::new(100.0 + 37.0 * k as f64, -40.0 + 11.0 * k as f64),
                plaza: comet::plaza(0),
                base: 3.0,
                low: 2.0,
                height: 30.0,
                dir: DVec2::Y,
                stairs: Vec::new(),
                variant: v,
            };
            let mut check =
                engine::zfight::ZFightCheck::new().options(crate::zfight::city_options());
            let name = |p: &Part| {
                let s = format!("{p:?}");
                s.split('(').next().unwrap_or("").to_string()
            };
            let mut kinds: Vec<String> = comet::parts(&l).iter().map(|p| name(&p.part)).collect();
            kinds.sort();
            kinds.dedup();
            for k in &kinds {
                let mut b = MeshBuilder::new();
                build(&mut b, &l, true, &|p| name(p) == *k);
                check = check.with(k.clone(), &b.finish());
            }
            let mut f = MeshBuilder::new();
            for it in comet::furniture(&l) {
                let o = DVec3::new(it.at.x, it.y, it.at.y);
                let front = DVec3::new(it.face.x, 0.0, it.face.y);
                let mut m = crate::interior::pieces::Maker::new(&mut f, o, front, 24, Wear::Fresh);
                piece(&mut m, plan_of(&l), look(&it));
            }
            check = check.with("Furniture", &f.finish());
            let report = check.run();
            if k == 0 {
                eprintln!(
                    "{}: {} triangles, {} pieces of furniture",
                    comet::name(&l),
                    report.triangles,
                    comet::furniture(&l).len()
                );
            }
            if !report.is_clean() {
                // By the two kinds of part and the way the faces look: one
                // line per kind of clash, with its area and a place to look.
                let mut found: std::collections::BTreeMap<String, (f64, usize, DVec3)> =
                    Default::default();
                for c in report.fighting() {
                    let axis = if c.normal.y > 0.9 {
                        "up"
                    } else if c.normal.y < -0.9 {
                        "down"
                    } else {
                        "side"
                    };
                    let mut two = [
                        format!(
                            "{} {}",
                            report.source_name(c.sources[0]),
                            crate::zfight::hex(c.colors[0])
                        ),
                        format!(
                            "{} {}",
                            report.source_name(c.sources[1]),
                            crate::zfight::hex(c.colors[1])
                        ),
                    ];
                    two.sort();
                    let e = found
                        .entry(format!("{} / {} {axis}", two[0], two[1]))
                        .or_insert((0.0, 0, c.at));
                    e.0 += c.area;
                    e.1 += 1;
                }
                let mut lines: Vec<_> = found.into_iter().collect();
                lines.sort_by(|a, b| b.1 .0.total_cmp(&a.1 .0));
                bad.push(format!("{} (site {k}):", comet::name(&l)));
                for (k, (area, n, at)) in lines {
                    bad.push(format!(
                        "  {k}: {area:.1} m2 in {n}, e.g. ({:.2}, {:.2}, {:.2})",
                        at.x, at.y, at.z
                    ));
                }
            }
        }
        assert!(bad.is_empty(), "{}", bad.join("\n"));
    }

    /// Every look of furniture at every level of detail, each on its own.
    #[test]
    #[ignore = "on demand (looks): scripts/check.sh --on-demand"]
    fn furniture_has_no_z_fighting_at_any_detail() {
        use hw_gen::layout::furnish::ItemKind as K;
        let mut bad = Vec::new();
        for (kind, w) in [
            (K::Desk, 155),
            (K::SwivelChair, 60),
            (K::Chair, 50),
            (K::Cabinet, 48),
            (K::Bookcase, 180),
            (K::Shelves, 240),
            (K::Sofa, 200),
            (K::Armchair, 90),
            (K::CoffeeTable, 130),
            (K::Table, 240),
            (K::Counter, 400),
            (K::Case, 160),
            (K::Plant, 50),
            (K::WaterCooler, 40),
            (K::Rug, 460),
        ] {
            for seg in FURNITURE_SEGS {
                let mesh = piece_mesh(Scheme::Atrium, (kind, w, 1), seg);
                let report = engine::zfight::ZFightCheck::new()
                    .options(crate::zfight::city_options())
                    .with("piece", &mesh)
                    .run();
                if !report.is_clean() {
                    bad.push(format!("{kind:?} at {seg} sides: {}", report.summary(3)));
                }
            }
        }
        assert!(bad.is_empty(), "{}", bad.join("\n"));
    }
}
