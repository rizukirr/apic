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
    pub url: EditUrl,
    pub headers: Vec<EditHeader>,
    pub request: Option<EditBody>,
    pub responses: Vec<EditResponse>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditUrl {
    pub protocol: String,
    pub host: String,
    pub path: Vec<String>,
    pub query: Vec<EditQuery>,
    pub variable: Vec<EditVariable>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditQuery {
    pub name: String,
    pub dtype: String,       // data type, e.g. "string", "int"
    pub description: String, // empty => None
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditVariable {
    pub name: String,
    pub dtype: String,       // defaults to "string" on save when empty
    pub description: String, // empty => None
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditBody {
    pub dtype: String, // e.g. "object", "object[]"
    pub schema: Vec<EditSchema>,
    pub example: String, // raw JSON text; empty => None
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditResponse {
    pub code: String, // numeric text; parsed to u16 on save
    pub description: String,
    pub dtype: String,
    pub schema: Vec<EditSchema>,
    pub example: String, // raw JSON text; empty => None
}

/// A schema field. `properties` nests recursively for object types.
#[derive(Debug, Clone, PartialEq)]
pub struct EditSchema {
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
    /// A new response shell defaulting to `200`, the most common code (editable).
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
                        dtype: q.dtype,
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

fn edit_schema_to_value(s: &EditSchema) -> Value {
    let mut map = serde_json::Map::new();
    map.insert("name".into(), Value::String(s.name.clone()));
    map.insert("type".into(), Value::String(s.dtype.clone()));
    map.insert(
        "default".into(),
        match str_opt(&s.default) {
            Some(d) => Value::String(d.to_string()),
            None => Value::Null,
        },
    );
    map.insert("description".into(), Value::String(s.description.clone()));
    map.insert("required".into(), Value::Bool(s.required));
    if !s.properties.is_empty() {
        map.insert(
            "properties".into(),
            Value::Array(s.properties.iter().map(edit_schema_to_value).collect()),
        );
    }
    if let Some(a) = str_opt(&s.accept) {
        map.insert("accept".into(), Value::String(a.to_string()));
    }
    Value::Object(map)
}

impl EditModel {
    /// Serializes the model to a pretty, schema-valid contract string.
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

        // url
        let mut url = serde_json::Map::new();
        url.insert("protocol".into(), Value::String(self.url.protocol.clone()));
        url.insert("host".into(), Value::String(self.url.host.clone()));
        if !self.url.path.is_empty() {
            url.insert(
                "path".into(),
                Value::Array(self.url.path.iter().cloned().map(Value::String).collect()),
            );
        }
        if !self.url.query.is_empty() {
            url.insert(
                "query".into(),
                Value::Array(
                    self.url
                        .query
                        .iter()
                        .map(|q| {
                            let mut m = serde_json::Map::new();
                            m.insert("name".into(), Value::String(q.name.clone()));
                            let dtype = if q.dtype.trim().is_empty() {
                                "string"
                            } else {
                                q.dtype.as_str()
                            };
                            m.insert("type".into(), Value::String(dtype.to_string()));
                            if let Some(d) = str_opt(&q.description) {
                                m.insert("description".into(), Value::String(d.to_string()));
                            }
                            m.insert("required".into(), Value::Bool(q.required));
                            Value::Object(m)
                        })
                        .collect(),
                ),
            );
        }
        if !self.url.variable.is_empty() {
            url.insert(
                "variable".into(),
                Value::Array(
                    self.url
                        .variable
                        .iter()
                        .map(|v| {
                            let mut m = serde_json::Map::new();
                            m.insert("name".into(), Value::String(v.name.clone()));
                            let dtype = if v.dtype.trim().is_empty() {
                                "string"
                            } else {
                                v.dtype.as_str()
                            };
                            m.insert("type".into(), Value::String(dtype.to_string()));
                            if let Some(d) = str_opt(&v.description) {
                                m.insert("description".into(), Value::String(d.to_string()));
                            }
                            m.insert("required".into(), Value::Bool(v.required));
                            Value::Object(m)
                        })
                        .collect(),
                ),
            );
        }
        root.insert("url".into(), Value::Object(url));

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
                        Value::Object(m)
                    })
                    .collect(),
            ),
        );

        // request (optional)
        if let Some(req) = &self.request {
            let mut m = serde_json::Map::new();
            m.insert("type".into(), Value::String(req.dtype.clone()));
            if !req.schema.is_empty() {
                m.insert(
                    "schema".into(),
                    Value::Array(req.schema.iter().map(edit_schema_to_value).collect()),
                );
            }
            if let Some(ex) = parse_example(&req.example, "request")? {
                m.insert("example".into(), ex);
            }
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
            m.insert("type".into(), Value::String(r.dtype.clone()));
            if !r.schema.is_empty() {
                m.insert(
                    "schema".into(),
                    Value::Array(r.schema.iter().map(edit_schema_to_value).collect()),
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

/// Walks `fields` following `path`, returning the addressed node.
fn schema_node_mut<'a>(fields: &'a mut [EditSchema], path: &[usize]) -> Option<&'a mut EditSchema> {
    let (&first, rest) = path.split_first()?;
    let node = fields.get_mut(first)?;
    if rest.is_empty() {
        Some(node)
    } else {
        schema_node_mut(&mut node.properties, rest)
    }
}

impl EditModel {
    /// Mutable access to a request-body schema node by path ([] is invalid;
    /// use the top-level vec directly for inserts).
    pub fn schema_at_mut_request(&mut self, path: &[usize]) -> Option<&mut EditSchema> {
        let req = self.request.as_mut()?;
        schema_node_mut(&mut req.schema, path)
    }

    /// Mutable access to a response schema node by path.
    pub fn schema_at_mut_response(
        &mut self,
        resp: usize,
        path: &[usize],
    ) -> Option<&mut EditSchema> {
        let r = self.responses.get_mut(resp)?;
        schema_node_mut(&mut r.schema, path)
    }

    /// The vector that should receive a new child for `path` ([] = top-level).
    /// Returns `None` if the path does not resolve.
    pub fn schema_children_mut_request(&mut self, path: &[usize]) -> Option<&mut Vec<EditSchema>> {
        let req = self.request.as_mut()?;
        if path.is_empty() {
            return Some(&mut req.schema);
        }
        schema_node_mut(&mut req.schema, path).map(|n| &mut n.properties)
    }

    /// Response counterpart of [`schema_children_mut_request`].
    pub fn schema_children_mut_response(
        &mut self,
        resp: usize,
        path: &[usize],
    ) -> Option<&mut Vec<EditSchema>> {
        let r = self.responses.get_mut(resp)?;
        if path.is_empty() {
            return Some(&mut r.schema);
        }
        schema_node_mut(&mut r.schema, path).map(|n| &mut n.properties)
    }
}

/// A blank schema field for inserts.
impl EditSchema {
    pub fn blank() -> Self {
        EditSchema {
            name: String::new(),
            dtype: "string".to_string(),
            default: String::new(),
            description: String::new(),
            required: false,
            properties: Vec::new(),
            accept: String::new(),
        }
    }
}

/// Builds an example JSON object from schema fields:
/// string/file → "{name}", int-like → 0, float-like → 0.0, bool → false,
/// object with properties → nested object, empty object → {} if required else null,
/// arrays → a one-element array of the base type.
pub fn example_from_schema(fields: &[EditSchema]) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for f in fields {
        map.insert(f.name.clone(), gen_field_value(f));
    }
    serde_json::Value::Object(map)
}

fn gen_field_value(f: &EditSchema) -> serde_json::Value {
    use serde_json::Value;
    let (base, is_array) = crate::json::parse_type(&f.dtype);
    let v = match base {
        "string" | "file" => Value::String(format!("{{{}}}", f.name)),
        "int" | "integer" | "number" | "long" | "short" => Value::Number(0.into()),
        "float" | "double" | "decimal" => serde_json::json!(0.0),
        "bool" | "boolean" => Value::Bool(false),
        "object" => {
            if !f.properties.is_empty() {
                example_from_schema(&f.properties)
            } else if f.required {
                Value::Object(serde_json::Map::new())
            } else {
                Value::Null
            }
        }
        _ => Value::Null,
    };
    if is_array { Value::Array(vec![v]) } else { v }
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
            "query": [{ "name": "page", "type": "1", "description": "Page", "required": false }],
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

    #[test]
    fn roundtrip_preserves_contract() {
        let contract = json_get(FULL, None).unwrap();
        let model = EditModel::from_contract(contract);
        let json = model.to_json().expect("valid model serializes");
        // Re-parse: the produced JSON must be a valid contract with the same shape.
        let back = json_get(&json, None).unwrap();
        assert_eq!(back.name, "login");
        assert_eq!(back.url.variable.unwrap()[0].dtype, "int");
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
    fn empty_example_becomes_absent() {
        let contract = json_get(FULL, None).unwrap();
        let mut model = EditModel::from_contract(contract);
        model.request.as_mut().unwrap().example = String::new();
        let json = model.to_json().unwrap();
        let back = json_get(&json, None).unwrap();
        assert!(back.request.unwrap().example.is_none());
    }

    #[test]
    fn schema_at_mut_reaches_nested() {
        let c = json_get(
            r#"{ "name":"t","method":"GET",
                 "url":{"protocol":"h","host":"h","path":["x"]},"headers":[],
                 "request":{"type":"object","schema":[
                   {"name":"wrap","type":"object","default":null,"description":"d","required":true,
                    "properties":[{"name":"leaf","type":"string","default":null,"description":"d","required":false}]}
                 ]},
                 "responses":[] }"#,
            None,
        )
        .unwrap();
        let mut m = EditModel::from_contract(c);
        let node = m.schema_at_mut_request(&[0, 0]).unwrap();
        assert_eq!(node.name, "leaf");
        node.name = "renamed".to_string();
        assert_eq!(m.request.unwrap().schema[0].properties[0].name, "renamed");
    }

    #[test]
    fn example_from_schema_generates_typed_placeholders() {
        use serde_json::json;
        let c = json_get(
            r#"{ "name":"t","method":"POST",
                 "url":{"protocol":"h","host":"h","path":["x"]},"headers":[],
                 "request":{"type":"object","schema":[
                    {"name":"status","type":"int","default":null,"description":"d","required":true},
                    {"name":"message","type":"string","default":null,"description":"d","required":true},
                    {"name":"data","type":"object","default":null,"description":"d","required":false}
                 ]},
                 "responses":[] }"#,
            None,
        ).unwrap();
        let m = EditModel::from_contract(c);
        let schema = &m.request.as_ref().unwrap().schema;
        let v = example_from_schema(schema);
        assert_eq!(v["status"], json!(0));
        assert_eq!(v["message"], json!("{message}"));
        assert_eq!(v["data"], serde_json::Value::Null); // object, not required -> null
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
