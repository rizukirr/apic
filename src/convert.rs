//! `apic convert` — import a Postman collection as apic contract files.

use std::collections::HashSet;

use crate::json::{Query, Url, Variable};

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
}
