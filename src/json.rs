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
#[derive(Debug)]
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

/// Validates that `json` parses as a well-formed contract.
///
/// # Errors
///
/// Returns the parse error (with line/column) when the document does not
/// conform to the contract schema.
pub fn validate(json: &str) -> Result<(), serde_json::Error> {
    serde_json::from_str::<JsonContent>(json).map(|_| ())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const CONTRACT: &str = r#"{
        "name": "t",
        "method": "GET",
        "path": "/t",
        "headers": [],
        "responses": [
            { "code": 200, "description": "ok", "schema": [] },
            { "code": 404, "description": "no", "schema": [] }
        ]
    }"#;

    #[test]
    fn json_get_returns_all_responses_when_status_is_none() {
        let c = json_get(CONTRACT, None).unwrap();
        assert_eq!(c.responses.len(), 2);
        assert_eq!(c.name, "t");
    }

    #[test]
    fn json_get_filters_to_a_single_status() {
        let c = json_get(CONTRACT, Some(404)).unwrap();
        assert_eq!(c.responses.len(), 1);
        assert_eq!(c.responses[0].code, 404);
    }

    #[test]
    fn json_get_returns_empty_when_status_matches_nothing() {
        let c = json_get(CONTRACT, Some(500)).unwrap();
        assert!(c.responses.is_empty());
    }

    #[test]
    fn validate_accepts_well_formed_contract() {
        assert!(validate(CONTRACT).is_ok());
    }

    #[test]
    fn validate_rejects_missing_required_field() {
        // Missing `method`, `path`, `headers`, `responses`.
        assert!(validate(r#"{ "name": "x" }"#).is_err());
    }

    #[test]
    fn json_get_errors_on_invalid_json() {
        assert!(json_get("{ not json", None).is_err());
    }

    #[test]
    fn json_get_rejects_deeply_nested_input_without_overflowing() {
        // serde_json enforces a recursion limit, so a pathologically nested
        // document returns an error instead of overflowing the stack.
        let deep = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
        assert!(json_get(&deep, None).is_err());
    }

    /// Creates a unique, empty temp directory for a single test and removes any
    /// leftover from a previous run.
    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("apic_test_{tag}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn scan_returns_full_paths_at_depth_zero() {
        let root = temp_dir("scan_depth0");
        fs::create_dir_all(root.join("a")).unwrap();
        fs::write(root.join("a/x.json"), "{}").unwrap();
        fs::write(root.join("a/y.json"), "{}").unwrap();

        let files = scan_json_file(&root, 0, true).unwrap();
        assert_eq!(files.len(), 2);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn scan_dedups_directory_prefixes_at_depth_one() {
        let root = temp_dir("scan_depth1");
        fs::create_dir_all(root.join("a")).unwrap();
        fs::write(root.join("a/x.json"), "{}").unwrap();
        fs::write(root.join("a/y.json"), "{}").unwrap();

        // Two files under `a/` truncate to the same prefix; expect one entry.
        let files = scan_json_file(&root, 1, false).unwrap();
        assert_eq!(files, vec![PathBuf::from("a")]);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn scan_reports_not_found_when_empty() {
        let root = temp_dir("scan_empty");
        assert!(matches!(
            scan_json_file(&root, 0, true),
            Err(JsonScanFileErr::NotFound)
        ));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn scan_reports_depth_too_large() {
        let root = temp_dir("scan_toodeep");
        fs::write(root.join("x.json"), "{}").unwrap();
        assert!(matches!(
            scan_json_file(&root, 99, true),
            Err(JsonScanFileErr::DepthTooLarge { .. })
        ));
        fs::remove_dir_all(&root).unwrap();
    }
}
