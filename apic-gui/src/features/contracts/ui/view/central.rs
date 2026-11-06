//! The central body for the open contract: the viewer/editor, the repair
//! editor for an invalid file, or the empty-state text. With no project open
//! it draws nothing, since the shell shows the welcome screen instead.

use eframe::egui;
use egui::RichText;

use super::editor::{
    endpoint_title, headers, method_url_row, query_section, request_body, response_body,
    response_fields, response_headers, response_picker,
};
use crate::features::contracts::ui::state::{ContractsAction, ContractsState, RespTab};
use crate::ui::components::{Tab, empty_state, icon_button, tab_strip, text_button};
use crate::ui::theme::*;

/// The central body for the loaded contract: the viewer/editor, the repair
/// editor for an invalid file, or the empty-state text. With no project open
/// it draws nothing, since the shell shows the welcome screen instead.
///
/// The panel frame is owned by the app shell. Saving, toggling edit mode and
/// promoting a repaired file are returned as an action rather than done here.
///
/// # Arguments
///
/// * `ui` - the central panel to fill
/// * `state` - the contracts view model
/// * `has_project` - whether a project is open, otherwise nothing is drawn
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
        resp_tab_view,
        repair,
        entries,
        ..
    } = state;
    if no_project {
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
    let defined = |_: &str| true;

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
                endpoint_title(ui, model, *editing);
            });
        });
    });
    ui.add_space(SPACE_SMALL);

    egui::Frame::NONE
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = SPACE_MEDIUM;
            // Request tabs in a resizable left pane, Response in the rest.
            // The panel paints its divider with the parent's noninteractive
            // stroke, so that stroke is swapped only around the panel.
            let half = (ui.available_width() / 2.0).max(240.0);
            let saved_stroke = ui.visuals().widgets.noninteractive.bg_stroke;
            ui.visuals_mut().widgets.noninteractive.bg_stroke =
                egui::Stroke::new(1.0, p.outline_variant);
            egui::Panel::left("contract_request_pane")
                .resizable(true)
                .default_size(half)
                .min_size(240.0)
                .frame(egui::Frame::NONE.inner_margin(egui::Margin {
                    right: 16,
                    ..Default::default()
                }))
                .show(ui, |left| {
                    left.visuals_mut().widgets.noninteractive.bg_stroke = saved_stroke;
                    method_url_row(left, model, *editing, &defined);
                    left.add_space(12.0);
                    // One scroll per pane (distinct id) so the left
                    // and right panes scroll independently.
                    egui::ScrollArea::vertical()
                        .id_salt("left_pane_scroll")
                        .auto_shrink([false, false])
                        .show(left, |ui| {
                            section_heading(ui, "Headers");
                            headers(ui, model, *editing, &defined);
                            ui.add_space(16.0);
                            section_heading(ui, "Query");
                            query_section(ui, model, *editing, &defined);
                            ui.add_space(16.0);
                            section_heading(ui, "Request");
                            if let Some(a) = request_body(ui, model, *editing, &defined) {
                                action = Some(a);
                            }
                        });
                });
            ui.visuals_mut().widgets.noninteractive.bg_stroke = saved_stroke;

            egui::Frame::NONE
                .inner_margin(egui::Margin {
                    left: 16,
                    ..Default::default()
                })
                .show(ui, |ui| {
                    if let Some(a) = response_pane(ui, model, resp_tab, resp_tab_view, *editing) {
                        action = Some(a);
                    }
                });
        });
    action
}

/// The right pane: one header row with the `Response` and `Headers` tabs on
/// the left and the response picker on the right, the selected response's
/// code and title fields while editing, then the selected tab's body.
///
/// # Returns
///
/// The action the response picker or the body-type dropdown asks for this
/// frame.
fn response_pane(
    ui: &mut egui::Ui,
    model: &mut apic_core::edit::EditModel,
    resp_tab: &mut usize,
    resp_tab_view: &mut RespTab,
    editing: bool,
) -> Option<ContractsAction> {
    let mut action = None;
    // An endpoint is expected to document a response, so default to a 200 in
    // edit mode (its editor shows without "+ New example").
    if editing && model.responses.is_empty() {
        model.responses.push(apic_core::edit::EditResponse::blank());
    }
    if *resp_tab >= model.responses.len() {
        *resp_tab = 0;
    }
    let resp_headers = model
        .responses
        .get(*resp_tab)
        .map_or(0, |r| r.headers.len());
    ui.horizontal(|ui| {
        let mut idx = resp_tab_index(*resp_tab_view);
        tab_strip(
            ui,
            &mut idx,
            &[
                Tab::new("Response"),
                Tab::new("Headers").count(resp_headers),
            ],
        );
        *resp_tab_view = resp_tab_at(idx);
        if model.responses.is_empty() {
            return;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            action = response_picker(ui, model, resp_tab, editing);
        });
    });
    if editing && !model.responses.is_empty() {
        response_fields(ui, model, *resp_tab);
    }
    ui.add_space(8.0);
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
                RespTab::Body => {
                    if let Some(a) = response_body(ui, model, idx, editing) {
                        action = Some(a);
                    }
                }
                RespTab::Headers => response_headers(ui, model, idx, editing),
            }
        });
    action
}

/// A left-pane section heading: `label` in bold, in `on_surface_variant`.
fn section_heading(ui: &mut egui::Ui, label: &str) {
    let p = palette(ui.ctx());
    ui.label(RichText::new(label).strong().color(p.on_surface_variant));
    ui.add_space(6.0);
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

                let mut described = ContractsState {
                    model: Some(model_from(
                        r#"{ "name": "Login", "description": "Authenticate the user", "method": "POST",
                            "url": "{{baseUrl}}/auth/login", "headers": [], "responses": [] }"#,
                    )),
                    ..Default::default()
                };
                central_body(ui, &mut described, true);
                let mut undescribed = ContractsState {
                    model: Some(model_from(
                        r#"{ "name": "Login", "description": "", "method": "POST",
                            "url": "{{baseUrl}}/auth/login", "headers": [], "responses": [] }"#,
                    )),
                    ..Default::default()
                };
                central_body(ui, &mut undescribed, true);

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

                let mut classes = ContractsState {
                    model: Some(model_from(
                        r#"{ "name": "classes", "method": "GET", "url": "https://example.com", "headers": [],
                            "responses": [ { "code": 200, "description": "ok" }, { "code": 301, "description": "moved" },
                                           { "code": 404, "description": "not found" }, { "code": 500, "description": "boom" } ] }"#,
                    )),
                    ..Default::default()
                };
                central_body(ui, &mut classes, true);
                classes.editing = true;
                classes.resp_tab = 2;
                central_body(ui, &mut classes, true);
            });
        }
    }
}
