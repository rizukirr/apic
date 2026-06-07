//! Command-line interface: argument parsing and subcommand handlers.

use crate::config::{Config, configured_editor, read_config_file};
use crate::file::{confine_to_dir, read_file};
use crate::fuzzy::{fuzzy_find, fuzzy_match_path};
use crate::json::{json_get, scan_json_file, validate as validate_contract};
use crate::picker;
use crate::render::{render, sanitize};
use crate::tree;
use clap::{Parser, Subcommand};
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

/// Top-level CLI parser for the `apic` binary.
#[derive(Debug, Parser)]
#[command(name = "apic")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(author = "rizukirr")]
#[command(about = "Git-able API contracts — per-endpoint JSON files in your repo")]
#[command(
    long_about = "apic stores API contracts as plain per-endpoint JSON files in your \
repository, so they are versioned, diffable, and reviewable alongside code.\n\n\
Typical flow: `apic init` to set up a project, `apic config --set-dir <dir>` to point at your \
contracts folder, then `create`, `open`, `read`, and `validate` to work with contracts."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// The subcommands accepted by `apic`.
#[derive(Debug, Subcommand)]
enum Commands {
    /// Change project settings in `.apic/config.toml`.
    ///
    /// Each flag is optional; given together they are all applied. With no
    /// flags, nothing changes.
    Config {
        /// Set the contracts working directory, relative to the project root
        /// (the directory must already exist).
        #[arg(long, value_name = "DIR")]
        set_dir: Option<String>,

        /// Set the editor used by `create` and `open`, e.g. `nvim` or
        /// `"code --wait"`. Your $VISUAL/$EDITOR still take precedence.
        #[arg(long, value_name = "EDITOR")]
        set_editor: Option<String>,
    },
    /// Initialize an apic project in the current directory.
    ///
    /// Creates `.apic/config.toml`. Run this once at the root of your repo.
    Init {
        /// Folder that holds your contract files, relative to here. Defaults to
        /// the current directory.
        #[arg(long, value_name = "DIR")]
        set_dir: Option<String>,
    },
    /// List discovered contract files under the working directory.
    List {
        /// Show only contracts whose path fuzzy-matches this query, best match
        /// first (e.g. `--filter user` matches `user/user.json`).
        #[arg(long, value_name = "QUERY")]
        filter: Option<String>,

        /// Print absolute paths (`true`) or paths relative to the working
        /// directory (`false`, the default).
        #[arg(long, value_name = "BOOL", default_value_t = false, action = clap::ArgAction::Set)]
        absolute: bool,
    },
    /// Render a contract as formatted, colorized tables.
    ///
    /// The filename is resolved flexibly: an exact path (`user/user.json`), the
    /// same without the `.json` extension (`user/user`, `auth/login`), or a
    /// fuzzy fragment (`user`, `logn`). Exact matches win over fuzzy ones.
    Read {
        /// Contract to read — path, extensionless path, or fuzzy fragment.
        #[arg(long, short = 'f', value_name = "FILENAME")]
        filename: String,

        /// Show only the response with this HTTP status code (e.g. `401`).
        #[arg(long, short = 's', value_name = "CODE")]
        status: Option<u16>,

        /// Show the raw JSON example payloads for the request and responses
        /// instead of the schema tables.
        #[arg(long, short = 'e')]
        example: bool,
    },
    /// Scaffold a new contract from a template and open it in your editor.
    ///
    /// The path is resolved against the working directory and confined to it;
    /// a `..` escape or an absolute path elsewhere is rejected. Refuses to
    /// overwrite an existing file.
    Create {
        /// Path for the new contract, relative to the working directory
        /// (e.g. `auth/login.json`).
        #[arg(long, short = 'f', value_name = "FILENAME")]
        filename: Option<String>,
    },
    /// Check that contracts parse and conform to the schema.
    ///
    /// With no filename, every contract under the working directory is checked.
    /// Prints `ok`/`FAIL` per file and exits non-zero if any contract is
    /// invalid, so it can gate CI or a pre-commit hook.
    Validate {
        /// Validate only this contract (path, extensionless, or fuzzy). Omit to
        /// check every contract.
        #[arg(long, short = 'f', value_name = "FILENAME")]
        filename: Option<String>,
    },
    /// Open an existing contract in your editor.
    ///
    /// The filename is resolved like `read`: an exact path (`user/user.json`),
    /// without the `.json` extension (`user/user`), or a fuzzy fragment
    /// (`user`). Uses the same editor resolution as `create`.
    Open {
        /// Contract to open — path, extensionless path, or fuzzy fragment.
        #[arg(long, short = 'f', value_name = "FILENAME")]
        filename: String,
    },
}

/// Updates the configured root working directory.
///
/// A `None` value is a no-op. Prints a success message on change; returns the
/// error message on failure so the caller can set a non-zero exit code.
pub fn update_working_dir(working_dir: Option<&str>) -> Result<(), String> {
    match working_dir {
        Some(dir) => {
            read_config_file().and_then(|mut conf| conf.update_root_dir(dir))?;
            println!("Successfully updated");
            Ok(())
        }
        None => Ok(()),
    }
}

/// Updates the configured editor.
///
/// A `None` value is a no-op. Prints a success message on change; returns the
/// error message on failure so the caller can set a non-zero exit code.
pub fn update_editor(editor: Option<&str>) -> Result<(), String> {
    match editor {
        Some(editor) => {
            read_config_file().and_then(|mut conf| conf.update_editor(editor))?;
            println!("Successfully updated");
            Ok(())
        }
        None => Ok(()),
    }
}

/// Initializes a new `.apic` project, optionally pointing at `working_dir`.
///
/// The directory creation and config write are delegated to [`Config::init`].
// TODO: scan json files and store them in a cache
// TODO: for example call read function then store it
pub fn init(working_dir: Option<&str>) -> Result<(), String> {
    Config::init(working_dir)?;
    println!("Successfully initialized");
    Ok(())
}

/// Lists JSON contract files under the configured root directory.
///
/// Returns `None` when no files are found. If the project is not initialized,
/// an error is printed and the process exits.
pub fn list(is_absolute: bool) -> Option<Vec<PathBuf>> {
    let root = match read_config_file().and_then(|conf| conf.get_root_dir()) {
        Ok(root) => root,
        Err(err) => {
            eprintln!("{}", err);
            std::process::exit(1);
        }
    };

    scan_json_file(&root, is_absolute)
}

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
    // exact file under the working directory, with or without `.json`.
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

    // basename ties, bare names only — a query with a separator already
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

    // fuzzy fallback with tie detection on the top score.
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

/// Reports a cancelled interactive pick; cancelling is not an error.
fn cancelled() -> Result<(), String> {
    println!("cancelled");
    Ok(())
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
            let labels: Vec<String> = candidates.iter().map(|c| rel_display(c, &root)).collect();
            if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
                // Non-interactive: fail loudly with every candidate and a hint.
                let mut msg = format!(
                    "'{}' is ambiguous, {} contracts match:\n",
                    sanitize(filename),
                    labels.len()
                );
                for label in &labels {
                    msg.push_str(&format!("  {label}\n"));
                }
                msg.push_str(&format!("Specify the path, e.g. -f {}", labels[0]));
                return Err(msg);
            }
            let prompt = format!(
                "{} contracts match \"{}\":",
                candidates.len(),
                sanitize(filename)
            );
            match picker::pick(&prompt, &labels).map_err(|err| format!("picker failed: {err}"))? {
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
        Resolved::Cancelled => cancelled(),
        Resolved::NotFound => {
            println!("No contract found");
            Ok(())
        }
    }
}

/// Parses `content` as a JSON contract, keeps only the responses whose code
/// matches `status` (or all responses when `status` is `None`), and renders
/// the resulting contract as formatted text. With `example`, the request and
/// response sections show their raw JSON example payloads instead of tables.
///
/// Parse errors are printed rather than returned. When a `status` filter
/// matches no response, a note is printed so the empty output is not mistaken
/// for a contract without responses.
fn read(content: &str, status: Option<u16>, example: bool) -> Result<(), String> {
    match json_get(content, status) {
        Ok(contract) => {
            render(&contract, example);
            if let Some(status) = status
                && contract.responses.is_empty()
            {
                println!("\n No response with status {status}");
            }
            Ok(())
        }
        Err(err) => Err(err.to_string()),
    }
}

/// Validates contracts under the working directory, printing one line per file.
///
/// With `filename`, the reference is resolved like `read` — exact path,
/// basename, then fuzzy, prompting when ambiguous; otherwise every contract
/// is checked. Each file is read (subject to the size cap) and parsed against
/// the contract schema. Prints `ok`/`FAIL` per file and a summary, and exits
/// the process non-zero if any contract is invalid so it can gate CI.
fn validate(filename: Option<&str>) -> Result<(), String> {
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
            Resolved::Cancelled => return cancelled(),
            Resolved::NotFound => {
                eprintln!("No contract matches {}", sanitize(name));
                std::process::exit(1);
            }
        },
        None => files,
    };

    let mut failed = 0usize;
    for path in &targets {
        let shown = root
            .as_ref()
            .and_then(|r| path.strip_prefix(r).ok())
            .unwrap_or(path);
        let shown = sanitize(&shown.to_string_lossy());

        let result = read_file(path)
            .map_err(|err| err.to_string())
            .and_then(|content| validate_contract(&content).map_err(|err| err.to_string()));

        match result {
            Ok(()) => println!("ok   {shown}"),
            Err(err) => {
                println!("FAIL {shown}: {}", sanitize(&err));
                failed += 1;
            }
        }
    }

    println!("\n{} passed, {} failed", targets.len() - failed, failed);
    if failed > 0 {
        std::process::exit(1);
    }

    Ok(())
}

/// Default contract template written by `apic create`.
///
/// Embedded at compile time; mirrors the shape of [`crate::json::JsonContent`].
const DEFAULT_CONTRACT: &str = include_str!("templates/contract.json");

/// Creates a new contract file from the default template and opens it in the
/// configured editor.
///
/// Inside an initialized project the `filename` is resolved against the
/// working directory and confined to it: a path that escapes via `..` or an
/// absolute path elsewhere is rejected. Outside a project the path is taken
/// as given. Refuses to overwrite an existing file.
fn create(filename: &str) -> Result<(), String> {
    let path = match read_config_file().and_then(|conf| conf.get_root_dir()) {
        Ok(root) => confine_to_dir(&root, Path::new(filename))?,
        Err(_) => PathBuf::from(filename),
    };

    if path.exists() {
        return Err(format!("{} already exists", path.display()));
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Failed to create {}: {}", parent.display(), err))?;
    }

    fs::write(&path, DEFAULT_CONTRACT)
        .map_err(|err| format!("Failed to write {}: {}", path.display(), err))?;
    println!("Created {}", sanitize(&path.to_string_lossy()));
    open_in_editor(&path).map_err(|err| format!("Failed to open editor: {err}"))?;
    Ok(())
}

/// Resolves `filename` to an existing contract and opens it in the editor.
fn open(filename: &str) -> Result<(), String> {
    match resolve_one(filename)? {
        Resolved::Path(path) => {
            open_in_editor(&path).map_err(|err| format!("Failed to open editor: {err}"))
        }
        Resolved::Cancelled => cancelled(),
        Resolved::NotFound => Err(format!("No contract found matching '{filename}'")),
    }
}

/// Opens `path` in the user's preferred editor and waits for it to close.
///
/// Resolves the editor from `$VISUAL`, then `$EDITOR`, then the `config.toml`
/// editor, falling back to `vi`. Personal environment variables take
/// precedence over the project config, so a shared, committed config can set a
/// team default without overriding anyone's own editor. Extra arguments in the
/// value (e.g. `code --wait`) are honored.
fn open_in_editor(path: &Path) -> std::io::Result<()> {
    let editor = std::env::var("VISUAL")
        .ok()
        .or_else(|| std::env::var("EDITOR").ok())
        .or_else(configured_editor)
        .unwrap_or_else(|| String::from("vi"));

    let mut parts = editor.split_whitespace();
    let program = parts.next().unwrap_or("vi");

    let status = std::process::Command::new(program)
        .args(parts)
        .arg(path)
        .status()?;

    if !status.success() {
        eprintln!("Editor exited with non-zero status: {}", status);
    }
    Ok(())
}

/// Parses command-line arguments and runs the selected subcommand.
///
/// This is the CLI entry point invoked from `main`.
pub fn run() {
    let cli = Cli::parse();
    let result: Result<(), String> = match cli.command {
        Commands::Config {
            set_dir,
            set_editor,
        } => update_working_dir(set_dir.as_deref())
            .and_then(|_| update_editor(set_editor.as_deref())),
        Commands::Create { filename } => match filename {
            Some(filename) => create(&filename),
            None => Err("no filename provided, use 'apic create -f <filename>'".to_string()),
        },
        Commands::Init { set_dir } => init(set_dir.as_deref()),
        Commands::List { filter, absolute } => {
            if let Some(files) = list(absolute) {
                // Fuzzy-match the filter against the sanitized, working-dir-
                // relative form so an absolute prefix can't skew scores and
                // the match indices stay aligned with what is printed. File
                // names come from the filesystem and may carry control
                // characters, so they are sanitized before matching.
                let root = read_config_file().and_then(|c| c.get_root_dir()).ok();
                let mut rows: Vec<(String, Vec<usize>, i32, String)> = files
                    .iter()
                    .filter_map(|file| {
                        let rel = root
                            .as_ref()
                            .and_then(|r| file.strip_prefix(r).ok())
                            .unwrap_or(file);
                        let rel = sanitize(&rel.to_string_lossy());
                        let (score, indices) = match &filter {
                            Some(query) => fuzzy_match_path(query, &rel)?,
                            None => (0, Vec::new()),
                        };
                        let shown = sanitize(&file.to_string_lossy());
                        Some((rel, indices, score, shown))
                    })
                    .collect();

                if rows.is_empty() {
                    // A filter that matches nothing prints nothing — also
                    // skips the `--absolute` root label.
                } else if std::io::stdout().is_terminal() {
                    // Tree view: alphabetical, directories first; under a
                    // filter only matching files appear, with matched
                    // characters highlighted.
                    let mut tree_root = tree::Node::default();
                    for (rel, indices, _, _) in &rows {
                        tree_root.insert(Path::new(rel), indices);
                    }
                    let root_label = if absolute {
                        root.as_ref()
                            .map(|r| format!("{}/", sanitize(&r.to_string_lossy())))
                    } else {
                        None
                    };
                    print!("{}", tree::render(root_label.as_deref(), &tree_root, true));
                } else {
                    // Piped: flat path-per-line for scripts. With a filter,
                    // print the best match first.
                    if filter.is_some() {
                        rows.sort_by_key(|(_, _, score, _)| std::cmp::Reverse(*score));
                    }
                    for (_, _, _, shown) in rows {
                        println!("{shown}");
                    }
                }
            }
            Ok(())
        }
        Commands::Read {
            filename,
            status,
            example,
        } => read_cmd(&filename, status, example),
        // `validate` exits the process itself when contracts fail
        // (per-file reporting); resolution errors return normally.
        Commands::Validate { filename } => validate(filename.as_deref()),
        Commands::Open { filename } => open(&filename),
    };

    if let Err(err) = result {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}

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
        assert!(matches!(classify("qqqq", &root, &files), Resolution::None));
    }
}
