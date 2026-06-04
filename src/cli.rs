//! Command-line interface: argument parsing and subcommand handlers.

use crate::config::{Config, configured_editor, read_config_file};
use crate::file::read_file;
use crate::fuzzy::fuzzy_find;
use crate::json::{JsonScanFileErr, json_get, scan_json_file};
use crate::render::render;
use clap::{Parser, Subcommand};
use std::fs;
use std::path::{Path, PathBuf};

/// Top-level CLI parser for the `apic` binary.
#[derive(Debug, Parser)]
#[command(name = "apic")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(author = "rizukirr")]
#[command(about = "API contract tools")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// The subcommands accepted by `apic`.
#[derive(Debug, Subcommand)]
enum Commands {
    /// Update configuration, e.g. the working directory or editor.
    Config {
        #[arg(long)]
        set_dir: Option<String>,

        #[arg(long)]
        set_editor: Option<String>,
    },
    /// Initialize an `.apic` project in the current directory.
    Init {
        #[arg(long)]
        set_dir: Option<String>,
    },
    /// List discovered JSON contract files, optionally limited by depth.
    List {
        #[arg(long)]
        depth: Option<usize>,

        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        absolute: bool,
    },
    /// Fuzzy-find a contract file by name and print its contents.
    Read {
        #[arg(long, short = 'f')]
        filename: String,

        #[arg(long, short = 's')]
        status: Option<u16>,
    },
    /// Scaffold a new contract from a template and open it in your editor.
    Create {
        #[arg(long, short = 'f')]
        filename: Option<String>,
    },
}

/// Updates the configured root working directory.
///
/// A `None` value is a no-op; on success or failure a message is printed to
/// stdout/stderr respectively.
pub fn update_working_dir(working_dir: Option<&str>) {
    if let Some(dir) = working_dir {
        let result = read_config_file().and_then(|mut conf| conf.update_root_dir(dir));
        match result {
            Ok(_) => println!("Successfully updated"),
            Err(err) => eprintln!("{}", err),
        }
    }
}

/// Updates the configured editor.
///
/// A `None` value is a no-op; on success or failure a message is printed to
/// stdout/stderr respectively.
pub fn update_editor(editor: Option<&str>) {
    if let Some(editor) = editor {
        let result = read_config_file().and_then(|mut conf| conf.update_editor(editor));
        match result {
            Ok(_) => println!("Successfully updated"),
            Err(err) => eprintln!("{}", err),
        }
    }
}

/// Initializes a new `.apic` project, optionally pointing at `working_dir`.
///
/// Prints a success or error message; the directory creation and config write
/// are delegated to [`Config::init`].
// TODO: scan json files and store them in a cache
// TODO: for example call read function then store it
pub fn init(working_dir: Option<&str>) {
    match Config::init(working_dir) {
        Ok(_) => println!("Successfully initialized"),
        Err(err) => eprintln!("{}", err),
    }
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

/// Fuzzy-finds the contract file best matching `filename` and returns its content.
///
/// The best match (by fuzzy score) is read and its content returned; `None` is
/// returned when no file matches or no contract files exist at all.
pub fn read_filename(filename: &str) -> Option<String> {
    let files = list(None, true)?;

    let file_str: Vec<String> = files
        .iter()
        .map(|f| f.to_string_lossy().to_string())
        .collect();

    let result = fuzzy_find(filename, &file_str);
    if let Some(result) = result {
        let path = Path::new(&result[0].0);
        match read_file(path) {
            Ok(content) => return Some(content),
            Err(err) => {
                eprintln!("Failed to read {}: {}", path.display(), err);
                return None;
            }
        }
    }
    None
}

/// Parses `content` as a JSON contract, keeps only the responses whose code
/// matches `status` (or all responses when `status` is `None`), and renders
/// the resulting contract as formatted text.
///
/// Parse errors are printed rather than returned. When a `status` filter
/// matches no response, a note is printed so the empty output is not mistaken
/// for a contract without responses.
fn read(content: &str, status: Option<u16>) {
    match json_get(content, status) {
        Ok(contract) => {
            render(&contract);
            if let Some(status) = status
                && contract.responses.is_empty()
            {
                println!("\n No response with status {status}");
            }
        }
        Err(err) => {
            eprintln!("Error: {}", err);
        }
    };
}

/// Default contract template written by `apic create`.
///
/// Embedded at compile time; mirrors the shape of [`crate::json::JsonContent`].
const DEFAULT_CONTRACT: &str = include_str!("templates/contract.json");

/// Creates a new contract file from the default template and opens it in the
/// configured editor.
///
/// A relative `filename` is resolved against the configured working directory
/// when the project is initialized; otherwise against the current directory.
/// Refuses to overwrite an existing file.
fn create(filename: &str) -> std::io::Result<()> {
    let mut path = PathBuf::from(filename);
    if path.is_relative()
        && let Ok(root) = read_config_file().and_then(|conf| conf.get_root_dir())
    {
        path = root.join(path);
    }

    if path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("{} already exists", path.display()),
        ));
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(&path, DEFAULT_CONTRACT)?;
    println!("Created {}", path.display());
    open_in_editor(&path)?;
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
    match cli.command {
        Commands::Config {
            set_dir,
            set_editor,
        } => {
            update_working_dir(set_dir.as_deref());
            update_editor(set_editor.as_deref());
        }
        Commands::Create { filename } => match filename {
            Some(filename) => {
                if let Err(err) = create(&filename) {
                    eprintln!("Error: {}", err);
                }
            }
            None => println!("Error: no filename provided, use 'apic create -f <filename>'"),
        },
        Commands::Init { set_dir } => init(set_dir.as_deref()),
        Commands::List { depth, absolute } => {
            let files = list(depth, absolute);
            if let Some(files) = files {
                for file in files {
                    println!("{}", file.display());
                }
            }
        }
        Commands::Read { filename, status } => match read_filename(&filename) {
            Some(content) => {
                read(content.as_str(), status);
            }
            None => {
                println!("No contract found");
            }
        },
    }
}
