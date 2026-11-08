//! The hero creator: the Actors web creator's sections, made for the game.
//!
//! Four sections (body, face and hair, costume, colours), each with a
//! Simple tab (a few presets, enough to make a hero in a minute) and a
//! Detailed tab (every slider). It edits an Actors `CharacterConfig` in
//! what the hero catalog allows (see [`crate::people::hero_catalog`]); the
//! title screen draws the card around it and the live preview beside it.

use crate::looks;
use crate::people;
use actors_generator::{outfit_garments, tables, CharacterCatalog, CharacterConfig, SkinKind};
use actors_human::{body_types, shape_sliders, HumanParams, ADULT_MIN_AGE, MAX_AGE};
use engine::ui::egui::{self, vec2, Color32, RichText, Stroke};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Body,
    Face,
    Costume,
    Colours,
}

pub const SECTIONS: [(Section, &str); 4] = [
    (Section::Body, "Body"),
    (Section::Face, "Face & hair"),
    (Section::Costume, "Costume"),
    (Section::Colours, "Colours"),
];

const GENDERS: [(&str, f64); 3] = [("Female", 0.0), ("Androgynous", 0.5), ("Male", 1.0)];
const AGES: [(&str, f64); 4] = [
    ("Young (22)", 22.0),
    ("Adult (35)", 35.0),
    ("Middle-aged (50)", 50.0),
    ("Old (70)", 70.0),
];
const HEIGHTS: [(&str, f64); 3] = [("Short", 0.2), ("Average", 0.5), ("Tall", 0.8)];
const ETHNICITIES: [(&str, [f64; 3]); 4] = [
    ("African", [1.0, 0.0, 0.0]),
    ("Asian", [0.0, 1.0, 0.0]),
    ("Caucasian", [0.0, 0.0, 1.0]),
    ("Mixed", [1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0]),
];
/// Detail slider groups that belong to the face (the rest go with the body).
const FACE_GROUPS: [&str; 10] = [
    "head", "eyes", "eyebrows", "nose", "mouth", "ears", "chin", "cheek", "forehead", "asym",
];

/// The creator's state: the looks being made and how the screen is set.
pub struct Creator {
    pub config: CharacterConfig,
    catalog: Arc<CharacterCatalog>,
    pub section: Section,
    /// Detailed tab per section (else Simple).
    detailed: [bool; 4],
    /// The preview strikes the heroic pose (else stands at ease).
    pub heroic: bool,
    /// How far the preview is turned, in degrees (drag it round).
    pub spin: f32,
    rolls: u32,
}

impl Creator {
    /// Starts from `config` (kept to the hero rules).
    pub fn new(catalog: Arc<CharacterCatalog>, config: &CharacterConfig) -> Self {
        Self {
            config: looks::hero_rules(&catalog, config),
            catalog,
            section: Section::Body,
            detailed: [false; 4],
            heroic: true,
            spin: 0.0,
            rolls: 0,
        }
    }

    /// A random hero dressed for the era (same `seed`, same hero): most
    /// in one of the game's hero costumes, the rest in their street clothes.
    pub fn random_look(catalog: &CharacterCatalog, seed: u32) -> CharacterConfig {
        let mut c = people::golden_age(catalog, seed, true);
        let mut rng = hw_core::Seed(seed as u64).child("costume").rng();
        if rng.chance(0.75) {
            let female = c.body.gender < 0.5;
            let fits: Vec<_> = catalog
                .outfits
                .iter()
                .filter(|o| o.id.starts_with("hw-"))
                .filter(|o| match o.gender.as_deref() {
                    Some("female") => female,
                    Some("male") => !female,
                    _ => true,
                })
                .collect();
            if !fits.is_empty() {
                let o = *rng.pick(&fits);
                c.outfit = outfit_garments(o, &c.body);
                c.garment_colors = o.colors.clone().unwrap_or_default();
            }
        }
        looks::hero_rules(catalog, &c)
    }

    /// A new random hero.
    pub fn roll(&mut self, seed: u32) {
        self.rolls += 1;
        self.config = Self::random_look(&self.catalog, seed.wrapping_add(self.rolls));
    }

    /// The looks as they stand, kept to the hero rules.
    pub fn look(&self) -> CharacterConfig {
        looks::hero_rules(&self.catalog, &self.config)
    }

    /// Puts the open section on its Detailed (or Simple) tab.
    pub fn set_detailed(&mut self, on: bool) {
        let t = self.tab();
        self.detailed[t] = on;
    }

    fn tab(&self) -> usize {
        SECTIONS
            .iter()
            .position(|(s, _)| *s == self.section)
            .unwrap_or(0)
    }

    /// The section tabs and the Simple/Detailed switch.
    pub fn tabs(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().button_padding = vec2(9.0, 5.0);
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.horizontal_wrapped(|ui| {
            for (s, label) in SECTIONS {
                let on = self.section == s;
                if ui
                    .add(egui::Button::new(label).selected(on).fill(if on {
                        Color32::from_rgb(255, 198, 64)
                    } else {
                        Color32::from_rgb(240, 230, 205)
                    }))
                    .clicked()
                {
                    self.section = s;
                }
            }
        });
        let t = self.tab();
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.detailed[t], false, "Simple");
            ui.selectable_value(&mut self.detailed[t], true, "Detailed");
        });
    }

    /// The open section's controls. Returns true when the looks changed.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> bool {
        let before = self.config.clone();
        let detailed = self.detailed[self.tab()];
        let cat = self.catalog.clone();
        match self.section {
            Section::Body => body(ui, &cat, &mut self.config, detailed),
            Section::Face => face(ui, &cat, &mut self.config, detailed),
            Section::Costume => costume(ui, &cat, &mut self.config, detailed),
            Section::Colours => colours(ui, &cat, &mut self.config, detailed),
        }
        self.config != before
    }
}

/// A row of preset buttons; the one nearest `current` (within `tol`) lit.
fn presets<T: Copy>(
    ui: &mut egui::Ui,
    label: &str,
    list: &[(&str, T)],
    lit: impl Fn(T) -> bool,
) -> Option<T> {
    let mut picked = None;
    ui.label(RichText::new(label).strong());
    ui.horizontal_wrapped(|ui| {
        for (name, v) in list {
            if ui.selectable_label(lit(*v), *name).clicked() {
                picked = Some(*v);
            }
        }
    });
    picked
}

fn slider(ui: &mut egui::Ui, label: &str, v: &mut f64, range: std::ops::RangeInclusive<f64>) {
    ui.horizontal(|ui| {
        ui.add_sized([130.0, 20.0], egui::Label::new(label).truncate());
        ui.add(egui::Slider::new(v, range).show_value(false));
    });
}

fn combo(ui: &mut egui::Ui, label: &str, current: &mut String, options: &[(String, String)]) {
    let shown = options
        .iter()
        .find(|(id, _)| id == current)
        .map_or("Custom", |(_, l)| l.as_str())
        .to_string();
    ui.label(RichText::new(label).strong());
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt(label)
            .selected_text(shown)
            .width(300.0)
            .height(320.0)
            .show_ui(ui, |ui| {
                for (id, l) in options {
                    ui.selectable_value(current, id.clone(), l);
                }
            });
    });
}

fn body_field<'a>(b: &'a mut HumanParams, key: &str) -> Option<&'a mut f64> {
    Some(match key {
        "gender" => &mut b.gender,
        "age" | "ageYears" | "age_years" => &mut b.age_years,
        "muscle" => &mut b.muscle,
        "weight" => &mut b.weight,
        "height" => &mut b.height,
        "proportions" => &mut b.proportions,
        "cupSize" | "cup_size" => &mut b.cup_size,
        "firmness" => &mut b.firmness,
        "african" => &mut b.african,
        "asian" => &mut b.asian,
        "caucasian" => &mut b.caucasian,
        _ => return None,
    })
}

fn body(ui: &mut egui::Ui, cat: &CharacterCatalog, c: &mut CharacterConfig, detailed: bool) {
    let b = c.body;
    if let Some(g) = presets(ui, "Gender", &GENDERS, |v| (b.gender - v).abs() < 0.15) {
        c.body.gender = g;
    }
    if !detailed {
        if let Some(a) = presets(ui, "Age", &AGES, |v| (b.age_years - v).abs() < 6.0) {
            c.body.age_years = a;
        }
        // Body types set the build and the body's shape sliders together.
        let same = |t: &actors_human::BodyType| {
            let mut b2 = c.body;
            t.body
                .iter()
                .all(|(k, v)| body_field(&mut b2, k).is_some_and(|x| (*x - v).abs() < 0.005))
                && t.shape
                    .iter()
                    .all(|(k, v)| (c.shape.get(k).copied().unwrap_or(0.0) - v).abs() < 0.005)
        };
        let mut current = body_types()
            .iter()
            .find(|t| same(t))
            .map(|t| t.id.clone())
            .unwrap_or_default();
        let options: Vec<(String, String)> = body_types()
            .iter()
            .map(|t| (t.id.clone(), t.label.clone()))
            .collect();
        let was = current.clone();
        combo(ui, "Build", &mut current, &options);
        if current != was {
            if let Some(t) = body_types().iter().find(|t| t.id == current) {
                for (k, v) in &t.body {
                    if let Some(x) = body_field(&mut c.body, k) {
                        *x = *v;
                    }
                }
                c.shape.retain(|k, _| {
                    !shape_sliders()
                        .iter()
                        .any(|s| s.id == *k && s.group == "Body")
                });
                for (k, v) in &t.shape {
                    c.shape.insert(k.clone(), *v);
                }
            }
        }
        if let Some(h) = presets(ui, "Height", &HEIGHTS, |v| (b.height - v).abs() < 0.15) {
            c.body.height = h;
        }
        let sum = (b.african + b.asian + b.caucasian).max(1e-6);
        let mix = [b.african / sum, b.asian / sum, b.caucasian / sum];
        if let Some(e) = presets(ui, "Heritage", &ETHNICITIES, |w| {
            w.iter().zip(mix).all(|(a, b)| (a - b).abs() < 0.02)
        }) {
            [c.body.african, c.body.asian, c.body.caucasian] = e;
        }
        return;
    }
    ui.label(RichText::new("Build").strong());
    let p = &mut c.body;
    slider(ui, "Age", &mut p.age_years, ADULT_MIN_AGE..=MAX_AGE);
    slider(ui, "Muscle", &mut p.muscle, 0.0..=1.0);
    slider(ui, "Weight", &mut p.weight, 0.0..=1.0);
    slider(ui, "Height", &mut p.height, 0.0..=1.0);
    slider(ui, "Proportions", &mut p.proportions, 0.0..=1.0);
    slider(ui, "Female ↔ Male", &mut p.gender, 0.0..=1.0);
    if p.gender < 0.75 {
        slider(ui, "Bust", &mut p.cup_size, 0.0..=1.0);
        slider(ui, "Bust firmness", &mut p.firmness, 0.0..=1.0);
    }
    ui.label(RichText::new("Heritage").strong());
    slider(ui, "African", &mut p.african, 0.0..=1.0);
    slider(ui, "Asian", &mut p.asian, 0.0..=1.0);
    slider(ui, "Caucasian", &mut p.caucasian, 0.0..=1.0);
    shape(ui, c, "Body");
    ui.label(RichText::new("Skin").strong());
    ui.horizontal(|ui| {
        ui.selectable_value(&mut c.skin, SkinKind::Photo, "Natural");
        ui.selectable_value(&mut c.skin, SkinKind::Procedural, "Painted");
    });
    if c.skin == SkinKind::Procedural {
        slider(ui, "Skin tone", &mut c.skin_tone, 0.0..=1.0);
    }
    fine_detail(ui, cat, c, false);
}

/// The plain-language shape sliders of one group ("Body" or "Face").
fn shape(ui: &mut egui::Ui, c: &mut CharacterConfig, group: &str) {
    ui.label(RichText::new("Shape").strong());
    for s in shape_sliders().iter().filter(|s| s.group == group) {
        let mut v = c.shape.get(&s.id).copied().unwrap_or(0.0);
        let was = v;
        ui.horizontal(|ui| {
            ui.add_sized([130.0, 20.0], egui::Label::new(&s.label).truncate())
                .on_hover_text(format!("{} ↔ {}", s.low, s.high));
            ui.add(egui::Slider::new(&mut v, -1.0..=1.0).show_value(false));
        });
        if v != was {
            if v.abs() < 0.005 {
                c.shape.shift_remove(&s.id);
            } else {
                c.shape.insert(s.id.clone(), v);
            }
        }
    }
}

/// The raw detail sliders, by group, folded: the face's or the body's.
fn fine_detail(ui: &mut egui::Ui, cat: &CharacterCatalog, c: &mut CharacterConfig, face: bool) {
    let cat_details = details_of(cat, face);
    if cat_details.is_empty() {
        return;
    }
    ui.label(RichText::new("Fine detail").strong());
    let mut group = String::new();
    let mut by_group: Vec<(String, Vec<DetailRow>)> = Vec::new();
    for d in cat_details {
        if d.group != group {
            group = d.group.clone();
            by_group.push((group.clone(), Vec::new()));
        }
        if let Some(last) = by_group.last_mut() {
            last.1.push(d);
        }
    }
    for (g, rows) in by_group {
        let title = if g == "asym" {
            "Asymmetry".to_string()
        } else {
            cap(&g)
        };
        egui::CollapsingHeader::new(title)
            .id_salt(("detail", face, g.clone()))
            .show(ui, |ui| {
                for d in rows {
                    let mut v = c.details.get(&d.id).copied().unwrap_or(0.0);
                    let was = v;
                    slider(ui, &d.label, &mut v, d.min..=d.max);
                    if v != was {
                        c.details.insert(d.id.clone(), v);
                    }
                }
            });
    }
}

/// One raw detail slider.
struct DetailRow {
    id: String,
    label: String,
    group: String,
    min: f64,
    max: f64,
}

/// The catalog's raw detail sliders of the face or of the body, leaving
/// out what only matters undressed.
fn details_of(cat: &CharacterCatalog, face: bool) -> Vec<DetailRow> {
    cat.details
        .iter()
        .filter(|x| {
            let g = x.group.to_lowercase();
            let private = g == "genitals" || x.id.contains("bulge");
            !private && FACE_GROUPS.contains(&g.as_str()) == face
        })
        .map(|x| DetailRow {
            id: x.id.clone(),
            label: x.label.clone(),
            group: x.group.clone(),
            min: x.min,
            max: x.max,
        })
        .collect()
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn face(ui: &mut egui::Ui, cat: &CharacterCatalog, c: &mut CharacterConfig, detailed: bool) {
    let mut hair = c.hair.clone().unwrap_or_default();
    let mut options = vec![(String::new(), "None".to_string())];
    options.extend(cat.hair.iter().map(|h| (h.id.clone(), h.label.clone())));
    combo(ui, "Hair", &mut hair, &options);
    c.hair = (!hair.is_empty()).then_some(hair);
    ui.label(RichText::new("Hair colour").strong());
    let mut hc = c.hair_color.clone();
    swatches(
        ui,
        &mut hc,
        cat.hair_colors.iter().map(|h| (&h.id, &h.label)),
        "Natural",
    );
    c.hair_color = hc;
    let beards: Vec<(String, String)> = tables()
        .beard_styles
        .iter()
        .map(|b| (b.clone(), cap(b)))
        .collect();
    combo(ui, "Beard", &mut c.beard, &beards);
    let eyes: Vec<(String, String)> = cat
        .eyes
        .iter()
        .map(|e| (e.id.clone(), e.label.clone()))
        .collect();
    let was = c.eyes.clone();
    combo(ui, "Eyes", &mut c.eyes, &eyes);
    if c.eyes != was {
        c.eye_color = None;
    }
    if !detailed {
        return;
    }
    slider(ui, "Stubble", &mut c.stubble, 0.0..=1.0);
    let mut brows = c.brows.clone().unwrap_or_default();
    let mut options: Vec<(String, String)> = cat
        .brows
        .iter()
        .map(|b| (b.id.clone(), b.label.clone()))
        .collect();
    options.push((tables().painted_brows.clone(), "Painted".into()));
    options.push((String::new(), "None".into()));
    combo(ui, "Eyebrows", &mut brows, &options);
    c.brows = (!brows.is_empty()).then_some(brows);
    ui.horizontal(|ui| {
        ui.add_sized([130.0, 20.0], egui::Label::new("Eye colour"));
        colour_pick(ui, &mut c.eye_color, "#5a7a9a");
    });
    shape(ui, c, "Face");
    fine_detail(ui, cat, c, true);
}

fn costume(ui: &mut egui::Ui, cat: &CharacterCatalog, c: &mut CharacterConfig, detailed: bool) {
    let same = |a: &[String], b: &[String]| a.len() == b.len() && a.iter().all(|x| b.contains(x));
    let mut current = cat
        .outfits
        .iter()
        .find(|o| same(&outfit_garments(o, &c.body), &c.outfit))
        .map(|o| o.id.clone())
        .unwrap_or_default();
    let options: Vec<(String, String)> = cat
        .outfits
        .iter()
        .map(|o| (o.id.clone(), o.label.clone()))
        .collect();
    let was = current.clone();
    combo(ui, "Outfit", &mut current, &options);
    if let Some(o) = cat.outfit(&current) {
        if current != was {
            c.outfit = outfit_garments(o, &c.body);
            c.garment_colors.clear();
            for (g, col) in o.colors.iter().flatten() {
                c.garment_colors.insert(g.clone(), col.clone());
            }
        }
        if let Some(d) = &o.description {
            ui.label(RichText::new(d).italics());
        }
    }
    if !detailed {
        ui.label("Detailed picks single pieces: a hat, gloves, boots, glasses.");
        return;
    }
    ui.checkbox(&mut c.cloth_motion, "Loose clothes swing");
    let mut cats: Vec<&str> = Vec::new();
    for g in &cat.garments {
        if !cats.contains(&g.category.as_str()) {
            cats.push(&g.category);
        }
    }
    for k in cats {
        let worn = cat
            .garments
            .iter()
            .any(|g| g.category == k && c.outfit.contains(&g.id));
        egui::CollapsingHeader::new(cap(k))
            .id_salt(("garments", k))
            .default_open(worn)
            .show(ui, |ui| {
                for g in cat.garments.iter().filter(|g| g.category == k) {
                    let mut on = c.outfit.contains(&g.id);
                    if ui.checkbox(&mut on, &g.label).changed() {
                        c.outfit.retain(|x| *x != g.id);
                        if on {
                            c.outfit.push(g.id.clone());
                        } else {
                            c.garment_colors.shift_remove(&g.id);
                        }
                    }
                }
            });
    }
}

fn colours(ui: &mut egui::Ui, cat: &CharacterCatalog, c: &mut CharacterConfig, detailed: bool) {
    ui.label("A colour for each piece you wear.");
    let worn = c.outfit.clone();
    for id in worn {
        let Some(g) = cat.garment(&id) else { continue };
        ui.label(RichText::new(&g.label).strong());
        let mut col = c.garment_colors.get(&id).cloned();
        ui.horizontal_wrapped(|ui| {
            swatches(
                ui,
                &mut col,
                cat.garment_colors.iter().map(|h| (&h.id, &h.label)),
                "Original",
            );
            if detailed {
                colour_pick(ui, &mut col, "#c02828");
            }
        });
        match col {
            Some(v) => {
                c.garment_colors.insert(id.clone(), v);
            }
            None => {
                c.garment_colors.shift_remove(&id);
            }
        }
    }
    if detailed {
        ui.separator();
        ui.label(RichText::new("Skin colour").strong());
        ui.horizontal(|ui| {
            let mut sc = c.skin_color.clone();
            if ui.selectable_label(sc.is_none(), "Natural").clicked() {
                sc = None;
            }
            colour_pick(ui, &mut sc, "#c8a080");
            if sc != c.skin_color {
                c.skin_color = sc;
                if c.skin_color.is_some() {
                    c.skin = SkinKind::Procedural;
                }
            }
        });
    }
}

/// Colour swatches: `none_label` (no colour of our own) and the palette.
fn swatches<'a>(
    ui: &mut egui::Ui,
    current: &mut Option<String>,
    palette: impl Iterator<Item = (&'a String, &'a String)>,
    none_label: &str,
) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        if ui.selectable_label(current.is_none(), none_label).clicked() {
            *current = None;
        }
        for (hex, label) in palette {
            let Some(rgb) = parse_hex(hex) else { continue };
            let on = current.as_deref() == Some(hex.as_str());
            let (rect, r) = ui.allocate_exact_size(vec2(22.0, 22.0), egui::Sense::click());
            let p = ui.painter();
            p.rect_filled(rect, 5.0, Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
            let stroke = if on {
                Stroke::new(3.0_f32, Color32::from_rgb(226, 52, 48))
            } else {
                Stroke::new(1.0_f32, Color32::from_rgb(22, 26, 64))
            };
            p.rect_stroke(rect, 5.0, stroke, egui::StrokeKind::Inside);
            if r.on_hover_text(label.as_str()).clicked() {
                *current = Some(hex.clone());
            }
        }
    });
}

/// A free colour picker for an optional sRGB hex colour.
fn colour_pick(ui: &mut egui::Ui, current: &mut Option<String>, fallback: &str) {
    let mut rgb = current
        .as_deref()
        .and_then(parse_hex)
        .or_else(|| parse_hex(fallback))
        .unwrap_or([128, 128, 128]);
    if ui.color_edit_button_srgb(&mut rgb).changed() {
        *current = Some(format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]));
    }
}

pub fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colours() {
        assert_eq!(parse_hex("#ff8000"), Some([255, 128, 0]));
        assert_eq!(parse_hex("ff8000"), None);
        assert_eq!(parse_hex("#ff80"), None);
    }

    #[test]
    fn body_types_fill_known_fields() {
        let mut b = HumanParams::default();
        for t in body_types() {
            for k in t.body.keys() {
                assert!(body_field(&mut b, k).is_some(), "unknown body key {k}");
            }
        }
    }
}
