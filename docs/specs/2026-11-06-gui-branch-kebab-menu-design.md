---
title: gui branch kebab menu
date: 2026-11-06
status: approved
---

# gui branch kebab menu: Design

## Problem

The branch actions from `docs/specs/2026-11-06-gui-branch-actions-design.md` open from a right-click on a branch row inside the app-bar branch dropdown. The user reported: "i cannot right click, nothing show when right click". A headless egui 0.35 repro shows the context menu drawn for one frame, then both it and the dropdown gone. Two egui behaviours close the parent `ui.menu_button`: its default `CloseOnClick` treats the secondary click as a closing click (`egui-0.35.0/src/containers/popup.rs:594-601`), and `Response::context_menu` opens a Memory popup that overwrites the single per-viewport popup slot the parent lives in (`egui-0.35.0/src/memory/mod.rs:1096-1099`). The same nesting exists at `d22e4cb`, so the old Delete-only right-click was broken too.

The pull and merge flow also asks the user to close the menu and pick the source again in a popup picker. The user wants the source chosen inside the menu.

## Goals

1. Each branch row in the dropdown shows the branch name and a ⋮ button (`egui_phosphor::regular::DOTS_THREE_VERTICAL`). Clicking the name still switches branch as before. Success: the headless interaction test below finds and clicks the ⋮ button by its label.
2. Clicking ⋮ opens a nested menu with Pull, Merge and Delete, drawn as egui sub menus of the dropdown (`ui.menu_button` inside the menu), so the dropdown stays open while they are open. No `context_menu` remains in `branches.rs`. Success: `grep -c context_menu apic-gui/src/features/git/ui/view/branches.rs` prints 0, and the interaction test reaches a source item through the nested menus.
3. Pull opens a further sub menu listing remote branches (`remote/branch`). Merge opens one listing every local branch except the row's branch, then every remote branch. An empty list shows one disabled line, "no branches". Success: unit tests on the list builder and the existing `branch_op_sources` tests pass.
4. Clicking a source closes the menu and opens a confirmation popup that names both branches, titled Pull or Merge, with Cancel and a primary button labelled Pull or Merge. The popup has no picker. Confirming runs the existing job (checkout when needed, fetch for Pull, then merge). Success: `grep -c ComboBox apic-gui/src/features/git/ui/view/branches.rs` prints 0, and the interaction test ends with `pending_branch_op` set to the clicked kind, row branch and source.
5. Delete in the ⋮ menu opens the existing fixed-target delete confirmation, and is disabled on the current branch. Success: a unit or interaction test shows Delete disabled on the current branch and enabled on another.
6. A headless interaction test drives the real `branch_menu` with synthetic pointer events: open the dropdown, click ⋮ on a non-current row, open Pull, click `origin/main`, and asserts `pending_branch_op`. Success: the test passes at HEAD and fails at the commit before this change.

## Non-goals

- Right-click menus anywhere.
- Changing the job, its error handling, the unsaved-edits guard, or the remote listing from the earlier spec.
- Keyboard navigation of the nested menus beyond what egui gives by default.

## Constraints

- egui 0.35, eframe 0.35, `egui-phosphor` 0.13, as already in `apic-gui/Cargo.toml`. No new dependency.
- The confirmation uses the shared `dialog` popup layout and theme palette.
- CI runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`.
- Run limits: at most 3 verify rounds, at most $0 on paid runs.

## Approach

Pushback turn: no simpler framing, the user agreed. The user chose: ⋮ button instead of right-click, source picked inside nested menus, then a confirm popup without a picker (option b over running immediately).

`branch_menu` draws each row as `ui.horizontal` holding the selectable name and a `ui.menu_button(DOTS_THREE_VERTICAL, ..)`. Inside a menu, `ui.menu_button` becomes a `SubMenuButton` tracked by `MenuState`, not the Memory popup slot, so the dropdown stays open. The ⋮ sub menu holds `ui.menu_button("Pull", ..)` and `ui.menu_button("Merge", ..)` sub menus listing `branch_op_sources(kind, row, &branches)`, and a Delete button. Clicking a source returns `GitAction::RequestBranchOp { kind, target, source }` (the action gains `source`) and calls `ui.close()`. `branch_op_dialog` drops its ComboBox and source fallback and shows "into" the target and "from" the source in `detail_box`es. `GitState::handle` stores the source from the action. Everything after `ConfirmBranchOp` is unchanged.

## Alternatives considered

- ⋮ opens Pull, Merge and Delete, each going to the existing popup with its picker. Smallest change, but keeps the close-then-pick-again step the user asked to remove.
- Keep right-click with an app-held `open_bool` popup and a `CloseOnClickOutside` parent. Shown working in a repro, but the user chose the ⋮ button.

## Testing

- Headless interaction test (goal 6) in `view/mod.rs` tests: render `branch_menu` in a plain `egui::Context` frame loop with `RawInput` pointer events, locating widgets by label and bounds from egui's AccessKit output (`ctx.enable_accesskit()`, the `accesskit` feature is already enabled through eframe), so no test dependency is added.
- Unit tests in `branches.rs`: `branch_op_sources` (existing), Delete enabled only off the current branch.
- Render test: the confirm popup in both themes with a pending op, staying open on a bare render.
- `state.rs` tests updated for the new `RequestBranchOp` field.

## Open questions

N/A: every decision was settled in the dialogue.
