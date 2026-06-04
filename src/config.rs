//! Project configuration: the `.apic/config.toml` file and its TOML schema.
//!
//! A project is rooted at an `.apic` directory found by walking up from the
//! current directory. The config records project metadata and the working
//! directory that contract files are scanned from.

use crate::file::{FindFileResult, find_file_downward, find_file_upward};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// The persisted `apic` project configuration (serialized to `config.toml`).
#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    name: String,
    version: String,
    author: String,
    /// Preferred editor command; overrides `$VISUAL`/`$EDITOR` when set.
    editor: Option<String>,
    root: Root,
}

/// The `[root]` section of the config holding the working directory.
#[derive(Debug, Deserialize, Serialize)]
pub struct Root {
    working_dir: PathBuf,
}

impl Config {
    /// Builds a default config with `dir` as the working directory.
    fn default(dir: &Path) -> Config {
        let root_dir = Root {
            working_dir: dir.to_path_buf(),
        };

        Config {
            name: "apic".to_string(),
            version: "0.1.0".to_string(),
            author: "rizukirr".to_string(),
            editor: None,
            root: root_dir,
        }
    }

    /// Returns the configured root working directory.
    pub fn get_root_dir(&self) -> Result<PathBuf, String> {
        let root = self.root.working_dir.to_path_buf();
        if !root.exists() {
            let err = format!(
                "working directory {} does not exist, try to run `apic config --set-dir <dir>`",
                root.display()
            );
            return Err(err);
        }
        Ok(root)
    }

    /// Initializes a new project: creates the `.apic` directory and writes a
    /// default `config.toml`.
    ///
    /// If `working_dir` is given it becomes the root (resolved relative to the
    /// current directory when not absolute); otherwise the current directory is
    /// used.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the project is already initialized, the given working
    /// directory does not exist, or the `.apic` directory cannot be created.
    pub fn init(working_dir: Option<&str>) -> Result<(), String> {
        match find_file_apic_config_file() {
            Ok(FindFileResult::Found(_)) => {
                return Err("Already initialized!".to_string());
            }
            Ok(FindFileResult::NotFound) => true,
            Err(err) => {
                return Err(err);
            }
        };

        let dir = PathBuf::from(".apic");
        let pwd = std::env::current_dir().unwrap();
        let makedir = pwd.join(&dir);

        if !makedir.exists() {
            match fs::create_dir(&makedir) {
                Ok(_) => true,
                Err(err) => {
                    let err = format!("Failed to create {}: {}", &dir.display(), err);
                    return Err(err);
                }
            };
        }

        let working_dir = match working_dir {
            Some(dir) => {
                let dir = PathBuf::from(dir);
                if !dir.exists() {
                    let err = format!("Directory {} does not exist", dir.display());
                    return Err(err);
                }

                if dir.is_absolute() {
                    dir
                } else {
                    pwd.join(dir)
                }
            }
            None => pwd,
        };
        write_config_file(makedir.clone(), &Config::default(&working_dir))
    }

    /// Changes the root working directory to `new_dir` and persists the config.
    ///
    /// `new_dir` is resolved relative to the project root (the parent of the
    /// `.apic` directory).
    ///
    /// # Errors
    ///
    /// Returns `Err` if the project is not initialized or `new_dir` already
    /// equals the current working directory.
    pub fn update_root_dir(&mut self, new_dir: &str) -> Result<(), String> {
        let apic_dir = match find_file_apic_dir() {
            Ok(FindFileResult::Found(dir)) => dir.first().unwrap().clone(),
            Ok(FindFileResult::NotFound) => {
                return Err("Not initialized yet".to_string());
            }
            Err(err) => {
                return Err(err);
            }
        };

        let root = apic_dir.parent().unwrap();
        let dir = root.join(new_dir);
        if !dir.exists() {
            let err = format!("Directory {} does not exist", dir.display());
            return Err(err);
        }
        if dir == self.root.working_dir {
            let err = format!("Already in {}", dir.display());
            return Err(err);
        }

        self.root.working_dir = dir;
        write_config_file(apic_dir, self)
    }

    /// Changes the preferred editor to `editor` and persists the config.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the project is not initialized or `editor` already
    /// equals the configured editor.
    pub fn update_editor(&mut self, editor: &str) -> Result<(), String> {
        let apic_dir = match find_file_apic_dir() {
            Ok(FindFileResult::Found(dir)) => dir.first().unwrap().clone(),
            Ok(FindFileResult::NotFound) => {
                return Err("Not initialized yet".to_string());
            }
            Err(err) => {
                return Err(err);
            }
        };

        if self.editor.as_deref() == Some(editor) {
            let err = format!("Editor is already {}", editor);
            return Err(err);
        }

        self.editor = Some(editor.to_string());
        write_config_file(apic_dir, self)
    }
}

/// Returns the editor configured in `config.toml`, if the project is
/// initialized and one is set.
///
/// Unlike [`read_config_file`] this never panics, so callers that work
/// outside an initialized project (e.g. `apic create`) can fall back to
/// environment variables.
pub fn configured_editor() -> Option<String> {
    let config_file = match find_file_apic_config_file().ok()? {
        FindFileResult::Found(path) => path.first()?.clone(),
        FindFileResult::NotFound => return None,
    };

    let content = fs::read_to_string(config_file).ok()?;
    let config: Config = toml::from_str(&content).ok()?;
    config.editor
}

/// Serializes `config` to TOML and writes it to `apic_dir/config.toml`.
///
/// # Panics
///
/// Panics if serialization or the file write fails.
fn write_config_file(apic_dir: PathBuf, config: &Config) -> Result<(), String> {
    let config_to_str = toml::to_string_pretty(config).unwrap();
    fs::write(apic_dir.join("config.toml"), config_to_str).unwrap();
    Ok(())
}

/// Searches upward from the current directory for the `.apic` directory.
fn find_file_apic_dir() -> Result<FindFileResult, String> {
    let pwd = match std::env::current_dir() {
        Ok(pwd) => pwd,
        Err(err) => {
            let err = format!("Failed to get current directory: {}", err);
            return Err(err);
        }
    };
    let name = vec![PathBuf::from(".apic")];

    Ok(find_file_upward(pwd, &name))
}

/// Locates `config.toml` inside the discovered `.apic` directory.
///
/// A missing `.apic` directory is reported as [`FindFileResult::NotFound`]
/// (the project simply is not initialized), not as an error.
fn find_file_apic_config_file() -> Result<FindFileResult, String> {
    let pwd = match find_file_apic_dir()? {
        FindFileResult::Found(pwd) => pwd.first().unwrap().clone(),
        FindFileResult::NotFound => return Ok(FindFileResult::NotFound),
    };

    let name = vec![PathBuf::from("config.toml")];

    Ok(find_file_downward(pwd, &name))
}

/// Reads and deserializes the project's `config.toml`.
///
/// # Errors
///
/// Returns `Err` if the project is not initialized or the config file cannot
/// be read or parsed.
pub fn read_config_file() -> Result<Config, String> {
    let config_file = match find_file_apic_config_file()? {
        FindFileResult::Found(path) => path.first().unwrap().clone(),
        FindFileResult::NotFound => {
            return Err("Not initialized yet, run `apic init` first".to_string());
        }
    };

    let content = fs::read_to_string(&config_file)
        .map_err(|err| format!("Failed to read {}: {}", config_file.display(), err))?;
    let config: Config = toml::from_str(&content)
        .map_err(|err| format!("Failed to parse {}: {}", config_file.display(), err))?;
    Ok(config)
}
