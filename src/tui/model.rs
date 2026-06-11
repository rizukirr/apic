//! Mutable working copy of a contract while it is being edited.
//!
//! Mirrors [`crate::json::JsonContent`] but stores free-text, numeric, and
//! example fields as raw `String` buffers so half-typed input is always a
//! valid in-memory state. Conversion to a real contract happens only on save.

use crate::json::Method;

/// The whole contract under edit.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EditModel {
    pub name: String,
    pub description: String, // empty => None
    pub method: Method,
    pub url: EditUrl,
    pub headers: Vec<EditHeader>,
    pub request: Option<EditBody>,
    pub responses: Vec<EditResponse>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EditUrl {
    pub protocol: String,
    pub host: String,
    pub path: Vec<String>,
    pub query: Vec<EditQuery>,
    pub variable: Vec<EditVariable>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EditHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EditQuery {
    pub name: String,
    pub value: String,
    pub description: String, // empty => None
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EditVariable {
    pub name: String,
    pub dtype: String,       // defaults to "string" on save when empty
    pub description: String, // empty => None
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EditBody {
    pub dtype: String, // e.g. "object", "object[]"
    pub schema: Vec<EditSchema>,
    pub example: String, // raw JSON text; empty => None
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EditResponse {
    pub code: String, // numeric text; parsed to u16 on save
    pub description: String,
    pub dtype: String,
    pub schema: Vec<EditSchema>,
    pub example: String, // raw JSON text; empty => None
}

/// A schema field. `properties` nests recursively for object types.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EditSchema {
    pub name: String,
    pub dtype: String,
    pub default: String, // empty => None
    pub description: String,
    pub required: bool,
    pub properties: Vec<EditSchema>, // empty => None on save
    pub accept: String,              // empty => None
}

impl EditBody {
    /// A request body with no fields and no example.
    pub fn empty() -> Self {
        EditBody {
            dtype: "object".to_string(),
            schema: Vec::new(),
            example: String::new(),
        }
    }
}

impl EditResponse {
    /// A blank response shell with a `200` default code.
    pub fn blank() -> Self {
        EditResponse {
            code: "200".to_string(),
            description: String::new(),
            dtype: "object".to_string(),
            schema: Vec::new(),
            example: String::new(),
        }
    }
}

use crate::json::{Header, JsonContent, Query, RequestBody, Response, Schema, Variable};
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

fn schema_to_edit(s: Schema) -> EditSchema {
    EditSchema {
        name: s.name,
        dtype: s.dtype,
        default: opt_to_string(s.default),
        description: s.description,
        required: s.required,
        properties: s
            .properties
            .unwrap_or_default()
            .into_iter()
            .map(schema_to_edit)
            .collect(),
        accept: opt_to_string(s.accept),
    }
}

impl EditModel {
    /// Lifts a parsed contract into an editable working copy.
    pub fn from_contract(c: JsonContent) -> Self {
        EditModel {
            name: c.name,
            description: opt_to_string(c.description),
            method: c.method,
            url: EditUrl {
                protocol: c.url.protocol,
                host: c.url.host,
                path: c.url.path.unwrap_or_default(),
                query: c
                    .url
                    .query
                    .unwrap_or_default()
                    .into_iter()
                    .map(|q: Query| EditQuery {
                        name: q.name,
                        value: q.value,
                        description: opt_to_string(q.description),
                        required: q.required,
                    })
                    .collect(),
                variable: c
                    .url
                    .variable
                    .unwrap_or_default()
                    .into_iter()
                    .map(|v: Variable| EditVariable {
                        name: v.name,
                        dtype: v.dtype,
                        description: opt_to_string(v.description),
                        required: v.required,
                    })
                    .collect(),
            },
            headers: c
                .headers
                .into_iter()
                .map(|h: Header| EditHeader {
                    name: h.name,
                    value: h.value,
                })
                .collect(),
            request: c.request.map(|r: RequestBody| EditBody {
                dtype: r.dtype,
                schema: r
                    .schema
                    .unwrap_or_default()
                    .into_iter()
                    .map(schema_to_edit)
                    .collect(),
                example: example_to_text(r.example.as_ref()),
            }),
            responses: c
                .responses
                .into_iter()
                .map(|r: Response| EditResponse {
                    code: r.code.to_string(),
                    description: r.description,
                    dtype: r.dtype,
                    schema: r.schema.into_iter().map(schema_to_edit).collect(),
                    example: example_to_text(r.example.as_ref()),
                })
                .collect(),
        }
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
        "url": {
            "protocol": "https", "host": "api.example.com",
            "path": ["auth", "{id}"],
            "query": [{ "name": "page", "value": "1", "description": "Page", "required": false }],
            "variable": [{ "name": "id", "type": "int", "description": "User id", "required": true }]
        },
        "headers": [{ "name": "Content-Type", "value": "application/json" }],
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
        assert_eq!(m.url.protocol, "https");
        assert_eq!(m.url.path, vec!["auth", "{id}"]);
        assert_eq!(m.url.query[0].name, "page");
        assert_eq!(m.url.variable[0].dtype, "int");
        assert!(m.url.variable[0].required);
        assert_eq!(m.headers[0].name, "Content-Type");

        let req = m.request.as_ref().unwrap();
        assert_eq!(req.dtype, "object");
        assert_eq!(req.schema[0].name, "user");
        assert_eq!(req.schema[0].properties[0].name, "email");
        // example is pretty-printed raw text containing the key
        assert!(req.example.contains("\"email\""));

        assert_eq!(m.responses[0].code, "200");
        assert_eq!(m.responses[0].schema[0].name, "token");
        assert!(m.responses[0].example.contains("\"token\""));
    }
}
