//! The API-contract feature: editing, viewing, and browsing `.apic` contracts
//! and the project templates.
//!
//! Three layers, each importing only from the ones before it:
//!
//! - `domain`: pure contract rules over plain data. No filesystem, process,
//!   thread or egui.
//! - `data`: contract and template files on disk. No egui.
//! - `ui`: the view model (`ui::state`) and the rendering (`ui::view`).
//!
//! No module here imports `crate::features::git`, `crate::features::project`
//! or `crate::app` outside its tests. The `layer_rules` test below checks this.

pub(crate) mod data;
pub(crate) mod domain;
pub(crate) mod ui;

#[cfg(test)]
mod layer_rules {
    use std::path::{Path, PathBuf};

    /// Every `.rs` file under `dir`, at any depth.
    fn rust_files(dir: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(dir).expect("layer directory reads") {
            let path = entry.expect("directory entry reads").path();
            if path.is_dir() {
                files.extend(rust_files(&path));
            } else if path.extension().is_some_and(|e| e == "rs") {
                files.push(path);
            }
        }
        files
    }

    /// The code lines of `text` before its test module, skipping comments,
    /// so a doc comment saying "no egui" does not count as using egui and
    /// test fixtures may borrow from the app shell.
    fn code_lines(text: &str) -> Vec<&str> {
        text.lines()
            .take_while(|line| !line.trim_start().starts_with("#[cfg(test)]"))
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect()
    }

    /// Fails naming every file and line under `layer` that mentions one of
    /// `forbidden`.
    fn assert_layer_avoids(layer: &str, forbidden: &[&str]) {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/features/contracts")
            .join(layer);
        let files = rust_files(&dir);
        assert!(!files.is_empty(), "no source files found under {dir:?}");
        let mut violations = Vec::new();
        for file in files {
            let text = std::fs::read_to_string(&file).expect("source file reads");
            for line in code_lines(&text) {
                if let Some(word) = forbidden.iter().find(|w| line.contains(*w)) {
                    violations.push(format!("{}: {word}: {}", file.display(), line.trim()));
                }
            }
        }
        assert!(
            violations.is_empty(),
            "layer `{layer}` breaks its rules:\n{}",
            violations.join("\n")
        );
    }

    #[test]
    fn domain_has_no_io_and_no_ui() {
        assert_layer_avoids(
            "domain",
            &[
                "egui",
                "eframe",
                "std::fs",
                "std::process",
                "std::thread",
                "contracts::data",
                "contracts::ui",
                "crate::app",
                "features::git",
                "features::project",
            ],
        );
    }

    #[test]
    fn data_has_no_ui() {
        assert_layer_avoids(
            "data",
            &[
                "egui",
                "eframe",
                "contracts::ui",
                "crate::app",
                "features::git",
                "features::project",
            ],
        );
    }

    #[test]
    fn ui_does_not_reach_into_other_features_or_the_shell() {
        assert_layer_avoids("ui", &["crate::app", "features::git", "features::project"]);
    }
}
