//! The panelled editor/viewer sections (endpoint, parameters, headers, request
//! body, responses). Each takes the shared [`EditModel`] and an `editing` flag
//! and renders the read or edit variant, composing the widgets in
//! [`super::widgets`]. Editing behavior itself lives in [`apic_core::edit`];
//! these functions only translate clicks into [`EditAction`]s.

use eframe::egui;
use egui::RichText;

use apic_core::edit::{EditAction, EditBody, EditModel, Field, apply};
use apic_core::json::method_str;

use crate::ui::theme::AMBER;

use super::theme::{
    CYAN, DIM, GREEN, RED, SPACE_MEDIUM, SPACE_SMALL, TEXT, method_badge, method_color,
};
use super::widgets::{
    TABLE_HEADER_H, TABLE_ROW_H, add_button, delete_button, fill_column, header_label, json_editor,
    metadata_table, request_new_row_focus, required_chip, section_label, table_frame,
    take_pending_focus, tcell_edit, text_button,
};
use egui_extras::Column;

// egui temp-data keys for the "focus the new row's name field" markers, one per
// editable list.
const FOCUS_QUERY: &str = "apic.focus.query";
const FOCUS_HEADER: &str = "apic.focus.header";

/// The endpoint name, rendered inline on the toolbar row (left of EDIT/SAVE).
/// Editable frameless heading in edit mode; a heading label otherwise.
pub(crate) fn endpoint_name(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    if editing {
        ui.add(
            egui::TextEdit::singleline(&mut model.name)
                .frame(false)
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
pub(crate) fn endpoint_description(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    if editing {
        ui.add(
            egui::TextEdit::multiline(&mut model.description)
                .frame(false)
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
pub(crate) fn method_url_row(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    table_frame(ui, |ui| {
        ui.horizontal(|ui| {
            let method = method_str(&model.method);
            if editing {
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(format!(" {method} "))
                                .color(method_color(&method))
                                .strong(),
                        )
                        .frame(false),
                    )
                    .clicked()
                {
                    apply(model, &EditAction::CycleMethod { forward: true });
                }
            } else {
                method_badge(ui, &method);
            }
            ui.add_space(SPACE_MEDIUM);
            if editing {
                ui.add(
                    egui::TextEdit::singleline(&mut model.url)
                        .frame(false)
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
pub(crate) fn query_section(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
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
pub(crate) fn response_headers(
    ui: &mut egui::Ui,
    model: &mut EditModel,
    idx: usize,
    editing: bool,
) {
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

pub(crate) fn headers(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
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

pub(crate) fn request_body(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
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
pub(crate) fn response_code_selector(
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
                                .frame(false)
                                .desired_width(code_w + 10.0)
                                .text_color(color),
                        );
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
                                .frame(false)
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
pub(crate) fn response_body(ui: &mut egui::Ui, model: &mut EditModel, idx: usize, editing: bool) {
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
