//! Inline interactive picker for disambiguating contract references.
//!
//! Renders a small arrow-key list in place (no alternate screen, scroll-back
//! preserved). Key handling is a pure function so it is testable without a
//! terminal; raw mode is held behind an RAII guard so it is restored on every
//! exit path, including panics.

use crossterm::cursor::{MoveToColumn, MoveUp};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, read};
use crossterm::style::Stylize;
use crossterm::terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode};
use crossterm::{execute, queue};
use std::io::{self, Write};

/// Terminal decision of one picker interaction.
#[derive(Debug, PartialEq)]
enum Outcome {
    /// The user confirmed the candidate at this index.
    Picked(usize),

    /// The user cancelled (Esc, `q`, or Ctrl-C).
    Cancelled,
}

/// Holds the terminal in raw mode for its lifetime.
///
/// Raw mode is restored in `Drop`, so every exit path — confirm, cancel, IO
/// error, panic — leaves the terminal usable.
struct RawGuard;

impl RawGuard {
    fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        Ok(RawGuard)
    }
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

/// Draws (or redraws) the prompt and the candidate list in place.
///
/// On a redraw the cursor is first moved back up over the previously drawn
/// block. Lines end with `\r\n` because the terminal is in raw mode.
fn draw(
    out: &mut impl Write,
    prompt: &str,
    labels: &[String],
    selected: usize,
    redraw: bool,
) -> io::Result<()> {
    if redraw {
        queue!(out, MoveUp(labels.len() as u16 + 1))?;
    }
    queue!(out, MoveToColumn(0), Clear(ClearType::FromCursorDown))?;
    write!(out, "{prompt}\r\n")?;
    for (i, label) in labels.iter().enumerate() {
        if i == selected {
            write!(out, "{}\r\n", format!("> {}. {label}", i + 1).bold())?;
        } else {
            write!(out, "  {}. {label}\r\n", i + 1)?;
        }
    }
    out.flush()
}

/// Shows an inline arrow-key picker over `labels` and blocks until the user
/// confirms or cancels.
///
/// Returns `Ok(Some(index))` for the chosen label, or `Ok(None)` when the
/// user cancels (Esc / `q` / Ctrl-C). On confirm the picker lines are cleared
/// and replaced with a one-line summary so scroll-back stays clean. The
/// caller must ensure stdin and stdout are terminals before calling.
pub(crate) fn pick(prompt: &str, labels: &[String]) -> io::Result<Option<usize>> {
    let mut out = io::stdout();
    let guard = RawGuard::new()?;
    let mut selected = 0usize;
    draw(&mut out, prompt, labels, selected, false)?;

    let outcome = loop {
        if let Event::Key(KeyEvent {
            code,
            modifiers,
            kind,
            ..
        }) = read()?
        {
            if kind != KeyEventKind::Press {
                continue;
            }
            let (next, outcome) = step(selected, labels.len(), code, modifiers);
            if next != selected {
                selected = next;
                draw(&mut out, prompt, labels, selected, true)?;
            }
            if let Some(outcome) = outcome {
                break outcome;
            }
        }
    };

    execute!(
        out,
        MoveUp(labels.len() as u16 + 1),
        MoveToColumn(0),
        Clear(ClearType::FromCursorDown)
    )?;
    drop(guard);

    match outcome {
        Outcome::Picked(idx) => {
            println!("→ {}", labels[idx]);
            Ok(Some(idx))
        }
        Outcome::Cancelled => Ok(None),
    }
}

/// Applies one key press to the picker state.
///
/// Returns the new selected index and, when the key was terminal (confirm or
/// cancel), the [`Outcome`]. Movement clamps at the list edges; digits `1`-`9`
/// jump-select directly when in range and are ignored otherwise. `len` must be
/// at least 1.
fn step(
    selected: usize,
    len: usize,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> (usize, Option<Outcome>) {
    match code {
        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
            (selected, Some(Outcome::Cancelled))
        }
        KeyCode::Esc | KeyCode::Char('q') => (selected, Some(Outcome::Cancelled)),
        KeyCode::Up | KeyCode::Char('k') => (selected.saturating_sub(1), None),
        KeyCode::Down | KeyCode::Char('j') => ((selected + 1).min(len - 1), None),
        KeyCode::Enter => (selected, Some(Outcome::Picked(selected))),
        KeyCode::Char(c @ '1'..='9') => {
            let idx = (c as usize) - ('1' as usize);
            if idx < len {
                (idx, Some(Outcome::Picked(idx)))
            } else {
                (selected, None)
            }
        }
        _ => (selected, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: KeyModifiers = KeyModifiers::NONE;

    #[test]
    fn picker_up_clamps_at_top() {
        assert_eq!(step(0, 3, KeyCode::Up, NONE), (0, None));
        assert_eq!(step(2, 3, KeyCode::Up, NONE), (1, None));
    }

    #[test]
    fn picker_down_clamps_at_bottom() {
        assert_eq!(step(2, 3, KeyCode::Down, NONE), (2, None));
        assert_eq!(step(0, 3, KeyCode::Down, NONE), (1, None));
    }

    #[test]
    fn picker_vim_keys_move() {
        assert_eq!(step(1, 3, KeyCode::Char('k'), NONE), (0, None));
        assert_eq!(step(1, 3, KeyCode::Char('j'), NONE), (2, None));
    }

    #[test]
    fn picker_enter_picks_selected() {
        assert_eq!(
            step(1, 3, KeyCode::Enter, NONE),
            (1, Some(Outcome::Picked(1)))
        );
    }

    #[test]
    fn picker_digit_jump_selects_in_range() {
        assert_eq!(
            step(0, 3, KeyCode::Char('2'), NONE),
            (1, Some(Outcome::Picked(1)))
        );
    }

    #[test]
    fn picker_digit_out_of_range_is_ignored() {
        assert_eq!(step(0, 3, KeyCode::Char('9'), NONE), (0, None));
    }

    #[test]
    fn picker_esc_q_and_ctrl_c_cancel() {
        assert_eq!(
            step(1, 3, KeyCode::Esc, NONE),
            (1, Some(Outcome::Cancelled))
        );
        assert_eq!(
            step(1, 3, KeyCode::Char('q'), NONE),
            (1, Some(Outcome::Cancelled))
        );
        assert_eq!(
            step(1, 3, KeyCode::Char('c'), KeyModifiers::CONTROL),
            (1, Some(Outcome::Cancelled))
        );
    }

    #[test]
    fn picker_unknown_keys_do_nothing() {
        assert_eq!(step(1, 3, KeyCode::Char('x'), NONE), (1, None));
    }
}
