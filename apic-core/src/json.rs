//! The contract data model plus parsing, validation, URL/JSON formatting, and
//! discovery of JSON contract files beneath a project root.

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
    HEAD,
    OPTIONS,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JsonContent {
    pub name: String,
    pub description: Option<String>,
    pub method: Method,
    /// Free-form request URL, e.g. `https://api.example.com/v1/users/{id}`.
    /// Path params are inline `{name}` tokens; no structured URL parts.
    pub url: String,
    /// Documented query parameters (`?key=value`), kept alongside the url.
    #[serde(default)]
    pub query: Vec<Query>,
    pub headers: Vec<Header>,
    /// The request body: the raw JSON payload directly (no wrapper key). `None`
    /// when the endpoint has no request body.
    #[serde(default)]
    pub request: Option<serde_json::Value>,
    pub responses: Vec<Response>,
}

/// A documented query parameter: `name` is the key, `value` an example value,
/// `description` optional prose. Serializes with all three keys.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Query {
    pub name: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Header {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Response {
    pub code: u16,
    pub description: String,

    #[serde(default)]
    pub headers: Vec<Header>,

    /// The response body: the raw JSON payload for this response.
    #[serde(default)]
    pub schema: Option<serde_json::Value>,
}

pub fn method_str(method: &Method) -> String {
    match method {
        Method::GET => "GET".to_string(),
        Method::POST => "POST".to_string(),
        Method::PUT => "PUT".to_string(),
        Method::PATCH => "PATCH".to_string(),
        Method::DELETE => "DELETE".to_string(),
        Method::HEAD => "HEAD".to_string(),
        Method::OPTIONS => "OPTIONS".to_string(),
    }
}

/// All HTTP methods in a fixed order, for cycling through choices in the TUI.
pub fn method_all() -> [Method; 7] {
    [
        Method::GET,
        Method::POST,
        Method::PUT,
        Method::PATCH,
        Method::DELETE,
        Method::HEAD,
        Method::OPTIONS,
    ]
}

/// Pretty-prints a JSON string with four-space indentation, reformatting only
/// whitespace so numbers, key order, and string contents are preserved exactly
/// (unlike a serde round-trip). Input is assumed to be valid JSON; it is not
/// validated, invalid input simply yields rearranged whitespace.
///
/// This is a trimmed, dependency-free adaptation of the pretty-printer from
/// jsonxf (gamache/jsonxf, MIT/Apache-2.0), keeping only what apic uses.
pub fn pretty_json(input: &str) -> String {
    fn newline_indent(out: &mut Vec<u8>, depth: usize) {
        out.push(b'\n');
        for _ in 0..depth {
            out.extend_from_slice(b"    ");
        }
    }

    let mut out: Vec<u8> = Vec::with_capacity(input.len() + input.len() / 4);
    let mut depth = 0usize;
    let mut in_string = false;
    let mut in_backslash = false;
    let mut empty = false; // inside a container that has no members yet

    for &b in input.as_bytes() {
        if in_string {
            out.push(b);
            if in_backslash {
                in_backslash = false;
            } else if b == b'\\' {
                in_backslash = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b' ' | b'\n' | b'\r' | b'\t' => {} // collapse insignificant whitespace
            b'[' | b'{' => {
                if empty {
                    newline_indent(&mut out, depth);
                }
                out.push(b);
                depth += 1;
                empty = true;
            }
            b']' | b'}' => {
                depth = depth.saturating_sub(1);
                if empty {
                    empty = false;
                } else {
                    newline_indent(&mut out, depth);
                }
                out.push(b);
            }
            b',' => {
                out.push(b',');
                newline_indent(&mut out, depth);
            }
            b':' => out.extend_from_slice(b": "),
            _ => {
                if empty {
                    newline_indent(&mut out, depth);
                    empty = false;
                }
                if b == b'"' {
                    in_string = true;
                }
                out.push(b);
            }
        }
    }
    // `out` is the input bytes plus ASCII whitespace, so it stays valid UTF-8.
    String::from_utf8(out).unwrap_or_else(|_| input.to_string())
}

// Scaffolding for the upcoming `method set` command; not yet wired in.
#[allow(dead_code)]
pub(crate) fn method_from_str(method: &str) -> Method {
    match method.to_uppercase().as_str() {
        "GET" => Method::GET,
        "POST" => Method::POST,
        "PUT" => Method::PUT,
        "PATCH" => Method::PATCH,
        "DELETE" => Method::DELETE,
        "HEAD" => Method::HEAD,
        "OPTIONS" => Method::OPTIONS,
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

    #[test]
    fn pretty_json_indents_with_four_spaces() {
        assert_eq!(pretty_json(r#"{"a":1}"#), "{\n    \"a\": 1\n}");
    }

    #[test]
    fn pretty_json_handles_nesting_and_empties() {
        assert_eq!(
            pretty_json(r#"{"empty":{},"one":[1]}"#),
            "{\n    \"empty\": {},\n    \"one\": [\n        1\n    ]\n}"
        );
    }

    #[test]
    fn pretty_json_preserves_string_punctuation_and_numbers() {
        // Braces/commas inside strings and exact number tokens are untouched.
        assert_eq!(
            pretty_json(r#"{"msg":"a, b: {c}","n":1.50}"#),
            "{\n    \"msg\": \"a, b: {c}\",\n    \"n\": 1.50\n}"
        );
    }

    const CONTRACT: &str = r#"{
        "name": "t",
        "method": "GET",
        "url": "https://api.example.com/t",
        "headers": [],
        "responses": [
            { "code": 200, "description": "ok", "schema": [] },
            { "code": 404, "description": "no", "schema": [] }
        ]
    }"#;

    #[test]
    fn url_is_a_string_and_query_headers_round_trip() {
        let json = r#"{
            "name": "u", "method": "GET",
            "url": "https://api.example.com/v1/users/{id}",
            "query": [{ "name": "page", "value": "2", "description": "page number" }],
            "headers": [],
            "responses": [
                { "code": 200, "description": "ok",
                  "headers": [{ "name": "X-Req", "value": "abc" }] }
            ]
        }"#;
        let c = json_get(json, None).unwrap();
        assert_eq!(c.url, "https://api.example.com/v1/users/{id}");
        assert_eq!(c.query[0].name, "page");
        assert_eq!(c.query[0].value, "2");
        assert_eq!(c.responses[0].headers[0].name, "X-Req");
        // query + response headers default to empty when absent
        let bare = r#"{ "name":"b","method":"GET","url":"/x","headers":[],
            "responses":[{ "code":200,"description":"ok" }] }"#;
        let b = json_get(bare, None).unwrap();
        assert!(b.query.is_empty());
        assert!(b.responses[0].headers.is_empty());
    }

    #[test]
    fn header_and_query_required_parse_and_default() {
        let json = r#"{
            "name": "u", "method": "GET", "url": "https://h/x",
            "query": [{ "name": "page", "value": "1", "required": true }],
            "headers": [
                { "name": "Content-Type", "value": "application/json", "required": true },
                { "name": "X-Optional", "value": "y" }
            ],
            "responses": []
        }"#;
        let c = json_get(json, None).unwrap();
        assert!(c.headers[0].required);
        assert!(!c.headers[1].required); // defaults false when absent
        assert!(c.query[0].required);
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
    fn request_and_response_parse_body_json() {
        let json = r#"{
            "name": "login", "method": "POST",
            "url": "https://api.example.com/l",
            "headers": [],
            "request": { "username": "rizukirr", "password": "123qweA@" },
            "responses": [
                { "code": 200, "description": "ok",
                  "schema": { "status": 200, "message": "welcome" } }
            ]
        }"#;
        let c = json_get(json, None).unwrap();
        // The request body is the raw JSON directly; a response body is under `schema`.
        assert_eq!(c.request.unwrap()["username"], "rizukirr");
        assert_eq!(c.responses[0].schema.as_ref().unwrap()["status"], 200);
    }

    #[test]
    fn bodies_default_to_none_when_absent() {
        let json = r#"{
            "name": "t", "method": "POST", "url": "/x", "headers": [],
            "responses": [ { "code": 200, "description": "ok" } ]
        }"#;
        let c = json_get(json, None).unwrap();
        assert!(c.request.is_none());
        assert!(c.responses[0].schema.is_none());
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
