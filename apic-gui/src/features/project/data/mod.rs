//! The project on disk: the native pickers, the project config, validation,
//! initialization and the Postman import. No egui.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

/// Which action consumes the path chosen by an in-flight file dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PickKind {
    /// Open an existing folder as a project.
    OpenProject,
    /// Initialize a fresh project in a folder.
    NewProject,
    /// Import a Postman collection file into the open project.
    ImportPostman,
}

/// Spawns a native dialog on a background thread (so the portal call never
/// freezes the UI) and returns where its result arrives.
///
/// # Arguments
///
/// * `kind` - which dialog to show: a folder picker for a project, a JSON
///   file picker for a Postman import
/// * `notify` - called once the result is sent, so the UI wakes to read it
///
/// # Returns
///
/// A receiver that yields the picked path, or `None` when the user cancels.
pub(crate) fn pick(
    kind: PickKind,
    notify: impl Fn() + Send + 'static,
) -> Receiver<Option<PathBuf>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let picked = match kind {
            PickKind::OpenProject | PickKind::NewProject => {
                let title = if kind == PickKind::OpenProject {
                    "Open apic project"
                } else {
                    "New apic project"
                };
                pollster::block_on(rfd::AsyncFileDialog::new().set_title(title).pick_folder())
            }
            PickKind::ImportPostman => pollster::block_on(
                rfd::AsyncFileDialog::new()
                    .add_filter("Postman collection", &["json"])
                    .set_title("Import Postman collection")
                    .pick_file(),
            ),
        }
        .map(|h| h.path().to_path_buf());
        let _ = tx.send(picked);
        notify();
    });
    rx
}

/// Whether `folder` is already a project, that is it holds
/// `.apic/config.toml`.
pub(crate) fn has_config(folder: &Path) -> bool {
    folder.join(".apic").join("config.toml").is_file()
}

/// Initializes a project in `folder`.
///
/// # Errors
///
/// Returns the init error text, for example when `folder` is already a
/// project.
pub(crate) fn init(folder: &Path) -> Result<(), String> {
    apic_core::config::Config::init_in(folder, None).map(|_| ())
}

/// The contracts working dir named by the config of the project at
/// `project_root`.
///
/// # Errors
///
/// Returns the error text when the config does not read or names an invalid
/// working dir.
pub(crate) fn contracts_root(project_root: &Path) -> Result<PathBuf, String> {
    apic_core::config::read_config_in(project_root).and_then(|c| c.root_dir_in(project_root))
}

/// Imports the Postman collection at `src` into `root` via apic-core's
/// converter, which writes contracts confined to the working dir.
///
/// # Returns
///
/// How many contracts were written and how many warnings the converter gave.
///
/// # Errors
///
/// Returns the converter's error text.
pub(crate) fn import_postman(src: &Path, root: &Path) -> Result<(usize, usize), String> {
    // The GUI import never overwrites; existing contracts must be removed
    // first (or edited), matching the CLI default.
    apic_core::convert::run(src, root, false).map(|out| (out.written, out.warnings.len()))
}
