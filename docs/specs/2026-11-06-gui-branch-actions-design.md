---
title: gui branch actions
date: 2026-11-06
status: approved
---

# gui branch actions: Design

## Problem

The app-bar branch dropdown (`apic-gui/src/features/git/ui/view/branches.rs`) lets the user switch, create and delete branches. Its right-click menu holds only "Delete branch", and the delete dialog carries a picker that can retarget the delete to a branch other than the one right-clicked. There is no way to pull or merge from the GUI.

## Goals

1. Right-clicking any branch row in the dropdown shows a menu with Pull, Merge and Delete. Success: the menu items test lists all three for a non-current branch.
2. Pull opens a popup showing the right-clicked branch (fixed, not editable) and a picker of remote branches named `remote/branch` (for example `origin/main`). Confirming runs, in one background job: `git checkout <target>` when the target is not the current branch, then `git fetch --all`, then `git merge <source>`. Success: a temp-repo test with a bare remote shows the target branch containing the remote commit after the job.
3. Merge opens the same popup with a picker of every local branch except the target plus every remote branch. Confirming runs the same job without the fetch. Success: a temp-repo test merges a local branch into a branch that is not checked out, and afterwards the current branch is the target and contains the source commit.
4. If the checkout fails (for example uncommitted changes would be overwritten), nothing else runs, git's error reaches the status line, and the repository is unchanged. Success: a temp-repo test with a dirty tree that blocks the checkout returns `Err` and the current branch is unchanged.
5. A conflicting merge leaves the repository mid-merge, puts git's message on the status line, and refreshes status and the branch list so the conflicted files appear in the existing resolver. Success: a temp-repo test of a conflicting merge returns `Err`, and a status read afterwards lists the conflicted path. A state test shows a failed merge mutation schedules a status and branch refresh.
6. Delete always targets the right-clicked branch. Its confirmation popup shows that branch fixed, with no picker. Delete stays disabled in the menu for the current branch, which git refuses to delete. Success: the delete dialog code holds no ComboBox, and the menu items test shows Delete disabled on the current branch.
7. The remote branch list leaves out the symbolic `remote/HEAD` alias. Success: a temp-repo test with a cloned repo lists `origin/main` and no `origin/HEAD`.

## Non-goals

- Push. Not requested.
- Rebase, squash or fast-forward-only pull modes.
- Aborting a merge from the GUI (`git merge --abort`). The existing resolver handles conflicts.
- Stashing before a switch. A blocked switch is reported, never worked around.
- Choosing which remote to fetch. `git fetch --all` fetches every remote.

## Constraints

- Every git call goes through `data/cli.rs`, threading stays in `data/jobs.rs`, as the git feature layers already require.
- One job at a time: the new work runs as one `Mutation`, so it obeys the existing `pending` rule and never contends for `index.lock`.
- `git checkout`, not `git switch`, for git older than 2.23, matching `cli::switch_branch`.
- Uses the shared `dialog` popup layout and theme palette like the other branch dialogs.
- Run limits: at most 3 verify rounds, at most $0 on paid runs

## Approach

Pushback turn: no simpler framing was found, the user confirmed the requirement as stated. The user chose: Pull fetches first then merges, Merge offers local and remote branches named `remote/branch`, a non-current target is checked out first and stays checked out, and a failed checkout aborts with git's own error.

Units:

- `cli.rs` gains `remote_branches(root) -> Result<Vec<String>, String>` (`git for-each-ref --format=%(refname:short) refs/remotes/`, dropping entries ending in `/HEAD` and the bare remote name that `%(refname:short)` prints for `refs/remotes/<remote>/HEAD`), `fetch_all(root)` and `merge(root, source)`. `Branches` in `domain/status.rs` gains `remotes: Vec<String>`, filled by `cli::branches`.
- `jobs.rs` gains `Mutation::Merge { target, source, fetch, switch }` and `MutateKind::Merge`. `run` does checkout when `switch`, then `fetch_all` when `fetch`, then `merge`, stopping at the first `Err`. Success message: `pulled <source> into <target>` or `merged <source> into <target>`.
- `state.rs` gains a `BranchOp { kind: Pull | Merge, target, source }` held in `pending_branch_op: Option<BranchOp>`, and actions `RequestBranchOp { kind, target }` and `ConfirmBranchOp`. Confirm builds the mutation with `switch` set when `target` differs from `branches.current`. `MutateKind::Merge` success sets `needs_branches` and emits `BranchSwitched` when a switch happened. A failed `Merge` sets `needs_branches` and starts a status refresh, so a conflict's files appear.
- `branches.rs`: `branch_menu_items` returns Pull, Merge and Delete. A new `branch_op_dialog` draws the target in a `detail_box` and a ComboBox of sources. `delete_branch_dialog` drops its picker and shows the target in a `detail_box`. The dialog is drawn wherever `delete_branch_dialog` is drawn now.

Empty source list (Pull with no remotes, or nothing else to merge): the popup shows "no branches to pull from" or "no branches to merge" and its confirm button is disabled.

## Alternatives considered

- Two separate mutations and dialogs, one per action. Rejected because about 90% of the code would be duplicated.
- `git pull <remote> <branch>` for Pull. Saves about two lines, but needs splitting `origin/feature/x` into remote and branch, which breaks on a remote name containing a slash, and fetches only one remote.

## Testing

`cli.rs` temp-repo tests, skipped when git is missing like the existing ones:

- merge fast-forwards a local branch into the current branch.
- merge into a non-current target: checkout then merge, current branch becomes the target.
- dirty tree blocking the checkout returns `Err` and leaves the current branch unchanged.
- conflicting merge returns `Err` and status then reports the conflicted path.
- `remote_branches` on a clone of a bare remote lists `origin/main` and omits `origin/HEAD`.
- pull path: commit pushed to the bare remote from a second clone, then `Mutation::Merge` with `fetch` brings it in.

Unit tests: `branch_menu_items` offers Pull, Merge, Delete with Delete disabled on the current branch. State test: `ConfirmBranchOp` on a non-current target builds a mutation with `switch` set, and a failed `Merge` result sets `needs_branches`.

## Open questions

N/A: every decision was settled in the dialogue.
