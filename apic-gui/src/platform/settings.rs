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

    /// Each project's last opened file, relative to the project root, as
    /// `(project root, relative path)` pairs with the most recent last. The
    /// relative path is untrusted: whoever reads it checks it against the
    /// project before opening anything.
    pub last_file: Vec<(PathBuf, String)>,

    /// Each project's chosen environment, as `(project root, env name)` pairs
    /// with the most recent last. The name is untrusted: whoever reads it
    /// keeps it only when the project still lists that environment.
    pub env: Vec<(PathBuf, String)>,
}

/// How many projects keep a remembered last file, and how many keep a remembered environment. The oldest is dropped past this.
const MAX_LAST_FILES: usize = 20;

impl Settings {
    /// Loads settings, returning defaults when the file is missing or unreadable.
    pub fn load() -> Settings {
        Self::path()
            .map(|path| Self::load_from(&path))
            .unwrap_or_default()
    }

    /// Loads the settings at `path`, applies `change`, and saves the result,
    /// so changing one setting never erases another. Best-effort: a write
    /// failure is ignored.
    ///
    /// # Arguments
    ///
    /// * `path` - the settings file, usually [`Settings::path`]
    /// * `change` - the edit to apply to the loaded settings
    pub fn update_at(path: &Path, change: impl FnOnce(&mut Settings)) {
        let mut settings = Self::load_from(path);
        change(&mut settings);
        settings.save_to(path);
    }

    /// Loads the settings at `path`, returning defaults when the file is
    /// missing or unreadable.
    pub fn load_from(path: &Path) -> Settings {
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

    /// Remembers `rel` as the last opened file of the project at `root`,
    /// making it the most recent entry and dropping the oldest entries past
    /// [`MAX_LAST_FILES`].
    ///
    /// # Arguments
    ///
    /// * `root` - the project root
    /// * `rel` - the file's path relative to `root`, with forward slashes
    pub fn remember_file(&mut self, root: PathBuf, rel: String) {
        self.last_file.retain(|(r, _)| *r != root);
        self.last_file.push((root, rel));
        let excess = self.last_file.len().saturating_sub(MAX_LAST_FILES);
        self.last_file.drain(..excess);
    }

    /// The remembered last file of the project at `root`, relative to it.
    pub fn last_file_of(&self, root: &Path) -> Option<&str> {
        self.last_file
            .iter()
            .find(|(r, _)| r == root)
            .map(|(_, rel)| rel.as_str())
    }

    /// Remembers `name` as the chosen environment of the project at `root`,
    /// or forgets the project's choice for `None`, dropping the oldest
    /// entries past [`MAX_LAST_FILES`].
    ///
    /// # Arguments
    ///
    /// * `root` - the project root
    /// * `name` - the environment name, `None` for no environment
    pub fn remember_env(&mut self, root: PathBuf, name: Option<String>) {
        self.env.retain(|(r, _)| *r != root);
        if let Some(name) = name {
            self.env.push((root, name));
        }
        let excess = self.env.len().saturating_sub(MAX_LAST_FILES);
        self.env.drain(..excess);
    }

    /// The remembered environment of the project at `root`.
    pub fn env_of(&self, root: &Path) -> Option<&str> {
        self.env
            .iter()
            .find(|(r, _)| r == root)
            .map(|(_, name)| name.as_str())
    }

    fn parse(text: &str) -> Settings {
        let mut top = String::new();
        let mut last_file = Vec::new();
        let mut env = Vec::new();
        let mut section: Option<&str> = None;
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') {
                section = Some(trimmed);
                continue;
            }
            match section {
                None => {
                    top.push_str(line);
                    top.push('\n');
                }
                Some("[last_file]") => {
                    if let Some(pair) = parse_last_file_line(trimmed) {
                        last_file.push(pair);
                    }
                }
                Some("[env]") => {
                    if let Some(pair) = parse_last_file_line(trimmed) {
                        env.push(pair);
                    }
                }
                Some(_) => {}
            }
        }
        Settings {
            last_project: parse_last_project(&top),
            theme: parse_theme(&top),
            last_file,
            env,
        }
    }

    fn body(&self) -> String {
        let mut body = String::new();
        if let Some(p) = &self.last_project {
            body.push_str(&format!("last_project = {:?}\n", p.to_string_lossy()));
        }
        body.push_str(&format!("theme = \"{}\"\n", theme_name(self.theme)));
        body.push_str("\n[last_file]\n");
        for (root, rel) in &self.last_file {
            body.push_str(&format!("{:?} = {:?}\n", root.to_string_lossy(), rel));
        }
        body.push_str("\n[env]\n");
        for (root, name) in &self.env {
            body.push_str(&format!("{:?} = {:?}\n", root.to_string_lossy(), name));
        }
        body
    }

    /// The settings file in the OS config dir, or `None` when the OS names
    /// no config dir.
    pub fn path() -> Option<PathBuf> {
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

/// Parses one `"<root>" = "<value>"` line of the `[last_file]` or `[env]` section, or `None` when the line has any other shape.
fn parse_last_file_line(line: &str) -> Option<(PathBuf, String)> {
    let (root, rest) = parse_quoted(line)?;
    let rest = rest.trim_start().strip_prefix('=')?.trim_start();
    let (rel, rest) = parse_quoted(rest)?;
    if root.is_empty() || rel.is_empty() || !rest.trim().is_empty() {
        return None;
    }
    Some((PathBuf::from(root), rel))
}

/// Reads one double-quoted string from the start of `text`, as written by
/// `{:?}`, and returns it with the text after the closing quote. Only the
/// `\\` and `\"` escapes are understood; any other escape is refused.
fn parse_quoted(text: &str) -> Option<(String, &str)> {
    let body = text.strip_prefix('"')?;
    let mut value = String::new();
    let mut chars = body.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Some((value, &body[i + 1..])),
            '\\' => match chars.next()?.1 {
                c @ ('\\' | '"') => value.push(c),
                _ => return None,
            },
            c => value.push(c),
        }
    }
    None
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
            last_file: Vec::new(),
            env: Vec::new(),
        };
        assert_eq!(Settings::parse(&settings.body()), settings);
    }

    #[test]
    fn last_file_round_trips_with_the_other_settings() {
        let path = temp_settings_path("last-file-round-trip");
        Settings::update_at(&path, |s| {
            s.last_project = Some(PathBuf::from("/home/u/proj"));
            s.theme = ThemePreference::Dark;
            s.remember_file(PathBuf::from("/home/u/proj"), "users/get.json".into());
            s.remember_file(PathBuf::from("/home/u/with \"quote\""), "a\\b.json".into());
        });
        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.last_project, Some(PathBuf::from("/home/u/proj")));
        assert_eq!(loaded.theme, ThemePreference::Dark);
        assert_eq!(
            loaded.last_file_of(Path::new("/home/u/proj")),
            Some("users/get.json")
        );
        assert_eq!(
            loaded.last_file_of(Path::new("/home/u/with \"quote\"")),
            Some("a\\b.json")
        );
        let _ = std::fs::remove_dir_all(path.parent().expect("temp path has a parent"));
    }

    #[test]
    fn a_malformed_last_file_line_is_skipped() {
        let text = "theme = \"light\"\n\n[last_file]\n\
            \"/a\" = \"a.json\"\n\
            garbage\n\
            \"/b\" = \n\
            \"/c\" \"c.json\"\n\
            \"/d\" = \"d.json\" trailing\n\
            \"/e\" = \"bad\\x\"\n\
            \"/f\" = \"f.json\"\n\
            [other]\n\
            \"/g\" = \"g.json\"\n\
            theme = \"dark\"\n";
        let settings = Settings::parse(text);
        assert_eq!(
            settings.last_file,
            vec![
                (PathBuf::from("/a"), "a.json".to_string()),
                (PathBuf::from("/f"), "f.json".to_string()),
            ]
        );
        assert_eq!(settings.theme, ThemePreference::Light);
    }

    #[test]
    fn last_file_keeps_at_most_twenty_projects() {
        let mut settings = Settings::default();
        for i in 0..25 {
            settings.remember_file(PathBuf::from(format!("/p{i}")), format!("{i}.json"));
        }
        settings.remember_file(PathBuf::from("/p10"), "again.json".into());
        assert_eq!(settings.last_file.len(), 20);
        assert_eq!(settings.last_file_of(Path::new("/p4")), None);
        assert_eq!(settings.last_file_of(Path::new("/p5")), Some("5.json"));
        assert_eq!(settings.last_file_of(Path::new("/p10")), Some("again.json"));
        assert_eq!(
            settings.last_file.last(),
            Some(&(PathBuf::from("/p10"), "again.json".to_string()))
        );
        settings.remember_file(PathBuf::from("/new"), "n.json".into());
        assert_eq!(settings.last_file.len(), 20);
        assert_eq!(settings.last_file_of(Path::new("/p5")), None);
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

    #[test]
    fn env_round_trips_and_none_forgets_it() {
        let path = temp_settings_path("env-round-trip");
        Settings::update_at(&path, |s| {
            s.remember_file(PathBuf::from("/home/u/proj"), "a.json".into());
            s.remember_env(PathBuf::from("/home/u/proj"), Some("dev".into()));
            s.remember_env(PathBuf::from("/home/u/other"), Some("staging".into()));
        });
        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.env_of(Path::new("/home/u/proj")), Some("dev"));
        assert_eq!(loaded.env_of(Path::new("/home/u/other")), Some("staging"));
        assert_eq!(
            loaded.last_file_of(Path::new("/home/u/proj")),
            Some("a.json")
        );

        Settings::update_at(&path, |s| {
            s.remember_env(PathBuf::from("/home/u/proj"), None)
        });

        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.env_of(Path::new("/home/u/proj")), None);
        assert_eq!(loaded.env_of(Path::new("/home/u/other")), Some("staging"));
        let _ = std::fs::remove_dir_all(path.parent().expect("temp path has a parent"));
    }
}
