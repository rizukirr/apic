//! The panelled contract editor/viewer sections (endpoint, parameters, headers,
//! request body, responses). Each takes the shared [`EditModel`] and an
//! `editing` flag and renders the read or edit variant, composing the
//! primitives in [`crate::ui::components`]. Editing behavior itself lives in
//! [`apic_core::edit`]; these functions only translate clicks into
//! [`EditAction`]s.

use eframe::egui;
use egui::{Color32, RichText};

use apic_core::edit::{BodyLoc, EditAction, EditBody, EditModel, EditPart, Field, apply};
use apic_core::json::method_str;

use crate::features::contracts::ui::state::ContractsAction;
use crate::ui::components::{
    delete_button, fill_column, grid_cell, header_label, json_editor, metadata_table,
    required_chip, section_label, table_frame, tcell_edit, tcell_edit_vars, text_button, url_bar,
    url_layout_job,
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

// egui temp-data keys for the "focus a new row's cell" markers, one per
// editable list.
const FOCUS_QUERY: &str = "apic.focus.query";
const FOCUS_HEADER: &str = "apic.focus.header";
const FOCUS_RESPONSE_HEADER: &str = "apic.focus.response_header";
const FOCUS_PARTS: &str = "apic.focus.parts";

/// Width of the delete column: the trash icon plus the grid cell's padding.
const DELETE_COL_W: f32 = 36.0;

/// An editable table that ends, while editing, in an empty add row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum RowTable {
    Query,
    Header,
    ResponseHeader(usize),
    /// The parts of the multipart request body.
    RequestParts,
    /// The parts of response `usize`'s multipart body.
    ResponseParts(usize),
}

/// A text column of a [`RowTable`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum RowColumn {
    Key,
    Name,
    Value,
    ContentType,
    Description,
}

impl RowColumn {
    /// The placeholder shown in an empty cell of this column.
    fn hint(self) -> &'static str {
        match self {
            RowColumn::Key => "Key",
            RowColumn::Name => "Name",
            RowColumn::Value => "Value",
            RowColumn::ContentType => "Content-Type",
            RowColumn::Description => "Description",
        }
    }
}

impl RowTable {
    /// The table's text columns, in display order.
    fn columns(self) -> &'static [RowColumn] {
        match self {
            RowTable::Query => &[RowColumn::Key, RowColumn::Value, RowColumn::Description],
            RowTable::Header | RowTable::ResponseHeader(_) => &[RowColumn::Key, RowColumn::Value],
            RowTable::RequestParts | RowTable::ResponseParts(_) => &[
                RowColumn::Name,
                RowColumn::Value,
                RowColumn::ContentType,
                RowColumn::Description,
            ],
        }
    }

    /// Whether the add row draws an empty non-text cell (a chip column)
    /// before text column `n`.
    fn spacer_before(self, n: usize) -> bool {
        match self {
            RowTable::Query | RowTable::Header | RowTable::ResponseHeader(_) => n == 1,
            RowTable::RequestParts | RowTable::ResponseParts(_) => n == 1 || n == 3,
        }
    }

    /// Whether value cells use the monospace font.
    fn monospace_values(self) -> bool {
        matches!(self, RowTable::Header | RowTable::ResponseHeader(_))
    }

    /// The temp-data key of the table's pending-focus marker.
    fn focus_key(self) -> &'static str {
        match self {
            RowTable::Query => FOCUS_QUERY,
            RowTable::Header => FOCUS_HEADER,
            RowTable::ResponseHeader(_) => FOCUS_RESPONSE_HEADER,
            RowTable::RequestParts | RowTable::ResponseParts(_) => FOCUS_PARTS,
        }
    }

    /// The field whose `Add` action appends an empty row to this table.
    fn add_field(self) -> Field {
        match self {
            RowTable::Query => Field::QueryAdd,
            RowTable::Header => Field::HeaderAdd,
            RowTable::ResponseHeader(r) => Field::ResponseHeaderAdd(r),
            RowTable::RequestParts => Field::PartAdd(BodyLoc::Request),
            RowTable::ResponseParts(r) => Field::PartAdd(BodyLoc::Response(r)),
        }
    }

    /// The number of rows the table holds in `model`.
    fn len(self, model: &EditModel) -> usize {
        match self {
            RowTable::Query => model.query.len(),
            RowTable::Header => model.headers.len(),
            RowTable::ResponseHeader(r) => model.responses.get(r).map_or(0, |r| r.headers.len()),
            RowTable::RequestParts => model.multipart.len(),
            RowTable::ResponseParts(r) => model.responses.get(r).map_or(0, |r| r.multipart.len()),
        }
    }
}

/// The parts `table` holds in `model`, or `None` for a table that is not a
/// parts table or a response that does not exist.
fn table_parts(model: &mut EditModel, table: RowTable) -> Option<&mut Vec<EditPart>> {
    match table {
        RowTable::RequestParts => Some(&mut model.multipart),
        RowTable::ResponseParts(r) => Some(&mut model.responses.get_mut(r)?.multipart),
        RowTable::Query | RowTable::Header | RowTable::ResponseHeader(_) => None,
    }
}

/// The text of `column` in row `row` of `table`, or `None` when the row or the
/// column does not exist.
fn cell_text_mut(
    model: &mut EditModel,
    table: RowTable,
    row: usize,
    column: RowColumn,
) -> Option<&mut String> {
    match table {
        RowTable::Query => {
            let q = model.query.get_mut(row)?;
            match column {
                RowColumn::Key => Some(&mut q.name),
                RowColumn::Value => Some(&mut q.value),
                RowColumn::Description => Some(&mut q.description),
                RowColumn::Name | RowColumn::ContentType => None,
            }
        }
        RowTable::Header | RowTable::ResponseHeader(_) => {
            let h = match table {
                RowTable::ResponseHeader(r) => model.responses.get_mut(r)?.headers.get_mut(row)?,
                _ => model.headers.get_mut(row)?,
            };
            match column {
                RowColumn::Key => Some(&mut h.name),
                RowColumn::Value => Some(&mut h.value),
                RowColumn::Name | RowColumn::ContentType | RowColumn::Description => None,
            }
        }
        RowTable::RequestParts | RowTable::ResponseParts(_) => {
            let part = table_parts(model, table)?.get_mut(row)?;
            match column {
                RowColumn::Name => Some(&mut part.name),
                // A file part's value column shows its filename.
                RowColumn::Value if !part.filename.is_empty() => Some(&mut part.filename),
                RowColumn::Value => Some(&mut part.value),
                RowColumn::ContentType => Some(&mut part.content_type),
                RowColumn::Description => Some(&mut part.description),
                RowColumn::Key => None,
            }
        }
    }
}

/// Turns the empty add row's text into a real row. `texts` holds one scratch
/// buffer per entry of [`RowTable::columns`]. When any is non-empty, one row is
/// appended through the table's `Add` action and every non-empty buffer is
/// moved into its column of that row, leaving the buffers empty.
///
/// # Returns
///
/// The new row's index and the first column that held text, or `None` when
/// every buffer was empty (or the table could not grow) and nothing changed.
fn adopt_ghost_text(
    model: &mut EditModel,
    table: RowTable,
    texts: &mut [String],
) -> Option<(usize, RowColumn)> {
    let first = table
        .columns()
        .iter()
        .zip(texts.iter())
        .find(|(_, t)| !t.is_empty())
        .map(|(c, _)| *c)?;
    let before = table.len(model);
    apply(
        model,
        &EditAction::Add {
            field: table.add_field(),
        },
    );
    if table.len(model) != before + 1 {
        return None;
    }
    for (column, text) in table.columns().iter().zip(texts.iter_mut()) {
        if !text.is_empty()
            && let Some(cell) = cell_text_mut(model, table, before, *column)
        {
            *cell = std::mem::take(text);
        }
    }
    Some((before, first))
}

/// The pending-focus ident of one cell, `row:column`.
fn cell_focus_ident(row: usize, column: RowColumn) -> String {
    format!("{row}:{column:?}")
}

/// The egui temp-data ids of `table`'s add-row scratch buffers, one per column.
fn ghost_ids(ui: &egui::Ui, table: RowTable) -> Vec<egui::Id> {
    table
        .columns()
        .iter()
        .map(|c| ui.make_persistent_id(("ghost_row", table, *c)))
        .collect()
}

/// Reads the add-row scratch buffers stored under `ids`.
fn load_ghost(ui: &egui::Ui, ids: &[egui::Id]) -> Vec<String> {
    ids.iter()
        .map(|id| ui.data(|d| d.get_temp::<String>(*id)).unwrap_or_default())
        .collect()
}

/// Adopts any text typed into the add row this frame as a new row, asks for
/// the same cell of that row to take focus next frame, and stores the
/// (now empty, or untouched) scratch buffers back under `ids`.
fn finish_ghost(ui: &egui::Ui, model: &mut EditModel, table: RowTable, mut texts: Vec<String>) {
    if let Some((row, column)) = adopt_ghost_text(model, table, &mut texts) {
        request_new_row_focus(ui, table.focus_key(), cell_focus_ident(row, column));
    }
    for (id, text) in ghost_ids(ui, table).into_iter().zip(texts) {
        ui.data_mut(|d| d.insert_temp(id, text));
    }
}

/// An editable text cell of `table`'s `column`, bound to `buf`. Value cells
/// colour their variable references by `defined` (monospace for headers, body font for query
/// values). For a real row (`row` is `Some`) a pending focus for this cell is
/// claimed and the caret put at the end of the text; the add row passes `None`.
fn edit_cell(
    ui: &mut egui::Ui,
    table: RowTable,
    (row, column): (Option<usize>, RowColumn),
    buf: &mut String,
    defined: &dyn Fn(&str) -> bool,
) {
    let hint = column.hint();
    let resp = match column {
        RowColumn::Value => tcell_edit_vars(ui, buf, hint, table.monospace_values(), defined),
        RowColumn::Key | RowColumn::Name | RowColumn::ContentType | RowColumn::Description => {
            tcell_edit(ui, buf, hint)
        }
    };
    let Some(row) = row else {
        return;
    };
    let key = table.focus_key();
    let ident = cell_focus_ident(row, column);
    let pending = ui.data(|d| d.get_temp::<String>(egui::Id::new(key)));
    if pending.as_deref() != Some(ident.as_str()) {
        return;
    }
    take_pending_focus(ui, key, &ident, &resp);
    let mut state = egui::TextEdit::load_state(ui.ctx(), resp.id).unwrap_or_default();
    let end = egui::text::CCursor::new(buf.chars().count());
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(end)));
    state.store(ui.ctx(), resp.id);
}

/// A read-only text cell of `table`'s `column`: keys in `info`, values with
/// their variable references coloured by `defined` (monospace for headers),
/// descriptions plain.
fn read_cell(
    ui: &mut egui::Ui,
    table: RowTable,
    column: RowColumn,
    text: &str,
    defined: &dyn Fn(&str) -> bool,
) {
    let p = palette(ui.ctx());
    match column {
        RowColumn::Key | RowColumn::Name => {
            ui.label(RichText::new(text).color(p.info));
        }
        RowColumn::Value => {
            let style = if table.monospace_values() {
                egui::TextStyle::Monospace
            } else {
                egui::TextStyle::Body
            };
            let font = style.resolve(ui.style());
            ui.label(url_layout_job(text, p, font, defined));
        }
        RowColumn::ContentType | RowColumn::Description => {
            ui.label(RichText::new(text).color(p.on_surface));
        }
    }
}

/// How a table's text cells draw.
#[derive(Clone, Copy)]
struct Mode<'a> {
    /// Whether the cells are editable.
    editing: bool,

    /// Whether the selected environment defines a variable name, for the
    /// value colours.
    defined: &'a dyn Fn(&str) -> bool,
}

/// A text cell of a real row: [`edit_cell`] while editing, else [`read_cell`].
fn text_cell(
    row: &mut egui_extras::TableRow<'_, '_>,
    table: RowTable,
    (i, column): (usize, RowColumn),
    buf: &mut String,
    mode: Mode<'_>,
) {
    row.col(|ui| {
        grid_cell(ui, |ui| {
            if mode.editing {
                edit_cell(ui, table, (Some(i), column), buf, mode.defined);
            } else {
                read_cell(ui, table, column, buf, mode.defined);
            }
        });
    });
}

/// The add row closing an editable table: empty fields bound to the scratch
/// buffers `texts` (one per [`RowTable::columns`] entry), with the chip
/// columns (see [`RowTable::spacer_before`]) and the delete cell left empty.
fn ghost_row(
    row: &mut egui_extras::TableRow<'_, '_>,
    table: RowTable,
    texts: &mut [String],
    defined: &dyn Fn(&str) -> bool,
) {
    let mut texts = texts.iter_mut();
    for (n, column) in table.columns().iter().enumerate() {
        if table.spacer_before(n) {
            row.col(|ui| grid_cell(ui, |_| {}));
        }
        if let Some(buf) = texts.next() {
            row.col(|ui| grid_cell(ui, |ui| edit_cell(ui, table, (None, *column), buf, defined)));
        }
    }
    row.col(|ui| grid_cell(ui, |_| {}));
}

/// The endpoint title on the toolbar row: the name in bold, then `: ` and the
/// description when it is not blank, both at body size and truncated to the
/// available width. In edit mode the name and description are two frameless
/// single-line fields separated by a `:` label.
pub(super) fn endpoint_title(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    let p = palette(ui.ctx());
    if editing {
        let font = egui::TextStyle::Body.resolve(ui.style());
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut model.name)
                    .frame(egui::Frame::NONE)
                    .hint_text("endpoint name")
                    .font(font)
                    .text_color(p.on_surface)
                    .desired_width(120.0)
                    .clip_text(false),
            );
            ui.label(RichText::new(":").color(p.on_surface));
            ui.add(
                egui::TextEdit::singleline(&mut model.description)
                    .frame(egui::Frame::NONE)
                    .hint_text("description")
                    .text_color(p.on_surface)
                    .desired_width(f32::INFINITY),
            );
        });
        return;
    }
    let style = ui.style().clone();
    let mut job = egui::text::LayoutJob::default();
    RichText::new(&model.name)
        .strong()
        .color(p.on_surface)
        .append_to(
            &mut job,
            &style,
            egui::FontSelection::Default,
            egui::Align::Center,
        );
    if !model.description.trim().is_empty() {
        RichText::new(format!(": {}", model.description))
            .color(p.on_surface)
            .append_to(
                &mut job,
                &style,
                egui::FontSelection::Default,
                egui::Align::Center,
            );
    }
    ui.add(egui::Label::new(job).truncate());
}

/// The `[ METHOD ] [ url................ ] [ Send ]` top row. The button
/// reads Cancel while a send is in flight.
///
/// # Arguments
///
/// * `ui` - the left pane
/// * `model` - the open contract
/// * `editing` - whether the method and URL are editable
/// * `defined` - whether the selected environment defines a variable name
/// * `pending` - whether a send is in flight
///
/// # Returns
///
/// [`ContractsAction::Send`] or [`ContractsAction::CancelSend`] when the
/// button was clicked.
pub(super) fn method_url_row(
    ui: &mut egui::Ui,
    model: &mut EditModel,
    editing: bool,
    defined: &dyn Fn(&str) -> bool,
    pending: bool,
) -> Option<ContractsAction> {
    let p = palette(ui.ctx());
    let mut action = None;
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
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (label, color) = if pending {
                ("Cancel", p.error)
            } else {
                ("Send", p.primary)
            };
            if text_button(ui, label, color) {
                action = Some(if pending {
                    ContractsAction::CancelSend
                } else {
                    ContractsAction::Send
                });
            }
            // The URL fills the space left of the button.
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let font = egui::TextStyle::Monospace.resolve(ui.style());
                if editing {
                    let mut layouter =
                        |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
                            let mut job = url_layout_job(buf.as_str(), p, font.clone(), defined);
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
                    ui.add(
                        egui::Label::new(url_layout_job(&model.url, p, font, defined)).truncate(),
                    );
                }
            });
        });
    });
    action
}

/// Body of the QUERY PARAMS section. Uses the flat `{name, value, description}`
/// query model; path variables now live inline in the URL string. While
/// editing, the table ends in an empty add row.
pub(super) fn query_section(
    ui: &mut egui::Ui,
    model: &mut EditModel,
    editing: bool,
    defined: &dyn Fn(&str) -> bool,
) {
    let table = RowTable::Query;
    let mode = Mode { editing, defined };
    let mut actions: Vec<EditAction> = Vec::new();
    let ids = ghost_ids(ui, table);
    let mut ghost = load_ghost(ui, &ids);
    table_frame(ui, |ui| {
        let fixed = if editing {
            vec![120.0, 88.0, 120.0, DELETE_COL_W]
        } else {
            vec![120.0, 88.0, 120.0]
        };
        let stretch = fill_column(ui, &fixed);
        let mut t = metadata_table(ui)
            .id_salt("query_table")
            .column(Column::exact(120.0)) // key
            .column(Column::exact(88.0)) // requirement
            .column(Column::exact(120.0)) // value
            .column(stretch); // description
        if editing {
            t = t.column(Column::exact(DELETE_COL_W)); // delete
        }
        t.header(TABLE_HEADER_H, |mut h| {
            for label in ["Key", "Requirement", "Value", "Description"] {
                h.col(|ui| grid_cell(ui, |ui| header_label(ui, label)));
            }
            if editing {
                h.col(|ui| grid_cell(ui, |_| {}));
            }
        })
        .body(|mut body| {
            for i in 0..model.query.len() {
                body.row(TABLE_ROW_H, |mut row| {
                    let q = &mut model.query[i];
                    text_cell(&mut row, table, (i, RowColumn::Key), &mut q.name, mode);
                    row.col(|ui| {
                        grid_cell(ui, |ui| {
                            if required_chip(ui, q.required, editing).is_some() {
                                actions.push(EditAction::ToggleBool {
                                    field: Field::QueryRequired(i),
                                });
                            }
                        });
                    });
                    text_cell(&mut row, table, (i, RowColumn::Value), &mut q.value, mode);
                    text_cell(
                        &mut row,
                        table,
                        (i, RowColumn::Description),
                        &mut q.description,
                        mode,
                    );
                    if editing {
                        row.col(|ui| {
                            grid_cell(ui, |ui| {
                                if delete_button(ui) {
                                    actions.push(EditAction::Delete {
                                        field: Field::QueryName(i),
                                    });
                                }
                            });
                        });
                    }
                });
            }
            if editing {
                body.row(TABLE_ROW_H, |mut row| {
                    ghost_row(&mut row, table, &mut ghost, defined)
                });
            }
        });
    });
    for a in &actions {
        apply(model, a);
    }
    if editing {
        finish_ghost(ui, model, table, ghost);
    }
}

/// Response header rows for the selected response `idx`. While editing, the
/// table ends in an empty add row.
pub(super) fn response_headers(
    ui: &mut egui::Ui,
    model: &mut EditModel,
    idx: usize,
    editing: bool,
) {
    if idx >= model.responses.len() {
        return;
    }
    let table = RowTable::ResponseHeader(idx);
    // Response headers document the reply and are never sent, so every name
    // counts as defined.
    let defined: &dyn Fn(&str) -> bool = &|_: &str| true;
    let mode = Mode { editing, defined };
    let mut actions: Vec<EditAction> = Vec::new();
    let ids = ghost_ids(ui, table);
    let mut ghost = load_ghost(ui, &ids);
    table_frame(ui, |ui| {
        let fixed = if editing {
            vec![150.0, 88.0, DELETE_COL_W]
        } else {
            vec![150.0, 88.0]
        };
        let stretch = fill_column(ui, &fixed);
        let mut t = metadata_table(ui)
            .id_salt("response_headers_table")
            .column(Column::exact(150.0)) // header
            .column(Column::exact(88.0)) // requirement
            .column(stretch); // value
        if editing {
            t = t.column(Column::exact(DELETE_COL_W)); // delete
        }
        t.header(TABLE_HEADER_H, |mut h| {
            for label in ["Header", "Requirement", "Value"] {
                h.col(|ui| grid_cell(ui, |ui| header_label(ui, label)));
            }
            if editing {
                h.col(|ui| grid_cell(ui, |_| {}));
            }
        })
        .body(|mut body| {
            let len = model.responses[idx].headers.len();
            for i in 0..len {
                body.row(TABLE_ROW_H, |mut row| {
                    let h = &mut model.responses[idx].headers[i];
                    text_cell(&mut row, table, (i, RowColumn::Key), &mut h.name, mode);
                    row.col(|ui| {
                        grid_cell(ui, |ui| {
                            // Response headers have no toggle action; flip directly.
                            if let Some(new) = required_chip(ui, h.required, editing) {
                                h.required = new;
                            }
                        });
                    });
                    text_cell(&mut row, table, (i, RowColumn::Value), &mut h.value, mode);
                    if editing {
                        row.col(|ui| {
                            grid_cell(ui, |ui| {
                                if delete_button(ui) {
                                    actions.push(EditAction::Delete {
                                        field: Field::ResponseHeaderName(idx, i),
                                    });
                                }
                            });
                        });
                    }
                });
            }
            if editing {
                body.row(TABLE_ROW_H, |mut row| {
                    ghost_row(&mut row, table, &mut ghost, defined)
                });
            }
        });
    });
    for a in &actions {
        apply(model, a);
    }
    if editing {
        finish_ghost(ui, model, table, ghost);
    }
}

/// The request headers table. While editing, it ends in an empty add row.
pub(super) fn headers(
    ui: &mut egui::Ui,
    model: &mut EditModel,
    editing: bool,
    defined: &dyn Fn(&str) -> bool,
) {
    let table = RowTable::Header;
    let mode = Mode { editing, defined };
    let mut actions: Vec<EditAction> = Vec::new();
    let ids = ghost_ids(ui, table);
    let mut ghost = load_ghost(ui, &ids);
    table_frame(ui, |ui| {
        let fixed = if editing {
            vec![150.0, 88.0, DELETE_COL_W]
        } else {
            vec![150.0, 88.0]
        };
        let stretch = fill_column(ui, &fixed);
        let mut t = metadata_table(ui)
            .id_salt("request_headers_table")
            .column(Column::exact(150.0)) // header
            .column(Column::exact(88.0)) // requirement
            .column(stretch); // value
        if editing {
            t = t.column(Column::exact(DELETE_COL_W)); // delete
        }
        t.header(TABLE_HEADER_H, |mut h| {
            for label in ["Header", "Requirement", "Value"] {
                h.col(|ui| grid_cell(ui, |ui| header_label(ui, label)));
            }
            if editing {
                h.col(|ui| grid_cell(ui, |_| {}));
            }
        })
        .body(|mut body| {
            for i in 0..model.headers.len() {
                body.row(TABLE_ROW_H, |mut row| {
                    let h = &mut model.headers[i];
                    text_cell(&mut row, table, (i, RowColumn::Key), &mut h.name, mode);
                    row.col(|ui| {
                        grid_cell(ui, |ui| {
                            if required_chip(ui, h.required, editing).is_some() {
                                actions.push(EditAction::ToggleBool {
                                    field: Field::HeaderRequired(i),
                                });
                            }
                        });
                    });
                    text_cell(&mut row, table, (i, RowColumn::Value), &mut h.value, mode);
                    if editing {
                        row.col(|ui| {
                            grid_cell(ui, |ui| {
                                if delete_button(ui) {
                                    actions.push(EditAction::Delete {
                                        field: Field::HeaderName(i),
                                    });
                                }
                            });
                        });
                    }
                });
            }
            if editing {
                body.row(TABLE_ROW_H, |mut row| {
                    ghost_row(&mut row, table, &mut ghost, defined)
                });
            }
        });
    });
    for a in &actions {
        apply(model, a);
    }
    if editing {
        finish_ghost(ui, model, table, ghost);
    }
}

/// The body-type dropdown shown while editing: `JSON` or `Multipart`.
///
/// # Arguments
///
/// * `ui` - where to draw it
/// * `loc` - which body it switches
/// * `is_multipart` - whether that body is multipart now
///
/// # Returns
///
/// [`ContractsAction::SwitchBody`] when the other type was picked.
fn body_kind_dropdown(
    ui: &mut egui::Ui,
    loc: BodyLoc,
    is_multipart: bool,
) -> Option<ContractsAction> {
    let p = palette(ui.ctx());
    let label = |multipart: bool| if multipart { "Multipart" } else { "JSON" };
    let mut picked = None;
    egui::ComboBox::from_id_salt(("body_kind", format!("{loc:?}")))
        .width(90.0)
        .selected_text(RichText::new(label(is_multipart)).color(p.on_surface))
        .show_ui(ui, |ui| {
            for multipart in [false, true] {
                if ui
                    .selectable_label(multipart == is_multipart, label(multipart))
                    .clicked()
                    && multipart != is_multipart
                {
                    picked = Some(multipart);
                }
            }
        });
    picked.map(|multipart| ContractsAction::SwitchBody { loc, multipart })
}

/// The start of a body's edit row: a `Body` label and the body-type
/// dropdown, drawn into a horizontal `ui`.
///
/// # Returns
///
/// What [`body_kind_dropdown`] returns.
fn body_kind_row(ui: &mut egui::Ui, loc: BodyLoc, is_multipart: bool) -> Option<ContractsAction> {
    section_label(ui, "Body");
    body_kind_dropdown(ui, loc, is_multipart)
}

/// The `Text` / `File` pill of a part's Type column. While editing it is a
/// button.
///
/// # Returns
///
/// Whether it was clicked this frame (always `false` when not editing).
fn part_kind_chip(ui: &mut egui::Ui, is_file: bool, editing: bool) -> bool {
    let p = palette(ui.ctx());
    let (label, fg) = if is_file {
        ("File", p.info)
    } else {
        ("Text", p.on_surface_variant)
    };
    let text = RichText::new(format!(" {label} "))
        .color(fg)
        .size(SIZE_SECONDARY)
        .background_color(p.secondary_container);
    if editing {
        ui.add(egui::Button::new(text).frame(false)).clicked()
    } else {
        ui.label(text);
        false
    }
}

/// The parts of the multipart body at `loc` as a grid table: Name, Type,
/// Value (the filename for a file part), Content-Type, Requirement and
/// Description, plus a delete column and an empty add row while editing.
/// Text typed into a cell lands in the model directly, and text typed into
/// the add row becomes a new part before this returns.
///
/// # Returns
///
/// The toggle and delete actions clicked this frame, for the caller to apply.
fn parts_table(
    ui: &mut egui::Ui,
    model: &mut EditModel,
    loc: BodyLoc,
    editing: bool,
    defined: &dyn Fn(&str) -> bool,
) -> Vec<EditAction> {
    let p = palette(ui.ctx());
    let mode = Mode { editing, defined };
    let (table, salt) = match loc {
        BodyLoc::Request => (RowTable::RequestParts, "parts_request".to_string()),
        BodyLoc::Response(r) => (RowTable::ResponseParts(r), format!("parts_response_{r}")),
    };
    let mut actions: Vec<EditAction> = Vec::new();
    let ids = ghost_ids(ui, table);
    let mut ghost = load_ghost(ui, &ids);
    table_frame(ui, |ui| {
        let mut fixed = vec![110.0, 60.0, 120.0, 110.0, 88.0];
        if editing {
            fixed.push(DELETE_COL_W);
        }
        let stretch = fill_column(ui, &fixed);
        let mut t = metadata_table(ui)
            .id_salt(salt)
            .column(Column::exact(110.0)) // name
            .column(Column::exact(60.0)) // type
            .column(Column::exact(120.0)) // value
            .column(Column::exact(110.0)) // content type
            .column(Column::exact(88.0)) // requirement
            .column(stretch); // description
        if editing {
            t = t.column(Column::exact(DELETE_COL_W)); // delete
        }
        t.header(TABLE_HEADER_H, |mut h| {
            for label in [
                "Name",
                "Type",
                "Value",
                "Content-Type",
                "Requirement",
                "Description",
            ] {
                h.col(|ui| grid_cell(ui, |ui| header_label(ui, label)));
            }
            if editing {
                h.col(|ui| grid_cell(ui, |_| {}));
            }
        })
        .body(|mut body| {
            for i in 0..table.len(model) {
                body.row(TABLE_ROW_H, |mut row| {
                    let Some(part) = table_parts(model, table).and_then(|parts| parts.get_mut(i))
                    else {
                        return;
                    };
                    text_cell(&mut row, table, (i, RowColumn::Name), &mut part.name, mode);
                    let is_file = !part.filename.is_empty();
                    row.col(|ui| {
                        grid_cell(ui, |ui| {
                            if part_kind_chip(ui, is_file, editing) {
                                actions.push(EditAction::ToggleBool {
                                    field: Field::PartIsFile(loc.clone(), i),
                                });
                            }
                        });
                    });
                    if is_file && !editing {
                        row.col(|ui| {
                            grid_cell(ui, |ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "{} {}",
                                        egui_phosphor::regular::FILE,
                                        part.filename
                                    ))
                                    .color(p.on_surface),
                                );
                            });
                        });
                    } else {
                        let value = if is_file {
                            &mut part.filename
                        } else {
                            &mut part.value
                        };
                        text_cell(&mut row, table, (i, RowColumn::Value), value, mode);
                    }
                    text_cell(
                        &mut row,
                        table,
                        (i, RowColumn::ContentType),
                        &mut part.content_type,
                        mode,
                    );
                    row.col(|ui| {
                        grid_cell(ui, |ui| {
                            if required_chip(ui, part.required, editing).is_some() {
                                actions.push(EditAction::ToggleBool {
                                    field: Field::PartRequired(loc.clone(), i),
                                });
                            }
                        });
                    });
                    text_cell(
                        &mut row,
                        table,
                        (i, RowColumn::Description),
                        &mut part.description,
                        mode,
                    );
                    if editing {
                        row.col(|ui| {
                            grid_cell(ui, |ui| {
                                if delete_button(ui) {
                                    actions.push(EditAction::Delete {
                                        field: Field::PartName(loc.clone(), i),
                                    });
                                }
                            });
                        });
                    }
                });
            }
            if editing {
                body.row(TABLE_ROW_H, |mut row| {
                    ghost_row(&mut row, table, &mut ghost, defined)
                });
            }
        });
    });
    if editing {
        finish_ghost(ui, model, table, ghost);
    }
    actions
}

/// The Request section's body: the parts table for a multipart request, else
/// the JSON example. While editing, a body-type dropdown sits above it, with
/// the Pretty control for a JSON body.
///
/// # Returns
///
/// The body switch the dropdown asks for this frame.
pub(super) fn request_body(
    ui: &mut egui::Ui,
    model: &mut EditModel,
    editing: bool,
    defined: &dyn Fn(&str) -> bool,
) -> Option<ContractsAction> {
    let p = palette(ui.ctx());
    ui.spacing_mut().item_spacing.y = SPACE_MEDIUM;
    let is_multipart = !model.multipart.is_empty();
    let mut action = None;
    if is_multipart {
        // A multipart body has no JSON example. Materializing an empty request
        // body here would let a keystroke create a second body that `to_json`
        // refuses to save.
        if editing {
            action = ui
                .horizontal(|ui| body_kind_row(ui, BodyLoc::Request, true))
                .inner;
        }
        for a in parts_table(ui, model, BodyLoc::Request, editing, defined) {
            apply(model, &a);
        }
    } else if editing {
        // The request body is always editable — materialize an empty one so the
        // JSON editor is shown by default. An untouched (blank) body is dropped
        // on save (see `EditModel::to_json`), so this never pollutes contracts.
        let req = model.request.get_or_insert_with(EditBody::empty);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = SPACE_MEDIUM;
            action = body_kind_row(ui, BodyLoc::Request, false);
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
    action
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

/// The stable id of the selected response's code field, so picking another
/// response in [`response_picker`] can move the caret to it in
/// [`response_fields`].
fn resp_code_edit_id() -> egui::Id {
    egui::Id::new("resp_code_edit")
}

/// The response-example picker: a frameless dropdown whose selected text is
/// `code title` in its status color, with one `code - title` entry per
/// response. In edit mode the list ends with a "+ New example" entry. Picking an
/// entry sets `resp_tab`, so the body and headers tabs show that response, and
/// while editing moves the caret to its code field.
///
/// # Returns
///
/// [`ContractsAction::OpenNewExample`] when "+ New example" was clicked.
pub(super) fn response_picker(
    ui: &mut egui::Ui,
    model: &EditModel,
    resp_tab: &mut usize,
    editing: bool,
) -> Option<ContractsAction> {
    let p = palette(ui.ctx());
    let mut action = None;
    let mut focus_code = false;
    let selected_text = model
        .responses
        .get(*resp_tab)
        .map(|r| {
            RichText::new(status_picker_label(&r.code, &r.description))
                .color(status_color(r.code.trim(), p))
                .strong()
        })
        .unwrap_or_else(|| RichText::new("?").color(p.on_surface_variant));
    ui.scope(|ui| {
        // No box around the closed dropdown, only the colored status text.
        let widgets = &mut ui.visuals_mut().widgets;
        for w in [
            &mut widgets.inactive,
            &mut widgets.hovered,
            &mut widgets.active,
            &mut widgets.open,
        ] {
            w.bg_fill = Color32::TRANSPARENT;
            w.weak_bg_fill = Color32::TRANSPARENT;
            w.bg_stroke = egui::Stroke::NONE;
        }
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
    });
    if editing && focus_code {
        ui.ctx()
            .memory_mut(|m| m.request_focus(resp_code_edit_id()));
    }
    action
}

/// The selected response's code and title, edited in place with no bordered
/// box as `[code] - [title] x`, where `x` deletes the response. Only drawn in
/// edit mode. The code field keeps at most three digits.
pub(super) fn response_fields(ui: &mut egui::Ui, model: &mut EditModel, resp_tab: usize) {
    let p = palette(ui.ctx());
    let i = resp_tab;
    if i >= model.responses.len() {
        return;
    }
    let mut actions: Vec<EditAction> = Vec::new();
    ui.horizontal(|ui| {
        // Tight spacing so the fields read as one label.
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
                .id(resp_code_edit_id())
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
    for a in &actions {
        apply(model, a);
    }
}

/// The status-tab label: `code - title`, or just `code` when the response has no
/// title, truncated to 30 characters (with an ellipsis) so long titles never
/// stretch a tab off the strip.
fn status_tab_label(code: &str, title: &str) -> String {
    status_label(code, title, " - ")
}

/// The picker's selected text: `code title`, or just `code` when the response
/// has no title, truncated like [`status_tab_label`].
fn status_picker_label(code: &str, title: &str) -> String {
    status_label(code, title, " ")
}

/// `code`, `sep` and `title` joined, or just `code` without a title, cut to 30
/// characters with an ellipsis.
fn status_label(code: &str, title: &str, sep: &str) -> String {
    let code = if code.trim().is_empty() { "?" } else { code };
    let full = if title.trim().is_empty() {
        code.to_string()
    } else {
        format!("{code}{sep}{title}")
    };
    if full.chars().count() > 30 {
        let head: String = full.chars().take(29).collect();
        format!("{head}…")
    } else {
        full
    }
}

/// Renders the selected response's body: its parts table when it is
/// multipart, else its JSON example. While editing, a body-type dropdown sits
/// above it, with the Pretty control for a JSON body. The caller (the Response
/// tab) has already drawn the code-tab strip, which also owns the response's
/// title, and guarantees `idx` is a valid response index.
///
/// # Returns
///
/// The body switch the dropdown asks for this frame.
pub(super) fn response_body(
    ui: &mut egui::Ui,
    model: &mut EditModel,
    idx: usize,
    editing: bool,
) -> Option<ContractsAction> {
    let p = palette(ui.ctx());
    ui.spacing_mut().item_spacing.y = SPACE_SMALL;
    let loc = BodyLoc::Response(idx);
    let is_multipart = !model.responses[idx].multipart.is_empty();
    let mut action = None;
    if is_multipart {
        if editing {
            action = ui
                .horizontal(|ui| body_kind_row(ui, loc.clone(), true))
                .inner;
        }
        // A response body is never sent, so every name counts as defined.
        for a in parts_table(ui, model, loc, editing, &|_: &str| true) {
            apply(model, &a);
        }
        ui.add_space(SPACE_MEDIUM);
        return action;
    }
    let r = &mut model.responses[idx];
    if editing {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = SPACE_MEDIUM;
            action = body_kind_row(ui, loc, false);
            ui.add_space(SPACE_MEDIUM);
            if text_button(ui, "Pretty", p.warning) {
                r.example = apic_core::json::pretty_json(&r.example);
            }
        });
    }
    json_editor(ui, &mut r.example, editing);
    ui.add_space(SPACE_MEDIUM);
    action
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
                endpoint_title(ui, &mut m, true);
                endpoint_title(ui, &mut m, false);
                method_url_row(ui, &mut m, true, &|_: &str| true, false);
                method_url_row(ui, &mut m, false, &|_: &str| false, true);
                headers(ui, &mut m, true, &|_: &str| true);
                headers(ui, &mut m, false, &|_: &str| true);
                query_section(ui, &mut m, true, &|_: &str| true);
                request_body(ui, &mut m, true, &|_: &str| true);
                response_picker(ui, &m, &mut resp, true);
                response_fields(ui, &mut m, resp);
                response_body(ui, &mut m, 0, true);
                response_headers(ui, &mut m, 0, true);
            });
        }
    }

    /// A contract with one row in every table, so each test can see that the
    /// add row appends after the existing rows.
    fn model_with_rows() -> EditModel {
        let c = apic_core::json::json_get(
            r#"{ "name":"t","method":"GET","url":"https://h",
             "query":[{"name":"page","value":"2","description":"d","required":true}],
             "headers":[{"name":"Accept","value":"*/*","required":true}],
             "responses":[{"code":200,"description":"ok",
                "headers":[{"name":"X","value":"1","required":false}]}] }"#,
            None,
        )
        .unwrap();
        EditModel::from_contract(c)
    }

    /// Every text column of `row` in `table`, in display order.
    fn row_texts(model: &mut EditModel, table: RowTable, row: usize) -> Vec<String> {
        table
            .columns()
            .iter()
            .map(|c| cell_text_mut(model, table, row, *c).unwrap().clone())
            .collect()
    }

    #[test]
    fn typing_in_the_empty_row_adds_one_row() {
        for table in [
            RowTable::Query,
            RowTable::Header,
            RowTable::ResponseHeader(0),
        ] {
            for (n, column) in table.columns().iter().enumerate() {
                let mut m = model_with_rows();
                let before = table.len(&m);
                let mut texts = vec![String::new(); table.columns().len()];
                texts[n] = "{{token}}".to_string();
                let adopted = adopt_ghost_text(&mut m, table, &mut texts);
                assert_eq!(adopted, Some((before, *column)), "{table:?} {column:?}");
                assert_eq!(table.len(&m), before + 1, "{table:?} {column:?}");
                let mut expected = vec![String::new(); table.columns().len()];
                expected[n] = "{{token}}".to_string();
                assert_eq!(row_texts(&mut m, table, before), expected);
                assert!(texts.iter().all(String::is_empty), "scratch is cleared");
            }
            let mut m = model_with_rows();
            let before = table.len(&m);
            let mut empty = vec![String::new(); table.columns().len()];
            assert_eq!(adopt_ghost_text(&mut m, table, &mut empty), None);
            assert_eq!(table.len(&m), before, "an empty add row adds nothing");
        }
    }

    #[test]
    fn typed_keys_land_in_one_new_row_with_the_caret_at_the_end() {
        let ctx = egui::Context::default();
        crate::ui::theme::install_themes(&ctx, egui::ThemePreference::Dark);
        let mut m = model_with_rows();
        m.headers.clear();
        let frame = |events: Vec<egui::Event>, m: &mut EditModel| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(800.0, 600.0),
                )),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| headers(ui, m, true, &|_: &str| true));
            out.textures_delta.clear();
        };
        let tab = egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        frame(Vec::new(), &mut m);
        // With no rows, the add row's key field is the first focusable widget.
        frame(vec![tab], &mut m);
        frame(vec![egui::Event::Text("a".into())], &mut m);
        frame(Vec::new(), &mut m);
        frame(vec![egui::Event::Text("b".into())], &mut m);
        frame(Vec::new(), &mut m);
        assert_eq!(m.headers.len(), 1, "typing adds exactly one row");
        assert_eq!(
            m.headers[0].name, "ab",
            "the caret followed into the new row"
        );
        assert_eq!(m.headers[0].value, "");
    }

    /// A contract with a text and a file part in the request and in the first
    /// response.
    fn multipart_model() -> EditModel {
        let c = apic_core::json::json_get(
            r#"{ "name":"u","method":"POST","url":"https://h","headers":[],
             "multipart":[{"name":"title","value":"hi","required":false},
                          {"name":"avatar","filename":"a.png","contentType":"image/png","required":true}],
             "responses":[{"code":201,"description":"ok",
                "multipart":[{"name":"thumb","filename":"t.png","required":false}]}] }"#,
            None,
        )
        .unwrap();
        EditModel::from_contract(c)
    }

    #[test]
    fn parts_table_renders_in_both_themes() {
        for preference in crate::ui::theme::BOTH_THEMES {
            crate::ui::theme::run_in_theme(preference, |ui| {
                let mut m = multipart_model();
                for editing in [false, true] {
                    assert!(request_body(ui, &mut m, editing, &|_: &str| true).is_none());
                    assert!(response_body(ui, &mut m, 0, editing).is_none());
                }
                assert!(m.request.is_none(), "no JSON body appears");
                assert_eq!(m.multipart.len(), 2);
                assert_eq!(m.responses[0].multipart.len(), 1);
                assert!(m.responses[0].example.is_empty());
            });
        }
    }

    #[test]
    fn typing_in_the_parts_add_row_adds_one_part() {
        for table in [RowTable::RequestParts, RowTable::ResponseParts(0)] {
            for (n, column) in table.columns().iter().enumerate() {
                let mut m = multipart_model();
                let before = table.len(&m);
                let mut texts = vec![String::new(); table.columns().len()];
                texts[n] = "x".to_string();
                let adopted = adopt_ghost_text(&mut m, table, &mut texts);
                assert_eq!(adopted, Some((before, *column)), "{table:?} {column:?}");
                assert_eq!(table.len(&m), before + 1, "{table:?} {column:?}");
                let mut expected = vec![String::new(); table.columns().len()];
                expected[n] = "x".to_string();
                assert_eq!(row_texts(&mut m, table, before), expected);
                assert!(texts.iter().all(String::is_empty), "scratch is cleared");
            }
            let mut m = multipart_model();
            let before = table.len(&m);
            let mut empty = vec![String::new(); table.columns().len()];
            assert_eq!(adopt_ghost_text(&mut m, table, &mut empty), None);
            assert_eq!(table.len(&m), before, "an empty add row adds nothing");
        }
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
                request_body(ui, &mut m, true, &|_: &str| true);
                // Materializing one here is what makes the file unsavable the
                // moment anything is typed into it.
                assert!(m.request.is_none());
            });
        }
    }
}
