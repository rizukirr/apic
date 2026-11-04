//! The central body for the open contract: the viewer/editor, the repair
//! editor for an invalid file, or the empty-state text.

use eframe::egui;
use egui::RichText;

use super::editor::{
    endpoint_description, endpoint_name, headers, method_url_row, query_section, request_body,
    response_body, response_code_selector, response_headers,
};
use crate::features::contracts::ui::state::{ContractsAction, ContractsState, MainTab, RespTab};
use crate::ui::components::text_button;
use crate::ui::theme::*;

/// The central body for the loaded contract: the viewer/editor, the repair
/// editor for an invalid file, or the empty-state text.
///
/// The panel frame is owned by the app shell. Saving, toggling edit mode and
/// promoting a repaired file are returned as an action rather than done here.
///
/// # Arguments
///
/// * `ui` - the central panel to fill
/// * `state` - the contracts view model
/// * `has_project` - whether a project is open, otherwise the empty state shows
///
/// # Returns
///
/// The action a click or a now-valid repair buffer asks for this frame.
pub(crate) fn central_body(
    ui: &mut egui::Ui,
    state: &mut ContractsState,
    has_project: bool,
) -> Option<ContractsAction> {
    let p = palette(ui.ctx());
    let no_project = !has_project;
    let mut action = None;
    let ContractsState {
        model,
        editing,
        resp_tab,
        main_tab,
        resp_tab_view,
        repair,
        entries,
        ..
    } = state;
    if no_project {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("No project open")
                    .color(p.on_surface_variant)
                    .size(16.0),
            );
            ui.add_space(SPACE_SMALL);
            ui.label(
                RichText::new("Use Open to open a project folder, or New to create one.")
                    .color(p.on_surface_variant),
            );
        });
        return None;
    }
    if let Some(rep) = repair.as_mut() {
        ui.add_space(SPACE_SMALL);
        if rep.error.is_empty() {
            ui.label(
                RichText::new("Valid, opening editor…")
                    .color(p.success)
                    .strong(),
            );
        } else {
            ui.label(RichText::new("INVALID CONTRACT").color(p.error).strong());
            ui.label(RichText::new(&rep.error).color(p.warning).size(12.0));
        }
        ui.add_space(SPACE_SMALL);
        let pretty = text_button(ui, "pretty", p.warning);
        if pretty {
            rep.buffer = apic_core::json::pretty_json(&rep.buffer);
        }
        ui.add_space(SPACE_SMALL);
        // The multiline TextEdit has no scrollbar of its own; a long
        // invalid contract is only reachable inside an enclosing
        // ScrollArea (egui lays the editor out to full content height).
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let resp = ui.add(
                    egui::TextEdit::multiline(&mut rep.buffer)
                        .code_editor()
                        .desired_width(f32::INFINITY)
                        .desired_rows(24),
                );
                if resp.changed() || pretty {
                    rep.error = match apic_core::json::validate(&rep.buffer) {
                        Ok(()) => String::new(),
                        Err(e) => e.to_string(),
                    };
                    if rep.error.is_empty() && entries.get(rep.index).is_some() {
                        action = Some(ContractsAction::PromoteRepair);
                    }
                }
            });
        return action;
    }
    let Some(model) = model.as_mut() else {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("WELCOME TO APIC").color(p.primary).size(28.0));
            ui.label(RichText::new("Select a contract on the left.").color(p.on_surface_variant));
        });
        return None;
    };

    // Toolbar row: endpoint name on the left, EDIT/SAVE on the right.
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(SPACE_SMALL); // end padding, right of Save
            if text_button(ui, "Save", p.primary) {
                action = Some(ContractsAction::Save);
            }
            let edit_label = if *editing { "Cancel" } else { "Edit" };
            if text_button(ui, edit_label, p.primary) {
                action = Some(ContractsAction::ToggleEdit);
            }
            // The name fills the space to the left of the buttons.
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add_space(SPACE_SMALL); // start padding, left of the name
                endpoint_name(ui, model, *editing);
            });
        });
    });
    ui.add_space(SPACE_SMALL);

    egui::Frame::NONE
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = SPACE_MEDIUM;
            method_url_row(ui, model, *editing);
            ui.add_space(SPACE_SMALL);

            // 50:50 split with a vertical divider between the panes:
            // request tabs on the left, Response on the right.
            ui.horizontal_top(|ui| {
                let gap = ui.spacing().item_spacing.x;
                // Reserve the divider's own width plus a gap on each side so the
                // two panes stay an even 50:50.
                let divider_w = 6.0;
                let col_w = ((ui.available_width() - divider_w - gap * 2.0) / 2.0).max(0.0);
                let col_h = ui.available_height();

                // Left pane: Overview / Headers / Query / Request.
                ui.allocate_ui_with_layout(
                    egui::vec2(col_w, col_h),
                    egui::Layout::top_down(egui::Align::Min),
                    |left| {
                        left.horizontal(|ui| {
                            let mut t = |ui: &mut egui::Ui, label: &str, which: MainTab| {
                                if ui
                                    .selectable_label(
                                        *main_tab == which,
                                        RichText::new(label).color(if *main_tab == which {
                                            p.primary
                                        } else {
                                            p.on_surface_variant
                                        }),
                                    )
                                    .clicked()
                                {
                                    *main_tab = which;
                                }
                            };
                            t(ui, "Overview", MainTab::Overview);
                            t(ui, "Headers", MainTab::Headers);
                            t(ui, "Query", MainTab::Query);
                            t(ui, "Request", MainTab::Request);
                        });
                        left.separator();
                        // One scroll per pane (distinct id) so the left
                        // and right panes scroll independently.
                        egui::ScrollArea::vertical()
                            .id_salt("left_pane_scroll")
                            .auto_shrink([false, false])
                            .show(left, |ui| match *main_tab {
                                MainTab::Overview => endpoint_description(ui, model, *editing),
                                MainTab::Headers => headers(ui, model, *editing),
                                MainTab::Query => query_section(ui, model, *editing),
                                MainTab::Request => request_body(ui, model, *editing),
                            });
                    },
                );

                ui.separator();

                // Right pane: a Response / Header tab bar over a shared,
                // per-response status strip.
                ui.allocate_ui_with_layout(
                    egui::vec2(col_w, col_h),
                    egui::Layout::top_down(egui::Align::Min),
                    |right| {
                        right.horizontal(|ui| {
                            let mut t = |ui: &mut egui::Ui, label: &str, which: RespTab| {
                                if ui
                                    .selectable_label(
                                        *resp_tab_view == which,
                                        RichText::new(label).color(if *resp_tab_view == which {
                                            p.primary
                                        } else {
                                            p.on_surface_variant
                                        }),
                                    )
                                    .clicked()
                                {
                                    *resp_tab_view = which;
                                }
                            };
                            t(ui, "Response", RespTab::Body);
                            t(ui, "Header", RespTab::Headers);
                        });
                        right.separator();
                        // One scroll per pane (distinct id) starting under
                        // the "Response" title, so the left and right panes
                        // scroll independently as plain vertical content.
                        egui::ScrollArea::vertical()
                            .id_salt("right_pane_scroll")
                            .auto_shrink([false, false])
                            .show(right, |ui| {
                                // An endpoint is expected to document a
                                // response, so default to a 200 in edit mode
                                // (its editor shows without "+ new response").
                                if *editing && model.responses.is_empty() {
                                    model.responses.push(apic_core::edit::EditResponse::blank());
                                }
                                // Per-response status strip; its `code -
                                // title` tabs select the response whose
                                // body/headers the tab bar above shows.
                                response_code_selector(ui, model, resp_tab, *editing);
                                if model.responses.is_empty() {
                                    ui.label(
                                        RichText::new("(no responses)").color(p.on_surface_variant),
                                    );
                                    return;
                                }
                                if *resp_tab >= model.responses.len() {
                                    *resp_tab = 0;
                                }
                                ui.add_space(SPACE_SMALL);
                                let idx = *resp_tab;
                                match *resp_tab_view {
                                    RespTab::Body => response_body(ui, model, idx, *editing),
                                    RespTab::Headers => response_headers(ui, model, idx, *editing),
                                }
                            });
                    },
                );
            });
        });
    action
}
