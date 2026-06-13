//! `apic convert` — import a Postman collection as apic contract files.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::converter;
use crate::converter::{PostmanCollection, v1_0_0, v2_0_0, v2_1_0};
use crate::file::confine_to_dir;
use crate::json::{
    Header, JsonContent, Query, RequestBody, Response, Url, Variable, method_from_str,
};

/// Convert a human request/folder name into a filesystem-safe slug using
/// underscores: lowercase, runs of non-alphanumeric characters collapse to a
/// single `_`, leading/trailing `_` trimmed. Empty input yields `"unnamed"`.
fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut prev_sep = true; // true so a leading separator run is dropped
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.extend(ch.to_lowercase());
            prev_sep = false;
        } else if !prev_sep {
            out.push('_');
            prev_sep = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        "unnamed".to_string()
    } else {
        out
    }
}

/// Reserve a unique slug within one directory. If `base` is taken, append
/// `_2`, `_3`, … until free. Records the chosen slug in `taken`.
fn unique_slug(taken: &mut HashSet<String>, base: &str) -> String {
    if taken.insert(base.to_string()) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}_{n}");
        if taken.insert(candidate.clone()) {
            return candidate;
        }
        n += 1;
    }
}

/// Parse a raw Postman URL string into apic's [`Url`].
///
/// Splits `scheme://host/path?query#frag`. The fragment is dropped. Path
/// segments are kept verbatim (including `:id` / `{id}` placeholders); each
/// placeholder segment also contributes a [`Variable`] entry documenting it.
/// Query pairs become [`Query`] entries (`required: false`). A missing scheme
/// yields an empty `protocol`; a host-only URL yields no path.
fn split_raw_url(raw: &str) -> Url {
    let raw = raw.split('#').next().unwrap_or(raw); // drop fragment

    let (protocol, rest) = match raw.split_once("://") {
        Some((scheme, rest)) => (scheme.to_string(), rest),
        None => (String::new(), raw),
    };

    let (host_path, query_str) = match rest.split_once('?') {
        Some((hp, q)) => (hp, Some(q)),
        None => (rest, None),
    };

    let (host, path_str) = match host_path.split_once('/') {
        Some((h, p)) => (h.to_string(), p),
        None => (host_path.to_string(), ""),
    };

    let segments: Vec<String> = path_str
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    let variables: Vec<Variable> = segments
        .iter()
        .filter_map(|seg| placeholder_name(seg))
        .map(|name| Variable {
            name,
            dtype: "string".to_string(),
            description: None,
            required: true,
        })
        .collect();

    let query: Vec<Query> = match query_str {
        Some(q) if !q.is_empty() => q
            .split('&')
            .filter(|pair| !pair.is_empty())
            .map(|pair| {
                let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
                Query {
                    name: k.to_string(),
                    value: v.to_string(),
                    description: None,
                    required: false,
                }
            })
            .collect(),
        _ => Vec::new(),
    };

    Url {
        protocol,
        host,
        path: if segments.is_empty() {
            None
        } else {
            Some(segments)
        },
        query: if query.is_empty() { None } else { Some(query) },
        variable: if variables.is_empty() {
            None
        } else {
            Some(variables)
        },
    }
}

/// If a path segment is a placeholder (`:id` or `{id}`), return its bare name.
fn placeholder_name(segment: &str) -> Option<String> {
    if let Some(name) = segment.strip_prefix(':') {
        return (!name.is_empty()).then(|| name.to_string());
    }
    if let Some(inner) = segment.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        return (!inner.is_empty()).then(|| inner.to_string());
    }
    None
}

/// Version-agnostic shape extracted from one Postman request, ready to build
/// into a [`JsonContent`]. Every version's walker produces this.
struct RawRequest {
    name: String,
    description: Option<String>,
    method: String,
    raw_url: String,
    headers: Vec<(String, String)>,
    /// Raw request body text (e.g. Postman `body.raw`), if any.
    body: Option<String>,
    /// Saved responses: (status code, status text, raw body text).
    responses: Vec<(u16, String, Option<String>)>,
}

/// Parse `raw` body/response text as JSON; fall back to a JSON string value when
/// it is not valid JSON (e.g. plain text). `None` when `raw` is `None`/empty.
fn body_example(raw: Option<&str>) -> Option<serde_json::Value> {
    let text = raw?.trim();
    if text.is_empty() {
        return None;
    }
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(value) => Some(value),
        Err(_) => Some(serde_json::Value::String(text.to_string())),
    }
}

/// Build an apic contract from extracted primitives.
fn build_contract(raw: RawRequest) -> JsonContent {
    let headers = raw
        .headers
        .into_iter()
        .map(|(name, value)| Header { name, value })
        .collect();

    let request = raw.body.as_deref().and_then(|text| {
        body_example(Some(text)).map(|example| RequestBody {
            dtype: "object".to_string(),
            schema: None,
            example: Some(example),
        })
    });

    let responses = raw
        .responses
        .into_iter()
        .map(|(code, status, body)| Response {
            code,
            description: status,
            dtype: "object".to_string(),
            schema: Vec::new(),
            example: body_example(body.as_deref()),
        })
        .collect();

    JsonContent {
        name: raw.name,
        description: raw.description,
        method: method_from_str(&raw.method),
        url: split_raw_url(&raw.raw_url),
        headers,
        request,
        responses,
    }
}

/// One contract destined for a file at `rel_path` (relative to `--destination`).
pub(crate) struct MappedContract {
    pub rel_path: PathBuf,
    pub contract: JsonContent,
}

// ---- v2.1 ----

/// Walk a v2.1 collection into mapped contracts.
fn map_v2_1(spec: &v2_1_0::Spec) -> Vec<MappedContract> {
    let mut out = Vec::new();
    walk_v2_1(&spec.item, Path::new(""), &mut out);
    out
}

fn walk_v2_1(items: &[v2_1_0::Items], dir: &Path, out: &mut Vec<MappedContract>) {
    let mut taken = HashSet::new();
    for item in items {
        match item {
            v2_1_0::Items::ItemGroup(group) => {
                let name = group.name.as_deref().unwrap_or("folder");
                let slug = unique_slug(&mut taken, &slugify(name));
                let child_dir = dir.join(&slug);
                walk_v2_1(&group.item, &child_dir, out);
            }
            v2_1_0::Items::Item(it) => {
                if let Some(raw) = raw_request_v2_1(it) {
                    let slug = unique_slug(&mut taken, &slugify(&raw.name));
                    out.push(MappedContract {
                        rel_path: dir.join(format!("{slug}.json")),
                        contract: build_contract(raw),
                    });
                }
            }
        }
    }
}

fn raw_request_v2_1(item: &v2_1_0::Item) -> Option<RawRequest> {
    let req = match &item.request {
        v2_1_0::RequestUnion::RequestClass(req) => req,
        v2_1_0::RequestUnion::String(_) => return None, // bare-URL string form: skip
    };

    let name = item.name.clone().unwrap_or_else(|| "request".to_string());
    let description = item.description.as_ref().and_then(description_text_v2_1);
    let method = req.method.clone().unwrap_or_else(|| "GET".to_string());
    let raw_url = req.url.as_ref().map(url_raw_v2_1).unwrap_or_default();

    let mut headers = match &req.header {
        Some(v2_1_0::HeaderUnion::HeaderArray(list)) => list
            .iter()
            .filter(|h| !h.disabled.unwrap_or(false))
            .map(|h| (h.key.clone(), h.value.clone()))
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    if let Some(auth_header) = req.auth.as_ref().and_then(auth_header_v2_1) {
        headers.push(auth_header);
    }

    let body = req.body.as_ref().and_then(|b| b.raw.clone());

    let responses = item
        .response
        .as_ref()
        .map(|rs| {
            rs.iter()
                .map(|r| {
                    let code = r.code.unwrap_or(0) as u16;
                    let status = r.status.clone().unwrap_or_else(|| code.to_string());
                    (code, status, r.body.clone())
                })
                .collect()
        })
        .unwrap_or_default();

    Some(RawRequest {
        name,
        description,
        method,
        raw_url,
        headers,
        body,
        responses,
    })
}

fn description_text_v2_1(d: &v2_1_0::DescriptionUnion) -> Option<String> {
    match d {
        v2_1_0::DescriptionUnion::String(s) => Some(s.clone()),
        v2_1_0::DescriptionUnion::Description(desc) => desc.content.clone(),
    }
}

fn url_raw_v2_1(url: &v2_1_0::Url) -> String {
    match url {
        v2_1_0::Url::String(s) => s.clone(),
        v2_1_0::Url::UrlClass(u) => u.raw.clone().unwrap_or_default(),
    }
}

/// Synthesize a single `Authorization`/api-key header from v2.1 auth.
fn auth_header_v2_1(auth: &v2_1_0::Auth) -> Option<(String, String)> {
    let attr = |attrs: &Option<Vec<v2_1_0::AuthAttribute>>, key: &str| -> Option<String> {
        attrs.as_ref()?.iter().find(|a| a.key == key).and_then(|a| {
            a.value.as_ref().map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
        })
    };
    match auth.auth_type {
        v2_1_0::AuthType::Bearer => {
            let token = attr(&auth.bearer, "token").unwrap_or_else(|| "{{token}}".to_string());
            Some(("Authorization".to_string(), format!("Bearer {token}")))
        }
        v2_1_0::AuthType::Basic => {
            let user = attr(&auth.basic, "username").unwrap_or_default();
            let pass = attr(&auth.basic, "password").unwrap_or_default();
            Some(("Authorization".to_string(), format!("Basic {user}:{pass}")))
        }
        v2_1_0::AuthType::Apikey => {
            let key = attr(&auth.api_key, "key").unwrap_or_else(|| "X-API-Key".to_string());
            let value = attr(&auth.api_key, "value").unwrap_or_else(|| "{{apiKey}}".to_string());
            Some((key, value))
        }
        _ => None,
    }
}

// ---- v2.0 (same shape, no auth mapping) ----

fn map_v2_0(spec: &v2_0_0::Spec) -> Vec<MappedContract> {
    let mut out = Vec::new();
    walk_v2_0(&spec.item, Path::new(""), &mut out);
    out
}

fn walk_v2_0(items: &[v2_0_0::Items], dir: &Path, out: &mut Vec<MappedContract>) {
    let mut taken = HashSet::new();
    for item in items {
        match item {
            v2_0_0::Items::ItemGroup(group) => {
                let name = group.name.as_deref().unwrap_or("folder");
                let slug = unique_slug(&mut taken, &slugify(name));
                let child_dir = dir.join(&slug);
                walk_v2_0(&group.item, &child_dir, out);
            }
            v2_0_0::Items::Item(it) => {
                if let Some(raw) = raw_request_v2_0(it) {
                    let slug = unique_slug(&mut taken, &slugify(&raw.name));
                    out.push(MappedContract {
                        rel_path: dir.join(format!("{slug}.json")),
                        contract: build_contract(raw),
                    });
                }
            }
        }
    }
}

fn raw_request_v2_0(item: &v2_0_0::Item) -> Option<RawRequest> {
    let req = match &item.request {
        v2_0_0::RequestUnion::RequestClass(req) => req,
        v2_0_0::RequestUnion::String(_) => return None,
    };

    let name = item.name.clone().unwrap_or_else(|| "request".to_string());
    let description = item.description.as_ref().and_then(description_text_v2_0);
    let method = req.method.clone().unwrap_or_else(|| "GET".to_string());
    let raw_url = req.url.as_ref().map(url_raw_v2_0).unwrap_or_default();

    let headers = match &req.header {
        Some(v2_0_0::HeaderUnion::HeaderArray(list)) => list
            .iter()
            .filter(|h| !h.disabled.unwrap_or(false))
            .map(|h| (h.key.clone(), h.value.clone()))
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };

    let body = req.body.as_ref().and_then(|b| b.raw.clone());

    let responses = item
        .response
        .as_ref()
        .map(|rs| {
            rs.iter()
                .map(|r| {
                    let code = r.code.unwrap_or(0) as u16;
                    let status = r.status.clone().unwrap_or_else(|| code.to_string());
                    (code, status, r.body.clone())
                })
                .collect()
        })
        .unwrap_or_default();

    Some(RawRequest {
        name,
        description,
        method,
        raw_url,
        headers,
        body,
        responses,
    })
}

fn description_text_v2_0(d: &v2_0_0::DescriptionUnion) -> Option<String> {
    match d {
        v2_0_0::DescriptionUnion::String(s) => Some(s.clone()),
        v2_0_0::DescriptionUnion::Description(desc) => desc.content.clone(),
    }
}

fn url_raw_v2_0(url: &v2_0_0::Url) -> String {
    match url {
        v2_0_0::Url::String(s) => s.clone(),
        v2_0_0::Url::UrlClass(u) => u.raw.clone().unwrap_or_default(),
    }
}

// ---- v1.0.0 (flat requests + folder-by-id reconstruction, no auth) ----

fn map_v1(spec: &v1_0_0::Spec) -> Vec<MappedContract> {
    use std::collections::HashMap;

    // Index requests by id.
    let by_id: HashMap<&str, &v1_0_0::Request> =
        spec.requests.iter().map(|r| (r.id.as_str(), r)).collect();

    let mut out = Vec::new();
    let mut placed: HashSet<&str> = HashSet::new();

    // Folders → directories; their `order` lists request ids.
    if let Some(folders) = &spec.folders {
        for folder in folders {
            let dir = PathBuf::from(slugify(&folder.name));
            let mut taken = HashSet::new();
            for id in &folder.order {
                if let Some(req) = by_id.get(id.as_str()) {
                    placed.insert(id.as_str());
                    push_v1_request(req, &dir, &mut taken, &mut out);
                }
            }
        }
    }

    // Unfoldered requests at the root.
    let mut root_taken = HashSet::new();
    for req in &spec.requests {
        if !placed.contains(req.id.as_str()) {
            push_v1_request(req, Path::new(""), &mut root_taken, &mut out);
        }
    }

    out
}

fn push_v1_request(
    req: &v1_0_0::Request,
    dir: &Path,
    taken: &mut HashSet<String>,
    out: &mut Vec<MappedContract>,
) {
    let raw = raw_request_v1(req);
    let slug = unique_slug(taken, &slugify(&raw.name));
    out.push(MappedContract {
        rel_path: dir.join(format!("{slug}.json")),
        contract: build_contract(raw),
    });
}

fn raw_request_v1(req: &v1_0_0::Request) -> RawRequest {
    // v1 stores the complete URL as a plain string.
    let raw_url = req.url.clone();
    // v1 raw body is `Option<RawModeData>`; take the string variant.
    let body = match &req.raw_mode_data {
        Some(v1_0_0::RawModeData::String(s)) => Some(s.clone()),
        _ => None,
    };
    RawRequest {
        name: req.name.clone(),
        description: req.description.clone(),
        method: req.method.clone(),
        raw_url,
        headers: parse_v1_headers(&req.headers),
        body,
        responses: Vec::new(),
    }
}

/// v1 stores headers as a single newline-separated `"Key: Value"` string.
fn parse_v1_headers(headers: &str) -> Vec<(String, String)> {
    headers
        .lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

/// Map any supported collection version to contracts.
fn map(collection: &PostmanCollection) -> Vec<MappedContract> {
    match collection {
        PostmanCollection::V1_0_0(spec) => map_v1(spec),
        PostmanCollection::V2_0_0(spec) => map_v2_0(spec),
        PostmanCollection::V2_1_0(spec) => map_v2_1(spec),
    }
}

/// Write mapped contracts under `dest_base`. Each contract's `rel_path` is
/// confined under `dest_base` (rejecting `..` escapes), its parent directories
/// are created, and the pretty-printed JSON is written. Existing files are not
/// overwritten. Returns the number of files written.
fn write_contracts(dest_base: &Path, mapped: &[MappedContract]) -> Result<usize, String> {
    let mut written = 0usize;
    for item in mapped {
        let path = confine_to_dir(dest_base, &item.rel_path)?;
        if path.exists() {
            return Err(format!(
                "{} already exists; refusing to overwrite",
                path.display()
            ));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(&item.contract)
            .map_err(|err| format!("failed to serialize contract: {err}"))?;
        fs::write(&path, json)
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
        written += 1;
    }
    Ok(written)
}

/// Run `apic convert`: parse the collection at `collection_path`, map it, and
/// write contracts under `dest_base`.
pub fn run(collection_path: &Path, dest_base: &Path) -> Result<(), String> {
    let collection = converter::from_path(collection_path)?;
    let mapped = map(&collection);
    if mapped.is_empty() {
        return Err("collection contained no convertible requests".to_string());
    }
    let count = write_contracts(dest_base, &mapped)?;
    println!("Converted {count} contract(s) into {}", dest_base.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_basic() {
        assert_eq!(slugify("Get User By ID"), "get_user_by_id");
        assert_eq!(slugify("create/login!!"), "create_login");
        assert_eq!(slugify("  spaced  "), "spaced");
        assert_eq!(slugify(""), "unnamed");
        assert_eq!(slugify("***"), "unnamed");
    }

    #[test]
    fn slug_unique_appends_suffix() {
        let mut taken = HashSet::new();
        assert_eq!(unique_slug(&mut taken, "user"), "user");
        assert_eq!(unique_slug(&mut taken, "user"), "user_2");
        assert_eq!(unique_slug(&mut taken, "user"), "user_3");
        assert_eq!(unique_slug(&mut taken, "auth"), "auth");
    }

    #[test]
    fn url_full() {
        let u = split_raw_url("https://api.example.com/v1/users/:id?limit=10&page=2");
        assert_eq!(u.protocol, "https");
        assert_eq!(u.host, "api.example.com");
        assert_eq!(
            u.path,
            Some(vec!["v1".into(), "users".into(), ":id".into()])
        );
        let q = u.query.unwrap();
        assert_eq!(q.len(), 2);
        assert_eq!(q[0].name, "limit");
        assert_eq!(q[0].value, "10");
        let vars = u.variable.unwrap();
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].name, "id");
    }

    #[test]
    fn url_template_host_no_scheme() {
        let u = split_raw_url("{{baseUrl}}/auth/login");
        assert_eq!(u.protocol, "");
        assert_eq!(u.host, "{{baseUrl}}");
        assert_eq!(u.path, Some(vec!["auth".into(), "login".into()]));
        assert!(u.query.is_none());
        assert!(u.variable.is_none());
    }

    #[test]
    fn url_host_only() {
        let u = split_raw_url("https://example.com");
        assert_eq!(u.host, "example.com");
        assert!(u.path.is_none());
    }

    #[test]
    fn build_maps_core_fields() {
        let raw = RawRequest {
            name: "Get User".into(),
            description: Some("fetch a user".into()),
            method: "get".into(),
            raw_url: "https://api.example.com/users/:id".into(),
            headers: vec![("Accept".into(), "application/json".into())],
            body: None,
            responses: vec![(200, "200 OK".into(), Some("{\"id\":1}".into()))],
        };
        let c = build_contract(raw);
        assert_eq!(c.name, "Get User");
        assert!(matches!(c.method, crate::json::Method::GET));
        assert_eq!(c.headers.len(), 1);
        assert_eq!(c.headers[0].name, "Accept");
        assert_eq!(c.responses.len(), 1);
        assert_eq!(c.responses[0].code, 200);
        assert!(c.responses[0].example.is_some());
        assert!(c.request.is_none());
    }

    #[test]
    fn build_body_parses_json_else_string() {
        assert_eq!(
            body_example(Some("{\"a\":1}")),
            Some(serde_json::json!({"a": 1}))
        );
        assert_eq!(
            body_example(Some("plain text")),
            Some(serde_json::Value::String("plain text".into()))
        );
        assert_eq!(body_example(Some("   ")), None);
        assert_eq!(body_example(None), None);
    }

    fn v2_1_collection(json: &str) -> v2_1_0::Spec {
        match crate::converter::from_slice(json.as_bytes()).unwrap() {
            PostmanCollection::V2_1_0(s) => s,
            other => panic!("expected v2.1, got {:?}", other.version()),
        }
    }

    #[test]
    fn v2_1_mirrors_folders_and_maps_request() {
        let json = r#"{
          "info": { "name": "X", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
          "item": [
            { "name": "users", "item": [
              { "name": "Get User",
                "request": { "method": "GET", "header": [],
                  "url": { "raw": "https://api.example.com/users/:id" } },
                "response": [] }
            ] }
          ]
        }"#;
        let mapped = map_v2_1(&v2_1_collection(json));
        assert_eq!(mapped.len(), 1);
        assert_eq!(
            mapped[0].rel_path,
            std::path::Path::new("users").join("get_user.json")
        );
        assert!(matches!(
            mapped[0].contract.method,
            crate::json::Method::GET
        ));
    }

    #[test]
    fn v2_1_bearer_auth_becomes_header() {
        let json = r#"{
          "info": { "name": "X", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
          "item": [
            { "name": "Me",
              "request": { "method": "GET",
                "auth": { "type": "bearer", "bearer": [ { "key": "token", "value": "abc123" } ] },
                "url": { "raw": "https://api.example.com/me" } },
              "response": [] }
          ]
        }"#;
        let mapped = map_v2_1(&v2_1_collection(json));
        let auth = mapped[0]
            .contract
            .headers
            .iter()
            .find(|h| h.name == "Authorization")
            .expect("authorization header");
        assert_eq!(auth.value, "Bearer abc123");
    }

    #[test]
    fn v1_folders_group_requests_by_id() {
        let json = r#"{
          "id": "col1", "name": "Legacy", "order": [],
          "folders": [ { "id": "f1", "name": "Users", "description": "", "order": ["r1"] } ],
          "requests": [
            { "id": "r1", "name": "List Users", "method": "GET", "headers": "",
              "url": "https://api.example.com/users", "collectionId": "col1" }
          ]
        }"#;
        let spec = match crate::converter::from_slice(json.as_bytes()).unwrap() {
            PostmanCollection::V1_0_0(s) => s,
            other => panic!("expected v1, got {:?}", other.version()),
        };
        let mapped = map_v1(&spec);
        assert_eq!(mapped.len(), 1);
        assert_eq!(
            mapped[0].rel_path,
            std::path::Path::new("users").join("list_users.json")
        );
    }

    #[test]
    fn write_creates_nested_files_and_refuses_overwrite() {
        let base = std::env::temp_dir().join("apic_convert_write_test");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        let mapped = vec![MappedContract {
            rel_path: std::path::Path::new("users").join("get_user.json"),
            contract: build_contract(RawRequest {
                name: "Get User".into(),
                description: None,
                method: "GET".into(),
                raw_url: "https://api.example.com/users/1".into(),
                headers: vec![],
                body: None,
                responses: vec![],
            }),
        }];

        let n = write_contracts(&base, &mapped).unwrap();
        assert_eq!(n, 1);
        assert!(base.join("users").join("get_user.json").is_file());

        // Second write to the same path is refused.
        let err = write_contracts(&base, &mapped).unwrap_err();
        assert!(err.contains("already exists"));

        std::fs::remove_dir_all(&base).unwrap();
    }
}
