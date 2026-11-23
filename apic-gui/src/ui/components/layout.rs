//! Framing shared across features: the URL bar and the empty state.

use eframe::egui;

use crate::ui::base::{Size, Text};
use crate::ui::theme::{SPACE_EXTRA_LARGE, SPACE_LARGE, SPACE_SMALL, palette};

/// A rounded bar with an `outline_variant` border and a `surface_container`
/// fill, as wide as the space it sits in, laid out left to right, for a
/// method and URL.
pub(crate) fn url_bar<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let p = palette(ui.ctx());
    egui::Frame::NONE
        .fill(p.surface_container)
        .stroke(egui::Stroke::new(1.0, p.outline_variant))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(8, 8))
        .show(ui, |ui| {
            // Fill the pane's width like the tables below, rather than hugging
            // the URL text.
            ui.set_min_width(ui.available_width());
            ui.set_min_height(20.0);
            ui.horizontal(add).inner
        })
        .inner
}

/// A pane's content area: `add` inside the standard padding on every side.
pub(crate) fn pane<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::NONE
        .inner_margin(egui::Margin::same(SPACE_LARGE as i8))
        .show(ui, add)
        .inner
}

/// A frame padding only the `side` of a split pane that faces the other
/// half, so the two halves keep the standard gap between them.
pub(crate) fn split_inset(side: Side) -> egui::Frame {
    let gap = SPACE_LARGE as i8;
    let margin = match side {
        Side::Left => egui::Margin {
            left: gap,
            ..Default::default()
        },
        Side::Right => egui::Margin {
            right: gap,
            ..Default::default()
        },
    };
    egui::Frame::NONE.inner_margin(margin)
}

/// Which side of something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    Left,
    Right,
}

/// A centred muted title with an optional hint below, for a pane with nothing to show.
pub(crate) fn empty_state(ui: &mut egui::Ui, title: &str, hint: &str) {
    ui.add_space(SPACE_EXTRA_LARGE);
    ui.vertical_centered(|ui| {
        Text::new(title).size(Size::Heading).muted().show(ui);
        if !hint.is_empty() {
            ui.add_space(SPACE_SMALL);
            Text::new(hint).muted().show(ui);
        }
    });
}
