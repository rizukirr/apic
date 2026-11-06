# gui branch actions: Implementation Plan

**Spec:** docs/specs/2026-11-06-gui-branch-actions-design.md
**Goal:** Right-clicking a branch in the app-bar dropdown offers Pull, Merge and Delete, each confirmed in a popup that names the right-clicked branch.
**Architecture:** `cli.rs` gains the git calls (remote branch list, fetch, merge), `jobs.rs` one `Mutation::Merge` that checks out, fetches and merges in one background job, `state.rs` a pending `BranchOp` with request and confirm actions. `branches.rs` draws the menu and popups, and the app shell refuses a confirm while the open contract has unsaved edits, as it already does for a switch.

## Global constraints

- Every git call goes through `apic-gui/src/features/git/data/cli.rs`, threading stays in `apic-gui/src/features/git/data/jobs.rs`.
- One job at a time: the new work runs as one `Mutation`, obeying the existing `pending` rule.
- `git checkout`, not `git switch`, matching `cli::switch_branch`.
- Uses the shared `dialog` popup layout and theme palette like the other branch dialogs.
- Run limits: at most 3 verify rounds, at most $0 on paid runs.
- CI runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`. Every task leaves all three passing.
- Test files use the existing helpers in `apic-gui/src/app/test_support.rs` (`git_available`, `project_fixture`, `tempdir`, `git_output`, `current_branch`, `git_at`, `settle_git`, `app_at`, `settle`). The fixture repo is on branch `apic-test` with a second branch `apic-second` that carries one extra commit touching `contracts/sample.json` and adding `contracts/other.json`.

### Task 1: git calls, merge job and state → verify: `cargo test -p apic-gui` exits 0 and `cargo clippy --workspace --all-targets -- -D warnings` exits 0

**Files:**
- Modify: `apic-gui/src/features/git/domain/status.rs` (struct `Branches`)
- Modify: `apic-gui/src/features/git/data/cli.rs` (`run`, `branches`, new functions, tests module)
- Modify: `apic-gui/src/features/git/data/jobs.rs` (`MutateKind`, `Mutation`, `impl Mutation`, tests module)
- Modify: `apic-gui/src/features/git/ui/state.rs` (`GitAction`, `GitState`, `handle`, `apply_result`, tests module)
- Modify: `apic-gui/src/features/git/ui/view/mod.rs` (tests module: every `Branches { .. }` struct literal)

- [x] Step 1: In `status.rs`, add a field to `Branches` after `all`, and extend its doc comment:

```rust
/// The local branches, which one is checked out, and the remote-tracking
/// branches.
///
/// `current` is `None` in detached HEAD. That is a normal state, not an
/// error: git reports no branch name there, and the panel should show it as
/// such rather than failing.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Branches {
    pub(crate) current: Option<String>,
    pub(crate) all: Vec<String>,

    /// Remote-tracking branches named `remote/branch`, such as `origin/main`.
    pub(crate) remotes: Vec<String>,
}
```

- [x] Step 2: In `cli.rs` `run`, add `.env("GIT_TERMINAL_PROMPT", "0")` before `.output()`, and make the error fall back to the last stdout line when stderr is empty (git prints merge conflicts to stdout). Replace the error block:

```rust
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let err = if err.is_empty() {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .rev()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .unwrap_or_default()
                .to_string()
        } else {
            err
        };
        return Err(if err.is_empty() {
            format!("git {} failed", args.join(" "))
        } else {
            err
        });
    }
```

Add one line to `run`'s doc comment: `GIT_TERMINAL_PROMPT=0 makes a command that needs credentials fail instead of waiting on a terminal the GUI does not have.`

- [x] Step 3: In `cli.rs` `branches`, replace `Ok(Branches { current, all })` with:

```rust
    Ok(Branches {
        current,
        all,
        remotes: remote_branches(root)?,
    })
```

- [x] Step 4: In `cli.rs`, after `delete_branch`, add:

```rust
/// Remote-tracking branches as `remote/branch`, such as `origin/main`. The
/// symbolic `refs/remotes/<remote>/HEAD` is left out: git prints it as the
/// bare remote name, or as `<remote>/HEAD` on older versions.
pub(crate) fn remote_branches(root: &Path) -> Result<Vec<String>, String> {
    let out = run(
        root,
        &["for-each-ref", "--format=%(refname:short)", "refs/remotes/"],
    )?;
    Ok(String::from_utf8_lossy(&out)
        .lines()
        .map(str::trim)
        .filter(|l| l.contains('/') && !l.ends_with("/HEAD"))
        .map(str::to_string)
        .collect())
}

/// Fetches every configured remote.
pub(crate) fn fetch_all(root: &Path) -> Result<(), String> {
    run(root, &["fetch", "--all"]).map(|_| ())
}

/// Merges `source`, a local branch or a `remote/branch`, into the checked-out
/// branch. `--no-edit` keeps git from opening an editor for a merge commit. A
/// conflict exits non-zero and leaves the repository mid-merge for the
/// resolver.
pub(crate) fn merge(root: &Path, source: &str) -> Result<(), String> {
    run(root, &["merge", "--no-edit", source]).map(|_| ())
}
```

- [x] Step 5: In `cli.rs` tests module, add these tests at the end:

```rust
    /// Commits `content` to `file.txt` on the checked-out branch.
    fn commit_file(dir: &Path, content: &str, message: &str) {
        fs::write(dir.join("file.txt"), content).unwrap();
        assert!(run(dir, &["add", "--", "file.txt"]).is_ok());
        assert!(commit(dir, message, false).is_ok());
    }

    #[test]
    fn merge_fast_forwards_a_local_branch_into_the_current_one() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        init_repo(&dir);
        let base = branches(&dir).unwrap().current.unwrap();
        create_branch(&dir, "feature").unwrap();
        switch_branch(&dir, "feature").unwrap();
        commit_file(&dir, "feature\n", "feature work");
        switch_branch(&dir, &base).unwrap();

        merge(&dir, "feature").unwrap();

        assert_eq!(fs::read_to_string(dir.join("file.txt")).unwrap(), "feature\n");
    }

    #[test]
    fn checkout_then_merge_lands_on_the_target() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        init_repo(&dir);
        let base = branches(&dir).unwrap().current.unwrap();
        create_branch(&dir, "target").unwrap();
        commit_file(&dir, "base moved\n", "base work");

        switch_branch(&dir, "target").unwrap();
        merge(&dir, &base).unwrap();

        assert_eq!(branches(&dir).unwrap().current.as_deref(), Some("target"));
        assert_eq!(fs::read_to_string(dir.join("file.txt")).unwrap(), "base moved\n");
    }

    #[test]
    fn checkout_blocked_by_uncommitted_changes_is_refused() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        init_repo(&dir);
        let base = branches(&dir).unwrap().current.unwrap();
        create_branch(&dir, "other").unwrap();
        switch_branch(&dir, "other").unwrap();
        commit_file(&dir, "other\n", "other work");
        switch_branch(&dir, &base).unwrap();
        fs::write(dir.join("file.txt"), "uncommitted\n").unwrap();

        assert!(switch_branch(&dir, "other").is_err());
        assert_eq!(branches(&dir).unwrap().current, Some(base));
    }

    #[test]
    fn conflicting_merge_fails_with_gits_message_and_leaves_the_conflict() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        init_repo(&dir);
        let base = branches(&dir).unwrap().current.unwrap();
        create_branch(&dir, "theirs").unwrap();
        commit_file(&dir, "ours\n", "ours");
        switch_branch(&dir, "theirs").unwrap();
        commit_file(&dir, "theirs\n", "theirs");
        switch_branch(&dir, &base).unwrap();

        let err = merge(&dir, "theirs").unwrap_err();

        assert!(err.to_lowercase().contains("conflict"), "got: {err}");
        let st = status(&dir, &[]).unwrap();
        assert!(st.find("file.txt").is_some());
    }

    #[test]
    fn remote_branches_lists_origin_branches_without_head() {
        if git_missing() {
            return;
        }
        let src = tempdir();
        init_repo(&src);
        let remote = tempdir();
        assert!(run(&remote, &["init", "--bare"]).is_ok());
        let remote_path = remote.to_string_lossy().into_owned();
        assert!(run(&src, &["remote", "add", "origin", &remote_path]).is_ok());
        assert!(run(&src, &["push", "origin", "HEAD"]).is_ok());
        let base = branches(&src).unwrap().current.unwrap();
        let clone = tempdir();
        assert!(run(&clone, &["clone", &remote_path, "."]).is_ok());

        let remotes = remote_branches(&clone).unwrap();

        assert!(remotes.contains(&format!("origin/{base}")), "got: {remotes:?}");
        assert!(remotes.iter().all(|r| r.contains('/') && !r.ends_with("/HEAD")), "got: {remotes:?}");
        assert_eq!(branches(&clone).unwrap().remotes, remotes);
    }
```

- [x] Step 6: In `jobs.rs`, add `Merge` as the last variant of `MutateKind`. Add to `Mutation`, after `DeleteBranch`:

```rust
    /// Brings `source` into `target`: checks out `target` first when
    /// `switch`, fetches every remote first when `fetch` (a pull), then
    /// merges. Stops at the first step that fails.
    Merge {
        target: String,
        source: String,
        fetch: bool,
        switch: bool,
    },
```

In `kind`: `Mutation::Merge { .. } => MutateKind::Merge,`

In `success_message`:

```rust
            Mutation::Merge {
                target,
                source,
                fetch: true,
                ..
            } => format!("pulled {source} into {target}"),
            Mutation::Merge {
                target,
                source,
                fetch: false,
                ..
            } => format!("merged {source} into {target}"),
```

In `run`:

```rust
            Mutation::Merge {
                target,
                source,
                fetch,
                switch,
            } => {
                if *switch {
                    cli::switch_branch(root, target)?;
                }
                if *fetch {
                    cli::fetch_all(root)?;
                }
                cli::merge(root, source)
            }
```

Update the `JobResult::Mutate` doc comment's list of mutations to include a pull or merge.

- [x] Step 7: In `jobs.rs` tests module, add:

```rust
    /// A pull fetches a commit another clone pushed, then merges it.
    #[test]
    fn merge_mutation_with_fetch_pulls_a_pushed_commit() {
        use crate::app::test_support::{git_available, git_output, project_fixture, tempdir};
        if !git_available() {
            return;
        }
        let local = project_fixture();
        let remote = tempdir();
        git_output(&remote, &["init", "--bare"]);
        let remote_path = remote.to_string_lossy().into_owned();
        git_output(&local, &["remote", "add", "origin", &remote_path]);
        git_output(&local, &["push", "origin", "apic-test"]);
        let other = tempdir();
        git_output(&other, &["clone", "-b", "apic-test", &remote_path, "."]);
        git_output(&other, &["config", "user.email", "test@example.com"]);
        git_output(&other, &["config", "user.name", "Test User"]);
        std::fs::write(other.join("pushed.txt"), "pushed\n").unwrap();
        git_output(&other, &["add", "--", "pushed.txt"]);
        git_output(&other, &["commit", "-m", "pushed"]);
        git_output(&other, &["push", "origin", "apic-test"]);

        Mutation::Merge {
            target: "apic-test".into(),
            source: "origin/apic-test".into(),
            fetch: true,
            switch: false,
        }
        .run(&local)
        .unwrap();

        assert!(local.join("pushed.txt").exists());
    }
```

- [x] Step 8: In `state.rs`, after the `GitAction` enum, add:

```rust
/// Which branch operation the pull/merge popup runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BranchOpKind {
    /// Fetch every remote, then merge a remote branch.
    Pull,

    /// Merge a local or remote branch, no fetch.
    Merge,
}

/// A pull or merge awaiting confirmation: bring `source` into `target`.
/// `target` is the right-clicked branch and never changes. `source` is empty
/// until the popup picks one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BranchOp {
    pub(crate) kind: BranchOpKind,
    pub(crate) target: String,
    pub(crate) source: String,
}
```

Add two `GitAction` variants after `ConfirmBranchDelete`:

```rust
    /// Open the pull or merge popup for this branch.
    RequestBranchOp { kind: BranchOpKind, target: String },

    /// The pending pull or merge was confirmed, run it.
    ConfirmBranchOp,
```

Add a `GitState` field after `pending_branch_delete`:

```rust
    /// The pull or merge awaiting confirmation, when its popup is open.
    pub(crate) pending_branch_op: Option<BranchOp>,
```

Replace the `pending_branch_delete` doc comment with: `/// The right-clicked branch a delete would remove, while its confirmation is open.`

- [x] Step 9: In `state.rs` `handle`, after the `ConfirmBranchDelete` arm, add:

```rust
            GitAction::RequestBranchOp { kind, target } => {
                self.pending_branch_op = Some(BranchOp {
                    kind,
                    target,
                    source: String::new(),
                })
            }
            GitAction::ConfirmBranchOp => {
                if let Some(op) = self.pending_branch_op.take()
                    && !op.source.is_empty()
                {
                    let switch = self.branches.current.as_deref() != Some(op.target.as_str());
                    self.start_mutation(
                        paths,
                        ctx,
                        Mutation::Merge {
                            target: op.target,
                            source: op.source,
                            fetch: op.kind == BranchOpKind::Pull,
                            switch,
                        },
                    );
                }
            }
```

- [x] Step 10: In `state.rs` `apply_result`, change the success arm `MutateKind::SwitchBranch => {` to `MutateKind::SwitchBranch | MutateKind::Merge => {` (a merge rewrites files, so the open contract is reconciled like after a switch). Replace the error arm:

```rust
            JobResult::Mutate(kind, _, Err(e)) => {
                self.error = e.clone();
                events.push(GitEvent::Status(e));
                if matches!(kind, MutateKind::Merge) {
                    // A failed merge may already have switched branch or
                    // left conflicts behind: reread both so the resolver
                    // shows them, and reconcile the open contract.
                    self.needs_branches = true;
                    self.needs_refresh = true;
                    events.push(GitEvent::BranchSwitched);
                }
            }
```

- [x] Step 11: In `state.rs` tests module, add:

```rust
    #[test]
    fn confirm_merge_into_another_branch_switches_to_it() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        let (mut git, paths) = git_at(&root);
        let ctx = egui::Context::default();
        git.branches.current = Some("apic-test".into());

        git.handle(
            GitAction::RequestBranchOp {
                kind: BranchOpKind::Merge,
                target: "apic-second".into(),
            },
            Some(&paths),
            &ctx,
        );
        git.pending_branch_op.as_mut().unwrap().source = "apic-test".into();
        git.handle(GitAction::ConfirmBranchOp, Some(&paths), &ctx);
        let events = settle_git(&mut git, &paths, &ctx);

        assert_eq!(current_branch(&root), "apic-second");
        assert!(events.contains(&GitEvent::BranchSwitched), "{events:?}");
        assert!(git.pending_branch_op.is_none());
    }

    #[test]
    fn confirm_with_no_source_runs_nothing() {
        let mut git = GitState::default();
        let ctx = egui::Context::default();
        git.pending_branch_op = Some(BranchOp {
            kind: BranchOpKind::Pull,
            target: "main".into(),
            source: String::new(),
        });
        git.handle(GitAction::ConfirmBranchOp, None, &ctx);
        assert!(git.pending.is_none());
        assert!(git.pending_branch_op.is_none());
    }

    #[test]
    fn failed_merge_rereads_status_and_branches() {
        let mut git = GitState::default();
        let mut events = Vec::new();
        git.apply_result(
            JobResult::Mutate(MutateKind::Merge, String::new(), Err("conflict".into())),
            &mut events,
        );
        assert!(git.needs_branches && git.needs_refresh);
        assert_eq!(git.error, "conflict");
        assert!(events.contains(&GitEvent::BranchSwitched));
    }
```

- [x] Step 12: In `view/mod.rs` tests module, add `remotes: Vec::new(),` after the `all: ...` line of every `Branches { .. }` struct literal. Find them with `grep -n "Branches {" apic-gui/src/features/git/ui/view/mod.rs`.

- [x] Step 13: Run `cargo fmt --all`.
- [x] Step 14: Run `cargo test -p apic-gui`.
- [x] Step 15: Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] Step 16: Commit: `feat(gui): pull and merge job with remote branch listing`.

### Task 2: right-click menu and popups → verify: `cargo test -p apic-gui` exits 0, `cargo clippy --workspace --all-targets -- -D warnings` exits 0, and `grep -c ComboBox apic-gui/src/features/git/ui/view/branches.rs` prints a count below 2

**Files:**
- Modify: `apic-gui/src/features/git/ui/view/branches.rs` (imports, `branch_menu_items`, `delete_branch_dialog`, new `branch_op_sources` and `branch_op_dialog`, tests module)
- Modify: `apic-gui/src/features/git/ui/view/mod.rs` (`use self::branches::...` line, `dialogs`, tests module)

- [ ] Step 1: In `branches.rs`, change the imports to:

```rust
use crate::features::git::domain::status::{Branches, DiscardTarget, discard_target};
use crate::features::git::ui::state::{BranchOp, BranchOpKind, GitAction, GitState};
```

Drop `BranchOp` from the import if clippy reports it unused.

- [ ] Step 2: Replace `branch_menu_items` and its doc comment with:

```rust
/// The menu entries for one branch: pull into it, merge into it, and delete
/// it. Delete is enabled unless it is the current branch, which git refuses
/// to delete.
fn branch_menu_items(name: &str, current: Option<&str>) -> Vec<(&'static str, bool, GitAction)> {
    vec![
        (
            "Pull",
            true,
            GitAction::RequestBranchOp {
                kind: BranchOpKind::Pull,
                target: name.to_string(),
            },
        ),
        (
            "Merge",
            true,
            GitAction::RequestBranchOp {
                kind: BranchOpKind::Merge,
                target: name.to_string(),
            },
        ),
        (
            "Delete",
            Some(name) != current,
            GitAction::RequestBranchDelete {
                name: name.to_string(),
            },
        ),
    ]
}
```

- [ ] Step 3: Replace `delete_branch_dialog` and its doc comment with:

```rust
/// Delete confirmation for the right-clicked branch. The target is fixed:
/// the dialog names it and offers no picker.
pub(super) fn delete_branch_dialog(ctx: &egui::Context, state: &mut GitState) -> Option<GitAction> {
    let p = palette(ctx);
    let name = state.pending_branch_delete.clone()?;
    let outcome = dialog(
        ctx,
        "delete_branch_modal",
        "Delete branch",
        |ui| {
            ui.label(RichText::new("Delete branch").color(p.on_surface_variant));
            ui.add_space(SPACE_SMALL);
            detail_box(ui, |ui| {
                ui.label(RichText::new(&name).color(p.on_surface).strong());
            });
        },
        &[
            DialogButton::new("Cancel", ButtonKind::Secondary),
            DialogButton::new("Delete", ButtonKind::Danger),
        ],
    );
    if outcome.clicked == Some(1) {
        Some(GitAction::ConfirmBranchDelete)
    } else if outcome.clicked == Some(0) || outcome.closed {
        state.pending_branch_delete = None;
        None
    } else {
        None
    }
}
```

- [ ] Step 4: After `delete_branch_dialog`, add:

```rust
/// The branches a pull or merge into `target` may bring in: remote branches
/// for a pull, every other local branch then every remote branch for a merge.
pub(super) fn branch_op_sources(kind: BranchOpKind, target: &str, branches: &Branches) -> Vec<String> {
    match kind {
        BranchOpKind::Pull => branches.remotes.clone(),
        BranchOpKind::Merge => branches
            .all
            .iter()
            .filter(|name| name.as_str() != target)
            .chain(branches.remotes.iter())
            .cloned()
            .collect(),
    }
}

/// The pull or merge popup: the right-clicked branch shown fixed, and a
/// picker for the branch to bring into it. The confirm button is disabled
/// when there is nothing to pick.
pub(super) fn branch_op_dialog(ctx: &egui::Context, state: &mut GitState) -> Option<GitAction> {
    let p = palette(ctx);
    let op = state.pending_branch_op.as_ref()?;
    let sources = branch_op_sources(op.kind, &op.target, &state.branches);
    let (title, empty) = match op.kind {
        BranchOpKind::Pull => ("Pull", "no branches to pull from"),
        BranchOpKind::Merge => ("Merge", "no branches to merge"),
    };
    let op = state.pending_branch_op.as_mut().expect("checked above");
    if !sources.contains(&op.source) {
        op.source = sources.first().cloned().unwrap_or_default();
    }
    let target = op.target.clone();
    let outcome = dialog(
        ctx,
        "branch_op_modal",
        title,
        |ui| {
            ui.label(RichText::new("into branch").color(p.on_surface_variant));
            ui.add_space(SPACE_SMALL);
            detail_box(ui, |ui| {
                ui.label(RichText::new(&target).color(p.on_surface).strong());
            });
            ui.add_space(SPACE_SMALL);
            ui.label(RichText::new("from").color(p.on_surface_variant));
            if sources.is_empty() {
                ui.label(RichText::new(empty).color(p.on_surface_variant));
            } else {
                egui::ComboBox::from_id_salt("branch_op_combo")
                    .selected_text(op.source.clone())
                    .show_ui(ui, |ui| {
                        for name in &sources {
                            ui.selectable_value(&mut op.source, name.clone(), name);
                        }
                    });
            }
        },
        &[
            DialogButton::new("Cancel", ButtonKind::Secondary),
            DialogButton {
                label: title,
                kind: ButtonKind::Primary,
                enabled: !sources.is_empty(),
            },
        ],
    );
    if outcome.clicked == Some(1) {
        Some(GitAction::ConfirmBranchOp)
    } else if outcome.clicked == Some(0) || outcome.closed {
        state.pending_branch_op = None;
        None
    } else {
        None
    }
}
```

- [ ] Step 5: In `branches.rs` tests module, replace `branch_menu_offers_delete_for_other_branches_only` with:

```rust
    #[test]
    fn branch_menu_offers_pull_merge_and_delete() {
        let other = branch_menu_items("feature", Some("main"));
        let labels: Vec<&str> = other.iter().map(|(l, _, _)| *l).collect();
        assert_eq!(labels, ["Pull", "Merge", "Delete"]);
        assert!(other.iter().all(|(_, enabled, _)| *enabled));
        assert!(other.iter().any(|(_, _, a)| matches!(
            a,
            GitAction::RequestBranchOp { kind: BranchOpKind::Pull, target } if target == "feature"
        )));
        assert!(other.iter().any(|(_, _, a)| matches!(
            a,
            GitAction::RequestBranchOp { kind: BranchOpKind::Merge, target } if target == "feature"
        )));
        assert!(other.iter().any(|(_, _, a)| matches!(
            a,
            GitAction::RequestBranchDelete { name } if name == "feature"
        )));
    }

    #[test]
    fn delete_is_disabled_on_the_current_branch_only() {
        let current = branch_menu_items("main", Some("main"));
        for (label, enabled, _) in current {
            assert_eq!(enabled, label != "Delete", "{label}");
        }
    }

    #[test]
    fn pull_sources_are_remote_branches() {
        let b = Branches {
            current: Some("main".into()),
            all: vec!["main".into(), "feature".into()],
            remotes: vec!["origin/main".into()],
        };
        assert_eq!(branch_op_sources(BranchOpKind::Pull, "main", &b), ["origin/main"]);
    }

    #[test]
    fn merge_sources_are_other_locals_then_remotes() {
        let b = Branches {
            current: Some("main".into()),
            all: vec!["main".into(), "feature".into()],
            remotes: vec!["origin/main".into()],
        };
        assert_eq!(
            branch_op_sources(BranchOpKind::Merge, "main", &b),
            ["feature", "origin/main"]
        );
    }
```

- [ ] Step 6: In `view/mod.rs`, change the import to `use self::branches::{branch_op_dialog, create_branch_dialog, delete_branch_dialog, discard_dialog};` and in `dialogs`, after the `delete_branch_dialog` block, add:

```rust
    if let Some(a) = branch_op_dialog(ctx, state) {
        action = Some(a);
    }
```

- [ ] Step 7: In `view/mod.rs` tests module, after `sidebar_renders_the_delete_branch_confirmation_without_panicking`, add:

```rust
    #[test]
    fn branch_op_popup_renders_and_picks_a_source() {
        for preference in BOTH_THEMES {
            for kind in [BranchOpKind::Pull, BranchOpKind::Merge] {
                let mut state = GitState {
                    branches: Branches {
                        current: Some("main".into()),
                        all: vec!["main".into(), "feature".into()],
                        remotes: vec!["origin/main".into()],
                    },
                    pending_branch_op: Some(BranchOp {
                        kind,
                        target: "main".into(),
                        source: String::new(),
                    }),
                    ..GitState::default()
                };
                run_in_theme(preference, |ui| {
                    dialogs(ui.ctx(), &mut state);
                });
                let op = state.pending_branch_op.as_ref().expect("stays open");
                assert!(!op.source.is_empty(), "a source is preselected");
            }
        }
    }
```

Add `BranchOp, BranchOpKind` to the tests module's imports from `crate::features::git::ui::state` (read the module's `use` lines first and extend the existing one).

- [ ] Step 8: Run `cargo fmt --all`.
- [ ] Step 9: Run `cargo test -p apic-gui`.
- [ ] Step 10: Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] Step 11: Commit: `feat(gui): pull, merge and delete in the branch right-click menu`.

### Task 3: unsaved-edits guard for pull and merge → verify: `cargo test -p apic-gui` exits 0 and `cargo clippy --workspace --all-targets -- -D warnings` exits 0

**Files:**
- Modify: `apic-gui/src/app/mod.rs` (the `Action::Git(GitAction::SwitchBranch { .. }) if self.contracts.has_unsaved_edits()` arm in `apply`, tests module)

- [ ] Step 1: Replace the arm that starts `Action::Git(GitAction::SwitchBranch { .. }) if self.contracts.has_unsaved_edits() => {` with:

```rust
            Action::Git(action @ (GitAction::SwitchBranch { .. } | GitAction::ConfirmBranchOp))
                if self.contracts.has_unsaved_edits() =>
            {
                let open = self
                    .contracts
                    .path
                    .as_deref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "the open contract".to_string());
                let what = match (&action, &self.git.pending_branch_op) {
                    (GitAction::ConfirmBranchOp, Some(op)) if op.kind == BranchOpKind::Pull => "pull",
                    (GitAction::ConfirmBranchOp, _) => "merge",
                    _ => "switch branch",
                };
                self.git.pending_branch_op = None;
                let msg = format!("cannot {what}, {open} has unsaved edits");
                self.git.error = msg.clone();
                self.shell.status = msg;
            }
```

Add `BranchOpKind` to the existing import of `GitAction` in `app/mod.rs` (read the `use` lines first).

- [ ] Step 2: In `app/mod.rs` tests module, after `switch_branch_is_refused_with_unsaved_edits`, add a test that copies that test's setup (fixture, `app_at`, `reload_project`, `settle`, load `sample.json`, `ToggleEdit`, rename the model to `dirtied`), records `let head = git_output(&root, &["rev-parse", "HEAD"]);`, then:

```rust
        app.git.pending_branch_op = Some(BranchOp {
            kind: BranchOpKind::Merge,
            target: "apic-test".into(),
            source: "apic-second".into(),
        });
        app.apply(Action::Git(GitAction::ConfirmBranchOp), &ctx);
        settle(&mut app, &ctx);

        assert_eq!(current_branch(&root), "apic-test");
        assert_eq!(git_output(&root, &["rev-parse", "HEAD"]), head);
        assert!(!app.git.error.is_empty());
        assert!(app.git.pending_branch_op.is_none());
```

Name it `merge_is_refused_with_unsaved_edits`. Import `BranchOp, BranchOpKind` in the tests module.

- [ ] Step 3: Run `cargo fmt --all`.
- [ ] Step 4: Run `cargo test -p apic-gui`.
- [ ] Step 5: Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] Step 6: Commit: `feat(gui): refuse pull and merge while the open contract has unsaved edits`.
