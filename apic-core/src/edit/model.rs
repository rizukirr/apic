//! Mutable working copy of a contract while it is being edited.
//!
//! Mirrors [`crate::json::JsonContent`] but stores free-text, numeric, and
//! example fields as raw `String` buffers so half-typed input is always a
//! valid in-memory state. Conversion to a real contract happens only on save.

use crate::json::Method;

/// The whole contract under edit.
#[derive(Debug, Clone, PartialEq)]
pub struct EditModel {
    pub name: String,
    pub description: String, // empty => None
    pub method: Method,
    pub url: String,
    pub query: Vec<EditQuery>,
    pub headers: Vec<EditHeader>,
    pub request: Option<EditBody>,
    pub multipart: Vec<EditPart>, // empty => no multipart body
    pub responses: Vec<EditResponse>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditHeader {
    pub name: String,
    pub value: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditQuery {
    pub name: String,
    pub value: String,
    pub description: String, // empty => None
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditPart {
    pub name: String,
    pub value: String,
    pub filename: String, // empty => None, and a file part is one with a filename
    pub content_type: String, // empty => None
    pub description: String, // empty => None
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditBody {
    pub example: String, // raw JSON text; empty => None
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditResponse {
    pub code: String, // numeric text; parsed to u16 on save
    pub description: String,
    pub headers: Vec<EditHeader>,
    pub example: String,          // raw JSON text; empty => None
    pub multipart: Vec<EditPart>, // empty => no multipart body
}

impl EditBody {
    /// A request body with no example.
    pub fn empty() -> Self {
        EditBody {
            example: String::new(),
        }
    }
}

impl EditResponse {
    /// A new response shell defaulting to `200`, the most common code (editable).
    pub fn blank() -> Self {
        EditResponse {
            code: "200".to_string(),
            description: String::new(),
            headers: Vec::new(),
            example: String::new(),
            multipart: Vec::new(),
        }
    }
}

use crate::json::{Header, JsonContent, Part, Query, Response};
use serde_json::Value;

/// Pretty-prints a JSON example value to raw text (4-space indent), or empty
/// string when absent. Mirrors the on-disk formatting.
fn example_to_text(value: Option<&Value>) -> String {
    match value {
        Some(v) => crate::template::render_pretty(v).unwrap_or_default(),
        None => String::new(),
    }
}

fn opt_to_string(opt: Option<String>) -> String {
    opt.unwrap_or_default()
}

/// Lifts contract parts into their editable form, mapping absent optional
/// strings to empty buffers the way `opt_to_string` does for every other
/// optional field on the model.
fn parts_in(parts: Vec<Part>) -> Vec<EditPart> {
    parts
        .into_iter()
        .map(|p| EditPart {
            name: p.name,
            value: p.value,
            filename: opt_to_string(p.filename),
            content_type: opt_to_string(p.content_type),
            description: opt_to_string(p.description),
            required: p.required,
        })
        .collect()
}

impl EditModel {
    /// Lifts a parsed contract into an editable working copy.
    pub fn from_contract(c: JsonContent) -> Self {
        EditModel {
            name: c.name,
            description: opt_to_string(c.description),
            method: c.method,
            url: c.url,
            query: c
                .query
                .into_iter()
                .map(|q: Query| EditQuery {
                    name: q.name,
                    value: q.value,
                    description: opt_to_string(q.description),
                    required: q.required,
                })
                .collect(),
            headers: c
                .headers
                .into_iter()
                .map(|h: Header| EditHeader {
                    name: h.name,
                    value: h.value,
                    required: h.required,
                })
                .collect(),
            request: c.request.map(|body: Value| EditBody {
                example: example_to_text(Some(&body)),
            }),
            multipart: c.multipart.map(parts_in).unwrap_or_default(),
            responses: c
                .responses
                .into_iter()
                .map(|r: Response| EditResponse {
                    code: r.code.to_string(),
                    description: r.description,
                    headers: r
                        .headers
                        .into_iter()
                        .map(|h: Header| EditHeader {
                            name: h.name,
                            value: h.value,
                            required: h.required,
                        })
                        .collect(),
                    example: example_to_text(r.schema.as_ref()),
                    multipart: r.multipart.map(parts_in).unwrap_or_default(),
                })
                .collect(),
        }
    }
}

use std::path::Path;

fn str_opt(s: &str) -> Option<&str> {
    if s.trim().is_empty() { None } else { Some(s) }
}

/// Parses a raw example buffer into a JSON value, or `None` when blank.
/// Returns a contextual error (mentioning "example") on malformed input.
fn parse_example(raw: &str, ctx: &str) -> Result<Option<Value>, String> {
    if raw.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str::<Value>(raw)
        .map(Some)
        .map_err(|err| format!("{ctx} example is not valid JSON: {err}"))
}

/// Serializes editable parts back to a `multipart` array, omitting each
/// optional string whose buffer is blank so a part never gains an empty key.
/// Mirrors the hand-built map style `to_json` already uses for `query`.
fn parts_out(parts: &[EditPart]) -> Value {
    Value::Array(
        parts
            .iter()
            .map(|p| {
                let mut m = serde_json::Map::new();
                m.insert("name".into(), Value::String(p.name.clone()));
                if let Some(v) = str_opt(&p.value) {
                    m.insert("value".into(), Value::String(v.to_string()));
                }
                m.insert("required".into(), Value::Bool(p.required));
                if let Some(f) = str_opt(&p.filename) {
                    m.insert("filename".into(), Value::String(f.to_string()));
                }
                if let Some(c) = str_opt(&p.content_type) {
                    m.insert("contentType".into(), Value::String(c.to_string()));
                }
                if let Some(d) = str_opt(&p.description) {
                    m.insert("description".into(), Value::String(d.to_string()));
                }
                Value::Object(m)
            })
            .collect(),
    )
}

impl EditModel {
    /// Serializes the model to a pretty, valid contract string.
    ///
    /// Returns `Err` (never panics) when an example buffer is malformed JSON, a
    /// response code is non-numeric, or the assembled document fails contract
    /// validation. The error is suitable for display on the TUI status line.
    pub fn to_json(&self) -> Result<String, String> {
        let mut root = serde_json::Map::new();
        root.insert("name".into(), Value::String(self.name.clone()));
        if let Some(d) = str_opt(&self.description) {
            root.insert("description".into(), Value::String(d.to_string()));
        }
        root.insert(
            "method".into(),
            Value::String(crate::json::method_str(&self.method)),
        );

        root.insert("url".into(), Value::String(self.url.clone()));

        if !self.query.is_empty() {
            root.insert(
                "query".into(),
                Value::Array(
                    self.query
                        .iter()
                        .map(|q| {
                            let mut m = serde_json::Map::new();
                            m.insert("name".into(), Value::String(q.name.clone()));
                            m.insert("value".into(), Value::String(q.value.clone()));
                            m.insert("required".into(), Value::Bool(q.required));
                            if let Some(d) = str_opt(&q.description) {
                                m.insert("description".into(), Value::String(d.to_string()));
                            }
                            Value::Object(m)
                        })
                        .collect(),
                ),
            );
        }

        // headers (always present, possibly empty array)
        root.insert(
            "headers".into(),
            Value::Array(
                self.headers
                    .iter()
                    .map(|h| {
                        let mut m = serde_json::Map::new();
                        m.insert("name".into(), Value::String(h.name.clone()));
                        m.insert("value".into(), Value::String(h.value.clone()));
                        m.insert("required".into(), Value::Bool(h.required));
                        Value::Object(m)
                    })
                    .collect(),
            ),
        );

        // request (optional): the raw body value written directly under
        // `request`, only when there is one (an empty buffer is not persisted).
        if let Some(req) = &self.request
            && let Some(body) = parse_example(&req.example, "request")?
        {
            root.insert("request".into(), body);
        }

        // multipart (optional): omitted when empty, the same way `query` is,
        // so a contract that has no parts never gains the key.
        if !self.multipart.is_empty() {
            root.insert("multipart".into(), parts_out(&self.multipart));
        }

        // responses (always present, possibly empty)
        let mut responses = Vec::new();
        for (i, r) in self.responses.iter().enumerate() {
            let code: u16 = r.code.trim().parse().map_err(|_| {
                format!(
                    "response #{}: status code '{}' is not a number (e.g. 200)",
                    i + 1,
                    r.code
                )
            })?;
            let mut m = serde_json::Map::new();
            m.insert("code".into(), Value::Number(code.into()));
            m.insert("description".into(), Value::String(r.description.clone()));
            if !r.headers.is_empty() {
                m.insert(
                    "headers".into(),
                    Value::Array(
                        r.headers
                            .iter()
                            .map(|h| {
                                let mut mm = serde_json::Map::new();
                                mm.insert("name".into(), Value::String(h.name.clone()));
                                mm.insert("value".into(), Value::String(h.value.clone()));
                                mm.insert("required".into(), Value::Bool(h.required));
                                Value::Object(mm)
                            })
                            .collect(),
                    ),
                );
            }
            if let Some(body) = parse_example(&r.example, &format!("response {code}"))? {
                m.insert("schema".into(), body);
            }
            if !r.multipart.is_empty() {
                m.insert("multipart".into(), parts_out(&r.multipart));
            }
            responses.push(Value::Object(m));
        }
        root.insert("responses".into(), Value::Array(responses));

        let contract = crate::template::render_pretty(&Value::Object(root))?;
        crate::json::validate(&contract).map_err(|err| format!("invalid contract: {err}"))?;
        Ok(contract)
    }

    /// Serializes and writes the contract to `path`, creating parent dirs.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let contract = self.to_json()?;
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
        }
        std::fs::write(path, contract)
            .map_err(|err| format!("failed to write {}: {err}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::json_get;

    const FULL: &str = r#"{
        "name": "login",
        "description": "Log a user in",
        "method": "POST",
        "url": "https://api.example.com/auth/{id}",
        "query": [{ "name": "page", "value": "1", "description": "Page", "required": true }],
        "headers": [{ "name": "Content-Type", "value": "application/json", "required": true }],
        "request": { "user": { "email": "a@b.c" } },
        "responses": [{ "code": 200, "description": "ok", "schema": { "token": "x" } }]
    }"#;

    #[test]
    fn from_contract_lifts_all_fields() {
        // A dedicated contract, not `FULL`: it adds a `multipart` array on the
        // request and on a response so both arrive on the model, without
        // disturbing `FULL`'s use by the other tests below.
        const WITH_MULTIPART: &str = r#"{
            "name": "login",
            "description": "Log a user in",
            "method": "POST",
            "url": "https://api.example.com/auth/{id}",
            "query": [{ "name": "page", "value": "1", "description": "Page", "required": true }],
            "headers": [{ "name": "Content-Type", "value": "application/json", "required": true }],
            "request": { "user": { "email": "a@b.c" } },
            "multipart": [{ "name": "avatar", "value": "", "filename": "photo.png", "contentType": "image/png", "required": true }],
            "responses": [{ "code": 200, "description": "ok", "schema": { "token": "x" },
                "multipart": [{ "name": "thumb", "value": "", "filename": "t.png", "required": false }] }]
        }"#;
        let contract = json_get(WITH_MULTIPART, None).unwrap();
        let m = EditModel::from_contract(contract);

        assert_eq!(m.name, "login");
        assert_eq!(m.description, "Log a user in");
        assert_eq!(m.method, Method::POST);
        assert_eq!(m.url, "https://api.example.com/auth/{id}");
        assert_eq!(m.query[0].name, "page");
        assert_eq!(m.query[0].value, "1");
        assert_eq!(m.headers[0].name, "Content-Type");

        let req = m.request.as_ref().unwrap();
        // example is pretty-printed raw text containing the key
        assert!(req.example.contains("\"email\""));

        assert_eq!(m.multipart[0].name, "avatar");
        assert_eq!(m.multipart[0].filename, "photo.png");

        assert_eq!(m.responses[0].code, "200");
        assert!(m.responses[0].example.contains("\"token\""));
        assert_eq!(m.responses[0].multipart[0].filename, "t.png");
    }

    #[test]
    fn roundtrip_preserves_contract() {
        let contract = json_get(FULL, None).unwrap();
        let model = EditModel::from_contract(contract);
        let json = model.to_json().expect("valid model serializes");
        // Re-parse: the produced JSON must be a valid contract with the same shape.
        let back = json_get(&json, None).unwrap();
        assert_eq!(back.name, "login");
        assert_eq!(back.url, "https://api.example.com/auth/{id}");
        assert_eq!(back.query[0].value, "1");
        assert!(back.headers[0].required);
        assert!(back.query[0].required);
        assert_eq!(back.responses[0].code, 200);
        assert_eq!(back.request.unwrap()["user"]["email"], "a@b.c");
    }

    #[test]
    fn invalid_example_is_rejected() {
        let contract = json_get(FULL, None).unwrap();
        let mut model = EditModel::from_contract(contract);
        model.responses[0].example = "{ not json".to_string();
        let err = model.to_json().unwrap_err();
        assert!(err.to_lowercase().contains("example"));
    }

    #[test]
    fn empty_request_body_is_omitted() {
        let contract = json_get(FULL, None).unwrap();
        let mut model = EditModel::from_contract(contract);
        model.request.as_mut().unwrap().example = String::new();
        let json = model.to_json().unwrap();
        let back = json_get(&json, None).unwrap();
        // A request body with no example is dropped entirely.
        assert!(back.request.is_none());
    }

    #[test]
    fn non_numeric_response_code_is_rejected() {
        let contract = json_get(FULL, None).unwrap();
        let mut model = EditModel::from_contract(contract);
        model.responses[0].code = "2xx".to_string();
        let err = model.to_json().unwrap_err();
        assert!(err.to_lowercase().contains("code"));
    }

    #[test]
    fn roundtrip_preserves_multipart_parts() {
        // `to_json` builds its map by hand rather than serializing
        // `JsonContent`, so a key it was never taught about is dropped in
        // silence. This is the check that catches that.
        let contract = r#"{
            "name": "Upload avatar",
            "method": "POST",
            "url": "https://h/u",
            "headers": [],
            "multipart": [
                { "name": "avatar", "value": "", "filename": "photo.png", "contentType": "image/png", "required": true },
                { "name": "caption", "value": "my holiday", "required": false }
            ],
            "responses": [
                { "code": 200, "description": "ok",
                  "multipart": [ { "name": "thumb", "value": "", "filename": "t.png", "required": false } ] }
            ]
        }"#;
        let model =
            EditModel::from_contract(crate::json::json_get(contract, None).expect("parses"));
        let out = model.to_json().expect("serializes");
        let back = crate::json::json_get(&out, None).expect("reparses");

        let parts = back.multipart.expect("request parts survived");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].name, "avatar");
        assert_eq!(parts[0].filename.as_deref(), Some("photo.png"));
        assert_eq!(parts[0].content_type.as_deref(), Some("image/png"));
        assert!(parts[0].required);
        assert_eq!(parts[1].value, "my holiday");
        assert!(parts[1].filename.is_none());
        assert!(parts[1].content_type.is_none());

        let rparts = back.responses[0]
            .multipart
            .as_ref()
            .expect("response parts survived");
        assert_eq!(rparts[0].filename.as_deref(), Some("t.png"));
    }

    #[test]
    fn example_contracts_roundtrip_without_gaining_a_multipart_key() {
        // The real contracts people have on disk, not a synthetic fixture. A
        // wrong `skip_serializing_if` or an unconditional insert would add a
        // multipart key to every one of them on first save.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../example");
        let paths = crate::json::scan_json_file(&dir, true).expect("example contracts found");
        assert!(
            !paths.is_empty(),
            "no example contracts under {}",
            dir.display()
        );
        for path in paths {
            let text = std::fs::read_to_string(&path).expect("readable");
            let model = EditModel::from_contract(
                crate::json::json_get(&text, None)
                    .unwrap_or_else(|e| panic!("{}: {e}", path.display())),
            );
            let out = model
                .to_json()
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert!(
                !out.contains("multipart"),
                "{} gained a multipart key",
                path.display()
            );
            crate::json::validate(&out)
                .unwrap_or_else(|e| panic!("{} no longer validates: {e}", path.display()));
        }
    }

    #[test]
    fn a_file_part_is_written_without_an_empty_value() {
        // A file part has no text value, and writing `"value": ""` puts a
        // field in the file that means nothing and reads as if the part had
        // an empty text value.
        let contract = r#"{
            "name": "x", "method": "POST", "url": "https://h", "headers": [],
            "multipart": [{ "name": "avatar", "filename": "a.png", "required": true }],
            "responses": []
        }"#;
        let model =
            EditModel::from_contract(crate::json::json_get(contract, None).expect("parses"));
        let out = model.to_json().expect("serializes");
        assert!(!out.contains("\"value\""), "empty value was written: {out}");

        // The part itself still has to survive the omission.
        let back = crate::json::json_get(&out, None).expect("reparses");
        let parts = back.multipart.expect("parts survived");
        assert_eq!(parts[0].filename.as_deref(), Some("a.png"));
        assert!(parts[0].required);
    }
}
