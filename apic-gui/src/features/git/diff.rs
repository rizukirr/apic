//! Field-level comparison of two contract revisions.
//!
//! This is the reason the panel exists: `git diff` can say a line changed,
//! this can say a required query parameter was added. Knows nothing about git.
//!
//! Consumed by `view`.

use std::collections::{BTreeMap, BTreeSet};

use apic_core::edit::{EditHeader, EditModel, EditQuery, EditResponse};
use apic_core::json::{method_str, pretty_json};

/// One named difference between two revisions of a contract, at leaf-field
/// granularity: one `FieldChange` per value a person could actually read and
/// compare, never a bundle of several fields glued into one string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FieldChange {
    /// What changed, e.g. `query param page value` or `response 200 body`.
    pub(crate) what: String,

    /// The old value, empty when the thing did not exist before.
    pub(crate) from: String,

    /// The new value, empty when the thing was removed.
    pub(crate) to: String,
}

/// One line of a line-level diff, produced by [`line_diff`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LineDiffKind {
    Removed,
    Added,
    Unchanged,
}

/// One rendered row of a line-level diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LineDiffRow {
    pub(crate) kind: LineDiffKind,
    pub(crate) text: String,
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
    diff_query_params(&mut changes, &old.query, &new.query);
    diff_headers(&mut changes, "header", &old.headers, &new.headers);
    scalar(
        &mut changes,
        "request body",
        &pretty_body(old.request.as_ref().map(|b| b.example.as_str())),
        &pretty_body(new.request.as_ref().map(|b| b.example.as_str())),
    );
    diff_responses(&mut changes, &old.responses, &new.responses);
    changes
}

/// Records a change when a single field differs.
fn scalar(out: &mut Vec<FieldChange>, what: &str, old: &str, new: &str) {
    if old != new {
        out.push(FieldChange::new(what, old, new));
    }
}

/// `true`/`false` as text, so a required flip compares like any other scalar.
fn bool_str(b: bool) -> String {
    b.to_string()
}

/// Pretty-prints a JSON body example so a diff shows formatted lines instead
/// of one long line. Empty when there is no example.
fn pretty_body(example: Option<&str>) -> String {
    match example {
        Some(text) if !text.trim().is_empty() => pretty_json(text),
        _ => String::new(),
    }
}

/// The keys present in either map, in a stable order. Order is not itself a
/// difference: reordering query params changes the file but not the contract.
fn union_keys<'a, V>(
    old: &'a BTreeMap<String, V>,
    new: &'a BTreeMap<String, V>,
) -> BTreeSet<&'a str> {
    old.keys().chain(new.keys()).map(String::as_str).collect()
}

/// One leaf change per query param field: value, description, required. A
/// param present on only one side compares against empty/false defaults, so
/// an add or a remove reports the same way a modification does.
fn diff_query_params(out: &mut Vec<FieldChange>, old: &[EditQuery], new: &[EditQuery]) {
    let old: BTreeMap<String, &EditQuery> = old.iter().map(|q| (q.name.clone(), q)).collect();
    let new: BTreeMap<String, &EditQuery> = new.iter().map(|q| (q.name.clone(), q)).collect();
    let empty = EditQuery {
        name: String::new(),
        value: String::new(),
        description: String::new(),
        required: false,
    };
    for name in union_keys(&old, &new) {
        let o = old.get(name).copied().unwrap_or(&empty);
        let n = new.get(name).copied().unwrap_or(&empty);
        scalar(
            out,
            &format!("query param {name} value"),
            &o.value,
            &n.value,
        );
        scalar(
            out,
            &format!("query param {name} description"),
            &o.description,
            &n.description,
        );
        scalar(
            out,
            &format!("query param {name} required"),
            &bool_str(o.required),
            &bool_str(n.required),
        );
    }
}

/// One leaf change per header field: value, required. Shared by the
/// top-level header list (`what` is `"header"`) and, indirectly, by a
/// response's own headers, which are diffed as one block in
/// [`diff_responses`] instead, since they belong to that response.
fn diff_headers(out: &mut Vec<FieldChange>, what: &str, old: &[EditHeader], new: &[EditHeader]) {
    let old: BTreeMap<String, &EditHeader> = old.iter().map(|h| (h.name.clone(), h)).collect();
    let new: BTreeMap<String, &EditHeader> = new.iter().map(|h| (h.name.clone(), h)).collect();
    let empty = EditHeader {
        name: String::new(),
        value: String::new(),
        required: false,
    };
    for name in union_keys(&old, &new) {
        let o = old.get(name).copied().unwrap_or(&empty);
        let n = new.get(name).copied().unwrap_or(&empty);
        scalar(out, &format!("{what} {name} value"), &o.value, &n.value);
        scalar(
            out,
            &format!("{what} {name} required"),
            &bool_str(o.required),
            &bool_str(n.required),
        );
    }
}

/// A header list rendered as readable text, one header per line, for use as
/// a single diffable block under a response.
fn format_headers(headers: &[EditHeader]) -> String {
    headers
        .iter()
        .map(|h| format!("{}: {} (required {})", h.name, h.value, h.required))
        .collect::<Vec<_>>()
        .join("\n")
}

/// One leaf change per response field: description, headers, body. A
/// response present on only one side compares against empty defaults, so an
/// added or removed response code reports the same way a modification does.
fn diff_responses(out: &mut Vec<FieldChange>, old: &[EditResponse], new: &[EditResponse]) {
    let old: BTreeMap<String, &EditResponse> = old.iter().map(|r| (r.code.clone(), r)).collect();
    let new: BTreeMap<String, &EditResponse> = new.iter().map(|r| (r.code.clone(), r)).collect();
    let empty = EditResponse {
        code: String::new(),
        description: String::new(),
        headers: Vec::new(),
        example: String::new(),
    };
    for code in union_keys(&old, &new) {
        let o = old.get(code).copied().unwrap_or(&empty);
        let n = new.get(code).copied().unwrap_or(&empty);
        scalar(
            out,
            &format!("response {code} description"),
            &o.description,
            &n.description,
        );
        scalar(
            out,
            &format!("response {code} headers"),
            &format_headers(&o.headers),
            &format_headers(&n.headers),
        );
        scalar(
            out,
            &format!("response {code} body"),
            &pretty_body(Some(&o.example)),
            &pretty_body(Some(&n.example)),
        );
    }
}

/// Line-level diff of two multi-line strings, by longest common prefix and
/// suffix rather than a general LCS. That is enough for the case this
/// exists for: a small edit inside an otherwise unchanged multi-line body,
/// where it reports the one changed line instead of the whole body twice.
pub(crate) fn line_diff(from: &str, to: &str) -> Vec<LineDiffRow> {
    let from_lines: Vec<&str> = from.lines().collect();
    let to_lines: Vec<&str> = to.lines().collect();

    let mut prefix = 0;
    while prefix < from_lines.len()
        && prefix < to_lines.len()
        && from_lines[prefix] == to_lines[prefix]
    {
        prefix += 1;
    }

    let mut suffix = 0;
    while suffix < from_lines.len() - prefix
        && suffix < to_lines.len() - prefix
        && from_lines[from_lines.len() - 1 - suffix] == to_lines[to_lines.len() - 1 - suffix]
    {
        suffix += 1;
    }

    let mut rows = Vec::new();
    for line in &from_lines[..prefix] {
        rows.push(LineDiffRow {
            kind: LineDiffKind::Unchanged,
            text: (*line).to_string(),
        });
    }
    for line in &from_lines[prefix..from_lines.len() - suffix] {
        rows.push(LineDiffRow {
            kind: LineDiffKind::Removed,
            text: (*line).to_string(),
        });
    }
    for line in &to_lines[prefix..to_lines.len() - suffix] {
        rows.push(LineDiffRow {
            kind: LineDiffKind::Added,
            text: (*line).to_string(),
        });
    }
    for line in &to_lines[to_lines.len() - suffix..] {
        rows.push(LineDiffRow {
            kind: LineDiffKind::Unchanged,
            text: (*line).to_string(),
        });
    }
    rows
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
    fn query_param_added_produces_a_value_change_with_empty_from() {
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
        // description and required are unchanged from their defaults, so the
        // add reports as one leaf change, not a bundle.
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].what, "query param page value");
        assert_eq!(changes[0].from, "");
        assert_eq!(changes[0].to, "1");
    }

    #[test]
    fn query_param_required_flip_reports_only_the_required_leaf() {
        let old = model(BASE);
        // Flip the query param's required flag from true to false to force a change.
        let new_text = BASE.replace(
            "{ \"name\": \"page\", \"value\": \"1\", \"description\": \"Page\", \"required\": true }",
            "{ \"name\": \"page\", \"value\": \"1\", \"description\": \"Page\", \"required\": false }",
        );
        let new = model(&new_text);
        let changes = diff_models(&old, &new);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].what, "query param page required");
        assert_eq!(changes[0].from, "true");
        assert_eq!(changes[0].to, "false");
    }

    #[test]
    fn response_code_added_produces_a_description_change() {
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
        assert!(changes.iter().any(|c| c.what == "response 404 description"));
    }

    #[test]
    fn response_body_edited_produces_a_body_change() {
        let old = model(BASE);
        let new_text = BASE.replace("\"token\": \"x\"", "\"token\": \"y\"");
        let new = model(&new_text);
        let changes = diff_models(&old, &new);
        assert!(changes.iter().any(|c| c.what == "response 200 body"));
    }

    /// The regression guard for the bug this task fixes: a leaf-level change
    /// never carries the `|` separator that used to be built into the
    /// display string, no matter which fixture produces it.
    #[test]
    fn no_field_change_contains_the_pipe_character() {
        let fixtures: &[(&str, &str)] = &[
            (BASE, NO_LISTS),
            (NO_LISTS, BASE),
            (BASE, &BASE.replace("\"token\": \"x\"", "\"token\": \"y\"")),
            (
                BASE,
                &BASE.replace(
                    "{ \"name\": \"page\", \"value\": \"1\", \"description\": \"Page\", \"required\": true }",
                    "{ \"name\": \"page\", \"value\": \"1\", \"description\": \"Page\", \"required\": false }",
                ),
            ),
        ];
        for (old_text, new_text) in fixtures {
            let old = model(old_text);
            let new = model(new_text);
            for change in diff_models(&old, &new) {
                assert!(!change.what.contains('|'), "what: {}", change.what);
                assert!(!change.from.contains('|'), "from: {}", change.from);
                assert!(!change.to.contains('|'), "to: {}", change.to);
            }
        }
    }

    /// The property that motivates the whole task: a one-word edit inside a
    /// response body reports as exactly one change, and that change's
    /// `from`/`to` differ only on the line carrying the edited word.
    #[test]
    fn response_body_word_edit_produces_exactly_one_change_differing_on_one_line() {
        let old_text = r#"{
            "name": "login",
            "description": "Log a user in",
            "method": "POST",
            "url": "https://api.example.com/auth",
            "headers": [],
            "responses": [{
                "code": 200,
                "description": "ok",
                "schema": { "message": "Password berhasil diubah", "ok": true }
            }]
        }"#;
        let new_text = old_text.replace("berhasil diubah", "berhasil diuba");
        let old = model(old_text);
        let new = model(&new_text);
        let changes = diff_models(&old, &new);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].what, "response 200 body");

        let from_lines: Vec<&str> = changes[0].from.lines().collect();
        let to_lines: Vec<&str> = changes[0].to.lines().collect();
        assert_eq!(from_lines.len(), to_lines.len());
        let differing: Vec<usize> = (0..from_lines.len())
            .filter(|&i| from_lines[i] != to_lines[i])
            .collect();
        assert_eq!(differing, vec![1], "exactly one line should differ");
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
        assert!(changes.iter().any(|c| c.what == "query param page value"));
        assert!(
            changes
                .iter()
                .any(|c| c.what == "header Content-Type value")
        );
        assert!(changes.iter().any(|c| c.what == "request body"));
        assert!(changes.iter().any(|c| c.what == "response 200 description"));
        assert!(changes.iter().any(|c| c.what == "response 200 body"));
    }
}
