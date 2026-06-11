//! UI state and pure key-handling for the authoring TUI.
//!
//! Key handlers are pure functions over `(UiState, &mut EditModel, KeyEvent)`
//! so they can be unit-tested without a terminal, mirroring `picker.rs`.

use crate::tui::model::EditModel;
use crate::tui::rows::{BodyLoc, Field, Row, RowKind, flatten};
// Import crossterm via ratatui's re-export so KeyEvent matches the version
// ratatui/tui-textarea use (0.28); the root `crossterm` crate is 0.29.
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
pub(crate) fn handle_normal(state: &mut UiState, _model: &mut EditModel, key: KeyEvent) -> Action {
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
}
