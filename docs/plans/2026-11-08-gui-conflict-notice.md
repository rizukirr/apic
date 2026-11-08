# gui conflict notice: Implementation Plan

**Spec:** docs/specs/2026-11-08-gui-conflict-notice-design.md
**Goal:** a "Merge conflict" popup opens whenever a status update brings a newly conflicted path, and OK dismisses it.
**Architecture:** `GitState::set_status`, the one place a status is stored, compares conflicted paths before and after and sets `conflict_notice`. A new `conflict_notice_dialog` in the git view draws it from `dialogs()` with the shared `dialog` and `detail_box` helpers.

## Global constraints
- No change outside the git feature except GUI.md and CHANGELOG.md.
- Run limits: at most 3 verify rounds, at most $0 on paid runs.
- Commits carry no `Co-Authored-By` line.
- CI: fmt --check, clippy -D warnings, test --workspace.

### Task 1: detect new conflicts in set_status → verify: `cargo test -p apic-gui conflict_notice` exits 0 and runs at least 1 test from `features::git::ui::state`

**Files:**
- Modify: `apic-gui/src/features/git/ui/state.rs` (the `GitState` struct, `set_status` at line 302, the tests module starting at line 630)

- [x] Step 1: Add a field to `GitState`, after `ticks`:

```rust
    /// Every conflicted path, set when a status brings a conflicted path the
    /// previous status did not have. Drives the "Merge conflict" popup, which
    /// clears it on OK. Cleared too by a status with no conflicts.
    pub(crate) conflict_notice: Option<Vec<String>>,
```

- [x] Step 2: Replace `set_status` with:

```rust
    /// Stores a fresh status and forgets ticks for paths no longer in it.
    /// Opens the conflict notice when a path is newly conflicted, and clears
    /// it when nothing is conflicted any more.
    fn set_status(&mut self, status: Status) {
        let before = conflicted_paths(&self.status);
        let now = conflicted_paths(&status);
        if now.is_empty() {
            self.conflict_notice = None;
        } else if now.iter().any(|p| !before.contains(p)) {
            self.conflict_notice = Some(now);
        }
        self.ticks.retain(|path, _| status.find(path).is_some());
        self.status = status;
    }
```

and add a free function in the same file, outside `impl GitState`:

```rust
/// Every conflicted path in `status`, inside the working dir first.
fn conflicted_paths(status: &Status) -> Vec<String> {
    status
        .inside
        .iter()
        .chain(status.outside.iter())
        .filter(|f| f.conflicted)
        .map(|f| f.path.clone())
        .collect()
}
```

- [x] Step 3: Add tests to the tests module in state.rs (import `crate::features::git::domain::status::fixtures::file` there):

```rust
    fn status_of(files: &[(&str, bool)]) -> Status {
        Status {
            inside: files.iter().map(|(p, c)| file(p, *c)).collect(),
            outside: vec![],
        }
    }

    #[test]
    fn conflict_notice_opens_when_a_conflict_appears() {
        let mut git = GitState::default();
        git.set_status(status_of(&[("a.json", true), ("b.json", false)]));
        assert_eq!(git.conflict_notice, Some(vec!["a.json".to_string()]));
    }

    #[test]
    fn conflict_notice_stays_dismissed_while_no_new_path_conflicts() {
        let mut git = GitState::default();
        git.set_status(status_of(&[("a.json", true)]));
        git.conflict_notice = None;
        git.set_status(status_of(&[("a.json", true)]));
        assert_eq!(git.conflict_notice, None);
    }

    #[test]
    fn conflict_notice_reopens_with_every_path_on_a_new_conflict() {
        let mut git = GitState::default();
        git.set_status(status_of(&[("a.json", true)]));
        git.conflict_notice = None;
        git.set_status(status_of(&[("a.json", true), ("b.json", true)]));
        assert_eq!(
            git.conflict_notice,
            Some(vec!["a.json".to_string(), "b.json".to_string()])
        );
    }

    #[test]
    fn conflict_notice_clears_when_no_conflict_is_left() {
        let mut git = GitState::default();
        git.set_status(status_of(&[("a.json", true)]));
        git.set_status(status_of(&[("a.json", false)]));
        assert_eq!(git.conflict_notice, None);
    }

    /// Spec goal 4: a project already mid-merge opens the notice on its
    /// first status.
    #[test]
    fn conflict_notice_opens_on_the_first_status_of_a_mid_merge_repo() {
        if !git_available() {
            return;
        }
        let root = conflict_fixture();
        let (mut git, paths) = git_at(&root);
        let ctx = egui::Context::default();
        git.handle(GitAction::Refresh, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        assert_eq!(
            git.conflict_notice,
            Some(vec!["contracts/sample.json".to_string()])
        );
    }

    /// Spec goal 3: a merge from the branch menu that conflicts opens the
    /// notice through the status reread after the failed merge.
    #[test]
    fn conflict_notice_opens_after_a_conflicting_merge_from_the_menu() {
        if !git_available() {
            return;
        }
        let root = conflict_fixture();
        git_output(&root, &["merge", "--abort"]);
        let (mut git, paths) = git_at(&root);
        let ctx = egui::Context::default();
        git.branches.current = Some(current_branch(&root));
        git.handle(
            GitAction::RequestBranchOp {
                kind: BranchOpKind::Merge,
                target: current_branch(&root),
                source: "conflict-branch".into(),
            },
            Some(&paths),
            &ctx,
        );
        git.handle(GitAction::ConfirmBranchOp, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        assert_eq!(
            git.conflict_notice,
            Some(vec!["contracts/sample.json".to_string()])
        );
    }
```

- [x] Step 4: Run `cargo test -p apic-gui conflict_notice`
- [x] Step 5: Run `cargo clippy -p apic-gui --all-targets -- -D warnings`
- [x] Step 6: Commit `feat(gui): note newly conflicted paths on each git status`

### Task 2: the Merge conflict popup and docs → verify: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` each exit 0, and `cargo test -p apic-gui conflict_notice_popup` runs at least 1 test

**Files:**
- Modify: `apic-gui/src/features/git/ui/view/mod.rs` (the `crate::ui::components` import at line 26, `dialogs()` at line 193, the tests module starting at line 318 including `menu_frame` at line 872)
- Modify: `GUI.md` (the Conflicts bullet at line 47)
- Modify: `CHANGELOG.md` (Unreleased, Added)

- [ ] Step 1: Add `detail_box` to the `crate::ui::components` import in view/mod.rs, and add this function below `dialogs()`:

```rust
/// The "Merge conflict" popup, shown while `conflict_notice` is set: it
/// lists every conflicted path. OK or closing dismisses it until a new path
/// conflicts.
fn conflict_notice_dialog(ctx: &egui::Context, state: &mut GitState) {
    let Some(paths) = state.conflict_notice.clone() else {
        return;
    };
    let p = palette(ctx);
    let outcome = dialog(
        ctx,
        "conflict_notice_modal",
        "Merge conflict",
        |ui| {
            ui.label(
                RichText::new("These files have conflicts. Resolve them in Changes.")
                    .color(p.on_surface_variant),
            );
            ui.add_space(SPACE_SMALL);
            detail_box(ui, |ui| {
                for path in &paths {
                    ui.label(RichText::new(path).color(p.on_surface).monospace());
                }
            });
        },
        &[DialogButton::new("OK", ButtonKind::Primary)],
    );
    if outcome.clicked.is_some() || outcome.closed {
        state.conflict_notice = None;
    }
}
```

- [ ] Step 2: In `dialogs()`, call `conflict_notice_dialog(ctx, state);` right before the final `action`.

- [ ] Step 3: In the tests module, move the AccessKit label extraction out of `menu_frame` into a helper and use it from `menu_frame`:

```rust
    /// Every AccessKit node in `out` that has a label, with its bounds.
    fn labelled_nodes(out: egui::FullOutput) -> Vec<(String, egui::Rect)> {
        out.platform_output
            .accesskit_update
            .map(|update| {
                update
                    .nodes
                    .into_iter()
                    .filter_map(|(_, node)| {
                        let b = node.bounds()?;
                        Some((
                            node.label()?.to_string(),
                            egui::Rect::from_min_max(
                                egui::pos2(b.x0 as f32, b.y0 as f32),
                                egui::pos2(b.x1 as f32, b.y1 as f32),
                            ),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
```

Then add:

```rust
    /// One headless frame of `dialogs` fed `events`, returning labelled nodes.
    fn dialogs_frame(
        ctx: &egui::Context,
        state: &mut GitState,
        events: Vec<egui::Event>,
    ) -> Vec<(String, egui::Rect)> {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            events,
            ..Default::default()
        };
        labelled_nodes(ctx.run_ui(input, |ui| {
            dialogs(ui.ctx(), state);
        }))
    }

    #[test]
    fn conflict_notice_popup_lists_paths_and_ok_dismisses_it() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut state = GitState {
            conflict_notice: Some(vec!["contracts/a.json".into()]),
            ..GitState::default()
        };
        dialogs_frame(&ctx, &mut state, vec![]);
        let nodes = dialogs_frame(&ctx, &mut state, vec![]);
        node(&nodes, "Merge conflict");
        node(&nodes, "contracts/a.json");
        let pos = node(&nodes, "OK").center();
        for events in [
            vec![egui::Event::PointerMoved(pos)],
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        ] {
            dialogs_frame(&ctx, &mut state, events);
        }
        assert_eq!(state.conflict_notice, None);
    }

    #[test]
    fn conflict_notice_popup_stays_open_until_dismissed() {
        for preference in BOTH_THEMES {
            let mut state = GitState {
                conflict_notice: Some(vec!["contracts/a.json".into()]),
                ..GitState::default()
            };
            run_in_theme(preference, |ui| {
                dialogs(ui.ctx(), &mut state);
            });
            assert!(state.conflict_notice.is_some());
        }
    }
```

- [ ] Step 4: In GUI.md, append to the Conflicts bullet: ` When a git status finds a file newly conflicted, from a Pull or Merge here or from git outside the app, a Merge conflict popup lists every conflicted file. OK closes it, and it comes back only when another file becomes conflicted.`

- [ ] Step 5: In CHANGELOG.md, add as the first bullet under Unreleased, Added: `- **Merge conflict popup.** When a git status finds a file newly conflicted, whether from a Pull or Merge in the branch menu, from git outside the app, or on opening a project mid-merge, a popup lists every conflicted file so the conflict is not missed. OK closes it until another file becomes conflicted.`

- [ ] Step 6: Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`
- [ ] Step 7: Commit `feat(gui): show a Merge conflict popup listing conflicted files`
