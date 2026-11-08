//! A showroom for vehicles and props: one model (or a row of them) on a
//! plain floor in daylight, for close-up screenshots while modelling.
//!
//! HW_SHOW=cruiser|bubble|hover|taxi|ambulance|wheel|cars|street|lamps|barricades|plaza,
//! or one prop (kiosk, pillar, fountain, jersey, drum, ...; default cars)
//! HW_LOD=near|mid|far  HW_PAINT=0..7  HW_YAW=degrees  HW_DIST=metres  HW_PITCH=degrees
//!
//! xvfb-run -a env ENGINE_SCREENSHOT=$PWD/screenshots/show.png HW_SHOW=cruiser \
//!   cargo run -p hw-client --bin showroom

use engine::prelude::*;
use glam::DVec3;
use hw_client::carmesh::{car_doors, car_into, car_shell_into, taxi_into, wheel, Door, Lod};
use hw_client::citymesh::MeshBuilder;
use hw_gen::urban::props::CarModel;

fn env_f(name: &str, default: f32) -> f32 {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn model(name: &str, lod: Lod, paint: u8) -> Mesh {
    let mut b = MeshBuilder::new();
    match name {
        "cruiser" => car_into(&mut b, CarModel::Cruiser, paint, lod, true),
        "bubble" => car_into(&mut b, CarModel::Bubble, paint, lod, true),
        "hover" => car_into(&mut b, CarModel::Hover, paint, lod, true),
        "taxi" => taxi_into(&mut b, lod, true),
        "taxi_open" => taxi_into(&mut b, lod, false),
        "cruiser_open" => car_shell_into(&mut b, CarModel::Cruiser, paint, lod, true),
        "bubble_open" => car_shell_into(&mut b, CarModel::Bubble, paint, lod, true),
        "hover_open" => car_shell_into(&mut b, CarModel::Hover, paint, lod, true),
        "ambulance" => hw_client::carmesh::ambulance_into(&mut b, lod),
        "wheel" => wheel(&mut b, DVec3::new(0.0, 0.36, 0.0), 0.36, 0.22, lod),
        _ => return prop(name, lod),
    }
    b.finish()
}

fn prop(name: &str, lod: Lod) -> Mesh {
    use hw_client::propmesh as pm;
    use hw_gen::urban::props::{BarricadePiece, LampStyle};
    match name {
        "lantern" => pm::lamp(LampStyle::Lantern, lod),
        "deco" => pm::lamp(LampStyle::Deco, lod),
        "atomic" => pm::lamp(LampStyle::Atomic, lod),
        "traffic" => pm::traffic_light(lod),
        "bench" => pm::bench(lod),
        "bin" => pm::bin(lod),
        "hydrant" => pm::hydrant(lod),
        "mailbox" => pm::mailbox(lod),
        "bollard" => pm::bollard(lod),
        "kiosk" => pm::kiosk(lod),
        "pillar" => pm::ad_pillar(lod),
        "fountain" => pm::fountain(lod),
        "jersey" => pm::barricade(BarricadePiece::Jersey, lod),
        "sawhorse" => pm::barricade(BarricadePiece::Sawhorse, lod),
        "sandbags" => pm::barricade(BarricadePiece::Sandbags, lod),
        "drum" => pm::barricade(BarricadePiece::Drum, lod),
        _ => Mesh::new(),
    }
}

fn main() -> EngineResult<()> {
    let show = std::env::var("HW_SHOW").unwrap_or_else(|_| "cars".into());
    let lod = match std::env::var("HW_LOD").as_deref() {
        Ok("mid") => Lod::Mid,
        Ok("far") => Lod::Far,
        _ => Lod::Near,
    };
    let paint = env_f("HW_PAINT", 0.0) as u8;
    let names: Vec<&str> = if show == "cars" {
        vec!["cruiser", "bubble", "hover", "taxi"]
    } else if show == "street" {
        vec!["bench", "bin", "hydrant", "mailbox", "bollard"]
    } else if show == "barricades" {
        vec!["jersey", "sawhorse", "sandbags", "drum"]
    } else if show == "plaza" {
        vec!["kiosk", "pillar"]
    } else if show == "lamps" {
        vec!["lantern", "deco", "atomic", "traffic"]
    } else {
        vec![show.as_str()]
    };
    let spacing = match show.as_str() {
        "street" => 1.6f32,
        "barricades" => 2.0,
        _ => 3.4,
    };
    let width = spacing * (names.len() as f32 - 1.0);
    let meshes: Vec<Mesh> = names
        .iter()
        .enumerate()
        .map(|(i, n)| model(n, lod, paint + i as u8))
        .collect();
    for (n, m) in names.iter().zip(&meshes) {
        println!("{n}: {} triangles", m.build().triangle_count());
    }
    let yaw = env_f("HW_YAW", 35.0);
    let pitch = env_f("HW_PITCH", 14.0);
    let dist = env_f("HW_DIST", if names.len() > 1 { 9.0 + width } else { 7.0 });
    let mut meshes = Some(meshes);
    Engine::run("showroom", move |world, frame| {
        if frame.first {
            for (i, m) in meshes.take().unwrap_or_default().into_iter().enumerate() {
                let x = -width * 0.5 + spacing * i as f32;
                world.place(m, Place::new(x, 0.0, 0.0))?;
            }
            world.spawn(Shape::box_at(
                (0.0, -0.05, 0.0),
                (60.0, 0.1, 60.0),
                rgb(120, 122, 126),
            )?);
            world.set_sun((0.5, 1.0, 0.35), 0.25);
            // Opened doors (HW_OPEN, a fraction of fully open).
            let open = env_f("HW_OPEN", 0.8) as f64;
            let doors: Vec<Door> = match show.as_str() {
                "taxi_open" => {
                    let (mesh, hinge, axis) = hw_client::carmesh::taxi_door(lod);
                    vec![Door {
                        mesh,
                        hinge,
                        axis,
                        open: 0.9,
                    }]
                }
                "cruiser_open" => car_doors(CarModel::Cruiser, paint, lod),
                "bubble_open" => car_doors(CarModel::Bubble, paint, lod),
                "hover_open" => car_doors(CarModel::Hover, paint, lod),
                _ => vec![],
            };
            for d in doors {
                let deg = (d.open * open).to_degrees() as f32;
                let a = d.axis;
                let mut place = Place::new(d.hinge.x as f32, d.hinge.y as f32, d.hinge.z as f32);
                if a.y.abs() > 0.5 {
                    // Engine turns a positive yaw clockwise seen from above:
                    // about -Y.
                    place = place.with_yaw_deg(-deg * a.y.signum() as f32);
                } else if a.x.abs() > 0.5 {
                    place = place.with_pitch_deg(deg * a.x.signum() as f32);
                } else {
                    place = place.with_roll_deg(deg * a.z.signum() as f32);
                }
                world.place(d.mesh, place)?;
            }
        }
        let target_y = env_f("HW_LOOK_Y", 0.8);
        world.look_orbit((0.0, target_y, 0.0), dist, yaw, pitch);
        Ok(())
    })
}
