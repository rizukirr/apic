//! The panelled contract editor/viewer sections (endpoint, parameters, headers,
//! request body, responses). Each takes the shared [`EditModel`] and an
//! `editing` flag and renders the read or edit variant, composing the
//! primitives in [`crate::ui::components`]. Editing behavior itself lives in
//! [`apic_core::edit`]; these functions only translate clicks into
//! [`EditAction`]s.

use eframe::egui;
use egui::{Color32, RichText};

use apic_core::edit::{EditAction, EditBody, EditModel, Field, apply};
use apic_core::json::method_str;

use crate::features::contracts::ui::state::ContractsAction;
use crate::ui::components::{
    add_button, delete_button, fill_column, header_label, json_editor, metadata_table,
    required_chip, section_label, table_frame, tcell_edit, tcell_edit_code, text_button, url_bar,
};
use crate::ui::focus::{request_new_row_focus, take_pending_focus};
use crate::ui::theme::*;
use egui_extras::Column;

/// Color for an HTTP method badge.
pub(super) fn method_color(method: &str, p: &Palette) -> Color32 {
    match method {
        "GET" | "HEAD" => p.method_get,
        "POST" => p.method_post,
        "PUT" | "PATCH" => p.method_put,
        "DELETE" => p.method_delete,
        _ => p.on_surface_variant,
    }
}

/// A filled, rounded, method-colored pill (read mode); the edit view uses a
/// dropdown instead so it can change the method.
fn method_badge(ui: &mut egui::Ui, method: &str) {
    let p = palette(ui.ctx());
    egui::Frame::NONE
        .fill(method_color(method, p))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(method).color(p.surface).strong());
        });
}

/// Lays out `url` in `font`, with every `{{...}}` variable span in
/// `syntax_number` and the rest in `on_surface`. A span runs from `{{` to the
/// next `}}` inclusive, and an unclosed `{{` runs to the end.
pub(super) fn url_layout_job(url: &str, p: &Palette, font: egui::FontId) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let format = |color| egui::TextFormat::simple(font.clone(), color);
    let mut rest = url;
    while !rest.is_empty() {
        let Some(open) = rest.find("{{") else {
            job.append(rest, 0.0, format(p.on_surface));
            break;
        };
        if open > 0 {
            job.append(&rest[..open], 0.0, format(p.on_surface));
        }
        let var = &rest[open..];
        let end = var.find("}}").map_or(var.len(), |close| close + 2);
        job.append(&var[..end], 0.0, format(p.syntax_number));
        rest = &var[end..];
    }
    job
}

// egui temp-data keys for the "focus the new row's name field" markers, one per
// editable list.
const FOCUS_QUERY: &str = "apic.focus.query";
const FOCUS_HEADER: &str = "apic.focus.header";

/// The endpoint name, rendered inline on the toolbar row (left of EDIT/SAVE).
/// Editable frameless heading in edit mode; a heading label otherwise.
pub(super) fn endpoint_name(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    let p = palette(ui.ctx());
    if editing {
        ui.add(
            egui::TextEdit::singleline(&mut model.name)
                .frame(egui::Frame::NONE)
                .hint_text("endpoint name")
                .font(egui::TextStyle::Heading)
                .text_color(p.on_surface)
                .desired_width(f32::INFINITY),
        );
    } else {
        ui.label(RichText::new(&model.name).color(p.on_surface).heading());
    }
}

/// The endpoint description, shown under the name/url row. Frameless multiline in
/// edit mode; dim text otherwise (hidden when empty in view mode).
pub(super) fn endpoint_description(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    let p = palette(ui.ctx());
    if editing {
        ui.add(
            egui::TextEdit::multiline(&mut model.description)
                .frame(egui::Frame::NONE)
                .hint_text("description")
                .text_color(p.on_surface_variant)
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        );
    } else if !model.description.is_empty() {
        ui.label(RichText::new(&model.description).color(p.on_surface_variant));
    }
}

/// The `[ METHOD ] [ url................ ]` top row.
pub(super) fn method_url_row(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    let p = palette(ui.ctx());
    url_bar(ui, |ui| {
        let method = method_str(&model.method);
        if editing {
            // Pick the HTTP method from a dropdown of all the choices.
            egui::ComboBox::from_id_salt("method")
                .width(90.0)
                .selected_text(
                    RichText::new(&method)
                        .color(method_color(&method, p))
                        .strong(),
                )
                .show_ui(ui, |ui| {
                    for m in apic_core::json::method_all() {
                        let name = method_str(&m);
                        let selected = name == method;
                        if ui
                            .selectable_label(
                                selected,
                                RichText::new(&name).color(method_color(&name, p)).strong(),
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
        let font = egui::TextStyle::Monospace.resolve(ui.style());
        if editing {
            let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
                let mut job = url_layout_job(buf.as_str(), p, font.clone());
                job.wrap.max_width = wrap_width;
                ui.fonts_mut(|f| f.layout_job(job))
            };
            ui.add(
                egui::TextEdit::singleline(&mut model.url)
                    .font(egui::TextStyle::Monospace)
                    .frame(egui::Frame::NONE)
                    .hint_text("https://host/path/{id}")
                    .layouter(&mut layouter)
                    .desired_width(f32::INFINITY),
            );
        } else {
            ui.label(url_layout_job(&model.url, p, font));
        }
    });
}

/// Body of the QUERY PARAMS section. Uses the flat `{name, value, description}`
/// query model; path variables now live inline in the URL string.
pub(super) fn query_section(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    let p = palette(ui.ctx());
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
                            ui.label(RichText::new(&model.query[i].name).color(p.info));
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
                            ui.label(RichText::new(&model.query[i].value).color(p.on_surface));
                        }
                    });
                    row.col(|ui| {
                        if editing {
                            tcell_edit(ui, &mut model.query[i].description, "description");
                        } else {
                            ui.label(
                                RichText::new(&model.query[i].description).color(p.on_surface),
                            );
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
pub(super) fn response_headers(
    ui: &mut egui::Ui,
    model: &mut EditModel,
    idx: usize,
    editing: bool,
) {
    let p = palette(ui.ctx());
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
                                RichText::new(&model.responses[idx].headers[i].name).color(p.info),
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
                                RichText::new(&model.responses[idx].headers[i].value)
                                    .color(p.on_surface),
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

pub(super) fn headers(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    let p = palette(ui.ctx());
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
                            ui.label(RichText::new(&model.headers[i].name).color(p.info));
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
                            tcell_edit_code(ui, &mut model.headers[i].value, "value");
                        } else {
                            ui.label(
                                RichText::new(&model.headers[i].value)
                                    .color(p.on_surface)
                                    .monospace(),
                            );
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

pub(super) fn request_body(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    let p = palette(ui.ctx());
    ui.spacing_mut().item_spacing.y = SPACE_MEDIUM;
    // A multipart body has no JSON example. Materializing an empty request
    // body here would let a keystroke create a second body that `to_json`
    // refuses to save, with nothing on screen explaining why.
    if !model.multipart.is_empty() {
        ui.label(
            RichText::new("multipart body, preserved on save, not editable here yet")
                .color(p.on_surface_variant),
        );
        ui.add_space(SPACE_MEDIUM);
        return;
    }
    if editing {
        // The request body is always editable — materialize an empty one so the
        // JSON editor is shown by default. An untouched (blank) body is dropped
        // on save (see `EditModel::to_json`), so this never pollutes contracts.
        let req = model.request.get_or_insert_with(EditBody::empty);
        ui.horizontal(|ui| {
            section_label(ui, "JSON");
            ui.spacing_mut().item_spacing.x = SPACE_MEDIUM;
            ui.add_space(SPACE_MEDIUM);
            if text_button(ui, "Pretty", p.warning) {
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

/// Color for an HTTP status code by its class: 2xx success, 3xx info, 4xx
/// warning, 5xx and anything that is not a three-digit code error.
pub(super) fn status_color(code: &str, p: &Palette) -> Color32 {
    if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return p.error;
    }
    match code.as_bytes()[0] {
        b'2' => p.success,
        b'3' => p.info,
        b'4' => p.warning,
        _ => p.error,
    }
}

/// The response-example dropdown: one entry per response, each labelled
/// `code - title` in its status color. In edit mode the list ends with a
/// "+ New example" entry, and the selected response's code and title are edited
/// inline next to the dropdown with an `x` to delete it. Selecting an entry sets
/// `resp_tab`, so the body and headers tabs below show that response.
///
/// # Returns
///
/// [`ContractsAction::OpenNewExample`] when "+ New example" was clicked.
pub(super) fn response_code_selector(
    ui: &mut egui::Ui,
    model: &mut EditModel,
    resp_tab: &mut usize,
    editing: bool,
) -> Option<ContractsAction> {
    let p = palette(ui.ctx());
    let mut actions: Vec<EditAction> = Vec::new();
    let mut action = None;
    // The selected response's code editor carries a stable id so the caret can
    // move to it the instant another response is selected.
    let code_edit_id = ui.make_persistent_id("resp_code_edit");
    let mut focus_code = false;
    ui.horizontal(|ui| {
        let selected_text = model
            .responses
            .get(*resp_tab)
            .map(|r| {
                RichText::new(status_tab_label(&r.code, &r.description))
                    .color(status_color(r.code.trim(), p))
                    .strong()
            })
            .unwrap_or_else(|| RichText::new("?").color(p.on_surface_variant));
        egui::ComboBox::from_id_salt("resp_example")
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                for (i, r) in model.responses.iter().enumerate() {
                    let label = RichText::new(status_tab_label(&r.code, &r.description))
                        .color(status_color(r.code.trim(), p));
                    if ui.selectable_label(i == *resp_tab, label).clicked() {
                        *resp_tab = i;
                        focus_code = true;
                    }
                }
                if editing {
                    ui.separator();
                    if ui.button("+ New example").clicked() {
                        action = Some(ContractsAction::OpenNewExample);
                        ui.close();
                    }
                }
            });
        if !editing {
            return;
        }
        let i = *resp_tab;
        if i >= model.responses.len() {
            return;
        }
        // The selected response is edited in place, no bordered box:
        // `[code] - [title] x`. Tight spacing so the fields read as one label.
        ui.spacing_mut().item_spacing.x = 4.0;
        let color = status_color(model.responses[i].code.trim(), p);
        // Size the code field to its text (min 3 digits) so the `-` hugs the
        // code instead of sitting past a fixed width.
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
        // A status code is exactly 3 digits: drop anything else and cap the
        // length as the user types.
        let code = &mut model.responses[i].code;
        if code.len() > 3 || !code.bytes().all(|b| b.is_ascii_digit()) {
            *code = code
                .chars()
                .filter(|c| c.is_ascii_digit())
                .take(3)
                .collect();
        }
        ui.label(RichText::new("-").color(p.on_surface_variant));
        // Size the title field to its text (30-char cap) so the `x` sits right
        // after the title instead of a fixed gap.
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
                .text_color(p.on_surface_variant),
        );
        if delete_button(ui) {
            actions.push(EditAction::Delete {
                field: Field::ResponseCode(i),
            });
        }
    });
    if editing && focus_code {
        ui.ctx().memory_mut(|m| m.request_focus(code_edit_id));
    }
    for a in &actions {
        apply(model, a);
    }
    action
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
pub(super) fn response_body(ui: &mut egui::Ui, model: &mut EditModel, idx: usize, editing: bool) {
    let p = palette(ui.ctx());
    ui.spacing_mut().item_spacing.y = SPACE_SMALL;
    let r = &mut model.responses[idx];
    // The title/description now lives in the status tab, so the body is just
    // the JSON example (with its Pretty control in edit mode).
    if editing {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = SPACE_MEDIUM;
            section_label(ui, "JSON");
            ui.add_space(SPACE_MEDIUM);
            if text_button(ui, "Pretty", p.warning) {
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
        // `run_in_theme` takes a closure, so build the model and tab index
        // fresh inside the closure rather than capturing them mutably.
        for preference in crate::ui::theme::BOTH_THEMES {
            crate::ui::theme::run_in_theme(preference, |ui| {
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

    #[test]
    fn url_layout_job_colours_variables_apart_from_the_path() {
        let p = &crate::ui::theme::TOKYO_NIGHT_STORM;
        let job = url_layout_job("{{baseUrl}}/auth/login", p, egui::FontId::monospace(12.0));
        let color_of = |needle: &str| {
            let start = job.text.find(needle).expect("needle is in the job text");
            let section = job
                .sections
                .iter()
                .find(|s| s.byte_range.contains(&egui::text::ByteIndex(start)))
                .expect("a section covers the needle");
            assert!(
                section.byte_range.end >= egui::text::ByteIndex(start + needle.len()),
                "{needle}"
            );
            section.format.color
        };
        assert_eq!(color_of("{{baseUrl}}"), p.syntax_number);
        assert_eq!(color_of("/auth/login"), p.on_surface);
    }

    #[test]
    fn status_color_follows_the_code_class() {
        let p = &crate::ui::theme::TOKYO_NIGHT_STORM;
        assert_eq!(status_color("200", p), p.success);
        assert_eq!(status_color("301", p), p.info);
        assert_eq!(status_color("404", p), p.warning);
        assert_eq!(status_color("500", p), p.error);
        assert_eq!(status_color("abc", p), p.error);
        assert_eq!(status_color("20", p), p.error);
    }

    #[test]
    fn the_request_editor_does_not_materialize_a_body_on_a_multipart_contract() {
        // `run_in_theme` takes a closure, so the model is built inside the
        // closure and asserted there too.
        for preference in crate::ui::theme::BOTH_THEMES {
            crate::ui::theme::run_in_theme(preference, |ui| {
                let c = apic_core::json::json_get(
                    r#"{ "name":"u","method":"POST","url":"https://h","headers":[],
                     "multipart":[{"name":"avatar","filename":"a.png","required":true}],
                     "responses":[] }"#,
                    None,
                )
                .unwrap();
                let mut m = EditModel::from_contract(c);
                assert!(!m.multipart.is_empty());
                request_body(ui, &mut m, true);
                // Materializing one here is what makes the file unsavable the
                // moment anything is typed into it.
                assert!(m.request.is_none());
            });
        }
    }
}
