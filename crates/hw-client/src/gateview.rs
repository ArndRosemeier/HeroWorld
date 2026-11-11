//! The portal power on screen (the rules are `hw_sim::gates`): after
//! MasterWorld's portal stone, and a little grander.
//!
//! Each open pair is an Engine portal: two elliptical openings, each seen
//! through as the place its twin stands in (Engine draws the view through
//! the opening's stencil). Round each opening a rim of light not of this
//! world ripples: a ragged haze of violet and sea green with a bright line
//! at the edge and tendrils crawling out of it (a see-through custom
//! material), wisps of light crawl round it shedding sparks, and a lamp in
//! its colour lights the street. From behind, or from too far away to be
//! worth drawing the view, a portal is a swirling disc of the same haze.
//! New portals open from their middles; marking home sends a beam to the
//! floor, which glows there and fades; a closing portal sucks its light
//! in; a hero stepping through glows briefly.
//!
//! The view through costs a second drawing of the scene, so it is drawn
//! only while the camera is within [`VIEW_RANGE`] of a pair.

use engine::prelude::*;
use glam::{DVec3, Vec3};
use hw_net::{GateView, PlayerId};
use hw_sim::gates::{ellipse, Pair, Side};
use std::collections::BTreeMap;

/// The view through a pair is drawn while the camera is this near one of
/// its sides (m); further away its openings show the swirling haze.
pub const VIEW_RANGE: f32 = 160.0;
/// Seconds a portal takes to open from its middle.
const OPENING: f32 = 1.8;
/// The rim reaches this far out of the opening (of its size).
const RIM_OUT: f32 = 1.5;
/// The rim reaches this far into the opening (of its size): the living
/// edge (see [`GATE_RIM`]) wanders between about 0.82 and 0.99.
const RIM_IN: f32 = 0.8;
/// Each opening's lamp: a glow on the street round it, not a wash over
/// everything seen through its twin.
const LAMP: f32 = 2.2;
const LAMP_RANGE: f32 = 7.0;
/// Wisps crawling round each rim.
const WISPS: usize = 8;

/// The light not of this world (sRGB): violet and sea green, and the rims'
/// own colours, home's green and the far side's blue (as the stone's).
pub fn violet() -> Color {
    Color::rgb(176, 96, 255)
}
pub fn teal() -> Color {
    Color::rgb(70, 240, 220)
}
fn white() -> Color {
    Color::rgb(240, 232, 255)
}
fn rim_colour(home: bool) -> Color {
    if home {
        Color::rgb(96, 245, 160)
    } else {
        Color::rgb(90, 170, 255)
    }
}

/// The rim: a living edge and the haze round it. The edge of what is seen
/// through wanders in and out round the opening, never still (as
/// MasterWorld's did): inside it the view; outside it a violet membrane
/// hides the opening's own fixed outline; on it a bright, flickering line;
/// beyond it a ragged haze with tendrils crawling off and motes drifting
/// out, between violet and sea green round the rim's own colour (the
/// vertex colour). `uv.x` goes round the rim, `uv.y` out of it, from
/// [`RIM_IN`] of the opening's size (0) to [`RIM_OUT`] (1).
pub const GATE_RIM: &str = r#"
fn material(m: MaterialIn, base: MaterialOut) -> MaterialOut {
    var o = base;
    let a = m.uv.x * 6.2831853;
    let rr = mix(0.8, 1.5, m.uv.y);
    let t = m.time;
    // Where the view ends: wandering round the opening, 0.82 to 0.99.
    let n = 0.45 * sin(a * 4.0 + t * 1.1) + 0.3 * sin(a * 9.0 - t * 1.9)
        + 0.17 * sin(a * 17.0 + t * 3.1) + 0.08 * sin(a * 31.0 - t * 4.7);
    let e = 0.905 + 0.085 * n;
    let d = rr - e;
    // Feathered into the view; the membrane covers the rest of the opening.
    let feather = smoothstep(-0.035, 0.0, d);
    let membrane = 1.0 - smoothstep(1.0, 1.07, rr);
    // The bright line on the living edge, flickering.
    let line = exp(-pow(d * 30.0, 2.0)) * (0.7 + 0.3 * sin(a * 37.0 + t * 11.0));
    // The haze beyond, reaching out raggedly.
    let rag = 0.5 + 0.2 * sin(a * 5.0 + t * 1.3) + 0.14 * sin(a * 11.0 - t * 2.1)
        + 0.09 * sin(a * 23.0 + t * 3.7);
    let reach = 0.14 + 0.38 * rag;
    let haze = pow(clamp(1.0 - max(d, 0.0) / reach, 0.0, 1.0), 1.6);
    // Tendrils of light crawling outward from the edge.
    let bend = sin(d * 9.0 - t * 2.4 + a * 2.0) * 0.9;
    let tendril = pow(abs(sin(a * 13.0 + bend)), 48.0) * smoothstep(reach * 1.4, 0.03, d)
        * step(0.0, d);
    // Motes drifting out through the haze.
    let cell = floor(a * 4.7746);
    let mote_a = fract(a * 4.7746) - 0.5;
    let mote_r = fract(t * 0.35 + sin(cell * 3.1) * 0.5) * 0.6;
    let mote = smoothstep(0.08, 0.0, length(vec2<f32>(mote_a * 0.6, (d - mote_r) * 2.5)));
    let swirl = 0.5 + 0.5 * sin(a * 3.0 - t * 0.8 + rr * 7.0);
    let hue = mix(vec3<f32>(0.42, 0.13, 1.0), vec3<f32>(0.12, 1.0, 0.95), swirl);
    let c = mix(hue, base.color.rgb, 0.45);
    // The membrane: deep violet, glowing more towards the edge.
    let deep = mix(vec3<f32>(0.05, 0.02, 0.12), c * 0.5, exp(-max(d, 0.0) * 12.0));
    let cover = max(membrane * 0.97, haze * 0.6);
    let glow = haze * 1.4 + line * 7.0 + tendril * 3.5 + mote * 4.0 + membrane * 0.6;
    o.color = vec4<f32>(mix(c * 0.12, deep, membrane),
        feather * clamp(cover + line + tendril * 0.8 + mote, 0.0, 1.0));
    o.roughness = 1.0;
    o.metallic = 0.0;
    o.emission = c * glow * feather;
    return o;
}
"#;

/// The haze a portal shows from behind, or when its view is not drawn:
/// slow spiral swirls of the rim's colour in the dark, brighter towards
/// the edge, motes winking in it. `uv` is the ellipse's own coordinates
/// (its edge at radius 1).
pub const GATE_HAZE: &str = r#"
fn material(m: MaterialIn, base: MaterialOut) -> MaterialOut {
    var o = base;
    let q = m.uv;
    let r = length(q);
    let a = atan2(q.y, q.x);
    let t = m.time;
    let s1 = 0.5 + 0.5 * sin(a * 3.0 + r * 9.0 - t * 1.1);
    let s2 = 0.5 + 0.5 * sin(a * 5.0 - r * 13.0 + t * 0.7);
    let swirl = s1 * s2;
    let hue = mix(vec3<f32>(0.42, 0.13, 1.0), vec3<f32>(0.12, 1.0, 0.95), 0.5 + 0.5 * sin(a + t * 0.3));
    let tint = mix(hue, base.color.rgb, 0.5);
    let deep = vec3<f32>(0.02, 0.012, 0.045);
    let edge = smoothstep(0.55, 1.0, r);
    let cell = floor(q * 9.0);
    let wink = fract(sin(dot(cell, vec2<f32>(12.9898, 78.233))) * 43758.5453);
    let mote = smoothstep(0.12, 0.0, length(fract(q * 9.0) - 0.5)) * step(0.88, wink)
        * (0.5 + 0.5 * sin(t * 3.0 + wink * 40.0));
    o.color = vec4<f32>(deep, 1.0);
    o.roughness = 1.0;
    o.metallic = 0.0;
    o.emission = tint * (swirl * 0.9 + edge * 1.6) + tint * mote * 3.0 + deep;
    return o;
}
"#;

/// One side of an open pair on screen.
struct ShownSide {
    side: Side,
    home: bool,
    opening: EntityId,
    /// The same opening facing the other way: a portal is seen through
    /// (and walked through) from both sides.
    back_opening: EntityId,
    rim: EntityId,
    haze: EntityId,
    lamp: Option<LampId>,
    wisps: Vec<EmitterId>,
    /// How far open (0..1): opens from its middle.
    open: f32,
}

struct Shown {
    pair: Pair,
    sides: [ShownSide; 2],
    /// Front to front and back to back.
    portals: Vec<PortalId>,
    viewing: bool,
}

/// Every open pair on screen.
pub struct Gates {
    shown: BTreeMap<PlayerId, Shown>,
    /// The pairs as the server last sent them (the hero camera looks
    /// through them too).
    pairs: Vec<Pair>,
    materials: Option<(CustomMaterialId, CustomMaterialId)>,
    /// The materials were tried (made once; a failure is not retried).
    tried: bool,
    t: f32,
    /// Lamps flashing up where something happened: (lamp, colour, peak,
    /// seconds left, seconds in all).
    flashes: Vec<(LampId, Color, f32, f32, f32)>,
}

impl Default for Gates {
    fn default() -> Self {
        Self::new()
    }
}

impl Gates {
    pub fn new() -> Self {
        Self {
            shown: BTreeMap::new(),
            pairs: Vec::new(),
            materials: None,
            tried: false,
            t: 0.0,
            flashes: Vec::new(),
        }
    }

    /// Adds the rims' and hazes' materials (once, at the start, so their
    /// pipelines are built before a portal first opens).
    pub fn install(&mut self, world: &mut World) -> EngineResult<()> {
        if !self.tried {
            self.tried = true;
            let rim =
                world.add_material(CustomMaterial::new("portal rim", GATE_RIM).see_through())?;
            let haze = world.add_material(CustomMaterial::new("portal haze", GATE_HAZE))?;
            self.materials = Some((rim, haze));
        }
        Ok(())
    }

    /// The open pairs, as last heard.
    pub fn pairs(&self) -> &[Pair] {
        &self.pairs
    }

    /// The server's list of open pairs: new ones open, closed ones go.
    pub fn apply(&mut self, world: &mut World, views: &[GateView]) -> EngineResult<()> {
        self.install(world)?;
        let pairs: Vec<Pair> = views.iter().map(Pair::from_view).collect();
        let owners: Vec<PlayerId> = pairs.iter().map(|p| p.owner).collect();
        let gone: Vec<PlayerId> = self
            .shown
            .keys()
            .filter(|o| !owners.contains(o))
            .copied()
            .collect();
        for o in gone {
            if let Some(s) = self.shown.remove(&o) {
                remove(world, s);
            }
        }
        for p in &pairs {
            let old = self.shown.get(&p.owner);
            if old.is_some_and(|s| s.pair == *p) {
                continue;
            }
            // Home stays open when only the far side moved.
            let home_open = old
                .filter(|s| s.pair.home == p.home && s.pair.size == p.size)
                .map_or(0.0, |s| s.sides[0].open);
            if let Some(s) = self.shown.remove(&p.owner) {
                remove(world, s);
            }
            let shown = self.show(world, p, home_open)?;
            self.shown.insert(p.owner, shown);
        }
        self.pairs = pairs;
        Ok(())
    }

    fn show(&mut self, world: &mut World, p: &Pair, home_open: f32) -> EngineResult<Shown> {
        let home = self.side(world, p.home, p.size, true, home_open)?;
        let far = self.side(world, p.far, p.size, false, 0.0)?;
        let mut portals = Vec::new();
        for (a, b) in [
            (home.opening, far.opening),
            (home.back_opening, far.back_opening),
        ] {
            if let Ok(id) = world.create_portal(a, b, PortalSettings::VIEW_ONLY) {
                world.set_portal_enabled(id, false)?;
                portals.push(id);
            }
        }
        Ok(Shown {
            pair: *p,
            sides: [home, far],
            portals,
            viewing: false,
        })
    }

    fn side(
        &mut self,
        world: &mut World,
        s: Side,
        size: f64,
        home: bool,
        open: f32,
    ) -> EngineResult<ShownSide> {
        let (_, rx, ry) = ellipse(size);
        let (rx, ry) = (rx as f32, ry as f32);
        let colour = rim_colour(home);
        let place = place_of(&s, size, open);
        let opening = world.spawn(opening_mesh(rx, ry)?);
        world.set_place(opening, place)?;
        world.set_casts_shadow(opening, false)?;
        let back_opening = world.spawn(opening_mesh(rx, ry)?);
        world.set_place(back_opening, place_of(&s.back(), size, open))?;
        world.set_casts_shadow(back_opening, false)?;
        let rim = world.spawn(rim_mesh(rx, ry, colour)?);
        world.set_place(rim, place)?;
        world.set_casts_shadow(rim, false)?;
        let haze = world.spawn(haze_mesh(rx, ry, colour)?);
        world.set_place(haze, place)?;
        world.set_casts_shadow(haze, false)?;
        if let Some((rim_m, haze_m)) = self.materials {
            world.set_custom_material(rim, rim_m)?;
            world.set_custom_material(haze, haze_m)?;
        }
        let centre = middle(&s, size);
        let lamp = world
            .add_lamp(Lamp::point(centre, colour, 0.01, LAMP_RANGE).flicker(0.15))
            .ok();
        let mut wisps = Vec::with_capacity(WISPS);
        for i in 0..WISPS {
            let tint = if i % 2 == 0 { violet() } else { teal() };
            let e = ParticleEmitter::new(centre)
                .with_mode(ParticleMode::Continuous)
                .with_rate(16.0)
                .with_lifetime(0.9)
                .with_size(0.16)
                .with_size_variation(0.5)
                .with_spread(Vec3::splat(0.07))
                .with_velocity(Vec3::Y * 0.3)
                .with_velocity_jitter(Vec3::splat(0.25))
                .with_turbulence(1.4)
                .with_drag(1.2)
                .with_color(tint)
                .with_secondary_color(colour)
                .with_silhouette(if i % 3 == 0 {
                    ParticleSilhouette::RuneMote
                } else {
                    ParticleSilhouette::SoftOrb
                })
                .with_size_over_life(SizeOverLife::FadeInOut)
                .with_emissive_intensity(4.5)
                .with_seed(0x6a7e_0000 + i as u32 * 97 + home as u32);
            if let Ok(id) = world.particles_mut().start(e) {
                wisps.push(id);
            }
        }
        Ok(ShownSide {
            side: s,
            home,
            opening,
            back_opening,
            rim,
            haze,
            lamp,
            wisps,
            open,
        })
    }

    /// Every frame: portals open from their middles, wisps crawl round the
    /// rims, lamps breathe, and the view through is drawn only near the
    /// camera (`eye`).
    pub fn update(&mut self, world: &mut World, dt: f32, eye: Vec3) -> EngineResult<()> {
        self.t += dt;
        let t = self.t;
        for shown in self.shown.values_mut() {
            let size = shown.pair.size;
            let (cy, rx, ry) = ellipse(size);
            let mut near = false;
            for s in shown.sides.iter_mut() {
                if s.open < 1.0 {
                    s.open = (s.open + dt / OPENING).min(1.0);
                    let place = place_of(&s.side, size, s.open);
                    for id in [s.opening, s.rim, s.haze] {
                        world.set_place(id, place)?;
                    }
                    world.set_place(s.back_opening, place_of(&s.side.back(), size, s.open))?;
                }
                let o = smooth(s.open);
                // Wisps crawl round the rim, just outside it.
                let right = right(&s.side);
                let base = s.side.at.as_vec3() + Vec3::Y * cy as f32;
                for (i, &id) in s.wisps.iter().enumerate() {
                    let way = if i % 2 == 0 { 1.0 } else { -0.7 };
                    let a = i as f32 / WISPS as f32 * std::f32::consts::TAU + t * 0.45 * way;
                    let wob = 1.06 + 0.04 * (t * 2.7 + i as f32 * 1.9).sin();
                    let p = base
                        + right * a.cos() * rx as f32 * wob * o
                        + Vec3::Y * a.sin() * ry as f32 * wob * o;
                    let _ = world.particles_mut().set_emitter_position(id, p);
                }
                if let Some(l) = s.lamp {
                    let breathe = 0.85 + 0.15 * (t * 1.7 + s.home as u8 as f32).sin();
                    // In the opening's middle: it lights both sides.
                    let mut lamp = Lamp::point(
                        middle(&s.side, size),
                        rim_colour(s.home),
                        LAMP * o * breathe,
                        LAMP_RANGE,
                    );
                    lamp.flicker = 0.15;
                    let _ = world.set_lamp(l, lamp);
                }
                near |= middle(&s.side, size).distance(eye) < VIEW_RANGE;
            }
            let ready = shown.sides.iter().all(|s| s.open > 0.15);
            let view = near && ready;
            if view != shown.viewing {
                for &id in &shown.portals {
                    world.set_portal_enabled(id, view)?;
                }
                shown.viewing = view;
            }
        }
        for f in self.flashes.iter_mut() {
            f.3 -= dt;
            let k = (f.3 / f.4).clamp(0.0, 1.0);
            if let Some(mut lamp) = world.lamp(f.0).copied() {
                lamp.intensity = (f.2 * k * k).max(0.001);
                lamp.color = f.1;
                let _ = world.set_lamp(f.0, lamp);
            }
        }
        for f in self.flashes.iter().filter(|f| f.3 <= 0.0) {
            world.remove_lamp(f.0);
        }
        self.flashes.retain(|f| f.3 > 0.0);
        Ok(())
    }

    /// A light flaring up at `at` and fading over `secs`.
    fn flash(&mut self, world: &mut World, at: Vec3, colour: Color, peak: f32, secs: f32) {
        if self.flashes.len() >= 8 {
            return;
        }
        if let Ok(id) = world.add_lamp(Lamp::point(at, colour, peak, 12.0)) {
            self.flashes.push((id, colour, peak, secs, secs));
        }
    }

    /// Home marked: a beam of light from the hero's raised hand down to
    /// the floor at `floor`, which glows there in two turning rings and
    /// fades.
    pub fn mark(&mut self, world: &mut World, floor: Vec3) {
        let top = floor + Vec3::Y * 3.0;
        let p = world.particles_mut();
        for k in 0..14 {
            let at = top.lerp(floor, k as f32 / 13.0);
            let _ = p.burst(
                ParticleEmitter::new(at)
                    .with_mode(ParticleMode::Burst)
                    .with_burst_count(5)
                    .with_size(0.22)
                    .with_lifetime(0.55)
                    .with_spread(Vec3::new(0.04, 0.12, 0.04))
                    .with_velocity(Vec3::Y * -1.5)
                    .with_color(white())
                    .with_secondary_color(teal())
                    .with_size_over_life(SizeOverLife::Shrink)
                    .with_emissive_intensity(7.0),
            );
        }
        let _ = p.burst(
            ParticleEmitter::new(floor + Vec3::Y * 0.05)
                .with_mode(ParticleMode::Burst)
                .with_shape(ParticleShape::Ring)
                .with_burst_count(60)
                .with_size(0.07)
                .with_lifetime(1.0)
                .with_spread(Vec3::new(0.3, 0.0, 0.3))
                .with_velocity(Vec3::Y * 1.0)
                .with_velocity_jitter(Vec3::new(0.6, 0.6, 0.6))
                .with_force(ParticleForce::Radial {
                    center: floor,
                    strength: 3.0,
                })
                .with_drag(1.4)
                .with_acceleration(Vec3::Y * -1.5)
                .with_color(white())
                .with_secondary_color(teal())
                .with_silhouette(ParticleSilhouette::SparkStreak)
                .with_velocity_stretch(0.05)
                .with_emissive_intensity(6.0),
        );
        let _ = p.burst(
            ParticleEmitter::new(floor + Vec3::Y * 0.3)
                .with_mode(ParticleMode::Burst)
                .with_burst_count(1)
                .with_size(2.2)
                .with_lifetime(0.45)
                .with_spread(Vec3::ZERO)
                .with_color(teal())
                .with_size_over_life(SizeOverLife::GrowThenFade)
                .with_emissive_intensity(4.0),
        );
        for (size, tint, glow) in [
            (3.0, teal(), 3.5),
            (2.0, violet(), 4.0),
            (4.6, violet(), 1.5),
        ] {
            let ring = Decal::new(DecalKind::Power, floor + Vec3::Y * 0.2, Vec3::Y, size)
                .tint(tint)
                .emission(glow)
                .depth(1.0)
                .lifetime(4.5)
                .fade(0.2, 3.5);
            let _ = world.spawn_decal(ring);
        }
        self.flash(world, floor + Vec3::Y * 0.6, teal(), 9.0, 4.0);
    }

    /// A portal tore open at `floor` (facing `yaw`): a flash in its middle
    /// and sparks thrown off its outline.
    pub fn tear_open(&mut self, world: &mut World, floor: Vec3, yaw: f32, size: f32) {
        let s = Side {
            at: floor.as_dvec3(),
            yaw: yaw as f64,
        };
        let (cy, rx, ry) = ellipse(size as f64);
        let mid = middle(&s, size as f64);
        let p = world.particles_mut();
        let _ = p.burst(
            ParticleEmitter::new(mid)
                .with_mode(ParticleMode::Burst)
                .with_burst_count(1)
                .with_size(4.2 * size)
                .with_lifetime(0.6)
                .with_spread(Vec3::ZERO)
                .with_color(white())
                .with_size_over_life(SizeOverLife::GrowThenFade)
                .with_emissive_intensity(5.0),
        );
        let right = right(&s);
        for k in 0..16 {
            let a = k as f32 / 16.0 * std::f32::consts::TAU;
            let at = floor
                + Vec3::Y * cy as f32
                + right * a.cos() * rx as f32
                + Vec3::Y * a.sin() * ry as f32;
            let tint = if k % 2 == 0 { violet() } else { teal() };
            let out = (at - mid).normalize_or_zero();
            let _ = p.burst(
                ParticleEmitter::new(at)
                    .with_mode(ParticleMode::Burst)
                    .with_burst_count(6)
                    .with_size(0.08)
                    .with_lifetime(0.9)
                    .with_spread(Vec3::splat(0.08))
                    .with_velocity(out * 2.2)
                    .with_velocity_jitter(Vec3::splat(1.0))
                    .with_drag(1.6)
                    .with_acceleration(Vec3::Y * 0.3)
                    .with_color(white())
                    .with_secondary_color(tint)
                    .with_silhouette(ParticleSilhouette::SparkStreak)
                    .with_velocity_stretch(0.05)
                    .with_emissive_intensity(6.5),
            );
        }
        self.flash(world, mid + normal(&s) * 0.8, white(), 14.0, 1.2);
    }

    /// A portal closed at `floor`: its light sucked into its middle and a
    /// last flash.
    pub fn shut(&mut self, world: &mut World, floor: Vec3, yaw: f32) {
        let s = Side {
            at: floor.as_dvec3(),
            yaw: yaw as f64,
        };
        let (cy, rx, ry) = ellipse(1.0);
        let mid = middle(&s, 1.0);
        let right = right(&s);
        let p = world.particles_mut();
        for k in 0..12 {
            let a = k as f32 / 12.0 * std::f32::consts::TAU;
            let at = floor
                + Vec3::Y * cy as f32
                + right * a.cos() * rx as f32
                + Vec3::Y * a.sin() * ry as f32;
            let _ = p.burst(
                ParticleEmitter::new(at)
                    .with_mode(ParticleMode::Burst)
                    .with_burst_count(4)
                    .with_size(0.07)
                    .with_lifetime(0.5)
                    .with_spread(Vec3::splat(0.05))
                    .with_velocity((mid - at) * 2.0)
                    .with_color(teal())
                    .with_secondary_color(violet())
                    .with_silhouette(ParticleSilhouette::SparkStreak)
                    .with_velocity_stretch(0.06)
                    .with_emissive_intensity(6.0),
            );
        }
        let _ = p.burst(
            ParticleEmitter::new(mid)
                .with_mode(ParticleMode::Burst)
                .with_burst_count(1)
                .with_size(2.4)
                .with_lifetime(0.5)
                .with_spread(Vec3::ZERO)
                .with_color(teal())
                .with_size_over_life(SizeOverLife::Shrink)
                .with_emissive_intensity(4.0),
        );
        self.flash(world, mid, violet(), 8.0, 0.8);
    }

    /// No room (or no home): a portal starts to open at `at` and fails,
    /// its light spitting out and falling, a puff of dark.
    pub fn fizzle(&mut self, world: &mut World, at: Vec3) {
        let p = world.particles_mut();
        let _ = p.burst(
            ParticleEmitter::new(at)
                .with_mode(ParticleMode::Burst)
                .with_burst_count(70)
                .with_size(0.07)
                .with_lifetime(1.0)
                .with_spread(Vec3::splat(0.35))
                .with_velocity_jitter(Vec3::splat(2.0))
                .with_drag(0.8)
                .with_acceleration(Vec3::Y * -6.0)
                .with_color(white())
                .with_secondary_color(violet())
                .with_silhouette(ParticleSilhouette::SparkStreak)
                .with_velocity_stretch(0.05)
                .with_emissive_intensity(5.0),
        );
        let _ = p.burst(
            ParticleEmitter::new(at)
                .with_mode(ParticleMode::Burst)
                .with_burst_count(8)
                .with_size(0.6)
                .with_lifetime(1.2)
                .with_spread(Vec3::splat(0.3))
                .with_velocity(Vec3::Y * 0.3)
                .with_color(Color::rgb(30, 20, 46))
                .with_secondary_color(Color::rgb(14, 12, 20))
                .with_silhouette(ParticleSilhouette::Puff)
                .with_blend(ParticleBlend::Alpha)
                .with_size_over_life(SizeOverLife::Expand),
        );
        self.flash(world, at, violet(), 5.0, 0.5);
    }

    /// Someone stepped through and came out with their feet at `feet`: a
    /// ring of light climbs them, wisps wind up round them, sparks fall.
    pub fn cross(&mut self, world: &mut World, feet: Vec3) {
        let mid = feet + Vec3::Y * 0.95;
        let p = world.particles_mut();
        for (speed, tint, glow) in [
            (3.2, white(), 6.0),
            (2.5, teal(), 5.0),
            (1.9, violet(), 5.0),
        ] {
            let _ = p.burst(
                ParticleEmitter::new(feet + Vec3::Y * 0.05)
                    .with_mode(ParticleMode::Burst)
                    .with_shape(ParticleShape::Ring)
                    .with_burst_count(48)
                    .with_size(0.09)
                    .with_lifetime(0.65)
                    .with_spread(Vec3::new(0.55, 0.0, 0.55))
                    .with_velocity(Vec3::Y * speed)
                    .with_velocity_jitter(Vec3::ZERO)
                    .with_turbulence(0.0)
                    .with_color(tint)
                    .with_secondary_color(violet())
                    .with_size_over_life(SizeOverLife::FadeInOut)
                    .with_emissive_intensity(glow),
            );
        }
        let _ = p.burst(
            ParticleEmitter::new(mid)
                .with_mode(ParticleMode::Burst)
                .with_shape(ParticleShape::Sphere)
                .with_burst_count(40)
                .with_size(0.14)
                .with_lifetime(1.0)
                .with_spread(Vec3::new(0.45, 0.9, 0.45))
                .with_velocity(Vec3::Y * 0.9)
                .with_force(ParticleForce::Vortex {
                    axis: Vec3::Y,
                    strength: 7.0,
                })
                .with_color(teal())
                .with_secondary_color(violet())
                .with_silhouette(ParticleSilhouette::RuneMote)
                .with_emissive_intensity(4.5),
        );
        let _ = p.burst(
            ParticleEmitter::new(mid)
                .with_mode(ParticleMode::Burst)
                .with_burst_count(36)
                .with_size(0.06)
                .with_lifetime(0.9)
                .with_spread(Vec3::new(0.35, 0.6, 0.35))
                .with_velocity_jitter(Vec3::splat(0.6))
                .with_acceleration(Vec3::Y * -2.0)
                .with_color(white())
                .with_secondary_color(teal())
                .with_silhouette(ParticleSilhouette::SparkStreak)
                .with_velocity_stretch(0.04)
                .with_emissive_intensity(6.0),
        );
        let _ = p.burst(
            ParticleEmitter::new(mid)
                .with_mode(ParticleMode::Burst)
                .with_burst_count(1)
                .with_size(2.8)
                .with_lifetime(0.7)
                .with_spread(Vec3::ZERO)
                .with_color(violet())
                .with_size_over_life(SizeOverLife::GrowThenFade)
                .with_emissive_intensity(2.5),
        );
        self.flash(world, mid, teal(), 7.0, 1.4);
    }

    /// Everything off the world (leaving it).
    pub fn clear(&mut self, world: &mut World) {
        for (_, s) in std::mem::take(&mut self.shown) {
            remove(world, s);
        }
        for f in self.flashes.drain(..) {
            world.remove_lamp(f.0);
        }
        self.pairs.clear();
    }
}

fn remove(world: &mut World, s: Shown) {
    // The portal first: an opening may not go while a portal links it.
    for id in s.portals {
        let _ = world.destroy_portal(id);
    }
    for side in s.sides {
        for id in [side.opening, side.back_opening, side.rim, side.haze] {
            world.despawn(id);
        }
        if let Some(l) = side.lamp {
            world.remove_lamp(l);
        }
        world.particles_mut().stop_all(&side.wisps);
    }
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

fn normal(s: &Side) -> Vec3 {
    s.normal().as_vec3()
}

fn right(s: &Side) -> Vec3 {
    s.right().as_vec3()
}

/// The middle of a side's ellipse.
fn middle(s: &Side, size: f64) -> Vec3 {
    (s.at + DVec3::Y * ellipse(size).0).as_vec3()
}

/// Where a side's meshes stand, `open` of the way open.
fn place_of(s: &Side, size: f64, open: f32) -> Place {
    let m = middle(s, size);
    Place::new(m.x, m.y, m.z)
        .with_yaw_deg(s.yaw.to_degrees() as f32)
        .with_scale(smooth(open).max(0.02))
}

/// The opening: an ellipse in the XY plane facing +Z, its middle at the
/// origin (the stencil the view is drawn through).
fn opening_mesh(rx: f32, ry: f32) -> EngineResult<Mesh> {
    const N: usize = 72;
    let mut m = Mesh::new();
    let c = m.add_point((0.0, 0.0, 0.0))?;
    m.set_point_color(c, Color::rgb(0, 0, 0))?;
    let ring: Vec<PointId> = (0..N)
        .map(|i| {
            let a = i as f32 / N as f32 * std::f32::consts::TAU;
            let p = m.add_point((a.cos() * rx, a.sin() * ry, 0.0))?;
            m.set_point_color(p, Color::rgb(0, 0, 0))?;
            Ok(p)
        })
        .collect::<EngineResult<_>>()?;
    for i in 0..N {
        m.add_triangle(c, ring[i], ring[(i + 1) % N])?;
    }
    Ok(m)
}

/// The rim: a band round the opening from [`RIM_IN`] of its size (well
/// inside it: the living edge wanders there) to [`RIM_OUT`], seen from
/// both sides. Its faces stand 4 cm out in front of and behind the
/// opening, before the depth Engine puts back over a portal's view (its
/// opening nudged 2.5 cm out).
fn rim_mesh(rx: f32, ry: f32, colour: Color) -> EngineResult<Mesh> {
    const N: usize = 160;
    const INNER: f32 = RIM_IN;
    let mut m = Mesh::new();
    for (z, front) in [(0.04f32, true), (-0.04, false)] {
        let mut inner = Vec::with_capacity(N + 1);
        let mut outer = Vec::with_capacity(N + 1);
        for i in 0..=N {
            let u = i as f32 / N as f32;
            let a = u * std::f32::consts::TAU;
            let (c, s) = (a.cos(), a.sin());
            let pi = m.add_point((c * rx * INNER, s * ry * INNER, z))?;
            m.set_point_uv(pi, [u, 0.0])?;
            m.set_point_color(pi, colour)?;
            let po = m.add_point((c * rx * RIM_OUT, s * ry * RIM_OUT, z))?;
            m.set_point_uv(po, [u, 1.0])?;
            m.set_point_color(po, colour)?;
            inner.push(pi);
            outer.push(po);
        }
        for i in 0..N {
            if front {
                m.add_triangle(inner[i], outer[i], outer[i + 1])?;
                m.add_triangle(inner[i], outer[i + 1], inner[i + 1])?;
            } else {
                m.add_triangle(inner[i], outer[i + 1], outer[i])?;
                m.add_triangle(inner[i], inner[i + 1], outer[i + 1])?;
            }
        }
    }
    Ok(m)
}

/// The haze disc, shown where the view is not drawn (too far off): its
/// front facing +Z a hair behind the opening, its back facing -Z a hair in
/// front, so each face lies behind the opening seen from its side.
fn haze_mesh(rx: f32, ry: f32, colour: Color) -> EngineResult<Mesh> {
    const N: usize = 72;
    const RINGS: usize = 6;
    let mut m = Mesh::new();
    for (z, front) in [(-0.006f32, true), (0.006, false)] {
        let c = m.add_point((0.0, 0.0, z))?;
        m.set_point_uv(c, [0.0, 0.0])?;
        m.set_point_color(c, colour)?;
        let mut rings: Vec<Vec<PointId>> = Vec::new();
        for k in 1..=RINGS {
            let r = k as f32 / RINGS as f32;
            let ring: Vec<PointId> = (0..N)
                .map(|i| {
                    let a = i as f32 / N as f32 * std::f32::consts::TAU;
                    let (x, y) = (a.cos() * r, a.sin() * r);
                    let p = m.add_point((x * rx, y * ry, z))?;
                    m.set_point_uv(p, [x, y])?;
                    m.set_point_color(p, colour)?;
                    Ok(p)
                })
                .collect::<EngineResult<_>>()?;
            rings.push(ring);
        }
        let tri = |m: &mut Mesh, a, b, c| {
            if front {
                m.add_triangle(a, b, c)
            } else {
                m.add_triangle(a, c, b)
            }
        };
        for i in 0..N {
            let j = (i + 1) % N;
            tri(&mut m, c, rings[0][i], rings[0][j])?;
            for k in 1..RINGS {
                let (a, b) = (&rings[k - 1], &rings[k]);
                tri(&mut m, a[i], b[i], b[j])?;
                tri(&mut m, a[i], b[j], a[j])?;
            }
        }
    }
    Ok(m)
}

/// The camera's boom from the hero's head `head` back to `eye`: if it
/// passes into a portal (either face), the camera looks from beyond the other
/// side (so stepping through never cuts away). The eye to use, and how
/// far to turn the camera's yaw (degrees).
pub fn eye_through(pairs: &[Pair], head: Vec3, eye: Vec3) -> Option<(Vec3, f32)> {
    let (h, e) = (head.as_dvec3(), eye.as_dvec3());
    let mut best: Option<(f64, Side, Side)> = None;
    for p in pairs {
        for (into, out) in p.ways() {
            let (a, b) = (into.local(h), into.local(e));
            if !(a.z > 0.0 && b.z <= 0.0) {
                continue;
            }
            let t = a.z / (a.z - b.z);
            let hit = a + (b - a) * t;
            if !hw_sim::gates::inside(p.size, hit.x, hit.y, 1.0) {
                continue;
            }
            if best.is_none_or(|(bt, ..)| t < bt) {
                best = Some((t, into, out));
            }
        }
    }
    let (_, into, out) = best?;
    let l = into.local(e);
    let mapped = out.world(DVec3::new(-l.x, l.y, -l.z));
    let turn = (out.yaw - into.yaw + std::f64::consts::PI).to_degrees() as f32;
    Some((mapped.as_vec3(), turn))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_meshes_build_and_the_materials_compile() {
        let (_, rx, ry) = ellipse(1.0);
        assert!(opening_mesh(rx as f32, ry as f32).is_ok());
        assert!(rim_mesh(rx as f32, ry as f32, teal()).is_ok());
        assert!(haze_mesh(rx as f32, ry as f32, teal()).is_ok());
        CustomMaterial::new("portal rim", GATE_RIM)
            .see_through()
            .check()
            .expect("rim compiles");
        CustomMaterial::new("portal haze", GATE_HAZE)
            .check()
            .expect("haze compiles");
    }

    #[test]
    fn the_camera_behind_a_hero_just_through_looks_from_the_far_side() {
        let home = Side {
            at: DVec3::new(0.0, 0.0, 0.0),
            yaw: std::f64::consts::FRAC_PI_2,
        };
        let far = Side {
            at: DVec3::new(0.0, 0.0, 50.0),
            yaw: std::f64::consts::PI,
        };
        let p = Pair {
            owner: PlayerId(1),
            home,
            far,
            size: 1.0,
            opened: 0,
        };
        // The hero 2 m out of home (walking +X); the camera 7 m behind.
        let head = Vec3::new(2.0, 1.6, 0.0);
        let eye = Vec3::new(-5.0, 2.5, 0.0);
        let (e, turn) = eye_through(&[p], head, eye).expect("through home");
        // Behind the far portal (it faces -Z), 5 m back, looking +Z.
        assert!((e - Vec3::new(0.0, 2.5, 45.0)).length() < 1e-3, "{e}");
        assert!(((turn + 90.0).rem_euclid(360.0)).abs() < 1e-3, "{turn}");
        // Far from it, nothing changes.
        assert!(eye_through(&[p], Vec3::new(20.0, 1.6, 0.0), Vec3::new(13.0, 2.5, 0.0)).is_none());
    }
}
