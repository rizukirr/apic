//! Minimal Postman Collection v2.0.0 model.
//!
//! Only the fields `apic convert` actually reads are declared; serde ignores
//! every other key in the export, so this parses any real-world v2.0
//! collection regardless of the Postman features it uses.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct Spec {
    #[serde(default)]
    pub(crate) item: Vec<Items>,
}

/// An entry in an `item` array: either a request (leaf) or a folder (group).
///
/// Disambiguated by required fields — a leaf has `request`, a folder has `item`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum Items {
    Item(Item),
    ItemGroup(ItemGroup),
}

/// A single request and its saved responses. The required `request` field is
/// what distinguishes a leaf from a folder during untagged matching.
#[derive(Debug, Deserialize)]
pub(crate) struct Item {
    pub(crate) name: Option<String>,
    pub(crate) description: Option<DescriptionUnion>,
    pub(crate) request: RequestClass,
    pub(crate) response: Option<Vec<ResponseClass>>,
}

/// A folder grouping nested items.
#[derive(Debug, Deserialize)]
pub(crate) struct ItemGroup {
    pub(crate) name: Option<String>,
    pub(crate) item: Vec<Items>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RequestClass {
    pub(crate) method: Option<String>,
    pub(crate) url: Option<Url>,
    pub(crate) header: Option<Vec<Header>>,
    pub(crate) body: Option<Body>,
}

/// A URL is either the literal string or a broken-down object with a `raw` form.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum Url {
    String(String),
    UrlClass(UrlClass),
}

#[derive(Debug, Deserialize)]
pub(crate) struct UrlClass {
    pub(crate) raw: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Header {
    #[serde(default)]
    pub(crate) key: String,
    #[serde(default)]
    pub(crate) value: String,
    pub(crate) disabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Body {
    pub(crate) raw: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ResponseClass {
    pub(crate) code: Option<i64>,
    pub(crate) status: Option<String>,
    pub(crate) body: Option<String>,
}

/// A description is either a raw string or an object holding the text.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum DescriptionUnion {
    Description(Description),
    String(String),
}

#[derive(Debug, Deserialize)]
pub(crate) struct Description {
    pub(crate) content: Option<String>,
}
