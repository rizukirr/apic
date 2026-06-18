//! Desktop GUI front-end for apic.
//!
//! A thin presentation layer over [`apic_core`]: it discovers and loads
//! contracts, displays them in a styled, panelled layout (a viewer that mirrors
//! `apic read`), and edits them through the shared [`apic_core::edit`] model.
//! The GUI owns only its widgets, theme, and layout, never the editing behavior,
//! so it cannot drift from the CLI/TUI.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use apic_core::edit::{BodyLoc, EditAction, EditModel, EditSchema, Field, apply};
use apic_core::json::method_str;
use eframe::egui;
use egui::{Color32, RichText, Stroke};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// Terminal/cyberpunk palette.
const BG: Color32 = Color32::from_rgb(8, 12, 10);
const PANEL_BG: Color32 = Color32::from_rgb(12, 17, 14);
const BORDER: Color32 = Color32::from_rgb(30, 64, 46);
const GREEN: Color32 = Color32::from_rgb(0, 230, 118);
const CYAN: Color32 = Color32::from_rgb(86, 197, 255);
const DIM: Color32 = Color32::from_rgb(110, 140, 122);
const TEXT: Color32 = Color32::from_rgb(190, 225, 205);
const RED: Color32 = Color32::from_rgb(255, 86, 86);
const AMBER: Color32 = Color32::from_rgb(255, 196, 0);

fn main() -> eframe::Result {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "apic",
        options,
        Box::new(|cc| {
            apply_theme(&cc.egui_ctx);
            Ok(Box::new(App::new()))
        }),
    )
}

/// Installs the dark, monospace, neon theme.
fn apply_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.override_text_style = Some(egui::TextStyle::Monospace);
    let v = &mut style.visuals;
    v.dark_mode = true;
    v.panel_fill = BG;
    v.window_fill = BG;
    v.extreme_bg_color = Color32::from_rgb(4, 6, 5);
    v.faint_bg_color = PANEL_BG;
    v.override_text_color = Some(TEXT);
    v.hyperlink_color = CYAN;
    v.selection.bg_fill = Color32::from_rgb(0, 80, 45);
    v.selection.stroke = Stroke::new(1.0, GREEN);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.inactive.bg_fill = PANEL_BG;
    v.widgets.inactive.weak_bg_fill = PANEL_BG;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, GREEN);
    v.widgets.active.bg_stroke = Stroke::new(1.0, GREEN);
    ctx.set_style(style);
}

/// A discovered contract plus the lightweight summary shown in the sidebar.
struct Entry {
    path: PathBuf,
    rel: String,
    method: String,
}

/// A one-shot action requested by the header or sidebar this frame.
enum SidebarAction {
    LoadContract(usize),
    LoadTemplate(usize),
    ImportApic,
    ImportPostman,
    NewTemplate,
    /// Open the new-request dialog, pre-filled with this path prefix (e.g.
    /// `auth/` when `+` is clicked on the `auth` folder, empty for the root
    /// button).
    NewRequest(String),
    /// Ask to delete something; shows a confirmation before anything is removed.
    RequestDelete(DeleteTarget),
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
            templates: Vec::new(),
            selected_template: None,
            new_template: None,
            new_request: None,
            new_request_seed: 0,
            pending_delete: None,
        };
        app.reload_project();
        app
    }

    /// Discovers contracts and reads each one's method for the sidebar badge.
    fn reload_project(&mut self) {
        // Templates live in `.apic/template/`, independent of the contracts root.
        self.apic_dir = apic_core::config::find_apic_dir();
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
        match apic_core::config::read_config_file().and_then(|c| c.get_root_dir()) {
            Ok(root) => {
                let mut paths = apic_core::json::scan_json_file(&root, true).unwrap_or_default();
                paths.sort();
                self.entries = paths
                    .into_iter()
                    .map(|path| {
                        let rel = apic_core::file::relative_slash(&path, &root);
                        let method = apic_core::file::read_file(&path)
                            .ok()
                            .and_then(|t| apic_core::json::json_get(&t, None).ok())
                            .map(|c| method_str(&c.method))
                            .unwrap_or_else(|| "?".to_string());
                        Entry { path, rel, method }
                    })
                    .collect();
                // Footer baseline: the project root, home-relative (never the
                // absolute path). Replaced by a contract's location once opened.
                self.status = display_location(&root);
                self.root = Some(root);
            }
            Err(err) => {
                self.status = apic_core::file::home_relative(&format!("No apic project: {err}"))
            }
        }
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

    /// Previews template `i` by resolving it against the built-in default into a
    /// full contract. Read-only: `path` is cleared so Save cannot overwrite the
    /// partial template file with a full contract.
    fn load_template(&mut self, i: usize) {
        let Some((name, path)) = self.templates.get(i).cloned() else {
            return;
        };
        match apic_core::template::resolve_contract_from(&path)
            .and_then(|(c, _w)| apic_core::json::json_get(&c, None).map_err(|e| e.to_string()))
        {
            Ok(contract) => {
                self.model = Some(EditModel::from_contract(contract));
                self.path = None;
                self.selected = None;
                self.selected_template = Some(i);
                self.resp_tab = 0;
                self.editing = false;
                self.row_height = 0.0;
                self.status = format!("template '{name}' (preview, read-only)");
            }
            Err(err) => self.status = format!("template error: {err}"),
        }
    }

    /// Imports an external apic contract into the project: pick a `.json`,
    /// validate it is a real contract, then copy it into the working dir.
    ///
    /// Safety: the destination is resolved with the symlink-aware
    /// [`apic_core::file::confine_to_dir`] so it cannot escape the working dir,
    /// the file is validated before anything is written, and an existing file is
    /// never overwritten.
    fn import_apic(&mut self) {
        let Some(root) = self.root.clone() else {
            self.status = "no project to import into".into();
            return;
        };
        let Some(src) = rfd::FileDialog::new()
            .add_filter("apic contract", &["json"])
            .set_title("Import apic contract")
            .pick_file()
        else {
            return; // user cancelled
        };

        let content = match apic_core::file::read_file(&src) {
            Ok(c) => c,
            Err(e) => {
                self.status = format!("read error: {e}");
                return;
            }
        };
        if let Err(e) = apic_core::json::validate(&content) {
            self.status = format!("not a valid contract: {e}");
            return;
        }

        let Some(name) = src.file_name() else {
            self.status = "source has no file name".into();
            return;
        };
        // Confine the destination to the working dir (rejects symlink escapes).
        let dest = match apic_core::file::confine_to_dir(&root, Path::new(name)) {
            Ok(p) => p,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        if dest.exists() {
            self.status = format!("{} already exists; not overwriting", name.to_string_lossy());
            return;
        }
        match std::fs::write(&dest, content) {
            Ok(()) => {
                self.reload_project();
                self.status = format!("imported {}", name.to_string_lossy());
            }
            Err(e) => self.status = format!("write error: {e}"),
        }
    }

    /// Imports a Postman collection into the project via apic-core's converter,
    /// which writes contracts confined to the working dir and never overwrites.
    fn import_postman(&mut self) {
        let Some(root) = self.root.clone() else {
            self.status = "no project to import into".into();
            return;
        };
        let Some(src) = rfd::FileDialog::new()
            .add_filter("Postman collection", &["json"])
            .set_title("Import Postman collection")
            .pick_file()
        else {
            return; // user cancelled
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
        let dest = match apic_core::file::confine_to_dir(&dir, Path::new(&format!("{name}.json"))) {
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
                ui.add_space(6.0);
                ui.label(RichText::new("template name").color(DIM));
                ui.add_space(8.0);
                let buf = self.new_template.as_mut().expect("dialog open");
                bordered_input(ui, buf, f32::INFINITY, "");
                ui.add_space(12.0);
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
    /// root. A name ending in `.json` creates a contract file seeded from
    /// `template` (or the built-in default when `None`) and opens it; any other
    /// name creates a folder.
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
        let dest = match apic_core::file::confine_to_dir(&root, Path::new(input)) {
            Ok(p) => p,
            Err(e) => {
                self.status = e;
                return;
            }
        };

        if input.ends_with(".json") {
            if dest.exists() {
                self.status = format!("{input} already exists; not overwriting");
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
                    self.status = format!("created {input}");
                }
                Err(e) => self.status = format!("write error: {e}"),
            }
        } else {
            // No `.json` extension: treat as a folder.
            match std::fs::create_dir_all(&dest) {
                Ok(()) => {
                    self.reload_project();
                    self.status = format!("created folder {input}/");
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
                ui.add_space(6.0);
                ui.label(RichText::new("path under the contracts directory").color(DIM));
                ui.add_space(8.0);
                let buf = self.new_request.as_mut().expect("dialog open");
                bordered_input(ui, buf, f32::INFINITY, "");
                ui.add_space(4.0);
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
                    ui.add_space(8.0);
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

                ui.add_space(12.0);
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
                ui.add_space(8.0);
                ui.label(RichText::new(format!("Delete {what}")).color(DIM));
                ui.label(RichText::new(&name).color(TEXT).strong());
                if folder_warn {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("this also deletes every contract inside it")
                            .color(RED)
                            .size(10.0),
                    );
                }
                ui.add_space(12.0);
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

/// Color for an HTTP method badge.
fn method_color(method: &str) -> Color32 {
    match method {
        "GET" | "HEAD" => GREEN,
        "POST" => CYAN,
        "PUT" | "PATCH" => AMBER,
        "DELETE" => RED,
        _ => DIM,
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let top = self.top_bar(ctx);
        self.bottom_bar(ctx);
        let side = self.sidebar(ctx);
        match top.or(side) {
            Some(SidebarAction::LoadContract(i)) => self.load(i),
            Some(SidebarAction::LoadTemplate(i)) => self.load_template(i),
            Some(SidebarAction::ImportApic) => self.import_apic(),
            Some(SidebarAction::ImportPostman) => self.import_postman(),
            Some(SidebarAction::NewTemplate) => self.new_template = Some(String::new()),
            Some(SidebarAction::NewRequest(prefix)) => {
                self.new_request = Some(prefix);
                self.new_request_seed = 0;
            }
            Some(SidebarAction::RequestDelete(target)) => {
                self.pending_delete = Some(target);
            }
            None => {}
        }
        self.central(ctx);
        self.new_template_dialog(ctx);
        self.new_request_dialog(ctx);
        self.delete_dialog(ctx);
    }
}

impl App {
    /// Top header: title, the Import menu, and the search box. Returns an action
    /// when Import is chosen.
    fn top_bar(&mut self, ctx: &egui::Context) -> Option<SidebarAction> {
        let mut action = None;
        egui::TopBottomPanel::top("nav").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let row_h = 26.0;
                ui.set_min_height(row_h);
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_min_height(row_h);
                    ui.label(RichText::new("APIC").color(GREEN).strong().size(18.0));
                    ui.add_space(8.0);
                    ui.menu_button(RichText::new("[ Import ]").color(GREEN), |ui| {
                        if ui.button("apic file").clicked() {
                            action = Some(SidebarAction::ImportApic);
                            ui.close();
                        }
                        if ui.button("Postman collection").clicked() {
                            action = Some(SidebarAction::ImportPostman);
                            ui.close();
                        }
                    });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.set_min_height(row_h);
                    ui.add_space(8.0);
                    bordered_input(ui, &mut self.search, 200.0, "SEARCH...");
                    ui.add_space(6.0);
                    ui.label(RichText::new("🔍").color(DIM));
                });
            });
            ui.add_space(4.0);
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
        let q = self.search.to_lowercase();
        let mut tree = TreeNode::default();
        for (i, e) in self.entries.iter().enumerate() {
            if q.is_empty() || e.rel.to_lowercase().contains(&q) {
                tree.insert(&e.rel, i, &e.method);
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
            .show(ctx, |ui| {
                // [ NEW REQUEST ] pinned to the bottom of the sidebar.
                egui::TopBottomPanel::bottom("new_request_bar")
                    .show_separator_line(false)
                    .show_inside(ui, |ui| {
                        ui.add_space(4.0);
                        let button = egui::Button::new(RichText::new("[ NEW REQUEST ]").color(BG))
                            .fill(GREEN);
                        if ui.add_sized([ui.available_width(), 26.0], button).clicked() {
                            action = Some(SidebarAction::NewRequest(String::new()));
                        }
                        ui.add_space(4.0);
                    });

                ui.add_space(6.0);
                ui.label(RichText::new("EXPLORER").color(GREEN).strong().size(16.0));

                // TEMPLATES section (on top), with a `+` to add a new template.
                ui.add_space(8.0);
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
                        if ui
                            .selectable_label(
                                sel_template == Some(i),
                                RichText::new(format!("◆ {name}")).color(AMBER),
                            )
                            .clicked()
                        {
                            action = Some(SidebarAction::LoadTemplate(i));
                        }
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
                        });
                    });
                }

                ui.add_space(10.0);
                ui.label(RichText::new("CONTRACTS").color(DIM).size(11.0));
                ui.separator();
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
        let App {
            model,
            path,
            status,
            editing,
            resp_tab,
            row_height,
            ..
        } = self;
        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(model) = model.as_mut() else {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("WELCOME TO APIC").color(GREEN).size(28.0));
                    ui.label(RichText::new("Select a contract on the left.").color(DIM));
                });
                return;
            };

            // Toolbar: Edit toggle + Save.
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
                    if ui.button(RichText::new("[ SAVE ]").color(GREEN)).clicked() {
                        match path.as_deref() {
                            Some(p) => match model.save(p) {
                                Ok(()) => *status = format!("saved {}", p.display()),
                                Err(e) => *status = format!("save error: {e}"),
                            },
                            None => *status = "no path to save to".into(),
                        }
                    }
                    let edit_label = if *editing { "[ CANCEL ]" } else { "[ EDIT ]" };
                    if ui.button(RichText::new(edit_label).color(GREEN)).clicked() {
                        *editing = !*editing;
                        *row_height = 0.0; // recompute equal-height row for the new mode
                    }
                });
            });
            ui.add_space(6.0);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    endpoint_info(ui, model, *editing);
                    ui.add_space(8.0);
                    // PARAMETERS and HEADERS sit side by side and must share a
                    // height. egui can't know the taller column until it's drawn,
                    // so force both to the previous frame's max and feed the new
                    // max forward (reset on load / edit-toggle so it can shrink).
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
                    ui.add_space(8.0);
                    request_body(ui, model, *editing);
                    ui.add_space(8.0);
                    responses(ui, model, resp_tab, *editing);
                });
        });
    }
}

/// A single-line text field with a border and consistent 8/4 padding, shared by
/// the popups and the header search box. Pass `f32::INFINITY` for `width` to
/// fill the available space, or a fixed width.
fn bordered_input(ui: &mut egui::Ui, buf: &mut String, width: f32, hint: &str) {
    egui::Frame::new()
        .stroke(Stroke::new(1.0, BORDER))
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            if width.is_finite() {
                ui.add(
                    egui::TextEdit::singleline(buf)
                        .frame(false)
                        .hint_text(hint)
                        .desired_width(width),
                );
            } else {
                ui.set_min_width(ui.available_width());
                ui.add(
                    egui::TextEdit::singleline(buf)
                        .frame(false)
                        .hint_text(hint)
                        .desired_width(f32::INFINITY),
                );
            }
        });
}

/// A labeled bordered panel, the `┌─ TITLE ─┐` box from the mockup. Pass
/// `min_height > 0.0` to force a minimum content height (used to equalize the
/// side-by-side row); returns the content height so callers can measure it.
fn panel(ui: &mut egui::Ui, title: &str, min_height: f32, add: impl FnOnce(&mut egui::Ui)) -> f32 {
    egui::Frame::group(ui.style())
        .fill(PANEL_BG)
        .stroke(Stroke::new(1.0, BORDER))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            // Fill the available width so the bordered box spans the panel/column
            // instead of shrinking to its content.
            ui.set_min_width(ui.available_width());
            if min_height > 0.0 {
                ui.set_min_height(min_height);
            }
            ui.label(RichText::new(title).color(DIM).size(11.0));
            ui.add_space(6.0);
            add(ui);
            ui.min_rect().height()
        })
        .inner
}

fn method_badge(ui: &mut egui::Ui, method: &str) {
    ui.label(
        RichText::new(format!(" {method} "))
            .color(BG)
            .background_color(method_color(method))
            .strong(),
    );
}

fn build_url(model: &EditModel) -> String {
    // Reuse the shared URL renderer so the GUI matches `apic read`/TUI exactly
    // (handles empty protocol/host/path the same way).
    apic_core::json::build_url(&model.url.protocol, &model.url.host, &model.url.path)
}

fn endpoint_info(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    panel(ui, "ENDPOINT_INFO", 0.0, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            if editing {
                if ui
                    .button(
                        RichText::new(method_str(&model.method))
                            .color(method_color(&method_str(&model.method))),
                    )
                    .clicked()
                {
                    apply(model, &EditAction::CycleMethod { forward: true });
                }
            } else {
                method_badge(ui, &method_str(&model.method));
            }
            ui.add_space(8.0);
            if editing {
                ui.add(egui::TextEdit::singleline(&mut model.url.protocol).desired_width(54.0));
                ui.label(RichText::new("://").color(DIM));
                ui.text_edit_singleline(&mut model.url.host);
            } else {
                ui.label(RichText::new(build_url(model)).color(CYAN).strong());
            }
        });
        ui.add_space(4.0);
        if editing {
            // Path segments (add / edit / delete).
            let mut actions: Vec<EditAction> = Vec::new();
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("path").color(DIM));
                let mut del = None;
                for i in 0..model.url.path.len() {
                    ui.label(RichText::new("/").color(DIM));
                    ui.add(egui::TextEdit::singleline(&mut model.url.path[i]).desired_width(80.0));
                    if ui.button(RichText::new("x").color(RED)).clicked() {
                        del = Some(i);
                    }
                }
                if ui.button(RichText::new("+ seg").color(GREEN)).clicked() {
                    actions.push(EditAction::Add {
                        field: Field::PathAdd,
                    });
                }
                if let Some(i) = del {
                    actions.push(EditAction::Delete {
                        field: Field::PathSeg(i),
                    });
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("name").color(DIM));
                ui.text_edit_singleline(&mut model.name);
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("desc").color(DIM));
                ui.text_edit_singleline(&mut model.description);
            });
            for a in &actions {
                apply(model, a);
            }
        } else {
            ui.label(RichText::new(&model.name).color(TEXT).strong());
            if !model.description.is_empty() {
                ui.label(RichText::new(&model.description).color(DIM));
            }
        }
    });
}

/// Body of the PARAMETERS panel (wrapped by `panel` at the call site).
fn parameters(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    let mut actions: Vec<EditAction> = Vec::new();

    ui.label(RichText::new("QUERY PARAMS").color(DIM).size(11.0));
    if model.url.query.is_empty() && !editing {
        ui.label(RichText::new("(none)").color(DIM));
    }
    for i in 0..model.url.query.len() {
        ui.horizontal(|ui| {
            if editing {
                let q = &mut model.url.query[i];
                ui.add(egui::TextEdit::singleline(&mut q.name).desired_width(90.0));
                ui.add(egui::TextEdit::singleline(&mut q.dtype).desired_width(56.0));
                ui.checkbox(&mut q.required, RichText::new("req").color(DIM));
                if ui.button(RichText::new("x").color(RED)).clicked() {
                    actions.push(EditAction::Delete {
                        field: Field::QueryName(i),
                    });
                }
            } else {
                let q = &model.url.query[i];
                ui.label(RichText::new(&q.name).color(TEXT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(&q.dtype).color(CYAN));
                });
            }
        });
    }
    if editing && ui.button(RichText::new("+ query").color(GREEN)).clicked() {
        actions.push(EditAction::Add {
            field: Field::QueryAdd,
        });
    }

    ui.add_space(6.0);
    ui.label(RichText::new("PATH VARIABLES").color(DIM).size(11.0));
    if model.url.variable.is_empty() && !editing {
        ui.label(RichText::new("(none)").color(DIM));
    }
    for i in 0..model.url.variable.len() {
        ui.horizontal(|ui| {
            if editing {
                let v = &mut model.url.variable[i];
                ui.add(egui::TextEdit::singleline(&mut v.name).desired_width(90.0));
                ui.add(egui::TextEdit::singleline(&mut v.dtype).desired_width(56.0));
                ui.checkbox(&mut v.required, RichText::new("req").color(DIM));
                if ui.button(RichText::new("x").color(RED)).clicked() {
                    actions.push(EditAction::Delete {
                        field: Field::VarName(i),
                    });
                }
            } else {
                let v = &model.url.variable[i];
                ui.label(RichText::new(&v.name).color(TEXT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(&v.dtype).color(CYAN));
                });
            }
        });
    }
    if editing
        && ui
            .button(RichText::new("+ variable").color(GREEN))
            .clicked()
    {
        actions.push(EditAction::Add {
            field: Field::VarAdd,
        });
    }

    for a in &actions {
        apply(model, a);
    }
}

/// Body of the HEADERS panel (wrapped by `panel` at the call site).
fn headers(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    if model.headers.is_empty() {
        ui.label(RichText::new("(none)").color(DIM));
    }
    let mut delete = None;
    for i in 0..model.headers.len() {
        ui.horizontal(|ui| {
            if editing {
                ui.text_edit_singleline(&mut model.headers[i].name);
                ui.text_edit_singleline(&mut model.headers[i].value);
                if ui.button(RichText::new("x").color(RED)).clicked() {
                    delete = Some(Field::HeaderName(i));
                }
            } else {
                ui.label(RichText::new(&model.headers[i].name).color(TEXT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(&model.headers[i].value).color(GREEN));
                });
            }
        });
    }
    if let Some(field) = delete {
        apply(model, &EditAction::Delete { field });
    }
    if editing
        && ui
            .button(RichText::new("+ add header").color(GREEN))
            .clicked()
    {
        apply(
            model,
            &EditAction::Add {
                field: Field::HeaderAdd,
            },
        );
    }
}

/// Renders schema fields as `name: type [REQUIRED]`, recursing into properties.
fn schema_fields(ui: &mut egui::Ui, fields: &[apic_core::edit::EditSchema], depth: usize) {
    for f in fields {
        ui.horizontal(|ui| {
            ui.add_space(depth as f32 * 14.0);
            ui.label(RichText::new(format!("{}:", f.name)).color(TEXT));
            ui.label(RichText::new(&f.dtype).color(CYAN));
            if f.required {
                ui.label(
                    RichText::new(" REQUIRED ")
                        .color(BG)
                        .background_color(RED)
                        .size(10.0),
                );
            } else {
                ui.label(RichText::new("[OPTIONAL]").color(DIM).size(10.0));
            }
        });
        if !f.properties.is_empty() {
            schema_fields(ui, &f.properties, depth + 1);
        }
    }
}

/// Edit-mode schema editor: binds name/type/required directly and collects
/// structural add/delete edits into `actions` (applied after the borrow ends).
/// Recurses into nested object `properties`; an object field gets a `+` to add a
/// child.
fn edit_schema_fields(
    ui: &mut egui::Ui,
    loc: &BodyLoc,
    fields: &mut [EditSchema],
    path: &mut Vec<usize>,
    actions: &mut Vec<EditAction>,
) {
    for (i, f) in fields.iter_mut().enumerate() {
        path.push(i);
        ui.horizontal(|ui| {
            ui.add_space((path.len() as f32 - 1.0) * 14.0);
            ui.add(egui::TextEdit::singleline(&mut f.name).desired_width(110.0));
            ui.add(egui::TextEdit::singleline(&mut f.dtype).desired_width(70.0));
            ui.checkbox(&mut f.required, RichText::new("req").color(DIM));
            if ui.button(RichText::new("x").color(RED)).clicked() {
                actions.push(EditAction::Delete {
                    field: Field::SchemaName(loc.clone(), path.clone()),
                });
            }
            if apic_core::json::parse_type(&f.dtype).0 == "object"
                && ui.button(RichText::new("+").color(GREEN)).clicked()
            {
                actions.push(EditAction::Add {
                    field: Field::SchemaAdd(loc.clone(), path.clone()),
                });
            }
        });
        if !f.properties.is_empty() {
            edit_schema_fields(ui, loc, &mut f.properties, path, actions);
        }
        path.pop();
    }
}

fn json_block(ui: &mut egui::Ui, raw: &str) {
    // Pretty-print via the shared core formatter (reformats whitespace only,
    // preserving numbers/key order/strings exactly).
    let mut text = if raw.trim().is_empty() {
        "(no example)".to_string()
    } else {
        apic_core::json::pretty_json(raw)
    };
    egui::Frame::new()
        .fill(Color32::from_rgb(4, 6, 5))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            // A read-only code editor preserves the indentation (a plain Label
            // collapses leading whitespace, flattening the JSON).
            ui.add(
                egui::TextEdit::multiline(&mut text)
                    .code_editor()
                    .interactive(false)
                    .frame(false)
                    .text_color(GREEN)
                    .desired_width(f32::INFINITY),
            );
        });
}

fn request_body(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    panel(ui, "REQUEST_BODY", 0.0, |ui| {
        let mut actions: Vec<EditAction> = Vec::new();
        if let Some(req) = model.request.as_mut() {
            if editing {
                ui.horizontal(|ui| {
                    if ui
                        .button(RichText::new(format!("type: {}", req.dtype)).color(CYAN))
                        .clicked()
                    {
                        actions.push(EditAction::ToggleBodyType {
                            loc: BodyLoc::Request,
                        });
                    }
                    if ui.button(RichText::new("remove body").color(RED)).clicked() {
                        actions.push(EditAction::Add {
                            field: Field::RequestToggle,
                        });
                    }
                });
            }
            ui.columns(2, |cols| {
                cols[0].label(RichText::new("SCHEMA DEFINITION").color(DIM).size(11.0));
                cols[0].add_space(4.0);
                if editing {
                    let mut path = Vec::new();
                    edit_schema_fields(
                        &mut cols[0],
                        &BodyLoc::Request,
                        &mut req.schema,
                        &mut path,
                        &mut actions,
                    );
                    if cols[0]
                        .button(RichText::new("+ field").color(GREEN))
                        .clicked()
                    {
                        actions.push(EditAction::Add {
                            field: Field::SchemaAdd(BodyLoc::Request, Vec::new()),
                        });
                    }
                } else if req.schema.is_empty() {
                    cols[0].label(RichText::new("(none)").color(DIM));
                } else {
                    schema_fields(&mut cols[0], &req.schema, 0);
                }
                cols[1].label(RichText::new("EXAMPLE JSON").color(DIM).size(11.0));
                cols[1].add_space(4.0);
                if editing {
                    if cols[1]
                        .button(RichText::new("generate from schema").color(GREEN))
                        .clicked()
                    {
                        actions.push(EditAction::GenerateExample {
                            loc: BodyLoc::Request,
                        });
                    }
                    cols[1].add(
                        egui::TextEdit::multiline(&mut req.example)
                            .code_editor()
                            .desired_rows(6),
                    );
                } else {
                    json_block(&mut cols[1], &req.example);
                }
            });
        } else {
            ui.label(RichText::new("(no request body)").color(DIM));
            if editing
                && ui
                    .button(RichText::new("+ add request body").color(GREEN))
                    .clicked()
            {
                actions.push(EditAction::Add {
                    field: Field::RequestToggle,
                });
            }
        }
        for a in &actions {
            apply(model, a);
        }
    });
}

fn responses(ui: &mut egui::Ui, model: &mut EditModel, resp_tab: &mut usize, editing: bool) {
    panel(ui, "RESPONSES", 0.0, |ui| {
        let mut actions: Vec<EditAction> = Vec::new();

        // Tabs (+ add).
        ui.horizontal_wrapped(|ui| {
            for (i, r) in model.responses.iter().enumerate() {
                let label = format!("[ {} ]", if r.code.is_empty() { "?" } else { &r.code });
                let color = if i == *resp_tab { GREEN } else { DIM };
                if ui
                    .selectable_label(i == *resp_tab, RichText::new(label).color(color))
                    .clicked()
                {
                    *resp_tab = i;
                }
            }
            if editing && ui.button(RichText::new("+ add").color(GREEN)).clicked() {
                actions.push(EditAction::Add {
                    field: Field::ResponseAdd,
                });
            }
        });

        if model.responses.is_empty() {
            ui.label(RichText::new("(no responses)").color(DIM));
            for a in &actions {
                apply(model, a);
            }
            return;
        }
        if *resp_tab >= model.responses.len() {
            *resp_tab = 0;
        }
        ui.separator();

        let idx = *resp_tab;
        let r = &mut model.responses[idx];
        if editing {
            ui.horizontal(|ui| {
                ui.label(RichText::new("code").color(DIM));
                ui.add(egui::TextEdit::singleline(&mut r.code).desired_width(60.0));
                ui.label(RichText::new("desc").color(DIM));
                ui.text_edit_singleline(&mut r.description);
            });
            ui.horizontal(|ui| {
                if ui
                    .button(RichText::new(format!("type: {}", r.dtype)).color(CYAN))
                    .clicked()
                {
                    actions.push(EditAction::ToggleBodyType {
                        loc: BodyLoc::Response(idx),
                    });
                }
                if ui
                    .button(RichText::new("delete response").color(RED))
                    .clicked()
                {
                    actions.push(EditAction::Delete {
                        field: Field::ResponseCode(idx),
                    });
                }
            });
        }
        ui.columns(2, |cols| {
            cols[0].label(RichText::new("RESPONSE SCHEMA").color(DIM).size(11.0));
            cols[0].add_space(4.0);
            if editing {
                let mut path = Vec::new();
                edit_schema_fields(
                    &mut cols[0],
                    &BodyLoc::Response(idx),
                    &mut r.schema,
                    &mut path,
                    &mut actions,
                );
                if cols[0]
                    .button(RichText::new("+ field").color(GREEN))
                    .clicked()
                {
                    actions.push(EditAction::Add {
                        field: Field::SchemaAdd(BodyLoc::Response(idx), Vec::new()),
                    });
                }
            } else if r.schema.is_empty() {
                cols[0].label(RichText::new("(none)").color(DIM));
            } else {
                schema_fields(&mut cols[0], &r.schema, 0);
            }
            cols[1].label(RichText::new("RESPONSE_PREVIEW").color(DIM).size(11.0));
            cols[1].add_space(4.0);
            if editing {
                if cols[1]
                    .button(RichText::new("generate from schema").color(GREEN))
                    .clicked()
                {
                    actions.push(EditAction::GenerateExample {
                        loc: BodyLoc::Response(idx),
                    });
                }
                cols[1].add(
                    egui::TextEdit::multiline(&mut r.example)
                        .code_editor()
                        .desired_rows(6),
                );
            } else {
                json_block(&mut cols[1], &r.example);
            }
        });

        for a in &actions {
            apply(model, a);
        }
    });
}

/// A folder tree of contracts built from their `/`-separated relative paths.
/// Leaves carry the index into `App::entries` and the method for the badge.
#[derive(Default)]
struct TreeNode {
    dirs: BTreeMap<String, TreeNode>,
    files: Vec<(String, usize, String)>, // (leaf label, entry index, method)
}

impl TreeNode {
    fn insert(&mut self, rel: &str, idx: usize, method: &str) {
        match rel.split_once('/') {
            Some((dir, rest)) => self
                .dirs
                .entry(dir.to_string())
                .or_default()
                .insert(rest, idx, method),
            None => self.files.push((rel.to_string(), idx, method.to_string())),
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
                    ui.label(RichText::new(name).color(DIM));
                    // Action buttons aligned to the end of the row.
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
                    });
                })
                .body(|ui| child.show(ui, &folder_path, selected, to_load, new_in, delete));
        }
        for (label, idx, method) in &self.files {
            let rel = if prefix.is_empty() {
                label.clone()
            } else {
                format!("{prefix}/{label}")
            };
            ui.horizontal(|ui| {
                ui.label(RichText::new(method).color(method_color(method)).size(11.0));
                if ui
                    .selectable_label(selected == Some(*idx), RichText::new(label).color(TEXT))
                    .clicked()
                {
                    *to_load = Some(*idx);
                }
                // Delete button aligned to the end of the row.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button(RichText::new("-").color(DIM))
                        .on_hover_text("Delete this contract")
                        .clicked()
                    {
                        *delete = Some((rel.clone(), false));
                    }
                });
            });
        }
    }
}
