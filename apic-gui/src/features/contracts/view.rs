//! The panelled contract editor/viewer sections (endpoint, parameters, headers,
//! request body, responses). Each takes the shared [`EditModel`] and an
//! `editing` flag and renders the read or edit variant, composing the
//! primitives in [`crate::ui::components`]. Editing behavior itself lives in
//! [`apic_core::edit`]; these functions only translate clicks into
//! [`EditAction`]s.

use std::collections::BTreeMap;
use std::path::PathBuf;

use eframe::egui;
use egui::{Color32, RichText};

use apic_core::edit::{EditAction, EditBody, EditModel, Field, apply};
use apic_core::json::method_str;

use crate::app::App;
use crate::app::actions::SidebarAction;
use crate::app::state::ShellState;
use crate::features::contracts::state::{ContractsState, DeleteTarget, MainTab, RespTab};
use crate::ui::components::{
    add_button, bordered_input, delete_button, fill_column, header_label, json_editor,
    metadata_table, required_chip, section_label, table_frame, tcell_edit, text_button,
};
use crate::ui::focus::{request_new_row_focus, take_pending_focus};
use crate::ui::theme::*;
use egui_extras::Column;

/// Color for an HTTP method badge.
fn method_color(method: &str) -> Color32 {
    match method {
        "GET" | "HEAD" => GREEN,
        "POST" => CYAN,
        "PUT" | "PATCH" => AMBER,
        "DELETE" => RED,
        _ => DIM,
    }
}

/// A filled, method-colored badge (read mode); the edit view uses a plain
/// button instead so it can cycle the method on click.
fn method_badge(ui: &mut egui::Ui, method: &str) {
    ui.label(
        RichText::new(format!(" {method} "))
            .color(BG)
            .background_color(method_color(method))
            .strong(),
    );
}

// egui temp-data keys for the "focus the new row's name field" markers, one per
// editable list.
const FOCUS_QUERY: &str = "apic.focus.query";
const FOCUS_HEADER: &str = "apic.focus.header";

/// The endpoint name, rendered inline on the toolbar row (left of EDIT/SAVE).
/// Editable frameless heading in edit mode; a heading label otherwise.
fn endpoint_name(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    if editing {
        ui.add(
            egui::TextEdit::singleline(&mut model.name)
                .frame(egui::Frame::NONE)
                .hint_text("endpoint name")
                .font(egui::TextStyle::Heading)
                .text_color(TEXT)
                .desired_width(f32::INFINITY),
        );
    } else {
        ui.label(RichText::new(&model.name).color(TEXT).heading());
    }
}

/// The endpoint description, shown under the name/url row. Frameless multiline in
/// edit mode; dim text otherwise (hidden when empty in view mode).
fn endpoint_description(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    if editing {
        ui.add(
            egui::TextEdit::multiline(&mut model.description)
                .frame(egui::Frame::NONE)
                .hint_text("description")
                .text_color(DIM)
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        );
    } else if !model.description.is_empty() {
        ui.label(RichText::new(&model.description).color(DIM));
    }
}

/// The `[ METHOD ] [ url................ ]` top row.
fn method_url_row(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    table_frame(ui, |ui| {
        ui.horizontal(|ui| {
            let method = method_str(&model.method);
            if editing {
                // Pick the HTTP method from a dropdown of all the choices.
                egui::ComboBox::from_id_salt("method")
                    .width(90.0)
                    .selected_text(RichText::new(&method).color(method_color(&method)).strong())
                    .show_ui(ui, |ui| {
                        for m in apic_core::json::method_all() {
                            let name = method_str(&m);
                            let selected = name == method;
                            if ui
                                .selectable_label(
                                    selected,
                                    RichText::new(&name).color(method_color(&name)).strong(),
                                )
                                .clicked()
                            {
                                model.method = m;
                            }
                        }
                    });
            } else {
                method_badge(ui, &method);
            }
            ui.add_space(SPACE_MEDIUM);
            if editing {
                ui.add(
                    egui::TextEdit::singleline(&mut model.url)
                        .frame(egui::Frame::NONE)
                        .hint_text("https://host/path/{id}")
                        .text_color(CYAN)
                        .desired_width(f32::INFINITY),
                );
            } else {
                ui.label(RichText::new(&model.url).color(CYAN).strong());
            }
        });
    });
}

/// Body of the QUERY PARAMS section. Uses the flat `{name, value, description}`
/// query model; path variables now live inline in the URL string.
fn query_section(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    let mut actions: Vec<EditAction> = Vec::new();
    table_frame(ui, |ui| {
        let fixed = if editing {
            vec![120.0, 88.0, 120.0, 26.0]
        } else {
            vec![120.0, 88.0, 120.0]
        };
        let stretch = fill_column(ui, &fixed);
        let mut t = metadata_table(ui)
            .column(Column::exact(120.0)) // key
            .column(Column::exact(88.0)) // requirement
            .column(Column::exact(120.0)) // value
            .column(stretch); // description
        if editing {
            t = t.column(Column::exact(26.0)); // delete
        }
        t.header(TABLE_HEADER_H, |mut h| {
            h.col(|ui| header_label(ui, "key"));
            h.col(|ui| header_label(ui, "requirement"));
            h.col(|ui| header_label(ui, "value"));
            h.col(|ui| header_label(ui, "description"));
            if editing {
                h.col(|_| {});
            }
        })
        .body(|mut body| {
            for i in 0..model.query.len() {
                body.row(TABLE_ROW_H, |mut row| {
                    row.col(|ui| {
                        if editing {
                            let r = tcell_edit(ui, &mut model.query[i].name, "name");
                            take_pending_focus(ui, FOCUS_QUERY, i, &r);
                        } else {
                            ui.label(RichText::new(&model.query[i].name).color(CYAN));
                        }
                    });
                    row.col(|ui| {
                        if required_chip(ui, model.query[i].required, editing).is_some() {
                            actions.push(EditAction::ToggleBool {
                                field: Field::QueryRequired(i),
                            });
                        }
                    });
                    row.col(|ui| {
                        if editing {
                            tcell_edit(ui, &mut model.query[i].value, "value");
                        } else {
                            ui.label(RichText::new(&model.query[i].value).color(TEXT));
                        }
                    });
                    row.col(|ui| {
                        if editing {
                            tcell_edit(ui, &mut model.query[i].description, "description");
                        } else {
                            ui.label(RichText::new(&model.query[i].description).color(TEXT));
                        }
                    });
                    if editing {
                        row.col(|ui| {
                            if delete_button(ui) {
                                actions.push(EditAction::Delete {
                                    field: Field::QueryName(i),
                                });
                            }
                        });
                    }
                });
            }
        });
    });
    if editing && add_button(ui, "+ query") {
        request_new_row_focus(ui, FOCUS_QUERY, model.query.len());
        actions.push(EditAction::Add {
            field: Field::QueryAdd,
        });
    }
    for a in &actions {
        apply(model, a);
    }
}

/// Response header rows for the selected response `idx`.
fn response_headers(ui: &mut egui::Ui, model: &mut EditModel, idx: usize, editing: bool) {
    let Some(_) = model.responses.get(idx) else {
        return;
    };
    let mut actions: Vec<EditAction> = Vec::new();
    table_frame(ui, |ui| {
        let fixed = if editing {
            vec![150.0, 88.0, 26.0]
        } else {
            vec![150.0, 88.0]
        };
        let stretch = fill_column(ui, &fixed);
        let mut t = metadata_table(ui)
            .column(Column::exact(150.0)) // header
            .column(Column::exact(88.0)) // requirement
            .column(stretch); // value
        if editing {
            t = t.column(Column::exact(26.0)); // delete
        }
        t.header(TABLE_HEADER_H, |mut h| {
            h.col(|ui| header_label(ui, "header"));
            h.col(|ui| header_label(ui, "requirement"));
            h.col(|ui| header_label(ui, "value"));
            if editing {
                h.col(|_| {});
            }
        })
        .body(|mut body| {
            let len = model.responses[idx].headers.len();
            for i in 0..len {
                body.row(TABLE_ROW_H, |mut row| {
                    row.col(|ui| {
                        if editing {
                            tcell_edit(ui, &mut model.responses[idx].headers[i].name, "name");
                        } else {
                            ui.label(
                                RichText::new(&model.responses[idx].headers[i].name).color(CYAN),
                            );
                        }
                    });
                    row.col(|ui| {
                        let required = model.responses[idx].headers[i].required;
                        // Response headers have no toggle action; flip directly.
                        if let Some(new) = required_chip(ui, required, editing) {
                            model.responses[idx].headers[i].required = new;
                        }
                    });
                    row.col(|ui| {
                        if editing {
                            tcell_edit(ui, &mut model.responses[idx].headers[i].value, "value");
                        } else {
                            ui.label(
                                RichText::new(&model.responses[idx].headers[i].value).color(TEXT),
                            );
                        }
                    });
                    if editing {
                        row.col(|ui| {
                            if delete_button(ui) {
                                actions.push(EditAction::Delete {
                                    field: Field::ResponseHeaderName(idx, i),
                                });
                            }
                        });
                    }
                });
            }
        });
    });
    if editing && add_button(ui, "+ header") {
        actions.push(EditAction::Add {
            field: Field::ResponseHeaderAdd(idx),
        });
    }
    for a in &actions {
        apply(model, a);
    }
}

fn headers(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    let mut actions: Vec<EditAction> = Vec::new();
    table_frame(ui, |ui| {
        let fixed = if editing {
            vec![150.0, 88.0, 26.0]
        } else {
            vec![150.0, 88.0]
        };
        let stretch = fill_column(ui, &fixed);
        let mut t = metadata_table(ui)
            .column(Column::exact(150.0)) // header
            .column(Column::exact(88.0)) // requirement
            .column(stretch); // value
        if editing {
            t = t.column(Column::exact(26.0)); // delete
        }
        t.header(TABLE_HEADER_H, |mut h| {
            h.col(|ui| header_label(ui, "header"));
            h.col(|ui| header_label(ui, "requirement"));
            h.col(|ui| header_label(ui, "value"));
            if editing {
                h.col(|_| {});
            }
        })
        .body(|mut body| {
            for i in 0..model.headers.len() {
                body.row(TABLE_ROW_H, |mut row| {
                    row.col(|ui| {
                        if editing {
                            let r = tcell_edit(ui, &mut model.headers[i].name, "name");
                            take_pending_focus(ui, FOCUS_HEADER, i, &r);
                        } else {
                            ui.label(RichText::new(&model.headers[i].name).color(CYAN));
                        }
                    });
                    row.col(|ui| {
                        if required_chip(ui, model.headers[i].required, editing).is_some() {
                            actions.push(EditAction::ToggleBool {
                                field: Field::HeaderRequired(i),
                            });
                        }
                    });
                    row.col(|ui| {
                        if editing {
                            tcell_edit(ui, &mut model.headers[i].value, "value");
                        } else {
                            ui.label(RichText::new(&model.headers[i].value).color(TEXT));
                        }
                    });
                    if editing {
                        row.col(|ui| {
                            if delete_button(ui) {
                                actions.push(EditAction::Delete {
                                    field: Field::HeaderName(i),
                                });
                            }
                        });
                    }
                });
            }
        });
    });
    if editing && add_button(ui, "+ header") {
        request_new_row_focus(ui, FOCUS_HEADER, model.headers.len());
        actions.push(EditAction::Add {
            field: Field::HeaderAdd,
        });
    }
    for a in &actions {
        apply(model, a);
    }
}

fn request_body(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    ui.spacing_mut().item_spacing.y = SPACE_MEDIUM;
    if editing {
        // The request body is always editable — materialize an empty one so the
        // JSON editor is shown by default. An untouched (blank) body is dropped
        // on save (see `EditModel::to_json`), so this never pollutes contracts.
        let req = model.request.get_or_insert_with(EditBody::empty);
        ui.horizontal(|ui| {
            section_label(ui, "JSON");
            ui.spacing_mut().item_spacing.x = SPACE_MEDIUM;
            ui.add_space(SPACE_MEDIUM);
            if text_button(ui, "Pretty", AMBER) {
                req.example = apic_core::json::pretty_json(&req.example);
            }
        });
        json_editor(ui, &mut req.example, true);
    } else {
        let mut empty = String::new();
        let text = match model.request.as_mut() {
            Some(req) => &mut req.example,
            None => &mut empty,
        };
        json_editor(ui, text, false);
    }
    ui.add_space(SPACE_MEDIUM);
}

/// The response-code tab strip: one selectable tab per response code plus a
/// `+ new response` button. In edit mode the active tab's code is edited inline
/// (frameless) and carries an `x` to delete that response. Selecting a tab sets
/// `resp_tab`, so both `responses()` and the RespHeader tab share it.
fn response_code_selector(
    ui: &mut egui::Ui,
    model: &mut EditModel,
    resp_tab: &mut usize,
    editing: bool,
) {
    let mut actions: Vec<EditAction> = Vec::new();
    // The active tab's code editor carries a stable id so we can move the caret
    // to it the instant a tab is selected or a new response is added.
    let code_edit_id = ui.make_persistent_id("resp_code_edit");
    // Set when the active response changes this frame so the code field takes
    // focus on the next frame (when it renders as the editable, active tab).
    let mut focus_code = false;
    // The strip scrolls horizontally rather than wrapping, so many response
    // codes stay on one line with the tabs' `code - title` labels intact.
    egui::ScrollArea::horizontal()
        .id_salt("resp_code_strip")
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for i in 0..model.responses.len() {
                    let selected = i == *resp_tab;
                    let code_ok = model.responses[i].code.trim().parse::<u16>().is_ok();
                    let color = if !code_ok {
                        RED
                    } else if selected {
                        GREEN
                    } else {
                        DIM
                    };
                    if editing && selected {
                        // The active tab is edited in place, no bordered box:
                        // `[code] - [title] x`, code first so focus lands there.
                        // Tight spacing so the fields read as one label, not a row.
                        ui.spacing_mut().item_spacing.x = 4.0;
                        // Size the code field to its text (min 3 digits) so the
                        // `-` hugs the code instead of sitting past a fixed width.
                        let code_w = {
                            let code = &model.responses[i].code;
                            let shown: String = if code.chars().count() < 3 {
                                "000".to_string()
                            } else {
                                code.chars().take(6).collect()
                            };
                            let font = egui::TextStyle::Body.resolve(ui.style());
                            ui.painter()
                                .layout_no_wrap(shown, font, egui::Color32::PLACEHOLDER)
                                .size()
                                .x
                        };
                        ui.add(
                            egui::TextEdit::singleline(&mut model.responses[i].code)
                                .id(code_edit_id)
                                .frame(egui::Frame::NONE)
                                .desired_width(code_w + 10.0)
                                .text_color(color),
                        );
                        // A status code is exactly 3 digits: drop anything else
                        // and cap the length as the user types.
                        let code = &mut model.responses[i].code;
                        if code.len() > 3 || !code.bytes().all(|b| b.is_ascii_digit()) {
                            *code = code
                                .chars()
                                .filter(|c| c.is_ascii_digit())
                                .take(3)
                                .collect();
                        }
                        ui.label(RichText::new("-").color(DIM));
                        // Size the title field to its text (30-char cap) so the
                        // `x` sits right after the title instead of a fixed gap.
                        let title_w = {
                            let title = &model.responses[i].description;
                            let shown: String = if title.is_empty() {
                                "title".to_string()
                            } else {
                                title.chars().take(30).collect()
                            };
                            let font = egui::TextStyle::Body.resolve(ui.style());
                            ui.painter()
                                .layout_no_wrap(shown, font, egui::Color32::PLACEHOLDER)
                                .size()
                                .x
                        };
                        ui.add(
                            egui::TextEdit::singleline(&mut model.responses[i].description)
                                .frame(egui::Frame::NONE)
                                .desired_width(title_w + 18.0)
                                .hint_text("title")
                                .text_color(DIM),
                        );
                        if delete_button(ui) {
                            actions.push(EditAction::Delete {
                                field: Field::ResponseCode(i),
                            });
                        }
                    } else {
                        let label = status_tab_label(
                            &model.responses[i].code,
                            &model.responses[i].description,
                        );
                        if ui
                            .selectable_label(selected, RichText::new(label).color(color).strong())
                            .clicked()
                        {
                            *resp_tab = i;
                            focus_code = true;
                        }
                    }
                }
                if editing && add_button(ui, "+ new response") {
                    // Focus the new tab's code so the user can type it straight away.
                    *resp_tab = model.responses.len();
                    focus_code = true;
                    actions.push(EditAction::Add {
                        field: Field::ResponseAdd,
                    });
                }
            });
        });
    if editing && focus_code {
        ui.ctx().memory_mut(|m| m.request_focus(code_edit_id));
    }
    for a in &actions {
        apply(model, a);
    }
}

/// The status-tab label: `code - title`, or just `code` when the response has no
/// title, truncated to 30 characters (with an ellipsis) so long titles never
/// stretch a tab off the strip.
fn status_tab_label(code: &str, title: &str) -> String {
    let code = if code.trim().is_empty() { "?" } else { code };
    let full = if title.trim().is_empty() {
        code.to_string()
    } else {
        format!("{code} - {title}")
    };
    if full.chars().count() > 30 {
        let head: String = full.chars().take(29).collect();
        format!("{head}…")
    } else {
        full
    }
}

/// Renders the selected response's JSON example body. The caller (the Response
/// tab) has already drawn the code-tab strip, which now also owns the response's
/// title, and guarantees `idx` is a valid response index.
fn response_body(ui: &mut egui::Ui, model: &mut EditModel, idx: usize, editing: bool) {
    ui.spacing_mut().item_spacing.y = SPACE_SMALL;
    let r = &mut model.responses[idx];
    // The title/description now lives in the status tab, so the body is just
    // the JSON example (with its Pretty control in edit mode).
    if editing {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = SPACE_MEDIUM;
            section_label(ui, "JSON");
            ui.add_space(SPACE_MEDIUM);
            if text_button(ui, "Pretty", AMBER) {
                r.example = apic_core::json::pretty_json(&r.example);
            }
        });
    }
    json_editor(ui, &mut r.example, editing);
    ui.add_space(SPACE_MEDIUM);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_section_renders_without_panicking() {
        // `__run_test_ui` takes `impl Fn`, so build the model and tab index
        // fresh inside the closure rather than capturing them mutably.
        eframe::egui::__run_test_ui(|ui| {
            let c = apic_core::json::json_get(
                r#"{ "name":"t","description":"d","method":"GET","url":"https://h/v1/{id}",
                     "query":[{"name":"page","value":"2","description":"d","required":true}],
                     "headers":[{"name":"Content-Type","value":"application/json","required":true}],
                     "responses":[{"code":200,"description":"ok",
                        "headers":[{"name":"X","value":"1","required":false}]}] }"#,
                None,
            )
            .unwrap();
            let mut m = EditModel::from_contract(c);
            let mut resp = 0usize;
            endpoint_name(ui, &mut m, true);
            endpoint_description(ui, &mut m, true);
            method_url_row(ui, &mut m, true);
            headers(ui, &mut m, true);
            headers(ui, &mut m, false);
            query_section(ui, &mut m, true);
            request_body(ui, &mut m, true);
            response_code_selector(ui, &mut m, &mut resp, true);
            response_body(ui, &mut m, 0, true);
            response_headers(ui, &mut m, 0, true);
        });
    }
}

// egui temp-data keys for the "focus the input when the dialog opens" markers,
// claimed once via `take_pending_focus` the first frame each modal renders.
pub(crate) const FOCUS_NEW_REQUEST: &str = "apic.focus.new_request";
pub(crate) const FOCUS_NEW_TEMPLATE: &str = "apic.focus.new_template";

// vibekit: these remain `impl App` methods because they read `self.shell`
// alongside `self.contracts`. The target shape is
// `fn view(&mut ContractsState) -> Option<ContractsAction>`; converting them is
// deferred so this stays a pure relocation. Upgrade path: thread the handful of
// `self.shell.status` writes through a returned action.
impl App {
    /// Enter edit mode, snapshotting the current model so the edits can be
    /// discarded on cancel.
    pub(crate) fn begin_edit(&mut self) {
        self.contracts.original_model = self.contracts.model.clone();
        self.contracts.editing = true;
    }

    /// Leave edit mode, restoring the pre-edit snapshot and discarding any edits
    /// made since [ EDIT ] was pressed.
    pub(crate) fn cancel_edit(&mut self) {
        if let Some(original) = self.contracts.original_model.take() {
            self.contracts.model = Some(original);
        }
        self.contracts.editing = false;
    }

    /// Renders the "new template" dialog when open, and applies the result.
    pub(crate) fn new_template_dialog(&mut self, ctx: &egui::Context) {
        if self.contracts.new_template.is_none() {
            return;
        }
        let mut create = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::new("new_template_modal"))
            .frame(
                egui::Frame::window(&ctx.style_of(ctx.theme()))
                    .inner_margin(egui::Margin::same(16)),
            )
            .show(ctx, |ui| {
                ui.set_min_width(320.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("NEW TEMPLATE")
                            .color(GREEN)
                            .strong()
                            .size(16.0),
                    );
                });
                ui.add_space(SPACE_SMALL);
                ui.label(RichText::new("template name").color(DIM));
                ui.add_space(SPACE_MEDIUM);
                let buf = self.contracts.new_template.as_mut().expect("dialog open");
                let resp = bordered_input(ui, buf, f32::INFINITY, "");
                // Drop the caret into the input the frame the dialog opens.
                take_pending_focus(ui, FOCUS_NEW_TEMPLATE, "open", &resp);
                // Submit on Enter, same as clicking Create.
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    create = true;
                }
                ui.add_space(SPACE_LARGE);
                ui.columns(2, |cols| {
                    cols[0].vertical_centered(|ui| {
                        if text_button(ui, "Create", GREEN) {
                            create = true;
                        }
                    });
                    cols[1].vertical_centered(|ui| {
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        if create {
            let name = self.contracts.new_template.take().unwrap_or_default();
            self.create_template(&name);
        } else if cancel || modal.should_close() {
            self.contracts.new_template = None;
        }
    }

    /// Renders the "new request" dialog when open, and applies the result.
    pub(crate) fn new_request_dialog(&mut self, ctx: &egui::Context) {
        if self.contracts.new_request.is_none() {
            return;
        }
        let mut create = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::new("new_request_modal"))
            .frame(egui::Frame::window(&ctx.style_of(ctx.theme())).inner_margin(egui::Margin::same(16)))
            .show(ctx, |ui| {
                ui.set_min_width(320.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("NEW REQUEST").color(GREEN).strong().size(16.0));
                });
                ui.add_space(SPACE_SMALL);
                ui.label(RichText::new("path under the contracts directory").color(DIM));
                ui.add_space(SPACE_MEDIUM);
                let buf = self.contracts.new_request.as_mut().expect("dialog open");
                let resp = bordered_input(ui, buf, f32::INFINITY, "");
                // Drop the caret into the input the frame the dialog opens.
                take_pending_focus(ui, FOCUS_NEW_REQUEST, "open", &resp);
                // Submit on Enter, same as clicking Create.
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    create = true;
                }
                ui.add_space(SPACE_EXTRA_SMALL);
                ui.label(
                    RichText::new("end with .json for a contract (auth/logout.json), a bare name makes a folder")
                        .color(DIM)
                        .size(10.0),
                );

                // With more than one template, let the user pick which one seeds
                // the contract. With one it is used automatically; with none the
                // built-in default is used.
                if self.contracts.templates.len() > 1 {
                    let names: Vec<String> =
                        self.contracts.templates.iter().map(|(n, _)| n.clone()).collect();
                    let current = names
                        .get(self.contracts.new_request_seed)
                        .cloned()
                        .unwrap_or_default();
                    ui.add_space(SPACE_MEDIUM);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("template").color(DIM));
                        egui::ComboBox::from_id_salt("new_request_template")
                            .selected_text(RichText::new(current).color(GREEN))
                            .show_ui(ui, |ui| {
                                for (i, name) in names.iter().enumerate() {
                                    ui.selectable_value(&mut self.contracts.new_request_seed, i, name);
                                }
                            });
                    });
                }

                ui.add_space(SPACE_LARGE);
                ui.columns(2, |cols| {
                    cols[0].vertical_centered(|ui| {
                        if text_button(ui, "Create", GREEN) {
                            create = true;
                        }
                    });
                    cols[1].vertical_centered(|ui| {
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        if create {
            let path = self.contracts.new_request.take().unwrap_or_default();
            // Choose the seeding template: none -> built-in default; one -> it;
            // many -> the user's pick.
            let template = match self.contracts.templates.len() {
                0 => None,
                1 => Some(self.contracts.templates[0].1.clone()),
                _ => self
                    .contracts
                    .templates
                    .get(self.contracts.new_request_seed)
                    .map(|(_, p)| p.clone()),
            };
            self.create_request(&path, template);
        } else if cancel || modal.should_close() {
            self.contracts.new_request = None;
        }
    }

    /// Modal shown when a picked non-project folder has invalid contracts: the
    /// user must fix them before it can be opened/initialized.
    pub(crate) fn open_blocked_dialog(&mut self, ctx: &egui::Context) {
        let Some(failures) = self.contracts.open_blocked.clone() else {
            return;
        };
        let mut close = false;
        egui::Window::new("Fix these contracts first")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("This folder is not an apic project and has invalid contracts. Fix them, then open it again.")
                        .color(TEXT),
                );
                ui.add_space(SPACE_SMALL);
                for (path, err) in &failures {
                    ui.label(RichText::new(path.to_string_lossy()).color(RED).strong());
                    ui.label(RichText::new(err).color(DIM).size(11.0));
                    ui.add_space(SPACE_EXTRA_SMALL);
                }
                ui.add_space(SPACE_EXTRA_SMALL);
                if text_button(ui, "Close", GREEN) {
                    close = true;
                }
            });
        if close {
            self.contracts.open_blocked = None;
        }
    }

    /// Renders the delete-confirmation dialog when a delete is pending, and
    /// performs the deletion on confirm.
    pub(crate) fn delete_dialog(&mut self, ctx: &egui::Context) {
        let Some(target) = self.contracts.pending_delete.clone() else {
            return;
        };
        let (what, name, folder_warn) = match &target {
            DeleteTarget::Contract { rel, is_dir } => (
                if *is_dir { "folder" } else { "contract" },
                rel.clone(),
                *is_dir,
            ),
            DeleteTarget::Template { name, .. } => ("template", name.clone(), false),
        };
        let mut confirm = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::new("delete_modal"))
            .frame(
                egui::Frame::window(&ctx.style_of(ctx.theme()))
                    .inner_margin(egui::Margin::same(16)),
            )
            .show(ctx, |ui| {
                ui.set_min_width(320.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("DELETE").color(RED).strong().size(16.0));
                });
                ui.add_space(SPACE_MEDIUM);
                ui.label(RichText::new(format!("Delete {what}")).color(DIM));
                ui.label(RichText::new(&name).color(TEXT).strong());
                if folder_warn {
                    ui.add_space(SPACE_EXTRA_SMALL);
                    ui.label(
                        RichText::new("this also deletes every contract inside it")
                            .color(RED)
                            .size(10.0),
                    );
                }
                ui.add_space(SPACE_LARGE);
                ui.columns(2, |cols| {
                    cols[0].vertical_centered(|ui| {
                        if text_button(ui, "Delete", RED) {
                            confirm = true;
                        }
                    });
                    cols[1].vertical_centered(|ui| {
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        if confirm {
            self.contracts.pending_delete = None;
            self.perform_delete(&target);
        } else if cancel || modal.should_close() {
            self.contracts.pending_delete = None;
        }
    }
}

/// What [`central_body`] asks the shell to do after the panel closes.
///
/// The body only holds the contracts slice, so anything needing `&mut App`
/// (re-reading the project, taking an edit snapshot) is recorded here and
/// applied by the caller.
#[derive(Default)]
pub(crate) struct CentralOutcome {
    /// The Edit/Cancel button was clicked; the shell enters or leaves edit
    /// mode so the snapshot is taken or restored on `App`.
    pub(crate) toggle_edit: bool,

    /// A repaired contract is now valid JSON: write this buffer to this path
    /// and reload the project.
    pub(crate) promote: Option<(PathBuf, String)>,
}

/// Left sidebar body: a TEMPLATES section on top, then the contract picker
/// (folder tree, method-badged, filtered by search).
///
/// The panel frame is owned by the app shell, not by this feature, so that a
/// second sidebar tab can render into the same frame; this only fills it.
pub(crate) fn sidebar_body(ui: &mut egui::Ui, state: &mut ContractsState) -> Option<SidebarAction> {
    let q = state.search.to_lowercase();
    let mut tree = TreeNode::default();
    for (i, e) in state.entries.iter().enumerate() {
        if q.is_empty() || e.rel.to_lowercase().contains(&q) {
            tree.insert(&e.rel, i, &e.method, e.error.is_some());
        }
    }
    let selected = state.selected;
    let sel_template = state.selected_template;
    let templates: Vec<(String, PathBuf)> = state.templates.clone();
    let mut action = None;
    let mut to_contract = None;
    egui::Panel::bottom("new_request_bar")
        .show_separator_line(false)
        .show(ui, |ui| {
            ui.add_space(SPACE_EXTRA_SMALL);
            let button = egui::Button::new(RichText::new("[ NEW REQUEST ]").color(BG)).fill(GREEN);
            if ui.add_sized([ui.available_width(), 26.0], button).clicked() {
                action = Some(SidebarAction::NewRequest(String::new()));
            }
            ui.add_space(SPACE_EXTRA_SMALL);
        });

    ui.add_space(SPACE_MEDIUM);
    ui.horizontal(|ui| {
        ui.label(RichText::new("TEMPLATES").color(DIM).size(11.0));
        if ui
            .small_button(RichText::new("+").color(GREEN))
            .on_hover_text("New template")
            .clicked()
        {
            action = Some(SidebarAction::NewTemplate);
        }
    });
    ui.separator();
    if templates.is_empty() {
        ui.label(RichText::new("(none)").color(DIM));
    }
    for (i, (name, path)) in templates.iter().enumerate() {
        ui.horizontal(|ui| {
            // Reserve the trailing delete button first so the name
            // label truncates to the space that's left instead of
            // forcing the panel wider than its dragged width.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button(RichText::new("-").color(DIM))
                    .on_hover_text("Delete this template")
                    .clicked()
                {
                    action = Some(SidebarAction::RequestDelete(DeleteTarget::Template {
                        name: name.clone(),
                        path: path.clone(),
                    }));
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let label = egui::Button::selectable(
                        sel_template == Some(i),
                        RichText::new(format!("◆ {name}")).color(AMBER),
                    )
                    .truncate();
                    if ui.add(label).clicked() {
                        action = Some(SidebarAction::LoadTemplate(i));
                    }
                });
            });
        });
    }

    ui.add_space(10.0);
    ui.label(RichText::new("CONTRACTS").color(DIM).size(11.0));
    ui.separator();
    bordered_input(ui, &mut state.search, f32::INFINITY, "SEARCH...");
    ui.add_space(SPACE_EXTRA_SMALL);

    let mut new_in = None;
    let mut delete = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            tree.show(ui, "", selected, &mut to_contract, &mut new_in, &mut delete);
        });
    if let Some(prefix) = new_in {
        action = Some(SidebarAction::NewRequest(prefix));
    }
    if let Some((rel, is_dir)) = delete {
        action = Some(SidebarAction::RequestDelete(DeleteTarget::Contract {
            rel,
            is_dir,
        }));
    }
    if let Some(i) = to_contract {
        action = Some(SidebarAction::LoadContract(i));
    }
    action
}

/// The central body for the loaded contract: the viewer/editor, the repair
/// editor for an invalid file, or the empty-state text.
///
/// The panel frame is owned by the app shell. Work that needs `&mut App` is
/// reported through `out` rather than done here, since this only sees the
/// contracts slice.
pub(crate) fn central_body(
    ui: &mut egui::Ui,
    shell: &mut ShellState,
    contracts: &mut ContractsState,
    out: &mut CentralOutcome,
) {
    let no_project = shell.project_root.is_none();
    let ContractsState {
        model,
        path,
        editing,
        resp_tab,
        main_tab,
        resp_tab_view,
        repair,
        entries,
        original_model,
        ..
    } = contracts;
    if no_project {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("No project open").color(DIM).size(16.0));
            ui.add_space(SPACE_SMALL);
            ui.label(
                RichText::new("Use Open to open a project folder, or New to create one.")
                    .color(DIM),
            );
        });
        return;
    }
    if let Some(rep) = repair.as_mut() {
        ui.add_space(SPACE_SMALL);
        if rep.error.is_empty() {
            ui.label(
                RichText::new("Valid, opening editor…")
                    .color(GREEN)
                    .strong(),
            );
        } else {
            ui.label(RichText::new("INVALID CONTRACT").color(RED).strong());
            ui.label(RichText::new(&rep.error).color(AMBER).size(12.0));
        }
        ui.add_space(SPACE_SMALL);
        let pretty = text_button(ui, "pretty", AMBER);
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
                    if rep.error.is_empty()
                        && let Some(entry) = entries.get(rep.index)
                    {
                        out.promote = Some((entry.path.clone(), rep.buffer.clone()));
                    }
                }
            });
        return;
    }
    let Some(model) = model.as_mut() else {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("WELCOME TO APIC").color(GREEN).size(28.0));
            ui.label(RichText::new("Select a contract on the left.").color(DIM));
        });
        return;
    };

    // Toolbar row: endpoint name on the left, EDIT/SAVE on the right.
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(SPACE_SMALL); // end padding, right of Save
            if text_button(ui, "Save", GREEN) {
                match path.as_deref() {
                    Some(p) => match model.save(p) {
                        Ok(()) => {
                            shell.status = format!("saved {}", p.display());
                            *editing = false; // back to read-only on success
                            *original_model = None; // commit: drop the snapshot
                            // Refresh this contract's sidebar method badge.
                            if let Some(e) = entries.iter_mut().find(|e| e.path.as_path() == p) {
                                e.method = method_str(&model.method);
                            }
                        }
                        Err(e) => shell.status = format!("save error: {e}"),
                    },
                    None => shell.status = "no path to save to".into(),
                }
            }
            let edit_label = if *editing { "Cancel" } else { "Edit" };
            if text_button(ui, edit_label, GREEN) {
                // Applied after the panel closure via begin_edit/cancel_edit
                // so the snapshot is taken/restored on `self`.
                out.toggle_edit = true;
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
                                            GREEN
                                        } else {
                                            DIM
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
                            let mut t =
                                |ui: &mut egui::Ui, label: &str, which: RespTab| {
                                    if ui
                                        .selectable_label(
                                            *resp_tab_view == which,
                                            RichText::new(label).color(
                                                if *resp_tab_view == which { GREEN } else { DIM },
                                            ),
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
                                    ui.label(RichText::new("(no responses)").color(DIM));
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
}

/// A folder tree of contracts built from their `/`-separated relative paths.
/// Leaves carry the index into `App::entries` and the method for the badge.
#[derive(Default)]
struct TreeNode {
    dirs: BTreeMap<String, TreeNode>,
    files: Vec<(String, usize, String, bool)>, // (leaf label, entry index, method, invalid)
}

impl TreeNode {
    fn insert(&mut self, rel: &str, idx: usize, method: &str, invalid: bool) {
        match rel.split_once('/') {
            Some((dir, rest)) => self
                .dirs
                .entry(dir.to_string())
                .or_default()
                .insert(rest, idx, method, invalid),
            None => self
                .files
                .push((rel.to_string(), idx, method.to_string(), invalid)),
        }
    }

    /// Renders the tree. `prefix` is the path accumulated so far (for folder ids
    /// and the `+` target); `to_load` records a clicked contract; `new_in`
    /// records a folder's path (with trailing `/`) when its `+` is clicked.
    #[allow(clippy::too_many_arguments)]
    fn show(
        &self,
        ui: &mut egui::Ui,
        prefix: &str,
        selected: Option<usize>,
        to_load: &mut Option<usize>,
        new_in: &mut Option<String>,
        // (relative path, is_folder) of an item whose `x` was clicked.
        delete: &mut Option<(String, bool)>,
    ) {
        for (name, child) in &self.dirs {
            let folder_path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let id = ui.make_persistent_id(("tree", &folder_path));
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
                .show_header(ui, |ui| {
                    // Trailing buttons are reserved first (right-to-left) so the
                    // folder name truncates to the remaining width rather than
                    // forcing the side panel wider than its dragged width.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button(RichText::new("-").color(DIM))
                            .on_hover_text("Delete this folder")
                            .clicked()
                        {
                            *delete = Some((folder_path.clone(), true));
                        }
                        if ui
                            .small_button(RichText::new("+").color(GREEN))
                            .on_hover_text("New request in this folder")
                            .clicked()
                        {
                            *new_in = Some(format!("{folder_path}/"));
                        }
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(RichText::new(name).color(DIM)).truncate());
                        });
                    });
                })
                .body(|ui| child.show(ui, &folder_path, selected, to_load, new_in, delete));
        }
        for (label, idx, method, invalid) in &self.files {
            let rel = if prefix.is_empty() {
                label.clone()
            } else {
                format!("{prefix}/{label}")
            };
            ui.horizontal(|ui| {
                if *invalid {
                    ui.label(RichText::new("●").color(RED))
                        .on_hover_text("Invalid contract, click to repair");
                }
                ui.label(RichText::new(method).color(method_color(method)).size(11.0));
                // Reserve the delete button on the right, then let the file name
                // truncate into whatever width is left. Without truncation a long
                // name measures wider than the panel, and egui stores that as the
                // panel width every frame — blocking resize below the longest name.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button(RichText::new("-").color(DIM))
                        .on_hover_text("Delete this contract")
                        .clicked()
                    {
                        *delete = Some((rel.clone(), false));
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let file = egui::Button::selectable(
                            selected == Some(*idx),
                            RichText::new(label).color(TEXT),
                        )
                        .truncate();
                        if ui.add(file).clicked() {
                            *to_load = Some(*idx);
                        }
                    });
                });
            });
        }
    }
}
