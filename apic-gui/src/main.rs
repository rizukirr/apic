//! Desktop GUI front-end for apic.
//!
//! A thin presentation layer over [`apic_core`]: it discovers and loads
//! contracts, edits them through the shared [`apic_core::edit`] model, and saves
//! them, exactly the same domain logic the CLI/TUI use. The GUI owns only its
//! widgets and layout, never the editing behavior, so it cannot drift from the
//! other front-ends.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use apic_core::edit::{EditAction, EditModel, Field, apply};
use apic_core::json::method_str;
use eframe::egui;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions::default();
    eframe::run_native("apic", options, Box::new(|_cc| Ok(Box::new(App::new()))))
}

/// Whole-app state: the discovered contracts and the one under edit.
struct App {
    /// Contracts working directory, `None` when not inside an apic project.
    root: Option<PathBuf>,
    /// Absolute paths of every discovered contract.
    contracts: Vec<PathBuf>,
    /// Index into `contracts` of the loaded contract.
    selected: Option<usize>,
    /// The contract under edit, lifted into the shared editable model.
    model: Option<EditModel>,
    /// Path the loaded model came from / saves back to.
    path: Option<PathBuf>,
    /// Status / error line shown at the bottom.
    status: String,
}

impl App {
    fn new() -> Self {
        let mut app = App {
            root: None,
            contracts: Vec::new(),
            selected: None,
            model: None,
            path: None,
            status: String::new(),
        };
        app.reload_project();
        app
    }

    /// Discovers the project root and its contracts via apic-core.
    fn reload_project(&mut self) {
        match apic_core::config::read_config_file().and_then(|c| c.get_root_dir()) {
            Ok(root) => {
                self.contracts = apic_core::json::scan_json_file(&root, true).unwrap_or_default();
                self.contracts.sort();
                self.status = format!(
                    "{} contract(s) under {}",
                    self.contracts.len(),
                    root.display()
                );
                self.root = Some(root);
            }
            Err(err) => {
                self.status = format!("No apic project: {err}");
            }
        }
    }

    /// Loads contract `i` into the editable model.
    fn load(&mut self, i: usize) {
        let Some(path) = self.contracts.get(i).cloned() else {
            return;
        };
        let loaded = apic_core::file::read_file(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| apic_core::json::json_get(&text, None).map_err(|e| e.to_string()))
            .map(EditModel::from_contract);
        match loaded {
            Ok(model) => {
                self.model = Some(model);
                self.path = Some(path);
                self.selected = Some(i);
                self.status = "loaded".to_string();
            }
            Err(err) => {
                self.status = format!("load error: {err}");
            }
        }
    }

    /// Path of contract `i` shown relative to the project root.
    fn display_name(&self, path: &Path) -> String {
        match &self.root {
            Some(root) => path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/"),
            None => path.to_string_lossy().into_owned(),
        }
    }
}

/// A folder tree of contracts built from their `/`-separated relative paths, so
/// the sidebar reads like a file picker. Leaves carry the index into
/// `App::contracts` so a click can load the right contract.
#[derive(Default)]
struct TreeNode {
    dirs: BTreeMap<String, TreeNode>,
    files: Vec<(String, usize)>,
}

impl TreeNode {
    /// Inserts a contract at `rel` (e.g. `auth/login`) carrying its `idx`.
    fn insert(&mut self, rel: &str, idx: usize) {
        match rel.split_once('/') {
            Some((dir, rest)) => self
                .dirs
                .entry(dir.to_string())
                .or_default()
                .insert(rest, idx),
            None => self.files.push((rel.to_string(), idx)),
        }
    }

    /// Renders folders as collapsing headers and contracts as selectable leaves;
    /// a clicked leaf records its index in `to_load`.
    fn show(&self, ui: &mut egui::Ui, selected: Option<usize>, to_load: &mut Option<usize>) {
        for (name, child) in &self.dirs {
            egui::CollapsingHeader::new(name)
                .default_open(true)
                .show(ui, |ui| child.show(ui, selected, to_load));
        }
        for (label, idx) in &self.files {
            if ui.selectable_label(selected == Some(*idx), label).clicked() {
                *to_load = Some(*idx);
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Left: contract picker as a folder tree built from the contracts'
        // relative paths. Reads of self are precomputed and the load is deferred
        // so we never alias a &mut self across the egui closure.
        let mut tree = TreeNode::default();
        for (i, path) in self.contracts.iter().enumerate() {
            tree.insert(&self.display_name(path), i);
        }
        let selected = self.selected;
        let mut to_load = None;
        egui::SidePanel::left("contracts")
            .resizable(true)
            .default_width(240.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Contracts");
                    if ui.button("Reload").clicked() {
                        to_load = Some(usize::MAX); // sentinel: reload project
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    tree.show(ui, selected, &mut to_load);
                });
            });
        match to_load {
            Some(usize::MAX) => self.reload_project(),
            Some(i) => self.load(i),
            None => {}
        }

        // Bottom: status line.
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.label(&self.status);
        });

        // Center: the editor. Disjoint field borrows so we can edit the model and
        // update status without aliasing self.
        let App {
            model,
            path,
            status,
            ..
        } = self;
        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(model) = model.as_mut() else {
                ui.heading("apic");
                ui.label("Select a contract on the left to edit it.");
                return;
            };
            editor(ui, model, path.as_deref(), status);
        });
    }
}

/// Renders the editor for `model`. Every discrete mutation goes through
/// `apic_core::edit::apply`, the same path the TUI uses, so behavior is shared.
fn editor(ui: &mut egui::Ui, model: &mut EditModel, path: Option<&Path>, status: &mut String) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        // Name / description: a bound TextEdit is the GUI equivalent of an
        // EditAction::SetText on Field::Name / Field::Description.
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut model.name);
        });
        ui.horizontal(|ui| {
            ui.label("Description");
            ui.text_edit_singleline(&mut model.description);
        });

        // Method: a discrete edit, routed through the shared action layer.
        ui.horizontal(|ui| {
            ui.label("Method");
            if ui.button(method_str(&model.method)).clicked() {
                apply(model, &EditAction::CycleMethod { forward: true });
            }
        });

        ui.separator();
        ui.label("URL");
        ui.horizontal(|ui| {
            ui.label("protocol");
            ui.text_edit_singleline(&mut model.url.protocol);
            ui.label("host");
            ui.text_edit_singleline(&mut model.url.host);
        });

        ui.separator();
        ui.heading("Headers");
        let mut delete: Option<Field> = None;
        for i in 0..model.headers.len() {
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut model.headers[i].name);
                ui.text_edit_singleline(&mut model.headers[i].value);
                if ui.button("delete").clicked() {
                    delete = Some(Field::HeaderName(i));
                }
            });
        }
        if let Some(field) = delete {
            apply(model, &EditAction::Delete { field });
        }
        if ui.button("+ add header").clicked() {
            apply(
                model,
                &EditAction::Add {
                    field: Field::HeaderAdd,
                },
            );
        }

        ui.separator();
        // Request/response editing is intentionally not built out in this
        // scaffold; the shared model already carries them, so a GUI table can be
        // added later the same way headers are handled above.
        let reqs = if model.request.is_some() { 1 } else { 0 };
        ui.label(format!(
            "request bodies: {reqs} · responses: {} (editing coming soon)",
            model.responses.len()
        ));

        ui.separator();
        if ui.button("Save").clicked() {
            match path {
                Some(path) => match model.save(path) {
                    Ok(()) => *status = format!("saved {}", path.display()),
                    Err(err) => *status = format!("save error: {err}"),
                },
                None => *status = "no path to save to".to_string(),
            }
        }
    });
}
