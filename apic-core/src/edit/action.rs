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
use super::model::{EditBody, EditHeader, EditModel, EditPart, EditQuery, EditResponse};

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

    /// Switch the body at `loc` between JSON and multipart, since a body
    /// carries one or the other. Switching to multipart clears the JSON
    /// example (the request body becomes `None`) and leaves any parts as they
    /// are. Switching to JSON clears the parts and, for the request, gives it
    /// an empty JSON body.
    SetBodyKind { loc: BodyLoc, multipart: bool },
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
        EditAction::SetBodyKind { loc, multipart } => set_body_kind(model, loc, *multipart),
    }
}

/// The parts of the multipart body at `loc`, or `None` for a response index
/// that does not exist.
fn parts_mut<'a>(model: &'a mut EditModel, loc: &BodyLoc) -> Option<&'a mut Vec<EditPart>> {
    match loc {
        BodyLoc::Request => Some(&mut model.multipart),
        BodyLoc::Response(r) => Some(&mut model.responses.get_mut(*r)?.multipart),
    }
}

/// Runs `f` on part `i` of the body at `loc`. Returns `false` when the body or
/// the part does not exist.
fn with_part(
    model: &mut EditModel,
    loc: &BodyLoc,
    i: usize,
    f: impl FnOnce(&mut EditPart),
) -> bool {
    match parts_mut(model, loc).and_then(|parts| parts.get_mut(i)) {
        Some(part) => {
            f(part);
            true
        }
        None => false,
    }
}

/// Applies [`EditAction::SetBodyKind`]. Returns `false` for a response index
/// that does not exist.
fn set_body_kind(model: &mut EditModel, loc: &BodyLoc, multipart: bool) -> bool {
    match (loc, multipart) {
        (BodyLoc::Request, true) => model.request = None,
        (BodyLoc::Request, false) => {
            model.multipart.clear();
            model.request = Some(EditBody::empty());
        }
        (BodyLoc::Response(r), true) => match model.responses.get_mut(*r) {
            Some(resp) => resp.example.clear(),
            None => return false,
        },
        (BodyLoc::Response(r), false) => match model.responses.get_mut(*r) {
            Some(resp) => resp.multipart.clear(),
            None => return false,
        },
    }
    true
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
        Field::PartName(loc, i) => return with_part(model, loc, *i, |p| p.name = value),
        Field::PartValue(loc, i) => return with_part(model, loc, *i, |p| p.value = value),
        Field::PartFilename(loc, i) => return with_part(model, loc, *i, |p| p.filename = value),
        Field::PartContentType(loc, i) => {
            return with_part(model, loc, *i, |p| p.content_type = value);
        }
        Field::PartDesc(loc, i) => return with_part(model, loc, *i, |p| p.description = value),
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
        Field::PartRequired(loc, i) => {
            return with_part(model, loc, *i, |p| p.required = !p.required);
        }
        Field::PartIsFile(loc, i) => return with_part(model, loc, *i, toggle_is_file),
        _ => return false,
    }
    true
}

/// Turns a text part into a file part or back. A file part is one with a
/// filename, so the text value moves into the filename (`file` when blank)
/// and the filename moves back into the value on the way out.
fn toggle_is_file(part: &mut EditPart) {
    if part.filename.is_empty() {
        part.filename = if part.value.trim().is_empty() {
            "file".to_string()
        } else {
            std::mem::take(&mut part.value)
        };
        part.value.clear();
    } else {
        part.value = std::mem::take(&mut part.filename);
    }
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
        Field::PartAdd(loc) => match parts_mut(model, loc) {
            Some(parts) => parts.push(EditPart {
                name: String::new(),
                value: String::new(),
                filename: String::new(),
                content_type: String::new(),
                description: String::new(),
                required: false,
            }),
            None => return false,
        },
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
        Field::PartName(loc, i)
        | Field::PartValue(loc, i)
        | Field::PartFilename(loc, i)
        | Field::PartContentType(loc, i)
        | Field::PartDesc(loc, i)
        | Field::PartRequired(loc, i)
        | Field::PartIsFile(loc, i) => {
            return match parts_mut(model, loc) {
                Some(parts) if *i < parts.len() => {
                    parts.remove(*i);
                    true
                }
                _ => false,
            };
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
                 "request":{"status":0},
                 "responses":[{"code":200,"description":"ok"}] }"#,
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
    fn part_fields_edit_both_bodies() {
        let mut m = model();
        for loc in [BodyLoc::Request, BodyLoc::Response(0)] {
            assert!(apply(
                &mut m,
                &EditAction::Add {
                    field: Field::PartAdd(loc.clone())
                }
            ));
            let sets = [
                (Field::PartName(loc.clone(), 0), "avatar"),
                (Field::PartValue(loc.clone(), 0), "v"),
                (Field::PartFilename(loc.clone(), 0), "a.png"),
                (Field::PartContentType(loc.clone(), 0), "image/png"),
                (Field::PartDesc(loc.clone(), 0), "d"),
            ];
            for (field, value) in sets {
                assert!(apply(
                    &mut m,
                    &EditAction::SetText {
                        field,
                        value: value.into()
                    }
                ));
            }
            assert!(apply(
                &mut m,
                &EditAction::ToggleBool {
                    field: Field::PartRequired(loc.clone(), 0)
                }
            ));
            // Out of range is a no-op.
            assert!(!apply(
                &mut m,
                &EditAction::SetText {
                    field: Field::PartName(loc.clone(), 9),
                    value: "x".into()
                }
            ));
            assert!(!apply(
                &mut m,
                &EditAction::Delete {
                    field: Field::PartName(loc.clone(), 9)
                }
            ));
        }
        for parts in [&m.multipart, &m.responses[0].multipart] {
            let p = &parts[0];
            assert_eq!(p.name, "avatar");
            assert_eq!(p.value, "v");
            assert_eq!(p.filename, "a.png");
            assert_eq!(p.content_type, "image/png");
            assert_eq!(p.description, "d");
            assert!(p.required);
        }
        assert!(!apply(
            &mut m,
            &EditAction::Add {
                field: Field::PartAdd(BodyLoc::Response(5))
            }
        ));
        assert!(apply(
            &mut m,
            &EditAction::Delete {
                field: Field::PartDesc(BodyLoc::Response(0), 0)
            }
        ));
        assert!(m.responses[0].multipart.is_empty());
        assert_eq!(m.multipart.len(), 1);
    }

    #[test]
    fn part_is_file_moves_value_and_filename() {
        let mut m = model();
        let toggle = EditAction::ToggleBool {
            field: Field::PartIsFile(BodyLoc::Request, 0),
        };
        apply(
            &mut m,
            &EditAction::Add {
                field: Field::PartAdd(BodyLoc::Request),
            },
        );
        m.multipart[0].value = "photo.png".into();
        assert!(apply(&mut m, &toggle));
        assert_eq!(m.multipart[0].filename, "photo.png");
        assert!(m.multipart[0].value.is_empty());
        assert!(apply(&mut m, &toggle));
        assert_eq!(m.multipart[0].value, "photo.png");
        assert!(m.multipart[0].filename.is_empty());

        m.multipart[0].value.clear();
        apply(&mut m, &toggle);
        assert_eq!(m.multipart[0].filename, "file");
        assert!(m.multipart[0].value.is_empty());
    }

    #[test]
    fn set_body_kind_switches_both_ways() {
        let mut m = model();
        m.responses[0].example = "{}".into();
        for loc in [BodyLoc::Request, BodyLoc::Response(0)] {
            apply(
                &mut m,
                &EditAction::Add {
                    field: Field::PartAdd(loc),
                },
            );
        }

        assert!(apply(
            &mut m,
            &EditAction::SetBodyKind {
                loc: BodyLoc::Request,
                multipart: true
            }
        ));
        assert!(m.request.is_none());
        assert_eq!(m.multipart.len(), 1);
        assert!(apply(
            &mut m,
            &EditAction::SetBodyKind {
                loc: BodyLoc::Response(0),
                multipart: true
            }
        ));
        assert!(m.responses[0].example.is_empty());
        assert_eq!(m.responses[0].multipart.len(), 1);

        assert!(apply(
            &mut m,
            &EditAction::SetBodyKind {
                loc: BodyLoc::Request,
                multipart: false
            }
        ));
        assert!(m.multipart.is_empty());
        assert_eq!(m.request, Some(EditBody::empty()));
        assert!(apply(
            &mut m,
            &EditAction::SetBodyKind {
                loc: BodyLoc::Response(0),
                multipart: false
            }
        ));
        assert!(m.responses[0].multipart.is_empty());

        assert!(!apply(
            &mut m,
            &EditAction::SetBodyKind {
                loc: BodyLoc::Response(3),
                multipart: true
            }
        ));
    }

    #[test]
    fn a_multipart_model_saves_valid_json() {
        let mut m = model();
        apply(
            &mut m,
            &EditAction::SetBodyKind {
                loc: BodyLoc::Request,
                multipart: true,
            },
        );
        for _ in 0..2 {
            apply(
                &mut m,
                &EditAction::Add {
                    field: Field::PartAdd(BodyLoc::Request),
                },
            );
        }
        m.multipart[0].name = "caption".into();
        m.multipart[0].value = "hello".into();
        m.multipart[1].name = "avatar".into();
        m.multipart[1].value = "a.png".into();
        apply(
            &mut m,
            &EditAction::ToggleBool {
                field: Field::PartIsFile(BodyLoc::Request, 1),
            },
        );

        let out = m.to_json().expect("serializes");
        assert!(crate::json::validate(&out).is_ok());
        let back = json_get(&out, None).expect("reparses");
        let parts = back.multipart.expect("parts written");
        assert_eq!(parts[0].value, "hello");
        assert_eq!(parts[1].filename.as_deref(), Some("a.png"));
        assert!(back.request.is_none());
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
