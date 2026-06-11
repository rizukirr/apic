//! UI state and pure key-handling for the table-based authoring TUI.
//!
//! Key handlers are pure functions over `(UiState, &mut EditModel, KeyEvent)` so
//! they are unit-testable without a terminal. The cursor is two-level:
//! `cell: None` selects a whole table row; `cell: Some(c)` edits a cell.

use crate::tui::model::EditModel;
use crate::tui::model::{EditBody, EditHeader, EditQuery, EditResponse, EditSchema, EditVariable};
use crate::tui::rows::{BodyLoc, CellKind, Expand, Field, RowKind, Section, TableRow, flatten};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::json::{method_all, method_str};
use std::path::Path;

const HINT: &str = "↑↓ select · Enter edit/open · ←→ cell · a add · d delete · Esc back · Ctrl-S save · q quit · ? help";

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) enum Mode {
    #[default]
    Normal,
    Insert(String),
    Example,
    Help,
    ConfirmQuit,
    ConfirmDelete(Field),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Action {
    None,
    OpenExample(Field, String),
    Save,
    Quit,
}

pub(crate) struct UiState {
    pub sections: Vec<Section>,
    pub sec: usize,
    pub row: usize,
    pub cell: Option<usize>,
    pub mode: Mode,
    pub dirty: bool,
    pub status: String,
    pub expanded: Option<Expand>,
}

impl UiState {
    pub fn new(model: &EditModel) -> Self {
        let sections = flatten(model, None);
        let mut s = UiState {
            sections,
            sec: 0,
            row: 0,
            cell: None,
            mode: Mode::Normal,
            dirty: false,
            status: HINT.to_string(),
            expanded: None,
        };
        s.snap_to_first_row();
        s
    }

    /// Rebuilds sections after a mutation, clamping the cursor; drops cell focus
    /// if it no longer addresses a valid cell.
    pub fn refresh(&mut self, model: &EditModel) {
        self.sections = flatten(model, self.expanded);
        if self.sec >= self.sections.len() {
            self.sec = self.sections.len().saturating_sub(1);
        }
        let nrows = self
            .sections
            .get(self.sec)
            .map(|s| s.rows.len())
            .unwrap_or(0);
        if self.row >= nrows {
            self.row = nrows.saturating_sub(1);
        }
        if let Some(c) = self.cell {
            let ncells = self.current_row().map(|r| r.cells.len()).unwrap_or(0);
            if c >= ncells {
                self.cell = None;
            }
        }
    }

    fn snap_to_first_row(&mut self) {
        for (si, s) in self.sections.iter().enumerate() {
            if !s.rows.is_empty() {
                self.sec = si;
                self.row = 0;
                return;
            }
        }
    }

    pub fn current_row(&self) -> Option<&TableRow> {
        self.sections.get(self.sec)?.rows.get(self.row)
    }

    /// The field of the focused cell (cell-edit mode), if any.
    fn focused_field(&self) -> Option<Field> {
        let c = self.cell?;
        self.current_row()?
            .cells
            .get(c)
            .map(|cell| cell.field.clone())
    }

    pub fn focused_field_pub(&self) -> Option<Field> {
        self.focused_field()
    }

    /// Indices of editable (non-Label) cells in the current row.
    fn editable_cells(&self) -> Vec<usize> {
        self.current_row()
            .map(|r| {
                r.cells
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.kind != CellKind::Label)
                    .map(|(i, _)| i)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Moves row selection by `dir` across section boundaries (cell reset).
    fn move_row(&mut self, dir: isize) {
        let coords: Vec<(usize, usize)> = self
            .sections
            .iter()
            .enumerate()
            .flat_map(|(si, s)| (0..s.rows.len()).map(move |ri| (si, ri)))
            .collect();
        if coords.is_empty() {
            return;
        }
        let pos = coords
            .iter()
            .position(|&(si, ri)| si == self.sec && ri == self.row)
            .unwrap_or(0);
        let np = (pos as isize + dir).clamp(0, coords.len() as isize - 1) as usize;
        let (s, r) = coords[np];
        self.sec = s;
        self.row = r;
        self.cell = None;
    }

    /// Moves the focused cell by `dir` among editable cells.
    fn move_cell(&mut self, dir: isize) {
        let edit = self.editable_cells();
        if edit.is_empty() {
            return;
        }
        let cur = self.cell.unwrap_or(edit[0]);
        let pos = edit.iter().position(|&i| i == cur).unwrap_or(0);
        let np = (pos as isize + dir).clamp(0, edit.len() as isize - 1) as usize;
        self.cell = Some(edit[np]);
    }
}

/// The field used by `d` (delete) on the focused row: the first editable cell,
/// else the first cell.
fn delete_field(state: &UiState) -> Option<Field> {
    let row = state.current_row()?;
    row.cells
        .iter()
        .find(|c| c.kind != CellKind::Label)
        .or_else(|| row.cells.first())
        .map(|c| c.field.clone())
}

/// Whether `delete_row` would actually remove a row for this field — matching
/// exactly the variants it handles (path/query/var/header/response/schema).
fn is_deletable(field: &Field) -> bool {
    matches!(
        field,
        Field::PathSeg(_)
            | Field::QueryName(_)
            | Field::QueryValue(_)
            | Field::QueryDesc(_)
            | Field::QueryRequired(_)
            | Field::VarName(_)
            | Field::VarType(_)
            | Field::VarDesc(_)
            | Field::VarRequired(_)
            | Field::HeaderName(_)
            | Field::HeaderValue(_)
            | Field::ResponseCode(_)
            | Field::ResponseDesc(_)
            | Field::SchemaName(_, _)
            | Field::SchemaType(_, _)
            | Field::SchemaDesc(_, _)
            | Field::SchemaRequired(_, _)
            | Field::SchemaAccept(_, _)
    )
}

/// Handles one key in Normal mode (row-select or cell-edit per `state.cell`).
pub(crate) fn handle_normal(state: &mut UiState, model: &mut EditModel, key: KeyEvent) -> Action {
    if (key.code, key.modifiers) == (KeyCode::Char('s'), KeyModifiers::CONTROL) {
        return Action::Save;
    }
    if state.cell.is_some() {
        return handle_cell(state, model, key);
    }
    match (key.code, key.modifiers) {
        // Esc first collapses any expanded region, before the quit flow.
        (KeyCode::Esc, _) if state.expanded.is_some() => {
            state.expanded = None;
            state.refresh(model);
            Action::None
        }
        (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => {
            if state.dirty {
                state.mode = Mode::ConfirmQuit;
                Action::None
            } else {
                Action::Quit
            }
        }
        (KeyCode::Char('?'), _) => {
            state.mode = Mode::Help;
            Action::None
        }
        (KeyCode::Down, _) | (KeyCode::Char('j'), _) => {
            state.move_row(1);
            Action::None
        }
        (KeyCode::Up, _) | (KeyCode::Char('k'), _) => {
            state.move_row(-1);
            Action::None
        }
        (KeyCode::Enter, _) => begin_row(state, model),
        (KeyCode::Char('a'), _) => {
            append_here(state, model);
            Action::None
        }
        (KeyCode::Char('d'), _) => {
            if let Some(f) = delete_field(state)
                && is_deletable(&f)
            {
                state.mode = Mode::ConfirmDelete(f);
            }
            Action::None
        }
        _ => Action::None,
    }
}

/// Appends a row near the focused schema field — a child under an object field,
/// a sibling otherwise — falling back to the current section's `add` target.
fn append_here(state: &mut UiState, model: &mut EditModel) {
    if let Some((loc, path, is_object)) = focused_schema_target(state) {
        let target = if is_object {
            Field::SchemaAdd(loc, path) // add a child under the object field
        } else {
            let mut parent = path;
            parent.pop();
            Field::SchemaAdd(loc, parent) // add a sibling at this level
        };
        add_row(state, model, &target);
        return;
    }
    if let Some(field) = state.sections.get(state.sec).and_then(|s| s.add.clone()) {
        add_row(state, model, &field);
    }
}

/// If the focused row is a schema field, returns its (loc, path) and whether its
/// declared type is an object (so `a` adds a child rather than a sibling).
fn focused_schema_target(state: &UiState) -> Option<(BodyLoc, Vec<usize>, bool)> {
    let row = state.current_row()?;
    let (loc, path) = row.cells.iter().find_map(|c| match &c.field {
        Field::SchemaName(l, p)
        | Field::SchemaType(l, p)
        | Field::SchemaRequired(l, p)
        | Field::SchemaAccept(l, p)
        | Field::SchemaDesc(l, p) => Some((l.clone(), p.clone())),
        _ => None,
    })?;
    let dtype = row
        .cells
        .iter()
        .find_map(|c| matches!(&c.field, Field::SchemaType(_, _)).then(|| c.value.clone()))
        .unwrap_or_default();
    let is_object = crate::json::parse_type(&dtype).0 == "object";
    Some((loc, path, is_object))
}

/// Keys while a cell is focused (cell-edit mode).
fn handle_cell(state: &mut UiState, model: &mut EditModel, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => {
            state.cell = None;
            Action::None
        }
        KeyCode::Left => {
            state.move_cell(-1);
            Action::None
        }
        KeyCode::Right => {
            state.move_cell(1);
            Action::None
        }
        KeyCode::Char('h') => {
            state.move_cell(-1);
            Action::None
        }
        KeyCode::Char('l') => {
            state.move_cell(1);
            Action::None
        }
        KeyCode::Char(' ') => {
            // Space toggles a focused bool cell.
            if let (Some(c), Some(field)) = (state.cell, state.focused_field()) {
                let is_bool = state
                    .current_row()
                    .and_then(|r| r.cells.get(c))
                    .map(|cell| cell.kind == CellKind::Bool)
                    .unwrap_or(false);
                if is_bool {
                    toggle_bool(model, &field);
                    state.dirty = true;
                    state.refresh(model);
                }
            }
            Action::None
        }
        KeyCode::Enter => begin_cell_edit(state, model),
        _ => Action::None,
    }
}

/// Enter on a selected row (row-select mode).
fn begin_row(state: &mut UiState, model: &mut EditModel) -> Action {
    let Some(row) = state.current_row().cloned() else {
        return Action::None;
    };
    match row.kind {
        RowKind::UrlLine | RowKind::Title => {
            let tgt = state.sections[state.sec].expand;
            state.expanded = if state.expanded == tgt { None } else { tgt };
            state.cell = None;
            state.refresh(model);
            Action::None
        }
        RowKind::Example => Action::OpenExample(row.cells[0].field.clone(), String::new()),
        RowKind::Name | RowKind::Desc | RowKind::Field => {
            if let Some(&first) = state.editable_cells().first() {
                state.cell = Some(first);
            }
            Action::None
        }
    }
}

/// Enter on a focused cell (cell-edit mode): dispatch by cell kind.
fn begin_cell_edit(state: &mut UiState, model: &mut EditModel) -> Action {
    let Some(c) = state.cell else {
        return Action::None;
    };
    let Some(cell) = state.current_row().and_then(|r| r.cells.get(c)).cloned() else {
        return Action::None;
    };
    match cell.kind {
        CellKind::Text => {
            state.mode = Mode::Insert(cell.value.clone());
            Action::None
        }
        CellKind::Enum => {
            cycle_method(state, model, true);
            Action::None
        }
        CellKind::Bool => {
            toggle_bool(model, &cell.field);
            state.dirty = true;
            state.refresh(model);
            Action::None
        }
        CellKind::Label => Action::None,
    }
}

/// Cycles the method enum forward/back (the only enum field today).
fn cycle_method(state: &mut UiState, model: &mut EditModel, forward: bool) {
    let all = method_all();
    let cur = method_str(&model.method);
    let idx = all.iter().position(|m| method_str(m) == cur).unwrap_or(0);
    let next = if forward {
        (idx + 1) % all.len()
    } else {
        (idx + all.len() - 1) % all.len()
    };
    model.method = all[next].clone();
    state.dirty = true;
    state.refresh(model);
}

fn add_row(state: &mut UiState, model: &mut EditModel, field: &Field) {
    match field {
        Field::PathAdd => model.url.path.push(String::new()),
        Field::QueryAdd => model.url.query.push(EditQuery {
            name: String::new(),
            value: String::new(),
            description: String::new(),
            required: false,
        }),
        Field::VarAdd => model.url.variable.push(EditVariable {
            name: String::new(),
            dtype: "string".to_string(),
            description: String::new(),
            required: false,
        }),
        Field::HeaderAdd => model.headers.push(EditHeader {
            name: String::new(),
            value: String::new(),
        }),
        Field::ResponseAdd => model.responses.push(EditResponse::blank()),
        Field::RequestToggle => {
            model.request = if model.request.is_some() {
                None
            } else {
                Some(EditBody::empty())
            };
        }
        Field::SchemaAdd(BodyLoc::Request, path) => {
            if let Some(children) = model.schema_children_mut_request(path) {
                children.push(EditSchema::blank());
            }
        }
        Field::SchemaAdd(BodyLoc::Response(r), path) => {
            if let Some(children) = model.schema_children_mut_response(*r, path) {
                children.push(EditSchema::blank());
            }
        }
        _ => return,
    }
    state.dirty = true;
    state.cell = None;
    state.refresh(model);
}

fn delete_row(state: &mut UiState, model: &mut EditModel, field: &Field) {
    let mut changed = true;
    match field {
        Field::PathSeg(i) => drop_at(&mut model.url.path, *i),
        Field::QueryName(i)
        | Field::QueryValue(i)
        | Field::QueryDesc(i)
        | Field::QueryRequired(i) => drop_at(&mut model.url.query, *i),
        Field::VarName(i) | Field::VarType(i) | Field::VarDesc(i) | Field::VarRequired(i) => {
            drop_at(&mut model.url.variable, *i)
        }
        Field::HeaderName(i) | Field::HeaderValue(i) => drop_at(&mut model.headers, *i),
        Field::ResponseCode(i) | Field::ResponseDesc(i) => drop_at(&mut model.responses, *i),
        Field::SchemaName(loc, path)
        | Field::SchemaType(loc, path)
        | Field::SchemaDesc(loc, path)
        | Field::SchemaRequired(loc, path)
        | Field::SchemaAccept(loc, path) => {
            if let Some((last, parent)) = path.split_last() {
                let children = match loc {
                    BodyLoc::Request => model.schema_children_mut_request(parent),
                    BodyLoc::Response(r) => model.schema_children_mut_response(*r, parent),
                };
                if let Some(c) = children {
                    drop_at(c, *last);
                }
            }
        }
        _ => changed = false,
    }
    if changed {
        state.dirty = true;
        state.cell = None;
        state.refresh(model);
    }
}

fn drop_at<T>(v: &mut Vec<T>, i: usize) {
    if i < v.len() {
        v.remove(i);
    }
}

/// Handles a key while editing a single-line field.
pub(crate) fn handle_insert(state: &mut UiState, model: &mut EditModel, key: KeyEvent) -> Action {
    let Mode::Insert(buf) = &mut state.mode else {
        return Action::None;
    };
    match key.code {
        KeyCode::Char(c) => {
            buf.push(c);
            Action::None
        }
        KeyCode::Backspace => {
            buf.pop();
            Action::None
        }
        KeyCode::Enter => {
            let value = buf.clone();
            if let Some(field) = state.focused_field_pub() {
                set_field(model, &field, value);
                state.dirty = true;
            }
            state.mode = Mode::Normal;
            state.cell = None;
            state.refresh(model);
            Action::None
        }
        KeyCode::Esc => {
            state.mode = Mode::Normal;
            Action::None
        }
        _ => Action::None,
    }
}

fn toggle_bool(model: &mut EditModel, field: &Field) {
    match field {
        Field::QueryRequired(i) => {
            if let Some(q) = model.url.query.get_mut(*i) {
                q.required = !q.required;
            }
        }
        Field::VarRequired(i) => {
            if let Some(v) = model.url.variable.get_mut(*i) {
                v.required = !v.required;
            }
        }
        Field::SchemaRequired(BodyLoc::Request, path) => {
            if let Some(n) = model.schema_at_mut_request(path) {
                n.required = !n.required;
            }
        }
        Field::SchemaRequired(BodyLoc::Response(r), path) => {
            if let Some(n) = model.schema_at_mut_response(*r, path) {
                n.required = !n.required;
            }
        }
        _ => {}
    }
}

/// Writes a string `value` into the model at `field`. No-op for non-text fields.
fn set_field(model: &mut EditModel, field: &Field, value: String) {
    match field {
        Field::Name => model.name = value,
        Field::Description => model.description = value,
        Field::Protocol => model.url.protocol = value,
        Field::Host => model.url.host = value,
        Field::PathSeg(i) => {
            if let Some(s) = model.url.path.get_mut(*i) {
                *s = value;
            }
        }
        Field::QueryName(i) => set_query(model, *i, |q| q.name = value.clone()),
        Field::QueryValue(i) => set_query(model, *i, |q| q.value = value.clone()),
        Field::QueryDesc(i) => set_query(model, *i, |q| q.description = value.clone()),
        Field::VarName(i) => set_var(model, *i, |v| v.name = value.clone()),
        Field::VarType(i) => set_var(model, *i, |v| v.dtype = value.clone()),
        Field::VarDesc(i) => set_var(model, *i, |v| v.description = value.clone()),
        Field::HeaderName(i) => {
            if let Some(h) = model.headers.get_mut(*i) {
                h.name = value;
            }
        }
        Field::HeaderValue(i) => {
            if let Some(h) = model.headers.get_mut(*i) {
                h.value = value;
            }
        }
        Field::BodyDtype(BodyLoc::Request) => {
            if let Some(b) = model.request.as_mut() {
                b.dtype = value;
            }
        }
        Field::BodyDtype(BodyLoc::Response(r)) => {
            if let Some(b) = model.responses.get_mut(*r) {
                b.dtype = value;
            }
        }
        Field::ResponseCode(i) => {
            if let Some(r) = model.responses.get_mut(*i) {
                r.code = value;
            }
        }
        Field::ResponseDesc(i) => {
            if let Some(r) = model.responses.get_mut(*i) {
                r.description = value;
            }
        }
        Field::SchemaName(loc, p) => set_schema(model, loc, p, |s| s.name = value.clone()),
        Field::SchemaType(loc, p) => set_schema(model, loc, p, |s| s.dtype = value.clone()),
        Field::SchemaDesc(loc, p) => set_schema(model, loc, p, |s| s.description = value.clone()),
        Field::SchemaAccept(loc, p) => set_schema(model, loc, p, |s| s.accept = value.clone()),
        _ => {}
    }
}

fn set_query(model: &mut EditModel, i: usize, f: impl FnOnce(&mut crate::tui::model::EditQuery)) {
    if let Some(q) = model.url.query.get_mut(i) {
        f(q);
    }
}
fn set_var(model: &mut EditModel, i: usize, f: impl FnOnce(&mut crate::tui::model::EditVariable)) {
    if let Some(v) = model.url.variable.get_mut(i) {
        f(v);
    }
}
fn set_schema(
    model: &mut EditModel,
    loc: &BodyLoc,
    path: &[usize],
    f: impl FnOnce(&mut crate::tui::model::EditSchema),
) {
    let node = match loc {
        BodyLoc::Request => model.schema_at_mut_request(path),
        BodyLoc::Response(r) => model.schema_at_mut_response(*r, path),
    };
    if let Some(n) = node {
        f(n);
    }
}

/// Saves the model to `path`, updating dirty flag and status line.
pub(crate) fn apply_save(state: &mut UiState, model: &EditModel, path: &Path) {
    match model.save(path) {
        Ok(()) => {
            state.dirty = false;
            state.status = format!("saved {}", path.display());
        }
        Err(err) => {
            state.status = format!("save error: {err}");
        }
    }
}

/// Handles keys while the quit confirmation is showing. Returns the action.
pub(crate) fn handle_confirm_quit(state: &mut UiState, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Char('y') => Action::Save, // event loop saves, then quits
        KeyCode::Char('n') => Action::Quit,
        KeyCode::Esc => {
            state.mode = Mode::Normal;
            state.status = "Ctrl-S save · q quit · ? help".into();
            Action::None
        }
        _ => Action::None,
    }
}

/// Handles keys while the delete confirmation is showing.
pub(crate) fn handle_confirm_delete(
    state: &mut UiState,
    model: &mut EditModel,
    key: KeyEvent,
) -> Action {
    match key.code {
        KeyCode::Char('y') => {
            if let Mode::ConfirmDelete(f) = state.mode.clone() {
                delete_row(state, model, &f);
            }
            state.mode = Mode::Normal;
            Action::None
        }
        KeyCode::Char('n') | KeyCode::Esc => {
            state.mode = Mode::Normal;
            Action::None
        }
        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::json_get;

    fn model() -> EditModel {
        let c = json_get(
            r#"{ "name":"t","description":"d","method":"GET",
                 "url":{"protocol":"https","host":"h","path":["x"],
                        "query":[{"name":"page","value":"1","description":"d","required":false}]},
                 "headers":[{"name":"A","value":"B"}],
                 "responses":[{"code":200,"description":"ok","schema":[]}] }"#,
            None,
        )
        .unwrap();
        EditModel::from_contract(c)
    }
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    fn goto(s: &mut UiState, pred: impl Fn(&Field) -> bool) {
        for (si, sec) in s.sections.iter().enumerate() {
            for (ri, row) in sec.rows.iter().enumerate() {
                if row.cells.iter().any(|c| pred(&c.field)) {
                    s.sec = si;
                    s.row = ri;
                    s.cell = None;
                    return;
                }
            }
        }
        panic!("no matching row");
    }

    #[test]
    fn enter_on_url_expands_then_esc_collapses() {
        let mut m = model();
        let mut s = UiState::new(&m);
        goto(&mut s, |f| matches!(f, Field::Method)); // url line carries Method
        handle_normal(&mut s, &mut m, key(KeyCode::Enter));
        assert_eq!(s.expanded, Some(Expand::Url));
        handle_normal(&mut s, &mut m, key(KeyCode::Esc));
        assert_eq!(s.expanded, None);
    }

    #[test]
    fn response_title_expands_and_code_is_editable() {
        let mut m = model();
        let mut s = UiState::new(&m);
        // find the RESPONSE title row (RowKind::Title in a Response section)
        let (si, ri) = s
            .sections
            .iter()
            .enumerate()
            .find_map(|(si, sec)| {
                sec.rows
                    .iter()
                    .position(|r| r.kind == RowKind::Title)
                    .filter(|_| matches!(sec.expand, Some(Expand::Response(_))))
                    .map(|ri| (si, ri))
            })
            .unwrap();
        s.sec = si;
        s.row = ri;
        s.cell = None;
        handle_normal(&mut s, &mut m, key(KeyCode::Enter)); // expand
        assert!(matches!(s.expanded, Some(Expand::Response(_))));
        // a code row now exists and is editable
        s.refresh(&m);
        goto(&mut s, |f| matches!(f, Field::ResponseCode(_)));
        handle_normal(&mut s, &mut m, key(KeyCode::Enter)); // cell mode
        handle_normal(&mut s, &mut m, key(KeyCode::Enter)); // insert (prefilled "200")
        handle_insert(&mut s, &mut m, key(KeyCode::Backspace));
        handle_insert(&mut s, &mut m, key(KeyCode::Char('1')));
        handle_insert(&mut s, &mut m, key(KeyCode::Enter));
        assert_eq!(m.responses[0].code, "201");
        // esc collapses
        handle_normal(&mut s, &mut m, key(KeyCode::Esc));
        assert_eq!(s.expanded, None);
    }

    #[test]
    fn a_appends_to_current_section() {
        let mut m = model();
        let mut s = UiState::new(&m);
        goto(&mut s, |f| matches!(f, Field::HeaderName(_)));
        handle_normal(&mut s, &mut m, key(KeyCode::Char('a')));
        assert_eq!(m.headers.len(), 2);
    }

    #[test]
    fn d_deletes_focused_row() {
        let mut m = model();
        let mut s = UiState::new(&m);
        goto(&mut s, |f| matches!(f, Field::QueryName(_)));
        handle_normal(&mut s, &mut m, key(KeyCode::Char('d')));
        handle_confirm_delete(&mut s, &mut m, key(KeyCode::Char('y')));
        assert_eq!(m.url.query.len(), 0);
    }

    #[test]
    fn delete_requires_confirmation() {
        let mut m = model();
        let mut s = UiState::new(&m);
        goto(&mut s, |f| matches!(f, Field::HeaderName(_)));
        handle_normal(&mut s, &mut m, key(KeyCode::Char('d')));
        assert!(matches!(s.mode, Mode::ConfirmDelete(_)));
        assert_eq!(m.headers.len(), 1);
        handle_confirm_delete(&mut s, &mut m, key(KeyCode::Char('n')));
        assert_eq!(s.mode, Mode::Normal);
        assert_eq!(m.headers.len(), 1);
        handle_normal(&mut s, &mut m, key(KeyCode::Char('d')));
        handle_confirm_delete(&mut s, &mut m, key(KeyCode::Char('y')));
        assert_eq!(m.headers.len(), 0);
    }

    #[test]
    fn h_and_l_move_cells() {
        let mut m = model();
        let mut s = UiState::new(&m);
        goto(&mut s, |f| matches!(f, Field::QueryName(_)));
        handle_normal(&mut s, &mut m, key(KeyCode::Enter)); // cell mode
        let first = s.cell.unwrap();
        handle_normal(&mut s, &mut m, key(KeyCode::Char('l')));
        assert!(s.cell.unwrap() > first);
        handle_normal(&mut s, &mut m, key(KeyCode::Char('h')));
        assert_eq!(s.cell.unwrap(), first);
    }

    #[test]
    fn edit_text_cell_commits() {
        let mut m = model();
        let mut s = UiState::new(&m);
        goto(&mut s, |f| matches!(f, Field::Name));
        handle_normal(&mut s, &mut m, key(KeyCode::Enter)); // cell mode
        handle_normal(&mut s, &mut m, key(KeyCode::Enter)); // insert
        assert!(matches!(s.mode, Mode::Insert(_)));
        handle_insert(&mut s, &mut m, key(KeyCode::Char('x')));
        handle_insert(&mut s, &mut m, key(KeyCode::Enter));
        assert_eq!(m.name, "tx");
    }

    #[test]
    fn method_cycles_when_url_expanded() {
        let mut m = model();
        let mut s = UiState::new(&m);
        s.expanded = Some(Expand::Url);
        s.refresh(&m);
        goto(&mut s, |f| matches!(f, Field::Method));
        // focus the method enum cell
        let mi = s
            .current_row()
            .unwrap()
            .cells
            .iter()
            .position(|c| matches!(c.field, Field::Method))
            .unwrap();
        s.cell = Some(mi);
        handle_normal(&mut s, &mut m, key(KeyCode::Enter));
        assert_ne!(method_str(&m.method), "GET");
    }

    #[test]
    fn quit_clean_and_dirty() {
        let mut m = model();
        let mut s = UiState::new(&m);
        s.dirty = false;
        assert_eq!(
            handle_normal(&mut s, &mut m, key(KeyCode::Char('q'))),
            Action::Quit
        );
        s.dirty = true;
        assert_eq!(
            handle_normal(&mut s, &mut m, key(KeyCode::Char('q'))),
            Action::None
        );
    }

    #[test]
    fn save_clears_dirty() {
        let dir = std::env::temp_dir().join("apic_tui_ri_save");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("c.json");
        let m = model();
        let mut s = UiState::new(&m);
        s.dirty = true;
        apply_save(&mut s, &m, &path);
        assert!(!s.dirty);
        assert!(path.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_adds_child_under_object_field() {
        let c = json_get(
            r#"{ "name":"t","method":"POST",
                 "url":{"protocol":"https","host":"h","path":["x"]},"headers":[],
                 "request":{"type":"object","schema":[
                    {"name":"wrap","type":"object","default":null,"description":"d","required":true,
                     "properties":[{"name":"a","type":"string","default":null,"description":"d","required":false}]}
                 ]},
                 "responses":[] }"#,
            None,
        ).unwrap();
        let mut m = EditModel::from_contract(c);
        let mut s = UiState::new(&m);
        // focus the object field "wrap" (top-level schema field, path [0])
        goto(
            &mut s,
            |f| matches!(f, Field::SchemaName(BodyLoc::Request, p) if p == &vec![0]),
        );
        let before = m.request.as_ref().unwrap().schema[0].properties.len();
        handle_normal(&mut s, &mut m, key(KeyCode::Char('a')));
        assert_eq!(
            m.request.as_ref().unwrap().schema[0].properties.len(),
            before + 1
        );
    }

    #[test]
    fn a_adds_sibling_for_non_object_field() {
        let c = json_get(
            r#"{ "name":"t","method":"POST",
                 "url":{"protocol":"https","host":"h","path":["x"]},"headers":[],
                 "request":{"type":"object","schema":[
                    {"name":"s","type":"string","default":null,"description":"d","required":false}
                 ]},
                 "responses":[] }"#,
            None,
        )
        .unwrap();
        let mut m = EditModel::from_contract(c);
        let mut s = UiState::new(&m);
        goto(
            &mut s,
            |f| matches!(f, Field::SchemaName(BodyLoc::Request, p) if p == &vec![0]),
        );
        let before = m.request.as_ref().unwrap().schema.len();
        handle_normal(&mut s, &mut m, key(KeyCode::Char('a')));
        assert_eq!(m.request.as_ref().unwrap().schema.len(), before + 1); // sibling at top level
    }
}
