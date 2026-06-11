//! Flattened navigable row model derived from `EditModel`.
//!
//! Navigation works over a `Vec<Row>`. A `Field` address tells the edit
//! handlers exactly which part of the model a row points at, including the path
//! through nested schema `properties`.

use crate::tui::model::{EditModel, EditSchema};

/// Where a request/response schema lives.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BodyLoc {
    Request,
    Response(usize),
}

/// The editable target a row points at.
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
    RequestToggle, // add/remove the request body section
    BodyDtype(BodyLoc),
    BodyExample(BodyLoc),
    SchemaName(BodyLoc, Vec<usize>),
    SchemaType(BodyLoc, Vec<usize>),
    SchemaDefault(BodyLoc, Vec<usize>),
    SchemaDesc(BodyLoc, Vec<usize>),
    SchemaRequired(BodyLoc, Vec<usize>),
    SchemaAccept(BodyLoc, Vec<usize>),
    SchemaAdd(BodyLoc, Vec<usize>), // add a child field under this path ([] = top level)
    ResponseCode(usize),
    ResponseDesc(usize),
    ResponseAdd,
    SectionHeader, // non-editable label row
}

/// How a row's value should be edited.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RowKind {
    Header,  // section title; not editable
    Text,    // inline text edit
    Enum,    // cycle through fixed choices
    Bool,    // toggle
    Example, // opens the modal textarea
    Add,     // pressing Enter inserts a new row
}

/// One displayable line.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Row {
    pub indent: u16,
    pub label: String,
    pub value: String,
    pub kind: RowKind,
    pub field: Field,
}

impl Row {
    fn header(label: &str, indent: u16) -> Row {
        Row {
            indent,
            label: label.to_string(),
            value: String::new(),
            kind: RowKind::Header,
            field: Field::SectionHeader,
        }
    }
}

fn push_schema_rows(
    out: &mut Vec<Row>,
    loc: &BodyLoc,
    fields: &[EditSchema],
    path: &mut Vec<usize>,
    indent: u16,
) {
    for (i, f) in fields.iter().enumerate() {
        path.push(i);
        out.push(Row {
            indent,
            label: "field".into(),
            value: f.name.clone(),
            kind: RowKind::Text,
            field: Field::SchemaName(loc.clone(), path.clone()),
        });
        out.push(Row {
            indent: indent + 1,
            label: "type".into(),
            value: f.dtype.clone(),
            kind: RowKind::Text,
            field: Field::SchemaType(loc.clone(), path.clone()),
        });
        out.push(Row {
            indent: indent + 1,
            label: "required".into(),
            value: f.required.to_string(),
            kind: RowKind::Bool,
            field: Field::SchemaRequired(loc.clone(), path.clone()),
        });
        out.push(Row {
            indent: indent + 1,
            label: "default".into(),
            value: f.default.clone(),
            kind: RowKind::Text,
            field: Field::SchemaDefault(loc.clone(), path.clone()),
        });
        out.push(Row {
            indent: indent + 1,
            label: "description".into(),
            value: f.description.clone(),
            kind: RowKind::Text,
            field: Field::SchemaDesc(loc.clone(), path.clone()),
        });
        out.push(Row {
            indent: indent + 1,
            label: "accept".into(),
            value: f.accept.clone(),
            kind: RowKind::Text,
            field: Field::SchemaAccept(loc.clone(), path.clone()),
        });
        // nested properties
        push_schema_rows(out, loc, &f.properties, path, indent + 1);
        out.push(Row {
            indent: indent + 1,
            label: "+ add nested field".into(),
            value: String::new(),
            kind: RowKind::Add,
            field: Field::SchemaAdd(loc.clone(), path.clone()),
        });
        path.pop();
    }
    out.push(Row {
        indent,
        label: "+ add field".into(),
        value: String::new(),
        kind: RowKind::Add,
        field: Field::SchemaAdd(loc.clone(), path.clone()),
    });
}

/// Flattens the whole model into rows in schema order.
pub(crate) fn flatten(m: &EditModel) -> Vec<Row> {
    let mut out = vec![Row::header("META", 0)];

    out.push(Row {
        indent: 1,
        label: "name".into(),
        value: m.name.clone(),
        kind: RowKind::Text,
        field: Field::Name,
    });
    out.push(Row {
        indent: 1,
        label: "description".into(),
        value: m.description.clone(),
        kind: RowKind::Text,
        field: Field::Description,
    });
    out.push(Row {
        indent: 1,
        label: "method".into(),
        value: crate::json::method_str(&m.method),
        kind: RowKind::Enum,
        field: Field::Method,
    });

    out.push(Row::header("URL", 0));
    out.push(Row {
        indent: 1,
        label: "protocol".into(),
        value: m.url.protocol.clone(),
        kind: RowKind::Text,
        field: Field::Protocol,
    });
    out.push(Row {
        indent: 1,
        label: "host".into(),
        value: m.url.host.clone(),
        kind: RowKind::Text,
        field: Field::Host,
    });
    for (i, seg) in m.url.path.iter().enumerate() {
        out.push(Row {
            indent: 1,
            label: "path".into(),
            value: seg.clone(),
            kind: RowKind::Text,
            field: Field::PathSeg(i),
        });
    }
    out.push(Row {
        indent: 1,
        label: "+ add path segment".into(),
        value: String::new(),
        kind: RowKind::Add,
        field: Field::PathAdd,
    });
    for (i, q) in m.url.query.iter().enumerate() {
        out.push(Row {
            indent: 1,
            label: "query name".into(),
            value: q.name.clone(),
            kind: RowKind::Text,
            field: Field::QueryName(i),
        });
        out.push(Row {
            indent: 2,
            label: "value".into(),
            value: q.value.clone(),
            kind: RowKind::Text,
            field: Field::QueryValue(i),
        });
        out.push(Row {
            indent: 2,
            label: "description".into(),
            value: q.description.clone(),
            kind: RowKind::Text,
            field: Field::QueryDesc(i),
        });
        out.push(Row {
            indent: 2,
            label: "required".into(),
            value: q.required.to_string(),
            kind: RowKind::Bool,
            field: Field::QueryRequired(i),
        });
    }
    out.push(Row {
        indent: 1,
        label: "+ add query".into(),
        value: String::new(),
        kind: RowKind::Add,
        field: Field::QueryAdd,
    });
    for (i, v) in m.url.variable.iter().enumerate() {
        out.push(Row {
            indent: 1,
            label: "var name".into(),
            value: v.name.clone(),
            kind: RowKind::Text,
            field: Field::VarName(i),
        });
        out.push(Row {
            indent: 2,
            label: "type".into(),
            value: v.dtype.clone(),
            kind: RowKind::Text,
            field: Field::VarType(i),
        });
        out.push(Row {
            indent: 2,
            label: "description".into(),
            value: v.description.clone(),
            kind: RowKind::Text,
            field: Field::VarDesc(i),
        });
        out.push(Row {
            indent: 2,
            label: "required".into(),
            value: v.required.to_string(),
            kind: RowKind::Bool,
            field: Field::VarRequired(i),
        });
    }
    out.push(Row {
        indent: 1,
        label: "+ add variable".into(),
        value: String::new(),
        kind: RowKind::Add,
        field: Field::VarAdd,
    });

    out.push(Row::header("HEADERS", 0));
    for (i, h) in m.headers.iter().enumerate() {
        out.push(Row {
            indent: 1,
            label: "header".into(),
            value: h.name.clone(),
            kind: RowKind::Text,
            field: Field::HeaderName(i),
        });
        out.push(Row {
            indent: 2,
            label: "value".into(),
            value: h.value.clone(),
            kind: RowKind::Text,
            field: Field::HeaderValue(i),
        });
    }
    out.push(Row {
        indent: 1,
        label: "+ add header".into(),
        value: String::new(),
        kind: RowKind::Add,
        field: Field::HeaderAdd,
    });

    out.push(Row::header("REQUEST", 0));
    match &m.request {
        None => {
            out.push(Row {
                indent: 1,
                label: "(no request body)".into(),
                value: "press Enter to add".into(),
                kind: RowKind::Add,
                field: Field::RequestToggle,
            });
        }
        Some(req) => {
            out.push(Row {
                indent: 1,
                label: "type".into(),
                value: req.dtype.clone(),
                kind: RowKind::Text,
                field: Field::BodyDtype(BodyLoc::Request),
            });
            let mut path = Vec::new();
            push_schema_rows(&mut out, &BodyLoc::Request, &req.schema, &mut path, 1);
            out.push(Row {
                indent: 1,
                label: "example".into(),
                value: example_preview(&req.example),
                kind: RowKind::Example,
                field: Field::BodyExample(BodyLoc::Request),
            });
            out.push(Row {
                indent: 1,
                label: "(remove request body)".into(),
                value: String::new(),
                kind: RowKind::Add,
                field: Field::RequestToggle,
            });
        }
    }

    out.push(Row::header("RESPONSES", 0));
    for (i, r) in m.responses.iter().enumerate() {
        out.push(Row {
            indent: 1,
            label: "code".into(),
            value: r.code.clone(),
            kind: RowKind::Text,
            field: Field::ResponseCode(i),
        });
        out.push(Row {
            indent: 2,
            label: "description".into(),
            value: r.description.clone(),
            kind: RowKind::Text,
            field: Field::ResponseDesc(i),
        });
        out.push(Row {
            indent: 2,
            label: "type".into(),
            value: r.dtype.clone(),
            kind: RowKind::Text,
            field: Field::BodyDtype(BodyLoc::Response(i)),
        });
        let mut path = Vec::new();
        push_schema_rows(&mut out, &BodyLoc::Response(i), &r.schema, &mut path, 2);
        out.push(Row {
            indent: 2,
            label: "example".into(),
            value: example_preview(&r.example),
            kind: RowKind::Example,
            field: Field::BodyExample(BodyLoc::Response(i)),
        });
    }
    out.push(Row {
        indent: 1,
        label: "+ add response".into(),
        value: String::new(),
        kind: RowKind::Add,
        field: Field::ResponseAdd,
    });

    out
}

/// One-line preview of an example buffer for the outline row.
fn example_preview(raw: &str) -> String {
    if raw.trim().is_empty() {
        "(empty)".to_string()
    } else {
        let first = raw.lines().next().unwrap_or("").trim();
        format!("{first} …")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::json_get;
    use crate::tui::model::EditModel;

    fn model() -> EditModel {
        let c = json_get(
            r#"{ "name":"t","method":"GET",
                 "url":{"protocol":"https","host":"h","path":["x"]},
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
    fn flatten_lists_sections_in_order() {
        let rows = flatten(&model());
        let headers: Vec<&str> = rows
            .iter()
            .filter(|r| r.kind == RowKind::Header)
            .map(|r| r.label.as_str())
            .collect();
        assert_eq!(
            headers,
            vec!["META", "URL", "HEADERS", "REQUEST", "RESPONSES"]
        );
    }

    #[test]
    fn nested_properties_produce_deeper_indent() {
        let rows = flatten(&model());
        // The nested field "g" must appear with a SchemaName address whose path
        // has length 2 (parent index, child index).
        let nested = rows.iter().any(|r| {
            matches!(
                &r.field, Field::SchemaName(BodyLoc::Request, p) if p.len() == 2
            )
        });
        assert!(nested);
    }
}
