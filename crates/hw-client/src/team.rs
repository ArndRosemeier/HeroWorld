//! The team frame and the friendly target.
//!
//! Every other hero of the session stands in a small frame on the left,
//! with a health bar. Clicking a name, clicking the hero in the world or
//! pressing F1-F4 (the heroes in name order) picks them as the friendly
//! target; the same again lets go. Heals, shields and revives then reach
//! them from up to `ALLY_REACH` metres away. Tab never picks a teammate:
//! it stays with the villains.

use crate::minimap::Mate;
use engine::ui::egui;
use hw_core::NetId;
use hw_net::EntityState;

/// At most this many are listed (the session holds ten players).
const SHOWN: usize = 9;

#[derive(Default)]
pub struct Team {
    /// The picked teammate.
    pub ally: Option<NetId>,
    /// The listed heroes, by name.
    list: Vec<Mate>,
    /// An augment another player offers us: who, their name, what.
    pub offer: Option<(hw_net::PlayerId, String, hw_net::Augment)>,
}

impl Team {
    /// This frame's teammates. A target that left, or is no longer in view
    /// of the mirror, is let go.
    pub fn set_mates(&mut self, mates: &[Mate]) {
        self.list = mates.to_vec();
        self.list.sort_by(|a, b| a.name.cmp(&b.name));
        self.list.truncate(SHOWN);
        if let Some(a) = self.ally {
            if !self.list.iter().any(|m| m.id == Some(a)) {
                self.ally = None;
            }
        }
    }

    /// The listed heroes with an entity (the ones that can be picked).
    pub fn pickable(&self) -> Vec<NetId> {
        self.list.iter().filter_map(|m| m.id).collect()
    }

    /// Picks `id`, or lets go when it already is the target.
    pub fn pick(&mut self, id: NetId) {
        self.ally = if self.ally == Some(id) {
            None
        } else {
            Some(id)
        };
    }

    /// F1-F4 pick the first four heroes. Returns whether a key was used.
    pub fn keys(&mut self, ctx: &egui::Context) -> bool {
        let keys = [egui::Key::F1, egui::Key::F2, egui::Key::F3, egui::Key::F4];
        for (n, k) in keys.into_iter().enumerate() {
            if ctx.input(|i| i.key_pressed(k)) {
                if let Some(id) = self.list.get(n).and_then(|m| m.id) {
                    self.pick(id);
                }
                return true;
            }
        }
        false
    }

    /// A click in the world (the ray under the mouse): picks the teammate
    /// there. True when one was hit, so the click does not also punch.
    pub fn click(&mut self, all: &[EntityState], ray: (glam::Vec3, glam::Vec3)) -> bool {
        let ids = self.pickable();
        match crate::fightview::mate_under(all, &ids, ray.0, ray.1) {
            Some(id) => {
                self.pick(id);
                true
            }
            None => false,
        }
    }

    /// The prompt for an offered augment. Returns the answer when one is
    /// given: (the giver, take it).
    pub fn offer_prompt(
        &mut self,
        ctx: &egui::Context,
        time: f32,
    ) -> Option<(hw_net::PlayerId, bool)> {
        let (from, who, a) = self.offer.clone()?;
        let mut answer = None;
        egui::Area::new(egui::Id::new("augment offer"))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -150.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (r, _) =
                            ui.allocate_exact_size(egui::vec2(36.0, 36.0), egui::Sense::hover());
                        crate::augmentview::chip(ui.painter(), r, a, time, 0.0);
                        ui.vertical(|ui| {
                            let grade = hw_sim::augments::Grade::of(a).name();
                            let name = hw_sim::augments::Effect::of(a).map_or("", |e| e.name());
                            ui.label(format!("{who} offers you a gift"));
                            ui.label(
                                egui::RichText::new(format!("{grade} {name}  +{}%", a.percent))
                                    .strong(),
                            );
                            ui.horizontal(|ui| {
                                if ui.button("Take it").clicked() {
                                    answer = Some(true);
                                }
                                if ui.button("No thanks").clicked() {
                                    answer = Some(false);
                                }
                            });
                        });
                    });
                });
            });
        if answer.is_some() {
            self.offer = None;
        }
        answer.map(|a| (from, a))
    }

    /// Draws the frame on the left; a click on a row picks that hero.
    pub fn draw(&mut self, ctx: &egui::Context) {
        if self.list.is_empty() {
            return;
        }
        let mut picked = None;
        egui::Area::new(egui::Id::new("team frame"))
            .anchor(egui::Align2::LEFT_CENTER, egui::vec2(10.0, -60.0))
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                ui.set_width(150.0);
                for (n, m) in self.list.iter().enumerate() {
                    let chosen = m.id.is_some() && m.id == self.ally;
                    let (rect, resp) =
                        ui.allocate_exact_size(egui::vec2(150.0, 34.0), egui::Sense::click());
                    let p = ui.painter();
                    let fill = egui::Color32::from_black_alpha(if chosen { 190 } else { 130 });
                    p.rect_filled(rect, 4.0, fill);
                    if chosen || resp.hovered() {
                        let col = if chosen {
                            m.colour()
                        } else {
                            egui::Color32::GRAY
                        };
                        p.rect_stroke(
                            rect,
                            4.0,
                            egui::Stroke::new(if chosen { 2.0_f32 } else { 1.0 }, col),
                            egui::StrokeKind::Inside,
                        );
                    }
                    let key = if n < 4 {
                        format!("F{} ", n + 1)
                    } else {
                        String::new()
                    };
                    p.text(
                        rect.left_top() + egui::vec2(6.0, 4.0),
                        egui::Align2::LEFT_TOP,
                        format!("{key}{}", m.name),
                        egui::FontId::proportional(14.0),
                        m.colour(),
                    );
                    let bar = egui::Rect::from_min_size(
                        rect.left_bottom() + egui::vec2(6.0, -11.0),
                        egui::vec2(138.0, 6.0),
                    );
                    p.rect_filled(bar, 2.0, egui::Color32::from_black_alpha(200));
                    let f = if m.down { 0.0 } else { m.health as f32 / 100.0 };
                    let fill = egui::Color32::from_rgb(
                        (230.0 * (1.0 - f) + 60.0) as u8,
                        (200.0 * f + 40.0) as u8,
                        60,
                    );
                    p.rect_filled(
                        egui::Rect::from_min_size(bar.min, egui::vec2(bar.width() * f, 6.0)),
                        2.0,
                        fill,
                    );
                    if m.down {
                        p.text(
                            bar.center(),
                            egui::Align2::CENTER_CENTER,
                            "DOWN",
                            egui::FontId::proportional(9.0),
                            egui::Color32::WHITE,
                        );
                    }
                    if resp.clicked() {
                        picked = m.id;
                    }
                    ui.add_space(3.0);
                }
            });
        if let Some(id) = picked {
            self.pick(id);
        }
    }
}
