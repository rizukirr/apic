//! Framing helpers shared across features: the URL bar, its variable-coloured
//! layout job and the empty state.

use eframe::egui;
use egui::RichText;

use crate::ui::theme::{Palette, SPACE_SMALL, palette};

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

/// Lays out `text` in `font` with every variable reference found by
/// [`apic_core::vars::tokens`] coloured: `$name`, `${name}`, `{{name}}` and
/// `{name}`. A reference whose name `defined` accepts is drawn in
/// `syntax_number`, one it rejects in `warning`, and the rest of the text in
/// `on_surface`. Only the text itself is drawn, never a variable's value.
///
/// # Arguments
///
/// * `text` - the URL or value to lay out
/// * `p` - the palette to colour from
/// * `font` - the font of every section
/// * `defined` - whether the selected environment defines a name
///
/// # Returns
///
/// The coloured layout job.
pub(crate) fn url_layout_job(
    text: &str,
    p: &Palette,
    font: egui::FontId,
    defined: &dyn Fn(&str) -> bool,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let format = |color| egui::TextFormat::simple(font.clone(), color);
    let mut at = 0;
    for token in apic_core::vars::tokens(text) {
        if token.range.start > at {
            job.append(&text[at..token.range.start], 0.0, format(p.on_surface));
        }
        let color = if defined(&token.name) {
            p.syntax_number
        } else {
            p.warning
        };
        job.append(&text[token.range.clone()], 0.0, format(color));
        at = token.range.end;
    }
    if at < text.len() {
        job.append(&text[at..], 0.0, format(p.on_surface));
    }
    job
}

/// A centred muted title with an optional hint below, for a pane with nothing to show.
pub(crate) fn empty_state(ui: &mut egui::Ui, title: &str, hint: &str) {
    let p = palette(ui.ctx());
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(title).color(p.on_surface_variant).size(16.0));
        if !hint.is_empty() {
            ui.add_space(SPACE_SMALL);
            ui.label(RichText::new(hint).color(p.on_surface_variant));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The colour of the section covering `needle`, the first match in the
    /// job's text.
    fn color_of(job: &egui::text::LayoutJob, needle: &str) -> egui::Color32 {
        let start = job.text.find(needle).expect("needle is in the job text");
        let section = job
            .sections
            .iter()
            .find(|s| s.byte_range.contains(&egui::text::ByteIndex(start)))
            .expect("a section covers the needle");
        assert!(
            section.byte_range.end >= egui::text::ByteIndex(start + needle.len()),
            "{needle}"
        );
        section.format.color
    }

    #[test]
    fn every_variable_form_is_coloured_when_defined() {
        let p = &crate::ui::theme::TOKYO_NIGHT_STORM;
        let job = url_layout_job(
            "$a/${b}/{{c}}/{d}/auth/login",
            p,
            egui::FontId::monospace(12.0),
            &|_: &str| true,
        );
        for needle in ["$a", "${b}", "{{c}}", "{d}"] {
            assert_eq!(color_of(&job, needle), p.syntax_number, "{needle}");
        }
        assert_eq!(color_of(&job, "/auth/login"), p.on_surface);
    }

    #[test]
    fn an_undefined_name_is_drawn_in_the_warning_colour() {
        let p = &crate::ui::theme::TOKYO_NIGHT_STORM;
        let job = url_layout_job(
            "{{baseUrl}}/u/{id}",
            p,
            egui::FontId::monospace(12.0),
            &|name: &str| name == "baseUrl",
        );
        assert_eq!(color_of(&job, "{{baseUrl}}"), p.syntax_number);
        assert_eq!(color_of(&job, "{id}"), p.warning);
        assert_eq!(color_of(&job, "/u/"), p.on_surface);
    }
}
