//! The chat window, bottom left, as in EverQuest and City of Heroes: one
//! log with a tab per channel, and a line to type in (Enter opens it, Enter
//! sends, Escape cancels).
//!
//! Only the Local channel exists today: what is said near the hero (a gang
//! warning them off its turf, mole folk thanking them, Hero Hall's heroes
//! giving tips, Commander Starling on the wrist radio, other players and the
//! hero themselves). A new channel (team, police radio, combat log) is one
//! more [`Channel`] variant: it gets its tab from [`Channel::ALL`], and whoever
//! writes to it calls [`ChatLog::add`] with it.
//!
//! The log keeps the last [`MAX_LINES`] lines, so a long session never grows
//! it without bound.

use engine::ui::egui;
use glam::Vec3;
use std::collections::VecDeque;

/// Lines kept, over all channels; older ones are dropped.
pub const MAX_LINES: usize = 400;
/// Characters a typed line may have (the network sends at most 255 bytes).
pub const MAX_TYPED: usize = 200;
/// Metres a spoken line carries.
pub const SAY_RANGE: f32 = 35.0;
/// Metres a shout carries (a gang warning someone off).
pub const SHOUT_RANGE: f32 = 70.0;
/// The same speaker saying the same thing within this many seconds is
/// logged once (a gang repeating its warning, a tip said twice).
const REPEAT_SECONDS: f64 = 6.0;

/// A tab of the chat window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Channel {
    /// What is said near the hero.
    Local,
    /// What the players of the session say to each other, wherever they
    /// are. Typing while this tab is on show sends to it.
    Team,
}

impl Channel {
    /// Every channel, in tab order.
    pub const ALL: [Channel; 2] = [Channel::Local, Channel::Team];

    pub fn name(self) -> &'static str {
        match self {
            Channel::Local => "Local",
            Channel::Team => "Team",
        }
    }
}

/// Who says a line: picks its colour in the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Voice {
    /// Our own hero.
    Me,
    /// Another player.
    Player,
    /// A hero of Hero Hall, or Commander Starling.
    Ally,
    /// Townsfolk, mole folk: people who are neither friend nor foe.
    Folk,
    /// Villains and gangs.
    Villain,
}

impl Voice {
    pub fn colour(self) -> egui::Color32 {
        match self {
            Voice::Me => egui::Color32::from_rgb(150, 210, 255),
            Voice::Player => egui::Color32::from_rgb(235, 235, 235),
            Voice::Ally => egui::Color32::from_rgb(255, 214, 110),
            Voice::Folk => egui::Color32::from_rgb(200, 170, 255),
            Voice::Villain => egui::Color32::from_rgb(255, 120, 90),
        }
    }
}

/// Something said out loud somewhere in the world, before it is known
/// whether the hero heard it.
#[derive(Clone, Debug, PartialEq)]
pub struct Said {
    /// Where the speaker stands.
    pub at: Vec3,
    /// How far it carries ([`SAY_RANGE`], [`SHOUT_RANGE`]).
    pub range: f32,
    pub who: String,
    pub text: String,
    pub voice: Voice,
}

/// One line in the log.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub channel: Channel,
    pub who: String,
    pub text: String,
    pub voice: Voice,
    /// Game clock when it was said (seconds since the window opened).
    pub when: f64,
}

#[derive(Default)]
pub struct ChatLog {
    lines: VecDeque<Line>,
    tab: Option<Channel>,
    /// Lines a tab has had since it was last looked at.
    unread: Vec<(Channel, usize)>,
    /// The typing line is open.
    pub typing: bool,
    input: String,
    clock: f64,
    /// The window was just opened for typing: focus the field.
    focus: bool,
}

impl ChatLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// The tab on show.
    pub fn tab(&self) -> Channel {
        self.tab.unwrap_or(Channel::Local)
    }

    /// Every line kept, oldest first.
    pub fn lines(&self) -> impl Iterator<Item = &Line> {
        self.lines.iter()
    }

    /// The lines of one channel, oldest first.
    pub fn channel(&self, c: Channel) -> impl Iterator<Item = &Line> {
        self.lines.iter().filter(move |l| l.channel == c)
    }

    /// Lines `c` has had since its tab was last on show.
    pub fn unread(&self, c: Channel) -> usize {
        self.unread.iter().find(|u| u.0 == c).map_or(0, |u| u.1)
    }

    /// Moves the clock on (seconds).
    pub fn update(&mut self, dt: f32) {
        self.clock += dt as f64;
    }

    /// Adds a line to a channel. The same speaker repeating the same words
    /// soon after is logged once.
    pub fn add(&mut self, channel: Channel, who: &str, text: &str, voice: Voice) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let repeat = self.lines.iter().rev().take(12).any(|l| {
            l.channel == channel
                && l.who == who
                && l.text == text
                && self.clock - l.when < REPEAT_SECONDS
        });
        if repeat {
            return;
        }
        self.lines.push_back(Line {
            channel,
            who: who.to_string(),
            text: text.to_string(),
            voice,
            when: self.clock,
        });
        while self.lines.len() > MAX_LINES {
            self.lines.pop_front();
        }
        if channel != self.tab() {
            match self.unread.iter_mut().find(|u| u.0 == channel) {
                Some(u) => u.1 += 1,
                None => self.unread.push((channel, 1)),
            }
        }
    }

    /// Something said in the world: logged in Local when `hero` stands
    /// within its range (no hero yet: nothing is heard).
    pub fn hear(&mut self, s: &Said, hero: Option<Vec3>) -> bool {
        let Some(h) = hero else { return false };
        if h.distance(s.at) > s.range {
            return false;
        }
        self.add(Channel::Local, &s.who, &s.text, s.voice);
        true
    }

    /// Opens the typing line (Enter, or `/` as in many MMOs).
    pub fn start_typing(&mut self) {
        self.typing = true;
        self.focus = true;
    }

    /// The chat's keys, early in the frame before menus look at them: Enter
    /// or `/` opens the typing line (unless `blocked`: a menu is open, or
    /// some other field has the keys), Escape closes it. Returns true when
    /// it took an Escape, which then must not open the game menu or quit.
    pub fn keys(&mut self, ctx: &egui::Context, blocked: bool) -> bool {
        if self.typing {
            let escape = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            if escape {
                self.stop_typing();
            }
            return escape;
        }
        let (enter, slash) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Slash),
            )
        });
        if (enter || slash) && !blocked && !ctx.wants_keyboard_input() {
            self.start_typing();
        }
        false
    }

    fn stop_typing(&mut self) {
        self.typing = false;
        self.focus = false;
        self.input.clear();
    }

    /// Draws the window. Returns the line the player sent and the channel
    /// it goes to (the tab on show), if any.
    ///
    /// `beside` is the room left of the power tray; when it is too narrow
    /// the window sits over the tray's left end instead, `above` up from
    /// the bottom of the screen.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        beside: f32,
        above: f32,
    ) -> Option<(Channel, String)> {
        let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
        let (width, bottom) = if beside >= 300.0 {
            (beside.min(460.0), 10.0)
        } else {
            (380.0, above + 8.0)
        };
        let screen = ctx.screen_rect();
        let mut sent = None;
        let mut close = false;
        // The comic panel, but see-through: the street matters more.
        let frame = egui::Frame::new()
            .fill(crate::uikit::with_alpha(crate::uikit::NAVY, 0.72))
            .stroke(egui::Stroke::new(
                1.5_f32,
                crate::uikit::with_alpha(crate::uikit::GOLD, 0.6),
            ))
            .corner_radius(crate::googie::leaf(16))
            .inner_margin(egui::Margin::symmetric(10, 7));
        // Where the player left it last time, else bottom left beside the tray.
        let resp = crate::layout::window(
            egui::Window::new("chat"),
            "chat",
            egui::Align2::LEFT_BOTTOM,
            egui::pos2(screen.left() + 10.0, screen.bottom() - bottom),
        )
        .title_bar(false)
        .resizable(true)
        .collapsible(false)
        .frame(frame)
        .default_size(crate::layout::size_of("chat").unwrap_or(egui::vec2(width, 160.0)))
        .min_size(egui::vec2(220.0, 80.0))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                for c in Channel::ALL {
                    let unread = self.unread(c);
                    let label = if unread > 0 && c != self.tab() {
                        format!("{} ({unread})", c.name())
                    } else {
                        c.name().to_string()
                    };
                    let text = egui::RichText::new(label).size(13.0).strong();
                    if ui.selectable_label(c == self.tab(), text).clicked() {
                        self.tab = Some(c);
                    }
                }
                if !self.typing {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new("Enter: chat")
                                .size(11.0)
                                .color(egui::Color32::from_gray(150)),
                        );
                    });
                }
            });
            let tab = self.tab();
            self.unread.retain(|u| u.0 != tab);
            ui.separator();
            let typing_h = if self.typing { 30.0 } else { 0.0 };
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .max_height((ui.available_height() - typing_h).max(30.0))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    let mut any = false;
                    for l in self.channel(tab) {
                        any = true;
                        ui.label(line_text(l));
                    }
                    if !any {
                        ui.label(
                            egui::RichText::new(match tab {
                                Channel::Local => "What people say near you shows here.",
                                Channel::Team => {
                                    "What your team says shows here, wherever they are."
                                }
                            })
                            .size(12.5)
                            .italics()
                            .color(egui::Color32::from_gray(150)),
                        );
                    }
                });
            if self.typing {
                let hint_tab = self.tab();
                let edit = egui::TextEdit::singleline(&mut self.input)
                    .hint_text(match hint_tab {
                        Channel::Local => "Say something (Enter sends, Esc cancels)",
                        Channel::Team => "Tell your team (Enter sends, Esc cancels)",
                    })
                    .char_limit(MAX_TYPED)
                    .desired_width(f32::INFINITY);
                let r = ui.add(edit);
                if self.focus {
                    r.request_focus();
                    self.focus = false;
                } else if r.lost_focus() {
                    // Enter in a one-line field gives up its focus: send.
                    // A click elsewhere just closes the line.
                    let text = self.input.trim().to_string();
                    if enter && !text.is_empty() {
                        sent = Some((self.tab(), text));
                    }
                    close = true;
                }
            }
        });
        if let Some(r) = &resp {
            crate::layout::remember(
                ctx,
                "chat",
                egui::Align2::LEFT_BOTTOM,
                r.response.rect,
                true,
            );
        }
        if close {
            self.stop_typing();
        }
        sent
    }
}

/// A log line as the window shows it: the speaker in their colour, then
/// the words.
fn line_text(l: &Line) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let font = egui::FontId::proportional(13.5);
    let colour = l.voice.colour();
    job.append(
        &format!("{}: ", l.who),
        0.0,
        egui::TextFormat {
            font_id: font.clone(),
            color: colour,
            ..Default::default()
        },
    );
    job.append(
        &l.text,
        0.0,
        egui::TextFormat {
            font_id: font,
            color: colour.gamma_multiply(0.92),
            ..Default::default()
        },
    );
    job.wrap = egui::text::TextWrapping {
        max_width: f32::INFINITY,
        ..Default::default()
    };
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(at: Vec3, range: f32) -> Said {
        Said {
            at,
            range,
            who: "Mutant Brute".into(),
            text: "Get off our turf!".into(),
            voice: Voice::Villain,
        }
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn only_what_is_said_near_the_hero_is_logged() {
        let mut log = ChatLog::new();
        let hero = Some(Vec3::ZERO);
        assert!(log.hear(&said(Vec3::new(20.0, 0.0, 0.0), SAY_RANGE), hero));
        log.update(10.0);
        assert!(!log.hear(&said(Vec3::new(50.0, 0.0, 0.0), SAY_RANGE), hero));
        assert!(log.hear(&said(Vec3::new(50.0, 0.0, 0.0), SHOUT_RANGE), hero));
        assert!(!log.hear(&said(Vec3::ZERO, SAY_RANGE), None));
        assert_eq!(log.channel(Channel::Local).count(), 2);
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn a_repeated_line_is_logged_once() {
        let mut log = ChatLog::new();
        log.add(Channel::Local, "Brute", "Go away!", Voice::Villain);
        log.update(1.0);
        log.add(Channel::Local, "Brute", "Go away!", Voice::Villain);
        assert_eq!(log.lines().count(), 1);
        log.update(REPEAT_SECONDS as f32 + 0.1);
        log.add(Channel::Local, "Brute", "Go away!", Voice::Villain);
        assert_eq!(log.lines().count(), 2);
        // Empty lines are never logged.
        log.add(Channel::Local, "Me", "   ", Voice::Me);
        assert_eq!(log.lines().count(), 2);
    }

    #[test]
    fn team_lines_stay_on_the_team_tab() {
        let mut log = ChatLog::new();
        log.add(Channel::Team, "Ada", "meet at the tower", Voice::Player);
        log.add(Channel::Local, "Brute", "Go away!", Voice::Villain);
        assert_eq!(log.channel(Channel::Team).count(), 1);
        assert_eq!(log.channel(Channel::Local).count(), 1);
        // The tab on show is Local: the team line is unread.
        assert_eq!(log.unread(Channel::Team), 1);
        assert_eq!(log.unread(Channel::Local), 0);
    }

    #[test]
    fn the_log_stays_bounded() {
        let mut log = ChatLog::new();
        for k in 0..MAX_LINES * 3 {
            log.add(
                Channel::Local,
                "Townsfolk",
                &format!("line {k}"),
                Voice::Folk,
            );
        }
        assert_eq!(log.lines().count(), MAX_LINES);
        assert_eq!(
            log.lines().last().map(|l| l.text.as_str()),
            Some(format!("line {}", MAX_LINES * 3 - 1).as_str())
        );
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn every_channel_has_a_tab_name() {
        for c in Channel::ALL {
            assert!(!c.name().is_empty());
        }
    }
}
