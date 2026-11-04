//! A row of underline tabs with optional counts, shared by every tabbed pane.

use eframe::egui;
use egui::RichText;

use crate::ui::theme::{Palette, palette};

/// One tab in a [`tab_strip`].
pub(crate) struct Tab<'a> {
    /// The tab's text.
    pub(crate) label: &'a str,

    /// A count drawn after the label, hidden when `None` or zero.
    pub(crate) count: Option<usize>,

    /// A colour that overrides the selected and unselected text colours.
    pub(crate) color: Option<egui::Color32>,
}

impl<'a> Tab<'a> {
    /// A plain tab with no count and the default colours.
    pub(crate) fn new(label: &'a str) -> Self {
        Tab {
            label,
            count: None,
            color: None,
        }
    }

    /// This tab with `count` shown after its label.
    pub(crate) fn count(mut self, count: usize) -> Self {
        self.count = Some(count);
        self
    }
}

/// Draws `tabs` in one horizontally scrolling row. The selected tab (index
/// `current`) draws in `on_surface` with a 2px `primary` underline, the rest
/// in `on_surface_variant`, unless a tab sets `color`. Clicking a tab sets
/// `current`.
///
/// # Returns
///
/// `true` when a click changed `current` this frame.
pub(crate) fn tab_strip(ui: &mut egui::Ui, current: &mut usize, tabs: &[Tab]) -> bool {
    let p = palette(ui.ctx());
    let mut changed = false;
    egui::ScrollArea::horizontal()
        .id_salt(ui.next_auto_id())
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for (i, tab) in tabs.iter().enumerate() {
                    let selected = i == *current;
                    let text = tab_text(ui.style(), tab, selected, p);
                    let response = ui.add(egui::Button::new(text).frame(false));
                    if selected {
                        let r = response.rect;
                        ui.painter().line_segment(
                            [
                                egui::pos2(r.left(), r.bottom()),
                                egui::pos2(r.right(), r.bottom()),
                            ],
                            egui::Stroke::new(2.0, p.primary),
                        );
                    }
                    if response.clicked() && !selected {
                        *current = i;
                        changed = true;
                    }
                }
            });
        });
    changed
}

/// The label of `tab` with its count appended when the count is above zero,
/// coloured for the selected or unselected state.
fn tab_text(style: &egui::Style, tab: &Tab, selected: bool, p: &Palette) -> egui::text::LayoutJob {
    let color = tab.color.unwrap_or(if selected {
        p.on_surface
    } else {
        p.on_surface_variant
    });
    let mut text = egui::text::LayoutJob::default();
    RichText::new(tab.label).color(color).append_to(
        &mut text,
        style,
        egui::FontSelection::Default,
        egui::Align::Center,
    );
    if let Some(n) = tab.count.filter(|n| *n > 0) {
        RichText::new(format!("  {n}"))
            .color(p.on_surface_variant)
            .append_to(
                &mut text,
                style,
                egui::FontSelection::Default,
                egui::Align::Center,
            );
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::components::{empty_state, url_bar};
    use crate::ui::theme::{BOTH_THEMES, install_themes, palette_for, run_in_theme};

    #[test]
    fn tab_strip_click_selects_and_reports_change() {
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let mut current = 0;
        let mut changes = Vec::new();
        let mut origin = egui::Pos2::ZERO;
        let run = |events: Vec<egui::Event>, current: &mut usize, origin: &mut egui::Pos2| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            let mut changed = false;
            let mut out = ctx.run_ui(input, |ui| {
                install_themes(ui.ctx(), egui::ThemePreference::Dark);
                *origin = ui.max_rect().min;
                changed = tab_strip(
                    ui,
                    current,
                    &[Tab::new("A"), Tab::new("A much longer second tab label")],
                );
            });
            out.textures_delta.clear();
            changed
        };
        changes.push(run(Vec::new(), &mut current, &mut origin));
        // The first tab is a single letter, so 100px in lands on the second.
        let pos = origin + egui::vec2(100.0, 8.0);
        let press = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        changes.push(run(
            vec![egui::Event::PointerMoved(pos), press(true)],
            &mut current,
            &mut origin,
        ));
        changes.push(run(vec![press(false)], &mut current, &mut origin));
        assert_eq!(current, 1, "clicking the second tab must select it");
        assert_eq!(
            changes,
            [false, false, true],
            "only the release reports a change"
        );
        assert!(
            !run(Vec::new(), &mut current, &mut origin),
            "an idle frame reports no change"
        );
    }

    #[test]
    fn tab_strip_hides_a_zero_count() {
        let style = egui::Style::default();
        let p = palette_for(egui::Theme::Dark);
        let zero = tab_text(&style, &Tab::new("Headers").count(0), false, p);
        assert!(
            !zero.text.chars().any(|c| c.is_ascii_digit()),
            "{}",
            zero.text
        );
        let three = tab_text(&style, &Tab::new("Headers").count(3), true, p);
        assert!(three.text.contains('3'), "{}", three.text);
    }

    #[test]
    fn tab_strip_url_bar_and_empty_state_render_in_both_themes() {
        for preference in BOTH_THEMES {
            run_in_theme(preference, |ui| {
                let mut current = 1;
                let mut colored = Tab::new("Errors").count(2);
                colored.color = Some(egui::Color32::RED);
                tab_strip(ui, &mut current, &[Tab::new("Body"), colored]);
                url_bar(ui, |ui| ui.label("GET https://example.com"));
                empty_state(ui, "Nothing here", "Pick something.");
                empty_state(ui, "Nothing here", "");
            });
        }
    }
}
