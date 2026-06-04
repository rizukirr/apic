//! Plain-text rendering of a parsed contract to stdout.
//!
//! Output is column-aligned text with one section per contract part (params,
//! query, headers, request, responses). Colors are applied only when stdout is
//! a terminal, so piped or redirected output stays clean.

use crate::json::{JsonContent, Request, Schema};
use crossterm::style::Stylize;
use std::io::IsTerminal;

/// Renders `contract` as formatted text to stdout.
pub fn render(contract: &JsonContent) {
    let p = Printer::new();
    p.contract(contract);
}

/// Stateful printer carrying the color-or-plain decision.
struct Printer {
    color: bool,
}

impl Printer {
    fn new() -> Self {
        Self {
            color: std::io::stdout().is_terminal(),
        }
    }

    /// Prints the whole contract, skipping sections that are `None`.
    fn contract(&self, c: &JsonContent) {
        println!(
            " {} {} · {}",
            self.method(&c.method),
            sanitize(&c.path),
            sanitize(&c.name)
        );
        if let Some(desc) = &c.description {
            println!(" {}", sanitize(desc));
        }

        if let Some(params) = &c.params {
            self.section("PARAMS");
            let rows: Vec<Vec<String>> = params
                .iter()
                .map(|p| {
                    vec![
                        p.name.clone(),
                        p.value.clone(),
                        req_mark(p.required),
                        p.description.clone().unwrap_or_default(),
                    ]
                })
                .collect();
            self.table(Some(&["NAME", "VALUE", "REQ", "DESCRIPTION"]), &rows);
        }

        if let Some(query) = &c.query {
            self.section("QUERY");
            let rows: Vec<Vec<String>> = query
                .iter()
                .map(|q| {
                    vec![
                        q.name.clone(),
                        q.value.clone(),
                        req_mark(q.required),
                        q.description.clone().unwrap_or_default(),
                    ]
                })
                .collect();
            self.table(Some(&["NAME", "VALUE", "REQ", "DESCRIPTION"]), &rows);
        }

        if !c.headers.is_empty() {
            self.section("HEADERS");
            let rows: Vec<Vec<String>> = c
                .headers
                .iter()
                .map(|h| vec![h.name.clone(), h.value.clone()])
                .collect();
            self.table(None, &rows);
        }

        if let Some(request) = &c.request {
            self.section("REQUEST");
            let (headers, rows) = request_rows(request);
            self.table(Some(&headers), &rows);
        }

        for response in &c.responses {
            self.response_title(response.code, &response.description);
            let mut rows = Vec::new();
            schema_rows(&response.schema, 0, &mut rows);
            self.table(Some(&["NAME", "TYPE", "REQ", "DESCRIPTION"]), &rows);
        }
    }

    /// Prints a blank line followed by a bold section title.
    fn section(&self, title: &str) {
        println!();
        if self.color {
            println!(" {}", title.bold());
        } else {
            println!(" {title}");
        }
    }

    /// Prints the `RESPONSE <code> — <description>` section title, coloring
    /// the status code by its class (2xx green, 4xx/5xx red).
    fn response_title(&self, code: u16, description: &str) {
        println!();
        let description = sanitize(description);
        if self.color {
            let code = code.to_string();
            let code = match code.as_bytes()[0] {
                b'2' => code.green().bold(),
                b'4' | b'5' => code.red().bold(),
                _ => code.yellow().bold(),
            };
            println!(" {} {code} — {description}", "RESPONSE".bold());
        } else {
            println!(" RESPONSE {code} — {description}");
        }
    }

    /// Returns the HTTP method, colored by convention when output is a terminal.
    fn method(&self, method: &str) -> String {
        let method = sanitize(method);
        let method = method.as_str();
        if !self.color {
            return method.to_string();
        }
        match method.to_uppercase().as_str() {
            "GET" => method.green().bold().to_string(),
            "POST" => method.blue().bold().to_string(),
            "PUT" => method.yellow().bold().to_string(),
            "PATCH" => method.magenta().bold().to_string(),
            "DELETE" => method.red().bold().to_string(),
            _ => method.bold().to_string(),
        }
    }

    /// Prints `rows` as a column-aligned table, with an optional dimmed
    /// header row. Widths are computed over the plain (uncolored) strings so
    /// alignment is never thrown off by escape codes.
    fn table(&self, headers: Option<&[&str]>, rows: &[Vec<String>]) {
        let cols = match (headers, rows.first()) {
            (Some(h), _) => h.len(),
            (None, Some(r)) => r.len(),
            (None, None) => return,
        };

        // Cells carry untrusted file content; strip control characters before
        // measuring widths so escapes can neither reach the terminal nor throw
        // off column alignment. Header labels are static literals and trusted.
        let rows: Vec<Vec<String>> = rows
            .iter()
            .map(|row| row.iter().map(|cell| sanitize(cell)).collect())
            .collect();
        let rows = &rows;

        let mut widths = vec![0usize; cols];
        if let Some(headers) = headers {
            for (w, h) in widths.iter_mut().zip(headers) {
                *w = h.chars().count();
            }
        }
        for row in rows {
            for (w, cell) in widths.iter_mut().zip(row) {
                *w = (*w).max(cell.chars().count());
            }
        }

        let fmt_line = |cells: &[String]| -> String {
            cells
                .iter()
                .zip(&widths)
                .map(|(cell, w)| format!("{cell:<w$}"))
                .collect::<Vec<_>>()
                .join("  ")
                .trim_end()
                .to_string()
        };

        if let Some(headers) = headers {
            let cells: Vec<String> = headers.iter().map(|h| h.to_string()).collect();
            let line = fmt_line(&cells);
            if self.color {
                println!(" {}", line.dark_grey());
            } else {
                println!(" {line}");
            }
        }
        for row in rows {
            println!(" {}", fmt_line(row));
        }
    }
}

/// Builds the REQUEST table headers and rows.
///
/// The ACCEPT column (allowed MIME types for `file` fields in multipart
/// requests) is included only when at least one field declares it, so
/// ordinary JSON-body contracts keep the compact four-column table.
fn request_rows(request: &[Request]) -> (Vec<&'static str>, Vec<Vec<String>>) {
    let has_accept = request.iter().any(|r| r.accept.is_some());
    let headers = if has_accept {
        vec!["NAME", "TYPE", "REQ", "ACCEPT", "DESCRIPTION"]
    } else {
        vec!["NAME", "TYPE", "REQ", "DESCRIPTION"]
    };

    let rows = request
        .iter()
        .map(|r| {
            let mut row = vec![r.name.clone(), r.dtype.clone(), req_mark(r.required)];
            if has_accept {
                row.push(r.accept.clone().unwrap_or_default());
            }
            row.push(r.description.clone());
            row
        })
        .collect();

    (headers, rows)
}

/// Flattens a schema (and its nested `properties`) into table rows, prefixing
/// nested names with `├─`/`└─` tree branches per depth level.
fn schema_rows(schemas: &[Schema], depth: usize, out: &mut Vec<Vec<String>>) {
    for (i, s) in schemas.iter().enumerate() {
        let prefix = if depth == 0 {
            String::new()
        } else {
            let branch = if i + 1 == schemas.len() {
                "└─ "
            } else {
                "├─ "
            };
            format!("{}{branch}", "  ".repeat(depth - 1))
        };

        out.push(vec![
            format!("{prefix}{}", s.name),
            s.dtype.clone(),
            req_mark(s.required),
            s.description.clone(),
        ]);

        if let Some(props) = &s.properties {
            schema_rows(props, depth + 1, out);
        }
    }
}

/// Marks a required field in table output.
fn req_mark(required: bool) -> String {
    if required {
        "✓".to_string()
    } else {
        String::new()
    }
}

/// Strips control characters from a file-derived string before it is printed.
///
/// Contract files are untrusted input; without this, embedded ANSI/OSC escape
/// sequences (e.g. `ESC[2J`, `OSC 0;…BEL`) would reach the terminal and could
/// clear the screen, rewrite the title bar, or spoof output. The tool's own
/// styling is applied *after* sanitization, so legitimate colors are kept.
pub(crate) fn sanitize(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::Schema;

    #[test]
    fn sanitize_strips_escape_and_bell_sequences() {
        // Regression: untrusted contract strings must not inject terminal escapes.
        let evil = "\x1b[2J\x1b[31mHACKED\x1b[0m\x07";
        let clean = sanitize(evil);
        assert!(!clean.contains('\x1b'), "ESC survived: {clean:?}");
        assert!(!clean.contains('\x07'), "BEL survived: {clean:?}");
        // Readable content is preserved (minus the control bytes).
        assert!(clean.contains("HACKED"));
    }

    #[test]
    fn sanitize_keeps_normal_and_multibyte_text() {
        assert_eq!(sanitize("café /auth/login"), "café /auth/login");
    }

    #[test]
    fn req_mark_renders_check_only_when_required() {
        assert_eq!(req_mark(true), "✓");
        assert_eq!(req_mark(false), "");
    }

    fn field(name: &str, properties: Option<Vec<Schema>>) -> Schema {
        Schema {
            name: name.to_string(),
            dtype: "string".to_string(),
            default: None,
            description: String::new(),
            required: true,
            properties,
        }
    }

    fn req_field(name: &str, dtype: &str, accept: Option<&str>) -> Request {
        Request {
            name: name.to_string(),
            dtype: dtype.to_string(),
            default: None,
            description: String::new(),
            required: true,
            accept: accept.map(str::to_string),
        }
    }

    #[test]
    fn request_rows_without_accept_keeps_four_columns() {
        let (headers, rows) = request_rows(&[req_field("username", "string", None)]);
        assert_eq!(headers, vec!["NAME", "TYPE", "REQ", "DESCRIPTION"]);
        assert_eq!(rows[0].len(), 4);
    }

    #[test]
    fn request_rows_with_file_field_adds_accept_column() {
        let fields = [
            req_field("avatar", "file", Some("image/png, image/jpeg")),
            req_field("caption", "string", None),
        ];
        let (headers, rows) = request_rows(&fields);
        assert_eq!(
            headers,
            vec!["NAME", "TYPE", "REQ", "ACCEPT", "DESCRIPTION"]
        );
        // The file field shows its accepted types; the plain field stays blank.
        assert_eq!(rows[0][3], "image/png, image/jpeg");
        assert_eq!(rows[1][3], "");
    }

    #[test]
    fn schema_rows_flattens_nested_properties_with_tree_prefixes() {
        let schema = vec![field(
            "data",
            Some(vec![field("first", None), field("last", None)]),
        )];
        let mut rows = Vec::new();
        schema_rows(&schema, 0, &mut rows);

        // Top-level name has no prefix; nested names get tree branches.
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0][0], "data");
        assert_eq!(rows[1][0], "├─ first");
        assert_eq!(rows[2][0], "└─ last");
    }
}
