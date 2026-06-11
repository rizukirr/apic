//! Mutable working copy of a contract while it is being edited.
//!
//! Mirrors [`crate::json::JsonContent`] but stores free-text, numeric, and
//! example fields as raw `String` buffers so half-typed input is always a
//! valid in-memory state. Conversion to a real contract happens only on save.

use crate::json::Method;

/// The whole contract under edit.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EditModel {
    pub name: String,
    pub description: String, // empty => None
    pub method: Method,
    pub url: EditUrl,
    pub headers: Vec<EditHeader>,
    pub request: Option<EditBody>,
    pub responses: Vec<EditResponse>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EditUrl {
    pub protocol: String,
    pub host: String,
    pub path: Vec<String>,
    pub query: Vec<EditQuery>,
    pub variable: Vec<EditVariable>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EditHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EditQuery {
    pub name: String,
    pub value: String,
    pub description: String, // empty => None
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EditVariable {
    pub name: String,
    pub dtype: String,       // defaults to "string" on save when empty
    pub description: String, // empty => None
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EditBody {
    pub dtype: String, // e.g. "object", "object[]"
    pub schema: Vec<EditSchema>,
    pub example: String, // raw JSON text; empty => None
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EditResponse {
    pub code: String, // numeric text; parsed to u16 on save
    pub description: String,
    pub dtype: String,
    pub schema: Vec<EditSchema>,
    pub example: String, // raw JSON text; empty => None
}

/// A schema field. `properties` nests recursively for object types.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EditSchema {
    pub name: String,
    pub dtype: String,
    pub default: String, // empty => None
    pub description: String,
    pub required: bool,
    pub properties: Vec<EditSchema>, // empty => None on save
    pub accept: String,              // empty => None
}

impl EditBody {
    /// A request body with no fields and no example.
    pub fn empty() -> Self {
        EditBody {
            dtype: "object".to_string(),
            schema: Vec::new(),
            example: String::new(),
        }
    }
}

impl EditResponse {
    /// A blank response shell with a `200` default code.
    pub fn blank() -> Self {
        EditResponse {
            code: "200".to_string(),
            description: String::new(),
            dtype: "object".to_string(),
            schema: Vec::new(),
            example: String::new(),
        }
    }
}
