//! Desktop GUI front-end for apic.
//!
//! A thin presentation layer over [`apic_core`]: it discovers and loads
//! contracts, displays them in a styled, panelled layout (a viewer that mirrors
//! `apic read`), and edits them through the shared [`apic_core::edit`] model.
//! The GUI owns only its widgets, theme, and layout, never the editing behavior,
//! so it cannot drift from the CLI/TUI.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use apic_core::edit::EditModel;
use apic_core::json::method_str;
use eframe::egui;
use egui::RichText;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

mod desktop;
mod settings;
mod ui;
use settings::Settings;
use ui::sections::{endpoint_info, headers, parameters, request_body, responses};
use ui::theme::*;
use ui::widgets::{bordered_input, panel, take_pending_focus};

// egui temp-data keys for the "focus the input when the dialog opens" markers,
// claimed once via `take_pending_focus` the first frame each modal renders.
const FOCUS_NEW_REQUEST: &str = "apic.focus.new_request";
const FOCUS_NEW_TEMPLATE: &str = "apic.focus.new_template";

fn main() -> eframe::Result {
    if std::env::args().skip(1).any(|a| a == "--desktop-entry") {
        match desktop::install_desktop_entry() {
            Ok(msg) => {
                println!("{msg}");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            // Stable app id => X11 WM_CLASS / Wayland app_id, which the Linux
            // .desktop entry matches via StartupWMClass so the launcher shows
            // the right name and icon for the running window. Inside a flatpak
            // the runtime sets FLATPAK_ID (io.github.rizukirr.apic); matching it
            // lets the compositor associate the window with the installed entry.
            .with_app_id(std::env::var("FLATPAK_ID").unwrap_or_else(|_| "apic-gui".to_string()))
            .with_icon(load_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "apic",
        options,
        Box::new(|cc| {
            apply_theme(&cc.egui_ctx);
            Ok(Box::new(App::new()))
        }),
    )
}

/// The window / taskbar icon, decoded from the PNG bundled with the crate.
fn load_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
        .expect("bundled icon.png is a valid PNG")
}

/// A discovered contract plus the lightweight summary shown in the sidebar.
struct Entry {
    path: PathBuf,
    rel: String,
    method: String,

    /// Validation error when this contract is invalid; `None` when it is valid.
    error: Option<String>,
}

/// In-progress raw-JSON repair of an invalid contract.
struct Repair {
    /// Index into `entries` of the file being repaired.
    index: usize,

    /// Editable raw file text.
    buffer: String,

    /// Current validation error for `buffer` (empty once valid).
    error: String,
}

/// A one-shot action requested by the header or sidebar this frame.
enum SidebarAction {
    LoadContract(usize),
    LoadTemplate(usize),
    OpenProject,
    NewProject,
    ImportPostman,
    NewTemplate,

    /// Open the new-request dialog, pre-filled with this path prefix (e.g.
    /// `auth/` when `+` is clicked on the `auth` folder, empty for the root
    /// button).
    NewRequest(String),

    /// Ask to delete something; shows a confirmation before anything is removed.
    RequestDelete(DeleteTarget),

    /// Toggle the left contracts sidebar between fully hidden and shown.
    ToggleSidebar,
}

/// A thing the user asked to delete (pending confirmation).
#[derive(Clone)]
enum DeleteTarget {
    /// A contract or folder, by path relative to the contracts root.
    Contract { rel: String, is_dir: bool },

    /// A template file in `.apic/template/`, by display name and absolute path.
    Template { name: String, path: PathBuf },
}

/// Whole-app state.
struct App {
    root: Option<PathBuf>,
    entries: Vec<Entry>,
    selected: Option<usize>,
    model: Option<EditModel>,
    path: Option<PathBuf>,
    status: String,
    editing: bool,
    search: String,
    resp_tab: usize,

    /// Shared height for the side-by-side PARAMETERS/HEADERS row (the taller of
    /// the two from the previous frame); reset on load / edit-toggle.
    row_height: f32,

    /// The `.apic` directory, for locating templates.
    apic_dir: Option<PathBuf>,

    /// Absolute root of the active project (the dir containing `.apic/`). `None`
    /// when no project is open. All discovery resolves against this, never cwd.
    project_root: Option<PathBuf>,

    /// When `Some`, a modal listing contracts that must be fixed before the
    /// picked non-project folder can be opened/initialized.
    open_blocked: Option<Vec<(PathBuf, String)>>,

    /// Raw-JSON repair editor state for an invalid contract; `None` when not
    /// repairing.
    repair: Option<Repair>,

    /// Project templates: (display name, path) from `.apic/template/`.
    templates: Vec<(String, PathBuf)>,

    /// Index into `templates` when a template is being previewed.
    selected_template: Option<usize>,

    /// When `Some`, the "new template" dialog is open with this name buffer.
    new_template: Option<String>,

    /// When `Some`, the "new request" dialog is open with this path buffer.
    new_request: Option<String>,

    /// Index into `templates` of the template to seed a new request from, used
    /// only when more than one template exists (the dialog shows a chooser).
    new_request_seed: usize,

    /// When `Some`, the delete-confirmation dialog is open for this target.
    pending_delete: Option<DeleteTarget>,

    /// In-flight native file dialog, run on a background thread so the portal
    /// call never blocks the UI, plus the action to perform on its result.
    pending_dialog: Option<(DialogKind, std::sync::mpsc::Receiver<Option<PathBuf>>)>,

    /// Whether the left contracts sidebar is shown. Toggled from the top bar;
    /// not persisted, so it always starts `true` on launch.
    sidebar_open: bool,

    /// Snapshot of `model` taken when edit mode is entered, so [ CANCEL ] can
    /// restore the pre-edit state. `None` whenever not editing.
    original_model: Option<EditModel>,
}

/// Which action consumes the path chosen by an in-flight file dialog.
#[derive(Clone, Copy)]
enum DialogKind {
    OpenProject,
    NewProject,
    ImportPostman,
}

impl App {
    fn new() -> Self {
        let mut app = App {
            root: None,
            entries: Vec::new(),
            selected: None,
            model: None,
            path: None,
            status: String::new(),
            editing: false,
            search: String::new(),
            resp_tab: 0,
            row_height: 0.0,
            apic_dir: None,
            project_root: None,
            open_blocked: None,
            repair: None,
            templates: Vec::new(),
            selected_template: None,
            new_template: None,
            new_request: None,
            new_request_seed: 0,
            pending_delete: None,
            pending_dialog: None,
            sidebar_open: true,
            original_model: None,
        };
        let settings = Settings::load();
        if let Some(root) = settings.last_project
            && root.is_dir()
        {
            app.project_root = Some(root);
        }
        app.reload_project();
        if let Ok(sub) = std::env::var("APIC_AUTOEDIT")
            && let Some(i) = app
                .entries
                .iter()
                .position(|e| e.error.is_none() && e.rel.contains(&sub))
        {
            app.load(i);
            app.begin_edit();
        }
        app
    }

    /// Discovers contracts for the active project and reads each one's method for
    /// the sidebar badge. Resolves everything against `self.project_root`; never
    /// reads the process current directory.
    fn reload_project(&mut self) {
        let Some(root) = self.project_root.clone() else {
            self.apic_dir = None;
            self.root = None;
            self.templates.clear();
            self.entries.clear();
            self.status = "No project open. Use [ Open ] or [ New ].".into();
            return;
        };

        self.apic_dir = Some(root.join(".apic"));
        self.templates = self
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
                // `self.root` is the contracts working dir consumed by import /
                // new-request / delete; keep it in sync with the active project.
                self.root = Some(contracts_root.clone());
                let failures = apic_core::validate_dir(&contracts_root);
                let mut paths =
                    apic_core::json::scan_json_file(&contracts_root, true).unwrap_or_default();
                paths.sort();
                self.entries = paths
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
                self.status = display_location(&contracts_root);
            }
            Err(err) => {
                self.root = None;
                self.entries.clear();
                self.status = apic_core::file::home_relative(&format!("Project error: {err}"));
            }
        }
    }

    /// Enter edit mode, snapshotting the current model so the edits can be
    /// discarded on cancel.
    fn begin_edit(&mut self) {
        self.original_model = self.model.clone();
        self.editing = true;
        self.row_height = 0.0; // recompute equal-height row for the new mode
    }

    /// Leave edit mode, restoring the pre-edit snapshot and discarding any edits
    /// made since [ EDIT ] was pressed.
    fn cancel_edit(&mut self) {
        if let Some(original) = self.original_model.take() {
            self.model = Some(original);
        }
        self.editing = false;
        self.row_height = 0.0; // recompute equal-height row for the new mode
    }

    /// Loads an invalid contract's raw text into the repair editor.
    fn enter_repair(&mut self, i: usize) {
        let Some(entry) = self.entries.get(i) else {
            return;
        };
        let buffer = apic_core::file::read_file(&entry.path).unwrap_or_default();
        let error = entry.error.clone().unwrap_or_default();
        self.model = None;
        self.original_model = None;
        self.selected = Some(i);
        self.selected_template = None;
        self.repair = Some(Repair {
            index: i,
            buffer,
            error,
        });
    }

    /// Loads entry `i` into the editable model.
    fn load(&mut self, i: usize) {
        let Some(entry) = self.entries.get(i) else {
            return;
        };
        let path = entry.path.clone();
        let loaded = apic_core::file::read_file(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| apic_core::json::json_get(&t, None).map_err(|e| e.to_string()))
            .map(EditModel::from_contract);
        match loaded {
            Ok(model) => {
                self.model = Some(model);
                self.path = Some(path);
                self.selected = Some(i);
                self.selected_template = None;
                self.resp_tab = 0;
                self.editing = false;
                self.original_model = None;
                self.row_height = 0.0;
                self.status = self
                    .path
                    .as_deref()
                    .map(display_location)
                    .unwrap_or_default();
            }
            Err(err) => self.status = format!("load error: {err}"),
        }
    }

    /// Loads template `i` into the editor, resolved against the built-in default
    /// into a full contract. Editable and savable like any contract: `path` keeps
    /// the template file so Save writes the edited contract straight back to it.
    /// (Saving a resolved template makes it a full template, every field it then
    /// contains is enforced when creating contracts from it.)
    fn load_template(&mut self, i: usize) {
        let Some((name, path)) = self.templates.get(i).cloned() else {
            return;
        };
        match apic_core::template::resolve_contract_from(&path)
            .and_then(|(c, _w)| apic_core::json::json_get(&c, None).map_err(|e| e.to_string()))
        {
            Ok(contract) => {
                self.model = Some(EditModel::from_contract(contract));
                self.path = Some(path);
                self.selected = None;
                self.selected_template = Some(i);
                self.resp_tab = 0;
                self.editing = false;
                self.original_model = None;
                self.row_height = 0.0;
                self.status = format!("template '{name}'");
            }
            Err(err) => self.status = format!("template error: {err}"),
        }
    }

    /// `[ Open ]`: launch the folder picker; `finish_open` runs on the result.
    fn open_project(&mut self, ctx: &egui::Context) {
        self.spawn_folder_dialog(DialogKind::OpenProject, "Open apic project", ctx);
    }

    /// `[ New ]`: launch the folder picker; `finish_new` runs on the result.
    fn new_project(&mut self, ctx: &egui::Context) {
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
                Err(e) => self.status = format!("init error: {e}"),
            }
        } else {
            self.open_blocked = Some(failures);
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
        self.status = "Waiting for the file dialog…".into();
    }

    /// Polls the in-flight dialog and runs its action once a path is chosen (or
    /// clears it on cancel). Called every frame from `update`.
    fn poll_dialog(&mut self, ctx: &egui::Context) {
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
        self.project_root = Some(folder.clone());
        self.model = None;
        self.selected = None;
        self.selected_template = None;
        self.repair = None;
        self.reload_project();
        Settings {
            last_project: Some(folder),
        }
        .save();
    }

    /// `[ Import ]` → Postman: launch the file picker (background thread).
    fn import_postman(&mut self, ctx: &egui::Context) {
        if self.root.is_none() {
            self.status = "no project to import into".into();
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
        self.status = "Waiting for the file dialog…".into();
    }

    /// Imports a Postman collection into the project via apic-core's converter,
    /// which writes contracts confined to the working dir and never overwrites.
    fn finish_import_postman(&mut self, src: PathBuf) {
        let Some(root) = self.root.clone() else {
            self.status = "no project to import into".into();
            return;
        };
        match apic_core::convert::run(&src, &root) {
            Ok(out) => {
                self.reload_project();
                let warn = if out.warnings.is_empty() {
                    String::new()
                } else {
                    format!(", {} warning(s)", out.warnings.len())
                };
                self.status = format!("imported {} contract(s){warn}", out.written);
            }
            Err(e) => self.status = format!("import error: {e}"),
        }
    }

    /// Creates a new template `<name>.json` in `.apic/template/`, seeded from the
    /// built-in default. Safety: the path is confined to the template dir, and an
    /// existing template is never overwritten.
    fn create_template(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            self.status = "template name required".into();
            return;
        }
        let Some(apic_dir) = self.apic_dir.clone() else {
            self.status = "no project".into();
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
                self.status = e;
                return;
            }
        };
        if dest.exists() {
            self.status = format!("template '{name}' already exists");
            return;
        }
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.status = format!("create dir error: {e}");
            return;
        }
        match std::fs::write(&dest, apic_core::template::DEFAULT) {
            Ok(()) => {
                self.reload_project();
                // Open the freshly created template in the central view, the same
                // way create_request opens a new contract.
                if let Some(i) = self.templates.iter().position(|(_, p)| *p == dest) {
                    self.load_template(i);
                }
                self.status = format!("created template '{name}'");
            }
            Err(e) => self.status = format!("write error: {e}"),
        }
    }

    /// Renders the "new template" dialog when open, and applies the result.
    fn new_template_dialog(&mut self, ctx: &egui::Context) {
        if self.new_template.is_none() {
            return;
        }
        let mut create = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::new("new_template_modal"))
            .frame(egui::Frame::window(&ctx.style()).inner_margin(egui::Margin::same(16)))
            .show(ctx, |ui| {
                ui.set_min_width(320.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("NEW TEMPLATE")
                            .color(GREEN)
                            .strong()
                            .size(16.0),
                    );
                });
                ui.add_space(SPACE_SMALL);
                ui.label(RichText::new("template name").color(DIM));
                ui.add_space(SPACE_MEDIUM);
                let buf = self.new_template.as_mut().expect("dialog open");
                let resp = bordered_input(ui, buf, f32::INFINITY, "");
                // Drop the caret into the input the frame the dialog opens.
                take_pending_focus(ui, FOCUS_NEW_TEMPLATE, "open", &resp);
                // Submit on Enter, same as clicking Create.
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    create = true;
                }
                ui.add_space(SPACE_LARGE);
                ui.columns(2, |cols| {
                    cols[0].vertical_centered(|ui| {
                        if ui.button(RichText::new("Create").color(GREEN)).clicked() {
                            create = true;
                        }
                    });
                    cols[1].vertical_centered(|ui| {
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        if create {
            let name = self.new_template.take().unwrap_or_default();
            self.create_template(&name);
        } else if cancel || modal.should_close() {
            self.new_template = None;
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
    fn create_request(&mut self, input: &str, template: Option<PathBuf>) {
        let input = input.trim();
        if input.is_empty() {
            self.status = "name required".into();
            return;
        }
        let Some(root) = self.root.clone() else {
            self.status = "no project".into();
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
            self.status = "name required".into();
            return;
        }
        let dest = match apic_core::file::confine_to_dir(&root, Path::new(&rel)) {
            Ok(p) => p,
            Err(e) => {
                self.status = e;
                return;
            }
        };

        if !is_folder {
            if dest.exists() {
                self.status = format!("{rel} already exists; not overwriting");
                return;
            }
            // Seed from the chosen template (merged onto the built-in default),
            // or the built-in default itself when there is no template.
            let contract = match &template {
                Some(path) => match apic_core::template::resolve_contract_from(path) {
                    Ok((c, _warnings)) => c,
                    Err(e) => {
                        self.status = format!("template error: {e}");
                        return;
                    }
                },
                None => apic_core::template::DEFAULT.to_string(),
            };
            if let Some(parent) = dest.parent()
                && let Err(e) = std::fs::create_dir_all(parent)
            {
                self.status = format!("create dir error: {e}");
                return;
            }
            match std::fs::write(&dest, contract) {
                Ok(()) => {
                    self.reload_project();
                    if let Some(i) = self.entries.iter().position(|e| e.path == dest) {
                        self.load(i);
                    }
                    self.status = format!("created {rel}");
                }
                Err(e) => self.status = format!("write error: {e}"),
            }
        } else {
            match std::fs::create_dir_all(&dest) {
                Ok(()) => {
                    self.reload_project();
                    self.status = format!("created folder {rel}/");
                }
                Err(e) => self.status = format!("create dir error: {e}"),
            }
        }
    }

    /// Renders the "new request" dialog when open, and applies the result.
    fn new_request_dialog(&mut self, ctx: &egui::Context) {
        if self.new_request.is_none() {
            return;
        }
        let mut create = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::new("new_request_modal"))
            .frame(egui::Frame::window(&ctx.style()).inner_margin(egui::Margin::same(16)))
            .show(ctx, |ui| {
                ui.set_min_width(320.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("NEW REQUEST").color(GREEN).strong().size(16.0));
                });
                ui.add_space(SPACE_SMALL);
                ui.label(RichText::new("path under the contracts directory").color(DIM));
                ui.add_space(SPACE_MEDIUM);
                let buf = self.new_request.as_mut().expect("dialog open");
                let resp = bordered_input(ui, buf, f32::INFINITY, "");
                // Drop the caret into the input the frame the dialog opens.
                take_pending_focus(ui, FOCUS_NEW_REQUEST, "open", &resp);
                // Submit on Enter, same as clicking Create.
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    create = true;
                }
                ui.add_space(SPACE_EXTRA_SMALL);
                ui.label(
                    RichText::new("end with .json for a contract (auth/logout.json); a bare name makes a folder")
                        .color(DIM)
                        .size(10.0),
                );

                // With more than one template, let the user pick which one seeds
                // the contract. With one it is used automatically; with none the
                // built-in default is used.
                if self.templates.len() > 1 {
                    let names: Vec<String> =
                        self.templates.iter().map(|(n, _)| n.clone()).collect();
                    let current = names
                        .get(self.new_request_seed)
                        .cloned()
                        .unwrap_or_default();
                    ui.add_space(SPACE_MEDIUM);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("template").color(DIM));
                        egui::ComboBox::from_id_salt("new_request_template")
                            .selected_text(RichText::new(current).color(GREEN))
                            .show_ui(ui, |ui| {
                                for (i, name) in names.iter().enumerate() {
                                    ui.selectable_value(&mut self.new_request_seed, i, name);
                                }
                            });
                    });
                }

                ui.add_space(SPACE_LARGE);
                ui.columns(2, |cols| {
                    cols[0].vertical_centered(|ui| {
                        if ui.button(RichText::new("Create").color(GREEN)).clicked() {
                            create = true;
                        }
                    });
                    cols[1].vertical_centered(|ui| {
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        if create {
            let path = self.new_request.take().unwrap_or_default();
            // Choose the seeding template: none -> built-in default; one -> it;
            // many -> the user's pick.
            let template = match self.templates.len() {
                0 => None,
                1 => Some(self.templates[0].1.clone()),
                _ => self
                    .templates
                    .get(self.new_request_seed)
                    .map(|(_, p)| p.clone()),
            };
            self.create_request(&path, template);
        } else if cancel || modal.should_close() {
            self.new_request = None;
        }
    }

    /// Modal shown when a picked non-project folder has invalid contracts: the
    /// user must fix them before it can be opened/initialized.
    fn open_blocked_dialog(&mut self, ctx: &egui::Context) {
        let Some(failures) = self.open_blocked.clone() else {
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
                        .color(TEXT),
                );
                ui.add_space(SPACE_SMALL);
                for (path, err) in &failures {
                    ui.label(RichText::new(path.to_string_lossy()).color(RED).strong());
                    ui.label(RichText::new(err).color(DIM).size(11.0));
                    ui.add_space(SPACE_EXTRA_SMALL);
                }
                ui.add_space(SPACE_EXTRA_SMALL);
                if ui.button(RichText::new("[ Close ]").color(GREEN)).clicked() {
                    close = true;
                }
            });
        if close {
            self.open_blocked = None;
        }
    }

    /// Renders the delete-confirmation dialog when a delete is pending, and
    /// performs the deletion on confirm.
    fn delete_dialog(&mut self, ctx: &egui::Context) {
        let Some(target) = self.pending_delete.clone() else {
            return;
        };
        let (what, name, folder_warn) = match &target {
            DeleteTarget::Contract { rel, is_dir } => (
                if *is_dir { "folder" } else { "contract" },
                rel.clone(),
                *is_dir,
            ),
            DeleteTarget::Template { name, .. } => ("template", name.clone(), false),
        };
        let mut confirm = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::new("delete_modal"))
            .frame(egui::Frame::window(&ctx.style()).inner_margin(egui::Margin::same(16)))
            .show(ctx, |ui| {
                ui.set_min_width(320.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("DELETE").color(RED).strong().size(16.0));
                });
                ui.add_space(SPACE_MEDIUM);
                ui.label(RichText::new(format!("Delete {what}")).color(DIM));
                ui.label(RichText::new(&name).color(TEXT).strong());
                if folder_warn {
                    ui.add_space(SPACE_EXTRA_SMALL);
                    ui.label(
                        RichText::new("this also deletes every contract inside it")
                            .color(RED)
                            .size(10.0),
                    );
                }
                ui.add_space(SPACE_LARGE);
                ui.columns(2, |cols| {
                    cols[0].vertical_centered(|ui| {
                        if ui.button(RichText::new("Delete").color(RED)).clicked() {
                            confirm = true;
                        }
                    });
                    cols[1].vertical_centered(|ui| {
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        if confirm {
            self.pending_delete = None;
            self.perform_delete(&target);
        } else if cancel || modal.should_close() {
            self.pending_delete = None;
        }
    }

    /// Removes the target (confined to its directory), then reloads. If the open
    /// contract/template was deleted, the editor is cleared.
    fn perform_delete(&mut self, target: &DeleteTarget) {
        let (removed_path, result, label) = match target {
            DeleteTarget::Contract { rel, is_dir } => {
                let Some(root) = self.root.clone() else {
                    self.status = "no project".into();
                    return;
                };
                let dest = match apic_core::file::confine_to_dir(&root, Path::new(rel)) {
                    Ok(p) => p,
                    Err(e) => {
                        self.status = e;
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
                let Some(apic_dir) = self.apic_dir.clone() else {
                    self.status = "no project".into();
                    return;
                };
                let dir = apic_core::template::dir(&apic_dir);
                let filename = path.file_name().map(Path::new).unwrap_or(Path::new(""));
                let dest = match apic_core::file::confine_to_dir(&dir, filename) {
                    Ok(p) => p,
                    Err(e) => {
                        self.status = e;
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
                    .path
                    .as_deref()
                    .is_some_and(|p| p == removed_path || p.starts_with(&removed_path))
                {
                    self.model = None;
                    self.path = None;
                    self.selected = None;
                    self.selected_template = None;
                }
                self.reload_project();
                self.status = format!("deleted {label}");
            }
            Err(e) => self.status = format!("delete error: {e}"),
        }
    }
}

/// Renders `path` with the home directory collapsed to `~` (forward-slashed),
/// reusing `apic_core::file::{to_slash, home_relative}` so the footer matches the
/// CLI and no logic is duplicated.
fn display_location(path: &Path) -> String {
    apic_core::file::home_relative(&apic_core::file::to_slash(path))
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_dialog(ctx);
        let top = self.top_bar(ctx);
        self.bottom_bar(ctx);
        let side = self.sidebar(ctx);
        match top.or(side) {
            Some(SidebarAction::LoadContract(i)) => {
                let invalid = self
                    .entries
                    .get(i)
                    .map(|e| e.error.is_some())
                    .unwrap_or(false);
                if invalid {
                    self.enter_repair(i);
                } else {
                    self.repair = None;
                    self.load(i);
                }
            }
            Some(SidebarAction::LoadTemplate(i)) => self.load_template(i),
            Some(SidebarAction::OpenProject) => self.open_project(ctx),
            Some(SidebarAction::NewProject) => self.new_project(ctx),
            Some(SidebarAction::ImportPostman) => self.import_postman(ctx),
            Some(SidebarAction::NewTemplate) => {
                self.new_template = Some(String::new());
                ctx.data_mut(|d| {
                    d.insert_temp(egui::Id::new(FOCUS_NEW_TEMPLATE), "open".to_string())
                });
            }
            Some(SidebarAction::NewRequest(prefix)) => {
                self.new_request = Some(prefix);
                self.new_request_seed = 0;
                ctx.data_mut(|d| {
                    d.insert_temp(egui::Id::new(FOCUS_NEW_REQUEST), "open".to_string())
                });
            }
            Some(SidebarAction::RequestDelete(target)) => {
                self.pending_delete = Some(target);
            }
            Some(SidebarAction::ToggleSidebar) => {
                self.sidebar_open = !self.sidebar_open;
            }
            None => {}
        }
        self.central(ctx);
        self.new_template_dialog(ctx);
        self.new_request_dialog(ctx);
        self.delete_dialog(ctx);
        self.open_blocked_dialog(ctx);
    }
}

impl App {
    /// Top header: title, the Import menu, and the search box. Returns an action
    /// when Import is chosen.
    fn top_bar(&mut self, ctx: &egui::Context) -> Option<SidebarAction> {
        let mut action = None;
        egui::TopBottomPanel::top("nav").show(ctx, |ui| {
            ui.add_space(SPACE_EXTRA_SMALL);
            ui.horizontal(|ui| {
                let row_h = 26.0;
                ui.set_min_height(row_h);
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_min_height(row_h);
                    let toggle_glyph = if self.sidebar_open { "☰" } else { "◧" };
                    if ui
                        .button(RichText::new(toggle_glyph).color(GREEN))
                        .on_hover_text("Toggle sidebar")
                        .clicked()
                    {
                        action = Some(SidebarAction::ToggleSidebar);
                    }
                    ui.add_space(SPACE_MEDIUM); // left padding so the title isn't flush to the edge
                    ui.label(RichText::new("APIC").color(GREEN).strong().size(18.0));
                    ui.add_space(SPACE_MEDIUM);
                    if ui.button(RichText::new("[ Open ]").color(GREEN)).clicked() {
                        action = Some(SidebarAction::OpenProject);
                    }
                    ui.add_space(SPACE_EXTRA_SMALL);
                    if ui.button(RichText::new("[ New ]").color(GREEN)).clicked() {
                        action = Some(SidebarAction::NewProject);
                    }
                    ui.add_space(SPACE_EXTRA_SMALL);
                    ui.menu_button(RichText::new("[ Import ]").color(GREEN), |ui| {
                        if ui.button("Postman collection").clicked() {
                            action = Some(SidebarAction::ImportPostman);
                            ui.close();
                        }
                    });
                });
            });
            ui.add_space(SPACE_EXTRA_SMALL);
        });
        action
    }

    /// Bottom bar: the loaded contract's location (home-relative), nothing else.
    fn bottom_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&self.status).color(DIM));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(concat!("apic v", env!("CARGO_PKG_VERSION")))
                            .color(DIM)
                            .size(11.0),
                    );
                });
            });
        });
    }

    /// Left sidebar: a TEMPLATES section on top, then the contract picker
    /// (folder tree, method-badged, filtered by search).
    fn sidebar(&mut self, ctx: &egui::Context) -> Option<SidebarAction> {
        // When collapsed, skip building/showing the panel entirely so the
        // CentralPanel reclaims the full width.
        if !self.sidebar_open {
            return None;
        }
        let q = self.search.to_lowercase();
        let mut tree = TreeNode::default();
        for (i, e) in self.entries.iter().enumerate() {
            if q.is_empty() || e.rel.to_lowercase().contains(&q) {
                tree.insert(&e.rel, i, &e.method, e.error.is_some());
            }
        }
        let selected = self.selected;
        let sel_template = self.selected_template;
        let templates: Vec<(String, PathBuf)> = self.templates.clone();
        let mut action = None;
        let mut to_contract = None;
        egui::SidePanel::left("contracts")
            .resizable(true)
            .default_width(240.0)
            .min_width(100.0)
            .show(ctx, |ui| {
                egui::TopBottomPanel::bottom("new_request_bar")
                    .show_separator_line(false)
                    .show_inside(ui, |ui| {
                        ui.add_space(SPACE_EXTRA_SMALL);
                        let button = egui::Button::new(RichText::new("[ NEW REQUEST ]").color(BG))
                            .fill(GREEN);
                        if ui.add_sized([ui.available_width(), 26.0], button).clicked() {
                            action = Some(SidebarAction::NewRequest(String::new()));
                        }
                        ui.add_space(SPACE_EXTRA_SMALL);
                    });

                ui.add_space(SPACE_MEDIUM);
                ui.label(RichText::new("EXPLORER").color(GREEN).strong().size(16.0));

                ui.add_space(SPACE_MEDIUM);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("TEMPLATES").color(DIM).size(11.0));
                    if ui
                        .small_button(RichText::new("+").color(GREEN))
                        .on_hover_text("New template")
                        .clicked()
                    {
                        action = Some(SidebarAction::NewTemplate);
                    }
                });
                ui.separator();
                if templates.is_empty() {
                    ui.label(RichText::new("(none)").color(DIM));
                }
                for (i, (name, path)) in templates.iter().enumerate() {
                    ui.horizontal(|ui| {
                        // Reserve the trailing delete button first so the name
                        // label truncates to the space that's left instead of
                        // forcing the panel wider than its dragged width.
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .small_button(RichText::new("-").color(DIM))
                                .on_hover_text("Delete this template")
                                .clicked()
                            {
                                action =
                                    Some(SidebarAction::RequestDelete(DeleteTarget::Template {
                                        name: name.clone(),
                                        path: path.clone(),
                                    }));
                            }
                            ui.with_layout(
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    let label = egui::Button::selectable(
                                        sel_template == Some(i),
                                        RichText::new(format!("◆ {name}")).color(AMBER),
                                    )
                                    .truncate();
                                    if ui.add(label).clicked() {
                                        action = Some(SidebarAction::LoadTemplate(i));
                                    }
                                },
                            );
                        });
                    });
                }

                ui.add_space(10.0);
                ui.label(RichText::new("CONTRACTS").color(DIM).size(11.0));
                ui.separator();
                if self.sidebar_open {
                    bordered_input(ui, &mut self.search, f32::INFINITY, "SEARCH...");
                    ui.add_space(SPACE_EXTRA_SMALL);
                }

                let mut new_in = None;
                let mut delete = None;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        tree.show(ui, "", selected, &mut to_contract, &mut new_in, &mut delete);
                    });
                if let Some(prefix) = new_in {
                    action = Some(SidebarAction::NewRequest(prefix));
                }
                if let Some((rel, is_dir)) = delete {
                    action = Some(SidebarAction::RequestDelete(DeleteTarget::Contract {
                        rel,
                        is_dir,
                    }));
                }
            });
        if let Some(i) = to_contract {
            action = Some(SidebarAction::LoadContract(i));
        }
        action
    }

    /// The central viewer/editor for the loaded contract.
    fn central(&mut self, ctx: &egui::Context) {
        let no_project = self.project_root.is_none();
        let mut promote: Option<(PathBuf, String)> = None;
        let mut toggle_edit = false;
        let App {
            model,
            path,
            status,
            editing,
            resp_tab,
            row_height,
            repair,
            entries,
            original_model,
            ..
        } = self;
        egui::CentralPanel::default().show(ctx, |ui| {
            if no_project {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("No project open").color(DIM).size(16.0));
                    ui.add_space(SPACE_SMALL);
                    ui.label(
                        RichText::new(
                            "Use [ Open ] to open a project folder, or [ New ] to create one.",
                        )
                        .color(DIM),
                    );
                });
                return;
            }
            if let Some(rep) = repair.as_mut() {
                ui.add_space(SPACE_SMALL);
                if rep.error.is_empty() {
                    ui.label(
                        RichText::new("Valid — opening editor…")
                            .color(GREEN)
                            .strong(),
                    );
                } else {
                    ui.label(RichText::new("INVALID CONTRACT").color(RED).strong());
                    ui.label(RichText::new(&rep.error).color(AMBER).size(12.0));
                }
                ui.add_space(SPACE_SMALL);
                let pretty = ui.button(RichText::new("pretty").color(AMBER)).clicked();
                if pretty {
                    rep.buffer = apic_core::json::pretty_json(&rep.buffer);
                }
                ui.add_space(SPACE_SMALL);
                let resp = ui.add_sized(
                    [
                        ui.available_width(),
                        (ui.available_height() - 8.0).max(40.0),
                    ],
                    egui::TextEdit::multiline(&mut rep.buffer)
                        .code_editor()
                        .desired_width(f32::INFINITY),
                );
                if resp.changed() || pretty {
                    rep.error = match apic_core::json::validate(&rep.buffer) {
                        Ok(()) => String::new(),
                        Err(e) => e.to_string(),
                    };
                    if rep.error.is_empty()
                        && let Some(entry) = entries.get(rep.index)
                    {
                        promote = Some((entry.path.clone(), rep.buffer.clone()));
                    }
                }
                return;
            }
            let Some(model) = model.as_mut() else {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("WELCOME TO APIC").color(GREEN).size(28.0));
                    ui.label(RichText::new("Select a contract on the left.").color(DIM));
                });
                return;
            };

            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
                    if ui.button(RichText::new("[ SAVE ]").color(GREEN)).clicked() {
                        match path.as_deref() {
                            Some(p) => match model.save(p) {
                                Ok(()) => {
                                    *status = format!("saved {}", p.display());
                                    *editing = false; // back to read-only on success
                                    *original_model = None; // commit: drop the snapshot
                                    *row_height = 0.0; // recompute equal-height row
                                }
                                Err(e) => *status = format!("save error: {e}"),
                            },
                            None => *status = "no path to save to".into(),
                        }
                    }
                    let edit_label = if *editing { "[ CANCEL ]" } else { "[ EDIT ]" };
                    if ui.button(RichText::new(edit_label).color(GREEN)).clicked() {
                        // Applied after the panel closure via begin_edit/cancel_edit
                        // so the snapshot is taken/restored on `self`.
                        toggle_edit = true;
                    }
                });
            });
            ui.add_space(SPACE_MEDIUM);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Frame::NONE
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = SPACE_LARGE;
                            endpoint_info(ui, model, *editing);
                            let target = *row_height;
                            let mut measured = 0.0_f32;
                            ui.columns(2, |cols| {
                                let h0 = panel(&mut cols[0], "PARAMETERS", target, |ui| {
                                    parameters(ui, model, *editing)
                                });
                                let h1 = panel(&mut cols[1], "HEADERS", target, |ui| {
                                    headers(ui, model, *editing)
                                });
                                measured = h0.max(h1);
                            });
                            *row_height = measured;
                            request_body(ui, model, *editing);
                            responses(ui, model, resp_tab, *editing);
                        });
                });
        });
        if toggle_edit {
            if self.editing {
                self.cancel_edit();
            } else {
                self.begin_edit();
            }
        }
        if let Some((path, buffer)) = promote
            && std::fs::write(&path, &buffer).is_ok()
        {
            self.repair = None;
            self.reload_project();
            if let Some(i) = self.entries.iter().position(|e| e.path == path) {
                self.load(i);
            }
        }
    }
}

/// A folder tree of contracts built from their `/`-separated relative paths.
/// Leaves carry the index into `App::entries` and the method for the badge.
#[derive(Default)]
struct TreeNode {
    dirs: BTreeMap<String, TreeNode>,
    files: Vec<(String, usize, String, bool)>, // (leaf label, entry index, method, invalid)
}

impl TreeNode {
    fn insert(&mut self, rel: &str, idx: usize, method: &str, invalid: bool) {
        match rel.split_once('/') {
            Some((dir, rest)) => self
                .dirs
                .entry(dir.to_string())
                .or_default()
                .insert(rest, idx, method, invalid),
            None => self
                .files
                .push((rel.to_string(), idx, method.to_string(), invalid)),
        }
    }

    /// Renders the tree. `prefix` is the path accumulated so far (for folder ids
    /// and the `+` target); `to_load` records a clicked contract; `new_in`
    /// records a folder's path (with trailing `/`) when its `+` is clicked.
    #[allow(clippy::too_many_arguments)]
    fn show(
        &self,
        ui: &mut egui::Ui,
        prefix: &str,
        selected: Option<usize>,
        to_load: &mut Option<usize>,
        new_in: &mut Option<String>,
        // (relative path, is_folder) of an item whose `x` was clicked.
        delete: &mut Option<(String, bool)>,
    ) {
        for (name, child) in &self.dirs {
            let folder_path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let id = ui.make_persistent_id(("tree", &folder_path));
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
                .show_header(ui, |ui| {
                    // Trailing buttons are reserved first (right-to-left) so the
                    // folder name truncates to the remaining width rather than
                    // forcing the side panel wider than its dragged width.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button(RichText::new("-").color(DIM))
                            .on_hover_text("Delete this folder")
                            .clicked()
                        {
                            *delete = Some((folder_path.clone(), true));
                        }
                        if ui
                            .small_button(RichText::new("+").color(GREEN))
                            .on_hover_text("New request in this folder")
                            .clicked()
                        {
                            *new_in = Some(format!("{folder_path}/"));
                        }
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(RichText::new(name).color(DIM)).truncate());
                        });
                    });
                })
                .body(|ui| child.show(ui, &folder_path, selected, to_load, new_in, delete));
        }
        for (label, idx, method, invalid) in &self.files {
            let rel = if prefix.is_empty() {
                label.clone()
            } else {
                format!("{prefix}/{label}")
            };
            ui.horizontal(|ui| {
                if *invalid {
                    ui.label(RichText::new("●").color(RED))
                        .on_hover_text("Invalid contract — click to repair");
                }
                ui.label(RichText::new(method).color(method_color(method)).size(11.0));
                // Reserve the delete button on the right, then let the file name
                // truncate into whatever width is left. Without truncation a long
                // name measures wider than the panel, and egui stores that as the
                // panel width every frame — blocking resize below the longest name.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button(RichText::new("-").color(DIM))
                        .on_hover_text("Delete this contract")
                        .clicked()
                    {
                        *delete = Some((rel.clone(), false));
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let file = egui::Button::selectable(
                            selected == Some(*idx),
                            RichText::new(label).color(TEXT),
                        )
                        .truncate();
                        if ui.add(file).clicked() {
                            *to_load = Some(*idx);
                        }
                    });
                });
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal but valid contract, loaded the same way `load()` does.
    fn sample_model() -> EditModel {
        let json = r#"{
            "name": "test",
            "method": "GET",
            "url": { "protocol": "https", "host": "example.com" },
            "headers": [],
            "responses": [ { "code": 200, "description": "ok" } ]
        }"#;
        let contract = apic_core::json::json_get(json, None).expect("valid sample contract");
        EditModel::from_contract(contract)
    }

    /// On Windows we enable eframe's `wgpu` feature alongside the default
    /// `glow`, and eframe's `Renderer::default()` then resolves to `Wgpu`
    /// (see eframe `epi.rs`). This locks that wiring so a feature regression
    /// can't silently drop us back to the OpenGL backend that fails in
    /// driverless environments.
    #[cfg(windows)]
    #[test]
    fn windows_defaults_to_wgpu_renderer() {
        assert!(matches!(
            eframe::Renderer::default(),
            eframe::Renderer::Wgpu
        ));
    }

    #[test]
    fn cancel_edit_restores_pre_edit_model() {
        let mut app = App::new();
        app.model = Some(sample_model());
        let original = app.model.clone();

        app.begin_edit();
        // Simulate the reported destructive edit: clear the response code 200.
        app.model.as_mut().unwrap().responses[0].code = String::new();
        assert_ne!(app.model, original, "the edit should change the model");

        app.cancel_edit();
        assert_eq!(
            app.model, original,
            "cancel must restore the pre-edit model"
        );
        assert!(!app.editing, "cancel must exit edit mode");
        assert!(
            app.original_model.is_none(),
            "snapshot must be cleared after cancel"
        );
    }

    #[test]
    fn edit_mode_layout_settles() {
        let json = r#"{
            "name": "test",
            "method": "POST",
            "url": { "protocol": "https", "host": "example.com", "path": ["users"],
                     "query": [{"name":"page","type":"int","required":false}],
                     "variable": [{"name":"id","type":"string","required":true}] },
            "headers": [{"name":"Authorization","value":"Bearer x"}],
            "request": { "type": "json", "example": {"name":"a"}, "schema": [
                {"name":"name","type":"string","default":null,"description":"n","required":true,"properties":null},
                {"name":"meta","type":"object","default":null,"description":"m","required":false,"properties":[
                    {"name":"age","type":"int","default":null,"description":"a","required":false,"properties":null}
                ]}
            ] },
            "responses": [ { "code": 200, "description": "ok", "example": {"id":"x"},
                "schema": [{"name":"id","type":"string","default":null,"description":"i","required":true,"properties":null}] } ]
        }"#;
        let contract = apic_core::json::json_get(json, None).expect("valid contract");
        let mut app = App::new();
        app.project_root = Some(std::path::PathBuf::from("/tmp"));
        app.model = Some(EditModel::from_contract(contract));
        app.begin_edit();

        let ctx = egui::Context::default();

        let run_at = |app: &mut App, w: f32, h: f32, frames: usize| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(w, h),
                )),
                ..Default::default()
            };
            let mut delays = Vec::new();
            for _ in 0..frames {
                let out = ctx.run(input.clone(), |ctx| {
                    app.top_bar(ctx);
                    app.bottom_bar(ctx);
                    app.sidebar(ctx);
                    app.central(ctx);
                });
                delays.push(
                    out.viewport_output
                        .get(&egui::ViewportId::ROOT)
                        .map(|v| v.repaint_delay)
                        .unwrap_or(std::time::Duration::MAX),
                );
            }
            delays
        };

        // Across a range of window sizes (a too-narrow window is the prime
        // suspect for a layout that overflows and oscillates), egui must stop
        // demanding an immediate (ZERO-delay) repaint once the row-height
        // feedback converges. Perpetual ZERO is the 100%-CPU "not responding"
        // spin.
        for (w, h) in [(1280.0, 800.0), (900.0, 700.0), (640.0, 600.0)] {
            let delays = run_at(&mut app, w, h, 16);
            let tail_zero = delays[8..].iter().all(|d| *d == std::time::Duration::ZERO);
            assert!(!tail_zero, "layout never settles at {w}x{h}: {delays:?}");
        }

        // Simulate the new-row focus feature firing: add a query row and mark it
        // for focus exactly as the `+ query` button does. A freshly focused
        // TextEdit must not pin egui into a permanent repaint.
        {
            let m = app.model.as_mut().unwrap();
            let new_idx = m.url.query.len();
            apic_core::edit::apply(
                m,
                &apic_core::edit::EditAction::Add {
                    field: apic_core::edit::Field::QueryAdd,
                },
            );
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("apic.focus.query"), new_idx.to_string()));
        }
        let delays = run_at(&mut app, 1280.0, 800.0, 16);
        let tail_zero = delays[8..].iter().all(|d| *d == std::time::Duration::ZERO);
        assert!(!tail_zero, "focus feature pins repaint: {delays:?}");
    }
}
