//! The git central pane's merge conflict resolver, shaped like VS Code's
//! inline merge view: per-block Accept links, the chosen text in place once
//! decided, and the Resolve action.

use eframe::egui;
use egui::RichText;

use super::diff::inline_diff_view;
use crate::features::git::domain::conflict::{self, Choice, Segment};
use crate::features::git::domain::diff::parse_unified;
use crate::features::git::ui::state::{GitAction, GitState, ResolveState};
use crate::ui::components::text_button;
use crate::ui::theme::*;

/// Ensures `state.resolve` holds the parsed conflict for `path`, parsing
/// `new_blob` (the working file, markers and all) the first time this path is
/// seen, then renders either the resolver or, when the text did not parse,
/// the same read-only view a conflict showed before this panel existed.
pub(super) fn conflict_resolver(
    ui: &mut egui::Ui,
    state: &mut GitState,
    path: &str,
    staged: bool,
    raw: &str,
    new_blob: Option<&str>,
) -> Option<GitAction> {
    let p = palette(ui.ctx());
    let matches_path = state.resolve.as_ref().is_some_and(|r| r.path == path);
    if !matches_path {
        state.resolve = new_blob.and_then(|text| ResolveState::parse(path, text));
    }

    let Some(resolve) = state.resolve.as_mut() else {
        ui.horizontal(|ui| {
            ui.label(RichText::new(path).color(p.on_surface).strong());
            let side = if staged { "staged" } else { "unstaged" };
            ui.label(RichText::new(side).color(p.on_surface_variant));
        });
        ui.separator();
        inline_diff_view(ui, &parse_unified(raw));
        ui.add_space(SPACE_SMALL);
        ui.label(
            RichText::new(
                "This file's conflict markers could not be read. Resolve it in your editor.",
            )
            .color(p.warning),
        );
        return None;
    };
    resolve_view(ui, path, staged, resolve)
}

/// The conflict resolver: take-all shortcuts and Resolve in the header, then
/// one full-width scroll of the file in order, its text between conflicts
/// and each conflict block either undecided or showing its chosen text.
///
/// Resolve writes `conflict::render` with the current choices, and a
/// decided block shows exactly the text `render` puts there, so what is on
/// screen is what reaches disk.
///
/// The whole-file actions live in this header row rather than a pinned bar
/// below the panes: the header is laid out first, at the top of the body, so
/// nothing rendered below it can push it off screen. The three buttons are
/// reserved first, in a right-to-left layout, so a long path can never push
/// them off the right edge, matching a real defect earlier in this cycle
/// where Resolve rendered off screen and could not be clicked.
///
/// What remains is given to a left-aligned group holding the path then the
/// side label, matching the non-conflicted header in `central_body`. The
/// side label's width is measured first with `layout_no_wrap` (the same
/// idiom `features::contracts::ui::view` uses to size a field to its text) so
/// the path's truncate width can be capped to leave room for it. Without
/// that cap, a `.truncate()` label sizes itself against all remaining
/// width, which is why the side label was pushed next to the buttons in an
/// earlier pass of this same header.
fn resolve_view(
    ui: &mut egui::Ui,
    path: &str,
    staged: bool,
    resolve: &mut ResolveState,
) -> Option<GitAction> {
    let p = palette(ui.ctx());
    let mut action = None;

    // Rendered fresh every frame from the current choices, by the exact
    // function Resolve calls below, so the right pane is always honest about
    // what pressing Resolve would write, including for still-undecided
    // blocks, which render as their original marker text.
    let rendered = conflict::render(&resolve.file, &resolve.choices);
    let all_chosen = resolve.choices.iter().all(Option::is_some);

    ui.horizontal(|ui| {
        // Right-aligned, and inserted in right_to_left order so Resolve
        // lands rightmost and last: it is the only one of the three that
        // writes to disk, so a stray click one button off lands on a
        // take-all, which only fills in a choice, rather than on Resolve.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_enabled_ui(all_chosen, |ui| {
                if text_button(ui, &resolve_label(&resolve.choices), p.primary) {
                    action = Some(GitAction::ResolveConflict {
                        path: path.to_string(),
                        text: rendered.clone(),
                    });
                }
            });
            if text_button(ui, "Take all theirs", p.primary) {
                resolve.choices.fill(Some(Choice::Theirs));
            }
            if text_button(ui, "Take all ours", p.primary) {
                resolve.choices.fill(Some(Choice::Ours));
            }

            let side = if staged { "staged" } else { "unstaged" };
            let font = egui::TextStyle::Body.resolve(ui.style());
            let side_width = ui
                .painter()
                .layout_no_wrap(side.to_string(), font, egui::Color32::PLACEHOLDER)
                .size()
                .x;
            let gap = ui.spacing().item_spacing.x;
            let path_max_width = (ui.available_width() - side_width - gap).max(0.0);

            let leftover = egui::vec2(ui.available_width(), ui.available_height());
            ui.allocate_ui_with_layout(
                leftover,
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.scope(|ui| {
                        ui.set_max_width(path_max_width);
                        ui.add(
                            egui::Label::new(RichText::new(path).color(p.on_surface).strong())
                                .truncate(),
                        );
                    });
                    ui.label(RichText::new(side).color(p.on_surface_variant));
                },
            );
        });
    });
    ui.separator();

    let json = path.ends_with(".json");
    if json && all_chosen && apic_core::json::validate(&rendered).is_err() {
        ui.label(
            RichText::new(
                "This resolution is not valid JSON. You can still resolve it and fix it in the repair editor.",
            )
            .color(p.warning),
        );
        ui.add_space(SPACE_EXTRA_SMALL);
    }

    egui::ScrollArea::vertical()
        .id_salt("resolve_inline_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let mut block = 0;
            for segment in &resolve.file.segments {
                match segment {
                    Segment::Text(text) => code_text(ui, text, json),
                    Segment::Conflict {
                        ours_label,
                        theirs_label,
                        ours,
                        theirs,
                    } => {
                        let sides = Sides {
                            ours_label,
                            theirs_label,
                            ours,
                            theirs,
                        };
                        conflict_block_view(ui, &sides, json, &mut resolve.choices[block]);
                        block += 1;
                    }
                }
            }
        });

    action
}

/// The Resolve button's label: how many of the file's blocks are decided
/// while any remain undecided, so a disabled button explains itself instead
/// of leaving the user to count blocks by eye, or plain "Resolve" once every
/// block has a choice.
pub(super) fn resolve_label(choices: &[Option<Choice>]) -> String {
    let decided = choices.iter().filter(|c| c.is_some()).count();
    if decided == choices.len() {
        "Resolve".to_string()
    } else {
        format!("Resolve ({decided}/{})", choices.len())
    }
}

/// The two labelled sides of one conflict block, borrowed from its
/// `Segment::Conflict`.
struct Sides<'a> {
    ours_label: &'a str,
    theirs_label: &'a str,
    ours: &'a str,
    theirs: &'a str,
}

/// One conflict block in place. Undecided: a row of Accept links, then the
/// Current side under its header band in the success tint and the Incoming
/// side under its header band in the info tint. Decided: what was accepted
/// with an Undo link, then the chosen text, ours then theirs for Both, the
/// same text `conflict::render` writes, on a faint primary tint with a
/// left bar.
fn conflict_block_view(ui: &mut egui::Ui, sides: &Sides, json: bool, choice: &mut Option<Choice>) {
    let p = palette(ui.ctx());
    ui.add_space(SPACE_EXTRA_SMALL);
    match *choice {
        None => {
            ui.horizontal(|ui| {
                if lens(ui, "Accept Current") {
                    *choice = Some(Choice::Ours);
                }
                ui.label(RichText::new("|").color(p.on_surface_variant));
                if lens(ui, "Accept Incoming") {
                    *choice = Some(Choice::Theirs);
                }
                ui.label(RichText::new("|").color(p.on_surface_variant));
                if lens(ui, "Accept Both") {
                    *choice = Some(Choice::Both);
                }
            });
            band(
                ui,
                &format!("{} (Current Change)", sides.ours_label),
                p.success.gamma_multiply(0.35),
            );
            tinted(ui, sides.ours, json, p.success.gamma_multiply(0.15));
            band(
                ui,
                &format!("{} (Incoming Change)", sides.theirs_label),
                p.info.gamma_multiply(0.35),
            );
            tinted(ui, sides.theirs, json, p.info.gamma_multiply(0.15));
        }
        Some(chosen) => {
            ui.horizontal(|ui| {
                ui.label(RichText::new(accepted_label(chosen)).color(p.on_surface_variant));
                if lens(ui, "Undo") {
                    *choice = None;
                }
            });
            let text = match chosen {
                Choice::Ours => sides.ours.to_string(),
                Choice::Theirs => sides.theirs.to_string(),
                Choice::Both => format!("{}{}", sides.ours, sides.theirs),
            };
            let rect = tinted(ui, &text, json, p.primary.gamma_multiply(0.10)).rect;
            let bar = egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height()));
            ui.painter().rect_filled(bar, 0.0, p.primary);
        }
    }
    ui.add_space(SPACE_EXTRA_SMALL);
}

/// What a decided block's link row says was accepted.
fn accepted_label(choice: Choice) -> &'static str {
    match choice {
        Choice::Ours => "Accepted Current",
        Choice::Theirs => "Accepted Incoming",
        Choice::Both => "Accepted Both",
    }
}

/// A small borderless clickable text in the primary colour, like VS Code's
/// CodeLens links above a conflict.
fn lens(ui: &mut egui::Ui, label: &str) -> bool {
    let p = palette(ui.ctx());
    ui.add(egui::Label::new(RichText::new(label).color(p.primary)).sense(egui::Sense::click()))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

/// A full-width header band naming one side of a conflict.
fn band(ui: &mut egui::Ui, title: &str, fill: egui::Color32) {
    let p = palette(ui.ctx());
    egui::Frame::new()
        .fill(fill)
        .inner_margin(SPACE_EXTRA_SMALL)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new(title).color(p.on_surface).small());
        });
}

/// `text` as code on a full-width `fill` band.
fn tinted(ui: &mut egui::Ui, text: &str, json: bool, fill: egui::Color32) -> egui::Response {
    egui::Frame::new()
        .fill(fill)
        .inner_margin(SPACE_EXTRA_SMALL)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            code_text(ui, text, json);
        })
        .response
}

/// Text from the conflicted file, monospace, JSON-highlighted for a `.json`
/// path. One trailing newline is dropped so a segment does not end in an
/// extra blank row, and a segment that is only a newline still renders as
/// its one blank line.
fn code_text(ui: &mut egui::Ui, text: &str, json: bool) {
    if text.is_empty() {
        return;
    }
    let text = text.strip_suffix('\n').unwrap_or(text);
    let p = palette(ui.ctx());
    if json {
        let font_id = egui::TextStyle::Monospace.resolve(ui.style());
        let job = crate::ui::syntax_highlighting::highlight_json(text, font_id, p);
        ui.label(job);
    } else {
        ui.label(RichText::new(text).color(p.on_surface).monospace());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepted_label_names_current_for_ours() {
        assert_eq!(accepted_label(Choice::Ours), "Accepted Current");
    }

    #[test]
    fn accepted_label_names_incoming_for_theirs() {
        assert_eq!(accepted_label(Choice::Theirs), "Accepted Incoming");
    }

    #[test]
    fn accepted_label_names_both() {
        assert_eq!(accepted_label(Choice::Both), "Accepted Both");
    }

    #[test]
    fn resolve_label_shows_progress_while_blocks_remain_undecided() {
        assert_eq!(resolve_label(&[None, None, None]), "Resolve (0/3)");
        assert_eq!(
            resolve_label(&[Some(Choice::Ours), None, Some(Choice::Theirs)]),
            "Resolve (2/3)"
        );
    }

    #[test]
    fn resolve_label_reads_plain_resolve_once_every_block_is_decided() {
        assert_eq!(
            resolve_label(&[Some(Choice::Ours), Some(Choice::Theirs)]),
            "Resolve"
        );
    }
}
