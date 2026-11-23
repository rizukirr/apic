//! Presentation layer: theme tokens, the base and component toolkit, and
//! deferred focus plumbing.
//!
//! The layers, innermost first:
//!
//! - [`theme`]: colors, the type scale and spacing tokens.
//! - [`base`]: single widgets that hold no other component (text, buttons,
//!   inputs, chips, menu items). Styled only through theme tokens.
//! - [`components`]: global pieces built from more than one base component
//!   (dialogs, tabs, tables, the URL bar, nested menus, sidebar headers).
//!
//! Dependencies only point inward: features use base and components,
//! components use base, base uses theme. Nothing in this module tree may
//! import from `crate::features` or name a domain type such as `EditModel`;
//! feature-specific rendering belongs in the feature's own view. The tests in
//! [`layering`] check these rules on the source.

pub(crate) mod base;
pub(crate) mod components;
pub(crate) mod focus;
pub(crate) mod syntax_highlighting;
pub(crate) mod theme;

#[cfg(test)]
mod layering {
    use std::path::{Path, PathBuf};

    /// The folders under `src/` that draw only through base and components,
    /// held to the same rule as components. Whole trees, so a new feature is
    /// checked from its first line.
    const FEATURE_CODE: &[&str] = &["features", "app"];

    /// Raw egui calls that build text or buttons outside base, so their look
    /// is chosen at the call site instead of by a base component.
    /// `.button(` also catches `ui` and `.button(` split across lines, and
    /// `.menu_button(` does not match the `row_menu_button(` component.
    const RAW_WIDGETS: &[&str] = &[
        "RichText::new",
        "egui::Button::",
        ".button(",
        ".menu_button(",
        "ComboBox::",
        "selectable_label(",
        "selectable_value(",
        "TextEdit::singleline",
        "TextEdit::multiline",
        ".checkbox(",
        "Checkbox::",
        "CollapsingHeader::",
        "CollapsingState::",
        "egui::Label::new",
        ".separator()",
        "Separator::",
        ".spinner()",
    ];

    fn src() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
    }

    /// Every `.rs` file under `dir`, recursively.
    fn rs_files(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("source dir reads") {
                let path = entry.expect("dir entry reads").path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(path);
                }
            }
        }
        out.sort();
        out
    }

    /// A file's code without comment lines, which may name what the rules
    /// forbid, and without its `#[cfg(test)]` modules, which may draw freely.
    /// Only each test module's own braced block is removed, so code after it
    /// is still checked.
    fn code(path: &Path) -> String {
        let text = std::fs::read_to_string(path).expect("source file reads");
        strip_comments(&strip_test_modules(&text))
    }

    /// `text` without comment lines.
    fn strip_comments(text: &str) -> String {
        text.lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// `text` with every `#[cfg(test)]` attribute and the item after it
    /// removed: a braced item through its closing `}`, or an item without a
    /// body, such as `mod test_support;`, through its `;`.
    fn strip_test_modules(text: &str) -> String {
        let mut out = String::new();
        let mut rest = text;
        while let Some(at) = rest.find("#[cfg(test)]") {
            out.push_str(&rest[..at]);
            let after = &rest[at..];
            let semi = after.find(';');
            rest = match after.find('{') {
                Some(open) if semi.is_none_or(|semi| open < semi) => {
                    &after[block_end(after, open)..]
                }
                _ => semi.map_or("", |semi| &after[semi + 1..]),
            };
        }
        out.push_str(rest);
        out
    }

    /// The byte just past the `}` that closes the `{` at `open`. Braces in
    /// strings, raw strings, char literals and line comments are skipped.
    fn block_end(text: &str, open: usize) -> usize {
        let bytes = text.as_bytes();
        let mut depth = 0usize;
        let mut i = open;
        while i < bytes.len() {
            match bytes[i] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return i + 1;
                    }
                }
                b'/' if bytes.get(i + 1) == Some(&b'/') => {
                    i = text[i..].find('\n').map_or(bytes.len(), |n| i + n);
                }
                b'r' if matches!(bytes.get(i + 1), Some(b'#' | b'"')) => {
                    let hashes = text[i + 1..].bytes().take_while(|b| *b == b'#').count();
                    let quote = i + 1 + hashes;
                    if bytes.get(quote) == Some(&b'"') {
                        let close = format!("\"{}", "#".repeat(hashes));
                        i = text[quote + 1..]
                            .find(&close)
                            .map_or(bytes.len(), |n| quote + n + close.len());
                    }
                }
                b'"' => {
                    i += 1;
                    while i < bytes.len() && bytes[i] != b'"' {
                        i += if bytes[i] == b'\\' { 2 } else { 1 };
                    }
                }
                b'\'' if bytes.get(i + 2) == Some(&b'\'') => i += 2,
                b'\'' if bytes.get(i + 1) == Some(&b'\\') => {
                    i = text[i + 2..].find('\'').map_or(bytes.len(), |n| i + 2 + n);
                }
                _ => {}
            }
            i += 1;
        }
        bytes.len()
    }

    /// What in `code` breaks the drawing rule: a raw widget, or a text size
    /// written as a number instead of a type-scale token.
    fn drawing_violations(code: &str) -> Vec<String> {
        let mut found: Vec<String> = RAW_WIDGETS
            .iter()
            .filter(|raw| code.contains(*raw))
            .map(|raw| raw.to_string())
            .collect();
        let literal_size = code
            .match_indices(".size(")
            .any(|(at, m)| code[at + m.len()..].starts_with(|c: char| c.is_ascii_digit()));
        if literal_size {
            found.push(".size(<number>)".to_string());
        }
        let literal_space = code
            .match_indices("add_space(")
            .any(|(at, m)| code[at + m.len()..].starts_with(|c: char| c.is_ascii_digit()));
        if literal_space {
            found.push("add_space(<number>)".to_string());
        }
        found
    }

    /// Each file under `dir` (relative to `src/`) whose code contains any of
    /// `forbidden`, with what it contains.
    fn imports_in(dir: &str, forbidden: &[&str]) -> Vec<String> {
        rs_files(&src().join(dir))
            .into_iter()
            .filter_map(|path| {
                let code = code(&path);
                let hits: Vec<&str> = forbidden
                    .iter()
                    .copied()
                    .filter(|f| code.contains(f))
                    .collect();
                (!hits.is_empty()).then(|| format!("{}: {hits:?}", path.display()))
            })
            .collect()
    }

    /// Each file under `dir` that breaks the drawing rule.
    fn drawing_in(dir: &str) -> Vec<String> {
        rs_files(&src().join(dir))
            .into_iter()
            .filter_map(|path| {
                let hits = drawing_violations(&code(&path));
                (!hits.is_empty()).then(|| format!("{}: {hits:?}", path.display()))
            })
            .collect()
    }

    #[test]
    fn base_imports_only_theme() {
        let bad = imports_in(
            "ui/base",
            &["crate::ui::components", "crate::features", "crate::app"],
        );
        assert!(bad.is_empty(), "base must not import these: {bad:#?}");
    }

    #[test]
    fn components_never_import_features() {
        let bad = imports_in("ui/components", &["crate::features", "crate::app"]);
        assert!(bad.is_empty(), "components must not import these: {bad:#?}");
    }

    #[test]
    fn components_draw_through_base() {
        let bad = drawing_in("ui/components");
        assert!(
            bad.is_empty(),
            "components must draw through base: {bad:#?}"
        );
    }

    #[test]
    fn features_and_app_draw_through_base_and_components() {
        for dir in FEATURE_CODE {
            assert!(src().join(dir).is_dir(), "{dir} is not a folder under src/");
            let bad = drawing_in(dir);
            assert!(bad.is_empty(), "{dir} must draw through base: {bad:#?}");
        }
    }

    #[test]
    fn test_modules_are_skipped_but_code_after_them_is_checked() {
        let text = "fn a() {}\n#[cfg(test)]\nmod tests {\n    fn t() { let s = \"}\"; let r = r#\"{\"#; let c = '}'; RichText::new(\"in test\"); }\n}\nfn after() { RichText::new(\"x\"); }\n";
        let kept = strip_test_modules(text);
        assert!(!kept.contains("in test"), "{kept}");
        assert!(kept.contains("fn after()"), "{kept}");
        assert_eq!(drawing_violations(&kept), ["RichText::new"]);
    }

    #[test]
    fn a_test_only_module_declaration_removes_only_itself() {
        let text = "#[cfg(test)]\nmod test_support;\nfn app() { RichText::new(\"x\"); }\n";
        let kept = strip_test_modules(text);
        assert!(kept.contains("fn app()"), "{kept}");
        assert_eq!(drawing_violations(&kept), ["RichText::new"]);
    }

    #[test]
    fn the_drawing_check_catches_a_literal_space() {
        assert_eq!(
            drawing_violations("ui.add_space(16.0);"),
            ["add_space(<number>)"]
        );
        assert!(drawing_violations("ui.add_space(SPACE_LARGE);").is_empty());
    }

    #[test]
    fn the_drawing_check_catches_raw_widgets_and_literal_sizes() {
        let code = r#"ui.label(RichText::new("x").size(11.0)); ui.button("y");"#;
        assert_eq!(
            drawing_violations(code),
            ["RichText::new", ".button(", ".size(<number>)"]
        );
        assert!(drawing_violations("Text::new(\"x\").size(Size::Caption).show(ui);").is_empty());
    }
}
