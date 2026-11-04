//! Project-level operations: opening, creating, reloading, and importing.
//!
//! These are methods on [`App`] rather than on a feature state because each one
//! spans both `shell` and `contracts`; keeping them here is what lets the two
//! state structs stay unaware of each other.

use std::path::PathBuf;

use eframe::egui;
use egui::RichText;

use crate::app::App;
use crate::app::state::DialogKind;
use crate::features::contracts::ui::state::ContractsPaths;
use crate::settings::Settings;
use crate::ui::components::text_button;
use crate::ui::theme::*;

impl App {
    /// Reconciles the open contract with the freshly checked-out branch.
    /// A checkout can change or remove the tree behind every piece of
    /// derived contract state at once, and `reload_project` only rebuilds
    /// `contracts.entries`. Does not touch `shell.project_root` or persisted
    /// settings, unlike `activate_project`: a checkout changes neither.
    pub(crate) fn reconcile_after_checkout(&mut self) {
        match self.read_project() {
            Some(paths) => {
                let events = self.contracts.reconcile(&paths);
                self.apply_contracts_events(events);
            }
            None => self.contracts.close(),
        }
    }

    /// Discovers contracts for the active project and reads each one's method for
    /// the sidebar badge. Resolves everything against `self.shell.project_root`; never
    /// reads the process current directory.
    pub(crate) fn reload_project(&mut self) {
        match self.read_project() {
            Some(paths) => {
                let events = self.contracts.reload(&paths);
                self.apply_contracts_events(events);
            }
            None => self.contracts.close(),
        }
    }

    /// Discovers the repository and reads the config of the active project into
    /// `self.shell`, and returns where its contracts live. `None` when no project
    /// is open. A config that does not read leaves `contracts_root` `None` and
    /// reports the error on the status line.
    fn read_project(&mut self) -> Option<ContractsPaths> {
        let Some(root) = self.shell.project_root.clone() else {
            self.shell.apic_dir = None;
            self.shell.root = None;
            self.shell.repo_root = None;
            self.shell.status = "No project open. Use Open or New.".into();
            return None;
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

        let apic_dir = root.join(".apic");
        self.shell.apic_dir = Some(apic_dir.clone());

        match apic_core::config::read_config_in(&root).and_then(|c| c.root_dir_in(&root)) {
            Ok(contracts_root) => {
                // `self.shell.root` is the contracts working dir consumed by import /
                // new-request / delete; keep it in sync with the active project.
                self.shell.root = Some(contracts_root.clone());
                Some(ContractsPaths {
                    contracts_root: Some(contracts_root),
                    apic_dir,
                })
            }
            Err(err) => {
                self.shell.root = None;
                self.shell.status =
                    apic_core::file::home_relative(&format!("Project error: {err}"));
                Some(ContractsPaths {
                    contracts_root: None,
                    apic_dir,
                })
            }
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

    /// Modal shown when a picked non-project folder has invalid contracts: the
    /// user must fix them before it can be opened/initialized.
    pub(crate) fn open_blocked_dialog(&mut self, ctx: &egui::Context) {
        let p = palette(ctx);
        let Some(failures) = self.contracts.open_blocked.clone() else {
            return;
        };
        let mut close = false;
        egui::Window::new("Fix these contracts first")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("This folder is not an apic project and has invalid contracts. Fix them, then open it again.")
                        .color(p.on_surface),
                );
                ui.add_space(SPACE_SMALL);
                for (path, err) in &failures {
                    ui.label(RichText::new(path.to_string_lossy()).color(p.error).strong());
                    ui.label(RichText::new(err).color(p.on_surface_variant).size(11.0));
                    ui.add_space(SPACE_EXTRA_SMALL);
                }
                ui.add_space(SPACE_EXTRA_SMALL);
                if text_button(ui, "Close", p.primary) {
                    close = true;
                }
            });
        if close {
            self.contracts.open_blocked = None;
        }
    }
}
