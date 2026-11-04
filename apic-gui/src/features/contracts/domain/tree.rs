//! The sidebar's contract folder tree, built from relative paths and a filter.

use std::collections::BTreeMap;

use crate::features::contracts::domain::entry::Entry;

/// The contract tree for `entries`, keeping only those whose relative path
/// contains `query` (case-insensitive; an empty query keeps every entry).
pub(crate) fn build_tree(entries: &[Entry], query: &str) -> TreeNode {
    let q = query.to_lowercase();
    let mut tree = TreeNode::default();
    for (i, e) in entries.iter().enumerate() {
        if q.is_empty() || e.rel.to_lowercase().contains(&q) {
            tree.insert(&e.rel, i, &e.method, e.error.is_some());
        }
    }
    tree
}

/// A folder tree of contracts built from their `/`-separated relative paths.
/// Leaves carry the index into `App::entries` and the method for the badge.
#[derive(Default)]
pub(crate) struct TreeNode {
    /// Child folders by name, in sorted order.
    pub(crate) dirs: BTreeMap<String, TreeNode>,

    /// Contracts directly in this folder.
    pub(crate) files: Vec<(String, usize, String, bool)>, // (leaf label, entry index, method, invalid)
}

impl TreeNode {
    /// Inserts the contract at `rel` (relative to this node), creating any
    /// intermediate folders.
    pub(crate) fn insert(&mut self, rel: &str, idx: usize, method: &str, invalid: bool) {
        match rel.split_once('/') {
            Some((dir, rest)) => self
                .dirs
                .entry(dir.to_string())
                .or_default()
                .insert(rest, idx, method, invalid),
            None => self
                .files
                .push((rel.to_string(), idx, method.to_string(), invalid)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(rel: &str) -> Entry {
        Entry {
            path: std::path::PathBuf::from(rel),
            rel: rel.to_string(),
            method: "GET".to_string(),
            error: None,
        }
    }

    #[test]
    fn build_tree_keeps_only_matching_contracts() {
        let entries = [entry("auth/login.json"), entry("profile/get.json")];
        let tree = build_tree(&entries, "LOGIN");
        assert!(tree.dirs.contains_key("auth"));
        assert!(!tree.dirs.contains_key("profile"));
        let all = build_tree(&entries, "");
        assert!(all.dirs.contains_key("auth") && all.dirs.contains_key("profile"));
    }
}
