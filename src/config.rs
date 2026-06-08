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
    root: Root,
}

/// The `[root]` section of the config holding the working directory.
#[derive(Debug, Deserialize, Serialize)]
pub struct Root {
    working_dir: PathBuf,
}

/// The outcome of a successful [`Config::init`] call.
#[derive(Debug, PartialEq, Eq)]
pub enum InitOutcome {
    /// A new project was created (the `.apic` directory, its `config.toml`,
    /// and the seeded `template.json`).
    Initialized,
    /// The project already existed and only its missing `template.json` was
    /// seeded.
    TemplateSeeded,
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
            root: root_dir,
        }
    }

    /// Returns the configured working directory as a normalized absolute path.
    ///
    /// The stored `working_dir` is relative to the project root (the parent of
    /// the `.apic` directory), which keeps the config portable across machines
    /// and clones. A legacy absolute `working_dir` is still honored, since
    /// joining an absolute path simply yields that path.
    pub fn get_root_dir(&self) -> Result<PathBuf, String> {
        let project_root = project_root()?;
        let resolved = project_root.join(&self.root.working_dir);
        if !resolved.exists() {
            let err = format!(
                "working directory {} does not exist, try to run `apic config --set-dir <dir>`",
                resolved.display()
            );
            return Err(err);
        }
        // Normalize away `.`/`..` components (and symlinks) so downstream path
        // stripping in scans operates on a clean absolute root.
        fs::canonicalize(&resolved)
            .map_err(|err| format!("Failed to resolve {}: {}", resolved.display(), err))
    }

    /// Initializes a new project: creates the `.apic` directory and writes a
    /// default `config.toml` and `template.json`.
    ///
    /// If `working_dir` is given it becomes the root (resolved relative to the
    /// current directory when not absolute); otherwise the current directory is
    /// used.
    ///
    /// When the project already exists, this does not error outright: a missing
    /// `template.json` is seeded (returning [`InitOutcome::TemplateSeeded`]) so
    /// a project whose template was deleted or whose seed predates template
    /// support can be repaired by re-running `init`. Only when the template is
    /// also already present is it reported as already initialized.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the project is already fully initialized, the given
    /// working directory does not exist, or the `.apic` directory cannot be
    /// created.
    pub fn init(working_dir: Option<&str>) -> Result<InitOutcome, String> {
        match find_file_apic_config_file() {
            Ok(FindFileResult::Found(_)) => {
                // Already initialized: recover a missing template instead of
                // failing, but still report a fully-initialized project as an
                // error so re-running `init` over a complete project is a no-op.
                let apic_dir = find_apic_dir().ok_or("Already initialized!")?;
                return if crate::template::seed_if_missing(&apic_dir)? {
                    Ok(InitOutcome::TemplateSeeded)
                } else {
                    Err("Already initialized!".to_string())
                };
            }
            Ok(FindFileResult::NotFound) => true,
            Err(err) => {
                return Err(err);
            }
        };

        let dir = PathBuf::from(".apic");
        let pwd = std::env::current_dir()
            .map_err(|err| format!("Failed to get current directory: {err}"))?;
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

        // Surface the contract template so the user can customize it. An
        // existing template (e.g. on a re-created project) is left untouched.
        // Best-effort: a seed failure must not abort an otherwise-successful
        // init — `apic create` re-seeds and falls back to the built-in default.
        if let Err(err) = crate::template::seed_if_missing(&makedir) {
            eprintln!("Warning: {err}");
        }

        // `working_dir` is stored relative to the project root (= `pwd` here,
        // where `.apic` is created) so the config stays portable. A `None`
        // working dir means the project root itself.
        let working_dir = match working_dir {
            Some(dir) => {
                let dir = PathBuf::from(dir);
                if !dir.exists() {
                    let err = format!("Directory {} does not exist", dir.display());
                    return Err(err);
                }
                relative_to_root(&pwd, &dir)
            }
            None => PathBuf::from("."),
        };
        write_config_file(makedir.clone(), &Config::default(&working_dir))?;
        Ok(InitOutcome::Initialized)
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

        // No-op if the new target resolves to the current working directory.
        let current = root.join(&self.root.working_dir);
        if let (Ok(a), Ok(b)) = (fs::canonicalize(&dir), fs::canonicalize(&current))
            && a == b
        {
            let err = format!("Already in {}", dir.display());
            return Err(err);
        }

        // Persist relative to the project root so the config stays portable.
        self.root.working_dir = relative_to_root(root, Path::new(new_dir));
        write_config_file(apic_dir, self)
    }
}

/// Serializes `config` to TOML and writes it to `apic_dir/config.toml`.
///
/// # Errors
///
/// Returns `Err` if the config cannot be serialized or the file cannot be
/// written (e.g. a read-only directory or a full disk).
fn write_config_file(apic_dir: PathBuf, config: &Config) -> Result<(), String> {
    let config_to_str = toml::to_string_pretty(config)
        .map_err(|err| format!("Failed to serialize config: {err}"))?;
    let path = apic_dir.join("config.toml");
    fs::write(&path, config_to_str)
        .map_err(|err| format!("Failed to write {}: {}", path.display(), err))?;
    Ok(())
}

/// Returns the project root: the parent of the discovered `.apic` directory.
///
/// # Errors
///
/// Returns `Err` if the project is not initialized.
fn project_root() -> Result<PathBuf, String> {
    match find_file_apic_dir()? {
        FindFileResult::Found(dir) => {
            let apic_dir = dir.first().unwrap();
            Ok(apic_dir.parent().unwrap_or(apic_dir).to_path_buf())
        }
        FindFileResult::NotFound => Err("Not initialized yet, run `apic init` first".to_string()),
    }
}

/// Expresses `dir` relative to `root` so it can be stored portably.
///
/// A relative `dir` is returned unchanged (it is already relative to `root`).
/// An absolute `dir` under `root` is stripped to the relative remainder;
/// an absolute `dir` outside the project cannot be made portable and is kept
/// as-is. A result equal to `root` itself collapses to `.`.
fn relative_to_root(root: &Path, dir: &Path) -> PathBuf {
    let rel = if dir.is_absolute() {
        dir.strip_prefix(root).unwrap_or(dir)
    } else {
        dir
    };
    if rel.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        rel.to_path_buf()
    }
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

/// Returns the project's `.apic` directory if one is found by walking upward
/// from the current directory; `None` when not inside a project.
///
/// Unlike [`project_root`] this never errors — callers that also work outside
/// a project (e.g. `apic create`) treat `None` as "no project".
pub fn find_apic_dir() -> Option<PathBuf> {
    match find_file_apic_dir().ok()? {
        FindFileResult::Found(dirs) => dirs.first().cloned(),
        FindFileResult::NotFound => None,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_input_is_kept_as_is() {
        let root = Path::new("/home/u/project");
        assert_eq!(
            relative_to_root(root, Path::new("api-contract")),
            PathBuf::from("api-contract")
        );
    }

    #[test]
    fn absolute_input_under_root_is_made_relative() {
        let root = Path::new("/home/u/project");
        assert_eq!(
            relative_to_root(root, Path::new("/home/u/project/api-contract")),
            PathBuf::from("api-contract")
        );
    }

    #[test]
    fn absolute_input_equal_to_root_collapses_to_dot() {
        let root = Path::new("/home/u/project");
        assert_eq!(
            relative_to_root(root, Path::new("/home/u/project")),
            PathBuf::from(".")
        );
    }

    #[test]
    fn absolute_input_outside_root_is_kept_absolute() {
        let root = Path::new("/home/u/project");
        assert_eq!(
            relative_to_root(root, Path::new("/etc/contracts")),
            PathBuf::from("/etc/contracts")
        );
    }
}
