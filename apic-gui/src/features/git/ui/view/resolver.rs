//! The git central pane's merge conflict resolver: per-block choices, the live
//! preview of what Resolve writes, and the Resolve action.

use eframe::egui;
use egui::RichText;

use super::diff::raw_diff_view;
use crate::features::git::domain::conflict::{self, Choice, Segment};
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
        raw_diff_view(ui, raw);
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

/// The conflict resolver: take-all shortcuts, a 50:50 split with the picker
/// on the left (every block with its two named sides, the unconflicted text
/// between them so the file still reads in order) and a live preview of what
/// Resolve would write on the right, then Resolve.
///
/// The preview is `conflict::render` called with the current choices, the
/// same function Resolve calls with the same choices, so nothing shown here
/// can drift from what actually reaches disk. Split shape matches
/// `features::contracts::ui::view`: `horizontal_top` with `allocate_ui_with_layout`
/// and a reserved divider width, rather than a second way to split a panel.
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

    ui.horizontal_top(|ui| {
        let gap = ui.spacing().item_spacing.x;
        let divider_w = 6.0;
        let col_w = ((ui.available_width() - divider_w - gap * 2.0) / 2.0).max(0.0);
        let col_h = ui.available_height();

        // Left pane: the picker, one block at a time.
        ui.allocate_ui_with_layout(
            egui::vec2(col_w, col_h),
            egui::Layout::top_down(egui::Align::Min),
            |left| {
                egui::ScrollArea::vertical()
                    .id_salt("resolve_picker_scroll")
                    .auto_shrink([false, false])
                    .show(left, |ui| {
                        let mut block = 0;
                        for segment in &resolve.file.segments {
                            match segment {
                                Segment::Text(text) => {
                                    if !text.trim().is_empty() {
                                        ui.label(RichText::new(text).color(p.on_surface_variant).monospace());
                                    }
                                }
                                Segment::Conflict {
                                    ours_label,
                                    theirs_label,
                                    ours,
                                    theirs,
                                } => {
                                    conflict_block_view(
                                        ui,
                                        ours_label,
                                        theirs_label,
                                        ours,
                                        theirs,
                                        &mut resolve.choices[block],
                                    );
                                    block += 1;
                                }
                            }
                        }
                    });
            },
        );

        ui.separator();

        // Right pane: the live preview of what Resolve would write.
        ui.allocate_ui_with_layout(
            egui::vec2(col_w, col_h),
            egui::Layout::top_down(egui::Align::Min),
            |right| {
                right.label(RichText::new("PREVIEW").color(p.on_surface_variant).size(11.0));
                egui::ScrollArea::vertical()
                    .id_salt("resolve_preview_scroll")
                    .auto_shrink([false, false])
                    .show(right, |ui| {
                        if path.ends_with(".json") {
                            // A partially decided preview still carries
                            // conflict markers and is not valid JSON;
                            // `highlight_json` handles non-JSON text without
                            // panicking, so this stays safe for every state.
                            let font_id = egui::TextStyle::Monospace.resolve(ui.style());
                            let job = crate::ui::syntax_highlighting::highlight_json(
                                &rendered,
                                font_id,
                                crate::ui::theme::palette(ui.ctx()),
                            );
                            ui.label(job);
                        } else {
                            ui.label(RichText::new(&rendered).monospace());
                        }
                    });
                if path.ends_with(".json") && apic_core::json::validate(&rendered).is_err() {
                    right.add_space(SPACE_EXTRA_SMALL);
                    right.label(
                        RichText::new(
                            "This resolution is not valid JSON. You can still resolve it and fix it in the repair editor.",
                        )
                        .color(p.warning),
                    );
                }
            },
        );
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

/// One conflict block, shaped like VS Code's inline merge view: a row of
/// named actions, a line stating what the current choice will write, then
/// the block stacked in file order with the real `<<<<<<<`, `=======` and
/// `>>>>>>>` marker lines still visible so the pane reads like the file on
/// disk. Ours is tinted green, theirs is tinted cyan.
fn conflict_block_view(
    ui: &mut egui::Ui,
    ours_label: &str,
    theirs_label: &str,
    ours: &str,
    theirs: &str,
    choice: &mut Option<Choice>,
) {
    let p = palette(ui.ctx());
    ui.group(|ui| {
        ui.horizontal(|ui| {
            if text_button(ui, "Accept ours", p.primary) {
                *choice = Some(Choice::Ours);
            }
            if text_button(ui, "Accept theirs", p.info) {
                *choice = Some(Choice::Theirs);
            }
            if text_button(ui, "Accept both", p.on_surface_variant) {
                *choice = Some(Choice::Both);
            }
        });
        ui.label(
            RichText::new(current_choice_summary(*choice, ours_label, theirs_label))
                .color(p.on_surface_variant),
        );
        ui.add_space(SPACE_EXTRA_SMALL);

        conflict_side_view(
            ui,
            p.success,
            &format!("<<<<<<< {ours_label}"),
            ours,
            MarkerPosition::Before,
        );
        ui.label(
            RichText::new("=======")
                .color(p.on_surface_variant)
                .monospace(),
        );
        conflict_side_view(
            ui,
            p.info,
            &format!(">>>>>>> {theirs_label}"),
            theirs,
            MarkerPosition::After,
        );
    });
    ui.add_space(SPACE_SMALL);
}

/// States what Resolve will write for this block, so a decided block looks
/// decided instead of leaving the choice only visible in the button state.
fn current_choice_summary(choice: Option<Choice>, ours_label: &str, theirs_label: &str) -> String {
    match choice {
        Some(Choice::Ours) => format!("Resolve will write {ours_label}"),
        Some(Choice::Theirs) => format!("Resolve will write {theirs_label}"),
        Some(Choice::Both) => format!("Resolve will write both {ours_label} and {theirs_label}"),
        None => "Not decided yet".to_string(),
    }
}

/// Where the marker line sits relative to a side's content, matching where
/// git placed `<<<<<<<` (before ours) and `>>>>>>>` (after theirs).
enum MarkerPosition {
    Before,
    After,
}

/// One tinted side of a conflict block: the marker line plus the side's text,
/// painted in `color` over a background derived from the same token via
/// `gamma_multiply` rather than a new colour, so the panel keeps one
/// palette.
fn conflict_side_view(
    ui: &mut egui::Ui,
    color: egui::Color32,
    marker: &str,
    text: &str,
    marker_position: MarkerPosition,
) {
    let background = color.gamma_multiply(0.15);
    egui::Frame::new()
        .fill(background)
        .inner_margin(SPACE_EXTRA_SMALL)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                if matches!(marker_position, MarkerPosition::Before) {
                    ui.label(RichText::new(marker).color(color).monospace());
                }
                for line in text.lines() {
                    ui.label(RichText::new(line).color(color).monospace());
                }
                if matches!(marker_position, MarkerPosition::After) {
                    ui.label(RichText::new(marker).color(color).monospace());
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_choice_summary_names_the_undecided_state() {
        assert_eq!(
            current_choice_summary(None, "HEAD", "feature"),
            "Not decided yet"
        );
    }

    #[test]
    fn current_choice_summary_names_what_resolve_will_write_for_ours() {
        assert_eq!(
            current_choice_summary(Some(Choice::Ours), "HEAD", "feature"),
            "Resolve will write HEAD"
        );
    }

    #[test]
    fn current_choice_summary_names_what_resolve_will_write_for_theirs() {
        assert_eq!(
            current_choice_summary(Some(Choice::Theirs), "HEAD", "feature"),
            "Resolve will write feature"
        );
    }

    #[test]
    fn current_choice_summary_names_both_sides_when_both_are_taken() {
        assert_eq!(
            current_choice_summary(Some(Choice::Both), "HEAD", "feature"),
            "Resolve will write both HEAD and feature"
        );
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
