//! State owned by the project feature: where the active project lives, the
//! in-flight file dialog, and the folder that could not be opened.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use eframe::egui;

use crate::features::project::data::{self, PickKind};
use crate::features::project::domain::{OpenDecision, open_decision};

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

    /// When `Some`, a modal listing contracts that must be fixed before the
    /// picked non-project folder can be opened/initialized.
    pub(crate) open_blocked: Option<Vec<(PathBuf, String)>>,
}

/// A one-shot action from the project menu or the blocked-folder dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectAction {
    /// `[ Open ]`: launch the folder picker.
    Open,
    /// `[ New ]`: launch the folder picker.
    New,
    /// `[ Import ]` → Postman: launch the file picker.
    ImportPostman,
    /// Close the blocked-folder dialog.
    CloseBlocked,
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
    /// Applies one action from the project menu or the blocked dialog.
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
            ProjectAction::CloseBlocked => {
                self.open_blocked = None;
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
                match (kind, result) {
                    (PickKind::OpenProject, Some(p)) => self.finish_open(p),
                    (PickKind::NewProject, Some(p)) => self.finish_new(p),
                    (PickKind::ImportPostman, Some(p)) => self.finish_import_postman(p),
                    (_, None) => Vec::new(),
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

    /// Verify a chosen folder, then open / auto-init / block.
    pub(crate) fn finish_open(&mut self, folder: PathBuf) -> Vec<ProjectEvent> {
        let has_config = data::has_config(&folder);
        // No project: validate the folder's contracts before auto-initializing.
        let failures = if has_config {
            Vec::new()
        } else {
            data::validate(&folder)
        };
        match open_decision(has_config, failures) {
            OpenDecision::Open => self.activate(folder),
            OpenDecision::Init => match data::init(&folder) {
                Ok(()) => self.activate(folder),
                Err(e) => vec![ProjectEvent::Status(format!("init error: {e}"))],
            },
            OpenDecision::Blocked(failures) => {
                self.open_blocked = Some(failures);
                Vec::new()
            }
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
    fn opening_an_invalid_folder_blocks_it() {
        let folder = tempdir();
        std::fs::write(folder.join("broken.json"), "{ not json").unwrap();
        let mut state = ProjectState::default();

        let events = state.finish_open(folder.clone());

        assert!(
            events.is_empty(),
            "a blocked folder reports nothing: {events:?}"
        );
        assert!(
            state.project_root.is_none(),
            "a blocked folder is not opened"
        );
        assert!(
            state.open_blocked.as_ref().is_some_and(|f| !f.is_empty()),
            "the invalid contract must be listed"
        );
        assert!(
            !folder.join(".apic").exists(),
            "a blocked folder must not be initialized"
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
        assert!(state.open_blocked.is_none());
    }
}
