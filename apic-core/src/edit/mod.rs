//! UI-agnostic contract editing: a mutable working model plus the set of edits
//! any front-end can apply to it.
//!
//! - [`model`] holds [`EditModel`], the free-text working copy of a contract and
//!   its `from_contract`/`to_json`/`save` conversions.
//! - [`address`] holds [`Field`]/[`BodyLoc`], the UI-agnostic way to point at a
//!   part of the model.
//! - [`action`] holds [`EditAction`] and [`apply`], the one place that defines
//!   what each edit does, shared by every front-end so behavior cannot drift.

pub mod action;
pub mod address;
pub mod model;

pub use action::{EditAction, apply};
pub use address::{BodyLoc, Field};
pub use model::{
    EditBody, EditHeader, EditModel, EditQuery, EditResponse, EditSchema, EditUrl, EditVariable,
    example_from_schema, schema_from_example,
};
