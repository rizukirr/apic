//! Interactive terminal UI for creating and editing contracts.
//!
//! The default authoring surface for `apic create` and `apic open`. The
//! external-editor path remains available behind `--editor`.

// The TUI is built incrementally across tasks; many items are defined before
// the task that wires them up. This module-wide allow keeps `clippy -D warnings`
// green during construction. Task 18 removes it and confirms nothing is left
// genuinely dead.
#![allow(dead_code)]

mod draw;
mod model;
mod rows;
mod seed;
mod state;

use std::path::Path;

/// Placeholder entry point; real implementation added in the event-loop task.
pub(crate) fn run(_path: &Path) -> Result<(), String> {
    Ok(())
}
