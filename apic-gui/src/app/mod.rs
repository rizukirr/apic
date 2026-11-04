//! Application shell: the top-level state container, the eframe entry point,
//! and the orchestration that spans features.
//!
//! `App` owns one state struct per feature plus the in-flight file dialog. A
//! new feature adds a field here and a match arm in [`actions`]; it does not
//! touch any other feature's state.

pub(crate) mod actions;
pub(crate) mod project;
pub(crate) mod state;
#[cfg(test)]
pub(crate) mod test_support;

use std::path::PathBuf;

use eframe::egui;
use egui::RichText;

use crate::app::actions::{Action, SidebarAction};
use crate::app::state::{CentralView, DialogKind, ShellState, SidebarView};
use crate::features::contracts::data::files;
use crate::features::contracts::state::ContractsState;
use crate::features::contracts::view::{
    CentralOutcome, FOCUS_NEW_REQUEST, FOCUS_NEW_TEMPLATE, central_body, header_actions,
    sidebar_body, templates_body,
};
use crate::features::git::ui::state::{GitAction, GitEvent, GitState};
use crate::features::git::ui::view;
use crate::settings::Settings;
use crate::ui::components::{icon_button, section_header};
use crate::ui::theme::*;

/// egui's default fonts plus the Phosphor icon font as a fallback for
/// proportional text, so any label can carry an `egui_phosphor::regular`
/// icon.
pub(crate) fn fonts_with_icons() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    fonts
}

/// Fixed row height of the top bar, so a `selectable_label` and a
/// `small_button` line up on one vertical centre instead of whatever baseline
/// their differing natural heights would give.
const TOOLBAR_ROW_H: f32 = 26.0;

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

    /// Top header: sidebar toggle, the project dropdown (new, open, import) and
    /// the branch dropdown and the theme menu. Returns an action when one of
    /// those is chosen.
    fn top_bar(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let p = palette(ui.ctx());
        let mut action = None;
        let project = self
            .shell
            .project_root
            .as_deref()
            .and_then(|root| root.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "No project".to_string());
        egui::Panel::top("nav").show(ui, |ui| {
            ui.add_space(SPACE_EXTRA_SMALL);
            ui.horizontal(|ui| {
                ui.set_min_height(TOOLBAR_ROW_H);
                if icon_button(ui, egui_phosphor::regular::SIDEBAR, "Toggle sidebar").clicked() {
                    action = Some(Action::Sidebar(SidebarAction::ToggleSidebar));
                }
                egui::MenuBar::new()
                    .style(|style: &mut egui::Style| {
                        egui::containers::menu::menu_style(style);
                        style.spacing.button_padding = egui::vec2(8.0, 4.0);
                    })
                    .ui(ui, |ui| {
                        let title = format!("{project} {}", egui_phosphor::regular::CARET_DOWN);
                        ui.menu_button(RichText::new(title).color(p.on_surface).strong(), |ui| {
                            if ui.button("New project").clicked() {
                                action = Some(Action::Sidebar(SidebarAction::NewProject));
                                ui.close();
                            }
                            if ui.button("Open project").clicked() {
                                action = Some(Action::Sidebar(SidebarAction::OpenProject));
                                ui.close();
                            }
                            if ui.button("Import Postman collection").clicked() {
                                action = Some(Action::Sidebar(SidebarAction::ImportPostman));
                                ui.close();
                            }
                        });
                        if self.shell.repo_root.is_some()
                            && let Some(a) = view::branch_menu(ui, &mut self.git)
                        {
                            action = Some(Action::Git(a));
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.menu_button(RichText::new("Theme").color(p.primary), |ui| {
                                let current = ui.ctx().options(|o| o.theme_preference);
                                for (preference, label) in [
                                    (egui::ThemePreference::System, "System"),
                                    (egui::ThemePreference::Dark, "Dark"),
                                    (egui::ThemePreference::Light, "Light"),
                                ] {
                                    if ui.selectable_label(current == preference, label).clicked() {
                                        ui.ctx().set_theme(preference);
                                        Settings::update(|s| s.theme = preference);
                                        ui.close();
                                    }
                                }
                            });
                        });
                    });
            });
            ui.add_space(SPACE_EXTRA_SMALL);
        });
        action
    }

    /// Bottom bar: the loaded contract's location (home-relative), nothing else.
    fn bottom_bar(&mut self, ui: &mut egui::Ui) {
        let p = palette(ui.ctx());
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&self.shell.status).color(p.on_surface_variant));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(concat!("apic v", env!("CARGO_PKG_VERSION")))
                            .color(p.on_surface_variant)
                            .size(11.0),
                    );
                });
            });
        });
    }

    /// The left sidebar: the Contracts or Changes list, with the Templates and
    /// Commit sections docked at the bottom.
    fn sidebar(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        if !self.shell.sidebar_open {
            return None;
        }
        let p = palette(ui.ctx());
        let mut action = None;
        egui::Panel::left("sidebar")
            .resizable(true)
            .default_size(240.0)
            .min_size(100.0)
            .show(ui, |ui| {
                egui::Panel::bottom("templates_section").show(ui, |ui| {
                    let label =
                        RichText::new(format!("{} Templates", egui_phosphor::regular::DIAMOND))
                            .color(p.on_surface);
                    let toggled = section_header(ui, self.shell.templates_open, label, |ui| {
                        if icon_button(ui, egui_phosphor::regular::PLUS, "New template").clicked() {
                            action = Some(Action::Sidebar(SidebarAction::NewTemplate));
                        }
                    });
                    if toggled {
                        action = Some(Action::Sidebar(SidebarAction::ToggleTemplatesSection));
                    }
                    if self.shell.templates_open
                        && let Some(a) = templates_body(ui, &mut self.contracts)
                    {
                        action = Some(Action::Sidebar(a));
                    }
                });
                let commit_title =
                    RichText::new(format!("{} Commit", egui_phosphor::regular::GIT_COMMIT))
                        .color(p.on_surface);
                if self.shell.commit_open {
                    egui::Panel::bottom("commit_section_open")
                        .resizable(true)
                        .default_size(160.0)
                        .min_size(100.0)
                        .show(ui, |ui| {
                            if section_header(ui, true, commit_title, |_| {}) {
                                action = Some(Action::Sidebar(SidebarAction::ToggleCommitSection));
                            }
                            if let Some(a) = view::commit_body(ui, &mut self.git) {
                                action = Some(Action::Git(a));
                            }
                        });
                } else {
                    egui::Panel::bottom("commit_section_closed").show(ui, |ui| {
                        if section_header(ui, false, commit_title, |_| {}) {
                            action = Some(Action::Sidebar(SidebarAction::ToggleCommitSection));
                        }
                    });
                }
                egui::Panel::top("sidebar_header").show(ui, |ui| {
                    ui.add_space(SPACE_SMALL);
                    ui.horizontal(|ui| {
                        let (changes_label, changes_color) =
                            view::tab_label("Changes", &self.git.status, p);
                        let changes_title =
                            format!("{} {changes_label}", egui_phosphor::regular::GIT_DIFF);
                        let current = match self.shell.sidebar_view {
                            SidebarView::Contracts => RichText::new(format!(
                                "{} Contracts {}",
                                egui_phosphor::regular::FOLDER_SIMPLE,
                                egui_phosphor::regular::CARET_DOWN
                            ))
                            .color(p.on_surface),
                            SidebarView::Changes => RichText::new(format!(
                                "{changes_title} {}",
                                egui_phosphor::regular::CARET_DOWN
                            ))
                            .color(changes_color),
                        };
                        ui.menu_button(current, |ui| {
                            let contracts =
                                format!("{} Contracts", egui_phosphor::regular::FOLDER_SIMPLE);
                            if ui
                                .button(RichText::new(contracts).color(p.on_surface))
                                .clicked()
                            {
                                action = Some(Action::Sidebar(SidebarAction::SwitchView(
                                    SidebarView::Contracts,
                                )));
                                ui.close();
                            }
                            if ui
                                .button(RichText::new(changes_title).color(changes_color))
                                .clicked()
                            {
                                action = Some(Action::Sidebar(SidebarAction::SwitchView(
                                    SidebarView::Changes,
                                )));
                                ui.close();
                            }
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            match self.shell.sidebar_view {
                                SidebarView::Contracts => {
                                    if let Some(a) = header_actions(ui, &mut self.contracts) {
                                        action = Some(Action::Sidebar(a));
                                    }
                                }
                                SidebarView::Changes => {
                                    if icon_button(
                                        ui,
                                        egui_phosphor::regular::ARROWS_CLOCKWISE,
                                        "Refresh status",
                                    )
                                    .clicked()
                                    {
                                        action = Some(Action::Git(GitAction::Refresh));
                                    }
                                }
                            }
                        });
                    });
                    ui.add_space(SPACE_SMALL);
                });
                match self.shell.sidebar_view {
                    SidebarView::Contracts => {
                        if let Some(a) = sidebar_body(ui, &mut self.contracts) {
                            action = Some(Action::Sidebar(a));
                        }
                    }
                    SidebarView::Changes => {
                        if let Some(a) =
                            view::changes_body(ui, &mut self.git, self.shell.repo_root.as_deref())
                        {
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
        let mut git_action = None;
        egui::CentralPanel::default().show(ui, |ui| match self.shell.central {
            CentralView::Contracts => {
                central_body(ui, &mut self.shell, &mut self.contracts, &mut out);
            }
            CentralView::Git => git_action = view::central_body(ui, &mut self.git),
        });
        // Applied after the panel closure ends: `apply` needs `&mut self`,
        // and the closure above is still holding a borrow of `self.git`.
        if let Some(action) = git_action {
            let ctx = ui.ctx().clone();
            self.apply(Action::Git(action), &ctx);
        }
        if out.toggle_edit {
            if self.contracts.editing {
                self.cancel_edit();
            } else {
                self.begin_edit();
            }
        }
        if let Some((path, buffer)) = out.promote
            && files::write(&path, &buffer).is_ok()
        {
            self.contracts.repair = None;
            self.reload_project();
            if let Some(i) = self.contracts.entries.iter().position(|e| e.path == path) {
                self.load(i);
            }
        }
        if out.saved {
            // A save is the one moment the app itself dirties the tree and
            // knows it, so keep the tab honest without a filesystem watcher.
            self.git.request_refresh();
        }
    }

    /// Polls the git view model and applies the events it returns. Called
    /// every frame from `App::ui`.
    pub(crate) fn poll_git(&mut self, ctx: &egui::Context) {
        let paths = self.shell.repo_paths();
        let events = self.git.poll(paths.as_ref(), ctx);
        for event in events {
            match event {
                GitEvent::Status(text) => self.shell.status = text,
                GitEvent::BranchSwitched => self.reconcile_after_checkout(),
            }
        }
    }

    /// Passes one git action to the git view model with the current repo paths.
    fn handle_git(&mut self, action: GitAction, ctx: &egui::Context) {
        let paths = self.shell.repo_paths();
        self.git.handle(action, paths.as_ref(), ctx);
    }

    /// Apply an action returned by the top bar or the sidebar to app state.
    fn apply(&mut self, action: Action, ctx: &egui::Context) {
        match action {
            Action::Sidebar(SidebarAction::LoadContract(i)) => {
                self.shell.central = CentralView::Contracts;
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
            Action::Sidebar(SidebarAction::LoadTemplate(i)) => {
                self.shell.central = CentralView::Contracts;
                self.load_template(i);
            }
            Action::Sidebar(SidebarAction::OpenProject) => self.open_project(ctx),
            Action::Sidebar(SidebarAction::NewProject) => self.new_project(ctx),
            Action::Sidebar(SidebarAction::ImportPostman) => self.import_postman(ctx),
            Action::Sidebar(SidebarAction::NewTemplate) => {
                self.contracts.new_template = Some(String::new());
                ctx.data_mut(|d| {
                    d.insert_temp(egui::Id::new(FOCUS_NEW_TEMPLATE), "open".to_string())
                });
            }
            Action::Sidebar(SidebarAction::NewRequest(prefix)) => {
                self.contracts.new_request = Some(prefix);
                self.contracts.new_request_seed = 0;
                ctx.data_mut(|d| {
                    d.insert_temp(egui::Id::new(FOCUS_NEW_REQUEST), "open".to_string())
                });
            }
            Action::Sidebar(SidebarAction::RequestDelete(target)) => {
                self.contracts.pending_delete = Some(target);
            }
            Action::Sidebar(SidebarAction::ToggleSidebar) => {
                self.shell.sidebar_open = !self.shell.sidebar_open;
            }
            Action::Sidebar(SidebarAction::ToggleCommitSection) => {
                self.shell.commit_open = !self.shell.commit_open;
            }
            Action::Sidebar(SidebarAction::SwitchView(view)) => {
                let was = self.shell.sidebar_view;
                self.shell.sidebar_view = view;
                if view == SidebarView::Changes && was != SidebarView::Changes {
                    self.handle_git(GitAction::Refresh, ctx);
                }
            }
            Action::Sidebar(SidebarAction::ToggleTemplatesSection) => {
                self.shell.templates_open = !self.shell.templates_open;
            }
            Action::Git(GitAction::SwitchBranch { .. })
                if self.contracts.editing
                    && self.contracts.model != self.contracts.original_model =>
            {
                let open = self
                    .contracts
                    .path
                    .as_deref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "the open contract".to_string());
                let msg = format!("cannot switch branch, {open} has unsaved edits");
                self.git.error = msg.clone();
                self.shell.status = msg;
            }
            Action::Git(action) => {
                if matches!(action, GitAction::Select { .. }) {
                    self.shell.central = CentralView::Git;
                }
                self.handle_git(action, ctx);
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
        let top = self.top_bar(ui);
        self.bottom_bar(ui);
        let side = self.sidebar(ui);
        if let Some(a) = top.or(side) {
            self.apply(a, &ctx);
        }
        self.central(ui);
        if let Some(a) = view::dialogs(&ctx, &mut self.git) {
            self.apply(Action::Git(a), &ctx);
        }
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

    use crate::app::test_support::{
        app_at, current_branch, dirty, git_available, git_output, project_fixture, settle, tempdir,
    };

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
    fn fonts_with_icons_registers_phosphor() {
        let fonts = fonts_with_icons();
        assert!(fonts.font_data.contains_key("phosphor"));
        let proportional = fonts
            .families
            .get(&egui::FontFamily::Proportional)
            .expect("proportional family exists");
        assert!(proportional.iter().any(|name| name == "phosphor"));
    }

    #[test]
    fn top_bar_renders_in_both_themes() {
        for preference in crate::ui::theme::BOTH_THEMES {
            let mut app = app_at(tempdir());
            crate::ui::theme::run_in_theme(preference, |ui| {
                let _ = app.top_bar(ui);
            });
        }
    }

    #[test]
    fn windows_defaults_to_wgpu_renderer() {
        assert!(matches!(
            eframe::Renderer::default(),
            eframe::Renderer::Wgpu
        ));
    }

    #[test]
    fn cancel_edit_restores_pre_edit_model() {
        let mut app = app_at(tempdir());
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
        let mut app = app_at(tempdir());
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
                let mut out = ctx.run_ui(input.clone(), |ui| {
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
                // egui 0.36's `TexturesDelta` panics on drop if it still holds
                // unapplied deltas. This harness stands in for eframe's
                // integration, which normally consumes the delta by uploading
                // it to the GPU, so clear it here instead.
                out.textures_delta.clear();
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

    #[test]
    fn sidebar_renders_each_view_and_section_state_in_both_themes() {
        for preference in crate::ui::theme::BOTH_THEMES {
            for view in [SidebarView::Contracts, SidebarView::Changes] {
                for (commit_open, templates_open) in
                    [(false, false), (true, false), (false, true), (true, true)]
                {
                    let mut app = app_at(tempdir());
                    app.shell.sidebar_view = view;
                    app.shell.commit_open = commit_open;
                    app.shell.templates_open = templates_open;
                    crate::ui::theme::run_in_theme(preference, |ui| {
                        let _ = app.sidebar(ui);
                    });
                }
            }
            if git_available() {
                let root = project_fixture();
                let dir = root.join(".apic").join("template");
                std::fs::create_dir_all(&dir).expect("template dir creates");
                std::fs::write(dir.join("base.json"), apic_core::template::DEFAULT)
                    .expect("template writes");
                let mut app = app_at(root);
                app.reload_project();
                app.shell.commit_open = true;
                app.shell.templates_open = true;
                for view in [SidebarView::Contracts, SidebarView::Changes] {
                    app.shell.sidebar_view = view;
                    crate::ui::theme::run_in_theme(preference, |ui| {
                        let _ = app.sidebar(ui);
                    });
                }
            }
        }
    }

    #[test]
    fn switching_to_changes_starts_a_status_refresh() {
        if !git_available() {
            return;
        }
        let mut app = app_at(project_fixture());
        let ctx = egui::Context::default();
        app.reload_project();
        settle(&mut app, &ctx);
        assert!(app.git.pending.is_none());

        app.apply(
            Action::Sidebar(SidebarAction::SwitchView(SidebarView::Changes)),
            &ctx,
        );

        assert_eq!(app.shell.sidebar_view, SidebarView::Changes);
        assert!(
            app.git.pending.is_some(),
            "switching to Changes must start a status refresh"
        );
        settle(&mut app, &ctx);
    }

    #[test]
    fn top_bar_renders_with_and_without_a_repository() {
        for preference in crate::ui::theme::BOTH_THEMES {
            let mut app = app_at(tempdir());
            crate::ui::theme::run_in_theme(preference, |ui| {
                let _ = app.top_bar(ui);
            });
            if git_available() {
                let mut app = app_at(project_fixture());
                app.reload_project();
                crate::ui::theme::run_in_theme(preference, |ui| {
                    let _ = app.top_bar(ui);
                });
            }
        }
    }

    #[test]
    fn selecting_a_git_file_shows_git_in_the_central_panel() {
        let mut app = app_at(tempdir());
        let ctx = egui::Context::default();
        app.apply(
            Action::Git(GitAction::Select {
                path: "contracts/sample.json".to_string(),
                staged: false,
            }),
            &ctx,
        );
        assert_eq!(app.shell.central, CentralView::Git);
    }

    #[test]
    fn loading_a_contract_shows_contracts_in_the_central_panel() {
        let mut app = app_at(tempdir());
        let ctx = egui::Context::default();
        app.shell.central = CentralView::Git;
        app.apply(Action::Sidebar(SidebarAction::LoadContract(0)), &ctx);
        assert_eq!(app.shell.central, CentralView::Contracts);
    }

    /// Pins the launch-indicator defect: a project load must populate
    /// `git.status` on its own, before the Git section is ever opened. Opening
    /// the section is what used to fetch status, so the indicator on the
    /// section itself stayed empty at launch.
    #[test]
    fn reload_project_alone_populates_status() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        dirty(&root.join("contracts").join("sample.json"));

        let mut app = app_at(root);
        let ctx = egui::Context::default();

        app.reload_project();
        assert!(
            app.shell.sidebar_view == SidebarView::Contracts,
            "the sidebar must never switch to Changes for this to prove anything"
        );
        app.poll_git(&ctx);
        settle(&mut app, &ctx);

        assert!(
            app.git
                .status
                .inside
                .iter()
                .any(|f| f.path == "contracts/sample.json"),
            "reload_project must populate status without the Git section ever opening: {:?}",
            app.git.status.inside
        );
    }

    /// `.apic/config.toml` sits outside the contracts working dir in the
    /// fixture, the layout that made the original scoping bug invisible.
    /// Confirms it still lands in `inside` rather than `outside`.
    #[test]
    fn apic_config_counts_as_inside_when_working_dir_is_a_subdirectory() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        dirty(&root.join(".apic").join("config.toml"));

        let mut app = app_at(root);
        let ctx = egui::Context::default();
        app.reload_project();
        app.poll_git(&ctx);
        settle(&mut app, &ctx);

        assert!(
            app.git
                .status
                .inside
                .iter()
                .any(|f| f.path == ".apic/config.toml"),
            "config.toml must count as inside: {:?}",
            app.git.status.inside
        );
        assert!(
            !app.git
                .status
                .outside
                .iter()
                .any(|f| f.path == ".apic/config.toml"),
            "config.toml must not also be reported as outside: {:?}",
            app.git.status.outside
        );
    }

    /// Pins the bound in `settle`: a job that never resolves must panic the
    /// test rather than hang the suite. No real git command runs here, so
    /// this does not need `git_available` or the repository fixture. The
    /// sender is kept alive in `_tx` so the channel never disconnects:
    /// `poll_git` treats a disconnected channel as an error and clears
    /// `pending`, which would end the wait and prove nothing about the bound.
    #[test]
    #[should_panic(expected = "did not complete")]
    fn settle_panics_rather_than_hangs_on_a_job_that_never_resolves() {
        let mut app = app_at(tempdir());
        let ctx = egui::Context::default();

        let (_tx, rx) = std::sync::mpsc::channel();
        app.git.pending = Some(rx);

        settle(&mut app, &ctx);
    }

    /// A switch must be refused, before anything is spawned, while the open
    /// contract has edits that differ from its pre-edit snapshot. Otherwise
    /// a checkout would silently discard them.
    #[test]
    fn switch_branch_is_refused_with_unsaved_edits() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        let mut app = app_at(root.clone());
        let ctx = egui::Context::default();
        app.reload_project();
        // Drain the refresh the reload queued, so its success cannot clear
        // the error the refusal sets.
        settle(&mut app, &ctx);
        let i = app
            .contracts
            .entries
            .iter()
            .position(|e| e.rel == "sample.json")
            .expect("sample.json is in the fixture");
        app.load(i);

        app.begin_edit();
        app.contracts.model.as_mut().unwrap().name = "dirtied".to_string();

        app.apply(
            Action::Git(GitAction::SwitchBranch {
                name: "apic-second".to_string(),
            }),
            &ctx,
        );
        settle(&mut app, &ctx);

        assert_eq!(
            current_branch(&root),
            "apic-test",
            "a refused switch must not touch the checked-out branch"
        );
        assert!(
            !app.git.error.is_empty(),
            "the refusal must be surfaced in git.error"
        );
    }

    /// A contract that parses but fails validation, such as one declaring
    /// both a JSON body and a multipart body, must be refused by `load`
    /// rather than silently accepted.
    #[test]
    fn loading_a_contract_that_declares_two_bodies_is_refused() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        // Parses, but fails validation: `json_get` accepts this document and
        // `load_for_edit` does not, which is the whole reason that helper
        // exists. Written into the fixture's working dir so `reload_project`
        // lists it alongside `sample.json`.
        std::fs::write(
            root.join("contracts").join("twobodies.json"),
            r#"{
                "name": "x", "method": "POST", "url": "https://h", "headers": [],
                "request": { "a": 1 },
                "multipart": [ { "name": "f", "filename": "f.txt" } ],
                "responses": []
            }"#,
        )
        .expect("fixture file writes");

        let mut app = app_at(root.clone());
        app.reload_project();
        let i = app
            .contracts
            .entries
            .iter()
            .position(|e| e.rel == "twobodies.json")
            .expect("twobodies.json is listed");
        app.load(i);

        // The status assertion is what proves the Err arm ran. `model` starts
        // as `None`, so on its own it would also pass if `load` did nothing.
        assert!(
            app.shell.status.contains("load error"),
            "the refusal must be reported, got: {}",
            app.shell.status
        );
        assert!(
            app.contracts.model.is_none(),
            "an invalid contract must not reach the editor"
        );
    }

    /// A switch to a branch that still has the open contract reopens it, so
    /// the body shows the new branch's version rather than a stale one.
    #[test]
    fn switch_branch_reconciles_a_contract_present_on_both_branches() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        let mut app = app_at(root.clone());
        let ctx = egui::Context::default();
        app.reload_project();
        let i = app
            .contracts
            .entries
            .iter()
            .position(|e| e.rel == "sample.json")
            .expect("sample.json is in the fixture");
        app.load(i);
        let path_before = app.contracts.path.clone();

        app.apply(
            Action::Git(GitAction::SwitchBranch {
                name: "apic-second".to_string(),
            }),
            &ctx,
        );
        settle(&mut app, &ctx);

        assert_eq!(current_branch(&root), "apic-second");
        assert_eq!(
            app.contracts.path, path_before,
            "the same file stays open across the switch"
        );
        let on_disk = std::fs::read_to_string(root.join("contracts").join("sample.json"))
            .expect("sample.json reads on the new branch");
        assert!(
            on_disk.contains("second-"),
            "the new branch's content must be on disk: {on_disk}"
        );
        assert_eq!(
            app.contracts.model.as_ref().map(|m| m.name.as_str()),
            Some("second-endpoint-name"),
            "the reopened model must reflect the new branch's file"
        );
    }

    /// A switch to a branch that lacks the open contract clears it, rather
    /// than leaving a stale model, path or index pointing at a file the new
    /// branch does not have.
    #[test]
    fn switch_branch_clears_a_contract_absent_from_the_new_branch() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        let mut app = app_at(root.clone());
        let ctx = egui::Context::default();
        app.reload_project();

        app.apply(
            Action::Git(GitAction::SwitchBranch {
                name: "apic-second".to_string(),
            }),
            &ctx,
        );
        settle(&mut app, &ctx);
        assert_eq!(current_branch(&root), "apic-second");

        let i = app
            .contracts
            .entries
            .iter()
            .position(|e| e.rel == "other.json")
            .expect("other.json only exists on apic-second");
        app.load(i);
        assert!(app.contracts.model.is_some());

        app.apply(
            Action::Git(GitAction::SwitchBranch {
                name: "apic-test".to_string(),
            }),
            &ctx,
        );
        settle(&mut app, &ctx);

        assert_eq!(current_branch(&root), "apic-test");
        assert!(
            app.contracts.model.is_none(),
            "a contract absent from the new branch must be cleared"
        );
        assert!(app.contracts.path.is_none());
        assert!(app.contracts.selected.is_none());
    }

    /// A git mutation's success message must reach the status bar through
    /// `App::poll_git`, not only as a `GitEvent` from the view model.
    #[test]
    fn unstage_through_app_reports_in_shell_status() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        dirty(&root.join("contracts").join("sample.json"));
        git_output(&root, &["add", "contracts/sample.json"]);
        let mut app = app_at(root);
        let ctx = egui::Context::default();
        app.reload_project();
        settle(&mut app, &ctx);

        app.apply(
            Action::Git(GitAction::Unstage {
                path: "contracts/sample.json".to_string(),
            }),
            &ctx,
        );
        settle(&mut app, &ctx);

        assert_eq!(app.shell.status, "unstaged contracts/sample.json");
    }
}
