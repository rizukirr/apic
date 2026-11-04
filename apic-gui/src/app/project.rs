//! Project-level operations: opening, creating, reloading, and importing.
//!
//! These are methods on [`App`] rather than on a feature state because each one
//! spans both `shell` and `contracts`; keeping them here is what lets the two
//! state structs stay unaware of each other.

use std::path::PathBuf;

use eframe::egui;

use crate::app::App;
use crate::app::state::{CentralView, DialogKind};
use crate::features::contracts::data::files;
use crate::features::contracts::domain::entry::{DeleteTarget, display_location};
use crate::features::contracts::domain::names::{request_target, template_file_name};
use crate::features::contracts::state::{MainTab, Repair, RespTab};
use crate::settings::Settings;

impl App {
    /// Reconciles the open contract with the freshly checked-out branch.
    /// A checkout can change or remove the tree behind every piece of
    /// derived contract state at once, and `reload_project` only rebuilds
    /// `contracts.entries`. Does not touch `shell.project_root` or persisted
    /// settings, unlike `activate_project`: a checkout changes neither.
    pub(crate) fn reconcile_after_checkout(&mut self) {
        self.reload_project();
        let path = self.contracts.path.clone();
        match path.and_then(|p| self.contracts.entries.iter().position(|e| e.path == p)) {
            Some(i) => self.load(i),
            None => {
                self.contracts.model = None;
                self.contracts.path = None;
                self.contracts.selected = None;
            }
        }
    }

    /// Discovers contracts for the active project and reads each one's method for
    /// the sidebar badge. Resolves everything against `self.shell.project_root`; never
    /// reads the process current directory.
    pub(crate) fn reload_project(&mut self) {
        let Some(root) = self.shell.project_root.clone() else {
            self.shell.apic_dir = None;
            self.shell.root = None;
            self.shell.repo_root = None;
            self.contracts.templates.clear();
            self.contracts.entries.clear();
            self.shell.status = "No project open. Use Open or New.".into();
            return;
        };

        match crate::features::git::data::cli::discover(&root) {
            Ok(repo_root) => {
                self.shell.repo_root = repo_root;
                // The Git section header's dirty indicator must be correct before the
                // Git section is ever opened, so a project load fetches status
                // itself instead of waiting for tab activation.
                if self.shell.repo_root.is_some() {
                    self.git.request_refresh();
                }
            }
            Err(err) => {
                self.shell.repo_root = None;
                self.shell.status = err;
            }
        }

        self.shell.apic_dir = Some(root.join(".apic"));
        self.contracts.templates = self
            .shell
            .apic_dir
            .as_deref()
            .map(files::templates)
            .unwrap_or_default();

        match apic_core::config::read_config_in(&root).and_then(|c| c.root_dir_in(&root)) {
            Ok(contracts_root) => {
                // `self.shell.root` is the contracts working dir consumed by import /
                // new-request / delete; keep it in sync with the active project.
                self.shell.root = Some(contracts_root.clone());
                self.contracts.entries = files::scan(&contracts_root);
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
        let buffer = files::read_text(&entry.path).unwrap_or_default();
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
        match files::load_contract(&path) {
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
        match files::load_template(&path) {
            Ok(model) => {
                self.contracts.model = Some(model);
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
    fn finish_open(&mut self, folder: PathBuf) {
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
    fn finish_new(&mut self, folder: PathBuf) {
        match apic_core::config::Config::init_in(&folder, None) {
            Ok(_) | Err(_) => self.activate_project(folder), // Err = already a project
        }
    }

    /// Spawns a native dialog on a background thread (so the portal call never
    /// freezes the UI) and records what to do with the result; polled by
    /// [`App::poll_dialog`]. A second dialog cannot start while one is pending.
    fn spawn_folder_dialog(&mut self, kind: DialogKind, title: &'static str, ctx: &egui::Context) {
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
    fn activate_project(&mut self, folder: PathBuf) {
        self.shell.project_root = Some(folder.clone());
        self.contracts.model = None;
        self.contracts.selected = None;
        self.contracts.selected_template = None;
        self.contracts.repair = None;
        self.reload_project();
        Settings::update(|s| s.last_project = Some(folder));
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
    fn finish_import_postman(&mut self, src: PathBuf) {
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
        let file_name = match template_file_name(name) {
            Ok(f) => f,
            Err(e) => {
                self.shell.status = e;
                return;
            }
        };
        let Some(apic_dir) = self.shell.apic_dir.clone() else {
            self.shell.status = "no project".into();
            return;
        };
        match files::create_template(&apic_dir, &file_name, name) {
            Ok(dest) => {
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
                    self.shell.central = CentralView::Contracts;
                }
                self.shell.status = format!("created template '{name}'");
            }
            Err(e) => self.shell.status = e,
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
        let target = match request_target(input) {
            Ok(t) => t,
            Err(e) => {
                self.shell.status = e;
                return;
            }
        };
        let Some(root) = self.shell.root.clone() else {
            self.shell.status = "no project".into();
            return;
        };
        let rel = &target.rel;
        match files::create_request(&root, &target, template.as_deref()) {
            Ok(_) if target.is_folder => {
                self.reload_project();
                self.shell.status = format!("created folder {rel}/");
            }
            Ok(dest) => {
                self.reload_project();
                if let Some(i) = self.contracts.entries.iter().position(|e| e.path == dest) {
                    self.load(i);
                    self.shell.central = CentralView::Contracts;
                }
                self.shell.status = format!("created {rel}");
            }
            Err(e) => self.shell.status = e,
        }
    }

    /// Removes the target (confined to its directory), then reloads. If the open
    /// contract/template was deleted, the editor is cleared.
    pub(crate) fn perform_delete(&mut self, target: &DeleteTarget) {
        match files::delete(
            target,
            self.shell.root.as_deref(),
            self.shell.apic_dir.as_deref(),
        ) {
            Ok((removed_path, label)) => {
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
            Err(e) => self.shell.status = e,
        }
    }
}
