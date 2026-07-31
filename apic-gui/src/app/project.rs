//! Project-level operations: opening, creating, reloading, and importing.
//!
//! These are methods on [`App`] rather than on a feature state because each one
//! spans both `shell` and `contracts`; keeping them here is what lets the two
//! state structs stay unaware of each other.

use std::path::{Path, PathBuf};

use apic_core::edit::EditModel;
use apic_core::json::method_str;
use eframe::egui;

use crate::app::App;
use crate::app::state::DialogKind;
use crate::features::contracts::state::{DeleteTarget, Entry, MainTab, Repair, RespTab};
use crate::settings::Settings;

/// Renders `path` with the home directory collapsed to `~` (forward-slashed),
/// reusing `apic_core::file::{to_slash, home_relative}` so the footer matches the
/// CLI and no logic is duplicated.
fn display_location(path: &Path) -> String {
    apic_core::file::home_relative(&apic_core::file::to_slash(path))
}

impl App {
    /// Discovers contracts for the active project and reads each one's method for
    /// the sidebar badge. Resolves everything against `self.shell.project_root`; never
    /// reads the process current directory.
    pub(crate) fn reload_project(&mut self) {
        let Some(root) = self.shell.project_root.clone() else {
            self.shell.apic_dir = None;
            self.shell.root = None;
            self.contracts.templates.clear();
            self.contracts.entries.clear();
            self.shell.status = "No project open. Use Open or New.".into();
            return;
        };

        self.shell.apic_dir = Some(root.join(".apic"));
        self.contracts.templates = self
            .shell
            .apic_dir
            .as_deref()
            .map(|dir| {
                apic_core::template::list_templates(dir)
                    .into_iter()
                    .map(|p| {
                        let name = p
                            .file_stem()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        (name, p)
                    })
                    .collect()
            })
            .unwrap_or_default();

        match apic_core::config::read_config_in(&root).and_then(|c| c.root_dir_in(&root)) {
            Ok(contracts_root) => {
                // `self.shell.root` is the contracts working dir consumed by import /
                // new-request / delete; keep it in sync with the active project.
                self.shell.root = Some(contracts_root.clone());
                let failures = apic_core::validate_dir(&contracts_root);
                let mut paths =
                    apic_core::json::scan_json_file(&contracts_root, true).unwrap_or_default();
                paths.sort();
                self.contracts.entries = paths
                    .into_iter()
                    .filter(|p| !p.components().any(|c| c.as_os_str() == ".apic"))
                    .map(|path| {
                        let rel = apic_core::file::relative_slash(&path, &contracts_root);
                        let method = apic_core::file::read_file(&path)
                            .ok()
                            .and_then(|t| apic_core::json::json_get(&t, None).ok())
                            .map(|c| method_str(&c.method))
                            .unwrap_or_else(|| "?".to_string());
                        let error = failures
                            .iter()
                            .find(|(p, _)| *p == path)
                            .map(|(_, e)| e.clone());
                        Entry {
                            path,
                            rel,
                            method,
                            error,
                        }
                    })
                    .collect();
                self.shell.status = display_location(&contracts_root);
            }
            Err(err) => {
                self.shell.root = None;
                self.contracts.entries.clear();
                self.shell.status =
                    apic_core::file::home_relative(&format!("Project error: {err}"));
            }
        }
    }

    /// Loads an invalid contract's raw text into the repair editor.
    pub(crate) fn enter_repair(&mut self, i: usize) {
        let Some(entry) = self.contracts.entries.get(i) else {
            return;
        };
        let buffer = apic_core::file::read_file(&entry.path).unwrap_or_default();
        let error = entry.error.clone().unwrap_or_default();
        self.contracts.model = None;
        self.contracts.original_model = None;
        self.contracts.selected = Some(i);
        self.contracts.selected_template = None;
        self.contracts.repair = Some(Repair {
            index: i,
            buffer,
            error,
        });
    }

    /// Loads entry `i` into the editable model.
    pub(crate) fn load(&mut self, i: usize) {
        let Some(entry) = self.contracts.entries.get(i) else {
            return;
        };
        let path = entry.path.clone();
        let loaded = apic_core::file::read_file(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| apic_core::json::json_get(&t, None).map_err(|e| e.to_string()))
            .map(EditModel::from_contract);
        match loaded {
            Ok(model) => {
                self.contracts.model = Some(model);
                self.contracts.path = Some(path);
                self.contracts.selected = Some(i);
                self.contracts.selected_template = None;
                self.contracts.resp_tab = 0;
                self.contracts.main_tab = MainTab::Overview;
                self.contracts.resp_tab_view = RespTab::Body;
                self.contracts.editing = false;
                self.contracts.original_model = None;
                self.shell.status = self
                    .contracts
                    .path
                    .as_deref()
                    .map(display_location)
                    .unwrap_or_default();
            }
            Err(err) => self.shell.status = format!("load error: {err}"),
        }
    }

    /// Loads template `i` into the editor, resolved against the built-in default
    /// into a full contract. Editable and savable like any contract: `path` keeps
    /// the template file so Save writes the edited contract straight back to it.
    /// (Saving a resolved template makes it a full template, every field it then
    /// contains is enforced when creating contracts from it.)
    pub(crate) fn load_template(&mut self, i: usize) {
        let Some((name, path)) = self.contracts.templates.get(i).cloned() else {
            return;
        };
        match apic_core::template::resolve_contract_from(&path)
            .and_then(|(c, _w)| apic_core::json::json_get(&c, None).map_err(|e| e.to_string()))
        {
            Ok(contract) => {
                self.contracts.model = Some(EditModel::from_contract(contract));
                self.contracts.path = Some(path);
                self.contracts.selected = None;
                self.contracts.selected_template = Some(i);
                self.contracts.resp_tab = 0;
                self.contracts.main_tab = MainTab::Overview;
                self.contracts.resp_tab_view = RespTab::Body;
                self.contracts.editing = false;
                self.contracts.original_model = None;
                self.shell.status = format!("template '{name}'");
            }
            Err(err) => self.shell.status = format!("template error: {err}"),
        }
    }

    /// `[ Open ]`: launch the folder picker; `finish_open` runs on the result.
    pub(crate) fn open_project(&mut self, ctx: &egui::Context) {
        self.spawn_folder_dialog(DialogKind::OpenProject, "Open apic project", ctx);
    }

    /// `[ New ]`: launch the folder picker; `finish_new` runs on the result.
    pub(crate) fn new_project(&mut self, ctx: &egui::Context) {
        self.spawn_folder_dialog(DialogKind::NewProject, "New apic project", ctx);
    }

    /// Verify a chosen folder, then open / auto-init / block.
    pub(crate) fn finish_open(&mut self, folder: PathBuf) {
        let has_apic = folder.join(".apic").join("config.toml").is_file();
        if has_apic {
            self.activate_project(folder);
            return;
        }

        // No project: validate the folder's contracts before auto-initializing.
        let failures = apic_core::validate_dir(&folder);
        if failures.is_empty() {
            match apic_core::config::Config::init_in(&folder, None) {
                Ok(_) => self.activate_project(folder),
                Err(e) => self.shell.status = format!("init error: {e}"),
            }
        } else {
            self.contracts.open_blocked = Some(failures);
        }
    }

    /// Initialize a fresh project in `folder` (opening it if it already is one).
    pub(crate) fn finish_new(&mut self, folder: PathBuf) {
        match apic_core::config::Config::init_in(&folder, None) {
            Ok(_) | Err(_) => self.activate_project(folder), // Err = already a project
        }
    }

    /// Spawns a native dialog on a background thread (so the portal call never
    /// freezes the UI) and records what to do with the result; polled by
    /// [`App::poll_dialog`]. A second dialog cannot start while one is pending.
    pub(crate) fn spawn_folder_dialog(
        &mut self,
        kind: DialogKind,
        title: &'static str,
        ctx: &egui::Context,
    ) {
        if self.pending_dialog.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let picked =
                pollster::block_on(rfd::AsyncFileDialog::new().set_title(title).pick_folder())
                    .map(|h| h.path().to_path_buf());
            let _ = tx.send(picked);
            ctx.request_repaint();
        });
        self.pending_dialog = Some((kind, rx));
        self.shell.status = "Waiting for the file dialog…".into();
    }

    /// Polls the in-flight dialog and runs its action once a path is chosen (or
    /// clears it on cancel). Called every frame from `update`.
    pub(crate) fn poll_dialog(&mut self, ctx: &egui::Context) {
        let Some((kind, rx)) = &self.pending_dialog else {
            return;
        };
        match rx.try_recv() {
            Ok(result) => {
                let kind = *kind;
                self.pending_dialog = None;
                match (kind, result) {
                    (DialogKind::OpenProject, Some(p)) => self.finish_open(p),
                    (DialogKind::NewProject, Some(p)) => self.finish_new(p),
                    (DialogKind::ImportPostman, Some(p)) => self.finish_import_postman(p),
                    (_, None) => {}
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint(),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => self.pending_dialog = None,
        }
    }

    /// Makes `folder` the active project: reload, then persist as last project.
    pub(crate) fn activate_project(&mut self, folder: PathBuf) {
        self.shell.project_root = Some(folder.clone());
        self.contracts.model = None;
        self.contracts.selected = None;
        self.contracts.selected_template = None;
        self.contracts.repair = None;
        self.reload_project();
        Settings {
            last_project: Some(folder),
        }
        .save();
    }

    /// `[ Import ]` → Postman: launch the file picker (background thread).
    pub(crate) fn import_postman(&mut self, ctx: &egui::Context) {
        if self.shell.root.is_none() {
            self.shell.status = "no project to import into".into();
            return;
        }
        if self.pending_dialog.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let picked = pollster::block_on(
                rfd::AsyncFileDialog::new()
                    .add_filter("Postman collection", &["json"])
                    .set_title("Import Postman collection")
                    .pick_file(),
            )
            .map(|h| h.path().to_path_buf());
            let _ = tx.send(picked);
            ctx.request_repaint();
        });
        self.pending_dialog = Some((DialogKind::ImportPostman, rx));
        self.shell.status = "Waiting for the file dialog…".into();
    }

    /// Imports a Postman collection into the project via apic-core's converter,
    /// which writes contracts confined to the working dir and never overwrites.
    pub(crate) fn finish_import_postman(&mut self, src: PathBuf) {
        let Some(root) = self.shell.root.clone() else {
            self.shell.status = "no project to import into".into();
            return;
        };
        // The GUI import never overwrites; existing contracts must be removed
        // first (or edited), matching the CLI default.
        match apic_core::convert::run(&src, &root, false) {
            Ok(out) => {
                self.reload_project();
                let warn = if out.warnings.is_empty() {
                    String::new()
                } else {
                    format!(", {} warning(s)", out.warnings.len())
                };
                self.shell.status = format!("imported {} contract(s){warn}", out.written);
            }
            Err(e) => self.shell.status = format!("import error: {e}"),
        }
    }

    /// Creates a new template `<name>.json` in `.apic/template/`, seeded from the
    /// built-in default. Safety: the path is confined to the template dir, and an
    /// existing template is never overwritten.
    pub(crate) fn create_template(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            self.shell.status = "template name required".into();
            return;
        }
        let Some(apic_dir) = self.shell.apic_dir.clone() else {
            self.shell.status = "no project".into();
            return;
        };
        let dir = apic_core::template::dir(&apic_dir);
        let file_name = if name.ends_with(".json") {
            name.to_string()
        } else {
            format!("{name}.json")
        };
        let dest = match apic_core::file::confine_to_dir(&dir, Path::new(&file_name)) {
            Ok(p) => p,
            Err(e) => {
                self.shell.status = e;
                return;
            }
        };
        if dest.exists() {
            self.shell.status = format!("template '{name}' already exists");
            return;
        }
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.shell.status = format!("create dir error: {e}");
            return;
        }
        match std::fs::write(&dest, apic_core::template::DEFAULT) {
            Ok(()) => {
                self.reload_project();
                // Open the freshly created template in the central view, the same
                // way create_request opens a new contract.
                if let Some(i) = self
                    .contracts
                    .templates
                    .iter()
                    .position(|(_, p)| *p == dest)
                {
                    self.load_template(i);
                }
                self.shell.status = format!("created template '{name}'");
            }
            Err(e) => self.shell.status = format!("write error: {e}"),
        }
    }

    /// Creates a new request from the dialog input, relative to the contracts
    /// root. A name ending in `/` creates a folder; any other name creates a
    /// contract file (with `.json` appended when the user did not type it),
    /// seeded from `template` (or the built-in default when `None`) and opened.
    /// Intermediate folders in the path are created as needed.
    ///
    /// Safety: the path is confined to the working dir (rejecting `..`/symlink
    /// escapes) and an existing file is never overwritten.
    pub(crate) fn create_request(&mut self, input: &str, template: Option<PathBuf>) {
        let input = input.trim();
        if input.is_empty() {
            self.shell.status = "name required".into();
            return;
        }
        let Some(root) = self.shell.root.clone() else {
            self.shell.status = "no project".into();
            return;
        };
        let is_folder = input.ends_with('/');
        let rel = if is_folder {
            input.trim_end_matches('/').to_string()
        } else if input.ends_with(".json") {
            input.to_string()
        } else {
            format!("{input}.json")
        };
        if rel.is_empty() {
            self.shell.status = "name required".into();
            return;
        }
        let dest = match apic_core::file::confine_to_dir(&root, Path::new(&rel)) {
            Ok(p) => p,
            Err(e) => {
                self.shell.status = e;
                return;
            }
        };

        if !is_folder {
            if dest.exists() {
                self.shell.status = format!("{rel} already exists, not overwriting");
                return;
            }
            // Seed from the chosen template (merged onto the built-in default),
            // or the built-in default itself when there is no template.
            let contract = match &template {
                Some(path) => match apic_core::template::resolve_contract_from(path) {
                    Ok((c, _warnings)) => c,
                    Err(e) => {
                        self.shell.status = format!("template error: {e}");
                        return;
                    }
                },
                None => apic_core::template::DEFAULT.to_string(),
            };
            if let Some(parent) = dest.parent()
                && let Err(e) = std::fs::create_dir_all(parent)
            {
                self.shell.status = format!("create dir error: {e}");
                return;
            }
            match std::fs::write(&dest, contract) {
                Ok(()) => {
                    self.reload_project();
                    if let Some(i) = self.contracts.entries.iter().position(|e| e.path == dest) {
                        self.load(i);
                    }
                    self.shell.status = format!("created {rel}");
                }
                Err(e) => self.shell.status = format!("write error: {e}"),
            }
        } else {
            match std::fs::create_dir_all(&dest) {
                Ok(()) => {
                    self.reload_project();
                    self.shell.status = format!("created folder {rel}/");
                }
                Err(e) => self.shell.status = format!("create dir error: {e}"),
            }
        }
    }

    /// Removes the target (confined to its directory), then reloads. If the open
    /// contract/template was deleted, the editor is cleared.
    pub(crate) fn perform_delete(&mut self, target: &DeleteTarget) {
        let (removed_path, result, label) = match target {
            DeleteTarget::Contract { rel, is_dir } => {
                let Some(root) = self.shell.root.clone() else {
                    self.shell.status = "no project".into();
                    return;
                };
                let dest = match apic_core::file::confine_to_dir(&root, Path::new(rel)) {
                    Ok(p) => p,
                    Err(e) => {
                        self.shell.status = e;
                        return;
                    }
                };
                let r = if *is_dir {
                    std::fs::remove_dir_all(&dest)
                } else {
                    std::fs::remove_file(&dest)
                };
                (dest, r, rel.clone())
            }
            DeleteTarget::Template { name, path } => {
                // Confine to the template dir so only a real template is removed.
                let Some(apic_dir) = self.shell.apic_dir.clone() else {
                    self.shell.status = "no project".into();
                    return;
                };
                let dir = apic_core::template::dir(&apic_dir);
                let filename = path.file_name().map(Path::new).unwrap_or(Path::new(""));
                let dest = match apic_core::file::confine_to_dir(&dir, filename) {
                    Ok(p) => p,
                    Err(e) => {
                        self.shell.status = e;
                        return;
                    }
                };
                let r = std::fs::remove_file(&dest);
                (dest, r, format!("template {name}"))
            }
        };
        match result {
            Ok(()) => {
                // Clear the editor if the deleted path was (or contained) what is open.
                if self
                    .contracts
                    .path
                    .as_deref()
                    .is_some_and(|p| p == removed_path || p.starts_with(&removed_path))
                {
                    self.contracts.model = None;
                    self.contracts.path = None;
                    self.contracts.selected = None;
                    self.contracts.selected_template = None;
                }
                self.reload_project();
                self.shell.status = format!("deleted {label}");
            }
            Err(e) => self.shell.status = format!("delete error: {e}"),
        }
    }
}
