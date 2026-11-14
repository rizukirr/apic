//! Sending a contract's request over the network.
//!
//! [`prepare`] is pure: it fills variables, appends the query, sets the body
//! and refuses a request that cannot be sent. [`execute`] performs it with
//! `ureq`. A non-2xx reply is a normal [`Reply`], not an error.

use crate::file::confine_to_dir;
use crate::json::{JsonContent, Part, method_str};
use crate::vars::{Vars, fill_path_params, path_params, substitute};
use serde_json::Value;
use std::path::Path;
use std::time::{Duration, Instant};

/// The largest response body read into memory.
const BODY_LIMIT: u64 = 50 * 1024 * 1024;

/// A request ready to send.
#[derive(Debug, Clone)]
pub struct Prepared {
    /// The HTTP method, e.g. `POST`.
    pub method: String,
    /// The full URL with variables filled and the query appended.
    pub url: String,
    /// Request headers in contract order.
    pub headers: Vec<(String, String)>,
    /// The body bytes, `None` when the request has no body.
    pub body: Option<Vec<u8>>,
}

/// A response received from the server.
#[derive(Debug, Clone)]
pub struct Reply {
    /// The status code, e.g. 200.
    pub status: u16,
    /// The canonical reason phrase for the status, empty when unknown.
    pub reason: String,
    /// Response headers in received order.
    pub headers: Vec<(String, String)>,
    /// The raw body.
    pub body: Vec<u8>,
    /// Time from sending to the end of the body.
    pub elapsed: Duration,
}

/// Percent-encodes `s` for a URL query component, keeping RFC 3986
/// unreserved characters.
fn encode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Fills variables in every string inside `value`. Object keys stay as
/// written.
fn substitute_json(value: &Value, vars: &Vars) -> Value {
    match value {
        Value::String(s) => Value::String(substitute(s, vars).0),
        Value::Array(items) => {
            Value::Array(items.iter().map(|v| substitute_json(v, vars)).collect())
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), substitute_json(v, vars)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// A multipart boundary unlikely to occur in any part.
fn boundary() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("apic-boundary-{nanos:x}")
}

/// Strips CR, LF and `"` so a value cannot break out of a part header.
fn header_safe(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '\r' | '\n' | '"'))
        .collect()
}

/// Builds a `multipart/form-data` body. A file part reads the file at its
/// filled `value`, resolved against `project_root` and confined to it.
fn multipart_body(
    parts: &[Part],
    vars: &Vars,
    project_root: &Path,
    boundary: &str,
) -> Result<Vec<u8>, String> {
    let mut body: Vec<u8> = Vec::new();
    for part in parts {
        let value = substitute(&part.value, vars).0;
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        let name = header_safe(&part.name);
        match &part.filename {
            Some(filename) => {
                if value.trim().is_empty() {
                    return Err(format!(
                        "File part `{}` has no file path in its value",
                        part.name
                    ));
                }
                let path = confine_to_dir(project_root, &project_root.join(value.trim()))?;
                let bytes = std::fs::read(&path).map_err(|err| {
                    format!(
                        "File part `{}`: cannot read {}: {}",
                        part.name,
                        path.display(),
                        err
                    )
                })?;
                let content_type = part
                    .content_type
                    .as_deref()
                    .unwrap_or("application/octet-stream");
                body.extend_from_slice(
                    format!(
                        "Content-Disposition: form-data; name=\"{name}\"; filename=\"{}\"\r\nContent-Type: {}\r\n\r\n",
                        header_safe(filename),
                        header_safe(content_type)
                    )
                    .as_bytes(),
                );
                body.extend_from_slice(&bytes);
            }
            None => {
                body.extend_from_slice(
                    format!("Content-Disposition: form-data; name=\"{name}\"\r\n").as_bytes(),
                );
                if let Some(ct) = &part.content_type {
                    body.extend_from_slice(
                        format!("Content-Type: {}\r\n", header_safe(ct)).as_bytes(),
                    );
                }
                body.extend_from_slice(b"\r\n");
                body.extend_from_slice(value.as_bytes());
            }
        }
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    Ok(body)
}

/// The contract's URL with `$name` variables and `{name}` path params filled
/// from `vars`.
fn fill_url(contract: &JsonContent, vars: &Vars) -> String {
    fill_path_params(&substitute(&contract.url, vars).0, vars)
}

/// Appends every query entry to `url` as `name=value`, filled from `vars` and
/// percent-encoded.
fn append_query(url: &mut String, contract: &JsonContent, vars: &Vars) {
    for q in &contract.query {
        url.push(if url.contains('?') { '&' } else { '?' });
        url.push_str(&encode_component(&substitute(&q.name, vars).0));
        url.push('=');
        url.push_str(&encode_component(&substitute(&q.value, vars).0));
    }
}

/// The contract's headers with values filled from `vars`, plus the
/// `Content-Type` the body needs: `application/json` for a JSON body unless
/// the contract sets one, or `multipart_type` for a multipart body, replacing
/// any the contract sets.
fn build_headers(
    contract: &JsonContent,
    vars: &Vars,
    multipart_type: &str,
) -> Vec<(String, String)> {
    let mut headers: Vec<(String, String)> = contract
        .headers
        .iter()
        .map(|h| (h.name.clone(), substitute(&h.value, vars).0))
        .collect();
    let is_content_type = |h: &(String, String)| h.0.eq_ignore_ascii_case("content-type");
    if contract.request.is_some() {
        if !headers.iter().any(is_content_type) {
            headers.push(("Content-Type".into(), "application/json".into()));
        }
    } else if contract.multipart.is_some() {
        headers.retain(|h| !is_content_type(h));
        headers.push(("Content-Type".into(), multipart_type.to_string()));
    }
    headers
}

/// Builds the request for `contract` with `vars`, reading file parts under
/// `project_root`. Refuses a URL with an unfilled `{name}` path param, a URL
/// that is not `http://` or `https://`, and an unreadable file part.
pub fn prepare(
    contract: &JsonContent,
    vars: &Vars,
    project_root: &Path,
) -> Result<Prepared, String> {
    let mut url = fill_url(contract, vars);
    if let Some(name) = path_params(&url).first() {
        return Err(format!(
            "The URL needs a value for {{{name}}}: set `{name}` in the environment or pass --var {name}=VALUE"
        ));
    }
    let lower = url.to_ascii_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return Err(format!(
            "The URL must start with http:// or https:// after filling variables: {}",
            contract.url
        ));
    }
    append_query(&mut url, contract, vars);

    let boundary = boundary();
    let headers = build_headers(
        contract,
        vars,
        &format!("multipart/form-data; boundary={boundary}"),
    );
    let body = if let Some(json) = &contract.request {
        Some(
            serde_json::to_vec(&substitute_json(json, vars))
                .map_err(|err| format!("Failed to encode the request body: {err}"))?,
        )
    } else if let Some(parts) = &contract.multipart {
        Some(multipart_body(parts, vars, project_root, &boundary)?)
    } else {
        None
    };

    Ok(Prepared {
        method: method_str(&contract.method),
        url,
        headers,
        body,
    })
}

/// What [`prepare`] would build, with every defined variable's value shown as
/// `$name`. Made by [`preview`].
#[derive(Debug)]
pub struct Preview {
    /// The HTTP method, e.g. `POST`.
    pub method: String,
    /// The real scheme, host and port, without any `user:pass@`.
    pub target: String,
    /// The URL with the query appended, values redacted.
    pub url: String,
    /// Headers in the order [`prepare`] builds them, values redacted except a
    /// `Content-Type` that [`prepare`] adds itself.
    pub headers: Vec<(String, String)>,
    /// The body, redacted.
    pub body: PreviewBody,
    /// Variable names used but not defined, each once, in first-seen order.
    pub undefined: Vec<String>,
}

/// The body of a [`Preview`].
#[derive(Debug)]
pub enum PreviewBody {
    /// No body.
    None,
    /// A JSON body, redacted.
    Json(Value),
    /// A multipart body, one entry per part.
    Parts(Vec<PartPreview>),
}

/// One multipart part of a [`Preview`]. A text part sets `value`. A file part
/// sets `file`, `filename` and `size`, never the file's bytes.
#[derive(Debug)]
pub struct PartPreview {
    pub name: String,
    /// A text part's value, redacted.
    pub value: Option<String>,
    /// A file part's path, redacted.
    pub file: Option<String>,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    /// A file part's size in bytes.
    pub size: Option<u64>,
}

/// `scheme://host[:port]` of `url`, without any `user:pass@`.
fn origin(url: &str) -> String {
    let Some(scheme_end) = url.find("://") else {
        return String::new();
    };
    let rest = &url[scheme_end + 3..];
    let authority = &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())];
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    format!("{}{host}", &url[..scheme_end + 3])
}

/// Pushes each name in `names` onto `out` unless it is already there.
fn note(out: &mut Vec<String>, names: Vec<String>) {
    for name in names {
        if !out.contains(&name) {
            out.push(name);
        }
    }
}

/// Notes the undefined names in every string inside `value`, in order.
fn json_undefined(value: &Value, vars: &Vars, out: &mut Vec<String>) {
    match value {
        Value::String(s) => note(out, substitute(s, vars).1),
        Value::Array(items) => items.iter().for_each(|v| json_undefined(v, vars, out)),
        Value::Object(map) => map.values().for_each(|v| json_undefined(v, vars, out)),
        _ => {}
    }
}

/// Describes the request [`prepare`] builds for `contract`, refusing exactly
/// where it does, with every variable defined in `vars` shown as `$name`
/// instead of its value. `target` is the real scheme, host and port. File
/// parts report their size, never their bytes.
pub fn preview(
    contract: &JsonContent,
    vars: &Vars,
    project_root: &Path,
) -> Result<Preview, String> {
    let prepared = prepare(contract, vars, project_root)?;
    let masked: Vars = vars.keys().map(|k| (k.clone(), format!("${k}"))).collect();

    let base = fill_url(contract, &masked);
    let mut url = base.clone();
    append_query(&mut url, contract, &masked);
    // Redacted names read `$name` in the query, not `%24name`.
    let url = format!("{base}{}", url[base.len()..].replace("%24", "$"));

    let multipart_type = prepared
        .headers
        .iter()
        .rev()
        .find(|(n, _)| n.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    let headers = build_headers(contract, &masked, &multipart_type);

    let mut undefined = Vec::new();
    note(&mut undefined, substitute(&contract.url, vars).1);
    for q in &contract.query {
        note(&mut undefined, substitute(&q.name, vars).1);
        note(&mut undefined, substitute(&q.value, vars).1);
    }
    for h in &contract.headers {
        note(&mut undefined, substitute(&h.value, vars).1);
    }

    let body = if let Some(json) = &contract.request {
        json_undefined(json, vars, &mut undefined);
        PreviewBody::Json(substitute_json(json, &masked))
    } else if let Some(parts) = &contract.multipart {
        let mut out = Vec::new();
        for part in parts {
            let (real, unknown) = substitute(&part.value, vars);
            note(&mut undefined, unknown);
            let shown = substitute(&part.value, &masked).0;
            out.push(match &part.filename {
                Some(filename) => {
                    let path = confine_to_dir(project_root, &project_root.join(real.trim()))?;
                    let size = std::fs::metadata(&path)
                        .map_err(|err| {
                            format!(
                                "File part `{}`: cannot read {}: {}",
                                part.name,
                                path.display(),
                                err
                            )
                        })?
                        .len();
                    PartPreview {
                        name: part.name.clone(),
                        value: None,
                        file: Some(shown),
                        filename: Some(filename.clone()),
                        content_type: part.content_type.clone(),
                        size: Some(size),
                    }
                }
                None => PartPreview {
                    name: part.name.clone(),
                    value: Some(shown),
                    file: None,
                    filename: None,
                    content_type: part.content_type.clone(),
                    size: None,
                },
            });
        }
        PreviewBody::Parts(out)
    } else {
        PreviewBody::None
    };

    Ok(Preview {
        method: prepared.method,
        target: origin(&prepared.url),
        url,
        headers,
        body,
        undefined,
    })
}

/// Sends `prepared`, following up to 5 redirects and giving up after
/// `timeout`. Any HTTP status is a [`Reply`]; a connection failure or a
/// timeout is an error.
pub fn execute(prepared: &Prepared, timeout: Duration) -> Result<Reply, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(timeout))
        .max_redirects(5)
        .build()
        .into();

    let mut builder = ureq::http::Request::builder()
        .method(prepared.method.as_str())
        .uri(prepared.url.as_str());
    for (name, value) in &prepared.headers {
        builder = builder.header(name.as_str(), value.as_str());
    }

    let start = Instant::now();
    let result = match &prepared.body {
        Some(bytes) => builder
            .body(bytes.clone())
            .map_err(|err| format!("Invalid request: {err}"))
            .and_then(|req| agent.run(req).map_err(|err| err.to_string())),
        None => builder
            .body(())
            .map_err(|err| format!("Invalid request: {err}"))
            .and_then(|req| agent.run(req).map_err(|err| err.to_string())),
    };
    let mut response = result?;

    let status = response.status();
    let headers = response
        .headers()
        .iter()
        .map(|(n, v)| {
            (
                n.as_str().to_string(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    let body = response
        .body_mut()
        .with_config()
        .limit(BODY_LIMIT)
        .read_to_vec()
        .map_err(|err| format!("Failed to read the response body: {err}"))?;

    Ok(Reply {
        status: status.as_u16(),
        reason: status.canonical_reason().unwrap_or("").to_string(),
        headers,
        body,
        elapsed: start.elapsed(),
    })
}

/// [`prepare`] then [`execute`].
pub fn send(
    contract: &JsonContent,
    vars: &Vars,
    project_root: &Path,
    timeout: Duration,
) -> Result<Reply, String> {
    execute(&prepare(contract, vars, project_root)?, timeout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc::{Receiver, channel};

    fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
        hay.windows(needle.len()).position(|w| w == needle)
    }

    /// Serves one request on 127.0.0.1 with the raw response `reply` and hands
    /// back the raw request bytes.
    fn serve_once(reply: &'static str) -> (String, Receiver<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let n = stream.read(&mut chunk).unwrap();
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                if let Some(end) = find(&buf, b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&buf[..end]).to_ascii_lowercase();
                    let len = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .map_or(0, |v| v.trim().parse::<usize>().unwrap());
                    if buf.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            stream.write_all(reply.as_bytes()).unwrap();
            tx.send(buf).unwrap();
        });
        (format!("http://{addr}"), rx)
    }

    fn contract(json: &str) -> JsonContent {
        serde_json::from_str(json).unwrap()
    }

    fn vars(pairs: &[(&str, &str)]) -> Vars {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    const OK_JSON: &str = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}";

    #[test]
    fn sends_method_path_query_headers_and_json_body() {
        let (base, rx) = serve_once(OK_JSON);
        let c = contract(
            r#"{"name":"n","method":"POST","url":"$base/users/{id}",
                "query":[{"name":"q","value":"a b"}],
                "headers":[{"name":"Authorization","value":"Bearer $token"}],
                "request":{"user":"$user","n":1},"responses":[]}"#,
        );
        let v = vars(&[
            ("base", &base),
            ("id", "7"),
            ("token", "T"),
            ("user", "ann"),
        ]);
        let reply = send(&c, &v, Path::new("."), Duration::from_secs(5)).unwrap();
        assert_eq!(reply.status, 200);
        assert_eq!(reply.body, b"{\"ok\":true}");

        let raw = String::from_utf8(rx.recv().unwrap()).unwrap();
        assert!(
            raw.starts_with("POST /users/7?q=a%20b HTTP/1.1\r\n"),
            "{raw}"
        );
        let lower = raw.to_ascii_lowercase();
        assert!(lower.contains("authorization: bearer t\r\n"), "{raw}");
        assert!(
            lower.contains("content-type: application/json\r\n"),
            "{raw}"
        );
        assert!(raw.ends_with("{\"user\":\"ann\",\"n\":1}"), "{raw}");
    }

    #[test]
    fn non_2xx_is_a_normal_reply() {
        let (base, _rx) = serve_once(
            "HTTP/1.1 404 Not Found\r\nContent-Length: 4\r\nConnection: close\r\n\r\nnope",
        );
        let c = contract(&format!(
            r#"{{"name":"n","method":"GET","url":"{base}/x","headers":[],"responses":[]}}"#
        ));
        let reply = send(&c, &Vars::new(), Path::new("."), Duration::from_secs(5)).unwrap();
        assert_eq!(reply.status, 404);
        assert_eq!(reply.reason, "Not Found");
        assert_eq!(reply.body, b"nope");
    }

    #[test]
    fn multipart_sends_text_and_file_parts() {
        let root = std::env::temp_dir().join(format!("apic_http_mp_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.txt"), "FILEDATA").unwrap();
        let (base, rx) = serve_once(OK_JSON);
        let c = contract(&format!(
            r#"{{"name":"n","method":"POST","url":"{base}/up","headers":[],
                "multipart":[{{"name":"title","value":"$t"}},
                             {{"name":"doc","value":"a.txt","filename":"a.txt","contentType":"text/plain"}}],
                "responses":[]}}"#
        ));
        send(&c, &vars(&[("t", "hello")]), &root, Duration::from_secs(5)).unwrap();
        let raw = String::from_utf8(rx.recv().unwrap()).unwrap();
        assert!(
            raw.to_ascii_lowercase()
                .contains("content-type: multipart/form-data; boundary="),
            "{raw}"
        );
        assert!(raw.contains("name=\"title\"\r\n\r\nhello\r\n"), "{raw}");
        assert!(
            raw.contains("filename=\"a.txt\"\r\nContent-Type: text/plain\r\n\r\nFILEDATA\r\n"),
            "{raw}"
        );
    }

    #[test]
    fn refuses_unfilled_path_param_and_bad_scheme_and_escaping_file() {
        let c = contract(
            r#"{"name":"n","method":"GET","url":"http://h/u/{id}","headers":[],"responses":[]}"#,
        );
        let err = prepare(&c, &Vars::new(), Path::new(".")).unwrap_err();
        assert!(err.contains("--var id="), "{err}");

        let c =
            contract(r#"{"name":"n","method":"GET","url":"$base/u","headers":[],"responses":[]}"#);
        assert!(prepare(&c, &Vars::new(), Path::new(".")).is_err());

        let root = std::env::temp_dir().join(format!("apic_http_esc_{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let c = contract(
            r#"{"name":"n","method":"POST","url":"http://h/","headers":[],
                "multipart":[{"name":"f","value":"../../etc/passwd","filename":"p"}],"responses":[]}"#,
        );
        assert!(prepare(&c, &Vars::new(), &root).is_err());
    }

    #[test]
    fn json_content_type_from_contract_is_kept() {
        let c = contract(
            r#"{"name":"n","method":"POST","url":"http://h/","headers":[{"name":"content-type","value":"application/vnd+json"}],
                "request":{"a":1},"responses":[]}"#,
        );
        let p = prepare(&c, &Vars::new(), Path::new(".")).unwrap();
        let types: Vec<&str> = p
            .headers
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case("content-type"))
            .map(|(_, v)| v.as_str())
            .collect();
        assert_eq!(types, vec!["application/vnd+json"]);
    }

    #[test]
    fn preview_redacts_every_defined_variable() {
        let c = contract(
            r#"{"name":"n","method":"POST","url":"$base/u/{id}",
                "query":[{"name":"k","value":"$key"}],
                "headers":[{"name":"Authorization","value":"Bearer $token"}],
                "request":{"secret":"$token","n":1},"responses":[]}"#,
        );
        let v = vars(&[
            ("base", "https://u:pw@api.example.com:8443/v1"),
            ("id", "42"),
            ("key", "s3cret"),
            ("token", "s3cret"),
        ]);
        let p = preview(&c, &v, Path::new(".")).unwrap();
        let shown = format!("{p:?}");
        assert!(!shown.contains("s3cret"), "{shown}");
        assert!(!shown.contains("u:pw@"), "{shown}");
        assert_eq!(p.method, "POST");
        assert_eq!(p.target, "https://api.example.com:8443");
        assert_eq!(p.url, "$base/u/$id?k=$key");
        assert_eq!(
            p.headers,
            vec![
                ("Authorization".to_string(), "Bearer $token".to_string()),
                ("Content-Type".to_string(), "application/json".to_string()),
            ]
        );
        match &p.body {
            PreviewBody::Json(body) => {
                assert_eq!(body, &serde_json::json!({"secret": "$token", "n": 1}))
            }
            other => panic!("{other:?}"),
        }
        assert!(p.undefined.is_empty());
    }

    #[test]
    fn preview_headers_match_prepare_names_and_order() {
        let names = |h: &[(String, String)]| h.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>();
        let v = vars(&[("a", "1")]);

        let c = contract(
            r#"{"name":"n","method":"POST","url":"http://h/",
                "headers":[{"name":"X-A","value":"$a"}],"request":{},"responses":[]}"#,
        );
        let real = prepare(&c, &v, Path::new(".")).unwrap();
        let p = preview(&c, &v, Path::new(".")).unwrap();
        assert_eq!(names(&p.headers), names(&real.headers));
        assert_eq!(p.headers[1].1, "application/json");

        let c = contract(
            r#"{"name":"n","method":"POST","url":"http://h/",
                "headers":[{"name":"Content-Type","value":"text/plain"},{"name":"X-A","value":"$a"}],
                "multipart":[{"name":"t","value":"x"}],"responses":[]}"#,
        );
        let real = prepare(&c, &v, Path::new(".")).unwrap();
        let p = preview(&c, &v, Path::new(".")).unwrap();
        assert_eq!(names(&p.headers), names(&real.headers));
        assert!(
            p.headers
                .last()
                .unwrap()
                .1
                .starts_with("multipart/form-data; boundary="),
            "{:?}",
            p.headers
        );
    }

    #[test]
    fn preview_lists_parts_without_bytes() {
        let root = std::env::temp_dir().join(format!("apic_http_pv_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.txt"), "FILEDATA").unwrap();
        let c = contract(
            r#"{"name":"n","method":"POST","url":"http://h/up","headers":[],
                "multipart":[{"name":"title","value":"$t"},
                             {"name":"doc","value":"a.txt","filename":"a.txt","contentType":"text/plain"}],
                "responses":[]}"#,
        );
        let p = preview(&c, &vars(&[("t", "hello-secret")]), &root).unwrap();
        let shown = format!("{p:?}");
        assert!(!shown.contains("FILEDATA"), "{shown}");
        assert!(!shown.contains("hello-secret"), "{shown}");
        let PreviewBody::Parts(parts) = &p.body else {
            panic!("{shown}")
        };
        assert_eq!(parts[0].value.as_deref(), Some("$t"));
        assert_eq!(parts[1].file.as_deref(), Some("a.txt"));
        assert_eq!(parts[1].filename.as_deref(), Some("a.txt"));
        assert_eq!(parts[1].content_type.as_deref(), Some("text/plain"));
        assert_eq!(parts[1].size, Some(8));
    }

    #[test]
    fn preview_refuses_like_prepare() {
        let root = std::env::temp_dir().join(format!("apic_http_pvr_{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        for json in [
            r#"{"name":"n","method":"GET","url":"http://h/u/{id}","headers":[],"responses":[]}"#,
            r#"{"name":"n","method":"GET","url":"$base/u","headers":[],"responses":[]}"#,
            r#"{"name":"n","method":"POST","url":"http://h/","headers":[],
                "multipart":[{"name":"f","value":"missing.bin","filename":"m"}],"responses":[]}"#,
        ] {
            let c = contract(json);
            assert_eq!(
                preview(&c, &Vars::new(), &root).unwrap_err(),
                prepare(&c, &Vars::new(), &root).unwrap_err()
            );
        }
    }

    #[test]
    fn preview_lists_undefined_names_once_in_order() {
        let c = contract(
            r#"{"name":"n","method":"POST","url":"http://h/$a",
                "query":[{"name":"$b","value":"$c"}],
                "headers":[{"name":"X","value":"$d $a"}],
                "request":{"k":["$e"]},"responses":[]}"#,
        );
        let p = preview(&c, &Vars::new(), Path::new(".")).unwrap();
        assert_eq!(p.undefined, vec!["a", "b", "c", "d", "e"]);

        let c = contract(
            r#"{"name":"n","method":"POST","url":"http://h/","headers":[],
                "multipart":[{"name":"p","value":"$f"}],"responses":[]}"#,
        );
        let p = preview(&c, &Vars::new(), Path::new(".")).unwrap();
        assert_eq!(p.undefined, vec!["f"]);
    }
}
