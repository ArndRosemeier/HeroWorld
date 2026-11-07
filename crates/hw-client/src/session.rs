//! Where the game state comes from: a server inside this process (single
//! player, optionally open to friends) or a connection to someone else's,
//! smoothed by prediction (our hero) and interpolation (everyone else).

use glam::DVec3;
use hw_core::{NetId, Seed};
use hw_net::{ConnState, Connection, EntityState, InputState, PlayerId};
use hw_sim::host::Host;
use hw_sim::predict::{Interpolator, Predictor};
use hw_sim::{GeneratedCity, Sim};
use std::time::{Duration, Instant};

pub enum Session {
    Local {
        host: Box<Host>,
        me: PlayerId,
    },
    Remote {
        conn: Box<Connection>,
        predictor: Option<Predictor>,
        interp: Interpolator,
        last_tick: u64,
    },
}

impl Session {
    /// Single player in `city`; `open` also listens for friends.
    pub fn single(
        seed: Seed,
        city: &GeneratedCity,
        name: &str,
        open: bool,
    ) -> std::io::Result<Self> {
        let mut host = Host::with_sim(Sim::for_city(seed, city));
        if open {
            let addr = host.listen(("0.0.0.0", hw_net::DEFAULT_PORT))?;
            println!("Hosting on {addr}");
        }
        let me = host.join_local(name);
        Ok(Self::Local {
            host: Box::new(host),
            me,
        })
    }

    /// Connects to a host and waits (up to 5 s) for its welcome. Returns the
    /// session, the host's world seed and its city's radius.
    pub fn join(addr: &str, name: &str) -> std::io::Result<(Self, u64, f64)> {
        let mut conn = Connection::open(addr, name)?;
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            conn.update(InputState::default());
            match &conn.state {
                ConnState::Connected {
                    world_seed,
                    city_radius,
                    ..
                } => {
                    let (seed, radius) = (*world_seed, *city_radius);
                    return Ok((
                        Self::Remote {
                            conn: Box::new(conn),
                            predictor: None,
                            interp: Interpolator::new(),
                            last_tick: 0,
                        },
                        seed,
                        radius,
                    ));
                }
                ConnState::Rejected(reason) => {
                    return Err(std::io::Error::other(format!(
                        "the host turned us away: {reason}"
                    )))
                }
                ConnState::Connecting => std::thread::sleep(Duration::from_millis(20)),
            }
        }
        Err(std::io::Error::other("no answer from the host"))
    }

    /// For a joined session: start predicting with the generated city.
    pub fn attach_city(&mut self, city: &GeneratedCity) {
        if let Session::Remote { predictor, .. } = self {
            *predictor = Some(Predictor::new(city.obstacles.clone()));
        }
    }

    /// One game tick with this frame's input.
    pub fn tick(&mut self, input: InputState) {
        match self {
            Session::Local { host, me } => {
                host.sim.set_input(*me, input);
                for e in host.tick() {
                    println!("{e:?}");
                }
            }
            Session::Remote {
                conn,
                predictor,
                interp,
                last_tick,
            } => {
                let seq = conn.update(input);
                let hero = conn.hero();
                if let Some((tick, ack, entities)) = &conn.latest {
                    if *tick > *last_tick {
                        *last_tick = *tick;
                        interp.push(*tick, entities.clone());
                        if let (Some(p), Some(h)) = (predictor.as_mut(), hero) {
                            if let Some(me) = entities.iter().find(|e| e.id == h) {
                                p.reconcile(*ack, me);
                            }
                        }
                    }
                }
                if let (Some(p), Some(seq)) = (predictor.as_mut(), seq) {
                    p.input(seq, input);
                }
            }
        }
    }

    /// Everything to draw this frame. Our own hero comes first if present.
    pub fn entities(&mut self, frame_dt: f64) -> Vec<EntityState> {
        match self {
            Session::Local { host, .. } => host.sim.snapshot(),
            Session::Remote {
                conn,
                predictor,
                interp,
                ..
            } => {
                let hero = conn.hero();
                let mut out = interp.sample(frame_dt);
                if let (Some(p), Some(h)) = (predictor.as_mut(), hero) {
                    if p.started() {
                        let pos = p.render_pos(frame_dt);
                        if let Some(e) = out.iter_mut().find(|e| e.id == h) {
                            e.pos = pos.to_array();
                        }
                    }
                }
                out
            }
        }
    }

    /// Puts our hero somewhere (single player only; for tests and screenshots).
    pub fn teleport(&mut self, pos: DVec3) {
        if let Session::Local { host, me } = self {
            if let Some(id) = host.sim.hero_of(*me) {
                host.sim.set_body(
                    id,
                    hw_sim::collision::Body {
                        pos,
                        ..Default::default()
                    },
                );
            }
        }
    }

    pub fn my_hero(&mut self) -> Option<NetId> {
        match self {
            Session::Local { host, me } => host.sim.hero_of(*me),
            Session::Remote { conn, .. } => conn.hero(),
        }
    }

    /// Our hero's position as last drawn by [`Self::entities`].
    pub fn my_position(&mut self, drawn: &[EntityState]) -> Option<DVec3> {
        let id = self.my_hero()?;
        drawn
            .iter()
            .find(|e| e.id == id)
            .map(|e| DVec3::from_array(e.pos))
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Session::Remote { conn, .. } = self {
            conn.close();
        }
    }
}
