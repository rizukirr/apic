//! The git feature: working-tree status, diffs, staging, and commits for the
//! repository containing the active project.
//!
//! `model` is plain data, `service` is every `git` invocation, `diff` compares
//! two contracts, `state` is what the panel remembers, `view` renders it. No
//! module here imports `crate::features::contracts`.

pub(crate) mod conflict;
pub(crate) mod diff;
pub(crate) mod model;
pub(crate) mod service;
pub(crate) mod state;
pub(crate) mod view;
