//! Where the game state comes from: a server inside this process (single
//! player, optionally open to friends) or a connection to someone else's,
//! smoothed by prediction (our hero) and interpolation (everyone else).

use glam::DVec3;
use hw_core::{NetId, Seed};
use hw_net::gamehub::{Hub, HubClientLink, HubServerLink};
use hw_net::{ConnState, Connection, EntityState, InputState, PlayerId};
use hw_sim::host::Host;
use hw_sim::predict::{Interpolator, Predictor};
use hw_sim::{GeneratedTown, Sim};
use std::time::{Duration, Instant};

pub enum Session {
    Local {
        host: Box<Host>,
        me: PlayerId,
        /// The GameHub room code friends join with, when hosting online.
        room: Option<String>,
    },
    Remote {
        conn: Box<Connection>,
        predictor: Option<Predictor>,
        interp: Interpolator,
        last_tick: u64,
    },
}

impl Session {
    /// Single player in `town`; `open` also listens for friends.
    pub fn single(
        seed: Seed,
        town: &GeneratedTown,
        name: &str,
        open: bool,
    ) -> std::io::Result<Self> {
        let mut host = Host::with_sim(Sim::for_town(seed, town));
        host.sim.place_starter_crates();
        if open {
            let addr = host.listen(("0.0.0.0", hw_net::DEFAULT_PORT))?;
            println!("Hosting on {addr}");
        }
        let mut room = None;
        if open {
            // Also list the world on GameHub, so friends anywhere can join
            // by its code through the relay; without it, LAN and UDP still work.
            let meta = serde_json::json!({
                "seed": seed.0,
                "population": town.population(),
                "version": hw_core::version(),
                "protocol": hw_net::PROTOCOL_VERSION,
            });
            let online = Hub::connect(&hw_net::gamehub::hub_url(), name).and_then(|mut hub| {
                let code = hub.host_room(&format!("{name}'s world"), meta)?;
                Ok((hub, code))
            });
            match online {
                Ok((hub, code)) => {
                    println!("Friends can join with the code {code} (HW_JOIN={code})");
                    host.add_link(Box::new(HubServerLink::new(hub)));
                    room = Some(code);
                }
                Err(e) => {
                    println!("Not listed on GameHub ({e}); friends can still join by address")
                }
            }
        }
        let me = host.join_local(name);
        Ok(Self::Local {
            host: Box::new(host),
            me,
            room,
        })
    }

    /// Connects to a host and waits (up to 5 s) for its welcome. Returns the
    /// session, the host's world seed and its town's population.
    /// `addr` is a GameHub room code (six letters and digits) or a UDP
    /// address (`host:port`).
    pub fn join(addr: &str, name: &str) -> std::io::Result<(Self, u64, f64)> {
        let is_code = addr.len() == 6 && addr.chars().all(|c| c.is_ascii_alphanumeric());
        let mut conn = if is_code {
            let mut hub = Hub::connect(&hw_net::gamehub::hub_url(), name)?;
            hub.join_room(addr)?;
            Connection::over(Box::new(HubClientLink::new(hub)), name)
        } else {
            Connection::open(addr, name)?
        };
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            conn.update(InputState::default());
            match &conn.state {
                ConnState::Connected {
                    world_seed,
                    town_population,
                    ..
                } => {
                    let (seed, people) = (*world_seed, *town_population);
                    return Ok((
                        Self::Remote {
                            conn: Box::new(conn),
                            predictor: None,
                            interp: Interpolator::new(),
                            last_tick: 0,
                        },
                        seed,
                        people,
                    ));
                }
                ConnState::Rejected(reason) => {
                    return Err(std::io::Error::other(format!(
                        "the host turned us away: {reason}"
                    )))
                }
                ConnState::Ended(reason) => {
                    return Err(std::io::Error::other(format!("the host left: {reason}")))
                }
                ConnState::Connecting => std::thread::sleep(Duration::from_millis(20)),
            }
        }
        Err(std::io::Error::other("no answer from the host"))
    }

    /// For a joined session: start predicting with the generated town.
    pub fn attach_town(&mut self, town: &GeneratedTown) {
        if let Session::Remote { predictor, .. } = self {
            *predictor = Some(Predictor::new(town.obstacles.clone()));
        }
    }

    /// One game tick with this frame's input.
    pub fn tick(&mut self, input: InputState) {
        match self {
            Session::Local { host, me, .. } => {
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
                let was_on = matches!(conn.state, ConnState::Connected { .. });
                let seq = conn.update(input);
                for news in conn.take_news() {
                    println!("{news:?}");
                }
                if let (true, ConnState::Ended(reason)) = (was_on, &conn.state) {
                    println!("Session over: {reason}");
                }
                let hero = conn.hero();
                if let Some((tick, ack, entities)) = conn.world() {
                    if tick > *last_tick {
                        *last_tick = tick;
                        if let (Some(p), Some(h)) = (predictor.as_mut(), hero) {
                            if let Some(me) = entities.iter().find(|e| e.id == h) {
                                p.reconcile(ack, me);
                            }
                        }
                        interp.push(tick, entities);
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

    /// The game clock in seconds (the server's ticks), `frac` of a tick
    /// on: citizens and anything else made from time read it, so every
    /// player sees them alike.
    pub fn time(&self, frac: f64) -> f64 {
        match self {
            Session::Local { host, .. } => (host.sim.tick as f64 + frac) * hw_sim::DT,
            Session::Remote { interp, .. } => interp.render_tick() * hw_sim::DT,
        }
    }

    /// Puts our hero somewhere (single player only; for tests and screenshots).
    pub fn teleport(&mut self, pos: DVec3) {
        if let Session::Local { host, me, .. } = self {
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

    /// The GameHub room code of a world we host online.
    pub fn room_code(&self) -> Option<&str> {
        match self {
            Session::Local { room, .. } => room.as_deref(),
            Session::Remote { .. } => None,
        }
    }

    pub fn my_hero(&mut self) -> Option<NetId> {
        match self {
            Session::Local { host, me, .. } => host.sim.hero_of(*me),
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
        match self {
            Session::Remote { conn, .. } => conn.close(),
            // Friends who joined are told the world is closing.
            Session::Local { host, .. } => {
                host.end_session("the host closed the game", Duration::from_millis(500))
            }
        }
    }
}
