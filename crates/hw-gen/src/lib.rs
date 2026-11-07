//! World generation as pure data. Everything here is a plan (planet, region,
//! city, building, interior) derived from the world seed; nothing renders and
//! nothing simulates. Same seed and inputs, same plan, on every machine.

pub use hw_core::WORLD_VERSION;

#[cfg(test)]
mod tests {
    #[test]
    fn links_against_core() {
        assert_eq!(super::WORLD_VERSION, hw_core::WORLD_VERSION);
    }
}
