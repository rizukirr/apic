//! Plain-text rendering of a parsed contract to stdout.
//!
//! Output is column-aligned text with one section per contract part (url,
//! query, headers, request, responses). Colors are applied only when stdout is
//! a terminal and `NO_COLOR` is unset, so piped or redirected output stays
//! clean. JSON bodies are syntax highlighted under the same rule.

use apic_core::json::{JsonContent, Method, Part, method_str};
use crossterm::style::Stylize;
use std::io::IsTerminal;

/// Whether output should be colored: stdout is a terminal and `NO_COLOR` is
/// unset or empty (<https://no-color.org>).
pub(crate) fn use_color() -> bool {
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
}

/// Renders `contract` as formatted text to stdout.
pub(crate) fn render(contract: &JsonContent) {
    let p = Printer::new();
    p.contract(contract);
}

/// Stateful printer carrying the color-or-plain decision.
struct Printer {
    color: bool,
}

impl Printer {
    fn new() -> Self {
        Self { color: use_color() }
    }

    /// Prints the whole contract. Every section is always shown; an empty one
    /// renders a dim `(none)` placeholder rather than being skipped, so every
    /// contract reads with the same sections in the same order.
    fn contract(&self, c: &JsonContent) {
        println!(" {}", sanitize(&c.name).to_uppercase());
        if let Some(desc) = &c.description {
            println!(" {}", sanitize(desc));
        }
        println!("\n {} {}", self.method(&c.method), sanitize(&c.url),);

        self.section("QUERY");
        if c.query.is_empty() {
            self.none();
        } else {
            let rows: Vec<Vec<String>> = c
                .query
                .iter()
                .map(|q| {
                    vec![
                        q.name.clone(),
                        q.value.clone(),
                        q.description.clone().unwrap_or_default(),
                    ]
                })
                .collect();
            self.table(Some(&["name", "value", "description"]), &rows);
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
        match (&c.request, &c.multipart) {
            (_, Some(parts)) => self.parts(parts),
            (Some(body), None) => self.example(Some(body)),
            (None, None) => self.none(),
        }

        if c.responses.is_empty() {
            self.section("RESPONSE");
            self.none();
        } else {
            for response in &c.responses {
                self.response_title(response.code, &response.description);
                match &response.multipart {
                    Some(parts) => self.parts(parts),
                    None => self.example(response.schema.as_ref()),
                }
            }
        }
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
                    if self.color {
                        println!(" {}", highlight_json_line(line));
                    } else {
                        println!(" {line}");
                    }
                }
            }
            None => println!(" (no example provided)"),
        }
    }

    /// Prints a multipart body as a table, one row per part. The kind column is
    /// derived from `filename` rather than read from a stored field, which is
    /// why a part cannot claim to be text while naming a file.
    ///
    /// Cells go in raw: `table` strips control characters from every cell it
    /// prints, so an escape sequence in a part name cannot reach the terminal.
    fn parts(&self, parts: &[Part]) {
        let rows: Vec<Vec<String>> = parts
            .iter()
            .map(|p| {
                let (kind, value) = match &p.filename {
                    Some(name) => ("file", name.as_str()),
                    None => ("text", p.value.as_str()),
                };
                vec![
                    p.name.clone(),
                    kind.to_string(),
                    value.to_string(),
                    p.content_type.as_deref().unwrap_or("").to_string(),
                    p.description.as_deref().unwrap_or("").to_string(),
                    p.required.to_string(),
                ]
            })
            .collect();
        self.table(
            Some(&["name", "kind", "value", "type", "description", "required"]),
            &rows,
        );
    }

    /// Prints a dim `(none)` placeholder for an empty section.
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
            println!(" {} {code}, {description}", "RESPONSE".bold());
        } else {
            println!(" RESPONSE {code}, {description}");
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
            Method::HEAD => method_str.cyan().bold().to_string(),
            Method::OPTIONS => method_str.white().bold().to_string(),
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

/// Colors one line of pretty-printed JSON by token: keys blue, string values
/// green, numbers yellow, `true`/`false`/`null` magenta, and `{ } [ ] : ,`
/// dim. Uses the terminal's basic colors so the user's theme decides the
/// exact shades.
///
/// The input comes from `serde_json::to_string_pretty`, which escapes every
/// control character, so no escape sequence from a contract can survive into
/// the colored output.
pub(crate) fn highlight_json_line(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                let start = i;
                i += 1;
                while i < bytes.len() {
                    match bytes[i] {
                        b'\\' => i += 2,
                        b'"' => {
                            i += 1;
                            break;
                        }
                        _ => i += 1,
                    }
                }
                i = i.min(bytes.len());
                let token = &line[start..i];
                // A string is a key when the next non-space byte is ':'.
                let is_key = line[i..].trim_start().starts_with(':');
                if is_key {
                    out.push_str(&token.blue().to_string());
                } else {
                    out.push_str(&token.green().to_string());
                }
            }
            b'-' | b'0'..=b'9' => {
                let start = i;
                i += 1;
                while i < bytes.len()
                    && matches!(bytes[i], b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
                {
                    i += 1;
                }
                out.push_str(&line[start..i].yellow().to_string());
            }
            b't' | b'f' | b'n' => {
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let word = &line[start..i];
                if matches!(word, "true" | "false" | "null") {
                    out.push_str(&word.magenta().to_string());
                } else {
                    out.push_str(word);
                }
            }
            b'{' | b'}' | b'[' | b']' | b':' | b',' => {
                out.push_str(&line[i..i + 1].dark_grey().to_string());
                i += 1;
            }
            _ => {
                // Whitespace and anything else passes through unchanged.
                let start = i;
                i += 1;
                while i < bytes.len()
                    && !matches!(
                        bytes[i],
                        b'"' | b'-' | b'0'
                            ..=b'9' | b't' | b'f' | b'n' | b'{' | b'}' | b'[' | b']' | b':' | b','
                    )
                {
                    i += 1;
                }
                out.push_str(&line[start..i]);
            }
        }
    }
    out
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
    fn json_tokens_get_their_colors() {
        let line = r#"  "message": "ok", "count": 2, "error": null, "ok": true,"#;
        let out = highlight_json_line(line);
        assert!(out.contains(&"\"message\"".blue().to_string()));
        assert!(out.contains(&"\"ok\"".green().to_string()));
        assert!(out.contains(&"2".yellow().to_string()));
        assert!(out.contains(&"null".magenta().to_string()));
        assert!(out.contains(&"true".magenta().to_string()));
        assert!(out.contains(&":".dark_grey().to_string()));
    }

    #[test]
    fn highlighting_keeps_the_text_of_every_line() {
        let value = serde_json::json!({
            "a": [1, -2.5e3, "x \"q\" y"], "b": { "c": false, "d": null }, "e": "naïve"
        });
        let pretty = serde_json::to_string_pretty(&value).expect("serializes");
        for line in pretty.lines() {
            let colored = highlight_json_line(line);
            // Stripping the ANSI sequences must give back the original line.
            let mut plain = String::new();
            let mut chars = colored.chars();
            while let Some(c) = chars.next() {
                if c == '\x1b' {
                    for c in chars.by_ref() {
                        if c == 'm' {
                            break;
                        }
                    }
                } else {
                    plain.push(c);
                }
            }
            assert_eq!(plain, line);
        }
    }

    #[test]
    fn sanitize_keeps_normal_and_multibyte_text() {
        assert_eq!(sanitize("café /auth/login"), "café /auth/login");
    }

    #[test]
    fn a_part_is_a_file_when_it_names_one_and_text_otherwise() {
        let contract = r#"{
            "name": "x", "method": "POST", "url": "https://h", "headers": [],
            "multipart": [
                { "name": "avatar", "filename": "photo.png", "contentType": "image/png", "required": true },
                { "name": "caption", "value": "my holiday" }
            ],
            "responses": []
        }"#;
        let parsed = apic_core::json::json_get(contract, None).expect("parses");
        let parts = parsed.multipart.expect("parts");
        assert_eq!(parts[0].filename.as_deref(), Some("photo.png"));
        assert!(parts[1].filename.is_none());
    }
}
