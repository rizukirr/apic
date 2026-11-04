//! State owned by the contracts feature: the loaded contract, the browse list,
//! templates, and the modal dialogs that create or delete contracts.

use std::path::PathBuf;

use apic_core::edit::{EditAction, EditModel, Field, apply};
use apic_core::json::method_str;

use crate::features::contracts::data::files;
use crate::features::contracts::domain::entry::{DeleteTarget, Entry, display_location};
use crate::features::contracts::domain::names::{request_target, template_file_name};

/// In-progress raw-JSON repair of an invalid contract.
pub(crate) struct Repair {
    /// Index into `entries` of the file being repaired.
    pub(crate) index: usize,

    /// Editable raw file text.
    pub(crate) buffer: String,

    /// Current validation error for `buffer` (empty once valid).
    pub(crate) error: String,
}

/// The active tab in the left pane. Response lives permanently in the right
/// pane, so it is not a variant here.
#[derive(Clone, Copy, PartialEq, Default)]
pub(crate) enum MainTab {
    #[default]
    Overview,
    Headers,
    Query,
    Request,
}

/// The active sub-tab inside the Response tab.
#[derive(Clone, Copy, PartialEq, Default)]
pub(crate) enum RespTab {
    #[default]
    Body,
    Headers,
}

/// Everything the contracts feature owns. `App` holds one of these; no other
/// feature may read or write it.
///
/// Deriving `Default` is correct here: every field's initial value is its
/// type's default, and `MainTab`/`RespTab` carry `#[default]` on the variants
/// the app starts on (`Overview` and `Body`).
#[derive(Default)]
pub(crate) struct ContractsState {
    pub(crate) entries: Vec<Entry>,
    pub(crate) selected: Option<usize>,
    pub(crate) model: Option<EditModel>,
    pub(crate) path: Option<PathBuf>,
    pub(crate) editing: bool,
    pub(crate) search: String,

    /// Whether the contract filter field is shown. Hiding it clears `search`, so no rows stay filtered without a visible field.
    pub(crate) search_open: bool,
    pub(crate) resp_tab: usize,

    /// The active top-level editor tab; reset to [`MainTab::Overview`] on load.
    pub(crate) main_tab: MainTab,

    /// The active sub-tab inside the Response tab; reset to [`RespTab::Body`].
    pub(crate) resp_tab_view: RespTab,

    /// Raw-JSON repair editor state for an invalid contract; `None` when not
    /// repairing.
    pub(crate) repair: Option<Repair>,

    /// Project templates: (display name, path) from `.apic/template/`.
    pub(crate) templates: Vec<(String, PathBuf)>,

    /// Index into `templates` when a template is being previewed.
    pub(crate) selected_template: Option<usize>,

    /// When `Some`, the "new template" dialog is open with this name buffer.
    pub(crate) new_template: Option<String>,

    /// When `Some`, the "new request" dialog is open with this path buffer.
    pub(crate) new_request: Option<String>,

    /// Index into `templates` of the template to seed a new request from, used
    /// only when more than one template exists (the dialog shows a chooser).
    pub(crate) new_request_seed: usize,

    /// The new-example popup's code and name buffers while it is open.
    pub(crate) new_example: Option<(String, String)>,

    /// When `Some`, the delete-confirmation dialog is open for this target.
    pub(crate) pending_delete: Option<DeleteTarget>,

    /// Snapshot of `model` taken when edit mode is entered, so [ CANCEL ] can
    /// restore the pre-edit state. `None` whenever not editing.
    pub(crate) original_model: Option<EditModel>,
}

/// Where the contracts feature reads and writes, built by `App` from the
/// project. `contracts_root` is `None` when the project config does not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContractsPaths {
    /// The contracts working dir named by the project config.
    pub(crate) contracts_root: Option<PathBuf>,

    /// The project's `.apic` directory, which holds the templates.
    pub(crate) apic_dir: PathBuf,
}

/// A one-shot action from a contracts view this frame.
pub(crate) enum ContractsAction {
    /// Open entry `i`, or its repair editor when it is invalid.
    Load(usize),
    /// Open template `i`.
    LoadTemplate(usize),
    /// Open the new-request dialog pre-filled with this path prefix.
    OpenNewRequest(String),
    /// Open the new-template dialog.
    OpenNewTemplate,
    /// Create what the new-request dialog holds.
    CreateRequest,
    /// Create what the new-template dialog holds.
    CreateTemplate,
    /// Open the new-example popup.
    OpenNewExample,
    /// Add the example the popup holds and select it.
    CreateExample,
    /// Ask to delete something, shows a confirmation first.
    RequestDelete(DeleteTarget),
    /// The pending delete was confirmed.
    ConfirmDelete,
    /// Close whichever of the new-request, new-template, new-example or delete dialogs is open.
    CancelDialog,
    /// Enter edit mode, or leave it restoring the pre-edit model.
    ToggleEdit,
    /// Save the open contract.
    Save,
    /// The repair buffer is valid: write it and open the contract.
    PromoteRepair,
    /// Show or hide the filter field.
    ToggleSearch,
}

/// Something a contracts action asks the app shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ContractsEvent {
    /// Text for the shell status line.
    Status(String),
    /// A contract was saved, or a file under the project was created,
    /// removed or repaired, so the git status is stale.
    Saved,
    /// Show the contracts view in the central panel.
    ShowCentral,
}

impl ContractsState {
    /// Shows or hides the filter field, clearing the filter when hiding it.
    pub(crate) fn toggle_search(&mut self) {
        self.search_open = !self.search_open;
        if !self.search_open {
            self.search.clear();
        }
    }

    /// Re-reads the templates in `paths.apic_dir` and the contracts under
    /// `paths.contracts_root`. With no contracts root the entries are cleared
    /// and nothing is reported.
    pub(crate) fn reload(&mut self, paths: &ContractsPaths) -> Vec<ContractsEvent> {
        let mut events = Vec::new();
        self.templates = files::templates(&paths.apic_dir);
        match &paths.contracts_root {
            Some(contracts_root) => {
                self.entries = files::scan(contracts_root);
                events.push(ContractsEvent::Status(display_location(contracts_root)));
            }
            None => self.entries.clear(),
        }
        events
    }

    /// Forgets everything read from a project: no project is open, or a
    /// different one is about to be.
    pub(crate) fn close(&mut self) {
        self.templates.clear();
        self.entries.clear();
        self.model = None;
        self.path = None;
        self.selected = None;
        self.selected_template = None;
        self.repair = None;
    }

    /// Reloads, then reconciles the open contract with the freshly
    /// checked-out branch. A checkout can change or remove the tree behind
    /// every piece of derived contract state at once, so the open contract is
    /// reopened when it still exists and cleared when it does not.
    pub(crate) fn reconcile(&mut self, paths: &ContractsPaths) -> Vec<ContractsEvent> {
        let mut events = self.reload(paths);
        let path = self.path.clone();
        match path.and_then(|p| self.entries.iter().position(|e| e.path == p)) {
            Some(i) => self.load(i, &mut events),
            None => {
                self.model = None;
                self.path = None;
                self.selected = None;
            }
        }
        events
    }

    /// Whether edit mode is on and the model differs from its pre-edit
    /// snapshot.
    pub(crate) fn has_unsaved_edits(&self) -> bool {
        self.editing && self.model != self.original_model
    }

    /// Applies one action from a contracts view.
    ///
    /// # Arguments
    ///
    /// * `action` - what the view asked for
    /// * `paths` - where the project's contracts and templates live, `None`
    ///   when no project is open
    ///
    /// # Returns
    ///
    /// What the app shell must apply, in order. A create, delete or repair
    /// promote with no `paths` reports "no project".
    pub(crate) fn handle(
        &mut self,
        action: ContractsAction,
        paths: Option<&ContractsPaths>,
    ) -> Vec<ContractsEvent> {
        let mut events = Vec::new();
        match action {
            ContractsAction::Load(i) => {
                events.push(ContractsEvent::ShowCentral);
                let invalid = self
                    .entries
                    .get(i)
                    .map(|e| e.error.is_some())
                    .unwrap_or(false);
                if invalid {
                    self.enter_repair(i);
                } else {
                    self.repair = None;
                    self.load(i, &mut events);
                }
            }
            ContractsAction::LoadTemplate(i) => {
                events.push(ContractsEvent::ShowCentral);
                self.load_template(i, &mut events);
            }
            ContractsAction::OpenNewRequest(prefix) => {
                self.new_request = Some(prefix);
                self.new_request_seed = 0;
            }
            ContractsAction::OpenNewTemplate => self.new_template = Some(String::new()),
            ContractsAction::CreateRequest => {
                let input = self.new_request.take().unwrap_or_default();
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
                self.create_request(&input, template, paths, &mut events);
            }
            ContractsAction::CreateTemplate => {
                let name = self.new_template.take().unwrap_or_default();
                self.create_template(&name, paths, &mut events);
            }
            ContractsAction::OpenNewExample => {
                self.new_example = Some((String::new(), String::new()));
            }
            ContractsAction::CreateExample => self.create_example(),
            ContractsAction::RequestDelete(target) => self.pending_delete = Some(target),
            ContractsAction::ConfirmDelete => {
                if let Some(target) = self.pending_delete.take() {
                    self.perform_delete(&target, paths, &mut events);
                }
            }
            ContractsAction::CancelDialog => {
                self.new_request = None;
                self.new_template = None;
                self.new_example = None;
                self.pending_delete = None;
            }
            ContractsAction::ToggleEdit => {
                if self.editing {
                    self.cancel_edit();
                } else {
                    self.begin_edit();
                }
            }
            ContractsAction::Save => self.save(&mut events),
            ContractsAction::PromoteRepair => self.promote_repair(paths, &mut events),
            ContractsAction::ToggleSearch => self.toggle_search(),
        }
        events
    }

    /// Adds the response the new-example popup holds and selects it. A code
    /// that is not exactly three ASCII digits, or no open model, leaves the
    /// popup open and changes nothing.
    fn create_example(&mut self) {
        let Some((code, name)) = self.new_example.take() else {
            return;
        };
        let valid = code.len() == 3 && code.bytes().all(|b| b.is_ascii_digit());
        let Some(model) = self.model.as_mut().filter(|_| valid) else {
            self.new_example = Some((code, name));
            return;
        };
        apply(
            model,
            &EditAction::Add {
                field: Field::ResponseAdd,
            },
        );
        let index = model.responses.len().saturating_sub(1);
        if let Some(response) = model.responses.last_mut() {
            response.code = code;
            response.description = name.trim().to_string();
        }
        self.resp_tab = index;
    }

    /// Enter edit mode, snapshotting the current model so the edits can be
    /// discarded on cancel.
    fn begin_edit(&mut self) {
        self.original_model = self.model.clone();
        self.editing = true;
    }

    /// Leave edit mode, restoring the pre-edit snapshot and discarding any edits
    /// made since [ EDIT ] was pressed.
    fn cancel_edit(&mut self) {
        if let Some(original) = self.original_model.take() {
            self.model = Some(original);
        }
        self.editing = false;
    }

    /// Loads an invalid contract's raw text into the repair editor.
    fn enter_repair(&mut self, i: usize) {
        let Some(entry) = self.entries.get(i) else {
            return;
        };
        let buffer = files::read_text(&entry.path).unwrap_or_default();
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
    fn load(&mut self, i: usize, events: &mut Vec<ContractsEvent>) {
        let Some(entry) = self.entries.get(i) else {
            return;
        };
        let path = entry.path.clone();
        match files::load_contract(&path) {
            Ok(model) => {
                self.model = Some(model);
                self.path = Some(path);
                self.selected = Some(i);
                self.selected_template = None;
                self.resp_tab = 0;
                self.main_tab = MainTab::Overview;
                self.resp_tab_view = RespTab::Body;
                self.editing = false;
                self.original_model = None;
                events.push(ContractsEvent::Status(
                    self.path
                        .as_deref()
                        .map(display_location)
                        .unwrap_or_default(),
                ));
            }
            Err(err) => events.push(ContractsEvent::Status(format!("load error: {err}"))),
        }
    }

    /// Loads template `i` into the editor, resolved against the built-in default
    /// into a full contract. Editable and savable like any contract: `path` keeps
    /// the template file so Save writes the edited contract straight back to it.
    /// (Saving a resolved template makes it a full template, every field it then
    /// contains is enforced when creating contracts from it.)
    fn load_template(&mut self, i: usize, events: &mut Vec<ContractsEvent>) {
        let Some((name, path)) = self.templates.get(i).cloned() else {
            return;
        };
        match files::load_template(&path) {
            Ok(model) => {
                self.model = Some(model);
                self.path = Some(path);
                self.selected = None;
                self.selected_template = Some(i);
                self.resp_tab = 0;
                self.main_tab = MainTab::Overview;
                self.resp_tab_view = RespTab::Body;
                self.editing = false;
                self.original_model = None;
                events.push(ContractsEvent::Status(format!("template '{name}'")));
            }
            Err(err) => events.push(ContractsEvent::Status(format!("template error: {err}"))),
        }
    }

    /// Creates a new template `<name>.json` in `.apic/template/`, seeded from the
    /// built-in default. Safety: the path is confined to the template dir, and an
    /// existing template is never overwritten.
    fn create_template(
        &mut self,
        name: &str,
        paths: Option<&ContractsPaths>,
        events: &mut Vec<ContractsEvent>,
    ) {
        let name = name.trim();
        let file_name = match template_file_name(name) {
            Ok(f) => f,
            Err(e) => {
                events.push(ContractsEvent::Status(e));
                return;
            }
        };
        let Some(paths) = paths else {
            events.push(ContractsEvent::Status("no project".into()));
            return;
        };
        match files::create_template(&paths.apic_dir, &file_name, name) {
            Ok(dest) => {
                events.extend(self.reload(paths));
                events.push(ContractsEvent::Saved);
                // Open the freshly created template in the central view, the same
                // way create_request opens a new contract.
                if let Some(i) = self.templates.iter().position(|(_, p)| *p == dest) {
                    self.load_template(i, events);
                    events.push(ContractsEvent::ShowCentral);
                }
                events.push(ContractsEvent::Status(format!("created template '{name}'")));
            }
            Err(e) => events.push(ContractsEvent::Status(e)),
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
    fn create_request(
        &mut self,
        input: &str,
        template: Option<PathBuf>,
        paths: Option<&ContractsPaths>,
        events: &mut Vec<ContractsEvent>,
    ) {
        if input.trim().is_empty() {
            events.push(ContractsEvent::Status("name required".into()));
            return;
        }
        let Some((paths, root)) =
            paths.and_then(|p| p.contracts_root.clone().map(|root| (p, root)))
        else {
            events.push(ContractsEvent::Status("no project".into()));
            return;
        };
        let target = match request_target(input) {
            Ok(t) => t,
            Err(e) => {
                events.push(ContractsEvent::Status(e));
                return;
            }
        };
        let rel = &target.rel;
        match files::create_request(&root, &target, template.as_deref()) {
            Ok(_) if target.is_folder => {
                events.extend(self.reload(paths));
                events.push(ContractsEvent::Saved);
                events.push(ContractsEvent::Status(format!("created folder {rel}/")));
            }
            Ok(dest) => {
                events.extend(self.reload(paths));
                events.push(ContractsEvent::Saved);
                if let Some(i) = self.entries.iter().position(|e| e.path == dest) {
                    self.load(i, events);
                    events.push(ContractsEvent::ShowCentral);
                }
                events.push(ContractsEvent::Status(format!("created {rel}")));
            }
            Err(e) => events.push(ContractsEvent::Status(e)),
        }
    }

    /// Removes the target (confined to its directory), then reloads. If the open
    /// contract/template was deleted, the editor is cleared.
    fn perform_delete(
        &mut self,
        target: &DeleteTarget,
        paths: Option<&ContractsPaths>,
        events: &mut Vec<ContractsEvent>,
    ) {
        let Some(paths) = paths else {
            events.push(ContractsEvent::Status("no project".into()));
            return;
        };
        match files::delete(
            target,
            paths.contracts_root.as_deref(),
            Some(paths.apic_dir.as_path()),
        ) {
            Ok((removed_path, label)) => {
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
                events.extend(self.reload(paths));
                events.push(ContractsEvent::Saved);
                events.push(ContractsEvent::Status(format!("deleted {label}")));
            }
            Err(e) => events.push(ContractsEvent::Status(e)),
        }
    }

    /// Writes the open contract to its path and leaves edit mode.
    fn save(&mut self, events: &mut Vec<ContractsEvent>) {
        let Some(model) = self.model.as_mut() else {
            return;
        };
        match self.path.as_deref() {
            Some(p) => match model.save(p) {
                Ok(()) => {
                    events.push(ContractsEvent::Status(format!("saved {}", p.display())));
                    self.editing = false; // back to read-only on success
                    self.original_model = None; // commit: drop the snapshot
                    // Refresh this contract's sidebar method badge.
                    if let Some(e) = self.entries.iter_mut().find(|e| e.path.as_path() == p) {
                        e.method = method_str(&model.method);
                    }
                    events.push(ContractsEvent::Saved);
                }
                Err(e) => events.push(ContractsEvent::Status(format!("save error: {e}"))),
            },
            None => events.push(ContractsEvent::Status("no path to save to".into())),
        }
    }

    /// Writes the now-valid repair buffer over the invalid file, reloads, and
    /// opens the repaired contract.
    fn promote_repair(&mut self, paths: Option<&ContractsPaths>, events: &mut Vec<ContractsEvent>) {
        let Some(paths) = paths else {
            events.push(ContractsEvent::Status("no project".into()));
            return;
        };
        let Some((path, buffer)) = self.repair.as_ref().and_then(|rep| {
            self.entries
                .get(rep.index)
                .map(|entry| (entry.path.clone(), rep.buffer.clone()))
        }) else {
            return;
        };
        if files::write(&path, &buffer).is_ok() {
            self.repair = None;
            events.extend(self.reload(paths));
            events.push(ContractsEvent::Saved);
            if let Some(i) = self.entries.iter().position(|e| e.path == path) {
                self.load(i, events);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_support::tempdir;

    /// A project directory holding `contracts/sample.json`, plus its paths.
    fn project() -> ContractsPaths {
        let root = tempdir();
        let contracts_root = root.join("contracts");
        std::fs::create_dir_all(&contracts_root).expect("contracts dir creates");
        std::fs::write(
            contracts_root.join("sample.json"),
            apic_core::template::DEFAULT,
        )
        .expect("sample writes");
        ContractsPaths {
            contracts_root: Some(contracts_root),
            apic_dir: root.join(".apic"),
        }
    }

    /// The index of the entry whose relative path is `rel`.
    fn entry(state: &ContractsState, rel: &str) -> usize {
        state
            .entries
            .iter()
            .position(|e| e.rel == rel)
            .expect("entry is listed")
    }

    #[test]
    fn load_opens_a_valid_contract() {
        let paths = project();
        let mut state = ContractsState::default();
        state.reload(&paths);
        let i = entry(&state, "sample.json");

        let events = state.handle(ContractsAction::Load(i), Some(&paths));

        assert!(state.model.is_some());
        assert_eq!(state.selected, Some(i));
        assert_eq!(state.path.as_ref(), Some(&state.entries[i].path));
        assert!(state.repair.is_none());
        assert!(events.contains(&ContractsEvent::ShowCentral));
        assert!(events.contains(&ContractsEvent::Status(display_location(
            &state.entries[i].path
        ))));
    }

    #[test]
    fn load_enters_repair_for_an_invalid_contract() {
        let paths = project();
        let contracts_root = paths.contracts_root.clone().expect("contracts root set");
        std::fs::write(contracts_root.join("broken.json"), "{ not json").expect("broken writes");
        let mut state = ContractsState::default();
        state.reload(&paths);
        let i = entry(&state, "broken.json");
        assert!(state.entries[i].error.is_some());

        let events = state.handle(ContractsAction::Load(i), Some(&paths));

        assert!(state.model.is_none());
        assert_eq!(state.selected, Some(i));
        let repair = state.repair.as_ref().expect("repair editor opens");
        assert_eq!(repair.index, i);
        assert_eq!(repair.buffer, "{ not json");
        assert!(!repair.error.is_empty());
        assert_eq!(events, vec![ContractsEvent::ShowCentral]);
    }

    #[test]
    fn deleting_the_open_contract_clears_it() {
        let paths = project();
        let mut state = ContractsState::default();
        state.reload(&paths);
        let i = entry(&state, "sample.json");
        state.handle(ContractsAction::Load(i), Some(&paths));
        let path = state.path.clone().expect("contract is open");

        state.handle(
            ContractsAction::RequestDelete(DeleteTarget::Contract {
                rel: "sample.json".to_string(),
                is_dir: false,
            }),
            Some(&paths),
        );
        let events = state.handle(ContractsAction::ConfirmDelete, Some(&paths));

        assert!(!path.exists());
        assert!(state.pending_delete.is_none());
        assert!(state.model.is_none());
        assert!(state.path.is_none());
        assert!(state.selected.is_none());
        assert!(state.entries.is_empty());
        assert_eq!(
            events.last(),
            Some(&ContractsEvent::Status("deleted sample.json".to_string()))
        );
    }

    #[test]
    fn save_reports_saved() {
        let paths = project();
        let mut state = ContractsState::default();
        state.reload(&paths);
        let i = entry(&state, "sample.json");
        state.handle(ContractsAction::Load(i), Some(&paths));
        state.handle(ContractsAction::ToggleEdit, Some(&paths));
        assert!(state.editing);

        let events = state.handle(ContractsAction::Save, Some(&paths));

        assert!(events.contains(&ContractsEvent::Saved));
        let path = state.path.clone().expect("contract is open");
        assert!(events.contains(&ContractsEvent::Status(format!("saved {}", path.display()))));
        assert!(!state.editing);
        assert!(state.original_model.is_none());
    }

    /// A state with `sample.json` loaded.
    fn loaded() -> (ContractsState, ContractsPaths) {
        let paths = project();
        let mut state = ContractsState::default();
        state.reload(&paths);
        let i = entry(&state, "sample.json");
        state.handle(ContractsAction::Load(i), Some(&paths));
        (state, paths)
    }

    #[test]
    fn new_example_adds_and_selects_it() {
        let (mut state, paths) = loaded();
        state.handle(ContractsAction::OpenNewExample, Some(&paths));
        state.new_example = Some(("404".to_string(), "Not found".to_string()));

        state.handle(ContractsAction::CreateExample, Some(&paths));

        let model = state.model.as_ref().expect("contract is open");
        let last = model.responses.last().expect("a response was added");
        assert_eq!(last.code, "404");
        assert_eq!(last.description, "Not found");
        assert_eq!(state.resp_tab, model.responses.len() - 1);
        assert!(state.new_example.is_none());
    }

    #[test]
    fn new_example_with_a_bad_code_changes_nothing() {
        let (mut state, paths) = loaded();
        let before = state
            .model
            .as_ref()
            .expect("contract is open")
            .responses
            .len();
        state.handle(ContractsAction::OpenNewExample, Some(&paths));
        state.new_example = Some(("40".to_string(), "Short".to_string()));

        state.handle(ContractsAction::CreateExample, Some(&paths));

        let after = state
            .model
            .as_ref()
            .expect("contract is open")
            .responses
            .len();
        assert_eq!(after, before);
        assert!(state.new_example.is_some());
    }

    #[test]
    fn cancel_dialog_closes_the_new_example_popup() {
        let (mut state, paths) = loaded();
        state.handle(ContractsAction::OpenNewExample, Some(&paths));
        assert!(state.new_example.is_some());

        state.handle(ContractsAction::CancelDialog, Some(&paths));

        assert!(state.new_example.is_none());
    }
}
