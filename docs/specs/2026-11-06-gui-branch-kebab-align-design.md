---
title: gui branch kebab align
date: 2026-11-06
status: approved
---

# gui branch kebab align: Design

## Problem

In the branch dropdown, each row is a branch name followed by a ⋮ button (`branch_menu` in `apic-gui/src/features/git/ui/view/branches.rs`). The name is only as wide as its text, so the ⋮ buttons sit at different horizontal positions on each row. The user wants them in one vertical column.

## Goals

1. Every ⋮ button in the open dropdown has the same left edge. Success: a headless test opens the real dropdown with branches of different name lengths and asserts every node labelled with `DOTS_THREE_VERTICAL` has the same `min.x` (within 0.5px). It fails before this change and passes after.
2. Branch names stay left-aligned and clicking a name still switches branch. Success: the existing `kebab_menu_picks_a_pull_source_through_nested_menus` test passes, and the row keeps using `Button::selectable` inside `ui.horizontal`, whose layout aligns its content left.
3. The dropdown does not grow past its content. Success: the same test asserts every ⋮ button's right edge is at most the right edge of the "+ New branch" button plus 0.5px.

## Non-goals

- Changing what the ⋮ menu contains or the confirm popup.
- Truncating long branch names.

## Constraints

- egui 0.35, no new dependency.
- CI runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`.
- Run limits: at most 3 verify rounds, at most $0 on paid runs.

## Approach

Pushback turn: no simpler framing, the user agreed. Before the row loop, `branch_menu` measures the widest branch name with the button text style and adds the button padding on both sides. Each row draws its name as `egui::Button::selectable(..).min_size(vec2(width, 0.0))`, then the ⋮ `SubMenuButton`. A headless prototype outside the repo put both ⋮ buttons at the same `x` and right-aligned them with "+ New branch".

## Alternatives considered

- Stretch each name to `ui.available_width()` minus the ⋮ width. The prototype showed the menu grows to about 1200px because a menu sizes to its content, so it was rejected.

## Testing

A headless test in `view/mod.rs` reusing `menu_frame`, `click_at` and `node`: open the dropdown with branches `main` and `feature/a-much-longer-branch-name`, then assert goals 1 and 3 on the AccessKit bounds.

## Open questions

N/A: every decision was settled in the dialogue.
