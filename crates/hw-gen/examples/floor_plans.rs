//! Draws office layouts from the game-scale interior designer
//! (`hw_gen::layout`) as SVG sheets: a plan of every floor and two
//! sections, to judge the layouts before anything is built in 3D.
//!
//! Usage: cargo run -p hw-gen --example floor_plans -- [out dir] [seed w d storeys [scheme]]...
//! Without buildings it draws a sample of every scheme.

use glam::DVec2;
use hw_core::Seed;
use hw_gen::geom::Rect;
use hw_gen::layout::office::{plan, plan_scheme};
use hw_gen::layout::{Flight, Layout, LinkKind, Scheme, Space, SpaceKind, TREAD};
use std::fmt::Write;

/// Pixels per metre.
const S: f64 = 9.0;

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args
        .next()
        .unwrap_or_else(|| "screenshots/floor_plans".into());
    let rest: Vec<String> = args.collect();
    let mut jobs: Vec<(u64, f64, f64, u32, Option<Scheme>)> = Vec::new();
    for c in rest.chunks(5) {
        let n = |i: usize| c.get(i).and_then(|v| v.parse::<f64>().ok());
        if let (Some(seed), Some(w), Some(d), Some(st)) = (n(0), n(1), n(2), n(3)) {
            let scheme = c.get(4).and_then(|s| match s.as_str() {
                "grand" => Some(Scheme::GrandHall),
                "atrium" => Some(Scheme::Atrium),
                "split" => Some(Scheme::SplitLevel),
                "house" => Some(Scheme::ShopHouse),
                _ => None,
            });
            jobs.push((seed as u64, w, d, st as u32, scheme));
        }
    }
    if jobs.is_empty() {
        jobs = vec![
            (11, 50.0, 46.0, 4, Some(Scheme::GrandHall)),
            (12, 44.0, 42.0, 5, Some(Scheme::GrandHall)),
            (21, 56.0, 54.0, 6, Some(Scheme::Atrium)),
            (24, 62.0, 58.0, 5, Some(Scheme::Atrium)),
            (31, 42.0, 48.0, 5, Some(Scheme::SplitLevel)),
            (34, 60.0, 50.0, 4, Some(Scheme::SplitLevel)),
        ];
    }
    std::fs::create_dir_all(&out).expect("output folder");
    for (seed, w, d, st, scheme) in jobs {
        let size = DVec2::new(w, d);
        let l = match scheme {
            Some(s) => plan_scheme(Seed(seed), size, st, s).unwrap_or_else(|| {
                eprintln!("{s:?} does not fit seed {seed} {w}x{d} x{st}; using the planner's pick");
                plan(Seed(seed), size, st)
            }),
            None => plan(Seed(seed), size, st),
        };
        let path = format!("{out}/{:?}-{seed}-{w}x{d}x{st}.svg", l.scheme).to_lowercase();
        std::fs::write(&path, sheet(&l, seed, st)).expect("write");
        let p = l.problems();
        println!("{path}: {} problems", p.len());
        for q in p.iter().take(10) {
            println!("  {q}");
        }
    }
}

fn colour(k: SpaceKind) -> &'static str {
    match k {
        SpaceKind::Vestibule => "#e8d9b0",
        SpaceKind::Lobby => "#f2c86b",
        SpaceKind::Atrium => "#9fd3e6",
        SpaceKind::Gallery => "#f6e3a1",
        SpaceKind::Arcade => "#efe0b8",
        SpaceKind::Bridge => "#c9a96e",
        SpaceKind::Landing => "#dccba4",
        SpaceKind::Concourse => "#e9cf8a",
        SpaceKind::Hall => "#c8ddb0",
        SpaceKind::Boardroom => "#e4b3b3",
        SpaceKind::Shop => "#e8b98a",
        SpaceKind::StairHall => "#c7b7e0",
        SpaceKind::Shaft => "#8a8a8a",
        SpaceKind::Canyon => "#a7c7e7",
        SpaceKind::Service => "#cfcfcf",
    }
}

fn name(k: SpaceKind) -> &'static str {
    match k {
        SpaceKind::Vestibule => "Vestibule",
        SpaceKind::Lobby => "Lobby",
        SpaceKind::Atrium => "Atrium",
        SpaceKind::Gallery => "Gallery",
        SpaceKind::Arcade => "Arcade",
        SpaceKind::Bridge => "Bridge",
        SpaceKind::Landing => "Landing",
        SpaceKind::Concourse => "Concourse",
        SpaceKind::Hall => "Office hall",
        SpaceKind::Boardroom => "Boardroom",
        SpaceKind::Shop => "Shop",
        SpaceKind::StairHall => "Stair hall",
        SpaceKind::Shaft => "Lift",
        SpaceKind::Canyon => "Canyon",
        SpaceKind::Service => "Service",
    }
}

fn scheme_name(s: Scheme) -> &'static str {
    match s {
        Scheme::GrandHall => "Grand hall",
        Scheme::Atrium => "Atrium",
        Scheme::SplitLevel => "Split level",
        Scheme::ShopHouse => "Shop house",
    }
}

/// A plan or section panel: its own coordinate frame on the sheet.
struct Panel {
    ox: f64,
    oy: f64,
}

impl Panel {
    fn p(&self, x: f64, z: f64) -> (f64, f64) {
        (self.ox + x * S, self.oy + z * S)
    }
    fn rect(&self, r: &Rect) -> (f64, f64, f64, f64) {
        let (x, y) = self.p(r.min.x, r.min.y);
        (x, y, r.size().x * S, r.size().y * S)
    }
}

fn sheet(l: &Layout, seed: u64, storeys: u32) -> String {
    let levels = l.levels();
    let (w, d) = (l.size.x, l.size.y);
    let cols = 3usize;
    let pw = w * S + 60.0;
    let ph = d * S + 70.0;
    let rows = levels.len().div_ceil(cols);
    let height = l.height();
    let sec_h = height * S + 70.0;
    let width = (cols as f64 * pw).max(d * S + w * S + 140.0) + 40.0;
    let total_h = 110.0 + rows as f64 * ph + sec_h + 60.0;
    let mut s = String::new();
    let _ = writeln!(
        s,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0}" height="{total_h:.0}" viewBox="0 0 {width:.0} {total_h:.0}" font-family="Helvetica, Arial, sans-serif">
<defs><pattern id="hatch" width="8" height="8" patternUnits="userSpaceOnUse" patternTransform="rotate(45)"><line x1="0" y1="0" x2="0" y2="8" stroke="#6b8fb3" stroke-width="1.2"/></pattern>
<marker id="arrow" markerWidth="8" markerHeight="8" refX="6" refY="4" orient="auto"><path d="M0,0 L8,4 L0,8 z" fill="#3a2a10"/></marker></defs>
<rect width="100%" height="100%" fill="#fbf8f1"/>"##
    );
    let lifts: Vec<String> = l.lifts.iter().map(|x| format!("{:?}", x.kind)).collect();
    let stairs: Vec<String> = l.stairs.iter().map(|x| format!("{:?}", x.kind)).collect();
    let _ = writeln!(
        s,
        r##"<text x="20" y="38" font-size="26" font-weight="bold">{} office · seed {seed} · {w:.0} × {d:.0} m · {storeys} storeys · {:.1} m tall</text>
<text x="20" y="66" font-size="15" fill="#444">Stairs: {} · Lifts: {} · {} columns · problems: {}</text>
<text x="20" y="88" font-size="13" fill="#666">Street at the top of each plan. Hatched blue: open to the room below (a tall room's upper part). Brown dashes: rails. Cyan: glass. Arrows: stairs going up.</text>"##,
        scheme_name(l.scheme),
        height,
        stairs.join(", "),
        lifts.join(", "),
        l.columns.len(),
        l.problems().len()
    );
    for (i, &y) in levels.iter().enumerate() {
        let next = levels.get(i + 1).copied().unwrap_or(f64::INFINITY);
        let panel = Panel {
            ox: 30.0 + (i % cols) as f64 * pw,
            oy: 130.0 + (i / cols) as f64 * ph,
        };
        plan_panel(&mut s, l, &panel, y, next, i);
    }
    let sy = 110.0 + rows as f64 * ph + 30.0;
    // Long section through the middle (z across), cross section through
    // the lobby or atrium (x across).
    let cut_x = match l.scheme {
        Scheme::SplitLevel => w * 0.5 - 2.75,
        _ => w * 0.5 + 0.01,
    };
    let cut_z = l
        .spaces
        .iter()
        .find(|s| {
            matches!(
                s.kind,
                SpaceKind::Lobby | SpaceKind::Atrium | SpaceKind::Canyon
            )
        })
        .map(|s| s.rect.center().y)
        .unwrap_or(d * 0.5);
    section(&mut s, l, 30.0, sy, height, true, cut_x);
    section(&mut s, l, 30.0 + d * S + 80.0, sy, height, false, cut_z);
    s.push_str("</svg>\n");
    s
}

fn label(s: &mut String, x: f64, y: f64, size: f64, text: &str, fill: &str) {
    let _ = writeln!(
        s,
        r##"<text x="{x:.1}" y="{y:.1}" font-size="{size}" text-anchor="middle" fill="{fill}">{text}</text>"##
    );
}

fn plan_panel(s: &mut String, l: &Layout, p: &Panel, y: f64, next: f64, i: usize) {
    let near = |a: f64, b: f64| (a - b).abs() < 1e-3;
    let title = if i == 0 {
        "Ground floor".to_string()
    } else {
        format!("Floor at +{y:.1} m")
    };
    let _ = writeln!(
        s,
        r##"<text x="{:.1}" y="{:.1}" font-size="17" font-weight="bold">{title}</text>"##,
        p.ox,
        p.oy - 12.0
    );
    // Rooms that start here, and tall rooms passing through.
    for sp in l.spaces.iter().filter(|t| t.within.is_none()) {
        let (x, yy, ww, hh) = p.rect(&sp.rect);
        if near(sp.y, y) {
            let _ = writeln!(
                s,
                r##"<rect x="{x:.1}" y="{yy:.1}" width="{ww:.1}" height="{hh:.1}" fill="{}" stroke="#2b2b2b" stroke-width="2.2"/>"##,
                colour(sp.kind)
            );
        } else if sp.y < y - 1e-3 && sp.top() > y + 1e-3 {
            let _ = writeln!(
                s,
                r##"<rect x="{x:.1}" y="{yy:.1}" width="{ww:.1}" height="{hh:.1}" fill="{}" fill-opacity="0.35" stroke="#2b2b2b" stroke-width="2.2"/><rect x="{x:.1}" y="{yy:.1}" width="{ww:.1}" height="{hh:.1}" fill="url(#hatch)" opacity="0.5"/>"##,
                colour(sp.kind)
            );
        }
    }
    // Things standing in tall rooms: bridges, landings, shafts.
    for sp in l.spaces.iter().filter(|t| t.within.is_some()) {
        let here = (sp.y >= y - 1e-3 && sp.y < next - 1e-3)
            || (sp.kind == SpaceKind::Shaft && sp.y <= y && sp.top() > y);
        if !here {
            continue;
        }
        let (x, yy, ww, hh) = p.rect(&sp.rect);
        let _ = writeln!(
            s,
            r##"<rect x="{x:.1}" y="{yy:.1}" width="{ww:.1}" height="{hh:.1}" fill="{}" stroke="#2b2b2b" stroke-width="1.4"/>"##,
            colour(sp.kind)
        );
        if sp.kind == SpaceKind::Shaft {
            let _ = writeln!(
                s,
                r##"<path d="M{x:.1},{yy:.1} l{ww:.1},{hh:.1} M{:.1},{yy:.1} l-{ww:.1},{hh:.1}" stroke="#eee" stroke-width="1.2"/>"##,
                x + ww
            );
        } else if !near(sp.y, y) {
            label(
                s,
                x + ww * 0.5,
                yy + hh * 0.5 + 4.0,
                10.0,
                &format!("+{:.1}", sp.y),
                "#333",
            );
        }
    }
    // Stair landings and flights that start on this floor (or up to the next).
    for st in &l.stairs {
        for la in st
            .landings
            .iter()
            .filter(|la| la.y >= y - 1e-3 && la.y < next - 1e-3)
        {
            let (x, yy, ww, hh) = p.rect(&la.rect);
            let _ = writeln!(
                s,
                r##"<rect x="{x:.1}" y="{yy:.1}" width="{ww:.1}" height="{hh:.1}" fill="#e7d6ad" stroke="#7a5c2e" stroke-width="1"/>"##
            );
        }
        for f in st
            .flights
            .iter()
            .filter(|f| f.y0() >= y - 1e-3 && f.y0() < next - 1e-3)
        {
            flight(s, p, f);
        }
    }
    // Links.
    for k in l.links.iter().filter(|k| near(k.y, y)) {
        let (a, b) = k.ends();
        let (x1, y1) = p.p(a.x, a.y);
        let (x2, y2) = p.p(b.x, b.y);
        let line = |s: &mut String, col: &str, wdt: f64, dash: &str| {
            let _ = writeln!(
                s,
                r##"<line x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}" stroke="{col}" stroke-width="{wdt}" {dash}/>"##
            );
        };
        match k.kind {
            LinkKind::Rail => {
                line(s, "#fbf8f1", 3.0, "");
                line(s, "#8a5a1e", 3.0, r#"stroke-dasharray="6 4""#);
            }
            LinkKind::Glass => {
                line(s, "#fbf8f1", 3.0, "");
                line(s, "#27b7d8", 3.5, "");
            }
            LinkKind::Open | LinkKind::Arch => {
                line(s, "#fbf8f1", 3.4, "");
                line(s, "#999", 0.8, r#"stroke-dasharray="2 4""#);
            }
            LinkKind::Door | LinkKind::Double | LinkKind::Entrance => {
                line(s, "#fbf8f1", 3.4, "");
                // Jambs.
                let n = if k.along_x { DVec2::Y } else { DVec2::X } * 0.5;
                for e in [a, b] {
                    let (u1, v1) = p.p(e.x - n.x, e.y - n.y);
                    let (u2, v2) = p.p(e.x + n.x, e.y + n.y);
                    let _ = writeln!(
                        s,
                        r##"<line x1="{u1:.1}" y1="{v1:.1}" x2="{u2:.1}" y2="{v2:.1}" stroke="#2b2b2b" stroke-width="2"/>"##
                    );
                }
                if k.kind == LinkKind::Entrance {
                    let (cx, cy) = p.p(k.at.x, k.at.y);
                    let _ = writeln!(
                        s,
                        r##"<path d="M{:.1},{:.1} l8,-12 l-16,0 z" fill="#c0392b"/>"##,
                        cx,
                        cy - 2.0
                    );
                }
            }
        }
    }
    for c in l.columns.iter().filter(|c| near(c.y, y)) {
        let (x, yy) = p.p(c.at.x, c.at.y);
        let _ = writeln!(
            s,
            r##"<circle cx="{x:.1}" cy="{yy:.1}" r="{:.1}" fill="#555"/>"##,
            c.r * S
        );
    }
    // Room names.
    for sp in l
        .spaces
        .iter()
        .filter(|t| t.within.is_none() && near(t.y, y))
    {
        room_label(s, p, sp);
    }
    for (k, lift) in l.lifts.iter().enumerate() {
        let sp = &l.spaces[lift.shaft];
        if i == 0 || k == 0 {
            let (x, yy, ww, _) = p.rect(&sp.rect);
            label(
                s,
                x + ww * 0.5,
                yy - 3.0,
                10.0,
                &format!("{:?}", lift.kind),
                "#222",
            );
        }
    }
    // Scale bar: 10 m.
    let (x, yy) = p.p(0.0, l.size.y + 3.0);
    let _ = writeln!(
        s,
        r##"<line x1="{x:.1}" y1="{yy:.1}" x2="{:.1}" y2="{yy:.1}" stroke="#222" stroke-width="3"/><text x="{:.1}" y="{:.1}" font-size="11">10 m</text>"##,
        x + 10.0 * S,
        x + 10.0 * S + 6.0,
        yy + 4.0
    );
}

fn room_label(s: &mut String, p: &Panel, sp: &Space) {
    let (x, y, w, h) = p.rect(&sp.rect);
    let size = sp.rect.size();
    if w < 40.0 || h < 26.0 {
        return;
    }
    let big = if w > 110.0 { 13.0 } else { 10.5 };
    label(
        s,
        x + w * 0.5,
        y + h * 0.5 - 2.0,
        big,
        name(sp.kind),
        "#1d1d1d",
    );
    label(
        s,
        x + w * 0.5,
        y + h * 0.5 + big,
        big - 2.0,
        &format!("{:.0}×{:.0} m, {:.1} m high", size.x, size.y, sp.h),
        "#333",
    );
}

fn flight(s: &mut String, p: &Panel, f: &Flight) {
    match *f {
        Flight::Straight {
            from,
            dir,
            width,
            risers,
            ..
        } => {
            let a = DVec2::new(from.x, from.z);
            let side = dir.perp() * (width * 0.5);
            let run = Flight::run(risers);
            let corners = [
                a - side,
                a + side,
                a + side + dir * run,
                a - side + dir * run,
            ];
            let pts: Vec<String> = corners
                .iter()
                .map(|c| {
                    let (x, y) = p.p(c.x, c.y);
                    format!("{x:.1},{y:.1}")
                })
                .collect();
            let _ = writeln!(
                s,
                r##"<polygon points="{}" fill="#f4ead2" stroke="#3a2a10" stroke-width="1.2"/>"##,
                pts.join(" ")
            );
            for k in 1..risers.saturating_sub(1) {
                let m = a + dir * (k as f64 * TREAD);
                let (x1, y1) = p.p((m - side).x, (m - side).y);
                let (x2, y2) = p.p((m + side).x, (m + side).y);
                let _ = writeln!(
                    s,
                    r##"<line x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}" stroke="#b49a6a" stroke-width="0.7"/>"##
                );
            }
            let b = a + dir * run;
            let (x1, y1) = p.p(a.x, a.y);
            let (x2, y2) = p.p(b.x, b.y);
            let _ = writeln!(
                s,
                r##"<line x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}" stroke="#3a2a10" stroke-width="1.6" marker-end="url(#arrow)"/>"##
            );
            let _ = writeln!(
                s,
                r##"<text x="{:.1}" y="{:.1}" font-size="9.5" text-anchor="middle" fill="#3a2a10">+{:.1}</text>"##,
                x2,
                y2 + if dir.y > 0.0 { 12.0 } else { -5.0 },
                f.y1()
            );
        }
        Flight::Arc {
            centre,
            r_in,
            r_out,
            a0,
            sweep,
            risers,
            ..
        } => {
            let n = 48;
            let at = |r: f64, a: f64| {
                let q = centre + DVec2::new(a.cos(), a.sin()) * r;
                p.p(q.x, q.y)
            };
            let mut d = String::new();
            for i in 0..=n {
                let (x, y) = at(r_out, a0 + sweep * i as f64 / n as f64);
                let _ = write!(d, "{}{x:.1},{y:.1} ", if i == 0 { "M" } else { "L" });
            }
            for i in (0..=n).rev() {
                let (x, y) = at(r_in, a0 + sweep * i as f64 / n as f64);
                let _ = write!(d, "L{x:.1},{y:.1} ");
            }
            let _ = writeln!(
                s,
                r##"<path d="{d}z" fill="#f4ead2" stroke="#3a2a10" stroke-width="1.2"/>"##
            );
            for k in 1..risers {
                let a = a0 + sweep * k as f64 / risers as f64;
                let (x1, y1) = at(r_in, a);
                let (x2, y2) = at(r_out, a);
                let _ = writeln!(
                    s,
                    r##"<line x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}" stroke="#b49a6a" stroke-width="0.7"/>"##
                );
            }
            let rm = (r_in + r_out) * 0.5;
            let mut d = String::new();
            for i in 0..=24 {
                let (x, y) = at(rm, a0 + sweep * (0.03 + 0.94 * i as f64 / 24.0));
                let _ = write!(d, "{}{x:.1},{y:.1} ", if i == 0 { "M" } else { "L" });
            }
            let _ = writeln!(
                s,
                r##"<path d="{d}" fill="none" stroke="#3a2a10" stroke-width="1.6" marker-end="url(#arrow)"/>"##
            );
            let (x, y) = at(rm, a0 + sweep);
            let _ = writeln!(
                s,
                r##"<text x="{x:.1}" y="{:.1}" font-size="9.5" text-anchor="middle" fill="#3a2a10">+{:.1}</text>"##,
                y - 4.0,
                f.y1()
            );
        }
    }
}

/// A section: `along_z` cuts at x = `cut` and runs along z, otherwise it
/// cuts at z = `cut` and runs along x. Ground at the bottom.
fn section(s: &mut String, l: &Layout, ox: f64, oy: f64, height: f64, along_z: bool, cut: f64) {
    let len = if along_z { l.size.y } else { l.size.x };
    let base = oy + height * S + 30.0;
    let title = if along_z {
        format!("Long section at x = {cut:.0} m (street on the left)")
    } else {
        format!("Cross section at z = {cut:.0} m")
    };
    let _ = writeln!(
        s,
        r##"<text x="{ox:.1}" y="{:.1}" font-size="17" font-weight="bold">{title}</text>"##,
        oy + 8.0
    );
    let span = |r: &Rect| -> Option<(f64, f64)> {
        let (c0, c1, u0, u1) = if along_z {
            (r.min.x, r.max.x, r.min.y, r.max.y)
        } else {
            (r.min.y, r.max.y, r.min.x, r.max.x)
        };
        (cut >= c0 && cut < c1).then_some((u0, u1))
    };
    let draw = |s: &mut String, u0: f64, u1: f64, y0: f64, y1: f64, fill: &str, sw: f64| {
        let _ = writeln!(
            s,
            r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="{fill}" stroke="#2b2b2b" stroke-width="{sw}"/>"##,
            ox + u0 * S,
            base - y1 * S,
            (u1 - u0) * S,
            (y1 - y0) * S
        );
    };
    for sp in l.spaces.iter().filter(|t| t.within.is_none()) {
        if let Some((u0, u1)) = span(&sp.rect) {
            draw(s, u0, u1, sp.y, sp.top(), colour(sp.kind), 2.2);
            let (w, h) = ((u1 - u0) * S, sp.h * S);
            if w > 50.0 && h > 26.0 {
                label(
                    s,
                    ox + (u0 + u1) * 0.5 * S,
                    base - (sp.y + sp.h * 0.5) * S + 4.0,
                    10.0,
                    name(sp.kind),
                    "#1d1d1d",
                );
            }
        }
    }
    for sp in l.spaces.iter().filter(|t| t.within.is_some()) {
        if let Some((u0, u1)) = span(&sp.rect) {
            if sp.kind == SpaceKind::Shaft {
                draw(s, u0, u1, sp.y, sp.top(), "#8a8a8a", 1.0);
            } else {
                draw(s, u0, u1, sp.y - 0.4, sp.y, "#7a5c2e", 1.0);
            }
        }
    }
    for st in &l.stairs {
        for la in &st.landings {
            if let Some((u0, u1)) = span(&la.rect) {
                draw(s, u0, u1, la.y - 0.4, la.y, "#7a5c2e", 1.0);
            }
        }
        for f in &st.flights {
            let mut d = String::new();
            let mut on = false;
            let n = (len / 0.05) as usize;
            for i in 0..=n {
                let u = i as f64 * 0.05;
                let q = if along_z {
                    DVec2::new(cut, u)
                } else {
                    DVec2::new(u, cut)
                };
                match f.height_at(q) {
                    Some(h) => {
                        let _ = write!(
                            d,
                            "{}{:.1},{:.1} ",
                            if on { "L" } else { "M" },
                            ox + u * S,
                            base - h * S
                        );
                        on = true;
                    }
                    None => on = false,
                }
            }
            if !d.is_empty() {
                let _ = writeln!(
                    s,
                    r##"<path d="{d}" fill="none" stroke="#3a2a10" stroke-width="2.4"/>"##
                );
            }
        }
    }
    // Ground, and a 1.8 m person for scale at the entrance.
    let _ = writeln!(
        s,
        r##"<line x1="{:.1}" y1="{base:.1}" x2="{:.1}" y2="{base:.1}" stroke="#222" stroke-width="3"/>"##,
        ox - 10.0,
        ox + len * S + 10.0
    );
    let px = ox + 3.0 * S;
    let _ = writeln!(
        s,
        r##"<g fill="#c0392b"><circle cx="{px:.1}" cy="{:.1}" r="{:.1}"/><rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}"/></g>"##,
        base - 1.6 * S,
        0.2 * S,
        px - 0.2 * S,
        base - 1.4 * S,
        0.4 * S,
        1.4 * S
    );
}
