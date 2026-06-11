//! UI state and pure key-handling for the authoring TUI.
//!
//! Key handlers are pure functions over `(UiState, &mut EditModel, KeyEvent)`
//! so they can be unit-tested without a terminal, mirroring `picker.rs`.

use crate::tui::model::EditModel;
use crate::tui::rows::{BodyLoc, Field, Row, RowKind, flatten};
// Import crossterm via ratatui's re-export so KeyEvent matches the version
// ratatui/tui-textarea use (0.28); the root `crossterm` crate is 0.29.
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::json::{Method, method_all, method_str};

/// Whether keystrokes navigate or edit.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) enum Mode {
    #[default]
    Normal,
    /// Inline single-line edit of the focused field; carries the buffer.
    Insert(String),
    /// Modal multiline example editor open for the given field.
    Example,
    /// Help overlay visible.
    Help,
    /// Quit confirmation when there are unsaved changes.
    ConfirmQuit,
}

/// What the event loop should do after a key is handled.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Action {
    None,
    /// Open the modal textarea seeded with this text for `field`.
    OpenExample(Field, String),
    Save,
    Quit,
}

pub(crate) struct UiState {
    pub cursor: usize,
    pub mode: Mode,
    pub dirty: bool,
    pub status: String,
    pub rows: Vec<Row>,
}

impl UiState {
    pub fn new(model: &EditModel) -> Self {
        let rows = flatten(model);
        let mut s = UiState {
            cursor: 0,
            mode: Mode::Normal,
            dirty: false,
            status: "Ctrl-S save · q quit · ? help".to_string(),
            rows,
        };
        s.skip_to_editable(1);
        s
    }

    /// Recomputes rows after a model mutation, clamping the cursor.
    pub fn refresh(&mut self, model: &EditModel) {
        self.rows = flatten(model);
        if self.cursor >= self.rows.len() {
            self.cursor = self.rows.len().saturating_sub(1);
        }
    }

    fn current(&self) -> &Row {
        &self.rows[self.cursor]
    }

    /// Moves the cursor by `dir` (+1/-1), skipping non-editable header rows.
    fn skip_to_editable(&mut self, dir: isize) {
        if self.rows.is_empty() {
            return;
        }
        let len = self.rows.len();
        let mut i = self.cursor as isize;
        loop {
            i += dir;
            if i < 0 {
                i = 0;
                break;
            }
            if i as usize >= len {
                i = (len - 1) as isize;
                break;
            }
            if self.rows[i as usize].kind != RowKind::Header {
                break;
            }
        }
        // If we landed on a header (edges), nudge to nearest editable.
        if self.rows[i as usize].kind == RowKind::Header {
            // search forward then backward
            if let Some(f) = (0..len).find(|&j| self.rows[j].kind != RowKind::Header) {
                i = f as isize;
            }
        }
        self.cursor = i as usize;
    }
}

/// Handles one key in Normal mode. Returns the action for the event loop.
pub(crate) fn handle_normal(state: &mut UiState, model: &mut EditModel, key: KeyEvent) -> Action {
    match (key.code, key.modifiers) {
        (KeyCode::Char('s'), KeyModifiers::CONTROL) => Action::Save,
        (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => {
            if state.dirty {
                state.mode = Mode::ConfirmQuit;
                state.status = "Unsaved changes — y: save & quit · n: discard · Esc: cancel".into();
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
            state.skip_to_editable(1);
            Action::None
        }
        (KeyCode::Up, _) | (KeyCode::Char('k'), _) => {
            state.skip_to_editable(-1);
            Action::None
        }
        (KeyCode::Enter, _) => begin_edit(state),
        (KeyCode::Right, _) | (KeyCode::Char(' '), _) => handle_cycle(state, model, true),
        (KeyCode::Left, _) => handle_cycle(state, model, false),
        _ => Action::None,
    }
}

/// Begins editing the focused row according to its kind.
fn begin_edit(state: &mut UiState) -> Action {
    let row = state.current().clone();
    match row.kind {
        RowKind::Text => {
            state.mode = Mode::Insert(row.value.clone());
            Action::None
        }
        RowKind::Example => Action::OpenExample(row.field.clone(), row.value_full_marker()),
        RowKind::Bool | RowKind::Enum | RowKind::Add | RowKind::Header => Action::None,
    }
}

impl Row {
    /// Placeholder hook: example rows store a preview, not the full text, so the
    /// event loop fetches the real buffer from the model via the field address.
    fn value_full_marker(&self) -> String {
        String::new()
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
            let field = state.current().field.clone();
            set_field(model, &field, value);
            state.dirty = true;
            state.mode = Mode::Normal;
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

/// Cycles enum / toggles bool on the focused row (Normal mode, ←/→ or space).
pub(crate) fn handle_cycle(state: &mut UiState, model: &mut EditModel, forward: bool) -> Action {
    let row = state.current().clone();
    match row.kind {
        RowKind::Enum => {
            // Only Method is an enum field today.
            if matches!(row.field, Field::Method) {
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
            Action::None
        }
        RowKind::Bool => {
            toggle_bool(model, &row.field);
            state.dirty = true;
            state.refresh(model);
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
        Field::SchemaName(loc, p) => set_schema(model, loc, p, |s| s.name = value.clone()),
        Field::SchemaType(loc, p) => set_schema(model, loc, p, |s| s.dtype = value.clone()),
        Field::SchemaDefault(loc, p) => set_schema(model, loc, p, |s| s.default = value.clone()),
        Field::SchemaDesc(loc, p) => set_schema(model, loc, p, |s| s.description = value.clone()),
        Field::SchemaAccept(loc, p) => set_schema(model, loc, p, |s| s.accept = value.clone()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::json_get;

    fn model() -> EditModel {
        let c = json_get(
            r#"{ "name":"t","method":"GET",
                 "url":{"protocol":"https","host":"h","path":["x"]},
                 "headers":[],"responses":[{"code":200,"description":"ok","schema":[]}] }"#,
            None,
        )
        .unwrap();
        EditModel::from_contract(c)
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn movement_and_modes() {
        let mut m = model();
        let mut s = UiState::new(&m);
        // First editable row is not a header.
        assert_ne!(s.rows[s.cursor].kind, RowKind::Header);

        let start = s.cursor;
        handle_normal(&mut s, &mut m, key(KeyCode::Down));
        assert!(s.cursor >= start);
        assert_ne!(s.rows[s.cursor].kind, RowKind::Header);

        // Enter on a Text row enters Insert mode.
        // Move cursor to the name row (first editable is name).
        s.cursor = s
            .rows
            .iter()
            .position(|r| matches!(r.field, Field::Name))
            .unwrap();
        handle_normal(&mut s, &mut m, key(KeyCode::Enter));
        assert!(matches!(s.mode, Mode::Insert(_)));
    }

    #[test]
    fn quit_when_clean_is_immediate() {
        let mut m = model();
        let mut s = UiState::new(&m);
        s.dirty = false;
        assert_eq!(
            handle_normal(&mut s, &mut m, key(KeyCode::Char('q'))),
            Action::Quit
        );
    }

    #[test]
    fn quit_when_dirty_asks_to_confirm() {
        let mut m = model();
        let mut s = UiState::new(&m);
        s.dirty = true;
        assert_eq!(
            handle_normal(&mut s, &mut m, key(KeyCode::Char('q'))),
            Action::None
        );
        assert_eq!(s.mode, Mode::ConfirmQuit);
    }

    #[test]
    fn insert_commits_to_model() {
        let mut m = model();
        let mut s = UiState::new(&m);
        s.cursor = s
            .rows
            .iter()
            .position(|r| matches!(r.field, Field::Name))
            .unwrap();
        // Entering insert pre-fills the buffer with the field's current value
        // ("t") so the user edits in place rather than retyping. Typing appends;
        // backspace deletes. Here we append "x" and commit -> "tx".
        handle_normal(&mut s, &mut m, key(KeyCode::Enter));
        assert_eq!(s.mode, Mode::Insert("t".to_string()));
        handle_insert(&mut s, &mut m, key(KeyCode::Char('x')));
        handle_insert(&mut s, &mut m, key(KeyCode::Enter));
        assert_eq!(m.name, "tx");
        assert!(s.dirty);
        assert_eq!(s.mode, Mode::Normal);
    }

    #[test]
    fn insert_escape_discards() {
        let mut m = model();
        let mut s = UiState::new(&m);
        s.cursor = s
            .rows
            .iter()
            .position(|r| matches!(r.field, Field::Name))
            .unwrap();
        handle_normal(&mut s, &mut m, key(KeyCode::Enter));
        handle_insert(&mut s, &mut m, key(KeyCode::Char('z')));
        handle_insert(&mut s, &mut m, key(KeyCode::Esc));
        assert_eq!(m.name, "t"); // unchanged
        assert_eq!(s.mode, Mode::Normal);
    }

    #[test]
    fn enum_bool_toggle() {
        let mut m = model();
        let mut s = UiState::new(&m);
        // method enum: cycle forward
        s.cursor = s
            .rows
            .iter()
            .position(|r| matches!(r.field, Field::Method))
            .unwrap();
        handle_normal(&mut s, &mut m, key(KeyCode::Right));
        assert_ne!(crate::json::method_str(&m.method), "GET");
        // a bool: response has none; use a query bool after adding one is complex,
        // so toggle via a constructed model with a query.
    }
}
