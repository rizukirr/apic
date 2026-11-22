//! Pills: the outlined chip and the filled badge.

use eframe::egui;
use egui::RichText;

use crate::ui::base::{Text, Tone};
use crate::ui::theme::{SIZE_CAPTION, palette};

/// The colours of a [`chip`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChipTone {
    /// Muted text on `secondary_container`.
    Neutral,
    /// `info` text on `secondary_container`.
    Info,
    /// `on_error_container` text on `error_container`.
    Danger,
}

/// A small caption-sized pill. When `clickable` it is a frameless button and
/// the return is whether it was clicked; otherwise it is a label and the
/// return is always `false`.
pub(crate) fn chip(ui: &mut egui::Ui, label: &str, tone: ChipTone, clickable: bool) -> bool {
    let p = palette(ui.ctx());
    let (fg, bg) = match tone {
        ChipTone::Neutral => (p.on_surface_variant, p.secondary_container),
        ChipTone::Info => (p.info, p.secondary_container),
        ChipTone::Danger => (p.on_error_container, p.error_container),
    };
    let text = RichText::new(format!(" {label} "))
        .color(fg)
        .size(SIZE_CAPTION)
        .background_color(bg);
    if clickable {
        ui.add(egui::Button::new(text).frame(false)).clicked()
    } else {
        ui.label(text);
        false
    }
}

/// The `Required` / `Optional` chip for the REQUIREMENT column. In edit mode
/// it is a button that returns `Some(new_value)` when clicked; in view mode
/// it is a static label returning `None`.
pub(crate) fn required_chip(ui: &mut egui::Ui, required: bool, editing: bool) -> Option<bool> {
    let (label, tone) = if required {
        ("Required", ChipTone::Danger)
    } else {
        ("Optional", ChipTone::Neutral)
    };
    (chip(ui, label, tone, editing) && editing).then_some(!required)
}

/// A filled, rounded pill in `tone` with bold `surface` text, such as the
/// method of a request.
pub(crate) fn badge(ui: &mut egui::Ui, label: &str, tone: Tone) {
    let p = palette(ui.ctx());
    egui::Frame::NONE
        .fill(tone.color(p))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(8, 2))
        .show(ui, |ui| {
            Text::new(label).tone(Tone::Surface).strong().show(ui);
        });
}
