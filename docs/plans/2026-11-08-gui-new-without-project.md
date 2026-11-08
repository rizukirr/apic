# gui new without project: Implementation Plan

**Spec:** docs/specs/2026-11-08-gui-new-without-project-design.md
**Goal:** With no project open, "+ New request" and "+ New template" open the folder picker, make the picked folder the project (initializing it when needed), then open the matching dialog.
**Architecture:** The project feature gains `PickForNew(NewKind)`, a `NewTarget(NewKind)` pick kind and an `OpenNewDialog(NewKind)` event. The app shell routes the two contracts actions to the project action while no project is open, and turns `OpenNewDialog` back into the contracts action once `Opened` has reloaded the project.

## Global constraints

- The project feature never reads or writes contracts state. The app shell connects them.
- The picker runs on the existing background thread through `data::pick`.
- No new dependency.
- CI runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`. The task leaves all three passing.
- Commits carry no `Co-Authored-By` line.
- Run limits: at most 3 verify rounds, at most $0 on paid runs.
- No test starts a real picker. Tests fill `ProjectState::pending` with an `mpsc` channel instead.
- `NewKind` lives in `features/project/data/mod.rs` beside `PickKind`, since `data` may not depend on `ui` (the spec's Approach names `state.rs`, and this places it one layer lower).

### Task 1: picker before the new request and new template dialogs → verify: `cargo test -p apic-gui` exits 0 and `cargo clippy --workspace --all-targets -- -D warnings` exits 0

**Files:**
- Modify: `apic-gui/src/features/project/data/mod.rs` (`PickKind`, new `NewKind`, `pick`)
- Modify: `apic-gui/src/features/project/ui/state.rs` (`ProjectAction`, `ProjectEvent`, `handle`, `poll`, new `finish_new_for`, tests module)
- Modify: `apic-gui/src/app/mod.rs` (imports, `continue_guarded` and `ui` and `apply` calls to `apply_project_events`, `apply_project_events`, `apply`, new `route_new_without_project`, tests module)
- Modify: `GUI.md` (the Browse bullet)
- Modify: `CHANGELOG.md` (`## [Unreleased]` / `### Added`)

- [ ] Step 1: In `data/mod.rs`, add after the `PickKind` enum:

```rust
/// Which dialog a folder picked for a new request or template goes on to
/// open once it is the project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NewKind {
    Request,
    Template,
}
```

Add a `PickKind` variant after `ImportTarget`:

```rust
    /// Pick the folder that becomes the project a new request or template
    /// is created in, when none is open.
    NewTarget(NewKind),
```

In `pick`, change the folder arm's pattern to `PickKind::OpenProject | PickKind::NewProject | PickKind::ImportTarget | PickKind::NewTarget(_) => {` and add two arms to its `title` match, before the `PickKind::NewProject | PickKind::ImportPostman` arm:

```rust
                    PickKind::NewTarget(NewKind::Request) => "Folder for the new request",
                    PickKind::NewTarget(NewKind::Template) => "Folder for the new template",
```

- [ ] Step 2: In `state.rs`, change the data import to `use crate::features::project::data::{self, NewKind, PickKind};`. Add a `ProjectAction` variant after `ImportPostman`:

```rust
    /// "+ New request" or "+ New template" with no project open: launch the
    /// folder picker for the project to create it in.
    PickForNew(NewKind),
```

Add a `ProjectEvent` variant after `Status`:

```rust
    /// The picked folder is now the project: open the new request or new
    /// template dialog. Always follows `Opened`.
    OpenNewDialog(NewKind),
```

In `handle`, add `ProjectAction::PickForNew(kind) => self.start_pick(PickKind::NewTarget(kind), ctx),` after the `ImportPostman` arm. In `poll`, add `(PickKind::NewTarget(kind), Some(p)) => self.finish_new_for(p, kind),` before the `(_, None)` arm.

After `finish_new`, add:

```rust
    /// Makes `folder` the active project, initializing it unless it already
    /// is one, then asks for the new request or new template dialog.
    pub(crate) fn finish_new_for(&mut self, folder: PathBuf, kind: NewKind) -> Vec<ProjectEvent> {
        let mut events = self.finish_new(folder);
        events.push(ProjectEvent::OpenNewDialog(kind));
        events
    }
```

- [ ] Step 3: In the `state.rs` tests module, add:

```rust
    #[test]
    fn new_for_a_plain_folder_initializes_opens_and_asks_for_the_dialog() {
        let folder = tempdir();
        let mut state = ProjectState::default();

        let events = state.finish_new_for(folder.clone(), NewKind::Request);

        assert_eq!(
            events,
            vec![
                ProjectEvent::Opened(folder.clone()),
                ProjectEvent::OpenNewDialog(NewKind::Request),
            ]
        );
        assert!(data::has_config(&folder), "the folder must be a project");
        assert_eq!(state.project_root, Some(folder));
    }

    #[test]
    fn new_for_an_existing_project_keeps_its_config() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        let config = root.join(".apic").join("config.toml");
        let before = std::fs::read_to_string(&config).unwrap();
        let mut state = ProjectState::default();

        let events = state.finish_new_for(root.clone(), NewKind::Template);

        assert_eq!(
            events,
            vec![
                ProjectEvent::Opened(root.clone()),
                ProjectEvent::OpenNewDialog(NewKind::Template),
            ]
        );
        assert_eq!(std::fs::read_to_string(&config).unwrap(), before);
        assert_eq!(state.project_root, Some(root));
    }

    #[test]
    fn picked_new_target_opens_the_folder_and_asks_for_the_dialog() {
        let folder = tempdir();
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(Some(folder.clone())).unwrap();
        let mut state = ProjectState {
            pending: Some((PickKind::NewTarget(NewKind::Request), rx)),
            ..ProjectState::default()
        };

        let events = state.poll(&egui::Context::default());

        assert_eq!(
            events,
            vec![
                ProjectEvent::Opened(folder.clone()),
                ProjectEvent::OpenNewDialog(NewKind::Request),
            ]
        );
        assert!(state.pending.is_none());
    }

    #[test]
    fn cancelled_new_target_pick_opens_nothing() {
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(None).unwrap();
        let mut state = ProjectState {
            pending: Some((PickKind::NewTarget(NewKind::Request), rx)),
            ..ProjectState::default()
        };

        let events = state.poll(&egui::Context::default());

        assert!(
            events
                .iter()
                .all(|e| matches!(e, ProjectEvent::Status(_))),
            "{events:?}"
        );
        assert!(state.project_root.is_none());
        assert!(state.pending.is_none());
    }
```

- [ ] Step 4: In `app/mod.rs`, change the imports to `use crate::features::project::data::{NewKind, project_root_of};`. Give `apply_project_events` a context parameter, `fn apply_project_events(&mut self, events: Vec<ProjectEvent>, ctx: &egui::Context) {`, and pass `ctx` (or `&ctx` where the caller holds an owned `Context`) at every call. Find them with `grep -n "apply_project_events(" apic-gui/src/app/mod.rs`. Add an arm after `ProjectEvent::Status`:

```rust
                ProjectEvent::OpenNewDialog(kind) => {
                    let action = match kind {
                        NewKind::Request => ContractsAction::OpenNewRequest(String::new()),
                        NewKind::Template => ContractsAction::OpenNewTemplate,
                    };
                    self.handle_contracts(action, ctx);
                }
```

- [ ] Step 5: In `app/mod.rs` `apply`, change the guard arm `ProjectAction::ImportPostman | ProjectAction::DismissRefusal => None,` to `ProjectAction::ImportPostman | ProjectAction::DismissRefusal | ProjectAction::PickForNew(_) => None,`. Replace `Action::Contracts(action) => self.handle_contracts(action, ctx),` with:

```rust
            Action::Contracts(action) => {
                match route_new_without_project(action, self.project.project_root.is_some()) {
                    Action::Contracts(action) => self.handle_contracts(action, ctx),
                    other => self.apply(other, ctx),
                }
            }
```

Add this free function to `app/mod.rs`, above `impl App` or beside the other free functions in the file:

```rust
/// Sends "+ New request" and "+ New template" to the folder picker while no
/// project is open, since both need a project to write into. Every other
/// action, and both of these with a project open, pass through.
fn route_new_without_project(action: ContractsAction, has_project: bool) -> Action {
    match action {
        ContractsAction::OpenNewRequest(_) if !has_project => {
            Action::Project(ProjectAction::PickForNew(NewKind::Request))
        }
        ContractsAction::OpenNewTemplate if !has_project => {
            Action::Project(ProjectAction::PickForNew(NewKind::Template))
        }
        other => Action::Contracts(other),
    }
}
```

- [ ] Step 6: In the `app/mod.rs` tests module, add:

```rust
    #[test]
    fn new_request_and_template_go_to_the_picker_without_a_project() {
        assert!(matches!(
            route_new_without_project(ContractsAction::OpenNewRequest(String::new()), false),
            Action::Project(ProjectAction::PickForNew(NewKind::Request))
        ));
        assert!(matches!(
            route_new_without_project(ContractsAction::OpenNewTemplate, false),
            Action::Project(ProjectAction::PickForNew(NewKind::Template))
        ));
        assert!(matches!(
            route_new_without_project(ContractsAction::OpenNewRequest("auth/".into()), true),
            Action::Contracts(ContractsAction::OpenNewRequest(p)) if p == "auth/"
        ));
        assert!(matches!(
            route_new_without_project(ContractsAction::OpenNewTemplate, true),
            Action::Contracts(ContractsAction::OpenNewTemplate)
        ));
    }

    #[test]
    fn a_folder_picked_for_new_opens_the_matching_dialog() {
        if !git_available() {
            return;
        }
        let ctx = egui::Context::default();

        let root = project_fixture();
        let mut app = app_at(root.clone());
        app.apply_project_events(
            vec![
                ProjectEvent::Opened(root.clone()),
                ProjectEvent::OpenNewDialog(NewKind::Request),
            ],
            &ctx,
        );
        assert_eq!(app.contracts.new_request, Some(String::new()));

        let root = project_fixture();
        let mut app = app_at(root.clone());
        app.apply_project_events(
            vec![
                ProjectEvent::Opened(root.clone()),
                ProjectEvent::OpenNewDialog(NewKind::Template),
            ],
            &ctx,
        );
        assert_eq!(app.contracts.new_template, Some(String::new()));
    }
```

`project_fixture` and `app_at` are in `crate::app::test_support`. Extend the tests module's existing `use crate::app::test_support::{..}` line if one of them is missing.

- [ ] Step 7: In `GUI.md`, in the bullet that starts `- **Browse.**`, replace `and \`+\` starts a new request.` with `and \`+\` starts a new request. With no project open, \`+\` and the Templates section's \`+\` first ask for a folder, set it up as a project unless it already is one, open it, and then show the dialog.`

- [ ] Step 8: In `CHANGELOG.md`, under `## [Unreleased]` / `### Added`, add as the first bullet:

```
- **New request and New template without a project.** With no project open, the sidebar's `+` and the Templates section's `+` ask for a folder first, set it up as a project unless it already is one, open it, and then show the dialog. Before, the dialog opened and Create only said "no project".
```

- [ ] Step 9: Run `cargo fmt --all`.
- [ ] Step 10: Run `cargo test -p apic-gui`.
- [ ] Step 11: Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] Step 12: Commit: `feat(gui): pick a folder before New request and New template when no project is open`.
