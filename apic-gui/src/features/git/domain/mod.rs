//! Git feature business logic as plain data and pure functions: working-tree
//! status, field-level contract diffs, and merge conflict markers.
//!
//! Nothing here spawns a process, touches the filesystem, starts a thread or
//! imports egui, so every function is testable from captured text alone.

pub(crate) mod conflict;
pub(crate) mod diff;
pub(crate) mod status;
