//! The git log panel: the filter toolbar, the commit table with its painted
//! graph, the selected commit's changed files, and the revert confirmation.

use eframe::egui;
use egui::RichText;

use super::files::{FileTree, RowMode};
use crate::features::git::domain::log::{Commit, LogFilter, PathFilter, Row, Since, short};
use crate::features::git::domain::status::FileStatus;
use crate::features::git::ui::state::{GitAction, GitState};
use crate::ui::components::{
    ButtonKind, DialogButton, detail_box, dialog, empty_state, icon_button,
};
use crate::ui::theme::*;

/// Height of one commit row.
const ROW_H: f32 = 22.0;

/// Width of one graph lane.
const LANE_W: f32 = 14.0;

/// Width of the author column.
const AUTHOR_W: f32 = 140.0;

/// Width of the date column.
const DATE_W: f32 = 120.0;

/// Seconds the search box waits after the last keystroke before rereading.
const DEBOUNCE_S: f64 = 0.3;

/// The trimmed `draft` to apply, once `DEBOUNCE_S` has passed since the
/// edit at `edited_at` and it differs from the `applied` text.
fn debounced(draft: &str, applied: &str, edited_at: f64, now: f64) -> Option<String> {
    let draft = draft.trim();
    (draft != applied && now - edited_at >= DEBOUNCE_S).then(|| draft.to_string())
}

/// `filter` with `change` applied, as the action that rereads the log.
fn set(filter: &LogFilter, change: impl FnOnce(&mut LogFilter)) -> GitAction {
    let mut filter = filter.clone();
    change(&mut filter);
    GitAction::SetLogFilter(filter)
}

/// The colour of graph lane `lane`.
fn lane_color(p: &Palette, lane: usize) -> egui::Color32 {
    [
        p.primary,
        p.success,
        p.warning,
        p.info,
        p.syntax_key,
        p.error,
    ][lane % 6]
}

/// The log panel body: the toolbar on top, the selected commit's files on
/// the right, the commit table in the rest.
pub(crate) fn log_panel(ui: &mut egui::Ui, state: &mut GitState) -> Option<GitAction> {
    let mut action = None;
    egui::Panel::top("git_log_toolbar").show(ui, |ui| {
        if let Some(a) = toolbar(ui, state) {
            action = Some(a);
        }
    });
    egui::Panel::right("git_log_files")
        .resizable(true)
        .default_size(260.0)
        .min_size(160.0)
        .show(ui, |ui| {
            if let Some(a) = commit_files(ui, state) {
                action = Some(a);
            }
        });
    if let Some(a) = commit_table(ui, state) {
        action = Some(a);
    }
    action
}

/// The filter row: search box with `.*` and `Cc`, Branch, User, Date and
/// Paths menus, and Refresh. Then git's error, when the last read failed.
fn toolbar(ui: &mut egui::Ui, state: &mut GitState) -> Option<GitAction> {
    let p = palette(ui.ctx());
    let mut action = None;
    let filter = state.log.filter.clone();
    ui.add_space(SPACE_EXTRA_SMALL);
    ui.horizontal(|ui| {
        let edit = ui.add(
            egui::TextEdit::singleline(&mut state.log.draft)
                .hint_text("Text or hash")
                .desired_width(180.0),
        );
        let now = ui.input(|i| i.time);
        if edit.changed() {
            state.log.edited_at = now;
        }
        match debounced(&state.log.draft, &filter.text, state.log.edited_at, now) {
            Some(text) => action = Some(set(&filter, |f| f.text = text)),
            None if state.log.draft.trim() != filter.text => {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(300));
            }
            None => {}
        }
        if ui
            .selectable_label(filter.regex, ".*")
            .on_hover_text("Regular expression")
            .clicked()
        {
            action = Some(set(&filter, |f| f.regex = !f.regex));
        }
        if ui
            .selectable_label(filter.match_case, "Cc")
            .on_hover_text("Match case")
            .clicked()
        {
            action = Some(set(&filter, |f| f.match_case = !f.match_case));
        }

        let current = state.branches.current.clone();
        let shown = filter
            .branch
            .clone()
            .or(current.clone())
            .unwrap_or_else(|| "HEAD".to_string());
        ui.menu_button(format!("Branch: {shown}"), |ui| {
            for name in state
                .branches
                .all
                .iter()
                .chain(state.branches.remotes.iter())
            {
                if ui.selectable_label(*name == shown, name.as_str()).clicked() {
                    let pick = (Some(name) != current.as_ref()).then(|| name.clone());
                    action = Some(set(&filter, |f| f.branch = pick));
                    ui.close();
                }
            }
        });

        let mut authors: Vec<&str> = state
            .log
            .commits
            .iter()
            .map(|c| c.author.as_str())
            .collect();
        authors.sort_unstable();
        authors.dedup();
        ui.menu_button(
            format!("User: {}", filter.author.as_deref().unwrap_or("All")),
            |ui| {
                if ui
                    .selectable_label(filter.author.is_none(), "All")
                    .clicked()
                {
                    action = Some(set(&filter, |f| f.author = None));
                    ui.close();
                }
                for author in &authors {
                    if ui
                        .selectable_label(filter.author.as_deref() == Some(*author), *author)
                        .clicked()
                    {
                        action = Some(set(&filter, |f| f.author = Some(author.to_string())));
                        ui.close();
                    }
                }
            },
        );

        ui.menu_button(format!("Date: {}", filter.since.label()), |ui| {
            for since in Since::ALL {
                if ui
                    .selectable_label(filter.since == since, since.label())
                    .clicked()
                {
                    action = Some(set(&filter, |f| f.since = since));
                    ui.close();
                }
            }
        });

        let path_label = match &filter.path {
            PathFilter::All => "All".to_string(),
            PathFilter::Contracts => "Contracts folder".to_string(),
            PathFilter::Typed(path) => path.clone(),
        };
        ui.menu_button(format!("Paths: {path_label}"), |ui| {
            if ui
                .selectable_label(filter.path == PathFilter::All, "All")
                .clicked()
            {
                action = Some(set(&filter, |f| f.path = PathFilter::All));
                ui.close();
            }
            if ui
                .selectable_label(filter.path == PathFilter::Contracts, "Contracts folder")
                .clicked()
            {
                action = Some(set(&filter, |f| f.path = PathFilter::Contracts));
                ui.close();
            }
            ui.horizontal(|ui| {
                let typed = ui.add(
                    egui::TextEdit::singleline(&mut state.log.typed_path)
                        .hint_text("path")
                        .desired_width(160.0),
                );
                let enter = typed.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let path = state.log.typed_path.trim().to_string();
                if (ui.button("Use").clicked() || enter) && !path.is_empty() {
                    action = Some(set(&filter, |f| f.path = PathFilter::Typed(path)));
                    ui.close();
                }
            });
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icon_button(ui, egui_phosphor::regular::ARROWS_CLOCKWISE, "Refresh log").clicked() {
                action = Some(GitAction::RefreshLog);
            }
        });
    });
    if !state.log.error.is_empty() {
        ui.label(RichText::new(&state.log.error).color(p.error));
    }
    ui.add_space(SPACE_EXTRA_SMALL);
    action
}

/// The commit table, virtualised with `show_rows`, with a Load more row at
/// the end while the last page was full.
fn commit_table(ui: &mut egui::Ui, state: &GitState) -> Option<GitAction> {
    let log = &state.log;
    if log.commits.is_empty() {
        let title = if log.loaded {
            "No commits match these filters"
        } else {
            "Loading log..."
        };
        empty_state(ui, title, "");
        return None;
    }
    let dots_only = log.filter.dots_only();
    let lanes = if dots_only {
        1
    } else {
        log.rows.iter().map(Row::width).max().unwrap_or(1)
    };
    let graph_w = lanes as f32 * LANE_W;
    let total = log.commits.len() + usize::from(log.more);
    let mut action = None;
    ui.scope(|ui| {
        // No gap between rows, so the graph's lines meet.
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, ROW_H, total, |ui, range| {
                for i in range {
                    let row = match (log.commits.get(i), log.rows.get(i)) {
                        (Some(commit), Some(row)) => {
                            commit_row(ui, state, commit, row, graph_w, dots_only)
                        }
                        _ => ui
                            .add_sized(
                                [ui.available_width(), ROW_H],
                                egui::Button::new("Load more"),
                            )
                            .clicked()
                            .then_some(GitAction::LoadMoreLog),
                    };
                    if let Some(a) = row {
                        action = Some(a);
                    }
                }
            });
    });
    action
}

/// One commit: graph, ref pills, subject, author and date, painted into one
/// clickable rect. A click lists its files, a right-click offers Revert and
/// Copy hash. A merge commit's text is dimmed.
fn commit_row(
    ui: &mut egui::Ui,
    state: &GitState,
    commit: &Commit,
    row: &Row,
    graph_w: f32,
    dots_only: bool,
) -> Option<GitAction> {
    let p = palette(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ROW_H),
        egui::Sense::click(),
    );
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &commit.subject));
    let painter = ui.painter_at(rect);
    if state.log.selected.as_deref() == Some(commit.hash.as_str()) || response.hovered() {
        painter.rect_filled(rect, 0.0, p.surface_container);
    }
    paint_graph(&painter, rect, row, dots_only, p);

    let font = egui::TextStyle::Body.resolve(ui.style());
    let mid = rect.center().y;
    let date_x = rect.right() - DATE_W;
    let author_x = date_x - AUTHOR_W;
    painter.text(
        egui::pos2(date_x, mid),
        egui::Align2::LEFT_CENTER,
        &commit.date,
        font.clone(),
        p.on_surface_variant,
    );
    painter
        .with_clip_rect(egui::Rect::from_x_y_ranges(
            author_x..=date_x - SPACE_SMALL,
            rect.y_range(),
        ))
        .text(
            egui::pos2(author_x, mid),
            egui::Align2::LEFT_CENTER,
            &commit.author,
            font.clone(),
            p.on_surface_variant,
        );
    let subject = painter.with_clip_rect(egui::Rect::from_x_y_ranges(
        rect.left()..=author_x - SPACE_SMALL,
        rect.y_range(),
    ));
    let mut x = rect.left() + graph_w + SPACE_SMALL;
    for name in &commit.refs {
        let galley = subject.layout_no_wrap(name.clone(), font.clone(), p.on_surface);
        let pill = egui::Rect::from_min_size(
            egui::pos2(x, mid - galley.size().y / 2.0 - 1.0),
            galley.size() + egui::vec2(8.0, 2.0),
        );
        subject.rect_filled(pill, 4.0, p.secondary_container);
        subject.galley(pill.min + egui::vec2(4.0, 1.0), galley, p.on_surface);
        x = pill.right() + SPACE_EXTRA_SMALL;
    }
    let text = if commit.is_merge() {
        p.on_surface_variant
    } else {
        p.on_surface
    };
    subject.text(
        egui::pos2(x, mid),
        egui::Align2::LEFT_CENTER,
        &commit.subject,
        font,
        text,
    );

    let mut action = response.clicked().then(|| GitAction::SelectCommit {
        hash: commit.hash.clone(),
    });
    response.context_menu(|ui| {
        let block = state.revert_block(commit);
        let revert = ui.add_enabled(block.is_none(), egui::Button::new("Revert"));
        let revert = match block {
            Some(reason) => revert.on_disabled_hover_text(reason),
            None => revert,
        };
        if revert.clicked() {
            action = Some(GitAction::RequestRevert {
                hash: commit.hash.clone(),
            });
            ui.close();
        }
        if ui.button("Copy hash").clicked() {
            ui.ctx().copy_text(commit.hash.clone());
            ui.close();
        }
    });
    action
}

/// Paints one row of the graph: the lanes passing through, the lines from
/// children into the dot and from the dot to its parents, then the dot. With
/// `dots_only`, a dot in the first lane and nothing else.
fn paint_graph(painter: &egui::Painter, rect: egui::Rect, row: &Row, dots_only: bool, p: &Palette) {
    let x = |lane: usize| rect.left() + LANE_W * (lane as f32 + 0.5);
    let (top, mid, bottom) = (rect.top(), rect.center().y, rect.bottom());
    let stroke = |lane: usize| egui::Stroke::new(1.5, lane_color(p, lane));
    if dots_only {
        painter.circle_filled(egui::pos2(x(0), mid), 3.5, lane_color(p, 0));
        return;
    }
    for &lane in &row.through {
        painter.line_segment(
            [egui::pos2(x(lane), top), egui::pos2(x(lane), bottom)],
            stroke(lane),
        );
    }
    for &lane in &row.incoming {
        painter.line_segment(
            [egui::pos2(x(lane), top), egui::pos2(x(row.lane), mid)],
            stroke(lane),
        );
    }
    for &lane in &row.outgoing {
        painter.line_segment(
            [egui::pos2(x(row.lane), mid), egui::pos2(x(lane), bottom)],
            stroke(lane),
        );
    }
    painter.circle_filled(egui::pos2(x(row.lane), mid), 3.5, lane_color(p, row.lane));
}

/// The selected commit's changed files, nested by folder with the Changes
/// file tree. A click shows that commit's diff of the file.
fn commit_files(ui: &mut egui::Ui, state: &GitState) -> Option<GitAction> {
    let p = palette(ui.ctx());
    let Some(hash) = &state.log.selected else {
        ui.label(RichText::new("Select a commit to see its files.").color(p.on_surface_variant));
        return None;
    };
    let Some(files) = &state.log.files else {
        ui.label(RichText::new("Loading files...").color(p.on_surface_variant));
        return None;
    };
    ui.label(
        RichText::new(short(hash))
            .color(p.on_surface_variant)
            .monospace(),
    );
    let refs: Vec<&FileStatus> = files.iter().collect();
    let tree = FileTree::from_files(&refs, |_| false);
    let selected = state.log.selected_file.clone().map(|path| (path, true));
    let mut action = None;
    egui::ScrollArea::vertical()
        .id_salt("git_log_files_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            tree.show(ui, "", RowMode::Commit, selected.as_ref(), &mut action);
        });
    action
}

/// The revert confirmation, shown while `pending_revert` is set: the short
/// hash and subject, Cancel and Revert.
pub(super) fn revert_dialog(ctx: &egui::Context, state: &mut GitState) -> Option<GitAction> {
    let p = palette(ctx);
    let commit = state.log.pending_revert.clone()?;
    let outcome = dialog(
        ctx,
        "revert_modal",
        "Revert commit",
        |ui| {
            ui.label(RichText::new("Add a new commit that undoes").color(p.on_surface_variant));
            ui.add_space(SPACE_SMALL);
            detail_box(ui, |ui| {
                ui.label(
                    RichText::new(commit.short())
                        .color(p.on_surface_variant)
                        .monospace(),
                );
                ui.label(RichText::new(&commit.subject).color(p.on_surface).strong());
            });
        },
        &[
            DialogButton::new("Cancel", ButtonKind::Secondary),
            DialogButton::new("Revert", ButtonKind::Danger),
        ],
    );
    if outcome.clicked == Some(1) {
        Some(GitAction::ConfirmRevert)
    } else if outcome.clicked == Some(0) || outcome.closed {
        state.log.pending_revert = None;
        None
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_view_debounce_waits_for_a_pause() {
        assert_eq!(debounced("fix", "", 0.0, 0.1), None);
        assert_eq!(debounced("fix", "", 0.0, 0.3), Some("fix".to_string()));
        assert_eq!(debounced(" fix ", "fix", 0.0, 1.0), None);
        assert_eq!(debounced("", "fix", 0.0, 1.0), Some(String::new()));
    }
}
