//! The stairs up to street doors that stand high over the pavement: a stone
//! landing at the door, a flight of treads along the facade and a railing
//! on the open side, drawn from [`hw_gen::stoop`], the same boxes the
//! server's collision uses.

use crate::citymesh::MeshBuilder;
use engine::prelude::*;
use glam::{DVec2, DVec3};
use hw_gen::building::BuildingPlan;
use hw_gen::stoop::{stoops, Block, Stoop, RAIL_H, RAIL_THICK, RUN};

/// Draws every stoop of a building into `m`. `ground` is the town's ground
/// height at a point (x, z) of the town's frame.
pub fn draw(m: &mut MeshBuilder, plan: &BuildingPlan, base: f64, ground: &dyn Fn(DVec2) -> f64) {
    let list = stoops(plan, base, ground);
    if list.is_empty() {
        return;
    }
    let site = &plan.site;
    let world = |p: DVec2| site.origin + site.along * p.x + site.inward * p.y;
    let stone = rgb(150, 142, 128);
    let iron = {
        let f = plan.look.frame;
        rgb(f[0], f[1], f[2])
    };
    m.tex = Some(crate::facades::facade(plan).base);
    for st in &list {
        for b in st.stone() {
            prism(m, &world, b, stone);
        }
    }
    m.tex = None;
    for st in &list {
        railing(m, &world, st, iron);
    }
}

fn prism(m: &mut MeshBuilder, world: &dyn Fn(DVec2) -> DVec2, b: &Block, color: Color) {
    let (a, z) = (b.rect.min, b.rect.max);
    let pts = [
        world(a),
        world(DVec2::new(z.x, a.y)),
        world(z),
        world(DVec2::new(a.x, z.y)),
    ];
    m.prism(&pts, b.y0, b.y1, color);
}

/// Posts at every corner of a railing and along its sloped runs, and a
/// square bar on top.
fn railing(m: &mut MeshBuilder, world: &dyn Fn(DVec2) -> DVec2, st: &Stoop, color: Color) {
    let at = |p: DVec3| {
        let w = world(DVec2::new(p.x, p.z));
        DVec3::new(w.x, p.y, w.y)
    };
    let h = RAIL_THICK * 0.5;
    let post = |m: &mut MeshBuilder, p: DVec3| {
        let c = DVec2::new(p.x, p.z);
        let pts = [
            world(c + DVec2::new(-h, -h)),
            world(c + DVec2::new(h, -h)),
            world(c + DVec2::new(h, h)),
            world(c + DVec2::new(-h, h)),
        ];
        m.prism(&pts, p.y - RAIL_H - 0.02, p.y, color);
    };
    for rail in &st.rails {
        for w in rail.windows(2) {
            let (p, q) = (w[0], w[1]);
            post(m, p);
            if (p.y - q.y).abs() > 1e-9 {
                let len = DVec2::new(p.x - q.x, p.z - q.z).length();
                let n = (len / (RUN * 4.0)).round().max(1.0) as usize;
                for k in 1..n {
                    post(m, p.lerp(q, k as f64 / n as f64));
                }
            }
            beam(m, at(p), at(q), h, color);
        }
        if let Some(l) = rail.last() {
            post(m, *l);
        }
    }
}

/// A square bar from `a` to `b`.
fn beam(m: &mut MeshBuilder, a: DVec3, b: DVec3, h: f64, color: Color) {
    let d = (b - a).normalize_or_zero();
    if d == DVec3::ZERO {
        return;
    }
    let side = d.cross(DVec3::Y).normalize_or_zero();
    let side = if side == DVec3::ZERO { DVec3::X } else { side };
    let up = side.cross(d).normalize();
    let corner = |p: DVec3, s: f64, u: f64| p + side * (h * s) + up * (h * u);
    let rings = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    for i in 0..4 {
        let (s0, u0) = rings[i];
        let (s1, u1) = rings[(i + 1) % 4];
        let out = side * ((s0 + s1) * 0.5) + up * ((u0 + u1) * 0.5);
        m.quad(
            [
                corner(a, s0, u0),
                corner(b, s0, u0),
                corner(b, s1, u1),
                corner(a, s1, u1),
            ],
            out,
            color,
        );
    }
}
