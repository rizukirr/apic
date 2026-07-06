//! Read-shaped table model derived from `EditModel`.
//!
//! `flatten` emits a `Vec<Section>` that mirrors exactly what `apic read`
//! prints (see `crate::render::Printer`): a bespoke header block followed by the
//! `QUERY`/`HEADERS`/`REQUEST`/`RESPONSE` sections, each carrying an
//! `add: Option<Field>` so the `a` key knows what to append. Every editable
//! `Cell` carries a `Field` address that the handlers in `state.rs` use to
//! locate the target in the model.

use crate::tui::model::EditModel;
// The cell-address enums are UI-agnostic and live in core so a GUI can reuse
// them; re-exported here under the path the TUI already uses.
pub(crate) use apic_core::edit::{BodyLoc, Field};

/// How a cell is edited.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CellKind {
    Label, // non-editable (column-1 field labels, built-url, example prompt)
    Text,
    Enum,
    Bool,
}

/// One cell in a table row.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Cell {
    pub field: Field,
    pub kind: CellKind,
    pub value: String,
}

/// What kind of section this is, for drawing.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SectionKind {
    Header,
    Table,
    Body,
}

/// Which collapsible region (if any) is currently expanded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Expand {
    Url,
    Response(usize),
}

/// Row behavior / how a row is drawn.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RowKind {
    Name,    // header name line (drawn uppercased)
    Desc,    // header description line (drawn only when non-empty)
    UrlLine, // collapsed ` METHOD <built-url>`; Enter expands
    Title,   // a Body section's bold title line; Enter expands code/description
    Field,   // an editable table / key-value row
    Example, // inline ` Example:` + raw JSON; Enter opens the modal
}

/// One displayable table row.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TableRow {
    pub kind: RowKind,
    pub indent: u16,
    pub cells: Vec<Cell>,
    pub raw: String,    // example buffer for RowKind::Example; empty otherwise
    pub prefix: String, // tree prefix (`├─ `/`└─ `) shown at display time only
}

/// A titled section. `headers: Some(cols)` renders a dim column-header line and
/// aligns `Field` rows whose cell count equals `cols.len()`; `None` is a
/// key/value or header-less table. `add` is the target the `a` key appends to.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Section {
    pub title: String,
    pub kind: SectionKind,
    pub headers: Option<Vec<&'static str>>,
    pub rows: Vec<TableRow>,
    pub add: Option<Field>,
    pub expand: Option<Expand>,
}

fn label(text: &str) -> Cell {
    Cell {
        field: Field::SectionHeader,
        kind: CellKind::Label,
        value: text.to_string(),
    }
}
fn text(field: Field, value: String) -> Cell {
    Cell {
        field,
        kind: CellKind::Text,
        value,
    }
}
fn enum_cell(field: Field, value: String) -> Cell {
    Cell {
        field,
        kind: CellKind::Enum,
        value,
    }
}
fn field_row(cells: Vec<Cell>) -> TableRow {
    TableRow {
        kind: RowKind::Field,
        indent: 0,
        cells,
        raw: String::new(),
        prefix: String::new(),
    }
}

/// A Body section's title row, drawn as the bold section title. Its single
/// non-editable `Label` cell carries the read title string.
fn title_row(title: String) -> TableRow {
    TableRow {
        kind: RowKind::Title,
        indent: 0,
        cells: vec![Cell {
            field: Field::SectionHeader,
            kind: CellKind::Label,
            value: title,
        }],
        raw: String::new(),
        prefix: String::new(),
    }
}
fn example_row(loc: BodyLoc, raw: String) -> TableRow {
    TableRow {
        kind: RowKind::Example,
        indent: 0,
        cells: vec![Cell {
            field: Field::BodyExample(loc),
            kind: CellKind::Label,
            value: String::new(),
        }],
        raw,
        prefix: String::new(),
    }
}

/// Builds a body section's rows: `lead` (its Title row) followed by the inline
/// example row.
fn body_rows(lead: Vec<TableRow>, loc: BodyLoc, example: &str) -> Vec<TableRow> {
    let mut rows = lead;
    rows.push(example_row(loc, example.to_string()));
    rows
}

/// Flattens the model into read-shaped display sections.
pub(crate) fn flatten(m: &EditModel, expanded: Option<Expand>) -> Vec<Section> {
    let mut out = Vec::new();

    // Header block: name, description, URL.
    let method_s = apic_core::json::method_str(&m.method);
    let url_expanded = expanded == Some(Expand::Url);
    let mut head_rows = vec![
        TableRow {
            kind: RowKind::Name,
            indent: 0,
            cells: vec![text(Field::Name, m.name.clone())],
            raw: String::new(),
            prefix: String::new(),
        },
        TableRow {
            kind: RowKind::Desc,
            indent: 0,
            cells: vec![text(Field::Description, m.description.clone())],
            raw: String::new(),
            prefix: String::new(),
        },
    ];
    let head_add: Option<Field> = None;
    if url_expanded {
        head_rows.push(field_row(vec![
            label("method"),
            enum_cell(Field::Method, method_s.clone()),
        ]));
        head_rows.push(field_row(vec![
            label("url"),
            text(Field::Url, m.url.clone()),
        ]));
    } else {
        head_rows.push(TableRow {
            kind: RowKind::UrlLine,
            indent: 0,
            cells: vec![
                enum_cell(Field::Method, method_s),
                Cell {
                    field: Field::Url,
                    kind: CellKind::Label,
                    value: m.url.clone(),
                },
            ],
            raw: String::new(),
            prefix: String::new(),
        });
    }
    out.push(Section {
        title: String::new(),
        kind: SectionKind::Header,
        headers: None,
        rows: head_rows,
        add: head_add,
        expand: Some(Expand::Url),
    });

    // QUERY
    let mut q_rows = vec![title_row("QUERY".to_string())];
    for (i, q) in m.query.iter().enumerate() {
        q_rows.push(field_row(vec![
            text(Field::QueryName(i), q.name.clone()),
            text(Field::QueryValue(i), q.value.clone()),
            text(Field::QueryDesc(i), q.description.clone()),
        ]));
    }
    out.push(Section {
        title: "QUERY".into(),
        kind: SectionKind::Table,
        headers: Some(vec!["NAME", "VALUE", "DESCRIPTION"]),
        rows: q_rows,
        add: Some(Field::QueryAdd),
        expand: None,
    });

    // HEADERS (no column header, like read)
    let mut h_rows = vec![title_row("HEADERS".to_string())];
    for (i, h) in m.headers.iter().enumerate() {
        h_rows.push(field_row(vec![
            text(Field::HeaderName(i), h.name.clone()),
            text(Field::HeaderValue(i), h.value.clone()),
        ]));
    }
    out.push(Section {
        title: "HEADERS".into(),
        kind: SectionKind::Table,
        headers: None,
        rows: h_rows,
        add: Some(Field::HeaderAdd),
        expand: None,
    });

    // REQUEST. `a` toggles the body on/off (RequestToggle); the body itself is
    // just the inline example JSON.
    match &m.request {
        Some(req) => {
            let lead = vec![title_row("REQUEST".to_string())];
            out.push(Section {
                title: String::new(),
                kind: SectionKind::Body,
                headers: None,
                rows: body_rows(lead, BodyLoc::Request, &req.example),
                add: Some(Field::RequestToggle),
                expand: None,
            });
        }
        None => out.push(Section {
            title: "REQUEST".to_string(),
            kind: SectionKind::Body,
            headers: None,
            rows: vec![title_row("REQUEST".to_string())],
            add: Some(Field::RequestToggle),
            expand: None,
        }),
    }

    // RESPONSE(s)
    if m.responses.is_empty() {
        out.push(Section {
            title: "RESPONSE".into(),
            kind: SectionKind::Table,
            headers: None,
            rows: vec![title_row("RESPONSE".to_string())],
            add: Some(Field::ResponseAdd),
            expand: None,
        });
    } else {
        for (i, r) in m.responses.iter().enumerate() {
            let title = format!("RESPONSE {} — {}", r.code, r.description);
            let mut lead = vec![title_row(title)];
            if expanded == Some(Expand::Response(i)) {
                lead.push(field_row(vec![
                    label("code"),
                    text(Field::ResponseCode(i), r.code.clone()),
                ]));
                lead.push(field_row(vec![
                    label("description"),
                    text(Field::ResponseDesc(i), r.description.clone()),
                ]));
            }
            out.push(Section {
                title: String::new(),
                kind: SectionKind::Body,
                headers: None,
                rows: body_rows(lead, BodyLoc::Response(i), &r.example),
                add: Some(Field::ResponseAdd),
                expand: Some(Expand::Response(i)),
            });
        }
    }

    // A trailing "+ add response" affordance so the cursor can land on it and
    // `a` appends a new response at any time (not just when there are zero).
    out.push(Section {
        title: String::new(),
        kind: SectionKind::Table,
        headers: None,
        rows: vec![field_row(vec![label("+ add response")])],
        add: Some(Field::ResponseAdd),
        expand: None,
    });

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use apic_core::json::json_get;

    fn model() -> EditModel {
        let c = json_get(
            r#"{ "name":"user","description":"User management","method":"GET",
                 "url":"https://api.example.com/user",
                 "query":[{"name":"id","value":"1","description":"User ID"}],
                 "headers":[{"name":"Content-Type","value":"application/json"}],
                 "responses":[{"code":200,"description":"ok","schema":[
                    {"name":"data","type":"object","default":null,"description":"d","required":false,
                     "properties":[{"name":"id","type":"int","default":null,"description":"d","required":true}]}
                 ],"example":{"status":200}}] }"#,
            None,
        )
        .unwrap();
        EditModel::from_contract(c)
    }

    /// The displayed title of a section: its `title` field, or for Body
    /// sections the value of the leading `Title` row.
    fn shown_title(s: &Section) -> String {
        if let Some(row) = s.rows.iter().find(|r| r.kind == RowKind::Title) {
            row.cells[0].value.clone()
        } else {
            s.title.clone()
        }
    }

    #[test]
    fn header_block_has_name_desc_and_collapsed_url() {
        let secs = flatten(&model(), None);
        let head = &secs[0];
        assert_eq!(head.kind, SectionKind::Header);
        assert!(head.rows.iter().any(|r| r.kind == RowKind::Name));
        assert!(head.rows.iter().any(|r| r.kind == RowKind::Desc));
        let url = head
            .rows
            .iter()
            .find(|r| r.kind == RowKind::UrlLine)
            .unwrap();
        // The built URL cell shows the assembled URL.
        assert!(
            url.cells
                .iter()
                .any(|c| c.value.contains("https://api.example.com/user"))
        );
    }

    #[test]
    fn url_expands_to_editable_url_row() {
        let secs = flatten(&model(), Some(Expand::Url));
        let head = &secs[0];
        assert!(head.rows.iter().all(|r| r.kind != RowKind::UrlLine));
        assert!(
            head.rows
                .iter()
                .any(|r| matches!(r.cells.last().map(|c| &c.field), Some(Field::Url)))
        );
        assert_eq!(head.add, None);
    }

    #[test]
    fn section_titles_match_read() {
        let secs = flatten(&model(), None);
        let titles: Vec<String> = secs.iter().map(shown_title).collect();
        assert!(titles.iter().any(|t| t == "QUERY"));
        assert!(titles.iter().any(|t| t == "HEADERS"));
        assert!(titles.iter().any(|t| t == "REQUEST"));
        assert!(titles.iter().any(|t| t.starts_with("RESPONSE 200 — ok")));
    }

    #[test]
    fn response_title_expands_to_editable_code_desc() {
        let secs = flatten(&model(), Some(Expand::Response(0)));
        let resp = secs
            .iter()
            .find(|s| s.expand == Some(Expand::Response(0)))
            .unwrap();
        assert!(resp.rows.iter().any(|r| matches!(
            r.cells.last().map(|c| &c.field),
            Some(Field::ResponseCode(0))
        )));
        assert!(resp.rows.iter().any(|r| matches!(
            r.cells.last().map(|c| &c.field),
            Some(Field::ResponseDesc(0))
        )));
    }

    #[test]
    fn add_targets_are_set() {
        let secs = flatten(&model(), None);
        let q = secs.iter().find(|s| s.title == "QUERY").unwrap();
        assert_eq!(q.add, Some(Field::QueryAdd));
        let h = secs.iter().find(|s| s.title == "HEADERS").unwrap();
        assert_eq!(h.add, Some(Field::HeaderAdd));
        assert!(h.headers.is_none()); // HEADERS has no column header, like read
    }

    #[test]
    fn response_has_inline_example_row() {
        let secs = flatten(&model(), None);
        let resp = secs
            .iter()
            .find(|s| s.expand == Some(Expand::Response(0)))
            .unwrap();
        assert!(
            resp.rows
                .iter()
                .any(|r| r.kind == RowKind::Example && r.raw.contains("status"))
        );
    }

    #[test]
    fn table_sections_have_selectable_title_rows() {
        let secs = flatten(&model(), None);
        for t in ["QUERY", "HEADERS"] {
            let s = secs.iter().find(|s| s.title == t).unwrap();
            assert!(
                s.rows
                    .first()
                    .map(|r| r.kind == RowKind::Title)
                    .unwrap_or(false),
                "{t} should start with a Title row"
            );
        }
    }

    #[test]
    fn trailing_add_response_affordance_present() {
        let secs = flatten(&model(), None);
        let aff = secs
            .iter()
            .find(|s| {
                s.add == Some(Field::ResponseAdd)
                    && s.rows.iter().any(|r| {
                        r.cells
                            .first()
                            .map(|c| c.value == "+ add response")
                            .unwrap_or(false)
                    })
            })
            .expect("a '+ add response' affordance section exists");
        assert_eq!(aff.add, Some(Field::ResponseAdd));
    }
}
