//! Rendering for the authoring TUI, replicating `apic read`'s exact layout
//! (see `crate::render::Printer`) with the selection / cell-edit overlaid.
//! Borderless throughout.

use crate::tui::rows::{Cell, CellKind, RowKind, Section, SectionKind, TableRow};
use crate::tui::state::{Mode, UiState};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph, Row, Table};
use ratatui_textarea::TextArea;

const GAP: usize = 2; // spaces between table columns

/// Draws the whole UI for the current frame.
pub(crate) fn draw(frame: &mut Frame, state: &UiState) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .margin(2)
        .split(area);

    let (mut lines, sel_start, sel_end, cursor) = build_lines(state);
    // Center the selected row in the viewport (neovim-style). Half a screen of
    // blank padding is appended so even the very bottom row can sit centered.
    // The scroll is clamped at the top so short contracts don't shift.
    let view = chunks[0].height as usize;
    let pad = view / 2;
    lines.extend(std::iter::repeat_with(|| Line::raw("")).take(pad));
    let max_scroll = lines.len().saturating_sub(view);
    let center = (sel_start + sel_end) / 2;
    let scroll = center.saturating_sub(view / 2).min(max_scroll) as u16;
    frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), chunks[0]);

    // Place a real terminal cursor at the end of the insert buffer, if any.
    if let Some((line, col)) = cursor
        && line >= scroll as usize
    {
        let y = chunks[0].y + (line - scroll as usize) as u16;
        let x = chunks[0].x + col as u16;
        if y < chunks[0].y + chunks[0].height && x < chunks[0].x + chunks[0].width {
            frame.set_cursor_position((x, y));
        }
    }

    draw_status(frame, chunks[1], state);

    match state.mode {
        Mode::Help => draw_help(frame, area),
        Mode::ConfirmQuit => draw_confirm(
            frame,
            area,
            " unsaved changes ",
            "Save before quitting?",
            "y: save & quit    n: discard    Esc: cancel",
        ),
        Mode::ConfirmDelete(_) => draw_confirm(
            frame,
            area,
            " confirm delete ",
            "Delete this row?",
            "y: delete    n/Esc: cancel",
        ),
        _ => {}
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
                let extra = if i == 0 {
                    row.prefix.chars().count()
                } else {
                    0
                };
                w[i] = w[i].max(extra + c.value.chars().count());
            }
        }
    }
    w
}

/// Builds all display lines, returning them, the FIRST and LAST line index of
/// the selected row's rendered content (for scrolling), and the insert cursor
/// position `(line, col)` where `col` is a char offset from the line start (if
/// editing).
fn build_lines(state: &UiState) -> (Vec<Line<'static>>, usize, usize, Option<(usize, usize)>) {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut sel: (usize, usize) = (0, 0);
    let mut cursor: Option<(usize, usize)> = None;

    for (si, section) in state.sections.iter().enumerate() {
        match section.kind {
            SectionKind::Header => {
                push_header(state, si, section, &mut lines, &mut sel, &mut cursor);
            }
            SectionKind::Table | SectionKind::Body => {
                push_section(state, si, section, &mut lines, &mut sel, &mut cursor);
            }
        }
    }
    (lines, sel.0, sel.1, cursor)
}

/// Emits the header block: ` NAME`, ` description` (when non-empty), a blank
/// line, then the URL (collapsed ` METHOD url` or expanded key/value rows).
fn push_header(
    state: &UiState,
    si: usize,
    section: &Section,
    lines: &mut Vec<Line<'static>>,
    sel: &mut (usize, usize),
    cursor: &mut Option<(usize, usize)>,
) {
    for (ri, row) in section.rows.iter().enumerate() {
        let selected = si == state.sec && ri == state.row;
        match row.kind {
            RowKind::Name => {
                let (line, col) =
                    header_value_line(state, row, selected, |v| format!(" {}", v.to_uppercase()));
                if selected {
                    *sel = (lines.len(), lines.len());
                }
                record_cursor(cursor, lines.len(), col);
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
                        *sel = (lines.len(), lines.len());
                    }
                    continue;
                }
                let (line, col) = header_value_line(state, row, selected, |v| format!(" {v}"));
                if selected {
                    *sel = (lines.len(), lines.len());
                }
                record_cursor(cursor, lines.len(), col);
                lines.push(line);
            }
            RowKind::UrlLine => {
                lines.push(Line::raw("")); // blank line before the URL
                if selected {
                    *sel = (lines.len(), lines.len());
                }
                lines.push(url_line(state, row, selected));
            }
            RowKind::Field => {
                // Expanded URL: blank line precedes the method row.
                if ri > 0 && section.rows[ri - 1].kind == RowKind::Desc {
                    lines.push(Line::raw(""));
                }
                if selected {
                    *sel = (lines.len(), lines.len());
                }
                let (line, col) = kv_line(state, row, selected);
                record_cursor(cursor, lines.len(), col);
                lines.push(line);
            }
            RowKind::Title | RowKind::Example => {}
        }
    }
}

/// Records the insert cursor at `(line, col)` when a line builder reported a
/// cursor column.
fn record_cursor(cursor: &mut Option<(usize, usize)>, line: usize, col: Option<usize>) {
    if let Some(c) = col {
        *cursor = Some((line, c));
    }
}

/// Renders a header text line (` NAME`, ` description`) honoring an in-progress
/// insert buffer (which edits the TRUE value, not the uppercased display).
fn header_value_line(
    state: &UiState,
    row: &TableRow,
    selected: bool,
    fmt: impl Fn(&str) -> String,
) -> (Line<'static>, Option<usize>) {
    let cell = &row.cells[0];
    let focused = selected && state.cell == Some(0);
    let base = sel_style(state, selected);
    if focused && let Mode::Insert(buf) = &state.mode {
        // Insert edits the raw value; show it as plain bold text and place a
        // real terminal cursor at its end (no yellow highlight while typing).
        let line = Line::from(Span::styled(
            format!(" {buf}"),
            base.add_modifier(Modifier::BOLD),
        ));
        // One leading space precedes the buffer.
        let col = 1 + buf.chars().count();
        return (line, Some(col));
    }
    let style = if focused { cell_hl() } else { base };
    (Line::from(Span::styled(fmt(&cell.value), style)), None)
}

/// The collapsed ` METHOD <built-url>` line.
fn url_line(state: &UiState, row: &TableRow, selected: bool) -> Line<'static> {
    let base = sel_style(state, selected);
    let method = &row.cells[0];
    let url = &row.cells[1];
    let method_focused = selected && state.cell == Some(0);

    let method_style = if method_focused {
        cell_hl()
    } else if selected {
        // On the selected (cursor) line, the method follows the row highlight
        // (white bold) instead of its GET/POST color.
        base
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
fn kv_line(state: &UiState, row: &TableRow, selected: bool) -> (Line<'static>, Option<usize>) {
    let base = sel_style(state, selected);
    let mut spans = vec![Span::styled(" ", base)];
    let mut emitted = 1usize; // leading space
    let mut cursor_col = None;
    for (i, c) in row.cells.iter().enumerate() {
        let focused = selected && state.cell == Some(i);
        if focused && let Mode::Insert(buf) = &state.mode {
            // Plain text + real cursor while typing (no yellow highlight).
            spans.push(Span::styled(buf.clone(), base.add_modifier(Modifier::BOLD)));
            cursor_col = Some(emitted + buf.chars().count());
            emitted += buf.chars().count();
        } else {
            let val = cell_text(state, c, focused);
            let style = if focused {
                cell_hl()
            } else if c.kind == CellKind::Label && !selected {
                // Labels are dim only when the row is not selected; when the
                // cursor is on the row, keep the consistent white-bold highlight.
                base.fg(Color::DarkGray)
            } else {
                base
            };
            emitted += val.chars().count();
            spans.push(Span::styled(val, style));
        }
        if i + 1 < row.cells.len() {
            spans.push(Span::styled("  ", base));
            emitted += 2;
        }
    }
    (Line::from(spans), cursor_col)
}

/// Emits a read-style section: a blank line, the bold title, then either the
/// rows (aligned table / body) or a dim ` (none)` for an empty section.
fn push_section(
    state: &UiState,
    si: usize,
    section: &Section,
    lines: &mut Vec<Line<'static>>,
    sel: &mut (usize, usize),
    cursor: &mut Option<(usize, usize)>,
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
                *sel = (lines.len(), lines.len());
            }
            let style = if selected && state.cell.is_none() {
                title_style().bg(Color::Red).fg(Color::White)
            } else {
                title_style()
            };
            lines.push(Line::from(Span::styled(
                format!(" {}", row.cells[0].value),
                style,
            )));
        }
        None => {
            // An empty-titled, title-rowless section (e.g. the "+ add response"
            // affordance) prints no title line.
            if !section.title.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!(" {}", section.title),
                    title_style(),
                )));
            }
        }
    }

    let field_rows: Vec<&TableRow> = section
        .rows
        .iter()
        .filter(|r| r.kind == RowKind::Field)
        .collect();
    let has_example = section.rows.iter().any(|r| r.kind == RowKind::Example);

    let ncols = section.headers.as_ref().map(|h| h.len()).unwrap_or(0);
    let widths = if ncols > 0 {
        col_widths(section, ncols)
    } else {
        Vec::new()
    };

    // BODY sections (REQUEST/RESPONSE) match `apic read`: show `(none)` when
    // there are no schema fields and no example; show only the example when it
    // is example-only; otherwise show the schema table plus its example.
    if section.kind == SectionKind::Body {
        // The real schema fields are the `Field` rows whose cell count equals
        // the column count (NAME/TYPE/REQ/DESCRIPTION); the 2-cell `label value`
        // rows are the expanded `type`/`code`/`description` lead rows.
        let schema_rows_exist = field_rows
            .iter()
            .any(|r| ncols > 0 && r.cells.len() == ncols);
        let example_nonempty = section
            .rows
            .iter()
            .any(|r| r.kind == RowKind::Example && !r.raw.trim().is_empty());

        // Always render the lead kv rows (expanded type/code/description).
        for (ri, row) in section.rows.iter().enumerate() {
            if row.kind != RowKind::Field || (ncols > 0 && row.cells.len() == ncols) {
                continue;
            }
            let selected = si == state.sec && ri == state.row;
            if selected {
                *sel = (lines.len(), lines.len());
            }
            let (line, col) = kv_line(state, row, selected);
            record_cursor(cursor, lines.len(), col);
            lines.push(line);
        }

        if schema_rows_exist {
            // Column header line + schema rows, then the example row (always, so
            // it stays editable / openable even when empty).
            if let Some(h) = &section.headers {
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
                    RowKind::Field if ncols > 0 && row.cells.len() == ncols => {
                        if selected {
                            *sel = (lines.len(), lines.len());
                        }
                        let (line, col) = table_line(state, row, &widths, ncols, selected);
                        record_cursor(cursor, lines.len(), col);
                        lines.push(line);
                    }
                    RowKind::Example => {
                        push_example_block(state, row, selected, lines, sel);
                    }
                    _ => {}
                }
            }
        } else if example_nonempty {
            // Example-only body, like `apic read`.
            for (ri, row) in section.rows.iter().enumerate() {
                if row.kind == RowKind::Example {
                    let selected = si == state.sec && ri == state.row;
                    push_example_block(state, row, selected, lines, sel);
                }
            }
        } else {
            // No schema fields and an empty example: render `(none)` and nothing
            // else (no header line, no example row).
            lines.push(Line::from(Span::styled(" (none)", dim())));
        }
        return;
    }

    if field_rows.is_empty() && !has_example {
        lines.push(Line::from(Span::styled(" (none)", dim())));
        return;
    }

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
                    *sel = (lines.len(), lines.len());
                }
                // Expanded title rows (label + value) render like URL kv rows.
                let (line, col) = if ncols > 0 && row.cells.len() == ncols {
                    table_line(state, row, &widths, ncols, selected)
                } else {
                    kv_line(state, row, selected)
                };
                record_cursor(cursor, lines.len(), col);
                lines.push(line);
            }
            RowKind::Example => {
                push_example_block(state, row, selected, lines, sel);
            }
            _ => {}
        }
    }
}

/// Renders the ` Example:` label + the example payload (or ` (no example
/// provided)`), tracking the row's selection span over the whole block.
fn push_example_block(
    state: &UiState,
    row: &TableRow,
    selected: bool,
    lines: &mut Vec<Line<'static>>,
    sel: &mut (usize, usize),
) {
    lines.push(Line::raw("")); // blank line before Example:
    let example_label = lines.len();
    lines.push(Line::from(Span::styled(" Example:", dim())));
    push_example(state, row, selected, lines);
    if selected {
        // First line of the block is ` Example:`; last is the final
        // example-content line just pushed.
        *sel = (example_label, lines.len().saturating_sub(1));
    }
}

/// Emits the inline example payload (` <line>` per line of the raw buffer), or
/// ` (no example provided)` when empty.
fn push_example(state: &UiState, row: &TableRow, selected: bool, lines: &mut Vec<Line<'static>>) {
    let base = sel_style(state, selected);
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
) -> (Line<'static>, Option<usize>) {
    let base = sel_style(state, selected);
    let editing_here = selected && state.cell.is_some();
    let mut spans = vec![Span::styled(" ", base)];
    let mut emitted = 1usize; // leading space
    let mut cursor_col = None;

    if row.cells.len() == ncols && ncols > 0 {
        let last = row.cells.len() - 1;
        for (i, c) in row.cells.iter().enumerate() {
            let focused = editing_here && state.cell == Some(i);
            // Column 0 carries the tree prefix at display time only.
            let prefix = if i == 0 { row.prefix.as_str() } else { "" };
            if focused && let Mode::Insert(buf) = &state.mode {
                // Plain text + real cursor (no yellow highlight). The prefix is
                // shown but the cursor sits after the buffer.
                let shown = format!("{prefix}{buf}");
                let cell_str = if i == last {
                    shown
                } else {
                    pad(&shown, widths[i])
                };
                cursor_col = Some(emitted + prefix.chars().count() + buf.chars().count());
                emitted += cell_str.chars().count();
                spans.push(Span::styled(cell_str, base.add_modifier(Modifier::BOLD)));
            } else {
                let mut val = cell_text(state, c, focused);
                if i == 0 && !prefix.is_empty() {
                    val = format!("{prefix}{val}");
                }
                // A focused but empty last cell renders a single space so its
                // highlight is visible (e.g. editing an empty description).
                if i == last && focused && val.is_empty() {
                    val = " ".to_string();
                }
                // Last column is not padded (its trailing space would trim).
                let cell_str = if i == last { val } else { pad(&val, widths[i]) };
                let style = if focused { cell_hl() } else { base };
                emitted += cell_str.chars().count();
                spans.push(Span::styled(cell_str, style));
            }
            if i + 1 < row.cells.len() {
                spans.push(Span::styled(" ".repeat(GAP), base));
                emitted += GAP;
            }
        }
    } else {
        // Header-less (HEADERS) or mismatched: space-joined cells.
        let last = row.cells.len().saturating_sub(1);
        let hdr_widths = !widths.is_empty();
        for (i, c) in row.cells.iter().enumerate() {
            let focused = editing_here && state.cell == Some(i);
            if focused && let Mode::Insert(buf) = &state.mode {
                let cell_str = if hdr_widths && i < widths.len() && i != last {
                    pad(buf, widths[i])
                } else {
                    buf.clone()
                };
                cursor_col = Some(emitted + buf.chars().count());
                emitted += cell_str.chars().count();
                spans.push(Span::styled(cell_str, base.add_modifier(Modifier::BOLD)));
            } else {
                let val = cell_text(state, c, focused);
                let cell_str = if hdr_widths && i < widths.len() && i != last {
                    pad(&val, widths[i])
                } else {
                    val
                };
                let style = if focused {
                    cell_hl()
                } else if c.kind == CellKind::Label {
                    base.fg(Color::DarkGray)
                } else {
                    base
                };
                emitted += cell_str.chars().count();
                spans.push(Span::styled(cell_str, style));
            }
            if i + 1 < row.cells.len() {
                spans.push(Span::styled(" ".repeat(GAP), base));
                emitted += GAP;
            }
        }
    }
    (Line::from(trim_trailing(spans)), cursor_col)
}

/// The base style for a row: a yellow highlight when selected in row-select mode
/// (no cell focused). In cell-edit mode the row carries no base — only the
/// focused cell is highlighted in red (see `cell_hl`).
fn sel_style(state: &UiState, selected: bool) -> Style {
    if selected && state.cell.is_none() {
        Style::default()
            .bg(Color::Red)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else if selected && state.cell.is_some() {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    }
}

/// The highlight for the focused cell in cell-edit mode: red, regardless of the
/// row base.
fn cell_hl() -> Style {
    Style::default()
        .bg(Color::Yellow)
        .fg(Color::White)
        .add_modifier(Modifier::BOLD)
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
        // Keep a highlighted (background-styled) trailing span so a focused
        // empty cell's cursor stays visible; only trim default whitespace.
        if last.content.trim().is_empty() && last.style.bg.is_none() && spans.len() > 1 {
            spans.pop();
        } else {
            break;
        }
    }
    spans
}

fn draw_status(frame: &mut Frame, area: Rect, state: &UiState) {
    let marker = if state.dirty { "● unsaved  " } else { "" };
    let line = Line::from(vec![
        Span::styled(
            marker,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(state.status.clone(), Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

/// A bordered, centered confirmation popup showing a prompt and the key legend.
fn draw_confirm(frame: &mut Frame, area: Rect, title: &str, prompt: &str, keys: &str) {
    let popup = centered(area, 50, 20);

    frame.render_widget(Clear, popup);

    let content = vec![
        Line::from(""),
        Line::from(Span::styled(
            prompt,
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(keys),
    ];

    let dialog = Paragraph::new(content).alignment(Alignment::Center).block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .padding(Padding::new(2, 2, 1, 1)),
    );

    frame.render_widget(dialog, popup);
}

/// A bordered help popup with an aligned `KEY  ACTION` two-column table.
fn draw_help(frame: &mut Frame, area: Rect) {
    let rows = vec![
        Row::new(vec!["↑/↓  j/k", "Select a row"]),
        Row::new(vec!["←/→  h/l", "Move between cells"]),
        Row::new(vec!["Enter", "Edit cell · expand url/title · open example"]),
        Row::new(vec!["i", "Insert — edit the focused text cell"]),
        Row::new(vec!["Esc", "Back · collapse · cancel"]),
        Row::new(vec![
            "a",
            "Add field/member · on '+ add response' adds a response",
        ]),
        Row::new(vec!["g", "Generate example from the schema"]),
        Row::new(vec!["d", "Delete the selected row"]),
        Row::new(vec!["Ctrl-S", "Save"]),
        Row::new(vec!["q", "Quit"]),
        Row::new(vec!["?", "Toggle this help"]),
    ];

    let popup = centered(area, 70, 50);

    let table = Table::new(rows, [Constraint::Length(12), Constraint::Min(10)])
        .block(
            Block::default()
                .title(" help ")
                .borders(Borders::ALL)
                .padding(Padding::uniform(1)),
        )
        .column_spacing(2);

    frame.render_widget(Clear, popup);
    frame.render_widget(table, popup);
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
                    {"name":"data","type":"object","default":null,"description":"d","required":false,
                     "properties":[{"name":"access_token","type":"string","default":null,"description":"tok","required":true}]}
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
        // The nested field renders with exactly one tree prefix (no doubling).
        assert!(text.contains("access_token"));
        assert!(
            !text.contains("└─ └─") && !text.contains("├─ ├─"),
            "tree prefix is doubled"
        );
        // No box borders (a single └─/├─ tree prefix is allowed).
        for g in ['│', '┌', '┐', '┘', '┤', '┬', '┴', '┼'] {
            assert!(!text.contains(g), "found border glyph {g:?}");
        }
    }

    #[test]
    fn empty_request_body_renders_none_not_example() {
        let c = json_get(
            r#"{ "name":"t","method":"POST",
                 "url":{"protocol":"https","host":"h","path":["x"]},
                 "headers":[],
                 "request":{"type":"object","schema":[]},
                 "responses":[] }"#,
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
        // The REQUEST section shows `(none)` with no empty example/header line.
        let req_idx = text.find("REQUEST").expect("REQUEST section present");
        assert!(
            text[req_idx..].trim_start().starts_with("REQUEST"),
            "REQUEST present"
        );
        assert!(text.contains("(none)"), "empty body shows (none)");
        assert!(
            !text.contains("no example provided"),
            "empty body should not render an Example row"
        );
    }

    #[test]
    fn cell_edit_highlights_only_focused_cell() {
        let c = json_get(
            r#"{ "name":"t","method":"GET",
                 "url":{"protocol":"https","host":"h","path":["x"],
                        "query":[{"name":"page","value":"1","description":"d","required":false}]},
                 "headers":[],"responses":[] }"#,
            None,
        )
        .unwrap();
        let m = EditModel::from_contract(c);
        let mut state = UiState::new(&m);
        // focus the QUERY name cell in cell-edit mode
        let (si, ri) = state
            .sections
            .iter()
            .enumerate()
            .find_map(|(si, s)| {
                s.rows
                    .iter()
                    .position(|r| {
                        r.cells
                            .iter()
                            .any(|c| matches!(c.field, crate::tui::rows::Field::QueryName(_)))
                    })
                    .map(|ri| (si, ri))
            })
            .unwrap();
        state.sec = si;
        state.row = ri;
        state.cell = Some(0);
        let backend = TestBackend::new(100, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();
        // Some cell in the buffer carries the focused-cell highlight background.
        let any_hl = buf
            .content()
            .iter()
            .any(|cell| cell.style().bg == Some(Color::Yellow));
        assert!(any_hl, "expected a highlighted focused cell");
    }

    #[test]
    fn scroll_follows_cursor_to_example_at_bottom() {
        // Many headers push the response example well below a short viewport.
        let mut headers = String::new();
        for i in 0..20 {
            if i > 0 {
                headers.push(',');
            }
            headers.push_str(&format!(r#"{{"name":"H{i}","value":"v{i}"}}"#));
        }
        let json = format!(
            r#"{{ "name":"t","method":"GET",
                 "url":{{"protocol":"https","host":"h","path":["x"]}},
                 "headers":[{headers}],
                 "responses":[{{"code":200,"description":"ok","schema":[],
                    "example":{{"unique_marker_xyz":1}} }}] }}"#
        );
        let c = json_get(&json, None).unwrap();
        let m = EditModel::from_contract(c);
        let mut state = UiState::new(&m);
        // place the cursor on the response example row
        let (si, ri) = state
            .sections
            .iter()
            .enumerate()
            .find_map(|(si, s)| {
                s.rows
                    .iter()
                    .position(|r| r.kind == crate::tui::rows::RowKind::Example)
                    .map(|ri| (si, ri))
            })
            .unwrap();
        state.sec = si;
        state.row = ri;
        state.cell = None;
        let backend = TestBackend::new(80, 12); // short viewport
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let text: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(
            text.contains("unique_marker_xyz"),
            "example should be scrolled into view"
        );
    }
}
