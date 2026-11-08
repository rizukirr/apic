//! Reading, creating and deleting contract and template files. Every create
//! and delete is confined to its directory with `apic_core::file::confine_to_dir`,
//! and nothing existing is ever overwritten.

use std::path::{Path, PathBuf};

use apic_core::edit::EditModel;
use apic_core::json::method_str;

use crate::features::contracts::domain::entry::{DeleteTarget, Entry};
use crate::features::contracts::domain::names::RequestTarget;

/// Discovers the contracts under `contracts_root`, sorted by path, skipping
/// anything inside `.apic`, with each one's method for the sidebar badge and
/// its validation error when it is invalid.
pub(crate) fn scan(contracts_root: &Path) -> Vec<Entry> {
    let failures = apic_core::validate_dir(contracts_root);
    let mut paths = apic_core::json::scan_json_file(contracts_root, true).unwrap_or_default();
    paths.sort();
    paths
        .into_iter()
        .filter(|p| !p.components().any(|c| c.as_os_str() == ".apic"))
        .map(|path| {
            let rel = apic_core::file::relative_slash(&path, contracts_root);
            let method = apic_core::file::read_file(&path)
                .ok()
                .and_then(|t| apic_core::json::json_get(&t, None).ok())
                .map(|c| method_str(&c.method))
                .unwrap_or_else(|| "?".to_string());
            let error = failures
                .iter()
                .find(|(p, _)| *p == path)
                .map(|(_, e)| e.clone());
            Entry {
                path,
                rel,
                method,
                error,
            }
        })
        .collect()
}

/// The project templates in `apic_dir`'s template folder, as (display name,
/// path) pairs. The display name is the file stem.
pub(crate) fn templates(apic_dir: &Path) -> Vec<(String, PathBuf)> {
    apic_core::template::list_templates(apic_dir)
        .into_iter()
        .map(|p| {
            let name = p
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            (name, p)
        })
        .collect()
}

/// The raw text of the file at `path`, for the repair editor.
///
/// # Errors
///
/// Returns the read error as a string.
pub(crate) fn read_text(path: &Path) -> Result<String, String> {
    apic_core::file::read_file(path).map_err(|e| e.to_string())
}

/// Reads and parses the contract at `path` into an editable model.
///
/// # Errors
///
/// Returns the read or parse error as a string.
pub(crate) fn load_contract(path: &Path) -> Result<EditModel, String> {
    apic_core::file::read_file(path)
        .map_err(|e| e.to_string())
        .and_then(|t| apic_core::json::load_for_edit(&t))
        .map(EditModel::from_contract)
}

/// Resolves the template at `path` against the built-in default into a full
/// contract and returns it as an editable model.
///
/// # Errors
///
/// Returns the resolve or parse error as a string.
pub(crate) fn load_template(path: &Path) -> Result<EditModel, String> {
    apic_core::template::resolve_contract_from(path)
        .and_then(|(c, _w)| apic_core::json::json_get(&c, None).map_err(|e| e.to_string()))
        .map(EditModel::from_contract)
}

/// Creates the folder or contract `target` names under `root`. A contract is
/// seeded from `template` (merged onto the built-in default) or the built-in
/// default itself when `None`, and its intermediate folders are created.
///
/// # Returns
///
/// The created path.
///
/// # Errors
///
/// Returns the `confine_to_dir` error when the path escapes `root`,
/// `"{rel} already exists, not overwriting"` when the contract exists,
/// `"template error: {e}"`, `"create dir error: {e}"` or `"write error: {e}"`.
pub(crate) fn create_request(
    root: &Path,
    target: &RequestTarget,
    template: Option<&Path>,
) -> Result<PathBuf, String> {
    let rel = &target.rel;
    let dest = apic_core::file::confine_to_dir(root, Path::new(rel))?;
    if target.is_folder {
        std::fs::create_dir_all(&dest).map_err(|e| format!("create dir error: {e}"))?;
        return Ok(dest);
    }
    if dest.exists() {
        return Err(format!("{rel} already exists, not overwriting"));
    }
    let contract = match template {
        Some(path) => match apic_core::template::resolve_contract_from(path) {
            Ok((c, _warnings)) => c,
            Err(e) => return Err(format!("template error: {e}")),
        },
        None => apic_core::template::DEFAULT.to_string(),
    };
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create dir error: {e}"))?;
    }
    std::fs::write(&dest, contract).map_err(|e| format!("write error: {e}"))?;
    Ok(dest)
}

/// Creates the template `file_name` in `apic_dir`'s template folder, seeded
/// from the built-in default. `name` is what the user typed, for the error.
///
/// # Returns
///
/// The created path.
///
/// # Errors
///
/// Returns the `confine_to_dir` error when the name escapes the template
/// folder, `"template '{name}' already exists"`, `"create dir error: {e}"` or
/// `"write error: {e}"`.
pub(crate) fn create_template(
    apic_dir: &Path,
    file_name: &str,
    name: &str,
) -> Result<PathBuf, String> {
    let dir = apic_core::template::dir(apic_dir);
    let dest = apic_core::file::confine_to_dir(&dir, Path::new(file_name))?;
    if dest.exists() {
        return Err(format!("template '{name}' already exists"));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("create dir error: {e}"))?;
    std::fs::write(&dest, apic_core::template::DEFAULT).map_err(|e| format!("write error: {e}"))?;
    Ok(dest)
}

/// Removes `target`, confined to the contracts `root` or to the template
/// folder of `apic_dir`.
///
/// # Returns
///
/// The removed path and the label the status line names it by.
///
/// # Errors
///
/// Returns `"no project"` when the needed directory is `None`, the
/// `confine_to_dir` error when the path escapes it, or `"delete error: {e}"`.
pub(crate) fn delete(
    target: &DeleteTarget,
    root: Option<&Path>,
    apic_dir: Option<&Path>,
) -> Result<(PathBuf, String), String> {
    match target {
        DeleteTarget::Contract { rel, is_dir } => {
            let root = root.ok_or_else(|| "no project".to_string())?;
            let dest = apic_core::file::confine_to_dir(root, Path::new(rel))?;
            let r = if *is_dir {
                std::fs::remove_dir_all(&dest)
            } else {
                std::fs::remove_file(&dest)
            };
            r.map_err(|e| format!("delete error: {e}"))?;
            Ok((dest, rel.clone()))
        }
        DeleteTarget::Template { name, path } => {
            // Confine to the template dir so only a real template is removed.
            let apic_dir = apic_dir.ok_or_else(|| "no project".to_string())?;
            let dir = apic_core::template::dir(apic_dir);
            let filename = path.file_name().map(Path::new).unwrap_or(Path::new(""));
            let dest = apic_core::file::confine_to_dir(&dir, filename)?;
            std::fs::remove_file(&dest).map_err(|e| format!("delete error: {e}"))?;
            Ok((dest, format!("template {name}")))
        }
    }
}

/// Writes `text` to `path`.
///
/// # Errors
///
/// Returns the write error as a string.
pub(crate) fn write(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_support::tempdir;

    #[test]
    fn scan_lists_contracts_with_method_and_error() {
        let root = tempdir();
        std::fs::write(root.join("good.json"), apic_core::template::DEFAULT).unwrap();
        std::fs::write(root.join("bad.json"), "{ not json").unwrap();
        let entries = scan(&root);
        assert_eq!(entries.len(), 2);
        let good = entries.iter().find(|e| e.rel == "good.json").unwrap();
        assert_ne!(good.method, "?");
        assert!(good.error.is_none());
        let bad = entries.iter().find(|e| e.rel == "bad.json").unwrap();
        assert!(bad.error.is_some());
    }

    #[test]
    fn create_request_writes_and_refuses_overwrite() {
        let root = tempdir();
        let target = RequestTarget {
            rel: "auth/login.json".into(),
            is_folder: false,
        };
        let dest = create_request(&root, &target, None).unwrap();
        assert_eq!(
            std::fs::read_to_string(&dest).unwrap(),
            apic_core::template::DEFAULT
        );
        let again = create_request(&root, &target, None).unwrap_err();
        assert_eq!(again, "auth/login.json already exists, not overwriting");

        let folder = RequestTarget {
            rel: "profile".into(),
            is_folder: true,
        };
        assert!(create_request(&root, &folder, None).unwrap().is_dir());
    }

    #[test]
    fn create_template_writes_and_refuses_overwrite() {
        let apic_dir = tempdir().join(".apic");
        let dest = create_template(&apic_dir, "base.json", "base").unwrap();
        assert!(dest.is_file());
        let again = create_template(&apic_dir, "base.json", "base").unwrap_err();
        assert_eq!(again, "template 'base' already exists");
    }

    #[test]
    fn delete_removes_a_contract_and_a_template() {
        let root = tempdir();
        let apic_dir = root.join(".apic");
        let target = RequestTarget {
            rel: "login.json".into(),
            is_folder: false,
        };
        let contract = create_request(&root, &target, None).unwrap();
        let template = create_template(&apic_dir, "base.json", "base").unwrap();

        let (removed, label) = delete(
            &DeleteTarget::Contract {
                rel: "login.json".into(),
                is_dir: false,
            },
            Some(&root),
            Some(&apic_dir),
        )
        .unwrap();
        assert_eq!(label, "login.json");
        assert!(!removed.exists() && !contract.exists());

        let (removed, label) = delete(
            &DeleteTarget::Template {
                name: "base".into(),
                path: template.clone(),
            },
            Some(&root),
            Some(&apic_dir),
        )
        .unwrap();
        assert_eq!(label, "template base");
        assert!(!removed.exists() && !template.exists());

        let missing = delete(
            &DeleteTarget::Contract {
                rel: "login.json".into(),
                is_dir: false,
            },
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(missing, "no project");
    }
}
