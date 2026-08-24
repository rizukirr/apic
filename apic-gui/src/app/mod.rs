//! Application shell: the top-level state container, the eframe entry point,
//! and the orchestration that spans features.
//!
//! `App` owns one state struct per feature plus the in-flight file dialog. A
//! new feature adds a field here and a match arm in [`actions`]; it does not
//! touch any other feature's state.

pub(crate) mod actions;
pub(crate) mod git;
pub(crate) mod project;
pub(crate) mod state;

use std::path::PathBuf;

use eframe::egui;
use egui::RichText;

use crate::app::actions::{Action, GitAction, SidebarAction};
use crate::app::state::{DialogKind, ShellState, SidebarTab};
use crate::features::contracts::state::ContractsState;
use crate::features::contracts::view::{
    CentralOutcome, FOCUS_NEW_REQUEST, FOCUS_NEW_TEMPLATE, central_body, sidebar_body,
};
use crate::features::git::state::GitState;
use crate::features::git::view as git_view;
use crate::settings::Settings;
use crate::ui::components::text_button;
use crate::ui::theme::*;

/// Whole-app state.
///
/// One field per feature plus the in-flight file dialog. A new feature adds a
/// field here and a match arm in the action dispatch; it does not touch any
/// other feature's state.
pub(crate) struct App {
    /// Application chrome and the active project location, shared by every
    /// feature.
    pub(crate) shell: ShellState,

    /// Everything the contracts feature owns.
    pub(crate) contracts: ContractsState,

    /// Everything the git feature owns.
    pub(crate) git: GitState,

    /// In-flight native file dialog, run on a background thread so the portal
    /// call never blocks the UI, plus the action to perform on its result.
    pub(crate) pending_dialog: Option<(DialogKind, std::sync::mpsc::Receiver<Option<PathBuf>>)>,
}

impl App {
    pub(crate) fn new() -> Self {
        let mut app = App {
            shell: ShellState::default(),
            contracts: ContractsState::default(),
            git: GitState::default(),
            pending_dialog: None,
        };
        let settings = Settings::load();
        if let Some(root) = settings.last_project
            && root.is_dir()
        {
            app.shell.project_root = Some(root);
        }
        app.reload_project();
        if let Ok(sub) = std::env::var("APIC_AUTOEDIT")
            && let Some(i) = app
                .contracts
                .entries
                .iter()
                .position(|e| e.error.is_none() && e.rel.contains(&sub))
        {
            app.load(i);
            app.begin_edit();
        }
        app
    }

    /// Top header: title, the Import menu, and the search box. Returns an action
    /// when Import is chosen.
    fn top_bar(&mut self, ui: &mut egui::Ui) -> Option<SidebarAction> {
        let mut action = None;
        egui::Panel::top("nav").show(ui, |ui| {
            ui.add_space(SPACE_EXTRA_SMALL);
            ui.horizontal(|ui| {
                let row_h = 26.0;
                ui.set_min_height(row_h);
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_min_height(row_h);
                    let toggle_glyph = if self.shell.sidebar_open {
                        "☰"
                    } else {
                        "◧"
                    };
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
                    if text_button(ui, "Open", GREEN) {
                        action = Some(SidebarAction::OpenProject);
                    }
                    ui.add_space(2.0);
                    if text_button(ui, "New", GREEN) {
                        action = Some(SidebarAction::NewProject);
                    }
                    ui.add_space(2.0);
                    ui.menu_button(RichText::new("Import").color(GREEN), |ui| {
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
    fn bottom_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&self.shell.status).color(DIM));
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

    /// The left sidebar frame. The panel belongs to the shell rather than to any
    /// one feature, so a tab row picks the active feature and only its body
    /// fills the frame.
    fn sidebar(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        // When collapsed, skip building/showing the panel entirely so the
        // CentralPanel reclaims the full width.
        if !self.shell.sidebar_open {
            return None;
        }
        let mut action = None;
        egui::Panel::left("sidebar")
            .resizable(true)
            .default_size(240.0)
            .min_size(100.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let mut tab = |ui: &mut egui::Ui, label: &str, which: SidebarTab| {
                        let selected = self.shell.sidebar_tab == which;
                        if ui
                            .selectable_label(
                                selected,
                                RichText::new(label).color(if selected { GREEN } else { DIM }),
                            )
                            .clicked()
                            && !selected
                        {
                            action = Some(Action::Sidebar(SidebarAction::SwitchTab(which)));
                        }
                    };
                    tab(ui, "Explorer", SidebarTab::Explorer);
                    tab(ui, "Git", SidebarTab::Git);
                });
                ui.separator();
                match self.shell.sidebar_tab {
                    SidebarTab::Explorer => {
                        if let Some(a) = sidebar_body(ui, &mut self.contracts) {
                            action = Some(Action::Sidebar(a));
                        }
                    }
                    SidebarTab::Git => {
                        if let Some(a) = git_view::sidebar_body(
                            ui,
                            &mut self.git,
                            self.shell.repo_root.as_deref(),
                        ) {
                            action = Some(Action::Git(a));
                        }
                    }
                }
            });
        action
    }

    /// The central panel frame, plus the deferred work its body reported: both
    /// need `&mut App`, which the body itself does not have.
    fn central(&mut self, ui: &mut egui::Ui) {
        let mut out = CentralOutcome::default();
        egui::CentralPanel::default().show(ui, |ui| match self.shell.sidebar_tab {
            SidebarTab::Explorer => {
                central_body(ui, &mut self.shell, &mut self.contracts, &mut out);
            }
            SidebarTab::Git => git_view::central_body(ui, &self.git),
        });
        if out.toggle_edit {
            if self.contracts.editing {
                self.cancel_edit();
            } else {
                self.begin_edit();
            }
        }
        if let Some((path, buffer)) = out.promote
            && std::fs::write(&path, &buffer).is_ok()
        {
            self.contracts.repair = None;
            self.reload_project();
            if let Some(i) = self.contracts.entries.iter().position(|e| e.path == path) {
                self.load(i);
            }
        }
    }
}

impl eframe::App for App {
    // eframe 0.35 replaced `update(ctx)` with `ui(ui)`: the root now hands us a
    // `Ui` (no margin/background) instead of a `Context`. Panels attach to that
    // `ui`; the file dialogs and modals still work off the `Context`, reached
    // via `ui.ctx()`.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_dialog(&ctx);
        self.poll_git(&ctx);
        let top = self.top_bar(ui).map(Action::Sidebar);
        self.bottom_bar(ui);
        let side = self.sidebar(ui);
        match top.or(side) {
            Some(Action::Sidebar(SidebarAction::LoadContract(i))) => {
                let invalid = self
                    .contracts
                    .entries
                    .get(i)
                    .map(|e| e.error.is_some())
                    .unwrap_or(false);
                if invalid {
                    self.enter_repair(i);
                } else {
                    self.contracts.repair = None;
                    self.load(i);
                }
            }
            Some(Action::Sidebar(SidebarAction::LoadTemplate(i))) => self.load_template(i),
            Some(Action::Sidebar(SidebarAction::OpenProject)) => self.open_project(&ctx),
            Some(Action::Sidebar(SidebarAction::NewProject)) => self.new_project(&ctx),
            Some(Action::Sidebar(SidebarAction::ImportPostman)) => self.import_postman(&ctx),
            Some(Action::Sidebar(SidebarAction::NewTemplate)) => {
                self.contracts.new_template = Some(String::new());
                ctx.data_mut(|d| {
                    d.insert_temp(egui::Id::new(FOCUS_NEW_TEMPLATE), "open".to_string())
                });
            }
            Some(Action::Sidebar(SidebarAction::NewRequest(prefix))) => {
                self.contracts.new_request = Some(prefix);
                self.contracts.new_request_seed = 0;
                ctx.data_mut(|d| {
                    d.insert_temp(egui::Id::new(FOCUS_NEW_REQUEST), "open".to_string())
                });
            }
            Some(Action::Sidebar(SidebarAction::RequestDelete(target))) => {
                self.contracts.pending_delete = Some(target);
            }
            Some(Action::Sidebar(SidebarAction::ToggleSidebar)) => {
                self.shell.sidebar_open = !self.shell.sidebar_open;
            }
            Some(Action::Sidebar(SidebarAction::SwitchTab(tab))) => {
                let entered_git =
                    tab == SidebarTab::Git && self.shell.sidebar_tab != SidebarTab::Git;
                self.shell.sidebar_tab = tab;
                if entered_git {
                    self.spawn(&ctx);
                }
            }
            Some(Action::Git(GitAction::Refresh)) => {
                self.spawn(&ctx);
            }
            Some(Action::Git(GitAction::Select { path, staged })) => {
                self.git.selected = Some((path, staged));
            }
            Some(Action::Git(GitAction::Stage { path })) => {
                self.spawn_stage(&ctx, path);
            }
            Some(Action::Git(GitAction::Unstage { path })) => {
                self.spawn_unstage(&ctx, path);
            }
            Some(Action::Git(GitAction::RequestDiscard { path })) => {
                self.git.pending_discard = Some(path);
            }
            Some(Action::Git(GitAction::ConfirmDiscard)) => {
                if let Some(path) = self.git.pending_discard.take() {
                    self.spawn_discard(&ctx, path);
                }
            }
            Some(Action::Git(GitAction::Commit)) => {
                self.spawn_commit(&ctx, self.git.commit_message.clone());
            }
            None => {}
        }
        self.central(ui);
        self.new_template_dialog(&ctx);
        self.new_request_dialog(&ctx);
        self.delete_dialog(&ctx);
        self.open_blocked_dialog(&ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apic_core::edit::EditModel;

    /// A minimal but valid contract, loaded the same way `load()` does.
    fn sample_model() -> EditModel {
        let json = r#"{
            "name": "test",
            "method": "GET",
            "url": "https://example.com",
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
        app.contracts.model = Some(sample_model());
        let original = app.contracts.model.clone();

        app.begin_edit();
        // Simulate the reported destructive edit: clear the response code 200.
        app.contracts.model.as_mut().unwrap().responses[0].code = String::new();
        assert_ne!(
            app.contracts.model, original,
            "the edit should change the model"
        );

        app.cancel_edit();
        assert_eq!(
            app.contracts.model, original,
            "cancel must restore the pre-edit model"
        );
        assert!(!app.contracts.editing, "cancel must exit edit mode");
        assert!(
            app.contracts.original_model.is_none(),
            "snapshot must be cleared after cancel"
        );
    }

    #[test]
    fn edit_mode_layout_settles() {
        let json = r#"{
            "name": "test",
            "method": "POST",
            "url": "https://example.com/users/{id}",
            "query": [{"name":"page","value":"","description":""}],
            "headers": [{"name":"Authorization","value":"Bearer x"}],
            "request": { "name": "a", "meta": { "age": 1 } },
            "responses": [ { "code": 200, "description": "ok", "schema": {"id":"x"} } ]
        }"#;
        let contract = apic_core::json::json_get(json, None).expect("valid contract");
        let mut app = App::new();
        app.shell.project_root = Some(std::path::PathBuf::from("/tmp"));
        app.contracts.model = Some(EditModel::from_contract(contract));
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
                // `run_ui` hands the closure a root `Ui`, exactly like eframe's
                // `App::ui` (egui 0.35 replaced the old `Context::run`, which
                // passed a `Context`).
                let out = ctx.run_ui(input.clone(), |ui| {
                    app.top_bar(ui);
                    app.bottom_bar(ui);
                    app.sidebar(ui);
                    app.central(ui);
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
            let m = app.contracts.model.as_mut().unwrap();
            let new_idx = m.query.len();
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
