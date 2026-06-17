//! Minimal Postman Collection v2.0.0 model.
//!
//! Only the fields `apic convert` actually reads are declared; serde ignores
//! every other key in the export, so this parses any real-world v2.0
//! collection regardless of the Postman features it uses.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Spec {
    #[serde(default)]
    pub item: Vec<Items>,
}

/// An entry in an `item` array: either a request (leaf) or a folder (group).
///
/// Disambiguated by required fields — a leaf has `request`, a folder has `item`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Items {
    Item(Item),
    ItemGroup(ItemGroup),
}

/// A single request and its saved responses. The required `request` field is
/// what distinguishes a leaf from a folder during untagged matching.
#[derive(Debug, Deserialize)]
pub struct Item {
    pub name: Option<String>,
    pub description: Option<DescriptionUnion>,
    pub request: RequestClass,
    pub response: Option<Vec<ResponseClass>>,
}

/// A folder grouping nested items.
#[derive(Debug, Deserialize)]
pub struct ItemGroup {
    pub name: Option<String>,
    pub item: Vec<Items>,
}

#[derive(Debug, Deserialize)]
pub struct RequestClass {
    pub method: Option<String>,
    pub url: Option<Url>,
    pub header: Option<Vec<Header>>,
    pub body: Option<Body>,
}

/// A URL is either the literal string or a broken-down object with a `raw` form.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Url {
    String(String),
    UrlClass(UrlClass),
}

#[derive(Debug, Deserialize)]
pub struct UrlClass {
    pub raw: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Header {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub value: String,
    pub disabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct Body {
    pub raw: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ResponseClass {
    pub code: Option<i64>,
    pub status: Option<String>,
    pub body: Option<String>,
}

/// A description is either a raw string or an object holding the text.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum DescriptionUnion {
    Description(Description),
    String(String),
}

#[derive(Debug, Deserialize)]
pub struct Description {
    pub content: Option<String>,
}
