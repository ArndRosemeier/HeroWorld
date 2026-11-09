//! What the people of a game start cost in memory: builds the looks the game
//! asks for at the start (our hero, the street villains, the citizens and
//! mole folk) and splits the bytes into meshes, maps and clips, and how much
//! of it is the same data held more than once.
//!
//! ```text
//! cargo run --release -p hw-client --example people_memory -- [town seed]
//! ```
//! Needs the packs (`scripts/fetch-packs.sh`).

use engine::anim::AnimatedModel;
use engine::mesh::AlbedoMap;
use hw_client::people::{self, VillainKind, Who};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicIsize, Ordering};

struct Counting;
static LIVE: AtomicIsize = AtomicIsize::new(0);
static PEAK: AtomicIsize = AtomicIsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let n = LIVE.fetch_add(l.size() as isize, Ordering::Relaxed) + l.size() as isize;
        PEAK.fetch_max(n, Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size() as isize, Ordering::Relaxed);
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        let n = LIVE.fetch_add(new as isize - l.size() as isize, Ordering::Relaxed) + new as isize
            - l.size() as isize;
        PEAK.fetch_max(n, Ordering::Relaxed);
        unsafe { System.realloc(p, l, new) }
    }
}

#[global_allocator]
static A: Counting = Counting;

fn mb(n: usize) -> f64 {
    n as f64 / (1024.0 * 1024.0)
}

fn live() -> usize {
    LIVE.load(Ordering::Relaxed).max(0) as usize
}

#[derive(Default)]
struct Split {
    vertices: usize,
    indices: usize,
    morphs: usize,
    maps: usize,
    clips: usize,
}

fn hash_of(bytes: &[u8]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

fn split(
    m: &AnimatedModel,
    maps: &mut BTreeMap<u64, usize>,
    meshes: &mut BTreeMap<u64, usize>,
) -> Split {
    let mut s = Split::default();
    for mesh in &m.meshes {
        let v = mesh.positions.len();
        let vb = v * (12 + 12 + 16 + 8 + 8 + 16);
        s.vertices += vb;
        s.indices += mesh.indices.len() * 4;
        s.morphs += mesh
            .morphs
            .iter()
            .map(|mm| mm.vertices.len() * 4 + (mm.positions.len() + mm.normals.len()) * 12)
            .sum::<usize>();
        let pos: Vec<u8> = mesh
            .positions
            .iter()
            .flat_map(|p| p.to_array().map(f32::to_bits))
            .flat_map(u32::to_le_bytes)
            .collect();
        *meshes.entry(hash_of(&pos)).or_default() += vb + mesh.indices.len() * 4;
        let all: [Option<&AlbedoMap>; 4] = [
            mesh.albedo.as_ref(),
            mesh.maps.normal.as_ref(),
            mesh.maps.orm.as_ref(),
            mesh.maps.emissive.as_ref(),
        ];
        for map in all.into_iter().flatten() {
            s.maps += map.rgba.len();
            *maps.entry(hash_of(&map.rgba)).or_default() += map.rgba.len();
        }
    }
    for c in &m.clips {
        for ch in &c.channels {
            if let Some(t) = &ch.translation {
                s.clips += t.times.len() * 4 + t.values.len() * 12;
            }
            if let Some(t) = &ch.rotation {
                s.clips += t.times.len() * 4 + t.values.len() * 16;
            }
            if let Some(t) = &ch.scale {
                s.clips += t.times.len() * 4 + t.values.len() * 12;
            }
        }
    }
    s
}

fn main() {
    let seed: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let packs = people::packs_dir().expect("no packs: run scripts/fetch-packs.sh");
    let before = live();
    let b = actors_engine::CharacterBuilder::load(&packs).expect("packs");
    println!(
        "a loaded builder with no characters built: {:.0} MB",
        mb(live() - before)
    );
    drop(b);
    let t0 = std::time::Instant::now();
    let p = people::People::start(packs);
    let mut whos = vec![Who::Hero("Hero".into())];
    for kind in VillainKind::ALL {
        for look in 0..people::VILLAIN_LOOKS {
            whos.push(Who::Villain(kind, look));
        }
    }
    for look in 0..hw_sim::citizens::LOOKS + hw_sim::citizens::MOLE_LOOKS {
        whos.push(Who::Citizen(look));
    }
    for w in &whos {
        p.request(w.clone(), seed);
    }
    let built = p.wait(whos.len());
    let _ = p.take_catalog();
    let total_live = live();
    let mut maps = BTreeMap::new();
    let mut meshes = BTreeMap::new();
    let mut sum = Split::default();
    if std::env::var_os("PEOPLE_MESHES").is_some() {
        for b in built.iter().take(3) {
            for m in &b.model.meshes {
                println!(
                    "  {:?}: {} vertices, {} triangles, maps {:?} {:?} {:?}",
                    b.who,
                    m.positions.len(),
                    m.indices.len() / 3,
                    m.albedo.as_ref().map(|a| (a.width, a.height)),
                    m.maps.normal.as_ref().map(|a| (a.width, a.height)),
                    m.maps.orm.as_ref().map(|a| (a.width, a.height)),
                );
            }
        }
    }
    for b in &built {
        let s = split(&b.model, &mut maps, &mut meshes);
        println!(
            "{:<28} meshes {:>3}  vertices {:>6.1} MB  indices {:>5.1} MB  morphs {:>5.1} MB  maps {:>6.1} MB  clips {:>5.1} MB",
            format!("{:?}", b.who),
            b.model.meshes.len(),
            mb(s.vertices),
            mb(s.indices),
            mb(s.morphs),
            mb(s.maps),
            mb(s.clips)
        );
        sum.vertices += s.vertices;
        sum.indices += s.indices;
        sum.morphs += s.morphs;
        sum.maps += s.maps;
        sum.clips += s.clips;
    }
    let models = sum.vertices + sum.indices + sum.morphs + sum.maps + sum.clips;
    println!();
    println!(
        "{} characters built in {:.1} s",
        built.len(),
        t0.elapsed().as_secs_f64()
    );
    println!(
        "models: {:.0} MB (vertices {:.0}, indices {:.0}, morphs {:.0}, maps {:.0}, clips {:.0})",
        mb(models),
        mb(sum.vertices),
        mb(sum.indices),
        mb(sum.morphs),
        mb(sum.maps),
        mb(sum.clips)
    );
    let unique_maps: usize = maps.keys().count();
    let distinct_map_bytes: usize = {
        // Each distinct map once: its bytes divided by how often it was counted.
        let mut seen = BTreeMap::new();
        for b in &built {
            for mesh in &b.model.meshes {
                for map in [
                    mesh.albedo.as_ref(),
                    mesh.maps.normal.as_ref(),
                    mesh.maps.orm.as_ref(),
                    mesh.maps.emissive.as_ref(),
                ]
                .into_iter()
                .flatten()
                {
                    seen.insert(hash_of(&map.rgba), map.rgba.len());
                }
            }
        }
        seen.values().sum()
    };
    println!(
        "maps: {} distinct of {:.0} MB held, {:.0} MB if each distinct map were held once",
        unique_maps,
        mb(sum.maps),
        mb(distinct_map_bytes)
    );
    println!("meshes: {} distinct shapes", meshes.len());
    println!(
        "heap live with the builder thread and its caches: {:.0} MB (peak {:.0} MB)",
        mb(total_live),
        mb(PEAK.load(Ordering::Relaxed).max(0) as usize)
    );
    if std::env::var_os("PEOPLE_QUICK").is_none() {
        std::thread::sleep(people::IDLE_DROP + std::time::Duration::from_secs(1));
    }
    println!(
        "heap live after the people thread idled {} s: {:.0} MB",
        people::IDLE_DROP.as_secs(),
        mb(live())
    );
    drop(p);
    std::thread::sleep(std::time::Duration::from_millis(500));
    println!(
        "heap live after the builder is gone (models only): {:.0} MB",
        mb(live())
    );
    drop(built);
    println!("heap live with nothing: {:.0} MB", mb(live()));
}
