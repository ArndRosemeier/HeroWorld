//! A hero's looks: an Actors `CharacterConfig`, kept in the hero file and
//! sent to the other players as compact JSON (only what differs from the
//! defaults, the same JSON the Actors web creator saves).
//!
//! Whatever arrives is put through the hero rules before it is built: the
//! catalog of what heroes may wear (see [`crate::people::hero_catalog`]),
//! grown-ups only, and never undressed.

use actors_generator::{sanitize_config, CharacterCatalog, CharacterConfig};
use serde_json::Value;

/// The looks as compact JSON bytes.
pub fn encode(config: &CharacterConfig) -> Vec<u8> {
    let (Ok(Value::Object(mine)), Ok(Value::Object(plain))) = (
        serde_json::to_value(config),
        serde_json::to_value(CharacterConfig::default()),
    ) else {
        return Vec::new();
    };
    let diff: serde_json::Map<String, Value> = mine
        .into_iter()
        .filter(|(k, v)| plain.get(k) != Some(v))
        .collect();
    serde_json::to_vec(&Value::Object(diff)).unwrap_or_default()
}

/// Looks from JSON (compact or full); `None` if it is not a character.
pub fn decode(bytes: &[u8]) -> Option<CharacterConfig> {
    serde_json::from_slice(bytes).ok()
}

/// A stable key for the looks (FNV-1a over the compact JSON): the same
/// looks build one model, on every machine.
pub fn key(config: &CharacterConfig) -> u64 {
    key_of(&encode(config))
}

/// The key of looks as they came over the wire (see [`key`]).
pub fn key_of(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// The looks kept to the hero rules: what `catalog` offers (made by
/// [`crate::people::hero_catalog`]), no anatomy, standing still in the
/// idle clip, and at least underwear on.
pub fn hero_rules(catalog: &CharacterCatalog, config: &CharacterConfig) -> CharacterConfig {
    let mut c = sanitize_config(catalog, config);
    c.anatomy = false;
    c.youth = 0.0;
    c.animation = Some("idle".into());
    c.outfit.retain(|g| catalog.garment(g).is_some());
    c.outfit.dedup();
    let covered = c
        .outfit
        .iter()
        .any(|g| catalog.garment(g).is_some_and(|g| g.covers_anatomy));
    if !covered && catalog.garment("briefs").is_some() {
        c.outfit.insert(0, "briefs".into());
    }
    c.garment_colors.retain(|g, _| c.outfit.contains(g));
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_and_back() {
        let mut c = CharacterConfig::default();
        c.body.muscle = 0.9;
        c.hair = Some("short02".into());
        c.outfit = vec!["briefs".into(), "future-suit".into()];
        c.garment_colors
            .insert("future-suit".into(), "#c02020".into());
        let bytes = encode(&c);
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(text.len() < 400, "{text}");
        assert!(!text.contains("skinWetness"), "defaults stay out: {text}");
        assert_eq!(decode(&bytes), Some(c.clone()));
        assert_eq!(key(&c), key(&decode(&bytes).unwrap()));
        let mut d = c.clone();
        d.body.muscle = 0.8;
        assert_ne!(key(&c), key(&d));
        assert_eq!(decode(b"not json"), None);
    }
}
