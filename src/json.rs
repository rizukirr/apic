//! Discovery of JSOj contract files beneath a project root.

use crate::file::{FindFileResult, find_file_by_ext_downward};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[allow(clippy::upper_case_acronyms)]
pub enum Method {
    GET,
    POST,
    PUT,
    PATCH,
    DELETE,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JsonContent {
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) method: Method,
    pub(crate) url: Url,
    pub(crate) headers: Vec<Header>,
    pub(crate) request: Option<RequestBody>,
    pub(crate) responses: Vec<Response>,
}

/// The request body section: a field-level schema, a raw JSON example payload,
/// or both. Either part may be omitted — early-stage contracts often have only
/// an example, formal ones only a schema.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RequestBody {
    /// Body shape: `"object"` (default) or an array form like `"object[]"`.
    #[serde(alias = "type", default = "default_body_type")]
    pub(crate) dtype: String,
    #[serde(default)]
    pub(crate) schema: Option<Vec<Schema>>,
    #[serde(default)]
    pub(crate) example: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Url {
    pub(crate) protocol: String,
    pub(crate) host: String,
    pub(crate) path: Option<Vec<String>>,
    pub(crate) query: Option<Vec<Query>>,
    pub(crate) variable: Option<Vec<Variable>>,
}

/// A path variable, e.g. `id` in `/resource/{id}`. The path segment carries the
/// `{id}` placeholder; this documents what it means. `type` defaults to
/// `string` when omitted, and `required` defaults to `false` when omitted.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Variable {
    pub(crate) name: String,
    #[serde(alias = "type", default = "default_variable_type")]
    pub(crate) dtype: String,
    pub(crate) description: Option<String>,
    #[serde(default)]
    pub(crate) required: bool,
}

/// Path variables default to `string` when `type` is omitted.
fn default_variable_type() -> String {
    "string".to_string()
}

/// Request/response bodies default to a single `object` when `type` is omitted.
fn default_body_type() -> String {
    "object".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Query {
    pub(crate) name: String,
    pub(crate) value: String,
    pub(crate) description: Option<String>,
    pub(crate) required: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Header {
    pub(crate) name: String,
    pub(crate) value: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Response {
    pub code: u16,
    pub description: String,
    /// Body shape: `"object"` (default) or an array form like `"object[]"`.
    #[serde(alias = "type", default = "default_body_type")]
    pub dtype: String,
    /// Field-level schema; may be omitted when only an example is provided.
    #[serde(default)]
    pub schema: Vec<Schema>,
    /// Raw JSON example payload for this response.
    #[serde(default)]
    pub example: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Schema {
    pub(crate) name: String,
    #[serde(alias = "type")]
    pub(crate) dtype: String,
    pub(crate) default: Option<String>,
    pub(crate) description: String,
    pub(crate) required: bool,
    #[serde(default)]
    pub(crate) properties: Option<Vec<Schema>>,
    /// Accepted MIME types for `file` fields in multipart requests, e.g.
    /// `"image/png, image/jpeg"`. Omitted for ordinary fields.
    #[serde(default)]
    pub(crate) accept: Option<String>,
}

pub fn method_str(method: &Method) -> String {
    match method {
        Method::GET => "GET".to_string(),
        Method::POST => "POST".to_string(),
        Method::PUT => "PUT".to_string(),
        Method::PATCH => "PATCH".to_string(),
        Method::DELETE => "DELETE".to_string(),
    }
}

/// All HTTP methods in a fixed order, for cycling through choices in the TUI.
#[allow(dead_code)]
pub(crate) fn method_all() -> [Method; 5] {
    [
        Method::GET,
        Method::POST,
        Method::PUT,
        Method::PATCH,
        Method::DELETE,
    ]
}

/// Splits a type string into its base type and array-ness:
/// `"object[]" -> ("object", true)`, `"string" -> ("string", false)`.
pub fn parse_type(dtype: &str) -> (&str, bool) {
    match dtype.strip_suffix("[]") {
        Some(base) => (base, true),
        None => (dtype, false),
    }
}

/// True if any field — at any depth via `properties` — declares `accept`.
/// Shared by the plain-text and TUI field-table renderers.
pub(crate) fn any_accept(fields: &[Schema]) -> bool {
    fields
        .iter()
        .any(|f| f.accept.is_some() || f.properties.as_deref().is_some_and(any_accept))
}

// Scaffolding for the upcoming `method set` command; not yet wired in.
#[allow(dead_code)]
pub fn method_from_str(method: &str) -> Method {
    match method.to_uppercase().as_str() {
        "GET" => Method::GET,
        "POST" => Method::POST,
        "PUT" => Method::PUT,
        "PATCH" => Method::PATCH,
        "DELETE" => Method::DELETE,
        _ => Method::GET,
    }
}

/// Finds `.json` files under `root`.
///
/// Paths are returned absolute when `is_absolute` is `true`, otherwise
/// relative to `root`. Returns `None` when no JSON files exist.
pub fn scan_json_file(root: &Path, is_absolute: bool) -> Option<Vec<PathBuf>> {
    let json_file = match find_file_by_ext_downward(root.to_path_buf(), &["json"]) {
        FindFileResult::Found(files) => files,
        FindFileResult::NotFound => return None,
    };

    let files = json_file
        .into_iter()
        .map(|p| {
            if is_absolute {
                p
            } else {
                match p.strip_prefix(root) {
                    Ok(rel) => rel.to_path_buf(),
                    Err(_) => p,
                }
            }
        })
        .collect();

    Some(files)
}

/// Validates that `json` parses as a well-formed contract.
///
/// # Errors
///
/// Returns the parse error (with line/column) when the document does not
/// conform to the contract schema.
pub fn validate(json: &str) -> Result<(), serde_json::Error> {
    serde_json::from_str::<JsonContent>(json).map(|_| ())
}

/// Parses a JSON contract, keeping only the responses whose code matches
/// `status` (all responses when `status` is `None`).
pub fn json_get(json: &str, status: Option<u16>) -> Result<JsonContent, serde_json::Error> {
    let mut json_content: JsonContent = serde_json::from_str(json)?;
    if let Some(status) = status {
        json_content.responses.retain(|r| r.code == status);
    }
    Ok(json_content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const CONTRACT: &str = r#"{
        "name": "t",
        "method": "GET",
        "url": { "protocol": "https", "host": "api.example.com", "path": ["t"] },
        "headers": [],
        "responses": [
            { "code": 200, "description": "ok", "schema": [] },
            { "code": 404, "description": "no", "schema": [] }
        ]
    }"#;

    #[test]
    fn parse_type_splits_the_array_suffix() {
        assert_eq!(parse_type("object[]"), ("object", true));
        assert_eq!(parse_type("string[]"), ("string", true));
        assert_eq!(parse_type("object"), ("object", false));
        assert_eq!(parse_type("string"), ("string", false));
    }

    #[test]
    fn any_accept_detects_a_declared_accept_at_any_depth() {
        // No field declares `accept`.
        let plain = json_get(
            r#"{ "name":"t","method":"GET",
                 "url":{"protocol":"h","host":"h","path":["x"]},"headers":[],
                 "responses":[{"code":200,"description":"ok","schema":[
                   {"name":"f","type":"string","default":null,"description":"d","required":true}
                 ]}] }"#,
            None,
        )
        .unwrap();
        assert!(!any_accept(&plain.responses[0].schema));

        // A nested `file` field declares `accept`.
        let nested = json_get(
            r#"{ "name":"t","method":"GET",
                 "url":{"protocol":"h","host":"h","path":["x"]},"headers":[],
                 "responses":[{"code":200,"description":"ok","schema":[
                   {"name":"wrap","type":"object","default":null,"description":"d","required":true,
                    "properties":[
                      {"name":"avatar","type":"file","default":null,"description":"d","required":true,"accept":"image/png"}
                    ]}
                 ]}] }"#,
            None,
        )
        .unwrap();
        assert!(any_accept(&nested.responses[0].schema));
    }

    #[test]
    fn json_get_returns_all_responses_when_status_is_none() {
        let c = json_get(CONTRACT, None).unwrap();
        assert_eq!(c.responses.len(), 2);
        assert_eq!(c.name, "t");
    }

    #[test]
    fn json_get_filters_to_a_single_status() {
        let c = json_get(CONTRACT, Some(404)).unwrap();
        assert_eq!(c.responses.len(), 1);
        assert_eq!(c.responses[0].code, 404);
    }

    #[test]
    fn json_get_returns_empty_when_status_matches_nothing() {
        let c = json_get(CONTRACT, Some(500)).unwrap();
        assert!(c.responses.is_empty());
    }

    #[test]
    fn validate_accepts_well_formed_contract() {
        assert!(validate(CONTRACT).is_ok());
    }

    #[test]
    fn request_field_parses_optional_accept_for_multipart() {
        let json = r#"{
            "name": "upload", "method": "POST",
            "url": { "protocol": "https", "host": "api.example.com", "path": ["u"] },
            "headers": [],
            "request": {
                "schema": [
                    { "name": "avatar", "type": "file", "default": null,
                      "description": "Image", "required": true,
                      "accept": "image/png" },
                    { "name": "caption", "type": "string", "default": null,
                      "description": "Text", "required": false }
                ]
            },
            "responses": []
        }"#;
        let c = json_get(json, None).unwrap();
        let schema = c.request.unwrap().schema.unwrap();
        assert_eq!(schema[0].accept.as_deref(), Some("image/png"));
        assert_eq!(schema[1].accept, None);
    }

    #[test]
    fn request_parses_example_only_without_schema() {
        let json = r#"{
            "name": "login", "method": "POST",
            "url": { "protocol": "https", "host": "api.example.com", "path": ["l"] },
            "headers": [],
            "request": {
                "example": { "username": "rizukirr", "password": "123qweA@" }
            },
            "responses": [
                { "code": 200, "description": "ok",
                  "example": { "status": 200, "message": "welcome" } }
            ]
        }"#;
        let c = json_get(json, None).unwrap();
        let request = c.request.unwrap();
        assert!(request.schema.is_none());
        assert_eq!(request.example.unwrap()["username"], "rizukirr");
        // Response: schema omitted defaults to empty; example parsed.
        assert!(c.responses[0].schema.is_empty());
        assert_eq!(c.responses[0].example.as_ref().unwrap()["status"], 200);
    }

    #[test]
    fn variable_type_defaults_to_string_when_omitted() {
        let json = r#"{
            "name": "t", "method": "GET",
            "url": {
                "protocol": "https", "host": "api.example.com", "path": ["u", "{id}"],
                "variable": [
                    { "name": "id", "description": "User ID" },
                    { "name": "slug", "type": "int", "description": "Slug", "required": true }
                ]
            },
            "headers": [], "responses": []
        }"#;
        let c = json_get(json, None).unwrap();
        let variable = c.url.variable.unwrap();
        assert_eq!(variable[0].dtype, "string");
        assert_eq!(variable[1].dtype, "int");
        // `required` defaults to false when omitted, and parses an explicit true.
        assert!(!variable[0].required);
        assert!(variable[1].required);
    }

    #[test]
    fn body_type_parses_array_and_defaults_to_object() {
        let json = r#"{
            "name": "t", "method": "POST",
            "url": { "protocol": "https", "host": "h", "path": ["x"] },
            "headers": [],
            "request": { "type": "object[]", "schema": [
                { "name": "id", "type": "string", "default": null, "description": "d", "required": true }
            ] },
            "responses": [ { "code": 200, "description": "ok" } ]
        }"#;
        let c = json_get(json, None).unwrap();
        assert_eq!(c.request.as_ref().unwrap().dtype, "object[]");
        // Response omits "type" -> defaults to "object".
        assert_eq!(c.responses[0].dtype, "object");
    }

    #[test]
    fn schema_properties_default_to_none_and_array_type_parses() {
        let json = r#"{
            "name": "t", "method": "GET",
            "url": { "protocol": "https", "host": "h", "path": ["x"] },
            "headers": [],
            "responses": [ { "code": 200, "description": "ok", "schema": [
                { "name": "f", "type": "string[]", "default": null, "description": "d", "required": true }
            ] } ]
        }"#;
        let c = json_get(json, None).unwrap();
        let s = &c.responses[0].schema[0];
        assert_eq!(s.dtype, "string[]");
        assert!(s.properties.is_none());
    }

    #[test]
    fn schema_accept_defaults_to_none_when_omitted() {
        let json = r#"{
            "name": "t", "method": "GET",
            "url": { "protocol": "https", "host": "h", "path": ["x"] },
            "headers": [],
            "responses": [ { "code": 200, "description": "ok", "schema": [
                { "name": "f", "type": "string", "default": null, "description": "d", "required": true }
            ] } ]
        }"#;
        let c = json_get(json, None).unwrap();
        assert!(c.responses[0].schema[0].accept.is_none());
    }

    #[test]
    fn validate_rejects_missing_required_field() {
        // Missing `method`, `url`, `headers`, `responses`.
        assert!(validate(r#"{ "name": "x" }"#).is_err());
    }

    #[test]
    fn json_get_errors_on_invalid_json() {
        assert!(json_get("{ not json", None).is_err());
    }

    #[test]
    fn json_get_rejects_deeply_nested_input_without_overflowing() {
        // serde_json enforces a recursion limit, so a pathologically nested
        // document returns an error instead of overflowing the stack.
        let deep = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
        assert!(json_get(&deep, None).is_err());
    }

    /// Creates a unique, empty temp directory for a single test and removes any
    /// leftover from a previous run.
    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("apic_test_{tag}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn scan_returns_absolute_paths() {
        let root = temp_dir("scan_abs");
        fs::create_dir_all(root.join("a")).unwrap();
        fs::write(root.join("a/x.json"), "{}").unwrap();
        fs::write(root.join("a/y.json"), "{}").unwrap();

        let files = scan_json_file(&root, true).unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|f| f.is_absolute()));

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn scan_returns_relative_paths_when_not_absolute() {
        let root = temp_dir("scan_rel");
        fs::create_dir_all(root.join("a")).unwrap();
        fs::write(root.join("a/x.json"), "{}").unwrap();

        let files = scan_json_file(&root, false).unwrap();
        assert_eq!(files, vec![PathBuf::from("a/x.json")]);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn scan_reports_none_when_empty() {
        let root = temp_dir("scan_empty");
        assert!(scan_json_file(&root, true).is_none());
        fs::remove_dir_all(&root).unwrap();
    }
}
