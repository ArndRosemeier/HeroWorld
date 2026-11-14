//! Runs a world without a window, for dedicated hosting, bots and tests.
//!
//! It hosts the same town the game hosts, so players who join see the world
//! the server simulates.
//!
//! Environment: `HW_SEED` (default 1), `HW_POP` (the town's people, default
//! [`hw_sim::DEFAULT_POPULATION`]), `HW_PORT` (default 7777),
//! `HW_TICKS` (stop after this many ticks; default: run forever),
//! `HW_SAVE` (world save file: loaded at start if it exists, written every
//! minute and at the end), `HW_ONLINE=1` (also list the world on GameHub;
//! friends join with the room code it prints), `HW_NAME` (the room's name),
//! `HW_NO_UPNP=1` (don't ask the router to forward the port).
//!
//! Type into the console: `quit` ends the session cleanly (players are told
//! and the world is saved), `save` saves now, anything else is chat.

use hw_core::Seed;
use hw_net::gamehub::{Hub, HubServerLink};
use hw_sim::host::{Host, HostEvent};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn env<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok().and_then(|v| v.parse().ok())
}

fn main() -> std::io::Result<()> {
    let seed = env("HW_SEED").unwrap_or(1u64);
    let port = env("HW_PORT").unwrap_or(hw_net::DEFAULT_PORT);
    let ticks: Option<u64> = env("HW_TICKS");
    let save: Option<PathBuf> = std::env::var_os("HW_SAVE").map(PathBuf::from);
    let people = env("HW_POP").unwrap_or(hw_sim::DEFAULT_POPULATION);
    let town = hw_sim::GeneratedTown::generate(Seed(seed), people);
    let mut host = Host::with_sim(hw_sim::Sim::for_town(Seed(seed), &town));
    match &save {
        Some(path) if path.exists() => {
            host.load_world(path)?;
            println!("hw-headless: loaded {}", path.display());
        }
        _ => {
            host.sim.place_starter_crates();
            host.sim.place_first_fight();
        }
    }
    let addr = host.listen(("0.0.0.0", port))?;
    println!("hw-headless: world {seed} listening on {addr}");
    // Ask the router to forward the port (a home PC); `HW_NO_UPNP` skips it.
    let upnp = (std::env::var_os("HW_NO_UPNP").is_none())
        .then(|| hw_net::upnp::map_udp_in_background(addr.port(), "HeroWorld server"));
    let mut mapping = None;
    if std::env::var_os("HW_ONLINE").is_some() {
        let name = std::env::var("HW_NAME").unwrap_or_else(|_| "A HeroWorld".into());
        let meta = serde_json::json!({
            "seed": seed,
            "population": host.sim.town_population,
            "version": hw_core::version(),
            "protocol": hw_net::PROTOCOL_VERSION,
        });
        let mut hub = Hub::connect(&hw_net::gamehub::hub_url(), &name)?;
        let code = hub.host_room(&format!("{name} (dedicated)"), meta)?;
        host.add_link(Box::new(HubServerLink::new(hub)));
        println!("hw-headless: on GameHub as room {code}: friends join with HW_JOIN={code}");
    }

    let (tx, console) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    let period = Duration::from_secs_f64(hw_sim::DT);
    let mut next = Instant::now();
    let mut last_save = Instant::now();
    let host_player = hw_net::PlayerId(0);
    loop {
        if mapping.is_none() {
            match upnp.as_ref().map(|rx| rx.try_recv()) {
                Some(Ok(Ok(m))) => {
                    match m.address() {
                        Some(a) => println!("hw-headless: the router forwards it: join at {a}"),
                        None => println!("hw-headless: the router forwards port {}", m.port),
                    }
                    mapping = Some(Some(m));
                }
                Some(Ok(Err(e))) => {
                    println!("hw-headless: no automatic port forwarding ({e})");
                    mapping = Some(None);
                }
                _ => {}
            }
        }
        for e in host.tick() {
            match e {
                HostEvent::Joined { player, name } => {
                    println!("{name} joined as player {}", player.0)
                }
                HostEvent::Left { player, name } => println!("{name} (player {}) left", player.0),
                HostEvent::Chat { name, text, .. } => println!("{name}: {text}"),
                HostEvent::TeamChat { name, text, .. } => println!("[team] {name}: {text}"),
            }
        }
        let mut quit = ticks.is_some_and(|t| host.sim.tick >= t);
        while let Ok(line) = console.try_recv() {
            match line.trim() {
                "quit" | "exit" => quit = true,
                "save" => last_save = Instant::now() - Duration::from_secs(3600),
                "" => {}
                text => host.chat(host_player, text),
            }
        }
        if let Some(path) = &save {
            if quit || last_save.elapsed() >= Duration::from_secs(60) {
                host.save_world(path)?;
                last_save = Instant::now();
            }
        }
        if quit {
            host.end_session("the host closed the world", Duration::from_secs(1));
            println!("hw-headless: ran {} ticks", host.sim.tick);
            return Ok(());
        }
        next += period;
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else {
            next = now;
        }
    }
}
