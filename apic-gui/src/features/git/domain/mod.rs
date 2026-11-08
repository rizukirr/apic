//! Git feature business logic as plain data and pure functions: working-tree
//! status, field-level contract diffs, merge conflict markers and the commit log.
//!
//! Nothing here spawns a process, touches the filesystem, starts a thread or
//! imports egui, so every function is testable from captured text alone.

pub(crate) mod conflict;
pub(crate) mod diff;
pub(crate) mod log;
pub(crate) mod status;
