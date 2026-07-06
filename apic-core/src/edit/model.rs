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
pub struct EditBody {
    pub example: String, // raw JSON text; empty => None
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditResponse {
    pub code: String, // numeric text; parsed to u16 on save
    pub description: String,
    pub headers: Vec<EditHeader>,
    pub example: String, // raw JSON text; empty => None
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
        }
    }
}

use crate::json::{Header, JsonContent, Query, RequestBody, Response};
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
            request: c.request.map(|r: RequestBody| EditBody {
                example: example_to_text(r.example.as_ref()),
            }),
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
                    example: example_to_text(r.example.as_ref()),
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

        // request (optional): only written when it carries an example, so an
        // empty body (the GUI always shows an editable one) is not persisted.
        if let Some(req) = &self.request
            && let Some(ex) = parse_example(&req.example, "request")?
        {
            let mut m = serde_json::Map::new();
            m.insert("example".into(), ex);
            root.insert("request".into(), Value::Object(m));
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
            if let Some(ex) = parse_example(&r.example, &format!("response {code}"))? {
                m.insert("example".into(), ex);
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
        "request": {
            "type": "object",
            "schema": [{ "name": "user", "type": "object", "default": null,
                         "description": "wrap", "required": true, "properties": [
                { "name": "email", "type": "string", "default": null, "description": "Email", "required": true }
            ] }],
            "example": { "user": { "email": "a@b.c" } }
        },
        "responses": [{ "code": 200, "description": "ok", "type": "object",
            "schema": [{ "name": "token", "type": "string", "default": null, "description": "JWT", "required": true }],
            "example": { "token": "x" } }]
    }"#;

    #[test]
    fn from_contract_lifts_all_fields() {
        let contract = json_get(FULL, None).unwrap();
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

        assert_eq!(m.responses[0].code, "200");
        assert!(m.responses[0].example.contains("\"token\""));
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
        assert_eq!(
            back.request.unwrap().example.unwrap()["user"]["email"],
            "a@b.c"
        );
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
}
