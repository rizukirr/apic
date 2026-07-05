//! The panelled editor/viewer sections (endpoint, parameters, headers, request
//! body, responses). Each takes the shared [`EditModel`] and an `editing` flag
//! and renders the read or edit variant, composing the widgets in
//! [`super::widgets`]. Editing behavior itself lives in [`apic_core::edit`];
//! these functions only translate clicks into [`EditAction`]s.

use eframe::egui;
use egui::RichText;

use apic_core::edit::{BodyLoc, EditAction, EditModel, EditSchema, Field, apply};
use apic_core::json::method_str;

use crate::ui::theme::{AMBER, SPACE_LARGE};

use super::theme::{
    CYAN, DIM, GREEN, RED, SPACE_MEDIUM, SPACE_SMALL, TEXT, method_badge, method_color,
};
use super::widgets::{
    SCHEMA_TYPES, add_button, cell_edit, cell_key, cell_text, chip_cell, delete_button,
    json_editor, request_new_row_focus, section_label, table_frame, table_header,
    take_pending_focus, type_dropdown,
};

// egui temp-data keys for the "focus the new row's name field" markers, one per
// editable list. Schema lists also append the body location so request and
// response schemas never claim each other's pending focus.
const FOCUS_QUERY: &str = "apic.focus.query";
const FOCUS_HEADER: &str = "apic.focus.header";
const FOCUS_SCHEMA: &str = "apic.focus.schema";

/// Compact editable name + description line, shown above the method/url row.
pub(crate) fn endpoint_header(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    if editing {
        ui.add(
            egui::TextEdit::singleline(&mut model.name)
                .frame(false)
                .hint_text("name")
                .font(egui::TextStyle::Heading)
                .text_color(TEXT)
                .desired_width(f32::INFINITY),
        );
        ui.add(
            egui::TextEdit::multiline(&mut model.description)
                .frame(false)
                .hint_text("description")
                .text_color(DIM)
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        );
    } else {
        ui.label(RichText::new(&model.name).color(TEXT).heading());
        if !model.description.is_empty() {
            ui.label(RichText::new(&model.description).color(DIM));
        }
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
        table_header(
            ui,
            &[
                ("key", 130.0),
                ("requirement", 90.0),
                ("value", 130.0),
                ("description", f32::INFINITY),
            ],
        );
        for i in 0..model.query.len() {
            ui.horizontal(|ui| {
                if editing {
                    let name = cell_edit(ui, &mut model.query[i].name, 130.0, "name");
                    take_pending_focus(ui, FOCUS_QUERY, i, &name);
                    if chip_cell(ui, model.query[i].required, true, 90.0).is_some() {
                        actions.push(EditAction::ToggleBool {
                            field: Field::QueryRequired(i),
                        });
                    }
                    cell_edit(ui, &mut model.query[i].value, 130.0, "value");
                    let w = (ui.available_width() - 24.0).max(60.0);
                    cell_edit(ui, &mut model.query[i].description, w, "description");
                    if delete_button(ui) {
                        actions.push(EditAction::Delete {
                            field: Field::QueryName(i),
                        });
                    }
                } else {
                    cell_key(ui, &model.query[i].name, 130.0);
                    chip_cell(ui, model.query[i].required, false, 90.0);
                    cell_text(ui, &model.query[i].value, 130.0);
                    cell_text(ui, &model.query[i].description, ui.available_width());
                }
            });
        }
        if editing && add_button(ui, "+ query") {
            request_new_row_focus(ui, FOCUS_QUERY, model.query.len());
            actions.push(EditAction::Add {
                field: Field::QueryAdd,
            });
        }
    });
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
        table_header(
            ui,
            &[
                ("header", 150.0),
                ("requirement", 90.0),
                ("value", f32::INFINITY),
            ],
        );
        let len = model.responses[idx].headers.len();
        for i in 0..len {
            ui.horizontal(|ui| {
                if editing {
                    cell_edit(ui, &mut model.responses[idx].headers[i].name, 150.0, "name");
                    if let Some(new) =
                        chip_cell(ui, model.responses[idx].headers[i].required, true, 90.0)
                    {
                        model.responses[idx].headers[i].required = new;
                    }
                    let w = (ui.available_width() - 24.0).max(60.0);
                    cell_edit(ui, &mut model.responses[idx].headers[i].value, w, "value");
                    if delete_button(ui) {
                        actions.push(EditAction::Delete {
                            field: Field::ResponseHeaderName(idx, i),
                        });
                    }
                } else {
                    cell_key(ui, &model.responses[idx].headers[i].name, 150.0);
                    chip_cell(ui, model.responses[idx].headers[i].required, false, 90.0);
                    cell_text(
                        ui,
                        &model.responses[idx].headers[i].value,
                        ui.available_width(),
                    );
                }
            });
        }
        if editing && add_button(ui, "+ header") {
            actions.push(EditAction::Add {
                field: Field::ResponseHeaderAdd(idx),
            });
        }
    });
    for a in &actions {
        apply(model, a);
    }
}

pub(crate) fn headers(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    let mut actions: Vec<EditAction> = Vec::new();
    table_frame(ui, |ui| {
        table_header(
            ui,
            &[
                ("header", 150.0),
                ("requirement", 90.0),
                ("value", f32::INFINITY),
            ],
        );
        for i in 0..model.headers.len() {
            ui.horizontal(|ui| {
                if editing {
                    let name = cell_edit(ui, &mut model.headers[i].name, 150.0, "name");
                    take_pending_focus(ui, FOCUS_HEADER, i, &name);
                    if chip_cell(ui, model.headers[i].required, true, 90.0).is_some() {
                        actions.push(EditAction::ToggleBool {
                            field: Field::HeaderRequired(i),
                        });
                    }
                    let w = (ui.available_width() - 24.0).max(60.0);
                    cell_edit(ui, &mut model.headers[i].value, w, "value");
                    if delete_button(ui) {
                        actions.push(EditAction::Delete {
                            field: Field::HeaderName(i),
                        });
                    }
                } else {
                    cell_key(ui, &model.headers[i].name, 150.0);
                    chip_cell(ui, model.headers[i].required, false, 90.0);
                    cell_text(ui, &model.headers[i].value, ui.available_width());
                }
            });
        }
        if editing && add_button(ui, "+ header") {
            request_new_row_focus(ui, FOCUS_HEADER, model.headers.len());
            actions.push(EditAction::Add {
                field: Field::HeaderAdd,
            });
        }
    });
    for a in &actions {
        apply(model, a);
    }
}

/// Renders a single view-mode schema field as
/// `name: type [REQUIRED]/[OPTIONAL] desc`, used by the request/response schema
/// viewer.
pub(crate) fn field_view_row(
    ui: &mut egui::Ui,
    name: &str,
    dtype: &str,
    required: bool,
    description: &str,
    depth: usize,
) {
    ui.horizontal(|ui| {
        let indent = depth as f32 * 14.0;
        ui.add_space(indent);
        cell_key(ui, name, (150.0 - indent).max(60.0));
        cell_text(ui, dtype, 90.0);
        chip_cell(ui, required, false, 90.0);
        cell_text(ui, description, ui.available_width());
    });
}

/// Renders schema fields as `name: type [REQUIRED]`, recursing into properties.
pub(crate) fn schema_fields(ui: &mut egui::Ui, fields: &[EditSchema], depth: usize) {
    for f in fields {
        field_view_row(ui, &f.name, &f.dtype, f.required, &f.description, depth);
        if !f.properties.is_empty() {
            schema_fields(ui, &f.properties, depth + 1);
        }
    }
}

/// Per-location temp-data key for schema focus, so the request schema and each
/// response schema never claim each other's pending new-row focus.
fn schema_focus_key(loc: &BodyLoc) -> String {
    match loc {
        BodyLoc::Request => format!("{FOCUS_SCHEMA}.req"),
        BodyLoc::Response(n) => format!("{FOCUS_SCHEMA}.resp{n}"),
    }
}

/// Stable string identity for a (possibly nested) schema field, used to match a
/// rendered row against the pending focus target.
fn schema_path_id(path: &[usize]) -> String {
    path.iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join("/")
}

/// Renders an add-field button; on click it marks the new row for focus and
/// queues a `SchemaAdd` under `parent` (the object's path, empty for the root).
/// `child_count` is how many fields the object already has, i.e. the index the
/// new field will land at.
fn schema_add_button(
    ui: &mut egui::Ui,
    label: &str,
    loc: &BodyLoc,
    parent: &[usize],
    child_count: usize,
    actions: &mut Vec<EditAction>,
) {
    if add_button(ui, label) {
        let mut child = parent.to_vec();
        child.push(child_count);
        request_new_row_focus(ui, &schema_focus_key(loc), schema_path_id(&child));
        actions.push(EditAction::Add {
            field: Field::SchemaAdd(loc.clone(), parent.to_vec()),
        });
    }
}

/// Edit-mode schema editor: binds name/type/required directly and collects
/// structural add/delete edits into `actions` (applied after the borrow ends).
/// Recurses into nested object `properties`; an object field gets a `+ field`
/// button at the bottom of its nested block.
pub(crate) fn edit_schema_fields(
    ui: &mut egui::Ui,
    loc: &BodyLoc,
    fields: &mut [EditSchema],
    path: &mut Vec<usize>,
    actions: &mut Vec<EditAction>,
) {
    for (i, f) in fields.iter_mut().enumerate() {
        path.push(i);
        ui.horizontal(|ui| {
            let indent = (path.len() as f32 - 1.0) * 14.0;
            ui.add_space(indent);
            let name = cell_edit(ui, &mut f.name, (150.0 - indent).max(60.0), "name");
            take_pending_focus(ui, &schema_focus_key(loc), schema_path_id(path), &name);
            let loc_tag = match loc {
                BodyLoc::Request => "req".to_string(),
                BodyLoc::Response(n) => format!("resp{n}"),
            };
            type_dropdown(
                ui,
                ("schema_type", &loc_tag, path.as_slice()),
                &mut f.dtype,
                SCHEMA_TYPES,
            );
            if let Some(new) = chip_cell(ui, f.required, true, 90.0) {
                f.required = new;
            }
            let width = (ui.available_width() - 24.0).max(60.0);
            cell_edit(ui, &mut f.description, width, "description");
            if delete_button(ui) {
                actions.push(EditAction::Delete {
                    field: Field::SchemaName(loc.clone(), path.clone()),
                });
            }
        });
        if !f.properties.is_empty() {
            edit_schema_fields(ui, loc, &mut f.properties, path, actions);
        }
        if apic_core::json::parse_type(&f.dtype).0 == "object" {
            ui.horizontal(|ui| {
                ui.add_space(path.len() as f32 * 14.0);
                schema_add_button(
                    ui,
                    "+ field",
                    loc,
                    path.as_slice(),
                    f.properties.len(),
                    actions,
                );
            });
        }
        path.pop();
    }
}

pub(crate) fn request_body(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    ui.spacing_mut().item_spacing.y = SPACE_MEDIUM;
    let mut actions: Vec<EditAction> = Vec::new();
    if let Some(req) = model.request.as_mut() {
        if editing {
            ui.horizontal(|ui| {
                if ui
                    .button(RichText::new(format!("type: {}", req.dtype)).color(CYAN))
                    .clicked()
                {
                    actions.push(EditAction::ToggleBodyType {
                        loc: BodyLoc::Request,
                    });
                }
                if ui.button(RichText::new("remove body").color(RED)).clicked() {
                    actions.push(EditAction::Add {
                        field: Field::RequestToggle,
                    });
                }
            });
        }
        ui.add_space(SPACE_LARGE);
        egui::CollapsingHeader::new(RichText::new("Schema").color(DIM))
            .default_open(false)
            .show(ui, |ui| {
                // Cap the schema list at half the remaining height so a large
                // schema scrolls internally instead of pushing the stretched
                // example off the bottom of the (unscrolled) tab.
                let schema_cap = (ui.available_height() * 0.5).max(120.0);
                egui::ScrollArea::vertical()
                    .id_salt("req_schema_scroll")
                    .max_height(schema_cap)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        table_frame(ui, |ui| {
                            table_header(
                                ui,
                                &[
                                    ("name", 150.0),
                                    ("type", 90.0),
                                    ("requirement", 90.0),
                                    ("description", f32::INFINITY),
                                ],
                            );
                            if editing {
                                let mut path = Vec::new();
                                edit_schema_fields(
                                    ui,
                                    &BodyLoc::Request,
                                    &mut req.schema,
                                    &mut path,
                                    &mut actions,
                                );
                                schema_add_button(
                                    ui,
                                    "+ field",
                                    &BodyLoc::Request,
                                    &[],
                                    req.schema.len(),
                                    &mut actions,
                                );
                            } else if req.schema.is_empty() {
                                ui.label(RichText::new("(none)").color(DIM));
                            } else {
                                schema_fields(ui, &req.schema, 0);
                            }
                        });
                    });
            });
        ui.add_space(SPACE_LARGE);
        ui.horizontal(|ui| {
            section_label(ui, "EXAMPLE");
            ui.spacing_mut().item_spacing.x = SPACE_MEDIUM;
            ui.add_space(SPACE_MEDIUM);
            if editing {
                if ui
                    .button(RichText::new("generate from schema").color(GREEN))
                    .clicked()
                {
                    actions.push(EditAction::GenerateExample {
                        loc: BodyLoc::Request,
                    });
                }

                if ui
                    .button(RichText::new("generate schema from example").color(CYAN))
                    .clicked()
                {
                    actions.push(EditAction::InferSchema {
                        loc: BodyLoc::Request,
                    });
                }

                if ui.button(RichText::new("pretty").color(AMBER)).clicked() {
                    req.example = apic_core::json::pretty_json(&req.example);
                }
            }
        });
        let h = ui.available_height().max(160.0);
        json_editor(ui, &mut req.example, editing, h);
    } else {
        ui.label(RichText::new("(no request body)").color(DIM));
        if editing && add_button(ui, "+ request body") {
            actions.push(EditAction::Add {
                field: Field::RequestToggle,
            });
        }
    }
    for a in &actions {
        apply(model, a);
    }
    if let Some((BodyLoc::Request, err)) = &model.last_error {
        ui.label(RichText::new(err.as_str()).color(RED));
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
    ui.horizontal_wrapped(|ui| {
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
                // The active tab's code is edited in place, no bordered box.
                ui.add(
                    egui::TextEdit::singleline(&mut model.responses[i].code)
                        .frame(false)
                        .desired_width(46.0)
                        .text_color(color),
                );
                if delete_button(ui) {
                    actions.push(EditAction::Delete {
                        field: Field::ResponseCode(i),
                    });
                }
            } else {
                let label = if model.responses[i].code.is_empty() {
                    "?".to_string()
                } else {
                    model.responses[i].code.clone()
                };
                if ui
                    .selectable_label(selected, RichText::new(label).color(color).strong())
                    .clicked()
                {
                    *resp_tab = i;
                }
            }
        }
        if editing && add_button(ui, "+ new response") {
            actions.push(EditAction::Add {
                field: Field::ResponseAdd,
            });
        }
    });
    for a in &actions {
        apply(model, a);
    }
}

/// Renders the selected response's description, schema, and example. The caller
/// (the Response tab) has already drawn the code-tab strip and guarantees `idx`
/// is a valid response index.
pub(crate) fn response_body(ui: &mut egui::Ui, model: &mut EditModel, idx: usize, editing: bool) {
    let mut actions: Vec<EditAction> = Vec::new();
    ui.spacing_mut().item_spacing.y = SPACE_SMALL;
    let r = &mut model.responses[idx];
    // Description: inline, frameless in edit mode; plain text otherwise.
    if editing {
        ui.add(
            egui::TextEdit::singleline(&mut r.description)
                .frame(false)
                .hint_text("description")
                .text_color(DIM)
                .desired_width(f32::INFINITY),
        );
    } else if !r.description.is_empty() {
        ui.label(RichText::new(&r.description).color(DIM));
    }
    egui::CollapsingHeader::new(RichText::new("Schema").color(DIM))
        .default_open(false)
        .show(ui, |ui| {
            // Cap the schema list at half the remaining height so a large schema
            // scrolls internally instead of pushing the stretched example off the
            // bottom.
            let schema_cap = (ui.available_height() * 0.5).max(120.0);
            egui::ScrollArea::vertical()
                .id_salt("resp_schema_scroll")
                .max_height(schema_cap)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    table_frame(ui, |ui| {
                        table_header(
                            ui,
                            &[
                                ("name", 150.0),
                                ("type", 90.0),
                                ("requirement", 90.0),
                                ("description", f32::INFINITY),
                            ],
                        );
                        if editing {
                            let mut path = Vec::new();
                            edit_schema_fields(
                                ui,
                                &BodyLoc::Response(idx),
                                &mut r.schema,
                                &mut path,
                                &mut actions,
                            );
                            schema_add_button(
                                ui,
                                "+ field",
                                &BodyLoc::Response(idx),
                                &[],
                                r.schema.len(),
                                &mut actions,
                            );
                        } else if r.schema.is_empty() {
                            ui.label(RichText::new("(none)").color(DIM));
                        } else {
                            schema_fields(ui, &r.schema, 0);
                        }
                    });
                });
        });
    ui.add_space(SPACE_LARGE);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = SPACE_MEDIUM;
        section_label(ui, "EXAMPLE");
        ui.add_space(SPACE_MEDIUM);
        if editing {
            if ui
                .button(RichText::new(format!("type: {}", r.dtype)).color(CYAN))
                .clicked()
            {
                actions.push(EditAction::ToggleBodyType {
                    loc: BodyLoc::Response(idx),
                });
            }
            if ui
                .button(RichText::new("generate from schema").color(GREEN))
                .clicked()
            {
                actions.push(EditAction::GenerateExample {
                    loc: BodyLoc::Response(idx),
                });
            }
            if ui
                .button(RichText::new("generate schema from example").color(CYAN))
                .clicked()
            {
                actions.push(EditAction::InferSchema {
                    loc: BodyLoc::Response(idx),
                });
            }
            if ui.button(RichText::new("pretty").color(AMBER)).clicked() {
                r.example = apic_core::json::pretty_json(&r.example);
            }
        }
    });
    let h = ui.available_height().max(160.0);
    json_editor(ui, &mut r.example, editing, h);

    for a in &actions {
        apply(model, a);
    }

    if let Some((BodyLoc::Response(i), err)) = &model.last_error
        && *i == idx
    {
        ui.label(RichText::new(err.as_str()).color(RED));
    }
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
            endpoint_header(ui, &mut m, true);
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
