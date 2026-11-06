# gui branch kebab menu: Implementation Plan

**Spec:** docs/specs/2026-11-06-gui-branch-kebab-menu-design.md
**Goal:** Replace the broken right-click menu on branch rows with a ⋮ button that opens nested Pull, Merge and Delete menus, the source picked inside the menu, then a confirm popup without a picker.
**Architecture:** `branch_menu` draws each row as a name plus a `SubMenuButton` (tracked by egui's `MenuState`, so the dropdown stays open), whose Pull and Merge sub menus list sources and return `GitAction::RequestBranchOp { kind, target, source }`. `branch_op_dialog` becomes a confirm-only popup. Everything after `ConfirmBranchOp` is unchanged.

## Global constraints

- egui 0.35, eframe 0.35, `egui-phosphor` 0.13, as already in `apic-gui/Cargo.toml`. No new dependency.
- The confirmation uses the shared `dialog` popup layout and theme palette.
- CI runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`. The task leaves all three passing.
- Run limits: at most 3 verify rounds, at most $0 on paid runs.
- A headless prototype (outside the repo) confirmed: inside a `ui.menu_button`, `SubMenuButton` and nested `ui.menu_button` keep the dropdown open, `ui.close()` on a leaf closes the whole chain, egui's AccessKit output (`ctx.enable_accesskit()`, then `FullOutput::platform_output.accesskit_update`) gives each widget's label and bounds, and a sub menu button's label ends with the arrow `⏵`, so tests match labels with `starts_with`.

### Task 1: ⋮ nested menu and confirm-only popup → verify: `cargo test -p apic-gui` exits 0, `cargo clippy --workspace --all-targets -- -D warnings` exits 0, `grep -c context_menu apic-gui/src/features/git/ui/view/branches.rs` prints 0, and `grep -c ComboBox apic-gui/src/features/git/ui/view/branches.rs` prints 0

**Files:**
- Modify: `apic-gui/src/features/git/ui/state.rs` (`GitAction::RequestBranchOp`, `BranchOp` doc comment, `handle`, the state test that sends `RequestBranchOp`)
- Modify: `apic-gui/src/features/git/ui/view/branches.rs` (imports, `branch_menu_items`, `branch_menu`, `branch_op_dialog`, tests module)
- Modify: `apic-gui/src/features/git/ui/view/mod.rs` (tests module)

- [ ] Step 1: In `state.rs`, change the variant and its doc comment to:

```rust
    /// Open the confirmation for pulling or merging `source` into `target`,
    /// both picked in the branch menu.
    RequestBranchOp {
        kind: BranchOpKind,
        target: String,
        source: String,
    },
```

Change the `BranchOp` doc comment to: `/// A pull or merge awaiting confirmation: bring \`source\` into \`target\`, both picked in the branch menu.`

Change the `handle` arm to:

```rust
            GitAction::RequestBranchOp {
                kind,
                target,
                source,
            } => {
                self.pending_branch_op = Some(BranchOp {
                    kind,
                    target,
                    source,
                })
            }
```

In the state test `confirm_merge_into_another_branch_switches_to_it`, add `source: "apic-test".into(),` to its `GitAction::RequestBranchOp { .. }` literal and delete the following line `git.pending_branch_op.as_mut().unwrap().source = "apic-test".into();`. Find the literal with `grep -n "RequestBranchOp" apic-gui/src/features/git/ui/state.rs`.

- [ ] Step 2: In `branches.rs`, replace `branch_menu_items` and its doc comment with:

```rust
/// Whether the menu may offer deleting `name`: not the current branch, which
/// git refuses to delete.
fn can_delete(name: &str, current: Option<&str>) -> bool {
    Some(name) != current
}

/// The menu and popup title for a branch operation.
fn op_label(kind: BranchOpKind) -> &'static str {
    match kind {
        BranchOpKind::Pull => "Pull",
        BranchOpKind::Merge => "Merge",
    }
}
```

- [ ] Step 3: In `branches.rs` `branch_menu`, replace `let branches = state.branches.all.clone();` with `let branches = state.branches.clone();`, and replace the whole `for name in &branches { .. }` loop inside the `ui.menu_button` closure with:

```rust
        for name in &branches.all {
            ui.horizontal(|ui| {
                let row = ui.selectable_label(Some(name.as_str()) == current.as_deref(), name);
                if row.clicked() {
                    if let Some(a) = branch_select_action(name, current.as_deref()) {
                        action = Some(a);
                    }
                    ui.close();
                }
                SubMenuButton::from_button(egui::Button::new(
                    egui_phosphor::regular::DOTS_THREE_VERTICAL,
                ))
                .ui(ui, |ui| {
                    for kind in [BranchOpKind::Pull, BranchOpKind::Merge] {
                        ui.menu_button(op_label(kind), |ui| {
                            let sources = branch_op_sources(kind, name, &branches);
                            if sources.is_empty() {
                                ui.add_enabled(false, egui::Button::new("no branches"));
                            }
                            for source in sources {
                                if ui.button(&source).clicked() {
                                    action = Some(GitAction::RequestBranchOp {
                                        kind,
                                        target: name.clone(),
                                        source,
                                    });
                                    ui.close();
                                }
                            }
                        });
                    }
                    if ui
                        .add_enabled(
                            can_delete(name, current.as_deref()),
                            egui::Button::new("Delete"),
                        )
                        .clicked()
                    {
                        action = Some(GitAction::RequestBranchDelete { name: name.clone() });
                        ui.close();
                    }
                });
            });
        }
```

Add `use egui::containers::menu::SubMenuButton;` to the imports. Update `branch_menu`'s doc comment: replace `(click to switch, right-click for its menu)` with `(click the name to switch, ⋮ for pull, merge and delete)`.

- [ ] Step 4: In `branches.rs`, replace `branch_op_dialog` and its doc comment with:

```rust
/// The pull or merge confirmation: names the source and the target branch,
/// both picked in the branch menu.
pub(super) fn branch_op_dialog(ctx: &egui::Context, state: &mut GitState) -> Option<GitAction> {
    let p = palette(ctx);
    let op = state.pending_branch_op.clone()?;
    let title = op_label(op.kind);
    let outcome = dialog(
        ctx,
        "branch_op_modal",
        title,
        |ui| {
            ui.label(RichText::new("from").color(p.on_surface_variant));
            ui.add_space(SPACE_SMALL);
            detail_box(ui, |ui| {
                ui.label(RichText::new(&op.source).color(p.on_surface).strong());
            });
            ui.add_space(SPACE_SMALL);
            ui.label(RichText::new("into branch").color(p.on_surface_variant));
            ui.add_space(SPACE_SMALL);
            detail_box(ui, |ui| {
                ui.label(RichText::new(&op.target).color(p.on_surface).strong());
            });
        },
        &[
            DialogButton::new("Cancel", ButtonKind::Secondary),
            DialogButton::new(title, ButtonKind::Primary),
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

- [ ] Step 5: In the `branches.rs` tests module, delete the tests `branch_menu_offers_pull_merge_and_delete` and `delete_is_disabled_on_the_current_branch_only` (they test the removed `branch_menu_items`) and add:

```rust
    #[test]
    fn delete_is_offered_off_the_current_branch_only() {
        assert!(can_delete("feature", Some("main")));
        assert!(!can_delete("main", Some("main")));
        assert!(can_delete("main", None));
    }
```

- [ ] Step 6: In the `view/mod.rs` tests module, replace the test `branch_op_popup_renders_and_picks_a_source` with:

```rust
    #[test]
    fn branch_op_popup_renders_in_both_themes() {
        for preference in BOTH_THEMES {
            for kind in [BranchOpKind::Pull, BranchOpKind::Merge] {
                let mut state = GitState {
                    pending_branch_op: Some(BranchOp {
                        kind,
                        target: "main".into(),
                        source: "origin/main".into(),
                    }),
                    ..GitState::default()
                };
                run_in_theme(preference, |ui| {
                    dialogs(ui.ctx(), &mut state);
                });
                // The popup stays open until the user confirms or cancels.
                assert!(state.pending_branch_op.is_some());
            }
        }
    }

    /// One headless frame of `branch_menu` fed `events`. Stores any action
    /// it returns into `action` and returns every AccessKit node that has a
    /// label, with its bounds.
    fn menu_frame(
        ctx: &egui::Context,
        state: &mut GitState,
        events: Vec<egui::Event>,
        action: &mut Option<GitAction>,
    ) -> Vec<(String, egui::Rect)> {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            events,
            ..Default::default()
        };
        let out = ctx.run_ui(input, |ui| {
            if let Some(a) = branch_menu(ui, state) {
                *action = Some(a);
            }
        });
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

    /// Moves the pointer to the centre of `rect` and clicks there, then runs
    /// two idle frames. Returns the nodes from the last frame.
    fn click_at(
        ctx: &egui::Context,
        state: &mut GitState,
        rect: egui::Rect,
        action: &mut Option<GitAction>,
    ) -> Vec<(String, egui::Rect)> {
        let pos = rect.center();
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let mut nodes = Vec::new();
        for events in [
            vec![egui::Event::PointerMoved(pos)],
            vec![button(true)],
            vec![button(false)],
            vec![],
            vec![],
        ] {
            nodes = menu_frame(ctx, state, events, action);
        }
        nodes
    }

    /// The bounds of the first node whose label starts with `label`. Sub
    /// menu buttons carry a trailing arrow, hence `starts_with`.
    fn node(nodes: &[(String, egui::Rect)], label: &str) -> egui::Rect {
        nodes
            .iter()
            .find(|(l, _)| l.starts_with(label))
            .map(|(_, r)| *r)
            .unwrap_or_else(|| {
                let labels: Vec<&String> = nodes.iter().map(|(l, _)| l).collect();
                panic!("no node {label:?} in {labels:?}")
            })
    }

    /// Drives the real dropdown with pointer events: open it, click ⋮ on a
    /// non-current row, open Pull, click a remote branch. Guards against the
    /// right-click menu bug, where the dropdown closed under the menu.
    #[test]
    fn kebab_menu_picks_a_pull_source_through_nested_menus() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut state = GitState {
            branches: Branches {
                current: Some("main".into()),
                all: vec!["main".into(), "feature".into()],
                remotes: vec!["origin/main".into()],
            },
            branches_loaded: true,
            ..GitState::default()
        };
        let mut action = None;

        let nodes = menu_frame(&ctx, &mut state, vec![], &mut action);
        let title = node(&nodes, egui_phosphor::regular::GIT_BRANCH);
        let nodes = click_at(&ctx, &mut state, title, &mut action);
        let row = node(&nodes, "feature");
        let kebab = nodes
            .iter()
            .find(|(l, r)| {
                l.starts_with(egui_phosphor::regular::DOTS_THREE_VERTICAL)
                    && (r.center().y - row.center().y).abs() < 2.0
            })
            .map(|(_, r)| *r)
            .expect("the feature row has a ⋮ button");
        let nodes = click_at(&ctx, &mut state, kebab, &mut action);
        let pull = node(&nodes, "Pull");
        let nodes = click_at(&ctx, &mut state, pull, &mut action);
        let source = node(&nodes, "origin/main");
        click_at(&ctx, &mut state, source, &mut action);

        state.handle(action.expect("picking a source returns an action"), None, &ctx);
        assert_eq!(
            state.pending_branch_op,
            Some(BranchOp {
                kind: BranchOpKind::Pull,
                target: "feature".into(),
                source: "origin/main".into(),
            })
        );
    }
```

If `GitAction` must be imported in the tests module, extend the existing `use crate::features::git::ui::state::{BranchOp, BranchOpKind};` line.

- [ ] Step 7: Run `cargo fmt --all`.
- [ ] Step 8: Run `cargo test -p apic-gui`.
- [ ] Step 9: Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] Step 10: Commit: `fix(gui): branch actions behind a ⋮ menu, source picked in the menu`.
