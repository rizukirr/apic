//! The central body for the open contract: the viewer/editor, the repair
//! editor for an invalid file, or the empty-state text.

use eframe::egui;
use egui::RichText;

use super::editor::{
    endpoint_description, endpoint_name, headers, method_url_row, query_section, request_body,
    response_body, response_code_selector, response_headers,
};
use crate::features::contracts::ui::state::{ContractsAction, ContractsState, MainTab, RespTab};
use crate::ui::components::{Tab, empty_state, icon_button, tab_strip, text_button};
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
    let has_unsaved = state.has_unsaved_edits();
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
        empty_state(
            ui,
            "No project open",
            "Use Open to open a project folder, or New to create one.",
        );
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
            ui.label(RichText::new("Invalid contract").color(p.error).strong());
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
        empty_state(ui, "No contract open", "Select a contract on the left.");
        return None;
    };

    // Toolbar row: endpoint name on the left, edit and save icons on the right.
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(SPACE_SMALL);
            if has_unsaved {
                ui.label(RichText::new("●").color(p.warning).size(10.0));
            }
            if icon_button(ui, egui_phosphor::regular::FLOPPY_DISK, "Save").clicked() {
                action = Some(ContractsAction::Save);
            }
            let (icon, hover) = if *editing {
                (egui_phosphor::regular::X, "Cancel")
            } else {
                (egui_phosphor::regular::PENCIL_SIMPLE, "Edit")
            };
            if icon_button(ui, icon, hover).clicked() {
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

            // Request tabs in a resizable left pane, Response in the rest.
            let half = (ui.available_width() / 2.0).max(240.0);
            egui::Panel::left("contract_request_pane")
                .resizable(true)
                .default_size(half)
                .min_size(240.0)
                .show(ui, |left| {
                    let mut idx = main_tab_index(*main_tab);
                    tab_strip(
                        left,
                        &mut idx,
                        &[
                            Tab::new("Overview"),
                            Tab::new("Headers").count(model.headers.len()),
                            Tab::new("Query").count(model.query.len()),
                            Tab::new("Request"),
                        ],
                    );
                    *main_tab = main_tab_at(idx);
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
                });

            // An endpoint is expected to document a response, so default to
            // a 200 in edit mode (its editor shows without "+ new response").
            if *editing && model.responses.is_empty() {
                model.responses.push(apic_core::edit::EditResponse::blank());
            }
            if *resp_tab >= model.responses.len() {
                *resp_tab = 0;
            }
            // Per-response status strip; its `code - title` tabs select the
            // response whose body/headers the tab bar below shows.
            response_code_selector(ui, model, resp_tab, *editing);
            let resp_headers = model
                .responses
                .get(*resp_tab)
                .map_or(0, |r| r.headers.len());
            let mut idx = resp_tab_index(*resp_tab_view);
            tab_strip(
                ui,
                &mut idx,
                &[Tab::new("Body"), Tab::new("Headers").count(resp_headers)],
            );
            *resp_tab_view = resp_tab_at(idx);
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("right_pane_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if model.responses.is_empty() {
                        empty_state(ui, "No responses", "");
                        return;
                    }
                    let idx = *resp_tab;
                    match *resp_tab_view {
                        RespTab::Body => response_body(ui, model, idx, *editing),
                        RespTab::Headers => response_headers(ui, model, idx, *editing),
                    }
                });
        });
    action
}

/// The [`tab_strip`] index of a left-pane tab.
fn main_tab_index(tab: MainTab) -> usize {
    match tab {
        MainTab::Overview => 0,
        MainTab::Headers => 1,
        MainTab::Query => 2,
        MainTab::Request => 3,
    }
}

/// The left-pane tab at a [`tab_strip`] index.
fn main_tab_at(idx: usize) -> MainTab {
    match idx {
        1 => MainTab::Headers,
        2 => MainTab::Query,
        3 => MainTab::Request,
        _ => MainTab::Overview,
    }
}

/// The [`tab_strip`] index of a response sub-tab.
fn resp_tab_index(tab: RespTab) -> usize {
    match tab {
        RespTab::Body => 0,
        RespTab::Headers => 1,
    }
}

/// The response sub-tab at a [`tab_strip`] index.
fn resp_tab_at(idx: usize) -> RespTab {
    match idx {
        1 => RespTab::Headers,
        _ => RespTab::Body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apic_core::edit::EditModel;

    use crate::ui::theme::{BOTH_THEMES, run_in_theme};

    fn model_from(json: &str) -> EditModel {
        let contract = apic_core::json::json_get(json, None).expect("valid sample contract");
        EditModel::from_contract(contract)
    }

    fn sample_model() -> EditModel {
        model_from(
            r#"{
            "name": "test",
            "method": "GET",
            "url": "https://example.com",
            "headers": [],
            "responses": [ { "code": 200, "description": "ok" } ]
        }"#,
        )
    }

    #[test]
    fn central_body_renders_each_state_in_both_themes() {
        for preference in BOTH_THEMES {
            run_in_theme(preference, |ui| {
                let mut state = ContractsState::default();
                central_body(ui, &mut state, false);
                central_body(ui, &mut state, true);

                state.model = Some(sample_model());
                state.original_model = state.model.clone();
                central_body(ui, &mut state, true);

                state.editing = true;
                if let Some(model) = state.model.as_mut() {
                    model.name = "renamed".to_owned();
                }
                assert!(state.has_unsaved_edits());
                central_body(ui, &mut state, true);

                let mut empty = ContractsState {
                    model: Some(model_from(
                        r#"{ "name": "none", "method": "GET", "url": "https://example.com", "headers": [], "responses": [] }"#,
                    )),
                    ..Default::default()
                };
                central_body(ui, &mut empty, true);
            });
        }
    }
}
