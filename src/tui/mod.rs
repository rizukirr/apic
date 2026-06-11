//! Interactive terminal UI for creating and editing contracts.
//!
//! The default authoring surface for `apic create` and `apic open`. The
//! external-editor path remains available behind `--editor`.

mod draw;
pub(crate) mod model;
mod rows;
mod seed;
mod state;

pub(crate) use model::EditModel;
use ratatui::style::Style;
use ratatui::widgets::Block;
pub(crate) use seed::seed_model;

use crate::tui::rows::{BodyLoc, Field};
use crate::tui::state::{
    Action, Mode, UiState, apply_save, handle_confirm_delete, handle_confirm_quit, handle_insert,
    handle_normal,
};
// Crossterm is imported via ratatui's re-export (== 0.28) so event/terminal
// types match ratatui and tui-textarea. The root `crossterm` 0.29 crate is used
// only by `picker.rs`; the two never exchange values.
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use std::io::{self, Stdout};
use std::path::Path;
use tui_textarea::TextArea;

/// Restores the terminal (raw mode + alternate screen) on every exit path.
struct TermGuard;

impl TermGuard {
    fn enter() -> Result<Self, String> {
        enable_raw_mode().map_err(|e| format!("enable raw mode: {e}"))?;
        execute!(io::stdout(), EnterAlternateScreen)
            .map_err(|e| format!("enter alt screen: {e}"))?;
        Ok(TermGuard)
    }
}

impl Drop for TermGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

/// Reads the full example buffer for a field straight from the model.
fn example_text(model: &EditModel, field: &Field) -> String {
    match field {
        Field::BodyExample(BodyLoc::Request) => model
            .request
            .as_ref()
            .map(|b| b.example.clone())
            .unwrap_or_default(),
        Field::BodyExample(BodyLoc::Response(i)) => model
            .responses
            .get(*i)
            .map(|r| r.example.clone())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// Writes an edited example buffer back into the model.
fn set_example(model: &mut EditModel, field: &Field, text: String) {
    match field {
        Field::BodyExample(BodyLoc::Request) => {
            if let Some(b) = model.request.as_mut() {
                b.example = text;
            }
        }
        Field::BodyExample(BodyLoc::Response(i)) => {
            if let Some(r) = model.responses.get_mut(*i) {
                r.example = text;
            }
        }
        _ => {}
    }
}

/// Runs the authoring TUI on `model`, writing to `path` on save.
pub(crate) fn run(mut model: EditModel, path: &Path) -> Result<(), String> {
    let _guard = TermGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal: Terminal<CrosstermBackend<Stdout>> =
        Terminal::new(backend).map_err(|e| format!("terminal init: {e}"))?;

    let mut state = UiState::new(&model);
    // Holds the active modal editor and the field it edits, if any.
    let mut modal: Option<(Field, TextArea<'static>)> = None;

    loop {
        terminal
            .draw(|f| {
                draw::draw(f, &state);
                if let Some((_, ta)) = &modal {
                    draw::draw_example_modal(f, ta);
                }
            })
            .map_err(|e| format!("draw: {e}"))?;

        let Event::Key(key) = event::read().map_err(|e| format!("read event: {e}"))? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        // Modal editor takes all keys until closed.
        if let Some((field, ta)) = &mut modal {
            use ratatui::crossterm::event::KeyCode;
            match key.code {
                KeyCode::Esc => {
                    let text = ta.lines().join("\n");
                    set_example(&mut model, field, text);
                    state.dirty = true;
                    state.refresh(&model);
                    modal = None;
                }
                _ => {
                    ta.input(key);
                }
            }
            continue;
        }

        let action = match &state.mode {
            Mode::Normal => handle_normal(&mut state, &mut model, key),
            Mode::Insert(_) => handle_insert(&mut state, &mut model, key),
            Mode::ConfirmQuit => handle_confirm_quit(&mut state, key),
            Mode::ConfirmDelete(_) => handle_confirm_delete(&mut state, &mut model, key),
            Mode::Help => {
                state.mode = Mode::Normal;
                Action::None
            }
            Mode::Example => Action::None,
        };

        match action {
            Action::None => {}
            Action::OpenExample(field, _) => {
                let text = example_text(&model, &field);
                let mut ta =
                    TextArea::from(text.lines().map(|l| l.to_string()).collect::<Vec<_>>());
                ta.set_block(
                    Block::bordered()
                        .title(" JSON Example ")
                        .title_bottom(" Ctrl-S Save • Esc Close "),
                );
                ta.set_line_number_style(Style::default());
                modal = Some((field, ta));
                state.mode = Mode::Example;
            }
            Action::Save => {
                let was_confirm = state.mode == Mode::ConfirmQuit;
                apply_save(&mut state, &model, path);
                if was_confirm {
                    if state.dirty {
                        // save failed; stay open, return to normal so user can fix
                        state.mode = Mode::Normal;
                    } else {
                        break;
                    }
                } else {
                    state.mode = Mode::Normal;
                }
            }
            Action::Quit => break,
        }

        // Leaving Example mode is handled by the modal branch; keep mode synced.
        if modal.is_none() && state.mode == Mode::Example {
            state.mode = Mode::Normal;
        }
    }

    Ok(())
}
