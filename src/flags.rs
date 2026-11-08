//! Curl-style authoring flags for `apic create` and `apic edit`.
//!
//! [`AuthorFlags`] holds the flags as parsed by clap, and [`apply_flags`]
//! turns them into [`EditAction`]s over an [`EditModel`], so the CLI changes a
//! contract the same way the GUI does.

use std::io::Read;
use std::path::Path;

use apic_core::edit::{BodyLoc, EditAction, EditBody, EditHeader, EditModel, Field, apply};
use apic_core::json::{method_all, method_str};

/// The authoring flags a `create` or `edit` run was given. Every field is
/// optional, and an absent or empty one leaves its part of the contract alone.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct AuthorFlags {
    /// `-X METHOD`.
    pub(crate) method: Option<String>,
    /// The bare URL argument, with an optional `?a=1&b=2` query string.
    pub(crate) url: Option<String>,
    /// Each `-H 'Name: value'`, in order.
    pub(crate) headers: Vec<String>,
    /// `-d TEXT`, `-d @file` or `-d @-`.
    pub(crate) data: Option<String>,
    /// Each `-F name=value`, `-F name=@file` or `-F 'name=@file;type=mime'`.
    pub(crate) form: Vec<String>,
    /// Each `--response CODE:DESC`.
    pub(crate) responses: Vec<String>,
    /// Each `--response-body CODE BODY`, as `(code, body)`.
    pub(crate) response_bodies: Vec<(String, String)>,
    /// Each `--response-header CODE 'Name: value'`, as `(code, header)`.
    pub(crate) response_headers: Vec<(String, String)>,
    /// `--name`.
    pub(crate) name: Option<String>,
    /// `--description`.
    pub(crate) description: Option<String>,
}

/// Applies `flags` to `model` in a fixed order: name, description, method,
/// URL, headers, data, form, responses, response bodies, response headers.
///
/// # Arguments
///
/// * `model` - the contract under edit
/// * `flags` - the parsed authoring flags
/// * `stdin` - read for a `-d @-` or `--response-body CODE @-` value
///
/// # Returns
///
/// `Ok(())` once every flag is applied. The model is not validated here.
///
/// # Errors
///
/// Returns a message naming the flag for a malformed header, form field,
/// response code or method, a body that is not JSON or cannot be read, a JSON
/// body on a multipart body or the other way round, and a response body or
/// header for a code the contract has no response for. The model may be
/// partly changed on error, so a caller writes nothing then.
pub(crate) fn apply_flags(
    model: &mut EditModel,
    flags: &AuthorFlags,
    stdin: &mut dyn Read,
) -> Result<(), String> {
    if let Some(name) = &flags.name {
        set(model, Field::Name, name);
    }
    if let Some(description) = &flags.description {
        set(model, Field::Description, description);
    }
    if let Some(method) = &flags.method {
        set_method(model, method)?;
    }
    if let Some(url) = &flags.url {
        set_url(model, url);
    }
    for header in &flags.headers {
        upsert_header(model, &BodyLoc::Request, header, "-H")?;
    }
    if let Some(data) = &flags.data {
        set_request_body(model, data, stdin)?;
    }
    for form in &flags.form {
        upsert_part(model, form)?;
    }
    for response in &flags.responses {
        upsert_response(model, response)?;
    }
    for (code, body) in &flags.response_bodies {
        set_response_body(model, code, body, stdin)?;
    }
    for (code, header) in &flags.response_headers {
        let r = response_index(model, code)?;
        upsert_header(model, &BodyLoc::Response(r), header, "--response-header")?;
    }
    Ok(())
}

fn set(model: &mut EditModel, field: Field, value: &str) {
    apply(
        model,
        &EditAction::SetText {
            field,
            value: value.to_string(),
        },
    );
}

/// Sets the method by name, ignoring case. `apply` only cycles the method, so
/// the model is written directly.
fn set_method(model: &mut EditModel, method: &str) -> Result<(), String> {
    let wanted = method.trim();
    model.method = method_all()
        .into_iter()
        .find(|m| method_str(m).eq_ignore_ascii_case(wanted))
        .ok_or_else(|| format!("`-X` got unknown method `{method}`"))?;
    Ok(())
}

/// Sets the URL path and upserts each `k=v` of its query string into the
/// query params by name.
fn set_url(model: &mut EditModel, url: &str) {
    let (path, query) = url.split_once('?').unwrap_or((url, ""));
    set(model, Field::Url, path);
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        let i = match model.query.iter().position(|q| q.name == name) {
            Some(i) => i,
            None => {
                apply(
                    model,
                    &EditAction::Add {
                        field: Field::QueryAdd,
                    },
                );
                let i = model.query.len() - 1;
                set(model, Field::QueryName(i), name);
                i
            }
        };
        set(model, Field::QueryValue(i), value);
    }
}

/// The headers of the request (`BodyLoc::Request`) or of response `r`.
fn headers<'a>(model: &'a EditModel, loc: &BodyLoc) -> &'a [EditHeader] {
    match loc {
        BodyLoc::Request => &model.headers,
        BodyLoc::Response(r) => model
            .responses
            .get(*r)
            .map_or(&[], |resp| resp.headers.as_slice()),
    }
}

/// The name, value and add fields for header `i` at `loc`.
fn header_fields(loc: &BodyLoc, i: usize) -> (Field, Field, Field) {
    match loc {
        BodyLoc::Request => (
            Field::HeaderName(i),
            Field::HeaderValue(i),
            Field::HeaderAdd,
        ),
        BodyLoc::Response(r) => (
            Field::ResponseHeaderName(*r, i),
            Field::ResponseHeaderValue(*r, i),
            Field::ResponseHeaderAdd(*r),
        ),
    }
}

/// Applies one `Name: value` header at `loc`. An empty value removes every
/// header with that name, otherwise the first match is replaced or a new
/// header is appended. Names compare without case.
fn upsert_header(
    model: &mut EditModel,
    loc: &BodyLoc,
    header: &str,
    flag: &str,
) -> Result<(), String> {
    let (name, value) = header
        .split_once(':')
        .ok_or_else(|| format!("`{flag}` expects `Name: value`, got `{header}`"))?;
    let (name, value) = (name.trim(), value.trim());
    if name.is_empty() {
        return Err(format!("`{flag}` has an empty header name in `{header}`"));
    }
    let matches = |h: &EditHeader| h.name.eq_ignore_ascii_case(name);
    if value.is_empty() {
        while let Some(i) = headers(model, loc).iter().rposition(matches) {
            let (name_field, _, _) = header_fields(loc, i);
            apply(model, &EditAction::Delete { field: name_field });
        }
        return Ok(());
    }
    let i = match headers(model, loc).iter().position(matches) {
        Some(i) => i,
        None => {
            let (_, _, add) = header_fields(loc, 0);
            apply(model, &EditAction::Add { field: add });
            let i = headers(model, loc).len() - 1;
            set(model, header_fields(loc, i).0, name);
            i
        }
    };
    set(model, header_fields(loc, i).1, value);
    Ok(())
}

/// Resolves a `-d` style value: `@-` reads `stdin`, `@path` reads the file,
/// anything else is the literal text. The result must be JSON.
fn read_body(value: &str, stdin: &mut dyn Read, flag: &str) -> Result<String, String> {
    let text = match value.strip_prefix('@') {
        Some("-") => {
            let mut text = String::new();
            stdin
                .read_to_string(&mut text)
                .map_err(|err| format!("`{flag}` could not read stdin: {err}"))?;
            text
        }
        Some(path) => std::fs::read_to_string(path)
            .map_err(|err| format!("`{flag}` could not read `{path}`: {err}"))?,
        None => value.to_string(),
    };
    serde_json::from_str::<serde_json::Value>(&text)
        .map_err(|err| format!("`{flag}` body is not valid JSON: {err}"))?;
    Ok(text)
}

/// Sets the request's JSON example. No `Field` writes a body example through
/// `apply`, so the model is written directly.
fn set_request_body(model: &mut EditModel, data: &str, stdin: &mut dyn Read) -> Result<(), String> {
    if !model.multipart.is_empty() {
        return Err("request has multipart parts, edit it with `apic open`".to_string());
    }
    let example = read_body(data, stdin, "-d")?;
    model.request = Some(EditBody { example });
    Ok(())
}

/// Sets response `code`'s JSON example. No `Field` writes a body example
/// through `apply`, so the model is written directly.
fn set_response_body(
    model: &mut EditModel,
    code: &str,
    body: &str,
    stdin: &mut dyn Read,
) -> Result<(), String> {
    let r = response_index(model, code)?;
    if !model.responses[r].multipart.is_empty() {
        return Err(format!(
            "response {} has multipart parts, edit it with `apic open`",
            code.trim()
        ));
    }
    let example = read_body(body, stdin, "--response-body")?;
    model.responses[r].example = example;
    Ok(())
}

/// The index of the first response with `code`.
fn response_index(model: &EditModel, code: &str) -> Result<usize, String> {
    let code = code.trim();
    model
        .responses
        .iter()
        .position(|r| r.code.trim() == code)
        .ok_or_else(|| format!("no response {code}, add it with `--response {code}:...`"))
}

/// Applies one `--response CODE:DESC`: updates the first response with that
/// code, or appends a new one.
fn upsert_response(model: &mut EditModel, response: &str) -> Result<(), String> {
    let (code, description) = response
        .split_once(':')
        .ok_or_else(|| format!("`--response` expects `CODE:DESC`, got `{response}`"))?;
    let code = code.trim();
    if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!(
            "`--response` code must be three digits, got `{code}`"
        ));
    }
    let i = match response_index(model, code) {
        Ok(i) => i,
        Err(_) => {
            apply(
                model,
                &EditAction::Add {
                    field: Field::ResponseAdd,
                },
            );
            let i = model.responses.len() - 1;
            set(model, Field::ResponseCode(i), code);
            i
        }
    };
    set(model, Field::ResponseDesc(i), description.trim());
    Ok(())
}

/// Applies one `-F` field to the request's multipart parts, replacing the
/// part with the same name or appending one. A file part records the file's
/// name, never its contents.
fn upsert_part(model: &mut EditModel, form: &str) -> Result<(), String> {
    if model
        .request
        .as_ref()
        .is_some_and(|body| !body.example.trim().is_empty())
    {
        return Err("request has a JSON body, edit it with `apic open`".to_string());
    }
    let (name, value) = form
        .split_once('=')
        .ok_or_else(|| format!("`-F` expects `name=value` or `name=@file`, got `{form}`"))?;
    let name = name.trim();
    if name.is_empty() {
        return Err(format!("`-F` has an empty part name in `{form}`"));
    }
    let file = match value.strip_prefix('@') {
        Some(spec) => Some(parse_file_part(spec, form)?),
        None => None,
    };

    let loc = BodyLoc::Request;
    if model.request.is_some() {
        apply(
            model,
            &EditAction::SetBodyKind {
                loc: loc.clone(),
                multipart: true,
            },
        );
    }
    let i = match model.multipart.iter().position(|p| p.name == name) {
        Some(i) => i,
        None => {
            apply(
                model,
                &EditAction::Add {
                    field: Field::PartAdd(loc.clone()),
                },
            );
            let i = model.multipart.len() - 1;
            set(model, Field::PartName(loc.clone(), i), name);
            i
        }
    };
    let (text, filename, content_type) = match &file {
        Some((filename, content_type)) => ("", filename.as_str(), *content_type),
        None => (value, "", ""),
    };
    set(model, Field::PartValue(loc.clone(), i), text);
    set(model, Field::PartFilename(loc.clone(), i), filename);
    set(model, Field::PartContentType(loc, i), content_type);
    Ok(())
}

/// Splits a `path[;type=mime]` file spec into the file name and media type.
fn parse_file_part<'a>(spec: &'a str, form: &str) -> Result<(String, &'a str), String> {
    let (path, content_type) = match spec.split_once(';') {
        Some((path, rest)) => {
            let mime = rest
                .trim()
                .strip_prefix("type=")
                .ok_or_else(|| format!("`-F` expects `;type=mime` after the file, got `{form}`"))?;
            (path, mime.trim())
        }
        None => (spec, ""),
    };
    let filename = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("`-F` has no file name in `{form}`"))?;
    Ok((filename.to_string(), content_type))
}

#[cfg(test)]
mod tests {
    use super::*;
    use apic_core::json::json_get;
    use std::io::Cursor;
    use std::path::PathBuf;

    fn model() -> EditModel {
        let c = json_get(
            r#"{ "name":"t","method":"GET","url":"/x",
                 "query":[{"name":"page","value":"1"}],
                 "headers":[{"name":"Accept","value":"*/*"},
                            {"name":"X-Trace","value":"a"},
                            {"name":"x-trace","value":"b"}],
                 "request":{"a":1},
                 "responses":[{"code":200,"description":"ok"}] }"#,
            None,
        )
        .unwrap();
        EditModel::from_contract(c)
    }

    fn run(m: &mut EditModel, flags: &AuthorFlags) -> Result<(), String> {
        apply_flags(m, flags, &mut Cursor::new(Vec::new()))
    }

    fn temp_file(tag: &str, contents: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("apic_flags_{tag}_{}.json", std::process::id()));
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn header_adds_replaces_and_removes() {
        let mut m = model();
        let flags = AuthorFlags {
            headers: vec![
                "accept: application/json".into(),
                "Authorization: Bearer x:y".into(),
                "X-Trace:".into(),
            ],
            ..AuthorFlags::default()
        };
        run(&mut m, &flags).unwrap();
        let got: Vec<(&str, &str)> = m
            .headers
            .iter()
            .map(|h| (h.name.as_str(), h.value.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                ("Accept", "application/json"),
                ("Authorization", "Bearer x:y")
            ]
        );

        let bad = AuthorFlags {
            headers: vec!["NoColon".into()],
            ..AuthorFlags::default()
        };
        let err = run(&mut model(), &bad).unwrap_err();
        assert!(err.contains("-H"), "{err}");
    }

    #[test]
    fn url_query_string_becomes_query_params() {
        let mut m = model();
        let flags = AuthorFlags {
            url: Some("/users/{id}?page=2&limit=10&flag".into()),
            ..AuthorFlags::default()
        };
        run(&mut m, &flags).unwrap();
        assert_eq!(m.url, "/users/{id}");
        let got: Vec<(&str, &str)> = m
            .query
            .iter()
            .map(|q| (q.name.as_str(), q.value.as_str()))
            .collect();
        assert_eq!(got, [("page", "2"), ("limit", "10"), ("flag", "")]);

        let mut m = model();
        let flags = AuthorFlags {
            url: Some("/plain".into()),
            ..AuthorFlags::default()
        };
        run(&mut m, &flags).unwrap();
        assert_eq!(m.url, "/plain");
        assert_eq!(m.query.len(), 1);
    }

    #[test]
    fn data_reads_text_file_and_stdin() {
        let mut m = model();
        let flags = AuthorFlags {
            data: Some(r#"{"text":true}"#.into()),
            ..AuthorFlags::default()
        };
        run(&mut m, &flags).unwrap();
        assert_eq!(m.request.as_ref().unwrap().example, r#"{"text":true}"#);

        let path = temp_file("data", r#"{"file":true}"#);
        let flags = AuthorFlags {
            data: Some(format!("@{}", path.display())),
            ..AuthorFlags::default()
        };
        run(&mut m, &flags).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(m.request.as_ref().unwrap().example, r#"{"file":true}"#);

        let flags = AuthorFlags {
            data: Some("@-".into()),
            ..AuthorFlags::default()
        };
        apply_flags(
            &mut m,
            &flags,
            &mut Cursor::new(br#"{"stdin":true}"#.to_vec()),
        )
        .unwrap();
        assert_eq!(m.request.as_ref().unwrap().example, r#"{"stdin":true}"#);
        assert!(m.to_json().is_ok());

        let flags = AuthorFlags {
            data: Some("not json".into()),
            ..AuthorFlags::default()
        };
        assert!(run(&mut model(), &flags).unwrap_err().contains("JSON"));
    }

    #[test]
    fn form_parses_text_file_and_type() {
        let mut m = model();
        m.request = None;
        let flags = AuthorFlags {
            form: vec![
                "caption=hello".into(),
                "avatar=@/tmp/pics/me.png;type=image/png".into(),
                "doc=@notes.txt".into(),
                "caption=bye".into(),
            ],
            ..AuthorFlags::default()
        };
        run(&mut m, &flags).unwrap();
        assert!(m.request.is_none());
        assert_eq!(m.multipart.len(), 3);
        let caption = &m.multipart[0];
        assert_eq!(
            (caption.name.as_str(), caption.value.as_str()),
            ("caption", "bye")
        );
        assert!(caption.filename.is_empty());
        let avatar = &m.multipart[1];
        assert_eq!(avatar.name, "avatar");
        assert_eq!(avatar.filename, "me.png");
        assert_eq!(avatar.content_type, "image/png");
        assert!(avatar.value.is_empty());
        let doc = &m.multipart[2];
        assert_eq!(doc.filename, "notes.txt");
        assert!(doc.content_type.is_empty());
        assert!(m.to_json().is_ok());

        let mut blank = model();
        blank.request = Some(EditBody::empty());
        let flags = AuthorFlags {
            form: vec!["a=b".into()],
            ..AuthorFlags::default()
        };
        run(&mut blank, &flags).unwrap();
        assert!(blank.request.is_none());
        assert_eq!(blank.multipart.len(), 1);

        let bad = AuthorFlags {
            form: vec!["novalue".into()],
            ..AuthorFlags::default()
        };
        assert!(run(&mut m, &bad).unwrap_err().contains("-F"));
    }

    #[test]
    fn response_flags_add_update_and_set() {
        let mut m = model();
        let path = temp_file("response", r#"{"id":1}"#);
        let flags = AuthorFlags {
            responses: vec!["200: Fine".into(), "404:Not found".into()],
            response_bodies: vec![
                ("200".into(), format!("@{}", path.display())),
                ("404".into(), "@-".into()),
            ],
            response_headers: vec![
                ("200".into(), "X-Rate: 10".into()),
                ("200".into(), "x-rate: 20".into()),
                ("404".into(), "X-Gone: yes".into()),
                ("404".into(), "X-Gone:".into()),
            ],
            ..AuthorFlags::default()
        };
        apply_flags(
            &mut m,
            &flags,
            &mut Cursor::new(br#"{"error":"x"}"#.to_vec()),
        )
        .unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(m.responses.len(), 2);
        let ok = &m.responses[0];
        assert_eq!((ok.code.as_str(), ok.description.as_str()), ("200", "Fine"));
        assert_eq!(ok.example, r#"{"id":1}"#);
        assert_eq!(ok.headers.len(), 1);
        assert_eq!(
            (ok.headers[0].name.as_str(), ok.headers[0].value.as_str()),
            ("X-Rate", "20")
        );
        let missing = &m.responses[1];
        assert_eq!(
            (missing.code.as_str(), missing.description.as_str()),
            ("404", "Not found")
        );
        assert_eq!(missing.example, r#"{"error":"x"}"#);
        assert!(missing.headers.is_empty());
        assert!(m.to_json().is_ok());

        let bad_code = AuthorFlags {
            responses: vec!["20:x".into()],
            ..AuthorFlags::default()
        };
        assert!(
            run(&mut model(), &bad_code)
                .unwrap_err()
                .contains("three digits")
        );

        let no_response = AuthorFlags {
            response_headers: vec![("500".into(), "A: b".into())],
            ..AuthorFlags::default()
        };
        assert_eq!(
            run(&mut model(), &no_response).unwrap_err(),
            "no response 500, add it with `--response 500:...`"
        );
        let no_response = AuthorFlags {
            response_bodies: vec![("500".into(), "{}".into())],
            ..AuthorFlags::default()
        };
        assert!(
            run(&mut model(), &no_response)
                .unwrap_err()
                .starts_with("no response 500")
        );
    }

    #[test]
    fn body_kind_conflicts_are_reported() {
        let multipart = AuthorFlags {
            form: vec!["a=b".into()],
            ..AuthorFlags::default()
        };
        assert_eq!(
            run(&mut model(), &multipart).unwrap_err(),
            "request has a JSON body, edit it with `apic open`"
        );

        let mut m = model();
        m.request = None;
        run(&mut m, &multipart).unwrap();
        let data = AuthorFlags {
            data: Some("{}".into()),
            ..AuthorFlags::default()
        };
        assert_eq!(
            run(&mut m, &data).unwrap_err(),
            "request has multipart parts, edit it with `apic open`"
        );

        let mut m = model();
        apply(
            &mut m,
            &EditAction::Add {
                field: Field::PartAdd(BodyLoc::Response(0)),
            },
        );
        let body = AuthorFlags {
            response_bodies: vec![("200".into(), "{}".into())],
            ..AuthorFlags::default()
        };
        assert_eq!(
            run(&mut m, &body).unwrap_err(),
            "response 200 has multipart parts, edit it with `apic open`"
        );
    }

    #[test]
    fn name_description_and_method_are_set() {
        let mut m = model();
        let flags = AuthorFlags {
            name: Some("renamed".into()),
            description: Some("about".into()),
            method: Some("post".into()),
            ..AuthorFlags::default()
        };
        run(&mut m, &flags).unwrap();
        assert_eq!(m.name, "renamed");
        assert_eq!(m.description, "about");
        assert_eq!(method_str(&m.method), "POST");

        let bad = AuthorFlags {
            method: Some("FETCH".into()),
            ..AuthorFlags::default()
        };
        assert!(run(&mut model(), &bad).unwrap_err().contains("-X"));
    }
}
