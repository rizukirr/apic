//! `apic convert` — import a Postman collection as apic contract files.

use std::collections::HashSet;

use crate::json::{Header, JsonContent, Query, RequestBody, Response, Url, Variable, method_from_str};

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
}
