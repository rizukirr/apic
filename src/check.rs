//! `apic check`: send contracts' requests and compare the live responses with
//! the documented ones.

use crate::cli::{query_targets, resolve_one};
use crate::output::{CliError, ErrorKind};
use crate::render::sanitize;
use crate::send::{apply_var_flags, load_contract};
use apic_core::config::find_apic_dir;
use apic_core::file::{home_relative, relative_slash};
use apic_core::http;
use apic_core::json::{Header, JsonContent, Method, Response, method_str};
use apic_core::vars::Vars;
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;

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

/// Parsed `apic check` arguments.
pub(crate) struct CheckArgs {
    /// The contract or folder query, `None` for every contract.
    pub(crate) find: Option<String>,
    /// The environment name.
    pub(crate) env: Option<String>,
    /// Raw `NAME=VALUE` pairs.
    pub(crate) vars: Vec<String>,
    /// The live status `--status` requires.
    pub(crate) status: Option<u16>,
    /// Send unsafe methods in folder and whole-project runs.
    pub(crate) unsafe_methods: bool,
    /// Timeout in seconds, per request.
    pub(crate) timeout: u64,
}

/// What happened to one contract.
enum Outcome {
    /// A reply arrived. Passed when `differences` is empty.
    Checked {
        code: u16,
        elapsed_ms: u64,
        differences: Vec<Difference>,
    },
    /// Not sent: an unsafe method in a folder or whole-project run.
    Skipped,
    /// Could not be checked.
    Error(CliError),
}

/// One line of the report.
struct Row {
    path: String,
    method: Option<String>,
    outcome: Outcome,
}

/// Sends `contract` and compares the reply with its documented responses.
fn check_one(
    contract: &JsonContent,
    vars: &Vars,
    project_root: &Path,
    args: &CheckArgs,
) -> Result<Outcome, CliError> {
    let prepared = http::prepare(contract, vars, project_root)?;
    let reply = http::execute(&prepared, Duration::from_secs(args.timeout))
        .map_err(|err| CliError::new(ErrorKind::Network, err))?;
    Ok(Outcome::Checked {
        code: reply.status,
        elapsed_ms: reply.elapsed.as_millis() as u64,
        differences: match_reply(&contract.responses, &reply, args.status),
    })
}

/// Handles `apic check`.
pub(crate) fn check_cmd(args: &CheckArgs) -> Result<(), CliError> {
    let apic_dir = find_apic_dir().ok_or("Not in an apic project (run `apic init`)")?;
    let project_root = apic_dir.parent().unwrap_or(&apic_dir).to_path_buf();
    apic_core::env::ensure_gitignore(&apic_dir)?;
    let mut vars = match &args.env {
        Some(name) => apic_core::env::load(&apic_dir, name)?,
        None => Vars::new(),
    };
    apply_var_flags(&args.vars, &mut vars)?;

    // A single contract is always sent, and its failure is the command's error.
    let single = args.find.as_deref().filter(|q| !q.ends_with('/'));
    let targets = match single {
        Some(query) => vec![resolve_one(query)?],
        None => query_targets(args.find.as_deref())?.unwrap_or_default(),
    };

    let mut rows = Vec::new();
    for path in &targets {
        let shown = relative_slash(path, &project_root);
        let contract = match load_contract(path) {
            Ok(contract) => contract,
            Err(err) if single.is_some() => return Err(err),
            Err(err) => {
                rows.push(Row {
                    path: shown,
                    method: None,
                    outcome: Outcome::Error(err),
                });
                continue;
            }
        };
        let safe = matches!(
            contract.method,
            Method::GET | Method::HEAD | Method::OPTIONS
        );
        let outcome = if single.is_none() && !safe && !args.unsafe_methods {
            Outcome::Skipped
        } else {
            match check_one(&contract, &vars, &project_root, args) {
                Ok(outcome) => outcome,
                Err(err) if single.is_some() => return Err(err),
                Err(err) => Outcome::Error(err),
            }
        };
        rows.push(Row {
            path: shown,
            method: Some(method_str(&contract.method)),
            outcome,
        });
    }
    report(&rows)
}

/// The run's exit status: the worst result, mismatch first, then network,
/// invalid and any other error. `None` when nothing failed.
fn worst(rows: &[Row]) -> Option<ErrorKind> {
    let rank = |kind: ErrorKind| match kind {
        ErrorKind::Mismatch => 4,
        ErrorKind::Network => 3,
        ErrorKind::Invalid => 2,
        _ => 1,
    };
    rows.iter()
        .filter_map(|row| match &row.outcome {
            Outcome::Checked { differences, .. } if !differences.is_empty() => {
                Some(ErrorKind::Mismatch)
            }
            Outcome::Error(err) => Some(err.kind),
            _ => None,
        })
        .max_by_key(|kind| rank(*kind))
}

/// The `--json` object for one row.
fn row_json(row: &Row) -> Value {
    let mut doc = json!({"path": row.path});
    if let Some(method) = &row.method {
        doc["method"] = json!(method);
    }
    match &row.outcome {
        Outcome::Checked {
            code,
            elapsed_ms,
            differences,
        } => {
            doc["result"] = json!(if differences.is_empty() {
                "pass"
            } else {
                "mismatch"
            });
            doc["code"] = json!(code);
            doc["elapsed_ms"] = json!(elapsed_ms);
            if !differences.is_empty() {
                let found: Vec<Value> = differences
                    .iter()
                    .map(|d| json!({"path": d.path, "message": d.message}))
                    .collect();
                doc["differences"] = json!(found);
            }
        }
        Outcome::Skipped => {
            doc["result"] = json!("skipped");
            doc["reason"] = json!("unsafe method");
        }
        Outcome::Error(err) => {
            doc["result"] = json!("error");
            doc["error"] = json!({"kind": err.kind.as_str(), "message": err.message});
        }
    }
    doc
}

/// Prints one row of the text report.
fn print_row(row: &Row) {
    let path = sanitize(&row.path);
    match &row.outcome {
        Outcome::Checked {
            code,
            elapsed_ms,
            differences,
        } => {
            let label = if differences.is_empty() {
                "pass"
            } else {
                "mismatch"
            };
            println!("{label:<9} {path}   {code} · {elapsed_ms} ms");
            for d in differences {
                println!("          {}: {}", sanitize(&d.path), sanitize(&d.message));
            }
        }
        Outcome::Skipped => println!(
            "{:<9} {path}   {}, pass --unsafe to send it",
            "skipped",
            row.method.as_deref().unwrap_or_default()
        ),
        Outcome::Error(err) => println!(
            "{:<9} {path}   {}",
            "error",
            home_relative(&sanitize(&err.message))
        ),
    }
}

/// Prints the report and returns the run's exit status.
fn report(rows: &[Row]) -> Result<(), CliError> {
    let count = |want: fn(&Outcome) -> bool| rows.iter().filter(|r| want(&r.outcome)).count();
    let passed =
        count(|o| matches!(o, Outcome::Checked { differences, .. } if differences.is_empty()));
    let mismatched =
        count(|o| matches!(o, Outcome::Checked { differences, .. } if !differences.is_empty()));
    let skipped = count(|o| matches!(o, Outcome::Skipped));
    let failed = count(|o| matches!(o, Outcome::Error(_)));
    if crate::output::json() {
        let results: Vec<Value> = rows.iter().map(row_json).collect();
        crate::output::emit(&json!({
            "results": results,
            "passed": passed,
            "mismatched": mismatched,
            "skipped": skipped,
            "failed": failed,
        }));
    } else {
        for row in rows {
            print_row(row);
        }
        println!("\n{passed} passed, {mismatched} mismatched, {skipped} skipped, {failed} failed");
    }
    match worst(rows) {
        Some(kind) => Err(CliError::reported(kind)),
        None => Ok(()),
    }
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
