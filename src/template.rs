//! The contract template used by `apic create`.
//!
//! The default template is embedded at compile time. A project can override it
//! by editing `.apic/template.json`; `apic init` seeds that file from the
//! default, and it is never overwritten once it exists.

use std::fs;
use std::path::Path;

/// The built-in contract template: the seed for `.apic/template.json` and the
/// fallback used when no usable project template is present.
pub const DEFAULT: &str = include_str!("templates/contract.json");

/// Name of the per-project template file inside the `.apic` directory.
const TEMPLATE_FILE: &str = "template.json";

/// Writes the default template to `<apic_dir>/template.json` when that file
/// does not already exist. An existing template is left untouched.
pub fn seed_if_missing(apic_dir: &Path) -> Result<(), String> {
    let path = apic_dir.join(TEMPLATE_FILE);
    if path.exists() {
        return Ok(());
    }
    fs::write(&path, DEFAULT).map_err(|err| format!("Failed to write {}: {}", path.display(), err))
}

/// Returns the contract body that `apic create` should write.
///
/// Inside a project the per-project `.apic/template.json` is seeded when
/// missing, then used when it parses as a valid contract. A malformed template
/// is reported and the embedded default is used instead, leaving the user's
/// file untouched. Outside a project the embedded default is returned.
pub fn resolve_for_create() -> String {
    let Some(apic_dir) = crate::config::find_apic_dir() else {
        return DEFAULT.to_string();
    };

    // Any problem reaching a usable project template falls back to the embedded
    // default without failing `create`.
    let fallback = |reason: String| {
        eprintln!("Warning: {reason}; using the built-in template");
        DEFAULT.to_string()
    };

    if let Err(err) = seed_if_missing(&apic_dir) {
        return fallback(err);
    }

    let path = apic_dir.join(TEMPLATE_FILE);
    match fs::read_to_string(&path) {
        Ok(content) if crate::json::validate(&content).is_ok() => content,
        Ok(_) => fallback(format!("{} is not a valid contract", path.display())),
        Err(err) => fallback(format!("failed to read {}: {}", path.display(), err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique, empty temp directory standing in for an `.apic` dir.
    fn temp_apic(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("apic_tmpl_{tag}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn seed_if_missing_writes_default_when_absent() {
        let dir = temp_apic("seed_absent");
        seed_if_missing(&dir).unwrap();
        let written = fs::read_to_string(dir.join("template.json")).unwrap();
        assert_eq!(written, DEFAULT);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn seed_if_missing_does_not_overwrite_existing() {
        let dir = temp_apic("seed_existing");
        let custom = r#"{ "marker": "mine" }"#;
        fs::write(dir.join("template.json"), custom).unwrap();
        seed_if_missing(&dir).unwrap();
        let after = fs::read_to_string(dir.join("template.json")).unwrap();
        assert_eq!(after, custom);
        fs::remove_dir_all(&dir).unwrap();
    }
}
