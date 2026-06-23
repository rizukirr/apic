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
    BG, CYAN, DIM, GREEN, RED, SPACE_EXTRA_SMALL, SPACE_MEDIUM, SPACE_SMALL, TEXT, method_badge,
    method_color,
};
use super::widgets::{
    PARAM_TYPES, SCHEMA_TYPES, add_button, bordered_input, bordered_input_colored, code_block,
    delete_button, json_block, kv_row, panel, request_new_row_focus, section_label,
    take_pending_focus, type_dropdown, weighted_columns,
};

// egui temp-data keys for the "focus the new row's name field" markers, one per
// editable list. Schema lists also append the body location so request and
// response schemas never claim each other's pending focus.
const FOCUS_QUERY: &str = "apic.focus.query";
const FOCUS_VARIABLE: &str = "apic.focus.variable";
const FOCUS_HEADER: &str = "apic.focus.header";
const FOCUS_SCHEMA: &str = "apic.focus.schema";

/// Fraction of the schema/example row given to the schema column; the example
/// preview takes the remaining ~30%.
const SCHEMA_COL_FRAC: f32 = 0.5;

/// Renders the full URL the way `apic read`/TUI do, so the GUI never drifts.
pub(crate) fn build_url(model: &EditModel) -> String {
    apic_core::json::build_url(&model.url.protocol, &model.url.host, &model.url.path)
}

pub(crate) fn endpoint_info(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    panel(ui, "ENDPOINT_INFO", 0.0, |ui| {
        ui.spacing_mut().item_spacing.y = SPACE_MEDIUM;
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            if editing {
                if ui
                    .button(
                        RichText::new(method_str(&model.method))
                            .color(method_color(&method_str(&model.method))),
                    )
                    .clicked()
                {
                    apply(model, &EditAction::CycleMethod { forward: true });
                }
            } else {
                method_badge(ui, &method_str(&model.method));
            }
            ui.add_space(SPACE_MEDIUM);
            if editing {
                bordered_input(ui, &mut model.url.protocol, 54.0, "");
                ui.label(RichText::new("://").color(DIM));
                bordered_input(ui, &mut model.url.host, f32::INFINITY, "host");
            } else {
                ui.label(RichText::new(build_url(model)).color(CYAN).strong());
            }
        });
        ui.add_space(SPACE_EXTRA_SMALL);
        if editing {
            let mut actions: Vec<EditAction> = Vec::new();
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = SPACE_MEDIUM;
                ui.label(RichText::new("path").color(DIM));
                let mut del = None;
                for i in 0..model.url.path.len() {
                    ui.label(RichText::new("/").color(DIM));
                    bordered_input(ui, &mut model.url.path[i], 80.0, "");
                    if delete_button(ui) {
                        del = Some(i);
                    }
                }
                if add_button(ui, "+ segment") {
                    actions.push(EditAction::Add {
                        field: Field::PathAdd,
                    });
                }
                if let Some(i) = del {
                    actions.push(EditAction::Delete {
                        field: Field::PathSeg(i),
                    });
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("name").color(DIM));
                bordered_input(ui, &mut model.name, f32::INFINITY, "name");
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("desc").color(DIM));
                bordered_input(ui, &mut model.description, f32::INFINITY, "description");
            });
            for a in &actions {
                apply(model, a);
            }
        } else {
            ui.label(RichText::new(&model.name).color(TEXT).strong());
            if !model.description.is_empty() {
                ui.label(RichText::new(&model.description).color(DIM));
            }
        }
    });
}

/// Body of the PARAMETERS panel (wrapped by `panel` at the call site).
pub(crate) fn parameters(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    let mut actions: Vec<EditAction> = Vec::new();

    let space = ui.spacing().item_spacing.y;
    ui.spacing_mut().item_spacing.y = SPACE_MEDIUM;
    section_label(ui, "QUERY PARAMS");
    if model.url.query.is_empty() && !editing {
        ui.label(RichText::new("(none)").color(DIM));
    }
    for i in 0..model.url.query.len() {
        if editing {
            ui.horizontal(|ui| {
                let q = &mut model.url.query[i];
                let name = bordered_input(ui, &mut q.name, 90.0, "name");
                take_pending_focus(ui, FOCUS_QUERY, i, &name);
                type_dropdown(ui, ("query_type", i), &mut q.dtype, PARAM_TYPES);
                ui.checkbox(&mut q.required, RichText::new("req").color(DIM));
                if delete_button(ui) {
                    actions.push(EditAction::Delete {
                        field: Field::QueryName(i),
                    });
                }
            });
        } else {
            let q = &model.url.query[i];
            field_view_row(ui, &q.name, &q.dtype, q.required, &q.description, 0);
        }
    }
    if editing && add_button(ui, "+ query") {
        request_new_row_focus(ui, FOCUS_QUERY, model.url.query.len());
        actions.push(EditAction::Add {
            field: Field::QueryAdd,
        });
    }

    ui.add_space(SPACE_LARGE);
    section_label(ui, "PATH VARIABLES");
    if model.url.variable.is_empty() && !editing {
        ui.label(RichText::new("(none)").color(DIM));
    }
    for i in 0..model.url.variable.len() {
        if editing {
            ui.horizontal(|ui| {
                let v = &mut model.url.variable[i];
                let name = bordered_input(ui, &mut v.name, 90.0, "name");
                take_pending_focus(ui, FOCUS_VARIABLE, i, &name);
                type_dropdown(ui, ("var_type", i), &mut v.dtype, PARAM_TYPES);
                ui.checkbox(&mut v.required, RichText::new("req").color(DIM));
                if delete_button(ui) {
                    actions.push(EditAction::Delete {
                        field: Field::VarName(i),
                    });
                }
            });
        } else {
            let v = &model.url.variable[i];
            field_view_row(ui, &v.name, &v.dtype, v.required, &v.description, 0);
        }
    }
    if editing && add_button(ui, "+ variable") {
        request_new_row_focus(ui, FOCUS_VARIABLE, model.url.variable.len());
        actions.push(EditAction::Add {
            field: Field::VarAdd,
        });
    }

    for a in &actions {
        apply(model, a);
    }

    ui.spacing_mut().item_spacing.y = space;
    ui.add_space(SPACE_MEDIUM);
}

pub(crate) fn headers(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    let space = ui.spacing().item_spacing.y;
    ui.spacing_mut().item_spacing.y = SPACE_MEDIUM;

    if model.headers.is_empty() {
        ui.label(RichText::new("(none)").color(DIM));
    }
    let mut delete = None;
    for i in 0..model.headers.len() {
        if editing {
            ui.horizontal(|ui| {
                let name = bordered_input(ui, &mut model.headers[i].name, 130.0, "name");
                take_pending_focus(ui, FOCUS_HEADER, i, &name);
                let gap = ui.spacing().item_spacing.x;
                let reserve = 18.0 + 24.0 + gap;
                let value_w = (ui.available_width() - reserve).max(40.0);
                bordered_input(ui, &mut model.headers[i].value, value_w, "value");
                if delete_button(ui) {
                    delete = Some(Field::HeaderName(i));
                }
            });
        } else {
            kv_row(ui, &model.headers[i].name, &model.headers[i].value, GREEN);
        }
    }
    if let Some(field) = delete {
        apply(model, &EditAction::Delete { field });
    }
    if editing && add_button(ui, "+ header") {
        request_new_row_focus(ui, FOCUS_HEADER, model.headers.len());
        apply(
            model,
            &EditAction::Add {
                field: Field::HeaderAdd,
            },
        );
    }

    ui.spacing_mut().item_spacing.y = space;
    ui.add_space(SPACE_MEDIUM);
}

/// Renders a single view-mode field as `name: type [REQUIRED]/[OPTIONAL] desc`.
/// Shared by the schema viewer and the query/path-variable viewers so query
/// params and path variables read identically to request/response fields.
pub(crate) fn field_view_row(
    ui: &mut egui::Ui,
    name: &str,
    dtype: &str,
    required: bool,
    description: &str,
    depth: usize,
) {
    ui.horizontal(|ui| {
        ui.add_space(depth as f32 * 14.0);
        ui.label(RichText::new(format!("{name}:")).color(TEXT));
        ui.label(RichText::new(dtype).color(CYAN));
        if required {
            ui.label(
                RichText::new(" REQUIRED ")
                    .color(BG)
                    .background_color(RED)
                    .size(10.0),
            );
        } else {
            ui.label(RichText::new("[OPTIONAL]").color(DIM).size(10.0));
        }
        if !description.is_empty() {
            ui.label(RichText::new(description).color(DIM).size(11.0));
        }
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
            ui.add_space((path.len() as f32 - 1.0) * 14.0);
            let name = bordered_input(ui, &mut f.name, 110.0, "name");
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
            ui.checkbox(&mut f.required, RichText::new("req").color(DIM));
            bordered_input(ui, &mut f.description, 160.0, "description");
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
    panel(ui, "REQUEST_BODY", 0.0, |ui| {
        ui.spacing_mut().item_spacing.y = SPACE_MEDIUM;
        let mut actions: Vec<EditAction> = Vec::new();
        if let Some(req) = model.request.as_mut() {
            weighted_columns(ui, SCHEMA_COL_FRAC, |cols| {
                if editing {
                    cols[0].horizontal(|ui| {
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
                section_label(&mut cols[0], "SCHEMA DEFINITION");
                if editing {
                    let mut path = Vec::new();
                    edit_schema_fields(
                        &mut cols[0],
                        &BodyLoc::Request,
                        &mut req.schema,
                        &mut path,
                        &mut actions,
                    );
                    schema_add_button(
                        &mut cols[0],
                        "+ field",
                        &BodyLoc::Request,
                        &[],
                        req.schema.len(),
                        &mut actions,
                    );
                } else if req.schema.is_empty() {
                    cols[0].label(RichText::new("(none)").color(DIM));
                } else {
                    schema_fields(&mut cols[0], &req.schema, 0);
                }
                cols[1].horizontal(|ui| {
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

                        if ui.button(RichText::new("pretty").color(AMBER)).clicked() {
                            req.example = apic_core::json::pretty_json(&req.example);
                        }
                    }
                });
                if editing {
                    code_block(&mut cols[1], &mut req.example);
                } else {
                    json_block(&mut cols[1], &req.example);
                }
            });
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
        ui.add_space(SPACE_MEDIUM);
    });
}

pub(crate) fn responses(
    ui: &mut egui::Ui,
    model: &mut EditModel,
    resp_tab: &mut usize,
    editing: bool,
) {
    panel(ui, "RESPONSES", 0.0, |ui| {
        let mut actions: Vec<EditAction> = Vec::new();

        // Tabs (switch between response codes), plus the edit-only `+ response`
        // button. Shown in preview mode too so the tabs stay clickable; only the
        // add button is gated on `editing`.
        if editing || !model.responses.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for (i, r) in model.responses.iter().enumerate() {
                    let label = format!("[ {} ]", if r.code.is_empty() { "?" } else { &r.code });
                    // Flag a response whose code is not a valid number so the user can
                    // see which tab is blocking the save.
                    let color = if r.code.trim().parse::<u16>().is_err() {
                        RED
                    } else if i == *resp_tab {
                        GREEN
                    } else {
                        DIM
                    };
                    if ui
                        .selectable_label(i == *resp_tab, RichText::new(label).color(color))
                        .clicked()
                    {
                        *resp_tab = i;
                    }
                }
                if editing && add_button(ui, "+ response") {
                    actions.push(EditAction::Add {
                        field: Field::ResponseAdd,
                    });
                }
            });
        }

        if model.responses.is_empty() {
            ui.label(RichText::new("(no responses)").color(DIM));
            for a in &actions {
                apply(model, a);
            }
            ui.add_space(SPACE_MEDIUM);
            return;
        }
        if *resp_tab >= model.responses.len() {
            *resp_tab = 0;
        }
        ui.separator();

        let idx = *resp_tab;
        let r = &mut model.responses[idx];
        if editing {
            ui.horizontal(|ui| {
                let code_ok = r.code.trim().parse::<u16>().is_ok();
                let box_h = ui.text_style_height(&egui::TextStyle::Body) + 10.0;
                ui.add_sized(
                    [34.0, box_h],
                    egui::Label::new(RichText::new("code").color(if code_ok { DIM } else { RED })),
                );
                bordered_input_colored(ui, &mut r.code, 60.0, "", !code_ok);
                ui.label(RichText::new("desc").color(DIM));
                bordered_input(ui, &mut r.description, f32::INFINITY, "description");
            });
            ui.horizontal(|ui| {
                if ui
                    .button(RichText::new(format!("type: {}", r.dtype)).color(CYAN))
                    .clicked()
                {
                    actions.push(EditAction::ToggleBodyType {
                        loc: BodyLoc::Response(idx),
                    });
                }
                if ui
                    .button(RichText::new("delete response").color(RED))
                    .clicked()
                {
                    actions.push(EditAction::Delete {
                        field: Field::ResponseCode(idx),
                    });
                }
            });
        }
        section_label(ui, "RESPONSE SCHEMA");
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
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = SPACE_MEDIUM;
            section_label(ui, "EXAMPLE");
            if editing {
                if ui
                    .button(RichText::new("generate from schema").color(GREEN))
                    .clicked()
                {
                    actions.push(EditAction::GenerateExample {
                        loc: BodyLoc::Response(idx),
                    });
                }
                if ui.button(RichText::new("pretty").color(AMBER)).clicked() {
                    r.example = apic_core::json::pretty_json(&r.example);
                }
            }
        });
        if editing {
            code_block(ui, &mut r.example);
        } else {
            json_block(ui, &r.example);
        }

        for a in &actions {
            apply(model, a);
        }

        ui.add_space(SPACE_MEDIUM);
    });
}
