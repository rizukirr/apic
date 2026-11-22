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

    /// Feature folders under `src/` that draw only through base and
    /// components. A folder joins once its migration lands, and from then on
    /// it is held to the same rule as components.
    const MIGRATED: &[&str] = &[];

    /// Raw egui calls that build text or buttons outside base, so their look
    /// is chosen at the call site instead of by a base component.
    const RAW_WIDGETS: &[&str] = &["RichText::new", "egui::Button::", "ui.button("];

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
    /// forbid, and without its test module, which by convention closes the
    /// file and may draw freely.
    fn code(path: &Path) -> String {
        let text = std::fs::read_to_string(path).expect("source file reads");
        let text = match text.find("#[cfg(test)]") {
            Some(at) => &text[..at],
            None => &text,
        };
        text.lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
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
    fn migrated_features_draw_through_base_and_components() {
        for dir in MIGRATED {
            assert!(src().join(dir).is_dir(), "{dir} is not a folder under src/");
            let bad = drawing_in(dir);
            assert!(bad.is_empty(), "{dir} must draw through base: {bad:#?}");
        }
    }

    #[test]
    fn the_drawing_check_catches_raw_widgets_and_literal_sizes() {
        let code = r#"ui.label(RichText::new("x").size(11.0)); ui.button("y");"#;
        assert_eq!(
            drawing_violations(code),
            ["RichText::new", "ui.button(", ".size(<number>)"]
        );
        assert!(drawing_violations("Text::new(\"x\").size(Size::Caption).show(ui);").is_empty());
    }
}
