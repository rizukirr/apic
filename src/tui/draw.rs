//! Rendering for the table-based authoring TUI (borderless).

use crate::tui::rows::{CellKind, RowKind, Section};
use crate::tui::state::{Mode, UiState};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use tui_textarea::TextArea;

const GAP: usize = 2; // spaces between columns

/// Draws the whole UI for the current frame.
pub(crate) fn draw(frame: &mut Frame, state: &UiState) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let lines = build_lines(state);
    // Simple top-anchored scroll: keep the selected line in view.
    let height = chunks[0].height as usize;
    let sel = selected_line_index(state);
    let scroll = sel.saturating_sub(height.saturating_sub(2)) as u16;
    frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), chunks[0]);

    draw_status(frame, chunks[1], state);

    if state.mode == Mode::Help {
        draw_help(frame, area);
    }
}

/// Column widths for a table section: max display width per column across its
/// `Data` rows whose cell count equals the header count.
fn col_widths(section: &Section, ncols: usize) -> Vec<usize> {
    let mut w = vec![0usize; ncols];
    if let Some(h) = &section.headers {
        for (i, head) in h.iter().enumerate() {
            w[i] = head.chars().count();
        }
    }
    for row in &section.rows {
        if row.kind == RowKind::Data && row.cells.len() == ncols {
            for (i, c) in row.cells.iter().enumerate() {
                w[i] = w[i].max(c.value.chars().count());
            }
        }
    }
    w
}

/// Builds all display lines, tracking which correspond to the selected row.
fn build_lines(state: &UiState) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (si, section) in state.sections.iter().enumerate() {
        lines.push(Line::from(Span::styled(
            section.title.clone(),
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Cyan),
        )));
        let ncols = section.headers.as_ref().map(|h| h.len()).unwrap_or(0);
        let widths = if ncols > 0 {
            col_widths(section, ncols)
        } else {
            Vec::new()
        };
        if let Some(h) = &section.headers {
            let mut spans = vec![Span::raw("  ")];
            for (i, head) in h.iter().enumerate() {
                spans.push(Span::styled(
                    pad(head, widths[i]),
                    Style::default().fg(Color::DarkGray),
                ));
                spans.push(Span::raw(" ".repeat(GAP)));
            }
            lines.push(Line::from(spans));
        }
        for (ri, row) in section.rows.iter().enumerate() {
            let selected = si == state.sec && ri == state.row;
            lines.push(row_line(state, section, row, &widths, ncols, selected));
        }
    }
    lines
}

/// Renders one row to a Line.
fn row_line(
    state: &UiState,
    section: &Section,
    row: &crate::tui::rows::TableRow,
    widths: &[usize],
    ncols: usize,
    selected: bool,
) -> Line<'static> {
    let indent = "  ".repeat(row.indent as usize + 1);
    let mut spans = vec![Span::raw(indent)];

    let base = if selected {
        Style::default().bg(Color::Rgb(40, 40, 60))
    } else {
        Style::default()
    };

    let editing_here = selected && state.cell.is_some();

    if row.kind == RowKind::Data && section.headers.is_some() && row.cells.len() == ncols {
        // aligned columns
        for (i, c) in row.cells.iter().enumerate() {
            let focused = editing_here && state.cell == Some(i);
            let val = cell_text(state, c, focused);
            let style = if focused {
                base.fg(Color::Black).bg(Color::Yellow)
            } else {
                base
            };
            spans.push(Span::styled(pad(&val, widths[i]), style));
            spans.push(Span::raw(" ".repeat(GAP)));
        }
    } else {
        // key/value or add/example: render cells separated by spaces
        for (i, c) in row.cells.iter().enumerate() {
            let focused = editing_here && state.cell == Some(i);
            let val = cell_text(state, c, focused);
            let style = if focused {
                base.fg(Color::Black).bg(Color::Yellow)
            } else if c.kind == CellKind::Label {
                base.fg(Color::DarkGray)
            } else {
                base
            };
            spans.push(Span::styled(val, style));
            if i + 1 < row.cells.len() {
                spans.push(Span::raw("  "));
            }
        }
    }
    Line::from(spans)
}

/// The text to show for a cell, accounting for an in-progress insert buffer.
fn cell_text(state: &UiState, cell: &crate::tui::rows::Cell, focused: bool) -> String {
    if focused && let Mode::Insert(buf) = &state.mode {
        return format!("{buf}_");
    }
    cell.value.clone()
}

fn pad(s: &str, w: usize) -> String {
    let len = s.chars().count();
    if len >= w {
        s.to_string()
    } else {
        format!("{s}{}", " ".repeat(w - len))
    }
}

/// Index of the line corresponding to the selected row (for scrolling).
fn selected_line_index(state: &UiState) -> usize {
    let mut idx = 0usize;
    for (si, section) in state.sections.iter().enumerate() {
        idx += 1; // title
        if section.headers.is_some() {
            idx += 1; // header line
        }
        for ri in 0..section.rows.len() {
            if si == state.sec && ri == state.row {
                return idx;
            }
            idx += 1;
        }
    }
    idx
}

fn draw_status(frame: &mut Frame, area: Rect, state: &UiState) {
    let dirty = if state.dirty { "*" } else { " " };
    frame.render_widget(
        Paragraph::new(format!("{dirty} {}", state.status))
            .style(Style::default().fg(Color::Yellow)),
        area,
    );
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let help = "\
Row select: ↑/↓ or j/k move between rows · Enter edits the row · d deletes a list row
Cell edit:  ←/→ move between cells · Enter edits the cell (type / cycle / toggle) · Esc back
Add/Open:   Enter on a '+ add' row inserts · Enter on an 'example' row opens the JSON editor
Save/quit:  Ctrl-S save · q quit · ? toggle this help";
    let popup = centered(area, 72, 40);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(help)
            .block(Block::default().borders(Borders::NONE).title(" help "))
            .wrap(Wrap { trim: false }),
        popup,
    );
}

/// Renders the borderless example-editor modal.
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
    fn renders_tables_without_borders() {
        let c = json_get(
            r#"{ "name":"login","method":"GET",
                 "url":{"protocol":"https","host":"h","path":["x"],
                        "query":[{"name":"page","value":"1","description":"d","required":false}]},
                 "headers":[{"name":"A","value":"B"}],
                 "responses":[{"code":200,"description":"ok","schema":[]}] }"#,
            None,
        )
        .unwrap();
        let m = EditModel::from_contract(c);
        let state = UiState::new(&m);

        let backend = TestBackend::new(100, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();

        let buffer = terminal.backend().buffer().clone();
        let text: String = buffer.content().iter().map(|c| c.symbol()).collect();
        // A column header and a value are present.
        assert!(text.contains("NAME"));
        assert!(text.contains("login"));
        // No box-drawing border glyphs anywhere.
        for g in ['│', '─', '┌', '┐', '└', '┘', '├', '┤', '┬', '┴', '┼'] {
            assert!(!text.contains(g), "found border glyph {g:?}");
        }
    }
}
