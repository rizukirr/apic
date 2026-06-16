//! Command-line interface: argument parsing and subcommand handlers.

use crate::config::{Config, InitOutcome, read_config_file};
use crate::file::{confine_to_dir, read_file, to_slash};
use crate::fuzzy::{fuzzy_find, fuzzy_match_path};
use crate::json::{json_get, scan_json_file, validate as validate_contract};
use crate::picker;
use crate::render::{render, sanitize};
use crate::tree;
use clap::{Parser, Subcommand};
use std::fs;
use std::io::{IsTerminal, Write};
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
        #[arg(long, short = 'f', value_name = "QUERY")]
        find: String,

        /// Show only the response with this HTTP status code (e.g. `401`).
        #[arg(long, short = 's', value_name = "CODE")]
        status: Option<u16>,

        /// Show the raw JSON example payloads for the request and responses
        /// instead of the schema tables.
        #[arg(long, short = 'e')]
        example: bool,
    },
    /// Scaffold a new contract and edit it in the interactive TUI.
    ///
    /// Opens a full-screen editor seeded from the project template's structure;
    /// the file is written when you save. Pass `--editor` to scaffold the file
    /// to disk and open it in `$VISUAL`/`$EDITOR` instead. The path is resolved
    /// against the working directory and confined to it; a `..` escape or an
    /// absolute path elsewhere is rejected. Refuses to overwrite an existing file.
    Create {
        /// Path for the new contract, relative to the working directory
        /// (e.g. `auth/login.json`).
        #[arg(long, short = 'f', value_name = "FILENAME")]
        filename: Option<String>,

        /// Edit in this external editor (e.g. `nvim` or `"code --wait"`) instead
        /// of the built-in TUI, overriding `$VISUAL` and `$EDITOR`.
        #[arg(long, short = 'e', value_name = "EDITOR")]
        editor: Option<String>,
    },
    /// Check that contracts parse and conform to the schema.
    ///
    /// With no query, every contract under the working directory is checked.
    /// A query ending in `/` (e.g. `auth/`) validates every contract under that
    /// folder, recursively; otherwise the query resolves to a single contract
    /// like `read` (path, extensionless, or fuzzy). Prints `ok`/`FAIL` per file
    /// and exits non-zero if any contract is invalid, so it can gate CI or a
    /// pre-commit hook.
    Validate {
        /// Validate a single contract (path, extensionless, or fuzzy), or every
        /// contract under a folder when the query ends in `/`. Omit to check
        /// every contract.
        #[arg(long, short = 'f', value_name = "QUERY", conflicts_with = "template")]
        find: Option<String>,

        /// Validate the project template (`.apic/template.json`) instead of
        /// contracts. Reports `ok`/`FAIL` and exits non-zero on failure.
        #[arg(long, conflicts_with = "find")]
        template: bool,
    },
    /// Edit an existing contract in the interactive TUI.
    ///
    /// Resolves the filename like `read` (exact, extensionless, or fuzzy) and
    /// opens it in the full-screen editor. Pass `--editor` to use your external
    /// editor instead. Uses the same editor resolution as `create`.
    ///
    /// Pass `--template` instead of a filename to edit the project template
    /// (`.apic/template.json`) that `apic create` scaffolds from.
    Open {
        /// Contract to open — path, extensionless path, or fuzzy fragment.
        /// Required unless `--template` is given.
        #[arg(
            long,
            short = 'f',
            value_name = "QUERY",
            required_unless_present = "template",
            conflicts_with = "template"
        )]
        find: Option<String>,

        /// Edit in this external editor (e.g. `nvim` or `"code --wait"`) instead
        /// of the built-in TUI, overriding `$VISUAL` and `$EDITOR`.
        #[arg(long, short = 'e', value_name = "EDITOR")]
        editor: Option<String>,

        /// Open the project template (`.apic/template.json`) instead of a
        /// contract, seeding it from the default if it does not exist yet.
        #[arg(long)]
        template: bool,
    },
    /// Delete a contract file.
    ///
    /// The filename is resolved like `read`: an exact path (`user/user.json`),
    /// without the `.json` extension (`user/user`), or a fuzzy fragment
    /// (`user`), prompting to pick when ambiguous. On an interactive terminal
    /// it asks for confirmation before deleting; in scripts it removes without
    /// prompting.
    Remove {
        /// Contract to remove — path, extensionless path, or fuzzy fragment.
        #[arg(long, short = 'f', value_name = "QUERY")]
        find: String,
    },
    /// Import a Postman collection as apic contract files.
    ///
    /// Reads a Postman Collection export (v1.0.0 / v2.0.0 / v2.1.0) and writes
    /// one JSON contract per request, mirroring the collection's folder nesting.
    /// Files are written under `--destination`, resolved within the configured
    /// working directory; when omitted, the working directory itself is used.
    /// `..` escapes and absolute paths elsewhere are rejected, and existing
    /// files are never overwritten. Requires an initialized apic project
    /// (`apic init`).
    Convert {
        /// Path to the Postman collection JSON file to import.
        #[arg(long, value_name = "FILE")]
        postman: PathBuf,

        /// Destination directory for the generated contracts, relative to the
        /// working directory (created if missing). Defaults to the working
        /// directory from `.apic/config.toml`.
        #[arg(long, value_name = "DIR")]
        destination: Option<String>,
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

/// Initializes a new `.apic` project, optionally pointing at `working_dir`.
///
/// The directory creation and config write are delegated to [`Config::init`].
/// On an already-initialized project a missing `template.json` is seeded
/// rather than erroring.
pub fn init_cmd(working_dir: Option<&str>) -> Result<(), String> {
    match Config::init(working_dir)? {
        InitOutcome::Initialized => println!("Successfully initialized"),
        InitOutcome::TemplateSeeded => {
            println!("Already initialized; created the missing template")
        }
    }
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
    let file_str: Vec<String> = files.iter().map(|f| to_slash(f)).collect();
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
    sanitize(&to_slash(shown))
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
/// A `find` query ending in `/` validates every contract under that folder,
/// recursively. Otherwise the reference is resolved like `read` — exact path,
/// basename, then fuzzy, prompting when ambiguous; with no query every contract
/// is checked. Each file is read (subject to the size cap) and parsed against
/// the contract schema. Prints `ok`/`FAIL` per file and a summary, and exits
/// the process non-zero if any contract is invalid so it can gate CI.
fn validate_cmd(template: bool, find: Option<&str>) -> Result<(), String> {
    if template {
        return validate_template_cmd();
    }

    let files = match list(true) {
        Some(files) => files,
        None => {
            println!("No contracts found");
            return Ok(());
        }
    };

    let root = read_config_file().and_then(|c| c.get_root_dir()).ok();

    let targets: Vec<PathBuf> = match find {
        // Folder mode: a query ending in `/` validates every contract beneath
        // that directory, at any depth.
        Some(name) if name.ends_with('/') => {
            let base = root
                .clone()
                .ok_or("Not in an apic project (run `apic init`)")?;
            let dir = confine_to_dir(&base, Path::new(name))?;
            if !dir.is_dir() {
                eprintln!("No such folder: {}", sanitize(name));
                std::process::exit(1);
            }
            let in_dir: Vec<PathBuf> = files
                .iter()
                .filter(|f| f.starts_with(&dir))
                .cloned()
                .collect();
            if in_dir.is_empty() {
                println!("No contracts found under {}", sanitize(name));
                return Ok(());
            }
            in_dir
        }
        // Single contract resolved like `read`.
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

    // Exclude anything inside `.apic/` (notably `template.json`) from the scan:
    // the template may be a partial that is not a valid stand-alone contract, and
    // it has its own check via `apic validate --template`.
    let targets: Vec<PathBuf> = match crate::config::find_apic_dir() {
        Some(apic_dir) => targets
            .into_iter()
            .filter(|p| !p.starts_with(&apic_dir))
            .collect(),
        None => targets,
    };

    // Template-conformance rules from `.apic/template.json`, loaded once and
    // reused for all targets; they enforce nothing when the template is absent.
    let rules = crate::template::load_rules()?;

    let mut failed = 0usize;
    for path in &targets {
        let shown = root
            .as_ref()
            .and_then(|r| path.strip_prefix(r).ok())
            .unwrap_or(path);
        // Normalize to forward slashes so output matches the rest of apic and is
        // stable across platforms (Windows would otherwise show backslashes).
        let shown = sanitize(&to_slash(shown));

        let result = read_file(path)
            .map_err(|err| err.to_string())
            .and_then(|content| {
                validate_contract(&content).map_err(|err| err.to_string())?;
                let issues = rules.check(&content)?;
                if issues.is_empty() {
                    Ok(())
                } else {
                    Err(issues.join("; "))
                }
            });

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

/// Validates the project template (`.apic/template.json`) for `apic validate
/// --template`.
///
/// Prints `ok`/`FAIL` using the same convention as contract validation and
/// exits non-zero when the template is invalid, so it can gate CI. A missing
/// template is not a failure — `create` would use the built-in default.
fn validate_template_cmd() -> Result<(), String> {
    match crate::template::check_template() {
        crate::template::TemplateCheck::Absent => {
            println!("No project template found; create will use the built-in template");
            Ok(())
        }
        crate::template::TemplateCheck::Valid => {
            println!("ok   .apic/template.json");
            Ok(())
        }
        crate::template::TemplateCheck::Invalid(reason) => {
            println!("FAIL .apic/template.json: {}", sanitize(&reason));
            std::process::exit(1);
        }
    }
}

/// Creates a new contract. Without `--editor` the interactive TUI is opened,
/// seeded from the project template's structure; with `--editor` the contract
/// is scaffolded to disk and opened in the external editor (legacy behavior).
///
/// Inside an initialized project the `filename` is resolved against the working
/// directory and confined to it; a `..` escape or absolute path elsewhere is
/// rejected. Refuses to overwrite an existing file.
fn create_cmd(filename: &str, editor: Option<&str>) -> Result<(), String> {
    let path = match read_config_file().and_then(|conf| conf.get_root_dir()) {
        Ok(root) => confine_to_dir(&root, Path::new(filename))?,
        Err(_) => PathBuf::from(filename),
    };

    if path.exists() {
        return Err(format!("{} already exists", path.display()));
    }

    if editor.is_some() {
        // Legacy path: scaffold to disk, then open the external editor.
        let contract = crate::template::resolve_for_create()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("Failed to create {}: {}", parent.display(), err))?;
        }
        fs::write(&path, contract)
            .map_err(|err| format!("Failed to write {}: {}", path.display(), err))?;
        println!("Created {}", sanitize(&path.to_string_lossy()));
        return open_in_editor(&path, editor)
            .map_err(|err| format!("Failed to open editor: {err}"));
    }

    // Default path: seed an EditModel and open the TUI. The file is written only
    // when the user saves inside the TUI.
    let overlay = read_project_template();
    let model = crate::tui::seed_model(overlay.as_deref())?;
    crate::tui::run(model, &path)
}

/// Reads `.apic/template.json` if present, for seeding the create TUI.
fn read_project_template() -> Option<String> {
    let apic_dir = crate::config::find_apic_dir()?;
    crate::template::seed_if_missing(&apic_dir).ok()?;
    fs::read_to_string(crate::template::path(&apic_dir)).ok()
}

/// Opens a contract for editing, or the project template when `template` is set.
///
/// Without `--editor` the interactive TUI is opened on the parsed contract;
/// with `--editor` the file is opened in the external editor (legacy behavior).
/// With `--template` the project's `.apic/template.json` is targeted, seeded
/// from the default first if it does not exist. The TUI is seeded like `create`
/// (template values over blanked builtin structure), so it shows the template's
/// own schema without the builtin's placeholder headers, fields, or examples.
fn open_cmd(template: bool, filename: Option<&str>, editor: Option<&str>) -> Result<(), String> {
    if template {
        let apic_dir =
            crate::config::find_apic_dir().ok_or("Not in an apic project (run `apic init`)")?;
        crate::template::seed_if_missing(&apic_dir)?;
        let path = crate::template::path(&apic_dir);
        if editor.is_some() {
            return open_in_editor(&path, editor)
                .map_err(|err| format!("Failed to open editor: {err}"));
        }
        // Seed the model from the template exactly like `create`: the project
        // template's own values plus the builtin's scalar defaults for
        // name/description/url, with the builtin's arrays and examples blanked.
        // This keeps the template's schema while dropping the builtin's
        // placeholder headers, schema fields, and examples.
        let overlay =
            read_file(&path).map_err(|err| format!("Failed to read {}: {err}", path.display()))?;
        let model = crate::tui::seed_model(Some(&overlay))
            .map_err(|reason| format!("{}: {reason}", path.display()))?;
        return crate::tui::run(model, &path);
    }

    // The parser requires `-f` unless `--template` is given, so this is safe.
    let filename = filename.expect("a find query is required without --template");
    match resolve_one(filename)? {
        Resolved::Path(path) => {
            if editor.is_some() {
                open_in_editor(&path, editor).map_err(|err| format!("Failed to open editor: {err}"))
            } else {
                open_path_in_tui(&path)
            }
        }
        Resolved::Cancelled => cancelled(),
        Resolved::NotFound => Err(format!("No contract found matching '{filename}'")),
    }
}

/// Reads, parses, and edits an existing contract file in the TUI.
fn open_path_in_tui(path: &Path) -> Result<(), String> {
    let text =
        read_file(path).map_err(|err| format!("Failed to read {}: {err}", path.display()))?;
    let contract = json_get(&text, None)
        .map_err(|err| format!("{} is not a valid contract: {err}", path.display()))?;
    let model = crate::tui::EditModel::from_contract(contract);
    crate::tui::run(model, path)
}

/// Resolves `filename` to one contract and deletes it after confirmation.
///
/// Resolution matches `read`/`open` (exact path, basename, then fuzzy, with the
/// interactive picker when ambiguous). On an interactive terminal the user is
/// asked to confirm; without a terminal (scripts) the deletion proceeds.
fn remove_cmd(filename: &str) -> Result<(), String> {
    match resolve_one(filename)? {
        Resolved::Path(path) => {
            let root = read_config_file().and_then(|c| c.get_root_dir())?;
            let shown = rel_display(&path, &root);
            if !confirm(&format!("Remove {shown}?"))? {
                return cancelled();
            }
            fs::remove_file(&path).map_err(|err| format!("Failed to remove {shown}: {err}"))?;
            println!("Removed {shown}");
            Ok(())
        }
        Resolved::Cancelled => cancelled(),
        Resolved::NotFound => Err(format!("No contract found matching '{filename}'")),
    }
}

/// Handles `apic convert`: resolve the destination under the working directory,
/// then parse the Postman collection and write contracts.
fn convert_cmd(postman: &Path, destination: Option<&str>) -> Result<(), String> {
    let root = read_config_file().and_then(|conf| conf.get_root_dir())?;
    let dest_base = match destination {
        Some(dir) => confine_to_dir(&root, Path::new(dir))?,
        None => root,
    };
    crate::convert::run(postman, &dest_base)
}

/// Asks the user `prompt` and returns whether they confirmed (default no).
///
/// Only prompts on an interactive terminal: when stdin or stdout is not a TTY
/// there is no one to answer, so the action proceeds. A leading `y`/`yes`
/// (case-insensitive) confirms; anything else declines.
fn confirm(prompt: &str) -> Result<bool, String> {
    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        return Ok(true);
    }

    print!("{prompt} [y/N] ");
    std::io::stdout()
        .flush()
        .map_err(|err| format!("Failed to write prompt: {err}"))?;

    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .map_err(|err| format!("Failed to read input: {err}"))?;

    let answer = answer.trim().to_lowercase();
    Ok(answer == "y" || answer == "yes")
}

/// Opens `path` in the user's preferred editor and waits for it to close.
///
/// Resolves the editor from the explicit `editor` argument (the `--editor`
/// flag), then `$VISUAL`, then `$EDITOR`, falling back to `vi`. The `--editor`
/// flag takes precedence over the environment. Extra arguments in the value
/// (e.g. `code --wait`) are honored.
fn open_in_editor(path: &Path, editor: Option<&str>) -> std::io::Result<()> {
    let user_editor = editor
        .map(String::from)
        .or_else(|| std::env::var("VISUAL").ok())
        .or_else(|| std::env::var("EDITOR").ok())
        .unwrap_or_else(|| String::from("vi"));

    let mut parts = user_editor.split_whitespace();
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

/// List files in the current working directory, print it as a tree.
///
/// If `filter` is given, only files whose path fuzzy-matches it are printed.
/// If `absolute` is true, the working directory is not prepended to the path.
fn list_cmd(filter: Option<&str>, absolute: bool) -> Result<(), String> {
    if let Some(files) = list(absolute) {
        // Fuzzy-match the filter against the sanitized, working-dir-
        // relative form so an absolute prefix can't skew scores and
        // the match indices stay aligned with what is printed. File
        // names come from the filesystem and may carry control
        // characters, so they are sanitized before matching.
        struct Row {
            /// Working-dir-relative display path (tree view).
            rel: String,
            /// Match positions in `rel`; unused when piped (the cost
            /// is bounded by the query length).
            indices: Vec<usize>,
            score: i32,
            /// Path in the requested form (flat piped view).
            shown: String,
        }

        let is_tty = std::io::stdout().is_terminal();
        let root = read_config_file().and_then(|c| c.get_root_dir()).ok();
        let mut rows: Vec<Row> = files
            .iter()
            .filter_map(|file| {
                let rel = root
                    .as_ref()
                    .and_then(|r| file.strip_prefix(r).ok())
                    .unwrap_or(file);
                let rel = sanitize(&to_slash(rel));
                let (score, indices) = match &filter {
                    Some(query) => fuzzy_match_path(query, &rel)?,
                    None => (0, Vec::new()),
                };
                let shown = sanitize(&to_slash(file));
                Some(Row {
                    rel,
                    indices,
                    score,
                    shown,
                })
            })
            .collect();

        if rows.is_empty() {
            // A filter that matches nothing prints nothing — also
            // skips the `--absolute` root label.
        } else if is_tty {
            // Tree view: alphabetical, directories first; under a
            // filter only matching files appear, with matched
            // characters highlighted.
            let mut tree_root = tree::Node::default();
            for row in &rows {
                tree_root.insert(Path::new(&row.rel), &row.indices);
            }
            let root_label = if absolute {
                root.as_ref()
                    .map(|r| format!("{}/", sanitize(&to_slash(r))))
            } else {
                None
            };
            print!("{}", tree::render(root_label.as_deref(), &tree_root, true));
        } else {
            // Piped: flat path-per-line for scripts. With a filter,
            // print the best match first.
            if filter.is_some() {
                rows.sort_by_key(|row| std::cmp::Reverse(row.score));
            }
            for row in rows {
                println!("{}", row.shown);
            }
        }
    }
    Ok(())
}

/// Parses command-line arguments and runs the selected subcommand.
///
/// This is the CLI entry point invoked from `main`.
pub fn run() {
    let cli = Cli::parse();
    let result: Result<(), String> = match cli.command {
        Commands::Config { set_dir } => update_working_dir(set_dir.as_deref()),
        Commands::Create { filename, editor } => match filename {
            Some(filename) => create_cmd(&filename, editor.as_deref()),
            None => Err("no filename provided, use 'apic create -f <filename>'".to_string()),
        },
        Commands::Init { set_dir } => init_cmd(set_dir.as_deref()),
        Commands::List { filter, absolute } => list_cmd(filter.as_deref(), absolute),
        Commands::Read {
            find,
            status,
            example,
        } => read_cmd(&find, status, example),
        Commands::Validate { find, template } => validate_cmd(template, find.as_deref()),
        Commands::Open {
            find,
            editor,
            template,
        } => open_cmd(template, find.as_deref(), editor.as_deref()),
        Commands::Remove { find } => remove_cmd(&find),
        Commands::Convert {
            postman,
            destination,
        } => convert_cmd(&postman, destination.as_deref()),
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
