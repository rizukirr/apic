//! Sidebar primitives: the row action menu and the collapsible section header.

use eframe::egui;

use crate::ui::base::{Text, plain_button};

/// The `⋮` action button at the right edge of a sidebar row. The button is
/// always laid out, so its menu stays open while the pointer moves into the
/// popup, but its glyph is only painted while `row_hovered` is true or the
/// menu is open. `menu` fills the popup.
pub(crate) fn row_menu_button(
    ui: &mut egui::Ui,
    row_hovered: bool,
    menu: impl FnOnce(&mut egui::Ui),
) {
    let glyph = Text::new(egui_phosphor::regular::DOTS_THREE_VERTICAL).muted();
    let glyph = if row_hovered { glyph } else { glyph.hidden() };
    ui.scope(|ui| {
        let w = &mut ui.style_mut().visuals.widgets;
        w.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
        w.inactive.bg_stroke = egui::Stroke::NONE;
        ui.menu_button(glyph.rich(ui.ctx()), menu);
    });
}

/// One collapsible section header row: a caret, then `label`, with
/// `trailing` controls reserved at the right edge first so a long label
/// truncates instead of pushing them off. Returns whether the label was
/// clicked, which the caller treats as a toggle.
pub(crate) fn section_header(
    ui: &mut egui::Ui,
    open: bool,
    label: Text,
    trailing: impl FnOnce(&mut egui::Ui),
) -> bool {
    let caret = if open {
        egui_phosphor::regular::CARET_DOWN
    } else {
        egui_phosphor::regular::CARET_RIGHT
    };
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            trailing(ui);
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                Text::new(caret).show(ui);
                clicked = plain_button(ui, label.rich(ui.ctx())).clicked();
            });
        });
    });
    clicked
}
