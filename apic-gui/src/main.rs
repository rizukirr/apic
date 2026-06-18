//! Desktop GUI front-end for apic.
//!
//! A thin presentation layer over [`apic_core`]: it discovers and loads
//! contracts, displays them in a styled, panelled layout (a viewer that mirrors
//! `apic read`), and edits them through the shared [`apic_core::edit`] model.
//! The GUI owns only its widgets, theme, and layout, never the editing behavior,
//! so it cannot drift from the CLI/TUI.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use apic_core::edit::{EditAction, EditModel, Field, apply};
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
        };
        app.reload_project();
        app
    }

    /// Discovers contracts and reads each one's method for the sidebar badge.
    fn reload_project(&mut self) {
        match apic_core::config::read_config_file().and_then(|c| c.get_root_dir()) {
            Ok(root) => {
                let mut paths = apic_core::json::scan_json_file(&root, true).unwrap_or_default();
                paths.sort();
                self.entries = paths
                    .into_iter()
                    .map(|path| {
                        let rel = rel_to(&root, &path);
                        let method = apic_core::file::read_file(&path)
                            .ok()
                            .and_then(|t| apic_core::json::json_get(&t, None).ok())
                            .map(|c| method_str(&c.method))
                            .unwrap_or_else(|| "?".to_string());
                        Entry { path, rel, method }
                    })
                    .collect();
                self.status = format!(
                    "{} contract(s) under {}",
                    self.entries.len(),
                    root.display()
                );
                self.root = Some(root);
            }
            Err(err) => self.status = format!("No apic project: {err}"),
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
                self.resp_tab = 0;
                self.editing = false;
                self.status = "loaded".to_string();
            }
            Err(err) => self.status = format!("load error: {err}"),
        }
    }
}

fn rel_to(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
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
        self.top_bar(ctx);
        self.bottom_bar(ctx);
        let to_load = self.sidebar(ctx);
        match to_load {
            Some(usize::MAX) => self.reload_project(),
            Some(i) => self.load(i),
            None => {}
        }
        self.central(ctx);
    }
}

impl App {
    /// Top nav bar: title, section tabs (only ENDPOINTS is wired today), search.
    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("nav").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("API_DOC_CLI_V1.0")
                        .color(GREEN)
                        .strong()
                        .size(18.0),
                );
                ui.add_space(16.0);
                for (label, active) in [
                    ("ENDPOINTS", true),
                    ("SCHEMAS", false),
                    ("AUTH", false),
                    ("HISTORY", false),
                ] {
                    let color = if active { GREEN } else { DIM };
                    ui.label(RichText::new(label).color(color));
                    ui.add_space(8.0);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text("SEARCH...")
                            .desired_width(200.0),
                    );
                    ui.label(RichText::new("🔍").color(DIM));
                });
            });
            ui.add_space(4.0);
        });
    }

    /// Bottom status bar: key hints, validity, host.
    fn bottom_bar(&mut self, ctx: &egui::Context) {
        let valid = self.model.as_ref().map(|m| m.to_json().is_ok());
        let host = self.model.as_ref().map(|m| m.url.host.clone());
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("API_DOC_TUI (C) 2024").color(DIM));
                ui.add_space(12.0);
                ui.label(RichText::new("[Q] Quit").color(GREEN));
                ui.label(RichText::new("[S] Save").color(GREEN));
                ui.label(RichText::new("[E] Edit").color(GREEN));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    match valid {
                        Some(true) => ui.label(RichText::new("VALID").color(GREEN)),
                        Some(false) => ui.label(RichText::new("INVALID").color(RED)),
                        None => ui.label(RichText::new("NO CONTRACT").color(DIM)),
                    };
                    if let Some(host) = host {
                        ui.add_space(12.0);
                        ui.label(RichText::new(format!("HOST: {host}")).color(DIM));
                    }
                });
            });
            // Status / error line.
            ui.label(RichText::new(&self.status).color(DIM));
        });
    }

    /// Left contract picker (folder tree, method-badged, filtered by search).
    /// Returns the index to load, or `usize::MAX` to reload the project.
    fn sidebar(&mut self, ctx: &egui::Context) -> Option<usize> {
        let q = self.search.to_lowercase();
        let mut tree = TreeNode::default();
        for (i, e) in self.entries.iter().enumerate() {
            if q.is_empty() || e.rel.to_lowercase().contains(&q) {
                tree.insert(&e.rel, i, &e.method);
            }
        }
        let selected = self.selected;
        let mut to_load = None;
        egui::SidePanel::left("contracts")
            .resizable(true)
            .default_width(240.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.label(
                    RichText::new("API_EXPLORER")
                        .color(GREEN)
                        .strong()
                        .size(16.0),
                );
                ui.label(RichText::new("v2.4.0-stable").color(DIM).size(11.0));
                ui.add_space(8.0);
                if ui
                    .button(
                        RichText::new("[ NEW REQUEST ]")
                            .color(BG)
                            .background_color(GREEN),
                    )
                    .clicked()
                {
                    to_load = Some(usize::MAX); // reload for now (create is a follow-up)
                }
                ui.add_space(8.0);
                ui.label(RichText::new("CONTRACTS").color(DIM).size(11.0));
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    tree.show(ui, selected, &mut to_load);
                });
            });
        to_load
    }

    /// The central viewer/editor for the loaded contract.
    fn central(&mut self, ctx: &egui::Context) {
        let App {
            model,
            path,
            status,
            editing,
            resp_tab,
            ..
        } = self;
        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(model) = model.as_mut() else {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("API_EXPLORER").color(GREEN).size(28.0));
                    ui.label(RichText::new("Select a contract on the left.").color(DIM));
                });
                return;
            };

            // Toolbar: Edit toggle + Save.
            ui.horizontal(|ui| {
                let edit_label = if *editing {
                    "[ VIEWING: EDIT ]"
                } else {
                    "[ EDIT ]"
                };
                if ui.button(RichText::new(edit_label).color(GREEN)).clicked() {
                    *editing = !*editing;
                }
                if ui.button(RichText::new("[ SAVE ]").color(GREEN)).clicked() {
                    match path.as_deref() {
                        Some(p) => match model.save(p) {
                            Ok(()) => *status = format!("saved {}", p.display()),
                            Err(e) => *status = format!("save error: {e}"),
                        },
                        None => *status = "no path to save to".into(),
                    }
                }
            });
            ui.add_space(6.0);

            egui::ScrollArea::vertical().show(ui, |ui| {
                endpoint_info(ui, model, *editing);
                ui.add_space(8.0);
                ui.columns(2, |cols| {
                    parameters(&mut cols[0], model);
                    headers(&mut cols[1], model, *editing);
                });
                ui.add_space(8.0);
                request_body(ui, model);
                ui.add_space(8.0);
                responses(ui, model, resp_tab);
            });
        });
    }
}

/// A labeled bordered panel, the `┌─ TITLE ─┐` box from the mockup.
fn panel(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style())
        .fill(PANEL_BG)
        .stroke(Stroke::new(1.0, BORDER))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.label(RichText::new(title).color(DIM).size(11.0));
            ui.add_space(6.0);
            add(ui);
        });
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
    let u = &model.url;
    let path = u.path.join("/");
    format!("{}://{}/{}", u.protocol, u.host, path)
}

fn endpoint_info(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    panel(ui, "ENDPOINT_INFO", |ui| {
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
                ui.label(RichText::new(format!("{}://", model.url.protocol)).color(DIM));
                ui.text_edit_singleline(&mut model.url.host);
            } else {
                ui.label(RichText::new(build_url(model)).color(CYAN).strong());
            }
        });
        ui.add_space(4.0);
        if editing {
            ui.horizontal(|ui| {
                ui.label(RichText::new("name").color(DIM));
                ui.text_edit_singleline(&mut model.name);
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("desc").color(DIM));
                ui.text_edit_singleline(&mut model.description);
            });
        } else {
            ui.label(RichText::new(&model.name).color(TEXT).strong());
            if !model.description.is_empty() {
                ui.label(RichText::new(&model.description).color(DIM));
            }
        }
    });
}

fn parameters(ui: &mut egui::Ui, model: &EditModel) {
    panel(ui, "PARAMETERS", |ui| {
        ui.label(RichText::new("QUERY PARAMS").color(DIM).size(11.0));
        if model.url.query.is_empty() {
            ui.label(RichText::new("(none)").color(DIM));
        }
        for q in &model.url.query {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&q.name).color(TEXT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(&q.dtype).color(CYAN));
                });
            });
        }
        ui.add_space(6.0);
        ui.label(RichText::new("PATH VARIABLES").color(DIM).size(11.0));
        if model.url.variable.is_empty() {
            ui.label(RichText::new("(none)").color(DIM));
        }
        for v in &model.url.variable {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&v.name).color(TEXT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(&v.dtype).color(CYAN));
                });
            });
        }
    });
}

fn headers(ui: &mut egui::Ui, model: &mut EditModel, editing: bool) {
    panel(ui, "HEADERS", |ui| {
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
    });
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

fn json_block(ui: &mut egui::Ui, raw: &str) {
    let text = if raw.trim().is_empty() {
        "(no example)"
    } else {
        raw
    };
    egui::Frame::new()
        .fill(Color32::from_rgb(4, 6, 5))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(GREEN).monospace());
        });
}

fn request_body(ui: &mut egui::Ui, model: &EditModel) {
    panel(ui, "REQUEST_BODY", |ui| {
        let Some(req) = &model.request else {
            ui.label(RichText::new("(no request body)").color(DIM));
            return;
        };
        ui.columns(2, |cols| {
            cols[0].label(RichText::new("SCHEMA DEFINITION").color(DIM).size(11.0));
            cols[0].add_space(4.0);
            if req.schema.is_empty() {
                cols[0].label(RichText::new("(none)").color(DIM));
            } else {
                schema_fields(&mut cols[0], &req.schema, 0);
            }
            cols[1].label(RichText::new("EXAMPLE JSON").color(DIM).size(11.0));
            cols[1].add_space(4.0);
            json_block(&mut cols[1], &req.example);
        });
    });
}

fn responses(ui: &mut egui::Ui, model: &EditModel, resp_tab: &mut usize) {
    panel(ui, "RESPONSES", |ui| {
        if model.responses.is_empty() {
            ui.label(RichText::new("(no responses)").color(DIM));
            return;
        }
        if *resp_tab >= model.responses.len() {
            *resp_tab = 0;
        }
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
        });
        ui.separator();
        let r = &model.responses[*resp_tab];
        ui.columns(2, |cols| {
            cols[0].label(RichText::new("RESPONSE SCHEMA").color(DIM).size(11.0));
            cols[0].add_space(4.0);
            if r.schema.is_empty() {
                cols[0].label(RichText::new("(none)").color(DIM));
            } else {
                schema_fields(&mut cols[0], &r.schema, 0);
            }
            cols[1].label(RichText::new("RESPONSE_PREVIEW").color(DIM).size(11.0));
            cols[1].add_space(4.0);
            json_block(&mut cols[1], &r.example);
        });
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

    fn show(&self, ui: &mut egui::Ui, selected: Option<usize>, to_load: &mut Option<usize>) {
        for (name, child) in &self.dirs {
            egui::CollapsingHeader::new(RichText::new(name).color(DIM))
                .default_open(true)
                .show(ui, |ui| child.show(ui, selected, to_load));
        }
        for (label, idx, method) in &self.files {
            ui.horizontal(|ui| {
                ui.label(RichText::new(method).color(method_color(method)).size(11.0));
                if ui
                    .selectable_label(selected == Some(*idx), RichText::new(label).color(TEXT))
                    .clicked()
                {
                    *to_load = Some(*idx);
                }
            });
        }
    }
}
