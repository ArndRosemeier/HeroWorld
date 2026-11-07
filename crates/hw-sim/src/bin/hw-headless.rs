//! Runs the server without a window, for tests, bots and dedicated hosting.

fn main() {
    let mut sim = hw_sim::Sim::new();
    let ticks = hw_sim::TICK_HZ;
    for _ in 0..ticks {
        sim.step();
    }
    println!("hw-headless: ran {} ticks", sim.tick);
}
