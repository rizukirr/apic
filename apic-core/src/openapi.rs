//! OpenAPI import: reads an OpenAPI 3.0 / 3.1 or Swagger 2.0 spec (JSON or
//! YAML) and maps each operation to an apic contract.
//!
//! A contract body is an example payload, not a JSON Schema, so each body is
//! taken from the spec's `example` / `examples` when present and otherwise
//! generated from its schema, following local `$ref`s. Writing the files is
//! shared with the Postman import in [`crate::convert`].

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::convert::{MappedContract, method_warning, slugify, unique_slug};
use crate::json::{Header, JsonContent, Part, Query, Response, method_from_str};

/// How deep the example generator follows nested schemas before giving up,
/// so a pathological spec cannot recurse without end.
const MAX_DEPTH: usize = 12;

/// The HTTP methods an OpenAPI path item can hold, in the order they are
/// imported.
const METHODS: [&str; 8] = [
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

/// Which spec family a document is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Version {
    /// Swagger 2.0.
    Swagger2,
    /// OpenAPI 3.0 or 3.1.
    OpenApi3,
}

/// Reads the spec at `path` as JSON or YAML.
///
/// # Errors
///
/// Returns the read or parse error text.
pub(crate) fn from_path(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|err| format!("failed to open {}: {err}", path.display()))?;
    from_str(&text)
}

/// Parses spec text. JSON is tried first, then YAML (a superset of JSON).
///
/// # Errors
///
/// Returns the YAML parse error when the text is neither.
pub(crate) fn from_str(text: &str) -> Result<Value, String> {
    if let Ok(value) = serde_json::from_str::<Value>(text) {
        return Ok(value);
    }
    let yaml: serde_norway::Value =
        serde_norway::from_str(text).map_err(|err| format!("invalid OpenAPI spec: {err}"))?;
    Ok(yaml_to_json(yaml))
}

/// Converts a YAML value to JSON. Non-string mapping keys (such as an
/// unquoted `200` response code) become their text form.
fn yaml_to_json(yaml: serde_norway::Value) -> Value {
    use serde_norway::Value as Y;
    match yaml {
        Y::Null => Value::Null,
        Y::Bool(b) => Value::Bool(b),
        Y::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::from(i)
            } else if let Some(u) = n.as_u64() {
                Value::from(u)
            } else {
                n.as_f64()
                    .and_then(serde_json::Number::from_f64)
                    .map_or(Value::Null, Value::Number)
            }
        }
        Y::String(s) => Value::String(s),
        Y::Sequence(seq) => Value::Array(seq.into_iter().map(yaml_to_json).collect()),
        Y::Mapping(map) => Value::Object(
            map.into_iter()
                .filter_map(|(k, v)| {
                    let key = match k {
                        Y::String(s) => s,
                        Y::Number(n) => n.to_string(),
                        Y::Bool(b) => b.to_string(),
                        _ => return None,
                    };
                    Some((key, yaml_to_json(v)))
                })
                .collect(),
        ),
        Y::Tagged(tagged) => yaml_to_json(tagged.value),
    }
}

/// Which spec family `spec` is.
///
/// # Errors
///
/// Returns an error when it is neither OpenAPI 3.x nor Swagger 2.0.
fn detect(spec: &Value) -> Result<Version, String> {
    if let Some(v) = spec.get("openapi").and_then(Value::as_str) {
        return if v.starts_with("3.") {
            Ok(Version::OpenApi3)
        } else {
            Err(format!(
                "unsupported OpenAPI version {v:?}, expected 3.0 or 3.1"
            ))
        };
    }
    if let Some(v) = spec.get("swagger").and_then(Value::as_str) {
        return if v.starts_with("2.") {
            Ok(Version::Swagger2)
        } else {
            Err(format!("unsupported Swagger version {v:?}, expected 2.0"))
        };
    }
    Err("not an OpenAPI 3.x or Swagger 2.0 spec (no `openapi` or `swagger` field)".to_string())
}

/// Walks a parsed spec, collecting warnings for lossy mappings.
struct Mapper<'a> {
    spec: &'a Value,
    version: Version,
    base_url: String,
    warnings: Vec<String>,
    /// External `$ref`s already warned about, so each is reported once.
    warned_refs: HashSet<String>,
}

impl<'a> Mapper<'a> {
    fn new(spec: &'a Value, version: Version) -> Self {
        let base_url = match version {
            Version::OpenApi3 => base_url_v3(spec),
            Version::Swagger2 => base_url_v2(spec),
        };
        Self {
            spec,
            version,
            base_url,
            warnings: Vec::new(),
            warned_refs: HashSet::new(),
        }
    }

    /// Follows `value`'s local `$ref` chain to the referenced value. An
    /// external or broken ref warns once and yields `None`.
    fn resolve(&mut self, value: &'a Value) -> Option<&'a Value> {
        let mut current = value;
        let mut seen = HashSet::new();
        while let Some(r) = current.get("$ref").and_then(Value::as_str) {
            if !seen.insert(r) {
                return None;
            }
            match r.strip_prefix('#').and_then(|p| self.spec.pointer(p)) {
                Some(target) => current = target,
                None => {
                    if self.warned_refs.insert(r.to_string()) {
                        self.warnings.push(format!(
                            "$ref {r:?} is external or missing, left out of the import"
                        ));
                    }
                    return None;
                }
            }
        }
        Some(current)
    }

    /// Builds an example value from a schema. `refs` holds the `$ref`s on the
    /// current path, so a recursive schema stops instead of looping.
    fn example(&mut self, schema: &'a Value, refs: &mut Vec<&'a str>, depth: usize) -> Value {
        if depth > MAX_DEPTH {
            return Value::Null;
        }
        if let Some(r) = schema.get("$ref").and_then(Value::as_str) {
            if refs.contains(&r) {
                return Value::Null;
            }
            let Some(target) = self.resolve(schema) else {
                return Value::Null;
            };
            refs.push(r);
            let out = self.example(target, refs, depth + 1);
            refs.pop();
            return out;
        }
        for key in ["example", "const", "default"] {
            if let Some(v) = schema.get(key) {
                return v.clone();
            }
        }
        if let Some(v) = schema
            .get("examples")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
        {
            return v.clone();
        }
        if let Some(v) = schema
            .get("enum")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
        {
            return v.clone();
        }
        if let Some(all) = schema.get("allOf").and_then(Value::as_array) {
            let mut merged = Map::new();
            let mut last = Value::Null;
            for part in all {
                match self.example(part, refs, depth + 1) {
                    Value::Object(obj) => merged.extend(obj),
                    other => last = other,
                }
            }
            // Sibling properties next to allOf belong to the same object.
            if let Value::Object(obj) = self.object_example(schema, refs, depth) {
                merged.extend(obj);
            }
            return if merged.is_empty() {
                last
            } else {
                Value::Object(merged)
            };
        }
        for key in ["oneOf", "anyOf"] {
            if let Some(first) = schema
                .get(key)
                .and_then(Value::as_array)
                .and_then(|a| a.first())
            {
                return self.example(first, refs, depth + 1);
            }
        }
        match schema_type(schema) {
            Some("object") => self.object_example(schema, refs, depth),
            Some("array") => match schema.get("items") {
                Some(items) => Value::Array(vec![self.example(items, refs, depth + 1)]),
                None => Value::Array(Vec::new()),
            },
            Some("string") => Value::String(string_example(schema)),
            Some("integer") | Some("number") => Value::from(0),
            Some("boolean") => Value::Bool(false),
            Some("null") => Value::Null,
            _ if schema.get("properties").is_some() => self.object_example(schema, refs, depth),
            _ => Value::Null,
        }
    }

    /// An object example: one key per declared property.
    fn object_example(
        &mut self,
        schema: &'a Value,
        refs: &mut Vec<&'a str>,
        depth: usize,
    ) -> Value {
        let mut out = Map::new();
        if let Some(props) = schema.get("properties").and_then(Value::as_object) {
            for (name, prop) in props {
                out.insert(name.clone(), self.example(prop, refs, depth + 1));
            }
        }
        Value::Object(out)
    }

    /// The example for a parameter, header or media object: its own
    /// `example`, then the first of its `examples`, then one built from its
    /// schema (Swagger 2.0 parameters carry the schema fields inline).
    fn value_example(&mut self, obj: &'a Value) -> Value {
        if let Some(v) = obj.get("example") {
            return self.inline_refs(v, 0);
        }
        if let Some(first) = obj
            .get("examples")
            .and_then(Value::as_object)
            .and_then(|m| m.values().next())
        {
            // An OpenAPI 3 Example Object wraps the payload in `value`.
            if let Some(ex) = self.resolve(first) {
                return self.inline_refs(ex.get("value").unwrap_or(&Value::Null), 0);
            }
        }
        let schema = obj.get("schema").unwrap_or(obj);
        self.example(schema, &mut Vec::new(), 0)
    }

    /// Copies an example value, replacing each `{"$ref": "#/..."}` object in
    /// it with what it points at. Example values are literal in OpenAPI, but
    /// some specs (GitHub's among them) reference other examples this way,
    /// and left as is the `$ref` key would read as an apic variable. A ref
    /// to an Example Object takes its `value`.
    fn inline_refs(&mut self, value: &'a Value, depth: usize) -> Value {
        match value {
            Value::Object(map) if depth <= MAX_DEPTH => {
                let local_ref = map.len() == 1
                    && map
                        .get("$ref")
                        .and_then(Value::as_str)
                        .is_some_and(|r| r.starts_with('#'));
                if local_ref {
                    return match self.resolve(value) {
                        Some(target) => {
                            let target = target.get("value").unwrap_or(target);
                            self.inline_refs(target, depth + 1)
                        }
                        None => Value::Null,
                    };
                }
                Value::Object(
                    map.iter()
                        .map(|(k, v)| (k.clone(), self.inline_refs(v, depth + 1)))
                        .collect(),
                )
            }
            Value::Array(items) if depth <= MAX_DEPTH => Value::Array(
                items
                    .iter()
                    .map(|v| self.inline_refs(v, depth + 1))
                    .collect(),
            ),
            other => other.clone(),
        }
    }

    /// Maps every operation in the spec. Operations land in a folder named
    /// after their first tag, or at the root when untagged.
    fn map(&mut self) -> Vec<MappedContract> {
        let spec = self.spec;
        let Some(paths) = spec.get("paths").and_then(Value::as_object) else {
            return Vec::new();
        };

        // Collect operations first so tag folders can claim their names
        // before any root-level file does.
        let mut ops: Vec<(&'a str, &'a str, &'a Value, &'a Value)> = Vec::new();
        for (path, item) in paths {
            let Some(item) = self.resolve(item) else {
                continue;
            };
            for method in METHODS {
                if let Some(op) = item.get(method) {
                    ops.push((path.as_str(), method, item, op));
                }
            }
        }

        let mut taken: HashMap<PathBuf, HashSet<String>> = HashMap::new();
        let mut tag_dirs: HashMap<String, PathBuf> = HashMap::new();
        for (_, _, _, op) in &ops {
            if let Some(tag) = first_tag(op)
                && !tag_dirs.contains_key(tag)
            {
                let slug = slugify(tag);
                let root = taken.entry(PathBuf::new()).or_default();
                // Tags that slug the same share one folder.
                root.insert(slug.clone());
                tag_dirs.insert(tag.to_string(), PathBuf::from(slug));
            }
        }

        let mut out = Vec::new();
        for (path, method, item, op) in ops {
            let dir = first_tag(op)
                .and_then(|t| tag_dirs.get(t).cloned())
                .unwrap_or_default();
            let (contract, file_stem) = self.operation(path, method, item, op);
            let warning = method_warning(method, &contract.name);
            let slug = unique_slug(taken.entry(dir.clone()).or_default(), &file_stem);
            out.push(MappedContract {
                rel_path: dir.join(format!("{slug}.json")),
                contract,
                warning,
            });
        }
        out
    }

    /// Maps one operation to a contract and the slug its file is named from.
    fn operation(
        &mut self,
        path: &str,
        method: &str,
        item: &'a Value,
        op: &'a Value,
    ) -> (JsonContent, String) {
        let summary = op.get("summary").and_then(Value::as_str);
        let op_id = op.get("operationId").and_then(Value::as_str);
        let upper = method.to_uppercase();
        let name = summary
            .or(op_id)
            .map(str::to_string)
            .unwrap_or_else(|| format!("{upper} {path}"));
        let file_stem = match op_id.or(summary) {
            Some(s) => slugify(&split_camel(s)),
            None => slugify(&format!("{method} {path}")),
        };
        let description = op
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string);

        let mut query = Vec::new();
        let mut headers = Vec::new();
        let mut request = None;
        let mut multipart = None;
        let mut form_parts: Vec<Part> = Vec::new();

        for param in self.parameters(item, op) {
            let pname = param
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let required = param
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            match param.get("in").and_then(Value::as_str) {
                Some("query") => query.push(Query {
                    name: pname,
                    value: scalar_text(&self.value_example(param)),
                    description: param
                        .get("description")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    required,
                }),
                Some("header") => headers.push(Header {
                    name: pname,
                    value: scalar_text(&self.value_example(param)),
                    required,
                }),
                // Swagger 2.0 carries the request body as a parameter.
                Some("body") => request = Some(self.value_example(param)),
                Some("formData") => form_parts.push(self.form_part(param, pname, required)),
                // Path params are inline in the url; cookies are not modeled.
                _ => {}
            }
        }

        if self.version == Version::OpenApi3
            && let Some(body) = op.get("requestBody").and_then(|b| self.resolve(b))
        {
            let (json, parts, content_type) = self.request_body(body, &name);
            request = json;
            multipart = parts;
            if let Some(ct) = content_type {
                headers.push(Header {
                    name: "Content-Type".to_string(),
                    value: ct,
                    required: true,
                });
            }
        }
        if !form_parts.is_empty() {
            let urlencoded = op
                .get("consumes")
                .or_else(|| self.spec.get("consumes"))
                .and_then(Value::as_array)
                .is_some_and(|c| c.iter().all(|t| t != "multipart/form-data"));
            if urlencoded && form_parts.iter().all(|p| p.filename.is_none()) {
                request = Some(Value::Object(
                    form_parts
                        .into_iter()
                        .map(|p| (p.name, Value::String(p.value)))
                        .collect(),
                ));
                self.warnings.push(format!(
                    "request {name:?} sends a form-urlencoded body, imported as a JSON body"
                ));
            } else {
                multipart = Some(form_parts);
            }
        }

        let responses = self.responses(op);
        let contract = JsonContent {
            name,
            description,
            method: method_from_str(method),
            url: format!("{}{path}", self.base_url),
            query,
            headers,
            request,
            multipart,
            responses,
        };
        (contract, file_stem)
    }

    /// The operation's parameters merged over the path item's: an operation
    /// parameter replaces a path-level one with the same `name` and `in`.
    fn parameters(&mut self, item: &'a Value, op: &'a Value) -> Vec<&'a Value> {
        let mut out: Vec<&'a Value> = Vec::new();
        for source in [item, op] {
            let Some(list) = source.get("parameters").and_then(Value::as_array) else {
                continue;
            };
            for raw in list {
                let Some(param) = self.resolve(raw) else {
                    continue;
                };
                let key = (param.get("name"), param.get("in"));
                out.retain(|p| (p.get("name"), p.get("in")) != key);
                out.push(param);
            }
        }
        out
    }

    /// A Swagger 2.0 `formData` parameter as a multipart part.
    fn form_part(&mut self, param: &'a Value, name: String, required: bool) -> Part {
        let is_file = param.get("type").and_then(Value::as_str) == Some("file");
        Part {
            value: if is_file {
                String::new()
            } else {
                scalar_text(&self.value_example(param))
            },
            filename: is_file.then(|| name.clone()),
            content_type: None,
            description: param
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string),
            required,
            name,
        }
    }

    /// Maps an OpenAPI 3 request body to a JSON body or multipart parts, plus
    /// a `Content-Type` header when the media type is one `apic send` would
    /// not set itself.
    fn request_body(
        &mut self,
        body: &'a Value,
        name: &str,
    ) -> (Option<Value>, Option<Vec<Part>>, Option<String>) {
        let Some(content) = body.get("content").and_then(Value::as_object) else {
            return (None, None, None);
        };
        let Some((media_type, media)) = pick_media(content) else {
            return (None, None, None);
        };
        let base = media_type.split(';').next().unwrap_or(media_type).trim();
        if base == "multipart/form-data" {
            return (None, Some(self.multipart_parts(media)), None);
        }
        let json = self.value_example(media);
        if is_json(base) {
            let ct = (base != "application/json").then(|| media_type.to_string());
            (Some(json), None, ct)
        } else {
            self.warnings.push(format!(
                "request {name:?} sends {base}, imported as a JSON body"
            ));
            (Some(json), None, Some(media_type.to_string()))
        }
    }

    /// One part per property of a multipart schema. A `format: binary`
    /// property is a file part.
    fn multipart_parts(&mut self, media: &'a Value) -> Vec<Part> {
        let Some(schema) = media.get("schema").and_then(|s| self.resolve(s)) else {
            return Vec::new();
        };
        let required: HashSet<&str> = schema
            .get("required")
            .and_then(Value::as_array)
            .map(|r| r.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let encoding = media.get("encoding");
        let Some(props) = schema.get("properties").and_then(Value::as_object) else {
            return Vec::new();
        };
        let mut parts = Vec::new();
        for (pname, prop) in props {
            let prop_resolved = self.resolve(prop).unwrap_or(prop);
            let is_file = is_binary(prop_resolved);
            let content_type = encoding
                .and_then(|e| e.get(pname))
                .and_then(|e| e.get("contentType"))
                .and_then(Value::as_str)
                .map(str::to_string);
            parts.push(Part {
                name: pname.clone(),
                value: if is_file {
                    String::new()
                } else {
                    scalar_text(&self.example(prop, &mut Vec::new(), 0))
                },
                filename: is_file.then(|| pname.clone()),
                content_type,
                description: prop_resolved
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                required: required.contains(pname.as_str()),
            });
        }
        parts
    }

    /// Maps the operation's responses. `default` has no status code, so it
    /// is left out; a range such as `2XX` becomes its first code.
    fn responses(&mut self, op: &'a Value) -> Vec<Response> {
        let Some(map) = op.get("responses").and_then(Value::as_object) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (code, raw) in map {
            let Some(code) = status_code(code) else {
                continue;
            };
            let Some(resp) = self.resolve(raw) else {
                continue;
            };
            let description = resp
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let headers = self.response_headers(resp);
            let (schema, multipart) = match self.version {
                Version::OpenApi3 => self.response_body_v3(resp),
                Version::Swagger2 => (self.response_body_v2(resp), None),
            };
            out.push(Response {
                code,
                description,
                headers,
                schema,
                multipart,
            });
        }
        out
    }

    fn response_headers(&mut self, resp: &'a Value) -> Vec<Header> {
        let Some(map) = resp.get("headers").and_then(Value::as_object) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (name, raw) in map {
            let Some(header) = self.resolve(raw) else {
                continue;
            };
            out.push(Header {
                name: name.clone(),
                value: scalar_text(&self.value_example(header)),
                required: header
                    .get("required")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            });
        }
        out
    }

    fn response_body_v3(&mut self, resp: &'a Value) -> (Option<Value>, Option<Vec<Part>>) {
        let Some(content) = resp.get("content").and_then(Value::as_object) else {
            return (None, None);
        };
        let Some((media_type, media)) = pick_media(content) else {
            return (None, None);
        };
        if media_type.starts_with("multipart/form-data") {
            return (None, Some(self.multipart_parts(media)));
        }
        // A media type with neither example nor schema documents no body.
        if media.get("schema").is_none()
            && media.get("example").is_none()
            && media.get("examples").is_none()
        {
            return (None, None);
        }
        (Some(self.value_example(media)), None)
    }

    fn response_body_v2(&mut self, resp: &'a Value) -> Option<Value> {
        if let Some(examples) = resp.get("examples").and_then(Value::as_object) {
            let json = examples
                .iter()
                .find(|(t, _)| is_json(t))
                .or_else(|| examples.iter().next());
            if let Some((_, v)) = json {
                return Some(v.clone());
            }
        }
        let schema = resp.get("schema")?;
        Some(self.example(schema, &mut Vec::new(), 0))
    }
}

/// The base URL for an OpenAPI 3 spec: the first server, with its variables
/// filled from their defaults. A missing or relative server is prefixed with
/// the `{{baseUrl}}` variable so it can be set per environment.
fn base_url_v3(spec: &Value) -> String {
    let Some(server) = spec
        .get("servers")
        .and_then(Value::as_array)
        .and_then(|s| s.first())
    else {
        return "{{baseUrl}}".to_string();
    };
    let mut url = server
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if let Some(vars) = server.get("variables").and_then(Value::as_object) {
        for (name, var) in vars {
            if let Some(default) = var.get("default").and_then(Value::as_str) {
                url = url.replace(&format!("{{{name}}}"), default);
            }
        }
    }
    let url = url.trim_end_matches('/');
    if url.contains("://") {
        url.to_string()
    } else {
        format!("{{{{baseUrl}}}}{url}")
    }
}

/// The base URL for a Swagger 2.0 spec from `schemes`, `host` and
/// `basePath`. With no host, the `{{baseUrl}}` variable stands in for it.
fn base_url_v2(spec: &Value) -> String {
    let base_path = spec
        .get("basePath")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim_end_matches('/');
    match spec.get("host").and_then(Value::as_str) {
        Some(host) => {
            let scheme = spec
                .get("schemes")
                .and_then(Value::as_array)
                .and_then(|s| {
                    s.iter()
                        .filter_map(Value::as_str)
                        .find(|s| *s == "https")
                        .or_else(|| s.first().and_then(Value::as_str))
                })
                .unwrap_or("https");
            format!("{scheme}://{host}{base_path}")
        }
        None => format!("{{{{baseUrl}}}}{base_path}"),
    }
}

/// The first tag of an operation, if any.
fn first_tag(op: &Value) -> Option<&str> {
    op.get("tags")
        .and_then(Value::as_array)
        .and_then(|t| t.first())
        .and_then(Value::as_str)
}

/// The schema's type. OpenAPI 3.1 allows a list, where the first non-null
/// entry is used.
fn schema_type(schema: &Value) -> Option<&str> {
    match schema.get("type")? {
        Value::String(s) => Some(s),
        Value::Array(list) => list
            .iter()
            .filter_map(Value::as_str)
            .find(|t| *t != "null")
            .or(Some("null")),
        _ => None,
    }
}

/// A placeholder string shaped by the schema's `format`.
fn string_example(schema: &Value) -> String {
    match schema.get("format").and_then(Value::as_str) {
        Some("date-time") => "2024-01-01T00:00:00Z",
        Some("date") => "2024-01-01",
        Some("time") => "00:00:00",
        Some("email") => "user@example.com",
        Some("uuid") => "00000000-0000-0000-0000-000000000000",
        Some("uri") | Some("url") => "https://example.com",
        Some("hostname") => "example.com",
        Some("ipv4") => "127.0.0.1",
        Some("ipv6") => "::1",
        Some("binary") | Some("byte") => "",
        _ => "string",
    }
    .to_string()
}

/// Whether a schema describes file content.
fn is_binary(schema: &Value) -> bool {
    matches!(
        schema.get("format").and_then(Value::as_str),
        Some("binary") | Some("base64")
    ) || schema.get("contentMediaType").is_some()
        || schema_type(schema) == Some("file")
}

/// Whether a media type carries JSON.
fn is_json(media_type: &str) -> bool {
    let base = media_type.split(';').next().unwrap_or(media_type).trim();
    base == "application/json" || base.ends_with("+json") || base == "*/*"
}

/// Picks the media type to import from a `content` map: JSON first, then
/// multipart, then whatever comes first.
fn pick_media(content: &Map<String, Value>) -> Option<(&str, &Value)> {
    let find = |pred: &dyn Fn(&str) -> bool| {
        content
            .iter()
            .find(|(t, _)| pred(t))
            .map(|(t, m)| (t.as_str(), m))
    };
    find(&|t| is_json(t) && t != "*/*")
        .or_else(|| find(&|t| t.starts_with("multipart/form-data")))
        .or_else(|| content.iter().next().map(|(t, m)| (t.as_str(), m)))
}

/// The status code for a response key: `200` as is, `2XX` as `200`, and
/// `None` for `default` or anything else.
fn status_code(key: &str) -> Option<u16> {
    if let Ok(code) = key.parse::<u16>() {
        return Some(code);
    }
    let mut chars = key.chars();
    let first = chars.next()?.to_digit(10)?;
    let rest = chars.as_str();
    rest.eq_ignore_ascii_case("xx")
        .then_some(first as u16 * 100)
}

/// A scalar example as text for a query, header or form value: strings
/// unquoted, null empty, anything else as JSON.
fn scalar_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Splits a camelCase identifier into words, so `listPets` slugs to
/// `list_pets`.
fn split_camel(s: &str) -> String {
    let mut out = String::new();
    let mut prev_lower = false;
    for ch in s.chars() {
        if ch.is_ascii_uppercase() && prev_lower {
            out.push(' ');
        }
        prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        out.push(ch);
    }
    out
}

/// Maps a parsed spec to contracts.
///
/// # Returns
///
/// The contracts and the warnings for lossy mappings that are not tied to a
/// single contract (external refs, form bodies).
///
/// # Errors
///
/// Returns an error when `spec` is not OpenAPI 3.x or Swagger 2.0.
pub(crate) fn map(spec: &Value) -> Result<(Vec<MappedContract>, Vec<String>), String> {
    let version = detect(spec)?;
    let mut mapper = Mapper::new(spec, version);
    let mapped = mapper.map();
    Ok((mapped, mapper.warnings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn map_ok(spec: Value) -> (Vec<MappedContract>, Vec<String>) {
        map(&spec).expect("spec maps")
    }

    #[test]
    fn detects_versions_and_rejects_others() {
        assert_eq!(detect(&json!({"openapi": "3.1.0"})), Ok(Version::OpenApi3));
        assert_eq!(detect(&json!({"swagger": "2.0"})), Ok(Version::Swagger2));
        assert!(detect(&json!({"openapi": "4.0.0"})).is_err());
        assert!(detect(&json!({"info": {}})).is_err());
    }

    #[test]
    fn yaml_with_numeric_response_keys_parses() {
        let spec = from_str(
            "openapi: 3.0.3\npaths:\n  /a:\n    get:\n      responses:\n        200:\n          description: ok\n",
        )
        .expect("yaml parses");
        assert!(spec.pointer("/paths/~1a/get/responses/200").is_some());
    }

    #[test]
    fn v3_maps_tags_params_body_and_responses() {
        let (mapped, warnings) = map_ok(json!({
            "openapi": "3.0.3",
            "servers": [{"url": "https://{env}.example.com/v1/", "variables": {"env": {"default": "api"}}}],
            "paths": {
                "/pets/{id}": {
                    "parameters": [{"name": "id", "in": "path", "required": true, "schema": {"type": "string"}}],
                    "put": {
                        "tags": ["Pets"],
                        "operationId": "updatePet",
                        "summary": "Update a pet",
                        "parameters": [
                            {"name": "notify", "in": "query", "schema": {"type": "boolean", "default": true}},
                            {"name": "X-Trace", "in": "header", "required": true, "example": "abc"}
                        ],
                        "requestBody": {"content": {"application/json": {"schema": {"$ref": "#/components/schemas/Pet"}}}},
                        "responses": {
                            "200": {"description": "Updated", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/Pet"}}}},
                            "4XX": {"description": "Bad"},
                            "default": {"description": "Error"}
                        }
                    }
                }
            },
            "components": {"schemas": {"Pet": {
                "type": "object",
                "properties": {"id": {"type": "integer"}, "name": {"type": "string", "example": "Rex"}, "born": {"type": "string", "format": "date"}}
            }}}
        }));
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(mapped.len(), 1);
        let m = &mapped[0];
        assert_eq!(m.rel_path, Path::new("pets").join("update_pet.json"));
        let c = &m.contract;
        assert_eq!(c.name, "Update a pet");
        assert!(matches!(c.method, crate::json::Method::PUT));
        assert_eq!(c.url, "https://api.example.com/v1/pets/{id}");
        assert_eq!(c.query[0].name, "notify");
        assert_eq!(c.query[0].value, "true");
        assert_eq!(c.headers[0].name, "X-Trace");
        assert!(c.headers[0].required);
        assert_eq!(
            c.request,
            Some(json!({"id": 0, "name": "Rex", "born": "2024-01-01"}))
        );
        let codes: Vec<u16> = c.responses.iter().map(|r| r.code).collect();
        assert_eq!(codes, vec![200, 400]);
        assert_eq!(c.responses[0].schema, c.request);
        assert!(c.responses[1].schema.is_none());
    }

    #[test]
    fn v3_recursive_schema_terminates_and_allof_merges() {
        let (mapped, _) = map_ok(json!({
            "openapi": "3.1.0",
            "paths": {"/n": {"get": {"responses": {"200": {"description": "ok",
                "content": {"application/json": {"schema": {"$ref": "#/components/schemas/Node"}}}}}}}},
            "components": {"schemas": {
                "Base": {"type": "object", "properties": {"id": {"type": "integer"}}},
                "Node": {"allOf": [{"$ref": "#/components/schemas/Base"}],
                         "properties": {"child": {"$ref": "#/components/schemas/Node"},
                                        "tags": {"type": ["array", "null"], "items": {"type": "string"}}}}
            }}
        }));
        let c = &mapped[0].contract;
        assert_eq!(c.url, "{{baseUrl}}/n");
        assert_eq!(mapped[0].rel_path, Path::new("get_n.json"));
        assert_eq!(
            c.responses[0].schema,
            Some(json!({"id": 0, "child": null, "tags": ["string"]}))
        );
    }

    #[test]
    fn v3_multipart_and_examples() {
        let (mapped, _) = map_ok(json!({
            "openapi": "3.0.0",
            "paths": {"/upload": {"post": {
                "operationId": "upload",
                "requestBody": {"content": {"multipart/form-data": {"schema": {
                    "type": "object", "required": ["file"],
                    "properties": {"file": {"type": "string", "format": "binary"}, "title": {"type": "string"}}
                }}}},
                "responses": {"201": {"description": "Created", "content": {"application/json": {
                    "examples": {"ok": {"value": {"done": true}}}
                }}}}
            }}}
        }));
        let c = &mapped[0].contract;
        let parts = c.multipart.as_ref().expect("multipart");
        assert_eq!(parts[0].filename.as_deref(), Some("file"));
        assert!(parts[0].required);
        assert_eq!(parts[1].value, "string");
        assert!(c.request.is_none());
        assert_eq!(c.responses[0].schema, Some(json!({"done": true})));
    }

    #[test]
    fn v3_refs_inside_example_values_are_followed() {
        let (mapped, _) = map_ok(json!({
            "openapi": "3.0.0",
            "paths": {"/v": {"get": {"responses": {"200": {"description": "ok",
                "content": {"application/json": {"examples": {
                    "table": {"value": {"$ref": "#/components/examples/View"}}
                }}}}}}}},
            "components": {"examples": {
                "View": {"value": {"id": 7, "owner": {"$ref": "#/components/examples/User"}}},
                "User": {"value": {"login": "octo"}}
            }}
        }));
        assert_eq!(
            mapped[0].contract.responses[0].schema,
            Some(json!({"id": 7, "owner": {"login": "octo"}}))
        );
    }

    #[test]
    fn swagger2_maps_host_body_and_form() {
        let (mapped, _) = map_ok(json!({
            "swagger": "2.0",
            "host": "petstore.example.com",
            "basePath": "/v2",
            "schemes": ["http", "https"],
            "paths": {
                "/pet": {"post": {
                    "tags": ["pet"], "operationId": "addPet",
                    "parameters": [{"in": "body", "name": "body", "schema": {"$ref": "#/definitions/Pet"}}],
                    "responses": {"200": {"description": "ok", "schema": {"$ref": "#/definitions/Pet"}}}
                }},
                "/pet/{petId}/uploadImage": {"post": {
                    "tags": ["pet"], "operationId": "uploadFile",
                    "consumes": ["multipart/form-data"],
                    "parameters": [
                        {"in": "formData", "name": "file", "type": "file"},
                        {"in": "formData", "name": "note", "type": "string"}
                    ],
                    "responses": {"200": {"description": "ok", "examples": {"application/json": {"code": 1}}}}
                }}
            },
            "definitions": {"Pet": {"type": "object", "properties": {"name": {"type": "string"}}}}
        }));
        assert_eq!(mapped.len(), 2);
        let add = &mapped[0].contract;
        assert_eq!(add.url, "https://petstore.example.com/v2/pet");
        assert_eq!(add.request, Some(json!({"name": "string"})));
        assert_eq!(add.responses[0].schema, Some(json!({"name": "string"})));
        let up = &mapped[1].contract;
        let parts = up.multipart.as_ref().expect("multipart");
        assert_eq!(parts[0].filename.as_deref(), Some("file"));
        assert_eq!(parts[1].value, "string");
        assert_eq!(up.responses[0].schema, Some(json!({"code": 1})));
        assert_eq!(
            mapped[1].rel_path,
            Path::new("pet").join("upload_file.json")
        );
    }

    #[test]
    fn external_ref_warns_once_and_trace_warns() {
        let (mapped, warnings) = map_ok(json!({
            "openapi": "3.0.0",
            "paths": {"/x": {
                "trace": {"responses": {"200": {"description": "ok"}}},
                "get": {"responses": {
                    "200": {"description": "a", "content": {"application/json": {"schema": {"$ref": "other.yaml#/A"}}}},
                    "201": {"description": "b", "content": {"application/json": {"schema": {"$ref": "other.yaml#/A"}}}}
                }}
            }}
        }));
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("other.yaml"));
        let trace = mapped
            .iter()
            .find(|m| m.contract.name == "TRACE /x")
            .unwrap();
        assert!(trace.warning.as_deref().unwrap().contains("TRACE"));
    }

    #[test]
    fn tag_folder_and_root_file_do_not_collide() {
        let (mapped, _) = map_ok(json!({
            "openapi": "3.0.0",
            "paths": {
                "/a": {"get": {"operationId": "users", "responses": {}}},
                "/b": {"get": {"operationId": "list", "tags": ["Users"], "responses": {}}}
            }
        }));
        let paths: Vec<String> = mapped
            .iter()
            .map(|m| m.rel_path.to_string_lossy().replace('\\', "/"))
            .collect();
        assert_eq!(paths, vec!["users_2.json", "users/list.json"]);
    }

    #[test]
    fn status_codes() {
        assert_eq!(status_code("204"), Some(204));
        assert_eq!(status_code("5XX"), Some(500));
        assert_eq!(status_code("default"), None);
    }
}
