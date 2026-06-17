//! Read-shaped table model derived from `EditModel`.
//!
//! `flatten` emits a `Vec<Section>` that mirrors exactly what `apic read`
//! prints (see `crate::render::Printer`): a bespoke header block followed by the
//! `VARIABLE`/`QUERY`/`HEADERS`/`REQUEST`/`RESPONSE` sections, each carrying an
//! `add: Option<Field>` so the `a` key knows what to append. Every editable
//! `Cell` carries a `Field` address that the handlers in `state.rs` use to
//! locate the target in the model (including the path through nested schema
//! `properties`).

use crate::tui::model::{EditModel, EditSchema, EditUrl};

/// Where a request/response schema lives.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BodyLoc {
    Request,
    Response(usize),
}

/// The editable target a cell points at. `SectionHeader` is a non-editable
/// placeholder used by `Label` cells.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Field {
    Name,
    Description,
    Method,
    Protocol,
    Host,
    PathSeg(usize),
    PathAdd,
    QueryName(usize),
    QueryType(usize),
    QueryDesc(usize),
    QueryRequired(usize),
    QueryAdd,
    VarName(usize),
    VarType(usize),
    VarDesc(usize),
    VarRequired(usize),
    VarAdd,
    HeaderName(usize),
    HeaderValue(usize),
    HeaderAdd,
    RequestToggle,
    BodyDtype(BodyLoc),
    ResponseCode(usize),
    ResponseDesc(usize),
    BodyExample(BodyLoc),
    SchemaName(BodyLoc, Vec<usize>),
    SchemaType(BodyLoc, Vec<usize>),
    SchemaDesc(BodyLoc, Vec<usize>),
    SchemaRequired(BodyLoc, Vec<usize>),
    SchemaAccept(BodyLoc, Vec<usize>),
    SchemaAdd(BodyLoc, Vec<usize>),
    ResponseAdd,
    SectionHeader,
}

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
    Request,
    Response(usize),
}

/// Row behavior / how a row is drawn.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RowKind {
    Name,    // header name line (drawn uppercased)
    Desc,    // header description line (drawn only when non-empty)
    UrlLine, // collapsed ` METHOD <built-url>`; Enter expands
    Title,   // a Body section's bold title line; Enter expands code/desc/type
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
fn bool_cell(field: Field, v: bool) -> Cell {
    Cell {
        field,
        kind: CellKind::Bool,
        value: if v { "✓".to_string() } else { String::new() },
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
fn field_row_i_prefixed(indent: u16, cells: Vec<Cell>, prefix: String) -> TableRow {
    TableRow {
        kind: RowKind::Field,
        indent,
        cells,
        raw: String::new(),
        prefix,
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

/// Inverse-free display URL, matching `render::build_url`'s rules over EditUrl.
fn built_url(u: &EditUrl) -> String {
    let path = u.path.join("/");
    let authority = if u.host.is_empty() {
        String::new()
    } else if u.protocol.is_empty() {
        u.host.clone()
    } else {
        format!("{}://{}", u.protocol, u.host)
    };
    match (authority.is_empty(), path.is_empty()) {
        (true, _) => format!("/{path}"),
        (false, true) => authority,
        (false, false) => format!("{}/{path}", authority.trim_end_matches('/')),
    }
}

/// True if any field at any depth declares `accept`.
fn any_accept(fields: &[EditSchema]) -> bool {
    fields
        .iter()
        .any(|f| !f.accept.trim().is_empty() || any_accept(&f.properties))
}

fn schema_columns(has_accept: bool) -> Vec<&'static str> {
    if has_accept {
        vec!["NAME", "TYPE", "REQ", "ACCEPT", "DESCRIPTION"]
    } else {
        vec!["NAME", "TYPE", "REQ", "DESCRIPTION"]
    }
}

/// Pushes schema field rows (recursively) into `rows`, with `├─`/`└─` prefixes
/// in the NAME cell for nested levels. Mirrors `render::push_field_rows`.
fn push_schema(
    rows: &mut Vec<TableRow>,
    loc: &BodyLoc,
    fields: &[EditSchema],
    path: &mut Vec<usize>,
    depth: usize,
    has_accept: bool,
) {
    for (i, f) in fields.iter().enumerate() {
        path.push(i);
        let prefix = if depth == 0 {
            String::new()
        } else {
            let branch = if i + 1 == fields.len() {
                "└─ "
            } else {
                "├─ "
            };
            format!("{}{branch}", "  ".repeat(depth - 1))
        };
        let mut cells = vec![
            text(Field::SchemaName(loc.clone(), path.clone()), f.name.clone()),
            text(
                Field::SchemaType(loc.clone(), path.clone()),
                f.dtype.clone(),
            ),
            bool_cell(Field::SchemaRequired(loc.clone(), path.clone()), f.required),
        ];
        if has_accept {
            cells.push(text(
                Field::SchemaAccept(loc.clone(), path.clone()),
                f.accept.clone(),
            ));
        }
        cells.push(text(
            Field::SchemaDesc(loc.clone(), path.clone()),
            f.description.clone(),
        ));
        rows.push(field_row_i_prefixed(depth as u16, cells, prefix));

        push_schema(rows, loc, &f.properties, path, depth + 1, has_accept);
        path.pop();
    }
}

/// Builds a body section's rows: `lead` (its Title row + any expanded editable
/// rows) followed by the schema field rows + the inline example row.
fn body_rows(
    lead: Vec<TableRow>,
    loc: BodyLoc,
    fields: &[EditSchema],
    example: &str,
) -> Vec<TableRow> {
    let has_accept = any_accept(fields);
    let mut rows = lead;
    let mut path = Vec::new();
    push_schema(&mut rows, &loc, fields, &mut path, 0, has_accept);
    rows.push(example_row(loc, example.to_string()));
    rows
}

/// Flattens the model into read-shaped display sections.
pub(crate) fn flatten(m: &EditModel, expanded: Option<Expand>) -> Vec<Section> {
    let mut out = Vec::new();

    // Header block: name, description, URL.
    let method_s = crate::json::method_str(&m.method);
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
    let mut head_add = None;
    if url_expanded {
        head_rows.push(field_row(vec![
            label("method"),
            enum_cell(Field::Method, method_s.clone()),
        ]));
        head_rows.push(field_row(vec![
            label("protocol"),
            text(Field::Protocol, m.url.protocol.clone()),
        ]));
        head_rows.push(field_row(vec![
            label("host"),
            text(Field::Host, m.url.host.clone()),
        ]));
        for (i, seg) in m.url.path.iter().enumerate() {
            head_rows.push(field_row(vec![
                label("path"),
                text(Field::PathSeg(i), seg.clone()),
            ]));
        }
        head_add = Some(Field::PathAdd);
    } else {
        head_rows.push(TableRow {
            kind: RowKind::UrlLine,
            indent: 0,
            cells: vec![
                enum_cell(Field::Method, method_s),
                Cell {
                    field: Field::Protocol,
                    kind: CellKind::Label,
                    value: built_url(&m.url),
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

    // VARIABLE
    let mut v_rows = vec![title_row("VARIABLE".to_string())];
    for (i, v) in m.url.variable.iter().enumerate() {
        v_rows.push(field_row(vec![
            text(Field::VarName(i), v.name.clone()),
            text(Field::VarType(i), v.dtype.clone()),
            bool_cell(Field::VarRequired(i), v.required),
            text(Field::VarDesc(i), v.description.clone()),
        ]));
    }
    out.push(Section {
        title: "VARIABLE".into(),
        kind: SectionKind::Table,
        headers: Some(vec!["NAME", "TYPE", "REQ", "DESCRIPTION"]),
        rows: v_rows,
        add: Some(Field::VarAdd),
        expand: None,
    });

    // QUERY
    let mut q_rows = vec![title_row("QUERY".to_string())];
    for (i, q) in m.url.query.iter().enumerate() {
        q_rows.push(field_row(vec![
            text(Field::QueryName(i), q.name.clone()),
            text(Field::QueryType(i), q.value.clone()),
            bool_cell(Field::QueryRequired(i), q.required),
            text(Field::QueryDesc(i), q.description.clone()),
        ]));
    }
    out.push(Section {
        title: "QUERY".into(),
        kind: SectionKind::Table,
        headers: Some(vec!["NAME", "VALUE", "REQ", "DESCRIPTION"]),
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

    // REQUEST. With no body, `a` toggles one on (RequestToggle); with a body,
    // `a` appends a top-level schema field (SchemaAdd). The title row expands
    // into an editable `type` row.
    match &m.request {
        Some(req) => {
            let title = format!("REQUEST{}", crate::render::array_marker(&req.dtype));
            let mut lead = vec![title_row(title)];
            if expanded == Some(Expand::Request) {
                lead.push(field_row(vec![
                    label("type"),
                    enum_cell(Field::BodyDtype(BodyLoc::Request), req.dtype.clone()),
                ]));
            }
            out.push(Section {
                title: String::new(),
                kind: SectionKind::Body,
                headers: Some(schema_columns(any_accept(&req.schema))),
                rows: body_rows(lead, BodyLoc::Request, &req.schema, &req.example),
                add: Some(Field::SchemaAdd(BodyLoc::Request, Vec::new())),
                expand: Some(Expand::Request),
            });
        }
        None => out.push(Section {
            title: "REQUEST".to_string(),
            kind: SectionKind::Body,
            headers: None,
            rows: vec![title_row("REQUEST".to_string())],
            add: Some(Field::RequestToggle),
            expand: Some(Expand::Request),
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
            let marker = crate::render::array_marker(&r.dtype);
            let title = format!("RESPONSE {} — {}{marker}", r.code, r.description);
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
                lead.push(field_row(vec![
                    label("type"),
                    enum_cell(Field::BodyDtype(BodyLoc::Response(i)), r.dtype.clone()),
                ]));
            }
            out.push(Section {
                title: String::new(),
                kind: SectionKind::Body,
                headers: Some(schema_columns(any_accept(&r.schema))),
                rows: body_rows(lead, BodyLoc::Response(i), &r.schema, &r.example),
                add: Some(Field::SchemaAdd(BodyLoc::Response(i), Vec::new())),
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
    use crate::json::json_get;

    fn model() -> EditModel {
        let c = json_get(
            r#"{ "name":"user","description":"User management","method":"GET",
                 "url":{"protocol":"https","host":"api.example.com","path":["user"],
                        "variable":[{"name":"id","type":"int","description":"User ID","required":false}]},
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
    fn url_expands_to_editable_parts() {
        let secs = flatten(&model(), Some(Expand::Url));
        let head = &secs[0];
        assert!(head.rows.iter().all(|r| r.kind != RowKind::UrlLine));
        assert!(
            head.rows
                .iter()
                .any(|r| matches!(r.cells.last().map(|c| &c.field), Some(Field::Protocol)))
        );
        assert!(
            head.rows
                .iter()
                .any(|r| matches!(r.cells.last().map(|c| &c.field), Some(Field::Host)))
        );
        assert_eq!(head.add, Some(Field::PathAdd));
    }

    #[test]
    fn section_titles_match_read() {
        let secs = flatten(&model(), None);
        let titles: Vec<String> = secs.iter().map(shown_title).collect();
        assert!(titles.iter().any(|t| t == "VARIABLE"));
        assert!(titles.iter().any(|t| t == "QUERY"));
        assert!(titles.iter().any(|t| t == "HEADERS"));
        assert!(titles.iter().any(|t| t == "REQUEST"));
        assert!(titles.iter().any(|t| t.starts_with("RESPONSE 200 — ok")));
    }

    #[test]
    fn response_title_expands_to_editable_code_desc_type() {
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
        assert!(resp.rows.iter().any(|r| matches!(
            r.cells.last().map(|c| &c.field),
            Some(Field::BodyDtype(BodyLoc::Response(0)))
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
    fn nested_field_has_tree_prefix_and_response_has_example_row() {
        let secs = flatten(&model(), None);
        let resp = secs
            .iter()
            .find(|s| s.expand == Some(Expand::Response(0)))
            .unwrap();
        assert!(resp.rows.iter().any(|r| {
            r.kind == RowKind::Field && (r.prefix.contains("├─") || r.prefix.contains("└─"))
        }));
        assert!(
            resp.rows
                .iter()
                .any(|r| r.kind == RowKind::Example && r.raw.contains("status"))
        );
    }

    #[test]
    fn table_sections_have_selectable_title_rows() {
        let secs = flatten(&model(), None);
        for t in ["VARIABLE", "QUERY", "HEADERS"] {
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

    #[test]
    fn nested_name_cell_is_bare_with_prefix_on_row() {
        let secs = flatten(&model(), None);
        let resp = secs
            .iter()
            .find(|s| s.title.starts_with("RESPONSE 200") || s.expand == Some(Expand::Response(0)))
            .unwrap();
        let nested = resp
            .rows
            .iter()
            .find(|r| matches!(&r.cells.first().map(|c| &c.field), Some(Field::SchemaName(BodyLoc::Response(_), p)) if p.len()==2))
            .unwrap();
        assert!(!nested.cells[0].value.contains('├') && !nested.cells[0].value.contains('└'));
        assert!(nested.prefix.contains('└') || nested.prefix.contains('├'));
    }
}
