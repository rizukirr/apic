//! The contract template used by `apic create`.
//!
//! The default template is embedded at compile time and supplies a complete
//! contract. A project can customize it by editing `.apic/template.json`, which
//! is overlaid onto the default — so the project template only needs to carry
//! the fields it wants to change and may be a small partial contract. `apic
//! init` seeds that file from the default, and it is never overwritten once it
//! exists.

use crate::config::find_apic_dir;
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

/// Template-conformance rules for `apic validate`, loaded once from
/// `.apic/template.json` and reused for every contract.
///
/// The template is treated as a *partial*: only the sections it actually
/// declares are enforced, so an empty template (or none at all) enforces
/// nothing. The checks compare structure — header/field/segment **names** — and,
/// for `url.protocol`/`url.host`, exact **values**; placeholder values elsewhere
/// (descriptions, examples, types) are ignored.
pub struct TemplateRules {
    /// The parsed template, or `None` when there is nothing to enforce
    /// (outside a project, or no template file present).
    template: Option<Value>,
}

/// Loads the project's template-conformance rules. A missing project or missing
/// template file yields rules that enforce nothing; malformed template JSON is an
/// `Err` so `validate` can report it.
pub fn load_rules() -> Result<TemplateRules, String> {
    let apic_dir = match find_apic_dir() {
        Some(dir) => dir,
        None => return Ok(TemplateRules { template: None }),
    };
    let template_path = path(&apic_dir);
    let template_src = match fs::read_to_string(&template_path) {
        Ok(src) => src,
        Err(_) => return Ok(TemplateRules { template: None }),
    };
    let template: Value = serde_json::from_str(&template_src)
        .map_err(|err| format!("{}: is not valid JSON: {err}", template_path.display()))?;
    Ok(TemplateRules {
        template: Some(template),
    })
}

impl TemplateRules {
    /// Returns the conformance issues for the contract `content_json`, one short
    /// message per violation. An empty list means the contract conforms (or the
    /// template enforces nothing). Malformed contract JSON is an `Err`.
    pub fn check(&self, content_json: &str) -> Result<Vec<String>, String> {
        let template = match &self.template {
            Some(template) => template,
            None => return Ok(Vec::new()),
        };
        let contract: Value = serde_json::from_str(content_json)
            .map_err(|err| format!("contract is not valid JSON: {err}"))?;

        let mut issues = Vec::new();
        check_headers(template, &contract, &mut issues);
        check_url(template, &contract, &mut issues);
        check_schema(
            template.pointer("/request/schema"),
            contract.pointer("/request/schema"),
            "request",
            &mut issues,
        );
        check_responses(template, &contract, &mut issues);
        Ok(issues)
    }
}

/// Every header name the template declares must appear in the contract.
/// Header names are compared case-insensitively, matching HTTP semantics.
fn check_headers(template: &Value, contract: &Value, issues: &mut Vec<String>) {
    let required = object_names(template.get("headers"));
    let present = object_names(contract.get("headers"));
    for name in required {
        if !present.iter().any(|have| have.eq_ignore_ascii_case(&name)) {
            issues.push(format!("missing header `{name}`"));
        }
    }
}

/// `url.protocol`/`url.host` must match the template's values exactly; each
/// `path` segment and each `query`/`variable` name the template declares must be
/// present in the contract (extras are allowed).
fn check_url(template: &Value, contract: &Value, issues: &mut Vec<String>) {
    let (Some(t_url), c_url) = (template.get("url"), contract.get("url")) else {
        return;
    };
    let c_url = c_url.unwrap_or(&Value::Null);

    for field in ["protocol", "host"] {
        if let Some(want) = t_url.get(field).and_then(Value::as_str) {
            let got = c_url.get(field).and_then(Value::as_str).unwrap_or("");
            if got != want {
                issues.push(format!("url.{field} must be `{want}` (found `{got}`)"));
            }
        }
    }

    // Path segments are plain strings, not named objects.
    if let Some(Value::Array(segments)) = t_url.get("path") {
        let present: Vec<&str> = match c_url.get("path") {
            Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        for seg in segments.iter().filter_map(Value::as_str) {
            if !present.contains(&seg) {
                issues.push(format!("url.path missing segment `{seg}`"));
            }
        }
    }

    for section in ["query", "variable"] {
        let required = object_names(t_url.get(section));
        let present = object_names(c_url.get(section));
        for name in required {
            if !present.contains(&name) {
                issues.push(format!("url.{section} missing `{name}`"));
            }
        }
    }
}

/// For each response code the template declares, the contract must carry that
/// code and contain every schema field name the template's response declares.
fn check_responses(template: &Value, contract: &Value, issues: &mut Vec<String>) {
    let Some(Value::Array(t_responses)) = template.get("responses") else {
        return;
    };
    let empty = Vec::new();
    let c_responses = match contract.get("responses") {
        Some(Value::Array(items)) => items,
        _ => &empty,
    };
    for t_resp in t_responses {
        let Some(code) = t_resp.get("code").and_then(Value::as_u64) else {
            continue;
        };
        let matched = c_responses
            .iter()
            .find(|c| c.get("code").and_then(Value::as_u64) == Some(code));
        match matched {
            None => issues.push(format!("missing response `{code}`")),
            Some(c_resp) => check_schema(
                t_resp.get("schema"),
                c_resp.get("schema"),
                &format!("response {code}"),
                issues,
            ),
        }
    }
}

/// Every field name declared by the template `schema` (descending into nested
/// `properties`, names joined with `.`) must be declared by the contract schema.
/// `label` prefixes the issue message, e.g. `request` or `response 200`.
fn check_schema(template: Option<&Value>, contract: Option<&Value>, label: &str, issues: &mut Vec<String>) {
    let Some(t_schema) = template else { return };
    let mut required = Vec::new();
    schema_field_names(t_schema, "", &mut required);
    if required.is_empty() {
        return;
    }
    let mut present = Vec::new();
    if let Some(c_schema) = contract {
        schema_field_names(c_schema, "", &mut present);
    }
    for name in required {
        if !present.contains(&name) {
            issues.push(format!("{label} schema missing field `{name}`"));
        }
    }
}

/// Collects the `name` of each object in a JSON array (e.g. headers, query,
/// variable). A non-array or absent value yields an empty list.
fn object_names(array: Option<&Value>) -> Vec<String> {
    match array {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.get("name").and_then(Value::as_str))
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

/// Collects dotted field names from a `schema` array, descending into nested
/// `properties`: `[{name:"data", properties:[{name:"x"}]}]` -> `["data", "data.x"]`.
fn schema_field_names(schema: &Value, prefix: &str, out: &mut Vec<String>) {
    let Value::Array(items) = schema else { return };
    for item in items {
        let Some(name) = item.get("name").and_then(Value::as_str) else {
            continue;
        };
        let full = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}.{name}")
        };
        if let Some(props) = item.get("properties") {
            schema_field_names(props, &full, out);
        }
        out.push(full);
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

    /// Builds rules from a template JSON literal (bypassing the filesystem).
    fn rules(template: &str) -> TemplateRules {
        TemplateRules {
            template: Some(serde_json::from_str(template).unwrap()),
        }
    }

    /// Conformance issues for `contract` against `template`.
    fn issues(template: &str, contract: &str) -> Vec<String> {
        rules(template).check(contract).unwrap()
    }

    #[test]
    fn empty_template_enforces_nothing() {
        assert!(issues("{}", r#"{ "name": "x" }"#).is_empty());
    }

    #[test]
    fn no_template_enforces_nothing() {
        let rules = TemplateRules { template: None };
        assert!(rules.check("{ not even json").unwrap().is_empty());
    }

    #[test]
    fn header_must_be_present_case_insensitive() {
        let template = r#"{ "headers": [ { "name": "Authorization", "value": "" } ] }"#;
        let ok = r#"{ "headers": [ { "name": "authorization", "value": "Bearer t" } ] }"#;
        let bad = r#"{ "headers": [ { "name": "Content-Type", "value": "x" } ] }"#;
        assert!(issues(template, ok).is_empty());
        assert_eq!(issues(template, bad), vec!["missing header `Authorization`"]);
    }

    #[test]
    fn url_protocol_and_host_must_match_exactly() {
        let template = r#"{ "url": { "protocol": "https", "host": "api.example.com" } }"#;
        let bad = r#"{ "url": { "protocol": "http", "host": "other.com" } }"#;
        let found = issues(template, bad);
        assert!(found.iter().any(|i| i == "url.protocol must be `https` (found `http`)"));
        assert!(found.iter().any(|i| i == "url.host must be `api.example.com` (found `other.com`)"));
    }

    #[test]
    fn url_path_query_variable_must_be_present() {
        let template = r#"{ "url": {
            "path": ["resource", "{id}"],
            "query": [ { "name": "page" } ],
            "variable": [ { "name": "id" } ]
        } }"#;
        let bad = r#"{ "url": { "path": ["resource"], "query": [], "variable": [] } }"#;
        let found = issues(template, bad);
        assert!(found.contains(&"url.path missing segment `{id}`".to_string()));
        assert!(found.contains(&"url.query missing `page`".to_string()));
        assert!(found.contains(&"url.variable missing `id`".to_string()));
    }

    #[test]
    fn url_extras_in_contract_are_allowed() {
        let template = r#"{ "url": { "query": [ { "name": "page" } ] } }"#;
        let ok = r#"{ "url": { "query": [ { "name": "page" }, { "name": "limit" } ] } }"#;
        assert!(issues(template, ok).is_empty());
    }

    #[test]
    fn request_schema_field_names_match_recursively() {
        let template = r#"{ "request": { "schema": [
            { "name": "data", "properties": [ { "name": "id" } ] }
        ] } }"#;
        let ok = r#"{ "request": { "schema": [
            { "name": "data", "properties": [ { "name": "id" }, { "name": "extra" } ] }
        ] } }"#;
        let bad = r#"{ "request": { "schema": [ { "name": "data", "properties": [] } ] } }"#;
        assert!(issues(template, ok).is_empty());
        assert_eq!(
            issues(template, bad),
            vec!["request schema missing field `data.id`"]
        );
    }

    #[test]
    fn response_must_have_matching_code_and_schema() {
        let template = r#"{ "responses": [
            { "code": 200, "schema": [ { "name": "status" }, { "name": "message" } ] }
        ] }"#;
        let missing_code = r#"{ "responses": [ { "code": 400, "schema": [] } ] }"#;
        let missing_field = r#"{ "responses": [ { "code": 200, "schema": [ { "name": "status" } ] } ] }"#;
        assert_eq!(issues(template, missing_code), vec!["missing response `200`"]);
        assert_eq!(
            issues(template, missing_field),
            vec!["response 200 schema missing field `message`"]
        );
    }

    #[test]
    fn partial_template_ignores_undeclared_sections() {
        // Only headers declared -> url/request/response are not enforced.
        let template = r#"{ "headers": [ { "name": "Authorization", "value": "" } ] }"#;
        let contract = r#"{ "headers": [ { "name": "Authorization", "value": "x" } ],
            "url": { "protocol": "ftp", "host": "anything" } }"#;
        assert!(issues(template, contract).is_empty());
    }

    #[test]
    fn check_errors_on_malformed_contract() {
        assert!(rules(r#"{ "headers": [] }"#).check("{ not json").is_err());
    }

    #[test]
    fn object_names_collects_names_or_empty() {
        let v: Value = serde_json::from_str(r#"{ "h": [ { "name": "A" }, { "name": "B" } ] }"#).unwrap();
        assert_eq!(object_names(v.get("h")), vec!["A".to_string(), "B".to_string()]);
        assert!(object_names(v.get("missing")).is_empty());
    }

    #[test]
    fn schema_field_names_flattens_nested_properties() {
        let v: Value = serde_json::from_str(
            r#"[ { "name": "data", "properties": [ { "name": "x" } ] }, { "name": "top" } ]"#,
        )
        .unwrap();
        let mut out = Vec::new();
        schema_field_names(&v, "", &mut out);
        assert_eq!(out, vec!["data.x", "data", "top"]);
    }
}
