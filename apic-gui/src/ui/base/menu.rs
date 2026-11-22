//! The menu item: one clickable entry inside an open menu.

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
