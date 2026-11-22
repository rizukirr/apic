//! Nested submenus inside an open menu.

use eframe::egui;

/// A nested menu inside an open menu: `label` opens `contents` beside it on
/// hover, and egui marks it with an arrow. When `enabled` is false the entry
/// is greyed out and does not open.
pub(crate) fn submenu(
    ui: &mut egui::Ui,
    enabled: bool,
    label: impl Into<egui::WidgetText>,
    contents: impl FnOnce(&mut egui::Ui),
) {
    ui.add_enabled_ui(enabled, |ui| {
        ui.menu_button(label, contents);
    });
}
