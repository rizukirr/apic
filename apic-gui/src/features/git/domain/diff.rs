//! Field-level comparison of two contract revisions, and the parse of a git diff into inline rows.
//!
//! This is the reason the panel exists: `git diff` can say a line changed,
//! this can say a required query parameter was added. Knows nothing about git.
//!
//! Consumed by `view`.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use apic_core::edit::{EditHeader, EditModel, EditPart, EditQuery, EditResponse};
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
    diff_parts(&mut changes, "part", &old.multipart, &new.multipart);
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

/// One leaf change per part field: value, filename, content type,
/// description, required. Mirrors [`diff_query_params`], including the empty
/// sentinel, so a part present on only one side reports the same way a
/// modification does. `what` carries the position, `"part"` for the request
/// and `"response {code} part"` for a response, so one function serves both.
fn diff_parts(out: &mut Vec<FieldChange>, what: &str, old: &[EditPart], new: &[EditPart]) {
    let old: BTreeMap<String, &EditPart> = old.iter().map(|p| (p.name.clone(), p)).collect();
    let new: BTreeMap<String, &EditPart> = new.iter().map(|p| (p.name.clone(), p)).collect();
    let empty = EditPart {
        name: String::new(),
        value: String::new(),
        filename: String::new(),
        content_type: String::new(),
        description: String::new(),
        required: false,
    };
    for name in union_keys(&old, &new) {
        let o = old.get(name).copied().unwrap_or(&empty);
        let n = new.get(name).copied().unwrap_or(&empty);
        scalar(out, &format!("{what} {name} value"), &o.value, &n.value);
        scalar(
            out,
            &format!("{what} {name} filename"),
            &o.filename,
            &n.filename,
        );
        scalar(
            out,
            &format!("{what} {name} content type"),
            &o.content_type,
            &n.content_type,
        );
        scalar(
            out,
            &format!("{what} {name} description"),
            &o.description,
            &n.description,
        );
        scalar(
            out,
            &format!("{what} {name} required"),
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

/// Keys each response by its code and its position among the responses
/// sharing that code (0 for the first), so a contract with two `200`
/// examples keeps both instead of the second replacing the first.
fn keyed_responses(responses: &[EditResponse]) -> BTreeMap<(String, usize), &EditResponse> {
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    responses
        .iter()
        .map(|r| {
            let n = seen.entry(r.code.as_str()).or_insert(0);
            let key = (r.code.clone(), *n);
            *n += 1;
            (key, r)
        })
        .collect()
}

/// One leaf change per response field: description, headers, body. A
/// response present on only one side compares against empty defaults, so an
/// added or removed response code reports the same way a modification does.
///
/// Responses are matched by code and by position among those sharing the
/// code. When a code repeats on either side, the label carries the 1-based
/// position (`response 200 (2) body`); a unique code keeps `response 200 body`.
fn diff_responses(out: &mut Vec<FieldChange>, old: &[EditResponse], new: &[EditResponse]) {
    let old: BTreeMap<(String, usize), &EditResponse> = keyed_responses(old);
    let new: BTreeMap<(String, usize), &EditResponse> = keyed_responses(new);
    let repeats = |code: &str| {
        old.contains_key(&(code.to_string(), 1)) || new.contains_key(&(code.to_string(), 1))
    };
    let empty = EditResponse {
        code: String::new(),
        description: String::new(),
        headers: Vec::new(),
        example: String::new(),
        multipart: Vec::new(),
    };
    let keys: std::collections::BTreeSet<&(String, usize)> = old.keys().chain(new.keys()).collect();
    for key in keys {
        let o = old.get(key).copied().unwrap_or(&empty);
        let n = new.get(key).copied().unwrap_or(&empty);
        let code = if repeats(&key.0) {
            format!("{} ({})", key.0, key.1 + 1)
        } else {
            key.0.clone()
        };
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
        diff_parts(
            out,
            &format!("response {code} part"),
            &o.multipart,
            &n.multipart,
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

/// Drops the `diff --git`, `index`, `---` and `+++` preamble, keeping the
/// hunks themselves: the first `@@` line is where the diff proper starts, so
/// everything from there on is returned. The untracked-file path synthesises
/// `+` lines with no header at all, so a body with no `@@` line is returned
/// unchanged rather than assuming a header is always there.
pub(crate) fn strip_diff_header(raw: &str) -> &str {
    let mut consumed = 0;
    for line in raw.lines() {
        if line.starts_with("@@") {
            return raw.get(consumed..).unwrap_or("");
        }
        consumed += line.len() + 1;
    }
    raw
}

/// Which side of a unified diff one line belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LineKind {
    Context,
    Removed,
    Added,
}

/// One line of the file in an inline diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiffLine {
    pub(crate) kind: LineKind,
    /// The line's number in the old file. `None` for an added line and for a
    /// notice such as `\ No newline at end of file`.
    pub(crate) old_no: Option<u32>,
    /// The line's number in the new file. `None` for a removed line and for
    /// a notice.
    pub(crate) new_no: Option<u32>,
    /// The line without its `-`, `+` or space marker.
    pub(crate) text: String,
    /// The byte range of `text` that differs from the paired line on the
    /// other side, drawn in a stronger tint. Always on `char` boundaries.
    pub(crate) emphasis: Option<Range<usize>>,
}

/// One row of an inline diff, built by [`parse_unified`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DiffRow {
    /// A whole `@@ ... @@` hunk header line.
    Hunk(String),
    /// One line of the file.
    Line(DiffLine),
}

/// Parses `git diff` output into inline rows, numbering lines from each
/// hunk header and marking the changed span of each removed line paired
/// with an added one. A body with no hunk whose lines all start with `+`
/// is the untracked file `diff_job` builds: every line is added, numbered
/// from 1. Any other body with no hunk, such as `Binary files ... differ`,
/// becomes unnumbered context with the `diff --git`, `index`, `---` and
/// `+++` lines dropped.
pub(crate) fn parse_unified(raw: &str) -> Vec<DiffRow> {
    let body = strip_diff_header(raw);
    let has_hunk = body.lines().any(|line| line.starts_with("@@"));
    let mut rows = Vec::new();

    if !has_hunk {
        let untracked = body.lines().all(|line| line.starts_with('+'));
        let mut new_no = 0;
        for line in body.lines() {
            if untracked {
                new_no += 1;
                rows.push(line_row(LineKind::Added, None, Some(new_no), &line[1..]));
            } else if !is_file_header(line) {
                rows.push(line_row(LineKind::Context, None, None, line));
            }
        }
        return rows;
    }

    let (mut old_no, mut new_no) = (1, 1);
    for line in body.lines() {
        if line.starts_with("@@") {
            (old_no, new_no) = hunk_starts(line);
            rows.push(DiffRow::Hunk(line.to_string()));
        } else if let Some(text) = line.strip_prefix('-') {
            rows.push(line_row(LineKind::Removed, Some(old_no), None, text));
            old_no += 1;
        } else if let Some(text) = line.strip_prefix('+') {
            rows.push(line_row(LineKind::Added, None, Some(new_no), text));
            new_no += 1;
        } else if line.is_empty() || line.starts_with(' ') {
            let text = line.get(1..).unwrap_or("");
            rows.push(line_row(
                LineKind::Context,
                Some(old_no),
                Some(new_no),
                text,
            ));
            old_no += 1;
            new_no += 1;
        } else {
            rows.push(line_row(LineKind::Context, None, None, line));
        }
    }
    emphasize(&mut rows);
    rows
}

/// Builds one `DiffRow::Line` with no emphasis.
fn line_row(kind: LineKind, old_no: Option<u32>, new_no: Option<u32>, text: &str) -> DiffRow {
    DiffRow::Line(DiffLine {
        kind,
        old_no,
        new_no,
        text: text.to_string(),
        emphasis: None,
    })
}

/// Whether `line` is part of git's per-file preamble rather than content.
fn is_file_header(line: &str) -> bool {
    ["diff --git ", "index ", "--- ", "+++ "]
        .iter()
        .any(|prefix| line.starts_with(prefix))
}

/// The old and new start lines from a `@@ -a,b +c,d @@` header, 1 for any
/// part that does not parse.
fn hunk_starts(header: &str) -> (u32, u32) {
    let mut parts = header.split_whitespace().skip(1);
    let mut start = |sign: char| {
        parts
            .next()
            .and_then(|part| part.strip_prefix(sign))
            .and_then(|part| part.split(',').next())
            .and_then(|n| n.parse().ok())
            .unwrap_or(1)
    };
    let old = start('-');
    let new = start('+');
    (old, new)
}

/// The kind of the row at `i`, `None` for a hunk header or past the end.
fn kind_at(rows: &[DiffRow], i: usize) -> Option<LineKind> {
    match rows.get(i) {
        Some(DiffRow::Line(line)) => Some(line.kind),
        _ => None,
    }
}

/// Sets `emphasis` on each removed line paired by position with an added
/// line in the run after it. Unnumbered notice rows, such as
/// `\ No newline at end of file`, between the two runs do not break the
/// pairing.
fn emphasize(rows: &mut [DiffRow]) {
    let mut i = 0;
    while i < rows.len() {
        if kind_at(rows, i) != Some(LineKind::Removed) {
            i += 1;
            continue;
        }
        let mut removed = Vec::new();
        while kind_at(rows, i) == Some(LineKind::Removed) {
            removed.push(i);
            i += 1;
        }
        while is_notice(rows, i) {
            i += 1;
        }
        let mut added = Vec::new();
        while kind_at(rows, i) == Some(LineKind::Added) {
            added.push(i);
            i += 1;
        }
        for (&o, &n) in removed.iter().zip(&added) {
            let (old_span, new_span) = match (&rows[o], &rows[n]) {
                (DiffRow::Line(old), DiffRow::Line(new)) => changed_spans(&old.text, &new.text),
                _ => (None, None),
            };
            if let DiffRow::Line(old) = &mut rows[o] {
                old.emphasis = old_span;
            }
            if let DiffRow::Line(new) = &mut rows[n] {
                new.emphasis = new_span;
            }
        }
    }
}

/// Whether the row at `i` is an unnumbered context notice.
fn is_notice(rows: &[DiffRow], i: usize) -> bool {
    matches!(
        rows.get(i),
        Some(DiffRow::Line(line))
            if line.kind == LineKind::Context && line.old_no.is_none() && line.new_no.is_none()
    )
}

/// The byte range of each line left after trimming the prefix and suffix
/// the two share, compared by `char`, then widened to whole words. A side
/// whose range is empty or the whole line gets `None`.
fn changed_spans(old: &str, new: &str) -> (Option<Range<usize>>, Option<Range<usize>>) {
    let prefix: usize = old
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum();
    let suffix: usize = old[prefix..]
        .chars()
        .rev()
        .zip(new[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum();
    let prefix = widen_prefix(old, new, prefix);
    let suffix = widen_suffix(old, new, suffix);
    let span = |s: &str| {
        let range = prefix..s.len() - suffix;
        (!range.is_empty() && range.len() < s.len()).then_some(range)
    };
    (span(old), span(new))
}

/// A letter, digit or `_`: what a word is made of for emphasis.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Moves the shared prefix's end back to the start of the word it cuts, if
/// it cuts one: a word character just before the cut and, on either side,
/// just after it.
fn widen_prefix(old: &str, new: &str, mut prefix: usize) -> usize {
    let after = |s: &str| s[prefix..].chars().next().is_some_and(is_word);
    if after(old) || after(new) {
        while let Some(c) = old[..prefix].chars().next_back().filter(|&c| is_word(c)) {
            prefix -= c.len_utf8();
        }
    }
    prefix
}

/// Moves the shared suffix's start forward to the end of the word it cuts,
/// if it cuts one: a word character just after the cut and, on either side,
/// just before it.
fn widen_suffix(old: &str, new: &str, mut suffix: usize) -> usize {
    let before = |s: &str| {
        s[..s.len() - suffix]
            .chars()
            .next_back()
            .is_some_and(is_word)
    };
    if before(old) || before(new) {
        while let Some(c) = old[old.len() - suffix..]
            .chars()
            .next()
            .filter(|&c| is_word(c))
        {
            suffix -= c.len_utf8();
        }
    }
    suffix
}

/// The field-level changes between two revisions of a contract, or `None`
/// when either side is absent, either side is not a contract, or the change
/// is formatting only. `None` is the caller's cue to show the raw diff.
pub(crate) fn semantic_changes(old: Option<&str>, new: Option<&str>) -> Option<Vec<FieldChange>> {
    let old = parse(old?)?;
    let new = parse(new?)?;
    let changes = diff_models(&old, &new);
    (!changes.is_empty()).then_some(changes)
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

    /// Pins the bug where two responses sharing a code collapsed into one,
    /// hiding every change to the first: the edit must be reported, and the
    /// label must say which of the two `200`s changed.
    #[test]
    fn an_edit_in_the_first_of_two_same_code_responses_is_reported() {
        let old_text = r#"{
            "name": "login",
            "method": "POST",
            "url": "https://api.example.com/auth",
            "headers": [],
            "responses": [
                { "code": 200, "description": "OK", "schema": { "message": "Login berhasil" } },
                { "code": 200, "description": "OK", "schema": { "message": "First login" } },
                { "code": 401, "description": "Unauthorized" }
            ]
        }"#;
        let new_text = old_text.replace("Login berhasil", "Berhasil");
        let changes = diff_models(&model(old_text), &model(&new_text));
        assert_eq!(changes.len(), 1, "exactly the one body changed");
        assert_eq!(changes[0].what, "response 200 (1) body");
        assert!(changes[0].from.contains("Login berhasil"));
        assert!(changes[0].to.contains("Berhasil"));
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

    #[test]
    fn changing_a_part_reports_one_change_per_leaf_field() {
        const BEFORE: &str = r#"{
            "name": "x", "method": "POST", "url": "https://h", "headers": [],
            "multipart": [{ "name": "avatar", "value": "", "filename": "a.png", "contentType": "image/png", "required": false }],
            "responses": []
        }"#;
        const AFTER: &str = r#"{
            "name": "x", "method": "POST", "url": "https://h", "headers": [],
            "multipart": [{ "name": "avatar", "value": "", "filename": "b.png", "contentType": "image/png", "required": true }],
            "responses": []
        }"#;
        let changes = diff_models(&model(BEFORE), &model(AFTER));
        assert!(changes.iter().any(|c| c.what == "part avatar filename"));
        assert!(changes.iter().any(|c| c.what == "part avatar required"));
        // The point of this view is leaf fields, not one glued blob.
        assert!(!changes.iter().any(|c| c.what == "part avatar"));
    }

    #[test]
    fn changing_a_response_part_names_the_response_code() {
        const BEFORE: &str = r#"{
            "name": "x", "method": "GET", "url": "https://h", "headers": [],
            "responses": [{ "code": 200, "description": "ok",
                "multipart": [{ "name": "thumb", "value": "", "filename": "a.png", "required": false }] }]
        }"#;
        const AFTER: &str = r#"{
            "name": "x", "method": "GET", "url": "https://h", "headers": [],
            "responses": [{ "code": 200, "description": "ok",
                "multipart": [{ "name": "thumb", "value": "", "filename": "b.png", "required": false }] }]
        }"#;
        let changes = diff_models(&model(BEFORE), &model(AFTER));
        assert!(
            changes
                .iter()
                .any(|c| c.what == "response 200 part thumb filename")
        );
    }

    #[test]
    fn strip_diff_header_drops_the_preamble_and_keeps_the_first_hunk_marker() {
        let raw = "diff --git a/contracts/login.json b/contracts/login.json\nindex 2a4e428..5d3ef15 100644\n--- a/contracts/login.json\n+++ b/contracts/login.json\n@@ -1,3 +1,3 @@\n {\n-    \"name\": \"user-login\",\n+    \"name\": \"login-v2\",\n }\n";
        let body = strip_diff_header(raw);
        assert!(!body.contains("diff --git"));
        assert!(!body.contains("index "));
        assert!(body.starts_with("@@ -1,3 +1,3 @@\n"));
    }

    #[test]
    fn strip_diff_header_keeps_every_hunk_marker() {
        // Hunk-sized output has more than one `@@` line. A stripper that
        // returned everything after the *first* one would leave the rest in
        // the body for the renderer to print with its first character eaten.
        let raw = "diff --git a/f b/f\nindex 1111111..2222222 100644\n--- a/f\n+++ b/f\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n@@ -30,3 +30,3 @@\n c\n-d\n+D\n";
        let body = strip_diff_header(raw);
        assert_eq!(body.matches("@@ -").count(), 2);
    }

    #[test]
    fn strip_diff_header_leaves_a_headerless_diff_unchanged() {
        // The untracked-file path synthesises `+` lines with no header at
        // all, so a diff with no `@@` line must pass through untouched.
        let raw = "+first\n+second\n";
        assert_eq!(strip_diff_header(raw), raw);
    }

    #[test]
    fn semantic_changes_is_none_when_a_side_is_missing() {
        let text = r#"{"name":"x","method":"GET","url":"http://a","headers":[],"responses":[]}"#;
        assert!(semantic_changes(None, Some(text)).is_none());
        assert!(semantic_changes(Some(text), None).is_none());
    }

    #[test]
    fn semantic_changes_is_none_for_identical_contracts() {
        let text = r#"{"name":"x","method":"GET","url":"http://a","headers":[],"responses":[]}"#;
        assert!(semantic_changes(Some(text), Some(text)).is_none());
    }

    #[test]
    fn semantic_changes_reports_a_method_change() {
        let get = r#"{"name":"x","method":"GET","url":"http://a","headers":[],"responses":[]}"#;
        let post = r#"{"name":"x","method":"POST","url":"http://a","headers":[],"responses":[]}"#;
        let changes = semantic_changes(Some(get), Some(post)).expect("a method change is semantic");
        assert!(!changes.is_empty());
    }

    type Shape = (LineKind, Option<u32>, Option<u32>, String);

    /// `(kind, old_no, new_no, text)` of every line row, `None` for a hunk.
    fn shape(rows: &[DiffRow]) -> Vec<Option<Shape>> {
        rows.iter()
            .map(|row| match row {
                DiffRow::Hunk(_) => None,
                DiffRow::Line(l) => Some((l.kind, l.old_no, l.new_no, l.text.clone())),
            })
            .collect()
    }

    fn line(rows: &[DiffRow], i: usize) -> &DiffLine {
        match &rows[i] {
            DiffRow::Line(l) => l,
            DiffRow::Hunk(h) => panic!("row {i} is a hunk: {h}"),
        }
    }

    #[test]
    fn parse_unified_numbers_lines_across_two_hunks() {
        let raw = "diff --git a/f b/f\nindex 1..2 100644\n--- a/f\n+++ b/f\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n@@ -10,2 +10,3 @@\n x\n+y\n z\n";
        use LineKind::{Added, Context, Removed};
        let s = |k, o, n, t: &str| Some((k, o, n, t.to_string()));
        assert_eq!(
            shape(&parse_unified(raw)),
            vec![
                None,
                s(Context, Some(1), Some(1), "a"),
                s(Removed, Some(2), None, "b"),
                s(Added, None, Some(2), "B"),
                s(Context, Some(3), Some(3), "c"),
                None,
                s(Context, Some(10), Some(10), "x"),
                s(Added, None, Some(11), "y"),
                s(Context, Some(11), Some(12), "z"),
            ]
        );
    }

    #[test]
    fn parse_unified_emphasizes_one_changed_word() {
        let rows = parse_unified("@@ -1 +1 @@\n-\"method\": \"GET\",\n+\"method\": \"HEAD\",\n");
        let old = line(&rows, 1);
        let new = line(&rows, 2);
        assert_eq!(&old.text[old.emphasis.clone().unwrap()], "GET");
        assert_eq!(&new.text[new.emphasis.clone().unwrap()], "HEAD");
    }

    #[test]
    fn parse_unified_emphasis_lands_on_char_boundaries() {
        let rows = parse_unified("@@ -1 +1 @@\n-café au lait\n+cafe au lait\n");
        let old = line(&rows, 1);
        let new = line(&rows, 2);
        assert_eq!(&old.text[old.emphasis.clone().unwrap()], "café");
        assert_eq!(&new.text[new.emphasis.clone().unwrap()], "cafe");
    }

    #[test]
    fn parse_unified_gives_no_emphasis_to_a_whole_line_change() {
        let rows = parse_unified("@@ -1 +1 @@\n-foo\n+bar\n");
        assert_eq!(line(&rows, 1).emphasis, None);
        assert_eq!(line(&rows, 2).emphasis, None);
    }

    #[test]
    fn parse_unified_pairs_by_position_and_leaves_extra_lines_alone() {
        let rows = parse_unified("@@ -1,2 +1 @@\n-a b\n-c\n+a x\n");
        let old = line(&rows, 1);
        let new = line(&rows, 3);
        assert_eq!(&old.text[old.emphasis.clone().unwrap()], "b");
        assert_eq!(&new.text[new.emphasis.clone().unwrap()], "x");
        assert_eq!(line(&rows, 2).emphasis, None);
    }

    #[test]
    fn parse_unified_widens_emphasis_to_whole_words() {
        let rows = parse_unified(
            "@@ -1,2 +1,2 @@\n-\"method\": \"GET\",\n-count: 10\n+\"method\": \"POST\",\n+count: 12\n",
        );
        let span = |i: usize| {
            let l = line(&rows, i);
            l.text[l.emphasis.clone().unwrap()].to_string()
        };
        assert_eq!(span(1), "GET");
        assert_eq!(span(3), "POST");
        assert_eq!(span(2), "10");
        assert_eq!(span(4), "12");
    }

    #[test]
    fn parse_unified_pairs_across_a_no_newline_notice() {
        let rows = parse_unified(
            "@@ -1,2 +1,2 @@\n-café au lait\n-end\n\\ No newline at end of file\n+cafe au lait\n+end\n",
        );
        let old = line(&rows, 1);
        let new = line(&rows, 4);
        assert_eq!(&old.text[old.emphasis.clone().unwrap()], "café");
        assert_eq!(&new.text[new.emphasis.clone().unwrap()], "cafe");
    }

    #[test]
    fn parse_unified_numbers_an_untracked_body_from_one() {
        let rows = parse_unified("+hello\n+world\n");
        assert_eq!(
            shape(&rows),
            vec![
                Some((LineKind::Added, None, Some(1), "hello".to_string())),
                Some((LineKind::Added, None, Some(2), "world".to_string())),
            ]
        );
    }

    #[test]
    fn parse_unified_leaves_no_newline_notices_unnumbered() {
        let rows = parse_unified(
            "@@ -1 +1 @@\n-a\n\\ No newline at end of file\n+b\n\\ No newline at end of file\n",
        );
        for i in [2, 4] {
            let notice = line(&rows, i);
            assert_eq!(notice.kind, LineKind::Context);
            assert_eq!((notice.old_no, notice.new_no), (None, None));
        }
    }

    #[test]
    fn parse_unified_shows_a_binary_notice_as_one_unnumbered_row() {
        let rows = parse_unified(
            "diff --git a/i.png b/i.png\nindex 1..2 100644\nBinary files a/i.png and b/i.png differ\n",
        );
        assert_eq!(
            shape(&rows),
            vec![Some((
                LineKind::Context,
                None,
                None,
                "Binary files a/i.png and b/i.png differ".to_string()
            ))]
        );
    }
}
