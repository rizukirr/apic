//! Field-level comparison of two contract revisions.
//!
//! This is the reason the panel exists: `git diff` can say a line changed,
//! this can say a required query parameter was added. Knows nothing about git.
//!
//! Nothing outside `tests` calls into this yet, wired in by `state` and
//! `view` in later tasks.
#![allow(dead_code)]

use apic_core::edit::{EditHeader, EditModel, EditQuery, EditResponse};
use apic_core::json::method_str;

/// One named difference between two revisions of a contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FieldChange {
    /// What changed, e.g. the field name or `query param <name>`.
    pub(crate) what: String,

    /// The old value, empty when the thing did not exist before.
    pub(crate) from: String,

    /// The new value, empty when the thing was removed.
    pub(crate) to: String,
}

impl FieldChange {
    fn new(what: impl Into<String>, from: impl Into<String>, to: impl Into<String>) -> FieldChange {
        FieldChange {
            what: what.into(),
            from: from.into(),
            to: to.into(),
        }
    }
}

/// Parses one contract revision. `None` when the text is not a contract, which
/// is the expected outcome for every non-contract file in the repo and sends
/// the caller to the line diff instead.
pub(crate) fn parse(text: &str) -> Option<EditModel> {
    apic_core::json::json_get(text, None)
        .ok()
        .map(EditModel::from_contract)
}

/// Every difference between `old` and `new`, in a stable field order. Empty
/// means the two revisions are semantically identical, which is how a
/// reformatting-only change reports.
pub(crate) fn diff_models(old: &EditModel, new: &EditModel) -> Vec<FieldChange> {
    let mut changes = Vec::new();
    scalar(&mut changes, "name", &old.name, &new.name);
    scalar(
        &mut changes,
        "description",
        &old.description,
        &new.description,
    );
    scalar(
        &mut changes,
        "method",
        &method_str(&old.method),
        &method_str(&new.method),
    );
    scalar(&mut changes, "url", &old.url, &new.url);
    keyed(
        &mut changes,
        "query param",
        old.query.iter().map(|q| (q.name.clone(), summary_query(q))),
        new.query.iter().map(|q| (q.name.clone(), summary_query(q))),
    );
    keyed(
        &mut changes,
        "header",
        old.headers
            .iter()
            .map(|h| (h.name.clone(), summary_header(h))),
        new.headers
            .iter()
            .map(|h| (h.name.clone(), summary_header(h))),
    );
    scalar(
        &mut changes,
        "request body",
        old.request
            .as_ref()
            .map(|b| b.example.as_str())
            .unwrap_or(""),
        new.request
            .as_ref()
            .map(|b| b.example.as_str())
            .unwrap_or(""),
    );
    keyed(
        &mut changes,
        "response",
        old.responses
            .iter()
            .map(|r| (r.code.clone(), summary_response(r))),
        new.responses
            .iter()
            .map(|r| (r.code.clone(), summary_response(r))),
    );
    changes
}

/// Records a change when a single field differs.
fn scalar(out: &mut Vec<FieldChange>, what: &str, old: &str, new: &str) {
    if old != new {
        out.push(FieldChange::new(what, old, new));
    }
}

/// Records additions, removals and modifications for a keyed list. Order is not
/// a difference: reordering query params changes the file but not the contract.
fn keyed<A, B>(out: &mut Vec<FieldChange>, what: &str, old: A, new: B)
where
    A: Iterator<Item = (String, String)>,
    B: Iterator<Item = (String, String)>,
{
    let old: std::collections::BTreeMap<String, String> = old.collect();
    let new: std::collections::BTreeMap<String, String> = new.collect();
    for (key, old_value) in &old {
        match new.get(key) {
            Some(new_value) if new_value != old_value => {
                out.push(FieldChange::new(
                    format!("{what} {key}"),
                    old_value,
                    new_value,
                ));
            }
            Some(_) => {}
            None => out.push(FieldChange::new(format!("{what} {key}"), old_value, "")),
        }
    }
    for (key, new_value) in &new {
        if !old.contains_key(key) {
            out.push(FieldChange::new(format!("{what} {key}"), "", new_value));
        }
    }
}

/// One comparable string for a query param: value, description, and whether it
/// is required, so making a parameter required registers as a change.
fn summary_query(q: &EditQuery) -> String {
    format!("{}|{}|{}", q.value, q.description, q.required)
}

/// One comparable string for a header: value and required flag.
fn summary_header(h: &EditHeader) -> String {
    format!("{}|{}", h.value, h.required)
}

/// One comparable string for a response: description, headers, and example
/// body, the fields `EditResponse` carries beside `code`, so a schema edit
/// registers.
fn summary_response(r: &EditResponse) -> String {
    let headers = r
        .headers
        .iter()
        .map(summary_header)
        .collect::<Vec<_>>()
        .join(",");
    format!("{}|{}|{}", r.description, headers, r.example)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = r#"{
        "name": "login",
        "description": "Log a user in",
        "method": "POST",
        "url": "https://api.example.com/auth/{id}",
        "query": [{ "name": "page", "value": "1", "description": "Page", "required": true }],
        "headers": [{ "name": "Content-Type", "value": "application/json", "required": true }],
        "request": { "user": { "email": "a@b.c" } },
        "responses": [{ "code": 200, "description": "ok", "schema": { "token": "x" } }]
    }"#;

    const NO_LISTS: &str = r#"{
        "name": "login",
        "description": "Log a user in",
        "method": "POST",
        "url": "https://api.example.com/auth/{id}",
        "headers": [],
        "responses": []
    }"#;

    fn model(text: &str) -> EditModel {
        parse(text).expect("valid contract")
    }

    #[test]
    fn identical_models_produce_no_changes() {
        let a = model(BASE);
        let b = model(BASE);
        assert_eq!(diff_models(&a, &b), Vec::new());
    }

    #[test]
    fn changed_method_produces_one_change_naming_the_method() {
        let old = model(BASE);
        let new_text = BASE.replace("\"POST\"", "\"GET\"");
        let new = model(&new_text);
        let changes = diff_models(&old, &new);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].what, "method");
        assert_eq!(changes[0].from, "POST");
        assert_eq!(changes[0].to, "GET");
    }

    #[test]
    fn query_param_added_produces_a_change_with_empty_from() {
        let old = model(NO_LISTS);
        let new_text = r#"{
            "name": "login",
            "description": "Log a user in",
            "method": "POST",
            "url": "https://api.example.com/auth/{id}",
            "query": [{ "name": "page", "value": "1", "description": "", "required": false }],
            "headers": [],
            "responses": []
        }"#;
        let new = model(new_text);
        let changes = diff_models(&old, &new);
        let change = changes
            .iter()
            .find(|c| c.what == "query param page")
            .expect("query param change present");
        assert_eq!(change.from, "");
        assert!(!change.to.is_empty());
    }

    #[test]
    fn query_param_made_required_produces_a_change_with_differing_from_and_to() {
        let old = model(BASE);
        // Flip the query param's required flag from true to false to force a change.
        let new_text = BASE.replace(
            "{ \"name\": \"page\", \"value\": \"1\", \"description\": \"Page\", \"required\": true }",
            "{ \"name\": \"page\", \"value\": \"1\", \"description\": \"Page\", \"required\": false }",
        );
        let new = model(&new_text);
        let changes = diff_models(&old, &new);
        let change = changes
            .iter()
            .find(|c| c.what == "query param page")
            .expect("query param change present");
        assert_ne!(change.from, change.to);
    }

    #[test]
    fn response_code_added_produces_a_change() {
        let old = model(NO_LISTS);
        let new_text = r#"{
            "name": "login",
            "description": "Log a user in",
            "method": "POST",
            "url": "https://api.example.com/auth/{id}",
            "headers": [],
            "responses": [{ "code": 404, "description": "not found" }]
        }"#;
        let new = model(new_text);
        let changes = diff_models(&old, &new);
        assert!(changes.iter().any(|c| c.what == "response 404"));
    }

    #[test]
    fn response_body_edited_produces_a_change() {
        let old = model(BASE);
        let new_text = BASE.replace("\"token\": \"x\"", "\"token\": \"y\"");
        let new = model(&new_text);
        let changes = diff_models(&old, &new);
        assert!(changes.iter().any(|c| c.what == "response 200"));
    }

    #[test]
    fn reordering_query_params_produces_no_changes() {
        let text = r#"{
            "name": "login",
            "description": "",
            "method": "POST",
            "url": "https://api.example.com/auth",
            "query": [
                { "name": "a", "value": "1", "description": "", "required": false },
                { "name": "b", "value": "2", "description": "", "required": false }
            ],
            "headers": [],
            "responses": []
        }"#;
        let reordered = r#"{
            "name": "login",
            "description": "",
            "method": "POST",
            "url": "https://api.example.com/auth",
            "query": [
                { "name": "b", "value": "2", "description": "", "required": false },
                { "name": "a", "value": "1", "description": "", "required": false }
            ],
            "headers": [],
            "responses": []
        }"#;
        let old = model(text);
        let new = model(reordered);
        assert_eq!(diff_models(&old, &new), Vec::new());
    }

    #[test]
    fn empty_model_reports_every_populated_field_as_changed() {
        let old = model(BASE);
        let empty = model(NO_LISTS);
        let changes = diff_models(&old, &empty);
        assert!(changes.iter().any(|c| c.what == "query param page"));
        assert!(changes.iter().any(|c| c.what == "header Content-Type"));
        assert!(changes.iter().any(|c| c.what == "request body"));
        assert!(changes.iter().any(|c| c.what == "response 200"));
    }
}
