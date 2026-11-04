//! State owned by the project feature: where the active project lives, the
//! in-flight file dialog, and why the last picked folder was refused.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use eframe::egui;

use crate::features::project::data::{self, PickKind};
use crate::features::project::domain::open_refusal;

/// Everything the project feature owns. `App` holds one of these; no other
/// feature may read or write it.
#[derive(Default)]
pub(crate) struct ProjectState {
    /// Absolute root of the active project (the dir containing `.apic/`). `None`
    /// when no project is open. All discovery resolves against this, never cwd.
    pub(crate) project_root: Option<PathBuf>,

    /// The contracts working dir consumed by import / new-request / delete,
    /// kept in sync with the active project by [`ProjectState::locate`].
    pub(crate) root: Option<PathBuf>,

    /// The `.apic` directory, for locating templates.
    pub(crate) apic_dir: Option<PathBuf>,

    /// In-flight native file dialog, run on a background thread so the portal
    /// call never blocks the UI, plus the action to perform on its result.
    pub(crate) pending: Option<(PickKind, Receiver<Option<PathBuf>>)>,

    /// Why the folder picked with Open was refused, shown in a popup until
    /// dismissed. `None` when there is nothing to show.
    pub(crate) refusal: Option<String>,
}

/// A one-shot action from the project menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectAction {
    /// `[ Open ]`: launch the folder picker.
    Open,
    /// `[ New ]`: launch the folder picker.
    New,
    /// `[ Import ]` → Postman: launch the file picker.
    ImportPostman,
    /// Close the refused-folder popup.
    DismissRefusal,
}

/// Something a project action asks the app shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProjectEvent {
    /// This folder is now the active project: reload it and remember it.
    Opened(PathBuf),
    /// Contracts were written into the project, so reload it.
    ContractsChanged,
    /// Text for the shell status line.
    Status(String),
}

impl ProjectState {
    /// Applies one action from the project menu.
    ///
    /// # Arguments
    ///
    /// * `action` - what the view asked for
    /// * `ctx` - repainted once a picker returns
    ///
    /// # Returns
    ///
    /// What the app shell must apply, in order.
    pub(crate) fn handle(
        &mut self,
        action: ProjectAction,
        ctx: &egui::Context,
    ) -> Vec<ProjectEvent> {
        match action {
            ProjectAction::Open => self.start_pick(PickKind::OpenProject, ctx),
            ProjectAction::New => self.start_pick(PickKind::NewProject, ctx),
            ProjectAction::ImportPostman => {
                if self.root.is_none() {
                    return vec![ProjectEvent::Status("no project to import into".into())];
                }
                self.start_pick(PickKind::ImportPostman, ctx)
            }
            ProjectAction::DismissRefusal => {
                self.refusal = None;
                Vec::new()
            }
        }
    }

    /// Starts a picker unless one is already pending. A second dialog cannot
    /// start while one is pending.
    fn start_pick(&mut self, kind: PickKind, ctx: &egui::Context) -> Vec<ProjectEvent> {
        if self.pending.is_some() {
            return Vec::new();
        }
        let ctx = ctx.clone();
        let rx = data::pick(kind, move || ctx.request_repaint());
        self.pending = Some((kind, rx));
        vec![ProjectEvent::Status("Waiting for the file dialog…".into())]
    }

    /// Polls the in-flight dialog and runs its action once a path is chosen (or
    /// clears it on cancel). Called every frame from `App::ui`.
    pub(crate) fn poll(&mut self, ctx: &egui::Context) -> Vec<ProjectEvent> {
        let Some((kind, rx)) = &self.pending else {
            return Vec::new();
        };
        match rx.try_recv() {
            Ok(result) => {
                let kind = *kind;
                self.pending = None;
                let events = match (kind, result) {
                    (PickKind::OpenProject, Some(p)) => self.finish_open(p),
                    (PickKind::NewProject, Some(p)) => self.finish_new(p),
                    (PickKind::ImportPostman, Some(p)) => self.finish_import_postman(p),
                    (_, None) => Vec::new(),
                };
                if events.is_empty() {
                    // Cancelled or refused: replace "Waiting for the file
                    // dialog…" with what the footer showed before it.
                    vec![ProjectEvent::Status(self.idle_status())]
                } else {
                    events
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint();
                Vec::new()
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.pending = None;
                Vec::new()
            }
        }
    }

    /// The footer text when nothing is happening: the contracts folder of the
    /// open project, or the hint to open one.
    fn idle_status(&self) -> String {
        match &self.root {
            Some(root) => apic_core::file::home_relative(&apic_core::file::to_slash(root)),
            None => "No project open. Use Open or New.".to_string(),
        }
    }

    /// Opens a chosen folder when it is already a project, otherwise
    /// refuses it in a popup and leaves the open project as it is.
    pub(crate) fn finish_open(&mut self, folder: PathBuf) -> Vec<ProjectEvent> {
        let location = apic_core::file::home_relative(&apic_core::file::to_slash(&folder));
        match open_refusal(data::has_config(&folder), &location) {
            Some(refusal) => {
                self.refusal = Some(refusal);
                Vec::new()
            }
            None => self.activate(folder),
        }
    }

    /// Initialize a fresh project in `folder` (opening it if it already is one).
    fn finish_new(&mut self, folder: PathBuf) -> Vec<ProjectEvent> {
        match data::init(&folder) {
            Ok(()) | Err(_) => self.activate(folder), // Err = already a project
        }
    }

    /// Makes `folder` the active project.
    fn activate(&mut self, folder: PathBuf) -> Vec<ProjectEvent> {
        self.project_root = Some(folder.clone());
        vec![ProjectEvent::Opened(folder)]
    }

    /// Imports a Postman collection into the project via apic-core's converter,
    /// which writes contracts confined to the working dir and never overwrites.
    fn finish_import_postman(&mut self, src: PathBuf) -> Vec<ProjectEvent> {
        let Some(root) = self.root.clone() else {
            return vec![ProjectEvent::Status("no project to import into".into())];
        };
        match data::import_postman(&src, &root) {
            Ok((written, warnings)) => {
                let warn = if warnings == 0 {
                    String::new()
                } else {
                    format!(", {warnings} warning(s)")
                };
                vec![
                    ProjectEvent::ContractsChanged,
                    ProjectEvent::Status(format!("imported {written} contract(s){warn}")),
                ]
            }
            Err(e) => vec![ProjectEvent::Status(format!("import error: {e}"))],
        }
    }

    /// Sets `apic_dir` and `root` from `project_root`, clearing both when no
    /// project is open.
    ///
    /// # Errors
    ///
    /// Returns the config error text when the project config does not read,
    /// with `root` cleared and `apic_dir` still set.
    pub(crate) fn locate(&mut self) -> Result<(), String> {
        let Some(project_root) = self.project_root.clone() else {
            self.apic_dir = None;
            self.root = None;
            return Ok(());
        };
        self.apic_dir = Some(project_root.join(".apic"));
        match data::contracts_root(&project_root) {
            Ok(contracts_root) => {
                self.root = Some(contracts_root);
                Ok(())
            }
            Err(err) => {
                self.root = None;
                Err(err)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::app::test_support::{git_available, project_fixture, tempdir};

    #[test]
    fn opening_a_folder_without_apic_is_refused() {
        let folder = tempdir();
        std::fs::write(folder.join("contract.json"), "{}").unwrap();
        let open = PathBuf::from("/already/open");
        let mut state = ProjectState {
            project_root: Some(open.clone()),
            ..ProjectState::default()
        };

        let events = state.finish_open(folder.clone());

        assert!(events.is_empty(), "a refusal opens nothing: {events:?}");
        assert!(
            state
                .refusal
                .as_deref()
                .is_some_and(|r| r.starts_with("Not an apic project")),
            "the refusal must be shown in the popup"
        );
        state.handle(ProjectAction::DismissRefusal, &egui::Context::default());
        assert!(state.refusal.is_none(), "dismissing closes the popup");
        assert_eq!(state.project_root, Some(open), "the open project stays");
        assert!(
            !folder.join(".apic").exists(),
            "Open must never initialize a folder"
        );
    }

    #[test]
    fn opening_a_project_reports_opened() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        let mut state = ProjectState::default();

        let events = state.finish_open(root.clone());

        assert_eq!(events, vec![ProjectEvent::Opened(root.clone())]);
        assert_eq!(state.project_root, Some(root));
    }
}
