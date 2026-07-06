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
    Action, Mode, UiState, apply_save, create_response, handle_confirm_delete, handle_confirm_quit,
    handle_insert, handle_normal, remove_response, update_response,
};

/// The two-field response dialog state: status code + short description.
struct ResponseForm {
    status: String,
    description: String,
    /// `false` = editing the status field, `true` = the description field.
    on_description: bool,
    /// `Some(idx)` edits response `idx` in place; `None` creates a new one and
    /// then chains into its JSON editor.
    editing: Option<usize>,
}

/// What the JSON modal writes to on save.
enum ModalTarget {
    /// Edit an existing body's example (request, or a response index).
    Body(Field),
    /// A new response awaiting its first example: created on save only when the
    /// JSON is non-empty, with this status/description.
    NewResponse { status: String, description: String },
}
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
use ratatui_textarea::TextArea;
use std::io::{self, Stdout};
use std::path::Path;

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
            // The request body is only its example: a non-empty save materializes
            // it, an empty save leaves/returns it to `(none)`.
            model.request = if text.trim().is_empty() {
                None
            } else {
                Some(model::EditBody { example: text })
            };
        }
        Field::BodyExample(BodyLoc::Response(i)) => {
            if let Some(r) = model.responses.get_mut(*i) {
                r.example = text;
            }
        }
        _ => {}
    }
}

/// Applies a Ctrl-S save from the JSON modal. An empty example on a response
/// removes that response (status and all); elsewhere it writes the buffer
/// through, materializing a new response only when the example is non-empty.
fn apply_modal_save(state: &mut UiState, model: &mut EditModel, target: ModalTarget, text: String) {
    match target {
        ModalTarget::Body(Field::BodyExample(BodyLoc::Response(i))) if text.trim().is_empty() => {
            remove_response(state, model, i);
        }
        ModalTarget::Body(field) => {
            set_example(model, &field, text);
            state.dirty = true;
            state.refresh(model);
        }
        ModalTarget::NewResponse {
            status,
            description,
        } => {
            if !text.trim().is_empty() {
                let action = create_response(state, model, status, description);
                if let Action::OpenExample(field, _) = action {
                    set_example(model, &field, text);
                    state.refresh(model);
                }
            }
        }
    }
}

/// Builds the bordered JSON-example modal editor seeded with `text`.
fn example_textarea(text: &str) -> TextArea<'static> {
    let mut ta = TextArea::from(text.lines().map(|l| l.to_string()).collect::<Vec<_>>());
    ta.set_block(
        Block::bordered()
            .title(" JSON Example ")
            .title_bottom(" Ctrl-S Save • Ctrl-P Pretty • Esc Cancel "),
    );
    ta.set_line_number_style(Style::default());
    ta
}

/// Runs the authoring TUI on `model`, writing to `path` on save.
pub(crate) fn run(mut model: EditModel, path: &Path) -> Result<(), String> {
    let _guard = TermGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal: Terminal<CrosstermBackend<Stdout>> =
        Terminal::new(backend).map_err(|e| format!("terminal init: {e}"))?;

    let mut state = UiState::new(&model);
    // Holds the active JSON modal editor and what it writes to, if any.
    let mut modal: Option<(ModalTarget, TextArea<'static>)> = None;
    // Holds the response (new / edit) dialog while it is open.
    let mut form: Option<ResponseForm> = None;

    loop {
        terminal
            .draw(|f| {
                draw::draw(f, &state);
                if let Some((_, ta)) = &modal {
                    draw::draw_example_modal(f, ta);
                }
                if let Some(fm) = &form {
                    draw::draw_response_form(
                        f,
                        &fm.status,
                        &fm.description,
                        fm.on_description,
                        fm.editing.is_some(),
                    );
                }
            })
            .map_err(|e| format!("draw: {e}"))?;

        // Block for the next event, then process EVERY event already queued
        // (key repeats, paste, mouse) before looping back to redraw. This
        // coalesces bursts of input into a single render instead of one render
        // per event, which is what makes fast navigation/typing feel snappy.
        let mut next = Some(event::read().map_err(|e| format!("read event: {e}"))?);
        loop {
            if let Some(Event::Key(key)) = next.take()
                && key.kind == KeyEventKind::Press
            {
                // The new-response form takes all keys until confirmed/cancelled.
                if form.is_some() {
                    use ratatui::crossterm::event::KeyCode;
                    match key.code {
                        KeyCode::Esc => form = None,
                        KeyCode::Enter => {
                            let fm = form.take().unwrap();
                            match fm.editing {
                                // Editing: update the response in place, no editor.
                                Some(idx) => update_response(
                                    &mut state,
                                    &mut model,
                                    idx,
                                    fm.status,
                                    fm.description,
                                ),
                                // New: open the JSON editor now; the response is
                                // created on save only if a non-empty example is
                                // written (cancelling creates nothing).
                                None => {
                                    modal = Some((
                                        ModalTarget::NewResponse {
                                            status: fm.status,
                                            description: fm.description,
                                        },
                                        example_textarea(""),
                                    ));
                                    state.mode = Mode::Example;
                                }
                            }
                        }
                        KeyCode::Tab | KeyCode::BackTab | KeyCode::Up | KeyCode::Down => {
                            if let Some(fm) = &mut form {
                                fm.on_description = !fm.on_description;
                            }
                        }
                        KeyCode::Backspace => {
                            if let Some(fm) = &mut form {
                                if fm.on_description {
                                    fm.description.pop();
                                } else {
                                    fm.status.pop();
                                }
                            }
                        }
                        KeyCode::Char(c) => {
                            if let Some(fm) = &mut form {
                                if fm.on_description {
                                    fm.description.push(c);
                                } else if c.is_ascii_digit() && fm.status.chars().count() < 3 {
                                    // Status is a 3-digit HTTP code.
                                    fm.status.push(c);
                                }
                            }
                        }
                        _ => {}
                    }
                } else if modal.is_some() {
                    use ratatui::crossterm::event::{KeyCode, KeyModifiers};
                    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                    match key.code {
                        // Esc cancels: discard the buffer, leave the model as-is.
                        KeyCode::Esc => {
                            modal = None;
                            state.mode = Mode::Normal;
                        }
                        // Ctrl-S applies the JSON changes and closes.
                        KeyCode::Char('s') if ctrl => {
                            let (target, ta) = modal.take().unwrap();
                            let text = ta.lines().join("\n");
                            apply_modal_save(&mut state, &mut model, target, text);
                            state.mode = Mode::Normal;
                        }
                        // Ctrl-P reformats the buffer as pretty-printed JSON.
                        KeyCode::Char('p') if ctrl => {
                            if let Some((_, ta)) = &mut modal {
                                let text = ta.lines().join("\n");
                                *ta = example_textarea(&apic_core::json::pretty_json(&text));
                            }
                        }
                        _ => {
                            if let Some((_, ta)) = &mut modal {
                                ta.input(key);
                            }
                        }
                    }
                } else {
                    let action = match &state.mode {
                        Mode::Normal => handle_normal(&mut state, &mut model, key),
                        Mode::Insert(_) => handle_insert(&mut state, &mut model, key),
                        Mode::ConfirmQuit => handle_confirm_quit(&mut state, key),
                        Mode::ConfirmDelete(_) => {
                            handle_confirm_delete(&mut state, &mut model, key)
                        }
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
                            modal = Some((ModalTarget::Body(field), example_textarea(&text)));
                            state.mode = Mode::Example;
                        }
                        Action::NewResponse => {
                            form = Some(ResponseForm {
                                status: String::new(),
                                description: String::new(),
                                on_description: false,
                                editing: None,
                            });
                        }
                        Action::EditResponse(idx) => {
                            if let Some(r) = model.responses.get(idx) {
                                form = Some(ResponseForm {
                                    status: r.code.clone(),
                                    description: r.description.clone(),
                                    on_description: false,
                                    editing: Some(idx),
                                });
                            }
                        }
                        Action::Save => {
                            let was_confirm = state.mode == Mode::ConfirmQuit;
                            apply_save(&mut state, &model, path);
                            if was_confirm {
                                if state.dirty {
                                    // save failed; stay open so the user can fix
                                    state.mode = Mode::Normal;
                                } else {
                                    return Ok(());
                                }
                            } else {
                                state.mode = Mode::Normal;
                            }
                        }
                        Action::Quit => return Ok(()),
                    }

                    // Leaving Example mode is handled by the modal branch.
                    if modal.is_none() && state.mode == Mode::Example {
                        state.mode = Mode::Normal;
                    }
                }
            }

            // Pull the next already-queued event without blocking; once the
            // queue is drained, break out and redraw a single time.
            if event::poll(std::time::Duration::from_millis(0)).map_err(|e| format!("poll: {e}"))? {
                next = Some(event::read().map_err(|e| format!("read event: {e}"))?);
            } else {
                break;
            }
        }
    }
}
