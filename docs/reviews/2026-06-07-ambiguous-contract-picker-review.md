# Review — ambiguous contract picker

**Date:** 2026-06-07
**Spec:** docs/specs/2026-06-07-ambiguous-contract-picker-design.md
**Plan:** docs/plans/2026-06-07-ambiguous-contract-picker.md
**Verify report:** docs/verifications/2026-06-07-ambiguous-contract-picker-verify.md (verdict: ready)
**Commits under review:** d3a8bc5..2757078 on vibe/ambiguous-contract-picker

## Diff summary

- Files changed: 6 (code: src/cli.rs, src/main.rs, src/picker.rs, tests/cli.rs; docs: plan checkboxes, verify report)
- Lines added (code only): 602, removed: 68
- Commits: 11 (5 feature, 5 plan-checkbox, 1 verify report)

## Findings

### Block

(none)

### Warn

- **Windows untested.** classify() handles `\\` separators (src/cli.rs:217) and crossterm is cross-platform, but all evidence (PTY transcripts, e2e runs) is Linux-only; the repo has no Windows CI. Follow-up: rely on crossterm's portability or add a Windows runner to .github workflows when one exists.

### Nit

- `ambiguous_message` (src/cli.rs:275) has a single caller and could be inlined into resolve_one; kept separate for readability — fine as-is.
- The three `Resolved::Cancelled => { println!("cancelled"); Ok(()) }` arms (src/cli.rs:334, :391 area, :470 area) repeat a two-line pattern; below the threshold where a helper pays for itself.

## Pass details

- **Pass 1 (spec coverage):** all 3 Goals, 3 Non-goals, 3 Constraints satisfied — independently triple-verified in the verify-gate report with PTY, test, and diff evidence. No forbidden areas touched.
- **Pass 2 (plan fidelity):** 5/5 tasks land in plan order; all 5 commit subjects byte-match the plan (`feat: add ambiguity-aware contract classification` … `feat: extend ambiguity picker to validate -f`); every plan "Files" entry appears in the diff.
- **Pass 3 (code quality):** no logic duplication beyond the nit above; no caller-less exports (`pick` ← resolve_one; `classify` ← resolve_one; `step` ← pick); every new behavior has a failing-capable assertion (7 classify unit tests, 8 picker step tests, 4 e2e non-TTY tests). Public names match spec wording (`Resolution`, `classify`, `pick`).
- **Pass 4 (simplicity):** +602 LOC incl. ~250 lines of tests. Largest construct: src/picker.rs (219 lines, of which ~90 are tests). The interactive picker is the spec's explicitly chosen approach (numbered-prompt and error-only alternatives were offered and declined in brainstorm), so it is not halvable without losing required behavior. RawGuard is RAII-necessary, not speculative.
- **Pass 5 (surgical diff):** independently audited in verify-gate — every hunk traces to a plan task; rustfmt-only hunks in 605a66b authorized by Task 5 Step 5; zero orphans.

## Self-critique (three risks)

1. **Candidate list taller than the terminal** — MoveUp clamps at the screen top, so a redraw could leave stale lines. Mitigation: spec Non-goal explicitly scopes this out ("No list scrolling in the picker"; contracts lists that long deemed unrealistic). Accepted scope, not a defect.
2. **Fuzzy tie detection compares scores over absolute path strings** — a longer working-directory prefix could in principle skew scores. Mitigation: within any single run all candidates share the identical root prefix, so the penalty/bonus shift is uniform and tie/non-tie relationships are preserved; covered by `classify_fuzzy_tie_returns_many_with_top_scorers` and the e2e fuzzy-tie test.
3. **Terminal left raw on abnormal kill (SIGKILL)** — no Drop runs on SIGKILL. Mitigation: nothing can run on SIGKILL by definition (same exposure as vim or any raw-mode tool); SIGINT-equivalent (Ctrl-C) is handled as a key event and verified by test + PTY transcript. No additional test possible.

Risk 1 and 3 are accepted-scope; risk 2 has test evidence — no follow-up warns beyond the Windows note above.

## Diff

Full diff: `git diff d3a8bc5..2757078` in the worktree (`.vibe-worktrees/2026-06-07-ambiguous-contract-picker`), or per-feature commits `git show 9afcc5c 4960f1c 0c9ca69 ffc9e2a 605a66b`.

## Sign-off

- [ ] User reviewed findings.
- [ ] User reviewed diff.
- [ ] User approves proceeding to finish-branch.
