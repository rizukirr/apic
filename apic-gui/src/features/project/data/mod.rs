//! The project on disk: the native pickers, the project config, validation,
//! initialization, and the Postman and OpenAPI imports and exports. No egui.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

/// Which action consumes the path chosen by an in-flight file dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PickKind {
    /// Open an existing folder as a project.
    OpenProject,
    /// Initialize a fresh project in a folder.
    NewProject,
    /// Import a Postman collection or OpenAPI spec into the open project, or
    /// pick one for an import into a new project when none is open.
    Import(ImportSource),
    /// Pick the folder that becomes the project an import is written into.
    ImportTarget,
    /// Pick the file an export of the open project is saved to.
    Export(ImportSource),
    /// Pick the folder that becomes the project a new request or template
    /// is created in, when none is open.
    NewTarget(NewKind),
}

/// The format of a file picked for an import or saved by an export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImportSource {
    /// A Postman collection (JSON).
    Postman,
    /// An OpenAPI 3.x or Swagger 2.0 spec (JSON or YAML).
    OpenApi,
}

/// Which dialog a folder picked for a new request or template goes on to
/// open once it is the project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NewKind {
    Request,
    Template,
}

/// Spawns a native dialog on a background thread (so the portal call never
/// freezes the UI) and returns where its result arrives.
///
/// # Arguments
///
/// * `kind` - which dialog to show: a folder picker for a project, a JSON
///   file picker for an import
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
            PickKind::OpenProject
            | PickKind::NewProject
            | PickKind::ImportTarget
            | PickKind::NewTarget(_) => {
                let title = match kind {
                    PickKind::OpenProject => "Open apic project",
                    PickKind::ImportTarget => "Folder for the imported project",
                    PickKind::NewTarget(NewKind::Request) => "Folder for the new request",
                    PickKind::NewTarget(NewKind::Template) => "Folder for the new template",
                    PickKind::NewProject | PickKind::Import(_) | PickKind::Export(_) => {
                        "New apic project"
                    }
                };
                pollster::block_on(rfd::AsyncFileDialog::new().set_title(title).pick_folder())
            }
            PickKind::Import(ImportSource::Postman) => pollster::block_on(
                rfd::AsyncFileDialog::new()
                    .add_filter("Postman collection", &["json"])
                    .set_title("Import Postman collection")
                    .pick_file(),
            ),
            // Export saves through its own dialog, see `pick_save`.
            PickKind::Export(_) => None,
            PickKind::Import(ImportSource::OpenApi) => pollster::block_on(
                rfd::AsyncFileDialog::new()
                    .add_filter("OpenAPI spec", &["json", "yaml", "yml"])
                    .set_title("Import OpenAPI spec")
                    .pick_file(),
            ),
        }
        .map(|h| h.path().to_path_buf());
        let _ = tx.send(picked);
        notify();
    });
    rx
}

/// Spawns a native save dialog for an export on a background thread, like
/// [`pick`].
///
/// # Arguments
///
/// * `format` - what is exported, which sets the filter and title
/// * `file_name` - the suggested file name
/// * `dir` - the folder the dialog opens in
/// * `notify` - called once the result is sent, so the UI wakes to read it
///
/// # Returns
///
/// A receiver that yields the chosen path, or `None` when the user cancels.
pub(crate) fn pick_save(
    format: ImportSource,
    file_name: String,
    dir: Option<PathBuf>,
    notify: impl Fn() + Send + 'static,
) -> Receiver<Option<PathBuf>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (filter, exts, title): (&str, &[&str], &str) = match format {
            ImportSource::Postman => ("Postman collection", &["json"], "Export Postman collection"),
            ImportSource::OpenApi => (
                "OpenAPI spec",
                &["yaml", "yml", "json"],
                "Export OpenAPI spec",
            ),
        };
        let mut dialog = rfd::AsyncFileDialog::new()
            .add_filter(filter, exts)
            .set_title(title)
            .set_file_name(file_name);
        if let Some(dir) = dir {
            dialog = dialog.set_directory(dir);
        }
        let picked = pollster::block_on(dialog.save_file()).map(|h| h.path().to_path_buf());
        let _ = tx.send(picked);
        notify();
    });
    rx
}

/// Exports every contract in the project at `project_root` to `out`, through
/// apic-core's exporter. OpenAPI is written as 3.0, as YAML or JSON by the
/// file's extension.
///
/// # Arguments
///
/// * `format` - Postman or OpenAPI
/// * `project_root` - the project folder, whose name titles the export
/// * `root` - the contracts folder
/// * `out` - the file to write, overwritten (the save dialog already asked)
/// * `env` - the environment whose shared file fills variable values. Its
///   local file is never read.
///
/// # Returns
///
/// How many contracts were exported and how many warnings there were.
///
/// # Errors
///
/// Returns the error text when there is nothing to export, the environment
/// does not load, or the file does not write.
pub(crate) fn export(
    format: ImportSource,
    project_root: &Path,
    root: &Path,
    out: &Path,
    env: Option<&str>,
) -> Result<(usize, usize), String> {
    use apic_core::export::{Format, load_items, openapi::Version, write};
    let apic_dir = project_root.join(".apic");
    let mut paths: Vec<PathBuf> = apic_core::json::scan_json_file(root, true)
        .unwrap_or_default()
        .into_iter()
        .filter(|p| !p.starts_with(&apic_dir))
        .collect();
    paths.sort();
    let (items, mut warnings) = load_items(&paths, root);
    if items.is_empty() {
        return Err("no contracts to export".to_string());
    }
    let values = match env {
        Some(env) => apic_core::env::load_shared(&apic_dir, env)?,
        None => apic_core::vars::Vars::new(),
    };
    let name = project_root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "apic".to_string());
    let format = match format {
        ImportSource::Postman => Format::Postman,
        ImportSource::OpenApi => Format::OpenApi(Version::V3_0),
    };
    warnings.extend(write(format, &items, &values, &name, out, root, &apic_dir)?);
    Ok((items.len(), warnings.len()))
}

/// Whether `folder` is already a project, that is it holds
/// `.apic/config.toml`.
pub(crate) fn has_config(folder: &Path) -> bool {
    folder.join(".apic").join("config.toml").is_file()
}

/// The project that holds `path`: the nearest folder above it, starting at
/// its parent, that holds `.apic/config.toml` as a file.
///
/// # Returns
///
/// That folder canonicalized, or `None` when no folder above `path` is a
/// project or the folder does not canonicalize.
pub(crate) fn project_root_of(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .skip(1)
        .find(|folder| has_config(folder))
        .and_then(|folder| folder.canonicalize().ok())
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

/// Imports the Postman collection or OpenAPI spec at `src` into `root` via
/// apic-core's converter, which writes contracts confined to the working dir.
///
/// # Returns
///
/// How many contracts were written and how many warnings the converter gave.
///
/// # Errors
///
/// Returns the converter's error text.
pub(crate) fn import(
    source: ImportSource,
    src: &Path,
    root: &Path,
) -> Result<(usize, usize), String> {
    // The GUI import never overwrites; existing contracts must be removed
    // first (or edited), matching the CLI default.
    let run = match source {
        ImportSource::Postman => apic_core::convert::run,
        ImportSource::OpenApi => apic_core::convert::run_openapi,
    };
    run(src, root, false).map(|out| (out.written, out.warnings.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_support::tempdir;

    /// Makes `folder` a project by writing an empty `.apic/config.toml`.
    fn make_project(folder: &Path) {
        std::fs::create_dir_all(folder.join(".apic")).expect("apic dir creates");
        std::fs::write(folder.join(".apic").join("config.toml"), "").expect("config writes");
    }

    #[test]
    fn project_root_of_finds_the_nearest_apic_root() {
        let outer = tempdir();
        make_project(&outer);
        let contract = outer.join("contracts").join("users").join("get.json");
        std::fs::create_dir_all(contract.parent().expect("contract has a parent"))
            .expect("contract dir creates");
        std::fs::write(&contract, "{}").expect("contract writes");
        let outer_canon = outer.canonicalize().expect("outer canonicalizes");
        assert_eq!(project_root_of(&contract), Some(outer_canon));

        let inner = outer.join("contracts").join("nested");
        make_project(&inner);
        let inner_contract = inner.join("x.json");
        std::fs::write(&inner_contract, "{}").expect("inner contract writes");
        let inner_canon = inner.canonicalize().expect("inner canonicalizes");
        assert_eq!(project_root_of(&inner_contract), Some(inner_canon));

        let loose = tempdir().join("loose.json");
        std::fs::write(&loose, "{}").expect("loose file writes");
        assert_eq!(project_root_of(&loose), None);
    }
}
