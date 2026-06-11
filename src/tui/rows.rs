//! Table model derived from `EditModel`.
//!
//! The TUI renders a list of `Section`s; each is a titled, optionally-columned
//! table of `TableRow`s of `Cell`s. Every `Cell` carries a `Field` address that
//! the edit handlers in `state.rs` use to locate the target in the model
//! (including the path through nested schema `properties`).

use crate::tui::model::{EditModel, EditSchema};

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
    QueryValue(usize),
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
    BodyExample(BodyLoc),
    SchemaName(BodyLoc, Vec<usize>),
    SchemaType(BodyLoc, Vec<usize>),
    SchemaDesc(BodyLoc, Vec<usize>),
    SchemaRequired(BodyLoc, Vec<usize>),
    SchemaAccept(BodyLoc, Vec<usize>),
    SchemaAdd(BodyLoc, Vec<usize>),
    ResponseCode(usize),
    ResponseDesc(usize),
    ResponseAdd,
    SectionHeader,
}

/// How a cell is edited.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CellKind {
    Label, // non-editable (column-1 field names, add/example prompts)
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

/// Row behavior on Enter.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RowKind {
    Data,    // enter cell-edit mode
    Add,     // Enter inserts a new entity (uses cells[0].field)
    Example, // Enter opens the JSON modal (uses cells[0].field)
}

/// One displayable table row.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TableRow {
    pub kind: RowKind,
    pub indent: u16,
    pub cells: Vec<Cell>,
}

/// A titled table. `headers: Some(cols)` renders a column-header line and aligns
/// `Data` rows whose cell count equals `cols.len()`; `None` is a key/value table.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Section {
    pub title: String,
    pub headers: Option<Vec<&'static str>>,
    pub rows: Vec<TableRow>,
}

fn label(text: &str) -> Cell {
    Cell {
        field: Field::SectionHeader,
        kind: CellKind::Label,
        value: text.to_string(),
    }
}
fn text_cell(field: Field, value: String) -> Cell {
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
        value: v.to_string(),
    }
}
fn data(cells: Vec<Cell>) -> TableRow {
    TableRow {
        kind: RowKind::Data,
        indent: 0,
        cells,
    }
}
fn data_i(indent: u16, cells: Vec<Cell>) -> TableRow {
    TableRow {
        kind: RowKind::Data,
        indent,
        cells,
    }
}
fn add_row(field: Field, prompt: &str) -> TableRow {
    TableRow {
        kind: RowKind::Add,
        indent: 0,
        cells: vec![Cell {
            field,
            kind: CellKind::Label,
            value: prompt.to_string(),
        }],
    }
}
fn add_row_i(indent: u16, field: Field, prompt: &str) -> TableRow {
    TableRow {
        kind: RowKind::Add,
        indent,
        cells: vec![Cell {
            field,
            kind: CellKind::Label,
            value: prompt.to_string(),
        }],
    }
}
fn example_row(field: Field, preview: String) -> TableRow {
    TableRow {
        kind: RowKind::Example,
        indent: 0,
        cells: vec![
            Cell {
                field: field.clone(),
                kind: CellKind::Label,
                value: "example".to_string(),
            },
            Cell {
                field,
                kind: CellKind::Label,
                value: preview,
            },
        ],
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
/// in the NAME cell for nested levels.
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
            text_cell(
                Field::SchemaName(loc.clone(), path.clone()),
                format!("{prefix}{}", f.name),
            ),
            text_cell(
                Field::SchemaType(loc.clone(), path.clone()),
                f.dtype.clone(),
            ),
            bool_cell(Field::SchemaRequired(loc.clone(), path.clone()), f.required),
        ];
        if has_accept {
            cells.push(text_cell(
                Field::SchemaAccept(loc.clone(), path.clone()),
                f.accept.clone(),
            ));
        }
        cells.push(text_cell(
            Field::SchemaDesc(loc.clone(), path.clone()),
            f.description.clone(),
        ));
        rows.push(data_i(depth as u16, cells));

        push_schema(rows, loc, &f.properties, path, depth + 1, has_accept);
        rows.push(add_row_i(
            (depth + 1) as u16,
            Field::SchemaAdd(loc.clone(), path.clone()),
            "+ add nested field",
        ));
        path.pop();
    }
    rows.push(add_row_i(
        depth as u16,
        Field::SchemaAdd(loc.clone(), path.clone()),
        "+ add field",
    ));
}

/// Builds the schema-fields section for a request/response body.
fn fields_section(title: String, loc: &BodyLoc, fields: &[EditSchema]) -> Section {
    let has_accept = any_accept(fields);
    let mut rows = Vec::new();
    let mut path = Vec::new();
    push_schema(&mut rows, loc, fields, &mut path, 0, has_accept);
    Section {
        title,
        headers: Some(schema_columns(has_accept)),
        rows,
    }
}

/// One-line preview of an example buffer.
fn example_preview(raw: &str) -> String {
    if raw.trim().is_empty() {
        "(empty)".to_string()
    } else {
        format!("{} …", raw.lines().next().unwrap_or("").trim())
    }
}

/// The ` · object[]` suffix on an array body title (reuses render::array_marker).
fn body_title(base: &str, dtype: &str) -> String {
    format!("{base}{}", crate::render::array_marker(dtype))
}

/// Flattens the model into display sections, in schema order.
pub(crate) fn flatten(m: &EditModel) -> Vec<Section> {
    let mut out = Vec::new();

    // META — key/value
    out.push(Section {
        title: "META".into(),
        headers: None,
        rows: vec![
            data(vec![label("name"), text_cell(Field::Name, m.name.clone())]),
            data(vec![
                label("description"),
                text_cell(Field::Description, m.description.clone()),
            ]),
            data(vec![
                label("method"),
                enum_cell(Field::Method, crate::json::method_str(&m.method)),
            ]),
        ],
    });

    // URL — key/value + path rows
    let mut url_rows = vec![
        data(vec![
            label("protocol"),
            text_cell(Field::Protocol, m.url.protocol.clone()),
        ]),
        data(vec![
            label("host"),
            text_cell(Field::Host, m.url.host.clone()),
        ]),
    ];
    for (i, seg) in m.url.path.iter().enumerate() {
        url_rows.push(data(vec![
            label("path"),
            text_cell(Field::PathSeg(i), seg.clone()),
        ]));
    }
    url_rows.push(add_row(Field::PathAdd, "+ add path segment"));
    out.push(Section {
        title: "URL".into(),
        headers: None,
        rows: url_rows,
    });

    // QUERY — NAME VALUE REQ DESCRIPTION
    let mut q_rows = Vec::new();
    for (i, q) in m.url.query.iter().enumerate() {
        q_rows.push(data(vec![
            text_cell(Field::QueryName(i), q.name.clone()),
            text_cell(Field::QueryValue(i), q.value.clone()),
            bool_cell(Field::QueryRequired(i), q.required),
            text_cell(Field::QueryDesc(i), q.description.clone()),
        ]));
    }
    q_rows.push(add_row(Field::QueryAdd, "+ add query"));
    out.push(Section {
        title: "QUERY".into(),
        headers: Some(vec!["NAME", "VALUE", "REQ", "DESCRIPTION"]),
        rows: q_rows,
    });

    // VARIABLES — NAME TYPE REQ DESCRIPTION
    let mut v_rows = Vec::new();
    for (i, v) in m.url.variable.iter().enumerate() {
        v_rows.push(data(vec![
            text_cell(Field::VarName(i), v.name.clone()),
            text_cell(Field::VarType(i), v.dtype.clone()),
            bool_cell(Field::VarRequired(i), v.required),
            text_cell(Field::VarDesc(i), v.description.clone()),
        ]));
    }
    v_rows.push(add_row(Field::VarAdd, "+ add variable"));
    out.push(Section {
        title: "VARIABLES".into(),
        headers: Some(vec!["NAME", "TYPE", "REQ", "DESCRIPTION"]),
        rows: v_rows,
    });

    // HEADERS — NAME VALUE
    let mut h_rows = Vec::new();
    for (i, h) in m.headers.iter().enumerate() {
        h_rows.push(data(vec![
            text_cell(Field::HeaderName(i), h.name.clone()),
            text_cell(Field::HeaderValue(i), h.value.clone()),
        ]));
    }
    h_rows.push(add_row(Field::HeaderAdd, "+ add header"));
    out.push(Section {
        title: "HEADERS".into(),
        headers: Some(vec!["NAME", "VALUE"]),
        rows: h_rows,
    });

    // REQUEST — key/value (type, example, toggle) + a separate FIELDS section
    match &m.request {
        None => out.push(Section {
            title: "REQUEST".into(),
            headers: None,
            rows: vec![add_row(
                Field::RequestToggle,
                "(no request body) — Enter to add",
            )],
        }),
        Some(req) => {
            out.push(Section {
                title: body_title("REQUEST", &req.dtype),
                headers: None,
                rows: vec![
                    data(vec![
                        label("type"),
                        text_cell(Field::BodyDtype(BodyLoc::Request), req.dtype.clone()),
                    ]),
                    example_row(
                        Field::BodyExample(BodyLoc::Request),
                        example_preview(&req.example),
                    ),
                    add_row(Field::RequestToggle, "(remove request body)"),
                ],
            });
            out.push(fields_section(
                "REQUEST · FIELDS".into(),
                &BodyLoc::Request,
                &req.schema,
            ));
        }
    }

    // RESPONSES — per response: key/value section + FIELDS section
    for (i, r) in m.responses.iter().enumerate() {
        out.push(Section {
            title: body_title(&format!("RESPONSE {}", r.code), &r.dtype),
            headers: None,
            rows: vec![
                data(vec![
                    label("code"),
                    text_cell(Field::ResponseCode(i), r.code.clone()),
                ]),
                data(vec![
                    label("description"),
                    text_cell(Field::ResponseDesc(i), r.description.clone()),
                ]),
                data(vec![
                    label("type"),
                    text_cell(Field::BodyDtype(BodyLoc::Response(i)), r.dtype.clone()),
                ]),
                example_row(
                    Field::BodyExample(BodyLoc::Response(i)),
                    example_preview(&r.example),
                ),
            ],
        });
        out.push(fields_section(
            format!("RESPONSE {} · FIELDS", r.code),
            &BodyLoc::Response(i),
            &r.schema,
        ));
    }
    out.push(Section {
        title: "RESPONSES".into(),
        headers: None,
        rows: vec![add_row(Field::ResponseAdd, "+ add response")],
    });

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::json_get;

    fn model() -> EditModel {
        let c = json_get(
            r#"{ "name":"t","method":"GET",
                 "url":{"protocol":"https","host":"h","path":["x"],
                        "query":[{"name":"page","value":"1","description":"d","required":false}]},
                 "headers":[{"name":"A","value":"B"}],
                 "request":{"type":"object","schema":[
                    {"name":"f","type":"object","default":null,"description":"d","required":true,
                     "properties":[{"name":"g","type":"string","default":null,"description":"d","required":false}]}
                 ]},
                 "responses":[{"code":200,"description":"ok","schema":[]}] }"#,
            None,
        )
        .unwrap();
        EditModel::from_contract(c)
    }

    #[test]
    fn flatten_lists_section_titles_in_order() {
        let secs = flatten(&model());
        let titles: Vec<String> = secs.iter().map(|s| s.title.clone()).collect();
        assert_eq!(titles[0], "META");
        assert_eq!(titles[1], "URL");
        assert_eq!(titles[2], "QUERY");
        assert_eq!(titles[3], "VARIABLES");
        assert_eq!(titles[4], "HEADERS");
        assert!(titles.iter().any(|t| t == "REQUEST"));
        assert!(titles.iter().any(|t| t == "REQUEST · FIELDS"));
        assert!(titles.iter().any(|t| t.starts_with("RESPONSE 200")));
    }

    #[test]
    fn query_row_has_four_cells() {
        let secs = flatten(&model());
        let q = secs.iter().find(|s| s.title == "QUERY").unwrap();
        let data_row = q.rows.iter().find(|r| r.kind == RowKind::Data).unwrap();
        assert_eq!(data_row.cells.len(), 4);
        assert_eq!(q.headers.as_ref().unwrap().len(), 4);
    }

    #[test]
    fn nested_field_has_indent_and_tree_prefix() {
        let secs = flatten(&model());
        let f = secs.iter().find(|s| s.title == "REQUEST · FIELDS").unwrap();
        let nested = f.rows.iter().find(|r| {
            r.indent == 1
                && matches!(&r.cells[0].field, Field::SchemaName(BodyLoc::Request, p) if p.len() == 2)
        });
        let nested = nested.expect("nested field row present");
        assert!(nested.cells[0].value.contains("└─") || nested.cells[0].value.contains("├─"));
    }

    #[test]
    fn example_and_add_rows_present() {
        let secs = flatten(&model());
        let req = secs.iter().find(|s| s.title == "REQUEST").unwrap();
        assert!(req.rows.iter().any(|r| r.kind == RowKind::Example));
        let resp_add = secs.iter().find(|s| s.title == "RESPONSES").unwrap();
        assert!(
            resp_add
                .rows
                .iter()
                .any(|r| matches!(r.cells[0].field, Field::ResponseAdd))
        );
    }
}
