//! The police visor: a hologram the City Patrol issues to licensed heroes.
//! It shows where trouble is without lighting up the town. A new call
//! switches it on and it stays on until the player switches it off (T; T
//! again switches it back on). It
//! materialises in front of the hero's eyes (a bright line at eye height
//! that opens out, flickering, into a faint amber screen with a rim, scan
//! lines and the Patrol's star), marks each call with corner brackets, its
//! name and distance (an arrow at the edge when it is out of sight), and
//! dissolves the same way when switched off.
//!
//! The screen is drawn with egui over the 3D view; nothing in the world
//! glows over a call (the Hero Signal still does, for the worst trouble).
//! Seen from behind (third person) the visor is also a see-through amber
//! band of light over the hero's eyes ([`Visor::holo`]), opening and
//! closing with the screen; the hero touches a hand to the temple to switch
//! it ([`TAP_SECONDS`], the "visor tap" clips in [`crate::poses`]), and it
//! switches a moment later, when the fingers get there.

use crate::fightview::con_colour;
use crate::radioview::{call_colour, Radio};
use engine::prelude::*;
use engine::ui::egui;
use glam::{EulerRot, Mat4, Quat, Vec2, Vec3};
use std::f32::consts::{PI, TAU};

/// How long a new call pulses and is named "new" on the visor (s).
pub const NEW_CALL_SECONDS: f32 = 8.0;
/// Seconds to materialise.
const IN_SECONDS: f32 = 0.55;
/// Seconds to dissolve.
const OUT_SECONDS: f32 = 0.4;
/// How long the hero's hand-to-temple gesture takes (s).
pub const TAP_SECONDS: f32 = 1.1;
/// From the start of the gesture to the fingers at the temple, when the
/// visor switches (s).
const TAP_LEAD: f32 = 0.35;
/// Its amber.
const AMBER: (u8, u8, u8) = (255, 196, 92);

/// Why the visor is down, for the first line on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// On, with a call that has just come in.
    NewCall,
    /// On.
    On,
}

/// The visor's state: how far it has materialised and why it is wanted.
#[derive(Default)]
pub struct Visor {
    /// 0 gone, 1 fully there.
    pub shown: f32,
    /// Switched on: it stays on until the player switches it off (T).
    on: bool,
    /// The newest call id seen (a newer one switches it on).
    newest: Option<u32>,
    /// Running seconds (flicker, scan band).
    time: f32,
    /// Was it coming in (true) or going out the last frame.
    rising: bool,
    /// `HW_VISOR=<0..1>`: kept down and frozen that far along (screenshots).
    pin: Option<f32>,
    /// Wanted down (switches `rising` once the hand is at the temple).
    want: bool,
    /// Seconds until the switch, after `want` changed.
    switch_in: f32,
    /// `want` changed this frame: the hero should touch the temple.
    tapped: bool,
    /// The band of light over the hero's eyes, while it shows.
    holo_e: Option<EntityId>,
    /// Its hologram material, once added.
    holo_m: Option<CustomMaterialId>,
}

impl Visor {
    /// A visor; `HW_VISOR=1` keeps it down, `HW_VISOR=0.4` freezes it
    /// part way through materialising (for screenshots).
    pub fn from_env() -> Visor {
        let pin = std::env::var("HW_VISOR")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .map(|v| v.clamp(0.0, 1.0));
        Visor {
            pin,
            ..Visor::default()
        }
    }

    /// Why it should be down now, if at all.
    pub fn wanted(&self, radio: &Radio) -> Option<Why> {
        if !self.on {
            return None;
        }
        if radio.calls.values().any(|c| c.age < NEW_CALL_SECONDS) {
            Some(Why::NewCall)
        } else {
            Some(Why::On)
        }
    }

    /// One frame: `toggle` is T pressed. A new call switches it on; only
    /// the player switches it off. Returns why it is wanted.
    pub fn update(&mut self, dt: f32, toggle: bool, radio: &Radio) -> Option<Why> {
        self.time += dt;
        let newest = radio.calls.keys().max().copied();
        if newest.is_some() && newest > self.newest {
            self.on = true;
        }
        self.newest = self.newest.max(newest);
        if toggle {
            self.on = !self.on;
        }
        if let Some(pin) = self.pin {
            self.on = true;
            self.shown = pin;
            self.rising = true;
            return self.wanted(radio);
        }
        let why = self.wanted(radio);
        if why.is_some() != self.want {
            self.want = why.is_some();
            self.switch_in = TAP_LEAD;
            self.tapped = true;
        }
        self.switch_in = (self.switch_in - dt).max(0.0);
        if self.switch_in <= 0.0 {
            self.rising = self.want;
        }
        self.shown = if self.rising {
            (self.shown + dt / IN_SECONDS).min(1.0)
        } else {
            (self.shown - dt / OUT_SECONDS).max(0.0)
        };
        why
    }

    /// The visor was switched since the last call: the hero's hand goes to
    /// the temple now.
    pub fn take_tap(&mut self) -> bool {
        std::mem::take(&mut self.tapped)
    }

    /// The band of light over the hero's eyes: placed on `hero`'s head,
    /// opening and closing with the screen, hidden while the camera is in
    /// the head (first person) or there is no hero.
    pub fn holo(&mut self, world: &mut World, hero: Option<EntityId>, eye: Vec3) {
        let at = hero.and_then(|e| holo_place(world, e));
        let close = at.is_some_and(|(p, _)| p.distance(eye) < 0.5);
        let show = self.shown > 0.0 && !close;
        let Some((_, place)) = at.filter(|_| show) else {
            if let Some(e) = self.holo_e.take() {
                world.despawn(e);
            }
            return;
        };
        if self.holo_m.is_none() {
            self.holo_m = world.add_material(holo_material()).ok();
        }
        let e = match self.holo_e {
            Some(e) => e,
            None => match world.place(holo_mesh(), place) {
                Ok(e) => {
                    if let Some(m) = self.holo_m {
                        let _ = world.set_custom_material(e, m);
                    }
                    self.holo_e = Some(e);
                    e
                }
                Err(_) => return,
            },
        };
        let _ = world.set_place(e, place);
        if let Some(m) = self.holo_m {
            let open = Opening::at(self.shown, self.rising, self.time);
            let flick = open.alpha.max(0.15);
            let _ = world.set_material_param(m, 0, [open.height.max(0.04), flick, 1.0, 0.0]);
        }
    }

    /// Draws it over the 3D view (under the HUD's own panels).
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        ctx: &egui::Context,
        view_proj: Mat4,
        eye: Vec3,
        radio: &Radio,
        mine: u8,
        why: Option<Why>,
    ) {
        if self.shown <= 0.0 {
            return;
        }
        let screen = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("police visor"),
        ));
        let open = Opening::at(self.shown, self.rising, self.time);
        let centre = screen.center();
        // The bright line at eye height it grows from (and shrinks into).
        if open.line > 0.0 {
            let half = screen.width() * 0.5 * open.line;
            let glow = (255.0 * open.line_glow) as u8;
            for (w, a) in [(7.0_f32, 0.25_f32), (2.0, 1.0)] {
                painter.line_segment(
                    [
                        egui::pos2(centre.x - half, centre.y),
                        egui::pos2(centre.x + half, centre.y),
                    ],
                    egui::Stroke::new(w, amber((glow as f32 * a) as u8)),
                );
            }
        }
        if open.height <= 0.0 {
            return;
        }
        // The screen opens out from the line: everything is clipped to it.
        let half_h = screen.height() * 0.5 * open.height;
        let clip = egui::Rect::from_min_max(
            egui::pos2(screen.left(), centre.y - half_h),
            egui::pos2(screen.right(), centre.y + half_h),
        );
        let p = painter.with_clip_rect(clip);
        let a = open.alpha;
        let fade = |x: f32| (x * a).round().clamp(0.0, 255.0) as u8;
        // The glass: a faint amber over everything, darker at the rim.
        p.rect_filled(screen, 0.0, amber(fade(26.0)));
        vignette(&p, screen, fade(150.0));
        // The rim: a rounded frame, with ticks at the corners.
        let rim = screen.shrink(22.0);
        p.rect_stroke(
            rim,
            38.0,
            egui::Stroke::new(3.0_f32, amber(fade(150.0))),
            egui::StrokeKind::Middle,
        );
        for (cx, cy, sx, sy) in [
            (rim.left(), rim.top(), 1.0, 1.0),
            (rim.right(), rim.top(), -1.0, 1.0),
            (rim.left(), rim.bottom(), 1.0, -1.0),
            (rim.right(), rim.bottom(), -1.0, -1.0),
        ] {
            let c = egui::pos2(cx + sx * 12.0, cy + sy * 12.0);
            let stroke = egui::Stroke::new(2.5_f32, amber(fade(200.0)));
            p.line_segment([c, c + egui::vec2(sx * 46.0, 0.0)], stroke);
            p.line_segment([c, c + egui::vec2(0.0, sy * 46.0)], stroke);
        }
        // Scan lines, and a brighter band rolling down.
        let lines = amber(fade(14.0));
        let mut y = clip.top() - clip.top().rem_euclid(4.0);
        while y < clip.bottom() {
            p.line_segment(
                [egui::pos2(screen.left(), y), egui::pos2(screen.right(), y)],
                egui::Stroke::new(1.0_f32, lines),
            );
            y += 4.0;
        }
        let band = screen.top() + (self.time / 3.5).fract() * (screen.height() + 160.0) - 80.0;
        p.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(screen.left(), band - 40.0),
                egui::pos2(screen.right(), band + 40.0),
            ),
            0.0,
            amber(fade(12.0)),
        );
        // The Patrol's star, see-through, low right of the line of sight.
        badge(
            &p,
            egui::pos2(
                centre.x + screen.width() * 0.3,
                centre.y + screen.height() * 0.18,
            ),
            70.0,
            fade(60.0),
        );
        // A word on why it came down, top right of the line of sight.
        let calls = radio.calls.len();
        let head = match why {
            _ if calls == 0 => "PATROL VISOR - NO CALLS".to_string(),
            Some(Why::NewCall) => format!("PATROL VISOR - NEW CALL ({calls} OPEN)"),
            _ => format!(
                "PATROL VISOR - {calls} CALL{} OPEN",
                if calls == 1 { "" } else { "S" }
            ),
        };
        shadowed(
            &p,
            egui::pos2(rim.right() - 70.0, rim.top() + 54.0),
            egui::Align2::RIGHT_TOP,
            &head,
            14.0,
            amber(fade(240.0)),
            fade(160.0),
        );
        shadowed(
            &p,
            egui::pos2(rim.right() - 70.0, rim.top() + 74.0),
            egui::Align2::RIGHT_TOP,
            "T: switch off",
            12.0,
            amber(fade(200.0)),
            fade(140.0),
        );
        // The calls: brackets on the spot, or an arrow at the rim.
        // Distances as the wrist radio gives them: from the eye.
        let from = eye;
        let jitter = open.jitter;
        for c in radio.calls.values() {
            let flat = Vec2::new(c.at.x - from.x, c.at.z - from.z).length();
            let (r, g, b) = call_colour(c.kind);
            let tint = egui::Color32::from_rgba_unmultiplied(r, g, b, fade(230.0));
            let con = con_colour(c.level, mine);
            let label_col =
                egui::Color32::from_rgba_unmultiplied(con.r(), con.g(), con.b(), fade(235.0));
            let label = format!("{}  {:.0} m", c.kind.name().to_uppercase(), flat);
            // Fresh calls pulse for their first seconds.
            let pulse = if c.age < NEW_CALL_SECONDS {
                1.0 + 0.25 * (c.age * TAU * 1.5).sin().abs()
            } else {
                1.0
            };
            match on_screen(view_proj, c.at + Vec3::Y * 1.6, screen) {
                Some(at) => {
                    let at = at + egui::vec2(jitter, 0.0);
                    let s = (1400.0 / flat.max(1.0)).clamp(14.0, 56.0) * pulse;
                    brackets(&p, at, s, egui::Stroke::new(2.0_f32, tint));
                    let d = 4.0;
                    p.add(egui::Shape::convex_polygon(
                        vec![
                            at + egui::vec2(0.0, -d),
                            at + egui::vec2(d, 0.0),
                            at + egui::vec2(0.0, d),
                            at + egui::vec2(-d, 0.0),
                        ],
                        tint,
                        egui::Stroke::NONE,
                    ));
                    shadowed(
                        &p,
                        at + egui::vec2(0.0, s + 6.0),
                        egui::Align2::CENTER_TOP,
                        &label,
                        14.0,
                        label_col,
                        fade(150.0),
                    );
                }
                None => {
                    let dir = off_screen_dir(view_proj, c.at);
                    let r = egui::vec2(screen.width() * 0.42, screen.height() * 0.38);
                    let tip =
                        centre + egui::vec2(dir.x * r.x, dir.y * r.y) + egui::vec2(jitter, 0.0);
                    let fwd = egui::vec2(dir.x, dir.y);
                    let side = egui::vec2(-fwd.y, fwd.x);
                    let size = 14.0 * pulse;
                    p.add(egui::Shape::convex_polygon(
                        vec![
                            tip + fwd * size,
                            tip - fwd * size * 0.6 + side * size * 0.8,
                            tip - fwd * size * 0.6 - side * size * 0.8,
                        ],
                        tint,
                        egui::Stroke::NONE,
                    ));
                    let at = tip - fwd * (size + 14.0);
                    shadowed(
                        &p,
                        at,
                        egui::Align2::CENTER_CENTER,
                        &label,
                        12.0,
                        label_col,
                        fade(150.0),
                    );
                }
            }
        }
    }
}

/// The hologram over the eyes: amber light that opens from the eye line
/// (`params[0].x` 0..1) with a bright edge, scan lines running up it, a
/// brighter rim top and bottom and where you look along it.
/// `params[0].y`: flicker (1 steady), `.z`: glow.
pub const HOLO_VISOR: &str = r#"
fn material(m: MaterialIn, base: MaterialOut) -> MaterialOut {
    var o = base;
    let q = m.object_pos;
    let open = m.params[0].x;
    let y = abs(q.y) / 0.026;
    let shown = 1.0 - smoothstep(open - 0.03, open, y);
    let edge = smoothstep(0.14, 0.0, abs(y - open)) * (1.0 - 0.6 * open);
    let rim = smoothstep(0.78, 0.96, y) * shown;
    let scan = 0.5 + 0.5 * sin(q.y * 700.0 - m.time * 18.0);
    let n = normalize(m.normal);
    let facing = clamp(abs(dot(n, m.view_dir)), 0.0, 1.0);
    let fres = pow(1.0 - facing, 2.0);
    let flick = m.params[0].y;
    let amber = vec3<f32>(1.0, 0.6, 0.16);
    // About 20% opaque once settled (Arnd); only the opening edge flares.
    // The band is seen through twice (its near and far side, both faces
    // drawn), so each layer is about half of that.
    let body = shown * clamp(0.07 + 0.03 * rim + 0.01 * scan + 0.02 * fres, 0.0, 0.11) * flick;
    let a = body + edge * 0.4;
    o.color = vec4<f32>(amber, clamp(a, 0.0, 0.5));
    o.roughness = 0.1;
    o.metallic = 0.0;
    o.emission = amber * (shown * (0.03 + 0.08 * rim + 0.02 * scan) * flick + edge * 1.5) * m.params[0].z;
    return o;
}
"#;

/// The hologram material, ready to add.
pub fn holo_material() -> CustomMaterial {
    CustomMaterial::new("police visor", HOLO_VISOR)
        .see_through()
        .param(0, [1.0, 1.0, 1.0, 0.0])
}

/// The band over the eyes: a slice of an upright, slightly barrel-shaped
/// ring wrapped round the front of the face from temple to temple. Origin at
/// the eye line between the eyes' centres of turn, +Z ahead (model units:
/// metres for a hero of scale 1).
pub fn holo_mesh() -> Mesh {
    const ACROSS: usize = 28;
    const UP: usize = 6;
    let (rx, rz) = (0.105_f32, 0.115_f32);
    let half = 0.026_f32;
    let wrap = 1.75_f32; // radians either side of straight ahead
    let mut m = Mesh::new();
    let mut rows: Vec<Vec<PointId>> = Vec::new();
    for j in 0..=UP {
        let v = j as f32 / UP as f32 * 2.0 - 1.0;
        let y = v * half;
        // Barrel: a little fuller at the eye line.
        let bulge = 1.0 + 0.06 * (1.0 - v * v);
        let row = (0..=ACROSS)
            .map(|i| {
                let a = (i as f32 / ACROSS as f32 * 2.0 - 1.0) * wrap;
                let p = Vec3::new(a.sin() * rx * bulge, y, a.cos() * rz * bulge - 0.04);
                let n = Vec3::new(a.sin() / rx, -v * 0.3, a.cos() / rz).normalize();
                let id = m.add_point(p).expect("finite point");
                m.set_point_color(id, Color::rgba(255, 170, 60, 120))
                    .expect("point exists");
                m.set_point_normal(id, n).expect("point exists");
                id
            })
            .collect();
        rows.push(row);
    }
    for w in rows.windows(2) {
        for i in 0..ACROSS {
            // Both faces, so it shows from behind the hero too.
            let (a, b, c, d) = (w[0][i], w[0][i + 1], w[1][i], w[1][i + 1]);
            m.add_triangle(a, b, d).expect("tri");
            m.add_triangle(a, d, c).expect("tri");
            m.add_triangle(a, d, b).expect("tri");
            m.add_triangle(a, c, d).expect("tri");
        }
    }
    m.set_surface_material(SurfaceMaterial {
        roughness: 0.1,
        emission: 0.0,
        ..SurfaceMaterial::DEFAULT
    });
    m
}

/// Where the eye band goes on an animated hero: from the head bone, turned
/// as the body is (and as far as the head nods), at the body's scale.
fn holo_place(world: &World, hero: EntityId) -> Option<(Vec3, Place)> {
    let head = world.bone_transform(hero, "head").ok()?;
    let body = world.world_transform(hero).ok()?;
    let (scale, body_rot, _) = body.to_scale_rotation_translation();
    let s = scale.y.max(0.01);
    let (_, head_rot, head_pos) = head.to_scale_rotation_translation();
    // The head bone's own frame differs per rig; take only how far it has
    // nodded or turned from the body's (its up against the body's up).
    let up_body = body_rot * Vec3::Y;
    let up_head = (head_rot * HEAD_UP).normalize_or_zero();
    let nod = if up_head == Vec3::ZERO {
        Quat::IDENTITY
    } else {
        Quat::from_rotation_arc(up_body, up_head)
    };
    let rot = nod * body_rot;
    let at = head_pos + rot * (EYES * s);
    let (yaw, pitch, roll) = rot.to_euler(EulerRot::YXZ);
    Some((
        at,
        Place::new(at.x, at.y, at.z)
            .with_yaw_deg(yaw.to_degrees())
            .with_pitch_deg(pitch.to_degrees())
            .with_roll_deg(roll.to_degrees())
            .with_scale(s),
    ))
}

/// The eye line from the head bone, in the body's frame (m at scale 1).
const EYES: Vec3 = Vec3::new(0.0, 0.05, 0.07);
/// The head bone's axis that points up the head.
const HEAD_UP: Vec3 = Vec3::Y;

/// How far the hologram is along: the line, then the screen opening out.
struct Opening {
    /// Width of the eye-height line, 0..1 of the screen.
    line: f32,
    /// How bright the line is, 0..1.
    line_glow: f32,
    /// Height of the open screen, 0..1.
    height: f32,
    /// How solid the screen is (flickers while it forms), 0..1.
    alpha: f32,
    /// Sideways shake of its marks while it forms (px).
    jitter: f32,
}

impl Opening {
    /// `shown` 0..1, `rising` whether it is coming or going, `time` for
    /// the flicker.
    fn at(shown: f32, rising: bool, time: f32) -> Opening {
        // A quarter of the way: the line grows; then the screen opens.
        let line_part = 0.25;
        let line = (shown / line_part).min(1.0);
        let rest = ((shown - line_part) / (1.0 - line_part)).clamp(0.0, 1.0);
        let height = ease(rest);
        // The flicker: strong while forming, gone once settled.
        let unsettled = 1.0 - rest;
        let n = flicker(time);
        let alpha = (rest * (1.0 - unsettled * 0.6 * n)).clamp(0.0, 1.0);
        let jitter = if rest < 1.0 {
            (n - 0.5) * 10.0 * unsettled
        } else {
            0.0
        };
        // The line burns bright while the screen opens and fades once it is
        // full; going out it flares as the screen collapses into it.
        let line_glow = if rising {
            (1.0 - rest * 0.85) * line
        } else {
            (1.0 - rest * 0.6) * line
        };
        Opening {
            line: if rest >= 1.0 { 0.0 } else { line },
            line_glow,
            height,
            alpha,
            jitter,
        }
    }
}

fn ease(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// A jumpy 0..1 value, new 30 times a second.
fn flicker(time: f32) -> f32 {
    let k = (time * 30.0) as u32;
    let mut x = k.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65535.0
}

fn amber(alpha: u8) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(AMBER.0, AMBER.1, AMBER.2, alpha)
}

/// The glass darkens and warms toward its rim.
fn vignette(p: &egui::Painter, screen: egui::Rect, alpha: u8) {
    let inner = screen.shrink2(egui::vec2(screen.width() * 0.16, screen.height() * 0.16));
    let edge = egui::Color32::from_rgba_unmultiplied(40, 26, 8, alpha);
    let clear = egui::Color32::TRANSPARENT;
    let mut m = egui::Mesh::default();
    let outer = [
        screen.left_top(),
        screen.right_top(),
        screen.right_bottom(),
        screen.left_bottom(),
    ];
    let inner_pts = [
        inner.left_top(),
        inner.right_top(),
        inner.right_bottom(),
        inner.left_bottom(),
    ];
    for i in 0..4 {
        m.colored_vertex(outer[i], edge);
        m.colored_vertex(inner_pts[i], clear);
    }
    for i in 0..4u32 {
        let j = (i + 1) % 4;
        let (o0, i0, o1, i1) = (i * 2, i * 2 + 1, j * 2, j * 2 + 1);
        m.add_triangle(o0, o1, i1);
        m.add_triangle(o0, i1, i0);
    }
    p.add(egui::Shape::mesh(m));
}

/// The City Patrol's star in a ring, see-through.
fn badge(p: &egui::Painter, c: egui::Pos2, r: f32, alpha: u8) {
    let col = amber(alpha);
    let star: Vec<egui::Pos2> = (0..10)
        .map(|i| {
            let a = -PI / 2.0 + PI * i as f32 / 5.0;
            let rr = if i % 2 == 0 { r * 0.78 } else { r * 0.34 };
            c + egui::vec2(a.cos() * rr, a.sin() * rr)
        })
        .collect();
    // Filled as a fan from the middle (a star is not convex).
    let mut m = egui::Mesh::default();
    let fill = amber(alpha / 2);
    m.colored_vertex(c, fill);
    for &q in &star {
        m.colored_vertex(q, fill);
    }
    for i in 0..10u32 {
        m.add_triangle(0, 1 + i, 1 + (i + 1) % 10);
    }
    p.add(egui::Shape::mesh(m));
    p.add(egui::Shape::closed_line(
        star,
        egui::Stroke::new(1.5_f32, col),
    ));
    p.circle_stroke(c, r, egui::Stroke::new(2.0_f32, col));
    p.circle_stroke(c, r * 1.12, egui::Stroke::new(1.0_f32, col));
    p.text(
        c + egui::vec2(0.0, r * 1.32),
        egui::Align2::CENTER_TOP,
        "METRO CITY PATROL",
        egui::FontId::monospace(12.0),
        col,
    );
}

/// Four corner brackets round `at`.
fn brackets(p: &egui::Painter, at: egui::Pos2, s: f32, stroke: egui::Stroke) {
    let l = s * 0.45;
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let c = at + egui::vec2(sx * s, sy * s);
        p.line_segment([c, c - egui::vec2(sx * l, 0.0)], stroke);
        p.line_segment([c, c - egui::vec2(0.0, sy * l)], stroke);
    }
}

/// Text with a soft dark copy under it, so it reads over a bright sky.
fn shadowed(
    p: &egui::Painter,
    at: egui::Pos2,
    align: egui::Align2,
    text: &str,
    size: f32,
    col: egui::Color32,
    shade: u8,
) {
    let font = egui::FontId::monospace(size);
    p.text(
        at + egui::vec2(1.0, 1.0),
        align,
        text,
        font.clone(),
        egui::Color32::from_black_alpha(shade),
    );
    p.text(at, align, text, font, col);
}

/// Where a point shows on screen, if it is in front and inside the visor.
fn on_screen(view_proj: Mat4, p: Vec3, screen: egui::Rect) -> Option<egui::Pos2> {
    let c = view_proj * p.extend(1.0);
    if c.w <= 0.05 {
        return None;
    }
    let n = c.truncate() / c.w;
    if n.x.abs() > 0.86 || n.y.abs() > 0.8 {
        return None;
    }
    Some(egui::pos2(
        screen.left() + (n.x * 0.5 + 0.5) * screen.width(),
        screen.top() + (0.5 - n.y * 0.5) * screen.height(),
    ))
}

/// Which way to point at something out of sight: a unit vector in screen
/// terms (x right, y down). Clip x and y before the divide are the camera's
/// own right and up scaled, with the right signs in front and behind (the
/// divide by a negative w is what mirrors). Something behind gets an arrow
/// at the side to turn to, tipped a little down, never up: up and down mean
/// little for a thing at your back, and an arrow at the top read as "in the
/// sky".
fn off_screen_dir(view_proj: Mat4, p: Vec3) -> Vec2 {
    let c = view_proj * p.extend(1.0);
    if c.w <= 0.05 {
        let side = if c.x < 0.0 { -1.0 } else { 1.0 };
        return Vec2::new(side, 0.35).normalize();
    }
    let v = Vec2::new(c.x, -c.y);
    if v.length_squared() < 1e-9 {
        return Vec2::Y;
    }
    v.normalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A camera at the origin looking down +Z, 60 degrees wide.
    fn camera() -> Mat4 {
        let view = glam::camera::rh::view::look_at_mat4(Vec3::ZERO, Vec3::Z, Vec3::Y);
        glam::camera::rh::proj::directx::perspective(1.0, 16.0 / 9.0, 0.1, 1000.0) * view
    }

    #[test]
    fn calls_behind_point_to_the_side_never_up() {
        let vp = camera();
        // Behind and a little to the left, on the ground below the eye: the
        // bug drew an arrow at the top of the screen, in the sky.
        let behind_left = off_screen_dir(vp, Vec3::new(30.0, -5.0, -200.0));
        let behind_right = off_screen_dir(vp, Vec3::new(-30.0, -5.0, -200.0));
        assert!(behind_left.y >= 0.0 && behind_right.y >= 0.0);
        assert!(behind_left.x * behind_right.x < 0.0, "opposite sides");
        // The camera looks down +Z with +Y up, so its right is -X.
        assert!(behind_right.x > 0.0);
        // Ahead but far to the right: an arrow on the right.
        let right = off_screen_dir(vp, Vec3::new(-500.0, 0.0, 50.0));
        assert!(right.x > 0.9);
        // Ahead and below the view: down.
        let below = off_screen_dir(vp, Vec3::new(0.0, -500.0, 50.0));
        assert!(below.y > 0.9);
    }

    #[test]
    #[ignore = "on demand (looks): scripts/check.sh --on-demand"]
    fn it_forms_and_dissolves_through_a_line() {
        // Just started: only the line.
        let o = Opening::at(0.1, true, 0.0);
        assert!(o.line > 0.0 && o.height == 0.0);
        // Fully there: no line, full and steady.
        let o = Opening::at(1.0, true, 0.37);
        assert_eq!(o.line, 0.0);
        assert_eq!(o.height, 1.0);
        assert_eq!(o.alpha, 1.0);
        assert_eq!(o.jitter, 0.0);
        // Half way out: the screen is closing onto the line.
        let o = Opening::at(0.5, false, 0.2);
        assert!(o.height > 0.0 && o.height < 1.0 && o.line > 0.0);
    }

    #[test]
    fn t_switches_it_on_and_off_and_nothing_else_does() {
        let radio = Radio::default();
        let mut v = Visor::default();
        assert_eq!(v.update(0.1, false, &radio), None);
        assert_eq!(v.update(0.1, true, &radio), Some(Why::On));
        assert!(v.take_tap());
        for _ in 0..20 {
            v.update(0.1, false, &radio);
        }
        assert_eq!(v.shown, 1.0);
        // It stays on however long nobody touches it.
        for _ in 0..1000 {
            v.update(0.1, false, &radio);
        }
        assert_eq!(v.shown, 1.0);
        assert_eq!(v.update(0.1, true, &radio), None);
        assert!(v.take_tap());
        for _ in 0..20 {
            v.update(0.1, false, &radio);
        }
        assert_eq!(v.shown, 0.0);
    }
}
