//! `apic check`: send contracts' requests and compare the live responses with
//! the documented ones.

use apic_core::http;
use apic_core::json::{Header, Response};
use serde_json::Value;

/// One way a live response differs from a documented one.
#[derive(Debug, PartialEq)]
pub(crate) struct Difference {
    /// Where: `$` paths for the body, `headers.<Name>`, or `status`.
    pub(crate) path: String,
    pub(crate) message: String,
}

fn diff(path: &str, message: impl Into<String>) -> Difference {
    Difference {
        path: path.to_string(),
        message: message.into(),
    }
}

/// The JSON type name of `value`. Integers and floats are both `number`.
fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Compares `live` against the shape of `example`. A `null` on either side
/// matches anything. Every example key must exist in the live object, extra
/// live keys are fine. Live array elements are compared against the
/// example's first element, and only the first failing element is reported.
/// Scalars compare by type only.
pub(crate) fn compare(example: &Value, live: &Value) -> Vec<Difference> {
    let mut out = Vec::new();
    walk(example, live, "$", &mut out);
    out
}

fn walk(example: &Value, live: &Value, path: &str, out: &mut Vec<Difference>) {
    match (example, live) {
        (Value::Null, _) | (_, Value::Null) => {}
        (Value::Object(ex), Value::Object(lv)) => {
            for (key, ex_value) in ex {
                let child = format!("{path}.{key}");
                match lv.get(key) {
                    Some(lv_value) => walk(ex_value, lv_value, &child, out),
                    None => out.push(diff(&child, "missing")),
                }
            }
        }
        (Value::Array(ex), Value::Array(lv)) => {
            let Some(first) = ex.first() else { return };
            for (i, item) in lv.iter().enumerate() {
                let mut found = Vec::new();
                walk(first, item, &format!("{path}[{i}]"), &mut found);
                if !found.is_empty() {
                    out.extend(found);
                    return;
                }
            }
        }
        _ if type_name(example) == type_name(live) => {}
        _ => out.push(diff(
            path,
            format!("expected {}, got {}", type_name(example), type_name(live)),
        )),
    }
}

/// Every `required` header in `documented` missing from `live`, compared by
/// name in any case. Values are not compared.
pub(crate) fn missing_headers(documented: &[Header], live: &[(String, String)]) -> Vec<Difference> {
    documented
        .iter()
        .filter(|h| {
            h.required
                && !live
                    .iter()
                    .any(|(name, _)| name.eq_ignore_ascii_case(&h.name))
        })
        .map(|h| diff(&format!("headers.{}", h.name), "missing"))
        .collect()
}

/// Differences between `reply` and one documented `response`. A response
/// with no example body, or a multipart one, accepts any body.
fn against(response: &Response, reply: &http::Reply) -> Vec<Difference> {
    let mut out = Vec::new();
    if response.multipart.is_none()
        && let Some(example) = &response.schema
    {
        match serde_json::from_slice::<Value>(&reply.body) {
            Ok(live) => out.extend(compare(example, &live)),
            Err(_) => out.push(diff("$", "body is not JSON")),
        }
    }
    out.extend(missing_headers(&response.headers, &reply.headers));
    out
}

/// Differences between `reply` and the closest documented response. Empty
/// when any response with the live status matches. The status must be
/// documented, and equal `expected` when given.
pub(crate) fn match_reply(
    responses: &[Response],
    reply: &http::Reply,
    expected: Option<u16>,
) -> Vec<Difference> {
    if let Some(want) = expected
        && want != reply.status
    {
        return vec![diff(
            "status",
            format!("expected {want}, got {}", reply.status),
        )];
    }
    let mut best: Option<Vec<Difference>> = None;
    for response in responses.iter().filter(|r| r.code == reply.status) {
        let found = against(response, reply);
        if found.is_empty() {
            return found;
        }
        if best.as_ref().is_none_or(|b| found.len() < b.len()) {
            best = Some(found);
        }
    }
    best.unwrap_or_else(|| {
        vec![diff(
            "status",
            format!("{} is not documented", reply.status),
        )]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;

    fn lines(found: Vec<Difference>) -> Vec<String> {
        found
            .into_iter()
            .map(|d| format!("{}: {}", d.path, d.message))
            .collect()
    }

    fn reply(status: u16, body: &[u8]) -> http::Reply {
        http::Reply {
            status,
            reason: String::new(),
            headers: Vec::new(),
            body: body.to_vec(),
            elapsed: Duration::ZERO,
        }
    }

    fn response(value: Value) -> Response {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn nulls_match_anything_on_either_side() {
        assert!(compare(&json!({"a": null}), &json!({"a": [1]})).is_empty());
        assert!(compare(&json!({"a": 1}), &json!({"a": null})).is_empty());
    }

    #[test]
    fn missing_key_and_wrong_type_are_reported() {
        assert_eq!(
            lines(compare(
                &json!({"a": {"b": 1}, "c": "x"}),
                &json!({"a": {}, "c": 2})
            )),
            vec!["$.a.b: missing", "$.c: expected string, got number"]
        );
    }

    #[test]
    fn extra_keys_and_int_versus_float_pass() {
        assert!(compare(&json!({"n": 1}), &json!({"n": 1.5, "x": true})).is_empty());
    }

    #[test]
    fn arrays_use_the_first_example_element_and_report_the_first_failure() {
        assert!(compare(&json!([]), &json!([1, "a"])).is_empty());
        assert!(compare(&json!([{"id": 1}]), &json!([])).is_empty());
        assert_eq!(
            lines(compare(
                &json!([{"id": 1}]),
                &json!([{"id": 2}, {"id": "x"}, {}])
            )),
            vec!["$[1].id: expected number, got string"]
        );
    }

    #[test]
    fn required_headers_match_in_any_case() {
        let documented: Vec<Header> = serde_json::from_value(json!([
            {"name": "X-Id", "value": "", "required": true},
            {"name": "X-Opt", "value": "", "required": false}
        ]))
        .unwrap();
        let live = vec![("x-id".to_string(), "1".to_string())];
        assert!(missing_headers(&documented, &live).is_empty());
        assert_eq!(
            lines(missing_headers(&documented, &[])),
            vec!["headers.X-Id: missing"]
        );
    }

    #[test]
    fn an_empty_or_non_json_body_is_one_difference() {
        let r = response(json!({"code": 200, "description": "", "schema": {"a": 1}}));
        assert_eq!(
            lines(against(&r, &reply(200, b""))),
            vec!["$: body is not JSON"]
        );
        assert_eq!(
            lines(against(&r, &reply(200, b"<html>"))),
            vec!["$: body is not JSON"]
        );
    }

    #[test]
    fn no_example_or_multipart_accepts_any_body() {
        let none = response(json!({"code": 200, "description": ""}));
        assert!(against(&none, &reply(200, b"<html>")).is_empty());
        let multipart = response(json!({"code": 200, "description": "", "multipart": []}));
        assert!(against(&multipart, &reply(200, b"<html>")).is_empty());
    }

    #[test]
    fn any_candidate_with_the_live_status_can_pass() {
        let responses = vec![
            response(json!({"code": 200, "description": "", "schema": {"token": "x"}})),
            response(json!({"code": 200, "description": "", "schema": {"otp": true}})),
        ];
        assert!(match_reply(&responses, &reply(200, br#"{"otp":false}"#), None).is_empty());
    }

    #[test]
    fn the_closest_candidate_is_reported() {
        let responses = vec![
            response(json!({"code": 200, "description": "", "schema": {"a": 1, "b": 1}})),
            response(json!({"code": 200, "description": "", "schema": {"a": 1, "c": 1}})),
        ];
        assert_eq!(
            lines(match_reply(
                &responses,
                &reply(200, br#"{"a":1,"b":1}"#),
                None
            )),
            Vec::<String>::new()
        );
        assert_eq!(
            lines(match_reply(
                &responses,
                &reply(200, br#"{"a":"x","b":1}"#),
                None
            )),
            vec!["$.a: expected number, got string"]
        );
    }

    #[test]
    fn an_undocumented_or_unexpected_status_is_a_difference() {
        let responses = vec![response(json!({"code": 200, "description": ""}))];
        assert_eq!(
            lines(match_reply(&responses, &reply(500, b"{}"), None)),
            vec!["status: 500 is not documented"]
        );
        assert_eq!(
            lines(match_reply(&responses, &reply(200, b"{}"), Some(404))),
            vec!["status: expected 404, got 200"]
        );
    }
}
