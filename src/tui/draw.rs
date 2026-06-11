//! Rendering for the authoring TUI, replicating `apic read`'s exact layout
//! (see `crate::render::Printer`) with the selection / cell-edit overlaid.
//! Borderless throughout.

use crate::tui::rows::{Cell, CellKind, RowKind, Section, SectionKind, TableRow};
use crate::tui::state::{Mode, UiState};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use tui_textarea::TextArea;

const GAP: usize = 2; // spaces between table columns

/// Draws the whole UI for the current frame.
pub(crate) fn draw(frame: &mut Frame, state: &UiState) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let (lines, sel) = build_lines(state);
    // Simple top-anchored scroll: keep the selected line in view.
    let height = chunks[0].height as usize;
    let scroll = sel.saturating_sub(height.saturating_sub(2)) as u16;
    frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), chunks[0]);

    draw_status(frame, chunks[1], state);

    if state.mode == Mode::Help {
        draw_help(frame, area);
    }
}

/// A style helper: the resting cyan section title.
fn title_style() -> Style {
    Style::default()
        .add_modifier(Modifier::BOLD)
        .fg(Color::Cyan)
}

fn dim() -> Style {
    Style::default().fg(Color::DarkGray)
}

/// The color of an HTTP method label, per `render::Printer::method`.
fn method_color(method: &str) -> Color {
    match method {
        "GET" => Color::Green,
        "POST" => Color::Blue,
        "PUT" => Color::Yellow,
        "PATCH" => Color::Magenta,
        "DELETE" => Color::Red,
        _ => Color::White,
    }
}

/// Column widths for a table section: max display width per column across its
/// `Field` rows whose cell count equals the header count.
fn col_widths(section: &Section, ncols: usize) -> Vec<usize> {
    let mut w = vec![0usize; ncols];
    if let Some(h) = &section.headers {
        for (i, head) in h.iter().enumerate() {
            w[i] = head.chars().count();
        }
    }
    for row in &section.rows {
        if row.kind == RowKind::Field && row.cells.len() == ncols {
            for (i, c) in row.cells.iter().enumerate() {
                w[i] = w[i].max(c.value.chars().count());
            }
        }
    }
    w
}

/// Builds all display lines, returning them and the index of the line that
/// carries the current selection (for scrolling).
fn build_lines(state: &UiState) -> (Vec<Line<'static>>, usize) {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut sel_line = 0usize;

    for (si, section) in state.sections.iter().enumerate() {
        match section.kind {
            SectionKind::Header => {
                push_header(state, si, section, &mut lines, &mut sel_line);
            }
            SectionKind::Table | SectionKind::Body => {
                push_section(state, si, section, &mut lines, &mut sel_line);
            }
        }
    }
    (lines, sel_line)
}

/// Emits the header block: ` NAME`, ` description` (when non-empty), a blank
/// line, then the URL (collapsed ` METHOD url` or expanded key/value rows).
fn push_header(
    state: &UiState,
    si: usize,
    section: &Section,
    lines: &mut Vec<Line<'static>>,
    sel_line: &mut usize,
) {
    for (ri, row) in section.rows.iter().enumerate() {
        let selected = si == state.sec && ri == state.row;
        match row.kind {
            RowKind::Name => {
                let line =
                    header_value_line(state, row, selected, |v| format!(" {}", v.to_uppercase()));
                if selected {
                    *sel_line = lines.len();
                }
                lines.push(line);
            }
            RowKind::Desc => {
                let value = &row.cells[0].value;
                let editing = selected && state.cell.is_some();
                // Read prints the description line only when non-empty; while
                // editing we still show it so the cursor has somewhere to live.
                if value.is_empty() && !editing {
                    // Still track selection so scroll/navigation stays sane.
                    if selected {
                        *sel_line = lines.len();
                    }
                    continue;
                }
                let line = header_value_line(state, row, selected, |v| format!(" {v}"));
                if selected {
                    *sel_line = lines.len();
                }
                lines.push(line);
            }
            RowKind::UrlLine => {
                lines.push(Line::raw("")); // blank line before the URL
                if selected {
                    *sel_line = lines.len();
                }
                lines.push(url_line(state, row, selected));
            }
            RowKind::Field => {
                // Expanded URL: blank line precedes the method row.
                if ri > 0 && section.rows[ri - 1].kind == RowKind::Desc {
                    lines.push(Line::raw(""));
                }
                if selected {
                    *sel_line = lines.len();
                }
                lines.push(kv_line(state, row, selected));
            }
            RowKind::Title | RowKind::Example => {}
        }
    }
}

/// Renders a header text line (` NAME`, ` description`) honoring an in-progress
/// insert buffer (which edits the TRUE value, not the uppercased display).
fn header_value_line(
    state: &UiState,
    row: &TableRow,
    selected: bool,
    fmt: impl Fn(&str) -> String,
) -> Line<'static> {
    let cell = &row.cells[0];
    let focused = selected && state.cell == Some(0);
    let base = sel_style(selected);
    if focused && let Mode::Insert(buf) = &state.mode {
        // Insert edits the raw value; show it verbatim with a cursor.
        return Line::from(Span::styled(
            format!(" {buf}_"),
            base.fg(Color::Black).bg(Color::Yellow),
        ));
    }
    let style = if focused {
        base.fg(Color::Black).bg(Color::Yellow)
    } else {
        base
    };
    Line::from(Span::styled(fmt(&cell.value), style))
}

/// The collapsed ` METHOD <built-url>` line.
fn url_line(state: &UiState, row: &TableRow, selected: bool) -> Line<'static> {
    let base = sel_style(selected);
    let method = &row.cells[0];
    let url = &row.cells[1];
    let method_focused = selected && state.cell == Some(0);

    let method_style = if method_focused {
        base.fg(Color::Black).bg(Color::Yellow)
    } else {
        base.fg(method_color(&method.value))
            .add_modifier(Modifier::BOLD)
    };
    Line::from(vec![
        Span::styled(" ", base),
        Span::styled(method.value.clone(), method_style),
        Span::styled(" ", base),
        Span::styled(url.value.clone(), base),
    ])
}

/// A ` label  value` key/value line (expanded URL rows). The label is dim; the
/// value cell honors focus and the insert buffer.
fn kv_line(state: &UiState, row: &TableRow, selected: bool) -> Line<'static> {
    let base = sel_style(selected);
    let mut spans = vec![Span::styled(" ", base)];
    for (i, c) in row.cells.iter().enumerate() {
        let focused = selected && state.cell == Some(i);
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
            spans.push(Span::styled("  ", base));
        }
    }
    Line::from(spans)
}

/// Emits a read-style section: a blank line, the bold title, then either the
/// rows (aligned table / body) or a dim ` (none)` for an empty section.
fn push_section(
    state: &UiState,
    si: usize,
    section: &Section,
    lines: &mut Vec<Line<'static>>,
    sel_line: &mut usize,
) {
    lines.push(Line::raw("")); // blank line before the title

    // For Body sections the title is carried by a `RowKind::Title` row (so the
    // expand selection lands on it); Table sections print `section.title`.
    let title_row = section
        .rows
        .iter()
        .enumerate()
        .find(|(_, r)| r.kind == RowKind::Title);
    match title_row {
        Some((ri, row)) => {
            let selected = si == state.sec && ri == state.row;
            if selected {
                *sel_line = lines.len();
            }
            let style = if selected {
                title_style().bg(Color::Rgb(40, 40, 60))
            } else {
                title_style()
            };
            lines.push(Line::from(Span::styled(
                format!(" {}", row.cells[0].value),
                style,
            )));
        }
        None => {
            lines.push(Line::from(Span::styled(
                format!(" {}", section.title),
                title_style(),
            )));
        }
    }

    let field_rows: Vec<&TableRow> = section
        .rows
        .iter()
        .filter(|r| r.kind == RowKind::Field)
        .collect();
    let has_example = section.rows.iter().any(|r| r.kind == RowKind::Example);

    if field_rows.is_empty() && !has_example {
        lines.push(Line::from(Span::styled(" (none)", dim())));
        return;
    }

    let ncols = section.headers.as_ref().map(|h| h.len()).unwrap_or(0);
    let widths = if ncols > 0 {
        col_widths(section, ncols)
    } else {
        Vec::new()
    };

    // Only schema field rows (cell count == ncols) feed the column-header line;
    // expanded kv rows (label + value) are rendered ` label  value`.
    let has_schema_rows = field_rows
        .iter()
        .any(|r| ncols > 0 && r.cells.len() == ncols);

    // Column header line (dim), when the section carries headers and schema rows.
    if let (Some(h), true) = (&section.headers, has_schema_rows) {
        let mut spans = vec![Span::raw(" ")];
        for (i, head) in h.iter().enumerate() {
            spans.push(Span::styled(pad(head, widths[i]), dim()));
            if i + 1 < h.len() {
                spans.push(Span::raw(" ".repeat(GAP)));
            }
        }
        lines.push(Line::from(trim_trailing(spans)));
    }

    for (ri, row) in section.rows.iter().enumerate() {
        let selected = si == state.sec && ri == state.row;
        match row.kind {
            RowKind::Field => {
                if selected {
                    *sel_line = lines.len();
                }
                // Expanded title rows (label + value) render like URL kv rows.
                if ncols > 0 && row.cells.len() == ncols {
                    lines.push(table_line(state, row, &widths, ncols, selected));
                } else {
                    lines.push(kv_line(state, row, selected));
                }
            }
            RowKind::Example => {
                lines.push(Line::raw("")); // blank line before Example:
                lines.push(Line::from(Span::styled(" Example:", dim())));
                if selected {
                    *sel_line = lines.len();
                }
                push_example(row, selected, lines);
            }
            _ => {}
        }
    }
}

/// Emits the inline example payload (` <line>` per line of the raw buffer), or
/// ` (no example provided)` when empty.
fn push_example(row: &TableRow, selected: bool, lines: &mut Vec<Line<'static>>) {
    let base = sel_style(selected);
    if row.raw.trim().is_empty() {
        lines.push(Line::from(Span::styled(" (no example provided)", dim())));
        return;
    }
    for raw_line in row.raw.lines() {
        lines.push(Line::from(Span::styled(format!(" {raw_line}"), base)));
    }
}

/// An aligned table row: columns left-padded to `widths`, joined by two spaces,
/// trailing whitespace trimmed — identical to `render::Printer::table`.
fn table_line(
    state: &UiState,
    row: &TableRow,
    widths: &[usize],
    ncols: usize,
    selected: bool,
) -> Line<'static> {
    let base = sel_style(selected);
    let editing_here = selected && state.cell.is_some();
    let mut spans = vec![Span::styled(" ", base)];

    if row.cells.len() == ncols && ncols > 0 {
        let last = row.cells.len() - 1;
        for (i, c) in row.cells.iter().enumerate() {
            let focused = editing_here && state.cell == Some(i);
            let val = cell_text(state, c, focused);
            // Last column is not padded (its trailing space would be trimmed).
            let cell_str = if i == last { val } else { pad(&val, widths[i]) };
            let style = if focused {
                base.fg(Color::Black).bg(Color::Yellow)
            } else {
                base
            };
            spans.push(Span::styled(cell_str, style));
            if i + 1 < row.cells.len() {
                spans.push(Span::styled(" ".repeat(GAP), base));
            }
        }
    } else {
        // Header-less (HEADERS) or mismatched: space-joined cells.
        let last = row.cells.len().saturating_sub(1);
        let hdr_widths = !widths.is_empty();
        for (i, c) in row.cells.iter().enumerate() {
            let focused = editing_here && state.cell == Some(i);
            let val = cell_text(state, c, focused);
            let cell_str = if hdr_widths && i < widths.len() && i != last {
                pad(&val, widths[i])
            } else {
                val
            };
            let style = if focused {
                base.fg(Color::Black).bg(Color::Yellow)
            } else if c.kind == CellKind::Label {
                base.fg(Color::DarkGray)
            } else {
                base
            };
            spans.push(Span::styled(cell_str, style));
            if i + 1 < row.cells.len() {
                spans.push(Span::styled(" ".repeat(GAP), base));
            }
        }
    }
    Line::from(trim_trailing(spans))
}

/// The base style for a row: a subtle background highlight when selected.
fn sel_style(selected: bool) -> Style {
    if selected {
        Style::default().bg(Color::Rgb(40, 40, 60))
    } else {
        Style::default()
    }
}

/// The text to show for a cell, accounting for an in-progress insert buffer.
fn cell_text(state: &UiState, cell: &Cell, focused: bool) -> String {
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

/// Drops a trailing whitespace-only span so lines `trim_end` like read's output.
fn trim_trailing(mut spans: Vec<Span<'static>>) -> Vec<Span<'static>> {
    while let Some(last) = spans.last() {
        if last.content.trim().is_empty() && spans.len() > 1 {
            spans.pop();
        } else {
            break;
        }
    }
    spans
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
Row select: ↑/↓ or j/k move between rows · Enter steps into a row's cells
Cell edit:  ←/→ move between cells · Enter edits the cell (type / cycle / toggle) · Esc back
URL:        Enter on the METHOD url line expands it; Esc collapses it again
Add/Delete: a adds a row to the current section · d deletes the selected row
Examples:   Enter on an example opens the JSON editor in a pop-up
Save/quit:  Ctrl-S save · q quit · ? toggle this help";
    let popup = centered(area, 76, 45);
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
    fn renders_like_apic_read() {
        let c = json_get(
            r#"{ "name":"user","description":"User management","method":"GET",
                 "url":{"protocol":"https","host":"api.example.com","path":["user"],
                        "variable":[{"name":"id","type":"int","description":"User ID","required":false}]},
                 "headers":[{"name":"Content-Type","value":"application/json"}],
                 "responses":[{"code":200,"description":"ok","schema":[
                    {"name":"status","type":"int","default":null,"description":"Status","required":true}
                 ],"example":{"status":200}}] }"#,
            None,
        )
        .unwrap();
        let m = EditModel::from_contract(c);
        let state = UiState::new(&m);
        let backend = TestBackend::new(100, 50);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let text: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(text.contains("USER")); // uppercased name
        assert!(text.contains("User management")); // description
        assert!(text.contains("https://api.example.com/user")); // built URL
        assert!(text.contains("VARIABLE"));
        assert!(text.contains("HEADERS"));
        assert!(text.contains("RESPONSE 200 — ok"));
        assert!(text.contains("Example:"));
        // The fixture has no nested fields, so no ├─/└─; assert no box borders.
        for g in ['│', '┌', '┐', '└', '┘', '├', '┤', '┬', '┴', '┼'] {
            assert!(!text.contains(g), "found border glyph {g:?}");
        }
    }
}
