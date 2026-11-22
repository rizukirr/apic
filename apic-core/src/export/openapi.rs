//! `apic export --openapi`: write apic contracts out as an OpenAPI document.
//!
//! A contract body is an example payload, so each body is exported as its
//! example plus a schema inferred from it. The inferred schema only knows
//! what the example shows: enums, formats, bounds and required lists cannot
//! be recovered.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use serde_json::{Map, Value, json};

use super::ExportItem;
use crate::json::{Header, JsonContent, Part, method_str};
use crate::vars::{Vars, tokens};

/// Which OpenAPI version to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    /// OpenAPI 3.0.3, the most widely supported by tools.
    V3_0,
    /// OpenAPI 3.1.0, aligned with JSON Schema.
    V3_1,
}

impl Version {
    fn as_str(self) -> &'static str {
        match self {
            Version::V3_0 => "3.0.3",
            Version::V3_1 => "3.1.0",
        }
    }
}

/// The document's `info` block.
pub struct Info<'a> {
    pub title: &'a str,
    pub version: &'a str,
}

/// A contract URL split into the server it targets and the path on it.
struct SplitUrl {
    /// The server URL in OpenAPI form, e.g. `https://api.example.com` or
    /// `{base}` with `base` as a server variable.
    server: String,
    /// The server variables the leading apic variables became, in order.
    server_vars: Vec<String>,
    /// The path in OpenAPI form, starting with `/`, with variables as
    /// `{name}` path params.
    path: String,
}

/// Splits a contract URL into server and path. A URL that starts with a
/// variable (`$base/users`) uses that variable as the server, along with any
/// whole path segments right after it that are variables too
/// (`$base/$prefix/users` gives the server `{base}/{prefix}`). A URL with a
/// scheme uses its origin. Anything else has no server (`/`). A query string
/// in the URL is dropped, since query params are documented separately.
fn split_url(url: &str) -> SplitUrl {
    let url = url.split(['?', '#']).next().unwrap_or(url);
    // Only a variable leads to a server. A leading `{id}` is a path param.
    let leading = tokens(url)
        .into_iter()
        .next()
        .filter(|t| t.range.start == 0 && (url.starts_with('$') || url.starts_with("{{")));
    let (server, server_vars, rest) = if let Some(t) = leading {
        let mut server = format!("{{{}}}", t.name);
        let mut vars = vec![t.name.clone()];
        let mut rest = &url[t.range.end..];
        while let Some(seg) = rest.strip_prefix('/') {
            let end = seg.find('/').unwrap_or(seg.len());
            let found = tokens(&seg[..end]);
            let whole_var = found
                .first()
                .filter(|t| t.range == (0..end) && (seg.starts_with('$') || seg.starts_with("{{")));
            let Some(t) = whole_var else { break };
            server.push_str(&format!("/{{{}}}", t.name));
            vars.push(t.name.clone());
            rest = &seg[end..];
        }
        (server, vars, rest)
    } else if let Some((scheme, after)) = url.split_once("://") {
        let end = after.find('/').unwrap_or(after.len());
        (
            format!("{scheme}://{}", &after[..end]),
            Vec::new(),
            &after[end..],
        )
    } else {
        ("/".to_string(), Vec::new(), url)
    };
    SplitUrl {
        server,
        server_vars,
        path: path_template(rest),
    }
}

/// Rewrites every variable in a path to an OpenAPI `{name}` param and makes
/// sure the path starts with `/`.
fn path_template(rest: &str) -> String {
    let mut out = String::new();
    let mut at = 0;
    for t in tokens(rest) {
        out.push_str(&rest[at..t.range.start]);
        out.push_str(&format!("{{{}}}", t.name));
        at = t.range.end;
    }
    out.push_str(&rest[at..]);
    let out = out.trim_end_matches('/');
    if out.starts_with('/') {
        out.to_string()
    } else {
        format!("/{out}")
    }
}

/// The names of the `{name}` params in an OpenAPI path, in order.
fn path_params(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = path;
    while let Some(start) = rest.find('{') {
        let Some(len) = rest[start..].find('}') else {
            break;
        };
        let name = &rest[start + 1..start + len];
        if !out.iter().any(|n| n == name) {
            out.push(name.to_string());
        }
        rest = &rest[start + len + 1..];
    }
    out
}

/// A JSON Schema inferred from an example value.
fn infer(value: &Value, version: Version) -> Value {
    match value {
        Value::Null => match version {
            Version::V3_0 => json!({ "nullable": true }),
            Version::V3_1 => json!({}),
        },
        Value::Bool(_) => json!({ "type": "boolean" }),
        Value::Number(n) if n.is_i64() || n.is_u64() => json!({ "type": "integer" }),
        Value::Number(_) => json!({ "type": "number" }),
        Value::String(_) => json!({ "type": "string" }),
        Value::Array(items) => match items.first() {
            Some(first) => json!({ "type": "array", "items": infer(first, version) }),
            None => json!({ "type": "array", "items": {} }),
        },
        Value::Object(map) => {
            let props: Map<String, Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), infer(v, version)))
                .collect();
            json!({ "type": "object", "properties": props })
        }
    }
}

/// Whether `text` holds an apic variable (`$name`, `${name}` or
/// `{{name}}`). A `{name}` outside a URL is plain text, not a variable.
fn has_var(text: &str) -> bool {
    tokens(text).iter().any(|t| {
        let raw = &text[t.range.clone()];
        raw.starts_with('$') || raw.starts_with("{{")
    })
}

/// A typed example for a query or header value written as text: `true`,
/// `false` and numbers become JSON values when they read back as the same
/// text, so `+1` or `007` stay strings. Other text, including a value
/// holding a variable such as `{{device_id}}`, stays a string. An empty value
/// has no example.
fn typed_example(text: &str) -> Option<Value> {
    if text.is_empty() {
        return None;
    }
    if has_var(text) {
        return Some(Value::String(text.to_string()));
    }
    match serde_json::from_str::<Value>(text) {
        Ok(v @ (Value::Bool(_) | Value::Number(_)))
            if serde_json::to_string(&v).ok().as_deref() == Some(text) =>
        {
            Some(v)
        }
        _ => Some(Value::String(text.to_string())),
    }
}

/// The schema and `example` entries for a text value.
fn text_schema(text: &str, version: Version) -> (Value, Option<Value>) {
    match typed_example(text) {
        Some(example) => (infer(&example, version), Some(example)),
        None => (json!({ "type": "string" }), None),
    }
}

/// A media type object for a JSON body: its inferred schema and the body as
/// the example.
fn json_media(body: &Value, version: Version) -> Value {
    json!({ "schema": infer(body, version), "example": body })
}

/// A `multipart/form-data` media type object for multipart parts.
fn multipart_media(parts: &[Part]) -> Value {
    let mut props = Map::new();
    let mut required = Vec::new();
    let mut encoding = Map::new();
    for p in parts {
        let mut prop = if p.filename.is_some() {
            json!({ "type": "string", "format": "binary" })
        } else {
            let mut s = json!({ "type": "string" });
            if !p.value.is_empty() {
                s["example"] = json!(p.value);
            }
            s
        };
        if let Some(d) = &p.description {
            prop["description"] = json!(d);
        }
        props.insert(p.name.clone(), prop);
        if p.required {
            required.push(json!(p.name));
        }
        if let Some(ct) = &p.content_type {
            encoding.insert(p.name.clone(), json!({ "contentType": ct }));
        }
    }
    let mut schema = json!({ "type": "object", "properties": props });
    if !required.is_empty() {
        schema["required"] = Value::Array(required);
    }
    let mut media = json!({ "schema": schema });
    if !encoding.is_empty() {
        media["encoding"] = Value::Object(encoding);
    }
    media
}

/// The security scheme an `Authorization` header value implies, as the
/// scheme's name and definition.
fn auth_scheme(value: &str) -> (&'static str, Value) {
    let lower = value.trim_start().to_ascii_lowercase();
    if lower.starts_with("bearer") {
        ("bearerAuth", json!({ "type": "http", "scheme": "bearer" }))
    } else if lower.starts_with("basic") {
        ("basicAuth", json!({ "type": "http", "scheme": "basic" }))
    } else {
        (
            "authorizationHeader",
            json!({ "type": "apiKey", "in": "header", "name": "Authorization" }),
        )
    }
}

/// Builds the document while collecting what it needs at the top level.
struct Builder {
    version: Version,
    security_schemes: BTreeMap<&'static str, Value>,
    warnings: Vec<String>,
}

impl Builder {
    /// The operation object for one contract.
    fn operation(
        &mut self,
        contract: &JsonContent,
        path: &str,
        op_id: String,
        tag: Option<&str>,
    ) -> Value {
        let mut op = Map::new();
        if let Some(tag) = tag {
            op.insert("tags".into(), json!([tag]));
        }
        op.insert("summary".into(), json!(contract.name));
        if let Some(d) = contract.description.as_ref().filter(|d| !d.is_empty()) {
            op.insert("description".into(), json!(d));
        }
        op.insert("operationId".into(), json!(op_id));

        let mut params = Vec::new();
        for name in path_params(path) {
            params.push(json!({
                "name": name, "in": "path", "required": true, "schema": { "type": "string" }
            }));
        }
        for q in &contract.query {
            let (schema, example) = text_schema(&q.value, self.version);
            let mut p = json!({ "name": q.name, "in": "query", "schema": schema });
            if q.required {
                p["required"] = json!(true);
            }
            if let Some(d) = q.description.as_ref().filter(|d| !d.is_empty()) {
                p["description"] = json!(d);
            }
            if let Some(e) = example {
                p["example"] = e;
            }
            params.push(p);
        }

        let mut content_type = None;
        let mut security = Vec::new();
        for h in &contract.headers {
            match h.name.to_ascii_lowercase().as_str() {
                // OpenAPI describes these through the body and security
                // schemes, never as header parameters.
                "content-type" => content_type = Some(h.value.clone()),
                "accept" => {}
                "authorization" => {
                    let (name, scheme) = auth_scheme(&h.value);
                    self.security_schemes.insert(name, scheme);
                    security.push(json!({ name: [] }));
                }
                _ => params.push(self.header_param(h)),
            }
        }
        if !params.is_empty() {
            op.insert("parameters".into(), Value::Array(params));
        }

        if let Some(parts) = contract.multipart.as_ref().filter(|p| !p.is_empty()) {
            op.insert(
                "requestBody".into(),
                json!({ "required": true, "content": { "multipart/form-data": multipart_media(parts) } }),
            );
        } else if let Some(body) = &contract.request {
            let media = content_type
                .filter(|ct| !ct.is_empty() && !has_var(ct))
                .unwrap_or_else(|| "application/json".to_string());
            op.insert(
                "requestBody".into(),
                json!({ "required": true, "content": { media: json_media(body, self.version) } }),
            );
        }

        let mut responses = Map::new();
        for r in &contract.responses {
            let mut resp = json!({
                "description": if r.description.is_empty() { "Response" } else { r.description.as_str() },
            });
            if !r.headers.is_empty() {
                let headers: Map<String, Value> = r
                    .headers
                    .iter()
                    .map(|h| {
                        let (schema, example) = text_schema(&h.value, self.version);
                        let mut obj = json!({ "schema": schema });
                        if h.required {
                            obj["required"] = json!(true);
                        }
                        if let Some(e) = example {
                            obj["example"] = e;
                        }
                        (h.name.clone(), obj)
                    })
                    .collect();
                resp["headers"] = Value::Object(headers);
            }
            if let Some(parts) = r.multipart.as_ref().filter(|p| !p.is_empty()) {
                resp["content"] = json!({ "multipart/form-data": multipart_media(parts) });
            } else if let Some(body) = &r.schema {
                resp["content"] = json!({ "application/json": json_media(body, self.version) });
            }
            if !(100..=599).contains(&r.code) {
                self.warnings.push(format!(
                    "{:?} documents status {}, which is not an HTTP status, so it is left out",
                    contract.name, r.code
                ));
                continue;
            }
            let code = r.code.to_string();
            match responses.get_mut(&code) {
                Some(first) => {
                    let merged =
                        merge_response(first, &r.description, r.schema.as_ref(), self.version);
                    if !merged {
                        self.warnings.push(format!(
                            "{:?} documents status {code} more than once with a multipart body, only the first is exported",
                            contract.name
                        ));
                    }
                }
                None => {
                    responses.insert(code, resp);
                }
            }
        }
        if responses.is_empty() {
            // OpenAPI requires at least one response.
            responses.insert("default".into(), json!({ "description": "Not documented" }));
        }
        op.insert("responses".into(), Value::Object(responses));
        if !security.is_empty() {
            op.insert("security".into(), Value::Array(security));
        }
        Value::Object(op)
    }

    fn header_param(&self, h: &Header) -> Value {
        let (schema, example) = text_schema(&h.value, self.version);
        let mut p = json!({ "name": h.name, "in": "header", "schema": schema });
        if h.required {
            p["required"] = json!(true);
        }
        if let Some(e) = example {
            p["example"] = e;
        }
        p
    }
}

/// Folds a second response for the same status into `first`. OpenAPI has
/// one response per status, so the JSON bodies become named `examples`, each
/// with its response's description as the summary, and distinct descriptions
/// are joined with ` / `.
///
/// # Returns
///
/// `false` when the first response has a multipart body, which cannot hold
/// several examples, so nothing was merged.
fn merge_response(
    first: &mut Value,
    description: &str,
    body: Option<&Value>,
    version: Version,
) -> bool {
    if first.pointer("/content/multipart~1form-data").is_some() {
        return false;
    }
    let desc = first["description"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let summary = if description.is_empty() {
        "Response"
    } else {
        description
    };
    if !desc.split(" / ").any(|d| d == summary) {
        first["description"] = json!(format!("{desc} / {summary}"));
    }
    let Some(body) = body else {
        return true;
    };
    let first_summary = desc.split(" / ").next().unwrap_or("Response").to_string();
    let Some(resp) = first.as_object_mut() else {
        return true;
    };
    let content = resp.entry("content").or_insert_with(|| json!({}));
    let media = &mut content["application/json"];
    if media.is_null() {
        *media = json!({ "schema": infer(body, version) });
    }
    let mut examples = match media.as_object_mut().and_then(|m| m.remove("example")) {
        Some(value) => {
            let mut m = Map::new();
            let key = example_key(&first_summary, &m);
            m.insert(key, json!({ "summary": first_summary, "value": value }));
            m
        }
        None => media
            .get("examples")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default(),
    };
    let key = example_key(summary, &examples);
    examples.insert(key, json!({ "summary": summary, "value": body }));
    media["examples"] = Value::Object(examples);
    true
}

/// A key for a named example, from its summary, unique within `taken`.
fn example_key(summary: &str, taken: &Map<String, Value>) -> String {
    let base = crate::convert::slugify(summary);
    let mut key = base.clone();
    let mut n = 2;
    while taken.contains_key(&key) {
        key = format!("{base}_{n}");
        n += 1;
    }
    key
}

/// The tag for a contract: its folder path, or `None` at the root.
fn tag_of(rel_path: &Path) -> Option<String> {
    let dir = rel_path.parent()?;
    let parts: Vec<String> = dir
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// A unique operationId from the file stem, prefixed with its folders when
/// the stem alone is taken.
fn operation_id(rel_path: &Path, taken: &mut HashSet<String>) -> String {
    let stem = rel_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "operation".to_string());
    let mut id = stem.clone();
    if !taken.insert(id.clone()) {
        let prefixed = tag_of(rel_path)
            .map(|t| format!("{}_{stem}", t.replace('/', "_")))
            .unwrap_or_else(|| stem.clone());
        id = prefixed.clone();
        let mut n = 2;
        while !taken.insert(id.clone()) {
            id = format!("{prefixed}_{n}");
            n += 1;
        }
    }
    id
}

/// Builds an OpenAPI document from `items`.
///
/// # Arguments
///
/// * `info` - the title and version for `info`
/// * `items` - the contracts, in the order they should appear
/// * `values` - values for server variables, taken from a shared env file.
///   A variable with no value here gets an empty default.
/// * `version` - which OpenAPI version to write
///
/// # Returns
///
/// The document and warnings for contracts that could not be exported as
/// they are, such as a second contract for a method and path already taken.
pub fn document(
    info: Info<'_>,
    items: &[ExportItem],
    values: &Vars,
    version: Version,
) -> (Value, Vec<String>) {
    let mut builder = Builder {
        version,
        security_schemes: BTreeMap::new(),
        warnings: Vec::new(),
    };

    // The server most contracts use goes at the top level; the others are
    // set on each operation that targets them.
    let splits: Vec<SplitUrl> = items.iter().map(|i| split_url(&i.contract.url)).collect();
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for s in &splits {
        match counts.iter_mut().find(|(server, _)| *server == s.server) {
            Some((_, n)) => *n += 1,
            None => counts.push((&s.server, 1)),
        }
    }
    let main_server = counts
        .iter()
        .max_by_key(|(_, n)| *n)
        .map(|(s, _)| s.to_string())
        .unwrap_or_else(|| "/".to_string());
    let server_object = |s: &SplitUrl| {
        let mut obj = json!({ "url": s.server });
        if !s.server_vars.is_empty() {
            let vars: Map<String, Value> = s
                .server_vars
                .iter()
                .map(|v| {
                    let default = values.get(v).cloned().unwrap_or_default();
                    (v.clone(), json!({ "default": default }))
                })
                .collect();
            obj["variables"] = Value::Object(vars);
        }
        obj
    };

    let mut paths: Map<String, Value> = Map::new();
    let mut tags: Vec<String> = Vec::new();
    let mut op_ids = HashSet::new();
    let mut seen: BTreeMap<(String, String), String> = BTreeMap::new();
    let mut main_server_object = json!({ "url": main_server });
    for (item, split) in items.iter().zip(&splits) {
        let method = method_str(&item.contract.method).to_lowercase();
        let shown = item.rel_path.to_string_lossy().replace('\\', "/");
        let key = (split.path.clone(), method.clone());
        if let Some(first) = seen.get(&key) {
            builder.warnings.push(format!(
                "skipped {shown}: {} {} is already exported from {first}",
                method.to_uppercase(),
                split.path
            ));
            continue;
        }
        seen.insert(key, shown);

        let tag = tag_of(&item.rel_path);
        if let Some(t) = &tag
            && !tags.contains(t)
        {
            tags.push(t.clone());
        }
        let op_id = operation_id(&item.rel_path, &mut op_ids);
        let mut op = builder.operation(&item.contract, &split.path, op_id, tag.as_deref());
        if split.server == main_server {
            main_server_object = server_object(split);
        } else {
            op["servers"] = json!([server_object(split)]);
        }
        let entry = paths
            .entry(split.path.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        entry[method.as_str()] = op;
    }

    let mut doc = json!({
        "openapi": version.as_str(),
        "info": { "title": info.title, "version": info.version },
        "servers": [main_server_object],
    });
    if !tags.is_empty() {
        doc["tags"] = Value::Array(tags.iter().map(|t| json!({ "name": t })).collect());
    }
    doc["paths"] = Value::Object(paths);
    if !builder.security_schemes.is_empty() {
        doc["components"] = json!({ "securitySchemes": builder.security_schemes });
    }
    (doc, builder.warnings)
}

/// The document as text: YAML when `yaml` is set, pretty JSON otherwise.
///
/// # Errors
///
/// Returns the serializer's error text.
pub fn to_text(doc: &Value, yaml: bool) -> Result<String, String> {
    if yaml {
        serde_norway::to_string(doc).map_err(|err| format!("failed to write YAML: {err}"))
    } else {
        serde_json::to_string_pretty(doc).map_err(|err| format!("failed to write JSON: {err}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn items(list: Vec<(&str, Value)>) -> Vec<ExportItem> {
        list.into_iter()
            .map(|(p, c)| ExportItem {
                rel_path: PathBuf::from(p),
                contract: serde_json::from_value(c).expect("contract parses"),
            })
            .collect()
    }

    fn build(list: Vec<(&str, Value)>, version: Version) -> (Value, Vec<String>) {
        document(
            Info {
                title: "Demo",
                version: "1.0.0",
            },
            &items(list),
            &Vars::new(),
            version,
        )
    }

    #[test]
    fn splits_urls_into_server_and_path() {
        let s = split_url("$base/users/{id}?x=1");
        assert_eq!(
            (s.server.as_str(), s.path.as_str()),
            ("{base}", "/users/{id}")
        );
        assert_eq!(s.server_vars, ["base"]);
        let s = split_url("{{baseUrl}}/{{base_path}}/auth/$x/{id}");
        assert_eq!(s.server, "{baseUrl}/{base_path}");
        assert_eq!(s.server_vars, ["baseUrl", "base_path"]);
        assert_eq!(s.path, "/auth/{x}/{id}");
        let s = split_url("{{host}}/v1/items/$item/");
        assert_eq!(
            (s.server.as_str(), s.path.as_str()),
            ("{host}", "/v1/items/{item}")
        );
        let s = split_url("https://api.example.com:8080/v1/me");
        assert_eq!(s.server, "https://api.example.com:8080");
        assert_eq!(s.path, "/v1/me");
        let s = split_url("https://api.example.com");
        assert_eq!(s.path, "/");
        // A leading `{id}` path param is not a server.
        let s = split_url("{id}/x");
        assert_eq!((s.server.as_str(), s.path.as_str()), ("/", "/{id}/x"));
    }

    #[test]
    fn text_examples_keep_their_exact_text() {
        assert_eq!(typed_example("20"), Some(json!(20)));
        assert_eq!(typed_example("2.5"), Some(json!(2.5)));
        assert_eq!(typed_example("false"), Some(json!(false)));
        assert_eq!(typed_example("+1"), Some(json!("+1")));
        assert_eq!(typed_example("007"), Some(json!("007")));
        assert_eq!(typed_example("Bearer $token"), Some(json!("Bearer $token")));
        assert_eq!(typed_example("$count"), Some(json!("$count")));
        assert_eq!(typed_example(""), None);
        // `{owner}` in a header value is text, not a variable.
        assert_eq!(
            typed_example("<https://h/{owner}>"),
            Some(json!("<https://h/{owner}>"))
        );
    }

    #[test]
    fn infers_schemas_from_examples() {
        let schema = infer(
            &json!({"id": 1, "price": 2.5, "tags": ["a"], "ok": true, "gone": null}),
            Version::V3_0,
        );
        assert_eq!(
            schema,
            json!({"type": "object", "properties": {
                "id": {"type": "integer"}, "price": {"type": "number"},
                "tags": {"type": "array", "items": {"type": "string"}},
                "ok": {"type": "boolean"}, "gone": {"nullable": true}
            }})
        );
        assert_eq!(infer(&Value::Null, Version::V3_1), json!({}));
    }

    #[test]
    fn maps_a_contract_to_an_operation() {
        let (doc, warnings) = build(
            vec![(
                "users/update_user.json",
                json!({
                    "name": "Update user", "description": "Change a user", "method": "PUT",
                    "url": "https://api.example.com/users/{id}",
                    "query": [{"name": "notify", "value": "true", "description": "Send mail", "required": true}],
                    "headers": [
                        {"name": "Authorization", "value": "Bearer $token", "required": true},
                        {"name": "Content-Type", "value": "application/merge-patch+json"},
                        {"name": "X-Trace", "value": "$trace"}
                    ],
                    "request": {"name": "Rex"},
                    "responses": [{"code": 200, "description": "OK",
                        "headers": [{"name": "X-Rate", "value": "60"}], "schema": {"id": 1}}]
                }),
            )],
            Version::V3_0,
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(doc["openapi"], "3.0.3");
        assert_eq!(doc["servers"][0]["url"], "https://api.example.com");
        assert_eq!(doc["tags"][0]["name"], "users");
        let op = &doc["paths"]["/users/{id}"]["put"];
        assert_eq!(op["operationId"], "update_user");
        assert_eq!(op["summary"], "Update user");
        assert_eq!(op["tags"], json!(["users"]));
        let params = op["parameters"].as_array().unwrap();
        assert_eq!(
            params[0],
            json!({"name": "id", "in": "path", "required": true, "schema": {"type": "string"}})
        );
        assert_eq!(params[1]["schema"], json!({"type": "boolean"}));
        assert_eq!(params[1]["example"], json!(true));
        assert_eq!(params[1]["required"], json!(true));
        // A header holding a variable keeps it as a string example;
        // Authorization and Content-Type are never header params.
        assert_eq!(
            params[2],
            json!({"name": "X-Trace", "in": "header", "schema": {"type": "string"}, "example": "$trace"})
        );
        assert_eq!(params.len(), 3);
        assert_eq!(op["security"], json!([{"bearerAuth": []}]));
        assert_eq!(
            doc["components"]["securitySchemes"]["bearerAuth"]["scheme"],
            "bearer"
        );
        let body = &op["requestBody"]["content"]["application/merge-patch+json"];
        assert_eq!(body["example"], json!({"name": "Rex"}));
        let resp = &op["responses"]["200"];
        assert_eq!(resp["headers"]["X-Rate"]["example"], json!(60));
        assert_eq!(
            resp["content"]["application/json"]["schema"]["properties"]["id"]["type"],
            "integer"
        );
    }

    #[test]
    fn variable_server_duplicates_and_other_servers() {
        let mut values = Vars::new();
        values.insert("base".into(), "https://api.example.com".into());
        let (doc, warnings) = document(
            Info {
                title: "Demo",
                version: "1.0.0",
            },
            &items(vec![
                (
                    "a.json",
                    json!({"name": "A", "method": "GET", "url": "$base/a", "headers": [], "responses": []}),
                ),
                (
                    "b.json",
                    json!({"name": "B", "method": "GET", "url": "$base/b", "headers": [], "responses": []}),
                ),
                (
                    "dup.json",
                    json!({"name": "Dup", "method": "GET", "url": "$base/a", "headers": [], "responses": []}),
                ),
                (
                    "x/a.json",
                    json!({"name": "Other", "method": "POST", "url": "https://other.example.com/o", "headers": [], "responses": []}),
                ),
            ]),
            &values,
            Version::V3_1,
        );
        assert_eq!(doc["openapi"], "3.1.0");
        assert_eq!(
            doc["servers"],
            json!([{"url": "{base}", "variables": {"base": {"default": "https://api.example.com"}}}])
        );
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].contains("dup.json") && warnings[0].contains("a.json"),
            "{warnings:?}"
        );
        let other = &doc["paths"]["/o"]["post"];
        assert_eq!(other["servers"][0]["url"], "https://other.example.com");
        // The stem `a` is taken, so the folder prefixes it.
        assert_eq!(other["operationId"], "x_a");
        assert_eq!(
            doc["paths"]["/a"]["get"]["responses"]["default"]["description"],
            "Not documented"
        );
    }

    #[test]
    fn repeated_statuses_become_named_examples() {
        let (doc, warnings) = build(
            vec![(
                "login.json",
                json!({"name": "Login", "method": "POST", "url": "https://h/login", "headers": [],
                "responses": [
                    {"code": 400, "description": "Bad Request", "schema": {"error": "missing email"}},
                    {"code": 400, "description": "Wrong password", "schema": {"error": "wrong password"}},
                    {"code": 400, "description": "Bad Request", "schema": {"error": "locked"}}
                ]}),
            )],
            Version::V3_0,
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        let resp = &doc["paths"]["/login"]["post"]["responses"]["400"];
        assert_eq!(resp["description"], "Bad Request / Wrong password");
        let media = &resp["content"]["application/json"];
        assert!(media.get("example").is_none());
        assert_eq!(
            media["examples"],
            json!({
                "bad_request": {"summary": "Bad Request", "value": {"error": "missing email"}},
                "wrong_password": {"summary": "Wrong password", "value": {"error": "wrong password"}},
                "bad_request_2": {"summary": "Bad Request", "value": {"error": "locked"}}
            })
        );

        // The importer turns each named example back into its own response.
        let (mapped, _) = crate::openapi::map(&doc).expect("export maps");
        let back: Vec<(u16, &str, Value)> = mapped[0]
            .contract
            .responses
            .iter()
            .map(|r| (r.code, r.description.as_str(), r.schema.clone().unwrap()))
            .collect();
        assert_eq!(
            back,
            vec![
                (400, "Bad Request", json!({"error": "missing email"})),
                (400, "Wrong password", json!({"error": "wrong password"})),
                (400, "Bad Request", json!({"error": "locked"})),
            ]
        );
    }

    #[test]
    fn a_status_outside_http_range_is_left_out_with_a_warning() {
        let (doc, warnings) = build(
            vec![(
                "x.json",
                json!({"name": "X", "method": "GET", "url": "https://h/x", "headers": [],
                    "responses": [{"code": 0, "description": "0"}, {"code": 204, "description": "No Content"}]}),
            )],
            Version::V3_0,
        );
        let responses = doc["paths"]["/x"]["get"]["responses"].as_object().unwrap();
        assert_eq!(responses.keys().collect::<Vec<_>>(), ["204"]);
        assert!(warnings[0].contains("status 0"), "{warnings:?}");
    }

    #[test]
    fn multipart_body_becomes_form_data() {
        let (doc, _) = build(
            vec![(
                "upload.json",
                json!({"name": "Upload", "method": "POST", "url": "https://h/upload", "headers": [],
                    "multipart": [
                        {"name": "title", "value": "Hi", "required": true},
                        {"name": "file", "filename": "a.png", "contentType": "image/png"}
                    ],
                    "responses": [{"code": 201, "description": "Created"}]}),
            )],
            Version::V3_0,
        );
        let media =
            &doc["paths"]["/upload"]["post"]["requestBody"]["content"]["multipart/form-data"];
        assert_eq!(
            media["schema"]["properties"]["file"],
            json!({"type": "string", "format": "binary"})
        );
        assert_eq!(media["schema"]["properties"]["title"]["example"], "Hi");
        assert_eq!(media["schema"]["required"], json!(["title"]));
        assert_eq!(media["encoding"]["file"]["contentType"], "image/png");
    }

    #[test]
    fn export_reimports_through_the_openapi_importer() {
        let (doc, _) = build(
            vec![(
                "pets/get_pet.json",
                json!({"name": "Get pet", "method": "GET", "url": "https://h/pets/{id}",
                    "query": [{"name": "full", "value": "true"}], "headers": [],
                    "responses": [{"code": 200, "description": "OK", "schema": {"id": 1, "name": "Rex"}}]}),
            )],
            Version::V3_0,
        );
        let (mapped, warnings) = crate::openapi::map(&doc).expect("export maps");
        assert!(warnings.is_empty(), "{warnings:?}");
        let m = &mapped[0];
        assert_eq!(m.rel_path, Path::new("pets").join("get_pet.json"));
        assert_eq!(m.contract.url, "https://h/pets/{id}");
        assert_eq!(m.contract.query[0].value, "true");
        assert_eq!(
            m.contract.responses[0].schema,
            Some(json!({"id": 1, "name": "Rex"}))
        );
    }
}
