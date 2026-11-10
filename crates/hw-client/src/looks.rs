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
    dye_suit(&mut c);
    c
}

/// A hero's suit is dyed cloth, not a print: coloured suit pieces without a
/// finish of their own get one (a finish dyes them one colour in Actors).
fn dye_suit(c: &mut CharacterConfig) {
    for g in &c.outfit {
        if !c.garment_colors.contains_key(g) || c.costume.finishes.contains_key(g) {
            continue;
        }
        if let Some((_, f)) = SUIT_FINISHES.iter().find(|(p, _)| g.starts_with(p)) {
            c.costume.finishes.insert(g.clone(), (*f).into());
        }
    }
}

/// Finishes for coloured suit pieces that have none: spandex for tights and
/// tops (the giraffe-print tank top read as a mottled chest), leather for
/// gloves and boots. Garment id prefixes.
const SUIT_FINISHES: [(&str, &str); 8] = [
    ("tights", "satin"),
    ("tank-top", "satin"),
    ("briefs", "satin"),
    ("turtleneck-halter", "satin"),
    ("gloves-", "leather"),
    ("gauntlet-gloves", "leather"),
    ("biker-boots", "leather"),
    ("leather-boots", "leather"),
];

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

    #[test]
    fn a_coloured_suit_is_dyed_but_a_chosen_finish_stays() {
        let mut c = CharacterConfig {
            outfit: vec![
                "briefs".into(),
                "tights".into(),
                "tank-top".into(),
                "biker-boots".into(),
            ],
            ..Default::default()
        };
        for g in ["tights", "tank-top", "biker-boots"] {
            c.garment_colors.insert(g.into(), "#c02020".into());
        }
        c.costume.finishes.insert("tights".into(), "cloth".into());
        dye_suit(&mut c);
        let r = c.costume.finishes;
        assert_eq!(r.get("tank-top").map(String::as_str), Some("satin"));
        assert_eq!(r.get("biker-boots").map(String::as_str), Some("leather"));
        assert_eq!(
            r.get("tights").map(String::as_str),
            Some("cloth"),
            "the player's pick"
        );
        assert_eq!(r.get("briefs"), None, "not coloured: its own look");
    }
}
