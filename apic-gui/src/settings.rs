//! Per-user GUI state, persisted outside any project so the app reopens the last
//! project, in the last chosen theme, regardless of where it was launched from.

use std::path::{Path, PathBuf};

use eframe::egui::ThemePreference;

/// GUI-global settings stored at the OS config dir
/// (`~/.config/apic-gui/config.toml` on Linux).
#[derive(Debug, Default, PartialEq)]
pub struct Settings {
    /// Absolute path of the last opened project root, if any.
    pub last_project: Option<PathBuf>,

    /// The chosen theme. Missing or unknown in the file means `System`.
    pub theme: ThemePreference,
}

impl Settings {
    /// Loads settings, returning defaults when the file is missing or unreadable.
    pub fn load() -> Settings {
        Self::path()
            .map(|path| Self::load_from(&path))
            .unwrap_or_default()
    }

    /// Loads the saved settings, applies `change`, and saves the result, so
    /// changing one setting never erases another. Best-effort: a write
    /// failure is ignored.
    pub fn update(change: impl FnOnce(&mut Settings)) {
        if let Some(path) = Self::path() {
            Self::update_at(&path, change);
        }
    }

    fn update_at(path: &Path, change: impl FnOnce(&mut Settings)) {
        let mut settings = Self::load_from(path);
        change(&mut settings);
        settings.save_to(path);
    }

    fn load_from(path: &Path) -> Settings {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text),
            Err(_) => Settings::default(),
        }
    }

    fn save_to(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, self.body());
    }

    fn parse(text: &str) -> Settings {
        Settings {
            last_project: parse_last_project(text),
            theme: parse_theme(text),
        }
    }

    fn body(&self) -> String {
        let mut body = String::new();
        if let Some(p) = &self.last_project {
            body.push_str(&format!("last_project = {:?}\n", p.to_string_lossy()));
        }
        body.push_str(&format!("theme = \"{}\"\n", theme_name(self.theme)));
        body
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

/// Extracts `theme = "..."`, falling back to `System` when it is missing or
/// names no known theme.
fn parse_theme(text: &str) -> ThemePreference {
    text.lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("theme")?
                .trim_start()
                .strip_prefix('=')
        })
        .find_map(|value| match value.trim().trim_matches('"') {
            "dark" => Some(ThemePreference::Dark),
            "light" => Some(ThemePreference::Light),
            "system" => Some(ThemePreference::System),
            _ => None,
        })
        .unwrap_or_default()
}

/// The settings-file spelling of `theme`.
fn theme_name(theme: ThemePreference) -> &'static str {
    match theme {
        ThemePreference::Dark => "dark",
        ThemePreference::Light => "light",
        ThemePreference::System => "system",
    }
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

    fn temp_settings_path(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("apic-gui-settings-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("config.toml")
    }

    #[test]
    fn theme_parses_each_known_value() {
        assert_eq!(parse_theme("theme = \"dark\"\n"), ThemePreference::Dark);
        assert_eq!(parse_theme("theme = \"light\"\n"), ThemePreference::Light);
        assert_eq!(parse_theme("theme = \"system\"\n"), ThemePreference::System);
    }

    #[test]
    fn missing_or_unknown_theme_is_system() {
        assert_eq!(parse_theme(""), ThemePreference::System);
        assert_eq!(parse_theme("theme = \"neon\"\n"), ThemePreference::System);
    }

    #[test]
    fn body_round_trips_both_settings() {
        let settings = Settings {
            last_project: Some(PathBuf::from("/home/u/proj")),
            theme: ThemePreference::Light,
        };
        assert_eq!(Settings::parse(&settings.body()), settings);
    }

    #[test]
    fn updating_last_project_keeps_the_saved_theme() {
        let path = temp_settings_path("keeps-theme");
        Settings::update_at(&path, |s| s.theme = ThemePreference::Light);
        Settings::update_at(&path, |s| {
            s.last_project = Some(PathBuf::from("/home/u/proj"))
        });
        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.theme, ThemePreference::Light);
        assert_eq!(loaded.last_project, Some(PathBuf::from("/home/u/proj")));
        let _ = std::fs::remove_dir_all(path.parent().expect("temp path has a parent"));
    }
}
