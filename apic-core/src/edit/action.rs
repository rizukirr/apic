//! UI-agnostic edits over an [`EditModel`].
//!
//! Every mutation a front-end can make to a contract is expressed as an
//! [`EditAction`] and applied through [`apply`]. Front-ends (the CLI/TUI table
//! handlers, a future GUI) translate their own input into these actions, so the
//! editing behavior, what an add/delete/toggle/generate actually does, lives in
//! one place and cannot drift between front-ends.
//!
//! Navigation and view state (cursor, focus, expanded regions) are NOT modeled
//! here; they belong to each front-end.

use super::address::{BodyLoc, Field};
use super::model::{
    EditBody, EditHeader, EditModel, EditQuery, EditResponse, EditSchema, EditVariable,
    example_from_schema,
};

/// A single edit to apply to an [`EditModel`].
#[derive(Debug, Clone, PartialEq)]
pub enum EditAction {
    /// Write a string `value` into the text field at `field`.
    SetText { field: Field, value: String },

    /// Flip the boolean field at `field` (e.g. a `required` flag).
    ToggleBool { field: Field },

    /// Append a new row/entity for an `*Add` field (or toggle the request body).
    Add { field: Field },

    /// Remove the row/entity addressed by `field`.
    Delete { field: Field },

    /// Cycle the HTTP method forward (`true`) or backward (`false`).
    CycleMethod { forward: bool },

    /// Toggle a body between `object` and `object[]`.
    ToggleBodyType { loc: BodyLoc },

    /// Fill a body's example buffer from its schema fields.
    GenerateExample { loc: BodyLoc },
}

/// Applies `action` to `model`, returning `true` when it changed the model
/// (or was a valid mutation target). Front-ends typically recompute their own
/// dirty/refresh state afterwards regardless of the return value.
pub fn apply(model: &mut EditModel, action: &EditAction) -> bool {
    match action {
        EditAction::SetText { field, value } => set_field(model, field, value.clone()),
        EditAction::ToggleBool { field } => toggle_bool(model, field),
        EditAction::Add { field } => add(model, field),
        EditAction::Delete { field } => delete(model, field),
        EditAction::CycleMethod { forward } => {
            cycle_method(model, *forward);
            true
        }
        EditAction::ToggleBodyType { loc } => toggle_body_type(model, loc),
        EditAction::GenerateExample { loc } => generate_example(model, loc),
    }
}

/// Writes a string `value` into the model at `field`. Returns `false` for a
/// field that is not a settable text target.
fn set_field(model: &mut EditModel, field: &Field, value: String) -> bool {
    match field {
        Field::Name => model.name = value,
        Field::Description => model.description = value,
        Field::Protocol => model.url.protocol = value,
        Field::Host => model.url.host = value,
        Field::PathSeg(i) => {
            if let Some(s) = model.url.path.get_mut(*i) {
                *s = value;
            }
        }
        Field::QueryName(i) => set_query(model, *i, |q| q.name = value.clone()),
        Field::QueryType(i) => set_query(model, *i, |q| q.dtype = value.clone()),
        Field::QueryDesc(i) => set_query(model, *i, |q| q.description = value.clone()),
        Field::VarName(i) => set_var(model, *i, |v| v.name = value.clone()),
        Field::VarType(i) => set_var(model, *i, |v| v.dtype = value.clone()),
        Field::VarDesc(i) => set_var(model, *i, |v| v.description = value.clone()),
        Field::HeaderName(i) => {
            if let Some(h) = model.headers.get_mut(*i) {
                h.name = value;
            }
        }
        Field::HeaderValue(i) => {
            if let Some(h) = model.headers.get_mut(*i) {
                h.value = value;
            }
        }
        Field::BodyDtype(BodyLoc::Request) => {
            if let Some(b) = model.request.as_mut() {
                b.dtype = value;
            }
        }
        Field::BodyDtype(BodyLoc::Response(r)) => {
            if let Some(b) = model.responses.get_mut(*r) {
                b.dtype = value;
            }
        }
        Field::ResponseCode(i) => {
            if let Some(r) = model.responses.get_mut(*i) {
                r.code = value;
            }
        }
        Field::ResponseDesc(i) => {
            if let Some(r) = model.responses.get_mut(*i) {
                r.description = value;
            }
        }
        Field::SchemaName(loc, p) => set_schema(model, loc, p, |s| s.name = value.clone()),
        Field::SchemaType(loc, p) => set_schema(model, loc, p, |s| s.dtype = value.clone()),
        Field::SchemaDesc(loc, p) => set_schema(model, loc, p, |s| s.description = value.clone()),
        Field::SchemaAccept(loc, p) => set_schema(model, loc, p, |s| s.accept = value.clone()),
        _ => return false,
    }
    true
}

fn set_query(model: &mut EditModel, i: usize, f: impl FnOnce(&mut EditQuery)) {
    if let Some(q) = model.url.query.get_mut(i) {
        f(q);
    }
}

fn set_var(model: &mut EditModel, i: usize, f: impl FnOnce(&mut EditVariable)) {
    if let Some(v) = model.url.variable.get_mut(i) {
        f(v);
    }
}

fn set_schema(
    model: &mut EditModel,
    loc: &BodyLoc,
    path: &[usize],
    f: impl FnOnce(&mut EditSchema),
) {
    let node = match loc {
        BodyLoc::Request => model.schema_at_mut_request(path),
        BodyLoc::Response(r) => model.schema_at_mut_response(*r, path),
    };
    if let Some(n) = node {
        f(n);
    }
}

/// Flips a boolean field. Returns `false` for a non-boolean target.
fn toggle_bool(model: &mut EditModel, field: &Field) -> bool {
    match field {
        Field::QueryRequired(i) => {
            if let Some(q) = model.url.query.get_mut(*i) {
                q.required = !q.required;
            }
        }
        Field::VarRequired(i) => {
            if let Some(v) = model.url.variable.get_mut(*i) {
                v.required = !v.required;
            }
        }
        Field::SchemaRequired(BodyLoc::Request, path) => {
            if let Some(n) = model.schema_at_mut_request(path) {
                n.required = !n.required;
            }
        }
        Field::SchemaRequired(BodyLoc::Response(r), path) => {
            if let Some(n) = model.schema_at_mut_response(*r, path) {
                n.required = !n.required;
            }
        }
        _ => return false,
    }
    true
}

/// Appends a new row/entity for an `*Add` field, or toggles the request body
/// for [`Field::RequestToggle`]. Returns `false` for a non-add field.
fn add(model: &mut EditModel, field: &Field) -> bool {
    match field {
        Field::PathAdd => model.url.path.push(String::new()),
        Field::QueryAdd => model.url.query.push(EditQuery {
            name: String::new(),
            dtype: String::new(),
            description: String::new(),
            required: false,
        }),
        Field::VarAdd => model.url.variable.push(EditVariable {
            name: String::new(),
            dtype: "string".to_string(),
            description: String::new(),
            required: false,
        }),
        Field::HeaderAdd => model.headers.push(EditHeader {
            name: String::new(),
            value: String::new(),
        }),
        Field::ResponseAdd => model.responses.push(EditResponse::blank()),
        Field::RequestToggle => {
            model.request = if model.request.is_some() {
                None
            } else {
                Some(EditBody::empty())
            };
        }
        Field::SchemaAdd(BodyLoc::Request, path) => {
            if let Some(children) = model.schema_children_mut_request(path) {
                children.push(EditSchema::blank());
            }
        }
        Field::SchemaAdd(BodyLoc::Response(r), path) => {
            if let Some(children) = model.schema_children_mut_response(*r, path) {
                children.push(EditSchema::blank());
            }
        }
        _ => return false,
    }
    true
}

/// Removes the row/entity addressed by `field`. Returns `false` for a field
/// that addresses nothing deletable.
fn delete(model: &mut EditModel, field: &Field) -> bool {
    match field {
        Field::PathSeg(i) => drop_at(&mut model.url.path, *i),
        Field::QueryName(i)
        | Field::QueryType(i)
        | Field::QueryDesc(i)
        | Field::QueryRequired(i) => drop_at(&mut model.url.query, *i),
        Field::VarName(i) | Field::VarType(i) | Field::VarDesc(i) | Field::VarRequired(i) => {
            drop_at(&mut model.url.variable, *i)
        }
        Field::HeaderName(i) | Field::HeaderValue(i) => drop_at(&mut model.headers, *i),
        Field::ResponseCode(i) | Field::ResponseDesc(i) => drop_at(&mut model.responses, *i),
        Field::SchemaName(loc, path)
        | Field::SchemaType(loc, path)
        | Field::SchemaDesc(loc, path)
        | Field::SchemaRequired(loc, path)
        | Field::SchemaAccept(loc, path) => {
            if let Some((last, parent)) = path.split_last() {
                let children = match loc {
                    BodyLoc::Request => model.schema_children_mut_request(parent),
                    BodyLoc::Response(r) => model.schema_children_mut_response(*r, parent),
                };
                if let Some(c) = children {
                    drop_at(c, *last);
                }
            }
        }
        _ => return false,
    }
    true
}

fn drop_at<T>(v: &mut Vec<T>, i: usize) {
    if i < v.len() {
        v.remove(i);
    }
}

/// Cycles the method enum forward/back.
fn cycle_method(model: &mut EditModel, forward: bool) {
    use crate::json::{method_all, method_str};
    let all = method_all();
    let cur = method_str(&model.method);
    let idx = all.iter().position(|m| method_str(m) == cur).unwrap_or(0);
    let next = if forward {
        (idx + 1) % all.len()
    } else {
        (idx + all.len() - 1) % all.len()
    };
    model.method = all[next].clone();
}

/// Toggles a request/response body type between `object` and `object[]`.
/// Anything else normalizes to `object[]`. Returns `false` when the body is
/// absent.
fn toggle_body_type(model: &mut EditModel, loc: &BodyLoc) -> bool {
    let cur = match loc {
        BodyLoc::Request => model.request.as_ref().map(|b| b.dtype.clone()),
        BodyLoc::Response(i) => model.responses.get(*i).map(|r| r.dtype.clone()),
    };
    let Some(cur) = cur else { return false };
    let next = if cur == "object[]" {
        "object"
    } else {
        "object[]"
    };
    set_field(model, &Field::BodyDtype(loc.clone()), next.to_string())
}

/// Fills a body's example buffer from its schema fields. An array body
/// (`object[]`) generates a one-element array. Returns `false` when the body is
/// absent or the example could not be rendered.
fn generate_example(model: &mut EditModel, loc: &BodyLoc) -> bool {
    let body = match loc {
        BodyLoc::Request => model
            .request
            .as_ref()
            .map(|b| (b.schema.clone(), b.dtype.clone())),
        BodyLoc::Response(i) => model
            .responses
            .get(*i)
            .map(|r| (r.schema.clone(), r.dtype.clone())),
    };
    let Some((schema, dtype)) = body else {
        return false;
    };
    let mut value = example_from_schema(&schema);
    if crate::json::parse_type(&dtype).1 {
        value = serde_json::Value::Array(vec![value]);
    }
    let Ok(text) = crate::template::render_pretty(&value) else {
        return false;
    };
    match loc {
        BodyLoc::Request => {
            if let Some(b) = model.request.as_mut() {
                b.example = text;
            }
        }
        BodyLoc::Response(i) => {
            if let Some(r) = model.responses.get_mut(*i) {
                r.example = text;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::json_get;

    fn model() -> EditModel {
        let c = json_get(
            r#"{ "name":"t","description":"d","method":"GET",
                 "url":{"protocol":"https","host":"h","path":["x"],
                        "query":[{"name":"page","type":"1","description":"d","required":false}],
                        "variable":[{"name":"id","type":"string","description":"d","required":false}]},
                 "headers":[{"name":"A","value":"B"}],
                 "request":{"type":"object","schema":[
                    {"name":"status","type":"int","default":null,"description":"d","required":true}
                 ]},
                 "responses":[{"code":200,"description":"ok","schema":[]}] }"#,
            None,
        )
        .unwrap();
        EditModel::from_contract(c)
    }

    #[test]
    fn set_text_writes_field() {
        let mut m = model();
        assert!(apply(
            &mut m,
            &EditAction::SetText {
                field: Field::Name,
                value: "renamed".into()
            }
        ));
        assert_eq!(m.name, "renamed");
    }

    #[test]
    fn toggle_bool_flips_required() {
        let mut m = model();
        assert!(!m.url.query[0].required);
        apply(
            &mut m,
            &EditAction::ToggleBool {
                field: Field::QueryRequired(0),
            },
        );
        assert!(m.url.query[0].required);
    }

    #[test]
    fn add_and_delete_header() {
        let mut m = model();
        apply(
            &mut m,
            &EditAction::Add {
                field: Field::HeaderAdd,
            },
        );
        assert_eq!(m.headers.len(), 2);
        apply(
            &mut m,
            &EditAction::Delete {
                field: Field::HeaderName(1),
            },
        );
        assert_eq!(m.headers.len(), 1);
    }

    #[test]
    fn cycle_method_advances() {
        let mut m = model();
        apply(&mut m, &EditAction::CycleMethod { forward: true });
        assert_ne!(crate::json::method_str(&m.method), "GET");
    }

    #[test]
    fn toggle_body_type_round_trips() {
        let mut m = model();
        apply(
            &mut m,
            &EditAction::ToggleBodyType {
                loc: BodyLoc::Request,
            },
        );
        assert_eq!(m.request.as_ref().unwrap().dtype, "object[]");
        apply(
            &mut m,
            &EditAction::ToggleBodyType {
                loc: BodyLoc::Request,
            },
        );
        assert_eq!(m.request.as_ref().unwrap().dtype, "object");
    }

    #[test]
    fn generate_example_fills_buffer() {
        let mut m = model();
        assert!(apply(
            &mut m,
            &EditAction::GenerateExample {
                loc: BodyLoc::Request
            }
        ));
        assert!(m.request.as_ref().unwrap().example.contains("\"status\""));
    }

    #[test]
    fn unhandled_target_returns_false() {
        let mut m = model();
        assert!(!apply(
            &mut m,
            &EditAction::SetText {
                field: Field::SectionHeader,
                value: "x".into()
            }
        ));
    }
}
