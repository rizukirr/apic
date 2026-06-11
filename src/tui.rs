use crate::json::{Header, JsonContent, Method, Query, Schema, Variable, any_accept, method_str};
use crate::render::{array_marker, build_url, sanitize};
use crossterm::event::{self, KeyCode};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::{DefaultTerminal, Frame};
use serde_json::Value;

/// Opens `contract` in the interactive TUI viewer.
pub fn tui(contract: JsonContent) -> Result<(), String> {
    match ratatui::run(|terminal| app(terminal, &contract)) {
        Ok(()) => Ok(()),
        Err(err) => Err(format!("ratatui error: {err}")),
    }
}

/// Top-level tabs over the request metadata sections.
const TABS: [&str; 3] = ["Headers", "Queries", "Params"];

/// Where the cursor sits. `←/→` act on the focused group; `↑/↓` move the cursor
/// between the top tab group and the response tab group.
#[derive(Default, PartialEq)]
enum Focus {
    #[default]
    Tabs,
    Response,
}

/// Whether keystrokes drive navigation or the bottom text input.
#[derive(Default, PartialEq)]
enum Mode {
    #[default]
    Normal,
    Insert,
}

#[derive(Default)]
struct State {
    tab: usize,
    response: usize,
    focus: Focus,
    mode: Mode,
    input: String,
}

fn parse(input: &str, contract: &JsonContent) {
    let tokens: Vec<&str> = input.split_whitespace().collect();
    if tokens[0] == "method" && tokens[1] == "set" {
        // TODO: method set <[GET|POST|PUT|PATCH|DELETE]>
    }

    if tokens[0] == "url" && tokens[1] == "set" {
        // TODO: url set <protocol> <host> <path>
        // or
        // TODO: url set --protocol <protocol> --host <host> --path <path>
    }

    // add new header
    if tokens[0] == "header" && tokens[1] == "add" {
        // TODO: header add <key> <value>
        // or
        // TODO: header add --key <key> --value <value>
    }

    // update existing header
    if tokens[0] == "header" && tokens[1] == "set" {
        // TODO: header set <key> <value>
        // or
        // TODO: header set --key <key> --value <value>
        // or
        // TODO: header set --key <key> --new-key <new-key>
    }

    if tokens[0] == "query" && tokens[1] == "add" {
        // TODO: query add <name> <value> <description> <required>
        // or
        // TODO: query add --name <name> --value <value> --required <required> --description <description>
    }

    // update existing query by name
    if tokens[0] == "query" && tokens[1] == "set" {
        // TODO: query set <name> <value> <description> <required>
        // or
        // TODO: query set --name <name> --value <value> --required <required> --description <description>
        // or
        // TODO: query set --name <name> --new-name <new-name>
    }

    if tokens[0] == "variable" && tokens[1] == "add" {
        // TODO: variable add <name> <type> <description> <required>
        // or
        // TODO: variable add --name <name> --type <type> --description <description> --required <required> --accept <accept>
    }

    if tokens[0] == "variable" && tokens[1] == "set" {
        // TODO: variable set <name> <type> <description> <required>
        // or
        // TODO: variable set --name <name> --type <type> --description <description> --required <required> --accept <accept>
        // or
        // TODO: variable set --name <name> --new-name <new-name>
    }

    if tokens[0] == "request" && tokens[1] == "add" {
        // TODO: request add <name> <type> <required> <description>
    }
}

fn app(terminal: &mut DefaultTerminal, contract: &JsonContent) -> std::io::Result<()> {
    let mut state = State::default();
    let response_max = contract.responses.len().saturating_sub(1);
    loop {
        terminal.draw(|frame| render(frame, contract, &state))?;
        if let Some(key) = event::read()?.as_key_press_event() {
            match state.mode {
                // Insert mode: keystrokes edit the bottom text input.
                Mode::Insert => match key.code {
                    KeyCode::Esc => state.mode = Mode::Normal,
                    KeyCode::Enter => {
                        parse(state.input.as_str(), contract);
                    }
                    KeyCode::Backspace => {
                        state.input.pop();
                    }
                    KeyCode::Char(c) => state.input.push(c),
                    _ => {}
                },
                // Normal mode: navigation.
                Mode::Normal => match key.code {
                    KeyCode::Esc => return Ok(()),
                    KeyCode::Char('i') | KeyCode::Char('/') => state.mode = Mode::Insert,
                    // Up/Down jump the cursor between the tab header and the response.
                    KeyCode::Up | KeyCode::Char('k') => state.focus = Focus::Tabs,
                    KeyCode::Down | KeyCode::Char('j') => state.focus = Focus::Response,
                    // Left/Right navigate items within the focused group.
                    KeyCode::Left | KeyCode::Char('h') => match state.focus {
                        Focus::Tabs => state.tab = state.tab.saturating_sub(1),
                        Focus::Response => state.response = state.response.saturating_sub(1),
                    },
                    KeyCode::Right | KeyCode::Char('l') => match state.focus {
                        Focus::Tabs => state.tab = (state.tab + 1).min(TABS.len() - 1),
                        Focus::Response => state.response = (state.response + 1).min(response_max),
                    },
                    _ => {}
                },
            }
        }
    }
}

fn render(frame: &mut Frame, contract: &JsonContent, state: &State) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints(vec![
            Constraint::Length(1), // name
            Constraint::Length(1), // endpoint
            Constraint::Length(1), // spacer
            Constraint::Min(0),    // document
            Constraint::Length(1), // input
        ])
        .split(frame.area());

    frame.render_widget(
        Line::from(Span::from(sanitize(&contract.name)).style(Style::default().bold())),
        layout[0],
    );

    let endpoint = Line::from(vec![
        Span::from(format!("{} ", method_str(&contract.method)))
            .style(method_style(&contract.method)),
        Span::from(sanitize(&build_url(&contract.url))).style(yellow()),
    ]);
    frame.render_widget(endpoint, layout[1]);

    // The body is one vertical document: no borders, tabs as highlighted text,
    // so it copies/pastes cleanly.
    frame.render_widget(Paragraph::new(document(contract, state)), layout[3]);

    // Borderless text input pinned to the bottom row.
    let input = layout[4];
    const PROMPT: &str = "> ";
    let input_line = if state.mode == Mode::Insert || !state.input.is_empty() {
        Line::from(vec![
            Span::from(PROMPT).style(gray()),
            Span::from(state.input.clone()),
        ])
    } else {
        Line::from(Span::from("press i to type · Esc to quit").style(gray()))
    };
    frame.render_widget(input_line, input);

    if state.mode == Mode::Insert {
        // Place the real terminal cursor at the end of the typed text.
        let cursor_x = input.x + PROMPT.len() as u16 + state.input.chars().count() as u16;
        let max_x = input.x + input.width.saturating_sub(1);
        frame.set_cursor_position((cursor_x.min(max_x), input.y));
    }
}

fn document(contract: &JsonContent, state: &State) -> Vec<Line<'static>> {
    let mut doc = Vec::new();

    if let Some(description) = &contract.description {
        doc.push(Line::from(Span::from(sanitize(description)).style(gray())));
        doc.push(Line::from(""));
    }

    // header / query / param tab row + the selected tab's content.
    doc.push(Line::from(tab_spans(
        &TABS,
        state.tab,
        state.focus == Focus::Tabs,
    )));
    doc.push(Line::from(""));
    match state.tab {
        0 if !contract.headers.is_empty() => doc.extend(header_table(&contract.headers)),
        1 => match contract.url.query.as_deref() {
            Some(query) if !query.is_empty() => doc.extend(query_table(query)),
            _ => doc.push(none_line()),
        },
        2 => match contract.url.variable.as_deref() {
            Some(variable) if !variable.is_empty() => doc.extend(param_table(variable)),
            _ => doc.push(none_line()),
        },
        _ => doc.push(none_line()),
    }

    // request section.
    doc.push(Line::from(""));
    let request_label = match &contract.request {
        Some(r) => format!("REQUEST{}", array_marker(&r.dtype)),
        None => "REQUEST".to_string(),
    };
    doc.push(Line::from(
        Span::from(request_label).style(Style::default().bold()),
    ));
    doc.push(Line::from(""));
    match &contract.request {
        Some(request) => {
            if let Some(schema) = &request.schema
                && !schema.is_empty()
            {
                doc.extend(field_table(schema));
                doc.push(Line::from(""));
            }
            doc.extend(example_block(request.example.as_ref()));
        }
        None => doc.push(none_line()),
    }

    // response section (a tab group of status codes).
    doc.push(Line::from(""));
    if contract.responses.is_empty() {
        doc.push(Line::from(
            Span::from("RESPONSE").style(Style::default().bold()),
        ));
        doc.push(none_line());
    } else {
        doc.extend(response_section(
            contract,
            state.response,
            state.focus == Focus::Response,
        ));
    }

    doc
}

/// Renders the selected response: header, schema table and example.
fn response_section(contract: &JsonContent, selected: usize, focused: bool) -> Vec<Line<'static>> {
    let response = &contract.responses[selected];
    let code = response.code.to_string();

    let code_style = if focused {
        Style::default().fg(Color::White).on_green().bold()
    } else {
        Style::default().bold().fg(status_color(&code))
    };
    let marker = array_marker(&response.dtype);
    let head = Line::from(vec![
        Span::from("RESPONSE ").style(Style::default().bold()),
        Span::from(code).style(code_style),
        Span::from(format!(" — {}{marker}", sanitize(&response.description))).style(gray()),
    ]);

    let mut lines = vec![head, Line::from("")];
    if !response.schema.is_empty() {
        lines.extend(field_table(&response.schema));
        lines.push(Line::from(""));
    }
    lines.extend(example_block(response.example.as_ref()));
    lines
}

/// Green for 2xx, red for 4xx/5xx, yellow otherwise.
fn status_color(code: &str) -> Color {
    match code.chars().next() {
        Some('2') => Color::Green,
        Some('4') | Some('5') => Color::Red,
        _ => Color::Yellow,
    }
}

/// Colors an HTTP method by convention.
fn method_style(method: &Method) -> Style {
    let base = Style::default().bold();
    match method {
        Method::GET => base.fg(Color::Green),
        Method::POST => base.fg(Color::Blue),
        Method::PUT => base.fg(Color::Yellow),
        Method::PATCH => base.fg(Color::Magenta),
        Method::DELETE => base.fg(Color::Red),
    }
}

/// Renders the tabs as styled spans. The selected tab is highlighted on green
/// when its group is focused, and just bold when it is not.
fn tab_spans(labels: &[&str], selected: usize, focused: bool) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (index, label) in labels.iter().enumerate() {
        if index > 0 {
            spans.push(Span::from(" · "));
        }
        let style = if index != selected {
            Style::default().fg(Color::Gray)
        } else if focused {
            Style::default().fg(Color::White).on_green().bold()
        } else {
            Style::default().bold()
        };
        spans.push(Span::from((*label).to_string()).style(style));
    }
    spans
}

// --- Section tables ---------------------------------------------------------

fn header_table(headers: &[Header]) -> Vec<Line<'static>> {
    let rows: Vec<Vec<(String, Style)>> = headers
        .iter()
        .map(|header| {
            vec![
                (sanitize(&header.name), Style::default()),
                (sanitize(&header.value), yellow()),
            ]
        })
        .collect();
    table(&["NAME", "VALUE"], &rows)
}

fn query_table(queries: &[Query]) -> Vec<Line<'static>> {
    let rows: Vec<Vec<(String, Style)>> = queries
        .iter()
        .map(|query| {
            vec![
                (sanitize(&query.name), Style::default()),
                (sanitize(&query.value), yellow()),
                (req_mark(query.required), green()),
                (sanitize(query.description.as_deref().unwrap_or("")), gray()),
            ]
        })
        .collect();
    table(&["NAME", "VALUE", "REQ", "DESCRIPTION"], &rows)
}

fn param_table(variables: &[Variable]) -> Vec<Line<'static>> {
    let rows: Vec<Vec<(String, Style)>> = variables
        .iter()
        .map(|variable| {
            vec![
                (sanitize(&variable.name), Style::default()),
                (sanitize(&variable.dtype), yellow()),
                (req_mark(variable.required), green()),
                (
                    sanitize(variable.description.as_deref().unwrap_or("")),
                    gray(),
                ),
            ]
        })
        .collect();
    table(&["NAME", "TYPE", "REQ", "DESCRIPTION"], &rows)
}

/// Renders a request/response field list as an aligned table. The ACCEPT
/// column appears only when some field declares it; nested `properties` are
/// drawn as a `├─`/`└─` tree.
fn field_table(fields: &[Schema]) -> Vec<Line<'static>> {
    let has_accept = any_accept(fields);
    let headers: Vec<&str> = if has_accept {
        vec!["NAME", "TYPE", "REQ", "ACCEPT", "DESCRIPTION"]
    } else {
        vec!["NAME", "TYPE", "REQ", "DESCRIPTION"]
    };
    let mut rows = Vec::new();
    push_field_rows(fields, "", true, has_accept, &mut rows);
    table(&headers, &rows)
}

fn push_field_rows(
    fields: &[Schema],
    ancestor: &str,
    is_root: bool,
    has_accept: bool,
    rows: &mut Vec<Vec<(String, Style)>>,
) {
    let count = fields.len();
    for (index, field) in fields.iter().enumerate() {
        let last = index + 1 == count;
        let name = if is_root {
            sanitize(&field.name)
        } else {
            format!(
                "{ancestor}{} {}",
                if last { "└─" } else { "├─" },
                sanitize(&field.name)
            )
        };
        let mut row = vec![
            (name, Style::default()),
            (sanitize(&field.dtype), yellow()),
            (req_mark(field.required), green()),
        ];
        if has_accept {
            row.push((sanitize(field.accept.as_deref().unwrap_or("")), gray()));
        }
        row.push((sanitize(&field.description), gray()));
        rows.push(row);

        if let Some(properties) = &field.properties {
            let child_ancestor = if is_root {
                String::new()
            } else {
                format!("{ancestor}{}", if last { "   " } else { "│  " })
            };
            push_field_rows(properties, &child_ancestor, false, has_accept, rows);
        }
    }
}

/// `✓` for a required field, blank otherwise.
fn req_mark(required: bool) -> String {
    if required {
        "✓".to_string()
    } else {
        String::new()
    }
}

fn yellow() -> Style {
    Style::default().fg(Color::Yellow)
}
fn green() -> Style {
    Style::default().fg(Color::Green)
}
fn gray() -> Style {
    Style::default().fg(Color::DarkGray)
}

/// A dim `(none)` placeholder for an empty section.
fn none_line() -> Line<'static> {
    Line::from(Span::from("(none)").style(gray()))
}

/// Renders an aligned table: a bold header row followed by `rows`. Every column
/// but the last is padded to its widest cell; each cell carries its own style.
fn table(headers: &[&str], rows: &[Vec<(String, Style)>]) -> Vec<Line<'static>> {
    let cols = headers.len();
    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (index, (text, _)) in row.iter().enumerate() {
            if index < cols {
                widths[index] = widths[index].max(text.chars().count());
            }
        }
    }

    // The last column is never padded so it can run to the edge / be clipped.
    let cell = |index: usize, text: &str| {
        if index + 1 == cols {
            text.to_string()
        } else {
            pad(text, widths[index])
        }
    };

    let header_style = Style::default().bold().fg(Color::DarkGray);
    let mut header_spans = Vec::new();
    for (index, label) in headers.iter().enumerate() {
        if index > 0 {
            header_spans.push(Span::from("  "));
        }
        header_spans.push(Span::from(cell(index, label)).style(header_style));
    }
    let mut lines = vec![Line::from(header_spans)];

    for row in rows {
        let mut spans = Vec::new();
        for (index, (text, style)) in row.iter().enumerate() {
            if index > 0 {
                spans.push(Span::from("  "));
            }
            spans.push(Span::from(cell(index, text)).style(*style));
        }
        lines.push(Line::from(spans));
    }
    lines
}

/// Left-pads `text` with spaces to `width` display columns.
fn pad(text: &str, width: usize) -> String {
    let len = text.chars().count();
    let mut out = text.to_string();
    if len < width {
        out.push_str(&" ".repeat(width - len));
    }
    out
}

// --- Example (JSON) ---------------------------------------------------------

/// An `Example:` label followed by the pretty-printed, coloured JSON body, or a
/// note when no example is provided.
fn example_block(example: Option<&Value>) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(
        Span::from("Example:").style(Style::default().bold()),
    )];
    match example {
        Some(value) => push_json(&mut lines, Vec::new(), value, 0, ""),
        None => lines.push(Line::from(
            Span::from("(no example provided)").style(gray()),
        )),
    }
    lines
}

/// Appends `value` as pretty JSON lines (two-space indent): keys cyan, string
/// values yellow, literals magenta. `suffix` is the trailing `,` (if any).
/// Strings/keys are serialized through serde_json so control characters are
/// escaped and cannot reach the terminal.
fn push_json(
    lines: &mut Vec<Line<'static>>,
    mut prefix: Vec<Span<'static>>,
    value: &Value,
    indent: usize,
    suffix: &'static str,
) {
    match value {
        Value::Object(map) => {
            prefix.push(Span::from("{"));
            lines.push(Line::from(prefix));
            for (index, (key, val)) in map.iter().enumerate() {
                let key_prefix = vec![
                    Span::from("  ".repeat(indent + 1)),
                    Span::from(quote(key)).style(Style::default().fg(Color::Cyan)),
                    Span::from(": "),
                ];
                let next = if index + 1 < map.len() { "," } else { "" };
                push_json(lines, key_prefix, val, indent + 1, next);
            }
            lines.push(Line::from(format!("{}}}{}", "  ".repeat(indent), suffix)));
        }
        Value::Array(items) => {
            prefix.push(Span::from("["));
            lines.push(Line::from(prefix));
            for (index, item) in items.iter().enumerate() {
                let item_prefix = vec![Span::from("  ".repeat(indent + 1))];
                let next = if index + 1 < items.len() { "," } else { "" };
                push_json(lines, item_prefix, item, indent + 1, next);
            }
            lines.push(Line::from(format!("{}]{}", "  ".repeat(indent), suffix)));
        }
        Value::String(string) => {
            prefix.push(Span::from(quote(string)).style(Style::default().fg(Color::Yellow)));
            prefix.push(Span::from(suffix));
            lines.push(Line::from(prefix));
        }
        other => {
            prefix.push(Span::from(other.to_string()).style(Style::default().fg(Color::Magenta)));
            prefix.push(Span::from(suffix));
            lines.push(Line::from(prefix));
        }
    }
}

/// Quotes and escapes `text` as a JSON string (escaping control characters).
fn quote(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| format!("\"{}\"", sanitize(text)))
}
