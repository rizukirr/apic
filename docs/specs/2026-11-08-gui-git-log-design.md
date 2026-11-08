---
title: gui git log
date: 2026-11-08
status: approved
---

# gui git log: Design

## Problem

apic-gui has no way to see a repository's history. Before v1.0.0 the user wants a git log panel like JetBrains' Git Log: a commit graph with search and filters, a commit's changed files, each file's diff in the existing diff view, and a way to revert a commit.

## Goals

1. A clock button (`CLOCK_COUNTER_CLOCKWISE`) next to the branch dropdown, shown only when the project is in a git repository, toggles a resizable bottom panel under the central area. Its open state persists across launches like the theme.
2. The panel lists commits of the Branch filter's branch (default the current branch, reset to it after a branch switch), 500 per load with a Load more row that appends the next 500. Each row shows a painted graph, the subject, ref labels as pills, the author and the date. Merge commits are dimmed. Rows are virtualised.
3. The graph draws each commit's lane and the lines between parents and children from `%H %P`. When a text or User filter is active it draws dots only. With only a Paths filter it keeps the lines, using git's rewritten parents.
4. The toolbar filters map onto `git log`: the text box ("Text or hash", 300 ms debounce) to `--grep` with `-E` when `.*` is on and `-F` otherwise, plus `-i` unless `Cc` is on. A text of 4 to 40 hex digits is treated as a hash prefix instead: the log runs without `--grep` and keeps only commits whose hash starts with it. Branch lists local then remote branches and never checks anything out. User lists All plus the authors in the loaded commits and maps to `--author`. Date offers All time, Last 24 hours, Last 7 days, Last 30 days and Last year, mapped to `--since`. Paths offers All, Contracts folder and a typed path, mapped to `-- <path>`. A Refresh button rereads. Any filter change rereads from the start.
5. Clicking a commit lists its changed files with their status on the right side of the panel, nested by folder. Clicking a file shows that commit's diff of that file in the central area through the existing diff view, including Show field changes for `.json` files, using blobs at `<sha>^` and `<sha>`.
6. Right-clicking a row offers Revert and Copy hash. Revert is disabled with a hover reason on a merge commit and when the Branch filter is not the current branch. Revert opens a confirmation popup with the short hash and subject, Cancel and Revert. Confirming runs `git revert --no-edit <sha>` as a mutation; on success the log reloads with the revert commit on top. A conflicting revert sets the conflict notice and the conflict appears in the resolver. Revert is refused while a contract has unsaved edits.
7. A `git log` error shows git's message in the panel and keeps the previous rows. An empty result shows "No commits match these filters". A commit, revert, merge, pull or branch switch made in the app refreshes the log.
8. GUI.md and CHANGELOG.md (Unreleased, Added) describe the panel.

## Non-goals

No cherry-pick, no checkout or switch from the log, no reset, no "go back to this commit", no abort revert, no calendar date picker, no all-branches graph, no commit editing, no push.

## Constraints

Everything shells out to the user's own `git` through `data/cli.rs`, following the git feature's layering (data, domain, ui/state, ui/view). Run limits: at most 3 verify rounds, at most $0 on paid runs. No new dependency. Git jobs keep the existing one-job-at-a-time queue. Lane layout lives in a pure `domain/log.rs` with no egui and no git. Commits carry no `Co-Authored-By` line. CI: fmt --check, clippy -D warnings, test --workspace on ubuntu, macos and windows, MSRV 1.92.

## Approach

`cli::log(root, rev, filter, skip, limit)` runs one `git log` with a NUL-separated format of hash, parents, author, author timestamp, refs (`%D`) and subject, plus the filter flags. `cli::commit_files(root, sha)` uses `git diff-tree --no-commit-id --name-status -r --root`. The diff job gains a commit variant producing `DiffData` from `git diff <sha>^ <sha> -- <path>` (root commit: against the empty tree) and blobs at `<sha>^` and `<sha>`. `cli::revert(root, sha)` runs `git revert --no-edit`, wrapped as a new `Mutation` so the existing status and branch rereads, events and conflict notice apply.

`domain/log.rs` holds `Commit`, `LogFilter`, the format parser and `lanes(&[Commit]) -> Vec<Row>`, each row giving the commit's lane, the lanes passing through and the edges to the next row. `GitState` gains a `LogState` (filter, commits, selected commit, its files, pending revert, open flag), with new `GitAction`s for filter changes, refresh, load more, select commit, select file, request and confirm revert, and copy hash. `ui/view/log.rs` draws the toolbar, the table with `ScrollArea::show_rows` and painter-drawn graph, and the file list reusing the Changes file tree. The revert popup goes in `dialogs()`. `app/mod.rs` adds the app bar toggle and `egui::Panel::bottom`, routes the unsaved-edits guard for revert, and sets `CentralView::Git` when a commit file is clicked.

Pushback: the user chose preset date ranges over a calendar picker to avoid adding `egui_extras` with `chrono` before 1.0. Earlier choices: current-branch log only (no all-branches graph), no cherry-pick, Revert means undo one commit with `git revert`, a confirm popup before reverting, and the commit's file list on the right of the panel (option a).

## Alternatives considered

Drawing git's own `--graph` ASCII output: no lane code, but jagged lines, extra connector rows that break one row per commit, and fragile parsing. A git library (`gix` or `git2`): a large new dependency before 1.0, against the shell-out rule. Showing all of a commit's file diffs stacked in the central area: rejected in favour of a file list (user picked a).

## Testing

Unit tests for `lanes` on hand-built histories: linear, branch and merge, two merges sharing a lane, an octopus merge, and a filtered list with missing parents (dots only). Parser tests: unicode subject, empty refs, root commit. `cli` tests on temp repos: each filter flag against real git, `commit_files` on a root commit, commit diff on a root commit. State tests on fixture repos: filter change reloads, Load more appends, select commit then file yields a commit diff, confirm revert creates a revert commit (read back with `git log`), a conflicting revert sets `conflict_notice`, revert refused for a merge commit and for a non-current branch. Headless view tests: the panel renders its toolbar labels, right-click Revert opens the popup. App tests: the toggle opens the panel, clicking a commit file sets `CentralView::Git`.

## Open questions

None.
