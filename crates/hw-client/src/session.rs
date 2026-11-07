//! Where the game state comes from: a server inside this process (single
//! player, optionally open to friends) or a connection to someone else's.

use hw_core::{NetId, Seed};
use hw_net::{ConnState, Connection, EntityState, InputState, PlayerId};
use hw_sim::host::Host;
use std::time::{Duration, Instant};

pub enum Session {
    Local { host: Box<Host>, me: PlayerId },
    Remote { conn: Box<Connection> },
}

impl Session {
    /// Single player; `open` also listens for friends on the default port.
    pub fn single(seed: Seed, name: &str, open: bool) -> std::io::Result<Self> {
        let mut host = Host::new(seed);
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

    pub fn join(addr: &str, name: &str) -> std::io::Result<Self> {
        Ok(Self::Remote {
            conn: Box::new(Connection::open(addr, name)?),
        })
    }

    /// For a joining player: wait (up to 5 s) for the host's welcome and
    /// return its world seed.
    pub fn wait_for_seed(&mut self) -> Option<u64> {
        match self {
            Session::Local { host, .. } => Some(host.sim.seed.0),
            Session::Remote { conn } => {
                let start = Instant::now();
                while start.elapsed() < Duration::from_secs(5) {
                    conn.update(InputState::default());
                    match &conn.state {
                        ConnState::Connected { world_seed, .. } => return Some(*world_seed),
                        ConnState::Rejected(reason) => {
                            eprintln!("The host turned us away: {reason}");
                            return None;
                        }
                        ConnState::Connecting => std::thread::sleep(Duration::from_millis(20)),
                    }
                }
                eprintln!("No answer from the host.");
                None
            }
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
            Session::Remote { conn } => conn.update(input),
        }
    }

    pub fn entities(&mut self) -> Vec<EntityState> {
        match self {
            Session::Local { host, .. } => host.sim.snapshot(),
            Session::Remote { conn } => conn
                .latest
                .as_ref()
                .map(|(_, _, e)| e.clone())
                .unwrap_or_default(),
        }
    }

    pub fn my_hero(&mut self) -> Option<NetId> {
        match self {
            Session::Local { host, me } => host.sim.hero_of(*me),
            Session::Remote { conn } => conn.hero(),
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Session::Remote { conn } = self {
            conn.close();
        }
    }
}
