//! State owned by the contracts feature: the loaded contract, the browse list,
//! templates, and the modal dialogs that create or delete contracts.

use std::path::PathBuf;

use apic_core::edit::EditModel;

/// A discovered contract plus the lightweight summary shown in the sidebar.
pub(crate) struct Entry {
    pub(crate) path: PathBuf,
    pub(crate) rel: String,
    pub(crate) method: String,

    /// Validation error when this contract is invalid; `None` when it is valid.
    pub(crate) error: Option<String>,
}

/// In-progress raw-JSON repair of an invalid contract.
pub(crate) struct Repair {
    /// Index into `entries` of the file being repaired.
    pub(crate) index: usize,

    /// Editable raw file text.
    pub(crate) buffer: String,

    /// Current validation error for `buffer` (empty once valid).
    pub(crate) error: String,
}

/// A thing the user asked to delete (pending confirmation).
#[derive(Clone)]
pub(crate) enum DeleteTarget {
    /// A contract or folder, by path relative to the contracts root.
    Contract { rel: String, is_dir: bool },

    /// A template file in `.apic/template/`, by display name and absolute path.
    Template { name: String, path: PathBuf },
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
    pub(crate) resp_tab: usize,

    /// The active top-level editor tab; reset to [`MainTab::Overview`] on load.
    pub(crate) main_tab: MainTab,

    /// The active sub-tab inside the Response tab; reset to [`RespTab::Body`].
    pub(crate) resp_tab_view: RespTab,

    /// When `Some`, a modal listing contracts that must be fixed before the
    /// picked non-project folder can be opened/initialized.
    pub(crate) open_blocked: Option<Vec<(PathBuf, String)>>,

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

    /// When `Some`, the delete-confirmation dialog is open for this target.
    pub(crate) pending_delete: Option<DeleteTarget>,

    /// Snapshot of `model` taken when edit mode is entered, so [ CANCEL ] can
    /// restore the pre-edit state. `None` whenever not editing.
    pub(crate) original_model: Option<EditModel>,
}
