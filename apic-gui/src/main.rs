//! Desktop GUI front-end for apic.
//!
//! A thin presentation layer over [`apic_core`]: it discovers and loads
//! contracts, displays them in a styled, panelled layout (a viewer that mirrors
//! `apic read`), and edits them through the shared [`apic_core::edit`] model.
//! The GUI owns only its widgets, theme, and layout, never the editing behavior,
//! so it cannot drift from the CLI/TUI.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;

mod app;
mod desktop;
mod features;
mod settings;
mod ui;

use app::App;
use ui::theme::apply_theme;

fn main() -> eframe::Result {
    if std::env::args().skip(1).any(|a| a == "--desktop-entry") {
        match desktop::install_desktop_entry() {
            Ok(msg) => {
                println!("{msg}");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            // Stable app id => X11 WM_CLASS / Wayland app_id, which the Linux
            // .desktop entry matches via StartupWMClass so the launcher shows
            // the right name and icon for the running window. Inside a flatpak
            // the runtime sets FLATPAK_ID (io.github.rizukirr.apic); matching it
            // lets the compositor associate the window with the installed entry.
            .with_app_id(std::env::var("FLATPAK_ID").unwrap_or_else(|_| "apic-gui".to_string()))
            .with_icon(load_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "apic",
        options,
        Box::new(|cc| {
            apply_theme(&cc.egui_ctx);
            Ok(Box::new(App::new()))
        }),
    )
}

/// The window / taskbar icon, decoded from the PNG bundled with the crate.
fn load_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
        .expect("bundled icon.png is a valid PNG")
}
