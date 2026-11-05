//! Addresses into an [`EditModel`]: which part of a contract an edit targets.
//!
//! These are UI-agnostic so every front-end (CLI flags, GUI widgets)
//! can describe an edit the same way and apply it through [`super::apply`].
//!
//! [`EditModel`]: super::EditModel

/// Which body (request, or a given response) an edit targets.
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
    QueryRequired(usize),
    QueryAdd,
    HeaderName(usize),
    HeaderValue(usize),
    HeaderRequired(usize),
    HeaderAdd,
    RequestToggle,
    ResponseCode(usize),
    ResponseDesc(usize),
    ResponseHeaderName(usize, usize),
    ResponseHeaderValue(usize, usize),
    ResponseHeaderAdd(usize),
    BodyExample(BodyLoc),
    /// The name of part `usize` in the multipart body at `BodyLoc`.
    PartName(BodyLoc, usize),
    /// The text value of part `usize`. Blank for a file part.
    PartValue(BodyLoc, usize),
    /// The filename of part `usize`. Set only on a file part.
    PartFilename(BodyLoc, usize),
    /// The part's own media type, e.g. `image/png`.
    PartContentType(BodyLoc, usize),
    /// The description of part `usize`.
    PartDesc(BodyLoc, usize),
    /// Whether part `usize` is required.
    PartRequired(BodyLoc, usize),
    /// Whether part `usize` is a file part rather than a text part.
    PartIsFile(BodyLoc, usize),
    /// Appends a blank part to the multipart body at `BodyLoc`.
    PartAdd(BodyLoc),
    ResponseAdd,
    SectionHeader,
}
