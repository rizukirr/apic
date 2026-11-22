//! `apic export`: write apic contracts out as a Postman collection or, in
//! [`openapi`], an OpenAPI document.
//!
//! The Postman mapping is the reverse of [`crate::convert`]: each folder
//! becomes a Postman folder, each contract a request, and each documented
//! response a saved example. Variables are rewritten to Postman's `{{name}}`
//! form and path params to `:name`.

pub mod openapi;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::json::{Header, JsonContent, Part, method_str};
use crate::vars::{Vars, tokens};

/// The schema URL that marks a Postman Collection v2.1 document.
const POSTMAN_SCHEMA: &str = "https://schema.getpostman.com/json/collection/v2.1.0/collection.json";

/// One contract to export.
pub struct ExportItem {
    /// The contract's path relative to the working directory. Its folders
    /// become Postman folders.
    pub rel_path: PathBuf,
    pub contract: JsonContent,
}

/// Rewrites apic variables in `text` to Postman's form: `$name`, `${name}`
/// and `{{name}}` become `{{name}}`, and `$$` becomes `$`. With `path_params`,
/// a `{name}` path param becomes `:name`; otherwise it is left as written.
/// Every variable name seen is added to `used`.
fn to_postman(text: &str, path_params: bool, used: &mut BTreeSet<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for token in tokens(text) {
        out.push_str(&text[at..token.range.start].replace("$$", "$"));
        let raw = &text[token.range.clone()];
        if raw.starts_with('$') || raw.starts_with("{{") {
            used.insert(token.name.clone());
            out.push_str(&format!("{{{{{}}}}}", token.name));
        } else if path_params {
            out.push(':');
            out.push_str(&token.name);
        } else {
            out.push_str(raw);
        }
        at = token.range.end;
    }
    out.push_str(&text[at..].replace("$$", "$"));
    out
}

/// Splits a Postman raw URL into its `host` and `path` arrays. A URL with no
/// scheme, such as `{{base}}/users`, takes its first segment as the host.
fn url_parts(raw: &str) -> (Option<&str>, &str, Vec<&str>) {
    let (protocol, rest) = match raw.split_once("://") {
        Some((scheme, rest)) => (Some(scheme), rest),
        None => (None, raw),
    };
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let path = path.split('/').filter(|s| !s.is_empty()).collect();
    (protocol, host, path)
}

/// The Postman request object for a contract, with variables rewritten.
fn request(contract: &JsonContent, used: &mut BTreeSet<String>) -> Value {
    let raw_base = to_postman(&contract.url, true, used);
    let query: Vec<Value> = contract
        .query
        .iter()
        .map(|q| {
            let mut entry = json!({
                "key": to_postman(&q.name, false, used),
                "value": to_postman(&q.value, false, used),
            });
            if let Some(d) = &q.description {
                entry["description"] = json!(d);
            }
            entry
        })
        .collect();
    let raw = if query.is_empty() {
        raw_base.clone()
    } else {
        let pairs: Vec<String> = query
            .iter()
            .map(|q| format!("{}={}", str_of(&q["key"]), str_of(&q["value"])))
            .collect();
        format!("{raw_base}?{}", pairs.join("&"))
    };

    let (protocol, host, path) = url_parts(&raw_base);
    let mut url = json!({ "raw": raw, "host": [host], "path": path });
    if let Some(p) = protocol {
        url["protocol"] = json!(p);
    }
    if !query.is_empty() {
        url["query"] = Value::Array(query);
    }
    let params = crate::vars::path_params(&contract.url);
    if !params.is_empty() {
        url["variable"] = Value::Array(
            params
                .iter()
                .map(|p| json!({ "key": p, "value": "" }))
                .collect(),
        );
    }

    let mut req = json!({
        "method": method_str(&contract.method),
        "header": headers(&contract.headers, used),
        "url": url,
    });
    if let Some(d) = &contract.description {
        req["description"] = json!(d);
    }
    if let Some(parts) = contract.multipart.as_ref().filter(|p| !p.is_empty()) {
        req["body"] = json!({ "mode": "formdata", "formdata": form_data(parts, used) });
    } else if let Some(body) = &contract.request {
        req["body"] = json!({
            "mode": "raw",
            "raw": to_postman(&pretty(body), false, used),
            "options": { "raw": { "language": "json" } },
        });
    }
    req
}

fn headers(list: &[Header], used: &mut BTreeSet<String>) -> Vec<Value> {
    list.iter()
        .map(|h| {
            json!({
                "key": to_postman(&h.name, false, used),
                "value": to_postman(&h.value, false, used),
            })
        })
        .collect()
}

/// Multipart parts as Postman `formdata`. A file part's value is the file
/// path, which Postman keeps as `src`.
fn form_data(parts: &[Part], used: &mut BTreeSet<String>) -> Vec<Value> {
    parts
        .iter()
        .map(|p| {
            let mut entry = json!({ "key": p.name });
            if p.filename.is_some() {
                entry["type"] = json!("file");
                entry["src"] = json!(to_postman(&p.value, false, used));
            } else {
                entry["type"] = json!("text");
                entry["value"] = json!(to_postman(&p.value, false, used));
            }
            if let Some(ct) = &p.content_type {
                entry["contentType"] = json!(ct);
            }
            if let Some(d) = &p.description {
                entry["description"] = json!(d);
            }
            entry
        })
        .collect()
}

/// One Postman request item, with each documented response as a saved
/// example.
fn item(contract: &JsonContent, used: &mut BTreeSet<String>) -> Value {
    let req = request(contract, used);
    let responses: Vec<Value> = contract
        .responses
        .iter()
        .map(|r| {
            let name = if r.description.is_empty() {
                r.code.to_string()
            } else {
                r.description.clone()
            };
            let mut resp = json!({
                "name": name,
                "originalRequest": req,
                "code": r.code,
                "status": r.description,
                "header": headers(&r.headers, used),
            });
            if let Some(body) = &r.schema {
                resp["body"] = json!(to_postman(&pretty(body), false, used));
                resp["_postman_previewlanguage"] = json!("json");
            }
            resp
        })
        .collect();
    json!({
        "name": contract.name,
        "request": req,
        "response": responses,
    })
}

/// Pretty JSON text for a body.
fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

fn str_of(value: &Value) -> &str {
    value.as_str().unwrap_or_default()
}

/// Inserts `entry` into the folder tree at `dirs`, creating Postman folders
/// along the way. Folders keep the order they are first seen in.
fn insert(items: &mut Vec<Value>, dirs: &[String], entry: Value) {
    let Some((first, rest)) = dirs.split_first() else {
        items.push(entry);
        return;
    };
    let pos = items
        .iter()
        .position(|i| i.get("item").is_some() && i["name"] == first.as_str());
    let pos = match pos {
        Some(pos) => pos,
        None => {
            items.push(json!({ "name": first, "item": [] }));
            items.len() - 1
        }
    };
    if let Some(Value::Array(children)) = items[pos].get_mut("item") {
        insert(children, rest, entry);
    }
}

/// Builds a Postman Collection v2.1 from `items`.
///
/// # Arguments
///
/// * `name` - the collection name
/// * `items` - the contracts, in the order they should appear
/// * `values` - values for the collection variables, taken from a shared
///   env file. A variable with no value here is listed with an empty value.
///
/// # Returns
///
/// The collection document.
pub fn postman_collection(name: &str, items: &[ExportItem], values: &Vars) -> Value {
    let mut used = BTreeSet::new();
    let mut root: Vec<Value> = Vec::new();
    for export in items {
        let dirs: Vec<String> = export
            .rel_path
            .parent()
            .unwrap_or(Path::new(""))
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        insert(&mut root, &dirs, item(&export.contract, &mut used));
    }
    let variables: Vec<Value> = used
        .iter()
        .map(|key| {
            let mut v = Map::new();
            v.insert("key".into(), json!(key));
            v.insert(
                "value".into(),
                json!(values.get(key).cloned().unwrap_or_default()),
            );
            Value::Object(v)
        })
        .collect();
    json!({
        "info": { "name": name, "schema": POSTMAN_SCHEMA },
        "item": root,
        "variable": variables,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::converter::{PostmanCollection, from_slice};

    fn contract(json: Value) -> JsonContent {
        serde_json::from_value(json).expect("contract parses")
    }

    fn export(items: Vec<(&str, Value)>, values: &Vars) -> Value {
        let items: Vec<ExportItem> = items
            .into_iter()
            .map(|(p, c)| ExportItem {
                rel_path: PathBuf::from(p),
                contract: contract(c),
            })
            .collect();
        postman_collection("Demo", &items, values)
    }

    #[test]
    fn rewrites_variables_and_path_params() {
        let mut used = BTreeSet::new();
        assert_eq!(
            to_postman("$base/users/{id}?t=${tok}x&p=$$5", true, &mut used),
            "{{base}}/users/:id?t={{tok}}x&p=$5"
        );
        assert_eq!(
            to_postman("{{ key }} {id}", false, &mut used),
            "{{key}} {id}"
        );
        let names: Vec<&str> = used.iter().map(String::as_str).collect();
        assert_eq!(names, ["base", "key", "tok"]);
    }

    #[test]
    fn maps_a_contract_with_folders_query_body_and_responses() {
        let mut values = Vars::new();
        values.insert("base".into(), "https://api.example.com".into());
        let out = export(
            vec![(
                "users/update.json",
                json!({
                    "name": "Update user", "description": "Change a user", "method": "PUT",
                    "url": "$base/users/{id}",
                    "query": [{"name": "notify", "value": "true", "description": "Send mail"}],
                    "headers": [{"name": "Authorization", "value": "Bearer $token", "required": true}],
                    "request": {"name": "$name"},
                    "responses": [{"code": 200, "description": "OK", "schema": {"ok": true}}]
                }),
            )],
            &values,
        );
        let folder = &out["item"][0];
        assert_eq!(folder["name"], "users");
        let it = &folder["item"][0];
        assert_eq!(it["name"], "Update user");
        let req = &it["request"];
        assert_eq!(req["method"], "PUT");
        assert_eq!(req["url"]["raw"], "{{base}}/users/:id?notify=true");
        assert_eq!(req["url"]["host"], json!(["{{base}}"]));
        assert_eq!(req["url"]["path"], json!(["users", ":id"]));
        assert_eq!(req["url"]["variable"][0]["key"], "id");
        assert_eq!(req["header"][0]["value"], "Bearer {{token}}");
        assert_eq!(req["body"]["mode"], "raw");
        assert!(str_of(&req["body"]["raw"]).contains("\"{{name}}\""));
        assert_eq!(it["response"][0]["code"], 200);
        assert_eq!(it["response"][0]["status"], "OK");
        let vars: Vec<(&str, &str)> = out["variable"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| (str_of(&v["key"]), str_of(&v["value"])))
            .collect();
        assert_eq!(
            vars,
            [
                ("base", "https://api.example.com"),
                ("name", ""),
                ("token", "")
            ]
        );
    }

    #[test]
    fn multipart_becomes_formdata() {
        let out = export(
            vec![(
                "upload.json",
                json!({
                    "name": "Upload", "method": "POST", "url": "https://h/upload", "headers": [],
                    "multipart": [
                        {"name": "title", "value": "Hi"},
                        {"name": "file", "value": "files/a.png", "filename": "a.png", "contentType": "image/png"}
                    ],
                    "responses": []
                }),
            )],
            &Vars::new(),
        );
        let form = &out["item"][0]["request"]["body"]["formdata"];
        assert_eq!(
            form[0],
            json!({"key": "title", "type": "text", "value": "Hi"})
        );
        assert_eq!(form[1]["type"], "file");
        assert_eq!(form[1]["src"], "files/a.png");
        assert_eq!(out["item"][0]["request"]["url"]["protocol"], "https");
    }

    #[test]
    fn export_reimports_through_the_postman_converter() {
        let out = export(
            vec![
                (
                    "a/b/list.json",
                    json!({"name": "List", "method": "GET", "url": "https://h/x", "headers": [], "responses": []}),
                ),
                (
                    "a/get.json",
                    json!({"name": "Get", "method": "GET", "url": "https://h/x/{id}", "headers": [], "responses": []}),
                ),
            ],
            &Vars::new(),
        );
        // Both contracts share folder `a`, and `b` nests inside it.
        assert_eq!(out["item"].as_array().unwrap().len(), 1);
        assert_eq!(out["item"][0]["item"][0]["name"], "b");
        let bytes = serde_json::to_vec(&out).unwrap();
        assert!(matches!(
            from_slice(&bytes).expect("export parses as Postman"),
            PostmanCollection::V2_1_0(_)
        ));
    }
}
