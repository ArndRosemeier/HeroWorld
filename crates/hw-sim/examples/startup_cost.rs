//! Where the time goes when a town starts up (macro plan, the area round the
//! start, collision): `cargo run --release -p hw-sim --example startup_cost -- 1000000`
use hw_core::Seed;
use hw_gen::urban::area::{Area, AREA_HALF};
use hw_gen::urban::world::TownWorld;
use hw_gen::urban::TownParams;
use std::sync::Arc;
use std::time::Instant;

fn main() {
    let pop: f64 = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(1e6);
    let seed = Seed(1);
    let t = Instant::now();
    let world = Arc::new(TownWorld::generate(seed, TownParams::sample(seed, pop)));
    println!("town plan {:.2?}", t.elapsed());
    let t = Instant::now();
    let (start, _) = world.spawn_line();
    println!("start {:.2?}", t.elapsed());
    let t = Instant::now();
    let area = Area::around(world.clone(), start, AREA_HALF);
    println!(
        "area {:.2?}: {} buildings, {} props",
        t.elapsed(),
        area.buildings.len(),
        area.props.len()
    );
    let t = Instant::now();
    let w = world.clone();
    let terrain = hw_sim::collision::Terrain(Arc::new(move |p| w.ground(p)));
    let _o = hw_sim::collision::Obstacles::from_town(&world, terrain);
    println!("collision {:.2?}", t.elapsed());
    let area = Arc::new(area);
    let t = Instant::now();
    let l = hw_gen::urban::lanes::street_lanes(&area, &area.roads);
    println!(
        "street lanes {:.2?}: {} loops from {} roads",
        t.elapsed(),
        l.len(),
        area.roads.len()
    );
    let t = Instant::now();
    let traffic = hw_sim::traffic::Traffic::new(area.clone());
    println!("traffic {:.2?}", t.elapsed());
    let t = Instant::now();
    let _c = hw_sim::citizens::Citizens::new(area, traffic.signals());
    println!("citizens {:.2?}", t.elapsed());
}
