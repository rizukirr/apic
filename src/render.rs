//! Plain-text rendering of a parsed contract to stdout.
//!
//! Output is column-aligned text with one section per contract part (variable,
//! query, headers, request, responses). Colors are applied only when stdout is
//! a terminal, so piped or redirected output stays clean.

use crate::json::{JsonContent, Method, Request, Schema, Url, method_str};
use crossterm::style::Stylize;
use std::io::IsTerminal;

/// Renders `contract` as formatted text to stdout.
///
/// With `example_mode` the request and response sections print their raw JSON
/// example payloads instead of schema tables.
pub fn render(contract: &JsonContent, example_mode: bool) {
    let p = Printer::new(example_mode);
    p.contract(contract);
}

/// Stateful printer carrying the color-or-plain decision and view mode.
struct Printer {
    color: bool,
    example_mode: bool,
}

impl Printer {
    fn new(example_mode: bool) -> Self {
        Self {
            color: std::io::stdout().is_terminal(),
            example_mode,
        }
    }

    /// Prints the whole contract. Every section is always shown; an empty one
    /// renders a dim `(none)` placeholder rather than being skipped, matching
    /// the TUI viewer so the two stay consistent.
    fn contract(&self, c: &JsonContent) {
        println!(" {}", sanitize(&c.name).to_uppercase());
        if let Some(desc) = &c.description {
            println!(" {}", sanitize(desc));
        }
        println!(
            "\n {} {}",
            self.method(&c.method),
            sanitize(&build_url(&c.url)),
        );

        self.section("VARIABLE");
        match c.url.variable.as_deref() {
            Some(variable) if !variable.is_empty() => {
                let rows: Vec<Vec<String>> = variable
                    .iter()
                    .map(|v| {
                        vec![
                            v.name.clone(),
                            v.dtype.clone(),
                            req_mark(v.required),
                            v.description.clone().unwrap_or_default(),
                        ]
                    })
                    .collect();
                self.table(Some(&["NAME", "TYPE", "REQ", "DESCRIPTION"]), &rows);
            }
            _ => self.none(),
        }

        self.section("QUERY");
        match c.url.query.as_deref() {
            Some(query) if !query.is_empty() => {
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
            _ => self.none(),
        }

        self.section("HEADERS");
        if c.headers.is_empty() {
            self.none();
        } else {
            let rows: Vec<Vec<String>> = c
                .headers
                .iter()
                .map(|h| vec![h.name.clone(), h.value.clone()])
                .collect();
            self.table(None, &rows);
        }

        self.section("REQUEST");
        match &c.request {
            Some(request) if self.example_mode => self.example(request.example.as_ref()),
            Some(request) => match &request.schema {
                Some(schema) if !schema.is_empty() => {
                    let (headers, rows) = request_rows(schema);
                    self.table(Some(&headers), &rows);
                    // Keep the concrete payload adjacent to its schema.
                    if let Some(example) = &request.example {
                        self.example_block(example);
                    }
                }
                // No schema — fall back to the example, or `(none)` when the
                // request carries neither schema nor example.
                _ => match &request.example {
                    Some(example) => self.example(Some(example)),
                    None => self.none(),
                },
            },
            None => self.none(),
        }

        if c.responses.is_empty() {
            self.section("RESPONSE");
            self.none();
        } else {
            for response in &c.responses {
                self.response_title(response.code, &response.description);
                if self.example_mode {
                    self.example(response.example.as_ref());
                } else if !response.schema.is_empty() {
                    let mut rows = Vec::new();
                    schema_rows(&response.schema, 0, &mut rows);
                    self.table(Some(&["NAME", "TYPE", "REQ", "DESCRIPTION"]), &rows);
                    if let Some(example) = &response.example {
                        self.example_block(example);
                    }
                } else {
                    self.example(response.example.as_ref());
                }
            }
        }
    }

    /// Prints a dimmed `Example:` label followed by the JSON payload. Used in
    /// the default view beneath a schema table, only when an example exists.
    fn example_block(&self, example: &serde_json::Value) {
        println!();
        if self.color {
            println!(" {}", "Example:".dark_grey());
        } else {
            println!(" Example:");
        }
        self.example(Some(example));
    }

    /// Prints a raw JSON example payload, pretty-printed and indented, or a
    /// note when none is provided.
    ///
    /// Serializing through serde_json escapes control characters as `\uXXXX`,
    /// so a hostile example cannot inject terminal escape sequences.
    fn example(&self, example: Option<&serde_json::Value>) {
        match example {
            Some(value) => {
                let pretty = serde_json::to_string_pretty(value)
                    .unwrap_or_else(|_| "(unrenderable example)".to_string());
                for line in pretty.lines() {
                    println!(" {line}");
                }
            }
            None => println!(" (no example provided)"),
        }
    }

    /// Prints a dim `(none)` placeholder for an empty section, mirroring the
    /// TUI viewer's `none_line`.
    fn none(&self) {
        if self.color {
            println!(" {}", "(none)".dark_grey());
        } else {
            println!(" (none)");
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
    fn method(&self, method: &Method) -> String {
        if !self.color {
            return method_str(method);
        }
        let method_str = method_str(method);

        match method {
            Method::GET => method_str.green().bold().to_string(),
            Method::POST => method_str.blue().bold().to_string(),
            Method::PUT => method_str.yellow().bold().to_string(),
            Method::PATCH => method_str.magenta().bold().to_string(),
            Method::DELETE => method_str.red().bold().to_string(),
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

/// Assembles the displayable URL from its parts: `protocol://host` followed by
/// the `/`-joined path segments. Each part is optional — an empty `host` yields
/// a leading-slash path, an empty `protocol` drops the scheme, and an empty
/// `path` yields the authority alone.
pub(crate) fn build_url(url: &Url) -> String {
    let path = url.path.as_deref().unwrap_or(&[]).join("/");

    let authority = if url.host.is_empty() {
        String::new()
    } else if url.protocol.is_empty() {
        url.host.clone()
    } else {
        format!("{}://{}", url.protocol, url.host)
    };

    match (authority.is_empty(), path.is_empty()) {
        (true, _) => format!("/{path}"),
        (false, true) => authority,
        (false, false) => format!("{}/{path}", authority.trim_end_matches('/')),
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

    fn url(protocol: &str, host: &str, path: Option<&[&str]>) -> Url {
        Url {
            protocol: protocol.to_string(),
            host: host.to_string(),
            path: path.map(|segs| segs.iter().map(|s| s.to_string()).collect()),
            query: None,
            variable: None,
        }
    }

    #[test]
    fn build_url_joins_protocol_host_and_path() {
        let u = url("https", "api.example.com", Some(&["auth", "login"]));
        assert_eq!(build_url(&u), "https://api.example.com/auth/login");
    }

    #[test]
    fn build_url_drops_scheme_when_protocol_empty() {
        let u = url("", "api.example.com", Some(&["user"]));
        assert_eq!(build_url(&u), "api.example.com/user");
    }

    #[test]
    fn build_url_falls_back_to_leading_slash_path_without_host() {
        let u = url("https", "", Some(&["auth", "login"]));
        assert_eq!(build_url(&u), "/auth/login");
    }

    #[test]
    fn build_url_renders_authority_alone_without_path() {
        let u = url("https", "api.example.com", None);
        assert_eq!(build_url(&u), "https://api.example.com");
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
