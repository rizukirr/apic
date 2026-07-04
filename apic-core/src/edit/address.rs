//! Addresses into an [`EditModel`]: which part of a contract an edit targets.
//!
//! These are UI-agnostic so every front-end (CLI/TUI table cells, GUI widgets)
//! can describe an edit the same way and apply it through [`super::apply`].
//!
//! [`EditModel`]: super::EditModel

/// Where a request/response schema lives.
#[derive(Debug, Clone, PartialEq)]
pub enum BodyLoc {
    Request,
    Response(usize),
}

/// The editable target an edit points at. `SectionHeader` is a non-editable
/// placeholder used by label cells in table front-ends.
#[derive(Debug, Clone, PartialEq)]
pub enum Field {
    Name,
    Description,
    Method,
    Url,
    QueryName(usize),
    QueryValue(usize),
    QueryDesc(usize),
    QueryAdd,
    HeaderName(usize),
    HeaderValue(usize),
    HeaderAdd,
    RequestToggle,
    BodyDtype(BodyLoc),
    ResponseCode(usize),
    ResponseDesc(usize),
    ResponseHeaderName(usize, usize),
    ResponseHeaderValue(usize, usize),
    ResponseHeaderAdd(usize),
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
