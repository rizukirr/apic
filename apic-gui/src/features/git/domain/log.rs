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
        let grep = !self.text.trim().is_empty() && self.hash_prefix().is_none();
        if grep {
            args.push(format!("--grep={}", self.text));
            args.push(if self.regex { "-E" } else { "-F" }.to_string());
            if !self.match_case {
                args.push("-i".to_string());
            }
        }
        if let Some(author) = &self.author {
            // `--author` follows the grep's pattern mode: literal under
            // `-F`, extended under `-E`, basic otherwise. Escape the picked
            // name so it only ever matches itself.
            let special = match (grep, self.regex) {
                (true, false) => "",
                (true, true) => ".[*^$\\+?(){|",
                (false, _) => ".[*^$\\",
            };
            let mut escaped = String::new();
            for c in author.chars() {
                if special.contains(c) {
                    escaped.push('\\');
                }
                escaped.push(c);
            }
            args.push(format!("--author={escaped}"));
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
        .as_chunks::<6>()
        .0
        .iter()
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
        .as_chunks::<2>()
        .0
        .iter()
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
        let bytes = "h1\x00p1 p2\x00Ann\x002026-11-08 10:00\x00HEAD -> main, tag: v1\x00fix ✓ ünï\x00h2\x00\x00Bob\x002026-11-07 09:00\x00\x00root\x00";
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
        assert!(
            files
                .iter()
                .all(|f| f.worktree == Change::Unmodified && !f.conflicted)
        );
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
            vec![
                row(0, &[], &[], &[0]),
                row(0, &[], &[0], &[0]),
                row(0, &[], &[0], &[])
            ]
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
        assert_eq!(
            with(PathFilter::Contracts).path("contracts"),
            Some("contracts")
        );
        assert_eq!(with(PathFilter::Contracts).path(""), None);
        assert_eq!(
            with(PathFilter::Typed(" a/b ".into())).path(""),
            Some("a/b")
        );
        assert_eq!(with(PathFilter::Typed("  ".into())).path(""), None);
    }

    #[test]
    fn since_offers_every_range_with_its_flag() {
        let got: Vec<(&str, Option<&str>)> =
            Since::ALL.iter().map(|s| (s.label(), s.arg())).collect();
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
