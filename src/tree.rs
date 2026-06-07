//! Tree-style rendering of contract paths for `apic list`.
//!
//! Relative paths are inserted component-by-component into a [`Node`] and
//! rendered depth-first with box-drawing characters. Directories sort before
//! files, both alphabetically (the natural `BTreeMap` order). Fuzzy-match
//! char positions ride along on each component so matched characters can be
//! highlighted when output is a terminal.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// One directory or file in the rendered tree.
#[derive(Default)]
pub(crate) struct Node {
    children: BTreeMap<String, Node>,
    /// Set on the last component of an inserted path.
    is_file: bool,
    /// Component-local char positions to highlight. For a directory this is
    /// the union across every matched descendant that was inserted through it.
    matches: BTreeSet<usize>,
}

impl Node {
    /// Inserts `rel` (a path relative to the contracts root), attributing each
    /// of `match_indices` — char positions into the separator-joined display
    /// string, as produced by [`crate::fuzzy::fuzzy_match`] — to the component
    /// whose char range contains it. Separator positions match no component
    /// and are dropped.
    pub(crate) fn insert(&mut self, rel: &Path, match_indices: &[usize]) {
        let components: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();

        let mut node = self;
        let mut offset = 0; // char offset of the current component
        let last = components.len().saturating_sub(1);
        for (i, name) in components.iter().enumerate() {
            let len = name.chars().count();
            let local = match_indices
                .iter()
                .filter(|&&ix| ix >= offset && ix < offset + len)
                .map(|&ix| ix - offset);
            node = node.children.entry(name.clone()).or_default();
            node.matches.extend(local);
            if i == last {
                node.is_file = true;
            }
            offset += len + 1; // +1 for the path separator
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_builds_nested_nodes_and_flags_files() {
        let mut root = Node::default();
        root.insert(Path::new("user/profile/user.json"), &[]);

        let user = &root.children["user"];
        assert!(!user.is_file);
        let profile = &user.children["profile"];
        assert!(!profile.is_file);
        let file = &profile.children["user.json"];
        assert!(file.is_file);
        assert!(file.children.is_empty());
    }

    #[test]
    fn insert_maps_match_indices_to_components() {
        // "user/profile/user.json": chars 0..4 are `user`, 5..12 `profile`,
        // 13..22 `user.json`; positions 4 and 12 are separators.
        let mut root = Node::default();
        root.insert(Path::new("user/profile/user.json"), &[0, 1, 4, 5, 13, 14]);

        let user = &root.children["user"];
        assert_eq!(user.matches, BTreeSet::from([0, 1]));
        let profile = &user.children["profile"];
        assert_eq!(profile.matches, BTreeSet::from([0]));
        let file = &profile.children["user.json"];
        assert_eq!(file.matches, BTreeSet::from([0, 1]));
    }

    #[test]
    fn dir_matches_union_across_descendants() {
        let mut root = Node::default();
        // `user` matched at char 0 via one file and char 3 via another.
        root.insert(Path::new("user/login.json"), &[0]);
        root.insert(Path::new("user/upload.json"), &[3]);

        assert_eq!(root.children["user"].matches, BTreeSet::from([0, 3]));
    }
}
