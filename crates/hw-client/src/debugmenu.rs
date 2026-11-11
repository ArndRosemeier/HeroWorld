//! The debug menu (Ctrl+Shift+F12): buttons for trying things out in the
//! game that need a human look. Go to the town's notable places, raise the
//! hero's level, buy powers, move faction standings, start any incident
//! near you, break things, and change the time of day.
//!
//! Single player only: in someone else's world the menu says so and offers
//! nothing. Changes it makes to the hero are not saved to the hero file.
//! This module draws the menu and finds the places; `main.rs` carries out
//! what was clicked ([`DebugAction`]).

use engine::ui::egui;
use glam::{DVec2, DVec3};
use hw_gen::urban::area::Area;
use hw_gen::urban::landmarks::LandmarkKind;
use hw_gen::urban::world::WATER_LEVEL;
use hw_net::{Faction, Sheet, FACTIONS};
use hw_sim::collision::{Obstacles, BODY_HEIGHT};
use hw_sim::incidents::IncidentKind;

/// A place the menu can take the hero to.
#[derive(Clone, Debug)]
pub struct Place {
    pub name: String,
    /// Which list it shows under.
    pub group: &'static str,
    /// Where the hero's feet go.
    pub at: DVec3,
    /// Which way the camera looks on arrival.
    pub facing: DVec2,
    /// Arrive flying (high in the air).
    pub fly: bool,
}

/// What a click asks for; `main.rs` does it.
#[derive(Clone, Debug, PartialEq)]
pub enum DebugAction {
    /// Go to place `i` of [`DebugMenu::places`].
    Go(usize),
    /// Go to an open incident (by its call id).
    GoCall(u32),
    /// Set the hero's level (karma earned to match).
    Level(u8),
    /// Add karma to spend.
    Karma(u32),
    /// Buy every power the level allows, in theme 0 or 1.
    /// Every power of the hero's trees (the first of each fork).
    BuyAll,
    /// Switch the hero's element (otherwise for good).
    Element(hw_sim::powers::Element),
    /// Give every power back (90% of each price back).
    ClearPowers,
    /// Move a faction's regard for the hero by this much.
    Regard(Faction, i8),
    /// Every faction back to how it sees townsfolk.
    ResetRegard,
    /// Health and energy full, standing again.
    Heal,
    /// Back to the hero file this PC keeps.
    RestoreHero,
    /// Start an incident of this kind near the hero (Collapse: bring the
    /// nearest small building down).
    Incident(IncidentKind),
    /// Blow a hole in the nearest building.
    BlastBuilding,
    /// Wreck the cars around the hero.
    WreckCars,
    /// Knock lamps, benches and trees around the hero over.
    SmashProps,
    /// Knock the people around the hero over (ragdolls).
    KnockPeople,
    /// Break the colossus's next weak point.
    BreakColossus,
    /// Set the hour of day.
    Hour(f32),
    /// Run the world on by this many seconds.
    FastForward(u32),
    /// The free-flying camera on or off (O).
    FreeCamera,
    /// Buildings cut open around the hero on or off.
    Cutaway,
    /// Show the floor below (-1) or above (+1) in the cut-open buildings.
    CutLevel(i32),
    /// The town map on or off (M).
    Map,
    /// Upload every mesh (true) or every texture (false) to the GPU again.
    Reupload(bool),
    /// Compare every mesh on the GPU with the game's copy (Engine's audit log).
    GpuCheck,
    /// Configure the window surface again (true) or rebuild the window-sized
    /// buffers (false), as a resize does.
    Rebuild(bool),
    /// Engine's drawing-check flags (`World::set_debug_shading`).
    Shading(u32),
}

/// The tab showing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Tab {
    #[default]
    Places,
    Hero,
    Events,
    World,
    Log,
}

/// What the menu shows about the game right now.
pub struct DebugInfo<'a> {
    /// Our hero file as the host has it.
    pub sheet: &'a Sheet,
    /// Open incidents on the wrist radio: (call id, what, metres away).
    pub calls: Vec<(u32, &'static str, f32)>,
    /// The hour of day.
    pub hour: f32,
    /// Playing in our own world (the menu works only there).
    pub local: bool,
    /// The hero file is no longer saved (debug changes or `HW_LEVEL`).
    pub unsaved: bool,
    /// Engine's drawing-check flags (`World::set_debug_shading`).
    pub shading: u32,
}

/// The debug menu: open or not, its tab and the town's places.
#[derive(Default)]
pub struct DebugMenu {
    pub open: bool,
    tab: Tab,
    /// The current town's places (found when the menu first opens).
    pub places: Vec<Place>,
    places_of: Option<u64>,
}

/// Ctrl+Shift+F12 this frame.
pub fn hotkey(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.key_pressed(egui::Key::F12) && i.modifiers.ctrl && i.modifiers.shift)
}

/// The debug log ([`crate::devlog`]): key presses and what became of
/// them, newest at the bottom, with Copy and Clear.
fn log_tab(ui: &mut egui::Ui) {
    let lines = crate::devlog::lines();
    ui.horizontal(|ui| {
        if ui.button("Copy all").clicked() {
            ui.ctx().copy_text(lines.join("\n"));
        }
        if ui.button("Clear").clicked() {
            crate::devlog::clear();
        }
        ui.label(
            egui::RichText::new(
                "Also in logs/debug.log. Keys pressed over this window go to the menu.",
            )
            .weak()
            .size(11.0),
        );
    });
    ui.separator();
    egui::ScrollArea::both()
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for l in &lines {
                ui.label(egui::RichText::new(l).monospace().size(11.0));
            }
        });
}

/// Hours on the time buttons.
const HOURS: [(&str, f32); 7] = [
    ("Dawn 6:00", 6.0),
    ("Morning 9:00", 9.0),
    ("Noon 12:00", 12.0),
    ("Afternoon 15:30", 15.5),
    ("Golden hour 18:30", 18.5),
    ("Night 22:00", 22.0),
    ("Midnight", 0.5),
];

impl DebugMenu {
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens on a tab by name (`places`, `hero`, `events`, `world`, `log`): for
    /// screenshots (`HW_DEBUG=<tab>`).
    pub fn open_on(&mut self, tab: &str) {
        self.open = true;
        self.tab = match tab {
            "hero" => Tab::Hero,
            "events" => Tab::Events,
            "world" => Tab::World,
            "log" => Tab::Log,
            _ => Tab::Places,
        };
    }

    /// Finds the places of `world` (once per town: `key` tells towns apart).
    pub fn find_places(&mut self, key: u64, world: &Area, solids: &Obstacles) {
        if self.places_of != Some(key) {
            self.places = places(world, solids);
            self.places_of = Some(key);
        }
    }

    /// Draws the menu (when open) and returns what was clicked.
    pub fn ui(&mut self, ctx: &egui::Context, info: &DebugInfo) -> Vec<DebugAction> {
        let mut out = Vec::new();
        if !self.open {
            return out;
        }
        let mut open = self.open;
        egui::Window::new("Debug  (Ctrl+Shift+F12)")
            .id(egui::Id::new("debug menu"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(460.0)
            .default_height(560.0)
            .anchor(egui::Align2::LEFT_TOP, egui::vec2(16.0, 60.0))
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(egui::Color32::from_rgba_unmultiplied(18, 22, 30, 238)),
            )
            .show(ctx, |ui| {
                if !info.local && self.tab != Tab::Log {
                    ui.label("The debug menu works only in your own world (single player).");
                    return;
                }
                ui.horizontal(|ui| {
                    for (t, name) in [
                        (Tab::Places, "Go to"),
                        (Tab::Hero, "Hero"),
                        (Tab::Events, "Events"),
                        (Tab::World, "World"),
                        (Tab::Log, "Log"),
                    ] {
                        if ui.selectable_label(self.tab == t, name).clicked() {
                            self.tab = t;
                        }
                    }
                });
                ui.separator();
                if self.tab == Tab::Log {
                    log_tab(ui);
                    return;
                }
                egui::ScrollArea::vertical().show(ui, |ui| match self.tab {
                    Tab::Places => self.places_tab(ui, info, &mut out),
                    Tab::Hero => hero_tab(ui, info, &mut out),
                    Tab::Events => events_tab(ui, info, &mut out),
                    Tab::World => world_tab(ui, info, &mut out),
                    Tab::Log => {}
                });
            });
        self.open = open;
        out
    }

    fn places_tab(&self, ui: &mut egui::Ui, info: &DebugInfo, out: &mut Vec<DebugAction>) {
        let wide = egui::vec2(ui.available_width().min(420.0), 22.0);
        if !info.calls.is_empty() {
            ui.label(egui::RichText::new("Open calls").strong());
            for &(id, what, d) in &info.calls {
                let text = format!("{what}  ({d:.0} m)");
                if ui.add(egui::Button::new(text).min_size(wide)).clicked() {
                    out.push(DebugAction::GoCall(id));
                }
            }
            ui.add_space(6.0);
        }
        let mut group = "";
        for (i, p) in self.places.iter().enumerate() {
            if p.group != group {
                group = p.group;
                ui.add_space(4.0);
                ui.label(egui::RichText::new(group).strong());
            }
            if ui
                .add(egui::Button::new(p.name.as_str()).min_size(wide))
                .clicked()
            {
                out.push(DebugAction::Go(i));
            }
        }
    }
}

fn hero_tab(ui: &mut egui::Ui, info: &DebugInfo, out: &mut Vec<DebugAction>) {
    use hw_sim::progress;
    let s = info.sheet;
    let level = progress::level_of(s.lifetime);
    ui.label(format!(
        "Level {level}, {} karma to spend, {} powers bought",
        s.karma,
        s.powers.len()
    ));
    if info.unsaved {
        ui.label(
            egui::RichText::new("Debug changes: this hero is not saved until you restart.")
                .color(egui::Color32::from_rgb(255, 200, 90)),
        );
    }
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Level").strong());
    ui.horizontal_wrapped(|ui| {
        if ui.button("+1 level").clicked() {
            out.push(DebugAction::Level((level + 1).min(progress::MAX_LEVEL)));
        }
        for l in [1u8, 5, 10, 15, 20] {
            if ui.button(format!("Level {l}")).clicked() {
                out.push(DebugAction::Level(l));
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        for k in [1_000u32, 10_000, 100_000] {
            if ui.button(format!("+{k} karma")).clicked() {
                out.push(DebugAction::Karma(k));
            }
        }
    });
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Powers").strong());
    ui.horizontal_wrapped(|ui| {
        if ui.button("Buy the whole tree").clicked() {
            out.push(DebugAction::BuyAll);
        }
        if ui.button("Give all back").clicked() {
            out.push(DebugAction::ClearPowers);
        }
    });
    ui.label("B opens the powers window to buy one at a time.");
    ui.horizontal_wrapped(|ui| {
        ui.label("Element:");
        for e in hw_sim::powers::Element::ALL {
            let on = s.element == e.code();
            if ui.selectable_label(on, e.name()).clicked() && !on {
                out.push(DebugAction::Element(e));
            }
        }
    });
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Factions (how they see you)").strong());
    egui::Grid::new("debug regard")
        .num_columns(4)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            for f in Faction::ALL {
                let r = hw_sim::factions::toward_hero(f, &s.regard);
                ui.label(format!("{f:?}"));
                ui.label(format!(
                    "{r:+} {}",
                    hw_sim::factions::Attitude::of(r).name()
                ));
                if ui.small_button("-20").clicked() {
                    out.push(DebugAction::Regard(f, -20));
                }
                if ui.small_button("+20").clicked() {
                    out.push(DebugAction::Regard(f, 20));
                }
                ui.end_row();
            }
        });
    if ui.button("Reset standings").clicked() {
        out.push(DebugAction::ResetRegard);
    }
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        if ui.button("Full health and energy").clicked() {
            out.push(DebugAction::Heal);
        }
        if ui.button("Back to my saved hero").clicked() {
            out.push(DebugAction::RestoreHero);
        }
    });
}

fn events_tab(ui: &mut egui::Ui, info: &DebugInfo, out: &mut Vec<DebugAction>) {
    ui.label(egui::RichText::new("Start near you").strong());
    ui.label("Fights start 150 to 800 m away (the radio and compass show them; Go to lists them).");
    let wide = egui::vec2(ui.available_width().min(420.0), 22.0);
    for k in IncidentKind::ALL {
        let text = match k {
            IncidentKind::Collapse => "Building collapse (brings the nearest small house down)",
            _ => k.name(),
        };
        if ui.add(egui::Button::new(text).min_size(wide)).clicked() {
            out.push(DebugAction::Incident(k));
        }
    }
    ui.add_space(6.0);
    ui.label(egui::RichText::new("Break things around you").strong());
    ui.horizontal_wrapped(|ui| {
        if ui.button("Blast the nearest building").clicked() {
            out.push(DebugAction::BlastBuilding);
        }
        if ui.button("Wreck cars").clicked() {
            out.push(DebugAction::WreckCars);
        }
        if ui.button("Knock over lamps and trees").clicked() {
            out.push(DebugAction::SmashProps);
        }
        if ui.button("Knock people over").clicked() {
            out.push(DebugAction::KnockPeople);
        }
        if ui.button("Break a colossus weak point").clicked() {
            out.push(DebugAction::BreakColossus);
        }
    });
    if !info.calls.is_empty() {
        ui.add_space(6.0);
        ui.label(format!("{} calls open.", info.calls.len()));
    }
}

fn world_tab(ui: &mut egui::Ui, info: &DebugInfo, out: &mut Vec<DebugAction>) {
    let h = info.hour;
    ui.label(
        egui::RichText::new(format!(
            "Time of day ({:02}:{:02})",
            h as u32,
            (h.fract() * 60.0) as u32
        ))
        .strong(),
    );
    ui.horizontal_wrapped(|ui| {
        for (name, hour) in HOURS {
            if ui.button(name).clicked() {
                out.push(DebugAction::Hour(hour));
            }
        }
    });
    ui.label("Lit windows are rebuilt when day turns to night (a short stall).");
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Run the world on").strong());
    ui.horizontal_wrapped(|ui| {
        for (name, s) in [("10 s", 10u32), ("1 min", 60), ("5 min", 300)] {
            if ui.button(name).clicked() {
                out.push(DebugAction::FastForward(s));
            }
        }
    });
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Views").strong());
    ui.horizontal_wrapped(|ui| {
        if ui.button("Free camera (O)").clicked() {
            out.push(DebugAction::FreeCamera);
        }
        if ui.button("Cut buildings open").clicked() {
            out.push(DebugAction::Cutaway);
        }
        if ui.button("Cut floor down").clicked() {
            out.push(DebugAction::CutLevel(-1));
        }
        if ui.button("Cut floor up").clicked() {
            out.push(DebugAction::CutLevel(1));
        }
        if ui.button("Town map (M)").clicked() {
            out.push(DebugAction::Map);
        }
        if ui.button("Re-upload meshes").clicked() {
            out.push(DebugAction::Reupload(true));
        }
        if ui.button("Re-upload textures").clicked() {
            out.push(DebugAction::Reupload(false));
        }
        if ui.button("Check meshes on the GPU").clicked() {
            out.push(DebugAction::GpuCheck);
        }
        if ui.button("Redo window surface").clicked() {
            out.push(DebugAction::Rebuild(true));
        }
        if ui.button("Rebuild screen buffers").clicked() {
            out.push(DebugAction::Rebuild(false));
        }
    });
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Drawing checks").strong());
    ui.horizontal_wrapped(|ui| {
        for (bit, name) in engine::world::DEBUG_SHADING_FLAGS {
            let mut on = info.shading & bit != 0;
            if ui.checkbox(&mut on, name).changed() {
                out.push(DebugAction::Shading(info.shading ^ bit));
            }
        }
    });
}

/// The hero's sheet at `level`: karma earned to match, and (going up) the
/// difference in karma to spend.
pub fn at_level(mut s: Sheet, level: u8) -> Sheet {
    use hw_sim::progress;
    let level = level.clamp(1, progress::MAX_LEVEL);
    let want = progress::karma_for_level(level);
    if want > s.lifetime {
        s.karma = s.karma.saturating_add(want - s.lifetime);
    }
    s.lifetime = want;
    s
}

/// Every power of the hero's trees bought, the first of each fork (karma
/// topped up so none is refused for want of it).
pub fn buy_all(mut s: Sheet) -> Sheet {
    use hw_sim::powers::{tree, Tree};
    s.karma = s.karma.max(1_000_000);
    let mine = Tree::of_origin(s.origin);
    // Rows top down, so what a power needs is bought before it.
    let mut all: Vec<_> = tree(mine).chain(tree(Tree::Common)).collect();
    all.sort_by_key(|d| (d.row, d.col, d.tree == Tree::Common));
    for _ in 0..2 {
        for d in &all {
            let _ = hw_sim::progress::buy(&mut s, d.id);
        }
    }
    s
}

/// The sheet with every power given back (90% of each price refunded),
/// the ones others need after those.
pub fn clear_powers(mut s: Sheet) -> Sheet {
    while !s.powers.is_empty() {
        let free = s
            .powers
            .iter()
            .rev()
            .map(|o| o.power)
            .find(|&p| hw_sim::progress::can_give_up(&s, p).is_ok());
        match free {
            Some(p) => {
                let _ = hw_sim::progress::give_up(&mut s, p);
            }
            None => {
                s.powers.pop();
            }
        }
    }
    s
}

/// The sheet with `f`'s regard moved by `by` (kept in range).
pub fn regard(mut s: Sheet, f: Faction, by: i8) -> Sheet {
    let i = f.index();
    s.regard[i] = s.regard[i].saturating_add(by);
    s
}

/// The sheet with every faction's regard back to how it sees townsfolk.
pub fn reset_regard(mut s: Sheet) -> Sheet {
    s.regard = [0; FACTIONS];
    s
}

/// A spot near `p` (within 12 m, nearest first) where a hero standing at
/// `y` (or the ground there) is clear of walls and above the water.
pub fn clear_near(solids: &Obstacles, p: DVec2, y: Option<f64>) -> Option<DVec3> {
    let fits = |q: DVec2| {
        let floor = y.unwrap_or_else(|| solids.ground_at(q));
        let feet = DVec3::new(q.x, floor + 0.3, q.y);
        let ok = floor > WATER_LEVEL + 0.2
            && [0.3, BODY_HEIGHT * 0.5, BODY_HEIGHT]
                .iter()
                .all(|&h| !solids.solid_at(DVec3::new(q.x, floor + h, q.y)));
        ok.then_some(feet)
    };
    if let Some(f) = fits(p) {
        return Some(f);
    }
    for ring in 1..=6 {
        let r = ring as f64 * 2.0;
        for k in 0..16 {
            let a = k as f64 / 16.0 * std::f64::consts::TAU;
            if let Some(f) = fits(p + DVec2::new(a.cos(), a.sin()) * r) {
                return Some(f);
            }
        }
    }
    None
}

/// The town's notable places, grouped: taxi stands and landmarks, the
/// villains' quarter, under the town, and views.
pub fn places(world: &Area, solids: &Obstacles) -> Vec<Place> {
    let mut out = Vec::new();
    let (spawn, spawn_dir) = world.spawn_line();
    let add = |out: &mut Vec<Place>,
               group: &'static str,
               name: String,
               at: Option<DVec3>,
               look_at: DVec2| {
        if let Some(at) = at {
            let facing = (look_at - DVec2::new(at.x, at.z)).normalize_or(DVec2::X);
            out.push(Place {
                name,
                group,
                at,
                facing,
                fly: false,
            });
        }
    };

    // The town: where you start, taxi stands, landmarks.
    add(
        &mut out,
        "Town",
        "Where you start".into(),
        clear_near(solids, spawn, None),
        spawn + spawn_dir,
    );
    for st in hw_sim::taxi::stands(world) {
        let p = DVec2::new(st.pos.x, st.pos.z);
        // Rooftop pads stand on the deck; the rest on the ground there.
        let roof = st.name.starts_with("Rooftop");
        let at = clear_near(solids, p, roof.then_some(st.pos.y - 1.6)).or(Some(st.pos));
        add(&mut out, "Town", st.name, at, spawn);
    }
    for (kind, name) in [
        (LandmarkKind::AetherSpire, "Aether Spire, at its steps"),
        (LandmarkKind::Lighthouse, "Lighthouse"),
        (LandmarkKind::AtomicPlant, "Atomic plant, at its gate"),
        (LandmarkKind::MoleDrill, "Mole drill rig"),
        (LandmarkKind::WeatherTower, "Weather tower"),
    ] {
        if let Some(l) = crate::landmarkview::nearest(world, kind, spawn) {
            let side = l.stairs.first().map_or(l.dir, |s| s.down);
            let p = l.pos + side * (l.plaza + 8.0);
            add(
                &mut out,
                "Town",
                name.into(),
                clear_near(solids, p, None),
                l.pos,
            );
        }
    }
    // Inside the nearest hospital, by its recovery pods.
    if let Some(l) = crate::landmarkview::nearest(world, LandmarkKind::Hospital, spawn) {
        if let Some(w) = l.ward() {
            let pod = w.pods[0];
            add(
                &mut out,
                "Town",
                "Hospital, by the recovery pods".into(),
                Some(pod.1),
                DVec2::new(w.door.x, w.door.z),
            );
        }
    }
    // The Comet buildings (the new game-scale interiors): in their lobbies.
    for l in world
        .landmarks
        .iter()
        .filter(|l| l.kind == LandmarkKind::CometBuilding)
    {
        if let Some((eye, at)) = crate::cometmesh::view(l, "lobby") {
            add(
                &mut out,
                "Town",
                format!("{}, in the lobby", hw_gen::urban::comet::name(l)),
                Some(DVec3::new(eye.x, l.base + 0.1, eye.z)),
                DVec2::new(at.x, at.z),
            );
        }
    }
    // A park: beside the fountain nearest the start.
    use hw_gen::urban::props::PropKind;
    if let Some(f) = world
        .props
        .iter()
        .filter(|p| p.kind == PropKind::Fountain)
        .min_by(|a, b| a.pos.distance(spawn).total_cmp(&b.pos.distance(spawn)))
    {
        let p = f.pos + f.dir.perp() * 7.0;
        add(
            &mut out,
            "Town",
            "Park fountain".into(),
            clear_near(solids, p, None),
            f.pos,
        );
    }
    // The tallest building near the start, in its lobby.
    if let Some(b) = world
        .buildings
        .iter()
        .zip(&world.building_ids)
        .filter(|(b, _)| b.site.origin.distance(spawn) < 400.0)
        .max_by_key(|(b, id)| (b.site.floors, std::cmp::Reverse(id.0)))
        .and_then(|(_, id)| solids.building(id.0))
    {
        let (plan, base) = b;
        let lobby = plan.floors[0]
            .rooms
            .iter()
            .find(|r| r.kind == hw_gen::building::RoomKind::Lobby)
            .map(|r| r.rect.center())
            .unwrap_or(plan.floors[0].outline.center());
        let p = plan.site.to_city(DVec3::new(lobby.x, base, lobby.y));
        let p2 = DVec2::new(p.x, p.z);
        let at = clear_near(solids, p2, Some(base)).or(Some(DVec3::new(p.x, base + 0.1, p.z)));
        add(
            &mut out,
            "Town",
            format!("Lobby of the tallest tower ({} floors)", plan.floors.len()),
            at,
            p2 + plan.site.inward,
        );
    }
    // A half-timbered house near the start, from across its street.
    if let Some((plan, _)) = world
        .buildings
        .iter()
        .zip(&world.building_ids)
        .filter(|(b, _)| b.site.origin.distance(spawn) < 700.0)
        .filter_map(|(_, id)| solids.building(id.0))
        .filter(|(p, _)| crate::facades::facade(p).timber.is_some())
        .min_by(|a, b| {
            a.0.site
                .origin
                .distance(spawn)
                .total_cmp(&b.0.site.origin.distance(spawn))
        })
    {
        let s = &plan.site;
        let front = s.origin + s.along * (s.size.x * 0.5);
        let p = front - s.inward * 12.0;
        add(
            &mut out,
            "Town",
            "Half-timbered house".into(),
            clear_near(solids, p, None),
            front,
        );
    }
    out.push(Place {
        name: "High above the town centre (flying)".into(),
        group: "Town",
        at: DVec3::new(
            spawn.x - 40.0,
            solids.ground_at(spawn) + 260.0,
            spawn.y + 40.0,
        ),
        facing: DVec2::new(1.0, -1.0).normalize(),
        fly: true,
    });

    // The villains: the mad scientist's quarter, its lookouts, the gangs.
    let tower = crate::landmarkview::nearest(world, LandmarkKind::TeslaTower, spawn);
    if let Some(l) = tower {
        let deep = |p: DVec2| {
            world
                .town
                .dystopia
                .as_ref()
                .is_some_and(|d| d.depth(p) > 0.9)
        };
        let p = world
            .roads
            .iter()
            .flat_map(|r| r.pts.iter().copied())
            .filter(|p| deep(*p))
            .min_by(|a, b| {
                (a.distance(l.pos) - 110.0)
                    .abs()
                    .total_cmp(&(b.distance(l.pos) - 110.0).abs())
            });
        if let Some(p) = p {
            add(
                &mut out,
                "Villains",
                "Mad scientist's quarter, below the Tesla tower".into(),
                clear_near(solids, p, None),
                l.pos,
            );
        }
    }
    if let Some(l) = crate::landmarkview::nearest(world, LandmarkKind::RobotWorks, spawn) {
        // Outside the gate, the line and the dock in view.
        let p = l.pos + l.dir * (l.plaza + 8.0);
        add(
            &mut out,
            "Villains",
            "Robot works, at its gate".into(),
            clear_near(solids, p, None),
            l.pos,
        );
    }
    let corners = hw_sim::quarter::corners(world);
    if let Some(l) = hw_sim::quarter::lookouts(world, &corners).first() {
        if let Some(top) = hw_sim::quarter::perch(solids, l) {
            let p = DVec2::new(top.x, top.z) - l.facing * 2.0;
            let at = clear_near(solids, p, Some(top.y)).or(Some(top + DVec3::Y * 0.3));
            add(
                &mut out,
                "Villains",
                "Rooftop lookout (on the roof)".into(),
                at,
                DVec2::new(top.x, top.z) + l.facing * 20.0,
            );
        }
    }
    if let Some(c) = corners.first() {
        let p = c.at + c.facing * 25.0;
        add(
            &mut out,
            "Villains",
            "Villain street corner".into(),
            clear_near(solids, p, None),
            c.at,
        );
    }
    if let Some(c) = hw_sim::quarter::hangouts(world, spawn, hw_sim::HANGOUT_CLEAR).first() {
        let p = c.at + c.facing.perp() * 26.0 + c.facing * 6.0;
        add(
            &mut out,
            "Villains",
            "Chrome Jackets hangout".into(),
            clear_near(solids, p, None),
            c.at,
        );
    }
    if let Some(b) = hw_gen::urban::edge::barricades(world, &world.roads).first() {
        let p = b.at - b.along * 14.0;
        add(
            &mut out,
            "Villains",
            "Barricade at the quarter's edge".into(),
            clear_near(solids, p, None),
            b.at,
        );
    }
    {
        let tags = hw_gen::urban::edge::tags(
            world,
            world.building_ids.iter().copied().zip(&world.buildings),
        );
        if let Some(t) = tags.iter().find(|t| t.image == 0).or(tags.first()) {
            let at = DVec2::new(t.at.x, t.at.z);
            let p = at + t.normal * 6.0;
            add(
                &mut out,
                "Villains",
                "Graffiti wall".into(),
                clear_near(solids, p, None),
                at,
            );
        }
    }
    {
        use hw_gen::building::BuildingUse;
        if let Some(b) = world
            .buildings
            .iter()
            .filter(|b| b.site.usage == BuildingUse::Lab)
            .min_by(|a, b| {
                a.site
                    .origin
                    .distance(spawn)
                    .total_cmp(&b.site.origin.distance(spawn))
            })
        {
            let s = &b.site;
            let front = s.origin + s.along * s.size.x * 0.5;
            add(
                &mut out,
                "Villains",
                "A lab (breakouts start here)".into(),
                clear_near(solids, front - s.inward * 8.0, None),
                front,
            );
        }
    }

    // Under the town.
    let u = world.underground();
    if let Some(e) = u
        .entrances
        .iter()
        .filter(|e| !e.molehaven)
        .min_by(|a, b| a.pos.distance(spawn).total_cmp(&b.pos.distance(spawn)))
    {
        let p = e.pos + e.door * 6.0;
        add(
            &mut out,
            "Underground",
            format!("Lift down: {}", e.name),
            clear_near(solids, p, None),
            e.pos,
        );
    }
    if let Some(v) = u
        .voids
        .iter()
        .filter(|v| v.kind == hw_gen::urban::underground::VoidKind::Tunnel)
        .filter(|v| v.max.x - v.min.x > 30.0)
        .min_by(|a, b| {
            a.origin
                .distance(spawn)
                .total_cmp(&b.origin.distance(spawn))
        })
    {
        let p = v.world(DVec2::new(v.min.x + 4.0, (v.min.y + v.max.y) * 0.5));
        let y = v.floor_at(p);
        add(
            &mut out,
            "Underground",
            "Utility tunnel".into(),
            Some(DVec3::new(p.x, y + 0.3, p.y)),
            p + v.along,
        );
    }
    if let Some(c) = &u.cavern {
        if let Some(hall) = c.chambers.first().and_then(|&i| u.voids.get(i)) {
            let p = hall.world(DVec2::new(
                hall.max.x - 4.0,
                (hall.min.y + hall.max.y) * 0.5,
            ));
            add(
                &mut out,
                "Underground",
                "Molehaven, the great hall".into(),
                Some(DVec3::new(p.x, hall.floor_at(p) + 0.3, p.y)),
                c.centre,
            );
        }
        let p = c.centre + DVec2::new(9.0, 0.0);
        add(
            &mut out,
            "Underground",
            "Molehaven market".into(),
            Some(DVec3::new(p.x, c.floor + 0.3, p.y)),
            c.centre,
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn level_and_powers_helpers() {
        let s = at_level(hw_sim::progress::starter(), 10);
        assert_eq!(hw_sim::progress::level_of(s.lifetime), 10);
        assert!(s.karma >= hw_sim::progress::karma_for_level(10) - 1);
        let s = buy_all(s);
        // Half the tree's forks are closed: the rest of the tree and
        // every travel power.
        assert!(s.powers.len() >= 20, "{}", s.powers.len());
        assert!(s.has(hw_net::PowerId(5)) && s.has(hw_net::PowerId(8)));
        let karma = s.karma;
        let s = clear_powers(s);
        assert!(s.powers.is_empty());
        assert!(s.karma > karma);
        let s = regard(s, Faction::Jackets, 100);
        let s = regard(s, Faction::Jackets, 100);
        assert_eq!(s.regard[Faction::Jackets.index()], i8::MAX);
        assert_eq!(reset_regard(s).regard, [0; FACTIONS]);
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn places_are_found_and_clear() {
        let town = hw_sim::GeneratedTown::generate(hw_core::Seed(1), hw_sim::DEFAULT_POPULATION);
        let solids = &town.obstacles;
        let ps = places(&town.area, solids);
        let names: Vec<&str> = ps.iter().map(|p| p.name.as_str()).collect();
        for want in [
            "Where you start",
            "Aether Spire, at its steps",
            "Molehaven market",
        ] {
            assert!(names.contains(&want), "{want} missing from {names:?}");
        }
        assert!(
            ps.iter().any(|p| p.group == "Villains"),
            "no villain places in {names:?}"
        );
        for p in &ps {
            if p.fly || p.group == "Underground" {
                continue;
            }
            assert!(
                !solids.solid_at(p.at + DVec3::Y * 1.0),
                "{} puts the hero inside something at {:?}",
                p.name,
                p.at
            );
        }
    }
}
