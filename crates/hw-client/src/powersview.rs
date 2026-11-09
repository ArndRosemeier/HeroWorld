//! A hero's progress on screen: the level and karma strip, the powers
//! window (B) where karma buys powers in the theme you like, the origin
//! card a new hero starts with, and the level-up moment.
//!
//! The rules live in `hw_sim::progress`; the host checks every purchase
//! again, so this only asks.

use crate::fightview::theme_ui_colour;
use engine::ui::egui;
use hw_net::{Power, Sheet};
use hw_sim::combat::power_spec;
use hw_sim::progress::{
    self, can_buy, discount, karma_for_level, level_of, themes, tier, tier_level, Affinity,
    BuyError, MAX_LEVEL, ORIGINS, REFUND,
};

/// How each faction regards the hero, one word each: a new hero is
/// regarded like any townsperson, and deeds move it (`hw_sim::factions`).
fn standing(ui: &mut egui::Ui, sheet: &Sheet) {
    use hw_sim::factions::{toward_hero, Attitude};
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new("Standing:")
                .size(13.0)
                .color(egui::Color32::from_gray(200)),
        );
        for f in hw_net::Faction::ALL {
            let a = Attitude::of(toward_hero(f, &sheet.regard));
            let colour = match a {
                Attitude::Hostile => egui::Color32::from_rgb(255, 90, 80),
                Attitude::Unfriendly => egui::Color32::from_rgb(255, 170, 90),
                Attitude::Neutral => egui::Color32::from_gray(185),
                Attitude::Friendly => egui::Color32::from_rgb(130, 220, 130),
                Attitude::Allied => GOLD,
            };
            ui.label(
                egui::RichText::new(format!("{} {}", f.name(), a.name().to_lowercase()))
                    .size(13.0)
                    .color(colour),
            );
            ui.add_space(8.0);
        }
    });
}

/// What the player asked for in the windows this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum PowerAction {
    Buy(Power, u8),
    GiveUp(Power),
    /// A new hero: origin picked, first power bought.
    Create(Sheet),
}

/// The powers a hero can buy, in the order the window lists them (by tier).
pub const SHOP: [Power; 15] = [
    Power::Bolt,
    Power::SuperJump,
    Power::Hold,
    Power::Shield,
    Power::Lift,
    Power::Flight,
    Power::Shockwave,
    Power::SuperSpeed,
    Power::Mend,
    Power::Beam,
    Power::Vitality,
    Power::Meteor,
    Power::Invulnerable,
    Power::Nova,
    Power::Rally,
];

const GOLD: egui::Color32 = egui::Color32::from_rgb(255, 214, 90);
const INK: egui::Color32 = egui::Color32::from_rgb(28, 30, 48);

pub struct PowersView {
    /// The powers window is open.
    pub open: bool,
    /// The theme picked for each power not owned yet (by code).
    picked: [u8; Power::ALL.len() + 1],
    /// A give-up click waiting for its second click.
    confirm: Option<Power>,
    /// The level-up banner: level, seconds shown.
    banner: Option<(u8, f32)>,
    /// The origin card: chosen origin and first power.
    origin: usize,
    first: (Power, u8),
}

impl Default for PowersView {
    fn default() -> Self {
        Self::new()
    }
}

impl PowersView {
    pub fn new() -> Self {
        Self {
            open: false,
            picked: [0; Power::ALL.len() + 1],
            confirm: None,
            banner: None,
            origin: 0,
            first: (Power::Bolt, 0),
        }
    }

    /// Our hero reached `level`: the banner.
    pub fn level_up(&mut self, level: u8) {
        self.banner = Some((level, 0.0));
    }

    pub fn update(&mut self, dt: f32) {
        if let Some(b) = &mut self.banner {
            b.1 += dt;
            if b.1 > 5.0 {
                self.banner = None;
            }
        }
    }

    /// Draws the strip, the windows and the banner. `fighting` is the
    /// level everyone fights at (the host's hero's). `creating` shows the
    /// origin card instead of the powers window.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        sheet: &Sheet,
        fighting: u8,
        creating: bool,
    ) -> Vec<PowerAction> {
        let mut out = Vec::new();
        self.strip(ctx, sheet, fighting);
        if creating {
            if let Some(s) = self.origin_card(ctx) {
                out.push(PowerAction::Create(s));
            }
        } else if self.open {
            self.window(ctx, sheet, &mut out);
        }
        self.banner(ctx, sheet);
        out
    }

    /// Level, karma to the next and karma to spend: a slim strip under
    /// the fight panel.
    fn strip(&self, ctx: &egui::Context, sheet: &Sheet, fighting: u8) {
        let screen = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("karma strip"),
        ));
        let level = level_of(sheet.lifetime);
        let w = 340.0;
        let r = egui::Rect::from_min_size(
            egui::pos2(screen.center().x - w * 0.5, screen.bottom() - 12.0),
            egui::vec2(w, 8.0),
        );
        painter.rect_filled(r.expand(1.0), 3.0, egui::Color32::from_black_alpha(190));
        let (from, to) = (karma_for_level(level), karma_for_level(level + 1));
        let f = if level >= MAX_LEVEL {
            1.0
        } else {
            (sheet.lifetime - from) as f32 / (to - from).max(1) as f32
        };
        painter.rect_filled(
            egui::Rect::from_min_size(r.min, egui::vec2(r.width() * f.clamp(0.0, 1.0), r.height())),
            3.0,
            egui::Color32::from_rgb(170, 120, 255),
        );
        let font = egui::FontId::proportional(12.0);
        painter.text(
            r.left_top() + egui::vec2(-8.0, 4.0),
            egui::Align2::RIGHT_CENTER,
            format!("Level {level}"),
            egui::FontId::proportional(14.0),
            GOLD,
        );
        let spend = if sheet.karma > 0 {
            format!("{} karma to spend · B", sheet.karma)
        } else {
            "B: powers".into()
        };
        painter.text(
            r.right_top() + egui::vec2(8.0, 4.0),
            egui::Align2::LEFT_CENTER,
            spend,
            font.clone(),
            egui::Color32::from_rgb(210, 190, 255),
        );
        if fighting != level {
            painter.text(
                r.center_top() - egui::vec2(0.0, 4.0),
                egui::Align2::CENTER_BOTTOM,
                format!("fighting at level {fighting} with your team"),
                font,
                egui::Color32::from_white_alpha(170),
            );
        }
    }

    fn window(&mut self, ctx: &egui::Context, sheet: &Sheet, out: &mut Vec<PowerAction>) {
        let level = level_of(sheet.lifetime);
        let mut open = self.open;
        egui::Window::new("Powers")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(640.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, -20.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("Level {level}"))
                            .size(24.0)
                            .color(GOLD)
                            .strong(),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new(format!("{} karma to spend", sheet.karma))
                            .size(18.0)
                            .color(egui::Color32::from_rgb(200, 170, 255)),
                    );
                    ui.add_space(12.0);
                    if level < MAX_LEVEL {
                        ui.label(format!(
                            "{} more to level {}",
                            karma_for_level(level + 1) - sheet.lifetime,
                            level + 1
                        ));
                    }
                });
                ui.label(
                    egui::RichText::new(format!(
                        "Origin: {}. Deeds lean you toward an affinity; its powers cost up to 30% less.",
                        ORIGINS[sheet.origin.min(4) as usize].name
                    ))
                    .size(13.0)
                    .color(egui::Color32::from_gray(200)),
                );
                ui.horizontal_wrapped(|ui| {
                    for a in Affinity::ALL {
                        let off = discount(sheet, a);
                        ui.label(
                            egui::RichText::new(format!("{} −{:.0}%", a.name(), off * 100.0))
                                .size(13.0)
                                .color(if off > 0.05 {
                                    GOLD
                                } else {
                                    egui::Color32::from_gray(170)
                                }),
                        );
                        ui.add_space(8.0);
                    }
                });
                standing(ui, sheet);
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(ctx.screen_rect().height() * 0.62)
                    .show(ui, |ui| {
                        for t in 1..=7u8 {
                            let opens = tier_level(t);
                            let head = if level >= opens {
                                format!("Tier {t}")
                            } else {
                                format!("Tier {t} · opens at level {opens}")
                            };
                            ui.label(
                                egui::RichText::new(head)
                                    .size(14.0)
                                    .strong()
                                    .color(if level >= opens {
                                        GOLD
                                    } else {
                                        egui::Color32::from_gray(150)
                                    }),
                            );
                            for p in SHOP.into_iter().filter(|p| tier(*p) == t) {
                                self.row(ui, sheet, p, out);
                                ui.add_space(4.0);
                            }
                        }
                    });
                ui.separator();
                ui.label(
                    egui::RichText::new(
                        "Powers grow with your level. Each power you own makes the next dearer; giving one up returns 90%.",
                    )
                    .size(12.0)
                    .color(egui::Color32::from_gray(170)),
                );
            });
        self.open = open;
    }

    fn row(&mut self, ui: &mut egui::Ui, sheet: &Sheet, p: Power, out: &mut Vec<PowerAction>) {
        let owned = sheet.has(p);
        let spec = power_spec(p);
        let t = tier(p);
        let opens = tier_level(t);
        let frame = egui::Frame::new()
            .fill(if owned {
                egui::Color32::from_rgb(44, 52, 86)
            } else {
                INK
            })
            .corner_radius(6.0)
            .inner_margin(egui::Margin::same(8));
        frame.show(ui, |ui| {
            ui.set_width(600.0);
            ui.horizontal(|ui| {
                let theme = if owned {
                    sheet.theme(p)
                } else {
                    self.picked[p.code() as usize]
                };
                let th = themes(p)[theme as usize];
                ui.label(
                    egui::RichText::new(th.name)
                        .size(18.0)
                        .strong()
                        .color(theme_ui_colour(p, theme)),
                );
                ui.label(
                    egui::RichText::new(spec.name)
                        .size(13.0)
                        .color(egui::Color32::from_gray(190)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if owned {
                        let back = (sheet
                            .powers
                            .iter()
                            .find(|o| o.power == p)
                            .map_or(0, |o| o.paid) as f64
                            * REFUND)
                            .floor() as u32;
                        let label = if self.confirm == Some(p) {
                            format!("Really give it up? (+{back})")
                        } else {
                            format!("Give up (+{back})")
                        };
                        if ui.button(label).clicked() {
                            if self.confirm == Some(p) {
                                out.push(PowerAction::GiveUp(p));
                                self.confirm = None;
                            } else {
                                self.confirm = Some(p);
                            }
                        }
                        ui.label(egui::RichText::new("owned").color(GOLD));
                    } else {
                        match can_buy(sheet, p, theme) {
                            Ok(price) => {
                                let b = egui::Button::new(
                                    egui::RichText::new(format!("Buy · {price} karma"))
                                        .strong()
                                        .color(INK),
                                )
                                .fill(GOLD);
                                if ui.add(b).clicked() {
                                    out.push(PowerAction::Buy(p, theme));
                                }
                            }
                            Err(BuyError::TooDear(price)) => {
                                ui.label(
                                    egui::RichText::new(format!("{price} karma"))
                                        .color(egui::Color32::from_gray(200)),
                                );
                            }
                            Err(BuyError::TooLow(l)) => {
                                ui.label(
                                    egui::RichText::new(format!("opens at level {l}"))
                                        .color(egui::Color32::from_gray(140)),
                                );
                            }
                            Err(_) => {}
                        }
                    }
                });
            });
            let theme = if owned {
                sheet.theme(p)
            } else {
                self.picked[p.code() as usize]
            };
            ui.label(
                egui::RichText::new(themes(p)[theme as usize].blurb)
                    .size(13.0)
                    .color(egui::Color32::from_gray(220)),
            );
            ui.label(
                egui::RichText::new(what_it_does(p))
                    .size(12.0)
                    .color(egui::Color32::from_gray(160)),
            );
            if !owned && level_of(sheet.lifetime) + 2 >= opens {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Theme:").size(12.0));
                    for k in [0u8, 1] {
                        let th = themes(p)[k as usize];
                        let tag = match th.affinity {
                            Some(a) => format!("{} ({})", th.name, a.name()),
                            None => th.name.to_string(),
                        };
                        let sel = self.picked[p.code() as usize] == k;
                        if ui.selectable_label(sel, tag).clicked() {
                            self.picked[p.code() as usize] = k;
                        }
                    }
                });
            }
        });
    }

    /// The card a new hero starts with: where the powers came from, and
    /// the first one.
    fn origin_card(&mut self, ctx: &egui::Context) -> Option<Sheet> {
        let mut made = None;
        egui::Window::new("Who are you?")
            .collapsible(false)
            .resizable(false)
            .default_width(560.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, -10.0))
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(
                        "Everyone starts as a normal person. Then the Aether touched you.",
                    )
                    .size(15.0),
                );
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("Your origin")
                        .size(17.0)
                        .color(GOLD)
                        .strong(),
                );
                for (k, o) in ORIGINS.iter().enumerate() {
                    let sel = self.origin == k;
                    let text = egui::RichText::new(format!(
                        "{}  ·  {} affinity\n{}",
                        o.name,
                        o.affinity.name(),
                        o.blurb
                    ))
                    .size(14.0);
                    if ui.selectable_label(sel, text).clicked() {
                        self.origin = k;
                    }
                }
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("Your first power")
                        .size(17.0)
                        .color(GOLD)
                        .strong(),
                );
                ui.horizontal_wrapped(|ui| {
                    for p in [Power::Bolt, Power::SuperJump] {
                        for k in 0..2u8 {
                            let th = themes(p)[k as usize];
                            let sel = self.first == (p, k);
                            let text = egui::RichText::new(th.name)
                                .size(15.0)
                                .color(theme_ui_colour(p, k));
                            if ui
                                .selectable_label(sel, text)
                                .on_hover_text(th.blurb)
                                .clicked()
                            {
                                self.first = (p, k);
                            }
                        }
                    }
                });
                let (p, k) = self.first;
                ui.label(
                    egui::RichText::new(format!(
                        "{} {}",
                        themes(p)[k as usize].blurb,
                        what_it_does(p)
                    ))
                    .size(13.0)
                    .color(egui::Color32::from_gray(200)),
                );
                ui.add_space(10.0);
                let go = egui::Button::new(
                    egui::RichText::new("Become a hero")
                        .size(18.0)
                        .strong()
                        .color(INK),
                )
                .fill(GOLD);
                if ui.add(go).clicked() {
                    let mut s = progress::new_hero(self.origin as u8);
                    let _ = progress::buy(&mut s, p, k);
                    made = Some(s);
                }
            });
        made
    }

    /// The level-up moment: a big golden banner and what just opened.
    fn banner(&self, ctx: &egui::Context, sheet: &Sheet) {
        let Some((level, t)) = self.banner else {
            return;
        };
        let screen = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("level up"),
        ));
        let pop = 1.0 + 0.6 * (1.0 - (t * 4.0).min(1.0)).powi(2);
        let a = if t > 4.0 { 1.0 - (t - 4.0) } else { 1.0 }.clamp(0.0, 1.0);
        let c = screen.center() - egui::vec2(0.0, screen.height() * 0.22);
        // Sun rays behind the words.
        for k in 0..16 {
            let ang = k as f32 / 16.0 * std::f32::consts::TAU + t * 0.4;
            let d = egui::vec2(ang.cos(), ang.sin());
            let n = egui::vec2(-d.y, d.x);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    c + d * 40.0 + n * 6.0,
                    c + d * 240.0 * pop + n * 22.0,
                    c + d * 240.0 * pop - n * 22.0,
                    c + d * 40.0 - n * 6.0,
                ],
                egui::Color32::from_rgba_unmultiplied(255, 214, 90, (40.0 * a) as u8),
                egui::Stroke::NONE,
            ));
        }
        let big = egui::FontId::proportional(64.0 * pop);
        let text = format!("LEVEL {level}!");
        painter.text(
            c + egui::vec2(4.0, 4.0),
            egui::Align2::CENTER_CENTER,
            &text,
            big.clone(),
            egui::Color32::from_black_alpha((200.0 * a) as u8),
        );
        painter.text(
            c,
            egui::Align2::CENTER_CENTER,
            &text,
            big,
            GOLD.gamma_multiply(a),
        );
        // What opened: a new tier, and what can be bought now.
        let mut lines = vec!["Health, damage and karma to spend grow".to_string()];
        if let Some(t) = (1..=7).find(|&t| tier_level(t) == level) {
            let names: Vec<&str> = SHOP
                .iter()
                .filter(|p| tier(**p) == t)
                .map(|p| power_spec(*p).name)
                .collect();
            if !names.is_empty() {
                lines.push(format!("Tier {t} opens: {}. Press B.", names.join(", ")));
            }
        }
        let affordable = SHOP
            .iter()
            .filter(|p| can_buy(sheet, **p, 0).is_ok() || can_buy(sheet, **p, 1).is_ok())
            .count();
        if affordable > 0 {
            lines.push(format!("{} karma: enough for a new power", sheet.karma));
        }
        for (k, l) in lines.iter().enumerate() {
            painter.text(
                c + egui::vec2(0.0, 58.0 + k as f32 * 24.0),
                egui::Align2::CENTER_CENTER,
                l,
                egui::FontId::proportional(19.0),
                egui::Color32::from_white_alpha((235.0 * a) as u8),
            );
        }
    }
}

/// One line on what a power does, from its numbers.
pub fn what_it_does(p: Power) -> String {
    let s = power_spec(p);
    match p {
        Power::Strike => "A punch: free, quick.".into(),
        Power::Bolt => format!(
            "Ranged blast, {:.0} m, knocks back. {:.0} energy.",
            s.range, s.cost
        ),
        Power::SuperJump => "Passive: jump seven metres, leap along fast.".into(),
        Power::Flight => "Passive: V to fly, Shift to go fast.".into(),
        Power::Hold => format!(
            "Pins a villain in place for {:.0} s. {:.0} energy.",
            s.lasts, s.cost
        ),
        Power::Shield => format!(
            "Halves blows to you and friends within {:.0} m for {:.0} s.",
            s.area, s.lasts
        ),
        Power::Lift => "Lift a crate (or tear up the street) and throw it: heavy hit.".into(),
        Power::Shockwave => format!(
            "Knocks down every villain within {:.0} m. {:.0} energy.",
            s.area, s.cost
        ),
        Power::SuperSpeed => format!(
            "Passive: run at {:.0} km/h, Shift for {:.0}.",
            hw_sim::SUPER_SPEED * 3.6,
            hw_sim::SUPER_SPEED_BOOST * 3.6
        ),
        Power::Mend => format!(
            "Heals you and friends within {:.0} m. {:.0} energy.",
            s.area, s.cost
        ),
        Power::Beam => format!(
            "Pierces every villain in a {:.0} m line, knocks back. {:.0} energy.",
            s.range, s.cost
        ),
        Power::Vitality => format!(
            "Passive: {:.0}% of your health back every second, even in a fight.",
            hw_sim::combat::VITALITY_REGEN * 100.0
        ),
        Power::Meteor => format!(
            "Calls a strike down on your target: hits all within {:.0} m a moment later. {:.0} energy.",
            s.area, s.cost
        ),
        Power::Invulnerable => format!(
            "Passive: every blow does {:.0}% less.",
            (1.0 - hw_sim::combat::TOUGH_LETS_THROUGH) * 100.0
        ),
        Power::Nova => format!(
            "Flattens every villain within {:.0} m. {:.0} energy.",
            s.area, s.cost
        ),
        Power::Rally => format!(
            "Passive: you and friends within {:.0} m hit {:.0}% harder and recharge faster.",
            s.area,
            (hw_sim::combat::RALLY_DAMAGE - 1.0) * 100.0
        ),
    }
}
