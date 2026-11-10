//! What makes the mad scientist's quarter look dangerous, building by
//! building: smokestacks on the roofs, searchlights that sweep the sky,
//! pipes glowing with his green Aether up the walls, loudspeakers on
//! brackets and his posters on every pier.
//!
//! Pure data from a building's site (no plan needed, so a view can ask for
//! many buildings cheaply). Building coordinates, like
//! [`crate::building::BuildingPlan`]: x along the front, y up from the
//! building's ground, z inwards; the front wall is z = 0 and its outside
//! is z < 0. [`Fixtures::to_world`] places a point in the town.
//!
//! Only the villains' own buildings carry them (labs and tenements), so
//! the quarter stands out from the town around it, by day and at night.

use crate::building::{bay_lines, BuildingSite, BuildingUse, GROUND_FLOOR_HEIGHT};
use crate::geom::Rect;
use glam::{DVec2, DVec3};

/// How many poster images there are to pick from.
pub const POSTER_IMAGES: u8 = 4;
/// How far a pipe's middle stands out of the wall (m).
pub const PIPE_OUT: f64 = 0.26;
/// A pipe's radius (m).
pub const PIPE_R: f64 = 0.11;
/// The roof slab over the top floor of a lab or tenement: its deck is where
/// things on the roof stand (m).
pub const ROOF_DECK: f64 = 0.3;
/// A lab's dome stands on a drum this tall, and spans this share of the
/// roof's narrower side (radius).
pub const DOME_DRUM: f64 = 3.3;
pub const DOME_SHARE: f64 = 0.32;
/// Over the dome's crown, the coil's top load (the ring the sparks jump
/// from) sits this high.
pub const COIL_LOAD: f64 = 4.6;

/// A brick smokestack standing on the roof.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stack {
    /// Where it stands on the roof (x, z).
    pub at: DVec2,
    /// Radius at its foot (it tapers towards the top).
    pub r: f64,
    /// The roof it stands on (y).
    pub foot: f64,
    /// How tall it rises over the roof.
    pub height: f64,
}

impl Stack {
    /// Its mouth, where the smoke comes out (building coordinates).
    pub fn mouth(&self) -> DVec3 {
        DVec3::new(self.at.x, self.foot + self.height, self.at.y)
    }

    /// The radius at the top.
    pub fn top_r(&self) -> f64 {
        self.r * 0.72
    }
}

/// A searchlight on the roof's edge that sweeps the sky over the quarter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Searchlight {
    /// Where its pedestal stands (x, z).
    pub at: DVec2,
    /// The roof it stands on (y).
    pub foot: f64,
    /// The height of the lamp's pivot.
    pub head: f64,
    /// Where in its sweep it starts (radians), and how fast it turns
    /// (radians a second, the sign says which way).
    pub phase: f64,
    pub speed: f64,
}

/// A cluster of horn loudspeakers on a bracket at a front corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Speakers {
    /// Along the front, and up.
    pub x: f64,
    pub y: f64,
    /// Which way the bracket points along the front (+1 or -1): away from
    /// the corner.
    pub side: f64,
}

/// A pipe glowing with Aether, up the front wall: out of the wall at
/// `from`, up, and back into it at `to`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pipe {
    pub x: f64,
    pub from: f64,
    pub to: f64,
}

/// One of the Doctor's posters, pasted on a pier of the front.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Poster {
    /// Its middle on the wall (x, y).
    pub x: f64,
    pub y: f64,
    /// Across (it is square).
    pub size: f64,
    /// Which image (below [`POSTER_IMAGES`]).
    pub image: u8,
    /// A little askew (radians).
    pub angle: f64,
}

/// Everything menacing on one building.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fixtures {
    /// A lab's Tesla coil: the middle of its top load.
    pub coil: Option<DVec3>,
    pub stack: Option<Stack>,
    pub light: Option<Searchlight>,
    pub speakers: Option<Speakers>,
    pub pipes: Vec<Pipe>,
    pub posters: Vec<Poster>,
}

impl Fixtures {
    /// The roof ground the fixtures take (building x, z), so rooftop gear
    /// keeps out of their way.
    pub fn roof_rects(&self) -> Vec<Rect> {
        let mut out = Vec::new();
        if let Some(s) = &self.stack {
            let r = s.r + 0.4;
            out.push(Rect::from_xz(
                s.at.x - r,
                s.at.y - r,
                s.at.x + r,
                s.at.y + r,
            ));
        }
        if let Some(l) = &self.light {
            out.push(Rect::from_xz(
                l.at.x - 1.2,
                l.at.y - 1.2,
                l.at.x + 1.2,
                l.at.y + 1.2,
            ));
        }
        out
    }

    /// A building point in the town, for a building whose ground floor
    /// stands at `base`.
    pub fn to_world(site: &BuildingSite, base: f64, local: DVec3) -> DVec3 {
        site.to_city(DVec3::new(local.x, local.y + base, local.z))
    }
}

/// Does this building carry the Doctor's fixtures at all?
pub fn carries(site: &BuildingSite) -> bool {
    matches!(site.usage, BuildingUse::Lab | BuildingUse::Tenement)
}

/// The fixtures on a building (none outside the villains' quarter).
pub fn fixtures(site: &BuildingSite) -> Fixtures {
    let mut out = Fixtures::default();
    if !carries(site) {
        return out;
    }
    let lab = site.usage == BuildingUse::Lab;
    let mut rng = site.seed.child("menace").rng();
    let (w, d) = (site.size.x, site.size.y);
    let look = site.look();
    // A taxi pad takes the whole roof.
    let padded = look.taxi_pad && w.min(d) >= 12.0;
    let roof = site.height() + ROOF_DECK;
    let middle = DVec2::new(w * 0.5, d * 0.5);
    // A lab's dome takes the middle of its roof, with the coil on top.
    let dome_r = w.min(d) * DOME_SHARE;
    let dome = if lab { dome_r + 0.9 } else { 0.0 };
    if lab {
        out.coil = Some(DVec3::new(
            middle.x,
            roof + DOME_DRUM + dome_r + COIL_LOAD,
            middle.y,
        ));
    }
    // The smokestack: in a back corner, clear of the dome.
    let stack_p = if lab { 0.6 } else { 0.14 };
    if !padded && w.min(d) >= 9.0 && rng.chance(stack_p) {
        let r = rng.range(0.95, 1.35);
        let edge = 1.0 + r;
        let left = rng.chance(0.5);
        let at = DVec2::new(if left { edge } else { w - edge }, d - edge);
        if at.distance(middle) > dome + r + 0.3 {
            out.stack = Some(Stack {
                at,
                r,
                foot: roof,
                height: if lab {
                    rng.range(18.0, 26.0)
                } else {
                    rng.range(11.0, 16.0)
                },
            });
        }
    }
    // The searchlight: on a front corner, the other side from the stack.
    if rng.chance(if lab { 0.55 } else { 0.06 }) {
        let left = out.stack.is_none_or(|s| s.at.x > w * 0.5);
        let at = DVec2::new(if left { 1.3 } else { w - 1.3 }, 1.3);
        if !padded && w >= 7.0 && at.distance(middle) > dome + 1.2 {
            let dir = if rng.chance(0.5) { 1.0 } else { -1.0 };
            out.light = Some(Searchlight {
                at,
                foot: roof,
                head: roof + 2.3,
                phase: rng.range(0.0, std::f64::consts::TAU),
                speed: dir * rng.range(0.22, 0.4),
            });
        }
    }
    // The loudspeakers: over the street at a front corner, a storey up.
    if rng.chance(if lab { 0.7 } else { 0.45 }) && site.floors >= 2 {
        let left = rng.chance(0.5);
        out.speakers = Some(Speakers {
            x: if left { 0.15 } else { w - 0.15 },
            y: GROUND_FLOOR_HEIGHT + 1.6,
            side: if left { 1.0 } else { -1.0 },
        });
    }
    // Glowing pipes up a lab's front corners (tenements: now and then one).
    let top = roof - ROOF_DECK - 0.7;
    if top > 6.0 {
        let corners = if lab {
            2
        } else if rng.chance(0.3) {
            1
        } else {
            0
        };
        let start = rng.chance(0.5);
        for k in 0..corners {
            let at_left = (k == 0) == start;
            let x = if at_left { 0.32 } else { w - 0.32 };
            // Keep clear of the loudspeakers' bracket.
            let clash = out
                .speakers
                .is_some_and(|s| (s.x - x).abs() < 1.0 && s.y > 2.6 && s.y < top);
            if !clash {
                out.pipes.push(Pipe {
                    x,
                    from: 2.8,
                    to: top,
                });
            }
        }
    }
    // Posters on the piers between the ground floor's bays.
    let lines = bay_lines(0.0, w, look.bay);
    for &l in lines.iter().skip(1).take(lines.len().saturating_sub(2)) {
        if !rng.chance(if lab { 0.35 } else { 0.5 }) {
            continue;
        }
        // A pier is the bay less its window.
        let pier = look.bay * (1.0 - look.window_share);
        let size = (pier - 0.3).clamp(0.7, 1.25);
        out.posters.push(Poster {
            x: l,
            y: rng.range(1.7, 2.3),
            size,
            image: rng.range_int(0, POSTER_IMAGES as i64 - 1) as u8,
            angle: rng.range(-0.05, 0.05),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::urban::world::TownWorld;
    use crate::urban::TownParams;
    use hw_core::Seed;

    #[test]
    #[ignore = "on demand (looks): scripts/check.sh --on-demand"]
    fn the_quarter_bristles_and_the_town_does_not() {
        let world = TownWorld::generate(Seed(1), TownParams::sample(Seed(1), 40_000.0));
        let d = world.town.dystopia.clone().expect("a villain quarter");
        let r = DVec2::splat(d.radius + 200.0);
        let (mut stacks, mut lights, mut posters, mut coils, mut others) = (0, 0, 0, 0, 0);
        for q in world.quarters_in(d.centre - r, d.centre + r) {
            for (_, b) in q.buildings_with_ids() {
                let f = fixtures(&b.site);
                if !carries(&b.site) {
                    others += usize::from(f != Fixtures::default());
                    continue;
                }
                stacks += usize::from(f.stack.is_some());
                lights += usize::from(f.light.is_some());
                posters += f.posters.len();
                coils += usize::from(f.coil.is_some());
                // Same site, same fixtures.
                assert_eq!(f, fixtures(&b.site));
                let (w, dd) = (b.site.size.x, b.site.size.y);
                for rect in f.roof_rects() {
                    assert!(
                        rect.min.x > -0.01 && rect.min.y > -0.01,
                        "off the roof: {rect:?}"
                    );
                    assert!(rect.max.x < w + 0.01 && rect.max.y < dd + 0.01);
                }
                for p in &f.posters {
                    assert!(p.x - p.size * 0.5 > 0.0 && p.x + p.size * 0.5 < w);
                    assert!(p.image < POSTER_IMAGES);
                }
            }
        }
        assert_eq!(others, 0, "fixtures outside the quarter");
        assert!(stacks >= 4, "only {stacks} smokestacks");
        assert!(lights >= 3, "only {lights} searchlights");
        assert!(posters >= 10, "only {posters} posters");
        assert!(coils >= 2, "only {coils} coils");
    }
}
