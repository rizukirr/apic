//! Buttons. Every action button in the app is one of these, so buttons share
//! one look.

use eframe::egui;

use crate::ui::base::{Text, Tone};
use crate::ui::theme::{SIZE_DELETE_GLYPH, palette};

/// How a [`button`] looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ButtonKind {
    /// Frameless text in the muted tone.
    Secondary,
    /// Frameless text in the error tone.
    SecondaryDanger,
    /// Filled `primary` with `on_primary` text.
    Primary,
    /// Filled `error` with `surface` text.
    Danger,
}

/// The height of a [`button`], so filled and frameless ones line up.
const BUTTON_H: f32 = 28.0;

/// A button drawn as `kind`: filled for [`ButtonKind::Primary`] and
/// [`ButtonKind::Danger`], frameless until hovered for the secondary kinds.
/// Returns the response so callers can chain `.clicked()`.
pub(crate) fn button(ui: &mut egui::Ui, label: &str, kind: ButtonKind) -> egui::Response {
    let p = palette(ui.ctx());
    let (tone, fill) = match kind {
        ButtonKind::Secondary => (Tone::Muted, None),
        ButtonKind::SecondaryDanger => (Tone::Error, None),
        ButtonKind::Primary => (Tone::OnPrimary, Some(p.primary)),
        ButtonKind::Danger => (Tone::Surface, Some(p.error)),
    };
    let text = Text::new(label).tone(tone).rich(ui.ctx());
    let mut widget = egui::Button::new(text).min_size(egui::vec2(0.0, BUTTON_H));
    widget = match fill {
        Some(fill) => widget.fill(fill).corner_radius(4),
        None => widget.frame_when_inactive(false),
    };
    ui.add(widget)
}

/// A frameless button showing `text`, truncated with an ellipsis when its
/// space runs out. For text that is itself the control, such as a tab or a
/// section title. `text` is a [`Text::rich`] or a multi-part layout job.
/// Returns the response so callers can chain `.clicked()`.
pub(crate) fn plain_button(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    ui.add(egui::Button::new(text).frame(false).truncate())
}

/// A frameless trash-icon row-removal button: muted by default, brightening to red
/// over a subtle rounded backdrop on hover so it clearly reads as clickable.
/// Returns `true` on click.
#[must_use]
pub(crate) fn delete_button(ui: &mut egui::Ui) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
    let resp = resp.on_hover_text("Remove");
    if ui.is_rect_visible(rect) {
        let p = palette(ui.ctx());
        let hovered = resp.hovered();
        if hovered {
            ui.painter()
                .rect_filled(rect, 4.0, ui.visuals().widgets.hovered.weak_bg_fill);
        }
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            egui_phosphor::regular::TRASH,
            egui::FontId::proportional(SIZE_DELETE_GLYPH),
            if hovered {
                p.error
            } else {
                p.on_surface_variant
            },
        );
    }
    resp.clicked()
}

/// A frameless icon button: `icon` (an `egui_phosphor::regular` constant) in
/// the muted tone, with egui's hover backdrop so it reads as clickable.
/// Returns the response so callers can chain `.clicked()`.
pub(crate) fn icon_button(ui: &mut egui::Ui, icon: &str, hover_text: &str) -> egui::Response {
    let text = Text::new(icon).muted().rich(ui.ctx());
    ui.add(egui::Button::new(text).frame_when_inactive(false))
        .on_hover_text(hover_text)
}

/// A flat text button: `label` in `tone`, framed so egui gives it the
/// standard hover and press feedback. Returns `true` on click.
#[must_use]
pub(crate) fn text_button(ui: &mut egui::Ui, label: &str, tone: Tone) -> bool {
    let text = Text::new(label).tone(tone).rich(ui.ctx());
    ui.button(text).clicked()
}
