---
title: gui conflict notice
date: 2026-11-08
status: approved
---

# gui conflict notice: Design

## Problem

When a Pull or Merge from the branch ⋮ menu ends in conflict, the only signals are the status bar error line and the Conflicts group in Changes. Both are easy to miss, so the user does not notice the repository is mid-merge. Conflicts that come from outside the app, such as a `git merge` in a terminal or opening a project that is already mid-merge, are just as quiet.

## Goals

1. When a status update brings a conflicted path that was not conflicted in the previous status, a popup titled "Merge conflict" opens and lists every currently conflicted path.
2. The popup has one OK button. Clicking it or closing the popup dismisses it, and it does not reopen while the conflicted set gains no new path.
3. A Pull or Merge from the ⋮ menu that ends in conflict opens the popup through the status reread that already follows a failed merge.
4. Opening a project whose repository is already mid-merge opens the popup on its first status.
5. A status with no conflicted paths clears any open notice.
6. GUI.md and CHANGELOG.md (Unreleased, Added) describe the popup.

## Non-goals

No Resolve button, no Abort merge button, no navigation to the resolver. No change to how conflicts are detected or resolved. No popup for conflicts outside the repository status.

## Constraints

Detection lives in `GitState::set_status` (apic-gui/src/features/git/ui/state.rs:302), the single choke point for status updates. The popup uses the shared `dialog` and `detail_box` helpers and is drawn from `dialogs()` in apic-gui/src/features/git/ui/view/mod.rs. No change outside the git feature except docs. Run limits: at most 3 verify rounds, at most $0 on paid runs. Commits carry no `Co-Authored-By` line. CI: fmt --check, clippy -D warnings, test --workspace.

## Approach

`GitState` gains `conflict_notice: Option<Vec<String>>`. `set_status` collects the new status's conflicted paths (via `split_conflicts` or `FileStatus.conflicted`). If any of them is absent from the previous status's conflicted paths, it sets `conflict_notice` to the full new list. If the new list is empty, it sets `conflict_notice` to `None`. Otherwise it leaves the notice as it is.

`conflict_notice_dialog(ctx, state)` in view/mod.rs draws the shared `dialog` when the notice is `Some`: the title "Merge conflict", a line saying the files below need resolving in Changes, a `detail_box` with the paths one per line, and an OK button (Primary). A click or a close sets the notice to `None`. It returns no action. `dialogs()` calls it after the existing dialogs.

## Alternatives considered

Detecting in `poll_git` in app/mod.rs, rejected because it moves git status logic into the app shell. Matching "CONFLICT" in the merge error text, rejected because it misses conflicts from outside the app.

## Testing

State unit tests on `set_status`: none to some conflicts sets the notice; same set again after dismissal keeps it `None`; a new extra path reopens it with the full list; empty status clears it. View test: with a notice set, the dialog renders "Merge conflict" and the paths (accesskit), and clicking OK clears it. App test with `conflict_fixture` or a real conflicting merge through the ⋮ action: after settle, `git.conflict_notice` lists the conflicted path.

## Open questions

None.
