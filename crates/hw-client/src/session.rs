//! Where the game state comes from: a server inside this process (single
//! player, optionally open to friends) or a connection to someone else's,
//! smoothed by prediction (our hero) and interpolation (everyone else).

use glam::{DVec2, DVec3};
use hw_core::{NetId, Seed};
use hw_net::gamehub::{Hub, HubClientLink, HubServerLink};
use hw_net::upnp::PortMapping;
use hw_net::{
    ConnState, Connection, EntityState, InputState, PlayerId, PowerId, Sheet, ToClient, ToServer,
};
use hw_sim::host::Host;
use hw_sim::hostthread::HostThread;
use hw_sim::predict::{Interpolator, Predictor};
use hw_sim::{GeneratedTown, Sim};
use std::time::{Duration, Instant};

pub enum Session {
    Local {
        /// The server, on its own thread.
        host: Box<HostThread>,
        /// The GameHub room code friends join with, when hosting online.
        room: Option<String>,
        /// The port the router forwards to us (UPnP), when open to friends.
        ports: Box<Ports>,
    },
    Remote {
        conn: Box<Connection>,
        predictor: Option<Box<Predictor>>,
        interp: Interpolator,
        last_tick: u64,
        /// World news (damage, impacts) not yet taken.
        news: Vec<ToClient>,
        /// Fight events received and not yet shown.
        fx: Vec<hw_net::Fx>,
        /// Everyone's names, from the join news.
        names: std::collections::BTreeMap<PlayerId, String>,
        /// Our hero file as the host last sent it.
        sheet: Option<Box<Sheet>>,
        /// Everyone's looks.
        looks: Box<Looks>,
    },
}

/// The game's UDP port forwarded by the router (UPnP): asked for in the
/// background when the world opens, given back when the session ends.
#[derive(Default)]
pub struct Ports {
    asking: Option<std::sync::mpsc::Receiver<Result<PortMapping, String>>>,
    mapping: Option<PortMapping>,
}

impl Ports {
    fn ask(port: u16) -> Self {
        Self {
            asking: Some(hw_net::upnp::map_udp_in_background(port, "HeroWorld")),
            mapping: None,
        }
    }

    fn poll(&mut self) {
        let Some(rx) = &self.asking else { return };
        match rx.try_recv() {
            Ok(Ok(m)) => {
                println!(
                    "The router forwards port {} to this PC{}",
                    m.port,
                    m.address()
                        .map(|a| format!(": friends can join at {a}"))
                        .unwrap_or_default()
                );
                self.mapping = Some(m);
                self.asking = None;
            }
            Ok(Err(e)) => {
                println!("No automatic port forwarding ({e}); the room code still works");
                self.asking = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => self.asking = None,
        }
    }
}

/// Everyone's looks (see `hw_net::look`), and pieces of them still on
/// their way.
#[derive(Default)]
pub struct Looks {
    whole: std::collections::BTreeMap<PlayerId, Vec<u8>>,
    parts: std::collections::BTreeMap<PlayerId, hw_net::look::LookParts>,
}

/// News the world shows rather than prints.
fn is_world_news(m: &ToClient) -> bool {
    matches!(
        m,
        ToClient::Damage { .. }
            | ToClient::Repaired { .. }
            | ToClient::Doors { .. }
            | ToClient::Gates { .. }
            | ToClient::Impact { .. }
            | ToClient::PropsBroken { .. }
            | ToClient::PropsRepaired { .. }
            | ToClient::CarsWrecked { .. }
            | ToClient::CarsCleared { .. }
            | ToClient::AreaMoved { .. }
            | ToClient::Traffic { .. }
            | ToClient::Incident { .. }
            | ToClient::Casualty { .. }
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
        // No villains wait where a new hero appears: trouble is found in
        // the world. The old test group stays for the fight screenshot
        // (without the crate pile, which would stand in the way).
        if std::env::var("HW_VIEW").as_deref() == Ok("fight") {
            host.sim.place_first_fight();
        } else {
            host.sim.place_starter_crates();
        }
        // Screenshots: `HW_INCIDENT=1` starts the first incident at once
        // (`fire`, `crash`, `breakout`...: of that kind).
        if let Ok(v) = std::env::var("HW_INCIDENT") {
            use hw_sim::incidents::IncidentKind;
            host.sim.incidents.first_after = 0.0;
            host.sim.incidents.next_kind = match v.as_str() {
                "mugging" => Some(IncidentKind::Mugging),
                "robbery" => Some(IncidentKind::Robbery),
                "rampage" => Some(IncidentKind::Rampage),
                "breakout" => Some(IncidentKind::Breakout),
                "crash" => Some(IncidentKind::CarCrash),
                "fire" => Some(IncidentKind::Fire),
                "colossus" => Some(IncidentKind::Colossus),
                "moles" => Some(IncidentKind::Surfacing),
                "pirates" => Some(IncidentKind::PirateRaid),
                "iron" => Some(IncidentKind::IronLanding),
                "saucer" => Some(IncidentKind::SaucerRaid),
                _ => None,
            };
        }
        // `HW_POD=1`: our hero floats in a hospital's recovery pod.
        host.sim
            .world
            .resource_mut::<hw_sim::combat::Fights>()
            .pod_start = std::env::var_os("HW_POD").is_some();
        // `HW_TIMEOUT=<s>`: calls end unanswered this soon (and their
        // crooks make off).
        host.sim.incidents.timeout = std::env::var("HW_TIMEOUT")
            .ok()
            .and_then(|v| v.parse().ok());
        // `HW_CARRY=1`: that rescue starts with the hurt one in our arms.
        host.sim.incidents.carried_start = std::env::var_os("HW_CARRY").is_some();
        // `HW_BESIDE=1`: by the next one trapped (`HW_HOLD=1` holds E).
        host.sim.incidents.beside_start = std::env::var_os("HW_BESIDE").is_some();
        let mut ports = Ports::default();
        if open {
            let addr = host.listen(("0.0.0.0", hw_net::DEFAULT_PORT))?;
            println!("Hosting on {addr}");
            // Ask the router to let friends in by address too.
            if std::env::var_os("HW_NO_UPNP").is_none() {
                ports = Ports::ask(addr.port());
            }
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
            host: Box::new(HostThread::spawn(host, me)),
            room,
            ports: Box::new(ports),
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
                            fx: Vec::new(),
                            names: Default::default(),
                            sheet: None,
                            looks: Default::default(),
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
                ConnState::TakingOver => {
                    return Err(std::io::Error::other("the host left as we joined"))
                }
            }
        }
        Err(std::io::Error::other("no answer from the host"))
    }

    /// The host left and GameHub made us the new one (see
    /// [`Self::take_over`]).
    pub fn wants_take_over(&self) -> bool {
        matches!(self, Session::Remote { conn, .. } if conn.state == ConnState::TakingOver)
    }

    /// Becomes the host of a world whose host left: a server of `town`
    /// from the save the old host published, in the same GameHub room, with
    /// us as its local player. The others find it on their own. Our hero
    /// file and looks still have to be sent ([`Self::send_sheet`]).
    pub fn take_over(&mut self, seed: Seed, town: &GeneratedTown, name: &str) -> bool {
        let Session::Remote { conn, .. } = self else {
            return false;
        };
        let state = conn.world_state().map(str::to_string);
        let Some(hub) = conn.take_hub() else {
            return false;
        };
        let mut host = Host::taking_over(Sim::for_town(seed, town), state.as_deref());
        let link = HubServerLink::new(hub);
        let room = link.code().map(str::to_string);
        host.add_link(Box::new(link));
        let me = host.join_local(name);
        println!(
            "The host left: this game hosts the world now (room {})",
            room.as_deref().unwrap_or("?")
        );
        *self = Session::Local {
            host: Box::new(HostThread::spawn(host, me)),
            room,
            ports: Box::default(),
        };
        true
    }

    /// For a joined session: start predicting with the generated town.
    pub fn attach_town(&mut self, town: &GeneratedTown) {
        if let Session::Remote { predictor, .. } = self {
            *predictor = Some(Box::new(Predictor::new(town.obstacles.clone())));
        }
    }

    /// One game tick with this frame's input.
    pub fn tick(&mut self, input: InputState) {
        match self {
            Session::Local { host, ports, .. } => {
                ports.poll();
                host.tick(input);
            }
            Session::Remote {
                conn,
                predictor,
                interp,
                last_tick,
                news,
                fx,
                names,
                sheet,
                looks,
            } => {
                let was_on = matches!(conn.state, ConnState::Connected { .. });
                let handovers = conn.handovers;
                let seq = conn.update(input);
                if conn.handovers != handovers {
                    // A new host: new IDs, a fresh start for what we draw.
                    *interp = Interpolator::new();
                    *last_tick = 0;
                    if let Some(p) = predictor.as_mut() {
                        p.reset();
                    }
                    println!("The host left; another player hosts the world now");
                }
                for m in conn.take_news() {
                    if let ToClient::Fx(e) = m {
                        fx.push(e);
                    } else if let ToClient::Sheet(s) = m {
                        *sheet = Some(Box::new(s));
                    } else if let ToClient::Look {
                        player,
                        part,
                        parts,
                        bytes,
                    } = m
                    {
                        let whole = looks
                            .parts
                            .entry(player)
                            .or_default()
                            .add(part, parts, bytes);
                        if let Some(l) = whole {
                            looks.whole.insert(player, l);
                        }
                    } else if is_world_news(&m)
                        || matches!(m, ToClient::Chat { .. } | ToClient::TeamChat { .. })
                    {
                        // Portals carry our hero when the guess walks into
                        // one, as the server does.
                        if let (ToClient::Gates { gates }, Some(p)) = (&m, predictor.as_mut()) {
                            p.gates = gates.iter().map(hw_sim::gates::Pair::from_view).collect();
                        }
                        news.push(m);
                    } else {
                        if let ToClient::Joined { player, name } = &m {
                            names.insert(*player, name.clone());
                        }
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
                                // In a taxi the server moves us: no guessing.
                                p.riding = hw_sim::taxi::riding(me, &entities)
                                    || hw_sim::tube::riding(me, &entities);
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

    /// Once a frame, after this frame's ticks: in single player, takes
    /// the world as the server thread left it after the frame before, while
    /// it works through this frame's ticks (see `hw_sim::hostthread`).
    /// Returns how long it waited for the server.
    pub fn sync(&mut self) -> Duration {
        match self {
            Session::Local { host, .. } => host.sync(),
            Session::Remote { .. } => Duration::ZERO,
        }
    }

    /// Server time spent on ticks and the ticks run, as of the last
    /// [`Self::sync`] (single player).
    pub fn server_load(&self) -> Option<(f64, u32)> {
        match self {
            Session::Local { host, .. } => Some((host.view().busy, host.view().ticks)),
            Session::Remote { .. } => None,
        }
    }

    /// Where the server's time went since the last [`Self::sync`]
    /// (single player), in microseconds.
    pub fn server_parts(&self) -> &[(&'static str, u64)] {
        match self {
            Session::Local { host, .. } => &host.view().parts,
            Session::Remote { .. } => &[],
        }
    }

    /// Ticks' parts that overran their budget since the last [`Self::sync`]
    /// (single player), in words.
    pub fn server_overruns(&self) -> &[String] {
        match self {
            Session::Local { host, .. } => &host.view().overruns,
            Session::Remote { .. } => &[],
        }
    }

    /// Everything to draw this frame. Our own hero comes first if present.
    /// `frac` is how far into the next tick the frame is
    /// ([`hw_sim::hostthread::TickClock::frac`]): in single player
    /// everyone is drawn between the server's ticks at that time, so they
    /// move on every frame.
    pub fn entities(&mut self, frame_dt: f64, frac: f64) -> Vec<EntityState> {
        match self {
            Session::Local { host, .. } => host.entities_at(frac),
            Session::Remote {
                conn,
                predictor,
                interp,
                ..
            } => {
                let hero = conn.hero();
                let mut out = interp.sample(frame_dt);
                if let (Some(p), Some(h)) = (predictor.as_mut(), hero) {
                    if p.started() && !p.riding {
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

    /// Fight events since the last call, with the server tick they are
    /// drawn at now (to drop ones that come too late to show).
    /// What the server did with our power keys, in words (single player;
    /// a friend's server keeps its own).
    pub fn take_server_log(&mut self) -> Vec<String> {
        match self {
            Session::Local { host, .. } => host.take_log(),
            Session::Remote { .. } => Vec::new(),
        }
    }

    pub fn take_fx(&mut self) -> (Vec<hw_net::Fx>, f64) {
        match self {
            Session::Local { host, .. } => (host.take_fx(), host.view().tick as f64),
            Session::Remote { fx, interp, .. } => (std::mem::take(fx), interp.render_tick()),
        }
    }

    /// Plays our hero by `sheet` (the hero file; sent once on joining).
    pub fn send_sheet(&mut self, sheet: Sheet) {
        match self {
            Session::Local { host, .. } => host.post(move |h, me| h.sim.set_sheet(me, sheet)),
            Session::Remote { conn, .. } => conn.send(ToServer::Sheet(sheet)),
        }
    }

    /// Shows everyone our hero's looks (compact JSON, see `crate::looks`).
    pub fn send_look(&mut self, look: Vec<u8>) {
        match self {
            Session::Local { host, .. } => host.post(move |h, me| h.set_look(me, look)),
            Session::Remote { conn, .. } => {
                for (part, parts, bytes) in hw_net::look::pieces(&look) {
                    conn.send(ToServer::Look { part, parts, bytes });
                }
            }
        }
    }

    /// A player's looks, once they have arrived.
    pub fn player_look(&self, p: PlayerId) -> Option<&[u8]> {
        let l = match self {
            Session::Local { host, .. } => host.view().players.get(&p).map(|i| i.look.as_slice()),
            Session::Remote { looks, .. } => looks.whole.get(&p).map(|l| l.as_slice()),
        }?;
        (!l.is_empty()).then_some(l)
    }

    /// Buys `power` in `theme` (the host checks the price).
    pub fn buy(&mut self, power: PowerId) {
        match self {
            Session::Local { host, .. } => host.post(move |h, me| {
                let _ = h.sim.buy(me, power);
            }),
            Session::Remote { conn, .. } => conn.send(ToServer::Buy { power }),
        }
    }

    /// Gives `power` up for 90% of its price.
    pub fn give_up(&mut self, power: PowerId) {
        match self {
            Session::Local { host, .. } => host.post(move |h, me| {
                let _ = h.sim.give_up(me, power);
            }),
            Session::Remote { conn, .. } => conn.send(ToServer::GiveUp { power }),
        }
    }

    /// Sets an augment from the bag into a socket, or trades one in (the
    /// host checks it).
    pub fn augment(&mut self, a: hw_client::augmentview::AugmentAction) {
        use hw_client::augmentview::AugmentAction;
        match (self, a) {
            (Session::Local { host, .. }, AugmentAction::Set { bag, power, slot }) => {
                host.post(move |h, me| {
                    let _ = h.sim.set_augment(me, bag as usize, power, slot);
                })
            }
            (Session::Local { host, .. }, AugmentAction::Trade { bag }) => {
                host.post(move |h, me| {
                    let _ = h.sim.trade_augment(me, bag as usize);
                })
            }
            (Session::Remote { conn, .. }, AugmentAction::Set { bag, power, slot }) => {
                conn.send(ToServer::SetAugment { bag, power, slot })
            }
            (Session::Remote { conn, .. }, AugmentAction::Trade { bag }) => {
                conn.send(ToServer::TradeAugment { bag })
            }
        }
    }

    /// Our hero file, if it changed since the last call.
    pub fn take_sheet(&mut self) -> Option<Sheet> {
        match self {
            Session::Local { host, .. } => host.take_sheet(),
            Session::Remote { sheet, .. } => sheet.take().map(|s| *s),
        }
    }

    /// A player's name.
    pub fn player_name(&self, p: PlayerId) -> Option<String> {
        match self {
            Session::Local { host, .. } => host.view().players.get(&p).map(|i| i.name.clone()),
            Session::Remote { names, .. } => names.get(&p).cloned(),
        }
    }

    /// The game clock in seconds (the server's ticks), `frac` of a tick
    /// on: citizens, cars and anything else made from time read it, so
    /// every player sees them alike. In single player it counts every tick
    /// sent, not the view's (a frame behind): read with the same
    /// [`hw_sim::hostthread::TickClock`], it moves on by each frame's time.
    pub fn time(&self, frac: f64) -> f64 {
        match self {
            Session::Local { host, .. } => (host.sent_tick() as f64 + frac) * hw_sim::DT,
            Session::Remote { interp, .. } => interp.render_tick() * hw_sim::DT,
        }
    }

    /// Our hero says `text` out loud (the chat window's Local channel):
    /// friends in the world hear it. Single player, nobody else does.
    pub fn say(&mut self, text: &str) {
        let text = text.to_string();
        match self {
            Session::Local { host, .. } => host.post(move |h, me| h.chat(me, &text)),
            Session::Remote { conn, .. } => conn.chat(&text),
        }
    }

    /// The player sitting at this machine, once known.
    pub fn my_player(&self) -> Option<PlayerId> {
        match self {
            Session::Local { host, .. } => Some(host.me()),
            Session::Remote { conn, .. } => match conn.state {
                ConnState::Connected { player, .. } => Some(player),
                _ => None,
            },
        }
    }

    /// The other players in the session, by id and name.
    pub fn others(&self) -> Vec<(PlayerId, String)> {
        let me = self.my_player();
        let all: Vec<(PlayerId, String)> = match self {
            Session::Local { host, .. } => host
                .view()
                .players
                .iter()
                .map(|(p, i)| (*p, i.name.clone()))
                .collect(),
            Session::Remote { names, .. } => names.iter().map(|(p, n)| (*p, n.clone())).collect(),
        };
        all.into_iter().filter(|(p, _)| Some(*p) != me).collect()
    }

    /// The team voice mailbox: speech to send goes in, what the others
    /// said comes out. The same call for a joined world and our own.
    pub fn voice_box(&self) -> hw_net::VoiceBox {
        match self {
            Session::Local { host, .. } => host.voice_box(),
            Session::Remote { conn, .. } => conn.voice.clone(),
        }
    }

    /// Our hero says `text` on the Team channel: every player of the
    /// session hears it, wherever they are. Single player, nobody else does.
    pub fn say_team(&mut self, text: &str) {
        let text = text.to_string();
        match self {
            Session::Local { host, .. } => host.post(move |h, me| h.team_chat(me, &text)),
            Session::Remote { conn, .. } => conn.team_chat(&text),
        }
    }

    /// Damage, repairs and impacts since the last call.
    pub fn take_news(&mut self) -> Vec<ToClient> {
        match self {
            Session::Local { host, .. } => host.take_news(),
            Session::Remote { news, .. } => std::mem::take(news),
        }
    }

    /// A blast at the buildings round `at` (for screenshots, `HW_SMASH`):
    /// how many elements broke and fell.
    pub fn smash(&mut self, at: DVec3, radius: f32, power: f32) -> usize {
        let Session::Local { host, .. } = self else {
            return 0;
        };
        host.call(move |host, _| {
            let hit = host.sim.hit_buildings(at, radius, power, None);
            hit.broken + hit.fell
        })
    }

    /// Wrecks the street cars within `r` of `near` (for screenshots).
    pub fn wreck_cars(&mut self, near: DVec3, r: f64) -> usize {
        let Session::Local { host, .. } = self else {
            return 0;
        };
        host.call(move |host, _| {
            let sim = &mut host.sim;
            let tick = sim.tick;
            let Some(t) = sim.wrecks.traffic.as_ref() else {
                return 0;
            };
            let cars: Vec<_> = t
                .near(
                    glam::DVec2::new(near.x, near.z),
                    r,
                    tick as f64 * hw_sim::DT,
                    12,
                )
                .into_iter()
                .filter(|v| !v.flies())
                .collect();
            let mut n = 0;
            for v in cars {
                let dir = DVec3::new(v.dir.z, 0.0, -v.dir.x);
                n += sim
                    .wrecks
                    .hit(v.pos + DVec3::Y * 0.8, dir, 1.0, 5000.0, tick)
                    .0;
            }
            n
        })
    }

    /// For screenshots (`HW_VIEW=wrecked`): wrecks two small buildings near
    /// the start, one with holes blasted into its front, one brought down.
    /// Returns where to look from and at.
    pub fn wreck_demo(&mut self, town: &GeneratedTown) -> Option<(DVec3, DVec3)> {
        use hw_sim::destruction::{hero_power, material_factor, Frame};
        let Session::Local { host, .. } = self else {
            return None;
        };
        // What the server thread needs of the town.
        let (spawn, _) = town.world.spawn_line();
        let ids: Vec<u64> = town.area.building_ids.iter().map(|id| id.0).collect();
        let props: Vec<(DVec2, f64)> = town.area.props.iter().map(|p| (p.pos, p.y)).collect();
        host.call(move |host, me| {
            let solids = host.sim.solids();
            let mut small: Vec<(f64, u64)> = ids
                .iter()
                .copied()
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
            let centre = |i: u64| {
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
            let by = Some(me);
            // A blast into the first floor up, with a scorch mark.
            let (plan, base) = solids.building(holed)?;
            let f = Frame::of(&plan.site, base);
            if let Some(floor) = plan.floors.get(1) {
                {
                    let x = plan.site.size.x * 0.3;
                    let at = f.to_world(DVec3::new(x, floor.elevation + 1.8, 0.0));
                    let r = host
                        .sim
                        .hit_buildings(at, power.blast_radius, power.blast, by);
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
                        .hit_buildings(at, power.punch_radius, power.punch, by);
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
            // The street between: lamps, trees and cars knocked over, some one
            // way, some the other.
            let street = mid + out * (span * 0.45 + 6.0);
            let strong = hero_power(20);
            let near: Vec<&(DVec2, f64)> = props
                .iter()
                .filter(|(pos, _)| pos.distance(street) < span * 0.5 + 10.0)
                .collect();
            for (k, &&(pos, y)) in near.iter().enumerate() {
                let side = DVec2::new(out.y, -out.x) * if k % 2 == 0 { 1.0 } else { -1.0 };
                let dir = (side - out * 0.4).normalize();
                let at = DVec3::new(pos.x, y + 1.0, pos.y);
                host.sim
                    .street
                    .hit(at, DVec3::new(dir.x, 0.0, dir.y), 0.5, strong.punch, tick);
            }
            Some((DVec3::new(eye.x, base + 7.0, eye.y), look_at))
        })
    }

    /// Playing in someone else's world.
    pub fn joined(&self) -> bool {
        matches!(self, Session::Remote { .. })
    }

    /// Puts our hero somewhere (single player only; for tests and screenshots).
    pub fn teleport(&mut self, pos: DVec3) {
        if let Session::Local { host, .. } = self {
            host.post(move |h, me| {
                if let Some(id) = h.sim.hero_of(me) {
                    h.sim.set_body(
                        id,
                        hw_sim::collision::Body {
                            pos,
                            ..Default::default()
                        },
                    );
                }
            });
        }
    }

    /// Debug menu: our hero file changed by `f` (single player only).
    pub fn debug_sheet(&mut self, f: impl FnOnce(Sheet) -> Sheet + Send + 'static) {
        if let Session::Local { host, .. } = self {
            host.post(move |h, me| {
                if let Some(s) = h.sim.sheet(me) {
                    h.sim.set_sheet(me, f(s));
                }
            });
        }
    }

    /// `HW_CARRY=1`: every villain there is carries an augment (common,
    /// rare and relic in turn), for looking at the gems and the drops.
    pub fn debug_carry(&mut self) {
        if let Session::Local { host, .. } = self {
            host.post(|h, _| {
                for (id, _, _, mut f) in h.sim.enemies() {
                    if f.carried.is_none() {
                        let grade = hw_sim::augments::Grade::from_code((id.0 % 3) as u8);
                        f.carried = Some(hw_net::Augment {
                            effect: 0,
                            percent: hw_sim::augments::strength(grade, f.level),
                            grade: grade.code(),
                        });
                        h.sim.set_fighter(id, f);
                    }
                }
            });
        }
    }

    /// Debug menu: our hero's health and energy full, standing again.
    pub fn debug_heal(&mut self) {
        if let Session::Local { host, .. } = self {
            host.post(|h, me| {
                if let Some(id) = h.sim.hero_of(me) {
                    if let Some(mut f) = h.sim.fighter(id) {
                        f.health = f.max_health;
                        f.energy = f.max_energy;
                        f.status = hw_net::Status::Standing;
                        f.revive = 0.0;
                        h.sim.set_fighter(id, f);
                    }
                }
            });
        }
    }

    /// Debug menu: an incident of `kind` starts near our hero within a
    /// second.
    pub fn debug_incident(&mut self, kind: hw_sim::incidents::IncidentKind) {
        if let Session::Local { host, .. } = self {
            host.post(move |h, _| h.sim.incidents.forced = Some(kind));
        }
    }

    /// Debug menu: breaks the colossus's first `n` weak points (knees,
    /// then the cannon).
    pub fn debug_break_colossus(&mut self, n: u8) {
        if let Session::Local { host, .. } = self {
            host.post(move |h, _| h.sim.break_colossus(n));
        }
    }

    /// Debug menu: brings the nearest small building down (its ground
    /// floor punched out front and back): people are trapped in it.
    /// Returns its centre.
    pub fn debug_collapse(&mut self, near: DVec3) -> Option<DVec3> {
        use hw_sim::destruction::{hero_power, Frame};
        let Session::Local { host, .. } = self else {
            return None;
        };
        host.call(move |host, me| {
            let (_, plan, base) = nearest_building(host, near, 250.0, true)?;
            let power = hero_power(1);
            let by = Some(me);
            let g = Frame::of(&plan.site, base);
            for z in [0.0, plan.site.size.y] {
                let mut x = 0.4;
                while x < plan.site.size.x {
                    let at = g.to_world(DVec3::new(x, plan.floors[0].elevation + 1.6, z));
                    host.sim
                        .hit_buildings(at, power.punch_radius, power.punch, by);
                    x += 1.0;
                }
            }
            Some(g.to_world(DVec3::new(
                plan.site.size.x * 0.5,
                0.0,
                plan.site.size.y * 0.5,
            )))
        })
    }

    /// Debug menu: a blast into the front of the nearest building, a floor
    /// up. Returns where it struck.
    pub fn debug_blast(&mut self, near: DVec3) -> Option<DVec3> {
        use hw_sim::destruction::{hero_power, Frame};
        let Session::Local { host, .. } = self else {
            return None;
        };
        host.call(move |host, me| {
            let (_, plan, base) = nearest_building(host, near, 120.0, false)?;
            let power = hero_power(20);
            let f = Frame::of(&plan.site, base);
            let floor = plan.floors.get(1).or(plan.floors.first())?;
            let x = plan.site.size.x * 0.5;
            let at = f.to_world(DVec3::new(x, floor.elevation + 1.8, 0.0));
            let r = host
                .sim
                .hit_buildings(at, power.blast_radius, power.blast, Some(me));
            let into = f.to_world(DVec3::new(x, 0.0, 1.0)) - f.to_world(DVec3::new(x, 0.0, 0.0));
            host.sim.damage.tell(ToClient::Impact {
                at: at.as_vec3().to_array(),
                dir: into.normalize().as_vec3().to_array(),
                kind: hw_net::ImpactKind::Blast,
                broke: r.broken > 0,
            });
            Some(at)
        })
    }

    /// Debug menu: knocks the street props within `r` of `near` over.
    /// Returns how many were struck.
    pub fn debug_smash_props(&mut self, town: &GeneratedTown, near: DVec3, r: f64) -> usize {
        use hw_sim::destruction::hero_power;
        let Session::Local { host, .. } = self else {
            return 0;
        };
        let props: Vec<(DVec2, f64)> = town.area.props.iter().map(|p| (p.pos, p.y)).collect();
        host.call(move |host, _| {
            let tick = host.sim.tick;
            let strong = hero_power(20);
            let c = DVec2::new(near.x, near.z);
            let mut n = 0;
            for (k, &(pos, y)) in props.iter().enumerate() {
                if pos.distance(c) > r {
                    continue;
                }
                let out = (pos - c).normalize_or(DVec2::X);
                let side = out.perp() * if k % 2 == 0 { 0.4 } else { -0.4 };
                let dir = (out + side).normalize();
                let at = DVec3::new(pos.x, y + 1.0, pos.y);
                host.sim
                    .street
                    .hit(at, DVec3::new(dir.x, 0.0, dir.y), 0.5, strong.punch, tick);
                n += 1;
            }
            n
        })
    }

    /// Where friends on the internet reach a world we host, when the router
    /// forwards our port (UPnP).
    pub fn open_address(&self) -> Option<String> {
        match self {
            Session::Local { ports, .. } => ports.mapping.as_ref()?.address(),
            Session::Remote { .. } => None,
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
            Session::Local { host, .. } => host.view().hero,
            Session::Remote { conn, .. } => conn.hero(),
        }
    }

    /// Runs the world on by `ticks` at once (single player; screenshots).
    pub fn fast_forward(&mut self, ticks: u32) {
        if let Session::Local { host, .. } = self {
            host.post(move |h, _| {
                for _ in 0..ticks {
                    h.sim.step();
                }
            });
        }
    }

    /// Hails a taxi to stand `dest` of the town's stands, or (255) gets out.
    pub fn hail_taxi(&mut self, dest: u8) {
        match self {
            Session::Local { host, .. } => host.post(move |h, me| h.sim.hail_taxi(me, dest)),
            Session::Remote { conn, .. } => conn.hail_taxi(dest),
        }
    }

    /// Wakes the Herobot of `slot` (standing near us) or sends a woken one
    /// home (F).
    pub fn herobot(&mut self, slot: u8) {
        match self {
            Session::Local { host, .. } => host.post(move |h, me| h.herobot(me, slot)),
            Session::Remote { conn, .. } => conn.send(hw_net::ToServer::Herobot { slot }),
        }
    }

    /// Opens (`open`) or shuts a door our hero stands at (E).
    pub fn use_door(&mut self, building: u64, key: u32, open: bool) {
        match self {
            Session::Local { host, .. } => {
                host.post(move |h, me| h.sim.use_door(me, building, key, open))
            }
            Session::Remote { conn, .. } => conn.send(hw_net::ToServer::Door {
                building,
                key,
                open,
            }),
        }
    }

    /// Takes crime `number` from the police computer's board.
    pub fn take_case(&mut self, number: u16) {
        match self {
            Session::Local { host, .. } => host.post(move |h, _| h.sim.missions.take_case(number)),
            Session::Remote { conn, .. } => conn.send(hw_net::ToServer::TakeCase { number }),
        }
    }

    /// Rides the Aether Tube to station `dest`, or ([`hw_sim::tube::HOP_OUT`])
    /// gets out.
    pub fn ride_tube(&mut self, dest: u16) {
        match self {
            Session::Local { host, .. } => host.post(move |h, me| h.sim.ride_tube(me, dest)),
            Session::Remote { conn, .. } => conn.ride_tube(dest),
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
            Session::Local { host, .. } => host.end(Duration::from_millis(500)),
        }
    }
}

/// Debug menu: the nearest building within `r` of `near` that our
/// powers can break: (its index, plan, base).
fn nearest_building(
    host: &mut Host,
    near: DVec3,
    r: f64,
    small: bool,
) -> Option<(u64, std::sync::Arc<hw_gen::building::BuildingPlan>, f64)> {
    use hw_sim::destruction::material_factor;
    let solids = host.sim.solids();
    let p = DVec2::new(near.x, near.z);
    solids
        .buildings_near(p, r)
        .into_iter()
        .filter(|&i| !solids.is_damaged(i))
        .filter_map(|i| {
            let (plan, base) = solids.building(i)?;
            let ok = !small
                || (plan.floors.len() >= 2
                    && plan.floors.len() <= 4
                    && material_factor(&plan) <= 1.3);
            let s = &plan.site;
            let c = s.origin + s.along * s.size.x * 0.5 + s.inward * s.size.y * 0.5;
            ok.then_some((c.distance(p), i, plan, base))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, i, plan, base)| (i, plan, base))
}
