//! Writes a contact sheet of wall skins: every base in every wear, each
//! tile repeated 2 x 2 and tinted like a wall, to look at them quickly.
//! `cargo run --release -p hw-client --example skins -- out.png [size]`

use engine::texgen::{FacadeLayer as F, MedievalLayer as M, RoofLayer as R};
use hw_client::skins::{make, Base, Skin, Wear};

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "skins.png".into());
    let size: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(128);
    let bases: &[(Base, [f32; 3])] = &[
        (Base::Facade(F::Plaster), [0.93, 0.86, 0.7]),
        (Base::Facade(F::Stucco), [0.95, 0.8, 0.72]),
        (Base::Medieval(M::Limewash), [1.0, 1.0, 1.0]),
        (Base::Facade(F::Concrete), [1.0, 1.0, 1.0]),
        (Base::Facade(F::BrickRed), [1.0, 1.0, 1.0]),
        (Base::Facade(F::Limestone), [1.0, 1.0, 1.0]),
        (Base::Facade(F::WoodSiding), [0.7, 0.85, 0.9]),
        (Base::Enamel, [0.55, 0.85, 0.8]),
        (Base::CastStone, [0.9, 0.88, 0.82]),
        (Base::PaintedWood, [0.9, 0.9, 0.9]),
        (Base::Roof(R::Slate), [1.0, 1.0, 1.0]),
        (Base::Roof(R::ClayTile), [1.0, 1.0, 1.0]),
    ];
    let cell = size * 2;
    let (w, h) = (cell * 4, cell * bases.len() as u32);
    let mut img = image::RgbImage::new(w, h);
    for (row, (base, tint)) in bases.iter().enumerate() {
        for (col, wear) in Wear::ALL.iter().enumerate() {
            let t = make(Skin::new(*base, *wear), size);
            for y in 0..cell {
                for x in 0..cell {
                    let i = (((y % size) * size + (x % size)) * 4) as usize;
                    let px = [0, 1, 2].map(|k| (t.albedo[i + k] as f32 * tint[k]) as u8);
                    img.put_pixel(col as u32 * cell + x, row as u32 * cell + y, image::Rgb(px));
                }
            }
        }
    }
    img.save(&out).expect("save");
}
