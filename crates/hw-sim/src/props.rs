//! Breakable street props: lamps, trees, traffic lights, pillars and
//! statues topple the way the blow went, cars flip onto their roofs,
//! benches, bins and shrubs are smashed. Like buildings, the server owns
//! which props are broken (and how), clients draw them so, and props nobody
//! has been near for a while are put back ([`crate::destruction::REPAIR_SECONDS`]).

use crate::collision::Obstacles;
use crate::destruction::{REPAIR_DISTANCE, REPAIR_SECONDS};
use glam::{DVec2, DVec3};
use hw_gen::urban::props::{BarricadePiece, Prop, PropKind};
use hw_gen::urban::quarter::DetailId;
use hw_gen::urban::world::TownWorld;
use hw_net::{PlayerId, ToClient};
use std::collections::BTreeMap;
use std::sync::Arc;

/// How a prop breaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Breaks {
    /// Falls over the way the blow went.
    Topples,
    /// Ends up on its roof.
    Flips,
    /// Smashed flat: only debris is left.
    Smashed,
}

/// How hard a prop is to break (Engine's strength scale, like building
/// elements) and how it breaks; `None`: it does not break (yet).
pub fn toughness(kind: PropKind) -> Option<(f32, Breaks)> {
    Some(match kind {
        PropKind::Shrub(_) => (20.0, Breaks::Smashed),
        PropKind::Hedge(_) => (60.0, Breaks::Smashed),
        PropKind::Bin => (40.0, Breaks::Smashed),
        PropKind::Bench => (120.0, Breaks::Smashed),
        PropKind::Mailbox => (90.0, Breaks::Topples),
        PropKind::Hydrant => (250.0, Breaks::Topples),
        PropKind::Lamp(_) => (200.0, Breaks::Topples),
        PropKind::TrafficLight => (260.0, Breaks::Topples),
        PropKind::Tree(..) => (320.0, Breaks::Topples),
        PropKind::AdPillar => (350.0, Breaks::Topples),
        PropKind::Car(..) => (300.0, Breaks::Flips),
        PropKind::Kiosk => (380.0, Breaks::Smashed),
        PropKind::Statue(_) => (900.0, Breaks::Topples),
        PropKind::Bollard => (700.0, Breaks::Topples),
        PropKind::Barricade(BarricadePiece::Jersey) => (600.0, Breaks::Topples),
        PropKind::Barricade(BarricadePiece::Sawhorse) => (50.0, Breaks::Smashed),
        PropKind::Barricade(BarricadePiece::Sandbags) => (180.0, Breaks::Smashed),
        PropKind::Barricade(BarricadePiece::Drum) => (60.0, Breaks::Topples),
        PropKind::Fountain => return None,
    })
}

/// A prop's trunk as a post: (radius, height) at scale 1, roughly what the
/// prop's mesh fills (a car or kiosk as a round post the size of it).
pub fn trunk(kind: PropKind) -> (f64, f64) {
    match kind {
        PropKind::Car(..) => (1.5, 1.5),
        PropKind::Kiosk => (1.6, 3.0),
        PropKind::Fountain => (3.0, 1.5),
        PropKind::Bench => (0.8, 0.9),
        PropKind::Statue(_) => (1.0, 4.5),
        PropKind::AdPillar => (0.7, 3.2),
        PropKind::Tree(..) => (0.4, 6.0),
        PropKind::Shrub(_) => (0.6, 1.0),
        PropKind::Hedge(_) => (1.2, 1.15),
        PropKind::Lamp(_) | PropKind::TrafficLight => (0.2, 5.0),
        PropKind::Bin | PropKind::Hydrant | PropKind::Bollard => (0.3, 0.9),
        PropKind::Mailbox => (0.35, 1.4),
        PropKind::Barricade(p) => {
            let (hx, hz, h) = p.block();
            (hx.max(hz), h)
        }
    }
}

/// Collateral cost of a broken prop.
fn cost(kind: PropKind) -> f64 {
    match kind {
        PropKind::Car(..) => 12_000.0,
        PropKind::Statue(_) => 20_000.0,
        PropKind::Kiosk => 6_000.0,
        PropKind::Tree(..) => 2_500.0,
        PropKind::TrafficLight | PropKind::Lamp(_) => 1_800.0,
        // The villains' own: nobody minds.
        PropKind::Barricade(_) => 0.0,
        _ => 500.0,
    }
}

/// A yaw (radians, about +y, 0 = +z) as one byte, and back.
pub fn yaw_to_byte(yaw: f64) -> u8 {
    ((yaw.rem_euclid(std::f64::consts::TAU) / std::f64::consts::TAU) * 256.0).round() as u32 as u8
}

pub fn byte_to_yaw(b: u8) -> f64 {
    b as f64 / 256.0 * std::f64::consts::TAU
}

/// One broken prop: which way it fell (a yaw byte) and when someone last
/// saw it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Broken {
    pub fell: u8,
    pub last_seen: u64,
}

/// The town's broken props, by prop id ([`hw_gen::urban::quarter::DetailId`]).
/// Only what changed is kept; the props themselves come from the town
/// where they are needed.
#[derive(Default)]
pub struct PropDamage {
    town: Option<Arc<TownWorld>>,
    pub broken: BTreeMap<u64, Broken>,
    /// Where each broken prop stands.
    at: BTreeMap<u64, DVec2>,
    news: Vec<ToClient>,
    /// The solids blocking props (barricades) stand in, kept in step.
    solids: Option<Arc<Obstacles>>,
}

impl PropDamage {
    pub fn new(town: Arc<TownWorld>) -> Self {
        Self {
            town: Some(town),
            ..Default::default()
        }
    }

    /// Keeps the boxes of blocking props in `solids` in step with breaks
    /// and repairs.
    pub fn with_solids(mut self, solids: Arc<Obstacles>) -> Self {
        self.solids = Some(solids);
        self
    }

    fn sync(&self, i: u64, broken: bool) {
        if let Some(s) = &self.solids {
            s.set_prop_broken(DetailId(i), broken);
        }
    }

    pub fn prop(&self, i: u64) -> Option<Prop> {
        self.town.as_ref()?.prop(DetailId(i))
    }

    /// Props whose foot is within the box `lo`..`hi` (x, z), by id.
    fn within(&self, lo: DVec2, hi: DVec2) -> Vec<(u64, Prop)> {
        let Some(town) = &self.town else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for q in town.quarters_in(lo, hi) {
            out.extend(
                q.props_with_ids()
                    .filter(|(_, p)| {
                        p.pos.x >= lo.x && p.pos.y >= lo.y && p.pos.x <= hi.x && p.pos.y <= hi.y
                    })
                    .map(|(i, p)| (i.0, *p)),
            );
        }
        out.sort_unstable_by_key(|e| e.0);
        out
    }

    /// Props whose foot is within `r` of `p` (x, z), by id.
    fn near(&self, p: DVec2, r: f64) -> Vec<(u64, Prop)> {
        self.within(p - DVec2::splat(r), p + DVec2::splat(r))
    }

    /// A blow at `at` going along `dir`: props within `radius` that are
    /// weaker than the power there break. Returns how many broke and their
    /// collateral cost.
    pub fn hit(
        &mut self,
        at: DVec3,
        dir: DVec3,
        radius: f32,
        power: f32,
        tick: u64,
    ) -> (usize, f64) {
        let r = radius as f64;
        let mut fresh = Vec::new();
        let mut spent = 0.0;
        for (i, p) in self.near(DVec2::new(at.x, at.z), r + 4.0) {
            if self.broken.contains_key(&i) {
                continue;
            }
            let Some((strength, _)) = toughness(p.kind) else {
                continue;
            };
            // Distance to the prop's trunk.
            let (round, tall) = trunk(p.kind);
            let y = at.y.clamp(p.y, p.y + tall * p.scale);
            let d = (DVec3::new(p.pos.x, y, p.pos.y) - at).length();
            let reach = (d - round * p.scale).max(0.0);
            if reach >= r || power as f64 * (1.0 - reach / r) < strength as f64 {
                continue;
            }
            // It falls away from the blow (or along it, if hit dead on).
            let away = DVec2::new(p.pos.x - at.x, p.pos.y - at.z);
            let along = DVec2::new(dir.x, dir.z);
            let fall = if away.length() > 0.3 { away } else { along };
            let fell = yaw_to_byte(fall.x.atan2(fall.y));
            self.broken.insert(
                i,
                Broken {
                    fell,
                    last_seen: tick,
                },
            );
            spent += cost(p.kind);
            self.at.insert(i, p.pos);
            self.sync(i, true);
            fresh.push((i, fell));
        }
        for chunk in fresh.chunks(hw_net::protocol::MAX_DAMAGE_IDS) {
            self.news.push(ToClient::PropsBroken {
                props: chunk.to_vec(),
                live: true,
            });
        }
        (fresh.len(), spent)
    }

    /// The first standing prop along a ray (from `from` along unit `dir`, at
    /// most `max` m): where the ray meets its trunk.
    pub fn raycast(&self, from: DVec3, dir: DVec3, max: f64) -> Option<DVec3> {
        self.town.as_ref()?;
        let (a, b) = (from, from + dir * max);
        let (a, b) = (DVec2::new(a.x, a.z), DVec2::new(b.x, b.z));
        let props = self.within(a.min(b) - DVec2::splat(4.0), a.max(b) + DVec2::splat(4.0));
        let mut t = 0.0;
        while t < max {
            let at = from + dir * t;
            let flat_at = DVec2::new(at.x, at.z);
            for (i, p) in props.iter().filter(|(_, p)| p.pos.distance(flat_at) < 4.0) {
                if self.broken.contains_key(i) || toughness(p.kind).is_none() {
                    continue;
                }
                let (round, tall) = trunk(p.kind);
                let flat = DVec2::new(at.x, at.z).distance(p.pos);
                if flat <= round * p.scale && at.y >= p.y && at.y <= p.y + tall * p.scale {
                    return Some(at);
                }
            }
            t += 0.25;
        }
        None
    }

    /// Takes over broken props (from the server or a save).
    pub fn apply(&mut self, props: &[(u64, u8)], tick: u64) {
        for &(i, fell) in props {
            if let Some(p) = self.prop(i) {
                self.at.insert(i, p.pos);
                self.sync(i, true);
                self.broken.insert(
                    i,
                    Broken {
                        fell,
                        last_seen: tick,
                    },
                );
            }
        }
    }

    /// Puts back props nobody has been near for [`REPAIR_SECONDS`].
    pub fn repair_unseen(&mut self, watchers: &[DVec3], tick: u64, hz: u32) {
        let due = (REPAIR_SECONDS * hz as f64) as u64;
        let mut fixed = Vec::new();
        for (i, b) in self.broken.iter_mut() {
            let p = self.at.get(i).copied().unwrap_or_default();
            if watchers
                .iter()
                .any(|w| DVec2::new(w.x, w.z).distance(p) < REPAIR_DISTANCE)
            {
                b.last_seen = tick;
            } else if tick.saturating_sub(b.last_seen) >= due {
                fixed.push(*i);
            }
        }
        for i in &fixed {
            self.broken.remove(i);
            self.at.remove(i);
            self.sync(*i, false);
        }
        for chunk in fixed.chunks(hw_net::protocol::MAX_DAMAGE_IDS) {
            self.news.push(ToClient::PropsRepaired {
                props: chunk.to_vec(),
            });
        }
    }

    /// Every broken prop as messages, for a player who joins.
    pub fn snapshot(&self) -> Vec<ToClient> {
        let all: Vec<(u64, u8)> = self.broken.iter().map(|(i, b)| (*i, b.fell)).collect();
        all.chunks(hw_net::protocol::MAX_DAMAGE_IDS)
            .map(|c| ToClient::PropsBroken {
                props: c.to_vec(),
                live: false,
            })
            .collect()
    }

    pub fn take_news(&mut self) -> Vec<ToClient> {
        std::mem::take(&mut self.news)
    }

    pub fn clear(&mut self) {
        for &i in self.broken.keys() {
            self.sync(i, false);
        }
        self.broken.clear();
        self.at.clear();
        self.news.clear();
    }
}

/// Who broke what, for the collateral tally.
pub fn charge(tally: &mut BTreeMap<PlayerId, f64>, by: Option<PlayerId>, cost: f64) {
    if let (Some(p), true) = (by, cost > 0.0) {
        *tally.entry(p).or_default() += cost;
    }
}
