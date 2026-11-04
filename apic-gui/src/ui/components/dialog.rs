//! The one popup layout every dialog shares: title left, close X right,
//! body, buttons right-aligned.

use eframe::egui;
use egui::RichText;

use crate::ui::components::icon_button;
use crate::ui::theme::{Palette, palette};

/// Every dialog is this wide, so popups read as one family.
const DIALOG_WIDTH: f32 = 420.0;

/// How a dialog button looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ButtonKind {
    /// Frameless text in `on_surface_variant`.
    Secondary,
    /// Frameless text in `error`.
    SecondaryDanger,
    /// Filled `primary` with `on_primary` text.
    Primary,
    /// Filled `error` with `surface` text.
    Danger,
}

/// One button in a dialog's footer.
pub(crate) struct DialogButton<'a> {
    /// The button's text.
    pub(crate) label: &'a str,
    /// How the button looks.
    pub(crate) kind: ButtonKind,
    /// Whether the button can be clicked.
    pub(crate) enabled: bool,
}

impl<'a> DialogButton<'a> {
    /// An enabled button labelled `label` drawn as `kind`.
    pub(crate) fn new(label: &'a str, kind: ButtonKind) -> Self {
        Self {
            label,
            kind,
            enabled: true,
        }
    }
}

/// What happened in a dialog this frame.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DialogOutcome {
    /// The index into `buttons` of the button clicked this frame.
    pub(crate) clicked: Option<usize>,
    /// The X, Escape or a click outside closed it this frame.
    pub(crate) closed: bool,
}

/// Draws a modal popup: a header with `title` and a close X, a separator,
/// `body`, then `buttons` right-aligned in the order given.
///
/// # Arguments
///
/// * `ctx` - the egui context to draw the modal on
/// * `id` - a stable id for the modal's area, unique per dialog
/// * `title` - the header text
/// * `body` - draws the dialog's content
/// * `buttons` - the footer buttons, read left to right
///
/// # Returns
///
/// Which button was clicked this frame, and whether the dialog was closed.
pub(crate) fn dialog(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    body: impl FnOnce(&mut egui::Ui),
    buttons: &[DialogButton],
) -> DialogOutcome {
    let p = palette(ctx);
    let frame = egui::Frame::window(&ctx.style_of(ctx.theme())).inner_margin(0);
    let mut outcome = DialogOutcome::default();
    let modal = egui::Modal::new(egui::Id::new(id))
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_min_width(DIALOG_WIDTH);
            ui.set_max_width(DIALOG_WIDTH);
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(16, 8))
                .show(ui, |ui| {
                    // `Sides` keeps the row as tall as its contents. A nested
                    // right-to-left layout here would grow to the modal's
                    // full available height and leave a gap under the title.
                    egui::containers::Sides::new().show(
                        ui,
                        |ui| ui.label(RichText::new(title).color(p.on_surface).size(15.0)),
                        |ui| {
                            if icon_button(ui, egui_phosphor::regular::X, "Close").clicked() {
                                outcome.closed = true;
                            }
                        },
                    );
                });
            ui.separator();
            egui::Frame::new().inner_margin(16).show(ui, |ui| {
                ui.set_width(ui.available_width());
                body(ui);
            });
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(16, 12))
                .show(ui, |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // right_to_left places the first widget rightmost, so
                        // walk backwards to keep the given reading order.
                        for (index, button) in buttons.iter().enumerate().rev() {
                            if footer_button(ui, button, p) {
                                outcome.clicked = Some(index);
                            }
                        }
                    });
                });
        });
    if modal.should_close() {
        outcome.closed = true;
    }
    outcome
}

/// Draws one footer button in its kind's look. Returns `true` on click.
fn footer_button(ui: &mut egui::Ui, button: &DialogButton, p: &Palette) -> bool {
    let (text, fill) = match button.kind {
        ButtonKind::Secondary => (p.on_surface_variant, None),
        ButtonKind::SecondaryDanger => (p.error, None),
        ButtonKind::Primary => (p.on_primary, Some(p.primary)),
        ButtonKind::Danger => (p.surface, Some(p.error)),
    };
    let mut widget =
        egui::Button::new(RichText::new(button.label).color(text)).min_size(egui::vec2(0.0, 28.0));
    widget = match fill {
        Some(fill) => widget.fill(fill).corner_radius(4),
        None => widget.frame_when_inactive(false),
    };
    ui.add_enabled(button.enabled, widget).clicked()
}

/// Draws `add` inside a full-width rounded box on `surface_container`, for
/// the name or path a dialog is about.
pub(crate) fn detail_box(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    let p = palette(ui.ctx());
    egui::Frame::new()
        .fill(p.surface_container)
        .corner_radius(6)
        .inner_margin(12)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::{BOTH_THEMES, install_themes, run_in_theme};

    const ID: &str = "test_dialog";

    fn buttons() -> [DialogButton<'static>; 2] {
        [
            DialogButton::new("Cancel", ButtonKind::Secondary),
            DialogButton::new("Remove", ButtonKind::Danger),
        ]
    }

    fn run(ctx: &egui::Context, events: Vec<egui::Event>) -> DialogOutcome {
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let input = egui::RawInput {
            screen_rect: Some(screen),
            events,
            ..Default::default()
        };
        let mut outcome = DialogOutcome::default();
        let mut out = ctx.run_ui(input, |ui| {
            install_themes(ui.ctx(), egui::ThemePreference::Dark);
            outcome = dialog(
                ui.ctx(),
                ID,
                "Remove collection",
                |ui| {
                    ui.label("Are you sure?");
                },
                &buttons(),
            );
        });
        out.textures_delta.clear();
        outcome
    }

    #[test]
    fn dialog_reports_the_clicked_button() {
        let ctx = egui::Context::default();
        // Two idle frames let the modal settle at its centred position.
        run(&ctx, Vec::new());
        assert_eq!(run(&ctx, Vec::new()), DialogOutcome::default());
        let rect = ctx
            .memory(|m| m.area_rect(egui::Id::new(ID)))
            .expect("modal drawn");
        // The last button sits against the footer's right and bottom padding.
        let pos = egui::pos2(rect.right() - 16.0 - 10.0, rect.bottom() - 12.0 - 10.0);
        let press = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run(&ctx, vec![egui::Event::PointerMoved(pos), press(true)]);
        let outcome = run(&ctx, vec![press(false)]);
        assert_eq!(outcome.clicked, Some(1), "Remove is the second button");
        assert!(!outcome.closed);
    }

    #[test]
    fn dialog_reports_a_close() {
        let ctx = egui::Context::default();
        run(&ctx, Vec::new());
        assert!(!run(&ctx, Vec::new()).closed);
        let escape = egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let outcome = run(&ctx, vec![escape]);
        assert!(outcome.closed, "Escape closes the dialog");
        assert_eq!(outcome.clicked, None);
    }

    #[test]
    fn dialog_renders_each_button_kind_in_both_themes() {
        let kinds = [
            DialogButton::new("Keep", ButtonKind::Secondary),
            DialogButton::new("Discard", ButtonKind::SecondaryDanger),
            DialogButton {
                label: "Save",
                kind: ButtonKind::Primary,
                enabled: false,
            },
            DialogButton::new("Delete", ButtonKind::Danger),
        ];
        for preference in BOTH_THEMES {
            run_in_theme(preference, |ui| {
                let outcome = dialog(
                    ui.ctx(),
                    ID,
                    "Every kind",
                    |ui| detail_box(ui, |ui| drop(ui.label("name"))),
                    &kinds,
                );
                assert_eq!(outcome.clicked, None);
            });
        }
    }
}
