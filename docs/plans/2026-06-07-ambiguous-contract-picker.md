# Ambiguous Contract Picker Implementation Plan

> **For executing agents:** implement this plan task-by-task. Each step uses checkbox (`- [ ]`) syntax. Do not skip steps. Do not batch commits across tasks.

**Goal:** When `apic read -f`, `apic open -f`, or `apic validate -f` resolves a contract reference ambiguously (multiple files with the same basename, or a fuzzy-score tie), show an interactive arrow-key picker; in non-TTY contexts, exit 1 listing the candidates.

**Architecture:** A new `Resolution` enum (`One`/`Many`/`None`) returned by a `classify` function in `src/cli.rs` replaces the silent `hits[0]` pick in `resolve_contract`. A new `src/picker.rs` module renders an inline crossterm picker (raw mode behind an RAII guard, pure `step` function for key handling). A shared `resolve_one` helper in `cli.rs` guards on TTY and dispatches to the picker or the error path. Spec: `docs/specs/2026-06-07-ambiguous-contract-picker-design.md`.

**Note (spec refinement):** the spec sketches `pick(prompt, candidates: &[PathBuf]) -> io::Result<Option<PathBuf>>`. The plan refines this to `pick(prompt, labels: &[String]) -> io::Result<Option<usize>>` — the caller pre-renders working-dir-relative, sanitized labels and maps the returned index back to a `PathBuf`. Behavior is identical to the spec; the picker stays render-only.

**Tech stack:** Rust (edition 2024), clap, crossterm 0.29 (already a dependency — no new deps), assert_cmd + predicates for e2e tests.

---

## File structure

New:
- `src/picker.rs` — inline interactive picker: RAII raw-mode guard, pure `step(selected, len, key, modifiers)` key logic, `pick()` render loop, unit tests for `step`.

Modified:
- `src/main.rs:10` — add `mod picker;` (alphabetical, between `json` and `render`).
- `src/cli.rs` — add `Resolution` enum + `classify()`; add `Resolved` enum + `resolve_one()` + `rel_display()` + `ambiguous_message()`; rewrite the `Read` handler, `open()`, and `validate()` to consume them; delete `resolve_contract()` and `read_filename()`; add a `#[cfg(test)] mod tests` for `classify`.
- `tests/cli.rs` — e2e tests for non-TTY ambiguity on `read`, `open`, `validate`.

---

### Task 1: Resolution enum + classify() → verify: `cargo test classify` passes (7 new unit tests in src/cli.rs)

**Files:**
- Modify: `src/cli.rs` (add `Resolution`, `classify`, and a tests module; do not touch handlers yet)

- [x] **Step 1: Write the failing tests**

Append to the end of `src/cli.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Creates a unique, empty temp directory for a single test.
    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("apic_test_cli_{tag}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Fake contract paths under a root that does not exist on disk, so the
    /// exact-path step (which checks `is_file`) never triggers.
    fn fake(root: &str, rels: &[&str]) -> (PathBuf, Vec<PathBuf>) {
        let root = PathBuf::from(root);
        let files = rels.iter().map(|r| root.join(r)).collect();
        (root, files)
    }

    #[test]
    fn classify_exact_path_wins_even_when_basenames_tie() {
        // Real files on disk: exact resolution checks is_file().
        let root = temp_dir("exact");
        fs::create_dir_all(root.join("user")).unwrap();
        fs::create_dir_all(root.join("auth")).unwrap();
        fs::write(root.join("user/user.json"), "{}").unwrap();
        fs::write(root.join("auth/user.json"), "{}").unwrap();
        let files = vec![root.join("user/user.json"), root.join("auth/user.json")];

        // Both with and without the .json extension.
        for query in ["user/user.json", "user/user"] {
            match classify(query, &root, &files) {
                Resolution::One(path) => assert_eq!(path, root.join("user/user.json")),
                other => panic!("expected One for {query}, got {other:?}"),
            }
        }
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn classify_basename_tie_returns_many_with_all_ties() {
        let (root, files) = fake(
            "/apic_no_such_root",
            &["user/user.json", "auth/user.json", "user/profile/user.json"],
        );
        match classify("user", &root, &files) {
            Resolution::Many(paths) => {
                assert_eq!(paths.len(), 3);
                assert!(paths.contains(&root.join("auth/user.json")));
            }
            other => panic!("expected Many, got {other:?}"),
        }
    }

    #[test]
    fn classify_single_basename_match_returns_one() {
        let (root, files) = fake("/apic_no_such_root", &["user/user.json", "auth/login.json"]);
        // Both bare and with explicit .json extension.
        for query in ["user", "user.json"] {
            match classify(query, &root, &files) {
                Resolution::One(path) => assert_eq!(path, root.join("user/user.json")),
                other => panic!("expected One for {query}, got {other:?}"),
            }
        }
    }

    #[test]
    fn classify_query_with_separator_skips_basename_matching() {
        // Two user.json basenames, but the query names a path, so basename
        // tie-detection is skipped and fuzzy resolves it (only the first
        // candidate contains an 'a' path segment).
        let (root, files) = fake("/proj", &["a/user.json", "b/user.json"]);
        match classify("a/user", &root, &files) {
            Resolution::One(path) => assert_eq!(path, root.join("a/user.json")),
            other => panic!("expected One, got {other:?}"),
        }
    }

    #[test]
    fn classify_fuzzy_tie_returns_many_with_top_scorers() {
        // Same structure, same length, same match positions -> equal scores.
        let (root, files) = fake("/proj", &["a/user-a.json", "b/user-b.json"]);
        match classify("usr", &root, &files) {
            Resolution::Many(paths) => assert_eq!(paths.len(), 2),
            other => panic!("expected Many, got {other:?}"),
        }
    }

    #[test]
    fn classify_distinct_fuzzy_winner_returns_one() {
        let (root, files) = fake("/proj", &["a/user.json", "b/zzz.json"]);
        match classify("usr", &root, &files) {
            Resolution::One(path) => assert_eq!(path, root.join("a/user.json")),
            other => panic!("expected One, got {other:?}"),
        }
    }

    #[test]
    fn classify_no_match_returns_none() {
        let (root, files) = fake("/proj", &["a/user.json"]);
        assert!(matches!(
            classify("qqqq", &root, &files),
            Resolution::None
        ));
    }
}
```

- [x] **Step 2: Run tests to verify they fail**

Run: `cargo test classify`
Expected: compilation fails — `classify` and `Resolution` are not defined yet (error E0425/E0412 referencing `classify` / `Resolution`).

- [x] **Step 3: Write the implementation**

In `src/cli.rs`, directly above the existing `resolve_contract` function (around line 178), add:

```rust
/// Outcome of resolving a contract reference against the discovered files.
#[derive(Debug, PartialEq)]
enum Resolution {
    /// Exactly one contract matched.
    One(PathBuf),
    /// The reference is ambiguous; the caller must disambiguate.
    Many(Vec<PathBuf>),
    /// Nothing matched.
    None,
}

/// Classifies `filename` against the discovered contract `files`.
///
/// Resolution tries, in order:
/// 1. an exact path relative to the working directory (`user/user.json`),
///    with or without the `.json` extension — always unambiguous;
/// 2. for bare names only (no path separator), files whose *basename* equals
///    the query (with `.json` appended when missing) — multiple matches are
///    returned as [`Resolution::Many`];
/// 3. the fuzzy fallback — a shared top score is ambiguous, a distinct top
///    score wins.
fn classify(filename: &str, root: &Path, files: &[PathBuf]) -> Resolution {
    // 1: exact file under the working directory, with or without `.json`.
    let candidates = [
        PathBuf::from(filename),
        PathBuf::from(format!("{filename}.json")),
    ];
    for candidate in candidates {
        if let Ok(path) = confine_to_dir(root, &candidate)
            && path.is_file()
        {
            return Resolution::One(path);
        }
    }

    // 2: basename ties, bare names only — a query with a separator already
    // had its chance at step 1 and falls through to fuzzy.
    if !filename.contains('/') && !filename.contains('\\') {
        let target = if filename.ends_with(".json") {
            filename.to_string()
        } else {
            format!("{filename}.json")
        };
        let matches: Vec<PathBuf> = files
            .iter()
            .filter(|f| f.file_name().is_some_and(|n| n.to_string_lossy() == target))
            .cloned()
            .collect();
        match matches.len() {
            0 => {}
            1 => return Resolution::One(matches.into_iter().next().unwrap()),
            _ => return Resolution::Many(matches),
        }
    }

    // 3: fuzzy fallback with tie detection on the top score.
    let file_str: Vec<String> = files
        .iter()
        .map(|f| f.to_string_lossy().to_string())
        .collect();
    match fuzzy_find(filename, &file_str) {
        Some(hits) => {
            let top = hits[0].1;
            let tied: Vec<PathBuf> = hits
                .iter()
                .take_while(|(_, score)| *score == top)
                .map(|(path, _)| PathBuf::from(path.as_str()))
                .collect();
            if tied.len() == 1 {
                Resolution::One(tied.into_iter().next().unwrap())
            } else {
                Resolution::Many(tied)
            }
        }
        None => Resolution::None,
    }
}
```

- [x] **Step 4: Run tests to verify they pass**

Run: `cargo test classify`
Expected: `test result: ok. 7 passed` (the rest of the suite is untouched). A dead-code warning on `classify`/`Resolution` is acceptable at this stage — they get callers in Task 4.

- [x] **Step 5: Commit**

```bash
git add src/cli.rs
git commit -m "feat: add ambiguity-aware contract classification"
```

---

### Task 2: picker key-step logic → verify: `cargo test picker` passes (7 new unit tests in src/picker.rs)

**Files:**
- Create: `src/picker.rs`
- Modify: `src/main.rs:10` (add `mod picker;`)

- [x] **Step 1: Create the module with failing tests**

Create `src/picker.rs`:

```rust
//! Inline interactive picker for disambiguating contract references.
//!
//! Renders a small arrow-key list in place (no alternate screen, scroll-back
//! preserved). Key handling is a pure function so it is testable without a
//! terminal; raw mode is held behind an RAII guard so it is restored on every
//! exit path, including panics.

use crossterm::event::{KeyCode, KeyModifiers};

/// Terminal decision of one picker interaction.
#[derive(Debug, PartialEq)]
enum Outcome {
    /// The user confirmed the candidate at this index.
    Picked(usize),
    /// The user cancelled (Esc, `q`, or Ctrl-C).
    Cancelled,
}

/// Applies one key press to the picker state.
///
/// Returns the new selected index and, when the key was terminal (confirm or
/// cancel), the [`Outcome`]. Movement clamps at the list edges; digits `1`-`9`
/// jump-select directly when in range and are ignored otherwise. `len` must be
/// at least 1.
fn step(
    selected: usize,
    len: usize,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> (usize, Option<Outcome>) {
    match code {
        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
            (selected, Some(Outcome::Cancelled))
        }
        KeyCode::Esc | KeyCode::Char('q') => (selected, Some(Outcome::Cancelled)),
        KeyCode::Up | KeyCode::Char('k') => (selected.saturating_sub(1), None),
        KeyCode::Down | KeyCode::Char('j') => ((selected + 1).min(len - 1), None),
        KeyCode::Enter => (selected, Some(Outcome::Picked(selected))),
        KeyCode::Char(c @ '1'..='9') => {
            let idx = (c as usize) - ('1' as usize);
            if idx < len {
                (idx, Some(Outcome::Picked(idx)))
            } else {
                (selected, None)
            }
        }
        _ => (selected, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: KeyModifiers = KeyModifiers::NONE;

    #[test]
    fn picker_up_clamps_at_top() {
        assert_eq!(step(0, 3, KeyCode::Up, NONE), (0, None));
        assert_eq!(step(2, 3, KeyCode::Up, NONE), (1, None));
    }

    #[test]
    fn picker_down_clamps_at_bottom() {
        assert_eq!(step(2, 3, KeyCode::Down, NONE), (2, None));
        assert_eq!(step(0, 3, KeyCode::Down, NONE), (1, None));
    }

    #[test]
    fn picker_vim_keys_move() {
        assert_eq!(step(1, 3, KeyCode::Char('k'), NONE), (0, None));
        assert_eq!(step(1, 3, KeyCode::Char('j'), NONE), (2, None));
    }

    #[test]
    fn picker_enter_picks_selected() {
        assert_eq!(
            step(1, 3, KeyCode::Enter, NONE),
            (1, Some(Outcome::Picked(1)))
        );
    }

    #[test]
    fn picker_digit_jump_selects_in_range() {
        assert_eq!(
            step(0, 3, KeyCode::Char('2'), NONE),
            (1, Some(Outcome::Picked(1)))
        );
    }

    #[test]
    fn picker_digit_out_of_range_is_ignored() {
        assert_eq!(step(0, 3, KeyCode::Char('9'), NONE), (0, None));
    }

    #[test]
    fn picker_esc_q_and_ctrl_c_cancel() {
        assert_eq!(step(1, 3, KeyCode::Esc, NONE), (1, Some(Outcome::Cancelled)));
        assert_eq!(
            step(1, 3, KeyCode::Char('q'), NONE),
            (1, Some(Outcome::Cancelled))
        );
        assert_eq!(
            step(1, 3, KeyCode::Char('c'), KeyModifiers::CONTROL),
            (1, Some(Outcome::Cancelled))
        );
    }

    #[test]
    fn picker_unknown_keys_do_nothing() {
        assert_eq!(step(1, 3, KeyCode::Char('x'), NONE), (1, None));
    }
}
```

In `src/main.rs`, after `mod json;` (line 10), add:

```rust
mod picker;
```

- [x] **Step 2: Run tests to verify they pass**

(The implementation is written together with its tests in Step 1 — the module is new, so there is no pre-existing behavior to fail against. The tests are the executable specification of the key map.)

Run: `cargo test picker`
Expected: `test result: ok. 8 passed`. A dead-code warning on `step`/`Outcome` is acceptable — `pick()` consumes them in Task 3.

- [x] **Step 3: Commit**

```bash
git add src/picker.rs src/main.rs
git commit -m "feat: add picker key-step logic"
```

---

### Task 3: picker render loop (pick) → verify: `cargo build` exits 0 and `cargo test` stays green

**Files:**
- Modify: `src/picker.rs` (add imports, `RawGuard`, `draw`, `pick`)

- [ ] **Step 1: Add the render loop**

In `src/picker.rs`, replace the existing single `use` line with:

```rust
use crossterm::cursor::{MoveToColumn, MoveUp};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, read};
use crossterm::style::Stylize;
use crossterm::terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode};
use crossterm::{execute, queue};
use std::io::{self, Write};
```

Then add, between the `Outcome` enum and the `step` function:

```rust
/// Holds the terminal in raw mode for its lifetime.
///
/// Raw mode is restored in `Drop`, so every exit path — confirm, cancel, IO
/// error, panic — leaves the terminal usable.
struct RawGuard;

impl RawGuard {
    fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        Ok(RawGuard)
    }
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

/// Draws (or redraws) the prompt and the candidate list in place.
///
/// On a redraw the cursor is first moved back up over the previously drawn
/// block. Lines end with `\r\n` because the terminal is in raw mode.
fn draw(
    out: &mut impl Write,
    prompt: &str,
    labels: &[String],
    selected: usize,
    redraw: bool,
) -> io::Result<()> {
    if redraw {
        queue!(out, MoveUp(labels.len() as u16 + 1))?;
    }
    queue!(out, MoveToColumn(0), Clear(ClearType::FromCursorDown))?;
    write!(out, "{prompt}\r\n")?;
    for (i, label) in labels.iter().enumerate() {
        if i == selected {
            write!(out, "{}\r\n", format!("> {}. {label}", i + 1).bold())?;
        } else {
            write!(out, "  {}. {label}\r\n", i + 1)?;
        }
    }
    out.flush()
}

/// Shows an inline arrow-key picker over `labels` and blocks until the user
/// confirms or cancels.
///
/// Returns `Ok(Some(index))` for the chosen label, or `Ok(None)` when the
/// user cancels (Esc / `q` / Ctrl-C). On confirm the picker lines are cleared
/// and replaced with a one-line summary so scroll-back stays clean. The
/// caller must ensure stdin and stdout are terminals before calling.
pub fn pick(prompt: &str, labels: &[String]) -> io::Result<Option<usize>> {
    let mut out = io::stdout();
    let guard = RawGuard::new()?;
    let mut selected = 0usize;
    draw(&mut out, prompt, labels, selected, false)?;

    let outcome = loop {
        if let Event::Key(KeyEvent {
            code,
            modifiers,
            kind,
            ..
        }) = read()?
        {
            if kind != KeyEventKind::Press {
                continue;
            }
            let (next, outcome) = step(selected, labels.len(), code, modifiers);
            if next != selected {
                selected = next;
                draw(&mut out, prompt, labels, selected, true)?;
            }
            if let Some(outcome) = outcome {
                break outcome;
            }
        }
    };

    // Clear the picker block, leave raw mode, then print the summary line.
    execute!(
        out,
        MoveUp(labels.len() as u16 + 1),
        MoveToColumn(0),
        Clear(ClearType::FromCursorDown)
    )?;
    drop(guard);

    match outcome {
        Outcome::Picked(idx) => {
            println!("→ {}", labels[idx]);
            Ok(Some(idx))
        }
        Outcome::Cancelled => Ok(None),
    }
}
```

- [ ] **Step 2: Build and test**

Run: `cargo build && cargo test`
Expected: build exits 0; full suite passes. A dead-code warning on `pick` is acceptable — it gets a caller in Task 4.

- [ ] **Step 3: Commit**

```bash
git add src/picker.rs
git commit -m "feat: add interactive crossterm picker"
```

---

### Task 4: resolve_one + read/open wiring → verify: `cargo test --test cli` passes including the 2 new ambiguity tests

**Files:**
- Modify: `src/cli.rs` (add `Resolved`, `rel_display`, `ambiguous_message`, `resolve_one`; rewrite the `Read` arm and `open()`; delete `resolve_contract` and `read_filename`)
- Test: `tests/cli.rs`

- [ ] **Step 1: Write the failing e2e tests**

Append to `tests/cli.rs`:

```rust
#[test]
fn read_ambiguous_basename_errors_when_not_a_tty() {
    let dir = init_project("ambiguous_read");
    apic(&dir)
        .args(["create", "-f", "user/user.json"])
        .assert()
        .success();
    apic(&dir)
        .args(["create", "-f", "auth/user.json"])
        .assert()
        .success();

    // Test stdin/stdout are pipes, not TTYs, so the picker must not run:
    // the command exits non-zero and lists every candidate.
    apic(&dir)
        .args(["read", "-f", "user"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("is ambiguous"))
        .stderr(predicate::str::contains("user/user.json"))
        .stderr(predicate::str::contains("auth/user.json"))
        .stderr(predicate::str::contains("Specify the path"));

    // A precise path still resolves without any prompt.
    apic(&dir)
        .args(["read", "-f", "auth/user.json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("/resource/{id}/action"));
}

#[test]
fn open_ambiguous_basename_errors_when_not_a_tty() {
    let dir = init_project("ambiguous_open");
    apic(&dir)
        .args(["create", "-f", "user/user.json"])
        .assert()
        .success();
    apic(&dir)
        .args(["create", "-f", "auth/user.json"])
        .assert()
        .success();

    apic(&dir)
        .args(["open", "-f", "user"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("is ambiguous"));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test cli ambiguous`
Expected: both new tests fail — the command currently exits 0 (read) / opens the editor (open) by silently picking the best fuzzy match, so the `.failure()` assertions trip.

- [ ] **Step 3: Implement resolve_one and rewire read + open**

In `src/cli.rs`:

3a. Add `use std::io::IsTerminal;` to the imports at the top of the file, and `use crate::picker;` alongside the other `crate::` imports.

3b. **Delete** the functions `resolve_contract` (lines 178–212 in the pre-task file) and `read_filename` (lines 214–226). In their place, add:

```rust
/// A contract reference resolved down to a single decision.
enum Resolved {
    /// Exactly one contract — proceed.
    Path(PathBuf),
    /// The user cancelled an interactive pick — not an error.
    Cancelled,
    /// Nothing matched.
    NotFound,
}

/// Renders `path` relative to `root` for display, control characters stripped.
fn rel_display(path: &Path, root: &Path) -> String {
    let shown = path.strip_prefix(root).unwrap_or(path);
    sanitize(&shown.to_string_lossy())
}

/// Builds the non-interactive ambiguity error: every candidate plus a hint.
fn ambiguous_message(filename: &str, root: &Path, candidates: &[PathBuf]) -> String {
    let rels: Vec<String> = candidates.iter().map(|c| rel_display(c, root)).collect();
    let mut msg = format!(
        "'{}' is ambiguous, {} contracts match:\n",
        sanitize(filename),
        rels.len()
    );
    for rel in &rels {
        msg.push_str(&format!("  {rel}\n"));
    }
    msg.push_str(&format!("Specify the path, e.g. -f {}", rels[0]));
    msg
}

/// Resolves `filename` to exactly one contract, asking the user to pick when
/// the reference is ambiguous.
///
/// Interactive sessions get an inline arrow-key picker. When stdin or stdout
/// is not a terminal the picker is never shown; an error listing every
/// candidate is returned instead, so scripts fail loudly rather than hang.
fn resolve_one(filename: &str) -> Result<Resolved, String> {
    let files = match list(true) {
        Some(files) => files,
        None => return Ok(Resolved::NotFound),
    };
    let root = read_config_file().and_then(|c| c.get_root_dir())?;

    match classify(filename, &root, &files) {
        Resolution::One(path) => Ok(Resolved::Path(path)),
        Resolution::None => Ok(Resolved::NotFound),
        Resolution::Many(candidates) => {
            if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
                return Err(ambiguous_message(filename, &root, &candidates));
            }
            let labels: Vec<String> = candidates
                .iter()
                .map(|c| rel_display(c, &root))
                .collect();
            let prompt = format!(
                "{} contracts match \"{}\":",
                candidates.len(),
                sanitize(filename)
            );
            match picker::pick(&prompt, &labels)
                .map_err(|err| format!("picker failed: {err}"))?
            {
                Some(idx) => Ok(Resolved::Path(candidates[idx].clone())),
                None => Ok(Resolved::Cancelled),
            }
        }
    }
}

/// Handles `apic read`: resolve to one contract, read it, render it.
fn read_cmd(filename: &str, status: Option<u16>, example: bool) -> Result<(), String> {
    match resolve_one(filename)? {
        Resolved::Path(path) => match read_file(&path) {
            Ok(content) => read(&content, status, example),
            Err(err) => {
                eprintln!("Failed to read {}: {}", path.display(), err);
                println!("No contract found");
                Ok(())
            }
        },
        Resolved::Cancelled => {
            println!("cancelled");
            Ok(())
        }
        Resolved::NotFound => {
            println!("No contract found");
            Ok(())
        }
    }
}
```

3c. Replace the body of `open` (currently lines 348–353) with:

```rust
/// Resolves `filename` to an existing contract and opens it in the editor.
fn open(filename: &str) -> Result<(), String> {
    match resolve_one(filename)? {
        Resolved::Path(path) => {
            open_in_editor(&path).map_err(|err| format!("Failed to open editor: {err}"))
        }
        Resolved::Cancelled => {
            println!("cancelled");
            Ok(())
        }
        Resolved::NotFound => Err(format!("No contract found matching '{filename}'")),
    }
}
```

3d. In `run()`, replace the `Commands::Read` arm (currently lines 431–441):

```rust
        Commands::Read {
            filename,
            status,
            example,
        } => read_cmd(&filename, status, example),
```

- [ ] **Step 4: Run the full suite**

Run: `cargo test`
Expected: everything passes — the two new ambiguity tests plus all pre-existing tests (`read_resolves_path_extensionless_and_fuzzy_forms` and `open_resolves_and_succeeds` exercise the unambiguous paths and must stay green).

- [ ] **Step 5: Commit**

```bash
git add src/cli.rs tests/cli.rs
git commit -m "feat: prompt to pick when read/open contract is ambiguous"
```

---

### Task 5: validate wiring + fuzzy-tie e2e → verify: `cargo test` fully green including validate-ambiguity and fuzzy-tie tests

**Files:**
- Modify: `src/cli.rs` (`validate` signature and target selection; `run()`'s Validate arm)
- Test: `tests/cli.rs`

- [ ] **Step 1: Write the failing e2e tests**

Append to `tests/cli.rs`:

```rust
#[test]
fn validate_ambiguous_basename_errors_when_not_a_tty() {
    let dir = init_project("ambiguous_validate");
    apic(&dir)
        .args(["create", "-f", "user/user.json"])
        .assert()
        .success();
    apic(&dir)
        .args(["create", "-f", "auth/user.json"])
        .assert()
        .success();

    apic(&dir)
        .args(["validate", "-f", "user"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("is ambiguous"));
}

#[test]
fn read_fuzzy_score_tie_errors_when_not_a_tty() {
    let dir = init_project("fuzzy_tie");
    // Different basenames, identical structure: "usr" is not a basename
    // match for either, and both fuzzy-score identically -> ambiguous.
    apic(&dir)
        .args(["create", "-f", "a/user-a.json"])
        .assert()
        .success();
    apic(&dir)
        .args(["create", "-f", "b/user-b.json"])
        .assert()
        .success();

    apic(&dir)
        .args(["read", "-f", "usr"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("is ambiguous"))
        .stderr(predicate::str::contains("user-a.json"))
        .stderr(predicate::str::contains("user-b.json"));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test cli -- ambiguous_validate fuzzy_tie`
Expected: both fail — `validate -f user` currently exits 0 validating the best fuzzy match, and `read -f usr` currently exits 0 rendering one of the tied candidates.

- [ ] **Step 3: Rewire validate**

In `src/cli.rs`:

3a. Change the `validate` signature from `fn validate(filename: Option<&str>)` to `fn validate(filename: Option<&str>) -> Result<(), String>`, and update its doc comment's first line to:

```rust
/// Validates contracts under the working directory, printing one line per file.
///
/// With `filename`, the reference is resolved like `read` — exact path,
/// basename, then fuzzy, prompting when ambiguous; otherwise every contract
/// is checked. Each file is read (subject to the size cap) and parsed against
/// the contract schema. Prints `ok`/`FAIL` per file and a summary, and exits
/// the process non-zero if any contract is invalid so it can gate CI.
```

3b. Replace its early-return and target-selection blocks (the `let files = match list(true)` block and the `let targets` block) with:

```rust
    let files = match list(true) {
        Some(files) => files,
        None => {
            println!("No contracts found");
            return Ok(());
        }
    };

    let root = read_config_file().and_then(|c| c.get_root_dir()).ok();

    // Narrow to a single contract when a filename is given.
    let targets: Vec<PathBuf> = match filename {
        Some(name) => match resolve_one(name)? {
            Resolved::Path(path) => vec![path],
            Resolved::Cancelled => {
                println!("cancelled");
                return Ok(());
            }
            Resolved::NotFound => {
                eprintln!("No contract matches {}", sanitize(name));
                std::process::exit(1);
            }
        },
        None => files,
    };
```

3c. At the very end of `validate` (after the `if failed > 0 { std::process::exit(1); }` block), add:

```rust
    Ok(())
```

3d. In `run()`, replace the `Commands::Validate` arm (and its stale comment) with:

```rust
        // `validate` exits the process itself when contracts fail
        // (per-file reporting); resolution errors return normally.
        Commands::Validate { filename } => validate(filename.as_deref()),
```

3e. If `fuzzy_find` is now unreferenced in `cli.rs` (its last direct caller was the old `validate` narrowing), remove it from the `use crate::fuzzy::...` import **only if** the compiler warns — `classify` still uses it, so the import stays.

- [ ] **Step 4: Run the full suite**

Run: `cargo test`
Expected: all tests pass, including `validate_passes_for_valid_and_fails_for_broken` (unchanged behavior for the no-filename and unambiguous cases).

- [ ] **Step 5: Lint and format**

Run: `cargo clippy --all-targets && cargo fmt --check`
Expected: no errors (warnings to be fixed if clippy flags the new code); `fmt --check` exits 0 — if it does not, run `cargo fmt` and include the result in the commit.

- [ ] **Step 6: Commit**

```bash
git add src/cli.rs tests/cli.rs
git commit -m "feat: extend ambiguity picker to validate -f"
```

---

## Manual verification (after all tasks, requires a real terminal)

Not automatable in CI — the picker needs a TTY. In a scratch project with `user/user.json` and `auth/user.json`:

1. `apic read -f user` → picker appears; ↑/↓/j/k move, `2` jump-selects, Enter confirms; picker lines are cleared and replaced by `→ <path>` plus the rendered contract.
2. `apic read -f user` then Esc (and again with `q`, and Ctrl-C) → "cancelled", exit code 0, terminal echoes normally afterward (raw mode restored).
3. `apic open -f user` → pick → editor opens the chosen file.
