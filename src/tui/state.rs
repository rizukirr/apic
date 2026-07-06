//! UI state and pure key-handling for the table-based authoring TUI.
//!
//! Key handlers are pure functions over `(UiState, &mut EditModel, KeyEvent)` so
//! they are unit-testable without a terminal. The cursor is two-level:
//! `cell: None` selects a whole table row; `cell: Some(c)` edits a cell.

use crate::tui::model::{EditModel, EditResponse};
use crate::tui::rows::{BodyLoc, CellKind, Field, RowKind, Section, TableRow, flatten};
use apic_core::edit::{EditAction, apply};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
    /// Open the two-field "new response" dialog (status + short description).
    NewResponse,
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

    /// The active RESPONSE tab: which response's example the section shows and
    /// which tab is highlighted. Clamped to the response count on refresh.
    pub resp: usize,

    /// Baseline snapshot (last loaded/saved model) against which `dirty` is
    /// computed, so the unsaved indicator reflects real differences.
    pub original: EditModel,
}

impl UiState {
    pub(super) fn new(model: &EditModel) -> Self {
        let sections = flatten(model, 0);
        let mut s = UiState {
            sections,
            sec: 0,
            row: 0,
            cell: None,
            mode: Mode::Normal,
            dirty: false,
            status: HINT.to_string(),
            resp: 0,
            original: model.clone(),
        };
        s.snap_to_first_row();
        s
    }

    /// Rebuilds sections after a mutation, clamping the cursor; drops cell focus
    /// if it no longer addresses a valid cell.
    pub(super) fn refresh(&mut self, model: &EditModel) {
        // Keep the active response tab in range as responses are added/deleted.
        self.resp = self.resp.min(model.responses.len().saturating_sub(1));
        self.sections = flatten(model, self.resp);
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
        self.dirty = model != &self.original;
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

    pub(super) fn current_row(&self) -> Option<&TableRow> {
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

    pub(super) fn focused_field_pub(&self) -> Option<Field> {
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
    // On the response tab strip `d` removes the active tab, not always the first.
    if row.kind == RowKind::RespTabs {
        return Some(Field::ResponseCode(state.resp));
    }
    row.cells
        .iter()
        .find(|c| c.kind != CellKind::Label)
        .or_else(|| row.cells.first())
        .map(|c| c.field.clone())
}

/// Whether `delete_row` would actually remove a row for this field — matching
/// exactly the variants it handles (query/header/response).
fn is_deletable(field: &Field) -> bool {
    matches!(
        field,
        Field::QueryName(_)
            | Field::QueryValue(_)
            | Field::QueryDesc(_)
            | Field::HeaderName(_)
            | Field::HeaderValue(_)
            | Field::ResponseCode(_)
            | Field::ResponseDesc(_)
            | Field::ResponseHeaderName(_, _)
            | Field::ResponseHeaderValue(_, _)
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
        (KeyCode::Char('a'), _) => append_here(state, model),
        (KeyCode::Char('e'), _) => edit_example_here(state),
        (KeyCode::Char('d'), _) => {
            // On an example row `d` clears the JSON; elsewhere it deletes the row.
            let example_field = state
                .current_row()
                .filter(|r| r.kind == RowKind::Example)
                .map(|r| r.cells[0].field.clone());
            if let Some(f) = example_field {
                state.mode = Mode::ConfirmDelete(f);
            } else if let Some(f) = delete_field(state)
                && is_deletable(&f)
            {
                state.mode = Mode::ConfirmDelete(f);
            }
            Action::None
        }
        _ => Action::None,
    }
}

/// Opens the JSON example editor for the request/response body the cursor is in,
/// so an example can be written even when it is empty. Finds the body from the
/// current section's inline example row. Returns `Action::None` when the cursor
/// is not inside a body.
fn edit_example_here(state: &UiState) -> Action {
    let Some(section) = state.sections.get(state.sec) else {
        return Action::None;
    };
    for row in &section.rows {
        if let Some(Field::BodyExample(loc)) = row.cells.first().map(|c| &c.field) {
            return Action::OpenExample(Field::BodyExample(loc.clone()), String::new());
        }
    }
    Action::None
}

/// Handles `a` for the current section. QUERY/HEADERS append a row and focus its
/// name in insert mode. REQUEST opens the JSON editor (creating the body first if
/// absent). RESPONSE adds a new tab, makes it active, and drops the cursor into
/// its code cell in insert mode so the status can be typed straight away.
fn append_here(state: &mut UiState, model: &mut EditModel) -> Action {
    let Some(target) = state.sections.get(state.sec).and_then(|s| s.add.clone()) else {
        return Action::None;
    };
    if target == Field::RequestToggle {
        // `a` opens the JSON editor. The body is materialized only when a real
        // example is saved (see `set_example`), so cancelling leaves it `(none)`.
        return Action::OpenExample(Field::BodyExample(BodyLoc::Request), String::new());
    }
    if target == Field::ResponseAdd {
        // A response is created through the two-step form (status/title, then
        // the JSON editor), not appended inline.
        return Action::NewResponse;
    }
    add_row(state, model, &target);
    if let Some(nf) = new_name_field(model, &target) {
        focus_and_insert(state, &nf);
    }
    Action::None
}

/// Creates a response from the new-response form: `status` (defaulting to 200
/// when blank) and `description`. Makes it the active tab and returns the action
/// to open its JSON example editor.
pub(crate) fn create_response(
    state: &mut UiState,
    model: &mut EditModel,
    status: String,
    description: String,
) -> Action {
    let mut r = EditResponse::blank();
    if !status.trim().is_empty() {
        r.code = status;
    }
    r.description = description;
    model.responses.push(r);
    let idx = model.responses.len() - 1;
    state.resp = idx;
    state.dirty = true;
    state.refresh(model);
    Action::OpenExample(Field::BodyExample(BodyLoc::Response(idx)), String::new())
}

/// The "name" field of the just-added entity for `target`, used to auto-focus
/// and enter insert mode after `add_row`. Returns `None` for `RequestToggle`
/// (no name) or when the add did not produce a row.
fn new_name_field(model: &EditModel, target: &Field) -> Option<Field> {
    match target {
        Field::QueryAdd => model.query.len().checked_sub(1).map(Field::QueryName),
        Field::HeaderAdd => model.headers.len().checked_sub(1).map(Field::HeaderName),
        Field::ResponseAdd => model
            .responses
            .len()
            .checked_sub(1)
            .map(Field::ResponseCode),
        _ => None,
    }
}

/// Focuses the row+cell whose field equals `name_field` and enters insert mode
/// seeded with that cell's current value (empty for a fresh row). No-op if not
/// found.
fn focus_and_insert(state: &mut UiState, name_field: &Field) {
    for (si, sec) in state.sections.iter().enumerate() {
        for (ri, row) in sec.rows.iter().enumerate() {
            if let Some(ci) = row.cells.iter().position(|c| &c.field == name_field) {
                state.sec = si;
                state.row = ri;
                state.cell = Some(ci);
                state.mode = Mode::Insert(row.cells[ci].value.clone());
                return;
            }
        }
    }
}

/// Keys while a cell is focused (cell-edit mode).
fn handle_cell(state: &mut UiState, model: &mut EditModel, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => {
            state.cell = None;
            Action::None
        }
        KeyCode::Left | KeyCode::Char('h') => {
            move_cell_synced(state, model, -1);
            Action::None
        }
        KeyCode::Right | KeyCode::Char('l') => {
            move_cell_synced(state, model, 1);
            Action::None
        }
        KeyCode::Char(' ') => {
            if let (Some(c), Some(field)) = (state.cell, state.focused_field()) {
                let is_bool = state
                    .current_row()
                    .and_then(|r| r.cells.get(c))
                    .map(|cell| cell.kind == CellKind::Bool)
                    .unwrap_or(false);
                if is_bool {
                    apply(model, &EditAction::ToggleBool { field });
                    state.dirty = true;
                    state.refresh(model);
                }
            }
            Action::None
        }
        KeyCode::Char('i') => {
            if let Some(cell) = state
                .cell
                .and_then(|c| state.current_row().and_then(|r| r.cells.get(c)))
                && cell.kind == CellKind::Text
            {
                state.mode = Mode::Insert(cell.value.clone());
            }
            Action::None
        }
        KeyCode::Enter => begin_cell_edit(state, model),
        _ => Action::None,
    }
}

/// Moves the focused cell, and — on the response tab strip — makes the tab under
/// the new cell active so the example beneath the strip follows the selection.
fn move_cell_synced(state: &mut UiState, model: &mut EditModel, dir: isize) {
    state.move_cell(dir);
    if matches!(
        state.current_row().map(|r| &r.kind),
        Some(RowKind::RespTabs)
    ) && let Some(c) = state.cell
    {
        // Cells are [code, desc] per response, so the tab index is `c / 2`.
        let tab = c / 2;
        if tab != state.resp {
            state.resp = tab;
            state.refresh(model);
        }
    }
}

/// Enter on a selected row (row-select mode). For editable rows this selects
/// the first cell and begins editing it immediately (no second keypress).
fn begin_row(state: &mut UiState, model: &mut EditModel) -> Action {
    let Some(row) = state.current_row().cloned() else {
        return Action::None;
    };
    match row.kind {
        RowKind::UrlLine => {
            // Focus the method cell (highlighted, not yet cycled) so a stray
            // Enter never flips the method. A second Enter cycles it; `l` moves
            // to the url cell, where Enter/i edits it inline.
            if let Some(mi) = row.cells.iter().position(|c| c.field == Field::Method) {
                state.cell = Some(mi);
            }
            Action::None
        }
        RowKind::Title => Action::None,
        RowKind::RespTabs => {
            // Focus the active response's code cell (highlighted, not editing);
            // `h`/`l` switch tabs, Enter/i edits the focused code or title.
            let cell = row
                .cells
                .iter()
                .position(|c| c.field == Field::ResponseCode(state.resp))
                .or(Some(0));
            state.cell = cell;
            Action::None
        }
        RowKind::Example => Action::OpenExample(row.cells[0].field.clone(), String::new()),
        RowKind::Name | RowKind::Desc | RowKind::Field => {
            // Select the first editable cell and drop straight into editing it
            // (insert for text, toggle/cycle for bool/enum) so the user can
            // start typing without a second Enter.
            if let Some(&first) = state.editable_cells().first() {
                state.cell = Some(first);
                return begin_cell_edit(state, model);
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
            // Method is the only enum field today.
            cycle_method(state, model, true);
            Action::None
        }
        CellKind::Bool => {
            apply(
                model,
                &EditAction::ToggleBool {
                    field: cell.field.clone(),
                },
            );
            state.dirty = true;
            state.refresh(model);
            Action::None
        }
        CellKind::Label => Action::None,
    }
}

/// Cycles the method enum forward/back (the only enum field today).
fn cycle_method(state: &mut UiState, model: &mut EditModel, forward: bool) {
    apply(model, &EditAction::CycleMethod { forward });
    state.dirty = true;
    state.refresh(model);
}

fn add_row(state: &mut UiState, model: &mut EditModel, field: &Field) {
    if apply(
        model,
        &EditAction::Add {
            field: field.clone(),
        },
    ) {
        state.dirty = true;
        state.cell = None;
        state.refresh(model);
    }
}

fn delete_row(state: &mut UiState, model: &mut EditModel, field: &Field) {
    if apply(
        model,
        &EditAction::Delete {
            field: field.clone(),
        },
    ) {
        state.dirty = true;
        state.cell = None;
        state.refresh(model);
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
            let field = state.focused_field_pub();
            if let Some(f) = &field {
                apply(
                    model,
                    &EditAction::SetText {
                        field: f.clone(),
                        value,
                    },
                );
                state.dirty = true;
            }
            state.mode = Mode::Normal;
            // Name and Description are single-cell rows: commit returns to row
            // focus instead of parking the cursor in a pointless cell focus.
            if matches!(field, Some(Field::Name | Field::Description)) {
                state.cell = None;
            }
            state.refresh(model);
            Action::None
        }
        // Tab / Shift-Tab commit the current value, then jump to the next /
        // previous cell — and keep typing there when it is a text cell, for a
        // fast tab-through-the-row entry flow.
        KeyCode::Tab | KeyCode::BackTab => {
            let value = buf.clone();
            let dir = if key.code == KeyCode::BackTab { -1 } else { 1 };
            if let Some(field) = state.focused_field_pub() {
                apply(model, &EditAction::SetText { field, value });
                state.dirty = true;
            }
            state.mode = Mode::Normal;
            state.refresh(model);
            state.move_cell(dir);
            if let Some(c) = state.cell
                && let Some(cell) = state.current_row().and_then(|r| r.cells.get(c))
                && cell.kind == CellKind::Text
            {
                state.mode = Mode::Insert(cell.value.clone());
            }
            Action::None
        }
        KeyCode::Esc => {
            // Name and Description never rest in cell focus: Esc out of editing
            // returns to row focus, same as committing with Enter.
            if matches!(
                state.focused_field_pub(),
                Some(Field::Name | Field::Description)
            ) {
                state.cell = None;
            }
            state.mode = Mode::Normal;
            Action::None
        }
        _ => Action::None,
    }
}

/// Saves the model to `path`, updating dirty flag and status line.
pub(crate) fn apply_save(state: &mut UiState, model: &EditModel, path: &Path) {
    match model.save(path) {
        Ok(()) => {
            state.original = model.clone();
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
                // An example body is cleared in place; every other field is a row.
                if matches!(f, Field::BodyExample(_)) {
                    if clear_example(model, &f) {
                        state.dirty = true;
                        state.refresh(model);
                    }
                } else {
                    delete_row(state, model, &f);
                }
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

/// Clears a request/response example body. Returns `false` when the field is not
/// a body example or the body is missing.
fn clear_example(model: &mut EditModel, field: &Field) -> bool {
    match field {
        // A request body is only its example, so clearing it removes the body
        // entirely and the section falls back to `(none)`.
        Field::BodyExample(BodyLoc::Request) => model.request.take().is_some(),
        Field::BodyExample(BodyLoc::Response(i)) => model
            .responses
            .get_mut(*i)
            .map(|r| r.example.clear())
            .is_some(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::rows::BodyLoc;
    use apic_core::json::{json_get, method_str};

    fn model() -> EditModel {
        let c = json_get(
            r#"{ "name":"t","description":"d","method":"GET",
                 "url":"https://h/x",
                 "query":[{"name":"page","value":"1","description":"d"}],
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
    fn enter_on_url_line_focuses_method() {
        let mut m = model();
        let mut s = UiState::new(&m);
        goto(&mut s, |f| matches!(f, Field::Method)); // url line carries Method + Url
        handle_normal(&mut s, &mut m, key(KeyCode::Enter));
        // Enter focuses the method cell without cycling it, so the method is
        // unchanged until a further Enter.
        assert!(matches!(s.focused_field_pub(), Some(Field::Method)));
        assert_eq!(s.mode, Mode::Normal);
        assert_eq!(method_str(&m.method), "GET");
    }

    #[test]
    fn enter_on_name_row_goes_straight_to_insert() {
        let mut m = model();
        let mut s = UiState::new(&m);
        goto(&mut s, |f| matches!(f, Field::Name));
        handle_normal(&mut s, &mut m, key(KeyCode::Enter)); // -> insert directly
        assert!(s.cell.is_some(), "first cell is selected");
        assert!(matches!(s.mode, Mode::Insert(_)), "no second Enter needed");
    }

    #[test]
    fn name_commit_returns_to_row_focus() {
        let mut m = model();
        let mut s = UiState::new(&m);
        goto(&mut s, |f| matches!(f, Field::Name));
        handle_normal(&mut s, &mut m, key(KeyCode::Enter)); // insert ("t")
        handle_insert(&mut s, &mut m, key(KeyCode::Char('x')));
        handle_insert(&mut s, &mut m, key(KeyCode::Enter)); // commit "tx"
        assert_eq!(m.name, "tx");
        assert!(s.cell.is_none(), "Name drops back to row focus on commit");
    }

    #[test]
    fn name_esc_returns_to_row_focus() {
        let mut m = model();
        let mut s = UiState::new(&m);
        goto(&mut s, |f| matches!(f, Field::Name));
        handle_normal(&mut s, &mut m, key(KeyCode::Enter)); // insert
        handle_insert(&mut s, &mut m, key(KeyCode::Esc)); // cancel editing
        assert_eq!(s.mode, Mode::Normal);
        assert!(s.cell.is_none(), "Name drops back to row focus on Esc too");
    }

    #[test]
    fn field_row_esc_keeps_cell_focus() {
        let mut m = model();
        let mut s = UiState::new(&m);
        goto(&mut s, |f| matches!(f, Field::QueryName(_)));
        handle_normal(&mut s, &mut m, key(KeyCode::Enter)); // insert
        handle_insert(&mut s, &mut m, key(KeyCode::Esc)); // cancel editing
        assert!(
            s.cell.is_some(),
            "multi-cell row falls back to cell focus on Esc"
        );
    }

    #[test]
    fn field_row_commit_keeps_cell_focus() {
        let mut m = model();
        let mut s = UiState::new(&m);
        goto(&mut s, |f| matches!(f, Field::QueryName(_)));
        handle_normal(&mut s, &mut m, key(KeyCode::Enter)); // insert directly
        assert!(matches!(s.mode, Mode::Insert(_)));
        handle_insert(&mut s, &mut m, key(KeyCode::Char('X')));
        handle_insert(&mut s, &mut m, key(KeyCode::Enter)); // commit
        assert!(
            s.cell.is_some(),
            "multi-cell row stays in cell focus after commit"
        );
    }

    #[test]
    fn i_enters_insert_on_text_cell() {
        let mut m = model();
        let mut s = UiState::new(&m);
        goto(&mut s, |f| matches!(f, Field::QueryName(_)));
        handle_normal(&mut s, &mut m, key(KeyCode::Enter)); // insert directly
        handle_insert(&mut s, &mut m, key(KeyCode::Esc)); // -> cell focus
        assert!(s.cell.is_some());
        assert_eq!(s.mode, Mode::Normal);
        handle_normal(&mut s, &mut m, key(KeyCode::Char('i'))); // i -> insert again
        assert!(matches!(s.mode, Mode::Insert(_)));
    }

    #[test]
    fn response_code_is_editable_inline_on_the_tab_strip() {
        let mut m = model();
        let mut s = UiState::new(&m);
        // The code cell lives on the RESPONSE tab strip — no expansion needed.
        goto(&mut s, |f| matches!(f, Field::ResponseCode(0)));
        handle_normal(&mut s, &mut m, key(KeyCode::Enter)); // focus the code cell
        assert!(matches!(
            s.focused_field_pub(),
            Some(Field::ResponseCode(0))
        ));
        handle_normal(&mut s, &mut m, key(KeyCode::Enter)); // insert (prefilled "200")
        handle_insert(&mut s, &mut m, key(KeyCode::Backspace));
        handle_insert(&mut s, &mut m, key(KeyCode::Char('1')));
        handle_insert(&mut s, &mut m, key(KeyCode::Enter));
        assert_eq!(m.responses[0].code, "201");
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
    fn a_auto_enters_insert_on_new_name() {
        let mut m = model();
        let mut s = UiState::new(&m);
        goto(&mut s, |f| matches!(f, Field::HeaderName(_)));
        handle_normal(&mut s, &mut m, key(KeyCode::Char('a')));
        assert_eq!(m.headers.len(), 2);
        assert!(matches!(s.mode, Mode::Insert(_)));
        assert!(matches!(s.focused_field_pub(), Some(Field::HeaderName(1))));
        // typing then Enter commits to the new header name and keeps cell focus
        handle_insert(&mut s, &mut m, key(KeyCode::Char('X')));
        handle_insert(&mut s, &mut m, key(KeyCode::Enter));
        assert_eq!(m.headers[1].name, "X");
        assert!(s.cell.is_some());
    }

    #[test]
    fn d_deletes_focused_row() {
        let mut m = model();
        let mut s = UiState::new(&m);
        goto(&mut s, |f| matches!(f, Field::QueryName(_)));
        handle_normal(&mut s, &mut m, key(KeyCode::Char('d')));
        handle_confirm_delete(&mut s, &mut m, key(KeyCode::Char('y')));
        assert_eq!(m.query.len(), 0);
    }

    #[test]
    fn d_on_the_example_row_clears_the_body_not_the_response() {
        let mut m = model();
        m.responses[0].example = r#"{"status":200}"#.to_string();
        let mut s = UiState::new(&m);
        assert!(!m.responses[0].example.trim().is_empty());
        goto(&mut s, |f| matches!(f, Field::BodyExample(_)));
        handle_normal(&mut s, &mut m, key(KeyCode::Char('d')));
        assert!(matches!(s.mode, Mode::ConfirmDelete(_)));
        handle_confirm_delete(&mut s, &mut m, key(KeyCode::Char('y')));
        assert!(m.responses[0].example.is_empty(), "the example is cleared");
        assert_eq!(m.responses.len(), 1, "the response itself stays");
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
        handle_normal(&mut s, &mut m, key(KeyCode::Enter)); // insert directly
        handle_insert(&mut s, &mut m, key(KeyCode::Esc)); // -> cell focus
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
        handle_normal(&mut s, &mut m, key(KeyCode::Enter)); // insert directly
        assert!(matches!(s.mode, Mode::Insert(_)));
        handle_insert(&mut s, &mut m, key(KeyCode::Char('x')));
        handle_insert(&mut s, &mut m, key(KeyCode::Enter));
        assert_eq!(m.name, "tx");
    }

    #[test]
    fn tab_commits_and_jumps_to_next_text_cell_in_insert() {
        let c = json_get(
            r#"{ "name":"t","method":"GET",
                 "url":"https://h/x",
                 "query":[{"name":"page","value":"1","description":"d"}],
                 "headers":[],"responses":[] }"#,
            None,
        )
        .unwrap();
        let mut m = EditModel::from_contract(c);
        let mut s = UiState::new(&m);
        goto(&mut s, |f| matches!(f, Field::QueryName(_)));
        handle_normal(&mut s, &mut m, key(KeyCode::Enter)); // insert (prefilled "page")
        assert!(matches!(s.mode, Mode::Insert(_)));
        handle_insert(&mut s, &mut m, key(KeyCode::Tab)); // commit + jump to the value cell
        assert_eq!(m.query[0].name, "page"); // committed unchanged
        assert!(matches!(s.focused_field_pub(), Some(Field::QueryValue(_))));
        assert!(
            matches!(s.mode, Mode::Insert(_)),
            "stays in insert on the next text cell"
        );
        // typing continues into the value cell (prefilled "1")
        handle_insert(&mut s, &mut m, key(KeyCode::Char('2')));
        handle_insert(&mut s, &mut m, key(KeyCode::Enter));
        assert_eq!(m.query[0].value, "12");
    }

    #[test]
    fn method_cycles_on_collapsed_url_line() {
        let mut m = model();
        let mut s = UiState::new(&m);
        goto(&mut s, |f| matches!(f, Field::Method));
        // focus the method enum cell on the collapsed url line
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
    fn e_key_opens_example_editor_for_request_body() {
        let c = json_get(
            r#"{ "name":"t","method":"POST",
                 "url":"https://h/x","headers":[],
                 "request":{"example":{"a":1}},
                 "responses":[] }"#,
            None,
        )
        .unwrap();
        let mut m = EditModel::from_contract(c);
        let mut s = UiState::new(&m);
        goto(&mut s, |f| {
            matches!(f, Field::BodyExample(BodyLoc::Request))
        });
        s.cell = None;
        let action = handle_normal(&mut s, &mut m, key(KeyCode::Char('e')));
        assert_eq!(
            action,
            Action::OpenExample(Field::BodyExample(BodyLoc::Request), String::new())
        );
    }

    #[test]
    fn e_key_is_noop_when_cursor_not_in_a_body() {
        // Default focus is the endpoint section (not a body): `e` does nothing.
        let c = json_get(
            r#"{ "name":"t","method":"POST",
                 "url":"https://h/x","headers":[],
                 "responses":[] }"#,
            None,
        )
        .unwrap();
        let mut m = EditModel::from_contract(c);
        let mut s = UiState::new(&m);
        let action = handle_normal(&mut s, &mut m, key(KeyCode::Char('e')));
        assert_eq!(action, Action::None);
    }

    #[test]
    fn e_key_opens_example_editor_for_response_body() {
        let c = json_get(
            r#"{ "name":"t","method":"POST",
                 "url":"https://h/x","headers":[],
                 "responses":[{"code":200,"description":"ok","example":{"a":1}}] }"#,
            None,
        )
        .unwrap();
        let mut m = EditModel::from_contract(c);
        let mut s = UiState::new(&m);
        goto(&mut s, |f| {
            matches!(f, Field::BodyExample(BodyLoc::Response(0)))
        });
        s.cell = None;
        let action = handle_normal(&mut s, &mut m, key(KeyCode::Char('e')));
        assert_eq!(
            action,
            Action::OpenExample(Field::BodyExample(BodyLoc::Response(0)), String::new())
        );
    }

    #[test]
    fn a_on_empty_query_title_adds_first_item() {
        // model() has no query — the QUERY section is empty
        let c = json_get(
            r#"{ "name":"t","method":"GET",
                 "url":"https://h/x",
                 "headers":[],"responses":[] }"#,
            None,
        )
        .unwrap();
        let mut m = EditModel::from_contract(c);
        let mut s = UiState::new(&m);
        // land on the QUERY title row
        let (si, ri) = s
            .sections
            .iter()
            .enumerate()
            .find_map(|(si, sec)| {
                (sec.title == "QUERY").then(|| {
                    (
                        si,
                        sec.rows
                            .iter()
                            .position(|r| r.kind == RowKind::Title)
                            .unwrap(),
                    )
                })
            })
            .unwrap();
        s.sec = si;
        s.row = ri;
        s.cell = None;
        // Enter does nothing on the title
        handle_normal(&mut s, &mut m, key(KeyCode::Enter));
        assert_eq!(m.query.len(), 0);
        // a adds the first query
        handle_normal(&mut s, &mut m, key(KeyCode::Char('a')));
        assert_eq!(m.query.len(), 1);
    }

    #[test]
    fn a_on_response_requests_the_new_response_form() {
        let mut m = model();
        let mut s = UiState::new(&m);
        // Anywhere in the RESPONSE section, `a` opens the form (no immediate add).
        goto(&mut s, |f| matches!(f, Field::ResponseCode(0)));
        let action = handle_normal(&mut s, &mut m, key(KeyCode::Char('a')));
        assert_eq!(action, Action::NewResponse);
        assert_eq!(
            m.responses.len(),
            1,
            "nothing is added until the form confirms"
        );
    }

    #[test]
    fn create_response_adds_active_tab_and_opens_its_example() {
        let mut m = model();
        let mut s = UiState::new(&m);
        let before = m.responses.len();
        let action = create_response(&mut s, &mut m, "404".into(), "not found".into());
        assert_eq!(m.responses.len(), before + 1);
        let new = m.responses.last().unwrap();
        assert_eq!(new.code, "404");
        assert_eq!(new.description, "not found");
        assert_eq!(s.resp, before, "the new tab becomes active");
        assert_eq!(
            action,
            Action::OpenExample(Field::BodyExample(BodyLoc::Response(before)), String::new())
        );
    }

    #[test]
    fn create_response_defaults_blank_status_to_200() {
        let mut m = model();
        let mut s = UiState::new(&m);
        create_response(&mut s, &mut m, String::new(), String::new());
        assert_eq!(m.responses.last().unwrap().code, "200");
    }

    #[test]
    fn a_on_request_opens_editor_without_creating_the_body() {
        let c = json_get(
            r#"{ "name":"t","method":"POST","url":"https://h/x","headers":[],"responses":[] }"#,
            None,
        )
        .unwrap();
        let mut m = EditModel::from_contract(c);
        assert!(m.request.is_none());
        let mut s = UiState::new(&m);
        // Land on the REQUEST section and press `a`.
        let si = s
            .sections
            .iter()
            .position(|sec| sec.add == Some(Field::RequestToggle))
            .unwrap();
        s.sec = si;
        s.row = 0;
        s.cell = None;
        let action = handle_normal(&mut s, &mut m, key(KeyCode::Char('a')));
        // The body is not materialized here — cancelling the editor must leave
        // REQUEST as (none). It is created only when a real example is saved.
        assert!(m.request.is_none(), "no body until an example is saved");
        assert_eq!(
            action,
            Action::OpenExample(Field::BodyExample(BodyLoc::Request), String::new())
        );
    }

    #[test]
    fn clearing_the_request_example_drops_the_body_to_none() {
        let c = json_get(
            r#"{ "name":"t","method":"POST","url":"https://h/x","headers":[],
                 "request":{"example":{"a":1}},"responses":[] }"#,
            None,
        )
        .unwrap();
        let mut m = EditModel::from_contract(c);
        assert!(m.request.is_some());
        let mut s = UiState::new(&m);
        goto(&mut s, |f| {
            matches!(f, Field::BodyExample(BodyLoc::Request))
        });
        handle_normal(&mut s, &mut m, key(KeyCode::Char('d')));
        handle_confirm_delete(&mut s, &mut m, key(KeyCode::Char('y')));
        assert!(
            m.request.is_none(),
            "clearing the request example removes the body"
        );
    }

    #[test]
    fn dirty_tracks_real_changes_against_baseline() {
        let mut m = model(); // name == "t"
        let mut s = UiState::new(&m);
        assert!(!s.dirty);
        // edit name "t" -> "tx"
        goto(&mut s, |f| matches!(f, Field::Name));
        handle_normal(&mut s, &mut m, key(KeyCode::Enter)); // insert (prefilled "t")
        handle_insert(&mut s, &mut m, key(KeyCode::Char('x')));
        handle_insert(&mut s, &mut m, key(KeyCode::Enter));
        assert!(s.dirty);
        // revert "tx" -> "t" clears dirty
        handle_normal(&mut s, &mut m, key(KeyCode::Enter)); // insert (prefilled "tx")
        handle_insert(&mut s, &mut m, key(KeyCode::Backspace));
        handle_insert(&mut s, &mut m, key(KeyCode::Enter));
        assert!(!s.dirty, "reverting to the original value clears dirty");
    }

    #[test]
    fn save_updates_baseline() {
        let dir = std::env::temp_dir().join("apic_tui_baseline");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("c.json");
        let mut m = model();
        let mut s = UiState::new(&m);
        goto(&mut s, |f| matches!(f, Field::Name));
        handle_normal(&mut s, &mut m, key(KeyCode::Enter));
        handle_insert(&mut s, &mut m, key(KeyCode::Char('z')));
        handle_insert(&mut s, &mut m, key(KeyCode::Enter));
        assert!(s.dirty);
        apply_save(&mut s, &m, &path);
        assert!(!s.dirty);
        // a subsequent refresh with no change keeps it clean (baseline moved)
        s.refresh(&m);
        assert!(!s.dirty);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
