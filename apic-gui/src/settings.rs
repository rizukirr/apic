//! Per-user GUI state, persisted outside any project so the app reopens the last
//! project regardless of where it was launched from.

use std::path::{Path, PathBuf};

/// GUI-global settings stored at the OS config dir
/// (`~/.config/apic-gui/config.toml` on Linux).
#[derive(Default)]
pub struct Settings {
    /// Absolute path of the last opened project root, if any.
    pub last_project: Option<PathBuf>,
}

impl Settings {
    /// Loads settings, returning defaults when the file is missing or unreadable.
    pub fn load() -> Settings {
        let Some(path) = Self::path() else {
            return Settings::default();
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Settings::default();
        };
        Settings {
            last_project: parse_last_project(&text),
        }
    }

    /// Persists settings; best-effort (a write failure is ignored).
    pub fn save(&self) {
        let Some(path) = Self::path() else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let body = match &self.last_project {
            Some(p) => format!("last_project = {:?}\n", p.to_string_lossy()),
            None => String::new(),
        };
        let _ = std::fs::write(path, body);
    }

    fn path() -> Option<PathBuf> {
        Some(dirs::config_dir()?.join("apic-gui").join("config.toml"))
    }
}

/// Extracts `last_project = "..."` from the tiny TOML body. Hand-parsed to avoid
/// a serde dependency for a single key.
fn parse_last_project(text: &str) -> Option<PathBuf> {
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("last_project") {
            let rest = rest.trim_start().strip_prefix('=')?.trim();
            let value = rest.trim_matches('"');
            if !value.is_empty() {
                return Some(PathBuf::from(value));
            }
        }
    }
    None
}

/// Checks that `p` ends with `apic-gui/config.toml`, the expected settings path.
#[allow(dead_code)]
fn _path_is(p: &Path) -> bool {
    p.ends_with("apic-gui/config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quoted_path() {
        assert_eq!(
            parse_last_project("last_project = \"/home/u/proj\"\n"),
            Some(PathBuf::from("/home/u/proj"))
        );
    }

    #[test]
    fn missing_or_empty_yields_none() {
        assert_eq!(parse_last_project(""), None);
        assert_eq!(parse_last_project("last_project = \"\""), None);
    }
}
