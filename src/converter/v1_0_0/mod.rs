//! Minimal Postman Collection v1.0.0 model.
//!
//! Only the fields `apic convert` actually reads are declared; serde ignores
//! every other key in the export. v1 stores requests in a flat `requests`
//! array; folders reference them by id through their `order` list.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Spec {
    #[serde(default)]
    pub requests: Vec<Request>,
    #[serde(default)]
    pub folders: Option<Vec<Folder>>,
}

#[derive(Debug, Deserialize)]
pub struct Folder {
    #[serde(default)]
    pub name: String,
    /// Request ids belonging to this folder, in order.
    #[serde(default)]
    pub order: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Request {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub method: String,
    /// Newline-separated `Key: Value` header blob.
    #[serde(default)]
    pub headers: String,
    #[serde(default)]
    pub url: String,
    pub description: Option<String>,
    /// Raw request body. Any JSON shape is accepted; only a string carries a
    /// payload apic maps (other shapes are treated as no body).
    #[serde(rename = "rawModeData")]
    pub raw_mode_data: Option<serde_json::Value>,
}
