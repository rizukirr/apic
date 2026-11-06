//! State owned by the contracts feature: the loaded contract, the browse list,
//! templates, and the modal dialogs that create or delete contracts.

use std::path::PathBuf;

use apic_core::edit::{BodyLoc, EditAction, EditModel, Field, apply};
use apic_core::json::method_str;

use crate::features::contracts::data::files;
use crate::features::contracts::domain::entry::{DeleteTarget, Entry, display_location};
use crate::features::contracts::domain::names::{request_target, template_file_name};
use crate::features::contracts::ui::send::SendState;

/// In-progress raw-JSON repair of an invalid contract.
pub(crate) struct Repair {
    /// Index into `entries` of the file being repaired.
    pub(crate) index: usize,

    /// Editable raw file text.
    pub(crate) buffer: String,

    /// Current validation error for `buffer` (empty once valid).
    pub(crate) error: String,
}

/// The active sub-tab inside the Response tab.
#[derive(Clone, Copy, PartialEq, Default)]
pub(crate) enum RespTab {
    #[default]
    Body,
    Headers,
}

/// An action that would drop unsaved edits, held while the unsaved-changes
/// popup asks what to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Guarded {
    /// Open entry `i`.
    Load(usize),
    /// Open template `i`.
    LoadTemplate(usize),
    /// Leave edit mode, dropping the edits.
    CancelEdit,
    /// Start the Open project picker (run by the shell).
    OpenProject,
    /// Start the New project picker (run by the shell).
    NewProject,
    /// Close the window (run by the shell).
    CloseWindow,
}

/// Everything the contracts feature owns. `App` holds one of these; no other
/// feature may read or write it.
///
/// Deriving `Default` is correct here: every field's initial value is its
/// type's default, and `RespTab` carries `#[default]` on the variant the app
/// starts on (`Body`).
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

    /// The action the unsaved-changes popup is asking about, while it is open.
    pub(crate) pending_discard: Option<Guarded>,

    /// A body-type switch waiting for confirmation: which body, and whether to multipart.
    pub(crate) pending_body_switch: Option<(BodyLoc, bool)>,

    /// The selected environment, the request in flight and the live reply.
    pub(crate) send: SendState,
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
    /// Close whichever of the new-request, new-template, new-example, delete or path-param dialogs is open.
    CancelDialog,
    /// Enter edit mode, or leave it restoring the pre-edit model.
    ToggleEdit,
    /// Save the open contract.
    Save,
    /// The repair buffer is valid: write it and open the contract.
    PromoteRepair,
    /// Show or hide the filter field.
    ToggleSearch,
    /// Save, then run the held action.
    SaveAndContinue,
    /// Drop the edits, then run the held action.
    DiscardAndContinue,
    /// Close the popup and keep editing.
    KeepEditing,
    /// Make the body at `loc` multipart (`true`) or JSON (`false`). A body
    /// with content asks for confirmation first.
    SwitchBody {
        /// Which body to switch.
        loc: BodyLoc,
        /// Whether the body becomes multipart.
        multipart: bool,
    },
    /// The pending body switch was confirmed.
    ConfirmBodySwitch,
    /// Drop the pending body switch, keeping the body as it is.
    CancelBodySwitch,
    /// Send the open contract as shown, or ask for its path params first.
    Send,
    /// Drop the request in flight and its reply.
    CancelSend,
    /// Send with the values the path-param popup holds.
    SubmitParams,
    /// Show the live reply (`true`) or the documented examples (`false`) in
    /// the right pane.
    SetLive(bool),
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
    /// Run this held action, which the shell owns.
    Continue(Guarded),
    /// This contract or template is now open.
    Opened(PathBuf),
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
        self.send = SendState::default();
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

    /// Holds `g` for the unsaved-changes popup when there are unsaved edits.
    ///
    /// # Arguments
    ///
    /// * `g` - the action that would drop the edits
    ///
    /// # Returns
    ///
    /// `true` when the action must wait for the popup, `false` when it can
    /// run now. A popup already open keeps the action it is asking about.
    pub(crate) fn guard(&mut self, g: Guarded) -> bool {
        if !self.has_unsaved_edits() {
            return false;
        }
        if self.pending_discard.is_none() {
            self.pending_discard = Some(g);
        }
        true
    }

    /// Runs `g` now, or holds it for the popup when there are unsaved edits.
    fn guarded(&mut self, g: Guarded, events: &mut Vec<ContractsEvent>) {
        if !self.guard(g) {
            self.run_guarded(g, events);
        }
    }

    /// Runs a held action. The ones the shell owns become
    /// [`ContractsEvent::Continue`].
    fn run_guarded(&mut self, g: Guarded, events: &mut Vec<ContractsEvent>) {
        match g {
            Guarded::Load(i) => {
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
                    self.load(i, events);
                }
            }
            Guarded::LoadTemplate(i) => {
                events.push(ContractsEvent::ShowCentral);
                self.load_template(i, events);
            }
            Guarded::CancelEdit => self.cancel_edit(),
            Guarded::OpenProject | Guarded::NewProject | Guarded::CloseWindow => {
                events.push(ContractsEvent::Continue(g));
            }
        }
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
            ContractsAction::Load(i) => self.guarded(Guarded::Load(i), &mut events),
            ContractsAction::LoadTemplate(i) => {
                self.guarded(Guarded::LoadTemplate(i), &mut events);
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
                self.pending_body_switch = None;
                self.send.prompt = None;
            }
            ContractsAction::ToggleEdit => {
                if self.editing {
                    self.guarded(Guarded::CancelEdit, &mut events);
                } else {
                    self.begin_edit();
                }
            }
            ContractsAction::Save => {
                self.save(&mut events);
            }
            ContractsAction::PromoteRepair => self.promote_repair(paths, &mut events),
            ContractsAction::ToggleSearch => self.toggle_search(),
            ContractsAction::SaveAndContinue => {
                // A failed save already reported why, so the held action is dropped.
                let held = self.pending_discard.take();
                if self.save(&mut events)
                    && let Some(g) = held
                {
                    self.run_guarded(g, &mut events);
                }
            }
            ContractsAction::DiscardAndContinue => {
                let held = self.pending_discard.take();
                self.cancel_edit();
                if let Some(g) = held {
                    self.run_guarded(g, &mut events);
                }
            }
            ContractsAction::KeepEditing => self.pending_discard = None,
            ContractsAction::SwitchBody { loc, multipart } => self.switch_body(loc, multipart),
            ContractsAction::ConfirmBodySwitch => {
                if let Some((loc, multipart)) = self.pending_body_switch.take() {
                    self.apply_body_switch(&loc, multipart);
                }
            }
            ContractsAction::CancelBodySwitch => self.pending_body_switch = None,
            ContractsAction::Send => {
                if let Some(model) = self.model.as_ref() {
                    events.extend(self.send.send(
                        model,
                        self.path.as_deref(),
                        paths.map(|p| p.apic_dir.as_path()),
                    ));
                }
            }
            ContractsAction::SubmitParams => {
                if let Some(model) = self.model.as_ref() {
                    events.extend(self.send.submit_params(
                        model,
                        self.path.as_deref(),
                        paths.map(|p| p.apic_dir.as_path()),
                    ));
                }
            }
            ContractsAction::CancelSend => self.send.cancel(),
            ContractsAction::SetLive(live) => self.send.show_live = live,
        }
        events
    }

    /// Switches the body at `loc` to multipart or JSON. Nothing happens when
    /// it already is that kind. A body holding a non-blank JSON example or any
    /// part waits for the confirmation popup, an empty one switches at once.
    fn switch_body(&mut self, loc: BodyLoc, multipart: bool) {
        let Some(model) = self.model.as_ref() else {
            return;
        };
        let (example, parts) = match &loc {
            BodyLoc::Request => (
                model.request.as_ref().map_or("", |r| r.example.as_str()),
                model.multipart.len(),
            ),
            BodyLoc::Response(r) => match model.responses.get(*r) {
                Some(resp) => (resp.example.as_str(), resp.multipart.len()),
                None => return,
            },
        };
        if (parts > 0) == multipart {
            return;
        }
        if !example.trim().is_empty() || parts > 0 {
            self.pending_body_switch = Some((loc, multipart));
        } else {
            self.apply_body_switch(&loc, multipart);
        }
    }

    /// Applies a body switch to the open model. A multipart body is one with
    /// parts, so switching to multipart also adds one blank part, or the body
    /// would read as JSON again.
    fn apply_body_switch(&mut self, loc: &BodyLoc, multipart: bool) {
        let Some(model) = self.model.as_mut() else {
            return;
        };
        apply(
            model,
            &EditAction::SetBodyKind {
                loc: loc.clone(),
                multipart,
            },
        );
        let empty = match loc {
            BodyLoc::Request => model.multipart.is_empty(),
            BodyLoc::Response(r) => model
                .responses
                .get(*r)
                .is_some_and(|r| r.multipart.is_empty()),
        };
        if multipart && empty {
            apply(
                model,
                &EditAction::Add {
                    field: Field::PartAdd(loc.clone()),
                },
            );
        }
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
                self.path = Some(path.clone());
                self.selected = Some(i);
                self.selected_template = None;
                self.resp_tab = 0;
                self.resp_tab_view = RespTab::Body;
                self.editing = false;
                self.original_model = None;
                self.send.clear();
                events.push(ContractsEvent::Status(display_location(&path)));
                events.push(ContractsEvent::Opened(path));
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
                self.path = Some(path.clone());
                self.selected = None;
                self.selected_template = Some(i);
                self.resp_tab = 0;
                self.resp_tab_view = RespTab::Body;
                self.editing = false;
                self.original_model = None;
                self.send.clear();
                events.push(ContractsEvent::Status(format!("template '{name}'")));
                events.push(ContractsEvent::Opened(path));
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
    ///
    /// # Returns
    ///
    /// Whether the contract was written.
    fn save(&mut self, events: &mut Vec<ContractsEvent>) -> bool {
        let Some(model) = self.model.as_mut() else {
            return false;
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
                    true
                }
                Err(e) => {
                    events.push(ContractsEvent::Status(format!("save error: {e}")));
                    false
                }
            },
            None => {
                events.push(ContractsEvent::Status("no path to save to".into()));
                false
            }
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
    fn load_emits_opened() {
        let paths = project();
        let template_dir = paths.apic_dir.join("template");
        std::fs::create_dir_all(&template_dir).expect("template dir creates");
        std::fs::write(template_dir.join("base.json"), "{}").expect("template writes");
        let mut state = ContractsState::default();
        state.reload(&paths);
        let i = entry(&state, "sample.json");

        let events = state.handle(ContractsAction::Load(i), Some(&paths));
        assert!(events.contains(&ContractsEvent::Opened(state.entries[i].path.clone())));

        let t = state
            .templates
            .iter()
            .position(|(name, _)| name == "base")
            .expect("template is listed");
        let events = state.handle(ContractsAction::LoadTemplate(t), Some(&paths));
        assert!(events.contains(&ContractsEvent::Opened(state.templates[t].1.clone())));
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

    /// A state with `sample.json` loaded and its name edited, and the index
    /// of a second contract, `other.json`.
    fn edited() -> (ContractsState, ContractsPaths, usize) {
        let paths = project();
        let contracts_root = paths.contracts_root.clone().expect("contracts root set");
        std::fs::write(
            contracts_root.join("other.json"),
            apic_core::template::DEFAULT,
        )
        .expect("other writes");
        let mut state = ContractsState::default();
        state.reload(&paths);
        let i = entry(&state, "sample.json");
        state.handle(ContractsAction::Load(i), Some(&paths));
        state.editing = true;
        state.original_model = state.model.clone();
        state.model.as_mut().expect("contract is open").name = "dirtied".to_string();
        let other = entry(&state, "other.json");
        (state, paths, other)
    }

    /// The name of the open model.
    fn open_name(state: &ContractsState) -> &str {
        &state.model.as_ref().expect("contract is open").name
    }

    #[test]
    fn guarded_load_keeps_the_open_contract() {
        let (mut state, paths, other) = edited();
        let sample = state.path.clone();

        let events = state.handle(ContractsAction::Load(other), Some(&paths));

        assert!(events.is_empty());
        assert_eq!(state.pending_discard, Some(Guarded::Load(other)));
        assert_eq!(state.path, sample);
        assert_eq!(open_name(&state), "dirtied");
        assert!(state.editing);
    }

    #[test]
    fn unguarded_load_loads() {
        let (mut state, paths, other) = edited();
        state.model = state.original_model.clone();

        state.handle(ContractsAction::Load(other), Some(&paths));

        assert!(state.pending_discard.is_none());
        assert_eq!(state.selected, Some(other));
        assert_eq!(state.path.as_ref(), Some(&state.entries[other].path));
    }

    #[test]
    fn discard_and_continue_restores_and_loads() {
        let (mut state, paths, other) = edited();
        let sample = state.path.clone().expect("contract is open");
        state.handle(ContractsAction::Load(other), Some(&paths));

        let events = state.handle(ContractsAction::DiscardAndContinue, Some(&paths));

        assert!(state.pending_discard.is_none());
        assert!(!state.editing);
        assert_eq!(state.selected, Some(other));
        assert!(events.contains(&ContractsEvent::ShowCentral));
        let on_disk = files::load_contract(&sample).expect("sample loads");
        assert_ne!(on_disk.name, "dirtied");
    }

    #[test]
    fn save_and_continue_writes_and_loads() {
        let (mut state, paths, other) = edited();
        let sample = state.path.clone().expect("contract is open");
        state.handle(ContractsAction::Load(other), Some(&paths));

        let events = state.handle(ContractsAction::SaveAndContinue, Some(&paths));

        assert!(state.pending_discard.is_none());
        assert!(events.contains(&ContractsEvent::Saved));
        let on_disk = files::load_contract(&sample).expect("sample loads");
        assert_eq!(on_disk.name, "dirtied");
        assert_eq!(state.selected, Some(other));
        assert_eq!(state.path.as_ref(), Some(&state.entries[other].path));
    }

    #[test]
    fn keep_editing_keeps_the_edits() {
        let (mut state, paths, other) = edited();
        let sample = state.path.clone();
        state.handle(ContractsAction::Load(other), Some(&paths));

        state.handle(ContractsAction::KeepEditing, Some(&paths));

        assert!(state.pending_discard.is_none());
        assert!(state.editing);
        assert_eq!(state.path, sample);
        assert_eq!(open_name(&state), "dirtied");
    }

    #[test]
    fn guarded_cancel_waits_for_the_popup() {
        let (mut state, paths, _) = edited();

        state.handle(ContractsAction::ToggleEdit, Some(&paths));

        assert_eq!(state.pending_discard, Some(Guarded::CancelEdit));
        assert!(state.editing);
        assert_eq!(open_name(&state), "dirtied");

        state.handle(ContractsAction::DiscardAndContinue, Some(&paths));

        assert!(!state.editing);
        assert_ne!(open_name(&state), "dirtied");
    }

    /// A state whose open model has a non-blank JSON request body.
    fn with_json_request() -> (ContractsState, ContractsPaths) {
        let (mut state, paths) = loaded();
        let model = state.model.as_mut().expect("contract is open");
        model.multipart.clear();
        model.request = Some(apic_core::edit::EditBody {
            example: r#"{"a":1}"#.to_string(),
        });
        state.editing = true;
        (state, paths)
    }

    #[test]
    fn switching_a_body_with_content_waits_for_the_popup() {
        let (mut state, paths) = with_json_request();
        let before = state.model.clone();

        state.handle(
            ContractsAction::SwitchBody {
                loc: BodyLoc::Request,
                multipart: true,
            },
            Some(&paths),
        );

        assert_eq!(state.pending_body_switch, Some((BodyLoc::Request, true)));
        assert!(state.model == before, "nothing changes before the popup");
    }

    #[test]
    fn confirming_the_switch_applies_it() {
        let (mut state, paths) = with_json_request();
        state.handle(
            ContractsAction::SwitchBody {
                loc: BodyLoc::Request,
                multipart: true,
            },
            Some(&paths),
        );

        state.handle(ContractsAction::ConfirmBodySwitch, Some(&paths));

        assert!(state.pending_body_switch.is_none());
        let model = state.model.as_ref().expect("contract is open");
        assert!(model.request.is_none());
        assert_eq!(model.multipart.len(), 1, "one blank part to fill in");
    }

    #[test]
    fn cancelling_the_switch_keeps_the_body() {
        let (mut state, paths) = with_json_request();
        let before = state.model.clone();
        state.handle(
            ContractsAction::SwitchBody {
                loc: BodyLoc::Request,
                multipart: true,
            },
            Some(&paths),
        );

        state.handle(ContractsAction::CancelBodySwitch, Some(&paths));

        assert!(state.pending_body_switch.is_none());
        assert!(state.model == before);
    }

    #[test]
    fn switching_an_empty_body_is_immediate() {
        let (mut state, paths) = loaded();
        let model = state.model.as_mut().expect("contract is open");
        model.request = None;
        model.multipart.clear();
        if model.responses.is_empty() {
            model.responses.push(apic_core::edit::EditResponse::blank());
        }
        model.responses[0].example.clear();
        model.responses[0].multipart.clear();

        for loc in [BodyLoc::Request, BodyLoc::Response(0)] {
            state.handle(
                ContractsAction::SwitchBody {
                    loc: loc.clone(),
                    multipart: true,
                },
                Some(&paths),
            );
            assert!(state.pending_body_switch.is_none(), "{loc:?}");
        }
        let model = state.model.as_ref().expect("contract is open");
        assert_eq!(model.multipart.len(), 1);
        assert_eq!(model.responses[0].multipart.len(), 1);
    }

    #[test]
    fn guarded_open_project_continues_to_the_shell() {
        let (mut state, paths, _) = edited();

        assert!(state.guard(Guarded::OpenProject));
        let events = state.handle(ContractsAction::DiscardAndContinue, Some(&paths));

        assert_eq!(events, vec![ContractsEvent::Continue(Guarded::OpenProject)]);
        assert!(!state.editing);
        assert!(!state.guard(Guarded::OpenProject));
    }
}
