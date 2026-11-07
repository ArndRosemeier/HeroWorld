//! The outside of a building, from its plan and look: walls with real
//! window openings, frames and sills, piers and floor bands, a stone base,
//! cornices, shop fronts with signs and awnings, entrance canopies,
//! balconies, fire escapes, roofs (flat with rooftop gear, gable, hip,
//! mansard, dome, Googie swoop, sawtooth), tower crowns, corner towers,
//! fins and neon signs.
//!
//! Near detail draws all of it; far detail keeps the silhouette (storeys,
//! roofs, crowns, towers) with windows as flat panes.
//!
//! Everything is built in building coordinates (x along the front, y up,
//! z inwards) and placed in the city by the site transform. Pieces that
//! touch never share a face in the same direction: the z-fighting test
//! checks every building.

use crate::citymesh::{MeshBuilder, Surf};
use engine::prelude::*;
use engine::texgen::{FacadeLayer, Layer, RoofLayer};
use glam::{DVec2, DVec3};
use hw_core::Rng;
use hw_gen::building::{bay_lines, BuildingPlan, Floor, Opening, OpeningKind, RoomKind, Side};
use hw_gen::geom::Rect;
use hw_gen::style::{Cornice, Crown, Look, Piers, Rgb, Ridge, Roof, Style, WindowKind};
use std::f64::consts::PI;

/// How much to draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detail {
    Near,
    Far,
}

/// Thickness of roof slabs (and of terraces where a storey steps back).
pub const ROOF_SLAB: f64 = 0.3;
/// How far a far-detail window pane stands out of the wall.
const PANE_OFFSET: f64 = 0.05;

fn col(c: Rgb) -> Color {
    rgb(c[0], c[1], c[2])
}

/// A colour darkened (k < 1) or lightened (k > 1).
fn shade(c: Rgb, k: f64) -> Color {
    let f = |v: u8| -> u8 {
        let v = v as f64;
        if k <= 1.0 {
            (v * k).round().clamp(0.0, 255.0) as u8
        } else {
            (v + (255.0 - v) * (k - 1.0)).round().clamp(0.0, 255.0) as u8
        }
    };
    rgb(f(c[0]), f(c[1]), f(c[2]))
}

/// Drawing in one building's coordinates.
struct Ctx<'a> {
    b: &'a mut MeshBuilder,
    plan: &'a BuildingPlan,
    look: &'a Look,
    /// Height of the building's ground above the city's y = 0.
    base: f64,
    u: DVec3,
    v: DVec3,
}

impl Ctx<'_> {
    /// Building point → city point.
    fn p(&self, q: DVec3) -> DVec3 {
        self.plan
            .site
            .to_city(DVec3::new(q.x, q.y + self.base, q.z))
    }

    /// Building direction → city direction.
    fn dir(&self, d: DVec3) -> DVec3 {
        self.u * d.x + DVec3::Y * d.y + self.v * d.z
    }

    /// An axis-aligned box in building coordinates, from its corners.
    fn bx(&mut self, lo: DVec3, hi: DVec3, color: Color) {
        let (lo, hi) = (lo.min(hi), lo.max(hi));
        if (hi - lo).min_element() < 1e-4 {
            return;
        }
        let c = self.p((lo + hi) * 0.5);
        let u = self.u;
        self.b.oriented_box(c, u, (hi - lo) * 0.5, color);
    }

    /// A box standing on something: no bottom face (it would hide in the
    /// surface below and double with other bottoms there).
    fn stand(&mut self, lo: DVec3, hi: DVec3, color: Color) {
        let (lo, hi) = (lo.min(hi), lo.max(hi));
        if (hi - lo).min_element() < 1e-4 {
            return;
        }
        let p = |x: f64, y: f64, z: f64| DVec3::new(x, y, z);
        let (a, b) = (lo, hi);
        self.quad(
            [
                p(a.x, b.y, a.z),
                p(b.x, b.y, a.z),
                p(b.x, b.y, b.z),
                p(a.x, b.y, b.z),
            ],
            DVec3::Y,
            color,
        );
        self.quad(
            [
                p(a.x, a.y, a.z),
                p(b.x, a.y, a.z),
                p(b.x, b.y, a.z),
                p(a.x, b.y, a.z),
            ],
            -DVec3::Z,
            color,
        );
        self.quad(
            [
                p(a.x, a.y, b.z),
                p(b.x, a.y, b.z),
                p(b.x, b.y, b.z),
                p(a.x, b.y, b.z),
            ],
            DVec3::Z,
            color,
        );
        self.quad(
            [
                p(a.x, a.y, a.z),
                p(a.x, a.y, b.z),
                p(a.x, b.y, b.z),
                p(a.x, b.y, a.z),
            ],
            -DVec3::X,
            color,
        );
        self.quad(
            [
                p(b.x, a.y, a.z),
                p(b.x, a.y, b.z),
                p(b.x, b.y, b.z),
                p(b.x, b.y, a.z),
            ],
            DVec3::X,
            color,
        );
    }

    fn quad(&mut self, q: [DVec3; 4], out: DVec3, color: Color) {
        let pts = q.map(|x| self.p(x));
        let o = self.dir(out);
        self.b.quad(pts, o, color);
    }

    fn polygon(&mut self, q: &[DVec3], out: DVec3, color: Color) {
        let pts: Vec<DVec3> = q.iter().map(|x| self.p(*x)).collect();
        let o = self.dir(out);
        self.b.polygon(&pts, o, color);
    }

    fn hexa(&mut self, bottom: [DVec3; 4], top: [DVec3; 4], color: Color) {
        let (b, t) = (bottom.map(|x| self.p(x)), top.map(|x| self.p(x)));
        self.b.hexa(b, t, color);
    }

    #[allow(clippy::too_many_arguments)]
    fn cylinder(
        &mut self,
        c: DVec3,
        (r, r_top): (f64, f64),
        h: f64,
        seg: usize,
        caps: bool,
        color: Color,
    ) {
        let c = self.p(c);
        self.b.cylinder(c, (r, r_top), h, seg, caps, color);
    }

    fn dome(&mut self, c: DVec3, r: f64, color: Color) {
        let c = self.p(c);
        self.b.dome(c, r, 20, 6, color);
    }

    fn surf(&mut self, s: Surf) {
        self.b.surf = s;
    }
}

/// One outside wall of a floor outline, with coordinates along it (t), up
/// (y) and out of it (d).
#[derive(Clone, Copy)]
struct Wall {
    side: Side,
    o: Rect,
}

impl Wall {
    fn range(&self) -> (f64, f64) {
        match self.side {
            Side::Front | Side::Back => (self.o.min.x, self.o.max.x),
            Side::Left | Side::Right => (self.o.min.y, self.o.max.y),
        }
    }

    fn at(&self, t: f64, y: f64, d: f64) -> DVec3 {
        match self.side {
            Side::Front => DVec3::new(t, y, self.o.min.y - d),
            Side::Back => DVec3::new(t, y, self.o.max.y + d),
            Side::Left => DVec3::new(self.o.min.x - d, y, t),
            Side::Right => DVec3::new(self.o.max.x + d, y, t),
        }
    }

    fn out(&self) -> DVec3 {
        match self.side {
            Side::Front => -DVec3::Z,
            Side::Back => DVec3::Z,
            Side::Left => -DVec3::X,
            Side::Right => DVec3::X,
        }
    }

    fn along(&self) -> DVec3 {
        match self.side {
            Side::Front | Side::Back => DVec3::X,
            Side::Left | Side::Right => DVec3::Z,
        }
    }

    /// A flat rectangle on the wall at depth `d`, facing `out`.
    fn rect(&self, t0: f64, t1: f64, y0: f64, y1: f64, d: f64) -> [DVec3; 4] {
        [
            self.at(t0, y0, d),
            self.at(t1, y0, d),
            self.at(t1, y1, d),
            self.at(t0, y1, d),
        ]
    }
}

const SIDES: [Side; 4] = [Side::Front, Side::Right, Side::Back, Side::Left];

/// A box on a wall: t, y and d ranges. A box that starts at the wall
/// (d = 0) has no back face: it would hide inside the wall and double
/// with the backs of other boxes there.
fn wall_box(c: &mut Ctx, w: &Wall, t: (f64, f64), y: (f64, f64), d: (f64, f64), color: Color) {
    let (t0, t1) = (t.0.min(t.1), t.0.max(t.1));
    let (y0, y1) = (y.0.min(y.1), y.0.max(y.1));
    let (d0, d1) = (d.0.min(d.1), d.0.max(d.1));
    if t1 - t0 < 1e-4 || y1 - y0 < 1e-4 || d1 - d0 < 1e-4 {
        return;
    }
    let a = w.along();
    let out = w.out();
    // Front, top, bottom, ends.
    c.quad(w.rect(t0, t1, y0, y1, d1), out, color);
    c.quad(
        [
            w.at(t0, y1, d0),
            w.at(t1, y1, d0),
            w.at(t1, y1, d1),
            w.at(t0, y1, d1),
        ],
        DVec3::Y,
        color,
    );
    c.quad(
        [
            w.at(t0, y0, d0),
            w.at(t1, y0, d0),
            w.at(t1, y0, d1),
            w.at(t0, y0, d1),
        ],
        -DVec3::Y,
        color,
    );
    c.quad(
        [
            w.at(t0, y0, d0),
            w.at(t0, y1, d0),
            w.at(t0, y1, d1),
            w.at(t0, y0, d1),
        ],
        -a,
        color,
    );
    c.quad(
        [
            w.at(t1, y0, d0),
            w.at(t1, y1, d0),
            w.at(t1, y1, d1),
            w.at(t1, y0, d1),
        ],
        a,
        color,
    );
    if d0.abs() > 1e-9 {
        c.quad(w.rect(t0, t1, y0, y1, d0), -out, color);
    }
}

/// The front-right corner tower of streamline buildings: centre and radius.
fn corner_tower(plan: &BuildingPlan) -> Option<(DVec2, f64)> {
    let o = plan.floors.first()?.outline;
    let r = (o.size().min_element() * 0.16).clamp(2.2, 3.6);
    // It stands out of each wall by only half a metre, clear of neighbours.
    Some((DVec2::new(o.max.x - r + 0.5, o.min.y + r - 0.5), r))
}

/// Does an opening fall inside the corner tower?
fn hidden_by_tower(plan: &BuildingPlan, op: &Opening) -> bool {
    if !plan.look.corner_tower {
        return false;
    }
    let Some((c, r)) = corner_tower(plan) else {
        return false;
    };
    let (a0, a1) = (op.along - op.width * 0.5, op.along + op.width * 0.5);
    match op.side {
        Side::Front => a1 > c.x - r - 0.3,
        Side::Right => a0 < c.y + r + 0.3,
        _ => false,
    }
}

/// Runs of floors with the same outline: (first, last) indices.
fn tiers(plan: &BuildingPlan, shown: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < shown {
        let mut j = i;
        while j + 1 < shown && plan.floors[j + 1].outline == plan.floors[i].outline {
            j += 1;
        }
        out.push((i, j));
        i = j + 1;
    }
    out
}

/// Where a floor's walls start: above the terrace slab when the floor below
/// is wider (a setback), else at the floor.
fn wall_bottom(plan: &BuildingPlan, level: usize) -> f64 {
    let f = &plan.floors[level];
    if level > 0 && plan.floors[level - 1].outline != f.outline {
        f.elevation + ROOF_SLAB
    } else {
        f.elevation
    }
}

/// Draws the outside of `plan`'s first `shown` floors. `roofed`: draw the
/// roof and crown on top (false when the building is cut open above).
pub fn exterior(
    b: &mut MeshBuilder,
    plan: &BuildingPlan,
    shown: usize,
    roofed: bool,
    detail: Detail,
) {
    let site = &plan.site;
    let mut c = Ctx {
        b,
        plan,
        look: &plan.look,
        base: crate::citymesh::BLOCK_TOP,
        u: DVec3::new(site.along.x, 0.0, site.along.y),
        v: DVec3::new(site.inward.x, 0.0, site.inward.y),
    };
    let look = &plan.look;
    c.b.tex = Some(wall_layer(look));
    let ts = tiers(plan, shown);
    for (n, &(i, j)) in ts.iter().enumerate() {
        let top_tier = n + 1 == ts.len();
        for level in i..=j {
            match detail {
                Detail::Near => near_floor(&mut c, level, level == i),
                Detail::Far => far_floor(&mut c, level),
            }
        }
        if detail == Detail::Near && look.piers == Piers::Full {
            full_piers(&mut c, i, j);
        }
        // Cornice and roof slab, except under a cut-open floor.
        let f = &plan.floors[j];
        let y = f.elevation + f.height;
        if top_tier && !roofed {
            continue;
        }
        let deck = cornice(&mut c, plan.floors[i].outline, y, detail);
        let last = top_tier && shown == plan.floors.len();
        if !last && look.parapet > 0.0 {
            // A terrace: a low railing wall along the edge.
            parapet(&mut c, plan.floors[i].outline, deck, 1.1, 0.25);
        }
        if last && roofed {
            roof(&mut c, plan.floors[i].outline, deck, detail);
        }
    }
    if look.corner_tower && shown > 0 {
        tower(&mut c, &ts, shown, roofed, detail);
    }
    if look.fins && shown == plan.floors.len() && roofed {
        fins(&mut c);
    }
    if look.sign && shown >= 3 {
        sign(&mut c);
    }
    c.surf(Surf::Plain);
    c.b.tex = None;
}

/// The texture of a building's walls (tinted by its wall colour).
pub fn wall_layer(look: &Look) -> Layer {
    use hw_gen::style::Material;
    let [r, g, b] = look.wall.map(|v| v as f64);
    let l: Layer = match look.material {
        Material::Limestone => FacadeLayer::Limestone.into(),
        Material::Granite => FacadeLayer::Granite.into(),
        Material::Brick if r + g + b < 300.0 => FacadeLayer::BrickBrown.into(),
        Material::Brick if g > r * 0.75 => FacadeLayer::BrickYellow.into(),
        Material::Brick => FacadeLayer::BrickRed.into(),
        Material::Terracotta => FacadeLayer::Sandstone.into(),
        Material::Stucco if look.style == Style::OldTown => FacadeLayer::Plaster.into(),
        Material::Stucco => FacadeLayer::Stucco.into(),
        Material::Concrete => FacadeLayer::Concrete.into(),
        Material::Metal | Material::Glass => FacadeLayer::MetalPanel.into(),
    };
    let _ = b;
    l
}

/// The texture of a pitched or mansard roof.
fn roof_layer(look: &Look) -> Layer {
    let [r, g, _] = look.roof_color.map(|v| v as f64);
    let l = if r > g + 40.0 {
        RoofLayer::ClayTile
    } else if look.roof == Roof::Mansard && look.style == Style::Haussmann {
        RoofLayer::Zinc
    } else if look.style == Style::Cottage {
        RoofLayer::AsphaltShingle
    } else {
        RoofLayer::Slate
    };
    l.into()
}

/// Far detail: plain walls and flat window panes.
fn far_floor(c: &mut Ctx, level: usize) {
    let plan = c.plan;
    let look = c.look;
    let f = &plan.floors[level];
    let y0 = wall_bottom(plan, level);
    let y1 = f.elevation + f.height;
    let o = f.outline;
    let wall = col(look.wall);
    let plinth = col(look.base);
    if level == 0 && look.plinth > 0.0 {
        let h = look.plinth.min(f.height * 0.5);
        c.bx(
            DVec3::new(o.min.x, y0, o.min.y),
            DVec3::new(o.max.x, h, o.max.y),
            plinth,
        );
        c.bx(
            DVec3::new(o.min.x, h, o.min.y),
            DVec3::new(o.max.x, y1, o.max.y),
            wall,
        );
    } else {
        c.bx(
            DVec3::new(o.min.x, y0, o.min.y),
            DVec3::new(o.max.x, y1, o.max.y),
            wall,
        );
    }
    for op in plan.openings.iter().filter(|op| op.floor as usize == level) {
        if hidden_by_tower(plan, op) {
            continue;
        }
        let w = Wall { side: op.side, o };
        let (t0, t1) = (op.along - op.width * 0.5, op.along + op.width * 0.5);
        let ya = (f.elevation + op.sill).max(y0 + 0.05);
        let yb = f.elevation + op.sill + op.height;
        let (color, surf) = match op.kind {
            OpeningKind::Window | OpeningKind::Entrance => (col(look.glass), Surf::Glass),
            OpeningKind::Door => (shade(look.frame, 0.8), Surf::Plain),
        };
        c.surf(surf);
        let q = w.rect(t0, t1, ya, yb, PANE_OFFSET);
        c.quad(q, w.out(), color);
        c.surf(Surf::Plain);
    }
}

/// Near detail: one floor's walls around real openings, with reveals,
/// glass, frames, sills, and the floor's own ornament.
fn near_floor(c: &mut Ctx, level: usize, first_of_tier: bool) {
    let plan = c.plan;
    let look = c.look;
    let f = &plan.floors[level];
    let y0 = wall_bottom(plan, level);
    let y1 = f.elevation + f.height;
    let o = f.outline;
    let wall = col(look.wall);
    let base_c = col(look.base);
    let trim = col(look.trim);
    let plinth = if level == 0 && look.plinth > 0.0 {
        look.plinth.min(f.height * 0.45)
    } else {
        0.0
    };
    for side in SIDES {
        let w = Wall { side, o };
        let (s0, s1) = w.range();
        // Openings cut into this wall (portholes sit on it instead).
        let mut holes: Vec<(&Opening, f64, f64, f64, f64)> = plan
            .openings
            .iter()
            .filter(|op| op.floor as usize == level && op.side == side)
            .filter(|op| !hidden_by_tower(plan, op))
            .map(|op| {
                let ya = (f.elevation + op.sill).max(y0 + 0.05);
                let yb = (f.elevation + op.sill + op.height).min(y1 - 0.05);
                (
                    op,
                    op.along - op.width * 0.5,
                    op.along + op.width * 0.5,
                    ya,
                    yb,
                )
            })
            .collect();
        holes.sort_by(|a, b| a.1.total_cmp(&b.1));
        let cut: Vec<_> = holes
            .iter()
            .filter(|h| {
                !(h.0.kind == OpeningKind::Window
                    && look.window == WindowKind::Porthole
                    && !street_glass(plan, f, h.0))
            })
            .copied()
            .collect();
        // Wall pieces: columns between holes, and above and below each hole.
        let mut pieces: Vec<(f64, f64, f64, f64)> = Vec::new();
        let mut t = s0;
        for &(_, t0, t1, ya, yb) in &cut {
            if t0 > t {
                pieces.push((t, t0, y0, y1));
            }
            pieces.push((t0, t1, y0, ya));
            pieces.push((t0, t1, yb, y1));
            t = t.max(t1);
        }
        if t < s1 {
            pieces.push((t, s1, y0, y1));
        }
        for (t0, t1, ya, yb) in pieces {
            if t1 - t0 < 1e-4 || yb - ya < 1e-4 {
                continue;
            }
            if plinth > ya && plinth < yb {
                c.quad(w.rect(t0, t1, ya, plinth, 0.0), w.out(), base_c);
                c.quad(w.rect(t0, t1, plinth, yb, 0.0), w.out(), wall);
            } else {
                let colr = if yb <= plinth { base_c } else { wall };
                c.quad(w.rect(t0, t1, ya, yb, 0.0), w.out(), colr);
            }
        }
        // A ledge on top of the stone base, between the openings.
        if plinth > 0.0 {
            let mut t = s0;
            let mut spans = Vec::new();
            for &(op, t0, t1, ya, _) in &cut {
                if ya < plinth + 0.12 {
                    // The entrance portal stands proud of the wall beside the door.
                    let m = if op.kind == OpeningKind::Entrance {
                        0.45
                    } else {
                        0.0
                    };
                    if t0 - m > t {
                        spans.push((t, t0 - m));
                    }
                    t = t.max(t1 + m);
                }
            }
            if t < s1 {
                spans.push((t, s1));
            }
            for (a, b) in spans {
                wall_box(c, &w, (a, b), (plinth, plinth + 0.12), (0.0, 0.1), base_c);
            }
        }
        for &(op, t0, t1, ya, yb) in &holes {
            opening(c, &w, f, op, (t0, t1), (ya, yb));
        }
        // Pilasters between bays, floor by floor.
        if look.piers == Piers::Pilasters {
            let lines = bay_lines(s0, s1, look.bay);
            let half = pier_half(look, &lines);
            for &l in &lines[1..lines.len() - 1] {
                if cut
                    .iter()
                    .any(|h| l + half > h.1 - 0.02 && l - half < h.2 + 0.02)
                {
                    continue;
                }
                wall_box(
                    c,
                    &w,
                    (l - half, l + half),
                    (y0 + 0.2, y1 - 0.2),
                    (0.0, 0.1),
                    trim,
                );
            }
        }
        // Floor bands (not where the tier starts on a terrace slab).
        if look.floor_bands && level > 0 && !first_of_tier {
            let ext = if matches!(side, Side::Front | Side::Back) {
                0.08
            } else {
                0.0
            };
            wall_box(
                c,
                &w,
                (s0 - ext, s1 + ext),
                (f.elevation - 0.14, f.elevation + 0.14),
                (0.0, 0.08),
                trim,
            );
        }
    }
    if look.balconies && level > 0 {
        balconies(c, level);
    }
    if look.fire_escape && level > 0 {
        fire_escape(c, level);
    }
}

/// Half width of piers between bays: as wide as the gap between windows allows.
fn pier_half(look: &Look, lines: &[f64]) -> f64 {
    let bay = if lines.len() > 1 {
        lines[1] - lines[0]
    } else {
        look.bay
    };
    let gap = (1.0 - look.window_share).max(0.05) * bay * 0.5;
    (gap * 0.8).min(0.32)
}

fn street_glass(plan: &BuildingPlan, f: &Floor, op: &Opening) -> bool {
    op.side == Side::Front
        && f.level == 0
        && op.kind == OpeningKind::Window
        && plan.floors[0]
            .rooms
            .get(op.room)
            .is_some_and(|r| matches!(r.kind, RoomKind::Shop | RoomKind::Lobby))
}

/// One window or door: reveals, glass or door leaf, frame, sill, and for
/// shops a sign and an awning; for the entrance a canopy.
fn opening(c: &mut Ctx, w: &Wall, f: &Floor, op: &Opening, t: (f64, f64), y: (f64, f64)) {
    let plan = c.plan;
    let look = c.look;
    let (t0, t1) = t;
    let (ya, yb) = y;
    let frame = col(look.frame);
    let trim = col(look.trim);
    let shop = street_glass(plan, f, op);
    if op.kind == OpeningKind::Window && look.window == WindowKind::Porthole && !shop {
        porthole(c, w, op, f);
        return;
    }
    let r = look.reveal;
    let reveal_c = shade(look.wall, 0.86);
    let a = w.along();
    // Reveals: jambs, head and sill.
    c.quad(
        [
            w.at(t0, ya, 0.0),
            w.at(t0, yb, 0.0),
            w.at(t0, yb, -r),
            w.at(t0, ya, -r),
        ],
        a,
        reveal_c,
    );
    c.quad(
        [
            w.at(t1, ya, 0.0),
            w.at(t1, yb, 0.0),
            w.at(t1, yb, -r),
            w.at(t1, ya, -r),
        ],
        -a,
        reveal_c,
    );
    c.quad(
        [
            w.at(t0, yb, 0.0),
            w.at(t1, yb, 0.0),
            w.at(t1, yb, -r),
            w.at(t0, yb, -r),
        ],
        -DVec3::Y,
        reveal_c,
    );
    c.quad(
        [
            w.at(t0, ya, 0.0),
            w.at(t1, ya, 0.0),
            w.at(t1, ya, -r),
            w.at(t0, ya, -r),
        ],
        DVec3::Y,
        reveal_c,
    );
    match op.kind {
        OpeningKind::Door => {
            c.quad(w.rect(t0, t1, ya, yb, -r), w.out(), shade(look.frame, 0.75));
            // Handle.
            let hx = t1 - 0.25;
            wall_box(
                c,
                w,
                (hx - 0.03, hx + 0.03),
                (ya + 1.0, ya + 1.15),
                (-r, -r + 0.06),
                col([200, 180, 120]),
            );
        }
        OpeningKind::Entrance | OpeningKind::Window => {
            c.surf(Surf::Glass);
            c.quad(w.rect(t0, t1, ya, yb, -r), w.out(), col(look.glass));
            c.surf(Surf::Plain);
            // Mullions and a transom.
            let width = t1 - t0;
            let pane = match look.window {
                WindowKind::Curtain => 1.2,
                _ if shop => 1.6,
                _ => 1.1,
            };
            let n = (width / pane).floor() as usize;
            let bar = 0.06;
            for k in 1..n.max(1) {
                let x = t0 + width * k as f64 / n as f64;
                wall_box(
                    c,
                    w,
                    (x - bar * 0.5, x + bar * 0.5),
                    (ya, yb),
                    (-r, -r + 0.05),
                    frame,
                );
            }
            let h = yb - ya;
            if h > 2.0 && look.window != WindowKind::Curtain {
                let yt = if shop || op.kind == OpeningKind::Entrance {
                    yb - 0.6
                } else {
                    ya + h * 0.7
                };
                wall_box(
                    c,
                    w,
                    (t0, t1),
                    (yt - bar * 0.5, yt + bar * 0.5),
                    (-r, -r + 0.05),
                    frame,
                );
            }
        }
    }
    // Sill.
    if op.kind == OpeningKind::Window && !shop && look.window != WindowKind::Curtain {
        wall_box(
            c,
            w,
            (t0 - 0.08, t1 + 0.08),
            (ya - 0.12, ya),
            (0.0, 0.1),
            trim,
        );
    }
    // Arched head: a fanlight with a stone arch around it.
    if op.kind == OpeningKind::Window && look.window == WindowKind::Arched && !shop {
        let rad = (t1 - t0) * 0.5;
        let cx = (t0 + t1) * 0.5;
        let top_room = f.elevation + f.height - yb - 0.3;
        if top_room > rad * 0.6 {
            let rr = rad.min(top_room);
            let seg = 10;
            let squash = rr / rad;
            let pt = |k: usize, rad: f64, d: f64| {
                let ang = PI * k as f64 / seg as f64;
                w.at(cx + rad * ang.cos(), yb + rad * squash * ang.sin(), d)
            };
            c.surf(Surf::Glass);
            let mut fan = vec![w.at(cx, yb, 0.03)];
            fan.extend((0..=seg).map(|k| pt(k, rad, 0.03)));
            c.polygon(&fan, w.out(), col(look.glass));
            c.surf(Surf::Plain);
            for k in 0..seg {
                let q = [
                    pt(k, rad, 0.06),
                    pt(k + 1, rad, 0.06),
                    pt(k + 1, rad + 0.22, 0.06),
                    pt(k, rad + 0.22, 0.06),
                ];
                c.quad(q, w.out(), trim);
            }
        }
    }
    // Shop front: a sign board above, an awning if the style has them.
    if shop {
        let room = &plan.floors[0].rooms[op.room];
        if room.kind == RoomKind::Shop {
            let top = (f.elevation + f.height - 0.25).min(yb + 1.1);
            if top - yb > 0.5 {
                c.surf(Surf::Glow);
                wall_box(
                    c,
                    w,
                    (t0, t1),
                    (yb + 0.2, top),
                    (0.0, 0.12),
                    sign_color(look, op),
                );
                c.surf(Surf::Plain);
            }
            if look.awnings {
                awning(c, w, (t0 - 0.1, t1 + 0.1), yb + 0.12, op);
            }
        }
    }
    if op.kind == OpeningKind::Entrance {
        entrance(c, w, f, (t0, t1), yb);
    }
}

fn sign_color(look: &Look, op: &Opening) -> Color {
    let k = (op.along * 7.3).abs() as usize % 3;
    match k {
        0 => col(look.accent),
        1 => shade(look.accent, 0.6),
        _ => col([250, 236, 200]),
    }
}

/// A striped awning sloping out from the wall over a shop window.
fn awning(c: &mut Ctx, w: &Wall, t: (f64, f64), y: f64, op: &Opening) {
    let look = c.look;
    let depth = 1.5;
    let drop = 0.7;
    let stripes = (((t.1 - t.0) / 0.6).round() as usize).max(2);
    let a = col(look.accent);
    let white = col([246, 242, 232]);
    let flip = (op.along as i64) % 2 == 0;
    for k in 0..stripes {
        let s0 = t.0 + (t.1 - t.0) * k as f64 / stripes as f64;
        let s1 = t.0 + (t.1 - t.0) * (k + 1) as f64 / stripes as f64;
        let color = if (k % 2 == 0) ^ flip { a } else { white };
        let bottom = [
            w.at(s0, y, 0.0),
            w.at(s1, y, 0.0),
            w.at(s1, y - drop, depth),
            w.at(s0, y - drop, depth),
        ];
        let top = bottom.map(|p| p + DVec3::Y * 0.06);
        c.hexa(bottom, top, color);
        // Valance.
        wall_box(
            c,
            w,
            (s0, s1),
            (y - drop - 0.32, y - drop),
            (depth - 0.03, depth),
            color,
        );
    }
}

/// The main entrance: a stone portal and, for most styles, a canopy.
fn entrance(c: &mut Ctx, w: &Wall, f: &Floor, t: (f64, f64), head: f64) {
    let look = c.look;
    let trim = col(look.trim);
    let (t0, t1) = t;
    let top = (head + 0.5).min(f.elevation + f.height - 0.3);
    // Portal frame, as wide as the neighbouring windows leave room for.
    let side = w.side;
    let (mut left, mut right) = (0.45_f64, 0.45_f64);
    for o in c
        .plan
        .openings
        .iter()
        .filter(|o| o.side == side && o.floor == f.level)
    {
        let (a, b) = (o.along - o.width * 0.5, o.along + o.width * 0.5);
        if b <= t0 + 1e-6 {
            left = left.min(t0 - b - 0.06);
        } else if a >= t1 - 1e-6 {
            right = right.min(a - t1 - 0.06);
        }
    }
    if left > 0.05 {
        wall_box(c, w, (t0 - left, t0), (f.elevation, top), (0.0, 0.22), trim);
    }
    if right > 0.05 {
        wall_box(
            c,
            w,
            (t1, t1 + right),
            (f.elevation, top),
            (0.0, 0.22),
            trim,
        );
    }
    wall_box(c, w, (t0, t1), (head, top), (0.0, 0.22), trim);
    if look.canopy {
        let color = match look.style {
            Style::Streamline | Style::Pavilion | Style::Googie => col(look.accent),
            _ => shade(look.trim, 0.8),
        };
        let y = top;
        wall_box(
            c,
            w,
            (t0 - 0.45, t1 + 0.45),
            (y, y + 0.28),
            (0.0, 2.6),
            color,
        );
        c.surf(Surf::Glow);
        wall_box(
            c,
            w,
            (t0 - left.max(0.0), t1 + right.max(0.0)),
            (y + 0.28, y + 0.46),
            (2.45, 2.6),
            col([255, 240, 200]),
        );
        c.surf(Surf::Plain);
    }
}

/// A round window on the wall: glass disc and a frame ring.
fn porthole(c: &mut Ctx, w: &Wall, op: &Opening, f: &Floor) {
    let look = c.look;
    let r = op.width * 0.5;
    let cy = f.elevation + op.sill + op.height * 0.5;
    let seg = 14;
    let pt = |k: usize, rad: f64, d: f64| {
        let a = std::f64::consts::TAU * k as f64 / seg as f64;
        w.at(op.along + rad * a.cos(), cy + rad * a.sin(), d)
    };
    c.surf(Surf::Glass);
    let disc: Vec<DVec3> = (0..seg).map(|k| pt(k, r, 0.03)).collect();
    c.polygon(&disc, w.out(), col(look.glass));
    c.surf(Surf::Metal);
    for k in 0..seg {
        let q = [
            pt(k, r, 0.1),
            pt(k + 1, r, 0.1),
            pt(k + 1, r + 0.18, 0.1),
            pt(k, r + 0.18, 0.1),
        ];
        c.quad(q, w.out(), col(look.frame));
        let s = [
            pt(k, r, 0.03),
            pt(k + 1, r, 0.03),
            pt(k + 1, r, 0.1),
            pt(k, r, 0.1),
        ];
        let mid = (s[0] + s[1]) * 0.5;
        let centre = w.at(op.along, cy, 0.03);
        c.quad(s, centre - mid, col(look.frame));
    }
    c.surf(Surf::Plain);
}

/// Deep piers at every bay line, the full height of a tier.
fn full_piers(c: &mut Ctx, i: usize, j: usize) {
    let plan = c.plan;
    let look = c.look;
    let o = plan.floors[i].outline;
    let y0 = wall_bottom(plan, i)
        + if i == 0 {
            look.plinth.min(plan.floors[0].height * 0.45) + 0.12
        } else {
            0.0
        };
    let y1 = plan.floors[j].elevation + plan.floors[j].height - 0.02;
    let trim = col(look.trim);
    for side in SIDES {
        let w = Wall { side, o };
        let (s0, s1) = w.range();
        let lines = bay_lines(s0, s1, look.bay);
        let half = pier_half(look, &lines);
        for (k, &l) in lines.iter().enumerate() {
            // Corners: only the front and back walls carry the corner pier.
            let end = k == 0 || k + 1 == lines.len();
            if end && matches!(side, Side::Left | Side::Right) {
                continue;
            }
            let (a, b) = if k == 0 {
                (l - 0.3, l + half)
            } else if k + 1 == lines.len() {
                (l - half, l + 0.3)
            } else {
                (l - half, l + half)
            };
            // Skip piers that would cover a door or shop window.
            let blocked = plan.openings.iter().any(|op| {
                op.side == side
                    && (op.floor as usize) >= i
                    && (op.floor as usize) <= j
                    && op.kind != OpeningKind::Window
                    && b > op.along - op.width * 0.5 - 0.5
                    && a < op.along + op.width * 0.5 + 0.5
            });
            let ya = if blocked {
                plan.floors[1.min(j)].elevation.max(y0)
            } else {
                y0
            };
            if y1 - ya < 1.0 {
                continue;
            }
            wall_box(c, &w, (a, b), (ya, y1), (0.0, 0.3), trim);
        }
    }
}

/// The cornice and roof slab on a tier at height `y`; returns the deck height.
fn cornice(c: &mut Ctx, o: Rect, y: f64, detail: Detail) -> f64 {
    let look = c.look;
    let trim = col(look.trim);
    let slab = |c: &mut Ctx, grow: f64, y0: f64, y1: f64, color: Color| {
        c.bx(
            DVec3::new(o.min.x - grow, y0, o.min.y - grow),
            DVec3::new(o.max.x + grow, y1, o.max.y + grow),
            color,
        );
    };
    match (look.cornice, detail) {
        // Overhangs stay under half a metre: neighbours can stand 1 m apart.
        (Cornice::Stepped, Detail::Near) => {
            slab(c, 0.15, y, y + 0.25, trim);
            slab(c, 0.3, y + 0.25, y + 0.45, trim);
            slab(c, 0.45, y + 0.45, y + 0.6, trim);
            y + 0.6
        }
        (Cornice::None, _) => {
            slab(c, 0.05, y, y + ROOF_SLAB, col(look.wall));
            y + ROOF_SLAB
        }
        _ => {
            slab(c, 0.3, y, y + ROOF_SLAB, trim);
            y + ROOF_SLAB
        }
    }
}

/// A wall of height `h` and thickness `th` along the edge of `o` on a deck.
fn parapet(c: &mut Ctx, o: Rect, y: f64, h: f64, th: f64) {
    let color = col(c.look.wall);
    let cap = col(c.look.trim);
    let e = 0.05;
    let (x0, x1, z0, z1) = (o.min.x - e, o.max.x + e, o.min.y - e, o.max.y + e);
    let walls = [
        (DVec3::new(x0, y, z0), DVec3::new(x1, y + h, z0 + th)),
        (DVec3::new(x0, y, z1 - th), DVec3::new(x1, y + h, z1)),
        (
            DVec3::new(x0, y, z0 + th),
            DVec3::new(x0 + th, y + h, z1 - th),
        ),
        (
            DVec3::new(x1 - th, y, z0 + th),
            DVec3::new(x1, y + h, z1 - th),
        ),
    ];
    for (a, b) in walls {
        c.stand(a, b, color);
    }
    // Coping along the top, a little wider.
    let caps = [
        (
            DVec3::new(x0 - 0.06, y + h, z0 - 0.06),
            DVec3::new(x1 + 0.06, y + h + 0.1, z0 + th + 0.06),
        ),
        (
            DVec3::new(x0 - 0.06, y + h, z1 - th - 0.06),
            DVec3::new(x1 + 0.06, y + h + 0.1, z1 + 0.06),
        ),
        (
            DVec3::new(x0 - 0.06, y + h, z0 + th + 0.06),
            DVec3::new(x0 + th + 0.06, y + h + 0.1, z1 - th - 0.06),
        ),
        (
            DVec3::new(x1 - th - 0.06, y + h, z0 + th + 0.06),
            DVec3::new(x1 + 0.06, y + h + 0.1, z1 - th - 0.06),
        ),
    ];
    for (a, b) in caps {
        c.bx(a, b, cap);
    }
}

fn rng_of(plan: &BuildingPlan, what: &str) -> Rng {
    plan.site.seed.child(what).rng()
}

/// The roof on the top tier, standing on the deck at `y`.
fn roof(c: &mut Ctx, o: Rect, y: f64, detail: Detail) {
    let look = c.look;
    let plan = c.plan;
    let roof_c = col(look.roof_color);
    match look.roof {
        Roof::Flat => {
            if look.parapet > 0.0 {
                parapet(c, o, y, look.parapet, 0.3);
            }
            let pad = look.taxi_pad && o.size().min_element() >= 12.0;
            if pad {
                taxi_pad(c, o, y, detail);
            } else if detail == Detail::Near && look.rooftop_gear {
                rooftop(c, o, y);
            }
            if detail == Detail::Near && look.chimneys {
                flat_chimneys(c, o, y);
            }
            crown(c, o, y);
        }
        Roof::Gable | Roof::Hip => {
            let hip = look.roof == Roof::Hip;
            pitched(c, o, y, hip, roof_c, detail);
            c.b.tex = Some(wall_layer(look));
        }
        Roof::Mansard => {
            let inset = 1.4;
            let h = 2.6;
            let lo = [
                DVec3::new(o.min.x - 0.2, y, o.min.y - 0.2),
                DVec3::new(o.max.x + 0.2, y, o.min.y - 0.2),
                DVec3::new(o.max.x + 0.2, y, o.max.y + 0.2),
                DVec3::new(o.min.x - 0.2, y, o.max.y + 0.2),
            ];
            let hi = [
                DVec3::new(o.min.x + inset, y + h, o.min.y + inset),
                DVec3::new(o.max.x - inset, y + h, o.min.y + inset),
                DVec3::new(o.max.x - inset, y + h, o.max.y - inset),
                DVec3::new(o.min.x + inset, y + h, o.max.y - inset),
            ];
            c.b.tex = Some(roof_layer(look));
            c.hexa(lo, hi, roof_c);
            c.b.tex = Some(wall_layer(look));
            if detail == Detail::Near {
                if look.dormers {
                    mansard_dormers(c, o, y);
                }
                if look.chimneys {
                    flat_chimneys(c, o, y + h);
                }
            }
        }
        Roof::Dome => {
            let r = o.size().min_element() * 0.42;
            let cc = o.center();
            let drum = col(look.trim);
            c.cylinder(DVec3::new(cc.x, y, cc.y), (r, r), 1.6, 24, false, drum);
            c.surf(Surf::Metal);
            c.dome(DVec3::new(cc.x, y + 1.6, cc.y), r, roof_dome_color(look));
            c.surf(Surf::Plain);
            // Flat roof around the drum.
            parapet(c, o, y, 0.8, 0.3);
        }
        Roof::Swoop => swoop(c, o, y),
        Roof::Sawtooth => sawtooth(c, o, y),
    }
    let _ = plan;
}

fn roof_dome_color(look: &Look) -> Color {
    match look.style {
        Style::Civic => col([118, 168, 150]),
        Style::Lab => col([150, 160, 156]),
        _ => col(look.accent),
    }
}

/// Gable or hip roof over `o`: pitch, eaves and ridge from the look, with
/// chimneys and dormers at close range.
fn pitched(c: &mut Ctx, o: Rect, y: f64, hip: bool, color: Color, detail: Detail) {
    let look = c.look;
    let ov = look.eaves;
    let along_x = match look.ridge {
        Ridge::Long => o.size().x >= o.size().y,
        Ridge::Front => true,
        Ridge::Deep => false,
    };
    // A local frame: s along the ridge, t across it.
    let (ws0, ws1, wt0, wt1) = if along_x {
        (o.min.x, o.max.x, o.min.y, o.max.y)
    } else {
        (o.min.y, o.max.y, o.min.x, o.max.x)
    };
    let p = move |s: f64, t: f64, yy: f64| {
        if along_x {
            DVec3::new(s, yy, t)
        } else {
            DVec3::new(t, yy, s)
        }
    };
    let n = move |ds: f64, dt: f64, dy: f64| p(ds, dt, dy);
    let (s0, s1, t0, t1) = (ws0 - ov, ws1 + ov, wt0 - ov, wt1 + ov);
    let half = (t1 - t0) * 0.5;
    let tm = (t0 + t1) * 0.5;
    let h = (half * look.pitch).clamp(1.0, 14.0);
    let inset = if hip {
        half.min((s1 - s0) * 0.5 - 0.05)
    } else {
        0.0
    };
    let r0 = p(s0 + inset, tm, y + h);
    let r1 = p(s1 - inset, tm, y + h);
    let (a, b, cc, d) = (p(s0, t0, y), p(s1, t0, y), p(s1, t1, y), p(s0, t1, y));
    c.b.tex = Some(roof_layer(look));
    c.quad([a, b, r1, r0], n(0.0, -h, half), color);
    c.quad([d, cc, r1, r0], n(0.0, h, half), color);
    if hip {
        c.polygon(&[a, d, r0], n(-h, 0.0, half), color);
        c.polygon(&[b, cc, r1], n(h, 0.0, half), color);
        c.b.tex = Some(wall_layer(look));
    } else {
        c.b.tex = Some(wall_layer(look));
        // Gable ends in the wall's colour, on the wall line, under the
        // roof's ends.
        let e = y + h * ov / half;
        let wall = col(look.wall);
        for (sw, dir) in [(ws0, -1.0), (ws1, 1.0)] {
            let g = [
                p(sw, wt0, y),
                p(sw, wt1, y),
                p(sw, wt1, e),
                p(sw, tm, y + h - 0.02),
                p(sw, wt0, e),
            ];
            c.polygon(&g, n(dir, 0.0, 0.0), wall);
        }
    }
    c.quad([a, b, cc, d], -DVec3::Y, shade(look.roof_color, 0.7));
    if detail != Detail::Near {
        return;
    }
    // Height of the roof over a point at `t` across.
    let roof_at = |t: f64| y + h * (1.0 - (t - tm).abs() / half);
    if look.chimneys {
        let mut rng = rng_of(c.plan, "chimneys");
        let len = ws1 - ws0;
        let count = if len > 14.0 { 2 } else { 1 };
        let brick = shade(look.wall, 0.75);
        for k in 0..count {
            let s = ws0 + len * (0.2 + 0.6 * k as f64) + rng.range(-0.8, 0.8);
            let t = tm + half * rng.range(-0.45, 0.45);
            let top = (roof_at(t) + 1.2).max(y + h + 0.5);
            c.bx(p(s - 0.35, t - 0.45, y), p(s + 0.35, t + 0.45, top), brick);
            c.bx(
                p(s - 0.45, t - 0.55, top),
                p(s + 0.45, t + 0.55, top + 0.15),
                shade(look.wall, 0.6),
            );
        }
    }
    if look.dormers && h >= 2.6 {
        let len = ws1 - ws0;
        let n_d = ((len - 3.0) / 4.0).floor().max(0.0) as usize;
        let wall = col(look.trim);
        let glass = col(look.glass);
        let roof_c = col(look.roof_color);
        for side in [-1.0, 1.0] {
            // Front of the dormer: a little in from the wall line.
            let tf = if side < 0.0 { wt0 + 0.5 } else { wt1 - 0.5 };
            let yf = roof_at(tf);
            let top = yf + 1.5;
            if top > y + h - 0.3 {
                continue;
            }
            let tb = tm;
            for k in 0..n_d {
                let s = ws0 + 2.0 + (len - 4.0) * (k as f64 + 0.5) / n_d as f64;
                c.bx(p(s - 0.7, tf, yf - 0.4), p(s + 0.7, tb, top), wall);
                c.bx(
                    p(s - 0.85, tf - side * 0.15, top),
                    p(s + 0.85, tb, top + 0.12),
                    roof_c,
                );
                c.surf(Surf::Glass);
                let tg = tf + side * 0.03;
                c.quad(
                    [
                        p(s - 0.45, tg, yf + 0.15),
                        p(s + 0.45, tg, yf + 0.15),
                        p(s + 0.45, tg, top - 0.2),
                        p(s - 0.45, tg, top - 0.2),
                    ],
                    n(0.0, side, 0.0),
                    glass,
                );
                c.surf(Surf::Plain);
            }
        }
    }
}

/// Dormers on all four sides of a mansard of height `h` standing on `o`.
fn mansard_dormers(c: &mut Ctx, o: Rect, y: f64) {
    let look = c.look;
    let wall = col(look.trim);
    let glass = col(look.glass);
    let roof_c = col(look.roof_color);
    let top = y + 1.9;
    // (wall line start, end, across position, outward sign, along x?)
    let sides = [
        (o.min.x, o.max.x, o.min.y, -1.0, true),
        (o.min.x, o.max.x, o.max.y, 1.0, true),
        (o.min.y, o.max.y, o.min.x, -1.0, false),
        (o.min.y, o.max.y, o.max.x, 1.0, false),
    ];
    for (a, b, t, out, along_x) in sides {
        let p = move |s: f64, tt: f64, yy: f64| {
            if along_x {
                DVec3::new(s, yy, tt)
            } else {
                DVec3::new(tt, yy, s)
            }
        };
        let len = b - a;
        let count = ((len - 3.0) / look.bay.max(2.0)).floor().max(0.0) as usize;
        let tf = t + out * 0.3;
        let tb = t - out * 2.2;
        for k in 0..count {
            let s = a + 2.0 + (len - 4.0) * (k as f64 + 0.5) / count as f64;
            c.bx(p(s - 0.6, tf, y + 0.2), p(s + 0.6, tb, top), wall);
            c.bx(
                p(s - 0.75, tf + out * 0.12, top),
                p(s + 0.75, tb, top + 0.12),
                roof_c,
            );
            c.surf(Surf::Glass);
            let tg = tf + out * 0.03;
            let q = [
                p(s - 0.4, tg, y + 0.5),
                p(s + 0.4, tg, y + 0.5),
                p(s + 0.4, tg, top - 0.2),
                p(s - 0.4, tg, top - 0.2),
            ];
            let nrm = if along_x {
                DVec3::new(0.0, 0.0, out)
            } else {
                DVec3::new(out, 0.0, 0.0)
            };
            c.quad(q, nrm, glass);
            c.surf(Surf::Plain);
        }
    }
}

/// Chimneys on a flat or mansard roof, on the party walls.
fn flat_chimneys(c: &mut Ctx, o: Rect, y: f64) {
    let brick = shade(c.look.wall, 0.75);
    let zm = o.center().y;
    for x in [o.min.x + 0.6, o.max.x - 0.6] {
        c.bx(
            DVec3::new(x - 0.4, y, zm - 1.2),
            DVec3::new(x + 0.4, y + 1.5, zm + 1.2),
            brick,
        );
    }
}

/// A Googie roof: a slab that sweeps up towards the street, on walls that
/// follow it up.
fn swoop(c: &mut Ctx, o: Rect, y: f64) {
    let look = c.look;
    let rise_front = 2.6;
    let rise_back = 0.7;
    let h = |z: f64| y + rise_back + (rise_front - rise_back) * (o.max.y - z) / o.size().y.max(1.0);
    let (x0, x1) = (o.min.x - 0.9, o.max.x + 0.9);
    let (z0, z1) = (o.min.y - 2.2, o.max.y + 0.7);
    let bottom = [
        DVec3::new(x0, h(z0), z0),
        DVec3::new(x1, h(z0), z0),
        DVec3::new(x1, h(z1), z1),
        DVec3::new(x0, h(z1), z1),
    ];
    let top = bottom.map(|p| p + DVec3::Y * 0.35);
    c.hexa(bottom, top, col(look.roof_color));
    // Walls up to the slab.
    let lo = [
        DVec3::new(o.min.x, y, o.min.y),
        DVec3::new(o.max.x, y, o.min.y),
        DVec3::new(o.max.x, y, o.max.y),
        DVec3::new(o.min.x, y, o.max.y),
    ];
    let hi = lo.map(|p| DVec3::new(p.x, h(p.z) - 0.001, p.z));
    c.surf(Surf::Glass);
    c.hexa(lo, hi, col(look.glass));
    c.surf(Surf::Plain);
    // Slim steel posts under the front overhang.
    for x in [x0 + 0.4, x1 - 0.4] {
        let z = z0 + 0.4;
        c.stand(
            DVec3::new(x - 0.08, 0.0, z - 0.08),
            DVec3::new(x + 0.08, h(z) - 0.001, z + 0.08),
            col(look.trim),
        );
    }
}

/// Factory sawtooth: sloped roofs with north-light glazing.
fn sawtooth(c: &mut Ctx, o: Rect, y: f64) {
    let look = c.look;
    let n = ((o.size().x / 6.0).round() as usize).max(1);
    let h = 2.4;
    let roof_c = col(look.roof_color);
    let wall = col(look.wall);
    for k in 0..n {
        let a = o.min.x + o.size().x * k as f64 / n as f64;
        let b = o.min.x + o.size().x * (k + 1) as f64 / n as f64;
        let (z0, z1) = (o.min.y, o.max.y);
        c.quad(
            [
                DVec3::new(a, y, z0),
                DVec3::new(b, y + h, z0),
                DVec3::new(b, y + h, z1),
                DVec3::new(a, y, z1),
            ],
            DVec3::new(-h, b - a, 0.0),
            roof_c,
        );
        c.surf(Surf::Glass);
        c.quad(
            [
                DVec3::new(b, y, z0),
                DVec3::new(b, y + h, z0),
                DVec3::new(b, y + h, z1),
                DVec3::new(b, y, z1),
            ],
            DVec3::X,
            col(look.glass),
        );
        c.surf(Surf::Plain);
        c.polygon(
            &[
                DVec3::new(a, y, z0),
                DVec3::new(b, y, z0),
                DVec3::new(b, y + h, z0),
            ],
            -DVec3::Z,
            wall,
        );
        c.polygon(
            &[
                DVec3::new(a, y, z1),
                DVec3::new(b, y, z1),
                DVec3::new(b, y + h, z1),
            ],
            DVec3::Z,
            wall,
        );
    }
    // A chimney stack for the works.
    let mut rng = rng_of(c.plan, "chimney");
    if rng.chance(0.6) {
        let x = o.min.x + o.size().x * rng.range(0.15, 0.85);
        let z = o.min.y + o.size().y * rng.range(0.6, 0.85);
        c.cylinder(
            DVec3::new(x, y, z),
            (1.3, 0.9),
            16.0,
            12,
            true,
            shade(look.wall, 0.8),
        );
        c.cylinder(
            DVec3::new(x, y + 16.0, z),
            (1.05, 1.05),
            0.6,
            12,
            true,
            col(look.trim),
        );
    }
}

/// A round landing pad for flying taxis on stilts over the roof, its rim
/// glowing, a beacon at one side: seen from far across the town.
fn taxi_pad(c: &mut Ctx, o: Rect, y: f64, detail: Detail) {
    let look = c.look;
    let cc = o.center();
    let r = (o.size().min_element() * 0.5 - 1.2).clamp(5.0, 9.0);
    let top = y + 1.6;
    let seg = if detail == Detail::Near { 32 } else { 16 };
    let deck = col([228, 226, 218]);
    let legs = col([110, 114, 118]);
    c.surf(Surf::Metal);
    for (dx, dz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let p = DVec3::new(cc.x + dx * r * 0.5, y, cc.y + dz * r * 0.5);
        c.cylinder(p, (0.25, 0.25), top - 0.35 - y, 8, false, legs);
    }
    c.surf(Surf::Plain);
    c.cylinder(
        DVec3::new(cc.x, top - 0.35, cc.y),
        (r, r),
        0.35,
        seg,
        true,
        deck,
    );
    // The glowing rim: a thin disc a little wider, just under the deck top.
    c.surf(Surf::Glow);
    c.cylinder(
        DVec3::new(cc.x, top - 0.25, cc.y),
        (r + 0.18, r + 0.18),
        0.12,
        seg,
        true,
        col([120, 230, 255]),
    );
    c.surf(Surf::Plain);
    // A painted "T" for taxi, in the accent colour.
    let paint = col(look.accent);
    let t = top + 0.02;
    c.bx(
        DVec3::new(cc.x - r * 0.45, t - 0.01, cc.y - r * 0.4),
        DVec3::new(cc.x + r * 0.45, t, cc.y - r * 0.22),
        paint,
    );
    c.bx(
        DVec3::new(cc.x - r * 0.09, t - 0.01, cc.y - r * 0.22),
        DVec3::new(cc.x + r * 0.09, t, cc.y + r * 0.45),
        paint,
    );
    // A beacon on a mast at the back edge.
    let m = DVec3::new(cc.x + r * 0.75, top, cc.y + r * 0.55);
    c.surf(Surf::Metal);
    c.cylinder(m, (0.08, 0.05), 3.2, 8, false, legs);
    c.surf(Surf::Glow);
    c.dome(m + DVec3::Y * 3.2, 0.35, col([255, 90, 70]));
    c.surf(Surf::Plain);
}

/// Stair house over the core, vents, boxes and a water tank.
fn rooftop(c: &mut Ctx, o: Rect, y: f64) {
    let plan = c.plan;
    let look = c.look;
    let mut rng = rng_of(plan, "rooftop");
    let gear = col([150, 150, 146]);
    let mut taken: Vec<Rect> = Vec::new();
    let crowned = look.crown != Crown::None;
    if let Some(core) = plan.core.filter(|_| !crowned) {
        if o.contains_rect(&core, 1e-6) {
            let lo = DVec3::new(core.min.x + 0.5, y, core.min.y + 0.5);
            let hi = DVec3::new(core.max.x - 0.5, y + 3.4, core.max.y - 0.5);
            c.stand(lo, hi, col(look.wall));
            c.bx(
                lo - DVec3::new(0.15, 0.0, 0.15) + DVec3::Y * 3.4,
                hi + DVec3::new(0.15, 0.25, 0.15),
                col(look.trim),
            );
            taken.push(core);
        }
    }
    if crowned {
        // The crown takes the middle (and stands over the core).
        taken.push(o.inset(o.size().min_element() * 0.12));
    }
    let free = |r: &Rect, taken: &[Rect]| {
        o.inset(1.0).contains_rect(r, 1e-6) && taken.iter().all(|t| !overlaps(t, r))
    };
    // Air handlers.
    for _ in 0..rng.range_int(2, 6) {
        let s = DVec2::new(rng.range(1.5, 3.5), rng.range(1.2, 2.4));
        let p = DVec2::new(rng.range(o.min.x, o.max.x), rng.range(o.min.y, o.max.y));
        let r = Rect::from_xz(p.x, p.y, p.x + s.x, p.y + s.y);
        if !free(&r, &taken) {
            continue;
        }
        let hgt = rng.range(1.0, 1.8);
        c.stand(
            DVec3::new(r.min.x, y, r.min.y),
            DVec3::new(r.max.x, y + hgt, r.max.y),
            gear,
        );
        c.surf(Surf::Metal);
        let cc = r.center();
        c.cylinder(
            DVec3::new(cc.x, y + hgt, cc.y),
            (0.45, 0.45),
            0.25,
            10,
            true,
            col([110, 112, 112]),
        );
        c.surf(Surf::Plain);
        taken.push(r);
    }
    if look.water_tank {
        let r = 1.7;
        for _ in 0..6 {
            let p = DVec2::new(
                rng.range(o.min.x + 2.5, o.max.x - 2.5),
                rng.range(o.min.y + 2.5, o.max.y - 2.5),
            );
            let area = Rect::from_xz(p.x - r - 0.3, p.y - r - 0.3, p.x + r + 0.3, p.y + r + 0.3);
            if !free(&area, &taken) {
                continue;
            }
            let wood = col([122, 92, 64]);
            for (dx, dz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let q = DVec3::new(p.x + dx * r * 0.7, y, p.y + dz * r * 0.7);
                c.stand(
                    q - DVec3::new(0.1, 0.0, 0.1),
                    q + DVec3::new(0.1, 2.2, 0.1),
                    col([60, 58, 56]),
                );
            }
            c.cylinder(DVec3::new(p.x, y + 2.2, p.y), (r, r), 3.4, 16, true, wood);
            c.cylinder(
                DVec3::new(p.x, y + 5.6, p.y),
                (r + 0.15, 0.0),
                1.0,
                16,
                false,
                shade([122, 92, 64], 0.7),
            );
            taken.push(area);
            break;
        }
    }
}

fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.min.x < b.max.x && b.min.x < a.max.x && a.min.y < b.max.y && b.min.y < a.max.y
}

/// The crown on top of a tower.
fn crown(c: &mut Ctx, o: Rect, y: f64) {
    let look = c.look;
    let wall = col(look.wall);
    let trim = col(look.trim);
    let w = o.size().min_element();
    let cc = o.center();
    match look.crown {
        Crown::None => {}
        Crown::Stepped | Crown::Spire | Crown::Fins => {
            let mut r = o;
            let mut yy = y;
            let steps = 3;
            for k in 0..steps {
                r = r.inset(w * 0.11);
                if r.size().min_element() < 2.0 {
                    break;
                }
                let h = 4.2 - k as f64 * 0.7;
                c.stand(
                    DVec3::new(r.min.x, yy, r.min.y),
                    DVec3::new(r.max.x, yy + h, r.max.y),
                    wall,
                );
                // Vertical stripes of glass on each step face.
                c.surf(Surf::Glass);
                for side in SIDES {
                    let wl = Wall { side, o: r };
                    let (s0, s1) = wl.range();
                    let n = ((s1 - s0) / 2.2).floor() as usize;
                    for i in 0..n {
                        let t = s0 + (s1 - s0) * (i as f64 + 0.5) / n as f64;
                        c.quad(
                            wl.rect(t - 0.35, t + 0.35, yy + 0.6, yy + h - 0.6, 0.03),
                            wl.out(),
                            col(look.glass),
                        );
                    }
                }
                c.surf(Surf::Plain);
                c.bx(
                    DVec3::new(r.min.x - 0.2, yy + h, r.min.y - 0.2),
                    DVec3::new(r.max.x + 0.2, yy + h + 0.3, r.max.y + 0.2),
                    trim,
                );
                yy += h + 0.3;
            }
            match look.crown {
                Crown::Spire => {
                    let base = r.size().min_element() * 0.5;
                    let h = (base * 5.0).clamp(10.0, 32.0);
                    c.surf(Surf::Metal);
                    c.cylinder(
                        DVec3::new(cc.x, yy, cc.y),
                        (base * 0.8, 0.0),
                        h,
                        8,
                        true,
                        trim,
                    );
                    c.surf(Surf::Glow);
                    c.dome(
                        DVec3::new(cc.x, yy + h - 0.3, cc.y),
                        0.5,
                        col([255, 120, 90]),
                    );
                    c.surf(Surf::Plain);
                }
                Crown::Fins => {
                    let base = r.size().min_element() * 0.35;
                    c.surf(Surf::Glass);
                    c.cylinder(
                        DVec3::new(cc.x, yy, cc.y),
                        (base * 0.6, base * 0.6),
                        6.0,
                        12,
                        true,
                        col(look.glass),
                    );
                    c.surf(Surf::Plain);
                    for k in 0..4 {
                        let a = PI * 0.5 * k as f64 + PI * 0.25;
                        let d = DVec2::new(a.cos(), a.sin());
                        let inner = cc + d * base * 0.6;
                        let outer = cc + d * base * 1.3;
                        let n = DVec2::new(-d.y, d.x) * 0.2;
                        let bottom = [
                            DVec3::new(inner.x - n.x, yy, inner.y - n.y),
                            DVec3::new(outer.x - n.x, yy, outer.y - n.y),
                            DVec3::new(outer.x + n.x, yy, outer.y + n.y),
                            DVec3::new(inner.x + n.x, yy, inner.y + n.y),
                        ];
                        let top = [
                            bottom[0] + DVec3::Y * 12.0,
                            DVec3::new(inner.x - n.x, yy + 9.0, inner.y - n.y)
                                + DVec3::new(d.x, 0.0, d.y) * 0.3,
                            DVec3::new(inner.x + n.x, yy + 9.0, inner.y + n.y)
                                + DVec3::new(d.x, 0.0, d.y) * 0.3,
                            bottom[3] + DVec3::Y * 12.0,
                        ];
                        c.hexa(bottom, top, trim);
                    }
                }
                _ => {}
            }
        }
        Crown::Mast => {
            let r = o.inset(w * 0.2);
            c.stand(
                DVec3::new(r.min.x, y, r.min.y),
                DVec3::new(r.max.x, y + 3.0, r.max.y),
                wall,
            );
            let yy = y + 3.0;
            c.surf(Surf::Metal);
            c.cylinder(
                DVec3::new(cc.x, yy, cc.y),
                (0.5, 0.15),
                26.0,
                8,
                true,
                col([200, 200, 196]),
            );
            for k in 0..3 {
                let yr = yy + 6.0 + k as f64 * 6.0;
                c.cylinder(
                    DVec3::new(cc.x, yr, cc.y),
                    (1.6 - k as f64 * 0.4, 1.6 - k as f64 * 0.4),
                    0.25,
                    16,
                    true,
                    trim,
                );
            }
            c.surf(Surf::Glow);
            c.dome(DVec3::new(cc.x, yy + 26.0, cc.y), 0.6, col([255, 80, 70]));
            c.surf(Surf::Plain);
        }
        Crown::Dome => {
            let r = w * 0.32;
            c.cylinder(DVec3::new(cc.x, y, cc.y), (r, r), 3.0, 24, false, wall);
            c.bx(
                DVec3::new(cc.x - r - 0.3, y + 3.0, cc.y - r - 0.3),
                DVec3::new(cc.x + r + 0.3, y + 3.3, cc.y + r + 0.3),
                trim,
            );
            c.surf(Surf::Metal);
            c.dome(DVec3::new(cc.x, y + 3.3, cc.y), r, roof_dome_color(look));
            c.surf(Surf::Plain);
            let top = y + 3.3 + r;
            if look.style == Style::Lab {
                // The coil.
                c.surf(Surf::Metal);
                c.cylinder(
                    DVec3::new(cc.x, top - 0.2, cc.y),
                    (0.3, 0.3),
                    5.0,
                    8,
                    true,
                    col([180, 120, 70]),
                );
                for k in 0..4 {
                    let rr = 1.2 - k as f64 * 0.2;
                    c.cylinder(
                        DVec3::new(cc.x, top + 1.0 + k as f64 * 1.0, cc.y),
                        (rr, rr),
                        0.2,
                        14,
                        true,
                        col([190, 130, 80]),
                    );
                }
                c.surf(Surf::Glow);
                c.dome(DVec3::new(cc.x, top + 4.8, cc.y), 0.9, col(look.accent));
                c.surf(Surf::Plain);
            } else {
                c.cylinder(
                    DVec3::new(cc.x, top - 0.2, cc.y),
                    (0.9, 0.9),
                    1.6,
                    10,
                    true,
                    trim,
                );
                c.surf(Surf::Metal);
                c.cylinder(
                    DVec3::new(cc.x, top + 1.4, cc.y),
                    (1.0, 0.0),
                    2.4,
                    10,
                    false,
                    trim,
                );
                c.surf(Surf::Plain);
            }
        }
    }
}

/// The streamline corner tower: a drum with glass bands at every floor.
fn tower(c: &mut Ctx, ts: &[(usize, usize)], shown: usize, roofed: bool, detail: Detail) {
    let plan = c.plan;
    let look = c.look;
    let Some((ctr, r)) = corner_tower(plan) else {
        return;
    };
    let (i, j) = ts[0];
    let f_top = &plan.floors[j];
    let full = shown == plan.floors.len() || ts.len() > 1;
    let top = f_top.elevation + f_top.height + if full && roofed { 2.6 } else { 0.0 };
    let seg = 24;
    let wall = col(look.wall);
    let trim = col(look.trim);
    c.cylinder(DVec3::new(ctr.x, 0.0, ctr.y), (r, r), top, seg, false, wall);
    for level in i..=j {
        let f = &plan.floors[level];
        let ya = f.elevation + 0.95;
        let h = (f.height - 2.2).clamp(1.4, 2.6);
        c.surf(Surf::Glass);
        c.cylinder(
            DVec3::new(ctr.x, ya, ctr.y),
            (r + 0.04, r + 0.04),
            h,
            seg,
            false,
            col(look.glass),
        );
        c.surf(Surf::Plain);
        if detail == Detail::Near && level > 0 {
            c.cylinder(
                DVec3::new(ctr.x, f.elevation - 0.15, ctr.y),
                (r + 0.1, r + 0.1),
                0.3,
                seg,
                false,
                trim,
            );
        }
    }
    if full && roofed {
        c.cylinder(
            DVec3::new(ctr.x, top, ctr.y),
            (r + 0.3, r + 0.3),
            0.3,
            seg,
            true,
            trim,
        );
        // A flagpole.
        c.surf(Surf::Metal);
        c.cylinder(
            DVec3::new(ctr.x, top + 0.3, ctr.y),
            (0.08, 0.05),
            6.0,
            6,
            true,
            col([220, 220, 216]),
        );
        c.surf(Surf::Plain);
    } else {
        // Cut open above: only an upward lid, so its underside cannot fight
        // the room floors that start at the same height.
        let lid: Vec<DVec3> = (0..seg)
            .map(|k| {
                let a = std::f64::consts::TAU * k as f64 / seg as f64;
                DVec3::new(ctr.x + r * a.cos(), top, ctr.y + r * a.sin())
            })
            .collect();
        c.polygon(&lid, DVec3::Y, wall);
    }
}

/// Tall pylon fins at the front-left corner (Googie, pavilions, streamline).
fn fins(c: &mut Ctx) {
    let plan = c.plan;
    let look = c.look;
    let o = plan.floors[0].outline;
    let h = plan.height() + 6.0;
    let count = if look.style == Style::Pavilion { 2 } else { 1 };
    for k in 0..count {
        let x = o.min.x + 1.2 + k as f64 * (o.size().x - 2.4);
        let bottom = [
            DVec3::new(x - 0.2, 0.0, o.min.y - 1.6),
            DVec3::new(x + 0.2, 0.0, o.min.y - 1.6),
            DVec3::new(x + 0.2, 0.0, o.min.y + 1.4),
            DVec3::new(x - 0.2, 0.0, o.min.y + 1.4),
        ];
        let top = [
            DVec3::new(x - 0.12, h, o.min.y - 3.2),
            DVec3::new(x + 0.12, h, o.min.y - 3.2),
            DVec3::new(x + 0.12, h, o.min.y - 1.6),
            DVec3::new(x - 0.12, h, o.min.y - 1.6),
        ];
        c.hexa(bottom, top, col(look.accent));
        c.surf(Surf::Glow);
        c.dome(DVec3::new(x, h, o.min.y - 2.4), 0.7, col([255, 230, 150]));
        c.surf(Surf::Plain);
    }
}

/// An upright neon sign on the front, near the left.
fn sign(c: &mut Ctx) {
    let plan = c.plan;
    let look = c.look;
    let o = plan.floors[0].outline;
    let lines = bay_lines(o.min.x, o.max.x, look.bay);
    let Some(&l) = lines.get(1) else {
        return;
    };
    let top_floor = plan.floors.len().min(5) - 1;
    let y0 = plan.floors[1].elevation + 0.5;
    let y1 = plan.floors[top_floor].elevation + plan.floors[top_floor].height - 0.5;
    if y1 - y0 < 3.0 {
        return;
    }
    let w = Wall {
        side: Side::Front,
        o,
    };
    c.surf(Surf::Glow);
    wall_box(
        c,
        &w,
        (l - 0.15, l + 0.15),
        (y0, y1),
        (0.36, 1.66),
        col(look.accent),
    );
    c.surf(Surf::Metal);
    wall_box(
        c,
        &w,
        (l - 0.05, l + 0.05),
        (y0 + 0.3, y0 + 0.45),
        (0.0, 0.36),
        col([60, 60, 60]),
    );
    wall_box(
        c,
        &w,
        (l - 0.05, l + 0.05),
        (y1 - 0.45, y1 - 0.3),
        (0.0, 0.36),
        col([60, 60, 60]),
    );
    c.surf(Surf::Plain);
}

/// Balconies under the windows of flats on the street side (at the back
/// they could meet the balconies of the building behind).
fn balconies(c: &mut Ctx, level: usize) {
    let plan = c.plan;
    let look = c.look;
    let f = &plan.floors[level];
    let slab_c = col(look.trim);
    let rail = col([240, 240, 236]);
    for op in plan.openings.iter().filter(|op| {
        op.floor as usize == level
            && op.kind == OpeningKind::Window
            && matches!(op.side, Side::Front | Side::Back)
            && f.rooms
                .get(op.room)
                .is_some_and(|r| r.kind == RoomKind::Apartment)
    }) {
        if hidden_by_tower(plan, op) {
            continue;
        }
        let w = Wall {
            side: op.side,
            o: f.outline,
        };
        let (t0, t1) = (
            op.along - op.width * 0.5 - 0.35,
            op.along + op.width * 0.5 + 0.35,
        );
        let y = f.elevation;
        let depth = 1.3;
        wall_box(c, &w, (t0, t1), (y - 0.05, y + 0.22), (0.0, depth), slab_c);
        let top = y + 0.22 + 1.0;
        c.surf(Surf::Metal);
        wall_box(
            c,
            &w,
            (t0, t1),
            (top - 0.06, top),
            (depth - 0.08, depth - 0.02),
            rail,
        );
        wall_box(
            c,
            &w,
            (t0, t0 + 0.06),
            (top - 0.06, top),
            (0.0, depth - 0.08),
            rail,
        );
        wall_box(
            c,
            &w,
            (t1 - 0.06, t1),
            (top - 0.06, top),
            (0.0, depth - 0.08),
            rail,
        );
        let n = ((t1 - t0) / 0.5).round().max(2.0) as usize;
        for k in 0..=n {
            let x = t0 + 0.03 + (t1 - t0 - 0.06) * k as f64 / n as f64;
            wall_box(
                c,
                &w,
                (x - 0.02, x + 0.02),
                (y + 0.22, top - 0.06),
                (depth - 0.07, depth - 0.03),
                rail,
            );
        }
        c.surf(Surf::Plain);
    }
}

/// An iron fire escape on the front: a platform at each floor and a stair
/// down to the one below.
fn fire_escape(c: &mut Ctx, level: usize) {
    let plan = c.plan;
    let f = &plan.floors[level];
    let o = f.outline;
    if o != plan.floors[0].outline {
        return;
    }
    let lines = bay_lines(o.min.x, o.max.x, c.look.bay);
    if lines.len() < 4 {
        return;
    }
    let k = (lines.len() - 1) / 2 - 1;
    let (t0, t1) = (lines[k] + 0.2, lines[k + 2] - 0.2);
    let w = Wall {
        side: Side::Front,
        o,
    };
    let iron = col([46, 44, 42]);
    let y = f.elevation;
    let depth = 1.3;
    c.surf(Surf::Metal);
    wall_box(c, &w, (t0, t1), (y - 0.08, y), (0.0, depth), iron);
    let top = y + 1.0;
    wall_box(
        c,
        &w,
        (t0, t1),
        (top - 0.05, top),
        (depth - 0.05, depth),
        iron,
    );
    wall_box(
        c,
        &w,
        (t0, t0 + 0.05),
        (top - 0.05, top),
        (0.0, depth - 0.05),
        iron,
    );
    wall_box(
        c,
        &w,
        (t1 - 0.05, t1),
        (top - 0.05, top),
        (0.0, depth - 0.05),
        iron,
    );
    let n = ((t1 - t0) / 0.4).round() as usize;
    for i in 0..=n {
        let x = t0 + 0.02 + (t1 - t0 - 0.04) * i as f64 / n as f64;
        wall_box(
            c,
            &w,
            (x - 0.015, x + 0.015),
            (y, top - 0.05),
            (depth - 0.04, depth - 0.01),
            iron,
        );
    }
    // Stair from this platform down to the one below (or a drop ladder).
    let below = plan.floors[level - 1].elevation;
    let tm = (t0 + t1) * 0.5;
    if level > 1 {
        let bottom = [
            w.at(t0 + 0.1, below + 0.01, 0.1),
            w.at(tm - 0.1, y - 0.09, 0.1),
            w.at(tm - 0.1, y - 0.09, 0.8),
            w.at(t0 + 0.1, below + 0.01, 0.8),
        ];
        let top_q = bottom.map(|p| p + DVec3::Y * 0.08);
        c.hexa(bottom, top_q, iron);
    } else {
        for x in [t1 - 0.8, t1 - 0.3] {
            wall_box(
                c,
                &w,
                (x - 0.025, x + 0.025),
                (y - 3.0, y - 0.08),
                (0.6, 0.65),
                iron,
            );
        }
        for r in 0..8 {
            let yr = y - 0.4 - r as f64 * 0.32;
            wall_box(
                c,
                &w,
                (t1 - 0.775, t1 - 0.325),
                (yr - 0.02, yr + 0.02),
                (0.61, 0.64),
                iron,
            );
        }
    }
    c.surf(Surf::Plain);
}
