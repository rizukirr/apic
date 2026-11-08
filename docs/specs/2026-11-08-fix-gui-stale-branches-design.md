---
title: fix gui stale branches
date: 2026-11-08
status: approved
---

# fix gui stale branches: Design

## Problem

Failure: user report: "wait, i don't think this branch is from apic-conflict-demo". After opening ~/Projects/apic-conflict-demo, the branch dropdown lists only `main` locally and origin/feat/... remotes, which are the apic repo's branches.

Repro: headless test in a throwaway worktree: open fixture A, draw the dropdown, settle (branches ["apic-second", "apic-test"]); open fixture B with an extra branch `only-in-second`, draw, settle; the list is still ["apic-second", "apic-test"]. Failed 1 of 1 run, deterministic.

Isolated: no recent commit; the one-time `branches_loaded` fetch has existed since the branch dropdown was added. Isolated by code path, not bisect.

## Goals

1. The guard test named in the report fails at BASE and passes at HEAD.
2. The failing command from Failure exits 0 at HEAD.
3. The full test suite passes at HEAD.

## Non-goals

No behaviour change other than the one the cause names.

## Constraints

No interface change.
Run limits: at most 3 verify rounds, at most $0 on paid runs.
Commits carry no `Co-Authored-By` line.

## Approach

Cause: `App::read_project` (apic-gui/src/app/mod.rs:670-678) updates `shell.repo_root` and requests a status refresh on project open, but never a branch listing. `branch_menu` lists branches only while `branches_loaded` is false (apic-gui/src/features/git/ui/view/branches.rs:33-35), and nothing resets it, so `GitState.branches` (current, all, remotes) keeps the first repo's list for the whole session while switch, pull, merge and delete run against the new repo.

Refuted: refuter returned "stands": the only branch-reload triggers are `RefreshBranches` (guarded by `branches_loaded`) and `needs_branches` (set only after branch mutations, state.rs:511, 515, 530), and `GitState` is never recreated after the constructor (app/mod.rs:130).

## Alternatives considered

N/A: a diagnosis, not a design. The refuter's result is under Approach.

## Testing

Guard: app test `opening_another_project_reloads_its_branches`: open project A, draw the dropdown, open project B with a branch only B has, draw, settle, assert B's branch is listed and A's are gone. Fails today with the stale list.

## Open questions

N/A: the report was approved as written.
