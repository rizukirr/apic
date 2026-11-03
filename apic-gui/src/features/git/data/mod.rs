//! Git feature data access: every `git` invocation, and the background jobs
//! that run them off the UI thread.
//!
//! Nothing here imports egui. A job wakes its caller through a plain
//! `Fn() + Send` callback instead.

pub(crate) mod cli;
pub(crate) mod jobs;
