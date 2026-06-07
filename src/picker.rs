//! Inline interactive picker for disambiguating contract references.
//!
//! Renders a small arrow-key list in place (no alternate screen, scroll-back
//! preserved). Key handling is a pure function so it is testable without a
//! terminal; raw mode is held behind an RAII guard so it is restored on every
//! exit path, including panics.

use crossterm::event::{KeyCode, KeyModifiers};

/// Terminal decision of one picker interaction.
#[derive(Debug, PartialEq)]
enum Outcome {
    /// The user confirmed the candidate at this index.
    Picked(usize),
    /// The user cancelled (Esc, `q`, or Ctrl-C).
    Cancelled,
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
        assert_eq!(step(1, 3, KeyCode::Esc, NONE), (1, Some(Outcome::Cancelled)));
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
