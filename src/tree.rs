//! Tree-style rendering of contract paths for `apic list`.
//!
//! Relative paths are inserted component-by-component into a [`Node`] and
//! rendered depth-first with box-drawing characters. Directories sort before
//! files, both alphabetically (the natural `BTreeMap` order). Fuzzy-match
//! char positions ride along on each component so matched characters can be
//! highlighted when output is a terminal.

use crossterm::style::Stylize;
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
    /// string, as produced by [`apic_core::fuzzy::fuzzy_match`] — to the component
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

/// Renders the tree to a string.
///
/// With `root_label` the label prints as the first line and every entry nests
/// under it with branch characters. Without it, top-level entries print
/// flush-left and branches start one level down. With `color`, matched
/// characters are highlighted cyan + bold.
pub(crate) fn render(root_label: Option<&str>, root: &Node, color: bool) -> String {
    let mut out = String::new();
    match root_label {
        Some(label) => {
            out.push_str(label);
            out.push('\n');
            render_into(root, "", color, &mut out);
        }
        None => {
            for (name, child) in ordered(root) {
                out.push_str(&styled(name, &child.matches, color));
                if !child.is_file {
                    out.push('/');
                }
                out.push('\n');
                render_into(child, "", color, &mut out);
            }
        }
    }
    out
}

/// A node's children with directories first, then files; `BTreeMap` iteration
/// keeps each group alphabetical.
fn ordered(node: &Node) -> Vec<(&String, &Node)> {
    node.children
        .iter()
        .filter(|(_, n)| !n.is_file)
        .chain(node.children.iter().filter(|(_, n)| n.is_file))
        .collect()
}

/// Appends `node`'s children to `out`, one line each, prefixed by `prefix`
/// plus a branch glyph; recurses with the continued prefix.
fn render_into(node: &Node, prefix: &str, color: bool, out: &mut String) {
    let entries = ordered(node);
    let last = entries.len().saturating_sub(1);
    for (i, (name, child)) in entries.iter().enumerate() {
        let (branch, cont) = if i == last {
            ("└── ", "    ")
        } else {
            ("├── ", "│   ")
        };
        out.push_str(prefix);
        out.push_str(branch);
        out.push_str(&styled(name, &child.matches, color));
        if !child.is_file {
            out.push('/');
        }
        out.push('\n');
        render_into(child, &format!("{prefix}{cont}"), color, out);
    }
}

/// Returns `name` with matched char runs highlighted (cyan + bold) when
/// `color` is set; the name unchanged otherwise.
fn styled(name: &str, matches: &BTreeSet<usize>, color: bool) -> String {
    if !color || matches.is_empty() {
        return name.to_string();
    }
    let mut out = String::new();
    let mut run = String::new();
    let mut run_matched = false;
    for (i, c) in name.chars().enumerate() {
        let matched = matches.contains(&i);
        if matched != run_matched && !run.is_empty() {
            push_run(&mut out, &run, run_matched);
            run.clear();
        }
        run_matched = matched;
        run.push(c);
    }
    push_run(&mut out, &run, run_matched);
    out
}

/// Appends `run` to `out`, styling it when it was a matched run.
fn push_run(out: &mut String, run: &str, matched: bool) {
    if run.is_empty() {
        return;
    }
    if matched {
        out.push_str(&run.cyan().bold().to_string());
    } else {
        out.push_str(run);
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

    fn build(paths: &[&str]) -> Node {
        let mut root = Node::default();
        for p in paths {
            root.insert(Path::new(p), &[]);
        }
        root
    }

    #[test]
    fn renders_tree_with_branch_chars_dirs_first() {
        let tree = build(&[
            "user/user.json",
            "user/upload.json",
            "user/profile/user.json",
            "auth/user.json",
            "auth/login.json",
        ]);
        let expected = "\
auth/
├── login.json
└── user.json
user/
├── profile/
│   └── user.json
├── upload.json
└── user.json
";
        assert_eq!(render(None, &tree, false), expected);
    }

    #[test]
    fn renders_root_label_with_tree_nested_under_it() {
        let tree = build(&["auth/login.json", "auth/user.json", "user/user.json"]);
        let expected = "\
/abs/contracts/
├── auth/
│   ├── login.json
│   └── user.json
└── user/
    └── user.json
";
        assert_eq!(render(Some("/abs/contracts/"), &tree, false), expected);
    }

    #[test]
    fn highlights_matched_chars_when_colored() {
        use crossterm::style::Stylize;

        // Simulates `--filter "us/up"` on `user/upload.json`: `us` matched in
        // the dir component, `up` in the file component.
        let mut tree = Node::default();
        tree.insert(Path::new("user/upload.json"), &[0, 1, 5, 6]);

        let out = render(None, &tree, true);
        let us = "us".cyan().bold().to_string();
        let up = "up".cyan().bold().to_string();
        assert!(
            out.contains(&format!("{us}er/")),
            "dir highlight missing: {out:?}"
        );
        assert!(
            out.contains(&format!("└── {up}load.json")),
            "file highlight missing: {out:?}"
        );
    }

    #[test]
    fn plain_render_has_no_ansi_escapes() {
        let mut tree = Node::default();
        tree.insert(Path::new("user/upload.json"), &[0, 1]);
        let out = render(None, &tree, false);
        assert!(!out.contains('\u{1b}'), "ANSI escape leaked: {out:?}");
    }
}
