//! Discovery of JSON contract files beneath a project root.

use crate::file::{FindFileResult, find_file_by_ext_downward};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonContent {
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) query: Option<Vec<Query>>,
    pub(crate) params: Option<Vec<Param>>,
    pub(crate) headers: Vec<Header>,
    pub(crate) request: Option<Vec<Request>>,
    pub(crate) responses: Vec<Response>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Param {
    pub(crate) name: String,
    pub(crate) value: String,
    pub(crate) description: Option<String>,
    pub(crate) required: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Query {
    pub(crate) name: String,
    pub(crate) value: String,
    pub(crate) description: Option<String>,
    pub(crate) required: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Header {
    pub(crate) name: String,
    pub(crate) value: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub(crate) name: String,
    #[serde(alias = "type")]
    pub(crate) dtype: String,
    pub(crate) default: Option<String>,
    pub(crate) description: String,
    pub(crate) required: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub code: u16,
    pub description: String,
    pub schema: Vec<Schema>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Schema {
    pub(crate) name: String,
    #[serde(alias = "type")]
    pub(crate) dtype: String,
    pub(crate) default: Option<String>,
    pub(crate) description: String,
    pub(crate) required: bool,
    pub(crate) properties: Option<Vec<Schema>>,
}

/// Errors returned by [`scan_json_file`].
pub enum JsonScanFileErr {
    /// No JSON files were found under the root.
    NotFound,
    /// The requested depth exceeds the deepest available file.
    DepthTooLarge { requested: usize, max: usize },
}

/// Finds `.json` files under `root` and truncates their paths to `depth`.
///
/// `depth` counts path components below the root. A depth of `0` returns the
/// full file paths; a larger depth truncates each path to that many components
/// past the root, which can yield directory prefixes rather than full files.
///
/// # Errors
///
/// Returns [`JsonScanFileErr::NotFound`] when no JSON files exist, or
/// [`JsonScanFileErr::DepthTooLarge`] when `depth` exceeds the deepest match.
pub fn scan_json_file(
    root: &Path,
    depth: usize,
    is_absolute: bool,
) -> Result<Vec<PathBuf>, JsonScanFileErr> {
    let json_file = match find_file_by_ext_downward(root.to_path_buf(), &["json"]) {
        FindFileResult::Found(files) => files,
        FindFileResult::NotFound => return Err(JsonScanFileErr::NotFound),
    };

    let min_depth = root.iter().count();
    let max_depth = json_file
        .iter()
        .map(|p| p.iter().count() - min_depth)
        .max()
        .unwrap_or(0);

    if depth > max_depth {
        return Err(JsonScanFileErr::DepthTooLarge {
            requested: depth,
            max: max_depth,
        });
    }

    let mut stripped_json_files: Vec<PathBuf> = Vec::new();
    for rel in json_file {
        let p: PathBuf = if depth == 0 {
            rel
        } else {
            rel.iter().take(depth + min_depth).collect()
        };

        let p = if is_absolute {
            p
        } else {
            match p.strip_prefix(root) {
                Ok(p) => p.to_path_buf(),
                Err(_) => p,
            }
        };

        // Truncation can map several files to the same directory prefix;
        // keep each result only once.
        if !stripped_json_files.contains(&p) {
            stripped_json_files.push(p);
        }
    }

    Ok(stripped_json_files)
}

/// Parses a JSON contract, keeping only the responses whose code matches
/// `status` (all responses when `status` is `None`).
pub fn json_get(json: &str, status: Option<u16>) -> Result<JsonContent, serde_json::Error> {
    let mut json_content: JsonContent = serde_json::from_str(json)?;
    if let Some(status) = status {
        json_content.responses.retain(|r| r.code == status);
    }
    Ok(json_content)
}
