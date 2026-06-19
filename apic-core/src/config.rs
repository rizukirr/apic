//! Project configuration: the `.apic/config.toml` file and its TOML schema.
//!
//! A project is rooted at an `.apic` directory found by walking up from the
//! current directory. The config records project metadata and the working
//! directory that contract files are scanned from.

use crate::file::{FindFileResult, find_file_upward, to_slash};
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
pub(crate) struct Root {
    working_dir: PathBuf,
}

/// The outcome of a successful [`Config::init`] call.
#[derive(Debug, PartialEq, Eq)]
pub enum InitOutcome {
    /// A new project was created (the `.apic` directory, its `config.toml`,
    /// and the seeded `template/convention.json`). `warning` carries a
    /// best-effort seed-failure note for the caller to print, if any.
    Initialized { warning: Option<String> },
    /// The project already existed and only its missing template was seeded.
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

    /// Resolves the working directory against an explicit `project_root`,
    /// returning a normalized absolute path. `working_dir` stays relative and
    /// portable; joining an absolute legacy value yields that value.
    pub fn root_dir_in(&self, project_root: &Path) -> Result<PathBuf, String> {
        let resolved = project_root.join(&self.root.working_dir);
        if !resolved.exists() {
            return Err(format!(
                "working directory {} does not exist, try to run `apic config --set-dir <dir>`",
                resolved.display()
            ));
        }
        fs::canonicalize(&resolved)
            .map_err(|err| format!("Failed to resolve {}: {}", resolved.display(), err))
    }

    /// Resolves the working directory against the cwd-discovered project root.
    pub fn get_root_dir(&self) -> Result<PathBuf, String> {
        self.root_dir_in(&project_root()?)
    }

    /// Initializes a project in `dir`: creates `dir/.apic`, seeds the template,
    /// and writes `config.toml`. `working_dir` (when given) is stored relative to
    /// `dir`; `None` means the project root itself.
    ///
    /// If `dir/.apic` already exists, a missing template is seeded
    /// ([`InitOutcome::TemplateSeeded`]); a complete project is reported as
    /// already initialized.
    pub fn init_in(dir: &Path, working_dir: Option<&str>) -> Result<InitOutcome, String> {
        let apic_dir = dir.join(".apic");
        if apic_dir.join("config.toml").exists() {
            return if crate::template::seed_if_missing(&apic_dir)? {
                Ok(InitOutcome::TemplateSeeded)
            } else {
                Err("Already initialized!".to_string())
            };
        }

        if !apic_dir.exists() {
            fs::create_dir_all(&apic_dir)
                .map_err(|err| format!("Failed to create {}: {}", apic_dir.display(), err))?;
        }

        let warning = crate::template::seed_if_missing(&apic_dir).err();

        let working_dir = match working_dir {
            Some(w) => {
                let w = PathBuf::from(w);
                let candidate = if w.is_absolute() { w.clone() } else { dir.join(&w) };
                if !candidate.exists() {
                    return Err(format!("Directory {} does not exist", candidate.display()));
                }
                relative_to_root(dir, &w)
            }
            None => PathBuf::from("."),
        };
        write_config_file(apic_dir, &Config::default(&working_dir))?;
        Ok(InitOutcome::Initialized { warning })
    }

    /// Initializes a project in the current directory.
    pub fn init(working_dir: Option<&str>) -> Result<InitOutcome, String> {
        let pwd = std::env::current_dir()
            .map_err(|err| format!("Failed to get current directory: {err}"))?;
        Config::init_in(&pwd, working_dir)
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
        // Store with `/` so the committed working_dir is portable across OSes.
        PathBuf::from(to_slash(rel))
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

/// Searches upward from `start` for the project's `.apic` directory.
pub fn find_apic_dir_in(start: &Path) -> Option<PathBuf> {
    match find_file_upward(start.to_path_buf(), &[PathBuf::from(".apic")]) {
        FindFileResult::Found(dirs) => dirs.first().cloned(),
        FindFileResult::NotFound => None,
    }
}

/// Returns the project's `.apic` directory by walking up from the current
/// directory; `None` when not inside a project.
pub fn find_apic_dir() -> Option<PathBuf> {
    find_apic_dir_in(&std::env::current_dir().ok()?)
}

/// Reads and deserializes the `config.toml` of the project rooted at
/// `project_root` (the directory that directly contains `.apic/`).
pub fn read_config_in(project_root: &Path) -> Result<Config, String> {
    let config_file = project_root.join(".apic").join("config.toml");
    let content = fs::read_to_string(&config_file)
        .map_err(|err| format!("Failed to read {}: {}", config_file.display(), err))?;
    toml::from_str(&content)
        .map_err(|err| format!("Failed to parse {}: {}", config_file.display(), err))
}

/// Reads the project config discovered by walking up from the current directory.
pub fn read_config_file() -> Result<Config, String> {
    let apic_dir = find_apic_dir().ok_or("Not initialized yet, run `apic init` first")?;
    let root = apic_dir.parent().unwrap_or(&apic_dir);
    read_config_in(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a platform-absolute path from components. A leading `/` is not
    /// absolute on Windows (it needs a drive prefix like `C:\`), so tests must
    /// construct roots this way to exercise the `is_absolute()` branch on every
    /// OS.
    fn abs(parts: &[&str]) -> PathBuf {
        let mut p = PathBuf::from(if cfg!(windows) { "C:\\" } else { "/" });
        for part in parts {
            p.push(part);
        }
        p
    }

    #[test]
    fn relative_input_is_kept_as_is() {
        let root = abs(&["home", "u", "project"]);
        assert_eq!(
            relative_to_root(&root, Path::new("api-contract")),
            PathBuf::from("api-contract")
        );
    }

    #[test]
    fn absolute_input_under_root_is_made_relative() {
        let root = abs(&["home", "u", "project"]);
        assert_eq!(
            relative_to_root(&root, &abs(&["home", "u", "project", "api-contract"])),
            PathBuf::from("api-contract")
        );
    }

    #[test]
    fn absolute_input_equal_to_root_collapses_to_dot() {
        let root = abs(&["home", "u", "project"]);
        assert_eq!(
            relative_to_root(&root, &abs(&["home", "u", "project"])),
            PathBuf::from(".")
        );
    }

    #[test]
    fn absolute_input_outside_root_is_kept_absolute() {
        let root = abs(&["home", "u", "project"]);
        let outside = abs(&["etc", "contracts"]);
        assert_eq!(relative_to_root(&root, &outside), outside);
    }

    fn tempdir() -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let id = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("apic-cfg-{}-{}", std::process::id(), id));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn init_in_creates_project_and_config_reads_back() {
        let dir = tempdir();
        let outcome = Config::init_in(&dir, None).unwrap();
        assert!(matches!(outcome, InitOutcome::Initialized { .. }));
        assert!(dir.join(".apic/config.toml").exists());

        let cfg = read_config_in(&dir).unwrap();
        // working_dir is "." so it resolves back to the project root.
        let resolved = cfg.root_dir_in(&dir).unwrap();
        assert_eq!(resolved, fs::canonicalize(&dir).unwrap());
    }

    #[test]
    fn init_in_twice_reports_already_initialized() {
        let dir = tempdir();
        Config::init_in(&dir, None).unwrap();
        let err = Config::init_in(&dir, None).unwrap_err();
        assert_eq!(err, "Already initialized!");
    }
}
