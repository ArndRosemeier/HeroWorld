//! Runs a world without a window, for dedicated hosting, bots and tests.
//!
//! Environment: `HW_SEED` (default 1), `HW_PORT` (default 7777),
//! `HW_TICKS` (stop after this many ticks; default: run forever).

use hw_core::Seed;
use hw_sim::host::{Host, HostEvent};
use std::time::{Duration, Instant};

fn env<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok().and_then(|v| v.parse().ok())
}

fn main() -> std::io::Result<()> {
    let seed = env("HW_SEED").unwrap_or(1u64);
    let port = env("HW_PORT").unwrap_or(hw_net::DEFAULT_PORT);
    let ticks: Option<u64> = env("HW_TICKS");
    let mut host = Host::new(Seed(seed));
    let addr = host.listen(("0.0.0.0", port))?;
    println!("hw-headless: world {seed} listening on {addr}");
    let period = Duration::from_secs_f64(hw_sim::DT);
    let mut next = Instant::now();
    loop {
        for e in host.tick() {
            match e {
                HostEvent::Joined { player, name } => {
                    println!("{name} joined as player {}", player.0)
                }
                HostEvent::Left { player, name } => println!("{name} (player {}) left", player.0),
            }
        }
        if ticks.is_some_and(|t| host.sim.tick >= t) {
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
