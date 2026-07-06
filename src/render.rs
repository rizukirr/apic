//! Plain-text rendering of a parsed contract to stdout.
//!
//! Output is column-aligned text with one section per contract part (url,
//! query, headers, request, responses). Colors are applied only when stdout is
//! a terminal, so piped or redirected output stays clean.

use apic_core::json::{JsonContent, Method, method_str};
use crossterm::style::Stylize;
use std::io::IsTerminal;

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
        Self {
            color: std::io::stdout().is_terminal(),
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
        match &c.request {
            Some(body) => self.example(Some(body)),
            None => self.none(),
        }

        if c.responses.is_empty() {
            self.section("RESPONSE");
            self.none();
        } else {
            for response in &c.responses {
                self.response_title(response.code, &response.description);
                self.example(response.schema.as_ref());
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
    fn sanitize_keeps_normal_and_multibyte_text() {
        assert_eq!(sanitize("café /auth/login"), "café /auth/login");
    }
}
