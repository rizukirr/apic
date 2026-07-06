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

use super::address::Field;
use super::model::{EditBody, EditHeader, EditModel, EditQuery, EditResponse};

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
    }
}

/// Writes a string `value` into the model at `field`. Returns `false` for a
/// field that is not a settable text target.
fn set_field(model: &mut EditModel, field: &Field, value: String) -> bool {
    match field {
        Field::Name => model.name = value,
        Field::Description => model.description = value,
        Field::Url => model.url = value,
        Field::QueryName(i) => set_query(model, *i, |q| q.name = value.clone()),
        Field::QueryValue(i) => set_query(model, *i, |q| q.value = value.clone()),
        Field::QueryDesc(i) => set_query(model, *i, |q| q.description = value.clone()),
        Field::ResponseHeaderName(r, i) => {
            if let Some(h) = model
                .responses
                .get_mut(*r)
                .and_then(|resp| resp.headers.get_mut(*i))
            {
                h.name = value;
            }
        }
        Field::ResponseHeaderValue(r, i) => {
            if let Some(h) = model
                .responses
                .get_mut(*r)
                .and_then(|resp| resp.headers.get_mut(*i))
            {
                h.value = value;
            }
        }
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
        _ => return false,
    }
    true
}

fn set_query(model: &mut EditModel, i: usize, f: impl FnOnce(&mut EditQuery)) {
    if let Some(q) = model.query.get_mut(i) {
        f(q);
    }
}

/// Flips a boolean field. Returns `false` for a non-boolean target.
fn toggle_bool(model: &mut EditModel, field: &Field) -> bool {
    match field {
        Field::HeaderRequired(i) => {
            if let Some(h) = model.headers.get_mut(*i) {
                h.required = !h.required;
            }
        }
        Field::QueryRequired(i) => {
            if let Some(q) = model.query.get_mut(*i) {
                q.required = !q.required;
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
        Field::QueryAdd => model.query.push(EditQuery {
            name: String::new(),
            value: String::new(),
            description: String::new(),
            required: false,
        }),
        Field::ResponseHeaderAdd(r) => {
            if let Some(resp) = model.responses.get_mut(*r) {
                resp.headers.push(EditHeader {
                    name: String::new(),
                    value: String::new(),
                    required: false,
                });
            }
        }
        Field::HeaderAdd => model.headers.push(EditHeader {
            name: String::new(),
            value: String::new(),
            required: false,
        }),
        Field::ResponseAdd => model.responses.push(EditResponse::blank()),
        Field::RequestToggle => {
            model.request = if model.request.is_some() {
                None
            } else {
                Some(EditBody::empty())
            };
        }
        _ => return false,
    }
    true
}

/// Removes the row/entity addressed by `field`. Returns `false` for a field
/// that addresses nothing deletable.
fn delete(model: &mut EditModel, field: &Field) -> bool {
    match field {
        Field::QueryName(i)
        | Field::QueryValue(i)
        | Field::QueryDesc(i)
        | Field::QueryRequired(i) => drop_at(&mut model.query, *i),
        Field::ResponseHeaderName(r, i) | Field::ResponseHeaderValue(r, i) => {
            if let Some(resp) = model.responses.get_mut(*r) {
                drop_at(&mut resp.headers, *i);
            }
        }
        Field::HeaderName(i) | Field::HeaderValue(i) | Field::HeaderRequired(i) => {
            drop_at(&mut model.headers, *i)
        }
        Field::ResponseCode(i) | Field::ResponseDesc(i) => drop_at(&mut model.responses, *i),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::json_get;

    fn model() -> EditModel {
        let c = json_get(
            r#"{ "name":"t","description":"d","method":"GET",
                 "url":"/x",
                 "query":[{"name":"page","value":"1","description":"d"}],
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
    fn set_query_value_writes() {
        let mut m = model();
        apply(
            &mut m,
            &EditAction::SetText {
                field: Field::QueryValue(0),
                value: "42".into(),
            },
        );
        assert_eq!(m.query[0].value, "42");
    }

    #[test]
    fn add_and_delete_response_header() {
        let mut m = model();
        apply(
            &mut m,
            &EditAction::Add {
                field: Field::ResponseHeaderAdd(0),
            },
        );
        assert_eq!(m.responses[0].headers.len(), 1);
        apply(
            &mut m,
            &EditAction::Delete {
                field: Field::ResponseHeaderName(0, 0),
            },
        );
        assert!(m.responses[0].headers.is_empty());
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
    fn toggle_header_and_query_required() {
        let mut m = model();
        apply(
            &mut m,
            &EditAction::ToggleBool {
                field: Field::HeaderRequired(0),
            },
        );
        assert!(m.headers[0].required);
        apply(
            &mut m,
            &EditAction::ToggleBool {
                field: Field::QueryRequired(0),
            },
        );
        assert!(m.query[0].required);
    }

    #[test]
    fn cycle_method_advances() {
        let mut m = model();
        apply(&mut m, &EditAction::CycleMethod { forward: true });
        assert_ne!(crate::json::method_str(&m.method), "GET");
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
