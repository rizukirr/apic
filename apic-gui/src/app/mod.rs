//! Application shell: the top-level state container, the eframe entry point,
//! and the orchestration that spans features.
//!
//! `App` owns one state struct per feature. A new feature adds a field here and a match arm in [`actions`]; it does not
//! touch any other feature's state.

pub(crate) mod actions;
pub(crate) mod state;
#[cfg(test)]
pub(crate) mod test_support;

use std::path::{Path, PathBuf};

use eframe::egui;
use egui::RichText;

use crate::app::actions::{Action, ShellAction};
use crate::app::state::{CentralView, ShellState, SidebarView};
use crate::features::contracts::ui::state::{
    ContractsAction, ContractsEvent, ContractsPaths, ContractsState, Guarded,
};
use crate::features::contracts::ui::view::{
    FOCUS_NEW_REQUEST, FOCUS_NEW_TEMPLATE, central_body, env_menu, header_actions, sidebar_body,
    templates_body,
};
use crate::features::git::ui::state::{GitAction, GitEvent, GitState};
use crate::features::git::ui::view;
use crate::features::project::data::project_root_of;
use crate::features::project::ui::state::{ProjectAction, ProjectEvent, ProjectState};
use crate::features::project::ui::view::welcome;
use crate::platform::settings::Settings;
use crate::ui::components::{icon_button, section_header};
use crate::ui::theme::*;

/// What the app bar's theme toggle switches to from the theme now showing,
/// with the icon and hover text for that switch: a sun while dark (switch to
/// light), a moon while light (switch to dark).
fn theme_toggle(current: egui::Theme) -> (egui::ThemePreference, &'static str, &'static str) {
    match current {
        egui::Theme::Dark => (
            egui::ThemePreference::Light,
            egui_phosphor::regular::SUN,
            "Switch to light theme",
        ),
        egui::Theme::Light => (
            egui::ThemePreference::Dark,
            egui_phosphor::regular::MOON,
            "Switch to dark theme",
        ),
    }
}

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
/// One field per feature. A new feature adds a field here and a match arm in the action dispatch; it does not touch any
/// other feature's state.
pub(crate) struct App {
    /// Application chrome and the active project location, shared by every
    /// feature.
    pub(crate) shell: ShellState,

    /// Everything the project feature owns: where the active project lives.
    pub(crate) project: ProjectState,

    /// Everything the contracts feature owns.
    pub(crate) contracts: ContractsState,

    /// Everything the git feature owns.
    pub(crate) git: GitState,

    /// Set once the window close was confirmed, so the close request that
    /// follows is not guarded again.
    pub(crate) closing: bool,

    /// The settings file every settings write from the app goes to, `None`
    /// when the OS names no config dir. Tests point it at a temp file so they
    /// never touch the developer's real settings.
    pub(crate) settings_path: Option<PathBuf>,
}

impl App {
    pub(crate) fn new() -> Self {
        let mut app = App {
            shell: ShellState::default(),
            project: ProjectState::default(),
            contracts: ContractsState::default(),
            git: GitState::default(),
            closing: false,
            settings_path: Settings::path(),
        };
        let settings = Settings::load();
        if let Some(root) = settings.last_project
            && root.is_dir()
        {
            app.project.project_root = Some(root);
        }
        app.reload_project();
        app.restore_env();
        let autoedit = std::env::var("APIC_AUTOEDIT").ok().and_then(|sub| {
            app.contracts
                .entries
                .iter()
                .position(|e| e.error.is_none() && e.rel.contains(&sub))
        });
        match autoedit {
            Some(i) => {
                let paths = app.contracts_paths();
                for action in [ContractsAction::Load(i), ContractsAction::ToggleEdit] {
                    let events = app.contracts.handle(action, paths.as_ref());
                    app.apply_contracts_events(events, None);
                }
            }
            None => app.restore_last_file(),
        }
        app
    }

    /// Loads the settings at `settings_path`, applies `change` and saves
    /// them. Does nothing when there is no settings path.
    fn update_settings(&self, change: impl FnOnce(&mut Settings)) {
        if let Some(path) = &self.settings_path {
            Settings::update_at(path, change);
        }
    }

    /// Remembers `path` as the open project's last file, relative to the
    /// project root with forward slashes. A path outside the project, or no
    /// open project, remembers nothing.
    fn remember_open_file(&self, path: &Path) {
        let Some(root) = self.project.project_root.clone() else {
            return;
        };
        let (Ok(canon_root), Ok(canon_path)) = (root.canonicalize(), path.canonicalize()) else {
            return;
        };
        let Ok(rel) = canon_path.strip_prefix(&canon_root) else {
            return;
        };
        let rel = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        self.update_settings(|s| s.remember_file(root, rel));
    }

    /// Reopens the open project's remembered last file. The saved relative
    /// path is untrusted, since the settings file can be edited, so it is
    /// never opened directly: it only selects a scanned valid contract or a
    /// listed template with the same canonical path, and only when the
    /// nearest `.apic` project above that file is the open project. Anything
    /// else opens nothing.
    fn restore_last_file(&mut self) {
        let Some(root) = self.project.project_root.clone() else {
            return;
        };
        let Some(settings_path) = &self.settings_path else {
            return;
        };
        let settings = Settings::load_from(settings_path);
        let Some(rel) = settings.last_file_of(&root) else {
            return;
        };
        if Path::new(rel).is_absolute() {
            return;
        }
        let (Ok(canon_root), Ok(abs)) = (root.canonicalize(), root.join(rel).canonicalize()) else {
            return;
        };
        if !abs.is_file() || project_root_of(&abs).as_ref() != Some(&canon_root) {
            return;
        }
        let same = |p: &Path| p.canonicalize().is_ok_and(|p| p == abs);
        let action = if let Some(i) = self
            .contracts
            .entries
            .iter()
            .position(|e| e.error.is_none() && same(&e.path))
        {
            ContractsAction::Load(i)
        } else if let Some(i) = self.contracts.templates.iter().position(|(_, p)| same(p)) {
            ContractsAction::LoadTemplate(i)
        } else {
            return;
        };
        let paths = self.contracts_paths();
        let events = self.contracts.handle(action, paths.as_ref());
        self.apply_contracts_events(events, None);
    }

    /// Remembers `name` as the open project's environment, or forgets the
    /// project's choice for `None`. No open project remembers nothing.
    fn remember_env(&self, name: Option<String>) {
        let Some(root) = self.project.project_root.clone() else {
            return;
        };
        self.update_settings(|s| s.remember_env(root, name));
    }

    /// Restores the open project's saved environment. The saved name is
    /// untrusted, so it is kept only when `.apic/env` still lists it, and no
    /// environment is selected otherwise.
    fn restore_env(&mut self) {
        let Some(apic_dir) = self.project.apic_dir.clone() else {
            return;
        };
        let saved = match (&self.project.project_root, &self.settings_path) {
            (Some(root), Some(path)) => Settings::load_from(path).env_of(root).map(str::to_string),
            _ => None,
        };
        self.contracts.send.restore_env(saved.as_deref(), &apic_dir);
    }

    /// Top header: sidebar toggle, the project dropdown (new, open, import), the branch dropdown, the environment dropdown and the theme toggle. Returns an action when one of those is chosen.
    fn top_bar(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let p = palette(ui.ctx());
        let mut action = None;
        let project = self
            .project
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
                    action = Some(Action::Shell(ShellAction::ToggleSidebar));
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
                                action = Some(Action::Project(ProjectAction::New));
                                ui.close();
                            }
                            if ui.button("Open project").clicked() {
                                action = Some(Action::Project(ProjectAction::Open));
                                ui.close();
                            }
                            if ui.button("Import Postman collection").clicked() {
                                action = Some(Action::Project(ProjectAction::ImportPostman));
                                ui.close();
                            }
                        });
                        if self.shell.repo_root.is_some()
                            && let Some(a) = view::branch_menu(ui, &mut self.git)
                        {
                            action = Some(Action::Git(a));
                        }
                        if let Some(apic_dir) = self.project.apic_dir.as_deref()
                            && let Some(a) = env_menu(ui, &mut self.contracts, apic_dir)
                        {
                            action = Some(Action::Contracts(a));
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (next, icon, hover) = theme_toggle(ui.ctx().theme());
                            if icon_button(ui, icon, hover).clicked() {
                                ui.ctx().set_theme(next);
                                self.update_settings(|s| s.theme = next);
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
                            action = Some(Action::Contracts(ContractsAction::OpenNewTemplate));
                        }
                    });
                    if toggled {
                        action = Some(Action::Shell(ShellAction::ToggleTemplatesSection));
                    }
                    if self.shell.templates_open
                        && let Some(a) = templates_body(ui, &mut self.contracts)
                    {
                        action = Some(Action::Contracts(a));
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
                                action = Some(Action::Shell(ShellAction::ToggleCommitSection));
                            }
                            if let Some(a) = view::commit_body(ui, &mut self.git) {
                                action = Some(Action::Git(a));
                            }
                        });
                } else {
                    egui::Panel::bottom("commit_section_closed").show(ui, |ui| {
                        if section_header(ui, false, commit_title, |_| {}) {
                            action = Some(Action::Shell(ShellAction::ToggleCommitSection));
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
                                action = Some(Action::Shell(ShellAction::SwitchView(
                                    SidebarView::Contracts,
                                )));
                                ui.close();
                            }
                            if ui
                                .button(RichText::new(changes_title).color(changes_color))
                                .clicked()
                            {
                                action = Some(Action::Shell(ShellAction::SwitchView(
                                    SidebarView::Changes,
                                )));
                                ui.close();
                            }
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            match self.shell.sidebar_view {
                                SidebarView::Contracts => {
                                    if let Some(a) = header_actions(ui) {
                                        action = Some(Action::Contracts(a));
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
                            action = Some(Action::Contracts(a));
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

    /// The central panel frame, plus the action its body returned, applied
    /// once the panel closure has released its borrow of the feature state.
    fn central(&mut self, ui: &mut egui::Ui) {
        let mut action = None;
        let has_project = self.project.project_root.is_some();
        egui::CentralPanel::default().show(ui, |ui| match self.shell.central {
            // Routed through `apply` so New and Open pass the unsaved-changes
            // guard like the app bar's.
            CentralView::Contracts if !has_project => {
                action = welcome(ui).map(Action::Project);
            }
            CentralView::Contracts => {
                action = central_body(ui, &mut self.contracts, has_project).map(Action::Contracts);
            }
            CentralView::Git => action = view::central_body(ui, &mut self.git).map(Action::Git),
        });
        // Applied after the panel closure ends: `apply` needs `&mut self`,
        // and the closure above is still holding a borrow of the feature state.
        if let Some(action) = action {
            let ctx = ui.ctx().clone();
            self.apply(action, &ctx);
        }
    }

    /// Where the contracts feature reads and writes, or `None` when no
    /// project is open.
    pub(crate) fn contracts_paths(&self) -> Option<ContractsPaths> {
        let apic_dir = self.project.apic_dir.clone()?;
        Some(ContractsPaths {
            contracts_root: self.project.root.clone(),
            apic_dir,
        })
    }

    /// Passes one contracts action to the contracts view model with the
    /// current project paths, then applies the events it returns.
    fn handle_contracts(&mut self, action: ContractsAction, ctx: &egui::Context) {
        if matches!(action, ContractsAction::OpenNewRequest(_)) {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new(FOCUS_NEW_REQUEST), "open".to_string()));
        }
        if matches!(action, ContractsAction::OpenNewTemplate) {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new(FOCUS_NEW_TEMPLATE), "open".to_string()));
        }
        let paths = self.contracts_paths();
        let events = self.contracts.handle(action, paths.as_ref());
        self.apply_contracts_events(events, Some(ctx));
    }

    /// Applies what the contracts view model asked the shell to do.
    ///
    /// # Arguments
    ///
    /// * `events` - what the contracts view model returned
    /// * `ctx` - the egui context, `None` off the frame loop. Only the
    ///   unsaved-changes popup yields [`ContractsEvent::Continue`], and its
    ///   actions always arrive with a context.
    pub(crate) fn apply_contracts_events(
        &mut self,
        events: Vec<ContractsEvent>,
        ctx: Option<&egui::Context>,
    ) {
        for event in events {
            match event {
                ContractsEvent::Status(text) => self.shell.status = text,
                // Writing a contract is the one moment the app itself dirties
                // the tree and knows it, so keep the tab honest without a
                // filesystem watcher.
                ContractsEvent::Saved => self.git.request_refresh(),
                ContractsEvent::ShowCentral => self.shell.central = CentralView::Contracts,
                ContractsEvent::Opened(path) => self.remember_open_file(&path),
                ContractsEvent::EnvChosen(name) => self.remember_env(name),
                ContractsEvent::Continue(g) => {
                    if let Some(ctx) = ctx {
                        self.continue_guarded(g, ctx);
                    }
                }
            }
        }
    }

    /// Runs a held action the shell owns, once the unsaved-changes popup let
    /// it through.
    fn continue_guarded(&mut self, g: Guarded, ctx: &egui::Context) {
        let action = match g {
            Guarded::OpenProject => ProjectAction::Open,
            Guarded::NewProject => ProjectAction::New,
            Guarded::CloseWindow => {
                self.closing = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            Guarded::Load(_) | Guarded::LoadTemplate(_) | Guarded::CancelEdit => return,
        };
        let events = self.project.handle(action, ctx);
        self.apply_project_events(events);
    }

    /// Holds a window close request for the unsaved-changes popup while the
    /// open contract has unsaved edits, cancelling the close.
    fn guard_close_request(&mut self, ctx: &egui::Context) {
        if !self.closing
            && ctx.input(|i| i.viewport().close_requested())
            && self.contracts.guard(Guarded::CloseWindow)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
    }

    /// Reconciles the open contract with the freshly checked-out branch.
    /// A checkout can change or remove the tree behind every piece of
    /// derived contract state at once, and `reload_project` only rebuilds
    /// `contracts.entries`. Does not touch `project.project_root` or persisted
    /// settings, unlike opening a project: a checkout changes neither.
    pub(crate) fn reconcile_after_checkout(&mut self) {
        match self.read_project() {
            Some(paths) => {
                let events = self.contracts.reconcile(&paths);
                self.apply_contracts_events(events, None);
            }
            None => self.contracts.close(),
        }
    }

    /// Discovers contracts for the active project and reads each one's method for
    /// the sidebar badge. Resolves everything against `self.project.project_root`;
    /// never reads the process current directory.
    pub(crate) fn reload_project(&mut self) {
        match self.read_project() {
            Some(paths) => {
                let events = self.contracts.reload(&paths);
                self.apply_contracts_events(events, None);
            }
            None => self.contracts.close(),
        }
    }

    /// Discovers the repository and locates the config of the active project,
    /// and returns where its contracts live. `None` when no project is open. A
    /// config that does not read leaves `contracts_root` `None` and reports the
    /// error on the status line.
    fn read_project(&mut self) -> Option<ContractsPaths> {
        let Some(root) = self.project.project_root.clone() else {
            let _ = self.project.locate();
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

        if let Err(err) = self.project.locate() {
            self.shell.status = apic_core::file::home_relative(&format!("Project error: {err}"));
        }
        self.contracts_paths()
    }

    /// Applies what the project view model asked the shell to do.
    fn apply_project_events(&mut self, events: Vec<ProjectEvent>) {
        for event in events {
            match event {
                ProjectEvent::Opened(folder) => {
                    self.contracts.close();
                    self.reload_project();
                    self.restore_env();
                    self.update_settings(|s| s.last_project = Some(folder));
                    self.restore_last_file();
                }
                ProjectEvent::ContractsChanged => self.reload_project(),
                ProjectEvent::Status(text) => self.shell.status = text,
            }
        }
    }

    /// Polls the git view model and applies the events it returns. Called
    /// every frame from `App::ui`.
    pub(crate) fn poll_git(&mut self, ctx: &egui::Context) {
        let paths = self.shell.repo_paths(&self.project);
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
        let paths = self.shell.repo_paths(&self.project);
        self.git.handle(action, paths.as_ref(), ctx);
    }

    /// Apply an action returned by the top bar or the sidebar to app state.
    fn apply(&mut self, action: Action, ctx: &egui::Context) {
        match action {
            Action::Project(action) => {
                let guarded = match action {
                    ProjectAction::Open => Some(Guarded::OpenProject),
                    ProjectAction::New => Some(Guarded::NewProject),
                    ProjectAction::ImportPostman | ProjectAction::DismissRefusal => None,
                };
                if guarded.is_some_and(|g| self.contracts.guard(g)) {
                    return;
                }
                let events = self.project.handle(action, ctx);
                self.apply_project_events(events);
            }
            Action::Shell(ShellAction::ToggleSidebar) => {
                self.shell.sidebar_open = !self.shell.sidebar_open;
            }
            Action::Shell(ShellAction::ToggleCommitSection) => {
                self.shell.commit_open = !self.shell.commit_open;
            }
            Action::Shell(ShellAction::SwitchView(view)) => {
                let was = self.shell.sidebar_view;
                self.shell.sidebar_view = view;
                if view == SidebarView::Changes && was != SidebarView::Changes {
                    self.handle_git(GitAction::Refresh, ctx);
                }
            }
            Action::Shell(ShellAction::ToggleTemplatesSection) => {
                self.shell.templates_open = !self.shell.templates_open;
            }
            Action::Contracts(action) => self.handle_contracts(action, ctx),
            Action::Git(GitAction::SwitchBranch { .. }) if self.contracts.has_unsaved_edits() => {
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
        self.guard_close_request(&ctx);
        let events = self.project.poll(&ctx);
        self.apply_project_events(events);
        self.poll_git(&ctx);
        self.contracts.send.poll(&ctx);
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
        if let Some(a) = crate::features::contracts::ui::view::dialogs(&ctx, &mut self.contracts) {
            self.handle_contracts(a, &ctx);
        }
        if let Some(a) = crate::features::project::ui::view::refusal_dialog(&ctx, &self.project) {
            self.apply(Action::Project(a), &ctx);
        }
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
    fn theme_toggle_flips_between_dark_and_light() {
        let (next, icon, _) = theme_toggle(egui::Theme::Dark);
        assert_eq!(next, egui::ThemePreference::Light);
        assert_eq!(icon, egui_phosphor::regular::SUN);
        let (next, icon, _) = theme_toggle(egui::Theme::Light);
        assert_eq!(next, egui::ThemePreference::Dark);
        assert_eq!(icon, egui_phosphor::regular::MOON);
    }

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
        let ctx = egui::Context::default();
        app.contracts.model = Some(sample_model());
        let original = app.contracts.model.clone();

        app.apply(Action::Contracts(ContractsAction::ToggleEdit), &ctx);
        // Simulate the reported destructive edit: clear the response code 200.
        app.contracts.model.as_mut().unwrap().responses[0].code = String::new();
        assert_ne!(
            app.contracts.model, original,
            "the edit should change the model"
        );

        app.apply(Action::Contracts(ContractsAction::ToggleEdit), &ctx);
        assert_eq!(
            app.contracts.pending_discard,
            Some(Guarded::CancelEdit),
            "cancel with edits must ask first"
        );
        app.apply(Action::Contracts(ContractsAction::DiscardAndContinue), &ctx);
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

    /// An app with `sample_model` open and its name edited.
    fn edited_app() -> App {
        let mut app = app_at(tempdir());
        app.contracts.model = Some(sample_model());
        app.contracts.editing = true;
        app.contracts.original_model = app.contracts.model.clone();
        app.contracts.model.as_mut().expect("model is open").name = "dirtied".to_string();
        app
    }

    #[test]
    fn open_project_with_unsaved_edits_waits_for_the_popup() {
        let mut app = edited_app();
        let ctx = egui::Context::default();

        app.apply(Action::Project(ProjectAction::Open), &ctx);

        assert_eq!(app.contracts.pending_discard, Some(Guarded::OpenProject));
        assert!(app.project.pending.is_none());
    }

    #[test]
    fn close_request_with_unsaved_edits_waits_for_the_popup() {
        let mut app = edited_app();
        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        input.viewports.insert(
            egui::ViewportId::ROOT,
            egui::ViewportInfo {
                events: vec![egui::ViewportEvent::Close],
                ..Default::default()
            },
        );

        let mut out = ctx.run_ui(input, |ui| app.guard_close_request(ui.ctx()));
        out.textures_delta.clear();

        assert_eq!(app.contracts.pending_discard, Some(Guarded::CloseWindow));
        let commands = out
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map(|v| v.commands.clone())
            .unwrap_or_default();
        assert!(commands.contains(&egui::ViewportCommand::CancelClose));
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
        let ctx = egui::Context::default();
        app.apply(Action::Contracts(ContractsAction::ToggleEdit), &ctx);

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
            ctx.data_mut(|d| {
                d.insert_temp(egui::Id::new("apic.focus.query"), format!("{new_idx}:Key"))
            });
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
            Action::Shell(ShellAction::SwitchView(SidebarView::Changes)),
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
        app.apply(Action::Contracts(ContractsAction::Load(0)), &ctx);
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
        app.apply(Action::Contracts(ContractsAction::Load(i)), &ctx);

        app.apply(Action::Contracts(ContractsAction::ToggleEdit), &ctx);
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
        let ctx = egui::Context::default();
        app.reload_project();
        let i = app
            .contracts
            .entries
            .iter()
            .position(|e| e.rel == "twobodies.json")
            .expect("twobodies.json is listed");
        // The scan flags this file invalid, which would send `Load` to the
        // repair editor. Clearing the flag reaches `load` itself, the path a
        // branch-switch reconcile takes without consulting the flag.
        app.contracts.entries[i].error = None;
        app.apply(Action::Contracts(ContractsAction::Load(i)), &ctx);

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
        app.apply(Action::Contracts(ContractsAction::Load(i)), &ctx);
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
        app.apply(Action::Contracts(ContractsAction::Load(i)), &ctx);
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

    /// An app on `root` whose temp settings remember `rel` as the project's
    /// last file, after the project loads and the last file is restored.
    fn restored_app(root: PathBuf, rel: &str) -> App {
        let mut app = app_at(root.clone());
        let settings_path = app
            .settings_path
            .clone()
            .expect("test app has a settings path");
        Settings::update_at(&settings_path, |s| s.remember_file(root, rel.to_string()));
        app.reload_project();
        app.restore_last_file();
        app
    }

    #[test]
    fn a_saved_contract_reopens() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        let app = restored_app(root.clone(), "contracts/sample.json");
        let expected = root
            .join("contracts")
            .join("sample.json")
            .canonicalize()
            .expect("sample canonicalizes");
        assert!(app.contracts.model.is_some());
        assert_eq!(
            app.contracts.path.as_ref().map(|p| p.canonicalize().ok()),
            Some(Some(expected))
        );
        assert_eq!(app.shell.central, CentralView::Contracts);
    }

    #[test]
    fn a_missing_saved_file_opens_nothing() {
        if !git_available() {
            return;
        }
        let app = restored_app(project_fixture(), "contracts/missing.json");
        assert!(app.contracts.model.is_none());
        assert!(app.contracts.path.is_none());
    }

    #[test]
    fn an_invalid_saved_contract_opens_nothing() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        std::fs::write(root.join("contracts").join("broken.json"), "{ not json")
            .expect("broken contract writes");
        let app = restored_app(root, "contracts/broken.json");
        assert!(app.contracts.model.is_none());
        assert!(app.contracts.path.is_none());
        assert!(app.contracts.repair.is_none());
    }

    #[test]
    fn a_saved_file_outside_the_project_opens_nothing() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        let outside = tempdir();
        std::fs::write(outside.join("x.json"), apic_core::template::DEFAULT)
            .expect("outside contract writes");
        let name = outside
            .file_name()
            .expect("temp dir has a name")
            .to_string_lossy()
            .into_owned();
        let app = restored_app(root, &format!("../{name}/x.json"));
        assert!(app.contracts.model.is_none());
        assert!(app.contracts.path.is_none());
    }

    #[test]
    fn a_saved_file_under_another_apic_root_opens_nothing() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        let nested = root.join("contracts").join("nested");
        std::fs::create_dir_all(nested.join(".apic")).expect("nested apic dir creates");
        std::fs::write(
            nested.join(".apic").join("config.toml"),
            "name = \"nested\"\nversion = \"0.1.0\"\n",
        )
        .expect("nested config writes");
        std::fs::write(nested.join("x.json"), apic_core::template::DEFAULT)
            .expect("nested contract writes");
        let app = restored_app(root, "contracts/nested/x.json");
        assert!(
            app.contracts
                .entries
                .iter()
                .any(|e| e.rel == "nested/x.json" && e.error.is_none()),
            "the nested contract must be a valid scanned entry for this to prove anything",
        );
        assert!(app.contracts.model.is_none());
        assert!(app.contracts.path.is_none());
    }

    /// An app on a project whose `.apic/env` holds `dev`, with `saved`
    /// remembered as its environment, after the project loads and the
    /// environment is restored.
    fn app_with_saved_env(saved: &str) -> App {
        let root = tempdir();
        let env_dir = root.join(".apic").join("env");
        std::fs::create_dir_all(&env_dir).expect("env dir creates");
        std::fs::write(env_dir.join("dev.toml"), "base = \"http://127.0.0.1:1\"\n")
            .expect("env file writes");
        let mut app = app_at(root.clone());
        let settings_path = app
            .settings_path
            .clone()
            .expect("test app has a settings path");
        Settings::update_at(&settings_path, |s| {
            s.remember_env(root, Some(saved.to_string()))
        });
        app.reload_project();
        app.restore_env();
        app
    }

    #[test]
    fn a_saved_env_that_still_exists_is_restored() {
        let app = app_with_saved_env("dev");
        assert_eq!(app.contracts.send.env.as_deref(), Some("dev"));
        assert!(app.contracts.send.env_vars.contains_key("base"));
    }

    #[test]
    fn a_saved_env_that_no_longer_exists_restores_as_none() {
        let app = app_with_saved_env("gone");
        assert_eq!(app.contracts.send.env, None);
    }

    #[test]
    fn choosing_an_env_saves_it_for_the_project() {
        let mut app = app_with_saved_env("dev");
        let ctx = egui::Context::default();
        let root = app.project.project_root.clone().expect("project is open");
        let settings_path = app
            .settings_path
            .clone()
            .expect("test app has a settings path");

        app.apply(Action::Contracts(ContractsAction::ChooseEnv(None)), &ctx);
        assert_eq!(Settings::load_from(&settings_path).env_of(&root), None);

        app.apply(
            Action::Contracts(ContractsAction::ChooseEnv(Some("dev".to_string()))),
            &ctx,
        );
        assert_eq!(
            Settings::load_from(&settings_path).env_of(&root),
            Some("dev")
        );
    }

    #[test]
    fn top_bar_renders_the_env_menu_in_both_themes() {
        for preference in crate::ui::theme::BOTH_THEMES {
            let mut app = app_with_saved_env("dev");
            crate::ui::theme::run_in_theme(preference, |ui| {
                let _ = app.top_bar(ui);
            });
        }
    }
}
