//! `apic convert` — import a Postman collection as apic contract files.

use std::collections::HashSet;

/// Convert a human request/folder name into a filesystem-safe slug using
/// underscores: lowercase, runs of non-alphanumeric characters collapse to a
/// single `_`, leading/trailing `_` trimmed. Empty input yields `"unnamed"`.
fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut prev_sep = true; // true so a leading separator run is dropped
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.extend(ch.to_lowercase());
            prev_sep = false;
        } else if !prev_sep {
            out.push('_');
            prev_sep = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        "unnamed".to_string()
    } else {
        out
    }
}

/// Reserve a unique slug within one directory. If `base` is taken, append
/// `_2`, `_3`, … until free. Records the chosen slug in `taken`.
fn unique_slug(taken: &mut HashSet<String>, base: &str) -> String {
    if taken.insert(base.to_string()) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}_{n}");
        if taken.insert(candidate.clone()) {
            return candidate;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_basic() {
        assert_eq!(slugify("Get User By ID"), "get_user_by_id");
        assert_eq!(slugify("create/login!!"), "create_login");
        assert_eq!(slugify("  spaced  "), "spaced");
        assert_eq!(slugify(""), "unnamed");
        assert_eq!(slugify("***"), "unnamed");
    }

    #[test]
    fn slug_unique_appends_suffix() {
        let mut taken = HashSet::new();
        assert_eq!(unique_slug(&mut taken, "user"), "user");
        assert_eq!(unique_slug(&mut taken, "user"), "user_2");
        assert_eq!(unique_slug(&mut taken, "user"), "user_3");
        assert_eq!(unique_slug(&mut taken, "auth"), "auth");
    }
}
