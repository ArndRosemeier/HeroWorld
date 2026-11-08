//! Where the game state comes from: a server inside this process (single
//! player, optionally open to friends) or a connection to someone else's,
//! smoothed by prediction (our hero) and interpolation (everyone else).

use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_net::gamehub::{Hub, HubClientLink, HubServerLink};
use hw_net::{ConnState, Connection, EntityState, InputState, PlayerId, ToClient};
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
        /// World news (damage, impacts) not yet taken.
        news: Vec<ToClient>,
    },
}

/// News the world shows rather than prints.
fn is_world_news(m: &ToClient) -> bool {
    matches!(
        m,
        ToClient::Damage { .. } | ToClient::Repaired { .. } | ToClient::Impact { .. }
    )
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
        // Hero level until heroes have their own (it sets what they can break).
        if let Some(level) = std::env::var("HW_LEVEL")
            .ok()
            .and_then(|v| v.parse::<u8>().ok())
        {
            host.sim.hero_level = level.clamp(1, 20);
        }
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
                            news: Vec::new(),
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
                news,
            } => {
                let was_on = matches!(conn.state, ConnState::Connected { .. });
                let seq = conn.update(input);
                for m in conn.take_news() {
                    if is_world_news(&m) {
                        news.push(m);
                    } else {
                        println!("{m:?}");
                    }
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

    /// Damage, repairs and impacts since the last call.
    pub fn take_news(&mut self) -> Vec<ToClient> {
        match self {
            Session::Local { host, .. } => host.take_local_news(),
            Session::Remote { news, .. } => std::mem::take(news),
        }
    }

    /// For screenshots (`HW_VIEW=wrecked`): wrecks two small buildings near
    /// the start, one with holes blasted into its front, one brought down.
    /// Returns where to look from and at.
    pub fn wreck_demo(&mut self, town: &GeneratedTown) -> Option<(DVec3, DVec3)> {
        use hw_sim::destruction::{hero_power, material_factor, Frame};
        let Session::Local { host, me, .. } = self else {
            return None;
        };
        let solids = host.sim.solids();
        let (spawn, _) = town.world.spawn_line();
        let mut small: Vec<(f64, usize)> = (0..town.world.buildings.len())
            .filter_map(|i| {
                let (plan, _) = solids.building(i)?;
                let ok = plan.floors.len() >= 2
                    && plan.floors.len() <= 4
                    && material_factor(&plan) <= 1.3;
                let s = &plan.site;
                let c = s.origin + s.along * s.size.x * 0.5 + s.inward * s.size.y * 0.5;
                ok.then_some((c.distance(spawn), i))
            })
            .collect();
        small.sort_by(|a, b| a.0.total_cmp(&b.0));
        let &(_, holed) = small.first()?;
        // The other one: the nearest to it.
        let centre = |i: usize| {
            let (p, _) = solids.building(i).expect("a building");
            let s = &p.site;
            s.origin + s.along * s.size.x * 0.5 + s.inward * s.size.y * 0.5
        };
        let felled = small
            .iter()
            .map(|&(_, i)| i)
            .filter(|&i| i != holed)
            .min_by(|&a, &b| {
                let d = |i| centre(i).distance(centre(holed));
                d(a).total_cmp(&d(b))
            })?;
        let power = hero_power(1);
        let tick = host.sim.tick;
        let by = Some(*me);
        // A blast into the first floor up, with a scorch mark.
        let (plan, base) = solids.building(holed)?;
        let f = Frame::of(&plan.site, base);
        if let Some(floor) = plan.floors.get(1) {
            {
                let x = plan.site.size.x * 0.3;
                let at = f.to_world(DVec3::new(x, floor.elevation + 1.8, 0.0));
                let r = host
                    .sim
                    .damage
                    .hit(&solids, at, power.blast_radius, power.blast, by, tick);
                let into =
                    f.to_world(DVec3::new(x, 0.0, 1.0)) - f.to_world(DVec3::new(x, 0.0, 0.0));
                host.sim.damage.tell(ToClient::Impact {
                    at: at.as_vec3().to_array(),
                    dir: into.normalize().as_vec3().to_array(),
                    kind: hw_net::ImpactKind::Blast,
                    broke: r.broken > 0,
                });
            }
        }
        // The other one: its whole ground floor front punched out.
        let (plan2, base2) = solids.building(felled)?;
        let g = Frame::of(&plan2.site, base2);
        for z in [0.0, plan2.site.size.y] {
            let mut x = 0.4;
            while x < plan2.site.size.x {
                let at = g.to_world(DVec3::new(x, plan2.floors[0].elevation + 1.6, z));
                host.sim
                    .damage
                    .hit(&solids, at, power.punch_radius, power.punch, by, tick);
                x += 1.0;
            }
        }
        // From the street, both in view.
        let a = centre(holed);
        let b = centre(felled);
        let mid = (a + b) * 0.5;
        let front = f.to_world(DVec3::new(plan.site.size.x * 0.5, 0.0, -1.0));
        let out = (DVec2::new(front.x, front.z) - a).normalize_or(DVec2::X);
        let span = a.distance(b).max(plan.site.size.x);
        let eye = mid + out * (span * 0.9 + 16.0);
        let look_at = DVec3::new(mid.x, base + 4.0, mid.y);
        Some((DVec3::new(eye.x, base + 7.0, eye.y), look_at))
    }

    /// Playing in someone else's world.
    pub fn joined(&self) -> bool {
        matches!(self, Session::Remote { .. })
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
