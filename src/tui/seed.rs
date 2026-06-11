//! Create-time seeding of a new `EditModel`.
//!
//! Structure is the union of the builtin template and the project's
//! `.apic/template.json`; values are taken only from the project template.
//! Fields that exist solely because the builtin contributed them start empty.

use crate::tui::model::EditModel;
use serde_json::Value;

/// Builds a seed contract value: the builtin `contract.json` provides the full
/// structure, but every scalar is blanked unless the project `overlay` supplies
/// a value at that path. Arrays present in the overlay replace the builtin's.
fn seed_value(overlay: Option<&str>) -> Result<Value, String> {
    let mut base: Value = serde_json::from_str(crate::template::DEFAULT)
        .map_err(|err| format!("builtin template invalid: {err}"))?;
    strip_to_seed_defaults(&mut base);

    if let Some(overlay) = overlay {
        let over: Value = serde_json::from_str(overlay)
            .map_err(|err| format!(".apic/template.json is not valid JSON: {err}"))?;
        overlay_values(&mut base, &over);
    }
    Ok(base)
}

/// Prepares the builtin template as a create seed: keeps scalar default values
/// (name, description, method, protocol, host, types) but EMPTIES every array so
/// a new contract starts with no leftover items, and clears every `example`.
/// The project `.apic/template.json` overlay then replaces any arrays/values it
/// defines.
fn strip_to_seed_defaults(v: &mut Value) {
    match v {
        Value::Array(items) => items.clear(),
        Value::Object(map) => {
            for (k, val) in map.iter_mut() {
                if k == "example" {
                    *val = Value::Null;
                } else {
                    strip_to_seed_defaults(val);
                }
            }
        }
        _ => {}
    }
}

/// Overlays the project template's values onto the blanked structure. Object
/// keys merge; arrays and scalars from the overlay replace wholesale.
fn overlay_values(base: &mut Value, overlay: &Value) {
    match (base, overlay) {
        (Value::Object(base_map), Value::Object(over_map)) => {
            for (k, ov) in over_map {
                overlay_values(base_map.entry(k.clone()).or_insert(Value::Null), ov);
            }
        }
        (slot, ov) => *slot = ov.clone(),
    }
}

/// Produces the seed `EditModel` for `apic create`.
///
/// `overlay` is the contents of `.apic/template.json` when present. Falls back
/// to the blanked builtin structure when absent. The seed must parse as a valid
/// contract (the builtin guarantees the required fields exist).
pub(crate) fn seed_model(overlay: Option<&str>) -> Result<EditModel, String> {
    let value = seed_value(overlay)?;
    let text = serde_json::to_string(&value).map_err(|err| format!("seed render failed: {err}"))?;
    let contract = crate::json::json_get(&text, None)
        .map_err(|err| format!("seed is not a valid contract: {err}"))?;
    Ok(EditModel::from_contract(contract))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_overlay_keeps_scalars_empties_arrays() {
        let m = seed_model(None).unwrap();
        // The builtin template's scalar defaults are kept.
        assert_eq!(m.name, "endpoint-name");
        assert_eq!(m.url.protocol, "https");
        assert_eq!(m.url.host, "api.example.com");
        // Every array starts empty.
        assert!(m.url.path.is_empty());
        assert!(m.url.query.is_empty());
        assert!(m.url.variable.is_empty());
        assert!(m.headers.is_empty());
        assert!(m.responses.is_empty());
        assert!(m.request.as_ref().unwrap().schema.is_empty());
    }

    #[test]
    fn overlay_values_are_kept() {
        let overlay = r#"{ "name": "real-endpoint",
            "url": { "host": "api.real.com" } }"#;
        let m = seed_model(Some(overlay)).unwrap();
        assert_eq!(m.name, "real-endpoint");
        assert_eq!(m.url.host, "api.real.com");
        // A scalar only the builtin defines (protocol) is kept, NOT blanked.
        assert_eq!(m.url.protocol, "https");
        assert!(m.url.path.is_empty());
    }

    #[test]
    fn seed_is_a_valid_contract() {
        // Round-trips through to_json without error.
        let m = seed_model(None).unwrap();
        assert!(m.to_json().is_ok());
    }
}
