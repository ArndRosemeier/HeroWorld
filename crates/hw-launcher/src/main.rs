//! The HeroWorld launcher: installs the game from the nightly GitHub build,
//! keeps it up to date (downloading only the parts that changed), shows
//! what's new and starts it, alone or with friends.
//!
//! It is a small egui window in the title screen's comic style. All
//! network and disk work runs on a worker thread; the window only shows
//! where it is.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod config;
mod install;
mod manifest;
mod net;
mod ui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("HeroWorld Launcher")
            .with_inner_size([1000.0, 640.0])
            .with_min_inner_size([760.0, 520.0])
            .with_icon(ui::icon()),
        ..Default::default()
    };
    eframe::run_native(
        "HeroWorld Launcher",
        options,
        Box::new(|cc| Ok(Box::new(ui::Launcher::new(cc)))),
    )
}
