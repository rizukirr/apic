//! The contract template used by `apic create`.
//!
//! The default template is embedded at compile time and supplies a complete
//! contract. A project can customize it by editing `.apic/template.json`, which
//! is overlaid onto the default — so the project template only needs to carry
//! the fields it wants to change and may be a small partial contract. `apic
//! init` seeds that file from the default, and it is never overwritten once it
//! exists.

use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// The built-in contract template: the seed for `.apic/template.json` and the
/// fallback used when no usable project template is present.
pub const DEFAULT: &str = include_str!("templates/contract.json");

/// Name of the per-project template file inside the `.apic` directory.
const TEMPLATE_FILE: &str = "template.json";

/// Returns the path to the per-project template inside `apic_dir`.
pub fn path(apic_dir: &Path) -> PathBuf {
    apic_dir.join(TEMPLATE_FILE)
}

/// Writes the default template to `<apic_dir>/template.json` when that file
/// does not already exist. An existing template is left untouched.
///
/// Returns `true` when the file was written and `false` when it was already
/// present, so callers can report whether they actually seeded it.
pub fn seed_if_missing(apic_dir: &Path) -> Result<bool, String> {
    let path = path(apic_dir);
    if path.exists() {
        return Ok(false);
    }
    fs::write(&path, DEFAULT)
        .map(|()| true)
        .map_err(|err| format!("Failed to write {}: {}", path.display(), err))
}

/// Returns the contract body that `apic create` should write.
///
/// The embedded default is always the base. Inside a project the per-project
/// `.apic/template.json` is seeded when missing, then overlaid onto the default
/// (see [`merge_onto_default`]) so the user only has to specify the fields they
/// want to change. A missing or unreadable file falls back to the plain default;
/// a template that exists but does not merge into a valid contract is returned as
/// an `Err` so `create` can abort. Outside a project the default is returned.
pub fn resolve_for_create() -> Result<String, String> {
    match crate::config::find_apic_dir() {
        Some(apic_dir) => resolve_at(&apic_dir),
        None => Ok(DEFAULT.to_string()),
    }
}

/// Resolves the contract body for `create` against a known `apic_dir`.
///
/// Missing/unreadable template files and seed failures fall back to the
/// built-in default with a warning (returned as `Ok`); only a template file
/// that exists but does not merge into a valid contract is a hard error.
fn resolve_at(apic_dir: &Path) -> Result<String, String> {
    let fallback = |reason: String| {
        eprintln!("Warning: {reason}; using the built-in template");
        DEFAULT.to_string()
    };

    if let Err(err) = seed_if_missing(apic_dir) {
        return Ok(fallback(err));
    }

    let path = path(apic_dir);
    let overlay = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) => {
            return Ok(fallback(format!(
                "failed to read {}: {err}",
                path.display()
            )));
        }
    };

    match merge_onto_default(&overlay) {
        Ok(contract) => Ok(contract),
        Err(reason) => Err(format!("{} {reason}", path.display())),
    }
}

/// Validation outcome for `apic validate --template`.
pub enum TemplateCheck {
    /// No project, or no readable template file present — nothing to validate.
    Absent,
    /// The template merges onto the default and yields a valid contract.
    Valid,
    /// The template exists but is invalid; the string explains why.
    Invalid(String),
}

/// Checks the project template without writing anything.
///
/// Returns [`TemplateCheck::Absent`] outside a project; otherwise delegates to
/// [`check_at`].
pub fn check_template() -> TemplateCheck {
    match crate::config::find_apic_dir() {
        Some(apic_dir) => check_at(&apic_dir),
        None => TemplateCheck::Absent,
    }
}

/// Read-only validity check of `<apic_dir>/template.json`.
///
/// A missing or unreadable file is [`TemplateCheck::Absent`] (consistent with
/// `create` treating those as non-fatal); a present file is `Valid`/`Invalid`
/// based on the same [`merge_onto_default`] used to build the contract.
fn check_at(apic_dir: &Path) -> TemplateCheck {
    let path = path(apic_dir);
    let overlay = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(_) => return TemplateCheck::Absent,
    };
    match merge_onto_default(&overlay) {
        Ok(_) => TemplateCheck::Valid,
        Err(reason) => TemplateCheck::Invalid(reason),
    }
}

/// Overlays the project template `overlay` onto the built-in default and
/// returns the rendered, validated contract.
///
/// The default provides every field; `overlay` only needs the values it wants
/// to change. The result is validated so a partial template that merges into an
/// invalid contract is rejected (the caller then falls back to the default).
pub(crate) fn merge_onto_default(overlay: &str) -> Result<String, String> {
    let mut base: Value = serde_json::from_str(DEFAULT)
        .map_err(|err| format!("built-in template is not valid JSON: {err}"))?;
    let overlay: Value =
        serde_json::from_str(overlay).map_err(|err| format!("is not valid JSON: {err}"))?;

    merge(&mut base, overlay);

    let contract = render_pretty(&base)?;
    crate::json::validate(&contract)
        .map_err(|err| format!("merged with the default is not a valid contract: {err}"))?;
    Ok(contract)
}

/// Recursively overlays `overlay` onto `base`: when both sides are objects the
/// keys are merged (overlay wins on conflicts, base keeps keys `overlay` omits);
/// otherwise — arrays and scalars — `overlay` replaces `base` wholesale.
fn merge(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(base_map), Value::Object(overlay_map)) => {
            for (key, value) in overlay_map {
                merge(base_map.entry(key).or_insert(Value::Null), value);
            }
        }
        (base_slot, overlay_value) => *base_slot = overlay_value,
    }
}

/// Serializes `value` as pretty JSON with four-space indentation, matching the
/// style of the embedded template.
pub(crate) fn render_pretty(value: &Value) -> Result<String, String> {
    let mut buf = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut serializer = serde_json::Serializer::with_formatter(&mut buf, formatter);
    value
        .serialize(&mut serializer)
        .map_err(|err| format!("failed to render merged template: {err}"))?;
    String::from_utf8(buf).map_err(|err| format!("merged template is not valid UTF-8: {err}"))
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
    fn path_is_template_json_in_apic_dir() {
        let dir = std::path::Path::new("/tmp/.apic");
        assert_eq!(path(dir), dir.join("template.json"));
    }

    #[test]
    fn seed_if_missing_writes_default_when_absent() {
        let dir = temp_apic("seed_absent");
        assert!(seed_if_missing(&dir).unwrap());
        let written = fs::read_to_string(dir.join("template.json")).unwrap();
        assert_eq!(written, DEFAULT);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn seed_if_missing_does_not_overwrite_existing() {
        let dir = temp_apic("seed_existing");
        let custom = r#"{ "marker": "mine" }"#;
        fs::write(dir.join("template.json"), custom).unwrap();
        assert!(!seed_if_missing(&dir).unwrap());
        let after = fs::read_to_string(dir.join("template.json")).unwrap();
        assert_eq!(after, custom);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn merge_keeps_base_keys_omitted_by_overlay() {
        let mut base = serde_json::json!({ "name": "default", "method": "POST" });
        merge(&mut base, serde_json::json!({ "name": "custom" }));
        assert_eq!(
            base,
            serde_json::json!({ "name": "custom", "method": "POST" })
        );
    }

    #[test]
    fn merge_deep_merges_nested_objects() {
        let mut base = serde_json::json!({
            "url": { "protocol": "https", "host": "old", "path": ["a"] }
        });
        merge(&mut base, serde_json::json!({ "url": { "host": "new" } }));
        assert_eq!(
            base,
            serde_json::json!({
                "url": { "protocol": "https", "host": "new", "path": ["a"] }
            })
        );
    }

    #[test]
    fn merge_replaces_arrays_wholesale() {
        let mut base = serde_json::json!({ "headers": [{ "name": "A" }, { "name": "B" }] });
        merge(
            &mut base,
            serde_json::json!({ "headers": [{ "name": "C" }] }),
        );
        assert_eq!(base, serde_json::json!({ "headers": [{ "name": "C" }] }));
    }

    #[test]
    fn merge_onto_default_fills_partial_overlay_and_validates() {
        // A template that overrides only the headers still produces a complete,
        // valid contract: name/method/etc. come from the embedded default.
        let overlay = r#"{ "headers": [ { "name": "X-Custom", "value": "1" } ] }"#;
        let contract = merge_onto_default(overlay).unwrap();
        assert!(crate::json::validate(&contract).is_ok());

        let value: Value = serde_json::from_str(&contract).unwrap();
        assert_eq!(value["name"], serde_json::json!("endpoint-name"));
        assert_eq!(
            value["headers"],
            serde_json::json!([{ "name": "X-Custom", "value": "1" }])
        );
    }

    #[test]
    fn merge_onto_default_rejects_invalid_json() {
        assert!(merge_onto_default("{ not json").is_err());
    }

    #[test]
    fn resolve_at_returns_ok_for_valid_partial_overlay() {
        let dir = temp_apic("resolve_valid");
        fs::write(
            dir.join("template.json"),
            r#"{ "headers": [ { "name": "X-Custom", "value": "1" } ] }"#,
        )
        .unwrap();
        let contract = resolve_at(&dir).unwrap();
        assert!(crate::json::validate(&contract).is_ok());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolve_at_errors_for_malformed_json() {
        let dir = temp_apic("resolve_malformed");
        fs::write(dir.join("template.json"), "{ not json").unwrap();
        assert!(resolve_at(&dir).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolve_at_errors_when_overlay_merges_to_invalid_contract() {
        let dir = temp_apic("resolve_invalid_merge");
        // method must be a string; a number makes the merged contract invalid.
        fs::write(dir.join("template.json"), r#"{ "method": 123 }"#).unwrap();
        assert!(resolve_at(&dir).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolve_at_seeds_and_returns_ok_when_template_missing() {
        let dir = temp_apic("resolve_seed");
        let contract = resolve_at(&dir).unwrap();
        assert!(crate::json::validate(&contract).is_ok());
        assert!(dir.join("template.json").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn check_at_reports_valid_for_good_overlay() {
        let dir = temp_apic("check_valid");
        fs::write(dir.join("template.json"), r#"{ "method": "GET" }"#).unwrap();
        assert!(matches!(check_at(&dir), TemplateCheck::Valid));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn check_at_reports_invalid_for_malformed_json() {
        let dir = temp_apic("check_malformed");
        fs::write(dir.join("template.json"), "{ not json").unwrap();
        assert!(matches!(check_at(&dir), TemplateCheck::Invalid(_)));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn check_at_reports_invalid_when_merge_yields_invalid_contract() {
        let dir = temp_apic("check_invalid_merge");
        fs::write(dir.join("template.json"), r#"{ "method": 123 }"#).unwrap();
        assert!(matches!(check_at(&dir), TemplateCheck::Invalid(_)));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn check_at_reports_absent_when_template_missing() {
        let dir = temp_apic("check_absent");
        assert!(matches!(check_at(&dir), TemplateCheck::Absent));
        fs::remove_dir_all(&dir).unwrap();
    }
}
