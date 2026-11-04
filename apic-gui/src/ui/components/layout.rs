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

/// Lays out `url` in `font`, with every `{{...}}` variable span in
/// `syntax_number` and the rest in `on_surface`. A span runs from `{{` to the
/// next `}}` inclusive, and an unclosed `{{` runs to the end.
pub(crate) fn url_layout_job(url: &str, p: &Palette, font: egui::FontId) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let format = |color| egui::TextFormat::simple(font.clone(), color);
    let mut rest = url;
    while !rest.is_empty() {
        let Some(open) = rest.find("{{") else {
            job.append(rest, 0.0, format(p.on_surface));
            break;
        };
        if open > 0 {
            job.append(&rest[..open], 0.0, format(p.on_surface));
        }
        let var = &rest[open..];
        let end = var.find("}}").map_or(var.len(), |close| close + 2);
        job.append(&var[..end], 0.0, format(p.syntax_number));
        rest = &var[end..];
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

    #[test]
    fn url_layout_job_colours_variables_apart_from_the_path() {
        let p = &crate::ui::theme::TOKYO_NIGHT_STORM;
        let job = url_layout_job("{{baseUrl}}/auth/login", p, egui::FontId::monospace(12.0));
        let color_of = |needle: &str| {
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
        };
        assert_eq!(color_of("{{baseUrl}}"), p.syntax_number);
        assert_eq!(color_of("/auth/login"), p.on_surface);
    }
}
