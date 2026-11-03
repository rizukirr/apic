//! The git feature: working-tree status, diffs, staging, and commits for the
//! repository containing the active project.
//!
//! `domain` is status, field-level diffs and conflict markers as pure logic,
//! `service` is every `git` invocation, `state` is what the panel remembers, `view` renders it. No
//! module here imports `crate::features::contracts`.

pub(crate) mod domain;
pub(crate) mod service;
pub(crate) mod state;
pub(crate) mod view;
