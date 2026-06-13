//! Postman Collection parsing.
//!
//! Reads a Postman Collection export (v1.0.0, v2.0.0, or v2.1.0) from JSON and
//! returns it as a [`PostmanCollection`]. This is the input side of `apic
//! convert`; mapping to apic contracts lives in [`crate::convert`].

use std::{fs::File, io::Read, path::Path};

use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::{Map, Value};

pub mod v1_0_0;
pub mod v2_0_0;
pub mod v2_1_0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SchemaVersion {
    major: u64,
    minor: u64,
    patch: u64,
}

/// Supported versions of the Postman Collection format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PostmanCollectionVersion {
    #[allow(non_camel_case_types)]
    V1_0_0,
    #[allow(non_camel_case_types)]
    V2_0_0,
    #[allow(non_camel_case_types)]
    V2_1_0,
}

/// A parsed Postman Collection, tagged by the version it was detected as.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum PostmanCollection {
    #[allow(non_camel_case_types)]
    V1_0_0(v1_0_0::Spec),
    #[allow(non_camel_case_types)]
    V2_0_0(v2_0_0::Spec),
    #[allow(non_camel_case_types)]
    V2_1_0(v2_1_0::Spec),
}

impl<'de> Deserialize<'de> for PostmanCollection {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        Self::from_value(value).map_err(de::Error::custom)
    }
}

impl PostmanCollection {
    fn from_value(value: Value) -> Result<Self, String> {
        match detect_version(&value)? {
            PostmanCollectionVersion::V1_0_0 => serde_json::from_value::<v1_0_0::Spec>(value)
                .map(Self::V1_0_0)
                .map_err(|err| format!("invalid v1.0.0 collection: {err}")),
            PostmanCollectionVersion::V2_0_0 => serde_json::from_value::<v2_0_0::Spec>(value)
                .map(Self::V2_0_0)
                .map_err(|err| format!("invalid v2.0.0 collection: {err}")),
            PostmanCollectionVersion::V2_1_0 => serde_json::from_value::<v2_1_0::Spec>(value)
                .map(Self::V2_1_0)
                .map_err(|err| format!("invalid v2.1.0 collection: {err}")),
        }
    }

    pub fn version(&self) -> PostmanCollectionVersion {
        match self {
            Self::V1_0_0(_) => PostmanCollectionVersion::V1_0_0,
            Self::V2_0_0(_) => PostmanCollectionVersion::V2_0_0,
            Self::V2_1_0(_) => PostmanCollectionVersion::V2_1_0,
        }
    }
}

/// Parse a Postman Collection from a file path.
pub fn from_path<P>(path: P) -> Result<PostmanCollection, String>
where
    P: AsRef<Path>,
{
    let path = path.as_ref();
    let file =
        File::open(path).map_err(|err| format!("failed to open {}: {err}", path.display()))?;
    from_reader(file)
}

/// Parse a Postman Collection from anything that implements `Read`.
pub fn from_reader<R>(mut read: R) -> Result<PostmanCollection, String>
where
    R: Read,
{
    let mut bytes = Vec::new();
    read.read_to_end(&mut bytes)
        .map_err(|err| format!("failed to read collection: {err}"))?;
    from_slice(&bytes)
}

/// Parse a Postman Collection from a byte slice.
pub fn from_slice(input: &[u8]) -> Result<PostmanCollection, String> {
    let value = serde_json::from_slice::<Value>(input)
        .map_err(|err| format!("collection is not valid JSON: {err}"))?;
    PostmanCollection::from_value(value)
}

fn detect_version(value: &Value) -> Result<PostmanCollectionVersion, String> {
    let object = value
        .as_object()
        .ok_or("expected the Postman Collection document root to be an object")?;

    if let Some(version) = version_from_schema(object)? {
        return Ok(version);
    }

    if is_v1_document(object) {
        return Ok(PostmanCollectionVersion::V1_0_0);
    }

    if looks_like_v2_document(object) {
        return Err("missing Postman Collection version; expected a v2 info.schema value \
                    or the v1 collection shape"
            .to_string());
    }

    Err("unrecognized Postman Collection document shape".to_string())
}

fn is_v1_document(object: &Map<String, Value>) -> bool {
    object.contains_key("id")
        && object.contains_key("name")
        && object.contains_key("order")
        && object.contains_key("requests")
}

fn looks_like_v2_document(object: &Map<String, Value>) -> bool {
    object.contains_key("info") || object.contains_key("item")
}

fn version_from_schema(
    object: &Map<String, Value>,
) -> Result<Option<PostmanCollectionVersion>, String> {
    let Some(schema) = object
        .get("info")
        .and_then(Value::as_object)
        .and_then(|info| info.get("schema"))
        .and_then(Value::as_str)
    else {
        return Ok(None);
    };

    let version = extract_schema_version(schema)
        .ok_or_else(|| format!("could not determine collection version from schema ({schema})"))?;

    match version {
        SchemaVersion {
            major: 2,
            minor: 0,
            patch: 0,
        } => Ok(Some(PostmanCollectionVersion::V2_0_0)),
        SchemaVersion {
            major: 2,
            minor: 1,
            patch: 0,
        } => Ok(Some(PostmanCollectionVersion::V2_1_0)),
        version => Err(format!(
            "unsupported Postman Collection version: {}.{}.{}",
            version.major, version.minor, version.patch
        )),
    }
}

fn extract_schema_version(schema: &str) -> Option<SchemaVersion> {
    schema
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '.'))
        .filter_map(|segment| segment.strip_prefix('v'))
        .find_map(parse_schema_version)
}

fn parse_schema_version(candidate: &str) -> Option<SchemaVersion> {
    let mut parts = candidate.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;

    if parts.next().is_some() {
        return None;
    }

    Some(SchemaVersion {
        major,
        minor,
        patch,
    })
}
