//! A hero's progress on screen: the level and karma strip, the powers
//! window (B) with the hero's origin tree and the travel tree, the origin
//! card a new hero starts with (origin and element), the element card an
//! older hero picks once, and the level-up moment.
//!
//! The rules live in `hw_sim::progress` and `hw_sim::powers`; the host
//! checks every purchase again, so this only asks.

use crate::augmentview::{self, AugmentAction, AugmentUi};
use crate::fightview::{element_ui_colour, power_ui_colour};
use crate::tray::{self, Tray};
use engine::ui::egui;
use hw_net::{PowerId, Sheet};
use hw_sim::combat;
use hw_sim::powers::{self, def_of, Delivery, Element, Lands, PowerDef, Tree};
use hw_sim::progress::{
    self, can_buy, can_give_up, karma_for_level, level_of, BuyError, MAX_LEVEL, ORIGINS, REFUND,
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
    Buy(PowerId),
    GiveUp(PowerId),
    /// A new hero: origin and element picked, first power bought.
    Create(Sheet),
    /// A hero from before the power trees picked their element.
    Element(Element),
    /// An augment set into a socket, or traded in.
    Augment(AugmentAction),
}

const GOLD: egui::Color32 = egui::Color32::from_rgb(255, 214, 90);
const INK: egui::Color32 = egui::Color32::from_rgb(28, 30, 48);
const HELPER: egui::Color32 = egui::Color32::from_rgb(120, 220, 140);

/// A node of the tree on screen (px).
const NODE_W: f32 = 132.0;
const NODE_H: f32 = 46.0;
const COL_STEP: f32 = 150.0;
const ROW_STEP: f32 = 74.0;
const COLUMNS: usize = 5;
const ROWS: usize = 6;

pub struct PowersView {
    /// The powers window is open.
    pub open: bool,
    /// The tree shown: the hero's origin's (false) or travel (true).
    travel: bool,
    /// The power picked in the tree (its details below it).
    selected: Option<PowerId>,
    /// A give-up click waiting for its second click.
    confirm: Option<PowerId>,
    /// The level-up banner: level, seconds shown.
    banner: Option<(u8, f32)>,
    /// The origin card: chosen origin and element.
    origin: usize,
    element: Element,
    /// Which key fires which power (kept in line by the game, shown and
    /// changed here).
    pub tray: Tray,
    /// The sockets and the bag.
    augments: AugmentUi,
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
            travel: false,
            selected: None,
            confirm: None,
            banner: None,
            origin: 0,
            element: Element::Fire,
            tray: Tray::default(),
            augments: AugmentUi::default(),
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
    /// origin card instead of the powers window; a hero with no element
    /// yet (from before the power trees) gets the element card.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        sheet: &Sheet,
        fighting: u8,
        creating: bool,
    ) -> Vec<PowerAction> {
        let mut out = Vec::new();
        self.augments.sync(sheet);
        self.strip(ctx, sheet, fighting);
        if creating {
            if let Some(s) = self.origin_card(ctx) {
                out.push(PowerAction::Create(s));
            }
        } else if Element::from_code(sheet.element) == Element::None {
            if let Some(e) = self.element_card(ctx, sheet) {
                out.push(PowerAction::Element(e));
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
            egui::pos2(screen.center().x - w * 0.5, screen.bottom() - 13.0),
            egui::vec2(w, 9.0),
        );
        let (from, to) = (karma_for_level(level), karma_for_level(level + 1));
        let f = if level >= MAX_LEVEL {
            1.0
        } else {
            (sheet.lifetime - from) as f32 / (to - from).max(1) as f32
        };
        crate::uikit::gauge(
            &painter,
            r,
            f,
            egui::Color32::from_rgb(170, 120, 255),
            crate::uikit::now(ctx),
            0.0,
        );
        let font = egui::FontId::proportional(12.0);
        crate::uikit::shadow_text(
            &painter,
            r.left_top() + egui::vec2(-10.0, 4.0),
            egui::Align2::RIGHT_CENTER,
            &format!("LEVEL {level}"),
            crate::uikit::ui_font(ctx, 14.0),
            GOLD,
        );
        let spend = if sheet.karma > 0 {
            format!("{} karma to spend · B", sheet.karma)
        } else {
            "B: powers".into()
        };
        crate::uikit::shadow_text(
            &painter,
            r.right_top() + egui::vec2(10.0, 4.0),
            egui::Align2::LEFT_CENTER,
            &spend,
            font.clone(),
            egui::Color32::from_rgb(220, 200, 255),
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
        let origin = &ORIGINS[(sheet.origin as usize).min(ORIGINS.len() - 1)];
        let element = Element::from_code(sheet.element);
        let mut open = self.open;
        crate::googie::window(
            ctx,
            egui::Window::new("Powers")
                .collapsible(false)
                .resizable(false)
                .default_width(COL_STEP * COLUMNS as f32 + augmentview::BAG_WIDTH + 40.0)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, -20.0)),
            "Powers",
            Some(&mut open),
            |ui| {
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
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(format!("Origin: {}.", origin.name))
                            .size(13.0)
                            .color(egui::Color32::from_gray(210)),
                    );
                    ui.label(
                        egui::RichText::new(format!("Element: {}", element.name()))
                            .size(13.0)
                            .strong()
                            .color(element_ui_colour(element)),
                    );
                    ui.label(
                        egui::RichText::new(format!("({}).", element.blurb()))
                            .size(13.0)
                            .color(egui::Color32::from_gray(190)),
                    );
                });
                standing(ui, sheet);
                ui.separator();
                // Two big tabs: the origin's own powers, and the travel powers
                // every hero can buy (they were easy to miss as small text).
                ui.horizontal(|ui| {
                    let tab = |text: String| {
                        egui::RichText::new(text)
                            .size(17.0)
                            .strong()
                            .color(egui::Color32::from_rgb(255, 214, 90))
                    };
                    let mine = tab(format!("{} powers", origin.tree.name()));
                    if ui.selectable_label(!self.travel, mine).clicked() {
                        self.travel = false;
                    }
                    ui.add_space(12.0);
                    let travel = tab("Travel powers: Flight, Super Jump, Super Speed ...".into());
                    if ui.selectable_label(self.travel, travel).clicked() {
                        self.travel = true;
                    }
                });
                ui.label(
                    egui::RichText::new(if self.travel {
                        "Travel powers are open to every origin, and cost karma like any other."
                    } else {
                        "Getting about (Flight, Super Jump, Super Speed, Glide, Wall Run) is on the Travel powers tab."
                    })
                    .size(12.5)
                    .color(egui::Color32::from_gray(190)),
                );
                let tree = if self.travel {
                    Tree::Common
                } else {
                    origin.tree
                };
                // A free power picked below stays picked on either tab.
                if self
                    .selected
                    .and_then(powers::get)
                    .is_none_or(|d| d.tree != tree && !powers::is_free(d.id))
                {
                    self.selected = powers::tree(tree).next().map(|d| d.id);
                }
                // The tree and the picked power on the left, the bag of
                // augments beside them (below, the window grew too tall).
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(COL_STEP * COLUMNS as f32);
                        self.tree(ui, sheet, tree);
                        ui.separator();
                        if let Some(d) = self.selected.and_then(powers::get) {
                            self.details(ui, sheet, d, out);
                        }
                    });
                    // (A separator here would stretch the window to the
                    // screen's height.)
                    ui.add_space(10.0);
                    ui.vertical(|ui| {
                        ui.set_width(augmentview::BAG_WIDTH);
                        let mut asked = Vec::new();
                        self.augments.bag(ui, sheet, &mut asked);
                        out.extend(asked.into_iter().map(PowerAction::Augment));
                    });
                });
                ui.separator();
                ui.label(
                    egui::RichText::new(
                        "A power needs the ones its arrows come from. Where two powers say \"or\", you pick one for good. \
                         Each power you own makes the next dearer; giving one up returns 90%.",
                    )
                    .size(12.0)
                    .color(egui::Color32::from_gray(170)),
                );
                ui.separator();
                // Every hero's own, free: Strike, Super Jump, Sprint and the portal.
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new("Every hero's, free:")
                            .size(13.0)
                            .strong()
                            .color(egui::Color32::from_gray(220)),
                    );
                    for p in PowerId::FREE {
                        let d = def_of(p);
                        // A click picks it, like a power of the tree: its
                        // sockets show above. Lit when what is in hand fits.
                        let lit = self.augments.fits_in_hand(ui.ctx(), sheet, d);
                        let text = egui::RichText::new(d.name).size(13.0).color(if lit {
                            egui::Color32::from_rgb(110, 240, 200)
                        } else {
                            power_ui_colour(p, sheet.element)
                        });
                        let resp = ui
                            .selectable_label(self.selected == Some(p), text)
                            .on_hover_text(what_it_does(d));
                        if resp.clicked() {
                            self.selected = Some(p);
                            self.confirm = None;
                        }
                        let (dots, _) =
                            ui.allocate_exact_size(egui::vec2(20.0, 10.0), egui::Sense::hover());
                        augmentview::socket_dots(
                            ui.painter(),
                            dots.right_center() - egui::vec2(3.0, 0.0),
                            sheet,
                            d,
                            2.5,
                        );
                        if tray::keyed(p) {
                            self.tray_handle(ui, p);
                        }
                        ui.add_space(8.0);
                    }
                });
                ui.label(
                    egui::RichText::new(
                        "The portal: Portal Home marks where it leads, Open Portal tears it open ahead of you. \
                         Step in, come out at home; step back in to return.",
                    )
                    .size(12.0)
                    .color(egui::Color32::from_gray(170)),
                );
            },
        );
        self.open = open;
    }

    /// The tree as boxes and arrows: owned powers gold, ones to buy now
    /// lit, the rest dim. A click picks a power.
    fn tree(&mut self, ui: &mut egui::Ui, sheet: &Sheet, tree: Tree) {
        let size = egui::vec2(
            COL_STEP * COLUMNS as f32,
            ROW_STEP * (ROWS as f32 - 1.0) + NODE_H + 8.0,
        );
        let (area, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let rect_of = |d: &PowerDef| {
            egui::Rect::from_min_size(
                area.min
                    + egui::vec2(
                        d.col as f32 * COL_STEP + (COL_STEP - NODE_W) * 0.5,
                        d.row as f32 * ROW_STEP + 4.0,
                    ),
                egui::vec2(NODE_W, NODE_H),
            )
        };
        let painter = ui.painter_at(area.expand(2.0));
        // Arrows first, under the boxes.
        for d in powers::tree(tree) {
            let to = rect_of(d);
            for any in d.needs {
                for &n in any.iter() {
                    let Some(from) = powers::get(PowerId(n)).filter(|f| f.tree == tree) else {
                        continue;
                    };
                    let a = rect_of(from).center_bottom();
                    let b = to.center_top();
                    let lit = sheet.has(from.id);
                    let colour = if lit {
                        GOLD.gamma_multiply(0.8)
                    } else {
                        egui::Color32::from_gray(90)
                    };
                    let width: f32 = if any.len() > 1 { 1.0 } else { 2.0 };
                    painter.line_segment([a, b], egui::Stroke::new(width, colour));
                    let dir = (b - a).normalized();
                    let side = egui::vec2(-dir.y, dir.x);
                    painter.add(egui::Shape::convex_polygon(
                        vec![b, b - dir * 8.0 + side * 4.0, b - dir * 8.0 - side * 4.0],
                        colour,
                        egui::Stroke::NONE,
                    ));
                }
            }
        }
        // "or" between powers of one fork in the same row.
        let forks: Vec<&PowerDef> = powers::tree(tree).filter(|d| d.fork != 0).collect();
        for (k, a) in forks.iter().enumerate() {
            for b in &forks[k + 1..] {
                if a.fork == b.fork && a.row == b.row && a.col.abs_diff(b.col) == 1 {
                    let (l, r) = if a.col < b.col { (a, b) } else { (b, a) };
                    let p = egui::pos2(
                        (rect_of(l).right() + rect_of(r).left()) * 0.5,
                        rect_of(l).center().y,
                    );
                    painter.text(
                        p,
                        egui::Align2::CENTER_CENTER,
                        "or",
                        egui::FontId::proportional(13.0),
                        egui::Color32::from_rgb(255, 150, 120),
                    );
                }
            }
        }
        for d in powers::tree(tree) {
            let r = rect_of(d);
            let id = ui.id().with(("power node", d.id.0));
            let resp = ui.interact(r, id, egui::Sense::click());
            let owned = sheet.has(d.id);
            let state = can_buy(sheet, d.id);
            let (fill, text, edge) = if owned {
                (egui::Color32::from_rgb(70, 62, 30), GOLD, GOLD)
            } else {
                match state {
                    Ok(_) => (
                        egui::Color32::from_rgb(44, 56, 96),
                        egui::Color32::WHITE,
                        egui::Color32::from_rgb(150, 200, 255),
                    ),
                    Err(BuyError::TooDear(_)) => (
                        egui::Color32::from_rgb(38, 42, 66),
                        egui::Color32::from_gray(225),
                        egui::Color32::from_gray(120),
                    ),
                    Err(BuyError::ForkTaken(_)) => (
                        egui::Color32::from_rgb(40, 28, 30),
                        egui::Color32::from_gray(110),
                        egui::Color32::from_rgb(110, 60, 60),
                    ),
                    Err(_) => (
                        egui::Color32::from_rgb(30, 31, 40),
                        egui::Color32::from_gray(150),
                        egui::Color32::from_gray(70),
                    ),
                }
            };
            let fill = if resp.hovered() {
                fill.gamma_multiply(1.4)
            } else {
                fill
            };
            // Googie leaves, like the tray's slots.
            let leaf = crate::googie::leaf(12);
            painter.rect_filled(r, leaf, fill);
            let edge = if d.helper && !owned { HELPER } else { edge };
            let width: f32 = if self.selected == Some(d.id) {
                3.0
            } else {
                1.5
            };
            painter.rect_stroke(
                r,
                leaf,
                egui::Stroke::new(width, edge),
                egui::StrokeKind::Inside,
            );
            let mark = if d.capstone { "★ " } else { "" };
            painter.text(
                r.center_top() + egui::vec2(0.0, 6.0),
                egui::Align2::CENTER_TOP,
                format!("{mark}{}", d.name),
                egui::FontId::proportional(13.0),
                text,
            );
            let sub = if owned {
                "owned".to_string()
            } else {
                match state {
                    Ok(k) | Err(BuyError::TooDear(k)) => format!("{k} karma"),
                    Err(BuyError::ForkTaken(_)) => "not your path".into(),
                    Err(_) => "locked".into(),
                }
            };
            painter.text(
                r.center_bottom() - egui::vec2(0.0, 6.0),
                egui::Align2::CENTER_BOTTOM,
                sub,
                egui::FontId::proportional(11.0),
                text.gamma_multiply(0.8),
            );
            if owned {
                augmentview::socket_dots(
                    &painter,
                    r.right_bottom() - egui::vec2(8.0, 8.0),
                    sheet,
                    d,
                    2.5,
                );
                // What is in hand fits this power: it lights up.
                if self.augments.fits_in_hand(ui.ctx(), sheet, d) {
                    let pulse = 0.6 + 0.4 * (crate::uikit::now(ui.ctx()) * 5.0).sin();
                    painter.rect_stroke(
                        r.expand(2.0),
                        leaf,
                        egui::Stroke::new(
                            2.5_f32,
                            egui::Color32::from_rgb(110, 240, 200).gamma_multiply(pulse),
                        ),
                        egui::StrokeKind::Outside,
                    );
                }
            }
            // An augment let go over the power: the power is picked, its
            // sockets show below.
            let carried_here = resp.contains_pointer()
                && self.augments.in_hand(ui.ctx(), sheet).is_some()
                && ui.input(|i| i.pointer.any_down());
            if resp.clicked() || (carried_here && self.selected != Some(d.id)) {
                self.selected = Some(d.id);
                self.confirm = None;
            }
            resp.on_hover_text(d.blurb);
        }
    }

    /// The picked power: what it does, what it needs, buy or give up.
    fn details(
        &mut self,
        ui: &mut egui::Ui,
        sheet: &Sheet,
        d: &PowerDef,
        out: &mut Vec<PowerAction>,
    ) {
        let owned = sheet.has(d.id);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(d.name)
                    .size(20.0)
                    .strong()
                    .color(power_ui_colour(d.id, sheet.element)),
            );
            ui.label(
                egui::RichText::new(d.blurb)
                    .size(14.0)
                    .color(egui::Color32::from_gray(200)),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if owned {
                    let back = (sheet
                        .powers
                        .iter()
                        .find(|o| o.power == d.id)
                        .map_or(0, |o| o.paid) as f64
                        * REFUND)
                        .floor() as u32;
                    match can_give_up(sheet, d.id) {
                        Ok(()) => {
                            let set = sheet.sockets(d.id).iter().filter(|a| !a.is_none()).count();
                            let label = if self.confirm == Some(d.id) && set > 0 {
                                format!(
                                    "Really give it up? (+{back}, its {set} augment{} lost)",
                                    if set == 1 { " is" } else { "s are" }
                                )
                            } else if self.confirm == Some(d.id) {
                                format!("Really give it up? (+{back})")
                            } else {
                                format!("Give up (+{back})")
                            };
                            if ui.button(label).clicked() {
                                if self.confirm == Some(d.id) {
                                    out.push(PowerAction::GiveUp(d.id));
                                    self.confirm = None;
                                } else {
                                    self.confirm = Some(d.id);
                                }
                            }
                        }
                        Err(e) => {
                            ui.label(
                                egui::RichText::new(format!("Can't give up: {e}"))
                                    .size(12.0)
                                    .color(egui::Color32::from_gray(160)),
                            );
                        }
                    }
                    ui.label(egui::RichText::new("owned").color(GOLD));
                    if tray::keyed(d.id) {
                        self.tray_handle(ui, d.id);
                    }
                } else {
                    match can_buy(sheet, d.id) {
                        Ok(price) => {
                            let b = egui::Button::new(
                                egui::RichText::new(format!("Buy · {price} karma"))
                                    .strong()
                                    .color(INK),
                            )
                            .fill(GOLD);
                            if ui.add(b).clicked() {
                                out.push(PowerAction::Buy(d.id));
                            }
                        }
                        Err(e) => {
                            ui.label(
                                egui::RichText::new(capitalise(&e.to_string()))
                                    .color(egui::Color32::from_gray(180)),
                            );
                        }
                    }
                }
            });
        });
        ui.label(
            egui::RichText::new(what_it_does(d))
                .size(13.0)
                .color(egui::Color32::from_gray(225)),
        );
        let mut notes = Vec::new();
        if !d.needs.is_empty() {
            let parts: Vec<String> = d
                .needs
                .iter()
                .map(|any| {
                    let names: Vec<&str> = any.iter().map(|&n| def_of(PowerId(n)).name).collect();
                    names.join(" or ")
                })
                .collect();
            notes.push(format!("Needs {}.", parts.join(" and ")));
        }
        if d.needs_any > 0 {
            notes.push(format!("Needs {} other powers of this tree.", d.needs_any));
        }
        if d.fork != 0 {
            let others: Vec<&str> = powers::tree(d.tree)
                .filter(|o| o.fork == d.fork && o.id != d.id)
                .map(|o| o.name)
                .collect();
            notes.push(format!(
                "Pick one for good: this or {}.",
                others.join(" or ")
            ));
        }
        if !notes.is_empty() {
            ui.label(
                egui::RichText::new(notes.join(" "))
                    .size(12.0)
                    .color(egui::Color32::from_rgb(255, 190, 150)),
            );
        }
        let mut asked = Vec::new();
        self.augments.sockets(ui, sheet, d, &mut asked);
        out.extend(asked.into_iter().map(PowerAction::Augment));
    }

    /// Where an owned power sits on the tray, as a handle to drag it there
    /// (onto any slot; the power there swaps over).
    fn tray_handle(&self, ui: &mut egui::Ui, p: PowerId) {
        let text = match self.tray.slot_of(p) {
            Some(s) => format!("⠿ key {}", tray::label(s)),
            None => "⠿ drag to tray".to_string(),
        };
        ui.dnd_drag_source(
            egui::Id::new(("tray drag", p.0)),
            tray::Drag::Power(p),
            |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(70, 78, 120))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::symmetric(6, 2))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(text).size(12.0).color(GOLD));
                    });
            },
        )
        .response
        .on_hover_text("Drag onto a slot of the power tray");
    }

    /// The card a new hero starts with: where the powers came from, the
    /// element riding on every blow, and the first power.
    fn origin_card(&mut self, ctx: &egui::Context) -> Option<Sheet> {
        let mut made = None;
        crate::googie::window(
            ctx,
            egui::Window::new("Who are you?")
                .collapsible(false)
                .resizable(false)
                .default_width(600.0)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, -10.0)),
            "Who are you?",
            None,
            |ui| {
                ui.label(
                    egui::RichText::new(
                        "Everyone starts as a normal person. Then the Aether touched you.",
                    )
                    .size(15.0),
                );
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("Your origin: it decides your power tree")
                        .size(17.0)
                        .color(GOLD)
                        .strong(),
                );
                for (k, o) in ORIGINS.iter().enumerate() {
                    let sel = self.origin == k;
                    let text = egui::RichText::new(format!("{}\n{}", o.name, o.blurb)).size(14.0);
                    if ui.selectable_label(sel, text).clicked() {
                        self.origin = k;
                    }
                }
                ui.add_space(8.0);
                self.element_picker(ui);
                ui.add_space(8.0);
                let first = def_of(progress::first_power(self.origin as u8));
                ui.label(
                    egui::RichText::new(format!("Your first power: {}", first.name))
                        .size(17.0)
                        .color(GOLD)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(what_it_does(first))
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
                    let mut s = progress::new_hero(self.origin as u8, self.element);
                    let _ = progress::buy(&mut s, first.id);
                    made = Some(s);
                }
            },
        );
        made
    }

    /// The six elements to pick from.
    fn element_picker(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new("Your element: it rides on every blow, for good")
                .size(17.0)
                .color(GOLD)
                .strong(),
        );
        egui::Grid::new("elements").num_columns(2).show(ui, |ui| {
            for e in Element::ALL {
                let text = egui::RichText::new(e.name())
                    .size(15.0)
                    .strong()
                    .color(element_ui_colour(e));
                if ui.selectable_label(self.element == e, text).clicked() {
                    self.element = e;
                }
                ui.label(
                    egui::RichText::new(e.blurb())
                        .size(13.0)
                        .color(egui::Color32::from_gray(200)),
                );
                ui.end_row();
            }
        });
    }

    /// A hero from before the power trees: their karma is back, and they
    /// pick an element once.
    fn element_card(&mut self, ctx: &egui::Context, sheet: &Sheet) -> Option<Element> {
        let mut picked = None;
        crate::googie::window(
            ctx,
            egui::Window::new("The power trees are here")
                .collapsible(false)
                .resizable(false)
                .default_width(560.0)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, -10.0)),
            "The power trees are here",
            None,
            |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "Powers now grow in a tree for each origin, and every hero has an element. \
                         All the karma you spent is back: {} karma to spend. Press B to pick your powers.",
                        sheet.karma
                    ))
                    .size(14.0),
                );
                ui.add_space(8.0);
                self.element_picker(ui);
                ui.add_space(10.0);
                let go = egui::Button::new(
                    egui::RichText::new(format!("Take {}", self.element.name()))
                        .size(17.0)
                        .strong()
                        .color(INK),
                )
                .fill(GOLD);
                if ui.add(go).clicked() {
                    picked = Some(self.element);
                }
            },
        );
        picked
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
        // The title's comic lettering, with its red block shadow.
        let big = crate::uikit::logo_font(ctx, 72.0 * pop);
        let text = format!("LEVEL {level}!");
        painter.text(
            c + egui::vec2(5.0, 6.0),
            egui::Align2::CENTER_CENTER,
            &text,
            big.clone(),
            crate::uikit::HERO_RED.gamma_multiply(a),
        );
        painter.text(
            c,
            egui::Align2::CENTER_CENTER,
            &text,
            big,
            GOLD.gamma_multiply(a),
        );
        // What can be bought now.
        let mut lines = vec!["Health, damage and karma to spend grow".to_string()];
        let tree = Tree::of_origin(sheet.origin);
        let affordable = powers::tree(tree)
            .chain(powers::tree(Tree::Common))
            .filter(|d| can_buy(sheet, d.id).is_ok())
            .count();
        if affordable > 0 {
            lines.push(format!(
                "{} karma: enough for a new power. Press B.",
                sheet.karma
            ));
        }
        for (k, l) in lines.iter().enumerate() {
            painter.text(
                c + egui::vec2(0.0, 58.0 + k as f32 * 24.0),
                egui::Align2::CENTER_CENTER,
                l,
                crate::uikit::ui_font(ctx, 19.0),
                crate::uikit::CREAM.gamma_multiply(a),
            );
        }
    }
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Seconds, said briefly.
fn secs(s: f64) -> String {
    if (s - s.round()).abs() < 0.05 {
        format!("{s:.0} s")
    } else {
        format!("{s:.1} s")
    }
}

fn pct(x: f32) -> String {
    format!("{:.0}%", x * 100.0)
}

/// What a power does, in a sentence or two, from its numbers.
pub fn what_it_does(d: &PowerDef) -> String {
    let f = &d.fields;
    let mut head = match d.delivery() {
        Delivery::Melee => "A blow at a villain in reach".to_string(),
        Delivery::Ranged => format!("A shot at one villain, {:.0} m", f.range),
        Delivery::Line => format!("Hits every villain in a {:.0} m line", f.range),
        Delivery::Cone => format!("Hits everything in a {:.0} m cone", f.range),
        Delivery::Burst => format!("Hits every villain within {:.0} m of you", f.radius),
        Delivery::Sky => format!(
            "Strikes all within {:.0} m of your target {} later",
            f.radius,
            secs(f.delay)
        ),
        Delivery::Allies if f.radius > 0.0 => format!("You and friends within {:.0} m", f.radius),
        Delivery::Allies => "You".to_string(),
        Delivery::Grab => format!("Lift something heavy and throw it, {:.0} m", f.range),
        Delivery::Blink => match f.lands {
            Lands::Home => "Takes you home to Hero Hall".to_string(),
            Lands::Path => format!("Dash {:.0} m, hitting everyone on the way", f.range),
            Lands::Start => format!("Blink {:.0} m, with a blast where you were", f.range),
            Lands::Arrival => format!("Leap {:.0} m, with a blast where you land", f.range),
            Lands::Target => format!("Blink up to {:.0} m to where you aim", f.range),
            Lands::GateMark => {
                "Marks where your portal home stands: a step ahead, facing the way you face"
                    .to_string()
            }
            Lands::GateOpen => {
                "Opens a portal a stride ahead; step in and come out at your portal home, and back"
                    .to_string()
            }
        },
        Delivery::Rescue => format!(
            "Floats {} trapped {} within {:.0} m out to safety",
            f.rescues.max(1),
            if f.rescues > 1 { "people" } else { "person" },
            f.range
        ),
        Delivery::Summon => format!(
            "Calls {} sidekick{} to fight at your side",
            f.summons.max(1),
            if f.summons > 1 { "s" } else { "" }
        ),
        Delivery::Passive if d.toggle => "Switch on and off with its key".to_string(),
        Delivery::Passive => "Always on".to_string(),
    };
    let mut bits: Vec<String> = Vec::new();
    if f.damage > 0.0 {
        let times = if f.repeats > 1 {
            format!(" x{}", f.repeats)
        } else {
            String::new()
        };
        bits.push(format!("{:.0} damage{times} at level 1", f.damage));
    }
    if f.knock >= 1.0 {
        bits.push("knocks down".into());
    } else if f.knock > 0.0 {
        bits.push("staggers".into());
    }
    if f.launch > 0.0 {
        bits.push("throws them up".into());
    }
    if f.push > 0.0 {
        bits.push(format!("shoves them {:.0} m", f.push));
    } else if f.push < 0.0 {
        bits.push("pulls them in".into());
    }
    if f.hold > 0.0 {
        bits.push(format!("holds them {}", secs(f.hold)));
    }
    if f.freeze > 0.0 {
        bits.push(format!("freezes minions {}", secs(f.freeze)));
    }
    if f.slow > 0.0 {
        bits.push(format!("slows {} for {}", pct(f.slow), secs(f.slow_secs)));
    }
    if f.dot > 0.0 {
        bits.push(format!(
            "burns {} more over {}",
            pct(f.dot),
            secs(powers::DOT_SECONDS)
        ));
    }
    if f.confuse > 0.0 {
        bits.push(format!("confuses {}", secs(f.confuse)));
    }
    if f.fear > 0.0 {
        bits.push(format!("sends them fleeing {}", secs(f.fear)));
    }
    if f.charm > 0.0 {
        bits.push(format!("a minion fights for you {}", secs(f.charm)));
    }
    if f.disarm > 0.0 {
        bits.push(format!("no attacks for {}", secs(f.disarm)));
    }
    if f.weaken > 0.0 {
        bits.push(format!(
            "they take {} more for {}",
            pct(f.weaken),
            secs(f.lasts)
        ));
    }
    if f.spread > 0.0 {
        bits.push(format!(
            "{} leaps to {} more",
            pct(f.spread),
            f.spread_to.max(1)
        ));
    }
    if f.drain > 0.0 {
        bits.push(format!("{} back as health", pct(f.drain)));
    }
    if f.exploit > 0.0 {
        bits.push(format!(
            "{} more on the held, frozen, confused or afraid",
            pct(f.exploit)
        ));
    }
    if f.ambush > 0.0 {
        bits.push(format!(
            "your first blow on a villain not yet fighting you does {} more",
            pct(f.ambush)
        ));
    }
    if f.dodge > 0.0 {
        bits.push(format!("{} of villains' blows miss you", pct(f.dodge)));
    }
    if f.brace > 0.0 {
        bits.push(format!("standing still you take {} less", pct(f.brace)));
    }
    if f.cuff > 0.0 {
        bits.push(format!(
            "a minion you beat down below {} of its health gives up",
            pct(f.cuff)
        ));
    }
    if f.second_wind > 0.0 {
        bits.push(format!(
            "once in {}, dropping below {} health gives {} of it back",
            secs(combat::SECOND_WIND_EVERY),
            pct(combat::SECOND_WIND_BELOW),
            pct(f.second_wind)
        ));
    }
    if f.get_up > 0.0 {
        bits.push(format!(
            "once in {}, you get up on your own {} after going down",
            secs(combat::GET_UP_EVERY),
            secs(f.get_up)
        ));
    }
    if f.heal > 0.0 {
        bits.push(format!("heals {:.0} at level 1", f.heal));
    }
    if f.revive > 0.0 {
        bits.push("gets downed friends up".into());
    }
    let lasting = if f.lasts > 0.0 && d.delivery() != Delivery::Passive {
        format!(" for {}", secs(f.lasts))
    } else {
        String::new()
    };
    if f.shield > 0.0 {
        bits.push(format!("stops {} of every blow{lasting}", pct(f.shield)));
    }
    if f.reflect > 0.0 {
        bits.push(format!("sends {} of a blow back", pct(f.reflect)));
    }
    if f.resist > 0.0 {
        bits.push(format!("{} less damage{lasting}", pct(f.resist)));
    }
    if f.regen > 0.0 {
        bits.push(format!("{} of your health back a second", pct(f.regen)));
    }
    if f.might > 0.0 {
        bits.push(format!("hits {} harder{lasting}", pct(f.might)));
    }
    if f.haste > 0.0 {
        bits.push(format!("powers recharge {} faster{lasting}", pct(f.haste)));
    }
    if f.steadfast > 0.0 {
        bits.push(format!("never knocked down{lasting}"));
    }
    if f.hide > 0.0 {
        bits.push(format!("villains lose sight of you{lasting}"));
    }
    if f.execute > 0.0 {
        bits.push(format!("minions fall at one blow{lasting}"));
    }
    if f.angel > 0.0 {
        bits.push("now and then a blow that would down you leaves you standing".into());
    }
    if f.aura > 0.0 {
        bits.push(format!(
            "friends within {:.0} m hit {} harder",
            powers::AURA_RADIUS,
            pct(f.aura)
        ));
    }
    if f.ward > 0.0 {
        bits.push(format!(
            "friends and sidekicks within {:.0} m take {} less, you {}",
            powers::AURA_RADIUS,
            pct(f.ward),
            pct(f.ward * combat::OWN_WARD)
        ));
    }
    if f.revive_reach > 0.0 {
        bits.push(format!("revives friends from {:.0} m", f.revive_reach));
    }
    if f.bounty > 0.0 {
        bits.push(format!("{} more karma for villains beaten", pct(f.bounty)));
    }
    if f.chain > 0.0 {
        bits.push(format!(
            "{} of a punch leaps to a villain near",
            pct(f.chain)
        ));
    }
    use powers::travel;
    for (bit, text) in [
        (travel::JUMP, "leap 14 m up at level 1, 45 m at level 20 (hold Space for full height); a hard landing knocks villains over"),
        (travel::SPRINT, "run twice as fast, faster with every level"),
        (travel::GLIDE, "hold jump to glide down"),
        (
            travel::FLY,
            "V to fly, Shift to go fast; no energy comes back while you are in the air",
        ),
        (travel::SPEED, "run 24 m/s (34 at level 20), Shift for 40 (58)"),
        (travel::WALL, "run up walls"),
    ] {
        if f.travel & bit != 0 {
            bits.push(text.into());
        }
    }
    if f.trail > 0.0 {
        bits.push(format!(
            "{:.0} damage a second to villains you rush past",
            f.trail
        ));
    }
    if f.wreck > 0.0 && f.spare < 1.0 {
        bits.push("wrecks the street round it".into());
    }
    if !bits.is_empty() {
        head.push_str(": ");
        head.push_str(&bits.join(", "));
    }
    head.push('.');
    let energy = powers::energy(d);
    if powers::is_free(d.id) {
        head.push_str(" Free.");
    } else if d.toggle {
        head.push_str(&format!(
            " While on, it takes {} of your energy recovery.",
            pct(f.upkeep)
        ));
    } else if !d.passive() {
        head.push_str(&format!(
            " {energy:.0} energy, ready again after {}.",
            secs(powers::recharge(d))
        ));
    }
    head
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_power_says_what_it_does() {
        for d in powers::POWERS {
            let t = what_it_does(d);
            assert!(t.len() > 12 && t.ends_with('.'), "{}: {t}", d.name);
        }
        assert!(what_it_does(def_of(PowerId(10))).contains("knocks down"));
    }

    #[test]
    fn every_tree_fits_its_grid() {
        for d in powers::POWERS {
            assert!(
                (d.col as usize) < COLUMNS && (d.row as usize) < ROWS,
                "{}",
                d.name
            );
        }
        for t in Tree::ORIGIN_TREES.into_iter().chain([Tree::Common]) {
            let mut seen = std::collections::BTreeSet::new();
            for d in powers::tree(t) {
                assert!(seen.insert((d.row, d.col)), "{} overlaps in {t:?}", d.name);
            }
        }
    }
}
