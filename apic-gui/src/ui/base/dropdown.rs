//! Dropdowns: a labelled trigger that opens a menu of options, and a select
//! box that shows the current choice. Every dropdown in the app is one of
//! these, so they share one look.

use std::fmt::Debug;
use std::hash::Hash;

use eframe::egui;

use crate::ui::base::Text;

/// A dropdown: `label` followed by a `⌄` caret, opening `contents` as a menu
/// below it. The caret is added here, so callers pass the label alone.
///
/// # Returns
///
/// The trigger's response, and what `contents` returned while the menu was
/// open.
pub(crate) fn dropdown<R>(
    ui: &mut egui::Ui,
    label: Text,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    let label = label
        .append(&format!(" {}", egui_phosphor::regular::CARET_DOWN))
        .rich(ui.ctx());
    ui.menu_button(label, contents)
}

/// One option inside an open dropdown or [`Select`]: highlighted while
/// `selected`. A click closes the menu.
///
/// # Returns
///
/// Whether the option was clicked.
pub(crate) fn dropdown_option(ui: &mut egui::Ui, selected: bool, label: Text) -> bool {
    let label = label.rich(ui.ctx());
    let clicked = ui.selectable_label(selected, label).clicked();
    if clicked {
        ui.close();
    }
    clicked
}

/// A select box: a box showing the current choice that opens its options
/// below it, filled with [`dropdown_option`]s.
///
/// # Examples
///
/// ```ignore
/// Select::new("method", Text::new("GET")).width(90.0).show(ui, |ui| {
///     if dropdown_option(ui, false, Text::new("POST")) { /* pick it */ }
/// });
/// ```
#[must_use]
pub(crate) struct Select {
    id: egui::Id,
    selected: Text,
    width: Option<f32>,
    frameless: bool,
}

impl Select {
    /// A select box identified by `id_salt` (unique per box on screen)
    /// showing `selected`.
    pub(crate) fn new(id_salt: impl Hash + Debug, selected: Text) -> Self {
        Self {
            id: egui::Id::new(id_salt),
            selected,
            width: None,
            frameless: false,
        }
    }

    /// Makes the box exactly `width` wide.
    pub(crate) fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Draws no box while closed, only the choice and the arrow, for a
    /// select that reads as part of a line of text.
    pub(crate) fn frameless(mut self) -> Self {
        self.frameless = true;
        self
    }

    /// Draws the box, with `contents` filling its options while open.
    ///
    /// # Returns
    ///
    /// What `contents` returned, when the box was open this frame.
    pub(crate) fn show<R>(
        self,
        ui: &mut egui::Ui,
        contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> Option<R> {
        let selected = self.selected.rich(ui.ctx());
        ui.scope(|ui| {
            if self.frameless {
                let widgets = &mut ui.visuals_mut().widgets;
                for w in [
                    &mut widgets.inactive,
                    &mut widgets.hovered,
                    &mut widgets.active,
                    &mut widgets.open,
                ] {
                    w.bg_fill = egui::Color32::TRANSPARENT;
                    w.weak_bg_fill = egui::Color32::TRANSPARENT;
                    w.bg_stroke = egui::Stroke::NONE;
                }
            }
            let mut combo = egui::ComboBox::from_id_salt(self.id).selected_text(selected);
            if let Some(width) = self.width {
                combo = combo.width(width);
            }
            combo.show_ui(ui, contents).inner
        })
        .inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::{BOTH_THEMES, run_in_theme};

    #[test]
    fn the_dropdown_label_ends_with_the_caret() {
        let label = Text::new("Import").append(&format!(" {}", egui_phosphor::regular::CARET_DOWN));
        let ctx = egui::Context::default();
        let text = label.rich(&ctx).text().to_string();
        assert!(text.ends_with(egui_phosphor::regular::CARET_DOWN), "{text}");
    }

    #[test]
    fn dropdowns_and_selects_render_in_both_themes() {
        for preference in BOTH_THEMES {
            run_in_theme(preference, |ui| {
                dropdown(ui, Text::new("Branch: main"), |ui| {
                    dropdown_option(ui, true, Text::new("main"));
                });
                Select::new("framed", Text::new("GET"))
                    .width(90.0)
                    .show(ui, |ui| dropdown_option(ui, false, Text::new("POST")));
                Select::new("frameless", Text::new("200").muted())
                    .frameless()
                    .show(ui, |_| ());
            });
        }
    }
}
