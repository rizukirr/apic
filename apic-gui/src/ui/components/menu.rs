//! Menu primitives: items and nested submenus inside an open egui menu.

use eframe::egui;

/// A clickable entry in an open menu. A click closes the whole menu, so the
/// caller only reacts to the returned flag.
///
/// # Returns
///
/// Whether the item was clicked.
pub(crate) fn menu_item(ui: &mut egui::Ui, label: impl Into<egui::WidgetText>) -> bool {
    let clicked = ui.button(label).clicked();
    if clicked {
        ui.close();
    }
    clicked
}

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
