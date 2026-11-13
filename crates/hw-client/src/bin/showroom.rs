//! A showroom for vehicles and props: one model (or a row of them) on a
//! plain floor in daylight, for close-up screenshots while modelling.
//!
//! HW_SHOW=cruiser|bubble|hover|taxi|ambulance|bus|bus_open|wheel|cars|street|lamps|barricades|plaza|life|fences,
//! or one prop (kiosk, pillar, fountain, jersey, drum, ...; default cars)
//! HW_LOD=near|mid|far  HW_PAINT=0..7  HW_YAW=degrees  HW_DIST=metres  HW_PITCH=degrees
//! HW_LOOK_Y=metres (what the camera looks at). Airships float: `airship`
//! (the pirates', its gondola's deck 2.6 m up) and `liner` (its deck's
//! floor 1 m up, nose at the origin). The Zorrak invaders' things:
//! `saucer` (resting just over the floor), `raid` (hanging 14 m up with its
//! tractor beam), `helmet` and `crown` (a zapper's and an overlord's
//! headgear), `claw` and `sceptre`; the Vael trader's `disc` and `wares`.
//! The town's animals: `wibbet`, `drakelet`, `driftbell` (`HW_CLIP` picks the clip).
//!
//! xvfb-run -a env ENGINE_SCREENSHOT=$PWD/screenshots/show.png HW_SHOW=cruiser \
//!   cargo run -p hw-client --bin showroom

use engine::prelude::*;
use glam::DVec3;
use hw_client::carmesh::{car_doors, car_into, car_shell_into, taxi_into, wheel, Door, Lod};
use hw_client::citymesh::MeshBuilder;
use hw_gen::urban::props::CarModel;

/// How high the `raid` saucer hangs (its rim's middle, m).
const RAID_LIFT: f32 = 14.5;

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
        "cap" => return hw_client::policeview::cap(),
        n if n.starts_with("officer") => {}
        "wibbet" | "drakelet" | "driftbell" => {}
        "patrol" => hw_client::carmesh::patrol_into(&mut b, lod, true),
        "patrol_open" => hw_client::carmesh::patrol_into(&mut b, lod, false),
        "cruiser_open" => car_shell_into(&mut b, CarModel::Cruiser, paint, lod, true),
        "bubble_open" => car_shell_into(&mut b, CarModel::Bubble, paint, lod, true),
        "hover_open" => car_shell_into(&mut b, CarModel::Hover, paint, lod, true),
        "ambulance" => hw_client::carmesh::ambulance_into(&mut b, lod),
        // The bus in livery HW_PAINT, its doors shut (bus) or open (bus_open).
        "bus" | "bus_open" => hw_client::carmesh::bus_into(&mut b, paint, lod, true),
        "wheel" => wheel(&mut b, DVec3::new(0.0, 0.36, 0.0), 0.36, 0.22, lod),
        _ => return prop(name, lod),
    }
    b.finish()
}

/// An invader's helmet, its glass bowl merged in (the game hangs the bowl
/// on its own).
fn helmet(kind: hw_net::EntityKind) -> Mesh {
    let Some((mut frame, bowl)) = hw_client::factionview::headgear(kind) else {
        return Mesh::new();
    };
    let _ = frame.append_translated(&bowl, Vec3::ZERO);
    frame
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
        "phone" => pm::phone_booth(lod),
        "post" => pm::post_tube(lod),
        "news" => pm::news_stand(lod),
        "busstop" => pm::bus_stop(lod),
        "cafe" => pm::cafe_table(0, lod),
        "cafe2" => pm::cafe_table(1, lod),
        "picket" => pm::fence(0, lod),
        "railing" => pm::fence(1, lod),
        "rail" => pm::fence(2, lod),
        "sweeper" => hw_client::sweeperview::sweeper(lod),
        "airship" => hw_client::factionview::airship(lod),
        "liner" => hw_client::mastmesh::ship(lod),
        "saucer" | "raid" => hw_client::factionview::saucer(lod),
        "helmet" => helmet(hw_net::EntityKind::Zapper),
        "crown" => helmet(hw_net::EntityKind::Overlord),
        "claw" => hw_client::factionview::weapon(hw_net::EntityKind::Grabber).unwrap_or_default(),
        "sceptre" => {
            hw_client::factionview::weapon(hw_net::EntityKind::Overlord).unwrap_or_default()
        }
        "disc" => hw_client::streetgear::Gear::Disc.mesh(lod),
        "wares" => hw_client::streetgear::Gear::Wares.mesh(lod),
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
    } else if show == "life" {
        vec!["phone", "post", "news", "busstop", "cafe"]
    } else if show == "fences" {
        vec!["picket", "railing", "rail", "sweeper"]
    } else if show == "lamps" {
        vec!["lantern", "deco", "atomic", "traffic"]
    } else {
        vec![show.as_str()]
    };
    let spacing = match show.as_str() {
        "street" => 1.6f32,
        "barricades" => 2.0,
        "life" => 3.2,
        "fences" => 2.7,
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
    let names: Vec<String> = names.iter().map(|n| n.to_string()).collect();
    let mut meshes = Some(meshes);
    Engine::run("showroom", move |world, frame| {
        if frame.first {
            for (i, m) in meshes.take().unwrap_or_default().into_iter().enumerate() {
                let x = -width * 0.5 + spacing * i as f32;
                let lift = env_f("HW_LIFT", 0.0)
                    + match names[i].as_str() {
                        "airship" => hw_client::factionview::GONDOLA_DECK as f32 + 2.6,
                        "liner" => hw_gen::urban::mast::DECK_DROP as f32 + 1.0,
                        "saucer" => hw_client::factionview::SAUCER_UNDER_DEPTH as f32 + 0.3,
                        "raid" => RAID_LIFT,
                        "disc" => hw_sim::streetlife::HOVER as f32,
                        _ => 0.0,
                    };
                world.place(m, Place::new(x, lift, 0.0))?;
                if names[i] == "raid" {
                    // Its beam from the lens down to the floor.
                    let lens = lift - hw_client::factionview::SAUCER_UNDER_DEPTH as f32 + 0.1;
                    let beam = hw_client::factionview::tractor_beam(lens);
                    world.place(beam, Place::new(x, lens, 0.0))?;
                }
            }
            // `HW_SHOW=officer` (or `officer1`): a police officer with their
            // cap on; `HW_CAP="x,y,z,tip,size"` moves the cap, for fitting it.
            if let Some(k) = show.strip_prefix("officer") {
                use hw_client::people::{packs_dir, People, Who};
                let look: u8 = k.parse().unwrap_or(0);
                let dir = packs_dir().expect("the Actors packs (assets/packs)");
                let people = People::start(dir);
                people.request(Who::Officer(look), 0);
                let built = people.wait(1).pop().expect("an officer");
                let e = world.spawn_animated_shared(
                    built.model,
                    Place::new(0.0, 0.0, 0.0).with_yaw_deg(env_f("HW_TURN", 0.0)),
                )?;
                world.play_animation(e, &std::env::var("HW_CLIP").unwrap_or("idle".into()))?;
                let (x, y, z) = hw_client::policeview::CAP_AT;
                let at: Vec<f32> = std::env::var("HW_CAP")
                    .ok()
                    .map(|v| v.split(',').filter_map(|n| n.parse().ok()).collect())
                    .filter(|v: &Vec<f32>| v.len() == 5)
                    .unwrap_or(vec![
                        x,
                        y,
                        z,
                        hw_client::policeview::CAP_TIP,
                        hw_client::policeview::CAP_SIZE,
                    ]);
                let cap = world.place(hw_client::policeview::cap(), Place::new(0.0, 0.0, 0.0))?;
                world.attach_to_bone(
                    cap,
                    e,
                    "head",
                    Place::new(at[0], at[1], at[2])
                        .with_pitch_deg(at[3])
                        .with_scale(at[4]),
                )?;
            }
            // `HW_SHOW=wibbet|drakelet|driftbell`: the town's animals, their
            // three colourings in a row, playing `HW_CLIP` (`HW_LOD` picks
            // the level, `HW_TURN` and `HW_TILT` turn them, `HW_LIFT` lifts).
            if matches!(show.as_str(), "wibbet" | "drakelet" | "driftbell") {
                use hw_sim::citizens::critters::Species;
                let (species, clip, gap) = match show.as_str() {
                    "wibbet" => (Species::Wibbet, "Idle", 0.75),
                    "drakelet" => (Species::Drakelet, "Hover", 1.25),
                    _ => (Species::Driftbell, "Drift", 1.3),
                };
                let dir = hw_client::critterview::creatures_dir().expect("assets/creatures");
                let level = match lod {
                    Lod::Near => 0,
                    Lod::Mid => 1,
                    Lod::Far => 2,
                };
                let clip = std::env::var("HW_CLIP").unwrap_or(clip.into());
                for (s, coat, levels) in hw_client::critterview::load_all(&dir)? {
                    if s != species {
                        continue;
                    }
                    let m = &levels[level];
                    let tris: usize = m.meshes.iter().map(|x| x.indices.len() / 3).sum();
                    println!("{show} {coat}: {tris} triangles");
                    let e = world.spawn_animated_shared(
                        m.clone(),
                        Place::new((coat as f32 - 1.0) * gap, env_f("HW_LIFT", 0.0), 0.0)
                            .with_yaw_deg(env_f("HW_TURN", 0.0))
                            .with_pitch_deg(env_f("HW_TILT", 0.0)),
                    )?;
                    world.play_animation(e, &clip)?;
                }
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
                "patrol_open" => {
                    let (mesh, hinge, axis) = hw_client::carmesh::patrol_door(lod);
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
            if show == "bus" || show == "bus_open" {
                let open = if show == "bus" { 0.0 } else { open };
                for (door, at) in hw_client::carmesh::bus_door_leaves(open) {
                    let leaf = hw_client::carmesh::bus_door_leaf(paint, door, lod);
                    world.place(leaf, Place::new(at.x as f32, at.y as f32, at.z as f32))?;
                }
            }
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
