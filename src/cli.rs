//! Command-line interface: argument parsing and subcommand handlers.

use crate::config::{Config, configured_editor, read_config_file};
use crate::file::{confine_to_dir, read_file};
use crate::fuzzy::fuzzy_find;
use crate::json::{JsonScanFileErr, json_get, scan_json_file, validate as validate_contract};
use crate::render::{render, sanitize};
use clap::{Parser, Subcommand};
use std::fs;
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
        /// Truncate reported paths to N components below the working directory.
        /// `0` (the default) prints full paths.
        #[arg(long, value_name = "N")]
        depth: Option<usize>,

        /// Print absolute paths (`true`) or paths relative to the working
        /// directory (`false`).
        #[arg(long, value_name = "BOOL", default_value_t = true, action = clap::ArgAction::Set)]
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
/// `depth` limits how deep paths are reported (`None` means unlimited / `0`).
/// Returns `None` when no files are found. If the project is not initialized
/// or the requested depth exceeds the deepest available file, an error is
/// printed and the process exits.
pub fn list(depth: Option<usize>, is_absolute: bool) -> Option<Vec<PathBuf>> {
    let depth = depth.unwrap_or(0);

    let root = match read_config_file().and_then(|conf| conf.get_root_dir()) {
        Ok(root) => root,
        Err(err) => {
            eprintln!("{}", err);
            std::process::exit(1);
        }
    };

    match scan_json_file(&root, depth, is_absolute) {
        Ok(files) => Some(files),
        Err(JsonScanFileErr::NotFound) => None,
        Err(JsonScanFileErr::DepthTooLarge { requested, max }) => {
            eprintln!("Error: depth={} exceeds max depth of {}", requested, max);
            std::process::exit(1);
        }
    }
}

/// Resolves a contract reference to an existing file path under the working dir.
///
/// Resolution tries, in order:
/// 1. an exact path relative to the working directory (`user/user.json`),
/// 2. the same with a `.json` extension appended (`user/user`, `auth/login`),
/// 3. the best fuzzy match over all contracts (`user`, `logn`).
///
/// Exact matches always win over fuzzy ones, so a precise path is never
/// mis-ranked. Returns `None` when nothing resolves or no contracts exist.
pub fn resolve_contract(filename: &str) -> Option<PathBuf> {
    let files = list(None, true)?;

    // 1 & 2: exact file under the working directory, with or without `.json`.
    if let Ok(root) = read_config_file().and_then(|c| c.get_root_dir()) {
        let candidates = [
            PathBuf::from(filename),
            PathBuf::from(format!("{filename}.json")),
        ];
        for candidate in candidates {
            if let Ok(path) = confine_to_dir(&root, &candidate)
                && path.is_file()
            {
                return Some(path);
            }
        }
    }

    // 3: fuzzy fallback over every discovered contract.
    let file_str: Vec<String> = files
        .iter()
        .map(|f| f.to_string_lossy().to_string())
        .collect();
    let hits = fuzzy_find(filename, &file_str)?;
    Some(PathBuf::from(&hits[0].0))
}

/// Resolves `filename` to a contract and returns its content.
///
/// `None` is returned when no file resolves or the file cannot be read.
pub fn read_filename(filename: &str) -> Option<String> {
    let path = resolve_contract(filename)?;
    match read_file(&path) {
        Ok(content) => Some(content),
        Err(err) => {
            eprintln!("Failed to read {}: {}", path.display(), err);
            None
        }
    }
}

/// Parses `content` as a JSON contract, keeps only the responses whose code
/// matches `status` (or all responses when `status` is `None`), and renders
/// the resulting contract as formatted text.
///
/// Parse errors are printed rather than returned. When a `status` filter
/// matches no response, a note is printed so the empty output is not mistaken
/// for a contract without responses.
fn read(content: &str, status: Option<u16>) -> Result<(), String> {
    match json_get(content, status) {
        Ok(contract) => {
            render(&contract);
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
/// With `filename`, only the best fuzzy match is checked; otherwise every
/// contract is checked. Each file is read (subject to the size cap) and parsed
/// against the contract schema. Prints `ok`/`FAIL` per file and a summary, and
/// exits the process non-zero if any contract is invalid so it can gate CI.
fn validate(filename: Option<&str>) {
    let files = match list(None, true) {
        Some(files) => files,
        None => {
            println!("No contracts found");
            return;
        }
    };

    let root = read_config_file().and_then(|c| c.get_root_dir()).ok();

    // Narrow to a single fuzzy match when a filename is given.
    let targets: Vec<PathBuf> = match filename {
        Some(name) => {
            let strs: Vec<String> = files
                .iter()
                .map(|f| f.to_string_lossy().to_string())
                .collect();
            match fuzzy_find(name, &strs) {
                Some(hits) => vec![PathBuf::from(&hits[0].0)],
                None => {
                    eprintln!("No contract matches {name}");
                    std::process::exit(1);
                }
            }
        }
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
    let path = resolve_contract(filename)
        .ok_or_else(|| format!("No contract found matching '{filename}'"))?;
    open_in_editor(&path).map_err(|err| format!("Failed to open editor: {err}"))?;
    Ok(())
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
        Commands::List { depth, absolute } => {
            if let Some(files) = list(depth, absolute) {
                for file in files {
                    // File names come from the filesystem and may carry control
                    // characters; strip them before printing to the terminal.
                    println!("{}", sanitize(&file.to_string_lossy()));
                }
            }
            Ok(())
        }
        Commands::Read { filename, status } => match read_filename(&filename) {
            Some(content) => read(content.as_str(), status),
            None => {
                println!("No contract found");
                Ok(())
            }
        },
        // `validate` exits the process itself on failure (per-file reporting).
        Commands::Validate { filename } => {
            validate(filename.as_deref());
            Ok(())
        }
        Commands::Open { filename } => open(&filename),
    };

    if let Err(err) = result {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}
