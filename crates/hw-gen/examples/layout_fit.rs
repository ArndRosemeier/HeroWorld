//! Which office schemes fit which plots: a table for whoever tunes plots.
use glam::DVec2;
use hw_core::Seed;
use hw_gen::layout::office::plan_scheme;
use hw_gen::layout::Scheme;

fn main() {
    for scheme in [Scheme::GrandHall, Scheme::Atrium, Scheme::SplitLevel] {
        println!("{scheme:?}: plans out of 8 seeds, by width (rows) and depth (columns)");
        for w in (24..=64).step_by(8) {
            let row: Vec<String> = (24..=64)
                .step_by(8)
                .map(|d| {
                    let n = (0..8u64)
                        .filter(|s| {
                            plan_scheme(Seed(*s), DVec2::new(w as f64, d as f64), 5, scheme)
                                .is_some_and(|l| l.problems().is_empty())
                        })
                        .count();
                    format!("{n}")
                })
                .collect();
            println!("  {w}: {}", row.join(" "));
        }
        let tall: Vec<String> = [2u32, 3, 4, 6, 8, 12, 16]
            .iter()
            .map(|n| {
                let ok = (0..8u64)
                    .filter(|s| {
                        plan_scheme(Seed(*s), DVec2::new(48.0, 48.0), *n, scheme)
                            .is_some_and(|l| l.problems().is_empty())
                    })
                    .count();
                format!("{n} storeys: {ok}")
            })
            .collect();
        println!("  on 48 x 48: {}", tall.join(", "));
    }
}
