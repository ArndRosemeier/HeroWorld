//! The solid parts of a building's roof: what stands on top of the walls
//! (the deck and its parapet, a pitched, hipped or mansard roof, a dome, a
//! Googie swoop, the sawtooth, a tower's crown, the taxi pad, the corner
//! tower's cap), as boxes a body bumps into. They follow what the client
//! draws (`hw-client::buildingmesh`: `cornice`, `roof`, `crown`, `tower`,
//! `taxi_pad`): keep the two in step. Curves and slopes become staircases
//! of boxes, each step at most [`RISE`] high and as high as the drawn
//! surface at its middle, so a body sees the roof within half a step of
//! where it is drawn. The test `every_roof_is_solid` (hw-client) probes
//! the drawn roofs of every look against the real solids.
//!
//! Heights are above the building's ground floor, plan positions in
//! building coordinates (x along the front, z inward), like
//! [`crate::layout::solid`].

use crate::building::BuildingPlan;
use crate::geom::Rect;
use crate::style::{Cornice, Crown, Look, Ridge, Roof};
use crate::urban::footprint::Footprint;
use glam::DVec2;

/// Most a step of a sloped or curved roof rises (m): a hero walks up a
/// roof of these.
pub const RISE: f64 = 0.3;
/// The cornice and roof slab on a flat tier: how far it stands over the
/// top floor's ceiling (m), plain and with the stepped cornice.
const DECK: f64 = 0.3;
const DECK_STEPPED: f64 = 0.6;
/// A tier's terrace railing: height and thickness.
const RAIL: (f64, f64) = (1.1, 0.25);

/// How tall the deck over a tier is, as the cornice leaves it.
pub fn deck_height(look: &Look) -> f64 {
    if look.cornice == Cornice::Stepped {
        DECK_STEPPED
    } else {
        DECK
    }
}

/// How far the roof and its cornice reach past the building's ground
/// outline (m): where its solids stand outside the site's corners.
pub fn overhang(look: &Look) -> f64 {
    match look.roof {
        Roof::Swoop => 2.4,
        Roof::Gable | Roof::Hip => look.eaves + 0.1,
        _ => 0.5,
    }
}

/// A box over `r` from `y0` to `y1`.
fn slab(out: &mut Vec<Footprint>, r: Rect, y0: f64, y1: f64) {
    let (s, c) = (r.size(), r.center());
    if s.x > 1e-6 && s.y > 1e-6 && y1 > y0 {
        out.push(Footprint::Box {
            at: c,
            dir: DVec2::Y,
            half: s * 0.5,
            y0,
            y1,
        });
    }
}

fn disc(out: &mut Vec<Footprint>, at: DVec2, r: f64, y0: f64, y1: f64) {
    if r > 0.05 && y1 > y0 {
        out.push(Footprint::Disc { at, r, y0, y1 });
    }
}

/// A wall of `h` and thickness `th` round the edge of `o`, with the
/// coping along its top (a little wider and 0.1 higher).
fn rim(out: &mut Vec<Footprint>, o: Rect, y: f64, h: f64, th: f64) {
    let e = 0.11;
    let (x0, x1, z0, z1) = (o.min.x - e, o.max.x + e, o.min.y - e, o.max.y + e);
    let t = th + 0.12;
    let top = y + h + 0.1;
    slab(out, Rect::from_xz(x0, z0, x1, z0 + t), y, top);
    slab(out, Rect::from_xz(x0, z1 - t, x1, z1), y, top);
    slab(out, Rect::from_xz(x0, z0 + t, x0 + t, z1 - t), y, top);
    slab(out, Rect::from_xz(x1 - t, z0 + t, x1, z1 - t), y, top);
}

/// How many steps climb `h`.
fn steps(h: f64, rise: f64) -> usize {
    ((h / rise).ceil() as usize).clamp(1, 60)
}

/// A pyramid-like roof over `o`: its sides slope up `h` to a flat top over
/// `o` grown by `grow` and shrunk by `top_inset` in x and z. Each step is
/// the box over the part of the roof higher than its middle.
fn frustum(
    out: &mut Vec<Footprint>,
    o: Rect,
    y: f64,
    h: f64,
    grow: DVec2,
    top_inset: DVec2,
    rise: f64,
) {
    let n = steps(h, rise);
    for k in 0..n {
        let f = (k as f64 + 0.5) / n as f64;
        // From the foot (grown) to the top (shrunk), by how far up.
        let inset = -grow + (grow + top_inset) * f;
        let r = Rect::from_xz(
            o.min.x + inset.x,
            o.min.y + inset.y,
            o.max.x - inset.x,
            o.max.y - inset.y,
        );
        slab(out, r, y, y + h * (k + 1) as f64 / n as f64);
    }
}

/// A round dome of radius `r` sitting on `y` (a hemisphere): a stack of
/// discs, each as wide as the dome is at its middle.
fn dome(out: &mut Vec<Footprint>, at: DVec2, r: f64, y: f64) {
    let n = steps(r, 0.4);
    for k in 0..n {
        let z = (k as f64 + 0.5) / n as f64 * r;
        let rho = (r * r - z * z).max(0.0).sqrt();
        disc(out, at, rho, y, y + r * (k + 1) as f64 / n as f64);
    }
}

/// A cone from radius `r0` at `y` to a point `h` over it.
fn cone(out: &mut Vec<Footprint>, at: DVec2, r0: f64, y: f64, h: f64) {
    let n = steps(h, 1.2);
    for k in 0..n {
        let f = (k as f64 + 0.5) / n as f64;
        disc(
            out,
            at,
            r0 * (1.0 - f),
            y,
            y + h * (k + 1) as f64 / n as f64,
        );
    }
}

/// A gable or hip roof over `o` (see `buildingmesh::pitched`).
fn pitched(out: &mut Vec<Footprint>, o: Rect, y: f64, hip: bool, look: &Look) {
    let ov = look.eaves;
    let along_x = match look.ridge {
        Ridge::Long => o.size().x >= o.size().y,
        Ridge::Front => true,
        Ridge::Deep => false,
    };
    // s along the ridge, t across it.
    let (ws0, ws1, wt0, wt1) = if along_x {
        (o.min.x, o.max.x, o.min.y, o.max.y)
    } else {
        (o.min.y, o.max.y, o.min.x, o.max.x)
    };
    let (s0, s1, t0, t1) = (ws0 - ov, ws1 + ov, wt0 - ov, wt1 + ov);
    let half = (t1 - t0) * 0.5;
    let h = (half * look.pitch).clamp(1.0, 14.0);
    let inset = if hip {
        half.min((s1 - s0) * 0.5 - 0.05)
    } else {
        0.0
    };
    let rect = |s0: f64, s1: f64, t0: f64, t1: f64| {
        if along_x {
            Rect::from_xz(s0, t0, s1, t1)
        } else {
            Rect::from_xz(t0, s0, t1, s1)
        }
    };
    let n = steps(h, RISE);
    for k in 0..n {
        let f = (k as f64 + 0.5) / n as f64;
        let top = y + h * (k + 1) as f64 / n as f64;
        slab(
            out,
            rect(s0 + inset * f, s1 - inset * f, t0 + half * f, t1 - half * f),
            y,
            top,
        );
    }
    dormers(
        out,
        look,
        &rect,
        Pitch {
            y,
            h,
            half,
            tm: (t0 + t1) * 0.5,
            ws0,
            ws1,
            wt0,
            wt1,
        },
    );
}

/// The dormers of a steep roof: boxes standing out of each slope.
fn dormers(
    out: &mut Vec<Footprint>,
    look: &Look,
    rect: &dyn Fn(f64, f64, f64, f64) -> Rect,
    geo: Pitch,
) {
    let Pitch {
        y,
        h,
        half,
        tm,
        ws0,
        ws1,
        wt0,
        wt1,
    } = geo;
    if !look.dormers || h < 2.6 {
        return;
    }
    let len = ws1 - ws0;
    let n_d = ((len - 3.0) / 4.0).floor().max(0.0) as usize;
    for side in [-1.0, 1.0] {
        let tf = if side < 0.0 { wt0 + 0.5 } else { wt1 - 0.5 };
        let yf = y + h * (1.0 - (tf - tm).abs() / half);
        let top = yf + 1.5;
        if top > y + h - 0.3 {
            continue;
        }
        for k in 0..n_d {
            let s = ws0 + 2.0 + (len - 4.0) * (k as f64 + 0.5) / n_d as f64;
            let (t0, t1) = if side < 0.0 {
                (tf - 0.15, tm)
            } else {
                (tm, tf + 0.15)
            };
            slab(out, rect(s - 0.85, s + 0.85, t0, t1), yf - 0.4, top + 0.12);
        }
    }
}

/// The measures of a pitched roof the dormers need.
#[derive(Clone, Copy)]
struct Pitch {
    y: f64,
    h: f64,
    half: f64,
    tm: f64,
    ws0: f64,
    ws1: f64,
    wt0: f64,
    wt1: f64,
}

/// The roof's solids on the top tier `o`, whose deck is at `y`.
fn roof(out: &mut Vec<Footprint>, plan: &BuildingPlan, o: Rect, y: f64) {
    let look = &plan.look;
    match look.roof {
        Roof::Flat => {
            if look.parapet > 0.0 {
                rim(out, o, y, look.parapet, 0.3);
            }
            if look.taxi_pad && o.size().min_element() >= 12.0 {
                let r = (o.size().min_element() * 0.5 - 1.2).clamp(5.0, 9.0);
                // The pad stands on four legs, its deck 0.35 thick.
                disc(out, o.center(), r + 0.2, y + 1.25, y + 1.6);
            } else if look.rooftop_gear && look.crown == Crown::None {
                // The stair house over the core.
                if let Some(core) = plan.core.filter(|c| o.contains_rect(c, 1e-6)) {
                    slab(
                        out,
                        Rect::from_xz(
                            core.min.x + 0.5,
                            core.min.y + 0.5,
                            core.max.x - 0.5,
                            core.max.y - 0.5,
                        ),
                        y,
                        y + 3.65,
                    );
                }
            }
            crown(out, look, o, y);
        }
        Roof::Gable | Roof::Hip => pitched(out, o, y, look.roof == Roof::Hip, look),
        Roof::Mansard => frustum(out, o, y, 2.6, DVec2::splat(0.2), DVec2::splat(1.4), RISE),
        Roof::Dome => {
            let r = o.size().min_element() * 0.42;
            let c = o.center();
            disc(out, c, r, y, y + 1.6);
            dome(out, c, r, y + 1.6);
            rim(out, o, y, 0.8, 0.3);
        }
        Roof::Swoop => swoop(out, o, y),
        Roof::Sawtooth => sawtooth(out, o, y),
    }
}

/// The Googie slab that sweeps up towards the street, the glazed wall
/// under it, and the overhang at the front.
fn swoop(out: &mut Vec<Footprint>, o: Rect, y: f64) {
    let (rise_front, rise_back) = (2.6, 0.7);
    let h = |z: f64| y + rise_back + (rise_front - rise_back) * (o.max.y - z) / o.size().y.max(1.0);
    let (x0, x1) = (o.min.x - 0.9, o.max.x + 0.9);
    let (z0, z1) = (o.min.y - 2.2, o.max.y + 0.7);
    let n = steps(z1 - z0, 0.5);
    for k in 0..n {
        let (a, b) = (
            z0 + (z1 - z0) * k as f64 / n as f64,
            z0 + (z1 - z0) * (k + 1) as f64 / n as f64,
        );
        let mid = (a + b) * 0.5;
        // Over the building the glazed wall fills up to the slab; over the
        // overhang there is only the slab.
        let from = if mid >= o.min.y && mid <= o.max.y {
            y
        } else {
            h(mid)
        };
        slab(out, Rect::from_xz(x0, a, x1, b), from, h(mid) + 0.35);
    }
}

/// Factory sawtooth: sloped teeth with a vertical glazed face.
fn sawtooth(out: &mut Vec<Footprint>, o: Rect, y: f64) {
    let n = ((o.size().x / 6.0).round() as usize).max(1);
    let h = 2.4;
    for k in 0..n {
        let a = o.min.x + o.size().x * k as f64 / n as f64;
        let b = o.min.x + o.size().x * (k + 1) as f64 / n as f64;
        let m = steps(h, RISE);
        for i in 0..m {
            // From the low foot at `a` up to the glazed face at `b`.
            let x0 = a + (b - a) * (i as f64 + 0.5) / m as f64;
            slab(
                out,
                Rect::from_xz(x0, o.min.y, b, o.max.y),
                y,
                y + h * (i + 1) as f64 / m as f64,
            );
        }
    }
    // The works chimney is thin and far from anything a body reaches.
}

/// What stands on a tower: stepped crown, spire, fins' glass drum, mast,
/// a dome and its lantern.
fn crown(out: &mut Vec<Footprint>, look: &Look, o: Rect, y: f64) {
    let w = o.size().min_element();
    let cc = o.center();
    match look.crown {
        Crown::None => {}
        Crown::Stepped | Crown::Spire | Crown::Fins => {
            let mut r = o;
            let mut yy = y;
            for k in 0..3 {
                r = r.inset(w * 0.11);
                if r.size().min_element() < 2.0 {
                    break;
                }
                let h = 4.2 - k as f64 * 0.7;
                slab(out, r, yy, yy + h);
                // The trim band over it, a little wider.
                slab(
                    out,
                    Rect::from_xz(r.min.x - 0.2, r.min.y - 0.2, r.max.x + 0.2, r.max.y + 0.2),
                    yy + h,
                    yy + h + 0.3,
                );
                yy += h + 0.3;
            }
            match look.crown {
                Crown::Spire => {
                    let base = r.size().min_element() * 0.5;
                    let h = (base * 5.0).clamp(10.0, 32.0);
                    cone(out, cc, base * 0.8, yy, h);
                }
                Crown::Fins => {
                    // The glass drum (the fins are blades too thin to stop
                    // anyone).
                    let base = r.size().min_element() * 0.35;
                    disc(out, cc, base * 0.6, yy, yy + 6.0);
                }
                _ => {}
            }
        }
        Crown::Mast => {
            let r = o.inset(w * 0.2);
            slab(out, r, y, y + 3.0);
            cone(out, cc, 0.5, y + 3.0, 26.0);
        }
        Crown::Dome => {
            let r = w * 0.32;
            disc(out, cc, r, y, y + 3.0);
            slab(
                out,
                Rect::from_xz(
                    cc.x - r - 0.3,
                    cc.y - r - 0.3,
                    cc.x + r + 0.3,
                    cc.y + r + 0.3,
                ),
                y + 3.0,
                y + 3.3,
            );
            dome(out, cc, r, y + 3.3);
        }
    }
}

/// The streamline corner tower's centre and radius (as the client draws
/// it, `buildingmesh::corner_tower`).
pub fn corner_tower(plan: &BuildingPlan) -> Option<(DVec2, f64)> {
    let o = plan.floors.first()?.outline;
    let r = (o.size().min_element() * 0.16).clamp(2.2, 3.6);
    Some((DVec2::new(o.max.x - r + 0.5, o.min.y + r - 0.5), r))
}

impl BuildingPlan {
    /// The roof's solid parts: every tier's deck and terrace railing, and
    /// on the top tier whatever the look puts on it (see the module).
    pub fn roof_footprints(&self) -> Vec<Footprint> {
        let mut out = Vec::new();
        let look = &self.look;
        let n = self.floors.len();
        if n == 0 {
            return out;
        }
        let deck = deck_height(look);
        // Tiers: runs of floors with the same outline.
        let mut i = 0;
        while i < n {
            let mut j = i;
            while j + 1 < n && self.floors[j + 1].outline == self.floors[i].outline {
                j += 1;
            }
            let f = &self.floors[j];
            let o = self.floors[i].outline;
            let y = f.elevation + f.height;
            let top_tier = j + 1 == n;
            // The deck's slab over the ceiling, with its cornice.
            slab(&mut out, o, y - 0.3, y + deck);
            if top_tier {
                roof(&mut out, self, o, y + deck);
            } else if look.parapet > 0.0 {
                rim(&mut out, o, y + deck, RAIL.0, RAIL.1);
            }
            i = j + 1;
        }
        // The corner tower's cap over the roof deck.
        if look.corner_tower {
            if let Some((c, r)) = corner_tower(self) {
                let f = &self.floors[n - 1];
                let top = f.elevation + f.height;
                disc(&mut out, c, r + 0.3, top, top + 2.6 + 0.3);
            }
        }
        out
    }
}
