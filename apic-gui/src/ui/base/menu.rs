//! Menu entries: the items inside an open menu, and the icon button that
//! opens a nested menu.

use eframe::egui;
use egui::containers::menu::SubMenuButton;

use crate::ui::base::Text;

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

/// A menu entry that is greyed out when `enabled` is false. A disabled
/// entry shows `disabled_hint` on hover, when given, to say why. A click on
/// an enabled entry closes the menu.
///
/// # Returns
///
/// Whether the item was clicked.
pub(crate) fn menu_item_enabled(
    ui: &mut egui::Ui,
    enabled: bool,
    label: &str,
    disabled_hint: Option<&str>,
) -> bool {
    let mut response = ui.add_enabled(enabled, egui::Button::new(label));
    if let Some(hint) = disabled_hint {
        response = response.on_disabled_hover_text(hint);
    }
    let clicked = response.clicked();
    if clicked {
        ui.close();
    }
    clicked
}

/// A selectable entry in an open menu, at least `min_width` wide with its
/// label on the left, so a column of entries lines up. Highlighted while
/// `selected`. Does not close the menu; the caller decides.
pub(crate) fn menu_choice(
    ui: &mut egui::Ui,
    selected: bool,
    label: &str,
    min_width: f32,
) -> egui::Response {
    // A growing atom after the label pushes it to the left edge. No gap, so
    // the atom adds no width.
    ui.add(
        egui::Button::selectable(selected, (label, egui::Atom::grow()))
            .gap(0.0)
            .min_size(egui::vec2(min_width, 0.0)),
    )
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

/// A bare icon that opens `contents` as a menu, with no caret and no box
/// while inactive, such as the `⋮` at the end of a row. `glyph` carries the
/// icon's tone, or is hidden to keep its space while not shown.
pub(crate) fn icon_menu(ui: &mut egui::Ui, glyph: Text, contents: impl FnOnce(&mut egui::Ui)) {
    let glyph = glyph.rich(ui.ctx());
    ui.scope(|ui| {
        let w = &mut ui.visuals_mut().widgets;
        w.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
        w.inactive.bg_stroke = egui::Stroke::NONE;
        ui.menu_button(glyph, contents);
    });
}

/// An icon button inside an open menu that opens `contents` as a nested
/// menu beside it, such as the `⋮` on a row of a menu.
pub(crate) fn icon_submenu(ui: &mut egui::Ui, icon: &str, contents: impl FnOnce(&mut egui::Ui)) {
    let glyph = Text::new(icon).rich(ui.ctx());
    SubMenuButton::from_button(egui::Button::new(glyph)).ui(ui, contents);
}
