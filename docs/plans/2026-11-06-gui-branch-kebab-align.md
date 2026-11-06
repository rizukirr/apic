# gui branch kebab align: Implementation Plan

**Spec:** docs/specs/2026-11-06-gui-branch-kebab-align-design.md
**Goal:** Every ⋮ button in the branch dropdown sits in one column at the right edge.
**Architecture:** `branch_menu` measures the widest branch name once, then draws every name as a selectable button with that minimum width, so the ⋮ that follows starts at the same `x` on every row.

## Global constraints

- egui 0.35, no new dependency.
- CI runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`. The task leaves all three passing.
- Run limits: at most 3 verify rounds, at most $0 on paid runs.
- The `view/mod.rs` tests module already has the helpers `menu_frame`, `click_at` and `node` (AccessKit labels and bounds, `node` matches with `starts_with`), used by `kebab_menu_picks_a_pull_source_through_nested_menus`.

### Task 1: fixed-width branch names → verify: `cargo test -p apic-gui` exits 0 and `cargo clippy --workspace --all-targets -- -D warnings` exits 0

**Files:**
- Modify: `apic-gui/src/features/git/ui/view/branches.rs` (`branch_menu`)
- Modify: `apic-gui/src/features/git/ui/view/mod.rs` (tests module)

- [x] Step 1: In `view/mod.rs` tests module, after `kebab_menu_picks_a_pull_source_through_nested_menus`, add:

```rust
    /// The ⋮ buttons line up in one column at the right edge of the
    /// dropdown, whatever the branch name lengths.
    #[test]
    fn kebab_buttons_share_one_column() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut state = GitState {
            branches: Branches {
                current: Some("main".into()),
                all: vec!["main".into(), "feature/a-much-longer-branch-name".into()],
                remotes: Vec::new(),
            },
            branches_loaded: true,
            ..GitState::default()
        };
        let mut action = None;

        let nodes = menu_frame(&ctx, &mut state, vec![], &mut action);
        let title = node(&nodes, egui_phosphor::regular::GIT_BRANCH);
        let nodes = click_at(&ctx, &mut state, title, &mut action);

        let kebabs: Vec<egui::Rect> = nodes
            .iter()
            .filter(|(l, _)| l.starts_with(egui_phosphor::regular::DOTS_THREE_VERTICAL))
            .map(|(_, r)| *r)
            .collect();
        assert!(kebabs.len() >= 2, "one ⋮ per branch: {kebabs:?}");
        let new_branch = node(&nodes, egui_phosphor::regular::PLUS);
        for r in &kebabs {
            assert!((r.min.x - kebabs[0].min.x).abs() < 0.5, "{kebabs:?}");
            assert!(r.max.x <= new_branch.max.x + 0.5, "{r:?} vs {new_branch:?}");
        }
    }
```

- [x] Step 2: Run `cargo test -p apic-gui kebab_buttons_share_one_column` and confirm it fails (the ⋮ buttons start at different `x` today).

- [x] Step 3: In `branches.rs` `branch_menu`, inside the `ui.menu_button` closure and before `for name in &branches.all {`, add:

```rust
        // Every name gets the widest name's width, so the ⋮ buttons that
        // follow line up in one column.
        let font = egui::TextStyle::Button.resolve(ui.style());
        let name_width = branches
            .all
            .iter()
            .map(|name| {
                ui.fonts_mut(|f| {
                    f.layout_no_wrap(name.clone(), font.clone(), egui::Color32::PLACEHOLDER)
                        .size()
                        .x
                })
            })
            .fold(0.0, f32::max)
            + ui.spacing().button_padding.x * 2.0;
```

Then replace the row's first line `let row = ui.selectable_label(Some(name.as_str()) == current.as_deref(), name);` with:

```rust
                let row = ui.add(
                    egui::Button::selectable(Some(name.as_str()) == current.as_deref(), name)
                        .min_size(egui::vec2(name_width, 0.0)),
                );
```

If `ui.fonts_mut` does not exist in this egui version, use `ui.fonts` with the same closure and say so in concerns.

- [x] Step 4: Run `cargo fmt --all`.
- [x] Step 5: Run `cargo test -p apic-gui`.
- [x] Step 6: Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] Step 7: Commit: `fix(gui): line up the branch ⋮ buttons in one column`.
