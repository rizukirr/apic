//! Minimal Postman Collection v1.0.0 model.
//!
//! Only the fields `apic convert` actually reads are declared; serde ignores
//! every other key in the export. v1 stores requests in a flat `requests`
//! array; folders reference them by id through their `order` list.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct Spec {
    #[serde(default)]
    pub(crate) requests: Vec<Request>,
    #[serde(default)]
    pub(crate) folders: Option<Vec<Folder>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Folder {
    #[serde(default)]
    pub(crate) name: String,

    /// Request ids belonging to this folder, in order.
    #[serde(default)]
    pub(crate) order: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Request {
    #[serde(default)]
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) method: String,

    /// Newline-separated `Key: Value` header blob.
    #[serde(default)]
    pub(crate) headers: String,
    #[serde(default)]
    pub(crate) url: String,
    pub(crate) description: Option<String>,

    /// Raw request body. Any JSON shape is accepted; only a string carries a
    /// payload apic maps (other shapes are treated as no body).
    #[serde(rename = "rawModeData")]
    pub(crate) raw_mode_data: Option<serde_json::Value>,
}
