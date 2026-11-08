# fix gui stale branches: Implementation Plan

**Spec:** docs/specs/2026-11-08-fix-gui-stale-branches-design.md
**Goal:** Opening a project in another repository replaces the branch dropdown's list with that repository's branches.
**Architecture:** `App::read_project` compares the discovered repo root with the current one. When it changes, it clears `GitState.branches` and queues a listing through the existing `needs_branches` flag, which `GitState::poll` already starts once nothing is pending.

## Global constraints

- No interface change.
- CI runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`. The task leaves all three passing.
- Commits carry no `Co-Authored-By` line.
- Run limits: at most 3 verify rounds, at most $0 on paid runs.

### Task 1: reload branches when the repository changes → verify: `cargo test -p apic-gui` exits 0 and `cargo clippy --workspace --all-targets -- -D warnings` exits 0

**Files:**
- Modify: `apic-gui/src/app/mod.rs` (`read_project`, tests module)

- [x] Step 1: In the `app/mod.rs` tests module, add the guard test:

```rust
    /// Draws the branch dropdown once, the way the app bar does every frame,
    /// and applies the action it returns.
    fn draw_branch_menu(app: &mut App, ctx: &egui::Context) {
        let mut action = None;
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            action = crate::features::git::ui::view::branch_menu(ui, &mut app.git);
        });
        if let Some(a) = action {
            app.apply(Action::Git(a), ctx);
        }
    }

    /// Opening a project in another repository must replace the branch list,
    /// not keep the first repository's for the whole session.
    #[test]
    fn opening_another_project_reloads_its_branches() {
        if !git_available() {
            return;
        }
        let ctx = egui::Context::default();
        let first = project_fixture();
        git_output(&first, &["branch", "only-in-first"]);
        let mut app = app_at(first.clone());
        app.reload_project();
        draw_branch_menu(&mut app, &ctx);
        settle(&mut app, &ctx);
        assert!(app.git.branches.all.contains(&"only-in-first".to_string()));

        let second = project_fixture();
        git_output(&second, &["branch", "only-in-second"]);
        app.apply_project_events(vec![ProjectEvent::Opened(second.clone())], &ctx);
        draw_branch_menu(&mut app, &ctx);
        settle(&mut app, &ctx);

        let all = &app.git.branches.all;
        assert!(all.contains(&"only-in-second".to_string()), "stale list: {all:?}");
        assert!(!all.contains(&"only-in-first".to_string()), "stale list: {all:?}");
    }
```

- [x] Step 2: Run `cargo test -p apic-gui opening_another_project_reloads_its_branches` and confirm it fails with the stale list.

- [x] Step 3: In `read_project`, replace the whole `match crate::features::git::data::cli::discover(&root) { .. }` block with:

```rust
        let repo_root = match crate::features::git::data::cli::discover(&root) {
            Ok(repo_root) => repo_root,
            Err(err) => {
                self.shell.status = err;
                None
            }
        };
        if repo_root != self.shell.repo_root {
            // Another repository, or none: the branch list belongs to the old
            // one, and the dropdown only lists branches once on its own.
            self.git.branches = Default::default();
            self.git.needs_branches = repo_root.is_some();
        }
        self.shell.repo_root = repo_root;
        // The Git section header's dirty indicator must be correct before the
        // Git section is ever opened, so a project load fetches status
        // itself instead of waiting for tab activation.
        if self.shell.repo_root.is_some() {
            self.git.request_refresh();
        }
```

- [x] Step 4: Run `cargo fmt --all`.
- [x] Step 5: Run `cargo test -p apic-gui`.
- [x] Step 6: Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] Step 7: Commit: `fix(gui): reload the branch list when a project in another repository opens`.
