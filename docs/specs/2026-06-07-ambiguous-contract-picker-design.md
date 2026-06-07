---
title: ambiguous contract picker
date: 2026-06-07
status: approved
---

# Ambiguous contract picker — Design

## Problem

`apic read -f user`, `apic open -f user`, and `apic validate -f user` resolve a
contract reference by exact path, then fuzzy match — silently taking the best
fuzzy hit (`hits[0]`). When multiple contracts share the same filename
(`user/user.json`, `auth/user.json`, `user/profile/user.json`), the user gets
whichever one ranked first, with no indication that other matches existed.
The user wants apic to show the candidates and let them pick.

## Goals

- When a contract reference is ambiguous, show the matching candidates and let
  the user pick interactively (arrow keys + Enter).
- Apply consistently to `read`, `open`, and `validate -f`.
- In non-interactive contexts (stdin/stdout not a TTY), fail loudly: print the
  candidates to stderr and exit 1.

## Non-goals

- No fzf-style type-to-filter mode or interactive contract browser (possible
  later feature; ratatui was considered and deferred).
- No list scrolling in the picker — all candidates render; digit jump-select
  covers 1–9, arrows reach the rest.
- No change to `apic list` or to unambiguous resolution behavior.

## Constraints

- No new dependencies — the picker is built on the existing `crossterm` dep.
- Inline rendering only (no alternate screen) so scroll-back is preserved.
- Raw mode must be restored on every exit path, including panic and Ctrl-C.

## Approach

**Pushback recorded:** an error-only flow (print candidates, exit, let the user
re-run with a precise path) was offered as the simpler framing. While weighing
it the user asked about migrating to an interactive picker; after comparing a
numbered stdin prompt, a raw-crossterm picker, `inquire`/`dialoguer`, and
ratatui, the user chose the raw-crossterm interactive picker.

### Resolution logic (src/cli.rs)

`resolve_contract` changes from `Option<PathBuf>` to:

```rust
enum Resolution {
    One(PathBuf),
    Many(Vec<PathBuf>),  // ambiguous — caller must disambiguate
    None,
}
```

Resolution order:

1. **Exact path** (`user/user.json`) or exact + `.json` (`user/user`) under the
   working dir — unchanged, always wins, never ambiguous.
2. **Basename ties** (new): only when the query is a bare name (no path
   separator) — the query (with `.json` appended when missing) is compared
   against the *filename* of every discovered contract. Multiple matches →
   `Many` with all of them; exactly one → `One`. Queries containing `/` skip
   this step (exact path already covered them) and fall through to fuzzy.
3. **Fuzzy fallback**: `fuzzy_find` as today, but a shared top score returns
   `Many` with all top-scorers; a distinct top score returns `One`.

`read`, `open`, and `validate -f` consume `Resolution`: `One` proceeds as
today, `Many` invokes the picker, `None` keeps current not-found handling.

### Picker (new module src/picker.rs)

```rust
/// Shows an inline arrow-key picker. Returns the chosen path,
/// or None if the user cancels (Esc / q / Ctrl-C).
pub fn pick(prompt: &str, candidates: &[PathBuf]) -> io::Result<Option<PathBuf>>
```

- Inline list, working-dir-relative, sanitized paths, selected row highlighted
  and number-prefixed:

  ```
  3 contracts match "user":
  > 1. user/user.json
    2. auth/user.json
    3. user/profile/user.json
  ```

- Keys: ↑/↓ and `k`/`j` move; `1`–`9` jump-select; Enter confirms; Esc / `q` /
  Ctrl-C cancels.
- Raw mode enabled only during the key loop; an RAII guard restores it on every
  exit path including panic. On exit the picker lines are cleared and one
  summary line is printed (`→ auth/user.json`).
- Cancel returns `Ok(None)`: caller prints "cancelled", exits 0.
- Key-loop logic factored as a pure `step(state, key) -> Action` function so it
  is unit-testable without a TTY.

### Non-TTY guard (call site, not picker)

If stdin or stdout is not a TTY, the picker is never invoked. Instead:

```
Error: 'user' is ambiguous, 3 contracts match:
  user/user.json
  auth/user.json
  user/profile/user.json
Specify the path, e.g. -f user/user.json
```

printed to stderr, exit 1.

## Alternatives considered

- **Error-only (no prompt):** print candidates and exit, user re-runs with a
  precise path. Simplest, fully script-safe — rejected because the user wants
  in-place interactive selection.
- **Numbered stdin prompt (`read_line`):** ~40–60 lines, no raw mode — rejected
  in favor of a real arrow-key picker at similar plumbing cost.
- **`inquire`/`dialoguer`:** ~15 lines at the call site but adds a dependency
  tree to an otherwise lean CLI — rejected; crossterm is already present.
- **ratatui:** right tool for a future interactive contract browser, overkill
  for a one-shot inline picker; adds ~15–20 transitive crates — deferred.

## Testing

- **Unit (resolution):** basename tie → `Many` with all ties; single basename
  match → `One`; exact path wins even when basenames tie; fuzzy top-score tie →
  `Many`; distinct top score → `One`.
- **Unit (picker):** `step(state, key)` covers move, wrap/clamp, digit jump,
  confirm, cancel.
- **Integration (`assert_cmd`, non-TTY):** ambiguous query → exit 1, stderr
  lists all candidates; unambiguous query behaves exactly as before.
- **Manual:** interactive pick, Esc cancel, Ctrl-C mid-pick leaves the terminal
  sane.

## Open questions

None.
