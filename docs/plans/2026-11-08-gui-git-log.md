# gui git log: Implementation Plan

**Spec:** docs/specs/2026-11-08-gui-git-log-design.md
**Goal:** a clock button next to the branch dropdown opens a resizable bottom panel listing a branch's commits with a painted graph, filters, each commit's changed files, its per-file diff in the existing diff view, and a confirmed Revert.
**Architecture:** `domain/log.rs` holds the pure pieces (commit and filter types, the NUL format parser, the name-status parser and the lane layout). `data/cli.rs` gains `log`, `commit_files`, `commit_diff_text` and `revert`, `data/jobs.rs` gains three log jobs and a `Revert` mutation, and `GitState` gains a `LogState` driven through the existing one-job queue. `ui/view/log.rs` draws the panel and the revert popup, `central_body` shows a commit file through a shared `diff_body`, and `app/mod.rs` adds the toggle, the bottom panel, the persisted open flag and the unsaved-edits guard for revert.

## Global constraints
- Everything shells out to the user's own `git` through `data/cli.rs`, following the git feature's layering (data, domain, ui/state, ui/view).
- Run limits: at most 3 verify rounds, at most $0 on paid runs.
- No new dependency.
- Git jobs keep the existing one-job-at-a-time queue.
- Lane layout lives in a pure `domain/log.rs` with no egui and no git.
- Commits carry no `Co-Authored-By` line.
- CI: fmt --check, clippy -D warnings, test --workspace on ubuntu, macos and windows, MSRV 1.92.

`apic-gui` is a binary crate (`apic-gui/Cargo.toml` has only a `[[bin]]`), so code added in Tasks 1 to 4 that the app does not call yet raises `dead_code` warnings until Task 5 wires it. Tasks 1 to 4 verify with tests, and Task 5 runs the full CI trio.

Every git test returns early when git is missing, the way the existing ones do (`git_missing()` in `cli.rs` tests, `git_available()` from `app/test_support.rs` elsewhere).

### Task 1: domain/log.rs, types, parsers and lanes → verify: `cargo test -p apic-gui domain::log::` exits 0 and runs at least 1 test

**Files:**
- Create: `apic-gui/src/features/git/domain/log.rs`
- Modify: `apic-gui/src/features/git/domain/mod.rs:7-9` (the module list, parent of the new module)
- Modify: `apic-gui/src/features/git/domain/status.rs:24` (`fn from_code` becomes `pub(crate) fn from_code`)

- [x] Step 1: In `domain/mod.rs`, add `pub(crate) mod log;` between `pub(crate) mod diff;` and `pub(crate) mod status;`, and replace the doc line `//! status, field-level contract diffs, and merge conflict markers.` with `//! status, field-level contract diffs, merge conflict markers and the commit log.`

- [x] Step 2: In `domain/status.rs` line 24, change `fn from_code(c: u8) -> Change {` to `pub(crate) fn from_code(c: u8) -> Change {`.

- [x] Step 3: Create `domain/log.rs`:

```rust
//! The commit log as plain data: the commit and filter types, the parsers for
//! `git log -z` and `git diff-tree --name-status -z` output, and the lane
//! layout the graph is painted from.
//!
//! Nothing here spawns a process or touches egui, so every function is
//! testable from hand-built commits and captured bytes.

use crate::features::git::domain::status::{Change, FileStatus};

/// One commit as `git log` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Commit {
    /// The full hash.
    pub(crate) hash: String,

    /// Parent hashes, first parent first. Under a Paths filter these are
    /// git's rewritten parents. Empty for a root commit.
    pub(crate) parents: Vec<String>,

    /// The author name.
    pub(crate) author: String,

    /// The author date, already formatted by git in local time.
    pub(crate) date: String,

    /// Ref names pointing here, such as `HEAD -> main` or `tag: v1`.
    pub(crate) refs: Vec<String>,

    /// The first line of the message.
    pub(crate) subject: String,
}

impl Commit {
    /// Whether this commit has more than one parent.
    pub(crate) fn is_merge(&self) -> bool {
        self.parents.len() > 1
    }

    /// The abbreviated hash shown in the panel and the revert popup.
    pub(crate) fn short(&self) -> &str {
        short(&self.hash)
    }
}

/// The first seven characters of `hash`, or all of it when shorter.
pub(crate) fn short(hash: &str) -> &str {
    hash.get(..7).unwrap_or(hash)
}

/// The Date filter's preset ranges.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Since {
    #[default]
    All,
    Day,
    Week,
    Month,
    Year,
}

impl Since {
    /// Every range, in menu order.
    pub(crate) const ALL: [Since; 5] = [
        Since::All,
        Since::Day,
        Since::Week,
        Since::Month,
        Since::Year,
    ];

    /// The menu label.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Since::All => "All time",
            Since::Day => "Last 24 hours",
            Since::Week => "Last 7 days",
            Since::Month => "Last 30 days",
            Since::Year => "Last year",
        }
    }

    /// The `--since` value, `None` for all time.
    pub(crate) fn arg(self) -> Option<&'static str> {
        match self {
            Since::All => None,
            Since::Day => Some("24.hours.ago"),
            Since::Week => Some("7.days.ago"),
            Since::Month => Some("30.days.ago"),
            Since::Year => Some("1.year.ago"),
        }
    }
}

/// The Paths filter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum PathFilter {
    #[default]
    All,

    /// The project's contracts folder.
    Contracts,

    /// A path typed by the user, relative to the repository root.
    Typed(String),
}

/// Everything the log toolbar filters on. Any change rereads the log from
/// the start.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LogFilter {
    /// The search box text, trimmed. A hash prefix when it is 4 to 40 hex
    /// digits, a `--grep` pattern otherwise.
    pub(crate) text: String,

    /// `.*`: the pattern is an extended regular expression, not a fixed string.
    pub(crate) regex: bool,

    /// `Cc`: the pattern matches case.
    pub(crate) match_case: bool,

    /// The branch whose history is listed, `None` for the current branch.
    pub(crate) branch: Option<String>,

    /// Only commits by this author.
    pub(crate) author: Option<String>,

    /// Only commits in this range.
    pub(crate) since: Since,

    /// Only commits touching this path.
    pub(crate) path: PathFilter,
}

impl LogFilter {
    /// The text as a lowercase hash prefix, when it is 4 to 40 hex digits.
    pub(crate) fn hash_prefix(&self) -> Option<String> {
        let text = self.text.trim();
        ((4..=40).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| text.to_ascii_lowercase())
    }

    /// Whether `commit` passes the hash prefix, true when there is none.
    pub(crate) fn keeps(&self, commit: &Commit) -> bool {
        self.hash_prefix()
            .is_none_or(|prefix| commit.hash.starts_with(&prefix))
    }

    /// Whether the graph draws dots only: a text or User filter leaves gaps
    /// in the history, so lines would join unrelated commits.
    pub(crate) fn dots_only(&self) -> bool {
        !self.text.trim().is_empty() || self.author.is_some()
    }

    /// The filter flags for `git log`, without the revision or the path.
    pub(crate) fn args(&self) -> Vec<String> {
        let mut args = Vec::new();
        if !self.text.trim().is_empty() && self.hash_prefix().is_none() {
            args.push(format!("--grep={}", self.text));
            args.push(if self.regex { "-E" } else { "-F" }.to_string());
            if !self.match_case {
                args.push("-i".to_string());
            }
        }
        if let Some(author) = &self.author {
            args.push(format!("--author={author}"));
        }
        if let Some(since) = self.since.arg() {
            args.push(format!("--since={since}"));
        }
        args
    }

    /// The path after `--`, if any. `contracts` is the contracts folder
    /// relative to the repository root, empty when it is the root itself.
    pub(crate) fn path<'a>(&'a self, contracts: &'a str) -> Option<&'a str> {
        match &self.path {
            PathFilter::All => None,
            PathFilter::Contracts => Some(contracts),
            PathFilter::Typed(path) => Some(path.trim()),
        }
        .filter(|path| !path.is_empty())
    }
}

/// Parses `git log -z --format=%H%x00%P%x00%an%x00%ad%x00%D%x00%s`: six
/// NUL-separated fields per commit, each commit ended by NUL.
pub(crate) fn parse_log(bytes: &[u8]) -> Vec<Commit> {
    let text = String::from_utf8_lossy(bytes);
    let fields: Vec<&str> = text.split('\0').collect();
    fields
        .chunks_exact(6)
        .map(|f| Commit {
            hash: f[0].trim().to_string(),
            parents: f[1].split_whitespace().map(str::to_string).collect(),
            author: f[2].to_string(),
            date: f[3].to_string(),
            refs: f[4]
                .split(", ")
                .filter(|r| !r.is_empty())
                .map(str::to_string)
                .collect(),
            subject: f[5].to_string(),
        })
        .collect()
}

/// Parses `git diff-tree --name-status -z`: a status letter and a path per
/// file. The change sits on the index side, so a file row reads it the way
/// it reads a staged change.
pub(crate) fn parse_name_status(bytes: &[u8]) -> Vec<FileStatus> {
    let text = String::from_utf8_lossy(bytes);
    let fields: Vec<&str> = text.split('\0').collect();
    fields
        .chunks_exact(2)
        .filter_map(|f| {
            Some(FileStatus {
                path: f[1].to_string(),
                index: Change::from_code(*f[0].as_bytes().first()?),
                worktree: Change::Unmodified,
                conflicted: false,
            })
        })
        .collect()
}

/// One commit's slice of the graph. Lanes are columns, counted from the left.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Row {
    /// The lane of this commit's dot.
    pub(crate) lane: usize,

    /// Lanes passing straight through this row without touching the dot.
    pub(crate) through: Vec<usize>,

    /// Lanes entering from the top that end at the dot: this commit's children.
    pub(crate) incoming: Vec<usize>,

    /// Lanes the dot leaves through at the bottom, one per parent: the edges
    /// to the next row.
    pub(crate) outgoing: Vec<usize>,
}

impl Row {
    /// How many lanes this row spans.
    pub(crate) fn width(&self) -> usize {
        std::iter::once(self.lane)
            .chain(self.through.iter().copied())
            .chain(self.incoming.iter().copied())
            .chain(self.outgoing.iter().copied())
            .max()
            .unwrap_or(0)
            + 1
    }
}

/// Lays out `commits`, newest first, into one `Row` each. A lane holds the
/// hash it waits for. A commit takes the leftmost lane waiting for it, or the
/// leftmost free lane, its first parent continues in that lane, and each
/// other parent joins the lane already waiting for it or takes a free one.
/// Lanes never move sideways, so a lane passing through is a straight line.
pub(crate) fn lanes(commits: &[Commit]) -> Vec<Row> {
    let mut slots: Vec<Option<&str>> = Vec::new();
    commits
        .iter()
        .map(|commit| {
            let incoming: Vec<usize> = slots
                .iter()
                .enumerate()
                .filter(|(_, s)| **s == Some(commit.hash.as_str()))
                .map(|(i, _)| i)
                .collect();
            let lane = incoming
                .first()
                .copied()
                .or_else(|| slots.iter().position(Option::is_none))
                .unwrap_or(slots.len());
            if lane == slots.len() {
                slots.push(None);
            }
            for &i in &incoming {
                slots[i] = None;
            }
            let through: Vec<usize> = slots
                .iter()
                .enumerate()
                .filter(|(_, s)| s.is_some())
                .map(|(i, _)| i)
                .collect();
            let mut outgoing = Vec::new();
            for (n, parent) in commit.parents.iter().enumerate() {
                let at = match slots.iter().position(|s| *s == Some(parent.as_str())) {
                    Some(i) => i,
                    None => {
                        let i = if n == 0 && slots[lane].is_none() {
                            lane
                        } else {
                            slots
                                .iter()
                                .position(Option::is_none)
                                .unwrap_or(slots.len())
                        };
                        if i == slots.len() {
                            slots.push(None);
                        }
                        slots[i] = Some(parent.as_str());
                        i
                    }
                };
                outgoing.push(at);
            }
            Row {
                lane,
                through,
                incoming,
                outgoing,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(hash: &str, parents: &[&str]) -> Commit {
        Commit {
            hash: hash.to_string(),
            parents: parents.iter().map(|p| p.to_string()).collect(),
            author: String::new(),
            date: String::new(),
            refs: Vec::new(),
            subject: String::new(),
        }
    }

    fn row(lane: usize, through: &[usize], incoming: &[usize], outgoing: &[usize]) -> Row {
        Row {
            lane,
            through: through.to_vec(),
            incoming: incoming.to_vec(),
            outgoing: outgoing.to_vec(),
        }
    }

    #[test]
    fn parse_log_reads_unicode_empty_refs_and_a_root_commit() {
        let bytes = "h1\0p1 p2\0Ann\02026-11-08 10:00\0HEAD -> main, tag: v1\0fix ✓ ünï\0h2\0\0Bob\02026-11-07 09:00\0\0root\0";
        let commits = parse_log(bytes.as_bytes());
        assert_eq!(
            commits,
            vec![
                Commit {
                    hash: "h1".into(),
                    parents: vec!["p1".into(), "p2".into()],
                    author: "Ann".into(),
                    date: "2026-11-08 10:00".into(),
                    refs: vec!["HEAD -> main".into(), "tag: v1".into()],
                    subject: "fix ✓ ünï".into(),
                },
                Commit {
                    hash: "h2".into(),
                    parents: vec![],
                    author: "Bob".into(),
                    date: "2026-11-07 09:00".into(),
                    refs: vec![],
                    subject: "root".into(),
                },
            ]
        );
        assert!(commits[0].is_merge());
        assert!(!commits[1].is_merge());
        assert!(parse_log(b"").is_empty());
    }

    #[test]
    fn parse_name_status_reads_each_letter() {
        let files = parse_name_status(b"A\0c/a.json\0M\0x y.txt\0D\0gone.json\0");
        let got: Vec<(&str, Change)> = files.iter().map(|f| (f.path.as_str(), f.index)).collect();
        assert_eq!(
            got,
            [
                ("c/a.json", Change::Added),
                ("x y.txt", Change::Modified),
                ("gone.json", Change::Deleted),
            ]
        );
        assert!(files.iter().all(|f| f.worktree == Change::Unmodified && !f.conflicted));
        assert!(parse_name_status(b"").is_empty());
    }

    #[test]
    fn short_takes_seven_characters_or_fewer() {
        assert_eq!(short("0123456789abcdef"), "0123456");
        assert_eq!(short("abc"), "abc");
    }

    #[test]
    fn lanes_of_a_linear_history_stay_in_lane_zero() {
        let rows = lanes(&[commit("a", &["b"]), commit("b", &["c"]), commit("c", &[])]);
        assert_eq!(
            rows,
            vec![row(0, &[], &[], &[0]), row(0, &[], &[0], &[0]), row(0, &[], &[0], &[])]
        );
    }

    #[test]
    fn lanes_of_a_branch_and_merge() {
        let rows = lanes(&[
            commit("m", &["a", "b"]),
            commit("a", &["base"]),
            commit("b", &["base"]),
            commit("base", &[]),
        ]);
        assert_eq!(
            rows,
            vec![
                row(0, &[], &[], &[0, 1]),
                row(0, &[1], &[0], &[0]),
                row(1, &[0], &[1], &[0]),
                row(0, &[], &[0], &[]),
            ]
        );
    }

    #[test]
    fn lanes_of_two_merges_share_the_side_lane() {
        let rows = lanes(&[
            commit("m2", &["m1", "d"]),
            commit("d", &["m1"]),
            commit("m1", &["a", "b"]),
            commit("b", &["a"]),
            commit("a", &[]),
        ]);
        assert_eq!(
            rows,
            vec![
                row(0, &[], &[], &[0, 1]),
                row(1, &[0], &[1], &[0]),
                row(0, &[], &[0], &[0, 1]),
                row(1, &[0], &[1], &[0]),
                row(0, &[], &[0], &[]),
            ]
        );
        assert!(rows.iter().all(|r| r.width() <= 2));
    }

    #[test]
    fn lanes_of_an_octopus_merge() {
        let rows = lanes(&[
            commit("o", &["a", "b", "c"]),
            commit("a", &["r"]),
            commit("b", &["r"]),
            commit("c", &["r"]),
            commit("r", &[]),
        ]);
        assert_eq!(
            rows,
            vec![
                row(0, &[], &[], &[0, 1, 2]),
                row(0, &[1, 2], &[0], &[0]),
                row(1, &[0, 2], &[1], &[0]),
                row(2, &[0], &[2], &[0]),
                row(0, &[], &[0], &[]),
            ]
        );
        assert_eq!(rows[0].width(), 3);
    }

    /// A text or User filter leaves parents out of the list: lanes still gives
    /// one row per commit, and the filter asks the view for dots only.
    #[test]
    fn lanes_of_a_filtered_list_with_missing_parents() {
        let rows = lanes(&[commit("x", &["gone1"]), commit("y", &["gone2"])]);
        assert_eq!(rows, vec![row(0, &[], &[], &[0]), row(1, &[0], &[], &[1])]);
        let text = LogFilter {
            text: "fix".into(),
            ..LogFilter::default()
        };
        let user = LogFilter {
            author: Some("Ann".into()),
            ..LogFilter::default()
        };
        let path = LogFilter {
            path: PathFilter::Typed("contracts".into()),
            ..LogFilter::default()
        };
        assert!(text.dots_only());
        assert!(user.dots_only());
        assert!(!path.dots_only());
        assert!(!LogFilter::default().dots_only());
    }

    #[test]
    fn filter_args_map_each_control() {
        assert!(LogFilter::default().args().is_empty());
        let fixed = LogFilter {
            text: "fix".into(),
            ..LogFilter::default()
        };
        assert_eq!(fixed.args(), ["--grep=fix", "-F", "-i"]);
        let regex_case = LogFilter {
            text: "fix".into(),
            regex: true,
            match_case: true,
            ..LogFilter::default()
        };
        assert_eq!(regex_case.args(), ["--grep=fix", "-E"]);
        let rest = LogFilter {
            author: Some("Ann".into()),
            since: Since::Week,
            ..LogFilter::default()
        };
        assert_eq!(rest.args(), ["--author=Ann", "--since=7.days.ago"]);
    }

    #[test]
    fn filter_reads_four_to_forty_hex_digits_as_a_hash_prefix() {
        let with = |text: &str| LogFilter {
            text: text.into(),
            ..LogFilter::default()
        };
        assert_eq!(with("ABCD").hash_prefix(), Some("abcd".to_string()));
        assert_eq!(with(&"f".repeat(40)).hash_prefix(), Some("f".repeat(40)));
        assert_eq!(with("abc").hash_prefix(), None);
        assert_eq!(with(&"f".repeat(41)).hash_prefix(), None);
        assert_eq!(with("abcx").hash_prefix(), None);
        assert!(with("abcd1234").args().is_empty());
        assert!(with("abcd").keeps(&commit("abcdef", &[])));
        assert!(!with("abcd").keeps(&commit("123456", &[])));
        assert!(with("fix").keeps(&commit("123456", &[])));
    }

    #[test]
    fn filter_path_resolves_each_choice() {
        let with = |path: PathFilter| LogFilter {
            path,
            ..LogFilter::default()
        };
        assert_eq!(with(PathFilter::All).path("contracts"), None);
        assert_eq!(with(PathFilter::Contracts).path("contracts"), Some("contracts"));
        assert_eq!(with(PathFilter::Contracts).path(""), None);
        assert_eq!(with(PathFilter::Typed(" a/b ".into())).path(""), Some("a/b"));
        assert_eq!(with(PathFilter::Typed("  ".into())).path(""), None);
    }

    #[test]
    fn since_offers_every_range_with_its_flag() {
        let got: Vec<(&str, Option<&str>)> = Since::ALL.iter().map(|s| (s.label(), s.arg())).collect();
        assert_eq!(
            got,
            [
                ("All time", None),
                ("Last 24 hours", Some("24.hours.ago")),
                ("Last 7 days", Some("7.days.ago")),
                ("Last 30 days", Some("30.days.ago")),
                ("Last year", Some("1.year.ago")),
            ]
        );
    }
}
```

- [x] Step 4: Run `cargo test -p apic-gui domain::log::`
- [x] Step 5: Run `cargo test -p apic-gui layer_rules`
- [x] Step 6: Run `cargo fmt --all`
- [x] Step 7: Commit `feat(gui): add the commit log domain: parsers, filter and lanes`

### Task 2: cli log, commit files, commit diff and revert → verify: `cargo test -p apic-gui history_` exits 0 and runs at least 1 test

**Files:**
- Modify: `apic-gui/src/features/git/data/cli.rs` (the import at line 8, new functions after `merge` at lines 243-245, the tests module at lines 247-634)

- [x] Step 1: Replace the import at line 8 with:

```rust
use crate::features::git::domain::log::{Commit, LogFilter, parse_log, parse_name_status};
use crate::features::git::domain::status::{Branches, FileStatus, Status, parse_status};
```

- [x] Step 2: Add after `merge` (line 245), before `#[cfg(test)]`:

```rust
/// The `git log` format: hash, parents, author, author date, refs and
/// subject, NUL-separated. `-z` ends each commit with NUL too.
const LOG_FORMAT: &str = "%H%x00%P%x00%an%x00%ad%x00%D%x00%s";

/// One page of `rev`'s history under `filter`: at most `limit` commits after
/// skipping `skip`. `--parents` makes `%P` carry git's rewritten parents
/// under a path filter, so the graph keeps its lines. `contracts` is the
/// contracts folder relative to the repo root, used by the Contracts folder
/// path filter. A hash prefix is not applied here, see `LogFilter::keeps`.
pub(crate) fn log(
    root: &Path,
    rev: &str,
    filter: &LogFilter,
    contracts: &str,
    skip: usize,
    limit: usize,
) -> Result<Vec<Commit>, String> {
    let mut args = vec![
        "log".to_string(),
        "-z".to_string(),
        "--parents".to_string(),
        "--date=format-local:%Y-%m-%d %H:%M".to_string(),
        format!("--format={LOG_FORMAT}"),
        format!("--skip={skip}"),
        format!("--max-count={limit}"),
    ];
    args.extend(filter.args());
    args.push(rev.to_string());
    args.push("--".to_string());
    args.extend(filter.path(contracts).map(str::to_string));
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    Ok(parse_log(&run(root, &args)?))
}

/// The files `hash` changed, against its first parent, or against nothing
/// for a root commit.
pub(crate) fn commit_files(root: &Path, hash: &str) -> Result<Vec<FileStatus>, String> {
    let out = run(
        root,
        &[
            "diff-tree",
            "--no-commit-id",
            "--name-status",
            "-r",
            "--root",
            "--diff-merges=first-parent",
            "-z",
            hash,
        ],
    )?;
    Ok(parse_name_status(&out))
}

/// The line diff of `path` in `hash`: the same as `git diff <hash>^ <hash>
/// -- <path>`, and against the empty tree for a root commit.
pub(crate) fn commit_diff_text(root: &Path, hash: &str, path: &str) -> Result<String, String> {
    let out = run(
        root,
        &[
            "diff-tree",
            "-p",
            "--no-color",
            "--no-commit-id",
            "-r",
            "--root",
            "--diff-merges=first-parent",
            hash,
            "--",
            path,
        ],
    )?;
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// Adds a commit that undoes `hash`. A conflict exits non-zero and leaves
/// the repository mid-revert for the resolver.
pub(crate) fn revert(root: &Path, hash: &str) -> Result<(), String> {
    run(root, &["revert", "--no-edit", hash]).map(|_| ())
}
```

- [x] Step 3: Add to the tests module (it already has `tempdir`, `git_missing`, `init_repo` at lines 273-283 and `commit_file` at lines 516-521):

```rust
    use crate::features::git::domain::log::{PathFilter, Since};
    use crate::features::git::domain::status::Change;

    /// Subjects of `git log HEAD` under `filter`, newest first.
    fn subjects(dir: &Path, filter: &LogFilter, contracts: &str) -> Vec<String> {
        log(dir, "HEAD", filter, contracts, 0, 500)
            .unwrap()
            .into_iter()
            .map(|c| c.subject)
            .collect()
    }

    #[test]
    fn history_log_reads_commits_newest_first() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        init_repo(&dir);
        commit_file(&dir, "second\n", "second ✓");
        let commits = log(&dir, "HEAD", &LogFilter::default(), "", 0, 500).unwrap();
        let got: Vec<&str> = commits.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(got, ["second ✓", "initial"]);
        assert_eq!(commits[0].parents, vec![commits[1].hash.clone()]);
        assert!(commits[1].parents.is_empty());
        assert_eq!(commits[0].author, "Test User");
        assert!(commits[0].refs.iter().any(|r| r.starts_with("HEAD -> ")), "{:?}", commits[0].refs);
        assert!(commits[1].refs.is_empty());
    }

    #[test]
    fn history_log_pages_with_skip_and_limit() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        init_repo(&dir);
        commit_file(&dir, "second\n", "second");
        let page = |skip| -> Vec<String> {
            log(&dir, "HEAD", &LogFilter::default(), "", skip, 1)
                .unwrap()
                .into_iter()
                .map(|c| c.subject)
                .collect()
        };
        assert_eq!(page(0), ["second"]);
        assert_eq!(page(1), ["initial"]);
    }

    #[test]
    fn history_log_grep_follows_regex_and_case() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        init_repo(&dir);
        commit_file(&dir, "1\n", "Fix a.b");
        commit_file(&dir, "2\n", "fix axb");
        let text = |text: &str, regex: bool, match_case: bool| LogFilter {
            text: text.into(),
            regex,
            match_case,
            ..LogFilter::default()
        };
        assert_eq!(subjects(&dir, &text("a.b", false, false), ""), ["Fix a.b"]);
        assert_eq!(subjects(&dir, &text("a.b", true, false), ""), ["fix axb", "Fix a.b"]);
        assert_eq!(subjects(&dir, &text("fix", false, true), ""), ["fix axb"]);
        assert_eq!(subjects(&dir, &text("fix", false, false), ""), ["fix axb", "Fix a.b"]);
    }

    #[test]
    fn history_log_filters_by_author_since_path_and_branch() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        init_repo(&dir);
        fs::write(dir.join("file.txt"), "other\n").unwrap();
        assert!(run(&dir, &["-c", "user.name=Other Person", "commit", "-am", "by other"]).is_ok());
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("sub").join("a.txt"), "a\n").unwrap();
        assert!(run(&dir, &["add", "--", "sub"]).is_ok());
        assert!(commit(&dir, "sub only", false).is_ok());

        let author = LogFilter {
            author: Some("Other".into()),
            ..LogFilter::default()
        };
        assert_eq!(subjects(&dir, &author, ""), ["by other"]);
        let day = LogFilter {
            since: Since::Day,
            ..LogFilter::default()
        };
        assert_eq!(subjects(&dir, &day, ""), ["sub only", "by other", "initial"]);
        let contracts = LogFilter {
            path: PathFilter::Contracts,
            ..LogFilter::default()
        };
        assert_eq!(subjects(&dir, &contracts, "sub"), ["sub only"]);
        let typed = LogFilter {
            path: PathFilter::Typed("file.txt".into()),
            ..LogFilter::default()
        };
        assert_eq!(subjects(&dir, &typed, ""), ["by other", "initial"]);

        let base = branches(&dir).unwrap().current.unwrap();
        create_branch(&dir, "feature").unwrap();
        switch_branch(&dir, "feature").unwrap();
        commit_file(&dir, "feature\n", "feature work");
        switch_branch(&dir, &base).unwrap();
        let feature: Vec<String> = log(&dir, "feature", &LogFilter::default(), "", 0, 500)
            .unwrap()
            .into_iter()
            .map(|c| c.subject)
            .collect();
        assert_eq!(feature[0], "feature work");
    }

    #[test]
    fn history_log_reads_a_hash_prefix_without_grep() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        init_repo(&dir);
        commit_file(&dir, "second\n", "second");
        let all = log(&dir, "HEAD", &LogFilter::default(), "", 0, 500).unwrap();
        let filter = LogFilter {
            text: all[1].hash[..7].to_string(),
            ..LogFilter::default()
        };
        let page = log(&dir, "HEAD", &filter, "", 0, 500).unwrap();
        assert_eq!(page.len(), all.len(), "a hash prefix must not become a --grep");
        let kept: Vec<&str> = page
            .iter()
            .filter(|c| filter.keeps(c))
            .map(|c| c.subject.as_str())
            .collect();
        assert_eq!(kept, ["initial"]);
    }

    #[test]
    fn history_commit_files_lists_a_root_commit() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        init_repo(&dir);
        let root = log(&dir, "HEAD", &LogFilter::default(), "", 0, 1).unwrap()[0].hash.clone();
        assert_eq!(
            commit_files(&dir, &root).unwrap(),
            vec![FileStatus {
                path: "file.txt".into(),
                index: Change::Added,
                worktree: Change::Unmodified,
                conflicted: false,
            }]
        );
    }

    #[test]
    fn history_commit_diff_text_of_a_root_commit_adds_every_line() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        init_repo(&dir);
        let root = log(&dir, "HEAD", &LogFilter::default(), "", 0, 1).unwrap()[0].hash.clone();
        let diff = commit_diff_text(&dir, &root, "file.txt").unwrap();
        assert!(diff.contains("+original"), "got: {diff}");
    }

    #[test]
    fn history_revert_adds_a_commit_that_undoes_one() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        init_repo(&dir);
        commit_file(&dir, "second\n", "second");
        let head = log(&dir, "HEAD", &LogFilter::default(), "", 0, 1).unwrap()[0].hash.clone();
        revert(&dir, &head).unwrap();
        assert_eq!(fs::read_to_string(dir.join("file.txt")).unwrap(), "original\n");
        assert!(last_commit_message(&dir).unwrap().starts_with("Revert \"second\""));
    }

    #[test]
    fn history_conflicting_revert_fails_and_leaves_the_conflict() {
        if git_missing() {
            return;
        }
        let dir = tempdir();
        init_repo(&dir);
        commit_file(&dir, "a\n", "a");
        let a = log(&dir, "HEAD", &LogFilter::default(), "", 0, 1).unwrap()[0].hash.clone();
        commit_file(&dir, "b\n", "b");
        assert!(revert(&dir, &a).is_err());
        assert!(status(&dir, &[]).unwrap().find("file.txt").is_some_and(|f| f.conflicted));
    }
```

- [x] Step 4: Run `cargo test -p apic-gui history_`
- [x] Step 5: Run `cargo fmt --all`
- [x] Step 6: Commit `feat(gui): read the git log, a commit's files and diff, and revert in cli`

### Task 3: log jobs, Revert mutation and LogState → verify: `cargo test -p apic-gui log_state_` exits 0 and runs at least 1 test, and `cargo test -p apic-gui features::git` exits 0

**Files:**
- Modify: `apic-gui/src/features/git/data/jobs.rs` (imports at lines 11-13, `MutateKind` at lines 40-49, `JobResult` at lines 52-73, after `diff_job` ending at line 125, `Mutation` at lines 197-252, `kind` at lines 256-267, `success_message` at lines 270-293, `run` at lines 296-336, after `branches` ending at line 439)
- Modify: `apic-gui/src/features/git/ui/state.rs` (imports at lines 7-11, `GitAction` at lines 49-113, `GitState` at lines 135-240, `request_refresh` at lines 268-270, the `Select` arm at lines 346-352, the end of `handle`'s match at line 450, `poll` at lines 475-486, `apply_result` at lines 491-565, after `start_branches` ending at line 658, the tests module from line 672)

- [ ] Step 1: In `jobs.rs`, replace the imports at lines 11-13 with:

```rust
use crate::features::git::data::cli;
use crate::features::git::domain::diff::{DiffRow, parse_unified};
use crate::features::git::domain::log::{Commit, LogFilter, short};
use crate::features::git::domain::status::{Branches, FileStatus, Status};
```

- [ ] Step 2: Add `Revert,` as the last variant of `MutateKind` (after `Merge,` at line 48).

- [ ] Step 3: Add these variants at the end of `JobResult` (after `Amend(...)` at line 72):

```rust

    /// One page of `git log`. Appended to the loaded commits when the bool
    /// is set, replacing them otherwise.
    Log(bool, Result<Vec<Commit>, String>),

    /// One commit's changed files, keyed by its hash so a reply for a commit
    /// the user has since left is dropped.
    CommitFiles(String, Result<Vec<FileStatus>, String>),

    /// One file's diff in one commit, keyed by `(hash, path)`.
    CommitDiff((String, String), Result<DiffData, String>),
```

- [ ] Step 4: Add after `diff_job` (line 125):

```rust
/// How many commits one log read lists, and Load more appends.
pub(crate) const LOG_PAGE: usize = 500;

/// Builds the `DiffData` for `path` in commit `hash`: its line diff against
/// the first parent, plus for a `.json` path the blobs at `<hash>^` and
/// `<hash>`. A root commit has no `<hash>^`, which `cli::blob` reads as `None`.
pub(crate) fn commit_diff_job(root: &Path, hash: &str, path: &str) -> Result<DiffData, String> {
    let raw = cli::commit_diff_text(root, hash, path)?;
    let (old_blob, new_blob) = if path.ends_with(".json") {
        (
            cli::blob(root, &format!("{hash}^"), path)?,
            cli::blob(root, hash, path)?,
        )
    } else {
        (None, None)
    };
    Ok(DiffData {
        rows: parse_unified(&raw),
        raw,
        old_blob,
        new_blob,
    })
}
```

- [ ] Step 5: Add a variant at the end of `Mutation` (after `Resolve { .. }`, line 251):

```rust

    /// Adds a commit that undoes `hash`, after the revert confirmation. A
    /// conflict leaves the repository mid-revert for the resolver.
    Revert {
        hash: String,
    },
```

Then add `Mutation::Revert { .. } => MutateKind::Revert,` to `kind`, `Mutation::Revert { hash } => format!("reverted {}", short(hash)),` to `success_message`, and `Mutation::Revert { hash } => cli::revert(root, hash),` to `run`.

- [ ] Step 6: Add after `branches` (line 439):

```rust
/// Starts one page of `git log` for `filter`, skipping `skip` commits.
/// `append` travels back with the result.
pub(crate) fn log(
    paths: RepoPaths,
    filter: LogFilter,
    skip: usize,
    append: bool,
    notify: impl Fn() + Send + 'static,
) -> Receiver<JobResult> {
    spawn(
        move || {
            let contracts = scopes(&paths.root, None, &paths.repo_root)
                .into_iter()
                .next()
                .unwrap_or_default();
            let rev = filter.branch.clone().unwrap_or_else(|| "HEAD".to_string());
            JobResult::Log(
                append,
                cli::log(&paths.repo_root, &rev, &filter, &contracts, skip, LOG_PAGE),
            )
        },
        notify,
    )
}

/// Starts listing the files commit `hash` changed.
pub(crate) fn commit_files(
    repo_root: PathBuf,
    hash: String,
    notify: impl Fn() + Send + 'static,
) -> Receiver<JobResult> {
    spawn(
        move || {
            let result = cli::commit_files(&repo_root, &hash);
            JobResult::CommitFiles(hash, result)
        },
        notify,
    )
}

/// Starts the diff of `path` in commit `hash`.
pub(crate) fn commit_diff(
    repo_root: PathBuf,
    hash: String,
    path: String,
    notify: impl Fn() + Send + 'static,
) -> Receiver<JobResult> {
    spawn(
        move || {
            let result = commit_diff_job(&repo_root, &hash, &path);
            JobResult::CommitDiff((hash, path), result)
        },
        notify,
    )
}
```

- [ ] Step 7: In `state.rs`, add to the imports at lines 7-11:

```rust
use crate::features::git::domain::log::{Commit, LogFilter, Row, lanes};
```

- [ ] Step 8: Add these variants at the end of `GitAction` (after `ResolveConflict`, line 112):

```rust

    /// Open or close the log panel. Opening reads the log.
    ToggleLog,

    /// Reread the log from the start with this filter.
    SetLogFilter(LogFilter),

    /// Reread the log from the start with the same filter.
    RefreshLog,

    /// Append the next page of commits.
    LoadMoreLog,

    /// List the changed files of this commit.
    SelectCommit { hash: String },

    /// Show the selected commit's diff of this repo-relative path.
    SelectCommitFile { path: String },

    /// Open the revert confirmation for this commit, unless `revert_block`
    /// refuses it.
    RequestRevert { hash: String },

    /// The pending revert was confirmed, run it.
    ConfirmRevert,
```

- [ ] Step 9: Add above `GitState` (before line 133):

```rust
/// Everything the log panel remembers.
#[derive(Default)]
pub(crate) struct LogState {
    /// Whether the panel is shown. Persisted by the app shell.
    pub(crate) open: bool,

    /// The filter the loaded commits were read with, or are being read with.
    pub(crate) filter: LogFilter,

    /// The search box, applied to `filter.text` once typing pauses.
    pub(crate) draft: String,

    /// The `ui.input` time of the last keystroke in the search box.
    pub(crate) edited_at: f64,

    /// The Paths menu's text field.
    pub(crate) typed_path: String,

    /// The loaded commits, newest first, hash prefix applied.
    pub(crate) commits: Vec<Commit>,

    /// `lanes(&commits)`, rebuilt whenever `commits` changes.
    pub(crate) rows: Vec<Row>,

    /// How many commits git returned so far, before the hash prefix, which
    /// is where Load more skips to.
    pub(crate) fetched: usize,

    /// The last page was full, so Load more is offered.
    pub(crate) more: bool,

    /// A read has finished, so an empty list means nothing matched.
    pub(crate) loaded: bool,

    /// The last log error, shown above the rows it leaves in place.
    pub(crate) error: String,

    /// The hash of the commit whose files are listed.
    pub(crate) selected: Option<String>,

    /// The selected commit's changed files, `None` while they load.
    pub(crate) files: Option<Vec<FileStatus>>,

    /// The commit file whose diff the central panel shows.
    pub(crate) selected_file: Option<String>,

    /// The diff for `(selected, selected_file)`, keyed the same way.
    pub(crate) diff: Option<((String, String), DiffData)>,

    /// The commit a revert would undo, while its confirmation is open.
    pub(crate) pending_revert: Option<Commit>,

    /// A read from the start is owed.
    pub(crate) needs_load: bool,

    /// A Load more is owed.
    pub(crate) needs_more: bool,

    /// The selected commit's file list is owed.
    pub(crate) needs_files: bool,

    /// The selected commit file's diff is owed.
    pub(crate) needs_diff: bool,
}
```

and add a field at the end of `GitState` (after `status_repo`, line 239):

```rust

    /// The log panel.
    pub(crate) log: LogState,
```

- [ ] Step 10: Add after `request_refresh` (line 270):

```rust
    /// Queues a log read from the start, when the panel is open.
    pub(crate) fn request_log(&mut self) {
        if self.log.open {
            self.log.needs_load = true;
        }
    }

    /// Why `commit` cannot be reverted from the log, or `None` when it can.
    pub(crate) fn revert_block(&self, commit: &Commit) -> Option<&'static str> {
        if commit.is_merge() {
            Some("Reverting a merge commit is not supported")
        } else if self
            .log
            .filter
            .branch
            .as_ref()
            .is_some_and(|b| Some(b) != self.branches.current.as_ref())
        {
            Some("Revert works on the current branch only")
        } else {
            None
        }
    }
```

- [ ] Step 11: In `handle`, add `self.log.selected_file = None;` as the first line of the `GitAction::Select` arm (line 347), and add these arms after the `ResolveConflict` arm (line 450):

```rust
            GitAction::ToggleLog => {
                self.log.open = !self.log.open;
                self.log.needs_load = self.log.open;
            }
            GitAction::SetLogFilter(filter) => {
                self.log.filter = filter;
                self.log.needs_load = true;
            }
            GitAction::RefreshLog => self.log.needs_load = true,
            GitAction::LoadMoreLog => self.log.needs_more = true,
            GitAction::SelectCommit { hash } => {
                self.log.selected = Some(hash);
                self.log.files = None;
                self.log.selected_file = None;
                self.log.needs_files = true;
            }
            GitAction::SelectCommitFile { path } => {
                self.selected = None;
                self.log.selected_file = Some(path);
                self.log.needs_diff = true;
            }
            GitAction::RequestRevert { hash } => {
                if let Some(commit) = self.log.commits.iter().find(|c| c.hash == hash)
                    && self.revert_block(commit).is_none()
                {
                    self.log.pending_revert = Some(commit.clone());
                }
            }
            GitAction::ConfirmRevert => {
                if let Some(commit) = self.log.pending_revert.take() {
                    self.start_mutation(paths, ctx, Mutation::Revert { hash: commit.hash });
                }
            }
```

- [ ] Step 12: In `poll`, extend the queue (lines 476-485) with a final branch so it reads:

```rust
            if self.needs_branches {
                self.needs_branches = false;
                self.start_branches(paths, ctx);
            } else if self.needs_amend_info {
                self.needs_amend_info = false;
                self.start_amend_info(paths, ctx);
            } else if self.needs_refresh {
                self.needs_refresh = false;
                self.start_status(paths, ctx);
            } else {
                self.start_log_work(paths, ctx);
            }
```

- [ ] Step 13: In `apply_result`, add these arms before `JobResult::Mutate(kind, message, Ok(status))` (line 522):

```rust
            JobResult::Log(append, Ok(page)) => {
                if !append {
                    self.log.commits.clear();
                    self.log.fetched = 0;
                }
                self.log.fetched += page.len();
                self.log.more = page.len() == jobs::LOG_PAGE;
                let filter = &self.log.filter;
                self.log
                    .commits
                    .extend(page.into_iter().filter(|c| filter.keeps(c)));
                self.log.rows = lanes(&self.log.commits);
                self.log.loaded = true;
                self.log.error.clear();
            }
            JobResult::Log(_, Err(e)) => self.log.error = e,
            JobResult::CommitFiles(hash, Ok(files)) => {
                if self.log.selected.as_ref() == Some(&hash) {
                    self.log.files = Some(files);
                }
            }
            JobResult::CommitFiles(_, Err(e)) => {
                self.log.files = Some(Vec::new());
                self.log.error = e;
            }
            JobResult::CommitDiff(key, Ok(data)) => self.log.diff = Some((key, data)),
            JobResult::CommitDiff(_, Err(e)) => self.log.error = e,
```

In the `Mutate(kind, message, Ok(status))` arm's `match kind` (lines 532-550), replace the `Commit` and `SwitchBranch | Merge` arms and add a `Revert` arm:

```rust
                    MutateKind::Commit => {
                        self.commit_message.clear();
                        self.amend = false;
                        self.message_before_amend = None;
                        self.amend_pushed_to = None;
                        self.request_log();
                    }
                    MutateKind::SwitchBranch | MutateKind::Merge => {
                        self.needs_branches = true;
                        self.log.filter.branch = None;
                        self.request_log();
                        events.push(GitEvent::BranchSwitched);
                    }
                    MutateKind::Revert => {
                        self.request_log();
                        events.push(GitEvent::WorkingTreeChanged);
                    }
```

Replace the `Mutate(kind, _, Err(e))` arm (lines 552-563) with:

```rust
            JobResult::Mutate(kind, _, Err(e)) => {
                self.error = e.clone();
                events.push(GitEvent::Status(e));
                match kind {
                    // A failed merge may already have switched branch or
                    // left conflicts behind: reread both so the resolver
                    // shows them, and reconcile the open contract.
                    MutateKind::Merge => {
                        self.needs_branches = true;
                        self.needs_refresh = true;
                        self.log.filter.branch = None;
                        self.request_log();
                        events.push(GitEvent::BranchSwitched);
                    }
                    // A conflicting revert writes markers into the working
                    // tree: the status reread raises the conflict notice.
                    MutateKind::Revert => {
                        self.needs_refresh = true;
                        events.push(GitEvent::WorkingTreeChanged);
                    }
                    _ => {}
                }
            }
```

- [ ] Step 14: Add after `start_branches` (line 658):

```rust
    /// Starts the first owed log job, under the one-job rule: a read from
    /// the start, then Load more, then the selected commit's files, then
    /// its file diff.
    fn start_log_work(&mut self, paths: Option<&RepoPaths>, ctx: &egui::Context) {
        if self.pending.is_some() {
            return;
        }
        let Some(paths) = paths else {
            return;
        };
        let log = &mut self.log;
        let job = if log.needs_load || log.needs_more {
            let append = !log.needs_load;
            log.needs_load = false;
            log.needs_more = false;
            let skip = if append { log.fetched } else { 0 };
            jobs::log(paths.clone(), log.filter.clone(), skip, append, notifier(ctx))
        } else if log.needs_files {
            log.needs_files = false;
            let Some(hash) = log.selected.clone() else {
                return;
            };
            jobs::commit_files(paths.repo_root.clone(), hash, notifier(ctx))
        } else if log.needs_diff {
            log.needs_diff = false;
            let (Some(hash), Some(path)) = (log.selected.clone(), log.selected_file.clone())
            else {
                return;
            };
            jobs::commit_diff(paths.repo_root.clone(), hash, path, notifier(ctx))
        } else {
            return;
        };
        self.pending = Some(job);
    }
```

- [ ] Step 15: Add to the tests module in `state.rs`:

```rust
    /// A git state on `root` with the log opened and read.
    fn open_log(root: &std::path::Path) -> (GitState, RepoPaths, egui::Context) {
        let (mut git, paths) = git_at(root);
        let ctx = egui::Context::default();
        git.handle(GitAction::ToggleLog, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        (git, paths, ctx)
    }

    fn subjects(git: &GitState) -> Vec<&str> {
        git.log.commits.iter().map(|c| c.subject.as_str()).collect()
    }

    fn log_commit(hash: &str, parents: &[&str]) -> Commit {
        Commit {
            hash: hash.to_string(),
            parents: parents.iter().map(|p| p.to_string()).collect(),
            author: "Ann".into(),
            date: String::new(),
            refs: Vec::new(),
            subject: hash.to_string(),
        }
    }

    #[test]
    fn log_state_toggle_opens_reads_and_closes() {
        if !git_available() {
            return;
        }
        let (mut git, paths, ctx) = open_log(&project_fixture());
        assert!(git.log.open);
        assert!(git.log.loaded);
        assert_eq!(subjects(&git), ["initial"]);
        git.handle(GitAction::ToggleLog, Some(&paths), &ctx);
        assert!(!git.log.open);
        assert!(!git.log.needs_load);
    }

    #[test]
    fn log_state_filter_change_rereads_from_the_start() {
        if !git_available() {
            return;
        }
        let (mut git, paths, ctx) = open_log(&project_fixture());
        let second = LogFilter {
            branch: Some("apic-second".into()),
            ..LogFilter::default()
        };
        git.handle(GitAction::SetLogFilter(second), Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        assert_eq!(subjects(&git), ["second branch contracts", "initial"]);

        let nothing = LogFilter {
            text: "no commit says this".into(),
            ..LogFilter::default()
        };
        git.handle(GitAction::SetLogFilter(nothing), Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        assert!(git.log.commits.is_empty());
        assert!(git.log.loaded);
        assert!(git.log.error.is_empty(), "{}", git.log.error);
    }

    #[test]
    fn log_state_load_more_appends_the_next_page() {
        if !git_available() {
            return;
        }
        let (mut git, paths, ctx) = open_log(&project_fixture());
        let second = LogFilter {
            branch: Some("apic-second".into()),
            ..LogFilter::default()
        };
        git.handle(GitAction::SetLogFilter(second), Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        git.log.commits.truncate(1);
        git.log.fetched = 1;

        git.handle(GitAction::LoadMoreLog, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);

        assert_eq!(subjects(&git), ["second branch contracts", "initial"]);
        assert_eq!(git.log.rows.len(), git.log.commits.len());
        assert!(!git.log.more);
    }

    #[test]
    fn log_state_a_failed_read_keeps_the_rows() {
        let mut git = GitState::default();
        git.log.commits = vec![log_commit("a", &[])];
        let mut events = Vec::new();
        git.apply_result(JobResult::Log(false, Err("bad revision".into())), &mut events);
        assert_eq!(git.log.commits.len(), 1);
        assert_eq!(git.log.error, "bad revision");
    }

    #[test]
    fn log_state_select_commit_then_file_reads_the_commit_diff() {
        if !git_available() {
            return;
        }
        let (mut git, paths, ctx) = open_log(&project_fixture());
        let hash = git.log.commits[0].hash.clone();
        git.handle(GitAction::SelectCommit { hash: hash.clone() }, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        let files: Vec<&str> = git
            .log
            .files
            .as_ref()
            .expect("the files load")
            .iter()
            .map(|f| f.path.as_str())
            .collect();
        assert!(files.contains(&"contracts/sample.json"), "{files:?}");

        git.selected = Some(("contracts/sample.json".into(), false));
        git.handle(
            GitAction::SelectCommitFile {
                path: "contracts/sample.json".into(),
            },
            Some(&paths),
            &ctx,
        );
        settle_git(&mut git, &paths, &ctx);
        let ((key_hash, key_path), data) = git.log.diff.as_ref().expect("the diff loads");
        assert_eq!(key_hash, &hash);
        assert_eq!(key_path, "contracts/sample.json");
        assert!(data.old_blob.is_none(), "the root commit has no old side");
        assert!(data.new_blob.is_some());
        assert!(!data.rows.is_empty());
        assert!(git.selected.is_none(), "a commit file replaces the working-tree selection");
    }

    #[test]
    fn log_state_confirm_revert_adds_a_revert_commit_on_top() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        dirty(&root.join("contracts").join("sample.json"));
        git_output(&root, &["commit", "-am", "change"]);
        let (mut git, paths, ctx) = open_log(&root);
        let hash = git.log.commits[0].hash.clone();
        git.handle(GitAction::RequestRevert { hash }, Some(&paths), &ctx);
        assert!(git.log.pending_revert.is_some());
        git.handle(GitAction::ConfirmRevert, Some(&paths), &ctx);
        let events = settle_git(&mut git, &paths, &ctx);
        assert_eq!(
            git_output(&root, &["log", "-1", "--format=%s"]).trim(),
            "Revert \"change\""
        );
        assert_eq!(subjects(&git)[0], "Revert \"change\"");
        assert!(events.contains(&GitEvent::WorkingTreeChanged), "{events:?}");
    }

    #[test]
    fn log_state_conflicting_revert_sets_the_conflict_notice() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        let path = root.join("contracts").join("sample.json");
        let named = |prefix: &str| {
            apic_core::template::DEFAULT.replace("\"name\": \"", &format!("\"name\": \"{prefix}"))
        };
        std::fs::write(&path, named("a-")).unwrap();
        git_output(&root, &["commit", "-am", "a"]);
        std::fs::write(&path, named("b-")).unwrap();
        git_output(&root, &["commit", "-am", "b"]);
        let (mut git, paths, ctx) = open_log(&root);
        let a = git
            .log
            .commits
            .iter()
            .find(|c| c.subject == "a")
            .expect("commit a is listed")
            .hash
            .clone();
        git.handle(GitAction::RequestRevert { hash: a }, Some(&paths), &ctx);
        git.handle(GitAction::ConfirmRevert, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        assert_eq!(
            git.conflict_notice,
            Some(vec!["contracts/sample.json".to_string()])
        );
    }

    #[test]
    fn log_state_revert_is_refused_for_a_merge_and_another_branch() {
        let ctx = egui::Context::default();
        let mut git = GitState::default();
        git.log.commits = vec![log_commit("m", &["a", "b"]), log_commit("p", &["a"])];
        git.handle(GitAction::RequestRevert { hash: "m".into() }, None, &ctx);
        assert!(git.log.pending_revert.is_none(), "a merge commit is refused");

        git.branches.current = Some("main".into());
        git.log.filter.branch = Some("other".into());
        git.handle(GitAction::RequestRevert { hash: "p".into() }, None, &ctx);
        assert!(git.log.pending_revert.is_none(), "another branch is refused");

        git.log.filter.branch = Some("main".into());
        git.handle(GitAction::RequestRevert { hash: "p".into() }, None, &ctx);
        assert_eq!(git.log.pending_revert.map(|c| c.hash), Some("p".to_string()));
    }

    #[test]
    fn log_state_a_commit_reloads_the_log() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        dirty(&root.join("contracts").join("sample.json"));
        let (mut git, paths, ctx) = open_log(&root);
        git.handle(GitAction::Refresh, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        git.handle(
            GitAction::SetTicked {
                paths: vec!["contracts/sample.json".to_string()],
                ticked: true,
            },
            Some(&paths),
            &ctx,
        );
        git.commit_message = "log me".to_string();
        git.handle(GitAction::Commit, Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        assert_eq!(subjects(&git)[0], "log me");
    }

    #[test]
    fn log_state_a_branch_switch_resets_the_branch_filter() {
        if !git_available() {
            return;
        }
        let (mut git, paths, ctx) = open_log(&project_fixture());
        let second = LogFilter {
            branch: Some("apic-second".into()),
            ..LogFilter::default()
        };
        git.handle(GitAction::SetLogFilter(second), Some(&paths), &ctx);
        settle_git(&mut git, &paths, &ctx);
        git.handle(
            GitAction::SwitchBranch {
                name: "apic-second".into(),
            },
            Some(&paths),
            &ctx,
        );
        settle_git(&mut git, &paths, &ctx);
        assert_eq!(git.log.filter.branch, None);
        assert_eq!(subjects(&git)[0], "second branch contracts");
    }
```

- [ ] Step 16: Run `cargo test -p apic-gui log_state_`
- [ ] Step 17: Run `cargo test -p apic-gui features::git`
- [ ] Step 18: Run `cargo fmt --all`
- [ ] Step 19: Commit `feat(gui): run git log, commit file and revert jobs from GitState`

### Task 4: the log panel view and the commit file diff → verify: `cargo test -p apic-gui log_view_` exits 0 and runs at least 1 test, and `cargo test -p apic-gui features::git` exits 0

**Files:**
- Create: `apic-gui/src/features/git/ui/view/log.rs`
- Modify: `apic-gui/src/features/git/ui/view/mod.rs` (the module list at lines 5-8, parent of the new module, the imports at lines 15-29, `dialogs` at lines 195-214, `central_body` at lines 282-350, the tests module from line 352, including `labelled_nodes` at line 905, `dialogs_frame` at line 928 and `node` at line 1048)
- Modify: `apic-gui/src/features/git/ui/view/files.rs` (`RowMode` at lines 25-29, the checks at lines 50 and 81, the click at lines 180-185, the tests module from line 367)

- [ ] Step 1: In `files.rs`, add a variant to `RowMode` after `Conflicted,`:

```rust

    /// A file a commit changed, in the log panel: no checkbox, no menu, and
    /// a click shows that commit's diff of it.
    Commit,
```

Change `if mode == RowMode::Conflicted {` to `if mode != RowMode::Changed {` in both `file_menu_items` (line 50) and `folder_menu_items` (line 81). Replace the click at lines 180-185 with:

```rust
                if response.clicked() {
                    action = Some(if mode == RowMode::Commit {
                        GitAction::SelectCommitFile {
                            path: file.path.clone(),
                        }
                    } else {
                        GitAction::Select {
                            path: file.path.clone(),
                            staged,
                        }
                    });
                }
```

Add to the files.rs tests module:

```rust
    #[test]
    fn log_view_commit_rows_offer_no_menu() {
        let added = side("dir/a.json", Change::Added, Change::Unmodified);
        assert!(file_menu_items(&added, RowMode::Commit).is_empty());
        let refs = [&added];
        let tree = FileTree::from_files(&refs, |_| false);
        let dir = tree.dirs.get("dir").expect("dir node");
        assert!(folder_menu_items("dir", RowMode::Commit, dir).is_empty());
    }
```

- [ ] Step 2: Create `ui/view/log.rs`:

```rust
//! The git log panel: the filter toolbar, the commit table with its painted
//! graph, the selected commit's changed files, and the revert confirmation.

use eframe::egui;
use egui::RichText;

use super::files::{FileTree, RowMode};
use crate::features::git::domain::log::{Commit, LogFilter, PathFilter, Row, Since, short};
use crate::features::git::domain::status::FileStatus;
use crate::features::git::ui::state::{GitAction, GitState};
use crate::ui::components::{
    ButtonKind, DialogButton, detail_box, dialog, empty_state, icon_button,
};
use crate::ui::theme::*;

/// Height of one commit row.
const ROW_H: f32 = 22.0;

/// Width of one graph lane.
const LANE_W: f32 = 14.0;

/// Width of the author column.
const AUTHOR_W: f32 = 140.0;

/// Width of the date column.
const DATE_W: f32 = 120.0;

/// Seconds the search box waits after the last keystroke before rereading.
const DEBOUNCE_S: f64 = 0.3;

/// The trimmed `draft` to apply, once `DEBOUNCE_S` has passed since the
/// edit at `edited_at` and it differs from the `applied` text.
fn debounced(draft: &str, applied: &str, edited_at: f64, now: f64) -> Option<String> {
    let draft = draft.trim();
    (draft != applied && now - edited_at >= DEBOUNCE_S).then(|| draft.to_string())
}

/// `filter` with `change` applied, as the action that rereads the log.
fn set(filter: &LogFilter, change: impl FnOnce(&mut LogFilter)) -> GitAction {
    let mut filter = filter.clone();
    change(&mut filter);
    GitAction::SetLogFilter(filter)
}

/// The colour of graph lane `lane`.
fn lane_color(p: &Palette, lane: usize) -> egui::Color32 {
    [p.primary, p.success, p.warning, p.info, p.syntax_key, p.error][lane % 6]
}

/// The log panel body: the toolbar on top, the selected commit's files on
/// the right, the commit table in the rest.
pub(crate) fn log_panel(ui: &mut egui::Ui, state: &mut GitState) -> Option<GitAction> {
    let mut action = None;
    egui::Panel::top("git_log_toolbar").show(ui, |ui| {
        if let Some(a) = toolbar(ui, state) {
            action = Some(a);
        }
    });
    egui::Panel::right("git_log_files")
        .resizable(true)
        .default_size(260.0)
        .min_size(160.0)
        .show(ui, |ui| {
            if let Some(a) = commit_files(ui, state) {
                action = Some(a);
            }
        });
    if let Some(a) = commit_table(ui, state) {
        action = Some(a);
    }
    action
}

/// The filter row: search box with `.*` and `Cc`, Branch, User, Date and
/// Paths menus, and Refresh. Then git's error, when the last read failed.
fn toolbar(ui: &mut egui::Ui, state: &mut GitState) -> Option<GitAction> {
    let p = palette(ui.ctx());
    let mut action = None;
    let filter = state.log.filter.clone();
    ui.add_space(SPACE_EXTRA_SMALL);
    ui.horizontal(|ui| {
        let edit = ui.add(
            egui::TextEdit::singleline(&mut state.log.draft)
                .hint_text("Text or hash")
                .desired_width(180.0),
        );
        let now = ui.input(|i| i.time);
        if edit.changed() {
            state.log.edited_at = now;
        }
        match debounced(&state.log.draft, &filter.text, state.log.edited_at, now) {
            Some(text) => action = Some(set(&filter, |f| f.text = text)),
            None if state.log.draft.trim() != filter.text => {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(300));
            }
            None => {}
        }
        if ui
            .selectable_label(filter.regex, ".*")
            .on_hover_text("Regular expression")
            .clicked()
        {
            action = Some(set(&filter, |f| f.regex = !f.regex));
        }
        if ui
            .selectable_label(filter.match_case, "Cc")
            .on_hover_text("Match case")
            .clicked()
        {
            action = Some(set(&filter, |f| f.match_case = !f.match_case));
        }

        let current = state.branches.current.clone();
        let shown = filter
            .branch
            .clone()
            .or(current.clone())
            .unwrap_or_else(|| "HEAD".to_string());
        ui.menu_button(format!("Branch: {shown}"), |ui| {
            for name in state.branches.all.iter().chain(state.branches.remotes.iter()) {
                if ui.selectable_label(*name == shown, name.as_str()).clicked() {
                    let pick = (Some(name) != current.as_ref()).then(|| name.clone());
                    action = Some(set(&filter, |f| f.branch = pick));
                    ui.close();
                }
            }
        });

        let mut authors: Vec<&str> = state.log.commits.iter().map(|c| c.author.as_str()).collect();
        authors.sort_unstable();
        authors.dedup();
        ui.menu_button(
            format!("User: {}", filter.author.as_deref().unwrap_or("All")),
            |ui| {
                if ui.selectable_label(filter.author.is_none(), "All").clicked() {
                    action = Some(set(&filter, |f| f.author = None));
                    ui.close();
                }
                for author in &authors {
                    if ui
                        .selectable_label(filter.author.as_deref() == Some(*author), *author)
                        .clicked()
                    {
                        action = Some(set(&filter, |f| f.author = Some(author.to_string())));
                        ui.close();
                    }
                }
            },
        );

        ui.menu_button(format!("Date: {}", filter.since.label()), |ui| {
            for since in Since::ALL {
                if ui.selectable_label(filter.since == since, since.label()).clicked() {
                    action = Some(set(&filter, |f| f.since = since));
                    ui.close();
                }
            }
        });

        let path_label = match &filter.path {
            PathFilter::All => "All".to_string(),
            PathFilter::Contracts => "Contracts folder".to_string(),
            PathFilter::Typed(path) => path.clone(),
        };
        ui.menu_button(format!("Paths: {path_label}"), |ui| {
            if ui.selectable_label(filter.path == PathFilter::All, "All").clicked() {
                action = Some(set(&filter, |f| f.path = PathFilter::All));
                ui.close();
            }
            if ui
                .selectable_label(filter.path == PathFilter::Contracts, "Contracts folder")
                .clicked()
            {
                action = Some(set(&filter, |f| f.path = PathFilter::Contracts));
                ui.close();
            }
            ui.horizontal(|ui| {
                let typed = ui.add(
                    egui::TextEdit::singleline(&mut state.log.typed_path)
                        .hint_text("path")
                        .desired_width(160.0),
                );
                let enter = typed.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let path = state.log.typed_path.trim().to_string();
                if (ui.button("Use").clicked() || enter) && !path.is_empty() {
                    action = Some(set(&filter, |f| f.path = PathFilter::Typed(path)));
                    ui.close();
                }
            });
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icon_button(ui, egui_phosphor::regular::ARROWS_CLOCKWISE, "Refresh log").clicked() {
                action = Some(GitAction::RefreshLog);
            }
        });
    });
    if !state.log.error.is_empty() {
        ui.label(RichText::new(&state.log.error).color(p.error));
    }
    ui.add_space(SPACE_EXTRA_SMALL);
    action
}

/// The commit table, virtualised with `show_rows`, with a Load more row at
/// the end while the last page was full.
fn commit_table(ui: &mut egui::Ui, state: &GitState) -> Option<GitAction> {
    let log = &state.log;
    if log.commits.is_empty() {
        let title = if log.loaded {
            "No commits match these filters"
        } else {
            "Loading log..."
        };
        empty_state(ui, title, "");
        return None;
    }
    let dots_only = log.filter.dots_only();
    let lanes = if dots_only {
        1
    } else {
        log.rows.iter().map(Row::width).max().unwrap_or(1)
    };
    let graph_w = lanes as f32 * LANE_W;
    let total = log.commits.len() + usize::from(log.more);
    let mut action = None;
    ui.scope(|ui| {
        // No gap between rows, so the graph's lines meet.
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, ROW_H, total, |ui, range| {
                for i in range {
                    let row = match (log.commits.get(i), log.rows.get(i)) {
                        (Some(commit), Some(row)) => {
                            commit_row(ui, state, commit, row, graph_w, dots_only)
                        }
                        _ => ui
                            .add_sized([ui.available_width(), ROW_H], egui::Button::new("Load more"))
                            .clicked()
                            .then_some(GitAction::LoadMoreLog),
                    };
                    if let Some(a) = row {
                        action = Some(a);
                    }
                }
            });
    });
    action
}

/// One commit: graph, ref pills, subject, author and date, painted into one
/// clickable rect. A click lists its files, a right-click offers Revert and
/// Copy hash. A merge commit's text is dimmed.
fn commit_row(
    ui: &mut egui::Ui,
    state: &GitState,
    commit: &Commit,
    row: &Row,
    graph_w: f32,
    dots_only: bool,
) -> Option<GitAction> {
    let p = palette(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ROW_H),
        egui::Sense::click(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &commit.subject)
    });
    let painter = ui.painter_at(rect);
    if state.log.selected.as_deref() == Some(commit.hash.as_str()) || response.hovered() {
        painter.rect_filled(rect, 0.0, p.surface_container);
    }
    paint_graph(&painter, rect, row, dots_only, p);

    let font = egui::TextStyle::Body.resolve(ui.style());
    let mid = rect.center().y;
    let date_x = rect.right() - DATE_W;
    let author_x = date_x - AUTHOR_W;
    painter.text(
        egui::pos2(date_x, mid),
        egui::Align2::LEFT_CENTER,
        &commit.date,
        font.clone(),
        p.on_surface_variant,
    );
    painter
        .with_clip_rect(egui::Rect::from_x_y_ranges(
            author_x..=date_x - SPACE_SMALL,
            rect.y_range(),
        ))
        .text(
            egui::pos2(author_x, mid),
            egui::Align2::LEFT_CENTER,
            &commit.author,
            font.clone(),
            p.on_surface_variant,
        );
    let subject = painter.with_clip_rect(egui::Rect::from_x_y_ranges(
        rect.left()..=author_x - SPACE_SMALL,
        rect.y_range(),
    ));
    let mut x = rect.left() + graph_w + SPACE_SMALL;
    for name in &commit.refs {
        let galley = subject.layout_no_wrap(name.clone(), font.clone(), p.on_surface);
        let pill = egui::Rect::from_min_size(
            egui::pos2(x, mid - galley.size().y / 2.0 - 1.0),
            galley.size() + egui::vec2(8.0, 2.0),
        );
        subject.rect_filled(pill, 4.0, p.secondary_container);
        subject.galley(pill.min + egui::vec2(4.0, 1.0), galley, p.on_surface);
        x = pill.right() + SPACE_EXTRA_SMALL;
    }
    let text = if commit.is_merge() {
        p.on_surface_variant
    } else {
        p.on_surface
    };
    subject.text(egui::pos2(x, mid), egui::Align2::LEFT_CENTER, &commit.subject, font, text);

    let mut action = response.clicked().then(|| GitAction::SelectCommit {
        hash: commit.hash.clone(),
    });
    response.context_menu(|ui| {
        let block = state.revert_block(commit);
        let revert = ui.add_enabled(block.is_none(), egui::Button::new("Revert"));
        let revert = match block {
            Some(reason) => revert.on_disabled_hover_text(reason),
            None => revert,
        };
        if revert.clicked() {
            action = Some(GitAction::RequestRevert {
                hash: commit.hash.clone(),
            });
            ui.close();
        }
        if ui.button("Copy hash").clicked() {
            ui.ctx().copy_text(commit.hash.clone());
            ui.close();
        }
    });
    action
}

/// Paints one row of the graph: the lanes passing through, the lines from
/// children into the dot and from the dot to its parents, then the dot. With
/// `dots_only`, a dot in the first lane and nothing else.
fn paint_graph(painter: &egui::Painter, rect: egui::Rect, row: &Row, dots_only: bool, p: &Palette) {
    let x = |lane: usize| rect.left() + LANE_W * (lane as f32 + 0.5);
    let (top, mid, bottom) = (rect.top(), rect.center().y, rect.bottom());
    let stroke = |lane: usize| egui::Stroke::new(1.5, lane_color(p, lane));
    if dots_only {
        painter.circle_filled(egui::pos2(x(0), mid), 3.5, lane_color(p, 0));
        return;
    }
    for &lane in &row.through {
        painter.line_segment([egui::pos2(x(lane), top), egui::pos2(x(lane), bottom)], stroke(lane));
    }
    for &lane in &row.incoming {
        painter.line_segment([egui::pos2(x(lane), top), egui::pos2(x(row.lane), mid)], stroke(lane));
    }
    for &lane in &row.outgoing {
        painter.line_segment([egui::pos2(x(row.lane), mid), egui::pos2(x(lane), bottom)], stroke(lane));
    }
    painter.circle_filled(egui::pos2(x(row.lane), mid), 3.5, lane_color(p, row.lane));
}

/// The selected commit's changed files, nested by folder with the Changes
/// file tree. A click shows that commit's diff of the file.
fn commit_files(ui: &mut egui::Ui, state: &GitState) -> Option<GitAction> {
    let p = palette(ui.ctx());
    let Some(hash) = &state.log.selected else {
        ui.label(RichText::new("Select a commit to see its files.").color(p.on_surface_variant));
        return None;
    };
    let Some(files) = &state.log.files else {
        ui.label(RichText::new("Loading files...").color(p.on_surface_variant));
        return None;
    };
    ui.label(RichText::new(short(hash)).color(p.on_surface_variant).monospace());
    let refs: Vec<&FileStatus> = files.iter().collect();
    let tree = FileTree::from_files(&refs, |_| false);
    let selected = state.log.selected_file.clone().map(|path| (path, true));
    let mut action = None;
    egui::ScrollArea::vertical()
        .id_salt("git_log_files_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            tree.show(ui, "", RowMode::Commit, selected.as_ref(), &mut action);
        });
    action
}

/// The revert confirmation, shown while `pending_revert` is set: the short
/// hash and subject, Cancel and Revert.
pub(super) fn revert_dialog(ctx: &egui::Context, state: &mut GitState) -> Option<GitAction> {
    let p = palette(ctx);
    let commit = state.log.pending_revert.clone()?;
    let outcome = dialog(
        ctx,
        "revert_modal",
        "Revert commit",
        |ui| {
            ui.label(
                RichText::new("Add a new commit that undoes").color(p.on_surface_variant),
            );
            ui.add_space(SPACE_SMALL);
            detail_box(ui, |ui| {
                ui.label(RichText::new(commit.short()).color(p.on_surface_variant).monospace());
                ui.label(RichText::new(&commit.subject).color(p.on_surface).strong());
            });
        },
        &[
            DialogButton::new("Cancel", ButtonKind::Secondary),
            DialogButton::new("Revert", ButtonKind::Danger),
        ],
    );
    if outcome.clicked == Some(1) {
        Some(GitAction::ConfirmRevert)
    } else if outcome.clicked == Some(0) || outcome.closed {
        state.log.pending_revert = None;
        None
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_view_debounce_waits_for_a_pause() {
        assert_eq!(debounced("fix", "", 0.0, 0.1), None);
        assert_eq!(debounced("fix", "", 0.0, 0.3), Some("fix".to_string()));
        assert_eq!(debounced(" fix ", "fix", 0.0, 1.0), None);
        assert_eq!(debounced("", "fix", 0.0, 1.0), Some(String::new()));
    }
}
```

- [ ] Step 3: In `view/mod.rs`, add `mod log;` after `mod files;` in the module list, then add after the `pub(crate) use self::branches::branch_menu;` line:

```rust
pub(crate) use self::log::log_panel;
use self::log::revert_dialog;
use crate::features::git::data::jobs::DiffData;
use crate::features::git::domain::log::short;
```

- [ ] Step 4: In `dialogs()`, add before `conflict_notice_dialog(ctx, state);`:

```rust
    if let Some(a) = revert_dialog(ctx, state) {
        action = Some(a);
    }
```

- [ ] Step 5: Replace `central_body` (lines 282-350, doc comment included) with:

```rust
/// The central body for the Git section: a commit file's diff when one was
/// picked in the log, otherwise the selected working-tree file's diff, an
/// empty state until then, or the conflict resolver for a conflicted file.
///
/// The panel frame is owned by the app shell. This only sees the git slice.
/// Returns the resolve action once the user clicks Resolve, for the app
/// shell to apply after this closure ends.
pub(crate) fn central_body(ui: &mut egui::Ui, state: &mut GitState) -> Option<GitAction> {
    if let Some(path) = state.log.selected_file.clone() {
        let hash = state.log.selected.clone().unwrap_or_default();
        match &state.log.diff {
            Some((key, data)) if *key == (hash.clone(), path.clone()) => {
                diff_body(ui, &mut state.show_fields, &path, short(&hash), data)
            }
            _ => empty_state(ui, "Loading diff...", ""),
        }
        return None;
    }
    let Some((path, staged)) = state.selected.clone() else {
        empty_state(ui, "No file selected", "Select a changed file on the left.");
        return None;
    };

    let loaded = match &state.diff {
        Some((key, data)) if *key == (path.clone(), staged) => Some(data),
        _ => None,
    };
    let Some(data) = loaded else {
        empty_state(ui, "Loading diff...", "");
        return None;
    };

    if state.status.find(&path).is_some_and(|f| f.conflicted) {
        // Cloned out of `data` before it is dropped, so `state` can be
        // borrowed mutably below without fighting the borrow checker over a
        // reference into `state.diff`.
        let raw = data.raw.clone();
        let new_blob = data.new_blob.clone();
        ui.add_space(SPACE_SMALL);
        return conflict_resolver(ui, state, &path, staged, &raw, new_blob.as_deref());
    }

    let side = if staged { "staged" } else { "unstaged" };
    diff_body(ui, &mut state.show_fields, &path, side, data);
    None
}

/// One read-only diff: `path` and `side` with the Show field changes toggle,
/// then the field list when asked for and available, the inline diff
/// otherwise. A non-contract file, an untracked file or a formatting-only
/// change always shows the inline diff, so no selection renders an empty pane.
fn diff_body(ui: &mut egui::Ui, show_fields: &mut bool, path: &str, side: &str, data: &DiffData) {
    let p = palette(ui.ctx());
    let field_changes =
        domain_diff::semantic_changes(data.old_blob.as_deref(), data.new_blob.as_deref());
    ui.add_space(SPACE_SMALL);
    ui.horizontal(|ui| {
        ui.label(RichText::new(path).color(p.on_surface).strong());
        ui.label(RichText::new(side).color(p.on_surface_variant));
        ui.add_enabled_ui(field_changes.is_some(), |ui| {
            ui.checkbox(show_fields, "Show field changes")
        })
        .response
        .on_disabled_hover_text("No field-level view for this change");
    });
    ui.separator();
    match field_changes.filter(|_| *show_fields) {
        Some(changes) => semantic_diff_view(ui, &changes),
        None => inline_diff_view(ui, &data.rows),
    }
}
```

- [ ] Step 6: In the `view/mod.rs` tests module, remove the now duplicate `use crate::features::git::data::jobs::DiffData;` (line 356), and add:

```rust
    use crate::features::git::domain::log::{Commit, lanes};
    use crate::features::git::ui::state::LogState;

    /// A merge on top of a root commit, both loaded, on branch `main`.
    fn log_view_state() -> GitState {
        let merge = Commit {
            hash: "b".repeat(40),
            parents: vec!["a".repeat(40), "c".repeat(40)],
            author: "Ann".into(),
            date: "2026-11-08 10:00".into(),
            refs: vec!["HEAD -> main".into()],
            subject: "Merge branch 'side'".into(),
        };
        let root = Commit {
            hash: "a".repeat(40),
            parents: vec![],
            author: "Bob".into(),
            date: "2026-11-07 09:00".into(),
            refs: vec![],
            subject: "first commit".into(),
        };
        let commits = vec![merge, root];
        GitState {
            branches: Branches {
                current: Some("main".into()),
                all: vec!["main".into()],
                remotes: vec![],
            },
            log: LogState {
                open: true,
                rows: lanes(&commits),
                commits,
                loaded: true,
                ..LogState::default()
            },
            ..GitState::default()
        }
    }

    /// One headless frame of `log_panel` fed `events`. Stores any action it
    /// returns into `action` and returns every labelled AccessKit node.
    fn log_frame(
        ctx: &egui::Context,
        state: &mut GitState,
        events: Vec<egui::Event>,
        action: &mut Option<GitAction>,
    ) -> Vec<(String, egui::Rect)> {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            events,
            ..Default::default()
        };
        labelled_nodes(ctx.run_ui(input, |ui| {
            if let Some(a) = log_panel(ui, state) {
                *action = Some(a);
            }
        }))
    }

    /// Moves to the centre of `rect` and clicks `button` there, then runs two
    /// idle frames. Returns the nodes from the last frame.
    fn log_click(
        ctx: &egui::Context,
        state: &mut GitState,
        rect: egui::Rect,
        button: egui::PointerButton,
        action: &mut Option<GitAction>,
    ) -> Vec<(String, egui::Rect)> {
        let pos = rect.center();
        let press = |pressed| egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let mut nodes = Vec::new();
        for events in [
            vec![egui::Event::PointerMoved(pos)],
            vec![press(true)],
            vec![press(false)],
            vec![],
            vec![],
        ] {
            nodes = log_frame(ctx, state, events, action);
        }
        nodes
    }

    #[test]
    fn log_view_panel_renders_its_toolbar_labels_and_rows() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut state = log_view_state();
        let mut action = None;
        log_frame(&ctx, &mut state, vec![], &mut action);
        let nodes = log_frame(&ctx, &mut state, vec![], &mut action);
        for label in [
            "Branch: main",
            "User: All",
            "Date: All time",
            "Paths: All",
            ".*",
            "Cc",
            "Merge branch 'side'",
            "first commit",
        ] {
            node(&nodes, label);
        }
    }

    #[test]
    fn log_view_right_click_revert_opens_the_popup() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut state = log_view_state();
        let mut action = None;
        log_frame(&ctx, &mut state, vec![], &mut action);
        let nodes = log_frame(&ctx, &mut state, vec![], &mut action);
        let row = node(&nodes, "first commit");
        let nodes = log_click(&ctx, &mut state, row, egui::PointerButton::Secondary, &mut action);
        let revert = node(&nodes, "Revert");
        log_click(&ctx, &mut state, revert, egui::PointerButton::Primary, &mut action);

        state.handle(action.expect("Revert returns an action"), None, &ctx);
        assert_eq!(
            state.log.pending_revert.as_ref().map(|c| c.hash.clone()),
            Some("a".repeat(40))
        );
        dialogs_frame(&ctx, &mut state, vec![]);
        let nodes = dialogs_frame(&ctx, &mut state, vec![]);
        node(&nodes, "Revert commit");
        node(&nodes, "first commit");
    }

    /// Every state the panel draws: loading, nothing matched, a failed
    /// read over rows, a selected commit with files and Load more, and dots
    /// only under a text filter.
    fn log_view_variants() -> Vec<GitState> {
        let loading = GitState::default();
        let empty = GitState {
            log: LogState {
                loaded: true,
                ..LogState::default()
            },
            ..GitState::default()
        };
        let mut failed = log_view_state();
        failed.log.error = "fatal: bad revision".into();
        let mut busy = log_view_state();
        busy.log.more = true;
        busy.log.selected = Some("a".repeat(40));
        busy.log.files = Some(vec![FileStatus {
            path: "contracts/a.json".into(),
            index: Change::Added,
            worktree: Change::Unmodified,
            conflicted: false,
        }]);
        busy.log.selected_file = Some("contracts/a.json".into());
        let mut dots = log_view_state();
        dots.log.filter.text = "first".into();
        vec![loading, empty, failed, busy, dots]
    }

    #[test]
    fn log_view_renders_each_state_in_both_themes() {
        for preference in BOTH_THEMES {
            for mut state in log_view_variants() {
                run_in_theme(preference, |ui| {
                    let _ = log_panel(ui, &mut state);
                });
            }
        }
    }

    #[test]
    fn log_view_central_shows_a_commit_file_diff() {
        for preference in BOTH_THEMES {
            for show_fields in [false, true] {
                let hash = "a".repeat(40);
                let mut state = GitState {
                    show_fields,
                    log: LogState {
                        selected: Some(hash.clone()),
                        selected_file: Some("contracts/a.json".into()),
                        diff: Some((
                            (hash, "contracts/a.json".into()),
                            diff_data(
                                "@@ -1 +1 @@\n-GET\n+POST\n",
                                Some(CONTRACT_GET),
                                Some(CONTRACT_POST),
                            ),
                        )),
                        ..LogState::default()
                    },
                    ..GitState::default()
                };
                run_in_theme(preference, |ui| {
                    assert!(central_body(ui, &mut state).is_none());
                });
            }
        }
    }
```

- [ ] Step 7: Run `cargo test -p apic-gui log_view_`
- [ ] Step 8: Run `cargo test -p apic-gui features::git`
- [ ] Step 9: Run `cargo fmt --all`
- [ ] Step 10: Commit `feat(gui): draw the git log panel, its file list and the revert popup`

### Task 5: app wiring, persisted open flag and docs → verify: `cargo test -p apic-gui log_app_` exits 0 and runs at least 1 test, and `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` each exit 0

**Files:**
- Modify: `apic-gui/src/platform/settings.rs` (`Settings` at lines 10-28, `parse` at lines 148-153, `body` at lines 156-171, after `parse_theme` ending at line 213, the test at lines 299-308)
- Modify: `apic-gui/src/app/mod.rs` (the import at line 26, `App::new` at lines 134-140, `top_bar` at lines 288-292, a new method after `sidebar` ending at line 464, `read_project` at lines 677-689, `apply` at lines 780-806, `ui` at lines 825-829, the tests module from line 842)
- Modify: `GUI.md:47-49`
- Modify: `CHANGELOG.md:10-12`

- [ ] Step 1: In `settings.rs`, add a field to `Settings` after `env`:

```rust

    /// Whether the git log panel was open, restored on the next launch.
    pub git_log_open: bool,
```

In `parse`, add `git_log_open: parse_flag(&top, "git_log_open"),` to the `Settings` literal. In `body`, add after the theme line (line 161):

```rust
        body.push_str(&format!("git_log_open = {}\n", self.git_log_open));
```

Add after `parse_theme`:

```rust
/// Extracts `key = true`, false when it is missing or holds anything else.
fn parse_flag(text: &str, key: &str) -> bool {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix(key)?.trim_start().strip_prefix('='))
        .any(|value| value.trim() == "true")
}
```

In the test `body_round_trips_both_settings`, add `git_log_open: true,` to its `Settings` literal, and add:

```rust
    #[test]
    fn log_app_open_flag_round_trips_in_settings() {
        let path = temp_settings_path("git-log-open");
        assert!(!Settings::load_from(&path).git_log_open);
        Settings::update_at(&path, |s| s.git_log_open = true);
        Settings::update_at(&path, |s| s.theme = ThemePreference::Dark);
        assert!(Settings::load_from(&path).git_log_open);
        assert!(!parse_flag("git_log_open = false\n", "git_log_open"));
    }
```

- [ ] Step 2: In `app/mod.rs`, change the import at line 26 to:

```rust
use crate::features::git::ui::state::{BranchOpKind, GitAction, GitEvent, GitState, LogState};
```

In `App::new`, add right after `let settings = Settings::load();` (line 134):

```rust
        app.git.log.open = settings.git_log_open;
```

- [ ] Step 3: In `top_bar`, replace lines 288-292 with:

```rust
                        if self.shell.repo_root.is_some() {
                            if let Some(a) = view::branch_menu(ui, &mut self.git) {
                                action = Some(Action::Git(a));
                            }
                            let clock = RichText::new(egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE)
                                .color(p.on_surface_variant);
                            if ui
                                .add(egui::Button::selectable(self.git.log.open, clock))
                                .on_hover_text("Git log")
                                .clicked()
                            {
                                action = Some(Action::Git(GitAction::ToggleLog));
                            }
                        }
```

- [ ] Step 4: Add after `sidebar` (line 464):

```rust
    /// The git log, docked under the central area while it is open and the
    /// project is inside a repository.
    fn log_panel(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        if !self.git.log.open || self.shell.repo_root.is_none() {
            return None;
        }
        egui::Panel::bottom("git_log")
            .resizable(true)
            .default_size(260.0)
            .min_size(120.0)
            .show(ui, |ui| view::log_panel(ui, &mut self.git))
            .inner
            .map(Action::Git)
    }
```

- [ ] Step 5: In `read_project`, replace lines 677-689 with:

```rust
        if repo_root != self.shell.repo_root {
            // Another repository, or none: the branch list and the log belong
            // to the old one, and the dropdown only lists branches once on its own.
            self.git.branches = Default::default();
            self.git.needs_branches = repo_root.is_some();
            self.git.log = LogState {
                open: self.git.log.open,
                ..LogState::default()
            };
        }
        self.shell.repo_root = repo_root;
        // The Git section header's dirty indicator must be correct before the
        // Git section is ever opened, so a project load fetches status
        // itself instead of waiting for tab activation.
        if self.shell.repo_root.is_some() {
            self.git.request_refresh();
            self.git.request_log();
        }
```

- [ ] Step 6: In `apply`, replace the guard arm and the generic git arm (lines 780-806) with:

```rust
            Action::Git(
                action @ (GitAction::SwitchBranch { .. }
                | GitAction::ConfirmBranchOp
                | GitAction::ConfirmRevert),
            ) if self.contracts.has_unsaved_edits() => {
                let open = self
                    .contracts
                    .path
                    .as_deref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "the open contract".to_string());
                let what = match (&action, &self.git.pending_branch_op) {
                    (GitAction::ConfirmRevert, _) => "revert",
                    (GitAction::ConfirmBranchOp, Some(op)) if op.kind == BranchOpKind::Pull => {
                        "pull"
                    }
                    (GitAction::ConfirmBranchOp, _) => "merge",
                    _ => "switch branch",
                };
                self.git.pending_branch_op = None;
                self.git.log.pending_revert = None;
                let msg = format!("cannot {what}, {open} has unsaved edits");
                self.git.error = msg.clone();
                self.shell.status = msg;
            }
            Action::Git(GitAction::ToggleLog) => {
                self.handle_git(GitAction::ToggleLog, ctx);
                let open = self.git.log.open;
                self.update_settings(|s| s.git_log_open = open);
            }
            Action::Git(action) => {
                if matches!(
                    action,
                    GitAction::Select { .. } | GitAction::SelectCommitFile { .. }
                ) {
                    self.shell.central = CentralView::Git;
                }
                self.handle_git(action, ctx);
            }
```

- [ ] Step 7: In `ui`, insert between the `top.or(side)` block (lines 826-828) and `self.central(ui);` (line 829):

```rust
        if let Some(a) = self.log_panel(ui) {
            self.apply(a, &ctx);
        }
```

- [ ] Step 8: Add to the `app/mod.rs` tests module:

```rust
    #[test]
    fn log_app_toggle_opens_the_panel_and_remembers_it() {
        if !git_available() {
            return;
        }
        let mut app = app_at(project_fixture());
        let ctx = egui::Context::default();
        app.reload_project();
        settle(&mut app, &ctx);

        app.apply(Action::Git(GitAction::ToggleLog), &ctx);
        assert!(app.git.log.open);
        settle(&mut app, &ctx);
        assert!(!app.git.log.commits.is_empty());
        let settings_path = app.settings_path.clone().expect("test app has a settings path");
        assert!(Settings::load_from(&settings_path).git_log_open);

        for preference in crate::ui::theme::BOTH_THEMES {
            crate::ui::theme::run_in_theme(preference, |ui| {
                let _ = app.top_bar(ui);
                let _ = app.log_panel(ui);
            });
        }

        app.apply(Action::Git(GitAction::ToggleLog), &ctx);
        assert!(!Settings::load_from(&settings_path).git_log_open);
    }

    #[test]
    fn log_app_clicking_a_commit_file_shows_git_in_the_central_panel() {
        let mut app = app_at(tempdir());
        let ctx = egui::Context::default();
        app.apply(
            Action::Git(GitAction::SelectCommitFile {
                path: "contracts/sample.json".to_string(),
            }),
            &ctx,
        );
        assert_eq!(app.shell.central, CentralView::Git);
    }

    #[test]
    fn log_app_revert_is_refused_with_unsaved_edits() {
        if !git_available() {
            return;
        }
        let root = project_fixture();
        let mut app = app_at(root.clone());
        let ctx = egui::Context::default();
        app.reload_project();
        settle(&mut app, &ctx);
        let i = app
            .contracts
            .entries
            .iter()
            .position(|e| e.rel == "sample.json")
            .expect("sample.json is in the fixture");
        app.apply(Action::Contracts(ContractsAction::Load(i)), &ctx);
        app.apply(Action::Contracts(ContractsAction::ToggleEdit), &ctx);
        app.contracts.model.as_mut().unwrap().name = "dirtied".to_string();
        let head = git_output(&root, &["rev-parse", "HEAD"]);

        app.git.log.pending_revert = Some(crate::features::git::domain::log::Commit {
            hash: head.trim().to_string(),
            parents: vec![],
            author: "Test User".into(),
            date: String::new(),
            refs: vec![],
            subject: "initial".into(),
        });
        app.apply(Action::Git(GitAction::ConfirmRevert), &ctx);
        settle(&mut app, &ctx);

        assert_eq!(git_output(&root, &["rev-parse", "HEAD"]), head);
        assert!(app.git.error.contains("revert"), "{}", app.git.error);
        assert!(app.git.log.pending_revert.is_none());
    }
```

- [ ] Step 9: In `GUI.md`, add a bullet after the Conflicts bullet (line 47):

```markdown
- **Log.** The clock button next to the branch dropdown opens the git log at the bottom of the window, and whether it is open is remembered. It lists one branch's commits, 500 at a time with Load more at the end, each with its graph, branch and tag labels, subject, author and date. Merge commits are dimmed. The toolbar filters by text or hash (`.*` for a regular expression, `Cc` to match case), Branch (local then remote branches, nothing is checked out), User, Date (last 24 hours, 7 days, 30 days or year) and Paths (the contracts folder or a typed path), and Refresh rereads. Clicking a commit lists its changed files on the right, and clicking a file shows that commit's diff of it in the main area, with Show field changes for a contract. Right-click a commit to Copy hash or Revert it. Revert asks first and adds a commit that undoes it. It is not offered for a merge commit or for another branch than the current one, and it is refused while a contract has unsaved edits. A conflicting revert goes to the conflict resolver.
```

and replace the last line (line 49) with:

```markdown
Not included: push, rebase, stash, cherry-pick, reset, aborting a merge or a revert, and checking out remote branches or commits. Those stay in your terminal.
```

- [ ] Step 10: In `CHANGELOG.md`, add as the first bullet under Unreleased, Added (before line 12):

```markdown
- **Git log panel.** The clock button next to the branch dropdown opens a resizable log at the bottom of the window, remembered across launches. It shows a branch's commits with a painted graph, ref labels, author and date, 500 at a time with Load more, and filters by text or hash, branch, user, date range and path. Clicking a commit lists its changed files, and clicking a file shows that commit's diff in the main area. Right-click a commit to copy its hash or revert it after a confirmation. A conflicting revert lands in the conflict resolver.
```

- [ ] Step 11: Run `cargo test -p apic-gui log_app_`
- [ ] Step 12: Run `cargo fmt --all`, then `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`
- [ ] Step 13: Commit `feat(gui): open the git log from the app bar and remember it`

## Coverage map

| Spec goal | Tasks |
|---|---|
| 1. Clock button, bottom panel, persisted open flag | 3 (open flag, ToggleLog), 5 (button, `Panel::bottom`, settings) |
| 2. Branch commits, 500 per page with Load more, row contents, dimmed merges, virtualised rows | 1 (Commit, parser), 2 (`cli::log` paging), 3 (Load more, branch reset after a switch), 4 (`show_rows`, pills, dimming) |
| 3. Graph from `%H %P`, dots only under text or User, lines under Paths with rewritten parents | 1 (`lanes`, `dots_only`), 2 (`--parents`), 4 (`paint_graph`) |
| 4. Toolbar filters mapped onto `git log`, hash prefix, debounce, Refresh, reread from the start | 1 (`LogFilter::args`, `path`, `hash_prefix`, `keeps`), 2 (flags against real git), 3 (`SetLogFilter`, `RefreshLog`), 4 (toolbar, `debounced`) |
| 5. Commit files nested by folder, file diff in the central area with Show field changes | 2 (`commit_files`, `commit_diff_text`), 3 (jobs, `SelectCommit`, `SelectCommitFile`), 4 (`RowMode::Commit`, `diff_body`), 5 (`CentralView::Git`) |
| 6. Right-click Revert and Copy hash, disabled reasons, confirm popup, revert mutation, conflict notice, unsaved-edits refusal | 2 (`cli::revert`), 3 (`Mutation::Revert`, `revert_block`, conflict reread), 4 (context menu, `revert_dialog`), 5 (guard arm) |
| 7. Error keeps rows, empty message, refresh after commit, revert, merge, pull and switch | 3 (`Log` error arm, `request_log` after mutations), 4 (empty state text, error label) |
| 8. GUI.md and CHANGELOG.md | 5 |
