//! Environments: named sets of variable values for sending requests.
//!
//! `.apic/env/<name>.toml` is shared and committed. `.apic/env/<name>.local.toml`
//! holds machine-local values such as secrets, is gitignored through
//! `.apic/.gitignore`, and overrides the shared file key by key. Both are flat
//! TOML tables of strings.

use crate::vars::Vars;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The `.apic/.gitignore` line that keeps local env files out of git.
pub const LOCAL_IGNORE_LINE: &str = "env/*.local.toml";

/// One environment found on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvEntry {
    /// The environment name, the file stem.
    pub name: String,
    /// Whether a `<name>.local.toml` exists.
    pub has_local: bool,
}

/// The directory that holds env files.
pub fn env_dir(apic_dir: &Path) -> PathBuf {
    apic_dir.join("env")
}

/// Whether `name` is a usable env name: `[A-Za-z0-9_-]+`. Keeps a name from
/// reaching outside `.apic/env`.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Lists the environments in `apic_dir`, sorted by name. A name exists when
/// its shared file, its local file, or both exist.
pub fn list(apic_dir: &Path) -> Vec<EnvEntry> {
    let mut found: BTreeMap<String, bool> = BTreeMap::new();
    let Ok(entries) = fs::read_dir(env_dir(apic_dir)) else {
        return Vec::new();
    };
    for entry in entries.flatten() {
        let file = entry.file_name();
        let Some(file) = file.to_str() else { continue };
        if let Some(name) = file.strip_suffix(".local.toml") {
            if valid_name(name) {
                found.insert(name.to_string(), true);
            }
        } else if let Some(name) = file.strip_suffix(".toml")
            && valid_name(name)
        {
            found.entry(name.to_string()).or_insert(false);
        }
    }
    found
        .into_iter()
        .map(|(name, has_local)| EnvEntry { name, has_local })
        .collect()
}

/// 1-based line number of byte `offset` in `text`.
fn line_of(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())].matches('\n').count() + 1
}

/// Reads one env file into `vars`. A missing file adds nothing.
fn read_into(path: &Path, vars: &mut Vars) -> Result<(), String> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(format!("Failed to read {}: {}", path.display(), err)),
    };
    // The toml error text quotes the offending line, which may be a secret, so
    // report only the line number.
    let table: toml::Table = toml::from_str(&content).map_err(|err| {
        let at = err
            .span()
            .map(|s| format!(" near line {}", line_of(&content, s.start)))
            .unwrap_or_default();
        format!("Failed to parse {}{}", path.display(), at)
    })?;
    for (key, value) in table {
        match value {
            toml::Value::String(s) => {
                vars.insert(key, s);
            }
            _ => {
                return Err(format!("{}: `{}` must be a string", path.display(), key));
            }
        }
    }
    Ok(())
}

/// Loads environment `name`: the shared file, then the local file over it.
/// An unknown name is an error listing the available ones.
pub fn load(apic_dir: &Path, name: &str) -> Result<Vars, String> {
    let known = list(apic_dir);
    if !valid_name(name) || !known.iter().any(|e| e.name == name) {
        let names: Vec<&str> = known.iter().map(|e| e.name.as_str()).collect();
        return Err(if names.is_empty() {
            format!(
                "No environment '{name}': no env files in {}",
                env_dir(apic_dir).display()
            )
        } else {
            format!("No environment '{name}'. Available: {}", names.join(", "))
        });
    }
    let dir = env_dir(apic_dir);
    let mut vars = Vars::new();
    read_into(&dir.join(format!("{name}.toml")), &mut vars)?;
    read_into(&dir.join(format!("{name}.local.toml")), &mut vars)?;
    Ok(vars)
}

/// Makes sure `.apic/.gitignore` holds [`LOCAL_IGNORE_LINE`], creating the
/// file or appending the line as needed. Leaves a file that has it untouched.
pub fn ensure_gitignore(apic_dir: &Path) -> Result<(), String> {
    let path = apic_dir.join(".gitignore");
    let existing = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(format!("Failed to read {}: {}", path.display(), err)),
    };
    if existing.lines().any(|l| l.trim() == LOCAL_IGNORE_LINE) {
        return Ok(());
    }
    let mut content = existing;
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(LOCAL_IGNORE_LINE);
    content.push('\n');
    fs::write(&path, content).map_err(|err| format!("Failed to write {}: {}", path.display(), err))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_apic(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("apic_env_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("env")).unwrap();
        dir
    }

    #[test]
    fn local_overrides_shared_key_by_key() {
        let apic = temp_apic("overlay");
        fs::write(
            apic.join("env/dev.toml"),
            "host = \"a\"\ntoken = \"shared\"\n",
        )
        .unwrap();
        fs::write(apic.join("env/dev.local.toml"), "token = \"secret\"\n").unwrap();
        let vars = load(&apic, "dev").unwrap();
        assert_eq!(vars.get("host").map(String::as_str), Some("a"));
        assert_eq!(vars.get("token").map(String::as_str), Some("secret"));
    }

    #[test]
    fn local_only_env_exists_and_lists_sorted_with_flag() {
        let apic = temp_apic("list");
        fs::write(apic.join("env/prod.toml"), "").unwrap();
        fs::write(apic.join("env/dev.local.toml"), "a = \"1\"\n").unwrap();
        fs::write(apic.join("env/notes.txt"), "").unwrap();
        assert_eq!(
            list(&apic),
            vec![
                EnvEntry {
                    name: "dev".into(),
                    has_local: true
                },
                EnvEntry {
                    name: "prod".into(),
                    has_local: false
                },
            ]
        );
        assert_eq!(
            load(&apic, "dev").unwrap().get("a").map(String::as_str),
            Some("1")
        );
    }

    #[test]
    fn unknown_env_lists_available_names() {
        let apic = temp_apic("unknown");
        fs::write(apic.join("env/dev.toml"), "").unwrap();
        let err = load(&apic, "stage").unwrap_err();
        assert!(err.contains("dev"), "{err}");
        assert!(load(&apic, "../dev").is_err());
    }

    #[test]
    fn non_string_value_is_refused_naming_key_not_value() {
        let apic = temp_apic("nonstring");
        fs::write(apic.join("env/dev.toml"), "port = 8080\n").unwrap();
        let err = load(&apic, "dev").unwrap_err();
        assert!(err.contains("port"), "{err}");
        assert!(!err.contains("8080"), "{err}");
    }

    #[test]
    fn parse_error_does_not_echo_the_line() {
        let apic = temp_apic("parse");
        fs::write(apic.join("env/dev.toml"), "token = \"s3cr3t\nbroken\n").unwrap();
        let err = load(&apic, "dev").unwrap_err();
        assert!(!err.contains("s3cr3t"), "{err}");
    }

    #[test]
    fn gitignore_is_created_appended_and_left_alone() {
        let apic = temp_apic("ignore");
        let path = apic.join(".gitignore");
        ensure_gitignore(&apic).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "env/*.local.toml\n");

        fs::write(&path, "cache/").unwrap();
        ensure_gitignore(&apic).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "cache/\nenv/*.local.toml\n"
        );

        ensure_gitignore(&apic).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "cache/\nenv/*.local.toml\n"
        );
    }
}
