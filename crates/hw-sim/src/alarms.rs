//! Trouble in the streets that bystanders notice: a mugging, a fight with
//! villains, a wall coming down. People near it run, those a little
//! further off stop and stare, and cars stop short of a fight or a
//! collapse and queue behind (see `hw_client::crowdview` and
//! [`crate::traffic::Traffic::set_blockers`]).
//!
//! Whatever starts the trouble raises an alarm where it is and ends it
//! when it is over (or refreshes it while it lasts, for a fight). Alarms
//! go to every client as [`ToClient::Alarm`], so every machine sees the
//! same people run and the same cars stop.

use glam::{DVec2, DVec3};
use hw_net::ToClient;
use std::collections::BTreeMap;

/// What kind of trouble.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AlarmKind {
    /// Someone held up in the street.
    Mugging,
    /// A shop or bank robbed.
    Robbery,
    /// A gang shouting someone off its turf.
    Warning,
    /// A fight with villains (refresh it while it lasts).
    Fight,
    /// A building or wall coming down.
    Collapse,
    /// An explosion.
    Explosion,
    /// A car crash: the street is blocked by the wrecks (refreshed while
    /// the rescue lasts).
    Crash,
    /// A police cruiser standing in the street (refreshed while it
    /// stands): cars keep off the road round it and wait.
    Police,
}

impl AlarmKind {
    pub const ALL: [AlarmKind; 8] = [
        AlarmKind::Mugging,
        AlarmKind::Robbery,
        AlarmKind::Warning,
        AlarmKind::Fight,
        AlarmKind::Collapse,
        AlarmKind::Explosion,
        AlarmKind::Crash,
        AlarmKind::Police,
    ];

    pub fn code(self) -> u8 {
        self as u8
    }

    pub fn from_code(c: u8) -> Option<Self> {
        Self::ALL.get(c as usize).copied()
    }

    /// How far people notice it (m): within it they get away, out to
    /// twice as far they stop and stare.
    pub fn radius(self) -> f64 {
        match self {
            AlarmKind::Mugging => 25.0,
            AlarmKind::Robbery => 40.0,
            AlarmKind::Warning => 20.0,
            AlarmKind::Fight => 50.0,
            AlarmKind::Collapse => 60.0,
            AlarmKind::Explosion => 70.0,
            // People keep a few steps back and look.
            AlarmKind::Crash => 8.0,
            AlarmKind::Police => 6.0,
        }
    }

    /// How far from it street cars stop (m; none for quiet trouble on the
    /// pavement, which drivers just pass).
    pub fn stops_cars(self) -> f64 {
        match self {
            AlarmKind::Mugging | AlarmKind::Robbery | AlarmKind::Warning => 0.0,
            AlarmKind::Fight => 22.0,
            AlarmKind::Collapse => 35.0,
            AlarmKind::Explosion => 35.0,
            // The wrecks fill the street: cars wait short of them.
            AlarmKind::Crash => 9.0,
            AlarmKind::Police => 9.0,
        }
    }

    /// How far round it the road itself is taken up (m; the wrecks of a
    /// crash): street cars are not on the road within it.
    pub fn blocks_road(self) -> f64 {
        match self {
            AlarmKind::Crash => 6.0,
            // The cruiser, and room for its officers to walk round it.
            AlarmKind::Police => 6.0,
            _ => 0.0,
        }
    }

    /// How long it lasts (s) unless ended sooner or refreshed.
    pub fn lasts(self) -> f64 {
        match self {
            AlarmKind::Warning => 6.0,
            AlarmKind::Police => 4.0,
            AlarmKind::Fight | AlarmKind::Crash => 10.0,
            AlarmKind::Explosion => 20.0,
            AlarmKind::Collapse => 30.0,
            AlarmKind::Mugging | AlarmKind::Robbery => 120.0,
        }
    }

    /// Whether people run from it (else they back off and watch).
    pub fn frightens(self) -> bool {
        !matches!(
            self,
            AlarmKind::Warning | AlarmKind::Crash | AlarmKind::Police
        )
    }
}

/// One alarm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Alarm {
    pub id: u32,
    pub kind: AlarmKind,
    pub at: DVec3,
    /// How far people notice it (m).
    pub radius: f64,
    /// When it ends by itself unless refreshed (ticks; server only).
    until: u64,
}

impl Alarm {
    /// Where it is on the ground plan.
    pub fn flat(&self) -> DVec2 {
        DVec2::new(self.at.x, self.at.z)
    }

    fn message(&self, live: bool) -> ToClient {
        ToClient::Alarm {
            id: self.id,
            kind: self.kind.code(),
            at: self.at.as_vec3().to_array(),
            radius: self.radius as f32,
            live,
        }
    }
}

/// The alarms now (the server's, or a client's copy of them).
#[derive(Default)]
pub struct Alarms {
    list: BTreeMap<u32, Alarm>,
    next: u32,
    news: Vec<ToClient>,
}

impl Alarms {
    /// Raises an alarm of `kind` at `at` at `tick` (`hz` ticks a second);
    /// returns its id, to refresh or end it.
    pub fn raise(&mut self, kind: AlarmKind, at: DVec3, tick: u64, hz: u32) -> u32 {
        self.next = self.next.wrapping_add(1).max(1);
        let id = self.next;
        let a = Alarm {
            id,
            kind,
            at,
            radius: kind.radius(),
            until: tick + (kind.lasts() * hz as f64) as u64,
        };
        self.news.push(a.message(true));
        self.list.insert(id, a);
        id
    }

    /// Alarm `id` goes on (where it is now, at `at`): a fight still on.
    pub fn refresh(&mut self, id: u32, at: DVec3, tick: u64, hz: u32) {
        if let Some(a) = self.list.get_mut(&id) {
            a.until = tick + (a.kind.lasts() * hz as f64) as u64;
            // Only a real move is sent.
            if a.at.distance(at) > 4.0 {
                a.at = at;
                self.news.push(a.message(true));
            }
        }
    }

    /// Raises an alarm of `kind` at `at`, or keeps up one of that kind
    /// already near (within half its radius); returns its id.
    pub fn raise_or_refresh(&mut self, kind: AlarmKind, at: DVec3, tick: u64, hz: u32) -> u32 {
        let near = self
            .list
            .values()
            .find(|a| a.kind == kind && a.at.distance(at) < kind.radius() * 0.5)
            .map(|a| a.id);
        match near {
            Some(id) => {
                self.refresh(id, at, tick, hz);
                id
            }
            None => self.raise(kind, at, tick, hz),
        }
    }

    /// Alarm `id` is over.
    pub fn end(&mut self, id: u32) {
        if let Some(a) = self.list.remove(&id) {
            self.news.push(a.message(false));
        }
    }

    /// Ends the alarms whose time is up.
    pub fn step(&mut self, tick: u64) {
        let over: Vec<u32> = self
            .list
            .values()
            .filter(|a| a.until <= tick)
            .map(|a| a.id)
            .collect();
        for id in over {
            self.end(id);
        }
    }

    /// Every alarm on now.
    pub fn active(&self) -> impl Iterator<Item = &Alarm> + '_ {
        self.list.values()
    }

    /// Where street cars stop: (where, how far round it).
    pub fn car_stops(&self) -> Vec<(DVec2, f64)> {
        self.list
            .values()
            .filter(|a| a.kind.stops_cars() > 0.0)
            .map(|a| (a.flat(), a.kind.stops_cars()))
            .collect()
    }

    /// Where the road is taken up, and how far round (for
    /// [`crate::traffic::Traffic::set_blocked`]).
    pub fn road_blocks(&self) -> Vec<(DVec2, f64)> {
        self.list
            .values()
            .filter(|a| a.kind.blocks_road() > 0.0)
            .map(|a| (a.flat(), a.kind.blocks_road()))
            .collect()
    }

    /// Every alarm as messages, for a player who joins.
    pub fn snapshot(&self) -> Vec<ToClient> {
        self.list.values().map(|a| a.message(true)).collect()
    }

    pub fn take_news(&mut self) -> Vec<ToClient> {
        std::mem::take(&mut self.news)
    }

    /// A client takes an alarm from the server (raised, moved or ended).
    pub fn apply(&mut self, msg: &ToClient) {
        let ToClient::Alarm {
            id,
            kind,
            at,
            radius,
            live,
        } = *msg
        else {
            return;
        };
        let Some(kind) = AlarmKind::from_code(kind) else {
            return;
        };
        if !live {
            self.list.remove(&id);
            return;
        }
        self.list.insert(
            id,
            Alarm {
                id,
                kind,
                at: glam::Vec3::from_array(at).as_dvec3(),
                radius: radius as f64,
                until: u64::MAX,
            },
        );
    }

    /// No alarms (a save is loaded, another town comes).
    pub fn clear(&mut self) {
        self.list.clear();
        self.news.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn alarms_are_raised_kept_up_and_ended_on_every_machine() {
        let mut server = Alarms::default();
        let mut client = Alarms::default();
        let fight = server.raise(AlarmKind::Fight, DVec3::new(10.0, 0.0, 5.0), 0, 30);
        let mug = server.raise(AlarmKind::Mugging, DVec3::ZERO, 0, 30);
        for m in server.take_news() {
            client.apply(&m);
        }
        assert_eq!(client.active().count(), 2);
        assert_eq!(client.car_stops().len(), 1, "only the fight stops cars");
        // Kept up for a while, then let go: it ends by itself.
        for tick in (0..600).step_by(30) {
            server.refresh(fight, DVec3::new(10.0, 0.0, 5.0), tick, 30);
            server.step(tick);
        }
        assert_eq!(server.active().count(), 2);
        server.step(600 + 10 * 30);
        assert!(server.active().all(|a| a.id == mug));
        server.end(mug);
        for m in server.take_news() {
            client.apply(&m);
        }
        assert_eq!(client.active().count(), 0);
        for k in AlarmKind::ALL {
            assert_eq!(AlarmKind::from_code(k.code()), Some(k));
        }
    }
}
