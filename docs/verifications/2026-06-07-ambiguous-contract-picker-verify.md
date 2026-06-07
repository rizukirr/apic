# Verification Report — ambiguous contract picker

**Date:** 2026-06-07
**Spec:** docs/specs/2026-06-07-ambiguous-contract-picker-design.md
**Plan:** docs/plans/2026-06-07-ambiguous-contract-picker.md
**Commit verified:** f2de5e1 (branch vibe/ambiguous-contract-picker, base main@d3a8bc5)

## Repo-level checks

- Tests: pass — `cargo test` → exit 0
  ```
  test open_resolves_and_succeeds ... ok
  test validate_ambiguous_basename_errors_when_not_a_tty ... ok
  test validate_passes_for_valid_and_fails_for_broken ... ok
  test read_ambiguous_basename_errors_when_not_a_tty ... ok
  test read_resolves_path_extensionless_and_fuzzy_forms ... ok

  test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
  ```
  (unit suite: `test result: ok. 53 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`)
- Linter: pass — `cargo clippy --all-targets` → exit 0
  ```
  Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.08s
  ```
- Format: pass — `cargo fmt --check` → exit 0
- Build: pass — `cargo build` → exit 0
  ```
  Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.07s
  ```
- `git status --porcelain`:
  ```
  (empty — clean)
  ```
- `git log --oneline d3a8bc5..HEAD`:
  ```
  f2de5e1 chore: complete Task 5 — validate wiring + fuzzy-tie e2e
  605a66b feat: extend ambiguity picker to validate -f
  4da29dc chore: complete Task 4 — resolve_one + read/open wiring
  ffc9e2a feat: prompt to pick when read/open contract is ambiguous
  f251118 chore: complete Task 3 — picker render loop
  0c9ca69 feat: add interactive crossterm picker
  2369482 chore: complete Task 2 — picker key-step logic
  4960f1c feat: add picker key-step logic
  dacbac6 chore: complete Task 1 — Resolution enum + classify()
  9afcc5c feat: add ambiguity-aware contract classification
  ```
- Surgical-diff pass: **clean** — every hunk between d3a8bc5..HEAD traced to a plan task
  (Tasks 1–5 file mappings; plan-checkbox edits authorized by exec process; rustfmt-only
  hunks in 605a66b authorized by Task 5 Step 5). Zero orphans.

## Interactive evidence (real PTY via `script`, 2026-06-07)

Spec's "Manual" testing items were executed against a real pseudo-terminal instead of
deferred. Project: two contracts `user/user.json` + `auth/user.json`.

**Pick (j + Enter), `apic read -f user`** — exit 0:
```
^[[1G^[[J2 contracts match "user":^M
^[[1m> 1. auth/user.json^[[0m^M
  2. user/user.json^M
^[[3A^[[1G^[[J2 contracts match "user":^M
  1. auth/user.json^M
^[[1m> 2. user/user.json^[[0m^M
^[[3A^[[1G^[[J→ user/user.json^M
 POST /resource/{id}/action · endpoint-name
```

**Cancel (Esc), `apic read -f user`**:
```
^[[3A^[[1G^[[Jcancelled^M
APP_EXIT=0^M
```
(terminal echoed normally afterwards — raw mode restored)

**Digit jump (2), `apic open -f user`** — selected candidate 2 and opened it in the editor.

No `^[[?1049h` (alternate screen) anywhere in any capture.

## Requirements

All verdicts via 3 independent fresh-subagent passes per requirement; no shared context.

### R1. "When a contract reference is ambiguous, show the matching candidates and let the user pick interactively (arrow keys + Enter)."
- Passes: yes / yes / yes — Verdict: **satisfied**
- Evidence: PTY pick transcript above; `picker_vim_keys_move`, `picker_up_clamps_at_top`, `picker_down_clamps_at_bottom`, `picker_enter_picks_selected` all ok.

### R2. "Apply consistently to `read`, `open`, and `validate -f`."
- Passes: yes / yes / yes — Verdict: **satisfied**
- Evidence: `resolve_one` called at src/cli.rs:325 (read_cmd), :388 (validate), :467 (open); `read_ambiguous_basename_errors_when_not_a_tty ... ok`, `open_ambiguous_basename_errors_when_not_a_tty ... ok`, `validate_ambiguous_basename_errors_when_not_a_tty ... ok`.

### R3. "In non-interactive contexts (stdin/stdout not a TTY), fail loudly: print the candidates to stderr and exit 1."
- Passes: yes / yes / yes — Verdict: **satisfied**
- Evidence: guard at src/cli.rs:306 (`if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal())` → `Err(ambiguous_message(...))` → exit 1); e2e tests assert `.failure()` + stderr contains `is ambiguous`, both candidates, `Specify the path`; all 4 non-TTY tests ok including `read_fuzzy_score_tie_errors_when_not_a_tty`.

### R4. (Non-goal) "No fzf-style type-to-filter mode or interactive contract browser."
- Passes: yes / yes / yes — Verdict: **satisfied** (forbidden behavior absent)
- Evidence: `step` match arms cover only cancel/move/confirm/digit + `_ => (selected, None)`; `picker_unknown_keys_do_nothing ... ok`; no TUI framework added.

### R5. (Non-goal) "No list scrolling in the picker — all candidates render; digit jump-select covers 1–9, arrows reach the rest."
- Passes: yes / yes / yes — Verdict: **satisfied**
- Evidence: `draw` renders every label unconditionally (no viewport logic); `picker_digit_jump_selects_in_range ... ok`, `picker_digit_out_of_range_is_ignored ... ok`; PTY digit-2 jump confirmed.

### R6. (Non-goal) "No change to `apic list` or to unambiguous resolution behavior."
- Passes: yes / yes / yes — Verdict: **satisfied**
- Evidence: pre-existing tests unmodified and passing: `list_defaults_to_relative_paths ... ok`, `list_filter_fuzzy_matches_contracts ... ok`, `read_resolves_path_extensionless_and_fuzzy_forms ... ok`, `open_resolves_and_succeeds ... ok`, `read_unknown_contract_reports_not_found ... ok`, `open_missing_contract_fails ... ok`.

### R7. (Constraint) "No new dependencies — the picker is built on the existing `crossterm` dep."
- Passes: yes / yes / yes — Verdict: **satisfied**
- Evidence: `git diff d3a8bc5..HEAD -- Cargo.toml Cargo.lock` → empty; src/picker.rs imports crossterm + std only.

### R8. (Constraint) "Inline rendering only (no alternate screen) so scroll-back is preserved."
- Passes: yes / yes / yes — Verdict: **satisfied**
- Evidence: PTY byte capture contains no `ESC[?1049h`; only `ESC[1G`/`ESC[J`/`ESC[3A`/SGR; no `EnterAlternateScreen` import.

### R9. (Constraint) "Raw mode must be restored on every exit path, including panic and Ctrl-C."
- Passes: yes / yes / yes — Verdict: **satisfied**
- Evidence: src/picker.rs:37-41 `impl Drop for RawGuard { fn drop(&mut self) { let _ = disable_raw_mode(); } }` (Drop runs on return, `?`, and panic unwind); Ctrl-C arrives as key event → Cancelled (`picker_esc_q_and_ctrl_c_cancel ... ok`); cancel PTY session restored normal terminal output (`cancelled` / `APP_EXIT=0`).

## Disagreements

None — all 27 passes unanimous.

## Overall verdict

**ready** — all 9 requirements satisfied, all repo-level checks exit 0, no disagreements, surgical-diff pass returned `clean`.
