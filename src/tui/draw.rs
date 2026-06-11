//! Rendering for the authoring TUI.

use crate::tui::rows::RowKind;
use crate::tui::state::{Mode, UiState};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use tui_textarea::TextArea;

/// Draws the whole UI for the current frame.
pub(crate) fn draw(frame: &mut Frame, state: &UiState) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    draw_outline(frame, chunks[0], state);
    draw_status(frame, chunks[1], state);

    match &state.mode {
        Mode::Help => draw_help(frame, area),
        Mode::ConfirmQuit => {} // status line carries the prompt
        _ => {}
    }
}

fn draw_outline(frame: &mut Frame, area: Rect, state: &UiState) {
    let items: Vec<ListItem> = state
        .rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let indent = "  ".repeat(row.indent as usize);
            let mut spans = vec![Span::raw(indent)];
            if row.kind == RowKind::Header {
                spans.push(Span::styled(
                    row.label.clone(),
                    Style::default()
                        .add_modifier(Modifier::BOLD)
                        .fg(Color::Cyan),
                ));
            } else {
                let editing = matches!(&state.mode, Mode::Insert(_)) && i == state.cursor;
                let value = if editing {
                    if let Mode::Insert(buf) = &state.mode {
                        format!("{buf}_")
                    } else {
                        row.value.clone()
                    }
                } else {
                    row.value.clone()
                };
                spans.push(Span::styled(
                    format!("{}: ", row.label),
                    Style::default().fg(Color::DarkGray),
                ));
                spans.push(Span::raw(value));
            }
            let mut style = Style::default();
            if i == state.cursor {
                style = style.bg(Color::Rgb(40, 40, 60));
            }
            ListItem::new(Line::from(spans)).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" apic — edit contract "),
    );
    frame.render_widget(list, area);
}

fn draw_status(frame: &mut Frame, area: Rect, state: &UiState) {
    let dirty = if state.dirty { "*" } else { " " };
    let text = format!("{dirty} {}", state.status);
    frame.render_widget(
        Paragraph::new(text).style(Style::default().fg(Color::Yellow)),
        area,
    );
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let help = "\
Navigation: ↑/↓ or j/k move · Enter edit/add · ←/→ or space cycle/toggle
Editing:    type to change · Enter commit · Esc cancel
Lists:      Enter on '+ add' inserts · d deletes the focused row
Examples:   Enter opens the JSON editor · Esc returns
Save/quit:  Ctrl-S save · q quit · ? toggle this help";
    let block = Block::default().borders(Borders::ALL).title(" help ");
    let popup = centered(area, 70, 40);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(help).block(block).wrap(Wrap { trim: false }),
        popup,
    );
}

/// Renders the example-editor modal over the screen.
pub(crate) fn draw_example_modal(frame: &mut Frame, textarea: &TextArea) {
    let area = centered(frame.area(), 80, 70);
    frame.render_widget(Clear, area);
    frame.render_widget(textarea, area);
}

/// A centered rect `pct_x`% × `pct_y`% of `area`.
fn centered(area: Rect, pct_x: u16, pct_y: u16) -> Rect {
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - pct_y) / 2),
            Constraint::Percentage(pct_y),
            Constraint::Percentage((100 - pct_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - pct_x) / 2),
            Constraint::Percentage(pct_x),
            Constraint::Percentage((100 - pct_x) / 2),
        ])
        .split(v[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::json_get;
    use crate::tui::model::EditModel;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn renders_without_panic() {
        let c = json_get(
            r#"{ "name":"login","method":"GET",
                 "url":{"protocol":"https","host":"h","path":["x"]},
                 "headers":[],"responses":[{"code":200,"description":"ok","schema":[]}] }"#,
            None,
        )
        .unwrap();
        let m = EditModel::from_contract(c);
        let state = UiState::new(&m);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();

        let buffer = terminal.backend().buffer().clone();
        let text: String = buffer.content().iter().map(|c| c.symbol()).collect();
        assert!(text.contains("META"));
        assert!(text.contains("login"));
    }
}
