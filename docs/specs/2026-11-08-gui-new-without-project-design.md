---
title: gui new without project
date: 2026-11-08
status: approved
---

# gui new without project: Design

## Problem

With no folder open, the sidebar still shows "+ New request" (`apic-gui/src/features/contracts/ui/view/sidebar.rs:69`) and the Templates section's "+ New template". Both open their dialog, but Create finds no contracts root and only sets the status line to "no project" (`apic-gui/src/features/contracts/ui/state.rs:724-729` and `:766`). The dialog closes and the typed name is lost.

## Goals

1. With no project open, "+ New request" opens the native folder picker instead of the New request dialog, and "+ New template" does the same instead of the New template dialog. Success: a unit test of the routing function shows `OpenNewRequest` and `OpenNewTemplate` become `ProjectAction::PickForNew(Request)` and `PickForNew(Template)` when no project is open, and pass through unchanged when one is.
2. Picking a folder that is not a project initializes it (`.apic/config.toml` created through `data::init`), and picking an existing project leaves its config untouched. Either way the folder becomes the active project. Success: unit tests of `finish_new_for` on a plain temp folder and on a project fixture check the config file and `project_root`.
3. After the pick, the matching dialog opens with an empty name and the caret in its input, exactly as with a project open. Success: `finish_new_for` returns `[Opened(folder), OpenNewDialog(kind)]` in that order, and an app test applying those events on a project fixture ends with `contracts.new_request == Some(String::new())` (and `contracts.new_template == Some(String::new())` for Template).
4. Cancelling the picker opens no dialog and leaves no project open. Success: a project state test sets `pending` to `PickKind::NewTarget(Request)` with a channel that has `None` sent into it, calls `poll`, and asserts no `OpenNewDialog` or `Opened` event and `project_root` still `None`. A second test sends a temp folder instead and gets `[Opened, OpenNewDialog(Request)]`, so the pick path is covered without a real dialog.
5. With a project open, both buttons behave as before. Success: the routing test from goal 1, and the existing contracts tests pass.

## Non-goals

- Greying out the buttons.
- The folder-scoped "New request here" row menu, which only exists inside an open project.
- Send, delete or any other action that reports "no project".

## Constraints

- The project feature never reads or writes contracts state. The app shell connects them, as it already does for `Opened` and `ContractsChanged`.
- The picker runs on the existing background thread through `data::pick`, never blocking the UI.
- No new dependency.
- CI runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`.
- Commits carry no `Co-Authored-By` line, and the PR body carries no Claude Code line.
- Run limits: at most 3 verify rounds, at most $0 on paid runs.

## Approach

Pushback turn: no simpler framing, the user agreed. The user chose: open the picker first, then the dialog (option b over greying out), initialize a folder that is not yet a project (option a over refusing), for both requests and templates.

- `features/project/ui/state.rs`: a `NewKind { Request, Template }` enum, `ProjectAction::PickForNew(NewKind)`, `ProjectEvent::OpenNewDialog(NewKind)`, and `finish_new_for(folder, kind)`, which runs the same init-or-open as `finish_new` and appends `OpenNewDialog(kind)`.
- `features/project/data/mod.rs`: `PickKind::NewTarget(NewKind)`, a folder picker titled "Folder for the new request" or "Folder for the new template".
- `app/mod.rs`: a pure routing function applied to contracts actions before `handle_contracts`, turning `OpenNewRequest` and `OpenNewTemplate` into `Action::Project(PickForNew(..))` when `project.project_root` is `None`. `OpenNewDialog(kind)` in the project-event handler calls `handle_contracts` with `OpenNewRequest(String::new())` or `OpenNewTemplate`, after `Opened` has reloaded the project, so the existing focus setup runs.

## Alternatives considered

- With no project, "+" behaves like New: picker, init, open, and the user clicks "+" again. About half the code, but the user asked for the dialog to follow.
- Grey out both buttons until a folder is open. Rejected by the user in favour of the picker.

## Testing

- Project state unit tests for `finish_new_for` on a plain folder and on an existing project.
- App unit test of the routing function in both project states.
- App test applying `[Opened(fixture), OpenNewDialog(Request)]` and `[.., OpenNewDialog(Template)]` through the project-event handler, asserting the dialog state.
- No test starts a real picker: it would open a native dialog on the test machine.

## Open questions

N/A: every decision was settled in the dialogue.
